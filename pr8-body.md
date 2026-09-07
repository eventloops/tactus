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
snapshot — under the binding the candidate ran under, after the review input has been classified
for size and opacity, with an observed infrastructure failure settled as one (a Runner that
established no gate process survives, foreign Git state, a timed-out gate) and a Runner that
cannot say whether its process still runs ending the command resumably instead — and publishes
the pinned proposal. `already_present` validates the head with a no-op expected-old swap and
manufactures no commit. A conflict or a code-attributed rejection registers a complete frozen
repair — its lineage, tier ladder, path hints, acceptance, admission, and a spec whose body embeds
the rejected candidate, the rejecting head and the evidence — in the same `merge_rejected` append,
before any repair effect. Snapshots are removed only after the verification's terminal is durable.

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
names (`SnapshotName::integration_review`). The second repair round adds the process fate a
Runner reports when it fails: `Runner::run` returns `RunnerError` (`src/runner/mod.rs`,
`src/error.rs`), the host funnel classifies its failures (`src/agent/proc.rs`, `src/runner/host.rs`),
the container runner classifies from its cancel and release results (`src/runner/container.rs`,
`src/runner/container/exec.rs`), `run_review` propagates an unresolved one (`src/review.rs`), the
probe boundaries carry it through (`src/engine/topology/preflight.rs`, `create.rs`), and the
review-input classifier is split so the integration consults its size/opacity half
(`src/engine/classify.rs`). Every Runner test double changed signature with the trait. The third
repair round changes the same seam again and nothing else: the host funnel claims `Gone` only from
group evidence and its Windows spawn boundary carries the fate its own cleanup established
(`src/agent/proc.rs`); the container runner's launch records whether `docker start` was attempted,
its release reports process evidence apart from which cleanup steps completed, and a container the
runtime cannot confirm stopped keeps its Git view and intent for the next census
(`src/runner/container.rs`, `src/runner/container/exec.rs`); the engine gains a read-only count of
cancelled reservations for a test (`src/engine/topology/identity.rs`, `run.rs`).

Out of scope, and neither built nor stubbed: repair **execution** (PR9), the production writer, the
slot broker. A PR8 build refuses, before any append, dispatch of a Repair-origin task and any
repair-admission answer. No attributed `DesignDefect` record is emitted; questions carry no
attribution.

**`src/topology/**` freeze — Class B changes, approved by the owner on 2026-09-07.** Three changes
touch the frozen fold layer. All are behavioural, each was approved per-instance against the
description below, and each description was verified against the code by the `gpt-6-astra`
record pass of 2026-09-07:

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
- A Runner error settles a terminal only when the Runner established that no process survives:
  `RunnerSpawnFailure` when none was started, `Infrastructure{Other}` when one may have started and
  is established gone, and no terminal at all — the command ends resumably with the transaction
  open and the snapshot retained — when the Runner cannot say; the next resume's census reclaims
  the container before the verification is settled interrupted (R25). The fate is process
  evidence kept apart from cleanup completion: the host funnel says `Gone` only when the process
  group (Unix) or the job (Windows) was established empty, never from the direct child's reap; the
  container runner says `NeverStarted` for any failure before `docker start` was attempted and,
  after it, `Gone` only when the runtime observed the exit or confirmed a stop or a forced removal,
  retaining a container's view and intent whenever it confirmed neither (R25, third repair round).
  Foreign Git state observed by the verification and a gate that times out are outages of the
  sequence (`Infrastructure{Other}`, R26, R27); every other verification error ends the command
  resumably (R24).
- The integration diff is classified for size and opacity and the review-input policy consulted;
  the attempt path's Test-provenance rule is not applied to it (R28).
- An integration's judged reviews are charged to the candidate's task and the run at the
  verification; on replay they are read off `merge_prepared` and `merge_rejected`. An unavailable
  terminal carries no review record, so a park's or an outage's review is charged live and lost on
  replay: the contract requires it recorded and the frozen vocabulary cannot carry it, an owner
  decision owed (`PR8-R2-SPEND-REPLAY`; R22 withdrawn).
- With no publication pending, the authorized integration head is the log's latest `task_merged`,
  or the recorded base before any; a ref elsewhere, or absent after a publication, refuses before
  any append (R23).
- The two-crash proof's "unsynced `merge_prepared`" is a complete line whose flush was made to fail
  after the full write; the restart is a child process killed at the `task_merged` write; "power
  loss" truncates the log to the length the barrier proved durable.

## Validation

All ten gates green locally, from the repository root, on the last code commit of this branch,
`287563f0ff7ff9e5736170fa574cfad8b6dc147d`; the commit that follows it changes the three record
files and no code, and the ten gates were rerun on it before the push:

```
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features            # 2351 lib tests passed, 0 failed, 43 ignored (Linux)
cargo +1.85.0 check --locked --all-targets --all-features
bash .github/scripts/test-release-record.sh
bash .github/scripts/test-pr-policy.sh
bash .github/scripts/test-pr-ledger-evidence.sh
bash .github/scripts/test-docs-consistency.sh
bash .github/scripts/test-internals-notes.sh
bash .github/scripts/test-pr-ready-audit.sh
```

The first full run at that head had one red, in a module this branch does not touch:
`workspace_manager::tests::sampled_git_child_kills_every_residue_classified_and_recovered` refused
one `Worktree.Add` sample with "worktree list record 1 names a HEAD but neither a branch nor a
detached checkout" — the standing P3 `PR172-SAMPLER-REFUSED-A-TORN-WORKTREE-LIST-RECORD`
(`reviews/findings/`), which asks for a count before anything is called a flake. Sighting recorded
here: it passed alone at this head, and the full test gate rerun at the same head passed clean;
the ten gates named above are green on that rerun.

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
  `a_gate_whose_runner_lost_it_after_start_and_reclaimed_it_defers_as_an_outage`,
  `a_git_error_observed_by_the_verification_settles_an_infrastructure_outage`,
  `a_gate_that_times_out_during_integration_verification_defers_instead_of_registering_a_repair`,
  `an_unjudgeable_proposal_parks_the_candidate_for_a_person`,
  `a_test_candidate_whose_test_was_already_published_is_verified_not_rejected_for_provenance`,
  `an_integration_review_is_selected_against_the_candidates_recorded_implementer`,
  `an_integration_reviews_cost_reaches_the_run_spend`,
  `a_paid_review_that_parks_is_charged_live_and_its_replay_loss_is_the_deferred_vocabulary_gap`,
  `a_verification_park_answer_is_ingested_at_the_hard_block_and_a_repair_admission_answer_is_refused_before_any_append`.
- **A lost gate process is never settled over.**
  `a_runner_that_loses_track_of_a_running_gate_refuses_resumably_and_reclaims_nothing` (the
  production `ContainerRunner` over the fake runtime with observe, stop and remove unreachable:
  the step ends in an error, the container survives running, its snapshot and intent are
  retained, no terminal is appended) and
  `a_lost_gate_container_is_reclaimed_by_the_next_resume_before_the_verification_is_settled` (a
  resume refuses while the runtime is unreachable and appends nothing; with it back the census
  stops and removes the container before any recovery event, then the verification settles
  interrupted, the snapshot goes, and the candidate re-verifies under the next sequence). The
  Runner's own classification:
  `runner::container::exec::tests::the_runner_reports_what_it_established_about_the_process_when_it_fails`,
  `review::tests::an_unresolved_runner_error_propagates_instead_of_reporting_the_review_unavailable`.
- **The fate is process evidence, at both Runners** (the third repair round). The container
  runner: `the_runner_reports_what_it_established_about_the_process_when_it_fails` is a
  thirteen-cell matrix over the fake runtime — the runtime down before `docker create`, a refused
  create, an image mismatch, a refused start, a committed start whose launch then fails, a start
  lost with the cancel establishing nothing, an observation lost with the release completing, a
  lost stop followed by a successful forced removal, all three lost, an observed exit followed by
  a lost collection and release, and three timed-out outputs (a failed view discard only, a failed
  stop with a successful removal, neither stop nor removal) — each asserting the fate, whether
  `docker start` was attempted, what survived and in what state, and whether the view and intent
  were retained; `a_container_the_runtime_cannot_confirm_stopped_keeps_its_mounted_git_view_and_intent`
  keeps both and reclaims them through the intent once the runtime is back;
  `recover::tests::repeated_container_launch_outages_before_start_consume_defers_through_the_production_runner`
  drives three restarts of the production runner over a runtime lost at `docker create` each time
  (Deferred 1, Deferred 2, Parked, no `docker start` ever issued). The host funnel:
  `agent::proc::tests::a_reaped_leader_does_not_prove_its_group_gone_when_the_reaper_failed`
  (Linux: the leader killed and reaped, a same-group `sleep` verified running, the fate
  `Unresolved`), `a_lost_group_settles_unresolved_even_when_the_leader_reaps_cleanly`,
  `an_established_group_settles_gone_whatever_the_leaders_own_reap_says`,
  `a_containment_failure_after_the_spawn_leaves_the_fate_unresolved` (the three post-spawn
  containment points), `a_spawn_that_fails_before_any_process_exists_is_never_started`, and on
  Windows `a_windows_spawn_that_fails_after_creation_leaves_no_suspended_stub` (now asserting the
  boundary's fate) and `a_windows_spawn_that_fails_before_creation_is_never_started`. The
  entitlements after an unresolved verification:
  `recover::tests::an_unresolved_verification_leaves_its_entitlements_with_the_open_transaction`
  (the transaction open, the fold counting it as holding the pipeline entitlement, the provisional
  reservation converted at the start append and not cancelled, a second step admitting nothing).
- **The production verifier, observed through the loop.**
  `the_production_verifier_judges_the_recorded_proposal_and_removes_its_snapshots_after_the_terminal`:
  for the prepared, rejected and parked shapes the gate ran on a checkout whose HEAD is the
  recorded proposal and never the candidate commit, and every snapshot removal began with the
  sequence's terminal as the last durable event.
- **The frozen repair spec.** `repair::tests::the_frozen_repair_spec_embeds_the_rejection_evidence_and_both_shas`:
  a conflict's paths, a code rejection's verdict, gates, review passes and detail, the rejected
  candidate's commit and ref, and the rejecting head, all in the registered spec's body.
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
  row names: the reviewer's m01, m02, m03, m05 and m06; the barrier sync removed; one mutation
  per repair of the first round; and, for the second round, the blanket Runner-error conversion
  restored, the Git arm removed, the timed-out-gate check disabled, the provenance rule restored
  at integration, the spec body not embedded, the two snapshot mutations the reviewers found
  surviving, and the sampler's kill deleted; and, for the third round, the two mutations that
  survived the suite at `79ddbffb` (M3, the host's post-spawn `Unresolved` made `Gone`; M4, a
  timed-out output with a failed release made `Gone`) and one mutation per repair of the round
  (`pr8-plan.md` §5).

## Review evidence

Implemented by Claude Fable 5.1 at max effort, running as an autonomous Claude Code session in
one continuous run against the frozen contract at `decisions.pr_sequence[9]` of the parallelism
packet as amended by the 2026-08-25 G2 errata, finishing on head
`9d0359394e63878e51abda1ec5c54c6f94578363` with the ten gates green locally; the body commit
`3414dc5861c9a523342ea0792a54f0129cf82f2f` followed. Commit-per-terminal-shape, as the packet's
own size mitigation asks. Both repair rounds below were Claude Fable 5.1 at max effort, each a
fresh autonomous session. (An earlier version of this paragraph named Opus 4.8 at high effort;
the plan named the correct model and the body was wrong.)

Three independent frontier reviews were then run by the owner against the exact head `3414dc58`,
each by `gpt-6-astra` at `max` effort, each with a different lens and each returning
`CHANGES_REQUIRED`:

- **conformance to the frozen design** — the only reviewer given the design contract: findings
  F1–F9, an audit of the twenty recorded readings, and a freeze-classification audit;
- **crash consistency and recovery** — six findings, three of them P1, each with a reproduction
  the reviewer ran;
- **test adequacy and code quality** — thirteen findings from a 24-mutation audit.

Every finding was triaged in `pr8-triage.md` (Claude Fable 5.1 at max effort, one continuous
autonomous session, 2026-09-07) and every one was confirmed; none was rejected. Every confirmed
finding was repaired on this branch at `916852c9`, one finding or closely related group per
commit, each with a test that fails at `3414dc58` or a mutation witness replayed against the
repair, except one that is beyond this slice's reach and is deferred with a ledger row and an
owner decision owed (`PR8-CRASH-002`). The two documentation defects the owner was being asked to
approve on — the over-limit `HumanBinding` description and the empty-intersection ladder — are
corrected: the description to what the fold does, and the code to what R10 recorded.

After that push, CI's macOS leg failed once in the residue sampler: a killed `git cherry-pick`
left `packed-refs.lock` in the common git dir — the Ref-lock residue class `PR8-CRASH-002`
defers — and the sampler now removes git's ref-lock residue after each kill as an operator
would. The prose this pull request had put into source files then moved to the modules' notes
under `docs/internals/` (CODING_STANDARDS §13), with notes files created for `integrate.rs`,
`repair.rs` and their test modules; that commit changes no code.

Three more independent frontier reviews were then run by the owner against the exact head
`916852c9`, over the repair range `3414dc58..916852c9` only, each by `gpt-6-astra` at `max`
effort, three lenses — repair adequacy (four issues), regression (two issues, the first the most
serious finding of the round), record honesty (four numbered findings and the record
discrepancies) — and each returning `CHANGES_REQUIRED`. They confirmed the two-crash proof sound,
the three Class B descriptions accurate, the freeze classification complete and the non-goal
boundary held, and found the round's outage handling had made something worse: every Runner error
was settled as a spawn failure, so a gate whose container Docker had lost was released, its
snapshot reclaimed and the next sequence admitted beside it. Every finding was triaged in
`pr8-triage.md` §5 (Claude Fable 5.1 at max effort, a fresh autonomous session, 2026-09-07) and
confirmed; none was rejected. Every confirmed finding was repaired on this branch with a test that
fails without the repair, except the spend replay gap, which the contract settles against the
frozen vocabulary and which is deferred with a ledger row and an owner decision owed
(`PR8-R2-SPEND-REPLAY`); R22, which had called it a permitted reading, is withdrawn.

A single frontier review was then run by the owner against the exact head `79ddbffb`, over the
repair range `916852c9..79ddbffb` only — the change to the `Runner` trait's error type, the one
thing in the round no reviewer had seen — by `gpt-6-astra` at `max` effort, returning
`CHANGES_REQUIRED` with five findings, three P1: the host claimed `Gone` from a reaped leader while
a same-group descendant lived, a container outage before `docker create` was `Unresolved` and
consumed no defer while a committed start was `NeverStarted`, an `Unresolved` release still pruned
the Git view the live container had mounted, positive evidence of exit and removal was discarded,
and the unresolved return was said to release both entitlements; two mutations had survived the
whole suite. Every finding was triaged in `pr8-triage.md` §6 (Claude Fable 5.1 at max effort, a
fresh autonomous session, 2026-09-07). Four were confirmed and repaired on this branch, each with
a test that fails without the repair, and both surviving mutations are killed. The fifth is
rejected as a code defect — a probe at the reviewed head shows the reservation converted at the
start append and the cancellation not reached, the entitlements being the open transaction's — and
confirmed as a record defect: the sentence that misled the reviewer was this branch's own triage,
now corrected, and the accounting is pinned by a test. The reviewer confirmed the rest of the seam
sound (the adapters, `gates.rs`, the v0.1 worker, `run_review`, no accidental catch-all), and
none of that was touched.

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

The second repair round changes the `Runner` trait's error type crate-wide: every Runner —
the host and container runners, the probe boundaries, and every test double — now says what it
established about the process when it fails, and the third repair round fixes what each of the
two Runners may claim. The v0.1 path is affected in type and in one exceptional path: the host
runner's errors are the same errors with a fate attached, and `run_review` reports a reviewer
unavailable exactly as before except when that fate is `Unresolved`, which it propagates as an
error instead. The host funnel says `Gone` only when the process group (Unix) or the job (Windows)
was established empty, and `Unresolved` whenever it returned without that — a containment failure
after the spawn, a reaper that failed, a Windows job cleanup that failed after the direct child
exited — the direct child's own reap never being the evidence. (An earlier version of this
paragraph said the funnel reports `Unresolved` only when its kill was not reaped; the review of
`79ddbffb` showed that false in both directions, and the code now matches this sentence.) The
Windows branch of the funnel was type-checked and clippy-clean for `x86_64-pc-windows-msvc` on the
build box and executes on the winguest CI leg; it was not run locally.

Two known gaps were deferred by the owner on 2026-09-07, each with a standing finding filed in
`reviews/findings/`. `PR8-CRASH-002`: a lock file left by a coordinator
killed inside `git update-ref`; the refusal it causes is resumable and loses nothing, and the
operator's removal of the lock lets the next resume complete the publication; reclaiming it needs
a `Ref.*` residue class in the frozen inventory. `PR8-R2-SPEND-REPLAY`: a paid review that parks
or meets an outage is charged live and lost on replay, so a restart can admit an integration the
previous incarnation's total would have refused; recording it needs a review record on the
unavailable terminal, which the frozen vocabulary lacks.

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
| PR8-TESTS-003 | P1 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/run.rs:677 | an integration review costs 2.50 -> Spend unchanged live and on replay -> the next selection admits work below an apparent ceiling the real spend has passed | introduced_by_feature | correctness | 2d1b4c72 | `an_integration_reviews_cost_reaches_the_run_spend` pins the live charge and the replay of the records merge_prepared and merge_rejected carry; the unavailable terminals' replay is PR8-R2-SPEND-REPLAY | fixed |
| PR8-TESTS-005 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/scaffold.rs:719 | the scaffold verifier ignores its workspace and no test supplies a gate -> VerifyRequest.proposed replaced by the candidate SHA -> all fourteen integration tests pass | introduced_by_feature | correctness | 2d1b4c72 | `stale_candidate_takes_staging_path_and_publishes_pinned_proposal` asserts the gate's workspace HEAD is the recorded proposal | fixed |
| PR8-TESTS-006 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/integrate/tests.rs:859 | the swap skipped whenever expected_head equals proposed_sha -> unchanged HEAD and object count still hold -> all fourteen integration tests pass | introduced_by_feature | correctness | 9d035939 | `an_already_present_candidate_settles_without_an_empty_commit` counts exactly one compare-and-swap | fixed |
| PR8-TESTS-007 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/integrate/tests.rs:952 | the registered repair's intent and worktree created after merge_rejected -> only the last event kind and staging intents checked -> all fourteen integration tests pass | introduced_by_feature | correctness | 9d035939 | `a_conflicting_candidate_is_rejected_with_an_atomic_repair_before_any_repair_effect` asserts no task worktree effect after the rejection | fixed |
| PR8-TESTS-008 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/topology/fold/tests.rs:6402 | the over-limit test replaced by limit == 0 -> a lineage with a positive limit admits automatic repairs without bound -> the entire library suite passes | introduced_by_feature | correctness | b31098db | `a_lineage_that_has_consumed_its_allowance_registers_only_a_human_required_repair` | fixed |
| PR8-TESTS-010 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/recover/tests.rs:6332 | the fixture plants an intent and a pin and no staging worktree -> the production remove_worktree call removed -> the interrupted-cleanup test passes | introduced_by_feature | correctness | 43d62194 | `a_resume_settles_an_interrupted_stale_verification_and_reclaims_its_residue` plants and asserts the worktree and drives the re-verification | fixed |
| PR8-TESTS-013 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/repair.rs:82 | the whole root entry cloned without a stated reason -> its nested fields cloned again -> an empty_intersection flag duplicates the ladder's admission | introduced_by_feature | correctness | 2d1b4c72 | standards section 6, held by review: the builder borrows the root, clones the specification once, and reads the admission off the ladder | fixed |
| PR8-AUDIT-R8 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/topology/fold/check_integration.rs:471 | the body and reading R8 claim an over-limit HumanBinding admission is refused -> the fold accepts it on either side of the limit -> the owner asked to approve a Class B change on a description of what it does not do | introduced_by_feature | docs-contract | 3414dc58 | `a_lineage_past_its_repair_limit_registers_only_a_human_required_repair` accepts HumanBinding at the limit, and R8 now says so | fixed |
| PR8-AUDIT-R10 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/repair.rs:190 | the mid floor intersects the root's ladder empty -> the root's tiers, floor and ceiling kept and its excluded rungs offered -> reading R10 promised the raised floor, no ceiling and the probed agents | introduced_by_feature | docs-contract | 2d1b4c72 | `an_empty_tier_intersection_registers_a_human_binding_ladder_with_the_allowed_agents` | fixed |
| PR8-R2-RUNNER-LIVENESS | P1 | 916852c9638383af3964f743d48e14bf859138c3 / src/engine/topology/run.rs:257 | merge_verification_started durable -> the gate's container starts -> Docker unreachable so observe, stop and remove fail -> the Runner error is settled as RunnerSpawnFailure -> Deferred appended, both entitlements released, the snapshot the container has mounted removed -> after the defer wake a second sequence starts beside the surviving gate | fix_regression | crash-consistency | f2e0c69f | `a_runner_that_loses_track_of_a_running_gate_refuses_resumably_and_reclaims_nothing` and `a_lost_gate_container_is_reclaimed_by_the_next_resume_before_the_verification_is_settled`; the Runner's fate is pinned by `the_runner_reports_what_it_established_about_the_process_when_it_fails` | fixed |
| PR8-R2-SPEND-REPLAY | P1 | 916852c9638383af3964f743d48e14bf859138c3 / src/engine/topology/select.rs:52 | 1.20 spent -> a 2.50 review returns needs_human -> live total 3.70 -> restart -> replay restores 1.20 -> the selector admits an integration under a 2.20 ceiling the previous incarnation refused | introduced_by_feature | correctness | 2d1b4c72 | `a_paid_review_that_parks_is_charged_live_and_its_replay_loss_is_the_deferred_vocabulary_gap` pins the live charge, the in-incarnation refusal, and the replayed total as exactly what the frozen unavailable terminal can carry; decisions.coordinator_integration.dispositions requires the spend recorded and MergeVerificationUnavailable has no review record, so the field is a Class C change owed an owner decision (or an erratum) | deferred |
| PR8-R2-GIT-STATE | P1 | 916852c9638383af3964f743d48e14bf859138c3 / src/engine/topology/run.rs:210 | merge_verification_started durable -> the disposable staging index is corrupt -> the review-input reader returns a Git error -> the ? propagates it with no terminal -> resume settles the sequence interrupted and the observed failure never reaches the defer accounting | introduced_by_feature | correctness | f2e0c69f | `a_git_error_observed_by_the_verification_settles_an_infrastructure_outage` | fixed |
| PR8-R2-GATE-TIMEOUT | P2 | 916852c9638383af3964f743d48e14bf859138c3 / src/engine/topology/attempt.rs:574 | a gate reaches its timeout -> the Runner returns timed_out with no verdict -> the ordinary gate-failure branch classifies it GateFailed -> merge_rejected registers a repair for a timeout | introduced_by_feature | correctness | 2d1b4c72 | `a_gate_that_times_out_during_integration_verification_defers_instead_of_registering_a_repair` | fixed |
| PR8-R2-FROZEN-SPEC | P2 | 916852c9638383af3964f743d48e14bf859138c3 / src/engine/topology/repair.rs:60 | a candidate is rejected with failing gate evidence -> the repair is registered -> its spec is the root spec with kind, hints and one acceptance line changed -> neither the evidence nor either SHA is in the spec PR9 dispatches from | introduced_by_feature | docs-contract | 2d1b4c72 | `the_frozen_repair_spec_embeds_the_rejection_evidence_and_both_shas` | fixed |
| PR8-R2-TEST-PROVENANCE | P2 | 916852c9638383af3964f743d48e14bf859138c3 / src/engine/topology/run.rs:189 | a Test candidate adds a test and a helper -> another candidate publishes the identical test first -> the cherry-pick leaves a helper-only integration diff -> diff_failure rejects it for adding no test code and a repair is registered | fix_regression | correctness | f2e0c69f | `a_test_candidate_whose_test_was_already_published_is_verified_not_rejected_for_provenance` | fixed |
| PR8-R2-SNAPSHOT-ORACLE | P2 | 916852c9638383af3964f743d48e14bf859138c3 / src/engine/topology/integrate/tests.rs:1162 | the production verifier snapshots the candidate commit, or removes snapshots as each role finishes -> the asserting tests use the scaffold's own verifier and the loop tests' runner ignores its workspace -> all 354 engine-topology tests pass | introduced_by_feature | correctness | 2ff9f14a | `the_production_verifier_judges_the_recorded_proposal_and_removes_its_snapshots_after_the_terminal` | fixed |
| PR8-R2-SAMPLER-ORACLE | P2 | 916852c9638383af3964f743d48e14bf859138c3 / src/engine/topology/recover/tests.rs:8098 | the sampler's child.kill() is deleted -> every cherry-pick completes -> eight classifications of clean picks satisfy the purported kill proof | introduced_by_feature | correctness | d14e1db0 | `sampled_cherry_pick_child_kills_every_residue_classified_and_recovered` now requires each child to die by the kill or to have completed, and at least one to have died by it | fixed |
| PR8-R2-UNREACHABLE | P2 | 916852c9638383af3964f743d48e14bf859138c3 / src/engine/topology/recover/tests.rs:6491 | recovery returns before the armed append, on a third-SHA refusal -> let _ discards the result -> a bare unreachable! ends the child with no report of why | introduced_by_feature | docs-contract | 69463499 | standards section 7, held by review: `two_crash_kill_child` reports what recovery returned into the parent's report file and fails plainly; no unreachable! was added by this pull request | fixed |
| PR8-R2-RECORD-PROSE | P3 | 916852c9638383af3964f743d48e14bf859138c3 / docs/internals/engine/topology/integrate.md:323 | the prose relocation preserves three statements false against the code -> a StaleNotImplemented refusal that does not exist, a head check skipped under any transaction, a person asked below the limit refused | introduced_by_feature | docs-contract | 4de4c74e | corrected to what `integrate_stale`, `ensure_recorded_integration_ref` and `check_merge_rejected` do; `test-internals-notes.sh` holds the pointers, review holds the sentences | fixed |
| PR8-R2-RECORD-PROVENANCE | P2 | 916852c9638383af3964f743d48e14bf859138c3 / pr8-body.md:194 | the body names Opus 4.8 at high effort and the review script's default reviewer -> the plan names Fable 5.1 at max -> MAINTAINING.md requires the frontier reviewer model and effort in the body and the body names what did not run | introduced_by_feature | docs-contract | 3414dc58 | the settled values from the owner and the worktree model-intent files are in the Review evidence section; `validate-pr-body.sh` holds the sections | fixed |
| PR8-R3-HOST-GROUP-GONE | P1 | 79ddbffbb5fb087bfb89f88321a3f60a2a88bdb1 / src/agent/proc.rs:442 | a gate forks a same-group descendant -> the reaper is lost and Supervisor.finish fails -> the direct child is killed and reaped -> settle_failed_supervision sets Gone from the reap -> the integration settles Infrastructure{Other} and disposes the snapshot beside the live descendant; on Windows Gone is stored before the job is observed empty, and a spawn that fails after CreateProcess discards its cleanup evidence behind NeverStarted | fix_regression | correctness | cd4610f6 | `a_reaped_leader_does_not_prove_its_group_gone_when_the_reaper_failed`, `a_lost_group_settles_unresolved_even_when_the_leader_reaps_cleanly`, `a_containment_failure_after_the_spawn_leaves_the_fate_unresolved` | fixed |
| PR8-R3-CONTAINER-START | P1 | 79ddbffbb5fb087bfb89f88321a3f60a2a88bdb1 / src/runner/container/exec.rs:631 | Docker is down before docker create -> create, stop and remove all fail -> cancelled classifies Unresolved though no process can exist -> the command ends resumably instead of deferring, and four such outages consume no defer; conversely a committed docker start followed by a clean cancel is NeverStarted and settles RunnerSpawnFailure | fix_regression | correctness | cd4610f6 | `the_runner_reports_what_it_established_about_the_process_when_it_fails`, `repeated_container_launch_outages_before_start_consume_defers_through_the_production_runner` | fixed |
| PR8-R3-CONTAINER-RETAIN | P1 | 79ddbffbb5fb087bfb89f88321a3f60a2a88bdb1 / src/runner/container.rs:527 | observe, stop and remove unreachable -> cancel_reached prunes the R19 view and removes the R26 intent after both failures -> the running container's mounted Git metadata is deleted and no record names the residue | fix_regression | crash-consistency | cd4610f6 | `a_container_the_runtime_cannot_confirm_stopped_keeps_its_mounted_git_view_and_intent` | fixed |
| PR8-R3-CONTAINER-EVIDENCE | P2 | 79ddbffbb5fb087bfb89f88321a3f60a2a88bdb1 / src/runner/container.rs:518 | a stop fails and the forced removal succeeds, or a timed-out output's release fails only at the view discard, or an observed exit precedes a failed stop -> container_released is false -> Unresolved -> an observed outage ends the command as an interruption and consumes no defer | fix_regression | correctness | cd4610f6 | `the_runner_reports_what_it_established_about_the_process_when_it_fails` | fixed |
| PR8-R3-RECORD-ENTITLEMENTS | P2 | 79ddbffbb5fb087bfb89f88321a3f60a2a88bdb1 / pr8-triage.md:137 | the round-two triage says the caller's cancel releases both entitlements after an unresolved verification -> the reviewer measures the reservation ledger at 0 and reports a release -> the reservation had converted at merge_verification_started and the open transaction holds the entitlements, and nothing cancels them | introduced_by_feature | docs-contract | 1e2a0733 | `an_unresolved_verification_leaves_its_entitlements_with_the_open_transaction` pins the conversion, the open transaction's holding and that a second step admits nothing; the triage sentence is corrected | fixed |
