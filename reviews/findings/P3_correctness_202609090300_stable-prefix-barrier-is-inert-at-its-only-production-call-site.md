---
id: G3-O1-BARRIER-INERT-AT-CALL-SITE
severity: P3
disposition: deferred
category: correctness
pr: 8
reviewed_sha:
location: src/engine/topology/recover.rs:481
provenance: pre_existing
first_bad:
guard: the round that gives the call site real reread, replay and sync observations, or the sentence saying it has none
---

## Failure sequence

`src/engine/topology/recover.rs:481`, inside `BarrierHeld::from`, is the only production call site of
`StablePrefixBarrier::establish`. It is handed `PrefixReread { first: m.clone(), second: m.clone() }`,
`PrefixReplay { replayed: m }` and `PrefixSync { synced_len: m.len }` — the same metadata three times.
Each of the validator's four refusals (boundary moved, digest changed, sync shorter than the prefix,
replay over other bytes) is therefore vacuously satisfied, and the call cannot fail. Every other call
site is a unit test of the validator in `src/runner/container/census/tests.rs`.

**The guarantee itself holds and this is not a defect.** The barrier's substance is enforced upstream
in `events/log.rs::establish_stable_prefix`, and G3's mutation M17 proves that detects: removing its
byte-equality check kills
`an_unstable_reread_refuses_naming_prove_prefix_stable_and_hands_out_no_handle`.

What does not hold is the impression the call site gives. A reader at `recover.rs:481` sees a barrier
being established and reasonably concludes that this is where the stable-prefix property is enforced.
It is not. This is the "redundant assertion defeats its own mutation" shape: defence in depth that is
inert exactly where it reads as load-bearing, so a later change that weakened the upstream proof would
leave this call site looking like a second line of defence that it is not.

## What the change that takes this up should do

Either hand the call site the observations it is nominally validating — the actual first and second
rereads, the actually replayed bytes, the actual synced length — so the four refusals become
reachable, or add one sentence at the call site stating that the proof is upstream in
`establish_stable_prefix` and that this call is a token. The second is cheap and honest; the first is
the real fix and is worth more.

Raised as observation O1 of the G3 cumulative review gate,
`reviews/2026-09-08-gate-G3.md`, which recorded it with the suggested disposition "a tech-debt ledger
row, plus one sentence at the call site". No row was filed at the time; this is that row.
