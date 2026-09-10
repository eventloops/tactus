---
id: PR262-WARM-BUILD-SLOT-SERVES-ANOTHER-WORKTREE
severity: P2
disposition: deferred
category: correctness
pr: 262
reviewed_sha: e8fcbdd8269449619b550393e5bfd27309a83582
location: 
provenance: pre_existing
first_bad:
guard: the project owner — the build box's tooling, not the tree
---

## Failure sequence

**The site is `~/bin/upstroke-build`, which is not in this repository**, so this row carries no `location:` — the same shape as `PR5D-TOOLBOX-DISCARDS-CLIPPY-OUTPUT`, whose subject is the same file, and `PR262-WIDENED-RESERVATION-NEVER-REACHES-THE-SCHEDULER`. It is filed here because this directory is what the sweep's owner reads.

`upstroke-build` binds `CARGO_TARGET_DIR` to a slot from a bounded pool, and **the slot is chosen by lock availability, never by worktree**: `~/bin/upstroke-build:114-139` walks `slot1..slotN`, takes the first whose lock `flock -n` succeeds on, and `exec env CARGO_TARGET_DIR="$d" … "$@"` (`:136`). `$PWD` is never consulted. That is deliberate and it is the point of the wrapper — a bounded set of repeating target paths is what makes sccache hit, and the header's own measurement puts one-target-dir-per-worktree at 0/55 hits against 54/55 (`:8-20`). The consequence is not the point: **many worktrees share one target directory, and everything in it that is not source-path keyed belongs to whichever worktree built there last.**

So a run can be handed the previous occupant's build. Its compile is skipped, cargo prints no `Compiling` or `Checking` line naming the crate path, and it exits 0. **Measured at 160 tests' difference on byte-identical source**; the sweep's standing rules carry that measurement and the rule derived from it — a green `upstroke-build` run is not evidence unless it names its own crate path (`~/findings-sweep/BRIEF-PREAMBLE.md`).

Executed on the build box, 2026-09-10, at `e8fcbdd8`:

```text
the slot ignores the worktree — three checkouts, one after another:
  /srv/worktrees/fsweep-triage   ->  CARGO_TARGET_DIR=/mnt/ramtarget/slot1
  /srv/worktrees/fsweep-251      ->  CARGO_TARGET_DIR=/mnt/ramtarget/slot1
  /srv/worktrees/fsweep-232      ->  CARGO_TARGET_DIR=/mnt/ramtarget/slot1

what the pool holds at rest — distinct /srv/worktrees roots named by each slot's
.d files, and the worktree named by that slot's top-level debug/libupstroke.d:
  slot1    5   <no libupstroke.d>          (this session cleaned the crate here)
  slot2    4   <no libupstroke.d>
  slot3   10   /srv/worktrees/sweep-fixture
  slot4    3   /srv/worktrees/topo-effects-parent
  slot5    9   /srv/worktrees/pr7-b
  slot6    6   /srv/worktrees/pr7-b
  slot7    6   /srv/worktrees/pr7-b
  slot8    5   <no libupstroke.d>
```

Five of the eight slots hold a top-level `debug/libupstroke.rlib` built from a **named other worktree**, and every slot's dependency files name between three and ten distinct checkouts. The per-crate outputs under `debug/deps/` are hash-keyed and coexist; the top-level names — `debug/libupstroke.rlib`, `debug/upstroke`, and the probe binaries beside them — are not.

**What this row does not settle** is which cargo decision produces the stale answer: whether a warm slot's fingerprints let cargo consider the current source fresh, or the reported test count comes from a binary the current source never produced. The 160-test measurement establishes that two builds differed; it does not name the step that skipped. Reproducing that number was not attempted here.

**The consequence is about evidence, not only about builds.** Every "gates green" claim made on this box is unattributable unless the run's own output named its crate path — including claims already recorded in merged pull request bodies. This pull request's gate table shows the shape: of three consecutive full-suite runs, run 2 returned rc=0 with **no crate-path line and was discarded as evidence for exactly that reason**, while runs 1 and 3 carried `Compiling upstroke v0.1.0 (/srv/worktrees/fsweep-triage)`.

## What the change that takes this up should do

Owner, as the ledger records it: the project owner — the build box's tooling, not the tree. `~/bin/upstroke-build` is outside this repository, so **no pull request here can close this row**, on the same terms as `PR5D-TOOLBOX-DISCARDS-CLIPPY-OUTPUT`.

Two things, and they are separable:

1. **Make attribution structural rather than a reading discipline.** The wrapper already knows both facts it needs: the slot it took and the directory it was invoked from. Stamping a slot with its current occupant and reacting when that changes turns "check that the output named your path" from a rule every session must remember into an invariant the wrapper holds. Whether the reaction is a `cargo clean -p` or a refusal is a cost decision this row does not make — a clean on every occupant change costs a full crate rebuild, which is the cost the slot pool exists to avoid, so the answer may be to widen the pool, or to bind slots to worktrees for gate runs only.
2. **Answer the mechanism question above**, because a repair that only stamps the slot leaves the stale-serve unexplained: if cargo can consider a foreign warm slot fresh, that is worth knowing independently of this wrapper.

**A guard the repair should carry**, executable from the wrapper's side alone and with an oracle that does not depend on parsing cargo's progress lines: build worktree A in slot N, then build worktree B in slot N, and assert that B's run either recompiled — its output names B's path — or was refused. `<slot>/debug/libupstroke.d` names the worktree whose source the top-level artifact came from, which is the assertion's ground truth. The measurement above is the unrepaired state: run that check against slots 3 to 7 today and it fails on the stamp alone.

Until then the workaround is the one this pull request used and the standing rules require: `cargo clean -p upstroke` through the wrapper before each gate command, and quote the line naming the crate path. A run without that line is not evidence, whatever its exit code.

Recorded 2026-09-10 on `docs/findings-triage-locations`, from finding 3 of the review of `e8fcbdd8` — the sixth round on PR #262, which the repair brief numbered 7 — and it labelled the **recording omission** P3; **P2** here is this pass's judgement of the defect from the consequence above, matching `PR5D-TOOLBOX-DISCARDS-CLIPPY-OUTPUT`, the other row whose subject is this file, and `category: correctness` is the closest word in the closed vocabulary for a build-infrastructure defect, as that row also records. **Filed as an independent observation, not a member of `CLASS-INTERMITTENT-SUBPROCESS-KILL-SETTLE-RESIDUE-FAILURES`**: that class's four members are a macOS pre-exec process group, two Windows settle-kill signatures and a Linux empty-gitdir residue, every one an intermittent failure of the product under test, whereas the slot allocation demonstrated above is a deterministic property of the wrapper and reproducible on demand. Folding it in would put a tooling defect inside a class whose subject is the engine's own kill and settle paths. `MAINTAINING.md` step 5 is why it is a file: every open finding gets one, including one unrelated to the change that found it.
