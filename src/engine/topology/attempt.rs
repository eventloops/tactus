//! Extended notes: `docs/internals/engine/topology/attempt.md`

use std::path::PathBuf;

use crate::agent::AdapterSource;
use crate::agent::proc::ProcessOutput;
use crate::engine::attempt::review_failure;
use crate::error::UpstrokeError;
use crate::events::ReviewRecord;
use crate::gates::GateFailure;
use crate::ir::WorkerProfile;
use crate::ladder::AttemptFailure;
use crate::review;
use crate::rundir::RunPaths;
use crate::runner::{
    AgentId, CommandSpec, InvocationId, Runner, RunnerError, RunnerRequest, gate_request,
    worker_request,
};
use crate::topology::events::{
    AttemptInterrupted4, AttemptNumber, AttemptStarted4, Materialization, RungBinding, SessionId,
    TopologyEventBody,
};
use crate::workspace_manager::{
    DeclaredResolution, ObjectId, ResolutionKind, ResolutionManifest, Slot, Snapshot,
    SnapshotInput, SnapshotName, WorkspaceManager,
};

use super::dispatch::{self, Dispatched, EventEmitter};
use super::identity::{
    AttemptIdentities, InvocationLedger, SequenceIdentities, SlotAssertion, SlotPair, is_slotted,
};
use super::seams::TopologyHooks;

#[derive(Debug, Clone)]
pub struct ReviewerPlan {
    pub agent: AgentId,
    pub profile: WorkerProfile,
    pub lens: review::Lens,
    pub preflight_cli_version: Option<String>,
    pub timeout: std::time::Duration,
}

pub struct ReviewInputs {
    pub title: String,
    pub body: String,
    pub acceptance: Vec<String>,
    pub diff: String,
    pub artifacts: Vec<(String, String)>,
    pub decisions: Vec<String>,
    pub stem: String,
}

pub struct PlanRequest<'a> {
    #[allow(dead_code)]
    pub key: crate::topology::registry::TaskKey,
    pub entry: &'a crate::topology::registry::TaskEntry,
    pub attempt: AttemptNumber,
    pub rung: u32,
    pub binding: RungBinding,
    pub workspace: &'a std::path::Path,
    pub resume_session: Option<SessionId>,
    pub feedback: Vec<crate::events::Feedback>,
    pub materialization_observed: Option<Materialization>,
}

pub struct InputsRequest<'a> {
    pub entry: &'a crate::topology::registry::TaskEntry,
    pub diff: String,
}

pub struct VerificationRequest<'a> {
    pub entry: &'a crate::topology::registry::TaskEntry,
    pub implementer: review::PassBinding,
}

#[derive(Debug, Clone)]
pub struct VerificationPlan {
    pub gates: Vec<GatePlan>,
    pub reviewers: Vec<ReviewerPlan>,
}

pub trait AttemptPlans {
    fn inputs(&self, request: &InputsRequest<'_>) -> Result<ReviewInputs, UpstrokeError>;

    fn pool_for(&self, agent: &str) -> Option<String>;

    fn plan(&self, request: &PlanRequest<'_>) -> Result<AttemptPlan, UpstrokeError>;

    fn verification(
        &self,
        request: &VerificationRequest<'_>,
    ) -> Result<VerificationPlan, UpstrokeError>;
}

pub trait ReviewInputPolicy {
    fn problem(
        &self,
        worktree: &std::path::Path,
        tree: &str,
    ) -> Result<Option<String>, UpstrokeError>;
}

pub trait ReviewPasses {
    fn run(
        &self,
        cx: &review::ReviewCx<'_>,
        runner: &dyn Runner,
        invocations: &review::ReviewInvocations,
    ) -> Result<review::ReviewOutcome, UpstrokeError>;
}

pub trait ReviewAccount {
    fn charge(&mut self, review: &ReviewRecord);
}

pub struct NoReviewAccount;

impl ReviewAccount for NoReviewAccount {
    fn charge(&mut self, _review: &ReviewRecord) {}
}

#[derive(Debug, Clone)]
pub struct AttemptPlan {
    pub attempt: AttemptNumber,
    pub rung: u32,
    pub binding: RungBinding,
    pub pool: Option<String>,
    pub resume_session: Option<SessionId>,
    pub materialization_observed: Option<Materialization>,
    pub agent: AgentId,
    pub session_resume: bool,
    pub worker: CommandSpec,
    pub worker_timeout: std::time::Duration,
    pub gates: Vec<GatePlan>,
    pub reviewers: Vec<ReviewerPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatePlan {
    pub name: String,
    pub command: CommandSpec,
    pub timeout: std::time::Duration,
}

#[derive(Debug, Clone)]
pub struct AttemptRun {
    pub identities: AttemptIdentities,
    pub worker: ProcessOutput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capture {
    pub tree: String,
    pub parent: String,
    pub unresolved: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ResolutionPlan {
    staged: Vec<DeclaredResolution>,
    refused: Vec<String>,
}

/// The worker's manifest reconciled with the paths it governs: `unmerged`,
/// the index's conflicted entries, every one of which must be declared, and
/// `resolved`, the entries a previous capture of this generation resolved and
/// the index still holds, which a declaration may revise and silence leaves to
/// the ordinary `add -A`, which stages the path's edits or deletion like any
/// other's. A declaration naming a path in neither list does nothing.
///
/// Per governed path: declared one way as the index spells it, staged that
/// way; declared both ways, refused; declared one way exactly and the other
/// way in another case (`Declaration::names_in_another_case`, a spelling that
/// governs nothing itself), refused as a contradiction, since a
/// case-insensitive filesystem reads the two as one file; declared only in
/// another case, refused naming the index's spelling; undeclared, refused if
/// unmerged and left to the `add -A` if already resolved. A malformed manifest refuses
/// every governed path and quotes the line. The refusal list is what the
/// worker is told (`classify::unresolved_conflict_failure`).
fn plan_resolutions(
    unmerged: &[String],
    resolved: &[String],
    manifest: &ResolutionManifest,
) -> ResolutionPlan {
    use crate::workspace_manager::{DELETED_KEYWORD, RESOLVED_KEYWORD};

    let governed: Vec<(&String, bool)> = unmerged
        .iter()
        .map(|path| (path, true))
        .chain(
            resolved
                .iter()
                .filter(|path| !unmerged.contains(path))
                .map(|path| (path, false)),
        )
        .collect();
    let declarations = match manifest {
        ResolutionManifest::Absent => &[][..],
        ResolutionManifest::Declared(declarations) => declarations.as_slice(),
        ResolutionManifest::Malformed { detail } => {
            let mut refused: Vec<String> =
                governed.iter().map(|(path, _)| (*path).clone()).collect();
            refused.push(format!("({detail})"));
            return ResolutionPlan {
                staged: Vec::new(),
                refused,
            };
        }
    };
    let keyword = |kind: ResolutionKind| match kind {
        ResolutionKind::Resolved => RESOLVED_KEYWORD,
        ResolutionKind::Deleted => DELETED_KEYWORD,
    };
    let mut plan = ResolutionPlan::default();
    for &(path, unmerged_now) in &governed {
        let exact = |kind: ResolutionKind| {
            declarations
                .iter()
                .any(|declaration| declaration.kind == kind && declaration.names(path))
        };
        // A declaration in another case whose own spelling governs no path is
        // an alias of this one on a case-insensitive filesystem, and nothing on
        // a case-sensitive one; either way it is refused, never matched.
        let alias = |kind: ResolutionKind| {
            declarations
                .iter()
                .find(|declaration| {
                    declaration.kind == kind
                        && declaration.names_in_another_case(path)
                        && !governed.iter().any(|(other, _)| declaration.names(other))
                })
                .map(|declaration| declaration.path.clone())
        };
        let resolved_exactly = exact(ResolutionKind::Resolved);
        let deleted_exactly = exact(ResolutionKind::Deleted);
        let contradiction = match (resolved_exactly, deleted_exactly) {
            (true, true) => Some(format!(
                "{path} (declared both `{RESOLVED_KEYWORD}` and `{DELETED_KEYWORD}`)"
            )),
            (true, false) => alias(ResolutionKind::Deleted).map(|spelling| {
                format!(
                    "{path} (declared `{RESOLVED_KEYWORD}`, and `{DELETED_KEYWORD}` as \
                     `{spelling}`, a spelling that differs only by case)"
                )
            }),
            (false, true) => alias(ResolutionKind::Resolved).map(|spelling| {
                format!(
                    "{path} (declared `{DELETED_KEYWORD}`, and `{RESOLVED_KEYWORD}` as \
                     `{spelling}`, a spelling that differs only by case)"
                )
            }),
            (false, false) => None,
        };
        if let Some(refusal) = contradiction {
            plan.refused.push(refusal);
            continue;
        }
        if resolved_exactly || deleted_exactly {
            plan.staged.push(DeclaredResolution {
                path: path.clone(),
                kind: if resolved_exactly {
                    ResolutionKind::Resolved
                } else {
                    ResolutionKind::Deleted
                },
            });
            continue;
        }
        let only_in_another_case = [ResolutionKind::Resolved, ResolutionKind::Deleted]
            .into_iter()
            .find_map(|kind| alias(kind).map(|spelling| (kind, spelling)));
        if let Some((kind, spelling)) = only_in_another_case {
            plan.refused.push(format!(
                "{path} (declared `{}` only as `{spelling}`, which differs from the index's \
                 spelling by case alone; spell the path as the index does)",
                keyword(kind)
            ));
        } else if unmerged_now {
            plan.refused.push(path.clone());
        }
    }
    plan
}

fn captured_object_id(source: &str, value: String) -> Result<ObjectId, UpstrokeError> {
    ObjectId::new(value).map_err(|refusal| UpstrokeError::Git {
        message: format!("{source} did not yield an object id: {refusal}"),
    })
}

#[derive(Debug, Clone)]
pub struct Assessment {
    pub outcome: crate::ir::Outcome,
    pub failure: Option<AttemptFailure>,
}

#[derive(Debug, Clone, Copy)]
pub struct AttemptSite<'a> {
    pub key: crate::topology::registry::TaskKey,
    pub generation: crate::topology::events::GenerationId,
    pub base: &'a crate::topology::events::CommitSha,
    pub slot: &'a Slot,
    pub worktree: &'a std::path::Path,
}

impl Dispatched {
    #[must_use]
    pub fn site(&self) -> AttemptSite<'_> {
        AttemptSite {
            key: self.key,
            generation: self.generation,
            base: &self.base,
            slot: &self.slot,
            worktree: &self.worktree,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Judging<'a> {
    pub run: &'a AttemptRun,
    pub capture: &'a Capture,
    pub assessed: &'a Assessment,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub output_limited: bool,
    pub timed_out: bool,
    pub log: String,
    pub invocation: InvocationId,
    pub workspace: PathBuf,
    pub code: Option<i32>,
}

impl Verdict {
    #[must_use]
    pub fn passed(&self) -> bool {
        self.code == Some(0)
    }
}

#[derive(Debug, Clone)]
pub struct Judgement {
    pub gates: Vec<Verdict>,
    pub reviews: Vec<ReviewRecord>,
    pub failure: Option<AttemptFailure>,
}

impl Judgement {
    #[must_use]
    pub fn accepted(&self) -> bool {
        self.failure.is_none()
    }

    #[must_use]
    pub fn timed_out_gate(&self) -> Option<&Verdict> {
        self.gates.iter().find(|verdict| verdict.timed_out)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptOutcome {
    Interrupted,
    Cancelled,
}

impl AttemptOutcome {
    const fn detail(self) -> &'static str {
        match self {
            Self::Interrupted => {
                "a coordinator died holding this attempt; the spend is unknown and nothing was \
                 judged"
            }
            Self::Cancelled => {
                "the run halted while this attempt was in flight; its invocations were cancelled \
                 and the spend is unknown"
            }
        }
    }
}

pub struct AttemptContext<'a> {
    pub manager: &'a WorkspaceManager,
    pub hooks: &'a mut dyn TopologyHooks,
    pub emitter: &'a mut dyn EventEmitter,
    pub runner: &'a dyn Runner,
    pub slots: &'a mut SlotAssertion,
    pub ledger: &'a mut InvocationLedger,
    pub adapters: &'a dyn AdapterSource,
    pub paths: &'a RunPaths,
    pub reviews: &'a dyn ReviewPasses,
    pub input_policy: &'a dyn ReviewInputPolicy,
}

impl AttemptContext<'_> {
    fn emit(&mut self, body: TopologyEventBody) -> Result<(), UpstrokeError> {
        self.emitter
            .emit(body, self.hooks)
            .map_err(|failure| failure.discharging(self.ledger))
    }

    fn judge_core(&mut self) -> Judge<'_> {
        Judge {
            manager: self.manager,
            hooks: &mut *self.hooks,
            runner: self.runner,
            slots: &mut *self.slots,
            ledger: &mut *self.ledger,
            adapters: self.adapters,
            paths: self.paths,
            reviews: self.reviews,
        }
    }

    pub fn start(
        &mut self,
        site: AttemptSite<'_>,
        plan: &AttemptPlan,
    ) -> Result<AttemptRun, UpstrokeError> {
        self.emit(TopologyEventBody::AttemptStarted {
            data: AttemptStarted4 {
                key: site.key,
                generation: site.generation,
                attempt: plan.attempt,
                rung: plan.rung,
                binding: plan.binding.clone(),
                pool: plan.pool.clone(),
                resume_session: plan.resume_session.clone(),
                materialization_observed: plan.materialization_observed,
            },
        })?;

        self.run_worker(site, plan)
    }

    pub fn run_worker(
        &mut self,
        site: AttemptSite<'_>,
        plan: &AttemptPlan,
    ) -> Result<AttemptRun, UpstrokeError> {
        let identities = AttemptIdentities::new(site.key, site.generation, plan.attempt);
        let invocation = identities.worker();
        let request = worker_request(
            plan.worker.clone(),
            site.worktree.to_path_buf(),
            plan.agent.clone(),
            plan.worker_timeout,
            invocation.clone(),
        );
        let worker = self.judge_core().execute(&request, plan.pool.clone())?;
        Ok(AttemptRun { identities, worker })
    }

    pub fn capture(&mut self, site: AttemptSite<'_>) -> Result<Capture, UpstrokeError> {
        // What the manifest governs: the index's unmerged entries, and the
        // entries a previous capture of this generation resolved (a retained
        // retry revising one). When the index holds neither, the manifest is
        // not read, and whatever the file says has no effect on this capture
        // — nor on a later one: the staging removes the worker's manifest
        // whether or not it was read (`candidate_stage`), so that a
        // declaration is applied once, by the capture of the attempt that
        // wrote it. A refused manifest outlives its capture (a refusal stages
        // nothing), as does one whose capture fails before that removal; a
        // further capture of this worktree would read either again, and the
        // driver makes none — a refusal is not resumable and a capture error
        // interrupts the attempt, and either closes the generation
        // (`RESOLUTION_MANIFEST`'s doc, `design/26` §26.4).
        let unmerged = self.manager.unresolved_conflicts(site.slot)?;
        let resolved = self.manager.resolved_conflicts(site.slot)?;
        let resolutions = if unmerged.is_empty() && resolved.is_empty() {
            Vec::new()
        } else {
            let manifest = self.manager.resolution_manifest(site.slot)?;
            let plan = plan_resolutions(&unmerged, &resolved, &manifest);
            if !plan.refused.is_empty() {
                let tree = self
                    .manager
                    .commit_tree_sha(site.base.as_str())?
                    .ok_or_else(|| UpstrokeError::Git {
                        message: format!(
                            "the recorded base {} has no tree; an unresolved capture cannot \
                             name the tree the worktree started from",
                            site.base
                        ),
                    })?;
                return Ok(Capture {
                    tree,
                    parent: site.base.0.clone(),
                    unresolved: plan.refused,
                });
            }
            plan.staged
        };
        self.manager
            .candidate_stage(self.hooks.effects(), site.slot, &resolutions)?;
        if !resolutions.is_empty() {
            let left = self.manager.unresolved_conflicts(site.slot)?;
            if !left.is_empty() {
                return Err(UpstrokeError::Git {
                    message: format!(
                        "the capture staged every declared resolution and the index of {} \
                         still holds unmerged entries: {}",
                        site.worktree.display(),
                        left.join(", ")
                    ),
                });
            }
        }
        let tree = self
            .manager
            .candidate_write_tree(self.hooks.effects(), site.slot)?;
        Ok(Capture {
            tree,
            parent: site.base.0.clone(),
            unresolved: Vec::new(),
        })
    }

    pub fn assess(
        &mut self,
        site: AttemptSite<'_>,
        plan: &AttemptPlan,
        run: &AttemptRun,
        capture: &Capture,
        diff: &str,
        kind: crate::ir::TaskKind,
    ) -> Result<Assessment, UpstrokeError> {
        let adapter =
            self.adapters
                .get(plan.agent.as_str())
                .ok_or_else(|| UpstrokeError::Refused {
                    message: format!(
                        "this attempt ran as agent `{}` and no adapter answers to that name",
                        plan.agent.as_str()
                    ),
                })?;
        let mut outcome = adapter.parse(&run.worker)?;
        outcome.diff = diff.to_owned();

        // The worker's own end (an error exit, a timeout, a question it asked) is
        // reported as what it is; a completed worker that left conflicted paths is
        // reported as that, before the diff is looked at, since an unresolved capture
        // carries the base's tree and the diff-shaped verdicts would misdescribe it.
        let mut failure = if outcome.status == crate::ir::OutcomeStatus::Completed
            && !capture.unresolved.is_empty()
        {
            Some(crate::engine::classify::unresolved_conflict_failure(
                &capture.unresolved,
            ))
        } else {
            crate::engine::attempt::evaluate_outcome(&outcome, &run.worker)
        };
        if failure.is_none() {
            failure = crate::engine::classify::diff_failure(
                &outcome.diff,
                kind,
                !plan.reviewers.is_empty(),
            );
        }
        if failure.is_none() {
            if let Some(problem) = self.input_policy.problem(site.worktree, &capture.tree)? {
                failure = Some(crate::engine::classify::review_input_failure(problem));
            }
        }
        Ok(Assessment { outcome, failure })
    }

    pub fn judge(
        &mut self,
        site: AttemptSite<'_>,
        plan: &AttemptPlan,
        judging: Judging<'_>,
        inputs: &ReviewInputs,
        invocations: &dyn Fn(u32) -> review::ReviewInvocations,
    ) -> Result<Judgement, UpstrokeError> {
        let Judging {
            run,
            capture,
            assessed,
        } = judging;
        let subject = Subject {
            snapshot: SnapshotOf::Tree {
                tree: captured_object_id("`git write-tree`", capture.tree.clone())?,
                parent: captured_object_id("the recorded base commit", capture.parent.clone())?,
            },
            disposal: SnapshotDisposal::AsEachRoleFinishes,
            names: JudgeNames::Attempt {
                generation: site.generation.0,
                attempt: plan.attempt.0,
            },
            identities: JudgeIdentities::Attempt(run.identities),
            stem: format!("{}-{}", inputs.stem, plan.attempt.0),
            gates: &plan.gates,
            reviewers: &plan.reviewers,
            inputs,
            prior_failure: assessed.failure.clone(),
            invocations,
        };
        Ok(self.judge_core().judge(&subject, &mut NoReviewAccount)?)
    }

    pub fn settle_interrupted(
        &mut self,
        dispatched: &Dispatched,
        attempt: AttemptNumber,
        outcome: AttemptOutcome,
    ) -> Result<(), UpstrokeError> {
        self.emit(TopologyEventBody::AttemptInterrupted {
            data: AttemptInterrupted4 {
                key: dispatched.key,
                generation: dispatched.generation,
                attempt,
                lease: dispatched.closing_disposition(),
                detail: outcome.detail().to_owned(),
            },
        })?;
        self.discard_residue(dispatched)
    }

    // Called between synchronous Runner invocations, after their children exit.
    // This context cancels remaining registrations and releases the held slot pair
    // before appending the terminal event and reclaiming the attempt's residue.
    pub fn cancel_in_flight(
        &mut self,
        dispatched: &Dispatched,
        attempt: AttemptNumber,
    ) -> Result<usize, UpstrokeError> {
        let cancelled = self.ledger.cancel_all_running();
        if let Some(held) = self.slots.held().cloned() {
            self.slots.release(&held)?;
        }
        self.settle_interrupted(dispatched, attempt, AttemptOutcome::Cancelled)?;
        Ok(cancelled)
    }

    fn discard_residue(&mut self, dispatched: &Dispatched) -> Result<(), UpstrokeError> {
        for slot in self.manager.intents()? {
            if matches!(slot, Slot::Snapshot { .. }) {
                self.manager.remove_worktree(self.hooks.effects(), &slot)?;
                self.manager.remove_intent(self.hooks.effects(), &slot)?;
            }
        }
        dispatch::scrub(self.manager, self.hooks, &dispatched.slot)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotOf {
    Tree { tree: ObjectId, parent: ObjectId },
    Commit(ObjectId),
}

impl SnapshotOf {
    fn input(&self) -> SnapshotInput {
        match self {
            Self::Tree { tree, parent } => SnapshotInput::Tree {
                tree: tree.clone(),
                parent: parent.clone(),
            },
            Self::Commit(commit) => SnapshotInput::Commit(commit.clone()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JudgeNames {
    Attempt { generation: u32, attempt: u32 },
    Integration { sequence: u64 },
}

impl JudgeNames {
    fn gates(self) -> SnapshotName {
        match self {
            Self::Attempt {
                generation,
                attempt,
            } => SnapshotName::gates(generation, attempt),
            Self::Integration { sequence } => SnapshotName::integration(sequence),
        }
    }

    fn review(self, pass: u32) -> SnapshotName {
        match self {
            Self::Attempt {
                generation,
                attempt,
            } => SnapshotName::review(generation, attempt, pass),
            Self::Integration { sequence } => SnapshotName::integration_review(sequence, pass),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JudgeIdentities {
    Attempt(AttemptIdentities),
    Sequence(SequenceIdentities),
}

impl JudgeIdentities {
    fn gate(self, gate: u32, ordinal: u32) -> InvocationId {
        match self {
            Self::Attempt(ids) => ids.gate(gate, ordinal),
            Self::Sequence(ids) => ids.gate(gate, ordinal),
        }
    }

    fn review_reask(self, reask: u32, ordinal: u32) -> InvocationId {
        match self {
            Self::Attempt(ids) => ids.review_reask(reask, ordinal),
            Self::Sequence(ids) => ids.review_reask(reask, ordinal),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotDisposal {
    AsEachRoleFinishes,
    AfterTheTerminal,
}

pub struct Subject<'s> {
    pub snapshot: SnapshotOf,
    pub disposal: SnapshotDisposal,
    pub names: JudgeNames,
    pub identities: JudgeIdentities,
    pub stem: String,
    pub gates: &'s [GatePlan],
    pub reviewers: &'s [ReviewerPlan],
    pub inputs: &'s ReviewInputs,
    pub prior_failure: Option<AttemptFailure>,
    pub invocations: &'s dyn Fn(u32) -> review::ReviewInvocations,
}

#[derive(Debug, thiserror::Error)]
pub enum JudgeError {
    #[error(transparent)]
    Runner(RunnerError),
    #[error(transparent)]
    Other(UpstrokeError),
}

impl From<JudgeError> for UpstrokeError {
    fn from(error: JudgeError) -> Self {
        match error {
            JudgeError::Runner(error) => error.into(),
            JudgeError::Other(error) => error,
        }
    }
}

pub struct Judge<'a> {
    pub manager: &'a WorkspaceManager,
    pub hooks: &'a mut dyn TopologyHooks,
    pub runner: &'a dyn Runner,
    pub slots: &'a mut SlotAssertion,
    pub ledger: &'a mut InvocationLedger,
    pub adapters: &'a dyn AdapterSource,
    pub paths: &'a RunPaths,
    pub reviews: &'a dyn ReviewPasses,
}

impl Judge<'_> {
    pub fn judge(
        &mut self,
        subject: &Subject<'_>,
        account: &mut dyn ReviewAccount,
    ) -> Result<Judgement, JudgeError> {
        let mut failure = subject.prior_failure.clone();
        let mut gates = Vec::with_capacity(subject.gates.len());
        if !subject.gates.is_empty() && failure.is_none() {
            let snapshot = self
                .snapshot(subject.names.gates(), &subject.snapshot)
                .map_err(JudgeError::Other)?;
            for (index, gate) in subject.gates.iter().enumerate() {
                let invocation = subject
                    .identities
                    .gate(u32::try_from(index).unwrap_or(u32::MAX), 0);
                let request = gate_request(
                    gate.command.clone(),
                    snapshot.path().to_path_buf(),
                    gate.timeout,
                    invocation,
                );
                let verdict = self.verdict(&request, None)?;
                let refused = !verdict.passed();
                if refused && failure.is_none() {
                    failure = Some(crate::engine::classify::gate_failure(&GateFailure {
                        gate: gate.name.clone(),
                        summary: format!(
                            "exit {}{}{}",
                            verdict
                                .code
                                .map_or_else(|| "signal".to_owned(), |code| code.to_string()),
                            if verdict.timed_out {
                                " (timed out)"
                            } else {
                                ""
                            },
                            if verdict.output_limited {
                                " (output truncated)"
                            } else {
                                ""
                            }
                        ),
                        log_tail: crate::util::tail(
                            &verdict.log,
                            crate::gates::FEEDBACK_TAIL_BYTES,
                        ),
                    }));
                }
                gates.push(verdict);
                if refused {
                    break;
                }
            }
            if subject.disposal == SnapshotDisposal::AsEachRoleFinishes {
                self.manager
                    .remove_snapshot(self.hooks.effects(), &snapshot)
                    .map_err(JudgeError::Other)?;
            }
        }

        let mut reviews = Vec::with_capacity(subject.reviewers.len());
        for (index, reviewer) in subject.reviewers.iter().enumerate() {
            if failure.is_some() {
                break;
            }
            let pass = u32::try_from(index).unwrap_or(u32::MAX);
            let snapshot = self
                .snapshot(subject.names.review(pass), &subject.snapshot)
                .map_err(JudgeError::Other)?;
            let adapter = self.adapters.get(reviewer.agent.as_str()).ok_or_else(|| {
                JudgeError::Other(UpstrokeError::Refused {
                    message: format!(
                        "review pass {pass} is bound to agent `{}` and no adapter answers to that \
                         name; pre-flight probed the agents this run recorded and this is not one \
                         of them",
                        reviewer.agent.as_str()
                    ),
                })
            })?;
            let inputs = subject.inputs;
            let outcome = self
                .reviews
                .run(
                    &review::ReviewCx {
                        adapter,
                        profile: reviewer.profile.clone(),
                        lens: reviewer.lens,
                        task: review::ReviewSubject {
                            title: &inputs.title,
                            body: &inputs.body,
                            acceptance: &inputs.acceptance,
                        },
                        diff: &inputs.diff,
                        artifacts: &inputs.artifacts,
                        decisions: &inputs.decisions,
                        workspace: snapshot.path(),
                        settings_dir: &self.paths.settings(),
                        reviews_dir: &self.paths.reviews(),
                        stem: subject.stem.clone(),
                        timeout: reviewer.timeout,
                    },
                    self.runner,
                    &(subject.invocations)(pass),
                )
                .map_err(JudgeError::Other)?;

            let unavailable = matches!(outcome.result, review::ReviewResult::Unavailable { .. });
            let cost_usd = outcome.cost_usd;
            let invocations = outcome.invocations;
            failure = review_failure(outcome.result, outcome.never_started);
            let record = super::super::classify::ReviewPassFacts {
                pass: reviewer.lens.name(),
                agent: &reviewer.profile.agent,
                model: &reviewer.profile.model,
                adapter: adapter.id(),
                preflight_cli_version: reviewer.preflight_cli_version.clone(),
                effort: reviewer.profile.effort,
                pool: crate::engine::attempt::pool_option(&reviewer.profile.pool),
                cost_usd,
                unavailable,
                failed: failure.is_some(),
            }
            .record();
            account.charge(&record);
            reviews.push(record);

            let ids = (subject.invocations)(pass);
            for ordinal in 0..invocations {
                let id = if ordinal == 0 {
                    ids.pass.clone()
                } else {
                    subject.identities.review_reask(pass, ordinal - 1)
                };
                self.ledger.register(&id).map_err(JudgeError::Other)?;
                self.ledger.complete(&id).map_err(JudgeError::Other)?;
            }

            if subject.disposal == SnapshotDisposal::AsEachRoleFinishes {
                self.manager
                    .remove_snapshot(self.hooks.effects(), &snapshot)
                    .map_err(JudgeError::Other)?;
            }
        }

        Ok(Judgement {
            gates,
            reviews,
            failure,
        })
    }

    fn snapshot(&mut self, name: SnapshotName, of: &SnapshotOf) -> Result<Snapshot, UpstrokeError> {
        self.manager
            .add_snapshot(self.hooks.effects(), &name, &of.input())
    }

    // This sequential context owns the invocation ledger and slot assertion.
    // Register before acquiring the atomic agent/pool pair; a refused acquisition
    // cancels that registration. Runner::run returns before the pair is released,
    // then each registered invocation is completed or cancelled exactly once.
    // Keep settlement here so a run_registered error cannot leave a Running entry.
    pub fn execute(
        &mut self,
        request: &RunnerRequest,
        pool: Option<String>,
    ) -> Result<ProcessOutput, UpstrokeError> {
        self.execute_typed(request, pool).map_err(Into::into)
    }

    fn execute_typed(
        &mut self,
        request: &RunnerRequest,
        pool: Option<String>,
    ) -> Result<ProcessOutput, JudgeError> {
        self.ledger
            .register(&request.invocation)
            .map_err(JudgeError::Other)?;
        match self.run_registered(request, pool) {
            Ok(Ok(output)) => {
                self.ledger
                    .complete(&request.invocation)
                    .map_err(JudgeError::Other)?;
                Ok(output)
            }
            Ok(Err(error)) => {
                self.ledger
                    .cancel(&request.invocation)
                    .map_err(JudgeError::Other)?;
                Err(JudgeError::Runner(error))
            }
            Err(error) => {
                drop(self.ledger.cancel(&request.invocation));
                Err(JudgeError::Other(error))
            }
        }
    }

    fn run_registered(
        &mut self,
        request: &RunnerRequest,
        pool: Option<String>,
    ) -> Result<Result<ProcessOutput, RunnerError>, UpstrokeError> {
        let slotted = is_slotted(&request.invocation);
        if slotted {
            let agent = request
                .agent
                .as_ref()
                .ok_or_else(|| UpstrokeError::Refused {
                    message: format!(
                        "`{}` is a slotted invocation and its request names no agent; the pair it \
                         would take is `{{agent, pool?}}` and there is no agent to key it by",
                        request.invocation
                    ),
                })?;
            self.slots.acquire(
                &request.invocation,
                SlotPair {
                    agent: agent.as_str().to_owned(),
                    pool,
                },
            )?;
        }
        let output = self.runner.run(request);
        if slotted {
            self.slots.release(&request.invocation)?;
        }
        Ok(output)
    }

    fn verdict(
        &mut self,
        request: &RunnerRequest,
        pool: Option<String>,
    ) -> Result<Verdict, JudgeError> {
        let output = self.execute_typed(request, pool)?;
        Ok(Verdict {
            output_limited: output.output_limited,
            timed_out: output.timed_out,
            log: format!("{}{}", output.stdout, output.stderr),
            invocation: request.invocation.clone(),
            workspace: request.workspace.clone(),
            code: output.code,
        })
    }
}

#[cfg(test)]
mod tests;
