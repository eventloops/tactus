---
id: PR265-R6-CLONE3-STRANDS-A-HOST-LOCK
severity: P2
disposition: deferred
category: liveness
pr: 265
reviewed_sha: c683b1cd17895e2e46954f1ae7068079928beff4
location: src/agent/proc.rs:2867
provenance: introduced_by_feature
first_bad: c683b1cd17895e2e46954f1ae7068079928beff4
guard: the next change to the identity path's helper creation
---

## Failure sequence

Executed by **both** round-6 review lenses, independently, at `c683b1cd17895e2e46954f1ae7068079928beff4`.

`clone3` does not run `pthread_atfork` handlers, so the helper copies the host's lock state as it
stood at creation. The `SAFETY` comment at `src/agent/proc.rs:2867` says that is harmless because
the child stays in syscall-only code. **It does not.** The guard calls `libc::fork()` at
`src/agent/proc.rs:3371`, and the reaper does the same in `spawn_group_anchor`
(`src/agent/proc.rs:2597`). Those calls run the inherited prepare handler.

1. An embedder registers ordinary `pthread_atfork` handlers — prepare takes a mutex, parent and
   child release it.
2. Another host thread holds that mutex while a helper is created.
3. `clone3` copies the locked mutex; the thread that would release it does not exist in the child.
4. The helper's own `fork` enters the prepare handler and waits on a lock nothing can release.

Measured with production code unchanged, all three identity syscalls permitted:

| fixture | identity off | identity on |
|---|---|---|
| `launched_helper_identity_helper` | `0` | `101` — guard missed READY, killed after 2s |
| reaper registration (base `749966f`: `0`) | — | `-9` after 5s, reaper blocked in `futex_wait_queue` |
| both controls without the lock | `0` | `0` |

## What the change that takes this up should do

Make the post-clone path hold what the `SAFETY` comment claims, or correct the comment and the body
to say what is actually true. The two `libc::fork()` sites the helper reaches after a `clone3`
creation are the whole of it: either they must not run inherited handlers, or the helper must not be
created by `clone3` while the embedder can hold a lock across it. Regression coverage is needed
through **both** guard startup and reaper registration — the existing 37 termination tests all pass
and miss this.

**Not merge-blocking, recorded here for why:** it needs the opt-in
`UPSTROKE_HELPER_IDENTITY=1` — off by default — *and* an embedder that holds a lock across helper
creation from `pthread_atfork` handlers. With the path off, every fixture above exits `0`. It is
reachable by neither limb of the owner's 2026-09-11 merge rule: not normal use, and not triggerable
by anyone without push access.
