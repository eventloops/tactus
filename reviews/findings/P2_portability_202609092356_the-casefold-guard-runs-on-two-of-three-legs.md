---
id: PR258-CASEFOLD-GUARD-PLATFORM-SHAPED
severity: P2
disposition: accepted-risk
category: portability
pr: 258
reviewed_sha: 2b048a672a51ff681d19ec7bcc369c07e252b6cd
location: src/workspace_manager/tests.rs:10930
provenance: fix_regression
first_bad: PR258-CASEFOLD-FANOUT-INVISIBLE
guard: `the_temporary_object_scan_resolves_case_aliases_as_the_filesystem_does` requires its native branch where `UPSTROKE_TEST_TEMP_FOLDS_CASE=1`, which `ci.yml` sets on the `macos-latest` test step and on the `winguest` step, so a green on those two legs records that the guard ran; the workflow oracle pins that declaration (`TEST_STEP_ENV`, `TEST_WINDOWS_STEP_ENV`); `a_native_case_insensitive_fan_out_alias_is_detected` is `#[ignore]`d, never runs in CI, and was executed on ext4 casefold by hand (`09-r3-targeted.log`, `13-r3-mutations-M9-native-casefold.log`)
---

## Failure sequence

The round-2 P1, `PR258-CASEFOLD-FANOUT-INVISIBLE`, was a `read_dir` name filter that
discarded a stored upper-case fan-out before resolving Git's lower-case path, so a real
killed write on a case-insensitive volume was invisible. Its repair is canonical-path
resolution. The evidence and regression lenses of round 4 both measured the guard on that
repair (`~/opus-review-258-evidence.md` B3, `~/opus-review-258-regression.md` R3):

    the exact `1d7f3cc9` filter restored into `2b048a67`, on ordinary case-sensitive /tmp
    -> `the_temporary_object_scan_resolves_case_aliases_as_the_filesystem_does` reads
      `native_alias == false` (its oracle is `fs::metadata` on the lower-case name, the same
      primitive the scan uses) and asserts `false == false`: passes
    -> its `#[cfg(unix)]` half symlinks the lower-case name, which the filter admits: passes
    -> the full suite: the same four harness failures as the unmutated control, set
      difference empty — not one test in the suite dies under the mutation on Linux
    -> `a_native_case_insensitive_fan_out_alias_is_detected`, the witness that does kill it,
      is `#[ignore]`d; nothing in `.github/` or `scripts/` passes `--ignored`, and run on
      case-sensitive /tmp it fails its prerequisite rather than passing

So the P1 is guarded where the filesystem folds case — the macOS leg (APFS) and the Windows
leg (NTFS), whose `std::env::temp_dir()` the fixture roots its store under — and unguarded
on the ubuntu leg and on this build box, which is where the ten-gate baseline every push
of this pull request cites is measured. The ledger cell for the P1 named the ignored test as
if it were an ordinary CI test; round 5 corrected the cell and made the sibling assert its
branch on those two legs by target OS, which failed the test on a supported case-sensitive
volume — a `TMPDIR` on case-sensitive APFS — before the scan was checked
(`PR258-CASEFOLD-EXPECTATION-KEYED-ON-TARGET-OS`, executed by the round-5 regression lens
as the equivalent `cfg!(unix)` branch on Linux). Round 6 replaced that keying with a per-leg
declaration: `ci.yml` sets `UPSTROKE_TEST_TEMP_FOLDS_CASE=1` on exactly the two steps whose
temporary directories fold case, the test requires the native branch only under it and
observes the branch everywhere else, and the workflow oracle pins the declaration as a whole
map (`~/tactus-artifacts/tmpobj-evidence/34-r6-casefold-declaration.log`: declared on this
box's ext4 `/tmp` the test fails loudly at the declaration, declared on a casefold mount it
passes).

## Why it stays

A case alias cannot be constructed on a case-sensitive volume, and a symlink is not one:
the scan's `fs::metadata` follows it, and so does the round-2 filter once the link's own
name is lower-case. Making the ubuntu leg carry the guard means giving it a folding
volume — an `ext4 -O casefold` loop image mounted under `sudo` in the workflow, the way the
round-3 harness built one (`14-r3-casefold-harness.py`) — a workflow change of a different
order from the one-line declaration round 6 added to the two folding legs, and one the owner
reads line by line before merging, as every gate diff (`MAINTAINING.md`, trust boundary).
Every pull request runs the full matrix before it
merges, so the P1 is guarded before merge on two legs; what is accepted is that a local
green on this box, or the ubuntu leg alone, says nothing about it.

## What the change that takes this up should do

Give the ubuntu leg a case-insensitive temporary directory for the one ignored test —
mount a casefold ext4 image in `ci.yml` and run
`a_native_case_insensitive_fan_out_alias_is_detected` with `--ignored` and `TMPDIR` on it,
declaring `UPSTROKE_TEST_TEMP_FOLDS_CASE=1` for that run so the default alias test requires
its native branch there too — or add a `winguest`-style leg that already folds. Then drop
`#[ignore]` from the test's
role in the ledger: it becomes the named regression test on every leg, and this row closes.
