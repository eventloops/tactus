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

**Read what the code does before reading this as a defect.** `resolve_program` (`src/runner/host/naming.rs:195`) walks the composed `PATH`, counting the absolute entries it searches (`:214`). For each candidate it calls `is_program`, which maps **only** `ErrorKind::NotFound` to "absent" (`:76`) and propagates every other `io::Error` (`:77`); `resolve_program` then returns on that error as `UpstrokeError::Filesystem { operation: "stat", … }` (`:220-226`) without walking the remaining entries.

**That stop is a deliberate contract, and it is witnessed.** `an_undetermined_candidate_stops_the_search_before_a_later_match` (`src/runner/host/naming.rs:372`) puts an undeterminable directory ahead of the directory that really holds the program and asserts that resolution **fails**, that the path reported is the undetermined candidate and **not** the later match, and that the carried source is the platform's own error rather than `NotFound`. Its counterpart `a_directory_that_is_merely_absent_is_walked_past_to_a_later_match` (`:408`) draws the other half of the line: mere absence is walked past, an unanswerable candidate is not. So "the walk should have continued" is **not** a repair this row asks for — it would reverse a tested decision, and this row does not propose it.

**What is left, and it is what this row is for.** The fixture's `\\server\share\bin` is written to be absolute-but-absent, and **which side of that contract it lands on is ambient**: an error meaning "no such share" maps to `NotFound`, the walk completes, and the message carries the `1 directory searched,` the test asserts; `os error 64` — "the specified network name is no longer available" — does not map to `NotFound`, so the deliberate stop fires instead and the assertion at `:5550` fails. Nothing in the source decides which happens. The guest's SMB stack does.

The consequence is not a program that cannot be found. It is **a required context that can go red on byte-identical source**, which is the same colour a real regression shows. The identical source produced both colours:

```text
e8fcbdd8  test (winguest)  success   2026-09-10T20:57:05Z
5df9f7c0  test (winguest)  failure   2026-09-10T21:47:57Z   2362 passed, 1 failed, 41 ignored
0d7a3b22  test (winguest)  success   2026-09-10T21:59:31Z   the head that carries this row
```

The `src/` tree object is `cfd11655dc31` at all three: **one source, twice green and once red inside ninety minutes.** The green run at `0d7a3b22` is **not a retirement** — it is a second observation of the colour that does not fail, and the row exists because a third disagreed with it. Runs after `0d7a3b22` are not tracked here; a later green adds nothing this table does not already contain, and a later red belongs against this id rather than filed again.

**Not caused by the change in front of it.** PR #262's diff is confined to `findings/`; `git diff origin/master...HEAD -- src/ Cargo.toml Cargo.lock` is empty, so the `src/` tree the table above pins is `master`'s. Linux and macOS were green at the red head — on Linux `\\server\share\bin` is not absolute, so the fixture takes the skipped branch and never reaches a `stat` at all.

**The ledger carried no row with this fingerprint**: searching `findings/` for the test name, for `os error 64` and for the network-name wording returned nothing.

## What the change that takes this up should do

Owner, as the ledger records it: project owner — the slice that next opens the host runner's program resolution.

**The actionable half is the fixture, and it is the only half this row asks for.** `\\server\share\bin` makes a required context depend on what a network stack answers this hour. Two shapes remove that without touching the search policy:

1. A `PATH` entry that is absolute on Windows and **cannot** be a reachable share whatever the environment does, so the entry is absolute-and-absent by construction rather than by luck.
2. Or split the case: keep an entry whose error kind the fixture asserts up front — the shape `undeterminable_directory` (`src/runner/host/naming.rs:275`) already uses, where an interior NUL guarantees a non-`NotFound` failure — and let the `1 directory searched,` assertion apply only to an entry that is genuinely absent.

Either keeps the test's subject, which is that an entry naming a location is searched rather than skipped, and stops the guest's SMB behaviour from deciding the result.

**Any change to the search policy itself is an explicit decision, not a repair, and this row does not make it.** Walking past an unanswerable candidate would overturn `an_undetermined_candidate_stops_the_search_before_a_later_match` (`:372`) and the reasoning under it: a runner that walks past a directory it could not read can resolve to a *different* program than the one that entry would have provided, which is a silent wrong-binary risk in place of a loud refusal. Whoever wants that trade changed has to say so and change that test deliberately; the decision sits with `DESIGN.md` §6, where the runner's obligation to resolve against the environment it composes is stated, and not with a triage pass. **This row records the interaction; it does not ask for the policy to move.** Anyone taking the fixture repair should leave `:372` and `:408` exactly as they are.

Recorded 2026-09-10 on `docs/findings-triage-locations`, from a red `test (winguest)` leg on this pull request's own head. **P2** is this pass's judgement of the consequence above — a required gate leg that can be red on unchanged source — and not of a resolution defect, because the resolution behaviour is the contract at `:372`. `portability` is the lane because the case arises only where a UNC entry is absolute, and the fixture repair wants the matrix's `executed-on-platform` lens. **Filed as an independent observation and not a member of `CLASS-INTERMITTENT-SUBPROCESS-KILL-SETTLE-RESIDUE-FAILURES`**: that class's four members are kill, settle and residue failures in subprocess and workspace handling, and this is a fixture reading an ambient network error; whether it belongs to any class is the class owner's scope decision. `MAINTAINING.md` step 5 is why it is a file: every open finding gets one, including one unrelated to the change that found it.
