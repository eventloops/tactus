---
id: G4-O1-REJECTION-KEEPS-CANDIDATES-REF-UNWITNESSED
severity: P3
disposition: deferred
category: correctness
pr: 249
reviewed_sha: 74da2cbbd24c55f7aed7f3593162981a11720f79
location: src/engine/topology/integrate.rs:650
provenance: pre_existing
first_bad:
guard: the round that makes the G3 shape table's two rejection arms read the rejected candidate's candidates ref and expect the candidate — before PR10 writes the first production caller of `Ref.DeleteCandidatesRef`
---

## Failure sequence

R11 says the rejected candidate's candidates ref is *"protected while the run can resume, whatever the
candidate's state"*, pruned only by Complete finalization; PR9 dispatches every repair from it
(`rejected_source`, `dispatched_source` in `src/engine/topology/run.rs`). At `74da2cbb` the protection
holds, and it holds for one reason: no production caller of `Ref.DeleteCandidatesRef` exists yet.
Nothing in the suite would notice one appearing.

    G4 mutation M22: the Conflict arm of `integrate.rs` (line 650) additionally deletes the rejected
    candidate's candidates ref through `Ref.DeleteCandidatesRef` after emitting `merge_rejected`
    -> the 79-test G4 detection set passes; the full library suite passes (2445 passed, 0 failed)
    G4 mutation M23: the same in the CodeRejected arm (line 785)
    -> the same result
    -> a temporary measurement that rejects a stale candidate through the production integration path
       and then lets the loop dispatch the repair shows, under either mutation, the dispatch refused:
       "its authoritative ref `refs/upstroke/runs/<run>/candidates/0/0` does not exist, and it is
       what keeps the candidate reachable" — the defence-in-depth refusal in `dispatch.rs` works,
       and no committed test ever reaches it

Why nothing catches it: every test that dispatches a repair (`src/engine/topology/recover/tests.rs`)
plants its `merge_rejected` directly, and every test that produces a rejection through `integrate`
(`terminal_shape_coverage_table_drives_every_shape_and_each_converges_on_replay` at
`src/engine/topology/integrate/tests.rs:732`, `a_conflicting_candidate_is_rejected_with_an_atomic_repair_before_any_repair_effect`)
asserts events, pins and hook order and never reads a candidates ref
(`grep -n 'candidates/' src/engine/topology/integrate/tests.rs` is empty).

The G3 gate report's Q2 table (R11 row) says *"The shape table's rejection arms assert the ref
remains."* That sentence was not true at `9cfaf0c8` and is not true at `74da2cbb`; G4 recorded the
correction as a dated addendum to that report rather than editing the row. The row's first sentence,
that `Ref.DeleteCandidatesRef` is reached only at Complete finalization, is the true one and is why the
protection held.

Not a behaviour defect at this sha: the G4 report's §8 measures the ref present and unchanged across a
parked resume, a budget-stopped resume, a real conflict rejection, a real code rejection and the
repair's merge. It is a guard with no witness, on the one invariant PR10's finalization is about to
gain a way to break.

## What the change that takes this up should do

Add to the shape table's `Conflict` and `CodeRejected` arms one assertion each: read the rejected
candidate's `candidate_ref` with `direct_ref_target` and expect its `commit_sha`. Alternatively adopt
the G4 measurement's shape as a recover test (a stale candidate rejected through `integrate`, then the
repair dispatched by the loop, then the ref read). Either makes mutations M22 and M23 die. Do it before
the change that adds the `Ref.DeleteCandidatesRef` caller, so that the caller is written against a
test that says where it may not run.
