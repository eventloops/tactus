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
snapshot, and publishes the pinned proposal. `already_present` validates the head with a no-op
expected-old swap and manufactures no commit. A conflict or a code-attributed rejection registers a
complete frozen repair — its lineage, tier ladder, path hints, acceptance and admission — in the
same `merge_rejected` append, before any repair effect.

Recovery establishes the stable-prefix barrier over the proven prefix before any CAS, so a CAS is
never issued on a merely replay-visible `merge_prepared`. A `Prepared` transaction is then completed
through the same publish the live path uses; a `VerificationStarted` one is settled interrupted and
its candidate re-verifies under a new sequence.

This slice is inert by default: the schema-4 topology engages only by explicit schema choice, and
the v0.1 path is unchanged.

## Scope

In scope: `src/engine/topology/integrate.rs` (the transaction, all terminals, the exact-base
decision, the CAS and publish, the recovery primitives), `src/engine/topology/repair.rs` (the
frozen-repair builder), the integration recovery in `src/engine/topology/recover.rs`
(`finish_integration`, `reclaim_stale_residue`), the loop and checkpoint wiring in
`run.rs`/`select.rs`, the verification harness in `scaffold.rs`, and the supporting
`workspace_manager` reads (`proposal_state`) and names (`SnapshotName::integration_review`).

Out of scope, and neither built nor stubbed: repair **execution** (PR9), the production writer, the
slot broker. A PR8 build refuses, before any append, dispatch of a Repair-origin task and any
repair-admission answer. No attributed `DesignDefect` record is emitted; questions carry no
attribution.

**`src/topology/**` freeze — Class B changes, owner approval owed before merge.** Two changes touch
the frozen fold layer. Both are behavioural and are listed here for per-instance approval:

- `TransactionClass::Prepared` retains `expected_head` (set in `apply_merge_prepared`, read by the
  CAS recovery). Retaining it in the fold rather than re-deriving it from the event list a second
  time (`src/topology/fold.rs`, `src/topology/fold/apply.rs`).
- `check_merge_rejected` counts a lineage's registered repairs against `max_merge_repairs` and
  refuses the wrong admission (INV-11): a `Runnable` or `HumanBinding` admission is refused once the
  lineage is at its limit, a `HumanRequired` admission below it (`src/topology/fold/check_integration.rs`).

No Class C (wire-vocabulary) change was needed. The fold readers added in
`src/topology/fold/predicates.rs` (`next_sequence`, `satisfies_closure`, `lineage_members`) are
Class A: read-only accessors over derivations the fold already makes.

**Recorded assumptions** (ambiguities resolved to the reading most consistent with the packet and
errata; the full set with rationale is in `pr8-plan.md`):

- The published proposal's pin is deleted expected-old right after `task_merged`; a pin surviving a
  crash before its deletion is pruned by the next resume at its recorded proposal sha.
- `already_present` issues a real expected-old no-op CAS so Git validates the head atomically and the
  site is observed.
- An empty cherry-pick is classified by a read-only `proposal_state` inspection after Git reports the
  failed pick; the cherry-pick funnel is unchanged.
- Defer arithmetic follows `check_defer_allowance`: an outage defers while `defers < max_defers` and
  parks at the limit, so `max_defers = 0` parks the first outage.
- When tier intersection is empty and the lineage is also over its repair limit, the empty
  intersection wins (`HumanBinding`): without a binding nothing runs whatever the limit says.
- The two-crash proof's "unsynced `merge_prepared`" is a complete line that reached the file without
  fsync; "power loss" truncates the log to the length the durability ledger proves durable.

## Validation

All ten gates green locally, from the repository root, on branch head
`9d0359394e63878e51abda1ec5c54c6f94578363` (the last code commit; the commit adding this file is
documentation only and compiles nothing):

```
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features            # 2319 passed, 0 failed, 42 ignored
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
  `recover::tests::a_resume_reclaims_orphan_staging_and_an_orphan_prepared_pin_with_no_transaction`.
- **Fast no-staging.** `fast_path_publishes_exact_candidate_without_staging_or_proposal_object`: the
  hook harness records no `Worktree.AddStaging`, no `Object.ProposalCherryPick`, no
  `Ref.PinPrepared`; object count unchanged; no `merge/s<seq>` intent.
- **The three fast mismatches, live and on replay.**
  `merge_prepared_fast_with_moved_head_or_wrong_proposed_or_pin_refused_live_and_on_replay`.
- **Stale path.** `stale_candidate_takes_staging_path_and_publishes_pinned_proposal`.
- **The two-crash proof.** `recover::tests::unsynced_merge_prepared_lost_to_power_failure_keeps_log_and_ref_agreeing`
  (the ref never runs ahead of a lost unsynced `merge_prepared`);
  `recover::tests::a_resume_completes_a_prepared_transaction_whose_cas_already_ran_by_recording_the_merge`
  (kill between CAS and `task_merged` converges);
  `integrate::tests::an_append_failure_at_merge_prepared_issues_no_cas_and_leaves_the_integration_ref`
  (a sync failure before the CAS issues none); the barrier's own convergence of an unsynced tail is
  `events::log::tests::unsynced_line_lost_before_barrier_converges_to_before_append_order`.
- **Terminal-shape coverage table.** `integrate::tests::terminal_shape_coverage_table_drives_every_shape_and_each_converges_on_replay`
  drives fast, stale_clean, already_present, conflict, code_rejected, deferred and parked end to end,
  each replayed twice for equality; Declined-after-park is `fold::tests`'
  `declined_parked_verification_fails_task_consumes_queue_position_releases_lease_and_halts_per_policy`
  and Interrupted is `recover::tests`'
  `a_resume_settles_an_interrupted_stale_verification_and_reclaims_its_residue`.
- **Kill/residue per site.** The `Object.ProposalCherryPick`, `Ref.CompareAndSwapIntegration`,
  `Ref.DeletePreparedPin` and staging sites are covered by the `effects` census
  (`src/topology/effects/tests.rs`), which enumerates every site in both injection modes and both
  hook phases and the sampled cherry-pick residue class.
- **Refusals proven, not merely coded.** Third SHA / symbolic / checked-out
  (`integrate::tests`, `recover::tests`); orphan pin outside the next sequence and a second
  unresolved transaction (`fold::tests`); non-eligible starts, the three fast mismatches, `Deferred`
  at `max_defers`, non-consecutive defers, `Parked` without a question, `HumanRequired` without
  `Parked` (`fold::tests`); repair-origin dispatch and repair-admission answers refused before any
  append (`select`/`run`).
- **Verification isolation.** The integration verification runs on fresh snapshots of the proposal
  or head commit and creates no new object (`integrate::tests` stale path assertions; `effects`
  snapshot census).

## Review evidence

Implemented by Claude (Opus 4.8) running as an autonomous Claude Code session, at high effort, in
one continuous run against the frozen contract at `decisions.pr_sequence[9]` of the parallelism
packet as amended by the 2026-08-25 G2 errata. Commit-per-terminal-shape, as the packet's own
size mitigation asks.

Finished on head `9d0359394e63878e51abda1ec5c54c6f94578363` with the ten gates green locally
(`upstroke-ci` and `upstroke-pr-policy` are pull-request contexts and do not run on an unopened
branch, so local green is the bar).

The frontier review is **owed and is being run separately by the owner** at `max` on this work; it
is not part of this branch. No review has been run here, and no finding has been triaged. The ledger
below carries the canonical header and no rows.

## Risk and rollback

The largest risk is the slice's size: it introduces the transaction, every terminal, and the
recovery for each in one PR. It is mitigated by internal commits per terminal shape, the terminal
coverage table, the two-crash proof, and replay-twice-equal on every shape.

The two Class B fold changes are the sharpest review surface: `expected_head` retention changes what
`TransactionClass::Prepared` carries, and the INV-11 count changes which repair admissions the fold
accepts. Both are exercised live and on replay in `fold::tests`.

Rollback is clean: the schema-4 topology is inert unless a plan selects it, so reverting the branch
removes the machinery without touching the v0.1 path or any released behaviour. No data migration,
no on-disk format change outside the run-scoped `refs/upstroke/runs/<run>/…` namespace this slice
owns.

## Review finding ledger

| ID | Severity | Reviewed SHA / location | Failure sequence | Provenance | Category | First bad / prior ID | Regression or documented guard | Disposition |
|---|---|---|---|---|---|---|---|---|
| None yet | — | — | — | — | — | — | — | — |
