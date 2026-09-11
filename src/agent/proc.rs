//! Extended notes: `docs/internals/agent/proc.md`

// Allowlist placement: the funnel section of `effects/allowlist.toml`, which
// carries this module's review clause. `effect_site_inventory.mechanism` (2).

#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::disallowed_macros
)]

use std::ops::{Deref, DerefMut};
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};

use crate::topology::effects::ProcessSite;

use crate::error::{ProcessFate, UpstrokeError};
use crate::topology::effects::SubEffectPoint;

mod hooks;
#[cfg(unix)]
use self::hooks::apply;
#[cfg(windows)]
use self::hooks::apply_io;
pub use self::hooks::{NoHooks, SpawnHooks};

#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn memoised_outcome<T>(memo: &Result<T, String>) -> Result<(), String> {
    match memo {
        Ok(_) => Ok(()),
        Err(message) => Err(message.clone()),
    }
}

#[derive(Debug, Clone)]
pub struct ProcessOutput {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration: Duration,
    pub timed_out: bool,
    pub output_limited: bool,
}

const DRAIN_GRACE_EXIT: Duration = Duration::from_secs(2);
const DRAIN_GRACE_KILL: Duration = Duration::from_millis(500);
const OUTPUT_LIMIT_BYTES: usize = 16 * 1024 * 1024;

struct ProcessTree {
    child: Child,
    #[cfg(windows)]
    job: windows_job::Job,
}

#[derive(Debug)]
struct SpawnFailure {
    error: std::io::Error,
    fate: ProcessFate,
}

impl std::fmt::Display for SpawnFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.error, self.fate.describe())
    }
}

impl ProcessTree {
    fn spawn(command: &mut Command, hooks: &mut dyn SpawnHooks) -> Result<Self, SpawnFailure> {
        #[cfg(windows)]
        {
            let (child, job) = windows_job::spawn_suspended_in_job(command, hooks)?;
            Ok(Self { child, job })
        }
        #[cfg(not(windows))]
        {
            let child = command.spawn().map_err(|error| SpawnFailure {
                error,
                fate: ProcessFate::NeverStarted,
            })?;
            hooks.child_created(child.id());
            Ok(Self { child })
        }
    }

    #[cfg(windows)]
    fn finish_direct_exit(&mut self) -> Result<(), UpstrokeError> {
        self.job
            .terminate_and_wait()
            .map_err(|error| UpstrokeError::Agent {
                message: format!("settling the Windows agent job after direct-child exit: {error}"),
            })?;
        Ok(())
    }
}

impl Deref for ProcessTree {
    type Target = Child;

    fn deref(&self) -> &Self::Target {
        &self.child
    }
}

impl DerefMut for ProcessTree {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.child
    }
}

pub fn run_with_timeout_at(
    spawn_site: ProcessSite,
    terminate_site: ProcessSite,
    command: Command,
    stdin_data: &[u8],
    timeout: Duration,
    hooks: &mut dyn SpawnHooks,
) -> Result<ProcessOutput, UpstrokeError> {
    run_with_timeout_classified(
        spawn_site,
        terminate_site,
        command,
        stdin_data,
        timeout,
        hooks,
    )
    .map_err(|failure| failure.error)
}

#[derive(Debug)]
pub struct ProcessFailure {
    pub fate: ProcessFate,
    pub error: UpstrokeError,
}

pub fn run_with_timeout_classified(
    spawn_site: ProcessSite,
    terminate_site: ProcessSite,
    command: Command,
    stdin_data: &[u8],
    timeout: Duration,
    hooks: &mut dyn SpawnHooks,
) -> Result<ProcessOutput, ProcessFailure> {
    validate_process_sites(spawn_site, terminate_site).map_err(|error| ProcessFailure {
        fate: ProcessFate::NeverStarted,
        error,
    })?;
    run_with_timeout_and_limit(
        spawn_site,
        terminate_site,
        command,
        stdin_data,
        timeout,
        OUTPUT_LIMIT_BYTES,
        hooks,
    )
}

fn validate_process_sites(
    spawn_site: ProcessSite,
    terminate_site: ProcessSite,
) -> Result<(), UpstrokeError> {
    match (spawn_site, terminate_site) {
        (ProcessSite::Spawn, ProcessSite::Terminate) => Ok(()),
        _ => Err(UpstrokeError::Agent {
            message: format!(
                "process funnel requires (Process.Spawn, Process.Terminate), got ({}, {})",
                spawn_site.name(),
                terminate_site.name()
            ),
        }),
    }
}

fn run_with_timeout_and_limit(
    spawn_site: ProcessSite,
    terminate_site: ProcessSite,
    mut command: Command,
    stdin_data: &[u8],
    timeout: Duration,
    output_limit: usize,
    hooks: &mut dyn SpawnHooks,
) -> Result<ProcessOutput, ProcessFailure> {
    let fate = std::cell::Cell::new(ProcessFate::NeverStarted);
    validate_process_sites(spawn_site, terminate_site).map_err(|error| ProcessFailure {
        fate: fate.get(),
        error,
    })?;
    let mut reports = [None, None, None];
    let [input_report, stdout_report, stderr_report] = &mut reports;
    let outcome = (|| {
        let prepared =
            pipe_io::Prepared::configure(&mut command).map_err(|error| UpstrokeError::Agent {
                message: format!("preparing agent pipes: {error}"),
            })?;

        #[cfg(unix)]
        let mut termination = termination::Supervisor::begin(terminate_site)?;
        #[cfg(unix)]
        apply(
            hooks.point(SubEffectPoint::ReaperStarted),
            SubEffectPoint::ReaperStarted,
        )?;
        #[cfg(unix)]
        termination.prepare(&mut command);

        let started = Instant::now();
        let mut child = ProcessTree::spawn(&mut command, hooks).map_err(|failure| {
            fate.set(failure.fate);
            UpstrokeError::Agent {
                message: format!(
                    "failed to spawn `{}`: {}",
                    command.get_program().to_string_lossy(),
                    failure.error
                ),
            }
        })?;
        fate.set(ProcessFate::Unresolved);
        drop(command);
        #[cfg(unix)]
        apply(
            hooks.point(SubEffectPoint::PreExecPgidAndRegister),
            SubEffectPoint::PreExecPgidAndRegister,
        )?;
        #[cfg(unix)]
        apply(hooks.point(SubEffectPoint::Exec), SubEffectPoint::Exec)?;
        #[cfg(unix)]
        if let Err(error) = termination.register(child.id()) {
            drop(termination);
            let killed = kill_tree(terminate_site, &mut child);
            if killed.is_ok() {
                fate.set(ProcessFate::Gone);
            }
            return Err(error.with_cleanup(killed));
        }
        #[cfg(unix)]
        apply(
            hooks.point(SubEffectPoint::Registered),
            SubEffectPoint::Registered,
        )?;

        let input_error = |error: input::FeedError| UpstrokeError::Agent {
            message: format!("supervising agent input: {error}"),
        };
        let output_error = |error: DrainError| UpstrokeError::Agent {
            message: format!("supervising agent output: {error}"),
        };
        let workers = (|| {
            let pipes = prepared
                .take(&mut child)
                .map_err(|error| UpstrokeError::Agent {
                    message: format!("taking agent pipes: {error}"),
                })?;
            let input =
                input::Feeder::start(pipes.stdin, stdin_data.to_vec()).map_err(input_error)?;
            *input_report = Some(input.failure_report());
            let stdout =
                Drain::start(Stream::Stdout, pipes.stdout, output_limit).map_err(output_error)?;
            *stdout_report = Some(stdout.failure_report());
            let stderr =
                Drain::start(Stream::Stderr, pipes.stderr, output_limit).map_err(output_error)?;
            *stderr_report = Some(stderr.failure_report());
            Ok::<_, UpstrokeError>((Some(input), Some(stdout), Some(stderr)))
        })();
        let (stdin_feeder, stdout_drain, stderr_drain) = match workers {
            Ok(workers) => workers,
            Err(error) => {
                #[cfg(unix)]
                {
                    let group = termination.finish();
                    let group_established = group.is_ok();
                    return Err(settle_failed_supervision(
                        error.with_cleanup(group),
                        group_established,
                        &mut child,
                        &fate,
                    ));
                }
                #[cfg(not(unix))]
                {
                    let killed = kill_tree(terminate_site, &mut child);
                    if killed.is_ok() {
                        fate.set(ProcessFate::Gone);
                    }
                    return Err(error.with_cleanup(killed));
                }
            }
        };

        let mut timed_out = false;
        let mut output_limited = false;
        #[cfg(unix)]
        let code = loop {
            match child_exited_unreaped(&child) {
                Ok(true) => {
                    if let Err(error) = termination.finish() {
                        return Err(settle_failed_supervision(error, false, &mut child, &fate));
                    }
                    fate.set(ProcessFate::Gone);
                    let status = child.wait().map_err(|e| UpstrokeError::Agent {
                        message: format!("reaping agent process: {e}"),
                    })?;
                    break status.code();
                }
                Ok(false) => {
                    if drain_limit_exceeded(&stdout_drain, &stderr_drain) {
                        output_limited = true;
                        if let Err(error) = termination.finish() {
                            return Err(settle_failed_supervision(error, false, &mut child, &fate));
                        }
                        fate.set(ProcessFate::Gone);
                        let _ = child.kill();
                        let _ = child.wait();
                        break None;
                    } else if started.elapsed() >= timeout {
                        timed_out = true;
                        if let Err(error) = termination.finish() {
                            return Err(settle_failed_supervision(error, false, &mut child, &fate));
                        }
                        fate.set(ProcessFate::Gone);
                        let _ = child.kill();
                        let _ = child.wait();
                        break None;
                    }
                    thread::sleep(Duration::from_millis(50));
                }
                Err(e) => {
                    let primary = UpstrokeError::Agent {
                        message: format!("waiting on agent process: {e}"),
                    };
                    let group = termination.finish();
                    let group_established = group.is_ok();
                    return Err(settle_failed_supervision(
                        primary.with_cleanup(group),
                        group_established,
                        &mut child,
                        &fate,
                    ));
                }
            }
        };
        #[cfg(not(unix))]
        let code = loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    child.finish_direct_exit()?;
                    fate.set(ProcessFate::Gone);
                    break status.code();
                }
                Ok(None) => {
                    if drain_limit_exceeded(&stdout_drain, &stderr_drain) {
                        output_limited = true;
                        kill_tree(terminate_site, &mut child)?;
                        fate.set(ProcessFate::Gone);
                        break None;
                    } else if started.elapsed() >= timeout {
                        timed_out = true;
                        kill_tree(terminate_site, &mut child)?;
                        fate.set(ProcessFate::Gone);
                        break None;
                    }
                    thread::sleep(Duration::from_millis(50));
                }
                Err(e) => {
                    let primary = UpstrokeError::Agent {
                        message: format!("waiting on agent process: {e}"),
                    };
                    let killed = kill_tree(terminate_site, &mut child);
                    if killed.is_ok() {
                        fate.set(ProcessFate::Gone);
                    }
                    return Err(primary.with_cleanup(killed));
                }
            }
        };
        let duration = started.elapsed();

        let grace = if timed_out || output_limited {
            DRAIN_GRACE_KILL
        } else {
            DRAIN_GRACE_EXIT
        };
        if let Some(feeder) = stdin_feeder {
            feeder.collect(grace).map_err(input_error)?;
        }
        let collect = |drain: Option<Drain>| -> Result<(String, bool), UpstrokeError> {
            match drain {
                Some(drain) => {
                    let Captured {
                        text,
                        limited,
                        ended: _,
                    } = drain.collect(grace).map_err(output_error)?;
                    Ok((text, limited))
                }
                None => Ok((String::new(), false)),
            }
        };
        let (stdout, stdout_limited) = collect(stdout_drain)?;
        let (stderr, stderr_limited) = collect(stderr_drain)?;
        output_limited |= stdout_limited || stderr_limited;

        Ok(ProcessOutput {
            code,
            stdout,
            stderr,
            duration,
            timed_out,
            output_limited,
        })
    })();
    finish_pipe_reports(outcome, reports).map_err(|error| ProcessFailure {
        fate: fate.get(),
        error,
    })
}

fn finish_pipe_reports<T>(
    outcome: Result<T, UpstrokeError>,
    reports: [Option<worker::FailureReport>; 3],
) -> Result<T, UpstrokeError> {
    let additional = reports
        .into_iter()
        .flatten()
        .filter_map(|report| report.take())
        .collect::<Vec<_>>();
    if additional.is_empty() {
        return outcome;
    }
    let primary = match outcome {
        Ok(_) => UpstrokeError::Agent {
            message: "pipe supervision failed during cleanup".to_owned(),
        },
        Err(primary) => primary,
    };
    Err(additional.into_iter().fold(primary, |primary, message| {
        primary.with_cleanup(Err(UpstrokeError::Agent { message }))
    }))
}

#[cfg(unix)]
fn settle_failed_supervision(
    primary: UpstrokeError,
    group_established: bool,
    child: &mut ProcessTree,
    fate: &std::cell::Cell<ProcessFate>,
) -> UpstrokeError {
    let kill = child.kill();
    let wait = child.wait().map(|_| ());
    if group_established {
        fate.set(ProcessFate::Gone);
    }
    finish_failed_supervision_cleanup(primary, kill, wait)
}

#[cfg(unix)]
fn finish_failed_supervision_cleanup(
    primary: UpstrokeError,
    kill: std::io::Result<()>,
    wait: std::io::Result<()>,
) -> UpstrokeError {
    // A successful wait proves the child was reaped, including the race where
    // it exited before kill. A failed wait cannot justify suppressing kill.
    match wait {
        Ok(_) => primary,
        Err(source) => primary
            .with_cleanup(kill.map_err(|source| UpstrokeError::Agent {
                message: format!("killing agent after supervision failure: {source}"),
            }))
            .with_cleanup(Err(UpstrokeError::Agent {
                message: format!("reaping agent after supervision failure: {source}"),
            })),
    }
}

mod drain;
use self::drain::{Captured, Drain, DrainError, Stream, drain_limit_exceeded};

mod input;
mod pipe_io;
mod worker;

fn kill_tree(terminate_site: ProcessSite, child: &mut ProcessTree) -> Result<(), UpstrokeError> {
    debug_assert_eq!(terminate_site, ProcessSite::Terminate);
    #[cfg(windows)]
    {
        let cleanup = child.job.terminate_and_wait();
        let _ = child.kill();
        let _ = child.wait();
        cleanup.map_err(|error| UpstrokeError::Agent {
            message: format!("terminating the Windows agent job: {error}"),
        })
    }
    #[cfg(not(windows))]
    {
        let mut not_established = Vec::new();
        #[cfg(unix)]
        if let Err(error) = signal_group_kill(child) {
            not_established.push(format!("the group signal failed ({error})"));
        }
        let _ = child.kill();
        if let Err(error) = child.wait() {
            not_established.push(format!("the direct child's reap failed ({error})"));
        }
        if not_established.is_empty() {
            Ok(())
        } else {
            Err(UpstrokeError::Agent {
                message: format!(
                    "terminating the agent process group did not establish it gone: {}",
                    not_established.join("; ")
                ),
            })
        }
    }
}

#[cfg(unix)]
fn signal_group_kill(child: &ProcessTree) -> std::io::Result<()> {
    let pid =
        i32::try_from(child.id()).map_err(|_| std::io::Error::from_raw_os_error(libc::EINVAL))?;
    // SAFETY: `run_with_timeout` put this child in a new process group whose id
    // is the child's pid. A negative pid targets that group only.
    if unsafe { libc::kill(-pid, libc::SIGKILL) } == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        return Ok(());
    }
    Err(error)
}

#[cfg(all(unix, test))]
pub(crate) fn child_leads_its_own_group(pid: u32) -> bool {
    observe_child_group(pid).leads_own_group()
}

#[cfg(all(target_os = "macos", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ZombieGroupAnswer {
    pub(crate) pgid: u32,
    pub(crate) status: u32,
}

#[cfg(all(unix, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GroupObservation {
    pub(crate) pid: libc::pid_t,
    pub(crate) exited_before: Result<bool, i32>,
    pub(crate) group: Result<libc::pid_t, i32>,
    #[cfg(target_os = "macos")]
    pub(crate) zombie_group: Result<ZombieGroupAnswer, i32>,
    pub(crate) exited_after: Result<bool, i32>,
}

#[cfg(all(unix, test))]
impl GroupObservation {
    pub(crate) fn leads_own_group(&self) -> bool {
        match self.group {
            Ok(pgid) => pgid == self.pid,
            Err(errno) => errno == libc::ESRCH && self.exited_record_names_its_own_group(),
        }
    }

    #[cfg(target_os = "macos")]
    fn exited_record_names_its_own_group(&self) -> bool {
        if self.exited_before != Ok(true) {
            return false;
        }
        match (u32::try_from(self.pid), self.zombie_group) {
            (Ok(pid), Ok(answer)) => answer.pgid == pid && answer.status == libc::SZOMB,
            _ => false,
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn exited_record_names_its_own_group(&self) -> bool {
        false
    }

    pub(crate) fn had_exited_before_the_look(&self) -> bool {
        self.exited_before == Ok(true)
    }
}

#[cfg(all(unix, test))]
impl std::fmt::Display for GroupObservation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn exit_answer(answer: Result<bool, i32>) -> String {
            match answer {
                Ok(true) => "exited, unreaped".to_owned(),
                Ok(false) => "running".to_owned(),
                Err(errno) => format!(
                    "waitid failed: {}",
                    std::io::Error::from_raw_os_error(errno)
                ),
            }
        }
        write!(
            f,
            "pid {}: before the look {}; getpgid {}",
            self.pid,
            exit_answer(self.exited_before),
            match self.group {
                Ok(pgid) => format!("answered {pgid}"),
                Err(errno) => format!("failed: {}", std::io::Error::from_raw_os_error(errno)),
            },
        )?;
        #[cfg(target_os = "macos")]
        match self.zombie_group {
            Ok(answer) => write!(
                f,
                "; proc_pidinfo(PROC_PIDT_SHORTBSDINFO, zombie) answered pgid {} status {}",
                answer.pgid, answer.status
            )?,
            Err(errno) => write!(
                f,
                "; proc_pidinfo(PROC_PIDT_SHORTBSDINFO, zombie) failed: {}",
                std::io::Error::from_raw_os_error(errno)
            )?,
        }
        write!(f, "; after the look {}", exit_answer(self.exited_after))
    }
}

#[cfg(all(unix, test))]
pub(crate) fn observe_child_group(pid: u32) -> GroupObservation {
    let (Ok(signed), Ok(unsigned)) = (libc::pid_t::try_from(pid), libc::id_t::try_from(pid)) else {
        return GroupObservation {
            pid: -1,
            exited_before: Err(libc::EINVAL),
            group: Err(libc::EINVAL),
            #[cfg(target_os = "macos")]
            zombie_group: Err(libc::EINVAL),
            exited_after: Err(libc::EINVAL),
        };
    };
    let exited_before =
        exited_unreaped(unsigned).map_err(|error| error.raw_os_error().unwrap_or(0));
    // SAFETY: `getpgid` reads process-table state for a pid this process owns
    // as a child and has not reaped; it borrows nothing.
    let pgid = unsafe { libc::getpgid(signed) };
    let group = if pgid < 0 {
        Err(last_errno_of_observation())
    } else {
        Ok(pgid)
    };
    #[cfg(target_os = "macos")]
    let zombie_group = zombie_group_answer(signed);
    let exited_after = exited_unreaped(unsigned).map_err(|error| error.raw_os_error().unwrap_or(0));
    GroupObservation {
        pid: signed,
        exited_before,
        group,
        #[cfg(target_os = "macos")]
        zombie_group,
        exited_after,
    }
}

#[cfg(all(unix, test))]
fn last_errno_of_observation() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

#[cfg(all(target_os = "macos", test))]
fn zombie_group_answer(pid: libc::pid_t) -> Result<ZombieGroupAnswer, i32> {
    // SAFETY: `proc_bsdshortinfo` is plain data for which zeroes are a valid
    // value; the kernel writes at most the byte count passed, which is the
    // size of the buffer it is given.
    let mut info: libc::proc_bsdshortinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdshortinfo>();
    let Ok(size_arg) = libc::c_int::try_from(size) else {
        return Err(libc::EINVAL);
    };
    // SAFETY: `info` outlives the call and is exactly `size` bytes; the
    // non-zero third argument asks the kernel for an exited, unreaped record.
    let read = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDT_SHORTBSDINFO,
            1,
            (&raw mut info).cast(),
            size_arg,
        )
    };
    if read <= 0 {
        return Err(last_errno_of_observation());
    }
    if read != size_arg {
        return Err(libc::EIO);
    }
    Ok(ZombieGroupAnswer {
        pgid: info.pbsi_pgid,
        status: info.pbsi_status,
    })
}

#[cfg(all(unix, test))]
pub(crate) fn await_exit_without_reaping(pid: u32, budget: Duration) -> Result<(), String> {
    let unsigned = libc::id_t::try_from(pid)
        .map_err(|_| format!("pid {pid} cannot be represented as a Unix wait id"))?;
    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let outcome = loop {
            // SAFETY: `siginfo_t` is plain data for which zeroes are a valid value.
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            // SAFETY: `info` outlives the call; `WNOWAIT` leaves the child
            // waitable, so the owner's later `wait` still reaps it.
            let result = unsafe {
                libc::waitid(
                    libc::P_PID,
                    unsigned,
                    &raw mut info,
                    libc::WEXITED | libc::WNOWAIT,
                )
            };
            if result == 0 {
                break Ok(());
            }
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::EINTR) {
                break Err(format!(
                    "waitid(WEXITED | WNOWAIT) on {pid} failed: {error}"
                ));
            }
        };
        let _ = tx.send(outcome);
    });
    match rx.recv_timeout(budget) {
        Ok(outcome) => outcome,
        Err(_) => Err(format!("pid {pid} had not exited within {budget:?}")),
    }
}

mod ambient;
#[cfg(all(windows, test))]
pub(crate) use self::ambient::poison_ambient_for_tests;
pub use self::ambient::{
    AMBIENT_REFUSAL_PREFIX, AMBIENT_REFUSAL_SIMULATED, join_ambient_job,
    set_container_reclaim_scope,
};
#[cfg(windows)]
pub use self::ambient::{
    ambient_job_established, child_in_ambient_job, process_alive, process_creation_time,
};

#[cfg(windows)]
mod windows_job {
    use std::io;
    use std::mem::size_of;
    use std::os::windows::io::AsRawHandle;
    use std::os::windows::process::CommandExt;
    use std::process::{Child, Command};
    use std::ptr;
    use std::thread;
    use std::time::{Duration, Instant};

    use std::sync::OnceLock;

    use windows_sys::Win32::Foundation::{
        CloseHandle, FILETIME, HANDLE, INVALID_HANDLE_VALUE, WAIT_TIMEOUT,
    };
    use windows_sys::Win32::Storage::FileSystem::SYNCHRONIZE;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, IsProcessInJob, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
        QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
    };
    use windows_sys::Win32::System::Threading::{
        CREATE_SUSPENDED, GetCurrentProcess, GetProcessTimes, OpenProcess, OpenThread,
        PROCESS_QUERY_LIMITED_INFORMATION, ResumeThread, THREAD_SUSPEND_RESUME,
        WaitForSingleObject,
    };

    use super::{ProcessFate, SpawnFailure, SpawnHooks, SubEffectPoint, apply_io};

    const CLEANUP_TIMEOUT: Duration = Duration::from_secs(2);

    pub(super) struct Job {
        handle: HANDLE,
    }

    fn real_create_job() -> HANDLE {
        // SAFETY: null security attributes and name request an unnamed,
        // non-inheritable job owned solely by this process.
        unsafe {
            windows_sys::Win32::System::JobObjects::CreateJobObjectW(ptr::null(), ptr::null())
        }
    }

    fn real_configure_job(
        handle: HANDLE,
        limits: &JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        size: u32,
    ) -> i32 {
        // SAFETY: `limits` has exactly the layout and lifetime required by
        // JobObjectExtendedLimitInformation; `handle` is live.
        unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                ptr::from_ref(limits).cast(),
                size,
            )
        }
    }

    fn real_terminate_job(handle: HANDLE) -> i32 {
        // SAFETY: the handle remains live for this call and the requested exit
        // code has no semantic meaning outside this private job.
        unsafe { TerminateJobObject(handle, 1) }
    }

    fn real_query_accounting(
        handle: HANDLE,
        accounting: &mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
    ) -> i32 {
        // SAFETY: the output buffer is correctly typed and sized and the
        // optional returned-length pointer is not needed.
        #[expect(clippy::expect_used, reason = "a fixed Win32 struct size fits in u32")]
        unsafe {
            QueryInformationJobObject(
                handle,
                JobObjectBasicAccountingInformation,
                ptr::from_mut(accounting).cast(),
                u32::try_from(size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>())
                    .expect("job accounting structure fits in u32"),
                ptr::null_mut(),
            )
        }
    }

    impl Job {
        fn create() -> io::Result<Self> {
            Self::create_with(real_create_job, real_configure_job)
        }

        fn create_with(
            create: impl FnOnce() -> HANDLE,
            configure: impl FnOnce(HANDLE, &JOBOBJECT_EXTENDED_LIMIT_INFORMATION, u32) -> i32,
        ) -> io::Result<Self> {
            let handle = create();
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            let job = Self { handle };
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            #[expect(clippy::expect_used, reason = "a fixed Win32 struct size fits in u32")]
            let size = u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
                .expect("job information structure fits in u32");
            if configure(job.handle, &limits, size) == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(job)
        }

        pub(super) fn terminate_and_wait(&self) -> io::Result<()> {
            self.terminate_and_wait_with(real_terminate_job, real_query_accounting)
        }

        pub(super) fn terminate_and_wait_with(
            &self,
            terminate: impl FnOnce(HANDLE) -> i32,
            query: impl Fn(HANDLE, &mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION) -> i32,
        ) -> io::Result<()> {
            if terminate(self.handle) == 0 {
                return Err(io::Error::last_os_error());
            }
            let deadline = Instant::now() + CLEANUP_TIMEOUT;
            loop {
                if self.active_processes_with(&query)? == 0 {
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "Windows agent job did not become empty within 2 seconds",
                    ));
                }
                thread::sleep(Duration::from_millis(10));
            }
        }

        pub(super) fn active_processes_with(
            &self,
            query: impl Fn(HANDLE, &mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION) -> i32,
        ) -> io::Result<u32> {
            let mut accounting = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
            if query(self.handle, &mut accounting) == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(accounting.ActiveProcesses)
        }

        #[cfg(test)]
        pub(super) fn contains(&self, pid: u32) -> Option<bool> {
            job_contains(self.handle, pid)
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: this object uniquely owns the non-inheritable handle.
            // KILL_ON_JOB_CLOSE is the final fail-safe if explicit settlement
            // returned an error or the conductor is being torn down.
            let _ = unsafe { CloseHandle(self.handle) };
        }
    }

    pub(super) fn spawn_suspended_in_job(
        command: &mut Command,
        hooks: &mut dyn SpawnHooks,
    ) -> Result<(Child, Job), SpawnFailure> {
        spawn_suspended_in_job_with(command, hooks, real_assign_to_job, resume_only_thread)
    }

    pub(super) fn real_assign_to_job(job: HANDLE, process: HANDLE) -> i32 {
        // SAFETY: `Child` owns a live process handle and `job` is live; both
        // are process-wide kernel object references, not borrowed memory.
        unsafe { AssignProcessToJobObject(job, process) }
    }

    #[cfg(test)]
    pub(super) fn job_contains(job: HANDLE, pid: u32) -> Option<bool> {
        let process = OpenHandle::open(pid)?;
        let mut member = 0;
        // SAFETY: both handles are live and `member` is a writable BOOL.
        let queried = unsafe { IsProcessInJob(process.0, job, &raw mut member) };
        if queried == 0 {
            return None;
        }
        Some(member != 0)
    }

    pub(super) fn spawn_suspended_in_job_with(
        command: &mut Command,
        hooks: &mut dyn SpawnHooks,
        assign: impl FnOnce(HANDLE, HANDLE) -> i32,
        resume: impl FnOnce(u32) -> io::Result<()>,
    ) -> Result<(Child, Job), SpawnFailure> {
        let never_started = |error| SpawnFailure {
            error,
            fate: ProcessFate::NeverStarted,
        };
        let job = Job::create().map_err(never_started)?;
        command.creation_flags(CREATE_SUSPENDED);
        let mut child = command.spawn().map_err(never_started)?;
        hooks.child_created(child.id());
        if let Err(error) = apply_io(
            hooks.point(SubEffectPoint::CreatedSuspended),
            SubEffectPoint::CreatedSuspended,
        ) {
            return Err(settle_suspended(error, &mut child, None));
        }
        let assigned = assign(job.handle, child.as_raw_handle() as HANDLE);
        if assigned == 0 {
            let error = io::Error::last_os_error();
            return Err(settle_suspended(error, &mut child, None));
        }
        if let Err(error) = apply_io(
            hooks.point(SubEffectPoint::PrivateJobAssigned),
            SubEffectPoint::PrivateJobAssigned,
        ) {
            return Err(settle_suspended(error, &mut child, Some(&job)));
        }
        if let Err(error) = resume(child.id()) {
            return Err(settle_resumed(error, &mut child, &job));
        }
        if let Err(error) = apply_io(
            hooks.point(SubEffectPoint::Resumed),
            SubEffectPoint::Resumed,
        ) {
            return Err(settle_resumed(error, &mut child, &job));
        }
        Ok((child, job))
    }

    fn settle_suspended(error: io::Error, child: &mut Child, job: Option<&Job>) -> SpawnFailure {
        let job_empty = job.is_some_and(|job| job.terminate_and_wait().is_ok());
        let _ = child.kill();
        let reaped = child.wait().is_ok();
        SpawnFailure {
            error,
            fate: if job_empty || reaped {
                ProcessFate::Gone
            } else {
                ProcessFate::Unresolved
            },
        }
    }

    fn settle_resumed(error: io::Error, child: &mut Child, job: &Job) -> SpawnFailure {
        let job_empty = job.terminate_and_wait().is_ok();
        let _ = child.kill();
        let _ = child.wait();
        SpawnFailure {
            error,
            fate: if job_empty {
                ProcessFate::Gone
            } else {
                ProcessFate::Unresolved
            },
        }
    }

    static AMBIENT: OnceLock<Result<AmbientJob, String>> = OnceLock::new();

    #[derive(Debug)]
    struct AmbientJob(HANDLE);

    // SAFETY: a Windows `HANDLE` is a process-wide reference to a kernel
    // object, not a pointer into this process's memory. The only calls made on
    // this one -- `AssignProcessToJobObject` and `IsProcessInJob` -- are
    // thread-safe, the value is never mutated after the `OnceLock` is set, and
    // it is never closed.
    unsafe impl Send for AmbientJob {}
    // SAFETY: as above.
    unsafe impl Sync for AmbientJob {}

    pub(super) fn join_ambient() -> Result<(), String> {
        super::memoised_outcome(AMBIENT.get_or_init(|| {
            // SAFETY: `GetCurrentProcess` is the documented pseudo-handle for
            // this process and the job handle is live. Windows 8 and later
            // nest jobs, so an existing job (cargo's, a CI runner's, an
            // OpenSSH session's) is a parent of this one rather than a
            // conflict.
            create_ambient(|job, process| unsafe { AssignProcessToJobObject(job, process) })
        }))
    }

    #[cfg(test)]
    pub(super) fn poison_ambient_for_tests(message: &str) -> bool {
        AMBIENT.set(Err(message.to_owned())).is_ok()
    }

    fn create_ambient(assign: impl Fn(HANDLE, HANDLE) -> i32) -> Result<AmbientJob, String> {
        create_ambient_with(Job::create, assign)
    }

    fn create_ambient_with(
        make_job: impl FnOnce() -> io::Result<Job>,
        assign: impl Fn(HANDLE, HANDLE) -> i32,
    ) -> Result<AmbientJob, String> {
        let job = make_job().map_err(|error| format!("it could not be created ({error})"))?;
        // SAFETY: `GetCurrentProcess` is the documented pseudo-handle for this
        // process and the job handle is live.
        let joined = assign(job.handle, unsafe { GetCurrentProcess() });
        if joined == 0 {
            return Err(format!(
                "this process could not join it ({})",
                io::Error::last_os_error()
            ));
        }
        let job = std::mem::ManuallyDrop::new(job);
        Ok(AmbientJob(job.handle))
    }

    pub(super) fn ambient_established() -> bool {
        matches!(AMBIENT.get(), Some(Ok(_)))
    }

    pub(super) fn ambient_contains(pid: u32) -> Option<bool> {
        let Some(Ok(job)) = AMBIENT.get() else {
            return None;
        };
        let process = OpenHandle::open(pid)?;
        let mut member = 0;
        // SAFETY: both handles are live and `member` is a writable BOOL.
        let queried = unsafe { IsProcessInJob(process.0, job.0, &raw mut member) };
        if queried == 0 {
            return None;
        }
        Some(member != 0)
    }

    struct OpenHandle(HANDLE);

    impl OpenHandle {
        fn open(pid: u32) -> Option<Self> {
            // SAFETY: no borrowed inputs; a failure returns null.
            let handle =
                unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, 0, pid) };
            if handle.is_null() {
                return None;
            }
            Some(Self(handle))
        }
    }

    impl Drop for OpenHandle {
        fn drop(&mut self) {
            // SAFETY: this wrapper uniquely owns the handle it opened.
            let _ = unsafe { CloseHandle(self.0) };
        }
    }

    fn creation_time(handle: HANDLE) -> Option<u64> {
        let mut created = FILETIME::default();
        let mut exited = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        // SAFETY: four correctly typed writable output structures and a live
        // handle with PROCESS_QUERY_LIMITED_INFORMATION.
        let queried = unsafe {
            GetProcessTimes(
                handle,
                &raw mut created,
                &raw mut exited,
                &raw mut kernel,
                &raw mut user,
            )
        };
        if queried == 0 {
            return None;
        }
        Some((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
    }

    pub(super) fn process_creation_time(pid: u32) -> Option<u64> {
        let handle = OpenHandle::open(pid)?;
        creation_time(handle.0)
    }

    pub(super) fn process_alive(pid: u32, expected_creation_time: u64) -> bool {
        let Some(handle) = OpenHandle::open(pid) else {
            return false;
        };
        if creation_time(handle.0) != Some(expected_creation_time) {
            return false;
        }
        // SAFETY: the handle carries SYNCHRONIZE. A process object is signaled
        // exactly when the process has terminated, which is a stronger answer
        // than an exit code a job termination chooses for us.
        unsafe { WaitForSingleObject(handle.0, 0) == WAIT_TIMEOUT }
    }

    pub(super) fn resume_only_thread(process_id: u32) -> io::Result<()> {
        let thread_handle = primary_thread(process_id)?;
        // SAFETY: this handle has THREAD_SUSPEND_RESUME access and identifies
        // the primary thread created suspended by `Command::spawn`.
        if unsafe { ResumeThread(thread_handle.0) } == u32::MAX {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn primary_thread_suspend_count(process_id: u32) -> io::Result<u32> {
        use windows_sys::Win32::System::Threading::SuspendThread;

        let thread_handle = primary_thread(process_id)?;
        // SAFETY: the handle carries THREAD_SUSPEND_RESUME and names a live
        // thread; the immediately following resume restores the count.
        let previous = unsafe { SuspendThread(thread_handle.0) };
        if previous == u32::MAX {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: as above; this undoes the suspend just taken.
        if unsafe { ResumeThread(thread_handle.0) } == u32::MAX {
            return Err(io::Error::last_os_error());
        }
        Ok(previous)
    }

    fn primary_thread(process_id: u32) -> io::Result<Snapshot> {
        // CREATE_SUSPENDED prevents the process from creating another thread,
        // so the one owned thread in this system snapshot is necessarily its
        // primary thread.
        // SAFETY: the snapshot call has no borrowed inputs.
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let snapshot = Snapshot(snapshot);
        #[expect(clippy::expect_used, reason = "a fixed Win32 struct size fits in u32")]
        let mut entry = THREADENTRY32 {
            dwSize: u32::try_from(size_of::<THREADENTRY32>())
                .expect("thread entry structure fits in u32"),
            ..THREADENTRY32::default()
        };
        // SAFETY: `entry` advertises its correct size and remains writable.
        if unsafe { Thread32First(snapshot.0, &mut entry) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let thread_id = loop {
            if entry.th32OwnerProcessID == process_id {
                break entry.th32ThreadID;
            }
            // SAFETY: same valid snapshot and output entry as above.
            if unsafe { Thread32Next(snapshot.0, &mut entry) } == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "could not find the suspended agent primary thread",
                ));
            }
        };
        // SAFETY: the enumerated thread id belongs to the still-suspended
        // child; the returned handle is non-inheritable.
        let thread_handle = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, thread_id) };
        if thread_handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Snapshot(thread_handle))
    }

    struct Snapshot(HANDLE);

    impl Drop for Snapshot {
        fn drop(&mut self) {
            // SAFETY: this wrapper uniquely owns its snapshot/thread handle.
            let _ = unsafe { CloseHandle(self.0) };
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn the_ambient_join_reads_a_win32_bool_the_way_win32_defines_one() {
            let refused =
                create_ambient(|_, _| 0).expect_err("a zero BOOL is a refused assignment");
            assert!(
                refused.contains("could not join"),
                "the diagnostic must name the join: {refused}"
            );

            for value in [1_i32, -1, i32::MIN, i32::MAX] {
                let job = create_ambient(move |_, _| value)
                    .unwrap_or_else(|error| panic!("BOOL {value} is success, not: {error}"));
                assert!(!job.0.is_null(), "BOOL {value} produced no job handle");
            }
        }

        #[test]
        fn the_ambient_job_refuses_when_it_cannot_be_created_or_configured() {
            use std::cell::Cell;

            let configured = Cell::new(false);
            let refused = create_ambient_with(
                || {
                    Job::create_with(ptr::null_mut, |_, _, _| {
                        configured.set(true);
                        1
                    })
                },
                |_, _| panic!("a job that was never created must not be joined"),
            )
            .expect_err("a job that cannot be created is not an ambient job");
            assert!(
                !configured.get(),
                "an uncreated job was handed to SetInformationJobObject"
            );
            assert!(
                refused.contains("could not be created"),
                "the diagnostic must name creation: {refused}"
            );

            let refused = create_ambient_with(
                || Job::create_with(real_create_job, |_, _, _| 0),
                |_, _| panic!("an unconfigured job must not be joined"),
            )
            .expect_err("an unconfigured job is not an ambient job");
            assert!(
                refused.contains("could not be created"),
                "the diagnostic must name establishment: {refused}"
            );
        }

        #[test]
        fn every_job_this_module_creates_is_configured_to_kill_on_close() {
            use std::cell::Cell;

            let seen = Cell::new(None);
            let job = Job::create_with(real_create_job, |handle, limits, size| {
                seen.set(Some((limits.BasicLimitInformation.LimitFlags, size)));
                real_configure_job(handle, limits, size)
            })
            .expect("create a job the ordinary way");
            assert!(!job.handle.is_null());
            let (flags, size) = seen.get().expect("the configuration call was made");
            assert_eq!(
                flags, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                "the kill-on-close fail-safe is the limit this job exists for"
            );
            assert_eq!(
                size,
                u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>()).expect("fits"),
                "the extended limit structure is declared at its own size"
            );
        }

        #[test]
        fn a_failed_accounting_query_is_never_read_as_an_empty_job() {
            let job = Job::create().expect("create a job");
            let error = job
                .active_processes_with(|_, _| 0)
                .expect_err("a zero BOOL from QueryInformationJobObject is a failure");
            assert!(
                !format!("{error}").is_empty(),
                "the OS's reason must survive"
            );
            for reported in [0_u32, 1, 7] {
                let observed = job
                    .active_processes_with(move |_, accounting| {
                        accounting.ActiveProcesses = reported;
                        1
                    })
                    .expect("a successful query is not an error");
                assert_eq!(observed, reported);
            }
        }

        #[test]
        fn cleanup_polls_the_accounting_until_the_job_is_empty() {
            use std::cell::Cell;

            let job = Job::create().expect("create a job");
            let terminated = Cell::new(false);
            let answers = Cell::new(0_usize);
            job.terminate_and_wait_with(
                |_| {
                    terminated.set(true);
                    1
                },
                |_, accounting| {
                    let index = answers.get();
                    answers.set(index + 1);
                    accounting.ActiveProcesses = if index < 2 { 1 } else { 0 };
                    1
                },
            )
            .expect("cleanup completes once the job reports empty");
            assert!(terminated.get(), "the job was never terminated");
            assert_eq!(
                answers.get(),
                3,
                "cleanup returned before the accounting said zero, or kept asking after it did"
            );
        }

        #[test]
        fn cleanup_gives_up_on_a_job_that_never_empties() {
            use std::sync::mpsc;

            let (sender, receiver) = mpsc::channel();
            thread::spawn(move || {
                let job = Job::create().expect("create a job");
                let outcome = job
                    .terminate_and_wait_with(
                        |_| 1,
                        |_, accounting| {
                            accounting.ActiveProcesses = 1;
                            1
                        },
                    )
                    .map_err(|error| (error.kind(), error.to_string()));
                let _ = sender.send(outcome);
            });
            let outcome = receiver
                .recv_timeout(Duration::from_secs(30))
                .expect("cleanup must be bounded: it never returned");
            let (kind, message) = outcome.expect_err("a job that never empties is not settled");
            assert_eq!(kind, io::ErrorKind::TimedOut, "{message}");
            assert!(
                message.contains("2 seconds"),
                "the diagnostic must name its bound: {message}"
            );
        }
    }
}

#[cfg(unix)]
fn child_exited_unreaped(child: &Child) -> std::io::Result<bool> {
    let pid = libc::id_t::try_from(child.id())
        .map_err(|_| std::io::Error::from_raw_os_error(libc::EINVAL))?;
    exited_unreaped(pid)
}

#[cfg(unix)]
fn exited_unreaped(pid: libc::id_t) -> std::io::Result<bool> {
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    let result = unsafe {
        libc::waitid(
            libc::P_PID,
            pid,
            &mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    if result != 0 {
        return Err(std::io::Error::last_os_error());
    }
    if unsafe { info.si_pid() } == 0 {
        return Ok(false);
    }
    Ok(matches!(
        info.si_code,
        libc::CLD_EXITED | libc::CLD_KILLED | libc::CLD_DUMPED
    ))
}

#[cfg(unix)]
mod termination {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, Ordering};
    use std::sync::{Arc, Mutex, OnceLock};
    use std::thread;
    use std::time::Duration;

    use crate::error::UpstrokeError;
    use crate::topology::effects::ProcessSite;

    static PENDING_TERMINATION: AtomicI32 = AtomicI32::new(0);
    static SUSPEND_REQUESTED: AtomicBool = AtomicBool::new(false);
    static CONTINUE_REQUESTED: AtomicBool = AtomicBool::new(false);
    static SUSPEND_ARMED: AtomicBool = AtomicBool::new(false);
    static GUARD_COMMAND_FD: AtomicI32 = AtomicI32::new(-1);
    static GUARD_WAKE_FD: AtomicI32 = AtomicI32::new(-1);
    static PROBE_PID: AtomicI32 = AtomicI32::new(-1);
    static HANDLED_TERMINATION_MASK: AtomicU8 = AtomicU8::new(0);
    static STATE: OnceLock<Result<Arc<Mutex<State>>, String>> = OnceLock::new();
    /// What a probe in a forked child found this host's identity system calls
    /// answering, and zero until one has run.
    ///
    /// A syscall policy can make `pidfd_open` fatal rather than refuse it --
    /// `SECCOMP_RET_KILL_PROCESS` is the disposition a disallowed call draws by
    /// default under systemd's `SystemCallFilter=`, and it ends the process
    /// before there is an errno to read -- and a policy can write `ESRCH` or
    /// `ECHILD` itself, which are the two answers this module otherwise reads
    /// as facts about a helper. Neither can be told from inside the call, so
    /// the calls are made once where being killed for making them is
    /// survivable, and what they answered there is what the calls made here are
    /// read against.
    #[cfg(target_os = "linux")]
    static IDENTITY_CALLS: AtomicU8 = AtomicU8::new(0);

    const GUARD_READY: u8 = 0x91;
    const GUARD_ARM: u8 = 0xa1;
    const GUARD_STOP: u8 = 0xb1;
    const GUARD_STOPPED: u8 = 0xb2;
    const GUARD_CANCELLED: u8 = 0xc1;
    const GUARD_DISARM: u8 = 0xd1;
    const GUARD_PROBE: u8 = 0xe1;
    const GUARD_POLL_SLICE_MS: libc::c_int = 250;
    const HANDLE_SIGINT: u8 = 1 << 0;
    const HANDLE_SIGTERM: u8 = 1 << 1;
    const HANDLE_SIGHUP: u8 = 1 << 2;
    const HANDLE_SIGQUIT: u8 = 1 << 3;
    const HANDLE_SIGTSTP: u8 = 1 << 0;
    const HANDLE_SIGTTIN: u8 = 1 << 1;
    const HANDLE_SIGTTOU: u8 = 1 << 2;
    const REAPER_READY: u8 = 0x81;
    const REAPER_REGISTER: u8 = 0x82;
    const REAPER_CLEANUP: u8 = 0x83;
    const REAPER_OK: u8 = 0x84;
    const REAPER_FAIL: u8 = 0x85;
    const REAPER_CANCEL: u8 = 0x86;
    const REAPER_RESUME_STABLE_POLLS: u8 = 50;
    const HELPER_ABORT: u8 = 0x71;
    const SETUP_FAILURE_FRAME_LEN: usize = 7;

    const HELPER_READY_BUDGET: Duration = Duration::from_secs(2);

    /// A probe reported, so none of the identity system calls ended the process
    /// that made them. Carried by every reported answer, so that an answer
    /// holding no other bit is still an answer and not "no probe has run".
    #[cfg(target_os = "linux")]
    const IDENTITY_PROBED: u8 = 1 << 0;
    /// `pidfd_open` answered with a descriptor for a process that was there.
    #[cfg(target_os = "linux")]
    const IDENTITY_NAMES: u8 = 1 << 1;
    /// `pidfd_send_signal(..., SIGKILL, ...)` answered zero through a
    /// descriptor naming a process that was there, so an `ESRCH` from it is the
    /// kernel answering about a helper and not a policy writing an errno.
    #[cfg(target_os = "linux")]
    const IDENTITY_SIGNALS: u8 = 1 << 2;
    /// `waitid(P_PIDFD, ..., WEXITED)` collected that process, so an `ECHILD`
    /// from it is the kernel answering about a helper and not a policy writing
    /// an errno.
    #[cfg(target_os = "linux")]
    const IDENTITY_COLLECTS: u8 = 1 << 3;

    #[derive(Clone, Copy)]
    struct SignalPolicy {
        termination_mask: u8,
        guard_wake_mask: u8,
        stop_mask: u8,
        job_control: bool,
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum SignalDisposition {
        Default,
        Ignored,
        Custom,
    }

    impl SignalPolicy {
        fn handles_termination(self, signal: libc::c_int) -> bool {
            let bit = match signal {
                libc::SIGINT => HANDLE_SIGINT,
                libc::SIGTERM => HANDLE_SIGTERM,
                libc::SIGHUP => HANDLE_SIGHUP,
                libc::SIGQUIT => HANDLE_SIGQUIT,
                _ => return false,
            };
            self.termination_mask & bit != 0
        }

        fn wakes_guard(self, signal: libc::c_int) -> bool {
            let bit = match signal {
                libc::SIGINT => HANDLE_SIGINT,
                libc::SIGTERM => HANDLE_SIGTERM,
                libc::SIGHUP => HANDLE_SIGHUP,
                libc::SIGQUIT => HANDLE_SIGQUIT,
                _ => return false,
            };
            self.guard_wake_mask & bit != 0
        }

        fn handles_stop(self, signal: libc::c_int) -> bool {
            let bit = match signal {
                libc::SIGTSTP => HANDLE_SIGTSTP,
                libc::SIGTTIN => HANDLE_SIGTTIN,
                libc::SIGTTOU => HANDLE_SIGTTOU,
                _ => return false,
            };
            self.stop_mask & bit != 0
        }
    }

    struct State {
        spawning: usize,
        groups: Vec<RegisteredGroup>,
        terminating: bool,
        suspending: bool,
        guard: Guard,
    }

    struct RegisteredGroup {
        pgid: i32,
        signal_leases: usize,
    }

    struct GroupSnapshot {
        state: Arc<Mutex<State>>,
        pgids: Vec<i32>,
    }

    impl std::ops::Deref for GroupSnapshot {
        type Target = [i32];

        fn deref(&self) -> &Self::Target {
            &self.pgids
        }
    }

    impl Drop for GroupSnapshot {
        fn drop(&mut self) {
            let mut locked = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            for pgid in &self.pgids {
                if let Some(group) = locked.groups.iter_mut().find(|group| group.pgid == *pgid) {
                    group.signal_leases = group.signal_leases.saturating_sub(1);
                }
            }
        }
    }

    #[derive(Clone, Copy)]
    struct Reaper {
        command_fd: libc::c_int,
        ack_fd: libc::c_int,
        _command_keepalive_fd: libc::c_int,
        pid: libc::pid_t,
        identity: libc::c_int,
    }

    #[derive(Clone, Copy)]
    struct Guard {
        command_fd: libc::c_int,
        ack_fd: libc::c_int,
        _command_keepalive_fd: libc::c_int,
        pid: libc::pid_t,
        identity: libc::c_int,
    }

    enum Phase {
        Spawning,
        Group(i32),
        Finished,
    }

    pub(super) struct Supervisor {
        state: Arc<Mutex<State>>,
        phase: Phase,
        reaper: Reaper,
        terminate_site: ProcessSite,
    }

    impl Supervisor {
        pub(super) fn begin(terminate_site: ProcessSite) -> Result<Self, UpstrokeError> {
            if terminate_site != ProcessSite::Terminate {
                return Err(UpstrokeError::Agent {
                    message: format!(
                        "process termination requires Process.Terminate, got {}",
                        terminate_site.name()
                    ),
                });
            }
            Self::begin_with_state(shared_state()?, terminate_site)
        }

        fn begin_with_state(
            state: Arc<Mutex<State>>,
            terminate_site: ProcessSite,
        ) -> Result<Self, UpstrokeError> {
            claim_launch(&state)?;
            let reaper = match spawn_reaper() {
                Ok(reaper) => reaper,
                Err(message) => {
                    release_launch(&state);
                    return Err(UpstrokeError::Agent { message });
                }
            };
            Ok(Self {
                state,
                phase: Phase::Spawning,
                reaper,
                terminate_site,
            })
        }

        pub(super) fn prepare(&self, command: &mut std::process::Command) {
            use std::os::unix::process::CommandExt;

            let reaper = self.reaper;
            // SAFETY: the closure uses only async-signal-safe syscalls. It
            // creates the private process group and registers it with the
            // external reaper before exec, so even SIGKILL in the parent's
            // post-spawn registration window cannot orphan the agent tree.
            unsafe {
                command.pre_exec(move || {
                    let pid = libc::getpid();
                    if libc::setpgid(0, 0) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    if reaper.register_raw(pid) {
                        Ok(())
                    } else {
                        Err(std::io::Error::from_raw_os_error(libc::EIO))
                    }
                });
            }
        }

        pub(super) fn register(&mut self, pid: u32) -> Result<(), UpstrokeError> {
            let pgid = i32::try_from(pid).map_err(|_| UpstrokeError::Agent {
                message: format!("agent pid {pid} cannot be represented as a Unix process group"),
            })?;
            let mut locked = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            locked.spawning = locked.spawning.saturating_sub(1);
            locked.groups.push(RegisteredGroup {
                pgid,
                signal_leases: 0,
            });
            self.phase = Phase::Group(pgid);
            Ok(())
        }

        pub(super) fn finish(&mut self) -> Result<(), UpstrokeError> {
            if self.terminate_site != ProcessSite::Terminate {
                return Err(UpstrokeError::Agent {
                    message: format!(
                        "process termination requires Process.Terminate, got {}",
                        self.terminate_site.name()
                    ),
                });
            }
            let Phase::Group(pgid) = self.phase else {
                return Ok(());
            };
            self.phase = Phase::Finished;
            if !self.reaper.cleanup(pgid) {
                arm_fail_closed_termination(
                    b"upstroke: fail-closed SIGTERM armed: cleanup reaper did not acknowledge CLEANUP\n",
                );
                return Err(UpstrokeError::Agent {
                    message: format!(
                        "Unix cleanup reaper failed while settling process group {pgid}"
                    ),
                });
            }
            loop {
                let mut locked = self
                    .state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if !locked.groups.iter().any(|group| group.pgid == pgid) {
                    return Ok(());
                }
                if remove_unpinned_group(&mut locked, pgid) {
                    return Ok(());
                }
                drop(locked);
                thread::sleep(Duration::from_millis(1));
            }
        }
    }

    fn claim_launch(state: &Arc<Mutex<State>>) -> Result<(), UpstrokeError> {
        loop {
            let mut locked = state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if locked.terminating || PENDING_TERMINATION.load(Ordering::SeqCst) != 0 {
                return Err(UpstrokeError::Agent {
                    message: "process launch interrupted by a termination signal".to_owned(),
                });
            }
            if !locked.suspending && locked.spawning == 0 {
                locked.spawning = locked.spawning.saturating_add(1);
                return Ok(());
            }
            drop(locked);
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[cfg(test)]
    pub(super) fn registered_groups() -> Vec<i32> {
        let Ok(state) = shared_state() else {
            return Vec::new();
        };
        let locked = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        locked.groups.iter().map(|group| group.pgid).collect()
    }

    fn release_launch(state: &Arc<Mutex<State>>) {
        let mut locked = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        locked.spawning = locked.spawning.saturating_sub(1);
    }

    impl Drop for Supervisor {
        fn drop(&mut self) {
            match self.phase {
                Phase::Spawning => {
                    self.reaper.cancel();
                    release_launch(&self.state);
                }
                Phase::Group(_) => {
                    let _ = self.finish();
                }
                Phase::Finished => {}
            }
        }
    }

    fn shared_state() -> Result<Arc<Mutex<State>>, UpstrokeError> {
        match STATE.get_or_init(install) {
            Ok(state) => Ok(Arc::clone(state)),
            Err(message) => Err(UpstrokeError::Agent {
                message: message.clone(),
            }),
        }
    }

    fn install() -> Result<Arc<Mutex<State>>, String> {
        let mut policy = SignalPolicy {
            termination_mask: 0,
            guard_wake_mask: 0,
            stop_mask: 0,
            job_control: false,
        };
        for (signal, bit) in [
            (libc::SIGINT, HANDLE_SIGINT),
            (libc::SIGTERM, HANDLE_SIGTERM),
            (libc::SIGHUP, HANDLE_SIGHUP),
            (libc::SIGQUIT, HANDLE_SIGQUIT),
        ] {
            match disposition(signal)? {
                SignalDisposition::Default => {
                    policy.termination_mask |= bit;
                    policy.guard_wake_mask |= bit;
                }
                SignalDisposition::Custom => policy.guard_wake_mask |= bit,
                SignalDisposition::Ignored => {}
            }
        }
        for (signal, bit) in [
            (libc::SIGTSTP, HANDLE_SIGTSTP),
            (libc::SIGTTIN, HANDLE_SIGTTIN),
            (libc::SIGTTOU, HANDLE_SIGTTOU),
        ] {
            if disposition(signal)? == SignalDisposition::Default {
                policy.stop_mask |= bit;
            }
        }
        let continue_disposition = disposition(libc::SIGCONT)?;
        if policy.stop_mask != 0 && continue_disposition != SignalDisposition::Default {
            return Err(
                "cannot safely proxy default Unix job-control stops while the embedding host owns or ignores SIGCONT"
                    .to_owned(),
            );
        }
        policy.job_control = policy.stop_mask != 0;
        HANDLED_TERMINATION_MASK.store(policy.termination_mask, Ordering::SeqCst);

        let guard = spawn_guard(policy)?;
        let state = Arc::new(Mutex::new(State {
            spawning: 0,
            groups: Vec::new(),
            terminating: false,
            suspending: false,
            guard,
        }));
        let monitored = Arc::clone(&state);
        let (monitor_ready, monitor_started) = std::sync::mpsc::sync_channel(1);
        let monitor = thread::Builder::new()
            .name("upstroke-signal-monitor".to_owned())
            .spawn(move || match prepare_monitor_signal_mask(policy) {
                Ok(()) => {
                    let _ = monitor_ready.send(Ok(()));
                    monitor(monitored)
                }
                Err(error) => {
                    let _ = monitor_ready.send(Err(error));
                }
            });
        let monitor = match monitor {
            Ok(monitor) => monitor,
            Err(error) => {
                guard.abort_setup();
                return Err(format!("starting Unix signal monitor: {error}"));
            }
        };
        match monitor_started.recv() {
            Ok(Ok(())) => drop(monitor),
            Ok(Err(error)) => {
                let _ = monitor.join();
                guard.abort_setup();
                return Err(error);
            }
            Err(error) => {
                let _ = monitor.join();
                guard.abort_setup();
                return Err(format!(
                    "starting Unix signal monitor: readiness channel closed: {error}"
                ));
            }
        }

        for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGQUIT] {
            if policy.handles_termination(signal) {
                install_handler(signal)?;
            }
        }

        if policy.job_control {
            for signal in [libc::SIGTSTP, libc::SIGTTIN, libc::SIGTTOU] {
                if policy.handles_stop(signal) {
                    install_handler(signal)?;
                }
            }
            install_handler(libc::SIGCONT)?;
        }
        Ok(state)
    }

    fn disposition(signal: libc::c_int) -> Result<SignalDisposition, String> {
        // SAFETY: a null `act` queries the current disposition without
        // changing it; `previous` is initialized by a successful call.
        unsafe {
            let mut previous: libc::sigaction = std::mem::zeroed();
            if libc::sigaction(signal, std::ptr::null(), &mut previous) != 0 {
                return Err(format!(
                    "reading Unix signal disposition for signal {signal}: {}",
                    std::io::Error::last_os_error()
                ));
            }
            Ok(if previous.sa_sigaction == libc::SIG_IGN {
                SignalDisposition::Ignored
            } else if previous.sa_sigaction == libc::SIG_DFL {
                SignalDisposition::Default
            } else {
                SignalDisposition::Custom
            })
        }
    }

    fn prepare_monitor_signal_mask(policy: SignalPolicy) -> Result<(), String> {
        if !policy.job_control {
            return Ok(());
        }

        unsafe {
            let mut signals: libc::sigset_t = std::mem::zeroed();
            if libc::sigemptyset(&mut signals) != 0
                || libc::sigaddset(&mut signals, libc::SIGCONT) != 0
            {
                return Err(format!(
                    "building Unix signal-monitor mask: {}",
                    std::io::Error::last_os_error()
                ));
            }
            let result = libc::pthread_sigmask(libc::SIG_UNBLOCK, &signals, std::ptr::null_mut());
            if result != 0 {
                return Err(format!(
                    "unblocking SIGCONT in Unix signal monitor: {}",
                    std::io::Error::from_raw_os_error(result)
                ));
            }
        }
        Ok(())
    }

    fn install_handler(signal: libc::c_int) -> Result<(), String> {
        // SAFETY: `record_signal` has the C ABI and performs only lock-free
        // atomic operations. The empty mask and SA_RESTART keep unrelated
        // syscalls from being exposed to the implementation detail that a
        // monitor thread, rather than the handler, owns process-group work.
        unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            if signal == libc::SIGCONT {
                action.sa_sigaction = record_signal_info as *const () as libc::sighandler_t;
                action.sa_flags = libc::SA_RESTART | libc::SA_SIGINFO;
            } else {
                action.sa_sigaction = record_signal as *const () as libc::sighandler_t;
                action.sa_flags = libc::SA_RESTART;
            }
            libc::sigemptyset(&mut action.sa_mask);
            if libc::sigaction(signal, &action, std::ptr::null_mut()) != 0 {
                return Err(format!(
                    "installing Unix signal forwarding for signal {signal}: {}",
                    std::io::Error::last_os_error()
                ));
            }
        }
        Ok(())
    }

    extern "C" fn record_signal(signal: libc::c_int) {
        match signal {
            libc::SIGTSTP | libc::SIGTTIN | libc::SIGTTOU => {
                SUSPEND_REQUESTED.store(true, Ordering::SeqCst)
            }
            libc::SIGCONT => {
                CONTINUE_REQUESTED.store(true, Ordering::SeqCst);
                notify_guard(signal);
            }
            _ => {
                let _ = PENDING_TERMINATION.compare_exchange(
                    0,
                    signal,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                );
                notify_guard(signal);
            }
        }
    }

    extern "C" fn record_signal_info(
        signal: libc::c_int,
        info: *mut libc::siginfo_t,
        _: *mut libc::c_void,
    ) {
        let is_guard_probe = signal == libc::SIGCONT
            && !info.is_null()
            && unsafe { (*info).si_pid() } == PROBE_PID.load(Ordering::SeqCst);
        if !is_guard_probe {
            record_signal(signal);
            return;
        }

        let already_recorded = PENDING_TERMINATION.load(Ordering::SeqCst);
        if already_recorded != 0 {
            notify_guard(already_recorded);
            return;
        }
        let pending = pending_termination_signal();
        if pending != 0 {
            let _ = PENDING_TERMINATION.compare_exchange(
                0,
                pending,
                Ordering::SeqCst,
                Ordering::SeqCst,
            );
            notify_guard(pending);
            return;
        }
        if unsafe { libc::kill(libc::getpid(), libc::SIGSTOP) } != 0 {
            arm_fail_closed_termination(
                b"upstroke: fail-closed SIGTERM armed: self-stop failed after a guard probe\n",
            );
            notify_guard(libc::SIGTERM);
        }
    }

    fn pending_termination_signal() -> libc::c_int {
        unsafe {
            let mut pending: libc::sigset_t = std::mem::zeroed();
            if libc::sigpending(&mut pending) != 0 {
                return libc::SIGTERM;
            }
            let mask = HANDLED_TERMINATION_MASK.load(Ordering::SeqCst);
            for (signal, bit) in [
                (libc::SIGINT, HANDLE_SIGINT),
                (libc::SIGTERM, HANDLE_SIGTERM),
                (libc::SIGHUP, HANDLE_SIGHUP),
                (libc::SIGQUIT, HANDLE_SIGQUIT),
            ] {
                if mask & bit != 0 && libc::sigismember(&pending, signal) == 1 {
                    return signal;
                }
            }
        }
        0
    }

    extern "C" fn record_guard_signal(signal: libc::c_int) {
        let fd = GUARD_WAKE_FD.load(Ordering::SeqCst);
        if fd < 0 {
            return;
        }
        let byte = u8::try_from(signal).unwrap_or(u8::MAX);
        // SAFETY: the wake descriptor is nonblocking and dedicated to the
        // guard's self-pipe. A full pipe is already readable, so dropping that
        // byte cannot lose the wakeup.
        let _ = unsafe { libc::write(fd, (&byte as *const u8).cast(), 1) };
    }

    fn notify_guard(signal: libc::c_int) {
        if !SUSPEND_ARMED.load(Ordering::SeqCst) {
            return;
        }
        let fd = GUARD_COMMAND_FD.load(Ordering::SeqCst);
        if fd < 0 {
            return;
        }
        let byte = u8::try_from(signal).unwrap_or(u8::MAX);
        // SAFETY: `write` is async-signal-safe, the descriptor is a dedicated
        // pipe, and a one-byte record is atomic. The parent retains a reader so
        // a failed guard cannot turn this write into SIGPIPE.
        let _ = unsafe { libc::write(fd, (&byte as *const u8).cast(), 1) };
    }

    fn arm_fail_closed_termination(site: &[u8]) {
        if PENDING_TERMINATION
            .compare_exchange(0, libc::SIGTERM, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            let _ = write_raw(libc::STDERR_FILENO, site);
        }
    }

    fn monitor(state: Arc<Mutex<State>>) -> ! {
        loop {
            let terminating = PENDING_TERMINATION.load(Ordering::SeqCst);
            if terminating != 0 {
                let Some(groups) = groups_when_registered(&state, true) else {
                    thread::sleep(Duration::from_millis(1));
                    continue;
                };
                signal_groups(&groups, libc::SIGKILL);

                // SAFETY: all isolated children have been synchronously sent
                // SIGKILL. Restore the ordinary terminal semantics and
                // terminate Upstroke with the original signal; `_exit` is a
                // defensive fallback if a platform returns from `raise`.
                unsafe {
                    libc::signal(terminating, libc::SIG_DFL);
                    libc::raise(terminating);
                    libc::_exit(128 + terminating);
                }
            }

            if SUSPEND_REQUESTED.swap(false, Ordering::SeqCst) {
                let Some((groups, guard)) = begin_suspend(&state) else {
                    SUSPEND_REQUESTED.store(true, Ordering::SeqCst);
                    thread::sleep(Duration::from_millis(1));
                    continue;
                };
                if !stop_groups(&groups) {
                    let groups = end_suspend(&state);
                    if PENDING_TERMINATION.load(Ordering::SeqCst) == 0 {
                        signal_groups(&groups, libc::SIGCONT);
                    }
                    continue;
                }

                SUSPEND_ARMED.store(true, Ordering::SeqCst);
                if !guard.arm() {
                    SUSPEND_ARMED.store(false, Ordering::SeqCst);
                    let _ = end_suspend(&state);
                    arm_fail_closed_termination(
                        b"upstroke: fail-closed SIGTERM armed: job-control guard did not acknowledge ARM\n",
                    );
                    continue;
                }

                if PENDING_TERMINATION.load(Ordering::SeqCst) != 0 {
                    SUSPEND_ARMED.store(false, Ordering::SeqCst);
                    guard.disarm();
                    let _ = end_suspend(&state);
                    continue;
                }
                if CONTINUE_REQUESTED.swap(false, Ordering::SeqCst) {
                    SUSPEND_ARMED.store(false, Ordering::SeqCst);
                    guard.disarm();
                    let groups = end_suspend(&state);
                    signal_groups(&groups, libc::SIGCONT);
                    continue;
                }

                match guard.stop_parent() {
                    Some(true) => {}
                    Some(false) => {
                        SUSPEND_ARMED.store(false, Ordering::SeqCst);
                        guard.disarm();
                        let groups = end_suspend(&state);
                        if PENDING_TERMINATION.load(Ordering::SeqCst) == 0 {
                            signal_groups(&groups, libc::SIGCONT);
                        }
                        continue;
                    }
                    None => {
                        SUSPEND_ARMED.store(false, Ordering::SeqCst);
                        guard.disarm();
                        let _ = end_suspend(&state);
                        arm_fail_closed_termination(
                            b"upstroke: fail-closed SIGTERM armed: job-control guard lost the STOP handshake\n",
                        );
                        continue;
                    }
                }
                SUSPEND_ARMED.store(false, Ordering::SeqCst);
                guard.disarm();
                let groups = end_suspend(&state);
                let terminating = PENDING_TERMINATION.load(Ordering::SeqCst) != 0;
                let _ = CONTINUE_REQUESTED.swap(false, Ordering::SeqCst);
                if !terminating {
                    signal_groups(&groups, libc::SIGCONT);
                }
                continue;
            }

            if CONTINUE_REQUESTED.swap(false, Ordering::SeqCst) {
                if let Some(groups) = groups_when_registered(&state, false) {
                    signal_groups(&groups, libc::SIGCONT);
                } else {
                    CONTINUE_REQUESTED.store(true, Ordering::SeqCst);
                }
            }

            thread::sleep(Duration::from_millis(10));
        }
    }

    fn groups_when_registered(
        state: &Arc<Mutex<State>>,
        terminating: bool,
    ) -> Option<GroupSnapshot> {
        let mut locked = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if locked.spawning != 0 {
            return None;
        }
        if terminating {
            locked.terminating = true;
        }
        Some(snapshot_groups(state, &mut locked))
    }

    fn begin_suspend(state: &Arc<Mutex<State>>) -> Option<(GroupSnapshot, Guard)> {
        let mut locked = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if locked.spawning != 0 || locked.suspending || locked.terminating {
            return None;
        }
        locked.suspending = true;
        let guard = locked.guard;
        Some((snapshot_groups(state, &mut locked), guard))
    }

    fn end_suspend(state: &Arc<Mutex<State>>) -> GroupSnapshot {
        let mut locked = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        locked.suspending = false;
        snapshot_groups(state, &mut locked)
    }

    fn snapshot_groups(state: &Arc<Mutex<State>>, locked: &mut State) -> GroupSnapshot {
        let mut pgids = Vec::with_capacity(locked.groups.len());
        for group in &mut locked.groups {
            group.signal_leases = group.signal_leases.saturating_add(1);
            pgids.push(group.pgid);
        }
        GroupSnapshot {
            state: Arc::clone(state),
            pgids,
        }
    }

    fn remove_unpinned_group(state: &mut State, pgid: i32) -> bool {
        let Some(index) = state.groups.iter().position(|group| group.pgid == pgid) else {
            return true;
        };
        if state.groups[index].signal_leases != 0 {
            return false;
        }
        state.groups.swap_remove(index);
        true
    }

    impl Reaper {
        fn register_raw(self, pgid: libc::pid_t) -> bool {
            self.transact_raw(REAPER_REGISTER, pgid) == Some(REAPER_OK)
        }

        fn cleanup(self, pgid: libc::pid_t) -> bool {
            let cleaned = self.transact_raw(REAPER_CLEANUP, pgid) == Some(REAPER_OK);
            self.close_and_wait();
            cleaned
        }

        fn transact_raw(self, operation: u8, pgid: libc::pid_t) -> Option<u8> {
            let mut frame = [0_u8; 5];
            frame[0] = operation;
            frame[1..].copy_from_slice(&pgid.to_ne_bytes());
            if !write_raw(self.command_fd, &frame) {
                return None;
            }
            read_raw_byte(self.ack_fd)
        }

        fn cancel(self) {
            let mut frame = [0_u8; 5];
            frame[0] = REAPER_CANCEL;
            let cancelled = write_raw(self.command_fd, &frame)
                && acknowledged(self.ack_fd, REAPER_OK, Duration::from_secs(2));
            if !cancelled {
                arm_fail_closed_termination(
                    b"upstroke: fail-closed SIGTERM armed: cleanup reaper did not acknowledge CANCEL\n",
                );
                close_fd(self.command_fd);
                close_fd(self.ack_fd);
                close_fd(self._command_keepalive_fd);
                close_fd(self.identity);
                return;
            }
            self.close_and_wait();
        }

        fn abandon(self) -> HelperEnd {
            let killed = signal_helper(self.pid, self.identity, libc::SIGKILL);
            let kill_errno = if killed == 0 { 0 } else { last_errno() };
            let (waited, wait_errno, status) = self.close_and_wait_reporting();
            HelperEnd {
                kill_errno,
                waited,
                wait_errno,
                status,
            }
        }

        fn close_and_wait(self) {
            let _ = self.close_and_wait_reporting();
        }

        fn close_and_wait_reporting(self) -> (libc::pid_t, libc::c_int, libc::c_int) {
            close_fd(self.command_fd);
            close_fd(self.ack_fd);
            close_fd(self._command_keepalive_fd);
            let mut status = 0;
            loop {
                let waited = collect_helper(self.pid, self.identity, &mut status);
                if waited > 0 {
                    close_fd(self.identity);
                    return (waited, 0, status);
                }
                if waited < 0 && !last_errno_is_interrupted() {
                    let errno = last_errno();
                    close_fd(self.identity);
                    return (waited, errno, 0);
                }
            }
        }
    }

    fn spawn_reaper() -> Result<Reaper, String> {
        use std::os::unix::ffi::OsStrExt;

        verify_group_scanner()?;
        let parent = unsafe { libc::getpid() };
        let cleanup_paths = crate::rundir::active_cleanup_lease_paths()
            .into_iter()
            .map(|path| {
                std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(|_| {
                    format!(
                        "run cleanup-lease path contains a null byte: {}",
                        path.display()
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        #[cfg(test)]
        let cleanup_delay_ms = std::env::var("UPSTROKE_TEST_CLEANUP_DELAY_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        #[cfg(not(test))]
        let cleanup_delay_ms = 0;
        #[cfg(test)]
        let ready_delay_ms = std::env::var("UPSTROKE_TEST_REAPER_READY_DELAY_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        #[cfg(not(test))]
        let ready_delay_ms = 0_u64;
        #[cfg(test)]
        let exit_before_ready = std::env::var("UPSTROKE_TEST_HELPER_EXIT_BEFORE_READY")
            .ok()
            .and_then(|value| value.parse::<libc::c_int>().ok());
        #[cfg(not(test))]
        let exit_before_ready: Option<libc::c_int> = None;
        let open_max = unsafe { libc::sysconf(libc::_SC_OPEN_MAX) };
        if open_max <= 0 {
            return Err("reading the Unix open-file descriptor ceiling".to_owned());
        }
        let open_max = libc::c_int::try_from(open_max)
            .map_err(|_| "Unix open-file descriptor ceiling exceeds c_int".to_owned())?;
        let containers = container_scope_for_a_new_reaper();
        let command = create_cloexec_pipe()
            .map_err(|error| format!("creating Unix cleanup-reaper command pipe: {error}"))?;
        let ack = match create_cloexec_pipe() {
            Ok(pipe) => pipe,
            Err(error) => {
                close_fd(command[0]);
                close_fd(command[1]);
                return Err(format!(
                    "creating Unix cleanup-reaper acknowledgement pipe: {error}"
                ));
            }
        };
        // SAFETY: the child immediately enters a fixed-storage syscall-only
        // loop. It never returns to the multithreaded Rust runtime.
        let pid = unsafe { libc::fork() };
        if pid < 0 {
            for fd in [command[0], command[1], ack[0], ack[1]] {
                close_fd(fd);
            }
            return Err(format!(
                "starting Unix cleanup reaper: {}",
                std::io::Error::last_os_error()
            ));
        }
        if pid == 0 {
            if !install_reaper_dispositions() {
                report_setup_failure_and_exit(
                    ack[1],
                    SetupFailure::at(SetupStep::SignalDispositions, last_errno()),
                );
            }
            close_fd(command[1]);
            close_fd(ack[0]);
            if unsafe { libc::setpgid(0, 0) } != 0 {
                report_setup_failure_and_exit(
                    ack[1],
                    SetupFailure::at(SetupStep::OwnProcessGroup, last_errno()),
                );
            }
            close_inherited_fds(&[command[0], ack[1]], open_max);
            if let Err(failure) = lock_cleanup_paths(&cleanup_paths) {
                report_setup_failure_and_exit(ack[1], failure);
            }
            let mut delay_left = ready_delay_ms;
            while delay_left > 0 {
                raw_sleep_10ms();
                delay_left = delay_left.saturating_sub(10);
            }
            if let Some(code) = exit_before_ready {
                unsafe { libc::_exit(code) };
            }
            reaper_loop(
                parent,
                command[0],
                ack[1],
                open_max,
                cleanup_delay_ms,
                containers.as_ref(),
            );
        }

        let identity = match open_helper_identity(pid) {
            Ok(identity) => identity,
            Err(errno) => {
                // This host names processes and this launch could not take the
                // name. Nothing is signalled: closing the command pipe is what
                // the helper reads as the end of its coordinator and it leaves
                // on that, where a `kill` on its number is what taking the name
                // exists to stop. Leaving is not being collected, though, and
                // the wait by number is the half that reaches no stranger.
                for fd in [command[0], command[1], ack[0], ack[1]] {
                    close_fd(fd);
                }
                collect_unnamed_helper(pid);
                return Err(format!(
                    "taking an identity for the Unix cleanup reaper: {}; it was left to end on \
                     its closed command pipe and collected by number, because a helper this \
                     launch cannot name is not one it may signal by number",
                    std::io::Error::from_raw_os_error(errno)
                ));
            }
        };
        close_fd(ack[1]);
        let reaper = Reaper {
            command_fd: command[1],
            ack_fd: ack[0],
            _command_keepalive_fd: command[0],
            pid,
            identity,
        };
        let ready_wait_began = std::time::Instant::now();
        let wait = await_ready(ack[0], REAPER_READY, HELPER_READY_BUDGET);
        if wait != ReadyWait::Ready {
            let waited = ready_wait_began.elapsed();
            let how = describe_ready_wait("reaper", wait, &cleanup_paths);
            let end = describe_helper_end(reaper.abandon());
            return Err(format!(
                "Unix cleanup reaper did not initialize; waited {waited:?} of \
                 {HELPER_READY_BUDGET:?}; descriptor ceiling {open_max}; {how}; ending it: {end}"
            ));
        }
        #[cfg(test)]
        if let Some(path) = std::env::var_os("UPSTROKE_TEST_REAPER_PID_PATH") {
            if let Err(error) = std::fs::write(&path, pid.to_string()) {
                reaper.cancel();
                return Err(format!(
                    "recording test cleanup-reaper pid at {}: {error}",
                    std::path::Path::new(&path).display()
                ));
            }
        }
        Ok(reaper)
    }

    fn install_reaper_dispositions() -> bool {
        unsafe {
            if !scrub_private_helper_dispositions() {
                return false;
            }
            let mut child_action: libc::sigaction = std::mem::zeroed();
            child_action.sa_sigaction = libc::SIG_DFL;
            child_action.sa_flags = 0;
            if libc::sigemptyset(&mut child_action.sa_mask) != 0
                || libc::sigaction(libc::SIGCHLD, &child_action, std::ptr::null_mut()) != 0
            {
                return false;
            }
            for signal in [
                libc::SIGINT,
                libc::SIGTERM,
                libc::SIGHUP,
                libc::SIGQUIT,
                libc::SIGTSTP,
                libc::SIGCONT,
                libc::SIGPIPE,
            ] {
                if libc::signal(signal, libc::SIG_IGN) == libc::SIG_ERR {
                    return false;
                }
            }
            let mut empty: libc::sigset_t = std::mem::zeroed();
            if libc::sigemptyset(&mut empty) != 0
                || libc::sigprocmask(libc::SIG_SETMASK, &empty, std::ptr::null_mut()) != 0
            {
                return false;
            }
        }
        true
    }

    fn lock_cleanup_paths(paths: &[std::ffi::CString]) -> Result<(), SetupFailure> {
        for (index, path) in paths.iter().enumerate() {
            // A report names the lease by position; more than 255 active
            // leases is not a state the conductor produces, and a saturated
            // position reads as "a lease past the ones the message can name".
            let index = u8::try_from(index).unwrap_or(u8::MAX);
            let fd = unsafe { libc::open(path.as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC) };
            if fd < 0 {
                return Err(SetupFailure {
                    step: SetupStep::OpenCleanupLease,
                    index,
                    errno: last_errno(),
                });
            }
            if unsafe { libc::flock(fd, libc::LOCK_SH | libc::LOCK_NB) } != 0 {
                let errno = last_errno();
                close_fd(fd);
                return Err(SetupFailure {
                    step: SetupStep::LockCleanupLease,
                    index,
                    errno,
                });
            }
        }
        Ok(())
    }

    fn reaper_loop(
        parent: libc::pid_t,
        command_fd: libc::c_int,
        ack_fd: libc::c_int,
        open_max: libc::c_int,
        cleanup_delay_ms: u64,
        containers: Option<&ReaperContainers>,
    ) -> ! {
        let mut pgid = 0_i32;
        let mut anchor = 0_i32;
        let mut mirrored_parent_stop = false;
        let mut parent_running_polls = 0_u8;
        if !write_raw(ack_fd, &[REAPER_READY]) {
            unsafe { libc::_exit(1) };
        }
        loop {
            let mut command = libc::pollfd {
                fd: command_fd,
                events: libc::POLLIN | libc::POLLHUP,
                revents: 0,
            };
            let polled = unsafe { libc::poll(&mut command, 1, 10) };
            if polled < 0 {
                if last_errno_is_interrupted() {
                    continue;
                }
                settle_after_coordinator_death(pgid, anchor, cleanup_delay_ms, containers);
                unsafe { libc::_exit(0) };
            }
            if unsafe { libc::getppid() } != parent {
                settle_after_coordinator_death(pgid, anchor, cleanup_delay_ms, containers);
                unsafe { libc::_exit(0) };
            }
            if polled > 0 && command.revents & (libc::POLLERR | libc::POLLNVAL) != 0 {
                settle_after_coordinator_death(pgid, anchor, cleanup_delay_ms, containers);
                unsafe { libc::_exit(0) };
            }
            if polled > 0 && command.revents & (libc::POLLIN | libc::POLLHUP) != 0 {
                let mut frame = [0_u8; 5];
                if !read_raw_exact(command_fd, &mut frame) {
                    settle_after_coordinator_death(pgid, anchor, cleanup_delay_ms, containers);
                    unsafe { libc::_exit(0) };
                }
                let requested = i32::from_ne_bytes([frame[1], frame[2], frame[3], frame[4]]);
                let accepted = match frame[0] {
                    REAPER_REGISTER if pgid == 0 && requested > 0 => {
                        let created = spawn_group_anchor(requested, open_max);
                        if created <= 0 {
                            false
                        } else {
                            pgid = requested;
                            anchor = created;
                            true
                        }
                    }
                    REAPER_CLEANUP if requested == pgid && pgid > 0 => {
                        cleanup_reaper_group(pgid, anchor, cleanup_delay_ms);
                        let _ = write_raw(ack_fd, &[REAPER_OK]);
                        unsafe { libc::_exit(0) };
                    }
                    REAPER_CANCEL if requested == 0 => {
                        if pgid > 0 {
                            cleanup_reaper_group(pgid, anchor, cleanup_delay_ms);
                        }
                        let _ = write_raw(ack_fd, &[REAPER_OK]);
                        unsafe { libc::_exit(0) };
                    }
                    _ => false,
                };
                if !write_raw(ack_fd, &[if accepted { REAPER_OK } else { REAPER_FAIL }]) {
                    settle_after_coordinator_death(pgid, anchor, cleanup_delay_ms, containers);
                    unsafe { libc::_exit(0) };
                }
            }

            if pgid > 0 {
                match process_is_stopped(parent) {
                    Some(true) if !mirrored_parent_stop => {
                        mirrored_parent_stop = unsafe { libc::kill(-pgid, libc::SIGSTOP) } == 0;
                        parent_running_polls = 0;
                    }
                    Some(true) => parent_running_polls = 0,
                    state @ Some(false) if mirrored_parent_stop => {
                        if parent_has_stably_resumed(state, &mut parent_running_polls) {
                            let _ = unsafe { libc::kill(-pgid, libc::SIGCONT) };
                            mirrored_parent_stop = false;
                            parent_running_polls = 0;
                        }
                    }
                    Some(false) => parent_running_polls = 0,
                    None => parent_running_polls = 0,
                }
            }
        }
    }

    fn parent_has_stably_resumed(stopped: Option<bool>, running_polls: &mut u8) -> bool {
        if stopped != Some(false) {
            *running_polls = 0;
            return false;
        }
        *running_polls = running_polls.saturating_add(1);
        *running_polls >= REAPER_RESUME_STABLE_POLLS
    }

    fn spawn_group_anchor(pgid: i32, open_max: libc::c_int) -> libc::pid_t {
        let anchor = unsafe { libc::fork() };
        if anchor < 0 {
            return -1;
        }
        if anchor == 0 {
            if unsafe { libc::setpgid(0, pgid) } != 0 {
                unsafe { libc::_exit(1) };
            }
            close_inherited_fds(&[], open_max);
            unsafe {
                libc::raise(libc::SIGSTOP);
                loop {
                    libc::pause();
                }
            }
        }
        let mut status = 0;
        loop {
            let waited = unsafe { libc::waitpid(anchor, &mut status, libc::WUNTRACED) };
            if waited == anchor {
                if libc::WIFSTOPPED(status) {
                    return anchor;
                }
                return -1;
            }
            if waited < 0 && !last_errno_is_interrupted() {
                return -1;
            }
        }
    }

    fn cleanup_reaper_group(pgid: i32, anchor: libc::pid_t, cleanup_delay_ms: u64) {
        unsafe {
            let _ = libc::kill(-pgid, libc::SIGKILL);
        }
        let mut delay_left = cleanup_delay_ms;
        while delay_left > 0 {
            raw_sleep_10ms();
            delay_left = delay_left.saturating_sub(10);
        }
        while group_has_non_zombie_members(pgid) != Some(false) {
            raw_sleep_10ms();
            unsafe {
                let _ = libc::kill(-pgid, libc::SIGKILL);
            }
        }
        if anchor > 0 {
            loop {
                let waited = unsafe { libc::waitpid(anchor, std::ptr::null_mut(), 0) };
                if waited == anchor || (waited < 0 && !last_errno_is_interrupted()) {
                    break;
                }
            }
        }
    }

    fn write_raw(fd: libc::c_int, bytes: &[u8]) -> bool {
        let mut offset = 0;
        while offset < bytes.len() {
            let written =
                unsafe { libc::write(fd, bytes.as_ptr().add(offset).cast(), bytes.len() - offset) };
            if written > 0 {
                offset += written as usize;
            } else if written < 0 && last_errno_is_interrupted() {
                continue;
            } else {
                return false;
            }
        }
        true
    }

    fn read_raw_exact(fd: libc::c_int, bytes: &mut [u8]) -> bool {
        let mut offset = 0;
        while offset < bytes.len() {
            let read = unsafe {
                libc::read(
                    fd,
                    bytes.as_mut_ptr().add(offset).cast(),
                    bytes.len() - offset,
                )
            };
            if read > 0 {
                offset += read as usize;
            } else if read < 0 && last_errno_is_interrupted() {
                continue;
            } else {
                return false;
            }
        }
        true
    }

    fn read_raw_byte(fd: libc::c_int) -> Option<u8> {
        let mut byte = 0_u8;
        read_raw_exact(fd, std::slice::from_mut(&mut byte)).then_some(byte)
    }

    impl Guard {
        fn abort_setup(self) {
            let _ = GUARD_COMMAND_FD.compare_exchange(
                self.command_fd,
                -1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            );
            PROBE_PID.store(-1, Ordering::SeqCst);
            for fd in [self.command_fd, self.ack_fd, self._command_keepalive_fd] {
                close_fd(fd);
            }
            let _ = signal_helper(self.pid, self.identity, libc::SIGKILL);
            let mut status = 0;
            loop {
                if collect_helper(self.pid, self.identity, &mut status) >= 0
                    || !last_errno_is_interrupted()
                {
                    break;
                }
            }
            close_fd(self.identity);
        }

        fn arm(self) -> bool {
            write_byte(self.command_fd, GUARD_ARM) && self.read_ack() == AckRead::Byte(GUARD_ARM)
        }

        fn stop_parent(self) -> Option<bool> {
            if !write_byte(self.command_fd, GUARD_STOP) {
                return None;
            }
            match read_guard_ack_blocking(self.ack_fd)? {
                GUARD_STOPPED => Some(true),
                GUARD_CANCELLED => Some(false),
                _ => None,
            }
        }

        fn disarm(self) {
            let _ = write_byte(self.command_fd, GUARD_DISARM);
        }

        fn read_ack(self) -> AckRead {
            read_guard_ack(self.ack_fd, HELPER_READY_BUDGET)
        }
    }

    const NO_HELPER_IDENTITY: libc::c_int = -1;

    #[cfg(target_os = "linux")]
    const HELPER_IDENTITY_ENDED: libc::c_int = -2;

    /// The probe's report: what the calls answered, and the errno of a step
    /// that could not be performed at all.
    #[cfg(target_os = "linux")]
    const IDENTITY_REPORT_LEN: usize = 2;

    /// What this host's identity system calls answered where being killed for
    /// asking was survivable, remembered for the life of the process.
    ///
    /// `Err(errno)` is a probe this process could not run or complete: no
    /// descriptor for its report pipe, no child to run it in, no descriptor for
    /// the call under test, no way to take an inherited `SIGCHLD` handler off
    /// the child. Every one of those is a resource or a state of this process
    /// and none is an answer about the host, so none is remembered and the
    /// launch asking fails on it exactly as it fails on any other shortage.
    #[cfg(target_os = "linux")]
    fn established_identity_calls() -> Result<u8, libc::c_int> {
        let remembered = IDENTITY_CALLS.load(Ordering::SeqCst);
        if remembered != 0 {
            return Ok(remembered);
        }
        let answered = probe_identity_calls()?;
        IDENTITY_CALLS.store(answered, Ordering::SeqCst);
        Ok(answered)
    }

    /// Whether the probe found this host answering `what` for a process whose
    /// state it already knew. A call made before any probe has run answers
    /// nothing, which is the reading that keeps an errno from standing as a
    /// fact on its own.
    #[cfg(target_os = "linux")]
    fn identity_calls_answer(what: u8) -> bool {
        IDENTITY_CALLS.load(Ordering::SeqCst) & what != 0
    }

    /// Make every identity system call once, in a forked child, against a
    /// process of that child's own.
    ///
    /// A policy that kills on one of the calls kills the child, and the parent
    /// reads the end of the report pipe rather than dying itself: no report is
    /// a child that never reached its write, and this process then makes none
    /// of the calls at all. A policy that answers one of them falsely is caught
    /// by the answer disagreeing with a fate the child arranged: the target is
    /// the child's own and it is there, so a truthful `pidfd_open` names it, a
    /// truthful `pidfd_send_signal` returns zero, and a truthful `waitid`
    /// collects it. The arguments are the teardown's own, because a filter
    /// selects on arguments.
    #[cfg(target_os = "linux")]
    fn probe_identity_calls() -> Result<u8, libc::c_int> {
        let report =
            create_cloexec_pipe().map_err(|error| error.raw_os_error().unwrap_or(libc::EIO))?;
        // SAFETY: the child makes system calls and `_exit` only. It never
        // returns to the multithreaded Rust runtime.
        let child = unsafe { libc::fork() };
        if child < 0 {
            let errno = last_errno();
            close_fd(report[0]);
            close_fd(report[1]);
            return Err(errno);
        }
        if child == 0 {
            close_fd(report[0]);
            report_identity_calls(report[1]);
        }
        close_fd(report[1]);
        let mut reported = [0_u8; IDENTITY_REPORT_LEN];
        let heard = read_raw_exact(report[0], &mut reported);
        close_fd(report[0]);
        collect_unnamed_helper(child);
        let [answered, blocked] = reported;
        if heard && blocked != 0 {
            return Err(libc::c_int::from(blocked));
        }
        Ok(if heard {
            answered | IDENTITY_PROBED
        } else {
            IDENTITY_PROBED
        })
    }

    /// The probe itself, in a child of this process. Everything here is a
    /// system call or `_exit`, as in every other helper this module forks.
    #[cfg(target_os = "linux")]
    fn report_identity_calls(report_fd: libc::c_int) -> ! {
        let mut report = [0_u8; IDENTITY_REPORT_LEN];
        // An embedding host's `SIGCHLD` disposition is inherited across a fork,
        // and a handler that waits for any child -- or `SIG_IGN`, or
        // `SA_NOCLDWAIT` -- would collect this probe's own target before the
        // collecting call under test reached it, which is the very thing being
        // measured. It comes off this child first.
        if default_child_disposition() {
            // SAFETY: the forked child calls only `_exit`.
            let target = unsafe { libc::fork() };
            if target == 0 {
                // SAFETY: leave the forked child without running destructors.
                unsafe { libc::_exit(0) };
            }
            if target < 0 {
                report[1] = blocked_byte(last_errno());
            } else {
                report = identity_calls_against(target);
            }
        } else {
            report[1] = blocked_byte(last_errno());
        }
        let _ = write_raw(report_fd, &report);
        // SAFETY: leave the probe without running destructors.
        unsafe { libc::_exit(0) };
    }

    /// Put `SIGCHLD` back to its default disposition, dropping whatever handler
    /// and flags this process inherited.
    #[cfg(target_os = "linux")]
    fn default_child_disposition() -> bool {
        // SAFETY: `sigaction` is a plain C aggregate whose all-zero bit pattern
        // is the one the kernel is documented to be handed. `sigemptyset` and
        // `sigaction` read and write through pointers live for their calls, and
        // the null third argument asks for no previous disposition back.
        unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = libc::SIG_DFL;
            action.sa_flags = 0;
            libc::sigemptyset(&mut action.sa_mask) == 0
                && libc::sigaction(libc::SIGCHLD, &action, std::ptr::null_mut()) == 0
        }
    }

    /// The three calls, in the order a launch and its teardown make them,
    /// against `target`: a child of this probe that nothing here has collected,
    /// so each call has a process to answer about and the truthful answer to
    /// each is known before it is made.
    #[cfg(target_os = "linux")]
    fn identity_calls_against(target: libc::pid_t) -> [u8; IDENTITY_REPORT_LEN] {
        let mut answered = 0_u8;
        let mut blocked = 0_u8;
        let named = libc::c_long::from(target);
        let flags: libc::c_long = 0;
        // SAFETY: as in `open_helper_identity`.
        let opened = unsafe { libc::syscall(libc::SYS_pidfd_open, named, flags) };
        let identity = libc::c_int::try_from(opened).unwrap_or(-1);
        if identity < 0 {
            blocked = blocked_byte(last_errno());
        } else {
            answered |= IDENTITY_NAMES;
            let held = libc::c_long::from(identity);
            let number = libc::c_long::from(libc::SIGKILL);
            // SAFETY: as in `signal_helper`, with the signal and the null
            // `siginfo_t` pointer a teardown sends.
            let sent = unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    held,
                    number,
                    std::ptr::null_mut::<libc::siginfo_t>(),
                    flags,
                )
            };
            if sent == 0 {
                answered |= IDENTITY_SIGNALS;
            }
            if let Ok(id) = libc::id_t::try_from(identity) {
                let mut status = 0;
                if collect_through_identity(id, &mut status) >= 0 {
                    answered |= IDENTITY_COLLECTS;
                }
            }
            close_fd(identity);
        }
        if answered & IDENTITY_COLLECTS == 0 {
            // Nothing collected the target through a descriptor, so it is still
            // this probe's own uncollected child and its number is still its
            // own. The wait by number is the one call left that can end it.
            collect_unnamed_helper(target);
        }
        [answered, blocked]
    }

    /// The errno of a probe step that could not be performed, which is this
    /// process being short of something and never an answer about the host;
    /// zero for every other failure, which is an answer. Every errno this can
    /// carry is far below `u8::MAX`, and one that is not is reported as a
    /// blocked probe all the same rather than as a fact about the host.
    #[cfg(target_os = "linux")]
    fn blocked_byte(errno: libc::c_int) -> u8 {
        match errno {
            libc::EMFILE | libc::ENFILE | libc::ENOMEM | libc::EAGAIN | libc::EINVAL => {
                u8::try_from(errno).unwrap_or(u8::MAX)
            }
            _ => 0,
        }
    }

    /// `Err(errno)` is an acquisition that failed for a reason which is not
    /// evidence this host has no identity to give: this process is out of a
    /// resource -- `EMFILE`, `ENFILE`, `ENOMEM` -- on a host whose identities
    /// work. The number is the documented fallback for a platform that cannot
    /// name a process, and a resource this process ran out of is not that, so
    /// the launch fails rather than reaching for the number.
    fn open_helper_identity(pid: libc::pid_t) -> Result<libc::c_int, libc::c_int> {
        #[cfg(target_os = "linux")]
        {
            // Asked before this process makes the call itself, because a policy
            // can make `pidfd_open` fatal rather than refuse it and a process
            // killed for a system call has no branch left to take. What a child
            // established about the three calls decides whether the first of
            // them is made here at all, and what the answers below are worth.
            let established = established_identity_calls()?;
            if established & IDENTITY_NAMES == 0 {
                // Where it was safe to try, the call named nothing: this host
                // does not offer it, or a policy refuses it, or it ended the
                // child that made it. That is the platform row DESIGN §15
                // names, and the number is its documented fallback.
                return Ok(NO_HELPER_IDENTITY);
            }
            let target = libc::c_long::from(pid);
            let flags: libc::c_long = 0;
            // SAFETY: `pidfd_open` takes a pid and a flag word by value and
            // reads through no pointer. It answers a close-on-exec descriptor
            // or a negative number, and touches nothing this process owns.
            let opened = unsafe { libc::syscall(libc::SYS_pidfd_open, target, flags) };
            if opened < 0 {
                let errno = last_errno();
                return match errno {
                    // The kernel answering about this helper: it has ended and
                    // been collected, so its number is free and is exactly what
                    // must not be signalled. Read as the kernel's answer only
                    // because the probe found this call naming a process that
                    // was there; a host where it does not is one this statement
                    // is never reached on.
                    libc::ESRCH => Ok(HELPER_IDENTITY_ENDED),
                    // The call is not one this host offers. A kernel older than
                    // 5.3 answers `ENOSYS`, a kernel without the anonymous inode
                    // filesystem `ENODEV`, a kernel that does not know the flag
                    // word `EINVAL`, and a policy that forbids the call `EPERM`
                    // or `EACCES`. That is the platform row DESIGN §15 names,
                    // and the number is its documented fallback.
                    libc::ENOSYS | libc::ENODEV | libc::EINVAL | libc::EPERM | libc::EACCES => {
                        Ok(NO_HELPER_IDENTITY)
                    }
                    other => Err(other),
                };
            }
            let Ok(identity) = libc::c_int::try_from(opened) else {
                // A descriptor this process cannot name. Not an answer about
                // the helper and not an answer about the host, so not a
                // licence to use the number either.
                return Err(libc::EBADF);
            };
            // The probes choose which path to prefer, never whether the number
            // is safe: `pidfd_open` landed in Linux 5.3, `P_PIDFD` in 5.4, and
            // `pidfd_send_signal` is a separate call a policy can refuse on its
            // own, so a host that cannot use a descriptor for all three is
            // better off not holding one. What the teardown may do is decided
            // by what the teardown's own call answers, in `signal_helper` and
            // `collect_helper`, because a filter selects on arguments and these
            // are not the arguments a teardown passes.
            let probes = [
                identity_can_collect(identity),
                identity_can_signal(identity),
            ];
            if probes.contains(&IdentityProbe::Refused) {
                close_fd(identity);
                return Ok(if probes.contains(&IdentityProbe::HelperEnded) {
                    HELPER_IDENTITY_ENDED
                } else {
                    NO_HELPER_IDENTITY
                });
            }
            Ok(identity)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = pid;
            Ok(NO_HELPER_IDENTITY)
        }
    }

    #[cfg(target_os = "linux")]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum IdentityProbe {
        Available,
        HelperEnded,
        Refused,
    }

    #[cfg(target_os = "linux")]
    fn identity_can_collect(identity: libc::c_int) -> IdentityProbe {
        let Ok(id) = libc::id_t::try_from(identity) else {
            return IdentityProbe::Refused;
        };
        // SAFETY: `siginfo_t` is a plain C aggregate whose all-zero bit pattern
        // is the one `waitid` is documented to be handed.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        // SAFETY: `identity` is the descriptor just opened here and `info` is
        // live for the call. `WNOHANG | WNOWAIT` collects nothing and leaves
        // whatever state the helper is in exactly as it was.
        let answered = unsafe {
            libc::waitid(
                libc::P_PIDFD,
                id,
                &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        };
        if answered == 0 {
            IdentityProbe::Available
        } else if last_errno() == libc::ECHILD {
            IdentityProbe::HelperEnded
        } else {
            IdentityProbe::Refused
        }
    }

    #[cfg(target_os = "linux")]
    fn identity_can_signal(identity: libc::c_int) -> IdentityProbe {
        let target = libc::c_long::from(identity);
        let number: libc::c_long = 0;
        let flags: libc::c_long = 0;
        // SAFETY: as in `signal_helper`, whose call this is with signal `0`:
        // the kernel runs the permission check and delivers nothing.
        let sent = unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                target,
                number,
                std::ptr::null_mut::<libc::siginfo_t>(),
                flags,
            )
        };
        if sent == 0 {
            IdentityProbe::Available
        } else if last_errno() == libc::ESRCH {
            IdentityProbe::HelperEnded
        } else {
            IdentityProbe::Refused
        }
    }

    fn signal_helper(pid: libc::pid_t, identity: libc::c_int, signal: libc::c_int) -> libc::c_int {
        #[cfg(target_os = "linux")]
        if identity == HELPER_IDENTITY_ENDED {
            set_errno(libc::ESRCH);
            return -1;
        }
        #[cfg(target_os = "linux")]
        if identity >= 0 {
            let target = libc::c_long::from(identity);
            let number = libc::c_long::from(signal);
            let flags: libc::c_long = 0;
            // SAFETY: `pidfd_send_signal` takes the descriptor, the signal and
            // the flags by value; the null `siginfo_t` pointer is the form the
            // kernel documents as "as if from kill(2)" and is the only pointer
            // argument it has.
            let sent = unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    target,
                    number,
                    std::ptr::null_mut::<libc::siginfo_t>(),
                    flags,
                )
            };
            if sent == 0 {
                return 0;
            }
            // The call was made and something answered it. `ESRCH` is the
            // answer about this helper -- it has ended and been collected --
            // and the number that named it is another process's now, but only
            // where the probe found this host returning zero from this call for
            // a process that was there. A policy can write `ESRCH` itself, and
            // on a host where it does, this call establishes nothing. Anything
            // else is the call refused for these arguments, which a policy may
            // do for `SIGKILL` while permitting the signal `0` the acquisition
            // probe sent. Either way that leaves this host where a host with no
            // identity at all stands, and the number is what it has.
            if last_errno() == libc::ESRCH && identity_calls_answer(IDENTITY_SIGNALS) {
                return -1;
            }
        }
        #[cfg(not(target_os = "linux"))]
        let _ = identity;
        // SAFETY: `kill` takes both arguments by value and reads no memory of
        // this process.
        unsafe { libc::kill(pid, signal) }
    }

    fn collect_helper(
        pid: libc::pid_t,
        identity: libc::c_int,
        status: &mut libc::c_int,
    ) -> libc::pid_t {
        #[cfg(target_os = "linux")]
        if identity == HELPER_IDENTITY_ENDED {
            set_errno(libc::ECHILD);
            return -1;
        }
        #[cfg(target_os = "linux")]
        if identity >= 0 {
            if let Ok(id) = libc::id_t::try_from(identity) {
                let collected = collect_through_identity(id, status);
                if collected >= 0 {
                    return collected;
                }
                // As in `signal_helper`: `ECHILD` is the kernel answering about
                // this helper where the probe found this host collecting
                // through a descriptor at all, and `EINTR` is this call to make
                // again -- both are answers, and the number stays out of reach.
                // Anything else is the consuming wait refused for these
                // arguments, which a policy may do while permitting the
                // `WNOHANG | WNOWAIT` the acquisition probe passed, or an
                // `ECHILD` a policy wrote itself; the number is then all there
                // is to collect the helper by.
                let errno = last_errno();
                if errno == libc::EINTR
                    || (errno == libc::ECHILD && identity_calls_answer(IDENTITY_COLLECTS))
                {
                    return -1;
                }
            }
        }
        #[cfg(not(target_os = "linux"))]
        let _ = identity;
        // SAFETY: `status` is writable for a `c_int`, and `pid` names a child
        // of this process that nothing else here has collected.
        unsafe { libc::waitpid(pid, status, 0) }
    }

    /// Collect a helper this launch could not name, by number, after its
    /// command pipe has been closed and it has been left to end on that.
    ///
    /// Closing the pipe lets the helper exit; it does not collect it, and a
    /// child nothing collects is one this process keeps for the rest of its
    /// life. The wait by number is safe where a `kill` by number is not:
    /// `waitpid` reaches none but this process's own children, and the helper
    /// is still one of them -- an uncollected child's number is not re-issued,
    /// so either this collects the helper or, where an embedding host's
    /// wildcard wait got there first, it answers `ECHILD` and collects nothing.
    /// Nothing between the two forks a child of this process, so there is no
    /// statement here at which the number could come back as another one.
    fn collect_unnamed_helper(pid: libc::pid_t) -> libc::pid_t {
        let mut status = 0;
        loop {
            // SAFETY: `status` is writable for a `c_int`, and `pid` names a
            // child of this process that nothing here has collected.
            let waited = unsafe { libc::waitpid(pid, &mut status, 0) };
            if waited >= 0 || !last_errno_is_interrupted() {
                return waited;
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn collect_through_identity(identity: libc::id_t, status: &mut libc::c_int) -> libc::pid_t {
        // SAFETY: as in `identity_can_collect`.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        // SAFETY: `identity` is the descriptor taken at this helper's fork and
        // `info` is live for the call.
        let collected = unsafe { libc::waitid(libc::P_PIDFD, identity, &mut info, libc::WEXITED) };
        if collected != 0 {
            return -1;
        }
        // SAFETY: `waitid` returned zero for a `WEXITED` state change, so the
        // SIGCHLD arm of the union `info` carries is the live one.
        let (value, collected_pid) = unsafe { (info.si_status(), info.si_pid()) };
        *status = wait_status_of(info.si_code, value);
        collected_pid
    }

    #[cfg(target_os = "linux")]
    fn wait_status_of(code: libc::c_int, value: libc::c_int) -> libc::c_int {
        const EXIT_SHIFT: libc::c_int = 8;
        const EXIT_MASK: libc::c_int = 0xff;
        const SIGNAL_MASK: libc::c_int = 0x7f;
        const CORE_FLAG: libc::c_int = 0x80;
        match code {
            libc::CLD_EXITED => (value & EXIT_MASK) << EXIT_SHIFT,
            libc::CLD_DUMPED => (value & SIGNAL_MASK) | CORE_FLAG,
            _ => value & SIGNAL_MASK,
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct HelperEnd {
        kill_errno: libc::c_int,
        waited: libc::pid_t,
        wait_errno: libc::c_int,
        status: libc::c_int,
    }

    fn describe_helper_end(end: HelperEnd) -> String {
        let signalled = match end.kill_errno {
            0 => "SIGKILL was delivered".to_owned(),
            libc::ESRCH => "SIGKILL answered ESRCH, so nothing of that number was there".to_owned(),
            errno => format!(
                "SIGKILL failed: {}",
                std::io::Error::from_raw_os_error(errno)
            ),
        };
        let reaped = if end.waited > 0 {
            if libc::WIFSIGNALED(end.status) {
                format!(
                    "and the wait collected it, killed by signal {}",
                    libc::WTERMSIG(end.status)
                )
            } else if libc::WIFEXITED(end.status) {
                format!(
                    "and the wait collected it, having already exited with status {}",
                    libc::WEXITSTATUS(end.status)
                )
            } else {
                format!("and the wait collected it with raw status {}", end.status)
            }
        } else {
            format!(
                "and the wait collected nothing: {}",
                std::io::Error::from_raw_os_error(end.wait_errno)
            )
        };
        format!("{signalled}, {reaped}")
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum SetupStep {
        SignalDispositions,
        OwnProcessGroup,
        OpenCleanupLease,
        LockCleanupLease,
        ForkProbe,
    }

    impl SetupStep {
        fn to_byte(self) -> u8 {
            match self {
                Self::SignalDispositions => 1,
                Self::OwnProcessGroup => 2,
                Self::OpenCleanupLease => 3,
                Self::LockCleanupLease => 4,
                Self::ForkProbe => 5,
            }
        }

        fn from_byte(byte: u8) -> Option<Self> {
            match byte {
                1 => Some(Self::SignalDispositions),
                2 => Some(Self::OwnProcessGroup),
                3 => Some(Self::OpenCleanupLease),
                4 => Some(Self::LockCleanupLease),
                5 => Some(Self::ForkProbe),
                _ => None,
            }
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct SetupFailure {
        step: SetupStep,
        index: u8,
        errno: libc::c_int,
    }

    impl SetupFailure {
        fn at(step: SetupStep, errno: libc::c_int) -> Self {
            Self {
                step,
                index: 0,
                errno,
            }
        }

        fn encode(self) -> [u8; SETUP_FAILURE_FRAME_LEN] {
            let [e0, e1, e2, e3] = self.errno.to_ne_bytes();
            [
                HELPER_ABORT,
                self.step.to_byte(),
                self.index,
                e0,
                e1,
                e2,
                e3,
            ]
        }

        fn decode(frame: [u8; SETUP_FAILURE_FRAME_LEN]) -> Option<Self> {
            let [marker, step, index, e0, e1, e2, e3] = frame;
            if marker != HELPER_ABORT {
                return None;
            }
            Some(Self {
                step: SetupStep::from_byte(step)?,
                index,
                errno: libc::c_int::from_ne_bytes([e0, e1, e2, e3]),
            })
        }
    }

    fn report_setup_failure_and_exit(ack_fd: libc::c_int, failure: SetupFailure) -> ! {
        // Best-effort by design: this process ends with status 1 whether or
        // not the frame arrives, and a parent that does not receive it reports
        // "closed with no report", which `helper_ready_failure_helper` pins.
        let _ = write_raw(ack_fd, &failure.encode());
        unsafe { libc::_exit(1) }
    }

    fn describe_setup_failure(failure: SetupFailure, lease_paths: &[std::ffi::CString]) -> String {
        use std::os::unix::ffi::OsStrExt;

        let lease = || match lease_paths.get(usize::from(failure.index)) {
            Some(path) => std::path::Path::new(std::ffi::OsStr::from_bytes(path.as_bytes()))
                .display()
                .to_string(),
            None => format!("number {}", failure.index),
        };
        let step = match failure.step {
            SetupStep::SignalDispositions => "installing its signal dispositions".to_owned(),
            SetupStep::OwnProcessGroup => "moving into its own process group".to_owned(),
            SetupStep::OpenCleanupLease => format!("opening the cleanup lease {}", lease()),
            SetupStep::LockCleanupLease => {
                format!("taking the shared lock on the cleanup lease {}", lease())
            }
            SetupStep::ForkProbe => "forking its probe".to_owned(),
        };
        format!(
            "{step} failed: {}",
            std::io::Error::from_raw_os_error(failure.errno)
        )
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum AckRead {
        Byte(u8),
        EndOfFile,
        TimedOut,
        Failed(libc::c_int),
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Readiness {
        Readable,
        TimedOut,
        Failed(libc::c_int),
    }

    fn read_guard_ack(fd: libc::c_int, timeout: Duration) -> AckRead {
        let started = std::time::Instant::now();
        loop {
            let remaining = timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return AckRead::TimedOut;
            }
            match wait_readable(fd, remaining) {
                Readiness::Readable => {}
                Readiness::TimedOut => return AckRead::TimedOut,
                Readiness::Failed(libc::EINTR) => continue,
                Readiness::Failed(errno) => return AckRead::Failed(errno),
            }
            let mut ack = 0_u8;
            // SAFETY: `fd` is the dedicated helper-to-parent pipe and `ack` is
            // valid writable storage for exactly one byte.
            let read = unsafe { libc::read(fd, (&mut ack as *mut u8).cast(), 1) };
            if read == 1 {
                return AckRead::Byte(ack);
            }
            if read == 0 {
                return AckRead::EndOfFile;
            }
            let errno = last_errno();
            if errno != libc::EINTR {
                return AckRead::Failed(errno);
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn wait_readable(fd: libc::c_int, timeout: Duration) -> Readiness {
        let timeout_ms = libc::c_int::try_from(timeout.as_millis())
            .unwrap_or(libc::c_int::MAX)
            .max(1);
        let mut poll_fd = libc::pollfd {
            fd,
            events: libc::POLLIN | libc::POLLHUP,
            revents: 0,
        };
        // SAFETY: `poll_fd` is valid storage for exactly one entry for the
        // duration of the call, and the timeout is finite.
        let polled = unsafe { libc::poll(&mut poll_fd, 1, timeout_ms) };
        match polled {
            0 => Readiness::TimedOut,
            ready if ready > 0 => Readiness::Readable,
            _ => Readiness::Failed(last_errno()),
        }
    }

    #[cfg(target_os = "macos")]
    unsafe extern "C" {
        #[link_name = "select$DARWIN_EXTSN"]
        fn select_beyond_fd_setsize(
            nfds: libc::c_int,
            readfds: *mut u32,
            writefds: *mut u32,
            errorfds: *mut u32,
            timeout: *mut libc::timeval,
        ) -> libc::c_int;
    }

    #[cfg(target_os = "macos")]
    fn wait_readable(fd: libc::c_int, timeout: Duration) -> Readiness {
        const BITS_PER_WORD: usize = 32;
        let (Ok(slot), Some(nfds)) = (usize::try_from(fd), fd.checked_add(1)) else {
            return Readiness::Failed(libc::EBADF);
        };
        let mut readfds = vec![0_u32; slot / BITS_PER_WORD + 1];
        if let Some(word) = readfds.get_mut(slot / BITS_PER_WORD) {
            *word |= 1_u32 << (slot % BITS_PER_WORD);
        }
        // `subsec_micros` is below one million, so the second conversion
        // cannot fail; a `Duration` past `time_t` is clamped, not refused.
        let mut remaining = libc::timeval {
            tv_sec: libc::time_t::try_from(timeout.as_secs()).unwrap_or(libc::time_t::MAX),
            tv_usec: libc::suseconds_t::try_from(timeout.subsec_micros()).unwrap_or(0),
        };
        // SAFETY: `readfds` holds `slot / 32 + 1` words, so every bit below
        // `nfds` lies inside it, which is the contract of the unlimited
        // variant; the write and error sets may be null; `remaining` is valid
        // for the call, which may write the time left back into it. The
        // function is `select(2)` under the symbol the header binds when
        // `_DARWIN_UNLIMITED_SELECT` is defined, with the same signature.
        let ready = unsafe {
            select_beyond_fd_setsize(
                nfds,
                readfds.as_mut_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut remaining,
            )
        };
        match ready {
            0 => Readiness::TimedOut,
            ready if ready > 0 => Readiness::Readable,
            _ => Readiness::Failed(last_errno()),
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum ReadyWait {
        Ready,
        SetupFailed(SetupFailure),
        TruncatedReport,
        Unexpected(u8),
        EndOfFile,
        TimedOut,
        Failed(libc::c_int),
    }

    fn await_ready(fd: libc::c_int, ready: u8, budget: Duration) -> ReadyWait {
        let started = std::time::Instant::now();
        match read_guard_ack(fd, budget) {
            AckRead::Byte(byte) if byte == ready => ReadyWait::Ready,
            AckRead::Byte(HELPER_ABORT) => {
                let mut frame = [HELPER_ABORT; SETUP_FAILURE_FRAME_LEN];
                for slot in frame.iter_mut().skip(1) {
                    match read_guard_ack(fd, budget.saturating_sub(started.elapsed())) {
                        AckRead::Byte(byte) => *slot = byte,
                        AckRead::EndOfFile | AckRead::TimedOut | AckRead::Failed(_) => {
                            return ReadyWait::TruncatedReport;
                        }
                    }
                }
                SetupFailure::decode(frame)
                    .map_or(ReadyWait::TruncatedReport, ReadyWait::SetupFailed)
            }
            AckRead::Byte(other) => ReadyWait::Unexpected(other),
            AckRead::EndOfFile => ReadyWait::EndOfFile,
            AckRead::TimedOut => ReadyWait::TimedOut,
            AckRead::Failed(errno) => ReadyWait::Failed(errno),
        }
    }

    fn describe_ready_wait(
        helper: &str,
        wait: ReadyWait,
        lease_paths: &[std::ffi::CString],
    ) -> String {
        match wait {
            ReadyWait::Ready => format!("the {helper} said READY"),
            ReadyWait::SetupFailed(failure) => format!(
                "the {helper} reported that {}",
                describe_setup_failure(failure, lease_paths)
            ),
            ReadyWait::TruncatedReport => format!("the {helper}'s failure report was cut short"),
            ReadyWait::Unexpected(byte) => {
                format!("the acknowledgement pipe carried {byte:#04x} where READY was expected")
            }
            ReadyWait::EndOfFile => "the acknowledgement pipe closed with no report".to_owned(),
            ReadyWait::TimedOut => {
                "the budget elapsed with nothing on the acknowledgement pipe".to_owned()
            }
            ReadyWait::Failed(errno) => format!(
                "waiting on the acknowledgement pipe failed: {}",
                std::io::Error::from_raw_os_error(errno)
            ),
        }
    }

    fn acknowledged(fd: libc::c_int, expected: u8, timeout: Duration) -> bool {
        let started = std::time::Instant::now();
        loop {
            let remaining = timeout.saturating_sub(started.elapsed());
            match read_guard_ack(fd, remaining) {
                AckRead::Byte(byte) if byte == expected => return true,
                AckRead::Byte(_) => continue,
                AckRead::EndOfFile | AckRead::TimedOut | AckRead::Failed(_) => return false,
            }
        }
    }

    fn read_guard_ack_blocking(fd: libc::c_int) -> Option<u8> {
        loop {
            let mut ack = 0_u8;
            // SAFETY: `fd` is the dedicated guard-to-parent pipe and `ack` is
            // valid writable storage for exactly one byte. This intentionally
            // blocks for the whole user-controlled suspension interval.
            let read = unsafe { libc::read(fd, (&mut ack as *mut u8).cast(), 1) };
            if read == 1 {
                return Some(ack);
            }
            if read == 0 {
                return None;
            }
            if std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
                return None;
            }
        }
    }

    fn write_byte(fd: libc::c_int, byte: u8) -> bool {
        loop {
            // SAFETY: `fd` is a dedicated pipe and `byte` remains valid for
            // the duration of the one-byte write.
            let written = unsafe { libc::write(fd, (&byte as *const u8).cast(), 1) };
            if written == 1 {
                return true;
            }
            if written < 0 && last_errno_is_interrupted() {
                continue;
            }
            return false;
        }
    }

    fn spawn_guard(policy: SignalPolicy) -> Result<Guard, String> {
        #[cfg(test)]
        let exit_before_ready = std::env::var("UPSTROKE_TEST_HELPER_EXIT_BEFORE_READY")
            .ok()
            .and_then(|value| value.parse::<libc::c_int>().ok());
        #[cfg(not(test))]
        let exit_before_ready: Option<libc::c_int> = None;
        let open_max = unsafe { libc::sysconf(libc::_SC_OPEN_MAX) };
        if open_max <= 0 {
            return Err("reading the Unix open-file descriptor ceiling".to_owned());
        }
        let open_max = libc::c_int::try_from(open_max)
            .map_err(|_| "Unix open-file descriptor ceiling exceeds c_int".to_owned())?;
        let command = create_cloexec_pipe()
            .map_err(|error| format!("creating Unix job-control command pipe: {error}"))?;
        let ack = match create_cloexec_pipe() {
            Ok(pipe) => pipe,
            Err(error) => {
                for fd in command {
                    close_fd(fd);
                }
                return Err(format!(
                    "creating Unix job-control acknowledgement pipe: {error}"
                ));
            }
        };
        let wake = match create_cloexec_pipe() {
            Ok(pipe) => pipe,
            Err(error) => {
                for fd in [command[0], command[1], ack[0], ack[1]] {
                    close_fd(fd);
                }
                return Err(format!("creating Unix job-control wake pipe: {error}"));
            }
        };
        let probe = match create_cloexec_pipe() {
            Ok(pipe) => pipe,
            Err(error) => {
                for fd in [command[0], command[1], ack[0], ack[1], wake[0], wake[1]] {
                    close_fd(fd);
                }
                return Err(format!("creating Unix job-control probe pipe: {error}"));
            }
        };
        if !set_nonblocking(wake[0]) || !set_nonblocking(wake[1]) {
            for fd in [
                command[0], command[1], ack[0], ack[1], wake[0], wake[1], probe[0], probe[1],
            ] {
                close_fd(fd);
            }
            return Err(format!(
                "creating Unix job-control wake/probe pipe: {}",
                std::io::Error::last_os_error()
            ));
        }
        GUARD_WAKE_FD.store(wake[1], Ordering::SeqCst);

        // SAFETY: the child enters `guard_loop` immediately, which uses only
        // libc syscalls and lock-free atomics after fork. It closes every
        // inherited descriptor except its two pipes before doing any work.
        let pid = unsafe { libc::fork() };
        if pid < 0 {
            GUARD_WAKE_FD.store(-1, Ordering::SeqCst);
            for fd in [
                command[0], command[1], ack[0], ack[1], wake[0], wake[1], probe[0], probe[1],
            ] {
                close_fd(fd);
            }
            return Err(format!(
                "starting Unix job-control guard: {}",
                std::io::Error::last_os_error()
            ));
        }
        if pid == 0 {
            if !install_guard_dispositions(policy) {
                report_setup_failure_and_exit(
                    ack[1],
                    SetupFailure::at(SetupStep::SignalDispositions, last_errno()),
                );
            }
            let parent = unsafe { libc::getppid() };
            let probe_pid = unsafe { libc::fork() };
            if probe_pid < 0 {
                report_setup_failure_and_exit(
                    ack[1],
                    SetupFailure::at(SetupStep::ForkProbe, last_errno()),
                );
            }
            if probe_pid == 0 {
                if !install_probe_dispositions() {
                    unsafe { libc::_exit(1) };
                }
                close_fd(probe[1]);
                close_inherited_fds(&[probe[0]], open_max);
                probe_loop(parent, probe[0]);
            }
            close_fd(command[1]);
            close_fd(ack[0]);
            close_fd(probe[0]);
            close_inherited_fds(&[command[0], ack[1], wake[0], wake[1], probe[1]], open_max);
            if let Some(code) = exit_before_ready {
                unsafe { libc::_exit(code) };
            }
            guard_loop(parent, command[0], ack[1], wake[0], probe[1], probe_pid);
        }

        let identity = open_helper_identity(pid);
        GUARD_WAKE_FD.store(-1, Ordering::SeqCst);
        close_fd(ack[1]);
        close_fd(wake[0]);
        close_fd(wake[1]);
        close_fd(probe[0]);
        close_fd(probe[1]);
        let identity = match identity {
            Ok(identity) => identity,
            Err(errno) => {
                // As in `spawn_reaper`: a helper this launch cannot name is
                // not one it may signal by number, so its command pipe is
                // closed, it ends on that, and the wait by number collects it.
                for fd in [command[0], command[1], ack[0]] {
                    close_fd(fd);
                }
                collect_unnamed_helper(pid);
                return Err(format!(
                    "taking an identity for the Unix job-control guard: {}; it was left to end \
                     on its closed command pipe and was collected by number, because a helper \
                     this launch cannot name is not one it may signal by number",
                    std::io::Error::from_raw_os_error(errno)
                ));
            }
        };
        if !set_nonblocking(command[1]) {
            for fd in [command[0], command[1], ack[0]] {
                close_fd(fd);
            }
            let mut status = 0;
            let _ = signal_helper(pid, identity, libc::SIGKILL);
            let _ = collect_helper(pid, identity, &mut status);
            close_fd(identity);
            return Err("configuring Unix job-control guard descriptors".to_owned());
        }
        let guard = Guard {
            command_fd: command[1],
            ack_fd: ack[0],
            _command_keepalive_fd: command[0],
            pid,
            identity,
        };
        let ready_wait_began = std::time::Instant::now();
        let mut probe_pid_bytes = [0_u8; 4];
        let wait = await_ready(ack[0], GUARD_READY, HELPER_READY_BUDGET);
        let probe_pid_arrived = wait == ReadyWait::Ready
            && read_raw_exact(ack[0], &mut probe_pid_bytes)
            && i32::from_ne_bytes(probe_pid_bytes) > 0;
        if !probe_pid_arrived {
            let waited = ready_wait_began.elapsed();
            let how = if wait == ReadyWait::Ready {
                "READY arrived but the probe pid behind it did not".to_owned()
            } else {
                describe_ready_wait("guard", wait, &[])
            };
            for fd in [command[0], command[1], ack[0]] {
                close_fd(fd);
            }
            let killed = signal_helper(pid, identity, libc::SIGKILL);
            let kill_errno = if killed == 0 { 0 } else { last_errno() };
            let mut status = 0;
            let waited_pid = collect_helper(pid, identity, &mut status);
            let wait_errno = if waited_pid < 0 { last_errno() } else { 0 };
            close_fd(identity);
            let end = describe_helper_end(HelperEnd {
                kill_errno,
                waited: waited_pid,
                wait_errno,
                status: if waited_pid > 0 { status } else { 0 },
            });
            return Err(format!(
                "Unix job-control guard did not initialize; waited {waited:?} of \
                 {HELPER_READY_BUDGET:?}; descriptor ceiling {open_max}; {how}; ending it: {end}"
            ));
        }
        PROBE_PID.store(i32::from_ne_bytes(probe_pid_bytes), Ordering::SeqCst);
        GUARD_COMMAND_FD.store(command[1], Ordering::SeqCst);
        Ok(guard)
    }

    fn guard_loop(
        parent: libc::pid_t,
        command_fd: libc::c_int,
        ack_fd: libc::c_int,
        wake_fd: libc::c_int,
        probe_fd: libc::c_int,
        probe_pid: libc::pid_t,
    ) -> ! {
        let [first, second, third, fourth] = probe_pid.to_ne_bytes();
        let ready = [GUARD_READY, first, second, third, fourth];
        if !write_raw(ack_fd, &ready) {
            unsafe { libc::_exit(1) };
        }
        let mut armed = false;
        let mut stopping = false;
        let mut wake = false;
        let mut buffer = [0_u8; 64];
        loop {
            let mut poll_fds = [
                libc::pollfd {
                    fd: command_fd,
                    events: libc::POLLIN | libc::POLLHUP,
                    revents: 0,
                },
                libc::pollfd {
                    fd: wake_fd,
                    events: libc::POLLIN | libc::POLLHUP,
                    revents: 0,
                },
            ];
            // SAFETY: `poll_fds` is valid storage for exactly the two entries
            // named for the duration of the call, and the timeout is finite.
            let polled = unsafe { libc::poll(poll_fds.as_mut_ptr(), 2, GUARD_POLL_SLICE_MS) };
            if polled < 0 && !last_errno_is_interrupted() {
                unsafe { libc::_exit(1) };
            }
            let [command_poll, wake_poll] = poll_fds;
            if polled == 0 {
                if unsafe { libc::getppid() } != parent {
                    unsafe { libc::_exit(0) };
                }
                if armed && stopping && !write_byte(probe_fd, GUARD_PROBE) {
                    unsafe { libc::_exit(0) };
                }
                continue;
            }
            if polled > 0 && command_poll.revents & (libc::POLLIN | libc::POLLHUP) != 0 {
                // SAFETY: `buffer` is valid writable storage and command_fd is
                // the guard's private read end.
                let count =
                    unsafe { libc::read(command_fd, buffer.as_mut_ptr().cast(), buffer.len()) };
                if count <= 0 {
                    unsafe { libc::_exit(0) };
                }
                let read = usize::try_from(count).unwrap_or(0);
                for byte in buffer.iter().copied().take(read) {
                    match byte {
                        GUARD_ARM => {
                            wake = false;
                            stopping = false;
                            drain_pipe(wake_fd);
                            armed = true;
                            let _ =
                                unsafe { libc::write(ack_fd, (&GUARD_ARM as *const u8).cast(), 1) };
                        }
                        GUARD_STOP => {
                            if !armed || wake {
                                let _ = write_byte(ack_fd, GUARD_CANCELLED);
                                armed = false;
                                stopping = false;
                                wake = false;
                                continue;
                            }
                            if unsafe { libc::getppid() } != parent {
                                unsafe { libc::_exit(0) };
                            }
                            if unsafe { libc::kill(parent, libc::SIGSTOP) } != 0 {
                                unsafe { libc::_exit(0) };
                            }
                            stopping = true;
                        }
                        GUARD_DISARM => {
                            armed = false;
                            stopping = false;
                            wake = false;
                            drain_pipe(wake_fd);
                        }
                        _ => wake = true,
                    }
                }
            }
            if polled > 0 && wake_poll.revents & (libc::POLLIN | libc::POLLHUP) != 0 {
                drain_pipe(wake_fd);
                wake = true;
            }
            if armed && stopping && wake {
                if unsafe { libc::getppid() } != parent {
                    unsafe { libc::_exit(0) };
                }
                // SAFETY: a positive pid targets only the Upstroke parent. A
                // generated SIGCONT resumes it even while blocked or caught.
                if unsafe { libc::kill(parent, libc::SIGCONT) } != 0 {
                    unsafe { libc::_exit(0) };
                }
                if !write_byte(ack_fd, GUARD_STOPPED) {
                    unsafe { libc::_exit(0) };
                }
                armed = false;
                stopping = false;
                wake = false;
            }
        }
    }

    fn scrub_private_helper_dispositions() -> bool {
        for signal in 1..=128 {
            if signal == libc::SIGKILL || signal == libc::SIGSTOP {
                continue;
            }
            let synchronous = matches!(
                signal,
                libc::SIGILL
                    | libc::SIGABRT
                    | libc::SIGFPE
                    | libc::SIGSEGV
                    | libc::SIGBUS
                    | libc::SIGTRAP
                    | libc::SIGSYS
            );
            let disposition = if synchronous {
                libc::SIG_DFL
            } else {
                libc::SIG_IGN
            };
            if !set_signal_disposition(signal, disposition) {
                return false;
            }
        }
        true
    }

    fn set_signal_disposition(signal: libc::c_int, disposition: libc::sighandler_t) -> bool {
        unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = disposition;
            action.sa_flags = 0;
            if libc::sigemptyset(&mut action.sa_mask) != 0 {
                return false;
            }
            if libc::sigaction(signal, &action, std::ptr::null_mut()) == 0 {
                true
            } else {
                last_errno() == libc::EINVAL
            }
        }
    }

    fn install_probe_dispositions() -> bool {
        unsafe {
            if !scrub_private_helper_dispositions() {
                return false;
            }
            let mut empty: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut empty) == 0
                && libc::sigprocmask(libc::SIG_SETMASK, &empty, std::ptr::null_mut()) == 0
        }
    }

    fn probe_loop(parent: libc::pid_t, command_fd: libc::c_int) -> ! {
        loop {
            let mut command = 0_u8;
            let read = unsafe { libc::read(command_fd, (&mut command as *mut u8).cast(), 1) };
            if read == 1 && command == GUARD_PROBE {
                if unsafe { libc::kill(parent, libc::SIGCONT) } == 0 {
                    continue;
                }
            } else if read < 0 && last_errno_is_interrupted() {
                continue;
            }
            unsafe { libc::_exit(0) };
        }
    }

    fn install_guard_dispositions(policy: SignalPolicy) -> bool {
        unsafe {
            if !scrub_private_helper_dispositions() {
                return false;
            }
            let mut empty: libc::sigset_t = std::mem::zeroed();
            if libc::sigemptyset(&mut empty) != 0
                || libc::sigprocmask(libc::SIG_SETMASK, &empty, std::ptr::null_mut()) != 0
            {
                return false;
            }
            if policy.job_control {
                if libc::signal(libc::SIGTSTP, libc::SIG_IGN) == libc::SIG_ERR
                    || libc::signal(
                        libc::SIGCONT,
                        record_guard_signal as *const () as libc::sighandler_t,
                    ) == libc::SIG_ERR
                {
                    return false;
                }
            } else {
                if libc::signal(libc::SIGTSTP, libc::SIG_IGN) == libc::SIG_ERR
                    || libc::signal(libc::SIGCONT, libc::SIG_IGN) == libc::SIG_ERR
                {
                    return false;
                }
            }
            for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGQUIT] {
                if policy.wakes_guard(signal)
                    && libc::signal(
                        signal,
                        record_guard_signal as *const () as libc::sighandler_t,
                    ) == libc::SIG_ERR
                {
                    return false;
                }
            }
        }
        true
    }

    fn drain_pipe(fd: libc::c_int) {
        let mut buffer = [0_u8; 64];
        loop {
            // SAFETY: `buffer` is writable for its complete length and `fd` is
            // the nonblocking read side of the guard's private wake pipe.
            if unsafe { libc::read(fd, buffer.as_mut_ptr().cast(), buffer.len()) } <= 0 {
                return;
            }
        }
    }

    fn close_inherited_fds(keep: &[libc::c_int], open_max: libc::c_int) {
        #[cfg(target_os = "linux")]
        if close_ranges_except(keep) {
            return;
        }
        for fd in 0..open_max {
            if !keep.contains(&fd) {
                close_fd(fd);
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn close_ranges_except(keep: &[libc::c_int]) -> bool {
        let mut first = 0_u32;
        loop {
            let next_keep = keep
                .iter()
                .copied()
                .filter(|fd| *fd >= 0 && (*fd as u32) >= first)
                .map(|fd| fd as u32)
                .min();
            if next_keep != Some(first) {
                let last = next_keep.map_or(u32::MAX, |fd| fd - 1);
                let result = unsafe { libc::syscall(libc::SYS_close_range, first, last, 0_u32) };
                if result != 0 {
                    return false;
                }
            }
            let Some(kept) = next_keep else {
                return true;
            };
            first = kept + 1;
        }
    }

    fn last_errno_is_interrupted() -> bool {
        last_errno() == libc::EINTR
    }

    #[cfg(target_os = "linux")]
    fn set_errno(errno: libc::c_int) {
        // SAFETY: `__errno_location` answers a pointer to this thread's own
        // `errno`, which is live for the whole thread and is the slot
        // `last_errno` reads back.
        unsafe { *libc::__errno_location() = errno };
    }

    fn last_errno() -> libc::c_int {
        #[cfg(target_os = "linux")]
        unsafe {
            *libc::__errno_location()
        }
        #[cfg(target_os = "macos")]
        unsafe {
            *libc::__error()
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
        }
    }

    fn raw_sleep_10ms() {
        let request = libc::timespec {
            tv_sec: 0,
            tv_nsec: 10_000_000,
        };
        let mut remaining = request;
        loop {
            let result = unsafe { libc::nanosleep(&remaining, &mut remaining) };
            if result == 0 || !last_errno_is_interrupted() {
                return;
            }
        }
    }

    fn verify_group_scanner() -> Result<(), String> {
        let own_group = unsafe { libc::getpgrp() };
        let own_pid = unsafe { libc::getpid() };
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            match (
                group_has_non_zombie_members(own_group),
                process_is_stopped(own_pid),
            ) {
                (Some(true), Some(false)) => return Ok(()),
                (Some(false), _) => {
                    return Err(format!(
                        "Unix process-group scanner did not find the current group {own_group}"
                    ));
                }
                (Some(true), Some(true)) => {
                    return Err(
                        "Unix parent-state scanner reported the running Upstroke process as stopped"
                            .to_owned(),
                    );
                }
                _ if std::time::Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(1));
                }
                _ => break,
            }
        }
        Err("Unix process-group scanner is unavailable; refusing to launch an agent whose cleanup could not be verified".to_owned())
    }

    #[cfg(target_os = "linux")]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum LinuxStatSnapshot {
        Present { pgid: i32, state: u8 },
        Vanished,
        Invalid,
    }

    #[cfg(target_os = "linux")]
    fn group_has_non_zombie_members(pgid: i32) -> Option<bool> {
        let directory = unsafe {
            libc::open(
                c"/proc".as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
            )
        };
        if directory < 0 {
            return None;
        }
        let mut entries = [0_u8; 16_384];
        loop {
            let count = unsafe {
                libc::syscall(
                    libc::SYS_getdents64,
                    directory,
                    entries.as_mut_ptr(),
                    entries.len(),
                )
            };
            if count == 0 {
                close_fd(directory);
                return Some(false);
            }
            if count < 0 {
                if last_errno_is_interrupted() {
                    continue;
                }
                close_fd(directory);
                return None;
            }
            let mut offset = 0_usize;
            while offset < count as usize {
                if offset + 19 > count as usize {
                    close_fd(directory);
                    return None;
                }
                let record_len =
                    u16::from_ne_bytes([entries[offset + 16], entries[offset + 17]]) as usize;
                if record_len < 20 || offset + record_len > count as usize {
                    close_fd(directory);
                    return None;
                }
                let name = &entries[offset + 19..offset + record_len];
                let name_len = name
                    .iter()
                    .position(|byte| *byte == 0)
                    .unwrap_or(name.len());
                if let Some(pid) = parse_decimal(&name[..name_len]) {
                    match read_linux_stat_raw(pid) {
                        LinuxStatSnapshot::Present {
                            pgid: candidate,
                            state,
                        } if candidate == pgid && !matches!(state, b'Z' | b'X' | b'x') => {
                            close_fd(directory);
                            return Some(true);
                        }
                        LinuxStatSnapshot::Present { .. } | LinuxStatSnapshot::Vanished => {}
                        LinuxStatSnapshot::Invalid => {
                            close_fd(directory);
                            return None;
                        }
                    }
                }
                offset += record_len;
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn read_linux_stat_raw(pid: i32) -> LinuxStatSnapshot {
        let mut path = [0_u8; 64];
        let prefix = b"/proc/";
        path[..prefix.len()].copy_from_slice(prefix);
        let mut end = prefix.len();
        let Some(written) = write_decimal(pid, &mut path[end..]) else {
            return LinuxStatSnapshot::Invalid;
        };
        end += written;
        let suffix = b"/stat\0";
        let Some(target) = path.get_mut(end..end + suffix.len()) else {
            return LinuxStatSnapshot::Invalid;
        };
        target.copy_from_slice(suffix);
        let fd = unsafe { libc::open(path.as_ptr().cast(), libc::O_RDONLY | libc::O_CLOEXEC) };
        if fd < 0 {
            return if matches!(last_errno(), libc::ENOENT | libc::ESRCH) {
                LinuxStatSnapshot::Vanished
            } else {
                LinuxStatSnapshot::Invalid
            };
        }
        let mut stat = [0_u8; 2_048];
        let count = loop {
            let read = unsafe { libc::read(fd, stat.as_mut_ptr().cast(), stat.len()) };
            if read < 0 && last_errno_is_interrupted() {
                continue;
            }
            break read;
        };
        let read_errno = (count < 0).then(last_errno);
        close_fd(fd);
        if matches!(read_errno, Some(libc::ENOENT | libc::ESRCH)) {
            return LinuxStatSnapshot::Vanished;
        }
        if count <= 0 {
            return LinuxStatSnapshot::Invalid;
        }
        parse_linux_stat_bytes(&stat[..count as usize])
            .map(|(pgid, state)| LinuxStatSnapshot::Present { pgid, state })
            .unwrap_or(LinuxStatSnapshot::Invalid)
    }

    #[cfg(target_os = "linux")]
    fn process_is_stopped(pid: i32) -> Option<bool> {
        match read_linux_stat_raw(pid) {
            LinuxStatSnapshot::Present { state, .. } => Some(matches!(state, b'T' | b't')),
            LinuxStatSnapshot::Vanished | LinuxStatSnapshot::Invalid => None,
        }
    }

    #[cfg(target_os = "linux")]
    fn parse_linux_stat_bytes(stat: &[u8]) -> Option<(i32, u8)> {
        let close = stat.iter().rposition(|byte| *byte == b')')?;
        let mut fields = stat.get(close + 1..)?;
        fields = trim_ascii_start(fields);
        let state = *fields.first()?;
        fields = next_ascii_field(fields)?.1;
        let (parent, tail) = next_ascii_field(fields)?;
        parse_decimal(parent)?;
        let (group, _) = next_ascii_field(tail)?;
        Some((parse_decimal(group)?, state))
    }

    #[cfg(target_os = "linux")]
    fn next_ascii_field(bytes: &[u8]) -> Option<(&[u8], &[u8])> {
        let bytes = trim_ascii_start(bytes);
        let end = bytes
            .iter()
            .position(u8::is_ascii_whitespace)
            .unwrap_or(bytes.len());
        (end != 0).then_some((&bytes[..end], &bytes[end..]))
    }

    #[cfg(target_os = "linux")]
    fn trim_ascii_start(mut bytes: &[u8]) -> &[u8] {
        while bytes.first().is_some_and(u8::is_ascii_whitespace) {
            bytes = &bytes[1..];
        }
        bytes
    }

    #[cfg(target_os = "linux")]
    fn parse_decimal(bytes: &[u8]) -> Option<i32> {
        if bytes.is_empty() {
            return None;
        }
        let mut value = 0_i32;
        for byte in bytes {
            if !byte.is_ascii_digit() {
                return None;
            }
            value = value.checked_mul(10)?.checked_add(i32::from(byte - b'0'))?;
        }
        Some(value)
    }

    #[cfg(target_os = "linux")]
    fn write_decimal(value: i32, output: &mut [u8]) -> Option<usize> {
        if value <= 0 {
            return None;
        }
        let mut reversed = [0_u8; 10];
        let mut count = 0_usize;
        let mut value = value as u32;
        while value != 0 {
            reversed[count] = b'0' + (value % 10) as u8;
            count += 1;
            value /= 10;
        }
        if output.len() < count {
            return None;
        }
        for index in 0..count {
            output[index] = reversed[count - index - 1];
        }
        Some(count)
    }

    #[cfg(any(target_os = "macos", test))]
    fn listed_pid_bytes(returned: i32, errno: i32, buffer_bytes: usize) -> Option<usize> {
        let listed = usize::try_from(returned).ok()?;
        if listed == 0 && errno != 0 {
            return None;
        }
        (listed < buffer_bytes).then_some(listed)
    }

    #[cfg(target_os = "macos")]
    fn group_has_non_zombie_members(pgid: i32) -> Option<bool> {
        const PROC_PGRP_ONLY: u32 = 2;
        const MAX_PIDS: usize = 16_384;
        let mut pids = [0_i32; MAX_PIDS];
        let buffer_bytes = std::mem::size_of_val(&pids);
        // SAFETY: `__error` is Darwin's per-thread `errno` location. It is
        // cleared here and read below with no intervening call, because a zero
        // return from `proc_listpids` is only readable as an empty group when
        // this call is the one that left `errno` where it is.
        unsafe { *libc::__error() = 0 };
        let returned = unsafe {
            libc::proc_listpids(
                PROC_PGRP_ONLY,
                pgid as u32,
                pids.as_mut_ptr().cast(),
                buffer_bytes as libc::c_int,
            )
        };
        // SAFETY: as above; nothing between the two calls touches `errno`.
        let errno = unsafe { *libc::__error() };
        let bytes = listed_pid_bytes(returned, errno, buffer_bytes)?;
        let count = bytes / std::mem::size_of::<libc::pid_t>();
        for pid in &pids[..count] {
            if *pid <= 0 {
                continue;
            }
            let mut info: libc::proc_bsdshortinfo = unsafe { std::mem::zeroed() };
            let read = unsafe {
                libc::proc_pidinfo(
                    *pid,
                    libc::PROC_PIDT_SHORTBSDINFO,
                    1,
                    (&mut info as *mut libc::proc_bsdshortinfo).cast(),
                    std::mem::size_of::<libc::proc_bsdshortinfo>() as libc::c_int,
                )
            };
            if read != std::mem::size_of::<libc::proc_bsdshortinfo>() as libc::c_int {
                return None;
            }
            if info.pbsi_pgid == pgid as u32 && info.pbsi_status != libc::SZOMB {
                return Some(true);
            }
        }
        Some(false)
    }

    #[cfg(target_os = "macos")]
    fn process_is_stopped(pid: i32) -> Option<bool> {
        let mut info: libc::proc_bsdshortinfo = unsafe { std::mem::zeroed() };
        let read = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDT_SHORTBSDINFO,
                0,
                (&mut info as *mut libc::proc_bsdshortinfo).cast(),
                std::mem::size_of::<libc::proc_bsdshortinfo>() as libc::c_int,
            )
        };
        (read == std::mem::size_of::<libc::proc_bsdshortinfo>() as libc::c_int)
            .then_some(info.pbsi_status == libc::SSTOP)
    }

    #[cfg(target_os = "linux")]
    fn create_cloexec_pipe() -> Result<[libc::c_int; 2], std::io::Error> {
        let mut pipe = [-1; 2];
        // SAFETY: `pipe` exposes storage for exactly two descriptors. `pipe2`
        // applies CLOEXEC in the same kernel operation that publishes them, so
        // a concurrent spawn can never inherit an intermediate descriptor.
        if unsafe { libc::pipe2(pipe.as_mut_ptr(), libc::O_CLOEXEC) } == 0 {
            Ok(pipe)
        } else {
            Err(std::io::Error::last_os_error())
        }
    }

    #[cfg(target_os = "macos")]
    fn create_cloexec_pipe() -> Result<[libc::c_int; 2], std::io::Error> {
        use std::os::unix::ffi::OsStrExt;

        let template = std::env::temp_dir().join(format!(".upstroke-pipe-{}-XXXXXX", unsafe {
            libc::getpid()
        }));
        let mut template = std::ffi::CString::new(template.as_os_str().as_bytes())
            .map_err(|_| std::io::Error::from_raw_os_error(libc::EINVAL))?
            .into_bytes_with_nul();
        if unsafe { libc::mkdtemp(template.as_mut_ptr().cast()) }.is_null() {
            return Err(std::io::Error::last_os_error());
        }

        let directory_len = template.len().saturating_sub(1);
        let mut fifo = Vec::with_capacity(directory_len + b"/channel\0".len());
        fifo.extend_from_slice(&template[..directory_len]);
        fifo.extend_from_slice(b"/channel\0");
        let cleanup = || unsafe {
            let _ = libc::unlink(fifo.as_ptr().cast());
            let _ = libc::rmdir(template.as_ptr().cast());
        };
        if unsafe { libc::mkfifo(fifo.as_ptr().cast(), 0o600) } != 0 {
            let error = std::io::Error::last_os_error();
            cleanup();
            return Err(error);
        }

        let read_fd = unsafe {
            libc::open(
                fifo.as_ptr().cast(),
                libc::O_RDONLY | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        };
        if read_fd < 0 {
            let error = std::io::Error::last_os_error();
            cleanup();
            return Err(error);
        }
        let write_fd = unsafe {
            libc::open(
                fifo.as_ptr().cast(),
                libc::O_WRONLY | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        };
        if write_fd < 0 {
            let error = std::io::Error::last_os_error();
            close_fd(read_fd);
            cleanup();
            return Err(error);
        }
        let unlinked = unsafe { libc::unlink(fifo.as_ptr().cast()) } == 0;
        let removed = unsafe { libc::rmdir(template.as_ptr().cast()) } == 0;
        if !unlinked || !removed || !clear_nonblocking(read_fd) || !clear_nonblocking(write_fd) {
            let error = std::io::Error::last_os_error();
            close_fd(read_fd);
            close_fd(write_fd);
            return Err(error);
        }
        Ok([read_fd, write_fd])
    }

    #[cfg(target_os = "macos")]
    fn clear_nonblocking(fd: libc::c_int) -> bool {
        unsafe {
            let flags = libc::fcntl(fd, libc::F_GETFL);
            flags >= 0 && libc::fcntl(fd, libc::F_SETFL, flags & !libc::O_NONBLOCK) == 0
        }
    }

    fn set_nonblocking(fd: libc::c_int) -> bool {
        unsafe {
            let flags = libc::fcntl(fd, libc::F_GETFL);
            if flags >= 0 {
                return libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) == 0;
            }
        }
        false
    }

    fn close_fd(fd: libc::c_int) {
        if fd >= 0 {
            // SAFETY: callers transfer ownership of each raw descriptor here.
            let _ = unsafe { libc::close(fd) };
        }
    }

    fn signal_groups(groups: &[i32], signal: libc::c_int) {
        for pgid in groups {
            // SAFETY: every registered child was created with
            // `process_group(0)`, so its pid is its private group id. A
            // negative id targets that group and never Upstroke's group.
            let _ = unsafe { libc::kill(-*pgid, signal) };
        }
    }

    fn stop_groups(groups: &[i32]) -> bool {
        signal_groups(groups, libc::SIGSTOP);
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            if groups_are_quiescent(groups) {
                return true;
            }
            if std::time::Instant::now() >= deadline
                || PENDING_TERMINATION.load(Ordering::SeqCst) != 0
                || CONTINUE_REQUESTED.load(Ordering::SeqCst)
            {
                return false;
            }
            thread::sleep(Duration::from_millis(1));
        }
    }

    #[cfg(target_os = "linux")]
    fn groups_are_quiescent(groups: &[i32]) -> bool {
        if groups.is_empty() {
            return true;
        }
        let entries = match std::fs::read_dir("/proc") {
            Ok(entries) => entries,
            Err(_) => return false,
        };
        let mut observed = vec![false; groups.len()];
        for entry in entries {
            let Ok(entry) = entry else {
                return false;
            };
            if entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<u32>().ok())
                .is_none()
            {
                continue;
            }
            let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
                continue;
            };
            let Some((pgid, state)) = parse_linux_process_stat(&stat) else {
                return false;
            };
            let Some(index) = groups.iter().position(|candidate| *candidate == pgid) else {
                continue;
            };
            observed[index] = true;
            if !matches!(state, b'T' | b't' | b'Z' | b'X' | b'x') {
                return false;
            }
        }
        quiescent_snapshot_is_complete(groups, &observed)
    }

    #[cfg(target_os = "linux")]
    fn parse_linux_process_stat(stat: &str) -> Option<(i32, u8)> {
        let tail = stat.get(stat.rfind(')')? + 1..)?.trim_start();
        let mut fields = tail.split_whitespace();
        let state = *fields.next()?.as_bytes().first()?;
        let _parent_pid = fields.next()?.parse::<i32>().ok()?;
        let process_group = fields.next()?.parse::<i32>().ok()?;
        Some((process_group, state))
    }

    #[cfg(not(target_os = "linux"))]
    fn groups_are_quiescent(groups: &[i32]) -> bool {
        if groups.is_empty() {
            return true;
        }
        let output = match std::process::Command::new("/bin/ps")
            .args(["-axo", "pgid=,stat="])
            .env("LC_ALL", "C")
            .output()
        {
            Ok(output) if output.status.success() => output,
            _ => return false,
        };
        let listing = String::from_utf8_lossy(&output.stdout);
        let mut observed = vec![false; groups.len()];
        for line in listing.lines() {
            let mut fields = line.split_whitespace();
            let Some(pgid) = fields.next().and_then(|field| field.parse::<i32>().ok()) else {
                continue;
            };
            let Some(index) = groups.iter().position(|candidate| *candidate == pgid) else {
                continue;
            };
            observed[index] = true;
            let Some(state) = fields.next().and_then(|field| field.as_bytes().first()) else {
                return false;
            };
            if !matches!(*state, b'T' | b'Z' | b'X') {
                return false;
            }
        }
        quiescent_snapshot_is_complete(groups, &observed)
    }

    fn quiescent_snapshot_is_complete(groups: &[i32], observed: &[bool]) -> bool {
        for (index, pgid) in groups.iter().enumerate() {
            if observed[index] {
                continue;
            }
            if unsafe { libc::kill(-*pgid, 0) } == 0
                || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
            {
                return false;
            }
        }
        true
    }

    struct ReaperContainers {
        program: std::ffi::CString,
        _ps: Vec<std::ffi::CString>,
        ps_argv: Vec<*const libc::c_char>,
    }

    static CONTAINER_SCOPE: OnceLock<
        Mutex<Option<crate::runner::container::census::ReaperContainerScope>>,
    > = OnceLock::new();

    const REAPER_PS_BUFFER: usize = 8192;

    const REAPER_DOCKER_TICKS: usize = 3_000;

    pub(super) fn set_container_reclaim_scope(
        scope: Option<&crate::runner::container::census::ReaperContainerScope>,
    ) -> Result<(), UpstrokeError> {
        if let Some(scope) = scope {
            render_container_argv(scope)?;
        }
        let mut held = CONTAINER_SCOPE
            .get_or_init(|| Mutex::new(None))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *held = scope.cloned();
        Ok(())
    }

    fn resolve_reaper_program(program: &std::path::Path) -> Result<PathBuf, UpstrokeError> {
        use std::os::unix::ffi::OsStrExt as _;
        if program.as_os_str().as_bytes().contains(&b'/') {
            return Ok(program.to_path_buf());
        }
        let refused = |why: &str| UpstrokeError::Refused {
            message: format!(
                "the Unix reaper's container scope names the program `{}`, which {why}; a reaper \
                 is a fork-only child restricted to async-signal-safe calls, so it must be handed \
                 an already-resolved path — `execv` does not search `PATH` and `execvp` is not \
                 async-signal-safe — and a scope whose program cannot be resolved is refused here \
                 rather than silently reclaiming nothing",
                program.display()
            ),
        };
        let name = program.to_str().ok_or_else(|| {
            refused("carries no separator and is not UTF-8, so it cannot be looked up on `PATH`")
        })?;
        crate::util::find_program(name)
            .ok_or_else(|| refused("carries no separator and is not on this process's `PATH`"))
    }

    fn render_container_argv(
        scope: &crate::runner::container::census::ReaperContainerScope,
    ) -> Result<ReaperContainers, UpstrokeError> {
        let nul = |value: &str| UpstrokeError::Refused {
            message: format!(
                "the Unix reaper's container scope renders `{value}`, which carries an interior \
                 NUL and cannot be an argument to `{}`",
                scope.program().display()
            ),
        };
        let resolved = resolve_reaper_program(scope.program())?;
        let program = std::ffi::CString::new(resolved.as_os_str().as_encoded_bytes())
            .map_err(|_| nul(&resolved.to_string_lossy()))?;
        let mut ps = Vec::new();
        for (index, argument) in scope.list_argv().into_iter().enumerate() {
            let argument = if index == 0 {
                resolved.to_string_lossy().into_owned()
            } else {
                argument
            };
            ps.push(std::ffi::CString::new(argument.clone()).map_err(|_| nul(&argument))?);
        }
        let mut ps_argv: Vec<*const libc::c_char> =
            ps.iter().map(|argument| argument.as_ptr()).collect();
        ps_argv.push(std::ptr::null());
        Ok(ReaperContainers {
            program,
            _ps: ps,
            ps_argv,
        })
    }

    fn container_scope_for_a_new_reaper() -> Option<ReaperContainers> {
        let scope = CONTAINER_SCOPE
            .get()?
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()?;
        render_container_argv(&scope).ok()
    }

    fn reclaim_labeled_containers(containers: &ReaperContainers) {
        let mut buffer = [0_u8; REAPER_PS_BUFFER];
        let mut previous = [0_u8; REAPER_PS_BUFFER];
        let mut previous_filled = usize::MAX;
        loop {
            let filled = list_labeled_containers(containers, &mut buffer);
            if filled == 0 {
                return;
            }
            if filled == previous_filled && buffer[..filled] == previous[..filled] {
                return;
            }
            previous[..filled].copy_from_slice(&buffer[..filled]);
            previous_filled = filled;
            let mut settled = 0_usize;
            let mut start = 0_usize;
            for index in 0..filled {
                if buffer[index] != b'\n' {
                    continue;
                }
                buffer[index] = 0;
                if index > start {
                    let id = buffer[start..].as_ptr().cast::<libc::c_char>();
                    let kill: [*const libc::c_char; 4] = [
                        containers.program.as_ptr(),
                        c"kill".as_ptr(),
                        id,
                        std::ptr::null(),
                    ];
                    spawn_docker(containers.program.as_ptr(), kill.as_ptr());
                    let remove: [*const libc::c_char; 6] = [
                        containers.program.as_ptr(),
                        c"rm".as_ptr(),
                        c"--force".as_ptr(),
                        c"--volumes".as_ptr(),
                        id,
                        std::ptr::null(),
                    ];
                    spawn_docker(containers.program.as_ptr(), remove.as_ptr());
                    settled = settled.saturating_add(1);
                }
                start = index + 1;
            }
            if settled == 0 {
                return;
            }
        }
    }

    fn list_labeled_containers(containers: &ReaperContainers, buffer: &mut [u8]) -> usize {
        let mut fds = [0 as libc::c_int; 2];
        if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
            return 0;
        }
        let pid = unsafe { libc::fork() };
        if pid < 0 {
            close_fd(fds[0]);
            close_fd(fds[1]);
            return 0;
        }
        if pid == 0 {
            unsafe {
                if fds[1] != 1 && libc::dup2(fds[1], 1) < 0 {
                    libc::_exit(127);
                }
                if fds[0] != 1 {
                    close_fd(fds[0]);
                }
                if fds[1] != 1 {
                    close_fd(fds[1]);
                }
                quiet_standard_descriptors();
                libc::execv(containers.program.as_ptr(), containers.ps_argv.as_ptr());
                libc::_exit(127);
            }
        }
        close_fd(fds[1]);
        let filled = read_bounded(fds[0], buffer);
        close_fd(fds[0]);
        reap_bounded(pid);
        filled
    }

    fn spawn_docker(program: *const libc::c_char, argv: *const *const libc::c_char) {
        let pid = unsafe { libc::fork() };
        if pid < 0 {
            return;
        }
        if pid == 0 {
            unsafe {
                quiet_standard_descriptors();
                libc::execv(program, argv);
                libc::_exit(127);
            }
        }
        reap_bounded(pid);
    }

    unsafe fn quiet_standard_descriptors() {
        unsafe {
            ensure_standard_descriptor(0, libc::O_RDONLY);
            ensure_standard_descriptor(1, libc::O_WRONLY);
            ensure_standard_descriptor(2, libc::O_WRONLY);
        }
    }

    unsafe fn ensure_standard_descriptor(target: libc::c_int, flags: libc::c_int) {
        unsafe {
            if libc::fcntl(target, libc::F_GETFD) != -1 {
                return;
            }
            let opened = libc::open(c"/dev/null".as_ptr(), flags);
            if opened < 0 {
                return;
            }
            if opened != target {
                let _ = libc::dup2(opened, target);
                close_fd(opened);
            }
        }
    }

    fn read_bounded(fd: libc::c_int, buffer: &mut [u8]) -> usize {
        let mut used = 0_usize;
        let mut ticks = 0_usize;
        while used < buffer.len() {
            let mut waiting = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            let ready = unsafe { libc::poll(&mut waiting, 1, 10) };
            if ready < 0 {
                if last_errno_is_interrupted() {
                    continue;
                }
                return used;
            }
            if ready == 0 {
                ticks = ticks.saturating_add(1);
                if ticks >= REAPER_DOCKER_TICKS {
                    return used;
                }
                continue;
            }
            let read = unsafe {
                libc::read(
                    fd,
                    buffer.as_mut_ptr().add(used).cast(),
                    buffer.len() - used,
                )
            };
            if read > 0 {
                used += read as usize;
            } else if read < 0 && last_errno_is_interrupted() {
                continue;
            } else {
                return used;
            }
        }
        used
    }

    fn reap_bounded(pid: libc::pid_t) {
        for _ in 0..REAPER_DOCKER_TICKS {
            let waited = unsafe { libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG) };
            if waited == pid {
                return;
            }
            if waited < 0 && !last_errno_is_interrupted() {
                return;
            }
            raw_sleep_10ms();
        }
        unsafe {
            let _ = libc::kill(pid, libc::SIGKILL);
        }
        loop {
            let waited = unsafe { libc::waitpid(pid, std::ptr::null_mut(), 0) };
            if waited == pid || (waited < 0 && !last_errno_is_interrupted()) {
                return;
            }
        }
    }

    fn settle_after_coordinator_death(
        pgid: i32,
        anchor: libc::pid_t,
        cleanup_delay_ms: u64,
        containers: Option<&ReaperContainers>,
    ) {
        if pgid > 0 {
            cleanup_reaper_group(pgid, anchor, cleanup_delay_ms);
        }
        if let Some(containers) = containers {
            reclaim_labeled_containers(containers);
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::process::{Command, Stdio};
        use std::time::Instant;

        static REAPED_CHILD_STOP: AtomicBool = AtomicBool::new(false);

        #[test]
        fn a_pid_enumeration_that_failed_is_not_an_empty_process_group() {
            const BUFFER: usize = 65_536;

            assert_eq!(listed_pid_bytes(0, libc::ENOMEM, BUFFER), None);
            assert_eq!(listed_pid_bytes(0, libc::EINVAL, BUFFER), None);
            assert_eq!(listed_pid_bytes(0, libc::ESRCH, BUFFER), None);

            assert_eq!(listed_pid_bytes(0, 0, BUFFER), Some(0));

            assert_eq!(listed_pid_bytes(8, 0, BUFFER), Some(8));
            assert_eq!(
                listed_pid_bytes(
                    i32::try_from(BUFFER - 4).expect("the buffer fits an i32"),
                    libc::ENOMEM,
                    BUFFER
                ),
                Some(BUFFER - 4)
            );

            assert_eq!(
                listed_pid_bytes(
                    i32::try_from(BUFFER).expect("the buffer fits an i32"),
                    0,
                    BUFFER
                ),
                None
            );
            assert_eq!(listed_pid_bytes(-1, 0, BUFFER), None);
        }

        #[test]
        fn the_reapers_cleanup_hold_is_shared_between_overlapping_invocations() {
            use std::os::unix::ffi::OsStrExt;

            let path = std::env::temp_dir().join(format!(
                "upstroke-r28-shared-{}-{}.lock",
                std::process::id(),
                crate::ulid::ulid()
            ));
            std::fs::write(&path, b"").expect("create a cleanup lease file");
            let target = std::ffi::CString::new(path.as_os_str().as_bytes())
                .expect("a temporary path without a null byte");
            let held = std::slice::from_ref(&target);

            assert_eq!(
                lock_cleanup_paths(held),
                Ok(()),
                "the first invocation's reaper could not take R28 at all"
            );
            assert_eq!(
                lock_cleanup_paths(held),
                Ok(()),
                "a second overlapping invocation's reaper was refused the shared hold: \
                 R28 is `one per reaper`, not one per run"
            );

            // And both holds still exclude the next coordinator, which is the
            // other half of R28: `observed (never owned or reset) by the next
            // coordinator … through the exclusive cleanup probe`.
            // SAFETY: a null-terminated path this test created; a failure
            // returns a negative descriptor.
            let fd = unsafe { libc::open(target.as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC) };
            assert!(fd >= 0, "reopening the lease file");
            // SAFETY: `fd` is live and owned here until it is closed.
            let exclusive = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
            let errno = last_errno();
            close_fd(fd);
            let _ = std::fs::remove_file(&path);
            assert_ne!(
                exclusive, 0,
                "the exclusive side was granted while two reapers held R28"
            );
            assert!(
                errno == libc::EWOULDBLOCK || errno == libc::EAGAIN,
                "the exclusive probe failed for an unrelated reason: {errno}"
            );
        }

        #[test]
        fn reaper_distinguishes_a_probe_pulse_from_a_stable_parent_resume() {
            let mut running_polls = 0;
            for _ in 1..REAPER_RESUME_STABLE_POLLS {
                assert!(!parent_has_stably_resumed(Some(false), &mut running_polls));
            }
            assert!(!parent_has_stably_resumed(Some(true), &mut running_polls));
            assert_eq!(running_polls, 0);

            for _ in 1..REAPER_RESUME_STABLE_POLLS {
                assert!(!parent_has_stably_resumed(Some(false), &mut running_polls));
            }
            assert!(parent_has_stably_resumed(Some(false), &mut running_polls));
            assert!(!parent_has_stably_resumed(None, &mut running_polls));
            assert_eq!(running_polls, 0);
        }

        extern "C" fn reap_child_transitions(_: libc::c_int) {
            if REAPED_CHILD_STOP.swap(true, Ordering::SeqCst) {
                return;
            }
            let mut status = 0;
            let child = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG | libc::WUNTRACED) };
            if child > 0 && libc::WIFSTOPPED(status) {
                let _ = unsafe { libc::kill(child, libc::SIGKILL) };
            }
        }

        fn spawn_sigchld_target() -> (libc::pid_t, std::os::fd::OwnedFd) {
            use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

            // Resolve libc's descriptor ceiling before entering the forked
            // child, where only async-signal-safe operations are permitted.
            // SAFETY: sysconf takes a constant selector and no pointers.
            let open_max = unsafe { libc::sysconf(libc::_SC_OPEN_MAX) };
            let open_max =
                libc::c_int::try_from(open_max).expect("the descriptor ceiling fits c_int");
            assert!(open_max > 0, "the descriptor ceiling is available");
            let [reader, writer] = create_cloexec_pipe().expect("the target lifetime pipe");
            // SAFETY: create_cloexec_pipe returned two distinct, owned file
            // descriptors. Each is transferred to exactly one OwnedFd here.
            let lifetime_read = unsafe { OwnedFd::from_raw_fd(reader) };
            // SAFETY: writer is the other newly created, independently owned fd.
            let lifetime_write = unsafe { OwnedFd::from_raw_fd(writer) };
            // SAFETY: the child only sets its group, closes descriptors, reads
            // its pipe, inspects errno, and exits without Rust destructors.
            let target = unsafe { libc::fork() };
            if target == 0 {
                // SAFETY: zero names this child and its own new group.
                if unsafe { libc::setpgid(0, 0) } != 0 {
                    // SAFETY: exit the forked child without running destructors.
                    unsafe { libc::_exit(1) };
                }
                close_inherited_fds(&[lifetime_read.as_raw_fd()], open_max);
                let mut byte = [0_u8; 1];
                loop {
                    // SAFETY: the retained fd is readable and byte is writable
                    // for its full length. No allocation or lock follows fork.
                    let read = unsafe {
                        libc::read(
                            lifetime_read.as_raw_fd(),
                            byte.as_mut_ptr().cast(),
                            byte.len(),
                        )
                    };
                    if read >= 0 || !last_errno_is_interrupted() {
                        // EOF is parent death/unwind. Any other terminal pipe
                        // result also ends this otherwise idle fixture target.
                        // SAFETY: no Rust state is shared back across fork.
                        unsafe { libc::_exit(0) };
                    }
                }
            }
            assert!(target > 0, "fork the SIGCHLD fixture target");
            drop(lifetime_read);
            // SAFETY: target is our newly forked child. The child also calls
            // setpgid so either scheduling order establishes the same group.
            let result = unsafe { libc::setpgid(target, target) };
            assert!(
                result == 0 || matches!(last_errno(), libc::EACCES | libc::EPERM),
                "setpgid: {}",
                std::io::Error::last_os_error()
            );
            (target, lifetime_write)
        }

        #[test]
        #[ignore = "subprocess helper"]
        fn sigchld_reaper_host_helper() {
            if std::env::var_os("UPSTROKE_SIGCHLD_REAPER_HELPER").is_none() {
                return;
            }
            REAPED_CHILD_STOP.store(false, Ordering::SeqCst);
            // SAFETY: this isolated helper deliberately installs the supplied
            // async-signal-safe SIGCHLD callback before creating its children.
            assert_ne!(
                unsafe {
                    libc::signal(
                        libc::SIGCHLD,
                        reap_child_transitions as *const () as libc::sighandler_t,
                    )
                },
                libc::SIG_ERR
            );
            let (target, _parent_lifetime) = spawn_sigchld_target();

            let reaper = spawn_reaper().expect("spawn private reaper");
            assert!(reaper.register_raw(target), "register target group");
            assert!(reaper.cleanup(target), "cleanup target group");
            let started = Instant::now();
            loop {
                // SAFETY: target is the fixture child. The installed callback
                // may already have reaped it, which is the accepted ECHILD case.
                let result = unsafe { libc::waitpid(target, std::ptr::null_mut(), libc::WNOHANG) };
                if result == target || (result < 0 && last_errno() == libc::ECHILD) {
                    break;
                }
                assert!(
                    result == 0 || last_errno_is_interrupted(),
                    "wait for settled target"
                );
                assert!(
                    started.elapsed() < Duration::from_secs(10),
                    "reap settled target"
                );
                thread::sleep(Duration::from_millis(10));
            }
        }

        fn wait_for_lifetime_target(target: libc::pid_t) -> Option<libc::c_int> {
            let started = Instant::now();
            while started.elapsed() < Duration::from_secs(10) {
                let mut status = 0;
                // SAFETY: status is writable, and target is this helper's
                // unreaped child. WNOHANG makes this observation bounded.
                let result = unsafe { libc::waitpid(target, &mut status, libc::WNOHANG) };
                if result == target {
                    return Some(status);
                }
                assert!(
                    result == 0 || last_errno_is_interrupted(),
                    "polling owned lifetime target: {}",
                    std::io::Error::last_os_error()
                );
                thread::sleep(Duration::from_millis(10));
            }
            None
        }

        #[test]
        #[ignore = "subprocess helper"]
        fn sigchld_target_setup_failure_helper() {
            if std::env::var_os("UPSTROKE_SIGCHLD_TARGET_FAILURE_HELPER").is_none() {
                return;
            }
            // SAFETY: this fresh helper process owns its only child and all
            // waits. Default SIGCHLD retains a zombie until our waitpid, so a
            // timed-out mutation can be killed without a PID-reuse window.
            assert_ne!(
                unsafe { libc::signal(libc::SIGCHLD, libc::SIG_DFL) },
                libc::SIG_ERR
            );
            let (target, parent_lifetime) = spawn_sigchld_target();
            let refusal = std::panic::catch_unwind(move || {
                let _parent_lifetime = parent_lifetime;
                let _reaper = spawn_reaper().expect("forced reaper startup refusal");
            });
            let exited = wait_for_lifetime_target(target);
            if exited.is_none() {
                // SAFETY: SIGCHLD is default, target has never been reaped,
                // and this fresh process has no other waiter. Its identity
                // remains pinned even if it exits between the poll and kill.
                assert_eq!(unsafe { libc::kill(target, libc::SIGKILL) }, 0);
                assert!(
                    wait_for_lifetime_target(target).is_some(),
                    "reap the deliberately broken fixture before failing its witness"
                );
            }
            assert!(refusal.is_err(), "the injected startup refusal occurred");
            let status = exited.expect("the parked target survived parent setup failure");
            assert!(libc::WIFEXITED(status), "the target exited on pipe closure");
            assert_eq!(libc::WEXITSTATUS(status), 0);
        }

        #[test]
        fn a_parked_sigchld_target_exits_after_parent_setup_failure() {
            use std::os::unix::process::CommandExt;

            let mut command = Command::new(std::env::current_exe().expect("test executable"));
            command
                .args([
                    "sigchld_target_setup_failure_helper",
                    "--ignored",
                    "--nocapture",
                ])
                .env("UPSTROKE_SIGCHLD_TARGET_FAILURE_HELPER", "1")
                .env("UPSTROKE_TEST_HELPER_EXIT_BEFORE_READY", "7")
                .process_group(0)
                .stdout(Stdio::null())
                .stderr(Stdio::inherit());
            let mut helper = command
                .spawn()
                .expect("spawn isolated target-failure helper");
            let pid = i32::try_from(helper.id()).expect("helper process-group id");
            let started = Instant::now();
            loop {
                if let Some(status) = helper.try_wait().expect("poll the target-failure helper") {
                    assert!(status.success(), "target-failure helper: {status}");
                    break;
                }
                if started.elapsed() >= Duration::from_secs(60) {
                    // SAFETY: the helper is our unreaped process-group leader;
                    // its group is isolated by CommandExt::process_group.
                    let signaled = unsafe { libc::kill(-pid, libc::SIGKILL) };
                    let cleanup_started = Instant::now();
                    let mut reaped = false;
                    while cleanup_started.elapsed() < Duration::from_secs(10) {
                        if helper
                            .try_wait()
                            .expect("reap the timed-out helper")
                            .is_some()
                        {
                            reaped = true;
                            break;
                        }
                        thread::sleep(Duration::from_millis(10));
                    }
                    panic!(
                        "target-failure helper exceeded its bound; kill={signaled}, reaped={reaped}"
                    );
                }
                thread::sleep(Duration::from_millis(10));
            }
        }

        #[test]
        fn a_host_sigchld_reaper_cannot_consume_the_private_anchor() {
            use std::os::unix::process::CommandExt;

            let mut command = Command::new(std::env::current_exe().expect("test executable"));
            command
                .args(["sigchld_reaper_host_helper", "--ignored", "--nocapture"])
                .env("UPSTROKE_SIGCHLD_REAPER_HELPER", "1")
                .process_group(0)
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let mut child = command.spawn().expect("spawn SIGCHLD helper");
            let pid = i32::try_from(child.id()).expect("helper pid");
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(status) = child.try_wait().expect("poll SIGCHLD helper") {
                    assert!(status.success(), "SIGCHLD helper status: {status}");
                    break;
                }
                if Instant::now() >= deadline {
                    let _ = unsafe { libc::kill(-pid, libc::SIGKILL) };
                    let _ = child.wait();
                    panic!("inherited SIGCHLD reaper consumed the private anchor transition");
                }
                thread::sleep(Duration::from_millis(10));
            }
        }

        #[test]
        #[ignore = "subprocess helper"]
        fn reaper_handshake_helper() {
            if std::env::var_os("UPSTROKE_REAPER_HANDSHAKE_HELPER").is_none() {
                return;
            }
            let started = Instant::now();
            let launch = spawn_reaper();
            let elapsed = started.elapsed();
            assert!(
                launch.is_err(),
                "a reaper that missed its READY deadline was accepted as initialized"
            );
            assert!(
                elapsed < Duration::from_secs(4),
                "the late reaper held the launch for {elapsed:?}, past its own deadline"
            );
            assert_eq!(
                PENDING_TERMINATION.load(Ordering::SeqCst),
                0,
                "a reaper that missed its READY deadline armed process-wide termination"
            );
            // SAFETY: `waitpid(-1, WNOHANG)` inspects only this process's
            // children, and the late reaper is the only child it has had.
            let waited = unsafe { libc::waitpid(-1, std::ptr::null_mut(), libc::WNOHANG) };
            assert!(
                waited < 0 && !last_errno_is_interrupted(),
                "the late reaper was left behind (waitpid(-1, WNOHANG) returned {waited})"
            );

            let command = create_cloexec_pipe().expect("a stand-in command pipe");
            let ack = create_cloexec_pipe().expect("a stand-in acknowledgement pipe");
            // SAFETY: the forked child calls only `_exit`.
            let pid = unsafe { libc::fork() };
            if pid == 0 {
                unsafe { libc::_exit(0) };
            }
            assert!(pid > 0, "fork a stand-in reaper for the cancel to reap");
            let identity = helper_identity(pid);
            assert!(write_raw(ack[1], &[REAPER_READY]), "queue the stale READY");
            let answer = thread::spawn(move || {
                let mut frame = [0_u8; 5];
                read_raw_exact(command[0], &mut frame)
                    && frame[0] == REAPER_CANCEL
                    && write_raw(ack[1], &[REAPER_OK])
            });
            Reaper {
                command_fd: command[1],
                ack_fd: ack[0],
                _command_keepalive_fd: command[0],
                pid,
                identity,
            }
            .cancel();
            assert!(
                answer.join().expect("the stand-in reaper thread"),
                "the stand-in reaper never saw CANCEL"
            );
            close_fd(ack[1]);
            assert_eq!(
                PENDING_TERMINATION.load(Ordering::SeqCst),
                0,
                "a CANCEL acknowledged behind a stale READY armed process-wide termination"
            );
        }

        #[test]
        fn a_late_reaper_fails_its_launch_without_arming_termination() {
            use std::os::unix::process::CommandExt;

            let output = Command::new(std::env::current_exe().expect("test executable"))
                .args(["reaper_handshake_helper", "--ignored", "--nocapture"])
                .env("UPSTROKE_REAPER_HANDSHAKE_HELPER", "1")
                .env("UPSTROKE_TEST_REAPER_READY_DELAY_MS", "4500")
                .process_group(0)
                .stdin(Stdio::null())
                .output()
                .expect("run the reaper handshake helper");
            assert!(
                output.status.success(),
                "reaper handshake helper: {}\n{}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        #[cfg(target_os = "linux")]
        #[test]
        #[ignore = "subprocess helper"]
        fn linux_close_range_fd_zero_helper() {
            if std::env::var_os("UPSTROKE_CLOSE_RANGE_FD_ZERO_HELPER").is_none() {
                return;
            }

            let closed = unsafe { libc::close(libc::STDIN_FILENO) };
            assert!(
                closed == 0 || last_errno() == libc::EBADF,
                "closing helper stdin: {}",
                std::io::Error::last_os_error()
            );
            let mut pipe = [-1; 2];
            assert_eq!(unsafe { libc::pipe(pipe.as_mut_ptr()) }, 0);
            assert_eq!(pipe[0], 0, "closed stdin was not reused as pipe fd zero");
            let open_max = unsafe { libc::sysconf(libc::_SC_OPEN_MAX) };
            assert!(open_max > 0, "invalid open-file descriptor ceiling");
            let open_max = libc::c_int::try_from(open_max).expect("descriptor ceiling fits c_int");

            close_inherited_fds(
                &[pipe[0], pipe[1], libc::STDOUT_FILENO, libc::STDERR_FILENO],
                open_max,
            );
            let sent = [0x5a_u8];
            let mut received = [0_u8];
            assert!(write_raw(pipe[1], &sent), "write through kept pipe");
            assert!(
                read_raw_exact(pipe[0], &mut received),
                "close_range closed kept fd zero"
            );
            assert_eq!(received, sent);
        }

        #[cfg(target_os = "linux")]
        #[test]
        fn linux_close_range_preserves_a_kept_fd_zero() {
            let mut command = Command::new(std::env::current_exe().expect("test executable"));
            command
                .args([
                    "linux_close_range_fd_zero_helper",
                    "--ignored",
                    "--nocapture",
                ])
                .env("UPSTROKE_CLOSE_RANGE_FD_ZERO_HELPER", "1")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            let output = command.output().expect("run fd-zero close-range helper");
            assert!(
                output.status.success(),
                "fd-zero helper failed: {}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        #[test]
        fn a_launch_cannot_enter_after_the_suspend_snapshot() {
            let state = Arc::new(Mutex::new(State {
                spawning: 0,
                groups: vec![RegisteredGroup {
                    pgid: 41,
                    signal_leases: 0,
                }],
                terminating: false,
                suspending: false,
                guard: Guard {
                    command_fd: -1,
                    ack_fd: -1,
                    _command_keepalive_fd: -1,
                    pid: -1,
                    identity: -1,
                },
            }));
            let (groups, _) = begin_suspend(&state).expect("begin suspend transition");
            assert_eq!(&*groups, &[41]);

            let waiting = Arc::clone(&state);
            let (sent, received) = std::sync::mpsc::channel();
            let launch = thread::spawn(move || {
                claim_launch(&waiting).expect("launch after resume");
                sent.send(()).expect("report launch");
                release_launch(&waiting);
            });
            assert!(
                received.recv_timeout(Duration::from_millis(50)).is_err(),
                "a launch entered while the frozen process-group snapshot was active"
            );

            let resumed = end_suspend(&state);
            assert_eq!(&*resumed, &[41]);
            drop(resumed);
            drop(groups);
            received
                .recv_timeout(Duration::from_secs(2))
                .expect("launch released after resume");
            launch.join().expect("join launch");
            let locked = state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            assert_eq!(locked.spawning, 0);
        }

        #[test]
        fn signal_snapshot_pins_a_group_until_delivery_finishes() {
            let state = Arc::new(Mutex::new(State {
                spawning: 0,
                groups: vec![RegisteredGroup {
                    pgid: 41,
                    signal_leases: 0,
                }],
                terminating: false,
                suspending: false,
                guard: Guard {
                    command_fd: -1,
                    ack_fd: -1,
                    _command_keepalive_fd: -1,
                    pid: -1,
                    identity: -1,
                },
            }));
            let snapshot = groups_when_registered(&state, false).expect("group snapshot");
            {
                let mut locked = state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                assert_eq!(locked.groups[0].signal_leases, 1);
                assert!(
                    !remove_unpinned_group(&mut locked, 41),
                    "finish exposed the group id while a signal snapshot held it"
                );
            }
            drop(snapshot);
            let mut locked = state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            assert!(remove_unpinned_group(&mut locked, 41));
            assert!(locked.groups.is_empty());
        }

        #[test]
        fn helper_pipe_descriptors_are_close_on_exec() {
            let pipe = create_cloexec_pipe().expect("atomic close-on-exec pipe");
            for fd in pipe {
                let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
                assert!(flags >= 0, "read descriptor flags");
                assert_ne!(
                    flags & libc::FD_CLOEXEC,
                    0,
                    "helper descriptor was visible without close-on-exec"
                );
                close_fd(fd);
            }
        }

        #[cfg(target_os = "linux")]
        #[test]
        fn linux_stat_parser_handles_spaces_and_closing_parentheses_in_comm() {
            let stat = "123 (reviewer ) helper) T 7 123 123 0 -1 0";
            assert_eq!(parse_linux_process_stat(stat), Some((123, b'T')));
            assert_eq!(parse_linux_stat_bytes(stat.as_bytes()), Some((123, b'T')));
            assert_eq!(parse_linux_process_stat("malformed"), None);
            assert_eq!(parse_linux_stat_bytes(b"malformed"), None);
        }

        #[cfg(target_os = "linux")]
        #[test]
        fn a_vanished_linux_pid_is_not_an_incomplete_scanner_snapshot() {
            assert_eq!(read_linux_stat_raw(i32::MAX), LinuxStatSnapshot::Vanished);
        }

        #[test]
        fn a_zombie_only_group_is_quiescent_for_cleanup() {
            // SAFETY: the child performs only async-signal-safe syscalls and
            // exits immediately. The parent deliberately observes it without
            // reaping so the cleanup scanner sees a real zombie-only PGID.
            let pid = unsafe { libc::fork() };
            if pid == 0 {
                let code = i32::from(unsafe { libc::setpgid(0, 0) } != 0);
                unsafe { libc::_exit(code) };
            }
            assert!(pid > 0, "fork failed: {}", std::io::Error::last_os_error());

            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            loop {
                let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
                let result = unsafe {
                    libc::waitid(
                        libc::P_PID,
                        pid as libc::id_t,
                        &mut info,
                        libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                    )
                };
                assert_eq!(result, 0, "waitid: {}", std::io::Error::last_os_error());
                if unsafe { info.si_pid() } == pid {
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "child never became a zombie"
                );
                thread::sleep(Duration::from_millis(1));
            }

            let scan_deadline = std::time::Instant::now() + Duration::from_secs(2);
            let observed = loop {
                match group_has_non_zombie_members(pid) {
                    observed @ Some(_) => break observed,
                    None if std::time::Instant::now() < scan_deadline => {
                        thread::sleep(Duration::from_millis(1));
                    }
                    None => break None,
                }
            };
            unsafe {
                let _ = libc::waitpid(pid, std::ptr::null_mut(), 0);
            }
            assert_eq!(observed, Some(false));
        }

        #[test]
        fn the_reaper_program_is_resolved_to_an_absolute_path_before_the_fork() {
            use crate::runner::container::census::ReaperContainerScope;
            use std::ffi::OsStr;
            use std::os::unix::ffi::OsStrExt as _;
            use std::path::Path;

            const ROOT: &str = "/srv/upstroke-reaper-resolve/private";
            const INCARNATION: &str = "01KZTBBBBBBBBBBBBBBBBBBBBB";

            fn scope(program: &str) -> ReaperContainerScope {
                ReaperContainerScope::new(program, Path::new(ROOT), INCARNATION)
                    .expect("a well-formed scope")
            }
            fn execd(rendered: &ReaperContainers) -> std::path::PathBuf {
                std::path::PathBuf::from(OsStr::from_bytes(rendered.program.as_bytes()))
            }

            let rendered = render_container_argv(&scope("git")).expect("git is on PATH");
            let program = execd(&rendered);
            assert!(
                program.is_absolute(),
                "`execv` was handed `{}`, which it will not search `PATH` for",
                program.display()
            );
            assert!(
                program.is_file(),
                "`{}` is not a file on this machine",
                program.display()
            );
            assert_eq!(
                program.file_name(),
                Some(OsStr::new("git")),
                "the resolution found something other than the program it was asked for: {}",
                program.display()
            );
            let argv0 = unsafe { std::ffi::CStr::from_ptr(rendered.ps_argv[0]) };
            assert_eq!(argv0.to_bytes(), program.as_os_str().as_bytes());

            let absent = scope("upstroke-definitely-not-a-real-docker");
            let message = match render_container_argv(&absent) {
                Ok(_) => panic!("an unresolvable bare program was accepted"),
                Err(error) => error.to_string(),
            };
            assert!(
                message.contains("PATH")
                    && message.contains("upstroke-definitely-not-a-real-docker"),
                "{message}"
            );
            assert!(
                set_container_reclaim_scope(Some(&absent)).is_err(),
                "arming accepted a scope whose program cannot be executed"
            );

            let rendered = render_container_argv(&scope("/usr/bin/docker")).expect("a path");
            assert_eq!(execd(&rendered), Path::new("/usr/bin/docker"));
            let rendered = render_container_argv(&scope("./docker")).expect("a relative path");
            assert_eq!(execd(&rendered), Path::new("./docker"));
        }

        fn reaper_stub(tag: &str, script: &str) -> (std::path::PathBuf, ReaperContainers) {
            use std::os::unix::fs::PermissionsExt as _;
            let dir = std::env::temp_dir().join(format!(
                "upstroke-reaper-rounds-{tag}-{}-{}",
                std::process::id(),
                crate::ulid::ulid()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("scratch");
            let stub = dir.join("docker-stub");
            std::fs::write(&stub, script).expect("write the stub");
            std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755))
                .expect("make the stub executable");
            let scope = crate::runner::container::census::ReaperContainerScope::new(
                &stub,
                std::path::Path::new("/srv/upstroke-reaper-rounds/private"),
                "01KZTAAAAAAAAAAAAAAAAAAAAA",
            )
            .expect("a scope");
            let rendered = render_container_argv(&scope).expect("argv");
            (dir, rendered)
        }

        fn logged(dir: &std::path::Path, verb: &str) -> Vec<String> {
            std::fs::read_to_string(dir.join("argv.log"))
                .unwrap_or_default()
                .lines()
                .filter(|line| line.split_whitespace().next() == Some(verb))
                .map(str::to_owned)
                .collect()
        }

        #[test]
        fn the_reaper_performs_as_many_rounds_as_the_machine_needs() {
            const ROUNDS: usize = 12;
            let (dir, rendered) = reaper_stub(
                "unbounded",
                &format!(
                    "#!/bin/sh\n\
                     d=$(dirname \"$0\")\n\
                     printf '%s\\n' \"$*\" >> \"$d/argv.log\"\n\
                     case \"$1\" in\n\
                     ps) n=$(cat \"$d/round\" 2>/dev/null || echo 0); n=$((n+1)); \
                     echo \"$n\" > \"$d/round\"; \
                     [ \"$n\" -gt {ROUNDS} ] || printf '%064d\\n' \"$n\" ;;\n\
                     esac\n\
                     exit 0\n"
                ),
            );

            reclaim_labeled_containers(&rendered);

            let killed: std::collections::BTreeSet<String> = logged(&dir, "kill")
                .into_iter()
                .map(|line| line["kill ".len()..].to_owned())
                .collect();
            let removed: std::collections::BTreeSet<String> = logged(&dir, "rm")
                .into_iter()
                .map(|line| line["rm --force --volumes ".len()..].to_owned())
                .collect();
            let expected: std::collections::BTreeSet<String> =
                (1..=ROUNDS).map(|n| format!("{n:064}")).collect();
            assert_eq!(
                killed,
                expected,
                "the reaper stopped early: it killed {} of {ROUNDS}",
                killed.len()
            );
            assert_eq!(removed, expected, "kill and rm did not settle the same set");
            assert!(
                logged(&dir, "ps").len() > ROUNDS,
                "{} listings for {ROUNDS} rounds plus the empty one that ends the loop",
                logged(&dir, "ps").len()
            );
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn the_reaper_settles_more_containers_than_one_listing_holds() {
            const CONTAINERS: usize = 130;
            let (dir, rendered) = reaper_stub(
                "over-buffer",
                "#!/bin/sh\n\
                 d=$(dirname \"$0\")\n\
                 printf '%s\\n' \"$*\" >> \"$d/argv.log\"\n\
                 case \"$1\" in\n\
                 ps) ls \"$d/ids\" 2>/dev/null ;;\n\
                 rm) rm -f \"$d/ids/$4\" ;;\n\
                 esac\n\
                 exit 0\n",
            );
            std::fs::create_dir_all(dir.join("ids")).expect("the id set");
            let expected: std::collections::BTreeSet<String> = (1..=CONTAINERS)
                .map(|n| format!("{n:064}"))
                .inspect(|id| std::fs::write(dir.join("ids").join(id), "").expect("an id"))
                .collect();
            assert_eq!(expected.len(), CONTAINERS);
            const {
                assert!(
                    CONTAINERS * 65 > REAPER_PS_BUFFER,
                    "the fixture must not fit in one listing"
                );
            }

            reclaim_labeled_containers(&rendered);

            let killed: std::collections::BTreeSet<String> = logged(&dir, "kill")
                .into_iter()
                .map(|line| line["kill ".len()..].to_owned())
                .collect();
            let removed: std::collections::BTreeSet<String> = logged(&dir, "rm")
                .into_iter()
                .map(|line| line["rm --force --volumes ".len()..].to_owned())
                .collect();
            assert_eq!(
                killed,
                expected,
                "{} of {CONTAINERS} containers were killed; the ids past the end of the first \
                 listing were dropped rather than settled by a later round",
                killed.len()
            );
            assert_eq!(removed, expected);
            assert_eq!(
                std::fs::read_dir(dir.join("ids"))
                    .expect("the id set")
                    .count(),
                0,
                "the stub still holds containers the reaper never removed"
            );
            assert!(
                logged(&dir, "ps").len() >= 3,
                "one listing cannot hold 130 ids"
            );
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn a_runtime_that_keeps_answering_the_same_listing_ends_the_loop() {
            let (dir, rendered) = reaper_stub(
                "no-progress",
                "#!/bin/sh\n\
                 d=$(dirname \"$0\")\n\
                 printf '%s\\n' \"$*\" >> \"$d/argv.log\"\n\
                 case \"$1\" in\n\
                 ps) n=$(cat \"$d/round\" 2>/dev/null || echo 0); n=$((n+1)); \
                 echo \"$n\" > \"$d/round\"; \
                 [ \"$n\" -gt 2 ] || { printf '%064d\\n' 1; printf '%064d\\n' 2; } ;;\n\
                 esac\n\
                 exit 0\n",
            );

            reclaim_labeled_containers(&rendered);

            assert_eq!(
                logged(&dir, "ps").len(),
                2,
                "a repeated listing was acted on again: {:?}",
                logged(&dir, "ps")
            );
            assert_eq!(logged(&dir, "kill").len(), 2, "{:?}", logged(&dir, "kill"));
            assert_eq!(logged(&dir, "rm").len(), 2, "{:?}", logged(&dir, "rm"));
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn a_helper_ending_is_described_by_what_the_kill_and_the_wait_answered() {
            let exited_one = 1 << 8;
            let killed_by_nine = 9;
            assert_eq!(
                describe_helper_end(HelperEnd {
                    kill_errno: 0,
                    waited: 4321,
                    wait_errno: 0,
                    status: exited_one,
                }),
                "SIGKILL was delivered, and the wait collected it, having already exited with \
                 status 1",
                "a helper that had already failed its own setup"
            );
            assert_eq!(
                describe_helper_end(HelperEnd {
                    kill_errno: 0,
                    waited: 4321,
                    wait_errno: 0,
                    status: killed_by_nine,
                }),
                "SIGKILL was delivered, and the wait collected it, killed by signal 9",
                "a helper that was still there when the parent gave up"
            );
            let gone = describe_helper_end(HelperEnd {
                kill_errno: libc::ESRCH,
                waited: -1,
                wait_errno: libc::ECHILD,
                status: 0,
            });
            assert!(
                gone.contains("SIGKILL answered ESRCH, so nothing of that number was there")
                    && gone.contains("the wait collected nothing"),
                "a helper already gone and already collected elsewhere: {gone}"
            );
            let refused = describe_helper_end(HelperEnd {
                kill_errno: libc::EPERM,
                waited: -1,
                wait_errno: libc::ECHILD,
                status: 0,
            });
            assert!(
                refused.starts_with("SIGKILL failed:") && !refused.contains("ESRCH"),
                "a signal the host refused is not the same as a helper that was gone: {refused}"
            );
        }

        #[cfg(target_os = "linux")]
        #[test]
        fn the_end_of_a_helper_follows_its_identity_and_not_its_number() {
            // SAFETY: the forked child calls only `_exit`.
            let helper = unsafe { libc::fork() };
            if helper == 0 {
                // SAFETY: leave the forked child without running destructors.
                unsafe { libc::_exit(0) };
            }
            assert!(
                helper > 0,
                "fork a helper: {}",
                std::io::Error::last_os_error()
            );
            let identity = helper_identity(helper);
            assert!(identity >= 0, "this kernel gave the helper no identity");
            let helper_status = reap(helper);
            assert!(
                libc::WIFEXITED(helper_status),
                "the helper was collected elsewhere before it exited: {helper_status}"
            );

            let (stranger, stranger_lifetime) = spawn_sigchld_target();
            let end = Reaper {
                command_fd: -1,
                ack_fd: -1,
                _command_keepalive_fd: -1,
                pid: stranger,
                identity,
            }
            .abandon();

            let mut status = 0;
            // SAFETY: `status` is writable and `stranger` is this test's own
            // child. `WNOHANG` makes the look bounded.
            let looked = unsafe { libc::waitpid(stranger, &mut status, libc::WNOHANG) };
            let looked_errno = last_errno();
            assert_eq!(
                looked, 0,
                "the helper's teardown reached the stranger now holding its number: the look \
                 for the stranger answered {looked} (status {status}, errno {looked_errno}) \
                 and the teardown reported kill_errno {}, waited {}, wait_errno {}",
                end.kill_errno, end.waited, end.wait_errno
            );
            assert_eq!(
                end.kill_errno,
                libc::ESRCH,
                "the signal did not answer for the helper's own identity"
            );
            assert_eq!(end.waited, -1, "the wait collected something");
            assert_eq!(
                end.wait_errno,
                libc::ECHILD,
                "the wait did not answer for the helper's own identity"
            );

            drop(stranger_lifetime);
            let settled =
                wait_for_lifetime_target(stranger).expect("the stranger ends with its pipe");
            assert!(
                libc::WIFEXITED(settled),
                "the stranger did not end on its own terms: {settled}"
            );
        }

        #[cfg(target_os = "linux")]
        fn statuses_for(exit_code: Option<libc::c_int>) -> (libc::c_int, libc::c_int) {
            let mut filled = [0, 0];
            for (index, through_identity) in [false, true].into_iter().enumerate() {
                // SAFETY: the forked child calls only `_exit` or `pause`.
                let child = unsafe { libc::fork() };
                if child == 0 {
                    match exit_code {
                        // SAFETY: leave the child without running destructors.
                        Some(code) => unsafe { libc::_exit(code) },
                        None => loop {
                            // SAFETY: `pause` takes no argument and returns
                            // when the kill below arrives.
                            unsafe { libc::pause() };
                        },
                    }
                }
                assert!(child > 0, "fork: {}", std::io::Error::last_os_error());
                let identity = helper_identity(child);
                assert!(identity >= 0, "this kernel gave the child no identity");
                if exit_code.is_none() {
                    // SAFETY: `child` is this test's own unreaped child.
                    let killed = unsafe { libc::kill(child, libc::SIGKILL) };
                    assert_eq!(killed, 0, "{}", std::io::Error::last_os_error());
                }
                let mut status = 0;
                let collected = if through_identity {
                    collect_helper(child, identity, &mut status)
                } else {
                    // SAFETY: `status` is writable and `child` is this test's
                    // own unreaped child.
                    unsafe { libc::waitpid(child, &mut status, 0) }
                };
                close_fd(identity);
                assert_eq!(
                    collected,
                    child,
                    "collecting {child}: {}",
                    std::io::Error::last_os_error()
                );
                filled[index] = status;
            }
            (filled[0], filled[1])
        }

        #[cfg(target_os = "linux")]
        #[test]
        fn a_wait_through_an_identity_answers_the_status_waitpid_answers() {
            let (by_number, by_identity) = statuses_for(Some(7));
            assert_eq!(by_identity, by_number, "a child that exited with 7");
            assert!(
                libc::WIFEXITED(by_identity) && libc::WEXITSTATUS(by_identity) == 7,
                "{by_identity}"
            );

            let (by_number, by_identity) = statuses_for(None);
            assert_eq!(by_identity, by_number, "a child killed by SIGKILL");
            assert!(
                libc::WIFSIGNALED(by_identity) && libc::WTERMSIG(by_identity) == libc::SIGKILL,
                "{by_identity}"
            );
        }

        #[cfg(target_os = "linux")]
        #[test]
        #[ignore = "subprocess helper"]
        fn reaper_identity_helper() {
            if std::env::var_os("UPSTROKE_REAPER_IDENTITY_HELPER").is_none() {
                return;
            }
            let reaper = spawn_reaper().expect("spawn private reaper");
            assert!(
                reaper.identity >= 0,
                "the reaper this launch forked carries no identity"
            );
            let fdinfo = std::fs::read_to_string(
                std::path::Path::new("/proc/self/fdinfo").join(reaper.identity.to_string()),
            )
            .expect("the identity's own /proc entry");
            let named = format!("Pid:\t{}", reaper.pid);
            assert!(
                fdinfo.lines().any(|line| line == named),
                "the identity does not name the reaper this launch forked ({}):\n{fdinfo}",
                reaper.pid
            );
            // SAFETY: `reaper.identity` is the descriptor this launch took and
            // still owns; `F_GETFD` reads flags and changes nothing.
            let flags = unsafe { libc::fcntl(reaper.identity, libc::F_GETFD) };
            assert!(flags >= 0, "read the identity's descriptor flags");
            assert_ne!(
                flags & libc::FD_CLOEXEC,
                0,
                "the identity would have been visible to an exec'd agent"
            );
            reaper.cancel();
            assert_eq!(
                PENDING_TERMINATION.load(Ordering::SeqCst),
                0,
                "the cancelled reaper armed process-wide termination"
            );
            // SAFETY: `waitpid(-1, WNOHANG)` inspects only this process's
            // children, and the reaper is the only child it has had.
            let waited = unsafe { libc::waitpid(-1, std::ptr::null_mut(), libc::WNOHANG) };
            assert!(
                waited < 0 && !last_errno_is_interrupted(),
                "the cancelled reaper was left behind (waitpid(-1, WNOHANG) returned {waited})"
            );
        }

        #[cfg(target_os = "linux")]
        #[test]
        fn a_launched_reaper_is_named_by_a_descriptor_and_not_only_by_its_number() {
            use std::os::unix::process::CommandExt;

            let output = Command::new(std::env::current_exe().expect("test executable"))
                .args(["reaper_identity_helper", "--ignored", "--nocapture"])
                .env("UPSTROKE_REAPER_IDENTITY_HELPER", "1")
                .process_group(0)
                .stdin(Stdio::null())
                .output()
                .expect("run the reaper identity helper");
            assert!(
                output.status.success(),
                "reaper identity helper: {}\n{}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        /// The acquisition every test below makes. A host that cannot be
        /// asked for an identity at all is not a state any of them is about,
        /// so it is a failure here and not a third answer to assert against.
        fn helper_identity(pid: libc::pid_t) -> libc::c_int {
            match open_helper_identity(pid) {
                Ok(identity) => identity,
                Err(errno) => panic!(
                    "asking this host for the identity of {pid}: {}",
                    std::io::Error::from_raw_os_error(errno)
                ),
            }
        }

        #[cfg(target_os = "linux")]
        fn wildcard_wait() -> libc::pid_t {
            let mut status = 0;
            loop {
                // SAFETY: `status` is writable. `waitpid(-1, ...)` is the
                // wildcard wait an embedding host's `SIGCHLD` handler makes,
                // and this helper process has only the children it forks here.
                let waited = unsafe { libc::waitpid(-1, &mut status, 0) };
                if waited > 0 {
                    return waited;
                }
                assert!(
                    waited < 0 && last_errno_is_interrupted(),
                    "waitpid(-1): {}",
                    std::io::Error::last_os_error()
                );
            }
        }

        #[cfg(target_os = "linux")]
        #[test]
        #[ignore = "subprocess helper"]
        fn helper_identity_after_a_host_reap_helper() {
            if std::env::var_os("UPSTROKE_HELPER_IDENTITY_HOST_REAP_HELPER").is_none() {
                return;
            }

            // SAFETY: the forked child calls only `_exit`.
            let helper = unsafe { libc::fork() };
            if helper == 0 {
                // SAFETY: leave the forked child without running destructors.
                unsafe { libc::_exit(0) };
            }
            assert!(
                helper > 0,
                "fork a helper: {}",
                std::io::Error::last_os_error()
            );
            assert_eq!(
                wildcard_wait(),
                helper,
                "the wildcard wait collected something other than the helper"
            );

            let identity = helper_identity(helper);
            let (stranger, stranger_lifetime) = spawn_sigchld_target();
            let end = Reaper {
                command_fd: -1,
                ack_fd: -1,
                _command_keepalive_fd: -1,
                pid: stranger,
                identity,
            }
            .abandon();
            let mut status = 0;
            // SAFETY: `status` is writable and `stranger` is this process's own
            // child. `WNOHANG` makes the look bounded.
            let looked = unsafe { libc::waitpid(stranger, &mut status, libc::WNOHANG) };
            let looked_errno = last_errno();
            assert_eq!(
                looked, 0,
                "the teardown reached the stranger now holding the helper's number: the look \
                 for the stranger answered {looked} (status {status}, errno {looked_errno}) \
                 and the teardown reported kill_errno {}, waited {}, wait_errno {}",
                end.kill_errno, end.waited, end.wait_errno
            );
            assert_eq!(
                end.kill_errno,
                libc::ESRCH,
                "the signal did not answer for the helper that had ended"
            );
            assert_eq!(end.waited, -1, "the wait collected something");
            assert_eq!(
                end.wait_errno,
                libc::ECHILD,
                "the wait did not answer for the helper that had ended"
            );
            assert_ne!(
                identity, NO_HELPER_IDENTITY,
                "a helper an embedding host had already collected was read as a kernel with \
                 no identities, which puts its number back under the signal and the wait"
            );

            drop(stranger_lifetime);
            let settled =
                wait_for_lifetime_target(stranger).expect("the stranger ends with its pipe");
            assert!(
                libc::WIFEXITED(settled),
                "the stranger did not end on its own terms: {settled}"
            );

            // SAFETY: the forked child calls only `_exit`.
            let helper = unsafe { libc::fork() };
            if helper == 0 {
                // SAFETY: leave the forked child without running destructors.
                unsafe { libc::_exit(0) };
            }
            assert!(
                helper > 0,
                "fork a second helper: {}",
                std::io::Error::last_os_error()
            );
            let identity = helper_identity(helper);
            assert!(identity >= 0, "this kernel gave the helper no identity");
            assert_eq!(
                wildcard_wait(),
                helper,
                "the wildcard wait collected something other than the second helper"
            );
            assert_eq!(
                identity_can_collect(identity),
                IdentityProbe::HelperEnded,
                "a helper the host collected reads as a kernel that will not collect through \
                 a descriptor, and the descriptor naming it is discarded"
            );
            assert_eq!(
                identity_can_signal(identity),
                IdentityProbe::HelperEnded,
                "a helper the host collected reads as a kernel that will not signal through \
                 a descriptor, and the descriptor naming it is discarded"
            );
            close_fd(identity);
        }

        #[cfg(target_os = "linux")]
        #[test]
        fn a_helper_a_host_collected_is_not_a_kernel_without_identities() {
            use std::os::unix::process::CommandExt;

            let output = Command::new(std::env::current_exe().expect("test executable"))
                .args([
                    "helper_identity_after_a_host_reap_helper",
                    "--ignored",
                    "--nocapture",
                ])
                .env("UPSTROKE_HELPER_IDENTITY_HOST_REAP_HELPER", "1")
                .process_group(0)
                .stdin(Stdio::null())
                .output()
                .expect("run the host-reap identity helper");
            assert!(
                output.status.success(),
                "host-reap identity helper: {}\n{}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        /// One BPF instruction. `code` is the class word the kernel reads,
        /// `jt`/`jf` are how many instructions a jump skips when the test
        /// passes and when it fails, and `k` is the instruction's constant.
        #[cfg(target_os = "linux")]
        fn seccomp_instruction(code: u32, jt: u8, jf: u8, k: u32) -> libc::sock_filter {
            libc::sock_filter {
                code: u16::try_from(code).expect("a BPF instruction class fits the kernel's field"),
                jt,
                jf,
                k,
            }
        }

        /// Load the 32 bits of `seccomp_data` at `offset` into the accumulator.
        #[cfg(target_os = "linux")]
        fn seccomp_load(offset: u32) -> libc::sock_filter {
            seccomp_instruction(libc::BPF_LD | libc::BPF_W | libc::BPF_ABS, 0, 0, offset)
        }

        /// Jump `jt` instructions when the accumulator equals `k`, `jf` when it
        /// does not.
        #[cfg(target_os = "linux")]
        fn seccomp_jump_if_equal(k: u32, jt: u8, jf: u8) -> libc::sock_filter {
            seccomp_instruction(libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K, jt, jf, k)
        }

        /// Jump `jt` instructions when any bit of `k` is set in the
        /// accumulator, `jf` when none is.
        #[cfg(target_os = "linux")]
        fn seccomp_jump_if_any_set(k: u32, jt: u8, jf: u8) -> libc::sock_filter {
            seccomp_instruction(libc::BPF_JMP | libc::BPF_JSET | libc::BPF_K, jt, jf, k)
        }

        #[cfg(target_os = "linux")]
        fn seccomp_return(k: u32) -> libc::sock_filter {
            seccomp_instruction(libc::BPF_RET | libc::BPF_K, 0, 0, k)
        }

        #[cfg(target_os = "linux")]
        fn seccomp_refuse_with_eperm() -> u32 {
            seccomp_refuse_with(libc::EPERM)
        }

        /// Answer `errno` from the call without making it. The errno is the
        /// policy's own and reaches the caller as if the kernel had written it,
        /// which is why an errno alone establishes nothing about a helper.
        #[cfg(target_os = "linux")]
        fn seccomp_refuse_with(errno: libc::c_int) -> u32 {
            libc::SECCOMP_RET_ERRNO | u32::try_from(errno).expect("an errno fits u32")
        }

        #[cfg(target_os = "linux")]
        fn seccomp_syscall_number(number: libc::c_long) -> u32 {
            u32::try_from(number).expect("a syscall number fits the kernel's field")
        }

        /// The two halves of `seccomp_data.args[index]`, low word first.
        /// `struct seccomp_data` is `nr` at 0, `arch` at 4, the instruction
        /// pointer at 8, and six eight-byte arguments from 16; a BPF program
        /// loads 32 bits at a time, and which half of an argument a given
        /// offset names is the target's byte order.
        #[cfg(target_os = "linux")]
        fn seccomp_argument_words(index: u32) -> (u32, u32) {
            const SECCOMP_DATA_ARGS_OFFSET: u32 = 16;
            const ARGUMENT_WIDTH: u32 = 8;
            let base = SECCOMP_DATA_ARGS_OFFSET + index * ARGUMENT_WIDTH;
            if cfg!(target_endian = "little") {
                (base, base + 4)
            } else {
                (base + 4, base)
            }
        }

        /// Install `program` as this process's seccomp policy. A filter cannot
        /// be lifted by the process that installs it, which is why every
        /// fixture here is a subprocess helper.
        #[cfg(target_os = "linux")]
        fn install_seccomp_policy(program: &mut [libc::sock_filter]) {
            const NONE: libc::c_long = 0;
            const NO_NEW_PRIVS: libc::c_long = 1;

            let filter = libc::sock_fprog {
                len: u16::try_from(program.len()).expect("the program fits the count"),
                filter: program.as_mut_ptr(),
            };

            // SAFETY: `prctl` takes its five arguments by value and reads
            // through no pointer for `PR_SET_NO_NEW_PRIVS`, which the kernel
            // refuses unless the remaining four are 1, 0, 0 and 0.
            let allowed = unsafe {
                libc::syscall(
                    libc::SYS_prctl,
                    libc::c_long::from(libc::PR_SET_NO_NEW_PRIVS),
                    NO_NEW_PRIVS,
                    NONE,
                    NONE,
                    NONE,
                )
            };
            assert_eq!(
                allowed,
                0,
                "PR_SET_NO_NEW_PRIVS: {}",
                std::io::Error::last_os_error()
            );

            let mode = libc::c_long::from(libc::SECCOMP_MODE_FILTER);
            // SAFETY: `PR_SET_SECCOMP` reads the `sock_fprog` behind the third
            // argument and the instructions behind that program's own pointer;
            // both are live here for the call and the kernel copies them.
            let installed = unsafe {
                libc::syscall(
                    libc::SYS_prctl,
                    libc::c_long::from(libc::PR_SET_SECCOMP),
                    mode,
                    std::ptr::from_ref(&filter),
                    NONE,
                    NONE,
                )
            };
            assert_eq!(
                installed,
                0,
                "PR_SET_SECCOMP: {}",
                std::io::Error::last_os_error()
            );
        }

        /// `EPERM` for the whole `pidfd_send_signal` system call: the shape a
        /// host takes when it does not offer the call at all.
        #[cfg(target_os = "linux")]
        fn refuse_pidfd_send_signal() {
            const SECCOMP_DATA_NR_OFFSET: u32 = 0;

            let mut program = [
                seccomp_load(SECCOMP_DATA_NR_OFFSET),
                seccomp_jump_if_equal(seccomp_syscall_number(libc::SYS_pidfd_send_signal), 0, 1),
                seccomp_return(seccomp_refuse_with_eperm()),
                seccomp_return(libc::SECCOMP_RET_ALLOW),
            ];
            install_seccomp_policy(&mut program);
        }

        /// `EPERM` for `pidfd_send_signal(..., signal, ...)` and for nothing
        /// else. A filter selects on arguments, so this permits the signal `0`
        /// the acquisition probe sends and refuses the `SIGKILL` the teardown
        /// sends through the same descriptor.
        #[cfg(target_os = "linux")]
        fn refuse_pidfd_send_signal_of(signal: libc::c_int) {
            refuse_pidfd_send_signal_of_with(signal, libc::EPERM);
        }

        /// As `refuse_pidfd_send_signal_of`, with the errno the policy answers
        /// named: `EPERM` is a call this host refuses, and `ESRCH` is the
        /// answer the kernel would have given about a helper that had ended
        /// and been collected, written by something that is not the kernel.
        #[cfg(target_os = "linux")]
        fn refuse_pidfd_send_signal_of_with(signal: libc::c_int, errno: libc::c_int) {
            const SECCOMP_DATA_NR_OFFSET: u32 = 0;
            let (signal_low, signal_high) = seccomp_argument_words(1);
            let number = u32::try_from(signal).expect("a signal number fits the kernel's field");

            let mut program = [
                seccomp_load(SECCOMP_DATA_NR_OFFSET),
                seccomp_jump_if_equal(seccomp_syscall_number(libc::SYS_pidfd_send_signal), 0, 4),
                seccomp_load(signal_high),
                seccomp_jump_if_equal(0, 0, 2),
                seccomp_load(signal_low),
                seccomp_jump_if_equal(number, 1, 0),
                seccomp_return(libc::SECCOMP_RET_ALLOW),
                seccomp_return(seccomp_refuse_with(errno)),
            ];
            install_seccomp_policy(&mut program);
        }

        /// `EPERM` for a `waitid` that collects, and for nothing else: the
        /// acquisition probe passes `WNOWAIT` and leaves the helper where it
        /// was, and the teardown's own wait does not.
        #[cfg(target_os = "linux")]
        fn refuse_the_consuming_waitid() {
            refuse_the_consuming_waitid_with(libc::EPERM);
        }

        /// As `refuse_the_consuming_waitid`, with the errno the policy answers
        /// named: `EPERM` is a call this host refuses, and `ECHILD` is the
        /// answer the kernel would have given about a helper that had already
        /// been collected, written by something that is not the kernel.
        #[cfg(target_os = "linux")]
        fn refuse_the_consuming_waitid_with(errno: libc::c_int) {
            const SECCOMP_DATA_NR_OFFSET: u32 = 0;
            let (options_low, _) = seccomp_argument_words(3);
            let leaves_it_uncollected =
                u32::try_from(libc::WNOWAIT).expect("an option bit fits the kernel's field");

            let mut program = [
                seccomp_load(SECCOMP_DATA_NR_OFFSET),
                seccomp_jump_if_equal(seccomp_syscall_number(libc::SYS_waitid), 0, 3),
                seccomp_load(options_low),
                seccomp_jump_if_any_set(leaves_it_uncollected, 1, 0),
                seccomp_return(seccomp_refuse_with(errno)),
                seccomp_return(libc::SECCOMP_RET_ALLOW),
            ];
            install_seccomp_policy(&mut program);
        }

        #[cfg(target_os = "linux")]
        #[test]
        #[ignore = "subprocess helper"]
        fn refused_identity_signal_helper() {
            if std::env::var_os("UPSTROKE_REFUSED_IDENTITY_SIGNAL_HELPER").is_none() {
                return;
            }
            refuse_pidfd_send_signal();

            let (helper, helper_lifetime) = spawn_sigchld_target();

            let target = libc::c_long::from(helper);
            let flags: libc::c_long = 0;
            // SAFETY: as in `open_helper_identity`.
            let opened = unsafe { libc::syscall(libc::SYS_pidfd_open, target, flags) };
            let witness = libc::c_int::try_from(opened).expect("a descriptor fits c_int");
            assert!(
                witness >= 0,
                "the policy refused `pidfd_open`, so what follows would not be about the \
                 signal: {}",
                std::io::Error::last_os_error()
            );
            assert_eq!(
                identity_can_collect(witness),
                IdentityProbe::Available,
                "the policy refused the wait, so what follows would not be about the signal"
            );
            assert_eq!(
                identity_can_signal(witness),
                IdentityProbe::Refused,
                "the policy did not refuse the signal syscall"
            );
            close_fd(witness);

            let identity = helper_identity(helper);
            assert_eq!(
                identity, NO_HELPER_IDENTITY,
                "an identity whose signal syscall this host refuses was taken, and the \
                 teardown through it enters its blocking wait on a helper it never signalled"
            );

            let end = Reaper {
                command_fd: -1,
                ack_fd: -1,
                _command_keepalive_fd: -1,
                pid: helper,
                identity,
            }
            .abandon();
            assert_eq!(
                end.kill_errno,
                0,
                "the teardown's signal was not delivered: {}",
                std::io::Error::from_raw_os_error(end.kill_errno)
            );
            assert_eq!(
                end.waited, helper,
                "the teardown collected {} and not the helper",
                end.waited
            );
            assert!(
                libc::WIFSIGNALED(end.status) && libc::WTERMSIG(end.status) == libc::SIGKILL,
                "the helper did not end on the teardown's SIGKILL: {}",
                end.status
            );
            drop(helper_lifetime);
        }

        #[cfg(target_os = "linux")]
        #[test]
        fn an_identity_whose_signal_syscall_is_refused_is_not_taken() {
            use std::os::unix::process::CommandExt;

            let output = Command::new(std::env::current_exe().expect("test executable"))
                .args(["refused_identity_signal_helper", "--ignored", "--nocapture"])
                .env("UPSTROKE_REFUSED_IDENTITY_SIGNAL_HELPER", "1")
                .process_group(0)
                .stdin(Stdio::null())
                .output()
                .expect("run the refused-signal helper");
            assert!(
                output.status.success(),
                "refused-signal helper: {}\n{}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        #[cfg(target_os = "linux")]
        #[test]
        #[ignore = "subprocess helper"]
        fn reaper_identity_under_a_descriptor_shortage_helper() {
            if std::env::var_os("UPSTROKE_REAPER_IDENTITY_SHORTAGE_HELPER").is_none() {
                return;
            }

            let devnull = std::ffi::CString::new("/dev/null").expect("a path with no null byte");
            let open_now = std::fs::read_dir("/proc/self/fd")
                .expect("this process's own descriptors")
                .count();
            let headroom =
                u64::try_from(open_now + 64).expect("a descriptor count fits the limit word");
            // SAFETY: `getrlimit` writes one `rlimit` through the pointer and
            // `ceiling` is live for the call.
            let mut ceiling: libc::rlimit = unsafe { std::mem::zeroed() };
            let read_ceiling = unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut ceiling) };
            assert_eq!(
                read_ceiling,
                0,
                "reading RLIMIT_NOFILE: {}",
                std::io::Error::last_os_error()
            );
            let lowered = libc::rlimit {
                rlim_cur: headroom,
                rlim_max: ceiling.rlim_max,
            };
            // SAFETY: `setrlimit` reads one `rlimit` through the pointer and
            // `lowered` is live for the call. The hard limit is carried
            // forward unchanged, so the soft one can be put back below.
            let narrowed = unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &lowered) };
            assert_eq!(
                narrowed,
                0,
                "narrowing RLIMIT_NOFILE to {headroom}: {}",
                std::io::Error::last_os_error()
            );

            let mut held = Vec::new();
            loop {
                // SAFETY: `open` reads the path behind the pointer, which is
                // live for the call, and answers a descriptor or a negative
                // number.
                let fd = unsafe { libc::open(devnull.as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC) };
                if fd < 0 {
                    break;
                }
                held.push(fd);
            }
            let exhausted = last_errno();
            // Give back exactly the four slots the two pipes `spawn_reaper`
            // opens before its `fork` take, so `pidfd_open` is the fifth
            // descriptor the launch asks the kernel for and the one that is
            // refused.
            for _ in 0..4 {
                let Some(fd) = held.pop() else {
                    panic!("the fill took fewer than four descriptors, so nothing is being tested")
                };
                close_fd(fd);
            }

            let launched = spawn_reaper();

            for fd in held.drain(..) {
                close_fd(fd);
            }
            // SAFETY: as above; `ceiling` is the limit this process started
            // with and is live for the call.
            let restored = unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &ceiling) };

            assert_eq!(
                exhausted,
                libc::EMFILE,
                "the descriptor table filled for some other reason: {}",
                std::io::Error::from_raw_os_error(exhausted)
            );
            assert_eq!(
                restored,
                0,
                "restoring RLIMIT_NOFILE: {}",
                std::io::Error::last_os_error()
            );
            match launched {
                Ok(reaper) => {
                    let identity = reaper.identity;
                    reaper.cancel();
                    panic!(
                        "a launch that could not take an identity handed the reaper back with \
                         identity {identity}: every end of that helper is a `kill` and a \
                         `waitpid` on its number, and a descriptor this process ran out of is \
                         not a kernel with no identities to give"
                    );
                }
                Err(message) => assert!(
                    message.starts_with("taking an identity for the Unix cleanup reaper"),
                    "the launch failed for some other reason: {message}"
                ),
            }
        }

        #[cfg(target_os = "linux")]
        #[test]
        fn a_descriptor_shortage_is_not_a_kernel_without_identities() {
            use std::os::unix::process::CommandExt;

            let output = Command::new(std::env::current_exe().expect("test executable"))
                .args([
                    "reaper_identity_under_a_descriptor_shortage_helper",
                    "--ignored",
                    "--nocapture",
                ])
                .env("UPSTROKE_REAPER_IDENTITY_SHORTAGE_HELPER", "1")
                .process_group(0)
                .stdin(Stdio::null())
                .output()
                .expect("run the descriptor-shortage helper");
            assert!(
                output.status.success(),
                "descriptor-shortage helper: {}\n{}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        /// A descriptor naming `helper`, opened the way `open_helper_identity`
        /// opens one and by hand, so a policy can be pinned against it before
        /// the acquisition under test runs.
        #[cfg(target_os = "linux")]
        fn identity_by_hand(helper: libc::pid_t) -> libc::c_int {
            let target = libc::c_long::from(helper);
            let flags: libc::c_long = 0;
            // SAFETY: as in `open_helper_identity`.
            let opened = unsafe { libc::syscall(libc::SYS_pidfd_open, target, flags) };
            let witness = libc::c_int::try_from(opened).expect("a descriptor fits c_int");
            assert!(
                witness >= 0,
                "the policy refused `pidfd_open`, so what follows would not be about the call \
                 under test: {}",
                std::io::Error::last_os_error()
            );
            witness
        }

        #[cfg(target_os = "linux")]
        #[test]
        #[ignore = "subprocess helper"]
        fn refused_sigkill_identity_helper() {
            if std::env::var_os("UPSTROKE_REFUSED_SIGKILL_IDENTITY_HELPER").is_none() {
                return;
            }
            refuse_pidfd_send_signal_of(libc::SIGKILL);

            let (helper, helper_lifetime) = spawn_sigchld_target();
            let witness = identity_by_hand(helper);
            assert_eq!(
                identity_can_collect(witness),
                IdentityProbe::Available,
                "the policy refused the wait, so what follows would not be about the signal"
            );
            assert_eq!(
                identity_can_signal(witness),
                IdentityProbe::Available,
                "the policy refused the signal `0` the probe sends, so the probe would have \
                 caught it and this is not the argument-sensitive case"
            );
            let flags: libc::c_long = 0;
            // SAFETY: as in `signal_helper`, with the signal the teardown
            // sends rather than the one the probe sends.
            let refused = unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    libc::c_long::from(witness),
                    libc::c_long::from(libc::SIGKILL),
                    std::ptr::null_mut::<libc::siginfo_t>(),
                    flags,
                )
            };
            let refused_errno = last_errno();
            assert_eq!(
                refused, -1,
                "the policy did not refuse `pidfd_send_signal` for SIGKILL, so the probe and \
                 the teardown are not being told apart"
            );
            assert_eq!(
                refused_errno,
                libc::EPERM,
                "the refusal came from somewhere other than the policy: {}",
                std::io::Error::from_raw_os_error(refused_errno)
            );
            close_fd(witness);

            let identity = helper_identity(helper);
            assert!(
                identity >= 0,
                "both probes pass under this policy, so the identity is taken and the teardown \
                 through it is what has to answer for the refusal"
            );

            // Asserted before any collection: a wait through an identity on a
            // helper nothing has killed does not return, and a test that hangs
            // reports nothing.
            let killed = signal_helper(helper, identity, libc::SIGKILL);
            let kill_errno = if killed == 0 { 0 } else { last_errno() };
            assert_eq!(
                killed,
                0,
                "the teardown's own SIGKILL was not delivered and nothing handled the refusal, \
                 so the collection that follows it waits on a helper still running: {}",
                std::io::Error::from_raw_os_error(kill_errno)
            );

            let end = Reaper {
                command_fd: -1,
                ack_fd: -1,
                _command_keepalive_fd: -1,
                pid: helper,
                identity,
            }
            .abandon();
            assert_eq!(
                end.kill_errno, 0,
                "the teardown reported a signal that was not delivered"
            );
            assert_eq!(
                end.waited, helper,
                "the teardown collected {} and not the helper",
                end.waited
            );
            assert!(
                libc::WIFSIGNALED(end.status) && libc::WTERMSIG(end.status) == libc::SIGKILL,
                "the helper did not end on the teardown's SIGKILL: {}",
                end.status
            );
            close_fd(identity);
            drop(helper_lifetime);
        }

        #[cfg(target_os = "linux")]
        #[test]
        fn a_refused_signal_is_answered_where_the_signal_is_sent() {
            use std::os::unix::process::CommandExt;

            let output = Command::new(std::env::current_exe().expect("test executable"))
                .args([
                    "refused_sigkill_identity_helper",
                    "--ignored",
                    "--nocapture",
                ])
                .env("UPSTROKE_REFUSED_SIGKILL_IDENTITY_HELPER", "1")
                .process_group(0)
                .stdin(Stdio::null())
                .output()
                .expect("run the refused-SIGKILL helper");
            assert!(
                output.status.success(),
                "refused-SIGKILL helper: {}\n{}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        #[cfg(target_os = "linux")]
        #[test]
        #[ignore = "subprocess helper"]
        fn refused_consuming_wait_identity_helper() {
            if std::env::var_os("UPSTROKE_REFUSED_CONSUMING_WAIT_HELPER").is_none() {
                return;
            }
            refuse_the_consuming_waitid();

            let (helper, helper_lifetime) = spawn_sigchld_target();
            let witness = identity_by_hand(helper);
            assert_eq!(
                identity_can_collect(witness),
                IdentityProbe::Available,
                "the policy refused the probe's own wait, so the probe would have caught it \
                 and this is not the argument-sensitive case"
            );
            let Ok(id) = libc::id_t::try_from(witness) else {
                panic!("a descriptor fits id_t")
            };
            // SAFETY: as in `collect_through_identity`, with `WNOHANG` added so
            // that a policy which does *not* refuse this call answers at once
            // rather than waiting on a helper that is still running.
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            // SAFETY: `witness` names the helper and `info` is live for the
            // call.
            let refused = unsafe {
                libc::waitid(libc::P_PIDFD, id, &mut info, libc::WEXITED | libc::WNOHANG)
            };
            let refused_errno = last_errno();
            assert_eq!(
                refused, -1,
                "the policy did not refuse a `waitid` that collects, so the probe's wait and \
                 the teardown's are not being told apart"
            );
            assert_eq!(
                refused_errno,
                libc::EPERM,
                "the refusal came from somewhere other than the policy: {}",
                std::io::Error::from_raw_os_error(refused_errno)
            );
            close_fd(witness);

            let identity = helper_identity(helper);
            assert!(
                identity >= 0,
                "the probes pass under this policy, so the identity is taken and the teardown \
                 through it is what has to answer for the refusal"
            );

            let end = Reaper {
                command_fd: -1,
                ack_fd: -1,
                _command_keepalive_fd: -1,
                pid: helper,
                identity,
            }
            .abandon();
            assert_eq!(
                end.kill_errno,
                0,
                "the teardown's SIGKILL was not delivered: {}",
                std::io::Error::from_raw_os_error(end.kill_errno)
            );
            assert_eq!(
                end.waited,
                helper,
                "the teardown left the helper behind: it collected {} and reported {}",
                end.waited,
                std::io::Error::from_raw_os_error(end.wait_errno)
            );
            assert!(
                libc::WIFSIGNALED(end.status) && libc::WTERMSIG(end.status) == libc::SIGKILL,
                "the helper did not end on the teardown's SIGKILL: {}",
                end.status
            );
            close_fd(identity);
            drop(helper_lifetime);
        }

        #[cfg(target_os = "linux")]
        #[test]
        fn a_refused_collection_is_answered_where_the_wait_is_made() {
            use std::os::unix::process::CommandExt;

            let output = Command::new(std::env::current_exe().expect("test executable"))
                .args([
                    "refused_consuming_wait_identity_helper",
                    "--ignored",
                    "--nocapture",
                ])
                .env("UPSTROKE_REFUSED_CONSUMING_WAIT_HELPER", "1")
                .process_group(0)
                .stdin(Stdio::null())
                .output()
                .expect("run the refused-collection helper");
            assert!(
                output.status.success(),
                "refused-collection helper: {}\n{}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        /// The probe is what every reading of an identity errno below now
        /// rests on, and a probe that reported nothing would leave every one of
        /// those readings false without failing a single test about a policy:
        /// each would simply fall back to the number, which is what they assert
        /// under a policy anyway. So this asserts the other direction, on a host
        /// with no policy at all.
        #[cfg(target_os = "linux")]
        #[test]
        fn the_identity_calls_this_host_offers_are_established_before_one_is_made() {
            let established = established_identity_calls().expect("this host answered the probe");
            for (call, what) in [
                (IDENTITY_PROBED, "a child reported at all"),
                (
                    IDENTITY_NAMES,
                    "`pidfd_open` named a process that was there",
                ),
                (
                    IDENTITY_SIGNALS,
                    "`pidfd_send_signal(..., SIGKILL, ...)` answered for one",
                ),
                (
                    IDENTITY_COLLECTS,
                    "`waitid(P_PIDFD, ..., WEXITED)` collected one",
                ),
            ] {
                assert_ne!(
                    established & call,
                    0,
                    "this Linux host offers the identity calls and the probe did not find that \
                     {what}: every errno read below would fall back to the number, which is \
                     what a policy test asserts too",
                );
            }
            assert_eq!(
                established,
                IDENTITY_CALLS.load(Ordering::SeqCst),
                "the answer was not remembered for the calls that follow it"
            );
        }

        /// `SECCOMP_RET_KILL_PROCESS` for `pidfd_open` and for nothing else:
        /// the disposition a disallowed call draws by default under systemd's
        /// `SystemCallFilter=`. The call does not return, so there is no errno
        /// for the caller to read and no branch for it to take.
        #[cfg(target_os = "linux")]
        fn kill_the_caller_of_pidfd_open() {
            const SECCOMP_DATA_NR_OFFSET: u32 = 0;

            let mut program = [
                seccomp_load(SECCOMP_DATA_NR_OFFSET),
                seccomp_jump_if_equal(seccomp_syscall_number(libc::SYS_pidfd_open), 0, 1),
                seccomp_return(libc::SECCOMP_RET_KILL_PROCESS),
                seccomp_return(libc::SECCOMP_RET_ALLOW),
            ];
            install_seccomp_policy(&mut program);
        }

        /// Whether this process has a child nothing collected: the pid of one
        /// if it has, and `-1` with `ECHILD` if it has not. The wait blocks and
        /// there is no clock in it — with every helper collected there is no
        /// child left and it answers at once, and an abandoned helper is either
        /// already a zombie or is ending on its closed command pipe, which is
        /// what this returns.
        #[cfg(target_os = "linux")]
        fn abandoned_child() -> libc::pid_t {
            loop {
                // SAFETY: `waitpid` writes no status through the null pointer
                // and reaches none but this process's own children.
                let waited = unsafe { libc::waitpid(-1, std::ptr::null_mut(), 0) };
                if waited > 0 || !last_errno_is_interrupted() {
                    return waited;
                }
            }
        }

        #[cfg(target_os = "linux")]
        #[test]
        #[ignore = "subprocess helper"]
        fn fatal_identity_policy_helper() {
            if std::env::var_os("UPSTROKE_FATAL_IDENTITY_POLICY_HELPER").is_none() {
                return;
            }
            kill_the_caller_of_pidfd_open();

            // A launch, in full, on a host that ends the process which asks it
            // for a name. Reaching the statement after this one is the whole
            // assertion: a `pidfd_open` made here never returns.
            let reaper =
                spawn_reaper().expect("a launch under a policy that kills on `pidfd_open`");
            let identity = reaper.identity;
            reaper.cancel();
            assert_eq!(
                identity, NO_HELPER_IDENTITY,
                "a call this host kills the caller of answered with a descriptor"
            );
            assert_eq!(
                PENDING_TERMINATION.load(Ordering::SeqCst),
                0,
                "the cancelled reaper armed process-wide termination"
            );
            let abandoned = abandoned_child();
            assert!(
                abandoned < 0 && last_errno() == libc::ECHILD,
                "the launch left {abandoned} behind uncollected"
            );
        }

        #[cfg(target_os = "linux")]
        #[test]
        fn a_policy_that_kills_on_the_identity_call_does_not_kill_this_process() {
            use std::os::unix::process::CommandExt;

            let output = Command::new(std::env::current_exe().expect("test executable"))
                .args(["fatal_identity_policy_helper", "--ignored", "--nocapture"])
                .env("UPSTROKE_FATAL_IDENTITY_POLICY_HELPER", "1")
                .process_group(0)
                .stdin(Stdio::null())
                .output()
                .expect("run the fatal-policy helper");
            assert!(
                output.status.success(),
                "fatal-policy helper: {}\n{}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        /// Fill this process's descriptor table, leaving exactly `spare` slots
        /// free. The soft limit is lowered first so the fill is bounded; the
        /// hard limit is carried forward unchanged so `release_descriptors` can
        /// put the soft one back.
        #[cfg(target_os = "linux")]
        fn fill_descriptors_leaving(spare: usize) -> (Vec<libc::c_int>, libc::rlimit) {
            let devnull = std::ffi::CString::new("/dev/null").expect("a path with no null byte");
            let open_now = std::fs::read_dir("/proc/self/fd")
                .expect("this process's own descriptors")
                .count();
            let headroom =
                u64::try_from(open_now + 64).expect("a descriptor count fits the limit word");
            // SAFETY: `getrlimit` writes one `rlimit` through the pointer and
            // `ceiling` is live for the call.
            let mut ceiling: libc::rlimit = unsafe { std::mem::zeroed() };
            let read_ceiling = unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut ceiling) };
            assert_eq!(
                read_ceiling,
                0,
                "reading RLIMIT_NOFILE: {}",
                std::io::Error::last_os_error()
            );
            let lowered = libc::rlimit {
                rlim_cur: headroom,
                rlim_max: ceiling.rlim_max,
            };
            // SAFETY: `setrlimit` reads one `rlimit` through the pointer and
            // `lowered` is live for the call.
            let narrowed = unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &lowered) };
            assert_eq!(
                narrowed,
                0,
                "narrowing RLIMIT_NOFILE to {headroom}: {}",
                std::io::Error::last_os_error()
            );

            let mut held = Vec::new();
            loop {
                // SAFETY: `open` reads the path behind the pointer, which is
                // live for the call, and answers a descriptor or a negative
                // number.
                let fd = unsafe { libc::open(devnull.as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC) };
                if fd < 0 {
                    break;
                }
                held.push(fd);
            }
            let exhausted = last_errno();
            assert_eq!(
                exhausted,
                libc::EMFILE,
                "the descriptor table filled for some other reason: {}",
                std::io::Error::from_raw_os_error(exhausted)
            );
            for _ in 0..spare {
                let Some(fd) = held.pop() else {
                    panic!(
                        "the fill took fewer than {spare} descriptors, so nothing is being tested"
                    )
                };
                close_fd(fd);
            }
            (held, ceiling)
        }

        #[cfg(target_os = "linux")]
        fn release_descriptors(held: Vec<libc::c_int>, ceiling: libc::rlimit) {
            for fd in held {
                close_fd(fd);
            }
            // SAFETY: as in `fill_descriptors_leaving`; `ceiling` is the limit
            // this process started with and is live for the call.
            let restored = unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &ceiling) };
            assert_eq!(
                restored,
                0,
                "restoring RLIMIT_NOFILE: {}",
                std::io::Error::last_os_error()
            );
        }

        #[cfg(target_os = "linux")]
        #[test]
        #[ignore = "subprocess helper"]
        fn identity_shortage_child_cleanup_helper() {
            if std::env::var_os("UPSTROKE_IDENTITY_SHORTAGE_CLEANUP_HELPER").is_none() {
                return;
            }
            const LAUNCHES: usize = 3;
            const REAPER: usize = 0;
            let policy = SignalPolicy {
                termination_mask: 0,
                guard_wake_mask: 0,
                stop_mask: 0,
                job_control: false,
            };

            // Four descriptors are the reaper's two pipes and eight are the
            // guard's four. Both are opened before the launch's `fork`, so the
            // identity is the first thing it asks the kernel for that there is
            // no descriptor left to answer.
            let mut left_behind = Vec::new();
            for site in [REAPER, 1] {
                let (what, spare) = if site == REAPER {
                    ("Unix cleanup reaper", 4)
                } else {
                    ("Unix job-control guard", 8)
                };
                let (held, ceiling) = fill_descriptors_leaving(spare);
                let mut failures = Vec::with_capacity(LAUNCHES);
                for _ in 0..LAUNCHES {
                    failures.push(if site == REAPER {
                        spawn_reaper().err()
                    } else {
                        spawn_guard(policy).err()
                    });
                }
                release_descriptors(held, ceiling);

                for failure in &failures {
                    let Some(message) = failure else {
                        panic!("a launch with no descriptor left for an identity was accepted")
                    };
                    assert!(
                        message.starts_with(&format!("taking an identity for the {what}")),
                        "the launch failed for some other reason: {message}"
                    );
                }
                let mut abandoned = Vec::new();
                loop {
                    let waited = abandoned_child();
                    if waited < 0 {
                        assert_eq!(
                            last_errno(),
                            libc::ECHILD,
                            "waiting for the children of {LAUNCHES} {what} launches: {}",
                            std::io::Error::last_os_error()
                        );
                        break;
                    }
                    abandoned.push(waited);
                }
                left_behind.push((what, abandoned));
            }

            let leaked = left_behind
                .iter()
                .filter(|(_, abandoned)| !abandoned.is_empty())
                .map(|(what, abandoned)| format!("{what} left {abandoned:?}"))
                .collect::<Vec<_>>();
            assert!(
                leaked.is_empty(),
                "{LAUNCHES} launches that could not name their helper left it behind \
                 uncollected -- {}: closing the command pipe lets a helper exit, and a child \
                 nothing waits for stays this process's for the rest of its life",
                leaked.join("; ")
            );
        }

        #[cfg(target_os = "linux")]
        #[test]
        fn a_launch_that_could_not_name_its_helper_still_collects_it() {
            use std::os::unix::process::CommandExt;

            let output = Command::new(std::env::current_exe().expect("test executable"))
                .args([
                    "identity_shortage_child_cleanup_helper",
                    "--ignored",
                    "--nocapture",
                ])
                .env("UPSTROKE_IDENTITY_SHORTAGE_CLEANUP_HELPER", "1")
                .process_group(0)
                .stdin(Stdio::null())
                .output()
                .expect("run the shortage-cleanup helper");
            assert!(
                output.status.success(),
                "shortage-cleanup helper: {}\n{}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        #[cfg(target_os = "linux")]
        #[test]
        #[ignore = "subprocess helper"]
        fn forged_esrch_identity_helper() {
            if std::env::var_os("UPSTROKE_FORGED_ESRCH_IDENTITY_HELPER").is_none() {
                return;
            }
            refuse_pidfd_send_signal_of_with(libc::SIGKILL, libc::ESRCH);

            let (helper, helper_lifetime) = spawn_sigchld_target();
            let witness = identity_by_hand(helper);
            assert_eq!(
                identity_can_collect(witness),
                IdentityProbe::Available,
                "the policy answered the wait, so what follows would not be about the signal"
            );
            assert_eq!(
                identity_can_signal(witness),
                IdentityProbe::Available,
                "the policy answered the signal `0` the probe sends, so this is not the \
                 argument-sensitive case"
            );
            let flags: libc::c_long = 0;
            // SAFETY: as in `signal_helper`, with the signal the teardown sends
            // rather than the one the probe sends.
            let forged = unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    libc::c_long::from(witness),
                    libc::c_long::from(libc::SIGKILL),
                    std::ptr::null_mut::<libc::siginfo_t>(),
                    flags,
                )
            };
            let forged_errno = last_errno();
            assert_eq!(
                forged, -1,
                "the policy did not answer `pidfd_send_signal` for SIGKILL itself, so nothing \
                 here is forged"
            );
            assert_eq!(
                forged_errno,
                libc::ESRCH,
                "the refusal came from somewhere other than the policy: {}",
                std::io::Error::from_raw_os_error(forged_errno)
            );
            close_fd(witness);
            // SAFETY: `helper` is this test's own child and nothing has
            // collected it, so it is still a process this wait can answer for.
            let alive = unsafe { libc::waitpid(helper, std::ptr::null_mut(), libc::WNOHANG) };
            assert_eq!(
                alive, 0,
                "the helper the policy is about to answer `ESRCH` for has ended, so the answer \
                 would have been the kernel's own"
            );

            let identity = helper_identity(helper);
            assert!(
                identity >= 0,
                "both probes pass under this policy, so the identity is taken and the teardown \
                 through it is what has to answer for the forged errno"
            );

            // Asserted before any collection: a wait through an identity on a
            // helper nothing has killed does not return, and a test that hangs
            // reports nothing.
            let killed = signal_helper(helper, identity, libc::SIGKILL);
            let kill_errno = if killed == 0 { 0 } else { last_errno() };
            assert_eq!(
                killed,
                0,
                "an `ESRCH` this policy wrote was read as the helper having ended and been \
                 collected: nothing was signalled, and the collection that follows waits on a \
                 helper still running: {}",
                std::io::Error::from_raw_os_error(kill_errno)
            );

            let end = Reaper {
                command_fd: -1,
                ack_fd: -1,
                _command_keepalive_fd: -1,
                pid: helper,
                identity,
            }
            .abandon();
            assert_eq!(
                end.kill_errno, 0,
                "the teardown reported a signal that was not delivered"
            );
            assert_eq!(
                end.waited, helper,
                "the teardown collected {} and not the helper",
                end.waited
            );
            assert!(
                libc::WIFSIGNALED(end.status) && libc::WTERMSIG(end.status) == libc::SIGKILL,
                "the helper did not end on the teardown's SIGKILL: {}",
                end.status
            );
            close_fd(identity);
            drop(helper_lifetime);
        }

        #[cfg(target_os = "linux")]
        #[test]
        fn an_esrch_a_policy_wrote_is_not_proof_the_helper_ended() {
            use std::os::unix::process::CommandExt;

            let output = Command::new(std::env::current_exe().expect("test executable"))
                .args(["forged_esrch_identity_helper", "--ignored", "--nocapture"])
                .env("UPSTROKE_FORGED_ESRCH_IDENTITY_HELPER", "1")
                .process_group(0)
                .stdin(Stdio::null())
                .output()
                .expect("run the forged-ESRCH helper");
            assert!(
                output.status.success(),
                "forged-ESRCH helper: {}\n{}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        #[cfg(target_os = "linux")]
        #[test]
        #[ignore = "subprocess helper"]
        fn forged_echild_identity_helper() {
            if std::env::var_os("UPSTROKE_FORGED_ECHILD_IDENTITY_HELPER").is_none() {
                return;
            }
            refuse_the_consuming_waitid_with(libc::ECHILD);

            let reaper = spawn_reaper().expect("spawn private reaper");
            let (pid, identity) = (reaper.pid, reaper.identity);
            assert!(
                identity >= 0,
                "both probes pass under this policy, so the identity is taken and the collection \
                 through it is what has to answer for the forged errno"
            );
            reaper.cancel();

            let abandoned = abandoned_child();
            assert!(
                abandoned < 0 && last_errno() == libc::ECHILD,
                "an `ECHILD` this policy wrote was read as the helper having been collected: \
                 the reaper {pid} this launch forked was left behind, and {abandoned} is what \
                 the wait for it answered"
            );
            assert_eq!(
                PENDING_TERMINATION.load(Ordering::SeqCst),
                0,
                "the cancelled reaper armed process-wide termination"
            );
        }

        #[cfg(target_os = "linux")]
        #[test]
        fn an_echild_a_policy_wrote_is_not_proof_the_helper_was_collected() {
            use std::os::unix::process::CommandExt;

            let output = Command::new(std::env::current_exe().expect("test executable"))
                .args(["forged_echild_identity_helper", "--ignored", "--nocapture"])
                .env("UPSTROKE_FORGED_ECHILD_IDENTITY_HELPER", "1")
                .process_group(0)
                .stdin(Stdio::null())
                .output()
                .expect("run the forged-ECHILD helper");
            assert!(
                output.status.success(),
                "forged-ECHILD helper: {}\n{}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }

        #[test]
        #[ignore = "subprocess helper"]
        fn helper_ready_failure_helper() {
            if std::env::var_os("UPSTROKE_HELPER_READY_FAILURE_HELPER").is_none() {
                return;
            }
            let policy = SignalPolicy {
                termination_mask: 0,
                guard_wake_mask: 0,
                stop_mask: 0,
                job_control: false,
            };
            let launches = [
                ("Unix cleanup reaper", spawn_reaper().err()),
                ("Unix job-control guard", spawn_guard(policy).err()),
            ];
            for (prefix, launch) in launches {
                let Some(message) = launch else {
                    panic!("{prefix} was accepted as initialized after ending before READY")
                };
                assert!(
                    message.starts_with(&format!("{prefix} did not initialize; waited ")),
                    "the READY failure said nothing about the wait: {message}"
                );
                assert!(
                    message.contains(&format!(" of {HELPER_READY_BUDGET:?}; descriptor ceiling ")),
                    "the READY failure carried neither the budget nor the ceiling: {message}"
                );
                assert!(
                    message.contains(
                        "ending it: SIGKILL was delivered, and the wait collected \
                                      it, having already exited with status 7"
                    ),
                    "the teardown's own answers did not reach the message: {message}"
                );
                assert!(
                    message.contains("; the acknowledgement pipe closed with no report; "),
                    "a helper that exited without a report was not seen to close its pipe: \
                     {message}"
                );
            }
            assert_eq!(
                PENDING_TERMINATION.load(Ordering::SeqCst),
                0,
                "a helper that missed READY armed process-wide termination"
            );
            // SAFETY: `waitpid(-1, WNOHANG)` inspects only this process's
            // children, and both helpers were collected by their own teardown.
            let waited = unsafe { libc::waitpid(-1, std::ptr::null_mut(), libc::WNOHANG) };
            assert!(
                waited < 0 && !last_errno_is_interrupted(),
                "a helper was left behind (waitpid(-1, WNOHANG) returned {waited})"
            );
        }

        /// Collect `pid`, retrying an interrupted wait, so that everything the
        /// child held is closed before the test looks at the pipe.
        fn reap(pid: libc::pid_t) -> libc::c_int {
            let mut status = 0;
            loop {
                // SAFETY: `pid` is a child this test forked and has not reaped.
                let waited = unsafe { libc::waitpid(pid, &mut status, 0) };
                if waited == pid {
                    return status;
                }
                assert!(
                    waited < 0 && last_errno_is_interrupted(),
                    "waitpid({pid}): {}",
                    std::io::Error::last_os_error()
                );
            }
        }

        /// Witnessed against `wait_readable` polling the descriptor on macOS:
        /// `poll(2)` on a Darwin FIFO reports data and never the last writer's
        /// close, so the wait ran to its budget and answered `TimedOut` for a
        /// child that was already collected. There is no clock in this test:
        /// the child is reaped before the wait begins, so its write end is
        /// closed whatever the scheduler does.
        #[test]
        fn a_helper_that_has_already_exited_ends_the_acknowledgement_wait_at_end_of_file() {
            let ack = create_cloexec_pipe().expect("an acknowledgement pipe");
            // SAFETY: the forked child calls only `_exit`.
            let pid = unsafe { libc::fork() };
            if pid == 0 {
                unsafe { libc::_exit(0) };
            }
            assert!(
                pid > 0,
                "fork a stand-in helper: {}",
                std::io::Error::last_os_error()
            );
            close_fd(ack[1]);
            let status = reap(pid);
            assert!(
                libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0,
                "the stand-in helper did not exit cleanly: {status}"
            );
            let read = read_guard_ack(ack[0], HELPER_READY_BUDGET);
            close_fd(ack[0]);
            assert_eq!(
                read,
                AckRead::EndOfFile,
                "a pipe whose only other writer has been collected did not read as closed"
            );
        }

        #[test]
        fn a_guard_whose_conductor_is_gone_ends_while_its_pipes_stay_open() {
            let [command_read, command_write] =
                create_cloexec_pipe().expect("a guard command pipe");
            let [ack_read, ack_write] =
                create_cloexec_pipe().expect("a guard acknowledgement pipe");
            let [wake_read, wake_write] = create_cloexec_pipe().expect("a guard wake pipe");
            let [probe_read, probe_write] = create_cloexec_pipe().expect("a guard probe pipe");
            // Resolved before either fork, as `spawn_guard` resolves it: the
            // multithreaded child may call only async-signal-safe primitives.
            let open_max = unsafe { libc::sysconf(libc::_SC_OPEN_MAX) };
            let open_max = libc::c_int::try_from(open_max).expect("a descriptor ceiling");
            assert!(open_max > 0, "a descriptor ceiling: {open_max}");
            // SAFETY: the forked child calls only `_exit`.
            let departed = unsafe { libc::fork() };
            if departed == 0 {
                unsafe { libc::_exit(0) };
            }
            assert!(
                departed > 0,
                "fork a stand-in conductor: {}",
                std::io::Error::last_os_error()
            );
            let status = reap(departed);
            assert!(
                libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0,
                "the stand-in conductor did not exit cleanly: {status}"
            );

            // SAFETY: the forked child closes its parent-side descriptors and
            // enters `guard_loop`, which uses only libc syscalls and ends in
            // `_exit`.
            let guard = unsafe { libc::fork() };
            if guard == 0 {
                close_fd(command_write);
                close_fd(ack_read);
                close_fd(wake_write);
                close_fd(probe_read);
                close_inherited_fds(&[command_read, ack_write, wake_read, probe_write], open_max);
                guard_loop(
                    departed,
                    command_read,
                    ack_write,
                    wake_read,
                    probe_write,
                    departed,
                );
            }
            assert!(
                guard > 0,
                "fork a guard: {}",
                std::io::Error::last_os_error()
            );
            close_fd(command_read);
            close_fd(ack_write);
            close_fd(wake_read);
            close_fd(probe_write);

            let end = |what: &str| -> String {
                // SAFETY: `guard` is this test's own child and has not been
                // collected.
                unsafe {
                    let _ = libc::kill(guard, libc::SIGKILL);
                }
                let killed = reap(guard);
                format!("{what}; it was killed and collected: {killed}")
            };
            let said_ready = await_ready(ack_read, GUARD_READY, HELPER_READY_BUDGET);
            let outcome = if said_ready == ReadyWait::Ready {
                wait_for_lifetime_target(guard).map_or_else(
                    || {
                        Err(end(
                            "the guard was still waiting with its recorded parent gone and every \
                             command and wake writer open",
                        ))
                    },
                    Ok,
                )
            } else {
                Err(end(&format!(
                    "the guard never reached its wait: {}",
                    describe_ready_wait("guard", said_ready, &[])
                )))
            };
            for fd in [command_write, ack_read, wake_write, probe_read] {
                close_fd(fd);
            }
            let status = match outcome {
                Ok(status) => status,
                Err(report) => panic!("{report}"),
            };
            assert!(
                libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0,
                "the guard did not end cleanly: {status}"
            );
        }

        /// Witnessed against a `describe_setup_failure` that drops the lease
        /// path or the errno, and against a `decode` that accepts a frame
        /// without the marker.
        #[test]
        fn a_setup_failure_report_names_the_step_the_lease_and_the_errno() {
            let failure = SetupFailure {
                step: SetupStep::LockCleanupLease,
                index: 0,
                errno: libc::EWOULDBLOCK,
            };
            let ack = create_cloexec_pipe().expect("an acknowledgement pipe");
            assert!(write_raw(ack[1], &failure.encode()), "queue the report");
            let wait = await_ready(ack[0], REAPER_READY, HELPER_READY_BUDGET);
            close_fd(ack[0]);
            close_fd(ack[1]);
            assert_eq!(wait, ReadyWait::SetupFailed(failure));

            let lease = std::ffi::CString::new("/runs/01J/cleanup.lock").expect("a path");
            let described = describe_ready_wait("reaper", wait, std::slice::from_ref(&lease));
            let errno_text = std::io::Error::from_raw_os_error(libc::EWOULDBLOCK).to_string();
            assert_eq!(
                described,
                format!(
                    "the reaper reported that taking the shared lock on the cleanup lease \
                     /runs/01J/cleanup.lock failed: {errno_text}"
                )
            );
            let unnamed = describe_ready_wait(
                "reaper",
                ReadyWait::SetupFailed(SetupFailure {
                    index: 3,
                    ..failure
                }),
                std::slice::from_ref(&lease),
            );
            assert!(
                unnamed.contains("the cleanup lease number 3 failed"),
                "a lease past the ones the parent knows was not named by position: {unnamed}"
            );

            let mut forged = failure.encode();
            forged[0] = REAPER_READY;
            assert_eq!(
                SetupFailure::decode(forged),
                None,
                "a frame without the marker decoded"
            );
            let mut unknown_step = failure.encode();
            unknown_step[1] = 0xff;
            assert_eq!(
                SetupFailure::decode(unknown_step),
                None,
                "an unknown step decoded"
            );
            for step in [
                SetupStep::SignalDispositions,
                SetupStep::OwnProcessGroup,
                SetupStep::OpenCleanupLease,
                SetupStep::LockCleanupLease,
                SetupStep::ForkProbe,
            ] {
                assert_eq!(SetupStep::from_byte(step.to_byte()), Some(step));
            }
        }

        /// A report cut short by the writer's exit is not misread as a step.
        #[test]
        fn a_report_cut_short_is_not_read_as_a_step() {
            let ack = create_cloexec_pipe().expect("an acknowledgement pipe");
            assert!(
                write_raw(ack[1], &[HELPER_ABORT, 4]),
                "queue a partial report"
            );
            close_fd(ack[1]);
            let wait = await_ready(ack[0], REAPER_READY, HELPER_READY_BUDGET);
            close_fd(ack[0]);
            assert_eq!(wait, ReadyWait::TruncatedReport);
        }

        /// End to end through `spawn_reaper`: a lease another holder has
        /// exclusively refuses the reaper's shared lock, the reaper reports
        /// which lease and which call before it ends, and the parent's failure
        /// names both. Witnessed against a child that exits without reporting
        /// (the message then says the pipe closed with no report) and against
        /// a parent that waits its whole budget for a child that has ended.
        #[test]
        fn a_reaper_refused_its_cleanup_lease_says_which_lease_and_why() {
            use std::os::fd::AsRawFd;

            let public = std::env::temp_dir().join(format!(
                "upstroke-ready-lease-{}-{}",
                std::process::id(),
                crate::ulid::ulid()
            ));
            std::fs::create_dir_all(&public).expect("a run directory");
            let lock = crate::rundir::RunLock::acquire(&public).expect("take the run lock");
            let scope = lock.enter_cleanup_scope();
            let paths = crate::rundir::active_cleanup_lease_paths();
            assert_eq!(
                paths.len(),
                1,
                "exactly one cleanup lease is active: {paths:?}"
            );
            let holder = std::fs::File::open(paths.first().expect("the active lease"))
                .expect("open the lease file");
            // SAFETY: `holder` is open for the duration of the call.
            let exclusive =
                unsafe { libc::flock(holder.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
            assert_eq!(
                exclusive,
                0,
                "hold the lease exclusively: {}",
                std::io::Error::last_os_error()
            );

            let launch = spawn_reaper();
            drop(holder);
            drop(scope);
            drop(lock);
            let _ = std::fs::remove_dir_all(&public);

            let message = match launch {
                Err(message) => message,
                Ok(reaper) => {
                    reaper.cancel();
                    panic!("a reaper whose shared lock was refused was accepted as initialized")
                }
            };
            assert!(
                message.contains(&format!(
                    "; the reaper reported that taking the shared lock on the cleanup lease {} \
                     failed: ",
                    paths.first().expect("the active lease").display()
                )),
                "the refused lease was not named: {message}"
            );
            assert!(
                message.contains("having already exited with status 1"),
                "the reaper's own exit did not reach the message: {message}"
            );
        }

        #[test]
        fn a_helper_that_never_acknowledged_reports_what_ending_it_answered() {
            use std::os::unix::process::CommandExt;

            let output = Command::new(std::env::current_exe().expect("test executable"))
                .args(["helper_ready_failure_helper", "--ignored", "--nocapture"])
                .env("UPSTROKE_HELPER_READY_FAILURE_HELPER", "1")
                .env("UPSTROKE_TEST_HELPER_EXIT_BEFORE_READY", "7")
                .process_group(0)
                .stdin(Stdio::null())
                .output()
                .expect("run the helper READY failure helper");
            assert!(
                output.status.success(),
                "helper READY failure helper: {}\n{}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    pub(crate) fn run_with_timeout(
        command: Command,
        stdin_data: &str,
        timeout: Duration,
    ) -> Result<ProcessOutput, UpstrokeError> {
        run_with_timeout_at(
            ProcessSite::Spawn,
            ProcessSite::Terminate,
            command,
            stdin_data.as_bytes(),
            timeout,
            &mut NoHooks,
        )
    }

    pub(crate) mod readiness;
}

#[cfg(test)]
mod tests;
