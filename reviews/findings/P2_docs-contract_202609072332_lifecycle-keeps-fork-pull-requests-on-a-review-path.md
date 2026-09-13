---
id: PR155-LIFECYCLE-FORK-PR-STILL-ON-A-REVIEW-PATH
severity: P2
disposition: deferred
category: docs-contract
pr: 155
reviewed_sha: 4af6ff026d2cfa172d7e55f15c9055da7b4f7d70
location: MAINTAINING.md:215
provenance: introduced_by_feature
first_bad: 4af6ff026d2cfa172d7e55f15c9055da7b4f7d70
guard: the owner, in `MAINTAINING.md`; the closure rule for fork pull requests is a lifecycle decision, and this pull request is confined to `CONTRIBUTING.md` and `README.md`
---

## Failure sequence

`CONTRIBUTING.md:6` now says a pull request from outside the project "will be closed without
review". `MAINTAINING.md:215-216` still says fork pull requests are provisional, and that the whole
diff including workflow edits is reviewed before merge. `MAINTAINING.md` is the authoritative change
lifecycle, so the contributor guide cannot override it; the repository states two rules and does not
say which wins.

An outside author opens a fork pull request. The contributor guide requires closure without review.
The maintainer follows the authoritative lifecycle instead and treats it as a provisional merge
candidate needing a full-diff review. Nothing in the repository settles whether review may begin,
and the answer differs by which document the reader reached first.

This pull request's body claimed in its Scope section that every excluded fork sentence merely
describes repository settings. That is true of `MAINTAINING.md:192-213`, which is the
workflow-approval setting and stays in force either way. It is not true of lines 215-216, which
describe how a fork pull request proceeds. The Scope section is corrected in the body of this pull
request; the underlying contradiction is not, and is what this row holds open.

Two sentences in `CONTRIBUTING.md` were left conditional on purpose and are not part of this
finding. The `DESIGN.md` §21 clause says when reopening will be reconsidered, and the licence terms
now say plainly that they bind whatever route a contribution arrives by and whether or not anyone
reads it, which is the P1 repair carried on this same branch. Neither leaves a reader guessing about the current rule.

## What the change that takes this up should do

Decide, as the owner, which rule governs a fork pull request opened today, then make
`MAINTAINING.md` say it once. Two shapes are consistent:

* Closure is the current rule. `MAINTAINING.md:215-216` states the closure rule as the lifecycle's
  own, and any future fork review path is written as explicitly conditional on contributions
  reopening. The workflow-approval setting at lines 192-213 is kept as defence in depth regardless,
  and should say that it is defence in depth rather than an expectation of fork traffic.
* Fork pull requests remain reviewable at the owner's discretion. Then `CONTRIBUTING.md:6` overstates
  and must be softened; "will be closed without review" becomes a statement of the default, not an
  absolute.

Whichever is chosen, the two documents change in the same pull request. Note that `src/export.rs`
embeds `MAINTAINING.md` through `include_str!`, so the Rust suite must run on any edit to it.
