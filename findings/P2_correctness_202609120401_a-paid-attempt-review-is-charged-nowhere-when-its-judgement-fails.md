---
id: ATTEMPT-PAID-REVIEW-CHARGED-NOWHERE-WHEN-THE-JUDGEMENT-FAILS
severity: P2
disposition: deferred
category: correctness
pr: TBD
reviewed_sha: 7da321c02cd9d8177174abed7633d2e0520c8276
location: src/engine/topology/run.rs:1320
provenance: pre_existing
first_bad: PR8-R2-SPEND-REPLAY — the same failure direction on the attempt path rather than the integration terminal
guard: the change that gives the attempt path a live review account, or that records a returned pass on `attempt_interrupted`; it needs a second Class C field and the branch `fix-P1/correctness_the-unavailable-terminal-carries-no-spend` deliberately did not take it
---

## Failure sequence

A schema-4 task attempt judges with `NoReviewAccount` (`src/engine/topology/attempt.rs:118`),
which charges nothing: an attempt's review spend reaches `Spend` only through the `AttemptRecord`
its settlement writes, read back by `Spend::replay` off `attempt_finished` or `candidate_prepared`.

    a two-reviewer attempt runs
    -> the first pass returns, reporting 2.50
    -> the second pass's snapshot creation, its invocation-ledger step, or its adapter lookup
       fails, so `Judge::judge` returns `Err(JudgeError::Other(..))`
    -> `TopologyRun::produce`'s `cx.judge(...)?` propagates it and the step ends the command; no
       settlement is written for that attempt, so the `Judgement` holding the pass is dropped
    -> the resume settles the attempt `attempt_interrupted`, whose payload carries no record and
       no cost (`src/topology/events.rs:795`), and whose detail says "the spend is unknown and
       nothing was judged" where a pass was judged and its cost is known
    -> the 2.50 is in no total, live or replayed, for the rest of the run's life, and every later
       admission is made against a ceiling that has not paid for it

The direction is overspending and it is permanent: unlike the in-flight overshoot DESIGN §26's
budget paragraph states an honest bound for, this cost is reported, known, and then discarded.

Established by reading the three steps above at this SHA, not by execution: no test exercises an
attempt whose judgement fails after a paid pass. The integration path's equivalent *is* exercised,
by `a_completed_integration_review_is_charged_when_the_next_reviewers_snapshot_fails`, and that
path is closed — the account it charges keeps the record, and the unavailable terminal carries it.

## What the change that takes this up should do

Two halves, and they are not the same decision.

Within an incarnation, give the attempt path a live account as the integration path has one:
`Judge::judge` already takes `&mut dyn ReviewAccount` and already hands it each record as it is
built, so the change is a `SpendAccount` at `TopologyRun::produce`'s call site instead of
`NoReviewAccount`, plus care that the settlement does not then charge the same passes twice —
`Spend::record` sums an `AttemptRecord`'s worker and review costs together, so the live charge and
the settlement charge would have to agree about which of them owns the reviews.

Across a restart, the record has to be durable, and `attempt_interrupted` cannot carry it: that is
a second Class C field on the frozen vocabulary, in the same shape
`merge_verification_unavailable.reviews` now has. It deserves its own pull request for the same
reason that one did. `AttemptOutcome::Interrupted`'s detail should stop claiming the spend is
unknown when a returned pass is recorded beside it.
