---
id: PR8-CRASH-002-PACKED-REFS-LOCK
severity: P2
disposition: deferred
category: crash-consistency
pr: 8
reviewed_sha: 3eea2ea04e0230beb3bec33d1fd70ce918273352
location: src/workspace_manager.rs:2766
provenance: pre_existing
first_bad: PR8-CRASH-002
guard: the project owner — a change to the ref protocol, not a reclaim rule
---

## Failure sequence

Git's files backend takes `packed-refs.lock` in the common git dir for every ref **deletion**,
whether or not the ref is packed (it locks first and decides afterwards whether the packed file
needs rewriting), and for every `pack-refs`. A process killed inside that window leaves the file,
and Git refuses every later deletion in the repository until it is removed.

    delete_ref_expected_old (a prepared pin, a candidate pin, a candidates ref at finalization)
    -> git update-ref -d killed after creating packed-refs.lock and before releasing it
    -> resume retries the deletion
    -> Git refuses on packed-refs.lock
    -> every later resume repeats the refusal until an operator removes the file

The same file was met on the macOS runner left by a killed `git cherry-pick`, whose deletion of
`CHERRY_PICK_HEAD` takes the same lock.

The refusal is resumable and loses nothing; the cost is liveness, as for `PR8-CRASH-002`.

## Why it is deferred rather than repaired

`PR8-CRASH-002` closed the `<ref>.lock` half of its class by reclaiming a lock the repository
proves is the engine's own and stale. No such proof exists for `packed-refs.lock`: it is the
repository's, not the run's; every Git process in the repository — agents' commands in their
worktrees, other runs, the operator — takes it; a live holder's and a dead holder's lock are
byte-identical (empty, or a partially written packed file); and a wrong removal is catastrophic
rather than merely racy, because the live holder's `rename(packed-refs.lock, packed-refs)` would
then publish whatever file another process had since created at that name over every packed ref in
the repository. Age and the absence of a process are not facts the repository records, so the
engine leaves the file, the deletion refuses resumably, and the operator removes it.

## What the change that takes this up should do

Decide, as an owner decision on the ref protocol rather than as a reclaim rule, how engine ref
deletions stop depending on a repository-global lock they cannot prove ownership of. The candidates
are a protocol that deletes nothing during a run (pins and candidate refs pruned only at
`run_finished`, where a refusal wedges no publication), or a ref store whose deletions take no
shared lock. Whatever is chosen, the witness is a killed `git update-ref -d` whose `packed-refs.lock`
survives it, followed by a resume that completes without an operator.
