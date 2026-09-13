---
id: PR262-DOCKER-OBSERVE-READS-RUNNING-AFTER-PROCESSGONE
severity: P2
disposition: deferred
category: correctness
pr: 262
reviewed_sha: a05612688843651a6d77f1db883f6b4d56f82345
location: src/runner/container/tests.rs:4104, src/runner/container.rs:1332
provenance: pre_existing
first_bad:
guard: project owner — the slice that next opens the container runtime's liveness read
---

## Failure sequence

`runner::container::tests::real_docker_lists_the_state_the_settlement_observation_reads` panicked at `src/runner/container/tests.rs:4104`, `left: Running  right: Exited`. The sequence is three statements long and all of it is in the test: `docker.start(&name)` (`:4098`), then `docker.stop(&name, StopMode::Kill)` **returns `Settled::ProcessGone`** (`:4100-4103`), then `docker.observe(&name)` — the very next statement — **still answers `Liveness::Running`** (`:4104`). The daemon told the runtime the process was gone and then listed the container as running.

`observe` is `src/runner/container.rs:1332`; it reads `docker ps --all --filter name=… --format "{{.Names}}\u{1f}{{.State}}"` (`CONTAINER_STATE_FORMAT`, `:1111`) and maps the listed state through `listed_state` (`:1113`). Whether the defect is a daemon-side lag between kill and listing, a `Settled::ProcessGone` derived from something weaker than the listing, or a state string the mapping reads as running, **is not answered here.**

**Evidence, and what it does and does not establish.** Observed on the build box at head `a05612688843651a6d77f1db883f6b4d56f82345`, three consecutive full-suite runs of `cargo test --all-targets --all-features` through `upstroke-build`, each attributed to this worktree by its `Compiling upstroke (/srv/worktrees/fsweep-triage)` line:

```text
run 1  rc=101  2442 passed, 1 failed, 44 ignored
               runner::container::tests::real_docker_lists_the_state_the_settlement_observation_reads
               src/runner/container/tests.rs:4104   left: Running   right: Exited
run 2  rc=0    2443 passed, 0 failed, 44 ignored
run 3  rc=0    2443 passed, 0 failed, 44 ignored
isolated       8 of 8 green   (the test alone, repeated)
```

**Intermittent by this project's own standard**: the identical commit produced both colours. **Not caused by the change in front of it**: PR #262's diff is confined to `findings/`, and `src/`, `Cargo.toml` and `Cargo.lock` are byte-identical to `master` — `git diff origin/master...HEAD -- src/ Cargo.toml Cargo.lock` is empty. Linux, real Docker; the test skips itself where no daemon is reachable (`docker_gate`, `:4027`), so it does not run on every machine that runs the suite. A later run at `a0f936c753973edd7d00cd99a0f64df50bb57207` was green: **one green run is not a retirement**, since the same head has already produced both colours.

**Filed as an independent observation. It is deliberately not classified.** `findings/` carries no row with this fingerprint — checked across the four members of `CLASS-INTERMITTENT-SUBPROCESS-KILL-SETTLE-RESIDUE-FAILURES`, which are a macOS pre-exec process group, two Windows settle-kill signatures and a Linux empty-gitdir residue; none is a Docker liveness read after a settled kill. Whether this is a fifth member of that class or a separate row is a **scope decision for the class's owner**, and that row's own guard is "project owner, undirected". What a triage pass can do is make the fingerprint findable after merge, which is what this file is for.

## What the change that takes this up should do

Owner, as the ledger records it: project owner — the slice that next opens the container runtime's liveness read.

**Decide the class question first**, because it changes what "taking this up" means: a fifth member of the intermittent class is evidence for that class's untested common mechanism, and a standalone Docker defect is a runtime bug with a two-statement reproduction.

Then, whichever it is, the open question is the one the observation poses and does not answer: **what `Settled::ProcessGone` is derived from, and whether it can be true while the daemon's own listing still says `running`.** If the kill path can return settled ahead of the listing it is asserted against, the assertion at `:4104` is reading a state that is legitimately not yet visible, and the repair is in the runtime — `observe`, or what `stop` waits for — not in the test. If it cannot, the listing is being misread and `listed_state` is where that lands. Both sites are in `src/runner/container`, which is the one module this row reserves.

**A guard the repair should carry:** kill, then observe, repeated under the real daemon until the interval between `Settled::ProcessGone` and a `running` listing is either shown not to exist or handled. The existing test is that sequence run once; it caught this in one run of three, so a repair that only makes one pass green has not established anything.

Recorded 2026-09-10 on `docs/findings-triage-locations`, from finding 3 of the round-5 review of `a0f936c7`, which labelled the omission **P3**; **P2** here is this pass's judgement from the consequence above, matching the four fingerprint rows already in this directory. `MAINTAINING.md` step 5 is why it is a file: every open finding gets one, including one unrelated to the change that found it.
