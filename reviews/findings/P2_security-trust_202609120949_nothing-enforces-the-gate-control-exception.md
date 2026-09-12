---
id: PR274-NOTHING-ENFORCES-THE-GATE-CONTROL-EXCEPTION
severity: P2
disposition: deferred
category: security-trust
pr: 274
reviewed_sha: bd5bce6ca196a66e9cf8461a71e2ea4a436bc6f0
location: MAINTAINING.md:112
provenance: introduced_by_feature
first_bad: 983d453aaca27086fdb12ac114f4fde54a7587ad
guard: a required check that refuses a pull request whose diff can change what a check runs unless the body discloses a delegation the owner wrote for it
---

## Failure sequence

**The rule that keeps a gate-control change with the owner is applied by eye, by the delegate whose
merge it restricts, and nothing in the repository refuses the merge if the delegate reads it wrong.**
`MAINTAINING.md` step 7 says so in terms — "no check enforces this" — and this finding is what that
sentence points at.

The exposure the rule covers is not hypothetical, and both halves of it were executed. Measured in a
disposable copy of `bd5bce6ca196a66e9cf8461a71e2ea4a436bc6f0` built with `git archive`, whose
tracked tree hashes identically to the worktree, with `cargo` through the build box's wrapper:

| step | command | result |
|---|---|---|
| add `.cargo/config.toml` binding `[target.'cfg(all())'] runner = ["sh","-c","exit 0"]` | `cargo test --all-targets --all-features` | **exit `0`**, 554 bytes of output, **zero `test result:` lines**, three harnesses compiled and handed to the wrapper |
| the same tree | `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, all six `.github/scripts/test-*.sh` | **exit `0` each** |
| rebind the runner to `["env"]` so harnesses execute | `cargo test … -- --exact effects::tests::no_repository_file_overrides_what_ci_compiles_or_runs` | **exit `101`** — "these files outrank `.github/workflows/ci.yml` … `[\".cargo/config.toml\"]`" |
| drop `.cargo/config.toml` from `OVERRIDING_REPO_FILES` (`src/effects/tests/ci_model.rs:139`) | the same test | **exit `0`** |
| instead narrow the filter in `no_repository_file_overrides_what_ci_compiles_or_runs` (`src/effects/tests.rs:1318`) | the same test | **exit `0`** |

Each `cargo` invocation named `Compiling upstroke v0.1.0 (<the copy>)`, so the harness under test was
built from the copy and not served warm from another worktree's slot.

**All four files the validator polices — `.cargo/config.toml`, `.cargo/config`, `rust-toolchain.toml`
and `rust-toolchain` — are absent from the tree**, so none of them is protected by being already
present and reviewed: a pull request adds one. And the validator that refuses them is ordinary source
in the same tree, reachable by two routes — the constant at `src/effects/tests/ci_model.rs:139` and
the filter at `src/effects/tests.rs:1318` — so a pull request may weaken it in the same diff.

**Against CI rather than the local suite, the shape is the same but the runner is scoped.**
`.github/workflows/ci.yml`'s self-hosted step counts `test result: ok. <n> passed` and throws below a
floor of 1700, so a runner bound with `cfg(all())` reddens `test (winguest)` — that leg is the one
place in the tree that refuses this effect, and it refuses it for the Windows suite only. Binding the
runner per target triple instead is measured, not reasoned: `[target.x86_64-unknown-linux-gnu]` with
the same runner gives **exit `0`, zero `test result:` lines** on this Linux box. The remaining step is
read off `ci.yml` and not executed here, having no CI of my own to run: such a binding leaves
`test (winguest)` counting a real Windows suite while `test (ubuntu-latest)` and `test (macos-latest)`
execute nothing, and the validator — which does run on the Windows leg — is what then has to be
weakened. Both changed paths are outside every directory the two earlier revisions of this rule
named, and a delegate who checked each changed path against those lists would have found nothing and
merged.

## Why it is P2 and not P1

The rule at `MAINTAINING.md:83` now covers this case: it asks of each changed path whether the file
could make a required check report success without doing its work, which `.cargo/config.toml` and a
weakened CI-contract test both answer yes to. What is left is that the answer is given by a reader
and believed. A delegate applying the rule honestly is not exposed; a delegate who misreads it, or an
author who does not think of the question, is — and nothing between them and `master` says so.

## What the change that takes this up should do

**A required check that fails when a pull request's diff can change what a check runs, unless the
body discloses a delegation the owner wrote for that pull request.** Sketch, deliberately not a
design:

- a path-pattern floor is the cheap half and is worth having even though it cannot close — the
  examples step 7 lists are exactly the patterns, and a check that refuses them catches the ordinary
  case that a delegate is most likely to wave through;
- the half that matters is the disclosure: the body already carries a **Merge delegation** section,
  so the check can require, for a diff that trips the floor, a line naming a delegation the owner
  wrote for this pull request, in the shape `validate-pr-body.sh` already parses for its other
  required fields;
- it fails expensive: a diff it cannot classify is a diff it refuses, matching this file's rule for
  the human reading the same diff.

**It cannot be written by a pull request merged under standing delegation.** The check would live in
`.github/scripts/` and be invoked from `.github/workflows/ci.yml`, which is the shape the rule sends
to the owner — this finding's fix is its own first test case, and that is the reason it is filed here
rather than implemented in the change that wrote the rule.

**The floor must not be written as a closed set.** Two successive revisions of this rule enumerated
the gate-control paths and a review broke each of them: the first named `.github/workflows/` and
`.github/scripts/` and missed `scripts/`, which the audit gate sources; the second added `scripts/`
and missed `.cargo/` and the CI-contract tests under `src/effects/`. A check whose pattern list is
presented as complete would restore exactly the false assurance the rule was rewritten to remove.
