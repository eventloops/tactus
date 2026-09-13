---
id: PR258-FINAL-CHECKS-COVERAGE-MISATTRIBUTED
severity: P3
disposition: deferred
category: docs-contract
pr: 258
reviewed_sha: f6fd7fc0faa7e79618da922270ebe666f6100a5f
location: reviews/2026-09-09-g4-fix-temporary-object-fanout-record.md:1273
provenance: introduced_by_feature
first_bad: f6fd7fc0faa7e79618da922270ebe666f6100a5f — §14.3 is new in that commit
guard: none — the four files exist and carry manifest rows, so no evidence is missing; the change that takes this up says what the checker checked (the round-7 files cited before it ran) and that the four written after it are attested by the manifest, not by the checker
---

## Failure sequence

Record §14.3 says the checks "that each round-7 file cited here and in the body exists — are
`45-r7-final-checks.log`", and the body's Gates paragraph says that log "covers ... the cited
round-7 files".

    45-r7-final-checks.py, line 46, collects every `4N-r7-*` name the record and the body cite
    -> line 47 lists `45-r7-final-checks.log`, `46-r7-pr-body.md`, `47-r7-final.patch` and
       `48-r7-manifest-update.py` as `later`, and the loop `continue`s past each of them
       ("produced after this check, by construction")
    -> the existence assertion runs for the six other cited names only — the log's `CITED=`
       lines — and then reports `CITED_FILES_PRESENT=PASS`
    -> the review's reasoned sequence: delete `46-r7-pr-body.md` and rerun; the cited-file
       phase still passes

Reasoned from the checker's source, as the review says; the deletion was not executed. The four
files exist and carry rows in `19-r3-evidence-manifest.md` (written by `48-r7-manifest-update.py`),
and the review's own read-only recount "found 229 files, 228 rows, only the manifest omitted,
with no digest or size mismatch" — so the evidence is present and what is wrong is the sentence
that attributes its verification to a checker that skipped it by name: false attribution, not
missing evidence. Finding 5 of the `ultra` review of `f6fd7fc0`, posted on the pull request as
the SHA-bound comment.

## Why it is deferred

Reasoned-only, no evidence missing, no failing test or witness; bears on no Gate 4 pass-rule
clause and no row-10 evidence, so the owner's rule files it and merges.

## What the change that takes this up should do

Make §14.3 and the body's Gates sentence say what ran: the checker verified the cited round-7
files that existed before it ran, skipped by name the four it knew would be written after it, and
those four are attested by the manifest's rows. Or run a checker over the full cited set after the
last file is written and cite that log instead.
