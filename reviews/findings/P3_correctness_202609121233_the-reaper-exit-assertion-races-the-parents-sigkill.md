---
id: PR274-REAPER-EXIT-ASSERTION-RACES-THE-PARENTS-SIGKILL
severity: P3
disposition: deferred
category: correctness
pr: 274
reviewed_sha: 12bcbd40b9eae26cb6cec0e9347cfee483d895d9
location: src/agent/proc.rs:5868
provenance: pre_existing
first_bad: b0ff0edf6629bd105ccecbbc89dcf7f51a6c765e
guard: the change that next opens `spawn_reaper`'s READY failure path or its tests; the assertion should name the report, which is what the test is about, and not the winner of the race between the child's `_exit` and the parent's `SIGKILL`
---

## Failure sequence

**A test that holds the cleanup lease and asks the reaper to fail asserts which of two racing
paths ended the reaper, and only one of them is the reaper's own exit.**

`a_reaper_refused_its_cleanup_lease_says_which_lease_and_why` takes the cleanup lease exclusively
and calls `spawn_reaper` -> the reaper child's shared `flock` on that lease is refused with
`EAGAIN`, so it writes its setup-failure frame to the acknowledgement pipe and calls `_exit(1)`
(`report_setup_failure_and_exit`, `src/agent/proc.rs:3040`) -> the parent's `await_ready` returns
that frame the moment it lands — `waited 247.533µs of 2s` in the sighting below, which is the time
to the report, not a budget nearly spent — and `spawn_reaper` then calls `reaper.abandon()`, which
sends `SIGKILL` and `waitpid`s the child (`src/agent/proc.rs:2256`) -> what the wait collects is
decided by scheduling and by nothing the parent controls: if the child's `_exit(1)` completed
first, the status is `WIFEXITED` and `describe_helper_end` writes `having already exited with
status 1`; if the `SIGKILL` landed between the child's `write` and its `_exit`, the status is
`WIFSIGNALED` and it writes `killed by signal 9` (`src/agent/proc.rs:2779`) -> the test's second
assertion, `src/agent/proc.rs:5868`, requires the first wording, so the test fails whenever the
parent reads the pipe and kills faster than the child leaves `write` and reaches `_exit`. The
parent kills precisely because it does not know the child has ended, so the race is inherent to
the path and the assertion is about its winner.

The test's first assertion — that the refused lease's path and the errno reach the message — is
what the test exists for, and it held in the sighting: the message read

```
Unix cleanup reaper did not initialize; waited 247.533µs of 2s; descriptor ceiling 65536;
the reaper reported that taking the shared lock on the cleanup lease
/tmp/upstroke-ready-lease-2490-01M2ANGS9R6EFYKMGZ70YYQ93Q/cleanup.lock failed:
Resource temporarily unavailable (os error 11); ending it: SIGKILL was delivered,
and the wait collected it, killed by signal 9
```

**Sighting.** `CI` run `34690703308`, job `103545346405` (`test (ubuntu-latest)`),
2026-09-12T11:21:48Z, on #274's head `12bcbd40b9eae26cb6cec0e9347cfee483d895d9`, whose diff is
Markdown only and reaches no Rust: `test result: FAILED. 2467 passed; 1 failed; 61 ignored`, the
one failure being this test at `src/agent/proc.rs:5868:13` with the message above. Two later runs
of the identical commit, `34691425771` (11:35Z) and `34691831067` (11:44Z), passed it. On the build
box no nine-command baseline log under `~/eight-logs/` records this test failing (measured by
`grep` over every `03-test.log` there at 2026-09-12T12:30Z), so its rate here is one CI red in one
day of runs and it has not been reproduced locally.

The Linux identity path has the same shape: `end_helper_through_identity` sends `SIGKILL` through
the pidfd and then waits, and `describe_helper_end` carries a second pair of wordings for it
(`through the helper's identity`). The sighting used the plain path.

**Provenance.** The test and both of its assertions arrived together in
`b0ff0edf6629bd105ccecbbc89dcf7f51a6c765e` (PR #172, 2026-09-05, "see a Unix helper end before
READY on Darwin, and let it say why"), which added the READY failure message's `ending it:` clause
and the test that pins it. It is not an assertion later added to an older test, and it predates
#265 by six days.

## Why it is P3

The property under test held; the assertion misnames it as the outcome of a race. The cost is a
red required context on an unrelated pull request and a re-run of the same head, once so far.

## What the change that takes this up should do

Make the assertion say what the test means. Either accept both endings when the child had
reported — `having already exited with status 1` or `killed by signal 9` both show the report
reached the message and the child was reaped — or, on a received failure report, have
`spawn_reaper` give a child that has promised to exit a short bounded wait before it kills, so the
status is the child's own. Which of those is right is the module owner's call: the second changes
`spawn_reaper`'s behaviour for every caller, the first changes only the test. Whatever is chosen,
keep the first assertion exactly as it is — the lease path and the errno reaching the message is
the thing `b0ff0edf` was written to establish.
