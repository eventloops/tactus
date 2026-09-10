// Allowlist placement: the **funnel section** of `effects/allowlist.toml`, by
// attachment to `src/workspace_manager.rs` -- the shape
// `src/runner/container/tests.rs` and `src/agent/proc/test_support/readiness.rs`
// established for a funnel's out-of-line child. This file builds the scratch
// repositories the Worktree/Snapshot/Ref/Object suites measure against, so it
// names `fs::write`, `fs::create_dir_all` and `std::process::Command` directly.
//
// `PR6-LANEF-004`: a Rust lint level is scoped by the MODULE TREE and not by
// the file, so without an attribute here the parent's inner allow of all three
// would reach this file silently and no reviewed record would name the file
// doing the work. `clippy::disallowed_macros` is RE-DENIED rather than
// inherited -- measured at zero sites -- so a `println!` here is still a build
// error. `decisions.effect_site_inventory.mechanism` (2).
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]
#![deny(clippy::disallowed_macros)]

use super::*;

// `OsStr` came from the parent's import list until the `m4-workspace` split
// moved its last production user into a child; named here for the same reason.
use std::cell::{Cell, RefCell};
use std::ffi::OsStr;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicU32, Ordering};

// -----------------------------------------------------------------------
// Fixtures
// -----------------------------------------------------------------------

pub(crate) static SCRATCH: AtomicU32 = AtomicU32::new(0);

// -----------------------------------------------------------------------
// Observing the removal retry
// -----------------------------------------------------------------------

/// What to run after each of this thread's removal attempts.
///
/// A named alias because the raw shape trips `clippy::type_complexity`, and
/// because the two `thread_local!` slots below read better for having it.
type AttemptObserver = Box<dyn FnMut(u32)>;

// Attempts `remove_tree_once_handles_close` has made **on this thread**, and
// the observer to run after each.
//
// Thread-local rather than global, and that is the whole reason it is sound:
// the suite runs tests in parallel and several of them remove worktrees, so a
// process-wide counter would be another test's number as often as this one's.
// The primitive runs on the thread that called `remove_worktree`, so a
// thread-local counts exactly the removals the observing test drove.
//
// `PR5-R1-CFG-TEST-SHRINKS-THE-DOMAIN` is why the parent's half of this seam
// is declared at the bottom of `src/workspace_manager.rs` rather than beside
// the primitive: `effects::production_region` truncates a source at its first
// `#[cfg(test)]`, so a `#[cfg(test)]` item above the funnels would take every
// one of them out of the census that proves the Worktree group has them.
//
// `//` rather than `///`: rustdoc does not document a macro invocation, and
// `-D unused-doc-comments` says so.
thread_local! {
    static REMOVAL_ATTEMPTS: Cell<u32> = const { Cell::new(0) };
    static REMOVAL_ATTEMPT_OBSERVER: RefCell<Option<AttemptObserver>> =
        const { RefCell::new(None) };
}

/// A live observation of this thread's removal attempts, ended by dropping it.
///
/// Held by the observing test for exactly as long as the observation is wanted;
/// its `Drop` uninstalls the observer, so a test that unwinds cannot leave a
/// closure behind for whatever runs next on this thread.
pub(crate) struct AttemptObservation {
    /// Not `Send`: the counter and the observer are this thread's.
    _not_send: PhantomData<*const ()>,
}

impl AttemptObservation {
    /// Attempts made since the observation began.
    pub(crate) fn count(&self) -> u32 {
        REMOVAL_ATTEMPTS.with(Cell::get)
    }
}

impl Drop for AttemptObservation {
    fn drop(&mut self) {
        REMOVAL_ATTEMPT_OBSERVER.with(|slot| {
            if let Ok(mut slot) = slot.try_borrow_mut() {
                *slot = None;
            }
        });
    }
}

/// Start counting this thread's removal attempts, running `observer` after each.
///
/// The observer runs **on the removing thread, after the attempt has already
/// returned**, which is the property the closing-handle control is built on: a
/// test that releases a held handle from here knows the attempt it is releasing
/// against has completed, rather than hoping it has. Nothing outside the loop
/// can establish that, which is why this seam exists at all.
pub(crate) fn observe_removal_attempts(observer: AttemptObserver) -> AttemptObservation {
    REMOVAL_ATTEMPTS.with(|count| count.set(0));
    REMOVAL_ATTEMPT_OBSERVER.with(|slot| {
        if let Ok(mut slot) = slot.try_borrow_mut() {
            *slot = Some(observer);
        }
    });
    AttemptObservation {
        _not_send: PhantomData,
    }
}

/// Record that attempt `attempt` has completed, and run the observer.
///
/// Called from `super::note_removal_attempt`, the `#[cfg(test)]` half of the
/// primitive's seam. `try_borrow_mut` rather than `borrow_mut` so that an
/// observer which somehow removes a tree of its own is a no-op here instead of
/// a panic inside production code.
pub(crate) fn note_removal_attempt(attempt: u32) {
    REMOVAL_ATTEMPTS.with(|count| count.set(attempt));
    REMOVAL_ATTEMPT_OBSERVER.with(|slot| {
        if let Ok(mut slot) = slot.try_borrow_mut() {
            if let Some(observer) = slot.as_mut() {
                observer(attempt);
            }
        }
    });
}

/// A scratch directory unique to this process *and* to this call, because
/// the suite runs tests in parallel and two fixtures sharing a directory
/// would each measure the other's Git repository.
pub(crate) fn scratch(tag: &str) -> PathBuf {
    let ordinal = SCRATCH.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "upstroke-wm-{tag}-{}-{ordinal}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create the scratch directory");
    dir
}

pub(crate) fn git_out(dir: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("run git")
}

pub(crate) fn git(dir: &Path, args: &[&str]) -> String {
    let output = git_out(dir, args);
    assert!(
        output.status.success(),
        "git {args:?} in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// A real repository, a real private root, and a manager over both.
/// The fixture's run id: a canonical ULID, as `derive` requires
/// (`DESIGN.md` §15, "run-id = ULID"), spelt to be recognisable in a path.
pub(crate) const RUN_ID: &str = "01KZSWEEP00000000000000001";

pub(crate) struct Fixture {
    pub(crate) root: PathBuf,
    pub(crate) base: PathBuf,
    pub(crate) private: PathBuf,
    pub(crate) manager: WorkspaceManager,
    /// The first commit.
    pub(crate) seed: String,
    /// The tip of `main`.
    pub(crate) head: String,
    /// A commit on a side branch, based on `seed`, for the cherry-picks.
    pub(crate) side: String,
}

impl Fixture {
    /// A SHA-1 repository, whatever `GIT_DEFAULT_HASH` says in the
    /// environment: the object format is part of what a test asserts about
    /// (an object id's length, the null id's spelling), so the fixture pins
    /// it rather than inheriting it (§12). [`Self::with_object_format`] is
    /// the other format.
    pub(crate) fn new(tag: &str) -> Self {
        Self::with_object_format(tag, "sha1")
    }

    /// A repository of the given object format, `sha1` or `sha256`.
    pub(crate) fn with_object_format(tag: &str, object_format: &str) -> Self {
        let root = scratch(tag);
        let base = root.join("repo");
        let private = root.join("private");
        fs::create_dir_all(&base).expect("repo directory");
        fs::create_dir_all(&private).expect("private root");

        let object_format = format!("--object-format={object_format}");
        git(&base, &["init", "-q", "-b", "main", &object_format]);
        git(&base, &["config", "user.email", "tests@upstroke.local"]);
        git(&base, &["config", "user.name", "upstroke tests"]);
        // Line endings are pinned for the same reason the object format is
        // (§12, and `PR126-REVIEW2-NULL-TESTS-INHERIT-THE-HASH-FORMAT`): an
        // ambient Git setting that silently changes what a test observes.
        // With `core.autocrlf` on, as it is on the Windows guest, a blob
        // written as `A\n` is checked out as `A\r\n`, so a test comparing
        // checked-out content against what it wrote fails on that platform
        // alone while the blob is the one it asked for.
        git(&base, &["config", "core.autocrlf", "false"]);
        git(&base, &["config", "core.eol", "lf"]);
        // `git worktree add` writes a reflog entry; keep the repository
        // self-contained so nothing depends on a global config.
        git(&base, &["config", "core.logAllRefUpdates", "true"]);
        fs::write(base.join("a.txt"), "one\n").expect("seed file");
        git(&base, &["add", "-A"]);
        git(&base, &["commit", "-q", "-m", "seed"]);
        let seed = git(&base, &["rev-parse", "HEAD"]);

        fs::write(base.join("b.txt"), "two\n").expect("second file");
        git(&base, &["add", "-A"]);
        git(&base, &["commit", "-q", "-m", "second"]);
        let head = git(&base, &["rev-parse", "HEAD"]);

        git(&base, &["checkout", "-q", "-b", "side", &seed]);
        fs::write(base.join("c.txt"), "side\n").expect("side file");
        git(&base, &["add", "-A"]);
        git(&base, &["commit", "-q", "-m", "side"]);
        let side = git(&base, &["rev-parse", "HEAD"]);
        git(&base, &["checkout", "-q", "main"]);

        let manager =
            WorkspaceManager::derive(&base, &private, RUN_ID, "inc-1").expect("derive the manager");
        Self {
            root,
            base,
            private,
            manager,
            seed,
            head,
            side,
        }
    }

    /// Re-open a fixture a **previous process** built.
    ///
    /// A kill child dies by `std::process::abort()`, so its `Drop` never
    /// runs and its scratch tree survives it. The parent then has to speak
    /// about that tree — which repository, which private root, which
    /// commits — and re-deriving it is the only honest way: a value passed
    /// through an environment variable would be the child's belief about
    /// its own state, and the whole point of a kill test is that the
    /// child's beliefs did not survive.
    ///
    /// The manager is derived with the **same** run id and incarnation as
    /// [`Self::new`], because an intent records both and a reclaim that
    /// derived a different pair would be reclaiming another run's residue.
    pub(crate) fn adopt(root: PathBuf) -> Self {
        let base = root.join("repo");
        let private = root.join("private");
        let head = git(&base, &["rev-parse", "main"]);
        let seed = git(&base, &["rev-parse", "main~1"]);
        let side = git(&base, &["rev-parse", "side"]);
        let manager = WorkspaceManager::derive(&base, &private, RUN_ID, "inc-1")
            .expect("derive the manager over an adopted fixture");
        Self {
            root,
            base,
            private,
            manager,
            seed,
            head,
            side,
        }
    }

    pub(crate) fn created(tag: &str) -> Self {
        let fixture = Self::new(tag);
        fixture
            .manager
            .create_execution_root(&mut NoHooks)
            .expect("create the execution root");
        fixture
    }

    /// [`Self::created`] over a SHA-256 repository, for the tests that assert
    /// something about both object formats.
    pub(crate) fn created_sha256(tag: &str) -> Self {
        let fixture = Self::with_object_format(tag, "sha256");
        fixture
            .manager
            .create_execution_root(&mut NoHooks)
            .expect("create the execution root");
        fixture
    }

    pub(crate) fn task(&self, key: &str, generation: u32) -> Slot {
        Slot::Task {
            key: key.to_owned(),
            generation,
        }
    }

    /// A task worktree at `head`, intent first.
    pub(crate) fn add_task(&self, hooks: &mut dyn EffectHooks, key: &str, generation: u32) -> Slot {
        let slot = self.task(key, generation);
        self.manager
            .write_intent(hooks, &slot)
            .expect("write the intent");
        self.manager
            .add_worktree(hooks, &slot, &self.head)
            .expect("add the worktree");
        slot
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

// -----------------------------------------------------------------------
// The primitives a topology module's test cannot reach for itself
// -----------------------------------------------------------------------

/// Write `bytes` at `path`, creating the parent directories.
///
/// This is a test's *worker*: in production an agent subprocess edits files
/// and the engine never does (DESIGN.md §4). A test has no agent, so it
/// writes what the agent would have written — and it does it here, where
/// the write is inside the reviewed funnel module, rather than in the
/// topology module whose whole point is that it cannot.
pub(crate) fn write_file(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("the parent directory of a fixture file");
    }
    fs::write(path, bytes).expect("write a fixture file");
}

/// Create `path` and every missing parent.
pub(crate) fn create_dir(path: &Path) {
    fs::create_dir_all(path).expect("create a fixture directory");
}

/// Remove `path` if it is there. Idempotent, like every reclaim.
pub(crate) fn remove_file(path: &Path) {
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => panic!("removing {}: {error}", path.display()),
    }
}

/// Remove the empty directory at `path`: a worker's file operation, standing
/// for a tool that removes a directory it has emptied.
pub(crate) fn remove_dir(path: &Path) {
    fs::remove_dir(path)
        .unwrap_or_else(|error| panic!("removing the directory {}: {error}", path.display()));
}

/// Run this test binary again, `--exact --ignored`, with `env` set, and
/// return its exit status.
///
/// The kill-test shape `src/rundir.rs` established: `Injection::Kill` is
/// `std::process::abort()`, a real process death, so the child has to be a
/// real process and the claim is what it left on disk. `env` is a list
/// rather than a map so a caller can pass the same key twice and see the
/// last win, exactly as `Command` does.
pub(crate) fn run_kill_child(test: &str, env: &[(&str, &OsStr)]) -> std::process::ExitStatus {
    let mut command = Command::new(std::env::current_exe().expect("this test binary"));
    command
        .args(["--exact", test, "--ignored", "--nocapture"])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (key, value) in env {
        command.env(key, value);
    }
    command.status().expect("spawn the kill child")
}

/// A `git` child a test can kill at a chosen moment.
///
/// The residue sampler's child, and deliberately **blind to what it is
/// running**: no argv reaches [`Self::kill`], so a per-command count taken
/// over these cannot be defeated inside this type. It is the same shape
/// `mod tests`'s `SampledChild` uses, which stays there because the
/// four-command sampler stays there; this one exists because the
/// two-command sampler of `T-ATTEMPT` lives in a module that cannot name
/// [`Command`].
pub(crate) struct KillableGitChild {
    child: std::process::Child,
    /// Started once the spawn has returned, so what [`Self::kill`] reads
    /// off it is time the child was left *running*.
    spawned: std::time::Instant,
    /// What the clock said when a kill fired at this child, or `None` if
    /// none ever did. Written only by [`Self::kill`].
    fired: Option<std::time::Duration>,
}

/// The `git` a sampled child runs, so that a kill of the child is a kill of
/// git.
///
/// On Unix that is `git` on the `PATH`. On Windows `git` on the `PATH` is Git
/// for Windows' `cmd\git.exe`, a 46 KB launcher that starts the real
/// `mingw64\bin\git.exe` as its own child and waits for it, so a
/// `Child::kill` there ends the launcher and the pick runs on to completion:
/// the winguest lane at `56ea88c9` recorded 30 kills, ten of which found no
/// write and twenty a finished pick, none an interruption
/// (`PR249-KILL-SAMPLER-WINDOWS-WRAPPER`). So the sampled child is the real
/// binary, found through `git --exec-path` (`<prefix>/mingw64/libexec/git-core`)
/// as `<prefix>/mingw64/bin/git.exe`, whose DLLs sit beside it. Resolved
/// once per process; `git` when that layout is absent.
pub(crate) fn sampled_git() -> &'static Path {
    static RESOLVED: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    RESOLVED.get_or_init(|| {
        let fallback = PathBuf::from("git");
        if !cfg!(windows) {
            return fallback;
        }
        let Ok(output) = Command::new("git")
            .arg("--exec-path")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
        else {
            return fallback;
        };
        if !output.status.success() {
            return fallback;
        }
        let exec_path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
        match exec_path
            .parent()
            .and_then(Path::parent)
            .map(|prefix| prefix.join("bin").join("git.exe"))
        {
            Some(real) if real.is_file() => real,
            _ => fallback,
        }
    })
}

impl KillableGitChild {
    /// Spawn `git -C cwd <args>` with its streams discarded; [`sampled_git`]
    /// says which `git`.
    pub(crate) fn spawn(cwd: &Path, args: &[String]) -> Self {
        let child = Command::new(sampled_git())
            .arg("-C")
            .arg(cwd)
            .args(["-c", "core.fsmonitor=false"])
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn the sampled git child");
        Self {
            child,
            spawned: std::time::Instant::now(),
            fired: None,
        }
    }

    /// Kill the child, recording when the kill fired.
    ///
    /// The clock is read at the instant of the kill and stored *after* the
    /// kill returns, so deleting `self.child.kill()` leaves `outcome`
    /// unbound and the module stops compiling.
    pub(crate) fn kill(&mut self) {
        let fired = self.spawned.elapsed();
        let outcome = self.child.kill();
        self.fired = Some(fired);
        let _ = outcome;
    }

    /// Whether the child has exited on its own, and when the parent saw it.
    ///
    /// The duration is the clock at the poll that found the child gone: the
    /// parent's observation of the exit, not the child's own time, which no
    /// platform hands a parent — `wait4` carries a child's CPU times and no
    /// wall-clock exit, and Windows' `GetProcessTimes`, which does, is not
    /// bound here. So the number is an upper bound on the child's run,
    /// tight by one poll while the parent holds a core and late by the
    /// scheduler's wake-up when it does not, and [`KillBudget`] follows it
    /// as the bound it is.
    ///
    /// `None` while it is still running.
    pub(crate) fn exited(&mut self) -> Option<std::time::Duration> {
        match self.child.try_wait() {
            Ok(Some(_)) => Some(self.spawned.elapsed()),
            _ => None,
        }
    }

    /// Reap it. The wait status is the only thing a kill changes.
    pub(crate) fn wait(&mut self) -> std::process::ExitStatus {
        self.child.wait().expect("reap the sampled git child")
    }

    /// Leave the child running until `aim` after its spawn or until it
    /// exits on its own, and say which: `Some` with how long it ran, as the
    /// parent saw it, if it exited first; `None` if it was still running at
    /// the aim — and then **the kill has fired**, sent by the poll that found
    /// it running there, and [`Self::fired`] says when.
    ///
    /// Polled, never slept through. `sleep(aim)` then `kill` reports nothing
    /// about the child and wakes when the scheduler pleases, so on a loaded
    /// host the kill is late by the wake-up and the sampler cannot tell a
    /// child it missed from one it never aimed inside. Here [`Self::exited`]
    /// is asked once a millisecond while the aim is far and continuously
    /// once it is within [`Self::SPIN_WITHIN`], and the poll that reaches
    /// the aim with the child still running sends the kill itself, with no
    /// return to the caller between the two. What remains is the
    /// platform's: between that poll's `try_wait` and the kill's own system
    /// call the parent can be descheduled, and a child that exits in that
    /// window is missed by the kill and ends on its own terms — its status
    /// is a completion, not the kill's signature, and [`Self::fired`] bounds
    /// how long it ran, which the samplers feed back like any completion.
    pub(crate) fn run_until(&mut self, aim: std::time::Duration) -> Option<std::time::Duration> {
        let deadline = self.spawned + aim;
        loop {
            if let Some(ran) = self.exited() {
                return Some(ran);
            }
            let now = std::time::Instant::now();
            if now >= deadline {
                self.kill();
                return None;
            }
            if deadline - now > Self::SPIN_WITHIN {
                std::thread::sleep(std::time::Duration::from_millis(1));
            } else {
                std::thread::yield_now();
            }
        }
    }

    /// How close to its aim [`Self::run_until`] stops sleeping and polls
    /// continuously: wider than a loaded host's wake-up from a
    /// one-millisecond sleep.
    const SPIN_WITHIN: std::time::Duration = std::time::Duration::from_millis(4);

    /// When a kill fired at this child, if one ever did.
    pub(crate) fn fired(&self) -> Option<std::time::Duration> {
        self.fired
    }
}

/// Whether `status` is the death `std::process::abort()` produces.
///
/// **Not `!status.success()`.** A kill child that reaches its own
/// `unreachable!` panics, and a panic also fails to succeed — so a parent
/// that accepted any unsuccessful exit would read "the injection stopped
/// killing" as "the injection killed", and would then go on to inspect a
/// directory the panicking child's `Drop` had already deleted. Measured:
/// exactly that, on a kill armed at a site the child never reached.
///
/// Unix has a value for it — `SIGABRT`, which no Rust panic raises. Windows
/// does not expose one portably (`abort()` reaches `__fastfail`, whose code
/// has moved between CRT versions), so there the oracle is the *negation*
/// of the panic's own exit code, which `std::process::abort` cannot produce
/// and `panic!` always does.
pub(crate) fn died_by_abort(status: &std::process::ExitStatus) -> bool {
    #[cfg(unix)]
    {
        std::os::unix::process::ExitStatusExt::signal(status) == Some(libc::SIGABRT)
    }
    #[cfg(windows)]
    {
        /// What a Rust process exits with when a panic unwinds out of main.
        const PANIC: i32 = 101;
        !status.success() && status.code() != Some(PANIC)
    }
}

/// Whether `status` carries this platform's signature of a
/// [`std::process::Child::kill`].
///
/// A **value** per platform, not `!status.success()`: a command that merely
/// failed also fails to succeed, and reading that as a kill is how a
/// kill-count keeps counting after the kill is gone.
pub(crate) fn died_by_kill(status: &std::process::ExitStatus) -> bool {
    // `Child::kill` sends `SIGKILL`, and no exit a child reaches on its own
    // carries a signal at all.
    #[cfg(unix)]
    {
        std::os::unix::process::ExitStatusExt::signal(status) == Some(libc::SIGKILL)
    }
    // `Child::kill` is `TerminateProcess(handle, 1)`; the sampler's probe
    // asserts the same command exits 0 when nothing kills it, so 1 is not
    // an end these commands reach by themselves.
    #[cfg(windows)]
    {
        status.code() == Some(1)
    }
}

/// Time one uninterrupted run of `git -C cwd <args>`.
///
/// The kill ladder is fractions of this duration, which is the only
/// variance a replay can pin — see `mod tests`'s `measure_budget` for the
/// argument, and for why the measurement runs in a **probe slot of its
/// own** rather than in the worktree the samples will kill in.
pub(crate) fn time_git(cwd: &Path, args: &[String]) -> std::time::Duration {
    let start = std::time::Instant::now();
    let output = git_out(cwd, &args.iter().map(String::as_str).collect::<Vec<_>>());
    let elapsed = start.elapsed();
    assert!(
        output.status.success(),
        "the probe must really run: git {args:?} in {}: {}",
        cwd.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    elapsed
}

/// A kill sampler's budget: how long an uninterrupted run of the sampled
/// command takes on this machine, now.
///
/// A sampler aims its kills at fixed fractions of one duration, and every
/// sampler in this tree that took that duration from a single measured run
/// has been red on a hosted macOS runner with every kill landing after its
/// child had already finished (`PR7-SAMPLER-SCHEDULES-FROM-A-COLD-PROBE`,
/// `PR80-MACOS-WORKSPACE-SAMPLER-COLD-PROBE-RECURRENCE`,
/// `RECOVER-CHERRY-PICK-SAMPLER-COLD-PROBE`,
/// `G4B-O10-REPAIR-MATERIALIZE-SAMPLER-MACOS-KILL-FLOOR`). So the budget is
/// never one number. It starts as the median of a probe's runs after a
/// discarded warm-up, and it follows the sampled children themselves: a
/// child that finished before its kill has measured the command under the
/// sampler's own conditions, at that moment, and the next kill is aimed
/// inside what it took. An inflated probe is corrected by the first child
/// that outruns it; a host that drifts is tracked rung by rung.
///
/// It follows in both directions and is not capped at the probe: a host
/// that slows after the probe needs rungs past it to reach the pick's
/// writes, and a completion the parent saw late — woken after the exit, or
/// a kill that missed — is an upper bound on the pick that the median of
/// the last [`KillBudget::RECENT`] damps, never a ceiling.
pub(crate) struct KillBudget {
    probe: std::time::Duration,
    /// Within how long of their spawn the children that completed before
    /// their kill had exited, as the parent saw it, oldest first.
    completed: Vec<std::time::Duration>,
}

impl KillBudget {
    /// No budget below this: a measurement that small is the clock's, not
    /// the command's.
    pub(crate) const FLOOR: std::time::Duration = std::time::Duration::from_micros(200);

    /// How many of the most recent completions the budget follows: enough
    /// that one descheduled parent does not set the ladder alone, few
    /// enough that the first completion moves it at once.
    pub(crate) const RECENT: usize = 3;

    /// From a probe's uninterrupted runs, in order. The first is the
    /// warm-up and is discarded — the first run in a fresh worktree pays
    /// for cold caches and, on Windows, an antivirus pass — and the budget
    /// is the median of the rest, because the failure mode of one
    /// measurement is one outlier, which a median discards and a mean
    /// keeps.
    pub(crate) fn probed(runs: &[std::time::Duration]) -> Self {
        assert!(
            runs.len() > 1,
            "a budget needs a warm-up run and at least one after it: {runs:?}"
        );
        let after_warm_up = runs.get(1..).unwrap_or_default();
        Self {
            probe: median(after_warm_up)
                .unwrap_or(Self::FLOOR)
                .max(Self::FLOOR),
            completed: Vec::new(),
        }
    }

    /// What the probe measured, for the record a red run carries.
    pub(crate) fn probe(&self) -> std::time::Duration {
        self.probe
    }

    /// The budget now: the median of the last [`Self::RECENT`] children
    /// that finished before their kill, or the probe's while none has.
    pub(crate) fn current(&self) -> std::time::Duration {
        let recent: Vec<std::time::Duration> = self
            .completed
            .iter()
            .rev()
            .take(Self::RECENT)
            .copied()
            .collect();
        median(&recent).unwrap_or(self.probe).max(Self::FLOOR)
    }

    /// Where the kill of rung `rung` (from zero) of `rungs` is aimed: the
    /// ladder's fraction `(rung + 1) / (rungs + 1)` of the current budget,
    /// so the rungs stay spread through the command and never reach its
    /// measured end.
    pub(crate) fn aim(&self, rung: u32, rungs: u32) -> std::time::Duration {
        self.current()
            .mul_f64(f64::from(rung + 1) / f64::from(rungs + 1))
    }

    /// A child completed before its kill, within `within` of its spawn: how
    /// long it ran as the parent saw it exit, or, when it exited between the
    /// poll that found it running and the kill, when the kill fired. Either
    /// bounds the pick from above, the ladder was aimed past it, and the
    /// budget follows what it measured.
    pub(crate) fn completed(&mut self, within: std::time::Duration) {
        self.completed.push(within);
    }

    /// How many children have moved the budget.
    pub(crate) fn completions(&self) -> usize {
        self.completed.len()
    }
}

/// The median of `durations`; `None` of none.
pub(crate) fn median(durations: &[std::time::Duration]) -> Option<std::time::Duration> {
    let mut sorted = durations.to_vec();
    sorted.sort_unstable();
    sorted.get(sorted.len() / 2).copied()
}
