---
id: PR262-LOCATION-COMPLETENESS-UNPROVABLE
severity: P2
disposition: accepted-risk
category: docs-contract
pr: 262
reviewed_sha: a0f936c753973edd7d00cd99a0f64df50bb57207
location: reviews/findings/README.md:47
provenance: introduced_by_feature
first_bad: PR262-LOCATION-NAMES-THE-EXAMPLE-NOT-THE-REPAIR
guard: the findings-sweep owner — whoever schedules against this directory
---

## Failure sequence

`findings-sweep-schedule.py` reads a finding's `location:` as **the whole set of modules a repair of that finding will write**: `module_of` derives one lane per path, and `allocate` treats two findings with disjoint lanes as safe to run at once. A `location:` is set by reading the row and the code at the path — and **that procedure cannot establish the property the scheduler relies on.** A row that names too few modules is admitted beside, or batched with, a finding holding the module it omitted; both workers write that module; separate hunks in one file cherry-pick cleanly (`rc=0` and `rc=0`, measured), so assembly does not catch it and nothing announces the collision.

**Eight instances on PR #262, and the reviews of four consecutive heads each found at least one that inspection had already passed.** Rounds 3, 4 and 5 are the three consecutive rounds the last repair pass was asked to record; round 2 makes it four.

| round, reviewed head | row | modules the `location:` omitted | found by |
|---|---|---|---|
| 2 `c5b0714c` | `PR73-LEXICAL-CLOSURE-001` | `src/runner/container`, `src/effects` | review |
| 3 `da014f8a` | `PR5-R2-OBJECT-GROUP-TAKES-NO-SITE` | `src/engine/topology` | review |
| 3 `da014f8a` | `PR7-R3-ATTEMPT-003-RESIDUE-DISCARD-UNREACHED` | `src/engine/topology` | review |
| — | `PR5-R2-LEGACY-ENGINE-APPEND-FAILURE` | `src/engine/resume` | generated text scan |
| — | `PR5-C-LEGACY-APPEND-ERROR-CENSUS` | `src/engine/resume` | generated text scan |
| 4 `a0561268` | `PR7-WRAPPERS-EMPTY-DOMAIN` | `effects/wrappers.toml` | review, by compiled probe |
| 4 `a0561268` | `PR3-REPORT-DOUBLE-NAME` | `src/effects`, `effect_sites.json` | review |
| 5 `a0f936c7` | `PR3-REPORT-DOUBLE-NAME` | `effects/funnel-modules.json` | review |

Each of the four rounds was preceded by a sweep of every location against a stated bar, and each sweep passed the row the next round disproved. The bar tightened every time — the row's own text names a concrete artifact there (round 3), a generated worklist of every module the row's text points at (round 4), a compiled probe applying the indicated repair (round 5) — and only the last class is evidence of completeness. It was run for two findings out of 46, because it costs a compile per finding.

**The property this establishes is a limit, not a backlog: a `location:` cannot be proven complete by inspection. For some findings the full write set is discoverable only by attempting the repair.** `PR7-WRAPPERS-EMPTY-DOMAIN` reads as one function in one file; the probe that applied its repair took the discovered names from zero to 11, 1 and 10 across three modules and made `effects/wrappers.toml` a mandatory write site. `PR3-REPORT-DOUBLE-NAME` reads as one enum entry; retiring `Report.Write` moves a census, a checked-in inventory and a recorded site count, in three files, found across two rounds.

## What the change that takes this up should do

Owner, as the ledger records it: the findings-sweep owner — whoever schedules against this directory.

**Record the limit where it is read, and stop the sweep's guarantees resting on completeness.** The in-tree half is `reviews/findings/README.md:47`, which describes `location:` and says nothing about what it can be trusted to mean; the other half is the sweep's own `PROCESS.md`, which this tree does not carry (it is corrected separately in #261) — so a repair of this row writes the README and the out-of-tree document together, and this `location:` reserves the half that is here. That is the same shape as `PR5D-TOOLBOX-DISCARDS-CLIPPY-OUTPUT`, whose site is wholly outside the tree.

Three candidate directions, none of them costed here:

- **Require a probe where the repair is not a single edit.** The only procedure that has produced evidence of completeness is compiling the hypothetical repair. Requiring it for every row costs a compile per finding, which is why it was run twice; requiring it for the rows whose own text describes a census, a manifest, a generated artifact or "every caller" is cheaper and catches the shape that has actually failed six times.
- **Stop depending on completeness.** Exclusion that survives an incomplete reservation is a scheduler-side property — a conflict check against what a worker actually wrote, rather than against what it declared before writing. `PR262-WIDENED-RESERVATION-NEVER-REACHES-THE-SCHEDULER` is the concrete gap on that path.
- **Prefer over-reservation.** Under-reservation has cost eight review findings on this pull request and over-reservation has cost none. A lane held for work nobody is doing costs parallelism; a lane not held costs two workers in one file with nothing announcing it.

**What must not happen is the sweep continuing to read a `location:` as a proof.** It is a best-effort reservation, and this row exists so that whoever schedules against this directory sees that before the first batch, not after it.

Recorded 2026-09-10 on `docs/findings-triage-locations`, at the request of the round-5 repair brief. Severity **P2** is this pass's judgement from the consequence above — two repairs in one module, silently — and not a reviewer's word; the reviews that produced the eight instances labelled each instance P2.
