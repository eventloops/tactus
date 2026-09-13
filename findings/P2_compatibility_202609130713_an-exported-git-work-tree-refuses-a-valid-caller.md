---
id: PR280-ENV-EXPORTED-WORK-TREE-REFUSES-A-VALID-CALLER
severity: P2
disposition: deferred
category: compatibility
pr: 280
reviewed_sha: 1cbb0a66dc770260d278b653dbda187bf2e951f6
location: .github/scripts/validate-pr-branch.sh:1191
provenance: fix_regression
first_bad: 339a238b
guard: a change that gives parent discovery a reading of an explicitly selected work tree
---

## Failure sequence

Run the validator with `GIT_WORK_TREE` exported, on a clean standalone repository holding one
committed finding at its root, asking about its own directory:

```bash
cd <repo> && GIT_WORK_TREE="$PWD" bash .github/scripts/validate-pr-branch.sh \
  fix-P2/correctness_explicit-work-tree .
```

The listing is that work tree's own root, so its name in its own index is empty and the gitlink
ascent runs. `enclosing_work_tree` asks git whether the PARENT directory is inside a work tree;
with the work tree pinned by the environment and no `.git` above the parent, discovery exits 128.
`repository_above` is then asked whether there was a repository for git to have failed about, and
its first line returns 0 -- yes -- for any `GIT_DIR` or `GIT_WORK_TREE` in the environment. The
ascent reads that as a repository above, and refuses: exit 1, `git could not say whether '<parent>'
is inside a work tree, and there is a repository at it or above it`.

That reading is right for the caller it was written for and wrong here. `locate_listing` asks
`repository_above` whether git's failure AT THE LISTING may be read as an absence, and a pinned
`GIT_DIR` is an index it cannot reach, so it must not fall back to the filesystem.
`enclosing_work_tree` asks a different question -- is there a work tree CONTAINING this one -- and
a work tree the caller selected is not a container of itself.

Measured on this box, one revision at a time, with the reproduction above:

| revision | result |
|---|---|
| `231c1aad` (master) | exit 0, `conforms` |
| `04432736` | exit 0, `conforms` |
| `5917a4d9` | exit 0, `conforms` |
| `339a238b` | **exit 1** |
| `88d2a801` | **exit 1** |
| `1cbb0a66` | **exit 1** |

So the first bad commit is `339a238b`, the commit that made the ascent repeat rather than stop at
the first repository that disclaims a parent. `04432736`, which introduced the ascent, is not
implicated: it is measured above at exit 0. The reviewer's own bracket named `231c1aad` and
`5917a4d9` good and `339a238b` bad and did not test `04432736`; this row closes that gap.

`GIT_DIR="$PWD/.git"` alone is exit 0 on every revision above, because discovery from the parent
then succeeds and the 128 that starts this never happens. It is `GIT_WORK_TREE` that reaches it.

The reviewer's reproduction ran at `/tmp/tmp.8CtEdANROb/review-execution/env-regression/reproduce.sh`.

## What the change that takes this up should do

Give parent discovery a reading of an explicitly selected work tree that is distinct from
"a repository above this one". Two shapes were considered and neither is a one-line correction,
which is why this is filed rather than fixed in `#280`:

- **End the ascent at entry when the environment pins the repository.** `enclosing_work_tree`
  returns no enclosing root, and the pinned repository's own index answers, exactly as it did
  before any ascent existed. Simple and restores `231c1aad`, but it drops the gitlink protection
  for every pinned caller, so it is a deliberate narrowing rather than a repair.
- **Let the containment caller ignore the pin and keep walking the filesystem.** More precise -- a
  real unreadable `.git` above would still refuse -- but with the work tree pinned, every discovery
  answer along the ascent is about the pinned repository rather than about what contains the
  listing, so the walk's later answers need a reading too, not just this one.

Whichever is chosen, it wants a fixture with `GIT_WORK_TREE` exported and one with `GIT_DIR`
exported, because only the first reaches the defect and a fixture for the second would pass
against the unrepaired code.
