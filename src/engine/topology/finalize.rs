//! Extended notes: `docs/internals/engine/topology/finalize.md`

use std::path::Path;

use crate::error::UpstrokeError;
use crate::events::RunOutcome;
use crate::rundir;
use crate::topology::effects::RefSite;
use crate::topology::events::TopologyEvent;
use crate::topology::fold::TopologyFold;
use crate::workspace_manager::{Slot, WorkspaceManager};

use super::candidate::run_namespace;
use super::report::{TopologyReport, outcome_label};
use super::seams::TopologyHooks;

pub struct Finalize<'a> {
    pub manager: &'a WorkspaceManager,
    pub public: &'a Path,
    pub run_id: &'a str,
    pub fold: &'a TopologyFold,
    pub events: &'a [TopologyEvent],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupStep {
    TaskWorktrees,
    Snapshots,
    Staging,
    Pins,
    CandidatesRefs,
    ExecutionRoot,
}

impl CleanupStep {
    pub const ORDER: [Self; 6] = [
        Self::TaskWorktrees,
        Self::Snapshots,
        Self::Staging,
        Self::Pins,
        Self::CandidatesRefs,
        Self::ExecutionRoot,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::TaskWorktrees => "task worktrees and intents",
            Self::Snapshots => "snapshots and intents",
            Self::Staging => "staging worktrees and intents",
            Self::Pins => "prepared and candidate-prepared pins",
            Self::CandidatesRefs => "candidates refs",
            Self::ExecutionRoot => "execution root",
        }
    }

    #[must_use]
    pub const fn applies_to(self, outcome: &RunOutcome) -> bool {
        match self {
            Self::Snapshots | Self::Staging | Self::Pins | Self::ExecutionRoot => true,
            Self::TaskWorktrees => matches!(outcome, RunOutcome::Complete | RunOutcome::Halted),
            Self::CandidatesRefs => matches!(outcome, RunOutcome::Complete),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finalized {
    pub outcome: RunOutcome,
    pub report_written: bool,
    pub removed: Vec<(CleanupStep, usize)>,
    pub execution_root_removed: bool,
    pub retained_candidates: usize,
}

pub fn finalize(
    inputs: &Finalize<'_>,
    hooks: &mut dyn TopologyHooks,
) -> Result<Finalized, UpstrokeError> {
    let outcome = inputs
        .fold
        .finished()
        .cloned()
        .ok_or_else(|| UpstrokeError::Refused {
            message: format!(
                "run `{}` has no durable `run_finished`, and finalization acts only after a \
                 valid one; nothing was written and nothing was deleted",
                inputs.run_id
            ),
        })?;

    let report = TopologyReport::derive(inputs.run_id, inputs.fold, inputs.events)?;
    let report_path = inputs.public.join("report.json");
    let fresh = match std::fs::read(&report_path) {
        Ok(existing) => report.is_fresh_against(&existing),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(source) => {
            return Err(UpstrokeError::Io {
                path: report_path,
                source,
            });
        }
    };
    if !fresh {
        rundir::write_report(inputs.public, &report, hooks.rundir())?;
    }

    let namespace = run_namespace(inputs.run_id);
    let mut removed = Vec::with_capacity(CleanupStep::ORDER.len());
    let mut execution_root_removed = false;
    for step in CleanupStep::ORDER {
        if !step.applies_to(&outcome) {
            continue;
        }
        let count = match step {
            CleanupStep::TaskWorktrees => {
                scrub_slots(inputs.manager, hooks, |slot| matches!(slot, Slot::Task { .. }))?
            }
            CleanupStep::Snapshots => scrub_slots(inputs.manager, hooks, |slot| {
                matches!(slot, Slot::Snapshot { .. })
            })?,
            CleanupStep::Staging => scrub_slots(inputs.manager, hooks, |slot| {
                matches!(slot, Slot::Staging { .. })
            })?,
            CleanupStep::Pins => {
                let prepared = delete_refs_under(
                    inputs.manager,
                    hooks,
                    &format!("{namespace}prepared/"),
                    RefSite::DeletePreparedPin,
                )?;
                let candidate_pins = delete_refs_under(
                    inputs.manager,
                    hooks,
                    &format!("{namespace}candidate-prepared/"),
                    RefSite::DeleteCandidatePin,
                )?;
                prepared.saturating_add(candidate_pins)
            }
            CleanupStep::CandidatesRefs => delete_refs_under(
                inputs.manager,
                hooks,
                &format!("{namespace}candidates/"),
                RefSite::DeleteCandidatesRef,
            )?,
            CleanupStep::ExecutionRoot => {
                execution_root_removed = inputs.manager.remove_execution_root(hooks.effects())?;
                usize::from(execution_root_removed)
            }
        };
        removed.push((step, count));
    }

    Ok(Finalized {
        outcome,
        report_written: !fresh,
        removed,
        execution_root_removed,
        retained_candidates: report.retained_candidates.len(),
    })
}

pub fn refuse_continuation(run_id: &str, finalized: &Finalized) -> UpstrokeError {
    UpstrokeError::Refused {
        message: format!(
            "run `{run_id}` already finished as `{}`, and a finished run does not continue. \
             Recovery step (b) finalized it first: the report was {}, {} and continuation is \
             refused",
            outcome_label(&finalized.outcome),
            if finalized.report_written {
                "regenerated"
            } else {
                "already current"
            },
            finalized
                .removed
                .iter()
                .map(|(step, count)| format!("{count} {}", step.label()))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn scrub_slots(
    manager: &WorkspaceManager,
    hooks: &mut dyn TopologyHooks,
    keep: impl Fn(&Slot) -> bool,
) -> Result<usize, UpstrokeError> {
    let mut count = 0;
    for slot in manager.intents()? {
        if !keep(&slot) {
            continue;
        }
        manager.remove_worktree(hooks.effects(), &slot)?;
        manager.remove_intent(hooks.effects(), &slot)?;
        count += 1;
    }
    Ok(count)
}

fn delete_refs_under(
    manager: &WorkspaceManager,
    hooks: &mut dyn TopologyHooks,
    prefix: &str,
    site: RefSite,
) -> Result<usize, UpstrokeError> {
    let mut count = 0;
    for (refname, oid) in manager.refs_under(prefix)? {
        manager.delete_ref_expected_old(hooks.effects(), site, &refname, &oid)?;
        count += 1;
    }
    Ok(count)
}
