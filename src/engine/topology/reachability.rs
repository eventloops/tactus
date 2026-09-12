//! Extended notes: `docs/internals/engine/topology/reachability.md`

use std::collections::BTreeMap;

use serde::Serialize;

use crate::events::RunOutcome;
use crate::topology::census::{Census, TransitionOutcome};
use crate::topology::effects::FaultRow;
use crate::topology::events::{DerivedOutcome, PreparedDisposition, VerificationBasis};
use crate::topology::fold::{GenerationClass, TaskState, TopologyFold, TransactionClass};
use crate::topology::leases::GenerationLease;
use crate::topology::registry::TaskKey;

use super::report::outcome_label;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ResumeAction {
    NotStarted,
    FinalizeThenRefuse { outcome: RunOutcome },
    Recover(RecoveryPlan),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RecoveryPlan {
    pub reopens: Option<RunOutcome>,
    pub settle_interrupted: Vec<InFlightIdentity>,
    pub close_retained: Vec<GenerationIdentity>,
    pub complete_promotions: Vec<GenerationIdentity>,
    pub publication: Option<PendingPublication>,
    pub interrupted_verification: Option<PendingVerification>,
    pub recreate_open: Vec<GenerationIdentity>,
    pub wakes_deferred_tasks: Vec<u32>,
    pub wakes_deferred_candidates: u32,
    pub clears_budget_stop: bool,
    pub open_questions: u32,
    pub halted: bool,
    pub derived: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InFlightIdentity {
    pub key: u32,
    pub generation: u32,
    pub attempt: u32,
    pub lineage: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GenerationIdentity {
    pub key: u32,
    pub generation: u32,
    pub lineage: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PendingPublication {
    pub sequence: u32,
    pub key: u32,
    pub disposition: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PendingVerification {
    pub sequence: u32,
    pub key: u32,
    pub pinned: bool,
}

#[must_use]
pub fn derived_label(derived: &DerivedOutcome) -> String {
    match derived {
        DerivedOutcome::NotEnding => "not ending".to_owned(),
        DerivedOutcome::Ending(outcome) => format!("ending: {}", outcome_label(outcome)),
        DerivedOutcome::FoldError => "no outcome".to_owned(),
    }
}

#[must_use]
pub fn classify(fold: &TopologyFold) -> ResumeAction {
    if fold.epoch().is_none() {
        return ResumeAction::NotStarted;
    }
    let reopens = match fold.finished() {
        Some(outcome @ (RunOutcome::Complete | RunOutcome::Halted)) => {
            return ResumeAction::FinalizeThenRefuse {
                outcome: outcome.clone(),
            };
        }
        Some(outcome @ (RunOutcome::Parked | RunOutcome::BudgetExceeded)) => Some(outcome.clone()),
        None => None,
    };

    let mut plan = RecoveryPlan {
        reopens,
        clears_budget_stop: fold.budget_stop().is_some(),
        open_questions: fold.open_questions().map_or(0, |questions| {
            u32::try_from(questions.len()).unwrap_or(u32::MAX)
        }),
        halted: fold.halted_at().is_some(),
        derived: derived_label(&fold.derived_outcome()),
        ..RecoveryPlan::default()
    };

    for key in keys(fold) {
        let Some(task) = fold.task(key) else { continue };
        if task.state == TaskState::Deferred {
            plan.wakes_deferred_tasks.push(key.0);
        }
        for generation in &task.generations {
            let lineage = matches!(generation.lease, GenerationLease::InheritedLineage { .. });
            let identity = GenerationIdentity {
                key: key.0,
                generation: generation.id.0,
                lineage,
            };
            match &generation.class {
                GenerationClass::OpenNoAttempt => plan.recreate_open.push(identity),
                GenerationClass::InFlight { attempt } => {
                    plan.settle_interrupted.push(InFlightIdentity {
                        key: key.0,
                        generation: generation.id.0,
                        attempt: attempt.0,
                        lineage,
                    });
                }
                GenerationClass::RetainedIdle { .. } => plan.close_retained.push(identity),
                GenerationClass::Promoting => plan.complete_promotions.push(identity),
                GenerationClass::Closed => {}
            }
        }
    }

    if let Some(queue) = fold.queue() {
        plan.wakes_deferred_candidates = u32::try_from(
            queue
                .entries()
                .iter()
                .filter(|entry| entry.verification_deferred)
                .count(),
        )
        .unwrap_or(u32::MAX);
    }

    if let Some(transaction) = fold.transaction() {
        match &transaction.class {
            TransactionClass::Prepared { disposition, .. } => {
                plan.publication = Some(PendingPublication {
                    sequence: transaction.sequence.0,
                    key: transaction.candidate.key.0,
                    disposition: match disposition {
                        PreparedDisposition::Fast => "fast",
                        PreparedDisposition::StaleClean => "stale_clean",
                        PreparedDisposition::AlreadyPresent => "already_present",
                    }
                    .to_owned(),
                });
            }
            TransactionClass::VerificationStarted { basis, .. } => {
                plan.interrupted_verification = Some(PendingVerification {
                    sequence: transaction.sequence.0,
                    key: transaction.candidate.key.0,
                    pinned: matches!(basis, VerificationBasis::StaleClean { .. }),
                });
            }
        }
    }

    ResumeAction::Recover(plan)
}

#[must_use]
pub fn rows_reached(fold: &TopologyFold, action: &ResumeAction) -> Vec<FaultRow> {
    let mut rows = Vec::new();
    let plan = match action {
        ResumeAction::NotStarted => return rows,
        ResumeAction::FinalizeThenRefuse { .. } => {
            rows.push(FaultRow::TFinalize);
            return rows;
        }
        ResumeAction::Recover(plan) => plan,
    };

    if fold.epoch().is_some()
        && fold.task_count() > 0
        && keys(fold).all(|key| {
            fold.task(key)
                .is_some_and(|task| task.generations.is_empty())
        })
        && fold.finished().is_none()
    {
        rows.push(FaultRow::TRunstart);
    }
    if plan.recreate_open.iter().any(|open| !open.lineage) {
        rows.push(FaultRow::TDispatch);
    }
    if plan.recreate_open.iter().any(|open| open.lineage) {
        rows.push(FaultRow::TRepairDispatch);
    }
    if plan
        .settle_interrupted
        .iter()
        .any(|attempt| attempt.attempt == 1)
    {
        rows.push(FaultRow::TAttempt);
        rows.push(FaultRow::TCandObj);
    }
    if plan
        .settle_interrupted
        .iter()
        .any(|attempt| attempt.attempt > 1)
    {
        rows.push(FaultRow::TRetry);
    }
    if !plan.complete_promotions.is_empty() {
        rows.push(FaultRow::TCandRef);
    }
    if !plan.close_retained.is_empty() {
        rows.push(FaultRow::TRetained);
    }
    if keys(fold).any(|key| {
        fold.task(key).is_some_and(|task| {
            task.state == TaskState::AwaitingMerge
                && task.generations.iter().any(|generation| {
                    generation.class == GenerationClass::Closed && generation.candidate.is_some()
                })
        })
    }) {
        rows.push(FaultRow::TScrub);
    }
    if keys(fold).any(|key| {
        fold.task(key).is_some_and(|task| {
            matches!(
                task.state,
                TaskState::Failed | TaskState::AwaitingInput | TaskState::Deferred
            ) && task
                .generations
                .iter()
                .any(|generation| generation.attempts > 0)
        })
    }) {
        rows.push(FaultRow::TFailed);
    }
    match &plan.publication {
        Some(publication) if publication.disposition == "fast" => rows.push(FaultRow::TFast),
        Some(_) => rows.push(FaultRow::TPrepared),
        None => {}
    }
    if let Some(verification) = &plan.interrupted_verification {
        rows.push(FaultRow::TVerify);
        if verification.pinned {
            rows.push(FaultRow::TProposal);
        }
    }
    if fold.registry().is_some_and(|registry| {
        registry
            .entries()
            .iter()
            .any(|entry| entry.lineage.is_some())
    }) {
        rows.push(FaultRow::TReject);
    }
    if plan.open_questions > 0 {
        rows.push(FaultRow::TAnswer);
    }
    if fold.finished().is_none()
        && (matches!(fold.derived_outcome(), DerivedOutcome::Ending(_)) || fold.run_is_ending())
    {
        rows.push(FaultRow::TFinish);
    }
    if plan.reopens.is_some() {
        rows.push(FaultRow::TResume);
    }
    rows
}

#[must_use]
pub const fn outside_the_fold(row: FaultRow) -> bool {
    matches!(row, FaultRow::TContainer | FaultRow::TAppend)
}

#[must_use]
pub fn matches_row(row: FaultRow, action: &ResumeAction) -> bool {
    let plan = match (row, action) {
        (FaultRow::TFinalize, ResumeAction::FinalizeThenRefuse { .. }) => return true,
        (FaultRow::TFinalize, _) | (_, ResumeAction::FinalizeThenRefuse { .. }) => return false,
        (_, ResumeAction::NotStarted) => return false,
        (_, ResumeAction::Recover(plan)) => plan,
    };
    match row {
        FaultRow::TRunstart => {
            plan.recreate_open.is_empty()
                && plan.settle_interrupted.is_empty()
                && plan.close_retained.is_empty()
                && plan.complete_promotions.is_empty()
                && plan.publication.is_none()
                && plan.interrupted_verification.is_none()
        }
        FaultRow::TDispatch => plan.recreate_open.iter().any(|open| !open.lineage),
        FaultRow::TRepairDispatch => plan.recreate_open.iter().any(|open| open.lineage),
        FaultRow::TAttempt | FaultRow::TCandObj => !plan.settle_interrupted.is_empty(),
        FaultRow::TRetry => plan
            .settle_interrupted
            .iter()
            .any(|attempt| attempt.attempt > 1),
        FaultRow::TCandRef => !plan.complete_promotions.is_empty(),
        FaultRow::TRetained => !plan.close_retained.is_empty(),
        // The item these rows are about needs no recovery event of its own: a
        // scrubbed candidate is re-scrubbed idempotently, a settled task, a
        // registered repair and an open question are read from the prefix.
        // Whatever another task in the same state needs is that task's row.
        FaultRow::TScrub | FaultRow::TFailed | FaultRow::TReject | FaultRow::TAnswer => true,
        FaultRow::TFast | FaultRow::TPrepared => plan.publication.is_some(),
        FaultRow::TProposal | FaultRow::TVerify => plan.interrupted_verification.is_some(),
        // A closure in progress: the run is ending (a halting settlement, a
        // budget stop, or nothing left to select) and no run_finished is
        // durable yet. The next process repeats the closure steps for the
        // classes still open — which is what the plan's other fields carry —
        // then evaluates derived_outcome and appends run_finished.
        FaultRow::TFinish => {
            plan.reopens.is_none()
                && (plan.derived != "not ending" || plan.halted || plan.clears_budget_stop)
        }
        FaultRow::TResume => plan.reopens.is_some(),
        FaultRow::TContainer | FaultRow::TAppend | FaultRow::TFinalize => false,
    }
}

#[must_use]
pub fn row_name(row: FaultRow) -> String {
    let debug = format!("{row:?}");
    let mut out = String::with_capacity(debug.len() + 4);
    for (index, ch) in debug.char_indices() {
        if index == 0 {
            out.push('T');
            continue;
        }
        if ch.is_ascii_uppercase() {
            out.push('-');
        }
        out.push(ch.to_ascii_uppercase());
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RowSummary {
    pub reachable_states: usize,
    pub every_reachable_state_classifies_as_tabled: bool,
    pub outside_the_fold: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CensusSummary {
    pub bounds: BTreeMap<String, u64>,
    pub states: usize,
    pub transitions: usize,
    pub accepted: usize,
    pub refused: usize,
    pub truncated: bool,
    pub outcomes: BTreeMap<String, usize>,
    pub actions: BTreeMap<String, usize>,
    pub fault_rows: BTreeMap<String, RowSummary>,
    pub classification_equal_live_and_on_replay: bool,
}

#[must_use]
pub fn summarize(census: &Census, classification_equal_live_and_on_replay: bool) -> CensusSummary {
    let bounds = census.bounds();
    let mut bound_map: BTreeMap<String, u64> = bounds
        .dimensions()
        .iter()
        .map(|(name, bound)| ((*name).to_owned(), u64::from(*bound)))
        .collect();
    bound_map.insert(
        "max_trace".to_owned(),
        u64::try_from(bounds.max_trace).unwrap_or(u64::MAX),
    );
    bound_map.insert(
        "max_states".to_owned(),
        u64::try_from(bounds.max_states).unwrap_or(u64::MAX),
    );

    let mut outcomes: BTreeMap<String, usize> = BTreeMap::new();
    let mut actions: BTreeMap<String, usize> = BTreeMap::new();
    let mut rows: BTreeMap<String, RowSummary> = FaultRow::ALL
        .iter()
        .map(|row| {
            (
                row_name(*row),
                RowSummary {
                    reachable_states: 0,
                    every_reachable_state_classifies_as_tabled: true,
                    outside_the_fold: outside_the_fold(*row),
                },
            )
        })
        .collect();
    for state in census.states() {
        *outcomes.entry(derived_label(&state.outcome)).or_insert(0) += 1;
        let action = classify(&state.fold);
        *actions.entry(action_label(&action)).or_insert(0) += 1;
        for row in rows_reached(&state.fold, &action) {
            if let Some(summary) = rows.get_mut(&row_name(row)) {
                summary.reachable_states += 1;
                if !matches_row(row, &action) {
                    summary.every_reachable_state_classifies_as_tabled = false;
                }
            }
        }
    }
    let accepted = census
        .transitions()
        .iter()
        .filter(|transition| matches!(transition.outcome, TransitionOutcome::Accepted { .. }))
        .count();
    CensusSummary {
        bounds: bound_map,
        states: census.states().len(),
        transitions: census.transitions().len(),
        accepted,
        refused: census.transitions().len().saturating_sub(accepted),
        truncated: census.truncated(),
        outcomes,
        actions,
        fault_rows: rows,
        classification_equal_live_and_on_replay,
    }
}

#[must_use]
pub fn action_label(action: &ResumeAction) -> String {
    match action {
        ResumeAction::NotStarted => "not started".to_owned(),
        ResumeAction::FinalizeThenRefuse { outcome } => {
            format!("finalize then refuse: {}", outcome_label(outcome))
        }
        ResumeAction::Recover(plan) => {
            let mut parts = Vec::new();
            if let Some(outcome) = &plan.reopens {
                parts.push(format!("reopen {}", outcome_label(outcome)));
            }
            if !plan.settle_interrupted.is_empty() {
                parts.push("settle interrupted".to_owned());
            }
            if !plan.close_retained.is_empty() {
                parts.push("close retained".to_owned());
            }
            if !plan.complete_promotions.is_empty() {
                parts.push("complete promotion".to_owned());
            }
            if plan.publication.is_some() {
                parts.push("complete publication".to_owned());
            }
            if plan.interrupted_verification.is_some() {
                parts.push("settle verification interrupted".to_owned());
            }
            if !plan.recreate_open.is_empty() {
                parts.push("recreate open".to_owned());
            }
            if parts.is_empty() {
                parts.push("run_resumed only".to_owned());
            }
            format!("recover: {}", parts.join(", "))
        }
    }
}

fn keys(fold: &TopologyFold) -> impl Iterator<Item = TaskKey> + '_ {
    (0..u32::try_from(fold.task_count()).unwrap_or(u32::MAX)).map(TaskKey)
}
