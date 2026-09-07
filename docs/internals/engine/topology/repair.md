# `src/engine/topology/repair.rs`

Extended notes for [`src/engine/topology/repair.rs`](../../../../src/engine/topology/repair.rs).

The code is the authority for what it does; this file is the whole of its prose, moved out of
the source verbatim. Each section is headed by the line of code the comment sat above, spelled
as it is in the source, so the heading is the grep string that finds the code.

## Module

The frozen repair a `merge_rejected` registers atomically with the
rejection.

`decisions.repairs.merge_rejected` and `DESIGN.md` §26.4: a conflict or a
code-attributed rejection records "one `merge_rejected` event whose
embedded frozen-spawn payload" carries the complete synthetic Fix task, so
rejection and registration are one append. This module builds that payload
and the whole `MergeRejected` around it; **it registers a repair, it never
dispatches one** — dispatch is `T-REPAIR-DISPATCH`, PR9's, and the
checkpoint refuses it.

What the payload freezes, from the contract:

* the repair descends from the rejected candidate: `lineage = {root,
  parent, index}`, the root being the candidate's own lineage root (or the
  candidate itself, for an ordinary one), the parent the rejected key, and
  the index the number of repairs the root already holds;
* `min_tier = mid` intersected with the root's frozen floor, pin and
  ceiling — the router's floor read off a recorded ladder rather than
  re-run — with an empty intersection registered `HumanBinding`;
* path hints expanded by the candidate's actual changed paths and the
  conflict paths (`§26.4`);
* the original acceptance plus the preserve-merged-behaviour requirement;
* admission `Runnable`, or `HumanRequired` once the lineage has consumed
  the frozen automatic-repair limit, or `HumanBinding` when the tier
  intersection is empty — the empty intersection winning, because without a
  binding nothing can run whatever the limit says.

## `const REPAIR_FLOOR: Tier = Tier::Mid;`

The mid floor a merge repair starts no lower than, before the root's own
floor and ceiling clip it further.

## `const PRESERVE_MERGED: &str = "preserve the behaviour already merged onto the integration head; the rejected candidate's \`

The requirement every merge repair carries beyond the root's acceptance.

## `pub fn merge_rejected(`

Build the `merge_rejected` for a rejected candidate: the terminal event, its
embedded repair, and its lease effect, all as one record.

`disposition` is what rejected the candidate — a textual conflict at the
cherry-pick, or a code-attributed gate/review failure — and `contended` is
the region the repair lineage takes a lease on: the conflict paths for a
conflict, the candidate's actual changed paths for a code rejection.

### Errors

A refusal when the run has not started or the candidate is not registered.

## `pub fn merge_rejected(` › `let mut spec = root_entry.spec.clone();`

The specification is the one part of the root the repair rewrites —
its kind, its hints and its acceptance — so it is cloned once and
edited in place (§6); the rest of the entry copies the root's fields
because the contract fixes them as the root's (`decisions.repairs`:
the root's authoritative deps, and the inherited review and agent
policy), and a registry row owns what it records.

## `fn candidate_paths(fold: &TopologyFold, candidate: &CandidateRef) -> PathSet {`

The candidate's actual changed region, as `candidate_prepared` recorded it.

## `fn repair_ladder(root: &FrozenLadder, allowed_agents: &[String]) -> FrozenLadder {`

The repair's ladder: `min_tier = mid` intersected with the root's frozen
floor and ceiling (`decisions.repairs.routing`).

The floor is `max(mid, root floor)`; the root's rungs at or above it
survive, in the root's order, and the ceiling is the highest of them.
When none survives the record says so: no tier, no rung, no ceiling, the
raised floor the repair still has to meet, and the ladder admitted
`HumanBinding` over the entry's allowed agents — every agent the run
probed, which is what `check_spawn` binds `allowed_agents` to — so a
person names what runs (E2: the answer's override is validated against
these options). The rungs the intersection excluded are not offered back:
each of them is below the floor by construction. `check_ladder` accepts
the shape — an absent ceiling is the maximum of an empty tier list.

## `fn admission_for(`

The repair's admission: `HumanBinding` when the ladder is (the empty
intersection), `HumanRequired` once the lineage is at its automatic-repair
limit, else `Runnable`. The empty intersection wins, because without a
binding nothing runs whatever the limit says — and the fold refuses
`HumanRequired` on a `HumanBinding` ladder, so it is the only admissible
shape for an over-limit rejection with no tier left.

## `fn expand_hints(base: &[String], actual: &PathSet, contended: &PathSet) -> Vec<String> {`

The repair's path hints: the root's hints, the candidate's actual changed
paths, and the contended (conflict or rejection) paths, deduplicated in
that order.

A `RepoWide` region contributes no hint — the lineage lease already holds
everything a repo-wide region would, so widening the hints by it says
nothing the lease does not.

## `pub fn code_rejection_record(`

The verification record a code rejection carries, from a judgement's gate
and review results.

`VerificationRecord.verdict` is `GatesFailed` when a gate refused,
`Rejected` when a reviewer did; `gates_passed` is whether every gate
passed; `reviews` are the pass records; `detail` is the failure's reason.
