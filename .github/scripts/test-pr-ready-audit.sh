#!/usr/bin/env bash
# The pure parts of scripts/pr-ready-audit.sh, exercised against fixtures: the lane and its
# severity set, the two review parsers (the workflow's fenced JSON verdict and the frontier
# prose form), the frontmatter id match, the newest-check-run choice and the ledger-row parse.
# Everything that talks to GitHub or git is out of scope here, with one exception: the argument
# parser is exercised by running the script as a process against a stub `gh`, because the defects
# it has had live in `main` and no call on the helpers can see them. The stub answers every call
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
expect MUT-REVIEWER-FROM-ORG "$(reviewer_login "" eventloops User)" eventloops
# An Organization owner authors no comments. Inheriting it yields a filter that matches nothing,
# which is not a stricter audit but a blind one, so it must fail rather than return a login.
if reviewer_login "" sourcemaps Organization > "$tmp/org.out" 2>&1; then
  error "MUT-REVIEWER-FROM-ORG: an Organization owner was accepted as the reviewer, got [$(cat "$tmp/org.out")]"
fi
expect MUT-REVIEWER-FROM-ORG "$(cat "$tmp/org.out")" ""
# An explicit override wins over either, and is the only way to audit an organization-owned repo.
expect MUT-REVIEWER-OVERRIDE-IGNORED "$(reviewer_login eventloops sourcemaps Organization)" eventloops
expect MUT-REVIEWER-OVERRIDE-IGNORED "$(reviewer_login someone-else eventloops User)" someone-else
# A missing login is not a reviewer either, whatever the type claims.
if reviewer_login "" "" User > "$tmp/empty.out" 2>&1; then
  error "MUT-REVIEWER-FROM-ORG: an empty owner login was accepted as the reviewer"
fi
# The helper is the last gate before a string becomes the audit's notion of who may say PASS, and
# what it returns is put to a jq program, so it takes GitHub's login shape and nothing wider. Each
# of these is a typo, another option read by mistake, or an injection attempt; none is an account.
for bad in 'eventloops" or true or .user.login == "eventloops' '--enqueue' '-abc' 'abc-' 'a--b' \
           'github-actions[bot]' 'two words' 'a.b' 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'; do
  if reviewer_login "$bad" eventloops User > "$tmp/bad.out" 2>&1; then
    error "MUT-REVIEWER-JQ-INJECTION: [$bad] was accepted as a login, got [$(cat "$tmp/bad.out")]"
  fi
done
# and nothing narrower: a real login, in any case, of any allowed length, is not refused.
for good in eventloops EventLoops a-b-c x 0 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa; do
  expect MUT-REVIEWER-JQ-INJECTION "$(reviewer_login "$good" sourcemaps Organization)" "$good"
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
  *"/comments?per_page=100"*) exit "${STUB_COMMENTS_STATUS:-1}" ;;
  *"--json body"*)            echo "no ledger" ;;
  "pr view"*)                 printf 'feature/x\n%s\nfalse\nCLEAN\n\nmaster\n%s\n' "$head" "$head" ;;
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
P3|E.DOTTED|0'
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
P1|SECOND|0'
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
STRAY|MUST/P1|0'
expect "MUT-PROSE-HEADING-FINDING/MUT-PROSE-LAST-VERDICT" "$got" "$want"

# A prose review that quotes a JSON object stays prose.
printf 'Reviewed head: %s\nThe object {"verdict":"PASS","findings":[]} is an example.\nVERDICT: CHANGES_REQUIRED\n' \
  "4ad962f000000000000000000000000000000001" > "$tmp/quoted.md"
expect MUT-QUOTED-JSON-IS-JSON "$(review_kind "$tmp/quoted.md")" prose

# --- the frontmatter id match ------------------------------------------------------------------
printf -- '---\nid: OTHER-ID\nseverity: P2\n---\n\nThe prose below repeats a line.\nid: TARGET-ID\n' > "$tmp/prose-id.md"
printf -- '---\nid: TARGET-ID\nseverity: P2\n---\n\nBody.\n' > "$tmp/front-id.md"
printf -- '---\nid: TARGETXID\n---\n' > "$tmp/x-id.md"
printf 'id: TARGET-ID\n---\nno opening fence\n' > "$tmp/no-front.md"
frontmatter_has_id TARGET-ID < "$tmp/prose-id.md" && error "MUT-FRONTMATTER-BY-SUBSTRING: an id in prose matched"
frontmatter_has_id TARGET-ID < "$tmp/front-id.md" || error "MUT-FRONTMATTER-BY-SUBSTRING: the frontmatter id did not match"
frontmatter_has_id TARGET.ID < "$tmp/x-id.md" && error "MUT-FRONTMATTER-PATTERN: a dot matched as a regex"
frontmatter_has_id TARGET-ID < "$tmp/no-front.md" && error "MUT-FRONTMATTER-BY-SUBSTRING: a file without a frontmatter block matched"

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
