# Changelog

## Unreleased

- Ending a Unix job-control guard the launch gives up on (its descriptors could not be configured,
  or the signal monitor could not start after it said READY) now reports what `kill` and `waitpid`
  answered, in the words a helper that missed READY already used, instead of discarding both
  answers. A refused signal is a distinct outcome the message names, and a helper's ending left as
  an unused expression statement is a build error. Each of the two paths keeps the `waitpid` it
  always made — the descriptor path asks for no exit status, and says so rather than reporting one
  it did not collect — so a host policy that refuses or kills a wait carrying a status pointer sees
  no new call (`PR125-CLOSE-DISCARDED-KILL-RESULT`).
- A schema-4 integration verification that ends unavailable — parked for a person, or deferred by
  an infrastructure outage — now records the review passes it paid for, so a resumed run's reported
  spend is the total the incarnation before it reached. `merge_verification_unavailable` gains a
  required `reviews` field; a schema-4 log written without it is refused rather than replayed to a
  total it cannot account for. Schema 4 is unreleased and inert by default, so no released run is
  affected (`DESIGN.md` §26, "The unavailable terminal's spend").
- A Unix private helper (the cleanup reaper, the job-control guard) can be ended through a name
  the kernel cannot re-issue instead of through its pid. **Opt-in, Linux 5.4+, off by default:**
  with `UPSTROKE_HELPER_IDENTITY=1` each helper is created by `clone3` with `CLONE_PIDFD`,
  signalled with `pidfd_send_signal` and collected with `waitid(P_PIDFD, ...)`, and no other
  call is made. Setting it asserts that the host's syscall policy permits those three calls.
  With it unset, upstroke makes none of them and ending a helper stays the `kill` and the
  `waitpid` it has always been, which `DESIGN.md` §15 now states is best effort against an
  embedding host that reaps this process's children with wildcard waits.
- Relicensed to Apache-2.0 with a NOTICE file; earlier releases keep the terms recorded in their
  own tagged metadata and source notices (decided 2026-09-01).
- The G2 checkpoint: the v0.2 parallel-execution machinery (worktree-per-task isolation, the
  compare-and-swap merge queue, the optional container runner, the topology layer) merged to
  master inert by default. The v0.1 sequential path is unchanged and schema-4 state engages only
  by explicit schema choice; no `0.2.0` tag (G2 checkpoint promotion, decided 2026-08-31).
- Retired the App-signed `upstroke-frontier-review` attestation gate: its two privileged workflows,
  four scripts and fixture tests, and the signing environment are gone, and the default-branch
  ruleset requires `upstroke-ci` and `upstroke-pr-policy` only. The review obligation is unchanged;
  the owner's merge is the attestation (decided 2026-08-23).
- Renamed project from `tactus` to `upstroke`. Binary, crate, env-var prefix (`UPSTROKE_*`),
  user directory (`~/.upstroke`) and run directory (`.upstroke/`) all change; no aliases. The
  transformation is `scripts/rename-tactus-to-upstroke.sh`.
- `upstroke export-decisions <run-id>`: a local, read-only JSONL/CSV projection of a finished run's
  plan and attempt log (`DESIGN.md` §25).

## 0.1.0 — 2026-08-10

- The sequential conductor, end to end: plan ingestion, routing, the Claude Code and Copilot
  adapters, the engine with git ownership, gates, cross-family review, the verification ladder,
  the event log with resume and status, and the read-only capacity engine. The acceptance
  write-up, kept in the repository history, is the evidence.
