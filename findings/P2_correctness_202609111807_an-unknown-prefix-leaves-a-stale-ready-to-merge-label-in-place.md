---
id: PR269-R2-UNKNOWN-PREFIX-LEAVES-READY-LABEL
severity: P2
disposition: deferred
category: correctness
pr: 269
reviewed_sha: 08c80be61e56e98700fb34ccfae8ed8a5bc30e28
location: scripts/pr-ready-audit.sh:882
provenance: introduced_by_feature
first_bad: 08c80be61e56e98700fb34ccfae8ed8a5bc30e28
guard: gpt-6-astra/max review round 2 of PR #269
---

## Failure sequence

A pull request on a prefix outside the vocabulary is refused a lane, which is correct and is the
point of the table. The refusal returns **before label reconciliation**, so whatever labels the
pull request already carries are left exactly as they were.

Give such a pull request an existing `ready-to-merge` and `lane:feature`, then run with `--apply`.
The audit prints `NOT-READY`, exits **0**, and makes **no label edits**. The pull request keeps a
`ready-to-merge` label that now advertises a readiness the audit has just declined to assert, and
a `lane:feature` naming a lane it does not have.

`ready-to-merge` is advisory and never permission — `scripts/pr-ready-audit.sh` and
`MAINTAINING.md` both say the enqueue is the act bound to the head, not the label. That is what
holds the severity at P2 rather than higher. It is still a label saying the opposite of the
audit's own verdict, left there by the audit itself.

## What the change that takes this up should do

The metadata was read successfully, so the refusal can still reconcile: remove `ready-to-merge`
and any `lane:*` label before returning, then report the unknown prefix. The lane cannot be
decided; that a stale readiness label must go is not in doubt.
