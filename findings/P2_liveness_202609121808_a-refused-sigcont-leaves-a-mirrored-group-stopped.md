---
id: REFUSED-SIGCONT-LEAVES-A-MIRRORED-GROUP-STOPPED
severity: P2
disposition: deferred
category: liveness
pr: 277
reviewed_sha: bbde7707ea907e6adb0879fff31545caf3cade2c
location: src/agent/proc.rs:2578
provenance: pre_existing
first_bad: undetermined
guard: deferred: the resume half of the stop/resume mirror must either observe that the group continued, as stop_groups observes that it stopped, or keep its retry latch set when the signal was refused
---

## Failure sequence

The stop half of the stop/resume mirror reads what its signal answered and the resume half does
not. In the reaper, `reaper_loop` mirrors a stopped conductor onto the agent's group with
`mirrored_parent_stop = kill(-pgid, SIGSTOP) == 0`, keeping the latch only when the signal landed
-> the conductor resumes, `parent_has_stably_resumed` becomes true, and the loop sends
`let _ = kill(-pgid, SIGCONT)` and clears `mirrored_parent_stop` unconditionally, whatever the
signal answered -> a host or LSM answers EPERM, so the group is never continued, and the latch now
says "not mirrored" while the group is still stopped -> the arm that would retry
(`Some(false) if mirrored_parent_stop`) can no longer fire, no later poll re-sends SIGCONT, and
nothing reports the refusal: the agent's whole group stays stopped for the rest of the run while
the conductor waits on output that cannot come.

The conductor side has the same shape through `signal_groups`, which discards every result for all
seven of its callers. Its SIGSTOP caller `stop_groups` then polls `groups_are_quiescent` to a
two-second deadline and returns a bool, so a refused stop is observed; its SIGKILL caller in
`monitor` restores `SIG_DFL` and re-raises on the next statement, so no action is available to it.
Its five SIGCONT callers in `monitor` (the `stop_groups` failure path, the CONTINUE_REQUESTED
paths, the `stop_parent` failure path and the ordinary end of a suspend) observe nothing and report
nothing, so a refused resume leaves the agent's group stopped there too.

ESRCH is the benign and common answer at these sites -- an empty group is a finished agent -- which
is why discarding the result looks harmless; EPERM is the answer that wedges the run.

## What the change that takes this up should do

Give the resume half the observation the stop half already has, or the retry the latch already
almost expresses: either poll that the group is running after SIGCONT, the way `stop_groups` polls
`groups_are_quiescent` after SIGSTOP, or set `mirrored_parent_stop` from what `kill` answered
(`mirrored_parent_stop = kill(-pgid, SIGCONT) != 0`) so a refused resume is retried on the next
stable-resume poll instead of being latched away. `reaper_loop` runs in a forked child under
async-signal-safety, so its report is a retry and not a message; the `monitor` callers can carry a
refused resume into the failure they already return. Note that ESRCH must stay benign under either
choice: an empty group is not a refusal to retry for ever.

Filed by the repair of PR125-CLOSE-DISCARDED-KILL-RESULT (pull request 277), which closed the five
helper-ending sites that finding named and classified the remaining group-signal sites rather than
leaving them unstated. This is not one of those five: no helper is being ended here.
