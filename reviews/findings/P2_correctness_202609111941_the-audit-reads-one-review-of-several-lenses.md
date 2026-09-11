---
id: AUDIT-JUDGES-ONE-REVIEW-OF-SEVERAL-LENSES
severity: P2
disposition: deferred
category: correctness
pr:
reviewed_sha: 8d97e25c6342b07d050779ba93624135669f3455
location: scripts/pr-ready-audit.sh:575
provenance: pre_existing
first_bad:
guard: project owner
---

## Failure sequence

`reviews/findings/PROCESS.md` §7 gives every findings-sweep pull request two review lenses, fix-check
and regression, and a third by lane. `MAINTAINING.md` step 4 describes one review pass whose verdict
the driver posts as one SHA-bound comment. `scripts/pr-ready-audit.sh` reads exactly one comment.
`review_comment_filter` keeps the trusted reviewer's comments that carry `<!-- upstroke-frontier-review`
or `Reviewed head: <sha>`, `last` at line 575 keeps the newest on each page, and `latest_review_id`
keeps the newest across pages. A multi-lens review can therefore reach the audit only as one combined,
marker-bearing comment in a form `scripts/pr-review-parse.py` reads, and nothing today writes one.

Executed on 2026-09-11 at 19:17 UTC with this commit's audit, read-only, with no `--apply` and no
`--enqueue`:

```
$ bash scripts/pr-ready-audit.sh --reviewer eventloops 265
PR    LANE           HEAD     STATE         DETAIL
#265  fix-p0p1       c683b1c  NOT-READY     verdict=none reviewed= blockers=draft,blocked-by-rules,upstroke-ci:failure,no-review
```

Pull request #265 had been through five review rounds of two lenses each. The P1 orchestrator ran the
lenses without posting them and then posted one combined comment per round, because the build box's
review poster keeps a single comment per pull request and head and edits it in place, so a second lens
posted through it overwrites the first. The combined comments carry neither marker, so the audit
reports `no-review` on a pull request with ten lens verdicts in its thread. The `upstroke-ci:failure`
in that row is a cancelled run that a body edit superseded, and is not part of this finding.

The other direction follows from the same two sites and was not executed. If every lens is posted
through the review poster, the comment for that head ends up holding only the lens that finished last.
A regression lens returning `CHANGES_REQUIRED` with a blocking P1, followed by a fix-check lens
returning `PASS`, leaves a `PASS` in the thread, and the audit judges that.

## What the change that takes this up should do

Decide what a multi-lens review looks like on a pull request, then make the review poster and the
audit agree on it. One way is a comment per lens that the audit reads together, where a blocker in any
lens blocks. Another is one combined comment in a form `scripts/pr-review-parse.py` reads, with every
lens's findings carrying ids. Until one of those exists the audit cannot judge a findings-sweep merge,
and `MAINTAINING.md` step 4 should say that a sweep review is more than one pass.
