---
id: G4-O1-REJECTION-KEEPS-CANDIDATES-REF-UNWITNESSED
severity: P3
disposition: deferred
category: correctness
pr: 249
reviewed_sha: 81ee09efb5926fb4b93565223ea05dd0052bc2f9
location: src/engine/topology/integrate.rs:650
provenance: pre_existing
first_bad:
guard: the round that makes the G3 shape table's two rejection arms read the rejected candidate's candidates ref and expect the candidate — before PR10 writes the first production caller of `Ref.DeleteCandidatesRef`
---

## Failure sequence

R11 says the rejected candidate's candidates ref is *"protected while the run can resume, whatever the
candidate's state"*, pruned only by Complete finalization; PR9 dispatches every repair from it
(`rejected_source`, `dispatched_source` in `src/engine/topology/run.rs`). At `81ee09ef` the
protection holds, and it holds for one reason: no production caller of `Ref.DeleteCandidatesRef`
exists yet. Nothing in the suite would notice one appearing.

    G4 rerun mutation M22: the Conflict arm of `integrate.rs` additionally deletes the rejected
    candidate's candidates ref through `Ref.DeleteCandidatesRef` after emitting `merge_rejected`
    -> the full library suite passes (2436 passed, 0 failed, samplers skipped)
    G4 rerun mutation M23: the same in the CodeRejected arm
    -> the same result
    -> the rerun's measurements that reject a candidate through the production integration path and
       then let the loop dispatch the repair (`G4B5`, a gate failure; `G4B6`, a conflict) read the ref
       present afterwards in the unmutated tree; under the mutations the same dispatch would meet the
       defence-in-depth refusal in `dispatch.rs` (`refuse_absent_source`), and no committed test
       reaches it

Why nothing catches it: every test that dispatches a repair (`src/engine/topology/recover/tests.rs`)
plants its `merge_rejected` directly, and every test that produces a rejection through `integrate`
(`terminal_shape_coverage_table_drives_every_shape_and_each_converges_on_replay`,
`a_conflicting_candidate_is_rejected_with_an_atomic_repair_before_any_repair_effect`) asserts events,
pins and hook order and never reads a candidates ref (`grep -c 'candidates/'
src/engine/topology/integrate/tests.rs` is 0).

The G3 gate report's Q2 table (R11 row) says *"The shape table's rejection arms assert the ref
remains."* That sentence was not true at `9cfaf0c8` and is not true at `81ee09ef`; the G4 rerun
recorded the correction as a dated addendum to that report rather than editing the row. The row's
first sentence, that `Ref.DeleteCandidatesRef` is reached only at Complete finalization, is the true
one and is why the protection held.

This finding was first filed by the first G4 run on its superseded branch (`gate/g4`, never merged)
at `74da2cbb`; it is re-filed here at the corrected range with the rerun's own measurements.

Not a behaviour defect at this sha: the rerun's §8 measures the ref present and unchanged across a
parked resume, a budget-stopped resume, a real conflict rejection, a real code rejection and the
repair's merge. It is a guard with no witness, on the one invariant PR10's finalization is about to
gain a way to break.

## What the change that takes this up should do

Add to the shape table's `Conflict` and `CodeRejected` arms one assertion each: read the rejected
candidate's `candidate_ref` with `direct_ref_target` and expect its `commit_sha`. Alternatively adopt
the rerun's `G4B5`/`G4B6` shape as a recover test (a candidate rejected through `integrate`, then the
repair dispatched by the loop, then the ref read). Either makes mutations M22 and M23 die. Do it
before the change that adds the `Ref.DeleteCandidatesRef` caller, so that the caller is written
against a test that says where it may not run.
