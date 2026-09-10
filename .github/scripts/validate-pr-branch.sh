#!/usr/bin/env bash
# validate-pr-branch.sh <branch-name> [base-findings [head-findings [range-findings]]]
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
#   findings/<slug>                 reviews/findings/ alone: filing or curating
#   fix-P<n>/<category>_<desc>      exactly one finding, n in 0..3
#   bulk-fix-P<n>/<slug>            a batch of findings, n in 2..3
#
# <slug> and <desc> are lower-case words joined by single hyphens. <category>
# is one of the eight the finding filenames and validate-pr-body.sh use; the
# list is duplicated there and the two must move together.
#
# WHY THERE IS NO fix/ PREFIX. There was one, for "a bug that is not a filed
# finding", and it is retired. It was a second way to repair something, and the
# two disagreed about whether the repair was on the record: a fix-P*/ branch
# named the finding it closed, a fix/ branch named nothing and left the reviewer
# to work out what it was for. A bug worth a branch is worth a finding, so every
# repair now goes through fix-P<n>/ and the finding exists: filed by an earlier
# pull request, or filed by this one. The base-OR-head resolution below is what
# makes the second case work, and without it retiring fix/ would have forced one
# pull request to file a finding and a second to repair it.
#
# findings/<slug> IS NOT ITS REPLACEMENT. It is for a pull request that touches
# reviews/findings/ and nothing else: filing what a review produced, or curating
# what is already there. It repairs no code. A branch that files a finding AND
# repairs it is a fix-P<n>/ branch, because the repair is the half that binds a
# merge and it is the half the name has to declare.
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
# THE FINDING IS LOOKED FOR ACROSS THE WHOLE RANGE, NOT AT ITS TWO ENDS.
# The claim a fix-P*/ name makes is "this pull request repairs that finding".
# The finding therefore has to have EXISTED somewhere in the pull request's
# range; requiring it at an endpoint is a different, stricter claim, and both
# endpoints refuse a pull request that is doing exactly the right thing:
#
#   The base alone fails the pull request that files its own finding. With no
#   fix/ prefix, a bug that was never filed has to become a finding before it
#   can have a branch name, and a base-only rule means the finding has to be on
#   master before the branch exists: one pull request to file, a second to fix.
#
#   The head alone fails the pull request that did its job. Repairing a finding
#   DELETES its file: master carries 69 such deletions. A fix-P*/ pull request
#   that has landed its repair has no finding file left in its own tree.
#
#   BOTH ENDPOINTS TOGETHER STILL FAIL THE ONE PULL REQUEST THE PREFIX EXISTS
#   FOR: one that files the finding in commit A and repairs it in commit B,
#   deleting the file as reviews/findings/README.md requires. The base has no
#   finding, the head has no finding, and the file existed only in between.
#   Keeping the file to get past the check is not the answer either: that leaves
#   finished work sitting in the outstanding queue, which the ledger forbids.
#
# So the caller passes the base tree, the head tree, and every finding path the
# range's own commits touched, and this script never looks at the working tree.
# Each is a file of finding filenames, one per line, or a directory to list when
# running by hand. They are taken as one SET: a filename in more than one of
# them is one finding and not two, a name that resolves in any of them resolves,
# and a name that matches two distinct findings across them is still ambiguous
# and still refused.
#
# WHY ALL THREE AND NOT JUST THE RANGE. A finding the pull request never touched
# appears in no commit of the range, so the base tree is needed. The head tree
# looks redundant next to base + range and very nearly is -- but `git log
# --name-only` reports nothing for a MERGE commit, so a finding that entered
# this branch through a merge rather than through a commit of its own is in the
# head tree and in neither of the others. It costs one ls-tree, and the
# expensive failure of this rule is a FALSE REFUSAL of a legitimate branch.
#
# With no listing at all only the grammar is checked, which is how the fixtures
# exercise it without a repository. With one listing, that listing alone is the
# set: a caller that has only the base gets the stricter rule and says so by
# passing only the base.
#
# A LISTING THIS SCRIPT CANNOT READ IS A REFUSAL AND NEVER AN EMPTY SET. An
# unreadable listing that read as "no findings here" would silently narrow the
# set, and narrowing it can turn a refusal into an acceptance: two findings match
# a description and the name is ambiguous, one listing goes unreadable, one match
# is left and the name "conforms". Read failures are propagated, separately from
# grep's ordinary no-match status.
#
# GRANDFATHERING. .github/legacy-branches.txt lists the branches that predate
# this rule, one per line. It is a to-do list that shrinks: a listed branch is
# accepted with a warning, and the file reaching zero entries is the signal the
# migration finished. It is not an escape hatch for new work, and adding to it
# is a diff the review and the owner see like any other.

set -euo pipefail
export PATH="/usr/bin:/bin:$PATH"

# `dirname` and not "${BASH_SOURCE[0]%/*}": that expansion strips nothing when
# the script is invoked by bare name from inside its own directory, which is the
# bug .github/scripts/test-pr-policy.sh carries and CLAUDE.md warns about.
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
legacy_file="${LEGACY_BRANCHES:-$script_dir/../legacy-branches.txt}"

branch="${1:-}"
base_findings="${2:-}"
head_findings="${3:-}"
range_findings="${4:-}"

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
  findings/<slug>                 reviews/findings/ alone: filing or curating
  fix-P<n>/<category>_<desc>      exactly one finding, n in 0..3
  bulk-fix-P<n>/<slug>            a batch of findings, n in 2..3

<slug> and <desc> are lower-case words joined by single hyphens, e.g.
`feature/pr9-repair-execution`. <category> is one of correctness,
crash-consistency, security-trust, portability, liveness, performance,
compatibility, docs-contract, and a fix-P*/ branch names one finding:

  reviews/findings/P1_correctness_202609040301_pid-identity-under-a-host-wildcard-waiter.md
  fix-P1/correctness_pid-identity-under-a-host-wildcard-waiter

That finding is looked for anywhere in this pull request's range, not just at
its two ends: the base tree, the head tree, and every finding path the range's
commits touched. A repair that deleted the file is resolved by the base; a pull
request that files the finding and repairs it in one go is resolved by the
range, whichever commit of it the file lived in.

There is no fix/<slug> prefix. A bug worth a branch is worth a finding, so file
the finding and branch fix-P<n>/ after it. findings/<slug> is for a pull request
that touches reviews/findings/ and nothing else; it is not somewhere to put a
repair.

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
#
# `--` IS LOAD-BEARING. Without it a branch name beginning with a dash is read by
# grep as its own options: `-ecodex/findings-p3-1a57a2730a12` becomes `-e` plus a
# LISTED pattern, so a name that is not in the file is granted the exemption and
# returns before the grammar is ever checked. The list is an escape hatch the
# owner intends to delete; a way to inherit an entry without being on it makes
# its contents meaningless.
if [[ -f "$legacy_file" ]] \
  && grep -v '^[[:space:]]*#' "$legacy_file" | grep -qxF -- "$branch"; then
  echo "branch-name-policy: '$branch' predates the branch vocabulary and is" >&2
  echo "  listed in ${legacy_file##*/}. Rename it when it next comes up for merge." >&2
  exit 0
fi

# A listing is checked here as well as where it is read: this catches a caller's
# mistake even for a branch that needs no resolution at all, and it reports the
# listing rather than leaving the reader to infer it from a missing finding.
check_listing() {
  if [[ -z "$1" ]]; then
    return 0
  fi
  if [[ ! -e "$1" ]]; then
    fail "findings listing '$1' is neither a file nor a directory"
  fi
  if [[ ! -r "$1" ]]; then
    fail "findings listing '$1' cannot be read"
  fi
}
check_listing "$base_findings"
check_listing "$head_findings"
check_listing "$range_findings"

# read_listing <listing>: the finding filenames in one listing, one per line,
# from whichever form the caller passed. A read that FAILS returns non-zero and
# never an empty set; the existence test above cannot stand in for this, because
# a listing can be readable when it is checked and unreadable when it is read,
# and a directory can be readable and still not listable.
read_listing() {
  local listing="$1" out
  if [[ -d "$listing" ]]; then
    out="$(ls -1 -- "$listing")" || return 1
  elif [[ -f "$listing" ]]; then
    out="$(cat -- "$listing")" || return 1
  else
    echo "branch-name-policy: findings listing '$listing' is neither a file nor a directory" >&2
    return 1
  fi
  [[ -z "$out" ]] || printf '%s\n' "$out"
}

# candidate_names: the base, the head and the range taken as one set. `sort -u`
# is what makes a file that appears in more than one listing one finding rather
# than several, which is every fix-P*/ pull request that has not touched
# reviews/findings/ yet.
#
# `|| exit 1` and not `|| return 1`: this block is the left-hand side of a
# pipeline and so a subshell, and a bare `return` from the middle of it would
# leave the LAST listing's status as the block's, hiding a failure on an earlier
# one. `exit` ends the subshell there and `pipefail` carries it out.
candidate_names() {
  {
    if [[ -n "$base_findings" ]]; then read_listing "$base_findings" || exit 1; fi
    if [[ -n "$head_findings" ]]; then read_listing "$head_findings" || exit 1; fi
    if [[ -n "$range_findings" ]]; then read_listing "$range_findings" || exit 1; fi
  } | sort -u
}

# resolve_finding <severity-digit> <category> <description>: the branch claims
# to repair one filed finding. Exactly one filename in that set must carry the
# severity, category and description; the timestamp between them is free.
resolve_finding() {
  local n="$1" cat="$2" desc="$3" names matches count
  [[ -n "$base_findings$head_findings$range_findings" ]] || return 0
  # The set is built FIRST, and a failure to build it refuses. Folding this into
  # the `grep` pipeline below would put a read error and an ordinary no-match on
  # the same footing, and `|| true` would then read an unreadable listing as a
  # listing with nothing in it.
  names="$(candidate_names)" \
    || fail "'$branch' could not be checked: a findings listing could not be read.
  The error is above. A gate that cannot see its input refuses rather than
  deciding it saw nothing, because a narrowed set turns an ambiguous name into
  an accepted one."
  matches="$(grep -E "^P${n}_${cat}_[0-9]+_${desc}\.md$" <<< "$names" || true)"
  count="$(grep -c . <<< "${matches:-}" || true)"
  [[ -n "$matches" ]] || count=0
  case "$count" in
    1) return 0 ;;
    0) fail "'$branch' names no finding anywhere in this pull request's range:
  expected exactly one P${n}_${cat}_<timestamp>_${desc}.md
  A fix-P*/ branch repairs one filed finding and mirrors its severity, category
  and description. If this bug was never filed, file it in this pull request:
  the whole range is read, so filing it in one commit and repairing it in the
  next resolves the name even though the repair deletes the file again." ;;
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
  fix/*)
    # A retired prefix says so. It was in the vocabulary until the finding was
    # allowed to be filed by the pull request that repairs it, and a bare "not a
    # known prefix" would send its author looking for a typo.
    fail "'fix/' was retired from the vocabulary: a repair names the finding it
  closes. File the finding under reviews/findings/ if it is not filed already,
  and branch fix-P<n>/<category>_<description> after it. The listing is read at
  the base AND at the head, so filing and repairing in one pull request works.
  A pull request that only files or curates findings is findings/<slug>."
    ;;
  feature/*|refactor/*|docs/*|standards/*|ci/*|gate/*|findings/*)
    [[ "$rest" =~ ^${slug_re}$ ]] \
      || fail "'$branch' must be lower-case words joined by single hyphens after the prefix"
    ;;
  *)
    fail "'${branch%%/*}/' is not a known branch prefix"
    ;;
esac

echo "branch-name-policy: '$branch' conforms"
