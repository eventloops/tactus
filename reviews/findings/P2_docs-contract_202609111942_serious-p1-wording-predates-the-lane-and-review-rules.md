---
id: SERIOUS-P1-WORDING-PREDATES-THE-LANE-AND-REVIEW-RULES
severity: P2
disposition: deferred
category: docs-contract
pr:
reviewed_sha: 8d97e25c6342b07d050779ba93624135669f3455
location: MAINTAINING.md:81
provenance: pre_existing
first_bad:
guard: project owner
---

## Failure sequence

Three rules decide which P1 blocks a merge, and they disagree.

- `MAINTAINING.md` step 5 fixes a relevant serious P1, and the "Serious P1" section at lines 81 to 95
  defines one by what its concrete failure reaches: a `DESIGN.md` §4 invariant, the trust boundary or
  the merge and release machinery, durable state, data in a user repository, or a legal defect. The
  owner classifies, and a P1 whose failure needs speculative preconditions is reclassified down with a
  ledger row saying why.
- The lane table at lines 320 to 332, and `scripts/lane.sh:108` behind it, put every P1 in the
  must-fix set of every lane that fixes P0 to P1, whatever the P1 reaches.
- The owner's ruling of 2026-09-11 blocks a merge on a P1 only if it can happen in normal use or
  someone without push access can trigger it. Seven P1 finding files on this commit quote it, and
  `MAINTAINING.md` does not. Pull requests #232 and #251 merged under it with P1s still labelled P1 and
  filed `deferred`, which is neither route the "Serious P1" section offers.

`scripts/pr-ready-audit.sh` follows the lane table. At line 1140 it tests the must-fix set against the
reviewer's label, before it reads the pull request's ledger, so no classification recorded in a ledger
row can reach it. Executed on 2026-09-11 at 19:17 UTC with this commit's audit, read-only:

```
$ bash scripts/pr-ready-audit.sh --reviewer eventloops 251
PR    LANE           HEAD     STATE         DETAIL
#251  ci             4f00440  NOT-READY     verdict=CHANGES_REQUIRED reviewed=546cee9 moved=merges-only blockers=mergeability-unknown,open-P1:unnamed,open-P1:unnamed,open-P1:unnamed,open-P1:unnamed
```

`mergeability-unknown` is there because #251 has already merged. The four `open-P1` blockers are the
P1s filed as non-blocking under the ruling. The `fix-p0p1` lane behaves the same way, so the audit
cannot pass any P1 fix whose review leaves a P1 the ruling allows.

## What the change that takes this up should do

Owner direction, 2026-09-11: rework the "Serious P1" wording, because what may merge is now set per
branch and per review. The branch prefix sets the must-fix set through the lane table, and the
2026-09-11 ruling decides whether a P1 raised in review blocks. The rework replaces the reaches-one-of
list with those rules, says how a P1 judged non-blocking is recorded in its ledger row and finding
file, and brings step 5, the summary of it in `CLAUDE.md` and the pull request template's checkbox
into line. The audit then reads that classification from the ledger row instead of from the
reviewer's label alone.

`WITNESS-RULE-CONTRADICTS-MERGE-BAR` is the evidence half of the same step and belongs in the same
rework.
