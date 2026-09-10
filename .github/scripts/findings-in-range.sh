#!/usr/bin/env bash
# findings-in-range.sh <base-sha> <head-sha> <out-dir>
#
# Write the three finding listings .github/scripts/validate-pr-branch.sh
# resolves a fix-P<n>/ branch name against, from the repository in the current
# directory: <out-dir>/base-findings, head-findings and range-findings, each a
# list of bare finding filenames.
#
# THIS IS A SCRIPT AND NOT FOUR LINES OF WORKFLOW BECAUSE IT HAS TO BE TESTED.
# It was four lines of workflow, and the fixtures could not reach it: the
# vocabulary suite exercised the validator against listings it wrote itself, so
# it proved what the validator does with a listing and nothing at all about
# whether the listing was right. Two ways of building it wrongly survived a
# frontier review each. .github/scripts/test-pr-policy.sh now builds real
# repositories and calls this.
#
# THE RANGE IS BUILT BY LISTING TREES, NOT BY ASKING `git log` WHAT CHANGED.
# The claim a fix-P<n>/ name makes is that the finding EXISTED somewhere in this
# pull request. `git log -- reviews/findings/` answers a different question --
# which commits changed that path, after simplification -- and it misses
# findings two ways, both measured on purpose-built ranges:
#
#   HISTORY SIMPLIFICATION prunes a side branch whose net effect on the path is
#   nothing. A completed repair -- commit A files the finding, commit B repairs
#   it and deletes the file, as reviews/findings/README.md requires -- merged in
#   after an unrelated commit reports NO paths at all.
#
#   MERGE COMMITS report no paths unless per-parent diffs are asked for, so a
#   finding created and deleted only inside merges reports nothing even under
#   --full-history.
#
# A hidden finding is not only a false red on a valid branch. It is a false
# GREEN on an ambiguous one: two findings share a description, the gate sees one
# of them, and a name that could mean either "conforms". That is the failure
# this gate exists to prevent, so the set is built the way that cannot miss --
# every commit in the range, and the directory as that commit left it. It
# depends on none of git's history or merge-diff rules.
#
# The base tree is listed separately because `<base>..<head>` excludes the base
# itself, and a finding this pull request never touched lives only there. The
# head tree is listed separately because `<base>..<head>` is empty when the head
# is an ancestor of the base, and the head's own findings would then be unseen.

set -euo pipefail
export PATH="/usr/bin:/bin:$PATH"

base="${1:-}"
head="${2:-}"
out="${3:-}"

if [[ -z "$base" || -z "$head" || -z "$out" ]]; then
  echo "usage: findings-in-range.sh <base-sha> <head-sha> <out-dir>" >&2
  exit 2
fi

# An end that will not resolve fails closed rather than being dropped: dropping
# one narrows the candidate set, and a narrowed set is what turns an ambiguous
# name into an accepted one.
git rev-parse --verify --quiet "$base^{commit}" >/dev/null \
  || { echo "base commit $base is not in this checkout" >&2; exit 1; }
git rev-parse --verify --quiet "$head^{commit}" >/dev/null \
  || { echo "head commit $head is not in this checkout" >&2; exit 1; }

mkdir -p "$out"

# Blank lines are dropped with sed and not `grep -v`: grep exits 1 when it
# filters everything out, and these pipelines must fail only when git does.
git ls-tree --name-only "$base" reviews/findings/ \
  | sed -e "s|.*/||" -e '/^$/d' | sort -u > "$out/base-findings"
git ls-tree --name-only "$head" reviews/findings/ \
  | sed -e "s|.*/||" -e '/^$/d' | sort -u > "$out/head-findings"

git rev-list "$base..$head" > "$out/range-commits"
while read -r commit; do
  git ls-tree --name-only "$commit" reviews/findings/ || exit 1
done < "$out/range-commits" \
  | sed -e "s|.*/||" -e '/^$/d' | sort -u > "$out/range-findings"
