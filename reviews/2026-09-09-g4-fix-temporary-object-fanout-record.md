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

Evidence filenames below are relative to `~/tactus-artifacts/tmpobj-evidence/` unless another
directory is named. Saved review verdicts are evidence of what those reviewers executed or reasoned;
they are not claims that this repair session repeated those executions.

## 0. Status

| Step | State |
|---|---|
| A reproduce the counterexample the verification derived, on the frozen tree, before any change | **done** — §1; three ways, including a real `SIGKILL` inside a real Git write |
| B establish what the change can reach before making it | **done** — §2: outside the `src/topology/**` freeze; successful inspections preserve verification results, while inspection errors can change |
| C the fix | **done** — §3, `src/workspace_manager.rs` |
| D check every other reader of the predicate | **done** — §4; one production caller, unchanged results for successful inspections, a disclosed inspection-error path (`PermissionDenied`, `FilesystemLoop`, `NotADirectory`), and the recovery reach stated as it is |
| E tests: the element where Git leaves it, and every element constructed alone | **done** — §5 |
| F mutate the fix and watch the tests die | **done** — §6 separates the historical surviving M7 from the saved current M7, M9 and M6 failures; §6 and §12 add round 5's eleven mutants, all dead, and the restored controls |
| G the module notes brought into agreement with the code | **done** — §7 |
| H the deferred findings left as they are | **done** — §8 |
| I ten gates green on this box | **done** — §11.4 for round 3, §12.7 for round 5; each run describes the tree it names by source hash |
| J a draft pull request with a validated body | PR #258 exists; this implementation session prepares a corrected local body for the driving session, which owns git and publication |
| K scoped reviews and their repairs | **implemented** — §10 records the first round; §11 the second; §12 the third, three `claude-opus-5` max lenses on `2b048a67`, all `CHANGES_REQUIRED`, and the repairs of every finding they carried |

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

and for `write-tree`, `objects/bf/tmp_obj_GgXRvX`. The temporary file of the common loose
write is created in **the fan-out directory the object's final name will live in** — Git
creates that directory first when it is missing, which is the `ENOENT` retry the trace also
shows. Those are the two commands the trace covers, and neither writes anywhere else.

**Git does write at the object root, and in `pack`, and the predicate has three arms
because all three places are real.** An earlier draft of this section generalised the two
traced commands into "never at the object root"; that universal was false, was copied into
the shipped rustdoc, a test comment and the internals notes, and is withdrawn (§12.2). Traced
in round 5 with the same `strace` on the same git
(`20-r5-strace-git-writes-in-three-places.log`):

- a **streamed** loose write — an object above `core.bigFileThreshold`, whose oid and so
  whose fan-out is unknown until the stream ends — goes to the object root:
  `git unpack-objects` with the threshold at 512 opened `.git/objects/tmp_obj_KSwW4k` and
  linked it to `.git/objects/88/fb3fef…`, while the three small objects in the same pack
  went to `8b/`, `ff/` and `ac/`. Any `unpack-objects`, `index-pack`, fetch or clone over the
  threshold reaches it, in the store the engine shares with the user's own git;
- bulk checkin above the threshold goes to `pack`: `hash-object -w` and `git add` of a
  100 000-byte file at the same threshold opened `.git/objects/pack/tmp_pack_2bgAj0` and
  `.git/objects/pack/tmp_idx_HebZJG` and renamed them to `pack-156f22….pack` and `.idx`;
- the common loose write, re-traced: `hash-object -w` opened `.git/objects/20/tmp_obj_XybLdf`
  and `write-tree` opened `.git/objects/65/tmp_obj_2Kql8B`.

The scan at `82de0767` read the root and `pack` — two of the three — and never the fan-out
the common write uses.

### 1.2 The counterexample, executed, with Git's real artefact

A real `SIGKILL` during a loose-object write, through the engine's own fixture and classifier, in a
repair worktree that was quiescent and whose store held no unreachable object. The harness measured
a separate uninterrupted write, then requested a delay of half that duration before killing the
next write; it did not measure elapsed time to the kill (`01-real-kill-frozen.log`; the harness is
`04-temporary-reproduction-test.patch`, deleted from the source once observed). The saved output
uses "SIGKILL at" for that requested delay:

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

The synthetic test constructed its stand-in at `objects/tmp_obj_repair` — the object root,
the one place the scan looked, and not where a materialization's own loose write goes (the
root is where Git puts a *streamed* object, §1.1; a cherry-pick's objects are not streamed).
Test and scan were self-consistent and blind together. §6's mutation M4 executes the pair:
the old construction against the frozen scan is **green**.

### 1.4 And which files are Git's

`resource_accounting[R27]` says "Git prunes temporary object files itself", so `git prune` is
the authority on which files these are. One candidate name planted in each plausible place,
read back through `git prune -n` (`03-git-prune-own-set.log`):

| planted | `git prune` calls it |
|---|---|
| `objects/tmp_obj_root`, `objects/tmp_other_root` | a stale temporary file |
| `objects/pack/tmp_pack_p`, `objects/pack/tmp_idx_p` | a stale temporary file |
| `objects/ab/tmp_obj_fanout` | a stale temporary file |
| `objects/ab/tmp_other_fanout` | a **bad sha1 file**, left as garbage |
| `objects/info/tmp_info` | nothing; left alone |

So the old scan was narrow in the root and in `pack` as well as blind to the fan-out: of the
seven names planted, it required `tmp_obj_`/`tmp_pack_` where Git prunes every `tmp_` name
at the root and in `pack` and the `tmp_obj_` name in a fan-out. Two of the five files Git
calls its own went unseen even in the directories the scan did read. This was found by
measurement, not by the verification, and the fix takes Git's own set rather than the one
the doc asserted.

**That is a statement about seven planted names, not a grammar of Git's producers.** An
earlier draft wrote "Git prunes any `tmp_` name", which the sample cannot support, and two
producers sit outside it (`21-r5-git-prune-producers-outside-the-sample.log`, executed in
round 5 with the same `git prune -n`):

| planted | `git prune` calls it | the predicate answers |
|---|---|---|
| `objects/tmp_objdir-incoming-AbCdEf/`, a **directory** — `receive-pack`'s quarantine, live for the length of a push | a stale temporary **directory** | `true`: the root arm matches the name and does not read the entry's type |
| `objects/.tmp-1-pack-x.pack`, `objects/pack/.tmp-1-pack-y.pack` — `repack`'s in-flight pack | **nothing**; `repack` cleans its own, `prune` never names it | `false`: not a `tmp_` name, and outside R27's sentence |
| `objects/00/tmp_obj_first`, `objects/ff/tmp_obj_last` — the two ends of the fan-out range | a stale temporary file | `true` (§12.4) |
| `objects/pack/tmp_rev_p` — the reverse index | a stale temporary file | `true` |

The quarantine directory is the one directory the predicate answers for. upstroke never
creates one — it runs no `push`, `fetch`, `clone` or `receive-pack` — so it appears only if
a person or another tool pushes into the repository while a run is in flight; it is a row in
the test's table now (§5), read against `git prune -n` in the test itself, and it is recorded
as `PR258-ROOT-ARM-MATCHES-QUARANTINE-DIRECTORY`. The `.tmp-` files are two negative rows of
the same table.

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

**Successful inspections preserve verification results.** §4 has the argument in full:
`classify_object_residue` has one production caller, `verify_object`, which returns the same result
for every inspection that succeeds. The widened scan can also return `Io(PermissionDenied)` where
the old scan reached `Refusal::ObjectMissing`; that observable error difference is part of the
change.

**What does change** is the engine's *reading* of what a killed command left, which is the
inspection predicate `command_internal_sub_effects` writes the residue-class evidence rule against.
Relative to the frozen root-and-pack scan, successful inspections retain the old detections and
add fan-out temporary files. Newly inspected directories can instead produce an inspection error;
neither predicate results nor error results are unconditionally equivalent across the change.

## 3. The fix

`temporary_object_files` answers for the set `git prune` removes, in every place Git leaves
one: any `tmp_` name in the object root or in `pack`, and a `tmp_obj_` name in a directory
resolved through one of Git's canonical lower-case fan-out paths, `00` through `ff`. A `tmp_`
name in a fan-out that is not `tmp_obj_` is Git's *garbage*, not its temporary file (§1.4),
and is deliberately not one of these.

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
+    for fan_out in fan_out_directories(&object_dir) {
+        if directory_holds_name_prefixed(&fan_out?, "tmp_obj_")? {
+            return Ok(true);
+        }
+    }
     Ok(false)
 }
```

with the loop body extracted to `directory_holds_name_prefixed` — a missing directory holds
nothing, which is what the old `continue` said — and `fan_out_directories`, an iterator that
resolves each canonical lower-case fan-out path with `fs::metadata` and yields it as it is
probed. Directory symlinks are followed. On a case-insensitive filesystem a path such as `f3`
can resolve to a directory stored as `F3`; on a case-sensitive filesystem a distinct `F3`
directory is outside Git's canonical traversal.

**Round 5 (§12).** The probe is streamed into the read: round 4's `fan_out_directories`
collected all 256 candidates into a `Vec` before the caller read any, so a symlink loop at
`objects/ff` turned a store whose `objects/00` held residue from `Ok(true)` into
`Err(FilesystemLoop)`; the iterator reads each fan-out before the next name is probed, and
the loop at `ff` is reached only once nothing earlier has answered.
`directory_holds_name_prefixed` opens the directory and hands the listing to
`holds_name_prefixed`, which is where a listing that fails part-way through is read as an
error rather than as its end — kept apart so that failure can be constructed, since no
filesystem the suite runs on fails a `readdir` to order.

**Bounded cost.** Fan-out discovery performs at most 256 metadata lookups regardless of the
names or number of entries stored at the object root, and stops at the first match. Only
resolved fan-out directories are read, at a fixed depth; object contents are not read. This
is on the residue classifier's path, beside the `git fsck --connectivity-only` that the same
classification runs for `UnreferencedObject`; it is not on `Worktree.Verify`'s path, which
reads neither (`WorkspaceManager::quiescence`, `src/workspace_manager.rs:2228`). The
function's rustdoc said, in a paragraph that survived the round-3 rewrite verbatim, that
"only names of exactly two hexadecimal digits are descended into … the walk is bounded by
the loose objects in the store" — round 2's implementation, deleted in round 3; that
paragraph now describes the 256-name probe (`PR258-RUSTDOC-COST-PARAGRAPH-STALE`).

**Filesystem lookup and errors.** All paths use `std::path` joins. `fs::metadata` resolves
Git's lower-case path through the filesystem's own case and symlink rules, and `starts_with`
checks an entry name rather than a path string. Only `NotFound` is treated as absence; every
other inspection error propagates: `PermissionDenied` for a fan-out this process cannot read,
`FilesystemLoop` for a fan-out symlink that loops, `NotADirectory` for one that leads through
a regular file. A regular file *at* a two-digit name resolves, is not a directory, and is
skipped rather than read. This preserves a missing-directory race without hiding a store that
cannot be inspected — on Unix. **On Windows the race is not a skip:** a directory another
process is deleting is delete-pending and answers `ERROR_ACCESS_DENIED`, which is
`PermissionDenied` and not `NotFound`, until the deleter's handle closes — the shape this
repository measured on the Windows guest for `remove_tree_once_handles_close` and for
`runner::container`'s `RACING_ACCESS_ATTEMPTS` (`docs/internals/runner/container.md`,
"delete-pending"), cited here rather than re-derived. A fan-out that a concurrent `git gc` is
removing is therefore an inspection error there rather than the skip it is on Unix, on a path
that refuses either way (§4). Native Windows and macOS behaviour of this scan has not been
executed in any repair session; its portability claims are reasoned from these standard-library
operations and from that measured precedent.

## 4. Every other reader of the predicate

`temporary_object_files` is read in exactly one place outside tests:
`observed_residue_elements` (`src/workspace_manager/residue.rs:376`), for
`ResidueElement::TemporaryObjectFile`. That feeds `internal_residue_present` (`:327`, whose
whole body is `Ok(!observed_residue_elements(site, target)?.is_empty())`) and through it
`classify_object_residue`. (An earlier draft named `internal_residue_present` at `:376` and
had the feed direction inverted; the chain is one either way.)

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

**Two widenings, not one.** The fan-out is the arm this change adds. The root and `pack`
prefixes were *also* widened, from `tmp_obj_` and `tmp_pack_` to `tmp_`, so
`objects/tmp_other_root` and `objects/pack/tmp_idx_p` answer `true` where they answered
`false` (§1.4). The same both-branches-refuse argument covers it, and the prune-set test pins
both rows: the round-4 regression lens's M4 and round 5's MA and MB (§12) each die on them.

**One case is not the same, and an earlier draft of this section said it was.** The scan now
opens directories the old one did not, so it can fail where the old one could not: with an
unreadable two-hex directory in the object store, `verify_object` returns `UpstrokeError::Io`
(`PermissionDenied`) through its `?` instead of reaching the `Refusal::ObjectMissing` branch.
Two of #258's three review lenses executed exactly that (`07-review-regression-da2298f.md` and
`07-review-evidence-da2298f.md`), and round 5 pins it with
`a_fan_out_this_process_cannot_read_is_an_inspection_error_not_an_absence`. The enumeration
is wider than `PermissionDenied`: the round-4 regression lens probed each shape
`fan_out_directories` can meet at `objects/XX` against `std` and found two more newly
reachable kinds — a self-referential or cyclic fan-out symlink is `FilesystemLoop` (errno 40
on Linux), and a symlink that leads through a regular file is `NotADirectory` (20); a dangling
link is `NotFound` and is skipped, a plain file is `Ok(is_dir = false)` and is skipped. Both
need a hand-made symlink inside `.git/objects/`, and both land where the disclosed one does.
Both versions refuse the promotion and neither admits a candidate the other would reject, so
nothing is admitted that was not admitted before — but the *error* a caller sees differs, and
the equivalence claim holds only for successful inspections. This is why `fan_out_directories`
propagates an inspection failure rather than swallowing it: a store this cannot read is a fact
the caller is entitled to, and a silent `false` would be the same blindness this change exists
to remove.

**What the predicate now reads.** `object_directory` resolves a linked worktree to the *main*
repository's object store — the store v0.2's worktree-per-task execution shares across every
task. A sibling task's healthy `hash-object -w`, `write-tree` or `cherry-pick` holds
`objects/xx/tmp_obj_*` there for the length of its write (§1.1). Before this change the
predicate read only the root and `pack`, where a sibling's loose write never appears, so it
could not see a sibling at all; now it can. No outcome moves — the argument above holds — but
the predicate is no longer a function of the worktree it is asked about. The property is not
new to the class: `unreachable_objects` already counts any unreachable object anywhere in the
shared store when the site records no published object, which is the shape round 1's
per-element repair measured and isolated for. Recorded, with that precedent, as
`PR258-SHARED-STORE-PREDICATE-READS-SIBLINGS`, a finding file: whether the two object-store
elements are per-attempt or store-wide is the design's question, not the predicate's.

**The recovery path and the predicate.** An earlier draft said the recovery path never reads
it. That is false of the call graph and true of every recovery the suite covers, and the reason
is a short-circuit. The worktree-reuse half is right as stated: `resume_open_no_attempt`
(`src/engine/topology/dispatch.rs:297`, called in production from
`src/engine/topology/run.rs:1073`) verifies through `verify_or_recreate` →
`WorkspaceManager::quiescence`, which reads registration, the worktree's own git dir and HEAD,
and never the object store — the property `element_breaks_quiescence` records and
`~/tactus-artifacts/G4-ROW10-DECISION.md` §4 argues must stay. So no worktree is recreated that
was not recreated before, and none is reused that was not reused before. But `finish_promotions`
(`src/engine/topology/recover.rs:1225`, production at `:1015`) finishes unfinished promotions
through `candidate::create_candidates_ref` (`:1264`) → `verify_object` →
`classify_object_residue`: the round-4 regression lens instrumented `verify_object` and counted
132 calls from the 118-test recovery suite, and instrumented `temporary_object_files` and
counted **0**, because in every recovery the suite covers the candidate commit is present and
`After` short-circuits before any residue element is read; with the after-reference forced
absent the same suite reached the predicate 54 times. A resume whose candidate commit is absent
from the store — the interrupted `commit-tree` this change exists for — does read it, and
refuses on `Internal` and `None` alike (`PR258-RECORD-RECOVERY-REACH-CLAIM-FALSE`).

## 5. The tests

**`temporary_object_files_answers_for_the_files_git_prunes_as_its_own`**
(`src/workspace_manager/tests.rs`). The tables of §1.4, executed, and no longer a
self-oracle: each name planted **alone**, the scan read, **this git's own `git prune -n` read
beside it** — a row the predicate and git disagree on fails — the name removed, and the store
read again to confirm it is back to none. Thirteen rows: the seven of round 3, the two ends of
the fan-out range `00` and `ff`, the reverse index `pack/tmp_rev_p`, the quarantine
*directory* `tmp_objdir-incoming-AbCdEf`, and `repack`'s `.tmp-1-pack-*` at the root and in
`pack` as two negatives. The negatives are as much of the test as the positives —
`objects/ab/tmp_other_fanout` is Git's garbage, `objects/info/tmp_info` is nobody's, and the
`.tmp-` pair is `repack`'s. The cross-check adds no prerequisite: every test in the file runs
git through the fixture already, and `git prune -n` writes nothing.

**`the_temporary_object_scan_answers_no_for_a_store_of_ordinary_objects`** (same file). A store
of real loose objects answers `no`; a fan-out holding only objects holds no temporary file; a
regular file at a two-digit name resolves, is not a directory, and is skipped rather than read
(without the `is_dir()` guard it would be `Io(NotADirectory)`). The `objects/abc` decoy is
kept, and its doc now says what it is: the scan resolves 256 canonical names by construction,
so `abc` is never a candidate against the shipped design and the assertion cannot fail against
it; it guards a revert to name filtering with a loose prefix match.

**`the_temporary_object_scan_resolves_case_aliases_as_the_filesystem_does`** (same file).
The test deliberately occupies `ab`, selects an unused alphabetic pair with
`unused_alphabetic_fan_out_pair`, creates its upper-case spelling and plants residue there.
It always asserts the scanner's result against native lower-case lookup: positive when that
lookup resolves, negative when it does not. **On macOS and Windows the native branch is now
asserted, not observed**: those legs' temporary directories fold case, and a green there records
that the guard of `PR258-CASEFOLD-FANOUT-INVISIBLE` ran. On a case-sensitive volume the native
assertion is `false == false`, which any implementation that answers `false` there satisfies —
the round-2 name filter included; the `#[cfg(unix)]` half then supplies a lower-case symlink
alias and asserts positive detection, which guards the link-following repair and nothing about
case. Removing the residue must restore a negative result. No timestamp-dependent occupied
fan-out can suppress the construction or the native assertion.

**`a_native_case_insensitive_fan_out_alias_is_detected`** (same file). This explicit integration
test requires a case-insensitive temporary filesystem and fails with a prerequisite diagnostic
otherwise. It is marked `#[ignore = "requires a case-insensitive temporary filesystem; run explicitly with --ignored"]`,
**never runs in CI** — nothing in `.github/` or `scripts/` passes `--ignored` — and was
explicitly executed on local ext4 casefold in round 3 (`09-r3-targeted.log`), where it also
kills the round-2 filter (`13-r3-mutations-M9-native-casefold.log`). It constructs a stored
upper-case directory without a symlink and asserts negative, positive and negative readings as
the temporary file is planted and removed. The same log saves the expected prerequisite failure
on ordinary case-sensitive `/tmp`; that is not counted as a passing native test. So the casefold
P1 is guarded on the macOS and Windows legs by the default test's asserted branch, and on the
ubuntu leg and this build box by nothing that a gate runs
(`PR258-CASEFOLD-GUARD-PLATFORM-SHAPED`, a finding file).

**`a_symlinked_fan_out_directory_is_followed_as_git_follows_it`** (same file). Its existing
plant/unplant/dangling-target assertions remain. The symlink uses an unused pair selected at
runtime, so fixture commits cannot collide with a fixed `objects/93`. Both the review's failing
timestamp and its one-second control pass (`09-r3-targeted.log`).

**`a_fan_out_link_that_loops_is_an_inspection_error_once_no_earlier_fan_out_answers`** (same
file, round 5, Unix). Residue in `objects/00` and a self-referential symlink at a later unused
name: the scan answers `true` — `00` is read before the loop is probed — and, with the residue
removed, `Err(Io)` naming the looping fan-out with the errno the filesystem gave
(`ErrorKind::FilesystemLoop` is not yet stable to name, and `ELOOP` is 40 on Linux and 62 on
Darwin, so the number is taken from the test's own `fs::metadata` premise). It kills the eager
collection and the `fs::metadata` error arm's replacement by a skip.

**`a_fan_out_this_process_cannot_read_is_an_inspection_error_not_an_absence`** (same file,
round 5, Unix). A fan-out holding residue at mode `000`: the scan is `Err(Io)` naming it with
`PermissionDenied`, the disclosed production difference of §4. The mode bit must bind — the
test fails its prerequisite loudly under root, the way the file's other mode-bit tests do — and
the mode is restored by the same `RestoreMode` guard.

**`a_listing_that_fails_part_way_through_is_an_inspection_error_not_the_end_of_it`** (same
file, round 5). `holds_name_prefixed` over a constructed listing whose second entry is an
error: a name after the error is not reached and the error is returned; a name before it is an
answer. No filesystem the suite runs on fails a `readdir` to order, which is why the listing is
handed in.

**`every_registered_residue_element_is_constructed_and_recovers`** (same file) and
**`synthetic_git_add_residue_unreferenced_objects_and_index_lock_then_forced_scrub_converges`**
(`src/engine/topology/attempt/tests.rs`). Their planting helpers, `construct_element` and
`plant_stage_residue`, wrote `objects/tmp_obj_synthetic` at the object root — the arm the scan
always had — so the `residue_classified_sites() × residue_elements()` grid that G4 row 10's
per-element evidence rests on stayed green with the fan-out loop deleted, at every site but
dispatch (§12.3). Both now plant in a fan-out directory the store already holds, through the
fixture's `fan_out_directory`, which the dispatch test uses as well; the helper moved from
`dispatch/tests.rs` to `workspace_manager/fixture.rs` so that the three sites share one.

**`repair_materialization_synthetic_residue_recreated_after_forced_removal`**
(`src/engine/topology/dispatch/tests.rs`), restructured in round 1. Two changes:

*The element is constructed where the materialization's own write leaves it* — in a fan-out
directory the store already holds, beside real objects, rather than at the object root.

*Each element is constructed alone, in a repository of its own.* This is the shape
`synthetic_git_add_residue_unreferenced_objects_and_index_lock_then_forced_scrub_converges`
already uses for `Object.CandidateStage`, for the same reason and with its own measurement behind it
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

`05-mutations.log` records M1–M5 against the first implementation;
`06-review-repair-mutations.log` records M6–M8 against the first review's repairs. These are
historical executions: canonical-path resolution replaces the name filter that M5 and M7 mutated.

Earlier drafts overstated M1, M2, M4 and M7. The table below reports what the named saved files
contain, including the surviving M7; an unsaved failing rerun cannot support a citation to the
passing log.

| # | Mutation | What the log records |
|---|---|---|
| M1 | the whole fix reverted, the tests kept | **two** die: `temporary_object_files_answers_for_the_files_git_prunes_as_its_own` at `tmp_other_root`, and the synthetic test — at its direct `temporary_object_files` assertion, not at the singleton assertion an earlier draft named. The negative test **passes**, which is what a negative test does under a narrowing |
| M2 | only the fan-out loop removed | the same two; the unit test names `ab/tmp_obj_fanout` |
| M3 | the root and `pack` prefixes narrowed back to `tmp_obj_`/`tmp_pack_` | the unit test, naming `tmp_other_root`. The synthetic test survives, correctly — it constructs in the fan-out |
| M4 | the element constructed at the object root again *and* the fix reverted | **the synthetic test passes.** The positive unit test still fails, on `tmp_other_root`, because it tests the whole prune set rather than this one placement. The load-bearing half is the synthetic test: the old construction against the old scan is green, which is why the suite could be green while the defect was live. An earlier draft said "nothing died", which the log does not support |
| M5 | the fan-out name test loosened from "exactly two hex" to "two or more hex" | the negative test, on the `objects/abc` decoy |
| M6 | the symlink repair reverted to `DirEntry::file_type` | `a_symlinked_fan_out_directory_is_followed_as_git_follows_it` |
| M7 | the lower-case fan-out rule loosened back to any hex digit | `06-review-repair-mutations.log` records **four passes and zero failures**. The previously claimed failing upper-case rerun was not saved there. The restriction itself was then shown to miss a real Git temporary on a case-insensitive filesystem, so it is replaced rather than defended |
| M8 | `git prune --expire=now` injected into the forced recreation | `a_forced_recreation_preserves_the_object_store_residue_it_recovers_over`, at its R27 assertion. This is the review's own mutation, which the shared-store test used to catch and the isolated one did not |

M4 is the row that matters for the gate: it is why "the synthetic test passes" was never evidence
that the element classifies. M8 is the row that matters for R27: it is why isolating the elements
needed a second test rather than none.

**Current repair executions.** `13-r3-mutations.py` applies each mutation in an independent source
copy, retains the repaired tests, and saves the exact patch, command, source hashes and output.
These executions replace the unsaved M7 claim with a failing run a reviewer can read:

| Mutation | Executed failure | Saved execution |
|---|---|---|
| M7 replay: restore `read_dir` discovery admitting any ASCII two-hex spelling | The default case-alias test fails its native lookup assertion on case-sensitive Linux, with ordinary `ab` already occupied. It fails with both Git dates `1788912660 +0000`, and again one second later; each run records zero passes and one failure | `13-r3-mutations-M7-date-ab.log`, `13-r3-mutations-M7-date-control.log` |
| M9: restore the exact `1d7f3cc9` lower-case `read_dir` filter | The native case-insensitive integration test on ext4 casefold passes its filesystem prerequisite, then fails the temporary-object detection assertion; zero passes and one failure | `13-r3-mutations-M9-native-casefold.log` |
| M6 replay: replace resolving `fs::metadata` with `fs::symlink_metadata` | The symlink test at the collision timestamp `1788912066 +0000` fails detection after successful construction; zero passes and one failure | `13-r3-mutations-M6-symlink-date-collision.log` |

The corresponding restored controls and their source hashes are saved in
`13-r3-mutations-summary.json` and its named `13-r3-mutations-restored-*.log` files; each control
passes. The mutations
never touch the main worktree. M7's original passing log remains unchanged and is still reported
as passing in the historical table above.

**Round 5 executions** (`22-r5-mutations.py`, `22-r5-mutations-summary.json`; one log per mutant
and test, named `22-r5-mutations-<mutant>--<test>.log`, each with the applied patch beside it).
Applied **in place** to the driving worktree's uncommitted repair, on its private target
directory, and restored after each case from a `cp` backup — never `git checkout --` — with the
restored file's SHA-256 checked against the baseline; the summary records the source hashes every
run was built from, and `22-r5-mutations-baseline.patch` is `git diff HEAD` at run time. The
oracle for a death is exit `101`, the exact `0 passed; 1 failed` line, no compile failure, and a
witness string from the assertion that fired; a control is exit `0` and `1 passed; 0 failed`.

| Mutation | Dies in | The assertion that fired |
|---|---|---|
| MA the root arm removed | `temporary_object_files_answers_for_the_files_git_prunes_as_its_own` | `tmp_obj_root: git prune does call it a stale temporary` (`tests.rs:10870`, `left: false`, `right: true`) |
| MB the `pack` arm removed | the same test | `pack/tmp_pack_p: …`, the same line |
| MC the fan-out loop removed | the same test; `every_registered_residue_element_is_constructed_and_recovers`; `synthetic_git_add_residue_unreferenced_objects_and_index_lock_then_forced_scrub_converges`; `repair_materialization_synthetic_residue_recreated_after_forced_removal`; `a_symlinked_fan_out_directory_is_followed_as_git_follows_it`; `the_temporary_object_scan_resolves_case_aliases_as_the_filesystem_does`; `a_fan_out_link_that_loops_…`; `a_fan_out_this_process_cannot_read_…` | `00/tmp_obj_first: …`; the grid at `tests.rs:8738` (`left: None`, `right: Internal`); the attempt test at `attempt/tests.rs:1451`; the dispatch test at `dispatch/tests.rs:1148`; and the four scan tests at their fan-out assertions |
| MC with the per-site probe | `r5_probe_every_site_registering_temporary_object_file` (a temporary test, `22-r5-mutations-MC-probe-every-site.patch`, never committed) | not a death — a reading: all six sites that register the element classify `Ok(None)` under the mutant and `Ok(Internal)` restored (§12.1) |
| MD the range `0..=254` | the prune-set test | `ff/tmp_obj_last: …` |
| ME the range `1..=255` | the prune-set test; the loop test | `00/tmp_obj_first: …`; `` `00` answers before the loop is probed `` |
| MF the `fs::metadata` error arm skipped | `a_fan_out_link_that_loops_is_an_inspection_error_once_no_earlier_fan_out_answers` | `a fan-out that cannot be resolved is not a fan-out that holds nothing` |
| MG the `read_dir` error arm returned as `Ok(false)` | `a_fan_out_this_process_cannot_read_is_an_inspection_error_not_an_absence` | `a fan-out that cannot be read is not a fan-out that holds nothing` |
| MH the per-entry error turned into `continue` | `a_listing_that_fails_part_way_through_is_an_inspection_error_not_the_end_of_it` | `a failure before the name is not the end of the listing` |
| MI the `is_dir()` guard dropped | `the_temporary_object_scan_answers_no_for_a_store_of_ordinary_objects` | `a regular file at a fan-out name is not an inspection failure` |
| MJ the fan-out prefix widened to `tmp_` **and** the table's `ab/tmp_other_fanout` row flipped to `true` | the prune-set test, at the `git prune -n` cross-check and nowhere else | `ab/tmp_other_fanout: this git's own prune -n does not name it, and the table says it does` (`tests.rs:10881`) |
| MK round 4's eager collection restored (`collect::<Result<Vec<_>, _>>()?` before the first read) | the loop test, at its first assertion | `` the answer `00` gave stands `` |
| restored controls | every test above, and the probe | pass |

A first attempt at MK collected a `Vec<Result<_, _>>`, which does not short-circuit on the error
and so was not round 4's shape; it survived, correctly, and is kept as
`22-r5-mutations-MK-first-attempt-vec-of-results-SURVIVED*` with the first run's summary,
`22-r5-mutations-summary-first-run.json`. The catalogue was then run again whole, with MK in round
4's exact shape.

## 7. The notes

`docs/internals/engine/topology/dispatch/tests.md`: the section for the synthetic test rewritten
in round 1 to say what it now does and why — a repository per element, the two readings, and the
fan-out — with a section added for `synthetic_materialization_residue_element`; in round 5 its
"where Git leaves it" paragraph corrected (the root is not a place Git never writes) and the
`fan_out_directory` section removed with the helper's move to the fixture.
`docs/internals/engine/topology/attempt/tests.md`: one paragraph under `plant_stage_residue`
saying the temporary object file goes into a fan-out. No other notes file mirrors a module this
change touches; `src/workspace_manager.rs` and its `fixture.rs` carry no `Extended notes:`
marker, so the function doc is the record and it carries the measurements — the prune table
with the round-5 rows, the three-producer trace, the cost of the 256-name probe, the error
enumeration and the Windows delete-pending caveat.

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

Three scoped `gpt-6-astra` max lenses on `da2298fc` — closure, regression, and evidence, saved as
`07-review-correctness-da2298f.md`, `07-review-regression-da2298f.md` and
`07-review-evidence-da2298f.md`. **All three
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

### 10.3 The first prose repair and its remaining gaps

- **The mutation table contradicted its own saved log** (closure and evidence lenses). The first
  repair corrected the "both new tests" and "nothing died" rows but added an unsupported M7 failure
  claim. §6 now reports the saved M7 pass as well.
- **"No production outcome moves" omitted a newly reachable error** (regression and evidence lenses).
  The first repair corrected §4 but left contradictory claims in §0 and §2. Those sections now
  also limit equivalence to successful inspections and disclose `Io(PermissionDenied)`.
- **The quoted reproduction figures came from an unsaved run** (evidence lens). The reproductions
  were run twice — once interactively and once to write the evidence files — and the record quoted
  the first. The first repair replaced the figures but left the introduction describing measured
  kill latency. §1.2 now identifies the printed time as the requested delay relative to a separate
  uninterrupted write; the literal output is retained with that explanation.

The first repair also added a lower-case entry-name restriction and an `objects/AB` negative case.
The second review showed that restriction introduced a P1 on case-insensitive filesystems and that
the negative assertion could disappear when fixture commits populated `objects/ab`. The current
repair resolves canonical paths and derives case-sensitive and case-insensitive expectations from
the filesystem lookup behavior instead.

### 10.4 What the lenses did not fault

The first round's saved verdicts distinguish execution from source reasoning. The closure lens executed
alternates, `GIT_OBJECT_DIRECTORY`, linked worktrees and separate git directories, bare and
SHA-256 repositories, packed-only stores, pack and maintenance temporaries including `tmp_rev_*`,
with no further closure failure. Its loose-object naming checks across Git 2.30, 2.50.1, 2.55 and
Git for Windows 2.43 were **source reasoning**, not executions of those versions; it explicitly did
not execute macOS or Windows binaries (`07-review-correctness-da2298f.md`). The regression lens
found `src/topology/**` byte-identical and no changed census row,
frozen count, golden inventory or durable residue-class consumer. The evidence lens re-executed all
seven prune-table cases, confirmed the four repositories really are distinct, confirmed that
omitting an element fails its singleton assertion and that adding an orphan beside `IndexLock`
yields `[UnreferencedObject, IndexLock]`, checked both first-bad commits and the `82de0767` /
`81ee09ef` code-identity claim, and stated: **"The per-element evidence is now real."** That is the
sentence the third G4 verification's finding 1 asked for.

## 11. Repair of the second review at `1d7f3cc9`

This session edits the worktree at `1d7f3cc946fb41e8bddf0ef2fbe96b62420e9d1a`; it creates no
commit, push, branch change or posted review. The separate `claude-opus-5` max review requested by
the owner is still pending. The second-round verdicts are
`~/review-258-correctness-r2.log`, `~/review-258-regression-r2.log` and
`~/review-258-evidence-r2.log`.

### 11.1 The new P1, executed before and after

The lower-case entry-name restriction introduced by the first repair skipped a stored upper-case
fan-out even where Git's lower-case lookup resolved to it. Resolving canonical paths with
`fs::metadata` fixes that condition and retains the previous symlink repair.

`14-r3-casefold-harness.py` creates a private ext4 casefold filesystem, builds separate probes
against the real original and repaired libraries through `upstroke-build`, starts a real
`git hash-object -w`, waits until a nonempty temporary file is observable, and sends `SIGKILL`.
It does not infer success from a planted classifier-only stand-in. Setup and build provenance are
saved in `10-r3-casefold-setup.log`, `11-r3-casefold-old-build.log` and
`12-r3-casefold-repaired-build.log`.

The saved execution (`13-r3-casefold-kill.json` and `13-r3-casefold-kill.log`) reports:

```
git version 2.43.0
stored residue: A8/tmp_obj_MYmjHV
residue bytes: 32768
writer exit: -9
git prune -n: Removing stale temporary file .git/objects/a8/tmp_obj_MYmjHV
```

The lower-case and upper-case paths resolve to the same inode. Before the write, both probes
answer `false`, `[]`, `None`. After the kill:

| Reading | Original `1d7f3cc9` | Repaired library |
|---|---|---|
| `temporary_object_files` | `Ok(false)` | `Ok(true)` |
| `unreachable_objects` | `Ok([])` | `Ok([])` |
| `observed_residue_elements` | `Ok([])` | `Ok([TemporaryObjectFile])` |
| `classify_object_residue` | `Ok(None)` | `Ok(Internal)` |

Changing only the stored spelling to lower case makes both probes detect the file and return
`Internal`. Source and executable SHA-256 hashes are in the same JSON, binding these readings to
the two real libraries. The harness reports process lifetime through `wait` returning, explicitly
not measured signal latency; no timing estimate is needed for this witness.

**The upper-case spelling is the harness's construction, not git's.** Git always creates a
fan-out lower-case; the harness computes the payload's oid in advance and forces that fan-out's
stored name to upper case *before* git writes — renaming an existing `a8` through a buffer, or
creating `A8` outright (`14-r3-casefold-harness.py:121-128`). That is the precondition of the
defect: a store whose spelling a person, a tool or a copy left upper-case, on a volume that
folds case, which git then writes into through its lower-case path. The narrative above says
what git did; this sentence says what the harness did first.

The fenced blocks in this section and in §1.2 are formatted extracts of the logs they cite —
line breaks and labels are the record's — and every value in them is in the cited file.

`14-r3-casefold-residue-repo.tar.gz` preserves the actual killed-Git repository with its original
upper-case spelling restored. `14-r3-casefold-residue-archive.log` saves its digest and repeats the
original-false/repaired-true classifier comparison after that restoration. The private casefold
mount was unmounted after the native tests (`15-r3-casefold-cleanup.log`).

Linux casefold and symlink behavior were executed. Native Windows and macOS execution was not
available in this session; expectations for those platforms are reasoned from `std::path` joins
and `fs::metadata` lookup, with portable tests supplied for their CI runs.

### 11.2 Timestamp and non-vacuity controls

`09-r3-targeted.py` saves each command and its complete output to `09-r3-targeted.log`:

- The symlink test passes with both Git dates set to `1788912066 +0000`, and again at
  `1788912067 +0000`.
- The default case-alias test passes with both dates set to `1788912660 +0000`, and again at
  `1788912661 +0000`. Its ordinary `ab` directory is explicitly occupied regardless of the clock.
- The native integration test and the default case-alias test both pass on casefold ext4.
- Explicitly requesting the native integration test on ordinary `/tmp` fails at its prerequisite,
  with exit `101`, as expected. It cannot silently claim a native case-insensitive pass there.

The earlier `08-r3-focused.log` retains the initial formatting-check failure and the passing
focused tests. The formatting issue was corrected; the subsequent formatting check passes in
`09-r3-targeted.log`. No failed execution is overwritten with its control.

### 11.3 Record corrections and handoff

Sections 0, 2 and 4 consistently limit verification-result equivalence to successful inspections
and disclose the `Io(PermissionDenied)` path. Section 3 describes metadata resolution and propagation
of every inspection error except `NotFound`. Section 1.2 describes the historical requested delay
against a separate uninterrupted write. Section 6 retains the actual historical M7 outcome instead
of attributing an unsaved failure to its passing log.

The symlink repair, per-element repository isolation and R27-across-recreation witness are retained.
The design packet, frozen `src/topology/**`, wire vocabulary, gate scripts, workflows, validators
and owner-deferred findings remain outside this repair. Row 10's contract correction and the G4
rerun remain the owner's work. The driving session receives a corrected local PR body; the current
remote body is saved before amendment in `17-r3-pr-before.json` and is not changed by this session.

### 11.4 Gates and final evidence

`15-r3-gates.log` saves the bare invocation of
`~/bin/w1-eight-iso /srv/worktrees/tmpobj` and its `ALL 9 PASS` result. The wrapper takes its own
flock; no outer flock was added. Its complete command logs are copied into `15-r3-gate-logs/`, and
the previous same-HEAD logs were preserved in `15-r3-prior-gate-logs/` before invocation.

All required commands passed:

- Through the wrapper: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`,
  `cargo test --all-targets --all-features`, and
  `cargo +1.85.0 check --locked --all-targets --all-features`.
- Through the same gate runner: `test-release-record.sh`, `test-pr-policy.sh`,
  `test-pr-ledger-evidence.sh`, `test-docs-consistency.sh` and `test-internals-notes.sh`.
- Separately: `bash .github/scripts/test-pr-ready-audit.sh`, saved in `16-r3-pr-ready.log`.

The Rust files retain the source hashes recorded before the gate run. This record's final evidence
additions are made after that run; the six shell gates, local PR-body validation, whitespace check,
protected-path checks and final source hashes are saved in `18-r3-final-checks.log`.
`18-r3-final.patch` contains the final uncommitted diff. `19-r3-evidence-manifest.md` lists every new
evidence file with its SHA-256 digest, including harnesses, failing runs, restored controls,
preserved prior gate logs and the unpublished `17-r3-pr-body.md` handoff.

## 12. Repair of the third review at `2b048a67`

Three independent `claude-opus-5` `max` lenses reviewed `2b048a672a51ff681d19ec7bcc369c07e252b6cd`:
closure (`~/opus-review-258-closure.md`, F1–F5), regression (`~/opus-review-258-regression.md`,
R1–R6) and evidence (`~/opus-review-258-evidence.md`, B1–B3, N1–N4). All three returned
`CHANGES_REQUIRED`, all three executed their counterexamples, and this section is the record of
closing every finding they carried — fixed, or recorded as a ledger row and a finding file. The
rule of this round, after two P1s came from a session reasoning about filesystem semantics that
a reviewer then measured: every figure quoted here is in a saved file the citation names, and
what could not be executed is marked reasoned. This session is `claude-fable-5-1` at `max`, per
the owner's 2026-09-09 assignment of implementation work to that model.

### 12.1 The grid planted at the object root (closure F4) — G4 row 10's evidence

`construct_element` (`src/workspace_manager/tests.rs:8957` at `2b048a67`) and
`plant_stage_residue` (`src/engine/topology/attempt/tests.rs:1442`) both wrote
`objects/tmp_obj_synthetic` at the object **root**. Only the dispatch test had moved into a
fan-out. So the `residue_classified_sites() × residue_elements()` grid — the per-element evidence
G4 row 10 rests on — exercised the arm the scan always had, and stayed green with the fan-out
loop deleted from production at every site but dispatch: the third verification's finding 1
relocated, not removed.

Both now plant through `fan_out_directory`, moved from `dispatch/tests.rs` into
`workspace_manager/fixture.rs` so that the three planting sites share one helper. Two things the
brief asked to be demonstrated, both executed (`22-r5-mutations-summary.json`):

1. **The grid exercises the fan-out arm at every site.** With the fan-out loop removed (MC), the
   grid dies at `tests.rs:8738` — `left: None`, `right: Internal` — and so do the attempt test
   (`attempt/tests.rs:1451`) and the dispatch test (`dispatch/tests.rs:1148`). Because the grid
   stops at its first failure, a temporary probe (`22-r5-mutations-MC-probe-every-site.patch`,
   never committed) ran `construct_and_recover(site, TemporaryObjectFile)` inside `catch_unwind`
   at every site that registers the element and printed each reading. Under MC
   (`22-r5-mutations-MC-probe-every-site--r5_probe_every_site_registering_temporar.log`):

   ```
   R5-PROBE site=Object.CandidateStage classified=Ok(None)
   R5-PROBE site=Object.CandidateWriteTree classified=Ok(None)
   R5-PROBE site=Object.SnapshotCommitTree classified=Ok(None)
   R5-PROBE site=Object.CandidateCommitTree classified=Ok(None)
   R5-PROBE site=Object.ProposalCherryPick classified=Ok(None)
   R5-PROBE site=Object.RepairMaterialize classified=Ok(None)
   R5-PROBE sites=6
   ```

   and restored (`22-r5-mutations-restored-probe-every-site--…`), the same six sites read
   `Ok(Internal)`. Every site's per-element evidence for this element now witnesses the fan-out
   arm; none witnesses only the root.

2. **No arm loses the witness it has.** Three arms, three mutations, three deaths, all in
   `temporary_object_files_answers_for_the_files_git_prunes_as_its_own` at `tests.rs:10870`
   (`left: false`, `right: true`): MA removes the root arm and the test dies on `tmp_obj_root`;
   MB removes the `pack` arm and it dies on `pack/tmp_pack_p`; MC removes the fan-out loop and it
   dies on `00/tmp_obj_first` — with the seven other deaths above beside it.

### 12.2 The root arm's warrant (closure F2, evidence B1)

Four statements in this change said Git never writes `objects/tmp_obj_*` at the object root —
`tests.rs:10752`, `dispatch/tests.rs:1134`, the dispatch notes at lines 353 and 356, and this
record's §1.1 and §1.3 — each generalising `02-strace-where-git-writes.log`, which traced two
commands. Git does write there (§1.1, re-executed:
`20-r5-strace-git-writes-in-three-places.log`): `git unpack-objects` with
`core.bigFileThreshold 512` opened `.git/objects/tmp_obj_KSwW4k` at the root and linked it to
`.git/objects/88/fb3fef…`, because a streamed object's oid, and so its fan-out, is unknown until
the stream ends. Bulk checkin above the threshold opened `.git/objects/pack/tmp_pack_2bgAj0` and
`tmp_idx_HebZJG`. All three places are real, which is why the predicate has three arms; the
production doc at `src/workspace_manager.rs` now carries that trace and that sentence, and is the
authority §7 nominates. The four sites are corrected; the doc-comment that was already narrow
("neither of those two commands writes at the root") keeps its meaning.

### 12.3 The bound that is the repair (closure F1)

`for prefix in 0_u8..=255` survived mutation to `0..=254` and to `1..=255` with the suite green:
the three fan-out indices any test planted into were `0a`, `ab` and `cd`. Two rows close the
boundary — `00/tmp_obj_first` and `ff/tmp_obj_last`, both named by `git prune -n`
(`21-r5-git-prune-producers-outside-the-sample.log`) — and MD dies on `ff`, ME on `00` (§6).

### 12.4 The three error arms and the `is_dir()` guard (regression R1, R2)

Each of the three error arms the change added could be replaced by a silent `false` with the
suite green, including the arm that carries the change's own disclosed production difference.
Three tests now (§5): an unreadable fan-out at mode `000` is `Err(Io)` naming it with
`PermissionDenied` (MG dies); a self-referential fan-out symlink is `Err(Io)` naming it with the
errno the filesystem gave, once nothing earlier answers (MF dies); a listing whose second entry
is an error is an error, not the end of the listing (MH dies) — that last one over a constructed
listing, through `holds_name_prefixed`, because no filesystem the suite runs on fails a `readdir`
to order; the seam's real caller is exercised by every positive test in the file, and the
mapping it feeds (`entry.file_name()`) by every one of them. A regular file at a two-digit name
is skipped, not read (MI dies).

### 12.5 The eager probe (closure F3) and the self-oracle (closure F5)

`fan_out_directories` is an iterator, read one fan-out at a time; residue at `00` answers
`Ok(true)` before a loop at a later name is probed (MK — round 4's exact `Result<Vec<_>>`
shape — dies at that first assertion). The prune-set test reads this git's own `git prune -n`
after every row, so the table is falsifiable: MJ widens the predicate and the table together and
dies only at the cross-check. The cross-check adds no prerequisite — every test in the file already
runs git through the fixture — and `git prune -n` writes nothing.

### 12.6 Recorded, not engineered

- **B2, the prune grammar.** §1.4 is narrowed to the seven names planted, with a second table for
  the two producers outside the sample, executed: `tmp_objdir-incoming-*` is a *directory* that
  `prune` removes as "a stale temporary directory" and the predicate answers `true` for (R5, a
  row in the doc table and in the test); `.tmp-<pid>-pack-*` is never named by `prune` and the
  predicate answers `false` for (two negative rows).
- **B3 and R3, the casefold guard.** The ledger cell for `PR258-CASEFOLD-FANOUT-INVISIBLE` says
  the named witness is `#[ignore]`d and names the legs that guard it; the sibling test asserts on
  macOS and Windows that its native branch was the one taken, so a green there records it.
  Recorded as `PR258-CASEFOLD-GUARD-PLATFORM-SHAPED`, `accepted-risk`, with a finding file that
  says what lifts it (a casefold volume for the ubuntu leg — a workflow change).
- **R4, the shared store.** The predicate reads the repository's store, which every
  worktree-per-task shares, so a sibling's healthy in-flight write is a temporary object file
  here; the precedent is `unreachable_objects`, which has always counted any unreachable object
  anywhere in that store. §4 and the function doc say so; recorded as
  `PR258-SHARED-STORE-PREDICATE-READS-SIBLINGS`, `deferred`, with a finding file: whether the
  object-store elements are per-attempt or store-wide is the design's question.
- **R6, the decoy.** `objects/abc` is never a candidate against the shipped design, so the
  assertion cannot fail against it; it is kept, and its doc says it guards a revert to loose prefix
  matching.
- **(a) the recovery reach, (b) the error enumeration, (c) Windows delete-pending, (d) the
  second widening** — §4 and §3 say each as it is: `finish_promotions` reaches the classifier and
  `After` short-circuits before the predicate in every recovery the suite covers; `FilesystemLoop`
  and `NotADirectory` beside `PermissionDenied`; a delete-pending directory on Windows answers
  `PermissionDenied`, not `NotFound`, the shape `remove_tree_once_handles_close` and
  `runner::container`'s `RACING_ACCESS_ATTEMPTS` measured on the guest (cited, not re-derived —
  reasoned here, since no Windows execution was available to this session); and the root and
  `pack` prefixes were widened as well as the fan-out added.
- **N1–N4.** §4 names `observed_residue_elements` at `residue.rs:376` as the reader and
  `internal_residue_present` at `:327` as what it feeds; the rustdoc's cost paragraph describes the
  256-name probe; §11.1 says the harness forced the upper-case spelling before git wrote; and the
  fenced blocks of §1.2 and §11.1 are declared formatted extracts.

Every finding above is a row of the pull request's ledger, bound to `2b048a67` and the line the
lens cited; the two open ones have files under `reviews/findings/`.

### 12.7 Gates and final evidence

`24-r5-gates.log` saves the bare invocation of `~/bin/w1-eight-iso /srv/worktrees/tmpobj` on the
uncommitted round-5 repair — the wrapper takes its own flock, and no outer flock was added — with
its result line, the tree it describes and the source hashes it was measured on:

```
HEAD=2b048a672a51ff681d19ec7bcc369c07e252b6cd
STATUS= M docs/internals/engine/topology/attempt/tests.md |  M docs/internals/engine/topology/dispatch/tests.md |  M reviews/2026-09-09-g4-fix-temporary-object-fanout-record.md |  M src/engine/topology/attempt/tests.rs |  M src/engine/topology/dispatch/tests.rs |  M src/workspace_manager.rs |  M src/workspace_manager/fixture.rs |  M src/workspace_manager/tests.rs | ?? reviews/findings/P2_correctness_202609092355_the-widened-predicate-reads-sibling-tasks-in-flight-writes.md | ?? reviews/findings/P2_portability_202609092356_the-casefold-guard-runs-on-two-of-three-legs.md |
sha256=db3af8242dc8a6b30085f1b8151e0ecd21d49d9a3d88af243396c8c5ce9334d5  src/workspace_manager.rs
sha256=df2d7fcff49181367622c60efeec30e965aaca04c1ed356bfc464ff0172ed0ce  src/workspace_manager/tests.rs
sha256=5e7bdc4188634045f9fa34307099cf8420ed331eb1b60c775e36f1d45d8247ff  src/workspace_manager/fixture.rs
sha256=867990458bf016a40f09cf7c41fabf6d89c08e23fdb9f5b50f9085373acffa87  src/engine/topology/attempt/tests.rs
sha256=e57eace99a03a49cb8cf163ef1d079d53323d3591858f86d71c1cabf848fb624  src/engine/topology/dispatch/tests.rs
sha256=9db2457d8fa6063556aeeadbcc737c3b38c931680ed6f1c7b913b46413a895e5  reviews/2026-09-09-g4-fix-temporary-object-fanout-record.md
PASS 01 fmt                 1s
PASS 02 clippy             16s
PASS 03 test               60s
PASS 04 msrv               11s
PASS 05 release-record      0s
PASS 06 pr-policy           0s
PASS 07 pr-ledger           0s
PASS 08 docs-consistency    0s
PASS 09 internals-notes     1s
ALL 9 PASS at 2b048a6
```

Its complete command logs are copied into `24-r5-gate-logs/`. The tenth gate ran separately
(`25-r5-pr-ready.log`: `test-pr-ready-audit: ok`, `PR_READY_EXIT=0`).

This section is the record's last edit after that run; the checks that follow it — formatting,
whitespace, the freeze, the six shell gates, the local validation of the pull request body, that
every round-5 file cited here and in the body exists, and that the mutation catalogue's source
hashes are the tree's — are saved in `26-r5-final-checks.log`. The committed head then gets a
bare run of its own, `27-r5-gates-committed.log`, which is the run the pull request body names as
its passing head; `28-r5-pr-body.md` is the body as published and `29-r5-final.patch` the
committed diff from `2b048a67`. `19-r3-evidence-manifest.md` lists every round-5 file with its
SHA-256 and byte count, in the rows round 3 established.
