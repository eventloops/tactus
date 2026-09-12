---
id: PREDICTABLE-SCRATCH-ROOTS-BEYOND-VALIDATE
severity: P2
disposition: deferred
category: crash-consistency
pr: 278
reviewed_sha: 71d44285071993439b87da02046b32b307c49ad0
location: src/runner/host/tests.rs
provenance: pre_existing
first_bad: undetermined
guard: whichever slice next opens shared test infrastructure; the counts below say which file buys the most per visit
---

## Failure sequence

`PR104-VALIDATE-SCRATCH-DIRECTORIES-PREDICTABLE-AND-UNRECLAIMED` was repaired in
`src/validate.rs` only. The shape it named is not confined to that file, and this row records how
much of it is left, measured rather than estimated, so the class is a number instead of an
impression.

Two patterns, each stated as the command that produced it, both run over `git ls-files src`
(`*.rs` only) with comment-only lines skipped.

**A. A temporary root nobody can be sure they own.** A `std::env::temp_dir()` path whose naming
statement — the source line plus continuation lines up to the terminating `;` — names
`std::process::id()` and does not name `ulid`.

| | `origin/master` at `71d44285` | after the `src/validate.rs` repair |
|---|---|---|
| A: pid-derived temp paths, no ULID | **66** sites in **30** files | **55** sites in **30** files |
| A2: of those, created as a directory within six lines — a scratch *root* | **57** sites in **25** files | **46** sites in **25** files |

**Positive control for the pattern**, because a count nobody has checked against a known-present
case is not evidence: run against `origin/master`, A reports **12** for `src/validate.rs`, which is
exactly the number `PR104` recorded there.

Run against the repaired tree it still reports **1** for that file, and that one is a **false
positive worth stating**, because it is the clearest illustration of what these patterns can and
cannot see: it is the regression test's stand-in for another process's directory, and its tag
carries a fresh ULID assigned one statement earlier, which a line-and-continuation pattern cannot
read. The path is unique and guarded. So the honest residue is **45 roots across 24 files**, not 46.

A pid is not a unique key. It repeats across containers and after wraparound, which is why
`PR7-SCRATCH-FIXTURE-LEAK` records a Windows suite failing on a "fresh" fixture that was not fresh,
and why `PR245-WINGUEST-IMAGE-SHIPS-FIXTURE-RESIDUE` measured 25,187 `upstroke-*` roots already in
the frozen Windows image.

**B. A pre-clean whose result is thrown away.** A line matching
`let _ = <path>remove_dir_all(`.

| | `origin/master` at `71d44285` | after the repair |
|---|---|---|
| B: discarded `remove_dir_all` results | **142** sites in **25** files | **141** sites in **24** files |

`src/runner/host/tests.rs` alone holds **47** of them — a third of the tree's total, and the single
biggest concentration by a factor of two. It has **zero** pattern-A sites: it already names its
roots with a ULID, so what is left there is the discarded result rather than the predictability.
That split matters, because `PR64-CLEANUP-003-SCRATCH-PRECLEAN` is explicit that *both* halves are
required and that a repair keeping either one keeps the sequence.

Of the B sites, **28** are the pre-clean-then-create shape specifically — a discarded
`remove_dir_all(&x)` followed within four lines by `create_dir` or `create_dir_all` of the same
binding — and **24** of those 28 build the path from a tag and the pid alone. Those 24 are the
exact sequence `PR64` describes, unrepaired.

**What these patterns do not catch**, stated so the numbers are not read as a closed set: a
pre-clean more than four lines from its create, or one whose create names a different binding; a
helper that pre-cleans a path its caller created; `remove_dir`, `remove_file` or a `.ok()`-suffixed
discard rather than `let _ =`; and a predictable name built from something other than
`std::process::id()` — a thread name and a nanosecond clock, for instance, which
`src/agent/proc/tests.rs` uses in two places and which pattern A counts as pid-derived while
pattern A's ULID exclusion does not clear.

## What the change that takes this up should do

**Not one pull request.** A tree-wide sweep cannot merge under the standing delegation and should
not be attempted as one change: `src/workspace.rs` is `[[legacy]]` in `effects/allowlist.toml` with
`invariants_preserved[1]` requiring its behaviour untouched, and `src/effects/tests.rs` is a
CI-contract test — an instrument. Both hold sites in the counts above.

**And every one of these files is more expensive than it looks**, for a reason the `src/validate.rs`
repair found by walking into it: `effects/allowlist.toml` describes several of these test modules in
present tense, naming the scratch shape and counting the denied calls per method. The row for
`src/rundir/tests.rs` says its `scratch` "is `std::env::temp_dir()` joined with a tag and the
process id, and … opens with `let _ = fs::remove_dir_all(&dir)` on that predictable path", and lists
`fs::create_dir_all` 38, `fs::remove_dir_all` 3, `fs::create_dir` 1 among 87 sites. Repairing that
helper — three lines — falsifies both, so the repair reaches an effect allowlist and stops being a
delegated merge. Whoever takes a file up should budget for the governance edit beside the code one,
and should expect the same to be true of `src/workspace_manager/fixture.rs`, whose row describes its
fixture naming the same way.

The mechanism itself is settled and needs no design: `crate::rundir::scratch_tree::acquire` names a
root with a fresh ULID, takes it with one exclusive `create_dir` so an occupied name is refused
rather than adopted or deleted, and returns a guard that reclaims the tree on drop and on unwind.
`src/connect.rs`, `src/workspace_manager/tests.rs`, `src/rundir/tests.rs` and now `src/validate.rs`
already call it. The work is per-file conversion, not invention.

Suggested order, worst first by what each visit buys: `src/runner/host/tests.rs` (47 B),
`src/agent/proc/tests.rs` (20 B, 5 A), `src/runner/container/census/tests.rs` and
`src/runner/container/tests.rs` (10 B each), `src/review.rs` (6 A), `src/util.rs` (5 A, 7 B),
`src/rundir/tests.rs` (1 A, 1 B — the smallest diff in the tree and the one that proves the
allowlist cost).

**Severity.** `P2` is this row's own judgement, not a reviewer's word. The harm for an individual
site is already carried at `P1` by `PR64-CLEANUP-003-SCRATCH-PRECLEAN`, and the leak half by
`PR7-SCRATCH-FIXTURE-LEAK`; what this row adds is the extent, which is what a planner needs and
neither of those carries. No new sighting is recorded here — `PR245`'s Windows measurement is the
nearest thing to one, and it is that row's.
