---
id: G4-O2-OVERRIDE-SINGLE-RUNG-EXHAUSTION-UNWITNESSED
severity: P3
disposition: deferred
category: correctness
pr: 249
reviewed_sha: 74da2cbbd24c55f7aed7f3593162981a11720f79
location: src/engine/topology/run.rs:1610
provenance: introduced_by_feature
first_bad:
guard: the round that gives the driven recover fixture a counting question-id source and drives an overridden repair through `attempts_per` failures to the human rung
---

## Failure sequence

`ladder_policy` in `src/engine/topology/run.rs` gives a task with a recorded one-off binding a ladder
of exactly one rung; without the override an empty-intersection repair's frozen ladder has zero rungs,
which `next_step` reads as `Fail` on the first failure. The packet states the consequence — *"a
single-rung ladder with the kind's attempts_per; exhaustion escalates to the human rung"* — and lists
no proof test for it, and none exists.

    G4 mutation M34: `rungs: if binding_override(key).is_some() { 1 } else { rungs.len() }`
      replaced by `rungs: entry.ladder.rungs.len()`
    -> the 79-test G4 detection set passes; the full library suite passes (2445 passed, 0 failed)
    -> at the ladder, measured directly: with `rungs = 1` the second failure yields `AskHuman`,
       with `rungs = 0` the first failure yields `Fail`; so under the mutation an overridden repair
       that fails once is Failed, and its lineage with it, where the design says a person is asked

Why nothing catches it, and why the obvious test cannot yet be written: the driven recover fixture's
question-id source (`FixedIds` at `src/engine/topology/recover/tests.rs:6371`) answers every
`question_id()` with `q-park-fixed`. A run that has already consumed that id for the repair's admission
question cannot raise a second one — the fold refuses the exhausted attempt's `Parked` settlement as a
reused question identity (`a question is asked once`). G4 drove the case (`G4C3`) and observed exactly
that refusal at the step that would have parked, which shows the human rung was reached and prevents
seeing it.

Not a behaviour defect at this sha: the arm is a two-line conditional, read, and the rule it feeds is
measured at the ladder. It is a guard with no loop-level witness.

## What the change that takes this up should do

Give the driven fixture an id source that counts (or takes a prefix), then drive an empty-intersection
repair: answer the binding question, fail the gate `attempts_per` times, and assert the settlement is
`Parked` with a fresh question rather than `Failed`; then assert that with the override arm removed the
same drive settles `Failed`. That kills M34 and also removes the fixture limit for any later test that
needs two questions in one run.
