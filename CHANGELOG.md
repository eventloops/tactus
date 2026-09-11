# Changelog

## Unreleased

- A Unix private helper (the cleanup reaper, the job-control guard) can now be ended through a
  name the kernel cannot re-issue — on Linux a pid file descriptor taken at the fork — instead of
  through its pid. **This is opt-in and off by default**: set `UPSTROKE_HELPER_IDENTITY=1` to turn
  it on, which asserts that the host's syscall policy permits `pidfd_open`, `pidfd_send_signal`
  and `waitid(P_PIDFD, ...)`, since a policy may end the process that makes one of them rather
  than refuse it. With it off, upstroke makes none of those calls and ending a helper stays the
  `kill` and the `waitpid` it has always been — best effort against an embedding host that reaps
  this process's children, which `DESIGN.md` §15 now states rather than leaving implied. With it
  on, a call a policy refuses is answered where that call is made, and the two errnos that would
  mean a helper has ended are acted on only where a second call through the same descriptor agrees.
  Where the launch could not take the descriptor — one the process had run out of — the launch
  fails instead of falling back to the pid, and waits for the helper it could not name within the
  same budget it gives a helper to start.
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
