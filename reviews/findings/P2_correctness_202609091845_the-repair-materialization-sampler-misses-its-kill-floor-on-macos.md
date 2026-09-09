---
id: G4B-O10-REPAIR-MATERIALIZE-SAMPLER-MACOS-KILL-FLOOR
severity: P2
disposition: deferred
category: correctness
pr: 249
reviewed_sha: 81ee09efb5926fb4b93565223ea05dd0052bc2f9
location: src/engine/topology/dispatch/tests.rs:1328
provenance: pre_existing
first_bad: PR7-SAMPLER-SCHEDULES-FROM-A-COLD-PROBE
guard: the project owner — post-promotion sampler hardening, together with `PR80-MACOS-WORKSPACE-SAMPLER-COLD-PROBE-RECURRENCE` and `RECOVER-CHERRY-PICK-SAMPLER-COLD-PROBE`
---

## Failure sequence

`sampled_repair_materialization_child_kills_every_residue_classified_and_recovered` needs
`SAMPLING_N` (8) children to die by the kill within `MAX_SPAWNS` (64) spawns, at least one of them
while the pick was writing. Each spawn's kill is aimed as a fraction of a budget taken from the
shorter of two probe picks — the warm-probe treatment its T-PROPOSAL sibling lacks. On the macOS
runner the one-file pick it schedules against completes before most kills land:

    test (macos-latest), run 34385164329 at c00c8638 — pull request #257, whose diff is Markdown under reviews/ only
    -> 64 spawns; 7 children died by the kill, 57 picks completed before their kill
    -> `kills.len() >= SAMPLING_N` fails (src/engine/topology/dispatch/tests.rs:1328)
    -> the required leg is red on a change that cannot have caused it, and the token cannot rerun a job

The test's own doc comment records the shape at `56ea88c9` (6 kills in 32 spawns on macOS, 26 picks
complete before their kill) and answered it with the two-probe budget and the 64-spawn bound; at
`c00c8638` the same shape appears under that answer, one kill short of the floor. On Linux the
sampler's five runs for G4 collected 40 kills in 48 spawns (10 of them mid-write); `test (winguest)`
is green at the same head; the frozen sha `81ee09ef` was green on all three legs. So the schedule is
adequate where the pick is slow relative to the aimed points and marginal on macOS, where it is not —
the class the nine sampler findings describe, now naming this test, which none of them did.

This does not bear on G4's row 10 evidence: the sampled half stands on the five Linux runs whose
histograms are in the report, every kill classified and recovered. It bears on the leg, and on every
pull request the leg gates.

## What the change that takes this up should do

Take it up with the sampler family rather than alone. Two options the siblings' history has
measured: schedule against a median of several probe picks rather than the shorter of two, or aim
the kill points at the pick's own progress (the `while_writing` observation shows when it is inside
its object writes) rather than at fractions of a probe duration. Keep the floor as a count of
children that actually died by the kill: the aim is what is wrong, not the floor.
