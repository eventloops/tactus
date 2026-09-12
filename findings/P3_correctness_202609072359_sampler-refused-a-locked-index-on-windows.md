---
id: PR247-SAMPLER-REFUSED-A-LOCKED-INDEX-ON-WINDOWS
severity: P3
disposition: deferred
category: correctness
pr: 247
reviewed_sha: 6ac29984ec1af8b06f409905ea135c47b74c59ed
location: src/engine/topology/recover/tests.rs:9421
provenance: undetermined
first_bad: —
guard: deferred: one red on CI's winguest leg in a module PR #247 does not touch, with the other ten checks green at the same head and the same test green on ubuntu and macOS in the same run; a rate is owed before anything is called a flake (§12), and the owner is the change that next opens the cherry-pick sampler
---

## Failure sequence

`engine::topology::recover::tests::sampled_cherry_pick_child_kills_every_residue_classified_and_recovered`
on CI's winguest leg at `6ac2998`, in the full suite -> the sampler kills the `git cherry-pick`
child at a sampled point -> `classify_object_residue` reads the staging worktree's `index.lock` to
decide what the interrupted pick left -> the read fails with

```text
failed to read C:/Users/Administrator/AppData/Local/Temp/upstroke-pr7e-3128-sample-2-114/repo/.git/worktrees/s1\index.lock: Access is denied. (os error 5)
```

-> the test's own guard fires, because an inspection that failed is not a residue in no class:
"the classifier refused 1 of 8 samples". `2296 passed; 1 failed; 40 ignored`, and every other leg
of the same run — lint on three platforms, msrv on three, test on ubuntu and macOS — is green.

**Second observation, `c8aebfbc`:** the next head on the same branch, one record commit later, is
green on all eleven checks including this leg. One red in two runs. This session's token cannot
re-run a job, so the second observation is the next push's own run and not a rerun of the first.

## Why it is filed rather than repaired here

**It is a different fingerprint from the two already filed.** `PR172-SAMPLER-REFUSED-A-TORN-WORKTREE-LIST-RECORD`
is the *workspace* sampler refusing a `git worktree list` record, and
`PR136-SAMPLER-FORCED-REMOVAL-DOES-NOT-CONVERGE` is `DirectoryNotEmpty` (os error 39) in the same
place. This is the *cherry-pick* sampler, a different classifier read, and `Access is denied`
(os error 5) — which on Windows is what a handle still open on a file the killed child held, or a
delete-pending state, answers. Nothing in PR #247's sixth repair round reaches it: its two code
changes are the Docker CLI's evidence for a gone container (`src/runner/container.rs`) and the
evidence a frozen repair carries (`src/engine/topology/integrate.rs`), and neither touches Git, a
worktree, or the residue classifier.

## What the change that takes this up should do

Count before concluding — that is what this file exists for. If it recurs, decide whether a
`.git/worktrees/<slot>/index.lock` the killed child's handle still holds is a *torn read* the
classifier should answer as `Interrupted` rather than refuse, the way `remove_git_ref_lock_residue`
already treats git's own ref-lock residue as an operator would. On Windows a handle survives the
process by the length of the kernel's delete-pending window, so a read that raced it is not
evidence of anything about the residue, and refusing is the conservative answer the sampler then
reports as a refusal rather than as a class.
