---
id: SENTINEL-WRITE-UNCHECKED
severity: P1
disposition: deferred
category: security-trust
pr: 251
reviewed_sha: 546cee9b612e8fc6351cebfc257679d72e5be10f
location: .github/scripts/validate-pr-branch.sh:552
provenance: fix_regression
first_bad: 546cee9b612e8fc6351cebfc257679d72e5be10f
guard: project owner
---

## Failure sequence

The capture primitive writes a sentinel byte to its stdout and stderr copies so a truncated capture
can be told from a complete one. **Neither sentinel write's status is checked.**

Reproduced (executed) by the `gpt-6-astra` max pass on
`546cee9b612e8fc6351cebfc257679d72e5be10f`: supply a file containing
`P2_correctness_202609110001_never-filed.md` followed by byte `0x01`, with no trailing newline.
Control: **exit 1, `names no finding`**. Then fault-inject failure of only the **stdout sentinel
write** after the copy — the real builtin `printf` returned **exit 1, `Bad file descriptor`**,
captured separately — and the validator returned **exit 0, `conforms`** with empty stderr.

The mechanism is precise and worth stating: the input's own trailing `0x01` is mistaken for the
marker the helper failed to write, and is then stripped as if it were that marker. A filename that
was never filed appears, and it matches. So a failed write does not merely lose data — it
**manufactures** a finding.

The capture primitive introduced at this head is the right shape; this was its one remaining
unchecked step.

## Why this does not block the merge

Owner ruling, 2026-09-11: *a P1 blocks a merge only if it can happen in normal use, or someone
without push access can trigger it.*

Reaching it requires injecting a write failure into a specific `printf` inside the helper, against a
file descriptor the script owns. Nothing a pull request contains can cause that, and no account
without push access can arrange it. In normal use the sentinel writes succeed or the run dies for
the same reason they failed.

## What the change that takes this up should do

Owner, as the ledger records it: project owner.

Check both sentinel writes explicitly and refuse on either failing. The rule the capture primitive
already states — every status consumed — has to include the statuses of the primitive's own writes,
not only those of the producer it wraps.
