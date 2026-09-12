#!/usr/bin/env bash
# Documentation and workflow-trigger claims that go stale silently, checked
# against the tree rather than against a hard-coded copy of it.
#
# THE CLAIMS THIS GATE ENFORCES -- exactly these, nothing else is in its scope:
#
#   C1  CLAUDE.md and CONTRIBUTING.md exist. Every repository path either names
#       in backticks exists at this head, or EACH occurrence is qualified within
#       its own window -- three lines before to four after, CLAMPED TO THE
#       OCCURRENCE'S OWN PARAGRAPH -- by one of the marker phrases below. A
#       paragraph ends at a line holding nothing but whitespace, the carriage
#       return included, so the line-ending convention a document is checked out
#       with never decides where its paragraphs end.
#       Qualification is syntactic within that paragraph: the gate checks that a
#       phrase is present in it, not what the phrase refers to, so a qualifier
#       still excuses every missing citation written beside it. What it can no
#       longer do is reach across a blank line into prose about something else.
#       And CLAUDE.md does not carry a sentence matching
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
#       directory only in case, never matches. AND THE LISTING MUST BOTH
#       SUCCEED AND BE READ TO ITS END: where `git ls-files` cannot answer, or
#       where its answer is not consumed whole, this check reports a failure and
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
#            could fail unnoticed, so the check it never ran read as a pass);
#   round 7: MUT-C5-CONSUMER-FAILS-OPEN (the producer's status checked and its
#            output never proved read, so a listing the loop failed to read was
#            indistinguishable from a clean one);
#   round 8: MUT-C1-QUALIFIER-CROSSES-A-PARAGRAPH (a fixed line window reached
#            past a blank line, so a qualifier excused a citation in a
#            neighbouring paragraph -- this file's own repair silenced C1);
#   round 9: MUT-C1-CRLF-DISSOLVES-A-PARAGRAPH (blankness was a field count, so
#            a document checked out with CRLF endings held no blank line at all,
#            round 8's clamp was inert, and the same qualifier excused the same
#            citation again -- one repair, silenced by a second route).
set -euo pipefail
export PATH="/usr/bin:/bin:$PATH"

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$script_dir/../.." && pwd)"
cd "$root"

# failed is a COUNT, not a flag: the FAIL line reports it, and C5's self-test
# below reads that number to assert which checks failed without counting the
# child's output lines -- see the fixture's own comment for why that matters.
failed=0
error() { echo "$*" >&2; failed=$(( failed + 1 )); }

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
# MUT-C1-QUALIFIER-CROSSES-A-PARAGRAPH: the window was three lines before to
# four after AND NOTHING ELSE, so it reached across a blank line into the
# neighbouring prose and a qualifier there excused a citation it was never
# written about. Measured on 38283eae: the trap paragraph this pull request added
# to CLAUDE.md ends its first sentence in the words "must not exist", four lines
# below the src/export.rs citation and separated from it by a blank line; with
# that citation changed to a path that does not exist, the base gate exited 1
# naming it and this head exited 0 printing PASS. A
# qualifier BELONGS TO THE PARAGRAPH IT IS WRITTEN IN, so the window is clamped
# to the citation's own paragraph -- blank line to blank line -- and can only
# ever SHRINK, never reach further than the ±3/+4 it already had. What is still
# syntactic: a qualifier excuses every missing citation in its OWN paragraph,
# because nothing here reads which path a phrase refers to.
marker='arrives with|arrive with|not yet|until that merges|until it merges|lands with|forward reference|\*\*no |there is \*\*?no|does not exist|must not exist'

# excused <doc> <line_no>: is the occurrence on that line qualified? This is the
# whole of C1's qualification decision, in one place, so the regression test at
# the foot asserts what C1 asks and not a restatement of it.
excused() {
  local doc="$1" line_no="$2" from to para_from para_to
  from=$(( line_no > 3 ? line_no - 3 : 1 ))
  to=$(( line_no + 4 ))
  # The paragraph the occurrence sits in: the line after the nearest blank line
  # above it, to the line before the nearest blank line below it. A document
  # with no blank line above or below is one paragraph.
  #
  # BLANK IS /^[[:space:]]*$/, IN BOTH PREDICATES, AND NOT `!NF`.
  # MUT-C1-CRLF-DISSOLVES-A-PARAGRAPH: `!NF` asks awk how many FIELDS a line
  # holds, and awk's default separator is blanks -- space and tab. A carriage
  # return is neither, so on a document checked out with CRLF endings every
  # line holds a field, NO line is blank, both predicates run off the ends of
  # the document, and round 8's clamp below is inert: para_from is 1, para_to
  # is the last line, and the +-3/+4 window reaches across a blank line into a
  # neighbouring paragraph exactly as it did before -- PR276-R4-002, reachable
  # by a second route. Measured on 8f0f203a, CLAUDE.md's src/export.rs citation
  # changed to a path that does not exist: LF exited 1 naming it, the same
  # document in CRLF exited 0 printing PASS.
  #
  # BOTH predicates carry it, not the one a report happens to hit: they are the
  # two ends of the same paragraph, and a fix to either alone still lets the
  # other end run to the edge of the document.
  #
  # `[[:space:]]` includes the carriage return, which is why the convention
  # stops deciding where paragraphs end. It is a SUPERSET of `!NF` and not a
  # different rule: measured over every whitespace class, no line `!NF` called
  # blank is a field here (empty, spaces, tabs, and mixtures of them all still
  # blank), and the only lines whose classification moves are ones a reader
  # already sees as empty -- a lone carriage return, whitespace around one, a
  # form feed, a vertical tab. Text is untouched, `  x` with a trailing
  # carriage return included.
  para_from=$(awk -v n="$line_no" 'NR < n && /^[[:space:]]*$/ { last = NR } END { print last + 1 }' "$doc")
  para_to=$(awk -v n="$line_no" 'NR > n && /^[[:space:]]*$/ { print NR - 1; found = 1; exit } END { if (!found) print NR }' "$doc")
  if (( from < para_from )); then from=$para_from; fi
  if (( to > para_to )); then to=$para_to; fi
  sed -n "${from},${to}p" "$doc" | grep -qiE "$marker"
}

for doc in CLAUDE.md CONTRIBUTING.md; do
  [[ -f "$doc" ]] || { error "$doc is missing: this gate requires it"; continue; }
  rooted=$(grep -oE '`(src|infra|\.github|acceptance|decisions|proposals|reviews|findings|examples|fixtures|docs)/[A-Za-z0-9_./-]*`' "$doc" | tr -d '`' || true)
  bare=$(grep -oE '`[A-Za-z0-9][A-Za-z0-9_.-]*\.(md|toml|lock)`' "$doc" | tr -d '`' || true)
  while IFS= read -r path; do
    [[ -z "$path" ]] && continue
    [[ -e "$path" ]] && continue
    while IFS= read -r line_no; do
      [[ -z "$line_no" ]] && continue
      excused "$doc" "$line_no" \
        || error "$doc:$line_no names \`$path\`, which does not exist at this head; this occurrence is neither marked as a forward reference nor stated as deliberately absent within its own paragraph"
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
# THE LISTING'S STATUS IS CHECKED BEFORE ITS OUTPUT IS BELIEVED, AND THE
# LISTING IS THEN PROVED TO HAVE BEEN READ TO ITS END.
#
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
# the same reason.
#
# MUT-C5-CONSUMER-FAILS-OPEN: and that status check is only HALF THE PIPE.
# CHECKING A PRODUCER'S STATUS DOES NOT ESTABLISH THAT ITS OUTPUT WAS CONSUMED:
# it answers "did the command succeed", and says nothing about whether what it
# wrote was read. `while ... done < "$tracked_paths"` TREATS A FAILED READ AS
# END OF FILE -- bash's read builtin reports EIO exactly as it reports EOF -- so
# the loop stopped, old_ledger_paths stayed empty, the `if` below was skipped
# again, and an UNREAD listing was indistinguishable from a clean one. A review
# lens injected read(2) EIO for this file alone (an LD_PRELOAD shim keyed on the
# temporary directory, the reviewed tree never edited): the gate printed
# `read error: Input/output error`, then `documentation consistency fixtures:
# PASS`, and exited 0. The FILE the producer fix introduced is what made that
# injection possible -- the process substitution it replaced had no intermediate
# file to fault -- so this hole arrived with that fix and not before it.
#
# THE REPAIR IS A COMPLETENESS CHECK AND NOT A THIRD STATUS CHECK. `-z`
# terminates every record with a NUL, so the number of records the producer
# wrote is a fact about the file: it is counted from the file BEFORE the loop
# runs, in its own status-checked command, and the loop must reach it. A SHORT
# READ IS AN `error` HERE AND NEVER AN EMPTY RESULT. Both regression tests are
# at the foot of this file.
old_ledger_paths=''
tracked_paths="$(mktemp)"
trap 'rm -f -- "$tracked_paths"' EXIT
ls_files_status=0
git ls-files -z > "$tracked_paths" || ls_files_status=$?
if (( ls_files_status != 0 )); then
  error "C5 could not read this head's tracked paths: git ls-files -z exited $ls_files_status, so the old finding-ledger prefix is UNCHECKED; that is a failure and not a pass"
else
  # One NUL per record is what the producer wrote, so this is the count the loop
  # below has to reach. It is taken from the file BEFORE the loop reads it, and
  # in its own right: `tr | wc` reads the whole file under `pipefail`, so a read
  # that fails here is a failed MEASUREMENT and not a count of zero. A failed
  # measurement is pinned to -1, a count no loop can reach, so the comparison
  # below REFUSES on its own even if the status check that names the failure is
  # ever dropped: the verdict fails closed structurally and the status check is
  # there to say why.
  count_status=0
  records_written="$(tr -dc '\000' < "$tracked_paths" | wc -c)" || { count_status=$?; records_written=-1; }
  # The consumer fixture's fault, injected BY ARGUMENT AND NEVER BY ENVIRONMENT
  # and only between the count and the loop: an emptied listing is what a read
  # that fails at its first byte looks like from where the loop stands.
  if [[ "${1:-}" == --child-of-c5-selftest && "${2:-}" == --lose-the-listing-after-counting-it ]]; then
    : > "$tracked_paths"
  fi
  records_parsed=0
  while IFS= read -r -d '' path; do
    records_parsed=$(( records_parsed + 1 ))
    [[ "$path" == reviews/findings/* ]] || continue
    old_ledger_paths+="  $path"$'\n'
  done < "$tracked_paths"
  if (( count_status != 0 )); then
    error "C5 could not measure this head's tracked-path listing: counting its NUL-terminated records exited $count_status, so the old finding-ledger prefix is UNCHECKED; that is a failure and not a pass"
  elif (( records_parsed != records_written )); then
    error "C5 read $records_parsed of the $records_written records git ls-files -z wrote: the listing was not consumed to its end, so the old finding-ledger prefix is UNCHECKED. A short read is an error here and never an empty result"
  fi
fi
if [[ -n "$old_ledger_paths" ]]; then
  error "reviews/findings/ was moved to findings/ in pull request #276 (2026-09-12) and must not come back. This head tracks these paths under the old prefix:"
  error "${old_ledger_paths%$'\n'}"
  error "Rebase onto master and move them under findings/ with git mv: nothing reads reviews/findings/ any more, so a finding left there is filed nowhere."
fi

# --- C1's regression test: a qualifier belongs to its own paragraph, and the
# --- line-ending convention does not move the paragraph ----------------------
# MUT-C1-QUALIFIER-CROSSES-A-PARAGRAPH and MUT-C1-CRLF-DISSOLVES-A-PARAGRAPH are
# killed here rather than by hand. Both were invisible to every fixture this
# repository had, because the only documents C1 reads are CLAUDE.md and
# CONTRIBUTING.md and both must keep passing: the weakening shows only when a
# citation in one of them is made to dangle, which a gate cannot do to its own
# repository. `excused` is the whole of the decision, so a fixture document pins
# it.
#
# EVERY ROW RUNS TWICE, ON THE SAME DOCUMENT IN BOTH CONVENTIONS, AND EXPECTS
# THE SAME ANSWER EITHER WAY. Round 8's fixture was LF-only, which is how a
# CRLF document walked past this file's own regression test without failing it.
# What is asserted is therefore the property -- the convention a document is
# written in does not change which lines count as blank -- as an equality across
# the two columns, and not one more case bolted beside the last one.
#
# WHAT WOULD STILL PASS IF THE CLAMP WERE GONE: rows 1 and 2 in the LF column,
# and only those -- they are the rows that need a blank line to be honoured.
# Rows 3 and 4 hold either way and are here so the clamp cannot be "fixed" by
# narrowing the window to the citation's own line, which would refuse a
# qualification that merely wrapped. Row 5 is the other bound: inside one
# paragraph the +-3/+4 window still applies, so clamping widened nothing.
# WHAT WOULD STILL PASS IF THE BLANKNESS PREDICATES WERE A FIELD COUNT AGAIN:
# nothing in the CRLF column, and row 6 in neither column. Row 6 is the blank
# line that is not empty -- a separator holding whitespace AROUND a stray
# carriage return, which `!NF` saw as a field and which a trailing-only strip
# (`sed 's/\r$//'`) would still see as one. It is appended with printf because
# the here-document above is quoted, and the carriage return has to be a byte.
if [[ "${1:-}" != --child-of-c5-selftest ]]; then
  c1_doc="$(mktemp)"
  c1_doc_crlf="$(mktemp)"
  cat > "$c1_doc" <<'C1FIXTURE'
**`src/a-no-qualifier.rs` is cited here.** Nothing in this paragraph
qualifies it.

**`other/thing.md` must not exist.** A separate paragraph, whose qualifier
belongs to the citation on this line and to no other.

**`src/b-below-a-qualifier.rs` is cited here.** The marker three lines above
belongs to the paragraph above and not to this one.

**`src/c-wrapped.rs` is cited here.** The qualifier for it
does not exist yet, and it is written on the next line of the same paragraph.

**`src/d-same-line.rs` does not exist.** One line, one paragraph.

A paragraph that does not exist as a qualifier for the citation below it.
Line two.
Line three.
Line four.
Line five cites `src/e-too-far.rs`.
C1FIXTURE
  printf '%s\n' '' \
    '**`src/f-stray-cr.rs` is cited here.** The separator below holds' \
    'whitespace around a carriage return, and it still ends this paragraph.' \
    >> "$c1_doc"
  printf ' \r \n' >> "$c1_doc"
  printf '%s\n' 'A neighbouring paragraph, in which the file does not exist.' >> "$c1_doc"
  # The same document, every line ending CRLF. Stripping first keeps row 6's
  # interior carriage return the only one that is not a line ending.
  sed -e 's/\r$//' -e 's/$/\r/' "$c1_doc" > "$c1_doc_crlf"
  c1_case() {  # c1_case <line> <yes|no> <what>
    local line_no="$1" want="$2" what="$3" convention doc got
    for convention in LF CRLF; do
      case "$convention" in
        LF) doc="$c1_doc" ;;
        *)  doc="$c1_doc_crlf" ;;
      esac
      got=no
      excused "$doc" "$line_no" && got=yes
      [[ "$got" == "$want" ]] \
        || error "C1 qualification, $convention, line $line_no ($what): excused=$got, and excused=$want was expected"
    done
  }
  c1_case 1  no  'a marker in the paragraph BELOW must not excuse it'
  c1_case 7  no  'a marker in the paragraph ABOVE must not excuse it'
  c1_case 10 yes 'a marker on the next line of the same paragraph excuses it'
  c1_case 13 yes 'a marker on the occurrence own line excuses it'
  c1_case 19 no  'a marker four lines above, inside one paragraph, is still out of the window'
  c1_case 21 no  'whitespace around a carriage return is a blank line, so the marker below it is in the next paragraph'
  rm -f "$c1_doc" "$c1_doc_crlf"
fi

# --- C5's regression tests: a listing that fails must go RED, never green ----
# MUT-C5-PRODUCER-FAILS-OPEN and MUT-C5-CONSUMER-FAILS-OPEN are killed here
# rather than by hand, because both mutations were invisible to every fixture
# this repository had: the gate passed.
#
# THE PRODUCER CHILD re-runs the whole gate with GIT_DIR pointing at a path that
# cannot be a directory, so `git ls-files` exits 128 without anything touching
# the index this run is reading.
#
# THE CONSUMER CHILD re-runs the whole gate against THIS repository, where git
# works and C5 would otherwise pass, and empties the listing between the count
# and the loop -- what a read failing at its first byte looks like from the
# loop. With the record comparison deleted that child prints `documentation
# consistency fixtures: PASS` and exits 0; with it, it exits 1 naming the short
# read. A truncated file is a stand-in for the observable effect of a failed
# read(2), not for the syscall: the syscall itself was reproduced by a review
# lens with an LD_PRELOAD shim, which is not something a gate can carry.
#
# WHAT IS ASSERTED IS OUTCOMES, NOT A COUNT OF THE CHILD'S OUTPUT LINES. The
# first version of this fixture required the producer child to say exactly three
# things. That pinned a property worth keeping -- C5 is the only check here that
# speaks to git, so nothing else can fail when git does -- but it pinned it
# through git's diagnostics as well as this gate's, and git has more to say when
# it is asked to: under GIT_TRACE2=1 the child said NINE things rather than
# three (measured on this box, git 2.43.0), so anyone who set that variable to
# investigate a gate got a failure caused by their own tracing -- the gate
# exited 1 under GIT_TRACE2=1 and 0 without it, on a tree where nothing else
# had changed. The property now rides on the error count the FAIL line reports
# instead -- exactly one error is C5 refusing and no other check failing --
# which no amount of tracing, and no wording of git's own messages, can move.
#
# git's diagnostic is still required and still deliberately not silenced: C5's
# message carries the status and git's carries the reason, and the reader of a
# red gate wants both. What is matched is the GIT_DIR value inside git's
# message, which no check here prints; the literal `fatal:` is NOT matched,
# because git translates that prefix where message catalogues are installed and
# the fixture would then fail on the reader's locale -- the same class of
# defect as the line count.
#
# THE CHILDREN ARE TOLD WHAT TO DO BY ARGUMENT AND NOT BY ENVIRONMENT: a
# variable that suppresses a test, or injects a fault, is a variable a stale
# export suppresses or injects with, and nothing invokes this gate with
# arguments.
if [[ "${1:-}" != --child-of-c5-selftest ]]; then
  self="$script_dir/${BASH_SOURCE[0]##*/}"
  only_c5_failed='documentation consistency fixtures: FAIL (errors: 1)'

  producer_status=0
  producer_out="$(GIT_DIR=/dev/null/not-a-git-repository \
    "$BASH" "$self" --child-of-c5-selftest 2>&1)" \
    || producer_status=$?
  producer_said="${producer_out//$'\n'/ | }"
  (( producer_status != 0 )) \
    || error "C5 passed with a failing git ls-files: the producer child exited 0 and said: $producer_said"
  [[ "$producer_out" == *'git ls-files -z exited 128'* ]] \
    || error "C5 must name the producer that failed; the producer child said: $producer_said"
  [[ "$producer_out" == *'/dev/null/not-a-git-repository'* ]] \
    || error "git's own diagnostic must reach the reader of a red gate, naming the repository it could not open; the producer child said: $producer_said"
  [[ "$producer_out" == *"$only_c5_failed"* ]] \
    || error "a child run with no repository must fail C5 and no other check, because C5 is the only check here that speaks to git; the producer child said: $producer_said"

  consumer_status=0
  consumer_out="$("$BASH" "$self" --child-of-c5-selftest --lose-the-listing-after-counting-it 2>&1)" \
    || consumer_status=$?
  consumer_said="${consumer_out//$'\n'/ | }"
  (( consumer_status != 0 )) \
    || error "C5 passed with a listing it never read: the consumer child exited 0 and said: $consumer_said"
  [[ "$consumer_out" == *'was not consumed to its end'* ]] \
    || error "C5 must name the short read; the consumer child said: $consumer_said"
  [[ "$consumer_out" == *"$only_c5_failed"* ]] \
    || error "a child run whose listing went unread must fail C5 and no other check; the consumer child said: $consumer_said"
fi

if (( failed )); then
  echo "documentation consistency fixtures: FAIL (errors: $failed)" >&2
  exit 1
fi
echo "documentation consistency fixtures: PASS"
