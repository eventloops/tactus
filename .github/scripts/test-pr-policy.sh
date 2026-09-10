#!/usr/bin/env bash
set -euo pipefail
export PATH="/usr/bin:/bin:$PATH"

script_dir="${BASH_SOURCE[0]%/*}"
root="$(cd "$script_dir/../.." && pwd)"
validator="$root/.github/scripts/validate-pr-body.sh"
title='fix(review): enforce the finding ledger'

prefix=$'## Summary\n\nSummary.\n\n## Scope\n\nScope.\n\n## Validation\n\nValidation.\n\n## Review evidence\n\nEvidence.\n\n## Risk and rollback\n\nRisk.\n\n## Review finding ledger\n\n'
header='| ID | Severity | Reviewed SHA / location | Failure sequence | Provenance | Category | First bad / prior ID | Regression or documented guard | Disposition |'
separator='|---|---|---|---|---|---|---|---|---|'
none_row='| None yet | — | — | — | — | — | — | — | — |'
finding_row='| PR7-001 | P1 | 0123456789abcdef0123456789abcdef01234567 / src/engine.rs:42 | crash after settlement -> replay loses decision -> old rung runs again | pre_existing | crash-consistency | abcdef0 / PR6-009 | `resume_repairs_attempt_transition` | fixed |'

expect_pass() {
  local name="$1"
  local body="$2"
  if ! printf '%s\n' "$body" | "$BASH" "$validator" "$title"; then
    echo "expected pass: $name" >&2
    exit 1
  fi
}

expect_fail() {
  local name="$1"
  local body="$2"
  if printf '%s\n' "$body" | "$BASH" "$validator" "$title" >/dev/null 2>&1; then
    echo "expected failure: $name" >&2
    exit 1
  fi
}

valid_none="$prefix$header"$'\n'"$separator"$'\n'"$none_row"
valid_finding="$prefix$header"$'\n'"$separator"$'\n'"$finding_row"
expect_pass 'empty canonical ledger' "$valid_none"
expect_pass 'fully classified finding' "$valid_finding"

expect_fail 'hyphenated provenance alias' "${valid_finding/pre_existing/pre-existing}"
expect_fail 'unsupported category alias' "${valid_finding/crash-consistency/test-reliability}"
expect_fail 'short reviewed SHA' "${valid_finding/0123456789abcdef0123456789abcdef01234567/01234567}"
expect_fail 'missing failure sequence' "${valid_finding/crash after settlement -> replay loses decision -> old rung runs again/—}"
expect_fail 'malformed ledger header' "${valid_finding/Failure sequence/Failure mode}"
expect_fail 'missing prevention record' "${valid_finding/\`resume_repairs_attempt_transition\`/—}"
expect_fail 'mixed none and finding rows' "$valid_none"$'\n'"$finding_row"
expect_fail 'canonical table outside ledger section' "$header"$'\n'"$separator"$'\n'"$finding_row"$'\n\n'"$prefix"'No table here.'


# ---- the branch vocabulary ----------------------------------------------------------------
#
# validate-pr-branch.sh is exercised here rather than in its own gate because it
# is part of the pull-request policy and CI already runs this file. The findings
# listings are fixtures, not the repository's own: these cases must not change
# meaning when a finding is filed or repaired.

branch_validator="$root/.github/scripts/validate-pr-branch.sh"
fixture_dir="$(mktemp -d)"
# The permission cases leave a mode-600 directory behind when one of them
# fails, and `rm -rf` cannot descend into it.
trap 'chmod -R u+rwX "$fixture_dir" 2>/dev/null; rm -rf "$fixture_dir"' EXIT

# The merge-base listing: what reviews/findings/ holds at the branch point --
# the commit the branch was cut from, and NOT the target branch's current head.
cat > "$fixture_dir/findings.txt" <<'EOF'
P1_correctness_202609040301_pid-identity-under-a-host-wildcard-waiter.md
P2_docs-contract_202609051200_readme-claims-unperformed-migrations.md
P3_liveness_202609061330_a-drain-that-never-returns.md
P3_liveness_202609061331_twinned-description.md
P3_liveness_202609071400_twinned-description.md
P2_performance_202609061000_split-twin.md
EOF

# The head listing: the same directory in the pull request's own tree. It has
# repaired the P1 and so DELETED its file, filed one finding of its own, and
# replaced the split twin with a second one carrying the same description.
cat > "$fixture_dir/head-findings.txt" <<'EOF'
P2_docs-contract_202609051200_readme-claims-unperformed-migrations.md
P3_liveness_202609061330_a-drain-that-never-returns.md
P3_liveness_202609061331_twinned-description.md
P3_liveness_202609071400_twinned-description.md
P2_correctness_202609081500_filed-by-the-pull-request-that-repairs-it.md
P2_performance_202609071000_split-twin.md
EOF

# The pull request's own commits: reviews/findings/ as each commit between the
# merge base and the head left it. A finding filed in one commit and deleted by
# its repair in the next appears here TWICE -- once for the commit that held it
# and once for a second commit that did -- and in NEITHER endpoint tree.
cat > "$fixture_dir/range-findings.txt" <<'EOF'
P2_correctness_202609101200_filed-and-repaired-in-one-range.md
P2_correctness_202609101200_filed-and-repaired-in-one-range.md
EOF

# The migration list is `<pull-request number> <head branch>`, and an entry is
# the pull request rather than the name.
cat > "$fixture_dir/legacy.txt" <<'EOF'
# a comment, and a blank line, are not branches

222 codex/findings-p3-1a57a2730a12
135 sweep/workspace-manager-fixture
EOF

# branch_pass / branch_fail resolve against the MERGE-BASE listing alone, which
# is what a caller that passes one listing gets.
branch_pass() {
  local name="$1" branch="$2"
  if ! PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
    "$BASH" "$branch_validator" "$branch" "$fixture_dir/findings.txt" >/dev/null 2>&1; then
    echo "expected branch to pass: $name ($branch)" >&2
    exit 1
  fi
}

branch_fail() {
  local name="$1" branch="$2"
  if PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
    "$BASH" "$branch_validator" "$branch" "$fixture_dir/findings.txt" >/dev/null 2>&1; then
    echo "expected branch to fail: $name ($branch)" >&2
    exit 1
  fi
}

# pair_pass / pair_fail resolve against BOTH ends, which is what the workflow
# passes and what a real pull request is judged by.
pair_pass() {
  local name="$1" branch="$2"
  if ! PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
    "$BASH" "$branch_validator" "$branch" \
    "$fixture_dir/findings.txt" "$fixture_dir/head-findings.txt" >/dev/null 2>&1; then
    echo "expected branch to pass against base and head: $name ($branch)" >&2
    exit 1
  fi
}

pair_fail() {
  local name="$1" branch="$2"
  if PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
    "$BASH" "$branch_validator" "$branch" \
    "$fixture_dir/findings.txt" "$fixture_dir/head-findings.txt" >/dev/null 2>&1; then
    echo "expected branch to fail against base and head: $name ($branch)" >&2
    exit 1
  fi
}

# triple_pass / triple_fail pass the base tree, the head tree and the range,
# which is what the workflow passes and what a real pull request is judged by.
triple_pass() {
  local name="$1" branch="$2"
  if ! PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
    "$BASH" "$branch_validator" "$branch" "$fixture_dir/findings.txt" \
    "$fixture_dir/head-findings.txt" "$fixture_dir/range-findings.txt" >/dev/null 2>&1; then
    echo "expected branch to pass over the range: $name ($branch)" >&2
    exit 1
  fi
}

triple_fail() {
  local name="$1" branch="$2"
  if PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
    "$BASH" "$branch_validator" "$branch" "$fixture_dir/findings.txt" \
    "$fixture_dir/head-findings.txt" "$fixture_dir/range-findings.txt" >/dev/null 2>&1; then
    echo "expected branch to fail over the range: $name ($branch)" >&2
    exit 1
  fi
}

# Every prefix in the vocabulary, so removing one is a failing test and not a
# silent loosening.
branch_pass 'feature'   'feature/pr9-repair-execution'
branch_pass 'refactor'  'refactor/describe-attempt-finished'
branch_pass 'docs'      'docs/pr8-records-off-master'
branch_pass 'standards' 'standards/w10-run-census'
branch_pass 'ci'        'ci/pages-nojekyll'
branch_pass 'gate'      'gate/g3'
branch_pass 'findings'  'findings/pr8-review-round'
branch_pass 'single digit slug word' 'feature/w10-census'

# findings/<slug> is a prefix like any other after the slash, and it is for a
# pull request that touches reviews/findings/ and nothing else. The validator
# sees a name and not a diff, so that half is a review duty; the grammar is not.
branch_fail 'findings empty name'  'findings/'
branch_fail 'findings upper case'  'findings/PR8-Review'
branch_fail 'findings underscore'  'findings/pr8_review_round'

# fix/<slug> was retired when the finding was allowed to be filed by the pull
# request that repairs it. These are the three branches open at the time it was
# retired, and each must now fail rather than be silently accepted.
branch_fail 'retired fix, #232' 'fix/audit-reviewer-identity'
branch_fail 'retired fix, #139' 'fix/rundir-unreadable-is-not-empty'
branch_fail 'retired fix, #145' 'fix/sampler-kill-and-inspection'

# An unrecognised prefix must fail rather than fall into a lane. This is the
# whole point of the validator: `feat/` is the near miss master's own history
# carries, and `codex/` is what every findings branch used before the rule.
branch_fail 'unknown prefix'      'chore/tidy-the-tree'
branch_fail 'conventional feat'   'feat/pr8-integration-transactions'
branch_fail 'agent name'          'codex/findings-114885184183'
branch_fail 'no prefix at all'    'justabranch'
branch_fail 'empty branch'        ''
branch_fail 'empty name'          'feature/'
branch_fail 'upper case'          'feature/PR9-Repair'
branch_fail 'underscore in slug'  'feature/pr9_repair_execution'
branch_fail 'double hyphen'       'feature/pr9--repair'
branch_fail 'leading hyphen'      'feature/-pr9'
branch_fail 'trailing hyphen'     'feature/pr9-'
branch_fail 'nested path'         'feature/pr9/repair'

# fix-P<n>/ names exactly one filed finding.
branch_pass 'fix-P1 resolves'       'fix-P1/correctness_pid-identity-under-a-host-wildcard-waiter'
branch_pass 'fix-P2 resolves'       'fix-P2/docs-contract_readme-claims-unperformed-migrations'
branch_pass 'hyphenated category'   'fix-P3/liveness_a-drain-that-never-returns'
branch_fail 'no such finding'       'fix-P1/correctness_a-finding-that-was-never-filed'
branch_fail 'wrong severity'        'fix-P3/correctness_pid-identity-under-a-host-wildcard-waiter'
branch_fail 'wrong category'        'fix-P1/liveness_pid-identity-under-a-host-wildcard-waiter'
branch_fail 'unknown category'      'fix-P1/flakiness_pid-identity-under-a-host-wildcard-waiter'
branch_fail 'no category separator' 'fix-P1/correctness-pid-identity-under-a-host-wildcard-waiter'
branch_fail 'severity out of range' 'fix-P9/correctness_pid-identity-under-a-host-wildcard-waiter'
# Two findings share a description across timestamps: the branch names both, and
# an ambiguous claim is refused rather than resolved to whichever sorts first.
branch_fail 'ambiguous description' 'fix-P3/liveness_twinned-description'

# The finding is resolved at the merge base OR at the head, and each end admits
# a pull request the other refuses.
#
# The merge-base end: the pull request repaired the P1 and its file is gone from the
# head, which is what every fix-P*/ pull request looks like once it has done its
# job. Resolving at the head alone would fail exactly those.
pair_pass 'repaired, gone from the head' 'fix-P1/correctness_pid-identity-under-a-host-wildcard-waiter'
# The head end: the finding was filed by this pull request, so it is at the head
# and not at the base. Resolving at the base alone would force one pull request
# to file it and a second to repair it, which is what retiring fix/ would
# otherwise have cost.
pair_pass 'filed by this pull request' 'fix-P2/correctness_filed-by-the-pull-request-that-repairs-it'
branch_fail 'filed at the head is not at the merge base' 'fix-P2/correctness_filed-by-the-pull-request-that-repairs-it'
# Untouched findings are at both ends. One filename in two listings is one
# finding, not two, so the ordinary case must not read as ambiguous.
pair_pass 'present at both ends'   'fix-P2/docs-contract_readme-claims-unperformed-migrations'
pair_pass 'present at both ends 2' 'fix-P3/liveness_a-drain-that-never-returns'
# The case that must still fail: named at neither end. This is the whole claim
# the prefix makes, and loosening the resolution must not have dropped it.
pair_fail 'named at neither end' 'fix-P2/correctness_never-filed-at-either-end'
# The head is matched with the same strictness as the base, severity included.
pair_fail 'wrong severity at the head' 'fix-P3/correctness_filed-by-the-pull-request-that-repairs-it'
# Ambiguity is judged over the set and not over each end: one twin at the merge
# base and a different one at the head resolve alone but not together, and a
# name that could mean either finding is refused rather than picked.
branch_pass 'split twin, merge base alone'  'fix-P2/performance_split-twin'
pair_fail   'split twin across ends'  'fix-P2/performance_split-twin'
pair_fail   'ambiguous at both ends'  'fix-P3/liveness_twinned-description'

# ---- the range, and not the two endpoints -------------------------------------------------
#
# The endpoints are not enough. A pull request that files a finding in one commit
# and repairs it in the next -- deleting the file, as reviews/findings/README.md
# requires -- has the finding at NEITHER end, and that is precisely the
# single-pull-request path retiring fix/ depends on. Keeping the file to satisfy
# the check is not an answer: it leaves finished work in the outstanding queue.
triple_pass 'filed and repaired inside the range' 'fix-P2/correctness_filed-and-repaired-in-one-range'
pair_fail   'the same name at the endpoints alone' 'fix-P2/correctness_filed-and-repaired-in-one-range'
# The add and the delete are two lines naming ONE file. `sort -u` in the
# validator is what keeps that one finding rather than two, so the pass above is
# also the guard on it: without the dedup the range reads as ambiguous.
#
# Resolving over the range must not have become a rubber stamp.
triple_fail 'in no tree and no commit'    'fix-P2/correctness_never-filed-at-all'
triple_fail 'wrong severity in the range' 'fix-P3/correctness_filed-and-repaired-in-one-range'
triple_fail 'wrong category in the range' 'fix-P2/liveness_filed-and-repaired-in-one-range'

# ---- a listing that cannot be read is a refusal, never an empty set -----------------------
#
# An unreadable listing read as "nothing here" NARROWS the candidate set, and a
# narrowed set turns a refusal into an acceptance: two findings match a
# description so the name is ambiguous, one listing goes unreadable, one match is
# left and the name "conforms". Root can read anything, so the permission cases
# only mean something as an ordinary user.
unreadable="$fixture_dir/unreadable.txt"
cp "$fixture_dir/head-findings.txt" "$unreadable"
if [[ "$(id -u)" -ne 0 ]] && chmod 000 "$unreadable" 2>/dev/null && [[ ! -r "$unreadable" ]]; then
  # The review's own reproduction: both listings readable is an ambiguous
  # refusal, and making one unreadable must not leave a single match behind.
  if PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
    "$BASH" "$branch_validator" 'fix-P2/performance_split-twin' \
    "$fixture_dir/findings.txt" "$unreadable" >/dev/null 2>&1; then
    echo 'expected an unreadable second listing to refuse, not to conform' >&2
    exit 1
  fi
  # And a failure on the FIRST listing must not be masked by a good second one
  # that resolves the name on its own.
  if PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
    "$BASH" "$branch_validator" 'fix-P2/correctness_filed-by-the-pull-request-that-repairs-it' \
    "$unreadable" "$fixture_dir/head-findings.txt" >/dev/null 2>&1; then
    echo 'expected an unreadable first listing to refuse, not to be masked' >&2
    exit 1
  fi
  chmod 644 "$unreadable"
else
  echo 'note: skipping the unreadable-listing cases (running as root, or chmod had no effect)' >&2
fi

# A listing that exists and is readable and is still not a listing. This one
# holds whoever is running the suite, root included, and it is the case the
# existence-and-permission checks cannot see: the refusal has to come from the
# read itself.
if [[ -c /dev/null ]]; then
  if PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
    "$BASH" "$branch_validator" 'fix-P1/correctness_pid-identity-under-a-host-wildcard-waiter' \
    "$fixture_dir/findings.txt" /dev/null >/dev/null 2>&1; then
    echo 'expected a listing that is neither a file nor a directory to refuse' >&2
    exit 1
  fi
fi

# bulk-fix-P<n>/ carries no finding, and never batches a severity that is
# repaired one at a time.
branch_pass 'bulk P3' 'bulk-fix-P3/docs-fixes'
branch_pass 'bulk P2' 'bulk-fix-P2/security-trust-sweep'
branch_fail 'bulk P1' 'bulk-fix-P1/correctness-fixes'
branch_fail 'bulk P0' 'bulk-fix-P0/correctness-fixes'
branch_fail 'bulk unknown severity' 'bulk-fix-P7/correctness-fixes'
branch_fail 'bulk upper case' 'bulk-fix-P3/Docs-Fixes'

# The grammar holds with no findings listing, which is how a caller with no
# repository checks a name.
if ! PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
  "$BASH" "$branch_validator" 'fix-P1/correctness_never-filed' >/dev/null 2>&1; then
  echo 'expected the grammar alone to pass without a findings listing' >&2
  exit 1
fi

# A listing that does not exist is a caller error, and it must be refused before
# the resolution runs rather than read as a finding that was never filed. The
# branch here needs no resolution at all, so only an eager check fails it.
if PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
  "$BASH" "$branch_validator" 'feature/pr9-repair-execution' \
  "$fixture_dir/no-such-listing" >/dev/null 2>&1; then
  echo 'expected a findings listing that does not exist to fail' >&2
  exit 1
fi

# ---- an exemption is a pull request, not a name -------------------------------------------
#
# Keyed on the branch name alone, the list exempted anybody who typed it: nothing
# stops a fork creating `codex/findings-p3-1a57a2730a12` today and opening a NEW
# pull request, and a lookup handed only that name cannot tell it from the pull
# request the entry was written for. The list's contents would then no longer
# decide who is exempt, and the population would no longer be the migration it
# claims to describe. Both fields must match the same line.
legacy_pass() {  # legacy_pass <name> <pr-number> <branch>
  local name="$1" pr="$2" branch="$3"
  if ! PR_NUMBER="$pr" LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
    "$BASH" "$branch_validator" "$branch" >/dev/null 2>&1; then
    echo "expected the exemption to apply: $name (#$pr $branch)" >&2
    exit 1
  fi
}

legacy_fail() {  # legacy_fail <name> <pr-number> <branch>
  local name="$1" pr="$2" branch="$3"
  if PR_NUMBER="$pr" LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
    "$BASH" "$branch_validator" "$branch" >/dev/null 2>&1; then
    echo "expected the exemption NOT to apply: $name (#$pr $branch)" >&2
    exit 1
  fi
}

legacy_pass 'the pull request the entry was written for' 222 'codex/findings-p3-1a57a2730a12'
legacy_pass 'the second listed pull request'             135 'sweep/workspace-manager-fixture'
# The one that matters: the same branch name, a different pull request. This is
# every future pull request, fork or not, that reuses a listed name.
legacy_fail 'the same name, a new pull request'          999 'codex/findings-p3-1a57a2730a12'
legacy_fail 'a listed number, a different branch'        222 'codex/findings-p3-deadbeefcafe'
legacy_fail 'the numbers crossed over'                   135 'codex/findings-p3-1a57a2730a12'
# No identity is no exemption. A caller checking a name by hand is not judging a
# pull request, and the safe answer for one is the rule itself.
legacy_fail 'no pull-request identity'                   ''  'codex/findings-p3-1a57a2730a12'
legacy_fail 'a non-numeric identity'                     'abc' 'codex/findings-p3-1a57a2730a12'
# The fields are compared and never handed to a pattern matcher, so a name that
# begins with a dash is a name and not a set of grep options.
legacy_fail 'short option injection'  222 '-ecodex/findings-p3-1a57a2730a12'
legacy_fail 'long option injection'   222 '--regexp=codex/findings-p3-1a57a2730a12'
# Still an exact match on the branch field, and comments are still not entries.
legacy_fail 'legacy as a substring'   222 'codex/findings-p3-1a57a2730a12-extra'
legacy_fail 'legacy comment line'     222 '# a comment, and a blank line, are not branches'
legacy_fail 'unlisted codex branch'   222 'codex/findings-p3-deadbeefcafe'

# ---- the listings themselves, built from real repositories ---------------------------------
#
# Everything above tests what the validator does with a listing. NOTHING above
# tests whether the listing is right, and that is where two frontier reviews
# found defects: the range was built with `git log -- reviews/findings/`, which
# answers which commits CHANGED the path after simplification rather than which
# findings EXISTED, and it missed them two ways. So these build real
# repositories and call .github/scripts/findings-in-range.sh, the script the
# workflow calls.
#
# A missed finding is not only a false red on a valid branch. It is a false
# GREEN on an ambiguous one, which is the failure this gate exists to prevent.

range_script="$root/.github/scripts/findings-in-range.sh"

new_repo() {  # new_repo <dir>
  mkdir -p "$1"
  git -C "$1" init -q .
  git -C "$1" config user.email fixture@example.invalid
  git -C "$1" config user.name 'fixture'
  git -C "$1" config commit.gpgsign false
  git -C "$1" config gc.auto 0
}

commit_finding() {  # commit_finding <dir> <filename> <message>
  mkdir -p "$1/reviews/findings"
  echo fixture > "$1/reviews/findings/$2"
  git -C "$1" add -A
  git -C "$1" commit -q -m "$3"
}

range_listing() {  # range_listing <dir> <target> <head> -> the range-findings lines
  ( cd "$1" && "$BASH" "$range_script" "$2" "$3" "$1/out" >/dev/null 2>&1 ) || return 1
  cat "$1/out/range-findings"
}

# verdict <dir> <target> <head> <branch>: build the three listings from that
# repository and print the validator's exit code. 99 means the listings could
# not be built at all, so a construction failure can never read as a verdict.
verdict() {
  local dir="$1" target="$2" head="$3" branch="$4" out rc=0
  out="$(mktemp -d "$fixture_dir/verdict-XXXXXX")"
  if ! ( cd "$dir" && "$BASH" "$range_script" "$target" "$head" "$out" ) >/dev/null 2>&1; then
    echo 99
    return 0
  fi
  PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
    "$BASH" "$branch_validator" "$branch" \
    "$out/merge-base-findings" "$out/head-findings" "$out/range-findings" \
    >/dev/null 2>&1 || rc=$?
  echo "$rc"
}

# A completed repair: commit A files the finding, commit B repairs it and
# DELETES the file as reviews/findings/README.md requires, and the whole thing
# is merged into the pull request's branch after an unrelated commit. The
# finding is in neither endpoint tree, and history simplification prunes the
# side branch entirely because its net effect on the path is nothing.
repo_a="$fixture_dir/repo-merged-repair"
new_repo "$repo_a"
echo seed > "$repo_a/seed.txt"
git -C "$repo_a" add -A && git -C "$repo_a" commit -q -m base
a_base="$(git -C "$repo_a" rev-parse HEAD)"
git -C "$repo_a" checkout -q -b side "$a_base"
commit_finding "$repo_a" 'P2_correctness_202609101200_a-new-bug.md' 'A: file the finding'
git -C "$repo_a" rm -q "reviews/findings/P2_correctness_202609101200_a-new-bug.md"
git -C "$repo_a" commit -q -m 'B: repair it and delete the finding'
git -C "$repo_a" checkout -q -b trunk "$a_base"
echo unrelated > "$repo_a/other.txt"
git -C "$repo_a" add -A && git -C "$repo_a" commit -q -m unrelated
git -C "$repo_a" merge -q --no-ff side -m 'merge the completed repair'
a_head="$(git -C "$repo_a" rev-parse HEAD)"

got="$(range_listing "$repo_a" "$a_base" "$a_head")" \
  || { echo 'findings-in-range.sh failed on the merged-repair repository' >&2; exit 1; }
if [[ "$got" != 'P2_correctness_202609101200_a-new-bug.md' ]]; then
  echo "the range must hold a finding filed and repaired on a merged side branch; got [$got]" >&2
  exit 1
fi

# The same shape, with a SECOND finding of the same description on the receiving
# branch. Miss the side branch's one and the name resolves to a single match and
# conforms; see both and it is ambiguous, which is what it is.
repo_b="$fixture_dir/repo-hidden-twin"
new_repo "$repo_b"
echo seed > "$repo_b/seed.txt"
git -C "$repo_b" add -A && git -C "$repo_b" commit -q -m base
b_base="$(git -C "$repo_b" rev-parse HEAD)"
git -C "$repo_b" checkout -q -b side "$b_base"
commit_finding "$repo_b" 'P2_correctness_202609101200_a-new-bug.md' 'A: file the finding'
git -C "$repo_b" rm -q "reviews/findings/P2_correctness_202609101200_a-new-bug.md"
git -C "$repo_b" commit -q -m 'B: repair it and delete the finding'
git -C "$repo_b" checkout -q -b trunk "$b_base"
commit_finding "$repo_b" 'P2_correctness_202609111500_a-new-bug.md' 'the receiving branch files its own'
git -C "$repo_b" merge -q --no-ff side -m 'merge the completed repair'
b_head="$(git -C "$repo_b" rev-parse HEAD)"

( cd "$repo_b" && "$BASH" "$range_script" "$b_base" "$b_head" "$repo_b/out" >/dev/null 2>&1 ) \
  || { echo 'findings-in-range.sh failed on the hidden-twin repository' >&2; exit 1; }
if PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
  "$BASH" "$branch_validator" 'fix-P2/correctness_a-new-bug' \
  "$repo_b/out/merge-base-findings" "$repo_b/out/head-findings" "$repo_b/out/range-findings" \
  >/dev/null 2>&1; then
  echo 'a finding hidden on a merged side branch made an ambiguous name conform' >&2
  exit 1
fi

# A finding created and deleted ONLY inside merge commits. `git log` reports no
# paths for a merge unless per-parent diffs are asked for, so this one is
# invisible to it even under --full-history. Listing trees does not care.
repo_c="$fixture_dir/repo-merge-only"
new_repo "$repo_c"
echo seed > "$repo_c/seed.txt"
git -C "$repo_c" add -A && git -C "$repo_c" commit -q -m base
c_base="$(git -C "$repo_c" rev-parse HEAD)"
git -C "$repo_c" checkout -q -b p1 "$c_base"
echo a > "$repo_c/a.txt"; git -C "$repo_c" add -A; git -C "$repo_c" commit -q -m c1
git -C "$repo_c" checkout -q -b trunk "$c_base"
echo b > "$repo_c/b.txt"; git -C "$repo_c" add -A; git -C "$repo_c" commit -q -m c2
git -C "$repo_c" merge -q --no-commit --no-ff p1 >/dev/null 2>&1 || true
mkdir -p "$repo_c/reviews/findings"
echo fixture > "$repo_c/reviews/findings/P2_correctness_202609101200_only-in-merges.md"
git -C "$repo_c" add -A
git -C "$repo_c" commit -q -m 'M1: a merge that files the finding in the merge itself'
git -C "$repo_c" checkout -q -b q
echo c > "$repo_c/c.txt"; git -C "$repo_c" add -A; git -C "$repo_c" commit -q -m c3
git -C "$repo_c" checkout -q trunk
echo d > "$repo_c/d.txt"; git -C "$repo_c" add -A; git -C "$repo_c" commit -q -m c4
git -C "$repo_c" merge -q --no-commit --no-ff q >/dev/null 2>&1 || true
git -C "$repo_c" rm -q "reviews/findings/P2_correctness_202609101200_only-in-merges.md"
git -C "$repo_c" commit -q -m 'M2: a merge that removes it in the merge itself'
c_head="$(git -C "$repo_c" rev-parse HEAD)"

got="$(range_listing "$repo_c" "$c_base" "$c_head")" \
  || { echo 'findings-in-range.sh failed on the merge-only repository' >&2; exit 1; }
if [[ "$got" != 'P2_correctness_202609101200_only-in-merges.md' ]]; then
  echo "the range must hold a finding that lived only inside merge commits; got [$got]" >&2
  exit 1
fi

# The merge-base tree is listed separately: the pull request's own commits
# exclude it, so a finding this pull request never touched lives only there.
repo_d="$fixture_dir/repo-untouched"
new_repo "$repo_d"
commit_finding "$repo_d" 'P1_liveness_202609010900_untouched-by-this-branch.md' 'base files a finding'
d_base="$(git -C "$repo_d" rev-parse HEAD)"
echo unrelated > "$repo_d/other.txt"
git -C "$repo_d" add -A && git -C "$repo_d" commit -q -m 'the branch changes something else'
d_head="$(git -C "$repo_d" rev-parse HEAD)"
( cd "$repo_d" && "$BASH" "$range_script" "$d_base" "$d_head" "$repo_d/out" >/dev/null 2>&1 ) \
  || { echo 'findings-in-range.sh failed on the untouched-finding repository' >&2; exit 1; }
if ! PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
  "$BASH" "$branch_validator" 'fix-P1/liveness_untouched-by-this-branch' \
  "$repo_d/out/merge-base-findings" "$repo_d/out/head-findings" "$repo_d/out/range-findings" \
  >/dev/null 2>&1; then
  echo 'a finding the branch never touched must still resolve, from the base tree' >&2
  exit 1
fi

# And an unresolvable end fails closed rather than being dropped.
if ( cd "$repo_d" && "$BASH" "$range_script" \
  '0000000000000000000000000000000000000000' "$d_head" "$repo_d/out2" ) >/dev/null 2>&1; then
  echo 'expected a base commit that is not in the checkout to fail' >&2
  exit 1
fi

# ---- nothing outside the pull request may change the verdict ------------------------------
#
# The listings were rooted at the EVENT'S BASE SHA, which is the target branch's
# head at the moment of the event and moves for reasons that have nothing to do
# with the pull request. Both repositories below are run twice against the SAME
# HEAD -- once with the target at the branch point, once with it advanced -- and
# the two verdicts must be equal. Rooted at the event's base they were not.
#
# The shape is a DIVERGENT BOUNDARY: a finding that exists at the branch point,
# is repaired by the pull request, and is independently deleted on master. Once
# master has moved, `<base>..<head>` no longer reaches the branch point and the
# finding is in no listing at all.

# One description, two findings, one of them repaired here and deleted there:
# losing it leaves a single match and an AMBIGUOUS name conforms, which is the
# acceptance this whole gate exists to prevent.
repo_e="$fixture_dir/repo-divergent-ambiguous"
new_repo "$repo_e"
mkdir -p "$repo_e/reviews/findings"
echo one > "$repo_e/reviews/findings/P2_correctness_202609010000_shared-name.md"
echo two > "$repo_e/reviews/findings/P2_correctness_202609020000_shared-name.md"
git -C "$repo_e" add -A && git -C "$repo_e" commit -q -m 'branch point: two findings share a description'
e_branch_point="$(git -C "$repo_e" rev-parse HEAD)"
git -C "$repo_e" checkout -q -b pr
git -C "$repo_e" rm -q 'reviews/findings/P2_correctness_202609010000_shared-name.md'
git -C "$repo_e" commit -q -m 'the pull request repairs the first, deleting the file'
e_head="$(git -C "$repo_e" rev-parse HEAD)"
git -C "$repo_e" checkout -q -B trunk "$e_branch_point"
git -C "$repo_e" rm -q 'reviews/findings/P2_correctness_202609010000_shared-name.md'
git -C "$repo_e" commit -q -m 'master deletes the same finding, independently'
e_advanced="$(git -C "$repo_e" rev-parse HEAD)"

e_before="$(verdict "$repo_e" "$e_branch_point" "$e_head" 'fix-P2/correctness_shared-name')"
e_after="$(verdict "$repo_e" "$e_advanced" "$e_head" 'fix-P2/correctness_shared-name')"
if [[ "$e_before" != "$e_after" ]]; then
  echo "advancing the target changed the verdict on one head: $e_before then $e_after" >&2
  exit 1
fi
if [[ "$e_before" != 1 ]]; then
  echo "an ambiguous name must be refused at either target; got $e_before" >&2
  exit 1
fi

# The same boundary, one finding rather than two: losing it is a false RED on a
# pull request that did exactly the right thing.
repo_f="$fixture_dir/repo-divergent-repair"
new_repo "$repo_f"
commit_finding "$repo_f" 'P2_correctness_202609010000_repaired-both-sides.md' 'branch point files it'
f_branch_point="$(git -C "$repo_f" rev-parse HEAD)"
git -C "$repo_f" checkout -q -b pr
git -C "$repo_f" rm -q 'reviews/findings/P2_correctness_202609010000_repaired-both-sides.md'
git -C "$repo_f" commit -q -m 'the pull request repairs it, deleting the file'
f_head="$(git -C "$repo_f" rev-parse HEAD)"
git -C "$repo_f" checkout -q -B trunk "$f_branch_point"
git -C "$repo_f" rm -q 'reviews/findings/P2_correctness_202609010000_repaired-both-sides.md'
git -C "$repo_f" commit -q -m 'master deletes it too'
f_advanced="$(git -C "$repo_f" rev-parse HEAD)"

f_before="$(verdict "$repo_f" "$f_branch_point" "$f_head" 'fix-P2/correctness_repaired-both-sides')"
f_after="$(verdict "$repo_f" "$f_advanced" "$f_head" 'fix-P2/correctness_repaired-both-sides')"
if [[ "$f_before" != "$f_after" ]]; then
  echo "advancing the target changed the verdict on one head: $f_before then $f_after" >&2
  exit 1
fi
if [[ "$f_before" != 0 ]]; then
  echo "a repaired finding must resolve from the merge base at either target; got $f_before" >&2
  exit 1
fi

# The merge base is also what keeps the pull request's OWN commits in view: the
# filed-and-repaired finding is in neither endpoint tree, and a listing built
# from the two-tree diff against the merge base is EMPTY for it, because the add
# and the delete cancel. Measured here, not asserted.
repo_g="$fixture_dir/repo-filed-and-repaired"
new_repo "$repo_g"
echo seed > "$repo_g/seed.txt"
git -C "$repo_g" add -A && git -C "$repo_g" commit -q -m base
g_base="$(git -C "$repo_g" rev-parse HEAD)"
commit_finding "$repo_g" 'P2_correctness_202609101200_a-new-bug.md' 'A: file the finding'
git -C "$repo_g" rm -q 'reviews/findings/P2_correctness_202609101200_a-new-bug.md'
git -C "$repo_g" commit -q -m 'B: repair it and delete the finding'
g_head="$(git -C "$repo_g" rev-parse HEAD)"
if [[ -n "$(git -C "$repo_g" diff --name-only "$g_base" "$g_head" -- reviews/findings/)" ]]; then
  echo 'the two-tree diff was expected to be empty for a filed-and-repaired finding' >&2
  exit 1
fi
g_verdict="$(verdict "$repo_g" "$g_base" "$g_head" 'fix-P2/correctness_a-new-bug')"
if [[ "$g_verdict" != 0 ]]; then
  echo "a finding filed and repaired inside the pull request must resolve; got $g_verdict" >&2
  exit 1
fi

# ---- a finding is a regular file, not a directory wearing its name -------------------------
#
# `git ls-tree --name-only` does not say whether an entry is a file or a tree,
# so committing reviews/findings/P2_correctness_<ts>_<desc>.md/placeholder --
# which creates a DIRECTORY and no finding -- satisfied fix-P2/correctness_<desc>
# and all three workflow steps returned exit 0 with no finding in existence.
repo_h="$fixture_dir/repo-directory-not-a-finding"
new_repo "$repo_h"
echo seed > "$repo_h/seed.txt"
git -C "$repo_h" add -A && git -C "$repo_h" commit -q -m base
h_base="$(git -C "$repo_h" rev-parse HEAD)"
mkdir -p "$repo_h/reviews/findings/P2_correctness_202609101200_missing-repair.md"
echo placeholder > "$repo_h/reviews/findings/P2_correctness_202609101200_missing-repair.md/placeholder"
# A real finding beside it, so the case proves the filter and not an empty tree.
echo fixture > "$repo_h/reviews/findings/P3_liveness_202609101300_a-real-finding.md"
git -C "$repo_h" add -A && git -C "$repo_h" commit -q -m 'a directory named like a finding'
h_head="$(git -C "$repo_h" rev-parse HEAD)"
if ! git -C "$repo_h" ls-tree "$h_head" reviews/findings/ | grep -q '^040000 tree '; then
  echo 'the fixture was meant to commit a TREE named like a finding' >&2
  exit 1
fi
h_verdict="$(verdict "$repo_h" "$h_base" "$h_head" 'fix-P2/correctness_missing-repair')"
if [[ "$h_verdict" != 1 ]]; then
  echo "a directory named like a finding must not resolve a fix-P*/ branch; got $h_verdict" >&2
  exit 1
fi
h_real="$(verdict "$repo_h" "$h_base" "$h_head" 'fix-P3/liveness_a-real-finding')"
if [[ "$h_real" != 0 ]]; then
  echo "the regular file beside it must still resolve; got $h_real" >&2
  exit 1
fi

# The same filter where a DIRECTORY is handed in as the listing, which is how
# the validator is run by hand against a working tree.
dir_input="$fixture_dir/findings-dir"
mkdir -p "$dir_input/P2_correctness_202609101200_missing-repair.md"
echo placeholder > "$dir_input/P2_correctness_202609101200_missing-repair.md/placeholder"
echo fixture > "$dir_input/P3_liveness_202609101300_a-real-finding.md"
if PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
  "$BASH" "$branch_validator" 'fix-P2/correctness_missing-repair' "$dir_input" >/dev/null 2>&1; then
  echo 'a subdirectory named like a finding must not resolve when a directory is the listing' >&2
  exit 1
fi
if ! PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
  "$BASH" "$branch_validator" 'fix-P3/liveness_a-real-finding' "$dir_input" >/dev/null 2>&1; then
  echo 'a regular file in a directory listing must still resolve' >&2
  exit 1
fi

# Filtering a directory down to its regular files must not become a NEW way to
# read a listing as empty. `ls` names the entries of a directory that is
# readable but not searchable, and every regular-file test on those names then
# fails, so the listing narrows to nothing -- and a narrowed set is what turns
# an ambiguous name into an accepted one. The shape is the twin: one match in a
# file listing, the other in the directory. Root can search anything, so this
# only means something as an ordinary user.
twin_dir="$fixture_dir/twin-dir"
mkdir -p "$twin_dir"
echo fixture > "$twin_dir/P2_performance_202609071000_split-twin.md"
if ! PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
  "$BASH" "$branch_validator" 'fix-P2/performance_split-twin' \
  "$fixture_dir/findings.txt" "$twin_dir" >/dev/null 2>&1; then
  : # both twins visible, so the name is ambiguous and refused, which is the control
else
  echo 'the twin across a file listing and a directory listing must be ambiguous' >&2
  exit 1
fi
if [[ "$(id -u)" -ne 0 ]] && chmod 600 "$twin_dir" 2>/dev/null && [[ ! -x "$twin_dir" ]]; then
  if PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
    "$BASH" "$branch_validator" 'fix-P2/performance_split-twin' \
    "$fixture_dir/findings.txt" "$twin_dir" >/dev/null 2>&1; then
    echo 'an unsearchable directory listing narrowed an ambiguous name into a pass' >&2
    exit 1
  fi
  chmod 700 "$twin_dir"
else
  echo 'note: skipping the unsearchable-directory case (running as root, or chmod had no effect)' >&2
fi

# ---- the gate must not recommend a destructive migration ------------------------------------
#
# Renaming a head branch CLOSES its pull request, measured on throwaway #264,
# and the gate told every exempted pull request to do exactly that. The body and
# MAINTAINING.md were corrected and the gate's own output was not, so the
# instruction a maintainer actually reads is the one under test here.
exempt_out="$(PR_NUMBER=222 LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
  "$BASH" "$branch_validator" 'codex/findings-p3-1a57a2730a12' 2>&1)"
if grep -Eqi '^[[:space:]]*rename[[:space:]]' <<< "$exempt_out"; then
  echo 'the exemption message still tells the author to rename the branch' >&2
  exit 1
fi
if ! grep -q 'DO NOT RENAME THE HEAD BRANCH' <<< "$exempt_out"; then
  echo 'the exemption message must say renaming the head branch closes the pull request' >&2
  exit 1
fi
if ! grep -q 'replacement pull request' <<< "$exempt_out"; then
  echo 'the exemption message must name the route that is not destructive' >&2
  exit 1
fi

# And the file the message points at says the same thing, because that is the
# other place the instruction is read.
legacy_shipped="$root/.github/legacy-branches.txt"
if ! grep -q 'DO NOT RENAME A LISTED HEAD BRANCH' "$legacy_shipped"; then
  echo 'legacy-branches.txt must warn that renaming a listed head branch closes it' >&2
  exit 1
fi
if grep -q 'renamed as they come up for merge' "$legacy_shipped"; then
  echo 'legacy-branches.txt still carries the rename-on-merge instruction' >&2
  exit 1
fi

echo 'listing-construction fixtures passed'

echo 'branch vocabulary fixtures passed'

echo 'PR policy fixtures passed'
