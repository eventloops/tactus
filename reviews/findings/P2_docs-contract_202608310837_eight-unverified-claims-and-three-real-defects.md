---
id: PR7-R4-CLAIMS-UNVERIFIED
severity: P2
disposition: deferred
category: docs-contract
pr: 7
reviewed_sha:
location: 
provenance: undetermined
first_bad:
guard: project owner — the claims protocol a fresh session carries
---

## Failure sequence

**Eight claims written into commit messages and doc comments of the round-3 repairs are false, and each is one `grep` from disproof.** Round 4 — five lenses over the six commits `0cd2001..040a100`, scoped to that diff alone — returned **27 findings, every one inside it**, on a head green on Linux (1702/0), the Windows guest (1651+10) and CI (10/10). The eight: (1) `an_ending_run_reaches_closure` cited as an existing test whose scoping gap justified a new witness — **the test does not exist**, the name occurs once, in that doc comment; (2) the pool census described as asserting "what actually failed" — it inspects `attempt.rs`/`settle.rs` while the defect was `pool: None` in `run.rs`, and restoring the pre-repair state leaves the whole suite green; (3) "no driver fixture can reach the arm", given as the structural reason a source census was necessary — `the_retaining_incarnation_retries_in_place` reaches it; (4) `AttemptPlans::pool_for` said to give the pool rule "one production implementation" — `capacity::pool_for` has three call sites in `assembly.rs`; (5) the ending witness said to cover "**every** arm" — three of six; (6) the pre-clean repair presented as complete — one of its two callers; (7) the packet-clause census said to have "would have caught… `Spend::replay`" — not among its eleven entries; (8) a fixture said to make two behaviours "not both pass" — its implementer and reviewer share `AGENT`, so both pass, and the mutation measured as killed died for the wrong reason

## What the change that takes this up should do

Owner, as the ledger records it: project owner — **the claims protocol a fresh session carries**.

**Recorded as a ledger correction, not repaired by history surgery.** The commit messages are pushed history and the owner's instruction is that they are corrected here, citing the table, exactly as `80a141b`'s false refutation was. The full table with per-claim citations is `~/tactus-artifacts/pr7/s5/r4/FALSIFICATION-TABLE.md`; the raw lens outputs are beside it. **Three confirmed code defects accompanied the claims and were open at the time of that review** (all three are overtaken at this head; the dated triage note below checks each), as round 4 recorded them: `expected_refs`'s census entry is satisfied by a substring collision (all four `expected_refs(` matches in `workspace_manager.rs` are `refuse_unexpected_refs(`; genuine calls zero); the pre-clean fix is half-applied, leaving the stranger-killing path live at `census/tests.rs:3645`; and `an_ending_run_offers_no_work_from_any_arm` covers three of six arms with `Integrate` in the gap. **What is not in doubt**: rounds 1-3 closed real defects — the E6 promotion stall, a resumed run that forgot its spend, and a path traversal from plan-authored input where the legacy engine sanitised and the extraction did not — and those repairs are behaviourally sound. Round 4 challenged the *claims about* several witnesses, not the fixes beneath them. **The pattern, stated once**: prose asserted at the moment of writing became the evidence for the work it described, and nothing earlier in the chain checks a claim made in a commit message — which is the artifact a reviewer trusts most. **The table itself is now in this file, verbatim, as §19**, with each of the eight disproofs re-run at `cca1276` and its command recorded beside its result — including one place the table over-reached, corrected there under the same rule

**Triage, 2026-09-10 — why this row reserves no site, and what became of its three code defects.**

`location:` is empty because the row's subject is a **protocol**. Its guard is "the claims protocol a fresh session carries", and no document in this tree carries such a protocol — `MAINTAINING.md` is where one would most plausibly land, which is a suggestion for the owner and not a location. The `location: attempt.rs` this row carried until `c5b0714c` was never its site: it is a fragment of claim (2)'s *disproof* — "it inspects `attempt.rs`/`settle.rs` while the defect was `pool: None` in `run.rs`" — transcribed into the field as though it named one — and `git ls-files --error-unmatch attempt.rs` exits 1, so it was not a tracked path either. The eight false claims are in pushed commit messages and are corrected in `reviews/FINDINGS.md` §19 rather than by history surgery, as this row itself directs; a commit message has no repository site either.

**The three code defects named above are all overtaken at this head**, which is why the empty field is not concealing three unreserved code sites. Each was checked on its own, by reading the tree at `e8fcbdd8`:

- `expected_refs`'s census entry, said to be satisfied by a substring collision with zero genuine calls: `census_domain::production_calls` now requires the byte before the needle to be a non-identifier byte (`src/effects.rs:869`), so `refuse_unexpected_refs(` no longer satisfies an `expected_refs` entry, and a definition no longer counts as a call (`:875`). `a_call_census_needle_is_not_satisfied_by_a_longer_name_ending_in_it` (`src/engine/topology/recover/tests.rs:10280`) pins both directions and asserts the production region of `workspace_manager.rs` still contains the colliding substring, so its zero proves something (`:10327`). Genuine callers exist at `src/engine/topology/recover.rs:992` and `src/engine/topology/candidate/tests.rs:878`, `:1140`.
- the half-applied pre-clean, "leaving the stranger-killing path live at `census/tests.rs:3645`": that line is now inside an owner-settlement cell table (`src/runner/container/census/tests.rs:3645`), and all three pre-clean call sites — `census/tests.rs:2764`, `container/tests.rs:409`, `exec/tests.rs:4085` — route through `fake::preclean_names`, which refuses any name whose repo-key component is not this build slot's before it reclaims anything (`src/runner/container/fake.rs:521`).
- `an_ending_run_offers_no_work_from_any_arm`, said to cover "three of six arms with `Integrate` in the gap": it drives seven arms at `src/engine/topology/select/tests.rs:1012`, `Integrate` among them (`:1027`), and the sibling test above it, `every_label_the_arm_classifier_returns_is_classified` (`:943`), asserts that every label `arm_label` can return is named by one of the two coverage lists, so a new `Step` variant cannot be added and left undriven.

Executed, not asserted: `cargo test --all-targets --all-features` filtered to those three test names, through `upstroke-build` after `cargo clean -p upstroke`, **rc=0**, `3 passed; 0 failed; 2484 filtered out`, attributed to this worktree by `Compiling upstroke v0.1.0 (/srv/worktrees/fsweep-triage)`.

**The disposition is unchanged.** Whether a row whose three code defects are all overtaken is still open is a disposition question and not a triage pass's to decide; what a triage pass can do is stop the row reading as though three code sites were open and unreserved while its `location:` reserves none. The same per-defect check is in `~/findings-sweep/TRIAGE-NOTES.md`, for the implementer who reads that instead.

Carried in `reviews/FINDINGS.md` §2, “Open — carried deliberately, with an owner”, and confirmed still carried by the full-ledger audit of 2026-08-31 (§39). The row carried no severity label; **P2** here is this migration's judgement from the consequence described above, not the reviewer's own word.
