use std::path::{Path, PathBuf};

use super::*;
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

#[test]
fn the_range_is_claimed_at_every_required_phase_on_both_hosts() {
    let registry = registry().expect("every claim is an entry the format accepts");
    for host in Host::ALL {
        for site in range() {
            for phase in required_phases(site, *host) {
                assert!(
                    CLAIMS
                        .iter()
                        .any(|claim| claim.site == site && claim.phase == phase),
                    "{host}: `{site}` has no claim for `{phase}`"
                );
            }
        }
    }
    for claim in CLAIMS {
        assert!(
            range().contains(&claim.site),
            "`{}` is claimed and is not in the range",
            claim.site
        );
        assert!(
            claim.site.scope().is_claimed(),
            "`{}` is outside ST-07's scope",
            claim.site
        );
    }
    assert_eq!(registry.entries().len(), CLAIMS.len());
    let mut keys: Vec<(EffectSiteId, EntryPhase)> = CLAIMS
        .iter()
        .map(|claim| (claim.site, claim.phase))
        .collect();
    let before = keys.len();
    keys.sort_by_key(|(site, phase)| format!("{site}/{phase}"));
    keys.dedup();
    assert_eq!(keys.len(), before, "two claims for one coordinate");
}

#[test]
fn every_claim_names_a_test_defined_exactly_once_under_src() {
    let mut names: Vec<&str> = CLAIMS.iter().map(|claim| claim.test).collect();
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

#[test]
fn the_sequential_registry_is_pinned() {
    let generated = registry_json().expect("the document serializes");
    let pinned = std::fs::read_to_string(repo_root().join(REGISTRY_JSON))
        .expect("effects/sequential-registry.json is tracked")
        .replace("\r\n", "\n");
    assert!(
        generated == pinned,
        "`{REGISTRY_JSON}` is not the document `coverage::registry_json` generates; regenerate it \
         with `cargo test --lib -- --ignored engine::topology::coverage::tests::write_the_sequential_registry`"
    );
    let parsed: RegistryDocument =
        serde_json::from_str(&pinned).expect("the pinned document parses back");
    assert_eq!(parsed.entries.len(), CLAIMS.len());
    assert_eq!(
        parsed.range,
        range()
            .into_iter()
            .map(EffectSiteId::name)
            .collect::<Vec<_>>()
    );
}

#[test]
#[ignore = "rewrites effects/sequential-registry.json from the claims; run on purpose"]
fn write_the_sequential_registry() {
    let generated = registry_json().expect("the document serializes");
    crate::workspace_manager::fixture::write_file(
        &repo_root().join(REGISTRY_JSON),
        generated.as_bytes(),
    );
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
    assert!(
        check_frozen_n(registry().expect("builds").entries(), &declarations).is_empty(),
        "the range's document cites no N"
    );
    assert!(
        range().iter().all(|site| site.residue_classes().is_empty()),
        "no site in the range registers a residue class"
    );
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

/// ST-07's merge check over the range: run the suite with
/// `UPSTROKE_HOOK_OBSERVATIONS=<dir>` first (the kill children inherit it
/// through the parent tests), then this test with the same variable. The
/// bijection over the range must be empty, every claim's named test must be
/// the one whose export shows the coordinate executed, and every recovery-
/// proven entry's N must be the declarations'.
#[test]
#[ignore = "reads the observation export a full suite run wrote under UPSTROKE_HOOK_OBSERVATIONS"]
fn st07_the_sequential_range_is_a_bijection_over_the_exported_observations() {
    let dir = std::env::var(OBSERVATIONS_ENV)
        .unwrap_or_else(|_| panic!("{OBSERVATIONS_ENV} names the directory a suite run exported"));
    let records = load_observations(Path::new(&dir)).expect("the export loads");
    assert!(
        records.len() > 100,
        "the export holds {} records; a full run of the engine::topology suite writes more",
        records.len()
    );
    let harness = harness_from(&records);
    let registry = registry().expect("the claims build");
    let inventory = range();
    let failures = check_bijection(&inventory, &harness, registry.entries(), Host::current());
    assert!(failures.is_empty(), "{failures:#?}");
    assert!(
        !failures
            .iter()
            .any(|failure| matches!(failure, BijectionFailure::Unobserved { .. }))
    );

    let mut unwitnessed = Vec::new();
    for claim in CLAIMS {
        let Some(phase) = claim.hook_phase() else {
            continue;
        };
        let witnessed = records
            .iter()
            .find(|record| record.test == claim.test)
            .is_some_and(|record| record.observed(claim.site, phase));
        if !witnessed {
            unwitnessed.push(format!("{}/{} by {}", claim.site, claim.phase, claim.test));
        }
    }
    assert!(
        unwitnessed.is_empty(),
        "claims whose named test did not execute the coordinate in this export:\n{}",
        unwitnessed.join("\n")
    );

    let declarations = std::fs::read_to_string(repo_root().join("effects/residue-classes.json"))
        .expect("the declarations are tracked");
    let problems = check_frozen_n(registry.entries(), &declarations);
    assert!(problems.is_empty(), "{problems:#?}");

    if let Ok(path) = std::env::var("UPSTROKE_ST07_SUMMARY") {
        let summary = serde_json::json!({
            "records": records.len(),
            "range": inventory.iter().copied().map(EffectSiteId::name).collect::<Vec<_>>(),
            "entries": registry.entries().len(),
            "host": Host::current().name(),
            "failures": failures.len(),
            "witnessed_claims": CLAIMS.len(),
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
