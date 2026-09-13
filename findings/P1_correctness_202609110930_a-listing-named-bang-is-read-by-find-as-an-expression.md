---
id: FIND-BANG-PATH-IS-EXPRESSION
severity: P1
disposition: deferred
category: correctness
pr: 251
reviewed_sha: 546cee9b612e8fc6351cebfc257679d72e5be10f
location: .github/scripts/validate-pr-branch.sh:678
provenance: fix_regression
first_bad: 546cee9b612e8fc6351cebfc257679d72e5be10f
guard: project owner
---

## Failure sequence

`list_dir` hands a relative starting path straight to `find`, and `find` reads a path named `!` as
its own negation operator rather than as a directory.

Reproduced (executed) by the `gpt-6-astra` max pass on
`546cee9b612e8fc6351cebfc257679d72e5be10f`: outside any repository, one matching finding name in
`twin-a.txt` and a distinct same-description twin inside a directory named `!`. Run the validator
with `twin-a.txt '!'`.

- parent revision: **exit 1, `names 2 findings`**
- this head: **exit 0, `conforms`**
- passing the directory's absolute path: **exit 1** again

`find ! -mindepth 1 -maxdepth 1 -print0` returns **exit 0 with empty output** — the `!` negated the
expression instead of naming anything. The empty enumeration then reads as "this directory names no
finding", so an ambiguous branch name is accepted.

This is a loosening introduced by the repair at this head, and it was not disclosed when it was
made.

## Why this does not block the merge

Owner ruling, 2026-09-11: *a P1 blocks a merge only if it can happen in normal use, or someone
without push access can trigger it.*

The starting paths are chosen by the caller. In CI the workflow builds the three listings itself
under `$RUNNER_TEMP/findings` with fixed names and passes those; nothing a pull request contains can
make the gate enumerate a directory called `!`. A person without push access has no way to choose
the argument.

## What the change that takes this up should do

Owner, as the ledger records it: project owner.

Prefix every relative starting path with `./` inside `list_dir`, so a path is always a path. Then
sweep the same class for every other externally-supplied value handed to a tool that has its own
argument grammar — a leading `-` is the obvious sibling, and `--` alone does not save `find`, whose
expression grammar begins before the operand list.

## Related

[[SENTINEL-WRITE-UNCHECKED]] is the other half of the capture path reviewed at this head.
