---
id: G4R3-Q3-UNDERSTATES-THE-SETTLEMENT-REFUSAL
severity: P3
disposition: deferred
category: docs-contract
pr: 260
reviewed_sha: 0a82b2dde6cf23a08bde4f955a10459ca76c6723
location: reviews/2026-09-10-gate-G4.md:1812
provenance: introduced_by_feature
first_bad: none — the claim appears in run 3's own Q3
guard: none needed; the test itself is the guard and it is stronger than the report says
---

## Failure sequence

Q3 says `retry_refused_with_stale_incarnation` "does two things" and concludes that it is
"**a refusal of the retry, not of a settlement**". The test's final branch
(`src/engine/topology/settle/tests.rs:1240`) also constructs an `AttemptFinished` settlement carrying
`retained_incarnation: Epoch(9)` and observes its refusal.

So the report **understates its own witness**: the named test checks a settlement refusal that Q3
explicitly excludes.

## Why this is deferred rather than repaired

This is an **under**-claim. The report credits its evidence with less than the evidence does, which
cannot make a clause pass on absent support — the direction of error that matters for a gate is the
opposite one. Raised by the seventh verification at `0a82b2dd`, which kept all six pass-rule clauses
`Established`.

Documented and merged under the owner's rule of 2026-09-09. See
[[G4R3-G4H-PUBLISHED-ANSWER-HANDOFF-NOT-EXECUTED]].

## What closing it would look like

One sentence in Q3: the test refuses the stale-incarnation retry **and** the `AttemptFinished`
settlement carrying `retained_incarnation: Epoch(9)`, so Q3's identity evidence is broader than
stated.
