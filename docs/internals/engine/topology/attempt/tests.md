# `src/engine/topology/attempt/tests.rs`

Extended notes for [`src/engine/topology/attempt/tests.rs`](../../../../../src/engine/topology/attempt/tests.rs).

The code is the authority for what it does; this file is the whole of its prose, moved out of
the source verbatim. Each section is headed by the line of code the comment sat above, spelled
as it is in the source, so the heading is the grep string that finds the code.

## Module

`T-ATTEMPT`: five ordering clauses and nine tabled prefixes.

## `const WORKED: &[u8] = b"the agent edited this, and the capture stages it\n";`

The bytes an "agent" leaves in the task worktree.

A file the fixture repository does not already carry, so the blob it stages
is one this test can find in the index by name and one nothing else in the
object store could be.

## `fn agent_edits(worktree: &Path) {`

The engine never edits a file; an agent does. A fake runner runs nothing, so
the test writes what the worker would have written.

## `struct Process {`

Every invocation identity of an attempt, and its ledger and slot table.

A helper rather than a repeated block, because the two ledgers are
process-lifetime state that must be *one* per run: a test that built a fresh
[`SlotAssertion`] per call would assert a single slotted invocation against
an empty table and never see the overlap the assertion exists to catch.

## `impl Process` › `fn balances(&self) -> bool {`

The two process-end conditions, together.

## `macro_rules! context {`

The context one attempt runs in, built from disjoint fields of the run.

## `fn index_blobs(worktree: &Path) -> Vec<String> {`

The blob ids the task worktree's index holds.

## `fn unreachable_ephemeral_commits(base: &Path) -> Vec<String> {`

Every unreachable commit whose message is the ephemeral snapshot input's.

The message is `WorkspaceManager::snapshot_commit_tree`'s own, so this
identifies the object by what wrote it rather than by an id the killed child
never got to report. `--no-dangling` is already applied by
[`unreachable_objects`], so what comes back is exactly R27.

## `fn attempt_started_is_durable_before_any_spawn() {`

---------------------------------------------------------------------------
O23, O25, O26, O27 — the order of one clean attempt
---------------------------------------------------------------------------

## `fn attempt_started_is_durable_before_any_spawn() {`

**O23.** `attempt_started` is durable before the worker exists.

The runner records every request it is given, so "before any spawn" is
checkable directly: at the moment the first request arrives the log on disk
must already carry the event. It is asserted two ways for the reason O21's
test gives — the harness order proves the *sequence*, and reading the bytes
back proves the append was not merely issued first but landed first.

## `fn attempt_started_is_durable_before_any_spawn()` › `let ran = run.runner.ran();`

**The oracle.** Every record above is read after both the append and the
spawn, so a `start` that spawned first and appended afterwards leaves all
of them identical — measured, this test stayed green under exactly that
reordering. `durable_at_spawn` is the log as it stood *at the instant the
process was requested*, which is the only moment the clause is about.

## `fn every_process_of_an_attempt_is_recorded_reviewers_included() {`

**Every Runner process of an attempt is in the ledger, reviewers included.**

`permits.protocol`: "the invocation ledger records registered/completed/
cancelled" and R4 is "every Runner process registered exactly once, settled
exactly once". A review pass reaches the Runner through `run_review` with
the raw handle, so the worker and the gates were recorded and the reviewers
were not — the ledger balanced the whole time, because an unregistered
process is not an unsettled one. Balance is not the assertion; the **count**
is.

## `fn a_refused_gate_ends_the_set_and_its_cause_survives() {`

**A refused gate ends the gate set, and its cause survives.**

`gates::run_all` — the legacy authority — `return`s the first `GateFailure`
rather than running the rest. Two consequences, and this asserts both.

A refused gate must not buy the gates after it: the diff is already
rejected, and every later gate is spend on a verdict that cannot change.

And the **first cause must survive**. This fixture's second gate would exit
127 — a spawn-shaped failure. A driver that ran it and let it overwrite the
real cause would hand `ladder::next_step` an infrastructure failure where a
gate rejection happened, and the ladder prices those differently: one is the
implementer's, the other is not.

## `fn a_refused_gate_ends_the_set_and_its_cause_survives()` › `let second = plan.gates[0].clone();`

Two gates: the first refuses, the second would fail to spawn.

## `fn a_refused_gate_ends_the_set_and_its_cause_survives()` › `assert_eq!(`

**The gate's diagnostic reaches the ladder.** §11.1 makes the 8-KiB tail
the feedback §11.4 sends back to the same rung; the driver built its
`GateFailure` with `log_tail: String::new()`, so a rejected attempt was
retried knowing the exit code and nothing else.

## `fn a_refused_gate_ends_the_set_and_its_cause_survives()` › `assert!(`

And the gate is named, not numbered: an operator reading `gate 0` has to
count their own config to find out which one rejected the task.

## `fn capture_precedes_the_snapshots_and_every_snapshot_commits_before_its_intent() {`

**O25, O26 and O27** in one clean attempt, read off the shared harness.

One test rather than three, because the clauses are one chain and splitting
them would let a reordering pass two of the three: a snapshot taken before
the capture, an intent written before the commit, or a reviewer running on
the gate set's snapshot are all failures of *the same sequence*.

The last position asserted is the last `Snapshot.Remove`, which is what
makes O27 checkable inside this lane at all: `candidate.rs` owns the
commit-tree, so what this module owes is that nothing here reaches it and
that every judgement is finished before anything could.

O26 is asserted **per snapshot**, from a fence that advances past each
one's add. A comparison of first observations would be a comparison of the
gate set's triple and of nothing else — the two reviewer snapshots would
execute unasserted, and a reviewer path that wrote its intent before its
commit would pass. The count check above the loop is what makes the loop
exhaustive rather than merely repeated.

## `fn capture_precedes_the_snapshots_and_every_snapshot_commits_before_its_intent() {` › `let review_inputs = run.review_inputs();`

Built before the context borrows `run` mutably.

## `fn capture_precedes_the_snapshots_and_every_snapshot_commits_before_its_intent() {` › `let assessed = context!(run, process)`

Through the production phase, over the same diff the reviewers are
shown: a fixture-built `Assessment` could show the judge a diff the
cheap rungs never saw.

## `fn capture_precedes_the_snapshots_and_every_snapshot_commits_before_its_intent() {` › `&|pass| crate::review::ReviewInvocations {`

Caller-supplied, ordinal included: nothing pass-shaped is
minted inside `judge`, so PR8's merge verification can
supply its `SequenceIdentities` here without a redesign.

## `fn capture_precedes_the_snapshots_and_every_snapshot_commits_before_its_intent() {` › `let stage = run.must_order_of(STAGE, HookPhase::Before);`

O25: both capture sites, in order, before any snapshot effect.

## `fn capture_precedes_the_snapshots_and_every_snapshot_commits_before_its_intent() {` › `let snapshots = 1 + plan.reviewers.len();`

O26, once per snapshot rather than once per test. Three snapshots are
created here — one for the gate set and one per reviewer — and comparing
*first* observations compares only the gate set's triple: a reviewer
snapshot that wrote its intent before its ephemeral commit would leave
that triple untouched and pass. Each iteration takes its fence past the
previous snapshot's add, so the three positions it compares are that
snapshot's own.

## `fn capture_precedes_the_snapshots_and_every_snapshot_commits_before_its_intent() {` › `assert!(judgement.accepted());`

O27: everything judged, and nothing here is a commit-tree.

## `fn capture_precedes_the_snapshots_and_every_snapshot_commits_before_its_intent() {` › `assert_eq!(`

The tree the capture produced is the tree the snapshots were taken of.

## `fn gates_and_reviewers_run_on_fresh_exact_snapshots_and_never_in_the_task_worktree() {`

**`decisions.workspace_candidates.snapshots`.** Gates and reviewers execute
only in exact snapshots, one per role, never reused, and never in the task
worktree.

Four claims, and the fourth is the one a weaker test misses: "worker
worktrees and the staging worktree are **never** used for verification
processes". Every workspace the runner was given is checked against the task
worktree, so a `judge` that ran a gate in place would fail here rather than
merely producing a snapshot nobody used.

"Never reused" is checked by counting distinct workspaces, not by counting
snapshot adds: three adds that all returned one path would pass a count of
adds and fail this.

## `fn gates_and_reviewers_run_on_fresh_exact_snapshots_and_never_in_the_task_worktree() {` › `let review_inputs = run.review_inputs();`

Built before the context borrows `run` mutably.

## `fn gates_and_reviewers_run_on_fresh_exact_snapshots_and_never_in_the_task_worktree() {` › `let assessed = context!(run, process)`

Through the production phase, over the same diff the reviewers are
shown: a fixture-built `Assessment` could show the judge a diff the
cheap rungs never saw.

## `fn gates_and_reviewers_run_on_fresh_exact_snapshots_and_never_in_the_task_worktree() {` › `&|pass| crate::review::ReviewInvocations {`

Caller-supplied, ordinal included: nothing pass-shaped is
minted inside `judge`, so PR8's merge verification can
supply its `SequenceIdentities` here without a redesign.

## `fn gates_and_reviewers_run_on_fresh_exact_snapshots_and_never_in_the_task_worktree() {` › `let workspaces: Vec<PathBuf> = run`

**Where the processes actually ran, not where the judgement says they
ran.** This used to read `Verdict::workspace` from both lists, and
`Judgement.reviews` is now `Vec<ReviewRecord>` — a wire type, which has
no path to carry and should not grow one. The runner's record is the
better evidence anyway: it observes the request each process was spawned
with, so a `judge` that reported one workspace and spawned in another
fails here, and the old assertion could not have seen that.

## `fn gates_and_reviewers_run_on_fresh_exact_snapshots_and_never_in_the_task_worktree() {` › `let distinct: std::collections::BTreeSet<&PathBuf> = workspaces.iter().collect();`

One shared snapshot for the gate set; one fresh per reviewer.

## `fn gates_and_reviewers_run_on_fresh_exact_snapshots_and_never_in_the_task_worktree() {` › `assert!(`

Cleaned on completion: nothing survives the judgement.

## `fn gates_take_no_slot_and_the_worker_and_reviewers_do() {`

**`permits.agent_pool_slots`.** The worker and the reviewers take a slot
pair; the gates take none.

The exclusion is the whole content of the clause — "gate invocations and the
shell probe acquire no slot" — and a scheduler that gave a gate one would
halve the parallelism of every run without failing anything else. It is
asserted from **both** sides: [`is_slotted`] over each identity, and the
[`SlotAssertion`] refusing a gate outright.

## `fn gates_take_no_slot_and_the_worker_and_reviewers_do()` › `let assessed = context!(run, process)`

Through the production phase, over the same diff the reviewers are
shown: a fixture-built `Assessment` could show the judge a diff the
cheap rungs never saw.

## `fn gates_take_no_slot_and_the_worker_and_reviewers_do()` › `&|pass| crate::review::ReviewInvocations {`

Caller-supplied, ordinal included: nothing pass-shaped is
minted inside `judge`, so PR8's merge verification can
supply its `SequenceIdentities` here without a redesign.

## `fn gates_take_no_slot_and_the_worker_and_reviewers_do()` › `let ran = run.runner.ran();`

Every process really went through the run's Runner, with the identity
this attempt assigns and the seat its role gives it.

## `fn retained_tree(worktree: &Path) -> String {`

---------------------------------------------------------------------------
O24 — the retry
---------------------------------------------------------------------------

## `fn retained_tree(worktree: &Path) -> String {`

The retained cumulative tree of `worktree`, staged.

A retained generation "holds the retained cumulative tree", and that is what
its retry verifies against — not the base. Producing it here writes objects,
which is what `git write-tree` does and why `Worktree.Verify` may not run it
(`PR5-CONF-002`).

## `fn settle_retry(`

The first two steps of O24, which are [`settle::retry`]'s: the provisional
`{pipeline}` reservation and the **one** `Worktree.Verify` against the
retained cumulative tree.

Driven through the production seam — [`ManagedWorktrees`] over the run's
real [`WorkspaceManager`] — rather than through a double, because the join
between the clause's two owners is the thing under test. `attempt.rs` has no
retry entry point of its own, so a test that reached the append without
going through here would be covering a composition no coordinator can take.

## `fn authorized_plan(run: &Run, authorized: &AttemptStarted4) -> AttemptPlan {`

The plan this module appends, built from the event `settle::retry`
authorized.

Every field the fold checks is taken from that event rather than written as
a literal beside it. A plan that disagreed with the authorization would then
be the fold's refusal at the append, which is the whole point of the two
halves naming the same attempt.

## `fn a_retry_verifies_once_then_appends_then_spawns() {`

**O24, across both of its owners.** A retry verifies **once**, then appends
exactly the event that verification authorized, then spawns.

The clause is "reservation, worktree verification, `attempt_started`
(retry), spawn" and it has two owners. [`settle::retry`] takes the
`{pipeline}` reservation and performs the single `Worktree.Verify`;
[`AttemptContext::start`] appends and spawns. There is no retry entry point
on this side, so this test drives the join, and the order is asserted as
positions in the one harness list plus the runner's own log — a verify after
the append would let a retry start against a worktree nothing had looked at,
and a spawn before the append would put a paid-for process outside the log.

**The count of verifications is an assertion, not a detail.** A second
observation on the attempt side would be a second implementation of O24's
verification, and its refusal would be a pre-append failure — which
`permits.provisional_reservations` requires to cancel the reservation. But
the cancellation lives in the *first* verify's failure branch, and that
verify passed, so the branch is not taken: the reservation would be neither
converted nor cancelled. One observation is what makes the reservation's two
outcomes exhaustive, and `count_after(VERIFY)` is where that is checked.

The quiescence is `HoldsTree`, the retained generation's form of the check,
and the tree is deliberately made to differ from the base's so a
verification against `AtBase` could not pass in its place.

## `fn a_retry_verifies_once_then_appends_then_spawns()` › `let mark = run.mark();`

Steps one and two, which are `settle::retry`'s.

## `fn a_retry_verifies_once_then_appends_then_spawns()` › `let retry = authorized_plan(&run, &authorized);`

Steps three and four, which are this module's.

## `fn a_retry_verifies_once_then_appends_then_spawns()` › `assert_eq!(`

"Reused" as a fact about the worktree rather than as a tag any call
returned: nothing was removed after the mark, and the cumulative tree the
generation retained is still the one the worktree holds.

## `fn a_retry_verifies_once_then_appends_then_spawns()` › `let durable = run.emitter.durable_events();`

The two owners named the same attempt. This module builds its
`attempt_started` from `dispatched` and the plan, and `settle::retry`
built its own from the fold; the bytes on disk are what says they agree.

## `fn git_dir(worktree: &Path) -> PathBuf {`

The git directory of a linked worktree, asked of Git rather than derived.

`<worktree>/.git` is a **file** in a linked worktree and the administrative
directory it points at is elsewhere, so deriving the path here would be a
second implementation of `workspace_manager`'s own `git_dir_of`.

## `fn a_retry_whose_retained_worktree_fails_verification_closes_and_destroys_nothing() {`

**INV-06 / O24.** A retry whose retained worktree fails `Worktree.Verify`
closes the generation, cancels its reservation, and destroys nothing.

`decisions.workspace_candidates.generation` gives the failure two recoveries
and they are not interchangeable: "failing verification an OpenNoAttempt or
repair worktree is removed with force and recreated, **and a RetainedIdle
generation is closed with `generation_closed{WorktreeMissing}`**". A retry
that took the first branch would force-remove the worktree — and a retained
worktree's whole content is a cumulative tree that **no base can be re-cut
into**, which is what INV-06's "never recreated" protects — and would then
append `attempt_started(retry)` carrying `resume_session`, so the next
worker would run against an empty tree and be gated as if it were the
retained work. The append is durable before any caller sees the outcome, so
there is no later place to catch it.

The recovery is driven end to end here rather than only observed: the
closure `settle::retry` builds is appended through the same fold-checked
emitter every other event uses, so "the generation closes" is a transition
the fold accepted and not a struct this test looked at.

Six assertions, and each is a different way the destructive branch or a
stranded reservation would show: nothing was removed, nothing was appended
before the closure, no process was asked for, the tree the worktree holds is
byte-for-byte the tree it held, the generation ends `Closed` rather than
rebuilt, and the `{pipeline}` reservation is **cancelled** —
`permits.provisional_reservations` requires "cancellation on any pre-append
failure", and a retry entry point that refused *after* this verify passed
would leave it held with nobody to settle it.

The residue planted is `index.lock`, which is exactly what the interrupted
Git command in the failure sequence leaves, and it is the cheapest way into
a failing verify that does not itself disturb the thing being protected.

## `fn a_retry_whose_retained_worktree_fails_verification_closes_and_destroys_nothing() {` › `let lock = git_dir(&dispatched.worktree).join("index.lock");`

An interrupted Git command, which is the case `Worktree.Verify` exists
for and the one the failure sequence describes.

## `fn a_retry_whose_retained_worktree_fails_verification_closes_and_destroys_nothing() {` › `assert_eq!(`

It looked, which is what stops every assertion below being about a
function that returned without doing anything.

## `fn a_retry_whose_retained_worktree_fails_verification_closes_and_destroys_nothing() {` › `run.emitter`

The recovery itself, through the fold that has to accept it.

## `fn a_retry_whose_retained_worktree_fails_verification_closes_and_destroys_nothing() {` › `assert!(`

The work itself. The lock is what `Worktree.Verify` refused for, so it
comes off before the index can be written out — removing it is this
test's own act and not part of what the retry did.

## `fn a_refused_slot_acquisition_settles_the_registration_it_took() {`

**R4 / `permits.protocol`.** A slot acquisition the assertion refuses must
not leave the invocation registered.

"The invocation ledger records registered/completed/cancelled exactly once
and **balances at process end**", and [`InvocationLedger::balances`] states
that as "no entry is `Running`". The register happens before the pair is
asked for, so a refused acquisition that propagated straight out would
abandon a `Running` entry — and at process end that entry is
*indistinguishable* from a process this coordinator genuinely lost. A leak
check that cannot tell a bookkeeping mistake from a lost process reports
both or neither.

The refusal is driven the way a real one arrives: a pair is already held.
At `max_parallel = 1` [`SlotAssertion`] refuses rather than queues, which is
its whole purpose, so this is the refusal the substrate actually produces
rather than a synthetic error injected at the seam.

The held pair is deliberately **not** registered in the ledger, so the
ledger's own balance is a statement about the worker alone: after the
refusal nothing is running, one entry is cancelled, and none is completed.

## `fn a_refused_slot_acquisition_settles_the_registration_it_took() {` › `let squatter = AttemptIdentities::new(ALPHA, GenerationId(9), AttemptNumber(9)).worker();`

Something else holds the one pair. `cancel_all_running` is not involved:
this invocation is in the slot table and not in the ledger.

## `fn a_refused_slot_acquisition_settles_the_registration_it_took() {` › `assert_eq!(`

What actually happened, so the assertions above are about the state this
test claims to have driven: O23's append is durable and no process was
ever asked for.

## `fn attempt_kill_child() {`

---------------------------------------------------------------------------
T-ATTEMPT — the kills
---------------------------------------------------------------------------

## `fn attempt_kill_child() {`

The child every `T-ATTEMPT` kill test spawns.

One child with a site switch rather than six children, because every one of
them needs the same prefix built — a run, a dispatch, an attempt, and for
most of them a capture — and six copies of that prefix would be six chances
for one of them to build a different state than its name claims.

## `fn attempt_kill_child()` › `run.arm(STAGE, HookPhase::Before, Injection::Kill);`

Sub-prefix (a): the worker ran and no capture has begun.

## `fn attempt_kill_child()` › `let tree = retained_tree(&dispatched.worktree);`

The retry's own in-flight prefix, in the generation that retained,
built through both owners of O24 exactly as the parent test's
non-kill sibling does.

## `fn attempt_kill_child()` › `let _ = context!(run, process).capture(dispatched.site());`

In flight, and now killed inside it: the arming is at the capture
because `retry` itself must succeed for the generation to be
`InFlight { attempt: 2 }` when the coordinator dies.

## `fn attempt_kill_child()` › `run.arm(WRITE_TREE, HookPhase::After, Injection::Kill);`

Sub-prefix (b): the staged blob and tree objects exist and are
referenced only by the task worktree's index.

## `fn attempt_kill_child()` › `"after_snapshot_commit" => run.arm(SNAPSHOT_COMMIT, HookPhase::After, Injection::Kill),`

Sub-prefix (c), the after phase: the id was read and nothing durable
claims the commit.

## `fn attempt_kill_child()` › `"id_unread" => run.arm_point(`

Sub-prefix (c), the `IdUnread` point: the child exited with the
object written and the coordinator never recorded the id. Armed on
the shared harness, because a point is a real injection coordinate
and `IdUnread` supports `Kill` alone.

## `fn attempt_kill_child()` › `"after_snapshot_add" => run.arm(SNAPSHOT_ADD, HookPhase::After, Injection::Kill),`

Sub-prefix (d): the intent is durable and the snapshot worktree
registered, so its HEAD holds the ephemeral commit (R24).

## `fn attempt_kill_child()` › `let assessed = context!(run, process)`

Through the production phase, over the same diff the reviewers are
shown: a fixture-built `Assessment` could show the judge a diff the
cheap rungs never saw.

## `fn attempt_kill_child()` › `&|pass| crate::review::ReviewInvocations {`

Caller-supplied, ordinal included: nothing pass-shaped is
minted inside `judge`, so PR8's merge verification can
supply its `SequenceIdentities` here without a redesign.

## `fn adopted_generation(run: &Run) -> Dispatched {`

The dispatched generation of the child's run, rebuilt in the parent.

## `fn kill_during_attempt_settles_interrupted_and_redispatches_new_generation() {`

**`T-ATTEMPT`.** A kill during an attempt settles `attempt_interrupted`, and
the task is redispatched into a **new** generation.

Every clause of the tabled resume action is asserted: the terminal is
appended with the lease disposition its kind gives, the generation goes
`Closed`, the task returns `Pending`, the residue is discarded, and the next
dispatch opens generation 1 rather than reopening generation 0. The last is
the one that matters most — "later dispatch **new generation** (spend may
repeat)" — because a recovery that reused the generation would silently
claim the dead coordinator's unknown spend as its own.

## `fn kill_during_attempt_settles_interrupted_and_redispatches_new_generation() {` › `let next = run.dispatch(ALPHA, 1);`

The redispatch: a new generation, at the same base, and the fold accepts
it — which it would not if the old generation were still open.

## `fn kill_after_capture_leaves_index_referenced_objects_then_scrub_releases_them() {`

**`T-ATTEMPT`, sub-prefix (b).** The staged objects are referenced by the
task index while the worktree stands, and the forced scrub releases them to
R27.

Both halves are the claim. "Referenced only by the task worktree index (R9)"
is checked by `git fsck --unreachable` **not** reporting the blob — the index
is one of fsck's roots, so an object it holds is reachable — and the release
is the same query answering differently after the scrub. Asserting only the
second would pass for an object that was already unreachable before the
scrub ran.

## `fn kill_after_ephemeral_snapshot_commit_before_worktree_leaves_gc_owned_object() {`

**`T-ATTEMPT`, sub-prefix (c).** An ephemeral snapshot commit written before
any intent is Git's, and there is nothing to reclaim.

The object is identified by the message `snapshot_commit_tree` writes rather
than by an id, because the point of this prefix is that the coordinator died
without recording one. What is asserted beside its presence is the
*absence* of everything that would make it the engine's: no snapshot intent,
no snapshot worktree, and after the tabled recovery the object is still
there — "an ephemeral commit without a snapshot … is left to Git (nothing to
reclaim)". An engine that pruned it would be establishing authority over the
object store.

## `fn kill_at_snapshot_commit_id_unread_point_leaves_gc_owned_object() {`

**`T-ATTEMPT`, sub-prefix (c), the `IdUnread` point.**

The same durable residue as the test above and a different way of reaching
it: the child exited with the object written and the coordinator never read
the printed id. `Object.SnapshotCommitTree` exposes the point and
`SubEffectPoint::IdUnread` supports **`Kill` only** — it has no error-return
contract, and inventing one would be inventing a resume action nothing
tables.

What proves the kill landed *at the point* rather than somewhere else is
the child's own `unreachable!`: nothing else in that path is armed, so a
point that was never consulted would let `judge` finish and the child would
fail rather than die.

## `fn kill_at_snapshot_commit_id_unread_point_leaves_gc_owned_object() {` › `let refusal = run`

The point supports one mode, and arming the other is refused rather than
silently ignored — which is what stops a suite claiming coverage of an
error contract this point does not have.

## `fn kill_after_snapshot_add_reclaims_snapshot_and_releases_its_commit() {`

**`T-ATTEMPT`, sub-prefix (d).** A snapshot whose add completed is reclaimed
by its intent, and its ephemeral commit returns to R27.

The two states are asserted on either side of the reclaim: while the
snapshot stands its HEAD references the commit (R24), so fsck does not
report it; once the snapshot is removed nothing does, so fsck does. A test
that checked only the second would pass against a snapshot that never
referenced the commit at all.

## `fn kill_during_retry_attempt_closes_generation() {`

**`T-RETRY` meeting `T-ATTEMPT`.** A kill during a retry closes the
generation it was retrying.

The distinction this holds is the one `generation` draws: "a same-session
retry re-enters InFlight in the **same** generation", and an interruption of
it closes that generation rather than retaining it — "the generation does
*not* survive an interruption". So the recovered state is generation 0
`Closed` with attempt **2** named in the terminal, and the retained session
is gone with it.

## `fn halt_cancels_in_flight_attempt() {`

**ST-17.** "at Halted the same terminal is appended by cancellation".

Two things, and only the second needs constructing. The terminal is the
same `attempt_interrupted` — an interruption is a statement about a
coordinator, not a judgement of the work — with a detail that says the run
halted.

The in-flight *invocation* is built directly, because a synchronous
substrate cannot leave one any other way: `Runner::run` returns before the
coordinator can observe a halt, so a registration that never settled is
exactly the state a halt arriving **during** a run leaves, and the honest
way to test the cancellation is to put the ledgers in it. Both ledgers are
then required to balance, which is the process-end condition
`permits.protocol` states.

## `fn halt_cancels_in_flight_attempt()` › `let reviewer = started.identities.review_pass(0, 0);`

A reviewer whose completion never ran, holding the pair its role takes.

## `fn stage_elements() -> Vec<ResidueElement> {`

---------------------------------------------------------------------------
The `Internal` residue class of the two capture commands

`command_internal_sub_effects` gives this class two kinds of evidence and
both are here: **(i)** synthetic construction of every registered element,
each classifying `Internal` and recovering by the tabled action, and **(ii)**
a real-command kill-sampling record with the observed-class histogram. It is
deliberately *recovery-proven rather than execution-observed*: a killed
`git add` is not a hook point, so nothing can stand inside it and say "this
is what it left". A never-hit `Internal` does not fail; an **unclassifiable**
residue does.
---------------------------------------------------------------------------

## `fn stage_elements() -> Vec<ResidueElement> {`

The three elements `Object.CandidateStage` registers, planted one at a time.

Read off the frozen enum rather than written out, so an element added to the
site fails this until it is constructed — `bounded_grid`, the failure this
project has recorded three times, is a grid over the elements its author
remembered.

## `fn unstaged_work(worktree: &Path) {`

The half of an interrupted `git add` that is not an element: work in the
tree that the command had not finished staging.

`command_internal_sub_effects` defines the class as the elements "**with the
after-phase reference absent**", and the order in `classify_object_residue`
is that sentence's — the after-phase reference decides `After` first, and
only its absence lets residue decide `Internal`. For
`Object.CandidateStage` the after-phase reference is "an index that reflects
the working tree", so a worktree whose index is clean classifies `After`
however much R27 residue is lying around, and correctly: a `git add` that
finished is not one that was killed. Measured — a temporary object file
planted in a pristine worktree classifies `After`.

So every synthetic element is planted into a worktree that also carries
unstaged work, which is what a `git add` killed part-way through leaves.

## `fn plant_stage_residue(base: &Path, worktree: &Path, element: ResidueElement) {`

Plant one element of `Object.CandidateStage`'s residue in `worktree`.

The two object-store elements are R27 — Git's — and live in the **shared**
object directory, which is why they survive the scrub below while the
index lock does not. That difference is the point of planting them
separately rather than as one blob of "residue".

## `fn plant_stage_residue(base: &Path, worktree: &Path, element: ResidueElement) {` › `write_file(`

An orphan blob: written into the store and referenced by nothing.
Untracked on purpose — the index must not hold it, or it would be
reachable and the classifier would be right to ignore it.

## `fn synthetic_git_add_residue_unreferenced_objects_and_index_lock_then_forced_scrub_converges() {`

**`T-ATTEMPT`, sub-prefix (b'), evidence (i).** Every residue element a
killed `git add` can leave, constructed, classified `Internal`, and
recovered by the tabled forced scrub.

**A repository per element.** Two of the three live in the *shared* object
store and are permanent until Git prunes them, so planting them in sequence
in one repository would leave the second element's slot carrying the first's
and a classifier that recognised only `UnreferencedObject` would answer
`Internal` for all three. Measured: it did.

Convergence is asserted for what the scrub owns and **not** for what it does
not. The lock leaves with the worktree's git dir; the orphan blob and the
temporary object file are R27 and stay, because "objects left unreferenced
by any of these prunings … are Git's" and an engine that pruned them would
be establishing authority over the object store, which `cleanup` forbids.

## `fn synthetic_git_add_residue_unreferenced_objects_and_index_lock_then_forced_scrub_converges() {` › `let target = ResidueTarget::new(&fixture.base).at(&worktree);`

Two controls, and the pair is what makes the `Internal` below mean
something. A classifier that answered `Internal` unconditionally
fails the first; one that ignored its element list and read only the
after-phase reference fails the second.

## `fn synthetic_git_add_residue_unreferenced_objects_and_index_lock_then_forced_scrub_converges() {` › `manager`

The tabled recovery: forced removal of the worktree, then its intent.

## `fn synthetic_git_add_residue_unreferenced_objects_and_index_lock_then_forced_scrub_converges() {` › `manager`

Idempotent, which `cleanup` requires of every reclaim.

## `fn synthetic_git_add_residue_unreferenced_objects_and_index_lock_then_forced_scrub_converges() {` › `let fixture = Fixture::created("synthetic-stage-all");`

And all three at once, which is the state a killed `git add` leaves.

## `const SAMPLED: [EffectSiteId; 2] = [STAGE, WRITE_TREE];`

---------------------------------------------------------------------------
Evidence (ii): the real-command kill-sampling record
---------------------------------------------------------------------------

## `const SAMPLED: [EffectSiteId; 2] = [STAGE, WRITE_TREE];`

The two commands `T-ATTEMPT`'s sub-prefix (b') names: "git add or write-tree
killed after writing objects and before publishing the index or cache-tree".

## `const SAMPLING_N: u32 = 8;`

The frozen sample count, per command.

`command_internal_sub_effects`: "the Git child of the site is killed at
uncontrolled points through the process funnel across N runs (N frozen per
site in the registry)". The claim each sample carries is *per sample* —
every observed residue classifies into exactly one class and recovers by the
classified action — and is not a coverage claim about the classes, which is
why N does not have to be large enough to hit `Internal`.

## `const HISTOGRAM: &str = "effects/attempt-residue-histogram.json";`

The observed-class histogram, which is a property of the machine and cannot
be pinned.

`effect_site_inventory.outputs` asks for "sampling N **and observed-class
histogram**" per site. Which class a sample lands in is a race between the
kill and Git, so it goes to a machine-varying evidence file rather than into
a byte-compared artifact — the same split, and for the same reason, as
`effects/residue-histogram.json`. This file is that one's `T-ATTEMPT`
sibling and is written to a **different path** so the two samplers cannot
overwrite each other's record.

## `struct Sample {`

One sample: what it ran, which rung its kill was aimed at, when the kill
actually fired, how the child ended, and what the classifier answered.

## `struct Sample` › `ran: Option<std::time::Duration>,`

How long the child had run when the poll found it finished, from the
spawn's return, when it finished before the kill — measured as `fired` is
(`let spawned = child.spawned();` below).

`None` when the kill got there first, which is the case this harness
wants. When every sample is `Some`, the schedule raced a number that
does not describe these runs, and these are the durations to rebuild it
from.

## `fn bulk(worktree: &Path) {`

Enough work in the worktree that the sampled command has a middle to be
killed in.

## `fn sampled_argv(site: EffectSiteId) -> Vec<String> {`

The exact argv the site's funnel runs.

Read from the funnel's own frozen lists, never transcribed: a funnel that
grew a flag beside a transcribed copy would leave this sampler killing a
stale command with every assertion here still green.

## `fn populate_for(site: EffectSiteId, worktree: &Path) {`

Populate a worktree for `site`, leaving it in the state the funnel would
find it in.

## `fn populate_for(site: EffectSiteId, worktree: &Path)` › `git(worktree, &["add", "-A"]);`

`write-tree` reads an index, so the bulk has to be in one.

## `fn sample_slot(generation: u32) -> crate::workspace_manager::Slot {`

A slot of the sampling fixture.

## `fn measure_budget(site: EffectSiteId, fixture: &Fixture) -> std::time::Duration {`

How long the same command takes when nothing kills it.

Measured in a **probe slot of its own**, which is then removed. Measuring it
in the worktree the next sample will kill in makes the probe *perform* the
command first, and the samples then classify a fixture artefact rather than
a kill — the "environment assumption in a test" class this project has
recorded.
A duration the sampled command plausibly takes, measured **warm**.

**The first invocation is the one that lies.** A cold worktree pays for a
filesystem cache miss and, on Windows CI, for an antivirus scan of files it
has just seen created — so a budget taken from run one is inflated relative
to every run that follows, and a schedule derived from it puts every kill
after its child has already exited. Measured: two consecutive
`test (windows-latest)` legs at `b07b8cc` in which **zero of sixteen**
sampled kills landed, on a commit that changed one line of a Markdown file.

So one run is discarded as warm-up and the median of the next three is
taken. The median rather than the mean because the failure mode is a single
outlier, and a mean carries an outlier's weight into the schedule that a
median discards.

## `fn measure_budget(site: EffectSiteId, fixture: &Fixture) -> std::time::Duration {` › `const PROBE_SLOTS: [u32; 4] = [9_996, 9_997, 9_998, 9_999];`

Slots the probes use, distinct from every sampled run's.

## `fn measure_budget(site: EffectSiteId, fixture: &Fixture) -> std::time::Duration {` › `median(&measured[1..])`

Discard the warm-up, then the median of what is left.

## `fn median(durations: &[std::time::Duration]) -> std::time::Duration {`

The median of a non-empty slice of durations.

## `fn sample(site: EffectSiteId) -> Vec<Sample> {`

Sample one command `SAMPLING_N` times and classify what each kill left.

**Self-healing against an unrepresentative probe, and only against that.**
The schedule races a measured duration, so a budget that does not describe
the runs it schedules puts every kill after its child has exited and the
sampling observes nothing. When that happens the runs themselves are the
better measurement — an unkilled run ran to completion, so its duration is
the true one — and the schedule is rebuilt from their median and retried
**once**.

Bounded at one retry on purpose. A second miss is not an unlucky probe; it
is the kill failing to land at all, which is a defect this harness exists to
report. The caller's vacuity refusal is what reports it, and nothing here
weakens that assertion — this only removes the case where it fires for an
environment rather than for a bug.

## `fn sample(site: EffectSiteId) -> Vec<Sample>` › `let observed: Vec<std::time::Duration> = first.iter().filter_map(|sample| sample.ran).collect();`

Premise failed: no kill landed mid-run. Every run therefore completed, so
every `ran` is a full duration and their median is the budget the probe
should have produced.

## `fn sample(site: EffectSiteId) -> Vec<Sample>` › `return first;`

Nothing landed and nothing finished either: the schedule is not the
explanation, so there is nothing honest to recalibrate from. Hand
back the first pass and let the caller's vacuity refusal report it.

## `fn sample_once(`

One pass of `SAMPLING_N` runs against `budget`, taking slots from
`slot_base` so a retry never reuses the first pass's worktrees.

## `let deadline = std::time::Instant::now() + after;`

Sleep the schedule, but notice if the child finishes first — and
record its OWN duration when it does. Wall time to the reap would
include this sleep, so an over-long schedule would report itself
back as the number it should have been, and the recalibration below
would inherit exactly the error it exists to correct. Measured: it
did, on the first version of this fix.
Poll to the deadline WITHOUT shortening it. Noticing that the child
finished is a measurement; acting on it is not. Breaking out early
and killing there fires the kill sooner than the rung it was aimed
at, which the shape assertions below refuse — measured on the
Windows guest, where a kill fired at 40.3ms against a 48.5ms rung.

## `let spawned = child.spawned();`

The rung is a delay after the spawn's return, where `deadline` was set.
The fixture's clocks — `fired`, and the one `exited` read into `ran` — run
from an origin read before the spawn (#259 moved it there on 2026-09-10,
for the two samplers that aim from it), and `KillableGitChild::spawned` is
the spawn's own latency on that clock. Both readings have it subtracted, so
the kill is compared with its rung on the clock the rung was set on, and
the retry's schedule is rebuilt from what the children ran after the spawn
returned — the reference this sampler had before the origin moved. Read
from the origin, a slow spawn counted toward the rung: the ultra review of
`ec87d6ed` (finding 1) paused the spawn one second after the origin and
removed the deadline loop, and every kill, fired the instant the spawn
returned, read 1.0002 s against rungs of 0.66–7.5 ms; the sampler passed,
twice, on sixteen kills that followed no rung. With the readings from the
spawn's return the same mutant fails on the first sample — a kill fired
3.5 µs after its child was spawned, sooner than the 951 µs rung it was
aimed at — and the sampler passes unmutated 25 of 25 (#259's body, W5).

## `fixture`

The tabled recovery for every class this prefix can leave: forced
removal of the worktree, then its intent. Idempotent for `None` and
`After`, which is why one action covers all three.

## `fn sampled_git_add_and_write_tree_child_kills_every_residue_classified_and_recovered() {`

**`T-ATTEMPT`, sub-prefix (b'), evidence (ii).** Real `git add` and
`write-tree` children, killed at N uncontrolled points each; every observed
residue classifies into exactly one class and recovers by that class's
action.

### What is asserted and what is only recorded

The class counts are **not** asserted: which class a sample lands in is a
race between the kill and Git, so a suite that required `Internal` would be
red whenever the machine was fast. What is asserted is that every sample
classified into one of the three and recovered, and that `unclassified` is
zero — an unclassifiable residue is durable state no tabled action recovers,
and that is the failure this evidence exists to exclude.

### The oracles that a green completion does not also satisfy

A sampler whose kills all missed would still spawn `2 × N` children, still
classify a legal residue from each, still recover, and still write its
evidence file — of *completion* residue, filed under the kill's name. Three
things separate the two, and all three are here:

* **the ladder**, asserted per command: N kills aimed at N distinct,
  increasing points, because the clause says "killed at **uncontrolled
  points**" and one fixed delay is one point sampled N times;
* **every child fired at**, asserted per command and exactly, because
  `fired` is written inside `KillableGitChild::kill` and a kill that was
  skipped leaves no record to count;
* **at least one kill landing**, asserted over the sampling as a whole,
  because only a wait status distinguishes a killed child from a finished
  one.

The floor is over the whole sampling and not per command deliberately.
`git add` measures roughly **1 in 8** on this project's machines — the
budget probe writes the very blobs the samples then find already in the
object store, so a sample runs in about a fifth of the time its ladder was
scaled to — and a per-command floor would stand on a margin of one sample
and be red on the next machine. The per-command counts are recorded in the
evidence file so the margin stays visible without being load-bearing.

## `fn sampled_git_add_and_write_tree_child_kills_every_residue_classified_and_recovered() {` › `let counted = |wanted: ObjectResidue| -> u32 {`

The classifier's answers, tallied here rather than by the code under
test: a histogram that counted a class under the wrong name agrees
with itself, and only a second expression over the same list can see
it.

## `fn sampled_git_add_and_write_tree_child_kills_every_residue_classified_and_recovered() {` › `let failed: Vec<Option<i32>> = samples.iter().filter_map(|s| s.failed.map(Some)).collect();`

The premise of every count below: a child that failed on its own left
the fixture's residue rather than the kill's.

## `fn sampled_git_add_and_write_tree_child_kills_every_residue_classified_and_recovered() {` › `let shape: Vec<&Sample> = samples`

The ladder, per command shape rather than per site label: the
contract names two *commands*, and two sites that sampled one shape
would leave two records intact.

## `fn sampled_git_add_and_write_tree_child_kills_every_residue_classified_and_recovered() {` › `for sample in &shape {`

A kill fired at every child, and no earlier than the rung it was
aimed at. `fired` is the clock read inside the kill, so deleting the
wait moves it and deleting the kill removes it.

## `fn sampled_git_add_and_write_tree_child_kills_every_residue_classified_and_recovered() {` › `let landed: usize = per_site`

The kill itself, over the sampling as a whole. Nothing else in this
harness changes when `KillableGitChild::kill` stops killing.

## `fn sampled_git_add_and_write_tree_child_kills_every_residue_classified_and_recovered() {` › `let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(HISTOGRAM);`

The evidence file `outputs` asks for, written and read back.

## `fn a_failing_gate_rejects_the_judgement_and_its_snapshot_is_still_cleaned() {`

A judgement is not a constant: a gate that fails rejects the attempt, and
its snapshot is still cleaned.

Without this, [`Judgement::accepted`] could `return true` and every other
test here would stay green — all of them drive a runner whose processes
succeed. The cleanup half matters as much: `snapshots` says they are
"cleaned on completion", and a completion is not the same thing as a pass.

## `fn a_failing_gate_rejects_the_judgement_and_its_snapshot_is_still_cleaned() {` › `let review_inputs = run.review_inputs();`

Built before the context borrows `run` mutably.

## `fn a_failing_gate_rejects_the_judgement_and_its_snapshot_is_still_cleaned() {` › `let assessed = context!(run, process)`

Through the production phase, over the same diff the reviewers are
shown: a fixture-built `Assessment` could show the judge a diff the
cheap rungs never saw.

## `fn a_failing_gate_rejects_the_judgement_and_its_snapshot_is_still_cleaned() {` › `&|pass| crate::review::ReviewInvocations {`

Caller-supplied, ordinal included: nothing pass-shaped is
minted inside `judge`, so PR8's merge verification can
supply its `SequenceIdentities` here without a redesign.

## `fn a_failing_gate_rejects_the_judgement_and_its_snapshot_is_still_cleaned() {` › `assert!(`

**The reviewers do not run, and that is a deliberate change.** This test
used to assert that they ran after the failing gate and passed, because
the old `judge` ran every gate and then every reviewer unconditionally.
The legacy engine does not: §11.2 is "a strong reviewer judges the diff
against the acceptance criteria **only once the cheap checks pass**", and
`run_attempt` guards its review block on `failure.is_none()`. Buying a
frontier invocation to judge a diff the gates have already refused is
spend for information the run cannot act on.

## `fn a_malformed_captured_id_is_a_git_error_naming_where_the_value_came_from() {`

A capture id that is not an object id is a Git error, not a refusal
(PR #130, pass 3).

`git write-tree`'s output and the recorded base are the engine's own
values, so a malformed one is the tool or the engine misbehaving. Reaching
`ObjectId::new` with a `?` made it `UpstrokeError::Refused`, which says a
caller offered something it should not have. Witnessed by restoring that
`?`: the error becomes `Refused` and the first assertion fails.

## `fn declare(worktree: &Path, manifest: &str) {`

The worker's whole conflict-resolution vocabulary, a file write: the
manifest goes where the engine designates it, with the tools an edit profile
has. Nothing here shells out to git, because the real worker cannot — the
helper this replaced (`git(&worktree, &["add", "--", "c.txt"])`, PR #249's
first repair round) staged through a privilege production lacks, and the
regression review's witness passed before that round and failed after it.

## `fn an_unresolved_conflict_fails_the_capture_before_any_gate_and_a_declared_one_is_staged_by_the_engine() {`

`R9`: a path the index still holds unmerged is read before staging, so
nothing is staged and the tree is the base's; the assessment fails
`AgentError` naming the path, with feedback that names the manifest, its two
words and "run no git command", and no gate or reviewer runs. Rewriting the
file without its markers changes nothing until the worker declares it; once
it has, the engine stages the resolution in the one `Object.CandidateStage`
execution, the index holds no unmerged entry, the tree carries the
resolution and not the manifest, and the manifest — acted on — is consumed.

## `fn a_declared_deletion_resolves_a_conflict_and_an_undeclared_missing_file_does_not() {`

A conflicted path whose file is gone is still an unmerged entry; `deleted
<path>` in the manifest has the engine `git rm` it, and the captured tree
then records the deletion.

## `fn a_conflict_rendered_with_a_longer_marker_size_is_unresolved_at_capture() {`

PR #249's conformance review, finding 2: the unmerged entry is what makes a
path unresolved, not the seven-character marker a scan would look for. A
`conflict-marker-size=8` conflict the worker left alone is refused before
staging while its other edit stands, and a declared resolution captures.

## `fn an_untouched_binary_conflict_is_unresolved_at_capture_and_a_declared_keep_ours_is_staged() {`

The same finding's other format: a `-merge` path Git leaves as the current
side with no marker at all. Untouched it is refused; declared resolved with
not one byte changed — keep-ours, byte-identical to abandonment — it
captures with the published side, which is why the declaration and not the
content carries the intent.

## `fn four_shapes(run: &mut Run) -> (String, String) {`

Default markers, `conflict-marker-size=8`, a `-merge` binary and a
delete/modify in one worktree: the base changes four files the ancestor had
and one candidate commit (`commit_changing`) changes three differently and
deletes the fourth. A cherry-pick applies one commit's change against its
parent, so a chain of one-file source commits would conflict in its tip
alone — the first draft of this fixture did, and only `d.txt` was unmerged.

## `fn four_conflict_shapes_declared_at_once_are_staged_by_the_engine_into_one_tree() {`

Two files rewritten, the binary kept, the deletion chosen for the fourth,
all four declared: one `Object.CandidateStage` execution stages three
`git add`s and one `git rm`, the index holds no unmerged entry, the engine
removed the file the worker declared deleted (its tools could not), and the
tree carries the three kept paths, not the fourth and not the manifest.

## `fn four_conflict_shapes_undeclared_are_all_refused_before_any_gate_and_nothing_is_staged() {`

The refusal side of the same conflict. Nothing declared: all four refused,
named in the `AgentError`, no gate and no reviewer. Three declared and one
forgotten: only the forgotten path is refused, and the three are not staged
either — a capture is all or nothing, and the declared deletion removes
nothing until a capture proceeds. A manifest with a line the grammar does
not admit: every path refused and the line quoted back as a fifth entry.

## `fn an_edit_only_worker_completes_a_conflict_repair_through_file_writes_alone() {`

PR #249's regression review, the P1, in the reviewer's own shape. The
production Claude Code worker command is assembled through
`WorkerAssembly`, its generated `permissions.allow` read back — file tools
and `Bash(cargo test)`, no rule naming git — and its deny list checked not
to cover the manifest's root-level path; Copilot's `permission_args` are
writes and the gate. The repair is then completed through the one operation
both admit, a file write: the resolved bytes and the manifest. The review's
witness did the same and failed at `3bce2c6a` with `unresolved == ["c.txt"]`.

## `fn a_declared_path_is_staged_literally_whatever_characters_it_holds() {`

`a[1].txt` is a glob to a pathspec and a file to the index; `sp ace.txt` is
one path. Declared in backticks, with `./` in front and CRLF line endings —
the grammar's tolerances — both are staged as themselves through
`:(literal)`.

## `fn a_resolution_manifest_written_where_nothing_conflicted_is_ignored_and_stays_out_of_the_candidate() {`

An ordinary attempt's worker that writes a manifest anyway: the index holds
nothing the manifest governs, so it is not read, the work is captured, and
the manifest — an untracked regular file the ignore rules do not cover — is
kept out by the exclusion `candidate_stage` appends for exactly that state,
and removed by the same staging, unread. Then the same with a manifest the
grammar refuses: read on its own it is malformed, and the capture, which
does not read it, stages the work as before and removes it as before. "A
malformed manifest stages nothing" is a promise about a capture that reads
one (the third round's manifest-contract review, finding 6).

## `fn the_resolution_manifest_grammar_reads_what_a_worker_writes_and_refuses_the_rest() {`

`ResolutionManifest::parse` against what a worker is likely to write —
CRLF, comments, blank lines, a list bullet, one colon after the keyword,
backticks and quotes around the path, `./`, surrounding whitespace — and
what it refuses, each with the line named: a second colon among them, and
a quoted path is what the quotes hold, whitespace included (the third
round's manifest-contract review, findings 4 and 5). `Declaration::names`
as a path comparison, a backslash a separator exactly where the platform's
Git reads it as one; `names_in_another_case` as the one alias that is
read, for refusing — a normalization form is not a case. And
`plan_resolutions` over both governed lists: no manifest refuses every
unmerged entry, a declaration of a path in neither list is nothing, a path
declared both ways is refused rather than guessed, a contradiction in two
cases and a lone respelling in another case are refused naming the index's
spelling while two index entries that differ only by case are two files and
a normalization-form pair is not read as a contradiction (the recorded
boundary), a malformed manifest refuses everything it governs and says why;
a resolved path is left to the `add -A` when undeclared, is revised to a
deletion when declared so, and is governed once when the index holds it
unmerged too. And `names_in_another_case` folds per character: `ΟΣ` beside
`οσ` and `AΣ` beside `aσ` are aliases, which the contextual string fold
denied (PR #249's fourth-round manifest-contract and adequacy reviews), and
`ς` against `σ` is not one, as the Windows guest's filesystem also says.

## `fn an_already_present_source_proceeds_as_an_ordinary_attempt_whose_empty_diff_fails_honestly() {`

`repairs.empty_source`: an `Empty` observation proceeds as an ordinary
attempt; a worker that then changes nothing fails under the existing
empty-diff rule rather than through a no-candidate settlement that does
not exist (a deferred decision).

## `fn retry_in_place(`

`run::retry_ready` in the fixture's hands: the previous attempt retained
with a gate failure, `settle::retry` reserving and verifying the worktree
against the retained tree, the authorized attempt started with what the
fold recorded for it, the reservation converted. Returned rather than
dropped because a retry's reservation is the caller's to keep alive while
its attempt runs.

## `fn blob_in(worktree: &Path, tree: &str, path: &str) -> Vec<u8> {`

The bytes a captured tree holds at a path, read from the object rather
than the working tree: what the queue would publish.

## `fn a_retained_retry_revises_a_declared_resolution_to_a_deletion_and_a_settled_deletion_is_not_reapplied() {`

PR #249's third-round regression review, finding 2. Attempt 1 resolves
`c.txt` and declares it; the capture stages it, consumes the manifest, and
the index's resolve-undo record now names it (`resolved_conflicts`, empty
before). Attempt 2 in the retained worktree declares `deleted c.txt`:
nothing is unmerged, the manifest is read all the same because the index
still holds what a capture resolved, the engine's `git rm --force` removes
the file the worker's tools could not, the tree no longer carries it, and
that manifest is consumed in turn — at `698777b0` the manifest went unread
and the capture reported success with `c.txt` in the tree and on disk.
Attempt 3, the worker declaring `deleted c.txt` again: the deleted path
has no entry left to govern, so the index holds nothing the manifest
governs, the manifest is not read — and is removed with the capture all the
same, since the fifth round (it stayed until then, and the four-attempt test
below is what a kept one did) — no `git rm` runs against a pathspec that
matches nothing, and the worker's other edit is captured as usual. The log
replays.

## `fn a_retained_retry_reads_the_manifest_while_the_index_holds_what_a_capture_resolved() {`

The other half of the retained rule: a manifest the grammar refuses, in
the retry, refuses the capture and stages nothing, and stays for the worker
to correct — the entry is still governed, so the malformed-manifest promise
holds here — while the corrected manifest re-stages the resolution from the
file's new content and is consumed; with no manifest and the entry still
governed, an ordinary capture stages what the tree holds — the file's newer
content, nothing preserved from the previous capture (PR #249's fourth-round
record review, finding 1, found the notes promising that it was).

## `fn a_tracked_file_of_the_manifests_name_is_the_repositorys_and_a_conflict_repair_there_is_refused() {`

The name, taken. An ordinary attempt in a repository that tracks
`.upstroke-resolved`: the worker's edit to that file is in the captured
tree — the third round's regression review found the old content there,
silently, under the unconditional exclusion. A conflict repair materialized
from a candidate that carries the file (the record review's witness): the
pick places it in the index, and the capture that needs the manifest refuses
`UpstrokeError::Refused` naming the tracked name, before anything is staged.
The same candidate picked cleanly: nothing to govern, the manifest unread,
and the candidate's file captured as the data it is.

## `fn an_ignored_manifest_is_read_and_kept_out_of_the_candidate_by_the_ignore_rules_alone() {`

The manifest-contract review's finding 1: a committed `.gitignore` naming
the manifest, and a required clean filter that would fail on it, so that
any `add` reaching the file is loud. The declaration is read, `c.txt` and
the untracked `.gitattributes` are staged, the manifest is not, and no
exclusion is appended — the one that exactly named an ignored path made
`git add -A` exit 1 at `698777b0`; consumed like any manifest the capture
acted on, since the `clean` carries `-x`.

## `fn a_directory_of_the_manifests_name_is_the_repositorys_and_its_contents_are_captured() {`

The manifest-contract review's finding 2: a pathspec exclusion is a
directory prefix too. An untracked `.upstroke-resolved/data.txt` beside an
ordinary edit, both captured; a tracked one edited, the edit captured (the
old content stayed in the tree at `698777b0`); and with a conflict to
declare, the worker cannot write the manifest where a directory stands, and
the capture refuses naming "a directory" rather than failing on the I/O
error the read would otherwise raise.

## `fn a_declaration_in_another_case_is_refused_and_the_checkout_says_whether_it_named_the_file() {`

The manifest-contract review's finding 3, whose filesystem half was
reasoned; this establishes it on each platform CI runs. `Dir/C.txt`
conflicted, and the checkout is asked whether `dir/c.txt` is that file —
yes on Windows and macOS, no on Linux, asserted so that the record's
platform statement is measured rather than assumed. The rule is the same on
all three: the pair `resolved Dir/C.txt` / `deleted dir/c.txt` is refused
as a contradiction, the lone `resolved dir/c.txt` is refused naming the
index's spelling, and `resolved Dir/C.txt` captures. Then the boundary the
exact rule leaves: `café.txt` composed, its decomposed respelling names the
file on macOS alone, and the engine reads it as nothing everywhere — the
pair is not refused and the composed declaration is staged. Pinned so that
folding normalization (`PR249-MANIFEST-NORMALIZATION-ALIAS`) moves this
test with it.

## `fn a_quoted_declaration_names_an_entry_exactly_whitespace_included() {`

The manifest-contract review's finding 4. An index entry ` c.txt `, spaces
and all: unquoted, the trim leaves `c.txt` and the entry stays undeclared;
quoted, the path is what the quotes hold and the resolution is staged. Unix
only, because Windows does not hold such a name.

## `fn a_declared_resolution_reaches_the_configured_checks_which_decide_what_they_detect() {`

The record and manifest-contract reviews' shared witness against
`design/26` §26.4's old word "caught": markers left in `c.txt`, the file
declared resolved, no gate and no reviewer in the plan. The capture stages
the declaration, no cheap rung reads the markers, and the judgment accepts.
What a wrong declaration reaches is the configured validation; the design
now says so.

## `fn a_settled_deletion_is_not_revived_by_the_manifest_that_made_it_once_the_path_is_recreated() {`

PR #249's fourth-round regression and manifest-contract reviews, one
witness each, in their shape. Attempt 1 declares `deleted c.txt` and the
capture removes the file and consumes the manifest; `resolved_conflicts`
is empty, a settled deletion governing nothing. Attempt 2 recreates the
file with a file write and declares nothing: an ordinary addition, and the
read now names the path again — the resolve-undo record survived the
deletion and the addition put an index entry back beside it, which is the
state the reviewers' third capture reread the standing declaration in and
removed the file from the disk and the candidate. Attempt 3 edits another
file: one staging, the recreated file on disk and in the tree with the
bytes of attempt 2. The log replays.

## `fn a_manifest_standing_where_a_tracked_directory_was_hides_none_of_its_deletions() {`

The manifest-contract review's finding 1 of the fourth round. An ordinary
attempt whose base tracks `.upstroke-resolved/data.txt`: the worker removes
the file and its directory with file operations, writes a regular file at
the name, edits another file and writes a nested `sub/.upstroke-resolved`.
The captured tree holds the edit and the nested file, not the worker's
file, and not the deleted `data.txt` — at `b2946956` the exclusion, a
directory prefix, kept that deletion out and the tree still held the file.
The idle manifest, read by nothing, is removed with the capture. Then a
conflict repair whose
worker removes the directory and declares: the file at the name is the
worker's manifest, read and consumed, the resolution and the directory's
deletion both in the tree, one `Object.CandidateStage` execution.

## `fn a_tracked_file_of_the_manifests_name_in_another_case_is_the_repositorys_on_every_platform() {`

The manifest-contract review's finding 2 of the fourth round, established
natively on the Windows guest. The base tracks `.UPSTROKE-RESOLVED`; the
worker resolves `c.txt` and writes its manifest at the lowercase name,
which the test first reads back through the tracked name to record what
the checkout did — the tracked file's bytes on Windows and macOS, a second
file on Linux. The capture refuses (`Refused`, naming `.UPSTROKE-RESOLVED`
and "by case alone"), stages nothing, and leaves the index unmerged; at
`b2946956` the exact-spelling reads found nothing and the repository's
file, overwritten, was read as the manifest and staged. An ordinary attempt
in that repository then edits the tracked file and is captured with the
edit.

## `fn a_case_alias_is_read_per_character_so_a_final_sigma_hides_no_contradiction() {`

The manifest-contract and adequacy reviews' shared finding of the fourth
round: `ΟΣ` conflicted and declared `resolved` beside `deleted οσ`. The
test first shows the contextual fold unequal on the pair, then records per
platform whether the checkout reads `οσ` as the conflicted file (the guest
said it does), and asserts the same three outcomes as the `Dir/C.txt` test:
the pair refused as a contradiction, the lone respelling refused naming the
index's spelling, the index's spelling staged. The reviewers' witness — and
the adequacy review's Unicode-to-ASCII mutation of the fold, which survived
every scoped test at `b2946956` — fail here.

## `fn a_declaration_no_capture_read_does_not_outlive_it_so_a_recreated_path_is_governed_by_no_stale_one() {`

PR #249's fifth-round adequacy and manifest-contract reviews, one witness
each, in their shape — the sequence the fourth round's repair did not reach,
because it removed the manifest a capture *acted on* and this one never
acts on the manifest it loses to. Attempt 1 declares `deleted c.txt`,
consumed. Attempt 2 declares it again while the path has no entry, and
edits another file: nothing is governed, the manifest is not read, the edit
is captured, and the manifest is removed with the capture — the assertion
the reviewers' witnesses fail without, since at `6448262e` it stayed.
Attempt 3 recreates the file: an ordinary addition, after which
`resolved_conflicts` names the path again. Attempt 4 edits another file: one
staging, the recreated file on disk and in the tree with attempt 3's bytes —
at `6448262e` this capture read attempt 2's declaration and deleted it. The
log replays.

## `fn an_ignored_manifest_standing_where_a_tracked_directory_was_captures() {`

PR #249's fifth-round regression review, both shapes it executed. The base
tracks `.upstroke-resolved/data.txt` and ignores `.upstroke-resolved`; the
worker deletes the file and its directory, writes its manifest at the name
and edits another file — and in the repair shape resolves and declares
`c.txt`. At `6448262e` the `add -A` of what the index held under the name
staged the deletion and exited 1, "The following paths are ignored", and
the capture aborted with the resolution already staged; `add -u` walks the
index alone. One staging; the edit, the resolution and the `.gitignore` in
the tree, the deleted file and the worker's file not; the ignored manifest
removed.

## `fn an_untracked_file_of_the_manifests_name_in_another_case_is_the_workers_where_the_checkout_folds_case() {`

PR #249's fifth-round manifest-contract review, established natively on the
Windows guest. An untracked `.Upstroke-Resolved` is written first; the worker
then writes its declaration through the lowercase name, and the test records
per platform whether the two are one file — the variant's bytes are the
declaration on Windows and macOS, and not on Linux. The capture reads the
manifest under the spelling the checkout lists (at `6448262e` the
exact-spelling reads found nothing, classified it ignored, staged it into the
candidate as `.Upstroke-Resolved` with the declaration text and left it on
disk), stages the resolution, keeps every spelling of the name out of the
candidate where the two are one file, and removes it under that spelling;
where they are two files, the variant is a second file of the repository's,
captured as one, and the manifest spelt as written is excluded and removed.
