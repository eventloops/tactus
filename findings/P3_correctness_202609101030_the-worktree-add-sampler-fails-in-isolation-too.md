---
id: G4R3-WM-ADD-SAMPLER-FAILS-IN-ISOLATION
severity: P3
disposition: deferred
category: correctness
pr: 172
reviewed_sha: 7e0110a13acce525567a7ac43abb016a232d6236
location: src/workspace_manager/tests.rs:9088
provenance: pre_existing
first_bad:
guard: the round that takes up the workspace-manager sampler with PR172-SAMPLER-REFUSED-A-TORN-WORKTREE-LIST-RECORD and SAMPLER-RECOVERY-PROVEN-IS-NOT-PROVEN-FOR-AN-EMPTY-GITDIR; this row supplies the rate those two asked for and withdraws the "passes alone" reading one of them rests on
---

## Failure sequence

`workspace_manager::tests::sampled_git_child_kills_every_residue_classified_and_recovered` is PR5's
sampler and is not a G4 site. Two of its `Worktree.Add` failures are already filed, each with its
own fingerprint. `PR172-SAMPLER-REFUSED-A-TORN-WORKTREE-LIST-RECORD` records that it *"passes alone
at the same head"* and asks for a rate before anything is called a flake. **The rate is now
measured, on Linux at `7e0110a1`, and it fails alone too.**

    G4 run 3, build box, x86_64-unknown-linux-gnu, git 2.43.0, 2026-09-10
    -> full cumulative suite, attempt 1: 1 failure in 1 run, at src/workspace_manager/tests.rs:9088
       "Worktree.Add: the classifier refused 1 of 8 samples ... [\"run 2: git error: worktree list
       record 1 names a HEAD but neither a branch nor a detached checkout\"]"
       — the PR172 fingerprint
    -> the same test alone, --exact, ten consecutive runs: 9 passed, 1 failed, at
       src/workspace_manager/tests.rs:10339 with
       "forced removal converges: Git { message: \"worktree registration
       .../worktrees/kalpha-g1 has an empty gitdir\" }"
       — the SAMPLER-RECOVERY-PROVEN-IS-NOT-PROVEN-FOR-AN-EMPTY-GITDIR fingerprint, and expressly
       not PR136-SAMPLER-FORCED-REMOVAL-DOES-NOT-CONVERGE, whose signature is DirectoryNotEmpty
       (os error 39)
    -> attempt 2 of the full suite: green, 2443 passed, 0 failed, 44 ignored

Two facts this adds to the two existing rows. **First, isolation is not a discriminator**: PR172's
"passes alone at the same head" was one observation, and ten isolated runs here produce a failure,
so a green single-test rerun is not evidence that a suite red was contention. **Second, one test has
two distinct failure fingerprints at one head**, so a red must be read by its assertion text and
message and never by the test name — reading it by name would attribute an `empty gitdir` recovery
failure to a torn `worktree list` parse, or the reverse.

Logs: `~/tactus-artifacts/g4r3-evidence-7e0110a1/gates/sampler-wm-isolated.log` (the ten runs) and
`~/eight-logs/7e0110a-failed-20260910T095824Z/03-test.log` (the suite red), on the build box.

Not a G4 failure and not repaired here: G4's own sampled evidence is
`Object.RepairMaterialize`'s, which took 40 kills in 40 spawns over five runs at this sha with zero
classifier refusals.

## What the change that takes this up should do

Take it up with the two existing rows rather than alone, and use this rate as the starting point
rather than re-measuring it. Decide the two questions those rows leave open: whether a
`git worktree list --porcelain` record carrying `HEAD` and neither `branch` nor `detached` is a torn
read of a worktree mid-`add` that the classifier should read as interrupted rather than refuse; and
what `recover_sample` should do with a registration whose `gitdir` file is empty, which
`registration_checkout` refuses. Whatever is decided, keep the sampler's refusal of an inspection
that failed: answering "no class" for a read that did not complete is the thing the assertion exists
to stop.
