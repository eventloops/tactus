---
id: PR258-SHARED-STORE-PREDICATE-READS-SIBLINGS
severity: P2
disposition: deferred
category: correctness
pr: 258
reviewed_sha: 2b048a672a51ff681d19ec7bcc369c07e252b6cd
location: src/workspace_manager.rs:4634
provenance: introduced_by_feature
first_bad: G4-TEMP-OBJECT-FANOUT-UNSCANNED — the widening that closes it is what makes the predicate see a sibling's ordinary fan-out write; the root-and-pack scan already saw a sibling's streamed root write and its pack writes, and the class it joins is `unreachable_objects`'s, which has read the whole shared store since it was written
guard: `temporary_object_files`' doc states the property and its precedent; no test constructs a sibling's in-flight write, because the property is a design question (row 10 / R27), not a defect a test can close
---

## Failure sequence

`object_directory` runs `git rev-parse --path-format=absolute --git-path objects`. From a
linked worktree that is the **main** repository's object directory — the one store every
worktree-per-task of a v0.2 run shares (`07-review-regression` executed: main and linked
worktree both resolve to `…/main/.git/objects`).

    task A's worktree: `verify_object` for `Object.CandidateCommitTree`, candidate commit
      absent, so `After` does not short-circuit and the residue elements are read
    -> task B, in its own worktree, is inside a healthy `hash-object -w`, `write-tree` or
      `cherry-pick`, which holds `objects/xx/tmp_obj_*` in the shared store for the length
      of the write (`02-strace-where-git-writes.log`)
    -> `temporary_object_files` answers `true` for A's store, which is B's store
    -> `observed_residue_elements` for A carries `TemporaryObjectFile`, and A classifies
      `Internal` on the strength of B's write

No outcome moves today: `verify_object` refuses on `Internal` and on `None` alike, and the
predicate is only reached on a path already headed for `Refusal::ObjectMissing`. What has
changed is the reach of a property the predicate already had: **it is not a function of the
worktree it is asked about.** Before the widening it read the object root and `pack`, where a
sibling's *streamed* loose write and its bulk-checkin pack write already appeared: the round-5
record lens paused sibling B inside a real streamed `unpack-objects` and an exact copy of the
frozen `81ee09ef` scanner answered `Ok(true)` from sibling A, as the current library did
(`~/tactus-artifacts/tmpobj-evidence/31-r6-record-lens-library-results.json`). What the
widening adds is a sibling's *ordinary* fan-out loose write — `hash-object -w`, `write-tree`,
`cherry-pick` — which the old scan never read; an earlier draft of this file said the old scan
"could not see a sibling at all", which is false
(`PR258-SIBLING-VISIBILITY-PREDATES-THE-WIDENING`). Either way the answer is not reproducible
from the worktree's own state.

**The precedent, so this row is not read as a regression this change introduced.** The
property is not new to the class. `unreachable_objects` runs `git fsck --unreachable` over
the same shared store, and when a site records no published object it counts *any*
unreachable object anywhere in it — `Object.RepairMaterialize`'s `UnreferencedObject` is
observed for a sibling's unpublished object exactly as this element now is for a sibling's
temporary file. The round-1 repair of the per-element test measured that shape (the orphan
constructed for one element supplied the `Internal` the next element read) and isolated
the elements per repository because of it. This change joins `TemporaryObjectFile` to a
class `UnreferencedObject` already belonged to.

## Why it is deferred

Whether `ObjectResidue::Internal` for one task may be supplied by another task's healthy
write is a question for the design — row 10's contract and `resource_accounting[R27]`,
which says Git prunes these files itself and says nothing about whose they are — and not
one a predicate can settle. The two readings on offer both change the design: scope the
element to the asking attempt (the file's mtime against the attempt's start, or a
temporary the attempt's own kill is known to have left), or state that the two
object-store elements are store-wide observations and that a site whose `After` is absent
refuses on either. The packet is the owner's, and this pull request does not touch it.

## What the change that takes this up should do

Decide in `DESIGN.md` whether the two object-store elements are per-attempt or store-wide.
If per-attempt, add the discriminating observation to `ResidueTarget` (the attempt's start
instant, or the kill's known write) and read it in `observed_residue_elements` for both
`UnreferencedObject` and `TemporaryObjectFile` together — one element scoped and the other
not would reintroduce the asymmetry this row records. If store-wide, say so in the
`command_internal_sub_effects` predicate's sentence and in R27, and close this row with the
design sentence as its guard. Either way, the test is a sibling worktree holding a real
in-flight write (a `KillableGitChild` paused inside `hash-object -w`) while the asking
worktree classifies.
