# PR8 plan — integration transactions

Branch `feat/pr8-integration-transactions`, cut from `master` at `44a33caa`. Contract:
`decisions.pr_sequence[9]` of `tactus-parallel-design-neutral-v16.json`, as amended by
`2026-08-25-g2-pass-errata.md` (the errata win on conflict; none of E1–E6 touches PR8's own
sentences, so the JSON text of the slice contract is the text implemented). Brief:
`START-PR8.md` as amended on 2026-09-06 (no pull request, no review, no findings lane; the body
goes to `pr8-body.md`), then `START-PR8-FIX.md` of 2026-09-07 for the repair round after the
three reviews of `3414dc58`, then `START-PR8-FIX2.md` of the same day for the second repair round
after the three reviews of the repair range `3414dc58..916852c9` (`pr8-triage.md` §5), then
`START-PR8-FIX3.md` of the same day for the third repair round — the Runner seam only — after the
single review of the repair range `916852c9..79ddbffb` (`pr8-triage.md` §6).
Implementation model: Claude Fable 5.1 at max effort, the implementation and all three repair
rounds.

This file is the standing plan and the record of every reading taken where the packet was
ambiguous. Each reading is a decision, not a question. It is updated as commits land. Where the
conformance review of `3414dc58` found that a "reading" claimed a discretion the contract did not
offer, the entry now says so and cites the passage that settles it.

## 1. Reading report

### What already exists (nothing below is re-implemented)

- The whole event vocabulary of the slice is PR3's and is on the wire already:
  `MergeVerificationStarted{sequence, candidate, basis: StaleClean{prepared_ref} | AlreadyPresent,
  expected_head, proposed_sha}`, `MergeVerificationUnavailable{sequence, cause, outcome}`,
  `MergeVerificationInterrupted`, `MergePrepared{disposition: Fast | StaleClean |
  AlreadyPresent, expected_head, proposed_sha, candidate_sha, candidate_ref, prepared_ref,
  verification_source, verification, satisfies}`, `MergeRejected{disposition: Conflict{paths} |
  CodeRejected{verification}, repair: FrozenSpawn, lease_effect}`, `TaskMerged{sequence,
  merged_sha, satisfies, lease_release}`, `DeferWaitElapsed`.
- The checked fold already refuses the slice's relations: one transaction at a time, dense
  sequences, first-eligible start, the exact-base case refused as a verification, the three
  `merge_prepared` relation families (fast / stale_clean / already_present), `satisfies` closure
  equality, Deferred at `max_defers`, non-consecutive defers, Parked without a complete question,
  HumanRequired without Parked, `task_merged` against a non-authorized transaction, lease
  release shape, and the repair registration shape (dense key, lineage root/parent/index,
  deps Merged, ladder well-formed, admission consistent with the entry).
- The funnels exist: staging slot `merge/s<seq>` (`Worktree.WriteStagingIntent/AddStaging/
  RemoveStaging/RemoveStagingIntent`), `Object.ProposalCherryPick`, `Ref.PinPrepared`,
  `Ref.DeletePreparedPin`, `Ref.CompareAndSwapIntegration`, exact snapshots of a commit
  (`SnapshotInput::Commit`, "no new object"), `assert_publishable`, `direct_ref_target`,
  `refs_under`, `refuse_unexpected_refs`, the residue classifier for the cherry-pick residue
  class, and the stable-prefix barrier of recovery step (a1) with its typed witness chain.
- The loop selects `Step::Integrate` for the first eligible candidate and the checkpoint refuses
  it; recovery refuses an unresolved transaction. Both refusals are PR7's and are replaced.

### What the slice obliges (in my words)

1. **Selection and reservation.** Ceiling check before selection (already in `select`); a
   provisional `{pipeline, merge}` reservation (`ReservationKind::Integration`, 2 entitlements)
   taken before any staging effect and converted at the first append — `merge_prepared(fast)`,
   `merge_verification_started`, or `merge_rejected(conflict)` — or cancelled on any pre-append
   failure, leaving the candidate queued and only reclaimable residue.
2. **The exact-base decision** is made under `assert_publishable` from the integration ref head
   `H` read through `direct_ref_target`, before any staging effect. `H == candidate.base_sha`
   (the base `candidate_prepared` recorded) is fast; anything else is stale.
3. **Fast**: `merge_prepared(fast){expected_head: H, proposed_sha: candidate.commit,
   prepared_ref: None, verification_source: CandidatePrepared{key, generation}, verification:
   None, satisfies}` → CAS `H -> candidate.commit` → `task_merged`. No staging worktree, intent,
   cherry-pick, object or pin at any point of the sequence; the hook harness records none of
   `Worktree.AddStaging`, `Object.ProposalCherryPick`, `Ref.PinPrepared` inside the fast
   sequence; `git fsck` object count unchanged; no `merge/s<seq>` intent ever written.
4. **Stale**: staging intent → `add_worktree(staging, H)` → `proposal_cherry_pick(candidate)`:
   clean → `prepared/<seq>` pin zero-old at the proposal → `merge_verification_started
   {StaleClean{prepared_ref}, expected_head: H, proposed_sha: proposal}`; empty → `merge_
   verification_started{AlreadyPresent, expected_head: H, proposed_sha: H}`; conflict →
   `merge_rejected{Conflict{paths}}` with the complete frozen repair. Verification first classifies
   its input (a review diff too large or opaque, or a tree the review-input policy refuses, is a
   `HumanRequired` park before any process runs), then runs every recorded gate on one fresh exact
   snapshot of the proposal (or head) commit and every review pass on its own fresh snapshot,
   through the Runner, with `InvocationId::sequence(seq, role, ordinal)` identities, the review
   passes selected against the binding the candidate ran under; the staging worktree runs
   nothing.
5. **Terminals** of `merge_verification_started`, each implemented: `merge_prepared(stale_clean |
   already_present)` carrying the passing verification record → CAS → `task_merged`;
   `merge_rejected{CodeRejected{verification}}` with the frozen repair; `merge_verification_
   unavailable{HumanRequired{verdict}, Parked{question}}`; `merge_verification_unavailable
   {Infrastructure{kind}, Deferred{defers} | Parked{question}}` — a reviewer outage as the review
   result reports it, and a Runner that cannot run a gate as `RunnerSpawnFailure`;
   `merge_verification_interrupted` appended by resume for a dangling start.
6. **Repair registration** inside `merge_rejected`: complete `FrozenSpawn` (key = registry.len(),
   display id `merge-fix-<index>-<root>`, origin MergeRepair, kind Fix, evidence in the spec,
   original acceptance plus preserve-merged-behaviour, path hints widened by the candidate's
   actual paths and the conflict paths, `min_tier = mid` intersected with the root's frozen
   floor/pin/ceiling, root's deps, root's reviews, allowed agents, lineage {root, parent, index}),
   admission Runnable | HumanRequired{limit} (over the frozen `max_merge_repairs`) |
   HumanBinding{options} (empty intersection), and the lease effect (CreatesLineage from an
   ordinary candidate, WidensLineage for a member).
7. **Cleanup after the terminal**: the verification's snapshots left in place by the judge and
   removed with force once the terminal is durable (`merge_prepared`, `merge_rejected`, or
   `merge_verification_unavailable`); the staging worktree and its intent removed with force after
   the terminal; the prepared pin deleted expected-old at the proposal the record names at
   Deferred/Parked/interrupted terminals and, for a prepared publication, after `task_merged`.
8. **Recovery** rows T-FAST, T-PROPOSAL, T-VERIFY, T-PREPARED, T-REJECT(registration): every
   CAS or promotion only after the barrier of step (a1); a `Prepared` transaction resolves by
   ref == expected → CAS then `task_merged`, ref == proposed → `task_merged`, anything else
   refuses (third SHA), symbolic or checked out refuses through `assert_publishable`; a
   `VerificationStarted` transaction settles `merge_verification_interrupted`, deletes its pin
   expected-old at the recorded proposal, and the candidate is re-verified under a new sequence;
   staging residue is reclaimed with force from intents, snapshot residue after every terminal a
   snapshot could belong to; pins are handled by their records (R15); with no publication pending
   the integration ref must name the last recorded publication, or the base before any (R23).
9. **Checkpoint refusals** (part of the slice): dispatch of a Repair-origin task and
   repair-admission answers are refused before any append, with the citation pattern of
   `run.rs`/`recover.rs`. Run-end closure stays refused (PR10).
10. **Verification-park questions** are PR8's: the permitted transitions name "parked verification
    Answered -> AwaitingMerge; Declined -> Failed (queue position consumed; lease released or
    lineage failed; halting per decline_halts_run)", and the proof tests name park/answer/decline.
    So the loop ingests answers to *verification-park* questions and refuses answers to
    *repair-admission* questions before any append.

### Readings taken where the text is ambiguous or silent

- **R1. Where the prepared pin of a published proposal is pruned.** R12 says "pruned by
  finalization when prepared" while `cleanup` says "prepared pins pruned after terminal" and the
  site `Ref.DeletePreparedPin` is `After(task_merged)`. Reading: delete the pin expected-old
  right after `task_merged` (the object stays reachable from the integration ref); a pin that
  survives a kill between `task_merged` and its deletion is pruned by the next resume at its
  recorded proposal sha — and refused at any other. PR10's finalization stays free to prune
  whatever is left.
- **R2. `already_present`'s "validation-only no-op CAS".** *Settled by the contract, not a
  reading*: `transaction_fault_matrix[T-PREPARED].resume_action` names the validation-only no-op
  and `C.side_effect_vs_event_ordering` puts the CAS before `task_merged`. Implementation note:
  the real expected-old update `update-ref --no-deref <ref> H H` is issued through
  `Ref.CompareAndSwapIntegration`, so the expected head is validated atomically by Git and the site
  is observed (`an_already_present_candidate_settles_without_an_empty_commit` counts it); nothing
  moves.
- **R3. Empty cherry-pick detection.** Git reports an already-present change as a failed
  cherry-pick (exit 1, `CHERRY_PICK_HEAD` left, index equal to HEAD, no unmerged paths). Reading:
  the funnel `proposal_cherry_pick` is unchanged; a read-only inspection `proposal_state`
  classifies the staging worktree after a failed pick into Conflict{unmerged paths} or Empty,
  and any other failed state is the Git error it was. The residue a failed pick leaves in the
  staging git dir leaves with the forced removal after the terminal.
- **R4. Which failures are infrastructure at integration.** Only partly a reading: the
  settlement of an infrastructure failure is mandated by T-VERIFY and INV-23; what is chosen is
  the mapping of each result. Reviewer `Unavailable` with RateLimited →
  `Infrastructure{RateLimited}`; reviewer `Timeout` → `ReviewerTimeout`; any other reviewer
  unavailability (a review process that could not run reaches the judgement this way through
  `run_review`) → `ReviewUnavailable`; a Runner error running a gate whose process fate is
  `NeverStarted` → `RunnerSpawnFailure` (R25; typed by the judge as `JudgeError::Runner`, never
  propagated as an error of the sequence), one whose fate is `Gone` → `Other` (a started process
  the Runner has established gone), and one whose fate is `Unresolved` → no terminal at all: the
  command ends resumably with the transaction open (R25); a Git error the verification observes →
  `Other` (foreign Git state, R26); a gate that times out → `Other` (R27); any other verification
  error — a containment refusal, a malformed record — is a defect of the run, not an outage, and
  ends the command resumably (R24). A gate that exits non-zero is code-attributed (`GatesFailed`);
  a gate that times out is **not** — the first version of this reading said it was, and
  `decisions.repairs.not_repairs` lists timeout among the outcomes that "at integration terminate
  in merge_verification_unavailable" (corrected in the second repair round, `pr8-triage.md` §5
  record F4); a reviewer verdict that rejects is `Rejected`; a reviewer that asks for a human is
  `HumanRequired{verdict: reasons}`; a review input that cannot be judged — the diff too large or
  opaque, or the review-input policy refusing the proposed tree — is classified before any process
  runs, as the attempt path classifies it, and is `HumanRequired` too: a Fix task cannot be asked
  to edit code without code evidence and waiting cannot make the same diff fit. The attempt path's
  Test-provenance rule is not applied to the integration diff (R28).
- **R5. Defer arithmetic.** *Settled by the contract*: `C.expected_failures_refusals[5]` forbids
  Deferred at the limit and non-consecutive counts, and `check_defer_allowance` is the fold's
  statement of it. `Deferred{defers}` with `defers = queued.defers + 1` while that is
  `< max_defers`; at `>= max_defers` the outage parks (so `max_defers = 0` parks the first outage).
- **R6. `merge_rejected` lease paths.** Conflict → the conflict paths; code rejection → the
  candidate's actual paths (the region the rejected code touched). Both are unioned with the
  candidate's held region by the fold.
- **R7. Over-limit and empty-intersection at once.** The fold refuses a `HumanRequired`
  admission on a `HumanBinding` entry, so one admission must win: the empty intersection wins
  (`HumanBinding`), because without a binding nothing can run whatever the limit says.
- **R8. Counting automatic repairs.** Counting is mandatory (`invariants[INV-11].enforced_by`:
  "fold counts automatic rejections per root"); what is chosen is the count. Reading: the
  registered lineage members of a root are its automatic rejections (each `merge_rejected`
  registers exactly one). What the fold then does — the Class B change the owner approves — is:
  a `Runnable` admission is refused once `members >= max_merge_repairs`, a `HumanRequired`
  admission is refused below it, and a `HumanBinding` admission is accepted on either side of the
  limit, because the empty intersection wins (R7) and `decisions.repairs.limits` registers
  over-limit *and* empty-intersection repairs "with human admission" — refusing `HumanBinding` at
  the limit would leave an over-limit empty-intersection rejection with no admissible shape at
  all. (The first version of this reading and of `pr8-body.md` said an over-limit `HumanBinding`
  was refused; the code never did that, the contract does not ask it, and the description was
  wrong — corrected in the repair round.) Enforced in the fold and derived identically by the
  driver.
- **R9. Repair path hints.** Original hints ∪ candidate actual paths ∪ conflict paths, as
  strings; a `RepoWide` region contributes nothing (an absent-or-unreadable region widens by
  nothing, and the lease already holds repo-wide).
- **R10. Repair ladder.** Tiers of the root's frozen ladder at or above `max(Mid, root floor)`
  and at or below its ceiling, with the root's rungs for exactly those tiers; `floor` recorded
  as that maximum, `ceiling` as the highest surviving tier; `attempts_per`, effort and reviews
  copied from the root. Empty survivors → `tiers: []`, `rungs: []`, `ceiling: None`, the raised
  floor, and `Admission::HumanBinding{options: the entry's allowed agents}` — the run's probed
  agents, which `check_spawn` binds `allowed_agents` to — never the sub-floor rungs the
  intersection excluded. `check_ladder` accepts the shape (an absent ceiling is the maximum of no
  tier: `an_empty_intersection_ladder_records_no_tier_no_ceiling_and_the_raised_floor`). The
  first implementation kept the root's tiers, floor and ceiling and offered the excluded rungs;
  the code was changed to this reading in the repair round.
- **R11. Repair deps.** The root's authoritative deps (`repairs.lineage`), all Merged by
  construction; display deps copied alongside.
- **R12. Verification-park question.** `FrozenQuestion{id: ids.question_id(), key: candidate.key,
  kind: Clarify for a reviewer's needs-human, Unblock for an unjudgeable input or an outage at
  max_defers, context from the coordinator's question builder with the verification failure (an
  outage's context carries what the infrastructure reported), options from
  `question_options(kind)`}`. Questions carry no attribution and no `DesignDefect` is emitted.
- **R13. When answers are ingested.** `AnswerSource::resolve` may block (terminal or file source
  with a wait budget), so answers are read where the legacy engine reads them: at the hard
  block, when nothing else is runnable. A verification-park answer appends
  `question_answered{Answered{option_index, binding_override: None}}` (the chosen option's
  index, or 0 for free text) or `{Declined{decline_halts_run}}`; a repair-admission answer is
  refused before any append (PR9). `decline_halts_run` follows the run's `on_task_failure`
  policy, carried into `RunSeams` beside `halts_run`. Driven end to end in
  `a_verification_park_answer_is_ingested_at_the_hard_block_and_a_repair_admission_answer_is_refused_before_any_append`.
- **R14. Residue reclaim at resume.** Cleanup is mandatory and its order constrained
  (`C.side_effect_vs_event_ordering`: removal after the terminal; `C.cancellation`: snapshots
  reclaimed). Reading, corrected in the repair round: staging worktrees no live transaction owns
  are reclaimed with force before the namespace check (T-PROPOSAL a', a); snapshots are reclaimed
  only after every terminal a snapshot could belong to — the attempts settled at (d) and the
  integration transaction resolved at (f) — so an interrupted verification's snapshots leave
  after `merge_verification_interrupted`, never before it; orphan pins stay in step (f) with the
  candidate orphan pins because they are ref work, which the existing order performs only after
  the Runner is rebuilt. (The first version proposed removing every snapshot before the Runner
  was rebuilt, which would have removed an unresolved verification's resources before its
  terminal, and then delivered no snapshot cleanup at all.)
- **R15. Expected refs at resume.** *Settled by the contract* (T-PROPOSAL's resume action and
  refusal, T-VERIFY's "pin SHA differs from record", INV-17), and now implemented as written:
  prepared pins expected under the run namespace are exactly `prepared/<seq>` for every sequence
  whose `merge_verification_started` has a StaleClean basis (derived from the proven prefix's
  events), plus `prepared/<next_seq>` (the possible provisional orphan, reclaimed expected-old at
  what it names only when no transaction is open). The open transaction's pin — verifying or
  prepared — must name its recorded proposal and is kept; a pin of a resolved sequence is pruned
  expected-old at its recorded proposal sha and refused at any other sha; a pin at any other
  sequence is refused by `refuse_unexpected_refs`, untouched. (The first implementation deleted
  every pin it did not recognise; see `pr8-triage.md` C2.)
- **R16. Expected head after `merge_prepared`.** CAS recovery needs the authorized
  `expected_head`; the fold's `TransactionClass::Prepared` did not retain it. Reading: retain it
  in the fold (Class B) rather than re-derive it from the event list a second time. Extended in
  the repair round: the `Prepared` transaction also retains the `disposition` and the
  `prepared_ref` the record named, because the SHAs alone cannot say what a publication left
  behind — an already-present publication at the candidate's own commit has
  `proposed_sha == candidate.commit_sha` exactly as a fast one does while still owning a staging
  worktree (`pr8-triage.md`, crash 5).
- **R17. Snapshot names for integration reviewers.** `SnapshotName::integration(seq)` is the
  gate snapshot; a new `SnapshotName::integration_review(seq, pass)` names one fresh snapshot
  per reviewer (`workspace_manager/naming.rs`, not a frozen path).
- **R18. Implementer binding for `passes_for`.** The binding the candidate ran under: the task's
  validated override when one exists (E2 binds every later attempt to it), else the frozen rung at
  the fold-derived rung position — the fold moves a task's rung only at an escalation settlement,
  and a task at `AwaitingMerge` settles no further attempt, so that position is the producing
  attempt's. (The first implementation passed the ladder's last rung, which a candidate produced
  lower down never ran under; `pr8-triage.md` C6.)
- **R19. The two-crash proof's "unsynced merge_prepared".** *Settled by the contract*:
  `C.proof_tests[3]` spells the sequence out. Implementation note on the construction: the first
  crash is an append whose flush is made to fail after the full line was written (`WrittenFull`
  error-return — the in-process shape of "kill at Written after the full line"), so the line is
  complete and unsynced and the durability ledger shows the write as the last step; the restart is
  a process of its own, whose barrier reports its sync of the log file, and which is killed at the
  `task_merged` write; the second power loss truncates the log to the length the barrier proved
  durable. (The first version deleted the line before the only resume; `pr8-triage.md` C5.)
- **R20. Kill sampling of the cherry-pick child.** *Settled by the contract*: `C.proof_tests[2]`
  and `[T-PROPOSAL].test` name both proofs. Delivered in the repair round at the engine level:
  synthetic residue (unreferenced objects, `CHERRY_PICK_HEAD`, `MERGE_MSG`, `index.lock`,
  sequencer state) in a staging git dir, classified by the workspace manager's classifier and
  reclaimed through the resume with the objects left to Git; and real `git cherry-pick` children
  killed on the funnel's own kill ladder, each classified and each recovered through the resume,
  the candidate integrating afterwards under a fresh pick. (The first version cited the `effects`
  census self-test, which samples constants; that claim, R21, is withdrawn.)
- **R21 (withdrawn).** The claim that the `effects` census framework covered PR8's per-site kill
  and residue obligations was wrong: it is a self-test of the framework. The recovery tests of R20
  carry those obligations.
- **R22 (withdrawn).** The first version of this entry read the unavailable terminals' missing
  review record as a permitted limitation: "charged live and not on replay; the ceiling is checked
  against the larger of the two". It claimed a discretion the contract does not offer.
  `decisions.coordinator_integration.dispositions` requires "spend recorded" for an Infrastructure
  `Deferred`, and DESIGN §26 says the four terminal shapes "carry the complete gate/review
  records, usage/cost". What holds: an integration's judged reviews are charged to the candidate's
  task and to the run at the verification, before the terminal, and `Spend::replay` reads them
  back off `merge_prepared` and `merge_rejected{CodeRejected}`; a review that ended in a
  `HumanRequired` park or an outage is charged live only, because
  `MergeVerificationUnavailable{sequence, cause, outcome}` has nowhere to carry it, and a restart
  therefore admits an integration the previous incarnation's total would have refused (the
  reviews of `916852c9`, adequacy 1 and record F2, reproduced). Recording it durably is a Class C
  change to the frozen vocabulary this slice may not make; the gap is an owner decision owed,
  `PR8-R2-SPEND-REPLAY` in `pr8-body.md`, pinned by
  `recover::tests::a_paid_review_that_parks_is_charged_live_and_its_replay_loss_is_the_deferred_vocabulary_gap`.
- **R23. The authorized head with no publication pending.** The latest `task_merged.merged_sha` in
  the proven prefix; before any publication, the recorded base (the P7/P8 create-at-base path is
  unchanged). A ref elsewhere, or absent after a publication, is foreign integration state
  (`[T-RESUME].refusal_condition`; DESIGN §26's "`task_merged` exists but the ref disagrees —
  refuse") and refuses before any append; it is never moved or recreated. The check is skipped
  only under a `Prepared` transaction, whose ref `finish_integration` owns; it runs under a
  verifying transaction, whose interrupted settlement moves no ref. (The first implementation
  compared against the base whenever no transaction was open, so a run that had published anything
  refused every later resume; `pr8-triage.md` C1.)
- **R24. Which verification errors are outages.** Rewritten in the second repair round; the first
  version said "only a Runner's own error running a gate is an outage; every other error ends the
  command resumably", which both over- and under-reached (`pr8-triage.md` §5, regression 1 and
  record F3). What decides is the error's own claim: a Runner error is settled by the process fate
  it carries (R25), a `UpstrokeError::Git` observed anywhere in the verification is foreign Git
  state and an outage (R26), a timed-out gate is an outage (R27), and every other error — a
  containment refusal, a plan the run cannot assemble, a malformed record — is a defect of the run
  and ends the command resumably with the transaction open, because deferral is applied to nothing
  the contract does not classify as infrastructure.
- **R25. A Runner error settles a terminal only when it establishes no process survives.**
  `decisions.repairs.not_repairs` and `[T-VERIFY].resume_action` require an *observed*
  infrastructure failure to terminate `merge_verification_unavailable`; `invariants[INV-15]`
  forbids cleanup of resumably open resources, and a terminal authorizes cleanup (the snapshot,
  the staging worktree, the pin) and readmission (both entitlements released, the next sequence
  admitted after the defer wake). An error observed while the gate's process may still be running
  is not an observed failure of the sequence but the coordinator's loss of its own process, and
  the resources the terminal would reclaim are that process's. So `Runner::run` returns
  `RunnerError { invocation, fate: ProcessFate, source }`, with the fate made where the evidence
  is — and, the rule the third repair round made explicit at both Runners, **process-fate
  evidence is kept distinct from whether every cleanup step completed**: a cleanup failure is
  never read as "the process may still run", and a reaped leader or a removed record is never
  read as "the tree is gone". The host funnel: `NeverStarted` until the spawn has created a
  process (on Windows the spawn boundary's own cleanup establishes `Gone`, or leaves
  `Unresolved`, for a process created behind a failed containment step); `Unresolved` from then
  on; `Gone` only from tree-level evidence — the Supervisor's `finish` establishing the process
  group has no non-zombie member on Unix, the job observed empty on Windows — with the direct
  child's kill and reap tidy-up whose failures are reported and never evidence, so a post-spawn
  containment failure, a reaper that failed, or a Windows job cleanup that failed after the direct
  child exited all leave `Unresolved`. The container runner: before `docker start` was attempted a
  failed launch is `NeverStarted` whatever the cancel achieved, because a created-but-never-started
  container holds no process; once `docker start` was attempted, or the container was running, the
  fate is `Gone` when the runtime established the process gone — the supervisor observed the
  container terminated, a stop succeeded, or a forced removal succeeded — and `Unresolved` only
  when none of those holds. A start the runtime refused cannot be told from a start whose
  acknowledgement was lost, so a refused start whose cancel completes is `Infrastructure{Other}`
  (established gone) rather than `RunnerSpawnFailure`; both defer identically. A container the
  runtime confirmed neither stopped nor removed keeps its R19 view and R26 intent, which the next
  census reclaims it through; a view or an intent that could not be released is residue, not
  liveness. The integration settles `Infrastructure{RunnerSpawnFailure}` on `NeverStarted`
  (INV-23's mid-run image mismatch is this), `Infrastructure{Other}` on `Gone`, and on
  `Unresolved` ends the command resumably with the transaction open, the snapshot retained and
  nothing appended; the next resume's startup census reclaims the earlier incarnation's container
  — refusing while the runtime is unreachable, as `[T-RESUME].refusal_condition` says — before
  recovery step (f) settles the verification interrupted and reclaims the snapshot. `run_review`
  propagates an `Unresolved` Runner error rather than reporting the review unavailable, so the
  reviewer path cannot settle a terminal over a running reviewer either. Test doubles must say
  what they claim: a double that returns an untyped error does not compile. (The second repair
  round's version of this reading classified the host by the leader's reap and the container by
  whether both cleanup operations completed; the review of `79ddbffb` found defect A and defect B
  both reachable through it, `pr8-triage.md` §6.)
- **R26. Foreign Git state at integration.** `decisions.repairs.not_repairs` lists "foreign Git
  state" among the failures that at integration terminate `merge_verification_unavailable{Deferred
  | Parked}`. A `UpstrokeError::Git` observed anywhere in the verification — the review diff, the
  proposed tree's read, the review-input policy's reader (the reviewer's reproduction: a corrupt
  disposable staging index), the judge's snapshot checkout — settles `Infrastructure{Other{detail:
  foreign Git state …}}`; every Git command the verification issues runs before or between its
  processes, never beside one, so no liveness question arises. The kind is `Other` because the
  frozen `InfrastructureKind` names no Git kind and `Other{detail}` is its extension point.
- **R27. A timed-out gate at integration.** `not_repairs` lists timeout. A gate verdict with
  `timed_out: true` (the Runner enforced the timeout: the process is stopped and reaped, the
  verdict is `None`) is asked about before the judgement's failure is read, and settles
  `Infrastructure{Other{detail: gate … timed out}}`, deferred inside the allowance and parked at
  it, registering no repair. The attempt path's treatment of a timed-out gate (PR7's: the attempt
  fails with the log tail as feedback) is untouched.
- **R28. The integration diff is judged for size and opacity only.** `classify::diff_failure`
  also enforces that a Test task's diff adds test code, an attempt-settlement rule (E4's allowance
  function feeds on it). At integration the diff is the candidate cherry-picked onto a moved head,
  and a test another candidate published first is absent from it without being absent from the
  tree; the reviewers reproduced the rule rejecting exactly that candidate and registering a
  repair. The candidate's provenance was judged when it was produced; the integration consults
  `classify::unjudgeable_diff` (the size/opacity half) and the review-input policy, nothing more.

## 2. Staged implementation plan (one commit per shape)

Each commit is gated locally (fmt, clippy, the touched test families) before the next; the full
ten-command baseline runs before every push.

| # | Commit | Production | Tests it lands | Ordering assertion it rests on |
|---|---|---|---|---|
| 0 | `docs(pr8): plan` | this file | — | — |
| 1 | `feat(topology): fold readers and the retained expected head` | `predicates.rs`: `next_sequence`, `satisfies_closure`, `lineage_members`; `TransactionClass::Prepared.expected_head`; INV-11 count in `check_merge_rejected` | fold tests: readers agree with the checks; over-limit admission refused both ways, live and on replay | none (state only) |
| 2 | `feat(engine): fast integration` | `integrate.rs`: selection → reservation → `assert_publishable` + head read → fast `merge_prepared` → CAS → `task_merged`; loop branch `Integration` performed; `checkpoint` admits `Integrate`, refuses Repair-origin dispatch; `expected_refs` gains pins | `fast_path_publishes_exact_candidate_without_staging_or_proposal_object` (real repo; harness fast sequence; fsck count; no intent), `merge_prepared_fast_with_moved_head_or_wrong_proposed_or_pin_refused_live_and_on_replay`, `third_sha_refused`, symbolic/checked-out refusals, `fast_dual_holding_released_once` (ST-13), repair-origin dispatch refused before append, replay twice equal | reservation before the head read; `merge_prepared` before CAS; CAS before `task_merged`; no staging site observed in the sequence |
| 3 | `feat(engine): stale_clean and already_present verification` | staging intent → add → cherry-pick → `proposal_state`; pin; `merge_verification_started`; verification on commit snapshots (gates + reviewers with sequence identities, `AttemptPlans::verification`); `merge_prepared(stale_clean/already_present)` → CAS → `task_merged` → pin delete; forced staging removal | `stale_candidate_takes_staging_path_and_publishes_pinned_proposal`, `an_already_present_candidate_settles_without_an_empty_commit` (no commit manufactured; CAS new == old, counted), `merge_prepared_stale_clean_with_unpinned_proposed_refused`, verification isolation (snapshots of the proposal commit, no new object, nothing runs in staging, the gate's workspace HEAD is the recorded proposal), ST-04 sequence aliasing (second start refused; occupied staging path refused) | staging intent before add; proposal objects before the pin; pin before `merge_verification_started`; snapshot intents before snapshot add; `merge_prepared` before CAS; removal after the terminal |
| 4 | `feat(engine): merge_rejected with atomic repair registration` | `repair.rs`: the frozen spawn, admission, lease effect; conflict and code-rejected terminals | conflict → lineage created, repair Pending, parent AwaitingRepair, lease transferred; code rejection (gate failure, review rejection); member rejection widens; over-limit → HumanRequired; empty intersection → HumanBinding; `kill_after_merge_rejected_neither_loses_nor_duplicates_repair`; replay twice equal | `merge_rejected` before any repair effect (no dispatch, no task worktree intent or add); reservation released at the terminal |
| 5 | `feat(engine): unavailable terminals` | Deferred/Parked for Infrastructure and HumanRequired; pin deletion and staging removal at the terminal; defer wake through the existing backoff branch | `human_required_verdict_parks_task`, `infrastructure_failure_defers_then_parks_at_max_defers`, `defer_wait_elapsed_reenables_deferred_candidate`, non-eligible start refused for deferred and parked candidates, Deferred at max / non-consecutive / Parked without question / HumanRequired without Parked refused live and on replay | terminal before pin deletion and staging removal; both entitlements released at the terminal |
| 6 | `feat(engine): verification-park answers and decline` | hard-block ingestion of verification-park answers; refusal of repair-admission answers; `decline_halts_run` seam | park/answer/decline tests; `declined_parked_verification_fails_task_consumes_queue_position_releases_lease_and_halts_per_policy`; repair-admission answer refused before append | answer append before any re-verification; decline before any release |
| 7 | `feat(engine): integration recovery` | recovery step (f): `merge_verification_interrupted`, CAS recovery after the barrier, orphan and resolved pins, residue reclaim of staging and snapshot intents; `refuse_unimplemented_terminals` narrowed to what PR8 still refuses | `kill_between_prepared_and_cas`, `kill_between_cas_and_merged`, `kill_during_merge_verification_settles_interrupted_and_reverifies`, `kill_after_proposal_commit_before_pin_reclaims_staging_and_leaves_object_to_git`, `orphan_prepared_pin_reclaimed_only_at_next_sequence`, `staging_residue_reclaimed`, synthetic cherry-pick residue converges, sampled cherry-pick child kills classified and recovered, `prepared_publication_completed_at_run_end` for the resume path | barrier before any CAS; `merge_verification_interrupted` before pin deletion and snapshot removal; no CAS on a replay-visible-only `merge_prepared` |
| 8 | `test(engine): the two-crash proof` | — | `unsynced_merge_prepared_two_crash_barrier_before_cas_then_power_loss_keeps_log_and_ref_agreeing`, `barrier_sync_failure_before_cas_issues_no_cas_and_converges_after_loss` | barrier (sync, stable reread, checked replay) before the CAS |
| 9 | `test(engine): terminal-shape coverage table and ST-13` | — | the eight-row table driven end to end; ST-13 sequential subset incl. the fast no-staging assertion; replay-twice-equal over every shape | — |
| 10 | `docs(pr8): body, design notes, internals` | `pr8-body.md`; DESIGN §26 additions only if a sentence the code enforces is missing; `docs/internals` notes for new modules if the gate requires them | gates | — |

Commits may be split further; none is merged with another. The order of commits 2–5 is by
terminal shape as the brief asks; 7 and 8 close the recovery rows the earlier commits opened.

### The repair round (2026-09-07)

The three reviews of `3414dc58` and their triage are `pr8-triage.md`. The repairs landed as one
commit per finding or closely related group, each with a test that fails at `3414dc58` (or a
mutation witness replayed against the repaired tree) and passes at the repair:

| Commit | Findings |
|---|---|
| `docs(pr8): triage of the three reviews at 3414dc58` | the dispositions |
| `fix(engine): recovery accepts the head the log's latest publication put there` | C1 (crash 1, conformance F2) |
| `fix(engine): recovery keeps, prunes, and refuses prepared pins by their records` | C2 (crash 3, conformance F5/F6, tests 12), crash 5, tests 10 (worktree) |
| `fix(engine): verification snapshots are removed after the terminal and reclaimed by resume` | conformance F4, C4 (crash 4, conformance F3) |
| `fix(engine): the integration verification runs under the candidate's binding, classifies its input, settles runner outages, and charges its reviews` | C6 (tests 1, conformance F7), C3 (conformance F1, tests 4), tests 2, tests 3, R13 coverage |
| `fix(engine): an empty tier intersection freezes the raised floor, no ceiling, and the allowed agents` | audit R10, tests 13, tests 8 (m01) |
| `test(engine): the integration tests observe the verified commit, the no-op swap, and the absence of repair effects` | tests 5 (m03), 6 (m02), 7 (m05) |
| `test(engine): the two-crash proof performs the two-crash sequence` | C5 (conformance F8, crash 6, tests 9) |
| `test(engine): cherry-pick residue converges through integration recovery, and the stale fixtures move the head by a publication` | conformance F9, tests 11; crash 2 pinned as deferred; R23's check under a verifying transaction |
| `test(engine): the recover tests satisfy the crate's effect lints` | clippy on the new tests; internals notes |
| `docs(pr8): the record after the repair round` | this file, `pr8-body.md`, `pr8-triage.md` |
| `test(engine): the residue sampler removes git's common-dir lock residue after a kill` | the macOS CI failure of the sampler; a second sighting of `PR8-CRASH-002` |
| `docs(internals): the integration slice's prose moves to its notes files` | CODING_STANDARDS §13: every comment the pull request added to a module with notes moves to `docs/internals/`, and the four new modules gain notes files; no code changes |

### The second repair round (2026-09-07)

The three reviews of the repair range `3414dc58..916852c9` and their triage are `pr8-triage.md`
§5. One commit per finding or closely related group, each with a test that fails without its
repair (the mutation named in §5 below), the records kept current between them:

| Commit | Findings |
|---|---|
| `docs(pr8): triage of the three reviews at 916852c9` | the dispositions |
| `fix(runner): a Runner error carries what it established about the process, and the integration settles a terminal only when no process survives` | regression 1 (`PR8-R2-RUNNER-LIVENESS`); with it the production halves of record F3 (`PR8-R2-GIT-STATE`), record F4 (`PR8-R2-GATE-TIMEOUT`) and adequacy 2 (`PR8-R2-TEST-PROVENANCE`), which share the rewritten `IntegrationCx::verify` |
| `fix(engine): the frozen repair spec embeds the rejection evidence, the rejecting head and the rejected candidate` | record F1 (`PR8-R2-FROZEN-SPEC`) |
| `test(engine): foreign Git state and a timed-out gate settle an outage, and a Test candidate is judged for size and opacity only` | the tests of record F3, record F4 and adequacy 2 |
| `test(engine): the production verifier is observed through the loop, the sampler proves its kills, the crash child reports, and the spend gap is pinned` | adequacy 3 (`PR8-R2-SNAPSHOT-ORACLE`), adequacy 4 (`PR8-R2-SAMPLER-ORACLE`), regression 2 (`PR8-R2-UNREACHABLE`), adequacy 1 / record F2 (`PR8-R2-SPEND-REPLAY`, deferred) |
| `docs(internals): the three statements the prose relocation preserved false` | record item 7 (`PR8-R2-RECORD-PROSE`) |
| `docs(pr8): the record after the second repair round` | this file, `pr8-body.md`, `pr8-triage.md`; the provenance section (`PR8-R2-RECORD-PROVENANCE`) |

### The third repair round (2026-09-07)

The single review of the repair range `916852c9..79ddbffb` — the Runner seam only — and its
triage are `pr8-triage.md` §6. One commit per finding or closely related group, each with a
test that fails without its repair (the mutation named in §5 below), the records kept current
between them:

| Commit | Findings |
|---|---|
| `docs(pr8): triage of the round-three review at 79ddbffb` | the dispositions |
| `fix(runner): the container runner classifies from process evidence alone, and keeps a live container's view and intent` | findings 2, 3 and 4 (`PR8-R3-CONTAINER-START`, `PR8-R3-CONTAINER-RETAIN`, `PR8-R3-CONTAINER-EVIDENCE`); the fate matrix that kills M4 |
| `fix(agent): the host funnel claims Gone only from group evidence, and the Windows spawn boundary carries its fate` | finding 1 (`PR8-R3-HOST-GROUP-GONE`); the host fate tests that kill M3 |
| `test(engine): repeated pre-start container outages consume defers, and an unresolved verification keeps its entitlements with the open transaction` | finding 2's loop witness through the production `ContainerRunner`; finding 5's pin (`PR8-R3-RECORD-ENTITLEMENTS`, a record defect) |
| `docs(pr8): the record after the third repair round` | this file, `pr8-body.md`, `pr8-triage.md` |

## 3. `src/topology/**` changes: Class A / B / C

Rule applied: a read-only accessor that exposes a derivation the fold already makes is
**Class A** (self-serve, disclosed here and in `pr8-body.md`); anything that changes what the
fold accepts, refuses, retains or applies is **Class B** (owner approval owed before landing);
any change to the wire vocabulary would be **Class C** (none planned). Where a change could be
read either way it is listed as B.

| Change | File | Class |
|---|---|---|
| `TopologyFold::next_sequence()`, `satisfies_closure(key)`, `lineage_members(root)` readers | `src/topology/fold/predicates.rs` | A |
| `TransactionClass::Prepared` retains `expected_head` (set in `apply_merge_prepared`; read by CAS recovery) | `src/topology/fold.rs`, `src/topology/fold/apply.rs` | B |
| `TransactionClass::Prepared` retains `disposition` and `prepared_ref` (set in `apply_merge_prepared`; read by `Authorized::from_fold`, so recovery reads what the publication left behind instead of inferring it from the SHAs — R16) | `src/topology/fold.rs`, `src/topology/fold/apply.rs` | B (added in the repair round) |
| `check_merge_rejected` counts a lineage's registered repairs against `max_merge_repairs`: a `Runnable` admission refused at or over the limit, a `HumanRequired` admission refused below it, a `HumanBinding` admission accepted on either side (INV-11, R8) | `src/topology/fold/check_integration.rs` | B |
| Fold tests for the above, incl. `a_lineage_that_has_consumed_its_allowance_registers_only_a_human_required_repair` and `an_empty_intersection_ladder_records_no_tier_no_ceiling_and_the_raised_floor` | `src/topology/fold/tests.rs` | A (tests only) |
| Test-only fixture adaptations: the admission fixtures at `max_merge_repairs = 0` and the repair-continuation selection as `RepairDispatch`; a `TransactionClass::Prepared` pattern | `src/topology/fold/tests/questions.rs`, `src/topology/census.rs` | A (tests only) |

No other frozen path changed. `check_integration.rs`'s additional `..` pattern is consequential
adaptation to the retained fields, not another behaviour change.

## 4. Blocking items

None. Two owner decisions are owed and recorded rather than blocking, both Class C vocabulary
changes this slice may not make: `PR8-CRASH-002` in `pr8-body.md` (a lock file left by a
coordinator killed inside `git update-ref`; no `Ref.*` site registers a residue class in the
frozen inventory, so reclaiming it is a residue class for a slice of its own), and
`PR8-R2-SPEND-REPLAY` (the unavailable terminal carries no review record, so a paid review that
parked or met an outage is charged live and lost on replay; the contract requires the spend
recorded — a field on `merge_verification_unavailable`, or an erratum on
`decisions.coordinator_integration.dispositions`).

## 5. Coverage notes recorded during implementation

- **R13 (driver ingest).** The verification-park answer/decline transitions are proven at the
  fold layer (`an_outage_that_needs_a_person_parks_with_a_question_that_can_be_answered`,
  `declined_parked_verification_fails_task_consumes_queue_position_releases_lease_and_halts_per_policy`),
  the terminals that open a park at the integrate layer (`a_human_required_verdict_parks_the_task`,
  `infrastructure_failure_defers_then_parks_at_max_defers`), and — since the repair round — the
  ingestion itself end to end: `recover::tests` drives `TopologyRun::step` over a resumed log
  carrying a verification park (Answered returns the candidate to the queue and the next step
  integrates it) and one carrying a repair-admission park (the answer is refused before any
  append).

- **Commit 7 (integration recovery) — landed, repaired.** `recover::finish_integration` resolves
  the open transaction at step (f): a `Prepared` one through the live `integrate::publish`
  (barrier-proven CAS, third-sha refusal, record-only when the ref already names the proposal,
  the staging and pin the retained disposition names), a `VerificationStarted` one through
  `merge_verification_interrupted` with the pin pruned at its recorded proposal and the staging
  reclaimed, and every snapshot intent reclaimed afterwards. `reclaim_stale_residue` reclaims
  T-PROPOSAL staging residue and exactly the orphan `prepared/<next_seq>`, prunes resolved pins at
  their recorded proposals, checks the open transaction's pin against its record, and hands that
  pin to the namespace check; every other pin refuses there. `ensure_recorded_integration_ref`
  requires the ref at the last publication (R23) and is skipped only under a `Prepared`
  transaction. Tests in `recover/tests.rs`:
  `a_resume_completes_a_prepared_fast_transaction_through_the_barrier_and_cas`,
  `a_resume_completes_a_prepared_transaction_whose_cas_already_ran_by_recording_the_merge`,
  `a_resume_of_a_prepared_transaction_whose_ref_moved_elsewhere_refuses_a_third_sha`,
  `a_resume_settles_an_interrupted_stale_verification_and_reclaims_its_residue` (the staging
  worktree removed, then the candidate re-verified and published under the next sequence),
  `a_resume_reclaims_an_interrupted_verifications_snapshots_after_settling_it`,
  `a_resume_reclaims_the_orphan_pin_at_the_next_sequence_and_orphan_staging`,
  `a_resume_refuses_a_prepared_pin_outside_the_sequences_the_log_pinned`,
  `a_resume_refuses_a_substituted_verification_pin_before_settling_it`,
  `a_resume_keeps_a_prepared_transactions_pin_when_publication_refuses`,
  `a_resume_prunes_a_resolved_sequences_pin_at_its_recorded_proposal_and_refuses_it_elsewhere`,
  `a_resume_completes_an_already_present_publication_at_the_candidate_commit_and_reclaims_its_staging`,
  `a_resume_after_a_completed_publication_accepts_its_own_head`,
  `a_resume_after_a_publication_refuses_a_ref_that_disagrees_with_the_log`,
  `synthetic_cherry_pick_residue_unreferenced_objects_and_cherry_pick_head_then_forced_reclaim_converges`,
  `sampled_cherry_pick_child_kills_every_residue_classified_and_recovered`, and the deferred
  `a_ref_lock_left_by_a_killed_compare_and_swap_refuses_resumably_until_removed`.

- **The stale fixtures.** The recover tests' stale-verification fixtures first moved the
  integration head by hand. Only a publication moves a run's head, and a head past the base with no
  recorded publication is the foreign state R23 refuses, so every such fixture now publishes a
  second task fast at sequence 0 and plants the stale sequence as 1. The driver harness
  (`drive`) and every publication test resume through the real `WorkspaceManager` as both ref
  interfaces; the `RecordingRefs` double, which answers "absent" whatever the repository holds,
  is kept for the run-start tests it was written for.

- **Commit 8 (two-crash proof) — landed, replaced.** The proof is now the sequence the contract
  spells out (R19): `unsynced_merge_prepared_two_crash_barrier_before_cas_then_power_loss_keeps_log_and_ref_agreeing`
  and `barrier_sync_failure_before_cas_issues_no_cas_and_converges_after_loss`, both in
  `recover::tests`, with the two-crash restart in a child process killed at the `task_merged`
  write. The earlier `unsynced_merge_prepared_lost_to_power_failure_keeps_log_and_ref_agreeing`
  is gone: it deleted the line before the only resume and could not fail when the barrier's sync
  was removed. `integrate::tests::an_append_failure_at_merge_prepared_issues_no_cas_and_leaves_the_integration_ref`
  remains as the live-path append-failure case; the barrier's own convergence of an unsynced tail
  stays `events::log::unsynced_line_lost_before_barrier_converges_to_before_append_order`.

- **Commit 9 (terminal-shape coverage) — landed, hardened.**
  `integrate::tests::terminal_shape_coverage_table_drives_every_shape_and_each_converges_on_replay`
  drives the seven integrate-path shapes (fast, stale_clean, already_present, conflict,
  code_rejected, deferred, parked) end to end in one table, each asserted at its terminal and
  replayed twice for equality, every verifying shape now running a gate whose workspace HEAD is
  asserted to be the recorded `proposed_sha` and never the candidate commit; it cites the two
  shapes that are not integrate terminals — Declined-after-park (`fold`) and Interrupted
  (`recover`) — which live in their own harnesses. The detailed per-shape tests remain, and
  `verification_snapshots_are_removed_only_after_the_terminal` fixes the removal order for the
  three verifying terminals. ST-13's fast no-staging assertion is in the fast-path test; the
  dual-hold release is `fast_dual_holding_released_once`.

- **Mutation witnesses replayed against the repaired tree** (each killed by the test named in
  `pr8-triage.md`): the reviewer's m01, m02, m03, m05 and m06; the crash reviewer's removal of
  the barrier's `sync_log_file`; and one mutation per repair of this round — the base compared
  after a publication, the SHA-inferred staging, the delete-everything pin loop, the skipped
  substituted-pin check, snapshot removal as each role finishes, the skipped snapshot reclaim,
  the last-rung implementer, the propagated Runner error, the skipped input classification, the
  uncharged review, and the removed staging reclaim.

- **The second repair round's witnesses** (`pr8-triage.md` §5), each replayed against the
  repaired tree and each failing exactly the test named: the blanket conversion restored (an
  `Unresolved` fate settled as `RunnerSpawnFailure`) fails
  `a_runner_that_loses_track_of_a_running_gate_refuses_resumably_and_reclaims_nothing` and
  `a_lost_gate_container_is_reclaimed_by_the_next_resume_before_the_verification_is_settled`; the
  Git arm removed fails `a_git_error_observed_by_the_verification_settles_an_infrastructure_outage`;
  the timed-out-gate check disabled fails
  `a_gate_that_times_out_during_integration_verification_defers_instead_of_registering_a_repair`;
  `diff_failure` restored at integration fails
  `a_test_candidate_whose_test_was_already_published_is_verified_not_rejected_for_provenance`; the
  spec body not embedded fails `the_frozen_repair_spec_embeds_the_rejection_evidence_and_both_shas`;
  the reviewers' two surviving mutations — `request.candidate.commit_sha` snapshotted in
  `IntegrationCx`, and `SnapshotDisposal::AsEachRoleFinishes` there — each fail
  `the_production_verifier_judges_the_recorded_proposal_and_removes_its_snapshots_after_the_terminal`;
  and `child.kill()` deleted fails `sampled_cherry_pick_child_kills_every_residue_classified_and_recovered`.
  The runner's own classification is pinned by
  `runner::container::exec::tests::the_runner_reports_what_it_established_about_the_process_when_it_fails`
  (three fates from the production `ContainerRunner` over the fake runtime) and
  `review::tests::an_unresolved_runner_error_propagates_instead_of_reporting_the_review_unavailable`.

- **The third repair round's witnesses** (`pr8-triage.md` §6), each replayed against the
  repaired tree at `507eee1f` with the mutation applied by an asserted replacement, the named
  tests run, and the file restored from `HEAD`; each fails exactly the tests named. The two
  mutations that survived `2342 passed, 43 ignored` at `79ddbffb`: M3, the host funnel's
  post-spawn `Unresolved` made `Gone`, fails
  `agent::proc::tests::a_containment_failure_after_the_spawn_leaves_the_fate_unresolved`; M4, a
  successful output with a failed release made `Gone` in `ContainerRunner::run`, fails the
  timed-out-with-neither-stop-nor-removal cell of
  `runner::container::exec::tests::the_runner_reports_what_it_established_about_the_process_when_it_fails`.
  One mutation per repair: `settle_failed_supervision` setting `Gone` from the leader's reap (the
  second round's rule) fails `a_reaped_leader_does_not_prove_its_group_gone_when_the_reaper_failed`,
  `a_lost_group_settles_unresolved_even_when_the_leader_reaps_cleanly` and
  `an_established_group_settles_gone_whatever_the_leaders_own_reap_says`; a lost `docker create`
  classified by the cancel's outcome fails the matrix and
  `recover::tests::repeated_container_launch_outages_before_start_consume_defers_through_the_production_runner`;
  an attempted start with a clean cancel made `NeverStarted` fails the matrix; the retention of the
  view and the intent removed fails
  `a_container_the_runtime_cannot_confirm_stopped_keeps_its_mounted_git_view_and_intent` and the
  matrix; `container_gone` made false on either cleanup failure (the second round's rule) fails
  the matrix; the release never told the exit was observed fails the matrix; and the integration's
  `Err` arm cancelling the reservation unconditionally fails
  `a_runner_that_loses_track_of_a_running_gate_refuses_resumably_and_reclaims_nothing` (the
  cancellation of a converted reservation is itself refused, and the step's error is no longer the
  Runner's). Two cells are pinned by reading and not by a mutation: the observed-exit branch of the
  Unix loop passing `false` (the group not established) when `finish` fails, because a
  funnel-level reaper failure arms the fail-closed `SIGTERM` and cannot be injected inside the test
  process — the reviewer's reproduction of that cell is the helper-level test above; and the
  Windows branch (the `Gone`-after-`finish_direct_exit` order and the spawn boundary's fate), which
  was type-checked and clippy-clean for `x86_64-pc-windows-msvc` on the build box and executes on
  winguest. The fix-up that followed the replay (`287563f0`, the Unix group signal of `kill_tree`
  moved into a positively gated helper because the platform census refuses a body no CI runner
  compiles) changes that helper's shape only; no witness exercises it.
