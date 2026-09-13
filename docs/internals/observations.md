# `src/observations.rs`

Extended notes for [`src/observations.rs`](../../src/observations.rs).
[Source on GitHub](https://github.com/sourcemaps/upstroke/blob/master/src/observations.rs).
The relative link works in a checkout or on GitHub; the GitHub link also works from the published site.

The code is the authority for what it does. The explanatory prose is preserved below.
Each backticked part of a section heading is an exact source excerpt. Search for the final
excerpt within the preceding item when a heading names both an item and a line inside it.

## Module

The ST-07 observation export: what a test's hook harness saw, written to a file named after the
test when `UPSTROKE_HOOK_OBSERVATIONS` names a directory. Every harness adapter — the Worktree,
Snapshot, Ref and Object families' `HarnessEffects`, the run-directory `HarnessHooks`, the Event
family's `HarnessEventHooks`, the container and process families' `HarnessHooks` — holds its
harness through [`Exported`], so the record is written when the last clone of an adapter on a
harness drops and again just before a `Kill` injection is handed back, since the process that
carries it out writes nothing afterwards. Outside tests the export is a no-op and the handle is
inert.

`engine::topology::coverage` reads the records back, rebuilds one harness from the ones that
are funnel executions and holds the sequential registry to it.

## `pub struct ObservationRecord {`

One test's observations: what its harness saw executed (`observed`, which for a point means an
injection fired there), what it reached without injecting, and the fast sequences it recorded.
Records of one test merge by the larger count per coordinate, so a test that builds several
adapters, or a kill child spawned more than once, exports one record. An empty record is not
written.

## `pub struct Exported {`

The harness and its export in one handle, so an adapter can derive `Default` — the derived
`default` builds a fresh harness with its own export — and every clone shares one export. The
export is written by the drop of the last clone, through the `ExportOnDrop` the handle holds
for its drop alone, and before a `Kill` is handed back, since a process that dies at a hook never
reaches its drop. Outside `cfg(test)` the export is a no-op.

## `pub struct Exported` › `_on_drop: Arc<ExportOnDrop>,`

Held for its drop: the last clone's release writes the export.

## `impl Exported` › `pub fn hook(&self, site: EffectSiteId, phase: HookPhase) ->…`

One `hook` call under the harness's lock (a poisoned lock is
entered), the answer carried through [`Self::carried`].

## `impl Exported` › `pub fn hook(&self, site: EffectSiteId, phase: HookPhase) -> Injection {`

One `hook` call under the harness's lock, the lock released before the answer is carried: the
export takes the lock itself, and `std::sync::Mutex` is not reentrant.

## `impl Exported` › `pub fn carried(&self, injection: Injection) -> Injection {`

Every adapter's answer passes through here: a `Kill` is exported before it is returned, because
the funnel aborts the process right after and a kill-mode observation that reached only the
in-memory harness would be lost with it — which is exactly the observation the merge check needs
for a kill-mode point. Call it with the harness's lock released: the export takes the lock itself.

## `mod export {`

Compiled two ways, cut at a module so the effects census's production region ends where every
other module's does. Under `cfg(test)` the export reads the variable, names the record after the
current thread — which `cargo test` names after the test — merges with the record an earlier
drop or a spawned kill child of the same test wrote, and writes through the fixture's
`write_file`, the one write a module outside the funnels may make in a test build. Outside tests
it does nothing.
## `mod export` › `pub(super) fn export(harness: &Arc<Mutex<HookHarness>>) {`

Merge what `harness` observed into the record named after the current
thread (the test) under the directory `UPSTROKE_HOOK_OBSERVATIONS`
names; nothing when the variable is unset or nothing was observed.

