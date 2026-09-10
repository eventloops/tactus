---
id: PR125-CLOSE-DISCARDED-KILL-RESULT
severity: P1
disposition: deferred
category: correctness
pr: 125
reviewed_sha: 0bff83dfa632b80a0373202613f37cce222410f9
location: src/agent/proc.rs:2320
provenance: pre_existing
first_bad: 6798089
guard: deferred: the end of a helper reports what kill returned (0, ESRCH, EPERM) and what each waitpid returned, and nothing else; a kill that failed is…
---

## Failure sequence

every one of the five sites writes `let _ = libc::kill(pid, libc::SIGKILL)` -> a sandbox or LSM answers EPERM, or an ESRCH race lands -> the helper is not signalled and may stay in its pre-READY `open` or `close` holding its cleanup lease, while the caller proceeds as if it were dead; a bounded end that then reports "sent SIGKILL" and "left running with the signal pending" invents a history that was not observed, which is what pass 8's second P1 found in the closed pull request

## What the change that takes this up should do

deferred: the end of a helper reports what `kill` returned (0, ESRCH, EPERM) and what each `waitpid` returned, and nothing else; a `kill` that failed is a distinct outcome the READY failure message carries, and §7 forbids discarding the result of a signal the caller depends on

Recorded by PR #125, closed after eight frontier passes; the row is carried out of `reviews/FINDINGS.md` in the words it
was written in.

## Adjacency, recorded 2026-09-10 by the findings-sweep Phase 0 triage

**Not a duplicate of `PR125-CLOSE-PID-IDENTITY-UNDER-A-HOST-WILDCARD-WAITER`, not
schedulable beside it, and — corrected 2026-09-10 after review — not the same
scope as it.**

Both rows carry `location: src/agent/proc.rs:2320`, recorded against `0bff83df`,
and the line has drifted. Re-derive the sites from the failure sequence rather
than trusting `:2320`.

**The failure sequence's "every one of the five sites writes
`let _ = libc::kill(pid, libc::SIGKILL)`" is no longer true of all five.**
Censused at this head, in the production region (the `termination` module's tests
begin at `:4384`), the five calls that signal a helper by PID split three to two:

| site | result | in this row's scope |
|---|---|---|
| `:2694` guard-setup failure | `let _ = …` | yes |
| `:3214` descriptor-configuration failure | `let _ = …` | yes |
| `:4359` `reap_bounded` | `let _ = …` | yes |
| `:2244` `Reaper::abandon` | keeps `kill_errno`, reported via `describe_helper_end` (`:2737`) | **no** |
| `:3245` guard failed-READY path | keeps `kill_errno`, reported in the error string | **no** |

Four further discarded kills target a **process group** (`:2558`, `:2613`,
`:2623`, `:3975`); whether §7's rule reaches those is this row's question, but
PID reuse is not their hazard.

`:2244` and `:3245` are outside this row and squarely inside
`PR125-CLOSE-PID-IDENTITY-UNDER-A-HOST-WILDCARD-WAITER`: checking what `kill`
returned says the call succeeded, not which process it reached. Do not treat a
repair here as covering them.

The three sites above are where the two rows overlap. Whichever is repaired first
moves the other's line numbers, so the two may not share a batch.
