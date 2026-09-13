---
id: PR3-FRAMEWORK-SILENT-2
severity: P3
disposition: deferred
category: docs-contract
pr: 3
reviewed_sha: 7dfbf8b0cb1203671300a097ca9be1c8107732a3
location: src/topology/effects/residue_authority.rs:288
provenance: undetermined
first_bad:
guard: the owner — a design section stating the residue authority's derived rules (`DESIGN.md` §26 is the owner's; re-checked at PR10)
---

## Failure sequence

Read-only sites' After phase leaves nothing

## What the change that takes this up should do

Owner, as the ledger records it: PR7–PR10 implementer.

Derived from the packet's "performs no effect", not stated by it

Carried in `reviews/FINDINGS.md` §2, “Open — carried deliberately, with an owner”, and confirmed still carried by the full-ledger audit of 2026-08-31 (§39). The row carried no severity label; **P3** here is this migration's judgement from the consequence described above, not the reviewer's own word.

**Re-checked at PR10.** The residue authority (`src/topology/effects/residue_authority.rs`) still states this derivation as code and as a test, and the packet still does not; PR10's registry document enters the run-end sites through that authority unchanged (`EffectSiteId::semantics`). What would close this is a design sentence stating the rule, and `DESIGN.md` is the owner's, so the guard passes from the PR7–PR10 implementer to the owner.
