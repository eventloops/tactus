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
counting one (gate report §6.3), read off the trace event by event rather than summarised:

    #14 question_answered k2 q=q-park-fixed  answer carries binding_override{claude-code/…/High}
    #15-#17 g0 attempt 1 -> attempt_finished failure=GateFailed
            settlement=Closed { transition: Retry, lease: LineageHeld }
    #18-#20 g1 attempt 1 -> attempt_finished failure=GateFailed
            settlement=Closed { transition: Parked { question g4r3-q-1, kind Unblock,
              context "2 attempt(s) across 1 rung(s) all failed, and the escalation chain is spent" },
              lease: LineageHeld }
    #21     question_answered k2 q=g4r3-q-1  (option 0, "retry this task"), no binding override
    #22-#24 g2 attempt 1 -> attempt_finished failure=GateFailed
            settlement=Closed { transition: Parked { question g4r3-q-2, "3 attempt(s) …" },
              lease: LineageHeld }
    after: repair=AwaitingInput rung=Some(0) attempts_on_rung=Some(3)
      generations=["g0:Closed:attempts1", "g1:Closed:attempts1", "g2:Closed:attempts1"]
      open_questions=[("g4r3-q-2", 2, Unblock)] override=true answers=2 outcome=Ending(Parked)

**So the escalation happens at the *second* failure, not the third, and a person authorises what
follows it.** With `attempts_per = 2` on the one rung, the first failure settles `Closed{Retry}` and
the second exhausts the rung and settles `Parked` with a fresh `Unblock` question — the human rung.
The third attempt exists only because `g4r3-q-1` was answered at `#21`; its failure parks again on
`g4r3-q-2`, which is still open at the end. An earlier version of this file said "the first two gate
failures settle `Closed{Retry}` … and the exhausted attempt settles `Parked`", which is one failure
out and omits the intervening authorisation; corrected here against
`measurements/by-tag/G4C3.txt`.

That is the claimed behaviour — a single-rung ladder exhausting to the human rung rather than to
`Failed` — executed at the loop; it lives only in the gate's scratch files.

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
