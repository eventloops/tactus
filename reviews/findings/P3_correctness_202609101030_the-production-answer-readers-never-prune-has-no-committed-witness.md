---
id: G4B-O9-EVENT-LOG-ANSWER-READER-NEVER-PRUNES-UNWITNESSED
severity: P3
disposition: deferred
category: correctness
pr: 249
reviewed_sha: 7e0110a13acce525567a7ac43abb016a232d6236
location: src/interaction.rs:288
provenance: pre_existing
first_bad:
guard: the round that makes `an_answer_published_into_the_run_directory_is_ingested_by_the_next_incarnations_first_step` read the published file after the loop's ingestion and assert it byte-identical (R21) — the G4 measurements' `published_unchanged` is printed, not asserted, so committing their shape unchanged would leave the rewrite case unguarded
---

## Failure sequence

R21 and the T-ANSWER row say the answer file is persistent run-directory content in every case:
ingestion is a **read**, never a take. The rundir layer has a witness —
`a_staged_partial_is_never_ingested_and_a_published_answer_survives_ingestion` reads the published
file after `rundir::ingest_answer`. The layer the loop actually calls, `EventLogAnswers::poll` and
`resolve` in `src/interaction.rs` — the production `AnswerSource` — has none.

    G4 run-3 mutation MR1: `EventLogAnswers::poll` removes `answers/<qid>.json` after reading it
    -> the full library suite passes; every committed test that ingests through EventLogAnswers
       (an_answer_published_into_the_run_directory_is_ingested_by_the_next_incarnations_first_step,
       the T-ANSWER tests in src/engine/topology/recover/tests.rs) checks the event that was
       appended and never reads the file afterwards
    -> the only tests that die are this run's temporary T-ANSWER kill measurements, `G4K-torn` and
       `G4K-complete`, and they die on their **existence** assertion —
       `assert!(published.is_file(), "R21: the answer file is never pruned")`. The byte comparison
       beside it is printed (`published_unchanged=true`) and not asserted, so a reader that
       *rewrote* the file rather than pruning it would not be caught even by these

Measured here at `7e0110a1` (gate report §3; log
`~/tactus-artifacts/g4r3-evidence-7e0110a1/mutations/mut-MR1.log`), reproducing what G4's second run
measured at `81ee09ef`. Not a behaviour defect at this sha: the production reader does not prune, and
both kill measurements read `published_unchanged=true` after the resume. It is a missing committed
witness for an R21 property the design states in every case.

## What the change that takes this up should do

One assertion: after the loop's ingestion in
`an_answer_published_into_the_run_directory_is_ingested_by_the_next_incarnations_first_step`, read
`answers/<qid>.json` and assert it is present and byte-identical to what was published. That kills
MR1 and puts the R21 read-not-take property under a committed guard at the layer the loop calls.
