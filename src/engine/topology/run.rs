//! Extended notes: `docs/internals/engine/topology/run.md`

use std::collections::BTreeMap;

use crate::error::{ProcessFate, UpstrokeError};
use crate::events::RunOutcome;
use crate::ir::{Answer, Question, QuestionId};
use crate::review;
use crate::topology::events::Answer4;
use crate::topology::events::TopologyEventBody;
use crate::topology::fold::QuestionOrigin;

use crate::events::AttemptRecord;
use crate::interaction::Sleeper;
use crate::topology::events::{
    AttemptNumber, CandidateLeaseEffect, CandidateRef, CommitSha, FrozenQuestion,
    GenerationCloseReason, GenerationId, InfrastructureKind, Materialization, SequenceId,
    SessionId, TopologyEvent,
};
use crate::topology::fold::{FrozenInputs, TopologyFold};
use crate::topology::registry::TaskKey;
use crate::workspace_manager::WorkspaceManager;

use super::attempt::{
    Assessment, AttemptContext, AttemptPlan, AttemptPlans, AttemptSite, Capture, InputsRequest,
    Judge, JudgeError, JudgeIdentities, JudgeNames, Judgement, Judging, PlanRequest,
    ReviewInputPolicy, ReviewPasses, SnapshotDisposal, SnapshotOf, Subject, VerificationRequest,
};
use super::candidate::{
    CandidateJournal, JudgedTree, append_candidate_created, append_candidate_prepared,
    create_candidates_ref, pin_candidate, reclaim_after_creation, write_candidate_commit,
};
use super::closure;
use super::dispatch::{
    DispatchKind, DispatchRequest, Dispatched, EventEmitter, OpenGeneration, dispatch,
    resume_open_no_attempt, task_slot,
};
use super::emit::{EmitFailure, EmitState, RunIdentity, emit};
use super::finalize;
use super::identity::{
    InvocationLedger, ReservationKind, Reservations, SequenceIdentities, SlotAssertion,
};
use super::integrate::{
    self, IntegrationJournal, IntegrationRequest, Terminal, Verification, Verified, VerifyRequest,
};
use super::recover::RunHandle;
use super::seams::{IdSource, TimeSource, TopologyHooks};
use super::select::{Admitted, Ceiling, Spend, Step, checkpoint, select};
use super::settle::{
    Deferral, FinishedAttempt, ManagedWorktrees, RetryOutcome, RetryRequest, close_generation,
    retry, settle_failed,
};

pub struct RunEmitter<'a> {
    pub identity: &'a RunIdentity,
    pub state: EmitState<'a>,
    pub clock: &'a dyn TimeSource,
}

impl EventEmitter for RunEmitter<'_> {
    fn emit(
        &mut self,
        body: TopologyEventBody,
        hooks: &mut dyn TopologyHooks,
    ) -> Result<(), EmitFailure> {
        emit(self.identity, &mut self.state, self.clock, body, hooks)?;
        Ok(())
    }
}

struct RunJournal<'a, 'h> {
    emitter: RunEmitter<'a>,
    hooks: &'h mut dyn TopologyHooks,
    invocations: &'h mut InvocationLedger,
}

impl CandidateJournal for RunJournal<'_, '_> {
    fn emit(&mut self, body: TopologyEventBody) -> Result<(), UpstrokeError> {
        self.emitter
            .emit(body, self.hooks)
            .map_err(|failure| failure.discharging(self.invocations))
    }

    fn fold(&self) -> &TopologyFold {
        self.emitter.state.fold
    }
}

struct IntegrationCx<'a, 'h> {
    emitter: RunEmitter<'a>,
    hooks: &'h mut dyn TopologyHooks,
    invocations: &'h mut InvocationLedger,
    slots: &'h mut SlotAssertion,
    spend: &'h mut Spend,
    seams: &'h RunSeams<'a>,
}

impl IntegrationJournal for IntegrationCx<'_, '_> {
    fn emit(&mut self, body: TopologyEventBody) -> Result<(), UpstrokeError> {
        self.emitter
            .emit(body, self.hooks)
            .map_err(|failure| failure.discharging(self.invocations))
    }

    fn fold(&self) -> &TopologyFold {
        self.emitter.state.fold
    }

    fn hooks(&mut self) -> &mut dyn TopologyHooks {
        &mut *self.hooks
    }

    fn converted(&mut self, key: TaskKey) -> Result<(), UpstrokeError> {
        self.emitter
            .state
            .reservations
            .convert(key, ReservationKind::Integration)
    }
}

fn implementer_binding(
    fold: &TopologyFold,
    key: TaskKey,
) -> Result<crate::review::PassBinding, UpstrokeError> {
    let rung = fold
        .task(key)
        .ok_or_else(|| UpstrokeError::Refused {
            message: format!("task {key} is not in this run's fold"),
        })?
        .rung;
    let binding = fold
        .rung_binding(key, rung)
        .ok_or_else(|| UpstrokeError::Refused {
            message: format!(
                "task {key}'s candidate was produced at rung {rung} and neither a validated \
                 one-off binding nor a frozen rung binds it, so there is no implementer to \
                 select its reviewers against"
            ),
        })?;
    Ok(crate::review::PassBinding::new(
        &binding.agent,
        &binding.model,
    ))
}

impl Verification for IntegrationCx<'_, '_> {
    fn verify(&mut self, request: &VerifyRequest<'_>) -> Result<Verified, UpstrokeError> {
        let mut charged = Vec::new();
        match self.judge_proposal(request, &mut charged) {
            Ok(judgement) => Ok(Verified::Judged(judgement)),
            Err(JudgeError::Runner(error)) => match error.fate {
                ProcessFate::NeverStarted => Ok(Verified::Unavailable {
                    kind: InfrastructureKind::RunnerSpawnFailure,
                    detail: error.to_string(),
                    reviews: charged,
                }),
                ProcessFate::Gone => Ok(Verified::Unavailable {
                    kind: InfrastructureKind::Other {
                        detail: format!(
                            "the Runner lost `{}` after its process started and has since \
                             established the process is gone; nothing was judged",
                            error.invocation
                        ),
                    },
                    detail: error.to_string(),
                    reviews: charged,
                }),
                ProcessFate::Unresolved => Err(error.into()),
            },
            Err(JudgeError::Other(UpstrokeError::Git { message })) => Ok(Verified::Unavailable {
                kind: InfrastructureKind::Other {
                    detail: format!(
                        "foreign Git state observed by the verification of sequence {}: {message}",
                        request.sequence.0
                    ),
                },
                detail: message,
                reviews: charged,
            }),
            Err(JudgeError::Other(error)) => Err(error),
        }
    }

    fn ids(&self) -> &dyn super::seams::IdSource {
        self.seams.ids
    }
}

impl IntegrationCx<'_, '_> {
    fn judge_proposal(
        &mut self,
        request: &VerifyRequest<'_>,
        charged: &mut Vec<crate::events::ReviewRecord>,
    ) -> Result<Judgement, JudgeError> {
        let key = request.candidate.key;
        let (entry, base, implementer) = {
            let fold = &*self.emitter.state.fold;
            let entry = fold
                .registry()
                .and_then(|registry| registry.get(key))
                .cloned()
                .ok_or_else(|| {
                    JudgeError::Other(UpstrokeError::Refused {
                        message: format!("task {key} is not in this run's registry"),
                    })
                })?;
            let base = fold
                .task(key)
                .and_then(|task| {
                    task.generations
                        .iter()
                        .find(|generation| generation.id == request.candidate.generation)
                })
                .and_then(|generation| generation.candidate.as_ref())
                .map(|prepared| prepared.base_sha.clone());
            (
                entry,
                base,
                implementer_binding(fold, key).map_err(JudgeError::Other)?,
            )
        };

        let (diff_parent, diff_tree) = if request.already_present {
            let base = base.ok_or_else(|| {
                JudgeError::Other(UpstrokeError::Refused {
                    message: "an already-present verification needs the candidate's recorded \
                              base to review its original patch"
                        .to_owned(),
                })
            })?;
            (base.0, request.candidate.commit_sha.0.clone())
        } else {
            (request.head.0.clone(), request.proposed.0.clone())
        };
        let diff = self
            .seams
            .manager
            .candidate_diff(request.staging, &diff_parent, &diff_tree)
            .map_err(JudgeError::Other)?;

        let plan = self
            .seams
            .plans
            .verification(&VerificationRequest {
                entry: &entry,
                implementer,
            })
            .map_err(JudgeError::Other)?;

        let prior_failure =
            match crate::engine::classify::unjudgeable_diff(&diff, !plan.reviewers.is_empty()) {
                Some(failure) => Some(failure),
                None => {
                    let tree = self
                        .seams
                        .manager
                        .commit_tree_sha(request.proposed.as_str())
                        .map_err(JudgeError::Other)?
                        .ok_or_else(|| {
                            JudgeError::Other(UpstrokeError::Git {
                                message: format!(
                                    "the proposed commit {} has no tree; the review-input policy \
                                     cannot be consulted for it",
                                    request.proposed
                                ),
                            })
                        })?;
                    let staging = self.seams.manager.slot_path(request.staging);
                    self.seams
                        .input_policy
                        .problem(&staging, &tree)
                        .map_err(JudgeError::Other)?
                        .map(crate::engine::classify::review_input_failure)
                }
            };

        let inputs = self
            .seams
            .plans
            .inputs(&InputsRequest {
                entry: &entry,
                diff,
            })
            .map_err(JudgeError::Other)?;

        let proposed = crate::workspace_manager::ObjectId::new(request.proposed.0.clone())
            .map_err(|refusal| {
                JudgeError::Other(UpstrokeError::Refused {
                    message: format!(
                        "the recorded proposal of sequence {} is not an object id: {refusal}",
                        request.sequence.0
                    ),
                })
            })?;
        let identities = SequenceIdentities::new(request.sequence);
        let mut judge = Judge {
            manager: self.seams.manager,
            hooks: &mut *self.hooks,
            runner: self.seams.runner,
            slots: self.slots,
            ledger: self.invocations,
            adapters: self.seams.adapters,
            paths: self.seams.paths,
            reviews: self.seams.reviews,
        };
        let mut account = SpendAccount {
            spend: &mut *self.spend,
            key,
            charged,
        };
        judge.judge(
            &Subject {
                snapshot: SnapshotOf::Commit(proposed),
                disposal: SnapshotDisposal::AfterTheTerminal,
                names: JudgeNames::Integration {
                    sequence: u64::from(request.sequence.0),
                },
                identities: JudgeIdentities::Sequence(identities),
                stem: format!("integration-s{}", request.sequence.0),
                gates: &plan.gates,
                reviewers: &plan.reviewers,
                inputs: &inputs,
                prior_failure,
                invocations: &move |pass| review::ReviewInvocations {
                    pass: identities.review_pass(pass, 0),
                    reask: identities.review_reask(pass, 0),
                },
            },
            &mut account,
        )
    }
}

struct SpendAccount<'a> {
    spend: &'a mut Spend,
    key: TaskKey,
    charged: &'a mut Vec<crate::events::ReviewRecord>,
}

impl crate::engine::topology::attempt::ReviewAccount for SpendAccount<'_> {
    fn charge(&mut self, review: &crate::events::ReviewRecord) {
        self.spend.record_review_cost(self.key, review.cost_usd);
        self.charged.push(review.clone());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LoopBranch {
    IngestAnswers,
    Integration,
    ReadyRetry,
    ReadyDispatch,
    DeferBackoff,
    HardBlock,
    Closure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    Performed,
    RefusedByCheckpoint,
    NotYetImplemented,
    #[allow(dead_code)]
    NotThisSlice {
        slice: &'static str,
        citation: &'static str,
    },
    #[allow(dead_code)]
    PartlyImplemented {
        performs: &'static str,
        owes: &'static str,
    },
}

impl LoopBranch {
    pub const ALL: [Self; 7] = [
        Self::IngestAnswers,
        Self::Integration,
        Self::ReadyRetry,
        Self::ReadyDispatch,
        Self::DeferBackoff,
        Self::HardBlock,
        Self::Closure,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::IngestAnswers => "ingest answers",
            Self::Integration => "integration",
            Self::ReadyRetry => "ready_retry",
            Self::ReadyDispatch => "ready dispatch",
            Self::DeferBackoff => "defer backoff",
            Self::HardBlock => "hard block",
            Self::Closure => "run-end closure",
        }
    }

    #[must_use]
    pub const fn disposition(self) -> Disposition {
        match self {
            Self::Closure => Disposition::Performed,
            Self::Integration => Disposition::Performed,
            Self::DeferBackoff => Disposition::Performed,
            Self::ReadyDispatch => Disposition::Performed,
            Self::ReadyRetry => Disposition::Performed,
            Self::HardBlock => Disposition::Performed,
            Self::IngestAnswers => Disposition::Performed,
        }
    }

    #[must_use]
    pub const fn of(step: &Step) -> Option<Self> {
        match step {
            Step::Poisoned => None,
            Step::BudgetExceeded(_) => None,
            Step::Integrate { .. } => Some(Self::Integration),
            Step::Retry { .. } => Some(Self::ReadyRetry),
            Step::Dispatch { .. } | Step::RepairDispatch { .. } => Some(Self::ReadyDispatch),
            Step::Backoff => Some(Self::DeferBackoff),
            Step::HardBlock { .. } => Some(Self::HardBlock),
            Step::Closure(_) => Some(Self::Closure),
            Step::NotStarted | Step::Finished(_) => None,
        }
    }

    #[allow(dead_code)]
    pub fn owes(self, clause: &str) -> UpstrokeError {
        UpstrokeError::Refused {
            message: format!(
                "the schema-4 run loop's `{}` branch reached a case this build does not \
                 implement: {clause}. Nothing was appended for it",
                self.label()
            ),
        }
    }

    pub fn unimplemented(self) -> UpstrokeError {
        match self.disposition() {
            Disposition::PartlyImplemented { performs, owes } => UpstrokeError::Refused {
                message: format!(
                    "the schema-4 run loop's `{}` branch performed {performs}, and this build \
                     does not {owes}",
                    self.label()
                ),
            },
            Disposition::NotThisSlice { slice, citation } => UpstrokeError::Refused {
                message: format!(
                    "the schema-4 run loop's `{}` branch belongs to {slice}, not to this build: \
                     {citation}. No effect was performed and no event was appended",
                    self.label()
                ),
            },
            _ => UpstrokeError::Refused {
                message: format!(
                    "the schema-4 run loop selected its `{}` branch, which this build does not \
                     implement yet; no effect was performed and no event was appended",
                    self.label()
                ),
            },
        }
    }
}

pub struct RunSeams<'a> {
    pub manager: &'a WorkspaceManager,
    pub clock: &'a dyn TimeSource,
    pub sleeper: &'a dyn Sleeper,
    pub runner: &'a dyn crate::runner::Runner,
    pub adapters: &'a dyn crate::agent::AdapterSource,
    pub paths: &'a crate::rundir::RunPaths,
    pub plans: &'a dyn AttemptPlans,
    pub reviews: &'a dyn ReviewPasses,
    pub input_policy: &'a dyn ReviewInputPolicy,
    pub answers: &'a dyn crate::interaction::AnswerSource,
    pub ids: &'a dyn IdSource,
    pub halts_run: bool,
}

#[derive(Debug, Clone)]
struct RunAs {
    feedback: Vec<crate::events::Feedback>,
    attempt: AttemptNumber,
    rung: u32,
    resume_session: Option<SessionId>,
    announced: bool,
    materialized: Option<Materialization>,
}

#[derive(Debug, Clone)]
struct OpenAsked {
    question: Question,
    origin: QuestionOrigin,
    key: TaskKey,
    binding: Option<Vec<String>>,
}

fn chosen_index(options: &[String], text: &str) -> Option<u32> {
    options
        .iter()
        .position(|option| option == text)
        .and_then(|index| u32::try_from(index).ok())
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Brief {
    per_task: BTreeMap<TaskKey, Vec<crate::events::Feedback>>,
}

impl Brief {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn lines(&self, key: TaskKey) -> Vec<crate::events::Feedback> {
        self.per_task.get(&key).cloned().unwrap_or_default()
    }

    pub fn record(&mut self, key: TaskKey, record: &AttemptRecord) {
        let Some(failure) = record.failure.as_ref() else {
            return;
        };
        self.per_task
            .entry(key)
            .or_default()
            .push(crate::events::Feedback {
                attempt: record.attempt,
                tier: record.tier.clone(),
                summary: failure.reason.clone(),
                detail: failure.detail.clone(),
                human: false,
            });
    }

    #[must_use]
    pub fn replay(events: &[TopologyEvent]) -> Self {
        let mut brief = Self::new();
        for event in events {
            if let TopologyEventBody::AttemptFinished { data } = &event.body {
                brief.record(data.key, &data.record);
            }
        }
        brief
    }
}

#[derive(Debug, Clone)]
struct Retained {
    tree: String,
}

#[derive(Debug, Clone, Copy)]
struct Produced<'a> {
    capture: &'a Capture,
    assessed: &'a Assessment,
    judgement: &'a Judgement,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    Settled {
        key: TaskKey,
        accepted: bool,
        spent_attempt: bool,
    },
    GenerationClosed {
        key: TaskKey,
    },
    Integrated {
        key: TaskKey,
        sequence: SequenceId,
        merged_sha: CommitSha,
    },
    Rejected {
        key: TaskKey,
        sequence: SequenceId,
    },
    Unavailable {
        key: TaskKey,
        sequence: SequenceId,
        parked: bool,
    },
    Answered {
        key: TaskKey,
        question: QuestionId,
        declined: bool,
    },
    Finished {
        outcome: RunOutcome,
        closed: usize,
        report_written: bool,
        execution_root_removed: bool,
    },
    Waited {
        waited_ms: u64,
        round: u32,
    },
    BudgetExceeded,
}

pub struct TopologyRun {
    handle: RunHandle,
    identity: RunIdentity,
    reservations: Reservations,
    invocations: InvocationLedger,
    warnings: Vec<String>,
    ceiling: Ceiling,
    spend: Spend,
    deferral: Deferral,
    slots: SlotAssertion,
    retained: BTreeMap<TaskKey, Retained>,
    brief: Brief,
}

impl TopologyRun {
    #[must_use]
    pub fn resumed(handle: RunHandle, inputs: FrozenInputs, ceiling: Ceiling) -> Self {
        let identity = RunIdentity {
            run_id: handle.started.run_id.clone(),
            inputs,
            committed_first_line_sha256: Some(handle.committed_first_line_sha256.clone()),
        };
        let spend = Spend::replay(&handle.events);
        let brief = Brief::replay(&handle.events);
        Self {
            handle,
            identity,
            reservations: Reservations::new(),
            invocations: InvocationLedger::new(),
            warnings: Vec::new(),
            ceiling,
            spend,
            deferral: Deferral::default_backoff(),
            slots: SlotAssertion::new(),
            retained: BTreeMap::new(),
            brief,
        }
    }

    pub fn commitment_digest(&self) -> Option<&str> {
        self.identity.committed_first_line_sha256.as_deref()
    }

    #[must_use]
    pub fn holds_entitlement(&mut self) -> bool {
        self.reservations.cancel_any()
    }

    pub fn invocations_balance(&self) -> bool {
        self.invocations.balances()
    }

    pub const fn spend(&self) -> &Spend {
        &self.spend
    }

    #[must_use]
    pub fn fold(&self) -> &TopologyFold {
        &self.handle.fold
    }

    #[must_use]
    pub fn events(&self) -> &[TopologyEvent] {
        &self.handle.events
    }

    #[must_use]
    #[allow(dead_code)]
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    #[must_use]
    pub fn entitlements_held(&self) -> u32 {
        self.reservations.entitlements_held()
    }

    #[must_use]
    pub const fn reservations_cancelled(&self) -> u32 {
        self.reservations.cancelled()
    }

    #[must_use]
    #[allow(dead_code)]
    pub fn defer_round(&self) -> u32 {
        self.deferral.round()
    }

    pub fn step(
        &mut self,
        seams: &RunSeams<'_>,
        hooks: &mut dyn TopologyHooks,
    ) -> Result<Progress, UpstrokeError> {
        let _cleanup_scope = self.handle.cleanup_scope();
        if let Some(answered) = self.ingest_answers(seams, hooks)? {
            return Ok(answered);
        }
        let selected = select(&self.handle.fold, &self.ceiling, &self.spend);

        let admitted = checkpoint(selected)?;
        match admitted {
            Admitted::BudgetExceeded(exceeded) => {
                self.emit(
                    TopologyEventBody::BudgetExceeded { data: *exceeded },
                    seams,
                    hooks,
                )?;
                Ok(Progress::BudgetExceeded)
            }
            Admitted::Backoff => {
                let elapsed = self.deferral.wait(seams.sleeper);
                let (waited_ms, round) = (elapsed.waited_ms, elapsed.round);
                self.emit(
                    TopologyEventBody::DeferWaitElapsed { data: elapsed },
                    seams,
                    hooks,
                )?;
                Ok(Progress::Waited { waited_ms, round })
            }
            Admitted::Integrate { candidate } => self.integrate(*candidate, seams, hooks),
            Admitted::Retry {
                key, generation, ..
            } => self.retry_ready(key, generation, seams, hooks),
            Admitted::Dispatch {
                key,
                generation,
                continuing,
            }
            | Admitted::RepairDispatch {
                key,
                generation,
                continuing,
            } => {
                let dispatched = if continuing {
                    self.continue_open(key, generation, seams, hooks)?
                } else {
                    self.dispatch_ready(key, generation, seams, hooks)?
                };
                self.run_first_attempt(&dispatched, seams, hooks)
            }
            Admitted::HardBlock { questions } => self.hard_block(&questions, seams, hooks),
            Admitted::Closure(_) => self.close_run(seams, hooks),
        }
    }

    fn run_first_attempt(
        &mut self,
        dispatched: &Dispatched,
        seams: &RunSeams<'_>,
        hooks: &mut dyn TopologyHooks,
    ) -> Result<Progress, UpstrokeError> {
        let key = dispatched.key;
        let (plan, capture, assessed, judgement) = self.attempt(
            dispatched.site(),
            RunAs {
                attempt: Self::FIRST_ATTEMPT,
                rung: self.ladder_position(key)?.0,
                resume_session: None,
                feedback: self.brief.lines(key),
                announced: false,
                materialized: dispatched.materialized,
            },
            seams,
            hooks,
        )?;
        let accepted = judgement.accepted();
        let spent_attempt = self.settle(
            dispatched.site(),
            &plan,
            Produced {
                capture: &capture,
                assessed: &assessed,
                judgement: &judgement,
            },
            seams,
            hooks,
        )?;
        Ok(Progress::Settled {
            key,
            accepted,
            spent_attempt,
        })
    }

    fn ingest_answers(
        &mut self,
        seams: &RunSeams<'_>,
        hooks: &mut dyn TopologyHooks,
    ) -> Result<Option<Progress>, UpstrokeError> {
        if self.handle.fold.is_poisoned() || self.handle.fold.run_is_ending() {
            return Ok(None);
        }
        let open: Vec<QuestionId> = self
            .handle
            .fold
            .open_questions()
            .map(|questions| questions.keys().cloned().collect())
            .unwrap_or_default();
        for id in &open {
            let asked = self.open_question(id)?;
            let answer = match seams.answers.poll(&asked.question)? {
                Answer::Unanswered => continue,
                answer => answer,
            };
            let answer4 = self.answer_for(id, &asked, answer, seams)?;
            return self
                .ingest_answer(id, asked.key, answer4, seams, hooks)
                .map(Some);
        }
        Ok(None)
    }

    fn dispatch_ready(
        &mut self,
        key: TaskKey,
        generation: GenerationId,
        seams: &RunSeams<'_>,
        hooks: &mut dyn TopologyHooks,
    ) -> Result<Dispatched, UpstrokeError> {
        let request = self.dispatch_request(key, generation, seams)?;

        self.reservations.take(key, ReservationKind::Dispatch)?;

        let dispatched = {
            let mut emitter = RunEmitter {
                identity: &self.identity,
                state: EmitState {
                    fold: &mut self.handle.fold,
                    log: &mut self.handle.log,
                    events: &mut self.handle.events,
                    reservations: &mut self.reservations,
                    warnings: &mut self.warnings,
                },
                clock: seams.clock,
            };
            dispatch(seams.manager, hooks, &mut emitter, &request)
        };

        match dispatched {
            Ok(dispatched) => {
                self.reservations.convert(key, ReservationKind::Dispatch)?;
                self.deferral.progressed();
                Ok(dispatched)
            }
            Err(error) => {
                let _ = self.reservations.cancel(key, ReservationKind::Dispatch);
                Err(error.discharging(&mut self.invocations))
            }
        }
    }

    fn integrate(
        &mut self,
        candidate: CandidateRef,
        seams: &RunSeams<'_>,
        hooks: &mut dyn TopologyHooks,
    ) -> Result<Progress, UpstrokeError> {
        let request =
            IntegrationRequest::from_log(&self.handle.fold, &self.handle.events, &candidate)?;
        let key = candidate.key;
        self.reservations.take(key, ReservationKind::Integration)?;

        let terminal = {
            let mut cx = IntegrationCx {
                emitter: RunEmitter {
                    identity: &self.identity,
                    state: EmitState {
                        fold: &mut self.handle.fold,
                        log: &mut self.handle.log,
                        events: &mut self.handle.events,
                        reservations: &mut self.reservations,
                        warnings: &mut self.warnings,
                    },
                    clock: seams.clock,
                },
                hooks,
                invocations: &mut self.invocations,
                slots: &mut self.slots,
                spend: &mut self.spend,
                seams,
            };
            integrate::integrate(&mut cx, seams.manager, &request)
        };

        match terminal {
            Ok(terminal) => {
                self.deferral.progressed();
                Ok(match terminal {
                    Terminal::Merged(published) => Progress::Integrated {
                        key: published.key,
                        sequence: published.sequence,
                        merged_sha: published.merged_sha,
                    },
                    Terminal::Rejected { sequence, key } => Progress::Rejected { key, sequence },
                    Terminal::Unavailable {
                        sequence,
                        key,
                        parked,
                    } => Progress::Unavailable {
                        key,
                        sequence,
                        parked,
                    },
                })
            }
            Err(error) => {
                if !self.reservations.is_empty() {
                    self.reservations
                        .cancel(key, ReservationKind::Integration)?;
                }
                Err(error)
            }
        }
    }

    fn hard_block(
        &mut self,
        questions: &[QuestionId],
        seams: &RunSeams<'_>,
        hooks: &mut dyn TopologyHooks,
    ) -> Result<Progress, UpstrokeError> {
        for id in questions {
            let asked = self.open_question(id)?;
            let answer = match seams.answers.resolve(&asked.question)? {
                Answer::Unanswered => continue,
                answer => answer,
            };
            let answer4 = self.answer_for(id, &asked, answer, seams)?;
            return self.ingest_answer(id, asked.key, answer4, seams, hooks);
        }
        self.close_run(seams, hooks)
    }

    fn answer_for(
        &self,
        id: &QuestionId,
        asked: &OpenAsked,
        answer: Answer,
        seams: &RunSeams<'_>,
    ) -> Result<Answer4, UpstrokeError> {
        let text = match answer {
            Answer::Declined => {
                return Ok(Answer4::Declined {
                    decline_halts_run: seams.halts_run,
                });
            }
            Answer::Answered { text } => text,
            Answer::Unanswered => {
                return Err(UpstrokeError::Refused {
                    message: format!("question {} resolved to no answer to ingest", id.0),
                });
            }
        };
        let (Some(authorized), QuestionOrigin::Admission) = (&asked.binding, asked.origin) else {
            return Ok(Answer4::Answered {
                option_index: chosen_index(&asked.question.options, &text).unwrap_or(0),
                binding_override: None,
            });
        };
        let option_index =
            chosen_index(&asked.question.options, &text).ok_or_else(|| UpstrokeError::Refused {
                message: format!(
                    "question {} asks which agent runs task {}, and `{text}` is none of the {} \
                     it offers; a one-off binding activates only through an option the spawn \
                     froze, so nothing was appended",
                    id.0,
                    asked.key.index(),
                    asked.question.options.len()
                ),
            })?;
        let agent = authorized
            .get(usize::try_from(option_index).unwrap_or(usize::MAX))
            .ok_or_else(|| UpstrokeError::Refused {
                message: format!(
                    "question {} offers {} option(s) and authorizes {} binding(s); option \
                     {option_index} exists in one and not the other, so nothing was appended",
                    id.0,
                    asked.question.options.len(),
                    authorized.len()
                ),
            })?;
        let entry = self
            .handle
            .fold
            .registry()
            .and_then(|registry| registry.get(asked.key))
            .ok_or_else(|| UpstrokeError::Refused {
                message: format!(
                    "task {} is not in this run's frozen registry",
                    asked.key.index()
                ),
            })?;
        Ok(Answer4::Answered {
            option_index,
            binding_override: Some(super::repair::one_off_binding(
                entry,
                asked.key,
                id,
                option_index,
                agent,
            )?),
        })
    }

    fn ingest_answer(
        &mut self,
        id: &QuestionId,
        key: TaskKey,
        answer4: Answer4,
        seams: &RunSeams<'_>,
        hooks: &mut dyn TopologyHooks,
    ) -> Result<Progress, UpstrokeError> {
        let declined = matches!(answer4, Answer4::Declined { .. });
        self.emit(
            TopologyEventBody::QuestionAnswered {
                data: crate::topology::events::QuestionAnswered4 {
                    key,
                    question: id.clone(),
                    answer: answer4,
                    via: seams.answers.id().to_owned(),
                },
            },
            seams,
            hooks,
        )?;
        Ok(Progress::Answered {
            key,
            question: id.clone(),
            declined,
        })
    }

    fn open_question(&self, id: &QuestionId) -> Result<OpenAsked, UpstrokeError> {
        let open = self
            .handle
            .fold
            .open_questions()
            .and_then(|open| open.get(id))
            .ok_or_else(|| UpstrokeError::Refused {
                message: format!("question {} is not open in this run's fold", id.0),
            })?;
        let frozen = &open.question;
        Ok(OpenAsked {
            question: Question {
                id: frozen.id.clone(),
                kind: frozen.kind,
                affected_tasks: vec![crate::ir::TaskId(self.display_id(frozen.key)?)],
                context: frozen.context.clone(),
                options: frozen.options.clone(),
            },
            origin: open.origin,
            key: frozen.key,
            binding: open.binding.clone(),
        })
    }

    fn continue_open(
        &mut self,
        key: TaskKey,
        generation: GenerationId,
        seams: &RunSeams<'_>,
        hooks: &mut dyn TopologyHooks,
    ) -> Result<Dispatched, UpstrokeError> {
        let base = self
            .handle
            .fold
            .task(key)
            .and_then(|task| task.generations.iter().find(|held| held.id == generation))
            .map(|held| held.base_sha.clone())
            .ok_or_else(|| UpstrokeError::Refused {
                message: format!(
                    "task {} has no generation {} to continue",
                    key.index(),
                    generation.0
                ),
            })?;
        let kind = match self.dispatch_kind(key)? {
            DispatchKind::Ordinary { paths } => DispatchKind::Ordinary { paths },
            DispatchKind::Repair { root, .. } => DispatchKind::Repair {
                root,
                source: dispatched_source(&self.handle.events, key, generation)?,
            },
        };
        let slot = task_slot(key, generation);
        let open = OpenGeneration {
            key,
            generation,
            base: base.clone(),
            slot: slot.clone(),
            source: match &kind {
                DispatchKind::Ordinary { .. } => None,
                DispatchKind::Repair { source, .. } => Some(source.clone()),
            },
        };
        let resumed = resume_open_no_attempt(seams.manager, hooks, &open)?;
        self.deferral.progressed();
        Ok(Dispatched {
            key,
            generation,
            base,
            worktree: seams.manager.slot_path(&slot),
            slot,
            kind,
            materialized: resumed.materialized,
        })
    }

    fn retry_ready(
        &mut self,
        key: TaskKey,
        generation: GenerationId,
        seams: &RunSeams<'_>,
        hooks: &mut dyn TopologyHooks,
    ) -> Result<Progress, UpstrokeError> {
        let position = self.ladder_position(key)?;
        let held = self
            .retained
            .get(&key)
            .cloned()
            .ok_or_else(|| UpstrokeError::Refused {
                message: format!(
                    "task {} has a retained generation this process did not retain, so the tree \
                     its retry must re-gate is not known here. A fresh process closes a retained \
                     generation in recovery rather than continuing it",
                    key.index()
                ),
            })?;
        let binding = self
            .handle
            .fold
            .rung_binding(key, position.0)
            .ok_or_else(|| UpstrokeError::Refused {
                message: format!(
                    "task {} has neither a validated one-off binding nor a rung {} in its \
                     frozen ladder",
                    key.index(),
                    position.0
                ),
            })?;
        let slot_for_run = task_slot(key, generation);
        let slot = slot_for_run.clone();
        let pool = seams.plans.pool_for(&binding.agent);
        let materialization = self.retry_materialization(key);

        let outcome = {
            let worktrees = ManagedWorktrees::new(seams.manager);
            retry(
                &self.handle.fold,
                &mut self.reservations,
                &worktrees,
                hooks.effects(),
                &RetryRequest {
                    key,
                    slot,
                    retained_tree: held.tree.clone(),
                    binding,
                    rung: position.0,
                    pool: pool.clone(),
                    materialization,
                },
            )?
        };

        match outcome {
            RetryOutcome::Start(started) => {
                let run_as = RunAs {
                    attempt: started.attempt,
                    rung: started.rung,
                    resume_session: started.resume_session.clone(),
                    feedback: self.brief.lines(key),
                    announced: true,
                    materialized: started.materialization_observed,
                };
                self.emit(
                    TopologyEventBody::AttemptStarted { data: *started },
                    seams,
                    hooks,
                )?;
                self.reservations.convert(key, ReservationKind::Retry)?;
                self.deferral.progressed();

                let base = self
                    .handle
                    .fold
                    .task(key)
                    .and_then(|task| task.generations.iter().find(|held| held.id == generation))
                    .map(|held| held.base_sha.clone())
                    .ok_or_else(|| UpstrokeError::Refused {
                        message: format!(
                            "generation {} of task {} left the fold mid-retry",
                            generation.0,
                            key.index()
                        ),
                    })?;
                let worktree = seams.manager.slot_path(&slot_for_run);
                let site = AttemptSite {
                    key,
                    generation,
                    base: &base,
                    slot: &slot_for_run,
                    worktree: &worktree,
                };
                let (plan, capture, assessed, judgement) =
                    self.attempt(site, run_as, seams, hooks)?;
                let accepted = judgement.accepted();
                let spent_attempt = self.settle(
                    site,
                    &plan,
                    Produced {
                        capture: &capture,
                        assessed: &assessed,
                        judgement: &judgement,
                    },
                    seams,
                    hooks,
                )?;
                return Ok(Progress::Settled {
                    key,
                    accepted,
                    spent_attempt,
                });
            }
            RetryOutcome::Close { closed, .. } => {
                self.emit(
                    TopologyEventBody::GenerationClosed { data: closed },
                    seams,
                    hooks,
                )?;
                self.retained.remove(&key);
                super::dispatch::scrub(seams.manager, hooks, &slot_for_run)?;
            }
        }
        Ok(Progress::GenerationClosed { key })
    }

    fn close_run(
        &mut self,
        seams: &RunSeams<'_>,
        hooks: &mut dyn TopologyHooks,
    ) -> Result<Progress, UpstrokeError> {
        let outcome = closure::ending_outcome(&self.handle.fold)?;
        closure::refuse_unclosable(&self.handle.fold)?;

        let reason = GenerationCloseReason::RunEnding {
            outcome: outcome.clone(),
        };
        let mut closed = 0;
        for key in closure::closable(&self.handle.fold) {
            let event = close_generation(&self.handle.fold, key, reason.clone())?;
            let slot = task_slot(key, event.generation);
            self.emit(
                TopologyEventBody::GenerationClosed { data: event },
                seams,
                hooks,
            )?;
            self.retained.remove(&key);
            super::dispatch::scrub(seams.manager, hooks, &slot)?;
            closed += 1;
        }

        if self.reservations.cancel_any() {
            self.warnings.push(
                "run-end closure found a provisional reservation still held and cancelled it"
                    .to_owned(),
            );
        }

        closure::confirm_derived(&self.handle.fold, &outcome)?;
        let finished = closure::run_finished(&self.handle.fold, outcome.clone());
        self.emit(
            TopologyEventBody::RunFinished { data: finished },
            seams,
            hooks,
        )?;

        let finalized = finalize::finalize(
            &finalize::Finalize {
                manager: seams.manager,
                public: &seams.paths.public,
                run_id: &self.identity.run_id,
                fold: &self.handle.fold,
                events: &self.handle.events,
            },
            hooks,
        )?;
        Ok(Progress::Finished {
            outcome,
            closed,
            report_written: finalized.report_written,
            execution_root_removed: finalized.execution_root_removed,
        })
    }

    fn retry_materialization(&self, key: TaskKey) -> Option<Materialization> {
        self.handle
            .fold
            .registry()
            .and_then(|registry| registry.get(key))
            .and_then(|entry| entry.lineage)
            .map(|_| Materialization::Retained)
    }

    const FIRST_ATTEMPT: crate::topology::events::AttemptNumber =
        crate::topology::events::AttemptNumber(1);

    fn attempt(
        &mut self,
        site: AttemptSite<'_>,
        run_as: RunAs,
        seams: &RunSeams<'_>,
        hooks: &mut dyn TopologyHooks,
    ) -> Result<(AttemptPlan, Capture, Assessment, Judgement), UpstrokeError> {
        let key = site.key;
        let binding = self
            .handle
            .fold
            .rung_binding(key, run_as.rung)
            .ok_or_else(|| UpstrokeError::Refused {
                message: format!(
                    "task {} has neither a validated one-off binding nor a rung {} in its \
                     frozen ladder, so there is no binding to run it under",
                    key.index(),
                    run_as.rung
                ),
            })?;

        let entry = self
            .handle
            .fold
            .registry()
            .and_then(|registry| registry.get(key))
            .ok_or_else(|| UpstrokeError::Refused {
                message: format!("task {} is not in this run's frozen registry", key.index()),
            })?
            .clone();

        let plan = seams.plans.plan(&PlanRequest {
            key,
            entry: &entry,
            attempt: run_as.attempt,
            rung: run_as.rung,
            binding,
            workspace: site.worktree,
            resume_session: run_as.resume_session.clone(),
            feedback: run_as.feedback.clone(),
            materialization_observed: run_as.materialized,
        })?;

        let mut emitter = RunEmitter {
            identity: &self.identity,
            state: EmitState {
                fold: &mut self.handle.fold,
                log: &mut self.handle.log,
                events: &mut self.handle.events,
                reservations: &mut self.reservations,
                warnings: &mut self.warnings,
            },
            clock: seams.clock,
        };
        let mut cx = AttemptContext {
            manager: seams.manager,
            hooks,
            emitter: &mut emitter,
            runner: seams.runner,
            slots: &mut self.slots,
            ledger: &mut self.invocations,
            adapters: seams.adapters,
            paths: seams.paths,
            reviews: seams.reviews,
            input_policy: seams.input_policy,
        };

        let run = if run_as.announced {
            cx.run_worker(site, &plan)?
        } else {
            cx.start(site, &plan)?
        };
        let capture = cx.capture(site)?;
        let diff = seams
            .manager
            .candidate_diff(site.slot, &capture.parent, &capture.tree)?;

        let assessed = cx.assess(site, &plan, &run, &capture, &diff, entry.spec.kind)?;

        let inputs = seams.plans.inputs(&InputsRequest {
            entry: &entry,
            diff,
        })?;

        let judgement = cx.judge(
            site,
            &plan,
            Judging {
                run: &run,
                capture: &capture,
                assessed: &assessed,
            },
            &inputs,
            &|pass| review::ReviewInvocations {
                pass: run.identities.review_pass(pass, 0),
                reask: run.identities.review_reask(pass, 0),
            },
        )?;
        Ok((plan, capture, assessed, judgement))
    }

    fn settle(
        &mut self,
        site: AttemptSite<'_>,
        plan: &AttemptPlan,
        produced: Produced<'_>,
        seams: &RunSeams<'_>,
        hooks: &mut dyn TopologyHooks,
    ) -> Result<bool, UpstrokeError> {
        let Produced {
            capture,
            assessed,
            judgement,
        } = produced;
        let record = crate::engine::classify::attempt_record(
            plan.attempt.0,
            crate::engine::classify::AttemptFacts {
                tier: plan.binding.tier,
                model: &plan.binding.model,
                pool: plan.pool.clone(),
                resumed: plan.resume_session.is_some(),
                outcome: &assessed.outcome,
                reviews: &judgement.reviews,
                failure: judgement.failure.as_ref(),
                feedback: crate::engine::classify::FeedbackCarrier::AttemptRecord,
            },
        );

        let Some(failure) = judgement.failure.as_ref() else {
            self.promote_candidate(site, plan, capture, record, seams, hooks)?;
            return Ok(crate::ladder::spends_allowance(None));
        };

        let policy = self.ladder_policy(site.key)?;

        let defers = self.deferrals_recorded(site.key)?;
        let position = self.ladder_position(site.key)?;

        let attempts_on_rung =
            position
                .1
                .saturating_add(u32::from(crate::ladder::spends_allowance(Some(
                    crate::ladder::FailureShape::of(failure),
                ))));

        let next = crate::ladder::next_step(
            failure,
            &crate::ladder::LadderState {
                rung: position.0 as usize,
                attempts_on_rung,
                defers,
                resumable: plan.session_resume
                    && assessed.outcome.session_id.is_some()
                    && capture.unresolved.is_empty(),
            },
            &policy,
        );
        let question = match next {
            crate::ladder::Next::AskHuman(kind) => {
                Some(self.park_question(site.key, attempts_on_rung, kind, failure, seams.ids)?)
            }
            _ => None,
        };

        let settled = settle_failed(
            &self.handle.fold,
            &FinishedAttempt {
                key: site.key,
                generation: site.generation,
                attempt: plan.attempt,
                record,
                next,
                session: assessed.outcome.session_id.clone().map(SessionId),
                question,
                halts_run: seams.halts_run,
                defers,
                reason: failure.reason.clone(),
                rung: position.0.saturating_add(1),
            },
        )?;

        if matches!(
            settled.event.settlement,
            crate::topology::events::AttemptSettlement::Retained { .. }
        ) {
            self.retained.insert(
                site.key,
                Retained {
                    tree: capture.tree.clone(),
                },
            );
        }
        self.spend.record(site.key, &settled.event.record);
        self.brief.record(site.key, &settled.event.record);
        let closed = matches!(
            settled.event.settlement,
            crate::topology::events::AttemptSettlement::Closed { .. }
        );
        self.emit(
            TopologyEventBody::AttemptFinished {
                data: Box::new(settled.event),
            },
            seams,
            hooks,
        )?;
        if closed {
            super::dispatch::scrub(seams.manager, hooks, site.slot)?;
        }
        Ok(settled.spent_attempt)
    }

    fn promote_candidate(
        &mut self,
        site: AttemptSite<'_>,
        plan: &AttemptPlan,
        capture: &Capture,
        record: AttemptRecord,
        seams: &RunSeams<'_>,
        hooks: &mut dyn TopologyHooks,
    ) -> Result<(), UpstrokeError> {
        let key = site.key;
        let actual_paths = seams.manager.changed_paths(site.slot, &capture.parent)?;

        let judged = JudgedTree {
            key,
            generation: site.generation,
            attempt: Box::new(record.clone()),
            base_sha: site.base.clone(),
            tree_sha: CommitSha(capture.tree.clone()),
            message: format!(
                "upstroke: {} attempt {}",
                self.display_id(key)?,
                plan.attempt.0
            ),
            actual_paths: actual_paths.clone(),
            lease_effect: match self.lineage_root(key) {
                Some(root) => CandidateLeaseEffect::WidensLineage {
                    root,
                    paths: actual_paths,
                },
                None => CandidateLeaseEffect::ReplacesPredicted {
                    paths: actual_paths,
                },
            },
        };

        let run_id = self.identity.run_id.clone();
        let unpinned = write_candidate_commit(seams.manager, hooks, &run_id, judged)?;
        let pinned = pin_candidate(seams.manager, hooks, unpinned)?;

        self.spend.record(key, &record);
        self.brief.record(key, &record);

        let promoting = self.with_journal(seams, hooks, |journal| {
            append_candidate_prepared(journal, pinned)
        })?;
        let referenced = create_candidates_ref(seams.manager, hooks, promoting)?;
        let created = self.with_journal(seams, hooks, |journal| {
            append_candidate_created(journal, referenced)
        })?;
        reclaim_after_creation(seams.manager, hooks, site.slot, created)?;
        Ok(())
    }

    fn with_journal<T>(
        &mut self,
        seams: &RunSeams<'_>,
        hooks: &mut dyn TopologyHooks,
        run: impl FnOnce(&mut RunJournal<'_, '_>) -> Result<T, UpstrokeError>,
    ) -> Result<T, UpstrokeError> {
        let mut journal = RunJournal {
            emitter: RunEmitter {
                identity: &self.identity,
                state: EmitState {
                    fold: &mut self.handle.fold,
                    log: &mut self.handle.log,
                    events: &mut self.handle.events,
                    reservations: &mut self.reservations,
                    warnings: &mut self.warnings,
                },
                clock: seams.clock,
            },
            hooks,
            invocations: &mut self.invocations,
        };
        run(&mut journal)
    }

    fn display_id(&self, key: TaskKey) -> Result<String, UpstrokeError> {
        Ok(self
            .handle
            .fold
            .registry()
            .and_then(|registry| registry.get(key))
            .ok_or_else(|| UpstrokeError::Refused {
                message: format!("task {} is not in this run's frozen registry", key.index()),
            })?
            .display_id
            .as_str()
            .to_owned())
    }

    fn park_question(
        &self,
        key: TaskKey,
        attempts_on_rung: u32,
        kind: crate::ir::QuestionKind,
        failure: &crate::ladder::AttemptFailure,
        ids: &dyn IdSource,
    ) -> Result<FrozenQuestion, UpstrokeError> {
        let entry = self
            .handle
            .fold
            .registry()
            .and_then(|registry| registry.get(key))
            .ok_or_else(|| UpstrokeError::Refused {
                message: format!("task {} is not in this run's frozen registry", key.index()),
            })?;
        let task = self
            .handle
            .fold
            .task(key)
            .ok_or_else(|| UpstrokeError::Refused {
                message: format!("task {} is not in this run's fold", key.index()),
            })?;
        let total_attempts: u32 = task
            .generations
            .iter()
            .map(|generation| generation.attempts)
            .sum();
        let rungs_spent = (task.rung as usize).saturating_add(1).max(1);
        let _ = attempts_on_rung;
        Ok(FrozenQuestion {
            id: ids.question_id(),
            key,
            kind,
            context: crate::engine::coordinator::question_context(
                crate::engine::coordinator::ParkSubject {
                    display_id: entry.display_id.as_str(),
                    title: &entry.spec.title,
                    acceptance: &entry.spec.acceptance,
                    attempts: total_attempts,
                    rungs_spent,
                },
                kind,
                failure,
            ),
            options: crate::engine::coordinator::topology_question_options(kind),
        })
    }

    fn ladder_position(&self, key: TaskKey) -> Result<(u32, u32), UpstrokeError> {
        let task = self
            .handle
            .fold
            .task(key)
            .ok_or_else(|| UpstrokeError::Refused {
                message: format!("task {} is not in this run's fold", key.index()),
            })?;
        Ok((task.rung, task.attempts_on_rung))
    }

    fn deferrals_recorded(&self, key: TaskKey) -> Result<u32, UpstrokeError> {
        Ok(self
            .handle
            .fold
            .task(key)
            .ok_or_else(|| UpstrokeError::Refused {
                message: format!("task {} is not in this run's fold", key.index()),
            })?
            .defers)
    }

    fn ladder_policy(&self, key: TaskKey) -> Result<crate::ladder::LadderPolicy, UpstrokeError> {
        let entry = self
            .handle
            .fold
            .registry()
            .and_then(|registry| registry.get(key))
            .ok_or_else(|| UpstrokeError::Refused {
                message: format!("task {} is not in this run's frozen registry", key.index()),
            })?;
        let limits = self
            .handle
            .fold
            .started()
            .ok_or_else(|| UpstrokeError::Refused {
                message: "the run has not started".to_owned(),
            })?
            .limits;
        Ok(crate::ladder::LadderPolicy {
            attempts_per: entry.ladder.attempts_per,
            rungs: if self.handle.fold.binding_override(key).is_some() {
                1
            } else {
                entry.ladder.rungs.len()
            },
            max_defers: limits.max_defers,
        })
    }

    fn lineage_root(&self, key: TaskKey) -> Option<TaskKey> {
        self.handle
            .fold
            .registry()
            .and_then(|registry| registry.get(key))
            .and_then(|entry| entry.lineage)
            .map(|lineage| lineage.root)
    }

    fn dispatch_request(
        &self,
        key: TaskKey,
        generation: GenerationId,
        seams: &RunSeams<'_>,
    ) -> Result<DispatchRequest, UpstrokeError> {
        let kind = self.dispatch_kind(key)?;
        let base = integrate::dispatch_head(
            seams.manager,
            &self.handle.started,
            &self.handle.events,
            key,
        )?;
        Ok(DispatchRequest {
            key,
            generation,
            base,
            kind,
        })
    }

    fn dispatch_kind(&self, key: TaskKey) -> Result<DispatchKind, UpstrokeError> {
        let entry = self
            .handle
            .fold
            .registry()
            .and_then(|registry| registry.get(key))
            .ok_or_else(|| UpstrokeError::Refused {
                message: format!(
                    "the fold selected task {} for dispatch and the frozen registry has no such \
                     entry; the two disagree and nothing is dispatched",
                    key.0
                ),
            })?;
        let Some(lineage) = entry.lineage else {
            let paths =
                self.handle
                    .fold
                    .predicted_region(key)
                    .ok_or_else(|| UpstrokeError::Refused {
                        message: format!("task {} has no predicted region", key.index()),
                    })?;
            return Ok(DispatchKind::Ordinary { paths });
        };
        Ok(DispatchKind::Repair {
            root: lineage.root,
            source: rejected_source(&self.handle.events, key)?,
        })
    }

    fn emit(
        &mut self,
        body: TopologyEventBody,
        seams: &RunSeams<'_>,
        hooks: &mut dyn TopologyHooks,
    ) -> Result<(), UpstrokeError> {
        let mut emitter = RunEmitter {
            identity: &self.identity,
            state: EmitState {
                fold: &mut self.handle.fold,
                log: &mut self.handle.log,
                events: &mut self.handle.events,
                reservations: &mut self.reservations,
                warnings: &mut self.warnings,
            },
            clock: seams.clock,
        };
        emitter
            .emit(body, hooks)
            .map_err(|failure| failure.discharging(&mut self.invocations))
    }
}

fn rejected_source(events: &[TopologyEvent], key: TaskKey) -> Result<CandidateRef, UpstrokeError> {
    events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            TopologyEventBody::MergeRejected { data } if data.repair.key == key => {
                Some(data.candidate.clone())
            }
            _ => None,
        })
        .ok_or_else(|| UpstrokeError::Refused {
            message: format!(
                "task {} is a repair and no `merge_rejected` in this log registered it, so the \
                 candidate it is materialized from is not known; nothing is dispatched",
                key.index()
            ),
        })
}

pub(super) fn dispatched_source(
    events: &[TopologyEvent],
    key: TaskKey,
    generation: GenerationId,
) -> Result<CandidateRef, UpstrokeError> {
    events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            TopologyEventBody::TaskDispatched { data }
                if data.key == key && data.generation == generation =>
            {
                Some(data.source_candidate.clone())
            }
            _ => None,
        })
        .ok_or_else(|| UpstrokeError::Refused {
            message: format!(
                "task {} generation {} has no `task_dispatched` in this log, so there is no \
                 record of what it was materialized from; nothing is continued",
                key.index(),
                generation.0
            ),
        })?
        .ok_or_else(|| UpstrokeError::Refused {
            message: format!(
                "task {} generation {} is a repair whose `task_dispatched` names no source \
                 candidate; the log disagrees with the registry and nothing is continued",
                key.index(),
                generation.0
            ),
        })
}

#[cfg(test)]
mod tests;
