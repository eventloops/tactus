#!/usr/bin/env bash
# validate-pr-branch.sh <branch-name> [findings]
#
# Refuse a pull request whose head branch is not in the branch vocabulary.
#
# WHY A BRANCH NAME IS POLICY AND NOT TASTE. The prefix already decides two
# things that bind a merge: the effort the frontier review is run at
# (review-poller.sh's lane_of and effort_for_lane) and the severities the pull
# request must fix before it is ready (scripts/pr-ready-audit.sh's lane_for and
# must_fix_for, the rule MAINTAINING.md states). Until this validator existed
# every prefix outside two `codex/` shapes fell into a `feature` catch-all, so a
# branch nobody had thought about was silently assigned the most expensive
# review and the loosest fix set, and nothing said so. An unrecognised prefix
# must fail here rather than default; that refusal is the point of this file,
# and the vocabulary below is the part that has to stay true.
#
# A lane:* label is an OUTPUT of that rule and never an input. Nothing may read
# a label to decide effort or must-fix: a label is not bound to a commit, so
# editing one would otherwise change which gates apply to a merge.
#
# THE VOCABULARY.
#
#   feature/<slug>                  new behaviour
#   refactor/<slug>                 behaviour-preserving change
#   docs/<slug>                     documentation only
#   standards/<slug>                a standards/ section, or a sweep under one
#   ci/<slug>                       gates, workflows, scripts/ and .github/
#   gate/<slug>                     a cumulative review-gate report
#   fix/<slug>                      a bug that is not a filed finding
#   fix-P<n>/<category>_<desc>      exactly one finding, n in 0..3
#   bulk-fix-P<n>/<slug>            a batch of findings, n in 2..3
#
# <slug> and <desc> are lower-case words joined by single hyphens. <category>
# is one of the eight the finding filenames and validate-pr-body.sh use; the
# list is duplicated there and the two must move together.
#
# WHY fix-P*/ AND bulk-fix-P*/ ARE DIFFERENT PREFIXES. A fix-P*/ branch repairs
# exactly one finding and its name resolves to that finding's file, so the
# mapping is total and this validator checks it. A batch holds many findings and
# can name none of them. Sharing one prefix would leave the check guessing which
# shape it was looking at, so they are separated and each prefix's rule is
# exact. P0 and P1 never batch, which is why bulk-fix stops at P2.
#
# THE PER-FINDING BRANCH IS A LOCK. For P2 and P3 the fix is done on the
# per-finding branch and cherry-picked into the batch; the branch existing is
# what advertises that the finding is taken, so nothing else picks it up. Those
# branches are not expected to open a pull request, but they are validated here
# anyway because one may.
#
# THE FINDINGS ARE READ AT THE BASE, NEVER AT THE HEAD. Repairing a finding
# DELETES its file: master carries 69 such deletions. A fix-P*/ pull request that
# has done its job therefore has no finding file left in its own tree, and
# resolving the name against that tree would fail exactly the pull requests that
# succeeded. The claim a branch name makes is about the finding that existed when
# the branch was cut, so the caller passes the base listing and this script never
# looks at the working tree. `findings` is that listing: a file of finding
# filenames, one per line, or a directory to list when running by hand.
#
# GRANDFATHERING. .github/legacy-branches.txt lists the branches that predate
# this rule, one per line. It is a to-do list that shrinks: a listed branch is
# accepted with a warning, and the file reaching zero entries is the signal the
# migration finished. It is not an escape hatch for new work, and adding to it
# is a diff the review and the owner see like any other.
#
# With no `findings` argument only the grammar is checked, which is how the
# fixtures exercise it without a repository.

set -euo pipefail
export PATH="/usr/bin:/bin:$PATH"

# `dirname` and not "${BASH_SOURCE[0]%/*}": that expansion strips nothing when
# the script is invoked by bare name from inside its own directory, which is the
# bug .github/scripts/test-pr-policy.sh carries and CLAUDE.md warns about.
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
legacy_file="${LEGACY_BRANCHES:-$script_dir/../legacy-branches.txt}"

branch="${1:-}"
findings="${2:-}"

slug_re='[a-z0-9]+(-[a-z0-9]+)*'
# Keep in step with .github/scripts/validate-pr-body.sh's category case.
category_re='(correctness|crash-consistency|security-trust|portability|liveness|performance|compatibility|docs-contract)'

vocabulary() {
  cat >&2 <<'EOF'

The head branch must be one of:

  feature/<slug>                  new behaviour
  refactor/<slug>                 behaviour-preserving change
  docs/<slug>                     documentation only
  standards/<slug>                a standards/ section, or a sweep under one
  ci/<slug>                       gates, workflows, scripts/ and .github/
  gate/<slug>                     a cumulative review-gate report
  fix/<slug>                      a bug that is not a filed finding
  fix-P<n>/<category>_<desc>      exactly one finding, n in 0..3
  bulk-fix-P<n>/<slug>            a batch of findings, n in 2..3

<slug> and <desc> are lower-case words joined by single hyphens, e.g.
`feature/pr9-repair-execution`. <category> is one of correctness,
crash-consistency, security-trust, portability, liveness, performance,
compatibility, docs-contract, and a fix-P*/ branch names one finding:

  reviews/findings/P1_correctness_202609040301_pid-identity-under-a-host-wildcard-waiter.md
  fix-P1/correctness_pid-identity-under-a-host-wildcard-waiter

There is deliberately no prefix for a `test`, `chore`, `perf`, `security` or
`build` change even though those are valid title types. If you need one, that is
a gap in the vocabulary to raise rather than a name to work around: say so and
the rule changes in MAINTAINING.md.
EOF
}

fail() {
  echo "branch-name-policy: $*" >&2
  vocabulary
  exit 1
}

[[ -n "$branch" ]] || fail 'no branch name was given'

# A listed legacy branch is accepted, loudly, so the exemption is visible in the
# check's log rather than silent.
if [[ -f "$legacy_file" ]] \
  && grep -v '^[[:space:]]*#' "$legacy_file" | grep -qxF "$branch"; then
  echo "branch-name-policy: '$branch' predates the branch vocabulary and is" >&2
  echo "  listed in ${legacy_file##*/}. Rename it when it next comes up for merge." >&2
  exit 0
fi

# finding_names: the finding filenames to resolve against, one per line, from
# whichever form the caller passed. Empty when no argument was given, which
# turns the resolution off and leaves the grammar checks in place.
finding_names() {
  if [[ -d "$findings" ]]; then
    ls -1 "$findings"
  elif [[ -f "$findings" ]]; then
    cat "$findings"
  else
    fail "findings listing '$findings' is neither a file nor a directory"
  fi
}

# resolve_finding <severity-digit> <category> <description>: the branch claims
# to repair one filed finding. Exactly one filename must carry that severity,
# category and description; the timestamp between them is free.
resolve_finding() {
  local n="$1" cat="$2" desc="$3" matches count
  [[ -n "$findings" ]] || return 0
  matches="$(finding_names | grep -E "^P${n}_${cat}_[0-9]+_${desc}\.md$" || true)"
  count="$(grep -c . <<< "${matches:-}" || true)"
  [[ -n "$matches" ]] || count=0
  case "$count" in
    1) return 0 ;;
    0) fail "'$branch' names no finding: expected exactly one
  P${n}_${cat}_<timestamp>_${desc}.md
  A fix-P*/ branch repairs one filed finding and mirrors its severity, category
  and description. If this is a bug that was never filed, use fix/<slug>." ;;
    *) fail "'$branch' names $count findings, which is ambiguous:
$(sed 's/^/  /' <<< "$matches")" ;;
  esac
}

rest="${branch#*/}"
[[ "$rest" != "$branch" ]] || fail "'$branch' has no prefix; it must be <prefix>/<name>"

case "$branch" in
  fix-P[0-3]/*)
    n="${branch#fix-P}"; n="${n%%/*}"
    [[ "$rest" =~ ^${category_re}_${slug_re}$ ]] \
      || fail "'$branch' is not <category>_<description> after the prefix"
    cat="${rest%%_*}"
    desc="${rest#*_}"
    resolve_finding "$n" "$cat" "$desc"
    ;;
  bulk-fix-P[23]/*)
    [[ "$rest" =~ ^${slug_re}$ ]] \
      || fail "'$branch' must be lower-case words joined by single hyphens after the prefix"
    ;;
  bulk-fix-P[01]/*)
    fail "'$branch' batches a severity that is never batched: a P0 or P1 finding
  is repaired on its own fix-P<n>/<category>_<description> branch."
    ;;
  feature/*|refactor/*|docs/*|standards/*|ci/*|gate/*|fix/*)
    [[ "$rest" =~ ^${slug_re}$ ]] \
      || fail "'$branch' must be lower-case words joined by single hyphens after the prefix"
    ;;
  *)
    fail "'${branch%%/*}/' is not a known branch prefix"
    ;;
esac

echo "branch-name-policy: '$branch' conforms"
