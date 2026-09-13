---
id: PR274-DOCKER-TERMINATION-POLL-COUNTS-YIELDS-NOT-TIME
severity: P3
disposition: deferred
category: correctness
pr: 274
reviewed_sha: 12bcbd40b9eae26cb6cec0e9347cfee483d895d9
location: src/runner/container/tests.rs:3226
provenance: pre_existing
first_bad: d94b2ba8a5a8003addfc486c1b6c7615858003f9
guard: the change that next opens the real-Docker tests under `src/runner/container/tests.rs`; `wait_until_terminated` should hold a wall-clock budget, sleep between observations, and say how long it waited when it gives up
---

## Failure sequence

**The real-Docker tests wait for a container to terminate by counting scheduler yields, so their
budget is whatever two hundred `observe` calls happen to take.**

`wait_until_terminated` (`src/runner/container/tests.rs:3218`) observes the container up to 200
times with `std::thread::yield_now()` between observations and panics on the 201st with
"`<name>` is still running after 200 observations" (`:3226`) -> `yield_now` cedes the processor
only when another thread is runnable on it, so the loop's duration is 200 round trips to the
daemon and nothing else; there is no lower bound on it and it is shortest exactly when the box is
least loaded -> a container whose `/bin/sh -c` needs longer to reach a terminated state than the
daemon needs to answer 200 observations — a loaded box, a slow daemon, a slow image start — fails
the test, though the container terminates a moment later and the property the test asserts holds
-> two tests call it and both have failed this way: `real_docker_kill_on_an_already_exited_container_is_tolerated`
(container `upstroke-f1-already-exited`, `:3357`) and
`real_docker_returns_both_streams_of_a_container_separately` (`upstroke-f1-two-streams`, `:3419`).

**Sightings**, all on the build box in full nine-command baselines through `upstroke-eight`, each
passing alone at the same head and on a full re-run of the same tree, none on a change that
touches `src/runner/`:

| when (UTC) | head | test | container |
|---|---|---|---|
| 2026-09-11 00:58 | `e6e5e56c` (#251) | `…already_exited_container_is_tolerated` | `upstroke-f1-already-exited` |
| 2026-09-11 16:23 | `3756fd27` | `…already_exited_container_is_tolerated` | `upstroke-f1-already-exited` |
| 2026-09-12 | `983d453a` (#274) | `…both_streams_of_a_container_separately` | `upstroke-f1-two-streams` |
| 2026-09-12 11:30 | `12bcbd40` (#274) | `…both_streams_of_a_container_separately` | `upstroke-f1-two-streams` |

The first two are in `~/eight-logs/e6e5e56/03-test.log` and
`~/eight-logs/3756fd2-failed-20260911T162343Z/03-test.log` on the box; the last two are recorded
in #274's body, their logs having been overwritten by the green re-runs at the same heads. Both
tests run on CI's `test (ubuntu-latest)` leg (they report `ok` in run `34690703308`), so the shape
is reachable there; no CI sighting is known.

## Why it is P3

The helper is test-only and the property it waits for is true; the defect is the unit of its
budget. Cost so far is four red local baselines in two days, each answered by a re-run.

## What the change that takes this up should do

Give `wait_until_terminated` a wall-clock budget measured from an `Instant`, sleep a few
milliseconds between observations instead of yielding, and put the elapsed time and the
observation count in the panic so a red can be read for what it is. `docs/internals/runner/container.md`
already records that `yield_now` is `SwitchToThread` on Windows and cedes only the current
processor; the same reasoning is why a yield count is not a duration here.
