//! Extended notes: `docs/internals/effects/tests.md`

// Allowlist placement: the funnel section of `effects/allowlist.toml`, which

#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::disallowed_macros
)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::census_domain::{CrateRoots, InventoryRefusal};
use super::{
    ALLOWLIST_TOML, CLIPPY_TOML, DENIAL_CONTROL, DENIAL_FIXTURES, EFFECT_SITES_JSON,
    FROZEN_LEGACY_ALLOWLIST, FUNNEL_MODULES_JSON, REGENERATE, RESIDUE_CLASSES_JSON,
    TOPOLOGY_MODULES, USED_GOVERNED_LINTS, WRAPPERS_TOML, blank_comments,
    blank_comments_and_strings, governed_allows, legacy_growth, normalize_lint, production_region,
    topology_modules_among,
};
use crate::topology::effects::{EffectSiteId, effect_sites, effect_sites_json};

mod policy;

use policy::{PACKET_PRIMITIVES, PACKET_TYPES, host_conditional_paths, marker_before};

mod classification;

use classification::checks;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub(in crate::effects) fn crate_roots() -> &'static CrateRoots {
    static ROOTS: std::sync::OnceLock<CrateRoots> = std::sync::OnceLock::new();
    ROOTS.get_or_init(|| crate_roots_of(&repo_root()).unwrap_or_else(|refusal| panic!("{refusal}")))
}

pub(in crate::effects) fn crate_roots_of(
    manifest_dir: &Path,
) -> Result<CrateRoots, InventoryRefusal> {
    let manifest = manifest_dir.join("Cargo.toml");
    CrateRoots::from_metadata_json(&cargo_metadata_json(&manifest)?, &manifest)
}

fn cargo_metadata_json(manifest: &Path) -> Result<String, InventoryRefusal> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = std::process::Command::new(cargo)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
        ])
        .arg("--manifest-path")
        .arg(manifest)
        .output()
        .map_err(|error| InventoryRefusal::NotRun {
            manifest: manifest.to_path_buf(),
            why: error.to_string(),
        })?;
    if !output.status.success() {
        return Err(InventoryRefusal::Failed {
            manifest: manifest.to_path_buf(),
            status: output.status.to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    String::from_utf8(output.stdout).map_err(|error| InventoryRefusal::Unreadable {
        manifest: manifest.to_path_buf(),
        why: error.to_string(),
    })
}

fn scanned_sources() -> Vec<(String, String)> {
    fn walk(dir: &Path, into: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        let mut paths: Vec<PathBuf> = entries.map(|e| e.expect("entry").path()).collect();
        paths.sort();
        for path in paths {
            if path.is_dir() {
                walk(&path, into);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                into.push(path);
            }
        }
    }
    let root = repo_root();
    let mut files = Vec::new();
    walk(&root.join("src"), &mut files);
    walk(&root.join("examples"), &mut files);
    assert!(files.len() > 30, "the walk found the tree: {}", files.len());
    files
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(&root)
                .expect("under the manifest")
                .to_string_lossy()
                .replace('\\', "/");
            (relative, fs::read_to_string(&path).expect("read source"))
        })
        .collect()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Allowlist {
    #[serde(default)]
    funnel: Vec<AllowlistEntry>,
    #[serde(default)]
    legacy: Vec<AllowlistEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AllowlistEntry {
    path: String,
    #[serde(default)]
    allows: Vec<String>,
    #[serde(default)]
    expect_sites: usize,
    #[serde(default)]
    absent: bool,
    packet: String,
    #[serde(default)]
    review: String,
    #[serde(default)]
    legacy_effect: String,
    #[serde(default)]
    shrinks_when: String,
}

fn allowlist() -> Allowlist {
    let text =
        fs::read_to_string(repo_root().join(ALLOWLIST_TOML)).expect("effects/allowlist.toml");
    toml::from_str(&text).expect("the allowlist parses")
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClippyToml {
    #[serde(default, rename = "disallowed-methods")]
    disallowed_methods: Vec<DeniedPath>,
    #[serde(default, rename = "disallowed-types")]
    disallowed_types: Vec<DeniedPath>,
    #[serde(default, rename = "disallowed-macros")]
    disallowed_macros: Vec<DeniedPath>,
    #[serde(default, rename = "allow-expect-in-tests")]
    allow_expect_in_tests: bool,
    #[serde(default, rename = "allow-panic-in-tests")]
    allow_panic_in_tests: bool,
    #[serde(default, rename = "allow-print-in-tests")]
    allow_print_in_tests: bool,
}

#[test]
fn clippy_toml_turns_the_allowances_on_and_gives_unwrap_none() {
    let clippy = denylist();
    assert!(
        clippy.allow_expect_in_tests,
        "§7 allows .expect( with a message in tests"
    );
    assert!(
        clippy.allow_panic_in_tests,
        "§7 allows panic! in a test's own assertion helpers"
    );
    assert!(clippy.allow_print_in_tests, "§7 allows printing from tests");
    let text = fs::read_to_string(repo_root().join(CLIPPY_TOML)).expect("clippy.toml");
    assert!(
        !text.contains("allow-unwrap-in-tests"),
        "§7: .unwrap() has no allowance -- it is denied everywhere, tests included"
    );
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeniedPath {
    path: String,
    reason: String,
    #[serde(default, rename = "allow-invalid")]
    allow_invalid: bool,
}

impl ClippyToml {
    fn all(&self) -> impl Iterator<Item = &DeniedPath> {
        self.disallowed_methods
            .iter()
            .chain(&self.disallowed_types)
            .chain(&self.disallowed_macros)
    }

    fn paths(&self) -> BTreeSet<&str> {
        self.all().map(|entry| entry.path.as_str()).collect()
    }
}

fn denylist() -> ClippyToml {
    let text = fs::read_to_string(repo_root().join(CLIPPY_TOML)).expect("clippy.toml");
    toml::from_str(&text).expect("clippy.toml parses")
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Wrappers {
    module: Vec<ModuleClassification>,
    libc: LibcClassification,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ModuleClassification {
    path: String,
    crate_path: String,
    #[serde(default)]
    funnel: Vec<String>,
    #[serde(default)]
    effectful: Vec<String>,
    #[serde(default)]
    effectful_unnameable: Vec<String>,
    #[serde(default)]
    effect_free: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LibcClassification {
    effect: Vec<String>,
    not_an_effect: Vec<String>,
}

fn wrappers() -> Wrappers {
    let text = fs::read_to_string(repo_root().join(WRAPPERS_TOML)).expect("effects/wrappers.toml");
    toml::from_str(&text).expect("the wrapper classification parses")
}

#[test]
fn the_readiness_expectations_are_per_site_and_both_records_say_so() {
    const READINESS: &str = "src/agent/proc/test_support/readiness.rs";
    const NOTES: &str = "docs/internals/agent/proc/test_support/readiness.md";
    const LINT: &str = "clippy::disallowed_methods";
    const SITES: usize = 6;
    const DECISION: &str = "standards/02_standards_automated_baseline.md";
    const SPELLED: [&str; 8] = [
        "one", "two", "three", "four", "five", "six", "seven", "eight",
    ];
    let sites_in_words = SPELLED[SITES - 1];

    let source = fs::read_to_string(repo_root().join(READINESS)).expect("the readiness module");

    for lint in USED_GOVERNED_LINTS {
        assert_eq!(
            crate::effects::lint_levels::file_level_lint_state(&source, lint),
            Some("deny"),
            "{READINESS} must deny `{lint}` at file-module level"
        );
    }

    let found = governed_allows(&source);
    let per_site: Vec<&crate::effects::GovernedAllow> =
        found.iter().filter(|allow| !allow.module_level).collect();
    assert_eq!(
        per_site.len(),
        SITES,
        "{READINESS} carries {} per-site governed attributes: {per_site:#?}",
        per_site.len()
    );
    assert!(
        found.len() == SITES,
        "a governed attribute at module level is a file-scope allowance and this file has \
         none: {found:#?}"
    );
    for allow in &per_site {
        assert_eq!(allow.keywords, ["expect"], "{READINESS}:{}", allow.line);
        assert_eq!(allow.written, [LINT], "{READINESS}:{}", allow.line);
        assert!(allow.reasoned, "{READINESS}:{} has no reason", allow.line);
    }
    let indices: BTreeSet<usize> = (1..=SITES)
        .filter(|index| source.contains(&format!("site {index} of {SITES}")))
        .collect();
    assert_eq!(
        indices,
        (1..=SITES).collect::<BTreeSet<usize>>(),
        "each expectation's reason names which of the {SITES} sites it is"
    );

    let list = allowlist();
    let row = list
        .funnel
        .iter()
        .find(|entry| entry.path == READINESS)
        .expect("the readiness row is in the funnel section");
    assert_eq!(row.allows, vec![LINT.to_owned()]);
    assert_eq!(row.expect_sites, SITES);

    let phrase = format!("five distinct denied paths across {sites_in_words} sites");
    let shouted = phrase.to_uppercase();
    let allowlist_text =
        fs::read_to_string(repo_root().join(ALLOWLIST_TOML)).expect("the allowlist");
    let notes = fs::read_to_string(repo_root().join(NOTES)).expect("the readiness notes");
    for (record, text, needle) in [
        (NOTES, notes.as_str(), phrase.as_str()),
        (ALLOWLIST_TOML, allowlist_text.as_str(), shouted.as_str()),
    ] {
        for spelling in [text.to_owned(), text.replace('\n', "\r\n")] {
            assert!(
                spelling.lines().any(|line| line.contains(needle)),
                "{record} no longer states `{needle}` on a line of its own"
            );
        }
        assert!(
            text.contains(DECISION),
            "{record} does not cite `{DECISION}`, which is what admits the placement"
        );
    }

    assert!(
        repo_root().join(DECISION).is_file(),
        "`{DECISION}` is cited by both records and is not in the tree"
    );
}

#[test]
fn the_internals_readme_names_the_records_that_carry_the_readiness_statement() {
    const README: &str = "docs/internals/README.md";
    const NOTES: &str = "docs/internals/agent/proc/test_support/readiness.md";
    const READINESS: &str = "src/agent/proc/test_support/readiness.rs";
    const SECTION: &str = "\n## What moves\n";

    let readme = fs::read_to_string(repo_root().join(README))
        .expect("the internals README")
        .replace("\r\n", "\n");
    let (_, below) = readme
        .split_once(SECTION)
        .unwrap_or_else(|| panic!("{README} no longer has a `What moves` section"));
    let what_moves = below
        .split_once("\n## ")
        .map_or(below, |(section, _)| section);

    for record in [NOTES, ALLOWLIST_TOML] {
        assert!(
            what_moves.contains(record),
            "{README}'s `What moves` section does not name `{record}`, which is one of the two \
             records `the_readiness_expectations_are_per_site_and_both_records_say_so` reads the \
             per-site allowance statement from"
        );
    }
    assert!(
        !what_moves.contains(READINESS),
        "{README}'s `What moves` section names `{READINESS}` as prose a census reads. The \
         statement moved to `{NOTES}` and `{ALLOWLIST_TOML}`; the module keeps its marker and \
         nothing else, so a maintainer sent to the source finds no such sentence"
    );
}

fn file_level_denies(source: &str, lint: &str) -> bool {
    matches!(
        crate::effects::lint_levels::file_level_lint_state(source, lint),
        Some("deny" | "forbid")
    )
}

#[test]
fn every_allow_of_a_governed_lint_is_module_level_and_in_the_allowlist() {
    let list = allowlist();
    let recorded: BTreeMap<&str, (&AllowlistEntry, &'static str)> = list
        .funnel
        .iter()
        .map(|entry| (entry.path.as_str(), (entry, "funnel")))
        .chain(
            list.legacy
                .iter()
                .map(|entry| (entry.path.as_str(), (entry, "legacy"))),
        )
        .collect();
    assert_eq!(
        recorded.len(),
        list.funnel.len() + list.legacy.len(),
        "a path is listed in both sections, or twice in one"
    );

    let mut carried: BTreeSet<String> = BTreeSet::new();
    let mut attributes = 0;
    for (path, source) in scanned_sources() {
        let found = governed_allows(&source);
        if found.is_empty() {
            continue;
        }
        attributes += found.len();
        let Some((entry, section)) = recorded.get(path.as_str()) else {
            panic!(
                "{path} allows a governed lint and is in no section of {ALLOWLIST_TOML}: {found:#?}"
            );
        };
        carried.insert(path.clone());
        let mut per_site = 0;
        for allow in &found {
            if !allow.module_level
                && allow.keywords == ["expect"]
                && entry.expect_sites > 0
                && allow.reasoned
                && allow
                    .lints
                    .iter()
                    .all(|lint| file_level_denies(&source, lint))
            {
                per_site += 1;
                continue;
            }
            assert!(
                allow.module_level,
                "{path}:{} allows {:?} below module level; `mechanism` (2) permits it \
                 \"only as module-level attributes\", and the per-site `#[expect]` the \
                 2026-08-30 amendment admits needs a reason, a file-level deny of the same \
                 lint, and an `expect_sites` count in {ALLOWLIST_TOML}",
                allow.line, allow.lints
            );
            let marker = marker_before(&source, allow.line, allow.inner);
            assert!(
                marker.contains(ALLOWLIST_TOML),
                "{path}:{} carries no pointer to {ALLOWLIST_TOML} above the attribute",
                allow.line
            );
            let expected_marker = if *section == "legacy" {
                "LEGACY-EFFECT"
            } else {
                "funnel section"
            };
            assert!(
                marker.contains(expected_marker),
                "{path}:{} is in the {section} section and its prologue never says \
                 `{expected_marker}`",
                allow.line
            );
        }
        let written: BTreeSet<&str> = found
            .iter()
            .flat_map(|allow| allow.written.iter().map(String::as_str))
            .filter(|entry| normalize_lint(entry).is_some())
            .collect();
        let declared: BTreeSet<&str> = entry.allows.iter().map(String::as_str).collect();
        assert_eq!(
            written, declared,
            "{path}: the attribute allows {written:?} and {ALLOWLIST_TOML} records {declared:?}"
        );
        assert_eq!(
            per_site, entry.expect_sites,
            "{path} carries {per_site} per-site `#[expect]` attributes and {ALLOWLIST_TOML} \
             records {}",
            entry.expect_sites
        );
    }

    for (path, (entry, _)) in &recorded {
        assert!(
            entry.expect_sites == 0 || carried.contains(*path),
            "{path} records {} per-site expectations and carries no governed attribute",
            entry.expect_sites
        );
    }

    for (path, (entry, _)) in &recorded {
        if entry.allows.is_empty() || entry.absent {
            continue;
        }
        assert!(
            carried.contains(*path),
            "{path} records allows {:?} and carries no attribute",
            entry.allows
        );
    }
    assert!(
        attributes >= 25,
        "the scan found only {attributes} governed attributes; it is measuring nothing"
    );
}

#[test]
fn the_placement_scan_refuses_an_allow_that_is_not_module_level_and_sees_through_no_disguise() {
    let on_a_function = "#[allow(clippy::disallowed_methods)]\nfn go() {}\n";
    let found = governed_allows(on_a_function);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(!found[0].module_level);

    let on_a_statement = "fn go() {\n    #[allow(clippy::disallowed_methods)]\n    let _ = 1;\n}\n";
    let found = governed_allows(on_a_statement);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(!found[0].module_level);

    let on_a_module = "#[allow(clippy::disallowed_methods)]\nmod inner { }\n";
    let found = governed_allows(on_a_module);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].module_level);

    let inner = "//! doc\n#![allow(clippy::disallowed_types)]\nfn go() {}\n";
    let found = governed_allows(inner);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].inner && found[0].module_level);

    let late = "fn go() {}\n#![allow(clippy::disallowed_types)]\n";
    let found = governed_allows(late);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(!found[0].module_level);

    let expected = "#![expect(clippy::disallowed_macros)]\n";
    assert_eq!(governed_allows(expected).len(), 1);

    assert!(governed_allows("#![allow(clippy::too_many_arguments)]\n").is_empty());
    assert!(governed_allows("#![allow(unused_variables)]\n").is_empty());

    let disguised = concat!(
        "//! ```\n",
        "//! #![allow(clippy::disallowed_methods)]\n",
        "//! ```\n",
        "// #![allow(clippy::disallowed_types)]\n",
        "/* #![allow(clippy::disallowed_macros)] */\n",
        "const FIXTURE: &str = \"#![allow(clippy::disallowed_methods)]\";\n",
        "const RAW: &str = r#\"#![allow(clippy::disallowed_types)]\"#;\n",
    );
    assert!(
        governed_allows(disguised).is_empty(),
        "{:#?}",
        governed_allows(disguised)
    );
    let blanked = blank_comments_and_strings(disguised);
    assert_eq!(blanked.len(), disguised.len(), "offsets are preserved");
    assert_ne!(blanked, disguised, "the blanking is a no-op");
    assert!(!blanked.contains("disallowed_methods"));

    let mixed = format!("{disguised}#![allow(clippy::disallowed_macros)]\n");
    let found = governed_allows(&mixed);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].lints, vec!["disallowed_macros".to_owned()]);

    let mechanisms = 9;
    assert_eq!(mechanisms, 9);
}

#[test]
fn the_three_blunt_governed_lints_are_used_by_nobody() {
    let mut blunt = Vec::new();
    for (path, source) in scanned_sources() {
        for allow in governed_allows(&source) {
            for lint in &allow.lints {
                if matches!(lint.as_str(), "style" | "all" | "warnings") {
                    blunt.push(format!("{path}:{} {lint}", allow.line));
                }
            }
        }
    }
    assert!(blunt.is_empty(), "{blunt:#?}");

    for probe in [
        "#![allow(warnings)]\n",
        "#![allow(clippy::all)]\n",
        "#![allow(clippy::style)]\n",
    ] {
        assert_eq!(governed_allows(probe).len(), 1, "{probe}");
    }

    let list = allowlist();
    let used: BTreeSet<&str> = list
        .funnel
        .iter()
        .chain(&list.legacy)
        .flat_map(|entry| entry.allows.iter().map(String::as_str))
        .collect();
    let expected: BTreeSet<&str> = USED_GOVERNED_LINTS.iter().copied().collect();
    assert_eq!(used, expected);
}

#[test]
fn cargo_toml_declares_no_lint_table_that_could_allow_a_governed_lint() {
    let text = fs::read_to_string(repo_root().join("Cargo.toml")).expect("Cargo.toml");
    let manifest: toml::Value = toml::from_str(&text).expect("Cargo.toml parses");
    let Some(lints) = manifest.get("lints") else {
        return;
    };
    let rendered = lints.to_string();
    for lint in super::GOVERNED_LINTS {
        assert!(
            !rendered.contains(lint),
            "Cargo.toml [lints] names the governed lint `{lint}`: {rendered}"
        );
    }
}

#[test]
fn the_legacy_section_is_frozen_and_may_only_shrink() {
    let list = allowlist();
    let current: Vec<&str> = list.legacy.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(
        legacy_growth(FROZEN_LEGACY_ALLOWLIST, &current),
        Vec::<&str>::new(),
        "the legacy section grew past the frozen list"
    );
    assert_eq!(
        current.len(),
        FROZEN_LEGACY_ALLOWLIST.len(),
        "PR5 freezes the list at exactly what it ships"
    );

    let grown: Vec<&str> = current.iter().copied().chain(["src/catalog.rs"]).collect();
    assert_eq!(
        legacy_growth(FROZEN_LEGACY_ALLOWLIST, &grown),
        vec!["src/catalog.rs"]
    );
    let shrunk: Vec<&str> = current.iter().copied().skip(1).collect();
    assert!(legacy_growth(FROZEN_LEGACY_ALLOWLIST, &shrunk).is_empty());

    let frozen: BTreeSet<&str> = FROZEN_LEGACY_ALLOWLIST.iter().copied().collect();
    let listed: BTreeSet<&str> = current.iter().copied().collect();
    assert_eq!(frozen, listed);
}

#[test]
fn the_legacy_section_never_contains_a_topology_module() {
    let list = allowlist();
    let current: Vec<&str> = list.legacy.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(
        topology_modules_among(&current),
        Vec::<&str>::new(),
        "a topology module is in the frozen legacy section"
    );

    let probes = [
        "src/topology/registry.rs",
        "src/runner/mod.rs",
        "src/workspace_manager.rs",
        "src/workspace_manager/residue.rs",
        "src/engine/topology.rs",
        "src/engine/topology/create.rs",
    ];
    for probe in probes {
        assert_eq!(
            topology_modules_among(&[probe]),
            vec![probe],
            "`{probe}` is a topology module and the check missed it"
        );
    }
    assert_eq!(
        probes.len(),
        TOPOLOGY_MODULES.len(),
        "one probe per banned shape"
    );

    let sentence_shapes = [
        "src/topology/",
        "src/runner/",
        "src/workspace_manager.rs",
        "src/engine/topology.rs",
    ];
    let submodule = "src/engine/topology/create.rs";
    assert!(
        !submodule.starts_with("src/engine/topology.rs"),
        "the prefix relation this entry exists for no longer holds"
    );
    assert!(
        !sentence_shapes
            .iter()
            .any(|banned| submodule.starts_with(banned) || *banned == submodule),
        "the four shapes the packet sentence names already cover `{submodule}`, \
         so the fifth entry is dead weight and should be removed"
    );

    let before_the_split = [
        "src/topology/",
        "src/runner/",
        "src/workspace_manager.rs",
        "src/engine/topology.rs",
        "src/engine/topology/",
    ];
    let child = "src/workspace_manager/residue.rs";
    assert!(
        !child.starts_with("src/workspace_manager.rs"),
        "the prefix relation this entry exists for no longer holds"
    );
    assert!(
        !before_the_split
            .iter()
            .any(|banned| child.starts_with(banned) || *banned == child),
        "the shapes that predate the `m4-workspace` split already cover \
         `{child}`, so the `src/workspace_manager/` entry is dead weight and \
         should be removed"
    );

    let funnel: BTreeSet<&str> = list.funnel.iter().map(|e| e.path.as_str()).collect();
    for expected in [
        "src/workspace_manager.rs",
        "src/runner/host.rs",
        "src/runner/invocation.rs",
        "src/topology/effects.rs",
    ] {
        assert!(
            funnel.contains(expected),
            "{expected} left the funnel section"
        );
    }
}

#[test]
fn every_allowlist_entry_carries_its_justification_and_names_a_real_file() {
    let list = allowlist();
    let mut absent = Vec::new();
    for entry in &list.funnel {
        assert!(
            !entry.review.trim().is_empty(),
            "{} has no funnel review clause",
            entry.path
        );
        assert!(!entry.packet.trim().is_empty(), "{}", entry.path);
    }
    for entry in &list.legacy {
        assert!(
            entry.legacy_effect.contains("LEGACY-EFFECT"),
            "{} carries no LEGACY-EFFECT justification",
            entry.path
        );
        assert!(
            !entry.shrinks_when.trim().is_empty(),
            "{} does not say when it shrinks",
            entry.path
        );
    }
    for entry in list.funnel.iter().chain(&list.legacy) {
        let exists = repo_root().join(&entry.path).is_file();
        assert_eq!(
            exists, !entry.absent,
            "{} is marked absent={} and exists={exists}",
            entry.path, entry.absent
        );
        if entry.absent {
            absent.push(entry.path.as_str());
            assert!(
                entry.allows.is_empty(),
                "{} is absent and cannot carry an attribute",
                entry.path
            );
        }
    }
    assert_eq!(absent, Vec::<&str>::new(), "the absent set moved");
    assert!(
        repo_root().join("src/runner/container.rs").is_file(),
        "the Container funnel is the entry that used to be absent; if it is gone \
         again, this assertion is the one that says so rather than an empty set \
         reading as agreement"
    );
}

#[test]
fn the_denylist_names_every_primitive_the_packet_enumerates() {
    let denied = denylist();
    let methods: BTreeSet<&str> = denied
        .disallowed_methods
        .iter()
        .map(|e| e.path.as_str())
        .collect();
    let types: BTreeSet<&str> = denied
        .disallowed_types
        .iter()
        .map(|e| e.path.as_str())
        .collect();

    let missing: Vec<&str> = PACKET_PRIMITIVES
        .iter()
        .copied()
        .filter(|path| !methods.contains(path))
        .collect();
    assert!(missing.is_empty(), "disallowed-methods omits {missing:?}");

    let missing: Vec<&str> = PACKET_TYPES
        .iter()
        .copied()
        .filter(|path| !types.contains(path))
        .collect();
    assert!(missing.is_empty(), "disallowed-types omits {missing:?}");

    assert!(!denied.disallowed_methods.is_empty());
    assert!(!denied.disallowed_types.is_empty());
    assert!(
        !denied.disallowed_macros.is_empty(),
        "the macro list is the one that can be vacuous without looking it"
    );

    for entry in denied.all() {
        assert!(
            entry.reason.starts_with("UPSTROKE-EFFECT")
                || entry.reason.starts_with("UPSTROKE-WRAPPER"),
            "{} has no classified reason: {}",
            entry.path,
            entry.reason
        );
    }

    const NAMES_A_CONTAINER_RUNTIME: &[(&str, &str)] = &[
        (
            "src/effects/tests.rs",
            "this census's own needle table, which is the one place the strings \
             have to be written down",
        ),
        (
            "src/agent/proc/tests.rs",
            "the Process funnel's `#[cfg(test)]` suite, out of line since M6. \
             The reaper-reclaim tests name the runtime the cleanup reaper is \
             armed with -- the same text was inside `src/agent/proc.rs` below \
             its `#[cfg(test)]` cut and so was never in this domain; it is \
             named for the same reason `fake.rs` is, the marker being at the \
             DECLARATION and not in the file",
        ),
        (
            "src/runner/container.rs",
            "the Container funnel: `FunnelGroup::Container.module()`, the one \
             production file that may reach a container runtime, and the one \
             `Command::new(` row in `every_production_process_start_is_classified`",
        ),
        (
            "src/runner/container/exec/tests.rs",
            "the `ContainerRunner`'s `#[cfg(test)]` suite, out of line since W1. \
             The same text was inside `exec.rs` below its `#[cfg(test)]` cut and \
             so was never in this domain; it is named for the same reason \
             `fake.rs` is, the marker being at the DECLARATION and not in the file",
        ),
        (
            "src/runner/container/fake.rs",
            "the funnel's `#[cfg(test)]` substrate — the fake runtime and the \
             Docker gate. Excluded from nothing by `production_region`, because \
             the `#[cfg(test)]` marker is at the DECLARATION and not in the file",
        ),
        (
            "src/runner/container/tests.rs",
            "the funnel's `#[cfg(test)]` suite, for the same reason",
        ),
    ];
    let expected: BTreeSet<&str> = NAMES_A_CONTAINER_RUNTIME
        .iter()
        .map(|(path, _)| *path)
        .collect();
    let mut naming: BTreeSet<String> = BTreeSet::new();
    for (path, source) in scanned_sources() {
        let production = blank_comments(&production_region(&source));
        for needle in ["\"docker", "\"podman", "docker::", "bollard", "DockerCli"] {
            if production.contains(needle) {
                naming.insert(path.clone());
            }
        }
    }
    assert_eq!(
        naming,
        expected.iter().map(|p| (*p).to_owned()).collect(),
        "the set of files naming a container runtime moved. A new one is either \
         a helper the denylist does not name, or a row this table needs"
    );

    for helper in [
        "upstroke::runner::container::runtime::ContainerRuntime::create",
        "upstroke::runner::container::runtime::ContainerRuntime::start",
        "upstroke::runner::container::runtime::ContainerRuntime::stop",
        "upstroke::runner::container::runtime::ContainerRuntime::remove",
        "upstroke::runner::container::GitView::materialize",
        "upstroke::runner::container::GitView::discard",
    ] {
        assert!(
            methods.contains(helper),
            "`{helper}` is a docker invocation helper and disallowed-methods does \
             not name it"
        );
    }
}

#[test]
fn every_denied_path_this_host_can_resolve_does_resolve() {
    let scratch = scratch_dir("resolve");
    let denied_text = fs::read_to_string(repo_root().join(CLIPPY_TOML)).expect("clippy.toml");
    let stripped = denied_text.replace(", allow-invalid = true", "");
    assert_ne!(stripped, denied_text, "no allow-invalid entry to strip");
    fs::write(scratch.join(CLIPPY_TOML), &stripped).expect("the probe config");

    let unresolved = unresolved_paths(&scratch, "probe");
    let expected: BTreeSet<String> = host_conditional_paths()
        .into_iter()
        .map(str::to_owned)
        .collect();
    assert_eq!(
        unresolved, expected,
        "the set of paths this host cannot resolve moved. Anything new here is a \
         denial that enforces nothing."
    );

    let with_typo = format!("{stripped}\n[[extra]]\n",).replace("[[extra]]\n", "");
    let with_typo = with_typo.replace(
        "disallowed-methods = [",
        "disallowed-methods = [\n    { path = \"std::fs::wrrite\", reason = \"UPSTROKE-EFFECT: control\" },",
    );
    fs::write(scratch.join(CLIPPY_TOML), with_typo).expect("the control config");
    let control = unresolved_paths(&scratch, "control");
    assert!(
        control.contains("std::fs::wrrite"),
        "the control typo was not reported: {control:?}"
    );
}

fn unresolved_paths(dir: &Path, tag: &str) -> BTreeSet<String> {
    let (deps, rlib) = crate_under_test();
    let source = dir.join(format!("{tag}.rs"));
    fs::write(&source, "pub fn nothing() {}\n").expect("the probe source");
    let out = dir.join(format!("{tag}-out"));
    fs::create_dir_all(&out).expect("an output directory");
    let mut command = std::process::Command::new(clippy_driver());
    command
        .env("CLIPPY_CONF_DIR", dir)
        .args([
            "--edition",
            "2024",
            "--crate-type",
            "lib",
            "--emit=metadata",
        ])
        .arg("--out-dir")
        .arg(&out)
        .arg("-L")
        .arg(format!("dependency={}", deps.display()))
        .arg("--extern")
        .arg(format!("upstroke={}", rlib.display()));
    for (name, path) in extern_dependencies(&deps) {
        command
            .arg("--extern")
            .arg(format!("{name}={}", path.display()));
    }
    let output = command
        .arg(&source)
        .output()
        .expect("clippy-driver runs; the lint gate uses the same binary");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    stderr
        .lines()
        .filter(|line| line.contains("does not refer to a reachable"))
        .filter_map(|line| {
            let start = line.find('`')? + 1;
            let end = line[start..].find('`')? + start;
            Some(line[start..end].to_owned())
        })
        .collect()
}

fn extern_dependencies(deps: &Path) -> Vec<(String, PathBuf)> {
    let mut best: BTreeMap<String, (std::time::SystemTime, PathBuf)> = BTreeMap::new();
    let Ok(entries) = fs::read_dir(deps) else {
        return Vec::new();
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(stem) = name
            .strip_prefix("lib")
            .and_then(|n| n.strip_suffix(".rlib"))
        else {
            continue;
        };
        let Some((crate_name, _)) = stem.rsplit_once('-') else {
            continue;
        };
        if crate_name == "upstroke" {
            continue;
        }
        let stamp = path
            .metadata()
            .and_then(|meta| meta.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        let slot = best
            .entry(crate_name.replace('-', "_"))
            .or_insert((stamp, path.clone()));
        if stamp >= slot.0 {
            *slot = (stamp, path);
        }
    }
    best.into_iter()
        .map(|(name, (_, path))| (name, path))
        .collect()
}

#[test]
fn every_platform_conditional_denial_names_something_real() {
    let denied = denylist();
    let sources: String = scanned_sources()
        .into_iter()
        .map(|(_, source)| source)
        .collect();
    let mut checked = 0;
    for entry in denied.all() {
        let conditional = entry.path.starts_with("windows_sys::")
            || entry.path.starts_with("libc::")
            || entry.path.starts_with("std::os::");
        if !conditional {
            continue;
        }
        let item = entry.path.rsplit("::").next().expect("a path has an item");
        const PACKET_ONLY: &[&str] = &[
            "setsid",
            "execv",
            "execve",
            "execvp",
            "execl",
            "execle",
            "execlp",
            "soft_link",
            "symlink_file",
            "symlink_dir",
            "OpenProcess",
            "TerminateProcess",
            "ResumeThread",
            "OpenJobObjectW",
            "TerminateJobObject",
            "UnlockFileEx",
        ];
        checked += 1;
        assert!(
            sources.contains(item) || PACKET_ONLY.contains(&item),
            "`{}` names `{item}`, which appears nowhere in this tree and is not one \
             of the primitives the packet's sentence requires regardless",
            entry.path
        );
    }
    assert!(
        checked >= 30,
        "only {checked} platform-conditional denials were checked"
    );

    let suppressed: BTreeSet<&str> = denied
        .all()
        .filter(|entry| entry.allow_invalid)
        .map(|entry| entry.path.as_str())
        .collect();
    assert_eq!(
        suppressed,
        BTreeSet::from([
            "libc::pipe2",
            "std::os::unix::fs::symlink",
            "std::os::windows::fs::symlink_dir",
            "std::os::windows::fs::symlink_file",
        ]),
        "an entry bought silence about whether it resolves"
    );
}

#[test]
fn every_declared_effect_denial_refuses_for_the_reason_it_declares() {
    let scratch = scratch_dir("denial");

    let (ok, diagnostics) = lint_fixture(&scratch, "control", DENIAL_CONTROL);
    assert!(
        ok && diagnostics.is_empty(),
        "the positive control did not compile clean, so no refusal below is \
         evidence of anything:\n{diagnostics:#?}"
    );

    let mut shapes = BTreeSet::new();
    let mut lints = BTreeSet::new();
    for fixture in DENIAL_FIXTURES {
        let tag = fixture.shape.replace([' ', '-'], "_");
        let (_, diagnostics) = lint_fixture(&scratch, &tag, fixture.source);
        let emitted: BTreeSet<&str> = diagnostics.iter().map(|(lint, _)| lint.as_str()).collect();
        assert_eq!(
            emitted,
            BTreeSet::from([fixture.lint]),
            "the `{}` fixture emitted {emitted:?}, not exactly {{{}}}",
            fixture.shape,
            fixture.lint
        );
        let named = diagnostics
            .iter()
            .any(|(_, message)| message.contains(fixture.resolves_to));
        assert!(
            named,
            "the `{}` fixture was denied, but clippy's message never names `{}` -- \
             so this proves a refusal, not that the alias resolved: {diagnostics:#?}",
            fixture.shape, fixture.resolves_to
        );
        shapes.insert(fixture.shape);
        lints.insert(fixture.lint);
    }

    assert_eq!(shapes.len(), 7, "{shapes:?}");
    assert_eq!(lints.len(), 3, "{lints:?}");
    for required in [
        "renamed-import",
        "re-export",
        "function-value",
        "legacy-wrapper call",
    ] {
        assert!(
            shapes.contains(required),
            "proof_tests[4] names `{required}`"
        );
    }
}

fn lint_fixture(dir: &Path, tag: &str, body: &str) -> (bool, Vec<(String, String)>) {
    let (deps, rlib) = crate_under_test();
    let source = dir.join(format!("{tag}.rs"));
    fs::write(&source, body).expect("the fixture");
    let out = dir.join(format!("{tag}-out"));
    fs::create_dir_all(&out).expect("an output directory");
    let mut command = std::process::Command::new(clippy_driver());
    command
        .env("CLIPPY_CONF_DIR", repo_root())
        .args([
            "--edition",
            "2024",
            "--crate-type",
            "lib",
            "--emit=metadata",
            "--error-format=json",
        ])
        .arg("--out-dir")
        .arg(&out)
        .arg("-L")
        .arg(format!("dependency={}", deps.display()))
        .arg("--extern")
        .arg(format!("upstroke={}", rlib.display()));
    for (name, path) in extern_dependencies(&deps) {
        command
            .arg("--extern")
            .arg(format!("{name}={}", path.display()));
    }
    let output = command
        .arg(&source)
        .output()
        .expect("clippy-driver runs; the lint gate uses the same binary");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut diagnostics = Vec::new();
    for line in stderr.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(code) = value
            .get("code")
            .and_then(|code| code.get("code"))
            .and_then(serde_json::Value::as_str)
        else {
            continue;
        };
        if !code.starts_with("clippy::disallowed") {
            continue;
        }
        let message = value
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        diagnostics.push((code.to_owned(), message));
    }
    (output.status.success(), diagnostics)
}

fn clippy_driver() -> PathBuf {
    let sysroot = std::process::Command::new("rustc")
        .arg("--print")
        .arg("sysroot")
        .output()
        .expect("rustc runs; it built this test");
    let sysroot = PathBuf::from(String::from_utf8_lossy(&sysroot.stdout).trim().to_owned());
    let name = if cfg!(windows) {
        "clippy-driver.exe"
    } else {
        "clippy-driver"
    };
    let in_sysroot = sysroot.join("bin").join(name);
    if in_sysroot.is_file() {
        return in_sysroot;
    }
    PathBuf::from(name)
}

mod ci_model;
mod workflow;

use ci_model::{
    CI_TARGETS, CI_WORKFLOW, MSRV_COMMAND, MSRV_JOB, OVERRIDING_REPO_FILES, RUSTFLAGS_KEY,
    TEST_COMMAND, WINDOWS_TEST_FLOOR, WINDOWS_TEST_WITNESS,
};
use workflow::{
    WORKFLOW_ESCAPES, ci_msrv_job_complaints, ci_test_job_complaints,
    ci_test_windows_job_complaints, ci_windows_build_witness_complaints, ci_workflow_text,
    complaint_codes, declared_msrv_toolchain, declared_rust_version, field, field_names,
    mutate_workflow, parse_workflow, rustflags_complaints, scalar, steps_of, three_component,
    workflow_complaints,
};

#[test]
fn the_workflow_parser_rejects_duplicate_keys_and_reads_on_as_a_string() {
    let clean = "jobs:\n  lint:\n    runs-on: ubuntu-latest\n";
    let parsed = parse_workflow(clean).expect("the control document parses");
    assert_eq!(
        field(&parsed, "jobs")
            .and_then(|jobs| field(jobs, "lint"))
            .and_then(|lint| scalar(lint, "runs-on")),
        Some("ubuntu-latest"),
        "the control parsed but did not read back"
    );

    for (shape, document) in [
        (
            "a duplicated top-level key",
            "jobs:\n  a: 1\njobs:\n  b: 2\n",
        ),
        (
            "a duplicated key inside a job",
            "jobs:\n  lint:\n    runs-on: ubuntu-latest\n    runs-on: windows-latest\n",
        ),
    ] {
        let refused = parse_workflow(document);
        assert!(
            refused.is_err(),
            "{shape} was accepted. Last-one-wins makes every structural equality in this \
             section read the winning entry while a mutation hides in the loser."
        );
    }

    let doc = parse_workflow(&ci_workflow_text()).expect(CI_WORKFLOW);
    assert!(
        field_names(&doc).contains("on"),
        "the workflow's `on:` key did not read back as the string `on`: {:?}",
        field_names(&doc)
    );
}

#[test]
fn the_workflow_shape_oracle_refuses_every_escape_the_ledger_names() {
    let text = ci_workflow_text();

    let doc = parse_workflow(&text).expect(CI_WORKFLOW);
    let clean = workflow_complaints(&doc);
    assert!(
        clean.is_empty(),
        "the unmutated workflow does not satisfy its own contract:\n{}",
        clean.join("\n")
    );

    let mut refused: BTreeSet<&str> = BTreeSet::new();
    for escape in WORKFLOW_ESCAPES {
        let mutated = mutate_workflow(&text, escape.job, escape.anchor, escape.replacement);
        assert_ne!(
            mutated, text,
            "{}: the mutation changed nothing, so it measures nothing",
            escape.name
        );
        let complaints = match parse_workflow(&mutated) {
            Ok(document) => workflow_complaints(&document),
            Err(error) => vec![error],
        };
        let codes = complaint_codes(&complaints);
        assert!(
            codes.contains(escape.refused_as),
            "{} was not refused as `{}` -- {}\nComplaints: {:#?}",
            escape.name,
            escape.refused_as,
            escape.escape,
            complaints
        );
        refused.insert(escape.name);
    }
    assert_eq!(
        refused.len(),
        WORKFLOW_ESCAPES.len(),
        "two escapes share a name, so one of them was never measured"
    );
}

#[test]
fn the_workflow_that_runs_these_tests_installs_the_compiler_they_need() {
    let doc = parse_workflow(&ci_workflow_text()).expect(CI_WORKFLOW);
    let complaints = ci_test_job_complaints(&doc);
    assert!(
        complaints.is_empty(),
        "the `test` job does not run these fixtures the way they need:\n{}",
        complaints.join("\n")
    );
}

#[test]
fn the_self_hosted_windows_leg_runs_these_fixtures_on_the_pinned_labels() {
    let doc = parse_workflow(&ci_workflow_text()).expect(CI_WORKFLOW);
    let complaints = ci_test_windows_job_complaints(&doc);
    assert!(
        complaints.is_empty(),
        "the self-hosted Windows leg does not run these fixtures the way the contract pins:\n{}",
        complaints.join("\n")
    );
}

#[test]
fn the_hosted_windows_leg_still_links_every_test_binary() {
    let doc = parse_workflow(&ci_workflow_text()).expect(CI_WORKFLOW);
    let complaints = ci_windows_build_witness_complaints(&doc);
    assert!(
        complaints.is_empty(),
        "no hosted leg code-generates and links the Windows tree the way the contract pins:\n{}",
        complaints.join("\n")
    );
}

#[test]
fn no_repository_file_overrides_what_ci_compiles_or_runs() {
    let root = repo_root();
    let present: Vec<&str> = OVERRIDING_REPO_FILES
        .iter()
        .copied()
        .filter(|name| root.join(name).exists())
        .collect();
    assert!(
        present.is_empty(),
        "these files outrank `{CI_WORKFLOW}` and this contract reads only the workflow: \
         {present:?}. A toolchain file replaces the compiler every leg runs; a Cargo config \
         can bind a target runner that reports success without executing a test binary. \
         Adding one is a deliberate act: extend this contract in the same change."
    );
    let manifest: toml::Value =
        toml::from_str(&fs::read_to_string(root.join("Cargo.toml")).expect("Cargo.toml"))
            .expect("Cargo.toml parses");
    assert!(
        manifest.get("workspace").is_none(),
        "Cargo.toml declares a workspace, so `--all-targets --all-features` no longer selects \
         this crate: `default-members` decides, and a member with no tests makes every CI \
         command succeed without running this suite."
    );
}

#[test]
fn the_self_hosted_leg_counts_the_tests_it_ran() {
    assert!(
        WINDOWS_TEST_WITNESS.starts_with(TEST_COMMAND),
        "the self-hosted leg's step does not open with `{TEST_COMMAND}`, so the suite it \
         witnesses is not the suite the other legs run"
    );
    assert!(
        WINDOWS_TEST_WITNESS.contains(&format!("-lt {WINDOWS_TEST_FLOOR}")),
        "the self-hosted leg's step does not test the count against \
         {WINDOWS_TEST_FLOOR}, so the floor this contract documents is not the floor it runs"
    );
}

#[test]
fn the_msrv_leg_checks_the_floor_the_manifest_publishes_on_every_platform() {
    assert_eq!(three_component("1.85"), "1.85.0");
    assert_eq!(three_component("1.85.0"), "1.85.0");
    assert_eq!(
        three_component("nightly"),
        "nightly",
        "a manifest value this does not understand must reach the equality below unchanged \
         and fail there with both strings quoted, not be normalised into agreement"
    );

    let doc = parse_workflow(&ci_workflow_text()).expect(CI_WORKFLOW);
    let complaints = ci_msrv_job_complaints(&doc);
    assert!(
        complaints.is_empty(),
        "the `{MSRV_JOB}` job does not check the floor the way the documents publish it:\n{}",
        complaints.join("\n")
    );

    let installed: Vec<&str> = field(&doc, "jobs")
        .and_then(|jobs| field(jobs, MSRV_JOB))
        .map(steps_of)
        .unwrap_or_default()
        .iter()
        .filter_map(|step| field(step, "with").and_then(|with| scalar(with, "toolchain")))
        .collect();
    let expected = declared_msrv_toolchain();
    assert_eq!(
        installed,
        vec![expected.as_str()],
        "`Cargo.toml` publishes `rust-version = \"{}\"`, whose toolchain name is \
         `{expected}`; the `{MSRV_JOB}` leg installs {installed:?}",
        declared_rust_version()
    );

    let steps = field(&doc, "jobs")
        .and_then(|jobs| field(jobs, MSRV_JOB))
        .map(steps_of)
        .unwrap_or_default();
    let install_at = steps.iter().position(|step| {
        scalar(step, "uses").is_some_and(|uses| uses.starts_with("dtolnay/rust-toolchain@"))
            && field(step, "with").and_then(|with| scalar(with, "toolchain"))
                == Some(expected.as_str())
    });
    let check_at = steps
        .iter()
        .position(|step| scalar(step, "run") == Some(MSRV_COMMAND));
    assert!(
        matches!((install_at, check_at), (Some(install), Some(check)) if install < check),
        "the `{MSRV_JOB}` leg installs toolchain `{expected}` at step {install_at:?} and runs \
         `{MSRV_COMMAND}` at step {check_at:?}. The install has to come first: it selects the \
         toolchain for the steps that follow it, and a check above it compiles on whatever \
         the runner image shipped."
    );
}

#[test]
fn the_workflow_scope_rustflags_pin_refuses_weakening_and_every_override() {
    fn probe(header: &str, job_body: &str) -> String {
        format!("{header}jobs:\n  probe:\n{job_body}")
    }
    const PLAIN: &str = "    runs-on: ubuntu-latest\n    steps:\n      - run: cargo check\n";
    const PINNED: &str = "env:\n  RUSTFLAGS: -D warnings\n";

    let doc = parse_workflow(&ci_workflow_text()).expect(CI_WORKFLOW);
    let live = rustflags_complaints(&doc);
    assert!(
        live.is_empty(),
        "the real workflow does not satisfy the `{RUSTFLAGS_KEY}` contract:\n{}",
        live.join("\n")
    );

    let control = parse_workflow(&probe(PINNED, PLAIN)).expect("the control document parses");
    let refused_control = rustflags_complaints(&control);
    assert!(
        refused_control.is_empty(),
        "the conforming probe is refused, so no refusal below is evidence of anything:\n{}",
        refused_control.join("\n")
    );

    for (shape, document, code) in [
        ("no workflow `env:` at all", probe("", PLAIN), "rustflags"),
        (
            "an `env:` that binds other names but not this one",
            probe("env:\n  CARGO_TERM_COLOR: always\n", PLAIN),
            "rustflags",
        ),
        (
            "warnings allowed instead of denied",
            probe("env:\n  RUSTFLAGS: -A warnings\n", PLAIN),
            "rustflags",
        ),
        (
            "an allow appended after the deny, which every `contains` reading accepts",
            probe(
                "env:\n  RUSTFLAGS: -D warnings -A clippy::disallowed_methods\n",
                PLAIN,
            ),
            "rustflags",
        ),
        (
            "a value YAML does not read as a string",
            probe("env:\n  RUSTFLAGS: true\n", PLAIN),
            "rustflags",
        ),
        (
            "the encoded form at workflow scope, which Cargo reads first",
            probe(
                "env:\n  RUSTFLAGS: -D warnings\n  CARGO_ENCODED_RUSTFLAGS: ''\n",
                PLAIN,
            ),
            "rustflags",
        ),
        (
            "a job-level rebinding",
            probe(
                PINNED,
                "    runs-on: ubuntu-latest\n    env:\n      RUSTFLAGS: -A warnings\n    \
                 steps:\n      - run: cargo check\n",
            ),
            "rustflags-override",
        ),
        (
            "a job-level binding of the name Cargo prefers",
            probe(
                PINNED,
                "    runs-on: ubuntu-latest\n    env:\n      CARGO_ENCODED_RUSTFLAGS: ''\n    \
                 steps:\n      - run: cargo check\n",
            ),
            "rustflags-override",
        ),
        (
            "a step-level rebinding",
            probe(
                PINNED,
                "    runs-on: ubuntu-latest\n    steps:\n      - run: cargo check\n        \
                 env:\n          RUSTFLAGS: -A warnings\n",
            ),
            "rustflags-override",
        ),
        (
            "a step-level binding of the name Cargo prefers",
            probe(
                PINNED,
                "    runs-on: ubuntu-latest\n    steps:\n      - run: cargo check\n        \
                 env:\n          CARGO_ENCODED_RUSTFLAGS: ''\n",
            ),
            "rustflags-override",
        ),
        (
            "a job-level rebinding in lower case, which is `RUSTFLAGS` on Windows",
            probe(
                PINNED,
                "    runs-on: windows-latest\n    env:\n      rustflags: -A warnings\n    \
                 steps:\n      - run: cargo check\n",
            ),
            "rustflags-override",
        ),
        (
            "a step-level rebinding in mixed case",
            probe(
                PINNED,
                "    runs-on: windows-latest\n    steps:\n      - run: cargo check\n        \
                 env:\n          RustFlags: -A warnings\n",
            ),
            "rustflags-override",
        ),
        (
            "a case variant beside the pinned line at workflow scope",
            probe(
                "env:\n  RUSTFLAGS: -D warnings\n  Rustflags: -A warnings\n",
                PLAIN,
            ),
            "rustflags",
        ),
        (
            "the encoded name in lower case at workflow scope",
            probe(
                "env:\n  RUSTFLAGS: -D warnings\n  cargo_encoded_rustflags: ''\n",
                PLAIN,
            ),
            "rustflags",
        ),
        (
            "a bash write to the job environment file",
            probe(
                PINNED,
                "    runs-on: ubuntu-latest\n    steps:\n      - run: echo \
                 \"RUSTFLAGS=-A warnings\" >> \"$GITHUB_ENV\"\n",
            ),
            "rustflags-persisted",
        ),
        (
            "the same write through the `github.env` expression",
            probe(
                PINNED,
                "    runs-on: ubuntu-latest\n    steps:\n      - run: echo \
                 \"CARGO_ENCODED_RUSTFLAGS=\" >> ${{ github.env }}\n",
            ),
            "rustflags-persisted",
        ),
        (
            "the PowerShell form, which shares no syntax with the bash one",
            probe(
                PINNED,
                "    runs-on: windows-latest\n    steps:\n      - run: Add-Content -Path \
                 $env:GITHUB_ENV -Value \"RUSTFLAGS=-A warnings\"\n",
            ),
            "rustflags-persisted",
        ),
        (
            "the cmd form, where the file is reached as a percent variable",
            probe(
                PINNED,
                "    runs-on: windows-latest\n    steps:\n      - run: echo \
                 RUSTFLAGS=-A warnings>>%GITHUB_ENV%\n        shell: cmd\n",
            ),
            "rustflags-persisted",
        ),
        (
            "a heredoc, with the name and the redirection on different lines",
            probe(
                PINNED,
                "    runs-on: ubuntu-latest\n    steps:\n      - run: |\n          cat >> \
                 \"$GITHUB_ENV\" <<'EOF'\n          RUSTFLAGS=-A warnings\n          EOF\n",
            ),
            "rustflags-persisted",
        ),
        (
            "flags scoped to one command, with no env file in sight",
            probe(
                PINNED,
                "    runs-on: ubuntu-latest\n    steps:\n      - run: RUSTFLAGS=-A warnings \
                 cargo build\n",
            ),
            "rustflags-in-script",
        ),
    ] {
        let parsed = parse_workflow(&document).expect(shape);
        let complaints = rustflags_complaints(&parsed);
        let codes = complaint_codes(&complaints);
        assert!(
            codes.contains(code),
            "{shape} was not refused as `{code}`. Document:\n{document}\nComplaints: {:#?}",
            complaints
        );
    }

    for (shape, document) in [
        (
            "an unrelated variable whose name contains the guarded one",
            probe(
                "env:\n  RUSTFLAGS: -D warnings\n  RUSTFLAGS_EXTRA: -C debuginfo=0\n",
                PLAIN,
            ),
        ),
        (
            "an unrelated variable the guarded one is a prefix of, at job scope",
            probe(
                PINNED,
                "    runs-on: ubuntu-latest\n    env:\n      MY_RUSTFLAGS: -A warnings\n      \
                 RUST_FLAGS: -A warnings\n    steps:\n      - run: cargo check\n",
            ),
        ),
        (
            "a script that writes the env file without touching the policy",
            probe(
                PINNED,
                "    runs-on: ubuntu-latest\n    steps:\n      - run: echo \
                 \"CARGO_TERM_COLOR=never\" >> \"$GITHUB_ENV\"\n",
            ),
        ),
        (
            "a script naming a variable the guarded one is only a prefix of",
            probe(
                PINNED,
                "    runs-on: ubuntu-latest\n    steps:\n      - run: echo \
                 \"RUSTFLAGS_EXTRA=1\" >> \"$GITHUB_ENV\"\n",
            ),
        ),
    ] {
        let parsed = parse_workflow(&document).expect(shape);
        let complaints = rustflags_complaints(&parsed);
        assert!(
            complaints.is_empty(),
            "{shape} was refused, so this scan reads substrings rather than whole names. \
             Document:\n{document}\nComplaints: {complaints:#?}"
        );
    }
}

pub(crate) mod cfg;

use cfg::{
    CFG_CENSUS_CONTROL, CFG_ESCAPES, CFG_GATE_FLOOR, CONTROL_GATES, CfgForm, CfgSite,
    NO_CI_RUNNER_COMPILES, WHOLE_FILE_TEST_MODULES, cfg_regions, compiled_by, parse_cfg,
};

#[test]
fn the_cfg_census_evaluates_effective_predicates_against_the_valuations_ci_sets() {
    for (text, expected, why) in CFG_ESCAPES {
        let pred = parse_cfg(text, false).unwrap_or_else(|error| {
            panic!("the census cannot read `cfg({text})`, which it must: {error}")
        });
        assert_eq!(
            pred.render(),
            text,
            "`cfg({text})` did not round-trip through the parser"
        );
        let expected: BTreeSet<&str> = expected.iter().copied().collect();
        let compiled = compiled_by(&pred)
            .unwrap_or_else(|error| panic!("`cfg({text})` is undecidable: {error}"));
        assert_eq!(compiled, expected, "`cfg({text})` -- {why}");
    }

    let unmodelled = parse_cfg("feature = \"unshipped\"", false).expect("a parseable predicate");
    let refused = compiled_by(&unmodelled);
    assert!(
        refused.is_err(),
        "a cfg key no valuation models was decided anyway, as {refused:?}"
    );

    let mut domain = scanned_sources();
    let real = domain.len();
    let fixture = "fixtures/cfg-census-control.rs";
    domain.push((fixture.to_owned(), CFG_CENSUS_CONTROL.to_owned()));
    let (sites, unreadable) = cfg_regions(&domain);
    assert!(
        unreadable.is_empty(),
        "the census could not read {} occurrence(s):\n{}",
        unreadable.len(),
        unreadable.join("\n")
    );
    let gates: Vec<&CfgSite> = sites
        .iter()
        .filter(|site| site.form == CfgForm::Gate)
        .collect();
    assert!(
        real > 30 && gates.len() > CFG_GATE_FLOOR,
        "the control was scanned inside a truncated domain: {real} files, {} gates",
        gates.len()
    );

    let injected: Vec<&CfgSite> = sites.iter().filter(|site| site.path == fixture).collect();
    let rendered: Vec<&str> = injected
        .iter()
        .filter(|site| site.form == CfgForm::Gate)
        .map(|site| site.rendered.as_str())
        .collect();
    assert_eq!(
        rendered, CONTROL_GATES,
        "the control fixture produced the wrong gates. `haiku` or `plan9` among them is a \
         non-gating form counted as a gate; a missing `all(...)` is a stacked attribute or a \
         module guard the scan did not conjoin; `android` is a `let` binding read as a \
         predicate, and a `cfg(` from `fn cfg(bits: u32)` is a parameter list read as one."
    );

    let by_form: BTreeMap<CfgForm, Vec<&str>> =
        injected
            .iter()
            .fold(BTreeMap::new(), |mut acc: BTreeMap<_, Vec<&str>>, site| {
                acc.entry(site.form)
                    .or_default()
                    .push(site.written.as_str());
                acc
            });
    assert_eq!(
        by_form.get(&CfgForm::Attribute).map(Vec::as_slice),
        Some(["target_os = \"haiku\""].as_slice()),
        "`#[cfg_attr(P, attr)]` applies an attribute conditionally; the item is compiled \
         everywhere and it is not a platform demand"
    );
    assert_eq!(
        by_form.get(&CfgForm::Macro).map(Vec::as_slice),
        Some(["target_os = \"plan9\""].as_slice()),
        "`cfg!(P)` is an expression: both arms around it compile on every platform"
    );

    let stacked = injected
        .iter()
        .find(|site| site.rendered == "all(unix, target_os = \"macos\")")
        .expect("the stacked control");
    assert_eq!(
        stacked.written, "all(unix, target_os = \"macos\")",
        "stacked `#[cfg]`s are one item's predicate, not two items'"
    );
    let nested = injected
        .iter()
        .find(|site| site.rendered == "all(windows, test)")
        .expect("the nested control");
    assert_eq!(
        nested.written, "test",
        "the nested item writes only `test`; `windows` comes from the module around it"
    );
    assert_eq!(
        compiled_by(&nested.pred).expect("decidable"),
        BTreeSet::from(["windows-latest"]),
        "an item inside a `#[cfg(windows)] mod` is not compiled by the Linux leg, whatever \
         its own attribute says"
    );
}

#[test]
fn every_platform_this_crate_configures_for_has_a_clippy_gate_the_aggregate_requires() {
    let sources = scanned_sources();
    let (sites, unreadable) = cfg_regions(&sources);
    assert!(
        unreadable.is_empty(),
        "the census could not read {} occurrence(s):\n{}",
        unreadable.len(),
        unreadable.join("\n")
    );
    let gates: Vec<&CfgSite> = sites
        .iter()
        .filter(|site| site.form == CfgForm::Gate)
        .collect();
    assert!(
        gates.len() > CFG_GATE_FLOOR,
        "only {} gating cfg attribute(s) found across {} files; the census is reading the \
         wrong shape",
        gates.len(),
        sources.len()
    );
    assert!(
        gates
            .iter()
            .any(|site| site.written == "not(any(target_os = \"linux\", target_os = \"macos\"))"),
        "the census did not find the nested negated predicate this tree is known to carry, \
         so it is reading a narrower grammar than the tree uses"
    );
    let under_a_file_guard: BTreeSet<&str> = gates
        .iter()
        .filter(|site| site.rendered.starts_with("all(test,") || site.rendered == "test")
        .map(|site| site.path.as_str())
        .collect();
    assert!(
        under_a_file_guard.len() >= WHOLE_FILE_TEST_MODULES.len(),
        "only {} file(s) carry a `test` guard the census resolved, and \
         `the_whole_file_test_modules_are_resolved_from_the_declarations_not_the_file_names` \
         resolves {} whole-file test modules on its own",
        under_a_file_guard.len(),
        WHOLE_FILE_TEST_MODULES.len()
    );

    let mut uncovered: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for site in &gates {
        let compiled = compiled_by(&site.pred).unwrap_or_else(|error| {
            panic!(
                "{}:{}: `cfg({})` cannot be decided: {error}",
                site.path, site.line, site.rendered
            )
        });
        if compiled.is_empty() {
            uncovered
                .entry(site.rendered.as_str())
                .or_default()
                .push(format!("{}:{}", site.path, site.line));
        }
    }
    let acknowledged: BTreeSet<&str> = NO_CI_RUNNER_COMPILES
        .iter()
        .map(|(pred, _)| *pred)
        .collect();
    let found: BTreeSet<&str> = uncovered.keys().copied().collect();
    assert_eq!(
        found, acknowledged,
        "the set of effective predicates no CI runner compiles moved. Every such body is \
         outside the effect denylist's reach on every job CI runs: add the platform's Clippy \
         leg, or add a row to `NO_CI_RUNNER_COMPILES` saying why the body is unreachable on \
         purpose.\n{uncovered:#?}"
    );

    for target in &CI_TARGETS {
        let only = BTreeSet::from([target.runner]);
        let witness = gates
            .iter()
            .find(|site| compiled_by(&site.pred).is_ok_and(|compiled| compiled == only));
        assert!(
            witness.is_some(),
            "no body in this tree is compiled by `{}` alone, so nothing here establishes \
             that its Clippy leg is needed",
            target.runner
        );
    }

    let doc = parse_workflow(&ci_workflow_text()).expect(CI_WORKFLOW);
    let complaints = workflow_complaints(&doc);
    assert!(
        complaints.is_empty(),
        "{CI_WORKFLOW} does not wire the gates its own cfg census requires:\n{}",
        complaints.join("\n\n")
    );
}

fn crate_under_test() -> (PathBuf, PathBuf) {
    let exe = std::env::current_exe().expect("the test executable");
    let deps = exe
        .parent()
        .expect("the test executable is in a directory")
        .to_path_buf();
    let mut rlibs: Vec<(std::time::SystemTime, PathBuf)> = fs::read_dir(&deps)
        .expect("the deps directory")
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let name = path.file_name()?.to_str()?;
            (name.starts_with("libupstroke-") && name.ends_with(".rlib")).then(|| {
                let stamp = path
                    .metadata()
                    .and_then(|meta| meta.modified())
                    .unwrap_or(std::time::UNIX_EPOCH);
                (stamp, path)
            })
        })
        .collect();
    rlibs.sort();
    let rlib = rlibs
        .pop()
        .unwrap_or_else(|| {
            panic!(
                "no libupstroke-*.rlib beside the test executable in {}",
                deps.display()
            )
        })
        .1;
    (deps, rlib)
}

fn scratch_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("upstroke-effects-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

#[test]
fn every_externally_reachable_fn_of_a_legacy_or_shared_module_is_classified() {
    checks::reachable_fns_are_classified();
}

#[test]
fn every_effectful_wrapper_is_on_the_disallowed_list() {
    checks::effectful_wrappers_are_denied();
}

#[test]
fn every_funnel_classified_fn_names_a_site() {
    checks::funnel_rows_name_a_site();
}

#[test]
fn every_libc_item_the_tree_names_is_classified_and_the_effects_are_denied() {
    checks::libc_items_are_classified_and_denied();
}

mod artifacts;

use artifacts::{
    SAMPLING_N, SITES_WITHOUT_A_FUNNEL, artifact_content, funnel_module, funnel_module_record,
    residue_record,
};

#[test]
fn the_checked_in_effect_sites_json_is_what_the_enums_generate() {
    let generated = format!(
        "{}\n",
        effect_sites_json().expect("the inventory serializes")
    );
    let path = repo_root().join(EFFECT_SITES_JSON);
    if std::env::var_os(REGENERATE).is_some() {
        fs::write(&path, &generated).expect("write the inventory");
    }
    let on_disk = fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("{EFFECT_SITES_JSON} is missing; run with {REGENERATE}=1"));
    let on_disk = artifact_content(&on_disk);
    assert_eq!(
        on_disk, generated,
        "{EFFECT_SITES_JSON} is stale; regenerate with {REGENERATE}=1"
    );
    assert_eq!(effect_sites().len(), EffectSiteId::all().len());
    assert!(on_disk.contains("\"site\": \"Event.OpenLog\""));
    assert!(on_disk.contains("\"site\": \"Object.CandidateCommitTree\""));
}

#[test]
fn the_checked_in_funnel_module_record_states_where_the_bodies_are() {
    let generated = funnel_module_record();
    let path = repo_root().join(FUNNEL_MODULES_JSON);
    if std::env::var_os(REGENERATE).is_some() {
        fs::write(&path, &generated).expect("write the funnel-module record");
    }
    let on_disk =
        artifact_content(&fs::read_to_string(&path).unwrap_or_else(|_| {
            panic!("{FUNNEL_MODULES_JSON} is missing; run with {REGENERATE}=1")
        }));
    assert_eq!(
        on_disk, generated,
        "{FUNNEL_MODULES_JSON} is stale; regenerate with {REGENERATE}=1"
    );

    let parsed: serde_json::Value = serde_json::from_str(&on_disk).expect("the record parses");
    assert_eq!(
        parsed["sites_checked"].as_u64().expect("a count"),
        EffectSiteId::all().len() as u64,
        "the record must cover the whole inventory; a record over a corner of it          would report agreement it never looked for"
    );
    let disagreements: Vec<&str> = parsed["disagreements"]
        .as_array()
        .expect("an array")
        .iter()
        .map(|entry| entry["site"].as_str().expect("a site name"))
        .collect();
    assert_eq!(
        disagreements,
        ["Answer.StageWrite", "Answer.PublishRename", "Answer.Ingest"],
        "the set of sites whose funnel bodies are not where the inventory says          moved. Each one is a claim a gate report carries about this tree."
    );
    for entry in parsed["disagreements"].as_array().expect("an array") {
        assert_eq!(entry["inventory_module"], "src/interaction.rs");
        assert_eq!(entry["funnel_module"], "src/rundir.rs");
    }
}

#[test]
fn every_site_the_inventory_declares_has_a_funnel_that_names_it_or_is_recorded_absent() {
    let list = allowlist();
    let funnel: BTreeMap<&str, &AllowlistEntry> = list
        .funnel
        .iter()
        .map(|entry| (entry.path.as_str(), entry))
        .collect();

    let mut modules: BTreeSet<String> = BTreeSet::new();
    for site in EffectSiteId::all() {
        modules.insert(site.module().to_owned());
    }
    assert_eq!(modules.len(), 7, "{modules:?}");
    for module in &modules {
        assert!(
            funnel.contains_key(module.as_str()),
            "`{module}` is a funnel module the inventory names and the allowlist's \
             funnel section does not list it"
        );
    }

    let mut sources: BTreeMap<String, String> = BTreeMap::new();
    let mut unimplemented = Vec::new();
    let mut mechanisms: BTreeMap<&str, &str> = BTreeMap::new();
    for site in EffectSiteId::all() {
        let group = site.group().name();
        let module = funnel_module(site);
        let entry = funnel[module];
        if entry.absent {
            unimplemented.push(site.name());
            continue;
        }
        let source = sources.entry(module.to_owned()).or_insert_with(|| {
            let text = fs::read_to_string(repo_root().join(module)).expect("read funnel module");
            blank_comments_and_strings(&production_region(&text))
        });
        let variant = format!("{group}Site::{}", site.variant());
        let parameter = format!(": {group}Site");
        if source.contains(&variant) {
            mechanisms.insert(group, "variant");
        } else if source.contains(&parameter) {
            mechanisms.insert(group, "parameter");
        } else {
            unimplemented.push(site.name());
        }
    }
    let distinct: BTreeSet<&str> = mechanisms.values().copied().collect();
    assert_eq!(distinct.len(), 2, "{mechanisms:?}");

    let expected: BTreeSet<String> = SITES_WITHOUT_A_FUNNEL
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let actual: BTreeSet<String> = unimplemented.into_iter().collect();
    assert_eq!(
        actual, expected,
        "the set of sites no funnel names moved. Each one is a row of the site \
         inventory in reconciliation-D.md and needs a reason."
    );
}

#[test]
fn no_production_api_exports_a_writable_process_command() {
    fn command_returning_public_signatures(source: &str) -> Vec<String> {
        let code = blank_comments_and_strings(&production_region(source));
        let mut found = Vec::new();
        for (at, _) in code.match_indices("pub") {
            let before_ok = at == 0
                || !(code.as_bytes()[at - 1].is_ascii_alphanumeric()
                    || code.as_bytes()[at - 1] == b'_');
            let tail = &code[at..];
            if !before_ok {
                continue;
            }
            let after_pub = tail["pub".len()..].trim_start();
            let item = if let Some(restricted) = after_pub.strip_prefix('(') {
                let Some(close) = restricted.find(')') else {
                    continue;
                };
                if restricted[..close].trim() != "crate" {
                    continue;
                }
                restricted[close + 1..].trim_start()
            } else {
                after_pub
            };
            if !item.starts_with("fn ") {
                continue;
            }
            let end = tail.find(['{', ';']).unwrap_or(tail.len());
            let signature = &tail[..end];
            let Some(arrow) = signature.find("->") else {
                continue;
            };
            let returns = &signature[arrow + 2..];
            let names_command = returns
                .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                .any(|token| token == "Command");
            if names_command {
                found.push(signature.split_whitespace().collect::<Vec<_>>().join(" "));
            }
        }
        found
    }

    let mut escapes = Vec::new();
    for (path, source) in scanned_sources() {
        for signature in command_returning_public_signatures(&source) {
            escapes.push(format!("{path}: {signature}"));
        }
    }
    assert!(escapes.is_empty(), "writable Command escapes: {escapes:#?}");

    assert_eq!(
        command_returning_public_signatures(
            "pub fn renamed() -> std::process::Command { todo!() }\n\
             pub ( crate )\n fn pointer() -> fn() -> Command { todo!() }\n\
             fn private() -> Command { todo!() }\n\
             pub fn consumes(_: Command) -> ProcessOutput { todo!() }"
        )
        .len(),
        2,
        "the structural control must catch direct and function-pointer returns only"
    );
}

#[test]
fn the_checked_in_residue_class_record_is_what_the_enums_generate() {
    let generated = residue_record();
    let path = repo_root().join(RESIDUE_CLASSES_JSON);
    if std::env::var_os(REGENERATE).is_some() {
        fs::write(&path, &generated).expect("write the residue record");
    }
    let on_disk = fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("{RESIDUE_CLASSES_JSON} is missing; run with {REGENERATE}=1"));
    let on_disk = artifact_content(&on_disk);
    assert_eq!(
        on_disk, generated,
        "{RESIDUE_CLASSES_JSON} is stale; regenerate with {REGENERATE}=1"
    );

    let harness = fs::read_to_string(repo_root().join("src/workspace_manager/tests.rs"))
        .expect("src/workspace_manager/tests.rs");
    assert!(
        harness.contains(&format!("const SAMPLING_N: u32 = {SAMPLING_N};")),
        "the sampling harness no longer runs N = {SAMPLING_N}"
    );
    assert!(on_disk.contains(&format!("\"sampling_n\": {SAMPLING_N}")));
}

#[test]
fn every_file_durability_barrier_in_a_funnel_module_goes_through_one_call() {
    const BARRIERS: &[(&str, &str, usize)] = &[
        ("src/util.rs", "fsync_file", 1),
        ("src/util.rs", "fsync_dir", 1),
    ];
    let util = artifact_content(
        &fs::read_to_string(repo_root().join("src/util.rs")).expect("src/util.rs"),
    );
    for (file, function, expected) in BARRIERS {
        let body = util
            .split_once(&format!("fn {function}("))
            .unwrap_or_else(|| panic!("{file} no longer defines `{function}`"))
            .1;
        let body = &body[..body.find("\n}\n").expect("the function ends")];
        let calls = body.matches(".sync_all()").count();
        assert_eq!(
            calls, *expected,
            "`{function}` makes {calls} durability syscall(s), not {expected}; deleting \
             the barrier from inside it is exactly PR5-CONF-012's surviving mutation"
        );
    }

    const FUNNELS: &[&str] = &[
        "src/rundir.rs",
        "src/workspace_manager.rs",
        "src/events/log.rs",
        "src/runner/container.rs",
    ];
    for path in FUNNELS {
        let source =
            fs::read_to_string(repo_root().join(path)).unwrap_or_else(|_| panic!("{path}"));
        let production = blank_comments_and_strings(&production_region(&source));
        assert_eq!(
            production.matches(".sync_all()").count(),
            0,
            "{path} calls `sync_all` directly; the file barrier is `util::fsync_file` \
             and the directory barrier is `util::fsync_dir`"
        );
    }

    let log = fs::read_to_string(repo_root().join("src/events/log.rs")).expect("src/events/log.rs");
    assert_eq!(
        blank_comments_and_strings(&production_region(&log))
            .matches(".sync_data()")
            .count(),
        1,
        "the log's own barrier is one `sync_data`; \
         `events::log::tests::the_event_log_is_written_in_exactly_one_module` \
         is the census that owns it"
    );
}

mod source_oracles;

use source_oracles::oracles;

#[test]
fn no_site_enums_row_mapping_has_a_wildcard_arm() {
    oracles::site_row_mappings_have_no_wildcard_arm();
}

#[test]
fn the_row_mapping_census_reads_the_declared_production_module() {
    oracles::the_row_mapping_census_domain_is_the_declared_module();
}

#[test]
fn no_topology_module_calls_a_funnel_in_production() {
    oracles::topology_production_names_no_funnel();
}

#[test]
fn the_reachable_fn_parser_finds_each_shape_this_tree_uses() {
    oracles::the_reachable_fn_parser_finds_every_shape();
}

#[test]
fn the_comment_blanker_models_raw_strings_and_still_blanks_comments() {
    oracles::the_comment_blanker_models_raw_strings();
}

#[test]
fn the_two_blankers_each_carry_their_own_contract_in_the_notes() {
    oracles::the_notes_give_each_blanker_its_own_contract();
}

#[test]
fn a_multi_byte_char_literal_does_not_desync_the_blanker() {
    oracles::a_multi_byte_char_literal_keeps_the_blankers_phase();
}

#[test]
fn a_region_that_cannot_find_an_items_end_blanks_the_attribute_not_the_file() {
    oracles::an_unfindable_item_end_blanks_the_attribute();
}

mod contract_mappings;

use contract_mappings::mappings;

#[test]
fn every_test_the_container_fault_row_names_is_a_test_in_this_tree() {
    mappings::every_fault_row_name_is_a_test_in_the_tree();
}

#[test]
fn the_container_fault_row_predicate_refuses_a_name_that_is_only_prose() {
    mappings::the_presence_predicate_refuses_a_non_test_shape();
}

#[test]
fn the_view_directory_has_one_definition_in_the_tree() {
    let container: Vec<(String, String)> = scanned_sources()
        .into_iter()
        .filter(|(path, _)| {
            path.starts_with("src/runner/container") && !path.ends_with("/tests.rs")
        })
        .collect();
    let modules: BTreeSet<&str> = container.iter().map(|(path, _)| path.as_str()).collect();
    assert_eq!(
        modules,
        BTreeSet::from([
            "src/runner/container.rs",
            "src/runner/container/census.rs",
            "src/runner/container/env.rs",
            "src/runner/container/exec.rs",
            "src/runner/container/fake.rs",
            "src/runner/container/intent.rs",
            "src/runner/container/resolve.rs",
            "src/runner/container/runtime.rs",
            "src/runner/container/view.rs",
        ]),
        "the container substrate's production modules moved; the seam this test \
         pins may no longer be inside the scanned set"
    );

    let mut sites = Vec::new();
    let mut located = Vec::new();
    for (path, source) in &container {
        let code = blank_comments(&production_region(source));
        for (index, _) in code.match_indices("\"views\"") {
            let line = code[..index].matches('\n').count() + 1;
            sites.push(path.clone());
            located.push(format!("{path}:{line}"));
        }
    }
    assert_eq!(
        sites,
        vec!["src/runner/container/census.rs".to_owned()],
        "the R19 view directory segment is declared in more than one production \
         site. `census::VIEWS_DIR` is the one definition and `exec::view_dir` \
         delegates to `census::view_path`; a second literal is a path that can \
         drift away from the census that has to find it, and no behavioural test \
         crosses the two halves. Sites found: {located:?}"
    );

    let (_, census) = container
        .iter()
        .find(|(path, _)| path == "src/runner/container/census.rs")
        .expect("the census module is in the scanned set");
    assert!(
        blank_comments(&production_region(census))
            .contains("pub const VIEWS_DIR: &str = \"views\";"),
        "the scan cannot see the declaration it is counting"
    );

    let root = Path::new("/private/root");
    let name = crate::runner::container::intent::ContainerName::from_parts(
        "repokey",
        "run01",
        "inc01",
        "0123456789abcdef",
    )
    .expect("a well-formed container name");
    assert_eq!(
        crate::runner::container::exec::view_dir(root, &name),
        crate::runner::container::census::view_path(root, &name),
        "the runner mounts the view somewhere the census does not look"
    );
}

#[test]
fn the_whole_file_test_modules_are_resolved_from_the_declarations_not_the_file_names() {
    oracles::the_whole_file_modules_are_read_from_the_declarations();
}

#[test]
fn the_module_scan_reads_ancestry_and_visibility_rather_than_text_after_an_attribute() {
    use crate::effects::census_domain::{
        Predicate, ScannedDeclaration, entails_test, parse_predicate, scan_module_declarations,
    };

    fn scan(source: &str) -> Vec<ScannedDeclaration> {
        scan_module_declarations(source)
            .unwrap_or_else(|refusal| panic!("the fixture is readable: {refusal}"))
    }
    fn only(source: &str) -> ScannedDeclaration {
        let mut found = scan(source);
        assert_eq!(found.len(), 1, "{source:?} -> {found:#?}");
        found.remove(0)
    }

    let plain = only("#[cfg(test)]\nmod tests;\n");
    assert_eq!(plain.name, "tests");
    assert!(plain.inline_path.is_empty());
    assert_eq!(plain.guard, "test");
    assert!(plain.test_only);

    for written in [
        "#[cfg(test)]\npub mod helpers;\n",
        "#[cfg(test)]\npub(crate) mod helpers;\n",
        "#[cfg(test)]\npub(super) mod helpers;\n",
        "#[cfg(test)]\npub(in crate::a::b) mod helpers;\n",
    ] {
        let qualified = only(written);
        assert_eq!(qualified.name, "helpers", "{written:?}");
        assert!(qualified.test_only, "{written:?}");
    }
    assert!(!only("pub(crate) mod helpers;\n").test_only);

    let inherited =
        only("#[cfg(test)]\npub(crate) mod test_support {\n    pub(crate) mod readiness;\n}\n");
    assert_eq!(inherited.name, "readiness");
    assert_eq!(inherited.inline_path, vec!["test_support".to_owned()]);
    assert_eq!(inherited.guard, "test");
    assert!(inherited.test_only);
    let ungated = only("pub(crate) mod test_support {\n    pub(crate) mod readiness;\n}\n");
    assert_eq!(ungated.inline_path, vec!["test_support".to_owned()]);
    assert!(
        !ungated.test_only,
        "a declaration under an unguarded inline module is production code"
    );

    let deep =
        only("mod outer {\n    #[cfg(test)]\n    mod middle {\n        pub mod leaf;\n    }\n}\n");
    assert_eq!(deep.name, "leaf");
    assert_eq!(
        deep.inline_path,
        vec!["outer".to_owned(), "middle".to_owned()]
    );
    assert!(deep.test_only);

    let both = scan("#[cfg(test)]\nmod inner {\n    mod under;\n}\nmod beside;\n");
    assert_eq!(both.len(), 2, "{both:#?}");
    assert_eq!(both[0].name, "under");
    assert_eq!(both[0].inline_path, vec!["inner".to_owned()]);
    assert!(both[0].test_only);
    assert_eq!(both[1].name, "beside");
    assert!(both[1].inline_path.is_empty());
    assert!(
        !both[1].test_only,
        "a declaration after the guarded block inherited a guard that had closed"
    );

    let after_a_function = scan("#[cfg(test)]\nfn helper() {}\nmod plain;\n");
    assert_eq!(after_a_function.len(), 1, "{after_a_function:#?}");
    assert!(!after_a_function[0].test_only);
    assert!(
        scan("#[cfg(test)]\nmod tests {\n    fn t() {}\n}\n").is_empty(),
        "an inline module with a body names no file"
    );

    for (written, expected) in [
        ("#[cfg(test)]\nmod x;\n", true),
        ("#[cfg(all(test, unix))]\nmod x;\n", true),
        ("#[cfg(all(unix, all(test, windows)))]\nmod x;\n", true),
        ("#[cfg(test)]\n#[cfg(unix)]\nmod x;\n", true),
        ("#[cfg(unix)]\nmod outer {\n#[cfg(test)]\nmod x;\n}\n", true),
        ("#[cfg(any(test, unix))]\nmod x;\n", false),
        ("#[cfg(not(test))]\nmod x;\n", false),
        ("#[cfg(unix)]\nmod x;\n", false),
        ("#[cfg(feature = \"slow\")]\nmod x;\n", false),
        ("mod x;\n", false),
    ] {
        assert_eq!(
            only(written).test_only,
            expected,
            "{written:?} was decided the other way"
        );
    }

    for written in ["test", "all(test, unix)", "not(any(not(test), unix))"] {
        let pred = parse_predicate(written).unwrap_or_else(|why| panic!("{written}: {why}"));
        assert!(entails_test(&pred), "`{written}` does not entail `test`");
    }
    for written in [
        "any(test, unix)",
        "not(test)",
        "unix",
        "target_os = \"linux\"",
        "all(unix, windows)",
    ] {
        let pred = parse_predicate(written).unwrap_or_else(|why| panic!("{written}: {why}"));
        assert!(
            !entails_test(&pred),
            "`{written}` was read as entailing `test`"
        );
    }
    assert_eq!(
        parse_predicate("all(test, unix)").map(|pred| pred.render()),
        Ok("all(test, unix)".to_owned())
    );
    assert_eq!(parse_predicate("test"), Ok(Predicate::Test));

    for prose in [
        "// #[cfg(test)] mod ghost;\n",
        "/* #[cfg(test)] mod ghost; */\n",
        "/// #[cfg(test)] mod ghost;\nfn documented() {}\n",
        "const S: &str = \"#[cfg(test)] mod ghost;\";\n",
        "const S: &str = r#\"#[cfg(test)] mod ghost;\"#;\n",
        "const S: &[u8] = b\"#[cfg(test)] mod ghost;\";\n",
    ] {
        assert!(scan(prose).is_empty(), "{prose:?} derived a declaration");
    }
    let after_a_brace_char = only("const C: char = '{';\n#[cfg(test)]\nmod real;\n");
    assert_eq!(after_a_brace_char.name, "real");
    assert!(after_a_brace_char.inline_path.is_empty());
    assert!(after_a_brace_char.test_only);

    assert!(scan("fn models() {}\nstruct modest;\n").is_empty());

    let past_a_macro = only("thread_local! {\n    static X: u8 = 0;\n}\n#[cfg(test)]\nmod real;\n");
    assert_eq!(past_a_macro.name, "real");
    assert!(past_a_macro.inline_path.is_empty());
    assert!(past_a_macro.test_only);
    let after_attributed_macro = only("#[cfg(test)]\nlazy! [ a, b ]\nmod plain;\n");
    assert_eq!(after_attributed_macro.name, "plain");
    assert!(
        !after_attributed_macro.test_only,
        "a `#[cfg(test)]` above a macro invocation carried to the next item"
    );
    let past_a_negation = only("fn f() { let _ = a != b; }\n#[cfg(test)]\nmod real;\n");
    assert_eq!(past_a_negation.name, "real");
    assert!(past_a_negation.test_only);

    for tokens in [
        "macro_rules! m {\n    (mod $n:ident) => {\n        ()\n    };\n}\n",
        "m! { mod }\n",
        "outer! { inner! { mod } }\n",
    ] {
        assert_eq!(
            scan_module_declarations(tokens).map(|found| found.len()),
            Ok(0),
            "{tokens:?} was read as items rather than discarded"
        );
    }
    let beside_a_macro = only(
        "macro_rules! m {\n    (mod $n:ident) => {\n        ()\n    };\n}\n#[cfg(test)]\nmod real;\n",
    );
    assert_eq!(beside_a_macro.name, "real");
    assert!(beside_a_macro.test_only);

    for spaced in [
        "vec ! [1, 2];\n#[cfg(test)]\nmod real;\n",
        "assert /* sic */ ! (a == b);\n#[cfg(test)]\nmod real;\n",
        "macro_rules ! m {\n    () => {\n        fn go() {}\n    };\n}\n#[cfg(test)]\nmod real;\n",
        "macro_rules ! m {\n    (mod $n:ident) => {\n        ()\n    };\n}\n#[cfg(test)]\nmod real;\n",
        "macro_rules /* named next */ ! m {\n    (mod $n:ident) => {\n        ()\n    };\n}\n#[cfg(test)]\nmod real;\n",
    ] {
        let past = only(spaced);
        assert_eq!(past.name, "real", "{spaced:?}");
        assert!(past.test_only, "{spaced:?}");
    }

    let inside_a_negated_block = only(
        "#[cfg(test)]\nmod outer {\n    fn f() {\n        if !ready { }\n    }\n    mod inner;\n}\n",
    );
    assert_eq!(inside_a_negated_block.name, "inner");
    assert_eq!(
        inside_a_negated_block.inline_path,
        vec!["outer".to_owned()],
        "a negated condition was read as a macro and swallowed the block"
    );
    assert!(inside_a_negated_block.test_only);
    for negation in [
        "fn f() { if !ready { } }\n#[cfg(test)]\nmod real;\n",
        "fn f() { while !done { } }\n#[cfg(test)]\nmod real;\n",
        "fn f() { let _ = !flag; }\n#[cfg(test)]\nmod real;\n",
    ] {
        let past = only(negation);
        assert_eq!(past.name, "real", "{negation:?}");
        assert!(past.test_only, "{negation:?}");
    }
    let inside_a_negated_block = only(
        "#[cfg(test)]\nmod outer {\n    fn f() {\n        if !ready {\n            mod local;\n        }\n    }\n}\n",
    );
    assert_eq!(inside_a_negated_block.name, "local");
    assert_eq!(
        inside_a_negated_block.inline_path,
        vec!["outer".to_owned()],
        "the negated block was skipped as a macro body and its declaration lost"
    );
    assert!(inside_a_negated_block.test_only);
    let inside_a_negated_loop = only(
        "mod outer {\n    fn f() {\n        while !done {\n            mod local;\n        }\n    }\n}\n",
    );
    assert_eq!(inside_a_negated_loop.name, "local");
    assert!(!inside_a_negated_loop.test_only);

    for negated_group in [
        "#[cfg(test)]\nmod outer {\n    fn f() -> bool {\n        if !({ mod local {} true }) { false } else { true }\n    }\n}\n",
        "mod outer {\n    fn f() -> bool {\n        !({ mod local {} true })\n    }\n}\n",
        "mod outer {\n    fn f() {\n        while !({ mod local {} false }) { }\n    }\n}\n",
        "mod outer {\n    fn f() -> bool {\n        return !({ mod local {} true });\n    }\n}\n",
    ] {
        let read = scan_module_declarations(negated_group)
            .unwrap_or_else(|refusal| panic!("{negated_group:?} was refused: {refusal}"));
        assert!(
            read.is_empty(),
            "an inline `mod local {{}}` names no file, so it is a scope and not a declaration: \
             {negated_group:?} -> {read:#?}"
        );
    }
    let through_a_negated_group = only(
        "#[cfg(test)]\nmod outer {\n    fn f() -> bool {\n        if !({ mod local; true }) { false } else { true }\n    }\n}\n",
    );
    assert_eq!(through_a_negated_group.name, "local");
    assert_eq!(
        through_a_negated_group.inline_path,
        vec!["outer".to_owned()],
        "the negated group was skipped as a macro body and its declaration lost"
    );
    assert!(through_a_negated_group.test_only);

    for (written, expected) in [
        ("#[cfg(test)]\nmod r#type;\n", "type"),
        ("#[cfg(test)]\npub(crate) mod r#fn;\n", "fn"),
        ("#[cfg(test)]\nmod r#tests;\n", "tests"),
    ] {
        let raw = only(written);
        assert_eq!(raw.name, expected, "{written:?}");
        assert!(raw.test_only, "{written:?}");
    }
    assert!(scan("struct r#mod;\nfn f() { let raw = 1; }\n").is_empty());
    let beside_a_raw_word = only("fn raw() {}\n#[cfg(test)]\nmod real;\n");
    assert_eq!(beside_a_raw_word.name, "real");

    let raw_binding = "fn f() { let r#mod = 1; }\n#[cfg(test)]\nmod tests;\n";
    let read = scan_module_declarations(raw_binding).unwrap_or_else(|refusal| {
        panic!("`let r#mod = 1;` is valid Rust and was refused: {refusal}")
    });
    assert_eq!(read.len(), 1, "{read:#?}");
    assert_eq!(read[0].name, "tests");
    assert!(read[0].test_only);

    let raw_in_a_use = "#[cfg(test)]\nmod harness {\n    use std::r#mod as tests;\n}\n";
    assert_eq!(
        scan_module_declarations(raw_in_a_use),
        Ok(Vec::new()),
        "`use std::r#mod as tests;` declares no module, and the text inside `r#mod` is not an \
         item"
    );

    for source in [
        "fn f() { let r#mod = 1; }\n#[cfg(test)]\nmod real;\n",
        "fn f() { let r#type = 1; }\n#[cfg(test)]\nmod real;\n",
        "fn f() { let r = 1; }\n#[cfg(test)]\nmod real;\n",
        "fn f() { let raw = 1; }\n#[cfg(test)]\nmod real;\n",
    ] {
        assert_eq!(only(source).name, "real", "{source:?}");
        assert_eq!(
            scan_module_declarations(&source.replace('\n', "\r\n")),
            scan_module_declarations(source),
            "CRLF: {source:?}"
        );
    }

    let past_a_raw_macro = only("r#if! { let _ = 1; }\n#[cfg(test)]\nmod real;\n");
    assert_eq!(past_a_raw_macro.name, "real");
    assert!(past_a_raw_macro.test_only);

    for fixture in [
        "#[cfg(test)]\npub(crate) mod test_support {\n    pub(crate) mod readiness;\n}\n",
        "mod outer {\n    #[cfg(test)]\n    mod middle {\n        pub mod leaf;\n    }\n}\n",
        "macro_rules ! m {\n    (mod $n:ident) => {\n        ()\n    };\n}\n#[cfg(test)]\nmod real;\n",
        "#[cfg(test)]\nmod r#type;\n",
    ] {
        let lf = scan_module_declarations(fixture);
        let crlf = scan_module_declarations(&fixture.replace('\n', "\r\n"));
        assert_eq!(lf, crlf, "CRLF changed the derivation for {fixture:?}");
        assert!(lf.is_ok_and(|found| found.len() == 1));
    }
    for refused in [
        "macro_rules! m {\n    () => {\n        #[cfg(test)]\n        mod x;\n    };\n}\n",
        "#[cfg(test)]\nmod tests;\n#[cfg(test)]\nmod tests;\n",
    ] {
        assert_eq!(
            scan_module_declarations(refused).is_err(),
            scan_module_declarations(&refused.replace('\n', "\r\n")).is_err(),
            "CRLF changed whether {refused:?} is refused"
        );
        assert!(scan_module_declarations(refused).is_err());
    }
}

fn is_the_literal_mod_tests_form(name: &str, inline_path: &[String], guard: &str) -> bool {
    name == "tests" && inline_path.is_empty() && guard == "test"
}

#[test]
fn a_narrowed_cfg_guard_is_test_only_but_is_not_the_literal_mod_tests_form() {
    use crate::effects::census_domain::{ScannedDeclaration, scan_module_declarations};

    fn only(source: &str) -> ScannedDeclaration {
        let mut found = scan_module_declarations(source)
            .unwrap_or_else(|refusal| panic!("the fixture is readable: {refusal}"));
        assert_eq!(found.len(), 1, "{source:?} -> {found:#?}");
        found.remove(0)
    }
    fn literal(declaration: &ScannedDeclaration) -> bool {
        is_the_literal_mod_tests_form(
            &declaration.name,
            &declaration.inline_path,
            &declaration.guard,
        )
    }

    let plain = only("#[cfg(test)]\nmod tests;\n");
    assert_eq!(plain.guard, "test");
    assert!(plain.test_only);
    assert!(literal(&plain), "{plain:#?}");

    for narrowed in [
        "#[cfg(all(test, unix))]\nmod tests;\n",
        "#[cfg(test)]\n#[cfg(unix)]\nmod tests;\n",
    ] {
        let declaration = only(narrowed);
        assert_eq!(declaration.name, plain.name);
        assert_eq!(declaration.inline_path, plain.inline_path);
        assert!(
            declaration.test_only,
            "a narrowed guard still entails `test`, so the file is still a whole-file test \
             module and still belongs in the census domain: {declaration:#?}"
        );
        assert_ne!(
            declaration.guard, plain.guard,
            "the guard is the only field that differs, so it is the only field that can \
             distinguish them"
        );
        assert!(
            !literal(&declaration),
            "{narrowed:?} is not the literal `#[cfg(test)] mod tests;` form: rustc compiles no \
             such module where the narrowing is false, and a census that counted it as the plain \
             form would skip a file that is not there and lose the module on that platform in \
             silence: {declaration:#?}"
        );
    }

    let inherited = only("#[cfg(test)]\nmod test_support {\n    pub(crate) mod readiness;\n}\n");
    assert_eq!(
        (inherited.guard.as_str(), inherited.name.as_str()),
        ("test", "readiness")
    );
    assert!(
        inherited.test_only && !literal(&inherited),
        "{inherited:#?}"
    );
    let other_name = only("#[cfg(test)]\nmod scaffold;\n");
    assert_eq!(other_name.guard, "test");
    assert!(other_name.inline_path.is_empty());
    assert!(
        other_name.test_only && !literal(&other_name),
        "{other_name:#?}"
    );
}

#[test]
fn the_module_resolver_refuses_every_shape_it_cannot_resolve() {
    use crate::effects::census_domain::{
        CandidateRefusal, ScanRefusal, candidates_for, contained_in, declaration_cycle,
        module_directory, parse_predicate, scan_module_declarations, sole_present,
    };

    fn refusal(source: &str) -> ScanRefusal {
        scan_module_declarations(source).expect_err("this source is refused")
    }

    assert_eq!(
        refusal("#[cfg(test)\nmod tests;\n"),
        ScanRefusal::UnclosedAttribute { line: 1 }
    );
    assert_eq!(
        refusal("mod a { }\n}\n"),
        ScanRefusal::UnbalancedBraces { line: 2 }
    );
    for malformed in ["mod ;\n", "mod x = 3;\n", "mod trailing\n"] {
        assert!(
            matches!(refusal(malformed), ScanRefusal::MalformedDeclaration { .. }),
            "{malformed:?} was read as a declaration"
        );
    }

    for unreadable in [
        "#[cfg(sometimes(test))]\nmod x;\n",
        "#[cfg(test]\nmod x;\n",
        "#[cfg()]\nmod x;\n",
        "#[cfg(not(test, unix))]\nmod x;\n",
        "#[cfg(feature =)]\nmod x;\n",
    ] {
        assert!(
            matches!(refusal(unreadable), ScanRefusal::UnreadablePredicate { .. }),
            "{unreadable:?} was decided rather than refused"
        );
    }
    for unreadable in [
        "",
        "all(test",
        "not(test, unix)",
        "maybe(test)",
        "all(test) extra",
    ] {
        assert!(
            parse_predicate(unreadable).is_err(),
            "`{unreadable}` parsed"
        );
    }

    for pathed in [
        "#[path = \"elsewhere.rs\"]\nmod x;\n",
        "#[cfg_attr(unix, path = \"elsewhere.rs\")]\nmod x;\n",
    ] {
        assert!(
            matches!(
                refusal(pathed),
                ScanRefusal::UnsupportedPathAttribute { .. }
            ),
            "{pathed:?} was resolved"
        );
    }
    assert!(
        scan_module_declarations("#[path = \"x\"]\nstruct S;\nmod y;\n").is_ok(),
        "a `path` attribute on a non-module item is not a module path attribute"
    );

    for shaped in [
        "macro_rules! m {\n    () => {\n        mod x;\n    };\n}\n",
        "macro_rules! m {\n    () => {\n        #[cfg(test)]\n        mod x;\n    };\n}\n",
        "quote! { mod x; }\n",
        "paste!( mod x { } );\n",
        "items![ pub(crate) mod x; ]\n",
        "outer! { inner! { mod x; } }\n",
        "macro_rules! r#mod {\n    () => {\n        mod x;\n    };\n}\n",
        "macro_rules ! r#type {\n    () => {\n        #[cfg(test)]\n        mod x;\n    };\n}\n",
        "quote! { mod r#type; }\n",
        "r#if! { mod r#fn { } }\n",
    ] {
        assert!(
            matches!(refusal(shaped), ScanRefusal::ModuleShapedMacroBody { .. }),
            "{shaped:?} was read rather than refused"
        );
    }
    for spaced in [
        "macro_rules ! m {\n    () => {\n        mod x;\n    };\n}\n",
        "macro_rules\n! m {\n    () => {\n        #[cfg(test)]\n        mod x;\n    };\n}\n",
        "macro_rules /* named next */ ! m {\n    () => {\n        mod x;\n    };\n}\n",
        "#[rustfmt::skip]\nmacro_rules  !  m  {\n    () => {\n        mod x;\n    };\n}\n",
        "quote ! { mod x; }\n",
        "quote // why\n! { mod x; }\n",
        "quote /* why */ ! { pub(crate) mod x; }\n",
        "items\n    ![ mod x { } ]\n",
    ] {
        assert!(
            matches!(refusal(spaced), ScanRefusal::ModuleShapedMacroBody { .. }),
            "{spaced:?} was read rather than refused"
        );
    }

    for ordinary in [
        "vec![1, 2, 3];\n",
        "assert!(a == b, \"mod x; is prose here\");\n",
        "macro_rules! m {\n    () => {\n        fn go() {}\n    };\n}\n",
        "modify!(x);\n",
    ] {
        assert_eq!(
            scan_module_declarations(ordinary).map(|found| found.len()),
            Ok(0),
            "{ordinary:?} was not discarded cleanly"
        );
    }

    assert!(matches!(
        refusal("#![cfg(test)]\nmod x;\n"),
        ScanRefusal::UnsupportedInnerCfg { .. }
    ));

    assert!(matches!(
        refusal("#[cfg(test)]\nmod tests;\n#[cfg(test)]\nmod tests;\n"),
        ScanRefusal::DuplicateDeclaration { .. }
    ));
    assert!(
        scan_module_declarations("mod a {\n    mod x;\n}\nmod b {\n    mod x;\n}\n").is_ok(),
        "two parents each declaring `x` are not a duplicate"
    );

    let roots = crate::effects::tests::crate_roots();
    let root = repo_root();
    let named = |file: &str, inline: &[String], name: &str| {
        candidates_for(roots, &root.join(file), inline, name)
    };
    assert_eq!(
        named(
            "src/agent/proc.rs",
            &["test_support".to_owned()],
            "readiness"
        ),
        Ok([
            root.join("src/agent/proc/test_support/readiness.rs"),
            root.join("src/agent/proc/test_support/readiness/mod.rs"),
        ])
    );
    assert_eq!(
        named("src/agent/proc.rs", &[], "readiness"),
        Ok([
            root.join("src/agent/proc/readiness.rs"),
            root.join("src/agent/proc/readiness/mod.rs"),
        ])
    );
    for flattened in named("src/agent/proc.rs", &[], "readiness").expect("inside the package") {
        assert!(
            !flattened.is_file(),
            "{} exists, so the flattening mutation would resolve instead of refusing",
            flattened.display()
        );
    }

    assert_eq!(
        named("src/engine/mod.rs", &[], "tests").map(|pair| pair[0].clone()),
        Ok(root.join("src/engine/tests.rs"))
    );
    assert_eq!(
        named("src/lib.rs", &[], "effects").map(|pair| pair[0].clone()),
        Ok(root.join("src/effects.rs"))
    );
    assert_eq!(
        named("src/main.rs", &[], "tests").map(|pair| pair[0].clone()),
        Ok(root.join("src/tests.rs"))
    );
    assert!(
        roots.is_root(&root.join("examples/probe.rs")),
        "`examples/probe.rs` is a target of this package: {:?}",
        roots.roots().collect::<Vec<_>>()
    );
    assert_eq!(
        named("examples/probe.rs", &[], "helper").map(|pair| pair[0].clone()),
        Ok(root.join("examples/helper.rs"))
    );
    assert_eq!(
        named("src/a/lib.rs", &[], "tests").map(|pair| pair[0].clone()),
        Ok(root.join("src/a/lib/tests.rs"))
    );
    assert_eq!(
        named("src/a/b/main.rs", &[], "tests").map(|pair| pair[0].clone()),
        Ok(root.join("src/a/b/main/tests.rs"))
    );
    assert_eq!(
        module_directory(roots, &root.join("src/a/mod.rs")),
        Ok(root.join("src/a"))
    );
    assert_eq!(
        module_directory(roots, &root.join("src/a/other.rs")),
        Ok(root.join("src/a/other")),
        "an ordinary module owns a directory named after it, never its parent"
    );
    let elsewhere = std::env::temp_dir().join("upstroke-not-this-package/src/lib.rs");
    assert_eq!(
        module_directory(roots, &elsewhere),
        Err(CandidateRefusal::OutsideThePackage {
            declared_in: elsewhere.clone(),
            package_dir: root.clone(),
        })
    );
    assert!(
        CandidateRefusal::OutsideThePackage {
            declared_in: elsewhere,
            package_dir: root.clone(),
        }
        .to_string()
        .contains("does not say whether it is a crate root"),
        "the refusal says what it could not decide"
    );

    let pair = named("src/a.rs", &[], "b").expect("an ordinary module");
    assert_eq!(sole_present(&pair, &|_| false), Err(0));
    assert_eq!(sole_present(&pair, &|_| true), Err(2));
    assert_eq!(sole_present(&pair, &|at| at == pair[0]), Ok(&pair[0]));
    assert_eq!(sole_present(&pair, &|at| at == pair[1]), Ok(&pair[1]));

    let base = Path::new("src/agent");
    assert!(contained_in(
        base,
        Path::new("src/agent/proc/test_support/readiness.rs")
    ));
    assert!(
        !contained_in(base, base),
        "a directory does not contain itself"
    );
    assert!(!contained_in(base, Path::new("src/effects.rs")));
    assert!(
        !contained_in(base, Path::new("src/agent/../effects.rs")),
        "a `..` component escapes and must not read as contained"
    );

    let edge = |from: &str, to: &str| (PathBuf::from(from), PathBuf::from(to));
    let forest = vec![edge("a.rs", "a/b.rs"), edge("a/b.rs", "a/b/c.rs")];
    assert_eq!(declaration_cycle(&forest), None);
    assert!(
        declaration_cycle(&[edge("a.rs", "a.rs")]).is_some(),
        "a file declaring itself is a cycle"
    );
    assert!(
        declaration_cycle(&[edge("a.rs", "b.rs"), edge("b.rs", "a.rs")]).is_some(),
        "a two-file loop is a cycle"
    );
    let branching = vec![
        edge("a.rs", "a/b.rs"),
        edge("a.rs", "a/c.rs"),
        edge("a/c.rs", "a.rs"),
    ];
    let closed = declaration_cycle(&branching).expect("the second edge closes a loop");
    assert_eq!(
        closed.first(),
        closed.last(),
        "a reported cycle must start and end at the same node: {closed:?}"
    );
    assert!(
        closed.contains(&PathBuf::from("a/c.rs")),
        "the reported cycle does not name the branch that closes it: {closed:?}"
    );
    assert_eq!(
        declaration_cycle(&[edge("a.rs", "a/b.rs"), edge("a.rs", "a/c.rs")]),
        None
    );
    let deferred = vec![
        edge("a.rs", "a/b.rs"),
        edge("a/b.rs", "a/b/leaf.rs"),
        edge("a.rs", "a/c.rs"),
        edge("a/c.rs", "a/d.rs"),
        edge("a/d.rs", "a/c.rs"),
    ];
    assert!(
        declaration_cycle(&deferred).is_some(),
        "a cycle two branches deep was not reached"
    );
}

#[test]
#[should_panic(expected = "does not describe the tree this census was handed")]
fn a_census_handed_a_source_root_the_manifest_does_not_describe_is_refused() {
    let elsewhere = std::env::temp_dir().join("upstroke-not-this-package");
    let _ = crate::effects::census_domain::declared_whole_file_test_modules(&elsewhere, &[]);
}

#[test]
fn the_cfg_census_resolves_module_directories_through_the_target_inventory() {
    for (file, directory) in [
        ("src/lib.rs", "src"),
        ("src/main.rs", "src"),
        ("examples/probe.rs", "examples"),
        ("src/engine/mod.rs", "src/engine"),
        ("src/effects.rs", "src/effects"),
        ("src/a/lib.rs", "src/a/lib"),
        ("src/a/main.rs", "src/a/main"),
    ] {
        assert_eq!(cfg::module_dir(file), directory, "`{file}`");
    }
}

#[test]
fn the_crate_roots_come_from_the_manifest_and_an_arbitrary_bin_path_is_one() {
    use crate::effects::census_domain::{CrateRoots, InventoryRefusal, module_directory};

    fn by_stem(file: &Path) -> PathBuf {
        let parent = file.parent().expect("a directory").to_path_buf();
        let stem = file.file_stem().expect("a name");
        if stem == "mod" || stem == "lib" || stem == "main" {
            parent
        } else {
            parent.join(stem)
        }
    }

    let scratch = scratch_dir("inventory");
    fs::write(
        scratch.join("Cargo.toml"),
        "[package]\n\
         name = \"upstroke-inventory-fixture\"\n\
         version = \"0.0.0\"\n\
         edition = \"2021\"\n\
         \n\
         [lib]\n\
         path = \"src/lib.rs\"\n\
         \n\
         [[bin]]\n\
         name = \"odd\"\n\
         path = \"src/tools/odd.rs\"\n\
         \n\
         [[bin]]\n\
         name = \"nested\"\n\
         path = \"src/deep/nest/main.rs\"\n\
         \n\
         [workspace]\n",
    )
    .expect("the fixture manifest");

    let inventory = crate_roots_of(&scratch).expect("cargo reads the fixture manifest");
    assert_eq!(inventory.package_dir(), scratch.as_path());
    assert_eq!(
        inventory.roots().collect::<Vec<_>>(),
        vec![
            scratch.join("src/deep/nest/main.rs").as_path(),
            scratch.join("src/lib.rs").as_path(),
            scratch.join("src/tools/odd.rs").as_path(),
        ],
        "the inventory is exactly the manifest's three targets"
    );

    for (file, owns, stem_says) in [
        ("src/tools/odd.rs", "src/tools", "src/tools/odd"),
        ("src/deep/nest/main.rs", "src/deep/nest", "src/deep/nest"),
        ("src/a/lib.rs", "src/a/lib", "src/a"),
    ] {
        let declared_in = scratch.join(file);
        assert_eq!(
            module_directory(&inventory, &declared_in),
            Ok(scratch.join(owns)),
            "`{file}` owns `{owns}`"
        );
        assert_eq!(
            by_stem(&declared_in),
            scratch.join(stem_says),
            "the stem rule's answer for `{file}` is recorded, not guessed"
        );
    }
    let disagreements = ["src/tools/odd.rs", "src/a/lib.rs"]
        .into_iter()
        .filter(|file| {
            let declared_in = scratch.join(file);
            module_directory(&inventory, &declared_in) != Ok(by_stem(&declared_in))
        })
        .count();
    assert_eq!(
        disagreements, 2,
        "the manifest and the stem rule must disagree on the arbitrary bin path and on the \
         nested `lib.rs`, or this control measures nothing"
    );

    let missing = scratch.join("no-such-package");
    assert!(
        matches!(
            crate_roots_of(&missing),
            Err(InventoryRefusal::Failed { .. })
        ),
        "a manifest that does not exist is a refusal, not an empty inventory"
    );
    let manifest = scratch.join("Cargo.toml");
    let refusals: Vec<InventoryRefusal> = [
        "this is not json",
        "{}",
        "{\"packages\":[]}",
        "{\"packages\":[{\"manifest_path\":\"/somewhere/else/Cargo.toml\",\"targets\":[{\"src_path\":\"/somewhere/else/src/lib.rs\"}]}]}",
        "{\"packages\":[{\"manifest_path\":\"PLACEHOLDER\",\"targets\":[]}]}",
        "{\"packages\":[{\"manifest_path\":\"PLACEHOLDER\",\"targets\":[{\"name\":\"x\"}]}]}",
    ]
    .into_iter()
    .map(|document| {
        let document = document.replace(
            "PLACEHOLDER",
            &manifest.display().to_string().replace('\\', "\\\\"),
        );
        CrateRoots::from_metadata_json(&document, &manifest).expect_err("this document is refused")
    })
    .collect();
    assert!(
        matches!(refusals[0], InventoryRefusal::Unreadable { .. }),
        "{:?}",
        refusals[0]
    );
    assert!(
        matches!(refusals[1], InventoryRefusal::Unreadable { .. }),
        "{:?}",
        refusals[1]
    );
    assert!(
        matches!(refusals[2], InventoryRefusal::NoPackage { .. }),
        "{:?}",
        refusals[2]
    );
    assert!(
        matches!(refusals[3], InventoryRefusal::NoPackage { .. }),
        "a document describing a different package is refused rather than adopted: {:?}",
        refusals[3]
    );
    assert!(
        matches!(refusals[4], InventoryRefusal::NoTargets { .. }),
        "{:?}",
        refusals[4]
    );
    assert!(
        matches!(refusals[5], InventoryRefusal::Unreadable { .. }),
        "a target with no `src_path` is unreadable rather than skipped: {:?}",
        refusals[5]
    );
    for refusal in &refusals {
        assert!(
            refusal.to_string().contains("cargo metadata")
                || refusal.to_string().contains("declares no target"),
            "the refusal names the authority it could not reach: {refusal}"
        );
    }

    let live = crate::effects::tests::crate_roots();
    assert_eq!(live.package_dir(), repo_root().as_path());
    assert_eq!(
        live.roots().collect::<Vec<_>>(),
        vec![
            repo_root().join("examples/probe.rs").as_path(),
            repo_root().join("src/lib.rs").as_path(),
            repo_root().join("src/main.rs").as_path(),
        ],
        "this package's exact target inventory"
    );

    let _ = fs::remove_dir_all(&scratch);
}

#[test]
fn the_file_level_lint_reader_is_a_census_instrument_and_not_a_shipped_api() {
    fn absent_from_production(source: &str) -> Vec<String> {
        let production = crate::effects::production_code(source);
        let whole = blank_comments_and_strings(source);
        let mut wrong = Vec::new();
        for needle in [
            "fn file_level_lint_state(",
            "fn names_lint(",
            "mod lint_levels",
            // `PR57-FINAL-002`. Every part of the structural parser
            // `PR57-FINAL-001` added is a census-only helper and each one is
            // named here: a helper that drifts above the `#[cfg(test)]` cut is
            // a shipped surface added for a test to call, which is the finding
            // this file already carries once.
            "struct Applied {",
            "enum Truth {",
            "fn evaluate(",
            "fn read_attribute(",
            "fn resolve(",
            "fn split_call(",
            "fn split_top_level(",
            "fn mentions(",
        ] {
            if !whole.contains(needle) {
                wrong.push(format!("`{needle}` is not in src/effects.rs at all"));
            }
            if production.contains(needle) {
                wrong.push(format!(
                    "`{needle}` survives into the production region, which makes it a shipped \
                     surface rather than a census instrument"
                ));
            }
        }
        wrong
    }

    let source = fs::read_to_string(repo_root().join("src/effects.rs")).expect("src/effects.rs");
    assert!(
        absent_from_production(&source).is_empty(),
        "{:#?}",
        absent_from_production(&source)
    );
    let crlf = source.replace('\n', "\r\n");
    assert!(
        absent_from_production(&crlf).is_empty(),
        "{:#?}",
        absent_from_production(&crlf)
    );

    assert!(
        blank_comments_and_strings(&source).contains("pub(crate) mod lint_levels"),
        "the lint reader's module is no longer `pub(crate)`"
    );
    assert!(
        !blank_comments_and_strings(&source).contains("pub mod lint_levels"),
        "the lint reader's module is `pub`, which is the surface this repair removed"
    );

    // **Every item the module exposes at all, as a set.** `PR57-FINAL-002`
    // asks whether a census-only helper became callable from outside; a needle
    // per helper answers it only for the helpers somebody remembered to name.
    // This reads the module's own text and compares what is visible for
    // equality, so a *new* helper that arrives `pub(crate)` fails without
    // having to be listed, and any `pub` at all fails twice — once here and
    // once against `upstroke::effects`, which gains no surface from a
    // `#[cfg(test)]` module and must gain none from this repair.
    //
    // Read from the blanked text: a doc comment saying `pub` is spaces by then,
    // which is what `PR4-CENSUS-COMMENT-ORACLE` is about, and the blanking
    // preserves the line structure this walks.
    let blanked = blank_comments_and_strings(&source);
    let braces = blanked
        .find("mod lint_levels {")
        .map(|at| at + "mod lint_levels ".len())
        .expect("the reader's module is in this file");
    let close = crate::effects::matching(blanked.as_bytes(), braces, b'{', b'}')
        .expect("the reader's module closes");
    // Whitespace inside a declaration is normalised, so the needles say what is
    // visible rather than how rustfmt happened to space it.
    let squeeze = |line: &str| line.split_whitespace().collect::<Vec<_>>().join(" ");
    let visible: BTreeSet<String> = blanked[braces + 1..close]
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("pub"))
        .map(squeeze)
        .collect();
    assert_eq!(
        visible,
        [
            "pub(crate) struct Resolution {",
            "pub(crate) level: Option<&'static str>,",
            "pub(crate) refused_downgrade: bool,",
            "pub(crate) ambiguous: bool,",
            "pub(crate) fn file_level_lint_resolution(source: &str, lint: &str) -> Resolution {",
            "pub(crate) fn file_level_lint_state(source: &str, lint: &str) \
             -> Option<&'static str> {",
        ]
        .into_iter()
        .map(squeeze)
        .collect::<BTreeSet<String>>(),
        "the lint reader's visible surface moved. The two functions and the answer type are \
         what `effects::tests` and `runner::container::tests` call; everything else the module \
         holds is a helper and stays private"
    );

    // **`PR74-GOVERNED-LINT-TOKEN-001`'s tokeniser is private.** Its helpers
    // cannot be `#[cfg(test)]` the way the level reader's are: `governed_allows`
    // is a `pub fn` OF THE PRODUCTION REGION and calls them, so a gated helper
    // would not compile. The claim that is available is the other half of
    // `PR72-API-001` — they add no name to `upstroke::effects` — and it is read
    // structurally rather than promised. `matching` is in the list as the
    // precedent: the class is "private structural helper of this file", and it
    // had one before this repair added three.
    for helper in [
        "fn calls_named",
        "fn identifier_token",
        "fn skip_blank",
        "fn matching",
    ] {
        assert!(
            blank_comments_and_strings(&source).contains(helper),
            "`{helper}` is not in src/effects.rs at all, so the claim below is vacuous"
        );
        for visibility in ["pub ", "pub(crate) ", "pub(super) ", "pub(in "] {
            assert!(
                !blank_comments_and_strings(&source).contains(&format!("{visibility}{helper}")),
                "`{helper}` is declared `{visibility}`, which puts a scan nothing outside this                  file runs on the crate's surface"
            );
        }
    }

    // The instrument still answers where it is used, so narrowing it did not
    // narrow it out of existence — under both spellings of a line ending.
    for prologue in [
        "#![deny(clippy::disallowed_types)]\n",
        "#![deny(clippy::disallowed_types)]\r\n",
        "//! docs\r\n#![allow(clippy::too_many_arguments)]\r\n#![forbid(clippy::disallowed_macros)]\r\n",
    ] {
        let wanted = if prologue.contains("forbid") {
            ("clippy::disallowed_macros", Some("forbid"))
        } else {
            ("clippy::disallowed_types", Some("deny"))
        };
        assert_eq!(
            crate::effects::lint_levels::file_level_lint_state(prologue, wanted.0),
            wanted.1,
            "{prologue:?}"
        );
    }
}

/// Compile one prologue under this repository's own `clippy.toml` and report
/// whether it built, plus every diagnostic that carries a code, as `(level, code)`.
///
/// Shared by the two file-level-lint-reader tests so both are measured by the
/// same oracle: the reader is never its own authority for what rustc does.
fn compile_prologue_probe(dir: &Path, tag: &str, source: &str) -> (bool, Vec<(String, String)>) {
    let file = dir.join(format!("{tag}.rs"));
    fs::write(&file, source).expect("the fixture");
    let out = dir.join("out");
    fs::create_dir_all(&out).expect("an output directory");
    let output = std::process::Command::new(clippy_driver())
        .env("CLIPPY_CONF_DIR", repo_root())
        .args([
            "--edition",
            "2024",
            "--crate-type",
            "lib",
            "--emit=metadata",
            "--error-format=json",
        ])
        .arg("--out-dir")
        .arg(&out)
        .arg(&file)
        .output()
        .expect("clippy-driver runs; the lint gate uses the same binary");
    let mut diagnostics = Vec::new();
    for line in String::from_utf8_lossy(&output.stderr).lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(code) = value
            .get("code")
            .and_then(|code| code.get("code"))
            .and_then(serde_json::Value::as_str)
        else {
            continue;
        };
        let level = value
            .get("level")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        diagnostics.push((level.to_owned(), code.to_owned()));
    }
    (output.status.success(), diagnostics)
}

/// **The file-level lint reader answers what rustc does**, on a table rustc
/// decides.
///
/// `PR72-LEVELS-001`. The reader returned at the *first* attribute naming the
/// lint, and a prologue is ordered: `#![deny(L)] #![allow(L)]` is a file where
/// `L` is allowed, and the reader called it a denial. Two censuses turn on that
/// answer — `every_allow_of_a_governed_lint_is_module_level_and_in_the_allowlist`
/// here and `runner::container::tests::every_child_module_of_the_container_
/// funnel_states_its_own_lint_level` — and the wrong answer is the reassuring
/// one: a module reported as having closed `PR6-LANEF-004` by a prologue whose
/// next line reopens it.
///
/// **No lexical restatement is accepted as authority.** The table below does not
/// say what each prologue means. Each row is compiled by `clippy-driver` under
/// this repository's own `clippy.toml`, against a body that reaches
/// `std::fs::write` — a denied path — and the *observed* diagnostics are the
/// verdict. The reader is asked the same question and its answer is turned into
/// a prediction of what the compiler must have emitted; the two are compared.
/// The only sentence written by hand is the bridge between a level and its
/// observable, and every arm of that bridge is exercised by a row, so a bridge
/// that was wrong could not stay green.
///
/// The rows include the two shapes that are the whole reason for the repair —
/// `deny` then `allow`, which must be **allow**, and `forbid` then `allow`,
/// which is `E0453` and not a level at all — and the decoys the blanking exists
/// for.
#[test]
fn the_file_level_lint_reader_answers_what_rustc_does() {
    use crate::effects::lint_levels::{Resolution, file_level_lint_resolution};

    const BODY: &str = "pub fn go(p: &std::path::Path) { let _ = std::fs::write(p, \"x\"); }\n";
    const LINT: &str = "clippy::disallowed_methods";

    /// What the compiler must have done, if the reader's answer is right.
    ///
    /// `(the crate builds, the levels at which the lint fired, E0453 present)`.
    /// The one hand-written sentence in this test, and every arm of it is
    /// reached by a row below.
    fn predict(resolution: Resolution) -> (bool, Vec<&'static str>, bool) {
        assert!(
            !resolution.ambiguous,
            "no prologue in this table is conditional, so nothing here may read as a refusal; \
             the conditional shapes are in \
             `the_file_level_lint_reader_refuses_a_condition_it_cannot_prove`"
        );
        if resolution.refused_downgrade {
            return (false, Vec::new(), true);
        }
        match resolution.level {
            Some("allow" | "expect") => (true, Vec::new(), false),
            None | Some("warn") => (true, vec!["warning"], false),
            Some("deny" | "forbid") => (false, vec!["error"], false),
            other => panic!("the reader answered `{other:?}`, which nothing predicts"),
        }
    }

    let scratch = scratch_dir("levels");
    let table: &[(&str, &str)] = &[
        ("bare", ""),
        ("allow", "#![allow(clippy::disallowed_methods)]\n"),
        ("warn", "#![warn(clippy::disallowed_methods)]\n"),
        ("deny", "#![deny(clippy::disallowed_methods)]\n"),
        ("forbid", "#![forbid(clippy::disallowed_methods)]\n"),
        ("expect", "#![expect(clippy::disallowed_methods)]\n"),
        (
            "deny_then_allow",
            "#![deny(clippy::disallowed_methods)]\n#![allow(clippy::disallowed_methods)]\n",
        ),
        (
            "allow_then_deny",
            "#![allow(clippy::disallowed_methods)]\n#![deny(clippy::disallowed_methods)]\n",
        ),
        (
            "deny_then_warn",
            "#![deny(clippy::disallowed_methods)]\n#![warn(clippy::disallowed_methods)]\n",
        ),
        (
            "deny_then_expect",
            "#![deny(clippy::disallowed_methods)]\n#![expect(clippy::disallowed_methods)]\n",
        ),
        (
            "allow_warn_deny",
            "#![allow(clippy::disallowed_methods)]\n#![warn(clippy::disallowed_methods)]\n\
             #![deny(clippy::disallowed_methods)]\n",
        ),
        (
            "allow_then_forbid",
            "#![allow(clippy::disallowed_methods)]\n#![forbid(clippy::disallowed_methods)]\n",
        ),
        (
            "forbid_then_allow",
            "#![forbid(clippy::disallowed_methods)]\n#![allow(clippy::disallowed_methods)]\n",
        ),
        (
            "forbid_then_warn",
            "#![forbid(clippy::disallowed_methods)]\n#![warn(clippy::disallowed_methods)]\n",
        ),
        (
            "forbid_then_deny",
            "#![forbid(clippy::disallowed_methods)]\n#![deny(clippy::disallowed_methods)]\n",
        ),
        (
            "deny_then_allow_bare",
            "#![deny(clippy::disallowed_methods)]\n#![allow(disallowed_methods)]\n",
        ),
        (
            "prose_decoy",
            "//! `#![allow(clippy::disallowed_methods)]` is written here in prose.\n\
             #![deny(clippy::disallowed_methods)]\n",
        ),
        (
            "attribute_after_the_prologue",
            "#![deny(clippy::disallowed_methods)]\npub const S: &str = \
             \"#![allow(clippy::disallowed_methods)]\";\n",
        ),
    ];

    let mut observed_shapes: BTreeSet<(bool, Vec<String>, bool)> = BTreeSet::new();
    for (tag, prologue) in table {
        let source = format!("{prologue}{BODY}");
        let resolution = file_level_lint_resolution(&source, LINT);
        let (built, diagnostics) = compile_prologue_probe(&scratch, tag, &source);
        let fired: Vec<String> = diagnostics
            .iter()
            .filter(|(_, code)| code == LINT)
            .map(|(level, _)| level.clone())
            .collect();
        let rejected = diagnostics.iter().any(|(_, code)| code == "E0453");
        let (wants_build, wants_fired, wants_rejected) = predict(resolution);
        assert_eq!(
            (built, fired.clone(), rejected),
            (
                wants_build,
                wants_fired
                    .iter()
                    .map(|level| (*level).to_owned())
                    .collect(),
                wants_rejected
            ),
            "`{tag}` — the reader answered {resolution:?} and clippy-driver did something else: \
             built={built} fired={fired:?} E0453={rejected}; all diagnostics {diagnostics:?}"
        );
        observed_shapes.insert((built, fired, rejected));

        assert_eq!(
            file_level_lint_resolution(&source.replace('\n', "\r\n"), LINT),
            resolution,
            "`{tag}` reads differently under CRLF"
        );
    }

    assert!(
        observed_shapes.len() >= 4,
        "the fixtures produced only {} distinct compiler outcomes: {observed_shapes:?}",
        observed_shapes.len()
    );

    let deny_then_allow = format!(
        "#![deny(clippy::disallowed_methods)]\n#![allow(clippy::disallowed_methods)]\n{BODY}"
    );
    assert_eq!(
        file_level_lint_resolution(&deny_then_allow, LINT),
        Resolution {
            level: Some("allow"),
            refused_downgrade: false,
            ambiguous: false,
        },
        "deny then allow is effectively allow"
    );
    let forbid_then_allow = format!(
        "#![forbid(clippy::disallowed_methods)]\n#![allow(clippy::disallowed_methods)]\n{BODY}"
    );
    assert_eq!(
        file_level_lint_resolution(&forbid_then_allow, LINT),
        Resolution {
            level: Some("forbid"),
            refused_downgrade: true,
            ambiguous: false,
        },
        "a forbid cannot be weakened; the attempt is E0453 and not a level"
    );

    let mut restated = Vec::new();
    for (path, source) in scanned_sources() {
        let blanked = blank_comments_and_strings(&source);
        for lint in USED_GOVERNED_LINTS {
            let bare = normalize_lint(lint).expect("a governed lint");
            let stated = blanked
                .split("#![")
                .skip(1)
                .filter(|attribute| {
                    attribute
                        .split(']')
                        .next()
                        .is_some_and(|body| body.contains(bare))
                })
                .count();
            if stated > 1 {
                restated.push(format!(
                    "{path} states `{lint}` in {stated} inner attributes"
                ));
            }
        }
    }
    assert!(
        restated.is_empty(),
        "the ordered reading is exercised by fixtures only while this holds: {restated:#?}"
    );

    let _ = fs::remove_dir_all(&scratch);
}

/// **The file-level lint reader refuses a condition it cannot prove**, and the
/// compiler decides which conditions those are.
///
/// `PR57-FINAL-001`. The reader read an inner attribute by stripping a level
/// keyword off the front of it, so it understood exactly one shape:
/// `#![deny(L)]` written literally at the top of the file. Everything else it
/// answered `None` to — which is the loud direction for a lone `#![cfg_attr(P,
/// deny(L))]` and the **silent** one for the shape that matters:
///
/// ```text
/// #![deny(clippy::disallowed_methods)]
/// #![cfg_attr(windows, allow(clippy::disallowed_methods))]
/// ```
///
/// The second attribute is invisible to a front-of-string strip, so the reader
/// answered `deny` — and on Windows that file allows the lint. Two censuses act
/// on that answer. `every_allow_of_a_governed_lint_is_module_level_and_in_the_
/// allowlist` admits a per-site `#[expect]` only in a file that DENIES the lint
/// at module level, so the expectation "narrows a denial" that is not there;
/// and `runner::container::tests::every_child_module_of_the_container_funnel_
/// states_its_own_lint_level` reports the module as having closed
/// `PR6-LANEF-004` on every target when it has closed it on all but one.
///
/// # The rule this pins
///
/// A `deny` or `forbid` counts only when it is **unconditional** at module top
/// level, or when its condition is proven true on every supported target. The
/// reader proves exactly two things — `all()` is true everywhere and `any()` is
/// false everywhere, and `not`/`all`/`any` compose over those — and models no
/// target list at all, because a list is a place to be wrong and being wrong
/// here means reporting a module as guarded on a target where it is not. Every
/// other predicate is a **refusal**: `ambiguous`, with no level. Wrongly red is
/// allowed; wrongly green is not.
///
/// # How it is measured
///
/// Section (1) is the same clippy-driver oracle
/// [`the_file_level_lint_reader_answers_what_rustc_does`] uses, restricted to
/// conditions that resolve the same way on ubuntu, macos and windows — so a row
/// here means the same thing on all three CI legs. `PR72-WIN-EOL-003`'s sibling
/// hazard, a fixture whose *meaning* is platform-shaped, is why `windows` and
/// `unix` appear nowhere in that table.
///
/// Section (2) is the slip attempt, and it is measured on whichever host runs
/// it: the condition is chosen to be true here and unprovable everywhere, so
/// clippy-driver really does let a `std::fs::write` through a prologue that
/// opens with `#![deny]`. Its mirror, the same shape under a condition false on
/// this host, really does refuse it. **One reader answer covers both compiler
/// behaviours**, which is the whole argument for refusing rather than picking.
#[test]
fn the_file_level_lint_reader_refuses_a_condition_it_cannot_prove() {
    use crate::effects::lint_levels::{Resolution, file_level_lint_resolution};

    /// The same body the sibling oracle uses: one denied path, so any level
    /// that does not suppress a `disallowed_methods` diagnostic produces one.
    const BODY: &str = "pub fn go(p: &std::path::Path) { let _ = std::fs::write(p, \"x\"); }\n";
    const LINT: &str = "clippy::disallowed_methods";

    /// What the compiler must have done, if the reader's answer is right.
    ///
    /// `(the crate builds, the levels at which the lint fired, E0453 present)`.
    /// A refusal predicts nothing — that is what makes it a refusal — so the
    /// rows driven through here are the ones the reader answers.
    fn predict(resolution: Resolution) -> (bool, Vec<&'static str>, bool) {
        assert!(
            !resolution.ambiguous,
            "a refusal predicts no compiler behaviour and must not be driven through this bridge"
        );
        if resolution.refused_downgrade {
            return (false, Vec::new(), true);
        }
        match resolution.level {
            Some("allow" | "expect") => (true, Vec::new(), false),
            None | Some("warn") => (true, vec!["warning"], false),
            Some("deny" | "forbid") => (false, vec!["error"], false),
            other => panic!("the reader answered `{other:?}`, which nothing predicts"),
        }
    }

    let scratch = scratch_dir("conditional-levels");

    // -----------------------------------------------------------------
    // (1) The conditions that resolve identically on every CI leg.
    //
    // Every row is a prologue and nothing here says what it means: the reader
    // is asked, its answer is turned into a prediction, and clippy-driver is
    // the verdict. `all()` is the empty conjunction and is true; `any()` is the
    // empty disjunction and is false. Both are stable across targets, which is
    // what lets a table assert them on ubuntu, macos and windows alike.
    // -----------------------------------------------------------------
    let table: &[(&str, &str)] = &[
        // Inactive on every target: the deny is not there, and a reader that
        // counted it would report a guarded module with an unguarded build.
        (
            "cfg_attr_any_deny",
            "#![cfg_attr(any(), deny(clippy::disallowed_methods))]\n",
        ),
        (
            "cfg_attr_not_all_deny",
            "#![cfg_attr(not(all()), deny(clippy::disallowed_methods))]\n",
        ),
        (
            "cfg_attr_any_forbid",
            "#![cfg_attr(any(), forbid(clippy::disallowed_methods))]\n",
        ),
        // Active on every target: the deny IS there, and a reader that ignored
        // every conditional would report an unguarded module that is guarded.
        // This is the half a blanket "refuse all `cfg_attr`" rule gets wrong.
        (
            "cfg_attr_all_deny",
            "#![cfg_attr(all(), deny(clippy::disallowed_methods))]\n",
        ),
        (
            "cfg_attr_all_forbid",
            "#![cfg_attr(all(), forbid(clippy::disallowed_methods))]\n",
        ),
        (
            "cfg_attr_not_any_deny",
            "#![cfg_attr(not(any()), deny(clippy::disallowed_methods))]\n",
        ),
        // Nesting, both ways round. A `cfg_attr` may carry another, and the
        // condition of the pair is the conjunction: one inactive level anywhere
        // in the chain and the attribute is not applied.
        (
            "nested_all_all_deny",
            "#![cfg_attr(all(), cfg_attr(all(), deny(clippy::disallowed_methods)))]\n",
        ),
        (
            "nested_all_any_deny",
            "#![cfg_attr(all(), cfg_attr(any(), deny(clippy::disallowed_methods)))]\n",
        ),
        (
            "nested_any_all_deny",
            "#![cfg_attr(any(), cfg_attr(all(), deny(clippy::disallowed_methods)))]\n",
        ),
        // A `cfg_attr` applies EVERY attribute after its condition, not just
        // the first. A reader that took `terms[1]` and stopped would miss this
        // deny entirely.
        (
            "cfg_attr_two_attributes",
            "#![cfg_attr(all(), allow(dead_code), deny(clippy::disallowed_methods))]\n",
        ),
        // Ordering still decides, and a conditional takes its place in the
        // order. An inactive allow after a deny leaves the deny standing; an
        // active one replaces it, and the effect really does slip through.
        (
            "deny_then_inactive_allow",
            "#![deny(clippy::disallowed_methods)]\n\
             #![cfg_attr(any(), allow(clippy::disallowed_methods))]\n",
        ),
        (
            "deny_then_active_allow",
            "#![deny(clippy::disallowed_methods)]\n\
             #![cfg_attr(all(), allow(clippy::disallowed_methods))]\n",
        ),
        // `forbid` is sticky against a conditional too: the weakening that is
        // actually applied is `E0453` and the crate does not build, and the one
        // that is not applied is not a weakening at all.
        (
            "forbid_then_active_allow",
            "#![forbid(clippy::disallowed_methods)]\n\
             #![cfg_attr(all(), allow(clippy::disallowed_methods))]\n",
        ),
        (
            "forbid_then_inactive_allow",
            "#![forbid(clippy::disallowed_methods)]\n\
             #![cfg_attr(any(), allow(clippy::disallowed_methods))]\n",
        ),
    ];

    let mut observed_shapes: BTreeSet<(bool, Vec<String>, bool)> = BTreeSet::new();
    for (tag, prologue) in table {
        let source = format!("{prologue}{BODY}");
        let resolution = file_level_lint_resolution(&source, LINT);
        let (built, diagnostics) = compile_prologue_probe(&scratch, tag, &source);
        let fired: Vec<String> = diagnostics
            .iter()
            .filter(|(_, code)| code == LINT)
            .map(|(level, _)| level.clone())
            .collect();
        let rejected = diagnostics.iter().any(|(_, code)| code == "E0453");
        let (wants_build, wants_fired, wants_rejected) = predict(resolution);
        assert_eq!(
            (built, fired.clone(), rejected),
            (
                wants_build,
                wants_fired
                    .iter()
                    .map(|level| (*level).to_owned())
                    .collect(),
                wants_rejected
            ),
            "`{tag}` — the reader answered {resolution:?} and clippy-driver did something else: \
             built={built} fired={fired:?} E0453={rejected}; all diagnostics {diagnostics:?}"
        );
        observed_shapes.insert((built, fired, rejected));

        // The same prologue with the line endings the Windows guest gives it.
        assert_eq!(
            file_level_lint_resolution(&source.replace('\n', "\r\n"), LINT),
            resolution,
            "`{tag}` reads differently under CRLF"
        );
    }

    // The table reaches all four compiler behaviours — clean, warned, errored
    // and rejected outright — so a reader collapsed to one answer cannot pass
    // it, and neither can one that refuses every conditional it sees.
    assert!(
        observed_shapes.len() >= 4,
        "the conditional fixtures produced only {} distinct compiler outcomes: {observed_shapes:?}",
        observed_shapes.len()
    );

    // -----------------------------------------------------------------
    // (2) The slip attempt, measured on this host.
    //
    // `HOST_TRUE` holds on whichever CI leg is running and is unprovable to the
    // reader on all of them; `HOST_FALSE` is its complement. The prologue is
    // the one a reviewer reads as a denial in both cases.
    // -----------------------------------------------------------------
    let (host_true, host_false) = if cfg!(windows) {
        ("windows", "unix")
    } else {
        ("unix", "windows")
    };
    for (tag, condition, effect_slipped) in [
        ("slip_here", host_true, true),
        ("slip_elsewhere", host_false, false),
    ] {
        let source = format!(
            "#![deny(clippy::disallowed_methods)]\n\
             #![cfg_attr({condition}, allow(clippy::disallowed_methods))]\n{BODY}"
        );
        let resolution = file_level_lint_resolution(&source, LINT);
        assert_eq!(
            resolution,
            Resolution {
                level: None,
                refused_downgrade: false,
                ambiguous: true,
            },
            "`{tag}` — a prologue whose denial is undone on some supported target must be a \
             refusal, not a level"
        );

        let (built, diagnostics) = compile_prologue_probe(&scratch, tag, &source);
        let errored = diagnostics
            .iter()
            .any(|(level, code)| level == "error" && code == LINT);
        assert_eq!(
            (built, errored),
            (effect_slipped, !effect_slipped),
            "`{tag}` — `#![cfg_attr({condition}, allow(..))]` after a deny did not behave as this \
             host requires: built={built} diagnostics={diagnostics:?}"
        );
    }

    // -----------------------------------------------------------------
    // (3) Every other predicate is a refusal, and refusing is not the same as
    // reading nothing. These cannot be compiled portably — that is exactly why
    // the reader may not guess at them — so the claim asserted is the reader's,
    // and it is asserted as a value.
    // -----------------------------------------------------------------
    const REFUSED: &[(&str, &str)] = &[
        (
            "target_family",
            "#![cfg_attr(windows, deny(clippy::disallowed_methods))]\n",
        ),
        (
            "target_family_negated",
            "#![cfg_attr(not(unix), deny(clippy::disallowed_methods))]\n",
        ),
        (
            "target_os",
            "#![cfg_attr(target_os = \"linux\", deny(clippy::disallowed_methods))]\n",
        ),
        (
            "cfg_test",
            "#![cfg_attr(test, deny(clippy::disallowed_methods))]\n",
        ),
        (
            "not_cfg_test",
            "#![cfg_attr(not(test), deny(clippy::disallowed_methods))]\n",
        ),
        (
            "feature",
            "#![cfg_attr(feature = \"strict\", deny(clippy::disallowed_methods))]\n",
        ),
        // A disjunction the reader could be tempted to call exhaustive. It is
        // not exhaustive to a reader that models no target list, and the tree
        // does not gain one for this.
        (
            "any_of_two_families",
            "#![cfg_attr(any(windows, unix), deny(clippy::disallowed_methods))]\n",
        ),
        // One unprovable term anywhere in the chain refuses the whole of it.
        (
            "all_with_one_unprovable",
            "#![cfg_attr(all(all(), unix), deny(clippy::disallowed_methods))]\n",
        ),
        (
            "nested_outer_unprovable",
            "#![cfg_attr(unix, cfg_attr(all(), deny(clippy::disallowed_methods)))]\n",
        ),
        (
            "nested_inner_unprovable",
            "#![cfg_attr(all(), cfg_attr(unix, deny(clippy::disallowed_methods)))]\n",
        ),
        // THE ONE THIS REPAIR IS NAMED FOR: a denial undone on one target.
        (
            "deny_then_conditional_allow",
            "#![deny(clippy::disallowed_methods)]\n\
             #![cfg_attr(windows, allow(clippy::disallowed_methods))]\n",
        ),
        // And the same against a forbid. Whether this file is `E0453` or a
        // clean forbid depends on the target, so it is neither a level nor a
        // refused downgrade: it is a refusal.
        (
            "forbid_then_conditional_allow",
            "#![forbid(clippy::disallowed_methods)]\n\
             #![cfg_attr(windows, allow(clippy::disallowed_methods))]\n",
        ),
        // An attribute this reader does not understand, that names the lint.
        // Silence about it would be a reader guessing, and the guess it would
        // make is that nothing is stated. Three ways to be unreadable, because
        // they leave the parser at three different points: brackets that never
        // close, a head that is not a level and is not `cfg_attr`, and one of
        // those nested inside a condition that IS proven.
        (
            "unreadable_brackets",
            "#![cfg_attr(all(), deny(clippy::disallowed_methods)]\n",
        ),
        // `allowance` begins with `allow` and is not it. The reader this
        // replaced stripped level keywords off the front of an attribute and
        // had to carry a comment about exactly this word; a structural reader
        // sees a call named `allowance`, does not know it, and refuses because
        // it names the lint.
        (
            "head_that_is_not_a_level",
            "#![allowance(clippy::disallowed_methods)]\n",
        ),
        (
            "unreadable_under_a_proven_condition",
            "#![cfg_attr(all(), lint_group(deny(clippy::disallowed_methods)))]\n",
        ),
    ];
    for (tag, prologue) in REFUSED {
        let source = format!("{prologue}{BODY}");
        for spelling in [source.clone(), source.replace('\n', "\r\n")] {
            assert_eq!(
                file_level_lint_resolution(&spelling, LINT),
                Resolution {
                    level: None,
                    refused_downgrade: false,
                    ambiguous: true,
                },
                "`{tag}` must be a refusal"
            );
            // The census-facing answer is the one that matters, and it is the
            // fail-closed one: no per-site `#[expect]` is admitted against it,
            // and no child module reads as having stated its own level.
            assert_eq!(
                crate::effects::lint_levels::file_level_lint_state(&spelling, LINT),
                None,
                "`{tag}` reached a census as a level"
            );
            assert!(
                !file_level_denies(&spelling, LINT),
                "`{tag}` read as a denial"
            );
        }
    }

    // -----------------------------------------------------------------
    // (4) A refusal is not the only way to be silent, and the two are
    // distinguished. An inherited allow — a child module that states nothing
    // and takes its level from the module tree above it, which is
    // `PR6-LANEF-004` itself — reads as nothing STATED and is not a refusal;
    // the reader never reaches for the parent it cannot see.
    // -----------------------------------------------------------------
    let inheriting = format!("//! A child module of a funnel that allows the lint.\n{BODY}");
    assert_eq!(
        file_level_lint_resolution(&inheriting, LINT),
        Resolution {
            level: None,
            refused_downgrade: false,
            ambiguous: false,
        },
        "a module that states nothing states nothing; it does not state its ancestor's allow"
    );
    // And an unrelated conditional attribute is not about this lint at all.
    // `src/engine/assembly.rs` and `src/engine/topology.rs` both open with one.
    let unrelated = format!("#![cfg_attr(not(test), allow(dead_code))]\n{BODY}");
    assert_eq!(
        file_level_lint_resolution(&unrelated, LINT),
        Resolution {
            level: None,
            refused_downgrade: false,
            ambiguous: false,
        },
        "a conditional attribute naming another lint is not an ambiguity about this one"
    );
    // Two files in this tree really do carry that prologue, so the claim above
    // is about the tree and not only about a fixture.
    for path in ["src/engine/assembly.rs", "src/engine/topology.rs"] {
        let source = fs::read_to_string(repo_root().join(path)).expect("an engine module");
        for lint in USED_GOVERNED_LINTS {
            assert!(
                !file_level_lint_resolution(&source, lint).ambiguous,
                "{path} reads as a refusal for `{lint}`, so this repair turned a green tree red"
            );
        }
    }

    // -----------------------------------------------------------------
    // (5) The other reader fails closed in the other direction, so nothing
    // slips between them. `governed_allows` finds an `allow`/`expect` at any
    // depth, conditional or not — so a conditional allow still has to be in
    // `effects/allowlist.toml`, and cannot hide from the placement census by
    // wearing a `cfg_attr`.
    // -----------------------------------------------------------------
    for prologue in [
        "#![cfg_attr(any(), allow(clippy::disallowed_methods))]\n",
        "#![cfg_attr(windows, allow(clippy::disallowed_methods))]\n",
        "#![cfg_attr(all(), cfg_attr(all(), allow(clippy::disallowed_methods)))]\n",
    ] {
        let found = governed_allows(&format!("{prologue}{BODY}"));
        assert_eq!(
            found.len(),
            1,
            "a conditional allow is invisible to the placement census: {prologue:?}"
        );
        assert_eq!(found[0].lints, ["disallowed_methods"], "{prologue:?}");
        assert!(found[0].inner, "{prologue:?}");
    }

    let _ = fs::remove_dir_all(&scratch);
}

/// **Both governed-lint readers consume tokens**, and a raw identifier is the
/// identifier it spells.
///
/// `PR74-GOVERNED-LINT-TOKEN-001`. Two readers decide whether a file has
/// allowed a governed lint, and both decided it by looking at bytes rather than
/// at tokens:
///
/// * [`governed_allows`] found the keyword and then required the **very next
///   byte** to be `(`. Rust does not: `#![allow /* why */ (clippy::…)]` and the
///   same attribute with a newline before its parenthesis are ordinary
///   attributes, and both were invisible to the placement census — so a file
///   could allow a governed lint with no `effects/allowlist.toml` row and
///   nothing would say so.
/// * [`normalize_lint`] took the last `::` segment verbatim, so
///   `clippy::r#disallowed_methods` was not `clippy::disallowed_methods`. A raw
///   identifier is the same identifier to rustc — it exists so a keyword can be
///   used as a name and applies to any identifier at all — so that spelling is
///   the governed lint, allowed or denied, and neither reader saw it.
///
/// The consequence is the one `PR57-FINAL-001` was about, reached by a
/// different route: a prologue that denies the lint and then takes it back in a
/// spelling the reader cannot see reads as a denial. Section (1) compiles both
/// such prologues and they **build clean** — the denied `std::fs::write` really
/// does slip.
///
/// # Nothing here is asserted from a rule
///
/// Section (1) is the same `clippy-driver` oracle
/// [`the_file_level_lint_reader_answers_what_rustc_does`] uses, so the claim
/// "this spelling means that" is the compiler's and not this file's. Section
/// (2) drives the placement reader over the same spellings; section (3) is the
/// negative half, where a near-miss must NOT be taken for the lint; section (4)
/// is the fail-closed arm.
#[test]
fn the_governed_lint_readers_consume_tokens_and_normalise_raw_identifiers() {
    use crate::effects::lint_levels::{Resolution, file_level_lint_resolution};

    const BODY: &str = "pub fn go(p: &std::path::Path) { let _ = std::fs::write(p, \"x\"); }\n";
    const LINT: &str = "clippy::disallowed_methods";

    /// What the compiler must have done, if the reader's answer is right.
    fn predict(resolution: Resolution) -> (bool, Vec<&'static str>, bool) {
        assert!(
            !resolution.ambiguous,
            "every spelling in this table is one rustc accepts, so refusing it is not the \
             fail-closed answer -- it is the reader still unable to read a token"
        );
        if resolution.refused_downgrade {
            return (false, Vec::new(), true);
        }
        match resolution.level {
            Some("allow" | "expect") => (true, Vec::new(), false),
            None | Some("warn") => (true, vec!["warning"], false),
            Some("deny" | "forbid") => (false, vec!["error"], false),
            other => panic!("the reader answered `{other:?}`, which nothing predicts"),
        }
    }

    let scratch = scratch_dir("lint-tokens");

    // -----------------------------------------------------------------
    // (1) Every spelling, measured. A row says only how it is written; the
    // compiler says what it means and the reader has to agree.
    // -----------------------------------------------------------------
    let table: &[(&str, &str)] = &[
        // The lint path, raw. `r#` on the lint segment, on the tool segment,
        // and on a bare unqualified name.
        (
            "raw_lint_segment_allow",
            "#![allow(clippy::r#disallowed_methods)]\n",
        ),
        (
            "raw_lint_segment_deny",
            "#![deny(clippy::r#disallowed_methods)]\n",
        ),
        (
            "raw_tool_segment_allow",
            "#![allow(r#clippy::disallowed_methods)]\n",
        ),
        ("raw_bare_lint_allow", "#![allow(r#disallowed_methods)]\n"),
        // The attribute's own name, raw.
        (
            "raw_attribute_name_allow",
            "#![r#allow(clippy::disallowed_methods)]\n",
        ),
        (
            "raw_attribute_name_deny",
            "#![r#deny(clippy::disallowed_methods)]\n",
        ),
        // The delimiter, not adjacent. A comment is blanked to spaces before
        // either reader sees it, so this row and the next are the same claim
        // written the two ways a file actually carries it.
        (
            "comment_before_delimiter",
            "#![allow /* why not */ (clippy::disallowed_methods)]\n",
        ),
        (
            "newline_before_delimiter",
            "#![allow\n    (clippy::disallowed_methods)]\n",
        ),
        (
            "comment_before_delimiter_deny",
            "#![deny /* why */ (clippy::disallowed_methods)]\n",
        ),
        // The conditional forms, which is where the two repairs meet.
        (
            "cfg_attr_comment_before_delimiter",
            "#![cfg_attr /* c */ (all(), deny(clippy::disallowed_methods))]\n",
        ),
        (
            "cfg_attr_raw_lint",
            "#![cfg_attr(all(), deny(clippy::r#disallowed_methods))]\n",
        ),
        (
            "cfg_attr_raw_inner_level",
            "#![cfg_attr(all(), r#deny(clippy::disallowed_methods))]\n",
        ),
        (
            "raw_cfg_attr_name",
            "#![r#cfg_attr(all(), deny(clippy::disallowed_methods))]\n",
        ),
        // A raw identifier in the CONDITION. `r#all` is `all`, so this is the
        // proven-true predicate and the deny counts.
        (
            "raw_cfg_predicate",
            "#![cfg_attr(r#all(), deny(clippy::disallowed_methods))]\n",
        ),
        // **THE TWO SLIPS.** A denial taken back in a spelling the reader could
        // not see. Both build clean, so the effect is not denied at all.
        (
            "slip_by_raw_identifier",
            "#![deny(clippy::disallowed_methods)]\n\
             #![allow(clippy::r#disallowed_methods)]\n",
        ),
        (
            "slip_by_comment_before_delimiter",
            "#![deny(clippy::disallowed_methods)]\n\
             #![allow /* taken back */ (clippy::disallowed_methods)]\n",
        ),
    ];

    let mut observed_shapes: BTreeSet<(bool, Vec<String>, bool)> = BTreeSet::new();
    for (tag, prologue) in table {
        let source = format!("{prologue}{BODY}");
        let resolution = file_level_lint_resolution(&source, LINT);
        let (built, diagnostics) = compile_prologue_probe(&scratch, tag, &source);
        let fired: Vec<String> = diagnostics
            .iter()
            .filter(|(_, code)| code == LINT)
            .map(|(level, _)| level.clone())
            .collect();
        let rejected = diagnostics.iter().any(|(_, code)| code == "E0453");
        let (wants_build, wants_fired, wants_rejected) = predict(resolution);
        assert_eq!(
            (built, fired.clone(), rejected),
            (
                wants_build,
                wants_fired
                    .iter()
                    .map(|level| (*level).to_owned())
                    .collect(),
                wants_rejected
            ),
            "`{tag}` — the reader answered {resolution:?} and clippy-driver did something else: \
             built={built} fired={fired:?} E0453={rejected}; all diagnostics {diagnostics:?}"
        );
        observed_shapes.insert((built, fired, rejected));

        assert_eq!(
            file_level_lint_resolution(&source.replace('\n', "\r\n"), LINT),
            resolution,
            "`{tag}` reads differently under CRLF"
        );
    }
    assert!(
        observed_shapes.len() >= 2,
        "every spelling produced the same compiler outcome, so the table cannot separate a \
         reader that sees them from one that does not: {observed_shapes:?}"
    );

    // -----------------------------------------------------------------
    // (2) The placement reader sees them too, and REFUSES them where the
    // census refuses. `mechanism` (2) permits an allowance "only as
    // module-level attributes in files listed in effects/allowlist.toml", and
    // `every_allow_of_a_governed_lint_is_module_level_and_in_the_allowlist`
    // enforces both halves off this scan: an allow it cannot see is an allow
    // that needs no row and may sit anywhere at all.
    // -----------------------------------------------------------------
    for (tag, attribute) in [
        ("raw_lint_segment", "allow(clippy::r#disallowed_methods)"),
        ("raw_tool_segment", "allow(r#clippy::disallowed_methods)"),
        ("raw_bare_lint", "allow(r#disallowed_methods)"),
        ("raw_attribute_name", "r#allow(clippy::disallowed_methods)"),
        (
            "comment_before_delimiter",
            "allow /* why */ (clippy::disallowed_methods)",
        ),
        (
            "newline_before_delimiter",
            "allow\n    (clippy::disallowed_methods)",
        ),
        (
            "raw_expect",
            "r#expect(clippy::r#disallowed_methods, reason = \"r\")",
        ),
        (
            "conditional_raw",
            "cfg_attr(windows, allow(clippy::r#disallowed_methods))",
        ),
    ] {
        // Module level: the scan must find it, so the file needs a row.
        let inner = format!("#![{attribute}]\n{BODY}");
        let found = governed_allows(&inner);
        assert_eq!(
            found.len(),
            1,
            "`{tag}` at module level is invisible to the placement census, so a file may allow \
             a governed lint with no {ALLOWLIST_TOML} row"
        );
        assert_eq!(found[0].lints, ["disallowed_methods"], "`{tag}`");
        assert!(found[0].inner && found[0].module_level, "`{tag}`");

        // Below module level: found, and reported as NOT module level, which is
        // the value the census refuses on.
        let on_a_function = format!("#[{attribute}]\npub fn reaches() {{}}\n{BODY}");
        let found = governed_allows(&on_a_function);
        assert_eq!(found.len(), 1, "`{tag}` on a function is invisible");
        assert!(
            !found[0].module_level,
            "`{tag}` on a function reads as module-level, so the placement rule admits it"
        );
    }

    // -----------------------------------------------------------------
    // (3) The negative half. A near-miss is not the lint, and a spelling that
    // only looks like one governs nothing — otherwise section (2) would be
    // satisfied by a reader that answered "yes" to everything.
    // -----------------------------------------------------------------
    for (tag, source) in [
        // A longer name that ends in the lint's, and one that begins with it.
        (
            "longer_name",
            "#![allow(clippy::disallowed_methods_extra)]\n",
        ),
        (
            "prefixed_name",
            "#![allow(clippy::not_disallowed_methods)]\n",
        ),
        // `r#` is a prefix of the token, not a substring of it.
        ("double_r", "#![allow(clippy::rr#disallowed_methods)]\n"),
        // A different governed lint is a different lint.
        ("another_governed", "#![allow(clippy::disallowed_types)]\n"),
        // The keyword is part of a longer identifier.
        (
            "keyword_inside_a_word",
            "#![allowance(clippy::disallowed_methods)]\n",
        ),
        // And the two decoys the blanking exists for, in the token reader's
        // own terms: a raw spelling in prose and in a string literal.
        (
            "raw_in_prose",
            "//! `#![allow(clippy::r#disallowed_methods)]`\n",
        ),
        (
            "raw_in_a_string",
            "pub const S: &str = \"#![allow(clippy::r#disallowed_methods)]\";\n",
        ),
    ] {
        let found = governed_allows(&format!("{source}{BODY}"));
        assert!(
            found
                .iter()
                .all(|allow| !allow.lints.iter().any(|lint| lint == "disallowed_methods")),
            "`{tag}` was taken for an allow of `disallowed_methods`: {found:#?}"
        );
    }
    // The near-misses are near: the reader answers `None` for them and `Some`
    // for the real thing, so section (3) is not passing because the fixtures
    // are unreadable.
    assert_eq!(
        normalize_lint("clippy::r#disallowed_methods"),
        Some("disallowed_methods")
    );
    assert_eq!(
        normalize_lint("r#disallowed_methods"),
        Some("disallowed_methods")
    );
    assert_eq!(normalize_lint("clippy::disallowed_methods_extra"), None);
    assert_eq!(normalize_lint("clippy::rr#disallowed_methods"), None);
    assert_eq!(normalize_lint("r#"), None);

    // -----------------------------------------------------------------
    // (4) Fail closed on syntax neither reader can resolve. An `allow(` whose
    // parenthesis never closes is not a file that compiles, and the answer that
    // matters is that it is not silently read as carrying no allowance.
    // -----------------------------------------------------------------
    let unbalanced = format!("#![allow(clippy::disallowed_methods]\n{BODY}");
    let found = governed_allows(&unbalanced);
    assert_eq!(
        found.len(),
        1,
        "an unresolvable `allow(` was dropped rather than reported: {found:#?}"
    );
    assert_eq!(found[0].lints, ["disallowed_methods"]);
    // The level reader refuses it rather than answering a level.
    assert_eq!(
        file_level_lint_resolution(&unbalanced, LINT),
        Resolution {
            level: None,
            refused_downgrade: false,
            ambiguous: true,
        },
        "an unresolvable attribute must be a refusal"
    );
    // And a raw identifier that is not one refuses too.
    assert_eq!(
        file_level_lint_resolution(&format!("#![r#(clippy::disallowed_methods)]\n{BODY}"), LINT),
        Resolution {
            level: None,
            refused_downgrade: false,
            ambiguous: true,
        },
        "`r#` opening no identifier must be a refusal"
    );

    let _ = fs::remove_dir_all(&scratch);
}

/// `rustfmt`, the binary the `cargo fmt` gate runs.
fn rustfmt_binary() -> PathBuf {
    let sysroot = std::process::Command::new("rustc")
        .arg("--print")
        .arg("sysroot")
        .output()
        .expect("rustc runs; it built this test");
    let sysroot = PathBuf::from(String::from_utf8_lossy(&sysroot.stdout).trim().to_owned());
    let name = if cfg!(windows) {
        "rustfmt.exe"
    } else {
        "rustfmt"
    };
    let in_sysroot = sysroot.join("bin").join(name);
    if in_sysroot.is_file() {
        return in_sysroot;
    }
    PathBuf::from(name)
}

/// **Both readers separate tokens the way the lexer does.**
///
/// `PR74-GOVERNED-LINT-TOKEN-002` and `PR74-GOVERNED-LINT-TOKEN-003`, two more
/// places where this file assumed bytes where Rust has tokens. The previous
/// repair taught the readers to consume an identifier whole; it left two
/// adjacency assumptions standing.
///
/// **The introducer is three tokens, not three bytes.** `#`, an optional `!`
/// and `[` may be separated by anything the lexer calls whitespace, comments
/// included. `# ! [allow(…)]`, `#/* why */![allow(…)]` and the same across line
/// breaks are ordinary attributes -- each one measured below, each one
/// suppressing the lint -- and both readers required the bytes to be adjacent.
///
/// **Rust's whitespace is not `char::is_whitespace`.** The lexer uses
/// `Pattern_White_Space`, and the two sets disagree in BOTH directions, which
/// is why the predicate is written out rather than borrowed:
///
/// * `U+200E` and `U+200F` are Rust whitespace and are **not** `White_Space`.
///   A reader using `char::is_whitespace` refuses a separator the compiler
///   accepts -- and `#![allow\u{200E}(clippy::disallowed_methods)]` compiles
///   and suppresses.
/// * `U+00A0`, `U+1680`, `U+2000`, `U+2003`, `U+202F`, `U+205F` and `U+3000`
///   are `White_Space` and are **not** Rust whitespace. A reader using
///   `char::is_whitespace` walks over a byte rustc refuses to compile, and
///   reports a level for a file that has none.
///
/// Section (5) is why neither is merely theoretical: `rustfmt` normalises both
/// shapes away, but under a `rustfmt::skip` -- an ordinary attribute this
/// repository's gates do not forbid -- it preserves them exactly. Measured by
/// running the same `rustfmt` binary the `cargo fmt` gate runs.
#[test]
fn the_governed_lint_readers_separate_tokens_the_way_the_lexer_does() {
    use crate::effects::lint_levels::{Resolution, file_level_lint_resolution};

    const BODY: &str = "pub fn go(p: &std::path::Path) { let _ = std::fs::write(p, \"x\"); }\n";
    const LINT: &str = "clippy::disallowed_methods";

    fn predict(resolution: Resolution) -> (bool, Vec<&'static str>, bool) {
        assert!(
            !resolution.ambiguous,
            "every shape in this table is one rustc accepts, so a refusal here is not the \
             fail-closed answer -- it is the reader still unable to separate two tokens"
        );
        if resolution.refused_downgrade {
            return (false, Vec::new(), true);
        }
        match resolution.level {
            Some("allow" | "expect") => (true, Vec::new(), false),
            None | Some("warn") => (true, vec!["warning"], false),
            Some("deny" | "forbid") => (false, vec!["error"], false),
            other => panic!("the reader answered `{other:?}`, which nothing predicts"),
        }
    }

    /// Ask the compiler, and hold the reader to the answer.
    fn measured(scratch: &Path, tag: &str, prologue: &str, body: &str, lint: &str) {
        let source = format!("{prologue}{body}");
        let resolution = file_level_lint_resolution(&source, lint);
        let (built, diagnostics) = compile_prologue_probe(scratch, tag, &source);
        let fired: Vec<String> = diagnostics
            .iter()
            .filter(|(_, code)| code == lint)
            .map(|(level, _)| level.clone())
            .collect();
        let rejected = diagnostics.iter().any(|(_, code)| code == "E0453");
        let (wants_build, wants_fired, wants_rejected) = predict(resolution);
        assert_eq!(
            (built, fired.clone(), rejected),
            (
                wants_build,
                wants_fired
                    .iter()
                    .map(|level| (*level).to_owned())
                    .collect(),
                wants_rejected
            ),
            "`{tag}` — the reader answered {resolution:?} and clippy-driver did something else: \
             built={built} fired={fired:?} E0453={rejected}; all diagnostics {diagnostics:?}"
        );
    }

    let scratch = scratch_dir("lexer-separators");

    // -----------------------------------------------------------------
    // (1) The introducer, separated every way the lexer allows.
    // -----------------------------------------------------------------
    for (tag, prologue) in [
        ("introducer_plain", format!("#![allow({LINT})]\n")),
        (
            "introducer_space_after_hash",
            format!("# ![allow({LINT})]\n"),
        ),
        (
            "introducer_space_before_bracket",
            format!("#! [allow({LINT})]\n"),
        ),
        ("introducer_space_both", format!("# ! [allow({LINT})]\n")),
        (
            "introducer_comment_after_hash",
            format!("#/* why */![allow({LINT})]\n"),
        ),
        (
            "introducer_comment_before_bracket",
            format!("#!/* why */[allow({LINT})]\n"),
        ),
        ("introducer_newlines", format!("#\n!\n[allow({LINT})]\n")),
        (
            "introducer_line_comment",
            format!("#\n// why\n!\n// and why\n[allow({LINT})]\n"),
        ),
        // Denied, so the direction that fails loudly is exercised too.
        ("introducer_split_deny", format!("# ! [deny({LINT})]\n")),
        // **THE SLIP.** A denial taken back through a separated introducer.
        (
            "slip_by_split_introducer",
            format!("#![deny({LINT})]\n# ! [allow({LINT})]\n"),
        ),
    ] {
        measured(&scratch, tag, &prologue, BODY, LINT);
    }

    // -----------------------------------------------------------------
    // (2) Every separator the lexer accepts, between `allow` and its `(`.
    // `Pattern_White_Space` in full. A reader built on `char::is_whitespace`
    // refuses the last four and fails this table.
    // -----------------------------------------------------------------
    const ACCEPTED: [(&str, &str); 11] = [
        ("tab", "\u{0009}"),
        ("line_feed", "\u{000A}"),
        ("vertical_tab", "\u{000B}"),
        ("form_feed", "\u{000C}"),
        ("carriage_return", "\u{000D}"),
        ("space", "\u{0020}"),
        ("next_line", "\u{0085}"),
        ("left_to_right_mark", "\u{200E}"),
        ("right_to_left_mark", "\u{200F}"),
        ("line_separator", "\u{2028}"),
        ("paragraph_separator", "\u{2029}"),
    ];
    for (name, separator) in ACCEPTED {
        measured(
            &scratch,
            &format!("accepted_{name}"),
            &format!("#![allow{separator}({LINT})]\n"),
            BODY,
            LINT,
        );
        // And in the introducer, which is the same separator logic reused.
        measured(
            &scratch,
            &format!("accepted_introducer_{name}"),
            &format!("#{separator}!{separator}[allow({LINT})]\n"),
            BODY,
            LINT,
        );
    }
    // The same separator INSIDE the lint list, which is the third place two
    // tokens meet and the one `str::trim` gets wrong on its own. A path may be
    // spaced around its `::` for the same reason.
    for (tag, prologue) in [
        ("entry_leading_mark", format!("#![allow(\u{200E}{LINT})]\n")),
        (
            "entry_trailing_mark",
            format!("#![allow({LINT}\u{200E})]\n"),
        ),
        (
            "entry_both_marks",
            format!("#![allow(\u{200E}{LINT}\u{200E})]\n"),
        ),
        (
            "path_spaced_around_its_colons",
            "#![allow(clippy :: disallowed_methods)]\n".to_owned(),
        ),
        (
            "entry_marks_on_a_deny",
            format!("#![deny(\u{200E}{LINT}\u{200E})]\n"),
        ),
    ] {
        measured(&scratch, tag, &prologue, BODY, LINT);
    }

    // The slip again, through the separator that survives formatting.
    measured(
        &scratch,
        "slip_by_left_to_right_mark",
        &format!("#![deny({LINT})]\n#![allow\u{200E}({LINT})]\n"),
        BODY,
        LINT,
    );

    // -----------------------------------------------------------------
    // (3) Every separator the lexer REFUSES. These are `White_Space` and are
    // not Rust whitespace, so a reader that skipped them would report a level
    // for a file that does not compile. The reader must refuse instead —
    // nearby non-whitespace Unicode is a shape it cannot read, not a gap it
    // may step over.
    // -----------------------------------------------------------------
    const REFUSED: [(&str, &str); 9] = [
        ("no_break_space", "\u{00A0}"),
        ("ogham_space_mark", "\u{1680}"),
        ("en_quad", "\u{2000}"),
        ("em_space", "\u{2003}"),
        ("narrow_no_break_space", "\u{202F}"),
        ("medium_mathematical_space", "\u{205F}"),
        ("ideographic_space", "\u{3000}"),
        ("zero_width_space", "\u{200B}"),
        ("zero_width_non_joiner", "\u{200C}"),
    ];
    for (name, separator) in REFUSED {
        for (where_it_sits, source) in [
            (
                "before the delimiter",
                format!("#![allow{separator}({LINT})]\n{BODY}"),
            ),
            (
                "inside the lint list",
                format!("#![allow({separator}{LINT})]\n{BODY}"),
            ),
        ] {
            let tag = format!("{name} {where_it_sits}");
            let (built, _) = compile_prologue_probe(
                &scratch,
                &format!("refused_{name}_{}", where_it_sits.replace(' ', "_")),
                &source,
            );
            assert!(
                !built,
                "`{tag}` compiles, so it belongs in the accepted table and not this one"
            );
            assert_eq!(
                file_level_lint_resolution(&source, LINT),
                Resolution {
                    level: None,
                    refused_downgrade: false,
                    ambiguous: true,
                },
                "`{tag}` was stepped over as though it were whitespace"
            );
            for spelling in [source.clone(), source.replace('\n', "\r\n")] {
                assert_eq!(
                    crate::effects::lint_levels::file_level_lint_state(&spelling, LINT),
                    None,
                    "`{tag}` reached a census as a level"
                );
            }
        }
        let source = format!("#![allow{separator}({LINT})]\n{BODY}");
        // The compiler refuses the file outright, which is what makes skipping
        // the character wrong rather than merely generous.
        let (built, _) = compile_prologue_probe(&scratch, &format!("refused_{name}"), &source);
        assert!(
            !built,
            "`{name}` compiles, so it belongs in the accepted table and not this one"
        );
        assert_eq!(
            file_level_lint_resolution(&source, LINT),
            Resolution {
                level: None,
                refused_downgrade: false,
                ambiguous: true,
            },
            "`{name}` was stepped over as though it were whitespace"
        );
        // Never a level, under either spelling of a line ending.
        for spelling in [source.clone(), source.replace('\n', "\r\n")] {
            assert_eq!(
                crate::effects::lint_levels::file_level_lint_state(&spelling, LINT),
                None,
                "`{name}` reached a census as a level"
            );
        }
    }

    // -----------------------------------------------------------------
    // (4) Placement and offsets are preserved. The introducer parser moved,
    // and everything computed from the `#` — the line number, inner vs outer,
    // module-level vs not — must be exactly what it was.
    // -----------------------------------------------------------------
    for (tag, source, wants_line, wants_inner, wants_module_level) in [
        (
            "inner_module_level_after_a_doc_comment",
            format!("//! docs\n\n# ! [allow({LINT})]\n{BODY}"),
            3,
            true,
            true,
        ),
        (
            "inner_module_level_after_another_attribute",
            format!("#![deny(clippy::disallowed_types)]\n#!/* c */[allow({LINT})]\n{BODY}"),
            2,
            true,
            true,
        ),
        (
            "inner_after_an_item_is_not_module_level",
            format!("{BODY}# ! [allow({LINT})]\n"),
            2,
            true,
            false,
        ),
        (
            "outer_on_a_module_is_module_level",
            format!("#  [allow({LINT})]\nmod inner {{}}\n"),
            1,
            false,
            true,
        ),
        (
            "outer_on_a_function_is_not_module_level",
            format!("#  [allow({LINT})]\npub fn reaches() {{}}\n"),
            1,
            false,
            false,
        ),
        (
            "separator_before_the_bracket_keeps_the_line",
            format!("// one\n// two\n#\u{200E}!\u{200E}[allow({LINT})]\n{BODY}"),
            3,
            true,
            true,
        ),
        // **The attribute BEFORE it is separated too.** Deciding module level
        // means stepping over everything that precedes the attribute, so a
        // reader that can classify a separated introducer but cannot skip one
        // still gets this wrong -- and the wrong answer is `false`, which the
        // placement census refuses on.
        (
            "after_a_separated_attribute",
            format!("# ! [deny(clippy::disallowed_types)]\n#![allow({LINT})]\n{BODY}"),
            2,
            true,
            true,
        ),
        (
            "after_a_mark_separated_attribute",
            format!(
                "#\u{200E}!\u{200E}[deny(clippy::disallowed_types)]\n#![allow({LINT})]\n{BODY}"
            ),
            2,
            true,
            true,
        ),
        // The introducer spans lines, so the reported line is the `#`'s and not
        // the bracket's. Two different numbers, and only one of them is where a
        // reviewer will look.
        (
            "introducer_across_lines_reports_the_hash_line",
            format!("//! docs\n#\n!\n[allow({LINT})]\n{BODY}"),
            2,
            true,
            true,
        ),
    ] {
        let found = governed_allows(&source);
        assert_eq!(found.len(), 1, "`{tag}` was not found at all: {found:#?}");
        assert_eq!(found[0].line, wants_line, "`{tag}` line");
        assert_eq!(found[0].inner, wants_inner, "`{tag}` inner");
        assert_eq!(
            found[0].module_level, wants_module_level,
            "`{tag}` module_level"
        );
        assert_eq!(found[0].lints, ["disallowed_methods"], "`{tag}` lints");
    }

    // **A `#` that opens no attribute is not an attribute**, and getting that
    // wrong is not merely a spurious row. `r#type` is a raw identifier -- its
    // `#` is followed by an identifier rather than by `!` or `[` -- and an
    // opener that accepted it would run on to the next bracket group in the
    // file, which is the REAL attribute below: one finding at the wrong line,
    // reported as not module-level, and the attribute that matters skipped
    // entirely because the scan resumes past it.
    let raw_identifier_then_attribute = format!(
        "pub fn first() {{ let r#type = 1; let _ = r#type; }}\n         #[allow({LINT})]\n         mod inner {{}}\n"
    );
    let found = governed_allows(&raw_identifier_then_attribute);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(
        found[0].line, 2,
        "the finding is not the real attribute: {found:#?}"
    );
    assert!(!found[0].inner, "{found:#?}");
    assert!(
        found[0].module_level,
        "the real module-level attribute was lost to a `#` that opens nothing: {found:#?}"
    );

    // An outer separated introducer really does govern the module it precedes:
    // the denied call inside builds clean under it, so `module_level` above is
    // a claim about a live attribute rather than about a fixture.
    let (built, _) = compile_prologue_probe(
        &scratch,
        "outer_split_governs_its_module",
        &format!(
            "#![deny({LINT})]\n#  [allow({LINT})]\nmod inner {{ {} }}\n",
            BODY.trim()
        ),
    );
    assert!(
        built,
        "a separated outer introducer did not govern its module, so the placement fixture above \
         is not measuring an attribute"
    );

    // -----------------------------------------------------------------
    // (5) Formatting is not a defence. `rustfmt` normalises both shapes away —
    // and under a `rustfmt::skip`, which nothing in this repository forbids, it
    // preserves them byte for byte. Run against the same binary `cargo fmt`
    // uses, so this is what the gate actually does rather than a claim about it.
    // -----------------------------------------------------------------
    let formatted = |name: &str, text: &str| -> String {
        let file = scratch.join(format!("{name}.rs"));
        fs::write(&file, text).expect("the fixture");
        let out = std::process::Command::new(rustfmt_binary())
            .args(["--edition", "2024"])
            .arg(&file)
            .output()
            .expect("rustfmt runs; the fmt gate uses the same binary");
        assert!(out.status.success(), "rustfmt refused `{name}`");
        fs::read_to_string(&file).expect("the formatted fixture")
    };
    for (name, shape) in [
        (
            "lrm",
            format!("#![allow\u{200E}({LINT})]\npub fn go() {{}}\n"),
        ),
        ("split", format!("# ! [allow({LINT})]\npub fn go() {{}}\n")),
    ] {
        let plain = formatted(&format!("fmt_plain_{name}"), &shape);
        assert_eq!(
            plain,
            format!("#![allow({LINT})]\npub fn go() {{}}\n"),
            "`{name}` — rustfmt no longer normalises this shape, so section (5) is stale"
        );
        let skipped = format!("#![rustfmt::skip]\n{shape}");
        assert_eq!(
            formatted(&format!("fmt_skipped_{name}"), &skipped),
            skipped,
            "`{name}` — under `rustfmt::skip` the shape must survive formatting untouched; that \
             is why the readers cannot lean on the fmt gate to remove it"
        );
    }

    let _ = fs::remove_dir_all(&scratch);
}

/// **A lint name is what the compiler resolves it to, and nothing else.**
///
/// `PR74-GOV-001` and `PR74-GOV-002`. `normalize_lint` decided which lint an
/// attribute entry names by taking the last `::` segment and looking it up.
/// That is wrong in both directions at once.
///
/// **It dropped a live alias.** `clippy::disallowed_method` -- singular -- is a
/// RENAMED lint that clippy still resolves: an allow of it suppresses
/// `clippy::disallowed_methods` and a deny of it makes the build fail, with a
/// `renamed_and_removed_lints` warning beside it. The reader answered `None`,
/// so a file could allow a governed lint in that spelling with no
/// `effects/allowlist.toml` row and no census would see it. `clippy::\
/// disallowed_type` is the same for `disallowed_types`.
///
/// **It accepted names the compiler does not.** Any path ending in a governed
/// name normalised to that lint, so `rustdoc::disallowed_methods`,
/// `rustc::disallowed_methods` and `clippy::extra::disallowed_methods` -- none
/// of which govern the lint, all of which compile -- read as the governed lint.
/// The wrongly-green shape is the review's: a file with a REAL allow and a FAKE
/// deny after it
///
/// ```text
/// #![allow(clippy::disallowed_methods)]
/// #![deny(rustdoc::disallowed_methods)]
/// ```
///
/// compiles clean with the lint SUPPRESSED, and the reader called it a denial.
/// Both censuses act on that: one admits a per-site `#[expect]` against a
/// denial that is not there, and the other reports `PR6-LANEF-004` closed.
///
/// # What is accepted now
///
/// A bare governed name, or exactly `clippy::<name>` where `<name>` is a
/// governed lint or one of the two measured aliases. Two segments at most, and
/// the first must be `clippy`. Everything else refuses: extra segments and
/// foreign namespaces are not skipped, because "this attribute says nothing
/// about that lint" is false for text that plainly names it.
///
/// # Measured on both toolchains this repository supports
///
/// Every row below is compiled by `clippy-driver`. The alias rows were also
/// measured by hand against the MSRV toolchain -- clippy 0.1.85, the 1.85.0
/// pin CI runs -- and against stable clippy 0.1.97, with identical results, so
/// the canonicalisation is not a stable-only behaviour. The MSRV leg runs here
/// too when that toolchain is installed, and says so when it is not.
#[test]
fn the_governed_lint_reader_canonicalises_aliases_and_refuses_foreign_namespaces() {
    use crate::effects::lint_levels::{Resolution, file_level_lint_resolution};

    const BODY: &str = "pub fn go(p: &std::path::Path) { let _ = std::fs::write(p, \"x\"); }\n";
    const LINT: &str = "clippy::disallowed_methods";

    fn predict(resolution: Resolution) -> (bool, Vec<&'static str>, bool) {
        assert!(
            !resolution.ambiguous,
            "every spelling in this table is one the compiler resolves, so a refusal is not \
             the fail-closed answer here -- it is the reader failing to canonicalise"
        );
        if resolution.refused_downgrade {
            return (false, Vec::new(), true);
        }
        match resolution.level {
            Some("allow" | "expect") => (true, Vec::new(), false),
            None | Some("warn") => (true, vec!["warning"], false),
            Some("deny" | "forbid") => (false, vec!["error"], false),
            other => panic!("the reader answered `{other:?}`, which nothing predicts"),
        }
    }

    let scratch = scratch_dir("lint-aliases");

    // -----------------------------------------------------------------
    // (1) The alias resolves, in both directions, and so does every accepted
    // spelling of every governed lint this slice uses.
    // -----------------------------------------------------------------
    let mut table: Vec<(String, String)> = vec![
        (
            "alias_allow".to_owned(),
            "#![allow(clippy::disallowed_method)]\n".to_owned(),
        ),
        (
            "alias_deny".to_owned(),
            "#![deny(clippy::disallowed_method)]\n".to_owned(),
        ),
        (
            "alias_forbid".to_owned(),
            "#![forbid(clippy::disallowed_method)]\n".to_owned(),
        ),
        // The alias and the canonical name are ONE lint, so ordering across the
        // two spellings still decides: a deny taken back by an alias-spelled
        // allow is an allow.
        (
            "deny_then_alias_allow".to_owned(),
            format!("#![deny({LINT})]\n#![allow(clippy::disallowed_method)]\n"),
        ),
        (
            "alias_deny_then_allow".to_owned(),
            format!("#![deny(clippy::disallowed_method)]\n#![allow({LINT})]\n"),
        ),
    ];
    for spelling in ["disallowed_methods", "clippy::disallowed_methods"] {
        table.push((
            format!("accepted_{}", spelling.replace("::", "_")),
            format!("#![allow({spelling})]\n"),
        ));
    }
    for (tag, prologue) in &table {
        let source = format!("{prologue}{BODY}");
        let resolution = file_level_lint_resolution(&source, LINT);
        let (built, diagnostics) = compile_prologue_probe(&scratch, tag, &source);
        let fired: Vec<String> = diagnostics
            .iter()
            .filter(|(_, code)| code == LINT)
            .map(|(level, _)| level.clone())
            .collect();
        let rejected = diagnostics.iter().any(|(_, code)| code == "E0453");
        let (wants_build, wants_fired, wants_rejected) = predict(resolution);
        assert_eq!(
            (built, fired.clone(), rejected),
            (
                wants_build,
                wants_fired
                    .iter()
                    .map(|level| (*level).to_owned())
                    .collect(),
                wants_rejected
            ),
            "`{tag}` — the reader answered {resolution:?} and clippy-driver did something else: \
             built={built} fired={fired:?} E0453={rejected}; all diagnostics {diagnostics:?}"
        );
    }

    // The other governed lint with an alias, asked about itself.
    for (tag, prologue, lint) in [
        (
            "types_alias_allow",
            "#![allow(clippy::disallowed_type)]\n",
            "clippy::disallowed_types",
        ),
        (
            "types_alias_deny",
            "#![deny(clippy::disallowed_type)]\n",
            "clippy::disallowed_types",
        ),
    ] {
        assert_eq!(
            crate::effects::lint_levels::file_level_lint_state(prologue, lint),
            Some(if tag.ends_with("allow") {
                "allow"
            } else {
                "deny"
            }),
            "`{tag}` — `clippy::disallowed_type` is a live rename of `{lint}`"
        );
    }

    // -----------------------------------------------------------------
    // (2) `normalize_lint` is the bridge, and it answers about NAMES rather
    // than about files. Stated as values, both directions, so a reader that
    // simply matched more would fail the second half.
    // -----------------------------------------------------------------
    for accepted in [
        "disallowed_methods",
        "clippy::disallowed_methods",
        "clippy::disallowed_method",
        " clippy :: disallowed_method ",
        "clippy::r#disallowed_method",
    ] {
        assert_eq!(
            normalize_lint(accepted),
            Some("disallowed_methods"),
            "`{accepted}` is a spelling the compiler resolves to this lint"
        );
    }
    assert_eq!(
        normalize_lint("clippy::disallowed_type"),
        Some("disallowed_types")
    );
    for refused in [
        // Foreign namespaces. Each compiles and none governs the lint.
        "rustdoc::disallowed_methods",
        "rustc::disallowed_methods",
        // An extra segment under the right tool is still not the lint.
        "clippy::extra::disallowed_methods",
        "clippy::disallowed_methods::extra",
        // An unknown tool is `E0710` and does not compile at all.
        "foo::disallowed_methods",
        "foo::bar::disallowed_methods",
        // The alias only exists tool-qualified: bare `disallowed_method` is
        // `unknown_lints` and governs nothing.
        "disallowed_method",
        "disallowed_type",
        // And there is no `disallowed_macro` rename at all — measured
        // `unknown_lints`, not `renamed_and_removed_lints`.
        "clippy::disallowed_macro",
        "disallowed_macro",
    ] {
        assert_eq!(
            normalize_lint(refused),
            None,
            "`{refused}` is not a spelling the compiler resolves to a governed lint"
        );
    }

    // -----------------------------------------------------------------
    // (3) A foreign namespace REFUSES, loudly, rather than being skipped —
    // and the compiler is asked what each one really does.
    // -----------------------------------------------------------------
    for (tag, entry, compiles) in [
        ("rustdoc_namespace", "rustdoc::disallowed_methods", true),
        ("rustc_namespace", "rustc::disallowed_methods", true),
        ("extra_segment", "clippy::extra::disallowed_methods", true),
        ("unknown_tool", "foo::disallowed_methods", false),
    ] {
        // What it does on its own: it does not deny, so the lint still fires
        // (or the file does not build at all).
        let alone = format!("#![deny({entry})]\n{BODY}");
        let (built, diagnostics) =
            compile_prologue_probe(&scratch, &format!("alone_{tag}"), &alone);
        assert_eq!(
            built, compiles,
            "`{tag}` — build expectation wrong: {diagnostics:?}"
        );
        if compiles {
            assert!(
                diagnostics
                    .iter()
                    .any(|(level, code)| level == "warning" && code == LINT),
                "`{tag}` denied the lint after all, so it is not a foreign namespace: \
                 {diagnostics:?}"
            );
        }
        // And the reader refuses it rather than reading a level from it.
        for spelling in [alone.clone(), alone.replace('\n', "\r\n")] {
            assert_eq!(
                file_level_lint_resolution(&spelling, LINT),
                Resolution {
                    level: None,
                    refused_downgrade: false,
                    ambiguous: true,
                },
                "`{tag}` was read as a level instead of refused"
            );
        }
    }

    // An attribute this reader cannot resolve is a refusal, and it is no less
    // about the lint for naming it in the spelling clippy renamed. Without the
    // aliases in `mentions`, these read as "this attribute says nothing about
    // that lint" — which is the reassuring answer and the false one.
    for (tag, prologue) in [
        (
            "alias_under_an_unknown_head",
            "#![allowance(clippy::disallowed_method)]\n",
        ),
        (
            "alias_under_a_nested_unknown_head",
            "#![cfg_attr(all(), lint_group(deny(clippy::disallowed_method)))]\n",
        ),
    ] {
        let source = format!("{prologue}{BODY}");
        assert_eq!(
            file_level_lint_resolution(&source, LINT),
            Resolution {
                level: None,
                refused_downgrade: false,
                ambiguous: true,
            },
            "`{tag}` — an unreadable attribute naming the lint by its alias must refuse"
        );
    }

    // -----------------------------------------------------------------
    // (4) **The review's shape.** A real allow, then a fake deny. The compiler
    // suppresses the lint — the file ALLOWS it — and the reader must never
    // answer `deny`, which is what both censuses act on.
    // -----------------------------------------------------------------
    for (tag, fake) in [
        ("fake_deny_rustdoc", "rustdoc::disallowed_methods"),
        (
            "fake_deny_extra_segment",
            "clippy::extra::disallowed_methods",
        ),
    ] {
        let source = format!("#![allow({LINT})]\n#![deny({fake})]\n{BODY}");
        let (built, diagnostics) = compile_prologue_probe(&scratch, tag, &source);
        assert!(built, "`{tag}` did not build: {diagnostics:?}");
        assert!(
            !diagnostics.iter().any(|(_, code)| code == LINT),
            "`{tag}` — the lint fired, so the real allow did not stand and this fixture is not \
             the shape it claims: {diagnostics:?}"
        );
        let resolution = file_level_lint_resolution(&source, LINT);
        assert_ne!(
            resolution.level,
            Some("deny"),
            "`{tag}` — a file the compiler compiles CLEAN, with the lint allowed, read as a \
             denial. That is the wrongly-green answer both censuses act on."
        );
        assert!(
            resolution.ambiguous,
            "`{tag}` — a fake deny naming the lint must refuse loudly, not be skipped: \
             {resolution:?}"
        );
        assert!(!file_level_denies(&source, LINT), "`{tag}`");
    }

    // -----------------------------------------------------------------
    // (5) The placement half. An alias-spelled allow must be VISIBLE to the
    // allowlist census — otherwise it is an allowance with no row — and a
    // foreign-namespace one must not be, because it allows nothing.
    // -----------------------------------------------------------------
    let found = governed_allows(&format!("#![allow(clippy::disallowed_method)]\n{BODY}"));
    assert_eq!(
        found.len(),
        1,
        "an alias-spelled allow is invisible to the placement census, so a file may allow a \
         governed lint with no {ALLOWLIST_TOML} row: {found:#?}"
    );
    assert_eq!(found[0].lints, ["disallowed_methods"]);
    assert!(found[0].module_level && found[0].inner);
    for silent in [
        "rustdoc::disallowed_methods",
        "clippy::extra::disallowed_methods",
        "disallowed_method",
    ] {
        let found = governed_allows(&format!("#![allow({silent})]\n{BODY}"));
        assert!(
            found.iter().all(|allow| allow.lints.is_empty()),
            "`{silent}` allows nothing the compiler recognises, so it is not a governed \
             allowance: {found:#?}"
        );
    }

    // -----------------------------------------------------------------
    // (6) The MSRV leg. The alias is not a stable-only behaviour: the same
    // fixture is compiled by the 1.85.0 toolchain CI pins whenever it is
    // installed, and the absence is reported rather than passed over.
    // -----------------------------------------------------------------
    let msrv = std::process::Command::new("rustup")
        .args(["run", "1.85.0", "rustc", "--print", "sysroot"])
        .output();
    let msrv_driver = msrv.ok().filter(|out| out.status.success()).map(|out| {
        PathBuf::from(String::from_utf8_lossy(&out.stdout).trim().to_owned())
            .join("bin")
            .join(if cfg!(windows) {
                "clippy-driver.exe"
            } else {
                "clippy-driver"
            })
    });
    match msrv_driver.filter(|path| path.is_file()) {
        Some(driver) => {
            let file = scratch.join("msrv_alias.rs");
            fs::write(
                &file,
                format!("#![deny(clippy::disallowed_method)]\n{BODY}"),
            )
            .expect("the fixture");
            let out = scratch.join("msrv-out");
            fs::create_dir_all(&out).expect("an output directory");
            let output = std::process::Command::new(&driver)
                .env("CLIPPY_CONF_DIR", repo_root())
                .args([
                    "--edition",
                    "2024",
                    "--crate-type",
                    "lib",
                    "--emit=metadata",
                    "--error-format=json",
                ])
                .arg("--out-dir")
                .arg(&out)
                .arg(&file)
                .output()
                .expect("the MSRV clippy-driver runs");
            assert!(
                !output.status.success(),
                "on the MSRV toolchain a deny of `clippy::disallowed_method` did not fail the \
                 build, so the alias is stable-only and the canonicalisation must say so"
            );
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("clippy::disallowed_methods"),
                "the MSRV toolchain did not resolve the alias to the canonical lint"
            );
        }
        None => {
            // Loud, not silent: the claim above is measured on stable here, and
            // the MSRV leg is evidence this host can add when the toolchain is
            // present. CI's `test` job installs `stable` only.
            eprintln!(
                "note: the 1.85.0 toolchain is not installed, so the MSRV leg of \
                 `the_governed_lint_reader_canonicalises_aliases_and_refuses_foreign_namespaces` \
                 did not run; the alias claim is measured on the available toolchain only"
            );
        }
    }

    let _ = fs::remove_dir_all(&scratch);
}

/// **A bare-spelled level is counted, and the envelope in which the compilers
/// agree with that reading is pinned.** `PR74-GOV-003`, recorded as a residual
/// here rather than repaired, and this test is the executable record.
///
/// The counted reading is inherited: `names_lint` has answered "the bare
/// spelling is the lint" since `0519514`, before this slice, and
/// `runner::container::tests` pins the same answer over the same fixture. The
/// convergence adjudication of 2026-08-30 measured where that reading is the
/// compiled truth, on both toolchains this repository supports — stable
/// clippy 0.1.97 and the 1.85.0 pin's 0.1.85 — and the answer is an envelope,
/// not a yes:
///
/// * **At one scope, the two compilers agree with the reading everywhere.**
///   `#![deny(disallowed_methods)]` uncontested denies on both; a qualified
///   deny taken back by a bare allow in the same prologue is an allow on both.
///   Every same-scope row in the tables above therefore reads identically
///   under either toolchain, which is what lets those tables run on any host.
/// * **Beneath an ancestor, the pin disagrees.** A child module whose prologue
///   writes the bare deny under a parent that allows the lint DENIES on
///   stable and **inherits the allow on 1.85** — the bare spelling cannot
///   override an ancestor there, while `clippy::disallowed_methods`,
///   `clippy::r#disallowed_methods` and the `clippy::disallowed_method` rename
///   all override on both. The file-scoped reader cannot see an ancestor, so
///   for exactly the files the funnel census reads — nested children — its
///   `deny` over-claims on the MSRV toolchain.
///
/// # Why this is a pinned residual and not a repair
///
/// The spelling cannot enter the tree while today's gates run:
/// `renamed_and_removed_lints` fires beside every bare governed spelling, and
/// the lint gate runs stable clippy with `-D warnings`, so the divergent
/// prologue is refused at the door. Both halves of that seal are asserted
/// below, per toolchain, because the residual is non-blocking only while they
/// hold. Narrowing the reader instead — counting only the spellings that
/// override on both toolchains, refusing the rest as it already refuses a
/// foreign namespace — flips the `runner::container::tests` fixture that pins
/// the inherited answer, which is outside this slice's write set. The
/// narrowing therefore needs one lease adjudication and lands as one
/// coordinated change or not at all; this test is what keeps the two files
/// honest in the meantime, in both directions: it goes red if the reader
/// stops counting the bare spelling without that adjudication, and it goes
/// red if a toolchain change moves the envelope it documents.
#[test]
fn the_bare_lint_spelling_is_counted_and_its_toolchain_envelope_is_pinned() {
    use crate::effects::lint_levels::file_level_lint_resolution;

    const BODY: &str = "pub fn go(p: &std::path::Path) { let _ = std::fs::write(p, \"x\"); }\n";
    const LINT: &str = "clippy::disallowed_methods";
    const RENAMED: &str = "renamed_and_removed_lints";

    // -----------------------------------------------------------------
    // (1) The inherited reading, stated as values. The bare spelling is
    // counted by the level reader and seen by the placement scan — the same
    // pair of answers `runner::container::tests` builds its census on.
    // -----------------------------------------------------------------
    let child = format!("#![deny(disallowed_methods)]\n{BODY}");
    let same_scope =
        format!("#![deny(clippy::disallowed_methods)]\n#![allow(disallowed_methods)]\n{BODY}");
    for (tag, source, level) in [
        ("bare_deny", &child, "deny"),
        (
            "bare_raw_deny",
            &format!("#![deny(r#disallowed_methods)]\n{BODY}"),
            "deny",
        ),
        ("qualified_deny_then_bare_allow", &same_scope, "allow"),
    ] {
        for spelling in [source.clone(), source.replace('\n', "\r\n")] {
            let resolution = file_level_lint_resolution(&spelling, LINT);
            assert_eq!(
                (resolution.level, resolution.ambiguous),
                (Some(level), false),
                "`{tag}` — the inherited counted reading changed. If that is deliberate, it is \
                 the coordinated narrowing this test's doc describes: re-adjudicate the lease, \
                 flip the `runner::container::tests` fixture that pins the same answer, and \
                 re-measure the envelope below."
            );
        }
    }
    let found = governed_allows(&format!("#![allow(disallowed_methods)]\n{BODY}"));
    assert_eq!(
        found.len(),
        1,
        "a bare-spelled allow left the placement census: {found:#?}"
    );
    assert_eq!(found[0].lints, ["disallowed_methods"]);

    // -----------------------------------------------------------------
    // (2) The envelope, measured per toolchain rather than on whichever
    // toolchain happens to host the suite. Each leg locates its own
    // `clippy-driver` and is loud when it cannot; the ambient driver decides
    // nothing here, so the answers cannot drift with the host.
    // -----------------------------------------------------------------
    fn toolchain_driver(toolchain: &str) -> Option<PathBuf> {
        let sysroot = std::process::Command::new("rustup")
            .args(["run", toolchain, "rustc", "--print", "sysroot"])
            .output()
            .ok()
            .filter(|out| out.status.success())?;
        let driver = PathBuf::from(String::from_utf8_lossy(&sysroot.stdout).trim().to_owned())
            .join("bin")
            .join(if cfg!(windows) {
                "clippy-driver.exe"
            } else {
                "clippy-driver"
            });
        driver.is_file().then_some(driver)
    }

    /// Run `driver` over `root`. `(built, diagnostic codes by level)`.
    fn drive(driver: &Path, dir: &Path, tag: &str, root: &Path) -> (bool, Vec<(String, String)>) {
        let out = dir.join(format!("{tag}-out"));
        fs::create_dir_all(&out).expect("an output directory");
        let output = std::process::Command::new(driver)
            .env("CLIPPY_CONF_DIR", repo_root())
            .args([
                "--edition",
                "2024",
                "--crate-type",
                "lib",
                "--emit=metadata",
                "--error-format=json",
            ])
            .arg("--out-dir")
            .arg(&out)
            .arg(root)
            .output()
            .expect("clippy-driver runs; the lint gate uses the same binary");
        let mut diagnostics = Vec::new();
        for line in String::from_utf8_lossy(&output.stderr).lines() {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            let Some(code) = value
                .get("code")
                .and_then(|code| code.get("code"))
                .and_then(serde_json::Value::as_str)
            else {
                continue;
            };
            let level = value
                .get("level")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            diagnostics.push((level.to_owned(), code.to_owned()));
        }
        (output.status.success(), diagnostics)
    }

    /// Compile `child` as `mod {tag}_child;` beneath a parent whose prologue
    /// allows the lint — the shape of every file the funnel census reads.
    fn compile_pair(
        driver: &Path,
        dir: &Path,
        tag: &str,
        child: &str,
    ) -> (bool, Vec<(String, String)>) {
        let parent = dir.join(format!("{tag}.rs"));
        fs::write(
            &parent,
            format!("#![allow(clippy::disallowed_methods)]\nmod {tag}_child;\n"),
        )
        .expect("the parent fixture");
        fs::write(dir.join(format!("{tag}_child.rs")), child).expect("the child fixture");
        drive(driver, dir, tag, &parent)
    }

    /// Compile `source` as its own crate root — the shape every table above
    /// drives, which is why this leg is measured at root and nowhere else.
    fn compile_single(
        driver: &Path,
        dir: &Path,
        tag: &str,
        source: &str,
    ) -> (bool, Vec<(String, String)>) {
        let root = dir.join(format!("{tag}.rs"));
        fs::write(&root, source).expect("the fixture");
        drive(driver, dir, tag, &root)
    }

    let scratch = scratch_dir("bare-spelling-envelope");
    // (toolchain, the child's bare deny holds beneath the ancestor allow)
    for (toolchain, overrides) in [("stable", true), ("1.85.0", false)] {
        let Some(driver) = toolchain_driver(toolchain) else {
            // Loud, not silent: CI's `test` job installs `stable` only, so the
            // MSRV half of the envelope is evidence a host adds by having the
            // pin installed, exactly as the alias test's MSRV leg does.
            eprintln!(
                "note: the {toolchain} toolchain is not installed, so that half of the \
                 envelope in `the_bare_lint_spelling_is_counted_and_its_toolchain_envelope_\
                 is_pinned` did not run"
            );
            continue;
        };
        let tag = format!("nested_{}", toolchain.replace('.', "_"));
        let (built, diagnostics) = compile_pair(&driver, &scratch, &tag, &child);
        let fired = diagnostics.iter().any(|(_, code)| code == LINT);
        assert_eq!(
            (built, fired),
            (!overrides, overrides),
            "on {toolchain}, a child's bare deny beneath an ancestor allow no longer \
             behaves as measured on 2026-08-30 — the envelope this residual was accepted \
             under has moved, so it must be re-adjudicated: {diagnostics:?}"
        );
        // The seal that keeps the residual non-blocking: the spelling cannot
        // pass the stable `-D warnings` lint gate, because the rename warning
        // fires beside it — on both toolchains, contested or not.
        assert!(
            diagnostics
                .iter()
                .any(|(level, code)| level == "warning" && code == RENAMED),
            "on {toolchain}, no `{RENAMED}` fired beside the bare spelling, so the gate \
             seal this residual relies on is gone: {diagnostics:?}"
        );
        // And the same-scope half, at crate root: the flip the in-suite tables
        // rely on reads identically under both toolchains, so those tables
        // hold on any host.
        let tag = format!("same_scope_{}", toolchain.replace('.', "_"));
        let (built, diagnostics) = compile_single(&driver, &scratch, &tag, &same_scope);
        assert!(
            built && !diagnostics.iter().any(|(_, code)| code == LINT),
            "on {toolchain}, a same-scope bare allow no longer takes back the qualified \
             deny, so the same-scope rows are host-relative after all: {diagnostics:?}"
        );
    }

    let _ = fs::remove_dir_all(&scratch);
}

#[test]
fn the_production_code_region_removes_a_configured_item_and_keeps_the_rest() {
    oracles::the_configured_item_is_removed_and_the_rest_kept();
}

#[test]
fn the_production_code_region_excludes_typed_test_functions() {
    oracles::typed_test_functions_are_removed_and_later_code_is_kept();
}

#[test]
fn a_configured_attribute_in_prose_removes_nothing() {
    oracles::a_configured_attribute_in_prose_is_inert();
}

#[test]
fn the_production_code_region_contains_the_truncated_one() {
    oracles::the_whole_region_contains_the_truncated_one();
}

#[test]
fn every_production_region_that_stops_early_stops_at_a_module() {
    oracles::every_early_stop_is_at_a_module();
}

#[test]
fn every_pr6_refusal_st16_variant_and_invariant_clause_names_a_test_or_an_owner() {
    mappings::every_promised_mapping_names_a_test_or_an_owner();
}
