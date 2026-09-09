---
id: G3-O3-INTEGRATE-SHAPE-TABLE-SHORT-BY-TWO
severity: P3
disposition: deferred
category: correctness
pr: 8
reviewed_sha:
location: src/engine/topology/integrate/tests.rs:721
provenance: pre_existing
first_bad:
guard: the round that extends the array to the contract's eight shapes, or renames the test to its actual domain
---

## Failure sequence

`EVERY_INTEGRATE_SHAPE` is declared `[Shape; 7]` at `src/engine/topology/integrate/tests.rs:721` and
iterated at `:733`. The design packet's required terminal-shape table has eight rows.

Coverage is not the gap: declined-after-park and interrupted are proven by tests elsewhere, and G3
resolved this in its §6. The gap is durability. The table's own iteration does not reach those two
shapes, so a future regression in either would not be caught by the test that calls itself the whole
table — the same shape as O2, a measurement whose name over-promises its domain.

## What the change that takes this up should do

Extend the array to the contract's eight shapes so the iteration is the census it appears to be, or
rename it to say which subset it covers. Prefer the first: the two missing shapes have tests already,
so the work is wiring rather than new coverage.

Raised as observation O3 of the G3 cumulative review gate, `reviews/2026-09-08-gate-G3.md`, with the
suggested disposition "a ledger row". No row was filed at the time; this is that row.
