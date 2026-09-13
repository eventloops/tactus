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
decision reads the same rule, so a head the log did not put there refuses before any append. So
does a fresh dispatch, which takes that head rather than the base the run started at: a task
dispatched after its dependency merged gets a worktree with the dependency's work in it.

This slice is inert by default: the schema-4 topology engages only by explicit schema choice, and
the v0.1 path is unchanged **but for two declared exceptions**. An unresolved host reviewer now
propagates the Runner's error through the legacy attempt instead of producing an unavailable-review
result. And the process-group scanner both engines share reads a **macOS** enumeration that failed
as unknown rather than as an empty group, so a legacy worker or gate whose cleanup meets that
failure keeps killing instead of settling — fail-closed where it was fail-open, on that one
platform. Both, and the type changes around them, are stated in Risk and rollback below; they are
the only behaviours of the released path this branch alters.

## Scope

In scope: `src/engine/topology/integrate.rs` (the transaction, all terminals, the exact-base
decision, the CAS and publish, the recovery primitives), `src/engine/topology/repair.rs` (the
frozen-repair builder), the integration recovery in `src/engine/topology/recover.rs`
(`finish_integration`, `reclaim_stale_residue`, the published-run form of
`ensure_recorded_integration_ref`), the loop and checkpoint wiring in `run.rs`/`select.rs`
(the verification context, the implementer binding, the review-input classification, the charged
reviews), the judge's typed Runner error and snapshot disposal in `attempt.rs`, the verification
harness in `scaffold.rs`, and the supporting `workspace_manager` reads (`proposal_state`) and
names (`SnapshotName::integration_review`). The eight repair rounds change the same seams and add no new ones;
**`pr8-plan.md` §2 carries each round's commit table and file list**. Beyond the files above they
reach the Runner's typed error and what each Runner may claim about a process (`src/runner/**`,
`src/agent/proc.rs`, `src/review.rs`, `src/engine/classify.rs`, `preflight.rs`, `create.rs`, and
every Runner double), the emit funnel's event mirror (`recover.rs`, `emit.rs`), the owned cleanup
scope (`src/rundir.rs` and the three sites that spawn under the lock), the review account and the
never-started fact (`src/ladder.rs`, `src/engine/attempt.rs`), and two Git reads made read-only
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
  group (Unix) or the job (Windows) was established empty, never from the direct child's reap, and
  the container runner only when the runtime observed the exit or answered a stop or forced
  removal with `Settled::ProcessGone`. **Every site that concludes a process gone is enumerated
  with what it observed in `pr8-triage.md` §7.3**, re-derived at this head. Foreign Git state
  observed by the verification and a gate that times out are outages of the sequence
  (`Infrastructure{Other}`, R26, R27); every other verification error ends the command resumably
  (R24).
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
- The live exact-base decision, the resume's startup check and a fresh dispatch read one rule for
  that head, `integrate::authorized_head`, derived from the event list the run carries and kept
  current by the one emit funnel; a head the log did not put there refuses before any append, and
  the fold retains nothing new (R29).
- The topology run enters its lock's cleanup scope wherever it spawns host processes under the
  lock — each step of the loop, the resume's probes, creation's probes — so its Unix reapers hold
  the shared `cleanup.lock` that R28 says the next coordinator observes (R30).
- The two-crash proof's "unsynced `merge_prepared`" is a complete line whose flush was made to fail
  after the full write; the restart is a child process killed at the `task_merged` write; "power
  loss" truncates the log to the length the barrier proved durable.

## Validation

All ten gates green locally, from the repository root, on the last code commit of this branch,
`aaf289f1215da02fd3d812912d8715f94dd1d0b6`; every commit that follows it changes the three record files and one
finding file and no code, and the ten gates were rerun on the pushed head before the push:

```
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features            # 2376 lib tests passed, 0 failed, 43 ignored (Linux)
cargo +1.85.0 check --locked --all-targets --all-features
bash .github/scripts/test-release-record.sh
bash .github/scripts/test-pr-policy.sh
bash .github/scripts/test-pr-ledger-evidence.sh
bash .github/scripts/test-docs-consistency.sh
bash .github/scripts/test-internals-notes.sh
bash .github/scripts/test-pr-ready-audit.sh
```

The residue sampler, in a module this branch does not touch, was red twice across the branch's
runs — at `287563f0` and at `cdcea656` — each time with one of its two filed fingerprints
(`PR172-SAMPLER-REFUSED-A-TORN-WORKTREE-LIST-RECORD`,
`PR136-SAMPLER-FORCED-REMOVAL-DOES-NOT-CONVERGE`), each time passing alone and on a full rerun at
the same head with the ten gates green. A **third** fingerprint appeared once, on CI's winguest leg
at `6ac29984` — the *cherry-pick* sampler refusing one of eight samples on a staging worktree's
`index.lock`, `Access is denied (os error 5)`, with that run's other ten checks green and the same
test passing on ubuntu and macOS within it — and it passed on the next head, all eleven checks
green. Filed as `PR247-SAMPLER-REFUSED-A-LOCKED-INDEX-ON-WINDOWS` rather than called a flake; all
three are in `findings/`. No fingerprint appeared in the fifth, sixth or seventh rounds'
local runs, and every local run that measured this worktree passed clean at the first attempt. The
sightings in full are `pr8-plan.md` §5.

The run above is on a target directory private to this worktree, taken after a touch inside the
gate lock so nothing came from cache: each of its three compiling steps — clippy, test, msrv —
logs `Compiling`/`Checking upstroke v0.1.0 (/srv/worktrees/pr8)`, and no other worktree path
appears in any of the nine logs (`pr8-triage.md` §9 is why that is checked).

The sixth round's two Docker-gated tests **ran** here rather than skipping, against a live daemon
(docker 29.7.2). Where no daemon answers they skip, as every `real_docker_*` test does, and the
rules they measure are pinned on every platform by the unit tests beside them; `pr8-triage.md` §9
records what each measured.

Two platform checks the box can make and the local gates do not: this head is
`cargo clippy --target <t> --all-targets --all-features -- -D warnings` clean for both
`x86_64-apple-darwin` and `x86_64-pc-windows-msvc`, which lints each `cfg` tree — the macOS scanner
and its `errno` protocol, the Windows job funnel and its `BOOL` guard. **Neither executes on this
box**; CI's macOS and winguest legs are the first things that run them.

**A third platform check this round adds, and it executes.** The seventh round's witness,
`a_dependent_task_is_dispatched_into_its_dependencys_merged_work`, was red on CI's winguest leg at
`91fe35b0` and `fc141710` with alpha's file *present* in beta's checkout and its bytes
`the candidate edit\r\n`: Git for Windows' system `core.autocrlf=true`, inherited by the recover
fixture's repository, and `git worktree add` rendering the LF blob as CRLF. The dispatch it
witnesses is correct on Windows — with `core.autocrlf=false` injected through the environment and
nothing else changed, every assertion in the test passed there. The fixture now pins its line
endings in its repository config, and the repaired tree was verified on the persistent Windows
guest (Windows 10.0.26100, git 2.50.1.windows.1, cargo 1.97.1; a fresh clone in its own directory
with its own target directory, the tree without the pin red there first) **before** the push: the
test alone, the recover module, and the full suite as CI runs it — `2300 passed; 0 failed; 40 ignored`.
`pr8-triage.md` §11 is the reproduction, the attribution and both witnesses.

Proof obligations from the contract, and where each is met: the enumeration is **`pr8-plan.md`
§6**, tracked at this head, **twenty-two** bullets, one per obligation, each naming the tests that
discharge it. The obligation this round touches is "the head the log authorizes, live and at
dispatch", widened rather than added. It and four other passages live outside this body because
the body reaches GitHub's 65,536-character limit for a pull-request description and the ledger
below, which the policy gate greps from the published body, cannot move: the sampler's sightings
in full (`pr8-plan.md` §5) and the round-by-round review narrative for rounds one to five
(`pr8-triage.md` §§2–8), each leaving its claim, its identifiers and a pointer here. The published
body is 57996 characters after this round compressed the ledger's rows through round seven
to their identity columns (the ledger's preface says where their full text is); a round with
something to add moves something of its own out first, as the fifth, sixth, seventh and eighth
did.

## Review evidence

Implemented by Claude Fable 5.1 at max effort, running as an autonomous Claude Code session in
one continuous run against the frozen contract at `decisions.pr_sequence[9]` of the parallelism
packet as amended by the 2026-08-25 G2 errata, finishing on head
`9d0359394e63878e51abda1ec5c54c6f94578363` with the ten gates green locally; the body commit
`3414dc5861c9a523342ea0792a54f0129cf82f2f` followed. Commit-per-terminal-shape, as the packet's
own size mitigation asks. Both repair rounds below were Claude Fable 5.1 at max effort, each a
fresh autonomous session. (An earlier version of this paragraph named Opus 4.8 at high effort;
the plan named the correct model and the body was wrong.)

Three independent frontier reviews (conformance, crash consistency, test adequacy) were then run
by the owner against the exact head `3414dc58`, each by `gpt-6-astra` at `max` effort, returning
`CHANGES_REQUIRED` with F1–F9, six crash findings and thirteen from a 24-mutation audit. Three more
followed at `916852c9` over the repair range only (repair adequacy, regression, record honesty),
and one at `79ddbffb` over the `Runner` trait's new error type, the one thing in the round no
reviewer had seen — five findings, three P1, with two mutations that had survived the whole suite.
Every finding of all seven reviews was triaged in `pr8-triage.md` §§2–6 (Claude Fable 5.1 at max
effort, a fresh autonomous session per round, 2026-09-07); all were confirmed and repaired with a
test that fails without the repair, except one rejected as a code defect and confirmed as a record
defect — the sentence that misled the reviewer was this branch's own triage — and the two gaps
deferred with a ledger row and an owner decision (`PR8-CRASH-002`, `PR8-R2-SPEND-REPLAY`; R22,
which had called the second a permitted reading, is withdrawn). The narrative of those rounds, and
what each reviewer cleared, is `pr8-triage.md` §§2–6 and `pr8-plan.md` §2; three things from it
belong here. The second round's outage handling made something worse before it made it better —
every Runner error settled as a spawn failure, so a gate whose container Docker had lost was
released and the next sequence admitted beside it. CI's macOS leg failed once in the residue
sampler on a killed `git cherry-pick`'s `packed-refs.lock`, the Ref-lock residue class
`PR8-CRASH-002` defers, and the sampler now removes git's ref-lock residue after each kill as an
operator would. And the two documentation defects the owner was being asked to approve on — the
over-limit `HumanBinding` description and the empty-intersection ladder — are corrected, the
description to what the fold does and the code to what R10 recorded.

Two cover reviews of the whole slice against master followed, each run by the owner against an
exact head that was green on the ten gates locally and on the full CI matrix, and each triaged by
Claude Fable 5.1 at max effort in a fresh autonomous session on 2026-09-07. At `8a5f59e8`, at
`ultra` effort: six findings, four P1, each with a reproduction the reviewer ran — the live
decision accepting a foreign ref reset and recording lost work as merged, a removal-in-progress
answer read as proof a process was gone, the topology reapers holding no cleanup lease, a
substituted pin deleted, a start refused before `docker start` recorded as attempted, and review
doubles ignoring their workspace so reviewers executing in staging passed every test; two were
recurrences of classes earlier rounds had declared fixed. At `716cf89a`, also `ultra`: six
findings, two P1, three P2, one P3, two of them in code that predates this slice — macOS
process-enumeration failures read as an empty group, completed review costs discarded live, the
read-only proposal classifier writing the index outside every effect hook, a reviewer's image
mismatch attributed as `ReviewUnavailable` where INV-23 names `RunnerSpawnFailure`, the §7.4
census not exhaustive though it claimed to be, and the body's unchanged-v0.1 claims contradicting
its own Risk section. Every finding of both rounds was confirmed and every code finding repaired
with a test that fails without the repair; both of the fourth round's classes were swept across
the tree (§7.3, §7.4), and each round raised a further row itself. **`pr8-triage.md` §§7–8 is the whole
of both narratives, with the mutations replayed and the readings taken**; two things from them
belong here. Neither deferred finding was reopened, and the fifth round's finding 2 is **adjacent
to `PR8-R2-SPEND-REPLAY` and distinct from it** — §8.2 records why, and the two are not to be
folded together later.

A further cover review of the whole slice against master, run by the owner against the exact
head `9ee9784e` (ten gates green locally, all eleven CI checks green across the ubuntu, macOS and
Windows matrix), returned `CHANGES_REQUIRED` with one P1, one P2, two P3: a container runtime
could be told a container was gone by text the environment wrote — the normalizers searched a
failed `docker` command's whole stderr, and a path named `no such container` under missing TLS
material was quoted back before the daemon was contacted (`PR8-R6-DOCKER-TEXT-SETTLEMENT`,
reproduced natively by the reviewer with controls); the frozen repair losing the failing gate's
output and the reviewer's required changes, with a test that could not catch it because it
inserted its evidence downstream of the conversion (`PR8-R6-REPAIR-EVIDENCE`); and two record
findings. Triaged in `pr8-triage.md` §9 (Claude Fable 5.1 at max effort, a fresh autonomous
session, 2026-09-07): every code finding confirmed and repaired with a test that fails without
the repair, the record findings corrected, a fifth row raised by the round itself sweeping the
same class in the direction the container census depends on (`PR8-R6-UNREACHABLE-SWEEP`), §7.3's
normalization clearance withdrawn as false and §7.4's derivation widened past `update-ref` to the
other ways Git moves a ref. Neither deferred finding is reopened.

The latest review, at the exact head `eb4e2997` — ten gates green locally and all eleven CI
checks green across the matrix — was asked for a **merge decision** rather than a findings list,
and returned `CHANGES_REQUIRED` on **one defect and no others**: freshly dispatched tasks could
not see their merged dependencies, because every `DispatchRequest` took the run's *starting* base
(`PR8-R7-DISPATCH-BASE`; the reviewer executed it against unchanged production sources with a
recording Runner; the line predates this slice, and PR8 turns a correct assumption into a wrong
one, the same shape as `PR8-R4-CLEANUP-LEASE`). Triaged in `pr8-triage.md` §10 (Claude Fable 5.1
at max effort, a fresh autonomous session, 2026-09-08), confirmed and repaired: a dispatch takes
the head the log authorizes, through the same `authorized_head` the live decision and the resume
check read, having confirmed the integration ref is at it before anything is appended. The
regression drives the loop end to end and asserts on the **contents of beta's worktree**, because
the durable record and the worktree agreed with each other throughout and both were wrong; a
second regression holds the replay direction; a second row was raised by the round itself
(`PR8-R7-DRIVER-REF-FUNNEL`). No Class A, B or C change was needed and neither deferred finding is
reopened. A merge decision's silence on P2s and P3s is not evidence that none exist, so the
round's second half re-read the record against the code, both census domains re-derived and
every line-number citation checked (`pr8-triage.md` §10.4).

The repaired head's first CI run showed the seventh round's own witness red on the winguest leg,
and the next head showed it again — deterministic, every other job of both runs green. The eighth
round (Claude Fable 5.1 at max effort, a fresh autonomous session, 2026-09-08) is not a review
round. It reproduced the red on the persistent Windows guest before touching anything and
established the layer: `dispatch_head` returns the published head, the worktree is cut at it,
alpha's file is on disk, and its bytes are `\r\n` because Git for Windows' system
`core.autocrlf=true` reaches the recover fixture's repository and `git worktree add` honours it.
It repaired the fixture — its repository now pins `core.autocrlf=false` and `core.eol=lf`, as
`workspace_manager::fixture` already does — and not the assertion, which is untouched. The
finding is re-filed from P1 `correctness` to P2 `portability`, a fixture defect pre-existing on
master and first observed by the seventh round's test; the original P1 is repaired on all three
platforms. `pr8-triage.md` §11.

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
was established empty, and `Unresolved` whenever it returned without that — the direct child's own
reap never being the evidence. (An earlier version of this paragraph said the funnel reports
`Unresolved` only when its kill was not reaped; the review of `79ddbffb` showed that false in both
directions, and the code now matches this sentence.) Its Windows branch executes on the winguest
CI leg and was not run locally. The fourth repair round
changes the `ContainerRuntime` trait's `stop` and `remove` to answer what they established
(`Settled`), which every runtime double follows; makes the run lock's cleanup scope an owned value
entered per step of the loop, per recovery order and per creation's probe stretch, where the v0.1
coordinator and resume hold a borrowed one for their whole command (those two sites are unchanged
in behaviour); and mirrors every successful append into the event list the run carries, in the one
emit funnel, so the live head rule reads what recovery appended.

The fifth repair round touches the v0.1 path in type only **but for the shared macOS scanner**,
the second declared exception. `review_failure` takes a second argument, and `ReviewOutcome` and
`AttemptFailure` each gain one in-memory field recording whether the Runner established that the
process never started; nothing serializes any of it and the legacy ladder reads none of it, so
every legacy path answers exactly what it answered before. The round's other changes are schema-4
only or are reads. The macOS process-group scanner is shared by both engines and its change is a
refusal where it previously answered wrongly: a failed enumeration is now unknown rather than an
empty group, which the reaper's loop already treats as "keep killing", so on the one platform it
affects the released path becomes fail-closed where it was fail-open. **It executes for the first
time on CI's macOS leg**; this box is Linux and the platform-independent half of the rule is what
the test covers.


The seventh repair round adds no v0.1 surface: `run.rs`'s topology loop and `integrate.rs` are
schema-4 only, and no v0.1 entry point reaches either. The eighth changes a test fixture's
repository config and the notes beside it, and no production code. The sixth adds none at all. It changes what `DockerCli` accepts as
evidence that a container is gone — a phrase in a failed command's stderr no longer settles
anything; a settlement is established against a `docker ps` listing that had to succeed — and the
container runtime has no production constructor outside `src/runner/container/**`, with `run` and
`resume` building a `HostRunner`. Its other change writes the failing gate's output and the
reviewer's required changes into the `merge_rejected` payload's existing `detail`, which is
schema-4 only. No trait signature, no serialized vocabulary and no effect site moves.

Two known gaps were deferred by the owner on 2026-09-07, each with a standing finding filed in
`findings/`. `PR8-CRASH-002`: a lock file left by a coordinator
killed inside `git update-ref`; the refusal it causes is resumable and loses nothing, and the
operator's removal of the lock lets the next resume complete the publication; reclaiming it needs
a `Ref.*` residue class in the frozen inventory. `PR8-R2-SPEND-REPLAY`: a paid review that parks
or meets an outage is charged live and lost on replay, so a restart can admit an integration the
previous incarnation's total would have refused; recording it needs a review record on the
unavailable terminal, which the frozen vocabulary lacks.

Rollback is clean: the schema-4 topology is inert unless a plan selects it, so reverting the branch
removes the machinery and restores the released path exactly — including the two behaviours of it
this branch changes, the unresolved host reviewer and the macOS scanner's refusal to read a failed
enumeration as an empty group, both declared above and both reverting with everything else.
Nothing else in the v0.1 path differs, so a revert has no other released behaviour to undo.
No data migration, no on-disk format change outside the run-scoped
`refs/upstroke/runs/<run>/…` namespace this slice owns.

## Review finding ledger

Fifty-nine findings over eight rounds, one row each. The rows through round seven are compressed to their identity columns — the id, severity, reviewed SHA and location, a one-line failure sequence, provenance, category, first bad, the guard's identifiers and the disposition — because the body is at GitHub's limit; each of those rows' full failure sequence and guard text is in `pr8-triage.md` §§1–9 and in this file's own history at `fc1417106c6cc3637036f1b070143b575b548e82`. The two deferred rows and this round's are unabridged.

| ID | Severity | Reviewed SHA / location | Failure sequence | Provenance | Category | First bad / prior ID | Regression or documented guard | Disposition |
|---|---|---|---|---|---|---|---|---|
| PR8-C1 | P1 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/recover.rs:1001 | publish B to P -> restart -> the startup check compares the ref with run_started.base_sha -> the run's own head refused as foreign | introduced_by_feature | crash-consistency | 43d62194 | `a_resume_after_a_completed_publication_accepts_its_own_head` | fixed |
| PR8-C2 | P1 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/recover.rs:1133 | stale merge_prepared -> restart with the ref at a third SHA -> reclaim deletes the Prepared pin -> publication refuses unpinned | introduced_by_feature | crash-consistency | 43d62194 | `a_resume_keeps_a_prepared_transactions_pin_when_publication_refuses` | fixed |
| PR8-C2-SUBSTITUTED | P1 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/integrate.rs:573 | verification started -> a writer moves the pin -> resume deletes it expected-old at the substituted target, proposed_sha never compared | introduced_by_feature | crash-consistency | 43d62194 | `a_resume_refuses_a_substituted_verification_pin_before_settling_it` | fixed |
| PR8-C2-ORPHAN | P1 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/recover.rs:1182 | next sequence 0 -> prepared/3 present -> reclaim deletes it before refuse_unexpected_refs can refuse it | introduced_by_feature | crash-consistency | 43d62194 | `a_resume_refuses_a_prepared_pin_outside_the_sequences_the_log_pinned` | fixed |
| PR8-C3 | P1 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/integrate.rs:763 | verification started -> the Runner errors spawning a gate -> ? escapes -> the transaction stays open and defers untouched | introduced_by_feature | correctness | 2d1b4c72 | `a_gate_spawn_failure_during_integration_verification_defers_inside_max_defers` | fixed |
| PR8-C4 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/recover.rs:1169 | verification started -> gate snapshot added -> crash -> resume prunes the pin and staging -> the snapshot survives every resume | introduced_by_feature | crash-consistency | 43d62194 | `a_resume_reclaims_an_interrupted_verifications_snapshots_after_settling_it` | fixed |
| PR8-C5 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/recover/tests.rs:6157 | the proof appends merge_prepared synced -> rewrites the log to remove it -> asserts the absence of what it deleted; the sync removed still passes | introduced_by_feature | docs-contract | a1759f16 | `unsynced_merge_prepared_two_crash_barrier_before_cas_then_power_loss_keeps_log_and_ref_agreeing` | fixed |
| PR8-C6 | P1 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/run.rs:166 | candidate at rung 0 -> the ladder's last rung passed as the implementer -> a model reviews its own candidate | introduced_by_feature | correctness | 2d1b4c72 | `an_integration_review_is_selected_against_the_candidates_recorded_implementer` | fixed |
| PR8-CONF-F4 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/attempt.rs:617 | gates run -> snapshots removed before the verification terminal -> a removal failure strands a completed judgement | introduced_by_feature | crash-consistency | 2d1b4c72 | `verification_snapshots_are_removed_only_after_the_terminal` | fixed |
| PR8-CONF-F9 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / pr8-body.md:116 | the body cites the census self-test as cherry-pick residue evidence -> no recovery test samples killed picks | introduced_by_feature | docs-contract | 3414dc58 | `synthetic_cherry_pick_residue_unreferenced_objects_and_cherry_pick_head_then_forced_reclaim_converges` | fixed |
| PR8-CRASH-002 | P1 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/integrate.rs:487 | merge_prepared durable -> git update-ref killed after creating integration.lock and before the rename -> resume retries the authorized CAS -> Git refuses on the lock -> every later resume repeats the refusal until an operator removes the file; the same class was met once more on the macOS runner, where a killed git cherry-pick left packed-refs.lock in the common git dir and the resume's next ref write refused on it | pre_existing | crash-consistency | — | `a_ref_lock_left_by_a_killed_compare_and_swap_refuses_resumably_until_removed` pins the resumable refusal and the completion after removal, and the residue sampler removes git's common-dir ref locks after each kill as an operator would (`remove_git_ref_lock_residue`); no Ref site registers a residue class in the frozen inventory, so reclaiming the lock is a Class C change owed an owner decision | deferred |
| PR8-CRASH-005 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/integrate.rs:369 | already_present at the candidate commit -> kill before publication -> from_fold infers fast from the SHAs -> the staging worktree survives recovery | introduced_by_feature | crash-consistency | 43d62194 | `a_resume_completes_an_already_present_publication_at_the_candidate_commit_and_reclaims_its_staging` | fixed |
| PR8-TESTS-002 | P1 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/run.rs:192 | the review-input policy refuses the tree -> Judge invoked with no prior failure -> the proposal is published | introduced_by_feature | correctness | 2d1b4c72 | `an_unjudgeable_proposal_parks_the_candidate_for_a_person` | fixed |
| PR8-TESTS-003 | P1 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/run.rs:677 | an integration review costs 2.50 -> Spend unchanged live and on replay -> the next selection admits work past the ceiling | introduced_by_feature | correctness | 2d1b4c72 | `an_integration_reviews_cost_reaches_the_run_spend` | fixed |
| PR8-TESTS-005 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/scaffold.rs:719 | the scaffold verifier ignores its workspace -> VerifyRequest.proposed replaced by the candidate SHA -> fourteen integration tests pass | introduced_by_feature | correctness | 2d1b4c72 | `stale_candidate_takes_staging_path_and_publishes_pinned_proposal` | fixed |
| PR8-TESTS-006 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/integrate/tests.rs:859 | the swap skipped when expected_head equals proposed_sha -> HEAD and object count still hold -> fourteen integration tests pass | introduced_by_feature | correctness | 9d035939 | `an_already_present_candidate_settles_without_an_empty_commit` | fixed |
| PR8-TESTS-007 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/integrate/tests.rs:952 | the repair's intent and worktree created after merge_rejected -> only the last event kind checked -> fourteen integration tests pass | introduced_by_feature | correctness | 9d035939 | `a_conflicting_candidate_is_rejected_with_an_atomic_repair_before_any_repair_effect` | fixed |
| PR8-TESTS-008 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/topology/fold/tests.rs:6402 | the over-limit test replaced by limit == 0 -> a positive limit admits automatic repairs without bound -> the suite passes | introduced_by_feature | correctness | b31098db | `a_lineage_that_has_consumed_its_allowance_registers_only_a_human_required_repair` | fixed |
| PR8-TESTS-010 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/recover/tests.rs:6332 | the fixture plants no staging worktree -> the production remove_worktree call removed -> the interrupted-cleanup test passes | introduced_by_feature | correctness | 43d62194 | `a_resume_settles_an_interrupted_stale_verification_and_reclaims_its_residue` | fixed |
| PR8-TESTS-013 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/repair.rs:82 | the root entry cloned without a stated reason -> its nested fields cloned again -> an empty_intersection flag duplicates the admission | introduced_by_feature | correctness | 2d1b4c72 | standards section 6, held by review: the builder borrows the root and clones the specification once | fixed |
| PR8-AUDIT-R8 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/topology/fold/check_integration.rs:471 | the body says an over-limit HumanBinding is refused -> the fold accepts it either side of the limit -> a Class B approved on a false description | introduced_by_feature | docs-contract | 3414dc58 | `a_lineage_past_its_repair_limit_registers_only_a_human_required_repair` | fixed |
| PR8-AUDIT-R10 | P2 | 3414dc5861c9a523342ea0792a54f0129cf82f2f / src/engine/topology/repair.rs:190 | the mid floor intersects the root's ladder empty -> the root's tiers, floor and ceiling kept -> R10 promised the raised floor and no ceiling | introduced_by_feature | docs-contract | 2d1b4c72 | `an_empty_tier_intersection_registers_a_human_binding_ladder_with_the_allowed_agents` | fixed |
| PR8-R2-RUNNER-LIVENESS | P1 | 916852c9638383af3964f743d48e14bf859138c3 / src/engine/topology/run.rs:257 | Docker unreachable during a gate -> the error settled as RunnerSpawnFailure -> Deferred, entitlements released, the mounted snapshot removed beside the surviving gate | fix_regression | crash-consistency | f2e0c69f | `a_runner_that_loses_track_of_a_running_gate_refuses_resumably_and_reclaims_nothing`, `a_lost_gate_container_is_reclaimed_by_the_next_resume_before_the_verification_is_settled`, `the_runner_reports_what_it_established_about_the_process_when_it_fails` | fixed |
| PR8-R2-SPEND-REPLAY | P1 | 916852c9638383af3964f743d48e14bf859138c3 / src/engine/topology/select.rs:52 | 1.20 spent -> a 2.50 review returns needs_human -> live total 3.70 -> restart -> replay restores 1.20 -> the selector admits an integration under a 2.20 ceiling the previous incarnation refused | introduced_by_feature | correctness | 2d1b4c72 | `a_paid_review_that_parks_is_charged_live_and_its_replay_loss_is_the_deferred_vocabulary_gap` pins the live charge, the in-incarnation refusal, and the replayed total as exactly what the frozen unavailable terminal can carry; decisions.coordinator_integration.dispositions requires the spend recorded and MergeVerificationUnavailable has no review record, so the field is a Class C change owed an owner decision (or an erratum) | deferred |
| PR8-R2-GIT-STATE | P1 | 916852c9638383af3964f743d48e14bf859138c3 / src/engine/topology/run.rs:210 | the staging index is corrupt -> the review-input reader returns a Git error -> ? propagates it with no terminal -> the outage never reaches the defer accounting | introduced_by_feature | correctness | f2e0c69f | `a_git_error_observed_by_the_verification_settles_an_infrastructure_outage` | fixed |
| PR8-R2-GATE-TIMEOUT | P2 | 916852c9638383af3964f743d48e14bf859138c3 / src/engine/topology/attempt.rs:574 | a gate times out -> timed_out with no verdict -> classified GateFailed -> merge_rejected registers a repair for a timeout | introduced_by_feature | correctness | 2d1b4c72 | `a_gate_that_times_out_during_integration_verification_defers_instead_of_registering_a_repair` | fixed |
| PR8-R2-FROZEN-SPEC | P2 | 916852c9638383af3964f743d48e14bf859138c3 / src/engine/topology/repair.rs:60 | a candidate is rejected with gate evidence -> the frozen spec carries neither the evidence nor either SHA | introduced_by_feature | docs-contract | 2d1b4c72 | `the_frozen_repair_spec_embeds_the_rejection_evidence_and_both_shas` | fixed |
| PR8-R2-TEST-PROVENANCE | P2 | 916852c9638383af3964f743d48e14bf859138c3 / src/engine/topology/run.rs:189 | a Test candidate's test is published first by another -> the pick leaves a helper-only diff -> rejected for adding no test code | fix_regression | correctness | f2e0c69f | `a_test_candidate_whose_test_was_already_published_is_verified_not_rejected_for_provenance` | fixed |
| PR8-R2-SNAPSHOT-ORACLE | P2 | 916852c9638383af3964f743d48e14bf859138c3 / src/engine/topology/integrate/tests.rs:1162 | the production verifier snapshots the wrong commit or removes snapshots early -> the asserting tests use doubles -> 354 tests pass | introduced_by_feature | correctness | 2ff9f14a | `the_production_verifier_judges_the_recorded_proposal_and_removes_its_snapshots_after_the_terminal` | fixed |
| PR8-R2-SAMPLER-ORACLE | P2 | 916852c9638383af3964f743d48e14bf859138c3 / src/engine/topology/recover/tests.rs:8098 | the sampler's child.kill() deleted -> every cherry-pick completes -> eight clean picks satisfy the kill proof | introduced_by_feature | correctness | d14e1db0 | `sampled_cherry_pick_child_kills_every_residue_classified_and_recovered` | fixed |
| PR8-R2-UNREACHABLE | P2 | 916852c9638383af3964f743d48e14bf859138c3 / src/engine/topology/recover/tests.rs:6491 | recovery returns before the armed append -> let _ discards the result -> a bare unreachable! ends the child with no report | introduced_by_feature | docs-contract | 69463499 | `two_crash_kill_child` | fixed |
| PR8-R2-RECORD-PROSE | P3 | 916852c9638383af3964f743d48e14bf859138c3 / docs/internals/engine/topology/integrate.md:323 | the prose relocation keeps three statements false against the code -> a refusal that does not exist, a check skipped, a person refused | introduced_by_feature | docs-contract | 4de4c74e | `integrate_stale`, `ensure_recorded_integration_ref`, `check_merge_rejected`, `test-internals-notes.sh` | fixed |
| PR8-R2-RECORD-PROVENANCE | P2 | 916852c9638383af3964f743d48e14bf859138c3 / pr8-body.md:194 | the body names Opus 4.8 at high effort -> the plan names Fable 5.1 at max -> the body names what did not run | introduced_by_feature | docs-contract | 3414dc58 | `validate-pr-body.sh` | fixed |
| PR8-R3-HOST-GROUP-GONE | P1 | 79ddbffbb5fb087bfb89f88321a3f60a2a88bdb1 / src/agent/proc.rs:442 | a gate forks a same-group descendant -> the reaper is lost -> the direct child's reap sets Gone -> the snapshot disposed beside the live descendant | fix_regression | correctness | cd4610f6 | `a_reaped_leader_does_not_prove_its_group_gone_when_the_reaper_failed`, `a_lost_group_settles_unresolved_even_when_the_leader_reaps_cleanly`, `a_containment_failure_after_the_spawn_leaves_the_fate_unresolved` | fixed |
| PR8-R3-CONTAINER-START | P1 | 79ddbffbb5fb087bfb89f88321a3f60a2a88bdb1 / src/runner/container/exec.rs:631 | Docker down before docker create -> cancelled classifies Unresolved though no process can exist -> the outage consumes no defer | fix_regression | correctness | cd4610f6 | `the_runner_reports_what_it_established_about_the_process_when_it_fails`, `repeated_container_launch_outages_before_start_consume_defers_through_the_production_runner` | fixed |
| PR8-R3-CONTAINER-RETAIN | P1 | 79ddbffbb5fb087bfb89f88321a3f60a2a88bdb1 / src/runner/container.rs:527 | observe, stop and remove unreachable -> cancel_reached prunes the view and intent -> the running container's mounted Git metadata deleted | fix_regression | crash-consistency | cd4610f6 | `a_container_the_runtime_cannot_confirm_stopped_keeps_its_mounted_git_view_and_intent` | fixed |
| PR8-R3-CONTAINER-EVIDENCE | P2 | 79ddbffbb5fb087bfb89f88321a3f60a2a88bdb1 / src/runner/container.rs:518 | a stop fails and the forced removal succeeds -> container_released false -> Unresolved -> an observed outage consumes no defer | fix_regression | correctness | cd4610f6 | `the_runner_reports_what_it_established_about_the_process_when_it_fails` | fixed |
| PR8-R3-RECORD-ENTITLEMENTS | P2 | 79ddbffbb5fb087bfb89f88321a3f60a2a88bdb1 / pr8-triage.md:137 | the triage says cancel releases both entitlements -> the reservation had converted and the open transaction holds them | introduced_by_feature | docs-contract | 1e2a0733 | `an_unresolved_verification_leaves_its_entitlements_with_the_open_transaction` | fixed |
| PR8-R4-LIVE-HEAD | P1 | 8a5f59e8ca2272e253e326685d15136c6008e1d0 / src/engine/topology/integrate.rs:222 | alpha publishes P -> a writer resets the ref to the base -> beta's decide compares only with beta's base -> beta published over the lost publication | introduced_by_feature | correctness | 12448215 | `a_foreign_reset_of_the_integration_ref_refuses_before_any_append_and_keeps_the_merged_task`, `authorized_head` | fixed |
| PR8-R4-REMOVAL-IN-PROGRESS | P1 | 8a5f59e8ca2272e253e326685d15136c6008e1d0 / src/runner/container.rs:538 | observe and stop lost -> docker rm answers removal in progress, normalized Ok -> Gone reported beside the running gate | fix_regression | correctness | 52d1fa05 | `the_runner_reports_what_it_established_about_the_process_when_it_fails`, `a_removal_another_reclaimer_holds_is_not_proof_the_gate_is_gone`, `a_removal_answer_meaning_already_in_progress_is_tolerated_and_a_real_failure_is_not` | fixed |
| PR8-R4-CLEANUP-LEASE | P1 | 8a5f59e8ca2272e253e326685d15136c6008e1d0 / src/engine/topology/run.rs:768 | the topology run enters no cleanup scope -> a reaper spawned with no lease -> the next resume's probe finds no hold while the reaper still reclaims | pre_existing | crash-consistency | 1db49779 on master, the attempt path's omission that this branch routes integration verification through | `a_host_integration_reaper_holds_the_runs_cleanup_lease` | fixed |
| PR8-R4-SUBSTITUTED-PIN-LIVE | P1 | 8a5f59e8ca2272e253e326685d15136c6008e1d0 / src/engine/topology/integrate.rs:869 | verification pins P -> a writer moves the pin to X -> reclaim_staging deletes expected-old at X -> the substituted ref gone, success returned | introduced_by_feature | correctness | 2d1b4c72 | `a_rejected_or_unavailable_terminal_refuses_to_delete_a_pin_another_writer_substituted` | fixed |
| PR8-R4-START-NOT-ATTEMPTED | P2 | 8a5f59e8ca2272e253e326685d15136c6008e1d0 / src/runner/container/exec.rs:618 | the funnel refuses before docker start -> the launch records Started -> Unresolved instead of NeverStarted -> the transaction stays open | fix_regression | correctness | 52d1fa05 | `the_runner_reports_what_it_established_about_the_process_when_it_fails`, `docker start` | fixed |
| PR8-R4-REVIEW-ORACLE | P2 | 8a5f59e8ca2272e253e326685d15136c6008e1d0 / src/engine/topology/recover/tests.rs:7315 | both review doubles ignore their workspace -> reviewers pointed into staging -> 366 tests pass with verification_isolation violated | introduced_by_feature | correctness | 2d1b4c72 | `stale_candidate_takes_staging_path_and_publishes_pinned_proposal`, `terminal_shape_coverage_table_drives_every_shape_and_each_converges_on_replay`, `the_production_verifier_judges_the_recorded_proposal_and_removes_its_snapshots_after_the_terminal` | fixed |
| PR8-R5-MACOS-ENUMERATION | P1 | 716cf89af6817b7797ca954944c83dd37ea7a7f3 / src/agent/proc.rs:3811 | proc_listpids fails and returns zero -> the scanner reads an empty group -> the reaper exits beside a survivor and the cleanup lease is released | pre_existing | correctness | the macOS scanner as first written; the Linux scanner has no such ambiguity | `a_pid_enumeration_that_failed_is_not_an_empty_process_group`, `listed_pid_bytes` | fixed |
| PR8-R5-DISCARDED-REVIEW-COST | P1 | 716cf89af6817b7797ca954944c83dd37ea7a7f3 / src/engine/topology/attempt.rs:624 | a 2.50 review completes -> the next snapshot fails with a Git error -> ? discards the completed reviews -> 6.30 accounted against 8.80 spent | introduced_by_feature | correctness | 2d1b4c72 | `a_completed_integration_review_is_charged_when_the_next_reviewers_snapshot_fails`, `ReviewAccount`, `verify` | fixed |
| PR8-R5-CLASSIFIER-INDEX-WRITE | P2 | 716cf89af6817b7797ca954944c83dd37ea7a7f3 / src/workspace_manager.rs:2609 | proposal_state classifies an empty pick -> porcelain git diff refreshes the index -> a hook-less read writes index.lock and the index | introduced_by_feature | correctness | 2d1b4c72 | `the_proposal_classifier_writes_no_index_while_reading_an_empty_pick`, `diff-files`, `diff.autoRefreshIndex` | fixed |
| PR8-R5-READ-ONLY-SWEEP | P2 | 716cf89af6817b7797ca954944c83dd37ea7a7f3 / src/workspace_manager.rs:3416 | the residue classifier runs git status --porcelain -> git writes back the stat cache -> its own index.lock is later read as proof of an unpublished add | pre_existing | correctness | PR5-CONF-002, which named this mechanism and applied the flag at one call site | `a_worktree_inspecting_read_writes_no_index`, `--no-optional-locks`, `read_only_git` | fixed |
| PR8-R5-REVIEWER-SPAWN-FAILURE | P2 | 716cf89af6817b7797ca954944c83dd37ea7a7f3 / src/review.rs:547 | a reviewer's image mismatches -> NeverStarted contained as Unavailable{AgentError} -> persisted as ReviewUnavailable where INV-23 names RunnerSpawnFailure | introduced_by_feature | docs-contract | 2d1b4c72 | `a_reviewer_whose_process_never_started_is_a_runner_spawn_failure`, `review_infrastructure_failures_become_unavailable_outcomes` | fixed |
| PR8-R5-CENSUS-DOMAIN | P2 | 716cf89af6817b7797ca954944c83dd37ea7a7f3 / pr8-triage.md:298 | the census greps the schema-4 primitives -> the legacy Workspace's direct git sites cannot appear -> the section claims every site and miscounts | introduced_by_feature | docs-contract | 1e2a0733 | the domain restated as every ref deletion, ref move, worktree removal and snapshot removal in both layers, with the grep that derives it named | fixed |
| PR8-R5-V01-CLAIM | P3 | 716cf89af6817b7797ca954944c83dd37ea7a7f3 / pr8-body.md:37 | Risk declares a legacy behaviour change -> Summary and rollback say the v0.1 path is unchanged -> the body contradicts itself | introduced_by_feature | docs-contract | cd4610f6 | `validate-pr-body.sh` | fixed |
| PR8-R6-DOCKER-TEXT-SETTLEMENT | P1 | 9ee9784ed25a58c8139f1a7dcffc03a32d5a53b0 / src/runner/container.rs:1166 | TLS material vanishes under a path named for a searched phrase -> docker fails before the daemon and quotes it -> Gone settled beside the running container | pre_existing | correctness | the phrase tables as first written; PR8-R4-CONTAINER-SETTLED made the answers typed and left them derived from arbitrary stderr | `a_diagnostic_that_is_not_the_daemon_answering_about_this_container_settles_nothing`, `a_listing_answers_for_exactly_the_container_it_was_asked_about`, `real_docker_fails_locally_without_ever_saying_a_container_is_gone`, `real_docker_lists_the_state_the_settlement_observation_reads` | fixed |
| PR8-R6-UNREACHABLE-SWEEP | P1 | 9ee9784ed25a58c8139f1a7dcffc03a32d5a53b0 / src/runner/container.rs:1151 | a failure quotes an engine-supplied label containing a connection phrase -> read as unreachable -> the write command proceeds with no container evidence | pre_existing | correctness | the same phrase-over-arbitrary-text mechanism as PR8-R6-DOCKER-TEXT-SETTLEMENT, read in the direction the census depends on | `the_two_docker_diagnostic_tables_never_claim_one_message` | fixed |
| PR8-R6-REPAIR-EVIDENCE | P2 | 9ee9784ed25a58c8139f1a7dcffc03a32d5a53b0 / src/engine/topology/integrate.rs:896 | a gate fails -> the tail lands in AttemptFailure::feedback -> code_record passes only reason -> the frozen spec says gate failed: exit 1 and nothing more | introduced_by_feature | correctness | 2d1b4c72 | `a_failing_gates_own_output_reaches_the_frozen_repair_spec`, `a_rejecting_reviewers_required_change_reaches_the_frozen_repair_spec`, `the_frozen_repair_spec_embeds_the_rejection_evidence_and_both_shas`, `gate_failure`, `review_failure`, `code_record` | fixed |
| PR8-R6-V01-CLAIM-2 | P3 | 9ee9784ed25a58c8139f1a7dcffc03a32d5a53b0 / pr8-body.md:38 | the macOS scanner change reaches legacy workers -> Risk says so -> Summary and rollback still promise one exception | introduced_by_feature | docs-contract | PR8-R5-V01-CLAIM, whose repair qualified the first exception and not the second | `validate-pr-body.sh` | fixed |
| PR8-R6-CENSUS-DOMAIN-2 | P3 | 9ee9784ed25a58c8139f1a7dcffc03a32d5a53b0 / pr8-triage.md:343 | the derivation greps update-ref and six primitives -> git moves refs by other names -> Workspace::commit and four sites absent from the census | introduced_by_feature | docs-contract | PR8-R5-CENSUS-DOMAIN, which restated the domain and left the derivation narrower than the claim | `Workspace::commit` | fixed |
| PR8-R7-DISPATCH-BASE | P1 | eb4e2997f2970e81da2350bbd016b6170caddcc3 / src/engine/topology/run.rs:1515 | beta depends on alpha -> alpha's publication moves the head to P -> dispatch_request builds beta's DispatchRequest with run_started.base_sha -> beta's worktree and its durable base_sha are both B, without alpha's file, and a retry selects B again | pre_existing | correctness | 199dc1dc on master, the ready-dispatch branch this slice is the first to publish behind | `a_dependent_task_is_dispatched_into_its_dependencys_merged_work`, `a_dispatch_recorded_before_this_rule_resumes_at_the_base_it_recorded`, `a_dispatch_takes_the_published_head_and_refuses_one_the_log_did_not_authorize` | fixed |
| PR8-R7-DRIVER-REF-FUNNEL | P2 | eb4e2997f2970e81da2350bbd016b6170caddcc3 / src/engine/topology/recover/tests.rs:1081 | thirteen tests resume through the RecordingRefs double -> the repository never receives the integration ref the faked recovery recorded -> the loop steps against a state P8 and every resume make impossible | pre_existing | correctness | 0aebd310 on master, the resume's ref funnel double that this slice is the first to step the loop behind | `resume_with_real_refs` | fixed |
| PR247-DISPATCH-HEAD-WITNESS-RED-ON-WINDOWS | P2 | 91fe35b008a0a612bb37ed43a9eb3b5b7487c8a3 / src/engine/topology/recover/tests.rs:9925 | Git for Windows' system config carries core.autocrlf=true -> the recover fixture's repository inherits it, pinning identity and reflog and nothing about line endings -> git worktree add renders alpha's LF blob as CRLF when it populates beta's checkout -> the contents assertion compares bytes and fails on winguest alone, twice and deterministically, while the dispatch it witnesses is correct on every platform (worktree at the published head, durable base_sha naming it, the file on disk) | pre_existing | portability | bcc3a533 on master, the fixture's git init without a line-ending pin; red since 45b5d429, the first test to compare checkout bytes through it | the fixture pins `core.autocrlf` and `core.eol` in its repository config, the layer `git worktree add` reads, so `a_dependent_task_is_dispatched_into_its_dependencys_merged_work` observes the bytes it committed on every platform with its contents assertion untouched; the tree without the pin is the reproduction on the Windows guest, and the tree with it is green there — the test alone and the full suite | fixed |
