---
id: PR262-WIDENED-RESERVATION-NEVER-REACHES-THE-SCHEDULER
severity: P2
disposition: deferred
category: correctness
pr: 262
reviewed_sha: a0f936c753973edd7d00cd99a0f64df50bb57207
location: 
provenance: pre_existing
first_bad:
guard: lab#3 or its successor — the owner of findings-sweep-schedule.py
---

## Failure sequence

**The site is `~/findings-sweep/findings-sweep-schedule.py`, which is not in this repository**, so this row carries no `location:` — the same shape as `PR5D-TOOLBOX-DISCARDS-CLIPPY-OUTPUT`, and recorded in `~/findings-sweep/TRIAGE-NOTES.md` for the same reason. It is filed here because this directory is what the sweep's owner reads.

The handoff tells an implementer who discovers a further required module to **stop and widen the reservation before writing**, as a one-line frontmatter commit. That instruction does not stop the collision it is written to stop.

`load()` (`findings-sweep-schedule.py:136`) reads the findings out of **the scheduler's own checkout**. `in_flight()` (`:193`) maps live claim **branch names** onto the `Finding` objects from that checkout and unions their modules (`mods |= f.modules`, `:203`). A worker's widened frontmatter lives on its repair branch; the scheduler never reads it. So:

worker A claims a finding → discovers mid-repair that it must also write module M → commits the wider `location:` on its repair branch → the scheduler re-runs, still deriving A's *original* modules → worker B, which reserves M, is admitted → both write M, and separate hunks in one file cherry-pick cleanly, so assembly does not catch it.

Reproduced with the scheduler's own `load`, `in_flight`, `claims`, `branch_for` and `allocate`, against two real checkouts — `c5b0714c` as the scheduler's view and this branch's head as A's repair branch, where `da014f8a` is the widening commit. A is `PR5-R2-OBJECT-GROUP-TAKES-NO-SITE`, B is `SWEEP-WORKTREE-007`, whose own row is unchanged between the two:

```text
1. scheduler's checkout, before A discovers the third module
     A location : src/workspace_manager.rs:2971, src/rundir/tests.rs:3655
     busy       : ['src/rundir', 'src/workspace_manager']
     B modules  : ['src/engine/topology']
     B admitted into src/engine/topology: True

2. same checkout, after A commits the wider location on its repair branch
     A location : src/workspace_manager.rs:2971, src/rundir/tests.rs:3655   <- unchanged
     busy       : ['src/rundir', 'src/workspace_manager']                   <- unchanged
     B admitted into src/engine/topology: True                              <- unchanged

3. what the scheduler would derive had the widening reached its view
     A location : … , src/engine/topology/attempt.rs:496
     busy       : ['src/engine/topology', 'src/rundir', 'src/workspace_manager']
     B admitted into src/engine/topology: False
```

**Publishing the widened row to the scheduler's view is necessary and still not sufficient.** Nothing re-examines a claim that is already live: `allocate` considers only `open_now = [f for f in found if f.id not in live]` (`:217`), and the module set it consults is a **union**, in which an overlap between two live claims is not representable. With A widened, published, and both A and B claimed:

```text
live claim branches        : ['fix-P3/correctness_the-object-group-apis-take-no-site',
                              'fix-P3/correctness_verify-failure-display-has-no-caller']
A.modules & B.modules      : ['src/engine/topology']      <- a real collision between two live claims
scheduler's modules held   : ['src/engine/topology', 'src/rundir', 'src/workspace_manager']
A, B in allocate's open_now: False, False
allocate() reports         : a batch of 1 in liveness/P1
anything naming the collision: no
```

## What the change that takes this up should do

Owner, as the ledger records it: lab#3 or its successor — the owner of `findings-sweep-schedule.py`.

Two mechanisms, and the instruction is sound only when both exist:

1. **Publication.** A widened reservation has to reach the view the scheduler allocates from before it means anything — the scheduler's checkout, or a claim-side record it reads alongside the branch list. Today the only thing a claim publishes is its branch *name*.
2. **A conflict check before work resumes.** Publication alone does not stop a worker already running in the added module. A widening should be checked against every live claim's modules — the intersection the current union discards — and the loser should wait or be told, which is a decision this row does not make.

Until both exist, the honest statement of what widening achieves is that it **records the wider reservation, which becomes authoritative only once the commit is on `master` and the scheduler's checkout has re-fetched it**. Until then it protects nothing — not the current allocation and not the next one, because the next allocation is derived from the same unrefreshed view. Once published it still does not protect against a worker already running in the module it adds. The handoff and PR #262's body were corrected to say exactly that on 2026-09-10; this row is the reason they had to be.

**Do not read this as a reason to keep an under-reserved row.** Recording the wider reservation is still the right act: it is the only thing that ever makes the added module exclusive, and sequence 3 above is what that looks like — after the row has landed and the scheduler has re-read it. What the instruction implied and does not deliver is protection that begins when the frontmatter is written. On the branch it is written on, the module stays free.

Recorded 2026-09-10 on `docs/findings-triage-locations`, from finding 2 of the round-5 review of `a0f936c7`, which labelled it **P2**. Related: `PR262-LOCATION-COMPLETENESS-UNPROVABLE`, the reason a worker discovers a module mid-repair at all.
