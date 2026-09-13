---
id: PR8-CRASH-002-PACKED-RUN-REF
severity: P2
disposition: deferred
category: crash-consistency
pr: 8
reviewed_sha: 3eea2ea04e0230beb3bec33d1fd70ce918273352
location: src/workspace_manager.rs:2743
provenance: pre_existing
first_bad: PR8-CRASH-002
guard: the project owner — whether a run's refs may be packed at all
---

## Failure sequence

`WorkspaceManager::reclaim_own_ref_lock` reclaims a stale `<ref>.lock` on one of the run's refs
only while the ref is **not** in `packed-refs`, because `git pack-refs --prune` takes exactly that
lock, after committing the packed copy, for the instant in which it deletes the loose ref, and a
prune's lock and a dead engine writer's are both empty. Once a `pack-refs --all` has run during a
run — an operator's `git gc`, or an auto-gc — every run ref is packed, and the reclaim refuses.

    git gc during the run packs refs/upstroke/runs/<id>/integration
    -> merge_prepared durable
    -> git update-ref killed inside its lock window, leaving integration.lock
    -> resume: the lock is empty, the ref is packed, so a live prune cannot be ruled out
    -> the write refuses resumably, naming the packed ref
    -> every later resume repeats the refusal until an operator removes the file

The refusal is resumable and loses nothing; the cost is liveness, as for `PR8-CRASH-002`, and it
needs two rare events in one run.

## Why it is deferred rather than repaired

The fact the reclaim wants is "no `pack-refs --prune` holds this lock now". With the ref unpacked
the repository records that fact (a prune's lock can exist only after its packed entry does, and
the reclaim reads the lock before the packed file). With the ref packed it records nothing that
distinguishes the two holders, and a reclaim that guessed wrong would remove a live prune's lock,
after which the prune's `unlink` of the loose ref can delete the publication the retried swap just
wrote while the log records it as merged — a correctness loss, not a liveness one. Holding
`packed-refs.lock` for the duration of the reclaim would exclude the prune but would hand every
other Git user of the repository a lock the engine could itself leave stale; it was considered and
rejected in the change that closed `PR8-CRASH-002`.

## What the change that takes this up should do

An owner decision on whether a run's refs may be packed at all. Git 2.45's `pack-refs --exclude`
and `gc` configuration can keep `refs/upstroke/runs/` out of the packed file on the operator's
side, and a run that recorded that guarantee could reclaim without the packed-refs condition; a run
that cannot guarantee it keeps today's refusal. Whichever is chosen, the witness is a packed run ref
with a stale lock, followed by a resume that completes without an operator.
