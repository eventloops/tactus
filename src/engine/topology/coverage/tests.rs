use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::*;
use crate::observations::OBSERVATIONS_ENV;
use crate::topology::effects::{
    BijectionFailure, ClassHistogram, ObjectResidue, ResidueClass, SamplingRecord, SyntheticRecord,
    check_bijection,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn rust_sources(dir: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            rust_sources(&path, into);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            into.push(path);
        }
    }
}

/// Line comments blanked, so a test named in prose is not a definition.
fn without_line_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("//") {
                ""
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every file under `src/` that defines `fn <name>(` with `#[test]` in the
/// four hundred bytes before it, relative to the repository root.
fn defining_files(name: &str) -> Vec<String> {
    let root = repo_root();
    let mut files = Vec::new();
    rust_sources(&root.join("src"), &mut files);
    let needle = format!("fn {name}(");
    let mut found = Vec::new();
    for path in files {
        let source = std::fs::read_to_string(&path).expect("a source file reads");
        let code = without_line_comments(&source);
        let mut from = 0;
        while let Some(index) = code[from..].find(&needle) {
            let at = from + index;
            let preceding = &code[at.saturating_sub(400)..at];
            if preceding.contains("#[test]") {
                found.push(
                    path.strip_prefix(&root)
                        .expect("under the manifest")
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
                break;
            }
            from = at + needle.len();
        }
    }
    found
}

/// `engine::topology::recover::tests::name` is defined in
/// `src/engine/topology/recover/tests.rs` or in `src/engine/topology/recover.rs`
/// (an inline `mod tests`), and nowhere else.
fn expected_files(test: &str) -> Vec<String> {
    let mut segments: Vec<&str> = test.split("::").collect();
    segments.pop();
    let module = segments.join("/");
    let parent = segments[..segments.len().saturating_sub(1)].join("/");
    vec![format!("src/{module}.rs"), format!("src/{parent}.rs")]
}

/// The residue-class evidence the registry embeds, read from the three
/// tracked files the tests that produce it write.
fn evidence() -> ResidueEvidence {
    let read = |path: &str| {
        std::fs::read_to_string(repo_root().join(path))
            .unwrap_or_else(|error| panic!("`{path}` is tracked: {error}"))
    };
    ResidueEvidence::parse(
        &read(crate::effects::RESIDUE_SYNTHETIC_JSON),
        &[
            &read(crate::effects::RESIDUE_HISTOGRAM_JSON),
            &read(crate::effects::SEQUENTIAL_RESIDUE_HISTOGRAM_JSON),
        ],
    )
    .expect("the three evidence files parse and name sites the enums generate")
}

/// The non-ignored half of the merge check: every required coordinate of
/// every site of the inventory, on both hosts, has exactly one claim or is
/// declared unobservable (never both); every claim is inside the inventory
/// and names a funnel execution; and the document builds with one entry per
/// claim, per residue class and per fast-path site.
#[test]
fn the_inventory_is_claimed_at_every_required_phase_on_both_hosts_or_declared() {
    let declared = declared_unobservable();
    let registry = registry(&evidence()).expect("every claim and every evidence entry is accepted");
    let sites = inventory();
    assert_eq!(sites.len(), 68, "the Topology- and Shared-scoped sites");
    let mut required = 0;
    let mut excused = 0;
    for host in Host::ALL {
        for site in &sites {
            for phase in required_phases(*site, *host) {
                let claimed = CLAIMS
                    .iter()
                    .any(|claim| claim.site == *site && claim.phase == phase);
                if declared.iter().any(|entry| entry.covers(*site, phase)) {
                    excused += 1;
                    assert!(
                        !claimed,
                        "{host}: `{site}`/`{phase}` is declared unobservable and claimed"
                    );
                } else {
                    required += 1;
                    assert!(claimed, "{host}: `{site}` has no claim for `{phase}`");
                }
            }
        }
    }
    assert_eq!(excused, 12, "six coordinates, on each of two hosts");
    assert_eq!(
        required, 315,
        "163 Unix and 164 Windows coordinates, six excused on each"
    );
    assert_eq!(
        CLAIMS.len(),
        162,
        "153 shared, four Unix-only and five Windows-only points"
    );
    for claim in CLAIMS {
        assert!(
            sites.contains(&claim.site),
            "`{}` is claimed and is not in the inventory",
            claim.site
        );
        assert!(
            Host::ALL
                .iter()
                .any(|host| required_phases(claim.site, *host).contains(&claim.phase)),
            "`{}`/`{}` is claimed and required on no host",
            claim.site,
            claim.phase
        );
        assert!(
            is_funnel_execution(claim.test),
            "`{}` is an adapter unit test, not a funnel execution",
            claim.test
        );
    }
    let mut keys: Vec<(EffectSiteId, EntryPhase)> = CLAIMS
        .iter()
        .map(|claim| (claim.site, claim.phase))
        .collect();
    let before = keys.len();
    keys.sort_by_key(|(site, phase)| format!("{site}/{phase}"));
    keys.dedup();
    assert_eq!(keys.len(), before, "two claims for one coordinate");

    let residue_sites: Vec<EffectSiteId> = sites
        .iter()
        .copied()
        .filter(|site| !site.residue_classes().is_empty())
        .collect();
    assert_eq!(residue_sites.len(), 9, "{residue_sites:?}");
    let fast_sites: Vec<EffectSiteId> = sites
        .iter()
        .copied()
        .filter(|site| site.skipped_on_fast_path())
        .collect();
    assert_eq!(fast_sites.len(), 3, "{fast_sites:?}");
    assert_eq!(
        registry.entries().len(),
        CLAIMS.len() + residue_sites.len() + fast_sites.len()
    );
    for site in residue_sites {
        assert!(
            registry.entries().iter().any(|entry| entry.site == site
                && matches!(entry.phase, EntryPhase::Residue { .. })
                && matches!(entry.evidence, Evidence::RecoveryProven { .. })),
            "`{site}` has no recovery-proven residue entry"
        );
    }
    for site in fast_sites {
        assert!(
            registry.entries().iter().any(|entry| entry.site == site
                && entry.phase == EntryPhase::NoExecution
                && matches!(&entry.evidence, Evidence::NotExecuted { sequences, .. } if sequences.len() == FAST_SEQUENCES.len())),
            "`{site}` has no no-execution record naming every fast sequence"
        );
    }
    for entry in &declared {
        assert!(
            sites.contains(&entry.site),
            "`{}` is declared and not in the inventory",
            entry.site
        );
        assert!(!entry.reason.trim().is_empty());
    }
    assert_eq!(
        checked_inventory().len(),
        sites.len() - 2,
        "two sites are declared unobservable as a whole"
    );
}

#[test]
fn every_claim_names_a_test_defined_exactly_once_under_src() {
    let mut names: Vec<&str> = CLAIMS.iter().map(|claim| claim.test).collect();
    names.push(FAST_PATH_TEST);
    names.sort_unstable();
    names.dedup();
    for name in names {
        let (module, bare) = name
            .rsplit_once("::")
            .expect("a claim names a test by its module path");
        let files = defining_files(bare);
        assert_eq!(
            files.len(),
            1,
            "`{name}` is defined as a test in {} files: {files:?}",
            files.len()
        );
        assert!(
            expected_files(name).contains(&files[0]),
            "`{name}` is defined in `{}`, which is not where `{module}` lives",
            files[0]
        );
    }
    assert!(
        defining_files("a_test_this_tree_does_not_contain_and_never_will").is_empty(),
        "the predicate finds a test that does not exist"
    );
    assert!(
        defining_files("st07_the_sequential_range_is_a_bijection_over_the_exported_observations")
            .len()
            == 1,
        "the predicate finds an ignored test defined in this file"
    );
}

/// The pin: `effects/sequential-registry.json` is the document the
/// generator produces from the claims and the three evidence files, the
/// machine-varying histogram counts set aside; those are held to the files
/// by the merge check, and the pinned copy of them is well-formed.
#[test]
fn the_sequential_registry_is_pinned() {
    let generated = registry_document(&evidence()).expect("the document builds");
    let pinned_text = std::fs::read_to_string(repo_root().join(REGISTRY_JSON))
        .expect("effects/sequential-registry.json is tracked")
        .replace("\r\n", "\n");
    let pinned: RegistryDocument =
        serde_json::from_str(&pinned_text).expect("the pinned document parses");
    assert!(
        without_histograms(generated.clone()) == without_histograms(pinned.clone()),
        "`{REGISTRY_JSON}` is not the document `coverage::registry_document` generates \
         (histogram counts aside); regenerate it with `cargo test --lib -- --ignored \
         engine::topology::coverage::tests::write_the_sequential_registry`"
    );
    assert_eq!(pinned.entries.len(), CLAIMS.len() + 12);
    assert_eq!(
        pinned.range,
        inventory()
            .into_iter()
            .map(EffectSiteId::name)
            .collect::<Vec<_>>()
    );
    assert_eq!(pinned.unobservable.len(), 3);
    assert_eq!(pinned.fast_sequences, FAST_SEQUENCES);
    for entry in &pinned.entries {
        if let Evidence::RecoveryProven { sampling, .. } = &entry.evidence {
            assert_eq!(
                sampling.histogram.total() + u64::from(sampling.unclassified),
                u64::from(sampling.n),
                "`{}`: the pinned histogram accounts for every sample",
                entry.site
            );
        }
    }
}

#[test]
#[ignore = "rewrites effects/sequential-registry.json from the claims and the evidence files; run on purpose"]
fn write_the_sequential_registry() {
    let generated = registry_json(&evidence()).expect("the document serializes");
    crate::workspace_manager::fixture::write_file(
        &repo_root().join(REGISTRY_JSON),
        generated.as_bytes(),
    );
}

/// Each declared-unobservable coordinate's reason is a fact about the
/// code, and the code is read here: the report module names no
/// `ReportSite`, the process funnel consults no hook phase, and
/// termination consults no hook at all.
#[test]
fn every_declared_unobservable_coordinate_has_its_reason_in_the_code() {
    use crate::effects::blank_comments_and_strings;

    let source = |path: &str| {
        blank_comments_and_strings(
            &std::fs::read_to_string(repo_root().join(path))
                .unwrap_or_else(|error| panic!("`{path}` reads: {error}"))
                .replace("\r\n", "\n"),
        )
    };
    let declared = declared_unobservable();
    assert_eq!(declared.len(), 3);

    let report = EffectSiteId::Report(ReportSite::Write);
    assert!(
        declared
            .iter()
            .any(|entry| entry.site == report && entry.phases.is_empty())
    );
    assert!(
        !source("src/util.rs").contains("ReportSite"),
        "`Report.Write`'s inventoried module names its site: it has a funnel now, so the \
         declaration is false"
    );
    assert!(
        source("src/rundir.rs").contains("RunDirSite::WriteReport"),
        "the report is written through `RunDir.WriteReport`"
    );

    let spawn = EffectSiteId::Process(ProcessSite::Spawn);
    assert!(declared.iter().any(
        |entry| entry.site == spawn && entry.phases == [EntryPhase::Before, EntryPhase::After]
    ));
    let hooks = source("src/agent/proc/hooks.rs");
    let trait_body = hooks
        .split("pub trait SpawnHooks")
        .nth(1)
        .and_then(|rest| rest.split_once("\n}\n"))
        .map(|(body, _)| body.to_owned())
        .expect("the trait is defined there");
    assert!(
        !trait_body.contains("HookPhase") && !trait_body.contains("fn phase"),
        "`SpawnHooks` grew a phase hook: {trait_body}"
    );
    let proc = source("src/agent/proc.rs");
    assert!(
        !proc.contains("HookPhase::"),
        "the process funnel consults a hook phase now; the declaration is false"
    );

    let terminate = EffectSiteId::Process(ProcessSite::Terminate);
    assert!(
        declared
            .iter()
            .any(|entry| entry.site == terminate && entry.phases.is_empty())
    );
    let kill_tree = proc
        .split("fn kill_tree(")
        .nth(1)
        .and_then(|rest| rest.split_once("\n}\n"))
        .map(|(body, _)| body.to_owned())
        .expect("kill_tree is defined there");
    assert!(
        !kill_tree.contains("hooks"),
        "`kill_tree` consults a hook now; the declaration is false: {kill_tree}"
    );
    for site in [report, terminate] {
        assert!(
            EffectSiteId::Process(ProcessSite::Terminate)
                .sub_effects()
                .is_empty()
                && site.sub_effects().len() <= 1,
            "a whole-site declaration covers a site with points; declare them one by one"
        );
    }
}

/// SWEEP-BIJECTION-005: the frozen `N` comes from the declarations file, and
/// an entry citing another number is refused by the gate that reads both.
#[test]
fn a_recovery_proven_entrys_n_is_held_to_the_declarations() {
    let declarations = std::fs::read_to_string(repo_root().join("effects/residue-classes.json"))
        .expect("the declarations are tracked");
    let site = EffectSiteId::Object(crate::topology::effects::ObjectSite::CandidateCommitTree);
    let frozen = frozen_sampling_n(&declarations, site)
        .expect("the declarations parse")
        .expect("the site is declared");
    assert_eq!(frozen, 8, "the frozen N the declarations carry");
    let entry = |n: u32| {
        let phase = EntryPhase::Residue {
            class: ResidueClass::ObjectInternal,
        };
        let semantics = site.semantics(phase);
        RegistryEntry {
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
                synthetic: site
                    .residue_elements()
                    .iter()
                    .map(|element| SyntheticRecord {
                        element: *element,
                        constructed: true,
                        classified: ObjectResidue::Internal,
                        recovered: true,
                    })
                    .collect(),
                sampling: SamplingRecord {
                    n,
                    histogram: ClassHistogram {
                        none: n,
                        internal: 0,
                        after: 0,
                    },
                    unclassified: 0,
                    recovered: true,
                },
            },
        }
    };
    assert!(check_frozen_n(&[entry(8)], &declarations).is_empty());
    let problems = check_frozen_n(&[entry(1)], &declarations);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].contains("cites n = 1") && problems[0].contains("freeze 8"),
        "{problems:?}"
    );
    assert!(
        frozen_sampling_n(&declarations, EffectSiteId::Worktree(WorktreeSite::Remove))
            .expect("parses")
            .is_none(),
        "a pruning site freezes no N"
    );
    let entries = residue_entries(&evidence()).expect("every residue class has both halves");
    assert_eq!(entries.len(), 9, "one entry per residue-classified site");
    assert!(
        check_frozen_n(&entries, &declarations).is_empty(),
        "every residue entry cites the frozen N"
    );
    for entry in &entries {
        let Evidence::RecoveryProven { synthetic, .. } = &entry.evidence else {
            panic!("a residue entry is recovery-proven");
        };
        assert_eq!(
            synthetic.len(),
            entry.site.residue_elements().len(),
            "`{}`: one synthetic record per registered element",
            entry.site
        );
    }
}

#[test]
fn an_observation_record_merges_by_the_larger_count_and_round_trips() {
    let mut harness = HookHarness::new();
    let site = EffectSiteId::Worktree(WorktreeSite::Remove);
    harness.hook(site, HookPhase::Before);
    harness.hook(site, HookPhase::Before);
    harness.hook(site, HookPhase::After);
    let mut record = ObservationRecord::of("a::test", &harness);
    assert!(record.observed(site, HookPhase::Before));
    assert!(!record.observed(
        site,
        HookPhase::Point {
            point: SubEffectPoint::Written,
            mode: InjectionMode::Kill
        }
    ));
    let mut later = HookHarness::new();
    later.hook(site, HookPhase::Before);
    later.hook(site, HookPhase::Before);
    later.hook(site, HookPhase::Before);
    let append = EffectSiteId::Event(EventSite::Append);
    later
        .arm(append, SubEffectPoint::Written, InjectionMode::Kill)
        .expect("armable");
    assert_eq!(
        later.hook(
            append,
            HookPhase::Point {
                point: SubEffectPoint::Written,
                mode: InjectionMode::Kill
            }
        ),
        crate::topology::effects::Injection::Kill
    );
    record.merge(ObservationRecord::of("a::test", &later));
    assert_eq!(
        record
            .observed
            .iter()
            .find(|seen| seen.site == site && seen.phase == HookPhase::Before)
            .map(|seen| seen.count),
        Some(3)
    );
    assert_eq!(
        record
            .observed
            .iter()
            .find(|seen| seen.site == site && seen.phase == HookPhase::After)
            .map(|seen| seen.count),
        Some(1)
    );
    let json = serde_json::to_string(&record).expect("serializes");
    let back: ObservationRecord = serde_json::from_str(&json).expect("parses");
    assert_eq!(back, record);
    assert_eq!(ObservationRecord::file_name("a::b::c"), "a__b__c.json");

    let replayed = harness_from(&[record]);
    assert!(replayed.observed(site, HookPhase::Before));
    assert!(replayed.observed(
        append,
        HookPhase::Point {
            point: SubEffectPoint::Written,
            mode: InjectionMode::Kill
        }
    ));
}

/// ST-07's merge check over the whole inventory: run the suite with
/// `UPSTROKE_HOOK_OBSERVATIONS=<dir>` first (every adapter exports on drop and
/// before a kill, and the kill children inherit the variable), then this test
/// with the same variable. The bijection over the checked inventory must be
/// empty but for the declared coordinates, no declared coordinate may have
/// been observed, the export's fast sequences must be the ones the
/// no-execution record names, every claim this host requires must be
/// witnessed by its named test, and every recovery-proven entry's N must be
/// the declarations'. The name keeps the range ST-07 called "sequential";
/// the range is the inventory.
#[test]
#[ignore = "reads the observation export a full suite run wrote under UPSTROKE_HOOK_OBSERVATIONS"]
fn st07_the_sequential_range_is_a_bijection_over_the_exported_observations() {
    let dir = std::env::var(OBSERVATIONS_ENV)
        .unwrap_or_else(|_| panic!("{OBSERVATIONS_ENV} names the directory a suite run exported"));
    let records = load_observations(Path::new(&dir)).expect("the export loads");
    assert!(
        records.len() > 100,
        "the export holds {} records; a full run of the suite writes more",
        records.len()
    );
    let executions: Vec<ObservationRecord> = records
        .iter()
        .filter(|record| is_funnel_execution(&record.test))
        .cloned()
        .collect();
    let harness = harness_from(&executions);
    let evidence = evidence();
    let registry = registry(&evidence).expect("the document builds");
    let declared = declared_unobservable();
    let host = Host::current();
    let inventory = checked_inventory();
    let failures = check_bijection(&inventory, &harness, registry.entries(), host);
    let unexcused: Vec<&BijectionFailure> = failures
        .iter()
        .filter(|failure| !excused(failure, &declared))
        .collect();
    assert!(unexcused.is_empty(), "{unexcused:#?}");
    for entry in &declared {
        for phase in required_phases(entry.site, host) {
            if !entry.covers(entry.site, phase) {
                continue;
            }
            if let Some(hook) = phase.hook_phase() {
                assert!(
                    !harness.observed(entry.site, hook),
                    "`{}`/`{phase}` is declared unobservable and the export observed it",
                    entry.site
                );
            }
        }
    }
    let mut sequences: Vec<&str> = harness
        .fast_sequences()
        .iter()
        .map(|sequence| sequence.name())
        .collect();
    sequences.sort_unstable();
    sequences.dedup();
    let mut expected = FAST_SEQUENCES.to_vec();
    expected.sort_unstable();
    assert_eq!(
        sequences, expected,
        "the fast sequences the export records are the ones the no-execution record names"
    );

    let mut unwitnessed = Vec::new();
    let mut witnessed = 0usize;
    for claim in CLAIMS {
        if !required_phases(claim.site, host).contains(&claim.phase) {
            continue;
        }
        let Some(phase) = claim.hook_phase() else {
            continue;
        };
        let seen = records
            .iter()
            .find(|record| record.test == claim.test)
            .is_some_and(|record| record.observed(claim.site, phase));
        if seen {
            witnessed += 1;
        } else {
            unwitnessed.push(format!("{}/{} by {}", claim.site, claim.phase, claim.test));
        }
    }
    assert!(
        unwitnessed.is_empty(),
        "claims whose named test did not execute the coordinate in this export:\n{}",
        unwitnessed.join("\n")
    );
    assert!(
        records
            .iter()
            .find(|record| record.test == FAST_PATH_TEST)
            .is_some_and(|record| record.fast_sequences.iter().any(|s| s.name == "s0")),
        "the no-execution record's test recorded the `s0` fast sequence in this export"
    );

    let declarations = std::fs::read_to_string(repo_root().join("effects/residue-classes.json"))
        .expect("the declarations are tracked");
    let problems = check_frozen_n(registry.entries(), &declarations);
    assert!(problems.is_empty(), "{problems:#?}");

    if let Ok(path) = std::env::var("UPSTROKE_ST07_SUMMARY") {
        let summary = serde_json::json!({
            "records": records.len(),
            "funnel_executions": executions.len(),
            "inventory": inventory.len(),
            "declared_unobservable": declared.len(),
            "entries": registry.entries().len(),
            "host": host.name(),
            "failures": failures.len(),
            "excused_failures": failures.len() - unexcused.len(),
            "witnessed_claims": witnessed,
            "fast_sequences": sequences,
        });
        crate::workspace_manager::fixture::write_file(
            Path::new(&path),
            format!(
                "{}\n",
                serde_json::to_string_pretty(&summary).expect("serializes")
            )
            .as_bytes(),
        );
    }
}

// ---------------------------------------------------------------------------
// PR10 ST-07 witnesses: the coordinates no other suite executes under the
// production adapters. Each test drives a production funnel through the
// adapter the sequential run uses (`rundir::HarnessHooks`,
// `events::log::HarnessEventHooks`, `runner::HarnessHooks`,
// `runner::container::HarnessHooks`), so its observation export is an
// execution of the site and not a call on the adapter.
// ---------------------------------------------------------------------------

fn shared_harness() -> Arc<Mutex<HookHarness>> {
    Arc::new(Mutex::new(HookHarness::new()))
}

fn saw(harness: &Arc<Mutex<HookHarness>>, site: EffectSiteId, phase: HookPhase) -> bool {
    harness
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .observed(site, phase)
}

fn assert_both_phases(harness: &Arc<Mutex<HookHarness>>, site: EffectSiteId) {
    assert!(saw(harness, site, HookPhase::Before), "`{site}` before");
    assert!(saw(harness, site, HookPhase::After), "`{site}` after");
}

/// `RunDir.WriteQuestionPayload`, `Answer.StageWrite`, `Answer.PublishRename`
/// and `Answer.Ingest`: the four funnels `src/rundir.rs` hooks and every
/// production caller (`src/interaction.rs`) reaches with `NoHooks`, so no
/// run of the sequential topology records them. Executed here through the
/// production adapter.
#[test]
fn the_question_and_answer_funnels_execute_both_phases_under_the_production_adapter() {
    use crate::topology::effects::AnswerSite;
    use crate::workspace_manager::fixture;

    let root = fixture::scratch("st07-question-answer-funnels");
    let questions = root.join("questions");
    let answers = root.join("answers");
    fixture::create_dir(&questions);
    fixture::create_dir(&answers);
    let harness = shared_harness();
    let mut hooks = crate::rundir::HarnessHooks::new(Arc::clone(&harness));

    crate::rundir::write_question_payload(
        &questions,
        "q-st07",
        &serde_json::json!({ "question": "which base?" }),
        &mut hooks,
    )
    .expect("the question payload is written");
    crate::rundir::stage_answer(
        &answers,
        "q-st07",
        &serde_json::json!({ "answer": "the exact one" }),
        &mut hooks,
    )
    .expect("the answer is staged");
    crate::rundir::publish_answer(&answers, "q-st07", &mut hooks).expect("the answer is published");
    let text = crate::rundir::ingest_answer(&answers, "q-st07", &mut hooks)
        .expect("the answer is read")
        .expect("the published answer is present");
    assert!(text.contains("the exact one"), "{text}");
    assert!(
        questions.join("q-st07.json").is_file() && answers.join("q-st07.json").is_file(),
        "both files exist after the four funnels"
    );
    assert!(
        !answers.join("q-st07.json.partial").exists(),
        "the rename consumed the staged file"
    );

    for site in [
        EffectSiteId::RunDir(RunDirSite::WriteQuestionPayload),
        EffectSiteId::Answer(AnswerSite::StageWrite),
        EffectSiteId::Answer(AnswerSite::PublishRename),
        EffectSiteId::Answer(AnswerSite::Ingest),
    ] {
        assert_both_phases(&harness, site);
    }
}

/// The three schema-4 append sites and a line each accepts: `site_for`
/// files `run_started` under `AppendFirst`, a transaction under `Append`,
/// and an informational kind under `AppendInformational`.
fn append_line(site: EventSite) -> crate::events::log::TopologyLine {
    use crate::events::log::TopologyLine;
    use crate::events::{BudgetKind, PoolExhausted};
    use crate::topology::events::{BudgetExceeded4, Epoch, TopologyEvent, TopologyEventBody};

    let body = match site {
        EventSite::AppendFirst => TopologyEventBody::RunStarted {
            data: Box::new(witness_run_started()),
        },
        EventSite::Append => TopologyEventBody::BudgetExceeded {
            data: BudgetExceeded4 {
                epoch: Epoch(0),
                budget: BudgetKind::Run,
                limit_usd: 1.0,
                spent_usd: 1.5,
                key: None,
            },
        },
        EventSite::AppendInformational => TopologyEventBody::PoolExhausted {
            data: PoolExhausted {
                pool: "claude-code".to_owned(),
                agent: "claude-code".to_owned(),
                reset_at: None,
                detail: "usage limit reached".to_owned(),
            },
        },
        other => panic!("`Event.{}` is not an append site", other.name()),
    };
    let (line, _) = TopologyLine::round_trip(&TopologyEvent {
        ts: "2026-09-13T00:00:00Z".to_owned(),
        body,
    })
    .expect("the witness line survives its wire format");
    assert_eq!(line.site(), site);
    line
}

/// A `run_started` record the log funnel accepts. The log writes bytes; no
/// field is read back here, so the values are placeholders.
fn witness_run_started() -> crate::topology::events::RunStarted4 {
    use crate::events::{ChainSummary, GateSummary};
    use crate::gates::ShellKind;
    use crate::ir::{Effort, ResolvedEffortPolicy, Tier};
    use crate::review::ReviewPlan;
    use crate::topology::events::{
        CommitSha, GitRef, IncarnationId, RunStarted4, RunnerContract, RunnerKind, RunnerPolicy,
        TopologyLimits,
    };
    use crate::topology::paths::{PathGrammar, PathPolicy, PathPolicyVersion};
    use crate::topology::schema::TOPOLOGY_SCHEMA;

    RunStarted4 {
        schema: TOPOLOGY_SCHEMA,
        upstroke_version: "0.2.0".to_owned(),
        run_id: "01KZST07000000000000000000".to_owned(),
        incarnation: IncarnationId("01KZST07000000000000000001".to_owned()),
        runner: RunnerPolicy {
            kind: RunnerKind::Host,
            policy: RunnerContract::HostV1,
            image: None,
            credential_volumes: None,
        },
        probed_agents: vec!["claude-code".to_owned()],
        branch: "upstroke/run-st07".to_owned(),
        integration_ref: GitRef::from("refs/upstroke/integration"),
        base_sha: CommitSha::from("0f5c1c4"),
        execution_root: "/var/lib/upstroke/roots".to_owned(),
        private_dir: "/var/lib/upstroke/private".to_owned(),
        plan_path: "docs/plan.md".to_owned(),
        config_path: None,
        plan_hash: "sha256:plan".to_owned(),
        normalized_plan_digest: "sha256:normalized".to_owned(),
        registry_digest: "sha256:registry".to_owned(),
        path_policy: PathPolicy {
            version: PathPolicyVersion::V2,
            case_fold: false,
            grammar: PathGrammar::Globset,
        },
        limits: TopologyLimits {
            max_parallel: 1,
            max_defers: 2,
            max_merge_repairs: 1,
        },
        gates: vec!["fmt".to_owned()],
        gates_from_config: true,
        gate_cmds: vec![GateSummary {
            name: "fmt".to_owned(),
            cmd: "cargo fmt --check".to_owned(),
            timeout: std::time::Duration::from_secs(60),
            shell: ShellKind::Sh,
        }],
        interaction_mode: "never".to_owned(),
        chains: vec![ChainSummary {
            task: "aleph".to_owned(),
            tiers: vec![Tier::Small],
            attempts_per: 1,
            bindings: None,
        }],
        effort_policy: ResolvedEffortPolicy {
            small: Effort::Low,
            mid: Effort::Medium,
            frontier: Effort::High,
            review: Effort::XHigh,
        },
        reviews: ReviewPlan {
            enabled: Some(false),
            alternative_available: Some(false),
            pass_timeout_secs: None,
            primary: None,
            alternative: None,
            second_opinion: Vec::new(),
        },
    }
}

const EVENT_SITES: [EventSite; 4] = [
    EventSite::OpenLog,
    EventSite::AppendFirst,
    EventSite::Append,
    EventSite::AppendInformational,
];

/// Every `(site, point, mode)` the four schema-4 event sites expose.
fn event_points(mode: InjectionMode) -> Vec<(EventSite, SubEffectPoint)> {
    let mut points = Vec::new();
    for site in EVENT_SITES {
        for point in EffectSiteId::Event(site).sub_effects() {
            if point.supports(mode) {
                points.push((site, *point));
            }
        }
    }
    points
}

/// Drive the production log funnel to `point` of `site` with `harness`
/// armed however the caller armed it. Returns the funnel's answer at the
/// step that consults the point.
fn drive_event_funnel(
    site: EventSite,
    point: SubEffectPoint,
    dir: &Path,
    harness: &Arc<Mutex<HookHarness>>,
) -> Result<(), crate::error::UpstrokeError> {
    use crate::events::log::{EventLog, HarnessEventHooks};
    use crate::workspace_manager::fixture;

    let path = dir.join("events.jsonl");
    let mut hooks = HarnessEventHooks::new(Arc::clone(harness));
    let mut warnings = Vec::new();
    if site == EventSite::OpenLog {
        if point == SubEffectPoint::TruncateTornTail {
            fixture::write_file(&path, b"{\"ts\":\"t\",\"kind\":\"x\"}\n{\"torn");
        }
        return EventLog::open_hooked(EventSite::OpenLog, &path, &mut warnings, &mut hooks)
            .map(|_| ());
    }
    let mut log = EventLog::open_hooked(EventSite::OpenLog, &path, &mut warnings, &mut hooks)
        .expect("a fresh log opens with nothing armed at its open points");
    if site != EventSite::AppendFirst {
        log.append_topology_hooked(
            EventSite::AppendFirst,
            &append_line(EventSite::AppendFirst),
            &mut hooks,
        )
        .expect("the first line is appended with nothing armed at its points");
    }
    log.append_topology_hooked(site, &append_line(site), &mut hooks)
}

/// Every error-return point of the four schema-4 event sites, fired through
/// the production adapter: the funnel returns the injected error and the
/// harness records the point executed in that mode.
#[test]
fn every_error_return_point_of_the_event_funnels_fires_under_the_production_adapter() {
    use crate::workspace_manager::fixture;

    let points = event_points(InjectionMode::ErrorReturn);
    assert_eq!(points.len(), 12, "{points:?}");
    for (site, point) in points {
        let dir = fixture::scratch(&format!("st07-event-error-{}-{point}", site.name()));
        let harness = shared_harness();
        harness
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .arm(EffectSiteId::Event(site), point, InjectionMode::ErrorReturn)
            .expect("the site exposes the point in this mode");
        let outcome = drive_event_funnel(site, point, &dir, &harness);
        assert!(
            outcome.is_err(),
            "`Event.{}`/{point}: the injected error was not returned",
            site.name()
        );
        let phase = HookPhase::Point {
            point,
            mode: InjectionMode::ErrorReturn,
        };
        assert!(
            saw(&harness, EffectSiteId::Event(site), phase),
            "`Event.{}`/{point}: the error-return point was not observed",
            site.name()
        );
    }
}

const KILL_SITE_ENV: &str = "UPSTROKE_TEST_KILL_SITE";
const KILL_DIR_ENV: &str = "UPSTROKE_TEST_KILL_DIR";

fn kill_coordinate() -> (EffectSiteId, SubEffectPoint) {
    let spec = std::env::var(KILL_SITE_ENV).expect("the parent named the coordinate");
    let (site, point) = spec
        .split_once('/')
        .expect("the coordinate is `<site>/<point>`");
    let site = EffectSiteId::from_name(site).expect("a site of the inventory");
    let point = site
        .sub_effects()
        .iter()
        .copied()
        .find(|candidate| candidate.name() == point)
        .expect("a point the site exposes");
    (site, point)
}

/// The child of `every_kill_point_of_the_event_funnels_kills_the_child_under_the_production_adapter`:
/// arms one kill point on the production adapter and drives the funnel to
/// it. It dies there; the export written just before the kill is the
/// observation.
#[test]
#[ignore = "spawned as a subprocess by the event kill-point witness"]
fn event_kill_child() {
    let (site, point) = kill_coordinate();
    let EffectSiteId::Event(event_site) = site else {
        panic!("`{site}` is not an event site");
    };
    let dir = PathBuf::from(std::env::var(KILL_DIR_ENV).expect("the parent named the directory"));
    let harness = shared_harness();
    harness
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .arm(site, point, InjectionMode::Kill)
        .expect("the site exposes the point in kill mode");
    let outcome = drive_event_funnel(event_site, point, &dir, &harness);
    panic!("the kill at `{site}`/{point} did not take this process: {outcome:?}");
}

/// Every kill point of the four schema-4 event sites: a child process arms
/// the point on the production adapter, drives the funnel to it and dies
/// by abort there, which is what `Injection::Kill` is.
#[test]
fn every_kill_point_of_the_event_funnels_kills_the_child_under_the_production_adapter() {
    use crate::workspace_manager::fixture;

    let points = event_points(InjectionMode::Kill);
    assert_eq!(points.len(), 9, "{points:?}");
    for (site, point) in points {
        let dir = fixture::scratch(&format!("st07-event-kill-{}-{point}", site.name()));
        let spec = format!("{}/{}", EffectSiteId::Event(site).name(), point.name());
        let status = fixture::run_kill_child(
            "engine::topology::coverage::tests::event_kill_child",
            &[
                (KILL_SITE_ENV, spec.as_ref()),
                (KILL_DIR_ENV, dir.as_os_str()),
            ],
        );
        assert!(
            fixture::died_by_abort(&status),
            "`Event.{}`/{point}: the child did not die by the kill: {status:?}",
            site.name()
        );
    }
}

/// A request the host runner can run to completion: this test binary,
/// asked for a test that does not exist, exits 0 at once.
fn trivial_request(workspace: &Path) -> crate::runner::RunnerRequest {
    let exe = std::env::current_exe().expect("this test binary");
    let mut command = crate::runner::CommandSpec::new(exe.to_string_lossy().into_owned());
    command.args = vec![
        "--exact".to_owned(),
        "a_test_this_tree_does_not_contain_and_never_will".to_owned(),
        "--ignored".to_owned(),
    ];
    crate::runner::gate_request(
        command,
        workspace.to_path_buf(),
        std::time::Duration::from_secs(60),
        crate::runner::InvocationId::attempt(
            crate::topology::registry::TaskKey(0),
            crate::topology::events::GenerationId(0),
            crate::topology::events::AttemptNumber(1),
            crate::runner::invocation::AttemptRole::Gate(1),
            0,
        ),
    )
}

/// The child of `every_kill_point_of_the_process_funnel_kills_the_child_on_this_host`:
/// arms one `Process.Spawn` point in kill mode on the production adapter and
/// runs one command through the host runner, which consults the point on the
/// parent side and dies there. The ambient-job point is consulted by the
/// containment step rather than the spawn, so that one is driven through
/// `contain_write_command`.
#[test]
#[ignore = "spawned as a subprocess by the process kill-point witness"]
fn spawn_kill_child() {
    let (site, point) = kill_coordinate();
    assert_eq!(site, crate::runner::SPAWN_SITE);
    let dir = PathBuf::from(std::env::var(KILL_DIR_ENV).expect("the parent named the directory"));
    let harness = shared_harness();
    harness
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .arm(site, point, InjectionMode::Kill)
        .expect("the site exposes the point in kill mode");
    let mut hooks = crate::runner::HarnessHooks::new(Arc::clone(&harness));
    if point == SubEffectPoint::AmbientJobJoined {
        let outcome = crate::runner::host::contain_write_command(&mut hooks);
        panic!("the kill at `{site}`/{point} did not take this process: {outcome:?}");
    }
    let runner = crate::runner::host::HostRunner::new().with_hooks(Box::new(hooks));
    let outcome = crate::runner::Runner::run(&runner, &trivial_request(&dir));
    panic!("the kill at `{site}`/{point} did not take this process: {outcome:?}");
}

/// Every parent-side point of `Process.Spawn` this host requires, in kill
/// mode: a child arms it on the production adapter, spawns one command
/// through the host runner and dies by abort at the point.
#[test]
fn every_kill_point_of_the_process_funnel_kills_the_child_on_this_host() {
    use crate::workspace_manager::fixture;

    let site = crate::runner::SPAWN_SITE;
    let points: Vec<SubEffectPoint> = required_phases(site, Host::current())
        .into_iter()
        .filter_map(|phase| match phase {
            EntryPhase::Point {
                point,
                mode: InjectionMode::Kill,
            } => Some(point),
            _ => None,
        })
        .collect();
    assert_eq!(points.len(), 4, "{points:?}");
    for point in points {
        let dir = fixture::scratch(&format!("st07-spawn-kill-{point}"));
        let spec = format!("{}/{}", site.name(), point.name());
        let status = fixture::run_kill_child(
            "engine::topology::coverage::tests::spawn_kill_child",
            &[
                (KILL_SITE_ENV, spec.as_ref()),
                (KILL_DIR_ENV, dir.as_os_str()),
            ],
        );
        assert!(
            fixture::died_by_abort(&status),
            "`{site}`/{point}: the child did not die by the kill: {status:?}"
        );
    }
}

/// `Process.Spawn`'s one error-return point, `ambient_job_joined`, exists on
/// Windows only: the containment step refuses with the injected error before
/// anything is spawned, and the point is observed in that mode.
#[cfg(windows)]
#[test]
fn the_ambient_job_join_error_return_point_fires_under_the_process_funnel() {
    let site = crate::runner::SPAWN_SITE;
    let harness = shared_harness();
    harness
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .arm(
            site,
            SubEffectPoint::AmbientJobJoined,
            InjectionMode::ErrorReturn,
        )
        .expect("the site exposes the point in this mode");
    let mut hooks = crate::runner::HarnessHooks::new(Arc::clone(&harness));
    let outcome = crate::runner::host::contain_write_command(&mut hooks);
    assert!(outcome.is_err(), "the containment step did not refuse");
    assert!(saw(
        &harness,
        site,
        HookPhase::Point {
            point: SubEffectPoint::AmbientJobJoined,
            mode: InjectionMode::ErrorReturn,
        }
    ));
}

/// `Container.WriteIntent`, `Container.MountGitView`, `Container.Create` and
/// `Container.Start`: the launch half of the container funnels, driven
/// through the production adapter against the fake runtime. The sequential
/// topology's own suites reach only the stop-and-remove half under the
/// harness.
#[test]
fn the_container_launch_funnels_execute_both_phases_under_the_production_adapter() {
    use crate::runner::container::intent::{ContainerIntent, ContainerName};
    use crate::runner::container::runtime::{ContainerTrace, CreateSpec, Mount};
    use crate::runner::container::{
        DisposableDirView, FakeRuntime, GitViewRequest, HarnessHooks, LaunchPlan, launch,
    };
    use crate::topology::effects::ContainerSite;
    use crate::workspace_manager::fixture;

    const REPO_KEY: &str = "0123456789abcdef";
    const RUN: &str = "01KZST07000000000000000000";
    const INCARNATION: &str = "01KZST07000000000000000001";
    const IMAGE_ID: &str =
        "sha256:1111111111111111111111111111111111111111111111111111111111111111";

    let root = fixture::scratch("st07-container-launch");
    let trace = ContainerTrace::recording();
    let runtime = FakeRuntime::new(trace.clone());
    runtime.add_image(IMAGE_ID, None);
    let invocation = crate::runner::InvocationId::attempt(
        crate::topology::registry::TaskKey(0),
        crate::topology::events::GenerationId(0),
        crate::topology::events::AttemptNumber(1),
        crate::runner::invocation::AttemptRole::Gate(1),
        0,
    );
    let intent = ContainerIntent {
        run_id: RUN.to_owned(),
        run_dir: format!("/srv/public/{RUN}"),
        incarnation: INCARNATION.to_owned(),
        repo_key: REPO_KEY.to_owned(),
        invocation: invocation.render(),
        runner_policy_sha256:
            "sha256:4444444444444444444444444444444444444444444444444444444444444444".to_owned(),
    };
    let name = ContainerName::new(REPO_KEY, RUN, INCARNATION, &invocation).expect("a name");
    let spec = CreateSpec {
        name: name.as_str().to_owned(),
        image_id: IMAGE_ID.to_owned(),
        labels: intent.labels(&root),
        mounts: vec![Mount::Path {
            source: PathBuf::from("/srv/work/task"),
            target: "/work".to_owned(),
            read_only: false,
        }],
        env: vec![("HOME".to_owned(), "/home/upstroke".to_owned())],
        command: vec!["/bin/sh".to_owned(), "-c".to_owned(), "exit 0".to_owned()],
        workdir: Some("/work".to_owned()),
        read_only_root: true,
    };
    let plan = LaunchPlan {
        private_root: root.clone(),
        view: GitViewRequest {
            path: root.join("views").join(name.as_str()),
            workspace: PathBuf::from("/srv/work/task"),
            head: Some("0".repeat(40)),
        },
        name,
        invocation,
        intent,
        spec,
    };
    let harness = shared_harness();
    let mut hooks = HarnessHooks::new(Arc::clone(&harness));
    let view = DisposableDirView::new(trace);
    let launched = launch(&mut hooks, &runtime, &view, &plan).expect("the fake runtime launches");
    assert_eq!(launched.name, plan.name);
    assert!(launched.intent_path.is_file(), "the intent was written");
    for site in [
        ContainerSite::WriteIntent,
        ContainerSite::MountGitView,
        ContainerSite::Create,
        ContainerSite::Start,
    ] {
        assert_both_phases(&harness, EffectSiteId::Container(site));
    }
}

// ---------------------------------------------------------------------------
// PR10 ST-07: kill sampling of the five residue-classified sites PR5's
// sampler does not run — its contract names four commands, and
// `Worktree.AddStaging`, `Snapshot.Add`, `Object.SnapshotCommitTree`,
// `Object.CandidateCommitTree` and `Object.RepairMaterialize` are the sites
// whose residue class has synthetic evidence and no sampling record at the
// base of this branch. The registry's residue entries need both halves for
// every class, so this samples each site's own command, through the argv
// its funnel shares with it, and writes the machine-varying half to
// `effects/residue-histogram-sequential.json` the way PR5's sampler writes
// `effects/residue-histogram.json`.
// ---------------------------------------------------------------------------

const REMAINING_SAMPLING_N: u32 = 8;

fn remaining_residue_sites() -> Vec<EffectSiteId> {
    use crate::topology::effects::ObjectSite;
    vec![
        EffectSiteId::Worktree(WorktreeSite::AddStaging),
        EffectSiteId::Snapshot(SnapshotSite::Add),
        EffectSiteId::Object(ObjectSite::SnapshotCommitTree),
        EffectSiteId::Object(ObjectSite::CandidateCommitTree),
        EffectSiteId::Object(ObjectSite::RepairMaterialize),
    ]
}

struct RemainingSample {
    argv: Vec<String>,
    after: std::time::Duration,
    fired: Option<std::time::Duration>,
    killed: bool,
    failed: Option<Option<i32>>,
    class: Option<crate::topology::effects::ObjectResidue>,
    recovered: bool,
}

fn remaining_slot(
    site: EffectSiteId,
    fixture: &crate::workspace_manager::fixture::Fixture,
    run: u32,
) -> crate::workspace_manager::Slot {
    use crate::workspace_manager::{Slot, SnapshotName};
    match site {
        EffectSiteId::Worktree(WorktreeSite::AddStaging) => Slot::Staging {
            sequence: 100 + u64::from(run),
        },
        EffectSiteId::Snapshot(SnapshotSite::Add) => Slot::Snapshot {
            name: SnapshotName::gates(run, 1),
        },
        _ => fixture.task("sample", run),
    }
}

/// The sampled command of one site at `slot`, prepared the way the funnel
/// finds it — the worktree added and populated where the command runs
/// inside one — and where it runs: `(argv, cwd, the residue target's
/// worktree)`.
fn prepare_remaining_sample(
    site: EffectSiteId,
    fixture: &crate::workspace_manager::fixture::Fixture,
    slot: &crate::workspace_manager::Slot,
) -> (Vec<String>, PathBuf, PathBuf) {
    use crate::topology::effects::ObjectSite;
    use crate::workspace_manager::fixture::{git, write_file};
    use crate::workspace_manager::{NoHooks, WorkspaceManager};

    fixture
        .manager
        .write_intent(&mut NoHooks, slot)
        .expect("the slot's intent");
    let path = fixture.manager.slot_path(slot);
    match site {
        EffectSiteId::Worktree(WorktreeSite::AddStaging)
        | EffectSiteId::Snapshot(SnapshotSite::Add) => {
            let mut argv: Vec<String> = WorkspaceManager::WORKTREE_ADD_ARGV
                .iter()
                .map(|arg| (*arg).to_owned())
                .collect();
            argv.push(path.to_string_lossy().into_owned());
            argv.push(fixture.head.clone());
            (argv, fixture.base.clone(), path)
        }
        EffectSiteId::Object(ObjectSite::SnapshotCommitTree | ObjectSite::CandidateCommitTree) => {
            fixture
                .manager
                .add_worktree(&mut NoHooks, slot, &fixture.head)
                .expect("the worktree the tree is written in");
            for directory in 0..40 {
                for index in 0..20 {
                    write_file(
                        &path.join(format!("bulk{directory}/f{index}.txt")),
                        format!("{directory}-{index}-{}", "x".repeat(1024)).as_bytes(),
                    );
                }
            }
            git(&path, &["add", "-A"]);
            let tree = git(&path, &["write-tree"]);
            let message = if site == EffectSiteId::Object(ObjectSite::SnapshotCommitTree) {
                "upstroke: ephemeral snapshot input"
            } else {
                "upstroke: sampled candidate"
            };
            let argv = vec![
                WorkspaceManager::COMMIT_TREE_ARGV[0].to_owned(),
                tree.trim().to_owned(),
                WorkspaceManager::COMMIT_TREE_PARENT_FLAG.to_owned(),
                fixture.head.clone(),
                WorkspaceManager::COMMIT_TREE_MESSAGE_FLAG.to_owned(),
                message.to_owned(),
            ];
            (argv, fixture.base.clone(), fixture.base.clone())
        }
        EffectSiteId::Object(ObjectSite::RepairMaterialize) => {
            fixture
                .manager
                .add_worktree(&mut NoHooks, slot, &fixture.head)
                .expect("the repair worktree");
            git(&path, &["read-tree", "--reset", "-u", "HEAD"]);
            let mut argv: Vec<String> = WorkspaceManager::REPAIR_CHERRY_PICK_ARGV
                .iter()
                .map(|arg| (*arg).to_owned())
                .collect();
            argv.push(fixture.side.clone());
            (argv, path.clone(), path)
        }
        other => panic!("`{other}` is not one of the five remaining residue sites"),
    }
}

fn remaining_recovered(
    fixture: &crate::workspace_manager::fixture::Fixture,
    slot: &crate::workspace_manager::Slot,
) -> bool {
    use crate::workspace_manager::NoHooks;
    fixture
        .manager
        .remove_worktree(&mut NoHooks, slot)
        .expect("forced removal converges");
    fixture
        .manager
        .remove_intent(&mut NoHooks, slot)
        .expect("intent removal converges");
    let path = fixture.manager.slot_path(slot);
    !path.exists()
        && !fixture
            .manager
            .worktree_records()
            .expect("records")
            .iter()
            .any(|record| crate::util::same_path(record.path(), &path))
}

fn sample_remaining_site(site: EffectSiteId) -> Vec<RemainingSample> {
    use crate::workspace_manager::fixture::{
        Fixture, KillBudget, KillableGitChild, died_by_kill, time_git,
    };
    use crate::workspace_manager::{ResidueTarget, classify_object_residue};

    let fixture = Fixture::created(&format!("st07-sample-{}", site.variant().to_lowercase()));
    let mut probes = Vec::new();
    for probe in 0..3 {
        let slot = remaining_slot(site, &fixture, 900 + probe);
        let (argv, cwd, _) = prepare_remaining_sample(site, &fixture, &slot);
        probes.push(time_git(&cwd, &argv));
        assert!(
            remaining_recovered(&fixture, &slot),
            "the probe slot is removed"
        );
    }
    // One fixed timescale for the whole ladder, so the N rungs are N
    // distinct, increasing fractions of one uninterrupted run; a ladder
    // recalibrated from the samples' own completions can step backwards.
    let budget = KillBudget::probed(&probes).probe();

    let mut samples = Vec::new();
    for run in 0..REMAINING_SAMPLING_N {
        let slot = remaining_slot(site, &fixture, run);
        let (argv, cwd, worktree) = prepare_remaining_sample(site, &fixture, &slot);
        let after = budget.mul_f64(f64::from(run + 1) / f64::from(REMAINING_SAMPLING_N + 1));
        let mut child = KillableGitChild::spawn(&cwd, &argv);
        let _ran = child.run_until(after);
        let status = child.wait();
        let target = ResidueTarget::new(&fixture.base)
            .at(&worktree)
            .from_base(&fixture.head);
        let class = classify_object_residue(site, &target).ok();
        let recovered = remaining_recovered(&fixture, &slot);
        samples.push(RemainingSample {
            argv,
            after,
            fired: child.fired(),
            killed: died_by_kill(&status),
            failed: (!status.success() && !died_by_kill(&status)).then(|| status.code()),
            class,
            recovered,
        });
    }
    samples
}

/// `decisions.effect_site_inventory.outputs`: "every residue class has
/// synthetic and sampling evidence". The five sites PR5's four-command
/// sampler leaves without a sampling record are kill-sampled here, each
/// through its own funnel's argv, every residue classified into a legal
/// class and recovered by the tabled action; the histogram is written to
/// `effects/residue-histogram-sequential.json` for the registry to embed.
#[test]
fn sampled_git_child_kills_of_the_remaining_residue_sites_are_classified_and_recovered() {
    use crate::topology::effects::ObjectResidue;

    let mut per_site = Vec::new();
    for site in remaining_residue_sites() {
        let samples = sample_remaining_site(site);
        assert_eq!(samples.len(), REMAINING_SAMPLING_N as usize);
        let counted = |wanted: ObjectResidue| -> u32 {
            u32::try_from(
                samples
                    .iter()
                    .filter(|sample| sample.class == Some(wanted))
                    .count(),
            )
            .expect("a sample count fits in u32")
        };
        let histogram = ClassHistogram {
            none: counted(ObjectResidue::None),
            internal: counted(ObjectResidue::Internal),
            after: counted(ObjectResidue::After),
        };
        let unclassified = u32::try_from(
            samples
                .iter()
                .filter(|sample| sample.class.is_none())
                .count(),
        )
        .expect("a sample count fits in u32");
        assert_eq!(
            histogram.total() + u64::from(unclassified),
            u64::from(REMAINING_SAMPLING_N),
            "{site}: every sample is accounted for by exactly one class"
        );
        assert_eq!(
            unclassified, 0,
            "{site}: an unclassifiable residue is durable state no tabled action recovers"
        );
        assert!(
            samples.iter().all(|sample| sample.recovered),
            "{site}: every sample recovered by its classified action"
        );
        let failed: Vec<Option<i32>> = samples.iter().filter_map(|sample| sample.failed).collect();
        assert!(
            failed.is_empty(),
            "{site}: a sampled child neither died by the kill nor reached its own successful \
             exit (codes {failed:?}), so what the classifier saw is this fixture's failure"
        );
        let first = &samples[0].argv;
        assert!(
            samples.iter().all(|sample| sample.argv[0] == first[0]),
            "{site}: every sample ran the site's own command"
        );
        let delays: Vec<std::time::Duration> = samples.iter().map(|sample| sample.after).collect();
        assert!(
            delays.windows(2).all(|rungs| rungs[0] < rungs[1]) || delays.len() < 2,
            "{site}: the kills are aimed at increasing points through the command: {delays:?}"
        );
        for sample in &samples {
            if let Some(fired) = sample.fired {
                assert!(
                    fired >= sample.after,
                    "{site}: a kill fired {fired:?} after its child was spawned, sooner than \
                     the {:?} rung it was aimed at",
                    sample.after
                );
            }
        }
        let killed = samples.iter().filter(|sample| sample.killed).count();
        per_site.push((site, histogram, killed));
    }
    assert_eq!(per_site.len(), 5);
    let landed: usize = per_site.iter().map(|(_, _, killed)| *killed).sum();
    assert!(
        landed > 0,
        "not one of the {} sampled Git children died by the kill, so this sampled the residue \
         its commands left when they finished",
        5 * REMAINING_SAMPLING_N
    );

    let emitted = serde_json::to_string_pretty(&serde_json::json!({
        "note": "decisions.effect_site_inventory.outputs, the observed-class histogram half \
                 for the five residue-classified sites PR5's four-command sampler does not \
                 run: written by engine::topology::coverage::tests::\
                 sampled_git_child_kills_of_the_remaining_residue_sites_are_classified_and_recovered \
                 on every run. Machine-varying by construction -- which class a sample lands \
                 in is a race between the kill and Git -- so it is emitted here rather than \
                 pinned; effects/sequential-registry.json embeds the histogram this file \
                 held when it was regenerated, and the registry's pin compares everything \
                 but these counts.",
        "sampling_n": REMAINING_SAMPLING_N,
        "sites": per_site
            .iter()
            .map(|(site, histogram, killed)| serde_json::json!({
                "site": site.name(),
                "n": REMAINING_SAMPLING_N,
                "none": histogram.none,
                "internal": histogram.internal,
                "after": histogram.after,
                "unclassified": 0,
                "recovered": true,
                "killed": killed,
            }))
            .collect::<Vec<_>>(),
    }))
    .expect("the histogram serializes");
    crate::workspace_manager::fixture::write_file(
        &repo_root().join(crate::effects::SEQUENTIAL_RESIDUE_HISTOGRAM_JSON),
        format!("{emitted}\n").as_bytes(),
    );
}
