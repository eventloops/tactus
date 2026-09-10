---
id: PR262-UNREACHABLE-PATH-ENTRY-ABORTS-PROGRAM-RESOLUTION
severity: P2
disposition: deferred
category: portability
pr: 262
reviewed_sha: 5df9f7c0defd464cae495ba91b82d8ed5ca1f17c
location: src/runner/host/naming.rs:220, src/runner/host/tests.rs:5550
provenance: pre_existing
first_bad:
guard: project owner — the slice that next opens the host runner's program resolution
---

## Failure sequence

`runner::host::tests::every_path_entry_this_runner_searches_names_a_location_on_its_own` failed on the Windows guest at `src\runner\host\tests.rs:5550`:

```text
`\\server\share\bin` names a location and was not searched: failed to stat
\\server\share\bin\upstroke-no-such-program.com: The specified network name is
no longer available. (os error 64)
```

**The defect the message exposes is not in the test.** `resolve_program` (`src/runner/host/naming.rs:195`) walks the composed `PATH`, counting the absolute entries it searches (`:214`). For each candidate it calls `is_program`, which maps **only** `ErrorKind::NotFound` to "absent" (`:76`) and propagates every other `io::Error` (`:77`). `resolve_program` then **returns immediately** on that error as `UpstrokeError::Filesystem { operation: "stat", … }` (`:220-226`), abandoning the rest of the walk.

So one unreachable `PATH` entry aborts resolution of the whole `PATH`: **a program that is installed on a later entry is not found, and the failure is reported as a stat error rather than as an absence.** On this run the entry was a UNC path whose share does not exist, and Windows answered `os error 64` — "the specified network name is no longer available" — which is not `NotFound`. A stale mapped drive, an unreachable share, or a directory the process cannot read (`EACCES` on any platform) reaches the same early return; nothing about this is Windows-only, and Windows is only where an unreachable-by-construction `PATH` entry is routine.

**Why it is intermittent, and it is intermittent by this project's own standard.** Whether the fixture's `\\server\share\bin` produces `Ok(false)` or an `Err` depends on which error the guest's SMB client returns for a host that does not resolve. The error that means "no such share" maps to `NotFound` and the walk completes, giving the `1 directory searched,` message the test asserts; `os error 64` does not, and the walk aborts. The identical source produced both colours:

```text
e8fcbdd8  test (winguest)  success   2026-09-10T20:57:05Z
5df9f7c0  test (winguest)  failure   2026-09-10T21:47:57Z   2362 passed, 1 failed, 41 ignored
0d7a3b22  test (winguest)  success   2026-09-10T21:59:31Z   the head that carries this row
```

The `src/` tree object is `cfd11655dc31` at all three: **one source, both colours, twice green and
once red inside ninety minutes.** The green run at `0d7a3b22` is **not a retirement** — it is the
second observation of the colour that does not fail, and the row exists because the third observation
disagreed with it. Runs after `0d7a3b22` are not tracked here; a later green adds nothing this table
does not already contain, and a later red should be recorded against this id rather than filed again.

**Not caused by the change in front of it.** PR #262's diff is confined to `reviews/findings/`; `git diff origin/master...HEAD -- src/ Cargo.toml Cargo.lock` is empty, so the `src/` tree the table above pins is `master`'s. Linux and macOS were green at the red head.

**The ledger carried no row with this fingerprint**: searching `reviews/findings/` for the test name, for `os error 64` and for the network-name wording returned nothing.

## What the change that takes this up should do

Owner, as the ledger records it: project owner — the slice that next opens the host runner's program resolution.

**Decide what an unsearchable `PATH` entry means, because the two readings repair different files:**

1. **The walk should continue.** An entry that cannot be searched is not an answer about the program; it is an entry that contributed nothing. `resolve_program` would count it — searched, skipped, or a third bucket of *unsearchable* — and keep walking, so a program present on a later entry is still found. The refusal message already distinguishes searched from skipped and would name the third case the same way. This is the reading that makes the runner's behaviour independent of ambient network state, and it is the one this row favours; it does not decide it.
2. **The abort is deliberate and the message is the defect.** If a stat failure must stop resolution — because a runner that silently tolerates an unreadable `PATH` entry can pick up the wrong program later — then the error should say which entry could not be searched and how many remained unwalked, and the test should assert that shape rather than `1 directory searched,`.

Whichever is chosen, **the fixture is a second, separate problem**: `\\server\share\bin` is written to be absolute-but-absent, and its reachability is ambient. A UNC path under a hostname reserved for non-resolution, or a synthesised unreachable entry whose error kind the test asserts up front — the shape `undeterminable_directory` already uses at `src/runner/host/naming.rs:275`, where an interior NUL guarantees a non-`NotFound` failure — would make the case deterministic instead of dependent on what the guest's SMB stack answers this hour.

**A guard the repair should carry:** a `PATH` whose first entry cannot be searched and whose second entry holds the program, asserting the program is found. That witness fails against the current code on every platform, so it does not need the Windows guest to be meaningful — which is the point, since the guest is where this was seen and the least convenient place to reproduce it. The existing test should keep a case that pins the message for an entry that is absolute and genuinely absent.

Recorded 2026-09-10 on `docs/findings-triage-locations`, from a red `test (winguest)` leg on this pull request's own head. **P2** is this pass's judgement: the consequence is a host runner that fails to find an installed program, and `portability` is the lane because the repair's witness wants the matrix's `executed-on-platform` lens even though the early return is platform-independent. **Filed as an independent observation and not a member of `CLASS-INTERMITTENT-SUBPROCESS-KILL-SETTLE-RESIDUE-FAILURES`**: that class's four members are kill, settle and residue failures in subprocess and workspace handling, and this is program resolution reading an ambient network error; whether it belongs to any class is the class owner's scope decision. `MAINTAINING.md` step 5 is why it is a file: every open finding gets one, including one unrelated to the change that found it.
