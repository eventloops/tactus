# `src/engine/topology/repair/tests.rs`

Extended notes for [`src/engine/topology/repair/tests.rs`](../../../../../src/engine/topology/repair/tests.rs).

The code is the authority for what it does; this file is the whole of its prose, moved out of
the source verbatim. Each section is headed by the line of code the comment sat above, spelled
as it is in the source, so the heading is the grep string that finds the code.

## Module

Tests for the repair-spawn builder.

## `fn the_repair_ladder_is_the_roots_rungs_at_or_above_the_raised_floor() {` › `let ladder = repair_ladder(`

`decisions.repairs.routing`: minimum tier mid intersected with the
root's frozen pin and ceiling. A root that starts at small loses that
rung; its floor becomes mid and its ceiling the highest survivor.

## `fn the_repair_ladder_is_the_roots_rungs_at_or_above_the_raised_floor() {` › `let frontier_only = repair_ladder(`

A root floored above mid keeps its own floor.

## `fn an_empty_tier_intersection_registers_a_human_binding_ladder_with_the_allowed_agents() {` › `let allowed = vec!["claude-code".to_owned(), "copilot".to_owned()];`

R10: a root whose every rung is below mid has no tier the repair may
run at. The frozen payload records exactly that — no tier, no rung, no
ceiling, the raised floor — and offers the run's allowed agents to the
person who must name a binding, never the sub-floor rungs it excluded.

## `fn the_admission_follows_the_ladder_first_and_the_consumed_allowance_second() {` › `assert!(matches!(`

Below the limit: runnable. At it: a person approves another attempt.

## `fn the_admission_follows_the_ladder_first_and_the_consumed_allowance_second() {` › `let mut waiting = entry;`

The empty intersection wins over the limit on either side of it: the
fold refuses HumanRequired on a HumanBinding ladder, so this is the one
admissible shape for an over-limit rejection with no tier left.

## `fn the_frozen_repair_spec_embeds_the_rejection_evidence_and_both_shas() {` › `let gate_output = "REPAIR-SPEC-GATE-EVIDENCE`

The evidence is not written into the record here. It is produced by the
production classification of a failing gate and of a rejecting reviewer, and
carried through `integrate::code_record` — the conversion that used to keep
`reason` and drop `feedback`. The version before round six handed
`code_rejection_record` its own `detail`, which sits **downstream** of that
conversion, so it proved the spec carries what it was given and nothing about
what gives it anything; it passed on a head where the evidence was being lost,
which is how the defect reached a green branch.

The needle is one line of the evidence, because the spec is asserted against its
JSON encoding and a newline is `\n` there; the fixture keeps a second line so
the conversion is exercised on a real multi-line log tail.
