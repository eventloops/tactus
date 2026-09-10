---
id: G4-O2-OVERRIDE-SINGLE-RUNG-EXHAUSTION-UNWITNESSED
severity: P3
disposition: deferred
category: correctness
pr: 249
reviewed_sha: 7e0110a13acce525567a7ac43abb016a232d6236
location: src/engine/topology/run.rs:1610
provenance: introduced_by_feature
first_bad:
guard: the round that gives the driven recover fixture a counting question-id source and commits the G4 measurement's `G4C3` shape — an overridden repair driven through `attempts_per` failures to a `Parked` settlement with a fresh `Unblock` question
---

## Failure sequence

`ladder_policy` in `src/engine/topology/run.rs` gives a task with a recorded one-off binding a
ladder of exactly one rung; without the override an empty-intersection repair's frozen ladder has
zero rungs, which `next_step` reads as `Fail` on the first failure. The packet states the
consequence — *"a single-rung ladder with the kind's attempts_per; exhaustion escalates to the human
rung"* — and lists no proof test for it, and none is committed.

    G4 run-3 mutation M34: `rungs: if binding_override(key).is_some() { 1 } else { rungs.len() }`
      replaced by `rungs: entry.ladder.rungs.len()`
    -> every committed test passes; the only test that dies is this run's temporary measurement
       `G4C3`, which is not in the tree
    -> so under the mutation an overridden repair that fails once is Failed, and its lineage with
       it, where the design says a person is asked — and no committed test says so

What this run observed at `7e0110a1`, with the fixture's fixed-id question source replaced by a
counting one (gate report §6.3): the override is recorded, the first two gate failures settle
`Closed{Retry}` on the one rung, and the exhausted attempt settles `Parked` with a **fresh**
`Unblock` question:

    G4C3 after: repair=Some(AwaitingInput) alpha=Some(AwaitingRepair) rung=Some(0)
      attempts_on_rung=Some(3) generations=Some(["g0:Closed:attempts1", "g1:Closed:attempts1",
      "g2:Closed:attempts1"]) open_questions=[("g4r3-q-2", 2, Unblock)] override=true answers=2
    G4C3 replay_twice_equal=true events=25 outcome=Ending(Parked)

That is the claimed behaviour, executed at the loop; it lives only in the gate's scratch files.

**Why the committed suite cannot see it, unchanged in this range**: the driven recover fixture's
question-id source (`FixedIds`, `src/engine/topology/recover/tests.rs`) answers every
`question_id()` with `q-park-fixed`, and a run that has consumed that id for the repair's admission
question cannot raise a second one — the fold refuses the exhausted attempt's `Parked` settlement as
a reused question identity. A `SequentialIds` source of a dozen lines removes the limit; this run
wrote one for its measurement, as run 2 did.

First filed by G4's first run at `74da2cbb` on the superseded branch `gate/g4`, re-filed by its
second run at `81ee09ef` on `gate/g4-corrected-range`, and carried here with the observation made
again at `7e0110a1` and the witness still uncommitted.

## What the change that takes this up should do

Commit a counting id source for the driven fixture (or take a prefix), then commit the `G4C3`
shape: an empty-intersection repair, the binding answer, the gate failing `attempts_per` times, and
the assertion that the settlement is `Parked` with a fresh question rather than `Failed`. That kills
M34 and removes the one-question fixture limit for any later test that needs two questions in one
run.
