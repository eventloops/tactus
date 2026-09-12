# `src/engine/topology/ledger.rs`

Extended notes for [`src/engine/topology/ledger.rs`](../../../../src/engine/topology/ledger.rs).
[Source on GitHub](https://github.com/sourcemaps/upstroke/blob/master/src/engine/topology/ledger.rs).
The relative link works in a checkout or on GitHub; the GitHub link also works from the published site.

The code is the authority for what it does. The explanatory prose is preserved below.
Each backticked part of a section heading is an exact source excerpt. Search for the final
excerpt within the preceding item when a heading names both an item and a line inside it.

## Module

`decisions.resource_accounting`, transcribed: the twenty-eight rows with their enforcement
domains, the five outcome equations row by row from each row's `at_run_end`, an observation of
every row from a fold plus a physical inventory plus the process-local rows, and a check that
reads each requirement against a before/after pair. ST-09 for the durable rows; the process-local
rows (R3, R4, R13, R17, R22, R28) are observed here as the loop reports them and certified at G6.

Nothing here measures the disk: [`PhysicalInventory`] is filled by the test that owns the
fixture (`recover/tests.rs`, `ledger_inventory`), and this module only says what the numbers
must be. That keeps the equations one transcription with one reader.

## `pub enum Row {`

R1 through R28, in the packet's order. [`Row::resource`] is the row's name and [`Row::domain`]
its enforcement domain; both are transcribed, and the domain test holds the counts to the
packet's twelve, twelve, one and three.

## `pub enum Class {`

The packet's five classes plus `Void`: Halted's "released (void)" rows hold whatever the fold
holds and nothing reads them, so the class is named rather than folded into `Released`.

## `pub enum Fact {`

What an observation found: a count held or zero, an artifact present or absent, a ledger
balanced or not.

## `pub enum Requirement {`

What an equation asks of a before/after pair. `Zero` and `Absent` read the after fact alone;
`Present` requires something after; `Retained` requires the after fact to equal the before one,
which is what "resumably_open" and "persistent_output" mean for a row whose count the fixture
chose; `Balanced` the process-local ledgers; `Monotone` the consumed counters (never reused,
so never smaller); `Any` the void rows.

## `pub struct ProcessLocal {`

R3, R4, R13, R17, R22 and R28 as the process reports them: the invocation ledger balanced, the
entitlements held, the run lock held, a surviving cleanup hold observed.

## `pub struct PhysicalInventory {`

The disk, Git and container facts the external rows read: slots by namespace (intents and
directories as one set), refs and pins under the run's namespace, the integration ref, the
execution root, the run directory's files, the private records, the two lock files, container
intents, the credential-volume check, and R27's objects — the ones the last pre-finalization
observation saw referenced, checked present in Git's store afterwards.

## `pub fn observe(`

One observation per row, with the evidence a disagreement quotes. R1 is the fold's own
pipeline count; R14 is the next sequence number; R20 is balanced when no effect site names a
volume and the recorded volume map is unchanged (operator-owned by classification).

## `pub fn equation(outcome: Outcome) -> Vec<Expectation> {`

The outcome equations. Complete: everything released, consumed or pruned. Parked and
BudgetExceeded: the queue, the leases, the candidates refs and the questions retained
(resumably open), the pins and worktrees pruned, the root pruned. Halted: the void rows `Any`,
the candidates refs retained (forensic). NoRunFinished: every row retained, the execution root
present — "a command ended by the append-error protocol leaves exactly this shape".

## `pub fn check(before: &Ledger, after: &Ledger, outcome: Outcome) -> Vec<Disagreement> {`

Every expectation against the pair; a missing observation is a disagreement too.

## `pub fn record(before: &Ledger, after: &Ledger, outcome: Outcome) -> LedgerRecord {`

The ledger as a document, one row per expectation with both facts and the evidence, rendered
as a Markdown table by [`LedgerRecord::render`] for the record.
