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
# listing is a fixture, not the repository's own: these cases must not change
# meaning when a finding is filed or repaired.

branch_validator="$root/.github/scripts/validate-pr-branch.sh"
fixture_dir="$(mktemp -d)"
trap 'rm -rf "$fixture_dir"' EXIT

cat > "$fixture_dir/findings.txt" <<'EOF'
P1_correctness_202609040301_pid-identity-under-a-host-wildcard-waiter.md
P2_docs-contract_202609051200_readme-claims-unperformed-migrations.md
P3_liveness_202609061330_a-drain-that-never-returns.md
P3_liveness_202609061331_twinned-description.md
P3_liveness_202609071400_twinned-description.md
EOF

cat > "$fixture_dir/legacy.txt" <<'EOF'
# a comment, and a blank line, are not branches

codex/findings-p3-1a57a2730a12
sweep/workspace-manager-fixture
EOF

branch_pass() {
  local name="$1" branch="$2"
  if ! LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
    "$BASH" "$branch_validator" "$branch" "$fixture_dir/findings.txt" >/dev/null 2>&1; then
    echo "expected branch to pass: $name ($branch)" >&2
    exit 1
  fi
}

branch_fail() {
  local name="$1" branch="$2"
  if LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
    "$BASH" "$branch_validator" "$branch" "$fixture_dir/findings.txt" >/dev/null 2>&1; then
    echo "expected branch to fail: $name ($branch)" >&2
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
branch_pass 'fix'       'fix/audit-reviewer-identity'
branch_pass 'single digit slug word' 'feature/w10-census'

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
if ! LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
  "$BASH" "$branch_validator" 'fix-P1/correctness_never-filed' >/dev/null 2>&1; then
  echo 'expected the grammar alone to pass without a findings listing' >&2
  exit 1
fi

# The legacy list is an exact-line match, so a branch that merely contains a
# listed one is still refused and the exemption cannot be widened by accident.
branch_pass 'listed legacy branch'  'codex/findings-p3-1a57a2730a12'
branch_pass 'second legacy branch'  'sweep/workspace-manager-fixture'
branch_fail 'legacy as a substring' 'codex/findings-p3-1a57a2730a12-extra'
branch_fail 'legacy comment line'   '# a comment, and a blank line, are not branches'
branch_fail 'unlisted codex branch' 'codex/findings-p3-deadbeefcafe'

echo 'branch vocabulary fixtures passed'

echo 'PR policy fixtures passed'
