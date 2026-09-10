---
id: PR125-CLOSE-PID-IDENTITY-UNDER-A-HOST-WILDCARD-WAITER
severity: P1
disposition: deferred
category: correctness
pr: 125
reviewed_sha: 0bff83dfa632b80a0373202613f37cce222410f9
location: src/agent/proc.rs:2320
provenance: pre_existing
first_bad: 6798089
guard: deferred: this is a design decision for the owner, not a repair round; the durable fix on Linux is pidfd_open at fork time, signalling through…
---

## Failure sequence

a helper is signalled by pid at every site above, and any diagnostic read of it (state, open descriptors) would be by pid too -> the library anticipates an embedding host whose SIGCHLD handler reaps with wildcard waits (the comment at src/agent/proc.rs:2487 on master; `install_reaper_dispositions` scrubs that handler from the reaper for exactly this reason) -> the helper dies before READY, the host's handler reaps it, the kernel reuses the number for another host fork, and the signal or the snapshot lands on that process; a `waitpid(pid, WNOHANG)` answering zero proves only that the number names some unreaped child of this process, not the helper, and no source census can see host code

## What the change that takes this up should do

deferred: this is a design decision for the owner, not a repair round; the durable fix on Linux is `pidfd_open` at fork time, signalling through `pidfd_send_signal` and waiting through `poll` on the descriptor, which cannot name a reused pid and gives the bounded wait above for free; on Darwin and the other hosts there is no equivalent, so the design must state a trust boundary, either that an embedding host may not reap this process's children with wildcard waits while a helper is being started or ended, or that a helper's end is best-effort there and says so; the closed pull request's ownership proof inventoried only this crate, which the pass-8 verdict shows is not enough

Recorded by PR #125, closed after eight frontier passes; the row is carried out of `reviews/FINDINGS.md` in the words it
was written in.

## Adjacency, recorded 2026-09-10 by the findings-sweep Phase 0 triage

**Not a duplicate of `PR125-CLOSE-DISCARDED-KILL-RESULT`, not schedulable beside
it, and — corrected 2026-09-10 after review — not the same scope as it.**

Both rows carry `location: src/agent/proc.rs:2320`, recorded against `0bff83df`,
and the line has drifted: it now lands in the reaper's spawn setup, with
`install_reaper_dispositions` at `:2411`. Re-derive the sites from the failure
sequence rather than trusting `:2320`.

**This row's scope is every helper kill that names a PID, whether or not the
result is checked.** Censused at this head, in the production region (the
`termination` module's tests begin at `:4384`), five calls signal a helper by
PID rather than by process group:

| site | result |
|---|---|
| `:2244` `Reaper::abandon` | **kept** — `kill_errno`, reported through `describe_helper_end` (`:2737`) |
| `:2694` guard-setup failure | discarded |
| `:3214` descriptor-configuration failure | discarded |
| `:3245` guard failed-READY path | **kept** — `kill_errno`, reported in the error string |
| `:4359` `reap_bounded` | discarded |

**`:2244` and `:3245` are this row's and not the other's.** They already preserve
what `kill` returned, so `PR125-CLOSE-DISCARDED-KILL-RESULT` has nothing left to
repair at either — and both still signal by PID, so a host that reaps with
wildcard waits can collect the helper, the kernel can reuse the number, and a
call whose return value is *checked* can kill the replacement and report success.
A repair scoped to the discarded-result sites omits exactly these two.

The three discarded PID kills (`:2694`, `:3214`, `:4359`) are where the two rows
overlap. Whichever is repaired first moves the other's line numbers, so the two
may not share a batch.
