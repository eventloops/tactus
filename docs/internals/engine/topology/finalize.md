# `src/engine/topology/finalize.rs`

Extended notes for [`src/engine/topology/finalize.rs`](../../../../src/engine/topology/finalize.rs).
[Source on GitHub](https://github.com/sourcemaps/upstroke/blob/master/src/engine/topology/finalize.rs).
The relative link works in a checkout or on GitHub; the GitHub link also works from the published site.

The code is the authority for what it does. The explanatory prose is preserved below.
Each backticked part of a section heading is an exact source excerpt. Search for the final
excerpt within the preceding item when a heading names both an item and a line inside it.

## Module

Terminal finalization (`run_end_policy.terminal_finalization`): after a durable `run_finished`,
the report is written and the cleanup steps run in the packet's fixed order. Two callers: the
loop, once closure has appended the end (`TopologyRun::close_run`), and recovery step (b)
(`recover::finalize_if_finished`), which finalizes a Complete or Halted run found on disk and
only then refuses continuation. Both reach the same function with the barrier-proven fold, so a
finalization is idempotent across incarnations: a step that already ran finds nothing to do.

## `pub struct Finalize<'a> {`

What finalization needs and nothing more: the manager for the execution root and the refs, the
public run directory for the report, the run id for the ref namespace, and the proven fold with
its events for the projection.

## `pub enum CleanupStep {`

The six steps, in `ORDER`, each a row of the outcome equations: task worktrees and intents
(R9), snapshots (R24), staging (R10), the prepared and candidate-prepared pins (R12, R23), the
candidates refs (R11), and the execution root (R18) once empty.

## `impl CleanupStep` › `pub const fn applies_to(self, outcome: &RunOutcome) -> bool {`

Per outcome, from `resource_accounting.rows[*].at_run_end`. Snapshots, staging, pins and the
root are pruned at every outcome. Task worktrees are pruned here only at Complete and Halted:
at Parked and BudgetExceeded every generation closure closed has already been scrubbed with its
close (record §3 R10), and nothing open is left. The candidates refs are Complete's alone —
"Parked: resumably_open (never pruned)", "Halted: persistent_output (forensic; listed in
report.json)", "BudgetExceeded: resumably_open (never pruned)".

## `pub struct Finalized {`

What finalization reports back: the outcome it finalized, whether the report was written or
found current, what each step removed, whether the root went, and how many candidates refs the
report lists as retained.

## `pub fn finalize(`

Refuses without a durable `run_finished` — nothing is written and nothing deleted for a run
that has not ended. Otherwise the report is derived from the fold and written when the file on
disk is missing or stale — a file whose stored digest is not the digest of its own bytes, or
whose outcome or runner is not the derived report's (`TopologyReport::is_fresh_against`) — then
the steps run.
A fault at any site ends the command with the steps before it done and the steps after it not
started; the next resume's step (b) runs the whole order again and converges (ST-18,
`kill_after_report_before_each_cleanup_step`, which at every cell of the matrix — both phases of
every effect's site, the run lock's release included — asserts what has and has not been
removed at the fault, and
`a_kill_inside_finalization_after_the_execution_root_is_removed_converges_on_the_next_resume`,
which kills a real child inside finalization).

## `pub fn refuse_continuation(run_id: &str, finalized: &Finalized) -> UpstrokeError {`

Step (b)'s refusal, worded with what the finalization it follows did: the report regenerated or
already current, and each step's count.

## `fn scrub_slots(`

One namespace of the execution root at a time, by slot kind, through the Worktree and Snapshot
funnels' remove-then-remove-intent pairs.

## `fn delete_refs_under(`

Every ref under a prefix of the run's namespace, deleted expected-old at the value just read,
through the Ref funnel's site for that pin or ref kind.
