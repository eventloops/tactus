# `src/engine/topology/dispatch/tests.rs`

Extended notes for [`src/engine/topology/dispatch/tests.rs`](../../../../../src/engine/topology/dispatch/tests.rs).

The code is the authority for what it does; this file is the whole of its prose, moved out of
the source verbatim. Each section is headed by the line of code the comment sat above, spelled
as it is in the source, so the heading is the grep string that finds the code.

## Module

`T-DISPATCH`, and the two clauses whose directions disagree.

## `fn git_dir(worktree: &Path) -> PathBuf {`

The git directory of a linked worktree, asked of Git rather than derived.

`<worktree>/.git` is a **file** in a linked worktree, and the administrative
directory it points at is where every residue element of an interrupted
command lands. Deriving the path here would be a second implementation of
`workspace_manager`'s own `git_dir_of`; asking `git` is the one answer both
agree with.

## `fn plant(worktree: &Path, element: ResidueElement) {`

Plant one residue element in a worktree's git dir.

The five this covers are exactly the ones
`workspace_manager::element_breaks_quiescence` holds of and that
`administrative_residue_at` reads — the object-store two (`R27`,
unreferenced objects and temporary object files) are deliberately **not**
here, because `Worktree.Verify` must not consult the object store: every
amended commit in a real repository leaves an unreferenced object, and a
verify that read one would refuse to reuse an `OpenNoAttempt` worktree in
essentially every repository this engine will ever run in.

## `const ADMINISTRATIVE: [ResidueElement; 5] = [`

The five administrative elements `Worktree.Verify` reads, with the
`VerifyFailure` each must produce.

## `fn healthy_at(manager: &WorkspaceManager, worktree: &Path, base: &str) -> bool {`

Whether `worktree` is a registered, populated worktree of `manager`'s
repository whose HEAD is `base`.

## `fn task_dispatched_is_durable_before_the_intent_and_the_add() {`

---------------------------------------------------------------------------
O21
---------------------------------------------------------------------------

## `fn task_dispatched_is_durable_before_the_intent_and_the_add() {`

**O21.** `task_dispatched` is appended, and is durable, before either
worktree effect.

Two axes, because the clause is about two different things and one
assertion covers neither on its own.

*Order* is read off the one [`crate::topology::effects::HookHarness`] all
five hook families record into, so the append and the `git worktree add`
are positions in a single list rather than two lists a test has to
interleave by hand.

*Durability* is read off the bytes on disk. Order alone would stay green if
the append were buffered and the worktree created before the buffer
reached the file — and the whole point of writing the event first is that a
process that dies between them left the event behind. So the log is read
back from the filesystem and its second line must already be the dispatch.

## `fn a_containment_condition_that_fails_mid_run_refuses_before_the_append() {`

**`T-DISPATCH`'s `refusal_condition`, the containment half.** A containment
condition that starts failing *during* a run refuses before the append.

`T-DISPATCH` lists "worktree path outside execution root or on a reparse
point" beside "source candidate object missing", and the reason
[`refuse_absent_source`] is hoisted above the append applies identically to
them: an event written for a generation whose worktree can never be built
leaves an `OpenNoAttempt` generation that every later resume tries and fails
to recover.

The three conditions are facts about the filesystem rather than about the
request — `execution_root` requires "the canonical root is inside no
repository worktree, **and no repository worktree is inside it**" — so they
can hold at `run_started` and fail by the time a dispatch runs. That is what
is constructed here: a worktree this manager never registered, created
inside its execution root after the run started.

`write_intent` and `add_worktree` each revalidate, so this refusal fires
with or without the pre-check. What the pre-check decides is **which side of
the append** it fires on, and that is the whole of what is asserted: the
durable log is unchanged and no append ran after the mark.

## `fn a_containment_condition_that_fails_mid_run_refuses_before_the_append() {` › `git(`

The control: with the foreign worktree gone the same dispatch is
accepted, so the refusal above is about containment and not about the
request being malformed in some other way.

## `fn a_fresh_dispatch_never_verifies_a_worktree_it_is_about_to_create() {`

**O22, the half that is easiest to get backwards.** A fresh dispatch does
not verify.

`Worktree.Verify` guards *reuse*. If it guarded creation there would be no
state in which a worktree exists, carries residue, and is recreated — and
`residue_carrying_worktree_fails_verify_and_is_recreated` below would be
inexpressible rather than merely failing.

The second half of the assertion is what stops this being vacuous: the same
harness, one reuse later, *must* have seen the site. A test that only
asserted the absence would pass against a `Verify` that had been deleted
from the module entirely.

## `fn residue_carrying_worktree_fails_verify_and_is_recreated() {`

---------------------------------------------------------------------------
O22 — reuse
---------------------------------------------------------------------------

## `fn residue_carrying_worktree_fails_verify_and_is_recreated() {`

**O22.** A worktree carrying the residue of an interrupted Git command
fails `Worktree.Verify` and is removed with force and recreated.

Driven once per administrative residue element rather than once, because
the classifier's list is `ResidueElement`'s and a verify that recognised
four of the five would answer "reusable" for a worktree holding sequencer
state — which is a `git cherry-pick` that stopped half way, and reusing it
would run the next attempt on a tree nobody chose.

The two non-administrative failures are crossed in beside them: a HEAD that
moved off the base, and a worktree that is not registered at all. All three
shapes route to the same recovery, and asserting only one would leave the
other two to `NotRegistered`'s branch by accident.

## `fn residue_carrying_worktree_fails_verify_and_is_recreated()` › `git(`

A HEAD that moved. Not administrative residue, and the same recovery.

## `fn residue_carrying_worktree_fails_verify_and_is_recreated()` › `run.fixture`

And a worktree that is simply gone.

## `fn residue_carrying_worktree_fails_verify_and_is_recreated()` › `for site in [REMOVE, INTENT, ADD] {`

The recreation really went through the two funnels, in the order the
clause gives them, and through the forced removal in between.

## `fn a_quiescent_worktree_is_reused_rather_than_rebuilt() {`

A verified worktree is **reused**, and the recreation path is not entered.

The control half of the test above. Without it, a `verify_or_recreate` that
removed and rebuilt unconditionally would satisfy every assertion there —
the failures would still be reported, because they are read off the
verification, and the worktree would still be healthy afterwards.

## `fn dispatch_kill_child() {`

---------------------------------------------------------------------------
T-DISPATCH — the kill
---------------------------------------------------------------------------

## `fn dispatch_kill_child() {`

The child of [`kill_after_dispatch_recreates_worktree_without_spend`].

Dies at one of the two prefixes `T-DISPATCH`'s boundary names: "worktree
intent or worktree not yet created" and "created without `attempt_started`".

## `fn kill_after_dispatch_recreates_worktree_without_spend() {`

**`T-DISPATCH`.** A coordinator killed after `task_dispatched` leaves an
`OpenNoAttempt` generation with **no spend**, and the recovery rebuilds or
reuses its worktree without repeating one.

Both prefixes of the boundary, because their recoveries differ and only one
of the two branches would otherwise be executed: at `before_intent` nothing
on disk exists and the worktree is built from nothing, at `after_add` the
worktree exists and quiesces and is reused. A test that drove only the first
would pass against a recovery that force-removed every worktree it found.

"Without spend" is asserted three ways: no `attempt_started` in the durable
log, the generation still `OpenNoAttempt`, and the task still `Pending`.
The first is the durable claim; the other two are what a scheduler reads,
and a fold that admitted a spend the log did not record would be caught by
the disagreement rather than by any one of them.

## `fn kill_after_dispatch_recreates_worktree_without_spend()` › `let existed = dispatched.worktree.is_dir();`

What the child actually left, which is the difference between the
two prefixes and is asserted rather than assumed.

## `fn protected_candidate(run: &mut Run) -> CandidateRef {`

---------------------------------------------------------------------------
T-DISPATCH — repairs
---------------------------------------------------------------------------

## `fn protected_candidate(run: &mut Run) -> CandidateRef {`

The candidate ref a repair fixture materializes from, and the `CandidateRef`
naming it.

The commit is the fixture's `side`, which is a real commit on `seed` adding
one file, so a cherry-pick of it onto `head` applies cleanly and its effect
is visible as a path in the index.

## `fn repair_kill_child() {`

The child of [`repair_materialization_reproduced_after_kill`]: dispatches a
repair and dies at `Object.RepairMaterialize`.

## `fn repair_materialization_reproduced_after_kill() {`

**`T-DISPATCH`.** "for repairs re-run the recorded materialization in a
verified or fresh worktree".

Both sides of the materialization, because they leave different worktrees
and the recovery has to converge from each: killed *before* it, the worktree
is at the base with a clean index; killed *after* it, the worktree carries
the merged index and, in this test's after-phase kill, the funnel's
`MERGE_MSG` and `AUTO_MERGE` not yet cleared, which `Worktree.Verify` reads
as administrative residue and recreates from. Not `CHERRY_PICK_HEAD`:
`cherry-pick --no-commit` never writes it (measured on git 2.43, clean and
conflicting picks alike; the record's R5). A kill between the index's
publish and `MERGE_MSG.lock` leaves the merged index with no state file at
all, which verifies like a completed pick and converges through the funnel's
restore of the base's tree (`a_materialization_killed_after_its_index_write_converges_from_both_of_its_states`).
This paragraph said until PR #249's fifth repair round that the after-kill
worktree carries `CHERRY_PICK_HEAD` and is refused; its record review found
the copy.

The oracle is the recorded source, not a path list: after recovery the
worktree's index must hold exactly what an uninterrupted materialization
would have produced. That is computed by materializing the same candidate in
a **second, independent** generation and comparing the two indexes — so the
assertion is "the same as doing it once, uninterrupted" rather than "some
file appeared", which a half-applied cherry-pick also satisfies.

## `fn repair_materialization_reproduced_after_kill()` › `let control = Dispatched {`

The independent oracle: the same candidate, materialized once, in a
worktree nothing ever killed.

## `fn a_repair_whose_source_candidate_is_missing_is_refused_before_any_append() {`

**`T-DISPATCH`'s `refusal_condition`.** "source candidate object missing",
and the refusal costs no durable state.

Three shapes, because a missing candidate arrives three ways and only one of
them is literally an absent object: the commit is gone, the authoritative
ref that keeps it reachable is gone, or the ref names something else. Each
must refuse **before the append**, which is what the durable-log assertion
after each one is for — a refusal raised after `task_dispatched` would leave
an open generation whose worktree can never be built.

## `fn a_repair_whose_source_candidate_is_missing_is_refused_before_any_append() {` › `let request = DispatchRequest {`

The control: the same dispatch with the real candidate is accepted, so
the three refusals above are about the candidate rather than about the
request being malformed in some other way.

## `fn reproducing_a_materialization_an_ordinary_dispatch_never_had_is_refused() {`

An ordinary dispatch has no materialization to reproduce, and asking for one
is a refusal rather than a silent success.

## `fn open_no_attempt_closed_at_run_end() {`

---------------------------------------------------------------------------
Run end
---------------------------------------------------------------------------

## `fn open_no_attempt_closed_at_run_end() {`

**ST-17 / `T-DISPATCH`.** "at run end: `generation_closed{RunEnding}`", and
the worktree is scrubbed **after** it.

The ordering is `cleanup`'s: "task worktree scrubbed only after
`task_candidate_created` is durable **or the generation is Closed**". A
scrub that ran first would remove a worktree the log still calls resumably
open, and a resume between the two would try to verify it.

## `fn open_no_attempt_closed_at_run_end()` › `let close = run.order_after(mark, APPEND, HookPhase::After);`

Counted from a mark, not from the first observation of each site: the
dispatch that opened this generation already drove both, so a comparison
of first observations compares the wrong pair and is true whatever this
function does. Measured — with the scrub moved in front of the closure it
stayed green.

## `fn a_repairs_run_end_closure_holds_the_lineage_lease() {`

A repair's run-end closure records `LineageHeld`, not `PredictedReleased`.

The one field the two dispatch kinds disagree about at a terminal, and the
fold refuses the wrong one — `check_lease_disposition` compares it against
`GenerationLease::expected(false)`. Without this the ordinary case above
would be the only disposition ever executed, and a `closing_disposition`
that answered `PredictedReleased` unconditionally would be invisible.

## `fn an_add_whose_intent_is_gone_is_refused_rather_than_leaking_a_worktree() {`

The intent is durable before the add, and the add refuses without it.

`Refusal::AddWithoutIntent` is `workspace_manager`'s, and this is the
dispatch-side statement of what it protects: a worktree created without a
durable intent is one `reclaim_intents` can never find. Driven by removing
the intent and re-adding, because the funnel cannot be made to skip it.

## `fn a_repair_dispatch_records_what_its_materialization_observed() {`

The three observations a materialization can make — `Clean`, `Conflict`,
`Empty` — each recorded on the `Dispatched` value, each generation closed
at run end with `LineageHeld`, and the log replaying equal.

## `fn repair_materialization_synthetic_residue_recreated_after_forced_removal() {`

`command_internal_sub_effects`, synthetic half, for `Object.RepairMaterialize`:
each of the four declared elements planted into a fresh worktree at the
base, classified `Internal`, and recovered by `resume_open_no_attempt` —
reused or recreated exactly as `element_breaks_quiescence` says, the
materialization reproduced once, the tree the control's, the planted
objects untouched.

## `fn checkout_bytes(worktree: &Path) -> BTreeMap<String, Vec<u8>> {`

The checkout the worker is handed, read from disk: every path the index
names, with the bytes its file holds. Never `write-tree`, which reads the
index and not the files — PR #249's refusals review kept the index right and
overwrote the files (mutation M6), and every SHA oracle in this module
passed.

## `fn expected_checkout(`

What one pick of `source` onto `base` must leave on disk, derived from the
two commits and nothing the materialization wrote: `base`'s files, with the
paths `source` adds or changes read from `source`. The independent expected
value the byte oracles compare against.

## `fn assert_checkout_is(worktree: &Path, expected: &BTreeMap<String, Vec<u8>>, label: &str) {`

The worktree's files are exactly `expected`, and the index agrees with them
(`diff-files --quiet`): the two reads that together pin what the worker
sees. Every materialization test in this module ends in it.

## `fn sampled_repair_materialization_child_kills_every_residue_classified_and_recovered() {`

The kill-sampling half: `SAMPLING_N` real `cherry-pick --no-commit`
children that died by the kill, at spread points, every residue classified
and every worktree recovered to the control's tree and bytes. Its first run
found the held `MERGE_MSG.lock` the verifier did not read.

**A completed pick is a control, not a sample.** PR #249's refusals review
killed one child at spawn and let seven picks finish (M3); the floor of
"eight classified samples and one kill" accepted `killed=1/8,
observed=[None, After ×7]`. The populations are kept apart: kills are
collected over a bounded number of spawns until `SAMPLING_N` of them have
been observed, and a completed pick is verified to converge and counted as
nothing.

**A kill of a child that had not begun is a sample of nothing.** The
adequacy review then removed the sampler's delay so that every child was
killed the instant it was spawned, and "at least one kill before the index
was published" accepted `killed=[None ×8]`. So each kill records whether it
landed while the pick was writing (`KilledSample::while_writing`): after its
first write — `index.lock`, so `Internal` — and before its last, `MERGE_MSG`,
which a completed `--no-commit` pick always leaves; the loop keeps sampling,
within `MAX_SPAWNS`, until it has seen one, and the test fails when none of
its kills did. Measured on the build box, ten runs after the change: every
run saw one or two such kills among its eight, in ten or eleven spawns, the
rest `None`; `Internal` is what they were in nine runs, `After` without
`MERGE_MSG` in the tenth, and two kills in all found a pick that had
finished.

**The kill has to reach git.** CI at `56ea88c9` failed this floor on the
winguest lane with 30 kills in 32 spawns, ten `None` and twenty `After` with
`MERGE_MSG` in place, none between: on Windows `git` on the `PATH` is Git for
Windows' `cmd\git.exe`, a 46 KB launcher that starts the real
`mingw64\bin\git.exe` as its own child and waits, so `Child::kill` ended
the launcher and the pick either never began or ran to completion — no
sampled kill had ever interrupted git on that lane. `KillableGitChild::spawn`
now runs the real binary, resolved once through `git --exec-path`
(`fixture::sampled_git`); PR5's four-command census sampler spawns `git` by
name still and is recorded, not changed
(`PR249-KILL-SAMPLER-WINDOWS-WRAPPER`). The same run failed the kill-count
floor on macOS with 6 kills in 32 spawns and 26 picks complete before their
kill — four of the six mid-write — because the budget was one cold probe pick
spread over a schedule most warm picks beat; the budget is now the shorter of
two probe picks and `MAX_SPAWNS` is `8 × SAMPLING_N`.

## `fn a_continuation_after_a_completed_pick_hands_the_worker_the_tree_one_pick_produces() {`

PR #249's crash review, finding 1: the coordinator dies after the pick
completed and before `attempt_started`; (g) reuses the quiescent worktree
and the continuation picks again, twice over. The index and the working
tree must hold what one pick produces — the funnel restores the base's tree
before each pick, because a pick onto the merged index applied its hunk a
second time on every resume.

## `fn repair_materialization_objects_released_to_git_on_scrub() {`

R9 → R27: a three-way merge's new blob is referenced by the repair index
and becomes unreachable, still present, when the worktree is scrubbed; the
candidate commit stays reachable through its ref.

## `fn a_materialization_killed_after_its_index_write_converges_from_both_of_its_states() {`

The two kill points after the pick's index write, pinned deterministically:
the held `MERGE_MSG.lock` is read as the message's residue and recreated
from; the merged index with no state file — also the completed after
phase — is reused, and the re-run pick starts from the restored base tree
and reports the same observation, for a clean source and for a conflicting
one.
