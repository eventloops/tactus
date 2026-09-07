//! The frozen repair a `merge_rejected` registers atomically with the
//! rejection.
//!
//! `decisions.repairs.merge_rejected` and `DESIGN.md` §26.4: a conflict or a
//! code-attributed rejection records "one `merge_rejected` event whose
//! embedded frozen-spawn payload" carries the complete synthetic Fix task, so
//! rejection and registration are one append. This module builds that payload
//! and the whole `MergeRejected` around it; **it registers a repair, it never
//! dispatches one** — dispatch is `T-REPAIR-DISPATCH`, PR9's, and the
//! checkpoint refuses it.
//!
//! What the payload freezes, from the contract:
//!
//! * the repair descends from the rejected candidate: `lineage = {root,
//!   parent, index}`, the root being the candidate's own lineage root (or the
//!   candidate itself, for an ordinary one), the parent the rejected key, and
//!   the index the number of repairs the root already holds;
//! * `min_tier = mid` intersected with the root's frozen floor, pin and
//!   ceiling — the router's floor read off a recorded ladder rather than
//!   re-run — with an empty intersection registered `HumanBinding`;
//! * path hints expanded by the candidate's actual changed paths and the
//!   conflict paths (`§26.4`);
//! * the original acceptance plus the preserve-merged-behaviour requirement;
//! * admission `Runnable`, or `HumanRequired` once the lineage has consumed
//!   the frozen automatic-repair limit, or `HumanBinding` when the tier
//!   intersection is empty — the empty intersection winning, because without a
//!   binding nothing can run whatever the limit says.

use crate::error::UpstrokeError;
use crate::ir::{QuestionKind, Tier};
use crate::topology::events::{
    CandidateRef, CommitSha, FrozenQuestion, FrozenSpawn, MergeRejected, RejectionDisposition,
    RejectionLeaseEffect, SequenceId, SpawnAdmission, VerificationRecord,
};
use crate::topology::fold::TopologyFold;
use crate::topology::paths::PathSet;
use crate::topology::registry::{
    Admission, FrozenLadder, Lineage, Origin, TaskEntry, TaskKey, repair_display_id,
};

use super::seams::IdSource;

/// The mid floor a merge repair starts no lower than, before the root's own
/// floor and ceiling clip it further.
const REPAIR_FLOOR: Tier = Tier::Mid;

/// The requirement every merge repair carries beyond the root's acceptance.
const PRESERVE_MERGED: &str = "preserve the behaviour already merged onto the integration head; the rejected candidate's \
     change must integrate without regressing it";

/// Build the `merge_rejected` for a rejected candidate: the terminal event, its
/// embedded repair, and its lease effect, all as one record.
///
/// `disposition` is what rejected the candidate — a textual conflict at the
/// cherry-pick, or a code-attributed gate/review failure — and `contended` is
/// the region the repair lineage takes a lease on: the conflict paths for a
/// conflict, the candidate's actual changed paths for a code rejection.
///
/// # Errors
///
/// A refusal when the run has not started or the candidate is not registered.
pub fn merge_rejected(
    fold: &TopologyFold,
    ids: &dyn IdSource,
    candidate: &CandidateRef,
    rejecting_head: CommitSha,
    sequence: SequenceId,
    disposition: RejectionDisposition,
    contended: PathSet,
) -> Result<MergeRejected, UpstrokeError> {
    let registry = fold
        .registry()
        .ok_or_else(|| refused("the run has not started"))?;
    let rejected_entry = registry
        .get(candidate.key)
        .ok_or_else(|| refused(&format!("task {} is not registered", candidate.key)))?;
    let root = rejected_entry
        .lineage
        .map_or(candidate.key, |lineage| lineage.root);
    let root_entry = registry
        .get(root)
        .ok_or_else(|| refused(&format!("lineage root {root} is not registered")))?;
    let members = fold
        .lineage_members(root)
        .ok_or_else(|| refused("the run has not started"))?;
    let limit = fold
        .started()
        .ok_or_else(|| refused("the run has not started"))?
        .limits
        .max_merge_repairs;

    let key = TaskKey(u32::try_from(registry.len()).map_err(|_| refused("the registry is full"))?);
    let ladder = repair_ladder(&root_entry.ladder, &root_entry.allowed_agents);
    let hints = expand_hints(
        &root_entry.spec.path_hints,
        &candidate_paths(fold, candidate),
        &contended,
    );

    // The specification is the one part of the root the repair rewrites —
    // its kind, its hints and its acceptance — so it is cloned once and
    // edited in place (§6); the rest of the entry copies the root's fields
    // because the contract fixes them as the root's (`decisions.repairs`:
    // the root's authoritative deps, and the inherited review and agent
    // policy), and a registry row owns what it records.
    let mut spec = root_entry.spec.clone();
    spec.kind = crate::ir::TaskKind::Fix;
    spec.path_hints = hints;
    spec.acceptance.push(PRESERVE_MERGED.to_owned());

    let entry = TaskEntry {
        key,
        display_id: crate::ir::TaskId::from(
            repair_display_id(members, &root_entry.display_id).as_str(),
        ),
        origin: Origin::MergeRepair,
        spec,
        deps: root_entry.deps.clone(),
        display_deps: root_entry.display_deps.clone(),
        ladder,
        reviews: root_entry.reviews.clone(),
        allowed_agents: root_entry.allowed_agents.clone(),
        lineage: Some(Lineage {
            root,
            parent: candidate.key,
            index: members,
        }),
    };

    let admission = admission_for(&entry, members, limit, ids, key);
    let lease_effect = if rejected_entry.lineage.is_some() {
        RejectionLeaseEffect::WidensLineage {
            root,
            paths: contended,
        }
    } else {
        RejectionLeaseEffect::CreatesLineage {
            root,
            paths: contended,
        }
    };

    Ok(MergeRejected {
        sequence,
        candidate: candidate.clone(),
        rejecting_head,
        disposition,
        repair: FrozenSpawn {
            key,
            entry,
            admission,
        },
        lease_effect,
    })
}

/// The candidate's actual changed region, as `candidate_prepared` recorded it.
fn candidate_paths(fold: &TopologyFold, candidate: &CandidateRef) -> PathSet {
    fold.task(candidate.key)
        .and_then(|task| {
            task.generations
                .iter()
                .find(|generation| generation.id == candidate.generation)
        })
        .and_then(|generation| generation.candidate.as_ref())
        .map_or(PathSet::RepoWide, |prepared| prepared.paths.clone())
}

/// The repair's ladder: `min_tier = mid` intersected with the root's frozen
/// floor and ceiling (`decisions.repairs.routing`).
///
/// The floor is `max(mid, root floor)`; the root's rungs at or above it
/// survive, in the root's order, and the ceiling is the highest of them.
/// When none survives the record says so: no tier, no rung, no ceiling, the
/// raised floor the repair still has to meet, and the ladder admitted
/// `HumanBinding` over the entry's allowed agents — every agent the run
/// probed, which is what `check_spawn` binds `allowed_agents` to — so a
/// person names what runs (E2: the answer's override is validated against
/// these options). The rungs the intersection excluded are not offered back:
/// each of them is below the floor by construction. `check_ladder` accepts
/// the shape — an absent ceiling is the maximum of an empty tier list.
fn repair_ladder(root: &FrozenLadder, allowed_agents: &[String]) -> FrozenLadder {
    let floor = root
        .floor
        .map_or(REPAIR_FLOOR, |floor| floor.max(REPAIR_FLOOR));
    let rungs: Vec<_> = root
        .rungs
        .iter()
        .filter(|rung| rung.tier >= floor)
        .cloned()
        .collect();
    if rungs.is_empty() {
        return FrozenLadder {
            tiers: Vec::new(),
            attempts_per: root.attempts_per,
            rungs: Vec::new(),
            floor: Some(floor),
            ceiling: None,
            effort: root.effort,
            admission: Admission::HumanBinding {
                options: allowed_agents.to_vec(),
            },
        };
    }
    let tiers: Vec<Tier> = rungs.iter().map(|rung| rung.tier).collect();
    let ceiling = tiers.iter().copied().max();
    FrozenLadder {
        tiers,
        attempts_per: root.attempts_per,
        rungs,
        floor: Some(floor),
        ceiling,
        effort: root.effort,
        admission: Admission::Runnable,
    }
}

/// The repair's admission: `HumanBinding` when the ladder is (the empty
/// intersection), `HumanRequired` once the lineage is at its automatic-repair
/// limit, else `Runnable`. The empty intersection wins, because without a
/// binding nothing runs whatever the limit says — and the fold refuses
/// `HumanRequired` on a `HumanBinding` ladder, so it is the only admissible
/// shape for an over-limit rejection with no tier left.
fn admission_for(
    entry: &TaskEntry,
    members: u32,
    limit: u32,
    ids: &dyn IdSource,
    key: TaskKey,
) -> SpawnAdmission {
    if let Admission::HumanBinding { options } = &entry.ladder.admission {
        return SpawnAdmission::HumanBinding {
            options: options.clone(),
            question: question(
                ids,
                key,
                QuestionKind::Unblock,
                "the merge repair's tier floor of mid intersected empty with the task's frozen \
                 pin and ceiling; a person must name an agent to run it, or decline the lineage",
                options.clone(),
            ),
        };
    }
    if members >= limit {
        return SpawnAdmission::HumanRequired {
            limit,
            question: question(
                ids,
                key,
                QuestionKind::Continue,
                &format!(
                    "this lineage has consumed its {limit} automatic repair(s); a person must \
                     approve another attempt with the latest evidence, or decline the lineage"
                ),
                crate::engine::coordinator::question_options(QuestionKind::Continue),
            ),
        };
    }
    SpawnAdmission::Runnable
}

fn question(
    ids: &dyn IdSource,
    key: TaskKey,
    kind: QuestionKind,
    context: &str,
    options: Vec<String>,
) -> FrozenQuestion {
    FrozenQuestion {
        id: ids.question_id(),
        key,
        kind,
        context: context.to_owned(),
        options,
    }
}

/// The repair's path hints: the root's hints, the candidate's actual changed
/// paths, and the contended (conflict or rejection) paths, deduplicated in
/// that order.
///
/// A `RepoWide` region contributes no hint — the lineage lease already holds
/// everything a repo-wide region would, so widening the hints by it says
/// nothing the lease does not.
fn expand_hints(base: &[String], actual: &PathSet, contended: &PathSet) -> Vec<String> {
    let mut hints: Vec<String> = base.to_vec();
    for region in [actual, contended] {
        if let Some(paths) = region.prefixes() {
            for path in paths {
                let hint = path.as_str().to_owned();
                if !hints.contains(&hint) {
                    hints.push(hint);
                }
            }
        }
    }
    hints
}

fn refused(message: &str) -> UpstrokeError {
    UpstrokeError::Refused {
        message: message.to_owned(),
    }
}

/// The verification record a code rejection carries, from a judgement's gate
/// and review results.
///
/// `VerificationRecord.verdict` is `GatesFailed` when a gate refused,
/// `Rejected` when a reviewer did; `gates_passed` is whether every gate
/// passed; `reviews` are the pass records; `detail` is the failure's reason.
#[must_use]
pub fn code_rejection_record(
    gates_passed: bool,
    reviews: Vec<crate::events::ReviewRecord>,
    detail: String,
) -> VerificationRecord {
    VerificationRecord {
        verdict: if gates_passed {
            crate::topology::events::VerificationVerdict::Rejected
        } else {
            crate::topology::events::VerificationVerdict::GatesFailed
        },
        gates_passed,
        reviews,
        detail,
    }
}

#[cfg(test)]
mod tests;
