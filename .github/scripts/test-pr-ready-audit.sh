#!/usr/bin/env bash
# The pure parts of scripts/pr-ready-audit.sh, exercised against fixtures: the lane and its
# severity set, the two review parsers (the workflow's fenced JSON verdict and the frontier
# prose form), the frontmatter id match, the newest-check-run choice and the ledger-row parse.
# Everything that talks to GitHub is out of scope here, with one exception: the argument parser and
# the audit's own refusals are exercised by running the script as a process against a stub `gh`,
# because the defects they have had live in `main` and no call on the helpers can see them. git is
# in scope in one place, `finding_file_count`, which is run against a small repository this file
# builds: reading a list of names is where a shape rule stops helping and only a count will do. The stub answers every call
# with a marker and a failure, so an argument that should have been refused and instead reached
# GitHub shows up as a failed case rather than a live request. The audit's behaviour on a real
# pull request is still observed on the pull request.
#
# Needs `jq` as well as bash: the comment filter is a jq program, and running it is the only way
# to prove the trusted login reaches it as data rather than as program text.
#
# Each case names the defect it exists to catch, so a green run says what it proved:
#   MUT-LANE-LABEL-INPUT         the lane came from a label, not the branch prefix
#   MUT-P3-LANE-DEFERS           the P3 lane let a P3 be deferred
#   MUT-JSON-SPLIT-BY-REGEX      findings read by splitting text on "},{" instead of a parser
#   MUT-NULL-WITNESS             a null witness field counted as a witness
#   MUT-MUST-UNSEEN              a MUST deviation was not flagged
#   MUT-BAD-SEVERITY-PASSES      a P9 finding passed as deferrable
#   MUT-STRAY-TOKEN-UNSEEN       a severity token outside the verdict object was ignored
#   MUT-VERDICT-FROM-PROSE       the verdict was read from prose, not the object with the findings
#   MUT-QUOTED-JSON-IS-JSON      a prose review quoting JSON was parsed as the JSON form
#   MUT-PROSE-HEADING-FINDING    a prose P1 written as a heading vanished
#   MUT-PROSE-LAST-VERDICT       the first VERDICT line won over the last
#   MUT-FRONTMATTER-BY-SUBSTRING an id in prose satisfied the frontmatter match
#   MUT-FRONTMATTER-PATTERN      an id with a dot matched as a regex
#   MUT-CHECK-RUN-FIRST-SUCCESS  an older success outranked a newer failure
#   MUT-CHECK-RUN-UNSTARTED      an unstarted run was ordered by a stand-in date, not its id
#   MUT-LEDGER-HEADER-AS-ROW     the header or separator line parsed as a row
#   MUT-REVIEWER-FROM-ORG        an organization owner was trusted as the reviewer, so the
#                                comment filter matched nothing and every pull request read
#                                no-review
#   MUT-REVIEWER-OVERRIDE-IGNORED  an explicit --reviewer was not preferred over the owner
#   MUT-REVIEWER-JQ-INJECTION    the trusted login was spliced into the comment filter's program
#                                text, so a value shaped like jq widened the author predicate to
#                                every commenter and another account's PASS became the verdict
#   MUT-REVIEWER-CASE-MISMATCH   the author comparison was case-sensitive, so naming the correct
#                                account in another spelling hid its blocking review as no-review
#   MUT-REVIEWER-ARG-EATEN       --reviewer read the next argument without checking it was one, so
#                                a following option was consumed as the login and switched off
#   MUT-REVIEWER-NULL-AUTHOR     the comment filter raised on a comment whose author is null, which
#                                failed the whole page it was on and dropped every review with it
#   MUT-REVIEW-LOOKUP-SUPPRESSED a failed comment lookup was reported as "no review", so the audit
#                                judged whatever survived the failure and an older PASS could win
#   MUT-TIMELINE-LOOKUP-SUPPRESSED  a failed timeline lookup was reported as "the base did not
#                                change", which is the check standing between a retarget and an
#                                enqueue
#   MUT-RULESET-LOOKUP-SUPPRESSED   a failed ruleset lookup was reported as "no ruleset requires an
#                                up-to-date branch", which drops the BEHIND blocker for the run
#   MUT-LOGIN-LOCALE-RANGE       the login check used a collating range, so what counted as an
#                                ASCII letter came from the locale and `evéntloops` was a login
#   MUT-REVIEWER-ENV-BEFORE-FLAG an inherited environment value was validated before the flag that
#                                replaces it, so a stale setting refused a run that never used it
#   MUT-REVIEWER-EMPTY-OVERRIDE  an empty --reviewer counted as no --reviewer, so an unset shell
#                                variable expanded into the flag moved trust to the repository owner
#   MUT-LIST-UNPAGINATED         a collection was read without --paginate, so page one was taken
#                                for the whole list and an active ruleset on page two was invisible
#   MUT-OPTION-LONE-HYPHEN       the option guard required a character after the hyphen, so a lone
#                                `-` was taken as a label name
#   MUT-REVIEW-PARSE-TRUNCATED   a parser that died partway printed a shorter findings list, and a
#                                shorter findings list is a weaker verdict, not an error
#   MUT-META-FIELD-COLLAPSE      an empty META field let `read` with IFS=tab shift every column
#                                after it, so a review recording no commit read as one that did
#   MUT-PR-LOOKUP-SUPPRESSED     a failed pull-request read left empty fields to be audited
#   MUT-PR-LIST-SUPPRESSED       a failed open-pull-request listing read as "nothing is open"
#   MUT-FAIL-OPEN-SHAPE          a read whose failure cannot be seen: `for x in $(gh ...)`,
#                                `< <(gh ...)`, `|| true` on a gh or git command, or a gh or git
#                                command piped where the pipeline's status is read as an answer
#   MUT-FINDING-FILE-COUNT       the filed-finding count read a listing wrongly -- an error taken
#                                for "no files", or NUL-separated names put through `$(...)`,
#                                which drops NUL and leaves nothing to read
#   MUT-FINDING-FILE-READ-AS-MISS  a finding file the count could not read counted as a file that
#                                does not carry the id, so two files filing one id became one
#   MUT-REVIEW-FORGES-PROTOCOL   a review string carrying the parser's own separators wrote rows
#                                of the parser's language, END among them, and the audit read a
#                                finished parse from a marker instead of from a status
#   MUT-PROSE-READ-SUPPRESSED    a read that failed inside the prose parser was reported as
#                                "nothing matched", and END was printed over it
#   MUT-FRONTMATTER-UPSTREAM-ERROR-AS-MISS  the frontmatter read was a pipeline, so under
#                                `pipefail` the reader's failure (2) stood behind the matcher's
#                                ordinary "no match" (1) and the caller was handed an answer
#   MUT-PROSE-VERDICT-SUFFIX     the verdict check was anchored at one end, so it validated the
#                                TAIL of the token and read `VERDICT: FAIL<U+00C9>PASS` as PASS --
#                                and read it differently under `C`, in a file whose header
#                                forbids locale-dependent output
#   MUT-PROSE-HEAD-NOT-FIRST     the head check searched the whole listing instead of its first
#                                line, so a first marker it could not read was stepped over and a
#                                later line's commit came back as the reviewed head
#   MUT-FINDING-SYMLINK-AS-FILE  the candidate listing dropped the mode git records, so a
#                                `120000` symlink was read as a file and its TARGET STRING as
#                                that file's frontmatter
#   MUT-PROSE-VERDICT-SALVAGE    a verdict token that failed the whole-token check was handed to
#                                a fallback that stripped the leading colons and asterisks off
#                                it, so `VERDICT: ::PASS` -- rejected one line earlier -- came
#                                back out of the rejecting branch as `PASS`
#   MUT-FINDING-BLOB-OPEN-AS-MISS  the blob the matcher was to read could not be OPENED, so the
#                                redirection returned 1 with the matcher never run -- and 1 was
#                                the matcher's own word for "does not carry the id"
#   MUT-FINDING-LIST-SHORT-READ  the candidate listing stopped arriving part way through and the
#                                entries that had arrived were counted as all of them: `read`
#                                ends a loop at end of input and at a failed read alike
#   MUT-REVIEW-KIND-UNREADABLE-IS-PROSE  a format detection that could not read the file chose
#                                the prose parser, which reads a JSON review's `VERDICT:` line
#                                from outside its verdict object and misses every severity the
#                                object spells with a JSON escape
#   MUT-HERESTRING-FAILURE-AS-ANSWER  a value was fed to a command through `<<<`, which spills to
#                                a temporary file once it outgrows a pipe buffer: a file bash
#                                cannot create is a redirection that failed, the command never
#                                runs, and the shell returns 1 -- `grep`'s "no match". A compound
#                                command is skipped outright, where neither a captured status nor
#                                `set -e` can see it
set -euo pipefail
export PATH="/usr/bin:/bin:$PATH"
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$script_dir/../.." && pwd)"
cd "$root"
failed=0
error() { echo "$*" >&2; failed=1; }
expect() {  # expect <case> <got> <want>
  [[ "$2" == "$3" ]] || error "$1: got [$2], want [$3]"
}
contains() {  # contains <case> <got> <want-substring>
  [[ "$2" == *"$3"* ]] || error "$1: got [$2], want it to contain [$3]"
}
command -v jq > /dev/null || { echo "test-pr-ready-audit: needs jq to run the comment filter" >&2; exit 1; }

PR_READY_AUDIT_LIBRARY=1 source scripts/pr-ready-audit.sh
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# --- lanes and severity sets ------------------------------------------------------------------
expect MUT-LANE-LABEL-INPUT "$(lane_for codex/findings-p3-7d2d8e9dc74a)" findings-p3
expect MUT-LANE-LABEL-INPUT "$(lane_for codex/findings-ba8fdc8dec2b)" findings-p1p2
expect MUT-LANE-LABEL-INPUT "$(lane_for codex/sweep-f3eb59a749fe)" feature
expect MUT-LANE-LABEL-INPUT "$(lane_for fix/sampler-kill-and-inspection)" feature
expect MUT-P3-LANE-DEFERS "$(must_fix_for findings-p3)" "P0 P1 P2 P3"
expect MUT-P3-LANE-DEFERS "$(must_fix_for findings-p1p2)" "P0 P1 P2"
expect MUT-P3-LANE-DEFERS "$(must_fix_for feature)" "P0 P1"

# --- whose review counts ------------------------------------------------------------------------
# A User owner stands in for the reviewer; that is the pre-organization behaviour and it stays.
expect MUT-REVIEWER-FROM-ORG "$(reviewer_login absent "" eventloops User)" eventloops
# An Organization owner authors no comments. Inheriting it yields a filter that matches nothing,
# which is not a stricter audit but a blind one, so it must fail rather than return a login.
if reviewer_login absent "" sourcemaps Organization > "$tmp/org.out" 2>&1; then
  error "MUT-REVIEWER-FROM-ORG: an Organization owner was accepted as the reviewer, got [$(cat "$tmp/org.out")]"
fi
expect MUT-REVIEWER-FROM-ORG "$(cat "$tmp/org.out")" ""
# An explicit override wins over either, and is the only way to audit an organization-owned repo.
expect MUT-REVIEWER-OVERRIDE-IGNORED "$(reviewer_login given eventloops sourcemaps Organization)" eventloops
expect MUT-REVIEWER-OVERRIDE-IGNORED "$(reviewer_login given someone-else eventloops User)" someone-else
# A missing login is not a reviewer either, whatever the type claims.
if reviewer_login absent "" "" User > "$tmp/empty.out" 2>&1; then
  error "MUT-REVIEWER-FROM-ORG: an empty owner login was accepted as the reviewer"
fi
# An override that was supplied and is empty is not an override that was not supplied. Reading the
# two as one is how `--reviewer ""` -- an unset shell variable expanded into the flag -- handed a
# User-owned repository's audit to its owner, over the reviewer the caller had named, and enqueued
# a pull request that reviewer had blocked. `given` is checked whatever it carries.
if reviewer_login given "" eventloops User > "$tmp/given-empty.out" 2>&1; then
  error "MUT-REVIEWER-EMPTY-OVERRIDE: an empty --reviewer fell back to the owner, got [$(cat "$tmp/given-empty.out")]"
fi
for bad_state in "" absent-ish 0 1 yes; do
  if reviewer_login "$bad_state" "" eventloops User > "$tmp/state.out" 2>&1; then
    error "MUT-REVIEWER-EMPTY-OVERRIDE: state [$bad_state] was treated as an answer, got [$(cat "$tmp/state.out")]"
  fi
done
# and `absent` still means absent: the owner stands in exactly where it did before.
expect MUT-REVIEWER-EMPTY-OVERRIDE "$(reviewer_login absent "" eventloops User)" eventloops
# The helper is the last gate before a string becomes the audit's notion of who may say PASS, and
# what it returns is put to a jq program, so it takes GitHub's login shape and nothing wider. Each
# of these is a typo, another option read by mistake, or an injection attempt; none is an account.
for bad in 'eventloops" or true or .user.login == "eventloops' '--enqueue' '-abc' 'abc-' 'a--b' \
           'github-actions[bot]' 'two words' 'a.b' 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'; do
  if reviewer_login given "$bad" eventloops User > "$tmp/bad.out" 2>&1; then
    error "MUT-REVIEWER-JQ-INJECTION: [$bad] was accepted as a login, got [$(cat "$tmp/bad.out")]"
  fi
done
# and nothing narrower: a real login, in any case, of any allowed length, is not refused.
for good in eventloops EventLoops a-b-c x 0 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa; do
  expect MUT-REVIEWER-JQ-INJECTION "$(reviewer_login given "$good" sourcemaps Organization)" "$good"
done

# --- the login shape means the same thing in every locale ---------------------------------------
# A range inside a bash regex is resolved by the locale's collation: under en_US.utf8 `[A-Za-z0-9]`
# admits `é` and U+212A KELVIN SIGN, so `--reviewer evéntloops` passed the check that exists to
# stop it and reached the API as a login nobody has. Every fixture is run under every locale this
# machine offers and each must give the same answer under all of them. The non-ASCII fixtures are
# written as byte escapes rather than as literal characters, because what is under test is which
# bytes the check accepts, and a literal would depend on how this file is encoded and read.
locales=(C)
for loc in C.UTF-8 C.utf8 en_US.UTF-8 en_US.utf8; do
  locale -a 2>/dev/null | grep -qxF "$loc" && locales+=("$loc")
done
for loc in "${locales[@]}"; do
  for good in eventloops EventLoops a-b-c x 0 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa; do
    LC_ALL="$loc" valid_login "$good" \
      || error "MUT-LOGIN-LOCALE-RANGE: [$good] is a login but LC_ALL=$loc refused it"
  done
  # ev<U+00E9>ntloops, <U+00E9>, <U+212A> alone and leading a word. None is a GitHub login; each is
  # inside `[A-Za-z0-9]` under a collation that is not C's.
  for bad in 'ev\xc3\xa9ntloops' '\xc3\xa9' '\xe2\x84\xaa' '\xe2\x84\xaavin' 'a\xc2\xa0b'; do
    if LC_ALL="$loc" valid_login "$(printf '%b' "$bad")"; then
      error "MUT-LOGIN-LOCALE-RANGE: [$bad] was accepted as a login under LC_ALL=$loc"
    fi
  done
done
# On a runner whose only locales are C and C.UTF-8 the defect does not reproduce -- under those two
# collations a range *is* ASCII -- so the fixtures above would pass against the unrepaired check
# there. This is the part of the guard that binds everywhere: the classes must be written out,
# because writing them as a range is what makes the answer the locale's to give.
if declare -f valid_login | grep -qE '\[[^]]*[A-Za-z0-9]-[A-Za-z0-9]'; then
  error "MUT-LOGIN-LOCALE-RANGE: valid_login matches with a collating range, whose meaning is the locale's, not ASCII"
fi

# --- the comment filter: the login is data, and spelling is not identity -------------------------
# The program the audit hands to `gh api --jq`, run here on a fixture by the jq on this machine.
# `gh` embeds its own jq, so this is a stand-in for that engine, not the engine itself; both read
# `env` and `ascii_downcase` the same way, and the audit's behaviour against the real API is
# observed on the pull request.
cat > "$tmp/comments.json" <<'EOF'
[{"created_at":"2026-09-01T00:00:00Z","id":1001,"user":{"login":"eventloops"},
  "body":"<!-- upstroke-frontier-review pr=1 head=x -->\nVERDICT: CHANGES_REQUIRED"},
 {"created_at":"2026-09-02T00:00:00Z","id":1002,"user":{"login":"a-contributor"},
  "body":"<!-- upstroke-frontier-review pr=1 head=x -->\nVERDICT: PASS"},
 {"created_at":"2026-09-03T00:00:00Z","id":1003,"user":{"login":"eventloops"},
  "body":"a comment carrying no review marker"}]
EOF
# Exported, not passed as a command prefix: a prefix is applied after the command's words are
# expanded, so a filter that went back to building the login into its text would read an empty
# variable here and look innocent. Exporting puts the value where both a splice and a lookup see it.
filter_as() {  # filter_as LOGIN: what the audit's comment filter selects from the fixture
  export UPSTROKE_AUDIT_REVIEWER="$1"
  jq -r "$(review_comment_filter)" "$tmp/comments.json"
}
expect MUT-REVIEWER-JQ-INJECTION "$(filter_as eventloops)" "2026-09-01T00:00:00Z 1001"
expect MUT-REVIEWER-JQ-INJECTION "$(filter_as a-contributor)" "2026-09-02T00:00:00Z 1002"
# Spliced into the program text this predicate is unconditionally true and the newest comment by
# anyone wins, which is another account's PASS. Compared as data it is a login nobody has.
expect MUT-REVIEWER-JQ-INJECTION \
  "$(filter_as 'eventloops" or true or .user.login == "eventloops')" ""
# GitHub resolves EventLoops and eventloops to one account, so the filter must too: the spelling
# is not the identity, and reading it as one turns a visible blocking review into `no-review`.
expect MUT-REVIEWER-CASE-MISMATCH "$(filter_as EventLoops)" "2026-09-01T00:00:00Z 1001"
expect MUT-REVIEWER-CASE-MISMATCH "$(filter_as EVENTLOOPS)" "2026-09-01T00:00:00Z 1001"
# Case-insensitivity widens the spelling of one account, never the set of accounts.
expect MUT-REVIEWER-CASE-MISMATCH "$(filter_as someone-else)" ""

# GitHub's REST schema lets an issue comment's `user` be null -- the account was deleted -- and a
# filter that lowers it raises. `--paginate` runs this program once per page, so raising on one
# unrelated comment loses every review on the page with it, and the newest review is exactly what
# a page can be holding. `body` is guarded on the same terms. Each of these is dropped as "not
# this reviewer's review"; nothing about the fetch's own health is inferred from them.
cat > "$tmp/nullable.json" <<'EOF'
[{"created_at":"2026-09-01T00:00:00Z","id":2001,"user":{"login":"eventloops"},
  "body":"<!-- upstroke-frontier-review pr=1 head=x -->\nVERDICT: CHANGES_REQUIRED"},
 {"created_at":"2026-09-02T00:00:00Z","id":2002,"user":null,
  "body":"<!-- upstroke-frontier-review pr=1 head=x -->\nVERDICT: PASS"},
 {"created_at":"2026-09-03T00:00:00Z","id":2003,"user":{"login":"eventloops"},"body":null},
 {"created_at":"2026-09-04T00:00:00Z","id":2004,"user":{},"body":"no login key at all"},
 {"created_at":"2026-09-05T00:00:00Z","id":2005,"user":{"login":null},"body":"a null login"}]
EOF
filter_nullable_as() {
  export UPSTROKE_AUDIT_REVIEWER="$1"
  jq -r "$(review_comment_filter)" "$tmp/nullable.json"
}
# The blocking review is still found, and jq does not raise: raising is what cost the page.
expect MUT-REVIEWER-NULL-AUTHOR "$(filter_nullable_as eventloops 2>&1)" "2026-09-01T00:00:00Z 2001"
if ! filter_nullable_as eventloops > /dev/null 2>&1; then
  error "MUT-REVIEWER-NULL-AUTHOR: the filter failed on a page holding a comment with a null author"
fi
# A deleted account is not an author to match against: an empty reviewer must not select it. No
# valid login is empty, which is the point -- the last defect here also came through a value that
# could not happen.
expect MUT-REVIEWER-NULL-AUTHOR "$(filter_nullable_as "" 2>&1)" ""
unset UPSTROKE_AUDIT_REVIEWER

# --- a lookup that failed is not a lookup that found nothing ------------------------------------
# Three helpers ask GitHub a question whose empty answer relaxes a blocker. Each used to give that
# empty answer when the request itself failed: the comment lookup by ending in `return 0`, the
# other two by `for x in $(gh api ...)`, which iterates zero times either way. A pull request whose
# reviewer had blocked it enqueued on an older PASS because of the first. The stub `gh` here fails
# every call, and each helper must report that rather than answer.
failing="$tmp/failing-gh"
mkdir -p "$failing"
printf '#!/usr/bin/env bash\necho "GH-FAILED $*" >&2\nexit 1\n' > "$failing/gh"
chmod +x "$failing/gh"
silent="$tmp/silent-gh"
mkdir -p "$silent"
printf '#!/usr/bin/env bash\nexit 0\n' > "$silent/gh"        # succeeds, finds nothing
chmod +x "$silent/gh"
timeline="$tmp/timeline-gh"
mkdir -p "$timeline"
printf '#!/usr/bin/env bash\necho 2026-09-05T00:00:00Z\n' > "$timeline/gh"
chmod +x "$timeline/gh"

if (PATH="$failing:$PATH"; repo=o/r; reviewer=eventloops; latest_review_id 999) > "$tmp/lr.out" 2>&1; then
  error "MUT-REVIEW-LOOKUP-SUPPRESSED: a failed comment lookup reported success, got [$(cat "$tmp/lr.out")]"
fi
# and the other direction, or a helper that always fails would pass the case above: a lookup that
# completed and found no review is the ordinary `no-review` answer, not a failure.
if ! got="$( (PATH="$silent:$PATH"; repo=o/r; reviewer=eventloops; latest_review_id 999) 2>&1 )"; then
  error "MUT-REVIEW-LOOKUP-SUPPRESSED: a completed lookup with no review reported failure"
fi
expect MUT-REVIEW-LOOKUP-SUPPRESSED "$got" ""

if (PATH="$failing:$PATH"; repo=o/r; base_changed_after 999 2026-09-01T00:00:00Z) > "$tmp/bc.out" 2>&1; then
  error "MUT-TIMELINE-LOOKUP-SUPPRESSED: an unreadable timeline was answered, got [$(cat "$tmp/bc.out")]"
fi
expect MUT-TIMELINE-LOOKUP-SUPPRESSED \
  "$( (PATH="$timeline:$PATH"; repo=o/r; base_changed_after 999 2026-09-01T00:00:00Z) )" yes
expect MUT-TIMELINE-LOOKUP-SUPPRESSED \
  "$( (PATH="$timeline:$PATH"; repo=o/r; base_changed_after 999 2026-09-09T00:00:00Z) )" no
expect MUT-TIMELINE-LOOKUP-SUPPRESSED \
  "$( (PATH="$silent:$PATH"; repo=o/r; base_changed_after 999 2026-09-01T00:00:00Z) )" no

if (PATH="$failing:$PATH"; repo=o/r; ruleset_state) > "$tmp/rs.out" 2>&1; then
  error "MUT-RULESET-LOOKUP-SUPPRESSED: an unreadable ruleset list was answered, got [$(cat "$tmp/rs.out")]"
fi
expect MUT-RULESET-LOOKUP-SUPPRESSED "$( (PATH="$silent:$PATH"; repo=o/r; ruleset_state) )" "0 0"

# --- a list is not complete until the API says it is --------------------------------------------
# GitHub pages every collection and defaults this endpoint to 30. Thirty rulesets in any state hid
# an active strict one on page two: the audit read "nothing requires an up-to-date branch", a
# BEHIND pull request with a clean review went READY, and merge was called. A page-two HTTP 500
# gave the identical answer, because rows never asked for and rows that failed to arrive are the
# same missing rows. The stub records what was asked.
recording="$tmp/recording-gh"
mkdir -p "$recording"
printf '#!/usr/bin/env bash\nprintf "%%s\\n" "$*" >> "$GH_RECORD"\nexit 0\n' > "$recording/gh"
chmod +x "$recording/gh"
( export GH_RECORD="$tmp/asked.txt"; export PATH="$recording:$PATH"; repo=o/r; ruleset_state ) > /dev/null
contains MUT-LIST-UNPAGINATED "$(cat "$tmp/asked.txt")" "--paginate"

# The class, not the instance. Every `gh api` call in the script reads a collection unless it is
# one of the three that read a single object by id, and a collection read without --paginate is
# page one mistaken for the whole. This is the guard that catches the next endpoint rather than
# this one; the three exceptions are named so adding a fourth is a decision, not an omission.
join_continuations() {
  awk '{
    if (buf != "") { sub(/^[[:space:]]+/, ""); buf = buf " " $0 } else { buf = $0 }
    if (buf ~ /\\$/) { sub(/\\$/, "", buf); next }
    print buf; buf = ""
  } END { if (buf != "") print buf }'
}
code_lines() { grep -vE '^[[:space:]]*#' scripts/pr-ready-audit.sh; }
while IFS= read -r call; do
  [[ "$call" == *"gh api "* ]] || continue
  [[ "$call" == *"--paginate"* ]] && continue
  case "$call" in
    *'gh api "repos/$repo" '*) continue ;;                              # the repository object
    *'gh api "repos/$repo/rulesets/$id" '*) continue ;;                 # one ruleset by id
    *'gh api "repos/$repo/issues/comments/$review_id" '*) continue ;;   # one comment by id
  esac
  error "MUT-LIST-UNPAGINATED: a collection is read without --paginate: [$call]"
done < <(code_lines | join_continuations)

# --- reads whose failure nothing can see --------------------------------------------------------
# Three shapes swallow the status of the command that produced the data, and each has cost this
# script a false READY: `for x in $(gh ...)` and `< <(gh ...)` iterate zero times whether the
# request failed or the answer was empty, and `|| true` turns any error into a successful empty
# read. The one survivor is the fetch, whose failure is checked immediately afterwards by asking
# git whether the objects arrived; it is named here so it stays the only one.
expect MUT-FAIL-OPEN-SHAPE "$(code_lines | grep -cE 'for [A-Za-z_]+ in \$\((gh|git)\b' || true)" 0
expect MUT-FAIL-OPEN-SHAPE "$(code_lines | grep -cE '< <\((gh|git)\b' || true)" 0
expect MUT-FAIL-OPEN-SHAPE \
  "$(code_lines | grep -E '\b(gh|git) .*\|\| true' | sed 's/^[[:space:]]*//' | tr '\n' ';')" \
  'git fetch -q origin "refs/pull/$pr/head" "$base" master 2>/dev/null || true;'
# A fourth shape, and the one that survived the first draft of this guard: a gh or git command
# piped into another and the pipeline's status read as an answer. `git diff ... | grep -q .` is
# false both when the diff is empty and when the diff failed, and the first means "edits no gate".
# Piping is fine where the result is captured and the status checked; it is not fine as a
# condition, because there the two outcomes are the same branch.
while IFS= read -r line; do
  [[ "$line" =~ (^|[^|])\|([^|]|$) ]] || continue
  [[ "$line" =~ (^|[[:space:]]|\()(gh|git)[[:space:]] ]] || continue
  [[ "$line" == *'="$('* ]] && continue
  error "MUT-FAIL-OPEN-SHAPE: a gh or git command is piped where its failure cannot be told from the pipeline's answer: [$line]"
done < <(code_lines | join_continuations)

# A fifth shape, one layer down: how a value is handed to a command. `<<<` is a here-document,
# and bash writes one to a TEMPORARY FILE once it outgrows a pipe buffer; a temporary file it
# cannot create is a redirection that failed, so the command never runs and the shell returns 1.
# For `grep` that 1 is "no match" -- an answer -- and the two become one. For a compound command
# it is worse still: `while ... done <<< "$x"` is skipped silently, with a status of 0, which
# neither a captured status nor `set -e` can see. Both shapes have been live in this file: one
# lost the only P1 in a 40,000-character review and printed END over it, the other would have
# dropped a findings list whole. Whole-line matching is done by expansion instead, and a read
# that needs a file is given a real one whose write is checked.
expect MUT-HERESTRING-FAILURE-AS-ANSWER \
  "$(code_lines | grep -E '\bgrep\b.*<<<' | sed 's/^[[:space:]]*//' | tr '\n' ';')" ''
expect MUT-HERESTRING-FAILURE-AS-ANSWER \
  "$(code_lines | grep -E '\bdone[[:space:]]*<<<' | sed 's/^[[:space:]]*//' | tr '\n' ';')" ''

# --- the option parser, through main -------------------------------------------------------------
# `main` is what these exercise: the defect was in its argument loop and no call on a helper can
# reach it. The stub gh answers with a marker and a failure, so an argument that should have been
# refused and instead got as far as GitHub is a failed case here rather than a live request.
stub="$tmp/stub"
mkdir -p "$stub"
printf '#!/usr/bin/env bash\necho "GH-REACHED $*" >&2\nexit 97\n' > "$stub/gh"
chmod +x "$stub/gh"
run_audit() {  # run_audit ARG...: "<exit status>|<output, on one line>"
  local out status=0
  out="$(PATH="$stub:$PATH" bash scripts/pr-ready-audit.sh "$@" 2>&1)" || status=$?
  printf '%s|%s' "$status" "$(tr '\n' ' ' <<< "$out")"
}
# A bare --reviewer read $2 before asking whether there was one and died on `$2: unbound variable`.
got="$(run_audit --reviewer)"
contains MUT-REVIEWER-ARG-EATEN "$got" "2|refusing: --reviewer needs a login"
# The next option is not a login. Consuming it named a reviewer nobody has *and* switched off the
# flag it swallowed, so the run reported no-review on everything and enqueued nothing, exit 0.
got="$(run_audit --reviewer --enqueue 123)"
contains MUT-REVIEWER-ARG-EATEN "$got" "2|refusing: --reviewer needs a login, got the option [--enqueue]"
[[ "$got" == *GH-REACHED* ]] && error "MUT-REVIEWER-ARG-EATEN: --reviewer --enqueue reached GitHub"
# The short option is an option too. A guard written as `--*` let `-h` through, and
# `--ready-label -h` ran a whole audit under a label named for the help flag while `--ready-label
# --help` refused -- the same defect the body claimed fixed, surviving in the half of the option
# space the guard did not name. Every option this parser takes, and one it does not, in both
# positions: what is refused is the leading hyphen, not a list that can drift again.
for opt in -h --help --enqueue --apply --reviewer --ready-label -x; do
  got="$(run_audit --ready-label "$opt" --reviewer eventloops 123)"
  contains MUT-REVIEWER-ARG-EATEN "$got" "2|refusing: --ready-label needs a label name, got the option [$opt]"
  [[ "$got" == *GH-REACHED* ]] && error "MUT-REVIEWER-ARG-EATEN: --ready-label $opt reached GitHub"
  got="$(run_audit --reviewer "$opt" 123)"
  contains MUT-REVIEWER-ARG-EATEN "$got" "2|refusing: --reviewer needs a login, got the option [$opt]"
  [[ "$got" == *GH-REACHED* ]] && error "MUT-REVIEWER-ARG-EATEN: --reviewer $opt reached GitHub"
done
# A malformed login is refused at the flag, before any request: the filter's encoding is the
# second bar, not the only one.
got="$(run_audit --reviewer 'eventloops" or true or .user.login == "eventloops' --enqueue 123)"
contains MUT-REVIEWER-JQ-INJECTION "$got" "2|refusing: --reviewer ["
[[ "$got" == *GH-REACHED* ]] && error "MUT-REVIEWER-JQ-INJECTION: a malformed --reviewer reached GitHub"
# UPSTROKE_REVIEW_AUTHOR takes the same value by another road and gets the same check.
got="$(UPSTROKE_REVIEW_AUTHOR='eventloops" or true' run_audit 123)"
contains MUT-REVIEWER-JQ-INJECTION "$got" "2|refusing: UPSTROKE_REVIEW_AUTHOR="
[[ "$got" == *GH-REACHED* ]] && error "MUT-REVIEWER-JQ-INJECTION: a malformed UPSTROKE_REVIEW_AUTHOR reached GitHub"
# The guard must stop the malformed and only the malformed: a real login gets through to the work.
got="$(run_audit --reviewer eventloops 123)"
contains MUT-REVIEWER-ARG-EATEN "$got" "GH-REACHED"
# --ready-label reads its argument the same way and had the same two defects.
contains MUT-REVIEWER-ARG-EATEN "$(run_audit --ready-label)" "2|refusing: --ready-label needs a label name"
contains MUT-REVIEWER-ARG-EATEN "$(run_audit --ready-label --enqueue 123)" "got the option [--enqueue]"
# A label name is still a label name, and a lane:* one is still refused for being a lane label.
contains MUT-REVIEWER-ARG-EATEN "$(run_audit --ready-label queue-me --reviewer eventloops 123)" "GH-REACHED"
contains MUT-REVIEWER-ARG-EATEN "$(run_audit --ready-label lane:feature 123)" "must not be a lane:* label"

# Which source supplies the login is settled before the value is checked. Validating the
# environment first made an inherited value this run does not use a precondition for the flag that
# replaces it: `--reviewer eventloops` refused from a shell whose UPSTROKE_REVIEW_AUTHOR was
# stale, and so did --help, which reads nothing at all.
got="$(UPSTROKE_REVIEW_AUTHOR='bad_login' run_audit --reviewer eventloops 123)"
contains MUT-REVIEWER-ENV-BEFORE-FLAG "$got" "GH-REACHED"
[[ "$got" == *refusing:* ]] && error "MUT-REVIEWER-ENV-BEFORE-FLAG: an overridden UPSTROKE_REVIEW_AUTHOR still refused the run"
got="$(UPSTROKE_REVIEW_AUTHOR='bad_login' run_audit --help)"
contains MUT-REVIEWER-ENV-BEFORE-FLAG "$got" "0|"
[[ "$got" == *refusing:* ]] && error "MUT-REVIEWER-ENV-BEFORE-FLAG: --help refused because of a value it never reads"
# Precedence is not permission: the environment value is still checked when it is the one that
# will be trusted, and the message says which source named it.
got="$(UPSTROKE_REVIEW_AUTHOR='bad_login' run_audit 123)"
contains MUT-REVIEWER-ENV-BEFORE-FLAG "$got" "2|refusing: UPSTROKE_REVIEW_AUTHOR=[bad_login] is not a GitHub login."
[[ "$got" == *GH-REACHED* ]] && error "MUT-REVIEWER-ENV-BEFORE-FLAG: an invalid UPSTROKE_REVIEW_AUTHOR reached GitHub"
got="$(run_audit --reviewer 'bad_login' 123)"
contains MUT-REVIEWER-ENV-BEFORE-FLAG "$got" "2|refusing: --reviewer [bad_login] is not a GitHub login."

# --- a failed review lookup, through main -------------------------------------------------------
# The helper case above proves latest_review_id reports a failure. This proves the audit acts on
# it: `no-review` and "the lookup did not complete" are different lines in the table, and only the
# second says the audit does not know what the reviewer said. Reading the second as the first is
# what let a pull request its reviewer had blocked reach the merge queue on an older PASS.
# This stub answers just enough to reach the review lookup; with no review id the audit resolves
# no reviewed commit, so nothing here touches git or the network.
lookup="$tmp/lookup-gh"
mkdir -p "$lookup"
cat > "$lookup/gh" <<'GH'
#!/usr/bin/env bash
head=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
case "$*" in
  "repo view"*)               echo eventloops/upstroke ;;
  *"--jq .owner.login")       echo eventloops ;;
  *"--jq .owner.type")        echo User ;;
  *rulesets*)                 ;;                                 # no branch ruleset
  *check-runs*)               printf 'upstroke-ci\t10\tsuccess\nupstroke-pr-policy\t11\tsuccess\n' ;;
  *"/pulls?state=open"*)      exit "${STUB_PULLS_STATUS:-0}" ;;
  *timeline*)                 ;;                                 # no base change
  *"/comments?per_page=100"*) [[ -n "${STUB_REVIEW_BODY:-}" ]] && echo "2026-09-01T00:00:00Z 5001"
                              exit "${STUB_COMMENTS_STATUS:-1}" ;;
  *"/issues/comments/5001 --jq .created_at") echo 2026-09-01T00:00:00Z ;;
  *"/issues/comments/5001 --jq .body")       cat "$STUB_REVIEW_BODY" ;;
  *"--json body"*)            echo "no ledger" ;;
  "pr view"*)                 (( ${STUB_PRVIEW_STATUS:-0} )) && exit "$STUB_PRVIEW_STATUS"
                              printf 'feature/x\n%s\nfalse\nCLEAN\n\nmaster\n%s\n' "$head" "$head" ;;
  *) echo "GH-UNSTUBBED $*" >&2; exit 97 ;;
esac
GH
chmod +x "$lookup/gh"
run_lookup() {  # run_lookup: the audit's table line, with the comment fetch exiting $1
  STUB_COMMENTS_STATUS="$1" PATH="$lookup:$PATH" bash scripts/pr-ready-audit.sh 999 2>&1 | tr '\n' ' '
}
got="$(run_lookup 1)"
contains MUT-REVIEW-LOOKUP-SUPPRESSED "$got" "NOT-READY"
contains MUT-REVIEW-LOOKUP-SUPPRESSED "$got" "blockers=review-lookup-failed"
[[ "$got" == *no-review* ]] && error "MUT-REVIEW-LOOKUP-SUPPRESSED: a failed lookup was reported as no-review"
[[ "$got" == *GH-UNSTUBBED* ]] && error "MUT-REVIEW-LOOKUP-SUPPRESSED: the audit made a call this case does not model: [$got]"
# The same run with the fetch succeeding and finding nothing is the ordinary no-review line, so
# the case above cannot be passed by a script that reports a lookup failure for everything.
got="$(run_lookup 0)"
contains MUT-REVIEW-LOOKUP-SUPPRESSED "$got" "blockers=no-review"
[[ "$got" == *review-lookup-failed* ]] && error "MUT-REVIEW-LOOKUP-SUPPRESSED: a completed lookup was reported as failed"

run_stub() {  # run_stub ARG...: "<exit status>|<output, on one line>", against the stub above
  local out status=0
  out="$(PATH="$lookup:$PATH" bash scripts/pr-ready-audit.sh "$@" 2>&1)" || status=$?
  printf '%s|%s' "$status" "$(tr '\n' ' ' <<< "$out")"
}
# A pull request whose own metadata could not be read is reported unaudited, not audited on the
# empty fields the failed read left behind. Read through `< <(...)` the status was invisible: the
# fields came back empty and the run died further down on an empty head, with no line for this
# pull request and no reason given.
got="$(STUB_PRVIEW_STATUS=1 run_stub 999)"
contains MUT-PR-LOOKUP-SUPPRESSED "$got" "blockers=pr-lookup-failed"
contains MUT-PR-LOOKUP-SUPPRESSED "$got" "NOT-READY"
[[ "$got" == *GH-UNSTUBBED* ]] && error "MUT-PR-LOOKUP-SUPPRESSED: the audit kept calling after the read failed: [$got]"
# and a readable pull request still reaches the review lookup, so the case above is about the
# failure and not about the stub.
got="$(run_stub 999)"
[[ "$got" == *pr-lookup-failed* ]] && error "MUT-PR-LOOKUP-SUPPRESSED: a readable pull request was reported unreadable"

# With no arguments the audit walks the open pull requests. An unread list became an empty one: a
# header, no rows, exit 0 -- which is what "every pull request was audited and none was ready"
# looks like. It must refuse instead.
got="$(STUB_PULLS_STATUS=1 run_stub)"
contains MUT-PR-LIST-SUPPRESSED "$got" "refusing: could not list"
[[ "$got" == *"#999"* ]] && error "MUT-PR-LIST-SUPPRESSED: the audit carried on past a failed listing"
# The stub's readable listing is empty, which is a real answer and must not refuse.
got="$(run_stub)"
[[ "$got" == *"refusing: could not list"* ]] && error "MUT-PR-LIST-SUPPRESSED: an empty listing was reported as a failed one"

# An empty --reviewer is a supplied value, not an absent one, and it is checked like any other.
got="$(run_stub --reviewer "" 999)"
contains MUT-REVIEWER-EMPTY-OVERRIDE "$got" "2|refusing: --reviewer [] is not a GitHub login."
[[ "$got" == *GH-REACHED* || "$got" == *"#999"* ]] && error "MUT-REVIEWER-EMPTY-OVERRIDE: an empty --reviewer reached the audit"
# So is an environment variable that is set and empty: a wrapper whose own variable was unset
# exports one, and reading it as "nothing was supplied" hands the run to the repository's owner.
got="$(UPSTROKE_REVIEW_AUTHOR= run_stub 999)"
contains MUT-REVIEWER-EMPTY-OVERRIDE "$got" "refusing: UPSTROKE_REVIEW_AUTHOR=[] is not a GitHub login."
# Unset is still unset, and the User owner still stands in: this narrows nothing that worked.
got="$(run_stub 999)"
[[ "$got" == *refusing* ]] && error "MUT-REVIEWER-EMPTY-OVERRIDE: an unset UPSTROKE_REVIEW_AUTHOR was read as supplied"

# A lone hyphen is an option too. `-?*` required a character after it, so `--ready-label -`
# labelled a pull request `-` and enqueued it.
for lone in - -h --help; do
  got="$(run_stub --ready-label "$lone" 999)"
  contains MUT-OPTION-LONE-HYPHEN "$got" "2|refusing: --ready-label needs a label name, got the option [$lone]"
  got="$(run_stub --reviewer "$lone" 999)"
  contains MUT-OPTION-LONE-HYPHEN "$got" "2|refusing: --reviewer needs a login, got the option [$lone]"
done
expect MUT-OPTION-LONE-HYPHEN "$(option_like - && echo yes || echo no)" yes
expect MUT-OPTION-LONE-HYPHEN "$(option_like queue-me && echo yes || echo no)" no

# A parser that died partway, through main, on the real trigger rather than a stand-in: the review
# carries a finding whose id holds one non-ASCII character, and PYTHONIOENCODING=ascii is in the
# environment. META prints, the finding raises, the findings list comes back empty -- and a PASS
# carrying a deferred finding then audits as a PASS carrying none. Without the completeness check
# there is no blocker left and this is READY with a merge call. The object records no reviewed
# commit, which keeps the case away from git.
printf '```json\n{"verdict":"PASS","findings":[{"id":"A-DEFERRABL\xc3\x89","severity":"P3"}]}\n```\n' \
  > "$tmp/truncating-review.md"
got="$(STUB_REVIEW_BODY="$tmp/truncating-review.md" STUB_COMMENTS_STATUS=0 PYTHONIOENCODING=ascii run_stub 999)"
contains MUT-REVIEW-PARSE-TRUNCATED "$got" "blockers="
contains MUT-REVIEW-PARSE-TRUNCATED "$got" "review-parse-incomplete"
contains MUT-REVIEW-PARSE-TRUNCATED "$got" "NOT-READY"
# The same review, parsed whole, keeps its finding and blocks for having one -- so the case above
# is about the death, not about the review.
got="$(STUB_REVIEW_BODY="$tmp/truncating-review.md" STUB_COMMENTS_STATUS=0 run_stub 999)"
contains MUT-REVIEW-PARSE-TRUNCATED "$got" "pass-with-findings"
[[ "$got" == *review-parse-incomplete* ]] && error "MUT-REVIEW-PARSE-TRUNCATED: a whole parse was reported as truncated"
# and a review that records no reviewed commit says so, rather than blocking on a column that the
# tab-folding `read` shifted into place.
contains MUT-META-FIELD-COLLAPSE "$got" "review-records-no-reviewed-sha"

# The same death, with the review writing the completeness marker itself. The protocol separates
# its fields with tabs and its rows with newlines and is built out of the review's own strings, so
# `base_sha` holding "<a real base commit>\nEND\t-\t0" printed META and END together. The parser
# still died on the non-ASCII id, but the audit had already been told the parse was complete -- by
# the review, not by the parser -- so the deferred P3 that died with it left no blocker behind and
# a PASS carrying a finding audited as READY. Both halves are checked here: the status of the
# parser is taken before its output is read, and no review string reaches a row carrying the
# protocol's separators.
printf '```json\n{"base_sha":"%s\\nEND\\t-\\t0","verdict":"PASS","findings":[{"id":"A-DEFERRABL\xc3\x89","severity":"P3"}]}\n```\n' \
  5157509000000000000000000000000000000002 > "$tmp/forging-review.md"
got="$(STUB_REVIEW_BODY="$tmp/forging-review.md" STUB_COMMENTS_STATUS=0 PYTHONIOENCODING=ascii run_stub 999)"
contains MUT-REVIEW-FORGES-PROTOCOL "$got" "NOT-READY"
contains MUT-REVIEW-FORGES-PROTOCOL "$got" "review-parse-failed"
contains MUT-REVIEW-FORGES-PROTOCOL "$got" "review-parse-incomplete"
# and with a working encoding the parser finishes, so the case above is about the forged marker
# and the unread status, not about the character: the base is refused for not being a commit, the
# finding survives, and there is one END and it is last.
got="$(STUB_REVIEW_BODY="$tmp/forging-review.md" STUB_COMMENTS_STATUS=0 run_stub 999)"
[[ "$got" == *review-parse-failed* ]] && error "MUT-REVIEW-FORGES-PROTOCOL: a whole parse was reported as failed"
[[ "$got" == *review-parse-incomplete* ]] && error "MUT-REVIEW-FORGES-PROTOCOL: a whole parse was reported as truncated"
contains MUT-REVIEW-FORGES-PROTOCOL "$got" "pass-with-findings"

# --- the workflow form: a fenced JSON verdict ---------------------------------------------------
cat > "$tmp/json.md" <<'EOF'
Findings workflow review 2/2.

Reviewed head: 4ad962f000000000000000000000000000000001
Base: 5157509000000000000000000000000000000002
Reviewer: gpt-5.6-sol, medium effort. An earlier draft said {"verdict":"PASS"} but see below;
the prose here also mentions a P1 that is not in the object.

Unedited verdict:

```json
{"reviewed_sha":"4ad962f000000000000000000000000000000001","base_sha":"5157509000000000000000000000000000000002","verdict":"CHANGES_REQUIRED","findings":[{"id":"A-DEFERRABLE","severity":"P3","reproduction":null,"witness":false},{"id":"B-WITNESSED","severity":"P2","failing_test":"a_test_that_fails"},{"id":"C-MUST","severity":"P2","correction":"This is a MUST deviation of standards section 7"},{"id":"D-BAD","severity":"P9"},{"id":"E.DOTTED","severity":"P3","location":"src/x.rs:1"}]}
```
EOF
expect MUT-QUOTED-JSON-IS-JSON "$(review_kind "$tmp/json.md")" json
got="$(parse_verdict_json "$tmp/json.md" | tr '\t' '|')"
want='META|4ad962f000000000000000000000000000000001|CHANGES_REQUIRED|5157509000000000000000000000000000000002
STRAY|P1|0
P3|A-DEFERRABLE|0
P2|B-WITNESSED|1
P2|C-MUST|2
ERR|bad-severity:D-BAD|0
P3|E.DOTTED|0
END|-|0'
expect "MUT-JSON-SPLIT-BY-REGEX/MUT-NULL-WITNESS/MUT-MUST-UNSEEN/MUT-BAD-SEVERITY-PASSES/MUT-STRAY-TOKEN-UNSEEN/MUT-VERDICT-FROM-PROSE" "$got" "$want"

# A pretty-printed object with "}, {" between findings is the same object to a parser.
cat > "$tmp/pretty.md" <<'EOF'
Reviewed head: 4ad962f000000000000000000000000000000001

```json
{
  "reviewed_sha": "4ad962f000000000000000000000000000000001",
  "verdict": "CHANGES_REQUIRED",
  "findings": [
    {"id": "FIRST", "severity": "P3"}, {"id": "SECOND", "severity": "P1"}
  ]
}
```
EOF
got="$(parse_verdict_json "$tmp/pretty.md" | tr '\t' '|')"
want='META|4ad962f000000000000000000000000000000001|CHANGES_REQUIRED|-
P3|FIRST|0
P1|SECOND|0
END|-|0'
expect MUT-JSON-SPLIT-BY-REGEX "$got" "$want"

# --- the frontier form: prose ------------------------------------------------------------------
cat > "$tmp/prose.md" <<'EOF'
<!-- upstroke-frontier-review pr=145 head=c3a6665000000000000000000000000000000003 -->
## Frontier review of `c3a6665` (gpt-5.6-sol, max effort)

**VERDICT: PASS**

<details>
1. **P2 — The claimed closure fails.** Detail.

2. **P3 — A scope claim does not match the diff.** Detail.

### P1 — data loss written as a heading, which a numbered-only parser would miss.

I found no MUST deviation.

VERDICT: CHANGES_REQUIRED
</details>
EOF
expect MUT-QUOTED-JSON-IS-JSON "$(review_kind "$tmp/prose.md")" prose
got="$(parse_prose_review "$tmp/prose.md" | tr '\t' '|')"
want='META|c3a6665000000000000000000000000000000003|CHANGES_REQUIRED|-
P2|-|0
P3|-|0
STRAY|MUST/P1|0
END|-|0'
expect "MUT-PROSE-HEADING-FINDING/MUT-PROSE-LAST-VERDICT" "$got" "$want"

# Both parsers end with END, and nothing else does. END is what tells the audit the findings list
# it just read is the whole list: a parser that dies partway prints a shorter one, which is not an
# error the caller can see, only a weaker verdict. `PYTHONIOENCODING=ascii` in the environment and
# one non-ASCII character in a finding id is enough to do it -- META prints, the finding raises --
# and a PASS carrying a deferred finding then audits as a PASS carrying none, which is READY.
expect MUT-REVIEW-PARSE-TRUNCATED "$(parse_verdict_json "$tmp/json.md" | tail -1)" "$(printf 'END\t-\t0')"
expect MUT-REVIEW-PARSE-TRUNCATED "$(parse_prose_review "$tmp/prose.md" | tail -1)" "$(printf 'END\t-\t0')"
printf 'Reviewed head: %s\n\n```json\n{"reviewed_sha":"%s","base_sha":"%s","verdict":"PASS","findings":[{"id":"A-DEFERRABL\xc3\x89","severity":"P3"}]}\n```\n' \
  4ad962f000000000000000000000000000000001 4ad962f000000000000000000000000000000001 \
  5157509000000000000000000000000000000002 > "$tmp/nonascii.md"
# The parser exits non-zero here, which is exactly the point: nothing downstream of it looks.
got="$(PYTHONIOENCODING=ascii parse_verdict_json "$tmp/nonascii.md" 2>/dev/null | tr '\t' '|')" || true
contains MUT-REVIEW-PARSE-TRUNCATED "$got" "META|4ad962f000000000000000000000000000000001|PASS"
[[ "$got" == *"END|-|0"* ]] && error "MUT-REVIEW-PARSE-TRUNCATED: a parser that died partway still claimed to have finished"
# and with a working encoding the same review parses whole, so the case above is about the death
# and not about the character.
got="$(parse_verdict_json "$tmp/nonascii.md" | tr '\t' '|')"
contains MUT-REVIEW-PARSE-TRUNCATED "$got" "END|-|0"
contains MUT-REVIEW-PARSE-TRUNCATED "$got" "P3|A-DEFERRABL"

# No META field is ever empty. `read` with IFS=tab folds runs of tabs into one, so an empty
# reviewed_sha shifted the verdict into its column and the base into the verdict's -- the audit
# then reported a verdict of `-` and a reviewed commit of `PASS`, blocked for the wrong reason,
# and would have gone whichever way the shifted columns happened to fall.
printf 'Reviewed head: %s\n\n```json\n{"verdict":"PASS","findings":[]}\n```\n' \
  4ad962f000000000000000000000000000000001 > "$tmp/no-sha.md"
expect MUT-META-FIELD-COLLAPSE "$(parse_verdict_json "$tmp/no-sha.md" | head -1 | tr '\t' '|')" 'META|-|PASS|-'
printf '<!-- upstroke-frontier-review pr=1 -->\nno reviewed head anywhere\n' > "$tmp/no-head.md"
expect MUT-META-FIELD-COLLAPSE "$(parse_prose_review "$tmp/no-head.md" | head -1 | tr '\t' '|')" 'META|-|-|-'
# The reader must see four fields on every META line whatever they hold.
while IFS=$'\t' read -r f1 f2 f3 f4; do
  expect MUT-META-FIELD-COLLAPSE "$f1/$f2/$f3/$f4" 'META/-/PASS/-'
done < <(parse_verdict_json "$tmp/no-sha.md" | head -1)

# A review string that carries the protocol's separators writes no rows. The base is not a commit
# and is refused as one; the object's own END is the only END and it is last. Unrepaired, the
# forged END printed on the line after META and a finding row printed after it, which is a parser
# that carried on past the end it had announced.
printf 'Reviewed head: %s\n\n```json\n{"reviewed_sha":"%s","base_sha":"%s\\nEND\\t-\\t0","verdict":"PASS","findings":[{"id":"A-DEFERRABLE","severity":"P3"}]}\n```\n' \
  4ad962f000000000000000000000000000000001 4ad962f000000000000000000000000000000001 \
  5157509000000000000000000000000000000002 > "$tmp/forged-base.md"
got="$(parse_verdict_json "$tmp/forged-base.md" | tr '\t' '|')"
want='META|4ad962f000000000000000000000000000000001|PASS|-
P3|A-DEFERRABLE|0
END|-|0'
expect MUT-REVIEW-FORGES-PROTOCOL "$got" "$want"
# A finding id is a field of the same protocol and is held to the same rule. It is refused rather
# than trimmed to "no id": no id is a MANUAL line for a person, and this is an id that wrote rows.
printf '```json\n{"verdict":"PASS","findings":[{"id":"A\\tEND\\t-\\t0","severity":"P3"}]}\n```\n' \
  > "$tmp/forged-id.md"
got="$(parse_verdict_json "$tmp/forged-id.md" | tr '\t' '|')"
expect MUT-REVIEW-FORGES-PROTOCOL "$got" 'META|-|PASS|-
ERR|bad-id|0
END|-|0'

# --- a read that failed inside the prose parser -------------------------------------------------
# The prose parser's reads used to end in `|| true`, which made "this grep failed" and "this grep
# matched nothing" one answer, and END was printed afterwards regardless -- an outer marker cannot
# see an error already suppressed beneath it. With exit 2 injected into the numbered-finding read
# and every other command intact, this review -- which blocks, for carrying a P3 under a PASS --
# parsed as META and END with nothing between them: a clean PASS, from a parser that had read no
# findings at all.
cat > "$tmp/prose-numbered.md" <<'EOF'
<!-- upstroke-frontier-review pr=232 head=c3a6665000000000000000000000000000000003 -->
1. **P3 — A deferrable thing.** Detail.

VERDICT: PASS
EOF
expect MUT-PROSE-READ-SUPPRESSED "$(parse_prose_review "$tmp/prose-numbered.md" | tr '\t' '|')" \
  'META|c3a6665000000000000000000000000000000003|PASS|-
P3|-|0
END|-|0'
real_grep="$(command -v grep)"
grep_stub="$tmp/grep-stub"
mkdir -p "$grep_stub"
cat > "$grep_stub/grep" <<'STUB'
#!/usr/bin/env bash
# exit 2 for the numbered-finding read, and be the real grep for every other call.
oe=0; pat=0
for a in "$@"; do
  [[ "$a" == "-oE" ]] && oe=1
  [[ "$a" == '^[0-9]+\. \*\*P[0-3]' ]] && pat=1
done
(( oe && pat )) && exit 2
exec "$REAL_GREP" "$@"
STUB
chmod +x "$grep_stub/grep"
prose_status=0
got="$(
  export REAL_GREP="$real_grep" PATH="$grep_stub:$PATH"
  hash -r
  parse_prose_review "$tmp/prose-numbered.md" 2>/dev/null
)" || prose_status=$?
[[ "$prose_status" == 0 ]] \
  && error "MUT-PROSE-READ-SUPPRESSED: a parser whose findings read exited 2 reported success"
[[ "$got" == *END* ]] \
  && error "MUT-PROSE-READ-SUPPRESSED: END was printed over a read that failed, got [$got]"

# A prose review that quotes a JSON object stays prose.
printf 'Reviewed head: %s\nThe object {"verdict":"PASS","findings":[]} is an example.\nVERDICT: CHANGES_REQUIRED\n' \
  "4ad962f000000000000000000000000000000001" > "$tmp/quoted.md"
expect MUT-QUOTED-JSON-IS-JSON "$(review_kind "$tmp/quoted.md")" prose

# A FORMAT DETECTION THAT FAILED IS NOT A FORMAT. `grep`'s 1 is "this comment carries no fenced
# object", an answer; its 2 is a file it could not read. `if grep ...; then json; else prose; fi`
# made the second into the first and CHOSE A PARSER on it -- and the two parsers do not agree
# about the same review. The JSON form's own verdict object says CHANGES_REQUIRED and carries a
# P1; sent to the prose parser it loses both, because the `VERDICT:` line the prose parser reads
# sits outside the object and says PASS, and a severity the object spells with a JSON escape
# (`"P1"`) holds no `P1` for any grep to find.
kind_stub="$tmp/kind-stub"
mkdir -p "$kind_stub"
cat > "$kind_stub/grep" <<'STUB'
#!/usr/bin/env bash
# exit 2 for the format-detection read, and be the real grep for every other call.
qe=0; pat=0
for a in "$@"; do
  [[ "$a" == "-qE" ]] && qe=1
  [[ "$a" == '^```json|"role_understanding"' ]] && pat=1
done
(( qe && pat )) && exit 2
exec "$REAL_GREP" "$@"
STUB
chmod +x "$kind_stub/grep"
printf '```json\n{"verdict":"CHANGES_REQUIRED","findings":[{"id":"CRITICAL","severity":"P\\u0031"}]}\n```\n\nVERDICT: PASS\n' \
  > "$tmp/escaped-severity.md"
# Read whole, this is a blocking review: a P1 in a feature lane, from an object that says so.
expect MUT-REVIEW-KIND-UNREADABLE-IS-PROSE "$(review_kind "$tmp/escaped-severity.md")" json
expect MUT-REVIEW-KIND-UNREADABLE-IS-PROSE "$(parse_verdict_json "$tmp/escaped-severity.md" | tr '\t' '|')" \
  'META|-|CHANGES_REQUIRED|-
P1|CRITICAL|0
END|-|0'
# The prose parser reads the same file as a clean PASS carrying nothing, which is what makes the
# choice of parser a verdict rather than a formatting detail.
expect MUT-REVIEW-KIND-UNREADABLE-IS-PROSE "$(parse_prose_review "$tmp/escaped-severity.md" | tr '\t' '|')" \
  'META|-|PASS|-
END|-|0'
kind_status=0
got="$(
  export REAL_GREP="$real_grep" PATH="$kind_stub:$PATH"
  hash -r
  review_kind "$tmp/escaped-severity.md" 2>/dev/null
)" || kind_status=$?
((kind_status != 0)) \
  || error "MUT-REVIEW-KIND-UNREADABLE-IS-PROSE: a detection that exited 2 answered with a format [$got]"
[[ -z "$got" ]] \
  || error "MUT-REVIEW-KIND-UNREADABLE-IS-PROSE: a failed detection named a parser [$got]"
# And through main, because the helper refusing is only half of it: the audit must not fall
# through to the prose parser on the way past. Neither parser runs, nothing is read out of the
# review, and the audit says which of its reads did not happen.
got="$(
  export REAL_GREP="$real_grep"
  STUB_REVIEW_BODY="$tmp/escaped-severity.md" STUB_COMMENTS_STATUS=0 \
    PATH="$kind_stub:$lookup:$PATH" bash scripts/pr-ready-audit.sh 999 2>&1 | tr '\n' ' '
)"
contains MUT-REVIEW-KIND-UNREADABLE-IS-PROSE "$got" "review-format-unreadable"
contains MUT-REVIEW-KIND-UNREADABLE-IS-PROSE "$got" "NOT-READY"
[[ "$got" == *"verdict=PASS"* ]] \
  && error "MUT-REVIEW-KIND-UNREADABLE-IS-PROSE: a verdict was read out of a review no parser read"
# The same review with the detection working is judged by the JSON parser and blocks on its P1,
# so the case above is about the failed detection and not about the review.
got="$(
  STUB_REVIEW_BODY="$tmp/escaped-severity.md" STUB_COMMENTS_STATUS=0 \
    PATH="$lookup:$PATH" bash scripts/pr-ready-audit.sh 999 2>&1 | tr '\n' ' '
)"
contains MUT-REVIEW-KIND-UNREADABLE-IS-PROSE "$got" "open-P1:CRITICAL"
[[ "$got" == *review-format-unreadable* ]] \
  && error "MUT-REVIEW-KIND-UNREADABLE-IS-PROSE: a readable review was reported unreadable"

# A verdict is a whole token, in every locale. `[A-Z_]+$` matches the TAIL of a dirty token, and
# `[A-Z_]` inside a grep is whatever the locale's collating order says it is: under en_US.utf8 the
# grep carried `FAIL\xc3\x89PASS` through and the check took the clean `PASS` off its end, so a
# review that says FAIL approved the change -- while the same file under `C` blocked. One answer
# everywhere, and the token that is not a verdict is carried whole -- `VERDICT:` and all, because
# a prefix stripped off a refused token is the salvage step below -- so the blocker names it.
printf '<!-- upstroke-frontier-review pr=232 head=%s -->\n\nVERDICT: FAIL\xc3\x89PASS\n' \
  c3a6665000000000000000000000000000000003 > "$tmp/prose-suffix.md"
suffix_want="$(printf 'META|c3a6665000000000000000000000000000000003|VERDICT: FAIL\xc3\x89PASS|-\nEND|-|0')"
# The ambient locale, and then `$locales` -- `C` and every UTF-8 locale this machine has, built
# for the login check above. The defect is a disagreement between them, so one alone cannot see it.
expect MUT-PROSE-VERDICT-SUFFIX "$(parse_prose_review "$tmp/prose-suffix.md" | tr '\t' '|')" "$suffix_want"
for loc in "${locales[@]}"; do
  got="$(LC_ALL="$loc" parse_prose_review "$tmp/prose-suffix.md" | tr '\t' '|')"
  expect "MUT-PROSE-VERDICT-SUFFIX under $loc" "$got" "$suffix_want"
done
# A verdict wrapped in the emphasis a frontier review writes is still that verdict, and the last
# line still wins: the whole-token rule must not refuse the form the reviews actually use.
printf 'head=%s\n**VERDICT: CHANGES_REQUIRED**\n**VERDICT: PASS**\n' \
  c3a6665000000000000000000000000000000003 > "$tmp/prose-emphasis.md"
# `| head -1` is not how a line is taken from a parser here: it closes the pipe at the first line,
# the parser still printing takes SIGPIPE, and under `pipefail` this file's own `set -e` ends the
# run at 141 -- the trap the script under test was repaired for, in the gate that proves it.
got="$(parse_prose_review "$tmp/prose-emphasis.md" | tr '\t' '|')"
expect MUT-PROSE-VERDICT-SUFFIX "${got%%$'\n'*}" 'META|c3a6665000000000000000000000000000000003|PASS|-'
# The head is the FIRST marker, and a first marker that does not read whole is not a licence to
# take the second: the audit would then check the pull request's head against a commit the review
# never named. `-` blocks; the later line does not stand in for it.
printf 'head=c3a6665\xc3\x8900000000000000000000000000000003\nReviewed head: %s\n' \
  4ad962f000000000000000000000000000000001 > "$tmp/prose-head.md"
for loc in "${locales[@]}"; do
  got="$(LC_ALL="$loc" parse_prose_review "$tmp/prose-head.md" | tr '\t' '|')"
  expect "MUT-PROSE-HEAD-NOT-FIRST under $loc" "${got%%$'\n'*}" 'META|-|-|-'
done

# --- a check that failed is the answer, not the input to another attempt --------------------------
# The whole-token check refuses `VERDICT: ::PASS`, and the branch it falls into used to strip the
# leading colons and asterisks off what it had just refused -- so the rejecting branch minted the
# `PASS` the check exists to withhold. Nothing is stripped now: the whole matched run stands, and
# because every one of these begins with `VERDICT:` it cannot be the token the audit lets through.
# `**VERDICT: PASS**`, the form the reviews are actually written in, is pinned above and stays.
for junk in ': ::PASS' ': :PASS' ': **PASS' ': *PASS'; do
  printf '<!-- upstroke-frontier-review pr=232 head=%s -->\n\nVERDICT%s\n' \
    c3a6665000000000000000000000000000000003 "$junk" > "$tmp/prose-salvage.md"
  got="$(parse_prose_review "$tmp/prose-salvage.md" | tr '\t' '|')"
  [[ "$got" == *'|PASS|'* ]] \
    && error "MUT-PROSE-VERDICT-SALVAGE: [VERDICT$junk] was salvaged into PASS, got [$got]"
  contains MUT-PROSE-VERDICT-SALVAGE "$got" 'META|c3a6665000000000000000000000000000000003|VERDICT'
  for loc in "${locales[@]}"; do
    got="$(LC_ALL="$loc" parse_prose_review "$tmp/prose-salvage.md" | tr '\t' '|')"
    [[ "$got" == *'|PASS|'* ]] \
      && error "MUT-PROSE-VERDICT-SALVAGE under $loc: [VERDICT$junk] was salvaged into PASS, got [$got]"
  done
done

# --- a here-string bash could not write, read as "nothing matched" -------------------------------
# `<<<` is a here-document: over a pipe buffer's worth of data bash writes it to a TEMPORARY FILE,
# and a temporary file it cannot create is a redirection that failed -- the command never runs and
# the shell returns 1. `ulimit -f` denies exactly that file and nothing else: 512 bytes is more
# than any fixture below needs written legitimately and far less than either here-document. SIGXFSZ
# is ignored so the failure arrives as a status rather than a signal, which is the shape an ENOSPC
# on the temp directory has. Both fixtures are also parsed with no limit at all, so what is under
# test is the denied write and not the fixture.
#
# The stray-token read: a review carrying the head, a PASS and one standalone `P1`, padded past the
# buffer. The P1 is a MANUAL blocker. Fed through `<<<`, the failed spill returned grep's 1, the
# token was dropped and `END` printed over it: a clean PASS, which is READY.
{ printf '<!-- upstroke-frontier-review pr=232 head=%s -->\n\nVERDICT: PASS\n\n' \
    c3a6665000000000000000000000000000000003
  for _ in $(seq 1 400); do printf '\xc3\xa9%.0s' $(seq 1 100); printf '\n'; done
  printf '\nA standalone P1 in running text.\n'; } > "$tmp/prose-big-stray.md"
expect MUT-HERESTRING-FAILURE-AS-ANSWER "$(parse_prose_review "$tmp/prose-big-stray.md" | tr '\t' '|')" \
  'META|c3a6665000000000000000000000000000000003|PASS|-
STRAY|P1|0
END|-|0'
big_status=0
got="$(trap '' XFSZ; ulimit -f 1; parse_prose_review "$tmp/prose-big-stray.md" 2>/dev/null)" || big_status=$?
((big_status != 0)) \
  || error "MUT-HERESTRING-FAILURE-AS-ANSWER: a parser whose token read could not be written reported success"
[[ "$got" == *END* ]] \
  && error "MUT-HERESTRING-FAILURE-AS-ANSWER: END was printed over a read that never happened, got [$got]"
# and the finding itself: it is in the parse, or the parse refuses. What it may not be is missing
# from a parse that reports success, which is a MANUAL blocker the audit never raises.
[[ "$got" == *STRAY* || "$big_status" != 0 ]] \
  || error "MUT-HERESTRING-FAILURE-AS-ANSWER: the standalone P1 went missing from a parse that reported success, got [$got]"

# The numbered-finding read, where the here-string fed a COMPOUND command: bash skips the loop
# body, the status is 0, and `set -e` sees nothing. Every finding in the review is then missing
# from a parse that says it finished -- a PASS carrying 8000 deferrable findings, audited as a
# PASS carrying none.
{ printf '<!-- upstroke-frontier-review pr=232 head=%s -->\n\n' \
    c3a6665000000000000000000000000000000003
  for i in $(seq 1 8000); do printf '%d. **P3 - a deferrable thing.** Detail.\n' "$i"; done
  printf '\nVERDICT: PASS\n'; } > "$tmp/prose-many-findings.md"
many_status=0
got="$(trap '' XFSZ; ulimit -f 1; parse_prose_review "$tmp/prose-many-findings.md" 2>/dev/null)" || many_status=$?
expect MUT-HERESTRING-FAILURE-AS-ANSWER "$many_status" 0
expect MUT-HERESTRING-FAILURE-AS-ANSWER "$(grep -c '^P3' <<< "$got")" 8000
expect MUT-HERESTRING-FAILURE-AS-ANSWER "$(tail -1 <<< "$got" | tr '\t' '|')" 'END|-|0'
# and the same fixture with no limit, so the count above is the review's and not the limit's.
expect MUT-HERESTRING-FAILURE-AS-ANSWER \
  "$(parse_prose_review "$tmp/prose-many-findings.md" | grep -c '^P3')" 8000

# --- the frontmatter id match ------------------------------------------------------------------
printf -- '---\nid: OTHER-ID\nseverity: P2\n---\n\nThe prose below repeats a line.\nid: TARGET-ID\n' > "$tmp/prose-id.md"
printf -- '---\nid: TARGET-ID\nseverity: P2\n---\n\nBody.\n' > "$tmp/front-id.md"
printf -- '---\nid: TARGETXID\n---\n' > "$tmp/x-id.md"
printf 'id: TARGET-ID\n---\nno opening fence\n' > "$tmp/no-front.md"
# The answer is what it PRINTS -- `1` or `0` -- and its status is only ever failure. `cmd < file`
# on a file bash cannot open returns 1 with the command never run, and 1 used to be this helper's
# word for "read to the end and did not carry the id", so an input that could not be opened
# answered in the matcher's place. Both are checked on every case: the answer, and a status of 0
# saying the matcher is the one who gave it.
has_id() {  # has_id ID FILE: "<status>|<answer>"
  local out status=0
  out="$(frontmatter_has_id "$1" < "$2")" || status=$?
  printf '%s|%s' "$status" "$out"
}
expect MUT-FRONTMATTER-BY-SUBSTRING "$(has_id TARGET-ID "$tmp/prose-id.md")" "0|0"   # an id in prose is not one
expect MUT-FRONTMATTER-BY-SUBSTRING "$(has_id TARGET-ID "$tmp/front-id.md")" "0|1"
expect MUT-FRONTMATTER-PATTERN "$(has_id TARGET.ID "$tmp/x-id.md")" "0|0"            # a dot is not a regex
expect MUT-FRONTMATTER-BY-SUBSTRING "$(has_id TARGET-ID "$tmp/no-front.md")" "0|0"   # no frontmatter block
# An input the redirection could not open is not a file that does not carry the id: nothing ran,
# so there is no answer, and the status says so.
expect MUT-FINDING-BLOB-OPEN-AS-MISS "$(has_id TARGET-ID "$tmp/no-such-finding-file.md" 2>/dev/null)" "1|"

# --- counting the files that file a finding ------------------------------------------------------
# The one place git is exercised here, in a repository built for it. This count decides whether a
# deferred finding is properly filed, and the shape rules above cannot see any of the ways it goes
# wrong: `git grep` exits 1 for "nothing matched" and above 1 for an error, and the names come back
# NUL-separated, which a command substitution silently drops -- leaving a loop with no separators,
# no iterations, and a count of zero for every finding in a tree that holds the file.
findings_repo="$tmp/findings-repo"
mkdir -p "$findings_repo/reviews/findings"
git init -q "$findings_repo"
printf -- '---\nid: A-DEFERRABLE\nseverity: P3\n---\n\nBody.\n' > "$findings_repo/reviews/findings/a.md"
printf -- '---\nid: B-OTHER\nseverity: P3\n---\n\nBody.\n' > "$findings_repo/reviews/findings/b with space.md"
printf -- '---\nid: C-TWICE\n---\n\nBody.\n' > "$findings_repo/reviews/findings/c1.md"
printf -- '---\nid: C-TWICE\n---\n\nBody.\n' > "$findings_repo/reviews/findings/c2.md"
printf -- 'id: D-PROSE-ONLY\nnot frontmatter\n' > "$findings_repo/reviews/findings/d.md"
# Two files filing one id, the second with a long line after the id inside its frontmatter. The
# match is on the id line, so `grep -q` exited there and the `awk` still writing the rest took
# SIGPIPE: `pipefail` reported 141, the caller read any non-zero as "this file does not carry the
# id", and the duplicate counted as no file at all. One file, and the pull request was ready.
printf -- '---\nid: E-LONG-LINE\n---\n\nBody.\n' > "$findings_repo/reviews/findings/e1.md"
{ printf -- '---\nid: E-LONG-LINE\ndescription: '
  head -c 262144 /dev/zero | tr '\0' 'x'
  printf -- '\n---\n\nBody.\n'
} > "$findings_repo/reviews/findings/e2.md"
git -C "$findings_repo" add -A
git -C "$findings_repo" -c user.email=t@example -c user.name=t commit -qm "file the findings"
count_in_fixture() { (cd "$findings_repo" && finding_file_count "$1" HEAD); }
expect MUT-FINDING-FILE-COUNT "$(count_in_fixture A-DEFERRABLE)" 1
expect MUT-FINDING-FILE-COUNT "$(count_in_fixture B-OTHER)" 1          # the name has a space in it
expect MUT-FINDING-FILE-COUNT "$(count_in_fixture C-TWICE)" 2          # two files is not one
expect MUT-FINDING-FILE-COUNT "$(count_in_fixture D-PROSE-ONLY)" 0     # the id is not in frontmatter
expect MUT-FINDING-FILE-COUNT "$(count_in_fixture NOT-FILED-ANYWHERE)" 0
# A tree it cannot read is not a tree with no files in it.
if (cd "$findings_repo" && finding_file_count A-DEFERRABLE deadbeefdeadbeefdeadbeefdeadbeefdeadbeef) > "$tmp/tree.out" 2>&1; then
  error "MUT-FINDING-FILE-COUNT: an unreadable tree was counted, got [$(cat "$tmp/tree.out")]"
fi
# Two files, one of which the frontmatter read cannot finish. This is the count's whole job: two
# files filing one id is `duplicate-file` and one is ready, so a file that dropped out of the
# count is the difference between blocked and merged.
expect MUT-FINDING-FILE-READ-AS-MISS "$(count_in_fixture E-LONG-LINE)" 2
# The helper's own report on that file, so the count above is not the only witness: `1` printed
# and a status of 0, from a read that went to the end of a frontmatter block a quarter of a
# megabyte long.
expect MUT-FINDING-FILE-READ-AS-MISS \
  "$(has_id E-LONG-LINE "$findings_repo/reviews/findings/e2.md")" "0|1"

# A blob the tree still names and the object store no longer holds. `git grep` cannot find this
# one for anybody: it printed `unable to read` on stderr, exited 0, and returned only the readable
# name -- so checking its status catches nothing, and the candidates have to come from the tree.
broken_repo="$tmp/broken-repo"
mkdir -p "$broken_repo/reviews/findings"
git init -q "$broken_repo"
printf -- '---\nid: F-TWICE\n---\n\nBody.\n' > "$broken_repo/reviews/findings/f1.md"
printf -- '---\nid: F-TWICE\n---\n\nAnother body.\n' > "$broken_repo/reviews/findings/f2.md"
git -C "$broken_repo" add -A
git -C "$broken_repo" -c user.email=t@example -c user.name=t commit -qm "file one id twice"
expect MUT-FINDING-FILE-READ-AS-MISS "$( (cd "$broken_repo" && finding_file_count F-TWICE HEAD) )" 2
broken_blob="$(git -C "$broken_repo" rev-parse HEAD:reviews/findings/f2.md)"
rm -f "$broken_repo/.git/objects/${broken_blob:0:2}/${broken_blob:2}"
if (cd "$broken_repo" && finding_file_count F-TWICE HEAD) > "$tmp/blob.out" 2>&1; then
  error "MUT-FINDING-FILE-READ-AS-MISS: a file whose blob could not be read was counted, got [$(cat "$tmp/blob.out")]"
fi

# A frontmatter read that DIES is not a file that does not carry the id. While the read was a
# pipeline this was unprovable from the outside: under `pipefail` the reader's 2 stood behind the
# matcher's ordinary 1 -- nothing matched, because nothing arrived -- and 1 is an answer. Two
# files filing one id came back as one, which is `duplicate-file` turning into ready.
#
# The injection is an `awk` that fails on the second file and is the real `awk` everywhere else,
# so the count has one good read and one dead one, exactly as a half-readable blob would give it.
awk_stub="$tmp/awk-stub"
mkdir -p "$awk_stub"
cat > "$awk_stub/awk" <<'STUB'
#!/usr/bin/env bash
# The frontmatter arrives on stdin, so the file is known by what is in it and not by an argument.
in="$(mktemp)"; cat > "$in"
if grep -qF 'SECOND-FILE-MARKER' "$in"; then rm -f "$in"; exit 2; fi
"$REAL_AWK" "$@" < "$in"; s=$?; rm -f "$in"; exit $s
STUB
chmod +x "$awk_stub/awk"
dup_repo="$tmp/dup-repo"
mkdir -p "$dup_repo/reviews/findings"
git init -q "$dup_repo"
printf -- '---\nid: G-TWICE\n---\n\nBody.\n' > "$dup_repo/reviews/findings/g1.md"
printf -- '---\nid: G-TWICE\nnote: SECOND-FILE-MARKER\n---\n\nBody.\n' > "$dup_repo/reviews/findings/g2.md"
git -C "$dup_repo" add -A
git -C "$dup_repo" -c user.email=t@example -c user.name=t commit -qm "file one id twice"
expect MUT-FRONTMATTER-UPSTREAM-ERROR-AS-MISS "$( (cd "$dup_repo" && finding_file_count G-TWICE HEAD) )" 2
dup_status=0
got="$(
  export REAL_AWK="$(command -v awk)" PATH="$awk_stub:$PATH"
  hash -r
  cd "$dup_repo" && finding_file_count G-TWICE HEAD 2>/dev/null
)" || dup_status=$?
((dup_status != 0)) \
  || error "MUT-FRONTMATTER-UPSTREAM-ERROR-AS-MISS: a count with a dead read in it succeeded, got [$got]"
[[ "$got" == 1 ]] \
  && error "MUT-FRONTMATTER-UPSTREAM-ERROR-AS-MISS: two files filing one id counted as one"
# And the helper itself: a read that never finished prints no answer and says so in its status.
front_status=0
front_out="$(
  export REAL_AWK="$(command -v awk)" PATH="$awk_stub:$PATH"
  hash -r
  frontmatter_has_id G-TWICE < "$dup_repo/reviews/findings/g2.md" 2>/dev/null
)" || front_status=$?
((front_status != 0)) \
  || error "MUT-FRONTMATTER-UPSTREAM-ERROR-AS-MISS: a read that died reported success"
[[ -z "$front_out" ]] \
  || error "MUT-FRONTMATTER-UPSTREAM-ERROR-AS-MISS: a read that died printed an answer [$front_out]"

# A BLOB THE CALLER COULD NOT OPEN is the same defect one layer out, and it does not need the
# matcher to fail: `frontmatter_has_id "$id" < "$blob_file"` on a file bash cannot open is a
# redirection that failed, so the shell returns 1 WITHOUT RUNNING THE MATCHER -- and 1 was the
# matcher's own word for "read to the end and did not carry the id". The count then dropped the
# file it could not open, two files filing one id came back as one, `duplicate-file` never fired
# and the pull request was READY.
#
# The injection is a `git` that is the real git everywhere except the `show` of the second file,
# whose output it writes and then REMOVES -- the blob is written, the read of it is what fails.
# Removal rather than a mode change so the case is the same case for a run as root.
git_stub="$tmp/git-stub"
mkdir -p "$git_stub"
cat > "$git_stub/git" <<'STUB'
#!/usr/bin/env bash
"$REAL_GIT" "$@"; s=$?
if [[ "$1" == show && -n "${BLOB_FILE:-}" && -f "$BLOB_FILE" ]] \
   && grep -qF 'SECOND-FILE-MARKER' "$BLOB_FILE"; then rm -f "$BLOB_FILE"; fi
exit $s
STUB
chmod +x "$git_stub/git"
# `mktemp` hands out predictable names so the stub above can name the blob file exactly;
# `finding_file_count` takes the candidate list first and the blob second.
mktemp_stub="$tmp/mktemp-stub"
mkdir -p "$mktemp_stub"
cat > "$mktemp_stub/mktemp" <<'STUB'
#!/usr/bin/env bash
(($# == 0)) || exec "$REAL_MKTEMP" "$@"
n=$(( $(cat "$MK_COUNT") + 1 )); echo "$n" > "$MK_COUNT"
p="$MK_DIR/ft.$n"; : > "$p"; echo "$p"
STUB
chmod +x "$mktemp_stub/mktemp"
mkdir -p "$tmp/ftmp"
blob_status=0
got="$(
  export REAL_GIT="$(command -v git)" REAL_MKTEMP="$(command -v mktemp)"
  export MK_COUNT="$tmp/ftmp/count" MK_DIR="$tmp/ftmp" BLOB_FILE="$tmp/ftmp/ft.2"
  echo 0 > "$MK_COUNT"
  export PATH="$git_stub:$mktemp_stub:$PATH"
  hash -r
  cd "$dup_repo" && finding_file_count G-TWICE HEAD 2>/dev/null
)" || blob_status=$?
((blob_status != 0)) \
  || error "MUT-FINDING-BLOB-OPEN-AS-MISS: a count whose blob could not be opened succeeded, got [$got]"
[[ "$got" == 1 ]] \
  && error "MUT-FINDING-BLOB-OPEN-AS-MISS: two files filing one id counted as one"

# A LISTING THAT ENDED IS NOT A LISTING THAT WAS READ. `read` returns non-zero at end of input and
# on a failed read alike, and the loop ends on either, so a list written whole and read half way
# through printed the entries that arrived as though they were all of them: one file, no
# `duplicate-file`, READY. `git ls-tree`'s status says the list was WRITTEN and cannot see it.
#
# The injection shortens the list under the loop -- the stub `git show` for the first candidate
# truncates it to its first NUL-terminated record -- so the loop reads one entry and then an end
# of input that is not the end of the list. A read that fails outright (EIO on the descriptor) is
# the same ending and is witnessed on the pull request; this is the shape a gate can inject with
# nothing but coreutils.
cat > "$git_stub/git" <<'STUB'
#!/usr/bin/env bash
"$REAL_GIT" "$@"; s=$?
if [[ "$1" == show && -n "${CAND_FILE:-}" && -f "$CAND_FILE" ]]; then
  keep="$(tr '\0' '\n' < "$CAND_FILE" | head -1 | wc -c)"    # the first record and its separator
  truncate -s "$keep" "$CAND_FILE"
fi
exit $s
STUB
short_status=0
got="$(
  export REAL_GIT="$(command -v git)" REAL_MKTEMP="$(command -v mktemp)"
  export MK_COUNT="$tmp/ftmp/count" MK_DIR="$tmp/ftmp" CAND_FILE="$tmp/ftmp/ft.1"
  echo 0 > "$MK_COUNT"
  export PATH="$git_stub:$mktemp_stub:$PATH"
  hash -r
  cd "$dup_repo" && finding_file_count G-TWICE HEAD 2>/dev/null
)" || short_status=$?
((short_status != 0)) \
  || error "MUT-FINDING-LIST-SHORT-READ: a count over a list that stopped arriving succeeded, got [$got]"
[[ "$got" == 1 ]] \
  && error "MUT-FINDING-LIST-SHORT-READ: two files filing one id counted as one"
# The same stubs with nothing shortened still count both files, so the two cases above are about
# the reads and not about the stubs.
expect MUT-FINDING-LIST-SHORT-READ "$(
  export REAL_GIT="$(command -v git)" REAL_MKTEMP="$(command -v mktemp)"
  export MK_COUNT="$tmp/ftmp/count" MK_DIR="$tmp/ftmp"
  echo 0 > "$MK_COUNT"
  export PATH="$git_stub:$mktemp_stub:$PATH"
  hash -r
  cd "$dup_repo" && finding_file_count G-TWICE HEAD
)" 2

# WHAT GIT RECORDS DECIDES WHAT AN ENTRY IS. A committed symlink is a `120000` blob whose content
# is its target string, and `git show` hands that string over exactly as it hands over a file's
# text -- so a broken link named like a finding, pointing at `---\nid: X\n---`, filed a finding
# that had never been written. The entry is built through the index rather than with `ln -s`, so
# the case is the same one wherever this suite is run by hand.
link_repo="$tmp/link-repo"
mkdir -p "$link_repo/reviews/findings"
git init -q "$link_repo"
link_blob="$(printf -- '---\nid: SYMLINK-ID\n---\n' | git -C "$link_repo" hash-object -w --stdin)"
git -C "$link_repo" update-index --add --cacheinfo "120000,$link_blob,reviews/findings/symlink.md"
git -C "$link_repo" -c user.email=t@example -c user.name=t commit -qm "commit a link named like a finding"
expect MUT-FINDING-SYMLINK-AS-FILE "$(git -C "$link_repo" ls-tree -r HEAD -- reviews/findings/ | cut -c1-6)" 120000
expect MUT-FINDING-SYMLINK-AS-FILE "$( (cd "$link_repo" && finding_file_count SYMLINK-ID HEAD) )" 0
# The regular file beside it still counts, so the mode filter is a filter and not a refusal. The
# new file is staged BY PATH: `add -A` would see no `symlink.md` in a work tree that never had one
# and stage its deletion, and the case under test would leave the tree it is testing.
printf -- '---\nid: REGULAR-ID\n---\n\nBody.\n' > "$link_repo/reviews/findings/regular.md"
git -C "$link_repo" add -- reviews/findings/regular.md
git -C "$link_repo" -c user.email=t@example -c user.name=t commit -qm "file one finding properly"
expect MUT-FINDING-SYMLINK-AS-FILE \
  "$(git -C "$link_repo" ls-tree -r HEAD -- reviews/findings/ | cut -c1-6 | sort -u | tr '\n' ' ')" "100644 120000 "
expect MUT-FINDING-SYMLINK-AS-FILE "$( (cd "$link_repo" && finding_file_count REGULAR-ID HEAD) )" 1
expect MUT-FINDING-SYMLINK-AS-FILE "$( (cd "$link_repo" && finding_file_count SYMLINK-ID HEAD) )" 0

# --- the newest check run per name -------------------------------------------------------------
got="$(printf 'upstroke-ci\t100\tsuccess\nupstroke-ci\t250\tfailure\nupstroke-pr-policy\t120\tsuccess\nupstroke-ci\t90\tsuccess\n' | newest_per_name | tr ' ' '\n' | grep . | sort | tr '\n' ' ')"
expect MUT-CHECK-RUN-FIRST-SUCCESS "$got" "upstroke-ci=failure upstroke-pr-policy=success "
got="$(printf 'upstroke-ci\t300\tsuccess\nupstroke-ci\t200\tcancelled\n' | newest_per_name)"
expect MUT-CHECK-RUN-UNSTARTED "$got" "upstroke-ci=success "
got="$(printf 'upstroke-ci\t1000\tqueued\nupstroke-ci\t999\tsuccess\n' | newest_per_name)"
expect MUT-CHECK-RUN-UNSTARTED "$got" "upstroke-ci=queued "

# --- the ledger rows ---------------------------------------------------------------------------
cat > "$tmp/body.md" <<'EOF'
## Summary

Text with a | pipe.

## Review finding ledger

| ID | Severity | Reviewed SHA / location | Failure sequence | Provenance | Category | First bad / prior ID | Regression or documented guard | Disposition |
|---|---|---|---|---|---|---|---|---|
| A-DEFERRABLE | P3 | 4ad962f / src/x.rs:1 | a -> b | pre_existing | correctness | — | `guard` | deferred |
| B-FIXED | P2 | 4ad962f / src/y.rs:2 | a -> b | introduced_by_feature | liveness | — | `test` | fixed |

## Risk and rollback

| not | a | ledger | row |
EOF
got="$(ledger_rows_from_body < "$tmp/body.md" | tr '\t' '|')"
want='A-DEFERRABLE|P3|4ad962f / src/x.rs:1|deferred
B-FIXED|P2|4ad962f / src/y.rs:2|fixed'
expect MUT-LEDGER-HEADER-AS-ROW "$got" "$want"

if ((failed)); then
  echo "test-pr-ready-audit: FAILED" >&2
  exit 1
fi
echo "test-pr-ready-audit: ok"
