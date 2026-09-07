---
id: PR245-WINGUEST-IMAGE-SHIPS-FIXTURE-RESIDUE
severity: P2
disposition: deferred
category: correctness
pr: 245
reviewed_sha: 6405218a80f886a72081d2c24de88408e68b9fd0
location: src/validate.rs:282
provenance: pre_existing
first_bad: undetermined
guard: the lane that next opens `src/validate.rs`'s test region, which is also `PR104-VALIDATE-SCRATCH-DIRECTORIES-PREDICTABLE-AND-UNRECLAIMED`'s guard; and the operator who owns the Windows image
---

## Failure sequence

`ci.yml:79` describes the Windows leg as "a throwaway overlay of a frozen
Server 2025 image on the build box, one job per boot". The overlay is
thrown away; the image under it is not. Every Windows job therefore begins
with the image's `%TEMP%`, and that directory is not empty: measured on
`windowsguest-ci` (libvirt domain `winguest-ci`) on 2026-09-07,
`C:\Users\Administrator\AppData\Local\Temp` holds **259,985 directories**,
**25,187** of them named `upstroke-*` — fixture roots earlier runs left,
newest mtime 2026-09-01T19:31:45Z, which is when the image was frozen.

**316 of them are under names the current tree still computes**: 29
`upstroke-validate-hermetic-<pid>`, 29 `upstroke-validate-gates-<pid>`, 29
`upstroke-validate-nopools-<pid>`, 29 `upstroke-validate-review-<pid>`, 29
`upstroke-validate-pools-<pid>`, 29 `upstroke-validate-effort-<pid>`, 29
`upstroke-validate-<pid>` and 29 `upstroke-validate-dup-<pid>` — 232 under
names eight sites compute directly — plus 84 under the three
`upstroke-validate-captured*-<pid>` names its `scratch_root` helper
computes. `src/validate.rs` derives all of them from `env::temp_dir()`,
the tag and `std::process::id()`. The eight direct sites (`:282`, `:292`,
`:603`, `:651`, `:699`, `:739`, `:794`, `:807`) create the root with
`fs::create_dir_all` and nothing else, so an existing directory is
accepted and kept; `scratch_root` (`:310`) instead runs
`let _ = fs::remove_dir_all(&dir);` first, which is destruction rather
than adoption and is `PR64-CLEANUP-003-SCRATCH-PRECLEAN`'s subject.

A job whose test binary draws one of the process ids those adopted
names carry runs its fixture inside a directory another run populated, and
whatever that run left is what the fixture reads.

`PR104-VALIDATE-SCRATCH-DIRECTORIES-PREDICTABLE-AND-UNRECLAIMED` is the
open P1 for the predictability itself, and its evidence is a planted
sentinel on a developer machine. What this row adds is that the
precondition is not hypothetical on the Windows leg: the occupied roots are
already in the image, under the current names, on every job. The same
image also carried the 28 `upstroke-pr7h-question-<pid>-0` /
`upstroke-pr7h-retained-<pid>-1` pairs that
`PR160-WINDOWS-SETTLE-ALREADYSTARTED` turned out to be — that fixture was
repaired at `54ef9d5a` and the row closed in PR #245; these were found by
the same reading and are not repaired.

**Not established here**: no Windows job has been observed failing through
one of these `upstroke-validate-*` roots. This row is the measured
precondition and the count, not a sighting.

## What the change that takes this up should do

Two independent halves, and the first does not wait on the second.

**In the tree.** Move the remaining predictable-name fixtures onto owned
roots the way the settlement kill fixture already moved:
`rundir::scratch_tree::acquire`, one exclusive `create_dir` on a
ULID-carrying name, with the guard reclaiming it. `src/validate.rs` is the
site this row names and `PR104` already owns; `src/rundir/tests.rs`,
`src/workspace_manager/fixture.rs`, `src/runner/container/tests.rs`,
`src/runner/container/census/tests.rs`, `src/runner/container/resolve/tests.rs`
and `src/runner/container/view.rs` compute the same shape and additionally
`remove_dir_all` it first, which is `PR64-CLEANUP-003-SCRATCH-PRECLEAN`'s
subject rather than this one's.

**In the image.** A frozen image whose `%TEMP%` holds a quarter of a
million directories makes "ephemeral runner" mean something weaker than it
reads, and every future fixture author will read it the strong way.
Whether the image is cleaned before freezing, or the leg clears `%TEMP%`
before the suite, is the operator's call and outside this repository; what
belongs here is that no fixture may depend on the answer.
