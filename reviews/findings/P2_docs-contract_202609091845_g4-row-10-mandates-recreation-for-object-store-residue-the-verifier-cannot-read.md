---
id: G4-O4-ROW10-VERIFY-CLAUSE-CORRECTION-OWED
severity: P2
disposition: deferred
category: docs-contract
pr: 249
reviewed_sha: 81ee09efb5926fb4b93565223ea05dd0052bc2f9
location: src/workspace_manager/residue.rs:413
provenance: pre_existing
first_bad: PR5-VERIFY-CLAUSE-NARROWER-THAN-STATED
guard: project owner — apply the replacement text in §6 of the row 10 decision document (`G4-ROW10-DECISION.md`, kept with G4's evidence on the build box, sha256 65f32b04…) to row 10 of the packet's G4 `adversarial_fault_tests`; the G4 verdict is conditional on it
---

## Failure sequence

Row 10 of G4's `adversarial_fault_tests` (the packet, v16) says the interrupted-materialization
residue class "(unreferenced objects plus `CHERRY_PICK_HEAD`/`index.lock`, constructed synthetically
and sampled by killing the cherry-pick child) fails `Worktree.Verify`, so the worktree is recreated
and the materialization reproduced deterministically". The packet's own definition of
`Worktree.Verify` — T-DISPATCH's `resume_action`, quoted word for word in
`docs/internals/engine/topology/dispatch.md` — is "linked worktree at the recorded path, HEAD == base,
index unlocked, no cherry-pick/merge/sequencer state": four conditions, none a read of the object
store.

    the synthetic test plants one element alone, as the packet's evidence rule requires
    -> for `UnreferencedObject` or `TemporaryObjectFile` the worktree's git dir is clean and HEAD is at the base
    -> `Worktree.Verify` (`WorkspaceManager::quiescence`) passes: it reads the git dir and HEAD, never the object store
    -> `element_breaks_quiescence` (`src/workspace_manager/residue.rs:413`) says so on purpose, and
       `repair_materialization_synthetic_residue_recreated_after_forced_removal` asserts reuse for those two
    -> the row's mechanism clause is not met for those two elements, and no recreation could meet it:
       the elements are in the shared object store, which `worktree remove` and `worktree add` never touch
    -> a gate judged on the row as written cannot pass. G4's second run first waived the clause
       ("engages no clause of the pass rule"); the independent verification rejected that as a report
       substituting its own acceptance condition, and the rejection is right

Measured for the owner's ruling (the decision document, 2026-09-09, git 2.43 on the build box; its
scripts and raw outputs are kept with G4's evidence): the two elements are R27, owner "Git object
store", in the packet's own ownership matrix; after recovery a reused and a recreated worktree are
identical in fifteen observables, byte for byte; a verifier that consulted the object store would
refuse this repository today (1,218 unreachable objects) and every clone after two `git add`s, and
would recreate on every resume for ever, because recreation cannot clear the condition; and the row's
real members already fail `Worktree.Verify` — git 2.43 holds `index.lock` across every object write of
the pick, so 36 of 36 real kills that died inside the pick left `index.lock`, and none left an
object-store element alone. The object-store-only worktree exists only because the evidence rule
requires each element to be constructed by itself.

The same over-statement is on the ledger at the PR5 slice level as
`PR5-VERIFY-CLAUSE-NARROWER-THAN-STATED` (P3, for the G2 erratum list), where it describes
`proof_tests[8]` and `command_internal_sub_effects`. This is the gate-level instance: an acceptance
criterion a gate is judged against, whose wording contradicts the packet's own verifier definition,
its ownership matrix (R27) and its integrated invariant ("a repair worktree with **administrative**
residue is recreated, never reused"). Not a behaviour defect — the outcome the row secures, the
materialization reproduced deterministically with the objects Git's, is established for all four
elements — but a gate that cannot pass unconditionally until the owner acts, which is why it is P2
and not P3.

## What the change that takes this up should do

The packet is not in this repository, so this is the owner's act, not a pull request here. Replace
row 10 with the §6 text of the decision document (quoted in full in `reviews/2026-09-09-gate-G4.md`,
§9 row 10). It keeps the first clause, the element list, the "constructed synthetically and sampled"
evidence requirement and the deterministic-reproduction outcome, and changes only the mechanism
clause: an element in the worktree's git dir fails `Worktree.Verify` and the worktree is recreated
with force; the object-store elements are R27, survive reuse and recreation alike, and by themselves
do not fail `Worktree.Verify`; in every case the materialization is reproduced deterministically to
the tree an uninterrupted one produces. If the retired decision text is still consulted, amend the
"`Worktree.Verify` fails" sentence of `command_internal_sub_effects` the same way; the living design
(`design/26_design_merge_queue_protocol.md`) already states the per-element requirement without a
Verify clause. Keep `CHERRY_PICK_HEAD` in the row: the frozen register still names it, and that is
`PR9-N1`, a separate approval. Change no code: the partition and its reason are already stated at
`element_breaks_quiescence` and asserted by the synthetic test. Then delete this file and record the
applied correction in G4's report or its successor, which turns the conditional pass into an
unconditional one on the same code sha.
