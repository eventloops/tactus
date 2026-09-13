//! Extended notes: `docs/internals/engine/topology/closure.md`

use crate::error::UpstrokeError;
use crate::events::RunOutcome;
use crate::topology::events::{DerivedOutcome, RunFinished4};
use crate::topology::fold::{GenerationClass, TaskState, TopologyFold};
use crate::topology::registry::TaskKey;

use super::report::{merged_and_parked, outcome_label};

pub fn ending_outcome(fold: &TopologyFold) -> Result<RunOutcome, UpstrokeError> {
    if fold.epoch().is_none() {
        return Err(refused(
            "the run has not started, so there is nothing to close and no outcome to derive",
        ));
    }
    if fold.halted_at().is_some() {
        return Ok(RunOutcome::Halted);
    }
    if fold.budget_stop().is_some() {
        return Ok(RunOutcome::BudgetExceeded);
    }
    match fold.derived_outcome() {
        DerivedOutcome::Ending(outcome) => Ok(outcome),
        DerivedOutcome::NotEnding => Err(refused(&format!(
            "the run is not ending: nothing is selectable and no halting settlement or budget \
             stop is recorded, yet the fold derives NotEnding because {}. Nothing was appended",
            blockers(fold).join("; ")
        ))),
        DerivedOutcome::FoldError => Err(refused(
            "the fold derives no outcome for this state (the arm the census asserts unreachable); \
             nothing was appended",
        )),
    }
}

pub fn refuse_unclosable(fold: &TopologyFold) -> Result<(), UpstrokeError> {
    let unclosable = unclosable(fold);
    if unclosable.is_empty() {
        return Ok(());
    }
    Err(refused(&format!(
        "run-end closure at max_parallel = 1 found work the synchronous loop never leaves open \
         and that a fresh process's recovery settles before the loop runs: {}. Closure under \
         concurrency (in-flight cancellation, the budget drain, promotion and publication \
         completion inside closure) is PR11's; nothing was appended",
        unclosable.join("; ")
    )))
}

#[must_use]
pub fn unclosable(fold: &TopologyFold) -> Vec<String> {
    let mut found = Vec::new();
    for key in keys(fold) {
        let Some(task) = fold.task(key) else { continue };
        for generation in &task.generations {
            match &generation.class {
                GenerationClass::InFlight { attempt } => found.push(format!(
                    "task k{} generation {} is in flight (attempt {})",
                    key.0, generation.id.0, attempt.0
                )),
                GenerationClass::Promoting => found.push(format!(
                    "task k{} generation {} is promoting",
                    key.0, generation.id.0
                )),
                GenerationClass::OpenNoAttempt
                | GenerationClass::RetainedIdle { .. }
                | GenerationClass::Closed => {}
            }
        }
    }
    if let Some(transaction) = fold.transaction() {
        found.push(format!(
            "integration sequence {} of task k{} is unresolved",
            transaction.sequence.0, transaction.candidate.key.0
        ));
    }
    found
}

#[must_use]
pub fn closable(fold: &TopologyFold) -> Vec<TaskKey> {
    keys(fold)
        .filter(|key| {
            fold.task(*key).is_some_and(|task| {
                task.generations.iter().any(|generation| {
                    matches!(
                        generation.class,
                        GenerationClass::OpenNoAttempt | GenerationClass::RetainedIdle { .. }
                    )
                })
            })
        })
        .collect()
}

pub fn confirm_derived(fold: &TopologyFold, outcome: &RunOutcome) -> Result<(), UpstrokeError> {
    match fold.derived_outcome() {
        DerivedOutcome::Ending(derived) if derived == *outcome => Ok(()),
        DerivedOutcome::Ending(derived) => Err(refused(&format!(
            "closure was ending the run as `{}` and, with every open generation closed, the fold \
             derives `{}`; nothing was appended for the end",
            outcome_label(outcome),
            outcome_label(&derived)
        ))),
        DerivedOutcome::NotEnding => Err(refused(&format!(
            "closure closed every open generation and the run is still not ending because {}; \
             nothing was appended for the end",
            blockers(fold).join("; ")
        ))),
        DerivedOutcome::FoldError => Err(refused(
            "closure closed every open generation and the fold derives no outcome for what is \
             left (the arm the census asserts unreachable); nothing was appended for the end",
        )),
    }
}

#[must_use]
pub fn run_finished(fold: &TopologyFold, outcome: RunOutcome) -> RunFinished4 {
    let (merged, parked) = merged_and_parked(fold);
    RunFinished4 {
        outcome,
        halted_at: fold.halted_at(),
        merged,
        parked,
    }
}

#[must_use]
pub fn blockers(fold: &TopologyFold) -> Vec<String> {
    let mut found = unclosable(fold);
    for key in keys(fold) {
        let Some(task) = fold.task(key) else { continue };
        for generation in &task.generations {
            match &generation.class {
                GenerationClass::OpenNoAttempt => found.push(format!(
                    "task k{} generation {} is open with no attempt",
                    key.0, generation.id.0
                )),
                GenerationClass::RetainedIdle { incarnation, .. } => found.push(format!(
                    "task k{} generation {} is retained idle from epoch {}",
                    key.0, generation.id.0, incarnation.0
                )),
                _ => {}
            }
        }
        if task.state == TaskState::Deferred {
            found.push(format!("task k{} is deferred (backoff pending)", key.0));
        }
    }
    if let Some(queue) = fold.queue() {
        for entry in queue.entries() {
            if entry.verification_deferred {
                found.push(format!(
                    "candidate k{} g{} is verification-deferred (backoff pending)",
                    entry.candidate.key.0, entry.candidate.generation.0
                ));
            }
        }
    }
    if fold.structurally_admissible() {
        found.push("structurally admissible work exists".to_owned());
    }
    if found.is_empty() {
        found.push("of a state this module cannot name".to_owned());
    }
    found
}

fn keys(fold: &TopologyFold) -> impl Iterator<Item = TaskKey> + '_ {
    let len = fold.registry().map_or(0, |registry| {
        u32::try_from(registry.len()).unwrap_or(u32::MAX)
    });
    (0..len).map(TaskKey)
}

fn refused(message: &str) -> UpstrokeError {
    UpstrokeError::Refused {
        message: message.to_owned(),
    }
}
