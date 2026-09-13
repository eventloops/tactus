---
id: HTML-COMMENT-FENCE-SWALLOWS-VERDICT
severity: P1
disposition: deferred
category: security-trust
pr: 232
reviewed_sha: ecbd3784a182b7599a1dec2beed71850a7532880
location: scripts/pr-review-parse.py:480
provenance: pre_existing
first_bad:
guard: project owner
---

## Failure sequence

`unresolved_material` checks only the text that lies **outside** the scanner's own blocks. So if the
scanner wrongly consumes a real verdict **as another block's content**, fence accounting and
candidate counting are defeated at the same time, by the same mistake.

Reproduced (executed) by the `gpt-6-astra` max pass on
`ecbd3784a182b7599a1dec2beed71850a7532880`: before a normal blocking JSON block, insert an **HTML
comment containing an opening `text` fence**, closed before the JSON block. The scanner opens that
hidden fence and swallows the blocking object through its closing fence. Append a clean `PASS`
block and it is the only candidate. Spell the blocking severity `P1` and the stray scan catches
nothing. Result: **READY, exit 0, one mocked merge call.**

`markdown-it-py` — a real CommonMark implementation — sees only the blocking verdict, per
[CommonMark's HTML-block rules](https://spec.commonmark.org/0.31.2/#html-blocks). The scanner is an
approximation of CommonMark, and an approximation has now been broken in three consecutive rounds,
each time by a construct nobody had listed.

## Why this does not block the merge

Owner ruling, 2026-09-11: *a P1 blocks a merge only if it can happen in normal use, or someone
without push access can trigger it.*

Same reasoning as `PROSE-CANDIDATE-ZERO-PERMITS-PROSE`: the crafted structure must appear in the
**trusted reviewer's own comment**, and a person without push access cannot post as that account.
Real review comments carry exactly one HTML comment — the canonical
`<!-- upstroke-frontier-review … -->` marker. The fork-contributor quoting path named in that
finding applies here too and is not treated as a direct trigger.

## What the change that takes this up should do

Owner, as the ledger records it: project owner.

Stop teaching the scanner HTML-block rules and make the ambiguity refuse instead: **allow exactly one
HTML comment — the canonical marker, matched strictly — and refuse on any other `<!--` anywhere in
the comment.** Real reviews carry exactly that one marker, so the rule costs nothing and ends this
whole family rather than the instance.

If a real CommonMark parser is available **to the gate and to CI as well as to this box**, using it
is better than any rule; check availability before taking the dependency, because the audit must run
wherever CI runs.

## Related

Sibling of [[PROSE-CANDIDATE-ZERO-PERMITS-PROSE]] and [[JSON-DUPLICATE-KEY-ERASES-FINDINGS]]: all
three are ways a review comment's own structure can hide a blocking verdict from the audit.
