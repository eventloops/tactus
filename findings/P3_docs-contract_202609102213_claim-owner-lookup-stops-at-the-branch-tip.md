---
id: PR261-CLAIM-OWNER-LOOKUP-STOPS-AT-THE-TIP
severity: P3
disposition: deferred
category: docs-contract
pr: 261
reviewed_sha: 57bbbc51147aed90e19a6ee2a8532dc4854e491e
location: findings/PROCESS.md:119
provenance: introduced_by_feature
first_bad: c5be6877
guard: the change that next edits §2 of `findings/PROCESS.md`
---

## Failure sequence

§2 makes the claim commit's message the record of who holds a claim, and names one lookup for it:
"that message is also the record of who holds the claim: `git log -1 <ref>` answers it, and nothing
else on the box does." That lookup queries the branch **tip**, and the tip stops being the claim
commit as soon as the claiming session does any work. From the first fix commit onward the claim is
one commit behind the tip and the lookup returns the fix commit instead, so the claiming session is
invisible for the whole of the window the board (`git ls-remote --heads origin 'fix-*'`) is meant to
cover — a claim is legible only before its holder starts.

The tip's author identity does not substitute for it. §1 resolves every Claude seat in the roster —
implementer and repair alike — to the `cameron` account, which is the reason §2 puts the session
name in the claim message in the first place.

Reproduced in disposable repositories on the build box, exit codes captured directly rather than
through a pipe, using §2's own `commit-tree` claim and an ordinary fix commit carrying a `Finding:`
trailer:

```
push the claim commit                              exit 0
git log -1 <ref>, before any work                  exit 0   claim: session-A 2026-09-10T22:00:00Z
push one ordinary fix commit onto the branch       exit 0
git log -1 <ref>, the documented lookup            exit 0   fix: correct fixture
                                                            Finding: PRX-FIXTURE
  does that output name the claiming session?               no
  the tip's author identity, which is what is left          cameron <cameron@example.invalid>
the claim commit itself                                     still present, one commit behind the tip
```

The exit code is 0 throughout: nothing reports that the answer is no longer the claim. It is the
`codex login status` shape `CLAUDE.md` records under *Traps that have already cost time* — a command
that exits 0 while answering a different question from the one asked.

Two things narrow the window in which the claim commit can be recovered at all, and both are
deliberate elsewhere in the document: assembly cherry-picks `"$CLAIM..$TIP"` (§2), so the claim
commit is excluded from the batch and never lands on `master`; and §8 deletes every member fix
branch after the merge. After that the claim commit is unreachable, and the record of who held the
claim is gone with it.

## What the change that takes this up should do

Replace the tip lookup in §2 with one that names the claim commit itself, and state it as a command
that can be run rather than as a property of `<ref>`. Both of these were executed against the
reproduction above, after the fix commit had landed, and both returned the claim commit:

```bash
# structural: the first commit on the branch after the base it was cut from
git log --reverse --format='%h %s' "$BASE..<ref>" | head -1

# no pipe, but it trusts the message
git log --format='%h %s' --grep='^claim: ' "$BASE..<ref>"
```

Each has a caveat worth writing down beside it rather than discovering later. The first ends in a
pipe, so `$?` is `head`'s and not `git log`'s — capture the status before the pipe if a caller acts
on it. The second matches on the message, so a fix commit that quotes a claim line in its own body
matches too, which is the same mistake §8 already had to correct in its attribution query when
`--grep` selected a commit that merely quoted another finding's trailer.

Neither survives §8's branch deletion. The durable form is to retain the claim SHA at the moment of
the push — that is the only record that still answers once the branch is gone, and it is what the
review that filed this suggested. Whichever is chosen, §2 should say the lookup is a claim-commit
lookup and not a branch-tip one, and should not offer the tip's author identity as a fallback while
§1 resolves every Claude seat to one account.

Recording only. `PROCESS.md` is not edited by the pull request that filed this, so that a push
confined to `findings/` keeps its frontier review under `MAINTAINING.md`.
