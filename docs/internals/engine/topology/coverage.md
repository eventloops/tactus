# `src/engine/topology/coverage.rs`

Extended notes for [`src/engine/topology/coverage.rs`](../../../../src/engine/topology/coverage.rs).
[Source on GitHub](https://github.com/sourcemaps/upstroke/blob/master/src/engine/topology/coverage.rs).
The relative link works in a checkout or on GitHub; the GitHub link also works from the published site.

The code is the authority for what it does. The explanatory prose is preserved below.
Each backticked part of a section heading is an exact source excerpt. Search for the final
excerpt within the preceding item when a heading names both an item and a line inside it.

## Module

ST-07 for the sequential run-end range: the observation export's record format, the range, the
claims — for every hook phase and every parent-side point in every injection mode of every site
in the range, the committed test that executes it — the registry document built from them
through `FaultRegistry::insert` and pinned at `effects/sequential-registry.json`, and the
authority for a recovery-proven entry's frozen N (`SWEEP-BIJECTION-005`).

The export itself is `seams.rs`'s: `HarnessTopologyHooks` writes what its harness observed
under `UPSTROKE_HOOK_OBSERVATIONS` when its last clone drops and just before a kill injection is
carried out. The merge check (`coverage/tests.rs`,
`st07_the_sequential_range_is_a_bijection_over_the_exported_observations`, ignored) rebuilds
one harness from a full run's export, runs `check_bijection` over the range at the current host,
and holds every claim to the export. The non-ignored tests hold the document to the tree.

## `pub struct ObservationRecord {`

One test's observations: what its harness saw executed (`observed`, which for a point means an
injection fired there), what it reached without injecting, and the fast sequences it recorded.
Records of one test merge by the larger count per coordinate, so a test that builds several
hooks, or a kill child spawned more than once, exports one record.

## `pub fn harness_from(records: &[ObservationRecord]) -> HookHarness {`

A harness that has seen what the records say, replayed through `HookHarness::hook`: a point is
armed and hooked so the injection fires and the execution is recorded as an execution, never
written into the harness by hand.

## `pub struct Claim {`

One coordinate of the range and the test that executes it.

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

## `pub fn range() -> Vec<EffectSiteId> {`

The sites the run's end performs at `max_parallel = 1`: the closure's appends and scrubs, and
terminal finalization's report write and cleanup steps. `Lock.Release` is not among them — the
run lock goes with the handle, outside the hooked funnels — and no site here registers a residue
class, so the document carries no recovery-proven entry; the frozen `N` such an entry would cite
is read from `effects/residue-classes.json` by [`check_frozen_n`], which the merge check runs
regardless.

## `pub const CLAIMS: &[Claim] = &[`

The evidence: for every phase and point of every site in [`range`], the committed test that
executes it, chosen from the suite's own observation export. A claim here is a statement the
merge check holds against a fresh export; the non-ignored tests hold it against the tree. The
finalization sites are claimed by ST-18's `kill_after_report_before_each_cleanup_step`, the
append's hook phases by `kill_after_run_finished_before_report`, its `Written` point by the
closure's append-error test (error-return) and `closure_kill_child` (kill); the `WrittenFull`
and `Synced` points, which closure's append reaches but no PR10 test injects at, are claimed by
the emit suite's and the settlement kill child's committed tests.

## `pub fn registry_document() -> Result<RegistryDocument, RegistryError> {`

The pinned document: a note, the range, both hosts (every point in the range is
platform-independent, so one document serves both), and the entries.

## `pub fn check_frozen_n(entries: &[RegistryEntry], declarations: &str) -> Vec<String> {`

SWEEP-BIJECTION-005: the frozen `N` a recovery-proven entry cites is the declarations file's,
not the entry's own. One line per disagreement.
