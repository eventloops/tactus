#!/usr/bin/env bash
# Documentation and workflow-trigger claims that go stale silently, checked
# against the tree rather than against a hard-coded copy of it.
#
# THE CLAIMS THIS GATE ENFORCES -- exactly these, nothing else is in its scope:
#
#   C1  CLAUDE.md and CONTRIBUTING.md exist. Every repository path either names
#       in backticks exists at this head, or EACH occurrence is qualified within
#       its own window (three lines before to four after) by one of the marker
#       phrases below. Qualification is syntactic: the gate checks that a phrase
#       is present near that occurrence, not what the phrase refers to. And
#       CLAUDE.md does not carry a sentence matching
#       /CONTRIBUTING\.md.{0,40}(omits|is stale|does not (carry|include))/ while
#       CONTRIBUTING.md carries `--all-features` -- the stale cross-document
#       claim PR #20's review had to catch by hand.
#   C2  ci.yml's msrv job selects exactly one toolchain, and it is Cargo.toml's
#       rust-version or a patch release of it.
#   C3  CLAUDE.md's gate-count claim equals the tree, and the set of test-*.sh
#       files in .github/scripts EQUALS the set the lint job invokes, both
#       directions. An invocation from any other job does not count.
#   C4  The workflow trigger contract is EXACTLY the value written below, which
#       restates what MAINTAINING.md's Repository rules say but is NOT compared
#       with that file -- this gate reads no document here, so a change to the
#       prose alone passes it, and the two move together by review, not by
#       machine: ci.yml triggers on push, pull_request and merge_group,
#       pr-policy.yml on pull_request and merge_group, each with the branch
#       list [master] and nothing else, and
#       merge_group with the activity type [checks_requested] only. The two
#       attestation workflows that record once pinned were retired with the App
#       check (decisions/2026-08-23-retire-app-attestation.md); there is no
#       privileged workflow left to pin.
#   C5  No tracked path exists under reviews/findings/. The finding ledger
#       moved to findings/ on 2026-09-12 (pull request #276), and git tracks
#       files rather than directories: a branch cut before the move that adds
#       a finding under the old prefix merges with no conflict and recreates
#       the directory, holding findings no gate, lane rule or ledger reads.
#       The prefix is matched case-sensitively and with its trailing slash, so
#       reviews/FINDINGS.md, the closed ledger that differs from the moved
#       directory only in case, never matches. AND THE LISTING MUST SUCCEED:
#       where `git ls-files` cannot answer, this check reports a failure and
#       never a pass, because an index that was not read is an UNCHECKED prefix
#       and not an empty one.
#
# WITHDRAWN, DELIBERATELY (round 5 of this file's review): this gate makes NO
# claim about which cargo commands CI runs, whether CI executes them, or which
# commands the documents list. Four review rounds showed that surface to be
# open-ended for a text checker -- a command can be present and skipped
# (`if: false`), a document can be missing, an example can contain the string --
# and the release gates are not enforced by prose in the first place: at the time
# the trusted attestation workflow reran them from its own default-branch
# definition on every dispatch (retired since, see C4), and the reviewer reads
# both the documents and ci.yml. The mutations that demonstrated
# the withdrawn claims are kept by name as history, not as kills:
# MUT-TEMPLATE-MSRV-REMOVED, MUT-CI-CLIPPY-ALL-FEATURES-REMOVED,
# MUT-CI-MSRV-TOOLCHAIN-DRIFT's document half, MUT-CI-CARGO-TEST-STEP-DELETED,
# MUT-CLAUDE-TEST-SCOPE-NARROWED, MUT-CI-CARGO-TEST-STEP-SKIPPED and
# MUT-TEMPLATE-DELETED.
#
# EVERY CHECK THAT REMAINS IS AN EQUALITY OR AN EXACT PIN. A presence test -- a
# substring, a one-way subset, a forbidden value standing in for a required one,
# a flag per path instead of per occurrence -- is how every earlier version of
# this file was killed, and each fix below names the mutation it exists to kill:
#   round 1: MUT-CI-PR-BRANCH-MASKED (whole-file grep),
#            MUT-ROOT-PATH-MISSPELLED (path regex blind to root files),
#            MUT-GATE-COUNT-STALE (a count nobody checked);
#   round 2: MUT-INVALIDATOR-MASTER-REMOVED (forbidding a value is not pinning
#            one), MUT-CI-MSRV-TOOLCHAIN-DRIFT (toolchain never compared),
#            MUT-CI-BASH-GATE-OMITTED (files counted, invocations not);
#   round 3: MUT-MASTER-TRIGGERS-REMOVED (integration branch present, master
#            not required), MUT-FORWARD-PATH-REUSED-AS-CURRENT (one qualified
#            occurrence marked the path for all of them);
#   round 4: MUT-CONTRIBUTING-DELETED (a required document treated as optional);
#   round 6: MUT-C5-PRODUCER-FAILS-OPEN (a guard whose own listing command
#            could fail unnoticed, so the check it never ran read as a pass).
set -euo pipefail
export PATH="/usr/bin:/bin:$PATH"

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$script_dir/../.." && pwd)"
cd "$root"

failed=0
error() { echo "$*" >&2; failed=1; }

# block <file> <key>: the lines nested under a two-space-indented YAML key --
# an `on:` event such as `pull_request_target`, or a job such as `lint`. The
# block ends at the next key at that indentation or at the next top-level key.
# Keys may carry hyphens (`merge-gate`), or the lint block would run on into
# the next job and a gate invoked from the wrong job would count as invoked.
block() {
  awk -v key="  $2:" '
    $0 == key { inblock = 1; next }
    inblock && /^  [A-Za-z0-9_-]+:/ { inblock = 0 }
    inblock && /^[A-Za-z]/ { inblock = 0 }
    inblock { print }
  ' "$1"
}

# events <file>: the event keys under `on:`, one per line, in file order.
events() {
  awk '
    /^on:/ { inon = 1; next }
    inon && /^[A-Za-z]/ { inon = 0 }
    inon && match($0, /^  [A-Za-z_]+:/) { print substr($0, 3, RLENGTH - 3) }
  ' "$1"
}

# branches_line <file> <event>: the branch filter under one event, with the
# surrounding whitespace stripped. Every line under the event that starts with
# `branches:` is printed, so two filters -- or none -- fail the exact comparison.
branches_line() {
  block "$1" "$2" | grep -E '^\s*branches:' | sed -E 's/^\s+//; s/\s+$//' || true
}
# types_line <file> <event>: the activity-type filter under one event, same terms as above.
types_line() {
  block "$1" "$2" | grep -E '^\s*types:' | sed -E 's/^\s+//; s/\s+$//' || true
}

# --- C1. the documents exist; every path they name resolves, per occurrence --
# MUT-CONTRIBUTING-DELETED: a document this gate reads is required, not
# optional -- a missing one is a failure, never a vacuous pass.
# MUT-ROOT-PATH-MISSPELLED: bare document names must exist at the root, not only
# directory-prefixed paths. MUT-FORWARD-PATH-REUSED-AS-CURRENT: a qualified
# forward reference used to mark the PATH, so a second, unqualified occurrence of
# the same missing path passed as a current pointer. Each occurrence is judged
# on its own window now. A qualifier may say the path is coming, or that it
# deliberately does not exist ("there is **no** rust-toolchain.toml").
marker='arrives with|arrive with|not yet|until that merges|until it merges|lands with|forward reference|\*\*no |there is \*\*?no|does not exist|must not exist'
for doc in CLAUDE.md CONTRIBUTING.md; do
  [[ -f "$doc" ]] || { error "$doc is missing: this gate requires it"; continue; }
  rooted=$(grep -oE '`(src|infra|\.github|acceptance|decisions|proposals|reviews|findings|examples|fixtures|docs)/[A-Za-z0-9_./-]*`' "$doc" | tr -d '`' || true)
  bare=$(grep -oE '`[A-Za-z0-9][A-Za-z0-9_.-]*\.(md|toml|lock)`' "$doc" | tr -d '`' || true)
  while IFS= read -r path; do
    [[ -z "$path" ]] && continue
    [[ -e "$path" ]] && continue
    while IFS= read -r line_no; do
      [[ -z "$line_no" ]] && continue
      from=$(( line_no > 3 ? line_no - 3 : 1 ))
      sed -n "${from},$(( line_no + 4 ))p" "$doc" | grep -qiE "$marker" \
        || error "$doc:$line_no names \`$path\`, which does not exist at this head; this occurrence is neither marked as a forward reference nor stated as deliberately absent"
    done < <(grep -nF -- "\`$path\`" "$doc" | cut -d: -f1)
  done < <(printf '%s\n%s\n' "$rooted" "$bare" | grep -v '^$' | sort -u)
done

# A claim that another document is stale must not outlive the fix. This is the
# sentence PR #20's review caught by hand: CLAUDE.md asserted CONTRIBUTING.md
# omitted --all-features while the same commit added it. The pattern is the
# claim; a differently worded claim is outside C1.
if [[ -f CLAUDE.md && -f CONTRIBUTING.md ]] \
   && grep -Fq -- '--all-features' CONTRIBUTING.md \
   && grep -qiE 'CONTRIBUTING\.md.{0,40}(omits|is stale|does not (carry|include))' CLAUDE.md; then
  error "CLAUDE.md claims CONTRIBUTING.md omits --all-features, but CONTRIBUTING.md carries it at this head"
fi

# --- C2. MSRV: ci.yml's msrv job agrees with Cargo.toml ----------------------
# MUT-CI-MSRV-TOOLCHAIN-DRIFT: the msrv job could move to 1.86.0 while
# Cargo.toml still promised 1.85. The job's toolchain is compared with
# rust-version directly; no document is consulted.
rust_version=$(sed -nE 's/^rust-version\s*=\s*"([0-9]+\.[0-9]+(\.[0-9]+)?)"\s*$/\1/p' Cargo.toml | head -1)
[[ -n "$rust_version" ]] || error "Cargo.toml carries no rust-version to pin the msrv job against"
msrv_toolchains=$(block .github/workflows/ci.yml msrv | grep -E '^\s*toolchain:' | sed -E 's/^\s*toolchain:\s*//; s/\s+$//' || true)
if [[ -z "$msrv_toolchains" ]]; then
  error "ci.yml has no msrv job selecting a toolchain"
elif [[ "$(wc -l <<< "$msrv_toolchains")" -ne 1 ]]; then
  error "ci.yml msrv job must select exactly one toolchain, got: $(tr '\n' ' ' <<< "$msrv_toolchains")"
elif [[ -n "$rust_version" && "$msrv_toolchains" != "$rust_version" && "$msrv_toolchains" != "$rust_version".* ]]; then
  error "ci.yml msrv job runs toolchain $msrv_toolchains but Cargo.toml rust-version is $rust_version"
fi

# --- C3. the gate inventory: tree == lint-job invocations; CLAUDE.md's count --
# MUT-GATE-COUNT-STALE: a count is a fact about the tree; check it.
# MUT-CI-BASH-GATE-OMITTED: files in the tree prove nothing about CI running
# them; every test-*.sh must be invoked by a `- run: bash .github/scripts/<name>`
# line inside the lint job's own block, and the lint job must invoke nothing the
# tree does not carry. An invocation from another job does not count: block()
# ends at the next job.
tree_gates=$(ls .github/scripts/test-*.sh 2>/dev/null | sed 's|^\.github/scripts/||' | sort -u)
lint_gates=$(block .github/workflows/ci.yml lint \
  | grep -oE '^\s*- run: bash \.github/scripts/test-[A-Za-z0-9_.-]+\.sh\s*$' \
  | sed -E 's|^\s*- run: bash \.github/scripts/||; s|\s*$||' | sort -u || true)
[[ -n "$lint_gates" ]] || error "ci.yml's lint job invokes no .github/scripts/test-*.sh gate"
while IFS= read -r gate; do
  [[ -z "$gate" ]] && continue
  grep -qxF "$gate" <<< "$lint_gates" \
    || error ".github/scripts/$gate exists but ci.yml's lint job never runs it"
done <<< "$tree_gates"
while IFS= read -r gate; do
  [[ -z "$gate" ]] && continue
  grep -qxF "$gate" <<< "$tree_gates" \
    || error "ci.yml's lint job runs .github/scripts/$gate, which is not in the tree"
done <<< "$lint_gates"
actual_gates=$(printf '%s\n' "$tree_gates" | grep -c . || true)
if [[ -f CLAUDE.md ]]; then
  while IFS= read -r claimed; do
    [[ -z "$claimed" ]] && continue
    [[ "$claimed" == "$actual_gates" ]] \
      || error "CLAUDE.md claims $claimed \`test-*.sh\` gates; the tree has $actual_gates"
  done < <(grep -oE '[0-9]+ `test-\*\.sh` gates' CLAUDE.md | grep -oE '^[0-9]+')
  grep -qE '[0-9]+ `test-\*\.sh` gates' CLAUDE.md \
    || error "CLAUDE.md must state the gate count as 'N \`test-*.sh\` gates' so it can be checked"
fi

# --- C4. the trigger contract, pinned exactly --------------------------------
# MUT-CI-PR-BRANCH-MASKED: each event's own block, never the whole file.
# MUT-MASTER-TRIGGERS-REMOVED: requiring the integration branch to be present
# let master be removed; the branch list is compared for exact equality, which
# is what keeps a second base from being added unnoticed now that the list is
# master alone. MUT-INVALIDATOR-MASTER-REMOVED: forbidding the
# integration-branch name is not pinning master; the invalidator's filter is
# compared for exact equality too. The event set of every workflow is pinned as
# well, so a trigger cannot be added or removed unnoticed.
# MAINTAINING.md, Repository rules
branch_list='branches: [master]'
pin_events() {  # pin_events <file> <expected events, sorted, space separated>
  local f="$1" want="$2" got
  got="$(events "$f" | sort | tr '\n' ' ' | sed -E 's/ +$//')"
  [[ "$got" == "$want" ]] \
    || error "$f must trigger on exactly [$want], got [${got:-<none>}]"
}
pin_branches() {  # pin_branches <file> <event> <expected branches line>
  local f="$1" event="$2" want="$3" got
  got="$(branches_line "$f" "$event")"
  [[ "$got" == "$want" ]] \
    || error "$f: $event must carry exactly '$want', got: ${got:-<none>}"
}
pin_types() {  # pin_types <file> <event> <expected types line>
  local f="$1" event="$2" want="$3" got
  got="$(types_line "$f" "$event")"
  [[ "$got" == "$want" ]] \
    || error "$f: $event must carry exactly '$want', got: ${got:-<none>}"
}
for f in .github/workflows/ci.yml .github/workflows/pr-policy.yml; do
  [[ -f "$f" ]] || error "$f is missing"
done
# merge_group is pinned on both workflows and on the same branch list: an entry
# the queue builds for a listed base must receive both required contexts, or it
# sits in the queue until it times out (MAINTAINING.md, Repository rules). Its
# activity type is pinned to checks_requested: an unpinned merge_group
# subscribes to every activity type GitHub adds later, and both contexts would
# start running on them.
merge_group_types='types: [checks_requested]'
if [[ -f .github/workflows/ci.yml ]]; then
  pin_events .github/workflows/ci.yml "merge_group pull_request push"
  pin_branches .github/workflows/ci.yml push "$branch_list"
  pin_branches .github/workflows/ci.yml pull_request "$branch_list"
  pin_branches .github/workflows/ci.yml merge_group "$branch_list"
  pin_types .github/workflows/ci.yml merge_group "$merge_group_types"
fi
if [[ -f .github/workflows/pr-policy.yml ]]; then
  pin_events .github/workflows/pr-policy.yml "merge_group pull_request"
  pin_branches .github/workflows/pr-policy.yml pull_request "$branch_list"
  pin_branches .github/workflows/pr-policy.yml merge_group "$branch_list"
  pin_types .github/workflows/pr-policy.yml merge_group "$merge_group_types"
fi

# --- C5. the finding ledger lives at findings/ and nowhere else -------------
# Git tracks files, not directories. reviews/findings/ was moved to findings/
# on 2026-09-12 (pull request #276). A branch cut before the move that ADDS a
# file under the old prefix merges with no conflict -- the move deleted the old
# paths and the branch adds new ones -- and master ends up with both
# directories, the old one holding findings that no gate, lane rule or ledger
# reads any more. A branch that MODIFIES an old path raises a modify/rename
# conflict and needs no help from here. `git ls-files` answers from the index,
# which on a clean checkout is the commit under test; the prefix test is bash's
# own, case-sensitive whatever core.ignorecase says, so reviews/FINDINGS.md --
# the closed ledger, which differs from the moved directory only in case -- is
# never matched. The message says what to do, because whoever reads it is in
# the middle of a rebase.
#
# THE LISTING'S STATUS IS CHECKED BEFORE ITS OUTPUT IS BELIEVED.
# MUT-C5-PRODUCER-FAILS-OPEN: this loop read `done < <(git ls-files -z)`, and
# BASH DISCARDS A PROCESS SUBSTITUTION'S EXIT STATUS -- what the loop reports is
# the loop's own status and the producer's belongs to nobody. `git ls-files -z`
# exits 128 on an index it cannot read and on no repository at all; either way
# the loop body then ran ZERO times, old_ledger_paths stayed empty, the `if`
# below was skipped, and this gate printed `documentation consistency fixtures:
# PASS` and exited 0. Two review lenses reproduced that independently on one
# head, one with `chmod 000` on the index and one with a corrupt GIT_INDEX_FILE.
# A guard that passes when its own producer fails is worse than no guard,
# because it reports a protection that was never applied. So the listing is
# written to a FILE, where the status belongs to the command that wrote it, and
# a producer failure is an `error` and not a pass -- the rule
# .github/scripts/changed-in-range.sh states at its own added-findings loop for
# the same reason. The regression test is at the foot of this file.
old_ledger_paths=''
tracked_paths="$(mktemp)"
trap 'rm -f -- "$tracked_paths"' EXIT
ls_files_status=0
git ls-files -z > "$tracked_paths" || ls_files_status=$?
if (( ls_files_status != 0 )); then
  error "C5 could not read this head's tracked paths: git ls-files -z exited $ls_files_status, so the old finding-ledger prefix is UNCHECKED; that is a failure and not a pass"
else
  while IFS= read -r -d '' path; do
    [[ "$path" == reviews/findings/* ]] || continue
    old_ledger_paths+="  $path"$'\n'
  done < "$tracked_paths"
fi
if [[ -n "$old_ledger_paths" ]]; then
  error "reviews/findings/ was moved to findings/ in pull request #276 (2026-09-12) and must not come back. This head tracks these paths under the old prefix:"
  error "${old_ledger_paths%$'\n'}"
  error "Rebase onto master and move them under findings/ with git mv: nothing reads reviews/findings/ any more, so a finding left there is filed nowhere."
fi

# --- C5's regression test: a failing producer must go RED, never green -------
# MUT-C5-PRODUCER-FAILS-OPEN is killed here rather than by hand, because that
# mutation was invisible to every fixture this repository had: the gate passed.
# The whole gate is re-run with GIT_DIR pointing at a path that cannot be a
# directory, so `git ls-files` exits 128 without anything touching the index
# this run is reading. C5 is the only check here that speaks to git, so the
# child's output is three lines and nothing else -- git's own `fatal:`, C5's
# refusal naming the status, and the FAIL line -- and that exactness is the part
# that pins "only C5 reads git" for whoever adds the next check. git's
# diagnostic is deliberately not silenced: C5's message carries the status and
# git's carries the reason, and the reader of a red gate wants both.
#
# THE CHILD IS TOLD NOT TO RECURSE BY ARGUMENT AND NOT BY ENVIRONMENT: a
# variable that suppresses a test is a variable a stale export suppresses it
# with, and nothing invokes this gate with arguments.
#
# Measured both ways on this file. With `done < <(git ls-files -z)` restored the
# child prints `documentation consistency fixtures: PASS`, exits 0, and these
# three checks all speak; with the status checked it exits 1 naming the producer.
if [[ "${1:-}" != --child-of-c5-selftest ]]; then
  selftest_status=0
  selftest_out="$(GIT_DIR=/dev/null/not-a-git-repository \
    "$BASH" "$script_dir/${BASH_SOURCE[0]##*/}" --child-of-c5-selftest 2>&1)" \
    || selftest_status=$?
  selftest_said="${selftest_out//$'\n'/ | }"
  (( selftest_status != 0 )) \
    || error "C5 passed with a failing git ls-files: the child run exited 0 and said: $selftest_said"
  [[ "$selftest_out" == *'git ls-files -z exited 128'* ]] \
    || error "C5 must name the producer that failed; the child run said: $selftest_said"
  [[ "$(grep -c . <<< "$selftest_out")" == 3 ]] \
    || error "a child run with no repository must say exactly three things -- git's fatal, C5's refusal, the FAIL line -- and fail no other check; it said: $selftest_said"
fi

if (( failed )); then
  echo "documentation consistency fixtures: FAIL" >&2
  exit 1
fi
echo "documentation consistency fixtures: PASS"
