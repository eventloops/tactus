---
id: HELPER-END-BY-NUMBER-WHERE-THE-IDENTITY-PATH-IS-OFF
severity: P2
disposition: deferred
category: correctness
pr: 265
reviewed_sha: 749966fbf761cc2a9d95fd5818292e7e4b4702ba
location: src/agent/proc.rs:2240
provenance: pre_existing
first_bad: PR125-CLOSE-PID-IDENTITY-UNDER-A-HOST-WILDCARD-WAITER — the half of it the opt-in identity path does not reach
guard: filed by the branch `fix-P1/correctness_pid-identity-under-a-host-wildcard-waiter`, which closed the sequence on Linux behind `UPSTROKE_HELPER_IDENTITY=1` and wrote the default into DESIGN §15 as best effort; whether the default should change, and whether Darwin has a name a reused number cannot impersonate, is the owner's call
---

## Failure sequence

With `UPSTROKE_HELPER_IDENTITY` unset, which is the default, and on macOS and every Unix target whatever the variable says, `Reaper::abandon`, `Reaper::close_and_wait_reporting`, `Guard::abort_setup` and the two `spawn_guard` failure arms end a private helper with `kill(pid, SIGKILL)` and `waitpid(pid, ...)` on its number -> an embedding host whose `SIGCHLD` handler reaps this process's children with a wildcard wait collects a helper that died before READY, and the kernel re-issues its number to another of that host's forks -> the `SIGKILL` kills that process and the `waitpid` collects it, taking its exit status from the host and blocking the launch for as long as it runs. This is the closed row's sequence unchanged; what changed is that DESIGN §15 states it as best effort for the default and for the platforms with no identity, and that a Linux embedder can turn it off by asserting its host permits `clone3`, `pidfd_send_signal` and `waitid(P_PIDFD, ...)`.

## What the change that takes this up should do

Two decisions, the owner's. First, whether the Linux default should become the identity path once the syscall-policy question that kept it opt-in has an answer the design can state — a host under a killing policy would then die on the first launch, which is why this pull request did not make it the default. Second, whether Darwin has a name for a process that a reused number cannot impersonate; `kqueue`'s `EVFILT_PROC` registration is the candidate, it would cover the wait and not the signal, and §11 requires the platform's own leg to establish it. Neither is a repair round's.
