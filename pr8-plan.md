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
single review of the repair range `916852c9..79ddbffb` (`pr8-triage.md` §6), then
`START-PR8-FIX4.md` of the same day for the fourth repair round after the cover review of the whole
slice at `8a5f59e8` (`pr8-triage.md` §7).
Implementation model: Claude Fable 5.1 at max effort, the implementation and all four repair
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
  `run_review`) → `ReviewUnavailable`, **except a reviewer whose process the Runner established
  was never started, which is `RunnerSpawnFailure`** — `invariants[22]` (INV-23) requires that
  settlement for a mid-run image mismatch and says the rule covers "every probe, worker, gate,
  review, and re-ask process of the run", so the generic reviewer mapping this reading chose does
  not answer for it; the first version of this clause said every reviewer unavailability was
  `ReviewUnavailable`, which made the invariant's named case unreachable for reviewers and re-asks
  (corrected in the fifth repair round, `pr8-triage.md` §8.2 finding 4). Deferral and containment
  are the same either way: `run_review` still contains the failure so the pass defers rather than
  ending the command, and only the durable attribution differs. A Runner error running a gate whose
  process fate is
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
  *Corrected in the fourth repair round* (`pr8-triage.md` §7): the third round's version said a
  stop or a forced removal that "succeeded" established the process gone, and read the daemon's
  "removal of container … is already in progress" — normalized to a bare `Ok(())` so racing
  reclaimers converge — as such a success. It is not: `containerRm` sets that flag before
  `cleanupContainer` kills, so the loser of a removal race learns only that the winner holds the
  flag. The runtime now answers what a stop or a removal established (`Settled::ProcessGone` for a
  completed `docker stop`, `docker kill` or `docker rm --force`, each of which returns after the
  exit is seen, and for the daemon's absent and not-running answers; `Settled::RemovalInProgress`
  for the in-progress answer), and only `ProcessGone` is evidence: an in-progress answer retains
  the view and the intent and leaves the fate `Unresolved`. The same round found the third round
  recording every `start_container` failure as an attempted start: a refusal at the funnel's
  `Before` phase never issues `docker start`, and the funnel now says on which side of the
  primitive it failed, so such a launch stays `Created` and its fate is `NeverStarted`. The rule
  above is otherwise unchanged; the sweep of every site that concludes a process gone is
  `pr8-triage.md` §7.3.
  *Corrected again in the sixth repair round* (`pr8-triage.md` §9, finding 1): "the daemon's absent
  and not-running answers" was the right rule and the code was not reading the daemon's answers. It
  searched the whole of a failed command's stderr, so a local CLI failure quoting a path named for
  a phrase settled `ProcessGone` — and `observe` settled `Gone` — beside a running container. A
  phrase now counts only inside a line the daemon spoke that names the container asked about, and
  even then it only proposes: the settlement is established against a `docker ps` listing that had
  to succeed to answer, which is also what `observe` now reads. What `ProcessGone` means and what
  it is worth are unchanged.
- **R29. One rule for the authorized integration head.** The live exact-base decision and the
  resume's startup check read the same rule: the log's latest `task_merged.merged_sha`, or the
  recorded base before any publication (`integrate::authorized_head`). `decide` requires the head
  it reads to be that value before choosing fast or stale — a head the log did not put there is
  the foreign ref `decisions.coordinator_integration.integration_sequence` says refuses at this
  read — and `ensure_recorded_integration_ref` requires the same value at resume (R23). The live
  engine derives it from the event list `RunHandle` carries, which the one `emit` funnel extends
  on every successful append for the loop and for recovery alike; the fold is not asked to retain
  a publication, so no new Class B change is made. (The first implementation compared the head
  only with the candidate's base: an external writer resetting the ref from a publication back to
  the base made the next candidate exact-base and the engine published it fast over the lost
  publication; the cover review of `8a5f59e8`, `pr8-triage.md` §7.)
- **R30. The cleanup scope is entered wherever the run spawns host processes under its lock.** A
  Unix reaper takes its cleanup-lease paths only from the thread-local scope active at its spawn,
  and R28's shared `cleanup.lock` hold — what the next coordinator's exclusive probe observes and
  refuses on while a surviving reaper still settles its groups — exists only through that scope.
  The v0.1 coordinator and resume enter it beside their lock; the topology path acquired its lock
  and entered nothing. Now `run_recovery_order` enters it from the lock take to its end (the
  probes at (c)), creation's P4 enters it for the probes, and `TopologyRun::step` enters it for
  each step, on the thread that drives the step. The scope is owned rather than borrowed from the
  lock, because the lock lives inside the handle the loop drives and cannot be borrowed across the
  `&mut self` a step needs; each site drops its scope before it returns, so no scope outlives the
  hold it names. (Found by the cover review of `8a5f59e8` with a real reaper observed holding no
  lease at `ReaperStarted`; the omission predates PR8's attempt path and this branch is what
  routes integration verification through it.)
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

### The fourth repair round (2026-09-07)

The cover review of the whole slice at `8a5f59e8` and its triage are `pr8-triage.md` §7. One
commit per finding, each with a test that fails without its repair (the mutations of §5 below),
the records kept current between them; the two class sweeps the brief asked for are §7.3 and
§7.4 of the triage:

| Commit | Findings |
|---|---|
| `docs(pr8): the plan of the fourth repair round, recorded before its first repair` | the mechanism decided per finding |
| `fix(engine): a rejected or unavailable terminal prunes the pin at the proposal the record names, never at what the ref says` | finding 4 (`PR8-R4-SUBSTITUTED-PIN-LIVE`) |
| `fix(engine): the live decision reads the head the log authorizes through the rule the resume check uses, and refuses a foreign one before any append` | finding 1 (`PR8-R4-LIVE-HEAD`); R29 |
| `fix(runner): a stop or a removal says what it established, and another reclaimer's removal in progress establishes nothing` | finding 2 (`PR8-R4-REMOVAL-IN-PROGRESS`); R25 corrected |
| `fix(runner): a start refused before docker start was attempted leaves the launch created, so its fate is never started` | finding 5 (`PR8-R4-START-NOT-ATTEMPTED`) |
| `fix(engine): the topology run enters its lock's cleanup scope wherever it spawns host processes, so its reapers hold the run's lease` | finding 3 (`PR8-R4-CLEANUP-LEASE`); R30 |
| `test(engine): the review doubles run a process in the workspace they are handed, and the isolation oracles observe each reviewer's checkout` | finding 6 (`PR8-R4-REVIEW-ORACLE`) |
| `docs(pr8): the record after the fourth repair round` | this file, `pr8-body.md`, `pr8-triage.md` |

### The fifth repair round (2026-09-07)

The final cover review of the whole slice at `716cf89a` and its triage are `pr8-triage.md` §8.
Six findings — two P1, three P2, one P3 — of which four are code and two are the record; a
seventh row was raised by this round rather than the reviewer, sweeping the class finding 3
named. One commit per finding, each code repair with a test that fails without it (the mutations
of §8.3 of the triage), the records kept current between them:

| Commit | Findings |
|---|---|
| `fix(agent): a macOS pid enumeration that failed is not an empty process group` | finding 1 (`PR8-R5-MACOS-ENUMERATION`); §7.3's macOS row corrected and the class re-examined |
| `fix(engine): a completed integration review is charged when it completes, not when the judgement returns` | finding 2 (`PR8-R5-DISCARDED-REVIEW-COST`) |
| `fix(workspace): the proposal classifier and the manager's reads write no index` | finding 3 (`PR8-R5-CLASSIFIER-INDEX-WRITE`) and the class sweep (`PR8-R5-READ-ONLY-SWEEP`) |
| `fix(engine): a reviewer whose process never started settles as a runner spawn failure` | finding 4 (`PR8-R5-REVIEWER-SPAWN-FAILURE`); INV-23 |
| `chore(effects): classify libc::ENOMEM, which the macOS enumeration test names` | the governance row the new test's constant needs |
| `test(engine): the snapshot obstruction is written through the fixture funnel the lint census requires` | the round's own regression: `std::fs::write` and `std::fs::create_dir_all` are denied here (R18/R21) and the reviewer's witness used both, so the first version of finding 2's test compiled and failed clippy |
| `docs(pr8): the plan and findings of the fifth repair round` | this file, `pr8-body.md`, `pr8-triage.md`: §7.3 corrected and re-examined, §7.4's domain restated and completed across both layers (finding 5, `PR8-R5-CENSUS-DOMAIN`), the body's v0.1 and rollback claims qualified (finding 6, `PR8-R5-V01-CLAIM`), and reading R4 corrected |
| `docs(internals): the fifth round's prose moves to the modules' notes` | §13: a module with a notes file carries the `Extended notes:` pointer and no other comment, so every doc comment this round wrote in a module that has one moved to `docs/internals/`, and the `review_failure` anchors there were corrected to its new signature. The `SAFETY:` comments on the two `errno` accesses stay at their site, which §13 names as §11's to place. `src/workspace_manager.rs` has no notes file, so its prose stays as rustdoc |
| `docs(pr8): the record after the fifth repair round` | the Validation section's head and counts |

Nothing in the round is Class B or Class C. `ReviewAccount`, `NoReviewAccount`, the two
`never_started` fields and `listed_pid_bytes` are in-memory types and a pure function; nothing new
is serialized; the frozen fold, the frozen event vocabulary and `src/topology/**` are untouched.
The three approved Class B changes and their descriptions are unchanged.

Two readings this round records, because both were choices and not deductions:

- **Finding 3 is repaired by making the read read-only rather than by routing it through the
  effect funnel.** The brief allowed either. There is no site to route it to: the frozen
  `EffectSiteId` names no classification read, every `ObjectSite` variant documents "the row that
  references the created object immediately after the effect", and a classifier creates nothing to
  reference — so adding one is a change under the `src/topology/**` freeze, for a function whose
  whole contract is that it is a read. `git diff-files` is the answer rather than
  `-c diff.autoRefreshIndex=false` because `diff.autoRefreshIndex`'s own documentation excludes the
  plumbing commands from the refresh, which makes the read-only-ness a property of the command
  rather than of a configuration default that a future Git could change.
- **Finding 4 is carried on an in-memory fact and not on a new vocabulary entry.** A
  `FailureKind::RunnerSpawnFailure` would be the obvious shape and is not available: `FailureKind`
  is serialized into `events.jsonl` through the schema-3 records, so a new variant is a Class C
  wire change. `InfrastructureKind::RunnerSpawnFailure` already exists and is exactly what INV-23
  names, so what was missing was never a vocabulary entry but the fact that reaches it.

### The sixth repair round (2026-09-07)

The cover review of the whole slice at `9ee9784e` and its triage are `pr8-triage.md` §9. Four
findings — one P1, one P2, two P3 — of which two are code and two are the record; a fifth row was
raised by this round rather than the reviewer, sweeping the class finding 1 named, in the direction
the census reads. The reviewed head was green on all eleven CI checks across the full
ubuntu/macOS/Windows matrix and on the ten gates locally, and the P1 survived every one of them:

| Commit | Findings |
|---|---|
| `fix(runner): a container is gone when the daemon says so, not when stderr spells it` | finding 1 (`PR8-R6-DOCKER-TEXT-SETTLEMENT`) |
| `fix(engine): a frozen repair carries the gate output and the required changes` | finding 2 (`PR8-R6-REPAIR-EVIDENCE`), the conversion and the test that could not catch it |
| `fix(runner): a daemon that answered was reached, whatever its message quotes` | the class sweep (`PR8-R6-UNREACHABLE-SWEEP`) |
| `docs(pr8): the plan and findings of the sixth repair round` | this file, `pr8-triage.md`: §7.3's normalization clearance corrected and the table re-read for text-derived observations, §7.4's derivation widened past `update-ref` (finding 4, `PR8-R6-CENSUS-DOMAIN-2`), and `pr8-body.md`'s Summary and rollback paragraphs qualified (finding 3, `PR8-R6-V01-CLAIM-2`) |

Nothing in the round is Class B or Class C. Finding 1 changes the wire commands `DockerCli` issues
and the private functions around them, not the `ContainerRuntime` trait's signatures; finding 2
writes a longer string into a `VerificationRecord::detail` that already exists. The frozen fold,
the frozen event vocabulary, the effect-site inventory and `src/topology/**` are untouched, and the
three approved Class B changes and their descriptions are unchanged. The round adds no v0.1
surface: the container runtime has no production constructor outside `src/runner/container/**` —
`run` and `resume` build a `HostRunner` — and `integrate.rs` is schema-4 only.

Three readings this round records, because each was a choice and not a deduction:

- **The grammar was measured before it was enforced.** The repair refuses on a shape — a line the
  daemon spoke — and a refusal written before the producer is enumerated refuses the wrong things.
  Every message the CLI prints for the commands this code runs was transcribed from docker 29.7.2
  first (`r6-evidence/docker-transcription.md`), which is what established that the typed
  subcommands all relay with one marker and all name the target, and that `docker image inspect`
  does not use the generic `docker inspect`'s `Error: No such object:` wording — the one shape that
  would have made a marker requirement break image resolution on a live daemon.
- **The marker is not trusted on its own, and neither is the name.** Text cannot be evidence about
  a process, whatever its shape: a directory can be named for a phrase, and it can be named for a
  container. So the shape only earns the right to *propose*, and the settlement is established
  against a command whose **success** is the daemon answering. That is why the repair is not a
  longer phrase table and why `observe` changed too — it was the one place a settlement could still
  be read out of a diagnostic.
- **`observe`'s status vocabulary and its fallthrough are left exactly as PR6 wrote them.** §7.3
  records that the fallthrough is the class's remaining weak arm and that refusing an unenumerated
  state is the conforming shape but a behaviour change to PR6 code with no reproduction behind it.
  That ruling still holds; this round changes where the *absence* answer comes from and nothing
  about how a state that is present is classified.

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

- **The fourth repair round's witnesses** (`pr8-triage.md` §7.5), each applied by an asserted
  replacement against the repaired tree, the named tests run, and the file restored from `HEAD`;
  each fails exactly the tests named: `decide`'s comparison with the authorized head disabled
  fails `a_foreign_reset_of_the_integration_ref_refuses_before_any_append_and_keeps_the_merged_task`
  with beta published over alpha's lost change; `cancel_reached` reading `RemovalInProgress` as
  `container_gone` fails the two in-progress cells of
  `the_runner_reports_what_it_established_about_the_process_when_it_fails` and
  `a_removal_another_reclaimer_holds_is_not_proof_the_gate_is_gone` (a `Deferred` terminal over
  the running gate), and `removal_answer` normalizing the in-progress answer to `ProcessGone`
  fails those and `a_removal_answer_meaning_already_in_progress_is_tolerated_and_a_real_failure_is_not`;
  the step's cleanup scope removed fails `a_host_integration_reaper_holds_the_runs_cleanup_lease`
  with the observation `[false]`; `reclaim_staging` reading the pin's current target fails
  `a_rejected_or_unavailable_terminal_refuses_to_delete_a_pin_another_writer_substituted` with
  `Rejected` returned as success; every start failure recorded as attempted fails the matrix cell
  "the funnel refuses before `docker start` is attempted and the cancel cannot reach the runtime";
  and the reviewer's own mutation — the integration reviewers' `ReviewCx.workspace` pointed into
  the staging worktree with snapshot creation and cleanup kept, which had passed all 366
  engine-topology tests — fails `stale_candidate_takes_staging_path_and_publishes_pinned_proposal`,
  `terminal_shape_coverage_table_drives_every_shape_and_each_converges_on_replay` and
  `the_production_verifier_judges_the_recorded_proposal_and_removes_its_snapshots_after_the_terminal`.

- **The fifth repair round's witnesses** (`pr8-triage.md` §8.3), applied the same way, except that
  each file is restored from the commit that carries its repair rather than from `HEAD`, because
  `git checkout -- <file>` over an uncommitted repair discards the repair rather than the mutation.
  Each fails exactly the test named: `listed_pid_bytes`'s `errno` arm dropped fails
  `a_pid_enumeration_that_failed_is_not_an_empty_process_group`, which then reads Apple's failure
  answer as an enumeration of nothing; the per-pass charge removed and `verify`'s old
  `record_reviews` on the returned judgement restored fails
  `a_completed_integration_review_is_charged_when_the_next_reviewers_snapshot_fails` at the
  reviewer's own numbers, 1.2999999999999998 → 6.3 with three reviewers where the ceiling admits
  one; `proposal_state`'s `diff-files` restored to the porcelain `diff` fails
  `the_proposal_classifier_writes_no_index_while_reading_an_empty_pick`; `--no-optional-locks`
  dropped from `read_only_git` fails `a_worktree_inspecting_read_writes_no_index`; and the
  never-started arm removed from `integrate::infrastructure` fails
  `a_reviewer_whose_process_never_started_is_a_runner_spawn_failure` with `ReviewUnavailable`
  where INV-23 names `RunnerSpawnFailure`.

  **Two of those tests were vacuous when first written, and the mutation is what said so.** With
  the stat-dirty input made by rewriting a tracked file byte for byte at the time of the test, both
  index-hash tests passed under their own mutations: Git will not cache a stat that is not older
  than the index it is writing, so the refresh had nothing it was permitted to write back and the
  index was unchanged for a reason that had nothing to do with the repair. The input was changed to
  age the file into the past, a different second per call, and measured first in a shell
  reproduction of the same linked-worktree shape — porcelain `diff` moves the index, `diff-files`,
  `diff --cached` and `--no-optional-locks status` do not — before the tests were rerun.

### The residue sampler's sightings on this branch

Moved here from `pr8-body.md` in the sixth repair round, when the body reached GitHub's
65,536-character limit; the body keeps the count, both finding ids and this pointer. The standing
findings ask for a count before anything is called a flake, and this is it.

Two sightings, in a module this branch does not touch. At the third
round's head `287563f0`, the first full run had one red:
`workspace_manager::tests::sampled_git_child_kills_every_residue_classified_and_recovered` refused
one `Worktree.Add` sample with "worktree list record 1 names a HEAD but neither a branch nor a
detached checkout" — the standing P3 `PR172-SAMPLER-REFUSED-A-TORN-WORKTREE-LIST-RECORD`
(`reviews/findings/`); it passed alone and the full test gate rerun passed clean. At `cdcea656`,
the first full run had one red in the same test with the other filed fingerprint: `recover_sample`'s
forced removal failed `DirectoryNotEmpty` (os error 39) on `tasks/kalpha-g4` — exactly the
standing P2 `PR136-SAMPLER-FORCED-REMOVAL-DOES-NOT-CONVERGE` (`reviews/findings/`), whose file
records that fingerprint verbatim; the full test gate rerun at the same head passed clean (2362
passed, 0 failed, 43 ignored), and the ten gates named above are green on that rerun. The fifth
repair round saw neither fingerprint: its three full test runs, at
`85bdb8c7`, `fe7966da` and `243cf617`, each passed clean at the first attempt, so the sampler is
not counted against this round.

The sixth repair round saw neither fingerprint either: its full test runs at the round's repaired
tree passed clean at the first attempt.

## 6. Proof obligations from the contract, and where each is met

Moved here from `pr8-body.md` in the fifth repair round: the pull-request body reached GitHub's
65,536-character limit, and of what it carried this is the part that is a mapping rather than a
statement — every obligation below is discharged by named tests in this tree, so it reads the same
from a tracked file. The body keeps the claim and the list of obligation names, and points here.

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
  candidate's commit and ref, and the rejecting head, all in the registered spec's body — with the
  code rejection's evidence built by the production classifiers and carried through
  `integrate::code_record` since the sixth round, because the version that inserted its own
  `detail` sat downstream of the conversion that was losing it. What the loop actually freezes is
  proven end to end by `integrate::tests::a_failing_gates_own_output_reaches_the_frozen_repair_spec`
  and `a_rejecting_reviewers_required_change_reaches_the_frozen_repair_spec`.
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
- **The head the log authorizes, live** (the fourth repair round).
  `integrate::tests::a_foreign_reset_of_the_integration_ref_refuses_before_any_append_and_keeps_the_merged_task`:
  alpha published, the ref reset to the base by an external writer, beta refused before any
  append with the refusal naming the base, alpha's commit and sequence 0; nothing appended, no
  staging effect, no object, the ref untouched, alpha still `Merged`; the ref put back, beta
  integrates and its publication still carries alpha's change. The rule is one function with two
  callers (`authorized_head`, read by `decide` and by `ensure_recorded_integration_ref`).
- **A removal another reclaimer holds is not evidence.**
  `the_runner_reports_what_it_established_about_the_process_when_it_fails` is now a sixteen-cell
  matrix: the two cells with the daemon's removal-in-progress answer are `Unresolved` with the
  container surviving `Running` and its view and intent retained, and the cell with the funnel
  refusing before `docker start` is `NeverStarted` with no `Start` op;
  `recover::tests::a_removal_another_reclaimer_holds_is_not_proof_the_gate_is_gone` drives the
  answer through the loop (no terminal, the snapshot retained, the transaction open holding its
  entitlements); `a_removal_answer_meaning_already_in_progress_is_tolerated_and_a_real_failure_is_not`
  pins the typed normalization, and the fake routes an armed diagnostic through the production
  normalizers.
- **The reapers hold the lease.** `recover::tests::a_host_integration_reaper_holds_the_runs_cleanup_lease`
  (Unix): a stale verification re-verified through the production `HostRunner`, the real reaper
  observed holding the run's `cleanup.lock` at `ReaperStarted`, and the hold released once the
  reaper is gone.
- **A substituted pin at a live terminal.**
  `integrate::tests::a_rejected_or_unavailable_terminal_refuses_to_delete_a_pin_another_writer_substituted`:
  rejected and parked, the refusal names the foreign object, the pin still names it, the terminal
  is the last durable event, and the staging worktree and snapshots are reclaimed.
- **The reviewers' checkouts.** The review doubles run a process in the workspace they are
  handed, and `stale_candidate_takes_staging_path_and_publishes_pinned_proposal`, the terminal
  coverage table and `the_production_verifier_judges_the_recorded_proposal_and_removes_its_snapshots_after_the_terminal`
  assert each reviewer's HEAD is the recorded proposal and its workspace is its own
  `integration_review` snapshot slot, never staging and never the gate's snapshot; the reviewer's
  mutation that had passed 366 tests fails all three.
- **Both class sweeps** are recorded in `pr8-triage.md` §7.3 and §7.4: every production site that
  concludes a process gone, with what it observed, and every cleanup that takes an expected-old
  value or removes a resource, with where its authority comes from.
- **Mutation witnesses replayed against the repaired tree**, each killed by the test its ledger
  row names: the reviewer's m01, m02, m03, m05 and m06; the barrier sync removed; one mutation
  per repair of the first round; and, for the second round, the blanket Runner-error conversion
  restored, the Git arm removed, the timed-out-gate check disabled, the provenance rule restored
  at integration, the spec body not embedded, the two snapshot mutations the reviewers found
  surviving, and the sampler's kill deleted; and, for the third round, the two mutations that
  survived the suite at `79ddbffb` (M3, the host's post-spawn `Unresolved` made `Gone`; M4, a
  timed-out output with a failed release made `Gone`) and one mutation per repair of the round
  (`pr8-plan.md` §5); and, for the fourth round, the seven mutations of `pr8-triage.md` §7.5,
  the reviewer's staging mutation among them.
