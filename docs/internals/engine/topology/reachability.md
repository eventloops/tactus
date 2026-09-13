# `src/engine/topology/reachability.rs`

Extended notes for [`src/engine/topology/reachability.rs`](../../../../src/engine/topology/reachability.rs).
[Source on GitHub](https://github.com/sourcemaps/upstroke/blob/master/src/engine/topology/reachability.rs).
The relative link works in a checkout or on GitHub; the GitHub link also works from the published site.

The code is the authority for what it does. The explanatory prose is preserved below.
Each backticked part of a section heading is an exact source excerpt. Search for the final
excerpt within the preceding item when a heading names both an item and a line inside it.

## Module

The resume classifier the bounded census runs over every explored state (ST-14: "the explorer
… classifies every reachable state with a resume action by running the recovery classifier over
it"). It lives in the engine because `src/topology/**` is frozen and the census tests (Class A)
call into it; nothing here mutates a fold.

## `pub enum ResumeAction {`

What a fresh process would do with the fold: nothing (not started), finalize then refuse
(Complete or Halted — "Complete and Halted classify as finalize-then-terminal"), or recover by
a plan.

## `pub struct RecoveryPlan {`

The recovery order's steps as a plan read from the fold: what reopens the run (a Parked or
BudgetExceeded end), the in-flight identities to settle interrupted (d), the retained
generations to close (e), the promoting generations to complete (f), the open generations to
recreate (g), the pending publication or verification (f), what `run_resumed` wakes, and the
halt and derivation for the summary.

## `pub fn classify(fold: &TopologyFold) -> ResumeAction {`

The classification, deterministic in the fold alone — which is what makes "the classification
computed during live emission equals the classification recomputed from the durable prefix
alone" a checkable sentence.

## `struct FoldView {`

What the fold holds for each fault row, read by the audit on its own: the same generation
classes, transaction and questions the classifier reads, walked again in `view` rather than
through the classifier's plan, so a classifier that drops an item cannot also drop the assertion
about it. The audit's mutation witness is exactly that: a `classify` that pushes every open
generation but beta's still reads every T-DISPATCH state through `rows_reached`, and
`matches_row` then finds the plan short of one generation.

## `pub fn rows_reached(fold: &TopologyFold) -> Vec<FaultRow> {`

Which fault rows a state is the durable prefix of, by the shape the row tables and from the fold
alone: an in-flight attempt is T-ATTEMPT's prefix, an open generation without an attempt
T-DISPATCH's, a prepared candidate T-CAND-OBJ's or T-CAND-REF's, and so on through the
twenty-one.

## `pub const fn outside_the_fold(row: FaultRow) -> bool {`

T-CONTAINER and T-APPEND have no fold state to classify — a container's prefix is the runner's
and an append's prefix is the log's — and the summary says so rather than counting them as
unreached.

## `pub fn matches_row(row: FaultRow, fold: &TopologyFold, action: &ResumeAction) -> bool {`

Whether the classifier's answer for this fold is the row's tabled resume action, item by item:
the per-item rows require the plan to name exactly the generations, the transaction or the
question count the fold holds for that row (sorted and compared, not merely non-empty); T-FINISH
a plan that reopens nothing at a run that is ending; T-RESUME and T-FINALIZE the outcome the fold
recorded.

T-SCRUB, T-FAILED and T-REJECT are satisfied by any recovery. The item these rows are about
needs no recovery event of its own: a scrubbed candidate is re-scrubbed idempotently, a settled
task and a registered repair are read from the prefix. Whatever another task in the same state
needs is that task's row. T-ANSWER's open question is read from the prefix too, and the plan's
count of it is held to the fold's.

T-FINISH is a closure in progress: the run is ending (a halting settlement, a budget stop, or
nothing left to select) and no `run_finished` is durable yet. The next process repeats the
closure steps for the classes still open — which is what the plan's other fields carry — then
evaluates `derived_outcome` and appends `run_finished`.

## `pub struct CensusSummary {`

What the G5 gate dumps: every fault row with the states that reach it, every action and every
outcome counted, the bounds the census ran under and whether live and replay classified alike.

## `pub fn summarize(census: &Census, classification_equal_live_and_on_replay: bool) -> CensusSummary {`

Over the census's recorded states and transitions. Serializable, written to
`UPSTROKE_CENSUS_SUMMARY` by the census test that names it.
