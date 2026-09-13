# `src/engine/topology/coverage.rs`

Extended notes for [`src/engine/topology/coverage.rs`](../../../../src/engine/topology/coverage.rs).
[Source on GitHub](https://github.com/sourcemaps/upstroke/blob/master/src/engine/topology/coverage.rs).
The relative link works in a checkout or on GitHub; the GitHub link also works from the published site.

The code is the authority for what it does. The explanatory prose is preserved below.
Each backticked part of a section heading is an exact source excerpt. Search for the final
excerpt within the preceding item when a heading names both an item and a line inside it.

## Module

ST-07 for the sequential topology over the full claimed inventory: the claims — for every hook
phase and every parent-side point in every injection mode of every Topology- and Shared-scoped
site, the committed test the suite's observation export shows executing it — the coordinates
declared unobservable with their reasons, the residue-class evidence read from the files the
tests that produce it write, the fast-path no-execution record, the registry document built
from all of them through `FaultRegistry::insert` and pinned at `effects/sequential-registry.json`,
and the authority for a recovery-proven entry's frozen N (`SWEEP-BIJECTION-005`).

The export itself is `src/observations.rs`'s: every harness adapter writes what its harness
observed under `UPSTROKE_HOOK_OBSERVATIONS` when its last clone drops and just before a kill
injection is handed back. The merge check (`coverage/tests.rs`,
`st07_the_sequential_range_is_a_bijection_over_the_exported_observations`, ignored) rebuilds one
harness from the funnel executions of a full run's export, runs `check_bijection` over the
checked inventory at the current host, excuses only the declared coordinates, requires every
declared coordinate to be unobserved, holds the fast sequences and every claim this host requires
to the export, and holds every recovery-proven N to the declarations. The non-ignored tests hold
the document to the tree and the declarations to the code.

## `pub fn load_observations(dir: &Path) -> Result<Vec<ObservationRecord>, String> {`

Every record under the export directory, records of one test merged.

## `pub fn harness_from(records: &[ObservationRecord]) -> HookHarness {`

A harness that has seen what the records say, replayed through `HookHarness::hook`: a point is
armed and hooked so the injection fires and the execution is recorded as an execution, never
written into the harness by hand.

## `pub struct Claim {`

One coordinate of the inventory and the test that executes it.

## `pub fn hook_entry(claim: &Claim) -> RegistryEntry {`

The entry the claim builds: the site's own semantics for the phase (rows, residue detail,
resume action), the site's one observable order or none, and `Executed { test, passed: true }`.
Every field `validate_entry` checks is read from the site, so a claim cannot table a residue
the site does not leave.

## `pub fn required_phases(site: EffectSiteId, host: Host) -> Vec<EntryPhase> {`

What the bijection asks of a site on a host: both hook phases and every point in every mode the
point supports, for the points the host has.

## `pub fn frozen_sampling_n(declarations: &str, site: EffectSiteId) -> Result<Option<u32>, String> {`

The frozen N for a site, read from `effects/residue-classes.json`'s text — the declarations
half the artifacts test pins — and `None` for a site that declares no residue class.

## `pub const ADAPTER_UNIT_TEST_MODULES: &[&str] = &[`

The tests whose observations are calls on an adapter rather than executions of a funnel — the
adapter unit tests hook the harness through the adapter's own methods to test the adapter. Their
records are left out of the merge check's harness and no claim may name them; the seams bundle's
unit test, for one, arms a Windows-only point on every host, which would otherwise read as that
point executed on Unix.

## `pub fn inventory() -> Vec<EffectSiteId> {`

Every Topology- and Shared-scoped site the enums generate: 68 at this head.

## `pub struct Unobservable {`

A coordinate no test can observe executed, with the reason. Three declarations: `Report.Write`
as a whole (no funnel names it; `report.json` is written through `RunDir.WriteReport`),
`Process.Spawn`'s two hook phases (the process funnel consults `SpawnHooks::point` at its
parent-side points only, and the trait has no phase hook) and `Process.Terminate` as a whole
(`kill_tree` consults no hook). `every_declared_unobservable_coordinate_has_its_reason_in_the_code`
reads the three sources and fails the moment a reason stops being true; the merge check fails if
a declared coordinate is observed.

## `pub fn checked_inventory() -> Vec<EffectSiteId> {`

The inventory minus the sites declared unobservable as a whole. `Process.Spawn` stays in — its
points are required and observed — and its two declared phases are the only findings
[`excused`] removes.

## `pub fn excused(failure: &BijectionFailure, declared: &[Unob…`

Whether a bijection failure is about a declared-unobservable coordinate.

## `pub const FAST_PATH_TEST: &str = "engine::topology::integra…`

The test whose fast-path assertion the no-execution record cites, and
the fast sequences the suite's export records: the record has to name
every one of them (`check_bijection`), and the merge check holds this
list to the export.

## `pub const FAST_PATH_TEST: &str =`

The integrate suite's exact-base fast path, whose assertion the no-execution record cites.

## `pub const FAST_SEQUENCES: &[&str] = &["s0", "exact-base-fas…`

Every fast sequence the suite's funnel executions record: `s0` is the
integrate suite's exact-base fast path; `exact-base-fast` is the
workspace manager lane's exact-base tour
(`workspace_manager::tests::every_site_this_lane_owns_executes_both_hook_phases`).

## `pub const FAST_SEQUENCES: &[&str] = &["s0", "exact-base-fast"];`

Every fast sequence the export's funnel executions record; the no-execution record has to name
each of them, and the merge check holds this list to the export both ways.

## `pub struct ResidueEvidence {`

The two halves of the residue-class evidence, read from three tracked files: the synthetic
records from `effects/residue-synthetic.json`, written by
`workspace_manager::tests::every_registered_residue_element_is_constructed_and_recovers` from
what it constructed, classified and recovered; the sampling records from
`effects/residue-histogram.json` (PR5's four-command sampler) and
`effects/residue-histogram-sequential.json`
(`sampled_git_child_kills_of_the_remaining_residue_sites_are_classified_and_recovered` in this
module's tests, which kill-samples the five residue-classified sites PR5's sampler does not run,
each through the argv its funnel shares with it).

## `impl ResidueEvidence` › `pub fn parse(synthetic_json: &str, histograms: &[&str]) -> …`

# Errors

A file that does not parse, or names a site the enums do not.

## `pub fn residue_entries(evidence: &ResidueEvidence) -> Resul…`

One recovery-proven entry per residue class of every site of the
inventory that registers one.

# Errors

A site whose class has no synthetic or no sampling evidence in `evidence`.

## `pub fn residue_entries(evidence: &ResidueEvidence) -> Result<Vec<RegistryEntry>, String> {`

One recovery-proven entry per residue class of every site of the inventory that registers one —
nine at this head — refusing a site whose class lacks either half.

## `pub const CLAIMS: &[Claim] = &[`

The evidence: for every required phase and point of every site of the inventory on either host,
the committed test that executes it, chosen from the suite's own observation export — 162
claims: 153 coordinates both hosts require, the four Unix-only and the five Windows-only points
of `Process.Spawn`. A claim is a statement the merge check holds against a fresh export on the
host that requires it; the non-ignored tests hold every claim against the tree. The run-end
sites keep the claims the first document made; the coordinates no other suite executes under
the production adapters are claimed by this module's own witnesses (the question and answer
funnels, every event error-return and kill point, the process funnel's kill points on each host,
the container launch funnels).

## `pub fn registry(evidence: &ResidueEvidence) -> Result<Fault…`

# Errors

An entry the format refuses, or residue evidence a site lacks.

## `pub fn registry(evidence: &ResidueEvidence) -> Result<FaultRegistry, String> {`

The claims' entries, the residue entries and the no-execution records, every one through
`FaultRegistry::insert`.

## `pub fn registry_document(evidence: &ResidueEvidence) -> Res…`

# Errors

See [`registry`].

## `pub fn registry_document(evidence: &ResidueEvidence) -> Result<RegistryDocument, String> {`

The pinned document: a note, the inventory, both hosts, the declared-unobservable coordinates
with their reasons, the fast sequences and the entries.

## `pub fn registry_json(evidence: &ResidueEvidence) -> Result<…`

# Errors

See [`registry`].

## `pub fn without_histograms(mut document: RegistryDocument) -…`

The document with every recovery-proven sampling histogram zeroed: what
the pin compares, the machine-varying half set aside.

## `pub fn without_histograms(mut document: RegistryDocument) -> RegistryDocument {`

The document with every recovery-proven histogram zeroed: the machine-varying half set aside,
which is what the pin compares. The pinned copy of the counts is what the two histogram files
held when the document was regenerated; the merge check reads the files.

## `pub fn check_frozen_n(entries: &[RegistryEntry], declarations: &str) -> Vec<String> {`

SWEEP-BIJECTION-005: the frozen `N` a recovery-proven entry cites is the declarations file's,
not the entry's own. One line per disagreement.
