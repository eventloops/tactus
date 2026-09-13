//! Extended notes: `docs/internals/engine/topology/report.md`

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::UpstrokeError;
use crate::events::{ReviewRecord, RunOutcome};
use crate::topology::events::{
    BindingOverride, BudgetStop, CandidateRef, FrozenQuestion, PreparedDisposition,
    RejectionDisposition, RunnerPolicy, TopologyEvent, TopologyEventBody, UnavailableCause,
    UnavailableOutcome, VerificationBasis, VerificationRecord,
};
use crate::topology::fold::{
    GenerationClass, QuestionOrigin, TaskState, TopologyFold, TransactionClass,
};
use crate::topology::leases::LeaseOwner;
use crate::topology::paths::PathSet;
use crate::topology::schema::TOPOLOGY_SCHEMA;
use crate::util::terminal::TerminalLines;

use super::candidate::candidates_ref;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TopologyReport {
    pub schema: u32,
    pub run_id: String,
    pub outcome: Option<RunOutcome>,
    pub halted_at: Option<u32>,
    pub epoch: u32,
    pub incarnation: String,
    pub runner: RunnerPolicy,
    pub budget_stop: Option<BudgetStop>,
    pub tasks: Vec<TaskProjection>,
    pub queue: Vec<QueueProjection>,
    pub lineages: Vec<LineageProjection>,
    pub open_questions: Vec<QuestionProjection>,
    pub transaction: Option<TransactionProjection>,
    pub retained_candidates: Vec<RetainedCandidate>,
    pub integration_ledger: Vec<LedgerRow>,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskProjection {
    pub key: u32,
    pub display_id: String,
    pub origin: String,
    pub lineage: Option<TaskLineage>,
    pub lineage_root: Option<u32>,
    pub state: String,
    pub rung: u32,
    pub attempts_on_rung: u32,
    pub defers: u32,
    pub binding_override: Option<BindingOverride>,
    pub generations: Vec<GenerationProjection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskLineage {
    pub root: u32,
    pub parent: u32,
    pub index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationProjection {
    pub id: u32,
    pub class: String,
    pub retained_session: Option<String>,
    pub retained_incarnation: Option<u32>,
    pub base_sha: String,
    pub attempts: u32,
    pub candidate: Option<CandidateRef>,
    pub holds_generation_lease: bool,
    pub holds_candidate_lease: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueueProjection {
    pub candidate: CandidateRef,
    pub paths: PathSet,
    pub lineage_root: Option<u32>,
    pub verification_deferred: bool,
    pub defers: u32,
    pub sequence: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineageProjection {
    pub root: u32,
    pub paths: PathSet,
    pub age: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionProjection {
    pub question: FrozenQuestion,
    pub origin: String,
    pub binding: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransactionProjection {
    pub sequence: u32,
    pub candidate: CandidateRef,
    pub class: String,
    pub expected_head: String,
    pub proposed_sha: String,
    pub disposition: Option<PreparedDisposition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetainedCandidate {
    pub key: u32,
    pub generation: u32,
    pub candidates_ref: String,
    pub commit_sha: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LedgerRow {
    pub sequence: u32,
    pub candidate: CandidateRef,
    pub basis: String,
    pub expected_head: Option<String>,
    pub proposed_sha: Option<String>,
    pub terminal: String,
    pub detail: String,
    pub verification: Option<VerificationRecord>,
    pub reviews: u32,
    pub review_cost_usd: Option<f64>,
    pub review_cost_incomplete: bool,
}

impl TopologyReport {
    pub fn derive(
        run_id: &str,
        fold: &TopologyFold,
        events: &[TopologyEvent],
    ) -> Result<Self, UpstrokeError> {
        let started = fold.started().ok_or_else(|| UpstrokeError::Refused {
            message: format!("run `{run_id}` has not started, so there is nothing to report"),
        })?;
        if started.run_id != run_id {
            return Err(UpstrokeError::Refused {
                message: format!(
                    "the fold belongs to run `{}` and the report was asked for `{run_id}`",
                    started.run_id
                ),
            });
        }
        let registry = fold.registry().ok_or_else(|| UpstrokeError::Refused {
            message: format!("run `{run_id}` has no frozen registry"),
        })?;
        let outcome = fold.finished().cloned();
        let leases = fold.leases();

        let mut tasks = Vec::with_capacity(registry.len());
        let mut retained_candidates = Vec::new();
        for entry in registry.entries() {
            let key = entry.key;
            let Some(task) = fold.task(key) else {
                continue;
            };
            let generations = task
                .generations
                .iter()
                .map(|generation| {
                    let (class, retained_session, retained_incarnation) = match &generation.class {
                        GenerationClass::OpenNoAttempt => ("open_no_attempt", None, None),
                        GenerationClass::InFlight { .. } => ("in_flight", None, None),
                        GenerationClass::RetainedIdle {
                            session,
                            incarnation,
                        } => (
                            "retained_idle",
                            Some(session.0.clone()),
                            Some(incarnation.0),
                        ),
                        GenerationClass::Promoting => ("promoting", None, None),
                        GenerationClass::Closed => ("closed", None, None),
                    };
                    let candidate = generation
                        .candidate
                        .as_ref()
                        .map(|prepared| prepared.candidate.clone());
                    if let Some(candidate) = &candidate {
                        if outcome != Some(RunOutcome::Complete) {
                            retained_candidates.push(RetainedCandidate {
                                key: key.0,
                                generation: generation.id.0,
                                candidates_ref: candidates_ref(run_id, key, generation.id).0,
                                commit_sha: candidate.commit_sha.0.clone(),
                            });
                        }
                    }
                    GenerationProjection {
                        id: generation.id.0,
                        class: class.to_owned(),
                        retained_session,
                        retained_incarnation,
                        base_sha: generation.base_sha.0.clone(),
                        attempts: generation.attempts,
                        candidate,
                        holds_generation_lease: leases.is_some_and(|table| {
                            table.holds(LeaseOwner::Generation {
                                key,
                                generation: generation.id,
                            })
                        }),
                        holds_candidate_lease: leases.is_some_and(|table| {
                            table.holds(LeaseOwner::Candidate {
                                key,
                                generation: generation.id,
                            })
                        }),
                    }
                })
                .collect();
            tasks.push(TaskProjection {
                key: key.0,
                display_id: entry.display_id.as_str().to_owned(),
                origin: match entry.origin {
                    crate::topology::registry::Origin::Original => "original",
                    crate::topology::registry::Origin::MergeRepair => "merge_repair",
                }
                .to_owned(),
                lineage: entry.lineage.as_ref().map(|lineage| TaskLineage {
                    root: lineage.root.0,
                    parent: lineage.parent.0,
                    index: lineage.index,
                }),
                lineage_root: entry.lineage.as_ref().map(|lineage| lineage.root.0),
                state: state_name(task.state).to_owned(),
                rung: task.rung,
                attempts_on_rung: task.attempts_on_rung,
                defers: task.defers,
                binding_override: fold.binding_override(key).cloned(),
                generations,
            });
        }

        let queue = fold
            .queue()
            .map(|queue| {
                queue
                    .entries()
                    .iter()
                    .map(|entry| QueueProjection {
                        candidate: entry.candidate.clone(),
                        paths: entry.paths.clone(),
                        lineage_root: entry.lineage_root.map(|root| root.0),
                        verification_deferred: entry.verification_deferred,
                        defers: entry.defers,
                        sequence: entry.sequence.map(|sequence| sequence.0),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let lineages = leases
            .map(|table| {
                table
                    .lineages()
                    .iter()
                    .map(|lease| LineageProjection {
                        root: lease.root.0,
                        paths: lease.paths.clone(),
                        age: lease.age,
                    })
                    .collect()
            })
            .unwrap_or_default();

        let open_questions = fold
            .open_questions()
            .map(|questions| {
                questions
                    .values()
                    .map(|open| QuestionProjection {
                        question: open.question.clone(),
                        origin: match open.origin {
                            QuestionOrigin::VerificationPark => "verification_park".to_owned(),
                            QuestionOrigin::Admission => "admission".to_owned(),
                        },
                        binding: open.binding.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let transaction = fold.transaction().map(|transaction| {
            let (class, expected_head, proposed_sha, disposition) = match &transaction.class {
                TransactionClass::VerificationStarted {
                    expected_head,
                    proposed_sha,
                    ..
                } => (
                    "verification_started",
                    expected_head.0.clone(),
                    proposed_sha.0.clone(),
                    None,
                ),
                TransactionClass::Prepared {
                    expected_head,
                    proposed_sha,
                    disposition,
                    ..
                } => (
                    "prepared",
                    expected_head.0.clone(),
                    proposed_sha.0.clone(),
                    Some(*disposition),
                ),
            };
            TransactionProjection {
                sequence: transaction.sequence.0,
                candidate: transaction.candidate.clone(),
                class: class.to_owned(),
                expected_head,
                proposed_sha,
                disposition,
            }
        });

        let incarnation = events
            .iter()
            .rev()
            .find_map(|event| match &event.body {
                TopologyEventBody::RunResumed { data } => Some(data.incarnation.0.clone()),
                _ => None,
            })
            .unwrap_or_else(|| started.incarnation.0.clone());

        let mut report = Self {
            schema: TOPOLOGY_SCHEMA,
            run_id: run_id.to_owned(),
            outcome,
            halted_at: fold.halted_at().map(|key| key.0),
            epoch: fold.epoch().map_or(0, |epoch| epoch.0),
            incarnation,
            runner: started.runner.clone(),
            budget_stop: fold.budget_stop(),
            tasks,
            queue,
            lineages,
            open_questions,
            transaction,
            retained_candidates,
            integration_ledger: integration_ledger(events),
            digest: String::new(),
        };
        report.digest = report.compute_digest()?;
        Ok(report)
    }

    pub fn compute_digest(&self) -> Result<String, UpstrokeError> {
        let mut undigested = self.clone();
        undigested.digest = String::new();
        let bytes = serde_json::to_vec(&undigested).map_err(|error| UpstrokeError::Parse {
            message: format!(
                "the report of run `{}` does not serialize: {error}",
                self.run_id
            ),
        })?;
        Ok(format!("{:x}", Sha256::digest(&bytes)))
    }

    #[must_use]
    pub fn is_fresh_against(&self, existing: &[u8]) -> bool {
        let Ok(existing) = serde_json::from_slice::<Self>(existing) else {
            return false;
        };
        existing.digest == self.digest
            && existing.compute_digest().ok().as_deref() == Some(self.digest.as_str())
            && existing.outcome == self.outcome
            && existing.runner == self.runner
    }

    #[must_use]
    pub fn render(&self) -> String {
        let mut out = TerminalLines::default();
        out.push(format_args!(
            "run {}: {} (epoch {}, incarnation {})",
            self.run_id,
            self.outcome
                .as_ref()
                .map_or_else(|| "in progress".to_owned(), outcome_label),
            self.epoch,
            self.incarnation
        ));
        match &self.runner.image {
            Some(image) => out.push(format_args!(
                "runner: {} ({}), image {} id {} digest {}",
                runner_kind(&self.runner),
                runner_policy(&self.runner),
                image.reference,
                image.id,
                image.digest.as_deref().unwrap_or("unknown")
            )),
            None => out.push(format_args!(
                "runner: {} ({}), no image",
                runner_kind(&self.runner),
                runner_policy(&self.runner)
            )),
        }
        if let Some(key) = self.halted_at {
            out.push(format_args!("halted at task {key}"));
        }
        if let Some(stop) = &self.budget_stop {
            out.push(format_args!(
                "budget stop: {} ceiling in epoch {}",
                stop.budget, stop.epoch.0
            ));
        }
        out.push(format_args!("tasks: {}", self.tasks.len()));
        for task in &self.tasks {
            out.push(format_args!(
                "  {} (k{}): {}, rung {}, {} attempt(s) on rung, {} deferral(s), {} generation(s){}{}",
                task.display_id,
                task.key,
                task.state,
                task.rung,
                task.attempts_on_rung,
                task.defers,
                task.generations.len(),
                task.lineage_root
                    .map_or_else(String::new, |root| format!(", lineage of k{root}")),
                task.binding_override
                    .as_ref()
                    .map_or_else(String::new, |binding| format!(
                        ", override {}/{}/{}",
                        binding.agent, binding.model, binding.effort
                    )),
            ));
        }
        if !self.queue.is_empty() {
            out.push(format_args!("queue: {} candidate(s)", self.queue.len()));
            for entry in &self.queue {
                out.push(format_args!(
                    "  k{} g{} {}{}",
                    entry.candidate.key.0,
                    entry.candidate.generation.0,
                    entry.candidate.commit_sha.0,
                    if entry.verification_deferred {
                        " (verification deferred)"
                    } else {
                        ""
                    }
                ));
            }
        }
        if !self.open_questions.is_empty() {
            out.push(format_args!(
                "open questions: {}",
                self.open_questions.len()
            ));
            for open in &self.open_questions {
                out.push(format_args!(
                    "  {} ({}) for task k{}",
                    open.question.id.0, open.origin, open.question.key.0
                ));
            }
        }
        if !self.retained_candidates.is_empty() {
            out.push(format_args!(
                "retained candidates refs: {}",
                self.retained_candidates.len()
            ));
            for retained in &self.retained_candidates {
                out.push(format_args!(
                    "  {} -> {}",
                    retained.candidates_ref, retained.commit_sha
                ));
            }
        }
        out.push(format_args!(
            "integration ledger: {} row(s)",
            self.integration_ledger.len()
        ));
        for row in &self.integration_ledger {
            out.push(format_args!(
                "  s{} k{} g{} {} expected {} proposed {} -> {} {} ({} review(s), cost {})",
                row.sequence,
                row.candidate.key.0,
                row.candidate.generation.0,
                row.basis,
                row.expected_head.as_deref().unwrap_or("-"),
                row.proposed_sha.as_deref().unwrap_or("-"),
                row.terminal,
                row.detail,
                row.reviews,
                match row.review_cost_usd {
                    Some(cost) if !row.review_cost_incomplete => format!("${cost:.4}"),
                    Some(cost) => format!("at least ${cost:.4} (?)"),
                    None => "? (unknown)".to_owned(),
                }
            ));
        }
        out.into_string()
    }
}

#[must_use]
pub fn outcome_label(outcome: &RunOutcome) -> String {
    match outcome {
        RunOutcome::Complete => "complete",
        RunOutcome::Parked => "parked",
        RunOutcome::Halted => "halted",
        RunOutcome::BudgetExceeded => "budget exceeded",
    }
    .to_owned()
}

fn runner_kind(runner: &RunnerPolicy) -> &'static str {
    match runner.kind {
        crate::topology::events::RunnerKind::Host => "host",
        crate::topology::events::RunnerKind::Container => "container",
    }
}

fn runner_policy(runner: &RunnerPolicy) -> &'static str {
    match runner.policy {
        crate::topology::events::RunnerContract::HostV1 => "host-v1",
        crate::topology::events::RunnerContract::ContainerV1 => "container-v1",
    }
}

fn state_name(state: TaskState) -> &'static str {
    match state {
        TaskState::Pending => "pending",
        TaskState::AwaitingMerge => "awaiting_merge",
        TaskState::AwaitingRepair => "awaiting_repair",
        TaskState::AwaitingInput => "awaiting_input",
        TaskState::Deferred => "deferred",
        TaskState::Merged => "merged",
        TaskState::Failed => "failed",
    }
}

fn review_cost(reviews: &[ReviewRecord]) -> (Option<f64>, bool) {
    let known: Vec<f64> = reviews
        .iter()
        .filter_map(|review| review.cost_usd)
        .collect();
    let incomplete = known.len() < reviews.len();
    if known.is_empty() {
        (None, incomplete)
    } else {
        (Some(known.iter().sum()), incomplete)
    }
}

fn row_for<'a>(
    rows: &'a mut BTreeMap<u32, LedgerRow>,
    sequence: u32,
    candidate: &CandidateRef,
) -> &'a mut LedgerRow {
    rows.entry(sequence).or_insert_with(|| LedgerRow {
        sequence,
        candidate: candidate.clone(),
        basis: "open".to_owned(),
        expected_head: None,
        proposed_sha: None,
        terminal: "open".to_owned(),
        detail: String::new(),
        verification: None,
        reviews: 0,
        review_cost_usd: None,
        review_cost_incomplete: false,
    })
}

fn charge(row: &mut LedgerRow, reviews: &[ReviewRecord]) {
    let (cost, incomplete) = review_cost(reviews);
    row.reviews = u32::try_from(reviews.len()).unwrap_or(u32::MAX);
    row.review_cost_usd = cost;
    row.review_cost_incomplete = incomplete;
}

#[must_use]
pub fn integration_ledger(events: &[TopologyEvent]) -> Vec<LedgerRow> {
    let mut rows: BTreeMap<u32, LedgerRow> = BTreeMap::new();
    let mut candidates: BTreeMap<u32, CandidateRef> = BTreeMap::new();
    for event in events {
        match &event.body {
            TopologyEventBody::MergeVerificationStarted { data } => {
                candidates.insert(data.sequence.0, data.candidate.clone());
                let row = row_for(&mut rows, data.sequence.0, &data.candidate);
                row.basis = match &data.basis {
                    VerificationBasis::StaleClean { .. } => "stale_clean".to_owned(),
                    VerificationBasis::AlreadyPresent => "already_present".to_owned(),
                };
                row.expected_head = Some(data.expected_head.0.clone());
                row.proposed_sha = Some(data.proposed_sha.0.clone());
            }
            TopologyEventBody::MergePrepared { data } => {
                let candidate = CandidateRef {
                    key: data.key,
                    generation: data.generation,
                    commit_sha: data.candidate_sha.clone(),
                    candidate_ref: data.candidate_ref.clone(),
                };
                candidates.insert(data.sequence.0, candidate.clone());
                let row = row_for(&mut rows, data.sequence.0, &candidate);
                row.basis = match data.disposition {
                    PreparedDisposition::Fast => "fast".to_owned(),
                    PreparedDisposition::StaleClean => "stale_clean".to_owned(),
                    PreparedDisposition::AlreadyPresent => "already_present".to_owned(),
                };
                row.expected_head = Some(data.expected_head.0.clone());
                row.proposed_sha = Some(data.proposed_sha.0.clone());
                row.terminal = "merge_prepared".to_owned();
                row.detail = format!(
                    "authorized; satisfies {}",
                    data.satisfies
                        .iter()
                        .map(|key| format!("k{}", key.0))
                        .collect::<Vec<_>>()
                        .join(",")
                );
                if let Some(verification) = &data.verification {
                    charge(row, &verification.reviews);
                    row.verification = Some(verification.clone());
                }
            }
            TopologyEventBody::TaskMerged { data } => {
                if let Some(candidate) = candidates.get(&data.sequence.0).cloned() {
                    let row = row_for(&mut rows, data.sequence.0, &candidate);
                    row.terminal = "task_merged".to_owned();
                    row.detail = format!("merged as {}", data.merged_sha.0);
                }
            }
            TopologyEventBody::MergeRejected { data } => {
                candidates.insert(data.sequence.0, data.candidate.clone());
                let row = row_for(&mut rows, data.sequence.0, &data.candidate);
                row.terminal = "merge_rejected".to_owned();
                match &data.disposition {
                    RejectionDisposition::Conflict { .. } => {
                        row.basis = "conflict".to_owned();
                        row.detail = format!(
                            "conflict at {}; repair k{}",
                            data.rejecting_head.0, data.repair.key.0
                        );
                    }
                    RejectionDisposition::CodeRejected { verification } => {
                        row.detail = format!(
                            "code rejected: {}; repair k{}",
                            verification.detail, data.repair.key.0
                        );
                        charge(row, &verification.reviews);
                        row.verification = Some(verification.clone());
                    }
                }
            }
            TopologyEventBody::MergeVerificationUnavailable { data } => {
                if let Some(candidate) = candidates.get(&data.sequence.0).cloned() {
                    let row = row_for(&mut rows, data.sequence.0, &candidate);
                    row.terminal = "merge_verification_unavailable".to_owned();
                    let cause = match &data.cause {
                        UnavailableCause::HumanRequired { verdict } => {
                            format!("human required: {verdict}")
                        }
                        UnavailableCause::Infrastructure { kind } => {
                            format!("infrastructure: {kind:?}")
                        }
                    };
                    row.detail = match &data.outcome {
                        UnavailableOutcome::Deferred { defers } => {
                            format!("deferred ({defers} deferral(s)); {cause}")
                        }
                        UnavailableOutcome::Parked { question } => {
                            format!("parked on question {}; {cause}", question.id.0)
                        }
                    };
                    charge(row, &data.reviews);
                }
            }
            TopologyEventBody::MergeVerificationInterrupted { data } => {
                if let Some(candidate) = candidates.get(&data.sequence.0).cloned() {
                    let row = row_for(&mut rows, data.sequence.0, &candidate);
                    row.terminal = "merge_verification_interrupted".to_owned();
                    row.detail = data.detail.clone();
                }
            }
            _ => {}
        }
    }
    rows.into_values().collect()
}

#[must_use]
pub fn merged_and_parked(fold: &TopologyFold) -> (u32, u32) {
    let Some(registry) = fold.registry() else {
        return (0, 0);
    };
    let mut merged = 0_u32;
    let mut parked = 0_u32;
    for entry in registry.entries() {
        match fold.task(entry.key).map(|task| task.state) {
            Some(TaskState::Merged) => merged = merged.saturating_add(1),
            Some(TaskState::AwaitingInput) => parked = parked.saturating_add(1),
            _ => {}
        }
    }
    (merged, parked)
}

pub fn topology_status(
    run_id: &str,
    prefix: &crate::events::log::StablePrefix,
) -> Result<TopologyReport, UpstrokeError> {
    TopologyReport::derive(run_id, prefix.fold(), prefix.events())
}
