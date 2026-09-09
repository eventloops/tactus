---
id: G3-O6-PR8-PLAN-AT-ROOT-CONTRADICTS-R12
severity: P3
disposition: deferred
category: docs-contract
pr: 8
reviewed_sha:
location: pr8-plan.md:207
provenance: pre_existing
first_bad:
guard: the round that moves the pr8-* working files off master, which removes the contradiction with them
---

## Failure sequence

`pr8-plan.md`, `pr8-body.md` and `pr8-triage.md` are tracked at the repository root. They are a
slice's working files, not a record the project keeps, and `pr8-plan.md` is roughly 100 KB of it.

The contract problem is that it is not merely surplus: its R12 paragraph (`pr8-plan.md:207`) describes
verification-park question behaviour that the code does not have, and `:118`'s R1 paragraph quotes R12
in turn. A tracked file at the repository root reads as current, so a reader who finds it before
finding `DESIGN.md` is told something false about the tree — and `DESIGN.md` is the sole living
authority precisely so that there is one place to look.

## What the change that takes this up should do

Move the three files off `master`. That removes the contradiction along with them and needs no
judgement about which paragraphs are stale, which is why it is preferable to correcting the prose in
place. Pull request #248 exists to do this and has not landed.

If they are kept for any reason, R12 and the R1 paragraph that quotes it must be corrected against the
code, and the files need something at the top saying they are a historical working record rather than
a current description.

Raised as observation O6 of the G3 cumulative review gate, `reviews/2026-09-08-gate-G3.md`. Still true
at `origin/master` on 2026-09-09, verified with `git ls-tree`.
