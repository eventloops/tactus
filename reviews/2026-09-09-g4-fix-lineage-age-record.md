# G4 fix — a released lineage's age is reused, and an overtake follows: the record

The record of the repair of the P1 that the independent frontier verification of G4's report found
in the frozen tree at `74da2cbb` (`~/tactus-artifacts/G4-VERIFICATION.md`, finding 1: *"Ordering
proven" is contradicted by the frozen tree*). Kept on the branch so the pull request's reviewer and
every later gate can read what was executed, what class the change is, and what kills its test.
It is **not** a design document: `DESIGN.md` stays the authority, and a sentence here that
disagrees with it is a defect in this file.

**Branch:** `fix-P1/correctness_a-released-lineage-age-is-reused-and-overtaken`, cut from `master`
at `74da2cbbd24c55f7aed7f3593162981a11720f79`, the merge commit that is G4's frozen input range.

**Where this file lives, and why.** The same reasoning as `reviews/2026-09-08-pr9-record.md`: no
new file at the repository root (the brief forbids one), and `docs/internals/` holds only files
that mirror a Rust module, so the record sits beside the gate reports in `reviews/`, dated.

## 0. Status

| Step | State |
|---|---|
| A execute the sequence the verification derived, on the frozen tree, before any change | **done** — §1; it reproduces exactly as derived |
| B establish the freeze class of the fix before implementing it | **done** — §2: Class B, row G4FIX-B1; no Class C change |
| C the fix | **done** — §3, `src/topology/leases.rs` |
| D check every other reader of an age or of the lineage count | **done** — §4 |
| E a test that walks the whole sequence, and one on the table and the queue alone | **done** — §5, both in `src/topology/fold/tests.rs` |
| F mutate the fix and watch the tests die | **done** — §6 |
| G the module notes brought into agreement with the code | **done** — §7 |
| H the six deferred findings left as they are | **done** — §8 |
| I ten gates green on this box | **done** at the head the pull request body records — §9 |
| J a draft pull request with a validated body; no review run, nothing marked ready, nothing enqueued | **done** — the body carries the ledger row `G4-LINEAGE-AGE-REUSED-AFTER-RELEASE` |
| K after review: the row and the rollback advice corrected to both directions, the test counts to the log | **done** — §10; this file's §2 and the pull request body; no code |

## 1. The defect, as found and as executed

At `74da2cbb`, `LeaseTable::grant` (`src/topology/leases.rs:100`) gave a new lineage
`age = self.lineages.len()`; `release` (`:120`) retained the survivors without renumbering them;
and `CandidateQueue::ineligible` (`src/topology/queue.rs:110`–`112`) held a lineage member behind
another lineage only through `lease.age < own_age`. The verification derived the overtake from
that code without executing it. The brief's first task was to execute it, so this was done before
anything was changed.

**The sequence, run through the fold's own doors** (the wide four-task plan the fold tests carry:
`zeta`, `alpha`, `mid`, `beta`, independent, `max_parallel` 3; keys 0–3; repairs registered as 4,
5 and 6), every event admitted by `plan_transition` and applied, in this order:

1. `zeta` and `alpha` dispatched, attempted, their candidates prepared and created.
2. Sequence 0: `merge_rejected`, a conflict, creates lineage **L0** rooted at `zeta` and registers
   repair 4. Sequence 1: the same for `alpha`, lineage **L1**, repair 5. Ages: L0 = 0, L1 = 1.
3. Repair 4 dispatched inside L0's lease, attempted, its candidate prepared (widening L0 by
   `src/Zebra`) and created; sequence 2 publishes it fast and `task_merged` releases L0. One live
   lineage remains, L1, which has no candidate.
4. `mid` dispatched, attempted, its candidate prepared and created; sequence 3: a conflict creates
   lineage **L2** rooted at `mid` and registers repair 6.
5. Repair 6 dispatched inside L2's lease, attempted, its candidate prepared with actual paths
   `src/alpha/lib.rs` and `src/mid` — it changed a path L1 holds — and the recorded effect
   `WidensLineage { root: mid, paths }` equal to those paths, which `check_lease_effect` accepts;
   the candidate created and queued.
6. The queue asked, the fold asked, and an integration start (`merge_verification_started` at
   sequence 4) for repair 6's candidate offered live and on replay.

**Observed on the frozen tree** (library code untouched; only the tests appended; the run is
`~/tactus-artifacts`' sibling evidence log `evidence-01-frozen-74da2cbb-three-tests.log`, kept
with this session's scratch files, and quoted here because the pull request body is the durable
place for it):

```
thread '…an_age_once_granted_is_never_reused_and_a_lineage_created_after_a_release_waits_behind_every_survivor' panicked
a lineage created after a release is younger than every survivor: 1 against 1
thread '…a_lineage_created_after_a_release_waits_behind_the_older_survivor_it_widens_onto_live_and_on_replay' panicked
a lineage created after a release is younger than every survivor: 1 against 1
thread '…g4fix_reproduction_the_frozen_fold_reuses_an_age_and_admits_the_overtake' panicked
REPRODUCED on the frozen tree: ages zeta=0 alpha=1 mid=1; younger candidate eligible; integration start accepted live and on replay
```

The third test was a temporary one whose assertions encoded the *frozen* behaviour and whose last
line panicked with the observed values only after every one of them held: L2 took age 1, L1's;
`CandidateQueue::ineligible` answered `None` for repair 6's candidate; the fold's
`eligible_integration_candidate` offered it; and `merge_verification_started` for it was accepted
by the live fold **and** by a replay of the log with it appended. It was deleted once it had been
observed; the two permanent tests are §5. So the overtake reproduces exactly as the verification
derived it, and one detail it left to be checked holds too: candidate creation legally widens L2
onto L1's region, because the checker compares the recorded widening with the diff's actual paths
and they match.

## 2. The freeze class

`src/topology/**` is frozen. The classes, as `reviews/2026-09-08-pr9-record.md` §5 states them:
Class A, no vocabulary change (a read-only reader or a constructor), free; Class B, fold
behaviour without a wire change, permitted with a declared row whose description matches the
code; Class C, wire vocabulary, an approval owed and not made in a slice.

**What the fix touches.** `LeaseTable` gains a private `next_age: u32`; `grant` takes a new
lineage's age from it and advances it; `release` is unchanged. `LineageLease::age` keeps its type
and its meaning — the run-local creation ordinal — and loses only its density.

**Not Class C.** The test is whether a correct fix needs a new or changed serialised field. It
does not. Nothing serialises the lease table: `LeaseTable` and `LineageLease` derive `Debug`,
`Clone`, `PartialEq`, `Eq` (and `Default`) and no serde trait; `age` names nothing in
`src/topology/events.rs`; no item outside `src/topology/` refers to `LeaseTable`, `LineageLease`
or `lineages()` (`grep -rn "LineageLease\|LeaseTable\|\.lineages()" src/ --include=*.rs` outside
that directory is empty; `src/status`, `src/export.rs` and `src/events` do not mention leases).
`TOPOLOGY_SCHEMA` does not move. Every byte of every `events.jsonl` is what it was.

**Reconstructs identically on replay.** The counter is a function of the order in which lineages
are created, and a lineage is created only by the `CreatesLineage` arm of `apply_merge_rejected`
(through `widen_lineage` and `grant`), so the n-th lineage the log creates takes age n − 1 on the
live path and on replay alike. This is the shape the fold already uses for `RunState::next_sequence`
(`src/topology/fold.rs`, "sequences are dense from 0 across the run"). `RunState` derives
`PartialEq` and the lease table is part of it, so the counter is inside every live-versus-replay
comparison the suite makes; the fold-level test of §5 ends with one and with the replayed age.

**Not Class A.** The fold's accept set changes, and in both directions. An integration start for
a lineage member — `merge_verification_started`, a fast `merge_prepared`, or a conflict's
`merge_rejected` — whose lineage was created after a release and overlaps an older live lineage
was admitted at `74da2cbb` and is now refused `NotFirstEligible` ("it is not eligible: it overlaps
the region the older lineage … holds"), live and on replay. And an integration start for the
*older survivor's* member, while a lineage created after a release overlaps it, was refused
`NotFirstEligible` at `74da2cbb` and is now admitted: after one release the ages were equal and
the frozen fold read the younger lineage's candidate, queued ahead of the survivor's, as eligible
and first ("task 6 generation 0 is queued ahead of it and eligible" in the executed sequence of
§10); after two or more the ages were inverted and it read the younger lineage as the older and
held the survivor's member behind its lease. The fixed fold holds the younger member behind the
survivor and offers the survivor's candidate. `eligible_integration_candidate` changes with both.
Either way it is fold behaviour, so:

**Class B, with this row.**

| # | Change | Where | Description (verified against the code at the commit that makes it) |
|---|---|---|---|
| G4FIX-B1 | An age, once granted, is never reused; a lineage created after a release is younger than every survivor | `src/topology/leases.rs` (`LeaseTable::next_age`, `grant`) | `grant` gave a new lineage the table's lineage count as its age, which a release shrinks, so a lineage created after one repeated a live lineage's age and the queue's `lease.age < own_age` no longer held its member behind the older survivor it overlapped: it could widen onto that survivor's region and publish ahead of it. The age now comes from a per-table counter that rises by one at each creation and is never lowered; a release leaves it alone, a holding replaced in place keeps its age. Changes what the fold admits in both directions, for lineage members only and only in a run that creates a lineage after releasing one older than a survivor. Refused now, admitted before: an integration start for such a member while the older overlapping lineage is held; it is admitted once that lineage settles. Admitted now, refused before: an integration start for the older survivor's member while the younger lineage overlaps it, which the frozen fold refused `NotFirstEligible` — after one release the ages were equal and it read the younger lineage's candidate, queued ahead, as eligible and first; after two or more the ages were inverted and it held the survivor's member behind the younger lineage's lease — and which the fixed fold admits, because the younger member is `BehindOlderLineage` and the survivor's candidate is the first eligible. Both directions hold live and on replay from one prefix; §10 has the two executed sequences and what a revert faces. Ages are sparse after a release and only their order is read. No wire form changes. |

Class A changes in the frozen layer: none. Class C: none.

**What the change means for an existing log, in both directions.** A log written by the frozen
fold that recorded an overtake — a younger member's integration start admitted in the first shape
above — now refuses at that event on replay. No such log is known to exist: PR9 shipped
`production_effect: none`, the schema-4 machinery engages only by explicit schema choice, and no
`0.2.0` has been released. Where one did exist the refusal would be the correct reading, because
the design orders overlapping lineages by creation (§7) and the record was written by a fold that
mis-ordered them.

The other direction is what a revert faces. A log written by the fixed fold that recorded the
survivor's integration start in the second shape — an older survivor's member integrated while a
lineage created after a release overlapped it — replays under the fixed fold and refuses under the
frozen one, at that event, `NotFirstEligible`. The correctness lens executed exactly this (§10): a
42-event log that replays with the fix and fails without it, its lineage ages `0, 1, 2, 3` read
back by the frozen fold as `0, 1, 2, 1`. Resume is replay-then-continue and there is no second
path (`DESIGN.md` §4), so a binary without the fix cannot resume such a run: `upstroke resume`
ends at the stable-prefix barrier's checked replay (`establish_stable_prefix` in
`src/events/log.rs`) with the fold's refusal as its explanation and nothing done — before any
recovery event, promotion, cleanup or admission, in the recovery order's words
(`docs/internals/engine/topology/recover.md`), the log and the run branch as they were — and does
so every time it is tried. Unlike the first direction this refusal is not the correct reading of
the record; it is the defect refusing a correctly ordered event. So a revert of this change is not
a no-op for durable state. Nothing needs migrating forward, but a run whose log was written under
the fix and holds such an event is stranded by the revert until a binary carrying the fix resumes
it; there is no migration tool and none is written here, and the log is ground truth and is not
rewritten. The pull request body's rollback paragraph now says so; its earlier wording, that
nothing durable had to be migrated either way because nothing durable changed, was wrong.

**The alternatives, and why the counter.** The creating rejection's `SequenceId` is durable, unique
and monotonic and would serve as the age; it is the same class, but it changes `grant`'s signature
and every direct caller, and `widen_lineage` would need it too. Renumbering the survivors densely
on every release would also keep a new lineage above them, but it makes an age mutable across a
lineage's life for no reader that needs density. The counter is the smallest change, mirrors
`next_sequence`, and gives the stronger property the brief prefers: never reused at all.

## 3. The fix

```
 pub struct LeaseTable {
     held: BTreeMap<LeaseOwner, PathSet>,
     lineages: Vec<LineageLease>,
+    next_age: u32,
 }
@@ pub fn grant
         if let LeaseOwner::Lineage { root } = owner {
-            let age = u32::try_from(self.lineages.len()).unwrap_or(u32::MAX);
             match self.lineages.iter_mut().find(|lease| lease.root == root) {
                 Some(existing) => existing.paths = paths,
-                None => self.lineages.push(LineageLease { root, paths, age }),
+                None => {
+                    let age = self.next_age;
+                    self.next_age = self.next_age.saturating_add(1);
+                    self.lineages.push(LineageLease { root, paths, age });
+                }
             }
             return;
         }
```

`saturating_add` is the fold's idiom for its other counters and shares their residual
(`SWEEP-FOLD-APPLY-SATURATING-COUNTERS`, P3): after `u32::MAX` creations two lineages would share
an age. §6 and §7 bind the hunk under the activation rule and it carries no `?`, no clone, no
index, no `unreachable!`, and no panic.

## 4. Every other reader of an age or of the count

The brief asks that `queue.rs:110`'s comparison be checked and that nothing else assume
contiguity or use `len()` as an identity. Read at `74da2cbb` and again at the fix:

- `src/topology/queue.rs:110`–`112`: `own_age` is the member's own lineage's age, or `u32::MAX`
  when that lineage is no longer held; `lease.age < own_age` over sparse ages orders exactly as
  over dense ones, and an absent own lineage still reads as younger than every live one, so every
  overlapping lineage holds it back. No contiguity is assumed. Untouched.
- `LeaseTable::any_candidate_or_lineage` reads `lineages.is_empty()`; `src/topology/census.rs:1747`
  reads `lineages().is_empty()`. Emptiness, not identity.
- `lineages()` is compared as a slice in three fold tests (a lineage untouched by an unrelated
  failure; the table before and after a refused release). Equality over the same table, not a
  count.
- `overlapping_lineages` yields "oldest first" in the notes' words, which is creation order —
  `Vec` push order — and is unchanged.
- No item outside `src/topology/` reads `age`, `lineages()` or the table (§2's grep).

`grep -rn "lineages.len\|\.age\b" src/ --include=*.rs` after the fix names only the queue's two
lines, the tests, and the fixed `grant`.

## 5. The tests

Both in `src/topology/fold/tests.rs`, both failing on the frozen tree with the messages of §1 and
passing at the fix (`cargo test --lib -- topology::` at the fix: `861 passed; 0 failed; 11 ignored`
on this box, Linux).

- `an_age_once_granted_is_never_reused_and_a_lineage_created_after_a_release_waits_behind_every_survivor`
  — the lease table and the queue alone: L0 and L1 granted (ages 0, 1), L0 released, L2 created
  through `widen_lineage` as the fold's `CreatesLineage` arm creates one, and widened onto L1's
  region. Asserts L2's age is above L1's; that `CandidateQueue::ineligible` answers
  `BehindOlderLineage { root: L1 }` for L2's member and `None` for L1's; that after a second release
  and a fourth lineage the granted ages are `0, 1, 2, 3`; and that a holding replaced in place
  keeps its age.
- `a_lineage_created_after_a_release_waits_behind_the_older_survivor_it_widens_onto_live_and_on_replay`
  — the whole sequence of §1 through the fold's doors, then further: L1's own repair queues
  *behind* repair 6 in position (`[6, 5]`) and goes *first* in eligibility; its publication at
  sequence 4 releases L1; repair 6's candidate is then the eligible one and its integration start
  at sequence 5 is accepted. The log replays to the live state (`fold.state() == replayed.state()`,
  ages included) and the replayed fold holds L2's age as 2.

The gate's own G4A (`two_lineages_publish_in_lineage_order_and_the_younger_candidate_waits_behind_the_older`
in `src/engine/topology/recover/tests.rs`) is left as it is: it never creates a lineage after a
release, which is why it and mutation M01 were green, and it stays green.

## 6. The mutations

Each applied to `src/topology/leases.rs` at the fix commit by an anchored replacement, the four
tests below run (`cargo test --lib`), the file restored from the commit and verified unchanged
(`git diff --quiet`). The detection set: the two tests of §5, the existing table-level test
`an_ordinary_candidate_waits_for_any_lineage_and_a_member_only_for_older_ones`, and G4A.

| Mutation (`src/topology/leases.rs`) | The two new tests | `an_ordinary_candidate_waits_for_any_lineage_and_a_member_only_for_older_ones` | G4A |
|---|---|---|---|
| **M1** — the age is the lineage count again: `let age = u32::try_from(self.lineages.len()).unwrap_or(u32::MAX)` in the creating arm, the counter unread (the defect restored) | **both die**: "a lineage created after a release is younger than every survivor: 1 against 1" | survives | survives |
| **M2** — the counter never advances: the `saturating_add` line removed | **both die**: "0 against 0" (the table test), `lineage_age(&fold, ALPHA) == 1` (the fold test) | **dies**: `assertion left == right failed` at its `age == 1` | **dies**: "and ineligible: it overlaps the older lineage's region, and that lineage has no candidate yet" |
| **M3** — `release` resets the counter to the surviving count: `self.next_age = u32::try_from(self.lineages.len()).unwrap_or(u32::MAX)` after the `retain` | **both die**: "1 against 1" | survives | survives |

Every run: `2 passed; 2 failed` for M1 and M3, `0 passed; 4 failed` for M2; the file restored and
`git diff --quiet` clean after each. M1 and M3 are the two shapes of the defect — reuse at creation,
reuse after release — and that the gate's two tests survive both is the gap the verification named:
neither creates a lineage after a release. The new tests do, and they die with the fix removed.

## 7. Notes and design

`docs/internals/topology/leases.md` (the `age` field, the new `next_age` field, `grant`),
`docs/internals/topology/queue.md` (the `own_age` comparison) and
`docs/internals/topology/fold/tests.md` (the new helpers and tests) say what the code now does;
the source files carry their single `Extended notes:` pointer and no other prose, as §13 asks.

`design/` is unchanged. The fix conforms to the design rather than amending it: `design/14`
("the repair's actual-path lease prevents known overlapping candidates overtaking it"), the
queue's stated rule that two lineages contending for one region resolve by creation order, and
G4's own remit ("lineage lease is the only overlap authority", "one per lineage; totally ordered").
No design sentence describes how the order is numbered, and none is added: the counter is an
implementation of an order the design already states.

## 8. Deferred findings this change must not disturb

`PR8-R2-SPEND-REPLAY`, `PR8-CRASH-002`, `PR249-ANSWER-TEXT-NOT-CARRIED`,
`PR249-KILL-SAMPLER-WINDOWS-WRAPPER`, `PR249-MANIFEST-NORMALIZATION-ALIAS` and
`PR249-REFUSED-MANIFEST-HANDOFF` are deferred by owner ruling with standing finding files. None is
reopened, repaired, narrowed or made newly reachable: the change touches one function of the lease
table and no event, no funnel, no sampler and no manifest.

## 9. Gates

The ten gates ran green on this box at the head the pull request body records, the first nine
through `w1-eight-iso` on a target directory private to this worktree and the tenth by hand; the
body carries the output and the observed counts. Linux only; the Windows and macOS legs are CI's
to report on the pull request.

## 10. The review's corrections

Three scoped frontier lenses reviewed `bcc17d8b`, the head that added this record, run by the
owner's review driver and not by this branch's sessions: regression **PASS**; correctness
**CHANGES_REQUIRED**, one P2; record **CHANGES_REQUIRED**, the same P2 and one P3. All three hold
the fix sound and Class B justified: the counter is unserialised, the schema constants did not
move, replay equality was established including the counter, the three mutations of §6 reproduced
exactly, and the named tests own the properties they claim. The correctness verdict is posted on
the pull request as its SHA-bound comment; the other two are saved on the build box
(`~/review-pr256-record.md`, `~/review-pr256-regression.md`). What they found, and what changed:

**P2 — "strictly narrows" was false, and the rollback advice that rested on it was unsafe.** Row
`G4FIX-B1` and the body's compatibility paragraph said the change strictly narrows what the fold
admits. It narrows in one direction and widens in the other. Two lenses executed the widening
independently; the third reasoned it from source (survivor and newcomer ages `2/1` before, `2/3`
after).

- *The record lens' sequence*, through the fold's doors on the frozen tree and at the fix from one
  identical prefix (`sha256 3bf6a8bf…`): create `zeta`'s and `alpha`'s lineages (ages 0, 1);
  publish `zeta`'s repair 4 and release its lineage; create `mid`'s lineage; queue `mid`'s repair 6
  widened onto `alpha`'s region, then `alpha`'s repair 5, so the queue is `[6, 5]`. Frozen: `alpha`
  and `mid` are both age 1, the fold offers repair 6's candidate, and
  `merge_verification_started(key 5, sequence 4)` for the survivor is refused `NotFirstEligible`
  ("task 6 generation 0 is queued ahead of it and eligible"), live and on replay. Fixed: the ages
  are 1 and 2, the fold offers repair 5's candidate, and the same event is accepted, live and on
  replay. Evidence: `~/tactus-artifacts/pr256-record-review-jveb6_r0/frozen-narrowing-witness.log`
  and `fixed-narrowing-witness.log` there, with the witness source beside them.
- *The correctness lens' sequence*, which is the one a revert faces: lineages L0, L1 and L2 (ages
  0, 1, 2); publish and release L0 and L1, leaving L2; create L3, widen its repair onto L2's region
  and queue L3's repair before L2's. At the fix L3 takes age 3 and L2's integration start is
  accepted. On the frozen tree L3 takes age 1 — below L2's, so that fold reads the newcomer as the
  older and holds the survivor behind its lease — and a replay of the log written at the fix
  refuses L2's integration start `NotFirstEligible`. The lens ran it as a 42-event JSONL log that
  replays with the fix and fails with the frozen implementation; its logs lived in the driver's
  scratch directory, since removed, and the verdict as posted carries the sequence.

This session executed neither sequence again — this round's brief forbids any change to code, a
temporary test included — and did not need to: the two executions agree with each other, with the
regression lens' reasoning, and with the queue rule read at the fix (`src/topology/queue.rs`,
`lease.age < own_age`: a survivor's member is held only behind a lineage whose age is below its
own, and after the fix no lineage created later has one). §2's "Not Class A" paragraph, row
`G4FIX-B1` and the paragraph on existing logs now describe both directions and what a revert
faces; the body's compatibility and rollback paragraphs say the same. The "Not Class A" paragraph
had also quoted a refusal detail the fold does not print; it now quotes the fold's own
(`ineligible_detail` in `src/topology/fold/region.rs`). The class does not change: both lenses
confirm Class B, and no wire form moves in either direction.

**P3 — the baseline test counts were attributed to the wrong binaries.** The body said the library
passed 1 test and the binary 2,435 with 43 ignored. The saved log
(`~/tactus-artifacts/g4fix-evidence-bcc17d8b/eight-logs-bcc17d8/03-test.log`) says the library
(`src/lib.rs`) passed 2,435 with 43 ignored (line 2501) and the binary (`src/main.rs`) passed 10
(line 2517); the single-test line (line 100, `2477 filtered out`) belongs to a child process the
library's tests spawn to run the isolated `native_pipe_cancellation_helper` witness. The body now
reports the counts of the gate run at the head it records, labelled by binary.

**Untouched, deliberately.** No code: `src/topology/leases.rs`, the two tests and the three module
notes are as `f3a71c1d` left them, and the regression lens checked the notes against the code. The
six deferred findings of §8 stay deferred.
