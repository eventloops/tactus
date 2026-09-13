---
id: PR274-REVIEWER-NAMED-IN-THE-CONTRACT-IS-NOT-THE-ONE-THAT-RUNS
severity: P2
disposition: deferred
category: docs-contract
pr: 274
reviewed_sha: 0d5a72463c428403d77563e19e02183bef8da207
location: MAINTAINING.md:25
provenance: pre_existing
first_bad: 04d407352307c6b6c7f8dfcac11b1463aef3e026
guard: the owner's ruling on which model holds the reviewer role, applied to all four files at once
---

## Failure sequence

**The tree names two different frontier reviewers, and the one three of the four files name is not
the one that executes.** Measured at `0d5a72463c428403d77563e19e02183bef8da207`:

| file:line | names |
|---|---|
| `MAINTAINING.md:25` | "an independent frontier-class reviewer at `max` effort — today **`gpt-5.6-sol`** through `codex exec`" |
| `AGENTS.md:96` | "(**`gpt-5.6-sol`** at `max`, the verdict posted to the PR as one SHA-bound comment)" |
| `CLAUDE.md:96` | the same sentence, these two files being kept in lockstep |
| `findings/PROCESS.md:28` | \| **Reviewer** \| **`gpt-6-astra`** \| `max` \| `codex` \| OpenAI \| |

What runs is `gpt-6-astra`. The review poller invokes it by name —
`REVIEW_MODEL=gpt-6-astra REVIEW_EFFORT="$REV_EFFORT"` — and every lens log of this programme opens
with `running codex (gpt-6-astra, max effort)`, including both round-1 lenses of this pull request.

The consequence is in the auditable record, not in the code. A pull request body's **Frontier
reviewer model and effort** field is where the review's provenance is preserved after the branch is
deleted; `MAINTAINING.md` §step 4 is what an author copies it from; so the field gets written
`gpt-5.6-sol` and records a reviewer that did not run.

**It has been written wrong three times and corrected twice, which is what makes it a process
defect rather than a typo.** Directly measured: #265's and #272's bodies read `gpt-6-astra` today,
each after a round-2 session caught the field, and #274's read `gpt-5.6-sol` at
`0d5a72463c428403d77563e19e02183bef8da207` — through its whole round-1 review. The sweep
orchestrator additionally records correcting #274's field to `gpt-6-astra` at 08:07 on 2026-09-12
and a later body rewrite losing the correction; GitHub exposes no body-edit history through the API,
so that step is its record and not a measurement of mine. A defect whose fix does not stick is a
defect in the source it is copied from.

The two authorities disagreed from `04d40735` (2026-09-10), which introduced `PROCESS.md`'s role
table naming `gpt-6-astra` beside three files that already said `gpt-5.6-sol` and were not updated
with it.

## What the change that takes this up should do

**The decision is the owner's, and it is a decision, not an edit**: either `gpt-6-astra` holds the
reviewer role and `MAINTAINING.md:25`, `AGENTS.md:96` and `CLAUDE.md:96` are wrong, or `gpt-5.6-sol`
does and the poller is running the wrong model. Nothing in the tree settles which, and this finding
deliberately does not choose — guessing would put the one authoritative record of who reviews this
project's changes on a coin flip.

Once the owner rules, change all four sites in one pull request; a fix that reaches three of them
recreates the disagreement in the other direction. Consider naming the reviewer in exactly one place
and having the others point at it, since the three-way copy is what let the files drift apart in the
first place, and consider whether `validate-pr-body.sh` should check the field against that one
place — nothing checks it today, which is why three wrong bodies reached review.

**Not merge-blocking for #274:** it is a pre-existing contract defect this pull request neither
introduced nor touches, and reconciling it means editing the review contract, which is a decision
outside a prose-only change to merge authorisation. #274's own field is corrected to `gpt-6-astra`,
the model that actually reviewed it.
