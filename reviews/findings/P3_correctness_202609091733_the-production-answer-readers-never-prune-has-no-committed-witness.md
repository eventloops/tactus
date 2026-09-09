---
id: G4B-O9-EVENT-LOG-ANSWER-READER-NEVER-PRUNES-UNWITNESSED
severity: P3
disposition: deferred
category: correctness
pr: 249
reviewed_sha: 81ee09efb5926fb4b93565223ea05dd0052bc2f9
location: src/interaction.rs:288
provenance: pre_existing
first_bad:
guard: the round that makes `an_answer_published_into_the_run_directory_is_ingested_by_the_next_incarnations_first_step` read the published file after the loop's ingestion and assert it byte-identical (R21), or commits the G4 rerun's `G4K` shape with its `published_unchanged` assertion
---

## Failure sequence

R21 and the T-ANSWER row say the answer file is persistent run-directory content in every case:
ingestion is a read, never a take. The rundir layer has a witness
(`a_staged_partial_is_never_ingested_and_a_published_answer_survives_ingestion` reads the published
file after `rundir::ingest_answer`). The layer the loop actually calls — `EventLogAnswers::poll` and
`resolve` in `src/interaction.rs`, the production `AnswerSource` — has none.

    G4 rerun mutation MR1: `EventLogAnswers::poll` removes `answers/<qid>.json` after reading it
    -> the full library suite passes (samplers skipped); every committed test that ingests through
       `EventLogAnswers` (`an_answer_published_into_the_run_directory_is_ingested_by_the_next_incarnations_first_step`,
       the T-ANSWER tests in `src/engine/topology/recover/tests.rs`) checks the event that was
       appended and never reads the file afterwards
    -> the only tests that died were the rerun's temporary T-ANSWER kill measurements (`G4K-torn`,
       `G4K-complete`), which assert `published.is_file()` after the resume

Measured at `81ee09ef` (report §3, MR1). Not a behaviour defect at this sha: the production reader
calls `read_answer`, which reads and deserializes and removes nothing, and the rerun's two kills read
the file byte-identical after the kill and after the resume. It is a guard with no committed witness
at the layer that matters, of the same class as `G4-O1` and `G4-O2`.

## What the change that takes this up should do

After the loop's ingestion in `an_answer_published_into_the_run_directory_is_ingested_by_the_next_incarnations_first_step`,
read `paths.answers().join("<component>.json")` and assert it exists and equals the bytes published
— one assertion. Better, commit the rerun's `G4K` shape (a child incarnation killed inside and after
the `question_answered` append, then resumed) with its file assertions; that gives T-ANSWER's
`answer_ingested_after_kill_returns_pending` slug a real kill as well as this witness.
