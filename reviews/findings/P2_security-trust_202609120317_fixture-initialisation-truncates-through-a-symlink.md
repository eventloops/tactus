---
id: PR271-R4-FIXTURE-TRUNCATES-THROUGH-A-SYMLINK
severity: P2
disposition: deferred
category: security-trust
pr: 271
reviewed_sha: 4c245225a36f7e815da113445849515a611f67e9
location: src/workspace_manager/fixture.rs:515
provenance: introduced_by_feature
first_bad: 4c245225a36f7e815da113445849515a611f67e9
guard: the next change to the fixture's temporary-file creation
---

## Failure sequence

Executed by the round-4 regression lens at `4c245225a36f7e815da113445849515a611f67e9`, without modifying either revision's source.

`neutral_git_config()` writes to a **predictable filename** —
`<TMPDIR>/upstroke-neutral-gitconfig-<pid>` — with `fs::write(..., b"")`, which **follows an
existing symlink and truncates its target**.

1. Create a 17-byte sentinel file.
2. Symlink `<TMPDIR>/upstroke-neutral-gitconfig-<pid>` to it.
3. Run the existing `gates::tests::quoted_arguments_survive_the_windows_shell` under that PID.

| revision | exit | sentinel |
|---|---|---|
| base `5aebbbf6` | `0` | **17 bytes**, untouched |
| head `4c245225a36f7e815da113445849515a611f67e9` | `0` | **0 bytes** |

So a **passing** test silently destroys an unrelated file. The test still reports success, which is
what makes it worth a finding rather than a note.

## What the change that takes this up should do

Create the configuration file **exclusively**, inside a private temporary directory the fixture owns,
and never follow or truncate an existing entry. `O_EXCL` semantics, or a directory created with
`TempDir`-style uniqueness, closes it. A regression test should pre-place a symlink and assert the
target survives.

**Not merge-blocking, recorded here for why:** a P2 that does not block this pull request's goal,
which both lenses confirmed is met. It is test-fixture code reaching no production path, and it
requires someone to pre-place a symlink at a PID-specific path in `TMPDIR` — so it is not reachable
in normal use, and someone without push access cannot trigger it on a machine they do not already
have a local account on. **It should still be fixed:** on a shared build box, test runs that destroy
files are a real operational hazard, and the fix is small.
