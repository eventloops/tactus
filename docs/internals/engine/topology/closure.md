# `src/engine/topology/closure.rs`

Extended notes for [`src/engine/topology/closure.rs`](../../../../src/engine/topology/closure.rs).
[Source on GitHub](https://github.com/sourcemaps/upstroke/blob/master/src/engine/topology/closure.rs).
The relative link works in a checkout or on GitHub; the GitHub link also works from the published site.

The code is the authority for what it does. The explanatory prose is preserved below.
Each backticked part of a section heading is an exact source excerpt. Search for the final
excerpt within the preceding item when a heading names both an item and a line inside it.

## Module

Run-end closure at `max_parallel = 1`: the pure half of `run_end_policy`'s closure procedure.
`TopologyRun::close_run` (`run.md`) is the acting half; everything here reads a fold and answers
a question about it, appends nothing and touches no file.

`decisions.run_end_policy.closure_procedure`, in order: (1) the ending outcome is derived —
halt, then budget, then the fold's own `derived_outcome`; (2)–(4) in-flight attempts, promoting
generations and unresolved transactions are settled, promoted and completed; (5) every open
generation is closed `RunEnding { outcome }` with its worktree scrubbed; (5b) deferred items
stay as they are (void at Halted, resumably open at BudgetExceeded); (6) the derived outcome is
confirmed against the closed fold and `run_finished` is appended. Steps (2)–(4) are the
refusals below, by the reading PR10's record states (§3 R1): the synchronous loop never leaves
an attempt in flight, a generation promoting or a transaction open when it reaches closure, and
a fresh process's recovery steps (d)–(f) settle, promote and complete them before the loop runs,
so a fold in one of those shapes at closure is a state this build cannot have produced.

## `pub fn ending_outcome(fold: &TopologyFold) -> Result<RunOutcome, UpstrokeError> {`

Step (1), read **before** any generation is closed (record §3 R3). A halting settlement
outranks a budget stop and both outrank the fold's derivation, exactly as `derived_outcome`
orders them; the difference is that the fold derives `NotEnding` while a retained or open
generation blocks `common`, and a budget-stopped run with a retained generation is precisely
the shape closure exists to end (`PR7-R4-LOOP-004`). Halted and BudgetExceeded are therefore
read from `halted_at` and `budget_stop` directly, and only Parked and Complete come from the
derivation — which for those two already requires every generation closed.

`NotEnding` here is refused with [`blockers`]'s diagnostic, naming the retained generation,
the deferred task or the admissible work rather than the derivation's arm; an unstarted run is
refused before anything is derived.

## `pub fn refuse_unclosable(fold: &TopologyFold) -> Result<(), UpstrokeError> {`

Steps (2)–(4) as refusals naming PR11. An in-flight generation, a promoting one or an
unresolved transaction at closure is not closed here and not appended for: `checkpoint_refusals`
— "an intermediate build refuses, before any append, any operation whose terminals it does not
implement" — and the terminals of closure under concurrency (in-flight cancellation, the budget
drain, promotion and publication completion inside closure) are `tokio_boundary`'s, PR11.

## `pub fn unclosable(fold: &TopologyFold) -> Vec<String> {`

The shapes [`refuse_unclosable`] names, one line each, so the refusal and the diagnostic read
the same list.

## `pub fn closable(fold: &TopologyFold) -> Vec<TaskKey> {`

Step (5)'s selection: every task holding an `OpenNoAttempt` or `RetainedIdle` generation, in
key order. The loop closes each with `close_generation` (`dispatch.rs`), appends the close, and
scrubs the slot after the append — the order `G4B-O3` fixed for the live `Close` arm.

## `pub fn confirm_derived(fold: &TopologyFold, outcome: &RunOutcome) -> Result<(), UpstrokeError> {`

Step (6)'s check: with every closable generation closed, the fold must now derive exactly the
outcome step (1) read. A disagreement is refused rather than appended, because
`check_run_finished` would refuse the append anyway and a refusal here names both outcomes.

## `pub fn run_finished(fold: &TopologyFold, outcome: RunOutcome) -> RunFinished4 {`

The event, with `merged` and `parked` derived from the fold the way the report derives them
(`report::merged_and_parked`). Neither count is validated by the fold on replay (record §3
R5): the fold checks the outcome and `halted_at`, and the counts are a projection.

## `pub fn blockers(fold: &TopologyFold) -> Vec<String> {`

Why a fold is not ending, for an operator: the unclosable shapes first, then every open or
retained generation, every deferred task, every verification-deferred candidate, and
structurally admissible work. The last line, "of a state this module cannot name", is the
`DerivedOutcome::FoldError` arm's; the totality census asserts that arm is never reached in the
fold's derivation over every explored state (`the_derived_outcome_is_total_over_every_explored_state`),
and says nothing about this function's text.
