---
id: G4-O2-OVERRIDE-SINGLE-RUNG-EXHAUSTION-UNWITNESSED
severity: P3
disposition: deferred
category: correctness
pr: 249
reviewed_sha: 81ee09efb5926fb4b93565223ea05dd0052bc2f9
location: src/engine/topology/run.rs:1610
provenance: introduced_by_feature
first_bad:
guard: the round that gives the driven recover fixture a counting question-id source and commits the G4 rerun's `G4C3` shape: an overridden repair driven through `attempts_per` failures to a `Parked` settlement with a fresh `Unblock` question
---

## Failure sequence

`ladder_policy` in `src/engine/topology/run.rs` gives a task with a recorded one-off binding a ladder
of exactly one rung; without the override an empty-intersection repair's frozen ladder has zero rungs,
which `next_step` reads as `Fail` on the first failure. The packet states the consequence — *"a
single-rung ladder with the kind's attempts_per; exhaustion escalates to the human rung"* — and lists
no proof test for it, and none is committed.

    G4 rerun mutation M34: `rungs: if binding_override(key).is_some() { 1 } else { rungs.len() }`
      replaced by `rungs: entry.ladder.rungs.len()`
    -> every committed test passes; the only test that dies is the rerun's temporary measurement
       `G4C3`, which is not in the tree
    -> so under the mutation an overridden repair that fails once is Failed, and its lineage with it,
       where the design says a person is asked — and no committed test says so

What the rerun observed at `81ee09ef`, with the fixture's fixed-id question source replaced by a
counting one (`G4C3`, report §6.3): the override is recorded, the first gate failure settles
`Closed{Retry}` (attempts 1 of 2 on the one rung), the second generation's failure settles `Parked`
with a fresh `Unblock` question — "2 attempt(s) across 1 rung(s) all failed, and the escalation chain
is spent" — the repair is `AwaitingInput` and the lineage alive. That is the claimed behaviour,
executed at the loop; it lives only in the gate's scratch files.

Why the committed suite cannot see it: the driven recover fixture's question-id source (`FixedIds`
at `src/engine/topology/recover/tests.rs`) answers every `question_id()` with `q-park-fixed`, and a
run that has consumed that id for the repair's admission question cannot raise a second one — the
fold refuses the exhausted attempt's `Parked` settlement as a reused question identity. A
`SequentialIds` source of a dozen lines removes the limit; the rerun wrote one for its measurement.

This finding was first filed by the first G4 run on its superseded branch (`gate/g4`, never merged)
at `74da2cbb`, when the loop-level behaviour had not yet been observed; it is re-filed here with the
observation made and the witness still uncommitted.

## What the change that takes this up should do

Commit a counting id source for the driven fixture (or take a prefix), then commit the `G4C3` shape:
an empty-intersection repair, the binding answer, the gate failing `attempts_per` times, and the
assertion that the settlement is `Parked` with a fresh question rather than `Failed`. That kills M34
and removes the fixture limit for any later test that needs two questions in one run.
