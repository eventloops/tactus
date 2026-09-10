---
id: G4B-O3-LIVE-WORKTREE-MISSING-CLOSE-KEEPS-THE-INTENT
severity: P3
disposition: deferred
category: correctness
pr: 249
reviewed_sha: 7e0110a13acce525567a7ac43abb016a232d6236
location: src/engine/topology/run.rs:1201
provenance: pre_existing
first_bad:
guard: the round that makes the live retry path's `Close` arm reclaim the closed generation's worktree (when residue, not absence, failed the verification) and its durable intent, as recovery step (e) already does for a close it makes — or that records in `docs/internals/engine/topology/run.md` and R9 that a live close defers the reclaim to the next resume
---

## Failure sequence

A retained generation is retried in place by the incarnation that retained it, and `Worktree.Verify`
fails — the checkout is gone, or an interrupted command left administrative residue in its git dir.
`retry` (`src/engine/topology/settle.rs`) answers `RetryOutcome::Close { WorktreeMissing }`, which is
the tabled action, and the loop's `retry_ready` appends the `generation_closed` and forgets the
retained session. It calls nothing on the workspace manager: at `src/engine/topology/run.rs:1201` the
`Close` arm is `self.emit(TopologyEventBody::GenerationClosed { data: closed }, …)?;
self.retained.remove(&key);` and nothing else.

    attempt_finished{Retained} durable; the worktree directory removed (or `index.lock` left in it)
    -> step: select -> Retry -> retry_ready -> Worktree.Verify fails
       -> generation_closed{WorktreeMissing, LineageHeld}
    -> the closed generation's durable intent is still under `intents/`
    -> the next dispatch opens generation 1 in its own slot, so nothing aliases; the stale intent
       and, in the residue case, the worktree itself stay until a fresh incarnation's
       `reclaim_closed_generations` sweeps every closed generation whose intent is present

**Re-measured at `7e0110a1`** by the G4 run-3 measurement `G4G`, driven through the production loop
(gate report §5.2):

    G4G after step2: generation_closed=["g0 reason=WorktreeMissing lease=LineageHeld"] ... intents_has=true worktree_exists=false
    G4G after step3: ... closed_g0_intent_still_present=true g1_intent_present=true g1_worktree_exists=true

R9's lifecycle says a generation that is `Closed` is *pruned (forced); intent removed*. Between a
live `WorktreeMissing` close and the next resume the intent is in neither `resumably_open` nor
`pruned` for the row's own words: it is durable residue of a generation the fold has closed,
reclaimed only by a process that has not started. A run that is never resumed keeps it until run end,
whose closure procedure is PR10's.

Not a behaviour defect that reaches any invariant: no dispatch reuses the slot, the intent names a
generation the fold knows is closed, and the next resume converges. It is an accounting gap between
R9's stated class and the live path, of the same family as PR #249's crash-review finding 2 (closed
generations keeping their worktrees), which that round fixed for the recovery-side close and not for
the live one.

First filed by G4's second run at `81ee09ef` on the unmerged branch `gate/g4-corrected-range`;
re-verified and carried here because the range does not close it — the `Close` arm is unchanged
between `81ee09ef` and `7e0110a1`.

## What the change that takes this up should do

In `retry_ready`'s `Close` arm, after the `generation_closed` append succeeds, remove the closed
generation's worktree (when it still exists) and its intent through the workspace manager, as
`reclaim_closed_generations` does on resume — or, if the design prefers the live path to leave
reclaim to recovery, say so in R9 and the run notes and add the missing assertion to
`retained_worktree_with_residue_closed_not_retried`. Either way, add to the settle test the read
`G4G` made: the intents list after the close.
