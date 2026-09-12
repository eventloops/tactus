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

## `pub fn rows_reached(fold: &TopologyFold, action: &ResumeAction) -> Vec<FaultRow> {`

Which fault rows a state is the durable prefix of, by the shape the row tables: an in-flight
attempt is T-ATTEMPT's prefix, an open generation without an attempt T-DISPATCH's, a prepared
candidate T-CAND-OBJ's or T-CAND-REF's, and so on through the twenty-one.

## `pub const fn outside_the_fold(row: FaultRow) -> bool {`

T-CONTAINER and T-APPEND have no fold state to classify — a container's prefix is the runner's
and an append's prefix is the log's — and the summary says so rather than counting them as
unreached.

## `pub fn matches_row(row: FaultRow, action: &ResumeAction) -> bool {`

Whether the classifier's answer is the row's tabled resume action. T-SCRUB, T-FAILED, T-REJECT
and T-ANSWER are satisfied by any recovery (the resume has nothing to settle for them);
T-FINISH by a plan that reopens nothing at a run that is ending; the per-item rows by the
matching item in the plan.

## `pub struct CensusSummary {`

What the G5 gate dumps: every fault row with the states that reach it, every action and every
outcome counted, the bounds the census ran under and whether live and replay classified alike.

## `pub fn summarize(census: &Census, classification_equal_live_and_on_replay: bool) -> CensusSummary {`

Over the census's recorded states and transitions. Serializable, written to
`UPSTROKE_CENSUS_SUMMARY` by the census test that names it.
