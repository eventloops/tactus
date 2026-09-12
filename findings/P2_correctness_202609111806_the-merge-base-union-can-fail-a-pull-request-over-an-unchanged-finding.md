---
id: PR269-R2-UNION-SCOPE-FALSE-REFUSAL
severity: P2
disposition: deferred
category: correctness
pr: 269
reviewed_sha: 08c80be61e56e98700fb34ccfae8ed8a5bc30e28
location: .github/scripts/changed-in-range.sh:150
provenance: introduced_by_feature
first_bad: 08c80be61e56e98700fb34ccfae8ed8a5bc30e28
guard: gpt-6-astra/max review round 2 of PR #269
---

## Failure sequence

`changed-in-range.sh` unions its results over **every** merge base, the same conservative choice
`findings-in-range.sh` makes for the finding listings. For the *candidate* listings that union is
safe, because a wider candidate set can only make an ambiguous name more ambiguous. For the
*changed-path* and *added-finding* listings it is not, because those decide a **refusal**.

Criss-crossed history reaches it. Sibling commits A and B add, respectively, an old `P9_`-named
finding and a source file. `master` merges A+B; another branch independently merges B+A into the
byte-identical tree and then adds one valid finding. The two merge bases are A and B. The union
therefore lists the **old `P9_` as added** and the **inherited source file as changed**, and the
complete workflow validation exits **1** over a finding that is unchanged and already on `master`,
although target and head differ only by the one valid new finding.

This is a **false refusal of a legitimate pull request**, which `validate-pr-branch.sh`'s own
header calls the expensive failure of this rule. It also falsifies a sentence in PR #269's body:
"nothing already on master can trigger it".

## What the change that takes this up should do

Give the two refusal-bearing listings an explicit scope rule rather than inheriting the candidate
listings' union. Intersecting over the merge bases, or computing against a single chosen base and
saying which, are both defensible; what is not defensible is a union whose only documented
justification is the one that applies to the candidate set. Whatever is chosen, state it where
`findings-in-range.sh` states its own argument, and fixture the criss-cross case.
