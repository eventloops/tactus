#!/usr/bin/env bash
# findings-in-range.sh <target-sha> <head-sha> <out-dir>
#
# Write the three finding listings .github/scripts/validate-pr-branch.sh
# resolves a fix-P<n>/ branch name against, from the repository in the current
# directory: <out-dir>/merge-base-findings, head-findings and range-findings,
# each a list of bare finding filenames.
#
# THIS IS A SCRIPT AND NOT FOUR LINES OF WORKFLOW BECAUSE IT HAS TO BE TESTED.
# It was four lines of workflow, and the fixtures could not reach it: the
# vocabulary suite exercised the validator against listings it wrote itself, so
# it proved what the validator does with a listing and nothing at all about
# whether the listing was right. Three ways of building it wrongly survived a
# frontier review each. .github/scripts/test-pr-policy.sh now builds real
# repositories and calls this.
#
# THE BOUNDARY IS THE MERGE BASE, NOT THE EVENT'S BASE SHA -- AND IT IS A
# DEFAULT AND NOT A GUARANTEE. Read the second paragraph before believing the
# first.
#
#   The event's base SHA is the TARGET BRANCH'S CURRENT HEAD, which moves for
#   reasons that have nothing to do with this pull request. Rooting the listings
#   there let master decide a pull request's verdict in the ordinary case: with
#   two findings sharing a description at the branch point, one of them repaired
#   by the pull request and the same one independently deleted on master,
#   `<base>..<head>` no longer reaches the branch point once master has moved.
#   The ambiguous name that was refused at exit 1 before master moved CONFORMED
#   at exit 0 after it. Same head, same diff, opposite verdict. The merge base
#   holds under that: the new commits descend from master's old head and are not
#   ancestors of this head, so the best common ancestor is where it was.
#
#   IT IS NOT A FIXED POINT, AND NOTHING HERE CAN MAKE IT ONE. The merge base
#   moves as soon as master absorbs a commit THIS BRANCH ALSO CONTAINS: master
#   merges another pull request carrying commit C, C is an ancestor of this
#   head, and the merge base advances to C. Everything between the old boundary
#   and C leaves the candidate set, and a name that was ambiguous becomes
#   unambiguous with no push to the branch. Measured on the same head: exit 1,
#   `names 2 findings` before that merge; exit 0, `conforms` after it.
#
#   THAT IS THE RIGHT ANSWER RATHER THAN A HOLE, AND IT IS WHY NO FOURTH
#   BOUNDARY IS TRIED HERE. What a fix-P<n>/ name claims is that this pull
#   request repairs one FILED finding, and whether a description picks out one
#   finding or two is a property of the LEDGER -- which other pull requests
#   legitimately change. If the twin really was resolved by something else, the
#   name really is unambiguous now, and exit 0 is correct. Three boundaries have
#   been tried -- the two endpoint trees, `<event base>..<head>`, and the merge
#   base -- and each was disproved by a case where the ledger moved underneath
#   an unchanged head. A boundary that does not move is not the missing piece;
#   there is no such boundary, because the thing being resolved is not a
#   property of the branch. So: THIS SCRIPT BUILDS THE LISTINGS THE NAME IS
#   RESOLVED AGAINST OUT OF THE PULL REQUEST -- its merge bases, its head, and
#   its own commits -- AS THEY STAND WHEN IT RUNS, and it runs again on every
#   synchronize and on the queue entry that is actually merged.
#
#   THAT IS NOT THE LEDGER AS IT STANDS ANYWHERE, and the three listings must
#   not be described as one. The tree of the commit being merged is never
#   listed: the head is the pull request's head and the boundary is its merge
#   base, so a finding the TARGET filed after the branch point is in none of
#   the three on the queue entry either. Replayed on a queue merge whose target
#   had added two same-description findings, the name conformed at exit 0 while
#   the queue commit's own findings/ named 2 findings at exit 1. What a
#   green check says is the narrow thing: the name resolved to exactly one
#   filed finding in the listings built here, at the moment they were built.
#
#   The merge base is still the right DEFAULT. It is the boundary that does not
#   drag in a finding this branch never saw -- rooted at the target's current
#   head, this pull request's own candidate set gained 23 of master's findings,
#   308 names instead of 285 -- and it holds under every way master advances
#   except absorbing this branch's own commits. It is chosen for that, not for a
#   fixity it does not have.
#
#   EVERY MERGE BASE IS LISTED, AND EACH ONE'S RANGE IS TAKEN SEPARATELY.
#   `git merge-base` picks one of several best common ancestors when the
#   histories criss-cross, and a candidate set that depends on which one it
#   picked is the same class of bug. But listing them all is conservative only
#   if the answer is the UNION of what each boundary gives ALONE; see the loop
#   at the foot of this file for the way that was got wrong.
#
# THE THREE SOURCES.
#
#   The merge-base trees hold a finding that PRE-EXISTED the pull request,
#   including one the pull request has since repaired and deleted -- which is
#   what every fix-P<n>/ pull request looks like once it has done its job.
#
#   The head tree holds a finding the pull request FILED, which is the shape of
#   a bug that was not on the record before.
#
#   A finding the pull request files AND repairs is in NEITHER, and that is the
#   single-pull-request path the absence of a fix/ prefix depends on. It is
#   visible only in the pull request's own commits, so the third source is the
#   pull request's diff against the merge base taken COMMIT BY COMMIT: every
#   commit in <merge base>..<head>, and findings/ as that commit left
#   it. Taken only at the two ends the diff is empty for such a finding -- the
#   add and the delete cancel -- which is measured in test-pr-policy.sh.
#
#   That commit set is a function of the merge base and the head alone, so it
#   does not drift when master advances -- until master advances THROUGH one of
#   this branch's own commits, which moves the merge base and is the case the
#   boundary paragraph above refuses to pretend it can prevent.
#
# THE TREES ARE LISTED, NOT ASKED WHAT CHANGED. `git log -- findings/`
# answers a different question -- which commits changed that path, after
# simplification -- and it misses findings two ways, both measured on
# purpose-built ranges: HISTORY SIMPLIFICATION prunes a side branch whose net
# effect on the path is nothing, which is exactly a completed repair merged in;
# and MERGE COMMITS report no paths unless per-parent diffs are asked for. A
# hidden finding is not only a false red on a valid branch, it is a false GREEN
# on an ambiguous one, so the set is built the way that depends on none of git's
# history-simplification or merge-diff rules.
#
# ONLY REGULAR FILES ARE FINDINGS. `git ls-tree --name-only` does not say
# whether an entry is a file or a directory, and a committed DIRECTORY named
# `findings/P2_correctness_<ts>_<desc>.md/` satisfied
# `fix-P2/correctness_<desc>` with no finding in existence. The mode is checked:
# 100644 and 100755 are findings, a tree is not, and neither is a symlink or a
# submodule.
#
# AND NO ENTRY NAME HERE TRAVERSES A SYMLINK, which is the half of the two-API
# equivalence this end supplies. `ls-tree -- findings/` lists the entries
# git RECORDS under that path; where `findings` itself is a
# committed symlink it is a `120000 blob` and nothing is recorded under it, so
# this listing is empty however many findings sit at the end of the link. That is
# a property of NAMES IN A TREE and needs no filesystem to hold.
#
# The other way in -- handing validate-pr-branch.sh a working tree's directory --
# used to answer from the filesystem with git as a cross-check, and every review
# round found another spelling of the same disagreement: a link followed by `-d`,
# the same link materialised as a regular file where `-L` had nothing to see, a
# sparse checkout the index outlived, a `findings` renamed out from under an
# unchanged index. It now locates the repository and the path within it and reads
# `git ls-files -s` alone, which is the same question this file asks of the same
# ledger one layer along -- names recorded under a path, filtered by recorded
# mode. Same commit, same answer, and now by construction rather than by fixture.
# The cost is stated in MAINTAINING.md: an untracked file in the working tree's
# findings/ counts for neither end.

set -euo pipefail
export PATH="/usr/bin:/bin:$PATH"

target="${1:-}"
head="${2:-}"
out="${3:-}"

if [[ -z "$target" || -z "$head" || -z "$out" ]]; then
  echo "usage: findings-in-range.sh <target-sha> <head-sha> <out-dir>" >&2
  exit 2
fi

# An end that will not resolve fails closed rather than being dropped: dropping
# one narrows the candidate set, and a narrowed set is what turns an ambiguous
# name into an accepted one.
git rev-parse --verify --quiet "$target^{commit}" >/dev/null \
  || { echo "target commit $target is not in this checkout" >&2; exit 1; }
git rev-parse --verify --quiet "$head^{commit}" >/dev/null \
  || { echo "head commit $head is not in this checkout" >&2; exit 1; }

mkdir -p "$out"

# No common ancestor is not an empty set either: it means the two ends are
# unrelated histories and nothing here can be resolved.
git merge-base --all "$target" "$head" > "$out/merge-bases" \
  || { echo "no merge base between $target and $head" >&2; exit 1; }
[[ -s "$out/merge-bases" ]] \
  || { echo "no merge base between $target and $head" >&2; exit 1; }

# findings_in <commit-ish>: the bare filenames of the REGULAR FILES directly in
# the ledger directory at that commit. `git ls-tree` prints `<mode> <type>
# <object> TAB <path>`; the mode is what separates a finding from a directory
# carrying a finding's name. Splitting on the tab keeps a path with spaces in it
# whole.
#
# BOTH LEDGER PREFIXES ARE LISTED, BECAUSE THIS FUNCTION IS ASKED ABOUT HISTORY.
# The ledger moved from reviews/findings/ to findings/ on 2026-09-12 (pull
# request #276). Every commit this function is called on is a commit as it
# stands -- the merge bases above and each commit of the ranges below -- and a
# branch cut before that move has a merge base whose findings are ALL under the
# old prefix. Listing findings/ alone read such a commit as an EMPTY LEDGER.
# Measured on the range of the pull request that made the move, 61ec7587 to
# f036ad1a: the merge-base listing came back with 0 names where the tree holds
# 336, and a fix-P<n>/ name whose twin was filed before the move and repaired by
# the branch CONFORMED at exit 0 where listing both prefixes refuses it at exit
# 1, `names 2 findings`. An emptied merge-base listing is not a false red. It is
# a false GREEN, because the twin that makes a name ambiguous is exactly what
# lives at the base and nowhere else once the branch has done its job.
#
# NO STATUS CHECK COULD HAVE CAUGHT IT, which is why it is fixed here and not at
# the call sites. `git ls-tree <commit> -- <prefix>` with a pathspec that matches
# nothing SUCCEEDS WITH EMPTY OUTPUT: at 61ec7587, `-- findings/` exits 0 with 0
# bytes while `-- reviews/findings/` exits 0 with 336 entries. There is no
# failure for the `|| exit 1` at each call site to propagate, so a prefix that
# does not exist at that commit reads as a ledger with nothing in it. The same
# shape, in the recurrence search `findings/README.md` documents, hid every
# finding closed before the move; the two are one defect and were fixed
# together.
#
# THE BARE FILENAME IS THE IDENTITY, so a finding survives the rename: one file
# is one name whether it is listed at reviews/findings/P2_... before the move or
# findings/P2_... after it. Where a single commit carries both directories -- a
# branch cut before the move that files under the old prefix, which C5 of
# test-docs-consistency.sh refuses at a head and cannot refuse in history -- awk
# prints a name per entry and the `sort -u` at every call site collapses it.
#
# THIS IS NOT TRANSITIONAL. History does not stop carrying the old prefix, so
# both are listed for as long as a commit from before 2026-09-12 can be a merge
# base, which is for as long as the repository keeps its history. What widening
# a listing can do is bounded below: it can only RAISE a name's match count, so
# no name is made to conform by it, and the acceptance it does allow -- a name
# that matched nothing because the finding it repairs was filed under the old
# prefix now matching that one finding -- is the false red this fixes and the
# right answer for the reason the safe-widening paragraph gives.
findings_in() {
  git ls-tree "$1" -- findings/ reviews/findings/ \
    | awk -F'\t' '$1 ~ /^100[0-7][0-7][0-7] blob / {
        name = $2; sub(/.*\//, "", name); if (name != "") print name }'
}

while read -r merge_base; do
  findings_in "$merge_base" || exit 1
done < "$out/merge-bases" | sort -u > "$out/merge-base-findings"

findings_in "$head" | sort -u > "$out/head-findings"

# THE RANGE IS THE UNION OF THE PER-BOUNDARY RANGES, NOT `rev-list head ^b1 ^b2`.
# One rev-list excluding every merge base at once is the INTERSECTION of the
# ranges the bases give individually, and an intersection is NARROWER than any
# of its members -- so a second merge base could shrink the candidate set, which
# is the direction that turns an ambiguous name into an accepted one.
#
#   Measured on criss-crossed histories with two best common ancestors L and R,
#   where L's history files a finding and then repairs and deletes it, and R
#   carries a second finding of the same severity, category and description.
#   Resolved from R alone the two twins are both in the set: exit 1, `names 2
#   findings`. Resolved from the real target, which discovers L as well, adding
#   L to one exclusion list dropped the commit that held the first twin and the
#   ambiguous name CONFORMED at exit 0.
#
# Each base therefore contributes the range it would give on its own, and the
# results are unioned. That restores the property the paragraph at the top of
# this file relies on: another merge base can only widen the set.
#
# Under a criss-cross the union then holds commits that are ALSO reachable from
# the target -- R's own commits, when L is the other base. That is deliberate,
# not a leak: they are reachable from this head, so they are in this branch.
#
# WHAT WIDENING IS SAFE AGAINST, EXACTLY. A wider set can only RAISE a name's
# match count, so a name that matches two findings still matches two: an
# AMBIGUOUS name can never conform because another boundary was added, which is
# the direction this gate exists to hold. It is not one-way in general, and an
# earlier revision of this comment claimed it was. A name matching NOTHING can
# become a name matching one, which is a refusal turning into an acceptance:
# measured on the criss-crossed histories below, `fix-P3/liveness_only-one` is
# refused at exit 1 `names no finding` by the single-rev-list form and conforms
# at exit 0 once each boundary's range is taken separately. That acceptance is
# the right answer -- the finding did exist inside the pull request, which is
# the whole of what the name claims -- but it is an acceptance the narrower set
# denied, and saying "never the other way round" hid it.
#
# Where there is one merge base, which is every open pull request in this
# repository today, this loop is the single rev-list it replaces.
while read -r merge_base; do
  git rev-list "$head" "^$merge_base" || exit 1
done < "$out/merge-bases" | sort -u > "$out/range-commits"
while read -r commit; do
  findings_in "$commit" || exit 1
done < "$out/range-commits" | sort -u > "$out/range-findings"
