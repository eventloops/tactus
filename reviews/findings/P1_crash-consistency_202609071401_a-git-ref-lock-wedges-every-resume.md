---
id: PR8-CRASH-002
severity: P1
disposition: deferred
category: crash-consistency
pr: 8
reviewed_sha: 3414dc5861c9a523342ea0792a54f0129cf82f2f
location: src/engine/topology/integrate.rs:487
provenance: pre_existing
first_bad: 
guard: the project owner — a Class C residue-class decision
---

## Failure sequence

Git takes a lock file for the duration of a ref write and removes it by rename. A coordinator
killed inside that window leaves the lock behind, and Git refuses every later write to that ref
until the file is removed.

    merge_prepared durable
    -> git update-ref killed after creating integration.lock and before the rename
    -> resume retries the authorized compare-and-swap
    -> Git refuses on the lock
    -> every later resume repeats the refusal until an operator removes the file

Met a second time on the macOS runner in a different shape: a killed `git cherry-pick` left
`packed-refs.lock` in the common git dir, and the resume's next ref write refused on it.

The refusal is **resumable and loses nothing** — no event is appended, no effect is half-applied,
and removing the file lets the next resume complete the publication exactly as it would have. The
cost is liveness, not correctness: an unattended run wedges until a human intervenes.

Pinned on the branch by `a_ref_lock_left_by_a_killed_compare_and_swap_refuses_resumably_until_removed`,
which asserts both the resumable refusal and the completion after removal. The residue sampler
removes git's common-dir ref locks after each kill, as an operator would
(`remove_git_ref_lock_residue`).

## Why it is deferred rather than repaired

Reclaiming the lock automatically means classifying it as residue the engine may delete, and no
`Ref.*` site registers a residue class in the frozen effects inventory. Adding one is a **Class C
change** under the `src/topology/**` freeze.

The condition is also pre-existing rather than introduced by PR8: any coordinator killed during a
ref write has always been able to leave one. PR8 is where it was first measured, not where it
began.

The owner's decision of 2026-09-07 was to accept it for v0.2 as an operator-remediated condition.

## What the change that takes this up should do

Register a `Ref.*` residue class in the frozen inventory and let recovery reclaim a ref lock it can
prove is its own. "Prove is its own" is the whole difficulty and the reason this is not a small
change: a lock held by a **live** concurrent Git process is indistinguishable by inspection from
one left by a dead one, so a reclamation that guesses wrong corrupts a ref write in progress. Any
proposal here needs an ownership test that does not rely on age or on the absence of a process the
engine did not spawn.

Weigh it against the premise rather than the frequency. Unattended operation is what upstroke is
for, and this is a condition that stops a run until a person arrives — so a low rate does not
settle it. Candidate for v0.3 alongside the other liveness work.
