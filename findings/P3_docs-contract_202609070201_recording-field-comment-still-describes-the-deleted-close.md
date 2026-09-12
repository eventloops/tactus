---
id: SWEEP-EFFECTS-HARNESS-001-REVIEW-01
severity: P3
disposition: deferred
category: docs-contract
pr: 244
reviewed_sha: 2ada369cd6fc4340beb113877d1c338a2b022630
location: src/topology/effects/harness.rs:141
provenance: fix_regression
first_bad: SWEEP-EFFECTS-HARNESS-001
guard: gpt-5.6-sol/medium review round 1 of PR #244
---

## Failure sequence

Location as first recorded: src/topology/effects/harness.rs:141 (as of `2ada369c`, PR #244)

Fixing `SWEEP-EFFECTS-HARNESS-001` deleted the `self.end_fast_sequence();` call from
`begin_fast_sequence`, which was a no-op. `begin_fast_sequence` now performs no preceding close: it
pushes the new `FastSequence` first, then sets `recording = true` unconditionally two statements
later.

The `recording` field's doc comment was not updated with the call it explained. It still says (line
141-143 as of the reviewed sha):

> The open sequence is always the last of `fast`: `begin_fast_sequence` closes the previous one and
> then pushes, and nothing else adds to the vector.

That describes the pre-fix operation order (explicit close, then push) which this diff removed. The
corrected explanation on `begin_fast_sequence` itself (lines 318-320 as of the reviewed sha) already
states the accurate mechanism — push first, unconditional reassignment second — so the two comments
now disagree about the order of operations on the same method.

Call: `SWEEP-EFFECTS-HARNESS-001-REVIEW-01`, `gpt-5.6-sol`/medium, review round 1 of PR #244,
verdict `CHANGES_REQUIRED` (P4 only; ledger recorded here per P3-workflow rule for a review that
produced no P0-P3 finding).

## What the change that takes this up should do

Update the `recording` field's doc comment to match the corrected mechanism: the previous sequence
stops being open because a new one is pushed as the last element of `fast`, not because it was
explicitly closed first; `recording` is then set unconditionally.

Deferred rather than fixed in PR #244 itself: this queue's P4 carve-out logs an observation without
spending a further review pass on a change that is otherwise complete and green.
