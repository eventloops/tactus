---
id: PR249-ANSWER-TEXT-NOT-CARRIED
severity: P2
disposition: deferred
category: correctness
pr: 249
reviewed_sha: 48017a43a5e23fc0a7876df2fc625f4c29e7d15a
location: src/engine/topology/run.rs:934
provenance: pre_existing
first_bad: PR8's fourteenth review found it at the `ingest_verification_answer` of that branch; the shape is `question_answered(schema 4)`'s from PR3
guard: the project owner — a Class C vocabulary decision (`Answer4::Answered` gains a text field, or `question_answered` gains a sibling carrying it); until then `topology_question_options` promises nothing about typed text and `a_verification_park_answer_is_ingested_and_the_candidate_re_verifies` holds the index-only ingestion
---

## Failure sequence

A schema-4 run parks a task on a question — a verification park, an attempt park, an
over-limit repair admission — and a person answers it in their own words, at the prompt or with
`upstroke answer --text`.

    question_raised (schema 4) with options [retry, give up]
    -> a person types guidance that names no option
    -> `answer_for` maps the text to `option_index: 0` (`chosen_index(...).unwrap_or(0)`)
    -> `question_answered { answer: Answered { option_index: 0, binding_override: None } }`
    -> the task un-parks and the next attempt's prompt carries none of what was typed

The legacy engine delivers the same text: `answer_question` turns a non-canned answer into
human feedback framed as "an instruction from a person" for the next attempt. Schema 4 cannot,
because the frozen `Answer4::Answered` carries an option index and an optional binding override
and nothing else, and the fold refuses an unknown field. Until PR #249's first repair round the
schema-4 `Clarify` question offered "answer in your own words (typed free text is sent back to
the agent)", a promise the driver did not keep; that wording is gone. The gap underneath it is
not: a person's typed guidance still reaches no agent in a schema-4 run.

## Why this is deferred as beyond reach, not as out of scope

Carrying the text needs a field the frozen vocabulary lacks — a Class C change to
`src/topology/events.rs`, which `MAINTAINING.md` and the packet reserve to the owner. No slice can
make it on its own authority. **If a later pass labels this P1, the disposition becomes
escalate-to-owner rather than still-deferred**, because the remedy is out of every session's
reach rather than merely out of this pull request's scope.

## What the change that takes this up should do

Decide the wire form — a `text: Option<String>` on `Answered`, or a distinct event — and then let
the schema-4 attempt path read it as the legacy `Feedback { human: true }` does, so the next
attempt's prompt carries the person's words with the same precedence the legacy engine gives
them. Until then, an answer that names no option un-parks the task and nothing more, and the
options say so.
