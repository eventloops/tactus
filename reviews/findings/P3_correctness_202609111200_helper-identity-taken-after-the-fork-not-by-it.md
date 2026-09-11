---
id: HELPER-IDENTITY-TAKEN-AFTER-THE-FORK-NOT-BY-IT
severity: P3
disposition: deferred
category: correctness
pr: TBD
reviewed_sha: d8fc3fc39cebc4f6893e3c720029101389d1c5a1
location: src/agent/proc.rs:2383
provenance: pre_existing
first_bad: PR125-CLOSE-PID-IDENTITY-UNDER-A-HOST-WILDCARD-WAITER — the residue that closing it on Linux leaves
guard: filed by the branch `fix-P1/correctness_pid-identity-under-a-host-wildcard-waiter`, which took the identity in the parent because `libc::fork` cannot hand one back; the race-free form replaces the fork primitive and is the owner's call
---

## Failure sequence

`spawn_reaper` and `spawn_guard` take the helper's identity with `open_helper_identity(pid)` as the next parent statement after `fork` returns, so the descriptor is opened a moment after the child exists rather than by the fork itself -> inside that window the child can exit, an embedding host's `SIGCHLD` handler can collect it, and the kernel can re-issue its number -> `pidfd_open` then names the process now holding the number, and the signal and the wait that end the helper reach that process exactly as they did before the repair. The window is narrow and not closed: re-issuing a number needs the pid allocator to wrap its whole space (`/proc/sys/kernel/pid_max` is 4194304 on the build box), but the parent thread can be descheduled inside it, so the window is wall-clock and not a count of instructions.

## What the change that takes this up should do

Take the descriptor from the fork itself. `clone3` with `CLONE_PIDFD` returns the pid and the descriptor together, so no window exists in which the number can be re-issued before the descriptor names the child. That replaces `libc::fork` at both private-helper sites with a raw syscall, which does not run the `pthread_atfork` handlers glibc's `fork` runs and does not reset the allocator state `fork` resets, so what the forked child may do changes with it — the helpers are syscall-only after the fork today, but the constraint would become load-bearing rather than incidental. It is also Linux-only, so it narrows nothing on the platforms row `HELPER-END-BY-PID-WHERE-THERE-IS-NO-IDENTITY` covers. Whether that trade is worth taking is the owner's, not a repair round's.
