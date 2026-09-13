//! Extended notes: `docs/internals/engine/topology/coverage.md`

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::topology::effects::{
    AnswerSite, BijectionFailure, ContainerSite, EffectSiteId, EntryPhase, EventSite, Evidence,
    EvidenceLabel, ExpectedResidue, FaultRegistry, HookHarness, HookPhase, Host, InjectionMode,
    LockSite, ObjectSite, ProcessSite, RefSite, RegistryEntry, RegistryError, ReportSite,
    RunDirSite, SamplingRecord, SnapshotSite, SubEffectPoint, SyntheticRecord, WorktreeSite,
};

use crate::observations::ObservationRecord;

pub const REGISTRY_JSON: &str = "effects/sequential-registry.json";

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

pub const ADAPTER_UNIT_TEST_MODULES: &[&str] = &[
    "workspace_manager::hooks::tests::",
    "engine::topology::seams::tests::",
    "runner::tests::",
];

#[must_use]
pub fn is_funnel_execution(test: &str) -> bool {
    !ADAPTER_UNIT_TEST_MODULES
        .iter()
        .any(|module| test.starts_with(module))
}

#[must_use]
pub fn inventory() -> Vec<EffectSiteId> {
    EffectSiteId::claimed()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unobservable {
    pub site: EffectSiteId,
    pub phases: &'static [EntryPhase],
    pub reason: &'static str,
}

impl Unobservable {
    #[must_use]
    pub fn covers(&self, site: EffectSiteId, phase: EntryPhase) -> bool {
        self.site == site && (self.phases.is_empty() || self.phases.contains(&phase))
    }
}

const BOTH_HOOK_PHASES: &[EntryPhase] = &[EntryPhase::Before, EntryPhase::After];

#[must_use]
pub fn declared_unobservable() -> Vec<Unobservable> {
    vec![
        Unobservable {
            site: EffectSiteId::Report(ReportSite::Write),
            phases: &[],
            reason: "no funnel names this site: `report.json` is written through \
                     `RunDir.WriteReport` (src/rundir.rs), and `Report.Write` is the one entry \
                     of `SITES_WITHOUT_A_FUNNEL` in src/effects/tests/artifacts.rs \
                     (PR3-REPORT-DOUBLE-NAME), so no hook of it can be called",
        },
        Unobservable {
            site: EffectSiteId::Process(ProcessSite::Spawn),
            phases: BOTH_HOOK_PHASES,
            reason: "the process funnel (src/agent/proc.rs) consults `SpawnHooks::point` at its \
                     parent-side points only; `SpawnHooks` (src/agent/proc/hooks.rs) has no \
                     before/after phase hook, so the two phases have no call to observe",
        },
        Unobservable {
            site: EffectSiteId::Process(ProcessSite::Terminate),
            phases: &[],
            reason: "termination (`kill_tree` in src/agent/proc.rs) consults no hook: the site \
                     is inventoried and row-mapped and `SpawnHooks` exposes nothing for it",
        },
    ]
}

#[must_use]
pub fn checked_inventory() -> Vec<EffectSiteId> {
    let declared = declared_unobservable();
    inventory()
        .into_iter()
        .filter(|site| {
            !declared
                .iter()
                .any(|entry| entry.site == *site && entry.phases.is_empty())
        })
        .collect()
}

#[must_use]
pub fn excused(failure: &BijectionFailure, declared: &[Unobservable]) -> bool {
    let (site, phase) = match failure {
        BijectionFailure::Unobserved { site, phase } => (
            *site,
            match *phase {
                HookPhase::Before => EntryPhase::Before,
                HookPhase::After => EntryPhase::After,
                HookPhase::Point { point, mode } => EntryPhase::Point { point, mode },
            },
        ),
        BijectionFailure::MissingEntry { site, phase, .. } => (*site, *phase),
        _ => return false,
    };
    declared.iter().any(|entry| entry.covers(site, phase))
}

pub const FAST_PATH_TEST: &str = "engine::topology::integrate::tests::fast_path_publishes_exact_candidate_without_staging_or_proposal_object";

pub const FAST_SEQUENCES: &[&str] = &["s0", "exact-base-fast"];

#[must_use]
pub fn no_execution_entries() -> Vec<RegistryEntry> {
    inventory()
        .into_iter()
        .filter(|site| site.skipped_on_fast_path())
        .map(|site| {
            let semantics = site.semantics(EntryPhase::NoExecution);
            RegistryEntry {
                site,
                phase: EntryPhase::NoExecution,
                order: None,
                fault_row: site.fault_row(),
                expected_residue: ExpectedResidue {
                    rows: semantics.rows,
                    detail: semantics.artifact.detail().to_owned(),
                },
                resume_action: semantics.action.text().to_owned(),
                label: EvidenceLabel::ExecutionObserved,
                evidence: Evidence::NotExecuted {
                    test: FAST_PATH_TEST.to_owned(),
                    passed: true,
                    sequences: FAST_SEQUENCES
                        .iter()
                        .map(|sequence| (*sequence).to_owned())
                        .collect(),
                },
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResidueEvidence {
    pub synthetic: Vec<(EffectSiteId, Vec<SyntheticRecord>)>,
    pub sampling: Vec<(EffectSiteId, SamplingRecord)>,
}

#[derive(Debug, Deserialize)]
struct SyntheticFile {
    sites: Vec<SyntheticSite>,
}

#[derive(Debug, Deserialize)]
struct SyntheticSite {
    site: String,
    synthetic: Vec<SyntheticRecord>,
}

#[derive(Debug, Deserialize)]
struct HistogramFile {
    sites: Vec<HistogramSite>,
}

#[derive(Debug, Deserialize)]
struct HistogramSite {
    site: String,
    n: u32,
    none: u32,
    internal: u32,
    after: u32,
    unclassified: u32,
    recovered: bool,
}

impl ResidueEvidence {
    pub fn parse(synthetic_json: &str, histograms: &[&str]) -> Result<Self, String> {
        let synthetic: SyntheticFile = serde_json::from_str(synthetic_json)
            .map_err(|error| format!("the synthetic evidence does not parse: {error}"))?;
        let mut evidence = Self::default();
        for site in synthetic.sites {
            let id = EffectSiteId::from_name(&site.site).map_err(|error| error.to_string())?;
            evidence.synthetic.push((id, site.synthetic));
        }
        for histogram in histograms {
            let parsed: HistogramFile = serde_json::from_str(histogram)
                .map_err(|error| format!("a residue histogram does not parse: {error}"))?;
            for site in parsed.sites {
                let id = EffectSiteId::from_name(&site.site).map_err(|error| error.to_string())?;
                if evidence.sampling.iter().any(|(held, _)| *held == id) {
                    return Err(format!("`{}` is sampled by two histograms", site.site));
                }
                evidence.sampling.push((
                    id,
                    SamplingRecord {
                        n: site.n,
                        histogram: crate::topology::effects::ClassHistogram {
                            none: site.none,
                            internal: site.internal,
                            after: site.after,
                        },
                        unclassified: site.unclassified,
                        recovered: site.recovered,
                    },
                ));
            }
        }
        Ok(evidence)
    }

    fn synthetic_of(&self, site: EffectSiteId) -> Option<&[SyntheticRecord]> {
        self.synthetic
            .iter()
            .find(|(held, _)| *held == site)
            .map(|(_, records)| records.as_slice())
    }

    fn sampling_of(&self, site: EffectSiteId) -> Option<SamplingRecord> {
        self.sampling
            .iter()
            .find(|(held, _)| *held == site)
            .map(|(_, record)| *record)
    }
}

pub fn residue_entries(evidence: &ResidueEvidence) -> Result<Vec<RegistryEntry>, String> {
    let mut entries = Vec::new();
    for site in inventory() {
        for class in site.residue_classes() {
            let phase = EntryPhase::Residue { class: *class };
            let semantics = site.semantics(phase);
            let synthetic = evidence
                .synthetic_of(site)
                .ok_or_else(|| format!("`{site}` has no synthetic evidence"))?
                .to_vec();
            let sampling = evidence
                .sampling_of(site)
                .ok_or_else(|| format!("`{site}` has no sampling record"))?;
            entries.push(RegistryEntry {
                site,
                phase,
                order: site.observable_orders().first().copied(),
                fault_row: site.fault_row(),
                expected_residue: ExpectedResidue {
                    rows: semantics.rows,
                    detail: semantics.artifact.detail().to_owned(),
                },
                resume_action: semantics.action.text().to_owned(),
                label: EvidenceLabel::RecoveryProven,
                evidence: Evidence::RecoveryProven {
                    synthetic,
                    sampling,
                },
            });
        }
    }
    Ok(entries)
}

pub const CLAIMS: &[Claim] = &[
    Claim {
        site: EffectSiteId::Worktree(WorktreeSite::CreateExecutionRoot),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_budget_stopped_run_with_a_retained_generation_is_closed_run_ending_and_ends",
    },
    Claim {
        site: EffectSiteId::Worktree(WorktreeSite::CreateExecutionRoot),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_budget_stopped_run_with_a_retained_generation_is_closed_run_ending_and_ends",
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
        site: EffectSiteId::Worktree(WorktreeSite::WriteIntent),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_budget_stopped_run_with_a_retained_generation_is_closed_run_ending_and_ends",
    },
    Claim {
        site: EffectSiteId::Worktree(WorktreeSite::WriteIntent),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_budget_stopped_run_with_a_retained_generation_is_closed_run_ending_and_ends",
    },
    Claim {
        site: EffectSiteId::Worktree(WorktreeSite::Add),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_budget_stopped_run_with_a_retained_generation_is_closed_run_ending_and_ends",
    },
    Claim {
        site: EffectSiteId::Worktree(WorktreeSite::Add),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_budget_stopped_run_with_a_retained_generation_is_closed_run_ending_and_ends",
    },
    Claim {
        site: EffectSiteId::Worktree(WorktreeSite::Verify),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_dispatch_recorded_before_this_rule_resumes_at_the_base_it_recorded",
    },
    Claim {
        site: EffectSiteId::Worktree(WorktreeSite::Verify),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_dispatch_recorded_before_this_rule_resumes_at_the_base_it_recorded",
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
        site: EffectSiteId::Worktree(WorktreeSite::WriteStagingIntent),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_binding_answer_naming_no_frozen_option_is_refused_before_any_append",
    },
    Claim {
        site: EffectSiteId::Worktree(WorktreeSite::WriteStagingIntent),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_binding_answer_naming_no_frozen_option_is_refused_before_any_append",
    },
    Claim {
        site: EffectSiteId::Worktree(WorktreeSite::AddStaging),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_binding_answer_naming_no_frozen_option_is_refused_before_any_append",
    },
    Claim {
        site: EffectSiteId::Worktree(WorktreeSite::AddStaging),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_binding_answer_naming_no_frozen_option_is_refused_before_any_append",
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
        site: EffectSiteId::Snapshot(SnapshotSite::WriteIntent),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_fresh_incarnation_closes_a_retained_repair_generation_lineage_held_and_the_next_materializes_again",
    },
    Claim {
        site: EffectSiteId::Snapshot(SnapshotSite::WriteIntent),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_fresh_incarnation_closes_a_retained_repair_generation_lineage_held_and_the_next_materializes_again",
    },
    Claim {
        site: EffectSiteId::Snapshot(SnapshotSite::Add),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_fresh_incarnation_closes_a_retained_repair_generation_lineage_held_and_the_next_materializes_again",
    },
    Claim {
        site: EffectSiteId::Snapshot(SnapshotSite::Add),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_fresh_incarnation_closes_a_retained_repair_generation_lineage_held_and_the_next_materializes_again",
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
        site: EffectSiteId::Ref(RefSite::CreateIntegration),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_budget_stopped_run_with_a_retained_generation_is_closed_run_ending_and_ends",
    },
    Claim {
        site: EffectSiteId::Ref(RefSite::CreateIntegration),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_budget_stopped_run_with_a_retained_generation_is_closed_run_ending_and_ends",
    },
    Claim {
        site: EffectSiteId::Ref(RefSite::CompareAndSwapIntegration),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_dependent_task_is_dispatched_into_its_dependencys_merged_work",
    },
    Claim {
        site: EffectSiteId::Ref(RefSite::CompareAndSwapIntegration),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_dependent_task_is_dispatched_into_its_dependencys_merged_work",
    },
    Claim {
        site: EffectSiteId::Ref(RefSite::CreateCandidates),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_fresh_incarnation_closes_a_retained_repair_generation_lineage_held_and_the_next_materializes_again",
    },
    Claim {
        site: EffectSiteId::Ref(RefSite::CreateCandidates),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_fresh_incarnation_closes_a_retained_repair_generation_lineage_held_and_the_next_materializes_again",
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
        site: EffectSiteId::Ref(RefSite::PinCandidatePrepared),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_fresh_incarnation_closes_a_retained_repair_generation_lineage_held_and_the_next_materializes_again",
    },
    Claim {
        site: EffectSiteId::Ref(RefSite::PinCandidatePrepared),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_fresh_incarnation_closes_a_retained_repair_generation_lineage_held_and_the_next_materializes_again",
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
        site: EffectSiteId::Ref(RefSite::PinPrepared),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_gate_spawn_failure_during_integration_verification_defers_inside_max_defers",
    },
    Claim {
        site: EffectSiteId::Ref(RefSite::PinPrepared),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_gate_spawn_failure_during_integration_verification_defers_inside_max_defers",
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
        site: EffectSiteId::Object(ObjectSite::CandidateStage),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_budget_stopped_run_with_a_retained_generation_is_closed_run_ending_and_ends",
    },
    Claim {
        site: EffectSiteId::Object(ObjectSite::CandidateStage),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_budget_stopped_run_with_a_retained_generation_is_closed_run_ending_and_ends",
    },
    Claim {
        site: EffectSiteId::Object(ObjectSite::CandidateWriteTree),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_budget_stopped_run_with_a_retained_generation_is_closed_run_ending_and_ends",
    },
    Claim {
        site: EffectSiteId::Object(ObjectSite::CandidateWriteTree),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_budget_stopped_run_with_a_retained_generation_is_closed_run_ending_and_ends",
    },
    Claim {
        site: EffectSiteId::Object(ObjectSite::SnapshotCommitTree),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_fresh_incarnation_closes_a_retained_repair_generation_lineage_held_and_the_next_materializes_again",
    },
    Claim {
        site: EffectSiteId::Object(ObjectSite::SnapshotCommitTree),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_fresh_incarnation_closes_a_retained_repair_generation_lineage_held_and_the_next_materializes_again",
    },
    Claim {
        site: EffectSiteId::Object(ObjectSite::SnapshotCommitTree),
        phase: EntryPhase::Point {
            point: SubEffectPoint::IdUnread,
            mode: InjectionMode::Kill,
        },
        test: "engine::topology::attempt::tests::attempt_kill_child",
    },
    Claim {
        site: EffectSiteId::Object(ObjectSite::CandidateCommitTree),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_fresh_incarnation_closes_a_retained_repair_generation_lineage_held_and_the_next_materializes_again",
    },
    Claim {
        site: EffectSiteId::Object(ObjectSite::CandidateCommitTree),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_fresh_incarnation_closes_a_retained_repair_generation_lineage_held_and_the_next_materializes_again",
    },
    Claim {
        site: EffectSiteId::Object(ObjectSite::CandidateCommitTree),
        phase: EntryPhase::Point {
            point: SubEffectPoint::IdUnread,
            mode: InjectionMode::Kill,
        },
        test: "engine::topology::candidate::tests::candidate_kill_child",
    },
    Claim {
        site: EffectSiteId::Object(ObjectSite::ProposalCherryPick),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_gate_spawn_failure_during_integration_verification_defers_inside_max_defers",
    },
    Claim {
        site: EffectSiteId::Object(ObjectSite::ProposalCherryPick),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_gate_spawn_failure_during_integration_verification_defers_inside_max_defers",
    },
    Claim {
        site: EffectSiteId::Object(ObjectSite::RepairMaterialize),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_fresh_incarnation_closes_a_retained_repair_generation_lineage_held_and_the_next_materializes_again",
    },
    Claim {
        site: EffectSiteId::Object(ObjectSite::RepairMaterialize),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_fresh_incarnation_closes_a_retained_repair_generation_lineage_held_and_the_next_materializes_again",
    },
    Claim {
        site: EffectSiteId::RunDir(RunDirSite::CreatePublicDir),
        phase: EntryPhase::Before,
        test: "engine::topology::create::tests::a_created_run_hands_itself_to_the_loop",
    },
    Claim {
        site: EffectSiteId::RunDir(RunDirSite::CreatePublicDir),
        phase: EntryPhase::After,
        test: "engine::topology::create::tests::a_created_run_hands_itself_to_the_loop",
    },
    Claim {
        site: EffectSiteId::RunDir(RunDirSite::StageMarker),
        phase: EntryPhase::Before,
        test: "engine::topology::create::tests::a_created_run_hands_itself_to_the_loop",
    },
    Claim {
        site: EffectSiteId::RunDir(RunDirSite::StageMarker),
        phase: EntryPhase::After,
        test: "engine::topology::create::tests::a_created_run_hands_itself_to_the_loop",
    },
    Claim {
        site: EffectSiteId::RunDir(RunDirSite::PublishMarker),
        phase: EntryPhase::Before,
        test: "engine::topology::create::tests::a_created_run_hands_itself_to_the_loop",
    },
    Claim {
        site: EffectSiteId::RunDir(RunDirSite::PublishMarker),
        phase: EntryPhase::After,
        test: "engine::topology::create::tests::a_created_run_hands_itself_to_the_loop",
    },
    Claim {
        site: EffectSiteId::RunDir(RunDirSite::RemoveMarker),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_binding_answer_naming_no_frozen_option_is_refused_before_any_append",
    },
    Claim {
        site: EffectSiteId::RunDir(RunDirSite::RemoveMarker),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_binding_answer_naming_no_frozen_option_is_refused_before_any_append",
    },
    Claim {
        site: EffectSiteId::RunDir(RunDirSite::CreatePrivateDir),
        phase: EntryPhase::Before,
        test: "engine::topology::create::tests::a_created_run_hands_itself_to_the_loop",
    },
    Claim {
        site: EffectSiteId::RunDir(RunDirSite::CreatePrivateDir),
        phase: EntryPhase::After,
        test: "engine::topology::create::tests::a_created_run_hands_itself_to_the_loop",
    },
    Claim {
        site: EffectSiteId::RunDir(RunDirSite::StageOwnerRecord),
        phase: EntryPhase::Before,
        test: "engine::topology::create::tests::a_created_run_hands_itself_to_the_loop",
    },
    Claim {
        site: EffectSiteId::RunDir(RunDirSite::StageOwnerRecord),
        phase: EntryPhase::After,
        test: "engine::topology::create::tests::a_created_run_hands_itself_to_the_loop",
    },
    Claim {
        site: EffectSiteId::RunDir(RunDirSite::PublishOwnerRecord),
        phase: EntryPhase::Before,
        test: "engine::topology::create::tests::a_created_run_hands_itself_to_the_loop",
    },
    Claim {
        site: EffectSiteId::RunDir(RunDirSite::PublishOwnerRecord),
        phase: EntryPhase::After,
        test: "engine::topology::create::tests::a_created_run_hands_itself_to_the_loop",
    },
    Claim {
        site: EffectSiteId::RunDir(RunDirSite::StageCommitRecord),
        phase: EntryPhase::Before,
        test: "engine::topology::create::tests::a_created_run_hands_itself_to_the_loop",
    },
    Claim {
        site: EffectSiteId::RunDir(RunDirSite::StageCommitRecord),
        phase: EntryPhase::After,
        test: "engine::topology::create::tests::a_created_run_hands_itself_to_the_loop",
    },
    Claim {
        site: EffectSiteId::RunDir(RunDirSite::PublishCommitRecord),
        phase: EntryPhase::Before,
        test: "engine::topology::create::tests::a_created_run_hands_itself_to_the_loop",
    },
    Claim {
        site: EffectSiteId::RunDir(RunDirSite::PublishCommitRecord),
        phase: EntryPhase::After,
        test: "engine::topology::create::tests::a_created_run_hands_itself_to_the_loop",
    },
    Claim {
        site: EffectSiteId::RunDir(RunDirSite::WritePlan),
        phase: EntryPhase::Before,
        test: "engine::topology::create::tests::a_created_run_hands_itself_to_the_loop",
    },
    Claim {
        site: EffectSiteId::RunDir(RunDirSite::WritePlan),
        phase: EntryPhase::After,
        test: "engine::topology::create::tests::a_created_run_hands_itself_to_the_loop",
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
    Claim {
        site: EffectSiteId::RunDir(RunDirSite::WriteQuestionPayload),
        phase: EntryPhase::Before,
        test: "engine::topology::coverage::tests::the_question_and_answer_funnels_execute_both_phases_under_the_production_adapter",
    },
    Claim {
        site: EffectSiteId::RunDir(RunDirSite::WriteQuestionPayload),
        phase: EntryPhase::After,
        test: "engine::topology::coverage::tests::the_question_and_answer_funnels_execute_both_phases_under_the_production_adapter",
    },
    Claim {
        site: EffectSiteId::RunDir(RunDirSite::RemovePrivateHusk),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::kill_during_recovery_repeats_recovery",
    },
    Claim {
        site: EffectSiteId::RunDir(RunDirSite::RemovePrivateHusk),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::kill_during_recovery_repeats_recovery",
    },
    Claim {
        site: EffectSiteId::RunDir(RunDirSite::RemovePublicHusk),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::kill_during_recovery_repeats_recovery",
    },
    Claim {
        site: EffectSiteId::RunDir(RunDirSite::RemovePublicHusk),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::kill_during_recovery_repeats_recovery",
    },
    Claim {
        site: EffectSiteId::Event(EventSite::OpenLog),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_binding_answer_naming_no_frozen_option_is_refused_before_any_append",
    },
    Claim {
        site: EffectSiteId::Event(EventSite::OpenLog),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_binding_answer_naming_no_frozen_option_is_refused_before_any_append",
    },
    Claim {
        site: EffectSiteId::Event(EventSite::OpenLog),
        phase: EntryPhase::Point {
            point: SubEffectPoint::Create,
            mode: InjectionMode::Kill,
        },
        test: "engine::topology::coverage::tests::event_kill_child",
    },
    Claim {
        site: EffectSiteId::Event(EventSite::OpenLog),
        phase: EntryPhase::Point {
            point: SubEffectPoint::Create,
            mode: InjectionMode::ErrorReturn,
        },
        test: "engine::topology::coverage::tests::every_error_return_point_of_the_event_funnels_fires_under_the_production_adapter",
    },
    Claim {
        site: EffectSiteId::Event(EventSite::OpenLog),
        phase: EntryPhase::Point {
            point: SubEffectPoint::TruncateTornTail,
            mode: InjectionMode::Kill,
        },
        test: "engine::topology::coverage::tests::event_kill_child",
    },
    Claim {
        site: EffectSiteId::Event(EventSite::OpenLog),
        phase: EntryPhase::Point {
            point: SubEffectPoint::TruncateTornTail,
            mode: InjectionMode::ErrorReturn,
        },
        test: "engine::topology::coverage::tests::every_error_return_point_of_the_event_funnels_fires_under_the_production_adapter",
    },
    Claim {
        site: EffectSiteId::Event(EventSite::OpenLog),
        phase: EntryPhase::Point {
            point: SubEffectPoint::SyncPrefix,
            mode: InjectionMode::Kill,
        },
        test: "engine::topology::coverage::tests::event_kill_child",
    },
    Claim {
        site: EffectSiteId::Event(EventSite::OpenLog),
        phase: EntryPhase::Point {
            point: SubEffectPoint::SyncPrefix,
            mode: InjectionMode::ErrorReturn,
        },
        test: "engine::topology::recover::tests::barrier_sync_failure_before_cas_issues_no_cas_and_converges_after_loss",
    },
    Claim {
        site: EffectSiteId::Event(EventSite::ProvePrefixStable),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_binding_answer_naming_no_frozen_option_is_refused_before_any_append",
    },
    Claim {
        site: EffectSiteId::Event(EventSite::ProvePrefixStable),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_binding_answer_naming_no_frozen_option_is_refused_before_any_append",
    },
    Claim {
        site: EffectSiteId::Event(EventSite::AppendFirst),
        phase: EntryPhase::Before,
        test: "engine::topology::attempt::tests::a_case_alias_is_read_per_character_so_a_final_sigma_hides_no_contradiction",
    },
    Claim {
        site: EffectSiteId::Event(EventSite::AppendFirst),
        phase: EntryPhase::After,
        test: "engine::topology::attempt::tests::a_case_alias_is_read_per_character_so_a_final_sigma_hides_no_contradiction",
    },
    Claim {
        site: EffectSiteId::Event(EventSite::AppendFirst),
        phase: EntryPhase::Point {
            point: SubEffectPoint::Written,
            mode: InjectionMode::Kill,
        },
        test: "engine::topology::coverage::tests::event_kill_child",
    },
    Claim {
        site: EffectSiteId::Event(EventSite::AppendFirst),
        phase: EntryPhase::Point {
            point: SubEffectPoint::Written,
            mode: InjectionMode::ErrorReturn,
        },
        test: "engine::topology::coverage::tests::every_error_return_point_of_the_event_funnels_fires_under_the_production_adapter",
    },
    Claim {
        site: EffectSiteId::Event(EventSite::AppendFirst),
        phase: EntryPhase::Point {
            point: SubEffectPoint::WrittenFull,
            mode: InjectionMode::ErrorReturn,
        },
        test: "engine::topology::coverage::tests::every_error_return_point_of_the_event_funnels_fires_under_the_production_adapter",
    },
    Claim {
        site: EffectSiteId::Event(EventSite::AppendFirst),
        phase: EntryPhase::Point {
            point: SubEffectPoint::Synced,
            mode: InjectionMode::Kill,
        },
        test: "engine::topology::coverage::tests::event_kill_child",
    },
    Claim {
        site: EffectSiteId::Event(EventSite::AppendFirst),
        phase: EntryPhase::Point {
            point: SubEffectPoint::Synced,
            mode: InjectionMode::ErrorReturn,
        },
        test: "engine::topology::coverage::tests::every_error_return_point_of_the_event_funnels_fires_under_the_production_adapter",
    },
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
        site: EffectSiteId::Event(EventSite::AppendInformational),
        phase: EntryPhase::Before,
        test: "engine::topology::coverage::tests::event_kill_child",
    },
    Claim {
        site: EffectSiteId::Event(EventSite::AppendInformational),
        phase: EntryPhase::After,
        test: "engine::topology::emit::tests::poisoned_fold_refuses_every_later_transition",
    },
    Claim {
        site: EffectSiteId::Event(EventSite::AppendInformational),
        phase: EntryPhase::Point {
            point: SubEffectPoint::Written,
            mode: InjectionMode::Kill,
        },
        test: "engine::topology::coverage::tests::event_kill_child",
    },
    Claim {
        site: EffectSiteId::Event(EventSite::AppendInformational),
        phase: EntryPhase::Point {
            point: SubEffectPoint::Written,
            mode: InjectionMode::ErrorReturn,
        },
        test: "engine::topology::coverage::tests::every_error_return_point_of_the_event_funnels_fires_under_the_production_adapter",
    },
    Claim {
        site: EffectSiteId::Event(EventSite::AppendInformational),
        phase: EntryPhase::Point {
            point: SubEffectPoint::WrittenFull,
            mode: InjectionMode::ErrorReturn,
        },
        test: "engine::topology::coverage::tests::every_error_return_point_of_the_event_funnels_fires_under_the_production_adapter",
    },
    Claim {
        site: EffectSiteId::Event(EventSite::AppendInformational),
        phase: EntryPhase::Point {
            point: SubEffectPoint::Synced,
            mode: InjectionMode::Kill,
        },
        test: "engine::topology::coverage::tests::event_kill_child",
    },
    Claim {
        site: EffectSiteId::Event(EventSite::AppendInformational),
        phase: EntryPhase::Point {
            point: SubEffectPoint::Synced,
            mode: InjectionMode::ErrorReturn,
        },
        test: "engine::topology::coverage::tests::every_error_return_point_of_the_event_funnels_fires_under_the_production_adapter",
    },
    Claim {
        site: EffectSiteId::Answer(AnswerSite::StageWrite),
        phase: EntryPhase::Before,
        test: "engine::topology::coverage::tests::the_question_and_answer_funnels_execute_both_phases_under_the_production_adapter",
    },
    Claim {
        site: EffectSiteId::Answer(AnswerSite::StageWrite),
        phase: EntryPhase::After,
        test: "engine::topology::coverage::tests::the_question_and_answer_funnels_execute_both_phases_under_the_production_adapter",
    },
    Claim {
        site: EffectSiteId::Answer(AnswerSite::PublishRename),
        phase: EntryPhase::Before,
        test: "engine::topology::coverage::tests::the_question_and_answer_funnels_execute_both_phases_under_the_production_adapter",
    },
    Claim {
        site: EffectSiteId::Answer(AnswerSite::PublishRename),
        phase: EntryPhase::After,
        test: "engine::topology::coverage::tests::the_question_and_answer_funnels_execute_both_phases_under_the_production_adapter",
    },
    Claim {
        site: EffectSiteId::Answer(AnswerSite::Ingest),
        phase: EntryPhase::Before,
        test: "engine::topology::coverage::tests::the_question_and_answer_funnels_execute_both_phases_under_the_production_adapter",
    },
    Claim {
        site: EffectSiteId::Answer(AnswerSite::Ingest),
        phase: EntryPhase::After,
        test: "engine::topology::coverage::tests::the_question_and_answer_funnels_execute_both_phases_under_the_production_adapter",
    },
    Claim {
        site: EffectSiteId::Lock(LockSite::AcquireRun),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_binding_answer_naming_no_frozen_option_is_refused_before_any_append",
    },
    Claim {
        site: EffectSiteId::Lock(LockSite::AcquireRun),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_binding_answer_naming_no_frozen_option_is_refused_before_any_append",
    },
    Claim {
        site: EffectSiteId::Lock(LockSite::AcquireWorktree),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_binding_answer_naming_no_frozen_option_is_refused_before_any_append",
    },
    Claim {
        site: EffectSiteId::Lock(LockSite::AcquireWorktree),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_binding_answer_naming_no_frozen_option_is_refused_before_any_append",
    },
    Claim {
        site: EffectSiteId::Lock(LockSite::ProbeCleanupExclusive),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_binding_answer_naming_no_frozen_option_is_refused_before_any_append",
    },
    Claim {
        site: EffectSiteId::Lock(LockSite::ProbeCleanupExclusive),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_binding_answer_naming_no_frozen_option_is_refused_before_any_append",
    },
    Claim {
        site: EffectSiteId::Lock(LockSite::Release),
        phase: EntryPhase::Before,
        test: "engine::topology::create::tests::a_commit_record_stat_that_cannot_answer_retains_both_halves_and_releases_the_lock",
    },
    Claim {
        site: EffectSiteId::Lock(LockSite::Release),
        phase: EntryPhase::After,
        test: "engine::topology::create::tests::a_commit_record_stat_that_cannot_answer_retains_both_halves_and_releases_the_lock",
    },
    Claim {
        site: EffectSiteId::Lock(LockSite::CreateWorktreeLockFile),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_binding_answer_naming_no_frozen_option_is_refused_before_any_append",
    },
    Claim {
        site: EffectSiteId::Lock(LockSite::CreateWorktreeLockFile),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_binding_answer_naming_no_frozen_option_is_refused_before_any_append",
    },
    Claim {
        site: EffectSiteId::Lock(LockSite::ObserveCleanupHold),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_binding_answer_naming_no_frozen_option_is_refused_before_any_append",
    },
    Claim {
        site: EffectSiteId::Lock(LockSite::ObserveCleanupHold),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_binding_answer_naming_no_frozen_option_is_refused_before_any_append",
    },
    Claim {
        site: EffectSiteId::Process(ProcessSite::Spawn),
        phase: EntryPhase::Point {
            point: SubEffectPoint::AmbientJobJoined,
            mode: InjectionMode::Kill,
        },
        test: "engine::topology::coverage::tests::spawn_kill_child",
    },
    Claim {
        site: EffectSiteId::Process(ProcessSite::Spawn),
        phase: EntryPhase::Point {
            point: SubEffectPoint::AmbientJobJoined,
            mode: InjectionMode::ErrorReturn,
        },
        test: "engine::topology::coverage::tests::the_ambient_job_join_error_return_point_fires_under_the_process_funnel",
    },
    Claim {
        site: EffectSiteId::Process(ProcessSite::Spawn),
        phase: EntryPhase::Point {
            point: SubEffectPoint::CreatedSuspended,
            mode: InjectionMode::Kill,
        },
        test: "engine::topology::coverage::tests::spawn_kill_child",
    },
    Claim {
        site: EffectSiteId::Process(ProcessSite::Spawn),
        phase: EntryPhase::Point {
            point: SubEffectPoint::PrivateJobAssigned,
            mode: InjectionMode::Kill,
        },
        test: "engine::topology::coverage::tests::spawn_kill_child",
    },
    Claim {
        site: EffectSiteId::Process(ProcessSite::Spawn),
        phase: EntryPhase::Point {
            point: SubEffectPoint::Resumed,
            mode: InjectionMode::Kill,
        },
        test: "engine::topology::coverage::tests::spawn_kill_child",
    },
    Claim {
        site: EffectSiteId::Process(ProcessSite::Spawn),
        phase: EntryPhase::Point {
            point: SubEffectPoint::ReaperStarted,
            mode: InjectionMode::Kill,
        },
        test: "engine::topology::coverage::tests::spawn_kill_child",
    },
    Claim {
        site: EffectSiteId::Process(ProcessSite::Spawn),
        phase: EntryPhase::Point {
            point: SubEffectPoint::PreExecPgidAndRegister,
            mode: InjectionMode::Kill,
        },
        test: "engine::topology::coverage::tests::spawn_kill_child",
    },
    Claim {
        site: EffectSiteId::Process(ProcessSite::Spawn),
        phase: EntryPhase::Point {
            point: SubEffectPoint::Exec,
            mode: InjectionMode::Kill,
        },
        test: "engine::topology::coverage::tests::spawn_kill_child",
    },
    Claim {
        site: EffectSiteId::Process(ProcessSite::Spawn),
        phase: EntryPhase::Point {
            point: SubEffectPoint::Registered,
            mode: InjectionMode::Kill,
        },
        test: "engine::topology::coverage::tests::spawn_kill_child",
    },
    Claim {
        site: EffectSiteId::Container(ContainerSite::WriteIntent),
        phase: EntryPhase::Before,
        test: "engine::topology::coverage::tests::the_container_launch_funnels_execute_both_phases_under_the_production_adapter",
    },
    Claim {
        site: EffectSiteId::Container(ContainerSite::WriteIntent),
        phase: EntryPhase::After,
        test: "engine::topology::coverage::tests::the_container_launch_funnels_execute_both_phases_under_the_production_adapter",
    },
    Claim {
        site: EffectSiteId::Container(ContainerSite::Create),
        phase: EntryPhase::Before,
        test: "engine::topology::coverage::tests::the_container_launch_funnels_execute_both_phases_under_the_production_adapter",
    },
    Claim {
        site: EffectSiteId::Container(ContainerSite::Create),
        phase: EntryPhase::After,
        test: "engine::topology::coverage::tests::the_container_launch_funnels_execute_both_phases_under_the_production_adapter",
    },
    Claim {
        site: EffectSiteId::Container(ContainerSite::Start),
        phase: EntryPhase::Before,
        test: "engine::topology::coverage::tests::the_container_launch_funnels_execute_both_phases_under_the_production_adapter",
    },
    Claim {
        site: EffectSiteId::Container(ContainerSite::Start),
        phase: EntryPhase::After,
        test: "engine::topology::coverage::tests::the_container_launch_funnels_execute_both_phases_under_the_production_adapter",
    },
    Claim {
        site: EffectSiteId::Container(ContainerSite::MountGitView),
        phase: EntryPhase::Before,
        test: "engine::topology::coverage::tests::the_container_launch_funnels_execute_both_phases_under_the_production_adapter",
    },
    Claim {
        site: EffectSiteId::Container(ContainerSite::MountGitView),
        phase: EntryPhase::After,
        test: "engine::topology::coverage::tests::the_container_launch_funnels_execute_both_phases_under_the_production_adapter",
    },
    Claim {
        site: EffectSiteId::Container(ContainerSite::Stop),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_lost_gate_container_is_reclaimed_by_the_next_resume_before_the_verification_is_settled",
    },
    Claim {
        site: EffectSiteId::Container(ContainerSite::Stop),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_lost_gate_container_is_reclaimed_by_the_next_resume_before_the_verification_is_settled",
    },
    Claim {
        site: EffectSiteId::Container(ContainerSite::Remove),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_lost_gate_container_is_reclaimed_by_the_next_resume_before_the_verification_is_settled",
    },
    Claim {
        site: EffectSiteId::Container(ContainerSite::Remove),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_lost_gate_container_is_reclaimed_by_the_next_resume_before_the_verification_is_settled",
    },
    Claim {
        site: EffectSiteId::Container(ContainerSite::UnmountGitView),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_lost_gate_container_is_reclaimed_by_the_next_resume_before_the_verification_is_settled",
    },
    Claim {
        site: EffectSiteId::Container(ContainerSite::UnmountGitView),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_lost_gate_container_is_reclaimed_by_the_next_resume_before_the_verification_is_settled",
    },
    Claim {
        site: EffectSiteId::Container(ContainerSite::RemoveIntent),
        phase: EntryPhase::Before,
        test: "engine::topology::recover::tests::a_lost_gate_container_is_reclaimed_by_the_next_resume_before_the_verification_is_settled",
    },
    Claim {
        site: EffectSiteId::Container(ContainerSite::RemoveIntent),
        phase: EntryPhase::After,
        test: "engine::topology::recover::tests::a_lost_gate_container_is_reclaimed_by_the_next_resume_before_the_verification_is_settled",
    },
];

pub fn registry(evidence: &ResidueEvidence) -> Result<FaultRegistry, String> {
    let mut registry = registry_of(CLAIMS).map_err(|error| error.to_string())?;
    for entry in residue_entries(evidence)? {
        registry.insert(entry).map_err(|error| error.to_string())?;
    }
    for entry in no_execution_entries() {
        registry.insert(entry).map_err(|error| error.to_string())?;
    }
    Ok(registry)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnobservableRecord {
    pub site: String,
    pub phases: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryDocument {
    pub note: String,
    pub range: Vec<String>,
    pub hosts: Vec<String>,
    pub unobservable: Vec<UnobservableRecord>,
    pub fast_sequences: Vec<String>,
    pub entries: Vec<RegistryEntry>,
}

pub fn registry_document(evidence: &ResidueEvidence) -> Result<RegistryDocument, String> {
    Ok(RegistryDocument {
        note: "decisions.fault_injection_registry, ST-07 for the sequential topology over the \
               full claimed inventory (every Topology- and Shared-scoped site effect_sites.json \
               generates): one entry per site x hook phase x parent-side point x injection \
               mode, built through FaultRegistry::insert, each naming the committed test the \
               suite's observation export shows executing it; one recovery-proven entry per \
               residue class, its synthetic records from effects/residue-synthetic.json and its \
               sampling record from effects/residue-histogram.json or \
               effects/residue-histogram-sequential.json (the histogram counts are the \
               machine-varying half: they are what the files held when this document was \
               regenerated, the pin compares everything but them, and the merge check reads \
               the files); and one no-execution record per site the exact-base fast path \
               skips, naming every fast sequence the export records. The Windows-only points \
               of Process.Spawn name the tests that execute them on that host; `hosts` lists \
               both, and the merge check on each host holds the document to the points that \
               host requires. `unobservable` declares, per coordinate and with the reason, the \
               six coordinates no test can observe executed. The frozen sampling N a \
               recovery-proven entry cites is effects/residue-classes.json's \
               (SWEEP-BIJECTION-005)."
            .to_owned(),
        range: inventory().into_iter().map(EffectSiteId::name).collect(),
        hosts: Host::ALL
            .iter()
            .map(|host| host.name().to_owned())
            .collect(),
        unobservable: declared_unobservable()
            .iter()
            .map(|entry| UnobservableRecord {
                site: entry.site.name(),
                phases: entry.phases.iter().map(ToString::to_string).collect(),
                reason: entry.reason.to_owned(),
            })
            .collect(),
        fast_sequences: FAST_SEQUENCES
            .iter()
            .map(|sequence| (*sequence).to_owned())
            .collect(),
        entries: registry(evidence)?.entries().to_vec(),
    })
}

pub fn registry_json(evidence: &ResidueEvidence) -> Result<String, String> {
    let document = registry_document(evidence)?;
    serde_json::to_string_pretty(&document)
        .map(|json| format!("{json}\n"))
        .map_err(|error| error.to_string())
}

#[must_use]
pub fn without_histograms(mut document: RegistryDocument) -> RegistryDocument {
    for entry in &mut document.entries {
        if let Evidence::RecoveryProven { sampling, .. } = &mut entry.evidence {
            sampling.histogram = crate::topology::effects::ClassHistogram::default();
        }
    }
    document
}

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
