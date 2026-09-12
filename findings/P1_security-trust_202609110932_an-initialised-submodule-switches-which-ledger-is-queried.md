---
id: SUBMODULE-SWITCHES-LEDGER
severity: P1
disposition: deferred
category: security-trust
pr: 251
reviewed_sha: 546cee9b612e8fc6351cebfc257679d72e5be10f
location: .github/scripts/validate-pr-branch.sh:1035
provenance: pre_existing
first_bad:
guard: project owner
---

## Failure sequence

Repository discovery enters an initialised submodule and uses **that submodule's index**, without
first examining the **superproject's** recorded gitlink for the path.

Reproduced (executed) by the `gpt-6-astra` max pass on
`546cee9b612e8fc6351cebfc257679d72e5be10f`: commit an initialised submodule at `findings`,
with a finding-shaped regular file at the submodule's root. The superproject records
`findings` as mode **`160000`**, and `git status --porcelain` returns exit 0 with empty
output — the checkout is clean.

- building the three tree listings: **exit 0**, and validating them: **exit 1, `names no finding`**
- passing the same checkout's `findings` **directory**: **exit 0, `conforms`**

So the directory API accepts a finding that is absent from the pull request's own ledger. This
disproves the two-API equivalence on a clean checkout, independently of the staging disagreement
this pull request already discloses.

A `160000` entry is a gitlink, not a directory to descend into. The `120000` case is already handled
by deciding from the recorded type; this is the same rule with one more recorded type in it.

## Why this does not block the merge

Owner ruling, 2026-09-11: *a P1 blocks a merge only if it can happen in normal use, or someone
without push access can trigger it.*

This one deserves care, because the input **is** externally controllable: a contributor with no push
access can open a pull request from a fork that commits a submodule at `findings`.

It still does not block, for a reason that is about the caller rather than the attacker:
**`.github/workflows/pr-policy.yml` never passes the directory form.** It builds the three tree
listings in `$RUNNER_TEMP` and validates those, and on this exact fixture the listings return
**exit 1, `names no finding`** — the correct answer. The directory form is a documented entry point
for someone running the validator by hand against a working tree, and reaching the defect requires
that person to run it against a checkout of the attacker's branch. That is neither normal use of the
gate nor something the attacker can trigger through CI.

## What the change that takes this up should do

Owner, as the ledger records it: project owner.

Decide from the superproject's recorded type before traversing: a `160000` gitlink at the requested
path, or at any recorded ancestor of it, must refuse rather than resolve inside the submodule. Then
sweep the recorded types as a set — `100644`, `100755`, `120000`, `160000` — and say for each what
the directory form does and what the listings do, because this is the third recorded type in a row
to produce a disagreement, after the ancestor symlink and the materialised symlink.
