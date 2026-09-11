# Maintaining upstroke

The operating contract for changes to the protected default branch, `master`. Repository rules
enforce the mechanical parts; the owner is responsible for the review evidence. It applies to
source, documentation, workflows, release machinery and this file.

## How a change lands

1. **Branch from current `master`** and keep the change to one coherent, independently revertible
   outcome. Conventional Commit title: `type(optional-scope): summary`.
2. **Open a draft pull request early.** The body has six sections — Summary, Scope, Validation,
   Review evidence, Risk and rollback, Review finding ledger — and
   `.github/scripts/validate-pr-body.sh` rejects anything else. Run it against your body before
   pushing.
3. **Run the ten-command baseline** (`CODING_STANDARDS.md` §2) before every push, then wait for
   the two required contexts: `upstroke-ci` (formatting, Clippy on three platforms, the Linux and macOS
   test matrix, the Windows suite on its self-hosted ephemeral runner `test (winguest)`, the
   MSRV matrix, the six Bash gates) and `upstroke-pr-policy` (title, body sections, ledger). A
   branch behind `master` is not updated by hand to merge once the ruleset carries the merge
   queue (Repository rules): the queue builds the entry on `master`'s head and runs both
   contexts there (step 7), so update it only when the change needs what `master` gained.
   Until the ruleset carries the queue, the up-to-date requirement stands: a branch behind
   `master` is updated first and waits again, and the audit reports it as `behind-master`.
4. **One frontier review pass on the green head.** Give the exact diff and head SHA to an
   independent frontier-class reviewer at `max` effort — today `gpt-5.6-sol` through `codex exec`,
   run by the owner's review driver against the pull request's own base. Allow at least 90 minutes per
   pass and stream the output; a timeout, transport failure or missing verdict is not a pass.
   Record in the body: implementation model and effort, reviewed head SHA, reviewer model and
   effort, transport and wall-clock limit, and a durable link to the verdict as written (the driver
   posts it to the pull request as one SHA-bound comment).
5. **Triage every finding.** The author babysits the pull request until it lands:
   - A finding that is **relevant to the change** and a **serious P1** (below) is fixed, and the
     repaired head gets a fresh pass.
   - A relevant finding that is not serious is fixed at the author's discretion or logged as tech
     debt: a ledger row with a stable id, an honest failure sequence, and disposition `deferred`
     or `accepted-risk`.
   - A finding that is **not relevant to the change** — pre-existing, out of scope, or against an
     unswept file under a transitional standard (`standards/SWEEP.md`) — is logged the same way or
     `rejected` with the reason, and blocks nothing.
   - Two rules outrank the label. A `MUST` deviation in materially touched code is fixed, or the
     standard is amended by reviewed change. A finding carrying a failing test, reproduction or
     mutation witness is fixed whatever its severity. Either may be `rejected` only by a row
     showing the evidence invalid: a `MUST` the code does not breach, a witness that does not
     reproduce on the head.

   **Every open finding gets its own file, and the file is deleted when it is resolved.** One file
   per finding, never one per pull request and never one per review pass: a pass that returns six
   findings produces six files. Severity leads the filename so `reviews/findings/` sorts worst
   first and an `ls` is the outstanding work; name and shape them as `reviews/findings/README.md`
   states. A finding fixed before merge needs no file at all — the body's ledger row is its
   permanent record, and that row is required whatever the disposition. `reviews/FINDINGS.md` is
   the same ledger up to 2026-09-04, closed to new sections; its section numbers are cited from
   source and design and do not move.

   A repair-only push after a pass that found no serious P1 needs no second pass: the owner reads
   `git diff <reviewed> <head>`, confirms it contains those repairs and nothing else — never a
   workflow, gate script or validator edit — and says so in the body. A push confined to
   the finding ledger (`reviews/findings/`, or `reviews/FINDINGS.md`), or a conflict-free merge of `master` that leaves `git diff master...HEAD`
   byte-identical with CI green on the merged head and no gate edited by the pull request, keeps
   the review as well; record both SHAs, and for the merge-in both base SHAs and the diff hash
   before and after. Anything wider is a new change and is reviewed again. A panel-reviewed
   checkpoint candidate is the exception: any head movement re-runs every seat.
6. **Record the pass as written.** A `CHANGES_REQUIRED` whose findings all landed as repairs or
   ledger rows is recorded as that verdict with each disposition, never as a pass. When the merged
   head differs from the reviewed head, list the delta commits and what verified each. Re-run
   `validate-pr-body.sh` from the default branch against the live title and body.
7. **Enqueue for merge** (`gh pr merge --merge --auto`, or the audit's `--enqueue`; both need
   auto-merge enabled on the repository, which the owner turns on together with the queue rule)
   once every conversation is resolved and both contexts are green on the head being merged.
   With the merge-queue rule in the ruleset (Repository rules), the queue builds a merge commit
   of that head onto `master`'s head plus any entry ahead of it, requires both contexts on that
   commit, and lands exactly the commit they passed on; an entry whose contexts fail leaves the
   queue and the pull request says why. Until the rule and the setting exist, the merge is the
   owner's merge commit on a head whose own contexts are green, as before. Enqueueing, or that
   merge, is the owner's attestation that the evidence is real and the
   merged head is accounted for: reviewed directly, or separated from the reviewed SHA only by the
   deltas step 5 allows. The owner may delegate the merge, in writing, to the agent doing the work
   on a pull request that has reached this state; the delegation is disclosed in the body. Never
   push to `master` directly. Delete the branch.

### Serious P1

A finding is a serious P1 when its failure sequence is concrete on the current head and reaches at
least one of:

- a `DESIGN.md` §4 invariant;
- the trust boundary, the merge or release machinery, or a gate change that misstates what the
  gate enforces (`security-trust`);
- durable state: the event log, replay, or anything that makes a recorded run unreproducible or
  corrupt;
- loss or corruption of data in a user repository — the engine owns git;
- a legal or licensing defect.

The reviewer's label does not decide this; the owner classifies, and a P1 whose failure needs
speculative preconditions is reclassified down with a ledger row saying why.

### When a pull request may be looping

Repair rounds are not free. A push waits on both required contexts, and a repaired serious P1 costs
another frontier pass under step 5. They also do not always converge. Two signals say a pull
request may be looping rather than converging, and each obliges the author to say so.

**This subsection never overrides step 5.** Neither signal withdraws a fix, closes a pull request
or decides anything by itself. When one appears, the author writes in the body that it has
appeared, what the evidence is, and why the pull request should continue — or proposes narrowing it
or closing it. Raising a signal is the author's, the moment it appears, and not the reviewer's to
keep finding: step 5 makes the author responsible for the pull request until it lands, and that
responsibility includes saying when it may not. Narrowing is the author's too, and the findings on
a narrowed pull request are triaged under step 5 exactly as before. Closing a pull request that has
already had a review pass is the owner's; the owner may delegate that closure in writing, for that
pull request, to the agent doing the work on it, and the delegation is disclosed with the closure.
This is said here because nothing else in this file assigns it.

**The premise looks disproved.** Every change is made for a stated reason, and a review or a CI run
can put that reason in doubt: the failure it was meant to fix happens again on its own head, the
measurement it rested on does not reproduce, the cause it named looks not to be the cause. Say so,
and say what the evidence does not establish as well as what it does. A message is not a defect and
two faults print one sentence, so a recurrence is worth exactly its fingerprint — the same test
failing the same assertion for the same reason, or the change's own mechanism measured and shown
not to have fired, is evidence; a shared label is not, and neither is one red run of a test already
known to fail under load. Then say which the pull request is: still converging on the defect it
names, narrowed to the part that stands on its own, or finished, with what it learned kept as
findings and the question it was answering re-opened. A narrowed pull request is retitled; step 6
re-validates the live title, and a title still naming a withdrawn fix is the next finding.

**A pass finds a P1 in machinery an earlier round of this pull request added.** Any earlier round,
not only the last one: a defect that takes two passes to surface is the same loop moving more
slowly. Say that too, and say why the next repair converges where the earlier ones did not — what
the defect in the repair was, what the fix is, and what test holds it. An inverted condition with a
regression test is not the same animal as a third round of machinery invented to keep the second
round's machinery safe, and which one it is shows only when someone writes it down. When it is the
second, the smaller change is the one to propose: keep what has survived a pass, drop the machinery
those rounds invented, and record what it was for as a finding carrying its proposal.

Neither signal is a licence to abandon a real defect. A relevant serious P1 is fixed and
re-reviewed under step 5 whatever the signals say, and what a narrowing drops is preserved where
step 5 puts any open finding: one file each under `reviews/findings/`, saying what the change that
takes it up should do. A pull request that does not merge carries nothing into the tree by itself,
so those files land through a change of their own. What a signal costs is a paragraph. A loop
neither signal catches is still a loop, so these are a floor and not a detector.

PR #125 is the pull request this subsection was written from; `reviews/FINDINGS.md` §49 is its
record.

### Tech debt sweeps

Logged rows are swept, not forgotten, at three points: before any release tag or crates.io publish,
where every open `accepted-risk` and `deferred` row is re-triaged and the release notes name what
ships open; at each integration checkpoint merge; and on owner call. A sweep fixes a row, re-accepts
it dated, or converts it to a tracked follow-up; a row re-accepted twice carries the owner's dated
note saying why it stays.

### Review finding ledger

Every actionable finding gets a stable id and one row in the pull request's ledger:

- severity `P0`–`P3`, the full reviewed SHA and `path:line`, and a concrete `A -> B -> failure`
  sequence;
- provenance: `pre_existing`, `introduced_by_feature`, `fix_regression`, or `undetermined`;
- category: `correctness`, `crash-consistency`, `security-trust`, `portability`, `liveness`,
  `performance`, `compatibility`, or `docs-contract`;
- first-bad commit where history can establish it, and any earlier finding id when it recurs;
- the named regression test or documented deterministic guard, and a disposition: `fixed`,
  `rejected`, `deferred`, or `accepted-risk`.

Provenance explains where a defect came from; it does not make it less real. Every code defect
fixed in the pull request gets a regression test that fails on the first-bad shape. Keep fixed and
rejected rows: the ledger preserves why, not only what remains open.

`validate-pr-body.sh` enforces the header and tokens. `validate-pr-ledger-evidence.sh` resolves
each row against the exact head: the reviewed SHA must be an ancestor, the path and line must exist
at that commit, and every backticked regression or guard identifier must occur in tracked content.
Bind rows to the first integrated commit, never to a lane commit that was later cherry-picked.
Change a validator and its fixtures in the same pull request as any schema change.

Slices of a long-running design land as pull requests into their integration branch under the same
steps; the integration branch's own pull request into `master` is reviewed once more, on the head
that merges. There is no integration branch today: the parallel-execution design's
`codex/parallelism-design` was deleted once its slices had landed, so standing another one up means
adding its name to both workflows' branch lists and to the gate that pins them in one change.
Merge commits only, everywhere: a rewrite orphans every ledger row bound to a replaced SHA.

## Repository rules

The default-branch ruleset requires a pull request, `upstroke-ci` and `upstroke-pr-policy` on
the current head, resolved conversations, and merge commits only; it blocks deletion and
non-fast-forward updates and has no bypass actor. The merge-queue contract is this: every merge
goes through the queue, which builds each entry on `master`'s head plus the entries ahead of it,
requires both contexts on that entry, and lands exactly the commit they passed on, so a branch is
never required to be up to date on its own. The queue merges only non-failing entries (the rule's
grouping strategy is all-green, never head-green): `pr-policy.yml` validates the one pull request
an entry's queue ref names, so an entry whose own contexts failed must leave the queue rather
than ride out under a later entry's green. The workflows and the audit below implement that
contract; the ruleset adopts it as the owner's act once the contract is on `master`, by adding
the merge-queue rule with merge method merge and all-green grouping, and removing the up-to-date
requirement the queue makes redundant. Until then `merge_group` never fires, the up-to-date
requirement stands, and step 7 is the owner's merge commit on a head whose own contexts are
green; auto-merge is enabled on the repository in the same change as the queue rule. A tag ruleset on `refs/tags/v*` blocks updates and deletions
with no bypass. Required-check names are API: to rename one, land the replacement, observe it on a
pull request, update the ruleset, then remove the old requirement. The workflow trigger contract
is fixed too: `ci.yml` runs on `push`, `pull_request` and `merge_group`, and `pr-policy.yml` on
`pull_request` and `merge_group`, each with the branch list exactly `[master]` and nothing else. A
branch list is matched against the base a pull request targets, so both contexts reach every pull
request and every queue entry whose base is `master`, and a pull request targeting any other branch
receives neither and, if they are required there, waits on checks that never arrive.
`test-docs-consistency.sh` pins the two workflows against its own copy of that list; it does not
read this file, so changing the contract is a change to this file and to the gate together.

Every head branch is in the vocabulary `.github/scripts/validate-pr-branch.sh` enforces, checked by
`upstroke-pr-policy` on each pull request and each queue entry. `feature/`, `refactor/`, `docs/`,
`standards/`, `ci/`, `gate/` and `findings/` take a lower-case name whose words are joined by single
hyphens; `findings/` is for a pull request that touches `reviews/findings/` and nothing else.
`fix-P<n>/` takes `<category>_<description>` and must name exactly one finding filed under
`reviews/findings/`, for `n` in 0–3. `bulk-fix-P<n>/` takes a hyphenated name and carries a batch of
them, for `n` in 2–3; P0 and P1 are never batched. The two are separate prefixes so that each one's
rule is exact: a single-finding branch resolves to its file and a batch names none. The finding is
looked for **anywhere in the pull request** and not at its two ends: the **merge-base** tree, the
head tree, and every commit between them, and only a regular file is a finding. Repairing a finding
deletes its file, so a pull request that has done its job carries none at the head; one that files
the finding it repairs carries none at the branch point; and one that files it in one commit and
repairs it in the next carries none at either end, which is the single-pull-request path the absence
of `fix/` depends on. The boundary is the merge base and **not the target branch's current head**,
which keeps `master` merely advancing out of the verdict — but **it does not make the verdict a
function of the head**, and nothing does. The merge base moves as soon as `master` absorbs a commit
the branch also carries, and a name that was ambiguous can resolve with no push to the branch. That
is the right answer rather than a hole: whether a description picks out one finding or two is a
property of **the ledger**, which other pull requests legitimately change, and what the check
answers is whether the name resolves to exactly one filed finding in the listings it is handed.

**A check that cannot see its input refuses; it never decides that it saw nothing.** The listings
the check is handed are the merge-base tree, the head tree and the pull request's own commits, and a
maintainer running it by hand may hand it a working tree's `reviews/findings/` directory instead.
Whichever form, an input that cannot be read — an unreadable file, a stream that fails part-way, a
directory that cannot be listed, an index or a repository git cannot read — is a **refusal** and
never an empty set, because a candidate set that silently narrows turns an ambiguous name into an
accepted one. Only "there is no repository here" falls back to the filesystem: metadata that is
missing is not metadata that cannot be examined, and a `.git` file that names a gitdir git will not
resolve — or holds a NUL where a `gitdir:` line should be — is the second of those. Every external
probe, every file read and every directory listing in `validate-pr-branch.sh` goes through three
audited helpers, and all three now obtain their bytes from **one capture primitive**: it opens its
own destination and takes the open's status, runs the producer and keeps the producer's status, reads
both private copies back as far as the sentinel byte it wrote, and hands nothing over unless all of
that held. Each of those four was a round's P1 on its own — a helper that checked three of them
reused the previous capture's bytes when its destination would not open, and one that took its names
from a glob after a separate command's exit 0 read an unreadable directory as an empty one. Owning a
file establishes nothing about reading it, and a successful producer establishes nothing about a
successful read. `test-pr-policy.sh`
holds the rest of the file to an **allowlist** — below the audited region a command may only be a
shell builtin from a short list or a function the file defines, and nothing may redirect from a path
— because five rounds of closing unsafe calls one at a time produced more of them each round, and
the ban list that replaced those cases was itself walked past by an assignment prefix, a `command
--`, and a reader it did not name. That check is a text scan over one file: it bounds what is
written in the validator, not what bash can be made to do, and what it buys is that **the reviewed
surface is the audited region**, which the gate caps at 250 lines. It is a helper and not a
guarantee: a command word written entirely inside quotes leaves nothing on the line for a text scan
to read, and a command reached through an `eval` of a string it cannot see is outside any such scan.

**A directory handed in as a listing is answered out of git's records, not out of the checkout.**
The directory form locates the repository and the path within it and then reads `git ls-files -s`
alone: which names are there, and what each one is. Nothing about the working tree is consulted for a
path git records anything at, under or above — not `-d`, not `-e`, not `-L`, not a glob, and never
the bytes of a file the checkout materialised. That is what makes the two ways in one code path from
the index down, so the equivalence below holds by construction: a committed symlink named like a
finding is a `120000 blob` to both; a sparse checkout's excluded finding is an index entry and a tree
entry; a `reviews` the checkout renamed and replaced with a link is still the directory the index
records; and a link materialised under `core.symlinks=false` — git's own setting, and what it uses
wherever a link cannot be made — is never read as a listing, which is how one was made to invent a
finding nobody had filed. A listing path is reduced to its components before it is judged, so
`reviews/findings`, `reviews/findings/`, `reviews/findings/.`, `reviews//findings` and
`reviews/./findings` are one listing and answer alike, and the path the index is asked about is built
from **those** components and never from where the filesystem takes them; a `..` after a named
component is refused rather than guessed at. The work tree's root is matched against those components
by inode, so a link above the repository costs nothing — but the path **through** that root is
matched by recorded mode, because an inode comparison cannot see one: `reviews` committed as a link
to the work tree's own root is `-ef` that root, and taking it as one named the listing `findings` and
answered it out of the root's own directory, past the `120000` the index records. The filesystem is the whole of the evidence in one
place only: a listing with no repository over it, which is how the validator is run against a scratch
directory, and a path inside a work tree that git records nothing at, under **or above** — an
ordinary untracked scratch directory, and the temporary files a caller builds the three listings in.

**The property is that the two ways in agree**: for one commit, the three tree listings and the
working tree's `reviews/findings/` give the same answer, and the fixture suite checks that as a
property over repositories and branch names rather than case by case. It costs one thing and gains
another, both deliberate. An **untracked** finding file inside a tracked `reviews/findings/` no
longer counts for the directory form — the ledger is what is committed, a merge gate decides about
commits and never about a work tree, and the answer for a finding an author has written and not yet
added is `git add`. And where git records a **directory** at the listing path and the checkout holds
a link or a file in its place, the directory form now **resolves** the name from the index instead of
refusing: that is a loosening, and it is the point, because the trees resolve it too.

There is no `fix/` prefix. A bug worth a branch is worth a finding, so a repair names the finding it
closes, and a bug that is not filed yet is filed by the same pull request that repairs it — which is
what reading the whole range is for, since the repair deletes the file again in the same range.
`findings/` is not a substitute: it carries no repair.

An unrecognised prefix fails rather than defaulting, which is the point of the check: until it
existed, every prefix outside two `codex/` shapes fell into the audit's catch-all and was silently
given the most expensive review and the loosest fix set. `test`, `chore`, `perf`, `security` and
`build` are valid title types with no branch prefix; needing one is a gap to raise here, not a name
to work around. **Every head branch is in the vocabulary.** There is no exemption and no list: the
rule shipped with a migration list of the pull requests that predated it, that list was only ever
shortened, and it reached zero open pull requests — so a name outside the vocabulary is refused
whoever opened the pull request and whatever its number.

The lane the audit derives from a prefix is a **separate** mapping, and it is unchanged: it still
reads `codex/findings-p3-*`, `codex/findings-*` and everything else. A `findings/`, `fix-P<n>/` or
`bulk-fix-P<n>/` branch is therefore audited as feature work, which is looser than any of the three
is meant to be. That was tolerable while `fix-P<n>/` was reserved and unused; with no `fix/` prefix
every repair is on it, so widening the lane table is the next change this rule owes. It is its own
change because it touches `scripts/pr-ready-audit.sh`.

Readiness to enqueue is the lane rule of 2026-09-06, audited by `scripts/pr-ready-audit.sh`,
which decides a pull request's lane from its branch prefix alone (`codex/findings-p3-*`,
`codex/findings-*`, everything else), counts only the owner's review comments, keeps the `lane:*`
and `ready-to-merge` labels current (a label is its output, never its input, and
`ready-to-merge` is advisory: it reports the audit's verdict on the head it read, while the act
bound to that head is the enqueue itself, which names the commit), and with `--enqueue` adds
each ready pull request to the queue in the order its arguments give, so the caller states the
priority. The
P3 findings lane is ready only on a `PASS`; the P1/P2 findings lane fixes P0–P2 and files P3;
feature and sweep work fixes P0–P1 and files P2 and P3, one file per finding under
`reviews/findings/` with a `deferred` ledger row. A witnessed defect or a `MUST` deviation is
fixed whatever its label, as step 5 says.

**Trust boundary.** There is one trusted same-repository writer: the owner. A pull request can
edit `ci.yml`, `pr-policy.yml` and the validators they run and still turn both contexts green, so
the checks catch honest mistakes and are not the security boundary. The boundary is that only the
owner merges — or an agent the owner has delegated to for that pull request — after an independent
review recorded per step 4, and that the diff the owner reads includes any change to the gates. No
automated process merges or mints a merge-gating check: there is no machine review check, no App,
and no token that can attest. A return of automated attestation needs a signer distinct from the
owner's token, structurally validated verdicts, and a reviewer that holds no attesting credential.

Keep workflow approval for **all** external contributors, and keep release immutability enabled;
bootstrap and audit both through the API:

```bash
gh api --method PUT \
  repos/sourcemaps/upstroke/actions/permissions/fork-pr-contributor-approval \
  -H 'X-GitHub-Api-Version: 2026-03-10' \
  -f approval_policy=all_external_contributors

gh api --method PUT repos/sourcemaps/upstroke/immutable-releases \
  -H 'X-GitHub-Api-Version: 2026-03-10'

gh variable set UPSTROKE_IMMUTABLE_RELEASES_REQUIRED \
  --repo sourcemaps/upstroke --body true

gh api repos/sourcemaps/upstroke/actions/permissions/fork-pr-contributor-approval \
  -H 'X-GitHub-Api-Version: 2026-03-10'
gh api repos/sourcemaps/upstroke/immutable-releases \
  -H 'X-GitHub-Api-Version: 2026-03-10'
gh variable get UPSTROKE_IMMUTABLE_RELEASES_REQUIRED \
  --repo sourcemaps/upstroke
```

Fork pull requests are provisional: their checks are candidate-controlled, so the whole diff
including workflow edits is reviewed before merge. Do not add another same-repository writer
without revisiting this section. Required approving reviews stay at zero because an author cannot
approve their own pull request; if a machine account ever opens pull requests,
`require_code_owner_review` with the owner as code owner restores an exact-head sign-off. Any future
emergency bypass is pull-request-only, limited to an Actions outage or a rule that blocks its own
repair, and never a direct push.

## Release contract

Release tags use `v*` and the tag ruleset makes them immutable. Repository release immutability is
a separate mandatory setting: tag protection fixes the commit, release immutability keeps published
binaries from being replaced under it. GitHub applies it only to future releases, so enable it and
read it back before creating another tag. The release job's token cannot read that setting, so
`UPSTROKE_IMMUTABLE_RELEASES_REQUIRED=true` records the owner's readback and makes an incomplete
bootstrap fail closed. Re-read the live setting before every release.

The release workflow verifies that the tagged commit is reachable from `origin/master`, that the
tag matches `Cargo.toml`, and that the release gates and platform builds pass before publishing. It
refuses an existing mutable or incomplete release, discards only an unpublished same-tag draft
created by `github-actions[bot]` after a failed attempt, and skips rather than overwrites a complete
immutable release. Uploads must contain exactly the three expected assets; the workflow verifies
GitHub's signed release attestation and each local archive against its attested digest. The GitHub
release is created before the irreversible crates.io publish. Create releases from an already
merged mainline commit.

The next release is also gated by the 2026-09-01 relicensing: each archive must carry `LICENSE`,
`NOTICE` and generated third-party attributions before any new `v*` tag is created. The workflow
does not yet inspect archive contents, so until it does, verify this at tag time by reading the
archives back.

Release `v0.1.0` predates release immutability and is the sole legacy exception. Do not rerun,
replace or delete its assets. Its preserved GitHub asset digests are:

- `upstroke-aarch64-apple-darwin.tar.gz`: `sha256:552302e348273143665d2604130e6c1487647a90b496a8d8f789d30839175289`
- `upstroke-x86_64-pc-windows-msvc.zip`: `sha256:e88206643c07ac5cee418ed27ddbbb7e6bcffc1835e727a68bbd716f876c8871`
- `upstroke-x86_64-unknown-linux-gnu.tar.gz`: `sha256:94447cfd56d0d8ba5eae1ec391c2564a7ddba2fceb15cee35ce537a0ba00d798`

## High-blast-radius changes

Changes to event or replay schemas, Git/ref handling, agent permissions, `upstroke.toml`, the
design, CI, release, or rules deserve an especially narrow pull request and a focused fresh-context
review. A pull request can edit ordinary Actions workflows, so the independent review is the trust
boundary for changes to the gates themselves, and the owner reads those diffs line by line before
merging.
