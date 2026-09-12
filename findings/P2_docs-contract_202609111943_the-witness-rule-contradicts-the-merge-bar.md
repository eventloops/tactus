---
id: WITNESS-RULE-CONTRADICTS-MERGE-BAR
severity: P2
disposition: deferred
category: docs-contract
pr:
reviewed_sha: 8d97e25c6342b07d050779ba93624135669f3455
location: MAINTAINING.md:41
provenance: pre_existing
first_bad:
guard: project owner
---

## Failure sequence

`MAINTAINING.md` step 5, lines 40 to 44, says a finding carrying a failing test, reproduction or
mutation witness is fixed whatever its severity, and may be `rejected` only by a row showing the
evidence invalid. `CLAUDE.md:98` and `findings/PROCESS.md` §7 say the same, and the checkbox at
`.github/pull_request_template.md:40` rules out `deferred` for such a finding. `scripts/pr-ready-audit.sh`
enforces it in every lane as the `witnessed:` blocker at line 1136, for findings in the JSON review
form.

The merges of 2026-09-11 did not follow it. Pull requests #232 and #251 merged with P1s filed
`deferred` whose failure sequences were executed reproductions, under the owner's ruling that a P1
blocks a merge only if it can happen in normal use or someone without push access can trigger it.
`P1_correctness_202609110907_duplicate-json-keys-erase-the-findings-array.md` is one: its failure
sequence is "Reproduced (executed)" and its disposition is `deferred`. The body of #232 leaves the
template's checkbox for this rule unticked.

A reader of the repository finds one rule and the merges follow another. A review that checks a pull
request against step 5 blocks what the ruling allows, and a merge made under the ruling leaves a
record that step 5 says cannot exist.

## What the change that takes this up should do

The owner decides which rule holds. If the 2026-09-11 bar stands, `MAINTAINING.md` step 5,
`CLAUDE.md`, `PROCESS.md` §7, the pull request template and the audit's `witnessed:` blocker change
together to say when a reproduced finding may be deferred. If step 5 stands, the P1s deferred with
reproductions on 2026-09-11 go back to being must-fix work.

The owner has directed that the "Serious P1" wording be reworked around the rules now set per branch
and per review. This rule sits in the same step and belongs in that rework, which
`SERIOUS-P1-WORDING-PREDATES-THE-LANE-AND-REVIEW-RULES` records.
