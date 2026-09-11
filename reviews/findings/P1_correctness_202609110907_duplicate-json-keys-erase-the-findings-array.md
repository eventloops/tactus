---
id: JSON-DUPLICATE-KEY-ERASES-FINDINGS
severity: P1
disposition: deferred
category: correctness
pr: 232
reviewed_sha: ecbd3784a182b7599a1dec2beed71850a7532880
location: scripts/pr-review-parse.py:561
provenance: pre_existing
first_bad:
guard: project owner
---

## Failure sequence

`parse_json_review` calls unrestricted `json.loads`, and JSON deserialisation
[keeps the last occurrence of a repeated name](https://docs.python.org/3/library/json.html#repeated-names-within-an-object).

Reproduced (executed) by the `gpt-6-astra` max pass on
`ecbd3784a182b7599a1dec2beed71850a7532880`: one fenced object with valid head and base fields,
`"verdict":"PASS"`, a `"findings"` array containing a CRITICAL/P1 entry, and then a second
`"findings":[]`. Deserialisation discards the blocking array **before validation ever sees it**, and
the stray scan excludes the object as a whole because it is a recognised verdict block. The audit
reaches **READY**.

This is inherited behaviour carried into the rewritten parser, and it contradicts the parser's
stated ambiguity-refusal guarantee: a document that two readers can read two ways is exactly what
that guarantee is supposed to refuse.

## Why this does not block the merge

Owner ruling, 2026-09-11: *a P1 blocks a merge only if it can happen in normal use, or someone
without push access can trigger it.*

The verdict object is emitted by the review harness from the reviewer model's output, and it carries
one `findings` key. A person without push access cannot post as the trusted reviewer account, which
is the only account whose comments the audit reads. No repeated-key object exists among the 327
review comments in this repository.

## What the change that takes this up should do

Owner, as the ledger records it: project owner.

Decode with an `object_pairs_hook` that raises on a repeated name, **recursively, including inside
`findings`**, so a document with two readings is refused rather than silently given one of them.
This is a small, self-contained change and it is worth doing on its own.
