//! Extended notes: `docs/internals/runner/container.md`

// Allowlist placement: the funnel section of `effects/allowlist.toml`, which
// carries this module's review clause. `effect_site_inventory.mechanism` (2).
#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::disallowed_macros
)]

pub mod census;
pub mod env;
pub mod exec;
pub mod intent;
pub mod resolve;
pub mod runtime;
pub mod view;

use std::collections::BTreeMap;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::error::UpstrokeError;
use crate::topology::effects::{ContainerSite, EffectSiteId, HookHarness, HookPhase, Injection};
use crate::util;

use crate::runner::InvocationId;
use intent::{ContainerIntent, ContainerName, INTENT_STAGED_SUFFIX, IntentWritten, containers_dir};
use runtime::{
    ContainerExecution, ContainerRuntime, ContainerTrace, CreateSpec, CreatedContainer,
    DiscoveredContainer, DurableStep, Liveness, RuntimeError, RuntimeOp, Settled, StopMode,
    TracePhase, ViewAction,
};

pub trait ContainerHooks {
    fn phase(&mut self, site: EffectSiteId, phase: HookPhase) -> Injection;

    fn trace(&self) -> ContainerTrace {
        ContainerTrace::off()
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoHooks;

impl ContainerHooks for NoHooks {
    fn phase(&mut self, _site: EffectSiteId, _phase: HookPhase) -> Injection {
        Injection::Proceed
    }
}

#[derive(Debug, Clone, Default)]
pub struct HarnessHooks {
    harness: crate::observations::Exported,
    trace: ContainerTrace,
}

impl HarnessHooks {
    #[must_use]
    pub fn new(harness: Arc<Mutex<HookHarness>>) -> Self {
        Self {
            harness: crate::observations::Exported::new(harness),
            trace: ContainerTrace::off(),
        }
    }

    #[must_use]
    pub fn harness(&self) -> &Arc<Mutex<HookHarness>> {
        self.harness.harness()
    }

    #[must_use]
    pub fn recording_trace(mut self, trace: ContainerTrace) -> Self {
        self.trace = trace;
        self
    }
}

impl ContainerHooks for HarnessHooks {
    fn phase(&mut self, site: EffectSiteId, phase: HookPhase) -> Injection {
        self.harness.hook(site, phase)
    }

    fn trace(&self) -> ContainerTrace {
        self.trace.clone()
    }
}

fn apply(injection: Injection, site: EffectSiteId, phase: HookPhase) -> Result<(), UpstrokeError> {
    match injection {
        Injection::Proceed => Ok(()),
        Injection::Kill => std::process::abort(),
        Injection::Error => Err(UpstrokeError::Refused {
            message: format!("the container funnel was made to fail at `{site}` ({phase})"),
        }),
    }
}

fn funnel<T>(
    hooks: &mut dyn ContainerHooks,
    site: ContainerSite,
    primitive: impl FnOnce() -> Result<T, UpstrokeError>,
) -> Result<T, UpstrokeError> {
    funnel_reporting_attempt(hooks, site, primitive).map_err(|(_, error)| error)
}

fn funnel_reporting_attempt<T>(
    hooks: &mut dyn ContainerHooks,
    site: ContainerSite,
    primitive: impl FnOnce() -> Result<T, UpstrokeError>,
) -> Result<T, (bool, UpstrokeError)> {
    let id = EffectSiteId::Container(site);
    let trace = hooks.trace();
    trace.site(site, TracePhase::Before);
    if let Err(error) = apply(hooks.phase(id, HookPhase::Before), id, HookPhase::Before) {
        return Err((false, error));
    }
    let produced = primitive().map_err(|error| (true, error))?;
    if let Err(error) = apply(hooks.phase(id, HookPhase::After), id, HookPhase::After) {
        return Err((true, error));
    }
    trace.site(site, TracePhase::After);
    Ok(produced)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operation {
    WriteIntent,
    Create,
    Start,
    MountGitView,
    Stop,
    Remove,
    UnmountGitView,
    RemoveIntent,
}

const fn operation_of(site: ContainerSite) -> Operation {
    match site {
        ContainerSite::WriteIntent => Operation::WriteIntent,
        ContainerSite::Create => Operation::Create,
        ContainerSite::Start => Operation::Start,
        ContainerSite::MountGitView => Operation::MountGitView,
        ContainerSite::Stop => Operation::Stop,
        ContainerSite::Remove => Operation::Remove,
        ContainerSite::UnmountGitView => Operation::UnmountGitView,
        ContainerSite::RemoveIntent => Operation::RemoveIntent,
    }
}

fn expect_site(site: ContainerSite, wanted: Operation) -> Result<(), UpstrokeError> {
    if operation_of(site) == wanted {
        return Ok(());
    }
    Err(UpstrokeError::Refused {
        message: format!(
            "the container funnel was asked to perform {wanted:?} under site \
             `Container.{}`; every effectful funnel API takes its group's site by value \
             (decisions.effect_site_inventory.identity) and the site must name the \
             operation it accounts for",
            site.name()
        ),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitViewRequest {
    pub path: PathBuf,
    pub workspace: PathBuf,
    pub head: Option<String>,
}

pub trait GitView: Send + Sync {
    fn materialize(&self, request: &GitViewRequest) -> Result<PathBuf, UpstrokeError>;

    fn discard(&self, path: &Path) -> Result<(), UpstrokeError>;
}

#[derive(Debug, Clone, Default)]
pub struct DisposableDirView {
    trace: ContainerTrace,
}

impl DisposableDirView {
    #[must_use]
    pub fn new(trace: ContainerTrace) -> Self {
        Self { trace }
    }
}

impl GitView for DisposableDirView {
    fn materialize(&self, request: &GitViewRequest) -> Result<PathBuf, UpstrokeError> {
        fs::create_dir_all(&request.path).map_err(|source| UpstrokeError::Io {
            path: request.path.clone(),
            source,
        })?;
        self.trace.view(ViewAction::Materialized, &request.path);
        Ok(request.path.clone())
    }

    fn discard(&self, path: &Path) -> Result<(), UpstrokeError> {
        racing_removal(path, || fs::remove_dir_all(path))?;
        self.trace.view(ViewAction::Discarded, path);
        Ok(())
    }
}

pub const RACING_ACCESS_ATTEMPTS: usize = 64;

pub const RACING_YIELD_ATTEMPTS: usize = 16;

pub const RACING_SLEEP: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RacingPause {
    Yield,
    Sleep,
    Done,
}

fn racing_pause_after(failed: usize) -> RacingPause {
    if failed >= RACING_ACCESS_ATTEMPTS {
        RacingPause::Done
    } else if failed <= RACING_YIELD_ATTEMPTS {
        RacingPause::Yield
    } else {
        RacingPause::Sleep
    }
}

fn racing_pause(failed: usize) {
    let pause = racing_pause_after(failed);
    note_racing_attempt(failed, pause);
    match pause {
        RacingPause::Yield => racing_yield(),
        RacingPause::Sleep => racing_sleep(RACING_SLEEP),
        RacingPause::Done => {}
    }
}

#[cfg(not(test))]
#[inline]
fn note_racing_attempt(_failed: usize, _pause: RacingPause) {}

#[cfg(not(test))]
#[inline]
fn racing_yield() {
    std::thread::yield_now();
}

#[cfg(not(test))]
#[inline]
fn racing_sleep(duration: Duration) {
    std::thread::sleep(duration);
}

pub fn write_intent(
    hooks: &mut dyn ContainerHooks,
    site: ContainerSite,
    private_root: &Path,
    name: &ContainerName,
    record: &ContainerIntent,
) -> Result<IntentWritten, UpstrokeError> {
    expect_site(site, Operation::WriteIntent)?;
    let path = name.intent_path(private_root);
    let trace = hooks.trace();
    let root = private_root.to_path_buf();
    funnel(hooks, site, || {
        let bytes = serde_json::to_vec(record).map_err(|error| UpstrokeError::Git {
            message: format!("serializing the container intent for `{name}`: {error}"),
        })?;
        write_synced(&path, &bytes, &trace)?;
        IntentWritten::certify(&root, name)
    })
}

pub fn create_container(
    hooks: &mut dyn ContainerHooks,
    site: ContainerSite,
    runtime: &dyn ContainerRuntime,
    intent: &IntentWritten,
    spec: &CreateSpec,
) -> Result<CreatedContainer, UpstrokeError> {
    expect_site(site, Operation::Create)?;
    expect_intent_for(intent, &spec.name, "created")?;
    expect_mounted_volumes_present(runtime, spec)?;
    funnel(hooks, site, || runtime.create(spec).map_err(refused))
}

fn expect_mounted_volumes_present(
    runtime: &dyn ContainerRuntime,
    spec: &CreateSpec,
) -> Result<(), UpstrokeError> {
    for mount in &spec.mounts {
        let runtime::Mount::Volume { name, target, .. } = mount else {
            continue;
        };
        let present = runtime
            .volume_present(name)
            .map_err(|error| UpstrokeError::Refused {
                message: format!(
                    "the container runtime could not be asked whether the credential volume \
                     `{name}` exists before creating `{}`: {error}. A named volume that \
                     `docker create` does not find is created empty by the runtime, and R20 is \
                     `operator_owned` — \"never created or pruned by a run\" \
                     (decisions.resource_accounting.rows[R20]) — so a runtime that will not \
                     answer refuses rather than risking it",
                    spec.name
                ),
            })?;
        if !present {
            return Err(UpstrokeError::Refused {
                message: format!(
                    "the credential volume `{name}`, which `{}` mounts at `{target}`, is not \
                     present in the container runtime. R20 credential volumes are \
                     `operator_owned` and `persistent_output` — \"never created or pruned by a \
                     run\" (decisions.resource_accounting.rows[R20]) — and `docker create` \
                     creates an absent named volume rather than refusing, so this invocation is \
                     refused before any container exists",
                    spec.name
                ),
            });
        }
    }
    Ok(())
}

#[derive(Debug)]
pub struct StartFailure {
    pub attempted: bool,
    pub error: UpstrokeError,
}

pub fn start_container(
    hooks: &mut dyn ContainerHooks,
    site: ContainerSite,
    runtime: &dyn ContainerRuntime,
    intent: &IntentWritten,
) -> Result<(), StartFailure> {
    expect_site(site, Operation::Start).map_err(|error| StartFailure {
        attempted: false,
        error,
    })?;
    let name = intent.name().as_str().to_owned();
    funnel_reporting_attempt(hooks, site, || runtime.start(&name).map_err(refused))
        .map_err(|(attempted, error)| StartFailure { attempted, error })
}

fn expect_intent_for(intent: &IntentWritten, name: &str, verb: &str) -> Result<(), UpstrokeError> {
    if intent.name().as_str() == name {
        return Ok(());
    }
    Err(UpstrokeError::Refused {
        message: format!(
            "`{name}` cannot be {verb} under the intent record of `{}`; every container \
             invocation writes its own synced intent in `<R>/containers` \
             (decisions.admission_and_leases.permits.crash_reconstruction) and \
             `container start without an intent is impossible by construction` \
             (expected_failures_refusals[6])",
            intent.name()
        ),
    })
}

pub fn mount_git_view(
    hooks: &mut dyn ContainerHooks,
    site: ContainerSite,
    view: &dyn GitView,
    request: &GitViewRequest,
) -> Result<PathBuf, UpstrokeError> {
    expect_site(site, Operation::MountGitView)?;
    funnel(hooks, site, || view.materialize(request))
}

pub fn stop_container(
    hooks: &mut dyn ContainerHooks,
    site: ContainerSite,
    runtime: &dyn ContainerRuntime,
    name: &ContainerName,
    mode: StopMode,
) -> Result<Settled, UpstrokeError> {
    expect_site(site, Operation::Stop)?;
    funnel(hooks, site, || {
        runtime.stop(name.as_str(), mode).map_err(refused)
    })
}

pub fn remove_container(
    hooks: &mut dyn ContainerHooks,
    site: ContainerSite,
    runtime: &dyn ContainerRuntime,
    name: &ContainerName,
) -> Result<Settled, UpstrokeError> {
    expect_site(site, Operation::Remove)?;
    funnel(hooks, site, || {
        runtime.remove(name.as_str()).map_err(refused)
    })
}

pub fn unmount_git_view(
    hooks: &mut dyn ContainerHooks,
    site: ContainerSite,
    view: &dyn GitView,
    path: &Path,
) -> Result<(), UpstrokeError> {
    expect_site(site, Operation::UnmountGitView)?;
    funnel(hooks, site, || view.discard(path))
}

pub fn remove_intent(
    hooks: &mut dyn ContainerHooks,
    site: ContainerSite,
    private_root: &Path,
    name: &ContainerName,
) -> Result<(), UpstrokeError> {
    expect_site(site, Operation::RemoveIntent)?;
    let path = name.intent_path(private_root);
    let trace = hooks.trace();
    funnel(hooks, site, || {
        let staged = staged_path(&path);
        remove_if_present(&staged, &trace)?;
        remove_if_present(&path, &trace)
    })
}

#[derive(Debug, Clone)]
pub struct LaunchPlan {
    pub private_root: PathBuf,
    pub name: ContainerName,
    pub invocation: InvocationId,
    pub intent: ContainerIntent,
    pub spec: CreateSpec,
    pub view: GitViewRequest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launched {
    pub name: ContainerName,
    pub intent_path: PathBuf,
    pub view_path: PathBuf,
    pub reported_image_id: String,
}

pub fn launch(
    hooks: &mut dyn ContainerHooks,
    runtime: &dyn ContainerRuntime,
    view: &dyn GitView,
    plan: &LaunchPlan,
) -> Result<Launched, UpstrokeError> {
    let written = write_intent(
        hooks,
        ContainerSite::WriteIntent,
        &plan.private_root,
        &plan.name,
        &plan.intent,
    )?;
    let intent_path = written.path().to_path_buf();
    let view_path = mount_git_view(hooks, ContainerSite::MountGitView, view, &plan.view)?;
    let created = create_container(hooks, ContainerSite::Create, runtime, &written, &plan.spec)?;
    if created.reported_image_id != plan.spec.image_id {
        let residue = cancel_created(
            hooks,
            runtime,
            view,
            &plan.private_root,
            &plan.name,
            Some(&view_path),
        );
        return Err(UpstrokeError::Refused {
            message: format!(
                "the container runtime created `{}` and reports image id `{}`, and the run's \
                 recorded image id is `{}`; a created container whose reported image id \
                 differs from the record is refused before start (INV-23){}",
                plan.name,
                created.reported_image_id,
                plan.spec.image_id,
                render_residue(&residue)
            ),
        });
    }
    start_container(hooks, ContainerSite::Start, runtime, &written)
        .map_err(|failure| failure.error)?;
    Ok(Launched {
        name: plan.name.clone(),
        intent_path,
        view_path,
        reported_image_id: created.reported_image_id,
    })
}

fn cancel_created(
    hooks: &mut dyn ContainerHooks,
    runtime: &dyn ContainerRuntime,
    view: &dyn GitView,
    private_root: &Path,
    name: &ContainerName,
    view_path: Option<&Path>,
) -> Vec<String> {
    cancel_reached(
        hooks,
        runtime,
        view,
        private_root,
        name,
        ContainerToRelease::MayBeRunning,
        view_path,
    )
    .messages
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerToRelease {
    Absent,
    MayBeRunning,
    ObservedExited,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelResidue {
    pub messages: Vec<String>,
    pub container_gone: bool,
}

impl CancelResidue {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
}

pub fn cancel_reached(
    hooks: &mut dyn ContainerHooks,
    runtime: &dyn ContainerRuntime,
    view: &dyn GitView,
    private_root: &Path,
    name: &ContainerName,
    container: ContainerToRelease,
    view_path: Option<&Path>,
) -> CancelResidue {
    let mut residue = Vec::new();
    let mut container_gone = container != ContainerToRelease::MayBeRunning;
    if container != ContainerToRelease::Absent {
        match stop_container(
            hooks,
            ContainerSite::Stop,
            runtime,
            name,
            StopMode::Graceful,
        ) {
            Ok(Settled::ProcessGone) => container_gone = true,
            Ok(Settled::RemovalInProgress) => {
                residue.push(removal_in_progress_establishes_nothing("the stop", name))
            }
            Err(error) => residue.push(format!("the container could not be stopped: {error}")),
        }
        match remove_container(hooks, ContainerSite::Remove, runtime, name) {
            Ok(Settled::ProcessGone) => container_gone = true,
            Ok(Settled::RemovalInProgress) => {
                residue.push(removal_in_progress_establishes_nothing("the removal", name))
            }
            Err(error) => residue.push(format!("the container could not be removed: {error}")),
        }
    }
    if !container_gone {
        residue.push(format!(
            "the R19 Git view and the R26 intent record of `{name}` are deliberately retained: \
             the runtime confirmed neither the stop nor the removal, so the container may still \
             be running with the view mounted, and the intent is what the next census reclaims \
             both through (decisions.resource_accounting.rows[R19].at_run_end.NoRunFinished)"
        ));
        return CancelResidue {
            messages: residue,
            container_gone,
        };
    }
    let mut view_survives = false;
    if let Some(path) = view_path {
        if let Err(error) = unmount_git_view(hooks, ContainerSite::UnmountGitView, view, path) {
            residue.push(format!("the R19 Git view could not be pruned: {error}"));
            view_survives = true;
        }
    }
    if view_survives {
        residue.push(format!(
            "the R26 intent record of `{name}` is deliberately retained, because it is the only \
             thing a later census can discover that unpruned R19 view through \
             (decisions.resource_accounting.rows[R19].at_run_end.NoRunFinished)"
        ));
        return CancelResidue {
            messages: residue,
            container_gone,
        };
    }
    if let Err(error) = remove_intent(hooks, ContainerSite::RemoveIntent, private_root, name) {
        residue.push(format!(
            "the R26 intent record could not be removed: {error}"
        ));
    }
    CancelResidue {
        messages: residue,
        container_gone,
    }
}

fn removal_in_progress_establishes_nothing(step: &str, name: &ContainerName) -> String {
    format!(
        "{step} of `{name}` was answered with another reclaimer's removal already in progress; the \
         daemon sets that flag before it kills, so this establishes nothing about the process"
    )
}

fn render_residue(residue: &[String]) -> String {
    if residue.is_empty() {
        return String::new();
    }
    format!(
        ". The cancel could not release everything the refused launch created, \
         so this run's R19/R26 ledgers do not balance and a census will find the \
         residue: {}",
        residue.join("; ")
    )
}

pub fn release(
    hooks: &mut dyn ContainerHooks,
    runtime: &dyn ContainerRuntime,
    view: &dyn GitView,
    private_root: &Path,
    launched: &Launched,
) -> Result<(), UpstrokeError> {
    release_classified(
        hooks,
        runtime,
        view,
        private_root,
        launched,
        ContainerToRelease::MayBeRunning,
    )
    .map_err(|failure| failure.error)
}

#[derive(Debug)]
pub struct ReleaseFailure {
    pub container_gone: bool,
    pub error: UpstrokeError,
}

pub fn release_classified(
    hooks: &mut dyn ContainerHooks,
    runtime: &dyn ContainerRuntime,
    view: &dyn GitView,
    private_root: &Path,
    launched: &Launched,
    container: ContainerToRelease,
) -> Result<(), ReleaseFailure> {
    let residue = cancel_reached(
        hooks,
        runtime,
        view,
        private_root,
        &launched.name,
        container,
        Some(&launched.view_path),
    );
    if residue.is_empty() {
        return Ok(());
    }
    Err(ReleaseFailure {
        container_gone: residue.container_gone,
        error: UpstrokeError::Refused {
            message: format!(
                "the release of `{}` could not complete every step, so this run's R19/R26 \
                 ledgers do not balance and a census will find the residue: {}",
                launched.name,
                residue.messages.join("; ")
            ),
        },
    })
}

pub const TERMINATION_OBSERVATIONS: usize = 8;

pub fn reclaim(
    hooks: &mut dyn ContainerHooks,
    runtime: &dyn ContainerRuntime,
    view: &dyn GitView,
    private_root: &Path,
    name: &ContainerName,
    view_path: Option<&Path>,
) -> Result<(), UpstrokeError> {
    stop_container(hooks, ContainerSite::Stop, runtime, name, StopMode::Kill)?;
    observe_terminated(runtime, name)?;
    remove_container(hooks, ContainerSite::Remove, runtime, name)?;
    if let Some(path) = view_path {
        unmount_git_view(hooks, ContainerSite::UnmountGitView, view, path)?;
    }
    remove_intent(hooks, ContainerSite::RemoveIntent, private_root, name)
}

pub fn observe_terminated(
    runtime: &dyn ContainerRuntime,
    name: &ContainerName,
) -> Result<Liveness, UpstrokeError> {
    for _ in 0..TERMINATION_OBSERVATIONS {
        let state = runtime.observe(name.as_str()).map_err(refused)?;
        if state.is_terminated() {
            return Ok(state);
        }
    }
    Err(UpstrokeError::Refused {
        message: format!(
            "`{name}` is still running after {TERMINATION_OBSERVATIONS} observations and \
             cannot be observed terminated; a dead owner's or dead incarnation's labeled \
             container that cannot be observed terminated blocks admission \
             (transaction_fault_matrix[T-CONTAINER].refusal_condition)"
        ),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OrphanWindow {
    ClosedByTheUnixReaper,
    UntilNextWriteCommandStart,
}

impl OrphanWindow {
    pub const ALL: &'static [Self] = &[
        Self::ClosedByTheUnixReaper,
        Self::UntilNextWriteCommandStart,
    ];

    #[must_use]
    pub const fn closed_by_a_reaper(self) -> bool {
        match self {
            Self::ClosedByTheUnixReaper => true,
            Self::UntilNextWriteCommandStart => false,
        }
    }
}

#[must_use]
pub const fn orphan_window() -> OrphanWindow {
    #[cfg(unix)]
    {
        OrphanWindow::ClosedByTheUnixReaper
    }
    #[cfg(not(unix))]
    {
        OrphanWindow::UntilNextWriteCommandStart
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundIntent {
    pub name: ContainerName,
    pub path: PathBuf,
    pub record: ContainerIntent,
}

pub fn read_intent(path: &Path) -> Result<ContainerIntent, UpstrokeError> {
    let bytes = fs::read(path).map_err(|source| UpstrokeError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|error| UpstrokeError::Refused {
        message: format!("`{}` is not a container intent: {error}", path.display()),
    })
}

fn read_racing(path: &Path) -> Result<Option<ContainerIntent>, UpstrokeError> {
    let mut last = None;
    for attempt in 0..RACING_ACCESS_ATTEMPTS {
        match read_intent(path) {
            Ok(record) => return Ok(Some(record)),
            Err(UpstrokeError::Io { source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                return Ok(None);
            }
            Err(UpstrokeError::Io { source, .. }) => {
                last = Some(source);
                racing_pause(attempt + 1);
            }
            Err(other) => return Err(other),
        }
    }
    Err(UpstrokeError::Io {
        path: path.to_path_buf(),
        source: last.unwrap_or_else(|| {
            std::io::Error::other("the record could not be read and reported no reason")
        }),
    })
}

pub fn list_intents(private_root: &Path) -> Result<Vec<FoundIntent>, UpstrokeError> {
    let dir = containers_dir(private_root);
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(UpstrokeError::Io { path: dir, source }),
    };
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| UpstrokeError::Io {
            path: dir.clone(),
            source,
        })?;
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if file_name.ends_with(INTENT_STAGED_SUFFIX) {
            continue;
        }
        let Some(name) = ContainerName::from_intent_file_name(&file_name)? else {
            continue;
        };
        let path = entry.path();
        let Some(record) = read_racing(&path)? else {
            continue;
        };
        found.push(FoundIntent { name, path, record });
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedIntent {
    pub name: ContainerName,
    pub path: PathBuf,
    pub record: Option<ContainerIntent>,
}

pub fn list_staged_intents(private_root: &Path) -> Result<Vec<StagedIntent>, UpstrokeError> {
    let dir = containers_dir(private_root);
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(UpstrokeError::Io { path: dir, source }),
    };
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| UpstrokeError::Io {
            path: dir.clone(),
            source,
        })?;
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let Some(stem) = file_name.strip_suffix(INTENT_STAGED_SUFFIX) else {
            continue;
        };
        let name = ContainerName::rebuild(stem)?;
        if name.intent_path(private_root).exists() {
            continue;
        }
        let path = entry.path();
        let record = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<ContainerIntent>(&bytes).ok(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(source) => return Err(UpstrokeError::Io { path, source }),
        };
        found.push(StagedIntent { name, path, record });
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}

pub fn remove_staged_intent(
    hooks: &mut dyn ContainerHooks,
    site: ContainerSite,
    private_root: &Path,
    name: &ContainerName,
) -> Result<(), UpstrokeError> {
    remove_intent(hooks, site, private_root, name)
}

fn refused(error: RuntimeError) -> UpstrokeError {
    UpstrokeError::Refused {
        message: error.to_string(),
    }
}

fn staged_path(path: &Path) -> PathBuf {
    let mut staged = path.as_os_str().to_owned();
    staged.push(".tmp");
    PathBuf::from(staged)
}

fn write_synced(path: &Path, bytes: &[u8], trace: &ContainerTrace) -> Result<(), UpstrokeError> {
    let parent = path.parent().ok_or_else(|| UpstrokeError::Git {
        message: format!("{} has no parent directory", path.display()),
    })?;
    fs::create_dir_all(parent).map_err(|source| UpstrokeError::Io {
        path: parent.to_path_buf(),
        source,
    })?;
    let staged = staged_path(path);
    {
        let mut file = fs::File::create(&staged).map_err(|source| UpstrokeError::Io {
            path: staged.clone(),
            source,
        })?;
        file.write_all(bytes).map_err(|source| UpstrokeError::Io {
            path: staged.clone(),
            source,
        })?;
        util::fsync_file(&file).map_err(|source| UpstrokeError::Io {
            path: staged.clone(),
            source,
        })?;
    }
    trace.durable(DurableStep::Synced, &staged);
    fs::rename(&staged, path).map_err(|source| UpstrokeError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    trace.durable(DurableStep::Renamed, path);
    util::fsync_dir(parent).map_err(|source| UpstrokeError::Io {
        path: parent.to_path_buf(),
        source,
    })?;
    trace.durable(DurableStep::DirSynced, parent);
    Ok(())
}

fn remove_if_present(path: &Path, trace: &ContainerTrace) -> Result<(), UpstrokeError> {
    if racing_removal(path, || fs::remove_file(path))? {
        trace.durable(DurableStep::Removed, path);
    }
    Ok(())
}

fn racing_removal(
    path: &Path,
    mut remove: impl FnMut() -> Result<(), std::io::Error>,
) -> Result<bool, UpstrokeError> {
    let mut last = None;
    for attempt in 0..RACING_ACCESS_ATTEMPTS {
        match remove() {
            Ok(()) => return Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => {
                last = Some(error);
                racing_pause(attempt + 1);
            }
        }
    }
    Err(UpstrokeError::Io {
        path: path.to_path_buf(),
        source: last.unwrap_or_else(|| {
            std::io::Error::other("the path could not be removed and reported no reason")
        }),
    })
}

pub const DOCKER_PROGRAM: &str = "docker";

#[derive(Debug, Clone, Default)]
pub struct DockerCli {
    trace: ContainerTrace,
}

impl DockerCli {
    #[must_use]
    pub fn new(trace: ContainerTrace) -> Self {
        Self { trace }
    }

    #[must_use]
    pub fn available() -> bool {
        util::find_program(DOCKER_PROGRAM).is_some()
            && Self::default()
                .exec(
                    RuntimeOp::Probe,
                    "daemon",
                    &["version", "--format", "{{.Server.Version}}"],
                )
                .is_ok()
    }

    fn exec(&self, op: RuntimeOp, target: &str, args: &[&str]) -> Result<String, RuntimeError> {
        self.exec_streams(op, target, args)
            .map(|(stdout, _)| String::from_utf8_lossy(&stdout).into_owned())
    }

    fn exec_streams(
        &self,
        op: RuntimeOp,
        target: &str,
        args: &[&str],
    ) -> Result<(Vec<u8>, Vec<u8>), RuntimeError> {
        self.trace.runtime(op, target);
        let output = Command::new(DOCKER_PROGRAM)
            .args(args)
            .output()
            .map_err(|error| RuntimeError::Unreachable {
                operation: op,
                detail: format!("{DOCKER_PROGRAM} could not be started: {error}"),
            })?;
        if output.status.success() {
            return Ok((output.stdout, output.stderr));
        }
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(classify_docker_failure(op, detail))
    }

    fn inspect(
        &self,
        op: RuntimeOp,
        target: &str,
        args: &[&str],
    ) -> Result<Option<String>, RuntimeError> {
        match self.exec(op, target, args) {
            Ok(text) => Ok(Some(text)),
            Err(RuntimeError::Failed { detail, .. }) if is_absent(target, &detail) => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn listing(&self, op: RuntimeOp, name: &str) -> Result<String, RuntimeError> {
        let filter = format!("name={name}");
        self.exec(
            op,
            name,
            &[
                "ps",
                "--all",
                "--filter",
                &filter,
                "--format",
                CONTAINER_STATE_FORMAT,
            ],
        )
    }

    fn image(
        &self,
        op: RuntimeOp,
        reference: &str,
    ) -> Result<Option<ImageInspectionRaw>, RuntimeError> {
        let Some(text) = self.inspect(
            op,
            reference,
            &[
                "image",
                "inspect",
                reference,
                "--format",
                "{{.Id}}\u{1f}{{join .RepoDigests \",\"}}\u{1f}{{join .RepoTags \",\"}}",
            ],
        )?
        else {
            return Ok(None);
        };
        let line = text.lines().next().unwrap_or_default();
        let fields: Vec<&str> = line.split('\u{1f}').collect();
        let id = fields
            .first()
            .copied()
            .unwrap_or_default()
            .trim()
            .to_owned();
        if id.is_empty() {
            return Err(RuntimeError::Failed {
                operation: op,
                detail: format!("`docker image inspect {reference}` reported no image id"),
            });
        }
        Ok(Some(ImageInspectionRaw {
            id,
            digests: split_list(fields.get(1).copied().unwrap_or_default()),
            tags: split_list(fields.get(2).copied().unwrap_or_default()),
        }))
    }
}

struct ImageInspectionRaw {
    id: String,
    digests: Vec<String>,
    tags: Vec<String>,
}

impl ImageInspectionRaw {
    fn into_inspection(self) -> runtime::ImageInspection {
        runtime::ImageInspection {
            id: self.id,
            digest: self
                .digests
                .into_iter()
                .next()
                .and_then(|entry| entry.rsplit('@').next().map(str::to_owned)),
            references: self.tags,
        }
    }
}

fn split_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_owned)
        .collect()
}

const PS_FIELD_SEPARATOR: char = '\u{1f}';

const CONTAINER_STATE_FORMAT: &str = "{{.Names}}\u{1f}{{.State}}";

fn listed_state<'a>(listing: &'a str, name: &str) -> Option<&'a str> {
    listing
        .lines()
        .filter_map(|line| line.split_once(PS_FIELD_SEPARATOR))
        .find(|(listed, _)| listed.trim() == name)
        .map(|(_, state)| state.trim())
}

fn liveness_of(listed: Option<&str>) -> Liveness {
    match listed {
        None => Liveness::Gone,
        Some("running" | "restarting" | "paused" | "removing") => Liveness::Running,
        Some(_) => Liveness::Exited,
    }
}

const PS_FORMAT: &str = "{{.Names}}\u{1f}{{.Label \"upstroke.private_root\"}}\
     \u{1f}{{.Label \"upstroke.run\"}}\u{1f}{{.Label \"upstroke.run_dir\"}}\
     \u{1f}{{.Label \"upstroke.incarnation\"}}\u{1f}{{.Label \"upstroke.invocation\"}}";

const PS_LABELS: &[&str] = &[
    intent::LABEL_PRIVATE_ROOT,
    intent::LABEL_RUN,
    intent::LABEL_RUN_DIR,
    intent::LABEL_INCARNATION,
    intent::LABEL_INVOCATION,
];

fn parse_ps_output(text: &str) -> Result<Vec<DiscoveredContainer>, RuntimeError> {
    let expected = 1 + PS_LABELS.len();
    let mut found = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let fields: Vec<&str> = line.split(PS_FIELD_SEPARATOR).collect();
        if fields.len() != expected {
            return Err(RuntimeError::Failed {
                operation: RuntimeOp::ListByLabel,
                detail: format!(
                    "`docker ps` rendered {} field(s) for one container and this format asks for \
                     {expected}: {line:?}. A label value carrying the field separator or a line \
                     terminator makes the values stop lining up with their names, and a census \
                     that guessed which field was the owner's run directory would probe another \
                     run's lock",
                    fields.len()
                ),
            });
        }
        let name = fields[0].trim().to_owned();
        if name.is_empty() {
            continue;
        }
        let mut labels = BTreeMap::new();
        for (key, value) in PS_LABELS.iter().zip(&fields[1..]) {
            if !value.is_empty() {
                labels.insert((*key).to_owned(), (*value).to_owned());
            }
        }
        found.push(DiscoveredContainer { name, labels });
    }
    Ok(found)
}

const UNREACHABLE_DIAGNOSTICS: &[&str] = &[
    "cannot connect to the docker daemon",
    "error during connect",
    "is the docker daemon running",
    "if the daemon is running",
    "permission denied while trying to connect",
    "failed to connect to the docker api",
    "the docker daemon is not running",
];

#[must_use]
pub fn is_unreachable_diagnostic(detail: &str) -> bool {
    if speaks_for_the_daemon(detail) {
        return false;
    }
    let lower = detail.to_ascii_lowercase();
    UNREACHABLE_DIAGNOSTICS
        .iter()
        .any(|shape| lower.contains(shape))
}

#[must_use]
pub fn classify_docker_failure(operation: RuntimeOp, detail: String) -> RuntimeError {
    if is_unreachable_diagnostic(&detail) {
        return RuntimeError::Unreachable { operation, detail };
    }
    RuntimeError::Failed { operation, detail }
}

const DAEMON_ANSWER: &str = "error response from daemon:";

fn daemon_answer_about(target: &str, detail: &str) -> Option<String> {
    let target = target.trim().to_ascii_lowercase();
    if target.is_empty() {
        return None;
    }
    daemon_lines(detail).find(|line| line.contains(&target))
}

fn speaks_for_the_daemon(detail: &str) -> bool {
    daemon_lines(detail).next().is_some()
}

fn daemon_lines(detail: &str) -> impl Iterator<Item = String> + '_ {
    detail
        .lines()
        .map(|line| line.trim().to_ascii_lowercase())
        .filter(|line| line.starts_with(DAEMON_ANSWER))
}

fn is_absent(target: &str, detail: &str) -> bool {
    daemon_answer_about(target, detail).is_some_and(|answer| {
        answer.contains("no such object")
            || answer.contains("no such container")
            || answer.contains("no such image")
            || answer.contains("no such volume")
    })
}

pub const REMOVAL_IN_PROGRESS: &str = "is already in progress";

fn removal_answer(target: &str, detail: &str) -> Option<Settled> {
    if is_absent(target, detail) {
        return Some(Settled::ProcessGone);
    }
    daemon_answer_about(target, detail)
        .is_some_and(|answer| answer.contains(REMOVAL_IN_PROGRESS))
        .then_some(Settled::RemovalInProgress)
}

fn establishes(proposed: Settled, observed: Liveness) -> bool {
    !proposed.process_gone() || observed.is_terminated()
}

fn settle(
    target: &str,
    outcome: Result<String, RuntimeError>,
    propose: fn(&str, &str) -> Option<Settled>,
    observe: impl FnOnce(&str) -> Result<Liveness, RuntimeError>,
) -> Result<Settled, RuntimeError> {
    match outcome {
        Ok(_) => Ok(Settled::ProcessGone),
        Err(RuntimeError::Failed { operation, detail }) => match propose(target, &detail) {
            Some(proposed) if !proposed.process_gone() => Ok(proposed),
            Some(proposed) => {
                if establishes(proposed, observe(target)?) {
                    Ok(proposed)
                } else {
                    Err(RuntimeError::Failed { operation, detail })
                }
            }
            None => Err(RuntimeError::Failed { operation, detail }),
        },
        Err(error) => Err(error),
    }
}

fn settle_remove(
    target: &str,
    outcome: Result<String, RuntimeError>,
    observe: impl FnOnce(&str) -> Result<Liveness, RuntimeError>,
) -> Result<Settled, RuntimeError> {
    settle(target, outcome, removal_answer, observe)
}

impl ContainerRuntime for DockerCli {
    fn probe(&self) -> Result<(), RuntimeError> {
        self.exec(
            RuntimeOp::Probe,
            "daemon",
            &["version", "--format", "{{.Server.Version}}"],
        )
        .map(|_| ())
    }

    fn image_by_reference(
        &self,
        reference: &str,
    ) -> Result<Option<runtime::ImageInspection>, RuntimeError> {
        Ok(self
            .image(RuntimeOp::InspectImageByReference, reference)?
            .map(ImageInspectionRaw::into_inspection))
    }

    fn image_by_id(&self, id: &str) -> Result<Option<runtime::ImageInspection>, RuntimeError> {
        let Some(found) = self.image(RuntimeOp::InspectImageById, id)? else {
            return Ok(None);
        };
        if found.id != id {
            return Ok(None);
        }
        Ok(Some(found.into_inspection()))
    }

    fn volume_present(&self, name: &str) -> Result<bool, RuntimeError> {
        Ok(self
            .inspect(
                RuntimeOp::InspectVolume,
                name,
                &["volume", "inspect", name, "--format", "{{.Name}}"],
            )?
            .is_some())
    }

    fn containers_with_label(
        &self,
        key: &str,
        value: &str,
    ) -> Result<Vec<DiscoveredContainer>, RuntimeError> {
        let filter = format!("label={key}={value}");
        let text = self.exec(
            RuntimeOp::ListByLabel,
            value,
            &["ps", "--all", "--filter", &filter, "--format", PS_FORMAT],
        )?;
        parse_ps_output(&text)
    }

    fn observe(&self, name: &str) -> Result<Liveness, RuntimeError> {
        let listing = self.listing(RuntimeOp::Observe, name)?;
        Ok(liveness_of(listed_state(&listing, name)))
    }

    fn collect(&self, name: &str) -> Result<ContainerExecution, RuntimeError> {
        let status = self
            .inspect(
                RuntimeOp::Collect,
                name,
                &[
                    "container",
                    "inspect",
                    name,
                    "--format",
                    "{{.State.ExitCode}}",
                ],
            )?
            .ok_or_else(|| RuntimeError::Failed {
                operation: RuntimeOp::Collect,
                detail: format!("`{name}` is gone, so its exit status cannot be collected"),
            })?;
        let exit_code = status.trim().parse::<i32>().ok();
        let (stdout, stderr) = self.exec_streams(RuntimeOp::Collect, name, &["logs", name])?;
        Ok(ContainerExecution {
            exit_code,
            stdout,
            stderr,
        })
    }

    fn create(&self, spec: &CreateSpec) -> Result<CreatedContainer, RuntimeError> {
        let mut args: Vec<String> =
            vec!["create".to_owned(), "--name".to_owned(), spec.name.clone()];
        if spec.read_only_root {
            args.push("--read-only".to_owned());
        }
        for (key, value) in &spec.labels {
            args.push("--label".to_owned());
            args.push(format!("{key}={value}"));
        }
        for mount in &spec.mounts {
            args.push("--mount".to_owned());
            args.push(mount_argument(mount));
        }
        for (key, value) in &spec.env {
            args.push("--env".to_owned());
            args.push(format!("{key}={value}"));
        }
        if let Some(workdir) = &spec.workdir {
            args.push("--workdir".to_owned());
            args.push(workdir.clone());
        }
        args.push(spec.image_id.clone());
        args.extend(spec.command.iter().cloned());
        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        self.exec(RuntimeOp::Create, &spec.name, &borrowed)?;
        let reported = self
            .inspect(
                RuntimeOp::Create,
                &spec.name,
                &["container", "inspect", &spec.name, "--format", "{{.Image}}"],
            )?
            .ok_or_else(|| RuntimeError::Failed {
                operation: RuntimeOp::Create,
                detail: format!("`{}` was created and cannot be inspected", spec.name),
            })?;
        Ok(CreatedContainer {
            name: spec.name.clone(),
            reported_image_id: reported.trim().to_owned(),
        })
    }

    fn start(&self, name: &str) -> Result<(), RuntimeError> {
        self.exec(RuntimeOp::Start, name, &["start", name])
            .map(|_| ())
    }

    fn stop(&self, name: &str, mode: StopMode) -> Result<Settled, RuntimeError> {
        let verb = match mode {
            StopMode::Graceful => "stop",
            StopMode::Kill => "kill",
        };
        settle_stop(
            name,
            self.exec(RuntimeOp::Stop, name, &[verb, name]),
            |target| self.observe(target),
        )
    }

    fn remove(&self, name: &str) -> Result<Settled, RuntimeError> {
        settle_remove(
            name,
            self.exec(
                RuntimeOp::Remove,
                name,
                &["rm", "--force", "--volumes", name],
            ),
            |target| self.observe(target),
        )
    }
}

// The daemon owns container state and arbitrates concurrent reclaimers' stop/remove
// requests. The winner moves running -> stopped -> removing -> absent; a loser
// seeing stopped, removing or absent continues observe/remove/view/intent cleanup.
// Other failures remain errors so failed or cancelled reclamation can be retried
// from the retained intent. "Removing" is settled for stop, not proof of absence.
fn stop_answer(target: &str, detail: &str) -> Option<Settled> {
    if daemon_answer_about(target, detail).is_some_and(|answer| answer.contains("is not running")) {
        return Some(Settled::ProcessGone);
    }
    removal_answer(target, detail)
}

fn settle_stop(
    target: &str,
    outcome: Result<String, RuntimeError>,
    observe: impl FnOnce(&str) -> Result<Liveness, RuntimeError>,
) -> Result<Settled, RuntimeError> {
    settle(target, outcome, stop_answer, observe)
}

fn mount_argument(mount: &runtime::Mount) -> String {
    let mut parts = Vec::new();
    match mount {
        runtime::Mount::Path { source, target, .. } => {
            parts.push("type=bind".to_owned());
            parts.push(format!(
                "source={}",
                source.to_string_lossy().replace('\\', "/")
            ));
            parts.push(format!("target={target}"));
        }
        runtime::Mount::Volume { name, target, .. } => {
            parts.push("type=volume".to_owned());
            parts.push(format!("source={name}"));
            parts.push(format!("target={target}"));
        }
        runtime::Mount::Tmpfs { target } => {
            parts.push("type=tmpfs".to_owned());
            parts.push(format!("target={target}"));
        }
    }
    if mount.read_only() {
        parts.push("readonly".to_owned());
    }
    parts.join(",")
}

#[cfg(test)]
mod fake;

#[cfg(test)]
pub(crate) use fake::{
    DOCKER_GATED_TESTS, FakeOwnerLiveness, FakeRuntime, RecordingHooks, docker_gate,
};

#[cfg(test)]
impl DockerCli {
    pub(crate) fn raw(
        &self,
        op: RuntimeOp,
        target: &str,
        args: &[&str],
    ) -> Result<String, RuntimeError> {
        self.exec(op, target, args)
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[inline]
fn note_racing_attempt(failed: usize, pause: RacingPause) {
    tests::note_racing_attempt(failed, pause);
}

#[cfg(test)]
#[inline]
fn racing_yield() {
    tests::note_racing_performed(tests::RacingPerformed::Yielded);
    std::thread::yield_now();
}

#[cfg(test)]
#[inline]
fn racing_sleep(duration: Duration) {
    tests::note_racing_performed(tests::RacingPerformed::Slept(duration));
    std::thread::sleep(duration);
}
