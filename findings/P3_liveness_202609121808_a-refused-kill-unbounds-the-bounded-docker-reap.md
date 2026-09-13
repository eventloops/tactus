---
id: REFUSED-KILL-UNBOUNDS-THE-BOUNDED-DOCKER-REAP
severity: P3
disposition: deferred
category: liveness
pr: 277
reviewed_sha: bbde7707ea907e6adb0879fff31545caf3cade2c
location: src/agent/proc.rs:4576
provenance: pre_existing
first_bad: undetermined
guard: deferred: the wait after reap_bounded's SIGKILL must be bounded on the arm where the signal was refused, since that is the arm where the child is not dying; the same bounded-end shape PR125-CLOSE-UNBOUNDED-KILL-AND-WAIT-AT-FIVE-SITES prescribes for the helper sites
---

## Failure sequence

`reap_bounded` exists to put a bound on holding the run's cleanup lease. Its internals note says so
in one sentence -- "Wait for one `docker`, and kill it rather than hold R28 for ever"
(`docs/internals/agent/proc.md`, under the `fn reap_bounded` heading; §13 keeps the prose there and
not in the source). It polls `waitpid(pid, WNOHANG)` for
`REAPER_DOCKER_TICKS` (3000 ticks of 10ms, so about 30 seconds), then sends
`let _ = kill(pid, SIGKILL)` and discards the result, then enters an unconditional blocking
`waitpid(pid, NULL, 0)` loop that returns only when the child is collected -> an LSM or seccomp
policy answers the SIGKILL with EPERM, so the `docker` process is not signalled -> the blocking wait
has nothing to collect and blocks for as long as `docker` runs, which is exactly the unbounded hold
of R28 the bound was introduced to prevent -> the reaper's caller waits behind it, and no record
anywhere says a signal was refused rather than delivered.

The discarded result is what hides the state: on the arm where the kill succeeded the following
blocking wait is correct and brief, and on the arm where it was refused the same wait is unbounded.
The two arms are indistinguishable to the code precisely because the return value is dropped.

The exposure is narrower than the helper sites': the reaper forked this `docker` child itself, so it
is signalling its own child and only a sandbox policy can refuse it; and the hold ends whenever
`docker` exits on its own. That is why this is a P3 and not the P1 the helper-ending sites carried.

## What the change that takes this up should do

Read what the `kill` answered and bound the wait on the arm where it failed: on a refused signal,
poll `waitpid(pid, WNOHANG)` to a short named budget the way
PR125-CLOSE-UNBOUNDED-KILL-AND-WAIT-AT-FIVE-SITES prescribes for the five helper sites, and leave a
child still alive after it for the process's exit to collect rather than blocking on it. A refused
signal should also be distinguishable in whatever the reaper can report from a forked child under
async-signal-safety -- at minimum it must not be recorded as a delivered kill.

Filed by the repair of PR125-CLOSE-DISCARDED-KILL-RESULT (pull request 277), which closed the five
helper-ending sites that finding named and classified the remaining sites rather than leaving them
unstated. This is not one of those five: the child here is a `docker` process the reaper spawned,
not one of the two helpers this process forks and hands a READY handshake.
