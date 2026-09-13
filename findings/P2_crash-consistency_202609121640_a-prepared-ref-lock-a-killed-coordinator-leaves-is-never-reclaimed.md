---
id: PR275-PREPARED-REF-LOCK-IS-NEVER-RECLAIMED
severity: P2
disposition: deferred
category: crash-consistency
pr: 275
reviewed_sha: b1f6b65307132c446d4daf2dd8069886ce4ff90e
location: src/workspace.rs:242
provenance: pre_existing
first_bad: undetermined
guard: the next change to prepared_update_ref
---

## Failure sequence

Found by the `moonshotai/kimi-k3` review pass on `b1f6b653`, **read-verified** against the tree (that
lens had read-only tools and no execution), and re-verified by me independently.

`Workspace::prepared_update_ref` (`src/workspace.rs:242`) spawns `git update-ref` children through
`run_git_with_private_hooks` with **no cleanup lease and no lock reclaim**. Measured: neither
`hold_cleanup_lease_for_child` nor `reclaim_own_ref_lock` appears anywhere in `src/workspace.rs`.
Its four spawn sites are `src/workspace.rs:985`, `:1140`, `:1157` and `:1200`, and it is production
code, reached from `engine/coordinator.rs:682` (`advance_prepared_commit`) and
`engine/resume.rs:455` and `:505`.

The refs it writes — `refs/upstroke/prepared/<run>/<seq>` and the run branch under `refs/heads/` —
are **outside `RUN_REF_ROOT`** and are not integration-site refs, so none of the three reclaiming
primitives this pull request adds covers them.

**A -> B -> failure.** A coordinator is killed inside `prepared_update_ref`, between Git creating
`<ref>.lock` and renaming it over the ref -> the lock file survives with no owner -> the next resume
on that path calls `prepared_update_ref` for the same ref and Git refuses with
`Unable to create '<ref>.lock': File exists` -> **the resume wedges until an operator removes the
file by hand.** That is the same failure shape as `PR8-CRASH-002`, the P1 this pull request closes
for the ref funnel, on a ref namespace the funnel does not reach.

## Why P2 and deferred rather than P1 and blocking

- **Pre-existing, not introduced.** This pull request neither creates nor widens the residual; it
  closes the same shape for the funnel and leaves this path exactly as it found it.
- **Partially protected.** The `Workspace` flow takes both locks (`engine/resume.rs`), and on Unix
  the reaper holds the cleanup lease while settling a killed coordinator's descendant group, so
  `WorktreeLock::acquire_in` via `Lock.ObserveCleanupHold` and `RunLock::acquire` via
  `Lock.ProbeCleanupExclusive` refuse a resume while a live writer exists. The exposure is the
  *dead* writer's orphaned lock, not a live one, and no reclaim here could remove a live
  `Workspace` child's lock.
- **The pull request's own bar.** A lens P1 blocks only when it is reachable in normal use or
  triggerable without push access *by the change under review*. This is a defect in a file the diff
  does not touch, so it is queue work rather than a merge blocker.

**If a later reading judges this P1-equivalent, it enters the P1 queue on its own merits** — the
grading here is deliberate and recorded, not an attempt to keep it out of the way.

## What the change that takes this up should do

Give `prepared_update_ref` the same treatment the funnel primitives received: hold the run's cleanup
lease across the spawn, and reclaim a lock the repository can prove is the engine's own and stale,
by the same four facts — no live writer, not packed elsewhere, content empty or naming exactly the
value about to be written, and a name inside a namespace only the engine writes. The prepared-ref
namespace will need its own `RefSite` treatment or an equivalent, since `RUN_REF_ROOT` does not
contain it.

## The body claim this corrects

Fact 1 of the liveness proof read *"Every `git update-ref` child the engine spawns now holds the
run's cleanup lease"*. **That universal is false** while `prepared_update_ref` exists. The body now
narrows the claim to the funnel's children and points at this finding, because the body is the
permanent auditable record and a P1 crash-consistency record must not assert coverage the tree
disproves.

## Why this cannot simply be fixed: `src/workspace.rs` is FROZEN

`effects/allowlist.toml:840`, under the `[[legacy]]` header at `:839`:

```
[[legacy]]
path = "src/workspace.rs"
legacy_effect = """
LEGACY-EFFECT: the schema-1..3 engine's Git operations — branch, checkout,
stage, commit, rollback — ... `invariants_preserved[1]` requires this
module's behaviour untouched; the schema-4 equivalents live behind funnels in
`crate::workspace_manager` and nothing here calls them."""
```

`invariants_preserved[1]` requires this module's **behaviour untouched**, and the file's own header
says the legacy section "may only shrink after PR5 (the test compares against the frozen list)". So
giving `prepared_update_ref` a lease and a reclaim is a **behaviour change to a frozen module** and
cannot be done under the current freeze.

**This is therefore a second concrete instance for the standing unfreeze-`workspace.rs` escalation**
— not merely a wish to tidy the module, but a crash-consistency residual of the same shape as a P1,
which the freeze forbids closing. Recorded here so the escalation can cite two cases rather than
one.

Note `effects/allowlist.toml` is **untouched** by this pull request (measured: zero occurrences in
its diff), so nothing here loosens or leans on the freeze.

## Provenance of this finding, stated exactly

Raised by the `moonshotai/kimi-k3` pass on `b1f6b653` via OpenRouter — **read-verified, not
executed**: that lens had `read,grep,find,ls` and said so itself, *"this environment gives me no
execution capability."* Independently re-verified against the tree by the orchestrator and by the
owner. The three load-bearing facts each hold: the body asserted the universal at body line 62;
`hold_cleanup_lease_for_child` and `reclaim_own_ref_lock` occur **zero** times in
`src/workspace.rs`; and `src/runner/container/view.rs:519`, the only other `update-ref` spawn outside
the funnel, is test-only (nearest `#[cfg(test)]` above it is line 414).
