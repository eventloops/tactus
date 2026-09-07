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
pending, the integration ref must name the log's latest publication — and the live exact-base
decision reads the same rule, so a head the log did not put there refuses before any append.

This slice is inert by default: the schema-4 topology engages only by explicit schema choice, and
the v0.1 path is unchanged **but for one declared exception** — an unresolved host reviewer now
propagates the Runner's error through the legacy attempt instead of producing an unavailable-review
result. That change, and the type changes around it, are stated in Risk and rollback below; it is
the only behaviour of the released path this branch alters.

## Scope

In scope: `src/engine/topology/integrate.rs` (the transaction, all terminals, the exact-base
decision, the CAS and publish, the recovery primitives), `src/engine/topology/repair.rs` (the
frozen-repair builder), the integration recovery in `src/engine/topology/recover.rs`
(`finish_integration`, `reclaim_stale_residue`, the published-run form of
`ensure_recorded_integration_ref`), the loop and checkpoint wiring in `run.rs`/`select.rs`
(the verification context, the implementer binding, the review-input classification, the charged
reviews), the judge's typed Runner error and snapshot disposal in `attempt.rs`, the verification
harness in `scaffold.rs`, and the supporting `workspace_manager` reads (`proposal_state`) and
names (`SnapshotName::integration_review`). The five repair rounds change the same seams and add no new ones. `pr8-plan.md` §2 carries each
round's commit table and its file list; in outline: rounds two and three are the Runner's typed
error and what each of the two Runners may claim about a process (`src/runner/**`,
`src/agent/proc.rs`, `src/review.rs`, `src/engine/classify.rs`, `src/engine/topology/preflight.rs`,
`create.rs`, and every Runner double); round four is the shared head rule and the emit funnel's
event mirror (`integrate.rs`, `recover.rs`, `emit.rs`, `run.rs`), the typed outcome of a container
stop or removal (`src/runner/container/**`), the owned cleanup scope (`src/rundir.rs` and the three
sites that spawn under the lock), and review doubles that run a process in the workspace they are
handed (`scaffold.rs`, `recover/tests.rs`); round five is the review account each completed pass is
charged to (`attempt.rs`, `run.rs`, `select.rs`, `scaffold.rs`), the fact of a process that never
started carried to the durable outage attribution (`src/review.rs`, `src/ladder.rs`,
`src/engine/attempt.rs`, `integrate.rs`), the macOS scanner's distinction between a failed
enumeration and an empty group (`src/agent/proc.rs`), and two Git reads made read-only
(`src/workspace_manager.rs`).

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
  container runner says `NeverStarted` for any failure before `docker start` was attempted — the
  funnel itself says whether it was, so a refusal before the primitive is never an attempted start
  — and, after it, `Gone` only when the runtime observed the exit or answered a stop or a forced
  removal with `Settled::ProcessGone`; the daemon's "removal already in progress" answers
  `Settled::RemovalInProgress` and establishes nothing, because the daemon sets that flag before
  it kills, and a container the runtime confirmed neither stopped nor removed keeps its view and
  intent (R25, corrected in the fourth repair round).
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
- The live exact-base decision and the resume's startup check read one rule for that head,
  `integrate::authorized_head`, derived from the event list the run carries and kept current by
  the one emit funnel; a head the log did not put there refuses before any append, and the fold
  retains nothing new (R29).
- The topology run enters its lock's cleanup scope wherever it spawns host processes under the
  lock — each step of the loop, the resume's probes, creation's probes — so its Unix reapers hold
  the shared `cleanup.lock` that R28 says the next coordinator observes (R30).
- The two-crash proof's "unsynced `merge_prepared`" is a complete line whose flush was made to fail
  after the full write; the restart is a child process killed at the `task_merged` write; "power
  loss" truncates the log to the length the barrier proved durable.

## Validation

All ten gates green locally, from the repository root, on the last code commit of this branch,
`243cf617f5ed8070cdd0951e56db4d560c0eb699`; the commits that follow it change the three record
files and no code, and the ten gates were rerun on the pushed head before the push:

```
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features            # 2367 lib tests passed, 0 failed, 43 ignored (Linux)
cargo +1.85.0 check --locked --all-targets --all-features
bash .github/scripts/test-release-record.sh
bash .github/scripts/test-pr-policy.sh
bash .github/scripts/test-pr-ledger-evidence.sh
bash .github/scripts/test-docs-consistency.sh
bash .github/scripts/test-internals-notes.sh
bash .github/scripts/test-pr-ready-audit.sh
```

Two sightings of the residue sampler, in a module this branch does not touch, are recorded here
because the standing findings ask for a count before anything is called a flake. At the third
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

Two platform checks the box can make and the local gates do not. The macOS repair changes code no
Linux gate compiles, so the branch is also `cargo clippy --target x86_64-apple-darwin --all-targets
--all-features -- -D warnings` clean here, which type-checks and lints the whole macOS `cfg` tree
including the scanner and its `errno` protocol. **It is not executed on this box** — CI's macOS leg
is the first thing that runs it, and the rule the repair rests on is exercised on every platform by
`listed_pid_bytes`'s test.

Proof obligations from the contract, and where each is met: the enumeration, one bullet per
obligation with the tests that discharge it, is **`pr8-plan.md` §6**, tracked at this head. It
moved there in the fifth repair round because the body reached GitHub's 65,536-character limit for
a pull-request description and the ledger below, which the policy gate greps from the published
body, cannot move. Nothing was dropped: the twenty-two obligations are real-repository CAS, orphan
and third-SHA publication; fast with no staging; the three fast mismatches live and on replay; the
stale path; the two-crash proof; completed publications resuming; the terminal-shape coverage
table; kill and residue for the cherry-pick class; outages, parks and answers driven through the
loop; a lost gate process never settled over; the fate as process evidence at both Runners; the
production verifier observed through the loop; the frozen repair spec; refusals proven rather than
coded; verification isolation; the head the log authorizes on the live path; a removal another
reclaimer holds not counting as evidence; the reapers holding the lease; a substituted pin at a
live terminal; the reviewers' checkouts; both class sweeps; and the mutation witnesses replayed
against the repaired tree.

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

A cover review of the whole slice against master was then run by the owner against the exact
head `8a5f59e8` — the branch merged up to master, ten gates green locally and the full CI matrix
green — by a frontier reviewer at `ultra` effort (the review record the owner supplied names the
effort and not the model), returning `CHANGES_REQUIRED` with six findings, four P1, each with a
reproduction the reviewer ran: the live decision accepting a foreign ref reset and recording lost
work as merged; the daemon's removal-in-progress answer read as proof a container process was
gone; the topology path's reapers holding no cleanup lease; the rejected and unavailable cleanup
deleting a substituted pin; a start refused before `docker start` recorded as attempted; and the
review doubles ignoring their workspace, so reviewers executing in staging passed every test. Two
of the six were recurrences of classes earlier rounds had declared fixed. Every finding was
triaged in `pr8-triage.md` §7 (Claude Fable 5.1 at max effort, a fresh autonomous session,
2026-09-07) and confirmed; every one was repaired on this branch with a test that fails without
the repair, and both classes were swept across the tree — every site that concludes a process
gone and every cleanup that takes an expected-old value, each listed with where its evidence
comes from (§7.3, §7.4). What the reviewer cleared is undisturbed, no new Class B change was
needed, and neither deferred finding was reopened.

A final cover review of the whole slice against master was then run by the owner against the exact
head `716cf89a` — the branch merged up to master, ten gates green locally and all eleven CI checks
green across the ubuntu, macOS and Windows matrix — by a frontier reviewer at `ultra` effort (the
review record the owner supplied names the effort and not the model), returning `CHANGES_REQUIRED`
with six findings: two P1, three P2, one P3, and two of the six in code that predates this slice.
It cleared the structural core again — no stable-prefix or CAS ordering defect, no undeclared
Class C, the three approved Class B descriptions matching the code — and what it found was edges:
macOS process-enumeration failures read as an empty process group, because Apple's `proc_listpids`
answers a failed call with the same zero it answers an empty group with; completed review costs
discarded live when a later snapshot failed, so the loop admitted another sequence in the same
incarnation against a total that was missing them; the read-only proposal classifier writing the
index outside every effect hook; a reviewer's image mismatch attributed as `ReviewUnavailable`
where INV-23 names `RunnerSpawnFailure`; the §7.4 cleanup census not exhaustive though it claimed
to be; and the body's unchanged-v0.1 and rollback claims contradicting its own Risk section. Every
finding was triaged in `pr8-triage.md` §8 (Claude Fable 5.1 at max effort, a fresh autonomous
session, 2026-09-07) and confirmed; the four code findings were repaired on this branch with a test
that fails without the repair, and the two record findings were corrected in the record. A seventh
row was raised by this round rather than the reviewer, sweeping the class finding 3 named:
`PR5-CONF-002` had established that Git's porcelain writes the index it is only asked to read and
applied `--no-optional-locks` at one call site, and the rest of the manager's reads never got it.
§7.3 is corrected for the macOS row and re-examined end to end for the species of error behind it —
an observation whose failure mode is indistinguishable from its success value — and §7.4's domain
is restated and completed across both the schema-4 and the legacy layer. Neither deferred finding
is reopened; finding 2 is adjacent to `PR8-R2-SPEND-REPLAY` and distinct from it, and §8.2 records
why, so the two are not folded together later.

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
build box and executes on the winguest CI leg; it was not run locally. The fourth repair round
changes the `ContainerRuntime` trait's `stop` and `remove` to answer what they established
(`Settled`), which every runtime double follows; makes the run lock's cleanup scope an owned value
entered per step of the loop, per recovery order and per creation's probe stretch, where the v0.1
coordinator and resume hold a borrowed one for their whole command (those two sites are unchanged
in behaviour); and mirrors every successful append into the event list the run carries, in the one
emit funnel, so the live head rule reads what recovery appended.

The fifth repair round touches the v0.1 path in type only. `review_failure` takes a second
argument, and `ReviewOutcome` and `AttemptFailure` each gain one in-memory field recording whether
the Runner established that the process never started; nothing serializes any of it and the legacy
ladder reads none of it, so every legacy path answers exactly what it answered before. The round's
other changes are schema-4 only or are reads: the review account is on the topology judge and the
legacy attempt passes `NoReviewAccount`, and the two Git reads made read-only —
`WorkspaceManager::proposal_state`'s unmerged-entry query and `read_only_git`'s
`--no-optional-locks` — are both in the schema-4 workspace manager, which no v0.1 command uses.
The macOS process-group scanner is shared by both engines and its change is a refusal where it
previously answered wrongly: a failed enumeration is now unknown rather than an empty group, which
the reaper's loop already treats as "keep killing", so on the one platform it affects the released
path becomes fail-closed where it was fail-open. **It executes for the first time on CI's macOS
leg**; this box is Linux and the platform-independent half of the rule is what the test covers.


Two known gaps were deferred by the owner on 2026-09-07, each with a standing finding filed in
`reviews/findings/`. `PR8-CRASH-002`: a lock file left by a coordinator
killed inside `git update-ref`; the refusal it causes is resumable and loses nothing, and the
operator's removal of the lock lets the next resume complete the publication; reclaiming it needs
a `Ref.*` residue class in the frozen inventory. `PR8-R2-SPEND-REPLAY`: a paid review that parks
or meets an outage is charged live and lost on replay, so a restart can admit an integration the
previous incarnation's total would have refused; recording it needs a review record on the
unavailable terminal, which the frozen vocabulary lacks.

Rollback is clean: the schema-4 topology is inert unless a plan selects it, so reverting the branch
removes the machinery and restores the released path exactly — including the one behaviour of it
this branch changes, the unresolved host reviewer declared above, which reverts with everything
else. Nothing else in the v0.1 path differs, so a revert has no other released behaviour to undo.
No data migration, no on-disk format change outside the run-scoped
`refs/upstroke/runs/<run>/…` namespace this slice owns.

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
| PR8-R4-LIVE-HEAD | P1 | 8a5f59e8ca2272e253e326685d15136c6008e1d0 / src/engine/topology/integrate.rs:222 | alpha publishes P -> an external writer resets the integration ref from P to the base -> beta's decide compares the head only with beta's base -> fast -> beta published over the lost publication and alpha stays Merged with its change gone | introduced_by_feature | correctness | 12448215 | `a_foreign_reset_of_the_integration_ref_refuses_before_any_append_and_keeps_the_merged_task`; the rule is `authorized_head`, shared with the resume check | fixed |
| PR8-R4-REMOVAL-IN-PROGRESS | P1 | 8a5f59e8ca2272e253e326685d15136c6008e1d0 / src/runner/container.rs:538 | a gate starts -> observe and stop are lost -> docker rm answers removal already in progress, normalized to Ok -> cancel_reached reads it as container_gone -> the runner reports Gone beside the running gate -> Deferred appended and the snapshot removed | fix_regression | correctness | 52d1fa05 | `the_runner_reports_what_it_established_about_the_process_when_it_fails`, `a_removal_another_reclaimer_holds_is_not_proof_the_gate_is_gone`, `a_removal_answer_meaning_already_in_progress_is_tolerated_and_a_real_failure_is_not` | fixed |
| PR8-R4-CLEANUP-LEASE | P1 | 8a5f59e8ca2272e253e326685d15136c6008e1d0 / src/engine/topology/run.rs:768 | the topology run acquires its lock and enters no cleanup scope -> a gate's reaper is spawned with no lease path active -> the coordinator dies during the gate -> the next resume's exclusive probe finds no hold and continues while the reaper still reclaims the group | pre_existing | crash-consistency | 1db49779 on master, the attempt path's omission that this branch routes integration verification through | `a_host_integration_reaper_holds_the_runs_cleanup_lease` | fixed |
| PR8-R4-SUBSTITUTED-PIN-LIVE | P1 | 8a5f59e8ca2272e253e326685d15136c6008e1d0 / src/engine/topology/integrate.rs:869 | verification records proposal P and pins it -> another writer moves the pin to X -> the verification rejects or parks -> reclaim_staging reads X and deletes expected-old at X -> the substituted ref is gone and the terminal returns success | introduced_by_feature | correctness | 2d1b4c72 | `a_rejected_or_unavailable_terminal_refuses_to_delete_a_pin_another_writer_substituted` | fixed |
| PR8-R4-START-NOT-ATTEMPTED | P2 | 8a5f59e8ca2272e253e326685d15136c6008e1d0 / src/runner/container/exec.rs:618 | the funnel refuses at Container.Start's Before phase -> docker start is never issued -> the launch records Started -> with stop and remove unavailable the fate is Unresolved instead of NeverStarted -> the transaction stays open instead of consuming the outage deferral | fix_regression | correctness | 52d1fa05 | `the_runner_reports_what_it_established_about_the_process_when_it_fails`, the cell with the funnel refusing before `docker start` | fixed |
| PR8-R4-REVIEW-ORACLE | P2 | 8a5f59e8ca2272e253e326685d15136c6008e1d0 / src/engine/topology/recover/tests.rs:7315 | both review doubles ignore the workspace they are handed -> the integration reviewers' workspace is pointed into staging -> snapshot creation and cleanup kept -> all 366 engine-topology tests pass while verification_isolation is violated | introduced_by_feature | correctness | 2d1b4c72 | `stale_candidate_takes_staging_path_and_publishes_pinned_proposal`, `terminal_shape_coverage_table_drives_every_shape_and_each_converges_on_replay`, `the_production_verifier_judges_the_recorded_proposal_and_removes_its_snapshots_after_the_terminal`; the doubles run a process in the workspace they are handed | fixed |
| PR8-R5-MACOS-ENUMERATION | P1 | 716cf89af6817b7797ca954944c83dd37ea7a7f3 / src/agent/proc.rs:3811 | a host gate times out -> SIGKILL is issued and a same-group descendant has not terminated -> proc_listpids fails and Apple's wrapper reports the failure as a return of zero -> the scanner reads zero as an empty enumeration and answers Some(false) -> the reaper's cleanup loop exits at once, reaps the anchor and acknowledges CLEANUP -> Supervisor::finish succeeds beside the survivor, permitting termination reporting, snapshot removal and release of the cleanup lease | pre_existing | correctness | the macOS scanner as first written; the Linux scanner has no such ambiguity | `a_pid_enumeration_that_failed_is_not_an_empty_process_group` over `listed_pid_bytes`, which is compiled and exercised on every platform because the scanner around it compiles only on macOS | fixed |
| PR8-R5-DISCARDED-REVIEW-COST | P1 | 716cf89af6817b7797ca954944c83dd37ea7a7f3 / src/engine/topology/attempt.rs:624 | 1.30 is charged under a 2.20 run ceiling -> the first integration review returns and costs 2.50 -> creating the second reviewer's snapshot fails with a Git error -> ? discards the judge's completed-review vector and charging happens only on a successful judgement return -> the sequence settles unavailable rather than ending the command -> the loop admits another sequence in the same incarnation and 6.30 is accounted against 8.80 spent | introduced_by_feature | correctness | 2d1b4c72 | `a_completed_integration_review_is_charged_when_the_next_reviewers_snapshot_fails`; the rule is `ReviewAccount`, reported as each pass returns, and `verify` no longer charges from the judgement | fixed |
| PR8-R5-CLASSIFIER-INDEX-WRITE | P2 | 716cf89af6817b7797ca954944c83dd37ea7a7f3 / src/workspace_manager.rs:2609 | a proposal cherry-pick is already present -> proposal_state is called to classify the failure -> its porcelain git diff runs update-index --refresh against the working tree -> index.lock is created and renamed over index -> a function that takes no hooks and names no effect site has written a resource outside every effect hook | introduced_by_feature | correctness | 2d1b4c72 | `the_proposal_classifier_writes_no_index_while_reading_an_empty_pick` hashes the index across the whole call; the read is the plumbing `diff-files`, which `diff.autoRefreshIndex` documents as outside the porcelain refresh | fixed |
| PR8-R5-READ-ONLY-SWEEP | P2 | 716cf89af6817b7797ca954944c83dd37ea7a7f3 / src/workspace_manager.rs:3416 | the residue classifier asks whether an interrupted git add published its blobs -> worktree_has_unstaged_changes runs git status --porcelain -> git takes the index lock opportunistically to write back a refreshed stat cache -> the classifier rewrites the index of the worktree it is classifying, and its own index.lock is what a later classification reads as proof the publication never happened | pre_existing | correctness | PR5-CONF-002, which named this mechanism and applied the flag at one call site | `a_worktree_inspecting_read_writes_no_index`; `--no-optional-locks` moves into `read_only_git` itself and the per-call constant is removed, so dropping it is a single witnessed change | fixed |
| PR8-R5-REVIEWER-SPAWN-FAILURE | P2 | 716cf89af6817b7797ca954944c83dd37ea7a7f3 / src/review.rs:547 | a reviewer's container reports an image id that is not the recorded one -> the runner refuses before docker start and answers NeverStarted -> run_review contains it as Unavailable{AgentError} so the pass defers instead of ending the command -> review_failure maps it to ReviewUnavailable -> integration persists Infrastructure{ReviewUnavailable} where INV-23 requires RunnerSpawnFailure for a mid-run image mismatch, reviewers and re-asks included | introduced_by_feature | docs-contract | 2d1b4c72 | `a_reviewer_whose_process_never_started_is_a_runner_spawn_failure`, with a Gone control; `review_infrastructure_failures_become_unavailable_outcomes` pins the production run_review carrying the fate | fixed |
| PR8-R5-CENSUS-DOMAIN | P2 | 716cf89af6817b7797ca954944c83dd37ea7a7f3 / pr8-triage.md:298 | the cleanup census is derived by grepping the schema-4 manager's primitive names -> the legacy Workspace reaches Git through git update-ref and git worktree remove --force directly -> no legacy site can appear -> the section claims every production site and states that only two orphan prunes read their expected-old from the ref, while workspace.rs:1157 is a third | introduced_by_feature | docs-contract | 1e2a0733 | the domain is restated as every ref deletion, ref move, worktree removal and snapshot removal in both layers, the grep that derives it is named, a legacy table is added with each site's authority, and the count is corrected to three; no runtime defect was established in the omitted operations, by the reviewer or here | fixed |
| PR8-R5-V01-CLAIM | P3 | 716cf89af6817b7797ca954944c83dd37ea7a7f3 / pr8-body.md:37 | the Risk section declares that an unresolved host reviewer now propagates an error through the legacy attempt -> the Summary says the v0.1 path is unchanged and the rollback paragraph says a revert touches no released behaviour -> the body contradicts itself about the one released behaviour this branch changes | introduced_by_feature | docs-contract | cd4610f6 | both sentences are qualified against the declared exception and the Risk section carries the fifth round's own v0.1 surface; `validate-pr-body.sh` holds the sections | fixed |
