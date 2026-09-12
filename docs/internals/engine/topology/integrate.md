# `src/engine/topology/integrate.rs`

Extended notes for [`src/engine/topology/integrate.rs`](../../../../src/engine/topology/integrate.rs).

The code is the authority for what it does; this file is the whole of its prose, moved out of
the source verbatim. Each section is headed by the line of code the comment sat above, spelled
as it is in the source, so the heading is the grep string that finds the code.

## Module

The integration transaction of one queued candidate.

`decisions.coordinator_integration.integration_sequence`: the loop selects
the first eligible candidate, checks the ceiling, takes the provisional
`{pipeline, merge}` reservation, and then — **before any staging effect** —
asserts the integration ref publishable and reads its head. That read is
the exact-base decision: a head equal to the base `candidate_prepared`
recorded publishes the immutable candidate commit itself; any other head
takes the staging path. Everything after the decision is a terminal of the
transaction it opened, and every authorized publication is completed
through one function, [`publish`], on the live path and on recovery alike.

The module performs no append of its own: every event goes through the
caller's [`IntegrationJournal`], which is the run's emitter and its fold,
so a live run and a replay reach the same state by the same checks. What
it owns is the *order* — reservation, then the head read, then the fast
`merge_prepared`, then the compare-and-swap, then `task_merged` — and the
refusals that order rests on.

## `pub fn prepared_pin_ref(run_id: &str, sequence: SequenceId) -> GitRef {`

`refs/upstroke/runs/<run>/prepared/<sequence>`: the pin that keeps a stale
candidate's proposal commit reachable while its verification runs (R12).

## `pub fn staging_slot(sequence: SequenceId) -> Slot {`

The staging worktree of a stale transaction, `merge/s<sequence>` (R10).

## `pub trait IntegrationJournal {`

What the integration sequence appends through, reads its state from, and
performs its effects with.

One trait rather than three parameters because the three are one object
on the live path: the run's emitter owns the fold it checks against, the
append handle, the hook bundle, and the reservation ledger the first
append converts. The sequence calls them in the order the packet fixes and
never holds two of them across a call.

## `pub trait IntegrationJournal {` › `fn emit(&mut self, body: TopologyEventBody) -> Result<(), UpstrokeError>;`

Append `body` through the run's emitter: checked against the fold,
written and synced, then applied.

### Errors

The fold's refusal, or the append-error protocol's report.

## `pub trait IntegrationJournal {` › `fn fold(&self) -> &TopologyFold;`

The fold every append is checked against, read to derive a repair and a
candidate's region.

## `pub trait IntegrationJournal {` › `fn hooks(&mut self) -> &mut dyn TopologyHooks;`

The hook bundle the funnels take.

## `pub trait IntegrationJournal {` › `fn converted(&mut self, key: TaskKey) -> Result<(), UpstrokeError>;`

The provisional integration reservation of `key` converted to a
fold-derived holding: called exactly once, right after the first
append of the sequence.

### Errors

The reservation ledger's refusal when no such reservation is held.

## `pub trait Verification {`

What runs an integration verification and mints its park question.

Implemented by the same object as [`IntegrationJournal`], because both are
the run: the gates and reviewers execute through the run's own
[`super::attempt::Judge`] over its ledgers, and a park question is minted
from the run's [`IdSource`].

## `pub trait Verification {` › `fn verify(&mut self, request: &VerifyRequest<'_>) -> Result<Verified, UpstrokeError>;`

Run every recorded gate on one fresh exact snapshot of the proposed
commit and every review pass on its own, reviewing it against the head,
and say what they decided. `staging` is read only for the review diff
and the review-input policy; no gate or reviewer runs in it.

### Errors

A plan the run cannot assemble, a refusal of the run's own records, or a
Runner error whose process fate is `Unresolved` — a gate or reviewer
process that may still be running, which no terminal may be settled over.
An observed infrastructure failure is not an error but
[`Verified::Unavailable`], with a terminal of its own.

## `pub trait Verification {` › `fn ids(&self) -> &dyn IdSource;`

The id source a park question's identity comes from.

## `pub enum Verified {`

What a verification came back with.

## `pub enum Verified {` › `Judged(Judgement),`

The gates and reviewers ran, or a prior failure stood in for them.

## `pub enum Verified {` › `Unavailable {`

An observed infrastructure failure the sequence terminates
`merge_verification_unavailable{Infrastructure{kind}}`, deferred inside
the frozen allowance and parked at it
(`transaction_fault_matrix[T-VERIFY].resume_action`). The kinds this
build settles: `RunnerSpawnFailure` for a Runner that established no
process of a gate was started (`invariants[INV-23]`), and `Other` for a
gate process the Runner lost after it started and has since established
gone, and for foreign Git state the verification observed
(`decisions.repairs.not_repairs`). `detail` is what the infrastructure
reported, carried into the park question.

## `pub enum Verified {` › `reviews: Vec<crate::events::ReviewRecord>,`

Whatever the verification charged before it failed. This is here because
the failure arm has no `Judgement` to take records from and the money was
still spent: the reviews run in pass order, each one's cost is charged to
the run as it returns, and a later pass's snapshot or ledger step can then
fail and take the whole judgement with it. `IntegrationCx::verify` keeps
what its account was charged and hands it out through this field, so the
terminal records the same passes the live total already counted and a
replay reaches that total again.

Empty for the two Runner arms by construction rather than by choice: a
`JudgeError::Runner` can only come from a gate verdict, and gates run
before any reviewer, so no pass has been charged when one is raised.

## `pub struct VerifyRequest<'a> {`

One integration verification to run.

## `pub struct VerifyRequest<'a> {` › `pub already_present: bool,`

The proposal equals the head: gates rerun on the head and the review
judges the head tree against the candidate's original patch.

## `pub enum Refusal {`

Why the sequence refused, each naming the record and the value it
disagreed with.

## `pub struct IntegrationRequest {`

What the fold recorded about the candidate the loop selected, read once
before any effect.

## `pub struct IntegrationRequest {` › `pub sequence: SequenceId,`

The sequence this transaction opens under: the fold's next dense one.

## `pub struct IntegrationRequest {` › `pub base_sha: CommitSha,`

The base `candidate_prepared` recorded — the head the exact-base
decision compares against.

## `pub struct IntegrationRequest {` › `pub authorized: AuthorizedHead,`

The head the log authorizes the integration ref to be at, read from the
proven prefix before any effect ([`authorized_head`]). `decide` requires
the ref there before it decides anything.

## `pub struct IntegrationRequest {` › `pub satisfies: Vec<TaskKey>,`

The closure this publication settles, as the fold derives it.

## `pub struct IntegrationRequest {` › `pub lease_release: MergeLeaseRelease,`

The lease `task_merged` releases: the candidate's own, or its lineage's.

## `pub struct IntegrationRequest {` › `pub integration_ref: GitRef,`

The ref this run publishes onto, as `run_started` recorded it.

## `impl IntegrationRequest {` › `pub fn from_log(`

Read the request from the fold that selected `candidate` and from the
events the fold was derived from — the fold retains nothing about past
publications, and the authorized head is the last one.

### Errors

[`Refusal::NoCandidateRecord`] when the fold holds no
`candidate_prepared` for the candidate, or a refusal when the run has
not started.

## `pub struct AuthorizedHead {`

Where the log says the integration ref is: the latest publication's
`merged_sha`, or the recorded base before any publication, and which
sequence put it there.

## `pub fn authorized_head(started: &RunStarted4, events: &[TopologyEvent]) -> AuthorizedHead {`

**The one head rule, with three callers.** The resume's startup check
([`super::recover::ensure_recorded_integration_ref`]) learned in the first
repair round to compare the ref with the log's latest `task_merged` rather
than with `run_started.base_sha` (`PR8-C1`); the live [`decide`] never
learned the same lesson and compared the head it read with the candidate's
base only, so an external writer resetting the ref from a publication back
to the base made the next candidate exact-base again, and the engine
published it fast — recording work as merged that no longer existed. The
cover review of `8a5f59e8` reproduced it on a real repository
(`PR8-R4-LIVE-HEAD`). This function is the rule both now read;
`transaction_fault_matrix[T-RESUME].durable_state` counts "CAS completions"
among what a resume continues from, and a live sequence continues from the
same record.

The live engine reads it off the event list `RunHandle` carries, which the
one `emit` funnel keeps current for every writer
([`super::emit::EmitState`]), so a publication recovery completed a moment
ago and one the loop performed itself are read the same way.

The third caller is [`dispatch_head`], added in the seventh repair round.
Nothing about the rule changed for it: a dispatch asks the same question
the two integration callers ask, and asking it a third way is what the
fourth round's repair was for.


## `fn prepared_base(`

The base the candidate's `candidate_prepared` recorded.

## `fn lease_release(fold: &TopologyFold, candidate: &CandidateRef) -> MergeLeaseRelease {`

The lease a publication of `candidate` releases.

`decisions.admission_and_leases.leases`: "task_merged releases the
Candidate lease or, when satisfies contains root, the lineage lease". A
lineage member's closure always reaches its root, so the release is the
lineage's exactly when the candidate's task descends from one.

## `pub enum ExactBase {`

The exact-base decision.

## `pub enum ExactBase {` › `Fast,`

The head is the candidate's base: publish the candidate commit itself.

## `pub enum ExactBase {` › `Stale,`

The head moved: cherry-pick and re-verify.

## `pub struct Decided {`

The head the integration ref was read at, and what it decided.

## `pub fn decide(`

The exact-base decision: `assert_publishable`, then the head read, then
the head required to be the one the log authorizes, and only then
fast-or-stale.

Read-only, and every staging effect of the sequence comes after it — that
is INV-09's "the exact-base decision is made from the integration ref head
before any staging effect", and the reason this is a separate function
from the paths it selects between.

**A head the log did not put there refuses here.**
`decisions.coordinator_integration.integration_sequence`: "assert_publishable
and read the integration ref head H (this is expected_head; a symbolic,
checked-out, or **foreign** ref refuses here)". The exact-base comparison
alone cannot tell a foreign reset from a fresh run: a ref an external
writer moved back to the base is exactly the candidate's base, and the
first version chose `Fast` and published over the lost publication. The
comparison with [`AuthorizedHead`] comes first, so a foreign head decides
nothing, stages nothing and appends nothing; the ref is neither moved nor
recreated, and the refusal names what was found, what the log authorizes
and the sequence that put it there.

### Errors

A symbolic or checked-out integration ref (`assert_publishable`),
[`Refusal::IntegrationRefAbsent`], [`Refusal::ForeignHead`], or a Git error
reading the ref.


## `pub fn dispatch_head(`

The head a freshly dispatched task's worktree is created at.

`design/26_design_merge_queue_protocol.md` verdict 1 gives a dispatched
task "a detached linked worktree at the run's **integration HEAD at
dispatch**", and which head that is, is the log's to say. This is
[`authorized_head`]'s third caller, not a third rule.

**Why the line it replaced was right until this slice.** The run loop
built every `DispatchRequest` with `run_started.base_sha`. Before
publication existed, that *was* the run's integration head for the whole
run, at every dispatch, and the two readings agreed everywhere. This slice
is what makes a publication move the ref, and so what turns the agreement
into a disagreement: with beta depending on alpha, alpha's publication
moves the head, and beta's first dispatch — an ordinary dependency chain,
no concurrent writer, hostile filename, crash or injected failure in it —
put beta's worktree at a base without alpha's merged file in it. The
review of `eb4e2997` reproduced it against unchanged production sources
(`PR8-R7-DISPATCH-BASE`). Nothing was erased and no publication was
falsely recorded; dependent work simply ran without its prerequisite, and
because a fresh generation selected the same starting base again, a retry
did not correct it.

**Why the ref is confirmed and not just read from the log.** A dispatch
spends an agent. Confirming the ref is where the log says, before the
caller appends anything, means a run whose integration ref has moved under
it refuses rather than paying for work on a foreign head — the posture
[`decide`] and [`super::recover::ensure_recorded_integration_ref`] already
take, down to `assert_publishable` preceding the read. The ref is neither
moved nor recreated here.

**What a replay sees.** Nothing: the base a task was dispatched at is
`task_dispatched.base_sha`, which is durable, and the fold reads it from
the log. `TopologyRun::continue_open` resumes an open generation at the
base its own record names, and `verify_or_recreate` checks quiescence
against that same base, so a resume derives the base it dispatched at and
a log written before this change replays to exactly the decisions it
replayed to before.

### Errors

[`UpstrokeError::Refused`] when the ref names nothing
([`Refusal::DispatchHeadAbsent`]) or names a head the log did not
authorize ([`Refusal::DispatchHeadForeign`]), and for a symbolic or
checked-out ref (`assert_publishable`); a Git error reading it.


## `pub struct Authorized {`

A publication the log has authorized and the ref move still owes.

Built on the live path by the `merge_prepared` that authorized it, and on
recovery from the fold's `Prepared` transaction. Either way [`publish`]
completes it: INV-09's "an authorized publication is always completed
(recovery or run-end closure), never abandoned".

## `pub struct Authorized {` › `pub pin: Option<GitRef>,`

The `prepared/<seq>` pin of a stale publication, pruned after
`task_merged`; `None` for a fast one, which pins nothing.

## `pub struct Authorized {` › `pub staging: Option<Slot>,`

The staging worktree of a stale publication, removed with force after
`task_merged`; `None` for a fast one, which stages nothing.

## `impl Authorized {` › `pub fn from_fold(fold: &TopologyFold) -> Result<Option<Self>, UpstrokeError> {`

The publication the fold's unresolved transaction authorizes, if the
transaction has reached `merge_prepared`.

`None` when there is no transaction or it is still verifying; the
latter is `T-VERIFY`'s row, settled elsewhere.

### Errors

A refusal when the run has not started.

## `pub fn from_fold(fold: &TopologyFold) -> Result<Option<Self>, UpstrokeError> {` › `let staged = *disposition != PreparedDisposition::Fast;`

A fast publication staged nothing; a stale-clean or already-present
one ran in `merge/s<seq>` and the fold retains which it was, because
the SHAs alone cannot say: an already-present publication at the
candidate's own commit has `proposed_sha == candidate.commit_sha`
and a staging worktree all the same (`pr8-triage.md`, crash 5).
The pin is the one the record names — `None` for fast and
already-present, which pin nothing.

## `pub struct Published {`

A completed publication.

## `pub fn prepare_fast(`

The fast path: `merge_prepared(fast)` for a candidate whose base is the
head that was just read.

The record names the candidate's own `candidate_prepared` as its
verification source, proposes the candidate commit and pins nothing, and
the fold refuses every other shape (`check_merge_prepared`). The provisional
reservation converts at this append, its first.

### Errors

The fold's refusal or the append-error protocol's report; the reservation
ledger's refusal after the append.

## `pub fn publish(`

Complete an authorized publication: the compare-and-swap, then
`task_merged`, then the pruning the terminal permits.

`decisions.coordinator_integration.publish` and `cas_recovery`, as one
function for both: `assert_publishable`, read the ref, and then

* at `expected_head`: `update-ref --no-deref <ref> <proposed> <expected>`
  through `Ref.CompareAndSwapIntegration` — for an already-present
  publication the two are equal and Git validates the expected old without
  moving anything;
* at `proposed_sha`: the move already happened and only the record is
  owed;
* at anything else: refuse. A third SHA is foreign history.

`task_merged` is appended only after the ref is at `proposed_sha`, which
is INV-09's "CAS before `task_merged`". The pin and the staging worktree
of a stale publication go afterwards, because both keep the proposal
reachable until the integration ref does.

### Errors

A symbolic or checked-out ref, [`Refusal::IntegrationRefAbsent`],
[`Refusal::ThirdSha`], a Git error from the swap, the fold's refusal of
`task_merged`, the append-error protocol's report, or
[`Refusal::PinAtAnotherSha`].

## `pub fn prune_pin(`

Delete a prepared pin expected-old at the proposal it recorded.

Absent is fine — a kill between two resumes may have pruned it already —
and a pin at another object refuses rather than deleting the evidence.

### Errors

[`Refusal::PinAtAnotherSha`] or a Git error.

## `pub fn integrate<J: IntegrationJournal + Verification>(`

One integration, from the decision to its terminal.

The reservation is the caller's: taken before this is entered and
cancelled by the caller if this returns before the first append converted
it.

### Errors

Any refusal of the sequence it runs. A candidate whose base is no longer
the head is not refused: it takes the stale path (`integrate_stale`),
cherry-picked onto the head in a staging worktree and verified there. (An
earlier build refused it as `StaleNotImplemented`; that variant no longer
exists, and the sentence that said so survived the prose relocation until
the reviews of `916852c9`.)

## `pub enum Terminal {`

The terminal one integration reached.

## `pub enum Terminal {` › `Merged(Published),`

A publication: `merge_prepared`, the compare-and-swap, `task_merged`.

## `pub enum Terminal {` › `Rejected { sequence: SequenceId, key: TaskKey },`

A conflict or a code-attributed rejection, with the repair registered.

## `pub enum Terminal {` › `Unavailable {`

The verification could not be run: deferred, or parked.

## `enum Picked {`

What a cherry-pick left in the staging worktree.

## `fn integrate_stale<J: IntegrationJournal + Verification>(`

The stale path: cherry-pick the immutable candidate onto the head in a
staging worktree, classify what the pick left, and reach the terminal the
classification and (for a clean or empty pick) the verification decide.

## `fn start_and_verify<J: IntegrationJournal + Verification>(`

Append `merge_verification_started`, run the verification, and reach the
terminal the judgement decides. A gate that timed out is asked about
before the judgement's failure is read: `decisions.repairs.not_repairs`
lists timeout among the outcomes that terminate unavailable at
integration, so it settles `Infrastructure{Other}` and registers no
repair, where the ordinary gate-failure branch would have.

## `fn start_and_verify<J: IntegrationJournal + Verification>(` › `reclaim_snapshots(journal, manager)?;`

`merge_prepared` is the verification's terminal: the snapshots
go now, before the ref moves, and never before the append.

## `fn prepare_verified(`

Authorize a verified publication, checked by the fold's `merge_prepared`
relations for stale_clean and already_present.

## `fn unavailable<J: IntegrationJournal + Verification>(`

A verification outage: `merge_verification_unavailable`, deferred while the
candidate is inside its frozen allowance and parked at it, then the
snapshots, the staging worktree and the pin reclaimed — the pin at the
proposal the record names ([`reclaim_staging`]). `detail` is what

the infrastructure reported, carried into the park question's context so
a person sees why the outage exhausted its deferrals.

`reviews` is the spend the terminal carries. Every one of the four call
sites supplies it: the judged sites from `judgement.reviews`, which is
every pass that returned before the verdict or the outage that ended the
verification, and the unjudged one from what
[`Verified::Unavailable`] carried out. DESIGN §26 asks all four terminal
shapes to carry their usage and cost; before this field the unavailable
one could not, and a restart replayed a total without the passes a park or
a deferral had already paid for (`PR8-R2-SPEND-REPLAY`). Gate verdicts are
not carried, because a gate is a local process with no reported cost and
the failure this closes is a spend one.

## `fn reclaim_snapshots(`

Remove every verification snapshot with force, each with its intent.

`side_effect_vs_event_ordering`: "staging and snapshot removal (forced)
after terminal (incl. Deferred/Parked)". The judge leaves its snapshots
in place ([`super::attempt::SnapshotDisposal::AfterTheTerminal`]) and
this runs once the terminal is durable, so a removal that fails can no
longer strand a completed judgement behind an unterminated verification.
Every snapshot intent is reclaimed rather than an exact list, because a
judgement that returned an error after adding a snapshot has no list to
hand back, and this sequential coordinator runs one judgement at a time.

## `fn reclaim_staging(`

After a rejection or an unavailable terminal: the snapshots, then the
stale transaction's staging worktree with force and its intent, then its
pin pruned through [`prune_pin`] at the proposal
`merge_verification_started` recorded.

**The expected-old value is the record's, never the ref's.** The first
version read the pin's current target and deleted expected-old at
whatever it found, so a pin another writer had moved was deleted at the
substituted object and the terminal returned success — the cover review
of `8a5f59e8` reproduced it on both the rejected and the parked branch
(`PR8-R4-SUBSTITUTED-PIN-LIVE`). `decisions.workspace_candidates.cleanup`
says cleanup "never establishes authority": the proposal the verification
recorded is the only thing that says what this pin may name, so the
deletion is issued at that SHA and refuses at any other, exactly as the
resume's prune of a resolved sequence's pin does. The refusal comes after
the terminal is durable and after the snapshots and the staging worktree
are reclaimed — those are the sequence's own residue — so what a
substitution leaves behind is the terminal, the substituted ref untouched,
and a command that ends with the refusal naming it.


## `fn infrastructure(failure: &AttemptFailure) -> UnavailableCause {`

What the durable terminal says an infrastructure outage was.

**A process that never started is a `RunnerSpawnFailure`, whatever the failure
it produced is called.** `invariants[22]` (INV-23) requires it in those words —
a container whose reported image id differs from the record "refuses during
pre-flight or rebuild and is a RunnerSpawnFailure outage settlement mid-run" —
and says the rule covers "every probe, worker, gate, review, and re-ask process
of the run". A reviewer's container refused before `docker start` reaches here
as a `ReviewUnavailable` because that is what an unavailable review pass is
called, and the generic mapping cannot override a named invariant.
`IntegrationCx::verify` already answers `RunnerSpawnFailure` for the gate and
worker processes, whose runner errors reach it as a `JudgeError::Runner`; a
reviewer's do not, because `run_review` contains them so the pass can defer
instead of ending the command. Deferral and containment are right; only the
attribution was not.

## `fn rejection_detail(failure: &AttemptFailure) -> String {`

What the frozen repair is told about the rejection.

The reason is a summary — ``gate `clippy` failed: exit 1``, `review failed: …` —
and the evidence a repair works from is in the feedback beside it: the gate's
own output, which `classify::gate_failure` puts there from `log_tail`, already
bounded to `crate::gates::FEEDBACK_TAIL_BYTES`; and the reviewer's
`required_changes`, which `attempt::review_failure` renders one per `- ` line.

Only the reason used to reach the record, so the frozen spec carried the summary
and nothing that says what to change. PR9 dispatches from that spec and never
sees this `AttemptFailure`, so evidence dropped here is evidence the repair
never gets (`pr8-triage.md` §9, finding 2).

The reason stays whole and leads, and the bound is applied to the feedback
alone, because a bound applied to the pair would cut the summary off first.
