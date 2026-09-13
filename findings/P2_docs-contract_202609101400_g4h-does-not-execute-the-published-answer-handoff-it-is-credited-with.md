---
id: G4R3-G4H-PUBLISHED-ANSWER-HANDOFF-NOT-EXECUTED
severity: P2
disposition: deferred
category: docs-contract
pr: 260
reviewed_sha: 0a82b2dde6cf23a08bde4f955a10459ca76c6723
location: reviews/2026-09-10-gate-G4.md:966
provenance: introduced_by_feature
first_bad: none — the claim appears in run 3's own §5.3 and Q6
guard: none in the tree; the claim is in the gate report, which no gate reads. The behaviour it describes is exercised at epoch 2 by the same harness, and the answer-ingestion evidence Q6 relies on elsewhere is unaffected
---

## Failure sequence

§5.3 says G4H's answer is refused in the stopped epoch "**and ingested by the next incarnation's
first step**", and Q6 repeats it. The saved harness (`harness/g4r3_meas.rs:1413`) does something
narrower:

    epoch 0: stage only a `.partial`; evaluate a forged `QuestionAnswered` against the fold
    epoch 1: resume with no published file -> the step returns `Blocked`
    then:    publish the answer
    epoch 2: resume again and ingest it

The recorded output (`measurements/by-tag/G4H.txt:2`) says plainly
`published_exists=false epoch=Some(1) budget_stop=None`.

So the construction **cannot** establish ingestion of an *already-published* answer on the **first**
resume after a budget stop. A loop defect specific to that handoff would not be caught by this
witness. Ingestion itself is executed, at epoch 2.

## Why this is deferred rather than repaired

Raised by the seventh `gpt-6-astra` `max` verification of this report, at `0a82b2dd`, which kept
**all six pass-rule clauses `Established`** — the third consecutive round in which none was
downgraded — and stated that "the evidence supports G4's six pass-rule clauses". It is a description
of a witness, not a defect in the engine and not a gap in any clause's evidence.

Under the owner's merge rule of 2026-09-09 — only P2s and P3s, none conflicting with the Gate 4 goal,
documented as findings and then merged — it is documented here and merged. See
[[G4R3-Q3-UNDERSTATES-THE-SETTLEMENT-REFUSAL]] for the round's other item.

## What closing it would look like

Either publish the answer **before** the first post-stop resume and re-run G4H, so the sentence
becomes true of what ran; or narrow §5.3 and Q6 to what the harness does — refusal in the stopped
epoch, `Blocked` on the first resume with nothing published, ingestion at the next. Do not describe
epoch 2's ingestion as the first resume's.
