# `src/engine/topology/recover/tests.rs`

Repository source for these notes: [`src/engine/topology/recover/tests.rs`](../../../../../src/engine/topology/recover/tests.rs).
[Source on GitHub](https://github.com/sourcemaps/upstroke/blob/master/src/engine/topology/recover/tests.rs).
The relative link works in a checkout or on GitHub; the GitHub link also works from the published site.

The code is the authority for what it does. The explanatory prose is preserved below.
Each backticked part of a section heading is an exact source excerpt. Search for the final
excerpt within the preceding item when a heading names both an item and a line inside it.

## Module

The recovery order, exercised against real directories, a real event log,
real locks and the fake container runtime.

### No raw effect primitive appears here

`src/engine/topology/**` is a `TOPOLOGY_MODULE`: it may carry no
module-level `allow` of a governed lint, and `std::fs`'s writing half is on
the clippy denylist **in tests too**. So every byte this file puts on disk
goes through the funnel that owns its site — `rundir::create_public_dir`
for a directory, `rundir::stage_/publish_owner_record` and its commit-record
pair for the two private records, and `EventLog` for the log. That is not a
ceremony: a fixture that planted `owner.json` with `fs::write` would be
asserting against a file the production writer never produced.

`rundir::remove_public_husk` is what takes a fixture down. It removes a
directory's children and then the directory, which is exactly a recursive
delete through a site-taking funnel, and it is the only such funnel this
module can reach.

## `const RUN_ID: &str = "01KZTPR7E00000000000000001";`

---------------------------------------------------------------------------
Fixed identities
---------------------------------------------------------------------------

## `const CREATOR_PID: u32 = 4242;`

The pid the creator wrote into its `.creating` marker. Never consulted by
the ownership proof — the marker's pid is not one of the twelve conjuncts —
but a marker is not a marker without one.

## `struct Frozen;`

A clock that does not move, so a durable byte can be asserted against a
literal.

## `fn fixture_root(tag: &str) -> PathBuf {`

---------------------------------------------------------------------------
The fixture
---------------------------------------------------------------------------

## `fn fixture_root(tag: &str) -> PathBuf {`

A unique directory per fixture, in one per-process tree.

## `struct Fixture {`

One repository, one committed schema-4 run, and both private records.

Every knob is a field rather than a constructor argument, because the
refusal tests differ from the healthy case in exactly one of them and a
nine-argument builder call would hide which.

## `struct Fixture` › `base_sha: CommitSha,`

The seed commit every fixture's repository holds — the base a step (g)
worktree is cut at.

## `struct Fixture` › `first_line: Vec<u8>,`

The committed first line, without its newline.

## `struct Damage {`

What a fixture may be built wrong in.

## `struct Damage` › `no_private_half: bool,`

Write no private half at all.

## `struct Damage` › `no_owner_record: bool,`

Write no `owner.json`.

## `struct Damage` › `owner: Option<fn(&mut OwnerRecord)>,`

Rewrite one field of the owner record.

## `struct Damage` › `commit: Option<fn(&mut CommitRecord)>,`

Rewrite one field of the commit record.

## `struct Damage` › `locator: Option<String>,`

Record a private locator of another shape.

## `struct Damage` › `host_runner: bool,`

Record a host runner rather than the container one.

## `struct Damage` › `extra: Vec<TopologyEventBody>,`

Extra events, appended after `run_started` in order.

## `struct Damage` › `open_generation: bool,`

Leave one generation **open with no attempt** — the state a crash
between `task_dispatched` and `attempt_started` leaves, and the only
state recovery step (g) has anything to do in.

## `struct Damage` › `two_tasks: bool,`

Register a **second task**, `beta`, beside `alpha`.

The default plan has one task, so a recovery step that loops over tasks
or generations cannot be told apart from one that handles the first and
stops. Not hypothetical: catalogue entry `PR7-PIPELINE-010` reduced step
(e) to `.take(1)` and the whole suite stayed green. Opt-in rather than
default, so no existing fixture's registry size moves.

## `struct Damage` › `two_tier: bool,`

Freeze a **two-tier** chain instead of the default one-tier one.

The default chain has a single tier, so a task's rung is always 0 and a
driver that read the rung from the fold is indistinguishable from one
that assumed zero. That is not a hypothetical: it is why the `rung` half
of `PR7-FOLD-LADDER-POSITION`'s reader stayed unwitnessed through the
repair filed against it, and why S5 round 2 found it still open.

## `struct Damage` › `deep_ladder: bool,`

Freeze a two-tier chain with **two attempts per rung**.

Neither existing chain can show an *accumulated* brief. `chain()` has one
tier, so its second failure exhausts the ladder and the task parks with
no third dispatch; `escalating_chain()` allows one attempt per rung, so
nothing ever carries two entries onto a rung. §11.4's second half —
"next rung, fresh session, **accumulated feedback summary included**" —
needs a ladder deep enough to hold two failures below the rung that reads
them, and this is that ladder. Additive so no existing fixture's chain
moves.

## `impl Fixture` › `fn manager(&self) -> crate::workspace_manager::WorkspaceManager {`

The manager recovery step (g) rebuilds worktrees through.

Derived from the fixture's own repository and private root rather than
stubbed: (g)'s whole subject is a real `Worktree.Verify` against a real
checkout, and a manager that could not reach one would make every
assertion about the step vacuous.

## `fn build(tag: &str, damage: Damage) -> Self` › `crate::workspace_manager::fixture::git(&repo_root, &["init", "-q", "-b", "main"]);`

A **real** repository, not a `.git` directory made with `mkdir`.
Recovery step (g) rebuilds worktrees through a `WorkspaceManager`,
and `WorkspaceManager::derive` asks Git where the common dir is — so
a fixture whose `.git` is an empty directory cannot express the step
at all, and every assertion about it would be vacuous.

## `fn build(tag: &str, damage: Damage) -> Self` › `for setting in [`

A seed commit, so the repository has a real base a worktree can be
cut at. Step (g) recreates `OpenNoAttempt` worktrees "at their bases",
and a base that names no object makes the step's own funnel fail for
a reason that has nothing to do with what is being tested.

## `fn build(tag: &str, damage: Damage) -> Self` › `["config", "core.autocrlf", "false"],`

Line endings are pinned in the repository's own config, for the reason
`workspace_manager::fixture` pins them (§12, and
`PR126-REVIEW2-NULL-TESTS-INHERIT-THE-HASH-FORMAT`): an ambient Git setting
that silently changes what a test observes. Git for Windows installs
`core.autocrlf=true` in its system config, and under it a blob written as
`A\n` is checked out as `A\r\n` — by `git worktree add` as much as by
`git checkout`, because a linked worktree reads this repository's config —
so a test that compares a checkout's bytes against what it committed fails
on that platform alone while the commit is the one it asked for. That is
what happened to
`a_dependent_task_is_dispatched_into_its_dependencys_merged_work` on CI's
winguest leg (`PR247-DISPATCH-HEAD-WITNESS-RED-ON-WINDOWS`): alpha's
`candidate.txt` was in beta's worktree with the right content and Windows
line endings, and the dispatch it witnesses was correct.

The pin is in the repository rather than in the environment of the
fixture's `git` helper because the checkout that matters is production's:
`WorkspaceManager` runs `git worktree add` with the process's own
environment, and the repository config is the one layer both read. A test
that compares checkout bytes through any other fixture inherits the same
obligation.

## `fn build(tag: &str, damage: Damage) -> Self` › `let marker = CreatingMarker {`

P1: the `.creating` marker the creator published and never removed,
because this run was interrupted between P5b's commit record and P8's
`RunDir.RemoveMarker`. That is the shape a resume exists for, and it
is what makes recovery step (a1)'s "this run's own stale marker,
**which the owner removes here**" a removal that removes something.
Without it every "no census effect followed this refusal" assertion
below is vacuously true, and the census's own write has nothing to be
the anchor of.

## `fn build(tag: &str, damage: Damage) -> Self` › `let mut warnings = Vec::new();`

The log, through the Event funnel and nothing else.

## `impl Fixture` › `fn worktree_lock_file(&self) -> PathBuf {`

The repository-scoped R25 lock file, whose *existence* is what a
`*_before_any_lock` test asserts about.

## `impl Fixture` › `fn derive(&self, explicit: Option<&Path>) -> Result<RootDerived, UpstrokeError> {`

(a0), with the reader ceiling raised so a schema-4 log is readable at
all. Production's ceiling is 3 and refuses here; see
`RootDerived::derive_with`.

## `fn drop(&mut self)` › `let _ = rundir::remove_public_husk(&self.root, &mut NoHooks);`

`remove_public_husk` removes a directory's children and then the
directory. It is the one recursive delete this module can reach
through a site-taking funnel, and a fixture per test is what
exhausts inodes on the build box when nothing does.

## `struct PlantedHusk {`

---------------------------------------------------------------------------
A husk beside the run
---------------------------------------------------------------------------

## `struct PlantedHusk {`

A husk this repository's next write command may reclaim, planted through the
same funnels a creator would have used.

The prefix is a creator that died after P3b and before P5b: a published
`.creating`, the private half it names, and the reciprocal `owner.json` — the
twelve conjuncts of [`rundir::prove_private_half_ownership`] all satisfied,
so [`crate::rundir::PrivateHalfOwnership::Proven`] and both halves
reclaimable. `committed` publishes `committed.json` as well, which fails
conjunct 12 and turns the same shape into a retention: the control half, so
"the census reclaimed it" is a claim about the proof rather than about the
census deleting whatever it walks over.

## `fn tree_bytes(root: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {`

Every file under `root`, by relative path, with its bytes.

What a "retained" assertion compares. Byte-identity, not existence: a census
that emptied `owner.json` would leave the directory present and every weaker
assertion green.

## `fn event_kinds(log: &[u8]) -> Vec<String> {`

Every `"event":"<kind>"` in a log, in order.

## `fn plan_with(two_tasks: bool) -> Plan {`

---------------------------------------------------------------------------
The recorded run
---------------------------------------------------------------------------

## `fn escalating_chain() -> ChainSummary {`

A chain with a rung above the first, so an escalation has somewhere to go.

The binding's **model differs per tier**, which is what makes the rung
observable in the dispatched attempt: a driver reading rung 0 for an
escalated task runs it on the cheap model forever, and the only visible
symptom is a task that never gets better.

## `fn escalating_chain() -> ChainSummary` › `BindingSummary {`

**Rung 0 matches the default chain's binding on purpose.** The
`attempt_started` helper seeds that binding, and the fold refuses
an attempt whose binding is not the one the run froze for its rung
— `check_attempt_started`'s `BindingMismatch`, which caught the
first draft of this fixture. Only the rung *above* differs, which
is the rung this test is about.

## `fn deep_chain() -> ChainSummary {`

Two tiers **and** two attempts per rung.

The only chain in this file on which a rung can read more than one earlier
failure. Rung 0 spends two attempts, both fail, and the escalation lands on
rung 1 with two records below it — which is what §11.4's "accumulated
feedback summary" is a claim about. Its bindings match
[`escalating_chain`]'s for the same reason that one's match [`chain`]'s: the
seeded `attempt_started` carries rung 0's binding, and the fold refuses an
attempt whose binding is not the one the run froze for its rung.

## `fn run_started(`

A `run_started` whose two digests authenticate against the frozen plan.

The registry digest is derived the way the fold derives it — from the plan,
this record and the probed agents — rather than written as a literal,
because a literal would be a second authority on the same number and the
fixture would drift from the fold the first time either changed.
`base` is the repository's real seed commit rather than a literal, because
the driver dispatches at it: a recorded base that names no object makes
`git worktree add` fail for a reason that has nothing to do with what is
being tested, and a fixture whose record disagrees with its own repository
is not the shape any real run has.

## `let mut reviews = review_plan();`

`second_opinion` is per task, so a second task needs a second
entry — the registry refuses a record whose review alignment does
not match its plan, which is the check working.

## `fn runtime_holding_the_record() -> FakeRuntime {`

---------------------------------------------------------------------------
Seams
---------------------------------------------------------------------------

## `fn runtime_holding_the_record() -> FakeRuntime {`

A runtime holding this run's recorded image and its credential volume.

## `struct RecordingRunner {`

A `Runner` that answers every request with `exit 0` and records what it saw.

## `struct RecordingRunner` › `failing: Mutex<Option<String>>,`

A program whose invocation fails, so a probe refusal can be constructed.

## `struct RecordingRunner` › `filters: Mutex<bool>,`

Whether the worker also declares a clean/smudge filter.

A `.gitattributes` naming a filter makes the staged bytes and the bytes
a gate would see potentially different, which is what the ladder's third
cheap rung refuses.

## `struct RecordingRunner` › `edits: Mutex<bool>,`

Whether an `Implement` invocation edits the worktree it was given.

Off by default, because most tests here only care that a process ran.
A driver test that means to reach the **candidate sequence** needs a
non-empty diff: the ladder's cheap rungs reject an empty one, which is
what `pr_sequence[8]`'s "empty-diff attempt failures" names.

## `impl RecordingRunner` › `fn filtering() -> Self {`

A worker that leaves a change behind **and** a filter declaration, so
the staged evidence is not the evidence a gate would see.

## `impl RecordingRunner` › `fn editing() -> Self {`

A runner whose worker leaves a change behind, so an attempt can succeed.

## `fn run(&self, request: &RunnerRequest) -> Result<ProcessOutput, UpstrokeError> {` › `if code == 0`

The worker's edit, which is the whole difference between an attempt
the cheap rungs reject and one that reaches a candidate.

## `fn run(&self, request: &RunnerRequest) -> Result<ProcessOutput, UpstrokeError> {` › `crate::workspace_manager::fixture::write_file(`

Through the fixture funnel every other test in this file uses:
`std::fs::write` is on the effect denylist here, and a test that
reached around it would be the first.

## `struct StubAdapter;`

An adapter that reports itself through one probe process.

## `struct AlwaysCertifies;`

A `RunnerPreflight` that certifies without spawning, for the tests whose
subject is a step other than (c).

## `enum RefShape {`

---------------------------------------------------------------------------
The integration ref namespace, with no Git behind it
---------------------------------------------------------------------------

## `enum RefShape {`

What `assert_publishable` finds at the recorded ref.

The two refusing shapes are the two `WorkspaceManager::assert_publishable`
has: `refuse_symbolic` first, then a walk of the worktree records. Both are
reproduced with the production [`Refusal`] values rather than with invented
messages, so an assertion on the sentence an operator reads is an assertion
on the sentence the real funnel would have produced.

## `enum RefShape` › `Direct,`

A direct ref nothing has checked out. Publishable.

## `enum RefShape` › `Symbolic,`

A symbolic ref. `INV-17` makes every engine ref direct.

## `enum RefShape` › `CheckedOut,`

A direct ref some worktree has checked out.

## `struct RecordingRefs {`

[`IntegrationRefs`] with no repository behind it, which still enters the
`Ref.CreateIntegration` funnel positions.

**It must enter them.** Every ordering claim in this file reads its evidence
out of the shared [`HookHarness`], and a double that performed the effect
without consulting `hooks.phase` would leave the one durable Git effect of
the whole recovery order invisible to it — a site contributing nothing to
the coverage evidence. `hooks` here is the bundle's own
[`crate::workspace_manager::EffectHooks`], so the recording lands in the same
harness the other four families record into and needs no second wiring.

It also snapshots the run's event log at each entry. The position claim this
file has to make about the P7/P8 step is "the ref was created **before any
recovery event was appended**", and the log's bytes at the instant of the
effect are that claim directly, with no ordering index standing in for it.

## `struct RecordingRefs` › `log: PathBuf,`

Where this run's `events.jsonl` is, so an entry can snapshot it.

## `struct RecordingRefs` › `targets_read: Mutex<usize>,`

How many times `direct_target` was asked. The control half of the
unpublishable-ref test: `assert_publishable` runs **first**, so a build
that dropped it would still refuse a symbolic ref — at
`direct_ref_target`'s own `refuse_symbolic` — and a test that asserted
only "it refused" would stay green through the loss.

## `struct RecordingRefs` › `entered: Mutex<Vec<Vec<u8>>>,`

The log's bytes at each entry into `Ref.CreateIntegration`.

## `impl RecordingRefs` › `fn with_log(log: &Path, shape: RefShape, at: Option<String>) -> Self {`

The general constructor, by log path — the kill child has no
[`Fixture`], only the repository the parent named it.

## `impl RecordingRefs` › `fn absent(fixture: &Fixture) -> Self {`

Nothing is there — a run killed between P6 and P8.

## `impl RecordingRefs` › `fn at(fixture: &Fixture, sha: &str) -> Self {`

A direct ref already at `sha`.

## `impl RecordingRefs` › `fn shaped(fixture: &Fixture, shape: RefShape) -> Self {`

Nothing is there, and `assert_publishable` answers `shape`.

## `impl RecordingRefs` › `fn created(&self) -> Vec<(String, String)> {`

Every `(refname, sha)` the funnel actually created.

## `impl RecordingRefs` › `fn target(&self) -> Option<String> {`

The ref's current target.

## `impl RecordingRefs` › `fn targets_read(&self) -> usize {`

How many times the ref's target was read.

## `impl RecordingRefs` › `fn log_bytes_at_entries(&self) -> Vec<Vec<u8>> {`

The log's bytes at each entry into the funnel.

## `impl RecordingRefs` › `fn log_kinds_at_entries(&self) -> Vec<Vec<String>> {`

The event kinds the log held at each entry into the funnel.

Beside [`Self::log_bytes_at_entries`] and asserted first, because the
byte comparison's failure output is two `run_started` lines rendered as
`Vec<u8>` and nobody can read which of them grew. The kinds say it in
one line, and the bytes still catch a difference the kinds cannot see.

## `fn direct_target(&self, refname: &str) -> Result<Option<String>, UpstrokeError> {` › `if self.shape == RefShape::Symbolic {`

`WorkspaceManager::direct_ref_target` opens with `refuse_symbolic`
too, and reproducing that is what makes the symbolic case's
`targets_read` assertion load-bearing rather than decorative: without
it, dropping `assert_publishable` would leave the symbolic arm still
refusing here and nothing would notice which check caught it.

## `impl IntegrationRefs for RecordingRefs` › `crate::workspace_manager::refuse_new(refname, new)?;`

The contract's refusal, before the funnel is entered: the real
primitive refuses a malformed or null value before its funnel runs.

## `impl IntegrationRefs for RecordingRefs` › `return Err(UpstrokeError::Git {`

What `git update-ref --no-deref <ref> <new> ""` answers when
the ref appeared between the read and the write.

## `fn the_recording_refs_refuse_a_null_new_value_as_the_real_primitive_does() {`

The contract [`IntegrationRefs::create_zero_old`] states binds this double
as it binds `WorkspaceManager` (`PR126-REVIEW2-DOUBLES-ACCEPT-NULL-NEW`):
a null value is refused before the funnel is entered, and nothing is stored.

## `fn injected(`

`workspace_manager::apply`, which is private to that module — the same three
answers, so an arming at this site does here what it does at every other Git
funnel.

## `struct ArmedHooks {`

---------------------------------------------------------------------------
A funnel that refuses, inside the whole recovery order
---------------------------------------------------------------------------

## `struct ArmedHooks {`

[`HarnessTopologyHooks`] with its run-directory family replaced by one that
returns [`Injection::Error`] at a nominated `(site, phase, nth)`, and records
into the same [`HookHarness`] the other four families do.

Module-local, because `HookHarness::arm` takes a [`SubEffectPoint`] and
`hook()` answers `Proceed` to `Before`/`After` unconditionally, so a
`RunDir` site's two phases are not armable through it.

## `struct Given<'a> {`

---------------------------------------------------------------------------
Driving one resume
---------------------------------------------------------------------------

## `struct Given<'a> {`

What one resume was given, beyond the fixture.

## `struct Given<'a>` › `refs: RecordingRefs,`

The integration ref namespace the P7/P8 step publishes into.

Owned rather than borrowed, so [`Given::healthy`] can build the
default — a resume killed between P6 and P8 finds **no** ref, which is
the shape every other test in this file already implied and none of them
could say. A test that needs another shape assigns the field.

## `impl<'a> Given<'a>` › `fn healthy(`

The healthy case: the runtime holds the record, the pre-flight
certifies, today's config is the recorded one, and the recorded
integration ref is not there yet.

## `fn resume(`

Run (a0) and then the whole order, recording every site into `harness`.

## `(outcome.map(|(recovered, _handle)| recovered), warnings)`

The handle is dropped here, which releases the run lock and then the
worktree lease — the same thing that happened at the end of the recovery
order before the loop existed to hold them. A test that needs them alive
takes [`resume_holding`].

## `fn resume_holding(`

[`resume`], keeping the [`RunHandle`] the order hands back.

## `fn resume_with(`

[`resume`], with the hook bundle supplied — so a test can arm one.

## `fn any_lock_site_ran(harness: &Arc<Mutex<HookHarness>>) -> Vec<&'static str> {`

Whether any lock site ran — the R17 half of "no hold was taken".

## `fn first_observation(harness: &Arc<Mutex<HookHarness>>, site: EffectSiteId) -> Option<usize> {`

The index of a site's first observation, for an ordering assertion.

## `fn resume_with_explicit_private_root_mismatch_refused_before_any_lock() {`

===========================================================================
(a0) — the read-only refusals, before any lock
===========================================================================

## `fn resume_with_explicit_private_root_mismatch_refused_before_any_lock() {`

An explicit `--private-root` that names another root refuses **before any
lock**, and "before any lock" is asserted as the packet states it: no R17
hold was taken and no R25 lock file was created.

"The command refused" is a weaker claim and would be green for an
implementation that took the worktree lease, created
`upstroke-worktree.lock`, and then noticed. The lock file is the one that
bites: `Lock.AcquireWorktree`'s funnel opens it with `create(true)`, so
merely *reaching* the acquisition leaves a repository-scoped artifact behind
on a command that was supposed to end read-only.

## `fn malformed_recorded_locator_refused_before_any_lock() {`

A recorded locator of any shape other than `<root>/runs/<run_id>` refuses
before any lock, and every shape is refused rather than only the obvious
one.

Three shapes, because each fails a different clause: a missing `runs`
component, a trailing component that is not the run id, and a locator whose
path escapes upwards. The third is the one a "does it end with the run id"
check would accept.

## `fn resume_derives_private_root_from_record_when_default_changed() {`

A resume takes the private root **from the record**, not from today's
default — even when the default root has moved somewhere else entirely.

The fixture's root is a temporary directory that is never
`rundir::default_private_root()`, so a `derive` that consulted the default
would produce a different path and the census below it would scan the wrong
tree. Asserted as an equality against the recorded locator's parent rather
than as "the resume succeeded".

## `fn resume_derives_private_root_from_record_when_default_changed() {` › `let canonical =`

Compared canonical-to-canonical, because `authorized_root` is
deliberately **lexical**: it refuses a locator whose shape is not
`<root>/runs/<run_id>` and resolves nothing, so it hands back the root in
whatever form the record wrote. Canonicalising only the right-hand side
compares two spellings of one directory and fails wherever the temporary
directory sits under a symlink — which is macOS, where `TMPDIR` is under
`/var` and `/var` is a link to `/private/var`. Linux's `/tmp` is real, so
this passed there and failed only in CI's macOS leg.

## `fn resume_refuses_missing_private_half() {`

===========================================================================
(a) — the records, before any private write
===========================================================================

## `fn resume_refuses_missing_private_half() {`

A recorded private half that is not on disk refuses, and **is not
recreated**.

`recovery_order` (a): "a missing schema-4 private half is not recreated —
deferred". So the assertion is two-sided: the command refuses, *and* the
directory the record names is still absent afterwards. A build that
helpfully created it would satisfy "refuses" for one more line and then
authorize deletions against a boundary nobody wrote.

## `fn resume_refuses_missing_or_disagreeing_owner_record() {`

A missing `owner.json`, and a present one disagreeing in any of the four
identity fields, both refuse — and each refusal names the field.

One test over five cases rather than five tests, because the claim is that
the check is a *conjunction*: a build that compared only the run id passes
any single-case test that happens to damage the run id.

## `fn resume_refuses_missing_or_disagreeing_owner_record()` › `let private = fixture.private_root.join("runs").join(RUN_ID);`

Before any private write: the private half still holds exactly the
two records the creator left, and nothing new.

## `fn resume_refuses_commit_record_digest_mismatch() {`

`committed.json`'s `run_started_sha256` must equal the digest of the
committed first line, and a mismatch refuses quoting **both** numbers.

## `fn resume_refuses_owner_record_runner_mismatch() {`

`owner.json.runner` must equal `run_started(4).runner` exactly, and the
refusal names **which field** moved.

INV-23 makes this an (a) refusal rather than a (c) one: "every later
incarnation rebuilds the Runner from `run_started(4).runner` — **verified
equal to `owner.json.runner`** — before its RunnerPreflight". A build that
checked only at the rebuild would already have censused, which is a
fold-derived reclaim decided under a runner identity nobody agreed on.

## `fn resume_refuses_digest_mismatch() {`

===========================================================================
(a1) — the stable-prefix barrier
===========================================================================

## `fn resume_refuses_digest_mismatch() {`

A plan whose digest is not the one the log recorded refuses at the barrier's
**checked replay**, and nothing fold-derived happens.

`refusal_condition`'s first clause is "plan or registry digest mismatch",
and `stable_prefix_barrier` step (5) is where a log is replayed through the
checked fold. So the refusal is the replay's, and the assertion is that it
names `CheckedReplay` — not merely that something went wrong.

## `fn resume_establishes_stable_prefix_barrier_before_any_fold_derived_effect() {`

`Event.OpenLog`, its `SyncPrefix` point and `Event.ProvePrefixStable` all
execute **before** the first fold-derived effect of the census.

The ordering is asserted over the harness's first-observation order, which
is what makes this a claim about the *sequence* rather than about
possession. `RunDir.RemoveMarker` is the census's own write and, **in this
fixture**, the earliest fold-derived effect the order performs — the runs
tree holds this run's directory and nothing else, so no husk reclaim can
precede it. That is a property of the fixture rather than of the order: a
husk sorting before this run's id would put `RunDir.RemovePrivateHusk`
first, and the census walks in ascending run-id order. So the fixture's
emptiness is asserted rather than assumed, and the anchor is the census's
first effect *here*: if the barrier's three sites do not all precede it, the
resume decided something from a prefix it had not proven.

## `fn resume_establishes_stable_prefix_barrier_before_any_fold_derived_effect() {` › `assert_eq!(`

Asserted **before** the resume, because the resume reclaims what it walks:
afterwards a husk that had preceded this run in the walk is gone and the
same assertion passes vacuously.

## `fn resume_refuses_before_any_fold_derived_effect_when_prefix_sync_fails() {`

A `SyncPrefix` that returns `Err` ends the command with **nothing done**.

`stable_prefix_barrier`: "a failed sync … performs none of those effects:
the write command ends … with an infrastructure error naming the run id and
the failed step, no append handle is used, the run is NoRunFinished and
resumable". Three assertions, because "it returned an error" is true of a
build that censused first and refused afterwards.

## `const ALPHA: TaskKey = TaskKey(0);`

---------------------------------------------------------------------------
Later events, for the prefixes a resume has to recover from
---------------------------------------------------------------------------

## `fn dispatched_at(base: &CommitSha) -> TopologyEventBody {`

[`dispatched`], at a base that names a real object.

The constant-SHA version is enough for every test whose subject is the
fold, because the fold does not resolve a base. Step (g) does — it cuts a
worktree at it — so its fixture has to name the repository's own commit.

## `fn for_task(key: TaskKey, prefix: &str, body: TopologyEventBody) -> TopologyEventBody {`

Re-key an event built for `alpha` onto another task.

The seeded-event helpers are all `ALPHA`'s, which was enough while every
fixture had one task. A step that loops needs a second, and re-keying is
cheaper than a second set of builders — and keeps the two tasks' events
identical apart from the key, which is what makes "the step handled both" a
claim about the step rather than about the fixture.

The predicted region moves with the key: two tasks holding the same region
is an overlap the fold refuses, and rightly.

## `fn in_generation(generation: GenerationId, body: TopologyEventBody) -> TopologyEventBody {`

Re-key an event built for generation 0 onto a later generation.

`for_task` moves a seeded event sideways; this moves it forward. A
**closed** generation is what a sessionless retry and every escalation leave
behind — `settle::failed` closes it and the next attempt runs in a fresh one
— so a log with two failures below one rung has two generations in it, and
`attempt_started` on a closed generation is a barrier refusal rather than a
fixture. The worktree path moves with the generation because two live
dispatches may not name the same one.

## `fn attempt_finished(attempt: u32, settlement: AttemptSettlement) -> TopologyEventBody {`

`attempt_finished`, whose record **says the attempt failed** — because every
settlement this helper can build is a failure.

`candidate_prepared` is the sole successful settlement, so an
`attempt_finished` is a retry, an escalation, a park, a deferral, a retained
hold or a terminal failure, and each of those is an attempt that did not
succeed. This built `attempt_record(attempt)` — `failure: None`, no reviews —
so every fixture using it produced a settlement that fails a task while
carrying a ledger line saying the work passed. The fold accepted that until
2026-08-27; now `check_attempt_finished` refuses it, and this helper would
have made ~8 fixtures refuse rather than making them coherent.

Deriving the record from the settlement is the fix, not attaching a failure
at each call site: a fixture that has to remember to make its own event
self-consistent is a fixture that will stop doing so.

## `fn attempt_finished(attempt: u32, settlement: AttemptSettlement) -> TopologyEventBody {` › `if let AttemptSettlement::Retained {`

The settlement's retained session and the record's `session_id` are one
value in production — both come from `assessed.outcome.session_id` — and
the fold refuses a retained settlement whose halves name different
conversations.

## `fn attempt_finished_failing(`

`attempt_finished` carrying the failure — and the feedback — a crash left
durable.

[`attempt_finished`] records `failure: None`, which is the shape of an
attempt nothing judged. A crash-resume claim is about the other shape: the
ladder decided something, and §11.4's feedback is on the record it decided
from. `detail` is what the next attempt is told, and it is the field this
helper exists to put in a log.

## `fn resume_finalizes_halted_then_refuses() {`

===========================================================================
(b) — Complete or Halted
===========================================================================

## `fn resume_finalizes_halted_then_refuses() {`

A Halted run does not continue.

### About the word "finalizes" in this test's name

Step (b) is "terminal finalization **then** refuse continuation", and this
slice implements the refusal only: `RunDir.WriteReport` carries
`fault_row: t_finalize`, which is not one of PR7's eleven rows, so writing a
report here would be an out-of-row effect with no fault coverage in this
slice. The name is the packet's and is kept unchanged so the row and the
test still correspond; what it asserts is the half in range, and it asserts
the other half's **absence** explicitly rather than leaving it unstated —
no `report.json`, and no `RunDir.WriteReport`.

## `fn resume_finalizes_halted_then_refuses()` › `attempt_finished(`

`halts_run: false`: the task ends terminal and the run does
not halt, so the derived outcome is Complete rather than
Halted — which is what makes both arms of (b) constructible
without any integration terminal this slice does not
implement.

## `fn resume_rebuilds_runner_from_record_and_warns_on_config_drift() {`

===========================================================================
(c) — the rebuild and its warnings
===========================================================================

## `fn resume_rebuilds_runner_from_record_and_warns_on_config_drift() {`

A `[runner]` config that differs from the record **warns naming the
difference** and is ignored: the run resumes on its recorded runner.

Both halves asserted. A build that warned and then used today's config
would satisfy the warning half, and `run_resumed(4).runner` would then
differ from `run_started(4).runner` — which the fold refuses, but only if
the record actually reaches it.

## `fn resume_rebuilds_runner_from_record_and_warns_on_config_drift() {` › `let log = String::from_utf8(fixture.log_bytes()).expect("the log is utf-8");`

The record won: `run_resumed` carries the recorded volume, not today's.

## `fn resume_warns_when_reference_moved_and_uses_recorded_image_id() {`

A recorded reference that now names another image warns, and the run keeps
running **from the recorded id**.

INV-23: "a moved reference cannot change what executes". The fake's mutable
tag table is what makes this constructible at all — the reference is moved
to a second image the runtime also holds, so the refusal path (an absent id)
is not what is being exercised.

## `fn resume_refuses_by_inspection_before_any_spawn_when_runtime_image_id_or_volume_absent() {`

An unavailable runtime, a recorded image id the runtime no longer holds, and
an absent credential volume each refuse **before any spawn**.

The predicate is the type, not the prose: `RunnerRebuilt::rebuild` runs
`rebuild_by_inspection`, and `PreflightCertified::certify` is the only thing
that spawns — so a refusal that produced no `RunnerRebuilt` cannot have
spawned. Asserted here through a pre-flight that would *panic* if it were
reached.

## `fn resume_refuses_by_inspection_before_any_spawn_when_runtime_image_id_or_volume_absent() {` › `struct NeverRuns;`

A pre-flight that must never run. `certify` is unreachable if the
inspection refusals really do precede every spawn, and this is what
turns "unreachable" into a failing test rather than a comment.

## `fn resume_refuses_by_inspection_before_any_spawn_when_runtime_image_id_or_volume_absent() {` › `type Damage = fn(&FakeRuntime);`

One way to leave a recorded runner un-re-establishable.

## `fn resume_refuses_by_inspection_before_any_spawn_when_runtime_image_id_or_volume_absent() {` › `runtime.add_image("sha256:absent", None);`

Remove the image itself: moving the tag alone leaves the id
present, and the id is what the rebuild asks about.

## `fn chain_to_census(`

---------------------------------------------------------------------------
Driving only as far as the census
---------------------------------------------------------------------------

## `fn chain_to_census(`

(a0) → (a) → (a1) → (a), stopping at the census, so a test can read what it
found. The full order consumes the witness at (h) and nothing survives it.

## `fn chain_to_census_with(`

[`chain_to_census`], with the hook bundle supplied — so a test can arm one.

## `fn resume_of_nondefault_root_run_reclaims_earlier_incarnation_intents_in_recorded_root() {`

===========================================================================
(a) — the census, in the recorded root
===========================================================================

## `fn resume_of_nondefault_root_run_reclaims_earlier_incarnation_intents_in_recorded_root() {`

A run whose private root is not today's default still has its **earlier
incarnations'** containers reclaimed, and they are reclaimed **in the
recorded root**.

Three assertions, and the third is the one the test is named for: the
census's own report has to name the recorded root. A build that censused
`default_private_root()` would find nothing, reclaim nothing, and return a
perfectly successful report — so "the container was reclaimed" alone is not
enough; the root the census scanned is part of the claim.

## `fn resume_of_nondefault_root_run_reclaims_earlier_incarnation_intents_in_recorded_root() {` › `let invocation = crate::runner::InvocationId::probe(`

An intent this run's *creator* incarnation left behind, in the recorded
root. It is dead by construction: the run lock is exclusive, so only one
incarnation of a run is ever live, and this process is a different one.

## `fn resume_reclaims_a_provable_husk_beside_the_run_and_retains_a_possibly_committed_one() {`

A resume **reclaims** the husks beside the run it is resuming: the private
half first, through the proof-token funnel, then the public directory with
the marker last.

`recovery_order` (a1)'s census is a "run-directory census incl. this run's
own stale marker, which the owner removes here, **and husk reclamation under
the ownership proof**", and INV-15 reclaims pre-run husks "at write-command
start under the worktree lock". A resume is a write command and holds that
lock. A run-directory pass that classified and reported would leave a
provable husk on disk for ever: every later resume would report it again, and
only a fresh `upstroke run` would ever reclaim it.

Three claims, and the third is what makes the first two mean anything:

* the provable husk is gone, both halves, and the report names the arm;
* `RunDir.RemovePrivateHusk` precedes `RunDir.RemovePublicHusk` — reversed, a
  kill between the two leaves a private half no marker names and no later
  census can ever prove;
* the husk carrying `committed.json` is byte-identical afterwards. A census
  that deleted whatever it walked over would pass the first two.

## `fn resume_reclaims_a_provable_husk_beside_the_run_and_retains_a_possibly_committed_one() {` › `assert_eq!(`

And the run being resumed: its own stale marker repaired by its owner, and
nothing else. The husk arms are gated on the run lock, which this process
holds for its own directory.

## `fn resume_completes_past_a_husk_whose_private_half_cannot_be_removed() {`

**A husk this resume cannot reclaim does not fail this resume.**

Before the census was shared, the resume's run-directory half was
`list_husks` + `husk_report` — both infallible — plus one `remove_marker`.
Sharing the reclaiming census moved a command-fatal error path onto the
resume: one dead run whose private half the filesystem will not release
(`EACCES`, `EPERM`, `EBUSY`, or on Windows any still-open handle) made
`upstroke resume <id>` fail for **every** run in the repository, on every
attempt, for a different run's residue. T-RESUME enumerates its refusals and
this is not among them, and `startup_census` and INV-15 answer "cannot be
reclaimed" with *retain and report* everywhere else.

The husk sorts **before** this run's id, which is what makes the second claim
worth making: `run_dir_names` sorts ascending, so a census that stopped at
the failure never reached this run's own directory at all — and recovery step
(a1) gives this run's stale-marker repair to its owner, which is this
process. So the repair was collateral damage of a different run's residue.

## `fn resume_completes_past_a_husk_whose_private_half_cannot_be_removed() {` › `assert!(stuck.public.exists(), "the public half was removed anyway");`

The husk: retained where it was, with the locator the next census needs.

## `fn resume_completes_past_a_husk_whose_private_half_cannot_be_removed() {` › `assert!(`

And this run's own stale marker, which sorts after the failure, was still
repaired by its owner.

## `fn the_resume_census_reports_the_husk_it_could_not_reclaim() {`

The same husk, from the report's side: an entry naming the step that refused
and carrying its error, beside the own run's completed repair.

The sibling above asserts the **tree** and that the command survived; this
asserts that the census *said* what happened rather than merely surviving
it. INV-15's answer is retained **and reported**, and a census that swallowed
the failure into a `Skipped` or an `Ok` with no entry would pass the sibling.

## `fn the_resume_census_reports_the_husk_it_could_not_reclaim()` › `assert_eq!(`

And the entry after it: this run's own repair, performed.

## `fn resume_refused_while_reaper_hold_observed_then_succeeds() {`

===========================================================================
(a) — the surviving reaper hold
===========================================================================

## `fn resume_refused_while_reaper_hold_observed_then_succeeds() {`

A resume refuses while a surviving reaper's shared cleanup hold (R28) is
observed, and succeeds once it is released.

The observation is [`rundir::observe_cleanup_hold`], which is fail-closed:
a `cleanup.lock` it cannot inspect is a hold, because "an observation that
was made to fail is not an observation that found nothing". A directory in
the lock file's place is exactly that state and is constructible on every
platform through the directory funnel, which is why it is what stands in for
a live reaper here — the alternative is `libc::flock`, which is on the
effect denylist and which this module may not reach.

The refusal half is `#[cfg(unix)]` because the hold is: R28 is "a surviving
**Unix** reaper's shared cleanup hold", and `rundir`'s non-Unix `cleanup`
module answers `false` unconditionally. The success half runs everywhere,
and asserts on both platforms that the observation site executed — a Windows
build that skipped the question entirely would pass a test that only
asserted the outcome.

## `fn resume_refused_while_reaper_hold_observed_then_succeeds()` › `let cleanup = fixture.public().join("cleanup.lock");`

Bound inside the `cfg`, because only the `cfg` uses it. Bound
outside, Windows compiles an unused local and CI's `lint (windows)`
leg refuses it under `-D warnings` — which is exactly the gap
recorded as `windows-gate-lint-level-gap`: a local
`--target x86_64-pc-windows-msvc` check accepts code the guest does
not, because only the guest sets the lint level.

## `fn replayed(fixture: &Fixture) -> TopologyFold {`

===========================================================================
(d), (e), (h)
===========================================================================

## `fn replayed(fixture: &Fixture) -> TopologyFold {`

Replay the fixture's log from disk, which is the only way to read state a
resume left behind: `run_resumed` consumes the witness that carried the
live fold.

Replaying rather than keeping the live fold is also the stronger assertion.
INV-02's "live state and replay use one checked transition over the exact
wire event" means a claim made against the replayed fold is a claim about
the bytes, not about a `TopologyFold` this process happens to hold.

## `fn resume_clears_budget_stop_and_wakes_deferred() {`

A resume clears the previous epoch's budget stop and wakes every Deferred
task.

Both halves, and both read off the **replayed** log rather than off the
return value: the epoch-scoped stop is what makes "raise the ceiling and
resume" the answer to a budget stop, and a build that cleared it only in
memory would leave the next process refusing for a stop the log still
carries.

## `fn steps_d_and_e_reach_every_generation_not_the_first() {`

**Steps (d) and (e) handle every entry, not the first one.**

Two catalogue entries survived the whole suite at `6a21be6` for one reason —
no fixture had a second thing for these loops to reach:

- `PR7-PIPELINE-010` reduced step (e) to
  `retained_idle(..).into_iter().take(1)`, closing only the first
  `RetainedIdle` generation. Green.
- `PR7-PIPELINE-008` added `if lease == LineageHeld { continue; }` to step
  (d)'s loop, skipping a whole lease class. Green.

Both loops were already correct. What was missing was a fixture that could
tell a loop from a `.first()`, which is why this is a witness and not a
repair. `Damage::two_tasks` registers `beta` beside `alpha` so there are two
of everything for the steps to walk.

**Live above `max_parallel = 1`, latent at it** — which is exactly the
condition a carried row would have named. It is cheaper to hold it than to
write it down: PR11 inherits a substrate whose recovery loops are witnessed
rather than a note saying they are not.

## `fn steps_d_and_e_reach_every_generation_not_the_first()` › `dispatched(),`

alpha: retained and idle — step (e)'s subject.

## `fn steps_d_and_e_reach_every_generation_not_the_first()` › `for_task(BETA, "beta", dispatched()),`

beta: the same, and the second entry the loop must reach.

## `fn steps_d_and_e_reach_every_generation_not_the_first()` › `let before = replayed(&fixture);`

The premise: two retained generations before the resume. Without this the
assertion below is satisfied by a fixture that only ever had one.

## `fn retry_refused_after_resume() {`

A retained session belongs to the incarnation that retained it. Step (e)
closes the generation, so after the resume there is no retry to evaluate —
and the fold refuses one.

`recovery_order` (i): "`ready_retry` is never evaluated before (h) and the
fold refuses a stale-incarnation retry". The first clause is structural
here: nothing in this file evaluates `ready_retry`, and the loop that does
is behind `run_resumed`, which consumes the witness. The second is asserted
directly, against the replayed fold.

## `fn retry_refused_after_resume()` › `let refused = after`

And the transition itself is refused: a forged retry into the closed
generation does not plan.

## `fn an_interrupted_attempts_worktree_and_intent_are_reclaimed_by_recovery() {`

`T-ATTEMPT.resume_action`, the clause after the settlement: "the task
worktree scrubbed with force". Step (d) closes the generation and its
worktree and intent go with it — the sibling arm of (e)'s reclaim, found by
asking whether the class had another member.

## `fn a_reclaim_the_closing_recovery_never_reached_is_finished_by_the_next(retained: bool) {`

PR #249's adequacy review, finding 4's second half, in both arms. The first
recovery's closing append — `attempt_interrupted` or `generation_closed` —
returns an error at its `Synced` point, so the close is durable and the
scrub never ran: the generation is `Closed` and its checkout and intent are
still there. The next recovery closes nothing and reclaims them all the
same, from the closed state (`reclaim_closed_generations`); a third removes
no worktree at all. The review's witness failed at `3bce2c6a` with
`worktree=true, intent=true` after the second recovery. The second
recovery's removals are not counted exactly, because it also finishes the
promotions the interrupted first never reached (step (f)).

## `fn run_resumed_records_identical_runner_identity() {`

`run_resumed(4).runner` equals `run_started(4).runner` field for field.

Read off the log rather than off the value this process passed in, and
compared with `RunnerPolicy::difference` — which names which field moved —
rather than with `assert_eq!`, so the failure message is the field rather
than two pretty-printed records.

## `fn forged_run_resumed_with_different_runner_identity_refused_on_replay() {`

A `run_resumed` whose runner differs from `run_started`'s is refused **on
replay**, not merely at the point it would be written.

The forged line is appended straight through the Event funnel, which is
exactly what a hand-edited log or a hostile process would produce: the fold
never saw it. So the refusal has to come from the reader, and it does — the
barrier's checked replay refuses the whole prefix, which is what stops a
forged identity from authorizing anything.

## `fn forged_run_resumed_with_different_runner_identity_refused_on_replay() {` › `let harness = harness();`

And a resume over that prefix refuses at the barrier, before anything.

## `fn resume_after_append_error_follows_surviving_prefix() {`

An append that returns an error ends the command, and the **next** resume
establishes the barrier over whichever prefix survived and continues from
it.

The injection is at `Synced`, which is the case where the line is on disk
and the process cannot tell whether it is durable. `append_error_protocol`:
"the event is outcome-unknown; `apply_delta` is not run and the in-memory
fold is marked poisoned … the append is never retried … the run is
NoRunFinished and resumable and the next resume follows the fault row of the
surviving prefix (T-APPEND) only after its own barrier".

So: the first resume fails with the line present, and the second resume sees
a prefix ending in `run_resumed` and opens the epoch after it. Two
`run_resumed` lines is the correct convergence for the after-append order,
not a duplicate.

## `fn resume_after_append_error_follows_surviving_prefix()` › `assert!(`

**The append is never retried.** A second attempt through the same handle
would come back as the *poison* error rather than the injected one — the
funnel poisons the handle at the point that failed — so the error the
command ends with is what tells a retry from an end. `INJECTED_PREFIX`
present and `POISONED_PREFIX` absent is that distinction, and it is the
only observable one: a retry cannot succeed through a poisoned handle, so
the line count is the same either way.

## `fn resume_after_append_error_follows_surviving_prefix()` › `assert!(text.contains(RUN_ID), "the report names the run: {text}");`

**The protocol ran, and its report is what the command ends with.**
Everything above this point is true of a build that merely poisoned the
fold and returned the funnel's error, which is why none of it can stand
for `append_error_protocol`. Obligation (5) is the observable one: reopen
through `Event.OpenLog` (torn-tail normalization), establish the
stable-prefix barrier, and end "naming the run id, the event kind, and
whether the proven prefix contains the line".

## `fn resume_after_append_error_follows_surviving_prefix()` › `let second = harness();`

The next resume: a fresh harness, nothing armed, and it follows the
surviving prefix.

## `fn an_append_error_during_recovery_cancels_the_reservation_and_every_running_invocation() {`

An outcome-unknown append during recovery cancels the provisional
reservation and every still-running invocation.

`append_error_protocol` obligations (2) and (3):
[`Reservations::cancel_any`] — `permits`: "cancellation on any pre-append
failure, run end, shutdown, or a poisoned fold" — and
[`InvocationLedger::cancel_all_running`], the ledger half of "in-flight
invocations are cancelled through the Runner".

The recovery order's own ledgers are empty, so on that path both obligations
are satisfied vacuously and no test of `resume` could tell a build that ran
them from one that did not. So this test hands the emitter ledgers that are
**not** empty — one held reservation, one registered running invocation —
which is exactly why they are `EmitContext` fields rather than locals inside
the recovery order. Both ledgers balance afterwards: every entry settled
exactly once, which is the process-end condition R4 states.

## `fn real_preflight<'a>(`

===========================================================================
(c) — the RunnerPreflight probes
===========================================================================

## `fn real_preflight<'a>(`

The real pre-flight, over a runner that answers every process.

## `fn resume_refuses_by_preflight_probe_when_shell_or_cli_fails_before_any_recovery_event() {`

A failing shell, and a failing agent CLI, each refuse **before any recovery
event**.

Two cases and not one, because they are two different processes with two
different accountings: the shell probe is non-slotted and the agent probe
takes a slot pair. A build that refused correctly on one could hold a slot
forever on the other, so both assert the ledgers as well as the refusal.

## `fn resume_refuses_by_preflight_probe_when_shell_or_cli_fails_before_any_recovery_event() {` › `let programs: Vec<String> = runner`

The shell probe fails first, so the agent CLI is never asked. That is
the sequence `runner` states — "probes execute through it
sequentially at pre-flight" — and it is what makes the shell the
cheaper refusal.

## `fn ledgers_empty_after_resume() {`

Every process-local ledger is empty after a resume, and the shell probe took
no slot while the agent probe did.

`crash_reconstruction` requires "provisional reservations, slot table,
invocation ledger, and the coordinator's own lock holds are empty at process
start", and the resume path is what has to leave them that way. The
asymmetry is asserted from the recorded requests rather than from the
ledger's totals, because "one slot was taken" is true of a build that took
it for the wrong process.

## `fn ledgers_empty_after_resume()` › `assert!(crate::engine::topology::identity::Reservations::new().is_empty());`

And the process-local ledgers a fresh coordinator starts with are empty
by construction, which is the other half of the row.

## `struct ProbeContainerRunner<'a> {`

A `Runner` that gives every probe a real container through the container
funnel, and releases it on both paths.

This is the shape `ContainerRunner::run` has — `launch` then `release`,
with the release running whether or not the invocation succeeded — driven
against the fake runtime so a test can read what survived. Built here rather
than reused because `ContainerRunner` owns its runtime by value and hands
back no way to inspect it, and because the four effectful `ContainerRuntime`
methods are on the effect denylist for every module but the funnel — so a
delegating wrapper around the fake is not something this module may write.

## `struct ProbeContainerRunner<'a>` › `failing: String,`

The program whose container exits non-zero.

## `fn run(&self, request: &RunnerRequest) -> Result<ProcessOutput, UpstrokeError> {` › `release(`

Released on both paths: R26 is "released on complete …, cancel, or
shutdown" and R19's view is "pruned on complete or cancel".

## `fn resume_preflight_probe_containers_reclaimed_after_refusal() {`

After a pre-flight refusal, the probe containers are reclaimed: no
container, no intent, no Git view survives.

`expected_failures_refusals[2]` ends "…refuses before any recovery event or
work spawn, **the probe containers reclaimed**", and R19/R26 both say
"pruned/released on complete **or cancel**". A refusal is a cancel, so the
namespace has to be empty afterwards — otherwise the next write command's
census finds residue from a command that never started.

## `fn create_ref_entries(harness: &Arc<Mutex<HookHarness>>) -> u32 {`

===========================================================================
T-RUNSTART's P7/P8 repair
===========================================================================

## `fn create_ref_entries(harness: &Arc<Mutex<HookHarness>>) -> u32 {`

The `Ref.CreateIntegration` funnel's `Before` count, which is what "the
funnel was entered" means everywhere below.

Counted rather than tested for presence: "no spend repeats" is a claim about
*how many times* the effect ran, and `touched` would be green for a build
that created the ref, then created it again.

## `fn kill_after_run_started_creates_integration_ref() {`

`transaction_fault_matrix[T-RUNSTART].resume_action`, first clause:
"**P7/P8: create the ref zero-old at the recorded base if absent**".

The fixture *is* the prefix a kill between P6 and P8 leaves —
`run_started(4)` durable, `committed.json` naming its digest, the creator's
`.creating` still on disk because P7 never ran — and the ref namespace is
empty. A resume over it must leave the ref there.

### What this asserts that calling the function could not

This test used to live in `create::tests` and called
[`super::super::create::ensure_integration_ref`] directly with two literals.
That proved the *function* creates a ref, which was never in doubt. Driving
[`run_recovery_order`] proves the three things that actually were:

1. **that the recovery order calls it at all** — its only production caller
   used to be P8, so a run killed between P6 and P8 resumed with no ref and
   nothing to create one;
2. **with the recorded arguments** — asserted against
   `fixture.started.integration_ref` and `fixture.started.base_sha` rather
   than against constants, so a resume that published today's configured ref
   name, or the fold's current head, fails here;
3. **at a point before any recovery event** — the funnel snapshots the log
   on entry, and the bytes it saw are compared against the committed prefix.

## `fn kill_after_run_started_creates_integration_ref()` › `assert_eq!(`

The position claim, read off the effect itself rather than off an index:
when `Ref.CreateIntegration` ran, the log was still exactly the prefix
the creator committed — no `attempt_interrupted`, no `generation_closed`,
no `run_resumed`.

## `fn kill_after_run_started_creates_integration_ref()` › `let after = fixture.log_bytes();`

And the appends did happen — otherwise the assertion above is green for a
resume that never got as far as (d)–(h) at all.

## `fn a_resume_adopts_an_integration_ref_already_at_the_recorded_base() {`

The second clause: "**if present == base continue (no spend repeats)**".

Two ways in, because they fail differently. A resume that *finds* the ref
already at the recorded base is the ordinary case — some other process, or
an earlier resume, got there first. A **second** resume of the same run is
the idempotence case, and it is the one that would catch a step that
remembered nothing and re-pointed the ref every time.

Both assert the funnel's entry count and not the command's exit status: an
implementation that called `create_zero_old` again would get an `Err` back
from Git ("already exists; zero-old refuses"), and a build that swallowed it
would be green on `result.is_ok()` while having repeated the spend.

## `fn a_resume_adopts_an_integration_ref_already_at_the_recorded_base() {` › `{`

(1) Already there when the resume arrives.

## `fn a_resume_adopts_an_integration_ref_already_at_the_recorded_base() {` › `{`

(2) Two resumes of one run: the second adopts what the first created.

## `fn a_resume_refuses_an_integration_ref_at_another_sha_before_touching_anything() {`

A ref at any other SHA refuses — and refuses **before anything the step
would otherwise have done**.

`ensure_integration_ref`'s third disposition. "It refused" is the weak half
of the claim; the load-bearing half is that the refusal costs nothing:
`Ref.CreateIntegration` is never entered, the ref keeps the target it had,
and the log is byte-identical to the prefix the resume started from. A ref
that already names another commit belongs to something else, and a run is
never made room for by moving it.

## `fn a_resume_refuses_a_symbolic_or_checked_out_integration_ref() {`

A symbolic ref, and a checked-out one, refuse at `assert_publishable` —
before the target is ever read.

Two shapes and not one: they are the two arms of
`WorkspaceManager::assert_publishable`, and `refuse_symbolic` is also the
first statement of `direct_ref_target`, so a symbolic ref has two chances to
be caught and a build that lost the first would still pass a test that only
asserted "it refused". The `direct_target` count is what separates them —
neither shape may reach it.

## `fn the_p7_p8_step_runs_after_the_refusals_that_bound_it() {`

The step's three lower bounds, each asserted by the refusal that must
precede it: **(b)**, **(c)** and **(f)** all leave the ref untouched.

The bounds are stated in this module's own comment and this is what makes
them checkable rather than asserted in prose:

* **(b)** a Complete or Halted run does not continue, and publishing a
  finished run's integration ref is continuing it;
* **(c)** the repository is touched only once the recorded Runner has been
  rebuilt and its probes have answered, so a resume that cannot run leaves
  the object store as it found it;
* **(f)** an unresolved integration transaction is a prefix whose
  integration ref may be mid-move, so the P7/P8 step is skipped when the
  proven prefix carries one and [`finish_integration`] owns the ref; an
  unresolved promotion is completed by [`finish_promotions`], not refused.

The fourth bound, "**before (d)**", is not here: it is asserted positively by
[`kill_after_run_started_creates_integration_ref`], which reads the log at
the instant the funnel ran.

## `fn the_p7_p8_step_runs_after_the_refusals_that_bound_it()` › `{`

(b): a Halted run.

## `fn the_p7_p8_step_runs_after_the_refusals_that_bound_it()` › `{`

(c): a shell probe that does not answer.

## (end of `fn the_p7_p8_step_runs_after_the_refusals_that_bound_it()`)

**(f)'s pin-absent refusal is gone with the convergence it guarded.**
This case drove a `Promoting` generation whose prepared pin had vanished,
and asserted that step (f) refuses it before P7/P8 publishes any ref.
Since the 2026-08-27 CONFORM ruling there is no convergence to guard:
`candidate_prepared` is the sole successful settlement and the only thing
that promotes a generation, so a promoting generation carries its own
candidate identity and a pin is no longer what recovery rebuilds from.

The refusal that still bounds (f) is the integration transaction's, and
`a_resume_refuses_an_integration_ref_at_another_sha_before_touching_anything`
holds that ordering. Removed rather than rewritten around a predicate
that cannot fire — a case asserting a refusal nothing can reach would
pass for the wrong reason.

## `fn recovery_kill_child() {`

===========================================================================
A kill during recovery
===========================================================================

## `fn recovery_kill_child() {`

The child half of [`kill_during_recovery_repeats_recovery`].

`Injection::Kill` is `std::process::abort()` — a real process death, chosen
so the claim is *what a coordinator that runs no cleanup leaves on disk*.
The `unreachable!` at the end is load-bearing: it is what fails the test if
the injection ever silently stops killing.

## `fn recovery_kill_child()` › `let refs = RecordingRefs::with_log(`

The child's ref namespace is process-local and empty, which is what a run
killed before P8 has. The P7/P8 step runs before the first append, so the
child creates it here and then dies at `Event.Append`'s `Written` point —
nothing of it survives, and the parent's assertions are about the disk.

## `fn recovery_kill_child()` › `let manager = crate::workspace_manager::WorkspaceManager::derive(`

Step (g)'s manager, derived from the root (a0) just computed rather than
from an env var the parent would have to pass: the private root is the
one thing (a0) exists to establish, and taking it from anywhere else
would let the child rebuild worktrees under a root the order refused.

## `fn kill_during_recovery_repeats_recovery() {`

A kill at a recovery event's append leaves the run resumable, and the next
process **repeats the whole order from (a0)**.

`recovery_order` (i): "a kill at any point repeats from (a0)". So the
assertion is not only that a second resume succeeds — it is that the second
process re-derived the root, re-took the locks, re-established the barrier
and re-censused, all of which are (a0), (a) and (a1) running again over a
prefix a dead process left. A build that resumed from a checkpoint would
skip them and still finish.

The child is spawned **through the host Runner**, not through
`std::process::Command`: `std::process::Command` is on the effect denylist
and `src/engine/topology/**` may not reach it even in tests. The Runner is
the funnel that owns `Process.Spawn`, which is exactly the rule.

## `fn kill_during_recovery_repeats_recovery()` › `assert!(`

**Died, rather than failed.** `Injection::Kill` is `std::process::abort()`,
which takes the process before the test harness can print anything about
the test — so an aborted child emits no result line at all. A child whose
injection silently stopped killing reaches the `unreachable!`, panics, and
the harness prints both its message and a result line. Asserting only a
non-zero exit cannot tell those apart, because a failed test is also
non-zero; this is what makes the `unreachable!` load-bearing.

## `fn kill_during_recovery_repeats_recovery()` › `let after_kill = fixture.log_bytes();`

What the dead coordinator left: the line it was writing, unsynced, and no
cleanup of any kind.

## `fn kill_during_recovery_repeats_recovery()` › `const AFTER_THE_KILL: &str = "01KZTKILL00000000000000004";`

And the next process repeats the order from (a0).

The census's evidence has to be something the *repeat* can act on. The
dead child had already censused before it reached the append it died at,
so this run's stale marker is gone and stays gone: `RunDir.RemoveMarker`
would be absent from a build that repeated the census perfectly. A husk
planted now is the evidence instead — another crashed run, arriving
between the two processes — and it is the stronger one, because reclaiming
it is a census *effect* rather than a repair that finds nothing to do.

## `fn the_barrier_is_the_only_topology_route_from_a_proven_prefix_to_an_append_handle() {`

===========================================================================
The chain's one entry point, as a source census
===========================================================================

## `fn the_barrier_is_the_only_topology_route_from_a_proven_prefix_to_an_append_handle() {`

`StablePrefix::into_log_and_fold` is reached from exactly one production region of
the topology engine: [`BarrierHeld::from`].

### Why this is a census and not a visibility

Design v4 §4 makes `BarrierHeld` unforgeable by taking a `StablePrefix` **by
value**, and `StablePrefix`'s only constructor is
`events::log::establish_stable_prefix` — so barrier *evidence* cannot be
manufactured. What it does not close is the other direction:
`StablePrefix::into_log_and_fold` is `pub`, so a topology module could take a
proven prefix apart and hold the append handle and the fold **without**
wrapping them in a `BarrierHeld`, and then everything the chain hangs off —
`ResumeCensused`, and through it every recovery emitter — would be reachable
beside the chain rather than through it.

Narrowing the visibility cannot fix that here. `pub(crate)` does not stop
one topology module reaching another's dependency, and anything tighter than
`pub(in crate::events)` would break `BarrierHeld::from` itself, which *is*
built on `into_parts`. So the claim is the honest one — `BarrierHeld` is the
only route **the topology engine takes** — and this is what makes it a
checkable claim rather than a convention. Same idiom, and same reason, as
`events::log::tests::the_stable_prefix_barrier_is_the_only_way_a_log_becomes_a_topology_fold`.

## `fn the_barrier_is_the_only_topology_route_from_a_proven_prefix_to_an_append_handle() {` › `if !relative.starts_with("engine/topology") {`

Only the topology engine is in scope: the funnel that defines
`into_parts` and its own tests are not a second route into the
chain, they are where it lives.

## `fn the_barrier_is_the_only_topology_route_from_a_proven_prefix_to_an_append_handle() {` › `if test_modules.contains(&path) {`

A file the crate declares as a whole-file test module is test
code in full and has no production half; counting one would
count a fixture as a second route. **Through the crate's own
declarations**, not through the file name: six of the crate's
whole-file test modules are not called `tests.rs`, and one of
those is `engine/topology/scaffold.rs` — inside this very
census's `engine/topology` domain. `PR7-R5-ATT-001`.

## `fn the_barrier_is_the_only_topology_route_from_a_proven_prefix_to_an_append_handle() {` › `let production = crate::effects::production_code(&source);`

The production half only. A test that takes a prefix apart is a
fixture, not a path a run can take.

**`effects::production_code`, not a cut at the first
`#[cfg(test)]`.** The cut was the bug: it fired on the first *raw*
occurrence of the text, comments included, and in
`engine/topology/run.rs` that is **line 83 of 1777** — inside a
doc comment — so this census was scanning 4.7% of the driver, the
single most likely file for a second route to appear in. In
`engine/topology.rs` it was line 39, inside the module doc.

An earlier repair built the needle with `format!` so that a
mention *in this file* could not cut it. That fixed one instance
of `PR4-CENSUS-COMMENT-ORACLE` and left the class open in every
file this walk reads. `production_code` blanks comments and
string literals and removes each `#[cfg(test)]` **item** in place
rather than truncating, which is the repair the four whole-tree
censuses already have.

Found by S5 round 2's `seams` lens, and it lands on this slice's
own evidence: the guard was cited as proving that
`StablePrefix::events` did not become a second entry point, and
that check ran against the truncated domain.

## `fn the_barrier_is_the_only_topology_route_from_a_proven_prefix_to_an_append_handle() {` › `regions.push((relative.clone(), production.len(), source.len()));`

Calls, not definitions — a definition is not a route.

The needle used to be the bare `into_parts(`, and at integration
it reported five false routes: three definitions in `startup.rs`
and two calls in `create.rs`, every one of them a typestate
witness of that lane handing back its own fields. The comment
here said the fix was "to rename, not to widen the needle", and
that is what was done: `StablePrefix`'s accessor is
`into_log_and_fold`, a name nothing else in the crate carries,
so the needle now means what it says.
**The control this census did not have.** A zero count from an
empty region is indistinguishable from a zero count from a clean
file, and that is exactly how the truncation hid: the driver's
region was 83 lines of 1777 and its zero looked like a pass. The
four whole-tree censuses each carry this control; this one did
not, which is why the class survived here.

## `fn the_barrier_is_the_only_topology_route_from_a_proven_prefix_to_an_append_handle() {` › `for (file, region, whole) in &regions {`

Every region is a real fraction of its file. A tenth is a generous floor
and still an order of magnitude above what the truncation left behind.

## `fn the_recovery_order_performs_every_step_the_packet_names() {`

---------------------------------------------------------------------------
The order's completeness against the packet's own list
---------------------------------------------------------------------------

## `fn the_recovery_order_performs_every_step_the_packet_names() {`

**The recovery order performs every step `recovery_order` names.**

This is the test that did not exist while step (g) did not exist. For the
whole of PR7's implementation and two review rounds, `run_recovery_order`
performed nine of the ten steps it owns, with all 117 named tests passing,
every gate green on three platforms, and its own doc comment claiming
"steps (a) through (h)". Nothing could see it: a mutation catalogue measures
whether existing code is pinned, and **omission has nothing to mutate**.

So the assertion is against [`RecoveryStep::ALL`], which is the packet's
sentence transcribed into a type, and not against a second list written from
the implementation. Two steps are excluded **by name and with a reason**
rather than by being quietly absent — see [`RecoveryStep::performer`].

## `fn the_recovery_order_performs_every_step_the_packet_names()` › `let mut performed = recovered.steps.clone();`

Completeness first, and it is the half this test exists for: every step
the packet gives this order was performed, exactly once. Sorted, so a
step that moved for a stated reason cannot fail the completeness claim —
that is the next assertion's subject and it is a different question.

## `fn the_recovery_order_performs_every_step_the_packet_names()` › `let packet_order: Vec<RecoveryStep> = owed`

Then the order, for every step whose position the packet alone decides.
`(f)` is excluded **by a named live clause**, not by being skipped: see
`RecoveryStep::position_override`.

## `fn the_recovery_order_performs_every_step_the_packet_names()` › `let at = |step: RecoveryStep| {`

And the one that does move, moved for its reason and not by accident:
**(f) had two halves and now has one.** Its refusing half — the
unresolved integration transaction, one of the two things
`checkpoint_refusals` authorises — still runs before any append. Its
converging half, which appended a rebuilt `candidate_prepared` for a
settled-but-unrecorded candidate, is deleted with erratum E6's window:
since the 2026-08-27 CONFORM ruling `Promoting` is set only in the block
that records the candidate, so a `Promoting` generation without one
cannot occur and the walk could only ever have returned nothing.

What survives at (f) is `finish_promotions`, which still appends —
`T-CAND-REF`'s four-step sequence for candidates that *are* recorded —
so the step keeps its position among the appending steps and is still
what `steps` records here. The ordering this asserts is unchanged; the
reason given for it was not.

## `fn the_transcribed_recovery_steps_are_the_packets_eleven() {`

The transcribed list is the packet's list — eleven steps, these labels, in
this order.

The companion to the test above, and it guards the *other* direction. That
one proves the implementation covers [`RecoveryStep::ALL`]; this one proves
`ALL` is still the packet's sentence, because a variant deleted from `ALL`
would make the first test pass by asking for less.

## `fn resume_recreates_an_open_no_attempt_worktree_at_its_base() {`

---------------------------------------------------------------------------
(g) — recreate `OpenNoAttempt` worktrees at their bases
---------------------------------------------------------------------------

## `fn resume_recreates_an_open_no_attempt_worktree_at_its_base() {`

**The step does work, and the work is a worktree at the recorded base.**

The companion to `the_recovery_order_performs_every_step_the_packet_names`,
and it is the half that test cannot give: over a healthy fixture (g) runs
and finds nothing, so "the step ran" and "the step is a no-op" are the same
observation. This fixture leaves the one state (g) exists for — a generation
dispatched and never attempted, which is what a crash between
`task_dispatched` and `attempt_started` leaves.

## `fn an_inherited_lease_on_an_ordinary_task_is_refused_at_the_barrier_before_step_g() {`

An inherited lease on an ordinary task is refused by the fold at the
barrier's checked replay, before (g) sees anything — the consistency rule
that keeps a `task_dispatched` from claiming a lineage its entry does not
descend from. Until PR9 this test also stood for "a repair generation
cannot reach (g)"; repairs now reach it, through entries `merge_rejected`
registers, and their path is
`a_repair_dispatch_interrupted_before_its_attempt_is_recreated_at_its_base_and_materialized_once`.

## `fn an_inherited_lease_on_an_ordinary_task_is_refused_at_the_barrier_before_step_g()` › `let repair = {`

The fold refuses an inherited lease on an ordinary task, at the barrier's
checked replay — so the event never becomes fold state at all.

## `fn an_inherited_lease_on_an_ordinary_task_is_refused_at_the_barrier_before_step_g()` › `let registry = TaskRegistry::originals_with_agents(`

And no *original* entry could carry one legally: `originals_with_agents`
gives every entry `lineage: None`; lineage members enter the registry only
through `merge_rejected`.

## `fn the_recovery_order_hands_the_run_on_rather_than_dropping_it() {`

**The recovery order hands its state on rather than dropping it.**

This is the assertion that did not exist while `TopologyRun` did not exist.
`run_resumed` consumed the last witness and returned a two-field summary, so
the append handle `(a1)` had just proved, the fold built from exactly those
bytes, and both locks were destroyed at the end of the order. A loop cannot
be written against a function that ends by throwing the run away — so the
missing driver was not only a missing function, it was a missing *value*.

What is asserted is that the three survive and are the *same* three, not
replacements: the log still appends to the proven prefix, the fold is the
one the barrier replayed, and the locks are still held.

## `fn the_recovery_order_hands_the_run_on_rather_than_dropping_it() {` › `let contested = rundir::RunLock::acquire(&rundir::public_dir(&fixture.repo_root, RUN_ID));`

The run lock is still held, which is the property that lets a loop run
at all. Measured by asking for it: a second acquisition must be refused
while the handle is alive.

## `fn the_recovery_order_hands_the_run_on_rather_than_dropping_it() {` › `drop(handle);`

And released when the handle dies, in declaration order.

## `fn the_driver_takes_over_from_the_recovery_order_and_steps() {`

---------------------------------------------------------------------------
The driver, taking over from the order
---------------------------------------------------------------------------

## `fn the_driver_takes_over_from_the_recovery_order_and_steps() {`

**`TopologyRun` drives a resumed run, and `Step` finally has a consumer.**

This test lives here rather than beside `run.rs` because the only thing that
produces a real [`RunHandle`] is a real recovery, and the fixture for that
is this file's. Duplicating it there to keep the test adjacent to its
subject would be a second fixture for one state — the duplication shape this
slice has paid for four times.

What it asserts is the seam that did not exist: the order hands the run on,
the driver takes it, and one iteration of `loop` selects a branch and acts.
Before `RunHandle`, there was no value to hand over; before `run.rs`,
nothing outside `select.rs` so much as matched on a `Step`.

## `fn the_driver_takes_over_from_the_recovery_order_and_steps()` › `let plans = crate::engine::assembly::FrozenPlans {`

**Through the production assembler, not a fixture plan shape.** The
condition on this extraction was that the scaffold be re-pointed at the
real one or round-tripped against it; a fixture that hand-built an
`AttemptPlan` here would be exactly the fifth copy the `frozen_binding`
precedent warns about.

## `fn the_driver_takes_over_from_the_recovery_order_and_steps()` › `let Progress::Settled {`

**The driver ran an attempt.** Named exactly, not with a `matches!` that
would pass whichever branch the fixture happened to reach — a fixture
that silently started reaching a different one would take the assertion
with it.

## `fn the_driver_takes_over_from_the_recovery_order_and_steps()` › `assert!(`

**Not accepted, and the reason is the contract's.** This fixture's runner
answers every request with `exit 0` and never touches the worktree, so
the capture's tree is the base's and the diff is empty.
`pr_sequence[8].slice_contract.expected_failures_refusals` names
"empty-diff and unresolved-index attempt failures" as this slice's, and
this is the driver reaching one.

It asserted `accepted` before the ladder's cheap rungs were wired, and
passed: `judge` starts at gates, the plan configures none, and nothing
had asked what the diff contained. A driver that accepted this would have
pinned a candidate whose commit is its own parent.

## `fn the_driver_takes_over_from_the_recovery_order_and_steps()` › `assert_eq!(`

The dispatch AND the attempt are real and durable, in that order. Both
went through the production emitter, which is what makes them subject to
the append-error protocol; the scaffold's emitter re-implements the
append and runs none of it.

## `fn the_driver_takes_over_from_the_recovery_order_and_steps()` › `assert!(`

**The allowance, from `ladder::spends_allowance` and nowhere else.** An
empty diff spends: the line is "the worker ran", not "a verdict was
reached". The settlement carries the answer out of the branch because it
is the input the *next* ladder decision reads.

## `fn the_driver_takes_over_from_the_recovery_order_and_steps()` › `assert!(`

**The worker ran through the Runner**, which is what makes this the
fourth clause rather than a plan that was built and dropped. The whole
point of the driver is that something calls the machinery.

## `fn the_driver_takes_over_from_the_recovery_order_and_steps()` › `let recorded = TopologyFold::parse_log(&fixture.log_bytes())`

**The recorded region is the fold's, not a second derivation.**
`dispatch_lease_check` admits this task by computing the region and
asking the lease table what it overlaps; the log then holds whatever the
dispatch recorded, and the lease table keeps the log's. Two derivations
means the fold admits on one answer and the run is protected by another.

The fixture's hint is a glob (`src/alpha/*.rs`), which is what makes this
assertion able to fail: the fold strips it to the literal prefix
`src/alpha`, and a driver taking hints literally would record a prefix
that overlaps nothing. Measured — that shipped, for one commit.

## `fn the_driver_takes_over_from_the_recovery_order_and_steps()` › `assert_eq!(`

And the provisional reservation did not leak. O24 converts it AT the
append; a refusal after that must not leave an entitlement held, or the
next selection at width 1 sees a full pipeline forever.

## `fn the_driver_carries_an_accepted_attempt_through_the_candidate_sequence() {`

**The driver carries an accepted attempt through the whole candidate
sequence.**

The companion to `the_driver_takes_over_from_the_recovery_order_and_steps`,
which is the rejection case: there the fixture's worker edits nothing and
the ladder's cheap rungs stop it at the empty diff. Here it leaves a change
behind, so nothing rejects and the branch runs the sequence
`side_effect_vs_event_ordering` specifies — commit object, pin, settlement,
`candidate_prepared`, candidates ref, `task_candidate_created`, then the pin
prune and the forced scrub.

Two tests rather than one parameterised over a flag: the two paths append
different events in different orders, and a grid would assert the union of
them.

## `fn the_driver_carries_an_accepted_attempt_through_the_candidate_sequence() {` › `let plans = crate::engine::assembly::FrozenPlans {`

**Through the production assembler, not a fixture plan shape.** The
condition on this extraction was that the scaffold be re-pointed at the
real one or round-tripped against it; a fixture that hand-built an
`AttemptPlan` here would be exactly the fifth copy the `frozen_binding`
precedent warns about.

## `fn the_driver_carries_an_accepted_attempt_through_the_candidate_sequence() {` › `assert_eq!(`

**The settlement *is* `candidate_prepared`, so there are six events and
not seven.** This comment required an `attempt_finished` between the pin
and `candidate_prepared`, on the deleted settle_succeeded's own note that
`INV-07`
was "about which event records the candidate, not about which event
settles the attempt". That reading is wrong and
`design/26_design_merge_queue_protocol.md` §26 had already
answered it: `attempt_finished` "is not also emitted for that attempt".
Ruled CONFORM 2026-08-27, and the count below is the assertion — a build
that re-introduced the pair puts a seventh kind back here.

## `fn the_driver_carries_an_accepted_attempt_through_the_candidate_sequence() {` › `assert_eq!(`

-----------------------------------------------------------------------
**The same clause over the EFFECTS, not only the events.**

`side_effect_vs_event_ordering`: "commit object (R27) before pin
(IdUnread between); **pin before `candidate_prepared`**; **candidates ref
after `candidate_prepared`** and before `task_candidate_created`". The
event list above cannot see any of that — it holds no refs and no objects.

`candidate::tests::pin_pruned_after_promotion` asserts exactly this, over
`candidate::promote`. The driver assembles the same steps from the three
split halves, and **no ordering assertion reached that composition**:
four `PR7-PIPELINE-*` catalogue mutations that reorder it — the pin moved
after `candidate_prepared`, the candidates ref moved before it, the commit
object moved to just after capture, the pin created before `commit-tree` —
were all green. One rule, two production compositions, one witness.

## `fn the_driver_carries_an_accepted_attempt_through_the_candidate_sequence() {` › `"Event.Append".to_owned(),`

task_dispatched and attempt_started: the branch's own prologue,
which this fixture drives in the same step.

## `fn the_driver_carries_an_accepted_attempt_through_the_candidate_sequence() {` › `"Event.Append".to_owned(),`

**candidate_prepared — one append here, not two.** This list
carried an `attempt_finished(succeeded)` above it; that event is
not emitted for a candidate-producing attempt
(`design/26_design_merge_queue_protocol.md` §26, ruled
CONFORM 2026-08-27), and the fold now refuses it. The count is
part of the ordering claim.

## `fn the_driver_carries_an_accepted_attempt_through_the_candidate_sequence() {` › `"Event.Append".to_owned(),`

task_candidate_created.

## `fn the_driver_carries_an_accepted_attempt_through_the_candidate_sequence() {` › `"Worktree.Remove".to_owned(),`

O31's scrub, which `PR7-PIPELINE-029` moved to immediately after
`candidate_prepared` — three appends too early — and was green.

## `fn a_runs_spend_is_the_same_live_as_on_replay() {`

**A run's spend is the same live as it is on replay.**

The ground-truth invariant, pinned as a property rather than as a count. The
ceiling reads `Spend`; a live process keeps it current as it settles, and
every fresh process rebuilds it with `Spend::replay` from the log. If those
two disagree, a resumed run either refuses work it could afford or buys work
it could not, and neither shows up as a wrong number anywhere — it shows up
as a run that behaves differently after a restart.

**Why this class, not this instance.** Both `attempt_finished` and
`candidate_prepared` carry an `AttemptRecord`, and for a successful attempt
the driver appends both. `Spend::replay` counted each occurrence, so replay
priced every success twice while live priced it once. Asserting a corrected
number would have fixed the instance; asserting **live == replay over the
run's own log** kills the class, including the next event kind that carries
a record.

## `fn a_runs_spend_is_the_same_live_as_on_replay()` › `let plans = crate::engine::assembly::FrozenPlans {`

**Through the production assembler, not a fixture plan shape.** The
condition on this extraction was that the scaffold be re-pointed at the
real one or round-tripped against it; a fixture that hand-built an
`AttemptPlan` here would be exactly the fifth copy the `frozen_binding`
precedent warns about.

## `fn a_runs_spend_is_the_same_live_as_on_replay()` › `let live = run.spend().run_total();`

What the process believes it has spent, after settling one success.

## `fn a_runs_spend_is_the_same_live_as_on_replay()` › `let events = TopologyFold::parse_log(&fixture.log_bytes()).expect("the log parses");`

What any fresh process would believe, from the same bytes.

## `fn the_driver_settles_an_outage_from_the_folds_deferral_count() {`

**The driver settles an outage from the fold's deferral count.**

The witness that closes the mutation named in `deferrals_recorded`'s own
doc. The fold-level witness
(`fold::tests::a_deferral_count_is_derived_by_replay_and_not_by_a_process_local_tally`)
covers the *accumulation*; this covers the **read**, which is load-bearing
on exactly one branch and so needed a fixture that reaches it.

The chain is the one the ladder specifies: an agent whose CLI reports a rate
limit -> `evaluate_outcome` maps it to `FailureKind::RateLimited` ->
`is_outage` recognises it -> `next_step` defers rather than blaming the
implementer -> `settle_failed` records `Deferred`.

**The prior deferral is what makes the read load-bearing.** The fixture's
log already holds one, so the settlement must record `defers: 2`. Without
it, a driver reading a constant zero would record `1` and be
indistinguishable from a correct one — which is precisely why the mutation
survived before this test existed.

## `fn the_driver_settles_an_outage_from_the_folds_deferral_count() {` › `let fixture = Fixture::build(`

One deferral already in the log, and the resume wakes the task back to
`Pending` so the driver can dispatch it again.

## `fn the_driver_settles_an_outage_from_the_folds_deferral_count() {` › `let plans = crate::engine::assembly::FrozenPlans {`

**Through the production assembler, not a fixture plan shape.** The
condition on this extraction was that the scaffold be re-pointed at the
real one or round-tripped against it; a fixture that hand-built an
`AttemptPlan` here would be exactly the fifth copy the `frozen_binding`
precedent warns about.

## `fn the_driver_settles_an_outage_from_the_folds_deferral_count() {` › `assert!(`

**An outage spends no allowance.** `next_step` defers precisely so that
"retrying would burn attempts on a run that never got a verdict" does not
happen, and `spends_allowance` prices it the same way.

## `fn the_driver_settles_an_outage_from_the_folds_deferral_count() {` › `let settlements: Vec<u32> = TopologyFold::parse_log(&fixture.log_bytes())`

**The count came from the fold**, not from a tally this process kept.
One deferral was already durable, so the settlement records two.

## `fn the_driver_parks_an_attempt_with_the_question_it_raised() {`

**The driver parks an attempt, and the question it raises is durable.**

The last case of the ready-dispatch branch. An agent that stops and asks has
not failed at anything — `evaluate_outcome` reads `UPSTROKE-QUESTION:` out
of the outcome before the evidence rules, precisely so that an agent is not
punished for the empty diff its own question explains — so the chain is
`NeedsHuman` -> `Next::AskHuman(Clarify)` -> a parking settlement.

**`settle_failed` refuses a park that carries no question**, so reaching a
durable settlement at all is half the assertion. The other half is that the
question is the one the legacy engine would have asked: its context comes
from `coordinator::question_context` and its options from
`coordinator::question_options`, and this test reads both back out of the
log rather than out of the builder.

## `fn the_driver_parks_an_attempt_with_the_question_it_raised()` › `let plans = crate::engine::assembly::FrozenPlans {`

**Through the production assembler, not a fixture plan shape.** The
condition on this extraction was that the scaffold be re-pointed at the
real one or round-tripped against it; a fixture that hand-built an
`AttemptPlan` here would be exactly the fifth copy the `frozen_binding`
precedent warns about.

## `fn the_driver_parks_an_attempt_with_the_question_it_raised()` › `assert!(`

**A park spends no allowance.** "The code was never judged, so nothing is
spent and nothing escalates" — `next_step`'s own words, and the cell that
was wrong when the settlement derived the allowance from `Next` instead
of from the failure.

## `fn the_driver_parks_an_attempt_with_the_question_it_raised()` › `let parked = TopologyFold::parse_log(&fixture.log_bytes())`

The settlement is durable and carries its question.

## `fn the_driver_parks_an_attempt_with_the_question_it_raised()` › `assert!(`

**The words are the legacy authorities', not the driver's.** The context
quotes the agent as data and names the task; the options are what
`question_options` gives a `Clarify`. A driver that worded its own would
pass every assertion above and fail these.

## `fn the_driver_refuses_a_tree_a_filter_has_transformed() {`

**The driver refuses a tree whose bytes a gate would not see.**

The ladder's third cheap rung, and the one that was owed longest.
`Workspace::review_input_problem_for_tree` refuses staged evidence a
clean/smudge filter has transformed, or a worktree still holding unstaged or
dirty nested state — either makes the reviewed diff describe something other
than what the gates run against.

The worker here leaves a real edit **and** a `.gitattributes` naming a
filter, so the diff is non-empty (the first two rungs pass) and the tree is
still unreviewable. Without this rung the attempt would be accepted and a
candidate pinned from a transformed blob.

## `fn the_driver_refuses_a_tree_a_filter_has_transformed()` › `let plans = crate::engine::assembly::FrozenPlans {`

**Through the production assembler, not a fixture plan shape.** The
condition on this extraction was that the scaffold be re-pointed at the
real one or round-tripped against it; a fixture that hand-built an
`AttemptPlan` here would be exactly the fifth copy the `frozen_binding`
precedent warns about.

## `fn the_driver_refuses_a_tree_a_filter_has_transformed()` › `let failure = TopologyFold::parse_log(&fixture.log_bytes())`

**The refusal is the policy's own words, attributed to the reviewer.**
`classify::review_input_failure` is the one place that decides what an
unreviewable tree means for the attempt, and the message is
`Workspace`'s, not a driver paraphrase.

## `fn the_retaining_incarnation_retries_in_place() {`

**The retaining incarnation takes its next attempt in place.**

The ready-retry branch, end to end and in two iterations of the loop.

The first settles `Retained`: the agent reports its own error, which is
neither an outage nor a question, so `next_step` retries on the same rung —
and `resume: true`, because pre-flight probed the agent as
`session_resume` and the attempt returned a session. **Both halves are
required**, which is why the caps are given here and were empty everywhere
else: with either missing the generation closes and the task retries from a
fresh one instead.

The second is the retry itself: `{pipeline}` reservation, `Worktree.Verify`
against the retained tree, `attempt_started(retry)` carrying the session,
then the attempt and its settlement.

`Quiescence::HoldsTree` is the reason this needs a real worktree: a retry
verified against the base would pass on a tree that had been reset and would
re-gate an empty one as if it were the retained work.

## `fn the_retaining_incarnation_retries_in_place()` › `const RETRY_POOL: &str = "the-retrying-agents-pool";`

The pool this fixture's agent resolves to. Named rather than empty so
that "took the pool from the authority" and "took nothing" are different
observations.

## `fn the_retaining_incarnation_retries_in_place()` › `let pools = vec![crate::capacity::Pool::discovered(`

**Through the production assembler, not a fixture plan shape.** The
condition on this extraction was that the scaffold be re-pointed at the
real one or round-tripped against it; a fixture that hand-built an
`AttemptPlan` here would be exactly the fifth copy the `frozen_binding`
precedent warns about.
**A pool the implementer's agent resolves to**, because the two
`attempt_started` appends below are asserted to carry it. With `pools:
&[]` — what this fixture had — `AttemptPlans::pool_for` returns `None`
for every agent, so the retry arm's `pool: None` and its repair are
indistinguishable, and `run.rs` passing a literal `None` left the whole
suite green. Measured, twice: once as `R3-SEAMS-001` and once when round
4 restored the literal.

## `fn the_retaining_incarnation_retries_in_place()` › `let retained = TopologyFold::parse_log(&fixture.log_bytes())`

The generation is retained, not closed: only a retained one is retried in
place, and `settle::retry` refuses any other class by name.

## `fn the_retaining_incarnation_retries_in_place()` › `let starts: Vec<(u32, bool, Option<String>)> = TopologyFold::parse_log(&fixture.log_bytes())`

**Two attempts in one generation, the second resuming the first.** A
driver that opened a fresh generation would append `task_dispatched`
again; a driver that lost the session would append `attempt_started`
with none.

**And the pool each attempt drained**, which is the field `R3-SEAMS-001`
was about and the one no test held. The dispatch arm reads `plan.pool`;
the retry arm appends before its plan exists and takes the same answer
from `AttemptPlans::pool_for` one step earlier. Both are asserted here, in
one run, against the pool the assembler actually resolves — which is the
behavioural witness `79cd9c8` said was unavailable because "no driver
fixture can reach the arm". This fixture reaches it, and reached it then.
`reviews/FINDINGS.md` §19, claims (2) and (3).

## `fn the_retaining_incarnation_retries_in_place()` › `let briefed = runner`

**And told what went wrong.** §11.4 sends the failure back to the same
rung; the plan hard-coded `retry: None`, so the second attempt got the
first attempt's prompt verbatim and no reason to behave differently. A
retry that is not informed is a rung's allowance spent to learn nothing.

## `fn the_retaining_incarnation_retries_in_place()` › `assert!(`

Balance, which says every registration was settled. It does **not** say
the reviewers were registered — an empty ledger balances too — so R4's
review coverage is asserted where reviewers actually run, in
`attempt::tests`.

## `fn the_retaining_incarnation_retries_in_place()` › `let resumed = runner`

**The worker was actually told to resume.** The event records that a
session was retained; this records that the command carried it. They are
different claims, and a retry that appended the first without the second
would re-implement the task from scratch on a worktree that already holds
its previous work.

## `fn a_refused_step_leaves_no_entitlement_held() {`

**A step that refused holds no entitlement afterwards.**

`permits.protocol` is "every Runner process registered exactly once, settled
exactly once", and `append_error_protocol`'s obligation (2) is
`Reservations::cancel_any` on any outcome-unknown path. Three catalogue
entries take an entitlement **before** the step that can refuse and leak it
on the refusing path — `PR7-PIPELINE-014` (the `Dispatch` take moved into the
`Ok` arm), `PR7-SELECT-024` (a `Retry` reservation taken before `select`),
`PR7-SELECT-033` (an `Integration` pair taken before `checkpoint` refuses) —
and all three were green, because nothing asked the ledger anything after a
refusal.

The budget breach is the refusal this drives, because it is the one a fixture
can reach without arming an injection: seed a settled attempt that cost
something, resume, and set a ceiling below it. That the spend is visible at
all is itself new — `Spend::replay` had no production caller until `6d3fc6f`.

## `fn a_refused_step_leaves_no_entitlement_held()` › `let mut run = TopologyRun::resumed(`

A ceiling the log's own spend has already passed.

## `fn a_retried_worker_is_told_what_the_last_attempt_failed_on() {`

**A retried worker is told what the last one failed on.**

§11.4, quoted on `PlanRequest::feedback` itself: "failure feedback goes back
to the same rung, and an escalation carries the accumulated feedback with
it."

The driver accumulated the brief inside [`Retained`], which `settle::retry`
produces **only** for a resumable same-rung retry that returned a session.
Every escalation and every sessionless retry — which is every Copilot
attempt, `DESIGN.md:452` — therefore dispatched with `feedback: Vec::new()`
and handed the next worker attempt 1's prompt verbatim: a rung's allowance
spent to be told nothing. Found by round 2's `contract`, `seams` and
`attempt` lenses independently.

This drives **two** attempts through the real assembler and asserts on the
second worker's own stdin, because the prompt is the only place the claim is
observable — a brief the driver holds and does not send is the defect.

## `fn a_retried_worker_is_told_what_the_last_attempt_failed_on() {` › `run.step(&seams, &mut hooks)`

Attempt one fails and, with one attempt per rung, escalates onto rung 1.

## `fn a_retried_worker_is_told_what_the_last_attempt_failed_on() {` › `run.step(&seams, &mut hooks)`

Attempt two, on the rung above, from a fresh generation.

## `fn a_retried_worker_is_told_what_the_last_attempt_failed_on() {` › `let settled: Vec<serde_json::Value> = TopologyFold::parse_log(&fixture.log_bytes())`

**And the feedback is on the durable record, because this engine has no
other carrier.** The crash-resume witnesses seed a log directly, so they
assert what a resume does with a `detail` that is already there and
cannot tell whether a *live* schema-4 settlement writes one. Nothing did,
until this: `classify::FeedbackCarrier` is a per-caller choice, and
pointing the driver at `LadderEvent` — the legacy answer — left every
test in this file green while making the brief empty again for every run
that actually crashes. Measured 2026-08-26.

## `struct Timeline(Arc<Mutex<Vec<String>>>);`

The order the funnels ran in, for the **driver's** composition of the
candidate sequence.

`candidate.rs` has one of these and it covers `candidate::promote`. The
driver assembles the same four steps from the three split halves
(`create_candidates_ref`, `append_candidate_created`,
`reclaim_after_creation`), and until this existed **no ordering assertion
reached that composition** — the only two `trace.order(` calls in the
topology engine were both in `candidate.rs`. Found by S5 round 2's catalogue:
four `PR7-PIPELINE-*` mutations that reorder the driver's sequence were green,
while `pin_pruned_after_promotion` would have caught every one of them on the
other path.

## `fn push(&self, site: EffectSiteId, phase: HookPhase)` › `if phase != HookPhase::Before {`

`Before` only: one entry per funnel, at the point it begins, which is
what an ordering clause is about.

## `impl Timeline` › `fn order(&self, of_interest: &[EffectSiteId]) -> Vec<String> {`

The recorded sequence with everything not `of_interest` dropped.

Filtered rather than compared whole: a driver step also creates an
execution root, writes an intent and adds a worktree, and an assertion
over the unfiltered list would be an assertion about the fixture.

## `impl crate::workspace_manager::EffectHooks for TracedEffects` › `fn refusal_cause(&self) -> Option<String> {`

Forwarded, so a poison the inner observer found is reported as poison
and not as a fault this double armed.

## `struct TracedHooks {`

[`HarnessTopologyHooks`] with the two families an ordering clause spans
recorded onto one timeline.

## `fn the_driver_escalates_onto_the_rung_above() {`

**The driver writes the rung an escalation climbs ONTO, not the one it leaves.**

The **write** side of `PR7-FOLD-LADDER-POSITION`'s class, and the exact
mirror of the read-side gap closed at `6d3fc6f`. That repair witnessed the
driver *reading* `ladder_position`; nothing witnessed the driver *writing*
the escalation target, and `PR7-R3-ESCALATED-RUNG-WRITER-UNPINNED` is what
got through: replacing `rung: position.0.saturating_add(1)` with
`rung: position.0` left the whole suite green.

The consequence of that mutation is not a wrong number in a record. The fold
assigns `task.rung = *rung` and resets the allowance, so the task escalates
onto the rung it is **leaving**, `ready` selects it again, the binding
resolves, and it loops without bound — never reaching the tier its chain
escalated it to and never exhausting the chain.

**Written as the round trip, which is the class boundary.** Asserting the
recorded number alone would pin the write and leave the same gap one step
over. This drives the escalation, reads the durable settlement, and then
drives the *next* attempt and asserts the model it actually ran at — so the
value the driver wrote and the value it later reads are held by one test.

## `fn the_driver_escalates_onto_the_rung_above()` › `let fixture = Fixture::build(`

Two tiers, one attempt per rung: the first failure exhausts rung 0.

## `fn the_driver_escalates_onto_the_rung_above()` › `run.step(&seams, &mut hooks)`

Attempt one, on rung 0, fails and exhausts the rung's allowance.

## `fn the_driver_escalates_onto_the_rung_above()` › `run.step(&seams, &mut hooks)`

And the read side, in the same test: the next attempt runs at rung 1's
binding. Pinning the written number alone would leave the same gap one
step over, which is how this class keeps recurring.

## `fn the_driver_escalates_onto_the_rung_above()` › `run.step(&seams, &mut hooks)`

**And the third step exhausts the chain, which is where the human is
told a number.** This is the class boundary the frontier review of
`75da796` (finding 3) found unguarded: `park_question` hard-coded
`rungs_spent: 1` and passed *this rung's* attempts as the total, so a
two-rung exhaustion said "1 attempt(s) across 1 rung(s) all failed" when
two attempts across two rungs had. Nothing asserted it — this file's
two-rung test stopped at the escalated model, its count test drove a
single rung, and **no topology test asserted `rung(s)` at all**.

Asserted here rather than in a fixture of its own because the numbers are
only true of a task that has *actually* climbed: a single-rung fixture
reports 1 and 1 whether the code derives them or hard-codes them, which
is exactly how the constant survived.

## `fn the_driver_dispatches_at_the_rung_the_log_records() {`

**The driver dispatches at the rung the log records, not at rung 0.**

The **other** driver-side half of `PR7-FOLD-LADDER-POSITION`, and the one
that stayed open through the repair filed against it.
`the_driver_spends_the_allowance_the_log_records` witnesses the
`attempts_on_rung` half of the same reader; nothing witnessed `rung`, because
that fixture's chain has one tier and a one-tier chain makes rung 0 the only
rung there is. Measured at `cf22a8c`: replacing the driver's
`self.ladder_position(key)?.0` with a literal `0` failed **no** topology
test.

That is occurrence 4 of `reviews/FINDINGS.md` §4's accumulator class, and the
sharpest argument for its re-scoping: the class was filed *from* this
instance, and half of this instance was still open.

The fold half — `fold::tests::a_ladder_position_is_derived_by_replay_and_not_assumed`
— already states the consequence in words: "A driver that assumed rung 0
would dispatch an escalated task on rung 0 forever, never reaching the tier
its chain escalated it to." This asserts it.

## `fn the_driver_dispatches_at_the_rung_the_log_records()` › `let fixture = Fixture::build(`

A two-tier chain, one attempt per rung, and an attempt already escalated
off rung 0 in the durable log. The task is `Pending` on **rung 1**.

## `fn the_driver_dispatches_at_the_rung_the_log_records()` › `let ran_at = TopologyFold::parse_log(&fixture.log_bytes())`

**The model the attempt actually ran at.** The rung selects the binding,
and the two tiers of this chain differ by model — so this is the rung,
observed rather than asserted about itself.

## `fn a_crash_does_not_erase_what_the_last_attempt_was_told_to_fix() {`

**A crash does not erase what the last attempt was told to fix.**

§11.4's first half: "failure feedback (gate log or `required_changes`) goes
back to the *same rung*". The brief that carries it was a process-local
`BTreeMap` the live loop pushed to, and `TopologyRun::resumed` created it
**empty** — so the sequence the 2026-08-26 frontier review of `75da796` set
out in finding 2 held exactly: attempt 1 fails a gate with an 8-KiB
diagnostic tail, `attempt_finished` is durably appended, the conductor
crashes before the next dispatch, and the retry is handed attempt 1's prompt
verbatim. A rung's allowance spent to be told nothing, and the same defect
free to repeat.

The fixture is that crash: one attempt already settled in the durable log,
carrying the tail, and a chain with a second attempt left on the rung. The
process that wrote it is gone — this run is built by `resumed` from the log
alone, which is the only path a real resume has.

**Asserted on the worker's own stdin**, because the prompt is the only place
the claim is observable: a brief the driver rebuilds and does not send is the
same defect one rung further along. And asserted on the tail's exact text
rather than on the prompt's length — a longer prompt is evidence that
*something* was carried, which is what the live-mode witness
`a_retried_worker_is_told_what_the_last_attempt_failed_on` could say and is
not what §11.4 requires.

## `fn a_crash_does_not_erase_what_the_last_attempt_was_told_to_fix() {` › `const TAIL: &str = "error[E0308]: mismatched types\n  --> src/alpha.rs:12:9\n   \`

§11.1's payload: what the gate printed, not the summary of it.

## `fn a_crash_does_not_erase_what_the_last_attempt_was_told_to_fix() {` › `let fixture = Fixture::build(`

One tier, `attempts_per = 2`: the retry the log entitles this run to is on
the **same rung**, which is the half of §11.4 this test is about.

## `fn an_escalation_after_a_crash_carries_the_accumulated_feedback() {`

**An escalation after a crash carries the accumulated feedback.**

§11.4's second half: "`attempts_per` exhausted → next rung, fresh session,
**accumulated feedback summary included**", and its other named source — the
reviewer's `required_changes`, which §11.2 says the retry gets back verbatim.

Two failures are already durable on rung 0 and the ladder has a rung above,
so the only dispatch this run can make is the escalation. The empty brief a
resume used to rebuild sent it none of them: a fresh, stronger worker on the
same task, given attempt 1's prompt and no reason to do anything different.

**What "accumulated" means here is what `feedback_section` actually sends**,
and this asserts that rather than a stronger reading of the sentence. Every
earlier attempt contributes its summary line; only the newest carries its
full detail, because "older ones would bury it, and the newest is the one
still standing in the way" — the production comment on that decision. So the
claim under test is: both summaries reach the rung above, the newest
reviewer's required changes reach it verbatim, and the accumulated section
exists at all — `feedback_section` writes its header **only** when it is
rendering more than one entry for a fresh rung, so that sentence is the
accumulation itself and not a statement about it.

## `fn an_escalation_after_a_crash_carries_the_accumulated_feedback() {` › `in_generation(GenerationId(1), dispatched()),`

A closed generation cannot take another attempt, so the
second failure is in the generation the retry opened —
which is exactly what a sessionless retry leaves in a log.

## `fn a_log_predating_the_detail_field_folds_and_resumes() {`

**A log written before the field existed still folds, and still resumes.**

`FailureRecord::detail` is additive and `SCHEMA_VERSION` does not move, which
is a claim about *older logs* rather than about new ones:
`decisions/2026-08-26-durable-retry-feedback.md` argues that a line without
the key reads back as `None`, folds unchanged, and passes schema 4's strict
door — the door being a witness comparison that reports "any key the input
carried that the record did not claim back", so an added output key is not an
unknown input key.

That argument is worth exactly as much as a log that tests it. This deletes
the key from every `attempt_finished` in a real fixture's bytes — the shape a
binary one commit older wrote — and resumes from the result through the
production parse.

## `fn a_log_predating_the_detail_field_folds_and_resumes()` › `let current = String::from_utf8(fixture.log_bytes()).expect("the log is utf-8");`

The older binary's bytes: the same log with the key it never wrote.

## `fn a_log_predating_the_detail_field_folds_and_resumes()` › `if position == 0 {`

**The first line is passed through byte for byte.** The commit
record pins its sha256, and a re-serialization that only reorders
keys is enough to make recovery refuse for a reason that has
nothing to do with this field.

## `fn a_log_predating_the_detail_field_folds_and_resumes()` › `if let Some(failure) = value.pointer_mut("/data/record/failure") {`

Nested rather than a let-chain: MSRV is 1.85 and let-chains
are 1.88.

## `fn a_log_predating_the_detail_field_folds_and_resumes()` › `let harness = harness();`

And the run it describes still resumes. **What the brief holds for those
attempts is a line per failure carrying its `summary` and no `detail`** —
not an empty brief, which is what this comment used to claim and what the
2026-08-26 re-review of `c2c0294` corrected as finding C. `Brief::record`
adds an entry whenever the record carries a failure at all, and that is
right: a summary is what an older log preserved, and sending the next
worker "attempt 1 failed: gate `x` failed" beats sending it nothing.
Asserted below rather than described, because a comment about content is
the thing this slice keeps getting wrong.

## `fn a_log_predating_the_detail_field_folds_and_resumes()` › `crate::workspace_manager::fixture::write_file(&fixture.log(), aged.as_bytes());`

`std::fs::write` is on the effect denylist here; the fixture writer is
the sanctioned way a test plants bytes.

## `fn a_log_predating_the_detail_field_folds_and_resumes()` › `let brief = crate::engine::topology::run::Brief::replay(&handle.events);`

The brief that resume rebuilds from those bytes, asserted rather than
described: one line for the failure the older log recorded, carrying its
summary and no detail.

## `fn drive_one_attempt(fixture: &Fixture, runner: &RecordingRunner) -> Vec<String> {`

Resume the fixture from its log alone, take one step, and return the
implementer prompts the run actually sent.

The whole apparatus of the driver tests above, factored out because the two
crash-resume witnesses differ only in what their logs already hold and in
what they expect to come back. **Recovery is not stubbed**: this is
`resume_holding` into `TopologyRun::resumed`, the same pair every other
driver test in this file uses, so the brief under test is rebuilt from the
barrier's own parse of the durable bytes.

## `fn the_driver_spends_the_allowance_the_log_records() {`

**The driver spends the allowance the log records, not the one it assumed.**

The driver-level half of `PR7-FOLD-LADDER-POSITION`. The fold half is
`fold::tests::a_ladder_position_is_derived_by_replay_and_not_assumed`; this
is the read, and it needed a fixture that makes the read observable.

This fixture's chain has **one** tier and `attempts_per = 2`, so an
escalation has nowhere to climb and the allowance is the whole ladder. One
attempt is already durable in the log. The driver's next attempt is
therefore the **second** on that rung, which exhausts the allowance, and
`next_step` has no rung to escalate onto — so the task fails terminally.

A driver that assumed `attempts_on_rung: 1` would hand `next_step` the first
attempt of two and get `RetrySameRung` instead: the task would retry
forever, spending a rung's allowance on every restart and never failing.
That is what the constant did before this test existed.

## `fn the_driver_spends_the_allowance_the_log_records()` › `let fixture = Fixture::build(`

One attempt already spent on rung 0, settled as a same-rung retry so the
task returns to `Pending` and this branch selects it again.

## `fn the_driver_spends_the_allowance_the_log_records()` › `let plans = crate::engine::assembly::FrozenPlans {`

**Through the production assembler, not a fixture plan shape.** The
condition on this extraction was that the scaffold be re-pointed at the
real one or round-tripped against it; a fixture that hand-built an
`AttemptPlan` here would be exactly the fifth copy the `frozen_binding`
precedent warns about.

## `fn the_driver_spends_the_allowance_the_log_records()` › `let SettlementTransition::Parked { question } = transition else {`

**Parked, not failed** — and that is `next_step`'s answer, not a
weakening of the assertion. A spent chain asks a human rather than
failing the task: "Nothing further can move this task ... and the
escalation chain is spent." What matters here is that the allowance was
seen as spent at all.

## `fn the_driver_spends_the_allowance_the_log_records()` › `assert!(`

**And the human is told how many attempts actually ran.** The count in
the question is the task's spend on this rung, not the new generation's
attempt number — a park that said "1 attempt" after two would send an
operator looking for a run that had barely started.

## `fn the_loop_continues_an_attempt_recovery_recreated() {`

**The loop continues an attempt in a generation recovery recreated.**

`T-DISPATCH`'s `resume_action` in its own words: "verify the worktree at the
recorded base ... or remove it with force and recreate it ... **continue
attempt (no spend repeats)**".

Step (g) recreated those worktrees and nothing then started an attempt in
them. `fold::ready` excludes the task — correctly, since a task with an open
generation is not *ready to be dispatched* — and `ready_retry` wants
`RetainedIdle`, so no branch could select it. The run stalled with its only
pipeline entitlement held by a generation nothing could drive, and the loop
fell through to a closure it refuses.

`fold::open_no_attempt` identifies the generation for recovery. Selection
now uses `fold::eligible_continuation` to check whether it may start, and
`resume_open_no_attempt` — which had no production caller — is what reuses
the ground.

## `fn the_loop_continues_an_attempt_recovery_recreated()` › `let fixture = Fixture::build(`

Killed after `task_dispatched`, before `attempt_started`.

## `fn the_loop_continues_an_attempt_recovery_recreated()` › `let plans = crate::engine::assembly::FrozenPlans {`

**Through the production assembler, not a fixture plan shape.** The
condition on this extraction was that the scaffold be re-pointed at the
real one or round-tripped against it; a fixture that hand-built an
`AttemptPlan` here would be exactly the fifth copy the `frozen_binding`
precedent warns about.

## `fn the_loop_continues_an_attempt_recovery_recreated()` › `let after = durable_kinds(&fixture);`

**No spend repeats**, in `T-DISPATCH`'s own words: the generation was
already open, so continuing it appends an attempt and never a second
`task_dispatched`.

## `fn a_reviewer_runs_at_the_review_effort_not_the_implementers() {`

**A reviewer runs at §10's review effort, not the implementer's.**

`ResolvedEffortPolicy` has four axes and `review` is one of them: the tier a
rung binds decides what the *work* costs, and review has its own budget.
`FrozenPlans` passed `request.binding.effort` — the implementer's — while its
own comment said "the reviewer's effort, not the implementer's". A comment
asserting the opposite of its line is worse than none: it answers the
question a reader would otherwise ask.

This fixture's Mid rung is `High` and its review axis is `Medium`, so the two
are distinguishable. A fixture where they matched would assert nothing.

## `fn a_reviewer_runs_at_the_review_effort_not_the_implementers() {` › `use crate::engine::topology::scaffold::REVIEW_AGENT;`

**The reviewer is bound to a different agent than the implementer, and it
has to be.** The comment here used to say the single pool was "named so
that a plan inheriting the implementer's pool and one looking up the
reviewer's own could not both pass" — and the fixture's primary reviewer
is `(claude-code, opus)` while its rung-0 implementer is
`(claude-code, alpha-Mid-model)`. `review::passes_for` rebinds only on
**exact `(agent, model)` equality**, so it does not fire and the pass
keeps agent `claude-code`: the two lookups were the same lookup, both
behaviours passed, and the mutation recorded as killed died because the
pool became *empty* rather than wrong. `reviews/FINDINGS.md` §19,
claim (8).

With `REVIEW_AGENT` on the primary and a pool for each agent, inheriting
the implementer's pool yields `the-implementers-pool` and fails.

Through the scaffold's own constant rather than a literal: it is the
agent that fixture's `alternative` binding already names, so this is the
second agent the run actually probed and not one invented here.

## `fn a_reviewer_runs_at_the_review_effort_not_the_implementers() {` › `assert_eq!(`

The implementer's own pool, so the two values in play are distinguishable
and the assertion below is about which one the reviewer got.

## `fn a_reviewer_runs_at_the_review_effort_not_the_implementers() {` › `assert_eq!(`

**And its own agent's pool**, which is the other cell of
`a_reviewers_profile_is_accounted_for_at_both_callers` whose value the
extraction dropped. That census checks the roll is complete and cannot
check a value — a cell is prose. This is the value.

§11.3/§13: a cross-vendor second opinion draws on a different
subscription than the implementer, so the pool is looked up from the
reviewer's own agent. `coordinator.rs` did it and `assembly.rs` did
not, leaving `profile_for`'s empty string — so the capacity engine
attributed a reviewer's spend to a pool with no name. Sol's
independent `seams` read, round 3.

## `fn the_loop_inherits_the_committed_digest_recovery_verified() {`

**The loop inherits the digest recovery verified.**

`committed.json.run_started_sha256` is what step (a) checks the committed
first line against, and the append-error protocol reads it back: the creator
disposition is a projection of the outcome onto the run's *commitment*
boundary, and a run that cannot say whether it is committed cannot report
one.

Recovery's own emitter passes `Some(...)`. `TopologyRun::resumed` passed
`None` — so over one run, the two emitters disagreed about whether it was
committed, and only the loop's appends lost the answer. Nothing observed it
because nothing compared them.

## `fn the_loop_inherits_the_committed_digest_recovery_verified() {` › `let expected = crate::rundir::run_started_sha256(&fixture.first_line);`

Computed independently from the run's own committed first line, rather
than read back from the record the handle came through: a comparison of
the record with itself would pass however the digest was carried.

## `fn the_loop_inherits_the_committed_digest_recovery_verified() {` › `let run = crate::engine::topology::run::TopologyRun::resumed(`

**And it survives into the loop's own identity**, which is the hop that
matters: `establish_stable_prefix` skips its check entirely when the
digest is `None`, so a loop that carried the handle's value and then
dropped it would reopen after an append error and accept a first line the
commit record does not name. `PR31-CONTRACT-006`.

## `fn a_prepared_pin_without_a_candidate_record_is_orphan_residue() {`

**A prepared pin with no `candidate_prepared` is orphan residue, not a
candidate to reconstruct.**

This replaces three tests —
`a_resume_converges_a_settled_candidate_that_was_never_recorded`,
`a_second_resume_finishes_nothing_and_appends_nothing` and
`a_converged_log_prices_its_attempt_once` — which drove erratum **E6**'s
convergence: a `Promoting` generation with no recorded candidate, whose
`candidate_prepared` a resume rebuilt from whatever the prepared pin pointed
at, deriving tree, message and paths from that commit.

**They were witnesses for a window that no longer exists, and for a path that
was a defect.** `Promoting` was reachable by `attempt_finished{Succeeded}`
alone; since the 2026-08-27 CONFORM ruling `candidate_prepared` is the sole
successful settlement and is the only thing that sets `Promoting`, in the
same block that records the candidate. The `bf927f3` review's first P1 was
exactly this reconstruction: substitute the pin between the settlement and
the append and recovery builds a successful candidate around an object no
gate judged — and the tree check cannot catch it, because recovery itself
recorded that tree.

So the fixture is the same crash and the expectation is the other one: the
attempt was never settled, so it settles **interrupted**, and the pin is
residue that recovery prunes. Not patched to pass — the log this drives is
one the fold accepts, which the old fixture no longer is.

## `fn a_prepared_pin_without_a_candidate_record_is_orphan_residue() {` › `extra: vec![attempt_started(1)],`

An attempt that started and never settled: the whole of what a
crash between the pin and `candidate_prepared` leaves durable.

## `fn a_prepared_pin_without_a_candidate_record_is_orphan_residue() {` › `let prepared: Vec<_> = TopologyFold::parse_log(&fixture.log_bytes())`

**And no `candidate_prepared` was invented.** This is the assertion the
removed trio inverted: they required exactly one to appear.

## `fn seed_candidate_commit(fixture: &Fixture, generation: u32) -> String {`

Put a real candidate commit and its pin in the fixture's repository.

**Both halves are real because the residue is real.** This said the halves
had to be real so that erratum E6's convergence could reconstruct the
candidate's identity from the object the pin points at, and called the state
"what a run killed after its settlement leaves behind". Neither survives the
2026-08-27 CONFORM ruling: the convergence is deleted with the window it
converged, and a run killed *after* its settlement has appended
`candidate_prepared`, so it leaves a recorded candidate rather than a bare
pin.

What this seeds is the crash **before** the settlement — the commit object
and its pin written, R27's order, and the append that would have settled the
attempt never reached. A fixture that seeded only events would leave nothing
on disk for the pruning to be about, so the object and the pin are written
for real.

Returns the commit sha, so a test can assert what became of the object the
pin named.

## `fn seed_candidate_commit(fixture: &Fixture, generation: u32) -> String {` › `write_file(&repo.join("candidate.txt"), b"the worker's edit\n");`

A change on top of the base, committed without moving the branch: the
candidate commit is unreferenced except by its pin, which is R23's shape.

**One path, never `add -A`.** The run's own directory lives under
`.upstroke/` inside this repository, so `add -A` stages it and any
subsequent worktree restore deletes it — measured, as a resume that could
not find its own run.

## `fn seed_candidate_commit(fixture: &Fixture, generation: u32) -> String {` › `git(repo, &["rm", "-q", "-f", "--", "candidate.txt"]);`

Unstage and remove just that path, so the index matches the base again
and the pin is the only thing referencing the commit. Targeted for the
same reason the add was.

`git rm` rather than `std::fs::remove_file`: deletion is a funnel in this
crate and the effect denylist refuses the raw call even in a fixture,
which is the rule working rather than getting in the way.

## `struct FixedIds;`

An [`IdSource`] whose question id is a constant.

A park appends the id it minted, and `rematerialize_question` reads it back
on resume rather than re-deciding it — so a test that asserts on the durable
question needs the id to be the same bytes every run. `RealIds` gives a
ULID, which is right in production and unpinnable here.

## `fn durable_kinds(fixture: &Fixture) -> Vec<String> {`

The kinds in a fixture's durable log, in order.

## `struct RecordingSleeper {`

A sleeper that records rather than sleeps.

## `fn a_call_census_needle_is_not_satisfied_by_a_longer_name_ending_in_it() {`

**A call census's needle is not satisfied by a longer name ending in it.**

The class boundary, not the instance. S5 round 4 found that
`every_packet_named_recovery_action_has_a_production_caller` counted
`refuse_unexpected_refs(` as a call to `expected_refs` — but the interesting
half is that the same needle is built for **every** entry from a name the
packet chose, so any future clause whose function name is a suffix of another
identifier is satisfied by that other identifier's call sites, silently and
in the passing direction.

So this asserts the needle's rule over the four shapes that decide it, and
then over the real file the collision was found in — a unit assertion alone
would pass against a helper that was never wired into the census.

## `fn a_call_census_needle_is_not_satisfied_by_a_longer_name_ending_in_it() {` › `let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));`

And on the file the collision was measured in, through the same region
the census reads — a unit assertion over literals would pass against a
helper nothing was wired into.

**The two counts differ, and the difference is worth keeping.** The module
carries four occurrences of `expected_refs(`; the region the census reads
carries one, because three of the four sit inside `#[cfg(test)]` items
that `production_code` blanks. The one that survives is the **definition
line** of `refuse_unexpected_refs`, which the "calls, not definitions"
filter does not catch: the text before the match is `pub fn refuse_un`,
and that does not end in `fn`.

The three that `production_code` blanks now sit in the sibling file
`mod tests;` declares, so the module is two files and `whole` is read
from both. The comparison is the one it always was: every occurrence the
module carries, against the one the production region keeps.

## `fn every_packet_named_recovery_action_has_a_production_caller() {`

**Every packet-named recovery action and refusal has a production caller.**

The class this slice produced more than any other, and the one census that
closes it. Across three review rounds, ten separate things were found
**built, correct, and never called**: `TopologyRun` itself, `settle_*`, the
candidate sequence, `resume_open_no_attempt`, `Started`, `CandidateJournal`,
`Spend::replay`, `complete_promotion`'s continuation, `prune_orphan_pin` and
`refuse_unexpected_refs`.

Two of those were P0/P1 liveness defects — a converged promotion that stalled
the run forever, and a resumed run that forgot its whole spend. The rest were
coverage gaps that would have become defects the moment a caller appeared.
Each was found separately, by a different reviewer noticing a different
symptom, over four rounds.

**This asserts the property the packet states, rather than waiting for a
reviewer to notice its absence.** A function that implements a
`resume_action` or a `refusal_condition` and has no production caller is not
an implementation of that clause — it is a plan to implement it.

**What this census covers, exactly.** The eleven entries below and nothing
else. Of the ten never-called things listed above, `Spend::replay`,
`TopologyRun`, `Started`, `CandidateJournal`, `settle_*` and
`complete_promotion`'s continuation are **not** among them — this would not
have caught them, and the commit that added it said otherwise. Corrected in
`reviews/FINDINGS.md` §19, claim (7); recorded here because the reader who
needs it is the one adding the twelfth entry.

Four ways this could pass while a clause stayed unperformed, and each is
closed by a named thing rather than by the needle being "obviously right":

* **A mention in a doc comment or a string.** The region is
  `effects::production_code`, which blanks comments and string literals.
* **A `#[cfg(test)]` caller in the same file.** The same region removes each
  configured item in place.
* **A caller in an out-of-line `tests.rs`**, where the attribute is on the
  parent's declaration and there is nothing in the file to blank. Skipped by
  file stem in the walk below. This was live until S5 round 4.
* **A longer identifier ending in the entry's name.** `expected_refs(` was
  satisfied by `refuse_unexpected_refs(`. Closed by [`crate::effects::census_domain::production_calls`],
  whose own witness is
  `a_call_census_needle_is_not_satisfied_by_a_longer_name_ending_in_it`.

The fourth is the one worth stating as a class: the needle is built from a
name **the packet chose**, so it cannot be renamed out of a collision the way
`into_log_and_fold` was.

## `fn every_packet_named_recovery_action_has_a_production_caller() {` › `const CLAUSES: &[(&str, crate::effects::census_domain::Call, &str)] = &[`

(function, how production calls it, the packet clause it performs).

## `fn every_packet_named_recovery_action_has_a_production_caller() {` › `let test_modules = crate::effects::census_domain::whole_file_test_modules(&root, &all, 13);`

**The crate's own declarations, not a file-name rule.** This skipped
by the stem `"tests"`, so it covered only the modules named
`tests.rs`; the crate declares **more** whole-file test modules than
that — `effects::tests::cfg::WHOLE_FILE_TEST_MODULES` lists them all,
against the `tests.rs` entries of it the stem finds —
and the six it missed — `scaffold`, `premove`, `fake`, `fixture`,
`scratch_tree`, `readiness` — are the ones most likely to name what
production names. `PR7-R5-ATT-001`.

## `fn every_packet_named_recovery_action_has_a_production_caller() {` › `if test_modules.contains(&path) {`

**An out-of-line test file is test code in full, and
`production_code` cannot tell.** The `#[cfg(test)]` is on the
*declaration* in the parent, so the file it names carries no
attribute of its own and nothing in it is blanked. Without
this skip a fixture calling a packet-named function satisfies
the clause on production's behalf, which is precisely the
class this census exists to close.

## `fn every_packet_named_recovery_action_has_a_production_caller() {` › `assert!(`

The skip is in force and it removed something. A zero here would mean the
control was silently inert — the same failure as an empty region, one
level up. The floor is the pinned list's length rather than a literal,
which is why it is that list and not `test_modules.len()`: the derivation
is what this floor exists to catch, so a floor read off its own output
would pass on an empty answer.

## `fn every_packet_named_recovery_action_has_a_production_caller() {` › `let defined: usize = sources`

**The named item exists.** The census never checked, so renaming a
clause's definition out of the tree left it green — measured, S5
round 4. Not pinned to exactly one definition, because
`settle_interrupted` legitimately names three items and `form` is
what separates them.

## `struct Damage {` › `alternative_reviewer: bool,`

The review plan names an alternative reviewer, so a candidate whose
implementer is the primary reviewer is reviewed by someone else.

## `struct Damage {` › `no_automatic_repairs: bool,`

`max_merge_repairs = 0`: the first rejection registers its repair with
human admission.

## `impl Fixture {` › `fn two_tasks(tag: &str) -> Self {`

A healthy two-task run: what every stale-verification fixture needs,
since only a publication moves the integration head past the base.

## `struct PlantedTransaction {`

--- integration-transaction recovery fixtures (PR8, step (f)) -------------

A planted integration transaction: the candidate it publishes and the
commit the fast path proposes.


## `fn append_events(fixture: &Fixture, bodies: &[TopologyEventBody]) {`

Append event bodies to the fixture's already-committed log, the way a live
run would have, so a resume folds them as part of its proven prefix.

## `fn alpha_commit(fixture: &Fixture) -> (CommitSha, CommitSha) {`

A commit for ALPHA on the fixture base, adding `candidate.txt`, left in the
object store with the worktree restored to the base.

## `fn obliged_reviews_for(fixture: &Fixture, key: TaskKey) -> Vec<crate::events::ReviewRecord> {`

The review passes ALPHA is frozen to require, as records that pass — read
off the registry the current log folds to, exactly as a live candidate's
would be.

## `fn alpha_candidate_prepared(`

The `candidate_prepared` for ALPHA's generation 0 at `commit`/`tree`.

## `fn candidate_prepared_for(`

The `candidate_prepared` of `key`'s generation 0 at `commit`/`tree`, whose
attempt edited exactly `path` on the fixture base.

## `fn plant_queued_candidate(fixture: &Fixture) -> PlantedTransaction {`

Plant a queued candidate for ALPHA on the base: its objects and refs, the
integration ref at the base, and the log through `task_candidate_created`.

## `fn plant_queued_candidate_events(fixture: &Fixture) -> PlantedTransaction {`

ALPHA's queued candidate on the base — its objects, refs and events —
leaving the integration ref wherever it is.

## `fn fast_prepared(fixture: &Fixture, planted: &PlantedTransaction) -> TopologyEventBody {`

The `merge_prepared(fast)` of sequence 0 for a planted candidate at the
base.

## `fn plant_prepared_fast(fixture: &Fixture) -> PlantedTransaction {`

Plant a fast integration transaction: the candidate objects and refs, the
integration ref at the base, and the log through `merge_prepared(fast)` with
no `task_merged` — the exact durable state a crash after the stable-prefix
barrier but before the compare-and-swap leaves.

## `fn a_resume_completes_a_prepared_fast_transaction_through_the_barrier_and_cas() {` › `let fixture = Fixture::healthy("finish-fast");`

The two-crash shape: `merge_prepared(fast)` is durable in the log (the
stable-prefix barrier held before the process died) and the integration
ref is still at the base (the compare-and-swap had not run when power was
lost). A resume must finish the one authorized publication.

## `fn resume_with_real_refs(`

Resume through the real `WorkspaceManager` as *both* ref interfaces — the
P7/P8 startup repair reads the same refs the transaction recovery moves.

The `RecordingRefs` double every other resume here supplies answers
"absent" to the startup repair whatever the repository holds, which is
exactly how a resume that refused its own published head stayed green
(`pr8-triage.md` C1). A publication test resumes through this.

## `fn a_resume_after_a_completed_publication_accepts_its_own_head() {` › `let fixture = Fixture::healthy("published-head");`

`transaction_fault_matrix[T-RESUME].durable_state` counts "CAS
completions" among what recovery continues from: once sequence 0 has
published, the integration ref legitimately names the proposal, not
`run_started.base_sha`, and the next resume must accept it rather than
refuse its own work as foreign history.

## `fn a_resume_after_a_completed_publication_accepts_its_own_head() {` › `drop(first);`

The first resumer exits, releasing its locks; the next incarnation must
accept the durable result of the publication it finds.

## `fn a_resume_after_a_publication_refuses_a_ref_that_disagrees_with_the_log() {` › `let fixture = Fixture::healthy("published-disagrees");`

DESIGN §26: "`task_merged` exists but the ref disagrees — refuse; the
log and integration branch no longer describe the same run". Neither a
ref moved elsewhere nor a deleted one is repaired from the base.

## `fn commit_on(`

A commit on `parent` that adds `file` with `content`, left in the object
store with the worktree restored.

## `fn plant_staging_intent(fixture: &Fixture, sequence: u32) {`

Write a staging intent for `sequence` through the manager, the record a live
stale cherry-pick would have left.

## `fn plant_staging_worktree(fixture: &Fixture, sequence: u32, head: &str) -> PathBuf {`

Write a staging intent for `sequence` and add its worktree at `head`
through the manager: the residue a live stale sequence leaves.

## `fn plant_snapshot(fixture: &Fixture, sequence: u64, commit: &str) -> PathBuf {`

Add the integration gate snapshot of `sequence` at `commit` through the
manager, as a verification that was killed mid-gate leaves it.

## `fn plant_stale_verification(`

Plant an interrupted stale-clean verification: BETA published fast at
sequence 0 (the only way the integration head moves), ALPHA's candidate on
the base and therefore stale, its cherry-pick proposal pinned under
`prepared/1`, the staging worktree at the proposal with its intent, and the
log through `merge_verification_started` for sequence 1 with no terminal —
the state a crash mid-verify leaves. Needs a two-task fixture.

## `fn plant_stale_verification(` › `let proposal = commit_on(`

The cherry-pick produced a proposal on the moved head.

## `fn append_event_hooked(`

Append one event through the hooked Event funnel, so an injection armed
on `harness` fires inside the append exactly as it would in a live run.

## `fn proven_durable_len(hooks: &HarnessTopologyHooks, fixture: &Fixture) -> u64 {`

The length the durability ledger proves durable for the fixture's log: the
last file length a sync reported, after which every write is unsynced.

## `fn lose_unsynced_writes(fixture: &Fixture, durable: u64) {`

A simulated power loss: every byte no sync proved durable is gone.

## `fn crash_with_unsynced_merge_prepared(`

The first crash of the two-crash proof: `merge_prepared(fast)` is written
to the log as one complete line and never synced — the append's flush was
made to fail after the full write — and the process ends under the
append-error protocol. Returns the hooks whose ledger recorded it.

## `struct ReportingHooks {`

A hook bundle for a child that will be killed: it forwards to the harness
bundle and writes, in order, every sync of the log file and every entry
into the integration compare-and-swap to a report file the parent reads
after the kill — the durability oracle carried across the process
boundary, since the child's ledger dies with it.

## `fn two_crash_kill_child() {` › `let repo_root = PathBuf::from(`

The restart of the two-crash proof, in a process of its own: the
barrier, the compare-and-swap, and then a kill at `Written` of the
task_merged append — the whole line in the file, nothing having synced
it — reported to the parent as it happens.

## `fn unsynced_merge_prepared_two_crash_barrier_before_cas_then_power_loss_keeps_log_and_ref_agreeing()` › `let fixture = Fixture::healthy("two-crash");`

`C.proof_tests[3]` and `[T-PREPARED].test`, the two-crash proof. A
complete but unsynced merge_prepared line; restart; recovery step (a1)
syncs and proves the prefix before the pre-CAS recovery of T-FAST; the
CAS moves the integration ref; a kill at `Written` of task_merged; then
a simulated power loss discards every unsynced write. The log still
contains merge_prepared, the ref is at proposed_sha, and the next resume
appends task_merged — on a real repository, with the sync ledger as the
durability oracle: in-process for the first crash, reported across the
process boundary for the second.

## `fn unsynced_merge_prepared_two_crash_barrier_before_cas_then_power_loss_keeps_log_and_ref_agreeing()` › `let report_path = fixture.root.join("two-crash-report");`

Restart, in a process of its own, killed at the task_merged write.

## `fn unsynced_merge_prepared_two_crash_barrier_before_cas_then_power_loss_keeps_log_and_ref_agreeing()` › `let reported = std::fs::read_to_string(&report_path).expect("the child reported");`

(a1) before the CAS: the child's barrier synced the whole surviving
prefix — the unsynced merge_prepared included — and only then was the
swap entered, once.

## `fn unsynced_merge_prepared_two_crash_barrier_before_cas_then_power_loss_keeps_log_and_ref_agreeing()` › `lose_unsynced_writes(&fixture, prefix_with_prepared);`

The second power loss: every unsynced write is discarded. What the
child proved durable is exactly what its barrier synced — the prefix
through merge_prepared — and nothing after it.

## `fn unsynced_merge_prepared_two_crash_barrier_before_cas_then_power_loss_keeps_log_and_ref_agreeing()` › `let third = harness();`

The next resume records the merge it finds done, with no second swap.

## `fn barrier_sync_failure_before_cas_issues_no_cas_and_converges_after_loss() {` › `let fixture = Fixture::healthy("barrier-sync-fails");`

`C.proof_tests[3]`, second sequence: the barrier's sync fails at
(a1). No CAS is issued, the command ends resumably having done nothing,
and after the loss of the unsynced line the before-append order holds:
the candidate is still queued, and the next incarnation integrates it.

## `fn barrier_sync_failure_before_cas_issues_no_cas_and_converges_after_loss() {` › `lose_unsynced_writes(&fixture, durable);`

The loss: the unsynced merge_prepared is gone, and the before-append
order stands — the candidate queued, no transaction, the ref at the
base — which the next incarnation carries through to publication.

## `fn a_resume_of_a_prepared_transaction_whose_ref_moved_elsewhere_refuses_a_third_sha() {` › `let fixture = Fixture::healthy("finish-third-sha");`

The barrier proved `merge_prepared` durable, but by the time recovery
runs the integration ref names neither the expected head nor the proposal
— a third writer moved it. Recovery refuses rather than clobbering it.

## `fn a_resume_completes_a_prepared_transaction_whose_cas_already_ran_by_recording_the_merge() {` › `let fixture = Fixture::healthy("finish-cas-done");`

The kill landed between the compare-and-swap and its `task_merged`: the
ref is already at the proposal. Recovery issues no second swap and records
the merge, so the log and the ref converge.

## `fn a_resume_settles_an_interrupted_stale_verification_and_reclaims_its_residue() {` › `let driven = drive(&fixture, &DriveSeams::default(), 1);`

And it does: the next incarnation's loop takes the requeued candidate
through a fresh stale sequence — cherry-pick, pin, verification,
publication — under sequence 2.

## `fn a_resume_reclaims_an_interrupted_verifications_snapshots_after_settling_it() {` › `let fixture = Fixture::two_tasks("interrupted-snapshot");`

T-VERIFY's resume action: "append merge_verification_interrupted; delete
pin expected-old; reclaim snapshots". The gate snapshot a killed
verification left is reclaimed with force — and only after the
interrupted terminal is durable, the order the contract fixes.

## `fn a_resume_reclaims_the_orphan_pin_at_the_next_sequence_and_orphan_staging() {` › `let fixture = Fixture::healthy("finish-orphan");`

T-PROPOSAL (a', a, b) with no transaction open: a cherry-pick killed
before anything was recorded left `merge/s0` and, killed between the pin
and `merge_verification_started`, the exact orphan `prepared/<next_seq>`.
Both are reclaimed; the proposal object is left to Git.

## `fn a_resume_reclaims_the_orphan_pin_at_the_next_sequence_and_orphan_staging() {` › `let snapshot = plant_snapshot(&fixture, 0, orphan_commit.as_str());`

A kill between a verification's terminal and its snapshot removal
leaves a snapshot with no transaction to own it.

## `fn a_resume_refuses_a_prepared_pin_outside_the_sequences_the_log_pinned() {` › `let fixture = Fixture::healthy("orphan-outside");`

`expected_failures_refusals`: "orphan pin outside next sequence". With
the next sequence at 0, `prepared/3` is a ref the log never accounts for:
recovery refuses it, before any append, and leaves it exactly as found.

## `fn a_resume_refuses_a_substituted_verification_pin_before_settling_it() {` › `let fixture = Fixture::two_tasks("substituted-pin");`

T-VERIFY's refusal condition: "pin SHA differs from record". A writer
moved `prepared/0` away from the proposal the verification recorded.
Expected-old deletion at what the ref *now* names would prove only that
nothing moved it since the read; authority comes from the record, and
the record disagrees, so recovery refuses — before the interrupted
terminal, and without touching the ref.

## `fn stale_clean_prepared(`

The `merge_prepared(stale_clean)` that authorizes the planted stale
verification's publication.

## `fn a_resume_keeps_a_prepared_transactions_pin_when_publication_refuses() {` › `let fixture = Fixture::two_tasks("prepared-pin-kept");`

T-PREPARED: `merge_prepared(stale_clean)` is durable and the ref has been
moved to a third SHA. Publication refuses, the transaction stays open,
and its pin — a resumably open resource the cleanup rule forbids
touching — is still there for the resume that will complete it.

## `fn a_resume_keeps_a_prepared_transactions_pin_when_publication_refuses() {` › `{`

What the resume will read back is exactly what the live sequence
authorized: a stale-clean publication with its pin and its staging.

## `fn a_resume_prunes_a_resolved_sequences_pin_at_its_recorded_proposal_and_refuses_it_elsewhere() {` › `for substituted in [false, true] {`

A kill between `task_merged` and the pin's deletion leaves a pin for a
resolved sequence. It is pruned expected-old at the proposal the
verification recorded, and at any other SHA it refuses and stays.

## `fn a_resume_completes_an_already_present_publication_at_the_candidate_commit_and_reclaims_its_staging()` › `let fixture = Fixture::healthy("already-present-at-candidate");`

The head was moved onto the candidate's own commit, so the stale path
found an empty cherry-pick and authorized `already_present` with
`expected_head == proposed_sha == candidate.commit_sha`. Inferring "fast"
from those SHAs would leak the staging worktree; the fold retains the
disposition, so recovery reclaims it after the no-op swap.

## `struct DriveSeams {`

---------------------------------------------------------------------------
Driving the loop after a resume: the run's own `TopologyRun::step` over a
resumed handle, with recording stand-ins for the seams a test varies.
---------------------------------------------------------------------------

The seams a driven step varies. Everything not named here is the run's
production assembly: `FrozenPlans` over the fixture's recorded gates and
review plan, the scaffold adapters, and a Runner that answers exit 0.


## `struct DriveSeams {` › `gate_spawn_fails: bool,`

The Runner returns an error for every process instead of an output.

## `struct DriveSeams {` › `input_rejected: bool,`

The review-input policy refuses the proposed tree.

## `struct DriveSeams {` › `review_cost_usd: Option<f64>,`

What each review pass reports as its cost.

## `struct DriveSeams {` › `answer_delivery: AnswerDelivery,`

Which read of the answer source delivers `answer`; the other refuses or
reports nobody there, so a test says which ingestion path it exercises and
the other cannot stand in for it.

## `enum AnswerDelivery {`

How `DrivenAnswers` delivers the seam's answer. PR #249's refusals review
found the mock answering `poll` and `resolve` alike, so removing the
pre-step ingestion (M4), replacing the non-blocking poll with the blocking
resolve (M5) and restoring PR8's hard-block refusal (M11) each passed every
topology test: either path satisfied the same assertions. `Polled` — the
default — answers `poll` and refuses `resolve`, which must never be reached
while a delivered answer is there; `Blocking` is terminal-style, `poll`
finds nobody and `resolve` (the hard block's prompt) answers.

## `struct DriveSeams {` › `answer: Option<crate::ir::Answer>,`

What the answer source answers every question with; `None` answers
nothing.

## `struct Driven {`

What a driven run observed.

## `struct Driven {` › `implementers: Vec<PassBinding>,`

The implementer each verification plan was requested against.

## `struct Driven {` › `reviewer_models: Vec<String>,`

The model each review pass actually ran as.

## `struct DrivenPlans<'a> {` › `implementers: std::cell::RefCell<Vec<PassBinding>>,`

One driver owns this record; `RefCell` lets the read-only seam write it.

## `struct Driven {` › `runs: Vec<DrivenRun>,`

Every process the driven runner was asked to run — gates and reviewers
alike — with its role, the workspace it was pointed at and that
workspace's HEAD at spawn. The verifier oracle reads the reviewers' entries
to prove each reviewer judged the recorded proposal in its own snapshot
slot and never in staging (`PR8-R4-REVIEW-ORACLE`).

## `impl crate::engine::topology::attempt::ReviewPasses for DrivenReviews` › `fn run(`

The driven review double runs a process through the Runner in the
workspace it was handed before answering its scripted verdict, for the
reason the scaffold's does (`scaffold.md`): a double that reads only the
profile cannot see where the production judge pointed it, and the cover
review of `8a5f59e8` moved that pointer into staging past every test.

## `struct DrivenRun {`

One process the driven runner ran: identity, role, workspace and the
workspace's HEAD at spawn.

## `fn drive(fixture: &Fixture, seams: &DriveSeams, steps: usize) -> Driven {`


Resume the fixture, then step the run's loop `steps` times under `seams`.

## `fn an_integration_review_is_selected_against_the_candidates_recorded_implementer() {` › `let fixture = Fixture::build(`

The candidate was produced at rung 0 (mid, claude-opus-5) of a two-rung
ladder whose last rung is claude-fable-5, and the review plan's primary
is claude-opus-5 with claude-fable-5 as the alternative. Selecting
reviewers against the last rung would find primary != implementer and
hand the candidate back to its own author; against the recorded binding
the alternative reviews it.

## `fn a_gate_spawn_failure_during_integration_verification_defers_inside_max_defers() {` › `let fixture = Fixture::two_tasks("gate-spawn-outage");`

`transaction_fault_matrix[T-VERIFY].resume_action`: an observed
infrastructure failure terminates merge_verification_unavailable
{Infrastructure, Deferred} inside the frozen allowance and Parked at it;
`invariants[INV-23]`: a Runner that cannot run the process is a
RunnerSpawnFailure outage. The fixture allows three deferrals.

## `fn an_unjudgeable_proposal_parks_the_candidate_for_a_person() {` › `let fixture = Fixture::two_tasks("input-rejected");`

R4: a review input that cannot be judged is HumanRequired, not a code
rejection and not a publication. The review-input policy refuses the
proposed tree before any reviewer runs.

## `fn an_integration_reviews_cost_reaches_the_run_spend() {` › `let fixture = Fixture::two_tasks("integration-spend");`

The ceiling is checked against `Spend`, so a review an integration ran
must be charged there, live and on replay of the terminal's record.

## `fn a_paid_review_that_parks_is_charged_live_and_its_cost_replays() {`

`PR8-R2-SPEND-REPLAY` from both ends, inside one incarnation: a 2.50
review returns needs_human under a 2.20 ceiling, the park is charged
live, the answered candidate's next integration is refused in the same
incarnation, the terminal carries the pass, and a replay of the log
reaches the total the run held rather than the one it started from.

This was the test that pinned the gap. Its last assertion required the
replayed total to equal the *opening* one and said in its message that
the day the terminal gained its record the assertion would fail and the
ledger row would close. It did, and it now asserts the closing total.

## `fn the_cost_of_a_parked_verification_still_refuses_the_next_integration_after_a_restart() {`

The same finding's harm, which is not a number but a decision, across the
restart that is the whole of it. One incarnation parks the paid
verification and answers it; a second resumes the log it wrote and must
refuse the integration the first one refused. Withdraw the replay of the
terminal's reviews and the second incarnation reports `Integrated`
instead — the spend assertion sits last precisely so the failure names
the decision and not the arithmetic behind it.

Two incarnations because one cannot witness this: the live account holds
the cost inside an incarnation whatever the log says, so any assertion
made without crossing a resume passes with the terminal empty.

## `fn a_verification_park_answer_is_ingested_and_the_candidate_re_verifies() {`

R13: the loop ingests an answer to a verification-park question —
Answered returns the candidate to the queue, and the next step integrates
it. The repair-admission half of PR8's version of this test, a refusal,
became
`a_repair_admission_answer_activates_the_repair_which_materializes_and_merges_through_the_queue`.

## `const BETA: TaskKey = TaskKey(1);`

---------------------------------------------------------------------------
T-PROPOSAL (a'): the cherry-pick residue class, recovered through the
resume — `C.proof_tests[2]` and `[T-PROPOSAL].test`.
---------------------------------------------------------------------------

## `fn plant_published_beta(fixture: &Fixture) -> CommitSha {`

Publish BETA's candidate fast at sequence 0 — the candidate on the base,
`merge_prepared(fast)`, the ref moved, `task_merged` — so the head has
legitimately moved past the base. Needs a two-task fixture.

## `fn plant_stale_queued_candidate(fixture: &Fixture) -> (PlantedTransaction, CommitSha) {`

A queued candidate whose base the integration head has legitimately moved
past — BETA published at sequence 0 — so ALPHA's integration takes the
staging path under sequence 1: returns the planted candidate and the head.

## `fn worktree_git_dir(worktree: &Path) -> PathBuf {`

The per-worktree git dir of a linked worktree, where its administrative
residue lives.

## `fn assert_staging_residue_reclaimed(fixture: &Fixture, staging: &Path, handle: &RunHandle) {`

After the residue is gone: the staging slot and every prepared pin, and
the candidate still queued for the next incarnation.

## `fn synthetic_cherry_pick_residue_unreferenced_objects_and_cherry_pick_head_then_forced_reclaim_converges()` › `let fixture = Fixture::build(`

The Internal residue class of Object.ProposalCherryPick, constructed by
hand: objects the pick wrote that nothing references, and CHERRY_PICK_HEAD,
MERGE_MSG, index.lock and sequencer state in the staging git dir. The
resume reclaims the staging worktree with force — administrative
residue and all — leaves the objects to Git, and the next incarnation
integrates the candidate under the very sequence the residue held.

## `fn synthetic_cherry_pick_residue_unreferenced_objects_and_cherry_pick_head_then_forced_reclaim_converges()` › `let orphan_file = fixture.root.join("orphan-bytes");`

Objects written and never published: a blob and a commit no ref names.

## `fn synthetic_cherry_pick_residue_unreferenced_objects_and_cherry_pick_head_then_forced_reclaim_converges()` › `let site = EffectSiteId::Object(ObjectSite::ProposalCherryPick);`

The workspace manager's classifier reads it as the site's Internal class
and names what was planted.

## `fn remove_packed_refs_lock_residue(git_dir: &Path) -> Option<PathBuf> {`

Git's one ref-lock residue recovery may not touch: `packed-refs.lock` in
the repository's common git dir. It is the repository's rather than the
run's, any Git process can be holding it, and a wrong removal would let a
concurrent `pack-refs` publish an empty packed file over every packed
ref, so the engine never reclaims it and the operator does. A lock on one
of the run's own refs is no longer removed here: the Ref funnel reclaims
it when the repository proves it stale
(`WorkspaceManager::reclaim_own_ref_lock`). Not the run's
`upstroke-worktree.lock`, which is the coordinator's lock file (R25), and
not a linked worktree's git dir, which recovery reclaims itself.

## `fn sampled_cherry_pick_child_kills_every_residue_classified_and_recovered() {` › `const SAMPLING_N: u32 = 8;`

Object.ProposalCherryPick's frozen sampling N (effects/residue-classes.json).

## `fn sampled_cherry_pick_child_kills_every_residue_classified_and_recovered() {` › `const MAX_SPAWNS: u32 = 2 * SAMPLING_N;`

One bounded retry, the shape `PR7-SAMPLER-SCHEDULES-FROM-A-COLD-PROBE` gave
the T-ATTEMPT sampler: when no kill of the first `SAMPLING_N` landed inside a
writing pick — every child completed before its kill, or every kill found a
child that had not begun or one that had finished — the ladder has by then
been re-aimed inside every pick that completed, and `SAMPLING_N` more are
sampled on it before the refusals at the end fire. Every child, in either
half, is classified, reclaimed and driven to integration, so a completed pick
is verified as a control and counted as nothing. The refusals count kills
only, over every spawn, and the retry does not weaken them: sixteen clean
exits are still a run in which nothing was killed, and sixteen kills of
children that had not begun are still a run in which no pick under way was
interrupted.

The second batch is a whole batch. The loop plans `SAMPLING_N` spawns and,
when the eighth is in and no kill has landed inside a writing pick, plans
`MAX_SPAWNS`; until 2026-09-10 it went on only while no kill had landed, so
the second batch ended at its first kill — reproduced: eight controls, run 8
killed at 117 µs and classified `None`, `spawns=9`, a pass — and one
earliest-rung, pre-write kill stood as the evidence of a batch (the ultra
review of `f837f4ca`, finding 1). Reproduced again at `62f55943` with the
first batch's aims fifty times too long: the old loop stops at `spawns=9,
killed=1`; this one runs `spawns=16, killed=8`, every kill in the second
batch, the ladder having followed the first batch's eight completions. The
batch's condition widened from "no kill" to "no kill inside a writing pick"
with the floor below (`killed_while_writing`), so that the retry serves it as
it serves the vacuity refusal. A batch of kills that all found children that
had not begun leaves the budget nothing to follow — a killed child measures no
pick — so the second batch is aimed as the first was, and when it lands the
same way the refusal fires with every aim and class in its message: the honest
outcome for a budget short of every pick, which the ladder can correct only
through a completion.

## `fn sampled_cherry_pick_child_kills_every_residue_classified_and_recovered() {` › `let two_tasks = || Damage {`

How long the same pick takes when nothing kills it, measured in a probe
fixture of its own: four uninterrupted picks in the probe's staging
worktree, the worktree reset to the head between them, the first discarded
as the warm-up and the median of the other three taken
(`fixture::KillBudget`). The kill ladder is fractions of that budget, and
the budget then follows the samples — a child that completed before its
kill has measured the pick under the sampler's own conditions, at that
moment, and the next rung is aimed inside the median of the last three
such completions.

Until 2026-09-10 the budget was one pick, the first cherry-pick in a fresh
staging worktree, and every kill was `sleep(fraction)` then `kill`.
`RECOVER-CHERRY-PICK-SAMPLER-COLD-PROBE`: on the hosted `test (macos-latest)`
leg that one measurement was, five times in two days, more than nine
times the picks it scheduled — all eight children exited 0 before the
lowest rung — and the refusal fired, with this text, on two pull requests
(run 34304029954 at `828da6cd`, whose diff was Markdown, and 34353183264
at `fbf3e50b`), on two pushes to master (34328230257 at `9a6897ea`,
34356671343 at `74da2cbb`) and on a merge-queue entry for #258
(34433061085): 5 of the 155 hosted macOS runs that completed between
2026-09-07 and 2026-09-10, in a census that names every run and matches
this message rather than the test's name (#259's body). The refusal was
right each time: nothing had been sampled. Measured on the
build box at `81ee09ef`: the first pick in a fresh staging worktree takes
1.1 ms against 0.94 ms warm, a pick's first write lands about 0.6 ms in
and the eight rungs land five `None` and three `Internal`, so an aim even
twice too long misses every write window; 0 of 25 runs alone failed here
before the change and 0 of 25 after it, and the leg's own evidence is CI's.

## `fn sampled_cherry_pick_child_kills_every_residue_classified_and_recovered() {` › `let mut killed_while_writing = 0_u32;`

The second floor, added 2026-09-10: at least one kill must have found the
pick's own state — `Internal`: its index lock, its sequencer state, an object
written and not yet published — which is a kill after the pick's first write
and before its publish, inside a pick under way. A kill that found nothing
written (`None`) interrupted a child that had not begun; one that found the
commit published (`After`) interrupted a pick that had finished; both are
kills, and `killed_while_running >= 1` — which stays, and fires first —
accepts either. Both P1s this pull request produced had one shape: the budget
collapsed, every kill landed before the first write, and this sampler passed
on seven `None` kills (the ultra reviews of `2d3fa9d1` and `8441c5fe`, finding
1 of each) while the dispatch sampler's `while_writing` floor refused the same
ladder both times. The floor here is that one's, on the class the classifier
answered for a killed child: `Internal` only — a killed `After` is not
counted, where the dispatch floor counts an `After` whose `MERGE_MSG` is
missing, because this sampler reads no element of a published pick before
recovery reclaims it.

Measured on the build box before it was added, 25 unmutated runs, every run
eight kills in eight spawns: 2 to 4 of the eight were `Internal` (median 3; 10
runs with 2, 12 with 3, 3 with 4), the rest `None` but for one `After`, and
every run had at least one. Where they land is structural: with the probe's
median at 946 µs (894–1072) and the first write about 0.6 ms in, rungs 1 to 3
(aimed at about 105, 210 and 315 µs) were `None` in all 25 runs, rung 4 (421
µs) `Internal` once, rung 5 (526 µs) 6 times, rung 6 (631 µs) 20 times, rung 7
(736 µs) 24 times and rung 8 (841 µs) 17 times, with 7 `None` — picks whose
first write came after 841 µs — and the one `After`. With the floor in place,
0 of 25 runs failed. Against it: every kill aimed at zero at spawn, which this
sampler passed before, now refuses twice with `none of the 16 kills in 16
spawns landed while the pick was writing`; and the failed-kill injection with
the feedback reverted to the kill's clock — the ladder collapsed as at
`8441c5fe` — refuses twice with `none of the 15 kills in 16 spawns`, the whole
second batch having run. macOS is reasoned, not measured: the three hosted
dispatch reds in the census each had only their lowest rung inside the pick,
at about two thirds of it, and 9 of those 18 kills had found the pick writing
(4 of 6, 3 of 7, 2 of 5), so the runner's first write sits where this box's
does relative to the pick; the full pick's write phase — index, tree, commit,
ref and the sequencer's cleanup — is longer than the no-commit pick's; and the
dispatch floor has caused none of that leg's reds. What the floor cannot do is
correct a budget short of every pick: a killed child measures no pick, so a
batch of `None` kills leaves the second batch aimed as the first was, and
sixteen of them refuse with every aim and class in the message — a red that
says what it sampled, which is the outcome this floor prefers to a pass on
kills of nothing.

## `fn sampled_cherry_pick_child_kills_every_residue_classified_and_recovered() {` › `let aim = budget.aim(run % SAMPLING_N, SAMPLING_N);`

The rung's aim, `(rung + 1) / (SAMPLING_N + 1)` of the current budget, and
`KillableGitChild::run_until` in place of a sleep then a kill: the child is
polled to the aim — once a millisecond while it is far, continuously through
its last four — and the poll that reaches the aim with the child still
running sends the kill itself, with no return to the caller between the two,
so the sleep that used to sit between the aim and the kill is gone. A child
that exits first is observed at the poll that finds it gone, and that
observation is the number the budget follows: the parent's clock, from an
origin read before the spawn — the next poll's when the parent holds a core,
the wake-up's when it does not — not the child's own time, which no wait reports
to a parent (`wait4` carries CPU times; Windows' `GetProcessTimes` carries
an exit time and the fixture does not bind it). What remains is the window
between that last poll's `try_wait` and the kill's system call: a parent
descheduled there lets the child finish, the kill misses, and the child's
status is a completion. Such a pick is fed back too, where until 2026-09-10 it was thrown away (the
ultra review of `f837f4ca`, finding 2), and the number it is fed back as is
the clock at which the parent established the exit: for a child the poll found
gone, that poll's; for a child that was running at its aim and ended with a
completion's status, the clock `KillableGitChild::wait` reads once
`Child::wait` has returned that status (`reaped`). The kill's own clock is not
that number. `Child::kill` returns `Ok` once the signal is sent and also for a
child that has already exited, and `Err` when nothing was sent, so the clock
once it has returned orders the call and bounds no child; two heads fed it
back as a bound and each was a P1. At `62f55943` the clock was read *before*
the system call, and the ultra review of `2d3fa9d1` (finding 1) showed what
that fed back: a 20 ms pause planted between the read and the call let run 0's
pick run to completion and fed it back as the instant before the pause — 120.8
µs, for a pick the probe had just measured at 1.09 ms, below the pick, clamped
to the 200 µs floor — so every later rung was aimed at 44–178 µs, before the
pick's first write at about 0.6 ms; seven children died as `None` with nothing
written, `killed_while_running >= 1` was satisfied, and the run passed on
kills of nothing (`spawns=8 killed=7 completions=1`, twice at `2d3fa9d1`),
where the reviewer's control with that feedback off reached two `Internal`
residues. At `95eece1c` the clock was read after the call and its `Result`
discarded, and the ultra review of `8441c5fe` (finding 1) made the first kill
return `Err` without sending, asserted `try_wait()` still `Ok(None)` once the
clock was recorded — the child alive after the alleged bound — and let it
finish: fed back as 117 µs, the same collapse, `spawns=8 killed=7
completions=1`, a pass. The same injection here, with an oracle in
`KillBudget::completed` that the value fed back does not precede the clock at
which the child's status came in: at `8441c5fe` it fails on run 0 (105.6 µs
fed back for a child whose exit was established at 1.20 ms; 102.2 µs against
966 µs the second time), and at this head the value fed back is the wait's
clock itself (the kill failed at 109.8 µs, the wait returned at 1.092 ms,
1.092 ms fed back; 108.6 µs, 1.036 ms, 1.036 ms), the ladder follows a real
pick and the run passes on kills inside it. The 20 ms pause, from `95eece1c`
on, feeds run 0 back as the pause's own length — here 20.174 and 20.172 ms
against a pause that ended at 20.169 and 20.167 ms, the value asserted not to
precede it; at `2d3fa9d1` that assertion fails on the first kill — and the
next two rungs, aimed at 4.5 and 6.7 ms, are past the pick and complete in
1.1–2.1 ms, the median of the three is back at the pick and the later rungs
are killed inside it (at `95eece1c`: runs 3 to 7 killed at 470–939 µs, four of
them `Internal`, `spawns=8 killed=5 completions=3`, twice). A pause on every
spawn refuses, having followed all sixteen completions: the vacuity floor is
unchanged and still fires when no kill can land. The clock's origin was the
last of the three reads to move: until 2026-09-10 `KillableGitChild::spawn`
read it once `Command::spawn` had returned, so a child could run, or finish,
before the origin existed (the ultra review of `d1fef26d`, finding 1). A 20
ms pause planted there on the first child — after the spawn had returned,
before the origin was read — let run 0's pick complete during the pause; the
first poll found it gone at 2.6–3.1 µs against a probe of 912–915 µs, that
was fed back and clamped to the 200 µs floor, every later rung was aimed at
22–178 µs, and the floor above refused, twice, with `none of the 15 kills in
16 spawns landed while the pick was writing` — the whole second batch run,
fifteen kills of children that had not begun: a red where the two earlier
collapses had passed, and the red this change exists to remove. The origin is
read before the spawn now, and the same pause feeds run 0 back as 20.1 ms
against a pause of 20.05 ms, the value asserted not to be shorter than the
pause the child was alive through; the next picks complete in 1.1–2.2 ms and
the rest of the batch is killed inside the pick (#259's body, W4). What the
budget follows is a
median over however many of the last three completions exist — one completion
sets the budget alone, of two the longer is taken, of three the middle — so a
late observation is not corrected by the next pick: a late first observation
holds until two shorter completions follow it, and three late ones hold until
two do, each correction costing the batch two controls. Measured on the build
box, and only there: over three unmutated runs at `95eece1c`, 24 kills, the
clock a kill recorded — read after `Child::kill` returned, so with the system
call inside it — was 2.9–5.6 µs past its aim, and the dispatch sampler's 24
were 1.6–3.6 µs; and over three separate populations of forty `git --version`
children each, the median clock at which a blocking `wait` saw its child gone
was 269 µs (242–382), at which `run_until` saw it while spinning 341 µs
(311–438) and while sleeping 1.054 ms (1.037–1.065). Those are medians of
separate populations, not per-exit lags and not maxima; they size the
observation's lateness on this host and bound nothing, and macOS and Windows
are reasoned, not measured. Every spawn's aim and outcome goes into the
refusals' messages — completed, in the poll's clock; killed, with the clock at
which the kill returned; outran the kill, with that clock and the wait's; or
outlived a kill that failed, with its error and the wait's clock — with the
probe and the number of completions the ladder followed, so a red leg carries
the timing evidence the finding said it lacked.

## `const SAMPLING_N: u32 = 8;` › `let mut child = KillableGitChild::spawn(`

The real child, killed at an uncontrolled point of the ladder.

## `const SAMPLING_N: u32 = 8;` › `let _ = remove_packed_refs_lock_residue(&fixture.git_dir);`

A killed git child can also leave `packed-refs.lock` in the repository's
common git dir — observed on the macOS runner's git — which no residue
class of the site names and no recovery step may remove
(`PR8-CRASH-002-PACKED-REFS-LOCK`). It is removed here as the operator
would, so the sampler measures the staging residue the site registers.

## `const SAMPLING_N: u32 = 8;` › `let (_, handle) = resume_with_real_refs(&fixture, &harness())`

Whatever the sample left, the resume reclaims it and the candidate
integrates under a fresh pick.

## `fn plant_integration_lock(fixture: &Fixture, content: &[u8]) -> PathBuf {`

The lock file a killed `git update-ref` leaves on the integration ref,
with the content Git leaves at the point of the kill. Measured on git
2.43 under `strace`: the file is empty from Git's `open` to its content
write — the longer part of the window, where the object lookup sits —
and holds the new object id and its newline from that write to the
publishing rename.

## `fn an_empty_ref_lock_left_by_a_killed_compare_and_swap_is_reclaimed_and_the_publication_completes()`

PR8-CRASH-002, closed: a coordinator killed inside `git update-ref` left
`<ref>.lock` and every later resume refused on it until an operator
removed the file. The Ref funnel now reclaims the lock before the retry
when the repository proves it the engine's own and stale
(`WorkspaceManager::reclaim_own_ref_lock`), and the publication the log
authorized completes on the first resume. Witnessed against the
unrepaired tree: the resume refused with Git's own "integration.lock …
File exists".

## `fn a_ref_lock_naming_the_authorized_proposal_is_reclaimed_with_or_without_its_newline() {`

The other shape a kill leaves: the lock already names the proposal,
with its newline after Git's second write and without it between the
two. Both are the engine's own write of this transition and both are
reclaimed.

## `fn a_ref_lock_naming_another_object_is_left_and_refuses_resumably_until_removed() {`

The negative control, and what still holds of the old behaviour: a lock
naming a value the swap would not write belongs to another write, so it
is left where it is. The refusal says why, the ref is unchanged,
`merge_prepared` stays durable, nothing is appended, and once an
operator removes the lock the next resume completes the publication.

## `fn a_ref_lock_on_a_packed_integration_ref_is_left_and_refuses_resumably_until_removed() {`

With the ref in `packed-refs`, a `git pack-refs --prune` may be holding
its lock at this instant and nothing the repository records says
otherwise, so the lock is left and the resume refuses
(`PR8-CRASH-002-PACKED-RUN-REF`). Once the operator has removed it the
swap writes the loose ref over the packed copy, as any swap of a packed
ref does.

## `fn a_surviving_ref_writer_of_the_dead_coordinator_refuses_the_resume_until_it_exits() {`

The liveness fact the reclaim rests on, end to end: a process holding
the run's cleanup lease the way an engine `git update-ref` child does
(`rundir::hold_cleanup_lease_for_child`) makes the resume refuse at its
worktree lease, with the lock untouched and nothing appended; once it
exits the kernel releases the lease, the lock is stale, and the next
resume reclaims it and completes the publication.

## `struct BlockNthSnapshotAdd {`

The obstruction the final cover reviewer's witness used: the second reviewer's
snapshot slot is occupied by a foreign non-empty directory just before its
`git worktree add` runs, so that `add` fails with a genuine Git error after the
first reviewer has already been paid for. `write_file` creates the parents, so
one write leaves the slot occupied; it goes through the fixture funnel because
`std::fs::write` and `std::fs::create_dir_all` are denied here (R18/R21).

## `fn a_completed_integration_review_is_charged_when_the_next_reviewers_snapshot_fails() {`

Two reviewers on the integration judgement. The first returns, costing 2.50
against a 2.20 run ceiling; creating the second's snapshot then fails with a Git
error, which `verify` settles as an infrastructure deferral rather than ending
the command. The completed pass is spent whatever the judgement does next, so
the loop's next admission — in this same incarnation, with no restart — has to
be made against a total that holds it. Charging only on a successful judgement
return discarded the whole vector with `?` and let another sequence in.

That was the whole of this test. `PR8-R2-SPEND-REPLAY` was the restart half of
the same money — the frozen terminal could carry no record, so a resumed run
replayed a total without it — and closing it reached this arm too, which is the
one with no judgement to record from. The terminal now carries the pass that
returned, taken from what the account charged rather than from a judgement that
did not survive, and the last two assertions hold the terminal's `reviews` and
the replayed total to it. Withdraw `charged` from the Git-error arm of
`IntegrationCx::verify` and the terminal comes back `reviews: []` while the live
total still reads 3.80.

## `fn a_reviewer_whose_process_never_started_is_a_runner_spawn_failure() {`

A reviewer whose process never started settles as a `RunnerSpawnFailure`
(`invariants[22]`, INV-23), and every other unavailable reviewer still settles
as `ReviewUnavailable`.

INV-23 requires it in those words — a container whose reported image id differs
from the record "refuses during pre-flight or rebuild and is a
RunnerSpawnFailure outage settlement mid-run" — and says the rule covers "every
probe, worker, gate, review, and re-ask process of the run". The container
runner detects a reviewer's image mismatch before start and answers
`NeverStarted`; `run_review` contains that so the pass can defer instead of
ending the command, and the contained result was `Unavailable{AgentError}`,
which the generic mapping turns into `ReviewUnavailable` — a statement about
the reviewer where the invariant names one about the runner. Deferral and
containment were right, and the `Gone` arm is the control that says this repair
changed only the attribution of the one fate the invariant names.

## `fn checkout_of(workspace: &Path) -> BTreeMap<String, String> {`

What the workspace held when the process was invoked: every tracked file and its
content, read at the moment `DrivenRunner` stands in for the agent. The
dependency regression asserts on this rather than on a recorded SHA, because a
SHA assertion is the shape that would have passed for the whole life of
`PR8-R7-DISPATCH-BASE` — the durable record and the worktree agreed with each
other throughout, and both were wrong.

## `fn a_dependent_task_is_dispatched_into_its_dependencys_merged_work() {`

The regression for `PR8-R7-DISPATCH-BASE`, driven end to end: alpha queued and
published, beta ready only then, and beta's agent handed a checkout that
contains alpha's merged file. Nothing exotic is planted — no concurrent writer,
no hostile filename, no crash, no injected failure — because the defect is the
base case of a dependency chain. Mutating `dispatch_request` back to
`run_started.base_sha` fails it at the checkout assertion with the worktree
holding only `seed.txt`.

## `fn a_dispatch_recorded_before_this_rule_resumes_at_the_base_it_recorded() {`

The other direction, and the one a repair that is right going forward can still
get wrong: a log in the shape the engine wrote before the rule changed — a task
dispatched at the run's starting base while a publication had already moved the
head — must replay to the decisions it recorded. The base is
`task_dispatched.base_sha`, so `continue_open` rebuilds the worktree at it, the
head that has since been published is not substituted, and no second
`task_dispatched` appears. Mutating `continue_open` to re-derive its base
through [`super::super::integrate::dispatch_head`] fails it at the worker's
HEAD.

## `fn plant_rejected_repair(fixture: &Fixture) -> (crate::topology::events::MergeRejected, TaskKey) {`

The one planting every repair test starts from: alpha's candidate,
stale-verified at beta's published head and rejected by review, so the
rejection registers alpha's first repair. Its admission is whatever the
fixture's `max_merge_repairs` decides — `Runnable` at the default of one,
`HumanRequired` under `no_automatic_repairs`, `HumanBinding` under
`small_only` (the root's one Small rung leaves the repair's Mid floor
empty).

## `fn an_over_limit_repair_spends_nothing_until_its_answer_activates_it() {`

An over-limit repair is `AwaitingInput`, the run hard-blocks on its one
question, and nothing is spent or appended beyond the resume's own record
until a person answers.

## `fn a_repair_admission_answer_activates_the_repair_which_materializes_and_merges_through_the_queue() {`

The whole of a repair's life at the loop, once per answer delivery: the
admission answer is ingested (`question_answered` before `task_dispatched`),
the repair dispatches inside its root's lineage lease at the head current
at its dispatch, materializes the rejected candidate (`Clean`, recorded
before the spawn), runs to a candidate that widens the lineage, and merges
exact-base with `satisfies` the canonical closure and the lineage lease
released. R11's candidates ref is still there afterwards. What the worker
saw and what was published are compared as bytes against the protected
source's blob and the content already merged at the base — PR #249's
refusals review (M6) showed the SHA oracles green over a corrupted checkout.

## `fn a_delivered_answer_is_ingested_before_unrelated_runnable_work_dispatches() {`

DESIGN §4 (6) at the loop: with beta genuinely runnable and a halting
decline already delivered to the polled source, the decline is ingested
first, beta never dispatches and no process runs. The test the admission
fixtures could not be — nothing else was runnable there, so ingestion
removed (M4) or made blocking (M5) still passed.

## `fn a_repair_dispatch_interrupted_before_its_attempt_is_recreated_at_its_base_and_materialized_once() {`

`T-REPAIR-DISPATCH` across the process boundary, for three of the states a
kill leaves between `task_dispatched` and `attempt_started`: no worktree, a
worktree with the pick's held `MERGE_MSG.lock`, a completed pick (which is
also the shape of a kill after the index publish and before the message
lock, the K2 state). Recovery (g) recreates the first two at the base and
reuses the third, materializing nothing (`R6`); the continuation
materializes exactly once, in the same generation — onto the restored base
tree, not as a no-op onto the merged index — and the observation
`attempt_started` records is the continuation's.

## `fn a_fresh_incarnation_closes_a_retained_repair_generation_lineage_held_and_the_next_materializes_again() {`

`ST-11` for a repair: (e) closes the retained generation
`ResumeDiscardsRetainedSession` with `LineageHeld`, the repair opens its
next generation from the same recorded source, and that generation is
materialized again, once; the closed one never is. The closed generation's
real worktree and intent, planted before the retained prefix, are gone
after the resume and still gone after the replacement merges and another
resume runs — PR #249's crash review, finding 2.

## `fn picking_the_last_of_two_offered_agents_binds_the_repair_to_it_rather_than_declining() {`

PR #249's conformance review, finding 1, end to end: two agents offered,
`2` typed at the production parser, and the repair runs under the second
agent with `option_index: 1` and the catalogue's lowest model for it —
where the last-option rule had recorded a decline and failed the lineage.

## `fn a_rejected_candidates_ref_survives_a_budget_stop_and_the_repair_dispatches_after_the_resume() {`

R11 across a budget stop: the candidates ref protecting the repair's source
is untouched by the stop and by the resume that clears it, and the repair
dispatches from it in the next epoch.

## `fn a_one_off_binding_answer_activates_a_repair_no_frozen_rung_can_run() {`

`ST-12` at the loop, once per answer delivery: a `HumanBinding` admission,
answered with an agent, carries the five-field override derived once at
ingest — the repair ladder's Mid floor, the catalogue's lowest model for the
agent at or above it, the policy's Mid effort — and the attempt runs under
exactly that binding, pinned, at rung 0. The `Blocking` arm is the one
PR8's hard-block refusal fails: PR #249's refusals review restored that
refusal (M11) and every test passed, because the polled path had ingested
the answer before the block was reached.

## `fn a_binding_answer_naming_no_frozen_option_is_refused_before_any_append() {`

Text that names no frozen option is refused, the log gains nothing beyond
the resume's own record, and the question stays open.

## `fn declining_a_repairs_admission_fails_its_lineage_and_halts_the_run_only_when_asked_to() {`

Each halting value arrives by a different delivery, so a decline is proven
ingested at the poll and at the hard block.

A decline fails the repair and its root, releases the lineage lease, and
ends the run exactly when `halts_run` says so; what follows is the run's
closure, which this build refuses.

## `fn a_repairs_same_session_retry_records_retained_and_is_not_materialized_again() {`

`ST-15` for a repair, in the incarnation that retained the session: the
retry is the same generation's second attempt, resumes the session,
records `Materialization::Retained`, and the materialization funnel ran
once. This test is what found the funnel leaving `MERGE_MSG` behind, which
failed the retry's `HoldsTree` verification and closed the generation.

## `fn an_answer_published_into_the_run_directory_is_ingested_by_the_next_incarnations_first_step() {`

`T-ANSWER` through the production reader: with no answer file the run
hard-blocks; an answer staged and published into `answers/` while the
engine is away is ingested by the next incarnation's first step, `via`
`event-log`, before anything else is selected.

## `fn two_lineages_publish_in_lineage_order_and_the_younger_candidate_waits_behind_the_older() {`

Two lineages overlapping on one path, the younger's repair already queued
when the older's has not started: the younger candidate is queued but
ineligible (`BehindOlderLineage`), the older repair dispatches and
publishes first, and the younger publishes onto the head it left —
lineage order, not queue position, each publication satisfying its own
closure and releasing its own lineage lease.

## `fn with_live_run<R>(`

PR10's loop-level fixture: a resumed `TopologyRun` under a ceiling of the
test's choosing, handed to a body with its seams and hooks, the resume
run first so an arming inside the body lands in the loop and not in
recovery. `with_live_run_hooked` takes the caller's hooks;
`with_live_run_hooked_runner` the caller's runner as well.

## `fn plant_finished_run_with(`

A run planted at its end for the ST-18 tests: alpha queued, published or
parked, beta's one attempt failed with the halting policy the outcome
needs, beta's closed generation still holding its worktree and intent, the
residue asked for (a snapshot, a staging worktree, a proposal pin), and
`run_finished` durable — what terminal finalization has to act on, with
nothing yet done to it.

## `struct ArmedFinalization {`

Hooks that inject an error return at one `(site, phase)` of one
finalization, the nth time it is reached, so the T-FINALIZE matrix is
driven at every cleanup site in turn and the next resume is shown to
converge from each.

## `fn live_vs_replay(`

Q1's comparison, committed at the G4 gate's request: the live incremental
fold of a stepped run and the report derived from it, against a fresh
replay of the bytes on disk and its report.

## `fn ledger_inventory(fixture: &Fixture, fold: &TopologyFold, released: &[String]) -> PhysicalInventory {`

The physical half of the ledger, measured from the fixture: slots by
namespace, refs and pins, the run directory and the private half, the two
lock files, container intents, the volume classification, and the objects
the pre-finalization observation saw referenced, checked present after.

## `fn drive_observing(`

`drive` with an observer called after every step with the step number and
the run, so a test can read the live fold and the process-local ledgers
before the run is dropped; `drive_handle` is the same loop with a no-op
observer.

## `struct ArmedAppendError {`

Hooks that return `Err` from the `Written` point of the nth transaction
append counted from the moment the countdown is set — the append-error
protocol, aimed at one line of the test's choosing, which is how the
NoRunFinished ledger is driven rather than planted.

## `fn observation_export_env() -> Vec<(String, String)> {`

The ST-07 observation export directory, handed on to a spawned kill child:
the host runner composes the child's environment from scratch, so a
variable the parent test was started with does not reach the child unless
the request carries it.

## `fn wait_for_cleanup_hold_release(public: &Path) -> bool {`

Wait, bounded, for the run's cleanup lease to be free. A `git` child of
the ref funnel holds the lease while it lives, through a descriptor made
inheritable for it, and under a parallel suite a child another test
thread forks in that window can inherit the descriptor and hold the
lease until it exits. The wait is bounded so a hold that never clears
still fails the assertion that follows it; the ledger's post-drop
observation and the finalization matrix's resumes wait through it.
