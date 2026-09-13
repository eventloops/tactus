---
id: LEGACY-WORKSPACE-READS-REPLACEMENT-OBJECTS
severity: P1
disposition: deferred
category: correctness
pr: 130
reviewed_sha: 7cb6ab9641a6c49360cb474dfbcfd2e933e53cd3
location: src/workspace.rs:485
provenance: pre_existing
first_bad:
guard: the schema-4 engine replacing the legacy paths (PR7-PR11), per `src/workspace.rs`'s `[[legacy]]` row in `effects/allowlist.toml`; or an owner decision to unfreeze the module for this one change
---

## Failure sequence

`design/15_design_event_log_resume_run_layout.md` now says an exact snapshot is measured against
the objects the repository holds, and `NO_REPLACEMENT_OBJECTS` is set at the four boundaries the
schema-4 path starts a Git child from. The **schema-1..3** path is not one of them: `Workspace`'s
twenty `std::process::Command::new("git")` sites in `src/workspace.rs` set no such pair, and the
module is frozen — `effects/allowlist.toml`'s `[[legacy]]` row for it cites
`invariants_preserved[1]`, "this module's behaviour untouched" — so this change did not widen
into it.

With `refs/replace/P -> Q` installed, `capture_candidate` takes `parent_oid` from
`head_sha_full` (`rev-parse HEAD`, which prints the raw `P`) and `tree_oid` from
`staged_tree_oid` (`write-tree`, raw from the index), then builds the review payload as
`git diff <parent_oid> <tree_oid>` -> Git resolves `parent_oid` through the replacement, so the
diff describes `Q -> T` -> the reviewer judges a diff against content the commit does not have as
its parent, while `commit_tree_with_upstroke_identity` records the raw `P`. Measured on git
2.43 in a two-commit repository (`P` holding `one`, `Q` holding `two`, the index holding
`agent-edit`): with the replacement absent the payload is `-one +agent-edit`, with it installed
the payload is `-two +agent-edit`, and `git cat-file commit` on the object `commit-tree`
returns names `parent P` either way. `DESIGN.md` §4's "ground truth is the diff" then names a
diff that describes a commit the engine did not create, on the released v0.1 path.

Recorded while implementing the repair for
`PR130-REVIEW3-REPLACEMENT-ISOLATION-STOPS-AT-THE-MANAGER`, which closed the same gap on the
schema-4 path and made the schema-1..3 one nameable.

## What the change that takes this up should do

Set `NO_REPLACEMENT_OBJECTS` on `src/workspace.rs`'s Git children, in as few places as the
module's shape allows (`git_output`, `run_git_with_private_hooks`, and the ad-hoc builders that
bypass both), and add one witness on `capture_candidate` of the shape above: the payload with a
replacement of the parent installed equals the payload without it. The module is frozen at PR5 and
its row says behaviour untouched, so this needs either the unfreeze decision for this one change or
the schema-4 replacement that retires the module; a sweep that touches the file for other reasons
should take it up at the same time.
