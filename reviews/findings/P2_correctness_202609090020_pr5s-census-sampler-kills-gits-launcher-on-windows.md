---
id: PR249-KILL-SAMPLER-WINDOWS-WRAPPER
severity: P2
disposition: deferred
category: correctness
pr: 249
reviewed_sha: 56ea88c9779f7787800bffbd6c64011c069edbc8
location: src/workspace_manager/tests.rs:9673
provenance: pre_existing
first_bad: PR5's four-command residue kill sampler (`SampledChild::spawn`), which spawns `git` by name; every kill it has delivered on the Windows lane since has ended Git for Windows' launcher, not git
guard: the PR9 and T-ATTEMPT samplers' fixture spawns the real binary (`fixture::sampled_git`) and the PR9 sampler's `KilledSample::while_writing` floor fails when no kill landed mid-write; PR5's census sampler has no such floor and is the open half
---

## Failure sequence

A kill sampler on the Windows lane spawns `git` by name and kills the child at a chosen moment.

    Command::new("git") on Windows -> resolves Git for Windows' cmd\git.exe, a 46 KB launcher
    -> the launcher starts the real mingw64\bin\git.exe as its own child and waits for it
    -> Child::kill terminates the launcher; the real git is never signalled
    -> the pick either had not begun (residue None) or runs on to completion (After, MERGE_MSG in place)
    -> the sampler records a kill that interrupted nothing, and every recovery it then proves is a
       recovery from a state a kill never left

Measured on the winguest lane at `56ea88c9`, by the PR9 repair-materialization sampler before its
fixture was changed: 30 kills in 32 spawns, ten `None`, twenty `After` with `MERGE_MSG` written,
none between the pick's first write and its last. On Linux the same sampler's kills land mid-write
about one time in six.

## What this pull request changed, and what it did not

`KillableGitChild::spawn` (`src/workspace_manager/fixture.rs`), the fixture of the PR9
materialization sampler and of the T-ATTEMPT two-command sampler, now spawns the real binary:
`git --exec-path` names `<prefix>/mingw64/libexec/git-core`, and `<prefix>/mingw64/bin/git.exe`
beside its DLLs is the child (`fixture::sampled_git`, resolved once; `git` where that layout is
absent). The PR9 sampler's floor — at least one kill while the pick was writing — therefore binds on
Windows too.

PR5's four-command census sampler, `SampledChild::spawn` in `src/workspace_manager/tests.rs`, still
spawns `git` by name. Its kill floors count "kills that landed" by exit status, which the launcher's
death satisfies, so its Windows histogram has been describing the launcher's death, not git's. This
is reasoned from the same spawn, not measured on that sampler.

## Why it is deferred, and what is and is not frozen

`effects/residue-classes.json` freezes the census's **declarations** — per site, the residue
elements and classes, and the sampling N — and says of the observed histogram that it "is
machine-varying and cannot be pinned in a file compared byte-for-byte", so it is emitted to
`effects/residue-histogram.json` on every run and checked only for accounting for every sample.
Routing `SampledChild::spawn` through `fixture::sampled_git` therefore changes no declaration and
no frozen count: the Windows histogram would change, and that file is not frozen. What defers the
change is its scope, not a frozen contract: the census is PR5's evidence, its samples cover four
commands (`worktree add`, `add -A`, `write-tree`, `cherry-pick`), and kills that reach git on
Windows for the first time need the guest to show every one of those commands' recoveries against
residue a launcher's death never left — a measurement with its own review, which this pull request
did not make and does not claim. (The first version of this file said the census's evidence record
was "PR5's frozen contract" and that the owner decides what its Windows samples must show; PR #249's
third-round record review read the evidence file and corrected it.)

## What the change that takes this up should do

Route `SampledChild::spawn` through `fixture::sampled_git`, run the census on the Windows guest,
and re-read its Windows kill floors against kills that now reach git, recording the histogram those
kills produce for each of the four commands. If a later pass labels this P1, the disposition
becomes escalate-to-owner rather than still-deferred, because the census's evidence is PR5's and a
change to what it shows on Windows is the owner's to accept.
