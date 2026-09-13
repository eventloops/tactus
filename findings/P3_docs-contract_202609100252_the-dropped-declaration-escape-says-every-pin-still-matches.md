---
id: PR258-DROPPED-DECLARATION-ESCAPE-EVERY-PIN-FALSE
severity: P3
disposition: deferred
category: docs-contract
pr: 258
reviewed_sha: f6fd7fc0faa7e79618da922270ebe666f6100a5f
location: src/effects/tests/workflow.rs:1707
provenance: fix_regression
first_bad: PR258-DECLARATION-ESCAPE-NARRATES-THE-WRONG-LOSS — the round-7 rewrite of that escape text added "while every pin still matches"; the line is an added line of the 4cc732d9..f6fd7fc0 diff
guard: the refusal is right — `step_env_complaints` compares the step's parsed map with `TEST_STEP_ENV` and complains as `test-step-env` when they differ, which is what the escape's `refused_as` names; the change that takes this up deletes the clause or names the one pin that fails
---

## Failure sequence

The escape text of `MUT-TEST-CASEFOLD-DECLARATION-DROPPED` ends "-- while every pin still
matches". The mutation deletes the hosted test step's whole `env:` block (its `anchor` is the
two-line block and its `replacement` is empty) and is `refused_as: "test-step-env"`.

    the `env:` block is deleted from the hosted test step
    -> `field(step, "env").and_then(scalar_map)` finds no map: the parsed map is `None`
       (`step_env_complaints`, src/effects/tests/workflow.rs:746)
    -> `None` is not `Some(TEST_STEP_ENV)`, so the function returns the `test-step-env`
       complaint — the refusal the escape names
    -> the `TEST_STEP_ENV` pin does not match; "every pin still matches" is false of the one
       pin the mutation exists to trip

Reasoned from the code, as the review says; not executed. The universal is a narration defect
only: the refusal, the anchor and the replacement are unchanged, and the escape still measures
what it measured. Round 7 wrote the clause while correcting the escape's stated loss
(`PR258-DECLARATION-ESCAPE-NARRATES-THE-WRONG-LOSS`), so the review marks item 5 of that round
"newly wrong": the substantive correction holds and the added clause is false. Finding 3 of the
`ultra` review of `f6fd7fc0`, posted on the pull request as the SHA-bound comment.

## Why it is deferred

Reasoned-only, with no failing test, reproduction or mutation witness; the refusal it
mis-describes is correct and unchanged. It bears on no Gate 4 pass-rule clause and no row-10
evidence, so the owner's rule files it and merges.

## What the change that takes this up should do

Delete "-- while every pin still matches", or replace it with what is true: every other pin
still matches, and `TEST_STEP_ENV` is the one that refuses. No new test: the refusal is already
what `MUT-TEST-CASEFOLD-DECLARATION-DROPPED` measures.
