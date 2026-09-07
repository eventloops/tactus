---
id: SAMPLER-A-WRITER-THAT-LEAVES-THE-GROUP-ESCAPES-THE-BARRIER
severity: P2
disposition: deferred
category: correctness
pr: 145
reviewed_sha: c3a6665819b850434e1c87c4eae1d93f8293e09a
location: src/workspace_manager/tests.rs:9889
provenance: pre_existing
first_bad:
guard: `WORKSPACE-GIT-BUILDER-DOES-NOT-PIN-ATTRIBUTES-OR-FILTERS`, which owns the configuration half, and the `remove_worktree` convergence stream, which owns what a removal does when a writer is there anyway; escalates to the owner if a later pass labels it P1 or P2 rather than accepting the deferral
---

## What the group kill closes, and what it does not

PR #145 kills the sampled Git child's **process group** and waits for that group to be empty before
anything reads the worktree. That closes the `DirectoryNotEmpty` race for every descendant that
stays in the group, which is every descendant `git worktree add` spawns on the machines this suite
has run on.

**It does not close it for a descendant that leaves the group.** A process that calls `setsid` — or
`setpgid` into a group of its own — is no longer named by `kill(-pgid, SIGKILL)` and no longer
counted by `kill(-pgid, 0)`. The barrier then reports the group empty while that process is still
writing, and the classification and forced removal that follow race it exactly as they did before
the kill was a group kill. The barrier is honest about what it observes — "no entry of this group
remains" — and this finding is about the gap between that and "nothing is writing".

This is why the fifth frontier pass refused "closed on Unix" as written, and it is right to: the
sentence is true of the group and was being read as true of the worktree.

## Failure sequence

The user's global attributes assign a clean or smudge filter -> a sampled `git add` or
`git worktree add` starts that filter as a child, which
`WORKSPACE-GIT-BUILDER-DOES-NOT-PIN-ATTRIBUTES-OR-FILTERS` records as reachable under every pin
`WorkspaceManager::command` sets -> the filter forks and calls `setsid`, as a daemonising helper
does -> `kill(-pgid, SIGKILL)` reaches the sampled command's group and not the helper ->
`settle_group` polls `kill(-pgid, 0)`, gets `ESRCH`, and reports `Empty` -> the sample classifies
the residue and `recover_sample` forces the worktree's removal while the helper is still writing
into it -> `remove_worktree` answers `Filesystem { operation: "remove", … DirectoryNotEmpty }` and
`expect("forced removal converges")` panics.

**Nothing has produced this sequence.** It is written from the mechanism, not from an observation:
the escape half is `setsid`'s documented effect, and the reachability half is pass 4's `FILTER_RAN`
probe. No CI leg and no build-box run has met it, because none of them has a filter configured.
That is why this is P2 with a stated mechanism rather than a red run with a fingerprint.

## What the change that takes this up should do

Two things, and neither belongs in a test-only pull request:

1. **Stop the escape from being reachable through configuration.** Pin what a funnel Git child sees
   of the user's configuration, which is `WORKSPACE-GIT-BUILDER-DOES-NOT-PIN-ATTRIBUTES-OR-FILTERS`
   and is a product behaviour change with a `DESIGN.md` sentence in it. This does not make the
   escape impossible — a hook or a filter the *repository* configures is a different question — but
   it removes the path this sequence takes.
2. **Contain rather than signal.** A group is a signalling convenience and not a containment
   boundary: on Unix only a subreaper (or a cgroup, on Linux) observes a process that leaves the
   group, and on Windows the Job Object in `agent::proc`'s private `windows_job` module is the
   thing that does hold an escaping child. Production's containment already reaches further than
   the sampler's kill does; the durable repair is the sampler taking production's containment
   through a test-only seam, which is what `SAMPLER-WINDOWS-STILL-KILLS-A-BARE-CHILD` asks for on
   the other platform and for the same reason.

Until then the honest claim is the narrow one, and it is what the source says: the barrier
establishes that no entry of the sampled command's process group remains, which is weaker than "no
writer is left" by exactly this finding.
