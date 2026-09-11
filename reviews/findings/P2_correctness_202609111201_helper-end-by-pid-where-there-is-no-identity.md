---
id: HELPER-END-BY-PID-WHERE-THERE-IS-NO-IDENTITY
severity: P2
disposition: deferred
category: correctness
pr: TBD
reviewed_sha: d8fc3fc39cebc4f6893e3c720029101389d1c5a1
location: src/agent/proc.rs:2806
provenance: pre_existing
first_bad: PR125-CLOSE-PID-IDENTITY-UNDER-A-HOST-WILDCARD-WAITER — the half of it a Linux identity does not reach
guard: filed by the branch `fix-P1/correctness_pid-identity-under-a-host-wildcard-waiter`, which took the narrower of the two readings the closed finding offered and wrote it into DESIGN §15; ratifying that reading, or replacing it with the stricter one, is the owner's call
---

## Failure sequence

On macOS, on any supported Unix whose kernel has no `pidfd_open`, and on any host whose syscall policy will not let this process use one of the three calls an identity is used through -- `pidfd_open`, `waitid(P_PIDFD, ...)` or `pidfd_send_signal`, whether the policy refuses the call, ends the process that makes it, or answers it with the errno the kernel would have used about a helper -- `open_helper_identity` answers `-1` and the end of a private helper is `kill(pid, SIGKILL)` and `waitpid(pid, ..., 0)` on the number -> an embedding host whose `SIGCHLD` handler reaps this process's children with a wildcard wait collects a helper that died before READY, and the kernel re-issues its number to another of that host's forks -> the `SIGKILL` kills that process and the `waitpid` collects it, taking its exit status from the host and blocking the launch for as long as it runs. This is the closed row's sequence unchanged; what changed is that it is now confined to the platforms with no identity, and that DESIGN §15 states it there rather than leaving it implied.

## What the change that takes this up should do

Two things, in the owner's order. First, ratify or replace the reading DESIGN §15 now carries: a helper's end is best effort where there is no identity, and upstroke asks no obligation of an embedding host. The closed row offered the other reading — that a host may not reap this process's children with wildcard waits while a helper is starting or ending — which is a stronger guarantee bought by a constraint on every embedder, and which this change did not take because it binds users who are not in the room. Second, if the stricter reading is not taken, evaluate on the platform whether Darwin has a name for a process that a reused number cannot impersonate; `kqueue`'s `EVFILT_PROC` registration is the candidate to measure, and it would cover the wait and not the signal, so it settles half of the sequence at best. Neither half can be established from a Linux host: §11 requires the platform's own leg.
