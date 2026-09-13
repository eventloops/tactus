---
id: PR258-STEP-LEVEL-MAP-COUNT-IS-THREE-NOT-TWO
severity: P3
disposition: deferred
category: docs-contract
pr: 258
reviewed_sha: f6fd7fc0faa7e79618da922270ebe666f6100a5f
location: src/effects/tests/workflow.rs:1727
provenance: fix_regression
first_bad: PR258-ENV-STEP-CARDINALITY-AND-PIN-ACCOUNTING — the round-7 repair of that P3 wrote the sentence; the "two step-level maps" line is an added line of the 4cc732d9..f6fd7fc0 diff
guard: the refusal the escape measures is unchanged and is not what is wrong; the change that takes this up rewrites the escape text of `MUT-TEST-STEP-RETARGETED-THROUGH-THE-ADMITTED-ENV` to count the suite-running maps, the way docs/internals/effects/tests/workflow.md and ci_model.md already do
---

## Failure sequence

The escape text of `MUT-TEST-STEP-RETARGETED-THROUGH-THE-ADMITTED-ENV` calls the hosted test
step's `env:` "one of the two step-level maps this contract admits".

    .github/workflows/ci.yml at f6fd7fc0 carries three step-level `env:` maps
    -> line 88, the hosted test step's, pinned whole by `TEST_STEP_ENV`
    -> line 133, the Windows test step's, pinned whole by `TEST_WINDOWS_STEP_ENV`
    -> line 249, the merge-gate aggregate's "Require every gate" step's, six `*_RESULT`
       keys, which the oracle pins as well (`AGGREGATE_STEP_FIELDS` admits `env`, and the
       ci_model notes say its mapping is pinned key by key)
    -> "two step-level maps" counts the first two and omits the third; "the two
       suite-running steps" (docs/internals/effects/tests/workflow.md:481,
       ci_model.md:281 and :330) is the accurate count, because the aggregate runs no suite

Established by reading the workflow, the pin constants and the notes at this sha; no execution
is involved and none is needed. The phrase occurs once in the tree. Round 7 wrote it while
correcting the previous count — `PR258-ENV-STEP-CARDINALITY-AND-PIN-ACCOUNTING` had faulted
"the one step this contract allows an `env:`" — so the review marks item 7 of that round
"newly wrong": the corrected text introduces a false count of its own. Finding 2 of the `ultra`
review of `f6fd7fc0`, posted on the pull request as the SHA-bound comment.

## Why it is deferred

A descriptive string in a test's escape catalogue; the refusal the escape measures
(`test-step-env`) is unchanged and correct. Reasoned-only in the review's terms — no failing
test, reproduction or mutation witness — bearing on no Gate 4 pass-rule clause and no row-10
evidence, so the owner's rule files it and merges.

## What the change that takes this up should do

Replace "one of the two step-level maps this contract admits" with "one of the two
suite-running maps", or count all three and say which two run a suite. Read the notes' sentences
first and match their count rather than write a fourth.
