---
id: PR3-REG-001-CONDITIONAL
severity: P3
disposition: deferred
category: correctness
pr: 3
reviewed_sha: 7dfbf8b0cb1203671300a097ca9be1c8107732a3
location: src/topology/effects.rs:338
provenance: undetermined
first_bad:
guard: the change that gives any site a second observable order (`EffectSiteId::observable_orders` returns one or none by construction; re-measured at PR10)
---

## Failure sequence

`A3-REG-001` is equivalent *for the current inventory*, because every constructible site exposes zero or one observable order

## What the change that takes this up should do

Owner, as the ledger records it: PR4-PR10 implementer.

It becomes live debt the moment any site exposes more than one observable order. Conditional debt, not closed

Carried in `reviews/FINDINGS.md` §2, “Open — carried deliberately, with an owner”, and confirmed still carried by the full-ledger audit of 2026-08-31 (§39). The row carried no severity label; **P3** here is this migration's judgement from the consequence described above, not the reviewer's own word.

**Re-checked at PR10, still conditional.** Every constructible site still exposes zero or one observable order (`EffectSiteId::observable_orders`, derived from `Adjacent`), and PR10's sequential registry document keys its entries the same way. The guard passes from the PR4–PR10 implementer to whichever change first gives a site two orders.
