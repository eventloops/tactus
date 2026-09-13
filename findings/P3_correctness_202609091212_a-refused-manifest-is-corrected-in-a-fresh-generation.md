---
id: PR249-REFUSED-MANIFEST-HANDOFF
severity: P3
disposition: deferred
category: correctness
pr: 249
reviewed_sha: 697c1ac896ddf054cfb743d38540ea279e0f646a
location: src/engine/topology/run.rs:1378
provenance: introduced_by_feature
first_bad: PR #249's third repair round (`a6f3898f`), which let a retained retry's manifest revise a resolution and wrote into `design/26` §26.4 that a refused manifest stays "for the worker to correct, and the next capture reads it"; the driver never made that capture
guard: `design/26` §26.4 says since the sixth repair round that a refused or failed capture leaves the manifest and the driver dispatches a fresh generation; the refusal's feedback names the entries and spells the grammar (`unresolved_conflict_failure`), so the fresh generation's worker can declare afresh; `an_unresolved_conflict_fails_the_capture_before_any_gate_and_a_declared_one_is_staged_by_the_engine` pins the refusal, `a_retained_retry_reads_the_manifest_while_the_index_holds_what_a_capture_resolved` the capture API's reread
---

## Failure sequence

A conflict repair's worker resolves its conflicted paths with its file tools and writes a
manifest the capture refuses — a line that does not parse, a path declared both ways or in
another case, an unmerged entry left undeclared.

    capture: `plan.refused` non-empty -> `Capture { unresolved, tree: the base's }`, nothing
      staged, the manifest left standing
    -> `assess` reports the worker's failure (`unresolved_conflict_failure`), whose feedback says
       the manifest "stays for you to correct; in a later attempt of this repair, write it again
       only for a path whose resolution you are changing"
    -> `resumable = plan.session_resume && session_id.is_some() && capture.unresolved.is_empty()`
       is false (`run.rs:1378`), so `next_step` yields no resumed retry
    -> `settle_failed` records `Closed` (only `RetrySameRung { resume: true }` with a session is
       `Retained`), and the fold closes the generation
    -> the next attempt is dispatched into a fresh generation and worktree: the conflict is
       materialized again, the worker resolves every path again and writes a new manifest; the
       refused one is reclaimed with the closed generation's worktree, never read

The correction happens, in a fresh generation, at the cost of the resolution work and one attempt
of the ladder's allowance; the feedback's "stays for you to correct" and "write it again only for a
path whose resolution you are changing" describe a retained worktree the worker will not see. Until
PR #249's sixth repair round `design/26` §26.4, `RESOLUTION_MANIFEST`'s doc and the capture note
promised the in-place handoff; that round corrected them to what the driver does (record §17
item 2) and left the worker-facing strings, which are production text, to this finding. The sixth
round's record review reasoned the sequence from production control flow; no witness was executed,
and the retained-retry test that corrects a refused manifest calls `capture` again directly,
without the settlement between.

## Why this is deferred

Making a refused capture resumable is new behaviour — a change to the ladder's input in
`src/engine/topology/run.rs` and to what a retained retry re-enters (a worktree with unmerged
entries, which is the state a conflict repair starts in) — outside a documentation-only round and
outside this slice's packet, which reads "unresolved index entries fail capture before gates" and
says nothing about resuming the attempt that failed. Whether the in-place correction is worth
having is the owner's call: the cost today is one attempt and the resolution work, bounded by the
ladder, and the fresh generation's worker receives feedback that spells the grammar.

## What the change that takes this up should do

Decide whether a refused capture may be resumed. If so, drop `capture.unresolved.is_empty()` from
`resumable` (or give the refusal its own settlement) so the retry re-enters the worktree with the
refused manifest standing and the next capture reads it — which the capture API already supports —
and add the retained-retry test's sequence with the live settlement between the two captures. If
not, correct the two worker-facing strings (`classify::unresolved_conflict_failure`,
`repair::repair_body`) to say the next attempt starts afresh and every resolution is declared
again, and let `design/26` §26.4 stand as the sixth round wrote it. Either way the feedback and
the design must say the same thing.
