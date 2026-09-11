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

# ---- a CRLF listing is the same listing --------------------------------------------------
#
# The file listing is documented input, and one written on Windows ends every
# name with a carriage return. `P2_..._shared-name.md\r` then matches no finding
# and the set NARROWS IN SILENCE, which is the way an ambiguous name becomes an
# accepted one: with both listings LF the name matches two findings and is
# refused at exit 1; convert only the SECOND to CRLF and it conformed at exit 0,
# one twin gone and nothing said about it. Both endings must give one answer,
# and a CRLF listing must still RESOLVE a name rather than being refused
# wholesale -- otherwise "handled" is indistinguishable from "rejected". A
# carriage return that is NOT a line ending is neither a line ending nor part of
# a name, so it refuses and says so.
twin_lf_a="$fixture_dir/twin-lf-a.txt"
twin_lf_b="$fixture_dir/twin-lf-b.txt"
twin_crlf_a="$fixture_dir/twin-crlf-a.txt"
twin_crlf_b="$fixture_dir/twin-crlf-b.txt"
twin_lone_cr="$fixture_dir/twin-lone-cr.txt"
printf 'P2_correctness_202609100001_shared-name.md\n' > "$twin_lf_a"
printf 'P2_correctness_202609100002_shared-name.md\n' > "$twin_lf_b"
printf 'P2_correctness_202609100001_shared-name.md\r\n' > "$twin_crlf_a"
printf 'P2_correctness_202609100002_shared-name.md\r\n' > "$twin_crlf_b"
printf 'P2_correctness_202609100001_shared-name.md\rP2_correctness_202609100002_shared-name.md\n' \
  > "$twin_lone_cr"
# AND A NUL IS NEITHER, which is the separator that loses a name in SILENCE
# rather than keeping one that cannot match. `$(cat …)` discards a NUL, so the
# record after it is concatenated onto the one before -- the twin below becomes
# `…shared-name.mdREADME.md`, matches nothing, and one match is left. The LF
# listing beside it holds the same two names and is the control: same names,
# same question, and the only difference is the byte between them.
twin_nul_b="$fixture_dir/twin-nul-b.txt"
twin_lf_b_pair="$fixture_dir/twin-lf-b-pair.txt"
printf 'P2_correctness_202609100002_shared-name.md\0README.md\0' > "$twin_nul_b"
printf 'P2_correctness_202609100002_shared-name.md\nREADME.md\n' > "$twin_lf_b_pair"

line_ending_case() {  # line_ending_case <name> <want-exit> <want-text> <listing>...
  local name="$1" want_rc="$2" want_text="$3" rc=0 out
  shift 3
  out="$(PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
    "$BASH" "$branch_validator" 'fix-P2/correctness_shared-name' "$@" 2>&1)" || rc=$?
  if [[ "$rc" != "$want_rc" ]] || ! grep -qF "$want_text" <<< "$out"; then
    echo "expected exit $want_rc and '$want_text': $name (got $rc)" >&2
    exit 1
  fi
}

line_ending_case 'LF throughout, the control'  1 'names 2 findings' "$twin_lf_a" "$twin_lf_b"
line_ending_case 'the second listing is CRLF'  1 'names 2 findings' "$twin_lf_a" "$twin_crlf_b"
line_ending_case 'both listings are CRLF'      1 'names 2 findings' "$twin_crlf_a" "$twin_crlf_b"
line_ending_case 'a CRLF listing resolves'     0 'conforms'         "$twin_crlf_a"
line_ending_case 'a carriage return that is not a line ending' \
  1 'holds a carriage return' "$twin_lf_a" "$twin_lone_cr"
line_ending_case 'two names on LF, the control for the NUL case' \
  1 'names 2 findings' "$twin_lf_a" "$twin_lf_b_pair"
line_ending_case 'NUL-delimited records are not lines' \
  1 'holds a NUL byte' "$twin_lf_a" "$twin_nul_b"

# ---- the bytes that are CHECKED are the bytes that are PARSED -------------------------------
#
# A caller's path is not a value: it can hold different bytes at every open. The
# NUL check counted the file twice and `$(cat)` read it a third time, so what was
# checked and what was parsed were three different reads of one name. A
# replacement landed -- atomically, by rename -- in the window between them, and
# the ambiguous name conformed at exit 0 over `warning: command substitution:
# ignored null byte in input` where the unreplaced listing refuses it at exit 1.
#
# THE REVIEWER USED inotify TO LAND THAT WRITE; an exported shell FUNCTION is the
# same interception with nothing to install. It runs BEFORE the real command
# opens the file, so the replacement lands INSIDE the window rather than near it,
# and it fires on this listing's own path -- named as an argument, or standing as
# the command's standard input, which is what /proc is needed for.
#
# TWO THINGS ARE ASSERTED, and the second is what stops the first passing for the
# wrong reason. The verdict may not change, wherever the replacement lands; and
# no more than ONE of those commands may see the listing at all. The repaired
# read OPENS the caller's path exactly once, with `exec 9<` in the shell itself,
# and copies it to a private file; the one hooked command below is the `cat` that
# reads that ALREADY-OPEN descriptor, which opens nothing and cannot be handed a
# different inode by a rename. Measured: one, in all three cases.
toctou_dir="$fixture_dir/toctou"
mkdir -p "$toctou_dir"
toctou_b="$toctou_dir/twin-b.txt"
printf 'P2_correctness_202609100001_shared-name.md\n' > "$toctou_dir/twin-a.txt"
printf 'P2_correctness_202609100002_shared-name.md\n' > "$toctou_dir/original"
# The same BYTE COUNT, and no NUL: nothing that counts bytes can see this land.
# Only the name is different, and that is enough to take the twin out of the set.
printf 'P2_correctness_202609100002_a-different.md\n' > "$toctou_dir/swap-clean"
# And the reviewer's own replacement, which `$(cat)` empties one byte at a time.
printf 'P2_correctness_202609100002_shared-name.md\0README.md\0' > "$toctou_dir/swap-nul"

# toctou_hooked <reads-before-the-replacement> <replacement> -- <command>...
toctou_hooked() {
  (
    export SWAP_TARGET="$toctou_b" SWAP_AFTER="$1" SWAP_FROM="$2" \
      SWAP_COUNTER="$toctou_dir/opens" PR_NUMBER='' \
      LEGACY_BRANCHES="$fixture_dir/legacy.txt"
    shift 3
    printf '0\n' > "$SWAP_COUNTER"
    _touches() {
      local argument
      for argument in "$@"; do
        if [[ "$argument" == "$SWAP_TARGET" ]]; then return 0; fi
      done
      [[ "$(readlink /proc/self/fd/0 2>/dev/null || true)" == "$SWAP_TARGET" ]]
    }
    _hook() {
      local n=0
      if _touches "$@"; then
        read -r n < "$SWAP_COUNTER" || true
        n=$((n + 1))
        printf '%s\n' "$n" > "$SWAP_COUNTER"
        if (( n > SWAP_AFTER )); then
          command cp -- "$SWAP_FROM" "$SWAP_TARGET.new"
          command mv -f -- "$SWAP_TARGET.new" "$SWAP_TARGET"
        fi
      fi
      return 0
    }
    wc()   { _hook "$@"; command wc "$@"; }
    tr()   { _hook "$@"; command tr "$@"; }
    cat()  { _hook "$@"; command cat "$@"; }
    head() { _hook "$@"; command head "$@"; }
    tail() { _hook "$@"; command tail "$@"; }
    sed()  { _hook "$@"; command sed "$@"; }
    awk()  { _hook "$@"; command awk "$@"; }
    od()   { _hook "$@"; command od "$@"; }
    cut()  { _hook "$@"; command cut "$@"; }
    dd()   { _hook "$@"; command dd "$@"; }
    export -f _touches _hook wc tr cat head tail sed awk od cut dd
    "$@" 2>&1
  )
}

if [[ ! -e /proc/self/fd/0 ]]; then
  echo 'note: skipping the replaced-listing cases (no /proc to name a read by)' >&2
else
  # THE INSTRUMENT IS TESTED FIRST. A hook that never fires would pass every
  # case below while proving nothing, so this reads the listing twice through
  # the hooked commands and asserts that both reads were seen AND that the
  # second one got the replacement.
  cp -- "$toctou_dir/original" "$toctou_b"
  probe_out="$(toctou_hooked 1 "$toctou_dir/swap-clean" -- \
    "$BASH" -c 'wc -c < "$1" >/dev/null; cat -- "$1"' _ "$toctou_b")"
  read -r probe_opens < "$toctou_dir/opens"
  if [[ "$probe_opens" != 2 ]] || [[ "$probe_out" != *a-different* ]]; then
    echo "the replacement hook does not work, so the cases below prove nothing:" >&2
    echo "  opens $probe_opens, second read [$probe_out]" >&2
    exit 1
  fi

  toctou_case() {  # toctou_case <label> <reads-before-the-replacement> <replacement>
    local label="$1" after="$2" from="$3" rc=0 out opens
    cp -- "$toctou_dir/original" "$toctou_b"
    out="$(toctou_hooked "$after" "$from" -- \
      "$BASH" "$branch_validator" 'fix-P2/correctness_shared-name' \
      "$toctou_dir/twin-a.txt" "$toctou_b")" || rc=$?
    read -r opens < "$toctou_dir/opens"
    if [[ "$rc" != 1 ]] || ! grep -qF 'names 2 findings' <<< "$out"; then
      echo "a listing replaced after it was read changed the verdict: $label (exit $rc)" >&2
      exit 1
    fi
    if (( opens > 1 )); then
      echo "the listing was opened $opens times, so there is a window to land in: $label" >&2
      exit 1
    fi
  }

  # The control first: with nothing replaced the name is ambiguous and refused,
  # which is the verdict the two cases below must not be able to move.
  toctou_case 'nothing replaced'                   99 "$toctou_dir/original"
  toctou_case 'replaced after the first read'       1 "$toctou_dir/swap-clean"
  toctou_case "the reviewer's window, before the parse" 2 "$toctou_dir/swap-nul"
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

# THE INITIAL BRANCH IS PINNED, because `git init` otherwise takes it from the
# CONTRIBUTOR'S `init.defaultBranch` and the fixtures below create branches of
# their own by name. With `init.defaultBranch=trunk` the merged-repair
# repository's `checkout -b trunk` met a branch git had already created and the
# suite died at exit 128, `fatal: a branch named 'trunk' already exists`, on a
# tree that passes everywhere else. `symbolic-ref` rather than `init -b` or
# `-c init.defaultBranch=`: HEAD is unborn here, so repointing it is the one
# form that needs no git newer than the rest of this file does. The name is not
# one any fixture creates, and the assertion is what keeps it that way.
new_repo() {  # new_repo <dir>
  mkdir -p "$1"
  git -C "$1" init -q .
  git -C "$1" symbolic-ref HEAD refs/heads/fixture-base
  if [[ "$(git -C "$1" symbolic-ref --short HEAD)" != fixture-base ]]; then
    echo "new_repo: the fixture's initial branch was not pinned: $1" >&2
    exit 1
  fi
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

# both_apis <label> <repo> <target> <head> <branch> <expected>: THE TWO
# DOCUMENTED WAYS IN, ASKED ABOUT ONE COMMIT. The workflow builds three listings
# with findings-in-range.sh; a maintainer running the validator by hand gives it
# that working tree's reviews/findings/ as a directory. One commit gets one
# answer whichever way it is asked -- that equivalence is what this pull request
# claims, and three P1s have been two APIs disagreeing -- so the two answers are
# compared WITH EACH OTHER first and against the expectation second. The
# expectation is there to stop them agreeing on the wrong answer.
#
# The directory is ONE listing where the trees are three, so this holds only for
# a repository where those hold the same set: no finding at the merge base, and
# none deleted between there and the head. A fixture that files and repairs a
# finding inside its own range is not one of those and has to be asserted the
# long way, as repo-filed-and-repaired is above.
both_apis() {
  local label="$1" repo="$2" target="$3" head="$4" branch="$5" want="$6" tree_rc dir_rc=0
  tree_rc="$(verdict "$repo" "$target" "$head" "$branch")"
  PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
    "$BASH" "$branch_validator" "$branch" "$repo/reviews/findings" >/dev/null 2>&1 || dir_rc=$?
  if [[ "$tree_rc" != "$dir_rc" ]]; then
    echo "$label ($branch): the tree listings answered $tree_rc, the directory $dir_rc" >&2
    exit 1
  fi
  if [[ "$tree_rc" != "$want" ]]; then
    echo "$label ($branch): both APIs answered $tree_rc, and $want was expected" >&2
    exit 1
  fi
}

# ONE LISTING HAS MANY SPELLINGS AND THEY MUST ANSWER ALIKE. `reviews/findings`,
# `reviews/findings/`, `reviews/findings/.`, `reviews/findings/./`,
# `reviews//findings` and `reviews/./findings` name one directory, and they did
# not answer alike: the last component decides what the path IS, and with `/.`
# appended the last component was `.`, so a COMMITTED SYMLINK at
# `reviews/findings` that the plain spelling refused at exit 1 conformed at exit 0
# with three characters added. Every case that asserts a verdict for a path
# asserts it for all six.
spellings_of() {  # spellings_of <path> -> the same path, written every way
  printf '%s\n' "$1" "$1/" "$1/." "$1/./" "${1%/*}//${1##*/}" "${1%/*}/./${1##*/}"
}

spelling_case() {  # spelling_case <label> <branch> <want-exit> <path>
  local label="$1" branch="$2" want="$3" path="$4" spelling rc
  while IFS= read -r spelling; do
    rc=0
    PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
      "$BASH" "$branch_validator" "$branch" "$spelling" >/dev/null 2>&1 || rc=$?
    if [[ "$rc" != "$want" ]]; then
      echo "$label: '$spelling' answered $rc and $want was expected" >&2
      exit 1
    fi
  done < <(spellings_of "$path")
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

# ---- what advancing master may and may not do to a verdict ---------------------------------
#
# The listings were rooted at the EVENT'S BASE SHA, which is the target branch's
# head at the moment of the event and moves for reasons that have nothing to do
# with the pull request. The first two repositories below are each run twice
# against the SAME HEAD -- once with the target at the branch point, once with
# it advanced -- and the two verdicts must be equal. Rooted at the event's base
# they were not.
#
# The shape is a DIVERGENT BOUNDARY: a finding that exists at the branch point,
# is repaired by the pull request, and is independently deleted on master. Once
# master has moved, `<base>..<head>` no longer reaches the branch point and the
# finding is in no listing at all.
#
# THAT IS NOT A CLAIM THAT THE VERDICT IS A FUNCTION OF THE HEAD. It is not, and
# the THIRD repository here pins the case where it legitimately changes: master
# merging a pull request that carries one of THIS branch's commits moves the
# merge base forward, and an ambiguous name becomes unambiguous with no push to
# the branch. That is the right answer rather than a hole -- whether a
# description names one finding or two is a property of the LEDGER, which other
# pull requests change -- and the two cases are drawn apart here so that neither
# can be read as the other.

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

# AND THE CASE THAT IS NOT A GUARANTEE, pinned so that the line between the two
# is a test and not a paragraph. master merges another pull request carrying
# commit C; C is an ancestor of THIS head, so the merge base advances to C,
# everything between the old boundary and C leaves the candidate set, and the
# ambiguous name conforms on an unchanged head. Executed by the round-4 frontier
# review against 277b3f26 and kept as documented behaviour rather than repaired:
# no choice of boundary makes the verdict a function of the head, because what
# is being resolved -- does this description name one finding, or two -- is a
# property of the ledger, and other pull requests legitimately change it.
repo_i="$fixture_dir/repo-absorbed-commit"
new_repo "$repo_i"
mkdir -p "$repo_i/reviews/findings"
echo one > "$repo_i/reviews/findings/P2_correctness_202609010000_shared-name.md"
echo two > "$repo_i/reviews/findings/P2_correctness_202609020000_shared-name.md"
git -C "$repo_i" add -A && git -C "$repo_i" commit -q -m 'branch point: two findings share a description'
i_branch_point="$(git -C "$repo_i" rev-parse HEAD)"
# C, the repair, on a commit that this pull request and an earlier one both
# carry. Nothing about it is exotic: a batch pull request opened from this
# branch point legitimately holds it.
git -C "$repo_i" checkout -q -b shared-repair
git -C "$repo_i" rm -q 'reviews/findings/P2_correctness_202609010000_shared-name.md'
git -C "$repo_i" commit -q -m 'C: the shared repair deletes the first twin'
i_shared="$(git -C "$repo_i" rev-parse HEAD)"
git -C "$repo_i" checkout -q -b pr
echo change > "$repo_i/pr-change.txt"
git -C "$repo_i" add -A && git -C "$repo_i" commit -q -m 'the pull request adds its own change'
i_head="$(git -C "$repo_i" rev-parse HEAD)"
git -C "$repo_i" checkout -q -B trunk "$i_branch_point"
echo unrelated > "$repo_i/target-change.txt"
git -C "$repo_i" add -A && git -C "$repo_i" commit -q -m 'master advances independently'
git -C "$repo_i" merge -q --no-ff "$i_shared" -m 'master merges an earlier pull request carrying C'
i_advanced="$(git -C "$repo_i" rev-parse HEAD)"

# The fixture is only about anything if the merge base really does move.
[[ "$(git -C "$repo_i" merge-base "$i_branch_point" "$i_head")" == "$i_branch_point" ]] \
  || { echo 'the fixture was meant to start with the merge base at the branch point' >&2; exit 1; }
[[ "$(git -C "$repo_i" merge-base "$i_advanced" "$i_head")" == "$i_shared" ]] \
  || { echo 'the fixture was meant to move the merge base onto the absorbed commit' >&2; exit 1; }

i_before="$(verdict "$repo_i" "$i_branch_point" "$i_head" 'fix-P2/correctness_shared-name')"
i_after="$(verdict "$repo_i" "$i_advanced" "$i_head" 'fix-P2/correctness_shared-name')"
if [[ "$i_before" != 1 ]]; then
  echo "both twins stand in the ledger at the branch point, so the name is ambiguous; got $i_before" >&2
  exit 1
fi
if [[ "$i_after" != 0 ]]; then
  echo "once master carries the repair one twin is left, so the name resolves; got $i_after" >&2
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

# ---- another merge base may only WIDEN the candidate set --------------------------------------
#
# `git merge-base` picks ONE of several best common ancestors when histories
# criss-cross, so every one of them is listed. That is conservative only if the
# answer is the UNION of what each boundary gives ALONE. Excluding the ancestors
# of every base from a single rev-list is the INTERSECTION of those ranges, and
# an intersection is narrower than its members: adding the second base DROPPED
# the commit holding one twin of an ambiguous name, and the same head that one
# base refused at exit 1 conformed at exit 0. Executed by the round-4 frontier
# review; the listings are built by findings-in-range.sh, so this is a test of
# the boundary and not of the validator.
repo_j="$fixture_dir/repo-criss-cross"
new_repo "$repo_j"
echo seed > "$repo_j/seed.txt"
git -C "$repo_j" add -A && git -C "$repo_j" commit -q -m 'common root'
j_root="$(git -C "$repo_j" rev-parse HEAD)"
# L files a twin and then repairs it, deleting the file, so that twin exists
# only in a commit BETWEEN the root and L -- which is what an exclusion list can
# swallow.
git -C "$repo_j" checkout -q -b left "$j_root"
commit_finding "$repo_j" 'P2_correctness_202609010000_shared-name.md' 'left files the first twin'
git -C "$repo_j" rm -q 'reviews/findings/P2_correctness_202609010000_shared-name.md'
git -C "$repo_j" commit -q -m 'left repairs and deletes its twin'
j_left="$(git -C "$repo_j" rev-parse HEAD)"
# R carries the second twin, and one finding nothing else names.
git -C "$repo_j" checkout -q -b right "$j_root"
commit_finding "$repo_j" 'P2_correctness_202609020000_shared-name.md' 'right files the second twin'
commit_finding "$repo_j" 'P3_liveness_202609030000_only-one-of-these.md' 'right files an unambiguous finding'
j_right="$(git -C "$repo_j" rev-parse HEAD)"
# The head and the target merge L and R in opposite orders, so neither base is
# an ancestor of the other and `git merge-base` has a choice to make.
git -C "$repo_j" checkout -q -b pr "$j_left"
git -C "$repo_j" merge -q --no-ff "$j_right" -m 'the head merges right into left'
j_head="$(git -C "$repo_j" rev-parse HEAD)"
git -C "$repo_j" checkout -q -b target "$j_right"
git -C "$repo_j" merge -q --no-ff "$j_left" -m 'the target merges left into right'
j_target="$(git -C "$repo_j" rev-parse HEAD)"
if [[ "$(git -C "$repo_j" merge-base --all "$j_target" "$j_head" | wc -l)" != 2 ]]; then
  echo 'the fixture was meant to produce two best common ancestors' >&2
  exit 1
fi
# One boundary alone sees both twins and refuses. Every boundary together must
# not see FEWER findings than one of them does.
j_one="$(verdict "$repo_j" "$j_right" "$j_head" 'fix-P2/correctness_shared-name')"
if [[ "$j_one" != 1 ]]; then
  echo "resolved from one merge base the twins are ambiguous; got $j_one" >&2
  exit 1
fi
j_both="$(verdict "$repo_j" "$j_target" "$j_head" 'fix-P2/correctness_shared-name')"
if [[ "$j_both" != 1 ]]; then
  echo "a second merge base narrowed the set and an ambiguous name conformed; got $j_both" >&2
  exit 1
fi
# And widening must not have become a blanket refusal: a name that really does
# pick out one finding still resolves across the same two boundaries.
j_unique="$(verdict "$repo_j" "$j_target" "$j_head" 'fix-P3/liveness_only-one-of-these')"
if [[ "$j_unique" != 0 ]]; then
  echo "an unambiguous name must still resolve across criss-crossed boundaries; got $j_unique" >&2
  exit 1
fi

# ---- a symlink wearing a finding's name is not a finding either -------------------------------
#
# git calls one a `120000 blob` and findings-in-range.sh's mode filter drops it.
# Bash's -e and -f FOLLOW a link, so the DIRECTORY-listing path -- the documented
# way to run this validator by hand against a working tree -- resolved
# fix-P2/correctness_<desc> at exit 0 on the very commit the workflow path
# refused at exit 1. Two answers to one documented question is the defect, so
# both paths are asserted on the same repository.
# The symlink cases need a filesystem that will make one. CI runs this gate on
# ubuntu-latest alone, but the suite is run by hand on all three platforms the
# project targets and Windows refuses a symlink without developer mode; a
# printed skip says more than a red that is about the checkout rather than
# about the gate.
symlink_probe="$fixture_dir/symlink-probe"
if ln -s ./nowhere-in-particular "$symlink_probe" 2>/dev/null && [[ -L "$symlink_probe" ]]; then
  repo_k="$fixture_dir/repo-symlink-not-a-finding"
  new_repo "$repo_k"
  echo seed > "$repo_k/seed.txt"
  git -C "$repo_k" add -A && git -C "$repo_k" commit -q -m base
  k_base="$(git -C "$repo_k" rev-parse HEAD)"
  mkdir -p "$repo_k/reviews/findings"
  ln -s ../../seed.txt "$repo_k/reviews/findings/P2_correctness_202609101200_not-a-finding.md"
  # A real finding beside it, so the case proves a filter and not an empty tree.
  echo fixture > "$repo_k/reviews/findings/P3_liveness_202609101300_a-real-finding.md"
  git -C "$repo_k" add -A && git -C "$repo_k" commit -q -m 'a symlink named like a finding'
  k_head="$(git -C "$repo_k" rev-parse HEAD)"
  if ! git -C "$repo_k" ls-tree "$k_head" reviews/findings/ | grep -q '^120000 blob '; then
    echo 'the fixture was meant to commit a SYMLINK named like a finding' >&2
    exit 1
  fi
  k_tree="$(verdict "$repo_k" "$k_base" "$k_head" 'fix-P2/correctness_not-a-finding')"
  if [[ "$k_tree" != 1 ]]; then
    echo "a committed symlink named like a finding must not resolve a fix-P*/ branch; got $k_tree" >&2
    exit 1
  fi
  # The same commit judged the documented by-hand way: that working tree's
  # reviews/findings/ handed straight in as the listing. This is the path that
  # accepted, and it must now agree with the one above.
  if PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
    "$BASH" "$branch_validator" 'fix-P2/correctness_not-a-finding' \
    "$repo_k/reviews/findings" >/dev/null 2>&1; then
    echo 'a symlink named like a finding resolved when a directory was the listing' >&2
    exit 1
  fi
  # The regular file beside it resolves both ways, so the filter is a filter and
  # not a listing read as empty.
  k_real="$(verdict "$repo_k" "$k_base" "$k_head" 'fix-P3/liveness_a-real-finding')"
  if [[ "$k_real" != 0 ]]; then
    echo "the regular file beside the symlink must still resolve from the trees; got $k_real" >&2
    exit 1
  fi
  if ! PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
    "$BASH" "$branch_validator" 'fix-P3/liveness_a-real-finding' \
    "$repo_k/reviews/findings" >/dev/null 2>&1; then
    echo 'the regular file beside the symlink must still resolve from a directory listing' >&2
    exit 1
  fi
  # A DANGLING link is a non-finding and not a read failure, which is what git
  # says about it too: a 120000 blob is one whether or not anything is at the
  # other end. Left as the only entry, the name is refused for naming no finding
  # rather than for a listing that could not be examined.
  dangling_dir="$fixture_dir/dangling-dir"
  mkdir -p "$dangling_dir"
  ln -s ./nothing-is-here "$dangling_dir/P2_correctness_202609101200_not-a-finding.md"
  dangling_rc=0
  dangling_out="$(PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
    "$BASH" "$branch_validator" 'fix-P2/correctness_not-a-finding' "$dangling_dir" 2>&1)" \
    || dangling_rc=$?
  if [[ "$dangling_rc" != 1 ]]; then
    echo "a dangling symlink named like a finding must be refused; got $dangling_rc" >&2
    exit 1
  fi
  if ! grep -q 'names no finding' <<< "$dangling_out"; then
    echo 'a dangling symlink must read as no finding, not as a listing that cannot be examined' >&2
    exit 1
  fi

  # THE SAME COMMIT AGAIN, CHECKED OUT WHERE THE FILESYSTEM CARRIES NO SYMLINK.
  # `core.symlinks=false` is git's own setting and what git uses wherever a link
  # cannot be made: the 120000 blob is materialised as a REGULAR FILE holding
  # the link target, `git status --porcelain` stays empty, and the recorded mode
  # is still 120000. A filesystem -L test has nothing left to see, so the
  # directory listing conformed at exit 0 on the very commit the tree listings
  # refuse at exit 1 -- the two answers to one question this whole section
  # exists to close, back again one repair later. What git RECORDS decides an
  # entry, and this is the case that says so.
  k_materialised='reviews/findings/P2_correctness_202609101200_not-a-finding.md'
  git -C "$repo_k" config core.symlinks false
  rm "$repo_k/$k_materialised"
  git -C "$repo_k" checkout -- "$k_materialised"
  if [[ -L "$repo_k/$k_materialised" ]] || [[ ! -f "$repo_k/$k_materialised" ]] \
    || [[ "$(git -C "$repo_k" ls-files -s -- "$k_materialised" | cut -d' ' -f1)" != 120000 ]]; then
    echo 'note: skipping the core.symlinks=false case (this git left the link a link)' >&2
  else
    if PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
      "$BASH" "$branch_validator" 'fix-P2/correctness_not-a-finding' \
      "$repo_k/reviews/findings" >/dev/null 2>&1; then
      echo 'a committed symlink checked out as a regular file resolved through the directory listing' >&2
      exit 1
    fi
    # And it is still a filter and not a listing read as empty: the regular file
    # beside it resolves, through a checkout that materialised neither as a link.
    if ! PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
      "$BASH" "$branch_validator" 'fix-P3/liveness_a-real-finding' \
      "$repo_k/reviews/findings" >/dev/null 2>&1; then
      echo 'the regular file beside the materialised symlink must still resolve' >&2
      exit 1
    fi

    # AND "GIT COULD NOT ANSWER" IS NOT "THERE IS NO REPOSITORY HERE". The
    # recorded mode is only read where git says the directory is inside a work
    # tree, and suppressing the status of THAT question turned a failure into
    # the filesystem fallback it was added to replace: with `.git/config`
    # unreadable, discovery exits 128 and this very commit -- the one the two
    # assertions above have just refused -- conformed at exit 0 with no
    # diagnostic at all. An unreadable repository is not permission to disregard
    # its recorded modes. Root can read anything, so this only means something
    # as an ordinary user.
    if [[ "$(id -u)" -ne 0 ]] && chmod 000 "$repo_k/.git/config" 2>/dev/null \
      && ! git -C "$repo_k/reviews/findings" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
      unreadable_rc=0
      unreadable_out="$(PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
        "$BASH" "$branch_validator" 'fix-P2/correctness_not-a-finding' \
        "$repo_k/reviews/findings" 2>&1)" || unreadable_rc=$?
      # The other API cannot be built at all where git cannot read the
      # repository, which is `verdict`'s 99. Neither may report conformance.
      unreadable_tree="$(verdict "$repo_k" "$k_base" "$k_head" 'fix-P2/correctness_not-a-finding')"
      chmod 600 "$repo_k/.git/config"
      if [[ "$unreadable_rc" == 0 || "$unreadable_tree" == 0 ]]; then
        echo "a repository git could not read conformed: directory $unreadable_rc, trees $unreadable_tree" >&2
        exit 1
      fi
      if ! grep -q 'git could not say what it records' <<< "$unreadable_out"; then
        echo 'a repository git could not read must refuse SAYING SO, not silently' >&2
        exit 1
      fi
      # A repository git CAN read is still not a refusal, so the case above is
      # about the unreadable config and not about the directory being in a
      # repository at all.
      if ! PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
        "$BASH" "$branch_validator" 'fix-P3/liveness_a-real-finding' \
        "$repo_k/reviews/findings" >/dev/null 2>&1; then
        echo 'restoring the config must restore the verdict' >&2
        exit 1
      fi
    else
      chmod 600 "$repo_k/.git/config" 2>/dev/null || true
      echo 'note: skipping the unreadable-repository case (running as root, or chmod had no effect)' >&2
    fi
  fi
else
  echo 'note: skipping the symlink cases (this filesystem will not create one)' >&2
fi

# ---- and the findings DIRECTORY is a tracked entry too ----------------------------------------
#
# The ENTRIES were decided by the mode git records; the listing PATH itself was
# still followed before its own recorded type was looked at. Commit
# `reviews/findings` as a SYMLINK to a sibling directory and git calls it a
# `120000 blob`, `git status` stays empty, and the tree listings hold no finding
# under reviews/findings/ and refuse at exit 1 -- while handing that path
# straight in FOLLOWED the link and resolved the name out of files no ledger
# holds, at exit 0. It is the rule one level up from the entries: a symlink is
# not a finding, and a symlink is not the findings directory either. Both APIs,
# one commit, which is the property this pull request claims.
if [[ -L "$symlink_probe" ]]; then
  repo_p="$fixture_dir/repo-symlinked-findings-dir"
  new_repo "$repo_p"
  echo seed > "$repo_p/seed.txt"
  git -C "$repo_p" add -A && git -C "$repo_p" commit -q -m base
  p_base="$(git -C "$repo_p" rev-parse HEAD)"
  mkdir -p "$repo_p/elsewhere" "$repo_p/reviews"
  echo fixture > "$repo_p/elsewhere/P2_correctness_202609100001_no-ledger-entry.md"
  ln -s ../elsewhere "$repo_p/reviews/findings"
  git -C "$repo_p" add -A && git -C "$repo_p" commit -q -m 'a findings directory that is a symlink'
  p_head="$(git -C "$repo_p" rev-parse HEAD)"
  if ! git -C "$repo_p" ls-tree "$p_head" reviews/ | grep -q '^120000 blob .*reviews/findings$' \
    || [[ -n "$(git -C "$repo_p" status --porcelain)" ]]; then
    echo 'the fixture was meant to COMMIT reviews/findings as a symlink, cleanly' >&2
    exit 1
  fi
  both_apis 'a symlinked findings directory holds no finding' \
    "$repo_p" "$p_base" "$p_head" 'fix-P2/correctness_no-ledger-entry' 1
  # And with `/.` appended it conformed at exit 0 on this very commit, because
  # the question moved from `findings` to `.` -- the same listing, refused one way
  # and accepted the other.
  spelling_case 'a symlinked findings directory, every spelling' \
    'fix-P2/correctness_no-ledger-entry' 1 "$repo_p/reviews/findings"

  # The same name in a REAL findings directory resolves, so the case above is a
  # filter on the listing path and not a listing read as empty.
  repo_q="$fixture_dir/repo-real-findings-dir"
  new_repo "$repo_q"
  echo seed > "$repo_q/seed.txt"
  git -C "$repo_q" add -A && git -C "$repo_q" commit -q -m base
  q_base="$(git -C "$repo_q" rev-parse HEAD)"
  commit_finding "$repo_q" 'P2_correctness_202609100001_no-ledger-entry.md' 'a real finding'
  q_head="$(git -C "$repo_q" rev-parse HEAD)"
  both_apis 'and a real findings directory still resolves it' \
    "$repo_q" "$q_base" "$q_head" 'fix-P2/correctness_no-ledger-entry' 0

  # THE SAME COMMIT CHECKED OUT WHERE THE FILESYSTEM CARRIES NO SYMLINK. Under
  # core.symlinks=false the 120000 blob is materialised as a REGULAR FILE
  # holding `../elsewhere`, which is not a directory at all: read as a file
  # listing it names one absent finding, and the recorded mode is what catches
  # it, because it is the same 120000 either way.
  git -C "$repo_p" config core.symlinks false
  rm "$repo_p/reviews/findings"
  git -C "$repo_p" checkout -- reviews/findings
  if [[ -L "$repo_p/reviews/findings" ]] || [[ ! -f "$repo_p/reviews/findings" ]]; then
    echo 'note: skipping the materialised symlinked-directory case (this git left the link a link)' >&2
  else
    both_apis 'a symlinked findings directory materialised as a file holds none either' \
      "$repo_p" "$p_base" "$p_head" 'fix-P2/correctness_no-ledger-entry' 1
  fi

  # AND THE OTHER WAY ROUND IS A REFUSAL AND NOT AN EMPTY SET. Where git records
  # a DIRECTORY and the checkout holds a link in its place, the findings are the
  # entries the index records under that path and the link is not the directory:
  # reading it would answer out of somebody else's files, and reading it as
  # empty would narrow the set in silence. This is the one case here where the
  # two APIs deliberately differ -- the tree listings resolve the name at exit 0
  # from the commit, and the directory listing refuses at exit 1 saying it
  # cannot be read -- because a refusal is neither a false green nor a silent
  # narrowing, and the checkout it needs is one `git checkout` away.
  mkdir -p "$repo_q/elsewhere"
  echo fixture > "$repo_q/elsewhere/P3_liveness_202609100009_not-in-the-ledger.md"
  rm -rf "$repo_q/reviews/findings"
  ln -s ../elsewhere "$repo_q/reviews/findings"
  replaced_case() {  # replaced_case <branch>
    local rc=0 out
    out="$(PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
      "$BASH" "$branch_validator" "$1" "$repo_q/reviews/findings" 2>&1)" || rc=$?
    if [[ "$rc" != 1 ]] || ! grep -q 'checkout does not hold one there' <<< "$out"; then
      echo "a tracked findings directory replaced by a link must refuse, saying so: $1 (got $rc)" >&2
      exit 1
    fi
  }
  # Neither the ledger's own name -- which is no longer readable from that path
  # -- nor the name at the end of the link, which no ledger holds.
  replaced_case 'fix-P2/correctness_no-ledger-entry'
  replaced_case 'fix-P3/liveness_not-in-the-ledger' 
else
  echo 'note: skipping the symlinked-findings-directory cases (this filesystem will not create one)' >&2
fi

# ---- what git RECORDS, not what the checkout happens to hold -----------------------------------
#
# A TRACKED finding need not be in the working tree, and the candidate names
# came from a filesystem glob alone. Here one twin is removed from the checkout
# and left in the index: the tree listings read the commit and refuse, and the
# directory API saw a single match and CONFORMED at exit 0. Sparse checkouts are
# the case that arrives without anyone doing anything unusual, and they are
# below; this one needs nothing but `rm` and holds everywhere.
recorded_repo="$fixture_dir/repo-recorded-not-materialised"
new_repo "$recorded_repo"
echo seed > "$recorded_repo/seed.txt"
git -C "$recorded_repo" add -A && git -C "$recorded_repo" commit -q -m base
mkdir -p "$recorded_repo/reviews/findings"
echo one > "$recorded_repo/reviews/findings/P2_correctness_202609100001_shared-name.md"
echo two > "$recorded_repo/reviews/findings/P2_correctness_202609100002_shared-name.md"
git -C "$recorded_repo" add -A \
  && git -C "$recorded_repo" commit -q -m 'two findings share a description'
rm "$recorded_repo/reviews/findings/P2_correctness_202609100002_shared-name.md"
recorded_rc=0
recorded_out="$(PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
  "$BASH" "$branch_validator" 'fix-P2/correctness_shared-name' \
  "$recorded_repo/reviews/findings" 2>&1)" || recorded_rc=$?
if [[ "$recorded_rc" != 1 ]] || ! grep -q 'names 2 findings' <<< "$recorded_out"; then
  echo "a finding the index records and the checkout lacks must still be a candidate; got $recorded_rc" >&2
  exit 1
fi

# ---- git's WORDS are not the decision ---------------------------------------------------------
#
# The same repository at a path holding the string git uses for "there is no
# repository here", read through a LINKED WORKTREE -- which is what puts the
# common `.git/config` on an absolute path, and so puts that path into git's
# diagnostic. With the config unreadable git exits 128 saying `unable to access
# '.../not a git repository - fixture/.git/config': Permission denied`, and a
# substring test for git's own sentence matched inside the PATHNAME, which the
# caller chooses. The validator fell back to the filesystem and answered
# `conforms` at exit 0 with empty stderr, on a checkout it refuses when the
# repository is readable. The exit status decides now, and "is there a
# repository at all" is a second question put to `git rev-parse
# --resolve-git-dir`, which reads no config and so still finds the repository
# whose config git could not read. Root can read anything, so this only means
# something as an ordinary user.
phrase_repo="$fixture_dir/not a git repository - fixture"
phrase_wt="$fixture_dir/linked-worktree"
new_repo "$phrase_repo"
echo seed > "$phrase_repo/seed.txt"
git -C "$phrase_repo" add -A && git -C "$phrase_repo" commit -q -m base
mkdir -p "$phrase_repo/reviews/findings"
echo one > "$phrase_repo/reviews/findings/P2_correctness_202609100001_shared-name.md"
echo two > "$phrase_repo/reviews/findings/P2_correctness_202609100002_shared-name.md"
git -C "$phrase_repo" add -A \
  && git -C "$phrase_repo" commit -q -m 'two findings share a description'
if ! git -C "$phrase_repo" worktree add -q --detach "$phrase_wt" HEAD 2>/dev/null; then
  echo 'note: skipping the path-shaped-like-a-message case (this git will not add a worktree)' >&2
elif [[ "$(id -u)" -eq 0 ]] || ! chmod 000 "$phrase_repo/.git/config" 2>/dev/null \
  || git -C "$phrase_wt/reviews/findings" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  chmod 600 "$phrase_repo/.git/config" 2>/dev/null || true
  echo 'note: skipping the path-shaped-like-a-message case (running as root, or chmod had no effect)' >&2
else
  # The linked worktree keeps its own index, and one twin is removed from its
  # CHECKOUT alone: the filesystem fallback -- which cannot see an index at all
  # -- answers `conforms` here rather than merely answering for another reason.
  rm "$phrase_wt/reviews/findings/P2_correctness_202609100002_shared-name.md"
  # git's message must really carry the path, or the case is about nothing.
  # Captured and then matched: git exits 128 here, and under `pipefail` a
  # pipeline out of it fails whatever grep found.
  phrase_probe="$(git -C "$phrase_wt/reviews/findings" rev-parse --is-inside-work-tree 2>&1 || true)"
  if ! grep -qF 'not a git repository - fixture' <<< "$phrase_probe"; then
    echo 'the fixture was meant to put the repository PATH into git-s diagnostic' >&2
    exit 1
  fi
  phrase_rc=0
  phrase_out="$(PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
    "$BASH" "$branch_validator" 'fix-P2/correctness_shared-name' \
    "$phrase_wt/reviews/findings" 2>&1)" || phrase_rc=$?
  chmod 600 "$phrase_repo/.git/config"
  if [[ "$phrase_rc" == 0 ]]; then
    echo 'a repository whose PATH holds git-s no-repository sentence conformed' >&2
    exit 1
  fi
  if ! grep -q 'git could not say what it records' <<< "$phrase_out"; then
    echo 'the refusal must be the unreadable repository, not a name that resolved elsewhere' >&2
    exit 1
  fi
  # And a readable repository at the same path is not refused for its name: the
  # index records both twins, so the name is ambiguous and says which two.
  phrase_ok_rc=0
  phrase_ok_out="$(PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
    "$BASH" "$branch_validator" 'fix-P2/correctness_shared-name' \
    "$phrase_wt/reviews/findings" 2>&1)" || phrase_ok_rc=$?
  if [[ "$phrase_ok_rc" != 1 ]] || ! grep -q 'names 2 findings' <<< "$phrase_ok_out"; then
    echo "restoring the config must restore the verdict; got $phrase_ok_rc" >&2
    exit 1
  fi
fi

# ---- metadata that cannot be EXAMINED is not metadata that is not there ------------------------
#
# Where git's discovery fails, "is there a repository at all" is asked as a
# second question -- and EVERY unsuccessful answer to that one read as "no",
# which is the discarded read failure of the case above, one level down. A
# LINKED WORKTREE keeps its `.git` in a FILE: make that file unreadable and both
# git probes exit 128 `Permission denied`, the filesystem fallback decided the
# entry, and the twin the index records and the checkout lacks was gone -- the
# ambiguous name CONFORMED at exit 0 with empty stderr on the checkout that is
# refused at exit 1 when the file is readable. Missing metadata may fall back;
# metadata that cannot be examined must refuse and say so. Root can read
# anything, so the permission cases only mean something as an ordinary user.
unexaminable_repo="$fixture_dir/repo-unexaminable-git"
unexaminable_wt="$fixture_dir/unexaminable-worktree"
new_repo "$unexaminable_repo"
echo seed > "$unexaminable_repo/seed.txt"
git -C "$unexaminable_repo" add -A && git -C "$unexaminable_repo" commit -q -m base
mkdir -p "$unexaminable_repo/reviews/findings"
echo one > "$unexaminable_repo/reviews/findings/P2_correctness_202609100001_shared-name.md"
echo two > "$unexaminable_repo/reviews/findings/P2_correctness_202609100002_shared-name.md"
git -C "$unexaminable_repo" add -A \
  && git -C "$unexaminable_repo" commit -q -m 'two findings share a description'
unexaminable_twin='reviews/findings/P2_correctness_202609100002_shared-name.md'

# unexaminable_verdict <checkout> -> the exit code, with stderr on stdout
unexaminable_verdict() {
  PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
    "$BASH" "$branch_validator" 'fix-P2/correctness_shared-name' \
    "$1/reviews/findings" 2>&1
}

if ! git -C "$unexaminable_repo" worktree add -q --detach "$unexaminable_wt" HEAD 2>/dev/null \
  || [[ ! -f "$unexaminable_wt/.git" ]]; then
  echo 'note: skipping the unexaminable-.git cases (this git made no linked worktree)' >&2
elif [[ "$(id -u)" -eq 0 ]] || ! chmod 000 "$unexaminable_wt/.git" 2>/dev/null \
  || git -C "$unexaminable_wt/reviews/findings" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  chmod 644 "$unexaminable_wt/.git" 2>/dev/null || true
  echo 'note: skipping the unexaminable-.git cases (running as root, or chmod had no effect)' >&2
else
  chmod 644 "$unexaminable_wt/.git"
  # The twin leaves the linked worktree's CHECKOUT and stays in its index, so
  # the filesystem fallback -- which cannot see an index at all -- answers
  # `conforms` here rather than merely answering for some other reason.
  rm "$unexaminable_wt/$unexaminable_twin"
  control_rc=0
  control_out="$(unexaminable_verdict "$unexaminable_wt")" || control_rc=$?
  if [[ "$control_rc" != 1 ]] || ! grep -q 'names 2 findings' <<< "$control_out"; then
    echo "the control was meant to refuse an ambiguous name; got $control_rc" >&2
    exit 1
  fi
  chmod 000 "$unexaminable_wt/.git"
  unreadable_file_rc=0
  unreadable_file_out="$(unexaminable_verdict "$unexaminable_wt")" || unreadable_file_rc=$?
  chmod 644 "$unexaminable_wt/.git"
  if [[ "$unreadable_file_rc" == 0 ]]; then
    echo 'a .git FILE that cannot be read conformed, which is the filesystem fallback again' >&2
    exit 1
  fi
  if ! grep -q 'cannot be examined' <<< "$unreadable_file_out"; then
    echo 'an unexaminable .git must refuse SAYING SO, not silently' >&2
    exit 1
  fi
  # And restoring the mode restores the verdict, so the refusal is about the
  # unreadable file and not about the worktree.
  restored_rc=0
  restored_out="$(unexaminable_verdict "$unexaminable_wt")" || restored_rc=$?
  if [[ "$restored_rc" != 1 ]] || ! grep -q 'names 2 findings' <<< "$restored_out"; then
    echo "restoring the .git file must restore the verdict; got $restored_rc" >&2
    exit 1
  fi
  # THE SAME HOLE ONE SHAPE OVER. A `.git` DIRECTORY that cannot be looked into
  # is reported by git itself as `not a gitdir` -- the same words as an empty
  # directory named `.git` and as no `.git` at all -- so the status cannot
  # separate them and this asks the filesystem whether there was anything to
  # read. The main checkout is the one with a `.git` directory.
  rm "$unexaminable_repo/$unexaminable_twin"
  main_rc=0
  main_out="$(unexaminable_verdict "$unexaminable_repo")" || main_rc=$?
  if [[ "$main_rc" != 1 ]] || ! grep -q 'names 2 findings' <<< "$main_out"; then
    echo "the main checkout was meant to refuse an ambiguous name; got $main_rc" >&2
    exit 1
  fi
  if chmod 000 "$unexaminable_repo/.git" 2>/dev/null \
    && ! git -C "$unexaminable_repo/reviews/findings" rev-parse --is-inside-work-tree >/dev/null 2>&1
  then
    dir_rc=0
    dir_out="$(unexaminable_verdict "$unexaminable_repo")" || dir_rc=$?
    chmod 755 "$unexaminable_repo/.git"
    if [[ "$dir_rc" == 0 ]] || ! grep -q 'cannot be examined' <<< "$dir_out"; then
      echo "a .git DIRECTORY that cannot be looked into conformed or said nothing; got $dir_rc" >&2
      exit 1
    fi
  else
    chmod 755 "$unexaminable_repo/.git" 2>/dev/null || true
    echo 'note: skipping the unexaminable-.git-directory case (chmod had no effect)' >&2
  fi
fi

# AND A DIRECTORY NAMED `.git` THAT IS NOT A REPOSITORY IS STILL NOT ONE. This
# is why the question is put to git rather than to `[[ -e ]]`, and why the
# examinability test above is an attempt to look inside rather than a test that
# something is there: a stray /tmp/.git -- an empty directory, readable, owned
# by whoever got there first -- would otherwise turn every by-hand listing under
# /tmp red, which is a false refusal of a legitimate branch.
stray_parent="$fixture_dir/stray-git"
mkdir -p "$stray_parent/.git" "$stray_parent/listing"
echo real > "$stray_parent/listing/P3_liveness_202609100003_a-real-finding.md"
if ! PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
  "$BASH" "$branch_validator" 'fix-P3/liveness_a-real-finding' \
  "$stray_parent/listing" >/dev/null 2>&1; then
  echo 'an empty directory named .git above a by-hand listing must not refuse it' >&2
  exit 1
fi

# ---- a sparse checkout is a smaller checkout and not a smaller ledger --------------------------
#
# The case that arrives on its own. `git sparse-checkout` leaves the index
# recording every excluded path and `git status` EMPTY, so nothing about the
# working tree says a finding is missing: with two findings sharing a
# description and the second excluded, the directory API saw one match and
# conformed at exit 0 while the tree listings refused the same commit at exit 1.
# Both directions are asserted -- the twin that must still make the name
# ambiguous, and an excluded finding that must still RESOLVE its own name --
# because a candidate set that grew is only right if it grew for both.
repo_n="$fixture_dir/repo-sparse-checkout"
new_repo "$repo_n"
echo seed > "$repo_n/seed.txt"
git -C "$repo_n" add -A && git -C "$repo_n" commit -q -m base
n_base="$(git -C "$repo_n" rev-parse HEAD)"
mkdir -p "$repo_n/reviews/findings"
echo one > "$repo_n/reviews/findings/P2_correctness_202609100001_shared-name.md"
echo two > "$repo_n/reviews/findings/P2_correctness_202609100002_shared-name.md"
echo three > "$repo_n/reviews/findings/P3_liveness_202609100003_left-out-of-the-checkout.md"
echo four > "$repo_n/reviews/findings/P3_liveness_202609100004_a-real-finding.md"
git -C "$repo_n" add -A && git -C "$repo_n" commit -q -m 'four findings, two sharing a description'
n_head="$(git -C "$repo_n" rev-parse HEAD)"
both_apis 'the control: a full checkout' \
  "$repo_n" "$n_base" "$n_head" 'fix-P2/correctness_shared-name' 1
n_twin='reviews/findings/P2_correctness_202609100002_shared-name.md'
n_unique='reviews/findings/P3_liveness_202609100003_left-out-of-the-checkout.md'
if git -C "$repo_n" sparse-checkout set --no-cone '/*' "!/$n_twin" "!/$n_unique" >/dev/null 2>&1 \
  && [[ ! -e "$repo_n/$n_twin" && ! -e "$repo_n/$n_unique" ]] \
  && [[ -z "$(git -C "$repo_n" status --porcelain)" ]]; then
  # The exclusion is the CHECKOUT's and not the index's, which is what makes
  # this a narrowed listing rather than a ledger that lost two findings.
  if [[ "$(git -C "$repo_n" ls-files -- reviews/findings/ | wc -l)" != 4 ]]; then
    echo 'the fixture was meant to leave all four findings in the index' >&2
    exit 1
  fi
  both_apis 'a sparse checkout hides neither twin' \
    "$repo_n" "$n_base" "$n_head" 'fix-P2/correctness_shared-name' 1
  both_apis 'and an excluded finding still resolves its own name' \
    "$repo_n" "$n_base" "$n_head" 'fix-P3/liveness_left-out-of-the-checkout' 0
  both_apis 'and so does one the checkout did materialise' \
    "$repo_n" "$n_base" "$n_head" 'fix-P3/liveness_a-real-finding' 0
else
  echo 'note: skipping the sparse-checkout cases (this git will not make one)' >&2
fi

# ---- a filename is not a line ----------------------------------------------------------------
#
# A NEWLINE IS LEGAL IN A FILENAME AND NUL IS NOT, which is the whole reason git
# is asked for `-z` records. Converting those NULs into newlines to read them
# threw away the one boundary that cannot be forged:
# `noise<LF>P2_<category>_<ts>_<desc>.md` arrived as two records, the second
# carrying neither a mode nor a tab, and the REAL finding wearing that second
# name was filed among the non-regular entries and dropped. One twin was left
# and the ambiguous name conformed at exit 0 -- while the tree listings, where
# `git ls-tree` C-QUOTES such a name into something that matches no finding at
# all, refused the same commit at exit 1.
#
# READING THE RECORDS WHOLE IS NOT ENOUGH ON ITS OWN, which is why both shapes
# are here. TWO HALF REPAIRS WERE MEASURED. One read git's records whole and
# carried the names through the newline-delimited recorded-mode sets: the
# symlink case below conformed at exit 0, because a name holding a newline
# corrupts a membership test wherever it is put, and the real finding of that
# name tested as a member of the non-regular set and was dropped exactly as
# before. The other dropped nothing at all and printed such a name into the
# candidate set: the `named-by-no-file` case conformed at exit 0, a finding
# nothing has filed invented out of the tail of a name, where the trees refuse
# at exit 1. So the names are read whole AND the ones that cannot be carried are
# dropped where they are read.
#
# Every case here is taken through BOTH APIs, because agreeing is the property.
newline_probe="$fixture_dir/$(printf 'newline\nprobe')"
if : > "$newline_probe" 2>/dev/null && [[ -f "$newline_probe" ]]; then
  nl_twin=$'noise\nP2_correctness_202609100002_shared-name.md'
  nl_ghost=$'zzz\nP2_correctness_202609100003_named-by-no-file.md'
  repo_l="$fixture_dir/repo-newline-in-a-name"
  new_repo "$repo_l"
  echo seed > "$repo_l/seed.txt"
  git -C "$repo_l" add -A && git -C "$repo_l" commit -q -m base
  l_base="$(git -C "$repo_l" rev-parse HEAD)"
  mkdir -p "$repo_l/reviews/findings"
  echo one > "$repo_l/reviews/findings/P2_correctness_202609100001_shared-name.md"
  echo two > "$repo_l/reviews/findings/P2_correctness_202609100002_shared-name.md"
  echo noise > "$repo_l/reviews/findings/noise"
  # The tail of this name is the second twin's name exactly, so a split hands
  # the set a name that IS a finding and takes the finding itself away.
  echo split > "$repo_l/reviews/findings/$nl_twin"
  # And the tail of this one is a finding NOTHING has filed, so a split invents
  # a finding, or refuses for a name that is not in the directory at all.
  echo invented > "$repo_l/reviews/findings/$nl_ghost"
  echo real > "$repo_l/reviews/findings/P3_liveness_202609101300_a-real-finding.md"
  git -C "$repo_l" add -A && git -C "$repo_l" commit -q -m 'a name with a newline in it'
  l_head="$(git -C "$repo_l" rev-parse HEAD)"
  if [[ ! -f "$repo_l/reviews/findings/$nl_twin" ]] \
    || [[ -n "$(git -C "$repo_l" status --porcelain)" ]]; then
    echo 'the fixture was meant to COMMIT a filename holding a newline' >&2
    exit 1
  fi
  both_apis 'a name holding a newline hides neither twin' \
    "$repo_l" "$l_base" "$l_head" 'fix-P2/correctness_shared-name' 1
  both_apis 'and a fragment of one resolves nothing' \
    "$repo_l" "$l_base" "$l_head" 'fix-P2/correctness_named-by-no-file' 1
  # Dropping a name that cannot be a finding must not become a way to refuse a
  # listing that holds one, or the two APIs part company the other way round.
  both_apis 'and the finding beside them still resolves' \
    "$repo_l" "$l_base" "$l_head" 'fix-P3/liveness_a-real-finding' 0

  # The same name RECORDED AS A SYMLINK, which is the half of this that survives
  # reading the records whole.
  if [[ -L "$symlink_probe" ]]; then
    repo_m="$fixture_dir/repo-newline-in-a-symlink-name"
    new_repo "$repo_m"
    echo seed > "$repo_m/seed.txt"
    git -C "$repo_m" add -A && git -C "$repo_m" commit -q -m base
    m_base="$(git -C "$repo_m" rev-parse HEAD)"
    mkdir -p "$repo_m/reviews/findings"
    echo one > "$repo_m/reviews/findings/P2_correctness_202609100001_shared-name.md"
    echo two > "$repo_m/reviews/findings/P2_correctness_202609100002_shared-name.md"
    ln -s ../../seed.txt "$repo_m/reviews/findings/$nl_twin"
    git -C "$repo_m" add -A && git -C "$repo_m" commit -q -m 'a symlink whose name holds a newline'
    m_head="$(git -C "$repo_m" rev-parse HEAD)"
    if ! git -C "$repo_m" ls-tree "$m_head" reviews/findings/ | grep -q '^120000 blob '; then
      echo 'the fixture was meant to commit a SYMLINK whose name holds a newline' >&2
      exit 1
    fi
    both_apis 'a newline in a non-regular entry name hides no finding' \
      "$repo_m" "$m_base" "$m_head" 'fix-P2/correctness_shared-name' 1
  fi
else
  echo 'note: skipping the newline-in-a-name cases (this filesystem will not create one)' >&2
fi

# ---- a read that FAILED is not the end of the file -------------------------------------------
#
# The whole class in one line. `read -d ''` reports end-of-input and an I/O error
# with the same status 1, and the group that was meant to catch the second --
# `{ IFS= read -r -d '' out; nul=$?; } < "$listing" || { ... }` -- ALWAYS
# SUCCEEDS, because a command group's status is the last command's and the last
# command was an assignment. So the `||` never ran and every failed read became
# an empty listing. `/proc/self/mem` is the case that exists everywhere: it is
# there, `-r` says it is readable, and every read of it fails with
# `Input/output error`. Measured at fb219790: exit 0 `conforms`, the listing
# silently empty, over a bash warning nothing acted on.
#
# The control beside it is what makes this about the READ and not about the path:
# the same two names in two ordinary files are an ambiguous refusal.
unreadable_stream=/proc/self/mem
stream_probe_rc=0
( { exec 9< "$unreadable_stream"; } 2>/dev/null && cat <&9 >/dev/null 2>&1 ) || stream_probe_rc=$?
if [[ ! -e "$unreadable_stream" ]] || [[ "$stream_probe_rc" == 0 ]]; then
  echo 'note: skipping the failed-read case (this platform has no stream that fails to read)' >&2
else
  stream_rc=0
  stream_out="$(PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
    "$BASH" "$branch_validator" 'fix-P2/correctness_shared-name' \
    "$twin_lf_a" "$unreadable_stream" 2>&1)" || stream_rc=$?
  if [[ "$stream_rc" == 0 ]]; then
    echo 'a listing that could not be read conformed, which is the empty-set fallback again' >&2
    exit 1
  fi
  if ! grep -q 'could not be read to the' <<< "$stream_out"; then
    echo 'a listing that could not be read must refuse SAYING SO, not as a missing finding' >&2
    printf '%s\n' "$stream_out" >&2
    exit 1
  fi
  # And it must not be refused for existing, or for being unopenable, or for
  # anything else the check before the read can see: those tests all pass here.
  if [[ ! -r "$unreadable_stream" ]]; then
    echo 'the fixture was meant to use a listing that -r calls readable' >&2
    exit 1
  fi
  line_ending_case 'two ordinary listings, the control for the failed read' \
    1 'names 2 findings' "$twin_lf_a" "$twin_lf_b"
fi

# ---- metadata git cannot read INSIDE a `.git` it can enter perfectly well ---------------------
#
# The third crop of the same defect. The examinability test was "can `.git/.` be
# reached", which is a test of SEARCH on the directory and says nothing about the
# files inside it. `chmod 000 .git/HEAD` -- or `.git/objects`, or `.git/refs` --
# leaves `.git` searchable and makes BOTH git probes exit 128 with the same words
# git uses for a directory that is not a repository at all, so the walk concluded
# "no repository", the filesystem fallback decided the entries, and the twin the
# index records and the checkout lacks was gone: exit 0 `conforms`, empty stderr,
# on the checkout that is exit 1 `names 2 findings` when the file is readable.
# Root can read anything, so the permission cases only mean something as an
# ordinary user.
inside_repo="$fixture_dir/repo-unreadable-inside-git"
new_repo "$inside_repo"
echo seed > "$inside_repo/seed.txt"
git -C "$inside_repo" add -A && git -C "$inside_repo" commit -q -m base
mkdir -p "$inside_repo/reviews/findings"
echo one > "$inside_repo/reviews/findings/P2_correctness_202609100001_shared-name.md"
echo two > "$inside_repo/reviews/findings/P2_correctness_202609100002_shared-name.md"
git -C "$inside_repo" add -A \
  && git -C "$inside_repo" commit -q -m 'two findings share a description'
# The twin leaves the CHECKOUT and stays in the index, so the filesystem
# fallback answers `conforms` here rather than merely answering for some other
# reason.
rm "$inside_repo/reviews/findings/P2_correctness_202609100002_shared-name.md"
inside_verdict() {  # inside_verdict -> the exit code, with stderr on stdout
  PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
    "$BASH" "$branch_validator" 'fix-P2/correctness_shared-name' \
    "$inside_repo/reviews/findings" 2>&1
}
inside_control_rc=0
inside_control_out="$(inside_verdict)" || inside_control_rc=$?
if [[ "$inside_control_rc" != 1 ]] || ! grep -q 'names 2 findings' <<< "$inside_control_out"; then
  echo "the control was meant to refuse an ambiguous name; got $inside_control_rc" >&2
  exit 1
fi
if [[ "$(id -u)" -eq 0 ]]; then
  echo 'note: skipping the unreadable-metadata cases (running as root)' >&2
else
  for victim in HEAD objects refs; do
    if [[ ! -e "$inside_repo/.git/$victim" ]]; then
      echo "note: skipping .git/$victim (this git did not create it)" >&2
      continue
    fi
    victim_mode=755
    [[ -d "$inside_repo/.git/$victim" ]] || victim_mode=644
    if ! chmod 000 "$inside_repo/.git/$victim" 2>/dev/null \
      || git -C "$inside_repo/reviews/findings" rev-parse --is-inside-work-tree >/dev/null 2>&1
    then
      chmod "$victim_mode" "$inside_repo/.git/$victim" 2>/dev/null || true
      echo "note: skipping .git/$victim (chmod had no effect on git)" >&2
      continue
    fi
    # `.git` itself is still a directory this can enter and look into, which is
    # what the old test checked and what makes this case the hole it was.
    if [[ ! -e "$inside_repo/.git/." ]]; then
      chmod "$victim_mode" "$inside_repo/.git/$victim"
      echo "note: skipping .git/$victim (it made .git itself unsearchable)" >&2
      continue
    fi
    victim_rc=0
    victim_out="$(inside_verdict)" || victim_rc=$?
    chmod "$victim_mode" "$inside_repo/.git/$victim"
    if [[ "$victim_rc" == 0 ]]; then
      echo "an unreadable .git/$victim conformed, which is the filesystem fallback again" >&2
      exit 1
    fi
    if ! grep -qE 'cannot be examined|git could not say' <<< "$victim_out"; then
      echo "an unreadable .git/$victim must refuse SAYING SO, not silently" >&2
      printf '%s\n' "$victim_out" >&2
      exit 1
    fi
    # Restoring the mode restores the verdict, so the refusal is about the
    # unreadable file and not about the repository.
    restored_victim_rc=0
    restored_victim_out="$(inside_verdict)" || restored_victim_rc=$?
    if [[ "$restored_victim_rc" != 1 ]] \
      || ! grep -q 'names 2 findings' <<< "$restored_victim_out"; then
      echo "restoring .git/$victim must restore the verdict; got $restored_victim_rc" >&2
      exit 1
    fi
  done

  # AND ONE INDIRECTION ALONG, WHICH NOBODY REPORTED. A linked worktree keeps its
  # `.git` in a FILE naming a gitdir; make the FILE unreadable and the old code
  # refused, because the open failed. Leave the file readable and make `HEAD` or
  # `commondir` INSIDE THE GITDIR IT NAMES unreadable and both probes exit 128
  # again, the open succeeded, and the old code conformed at exit 0 -- measured
  # here against fb219790 before this fixture was written. It is the same rule one
  # level down: a `.git` file that git would not resolve names either no gitdir at
  # all, which is nothing to examine, or one that must be examined before the
  # level is called empty.
  pointed_repo="$fixture_dir/repo-pointed-gitdir"
  pointed_wt="$fixture_dir/pointed-worktree"
  new_repo "$pointed_repo"
  echo seed > "$pointed_repo/seed.txt"
  git -C "$pointed_repo" add -A && git -C "$pointed_repo" commit -q -m base
  mkdir -p "$pointed_repo/reviews/findings"
  echo one > "$pointed_repo/reviews/findings/P2_correctness_202609100001_shared-name.md"
  echo two > "$pointed_repo/reviews/findings/P2_correctness_202609100002_shared-name.md"
  git -C "$pointed_repo" add -A \
    && git -C "$pointed_repo" commit -q -m 'two findings share a description'
  pointed_verdict() {
    PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
      "$BASH" "$branch_validator" 'fix-P2/correctness_shared-name' \
      "$pointed_wt/reviews/findings" 2>&1
  }
  if ! git -C "$pointed_repo" worktree add -q --detach "$pointed_wt" HEAD 2>/dev/null \
    || [[ ! -f "$pointed_wt/.git" ]]; then
    echo 'note: skipping the pointed-gitdir cases (this git made no linked worktree)' >&2
  else
    # The twin leaves this worktree's CHECKOUT and stays in its index, so the
    # filesystem fallback answers `conforms` rather than answering for some other
    # reason.
    rm "$pointed_wt/reviews/findings/P2_correctness_202609100002_shared-name.md"
    pointed_control_rc=0
    pointed_control_out="$(pointed_verdict)" || pointed_control_rc=$?
    if [[ "$pointed_control_rc" != 1 ]] \
      || ! grep -q 'names 2 findings' <<< "$pointed_control_out"; then
      echo "the control was meant to refuse an ambiguous name; got $pointed_control_rc" >&2
      exit 1
    fi
    # The gitdir the `.git` FILE names, taken from the file the way git takes it.
    pointed_gitdir=''
    while IFS= read -r pointed_line; do
      case "$pointed_line" in
        'gitdir: '*) pointed_gitdir="${pointed_line#gitdir: }" ;;
      esac
    done < "$pointed_wt/.git"
    if [[ -z "$pointed_gitdir" || ! -d "$pointed_gitdir" ]]; then
      echo 'note: skipping the pointed-gitdir cases (the .git file names no directory)' >&2
    else
      for pointed_victim in HEAD commondir; do
        if [[ ! -e "$pointed_gitdir/$pointed_victim" ]]; then
          continue
        fi
        if ! chmod 000 "$pointed_gitdir/$pointed_victim" 2>/dev/null \
          || git -C "$pointed_wt/reviews/findings" rev-parse --is-inside-work-tree >/dev/null 2>&1
        then
          chmod 644 "$pointed_gitdir/$pointed_victim" 2>/dev/null || true
          continue
        fi
        # The `.git` FILE is still perfectly readable, which is what made this
        # case survive the repair that closed the unreadable one.
        if [[ ! -r "$pointed_wt/.git" ]]; then
          chmod 644 "$pointed_gitdir/$pointed_victim"
          echo "note: skipping pointed $pointed_victim (it made the .git file unreadable)" >&2
          continue
        fi
        pointed_rc=0
        pointed_out="$(pointed_verdict)" || pointed_rc=$?
        chmod 644 "$pointed_gitdir/$pointed_victim"
        if [[ "$pointed_rc" == 0 ]]; then
          echo "an unreadable $pointed_victim in the gitdir a .git FILE names conformed" >&2
          exit 1
        fi
        if ! grep -qE 'cannot be examined|git could not say' <<< "$pointed_out"; then
          echo "an unexaminable pointed gitdir must refuse SAYING SO, not silently" >&2
          printf '%s\n' "$pointed_out" >&2
          exit 1
        fi
        pointed_back_rc=0
        pointed_back_out="$(pointed_verdict)" || pointed_back_rc=$?
        if [[ "$pointed_back_rc" != 1 ]] \
          || ! grep -q 'names 2 findings' <<< "$pointed_back_out"; then
          echo "restoring $pointed_victim must restore the verdict; got $pointed_back_rc" >&2
          exit 1
        fi
      done
    fi
  fi

  # AND THE INDEX ITSELF. `rev-parse --is-inside-work-tree` does not read the
  # index and answers `true`, so the refusal cannot come from discovery: it has to
  # come from `ls-files` exiting 128, which was thrown away by a process
  # substitution followed by an unconditional `return 0`. What followed was
  # worse than a narrowed set. With reviews/findings a COMMITTED SYMLINK whose
  # target text is a finding's filename -- which core.symlinks=false
  # materialises as a REGULAR FILE holding that name -- the recorded mode went
  # unread, the file was read as a FILE LISTING, and the link's target resolved
  # a fix-P*/ branch: a finding NOBODY HAS FILED, invented out of a read
  # failure, at exit 0.
  invented_repo="$fixture_dir/repo-invented-by-a-read-failure"
  invented_name='P2_correctness_202609100001_invented-by-a-read-failure.md'
  new_repo "$invented_repo"
  mkdir -p "$invented_repo/reviews"
  if [[ -L "$symlink_probe" ]] && ln -s "$invented_name" "$invented_repo/reviews/findings"; then
    git -C "$invented_repo" add -A \
      && git -C "$invented_repo" commit -q -m 'a findings directory that is a symlink naming a finding'
    git -C "$invented_repo" config core.symlinks false
    rm "$invented_repo/reviews/findings"
    git -C "$invented_repo" checkout -- reviews/findings
    invented_verdict() {
      PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
        "$BASH" "$branch_validator" 'fix-P2/correctness_invented-by-a-read-failure' \
        "$invented_repo/reviews/findings" 2>&1
    }
    if [[ -L "$invented_repo/reviews/findings" ]] || [[ ! -f "$invented_repo/reviews/findings" ]]; then
      echo 'note: skipping the invented-finding case (this git left the link a link)' >&2
    else
      invented_control_rc=0
      invented_control_out="$(invented_verdict)" || invented_control_rc=$?
      if [[ "$invented_control_rc" != 1 ]] \
        || ! grep -q 'as mode 120000' <<< "$invented_control_out"; then
        echo "the control was meant to refuse on the recorded mode; got $invented_control_rc" >&2
        exit 1
      fi
      # The reviewer executed `.git/index` and `.git/config` and both conformed:
      # one breaks `ls-files` with discovery still answering `true`, the other
      # breaks discovery itself. Two statuses, one wrong answer, so both are here.
      for invented_victim in index config; do
        if ! chmod 000 "$invented_repo/.git/$invented_victim" 2>/dev/null \
          || git -C "$invented_repo/reviews" ls-files -sz -- . >/dev/null 2>&1; then
          chmod 644 "$invented_repo/.git/$invented_victim" 2>/dev/null || true
          echo "note: skipping the unreadable-$invented_victim case (chmod had no effect on git)" >&2
          continue
        fi
        invented_rc=0
        invented_out="$(invented_verdict)" || invented_rc=$?
        chmod 644 "$invented_repo/.git/$invented_victim"
        if [[ "$invented_rc" == 0 ]]; then
          echo "an unreadable .git/$invented_victim invented a finding out of a symlink target, at exit 0" >&2
          exit 1
        fi
        if ! grep -qE 'does not read|git could not say' <<< "$invented_out"; then
          echo "an unreadable .git/$invented_victim must refuse SAYING SO" >&2
          printf '%s\n' "$invented_out" >&2
          exit 1
        fi
        restored_invented_rc=0
        restored_invented_out="$(invented_verdict)" || restored_invented_rc=$?
        if [[ "$restored_invented_rc" != 1 ]] \
          || ! grep -q 'as mode 120000' <<< "$restored_invented_out"; then
          echo "restoring .git/$invented_victim must restore the verdict; got $restored_invented_rc" >&2
          exit 1
        fi
      done
    fi
  else
    echo 'note: skipping the invented-finding case (this filesystem will not create a symlink)' >&2
  fi
fi

# ---- a RECORDED ANCESTOR is followed too, and one listing has many spellings ------------------
#
# The entries were decided by the mode git records, and so was the listing path
# itself -- and the path ABOVE it was not. Commit `reviews` as a SYMLINK to a
# sibling directory holding a finding and git calls it a `120000 blob`, `git
# status` stays empty, and no tree entry and no index entry is NAMED
# `reviews/findings/...`: the tree listings hold no finding and refuse at exit 1.
# Handing `reviews/findings` straight in asked the index from INSIDE the link --
# `git -C reviews` chdirs to `elsewhere` -- and resolved the name out of files no
# ledger holds at that path, at exit 0.
#
# AND THE SAME LISTING WRITTEN FIVE WAYS IS ONE LISTING. `reviews/findings/.`
# moved the question from `findings` to `.`, so the committed symlink AT
# `reviews/findings` that the plain spelling refused at exit 1 conformed at exit 0
# with three characters added. Every spelling is asserted, both for the paths that
# must refuse and for a real findings directory that must still resolve --
# otherwise "normalised" is indistinguishable from "rejected".
if [[ -L "$symlink_probe" ]]; then
  repo_r="$fixture_dir/repo-symlinked-ancestor"
  new_repo "$repo_r"
  echo seed > "$repo_r/seed.txt"
  git -C "$repo_r" add -A && git -C "$repo_r" commit -q -m base
  r_base="$(git -C "$repo_r" rev-parse HEAD)"
  mkdir -p "$repo_r/elsewhere/findings"
  echo fixture > "$repo_r/elsewhere/findings/P2_correctness_202609100001_no-ledger-entry.md"
  ln -s elsewhere "$repo_r/reviews"
  git -C "$repo_r" add -A && git -C "$repo_r" commit -q -m 'reviews is a symlink'
  r_head="$(git -C "$repo_r" rev-parse HEAD)"
  if ! git -C "$repo_r" ls-tree "$r_head" | grep -q '^120000 blob .*reviews$' \
    || [[ -n "$(git -C "$repo_r" status --porcelain)" ]]; then
    echo 'the fixture was meant to COMMIT reviews as a symlink, cleanly' >&2
    exit 1
  fi
  both_apis 'a findings directory reached through a committed symlink holds none' \
    "$repo_r" "$r_base" "$r_head" 'fix-P2/correctness_no-ledger-entry' 1
  spelling_case 'a symlinked ancestor, every spelling' \
    'fix-P2/correctness_no-ledger-entry' 1 "$repo_r/reviews/findings"

  # The same name through the REAL directory the link points at resolves, so the
  # case above is a rule about the path and not a listing read as empty.
  if ! PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
    "$BASH" "$branch_validator" 'fix-P2/correctness_no-ledger-entry' \
    "$repo_r/elsewhere/findings" >/dev/null 2>&1; then
    echo 'the directory the link points at must still resolve the name it holds' >&2
    exit 1
  fi

  # THE SAME COMMIT CHECKED OUT WHERE THE FILESYSTEM CARRIES NO SYMLINK. Under
  # core.symlinks=false the 120000 blob at `reviews` is materialised as a REGULAR
  # FILE holding `elsewhere`, so `reviews/findings` is not there at all: both APIs
  # refuse, one for a name it cannot find in the trees and one for a listing that
  # is neither a file nor a directory. One answer, two ways of arriving at it.
  git -C "$repo_r" config core.symlinks false
  rm "$repo_r/reviews"
  git -C "$repo_r" checkout -- reviews
  if [[ -L "$repo_r/reviews" ]] || [[ ! -f "$repo_r/reviews" ]]; then
    echo 'note: skipping the materialised-ancestor case (this git left the link a link)' >&2
  else
    both_apis 'a symlinked ancestor materialised as a file holds no finding either' \
      "$repo_r" "$r_base" "$r_head" 'fix-P2/correctness_no-ledger-entry' 1
    spelling_case 'a materialised ancestor, every spelling' \
      'fix-P2/correctness_no-ledger-entry' 1 "$repo_r/reviews/findings"
  fi
  # Put the link back, because the equivalence property below registers this
  # repository and the shape it registers is the committed symlink.
  rm "$repo_r/reviews"
  git -C "$repo_r" config core.symlinks true
  git -C "$repo_r" checkout -- reviews

  # AND A LINK ABOVE THE WORK TREE IS NOT ONE OF THESE. `/tmp` and `/var` are
  # symlinks on macOS and `mktemp -d` hands back a path through them, so a
  # blanket "no symlink above the listing" would answer the empty set for every
  # by-hand listing on that platform. Nothing above a work tree is recorded
  # anywhere, so nothing above one can disagree with a tree listing. This builds
  # that shape on purpose: a link to the directory a repository sits in.
  outer_link="$fixture_dir/link-to-the-repo-parent"
  outer_dir="$fixture_dir/outer"
  mkdir -p "$outer_dir"
  repo_s="$outer_dir/repo-under-a-linked-parent"
  new_repo "$repo_s"
  commit_finding "$repo_s" 'P3_liveness_202609100007_under-a-linked-parent.md' 'a real finding'
  if ln -s outer "$outer_link" 2>/dev/null && [[ -L "$outer_link" ]]; then
    if ! PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
      "$BASH" "$branch_validator" 'fix-P3/liveness_under-a-linked-parent' \
      "$outer_link/repo-under-a-linked-parent/reviews/findings" >/dev/null 2>&1; then
      echo 'a symlink ABOVE the work tree must not refuse a listing inside it' >&2
      exit 1
    fi
  else
    echo 'note: skipping the link-above-the-work-tree case (no symlink could be made)' >&2
  fi
fi

# And a real findings directory answers the same in every spelling, which is what
# makes the refusals above about the path and not about the extra characters.
spelling_repo="$fixture_dir/repo-spellings"
new_repo "$spelling_repo"
commit_finding "$spelling_repo" 'P3_liveness_202609100005_written-five-ways.md' 'a real finding'
spelling_case 'a real findings directory, every spelling' \
  'fix-P3/liveness_written-five-ways' 0 "$spelling_repo/reviews/findings"
spelling_case 'and a name it does not hold, every spelling' \
  'fix-P3/liveness_not-in-this-directory' 1 "$spelling_repo/reviews/findings"

# A `..` AFTER A NAMED COMPONENT IS REFUSED AND NOT GUESSED AT. `a/b/..` is `a`
# when `b` is a directory and the LINK'S parent when `b` is a symlink, so
# reducing it lexically answers about a path the caller did not name and
# resolving it on the filesystem follows the link this rule exists to refuse. A
# LEADING run of `..` is a starting directory, above every component this judges,
# and is accepted.
dotdot_rc=0
dotdot_out="$(PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
  "$BASH" "$branch_validator" 'fix-P3/liveness_written-five-ways' \
  "$spelling_repo/reviews/findings/../findings" 2>&1)" || dotdot_rc=$?
if [[ "$dotdot_rc" != 1 ]] || ! grep -q "holds a '\.\.' after a named component" <<< "$dotdot_out"; then
  echo "a '..' after a named component must be refused, saying so; got $dotdot_rc" >&2
  exit 1
fi
if ! ( cd "$spelling_repo/reviews" && PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
  "$BASH" "$branch_validator" 'fix-P3/liveness_written-five-ways' \
  '../reviews/findings' >/dev/null 2>&1 ); then
  echo 'a LEADING .. is a starting directory and must still resolve' >&2
  exit 1
fi

# ---- the equivalence, as a property rather than a list of cases -------------------------------
#
# Three of the four P1s in this pull request's reviews were the two documented
# ways in disagreeing about one commit, so the claim is tested as a property: for
# every repository below and every branch name below, the answer through the
# three tree listings EQUALS the answer through the working tree's
# reviews/findings/ directory. The expectation is not asserted here at all --
# each shape's expected answer is asserted in its own section above -- because
# what this checks is that the two APIs cannot part company, including for pairs
# nobody thought to write down.
#
# The repositories are the ones where the property is even claimable: the
# directory is ONE listing where the trees are three, so it holds for a
# repository with no finding at the merge base and none deleted between there and
# the head.
equivalence_repos=()
register_equivalence() {  # register_equivalence <repo> <base> <head>
  equivalence_repos[${#equivalence_repos[@]}]="$1|$2|$3"
}

eq_plain="$fixture_dir/eq-plain-findings"
new_repo "$eq_plain"
echo seed > "$eq_plain/seed.txt"
git -C "$eq_plain" add -A && git -C "$eq_plain" commit -q -m base
eq_plain_base="$(git -C "$eq_plain" rev-parse HEAD)"
mkdir -p "$eq_plain/reviews/findings"
echo one > "$eq_plain/reviews/findings/P2_correctness_202609100001_shared-name.md"
echo two > "$eq_plain/reviews/findings/P2_correctness_202609100002_shared-name.md"
echo three > "$eq_plain/reviews/findings/P3_liveness_202609100003_a-real-finding.md"
git -C "$eq_plain" add -A && git -C "$eq_plain" commit -q -m 'three findings, two sharing a description'
register_equivalence "$eq_plain" "$eq_plain_base" "$(git -C "$eq_plain" rev-parse HEAD)"

eq_empty="$fixture_dir/eq-no-findings-at-all"
new_repo "$eq_empty"
echo seed > "$eq_empty/seed.txt"
git -C "$eq_empty" add -A && git -C "$eq_empty" commit -q -m base
eq_empty_base="$(git -C "$eq_empty" rev-parse HEAD)"
echo more > "$eq_empty/other.txt"
git -C "$eq_empty" add -A && git -C "$eq_empty" commit -q -m 'nothing to do with findings'
register_equivalence "$eq_empty" "$eq_empty_base" "$(git -C "$eq_empty" rev-parse HEAD)"

# A twin the index records and the checkout does not hold, which is where the
# filesystem and the ledger part company if anything lets them.
eq_recorded="$fixture_dir/eq-recorded-not-materialised"
new_repo "$eq_recorded"
echo seed > "$eq_recorded/seed.txt"
git -C "$eq_recorded" add -A && git -C "$eq_recorded" commit -q -m base
eq_recorded_base="$(git -C "$eq_recorded" rev-parse HEAD)"
mkdir -p "$eq_recorded/reviews/findings"
echo one > "$eq_recorded/reviews/findings/P2_correctness_202609100001_shared-name.md"
echo two > "$eq_recorded/reviews/findings/P2_correctness_202609100002_shared-name.md"
git -C "$eq_recorded" add -A && git -C "$eq_recorded" commit -q -m 'two findings share a description'
register_equivalence "$eq_recorded" "$eq_recorded_base" "$(git -C "$eq_recorded" rev-parse HEAD)"
rm "$eq_recorded/reviews/findings/P2_correctness_202609100002_shared-name.md"

# A subdirectory wearing a finding's name, which is a tree and not a finding.
eq_subdir="$fixture_dir/eq-directory-named-like-a-finding"
new_repo "$eq_subdir"
echo seed > "$eq_subdir/seed.txt"
git -C "$eq_subdir" add -A && git -C "$eq_subdir" commit -q -m base
eq_subdir_base="$(git -C "$eq_subdir" rev-parse HEAD)"
mkdir -p "$eq_subdir/reviews/findings/P2_correctness_202609100001_shared-name.md"
echo inside > "$eq_subdir/reviews/findings/P2_correctness_202609100001_shared-name.md/inside.txt"
echo real > "$eq_subdir/reviews/findings/P3_liveness_202609100003_a-real-finding.md"
git -C "$eq_subdir" add -A && git -C "$eq_subdir" commit -q -m 'a directory wearing a finding name'
register_equivalence "$eq_subdir" "$eq_subdir_base" "$(git -C "$eq_subdir" rev-parse HEAD)"

if [[ -L "$symlink_probe" ]]; then
  # The findings directory itself a committed symlink, and an ancestor of it a
  # committed symlink: the two shapes the last round's P1s were.
  register_equivalence "$repo_p" "$p_base" "$p_head"
  register_equivalence "$repo_r" "$r_base" "$r_head"
  register_equivalence "$repo_k" "$k_base" "$k_head"
fi

# The branch names: ones that resolve, ones that resolve nothing, one that is
# ambiguous, and one whose severity or category is wrong for the file that is
# there. A pair that cannot happen in a given repository is exactly as
# interesting as one that can -- both APIs must still agree.
equivalence_branches=(
  'fix-P2/correctness_shared-name'
  'fix-P3/liveness_a-real-finding'
  'fix-P2/correctness_no-ledger-entry'
  'fix-P2/correctness_not-a-finding'
  'fix-P3/liveness_written-five-ways'
  'fix-P1/correctness_never-filed-anywhere'
  'fix-P3/correctness_shared-name'
)

equivalence_pairs=0
for equivalence_entry in "${equivalence_repos[@]}"; do
  eq_repo="${equivalence_entry%%|*}"
  eq_rest="${equivalence_entry#*|}"
  eq_base="${eq_rest%%|*}"
  eq_head="${eq_rest##*|}"
  for eq_branch in "${equivalence_branches[@]}"; do
    eq_tree="$(verdict "$eq_repo" "$eq_base" "$eq_head" "$eq_branch")"
    eq_dir=0
    PR_NUMBER= LEGACY_BRANCHES="$fixture_dir/legacy.txt" \
      "$BASH" "$branch_validator" "$eq_branch" "$eq_repo/reviews/findings" \
      >/dev/null 2>&1 || eq_dir=$?
    if [[ "$eq_tree" != "$eq_dir" ]]; then
      echo "the two APIs disagree: ${eq_repo##*/} / $eq_branch -> trees $eq_tree, directory $eq_dir" >&2
      exit 1
    fi
    equivalence_pairs=$(( equivalence_pairs + 1 ))
  done
done
if (( equivalence_pairs < 28 )); then
  echo "the equivalence property checked only $equivalence_pairs pairs, which is too few to mean anything" >&2
  exit 1
fi

# ---- the shape rule: the unsafe call must be impossible to WRITE, not just absent -------------
#
# Four rounds of finding these one at a time produced more of them each round,
# and three of the four in the last round were introduced by the repairs
# themselves. A gate can only test the instances somebody imagined. So the
# validator routes every external probe and every file read through two audited
# helpers, and this asserts THE SHAPE: below the AUDITED HELPERS END marker there
# is no `git`, no `<` redirection from a path, no process substitution, and no
# command handed a file to read. What the helpers do is then a dozen lines to
# review rather than forty call sites to audit.
#
# THE INSTRUMENT IS TESTED FIRST, because a rule that matches nothing would pass
# this file forever while proving nothing: each shape below is appended to a COPY
# of the validator and must be caught.
shape_violations() {  # shape_violations <script> -> "<line>: <text>" per violation
  awk '
    BEGIN { audited = 0; heredoc = 0; instring = 0
            # Command position: the start of a line, after a separator, after a
            # keyword that takes a command, or after a wrapper that RUNS one --
            # `command git`, `exec git`, `env git`, `xargs git` are all git.
            cmd = "(^[[:space:]]*|[;&|(){}!][[:space:]]*|(^|[[:space:]])(then|else|do|if|elif|while|until|command|exec|env|eval|xargs|nohup|time|builtin)[[:space:]]+)"
          }
    /^# ==== AUDITED HELPERS BEGIN/ { audited = 1; next }
    /^# ==== AUDITED HELPERS END/   { audited = 0; next }
    heredoc { if ($0 == "EOF") { heredoc = 0 } next }
    {
      line = $0
      # A line CONTINUING a string carries prose, not code: keep only what
      # follows the quote that closes it.
      if (instring) {
        i = index(line, "\"")
        if (i == 0) { next }
        line = substr(line, i + 1)
        instring = 0
      }
      if (line ~ /^[[:space:]]*#/) { next }
      if (line ~ /<<'"'"'EOF'"'"'/) { heredoc = 1 }
      quotes = gsub(/"/, "\"", line)
      if (quotes % 2 == 1) { instring = 1 }
      if (audited) { next }
      if (line ~ cmd "git[[:space:]]")                       { print FNR ": " $0; next }
      if (line ~ /-v[[:space:]]+git([[:space:]]|$)/)          { print FNR ": " $0; next }
      if (line ~ /<\(/)                                       { print FNR ": " $0; next }
      if (line ~ /\$\(</)                                     { print FNR ": " $0; next }
      if (line ~ /(^|[[:space:]0-9])<[[:space:]]*["'"'"'$\/]/)  { print FNR ": " $0; next }
      if (line ~ cmd "(cat|dd|head|tail|od|xxd|wc)[[:space:]]+[^<>[:space:]]") { print FNR ": " $0; next }
    }
  ' "$1"
}

# The quote tracking above assumes no escaped double quote, so that is asserted
# rather than hoped for.
if grep -q '\\"' "$branch_validator"; then
  echo 'validate-pr-branch.sh has an escaped double quote, which the shape rule cannot parse' >&2
  exit 1
fi
if ! grep -q '^# ==== AUDITED HELPERS BEGIN' "$branch_validator" \
  || ! grep -q '^# ==== AUDITED HELPERS END' "$branch_validator"; then
  echo 'validate-pr-branch.sh has lost its audited-helpers markers, so the shape rule bounds nothing' >&2
  exit 1
fi

shape_mutant="$fixture_dir/shape-mutant.sh"
shape_probe() {  # shape_probe <line to append>
  cp -- "$branch_validator" "$shape_mutant"
  printf '%s\n' "$1" >> "$shape_mutant"
  if [[ -z "$(shape_violations "$shape_mutant")" ]]; then
    echo "the shape rule does not catch [$1], so it proves nothing about the rest" >&2
    exit 1
  fi
}
shape_probe 'git status >/dev/null'
shape_probe 'if git diff --quiet; then :; fi'
shape_probe 'mode="$(git -C "$d" ls-files -s -- x)"'
shape_probe 'echo hi | git hash-object --stdin'
shape_probe '{ git status; } >/dev/null'
shape_probe 'command -v git >/dev/null'
shape_probe 'while IFS= read -r record; do :; done < <(git ls-files -z)'
shape_probe 'IFS= read -r line < "$listing"'
shape_probe 'while read -r l; do :; done < "$f"'
shape_probe 'out="$(cat -- "$listing")"'
shape_probe 'n="$(wc -c < "$listing")"'
shape_probe 'x=$(< "$listing")'
shape_probe 'head -1 "$f"'
shape_probe 'dd if="$f" of=/dev/null'
shape_probe 'command git status'
shape_probe 'exec git status'
shape_probe 'env git status'
shape_probe 'xargs git add'
shape_probe 'eval git status'
shape_probe 'while read -r x; do :; done < /etc/hostname'

# And the file as it stands has none of them.
shape_found="$(shape_violations "$branch_validator")"
if [[ -n "$shape_found" ]]; then
  echo 'validate-pr-branch.sh runs git or reads a file outside its audited helpers:' >&2
  printf '%s\n' "$shape_found" >&2
  exit 1
fi

# The audited region has to stay small enough that reading it is the whole audit.
shape_region_lines="$(awk '
  /^# ==== AUDITED HELPERS BEGIN/ { inside = 1 }
  inside { n = n + 1 }
  /^# ==== AUDITED HELPERS END/ { inside = 0 }
  END { print n }' "$branch_validator")"
if (( shape_region_lines > 200 )); then
  echo "the audited helpers have grown to $shape_region_lines lines, which is no longer an audit" >&2
  exit 1
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
