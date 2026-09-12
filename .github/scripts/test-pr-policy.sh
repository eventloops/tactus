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

# The merge-base listing: what findings/ holds at the branch point --
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

# The pull request's own commits: findings/ as each commit between the
# merge base and the head left it. A finding filed in one commit and deleted by
# its repair in the next appears here TWICE -- once for the commit that held it
# and once for a second commit that did -- and in NEITHER endpoint tree.
cat > "$fixture_dir/range-findings.txt" <<'EOF'
P2_correctness_202609101200_filed-and-repaired-in-one-range.md
P2_correctness_202609101200_filed-and-repaired-in-one-range.md
EOF

# branch_pass / branch_fail resolve against the MERGE-BASE listing alone, which
# is what a caller that passes one listing gets.
branch_pass() {
  local name="$1" branch="$2"
  if ! "$BASH" "$branch_validator" "$branch" "$fixture_dir/findings.txt" >/dev/null 2>&1; then
    echo "expected branch to pass: $name ($branch)" >&2
    exit 1
  fi
}

branch_fail() {
  local name="$1" branch="$2"
  if "$BASH" "$branch_validator" "$branch" "$fixture_dir/findings.txt" >/dev/null 2>&1; then
    echo "expected branch to fail: $name ($branch)" >&2
    exit 1
  fi
}

# pair_pass / pair_fail resolve against BOTH ends, which is what the workflow
# passes and what a real pull request is judged by.
pair_pass() {
  local name="$1" branch="$2"
  if ! "$BASH" "$branch_validator" "$branch" \
    "$fixture_dir/findings.txt" "$fixture_dir/head-findings.txt" >/dev/null 2>&1; then
    echo "expected branch to pass against base and head: $name ($branch)" >&2
    exit 1
  fi
}

pair_fail() {
  local name="$1" branch="$2"
  if "$BASH" "$branch_validator" "$branch" \
    "$fixture_dir/findings.txt" "$fixture_dir/head-findings.txt" >/dev/null 2>&1; then
    echo "expected branch to fail against base and head: $name ($branch)" >&2
    exit 1
  fi
}

# triple_pass / triple_fail pass the base tree, the head tree and the range,
# which is what the workflow passes and what a real pull request is judged by.
triple_pass() {
  local name="$1" branch="$2"
  if ! "$BASH" "$branch_validator" "$branch" "$fixture_dir/findings.txt" \
    "$fixture_dir/head-findings.txt" "$fixture_dir/range-findings.txt" >/dev/null 2>&1; then
    echo "expected branch to pass over the range: $name ($branch)" >&2
    exit 1
  fi
}

triple_fail() {
  local name="$1" branch="$2"
  if "$BASH" "$branch_validator" "$branch" "$fixture_dir/findings.txt" \
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
# pull request that touches findings/ and nothing else. The validator
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
# and repairs it in the next -- deleting the file, as findings/README.md
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
  if "$BASH" "$branch_validator" 'fix-P2/performance_split-twin' \
    "$fixture_dir/findings.txt" "$unreadable" >/dev/null 2>&1; then
    echo 'expected an unreadable second listing to refuse, not to conform' >&2
    exit 1
  fi
  # And a failure on the FIRST listing must not be masked by a good second one
  # that resolves the name on its own.
  if "$BASH" "$branch_validator" 'fix-P2/correctness_filed-by-the-pull-request-that-repairs-it' \
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
  if "$BASH" "$branch_validator" 'fix-P1/correctness_pid-identity-under-a-host-wildcard-waiter' \
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
  out="$("$BASH" "$branch_validator" 'fix-P2/correctness_shared-name' "$@" 2>&1)" || rc=$?
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
      SWAP_COUNTER="$toctou_dir/opens"
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
if ! "$BASH" "$branch_validator" 'fix-P1/correctness_never-filed' >/dev/null 2>&1; then
  echo 'expected the grammar alone to pass without a findings listing' >&2
  exit 1
fi

# A listing that does not exist is a caller error, and it must be refused before
# the resolution runs rather than read as a finding that was never filed. The
# branch here needs no resolution at all, so only an eager check fails it.
if "$BASH" "$branch_validator" 'feature/pr9-repair-execution' \
  "$fixture_dir/no-such-listing" >/dev/null 2>&1; then
  echo 'expected a findings listing that does not exist to fail' >&2
  exit 1
fi

# ---- the migration list is retired, and its entries are refused ---------------------------
#
# The rule shipped with a migration list of the pull requests that predated it,
# `<number> <head branch>`. A listed pull request was accepted with a warning,
# and PR_NUMBER was the identity the entry was matched on: the number was the
# whole of what bought the exemption, because a bare name would have exempted
# anyone who later typed it. That list reached zero open pull requests and the
# owner ruled on 2026-09-09 that every head branch conforms, so the entries it
# carried are the cases that must now be REFUSED -- each under the number it was
# listed with, which is the input that used to buy it the opposite verdict.
# The refusal is asserted by TEXT and not just by exit code, so an entry that
# started failing for some other reason -- a listing this run could not build,
# a caller error -- would not be read as the rule binding.
retired_entry() {  # retired_entry <name> <pr-number> <branch> <refusal text>
  local name="$1" pr="$2" branch="$3" want="$4" out rc=0
  out="$(PR_NUMBER="$pr" "$BASH" "$branch_validator" "$branch" 2>&1)" || rc=$?
  if (( rc == 0 )); then
    echo "expected a retired entry to be refused: $name (#$pr $branch)" >&2
    exit 1
  fi
  if ! grep -qF "$want" <<< "$out"; then
    echo "a retired entry must be refused for its prefix: $name (#$pr $branch)" >&2
    printf '%s\n' "$out" >&2
    exit 1
  fi
}

retired_entry 'the first listed pull request' 135 'sweep/workspace-manager-fixture' \
  "'sweep/' is not a known branch prefix"
retired_entry 'a listed codex/findings-p3 entry' 222 'codex/findings-p3-1a57a2730a12' \
  "'codex/' is not a known branch prefix"
retired_entry 'a listed codex/findings entry' 179 'codex/findings-9dc6604a62e3' \
  "'codex/' is not a known branch prefix"
retired_entry 'a listed codex/sweep entry' 189 'codex/sweep-1dcb506fe31f' \
  "'codex/' is not a known branch prefix"
# The two the list picked up when fix/ was retired. Their prefix has a refusal
# of its own, and it is the one they must get: the entry is gone, and so is the
# prefix it was written on.
retired_entry 'a stranded fix/ entry' 139 'fix/rundir-unreadable-is-not-empty' \
  "'fix/' was retired from the vocabulary"
retired_entry 'the other stranded fix/ entry' 145 'fix/sampler-kill-and-inspection' \
  "'fix/' was retired from the vocabulary"

# And the identity itself is inert, on BOTH verdicts and for every shape the old
# lookup distinguished: the number an entry carried, a number that was never
# listed, no identity at all, and a non-numeric one. A conforming name conforms
# under all of them and a retired name is refused under all of them, so nothing
# is left for an identity to buy.
for pr_identity in '' '135' '222' '999' 'abc' '-e135'; do
  if ! PR_NUMBER="$pr_identity" \
    "$BASH" "$branch_validator" 'ci/pages-nojekyll' >/dev/null 2>&1; then
    echo "a conforming name was refused with PR_NUMBER=[$pr_identity]" >&2
    exit 1
  fi
  if PR_NUMBER="$pr_identity" \
    "$BASH" "$branch_validator" 'sweep/workspace-manager-fixture' >/dev/null 2>&1; then
    echo "a retired name conformed with PR_NUMBER=[$pr_identity]" >&2
    exit 1
  fi
done

# The override that pointed the validator at a list of its own is inert too. A
# file naming the branch, handed over under the variable that used to name it,
# and the number beside it: the verdict is still a refusal, so the exemption
# cannot be reached from the environment either.
cat > "$fixture_dir/stale-exemption-list.txt" <<'EOF'
222 codex/findings-p3-1a57a2730a12
135 sweep/workspace-manager-fixture
EOF
if PR_NUMBER=222 LEGACY_BRANCHES="$fixture_dir/stale-exemption-list.txt" \
  "$BASH" "$branch_validator" 'codex/findings-p3-1a57a2730a12' >/dev/null 2>&1; then
  echo 'LEGACY_BRANCHES still buys an exemption' >&2
  exit 1
fi

# ---- the listings themselves, built from real repositories ---------------------------------
#
# Everything above tests what the validator does with a listing. NOTHING above
# tests whether the listing is right, and that is where two frontier reviews
# found defects: the range was built with `git log -- findings/`, which
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
  mkdir -p "$1/findings"
  echo fixture > "$1/findings/$2"
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
  "$BASH" "$branch_validator" "$branch" \
    "$out/merge-base-findings" "$out/head-findings" "$out/range-findings" \
    >/dev/null 2>&1 || rc=$?
  echo "$rc"
}

# both_apis <label> <repo> <target> <head> <branch> <expected>: THE TWO
# DOCUMENTED WAYS IN, ASKED ABOUT ONE COMMIT. The workflow builds three listings
# with findings-in-range.sh; a maintainer running the validator by hand gives it
# that working tree's findings/ as a directory. One commit gets one
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
  "$BASH" "$branch_validator" "$branch" "$repo/findings" >/dev/null 2>&1 || dir_rc=$?
  if [[ "$tree_rc" != "$dir_rc" ]]; then
    echo "$label ($branch): the tree listings answered $tree_rc, the directory $dir_rc" >&2
    exit 1
  fi
  if [[ "$tree_rc" != "$want" ]]; then
    echo "$label ($branch): both APIs answered $tree_rc, and $want was expected" >&2
    exit 1
  fi
}

# ONE LISTING HAS MANY SPELLINGS AND THEY MUST ANSWER ALIKE. `findings`,
# `findings/`, `findings/.`, `findings/./`,
# `reviews//findings` and `reviews/./findings` name one directory, and they did
# not answer alike: the last component decides what the path IS, and with `/.`
# appended the last component was `.`, so a COMMITTED SYMLINK at
# `findings` that the plain spelling refused at exit 1 conformed at exit 0
# with three characters added. Every case that asserts a verdict for a path
# asserts it for all six.
spellings_of() {  # spellings_of <path> -> the same path, written every way
  printf '%s\n' "$1" "$1/" "$1/." "$1/./" "${1%/*}//${1##*/}" "${1%/*}/./${1##*/}"
}

spelling_case() {  # spelling_case <label> <branch> <want-exit> <path>
  local label="$1" branch="$2" want="$3" path="$4" spelling rc
  while IFS= read -r spelling; do
    rc=0
    "$BASH" "$branch_validator" "$branch" "$spelling" >/dev/null 2>&1 || rc=$?
    if [[ "$rc" != "$want" ]]; then
      echo "$label: '$spelling' answered $rc and $want was expected" >&2
      exit 1
    fi
  done < <(spellings_of "$path")
}

# A completed repair: commit A files the finding, commit B repairs it and
# DELETES the file as findings/README.md requires, and the whole thing
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
git -C "$repo_a" rm -q "findings/P2_correctness_202609101200_a-new-bug.md"
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
git -C "$repo_b" rm -q "findings/P2_correctness_202609101200_a-new-bug.md"
git -C "$repo_b" commit -q -m 'B: repair it and delete the finding'
git -C "$repo_b" checkout -q -b trunk "$b_base"
commit_finding "$repo_b" 'P2_correctness_202609111500_a-new-bug.md' 'the receiving branch files its own'
git -C "$repo_b" merge -q --no-ff side -m 'merge the completed repair'
b_head="$(git -C "$repo_b" rev-parse HEAD)"

( cd "$repo_b" && "$BASH" "$range_script" "$b_base" "$b_head" "$repo_b/out" >/dev/null 2>&1 ) \
  || { echo 'findings-in-range.sh failed on the hidden-twin repository' >&2; exit 1; }
if "$BASH" "$branch_validator" 'fix-P2/correctness_a-new-bug' \
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
mkdir -p "$repo_c/findings"
echo fixture > "$repo_c/findings/P2_correctness_202609101200_only-in-merges.md"
git -C "$repo_c" add -A
git -C "$repo_c" commit -q -m 'M1: a merge that files the finding in the merge itself'
git -C "$repo_c" checkout -q -b q
echo c > "$repo_c/c.txt"; git -C "$repo_c" add -A; git -C "$repo_c" commit -q -m c3
git -C "$repo_c" checkout -q trunk
echo d > "$repo_c/d.txt"; git -C "$repo_c" add -A; git -C "$repo_c" commit -q -m c4
git -C "$repo_c" merge -q --no-commit --no-ff q >/dev/null 2>&1 || true
git -C "$repo_c" rm -q "findings/P2_correctness_202609101200_only-in-merges.md"
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
if ! "$BASH" "$branch_validator" 'fix-P1/liveness_untouched-by-this-branch' \
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
mkdir -p "$repo_e/findings"
echo one > "$repo_e/findings/P2_correctness_202609010000_shared-name.md"
echo two > "$repo_e/findings/P2_correctness_202609020000_shared-name.md"
git -C "$repo_e" add -A && git -C "$repo_e" commit -q -m 'branch point: two findings share a description'
e_branch_point="$(git -C "$repo_e" rev-parse HEAD)"
git -C "$repo_e" checkout -q -b pr
git -C "$repo_e" rm -q 'findings/P2_correctness_202609010000_shared-name.md'
git -C "$repo_e" commit -q -m 'the pull request repairs the first, deleting the file'
e_head="$(git -C "$repo_e" rev-parse HEAD)"
git -C "$repo_e" checkout -q -B trunk "$e_branch_point"
git -C "$repo_e" rm -q 'findings/P2_correctness_202609010000_shared-name.md'
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
git -C "$repo_f" rm -q 'findings/P2_correctness_202609010000_repaired-both-sides.md'
git -C "$repo_f" commit -q -m 'the pull request repairs it, deleting the file'
f_head="$(git -C "$repo_f" rev-parse HEAD)"
git -C "$repo_f" checkout -q -B trunk "$f_branch_point"
git -C "$repo_f" rm -q 'findings/P2_correctness_202609010000_repaired-both-sides.md'
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
mkdir -p "$repo_i/findings"
echo one > "$repo_i/findings/P2_correctness_202609010000_shared-name.md"
echo two > "$repo_i/findings/P2_correctness_202609020000_shared-name.md"
git -C "$repo_i" add -A && git -C "$repo_i" commit -q -m 'branch point: two findings share a description'
i_branch_point="$(git -C "$repo_i" rev-parse HEAD)"
# C, the repair, on a commit that this pull request and an earlier one both
# carry. Nothing about it is exotic: a batch pull request opened from this
# branch point legitimately holds it.
git -C "$repo_i" checkout -q -b shared-repair
git -C "$repo_i" rm -q 'findings/P2_correctness_202609010000_shared-name.md'
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
git -C "$repo_g" rm -q 'findings/P2_correctness_202609101200_a-new-bug.md'
git -C "$repo_g" commit -q -m 'B: repair it and delete the finding'
g_head="$(git -C "$repo_g" rev-parse HEAD)"
if [[ -n "$(git -C "$repo_g" diff --name-only "$g_base" "$g_head" -- findings/)" ]]; then
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
# so committing findings/P2_correctness_<ts>_<desc>.md/placeholder --
# which creates a DIRECTORY and no finding -- satisfied fix-P2/correctness_<desc>
# and all three workflow steps returned exit 0 with no finding in existence.
repo_h="$fixture_dir/repo-directory-not-a-finding"
new_repo "$repo_h"
echo seed > "$repo_h/seed.txt"
git -C "$repo_h" add -A && git -C "$repo_h" commit -q -m base
h_base="$(git -C "$repo_h" rev-parse HEAD)"
mkdir -p "$repo_h/findings/P2_correctness_202609101200_missing-repair.md"
echo placeholder > "$repo_h/findings/P2_correctness_202609101200_missing-repair.md/placeholder"
# A real finding beside it, so the case proves the filter and not an empty tree.
echo fixture > "$repo_h/findings/P3_liveness_202609101300_a-real-finding.md"
git -C "$repo_h" add -A && git -C "$repo_h" commit -q -m 'a directory named like a finding'
h_head="$(git -C "$repo_h" rev-parse HEAD)"
if ! git -C "$repo_h" ls-tree "$h_head" findings/ | grep -q '^040000 tree '; then
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
if "$BASH" "$branch_validator" 'fix-P2/correctness_missing-repair' "$dir_input" >/dev/null 2>&1; then
  echo 'a subdirectory named like a finding must not resolve when a directory is the listing' >&2
  exit 1
fi
if ! "$BASH" "$branch_validator" 'fix-P3/liveness_a-real-finding' "$dir_input" >/dev/null 2>&1; then
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
if ! "$BASH" "$branch_validator" 'fix-P2/performance_split-twin' \
  "$fixture_dir/findings.txt" "$twin_dir" >/dev/null 2>&1; then
  : # both twins visible, so the name is ambiguous and refused, which is the control
else
  echo 'the twin across a file listing and a directory listing must be ambiguous' >&2
  exit 1
fi
if [[ "$(id -u)" -ne 0 ]] && chmod 600 "$twin_dir" 2>/dev/null && [[ ! -x "$twin_dir" ]]; then
  if "$BASH" "$branch_validator" 'fix-P2/performance_split-twin' \
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
git -C "$repo_j" rm -q 'findings/P2_correctness_202609010000_shared-name.md'
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
  mkdir -p "$repo_k/findings"
  ln -s ../../seed.txt "$repo_k/findings/P2_correctness_202609101200_not-a-finding.md"
  # A real finding beside it, so the case proves a filter and not an empty tree.
  echo fixture > "$repo_k/findings/P3_liveness_202609101300_a-real-finding.md"
  git -C "$repo_k" add -A && git -C "$repo_k" commit -q -m 'a symlink named like a finding'
  k_head="$(git -C "$repo_k" rev-parse HEAD)"
  if ! git -C "$repo_k" ls-tree "$k_head" findings/ | grep -q '^120000 blob '; then
    echo 'the fixture was meant to commit a SYMLINK named like a finding' >&2
    exit 1
  fi
  k_tree="$(verdict "$repo_k" "$k_base" "$k_head" 'fix-P2/correctness_not-a-finding')"
  if [[ "$k_tree" != 1 ]]; then
    echo "a committed symlink named like a finding must not resolve a fix-P*/ branch; got $k_tree" >&2
    exit 1
  fi
  # The same commit judged the documented by-hand way: that working tree's
  # findings/ handed straight in as the listing. This is the path that
  # accepted, and it must now agree with the one above.
  if "$BASH" "$branch_validator" 'fix-P2/correctness_not-a-finding' \
    "$repo_k/findings" >/dev/null 2>&1; then
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
  if ! "$BASH" "$branch_validator" 'fix-P3/liveness_a-real-finding' \
    "$repo_k/findings" >/dev/null 2>&1; then
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
  dangling_out="$("$BASH" "$branch_validator" 'fix-P2/correctness_not-a-finding' "$dangling_dir" 2>&1)" \
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
  k_materialised='findings/P2_correctness_202609101200_not-a-finding.md'
  git -C "$repo_k" config core.symlinks false
  rm "$repo_k/$k_materialised"
  git -C "$repo_k" checkout -- "$k_materialised"
  if [[ -L "$repo_k/$k_materialised" ]] || [[ ! -f "$repo_k/$k_materialised" ]] \
    || [[ "$(git -C "$repo_k" ls-files -s -- "$k_materialised" | cut -d' ' -f1)" != 120000 ]]; then
    echo 'note: skipping the core.symlinks=false case (this git left the link a link)' >&2
  else
    if "$BASH" "$branch_validator" 'fix-P2/correctness_not-a-finding' \
      "$repo_k/findings" >/dev/null 2>&1; then
      echo 'a committed symlink checked out as a regular file resolved through the directory listing' >&2
      exit 1
    fi
    # And it is still a filter and not a listing read as empty: the regular file
    # beside it resolves, through a checkout that materialised neither as a link.
    if ! "$BASH" "$branch_validator" 'fix-P3/liveness_a-real-finding' \
      "$repo_k/findings" >/dev/null 2>&1; then
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
      && ! git -C "$repo_k/findings" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
      unreadable_rc=0
      unreadable_out="$("$BASH" "$branch_validator" 'fix-P2/correctness_not-a-finding' \
        "$repo_k/findings" 2>&1)" || unreadable_rc=$?
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
      if ! "$BASH" "$branch_validator" 'fix-P3/liveness_a-real-finding' \
        "$repo_k/findings" >/dev/null 2>&1; then
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
# `findings` as a SYMLINK to a sibling directory and git calls it a
# `120000 blob`, `git status` stays empty, and the tree listings hold no finding
# under findings/ and refuse at exit 1 -- while handing that path
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
  ln -s ../elsewhere "$repo_p/findings"
  git -C "$repo_p" add -A && git -C "$repo_p" commit -q -m 'a findings directory that is a symlink'
  p_head="$(git -C "$repo_p" rev-parse HEAD)"
  if ! git -C "$repo_p" ls-tree "$p_head" reviews/ | grep -q '^120000 blob .*findings$' \
    || [[ -n "$(git -C "$repo_p" status --porcelain)" ]]; then
    echo 'the fixture was meant to COMMIT findings as a symlink, cleanly' >&2
    exit 1
  fi
  both_apis 'a symlinked findings directory holds no finding' \
    "$repo_p" "$p_base" "$p_head" 'fix-P2/correctness_no-ledger-entry' 1
  # And with `/.` appended it conformed at exit 0 on this very commit, because
  # the question moved from `findings` to `.` -- the same listing, refused one way
  # and accepted the other.
  spelling_case 'a symlinked findings directory, every spelling' \
    'fix-P2/correctness_no-ledger-entry' 1 "$repo_p/findings"

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
  rm "$repo_p/findings"
  git -C "$repo_p" checkout -- findings
  if [[ -L "$repo_p/findings" ]] || [[ ! -f "$repo_p/findings" ]]; then
    echo 'note: skipping the materialised symlinked-directory case (this git left the link a link)' >&2
  else
    both_apis 'a symlinked findings directory materialised as a file holds none either' \
      "$repo_p" "$p_base" "$p_head" 'fix-P2/correctness_no-ledger-entry' 1
  fi

  # AND THE OTHER WAY ROUND IS THE LEDGER'S DIRECTORY STILL. Where git records a
  # DIRECTORY at the listing path and the checkout holds a link in its place, the
  # findings are the entries the INDEX records under that path: what the link
  # points at is recorded somewhere else, and reading it would answer out of
  # somebody else's files. This used to refuse at exit 1 while the tree listings
  # resolved the name at exit 0, and that was the one disagreement between the
  # two APIs this gate kept on purpose. It is gone: the records answer both ways
  # in, so the commit gets one verdict and the mangled checkout changes nothing.
  # Both names are asserted, because "the records decide" has to mean the ledger
  # RESOLVES and the link's own contents do NOT.
  mkdir -p "$repo_q/elsewhere"
  echo fixture > "$repo_q/elsewhere/P3_liveness_202609100009_not-in-the-ledger.md"
  rm -rf "$repo_q/findings"
  ln -s ../elsewhere "$repo_q/findings"
  if [[ "$(git -C "$repo_q" ls-files -- findings/ | wc -l)" != 1 ]]; then
    echo 'the fixture was meant to leave the findings directory in the index' >&2
    exit 1
  fi
  both_apis 'a findings directory the checkout replaced with a link is the index directory still' \
    "$repo_q" "$q_base" "$q_head" 'fix-P2/correctness_no-ledger-entry' 0
  both_apis 'and the name at the end of that link is in no ledger' \
    "$repo_q" "$q_base" "$q_head" 'fix-P3/liveness_not-in-the-ledger' 1
  # Every spelling of it, because appending `/.` used to move the question.
  spelling_case 'a replaced findings directory, every spelling' \
    'fix-P2/correctness_no-ledger-entry' 0 "$repo_q/findings"
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
mkdir -p "$recorded_repo/findings"
echo one > "$recorded_repo/findings/P2_correctness_202609100001_shared-name.md"
echo two > "$recorded_repo/findings/P2_correctness_202609100002_shared-name.md"
git -C "$recorded_repo" add -A \
  && git -C "$recorded_repo" commit -q -m 'two findings share a description'
rm "$recorded_repo/findings/P2_correctness_202609100002_shared-name.md"
recorded_rc=0
recorded_out="$("$BASH" "$branch_validator" 'fix-P2/correctness_shared-name' \
  "$recorded_repo/findings" 2>&1)" || recorded_rc=$?
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
mkdir -p "$phrase_repo/findings"
echo one > "$phrase_repo/findings/P2_correctness_202609100001_shared-name.md"
echo two > "$phrase_repo/findings/P2_correctness_202609100002_shared-name.md"
git -C "$phrase_repo" add -A \
  && git -C "$phrase_repo" commit -q -m 'two findings share a description'
if ! git -C "$phrase_repo" worktree add -q --detach "$phrase_wt" HEAD 2>/dev/null; then
  echo 'note: skipping the path-shaped-like-a-message case (this git will not add a worktree)' >&2
elif [[ "$(id -u)" -eq 0 ]] || ! chmod 000 "$phrase_repo/.git/config" 2>/dev/null \
  || git -C "$phrase_wt/findings" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  chmod 600 "$phrase_repo/.git/config" 2>/dev/null || true
  echo 'note: skipping the path-shaped-like-a-message case (running as root, or chmod had no effect)' >&2
else
  # The linked worktree keeps its own index, and one twin is removed from its
  # CHECKOUT alone: the filesystem fallback -- which cannot see an index at all
  # -- answers `conforms` here rather than merely answering for another reason.
  rm "$phrase_wt/findings/P2_correctness_202609100002_shared-name.md"
  # git's message must really carry the path, or the case is about nothing.
  # Captured and then matched: git exits 128 here, and under `pipefail` a
  # pipeline out of it fails whatever grep found.
  phrase_probe="$(git -C "$phrase_wt/findings" rev-parse --is-inside-work-tree 2>&1 || true)"
  if ! grep -qF 'not a git repository - fixture' <<< "$phrase_probe"; then
    echo 'the fixture was meant to put the repository PATH into git-s diagnostic' >&2
    exit 1
  fi
  phrase_rc=0
  phrase_out="$("$BASH" "$branch_validator" 'fix-P2/correctness_shared-name' \
    "$phrase_wt/findings" 2>&1)" || phrase_rc=$?
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
  phrase_ok_out="$("$BASH" "$branch_validator" 'fix-P2/correctness_shared-name' \
    "$phrase_wt/findings" 2>&1)" || phrase_ok_rc=$?
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
mkdir -p "$unexaminable_repo/findings"
echo one > "$unexaminable_repo/findings/P2_correctness_202609100001_shared-name.md"
echo two > "$unexaminable_repo/findings/P2_correctness_202609100002_shared-name.md"
git -C "$unexaminable_repo" add -A \
  && git -C "$unexaminable_repo" commit -q -m 'two findings share a description'
unexaminable_twin='findings/P2_correctness_202609100002_shared-name.md'

# unexaminable_verdict <checkout> -> the exit code, with stderr on stdout
unexaminable_verdict() {
  "$BASH" "$branch_validator" 'fix-P2/correctness_shared-name' \
    "$1/findings" 2>&1
}

if ! git -C "$unexaminable_repo" worktree add -q --detach "$unexaminable_wt" HEAD 2>/dev/null \
  || [[ ! -f "$unexaminable_wt/.git" ]]; then
  echo 'note: skipping the unexaminable-.git cases (this git made no linked worktree)' >&2
elif [[ "$(id -u)" -eq 0 ]] || ! chmod 000 "$unexaminable_wt/.git" 2>/dev/null \
  || git -C "$unexaminable_wt/findings" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
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
    && ! git -C "$unexaminable_repo/findings" rev-parse --is-inside-work-tree >/dev/null 2>&1
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
if ! "$BASH" "$branch_validator" 'fix-P3/liveness_a-real-finding' \
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
mkdir -p "$repo_n/findings"
echo one > "$repo_n/findings/P2_correctness_202609100001_shared-name.md"
echo two > "$repo_n/findings/P2_correctness_202609100002_shared-name.md"
echo three > "$repo_n/findings/P3_liveness_202609100003_left-out-of-the-checkout.md"
echo four > "$repo_n/findings/P3_liveness_202609100004_a-real-finding.md"
git -C "$repo_n" add -A && git -C "$repo_n" commit -q -m 'four findings, two sharing a description'
n_head="$(git -C "$repo_n" rev-parse HEAD)"
both_apis 'the control: a full checkout' \
  "$repo_n" "$n_base" "$n_head" 'fix-P2/correctness_shared-name' 1
n_twin='findings/P2_correctness_202609100002_shared-name.md'
n_unique='findings/P3_liveness_202609100003_left-out-of-the-checkout.md'
if git -C "$repo_n" sparse-checkout set --no-cone '/*' "!/$n_twin" "!/$n_unique" >/dev/null 2>&1 \
  && [[ ! -e "$repo_n/$n_twin" && ! -e "$repo_n/$n_unique" ]] \
  && [[ -z "$(git -C "$repo_n" status --porcelain)" ]]; then
  # The exclusion is the CHECKOUT's and not the index's, which is what makes
  # this a narrowed listing rather than a ledger that lost two findings.
  if [[ "$(git -C "$repo_n" ls-files -- findings/ | wc -l)" != 4 ]]; then
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
  mkdir -p "$repo_l/findings"
  echo one > "$repo_l/findings/P2_correctness_202609100001_shared-name.md"
  echo two > "$repo_l/findings/P2_correctness_202609100002_shared-name.md"
  echo noise > "$repo_l/findings/noise"
  # The tail of this name is the second twin's name exactly, so a split hands
  # the set a name that IS a finding and takes the finding itself away.
  echo split > "$repo_l/findings/$nl_twin"
  # And the tail of this one is a finding NOTHING has filed, so a split invents
  # a finding, or refuses for a name that is not in the directory at all.
  echo invented > "$repo_l/findings/$nl_ghost"
  echo real > "$repo_l/findings/P3_liveness_202609101300_a-real-finding.md"
  git -C "$repo_l" add -A && git -C "$repo_l" commit -q -m 'a name with a newline in it'
  l_head="$(git -C "$repo_l" rev-parse HEAD)"
  if [[ ! -f "$repo_l/findings/$nl_twin" ]] \
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
    mkdir -p "$repo_m/findings"
    echo one > "$repo_m/findings/P2_correctness_202609100001_shared-name.md"
    echo two > "$repo_m/findings/P2_correctness_202609100002_shared-name.md"
    ln -s ../../seed.txt "$repo_m/findings/$nl_twin"
    git -C "$repo_m" add -A && git -C "$repo_m" commit -q -m 'a symlink whose name holds a newline'
    m_head="$(git -C "$repo_m" rev-parse HEAD)"
    if ! git -C "$repo_m" ls-tree "$m_head" findings/ | grep -q '^120000 blob '; then
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
  stream_out="$("$BASH" "$branch_validator" 'fix-P2/correctness_shared-name' \
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
mkdir -p "$inside_repo/findings"
echo one > "$inside_repo/findings/P2_correctness_202609100001_shared-name.md"
echo two > "$inside_repo/findings/P2_correctness_202609100002_shared-name.md"
git -C "$inside_repo" add -A \
  && git -C "$inside_repo" commit -q -m 'two findings share a description'
# The twin leaves the CHECKOUT and stays in the index, so the filesystem
# fallback answers `conforms` here rather than merely answering for some other
# reason.
rm "$inside_repo/findings/P2_correctness_202609100002_shared-name.md"
inside_verdict() {  # inside_verdict -> the exit code, with stderr on stdout
  "$BASH" "$branch_validator" 'fix-P2/correctness_shared-name' \
    "$inside_repo/findings" 2>&1
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
      || git -C "$inside_repo/findings" rev-parse --is-inside-work-tree >/dev/null 2>&1
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
  mkdir -p "$pointed_repo/findings"
  echo one > "$pointed_repo/findings/P2_correctness_202609100001_shared-name.md"
  echo two > "$pointed_repo/findings/P2_correctness_202609100002_shared-name.md"
  git -C "$pointed_repo" add -A \
    && git -C "$pointed_repo" commit -q -m 'two findings share a description'
  pointed_verdict() {
    "$BASH" "$branch_validator" 'fix-P2/correctness_shared-name' \
      "$pointed_wt/findings" 2>&1
  }
  if ! git -C "$pointed_repo" worktree add -q --detach "$pointed_wt" HEAD 2>/dev/null \
    || [[ ! -f "$pointed_wt/.git" ]]; then
    echo 'note: skipping the pointed-gitdir cases (this git made no linked worktree)' >&2
  else
    # The twin leaves this worktree's CHECKOUT and stays in its index, so the
    # filesystem fallback answers `conforms` rather than answering for some other
    # reason.
    rm "$pointed_wt/findings/P2_correctness_202609100002_shared-name.md"
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
          || git -C "$pointed_wt/findings" rev-parse --is-inside-work-tree >/dev/null 2>&1
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
  # worse than a narrowed set. With findings a COMMITTED SYMLINK whose
  # target text is a finding's filename -- which core.symlinks=false
  # materialises as a REGULAR FILE holding that name -- the recorded mode went
  # unread, the file was read as a FILE LISTING, and the link's target resolved
  # a fix-P*/ branch: a finding NOBODY HAS FILED, invented out of a read
  # failure, at exit 0.
  invented_repo="$fixture_dir/repo-invented-by-a-read-failure"
  invented_name='P2_correctness_202609100001_invented-by-a-read-failure.md'
  new_repo "$invented_repo"
  mkdir -p "$invented_repo/reviews"
  if [[ -L "$symlink_probe" ]] && ln -s "$invented_name" "$invented_repo/findings"; then
    git -C "$invented_repo" add -A \
      && git -C "$invented_repo" commit -q -m 'a findings directory that is a symlink naming a finding'
    git -C "$invented_repo" config core.symlinks false
    rm "$invented_repo/findings"
    git -C "$invented_repo" checkout -- findings
    invented_verdict() {
      "$BASH" "$branch_validator" 'fix-P2/correctness_invented-by-a-read-failure' \
        "$invented_repo/findings" 2>&1
    }
    if [[ -L "$invented_repo/findings" ]] || [[ ! -f "$invented_repo/findings" ]]; then
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
# `findings/...`: the tree listings hold no finding and refuse at exit 1.
# Handing `findings` straight in asked the index from INSIDE the link --
# `git -C reviews` chdirs to `elsewhere` -- and resolved the name out of files no
# ledger holds at that path, at exit 0.
#
# AND THE SAME LISTING WRITTEN FIVE WAYS IS ONE LISTING. `findings/.`
# moved the question from `findings` to `.`, so the committed symlink AT
# `findings` that the plain spelling refused at exit 1 conformed at exit 0
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
    'fix-P2/correctness_no-ledger-entry' 1 "$repo_r/findings"

  # The same name through the REAL directory the link points at resolves, so the
  # case above is a rule about the path and not a listing read as empty.
  if ! "$BASH" "$branch_validator" 'fix-P2/correctness_no-ledger-entry' \
    "$repo_r/elsewhere/findings" >/dev/null 2>&1; then
    echo 'the directory the link points at must still resolve the name it holds' >&2
    exit 1
  fi

  # THE SAME COMMIT CHECKED OUT WHERE THE FILESYSTEM CARRIES NO SYMLINK. Under
  # core.symlinks=false the 120000 blob at `reviews` is materialised as a REGULAR
  # FILE holding `elsewhere`, so `findings` is not there at all: both APIs
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
      'fix-P2/correctness_no-ledger-entry' 1 "$repo_r/findings"
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
    if ! "$BASH" "$branch_validator" 'fix-P3/liveness_under-a-linked-parent' \
      "$outer_link/repo-under-a-linked-parent/findings" >/dev/null 2>&1; then
      echo 'a symlink ABOVE the work tree must not refuse a listing inside it' >&2
      exit 1
    fi
  else
    echo 'note: skipping the link-above-the-work-tree case (no symlink could be made)' >&2
  fi

  # A SYMLINK TO THE WORK TREE'S OWN ROOT IS THE SAME RULE AT THE INODE. `-ef`
  # FOLLOWS a link, so with `reviews` committed as a link to `.` the prefix
  # `<repo>/reviews` IS the work tree root by inode: taking the deepest such
  # match named the listing `findings`, answered it out of the root's own
  # `findings/`, and never consulted the 120000 the index records for `reviews`.
  # The checkout is clean, the tree listings hold nothing under
  # `findings` and refuse at exit 1, and the directory conformed at exit
  # 0 -- the last counter-example to the equivalence on a clean checkout. The
  # root is matched by inode; the path through it is matched by RECORDED MODE,
  # and this is the half that asserts the second.
  root_loop_repo="$fixture_dir/repo-symlink-to-the-root"
  new_repo "$root_loop_repo"
  echo seed > "$root_loop_repo/seed.txt"
  git -C "$root_loop_repo" add -A && git -C "$root_loop_repo" commit -q -m base
  root_loop_base="$(git -C "$root_loop_repo" rev-parse HEAD)"
  mkdir -p "$root_loop_repo/findings"
  echo fixture > "$root_loop_repo/findings/P2_correctness_202609110001_root-loop.md"
  ln -s . "$root_loop_repo/reviews"
  git -C "$root_loop_repo" add -A \
    && git -C "$root_loop_repo" commit -q -m 'reviews is a symlink to the work tree root'
  root_loop_head="$(git -C "$root_loop_repo" rev-parse HEAD)"
  if ! git -C "$root_loop_repo" ls-tree "$root_loop_head" | grep -q '^120000 blob .*reviews$' \
    || [[ -n "$(git -C "$root_loop_repo" status --porcelain)" ]]; then
    echo 'the fixture was meant to COMMIT reviews as a link to the root, cleanly' >&2
    exit 1
  fi
  both_apis 'a symlink to the work tree root names no findings directory' \
    "$root_loop_repo" "$root_loop_base" "$root_loop_head" 'fix-P2/correctness_root-loop' 1
  spelling_case 'a symlink to the work tree root, every spelling' \
    'fix-P2/correctness_root-loop' 1 "$root_loop_repo/findings"
  # And the real directory the link comes back to still answers for its OWN
  # name, so the case above is a rule about the path and not a listing read as
  # empty.
  if ! "$BASH" "$branch_validator" 'fix-P2/correctness_root-loop' \
    "$root_loop_repo/findings" >/dev/null 2>&1; then
    echo 'the directory the link comes back to must still resolve the finding it holds' >&2
    exit 1
  fi

  # AND THE SAME THING AT THE LAST COMPONENT. `reviews` is a tracked directory
  # and `findings` a committed link to `..`, so the LISTING PATH ITSELF
  # is the work tree root by inode while the index records it as a 120000 blob.
  # Found by sweeping shapes for more counter-examples rather than by review, and
  # it was one: trees 1, directory 0 on the unrepaired file.
  last_loop_repo="$fixture_dir/repo-listing-loops-to-the-root"
  new_repo "$last_loop_repo"
  echo seed > "$last_loop_repo/seed.txt"
  git -C "$last_loop_repo" add -A && git -C "$last_loop_repo" commit -q -m base
  last_loop_base="$(git -C "$last_loop_repo" rev-parse HEAD)"
  mkdir -p "$last_loop_repo/reviews"
  echo fixture > "$last_loop_repo/P2_correctness_202609110002_loops-to-the-root.md"
  echo keep > "$last_loop_repo/reviews/keep.txt"
  ln -s .. "$last_loop_repo/findings"
  git -C "$last_loop_repo" add -A \
    && git -C "$last_loop_repo" commit -q -m 'findings is a link to the work tree root'
  last_loop_head="$(git -C "$last_loop_repo" rev-parse HEAD)"
  if [[ -n "$(git -C "$last_loop_repo" status --porcelain)" ]]; then
    echo 'the loop-to-the-root fixture was meant to be a clean checkout' >&2
    exit 1
  fi
  both_apis 'a listing path that loops back to the work tree root holds no finding' \
    "$last_loop_repo" "$last_loop_base" "$last_loop_head" \
    'fix-P2/correctness_loops-to-the-root' 1
  spelling_case 'a listing that loops to the root, every spelling' \
    'fix-P2/correctness_loops-to-the-root' 1 "$last_loop_repo/findings"
fi

# And a real findings directory answers the same in every spelling, which is what
# makes the refusals above about the path and not about the extra characters.
spelling_repo="$fixture_dir/repo-spellings"
new_repo "$spelling_repo"
commit_finding "$spelling_repo" 'P3_liveness_202609100005_written-five-ways.md' 'a real finding'
spelling_case 'a real findings directory, every spelling' \
  'fix-P3/liveness_written-five-ways' 0 "$spelling_repo/findings"
spelling_case 'and a name it does not hold, every spelling' \
  'fix-P3/liveness_not-in-this-directory' 1 "$spelling_repo/findings"

# A `..` AFTER A NAMED COMPONENT IS REFUSED AND NOT GUESSED AT. `a/b/..` is `a`
# when `b` is a directory and the LINK'S parent when `b` is a symlink, so
# reducing it lexically answers about a path the caller did not name and
# resolving it on the filesystem follows the link this rule exists to refuse. A
# LEADING run of `..` is a starting directory, above every component this judges,
# and is accepted.
dotdot_rc=0
dotdot_out="$("$BASH" "$branch_validator" 'fix-P3/liveness_written-five-ways' \
  "$spelling_repo/findings/../findings" 2>&1)" || dotdot_rc=$?
if [[ "$dotdot_rc" != 1 ]] || ! grep -q "holds a '\.\.' after a named component" <<< "$dotdot_out"; then
  echo "a '..' after a named component must be refused, saying so; got $dotdot_rc" >&2
  exit 1
fi
if ! ( cd "$spelling_repo/reviews" && "$BASH" "$branch_validator" \
  'fix-P3/liveness_written-five-ways' \
  '../findings' >/dev/null 2>&1 ); then
  echo 'a LEADING .. is a starting directory and must still resolve' >&2
  exit 1
fi

# ---- the records answer, and the checkout is not consulted where they do -----------------------
#
# The directory input used to answer from the filesystem with git as a
# cross-check, and five rounds each found another spelling of the same
# disagreement. It now LOCATES the repository and the path within it and then
# answers from `git ls-files -s` alone. These are the shapes that closed it, each
# with the control beside it, and each was executed against the parent revision
# first.

# A CHECKOUT DIRECTORY RENAMED AND REPLACED BY A LINK IS STILL THE INDEX'S
# DIRECTORY. `mv reviews saved-reviews; ln -s saved-reviews reviews` leaves the
# index recording `findings/<twin>.md` and leaves `git ls-tree` holding
# it too -- and the ancestor rule that used to see the link returned the EMPTY
# SET before looking at what the index records, so the twin vanished and an
# ambiguous name conformed at exit 0 where the parent refuses at exit 1. The
# names come from the records now, so the rename changes nothing.
if [[ -L "$symlink_probe" ]]; then
  renamed_repo="$fixture_dir/repo-renamed-reviews"
  new_repo "$renamed_repo"
  echo seed > "$renamed_repo/seed.txt"
  git -C "$renamed_repo" add -A && git -C "$renamed_repo" commit -q -m base
  mkdir -p "$renamed_repo/findings"
  echo two > "$renamed_repo/findings/P2_correctness_202609100002_shared-name.md"
  git -C "$renamed_repo" add -A && git -C "$renamed_repo" commit -q -m 'one twin, committed'
  printf 'P2_correctness_202609100001_shared-name.md\n' > "$fixture_dir/renamed-twin-a.txt"
  renamed_verdict() {
    "$BASH" "$branch_validator" 'fix-P2/correctness_shared-name' \
      "$fixture_dir/renamed-twin-a.txt" "$renamed_repo/findings" 2>&1
  }
  renamed_control_rc=0
  renamed_control_out="$(renamed_verdict)" || renamed_control_rc=$?
  if [[ "$renamed_control_rc" != 1 ]] || ! grep -q 'names 2 findings' <<< "$renamed_control_out"; then
    echo "the control was meant to refuse an ambiguous name; got $renamed_control_rc" >&2
    exit 1
  fi
  mv "$renamed_repo/reviews" "$renamed_repo/saved-reviews"
  ln -s saved-reviews "$renamed_repo/reviews"
  # The point of the fixture is that the INDEX did not move.
  if [[ "$(git -C "$renamed_repo" ls-files -- findings/ | wc -l)" != 1 ]]; then
    echo 'the fixture was meant to leave the index recording the finding' >&2
    exit 1
  fi
  renamed_rc=0
  renamed_out="$(renamed_verdict)" || renamed_rc=$?
  if [[ "$renamed_rc" != 1 ]] || ! grep -q 'names 2 findings' <<< "$renamed_out"; then
    echo "a checkout rename must not drop a finding the index records; got $renamed_rc" >&2
    printf '%s\n' "$renamed_out" >&2
    exit 1
  fi

  # A MATERIALISED LINK BEHIND ANOTHER LINK INVENTED A FINDING ON A CLEAN
  # CHECKOUT. `reviews -> elsewhere` and `elsewhere/findings -> <finding>.md`
  # are both committed; under core.symlinks=false the second is materialised as
  # a REGULAR FILE holding that filename, `git status` stays empty, and
  # `findings` was read as a FILE LISTING naming a finding nobody has
  # filed -- exit 0 on a commit whose trees refuse at exit 1. No index entry and
  # no tree entry is named `findings` at all: `reviews` is a 120000
  # blob, so the path is unnameable and holds nothing, whatever the checkout put
  # at the end of it.
  behind_repo="$fixture_dir/repo-materialised-behind-a-link"
  new_repo "$behind_repo"
  echo seed > "$behind_repo/seed.txt"
  git -C "$behind_repo" add -A && git -C "$behind_repo" commit -q -m base
  behind_base="$(git -C "$behind_repo" rev-parse HEAD)"
  mkdir -p "$behind_repo/elsewhere"
  ln -s elsewhere "$behind_repo/reviews"
  ln -s P2_correctness_202609100001_shared-name.md "$behind_repo/elsewhere/findings"
  git -C "$behind_repo" add -A \
    && git -C "$behind_repo" commit -q -m 'a findings path that is a link behind a link'
  behind_head="$(git -C "$behind_repo" rev-parse HEAD)"
  git -C "$behind_repo" config core.symlinks false
  rm "$behind_repo/elsewhere/findings"
  git -C "$behind_repo" checkout -- elsewhere/findings
  if [[ -L "$behind_repo/elsewhere/findings" ]] || [[ ! -f "$behind_repo/elsewhere/findings" ]] \
    || [[ -n "$(git -C "$behind_repo" status --porcelain)" ]]; then
    echo 'note: skipping the link-behind-a-link case (this git left the link a link)' >&2
  else
    both_apis 'a materialised link behind a link invents no finding' \
      "$behind_repo" "$behind_base" "$behind_head" 'fix-P2/correctness_shared-name' 1
    spelling_case 'a materialised link behind a link, every spelling' \
      'fix-P2/correctness_shared-name' 1 "$behind_repo/findings"
    # And the path the link itself names is a 120000 blob, which is not a
    # listing either: the two ways of asking are one answer.
    behind_direct_rc=0
    behind_direct_out="$("$BASH" "$branch_validator" 'fix-P2/correctness_shared-name' \
      "$behind_repo/elsewhere/findings" 2>&1)" || behind_direct_rc=$?
    if [[ "$behind_direct_rc" != 1 ]] || ! grep -q 'as mode 120000' <<< "$behind_direct_out"; then
      echo "the link's own path must be refused on its recorded mode; got $behind_direct_rc" >&2
      exit 1
    fi
  fi
fi

# A `.git` FILE THAT READS PERFECTLY AND NAMES A GITDIR NOBODY MAY STAT. The
# linked worktree's `.git` is readable, its `gitdir:` line is readable, and with
# the MAIN repository's `.git/worktrees` unsearchable `[[ -d <target> ]]` is
# FALSE -- not because the target is absent but because nothing may stat through
# the directory above it. Both git probes exit 128, the level was called empty,
# the walk concluded "no repository", and the twin the index records and the
# checkout lacks was gone: exit 0 `conforms` with empty stderr on the checkout
# that is exit 1 `names 2 findings` when the directory is searchable. Git
# resolves a healthy `.git` file, so a refusal on one that names a gitdir is a
# repository this cannot read and never an absence. Root can read anything, so
# the permission cases only mean something as an ordinary user.
worktrees_repo="$fixture_dir/repo-unsearchable-worktrees"
worktrees_wt="$fixture_dir/unsearchable-worktrees-checkout"
new_repo "$worktrees_repo"
echo seed > "$worktrees_repo/seed.txt"
git -C "$worktrees_repo" add -A && git -C "$worktrees_repo" commit -q -m base
mkdir -p "$worktrees_repo/findings"
echo one > "$worktrees_repo/findings/P2_correctness_202609100001_shared-name.md"
echo two > "$worktrees_repo/findings/P2_correctness_202609100002_shared-name.md"
git -C "$worktrees_repo" add -A \
  && git -C "$worktrees_repo" commit -q -m 'two findings share a description'
worktrees_verdict() {
  "$BASH" "$branch_validator" 'fix-P2/correctness_shared-name' \
    "$worktrees_wt/findings" 2>&1
}
if ! git -C "$worktrees_repo" worktree add -q --detach "$worktrees_wt" HEAD 2>/dev/null \
  || [[ ! -d "$worktrees_repo/.git/worktrees" ]]; then
  echo 'note: skipping the unsearchable-worktrees case (this git made no linked worktree)' >&2
else
  # The twin leaves the linked worktree's CHECKOUT and stays in its index, so the
  # filesystem fallback -- which cannot see an index at all -- answers `conforms`
  # here rather than merely answering for some other reason.
  rm "$worktrees_wt/findings/P2_correctness_202609100002_shared-name.md"
  worktrees_control_rc=0
  worktrees_control_out="$(worktrees_verdict)" || worktrees_control_rc=$?
  if [[ "$worktrees_control_rc" != 1 ]] \
    || ! grep -q 'names 2 findings' <<< "$worktrees_control_out"; then
    echo "the control was meant to refuse an ambiguous name; got $worktrees_control_rc" >&2
    exit 1
  fi

  # METADATA WITH A NUL IN IT IS UNEXAMINABLE TOO, AND THAT IS A STATUS AND NOT A
  # FLAG. A NUL appended after the newline of that same `.git` file makes both
  # discovery probes exit 128 -- the same 128 a directory that is no repository
  # gives. read_file reported the NUL correctly and repository_above never looked
  # at the report: the gitdir pointer read as empty, the walk found no repository
  # above, and the filesystem answered for a listing whose index still records
  # the twin. Exit 0 `conforms`, with empty stderr, on the checkout the control
  # refuses at exit 1. A flag one of three callers may skip is not a contract, so
  # the NUL is a status now and `if ! read_file` consumes it.
  cp -- "$worktrees_wt/.git" "$fixture_dir/worktrees-git-file"
  printf '\0' >> "$worktrees_wt/.git"
  worktrees_nul_rc=0
  worktrees_nul_out="$(worktrees_verdict)" || worktrees_nul_rc=$?
  cp -- "$fixture_dir/worktrees-git-file" "$worktrees_wt/.git"
  if [[ "$worktrees_nul_rc" == 0 ]]; then
    echo 'a NUL in a linked worktree .git file conformed: the filesystem fallback again' >&2
    printf '%s\n' "$worktrees_nul_out" >&2
    exit 1
  fi
  if ! grep -q 'cannot be examined' <<< "$worktrees_nul_out"; then
    echo 'metadata holding a NUL must refuse SAYING SO, not silently' >&2
    printf '%s\n' "$worktrees_nul_out" >&2
    exit 1
  fi
  worktrees_restored_rc=0
  worktrees_restored_out="$(worktrees_verdict)" || worktrees_restored_rc=$?
  if [[ "$worktrees_restored_rc" != 1 ]] \
    || ! grep -q 'names 2 findings' <<< "$worktrees_restored_out"; then
    echo "taking the NUL off must restore the verdict; got $worktrees_restored_rc" >&2
    exit 1
  fi
  if [[ "$(id -u)" -eq 0 ]] || ! chmod 000 "$worktrees_repo/.git/worktrees" 2>/dev/null \
    || git -C "$worktrees_wt/findings" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    chmod 755 "$worktrees_repo/.git/worktrees" 2>/dev/null || true
    echo 'note: skipping the unsearchable-worktrees case (running as root, or chmod had no effect)' >&2
  else
    # The `.git` FILE is readable throughout, which is what made this case
    # survive the repair that closed the unreadable one.
    if [[ ! -r "$worktrees_wt/.git" ]]; then
      chmod 755 "$worktrees_repo/.git/worktrees"
      echo 'note: skipping the unsearchable-worktrees case (the .git file went unreadable)' >&2
    else
      worktrees_rc=0
      worktrees_out="$(worktrees_verdict)" || worktrees_rc=$?
      chmod 755 "$worktrees_repo/.git/worktrees"
      if [[ "$worktrees_rc" == 0 ]]; then
        echo 'an unsearchable .git/worktrees conformed, which is the filesystem fallback again' >&2
        exit 1
      fi
      if ! grep -q 'cannot be examined' <<< "$worktrees_out"; then
        echo 'an unexaminable pointed gitdir must refuse SAYING SO, not silently' >&2
        printf '%s\n' "$worktrees_out" >&2
        exit 1
      fi
      worktrees_back_rc=0
      worktrees_back_out="$(worktrees_verdict)" || worktrees_back_rc=$?
      if [[ "$worktrees_back_rc" != 1 ]] \
        || ! grep -q 'names 2 findings' <<< "$worktrees_back_out"; then
        echo "restoring .git/worktrees must restore the verdict; got $worktrees_back_rc" >&2
        exit 1
      fi
    fi
  fi
fi

# ---- a private copy is not evidence that it was READ ------------------------------------------
#
# The helpers written to end the swallowed-status class swallowed one of their
# own. After the real `cat` had copied a listing -- exit 0, captured -- taking
# READ PERMISSION off the private copy left `read … < copy` failing to OPEN,
# reporting the same 1 it reports at end of input, and the helper returning
# SUCCESS WITH EMPTY BYTES: the twin was gone, and the ambiguous name conformed
# at exit 0 where the control refuses at exit 1. The same injection on
# `git_probe`'s `git.out`, after git returned 0, conformed on a materialised
# symlink. Owning a file establishes nothing about reading it.
#
# THE INJECTION IS AN EXPORTED SHELL FUNCTION, which is the reviewer's fault
# injection with nothing to install: it runs the real command, and then takes
# the permission off the private file the validator is about to read back. TMPDIR
# is moved so the hook can only see the run under test. Root can read anything,
# so this only means something as an ordinary user.
inject_dir="$fixture_dir/inject"
mkdir -p "$inject_dir"
printf 'P2_correctness_202609100001_shared-name.md\n' > "$fixture_dir/inject-twin-a.txt"
printf 'P2_correctness_202609100002_shared-name.md\n' > "$fixture_dir/inject-twin-b.txt"
inject_symlink_repo=''
if [[ -L "$symlink_probe" ]]; then
  inject_symlink_repo="$fixture_dir/repo-injected-record"
  new_repo "$inject_symlink_repo"
  mkdir -p "$inject_symlink_repo/reviews"
  ln -s P2_correctness_202609100001_invented-by-a-lost-record.md \
    "$inject_symlink_repo/findings"
  git -C "$inject_symlink_repo" add -A \
    && git -C "$inject_symlink_repo" commit -q -m 'a findings path that is a symlink naming a finding'
  git -C "$inject_symlink_repo" config core.symlinks false
  rm "$inject_symlink_repo/findings"
  git -C "$inject_symlink_repo" checkout -- findings
  if [[ -L "$inject_symlink_repo/findings" ]] \
    || [[ ! -f "$inject_symlink_repo/findings" ]]; then
    inject_symlink_repo=''
    echo 'note: skipping the injected-record case (this git left the link a link)' >&2
  fi
fi
inject_probe="$fixture_dir/inject-probe.sh"
cat > "$inject_probe" <<'INJECT'
# inject-probe.sh <hooked-command> <private-file> <trigger> <mode> <validator> <branch> <listing>...
# Runs the validator with <hooked-command> wrapped so that, ONCE THE PRIVATE FILE
# HOLDS <trigger>, its mode becomes <mode>. The trigger is what makes this the
# reviewer's injection and not a blunt one: the copy of the FIRST listing is left
# alone and the copy of the second is interfered with after the real command
# wrote it at exit 0.
#
# MODE 000 IS THE READ BACK AND MODE 400 IS THE WRITE. A copy that cannot be read
# back was round 10's P1 -- the helper returned success with empty bytes. A copy
# that cannot be OPENED FOR WRITING is round 12's: the redirection fails, the
# status assignment inside the group never runs, and THE PREVIOUS CAPTURE'S BYTES
# are still there with their sentinel on the end. One trigger fires at the first
# capture and the failure lands on the second.
hooked="$1"; private="$2"; trigger="$3"; mode="$4"; shift 4
eval "$hooked"'() {
  command '"$hooked"' "$@"
  local rc=$? d
  for d in "${TMPDIR:-/tmp}"/branch-name-policy.*; do
    if [[ -r "$d/'"$private"'" ]] \
      && command grep -q -- "'"$trigger"'" "$d/'"$private"'" 2>/dev/null; then
      chmod '"$mode"' "$d/'"$private"'" 2>/dev/null
    fi
  done
  return $rc
}'
export -f "$hooked"
rc=0
out="$("$BASH" "$@" 2>&1)" || rc=$?
printf '%s\n' "$rc"
printf '%s\n' "$out"
INJECT
# inject_output <hooked> <private-file> <trigger> <mode> <branch> <listing>...
# -> the exit code on the first line and the run's output below it.
inject_output() {
  local hooked="$1" private="$2" trigger="$3" mode="$4"
  shift 4
  TMPDIR="$inject_dir" BASH="$BASH" \
    "$BASH" "$inject_probe" "$hooked" "$private" "$trigger" "$mode" "$branch_validator" "$@" 2>&1
}

inject_case() {  # inject_case <hooked> <private-file> <trigger> <mode> <label> <branch> <listing>...
  local hooked="$1" private="$2" trigger="$3" mode="$4" label="$5" out rc
  shift 5
  out="$(inject_output "$hooked" "$private" "$trigger" "$mode" "$@")"
  rc="${out%%$'\n'*}"
  if [[ "$rc" == 0 ]]; then
    echo "$label: an interfered-with private copy conformed at exit 0" >&2
    printf '%s\n' "$out" >&2
    exit 1
  fi
}
if [[ "$(id -u)" -eq 0 ]]; then
  echo 'note: skipping the injected-read cases (running as root)' >&2
else
  # The controls first: both listings answer, and the answer is a refusal for
  # the RIGHT reason, so a refusal under injection is not the same refusal.
  inject_control_rc=0
  inject_control_out="$(TMPDIR="$inject_dir" \
    "$BASH" "$branch_validator" 'fix-P2/correctness_shared-name' \
    "$fixture_dir/inject-twin-a.txt" "$fixture_dir/inject-twin-b.txt" 2>&1)" || inject_control_rc=$?
  if [[ "$inject_control_rc" != 1 ]] || ! grep -q 'names 2 findings' <<< "$inject_control_out"; then
    echo "the injection control was meant to refuse an ambiguous name; got $inject_control_rc" >&2
    exit 1
  fi
  inject_case cat slurp 202609100002 000 'read_file' 'fix-P2/correctness_shared-name' \
    "$fixture_dir/inject-twin-a.txt" "$fixture_dir/inject-twin-b.txt"
  # AND THE SAME COPY UNWRITABLE, which is the other half and was a P1 of its
  # own: the first listing's bytes are left readable and the SECOND capture
  # cannot open its destination, so the group never runs, `copy_status` stays 0,
  # and the first listing's bytes pass the sentinel check a second time. Measured
  # against the unrepaired file: exit 0 `conforms`, over `slurp: Permission
  # denied`, where the same pair refuses at exit 1.
  inject_case cat slurp 202609100001 400 'read_file, destination unwritable' \
    'fix-P2/correctness_shared-name' \
    "$fixture_dir/inject-twin-a.txt" "$fixture_dir/inject-twin-b.txt"
  # git's own output, on the shape where losing it INVENTS a finding rather than
  # merely dropping one: findings is a committed symlink whose target
  # text is a finding's filename, materialised by core.symlinks=false as a
  # regular file holding that name. Read the recorded mode and it is a 120000
  # blob and no listing; lose it and the file's BYTES resolve a fix-P*/ branch.
  if [[ -n "$inject_symlink_repo" ]]; then
    inject_case git git.out 120000 000 'git_probe' \
      'fix-P2/correctness_invented-by-a-lost-record' \
      "$inject_symlink_repo/findings"
  fi
  # git's destination unwritable, on the shape where the LOST RECORD drops a
  # whole listing: a file listing naming one twin, and a repository directory
  # recording the other. The trigger is the work tree root `--show-toplevel`
  # writes, so the capture that fails is the `ls-files` after it -- which read
  # the toplevel back as its records, found no entry, and answered the empty set.
  # Measured against the unrepaired file: exit 0 `conforms` where the pair
  # refuses at exit 1.
  inject_toplevel_repo="$fixture_dir/repo-injected-toplevel"
  new_repo "$inject_toplevel_repo"
  commit_finding "$inject_toplevel_repo" 'P2_correctness_202609100002_shared-name.md' 'one twin'
  inject_toplevel_control_rc=0
  inject_toplevel_control_out="$(TMPDIR="$inject_dir" \
    "$BASH" "$branch_validator" 'fix-P2/correctness_shared-name' \
    "$fixture_dir/inject-twin-a.txt" "$inject_toplevel_repo/findings" 2>&1)" \
    || inject_toplevel_control_rc=$?
  if [[ "$inject_toplevel_control_rc" != 1 ]] \
    || ! grep -q 'names 2 findings' <<< "$inject_toplevel_control_out"; then
    echo "the git-destination control was meant to refuse; got $inject_toplevel_control_rc" >&2
    exit 1
  fi
  inject_case git git.out repo-injected-toplevel 400 'git_probe, destination unwritable' \
    'fix-P2/correctness_shared-name' \
    "$fixture_dir/inject-twin-a.txt" "$inject_toplevel_repo/findings"
fi

# ---- a directory's NAMES and its STATUS must come from one run of one command ------------------
#
# `ls` was run for its status and a GLOB then supplied the names, and the two are
# not one read: `chmod 000` on the directory AFTER the real `ls` exited 0 left
# `nullglob` expanding to nothing, list_dir returning success with no entries,
# one twin dropped and the ambiguous name conforming at exit 0 where the pair
# refuses at exit 1. A SUCCESSFUL PRODUCER IS NOT A SUCCESSFUL READ.
#
# The hook wraps BOTH enumerators -- the `ls` the defect used and the `find` that
# replaced it -- and the fixture asserts it FIRED, because a hook that matches
# nothing would pass this file forever while proving nothing.
enumerate_dir="$fixture_dir/enumerated-outside-a-repository"
mkdir -p "$enumerate_dir"
echo two > "$enumerate_dir/P2_correctness_202609100002_shared-name.md"
enumerate_probe="$fixture_dir/enumerate-probe.sh"
cat > "$enumerate_probe" <<'ENUMERATE'
# enumerate-probe.sh <directory> <validator> <branch> <listing>...
# Every enumerating command runs for real and the directory goes unreadable
# straight afterwards, which is the reviewer's injection: the command's own
# status says nothing about a second look at the same directory.
export ENUMERATE_TARGET="$1"; shift
export ENUMERATE_FIRED="$ENUMERATE_TARGET.fired"
: > "$ENUMERATE_FIRED"
_enumerated() {
  local rc=$?
  chmod 000 "$ENUMERATE_TARGET" 2>/dev/null
  printf 'x' >> "$ENUMERATE_FIRED"
  return $rc
}
ls()   { command ls "$@"; _enumerated; }
find() { command find "$@"; _enumerated; }
export -f _enumerated ls find
rc=0
out="$("$BASH" "$@" 2>&1)" || rc=$?
chmod 755 "$ENUMERATE_TARGET" 2>/dev/null
printf '%s\n' "$rc"
printf '%s\n' "$out"
ENUMERATE
if [[ "$(id -u)" -eq 0 ]]; then
  echo 'note: skipping the enumerated-directory cases (running as root)' >&2
else
  enumerate_control_rc=0
  enumerate_control_out="$("$BASH" "$branch_validator" 'fix-P2/correctness_shared-name' \
    "$fixture_dir/inject-twin-a.txt" "$enumerate_dir" 2>&1)" || enumerate_control_rc=$?
  if [[ "$enumerate_control_rc" != 1 ]] \
    || ! grep -q 'names 2 findings' <<< "$enumerate_control_out"; then
    echo "the enumeration control was meant to refuse; got $enumerate_control_rc" >&2
    exit 1
  fi
  enumerate_out="$(BASH="$BASH" \
    "$BASH" "$enumerate_probe" "$enumerate_dir" "$branch_validator" \
    'fix-P2/correctness_shared-name' "$fixture_dir/inject-twin-a.txt" "$enumerate_dir" 2>&1)"
  enumerate_rc="${enumerate_out%%$'\n'*}"
  read -r enumerate_fired < <(wc -c < "$enumerate_dir.fired")
  if (( enumerate_fired == 0 )); then
    echo 'the enumeration hook never fired, so this case proves nothing' >&2
    exit 1
  fi
  if [[ "$enumerate_rc" == 0 ]]; then
    echo 'a directory made unreadable after its enumerator ran conformed at exit 0' >&2
    printf '%s\n' "$enumerate_out" >&2
    exit 1
  fi
  # AND THE LISTING'S OWN PRIVATE COPY IS READ BACK THROUGH THE SENTINEL, which
  # is what makes list_dir a caller of the one capture rather than a fourth way
  # in: the copy goes unreadable after the enumerator wrote it at exit 0, and the
  # refusal has to be the LISTING that could not be read rather than the verdict
  # the candidate set would otherwise have given.
  enumerate_copy_out="$(inject_output find dir.out 202609100002 000 \
    'fix-P2/correctness_shared-name' "$fixture_dir/inject-twin-a.txt" "$enumerate_dir")"
  enumerate_copy_rc="${enumerate_copy_out%%$'\n'*}"
  if [[ "$enumerate_copy_rc" == 0 ]] \
    || ! grep -q 'could not be listed' <<< "$enumerate_copy_out"; then
    echo "a listing whose private copy went unreadable must refuse saying so; got $enumerate_copy_rc" >&2
    printf '%s\n' "$enumerate_copy_out" >&2
    exit 1
  fi
fi

# ---- the cost of answering from the records, asserted rather than described -------------------
#
# An UNTRACKED finding file inside a tracked findings/ no longer counts:
# the ledger is what is committed, and a merge gate decides about commits and
# never about a work tree. That is a deliberate loosening and this is where it
# is pinned, because a silent return to counting it would part the two APIs
# again -- the other way round, which is how it used to be: an untracked twin
# beside a committed finding was exit 1 `names 2 findings` through the directory
# and exit 0 `conforms` through the trees.
untracked_repo="$fixture_dir/repo-untracked-twin"
new_repo "$untracked_repo"
echo seed > "$untracked_repo/seed.txt"
git -C "$untracked_repo" add -A && git -C "$untracked_repo" commit -q -m base
untracked_base="$(git -C "$untracked_repo" rev-parse HEAD)"
commit_finding "$untracked_repo" 'P2_correctness_202609100001_shared-name.md' 'one filed finding'
untracked_head="$(git -C "$untracked_repo" rev-parse HEAD)"
both_apis 'the control: one filed finding resolves' \
  "$untracked_repo" "$untracked_base" "$untracked_head" 'fix-P2/correctness_shared-name' 0
echo twin > "$untracked_repo/findings/P2_correctness_202609100002_shared-name.md"
if [[ -z "$(git -C "$untracked_repo" status --porcelain)" ]]; then
  echo 'the fixture was meant to leave an UNTRACKED file in the findings directory' >&2
  exit 1
fi
both_apis 'and an untracked twin beside it changes nothing' \
  "$untracked_repo" "$untracked_base" "$untracked_head" 'fix-P2/correctness_shared-name' 0

# AND AN UNTRACKED FINDINGS DIRECTORY IS THE EMPTY SET, which is the same rule
# one level up and the one that would otherwise be the whole disagreement back
# again: nothing under findings/ is committed, a finding file sits there
# untracked, and the trees hold none. Both ends must say none.
bare_untracked="$fixture_dir/repo-untracked-findings-dir"
new_repo "$bare_untracked"
echo seed > "$bare_untracked/seed.txt"
git -C "$bare_untracked" add -A && git -C "$bare_untracked" commit -q -m base
bare_untracked_base="$(git -C "$bare_untracked" rev-parse HEAD)"
echo more > "$bare_untracked/other.txt"
git -C "$bare_untracked" add -A && git -C "$bare_untracked" commit -q -m 'nothing to do with findings'
bare_untracked_head="$(git -C "$bare_untracked" rev-parse HEAD)"
mkdir -p "$bare_untracked/findings"
echo fixture > "$bare_untracked/findings/P2_correctness_202609100011_never-committed.md"
both_apis 'an untracked findings directory holds no filed finding' \
  "$bare_untracked" "$bare_untracked_base" "$bare_untracked_head" \
  'fix-P2/correctness_never-committed' 1

# AND AN UNTRACKED SYMLINK STANDING IN FOR IT IS NOT A FINDINGS DIRECTORY. `-d`
# follows a link, so a listing path that git records nothing about and that
# points at a directory of finding files would resolve names out of somebody
# else's directory while the trees hold none.
if [[ -L "$symlink_probe" ]]; then
  link_untracked="$fixture_dir/repo-untracked-findings-link"
  new_repo "$link_untracked"
  echo seed > "$link_untracked/seed.txt"
  git -C "$link_untracked" add -A && git -C "$link_untracked" commit -q -m base
  link_untracked_base="$(git -C "$link_untracked" rev-parse HEAD)"
  echo more > "$link_untracked/other.txt"
  git -C "$link_untracked" add -A && git -C "$link_untracked" commit -q -m 'nothing to do with findings'
  link_untracked_head="$(git -C "$link_untracked" rev-parse HEAD)"
  mkdir -p "$link_untracked/elsewhere" "$link_untracked/reviews"
  echo fixture > "$link_untracked/elsewhere/P2_correctness_202609100012_at-the-end-of-a-link.md"
  ln -s ../elsewhere "$link_untracked/findings"
  both_apis 'an untracked symlink is not a findings directory' \
    "$link_untracked" "$link_untracked_base" "$link_untracked_head" \
    'fix-P2/correctness_at-the-end-of-a-link' 1
fi

# AND A TRACKED FILE LISTING THE CHECKOUT REPLACED WITH A LINK IS A REFUSAL. The
# bytes of whatever the checkout put there are not the file git records, and a
# link's target text read as a listing is how a finding nobody filed was invented
# at the other end of this rule.
if [[ -L "$symlink_probe" ]]; then
  tracked_listing_repo="$fixture_dir/repo-tracked-file-listing"
  new_repo "$tracked_listing_repo"
  printf 'P3_liveness_202609100013_named-by-a-tracked-listing.md\n' > "$tracked_listing_repo/listing.txt"
  git -C "$tracked_listing_repo" add -A \
    && git -C "$tracked_listing_repo" commit -q -m 'a tracked file listing'
  if ! "$BASH" "$branch_validator" 'fix-P3/liveness_named-by-a-tracked-listing' \
    "$tracked_listing_repo/listing.txt" >/dev/null 2>&1; then
    echo 'a tracked regular file must still be read as a file listing' >&2
    exit 1
  fi
  echo decoy > "$tracked_listing_repo/decoy.txt"
  rm "$tracked_listing_repo/listing.txt"
  ln -s decoy.txt "$tracked_listing_repo/listing.txt"
  tracked_listing_rc=0
  tracked_listing_out="$("$BASH" "$branch_validator" 'fix-P3/liveness_named-by-a-tracked-listing' \
    "$tracked_listing_repo/listing.txt" 2>&1)" || tracked_listing_rc=$?
  if [[ "$tracked_listing_rc" != 1 ]] \
    || ! grep -q 'checkout does not hold one there' <<< "$tracked_listing_out"; then
    echo "a tracked file listing replaced by a link must refuse; got $tracked_listing_rc" >&2
    exit 1
  fi
fi

# A RELATIVE LISTING IS RELATIVE TO WHERE THE SHELL IS STANDING, and the
# components a caller names are the ones they typed PLUS the ones `$PWD` holds.
# Run from a checkout's `reviews/`, the listing `findings` is `findings`
# in the index and nothing else -- and a prefix chain that starts at `.` reaches
# no root above it, which refused an ordinary by-hand invocation that every
# earlier head accepted. Three spellings of one directory, from three different
# working directories, and each must resolve the finding that is there.
relative_repo="$fixture_dir/repo-relative-listing"
new_repo "$relative_repo"
commit_finding "$relative_repo" 'P3_liveness_202609100015_named-relatively.md' 'a real finding'
relative_case() {  # relative_case <label> <cwd> <listing>
  if ! ( cd "$2" && "$BASH" "$branch_validator" \
    'fix-P3/liveness_named-relatively' "$3" >/dev/null 2>&1 ); then
    echo "a relative listing must resolve the finding it holds: $1" >&2
    exit 1
  fi
}
relative_case 'from the work tree root' "$relative_repo" 'findings'
relative_case 'from reviews/'           "$relative_repo/reviews" 'findings'
relative_case 'from the directory itself' "$relative_repo/findings" '.'
# And a relative path that is NOT the ledger's directory still holds nothing:
# the rule is about naming the path, not about accepting every relative one.
relative_miss_rc=0
( cd "$relative_repo/reviews" && "$BASH" "$branch_validator" \
  'fix-P3/liveness_not-in-this-directory' 'findings' >/dev/null 2>&1 ) \
  || relative_miss_rc=$?
if [[ "$relative_miss_rc" != 1 ]]; then
  echo "a relative listing must still refuse a name it does not hold; got $relative_miss_rc" >&2
  exit 1
fi

# AND THE LONG WAY ROUND IS THE SAME PATH. The work tree's root is matched
# against the caller's own components, and a spelling that passes the root on
# the way down and comes back to it -- `../../<repo>/findings` from
# inside `<repo>/reviews` -- must be named `findings` in the index and
# not `../<repo>/findings`, which is no index entry and which git refuses
# as a pathspec leaving the work tree.
long_way_repo="$fixture_dir/repo-long-way-round"
new_repo "$long_way_repo"
commit_finding "$long_way_repo" 'P3_liveness_202609100014_spelled-the-long-way.md' 'a real finding'
if ! ( cd "$long_way_repo/reviews" && "$BASH" "$branch_validator" \
  'fix-P3/liveness_spelled-the-long-way' \
  '../../repo-long-way-round/findings' >/dev/null 2>&1 ); then
  echo 'a path that passes the work tree root on the way down must still resolve' >&2
  exit 1
fi

# AND AN UNTRACKED FILE IS STILL A FILE LISTING. The three listings the workflow
# builds are files a caller wrote, and a caller may write them anywhere --
# $RUNNER_TEMP in the workflow, a scratch directory inside the checkout by hand.
# A file listing is not the ledger's directory and is read wherever it lives; it
# is only a path git records something at, under or ABOVE that is answered from
# the records.
untracked_listing="$untracked_repo/scratch/listing.txt"
mkdir -p "$untracked_repo/scratch"
printf 'P3_liveness_202609100003_written-into-the-checkout.md\n' > "$untracked_listing"
if ! "$BASH" "$branch_validator" 'fix-P3/liveness_written-into-the-checkout' \
  "$untracked_listing" >/dev/null 2>&1; then
  echo 'an untracked file listing inside a work tree must still be read as a listing' >&2
  exit 1
fi

# ---- the equivalence, as a property rather than a list of cases -------------------------------
#
# Three of the four P1s in this pull request's reviews were the two documented
# ways in disagreeing about one commit, so the claim is tested as a property: for
# every repository below and every branch name below, the answer through the
# three tree listings EQUALS the answer through the working tree's
# findings/ directory. The expectation is not asserted here at all --
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
mkdir -p "$eq_plain/findings"
echo one > "$eq_plain/findings/P2_correctness_202609100001_shared-name.md"
echo two > "$eq_plain/findings/P2_correctness_202609100002_shared-name.md"
echo three > "$eq_plain/findings/P3_liveness_202609100003_a-real-finding.md"
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
mkdir -p "$eq_recorded/findings"
echo one > "$eq_recorded/findings/P2_correctness_202609100001_shared-name.md"
echo two > "$eq_recorded/findings/P2_correctness_202609100002_shared-name.md"
git -C "$eq_recorded" add -A && git -C "$eq_recorded" commit -q -m 'two findings share a description'
register_equivalence "$eq_recorded" "$eq_recorded_base" "$(git -C "$eq_recorded" rev-parse HEAD)"
rm "$eq_recorded/findings/P2_correctness_202609100002_shared-name.md"

# A subdirectory wearing a finding's name, which is a tree and not a finding.
eq_subdir="$fixture_dir/eq-directory-named-like-a-finding"
new_repo "$eq_subdir"
echo seed > "$eq_subdir/seed.txt"
git -C "$eq_subdir" add -A && git -C "$eq_subdir" commit -q -m base
eq_subdir_base="$(git -C "$eq_subdir" rev-parse HEAD)"
mkdir -p "$eq_subdir/findings/P2_correctness_202609100001_shared-name.md"
echo inside > "$eq_subdir/findings/P2_correctness_202609100001_shared-name.md/inside.txt"
echo real > "$eq_subdir/findings/P3_liveness_202609100003_a-real-finding.md"
git -C "$eq_subdir" add -A && git -C "$eq_subdir" commit -q -m 'a directory wearing a finding name'
register_equivalence "$eq_subdir" "$eq_subdir_base" "$(git -C "$eq_subdir" rev-parse HEAD)"

# The path the ledger records nothing about: an untracked findings directory,
# and a tracked one with an untracked file beside the filed finding. These are
# where the filesystem and the index part company without anybody doing anything
# exotic, and the property is that the two ways in do not.
register_equivalence "$bare_untracked" "$bare_untracked_base" "$bare_untracked_head"
register_equivalence "$untracked_repo" "$untracked_base" "$untracked_head"

if [[ -L "$symlink_probe" ]]; then
  # The findings directory itself a committed symlink, and an ancestor of it a
  # committed symlink: the two shapes an earlier round's P1s were. The link
  # behind a link, materialised, is this round's.
  register_equivalence "$repo_p" "$p_base" "$p_head"
  register_equivalence "$repo_r" "$r_base" "$r_head"
  register_equivalence "$repo_k" "$k_base" "$k_head"
  register_equivalence "$root_loop_repo" "$root_loop_base" "$root_loop_head"
  register_equivalence "$last_loop_repo" "$last_loop_base" "$last_loop_head"
  if [[ -n "${behind_base:-}" ]]; then
    register_equivalence "$behind_repo" "$behind_base" "$behind_head"
  fi
  if [[ -n "${link_untracked_base:-}" ]]; then
    register_equivalence "$link_untracked" "$link_untracked_base" "$link_untracked_head"
  fi
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
  'fix-P2/correctness_root-loop'
  'fix-P2/correctness_loops-to-the-root'
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
    "$BASH" "$branch_validator" "$eq_branch" "$eq_repo/findings" \
      >/dev/null 2>&1 || eq_dir=$?
    if [[ "$eq_tree" != "$eq_dir" ]]; then
      echo "the two APIs disagree: ${eq_repo##*/} / $eq_branch -> trees $eq_tree, directory $eq_dir" >&2
      exit 1
    fi
    equivalence_pairs=$(( equivalence_pairs + 1 ))
  done
done
if (( equivalence_pairs < 42 )); then
  echo "the equivalence property checked only $equivalence_pairs pairs, which is too few to mean anything" >&2
  exit 1
fi

# ---- what the pull request DID: the findings/ limit and the P0-P3 severity check ---------------
#
# Two rules about the DIFF rather than the name, and two listings they are judged from. They are
# built by .github/scripts/changed-in-range.sh -- the validator may run no command and open no file
# outside its audited helpers -- so these build REAL REPOSITORIES and call that script, for the
# reason the listing-construction fixtures above exist: a suite that feeds the validator listings it
# wrote itself proves what the validator does with a listing and nothing about whether the listing
# was right, and three wrong ways of building one each survived a frontier review.

changed_script="$root/.github/scripts/changed-in-range.sh"

# diff_verdict <dir> <target> <head> <branch>: build both listings from that repository and print the
# validator's exit code. 99 means the listings could not be built at all, so a construction failure
# can never read as a verdict.
diff_verdict() {
  local dir="$1" target="$2" head="$3" branch="$4" out rc=0
  out="$(mktemp -d "$fixture_dir/changed-XXXXXX")"
  if ! ( cd "$dir" && "$BASH" "$changed_script" "$target" "$head" "$out" ) >/dev/null 2>&1; then
    echo 99
    return 0
  fi
  "$BASH" "$branch_validator" "$branch" '' '' '' \
    "$out/changed-paths" "$out/added-findings" >/dev/null 2>&1 || rc=$?
  echo "$rc"
}

# diff_message <dir> <target> <head> <branch>: the same run's output, so a refusal can be checked for
# NAMING THE PATH it refused over. A refusal that does not say which path is a refusal somebody has
# to reproduce locally to act on.
diff_message() {
  local dir="$1" target="$2" head="$3" branch="$4" out
  out="$(mktemp -d "$fixture_dir/changed-XXXXXX")"
  if ! ( cd "$dir" && "$BASH" "$changed_script" "$target" "$head" "$out" ) >/dev/null 2>&1; then
    echo 'THE LISTINGS COULD NOT BE BUILT'
    return 0
  fi
  "$BASH" "$branch_validator" "$branch" '' '' '' \
    "$out/changed-paths" "$out/added-findings" 2>&1 || true
}

diff_case() {  # diff_case <label> <dir> <target> <head> <branch> <want-exit>
  local label="$1" got
  got="$(diff_verdict "$2" "$3" "$4" "$5")"
  if [[ "$got" != "$6" ]]; then
    echo "$label ($5): answered $got, and $6 was expected" >&2
    exit 1
  fi
}

diff_says() {  # diff_says <label> <dir> <target> <head> <branch> <substring>
  local label="$1" got
  got="$(diff_message "$2" "$3" "$4" "$5")"
  if [[ "$got" != *"$6"* ]]; then
    echo "$label ($5): the refusal did not name [$6]:" >&2
    printf '%s\n' "$got" >&2
    exit 1
  fi
}

finding_body() {  # finding_body <severity> -> a finding file's bytes, frontmatter first
  printf -- '---\nid: FIXTURE-1\nseverity: %s\ndisposition: deferred\n---\n\n## Failure sequence\n' "$1"
}

# A repository whose BASE ALREADY CARRIES A BADLY NAMED FILE under findings/. Every case
# below branches from it, so every one of them also asserts the rule that matters most here: a name
# already on master must never turn somebody else's pull request red. A rule over the directory as it
# stands would refuse every open pull request the day such a file landed -- including the pull
# request that was going to fix it.
repo_diff="$fixture_dir/repo-diff-rules"
new_repo "$repo_diff"
mkdir -p "$repo_diff/findings" "$repo_diff/src"
echo seed > "$repo_diff/seed.txt"
echo 'fn main() {}' > "$repo_diff/src/engine.rs"
# A source file with enough content for git to CALL a move of it a rename. `fn main() {}` moved
# under findings/ with frontmatter bolted on is too dissimilar to be detected as one, and
# case 10 is about what happens when detection fires.
awk 'BEGIN { for (i = 0; i < 200; i++) print "pub fn archived_" i "() { let _ = " i "; }" }' \
  > "$repo_diff/src/archive-me.rs"
finding_body P9 > "$repo_diff/findings/P9_correctness_202609010000_already-here.md"
finding_body P2 > "$repo_diff/findings/NOT-A-FINDING.md"
finding_body P2 > "$repo_diff/findings/P2_correctness_202609010001_to-be-renamed.md"
git -C "$repo_diff" add -A
git -C "$repo_diff" commit -q -m base
diff_base="$(git -C "$repo_diff" rev-parse HEAD)"

branch_from() {  # branch_from <name>: a branch off the base, checked out
  git -C "$repo_diff" checkout -q -B "$1" "$diff_base"
}

# 1. A findings/ pull request confined to findings/, with two badly named files sitting in
#    the directory it never touches.
branch_from confined
finding_body P3 > "$repo_diff/findings/P3_liveness_202609110000_a-new-one.md"
git -C "$repo_diff" add -A && git -C "$repo_diff" commit -q -m 'file a finding'
diff_confined="$(git -C "$repo_diff" rev-parse HEAD)"
diff_case 'a findings/ branch confined to the ledger' \
  "$repo_diff" "$diff_base" "$diff_confined" findings/file-a-new-one 0

# 2. The same pull request with one file outside the ledger. THE PATH IS NAMED.
branch_from outside
finding_body P3 > "$repo_diff/findings/P3_liveness_202609110000_a-new-one.md"
echo '// repaired' >> "$repo_diff/src/engine.rs"
git -C "$repo_diff" add -A && git -C "$repo_diff" commit -q -m 'file a finding and repair it'
diff_outside="$(git -C "$repo_diff" rev-parse HEAD)"
diff_case 'a findings/ branch that repairs code' \
  "$repo_diff" "$diff_base" "$diff_outside" findings/file-a-new-one 1
diff_says 'a findings/ branch that repairs code' \
  "$repo_diff" "$diff_base" "$diff_outside" findings/file-a-new-one 'src/engine.rs'
# and it is the PREFIX'S limit and not a rule about repairs: the same diff on a prefix that admits
# code is exactly what that prefix is for.
diff_case 'the same diff on a prefix that admits code' \
  "$repo_diff" "$diff_base" "$diff_outside" fix-P3/liveness_a-new-one 0
# A PATH THAT MERELY CONTAINS `findings/` IS NOT UNDER IT. The test is the start of the
# path and not a substring of it: matched anywhere, `src/findings/sneaky.rs` is inside the
# ledger and a findings/ branch may carry any code that sits under a directory of that name.
branch_from outside-lookalike
mkdir -p "$repo_diff/src/findings" "$repo_diff/findings-archive"
echo 'fn sneaky() {}' > "$repo_diff/src/findings/sneaky.rs"
echo archived > "$repo_diff/findings-archive/old.md"
git -C "$repo_diff" add -A && git -C "$repo_diff" commit -q -m 'paths that look like the ledger'
diff_lookalike="$(git -C "$repo_diff" rev-parse HEAD)"
diff_case 'a findings/ branch adding a path that only looks like the ledger' \
  "$repo_diff" "$diff_base" "$diff_lookalike" findings/tidy-up 1
diff_says 'a findings/ branch adding a path that only looks like the ledger' \
  "$repo_diff" "$diff_base" "$diff_lookalike" findings/tidy-up 'src/findings/sneaky.rs'
diff_says 'a findings/ branch adding a sibling of the ledger directory' \
  "$repo_diff" "$diff_base" "$diff_lookalike" findings/tidy-up 'findings-archive/old.md'

# A DELETION outside the ledger is a path this pull request touches too. `--name-only` lists it, and
# a limit that only saw additions would let a findings/ branch remove a gate.
branch_from outside-delete
git -C "$repo_diff" rm -q src/engine.rs
# AND A FILE UNDER THE LEDGER, so the listing is not empty. Without it the deletion is the whole of
# the diff, an implementation that dropped deletions would hand over an EMPTY listing, and the empty
# rule would refuse the pull request for a reason that has nothing to do with the deletion -- a
# fixture passing for the wrong reason, and one that would not notice deletions going missing.
finding_body P3 > "$repo_diff/findings/P3_liveness_202609110006_alongside.md"
git -C "$repo_diff" add -A
git -C "$repo_diff" commit -q -m 'delete a source file and file a finding'
diff_outside_delete="$(git -C "$repo_diff" rev-parse HEAD)"
diff_case 'a findings/ branch that deletes a source file' \
  "$repo_diff" "$diff_base" "$diff_outside_delete" findings/tidy-up 1

# 3. A finding whose NAME carries a severity the ladder does not have. Its frontmatter is P3, so the
#    refusal is about the name alone.
branch_from p4-name
finding_body P3 > "$repo_diff/findings/P4_correctness_202609110001_out-of-range.md"
git -C "$repo_diff" add -A && git -C "$repo_diff" commit -q -m 'file a P4'
diff_p4_name="$(git -C "$repo_diff" rev-parse HEAD)"
diff_case 'an added finding named P4_' \
  "$repo_diff" "$diff_base" "$diff_p4_name" findings/file-a-p4 1
diff_says 'an added finding named P4_' \
  "$repo_diff" "$diff_base" "$diff_p4_name" findings/file-a-p4 'P4_correctness_202609110001_out-of-range.md'
# EVERY PREFIX, not just findings/. A finding nothing can act on is the same defect on any branch.
diff_case 'an added finding named P4_ on a docs/ branch' \
  "$repo_diff" "$diff_base" "$diff_p4_name" docs/a-slug 1

# 4. A finding whose NAME is fine and whose FRONTMATTER severity is not.
branch_from p4-severity
finding_body P4 > "$repo_diff/findings/P3_correctness_202609110002_mismatched.md"
git -C "$repo_diff" add -A && git -C "$repo_diff" commit -q -m 'file a mismatched severity'
diff_p4_sev="$(git -C "$repo_diff" rev-parse HEAD)"
diff_case 'an added finding whose frontmatter severity is P4' \
  "$repo_diff" "$diff_base" "$diff_p4_sev" findings/file-a-mismatch 1
diff_says 'an added finding whose frontmatter severity is P4' \
  "$repo_diff" "$diff_base" "$diff_p4_sev" findings/file-a-mismatch 'its frontmatter severity is [P4]'
# A file under the ledger with no frontmatter at all is not a finding either, and says so as [-].
# THE BODY IS NOT THE FRONTMATTER, which is the same distinction scripts/pr-ready-audit.sh makes for
# the `id:` line and for the same reason: this file carries `severity: P2` in its prose, and a
# severity read from anywhere in the file would make it a P2 finding.
branch_from no-frontmatter
printf 'no frontmatter here\n\nseverity: P2\n' \
  > "$repo_diff/findings/P3_correctness_202609110003_bare.md"
git -C "$repo_diff" add -A && git -C "$repo_diff" commit -q -m 'file a bare file'
diff_bare="$(git -C "$repo_diff" rev-parse HEAD)"
diff_case 'an added file with no frontmatter' \
  "$repo_diff" "$diff_base" "$diff_bare" findings/file-a-bare-one 1
diff_says 'an added file with no frontmatter' \
  "$repo_diff" "$diff_base" "$diff_bare" findings/file-a-bare-one 'its frontmatter severity is [-]'
# A SECOND severity LINE INSIDE THE BLOCK DOES NOT OVERWRITE THE FIRST. Two `severity:` keys is not
# YAML anybody meant to write, and the two readings disagree about what it says: first-wins reads the
# P4 this file opens with, last-wins reads the P3 appended under it, and last-wins is a way to
# launder a severity past this check by adding a line.
branch_from two-severities
{ printf -- '---\nid: FIXTURE-2\nseverity: P4\nseverity: P3\n---\n\n## Failure sequence\n'; } \
  > "$repo_diff/findings/P3_correctness_202609110008_twice.md"
git -C "$repo_diff" add -A && git -C "$repo_diff" commit -q -m 'file a finding with two severities'
diff_twice="$(git -C "$repo_diff" rev-parse HEAD)"
diff_case 'an added finding with two frontmatter severities' \
  "$repo_diff" "$diff_base" "$diff_twice" findings/file-a-double 1
diff_says 'an added finding with two frontmatter severities' \
  "$repo_diff" "$diff_base" "$diff_twice" findings/file-a-double 'its frontmatter severity is [P4]'

# And the frontmatter's OWN first severity line wins: this file's block says P4 and its body says
# P3, and the block is what the ladder reads.
branch_from severity-in-the-body
{ finding_body P4; printf '\nA quoted row: severity: P3\n'; } \
  > "$repo_diff/findings/P3_correctness_202609110007_quoting.md"
git -C "$repo_diff" add -A && git -C "$repo_diff" commit -q -m 'file a finding quoting a severity'
diff_quoting="$(git -C "$repo_diff" rev-parse HEAD)"
diff_case 'an added finding whose body quotes another severity' \
  "$repo_diff" "$diff_base" "$diff_quoting" findings/file-a-quoting-one 1
diff_says 'an added finding whose body quotes another severity' \
  "$repo_diff" "$diff_base" "$diff_quoting" findings/file-a-quoting-one 'its frontmatter severity is [P4]'

# 5. A RENAME is checked, and it is checked at the name it leaves behind. Renaming a finding is how
#    a reviewer reclassifies one (findings/README.md), so this is the live path into a bad
#    name and not a hypothetical.
branch_from rename-into-range
git -C "$repo_diff" mv findings/P2_correctness_202609010001_to-be-renamed.md \
  findings/P3_correctness_202609010001_to-be-renamed.md
finding_body P3 > "$repo_diff/findings/P3_correctness_202609010001_to-be-renamed.md"
git -C "$repo_diff" add -A && git -C "$repo_diff" commit -q -m 'reclassify P2 as P3'
diff_rename_ok="$(git -C "$repo_diff" rev-parse HEAD)"
diff_case 'a reclassifying rename inside P0-P3' \
  "$repo_diff" "$diff_base" "$diff_rename_ok" findings/reclassify-a-finding 0
branch_from rename-out-of-range
git -C "$repo_diff" mv findings/P2_correctness_202609010001_to-be-renamed.md \
  findings/P4_correctness_202609010001_to-be-renamed.md
git -C "$repo_diff" add -A && git -C "$repo_diff" commit -q -m 'reclassify P2 as P4'
diff_rename_bad="$(git -C "$repo_diff" rev-parse HEAD)"
diff_case 'a rename out of P0-P3' \
  "$repo_diff" "$diff_base" "$diff_rename_bad" findings/reclassify-a-finding 1
diff_says 'a rename out of P0-P3' \
  "$repo_diff" "$diff_base" "$diff_rename_bad" findings/reclassify-a-finding \
  'P4_correctness_202609010001_to-be-renamed.md'
# The file it was renamed FROM is not reported: what a pull request leaves behind is what it is held
# to, and the old name is gone.
got_rename="$(diff_message "$repo_diff" "$diff_base" "$diff_rename_bad" findings/reclassify-a-finding)"
if [[ "$got_rename" == *'P2_correctness_202609010001_to-be-renamed.md'* ]]; then
  echo 'a rename out of P0-P3: the refusal named the path the file was renamed FROM' >&2
  exit 1
fi

# 6. AND THE FILES THE DIFF DOES NOT TOUCH ARE NOT CHECKED, which is what every case above has been
#    quietly asserting: the base carries `P9_correctness_...md` and `NOT-A-FINDING.md`, and the
#    confined case passed. Asserted directly too, because it is the property most easily lost.
diff_case 'a pull request that touches no finding at all' \
  "$repo_diff" "$diff_base" "$diff_confined" docs/a-slug 0
branch_from untouched-ledger
echo '// unrelated' >> "$repo_diff/src/engine.rs"
git -C "$repo_diff" add -A && git -C "$repo_diff" commit -q -m 'change code only'
diff_code_only="$(git -C "$repo_diff" rev-parse HEAD)"
diff_case 'a code-only pull request beside two badly named files' \
  "$repo_diff" "$diff_base" "$diff_code_only" feature/a-slug 0
# A file ADDED AND DELETED inside the pull request is in neither the diff nor the listing: what the
# pull request leaves behind is nothing, and there is nothing to hold it to.
branch_from added-then-deleted
finding_body P3 > "$repo_diff/findings/P4_correctness_202609110004_transient.md"
finding_body P3 > "$repo_diff/findings/P3_liveness_202609110005_the-real-one.md"
git -C "$repo_diff" add -A && git -C "$repo_diff" commit -q -m 'file a P4 and a P3'
git -C "$repo_diff" rm -q findings/P4_correctness_202609110004_transient.md
git -C "$repo_diff" commit -q -m 'and take the P4 away again'
diff_transient="$(git -C "$repo_diff" rev-parse HEAD)"
diff_case 'a badly named file added and deleted inside the pull request' \
  "$repo_diff" "$diff_base" "$diff_transient" findings/think-again 0

# 7. THE BOUNDARY IS THE MERGE BASE AND NOT THE TARGET'S CURRENT HEAD, which is the argument
#    findings-in-range.sh makes at length and which costs more here: rooted at the target's head,
#    every path MASTER has changed since the branch point is reported as a path this pull request
#    changed, and a findings/ branch confined to the ledger goes red because somebody else touched
#    `src/`. A false red on a pull request that is doing exactly the right thing.
#    It costs the same on the other listing and in the other direction: master DELETES a badly named
#    file that is still present at this branch's head, and rooted at the target that deletion reads
#    as this pull request ADDING the file -- so a pull request that has never been near
#    findings/ is refused over a name somebody else wrote and somebody else removed.
git -C "$repo_diff" checkout -q -B trunk-advanced "$diff_base"
echo '// master moved on' >> "$repo_diff/src/engine.rs"
echo 'more' > "$repo_diff/src/other.rs"
git -C "$repo_diff" rm -q findings/NOT-A-FINDING.md
git -C "$repo_diff" add -A
git -C "$repo_diff" commit -q -m 'master advances over src/ and tidies the ledger'
diff_advanced="$(git -C "$repo_diff" rev-parse HEAD)"
diff_case 'a confined findings/ branch after master advanced over src/' \
  "$repo_diff" "$diff_advanced" "$diff_confined" findings/file-a-new-one 0
# and the same pair still refuses the branch that really does change code, so the case above is
# about the boundary and not about the check having stopped working.
diff_case 'a findings/ branch that repairs code, after master advanced' \
  "$repo_diff" "$diff_advanced" "$diff_outside" findings/file-a-new-one 1

# 8. AN END THAT WILL NOT RESOLVE FAILS CLOSED IN THE BUILDER. A listing that could not be built is
#    not a listing with nothing in it, and the workflow step dies on it rather than handing the
#    validator a pair of empty files -- which for a findings/ branch is a pull request that changed
#    nothing and for every branch is a pull request that filed nothing.
empty_tree="$(git -C "$repo_diff" hash-object -t tree -w /dev/null)"
unrelated_commit="$(git -C "$repo_diff" commit-tree "$empty_tree" -m 'an unrelated history')"
builder_refuses() {  # builder_refuses <label> <target> <head> <expected substring>
  local said
  if said="$( cd "$repo_diff" && "$BASH" "$changed_script" "$2" "$3" "$fixture_dir/never-built" 2>&1 )"; then
    echo "changed-in-range.sh built listings for $1" >&2
    exit 1
  fi
  # AND IT SAYS WHICH END, because a step that goes red with git's own `fatal:` and nothing else
  # sends its reader to the wrong file. The refusal is the behaviour; naming the end is the half
  # that makes it actionable.
  if [[ "$said" != *"$4"* ]]; then
    echo "changed-in-range.sh refused $1 without saying so: [$said]" >&2
    exit 1
  fi
}
builder_refuses 'a head that is not in the checkout' \
  "$diff_base" 0000000000000000000000000000000000000000 'head commit 0000000'
builder_refuses 'a target that is not in the checkout' \
  0000000000000000000000000000000000000000 "$diff_confined" 'target commit 0000000'
builder_refuses 'two ends with no merge base' "$unrelated_commit" "$diff_confined" 'no merge base'
# A NAME THAT CANNOT BE HELD ON A LINE IS REFUSED RATHER THAN WRITTEN OUT. A newline is legal in a
# filename and it is the separator both listings are built from, so a record carrying one would
# arrive at the validator as two -- and the validator refuses a record with no tab in it, which is
# the second line of defence and not this one. This is the first: the builder does not write it.
newline_branch="$repo_diff/findings/$(printf 'P2_correctness_202609110009_a\nb.md')"
branch_from newline-in-a-name
finding_body P2 > "$newline_branch"
git -C "$repo_diff" add -A && git -C "$repo_diff" commit -q -m 'file a finding whose name has a newline'
diff_newline="$(git -C "$repo_diff" rev-parse HEAD)"
builder_refuses 'an added path holding a newline' "$diff_base" "$diff_newline" 'holds a newline'
branch_from after-the-newline
# and the same builder still writes both listings for a pair that resolves, so the three above are
# about the ends and not about the builder.
if ! ( cd "$repo_diff" && "$BASH" "$changed_script" "$diff_base" "$diff_confined" \
  "$fixture_dir/built" ) >/dev/null 2>&1; then
  echo 'changed-in-range.sh refused a pair of commits that both resolve' >&2
  exit 1
fi
for built in changed-paths added-findings; do
  [[ -s "$fixture_dir/built/$built" ]] \
    || { echo "changed-in-range.sh wrote no $built for a pull request that has one" >&2; exit 1; }
done

# 9. A LISTING THIS CANNOT READ IS A REFUSAL, NEVER AN EMPTY SET, and an EMPTY changed-path listing
#    is a refusal for a findings/ branch: a pull request that changes nothing files nothing, and
#    "the list was empty" is the shape every false acceptance this file has given wore.
missing_listing="$fixture_dir/no-such-listing"
present_listing="$fixture_dir/empty-listing"
: > "$present_listing"
unreadable_verdict() {  # unreadable_verdict <branch> <changed-paths> <added-findings>
  local rc=0
  "$BASH" "$branch_validator" "$1" '' '' '' "$2" "$3" >/dev/null 2>&1 || rc=$?
  echo "$rc"
}
if [[ "$(unreadable_verdict findings/a-slug "$missing_listing" "$present_listing")" != 1 ]]; then
  echo 'a changed-paths listing that could not be opened was not a refusal' >&2
  exit 1
fi
if [[ "$(unreadable_verdict docs/a-slug "$present_listing" "$missing_listing")" != 1 ]]; then
  echo 'an added-findings listing that could not be opened was not a refusal' >&2
  exit 1
fi
if [[ "$(unreadable_verdict findings/a-slug "$present_listing" "$present_listing")" != 1 ]]; then
  echo 'an empty changed-paths listing was accepted for a findings/ branch' >&2
  exit 1
fi
# An empty one is fine everywhere else -- a pull request that adds no finding adds no finding -- and
# with NO listings at all only the grammar is checked, which is how the fixtures above run.
if [[ "$(unreadable_verdict docs/a-slug "$present_listing" "$present_listing")" != 0 ]]; then
  echo 'an empty pair of listings was refused for a branch the limit does not apply to' >&2
  exit 1
fi
if [[ "$(unreadable_verdict findings/a-slug '' '')" != 0 ]]; then
  echo 'a findings/ branch with no listings at all was refused' >&2
  exit 1
fi

# 10. WHAT ROUND 1 OF THIS PULL REQUEST'S REVIEW FOUND, one case per finding. Each was measured
#     against the unrepaired builder before the repair and each failed there; they are here so the
#     suite, and not only a review, refuses the next one.
#
#     A RENAME INTO THE LEDGER IS A PATH OUTSIDE THE LEDGER TOO. `git diff --name-only` detects
#     renames by default and prints ONLY THE DESTINATION, so `git mv src/archive-me.rs
#     findings/P3_<...>.md` with frontmatter added arrived as a changed-path listing holding
#     one path under findings/ and nothing else -- and the confinement limit, which is the
#     whole of what makes the findings/ lane's low review safe, accepted a pull request that
#     deletes a source file. No push access is needed to open one: anyone can, from a fork.
branch_from rename-into-the-ledger
git -C "$repo_diff" mv src/archive-me.rs findings/P3_correctness_202609110012_archived.md
{ finding_body P3
  cat "$repo_diff/findings/P3_correctness_202609110012_archived.md"
} > "$fixture_dir/archived.md"
cp -- "$fixture_dir/archived.md" "$repo_diff/findings/P3_correctness_202609110012_archived.md"
git -C "$repo_diff" add -A && git -C "$repo_diff" commit -q -m 'archive a source file as a finding'
diff_rename_in="$(git -C "$repo_diff" rev-parse HEAD)"
# THE PRECONDITION IS ASSERTED, because without it this case passes for the wrong reason: if the
# two files were too dissimilar git would report an add and a delete, `src/archive-me.rs` would be
# listed by any implementation, and the case would say nothing about rename detection at all.
rename_status="$(git -C "$repo_diff" diff --name-status -M "$diff_base" "$diff_rename_in")"
case "$rename_status" in
  R*) ;;
  *) echo "the rename fixture is not a rename git detects, so it proves nothing: [$rename_status]" >&2
     exit 1 ;;
esac
diff_case 'a findings/ branch that renames a source file into the ledger' \
  "$repo_diff" "$diff_base" "$diff_rename_in" findings/archive-the-engine 1
diff_says 'a findings/ branch that renames a source file into the ledger' \
  "$repo_diff" "$diff_base" "$diff_rename_in" findings/archive-the-engine 'src/archive-me.rs'
# and the destination is still judged as an added finding, so turning detection off on the
# changed-path listing did not cost the other listing its rename handling.
diff_case 'the same rename on a prefix that admits code' \
  "$repo_diff" "$diff_base" "$diff_rename_in" ci/archive-the-engine 0

#     A SEVERITY HOLDING THE FIELD DELIMITER FORGES THE RECORD. added-findings is
#     `<severity><TAB><path>` and the severity is whatever the frontmatter said, so a block reading
#     `severity: P3<TAB>findings/forged` emitted three fields: the validator read severity
#     `P3`, took the injected text as the path, and the real file's severity was never judged.
#     Measured on the unrepaired builder: accepted, rc=0, on three payloads.
for forged_tail in 'findings/forged' \
                   'findings/P3_correctness_202609110009_forged.md' \
                   'findings/P3_correctness_202609110013_forged.md'; do
  branch_from severity-holding-a-tab
  # The injected severity is P3 -- a value the validator ACCEPTS -- which is what makes this the
  # exploit and not a refusal for some other reason: read back, the record says P3 and names a
  # path of the author's choosing, while the file's real severity is the whole injected string
  # and is judged by nothing.
  printf -- '---\nid: FIXTURE-6\nseverity: P3\t%s\ndisposition: deferred\n---\n' "$forged_tail" \
    > "$repo_diff/findings/P3_correctness_202609110013_forged.md"
  git -C "$repo_diff" add -A
  git -C "$repo_diff" commit -q -m 'file a finding whose severity holds a tab'
  diff_forged="$(git -C "$repo_diff" rev-parse HEAD)"
  builder_refuses "a frontmatter severity holding a tab [$forged_tail]" \
    "$diff_base" "$diff_forged" 'holds a tab or a line ending'
done
# A LINE ENDING IN THE VALUE SPLITS THE RECORD, and is refused for the reason a newline in a PATH
# is: one record per line is what this listing promises.
branch_from severity-holding-a-carriage-return
printf -- '---\nid: FIXTURE-6\nseverity: P3\rP3\ndisposition: deferred\n---\n' \
  > "$repo_diff/findings/P3_correctness_202609110015_split.md"
git -C "$repo_diff" add -A && git -C "$repo_diff" commit -q -m 'file a finding whose severity holds a CR'
diff_split="$(git -C "$repo_diff" rev-parse HEAD)"
builder_refuses 'a frontmatter severity holding a carriage return' \
  "$diff_base" "$diff_split" 'holds a tab or a line ending'

#     A LARGE FINDING IS STILL A FINDING. `git cat-file blob | awk` with an `exit` at the closing
#     fence left git writing into a pipe nobody was reading: SIGPIPE, status 141, and under
#     `pipefail` the builder exited 1 as soon as a finding outgrew a pipe buffer -- on EVERY branch
#     prefix, since the builder runs for all of them. The padding is well over 64 KB on purpose.
branch_from a-large-finding
{ finding_body P3
  awk 'BEGIN { for (i = 0; i < 4000; i++) print "padding padding padding padding padding padding" }'
} > "$repo_diff/findings/P3_liveness_202609110014_large.md"
git -C "$repo_diff" add -A && git -C "$repo_diff" commit -q -m 'file a large finding'
diff_large="$(git -C "$repo_diff" rev-parse HEAD)"
large_bytes="$(wc -c < "$repo_diff/findings/P3_liveness_202609110014_large.md")"
(( large_bytes > 100000 )) \
  || { echo "the large-finding fixture is only $large_bytes bytes, which is not comfortably past a pipe buffer" >&2; exit 1; }
diff_case 'a findings/ branch filing a finding larger than a pipe buffer' \
  "$repo_diff" "$diff_base" "$diff_large" findings/file-a-large-one 0
# and its severity is READ, not merely survived: the same file named P3_ with a P4 block is
# refused, which a builder that answered `-` for everything large would not do.
branch_from a-large-mismatched-finding
{ finding_body P4
  awk 'BEGIN { for (i = 0; i < 4000; i++) print "padding padding padding padding padding padding" }'
} > "$repo_diff/findings/P3_liveness_202609110016_large-mismatch.md"
git -C "$repo_diff" add -A && git -C "$repo_diff" commit -q -m 'file a large mismatched finding'
diff_large_bad="$(git -C "$repo_diff" rev-parse HEAD)"
diff_says 'a large finding whose frontmatter severity is P4' \
  "$repo_diff" "$diff_base" "$diff_large_bad" findings/file-a-large-one 'its frontmatter severity is [P4]'

#     CRLF IS A LINE ENDING HERE TOO. The opening fence was compared exactly, so a file authored on
#     Windows opens `---\r`, matched nothing, and every CRLF finding read as having no frontmatter
#     at all -- refused at rc=1 for the line endings it was written with. validate-pr-branch.sh
#     already takes CRLF as a line ending in every listing it reads and this reader has to agree.
branch_from crlf-frontmatter
printf -- '---\r\nid: FIXTURE-7\r\nseverity: P3\r\ndisposition: deferred\r\n---\r\n' \
  > "$repo_diff/findings/P3_correctness_202609110017_crlf.md"
git -C "$repo_diff" add -A && git -C "$repo_diff" commit -q -m 'file a CRLF finding'
diff_crlf="$(git -C "$repo_diff" rev-parse HEAD)"
diff_case 'a findings/ branch filing a CRLF-authored finding' \
  "$repo_diff" "$diff_base" "$diff_crlf" findings/file-a-crlf-one 0
# and the value is read, not defaulted: the same file with a P4 block is refused AS A P4, which a
# reader that answered `-` for every CRLF file would report as [-].
branch_from crlf-frontmatter-mismatched
printf -- '---\r\nid: FIXTURE-7\r\nseverity: P4\r\ndisposition: deferred\r\n---\r\n' \
  > "$repo_diff/findings/P3_correctness_202609110018_crlf-bad.md"
git -C "$repo_diff" add -A && git -C "$repo_diff" commit -q -m 'file a mismatched CRLF finding'
diff_crlf_bad="$(git -C "$repo_diff" rev-parse HEAD)"
diff_says 'a CRLF-authored finding whose frontmatter severity is P4' \
  "$repo_diff" "$diff_base" "$diff_crlf_bad" findings/file-a-crlf-one 'its frontmatter severity is [P4]'

echo 'diff-rule fixtures passed'

# ---- the shape rule: the unsafe call must be impossible to WRITE, not just absent -------------
#
# Five rounds of finding these one at a time produced more of them each round,
# and three of one round's four were introduced by the repairs themselves. A gate
# can only test the instances somebody imagined. So the validator routes every
# external probe, every file read and every directory listing through three
# audited helpers, and this asserts THE SHAPE of everything below the AUDITED
# HELPERS END marker.
#
# IT IS AN ALLOWLIST AND NOT A BAN LIST, because the ban list was defeated the
# round it was written. `LC_ALL=C git ls-files` walked past a command-position
# regex with no room for an assignment; `command -- git` past one with no room
# for an option; `sed -n p -- "$1"` past a reader list that named `cat` and
# `head` and not `sed`; and `read -r record<"$f"` past a redirection regex that
# wanted a space before the `<`. All four are probes below. Naming what MAY run
# has no such gaps to find: a command must be a shell builtin from the short list
# in the scanner or a function the validator itself defines, and nothing may
# redirect from a path.
#
# WHAT IT IS AND IS NOT, because the body used to claim more than this. It is a
# TEXT SCAN over one file. It bounds what is WRITTEN in the validator; it does
# not bound what bash can be made to do, and it is not a sandbox. What it buys is
# the only thing claimed for it: THE REVIEWED SURFACE IS THE AUDITED REGION,
# capped below at a size that can be read in one sitting, instead of every call
# site in a 1500-line file.
#
# THE RESIDUALS ARE LISTED RATHER THAN DENIED, and the list shortens as they are
# closed. A command word that is ENTIRELY inside quotes leaves nothing on the
# bare line to read -- `"$reader"` on its own is invisible here -- and a command
# reached through an `eval` of a string this cannot see is outside any text scan.
# Two more were on this list until the review that executed them: a command
# substitution inside `[[ … ]]`, whose words this skipped because the test itself
# holds no command, and one inside a `case` WORD, skipped up to the `)` that ends
# a label. Bash runs both, and both are caught below now.
#
# THE INSTRUMENT IS TESTED FIRST, because a rule that matches nothing would pass
# this file forever while proving nothing: each shape below is appended to a COPY
# of the validator and must be caught.
shape_violations() {  # shape_violations <script> -> "<line>: <text>" per violation
  awk '
    # Pass one: the functions this file defines are commands it may run.
    NR == FNR {
      if ($0 ~ /^[[:space:]]*[A-Za-z_][A-Za-z0-9_]*\(\)/) {
        name = $0
        sub(/^[[:space:]]*/, "", name)
        sub(/\(\).*$/, "", name)
        defined[name] = 1
      }
      next
    }
    BEGIN {
      # The builtins the validator may run. `cd` and `pwd` are here because
      # locating a repository means entering a directory; they open no file and
      # run no program.
      split(": true false echo printf local return exit shift unset shopt read set export cd pwd trap break continue", w, " ")
      for (i in w) allowed[w[i]] = 1
      # Keywords that introduce a COMMAND: scanning continues past them.
      split("if elif while until then else do time", t, " ")
      for (i in t) transparent[t[i]] = 1
      # Keywords that do not: what follows them on that line is a variable, a
      # word list or nothing, and a command comes only after the next separator.
      split("for select in function fi done", o, " ")
      for (i in o) opaque[o[i]] = 1
      audited = 0; heredoc = ""; instring = 0; insingle = 0
      incase = 0; want_label = 0; subst = 0
    }
    {
      line = $0
      if (heredoc != "") { if (line == heredoc) { heredoc = "" } next }
      if (line ~ /^# ==== AUDITED HELPERS BEGIN/) { audited = 1; next }
      if (line ~ /^# ==== AUDITED HELPERS END/)   { audited = 0; next }
      if (line ~ /^[[:space:]]*#/) { next }
      if (match(line, /<<-?[\x27"][A-Za-z_][A-Za-z0-9_]*[\x27"]/)) {
        tag = substr(line, RSTART, RLENGTH)
        sub(/^<<-?[\x27"]/, "", tag); sub(/[\x27"]$/, "", tag)
        heredoc = tag
      }
      # A quoted run becomes one Q on the masked line and vanishes from the bare
      # one. Redirections are judged on the masked line, so a `<` inside a
      # message is prose and a `<` outside one is an open; commands are read off
      # the bare line, so a case label like `100644|100755)` is not read as one.
      masked = ""; bare = ""
      n = length(line); i = 1
      while (i <= n) {
        c = substr(line, i, 1)
        # A COMMAND SUBSTITUTION INSIDE A STRING IS STILL CODE. `x="$(git …)"`
        # is quoted from end to end, so a scan that dropped quoted runs whole
        # saw no command at all -- and `$(cat -- "$f")` with it.
        if (!insingle && c == "$" && substr(line, i + 1, 1) == "(" && substr(line, i + 2, 1) != "(") {
          subst++; subst_string[subst] = instring; subst_single[subst] = insingle
          instring = 0; insingle = 0
          masked = masked "$("; bare = bare "$("
          i += 2; continue
        }
        if (c == ")" && subst > 0 && !instring && !insingle) {
          instring = subst_string[subst]; insingle = subst_single[subst]; subst--
          masked = masked ")"; bare = bare ")"
          i++; continue
        }
        if (instring)    { if (c == "\"")   { instring = 0 } ; masked = masked "Q"; i++; continue }
        if (insingle)    { if (c == "\x27") { insingle = 0 } ; masked = masked "Q"; i++; continue }
        if (c == "\"")   { instring = 1; masked = masked "Q"; i++; continue }
        if (c == "\x27") { insingle = 1; masked = masked "Q"; i++; continue }
        masked = masked c; bare = bare c; i++
      }
      if (audited) { next }
      # ARITHMETIC HOLDS NEITHER A COMMAND NOR A REDIRECTION, and it is taken out
      # of both readings before either is made: `(( i < n ))` is a comparison,
      # and read as an open it reported every `<` the file writes in arithmetic.
      gsub(/\$\(\([^)]*\)\)/, " ", bare)
      gsub(/\(\([^)]*\)\)/, " ", bare)
      gsub(/\$\(\([^)]*\)\)/, " ", masked)
      gsub(/\(\([^)]*\)\)/, " ", masked)
      # A redirection FROM a path. `<<` and `<<<` are a heredoc and a here-string
      # and open nothing; `<&` duplicates a descriptor this shell already holds.
      if (masked ~ /(^|[^<])<[[:space:]]*[^<&[:space:]]/) { print FNR ": " $0; next }
      cmdpos = 1; intest = 0; depth = 0; i = 1; n = length(bare)
      while (i <= n) {
        c = substr(bare, i, 1)
        if (c == " " || c == "\t") { i++; continue }
        # A COMMAND SUBSTITUTION IS CODE WHEREVER IT IS WRITTEN, AND TWO PLACES
        # THIS SKIPS ARE PLACES BASH DOES NOT. `[[ … ]]` holds no command of its
        # own, so its words are skipped; a `case` label is skipped up to its
        # `)`. `[[ -n "$(git ls-files -sz -- .)" ]]` and `case "$(git rev-parse
        # HEAD)" in` are both of those, both run the command, and both were
        # reported clean. So `$(` suspends whichever skip is in force and its
        # matching `)` restores it, tracked by depth so a plain `(` -- a
        # subshell, or a `(pattern)` case label -- restores nothing.
        if (c == "(") {
          depth++
          substitution[depth] = (substr(bare, i - 1, 1) == "$")
          if (substitution[depth]) {
            test_stack[depth] = intest; label_stack[depth] = want_label
            intest = 0; want_label = 0
          }
          cmdpos = 1; i++; continue
        }
        if (c == ")") {
          if (depth > 0) {
            if (substitution[depth]) { intest = test_stack[depth]; want_label = label_stack[depth] }
            depth--
          }
          cmdpos = 1; i++; continue
        }
        if (c == ";" || c == "&" || c == "|") {
          if (c == ";" && substr(bare, i + 1, 1) == ";" && !intest) {
            want_label = (incase > 0)
          }
          cmdpos = 1; i++; continue
        }
        tok = ""
        while (i <= n) {
          c = substr(bare, i, 1)
          if (c == " " || c == "\t" || c == ";" || c == "&" || c == "|" || c == "(" || c == ")") { break }
          tok = tok c
          i++
        }
        term = (i <= n) ? substr(bare, i, 1) : ""
        # A `[[ … ]]` test holds no command, and its contents may carry every
        # separator there is.
        if (intest) { if (tok == "]]") { intest = 0; cmdpos = 0 } ; continue }
        if (tok == "[[") { intest = 1; continue }
        if (tok == "esac") { if (incase > 0) { incase-- } ; want_label = 0; cmdpos = 0; continue }
        # Inside a case block the words up to the first `)` name a branch; the
        # branch itself follows them.
        if (want_label) { if (term == ")") { want_label = 0; cmdpos = 1 } ; continue }
        if (!cmdpos) { continue }
        if (tok == "{" || tok == "}" || tok == "!" || tok == "\\") { continue }
        if (tok ~ /^[A-Za-z_][A-Za-z0-9_]*(\[.*\])?\+?=/) { continue }
        if (tok == "case") { incase++; want_label = 1; continue }
        if (transparent[tok]) { continue }
        if (opaque[tok]) { cmdpos = 0; continue }
        cmdpos = 0
        if (tok ~ /[*?]/) { continue }
        if (tok ~ /^[0-9]/) { continue }
        if (allowed[tok] || defined[tok]) { continue }
        print FNR ": " $0
        break
      }
    }
  ' "$1" "$1"
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
# git, in every position a shell will take it.
shape_probe 'git status >/dev/null'
shape_probe 'if git diff --quiet; then :; fi'
shape_probe 'mode="$(git -C "$d" ls-files -s -- x)"'
shape_probe 'echo hi | git hash-object --stdin'
shape_probe '{ git status; } >/dev/null'
shape_probe 'command -v git >/dev/null'
shape_probe 'command git status'
shape_probe 'exec git status'
shape_probe 'env git status'
shape_probe 'xargs git add'
shape_probe 'eval git status'
shape_probe '/usr/bin/git status'
shape_probe 'while :; do git status; done'
shape_probe 'until git status; do :; done'
shape_probe 'time git status'
shape_probe '! git diff --quiet'
shape_probe 'probe() { case x in a) git status ;; esac; }'
# THE TWO THIS ROUND'S REVIEW EXECUTED, and the third of the same shape. Bash
# runs every one of these substitutions; the scan reported no violation.
shape_probe 'unchecked_git() { [[ -n "$(git ls-files -sz -- .)" ]] || true; }'
shape_probe 'unchecked_read() { [[ -n "$(cat -- "$1")" ]] || true; }'
shape_probe 'case "$(git rev-parse HEAD)" in *) : ;; esac'
shape_probe 'probe() { if [[ -z "$(cat "$f")" ]]; then :; fi; }'
shape_probe 'probe() { [[ "$(git rev-parse HEAD)" == x ]] || true; }'
# And arithmetic is not a redirection, but a redirection beside it still is.
shape_probe 'if (( 1 < 2 )); then read -r l < "$f"; fi'
# THE FOUR THE ROUND BEFORE EXECUTED, and the two shapes they are.
shape_probe 'unchecked_git() { LC_ALL=C git ls-files -sz -- . || true; }'
shape_probe 'unchecked_read() { sed -n p -- "$1" || true; }'
shape_probe 'probe() { command -- git status; }'
shape_probe 'probe() { grep -c . "$listing"; }'
shape_probe 'probe() { read -r record<"$listing"; }'
shape_probe 'GIT_DIR=x git rev-parse'
shape_probe 'LC_ALL=C LANG=C sed -n p "$f"'
shape_probe 'nice -n 1 git status'
# A file read, by redirection and by argument, builtin and external.
shape_probe 'while IFS= read -r record; do :; done < <(git ls-files -z)'
shape_probe 'IFS= read -r line < "$listing"'
shape_probe 'while read -r l; do :; done < "$f"'
shape_probe 'while read -r x; do :; done < /etc/hostname'
shape_probe 'out="$(cat -- "$listing")"'
shape_probe 'n="$(wc -c < "$listing")"'
shape_probe 'x=$(< "$listing")'
shape_probe 'head -1 "$f"'
shape_probe 'tail -n +1 -- "$f"'
shape_probe 'dd if="$f" of=/dev/null'
shape_probe 'mapfile -t arr < "$f"'
shape_probe 'readarray -t a <"$f"'
shape_probe 'awk "{print}" "$f"'
shape_probe 'cut -f1 "$f"'
shape_probe 'busybox cat "$f"'
shape_probe 'exec 7< "$f"'
shape_probe 'source "$f"'
shape_probe '. "$f"'
shape_probe 'builtin read -r l < "$f"'
shape_probe 'for f in a b; do sed -n p "$f"; done'
shape_probe 'select f in a; do cat "$f"; done'
shape_probe 'probe() { if [[ -n "$x" ]]; then sed -n p "$f"; fi; }'
# A line that ENDS in a continuation, because the backslash is a word and was
# read as one: the command before it was accepted and the backslash after it was
# not, so every `x="$(cmd)" \` in the file reported a violation of its own.
shape_probe 'x="$(git rev-parse HEAD)" \'
shape_probe 'out="$(cat -- "$listing")" \'

# And the file as it stands has none of them.
shape_found="$(shape_violations "$branch_validator")"
if [[ -n "$shape_found" ]]; then
  echo 'validate-pr-branch.sh runs a command it may not, or reads a file, outside its audited helpers:' >&2
  printf '%s\n' "$shape_found" >&2
  exit 1
fi

# The audited region has to stay small enough that reading it is the whole audit.
#
# THE CAP HAS MOVED ONCE, FROM 200 TO 250, in the round that made `capture` the
# only way any helper reaches bytes: 133 lines of code became 158, because
# checking that a destination opened, that the producer ran, and that both copies
# read back to their sentinel is four checks where each helper used to make three
# and the three were not the same three. A cap is a reviewer's reading time and
# not a budget to spend, so the number is stated here rather than adjusted
# quietly, and the pull request that moves it again says why in its body.
shape_region_lines="$(awk '
  /^# ==== AUDITED HELPERS BEGIN/ { inside = 1 }
  inside { n = n + 1 }
  /^# ==== AUDITED HELPERS END/ { inside = 0 }
  END { print n }' "$branch_validator")"
if (( shape_region_lines > 250 )); then
  echo "the audited helpers have grown to $shape_region_lines lines, which is no longer an audit" >&2
  exit 1
fi

# ---- the retired migration list stays retired ----------------------------------------------
#
# The list and the exemption it fed are gone, and a gate that simply stopped
# mentioning them would not notice either coming back. Both names are checked
# by text because both are what a reintroduction would have to spell.
if [[ -e "$root/.github/legacy-branches.txt" ]]; then
  echo 'the retired migration list is back in .github/' >&2
  exit 1
fi
if grep -q 'LEGACY_BRANCHES\|legacy-branches' "$branch_validator"; then
  echo 'validate-pr-branch.sh reads a migration list again' >&2
  exit 1
fi

echo 'listing-construction fixtures passed'

echo 'branch vocabulary fixtures passed'

echo 'PR policy fixtures passed'
