---
id: PROSE-CANDIDATE-ZERO-PERMITS-PROSE
severity: P1
disposition: deferred
category: security-trust
pr: 232
reviewed_sha: ecbd3784a182b7599a1dec2beed71850a7532880
location: scripts/pr-review-parse.py:501
provenance: fix_regression
first_bad: ecbd3784a182b7599a1dec2beed71850a7532880
guard: project owner
---

## Failure sequence

`verdict_candidates` excludes bare verdict objects that sit inside a recognised block, so the
candidate count becomes **zero**, and `review_result` (`scripts/pr-review-parse.py:660`) then
**permits prose parsing** rather than refusing.

Reproduced (executed) by the `gpt-6-astra` max pass on
`ecbd3784a182b7599a1dec2beed71850a7532880`: supply the prose marker with the current head, a
**`text`-fenced** object beginning `"role_understanding"` that contains `CHANGES_REQUIRED` and
`"severity":"P1"`, and finish with `VERDICT: PASS`. The candidate count is zero, the prose
parser takes the `PASS`, and the escaped severity is invisible to the stray scan because that scan
runs over raw text while the severity only exists after JSON decoding.

The identical witness gives **NOT-READY, zero merge calls at `91017f40`** and **READY, one mocked
merge call at this head**. It is a widening introduced by the repair at this head.

## Why this does not block the merge

Owner ruling, 2026-09-11: *a P1 blocks a merge only if it can happen in normal use, or someone
without push access can trigger it.*

The audit reads review comments **only from the account named by `--reviewer` or
`UPSTROKE_REVIEW_AUTHOR`**. To reach this path, that trusted account's own comment must carry a
fenced verdict object whose severity is escaped **and** a contradicting `VERDICT: PASS` — a
self-contradictory review. Measured on this repository: all 327 review comments, and all 103 in the
JSON form, carry exactly one block with nothing after it. No such comment exists or is produced by
the review harness.

**The reachability worth naming.** A contributor with no push access can open a pull request from a
fork, and the reviewer may quote their file content verbatim into its review. That is the one path
by which attacker-controlled text could reach the trusted comment. It still requires the reviewer to
both reproduce the text and return `PASS`. That is not normal use, and it is not a direct external
trigger, so this is filed rather than blocking — but it is written down here so the judgement is
visible and can be revisited.

## What the change that takes this up should do

Owner, as the ledger records it: project owner.

Two things, together. **Run the stray severity scan over decoded content**, so `P1`, a
surrogate-pair spelling and any other encoding of `P1` are seen by the scan that exists to catch
them — this single change disarms most of the witnesses in this family. And **make a zero candidate
count refuse**: a comment carrying anything shaped like a JSON verdict object, in any fence, any
language tag, any encoding, is ambiguous and is not a prose review. Ambiguity must refuse rather than
fall back to the parser that approves.

A partial implementation exists on the local branch `rescue/232-r14-partial` (`747c7cd9`,
"read the severity the parser reads, and refuse a second verdict wherever it is"), written against
this head and never reviewed. Read it before starting.
