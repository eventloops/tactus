---
id: PR269-R2-GH-CSV-LABEL-PARSE
severity: P2
disposition: deferred
category: correctness
pr: 269
reviewed_sha: 08c80be61e56e98700fb34ccfae8ed8a5bc30e28
location: scripts/pr-ready-audit.sh:1216
provenance: introduced_by_feature
first_bad: 08c80be61e56e98700fb34ccfae8ed8a5bc30e28
guard: gpt-6-astra/max review round 2 of PR #269
---

## Failure sequence

Round 1's `PR269-005` was that label reconciliation word-split label names. The repair passes each
label as **one shell argument**, which closes the shell layer — but `gh` declares
`--remove-label` as a **CSV-parsed string slice**, so there is a second parser underneath the one
the repair fixed.

A label named `lane:legacy "docs"` makes `gh` fail with `bare " in non-quoted-field`. Running
`--apply 999 1000` against mocked GitHub responses and the **installed CLI's real argument
parser** exits **1** after #999, and #1000 is never audited — the same "one bad label stops the
whole run" failure the round-1 repair was meant to end, reached through the layer below it. A comma
in a label name splits it likewise.

The fixture added in the repair checks **shell** argument boundaries and therefore cannot see
this: it stubs `gh` and never exercises the CSV parsing the real CLI does.

## What the change that takes this up should do

Either call an API that takes label **IDs** rather than names, or encode the name as a CSV field
`gh` will parse back intact. Whichever is chosen, the fixture has to exercise the real parser
rather than a stub that stops at `argv`, because the stub is what let this survive the first
repair.
