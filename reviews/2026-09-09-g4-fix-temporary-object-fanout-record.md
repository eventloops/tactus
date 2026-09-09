# G4 fix — the residue classifier never looked where Git puts its temporary object: the record

The record of the repair of the P1 that the third independent frontier verification of G4's report
found in the frozen tree at `82de0767` (`~/tactus-artifacts/G4-VERIFICATION-3.md`, finding 1: *the
corrected row still lacks the evidence required for automatic passage*). Kept on the branch so the
pull request's reviewer and every later gate can read what was executed, what the change can and
cannot reach, and what kills its tests. It is **not** a design document: `DESIGN.md` stays the
authority, and a sentence here that disagrees with it is a defect in this file.

**Branch:** `fix-P1/correctness_temporary-object-files-does-not-scan-the-fan-out-directories`, cut
from `master` at `81ee09efb5926fb4b93565223ea05dd0052bc2f9` — the merge of #256, and G4's frozen
input range.

**Where this file lives, and why.** The same reasoning as
`reviews/2026-09-09-g4-fix-lineage-age-record.md`: no new file at the repository root, and
`docs/internals/` holds only files that mirror a Rust module, so the record sits beside the gate
reports in `reviews/`, dated.

## 0. Status

| Step | State |
|---|---|
| A reproduce the counterexample the verification derived, on the frozen tree, before any change | **done** — §1; three ways, including a real `SIGKILL` inside a real Git write |
| B establish what the change can reach before making it | **done** — §2: outside the `src/topology/**` freeze, and no production outcome moves |
| C the fix | **done** — §3, `src/workspace_manager.rs` |
| D check every other reader of the predicate | **done** — §4; one production caller, provably unaffected |
| E tests: the element where Git leaves it, and every element constructed alone | **done** — §5 |
| F mutate the fix and watch the tests die | **done** — §6, five mutations |
| G the module notes brought into agreement with the code | **done** — §7 |
| H the deferred findings left as they are | **done** — §8 |
| I ten gates green on this box | **done** at the head the pull request body records — §9 |
| J a draft pull request with a validated body | **done** — the body carries the ledger row `G4-TEMP-OBJECT-FANOUT-UNSCANNED` |
| K three scoped `gpt-6-astra` max reviews, and their repairs | **done** — §10; two blocking P1s fixed, three prose corrections made |

## 1. The defect, as found and as executed

At `82de0767`, `temporary_object_files` (`src/workspace_manager.rs:4507`) scanned the object
directory's **root** for `tmp_obj_` and **`pack`** for `tmp_pack_`. Its doc said Git "writes a loose
object to `objects/tmp_obj_XXXXXX`". `classify_object_residue` reads it for
`ResidueElement::TemporaryObjectFile`. The verification derived the counterexample from that code
without executing it, and said so. The first task was to execute it.

### 1.1 Where Git actually writes the file

`strace -f -e trace=openat,rename,link,unlink` on git 2.43.0
(`~/tactus-artifacts/tmpobj-evidence/02-strace-where-git-writes.log`):

```
openat(AT_FDCWD, ".git/objects/01/tmp_obj_z86GbB", O_RDWR|O_CREAT|O_EXCL, 0444) = 4
link(".git/objects/01/tmp_obj_z86GbB", ".git/objects/01/74d67c349a9f9c9fc9b225c3897ebad6cf374b") = 0
unlink(".git/objects/01/tmp_obj_z86GbB") = 0
```

and for `write-tree`, `objects/bf/tmp_obj_GgXRvX`. The temporary file is created in **the fan-out
directory the object's final name will live in** — Git creates that directory first when it is
missing, which is the `ENOENT` retry the trace also shows — and never at the object root. The scan
read a place Git does not write.

### 1.2 The counterexample, executed, with Git's real artefact

A real `SIGKILL` at 1.939 s of a 3.878 s loose-object write, through the engine's own fixture and
its own classifier, in a repair worktree that was quiescent and whose store held no unreachable
object (`01-real-kill-frozen.log`; the temporary test is
`04-temporary-reproduction-test.patch`, deleted once observed):

```
REPRODUCED on the frozen tree (attempt 1: SIGKILL at 1.93908743s of a 3.878174861s write,
git signal: 9 (SIGKILL)):
  on disk under objects/: ["b7/tmp_obj_ybqfZf (122179584 bytes, mode 444)"]
  `count-objects -v`: garbage: 1
  `prune -n`: "Removing stale temporary file .git/objects/b7/tmp_obj_ybqfZf"
  unreachable objects: 0
  temporary_object_files = false
  observed_residue_elements(Object.RepairMaterialize) = []
  classify_object_residue = None
  Worktree.Verify passes = true
```

Git itself names the file its own stale temporary file. The classifier saw nothing, observed no
element, and answered `None`. The corrected row 10 requires *"every element constructed alone
classifies `Internal`"*, so the correction as proposed would not have delivered a pass. The
verification's finding 1 is upheld exactly as written.

### 1.3 Why the suite was green

The synthetic test constructed its stand-in at `objects/tmp_obj_repair` — the object root, which is
where the scan looked and where no Git writes. Test and scan were self-consistent and blind
together. §6's mutation M4 executes the pair: the old construction against the frozen scan is
**green**.

### 1.4 And which files are Git's

`resource_accounting[R27]` says "Git prunes temporary object files itself", so `git prune` is the
authority on which files these are. One candidate name planted in each plausible place, read back
through `git prune -n` (`03-git-prune-own-set.log`):

| planted | `git prune` calls it |
|---|---|
| `objects/tmp_obj_root`, `objects/tmp_other_root` | a stale temporary file |
| `objects/pack/tmp_pack_p`, `objects/pack/tmp_idx_p` | a stale temporary file |
| `objects/ab/tmp_obj_fanout` | a stale temporary file |
| `objects/ab/tmp_other_fanout` | a **bad sha1 file**, left as garbage |
| `objects/info/tmp_info` | nothing; left alone |

So the old scan was narrow in the root and in `pack` as well as blind to the fan-out: it required
`tmp_obj_`/`tmp_pack_` where Git prunes any `tmp_` name. Two of the five files Git calls its own
went unseen even in the directories the scan did read. This was found by measurement, not by the
verification, and the fix takes Git's own set rather than the one the doc asserted.

## 2. What the change can reach

**Outside the freeze.** `src/topology/**` is frozen; `src/workspace_manager.rs` is not, and nothing
under `src/topology/` is touched. The Class A/B/C taxonomy of
`reviews/2026-09-08-pr9-record.md` §5 does not apply, but the same question does, and it is
answered here rather than assumed.

**No wire vocabulary, and nothing durable.** `ObjectResidue` derives `Serialize`/`Deserialize` but
names nothing in `src/topology/events.rs` or under `src/events/` — no event records a residue
class, and `git grep -n 'ObjectResidue' src/topology/events.rs src/events/` is empty. The one
serialised occurrence is `ResidueClassExport::classified_as`
(`src/topology/effects/export.rs:45`), which is filled from `ResidueClass::classified_as()`, a
`const fn` on the frozen registry — the *declared* class of a residue class, not a live reading. It
does not move, so `effect_sites.json` is byte-identical. `TOPOLOGY_SCHEMA` does not move. Every
byte of every `events.jsonl` is what it was, and no log written by either version replays
differently under the other: resume is replay-then-continue, and the fold reads nothing this
touches.

**No production outcome moves.** §4 has the argument in full: `classify_object_residue` has exactly
one production caller and its result is unchanged in every case.

**What does change** is the engine's *reading* of what a killed command left, which is the
inspection predicate `command_internal_sub_effects` writes the residue-class evidence rule against.
The change is a widening in one direction only: `temporary_object_files` answers `true` for files it
could not previously see, never `false` for one it could. A worktree that classified `Internal`
still does.

## 3. The fix

`temporary_object_files` answers for the set `git prune` removes, in every place Git leaves one:
any `tmp_` name in the object root or in `pack`, and a `tmp_obj_` name in a two-hexadecimal-digit
fan-out directory. A `tmp_` name in a fan-out that is not `tmp_obj_` is Git's *garbage*, not its
temporary file (§1.4), and is deliberately not one of these.

```
 pub fn temporary_object_files(worktree: &Path) -> Result<bool, UpstrokeError> {
     let object_dir = object_directory(worktree)?;
-    for (directory, prefix) in [
-        (object_dir.clone(), "tmp_obj_"),
-        (object_dir.join("pack"), "tmp_pack_"),
-    ] { ... }
+    if directory_holds_name_prefixed(&object_dir, "tmp_")?
+        || directory_holds_name_prefixed(&object_dir.join("pack"), "tmp_")?
+    {
+        return Ok(true);
+    }
+    for fan_out in fan_out_directories(&object_dir)? {
+        if directory_holds_name_prefixed(&fan_out, "tmp_obj_")? {
+            return Ok(true);
+        }
+    }
     Ok(false)
 }
```

with the loop body extracted to `directory_holds_name_prefixed` — a missing directory holds nothing,
which is what the old `continue` said — and `fan_out_directories`, which admits only names of
exactly two hexadecimal digits that are directories.

**Bounded cost.** Only two-hex names are descended into, so `pack`, `info` and anything else in the
object directory cost the one `read_dir` that names them. The walk is bounded by the store's loose
objects and is made on the residue classifier's path, beside the `git fsck --connectivity-only`
that the same classification runs for `UnreferencedObject`; it is not on `Worktree.Verify`'s path,
which reads neither (`WorkspaceManager::quiescence`, `src/workspace_manager.rs:2228`).

**Windows and macOS.** The scan is `std::fs::read_dir` and `std::path` joins, with no separator or
case assumption: the fan-out test is on the name's characters, and `starts_with` is on the name, not
on a path string. An entry whose `file_type()` cannot be read is skipped rather than failing the
scan — that is a name that went away between the read and the question, which is what a store Git is
pruning concurrently looks like, and a name that is gone holds no temporary object file.

## 4. Every other reader of the predicate

`temporary_object_files` is read in exactly one place outside tests:
`internal_residue_present` (`src/workspace_manager/residue.rs:376`), for
`ResidueElement::TemporaryObjectFile`. That feeds `observed_residue_elements` and
`classify_object_residue`.

`classify_object_residue` has exactly one production caller — `verify_object`
(`src/engine/topology/candidate.rs:524`), for `Object.CandidateCommitTree`:

```
if residue != ObjectResidue::After {
    return Err(Refusal::ObjectMissing { ... }.into());
}
```

Its outcome cannot move **for any inspection that succeeds**. `After` for that site is
`object_exists(published)`, which this change does not touch, and `after_reference_present` is
consulted **before** any residue element; so when the object exists the answer is `After` either
way, and when it does not the answer is `Internal` or `None` — both of which take the same
`!= After` branch to the same refusal.

**One case is not the same, and an earlier draft of this section said it was.** The scan now opens
directories the old one did not, so it can fail where the old one could not: with an unreadable
two-hex directory in the object store, `verify_object` returns `UpstrokeError::Io`
(`PermissionDenied`) through its `?` instead of reaching the `Refusal::ObjectMissing` branch. Two of
#258's three review lenses executed exactly that. Both versions refuse the promotion and neither
admits a candidate the other would reject, so nothing is admitted that was not admitted before — but
the *error* a caller sees differs, and the equivalence claim holds only for successful inspections.
This is why `fan_out_directories` propagates an inspection failure rather than swallowing it: a
store this cannot read is a fact the caller is entitled to, and a silent `false` would be the same
blindness this change exists to remove.

The recovery path does not read it at all. `resume_open_no_attempt`
(`src/engine/topology/dispatch.rs:297`, called in production from
`src/engine/topology/run.rs:1073`) verifies through `verify_or_recreate` →
`WorkspaceManager::quiescence`, which reads registration, the worktree's own git dir and HEAD, and
never the object store — the property `element_breaks_quiescence` records and
`~/tactus-artifacts/G4-ROW10-DECISION.md` §4 argues must stay. So no worktree is recreated that was
not recreated before, and none is reused that was not reused before.

## 5. The tests

**`temporary_object_files_answers_for_the_files_git_prunes_as_its_own`**
(`src/workspace_manager/tests.rs`). The table of §1.4, executed: each of the seven names planted
**alone**, the scan read, the name removed, and the store read again to confirm it is back to none.
The two negatives are as much of the test as the five positives — `objects/ab/tmp_other_fanout` is
Git's garbage and `objects/info/tmp_info` is nobody's.

**`the_temporary_object_scan_answers_no_for_a_store_of_ordinary_objects`** (same file). A store of
real loose objects answers `no`; a directory named `abc` — two hexadecimal digits and one more — is
not a fan-out and a `tmp_obj_` name inside it is not one of Git's; a fan-out holding only objects
holds no temporary file.

**`repair_materialization_synthetic_residue_recreated_after_forced_removal`**
(`src/engine/topology/dispatch/tests.rs`), restructured. Two changes:

*The element is constructed where Git leaves it* — in a fan-out directory the store already holds,
beside real objects, rather than at the object root.

*Each element is constructed alone, in a repository of its own.* This is the shape
`synthetic_git_add_residue_unreferenced_objects_and_index_lock_then_forced_scrub_converges` already
uses for `Object.CandidateStage`, for the same reason and with its own measurement behind it
("planting them in sequence in one repository would leave the second element's slot carrying the
first's … Measured: it did"). `Object.RepairMaterialize` records no published object, so
`UnreferencedObject` is observed whenever the store holds *any* unreachable object, and the orphan
this test constructs for that element is deliberately never deleted — that is R27, and it is
asserted. Measured on the shared-store form: the orphan constructed for the first element was still
there supplying the `Internal` that the second element's assertion read, which is the verification's
own second point, executed. The four elements now get four runs.

The class is read **twice** per element: before the construction, where
`observed_residue_elements` must be empty and the class must be `None`, and after, where the answer
must be exactly `vec![element]`. The pair is what makes the `Internal` between them this element's
own — an assertion that the element *caused* the reading, not that the reading happened to hold.

## 6. The mutations

`05-mutations.log` (M1–M5, the first round) and `06-review-repair-mutations.log` (M6–M8, the
review's repairs). Every branch of the fix has a test that dies without it.

An earlier draft of this table overstated three rows; the review of #258 compared it against the
saved logs and was right. What follows is read off those logs line by line.

| # | Mutation | What the log records |
|---|---|---|
| M1 | the whole fix reverted, the tests kept | **two** die: `temporary_object_files_answers_for_the_files_git_prunes_as_its_own` at `tmp_other_root`, and the synthetic test — at its direct `temporary_object_files` assertion, not at the singleton assertion an earlier draft named. The negative test **passes**, which is what a negative test does under a narrowing |
| M2 | only the fan-out loop removed | the same two; the unit test names `ab/tmp_obj_fanout` |
| M3 | the root and `pack` prefixes narrowed back to `tmp_obj_`/`tmp_pack_` | the unit test, naming `tmp_other_root`. The synthetic test survives, correctly — it constructs in the fan-out |
| M4 | the element constructed at the object root again *and* the fix reverted | **the synthetic test passes.** The positive unit test still fails, on `tmp_other_root`, because it tests the whole prune set rather than this one placement. The load-bearing half is the synthetic test: the old construction against the old scan is green, which is why the suite could be green while the defect was live. An earlier draft said "nothing died", which the log does not support |
| M5 | the fan-out name test loosened from "exactly two hex" to "two or more hex" | the negative test, on the `objects/abc` decoy |
| M6 | the symlink repair reverted to `DirEntry::file_type` | `a_symlinked_fan_out_directory_is_followed_as_git_follows_it` |
| M7 | the lower-case fan-out rule loosened back to any hex digit | the negative test, on the `objects/AB` case. **This row was added because the first attempt at M7 killed nothing**: the tightening had no test until one was written for it |
| M8 | `git prune --expire=now` injected into the forced recreation | `a_forced_recreation_preserves_the_object_store_residue_it_recovers_over`, at its R27 assertion. This is the review's own mutation, which the shared-store test used to catch and the isolated one did not |

M4 is the row that matters for the gate: it is why "the synthetic test passes" was never evidence
that the element classifies. M8 is the row that matters for R27: it is why isolating the elements
needed a second test rather than none.

## 7. The notes

`docs/internals/engine/topology/dispatch/tests.md`: the section for the synthetic test rewritten to
say what it now does and why — a repository per element, the two readings, and the fan-out — with
sections added for `synthetic_materialization_residue_element` and `fan_out_directory`. No other
notes file mirrors a module this change touches; `src/workspace_manager.rs` carries no
`Extended notes:` marker, so its function doc is the record and it carries the measurements.

## 8. The deferred findings

Untouched and still deferred, by the owner's ruling: `PR8-R2-SPEND-REPLAY`, `PR8-CRASH-002`,
`PR249-ANSWER-TEXT-NOT-CARRIED`, `PR249-KILL-SAMPLER-WINDOWS-WRAPPER`,
`PR249-MANIFEST-NORMALIZATION-ALIAS`, `PR249-REFUSED-MANIFEST-HANDOFF`. Nothing here claims any of
them is fixed, and nothing here depends on one.

## 9. What G4 still needs, and what this does not do

This fixes the classifier. It does **not** make G4 pass on its own, and it does not touch the gate's
report.

Row 10 as written in the packet requires the interrupted-materialization residue to fail
`Worktree.Verify` so the worktree is recreated, and `~/tactus-artifacts/G4-ROW10-DECISION.md` rules
that the row over-specifies a mechanism for the two object-store elements and should be corrected
rather than the code changed. **That correction is owed to the design packet and is the owner's
act** — the packet is not in this repository and nothing here edits it. What this change does is
make the corrected row's other requirement — that every element constructed alone classifies
`Internal` — true, which the third verification found it was not.

G4 must therefore re-run on the range this merge creates: the code sha moves, so the range moves,
and the packet forbids amending a failed gate's report.


## 10. The review round, and what it changed

Three scoped `gpt-6-astra` max lenses on `da2298fc` — closure, regression, and evidence. **All three
returned `CHANGES_REQUIRED`, and all three were right.** Two carried executed reproductions, which
`MAINTAINING.md` fixes whatever the label.

### 10.1 The fix did not close its own defect — a symlinked fan-out (P1, closure lens)

`DirEntry::file_type` reports the **link**, not its target, so `objects/93 -> elsewhere` was walked
past. Git's loose-object writer and its prune traversal both follow it. The lens executed a real
`SIGKILL` inside a `hash-object -w` in such a store and read back:

```
residue: objects/93/tmp_obj_HuHQtZ (32768 bytes)
git prune -n: Removing stale temporary file .git/objects/93/tmp_obj_HuHQtZ
unreachable_objects: []
temporary_object_files: false
observed_residue_elements: []
classify_object_residue: None
```

— the original defect, surviving its own repair. It then changed only the fan-out layout in this
change's own fixture to a symlink and watched the temporary-object assertion fail with the
implementation untouched.

Repaired: `fan_out_directories` resolves each candidate with `fs::metadata`, which follows the link,
and the new test `a_symlinked_fan_out_directory_is_followed_as_git_follows_it` constructs the
layout, plants, unplants, and leaves a dangling link behind to show that a name that is gone is not
an inspection failure. Mutation M6 kills it.

### 10.2 Isolating the elements lost the R27 guard on recreation (P1, regression lens)

The per-element isolation this change introduced put the two object-store elements in exactly the
runs whose recovery **reuses** the worktree, so nothing exercised preservation across a **forced
recreation**. The shared-store form covered it by accident: its undeleted orphan survived into the
`IndexLock` and `CherryPickHead` iterations, which do recreate.

The lens proved the gap by mutation — `git prune --expire=now` injected into the forced-recreation
branch killed the old test at its R27 assertion and the new one accepted it.

Repaired by a second test rather than by weakening the first:
`a_forced_recreation_preserves_the_object_store_residue_it_recovers_over` constructs both
object-store elements **and** `index.lock`, asserts the recovery really recreated, and asserts both
survive. It deliberately asserts nothing about classification — mixing the two is what made the
per-element evidence vacuous to begin with. Mutation M8, the lens's own, kills it.

### 10.3 Three prose corrections

- **The mutation table contradicted its own saved log** (closure and evidence lenses). §6 is rewritten
  from the logs line by line; the "both new tests" and "nothing died" rows were wrong.
- **"No production outcome moves" omitted a newly reachable error** (regression and evidence lenses).
  §4 now says the equivalence holds for successful inspections, and names the `PermissionDenied`
  case.
- **The quoted reproduction figures came from an unsaved run** (evidence lens). The reproductions
  were run twice — once interactively and once to write the evidence files — and the record quoted
  the first. Every figure now matches the saved log, and the timing is described as what the harness
  actually does: a kill requested at half of a separately measured uninterrupted write, not a
  measured elapsed time.

Also corrected without a finding: the lower-case fan-out rule had **no test** until mutation M7 was
first run and killed nothing. The negative test now carries an `objects/AB` case.

### 10.4 What the lenses did not fault

Worth recording, because it is the part of the argument that held. The closure lens executed
alternates, `GIT_OBJECT_DIRECTORY`, linked worktrees and separate git directories, bare and
SHA-256 repositories, packed-only stores, pack and maintenance temporaries including `tmp_rev_*`,
and loose-object naming across git 2.30, 2.50.1, 2.55 and Git for Windows 2.43 — no further closure
failure. The regression lens found `src/topology/**` byte-identical and no changed census row,
frozen count, golden inventory or durable residue-class consumer. The evidence lens re-executed all
seven prune-table cases, confirmed the four repositories really are distinct, confirmed that
omitting an element fails its singleton assertion and that adding an orphan beside `IndexLock`
yields `[UnreferencedObject, IndexLock]`, checked both first-bad commits and the `82de0767` /
`81ee09ef` code-identity claim, and stated: **"The per-element evidence is now real."** That is the
sentence the third G4 verification's finding 1 asked for.
