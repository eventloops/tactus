---
id: READINESS-PRODUCER-DROP-LEAVES-A-GRANDCHILD
severity: P2
disposition: deferred
category: liveness
pr: 36
reviewed_sha: 25a9e1890f3bae955d3035e1e2dadca635a0776d
location: src/agent/proc/test_support/readiness.rs:309
provenance: pre_existing
first_bad: 1cd4b1e
guard: deferred: no test witnesses a wedged grandchild today; the fix is a process-tree kill in Producer::drop and a panic-safe store guard, and both need a fixture that spawns a helper which itself spawns a child that outlives it…
---

## Failure sequence

`readiness::Producer` owns one `std::process::Child`, and `impl Drop for Producer`
(`src/agent/proc/test_support/readiness.rs:309`) ends it with `self.child.kill()` then
`self.child.wait()` -> that signals **one process**, not the helper's process tree, so a helper that
has itself spawned `git` and then wedged leaves that `git` running when the `Producer` is dropped ->
`await_signal` returns `Waited::TimedOut` and the test fails, which is correct, but the surviving
grandchild still holds handles on the snapshot store the test is about to remove -> on Windows
`fs::remove_dir_all` then fails against the open handles, and the documented NTFS behaviour is that
the name is unlinked only when the last handle closes, so the directory persists past the call that
asked for it and a later run can collide with the leak.

The store's cleanup is also not panic-safe. `PendingSnapshot::create` (`src/workspace.rs:1280-1299`)
creates the store with `create_private_dir`, then cleans up only on the `Err` arm of the match:

    match Self::create_in_store_inner(source_root, &store, Some(store.clone())) {
        Ok(pending) => Ok(pending),
        Err(error) => { let _ = fs::remove_dir_all(&store); Err(error) }
    }

There is no `Drop` guard on `store`, so an unwind between `create_private_dir` and that match — an
assertion failure in a test, or any panic in `create_in_store_inner` — passes over the cleanup and
leaks the directory. `let _ =` additionally discards the removal's own failure, so the leak is
silent in the error path too.

Neither leg is a defect in the passing path: a helper that publishes and exits is killed and reaped
correctly, and a clean `Err` return does remove the store. Both are cleanup-on-failure gaps, which is
why this is filed rather than fixed here — but they convert an already-failing test into a dirty
failure, and directory leaks of this shape are a known source of worktree-slot poisoning in this
repository.

`first_bad` names `1cd4b1e refactor(effects): resolve whole-file test modules structurally`, which is
where the current text of `impl Drop for Producer` first appears in this file. That commit moved test
modules structurally, so the single-process kill probably predates it under another path; the field
records where the code reads today, not a proven introduction.

## What the change that takes this up should do

Kill the tree, not the process, and make the store's removal survive an unwind.

The machinery already exists in-tree: `src/agent/proc.rs` carries the Job Object and process-group
handling (`windows_job`, `spawn_suspended_in_job`) that ends a whole tree on both targets, and
`Producer` is a test-support type in the same module, so it can use them without widening any public
surface. Note that `spawn_suspended_in_job` is `pub(super)` and takes a `&mut dyn SpawnHooks`, so the
call has to be arranged rather than copied from older code — closed PR #36 attempted exactly this
work and no longer compiles against the current signature, which is part of why it was superseded.

For the store, replace the `Err`-arm removal with a guard value whose `Drop` removes the directory
unless it is disarmed on success, so an unwind cleans up on the same path an error return does.

A fixture has to witness the grandchild: spawn a helper that itself spawns a child which outlives it,
drop the `Producer`, and assert the grandchild is gone — and assert on Windows that the store
directory is actually removed rather than left pending-delete. Without that witness the repair is
unfalsifiable, and §7's panic policy applies to any new test code.

Found while assessing PR #36 for supersession. Master closed that PR's actual race in `14aa611`
(atomic publish via staging and rename, plus a producer-bounded wait); this is the part of #36's
third commit `e31e642` that master's fix does **not** cover.
