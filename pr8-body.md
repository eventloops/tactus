# feat(engine): integration transactions (PR8)

## Summary

PR8 implements the integration transaction machinery for upstroke v0.2: the FIFO merge queue's
one-at-a-time transaction, the exact-base decision read from the integration ref head under
`assert_publishable` before any staging effect, all three admission shapes (`fast`, `stale_clean`,
`already_present`), every `merge_verification_started` terminal (`merge_prepared`, `merge_rejected`
with atomic frozen-repair registration, `merge_verification_unavailable` for `HumanRequired` and
`Infrastructure` with `Deferred` or `Parked` outcomes, and `merge_verification_interrupted`), and
the recovery that completes or settles whatever transaction a crash leaves open.

The compare-and-swap on the integration ref is the queue's serialization point. A `fast` sequence
publishes the candidate commit itself with no staging worktree, no intent, no proposal object and
no pin. A `stale_clean` sequence cherry-picks onto the moved head, verifies the proposal on a fresh
snapshot — under the binding the candidate ran under, after the review input has been classified,
with a Runner outage settled as one — and publishes the pinned proposal. `already_present`
validates the head with a no-op expected-old swap and manufactures no commit. A conflict or a
code-attributed rejection registers a complete frozen repair — its lineage, tier ladder, path
hints, acceptance and admission — in the same `merge_rejected` append, before any repair effect.
Snapshots are removed only after the verification's terminal is durable.

Recovery establishes the stable-prefix barrier over the proven prefix before any CAS, so a CAS is
never issued on a merely replay-visible `merge_prepared`. A `Prepared` transaction is then completed
through the same publish the live path uses; a `VerificationStarted` one is settled interrupted,
its pin pruned at the proposal the record names, its staging and snapshots reclaimed, and its
candidate re-verifies under a new sequence. Pins are handled by their records: the open
transaction's is kept, a resolved sequence's is pruned at its recorded proposal, exactly the
provisional orphan is reclaimed, and everything else refuses untouched. With no publication
pending, the integration ref must name the log's latest publication.

This slice is inert by default: the schema-4 topology engages only by explicit schema choice, and
the v0.1 path is unchanged.

## Scope

In scope: `src/engine/topology/integrate.rs` (the transaction, all terminals, the exact-base
decision, the CAS and publish, the recovery primitives), `src/engine/topology/repair.rs` (the
frozen-repair builder), the integration recovery in `src/engine/topology/recover.rs`
(`finish_integration`, `reclaim_stale_residue`, the published-run form of
`ensure_recorded_integration_ref`), the loop and checkpoint wiring in `run.rs`/`select.rs`
(the verification context, the implementer binding, the review-input classification, the charged
reviews), the judge's typed Runner error and snapshot disposal in `attempt.rs`, the verification
harness in `scaffold.rs`, and the supporting `workspace_manager` reads (`proposal_state`) and
names (`SnapshotName::integration_review`).

Out of scope, and neither built nor stubbed: repair **execution** (PR9), the production writer, the
slot broker. A PR8 build refuses, before any append, dispatch of a Repair-origin task and any
repair-admission answer. No attributed `DesignDefect` record is emitted; questions carry no
attribution.

**`src/topology/**` freeze — Class B changes, owner approval owed before merge.** Three changes
touch the frozen fold layer. All are behavioural and are listed here for per-instance approval:

- `TransactionClass::Prepared` retains `expected_head` (set in `apply_merge_prepared`, read by the
  CAS recovery). Retaining it in the fold rather than re-deriving it from the event list a second
  time (`src/topology/fold.rs`, `src/topology/fold/apply.rs`).
- `TransactionClass::Prepared` also retains the `disposition` and the `prepared_ref` the record
  named (same two files; added in the repair round). Recovery reads them to know what a prepared
  publication left behind, because the SHAs cannot say: an already-present publication at the
  candidate's own commit has `proposed_sha == candidate.commit_sha` exactly as a fast one does
  while still owning a staging worktree, and inferring "fast" from that equality leaked it.
- `check_merge_rejected` counts a lineage's registered repairs against `max_merge_repairs` and
  refuses the wrong admission (INV-11): a `Runnable` admission is refused once the lineage is at
  its limit, a `HumanRequired` admission below it, and a `HumanBinding` admission is accepted on
  either side of the limit, because the empty intersection wins and the fold refuses
  `HumanRequired` on a `HumanBinding` ladder (`src/topology/fold/check_integration.rs`). The
  first version of this body said an over-limit `HumanBinding` was refused; the code never did
  that and the contract does not ask it (`pr8-plan.md` R8).

No Class C (wire-vocabulary) change was needed. The fold readers added in
`src/topology/fold/predicates.rs` (`next_sequence`, `satisfies_closure`, `lineage_members`) are
Class A: read-only accessors over derivations the fold already makes. The test-only adaptations in
`src/topology/fold/tests.rs`, `src/topology/fold/tests/questions.rs` and `src/topology/census.rs`
are Class A.

**Recorded assumptions** (ambiguities resolved to the reading most consistent with the packet and
errata; the full set with rationale is in `pr8-plan.md`, where the readings the contract had
already settled are marked as such):

- The published proposal's pin is deleted expected-old right after `task_merged`; a pin surviving a
  crash before its deletion is pruned by the next resume at its recorded proposal sha and refused
  at any other.
- `already_present` issues a real expected-old no-op CAS so Git validates the head atomically and the
  site is observed.
- An empty cherry-pick is classified by a read-only `proposal_state` inspection after Git reports the
  failed pick; the cherry-pick funnel is unchanged.
- When tier intersection is empty and the lineage is also over its repair limit, the empty
  intersection wins (`HumanBinding`): without a binding nothing runs whatever the limit says. The
  empty-intersection ladder records no tier, no rung, no ceiling, the raised floor, and the entry's
  allowed agents as the options (R10).
- The implementer a verification's review passes are selected against is the binding the candidate
  ran under: the task's validated override, else the frozen rung at the fold-derived rung position
  (R18).
- Only a Runner's own error running a gate is an outage of the sequence (`RunnerSpawnFailure`);
  every other verification error ends the command resumably (R24).
- An integration's judged reviews are charged to the candidate's task and the run at the
  verification; on replay they are read off `merge_prepared` and `merge_rejected`. An unavailable
  terminal carries no review record, so a park's or an outage's review is charged live only (R22).
- With no publication pending, the authorized integration head is the log's latest `task_merged`,
  or the recorded base before any; a ref elsewhere, or absent after a publication, refuses before
  any append (R23).
- The two-crash proof's "unsynced `merge_prepared`" is a complete line whose flush was made to fail
  after the full write; the restart is a child process killed at the `task_merged` write; "power
  loss" truncates the log to the length the barrier proved durable.

## Validation

All ten gates green locally, from the repository root, on the last code commit of this branch,
`254597cd44f3b1f9adc5f4d7ac6d1a0e7b8ff1a3`; the commit that follows it changes the three record
files and no code, and the ten gates were rerun on it before the push:

```
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features            # 2332 lib tests passed, 0 failed, 43 ignored (Linux)
cargo +1.85.0 check --locked --all-targets --all-features
bash .github/scripts/test-release-record.sh
bash .github/scripts/test-pr-policy.sh
bash .github/scripts/test-pr-ledger-evidence.sh
bash .github/scripts/test-docs-consistency.sh
bash .github/scripts/test-internals-notes.sh
bash .github/scripts/test-pr-ready-audit.sh
```

Proof obligations from the contract, and where each is met:

- **Real-repository CAS, orphan, third-SHA.** `integrate::tests` fast/stale/already-present publish
  against a real repository; `third_sha_refused_and_a_ref_already_at_the_proposal_only_records`;
  `recover::tests::a_resume_reclaims_the_orphan_pin_at_the_next_sequence_and_orphan_staging`,
  `a_resume_refuses_a_prepared_pin_outside_the_sequences_the_log_pinned`,
  `a_resume_refuses_a_substituted_verification_pin_before_settling_it`,
  `a_resume_keeps_a_prepared_transactions_pin_when_publication_refuses`,
  `a_resume_prunes_a_resolved_sequences_pin_at_its_recorded_proposal_and_refuses_it_elsewhere`,
  `a_resume_of_a_prepared_transaction_whose_ref_moved_elsewhere_refuses_a_third_sha`.
- **Fast no-staging.** `fast_path_publishes_exact_candidate_without_staging_or_proposal_object`: the
  hook harness records no `Worktree.AddStaging`, no `Object.ProposalCherryPick`, no
  `Ref.PinPrepared`; object count unchanged; no `merge/s<seq>` intent.
- **The three fast mismatches, live and on replay.**
  `merge_prepared_fast_with_moved_head_or_wrong_proposed_or_pin_refused_live_and_on_replay`.
- **Stale path.** `stale_candidate_takes_staging_path_and_publishes_pinned_proposal`, whose gate
  is asserted to have run on a snapshot whose HEAD is the recorded proposal and never in the
  staging worktree.
- **The two-crash proof.** `recover::tests::unsynced_merge_prepared_two_crash_barrier_before_cas_then_power_loss_keeps_log_and_ref_agreeing`:
  a complete, unsynced `merge_prepared`; the restart in a child process whose barrier reports its
  sync of the log file before the swap is entered; the CAS; the kill at the `task_merged` write; the
  loss of every byte the barrier did not prove durable; the log still holding `merge_prepared`,
  the ref at `proposed_sha`, and the next resume appending `task_merged` without a second swap,
  replay twice equal. `barrier_sync_failure_before_cas_issues_no_cas_and_converges_after_loss`:
  the sync fails at the barrier, no CAS, nothing appended, and after the loss the candidate is
  still queued and integrates. The crash reviewer's mutation (the barrier's `sync_log_file`
  removed) fails both.
- **Completed publications resume.** `a_resume_after_a_completed_publication_accepts_its_own_head`,
  `a_resume_after_a_publication_refuses_a_ref_that_disagrees_with_the_log`, and
  `a_resume_completes_a_prepared_transaction_whose_cas_already_ran_by_recording_the_merge`
  (kill between CAS and `task_merged` converges).
- **Terminal-shape coverage table.** `integrate::tests::terminal_shape_coverage_table_drives_every_shape_and_each_converges_on_replay`
  drives fast, stale_clean, already_present, conflict, code_rejected, deferred and parked end to end,
  each replayed twice for equality and each verifying shape's gate asserted to have judged the
  recorded proposal; Declined-after-park is `fold::tests`'
  `declined_parked_verification_fails_task_consumes_queue_position_releases_lease_and_halts_per_policy`
  and Interrupted is `recover::tests`'
  `a_resume_settles_an_interrupted_stale_verification_and_reclaims_its_residue`, which then
  re-verifies and publishes the candidate under the next sequence.
- **Kill/residue for the cherry-pick class.** `recover::tests::synthetic_cherry_pick_residue_unreferenced_objects_and_cherry_pick_head_then_forced_reclaim_converges`
  and `sampled_cherry_pick_child_kills_every_residue_classified_and_recovered`: the Internal residue
  class constructed by hand and sampled from real killed `git cherry-pick` children, each classified
  by the workspace manager's classifier and each reclaimed through the resume, the objects left to
  Git and the candidate integrating afterwards. Snapshot residue: `verification_snapshots_are_removed_only_after_the_terminal`
  and `a_resume_reclaims_an_interrupted_verifications_snapshots_after_settling_it`.
- **Outages, parks and answers, driven through the loop.**
  `a_gate_spawn_failure_during_integration_verification_defers_inside_max_defers`,
  `an_unjudgeable_proposal_parks_the_candidate_for_a_person`,
  `an_integration_review_is_selected_against_the_candidates_recorded_implementer`,
  `an_integration_reviews_cost_reaches_the_run_spend`,
  `a_verification_park_answer_is_ingested_at_the_hard_block_and_a_repair_admission_answer_is_refused_before_any_append`.
- **Refusals proven, not merely coded.** Third SHA / symbolic / checked-out
  (`integrate::tests`, `recover::tests`); orphan pin outside the next sequence (`recover::tests`)
  and a second unresolved transaction (`fold::tests`); non-eligible starts, the three fast
  mismatches, `Deferred` at `max_defers`, non-consecutive defers, `Parked` without a question,
  `HumanRequired` without `Parked` (`fold::tests`); a lineage that has consumed its allowance
  (`a_lineage_that_has_consumed_its_allowance_registers_only_a_human_required_repair`);
  repair-origin dispatch and repair-admission answers refused before any append (`select`,
  `recover::tests`).
- **Verification isolation.** The integration verification runs on fresh snapshots of the proposal
  or head commit, creates no new object, and the recording runner's workspace HEAD is the proposal
  for every verifying shape (`integrate::tests`).
- **Mutation witnesses replayed against the repaired tree**, each killed by the test its ledger
  row names: the reviewer's m01, m02, m03, m05 and m06; the barrier sync removed; and one
  mutation per repair of the round (`pr8-plan.md` §5).

## Review evidence

Implemented by Claude (Opus 4.8) running as an autonomous Claude Code session, at high effort, in
one continuous run against the frozen contract at `decisions.pr_sequence[9]` of the parallelism
packet as amended by the 2026-08-25 G2 errata, finishing on head
`9d0359394e63878e51abda1ec5c54c6f94578363` with the ten gates green locally; the body commit
`3414dc5861c9a523342ea0792a54f0129cf82f2f` followed. Commit-per-terminal-shape, as the packet's
own size mitigation asks.

Three independent frontier reviews were then run by the owner against the exact head `3414dc58`,
each through the lens review script (`review-pr-lens.sh`, whose reviewer is `gpt-5.6-sol` at
`max` effort by default; the review records do not restate the model), each with a different
lens and each returning `CHANGES_REQUIRED`:

- **conformance to the frozen design** — the only reviewer given the design contract: findings
  F1–F9, an audit of the twenty recorded readings, and a freeze-classification audit;
- **crash consistency and recovery** — six findings, three of them P1, each with a reproduction
  the reviewer ran;
- **test adequacy and code quality** — thirteen findings from a 24-mutation audit.

Every finding was triaged in `pr8-triage.md` (Claude Fable 5.1 at max effort, one continuous
autonomous session, 2026-09-07) and every one was confirmed; none was rejected. Every confirmed
finding was repaired on this branch, one finding or closely related group per commit, each with a
test that fails at `3414dc58` or a mutation witness replayed against the repair, except one that
is beyond this slice's reach and is deferred with a ledger row and an owner decision owed
(`PR8-CRASH-002`). The two documentation defects the owner was being asked to approve on — the
over-limit `HumanBinding` description and the empty-intersection ladder — are corrected: the
description to what the fold does, and the code to what R10 recorded.

The repaired head has not been reviewed. The frontier review of it is **owed and is the owner's
to run**; it is not part of this branch. The ledger below carries the canonical header and one
row per distinct finding.

## Risk and rollback

The largest risk is the slice's size: it introduces the transaction, every terminal, and the
recovery for each in one PR. It is mitigated by internal commits per terminal shape, the terminal
coverage table, the two-crash proof, the residue proofs, and replay-twice-equal on every shape.

The three Class B fold changes are the sharpest review surface: the retained `expected_head`,
`disposition` and `prepared_ref` change what `TransactionClass::Prepared` carries, and the INV-11
count changes which repair admissions the fold accepts. All are exercised live and on replay in
`fold::tests`.

One known residue class is not reclaimed (`PR8-CRASH-002`): a lock file left by a coordinator
killed inside `git update-ref`. The refusal it causes is resumable and loses nothing, and the
operator's removal of the lock lets the next resume complete the publication; reclaiming it needs
a `Ref.*` residue class in the frozen inventory.

Rollback is clean: the schema-4 topology is inert unless a plan selects it, so reverting the branch
removes the machinery without touching the v0.1 path or any released behaviour. No data migration,
no on-disk format change outside the run-scoped `refs/upstroke/runs/<run>/…` namespace this slice
owns.

## Review finding ledger

| ID | Severity | Reviewed SHA / location | Failure sequence | Provenance | Category | First bad / prior ID | Regression or documented guard | Disposition |
|---|---|---|---|---|---|---|---|---|
| PR8-C1 | P1 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/recover.rs:1001 | publish B to P -> task_merged durable -> restart -> startup check compares the ref with run_started.base_sha -> the run's own head refused as foreign on every resume | introduced_by_feature | crash-consistency | 43d62194 | `a_resume_after_a_completed_publication_accepts_its_own_head` | fixed |
| PR8-C2 | P1 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/recover.rs:1133 | stale merge_prepared durable -> restart with the ref at a third SHA -> residue reclaim deletes the Prepared transaction's pin -> publication refuses with the still-authorized proposal unpinned | introduced_by_feature | crash-consistency | 43d62194 | `a_resume_keeps_a_prepared_transactions_pin_when_publication_refuses` | fixed |
| PR8-C2-SUBSTITUTED | P1 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/integrate.rs:573 | verification started -> a writer moves the pin to another object -> resume appends interrupted and deletes the pin expected-old at the substituted target, proposed_sha never compared | introduced_by_feature | crash-consistency | 43d62194 | `a_resume_refuses_a_substituted_verification_pin_before_settling_it` | fixed |
| PR8-C2-ORPHAN | P1 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/recover.rs:1182 | next sequence 0 -> prepared/3 present -> residue reclaim deletes it before refuse_unexpected_refs can refuse it; the branch's orphan test expected the deletion | introduced_by_feature | crash-consistency | 43d62194 | `a_resume_refuses_a_prepared_pin_outside_the_sequences_the_log_pinned` | fixed |
| PR8-C3 | P1 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/integrate.rs:763 | merge_verification_started -> the Runner errors spawning a gate -> the error escapes through ? -> the transaction stays open and defers untouched -> repeated outages bypass the defer and park limit | introduced_by_feature | correctness | 2d1b4c72 | `a_gate_spawn_failure_during_integration_verification_defers_inside_max_defers` | fixed |
| PR8-C4 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/recover.rs:1169 | verification started -> gate snapshot added -> crash -> resume settles interrupted, prunes the pin and staging -> the snapshot checkout and its intent survive every resume | introduced_by_feature | crash-consistency | 43d62194 | `a_resume_reclaims_an_interrupted_verifications_snapshots_after_settling_it` | fixed |
| PR8-C5 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/recover/tests.rs:6157 | the proof appends merge_prepared synced -> rewrites the log to remove it -> resumes once -> asserts the absence of what it deleted; the barrier's sync removed from production still passes | introduced_by_feature | docs-contract | a1759f16 | `unsynced_merge_prepared_two_crash_barrier_before_cas_then_power_loss_keeps_log_and_ref_agreeing` | fixed |
| PR8-C6 | P1 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/run.rs:166 | candidate produced at rung 0 -> the ladder's last rung passed as the implementer -> passes_for keeps the primary that equals the real implementer -> a model reviews its own candidate | introduced_by_feature | correctness | 2d1b4c72 | `an_integration_review_is_selected_against_the_candidates_recorded_implementer` | fixed |
| PR8-CONF-F4 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/attempt.rs:617 | gates run -> gate snapshot removed -> each reviewer snapshot removed after its pass -> the verification terminal appended afterwards; a removal failure strands a completed judgement behind an unterminated verification | introduced_by_feature | crash-consistency | 2d1b4c72 | `verification_snapshots_are_removed_only_after_the_terminal` | fixed |
| PR8-CONF-F9 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / pr8-body.md:116 | the body cites the effects census self-test as the cherry-pick residue evidence -> no recovery test constructs the residue class or samples killed picks through the recovery path | introduced_by_feature | docs-contract | 3414dc58 | `synthetic_cherry_pick_residue_unreferenced_objects_and_cherry_pick_head_then_forced_reclaim_converges` | fixed |
| PR8-CRASH-002 | P1 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/integrate.rs:487 | merge_prepared durable -> git update-ref killed after creating integration.lock and before the rename -> resume retries the authorized CAS -> Git refuses on the lock -> every later resume repeats the refusal until an operator removes the file; the same class was met once more on the macOS runner, where a killed git cherry-pick left packed-refs.lock in the common git dir and the resume's next ref write refused on it | pre_existing | crash-consistency | — | `a_ref_lock_left_by_a_killed_compare_and_swap_refuses_resumably_until_removed` pins the resumable refusal and the completion after removal, and the residue sampler removes git's common-dir ref locks after each kill as an operator would (`remove_git_ref_lock_residue`); no Ref site registers a residue class in the frozen inventory, so reclaiming the lock is a Class C change owed an owner decision | deferred |
| PR8-CRASH-005 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/integrate.rs:369 | the head moves onto the candidate commit -> empty cherry-pick -> merge_prepared(already_present) with expected_head == proposed_sha == candidate.commit_sha -> kill before publication -> from_fold infers fast from the SHAs -> the staging worktree survives recovery | introduced_by_feature | crash-consistency | 43d62194 | `a_resume_completes_an_already_present_publication_at_the_candidate_commit_and_reclaims_its_staging` | fixed |
| PR8-TESTS-002 | P1 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/run.rs:192 | the review-input policy refuses the proposed tree -> Judge invoked with no prior failure -> the proposal is published | introduced_by_feature | correctness | 2d1b4c72 | `an_unjudgeable_proposal_parks_the_candidate_for_a_person` | fixed |
| PR8-TESTS-003 | P1 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/run.rs:677 | an integration review costs 2.50 -> Spend unchanged live and on replay -> the next selection admits work below an apparent ceiling the real spend has passed | introduced_by_feature | correctness | 2d1b4c72 | `an_integration_reviews_cost_reaches_the_run_spend` | fixed |
| PR8-TESTS-005 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/scaffold.rs:719 | the scaffold verifier ignores its workspace and no test supplies a gate -> VerifyRequest.proposed replaced by the candidate SHA -> all fourteen integration tests pass | introduced_by_feature | correctness | 2d1b4c72 | `stale_candidate_takes_staging_path_and_publishes_pinned_proposal` asserts the gate's workspace HEAD is the recorded proposal | fixed |
| PR8-TESTS-006 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/integrate/tests.rs:859 | the swap skipped whenever expected_head equals proposed_sha -> unchanged HEAD and object count still hold -> all fourteen integration tests pass | introduced_by_feature | correctness | 9d035939 | `an_already_present_candidate_settles_without_an_empty_commit` counts exactly one compare-and-swap | fixed |
| PR8-TESTS-007 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/integrate/tests.rs:952 | the registered repair's intent and worktree created after merge_rejected -> only the last event kind and staging intents checked -> all fourteen integration tests pass | introduced_by_feature | correctness | 9d035939 | `a_conflicting_candidate_is_rejected_with_an_atomic_repair_before_any_repair_effect` asserts no task worktree effect after the rejection | fixed |
| PR8-TESTS-008 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/topology/fold/tests.rs:6402 | the over-limit test replaced by limit == 0 -> a lineage with a positive limit admits automatic repairs without bound -> the entire library suite passes | introduced_by_feature | correctness | b31098db | `a_lineage_that_has_consumed_its_allowance_registers_only_a_human_required_repair` | fixed |
| PR8-TESTS-010 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/recover/tests.rs:6332 | the fixture plants an intent and a pin and no staging worktree -> the production remove_worktree call removed -> the interrupted-cleanup test passes | introduced_by_feature | correctness | 43d62194 | `a_resume_settles_an_interrupted_stale_verification_and_reclaims_its_residue` plants and asserts the worktree and drives the re-verification | fixed |
| PR8-TESTS-013 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/repair.rs:82 | the whole root entry cloned without a stated reason -> its nested fields cloned again -> an empty_intersection flag duplicates the ladder's admission | introduced_by_feature | correctness | 2d1b4c72 | standards section 6, held by review: the builder borrows the root, clones the specification once, and reads the admission off the ladder | fixed |
| PR8-AUDIT-R8 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/topology/fold/check_integration.rs:471 | the body and reading R8 claim an over-limit HumanBinding admission is refused -> the fold accepts it on either side of the limit -> the owner asked to approve a Class B change on a description of what it does not do | introduced_by_feature | docs-contract | 3414dc58 | `a_lineage_past_its_repair_limit_registers_only_a_human_required_repair` accepts HumanBinding at the limit, and R8 now says so | fixed |
| PR8-AUDIT-R10 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/repair.rs:190 | the mid floor intersects the root's ladder empty -> the root's tiers, floor and ceiling kept and its excluded rungs offered -> reading R10 promised the raised floor, no ceiling and the probed agents | introduced_by_feature | docs-contract | 2d1b4c72 | `an_empty_tier_intersection_registers_a_human_binding_ladder_with_the_allowed_agents` | fixed |
