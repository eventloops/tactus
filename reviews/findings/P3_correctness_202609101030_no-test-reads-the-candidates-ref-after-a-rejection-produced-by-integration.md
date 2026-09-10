---
id: G4-O1-REJECTION-KEEPS-CANDIDATES-REF-UNWITNESSED
severity: P3
disposition: deferred
category: correctness
pr: 249
reviewed_sha: 7e0110a13acce525567a7ac43abb016a232d6236
location: src/engine/topology/integrate.rs:650
provenance: pre_existing
first_bad:
guard: the round that makes the G3 shape table's two rejection arms read the rejected candidate's candidates ref and expect the candidate — before PR10 writes the first production caller of `Ref.DeleteCandidatesRef`
---

## Failure sequence

R11 says the rejected candidate's candidates ref is *"protected while the run can resume, whatever
the candidate's state"*, pruned only by Complete finalization; PR9 dispatches every repair from it.
At `7e0110a1` the protection holds, and it holds for one reason: **no production caller of
`Ref.DeleteCandidatesRef` exists yet**. Nothing in the suite would notice one appearing.

    G4 run-3 mutation M22: the Conflict arm of integrate.rs additionally deletes the rejected
    candidate's candidates ref through Ref.DeleteCandidatesRef after emitting merge_rejected
    -> the full library suite passes: rc=0, "test result: ok. 2452 passed; 0 failed; 45 ignored"
       (samplers and the two allow-placement censuses skipped)
    G4 run-3 mutation M23: the same in the CodeRejected arm
    -> the same result, rc=0, 2452 passed, 0 failed

Measured here at `7e0110a1` (`~/tactus-artifacts/g4r3-evidence-7e0110a1/mutations/mut-M22.log`,
`mut-M23.log`), reproducing the same two survivors G4's second run measured at `81ee09ef`. The
run's own measurements that reject a candidate through the production integration path and then let
the loop dispatch the repair (`G4B5`, a gate failure; `G4B6`, a conflict) read the ref present
afterwards in the unmutated tree; under the mutations the same dispatch would meet the
defence-in-depth refusal in `dispatch.rs` (`refuse_absent_source`), and no committed test reads the
ref itself after a rejection produced by `integrate`.

So R11's protection at this sha rests on end-to-end measurement and on the structural absence of a
caller, not on a committed regression witness. PR10 is the change that adds the first such caller.

## What the change that takes this up should do

One assertion in each rejection arm of the G3 shape table: after the rejection, read
`refs/upstroke/runs/<run>/candidates/<key>/<gen>` and expect the rejected candidate's commit, and
read the object. That kills M22 and M23 and turns the protection from a structural accident of this
build into a guarded invariant. Do it **before** PR10 lands, not after.
