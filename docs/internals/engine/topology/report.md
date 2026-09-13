# `src/engine/topology/report.rs`

Extended notes for [`src/engine/topology/report.rs`](../../../../src/engine/topology/report.rs).
[Source on GitHub](https://github.com/sourcemaps/upstroke/blob/master/src/engine/topology/report.rs).
The relative link works in a checkout or on GitHub; the GitHub link also works from the published site.

The code is the authority for what it does. The explanatory prose is preserved below.
Each backticked part of a section heading is an exact source excerpt. Search for the final
excerpt within the preceding item when a heading names both an item and a line inside it.

## Module

The schema-4 projection: `report.json` and the status text, derived from a fold and its events
and never read back as state (`DESIGN.md` §15). INV-13's registry projections — every task with
its generations, the queue, the lineages, the open questions, the transaction — the run's
runner identity in full (kind, policy, image reference, id and digest; ST-20's "report.json and
status name the run's kind, policy, image reference, id, and digest"), the retained candidates
refs (record §3 R6: listed for every outcome but Complete), and the integration ledger section.

The production reader stays at schema 3 (record §3 R9): `topology_status` is reachable from
tests and from a future schema-4 command, and takes a `StablePrefix` so that the only route from
bytes to a fold remains the stable-prefix barrier.

## `pub struct TopologyReport {`

The document. `digest` is the SHA-256 of the document without the digest, so a report on disk
can be told fresh or stale against the fold it should reflect without a clock.

## `pub struct LedgerRow {`

One row of the integration ledger: the sequence, its candidate, the basis it was prepared on
(fast, stale-clean, already-present), and the terminal it reached (`task_merged`,
`merge_rejected`, or an unavailable verification's outcome).

## `impl TopologyReport` › `pub fn derive(`

From the fold and the events, in that order of authority: the fold for every state, the events
for what the fold does not retain (the integration ledger, the runner record's spelling).

## `impl TopologyReport` › `pub fn is_fresh_against(&self, existing: &[u8]) -> bool {`

Fresh when the bytes on disk parse as a report whose stored digest is this one's and is the
digest of the stored content itself, and whose outcome and runner are this one's. A file that
does not parse, one carrying this digest over other bytes, and one recording another outcome or
runner under a matching digest are stale, never an error: finalization then rewrites it.

## `impl TopologyReport` › `pub fn render(&self) -> String {`

The status text a person reads: outcome, runner identity, tasks, queue, questions, the
retained candidates refs and the ledger.

## `pub fn outcome_label(outcome: &RunOutcome) -> String {`

The wire spelling of an outcome, shared with the refusals that quote it.

## `pub fn integration_ledger(events: &[TopologyEvent]) -> Vec<LedgerRow> {`

One row per `merge_prepared` or `merge_verification_started`, closed by the terminal that
followed it in the same sequence.

## `pub fn merged_and_parked(fold: &TopologyFold) -> (u32, u32) {`

The two counts `run_finished` carries, from the task states.

## `pub fn topology_status(`

The status of a run from its proven prefix alone: `StablePrefix` is the barrier's output, and
this is the only signature the census of fold mentions admits for a reader.
