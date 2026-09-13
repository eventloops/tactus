---
id: PR258-SECTION-14-MISREPORTS-THE-REVIEW-ARITHMETIC
severity: P3
disposition: deferred
category: docs-contract
pr: 258
reviewed_sha: f6fd7fc0faa7e79618da922270ebe666f6100a5f
location: reviews/2026-09-09-g4-fix-temporary-object-fanout-record.md:1194
provenance: introduced_by_feature
first_bad: f6fd7fc0faa7e79618da922270ebe666f6100a5f — §14 is new in that commit
guard: the copied review 42-r7-review-ultra-4cc732d.md is the source, its first sentence and its twelve-row table both giving 7 corrected / 2 over-corrected / 3 still wrong; the change that takes this up corrects the record's three numbers to the table's
---

## Failure sequence

Record §14's opening paragraph says the fifth review "audited round 6's twelve corrections —
eight corrected, two over-corrected into a new universal, two still wrong".

    42-r7-review-ultra-4cc732d.md, first sentence: "seven are corrected, two are
      over-corrected, and three remain wrong"
    -> its table: items 1, 3, 4, 6, 7, 9 and 11 `corrected` (seven); 2 and 10
       `over-corrected` (two); 5, 8 and 12 `still wrong` (three)
    -> the record reports 8 / 2 / 2 for a review that says 7 / 2 / 3: one still-wrong item is
       counted as corrected

Established by reading the copied review against the record at this sha (a `grep -c` of the
table's three labels gives 7, 2 and 3). §14.1 enumerates the seven items the review asked to be
closed by name, and does not depend on the count. Finding 4 of the `ultra` review of `f6fd7fc0`,
posted on the pull request as the SHA-bound comment.

## Why it is deferred

A miscount in the record's narration of a review whose text is saved verbatim beside it.
Reasoned-only in the review's terms, bearing on no Gate 4 pass-rule clause and no row-10
evidence, so the owner's rule files it and merges.

## What the change that takes this up should do

Change "eight corrected, two over-corrected into a new universal, two still wrong" to "seven
corrected, two over-corrected into a new universal, three still wrong", and check the sentence
against the table rather than against memory.
