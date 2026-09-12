---
id: PR247-DISPATCH-HEAD-WITNESS-RED-ON-WINDOWS
severity: P2
disposition: fixed
category: portability
pr: 247
reviewed_sha: 91fe35b008a0a612bb37ed43a9eb3b5b7487c8a3
location: src/engine/topology/recover/tests.rs:9925
provenance: pre_existing
first_bad: bcc3a533 on master, the recover fixture's `git init` without a line-ending pin; red since 45b5d429, the first test to compare checkout bytes through it
guard: `a_dependent_task_is_dispatched_into_its_dependencys_merged_work`, green on the Windows guest with the fixture's repository pinned and the contents assertion untouched; the tree without the pin is the reproduction. Fixed by the eighth repair round in `aaf289f1`. This file is kept and updated rather than deleted because the round's brief asked for the cause and the change to be recorded here; `reviews/findings/README.md` deletes a resolved finding, and whoever merges may delete this one with nothing lost — the body's row and `pr8-triage.md` §11 carry everything below.
---

## Failure sequence

`engine::topology::recover::tests::a_dependent_task_is_dispatched_into_its_dependencys_merged_work`
on CI's winguest leg at `91fe35b0` and again at `fc141710`, in the full suite, every other job of
both runs green — lint on three platforms, msrv on three, `upstroke-pr-policy`, and the same test
on ubuntu and macOS. `2299 passed; 1 failed; 40 ignored` both times:

```text
thread '…a_dependent_task_is_dispatched_into_its_dependencys_merged_work' panicked at
src\engine\topology\recover\tests.rs:9925:5:
assertion `left == right` failed: beta's agent reads what alpha merged, in the worktree it was
handed: DESIGN §26 verdict 1 dispatches at the run's integration head at dispatch, and beta is
the first task this run dispatches after a publication moved that head.
The checkout held ["candidate.txt", "seed.txt"]
  left: Some("the candidate edit\r\n")
 right: Some("the candidate edit\n")
```

The first version of this file said alpha's merged file was absent from beta's checkout. It was
not: the checkout held it, and the two lines above, which the first version did not quote, show
the content was right and the line endings were Windows'. Git for Windows installs
`core.autocrlf=true` in its system config (`C:/Program Files/Git/etc/gitconfig`); the recover
fixture's repository inherited it, having pinned identity and `core.logAllRefUpdates` and nothing
about line endings; and `git worktree add`, which reads a linked worktree's configuration from the
repository it belongs to, rendered alpha's LF blob as CRLF when it populated beta's checkout. The
test compares bytes, so it failed on the one platform whose Git rewrites them, twice and
deterministically, while the dispatch it witnesses was correct.

## The cause, established on the Windows guest

Reproduced on the persistent guest (Windows 10.0.26100.1742, git 2.50.1.windows.1, cargo 1.97.1)
in a fresh clone of `fc141710` with its own target directory, before anything was changed:

1. The test alone, as CI runs it: `FAILED`, the same panic, the same `left`/`right` pair.
2. The same test with `core.autocrlf=false` injected through the environment
   (`GIT_CONFIG_COUNT=1`, `GIT_CONFIG_KEY_0=core.autocrlf`, `GIT_CONFIG_VALUE_0=false`) and
   nothing else changed: `ok`. The test's later assertions run only when the contents assertion
   passes, and they all passed — the worker's HEAD is the published head, not the run's starting
   base; the durable `task_dispatched.base_sha` names it; the log replays twice equal. So
   `dispatch_head` returned the published head, the worktree was cut at it, alpha's file was on
   disk, and the assertion read it correctly: the bytes on disk were CRLF.
3. Outside the test, in a scratch repository on the guest: `line\n` committed under the
   inherited config is the blob `6c 69 6e 65 0a`; `git worktree add --detach` produces a file of
   `6c 69 6e 65 0d 0a`; after `git config core.autocrlf false` and `core.eol lf` in that
   repository, a second `git worktree add` produces `6c 69 6e 65 0a`, and `git config
   --show-origin` inside the new worktree names the repository's `.git/config`. The repository
   config is the layer a linked worktree reads, so a pin there governs the checkout production
   makes.

So the fault was in neither `dispatch_head` nor the worktree nor the materialisation: it was the
fixture inheriting an ambient Git setting, the shape `workspace_manager::fixture` already pins
against with a comment that describes this failure exactly, and the shape
`PR126-REVIEW2-NULL-TESTS-INHERIT-THE-HASH-FORMAT` recorded for the object format. The seventh
round verified its repair on Linux only, which is why the assumption went unnoticed until CI.

## What the change did

`Fixture::build` in `src/engine/topology/recover/tests.rs` pins `core.autocrlf=false` and
`core.eol=lf` in the fixture repository's config — the two settings `workspace_manager::fixture`
pins, in the layer both the fixture's `git` and production's `git worktree add` read. The
contents assertion is untouched; a SHA comparison is the shape that passed throughout the
original defect's life, and a comparison that normalised line endings would forgive exactly the
transformation it was meant to see through. The module's notes carry the reason beside the pin.
Verified on the guest with the repaired tree checked out at `aaf289f1`: the test alone `ok`, the
recover module `101 passed; 0 failed; 2 ignored`, and the full suite as CI runs it, recorded in
`pr8-triage.md` §11.3.

## Why it is re-filed at P2 `portability`

The failure sequence reaches no `DESIGN.md` §4 invariant, no trust boundary, no durable state and
no user data; it reaches one CI leg. That is the severity this branch's other test-fidelity rows
carry (`PR8-R7-DRIVER-REF-FUNNEL`, `PR8-R4-REVIEW-ORACLE`, `PR8-R2-SAMPLER-ORACLE`), and the
category `PR5-WORKSPACE-003` uses for a test that behaves differently on Windows only. The
provenance is `pre_existing`: the unpinned fixture is master's, since `bcc3a533`, and this slice's
seventh round is the first to compare checkout bytes through it. The id is unchanged, as the
README asks of a reclassification.
