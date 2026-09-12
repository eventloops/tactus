---
id: VERIFICATION-TERMINALS-WITHOUT-GATE-RECORDS
severity: P3
disposition: deferred
category: docs-contract
pr: TBD
reviewed_sha: 7da321c02cd9d8177174abed7633d2e0520c8276
location: design/26_design_merge_queue_protocol.md:173
provenance: pre_existing
first_bad: PR8-R2-SPEND-REPLAY — the review half of the same sentence, which is now implemented
guard: the change that decides whether §26's sentence is implemented for all four terminals or narrowed to the two that publish; the branch `fix-P1/correctness_the-unavailable-terminal-carries-no-spend` implemented the spend half and stated this half as unimplemented rather than deciding it
---

## Failure sequence

DESIGN §26's event list says of the four merge-verification terminals: "Those four terminal shapes
carry the complete gate/review records, usage/cost, and outcome, and ledger/status count exactly
one of them." Two of them carry no gate records at all.

    an integration verification runs four gates on the proposed tree
    -> the third gate's verdict is a refusal a reviewer would have had to judge, or the reviewer
       is rate-limited, and the sequence settles `merge_verification_unavailable`
    -> the terminal records `sequence`, `cause`, `outcome` and, since this branch, `reviews`
    -> nothing in the log says which gates ran on that tree or what they answered, so a reader
       reconstructing why the candidate was parked has the reviewer's verdict and not the gates'
    -> the same is true of `merge_verification_interrupted`, which records `sequence` and `detail`

`merge_prepared` and `merge_rejected` do carry them, inside their `VerificationRecord`. So the
sentence is true of the two terminals that decide a publication and false of the two that do not.

This is a reporting gap and not a budget one: a gate is a local process with no reported cost, so
no ceiling and no `Spend` total is wrong because of it. What is wrong is the design sentence, which
claims something of four records that is true of two.

## What the change that takes this up should do

Decide which way the sentence goes, and make the tree and the text agree.

Either give both terminals their gate verdicts — a third Class C field in the shape
`VerificationRecord.gates` has, with the same wire strictness and the same required-field
treatment `reviews` took — or narrow the sentence to the two publishing terminals and say
explicitly that an unavailable or interrupted verification records its cause and its spend and not
its gate evidence. The second is smaller and may well be right: `merge_verification_started`
already records "the recorded gates/review passes about to run", so which gates a sequence was
going to run is in the log either way, and what the unavailable terminal would add is their
verdicts.

Do not decide it by widening the §13 same-change subsection this branch added, which states the
gap and claims nothing else.
