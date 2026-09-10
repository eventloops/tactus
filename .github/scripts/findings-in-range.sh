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
# THE BOUNDARY IS THE MERGE BASE, NOT THE EVENT'S BASE SHA. This is the whole
# shape of the file and it is the third attempt at it.
#
#   The event's base SHA is the TARGET BRANCH'S CURRENT HEAD, which moves for
#   reasons that have nothing to do with this pull request. Rooting the listings
#   there let master decide a pull request's verdict: with two findings sharing
#   a description at the branch point, one of them repaired by the pull request
#   and the same one independently deleted on master, `<base>..<head>` no longer
#   reaches the branch point once master has moved. The ambiguous name that was
#   refused at exit 1 before master moved CONFORMED at exit 0 after it. Same
#   head, same diff, opposite verdict.
#
#   The merge base does not move. Advancing master adds commits that descend
#   from its old head and are not ancestors of this head, so the best common
#   ancestor is unchanged; only rewriting the target branch's history, or the
#   pull request itself merging or rebasing onto master -- which changes the
#   head, and so is inside the pull request -- can move it.
#
#   Every merge base is listed, not one. `git merge-base` picks one of several
#   best common ancestors when the histories criss-cross, and a candidate set
#   that depends on which one it picked is the same class of bug. Listing all of
#   them can only WIDEN the set, and a wider set fails closed: it can turn an
#   accepted name into an ambiguous refusal and never the other way round.
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
#   commit in <merge base>..<head>, and reviews/findings/ as that commit left
#   it. Taken only at the two ends the diff is empty for such a finding -- the
#   add and the delete cancel -- which is measured in test-pr-policy.sh.
#
#   That commit set is fixed by the merge base and the head alone, so it does
#   not drift when master advances either.
#
# THE TREES ARE LISTED, NOT ASKED WHAT CHANGED. `git log -- reviews/findings/`
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
# `reviews/findings/P2_correctness_<ts>_<desc>.md/` satisfied
# `fix-P2/correctness_<desc>` with no finding in existence. The mode is checked:
# 100644 and 100755 are findings, a tree is not, and neither is a symlink or a
# submodule.

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
# reviews/findings/ at that commit. `git ls-tree` prints `<mode> <type> <object>
# TAB <path>`; the mode is what separates a finding from a directory carrying a
# finding's name. Splitting on the tab keeps a path with spaces in it whole.
findings_in() {
  git ls-tree "$1" -- reviews/findings/ \
    | awk -F'\t' '$1 ~ /^100[0-7][0-7][0-7] blob / {
        name = $2; sub(/.*\//, "", name); if (name != "") print name }'
}

while read -r merge_base; do
  findings_in "$merge_base" || exit 1
done < "$out/merge-bases" | sort -u > "$out/merge-base-findings"

findings_in "$head" | sort -u > "$out/head-findings"

# Reachable from the head and from none of the merge bases: the pull request's
# own commits, and nothing the target branch has done since the branch point.
{ echo "$head"; sed -e 's/^/^/' "$out/merge-bases"; } \
  | git rev-list --stdin > "$out/range-commits"
while read -r commit; do
  findings_in "$commit" || exit 1
done < "$out/range-commits" | sort -u > "$out/range-findings"
