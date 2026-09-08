---
id: PR8-R2-SPEND-REPLAY
severity: P1
disposition: deferred
category: correctness
pr: 8
reviewed_sha: 916852c9638383af3964f743d48e14bf859138c3
location: src/engine/topology/select.rs:52
provenance: introduced_by_feature
first_bad: 2d1b4c72
guard: the project owner — a Class C vocabulary decision, or an erratum
---

## Failure sequence

An integration verification's reviews are charged to the run's `Spend` when they are judged. A
verification that ends in `merge_prepared` or `merge_rejected` replays its costs, because those
records carry the review passes. A verification that ends in `merge_verification_unavailable` —
parked for a person, or deferred by an infrastructure outage — does not: the frozen terminal has
no review record to carry them.

    1.20 spent
    -> a 2.50 integration review returns needs_human
    -> live total 3.70, and the selector refuses further integration under a 3.00 ceiling
    -> restart
    -> replay restores 1.20
    -> the selector admits an integration the previous incarnation refused

The failure direction is overspending, and it is unbounded across repeated restarts: every
incarnation forgets what the parked and deferred verifications of the last one cost.

Reproduced by the `gpt-6-astra` conformance/record pass of 2026-09-07 against `916852c9`, and
independently by the repair-adequacy pass of the same round. Pinned on the branch by
`a_paid_review_that_parks_is_charged_live_and_its_replay_loss_is_the_deferred_vocabulary_gap`,
which asserts the live charge, the in-incarnation refusal, and the replayed total as exactly what
the frozen vocabulary can carry.

## Why it is deferred rather than repaired

The contract is not silent, so this is not a reading that was available to the implementer:
`decisions.coordinator_integration.dispositions` requires the spend recorded for an Infrastructure
`Deferred`. Reading R22 originally called the omission a permitted reading; that reading was
withdrawn in the second repair round.

Conforming needs a review record — or a cost field — on `MergeVerificationUnavailable`, which the
frozen event vocabulary does not have. Adding one is a **Class C wire-vocabulary change** under
the `src/topology/**` freeze: it changes what is written to `events.jsonl` and is therefore in
every log forever.

The owner's decision of 2026-09-07 was to defer rather than to make that change inside PR8. A wire
change buried in a 5,800-line slice is the shape the freeze classification exists to prevent, and
it deserves a pull request that can be reviewed on its own.

## What the change that takes this up should do

Decide, and record the decision, between:

1. **The Class C field.** Give `merge_verification_unavailable` a review record in the same shape
   the prepared and rejected terminals carry, so replay restores the spend by the same path. This
   is the conforming answer and the one the contract's present wording asks for. It needs a
   decision record and schema care for logs written before the field existed.
2. **An erratum.** Amend `decisions.coordinator_integration.dispositions` to say the unavailable
   terminal carries no spend, and state the accepted bound: a restart forgets the cost of the
   parked and deferred verifications of the incarnation it replaces. This makes the record honest
   without touching the wire, and accepts the overspend direction permanently.

Either way the ledger row and this finding close together. Do not close this by narrowing the
test: the test is correct and pins the gap deliberately.
