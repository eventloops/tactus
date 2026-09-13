---
id: PR277-HELPER-END-SITE-COUNT-SAYS-THREE-AND-THERE-ARE-FOUR
severity: P3
disposition: deferred
category: documentation
pr: 277
reviewed_sha: a6411c63b0fe516df28fb282d53245ce20cd3e6b
location: docs/internals/agent/proc.md:1280
provenance: introduced
first_bad: a6411c63b0fe516df28fb282d53245ce20cd3e6b
guard: deferred: the sentence counting the sites that report through HelperEnd must name four, not three, and whoever edits it should re-derive the count from the signatures rather than trusting the prose
---

## Failure sequence

`docs/internals/agent/proc.md:1280` states that, of the five sites
`PR125-CLOSE-UNBOUNDED-KILL-AND-WAIT-AT-FIVE-SITES` names, **"three report through `HelperEnd`"**.
**There are four.** They are:

1. `Reaper::abandon`, through `close_and_wait_reporting`
2. `Guard::abort_setup`
3. the descriptor-configuration failure in `spawn_guard`
4. the READY failure in `spawn_guard`

`git grep -c -- '-> HelperEnd'` over `src/agent/proc.rs` at `a6411c63` returns **4**, and the four
names above are the functions carrying that return type. The fifth site of the row,
`spawn_reaper`'s parent-side `setpgid(pid, pid)`, no longer exists to fail — which the same
paragraph states correctly. So the sentence undercounts by exactly the site this pull request's own
round-4 and round-5 work added to the set.

The sentence is **new text introduced by this pull request**, not a previously-correct count that
the repair falsified — the base carries no such sentence. It is a plain miscount in prose, with no
behavioural surface: nothing compiles, executes, or gates on this file. `src/agent/proc.rs:1`
references it only as `//! Extended notes: docs/internals/agent/proc.md`, a path mention rather than
an `include_str!` pin, so no test fails on its contents.

## Why this is filed rather than fixed

Fixing it means editing a **tracked file**, which moves the head and **invalidates two PASS review
lenses over a one-character count**. Round 5's review of record — fix-check and regression, both
executed, at `a6411c63` — would no longer be a review of the merged head. Trading that property for
a numeral is precisely the trade the findings mechanism exists to avoid, so the count is recorded
here and the next change that touches this file should carry the correction.

## Repair

Change "three" to "four" in that sentence. Re-derive the count from the signatures
(`grep -- '-> HelperEnd' src/agent/proc.rs`) rather than from the surrounding prose, because the
prose is what was wrong.
