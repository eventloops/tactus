---
id: SHAPE-GATE-MISSES-ARITHMETIC-SUBSTITUTION
severity: P1
disposition: deferred
category: correctness
pr: 251
reviewed_sha: 546cee9b612e8fc6351cebfc257679d72e5be10f
location: .github/scripts/test-pr-policy.sh:2979
provenance: fix_regression
first_bad: 546cee9b612e8fc6351cebfc257679d72e5be10f
guard: project owner
---

## Failure sequence

The shape gate is meant to keep every external call inside the audited region, so that reviewing that
region is enough. Its arithmetic-removal rule deletes `$(( … ))` spans **before** command scanning,
which also deletes any command substitution nested inside them.

Reproduced (executed) by the `gpt-6-astra` max pass on
`546cee9b612e8fc6351cebfc257679d72e5be10f`. Appending this below the audited region:

```bash
unchecked_git() { : $((1 + $(git rev-parse --is-inside-work-tree >/dev/null; echo 1))); }
unchecked_git
```

Bash tracing confirmed `git` **executed**; the function returned exit 0; the scanner reported
**no violations**; and the complete fixture suite with the live mutation returned **exit 0, no
skips**. This is neither of the two residuals the gate documents (a command word entirely inside
quotes, and `eval` of a string the scanner cannot see) — it is a third route, found by the reviewer
rather than listed by the author, which is the point.

## Why this does not block the merge

Owner ruling, 2026-09-11: *a P1 blocks a merge only if it can happen in normal use, or someone
without push access can trigger it.*

Exercising this requires adding code to `.github/scripts/validate-pr-branch.sh` in the repository,
which requires push access. It changes nothing about how the validator judges any pull request; it
weakens an argument about how much of the validator a reviewer must read.

## What the change that takes this up should do

Owner, as the ledger records it: project owner.

Inspect substitutions **before** discarding arithmetic, which closes this route. But the durable fix
is to stop underwriting the scanner: it has now been broken in three consecutive rounds, each time by
a construct nobody had listed, and a text scan over one file cannot bound what that file does.
Describe it in the body, in `MAINTAINING.md` and in the gate's own comments as a **best-effort lint**
that catches the mistakes people actually make, and state the guarantee that is real — the audited
region is a stated number of lines and a reviewer reads them. The severity of this finding comes from
the claim of enforcement, not from the scanner.

Round 12 also raised this gate's audited-region cap from 200 to 250 lines (the region is 242, 158 of
them code) rather than fitting under the old cap. That is a gate loosened by the author of the code
it audits, and it wants either a reversal or a stated justification.
