//! Extended notes: `docs/internals/engine/topology/coverage.md`

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::topology::effects::{
    EffectSiteId, EntryPhase, EventSite, Evidence, EvidenceLabel, ExpectedResidue, FaultRegistry,
    HookHarness, HookPhase, Host, InjectionMode, Observation, RefSite, RegistryEntry,
    RegistryError, RunDirSite, SnapshotSite, SubEffectPoint, WorktreeSite,
};

pub const OBSERVATIONS_ENV: &str = "UPSTROKE_HOOK_OBSERVATIONS";

pub const REGISTRY_JSON: &str = "effects/sequential-registry.json";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationRecord {
    pub test: String,
    pub observed: Vec<Observation>,
    pub reached: Vec<Observation>,
    pub fast_sequences: Vec<FastSequenceRecord>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FastSequenceRecord {
    pub name: String,
    pub touched: Vec<String>,
}

impl ObservationRecord {
    #[must_use]
    pub fn of(test: &str, harness: &HookHarness) -> Self {
        Self {
            test: test.to_owned(),
            observed: harness.coverage().to_vec(),
            reached: harness.reached().to_vec(),
            fast_sequences: harness
                .fast_sequences()
                .iter()
                .map(|sequence| FastSequenceRecord {
                    name: sequence.name().to_owned(),
                    touched: sequence.touched().iter().map(|site| site.name()).collect(),
                })
                .collect(),
        }
    }

    pub fn merge(&mut self, other: Self) {
        merge_observations(&mut self.observed, other.observed);
        merge_observations(&mut self.reached, other.reached);
        for sequence in other.fast_sequences {
            if !self.fast_sequences.contains(&sequence) {
                self.fast_sequences.push(sequence);
            }
        }
    }

    #[must_use]
    pub fn observed(&self, site: EffectSiteId, phase: HookPhase) -> bool {
        self.observed
            .iter()
            .any(|seen| seen.site == site && seen.phase == phase && seen.count > 0)
    }

    #[must_use]
    pub fn file_name(test: &str) -> String {
        format!("{}.json", test.replace("::", "__"))
    }
}

fn merge_observations(into: &mut Vec<Observation>, from: Vec<Observation>) {
    for seen in from {
        match into
            .iter_mut()
            .find(|held| held.site == seen.site && held.phase == seen.phase)
        {
            Some(held) => held.count = held.count.max(seen.count),
            None => into.push(seen),
        }
    }
}

pub fn load_observations(dir: &Path) -> Result<Vec<ObservationRecord>, String> {
    let entries = std::fs::read_dir(dir)
        .map_err(|error| format!("cannot read `{}`: {error}", dir.display()))?;
    let mut records: Vec<ObservationRecord> = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|error| format!("cannot list `{}`: {error}", dir.display()))?
            .path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        let bytes = std::fs::read(&path)
            .map_err(|error| format!("cannot read `{}`: {error}", path.display()))?;
        let record: ObservationRecord = serde_json::from_slice(&bytes).map_err(|error| {
            format!("`{}` is not an observation record: {error}", path.display())
        })?;
        match records.iter_mut().find(|held| held.test == record.test) {
            Some(held) => held.merge(record),
            None => records.push(record),
        }
    }
    records.sort_by(|a, b| a.test.cmp(&b.test));
    Ok(records)
}

#[must_use]
pub fn harness_from(records: &[ObservationRecord]) -> HookHarness {
    let mut harness = HookHarness::new();
    for record in records {
        for seen in &record.reached {
            if let HookPhase::Point { .. } = seen.phase {
                harness.hook(seen.site, seen.phase);
            }
        }
        for seen in &record.observed {
            match seen.phase {
                HookPhase::Before | HookPhase::After => {
                    harness.hook(seen.site, seen.phase);
                }
                HookPhase::Point { point, mode } => {
                    if harness.arm(seen.site, point, mode).is_ok() {
                        harness.hook(seen.site, seen.phase);
                        harness.disarm();
                    }
                }
            }
        }
        for sequence in &record.fast_sequences {
            harness.begin_fast_sequence(&sequence.name);
            for name in &sequence.touched {
                if let Ok(site) = EffectSiteId::from_name(name) {
                    harness.hook(site, HookPhase::Before);
                }
            }
            harness.end_fast_sequence();
        }
    }
    harness
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Claim {
    pub site: EffectSiteId,
    pub phase: EntryPhase,
    pub test: &'static str,
}

impl Claim {
    #[must_use]
    pub fn hook_phase(&self) -> Option<HookPhase> {
        match self.phase {
            EntryPhase::Before => Some(HookPhase::Before),
            EntryPhase::After => Some(HookPhase::After),
            EntryPhase::Point { point, mode } => Some(HookPhase::Point { point, mode }),
            EntryPhase::Residue { .. } | EntryPhase::NoExecution => None,
        }
    }
}

#[must_use]
pub fn hook_entry(claim: &Claim) -> RegistryEntry {
    let semantics = claim.site.semantics(claim.phase);
    RegistryEntry {
        site: claim.site,
        phase: claim.phase,
        order: claim.site.observable_orders().first().copied(),
        fault_row: claim.site.fault_row(),
        expected_residue: ExpectedResidue {
            rows: semantics.rows,
            detail: semantics.artifact.detail().to_owned(),
        },
        resume_action: semantics.action.text().to_owned(),
        label: EvidenceLabel::ExecutionObserved,
        evidence: Evidence::Executed {
            test: claim.test.to_owned(),
            passed: true,
        },
    }
}

#[must_use]
pub fn required_phases(site: EffectSiteId, host: Host) -> Vec<EntryPhase> {
    let mut phases = vec![EntryPhase::Before, EntryPhase::After];
    for point in site.sub_effects() {
        if !point.platform().required_on(host) {
            continue;
        }
        for mode in point.modes() {
            phases.push(EntryPhase::Point {
                point: *point,
                mode: *mode,
            });
        }
    }
    phases
}

pub fn registry_of(claims: &[Claim]) -> Result<FaultRegistry, RegistryError> {
    let mut registry = FaultRegistry::new();
    for claim in claims {
        registry.insert(hook_entry(claim))?;
    }
    Ok(registry)
}

pub fn frozen_sampling_n(declarations: &str, site: EffectSiteId) -> Result<Option<u32>, String> {
    let document: serde_json::Value = serde_json::from_str(declarations)
        .map_err(|error| format!("the residue declarations do not parse: {error}"))?;
    let name = site.name();
    let Some(sites) = document.get("sites").and_then(serde_json::Value::as_array) else {
        return Err("the residue declarations carry no `sites`".to_owned());
    };
    for declared in sites {
        if declared.get("site").and_then(serde_json::Value::as_str) == Some(name.as_str()) {
            let n = declared
                .get("sampling_n")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| format!("`{name}` declares no `sampling_n`"))?;
            return u32::try_from(n)
                .map(Some)
                .map_err(|_| format!("`{name}`'s `sampling_n` does not fit"));
        }
    }
    Ok(None)
}

/// The sites the run's end performs at `max_parallel = 1`: the closure's
/// appends and scrubs, and terminal finalization's report write and cleanup
/// steps. `Lock.Release` is not among them — the run lock goes with the
/// handle, outside the hooked funnels — and no site here registers a residue
/// class, so the document carries no recovery-proven entry; the frozen `N`
/// such an entry would cite is read from `effects/residue-classes.json` by
/// [`check_frozen_n`], which the merge check runs regardless.
#[must_use]
pub fn range() -> Vec<EffectSiteId> {
    vec![
        EffectSiteId::Event(EventSite::Append),
        EffectSiteId::Worktree(WorktreeSite::Remove),
        EffectSiteId::Worktree(WorktreeSite::RemoveIntent),
        EffectSiteId::Snapshot(SnapshotSite::Remove),
        EffectSiteId::Snapshot(SnapshotSite::RemoveIntent),
        EffectSiteId::Worktree(WorktreeSite::RemoveStaging),
        EffectSiteId::Worktree(WorktreeSite::RemoveStagingIntent),
        EffectSiteId::Ref(RefSite::DeletePreparedPin),
        EffectSiteId::Ref(RefSite::DeleteCandidatePin),
        EffectSiteId::Ref(RefSite::DeleteCandidatesRef),
        EffectSiteId::Worktree(WorktreeSite::RemoveExecutionRoot),
        EffectSiteId::RunDir(RunDirSite::WriteReport),
    ]
}

/// The evidence: for every phase and point of every site in [`range`], the
/// committed test that executes it, chosen from the suite's own observation
/// export. A claim here is a statement the merge check holds against a
/// fresh export; the non-ignored tests hold it against the tree.
pub const CLAIMS: &[Claim] = &[
    Claim {
        site: EffectSiteId::Event(EventSite::Append),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::kill_after_run_finished_before_report",
    },
    Claim {
        site: EffectSiteId::Event(EventSite::Append),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::kill_after_run_finished_before_report",
    },
    Claim {
        site: EffectSiteId::Event(EventSite::Append),
        phase: EntryPhase::Point {
            point: SubEffectPoint::Written,
            mode: InjectionMode::Kill,
        },
        test: "engine::topology::recover::tests::closure_kill_child",
    },
    Claim {
        site: EffectSiteId::Event(EventSite::Append),
        phase: EntryPhase::Point {
            point: SubEffectPoint::Written,
            mode: InjectionMode::ErrorReturn,
        },
        test: "engine::topology::recover::tests::append_error_inside_closure_ends_command_and_resume_completes_closure",
    },
    Claim {
        site: EffectSiteId::Event(EventSite::Append),
        phase: EntryPhase::Point {
            point: SubEffectPoint::WrittenFull,
            mode: InjectionMode::ErrorReturn,
        },
        test: "engine::topology::emit::tests::append_error_with_failed_prefix_sync_reports_undetermined_without_effects",
    },
    Claim {
        site: EffectSiteId::Event(EventSite::Append),
        phase: EntryPhase::Point {
            point: SubEffectPoint::Synced,
            mode: InjectionMode::Kill,
        },
        test: "engine::topology::settle::tests::settlement_kill_child",
    },
    Claim {
        site: EffectSiteId::Event(EventSite::Append),
        phase: EntryPhase::Point {
            point: SubEffectPoint::Synced,
            mode: InjectionMode::ErrorReturn,
        },
        test: "engine::topology::emit::tests::a_refusal_before_the_append_was_entered_does_not_run_the_protocol",
    },
    Claim {
        site: EffectSiteId::Worktree(WorktreeSite::Remove),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::kill_after_report_before_each_cleanup_step",
    },
    Claim {
        site: EffectSiteId::Worktree(WorktreeSite::Remove),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::kill_after_report_before_each_cleanup_step",
    },
    Claim {
        site: EffectSiteId::Worktree(WorktreeSite::RemoveIntent),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::kill_after_report_before_each_cleanup_step",
    },
    Claim {
        site: EffectSiteId::Worktree(WorktreeSite::RemoveIntent),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::kill_after_report_before_each_cleanup_step",
    },
    Claim {
        site: EffectSiteId::Snapshot(SnapshotSite::Remove),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::kill_after_report_before_each_cleanup_step",
    },
    Claim {
        site: EffectSiteId::Snapshot(SnapshotSite::Remove),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::kill_after_report_before_each_cleanup_step",
    },
    Claim {
        site: EffectSiteId::Snapshot(SnapshotSite::RemoveIntent),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::kill_after_report_before_each_cleanup_step",
    },
    Claim {
        site: EffectSiteId::Snapshot(SnapshotSite::RemoveIntent),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::kill_after_report_before_each_cleanup_step",
    },
    Claim {
        site: EffectSiteId::Worktree(WorktreeSite::RemoveStaging),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::kill_after_report_before_each_cleanup_step",
    },
    Claim {
        site: EffectSiteId::Worktree(WorktreeSite::RemoveStaging),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::kill_after_report_before_each_cleanup_step",
    },
    Claim {
        site: EffectSiteId::Worktree(WorktreeSite::RemoveStagingIntent),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::kill_after_report_before_each_cleanup_step",
    },
    Claim {
        site: EffectSiteId::Worktree(WorktreeSite::RemoveStagingIntent),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::kill_after_report_before_each_cleanup_step",
    },
    Claim {
        site: EffectSiteId::Ref(RefSite::DeletePreparedPin),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::kill_after_report_before_each_cleanup_step",
    },
    Claim {
        site: EffectSiteId::Ref(RefSite::DeletePreparedPin),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::kill_after_report_before_each_cleanup_step",
    },
    Claim {
        site: EffectSiteId::Ref(RefSite::DeleteCandidatePin),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::kill_after_report_before_each_cleanup_step",
    },
    Claim {
        site: EffectSiteId::Ref(RefSite::DeleteCandidatePin),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::kill_after_report_before_each_cleanup_step",
    },
    Claim {
        site: EffectSiteId::Ref(RefSite::DeleteCandidatesRef),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::kill_after_report_before_each_cleanup_step",
    },
    Claim {
        site: EffectSiteId::Ref(RefSite::DeleteCandidatesRef),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::kill_after_report_before_each_cleanup_step",
    },
    Claim {
        site: EffectSiteId::Worktree(WorktreeSite::RemoveExecutionRoot),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::kill_after_report_before_each_cleanup_step",
    },
    Claim {
        site: EffectSiteId::Worktree(WorktreeSite::RemoveExecutionRoot),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::kill_after_report_before_each_cleanup_step",
    },
    Claim {
        site: EffectSiteId::RunDir(RunDirSite::WriteReport),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::kill_after_report_before_each_cleanup_step",
    },
    Claim {
        site: EffectSiteId::RunDir(RunDirSite::WriteReport),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::kill_after_report_before_each_cleanup_step",
    },
];

pub fn registry() -> Result<FaultRegistry, RegistryError> {
    registry_of(CLAIMS)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryDocument {
    pub note: String,
    pub range: Vec<String>,
    pub hosts: Vec<String>,
    pub entries: Vec<RegistryEntry>,
}

pub fn registry_document() -> Result<RegistryDocument, RegistryError> {
    Ok(RegistryDocument {
        note: "decisions.fault_injection_registry, ST-07 for the sequential run-end range: one \
               entry per site x hook phase x parent-side point x injection mode, built through \
               FaultRegistry::insert, each naming the committed test the suite's observation \
               export shows executing it. Every point in the range is platform-independent, so \
               one document serves both hosts. No site in the range registers a residue class; \
               the frozen sampling N a recovery-proven entry cites is effects/residue-classes.json's \
               (SWEEP-BIJECTION-005)."
            .to_owned(),
        range: range().into_iter().map(EffectSiteId::name).collect(),
        hosts: Host::ALL.iter().map(|host| host.name().to_owned()).collect(),
        entries: registry()?.entries().to_vec(),
    })
}

pub fn registry_json() -> Result<String, String> {
    let document = registry_document().map_err(|error| error.to_string())?;
    serde_json::to_string_pretty(&document)
        .map(|json| format!("{json}\n"))
        .map_err(|error| error.to_string())
}

/// SWEEP-BIJECTION-005: the frozen `N` a recovery-proven entry cites is the
/// declarations file's, not the entry's own. One line per disagreement.
#[must_use]
pub fn check_frozen_n(entries: &[RegistryEntry], declarations: &str) -> Vec<String> {
    let mut problems = Vec::new();
    for entry in entries {
        let Evidence::RecoveryProven { sampling, .. } = &entry.evidence else {
            continue;
        };
        match frozen_sampling_n(declarations, entry.site) {
            Ok(Some(frozen)) if frozen == sampling.n => {}
            Ok(Some(frozen)) => problems.push(format!(
                "`{}`'s recovery-proven entry cites n = {}, and the declarations freeze {frozen}",
                entry.site.name(),
                sampling.n
            )),
            Ok(None) => problems.push(format!(
                "`{}` carries a recovery-proven entry and the declarations freeze no N for it",
                entry.site.name()
            )),
            Err(error) => problems.push(error),
        }
    }
    problems
}

#[cfg(test)]
mod tests;
