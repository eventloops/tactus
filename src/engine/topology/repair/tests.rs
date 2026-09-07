//! Extended notes: `docs/internals/engine/topology/repair/tests.md`

use crate::engine::topology::scaffold::{ALPHA, Run};
use crate::ir::Tier;
use crate::topology::events::{RejectionLeaseEffect, SequenceId, SpawnAdmission};
use crate::topology::paths::PathSet;
use crate::topology::registry::{Admission, FrozenLadder, FrozenRung, Origin};

use super::{admission_for, merge_rejected, repair_ladder};

#[test]
fn a_conflict_rejection_registers_a_runnable_repair_that_descends_from_the_candidate() {
    let mut run = Run::started("repair-builder");
    let candidate = run.queue_candidate(ALPHA);
    let ids = crate::engine::topology::seams::RealIds;

    let rejected = merge_rejected(
        run.emitter.fold(),
        &ids,
        &candidate,
        crate::topology::events::CommitSha("f".repeat(40)),
        SequenceId(0),
        crate::topology::events::RejectionDisposition::Conflict {
            paths: region(&["shared.txt"]),
        },
        region(&["shared.txt"]),
    )
    .expect("the rejection builds");

    assert_eq!(rejected.candidate, candidate);
    let repair = &rejected.repair;
    assert_eq!(repair.entry.origin, Origin::MergeRepair);
    assert_eq!(repair.entry.spec.kind, crate::ir::TaskKind::Fix);
    let lineage = repair.entry.lineage.expect("a repair has a lineage");
    assert_eq!(lineage.root, ALPHA);
    assert_eq!(lineage.parent, ALPHA);
    assert_eq!(lineage.index, 0, "the first repair of a root is member 0");
    assert!(matches!(repair.admission, SpawnAdmission::Runnable));
    assert!(matches!(
        rejected.lease_effect,
        RejectionLeaseEffect::CreatesLineage { root, .. } if root == ALPHA
    ));
    assert!(
        repair
            .entry
            .spec
            .path_hints
            .iter()
            .any(|hint| hint == "shared.txt"),
        "the conflict path widens the repair's hints: {:?}",
        repair.entry.spec.path_hints
    );
    assert!(
        repair
            .entry
            .spec
            .acceptance
            .iter()
            .any(|line| line.contains("preserve the behaviour already merged")),
        "the repair carries the preserve-merged-behaviour requirement"
    );
}

fn rung(tier: Tier) -> FrozenRung {
    FrozenRung {
        tier,
        agent: format!("root-{tier}-agent"),
        model: format!("root-{tier}-model"),
        pinned: false,
    }
}

fn root_ladder(tiers: &[Tier], floor: Option<Tier>) -> FrozenLadder {
    FrozenLadder {
        tiers: tiers.to_vec(),
        attempts_per: 2,
        rungs: tiers.iter().copied().map(rung).collect(),
        floor,
        ceiling: tiers.iter().copied().max(),
        effort: crate::ir::ResolvedEffortPolicy {
            small: crate::ir::Effort::Low,
            mid: crate::ir::Effort::High,
            frontier: crate::ir::Effort::Max,
            review: crate::ir::Effort::Medium,
        },
        admission: Admission::Runnable,
    }
}

#[test]
fn the_repair_ladder_is_the_roots_rungs_at_or_above_the_raised_floor() {
    let ladder = repair_ladder(
        &root_ladder(&[Tier::Small, Tier::Mid, Tier::Frontier], Some(Tier::Small)),
        &["claude-code".to_owned()],
    );
    assert_eq!(ladder.tiers, vec![Tier::Mid, Tier::Frontier]);
    assert_eq!(
        ladder.rungs,
        vec![rung(Tier::Mid), rung(Tier::Frontier)],
        "the root's own rungs at the surviving tiers, in the root's order"
    );
    assert_eq!(ladder.floor, Some(Tier::Mid));
    assert_eq!(ladder.ceiling, Some(Tier::Frontier));
    assert_eq!(ladder.attempts_per, 2);
    assert!(matches!(ladder.admission, Admission::Runnable));

    let frontier_only = repair_ladder(
        &root_ladder(&[Tier::Mid, Tier::Frontier], Some(Tier::Frontier)),
        &["claude-code".to_owned()],
    );
    assert_eq!(frontier_only.floor, Some(Tier::Frontier));
    assert_eq!(frontier_only.tiers, vec![Tier::Frontier]);
}

#[test]
fn an_empty_tier_intersection_registers_a_human_binding_ladder_with_the_allowed_agents() {
    let allowed = vec!["claude-code".to_owned(), "copilot".to_owned()];
    let ladder = repair_ladder(&root_ladder(&[Tier::Small], Some(Tier::Small)), &allowed);
    assert!(
        ladder.tiers.is_empty(),
        "no tier survived: {:?}",
        ladder.tiers
    );
    assert!(ladder.rungs.is_empty());
    assert_eq!(
        ladder.floor,
        Some(Tier::Mid),
        "the floor the repair still has to meet"
    );
    assert_eq!(ladder.ceiling, None, "no ceiling: the maximum of no tier");
    assert_eq!(ladder.attempts_per, 2);
    assert_eq!(
        ladder.admission,
        Admission::HumanBinding {
            options: allowed.clone()
        },
        "the options are the entry's allowed agents"
    );
    assert!(
        !allowed.contains(&"root-small-agent".to_owned()),
        "the excluded rung's agent is not among what is offered"
    );
}

#[test]
fn the_admission_follows_the_ladder_first_and_the_consumed_allowance_second() {
    let mut run = Run::started("repair-admission");
    let candidate = run.queue_candidate(ALPHA);
    let ids = crate::engine::topology::seams::RealIds;
    let rejected = merge_rejected(
        run.emitter.fold(),
        &ids,
        &candidate,
        crate::topology::events::CommitSha("f".repeat(40)),
        SequenceId(0),
        crate::topology::events::RejectionDisposition::Conflict {
            paths: region(&["shared.txt"]),
        },
        region(&["shared.txt"]),
    )
    .expect("the rejection builds");
    let entry = rejected.repair.entry;
    let key = entry.key;

    assert!(matches!(
        admission_for(&entry, 2, 3, &ids, key),
        SpawnAdmission::Runnable
    ));
    let SpawnAdmission::HumanRequired { limit, question } = admission_for(&entry, 3, 3, &ids, key)
    else {
        panic!("a lineage at its limit registers with human admission");
    };
    assert_eq!(limit, 3);
    assert_eq!(question.key, key);
    assert!(question.is_complete());

    let mut waiting = entry;
    waiting.ladder = repair_ladder(
        &root_ladder(&[Tier::Small], Some(Tier::Small)),
        &["claude-code".to_owned()],
    );
    for members in [0, 3] {
        let SpawnAdmission::HumanBinding { options, question } =
            admission_for(&waiting, members, 3, &ids, key)
        else {
            panic!("an empty intersection asks for a binding with {members} member(s)");
        };
        assert_eq!(options, vec!["claude-code".to_owned()]);
        assert_eq!(question.options, options);
        assert_eq!(question.kind, crate::ir::QuestionKind::Unblock);
    }
}

#[test]
fn the_frozen_repair_spec_embeds_the_rejection_evidence_and_both_shas() {
    let mut run = Run::started("repair-spec-evidence");
    let candidate = run.queue_candidate(ALPHA);
    let ids = crate::engine::topology::seams::RealIds;
    let head = crate::topology::events::CommitSha("f".repeat(40));
    let root_body = run
        .emitter
        .fold()
        .registry()
        .expect("a registry")
        .get(ALPHA)
        .expect("alpha is registered")
        .spec
        .body
        .clone();

    let conflict = merge_rejected(
        run.emitter.fold(),
        &ids,
        &candidate,
        head.clone(),
        SequenceId(0),
        crate::topology::events::RejectionDisposition::Conflict {
            paths: region(&["shared.txt"]),
        },
        region(&["shared.txt"]),
    )
    .expect("the conflict rejection builds");
    let spec = serde_json::to_string(&conflict.repair.entry.spec).expect("the spec serializes");
    for needle in [
        candidate.commit_sha.as_str(),
        candidate.candidate_ref.as_str(),
        head.as_str(),
        "shared.txt",
        "conflicted",
        root_body.as_str(),
    ] {
        assert!(
            spec.contains(needle),
            "the frozen spec of a conflict repair embeds `{needle}`: {spec}"
        );
    }

    let evidence = "the gate `clippy` failed: exit 1 (REPAIR-SPEC-UNIQUE-EVIDENCE)";
    let review = crate::events::ReviewRecord {
        pass: "acceptance".to_owned(),
        agent: "claude-code".to_owned(),
        model: "claude-opus-5".to_owned(),
        adapter: None,
        preflight_cli_version: None,
        effort: None,
        pool: None,
        cost_usd: Some(0.5),
        outcome: crate::events::ReviewPassOutcome::Failed,
    };
    let rejected = merge_rejected(
        run.emitter.fold(),
        &ids,
        &candidate,
        head.clone(),
        SequenceId(1),
        crate::topology::events::RejectionDisposition::CodeRejected {
            verification: super::code_rejection_record(false, vec![review], evidence.to_owned()),
        },
        region(&["shared.txt"]),
    )
    .expect("the code rejection builds");
    let spec = serde_json::to_string(&rejected.repair.entry.spec).expect("the spec serializes");
    for needle in [
        candidate.commit_sha.as_str(),
        candidate.candidate_ref.as_str(),
        head.as_str(),
        "REPAIR-SPEC-UNIQUE-EVIDENCE",
        "gates failed",
        "acceptance",
        "claude-opus-5",
        "sequence 1",
        root_body.as_str(),
    ] {
        assert!(
            spec.contains(needle),
            "the frozen spec of a code-rejected repair embeds `{needle}`: {spec}"
        );
    }
    assert_eq!(
        rejected.repair.entry.spec.kind,
        crate::ir::TaskKind::Fix,
        "and it is still a Fix"
    );
}

fn region(paths: &[&str]) -> PathSet {
    PathSet::Prefixes {
        paths: paths
            .iter()
            .map(|p| crate::topology::paths::GitPath::from(*p))
            .collect(),
    }
}
