//! Extended notes: `docs/internals/engine/topology/repair.md`

use crate::error::UpstrokeError;
use crate::ir::{QuestionKind, Tier};
use crate::topology::events::{
    BindingOverride, CandidateRef, CommitSha, FrozenQuestion, FrozenSpawn, MergeRejected,
    RejectionDisposition, RejectionLeaseEffect, SequenceId, SpawnAdmission, VerificationRecord,
};
use crate::topology::fold::TopologyFold;
use crate::topology::paths::PathSet;
use crate::topology::registry::{
    Admission, FrozenLadder, Lineage, Origin, TaskEntry, TaskKey, repair_display_id,
};

use super::seams::IdSource;

const REPAIR_FLOOR: Tier = Tier::Mid;

const PRESERVE_MERGED: &str = "preserve the behaviour already merged onto the integration head; the rejected candidate's \
     change must integrate without regressing it";

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

    let mut spec = root_entry.spec.clone();
    spec.kind = crate::ir::TaskKind::Fix;
    spec.path_hints = hints;
    spec.acceptance.push(PRESERVE_MERGED.to_owned());
    spec.body = repair_body(
        &root_entry.spec.body,
        candidate,
        &rejecting_head,
        sequence,
        &disposition,
    );

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

fn repair_body(
    root_body: &str,
    candidate: &CandidateRef,
    rejecting_head: &CommitSha,
    sequence: SequenceId,
    disposition: &RejectionDisposition,
) -> String {
    let mut body = String::new();
    body.push_str(root_body);
    body.push_str("\n\n## Merge repair\n\n");
    body.push_str(&format!(
        "Candidate {} ({}) of task {} generation {} was rejected at integration sequence {} \
         against the integration head {}.\n",
        candidate.commit_sha,
        candidate.candidate_ref.as_str(),
        candidate.key,
        candidate.generation.0,
        sequence.0,
        rejecting_head
    ));
    match disposition {
        RejectionDisposition::Conflict { paths } => {
            body.push_str("\nThe cherry-pick onto that head conflicted in: ");
            body.push_str(&render_paths(paths));
            body.push_str(".\n");
        }
        RejectionDisposition::CodeRejected { verification } => {
            body.push_str(&format!(
                "\nThe verification of the proposal on that head ended {} (gates {}): {}\n",
                match verification.verdict {
                    crate::topology::events::VerificationVerdict::Passed => "passed",
                    crate::topology::events::VerificationVerdict::GatesFailed => "gates failed",
                    crate::topology::events::VerificationVerdict::Rejected => "rejected",
                },
                if verification.gates_passed {
                    "passed"
                } else {
                    "failed"
                },
                verification.detail
            ));
            for review in &verification.reviews {
                body.push_str(&format!(
                    "- review pass `{}` by {} ({}): {:?}\n",
                    review.pass, review.agent, review.model, review.outcome
                ));
            }
        }
    }
    body
}

fn render_paths(paths: &PathSet) -> String {
    match paths.prefixes() {
        Some(paths) if !paths.is_empty() => paths
            .iter()
            .map(|path| format!("`{}`", path.as_str()))
            .collect::<Vec<_>>()
            .join(", "),
        _ => "the whole repository".to_owned(),
    }
}

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

pub fn one_off_binding(
    entry: &TaskEntry,
    key: TaskKey,
    question: &crate::ir::QuestionId,
    option_index: u32,
    agent: &str,
) -> Result<BindingOverride, UpstrokeError> {
    let floor = entry.ladder.floor.ok_or_else(|| {
        refused(&format!(
            "task {key} waits for a one-off binding and its ladder records no floor, so there is \
             no tier the binding would run at; nothing was appended"
        ))
    })?;
    let model = catalogued_model(agent, floor).ok_or_else(|| {
        refused(&format!(
            "task {key} was answered with agent `{agent}` and this build's model catalogue knows \
             no `{agent}` model at tier `{floor}` or above, which is the floor its repair ladder \
             froze; nothing was appended"
        ))
    })?;
    Ok(BindingOverride {
        key,
        question: question.clone(),
        option_index,
        agent: agent.to_owned(),
        model,
        effort: entry.ladder.effort.implementation_for(floor),
    })
}

fn catalogued_model(agent: &str, floor: Tier) -> Option<String> {
    crate::catalog::CATALOG
        .iter()
        .filter(|entry| entry.agent == agent && entry.tier >= floor)
        .min_by_key(|entry| entry.tier)
        .map(|entry| entry.model.to_owned())
}

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
