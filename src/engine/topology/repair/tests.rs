//! Tests for the repair-spawn builder.

use crate::engine::topology::scaffold::{ALPHA, Run};
use crate::topology::events::{RejectionLeaseEffect, SequenceId, SpawnAdmission};
use crate::topology::paths::PathSet;
use crate::topology::registry::Origin;

use super::merge_rejected;

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

fn region(paths: &[&str]) -> PathSet {
    PathSet::Prefixes {
        paths: paths
            .iter()
            .map(|p| crate::topology::paths::GitPath::from(*p))
            .collect(),
    }
}
