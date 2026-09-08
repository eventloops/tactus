---
id: PR247-DISPATCH-HEAD-WITNESS-RED-ON-WINDOWS
severity: P1
disposition: open
category: correctness
pr: 247
reviewed_sha: 91fe35b0
location: src/engine/topology/recover/tests.rs:9925
provenance: undetermined
first_bad: 45b5d429
guard: open, and the next run on this branch settles it: this is the seventh repair round's own regression test failing on the platform its fix was not run on. Unlike the sampler fingerprints beside it, a red here is either an intermittent or the repair not holding on Windows, and those two have very different consequences. A rate is owed before anything is called a flake (§12), and this session's token cannot re-run a job.
---

## Failure sequence

`engine::topology::recover::tests::a_dependent_task_is_dispatched_into_its_dependencys_merged_work`
on CI's winguest leg at `91fe35b0`, in the full suite:

```text
thread '…a_dependent_task_is_dispatched_into_its_dependencys_merged_work' (5312) panicked at
src\engine\topology\recover\tests.rs:9925:5:
assertion `left == right` failed: beta's agent reads what alpha merged, in the worktree it was
handed: DESIGN §26 verdict 1 dispatches at the run's integration head at dispatch, and beta is
the first task this run dispatches after a publication moved that head.
The checkout held ["candidate.txt", "seed.txt"]
```

`2299 passed; 1 failed; 40 ignored`. Every other leg of the same run is green — lint on three
platforms, msrv on three, `upstroke-pr-policy`, and **the same test on ubuntu and macOS**.

Alpha's merged file is absent from beta's checkout. The test asserts on the worktree's contents
rather than on a recorded SHA, deliberately, because a SHA assertion is the shape that would have
passed throughout the defect's life.

## Why this one is not filed as a flake

The three sampler fingerprints already in this directory are reds in modules PR #247 does not
touch, found by tests that kill a child at a sampled point and are inherently racy. **This is
different in kind**, and the difference is the reason for the severity:

- It is **this round's own regression test**, added by `10714de5` to guard the P1 the twelfth
  frontier review found: `run.rs`'s `dispatch_request` built every `DispatchRequest` with
  `run_started.base_sha`, so a task dispatched after a publication could not see its dependency's
  merged work. `45b5d429` repaired it.
- The test has **no history**. This is its first Windows run, so there is no rate to compare
  against and no prior green on this platform.
- The repair was verified on Linux only. The build box is Linux; the ten gates pass there and on
  macOS in CI. Windows is a first-class target and CI's winguest leg is the first execution of
  this path on it.

So a red here has two readings with very different consequences: an intermittent in the fixture,
or **the dispatch repair not holding on Windows**, which would mean the original P1 is still live
on one of three supported platforms. Calling it a flake before counting would be exactly the
mistake §12 warns against, and would close a P1 by assumption.

## What the change that takes this up should do

**Count first.** The next push to this branch produces a fresh winguest run. One further green is
not a rate but it distinguishes the two readings' likelihood; a second red settles it as
deterministic and reopens the dispatch defect on Windows.

If it is deterministic, look at the checkout rather than the base selection. `dispatch_head` derives
from the event log and is platform-independent Rust, so the likelier fault is that beta's worktree
does not materialise alpha's file on Windows. This repository has form there: NTFS's delete-pending
window (`PR174` and the `windows-delete-pending-window` measurements), worktree registration races,
and `index.lock` access-denied under a handle the kernel has not released. Reproduce on the
winguest guest before repairing — the seventh round pushed and read CI, which is how this reached
`master`'s door instead of being caught locally.

Do not close this by weakening the assertion to a SHA comparison. The contents assertion is the
finding's whole value.
