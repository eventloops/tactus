# `src/topology/fold/tests/questions.rs`

Extended notes for [`src/topology/fold/tests/questions.rs`](../../../../../src/topology/fold/tests/questions.rs).

## `replay` › `let replay =`

Replay owns an independent copy of the same frozen inputs.

## `a_new_bare_or_standalone_admission_question_cannot_enter_an_active_lineage_transaction` › `trace.record(settle(`

Settle the sibling without a question so transaction ownership is the
only reason the later questions must be refused.

## `bare_questions_and_active_generations_exclude_each_other_across_a_lineage` › `trace.record(raised("quiet", queried));`

Question first: the affected lineage cannot acquire a generation.

## `bare_questions_and_active_generations_exclude_each_other_across_a_lineage` › `trace.record(dispatched);`

Generation first: bare or standalone questions cannot enter it.

## `const NO_AUTOMATIC_REPAIRS: u32 = 0;`

The limit a trace freezes when its first rejection must park its repair
on a repair-admission question. The fold admits a `HumanRequired` repair
only once the lineage has consumed the automatic repairs the run froze
(INV-11, `check_merge_rejected`), so a trace that wants the first
rejection of a root to ask a person freezes none — `Trace::parking` and
`Trace::wide_parking` — while a trace whose first rejection registers a
runnable repair keeps the fixture's limit through `Trace::started`. The
`limit` a human-required admission records is held to the frozen one by
`check_admission`, so it is spelled with the same constant.

## `fn without_automatic_repairs(mut event: TopologyEvent) -> TopologyEvent {`

The fixture's `run_started` with `max_merge_repairs` set to
`NO_AUTOMATIC_REPAIRS`. The registry digest does not cover the limits, so
the frozen inputs are unchanged.
