---
id: SWEEP-CONNECT-014
severity: P2
disposition: deferred
category: crash-consistency
pr: 189
reviewed_sha: 0feb10229fe8f7c777d03df958f05d0ac8f7b2de
location: src/connect.rs:274
provenance: fix_regression
first_bad: cfabd16d3a5478490a842fc0c29901507a905926
guard: no regression yet. `a_directory_replaced_after_its_entry_was_flushed_has_the_replacement_flushed_before_return` covers only an empty component replaced before the publication; the rename-away after publication needs identity-bound barriers and an end-to-end test of its own, which is more than the four-review budget of the parked-PR recovery had left
---

## Failure sequence

Location as first recorded: `src/connect.rs:274` at the reviewed sha, the loop in
`flush_created_entries` that reopens each created component's parent by path.

`upstroke connect --force` is asked to publish `<root>/made/here/pools.toml`, where neither
`made` nor `here` exists -> `create_directory_durably` creates `made` and flushes its entry in
`<root>` -> another process removes `made` and recreates it before `here` is created, so the
replacement `made` entry is in `<root>` unflushed -> this call creates `here` inside the
replacement, stages `pools.toml`, renames it into place and flushes `made/here` -> before the
final pass the other process renames the now non-empty `made` to `moved`, which moves the whole
published subtree without deleting anything, and creates a new empty `made` -> the final pass
walks the recorded path names, so it opens and flushes the *new* `made` and then `<root>` ->
`write_pools` returns `Ok`, with the published file at `moved/here/pools.toml` and nothing at the
path the operator asked for. There is no instant after the replacement `made` was adopted at which
the complete published tree was both durable and reachable at the requested path.

The `cfabd16d` repair that closed `SWEEP-CONNECT-013` rests on a premise recovery review 4 showed
to be false — that publishing the file makes each ancestor non-empty and therefore unreplaceable.
POSIX `rename(2)` moves a non-empty directory, taking its whole subtree with it and deleting
nothing, so "non-empty" bounds nothing. Because the repair keeps only path names, its second flush
follows whatever occupies those names at the moment it runs rather than the chain that holds the
file it published.

The existing regression
`a_directory_replaced_after_its_entry_was_flushed_has_the_replacement_flushed_before_return`
replaces an *empty* component *before* the publication and drives the two helpers separately; it
never renames an ancestor away afterwards, so mutations M19 and M20 do not reach this sequence.
Nothing in the suite fails on this defect today, which is why it is filed rather than claimed
fixed.

## What the change that takes this up should do

One change, with a review budget of its own, touching `src/connect.rs`, `docs/internals/connect.md`
and `standards/SWEEP.md`:

- Tie the final barriers and the success decision to the *identity* of the directory chain that
  holds the published file rather than to its path names. Either build and flush through directory
  handles with identity validation and a stated linearization point, or, after the last flush,
  compare the identity of every component this call created and of the published file — device and
  inode on Unix, volume serial and file index on Windows — against what this call created and
  published, and refuse with an error naming the component that moved when any identity differs.
- Add a deterministic end-to-end regression that reproduces `SWEEP-CONNECT-013`'s pre-publication
  replacement first, then renames the non-empty ancestor away and installs a replacement after the
  publication and before the final barriers; the injectable flush is the seam. It must witness that
  the call cannot return durable success unless the file is still at the requested path with its
  ancestor entries flushed.
- Correct the record in the same change. The `flush_created_entries` section of
  `docs/internals/connect.md` still carries the sentence review 4 disproved, that a non-empty
  directory cannot be replaced by anything short of deleting the file; the `src/connect.rs` row in
  `standards/SWEEP.md` carries the same premise as "only the published file pins the chain"; and
  PR #189's durability account is written to the contract this defect breaks. All three are
  rewritten to whatever contract the repair establishes. They were left standing deliberately:
  PR #189's review budget was spent, and a prose correction pushed after the last pass would be a
  change no review covered.

Until then the honest statement, which PR #189's body makes in its Summary banner and its Risk and
rollback section, is that a non-empty ancestor renamed away between the publication and the final
flush is not detected, and `connect --force` can report durable success with the pools file no
longer at the requested path.
