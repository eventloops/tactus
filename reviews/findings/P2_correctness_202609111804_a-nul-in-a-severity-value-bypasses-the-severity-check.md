---
id: PR269-R2-NUL-IN-SEVERITY
severity: P2
disposition: deferred
category: correctness
pr: 269
reviewed_sha: 08c80be61e56e98700fb34ccfae8ed8a5bc30e28
location: .github/scripts/changed-in-range.sh:188
provenance: introduced_by_feature
first_bad: 08c80be61e56e98700fb34ccfae8ed8a5bc30e28
guard: gpt-6-astra/max review round 2 of PR #269
---

## Failure sequence

Commit a correctly named finding under `reviews/findings/` whose frontmatter severity line holds a
NUL byte, `severity: P<NUL>3`. `severity_of` reads the value through a Bash command substitution,
and **command substitution deletes NUL bytes** — bash says so on stderr and carries on:

```
warning: command substitution: ignored null byte in input
```

The value that reaches the record is therefore `P3`, the builder writes
`P3<TAB>reviews/findings/P3_..._nul.md` and exits **0**, and the validator exits **0** with all six
workflow inputs supplied. The file's actual frontmatter severity is `P<NUL>3`, which is outside
P0-P3, and the severity rule this pull request adds is the rule that was supposed to refuse it.

Re-executed by the branch-policy orchestrator at the reviewed SHA before this finding was filed:
builder rc=0 with the warning above, record `P3<TAB><path>`, validator rc=0.

This is the **same class** as `PR269-001` of round 1, where a literal tab in the same value forged
the tab-separated record. That one was repaired by refusing tab, LF and CR before the value is
written. NUL is the member of that set that Bash removes before the guard can see it, so the guard
has nothing to reject.

## What the change that takes this up should do

Reject a NUL **before the value enters a Bash variable** — test the bytes as `git cat-file` emits
them rather than after substitution has silently rewritten them. The refusal should name the path
and say a NUL was found, like the tab, LF and CR refusals beside it.
