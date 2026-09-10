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
trap 'rm -rf "$fixture_dir"' EXIT

# The base listing: what reviews/findings/ holds at the commit the branch was
# cut from.
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

# The range: every finding path the pull request's own commits touched, which is
# what `git log --name-only <base>..<head> -- reviews/findings/` reports. A
# finding filed in one commit and deleted by its repair in the next appears here
# TWICE, once for the add and once for the delete, and in NEITHER endpoint tree.
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

# branch_pass / branch_fail resolve against the BASE listing alone, which is
# what a caller that passes one listing gets.
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

# The finding is resolved at the base OR at the head, and each end admits a pull
# request the other refuses.
#
# The base end: the pull request repaired the P1 and its file is gone from the
# head, which is what every fix-P*/ pull request looks like once it has done its
# job. Resolving at the head alone would fail exactly those.
pair_pass 'repaired, gone from the head' 'fix-P1/correctness_pid-identity-under-a-host-wildcard-waiter'
# The head end: the finding was filed by this pull request, so it is at the head
# and not at the base. Resolving at the base alone would force one pull request
# to file it and a second to repair it, which is what retiring fix/ would
# otherwise have cost.
pair_pass 'filed by this pull request' 'fix-P2/correctness_filed-by-the-pull-request-that-repairs-it'
branch_fail 'filed at the head is not at the base' 'fix-P2/correctness_filed-by-the-pull-request-that-repairs-it'
# Untouched findings are at both ends. One filename in two listings is one
# finding, not two, so the ordinary case must not read as ambiguous.
pair_pass 'present at both ends'   'fix-P2/docs-contract_readme-claims-unperformed-migrations'
pair_pass 'present at both ends 2' 'fix-P3/liveness_a-drain-that-never-returns'
# The case that must still fail: named at neither end. This is the whole claim
# the prefix makes, and loosening the resolution must not have dropped it.
pair_fail 'named at neither end' 'fix-P2/correctness_never-filed-at-either-end'
# The head is matched with the same strictness as the base, severity included.
pair_fail 'wrong severity at the head' 'fix-P3/correctness_filed-by-the-pull-request-that-repairs-it'
# Ambiguity is judged over the set and not over each end: one twin at the base
# and a different one at the head resolve alone but not together, and a name
# that could mean either finding is refused rather than picked.
branch_pass 'split twin, base alone'  'fix-P2/performance_split-twin'
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

range_listing() {  # range_listing <dir> <base> <head> -> the range-findings lines
  ( cd "$1" && "$BASH" "$range_script" "$2" "$3" "$1/out" >/dev/null 2>&1 ) || return 1
  cat "$1/out/range-findings"
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
  "$repo_b/out/base-findings" "$repo_b/out/head-findings" "$repo_b/out/range-findings" \
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

# The base tree is listed separately: `<base>..<head>` excludes the base, so a
# finding this pull request never touched lives only there.
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
  "$repo_d/out/base-findings" "$repo_d/out/head-findings" "$repo_d/out/range-findings" \
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

echo 'listing-construction fixtures passed'

echo 'branch vocabulary fixtures passed'

echo 'PR policy fixtures passed'
