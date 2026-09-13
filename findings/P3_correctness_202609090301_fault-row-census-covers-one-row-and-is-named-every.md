---
id: G3-O2-FAULT-ROW-CENSUS-COVERS-ONE-ROW
severity: P3
disposition: deferred
category: correctness
pr: 8
reviewed_sha:
location: src/effects/tests/contract_mappings.rs:54
provenance: pre_existing
first_bad:
guard: the round that transcribes the remaining fault-row arrays, or renames the census to the row it covers
---

## Failure sequence

`every_fault_row_name_is_a_test_in_the_tree` is a good census as far as it goes: it requires each
named test to exist as a `#[test]`, to be defined in exactly one file, and it carries a positive
control against a name that cannot exist.

Its domain is the single const array `T_CONTAINER_TESTS` — the T-CONTAINER row alone. `FaultRow`
carries all 21 rows, and grep finds no `T_FAST`, `T_PROPOSAL`, `T_VERIFY`, `T_PREPARED` or `T_REJECT`
transcription anywhere in `src/`. So a test named by any row other than T-CONTAINER can be renamed or
deleted and nothing mechanically notices.

The name is the problem as much as the domain. A census called `every_fault_row_name_...` is read as
covering every fault row, so the gap is invisible to anyone who does not open it. For G3's own five
rows the obligation was discharged by hand — 24 of 26 slugs located, the 2 absent for a stated reason
— which is a review duty being met, not a measurement.

## What the change that takes this up should do

Transcribe the remaining row arrays so the census's domain matches its name, or rename it to
`every_t_container_fault_row_name_is_a_test_in_the_tree` so the gap is stated rather than implied. The
first is the fix; the second is the honest minimum and should not be skipped if the first is deferred
again.

Raised as observation O2 of the G3 cumulative review gate, `reviews/2026-09-08-gate-G3.md`, with the
suggested disposition "a ledger row proposing the five arrays, or a rename of the test to say which
row it covers". No row was filed at the time; this is that row.
