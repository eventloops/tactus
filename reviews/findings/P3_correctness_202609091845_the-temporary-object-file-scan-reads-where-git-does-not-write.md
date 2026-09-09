---
id: G4B-O11-TEMPORARY-OBJECT-FILE-SCAN-READS-WHERE-GIT-DOES-NOT-WRITE
severity: P3
disposition: deferred
category: correctness
pr: 5
reviewed_sha: 81ee09efb5926fb4b93565223ea05dd0052bc2f9
location: src/workspace_manager.rs:4507
provenance: pre_existing
first_bad:
guard: the next change to the residue classifier's object-store reads — scan the fan-out directories too, and plant the synthetic element where git writes it
---

## Failure sequence

`temporary_object_files` (`src/workspace_manager.rs:4507`) reads `objects/tmp_obj_*` and
`objects/pack/tmp_pack_*`, and its doc says git "writes a loose object to `objects/tmp_obj_XXXXXX`".
Git 2.43 does not write there:

    strace -f -e trace=openat git hash-object -w big.bin        (git 2.43.0, the build box, 2026-09-09)
    -> openat(".git/objects/44/tmp_obj_0ztpj0"), openat(".git/objects/44/tmp_obj_eFakOS")
    -> the temporary file is in the object's fan-out directory, objects/XX/tmp_obj_*
    -> a `hash-object -w` killed mid-write leaves objects/XX/tmp_obj_* behind (`count-objects -v`
       reports `garbage: 1`; `fsck --unreachable` does not list it) and `temporary_object_files`
       returns false — measured in the row 10 decision document, §2.3, kills at 30 % and 60 % of a
       300 MB write, and the same location in the strace of `cherry-pick`, `write-tree` and `commit-tree`
    -> the synthetic tests plant `tmp_obj_repair` and `tmp_obj_synthetic` directly under `objects/`,
       where the scan looks (`src/engine/topology/dispatch/tests.rs:1083`,
       `src/engine/topology/attempt/tests.rs:1442`, `src/workspace_manager/tests.rs:8957`), so the
       synthetic half is self-consistent and does not model the file git leaves

The consequence is bounded. `cherry-pick --no-commit` and `write-tree` write their objects while
`index.lock` is held (the same document, §2.2: every object write of the pick between the lock's
creation and the publishing rename), so a kill mid-write at those sites classifies `Internal` through
the lock and the worktree is recreated regardless. The two `commit-tree` sites take no index lock, so
a kill inside their object write leaves only the fan-out temporary file and classifies `None` rather
than `Internal`; the tabled recovery for both classes re-runs the command, which converges, and the
file is R27 cruft `git prune` removes. So this is a classifier-evidence defect — an element the class
lists is never observed where the real command leaves it — and not a recovery defect. It does not
change G4's row 10 outcome.

A related sentence in the same module: the doc of `element_breaks_quiescence`
(`src/workspace_manager/residue.rs:403`) says ordinary Git use leaves unreferenced objects because
"every amended commit leaves one". An amend's old commit stays reflog-reachable, and
`fsck --unreachable` reports it only after reflog expiry; `git add` of an edited file, which leaves an
unreferenced blob at once, is the accurate example.

## What the change that takes this up should do

Extend the scan to the 256 fan-out directories (`objects/XX/tmp_obj_*`) beside the two paths it
reads, keep the pack path, and move the three synthetic plants into a fan-out directory so the
constructed element is the file git really leaves. Then re-run the RepairMaterialize and commit-tree
samplers and read whether any kill now classifies through the temporary file; if none does, say so in
the site's evidence rather than leaving the element implied. Correct the doc sentence at the same
time.
