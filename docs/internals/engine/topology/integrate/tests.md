# `src/engine/topology/integrate/tests.rs`

Extended notes for [`src/engine/topology/integrate/tests.rs`](../../../../../src/engine/topology/integrate/tests.rs).

The code is the authority for what it does; this file is the whole of its prose, moved out of
the source verbatim. Each section is headed by the line of code the comment sat above, spelled
as it is in the source, so the heading is the grep string that finds the code.

## Module

Tests for the integration transaction.

## `fn integrate_through(run: &mut Run, candidate: &CandidateRef) -> Result<Terminal, UpstrokeError> {`

Integrate `candidate` as the loop would: the reservation before any effect,
the sequence, and the reservation cancelled when the sequence ended before
its first append.

## `fn fast_path_publishes_exact_candidate_without_staging_or_proposal_object() {` › `assert_eq!(run.head().as_deref(), Some(candidate.commit_sha.as_str()));`

The integration ref moved onto the very object gated at candidate_prepared.

## `fn fast_path_publishes_exact_candidate_without_staging_or_proposal_object() {` › `{`

The hook harness recorded no staging, cherry-pick or prepared-pin site
inside the fast sequence, and did record the swap.

## `fn fast_path_publishes_exact_candidate_without_staging_or_proposal_object() {` › `let prepared_at = run.order_after(`

merge_prepared before the CAS, the CAS before task_merged.

## `fn fast_path_publishes_exact_candidate_without_staging_or_proposal_object() {` › `assert_eq!(run.task_state(ALPHA), TaskState::Merged);`

Every task of the closure is Merged, the queue position is consumed, and
both entitlements are released exactly once.

## `fn an_append_failure_at_merge_prepared_issues_no_cas_and_leaves_the_integration_ref() {` › `let mut run = Run::started("append-fails-no-cas");`

The two-crash proof, live half. The compare-and-swap follows the
merge_prepared append in `integrate`, so an append that does not complete
— the stable-prefix barrier's sync failing before it returns is one way —
aborts the sequence through `?` before any CAS, so no ref moves. What a
resume then derives from the durable prefix — a lost unsynced line, or a
kept one — is `recover`'s `unsynced_merge_prepared_lost_to_power_failure`
and `events::log`'s barrier tests; here the point is only that the ref
never ran ahead of the append.

## `fn an_append_failure_at_merge_prepared_issues_no_cas_and_leaves_the_integration_ref() {` › `run.arm_point(`

A sync failure at the append is the barrier failing before it can prove
the line durable; `merge_prepared` is the first append after the
candidate is queued.

## `fn fast_dual_holding_released_once() {` › `assert!(`

PreCAS: the pair converted at the append and both holdings are the fold's.

## `fn a_symbolic_or_checked_out_integration_ref_refuses_before_any_append() {` › `let branch = refname`

Checked out in a worktree the run does not own: by the branch name, so
the worktree's HEAD is the branch rather than a detached commit.

## `fn a_symbolic_or_checked_out_integration_ref_refuses_before_any_append() {` › `git(`

Symbolic.

## `fn third_sha_refused_and_a_ref_already_at_the_proposal_only_records() {` › `let foreign = {`

Foreign history: the ref is moved to a commit the log never recorded.

## `fn third_sha_refused_and_a_ref_already_at_the_proposal_only_records() {` › `git(`

Put the ref at the proposal, as a swap that died before task_merged
would have: only the record is owed, and no second swap is issued.

## `fn stale_candidate_takes_staging_path_and_publishes_pinned_proposal() {` › `assert_eq!(`

The gate judged the proposal itself — the commit the record names and
the ref now publishes — on an exact snapshot of it, never the candidate
commit the cherry-pick was made from and never the staging worktree.

## `enum Shape {`

Every terminal shape a `merge_verification_started` transaction can reach
through the integrate path.

## `fn terminal_shape_coverage_table_drives_every_shape_and_each_converges_on_replay() {` › `for shape in EVERY_INTEGRATE_SHAPE {`

The eight-shape coverage table. Seven shapes are reachable through the
integrate path and driven here end to end; each is asserted at its
terminal and then replayed twice for equality. The remaining two live in
other harnesses because they are not integrate terminals:
Declined-after-park is `fold`'s
`declined_parked_verification_fails_task_consumes_queue_position_releases_lease_and_halts_per_policy`
(a question answer, not a verification outcome), and Interrupted is
`recover`'s `a_resume_settles_an_interrupted_stale_verification_and_reclaims_its_residue`
(a resume terminal, never a live one).

## `fn terminal_shape_coverage_table_drives_every_shape_and_each_converges_on_replay() {` › `if !matches!(shape, Shape::Fast | Shape::Conflict) {`

Every verifying shape judged exactly the commit its
merge_verification_started recorded as proposed — the proposal for
a stale-clean sequence, the head for an already-present one — and
never the candidate commit the pick was made from.

## `fn an_already_present_candidate_settles_without_an_empty_commit() {` › `let first = run.queue_candidate_editing(ALPHA, "shared.txt", "the shared change\n");`

Both candidates make the same change to the same path, so beta's
cherry-pick onto the merged head is empty.

## `fn an_already_present_candidate_settles_without_an_empty_commit() {` › `assert_eq!(`

`transaction_fault_matrix[T-PREPARED]`: "already_present is a
validation-only no-op" — the expected-old swap at the head runs, once,
so Git validates the head atomically; it just moves nothing.

## `fn a_conflicting_candidate_is_rejected_with_an_atomic_repair_before_any_repair_effect() {` › `assert_eq!(run.task_state(BETA), TaskState::AwaitingRepair);`

The rejection registered the repair atomically: beta is AwaitingRepair,
a new Pending repair task exists, and the lineage lease is held.

## `fn a_conflicting_candidate_is_rejected_with_an_atomic_repair_before_any_repair_effect() {` › `let mut expected_kinds = kinds_before;`

"merge_rejected before any repair effect": the sequence appended
exactly the rejection, dispatched nothing, and performed no task
worktree effect — no intent written, no worktree added — for the
repair it registered, whose slot does not exist.

## `fn a_conflicting_candidate_is_rejected_with_an_atomic_repair_before_any_repair_effect() {` › `assert!(`

The staging worktree of the rejected transaction is gone.

## `fn a_human_required_verdict_parks_the_task() {` › `assert!(`

The pin and staging are reclaimed at the terminal.

## `fn infrastructure_failure_defers_then_parks_at_max_defers() {` › `let terminal = integrate_through(&mut run, &second).expect("beta defers");`

First outage: deferred, inside the allowance.

## `fn infrastructure_failure_defers_then_parks_at_max_defers() {` › `run.wake_deferred();`

Wake it, then the second outage parks at max_defers.

## `fn review_runs(run: &Run) -> Vec<crate::engine::topology::scaffold::Ran> {`

Every reviewer process the recording runner ran, with the workspace it
was pointed at and that workspace's HEAD at spawn. The scaffold's review
double spawns through the runner in the workspace it is handed, so these
are observations of where the production judge sent each reviewer, not of
what the double was told.

## `fn review_snapshot_path(run: &Run, sequence: u64, pass: u32) -> std::path::PathBuf {`

Where the judge's fresh per-reviewer snapshot lives
(`SnapshotName::integration_review`). The isolation tests require each
reviewer's workspace to be exactly this — never the staging worktree,
never the gate's snapshot — because `verification_isolation` says "one for
the gate set and one fresh per reviewer", and the cover review of
`8a5f59e8` showed the reviewers' workspace could be pointed into staging
with every test green (`PR8-R4-REVIEW-ORACLE`).

## `fn gate_heads(run: &Run) -> Vec<String> {`


The commits the recording runner's gate processes looked at: the HEAD of
each gate's workspace at spawn, in spawn order.

## `fn passing_gate() -> crate::engine::topology::attempt::GatePlan {`

One gate the scaffold's recording runner answers with exit 0.

## `fn verification_snapshots_are_removed_only_after_the_terminal() {` › `for (label, review) in [`

`side_effect_vs_event_ordering`: "staging and snapshot removal (forced)
after terminal (incl. Deferred/Parked)". For each terminal a verification
can reach — merge_prepared, merge_rejected, merge_verification_unavailable
— every Snapshot.Remove the sequence performs comes after the terminal's
append, and the sequence leaves no snapshot behind.

## `fn verification_snapshots_are_removed_only_after_the_terminal() {` › `let terminal = *appends`

The first append after the mark is merge_verification_started; the
second is the verification's terminal, whichever shape it took.

## `fn code_rejection(run: &Run) -> (crate::topology::events::VerificationRecord, String) {`

A rejection's frozen repair spec and the record it was built from. What PR9
dispatches from is the spec's body; the record is what the spec was built out
of, and a repair that reaches one and not the other is not a repair.

## `fn a_rejecting_reviewers_required_change_reaches_the_frozen_repair_spec() {` › `verification.detail.contains("- restore it")`

`attempt::review_failure` renders each `required_changes` entry as its own `- `
line and leaves the verdict's reasons in the summary, so the `- ` prefix is what
makes this the reviewer's demand and not a restatement of why the pass failed.

## `fn a_failing_gates_own_output_reaches_the_frozen_repair_spec() {`

The two witnesses of `pr8-triage.md` §9, finding 2, written here rather than
against the conversion: they drive a failing gate and a rejecting reviewer
through the production loop to a durable `merge_rejected`, and read both the
`VerificationRecord` and the frozen repair spec's body. Each asserts the summary
is still there beside the evidence, so a repair that replaced one with the other
fails too, and each ends `replay_twice_equal`.

## `fn a_dispatch_takes_the_published_head_and_refuses_one_the_log_did_not_authorize() {`

The guard on [`dispatch_head`], beside the sequence's own foreign-head test and
against the same real repository. Three readings of one run: with nothing
published the head is the run's base, which is why reading `run_started`
straight was right for as long as nothing could publish; after alpha's
publication it is the published commit; and with the ref reset behind the log's
back it refuses, naming what it found, what the log authorizes and the sequence
that put it there, without moving or recreating the ref. The deleted arm is the
fourth: a published run's ref is never recreated from its base to supply a
dispatch with a head.

The defect this guards against is `PR8-R7-DISPATCH-BASE`; the regression that
witnesses the defect itself is a composition test,
`a_dependent_task_is_dispatched_into_its_dependencys_merged_work` in
`super::super::recover::tests`, because what went wrong was what a second task's
agent could read and not what a SHA said.
