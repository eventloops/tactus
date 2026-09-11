#!/usr/bin/env bash
# The pure parts of scripts/pr-ready-audit.sh and the whole of scripts/pr-review-parse.py,
# exercised against fixtures: the lane and its severity set, both review forms (the workflow's
# fenced JSON verdict and the frontier prose form) and the ledger, all four of which the one
# parser reads; the reader that takes its payload; the state a set of blockers puts a pull request
# in; the frontmatter id match and the newest-check-run choice.
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
#   MUT-PROSE-READ-SUPPRESSED    a read that failed inside the parse was reported as "nothing
#                                matched", and the completeness marker was printed over it -- so a
#                                prose review carrying a P3 under a PASS parsed as a clean PASS
#                                with no findings. There is no read inside the parse for a status
#                                to be suppressed on now: the input is read once, and a read that
#                                failed is a parse that failed
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
#   MUT-PARSE-WRITE-UNCHECKED    a protocol write returned EIO and the parse reported success, so
#                                one finding vanished from a findings list that still announced
#                                itself complete -- READY, and a merge call
#   MUT-PARSE-PAYLOAD-COUNT      the payload the audit reads did not declare how many records it
#                                carries, so a read that stopped part way was a shorter list
#   MUT-MANUAL-SCAN-FAILS-OPEN   the manual-blocker test was a command in a condition, which has
#                                two ways to be false: the answer, and a failure. `grep` exiting 2
#                                left state=READY with the blocker printed beside it
#   MUT-LEDGER-LOOKUP-SUPPRESSED a pull-request body the audit could not fetch was read as a body
#                                with no ledger rows in it, which is `no-row` for a filed finding
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
#                                object spells with a JSON escape. Detection is inside the one
#                                parser now, so a file it cannot read has no format at all
#   MUT-RETARGET-ANSWER-UNCHECKED  a helper's ANSWER is a write, and the write's failure was
#                                discarded: `{ echo yes; return 0; }` left the caller an empty
#                                string and a status of 0, `[[ "$x" == yes ]]` read that as "the
#                                base did not change", and a pull request retargeted since its
#                                review enqueued. The same case covers every channel that carries
#                                an answer back: the retarget `yes`/`no`, the ruleset's two flags,
#                                the comment id, and the lane's severity set -- each names its
#                                permissive answer and blocks on everything else
#   MUT-VERDICT-REVIVED-BY-TRUNCATION  the workflow form's verdict was the last block THAT PARSED
#                                rather than the last block, so a final object missing its closing
#                                fence or its final `}` was stepped over and the `PASS` quoted
#                                above it as an example became the verdict -- with the real
#                                object's `"P\u0031"` invisible to the stray scan
#   MUT-VERDICT-REVIVED-BY-INDENT  the verdict block was recognised by `^```json`, which is not
#                                CommonMark's rule: a fence may carry up to three leading spaces
#                                and its closing fence may be indented independently. One space
#                                before the real `CHANGES_REQUIRED` object and it stopped being a
#                                block at all, so the `PASS` quoted above it as an example became
#                                the verdict -- READY and a merge call, with `"P\u0031"` invisible
#                                to the stray scan. The case pins the recogniser AND the rule that
#                                stands when a recogniser is wrong again: the verdict is the
#                                comment's LAST block, and nothing that could be another block's
#                                material may follow it
#   MUT-VERDICT-AMBIGUOUS-BLOCK-CHOSEN  the parse CHOSE between the places a verdict could be read
#                                from instead of refusing a comment that offers more than one.
#                                Three spellings of one fenced block, each making the WRONG block
#                                the one that was read: `{ "role_understanding"` with a space
#                                after the brace, which the tail check's enumeration of "block
#                                material" did not list; the real block inside a blockquote's
#                                `> `, which the recogniser did not see at all, so format
#                                detection fell back to prose and took the `VERDICT: PASS` written
#                                outside it; and a second block hidden inside an HTML comment,
#                                which a reader never sees and the scanner read as the last block.
#                                All three reached READY, exit 0 and one merge call out of a
#                                review carrying a P1 spelled `"P\u0031"`, which the stray scan
#                                cannot see either. A recogniser made cleverer closes the spelling
#                                in front of it; the count closes the class: EXACTLY ONE place a
#                                verdict may be read from, every fence run in the comment
#                                accounted for by the structure scan, nothing but whitespace after
#                                the block, no `VERDICT:` line outside it, and the form taken from
#                                the comment's marker rather than from whichever parser answers
#   MUT-PARSE-SHORT-WRITE        `write()` returned a smaller count than the payload and the count
#                                was discarded: under `PYTHONUNBUFFERED=1`, with `RLIMIT_FSIZE` at
#                                1,024 and a 3,560-byte result, `write(1, ..., 3560) = 1024` and
#                                the program exited 0 over a findings list 2,536 bytes short of
#                                itself, in both renderings
#   MUT-STAGING-PATH-SHARED      the staging pathname was derived from the destination, so every
#                                invocation publishing to one destination used one file: two of
#                                them overlapping renamed one's bytes under the other's exit 0
#   MUT-HERESTRING-FAILURE-AS-ANSWER  a value was fed to a command through `<<<`, which spills to
#                                a temporary file once it outgrows a pipe buffer: a file bash
#                                cannot create is a redirection that failed, the command never
#                                runs, and the shell returns 1 -- `grep`'s "no match". A compound
#                                command is skipped outright, where neither a captured status nor
#                                `set -e` can see it, and `read` or `mapfile` leaves what it was
#                                to fill holding whatever it held before. The guard is the class:
#                                the script carries no here-string and no here-document at all
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
# A lane this does not know is not a lane with nothing to fix in it: `[[ "  " == *" P1 "* ]]` is
# false, so an empty must-fix set makes every severity in every review deferrable. The permissive
# answer is said, never fallen into.
for unknown in "" lane:feature findings feature-ish FEATURE; do
  if must_fix_for "$unknown" > "$tmp/lane.out" 2>&1; then
    error "MUT-P3-LANE-DEFERS: lane [$unknown] was answered with [$(cat "$tmp/lane.out")]"
  fi
done

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
empty_path_early="$tmp/empty-path-early"
mkdir -p "$empty_path_early"
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

# --- the answer itself is a write, and a write is a thing that can fail --------------------------
# THE LOOKUP SUCCEEDING IS NOT THE ANSWER ARRIVING. `{ echo yes; return 0; }` discarded the status
# of the one write that carries the blocking answer out of this helper: with that write failing,
# the helper exited 0 having said nothing, the caller's `[[ "$retargeted" == yes ]]` read the empty
# string as "the base did not change", and a pull request retargeted since its review was enqueued
# -- exit 0, one merge call. `/dev/full` is a real device whose every write returns ENOSPC, so this
# is the failure itself and not a stand-in for it.
full_status=0
( PATH="$timeline:$PATH"; repo=o/r; base_changed_after 999 2026-09-01T00:00:00Z > /dev/full ) 2>/dev/null \
  || full_status=$?
((full_status != 0)) \
  || error 'MUT-RETARGET-ANSWER-UNCHECKED: a [yes] whose write failed was reported as an answer'
# The other branch is held to the same rule, so the case above is about the write and not about
# which answer it was carrying.
full_status=0
( PATH="$timeline:$PATH"; repo=o/r; base_changed_after 999 2026-09-09T00:00:00Z > /dev/full ) 2>/dev/null \
  || full_status=$?
((full_status != 0)) \
  || error 'MUT-RETARGET-ANSWER-UNCHECKED: a [no] whose write failed was reported as an answer'

# AND AN ANSWER THAT ARRIVED IS ONE OF TWO WORDS. Everything else -- the empty string a partial or
# discarded write leaves behind most of all -- used to fall into `no`, which is the answer that
# lets a merge happen. The permissive answer has to be said. `retarget_blocker` sets a variable and
# runs no command, for the reason `audit_state` does, so these cases run it with an EMPTY PATH:
# a rewrite that reaches for an external command fails here whatever it reaches for.
blocker_of() {  # blocker_of ANSWER: the blocker that answer calls for, or nothing
  retarget_why="not-set"
  retarget_blocker "$1"
  printf '%s' "$retarget_why"
}
expect MUT-RETARGET-ANSWER-UNCHECKED "$(PATH="$empty_path_early" blocker_of no)" ""
expect MUT-RETARGET-ANSWER-UNCHECKED "$(PATH="$empty_path_early" blocker_of yes)" retargeted-after-review
for junk in "" " " YES Yes yes-ish "yes " " yes" no-ish "no " 0 1 true false; do
  expect "MUT-RETARGET-ANSWER-UNCHECKED [$junk]" "$(PATH="$empty_path_early" blocker_of "$junk")" \
    timeline-answer-unreadable
done

if (PATH="$failing:$PATH"; repo=o/r; ruleset_state) > "$tmp/rs.out" 2>&1; then
  error "MUT-RULESET-LOOKUP-SUPPRESSED: an unreadable ruleset list was answered, got [$(cat "$tmp/rs.out")]"
fi
expect MUT-RULESET-LOOKUP-SUPPRESSED "$( (PATH="$silent:$PATH"; repo=o/r; ruleset_state) )" "0 0"
# The same helper's answer is also a write, and its caller splits it by expansion: `${x%% *}` on
# anything but two flags and a space yields a `strict_up_to_date` that `((...))` reads as 0, which
# drops the BEHIND blocker for every pull request in the run.
ruleset_full_status=0
( PATH="$silent:$PATH"; repo=o/r; ruleset_state > /dev/full ) 2>/dev/null || ruleset_full_status=$?
((ruleset_full_status != 0)) \
  || error "MUT-RETARGET-ANSWER-UNCHECKED: a ruleset state whose write failed reported success"

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
# The guard used to name the two shapes that had gone wrong -- `grep ... <<<` and `done <<<` --
# and each round found a third. `read ... <<<` leaves the variables it was to fill holding what
# they held before; `mapfile ... <<<` leaves the array empty, which for a list of pull requests is
# a run that audits nothing and exits 0; `$(wc -w <<< ...)` leaves an arithmetic test with nothing
# in it. So the class is named instead of its members: THIS SCRIPT CONTAINS NO HERE-STRING AND NO
# HERE-DOCUMENT AT ALL. Lines are walked by expansion, a fixed program text is written with
# `printf`, and a read that needs a file gets a real one whose write is checked.
expect MUT-HERESTRING-FAILURE-AS-ANSWER \
  "$(code_lines | grep -E '<<' | sed 's/^[[:space:]]*//' | tr '\n' ';')" ''

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
  *"/comments?per_page=100"*) [[ -n "${STUB_REVIEW_BODY:-}" ]] && echo "2026-09-01T00:00:00Z ${STUB_REVIEW_ID:-5001}"
                              exit "${STUB_COMMENTS_STATUS:-1}" ;;
  *"/issues/comments/5001 --jq .created_at") echo 2026-09-01T00:00:00Z ;;
  *"/issues/comments/5001 --jq .body")       cat "$STUB_REVIEW_BODY" ;;
  *"--json body"*)            (( ${STUB_BODY_STATUS:-0} )) && exit "$STUB_BODY_STATUS"
                              echo "no ledger" ;;
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

# The third answer that channel can carry. A comment id is a number, and the two API paths built
# from it are the last thing it passes through; a value that is neither empty nor a number is a
# lookup that came back with something nobody checked, and it is not a review to go and fetch.
printf '```json\n{"verdict":"PASS","findings":[]}\n```\n' > "$tmp/id-check-review.md"
for bad_id in "not-a-number" "-5001" "5001;x" "P1" "0x10" "5001.0"; do
  got="$(STUB_REVIEW_BODY="$tmp/id-check-review.md" STUB_REVIEW_ID="$bad_id" run_lookup 0)"
  contains "MUT-RETARGET-ANSWER-UNCHECKED id [$bad_id]" "$got" "review-id-unreadable"
  contains "MUT-RETARGET-ANSWER-UNCHECKED id [$bad_id]" "$got" "NOT-READY"
  [[ "$got" == *GH-UNSTUBBED* ]] \
    && error "MUT-RETARGET-ANSWER-UNCHECKED: the audit fetched a comment id it could not read: [$got]"
done
# and a number still reaches the fetch, so the case above is about the shape of the id.
got="$(STUB_REVIEW_BODY="$tmp/id-check-review.md" run_lookup 0)"
[[ "$got" == *review-id-unreadable* ]] \
  && error "MUT-RETARGET-ANSWER-UNCHECKED: a numeric comment id was refused"

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

# A ruleset state that is not two flags is not a ruleset state. `ruleset_state` can only write the
# four, so this drives `main` with the helper replaced -- which is what a partial write, or a
# rewrite of that helper, would leave it holding. `${rulesets%% *}` on anything else yields a
# `strict_up_to_date` that `((...))` reads as 0, and that drops the BEHIND blocker for every pull
# request in the run, so the value is checked whole before it is split.
ruleset_answer_run() {  # ruleset_answer_run ANSWER: "<status>|<output>" with ruleset_state saying ANSWER
  local out status=0
  out="$(
    RULESET_ANSWER="$1"
    ruleset_state() { printf '%s\n' "$RULESET_ANSWER"; }
    PATH="$lookup:$PATH" main --reviewer eventloops 999 2>&1
  )" || status=$?
  printf '%s|%s' "$status" "$(printf '%s' "$out" | tr '\n' ' ')"
}
for bad_rulesets in "" "0" "1" "0 0 0" "yes no" "0  0" " 0 0"; do
  got="$(ruleset_answer_run "$bad_rulesets")"
  contains "MUT-RETARGET-ANSWER-UNCHECKED rulesets [$bad_rulesets]" "$got" \
    "2|refusing: eventloops/upstroke's ruleset state read back as [$bad_rulesets]"
  [[ "$got" == *"#999"* ]] \
    && error "MUT-RETARGET-ANSWER-UNCHECKED: the audit judged a pull request on a ruleset state it could not read"
done
# The four real answers still run, so the case above is about the shape and not about the check.
for good_rulesets in "0 0" "0 1" "1 0" "1 1"; do
  got="$(ruleset_answer_run "$good_rulesets")"
  contains "MUT-RETARGET-ANSWER-UNCHECKED rulesets [$good_rulesets]" "$got" "#999"
  [[ "$got" == *"ruleset state read back"* ]] \
    && error "MUT-RETARGET-ANSWER-UNCHECKED: [$good_rulesets] was refused as a ruleset state"
done

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

# A parse that could not be completed, through main. The old parser printed tab-separated rows and
# an `END` row to say the stream was whole, and a review's own strings could write an `END`: one
# recording `base_sha: "<a real base commit>\nEND\t-\t0"` printed the completeness marker itself,
# the parser died on the next field, and the audit read a finished parse and a clean PASS out of a
# process that exited 1. There is no marker to forge now -- completeness is the exit status, plus
# a record count the parser computed -- and `PYTHONIOENCODING=ascii`, the environment that used to
# kill the parser halfway through its output, no longer reaches the output at all: the result is
# rendered to bytes this program encodes itself.
#
# So the same two reviews are run here for the opposite assertion. Each must parse WHOLE, keep its
# finding, and block for having one; the review that writes `END` into its own base must have that
# base refused as a commit and nothing else.
printf '```json\n{"verdict":"PASS","findings":[{"id":"A-DEFERRABL\xc3\x89","severity":"P3"}]}\n```\n' \
  > "$tmp/truncating-review.md"
got="$(STUB_REVIEW_BODY="$tmp/truncating-review.md" STUB_COMMENTS_STATUS=0 PYTHONIOENCODING=ascii run_stub 999)"
contains MUT-REVIEW-PARSE-TRUNCATED "$got" "NOT-READY"
contains MUT-REVIEW-PARSE-TRUNCATED "$got" "pass-with-findings"
[[ "$got" == *review-parse-incomplete* ]] \
  && error "MUT-REVIEW-PARSE-TRUNCATED: a whole parse was reported as truncated"
[[ "$got" == *review-parse-failed* ]] \
  && error "MUT-REVIEW-PARSE-TRUNCATED: a whole parse was reported as failed"
# The same review with the environment left alone, so the case above is about the encoding and not
# about the review.
got="$(STUB_REVIEW_BODY="$tmp/truncating-review.md" STUB_COMMENTS_STATUS=0 run_stub 999)"
contains MUT-REVIEW-PARSE-TRUNCATED "$got" "pass-with-findings"
contains MUT-META-FIELD-COLLAPSE "$got" "review-records-no-reviewed-sha"

printf '```json\n{"base_sha":"%s\\nEND\\t-\\t0","verdict":"PASS","findings":[{"id":"A-DEFERRABL\xc3\x89","severity":"P3"}]}\n```\n' \
  5157509000000000000000000000000000000002 > "$tmp/forging-review.md"
got="$(STUB_REVIEW_BODY="$tmp/forging-review.md" STUB_COMMENTS_STATUS=0 PYTHONIOENCODING=ascii run_stub 999)"
contains MUT-REVIEW-FORGES-PROTOCOL "$got" "NOT-READY"
contains MUT-REVIEW-FORGES-PROTOCOL "$got" "pass-with-findings"
[[ "$got" == *review-parse-incomplete* ]] \
  && error "MUT-REVIEW-FORGES-PROTOCOL: a forged marker shortened the parse"

# --- one parser, one success condition ----------------------------------------------------------
# Everything below runs `scripts/pr-review-parse.py`, which is what the audit runs. Two readings of
# every case: the JSON rendering, which says what the parser decided, and the NUL rendering, which
# is the payload the audit actually reads.
command -v python3 > /dev/null || command -v python > /dev/null \
  || { echo "test-pr-ready-audit: needs python3 or python to run the review parser" >&2; exit 1; }
parser_python="$(command -v python3 || command -v python)"
review_rows() {  # review_rows FILE: "<status>|<kind>/<head>/<verdict>/<base>/<stray>[;<sev>:<id>:<flags>]..."
  local out status=0
  out="$("$parser_python" scripts/pr-review-parse.py review "$1" 2>/dev/null)" || status=$?
  ((status == 0)) || { printf '%s|' "$status"; return 0; }
  printf '%s|%s' "$status" "$(
    printf '%s' "$out" | jq -j '"\(.kind)/\(.reviewed_sha//"-")/\(.verdict//"-")/\(.base_sha//"-")/\(.stray//"-")",
      ([.findings[] | ";\(.severity):\(.id//"-"):\(.flags)"] | add // "")'
  )"
}
parse_nul() {  # parse_nul SUBCOMMAND FILE [OUT]: "<status>|<the payload, NULs shown as |>"
  local out="${3:-$tmp/nul.out}" status=0
  : > "$out"
  "$parser_python" scripts/pr-review-parse.py "$1" --nul --out "$out" "$2" 2>/dev/null || status=$?
  printf '%s|%s' "$status" "$(tr '\0' '|' < "$out")"
}

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
expect "MUT-JSON-SPLIT-BY-REGEX/MUT-NULL-WITNESS/MUT-MUST-UNSEEN/MUT-BAD-SEVERITY-PASSES/MUT-STRAY-TOKEN-UNSEEN/MUT-VERDICT-FROM-PROSE/MUT-QUOTED-JSON-IS-JSON" \
  "$(review_rows "$tmp/json.md")" \
  '0|json/4ad962f000000000000000000000000000000001/CHANGES_REQUIRED/5157509000000000000000000000000000000002/P1;P3:A-DEFERRABLE:0;P2:B-WITNESSED:1;P2:C-MUST:2;ERR:bad-severity:D-BAD:0;P3:E.DOTTED:0'
# The same result as the audit reads it: seven fields, the seventh the finding count, then three
# per finding. The count is the parser's own and the audit checks the payload against it, so a
# payload that stopped arriving is not a shorter findings list.
expect MUT-PARSE-PAYLOAD-COUNT "$(parse_nul review "$tmp/json.md")" \
  '0|review|json|4ad962f000000000000000000000000000000001|CHANGES_REQUIRED|5157509000000000000000000000000000000002|P1|5|P3|A-DEFERRABLE|0|P2|B-WITNESSED|1|P2|C-MUST|2|ERR|bad-severity:D-BAD|0|P3|E.DOTTED|0|'

# --- an incomplete final verdict is not a licence to take the previous one -----------------------
# THE LAST BLOCK IS THE VERDICT, AND WHETHER IT IS WHOLE IS ASKED AFTER IT HAS BEEN IDENTIFIED.
# The pattern this replaces matched only blocks that HOLD A WHOLE OBJECT and then took the last of
# those, which is the two operations in the wrong order: with the final closing fence removed, or
# the final `}`, the real verdict was not a match at all and `[-1]` named the `PASS` quoted above
# it as an example. The severity is written `P\u0031`, as the review that found this wrote it,
# because a JSON escape carries no `P1` for the stray scan to catch -- so nothing else was left to
# block: the parser reported PASS with zero findings and exit 0, and the audit READY with a merge
# call. A truncated review has no verdict; it does not have its previous verdict.
revived_head=4ad962f000000000000000000000000000000001
revived_base=5157509000000000000000000000000000000002
example_pass() {  # example_pass: the clean `PASS` block these revivals all reached back to
  printf 'An earlier pass, kept as an example of the shape:\n\n'
  printf '```json\n{"reviewed_sha":"%s","base_sha":"%s","verdict":"PASS","findings":[]}\n```\n\n' \
    "$revived_head" "$revived_base"
}
{ printf 'Reviewed head: %s\n\nUnedited verdict:\n\n```json\n' "$revived_head"
  printf '{"reviewed_sha":"%s","base_sha":"%s","verdict":"CHANGES_REQUIRED","findings":[{"id":"CRITICAL","severity":"P\\u0031"}]}' \
    "$revived_head" "$revived_base"
  printf '\n'; } > "$tmp/revived-body.txt"
{ cat "$tmp/revived-body.txt"; printf '```\n'; } > "$tmp/revived-whole.md"
cp "$tmp/revived-body.txt" "$tmp/revived-no-fence.md"
{ head -c -2 "$tmp/revived-body.txt"; printf '\n```\n'; } > "$tmp/revived-no-brace.md"
# Whole, the real verdict is the one that counts and its escaped severity is a finding.
expect MUT-VERDICT-REVIVED-BY-TRUNCATION "$(review_rows "$tmp/revived-whole.md")" \
  "0|json/$revived_head/CHANGES_REQUIRED/$revived_base/-;P1:CRITICAL:0"
# Cut either way, there is no verdict at all -- not the one above it.
for cut in no-fence no-brace; do
  got="$(review_rows "$tmp/revived-$cut.md")"
  [[ "$got" == 0\|* ]] \
    && error "MUT-VERDICT-REVIVED-BY-TRUNCATION [$cut]: a truncated review parsed, got [$got]"
  [[ "$got" == *PASS* ]] \
    && error "MUT-VERDICT-REVIVED-BY-TRUNCATION [$cut]: an earlier PASS was revived, got [$got]"
  # and nothing was written where a caller ignoring the status would read it
  expect "MUT-VERDICT-REVIVED-BY-TRUNCATION [$cut] payload" \
    "$(parse_nul review "$tmp/revived-$cut.md")" '1|'
done
# Through main, because the parser refusing is only half of it: READY and a merge call is what the
# audit did with the revived PASS.
for cut in no-fence no-brace; do
  got="$(STUB_REVIEW_BODY="$tmp/revived-$cut.md" STUB_COMMENTS_STATUS=0 run_stub 999)"
  contains "MUT-VERDICT-REVIVED-BY-TRUNCATION [$cut]" "$got" "review-parse-failed"
  contains "MUT-VERDICT-REVIVED-BY-TRUNCATION [$cut]" "$got" "NOT-READY"
  [[ "$got" == *"verdict=PASS"* ]] \
    && error "MUT-VERDICT-REVIVED-BY-TRUNCATION [$cut]: the audit read PASS out of a truncated review"
done
# The same comment whole still reaches the audit as the blocking review it is, so the cases above
# are about the truncation and not about the fixture.
got="$(STUB_REVIEW_BODY="$tmp/revived-whole.md" STUB_COMMENTS_STATUS=0 run_stub 999)"
contains MUT-VERDICT-REVIVED-BY-TRUNCATION "$got" "open-P1:CRITICAL"
# AND THE SHAPE EVERY ONE OF THESE REVIVALS REACHED BACK INTO: a complete `PASS` example above the
# real verdict. Round 11 got to it through a final block that did not close, round 12 through an
# indented fence, round 13 through a `> ` and through an HTML comment -- four ways to make the
# example the block that was read. The comment offers two places a verdict could be read from, and
# a parse that picks one of them is picking a verdict: there is no result here.
{ example_pass; cat "$tmp/revived-body.txt"; printf '```\n'; } > "$tmp/two-blocks.md"
got="$(review_rows "$tmp/two-blocks.md")"
[[ "$got" == 0\|* ]] \
  && error "MUT-VERDICT-AMBIGUOUS-BLOCK-CHOSEN: a comment offering two verdicts parsed, got [$got]"
[[ "$got" == *PASS* ]] \
  && error "MUT-VERDICT-AMBIGUOUS-BLOCK-CHOSEN: the example PASS was chosen, got [$got]"
expect MUT-VERDICT-AMBIGUOUS-BLOCK-CHOSEN "$(parse_nul review "$tmp/two-blocks.md")" '1|'
got="$(STUB_REVIEW_BODY="$tmp/two-blocks.md" STUB_COMMENTS_STATUS=0 run_stub 999)"
contains MUT-VERDICT-AMBIGUOUS-BLOCK-CHOSEN "$got" "review-parse-failed"
contains MUT-VERDICT-AMBIGUOUS-BLOCK-CHOSEN "$got" "NOT-READY"
[[ "$got" == *"verdict=PASS"* ]] \
  && error "MUT-VERDICT-AMBIGUOUS-BLOCK-CHOSEN: the audit read PASS out of an ambiguous comment"
# A block whose fences the review quotes INSIDE it is still that one block: a `failure_sequence`
# describing a code span carries ``` mid-line, and a close matched anywhere ends the block there.
printf 'Reviewed head: %s\n\n```json\n{"reviewed_sha":"%s","verdict":"CHANGES_REQUIRED","findings":[{"id":"SPAN","severity":"P2","failure_sequence":"append ```a code span``` and a blank line"}]}\n```\n' \
  "$revived_head" "$revived_head" > "$tmp/inner-fence.md"
expect MUT-VERDICT-REVIVED-BY-TRUNCATION "$(review_rows "$tmp/inner-fence.md")" \
  "0|json/$revived_head/CHANGES_REQUIRED/-/-;P2:SPAN:0"

# --- an indented fence is a fence, and the block before the verdict is never a candidate ---------
# THE SAME REVIVAL, ARRIVING THROUGH THE RECOGNISER INSTEAD OF THE COMPLETENESS CHECK. CommonMark
# lets an opening fence carry up to three leading spaces, lets the closing fence be indented
# independently of it, and strips the opening fence's indentation from the content
# (https://spec.commonmark.org/0.31.2/#fenced-code-blocks). `^```json` matched none of that: ONE
# SPACE before the real `CHANGES_REQUIRED` block and it stopped being a block at all, so the `PASS`
# quoted above it as an example was selected -- READY and a merge call out of a review carrying a
# P1, whose `"severity":"P\u0031"` the stray scan cannot see either.
#
# Two things are asserted, and the second is the one that matters. Indented fences are recognised;
# AND the verdict is the comment's LAST block, with nothing that could be another block's material
# after it -- so a fence shape this parser gets wrong AGAIN is a refusal, never the block before it.
indented_body() {  # indented_body OPEN-INDENT CLOSE-INDENT FILE
  local open close
  printf -v open '%*s' "$1" ''
  printf -v close '%*s' "$2" ''
  { printf 'Reviewed head: %s\n' "$revived_head"
    printf '\nUnedited verdict:\n\n%s```json\n' "$open"
    printf '%s{"reviewed_sha":"%s","base_sha":"%s","verdict":"CHANGES_REQUIRED","findings":[{"id":"CRITICAL","severity":"P\\u0031"}]}' \
      "$open" "$revived_head" "$revived_base"
    printf '\n%s```\n' "$close"; } > "$3"
}
# One, two and three spaces, on the opening fence, on the closing fence, and on the two
# independently: every one of them is the real verdict, and none of them is the example above it.
for spec in 0:0 1:1 2:2 3:3 1:0 3:0 0:1 0:3 1:3 3:1; do
  indented_body "${spec%%:*}" "${spec##*:}" "$tmp/indent-$spec.md"
  expect "MUT-VERDICT-REVIVED-BY-INDENT [open ${spec%%:*}, close ${spec##*:}]" \
    "$(review_rows "$tmp/indent-$spec.md")" \
    "0|json/$revived_head/CHANGES_REQUIRED/$revived_base/-;P1:CRITICAL:0"
done
# Through main, because the parser reading it right is only half of it: READY with a merge call is
# what the audit did with the revived PASS.
for spec in 1:1 2:2 3:3; do
  got="$(STUB_REVIEW_BODY="$tmp/indent-$spec.md" STUB_COMMENTS_STATUS=0 run_stub 999)"
  contains "MUT-VERDICT-REVIVED-BY-INDENT [$spec]" "$got" "open-P1:CRITICAL"
  contains "MUT-VERDICT-REVIVED-BY-INDENT [$spec]" "$got" "NOT-READY"
  [[ "$got" == *"verdict=PASS"* ]] \
    && error "MUT-VERDICT-REVIVED-BY-INDENT [$spec]: the audit read PASS out of an indented verdict"
done
# A fence longer than three backticks opens and closes a block, and a closing fence must be at
# least as long as the one it closes -- so a shorter run inside the block is content, and a block
# the comment never closes is a failed parse rather than the block before it.
printf 'Reviewed head: %s\n\n````json\n{"reviewed_sha":"%s","verdict":"CHANGES_REQUIRED","findings":[{"id":"LONG","severity":"P2","failure_sequence":"a line reading ``` on its own"}]}\n````\n' \
  "$revived_head" "$revived_head" > "$tmp/long-fence.md"
expect MUT-VERDICT-REVIVED-BY-INDENT "$(review_rows "$tmp/long-fence.md")" \
  "0|json/$revived_head/CHANGES_REQUIRED/-/-;P2:LONG:0"
# THE RULE THAT DOES NOT DEPEND ON THE RECOGNISER. A block that follows the verdict is not a
# reason to reach back past it, whatever that block is -- and with the example `PASS` sitting above
# it, reaching back is exactly what there is to catch.
{ example_pass; cat "$tmp/indent-0:0.md"
  printf '\n```text\nA note appended under the verdict.\n```\n'; } > "$tmp/block-after.md"
got="$(review_rows "$tmp/block-after.md")"
[[ "$got" == 0\|* ]] \
  && error "MUT-VERDICT-REVIVED-BY-INDENT: a comment whose last block is not its verdict parsed, got [$got]"
[[ "$got" == *PASS* ]] \
  && error "MUT-VERDICT-REVIVED-BY-INDENT: an earlier PASS was revived past a later block, got [$got]"
# And a `PASS` written in every spelling of a block this parser is NOT sure about, which is the
# case that has to hold when the recogniser is wrong again. Four spaces is CommonMark's indented
# code block and not a fence; a tilde fence is a fence this comment's own fences are not; the bare
# form's object opener is neither -- AND ITS OPENER MAY CARRY WHITESPACE, which is the spelling
# that walked past an enumerated tail check; a `> ` puts a real fenced block inside a blockquote,
# where the recogniser saw nothing at all; an HTML comment hides a fenced block from the reader
# while leaving it a block to the scanner; a list marker indents one by two spaces. Each of them
# beside the verdict is a refusal, and none of them is a PASS.
for trailing in '    ```json%b    {"verdict":"PASS","findings":[]}%b    ```' \
                '~~~json%b{"verdict":"PASS","findings":[]}%b~~~' \
                '{"role_understanding":"x","verdict":"PASS","findings":[]}%b%b' \
                '{ "role_understanding":"x","verdict":"PASS","findings":[]}%b%b' \
                '{%b  "role_understanding":"x","verdict":"PASS","findings":[]}%b' \
                '> ```json%b> {"verdict":"PASS","findings":[]}%b> ```' \
                '<!--%b```json%b{"verdict":"PASS","findings":[]}%b```%b-->' \
                '- an example:%b%b  ```json%b  {"verdict":"PASS","findings":[]}%b  ```' \
                '```json%b{"verdict":"PASS","findings":[]}%b```'; do
  { cat "$tmp/indent-0:0.md"; printf "\n$trailing\n" '
' '
' '
' '
'; } > "$tmp/tail-material.md"
  got="$(review_rows "$tmp/tail-material.md")"
  [[ "$got" == 0\|* ]] \
    && error "MUT-VERDICT-AMBIGUOUS-BLOCK-CHOSEN [$trailing]: a comment carrying a second verdict parsed, got [$got]"
  [[ "$got" == *PASS* ]] \
    && error "MUT-VERDICT-AMBIGUOUS-BLOCK-CHOSEN [$trailing]: a second PASS was chosen, got [$got]"
  expect "MUT-VERDICT-AMBIGUOUS-BLOCK-CHOSEN payload" "$(parse_nul review "$tmp/tail-material.md")" '1|'
  # and through main, because the parser refusing is only half of it
  got="$(STUB_REVIEW_BODY="$tmp/tail-material.md" STUB_COMMENTS_STATUS=0 run_stub 999)"
  contains "MUT-VERDICT-AMBIGUOUS-BLOCK-CHOSEN [$trailing]" "$got" "review-parse-failed"
  [[ "$got" == *"verdict=PASS"* ]] \
    && error "MUT-VERDICT-AMBIGUOUS-BLOCK-CHOSEN [$trailing]: the audit read PASS out of it"
done
# The same spellings BEFORE the verdict, because a second place is a second place wherever it is:
# an enumerated tail check only ever looked after the block it had already chosen.
for leading in '```json%b{"verdict":"PASS","findings":[]}%b```' \
               '{ "role_understanding":"x","verdict":"PASS","findings":[]}%b%b' \
               '> ```json%b> {"verdict":"PASS","findings":[]}%b> ```' \
               '~~~json%b{"verdict":"PASS","findings":[]}%b~~~' \
               '<!--%b```json%b{"verdict":"PASS","findings":[]}%b```%b-->'; do
  { printf "$leading\n\n" '
' '
' '
' '
'; cat "$tmp/indent-0:0.md"; } > "$tmp/head-material.md"
  got="$(review_rows "$tmp/head-material.md")"
  [[ "$got" == 0\|* ]] \
    && error "MUT-VERDICT-AMBIGUOUS-BLOCK-CHOSEN [$leading, before]: a comment carrying a second verdict parsed, got [$got]"
  [[ "$got" == *PASS* ]] \
    && error "MUT-VERDICT-AMBIGUOUS-BLOCK-CHOSEN [$leading, before]: a second PASS was chosen, got [$got]"
done
# Detection reads the same structure: a comment whose ONLY verdict block is indented is the
# workflow form, not prose -- the prose parser would read its `VERDICT:` line from outside the
# object and miss every severity the object spells with an escape.
printf 'Reviewed head: %s\n\n   ```json\n   {"reviewed_sha":"%s","verdict":"CHANGES_REQUIRED","findings":[{"id":"ONLY","severity":"P\\u0031"}]}\n   ```\n' \
  "$revived_head" "$revived_head" > "$tmp/indent-only.md"
expect MUT-VERDICT-REVIVED-BY-INDENT "$(review_rows "$tmp/indent-only.md")" \
  "0|json/$revived_head/CHANGES_REQUIRED/-/-;P1:ONLY:0"
# A `VERDICT:` LINE OUTSIDE THE OBJECT IS MATERIAL TOO. The same comment with `VERDICT: PASS`
# written under it says two different things -- the object blocks, the line approves -- and the
# two forms disagree about which one a reader sees. Choosing between them is what the prose fall
# back did; there is nothing to choose here, because the comment does not read.
{ cat "$tmp/indent-only.md"; printf '\nVERDICT: PASS\n'; } > "$tmp/verdict-line-outside.md"
got="$(review_rows "$tmp/verdict-line-outside.md")"
[[ "$got" == 0\|* ]] \
  && error "MUT-VERDICT-AMBIGUOUS-BLOCK-CHOSEN: a comment carrying a verdict object and a verdict line parsed, got [$got]"
[[ "$got" == *PASS* ]] \
  && error "MUT-VERDICT-AMBIGUOUS-BLOCK-CHOSEN: the line outside the object was taken, got [$got]"
# and written ABOVE the object, where a tail check never looked
{ printf 'VERDICT: PASS\n\n'; cat "$tmp/indent-only.md"; } > "$tmp/verdict-line-above.md"
got="$(review_rows "$tmp/verdict-line-above.md")"
[[ "$got" == 0\|* ]] \
  && error "MUT-VERDICT-AMBIGUOUS-BLOCK-CHOSEN: a verdict line above the object parsed, got [$got]"
# And what is OUTSIDE the verdict is found by POSITION. `text.replace(block, "")` removes every
# copy of the block's text and not the one that was read -- and an indented block's content, with
# its indentation stripped, is no longer a substring of the comment at all, so it removes nothing
# and the object's own severities are counted as tokens loose in the prose. A review with one
# recorded finding would go to a person carrying a stray `P1` it does not have.
{ printf 'Reviewed head: %s\n\n  ```json\n  {\n' "$revived_head"
  printf '    "reviewed_sha": "%s",\n    "verdict": "CHANGES_REQUIRED",\n' "$revived_head"
  printf '    "findings": [{"id": "INSIDE", "severity": "P1"}]\n  }\n  ```\n'; } > "$tmp/indent-stray.md"
expect MUT-VERDICT-REVIVED-BY-INDENT "$(review_rows "$tmp/indent-stray.md")" \
  "0|json/$revived_head/CHANGES_REQUIRED/-/-;P1:INSIDE:0"
# A tilde fence is a block -- it has to be, or a ``` inside one would be read as a fence of the
# comment's own -- and it is NOT a verdict, because the workflow writes backticks and narrowing is
# the safe direction. It is still the workflow form, though: reading a comment whose only object is
# tilde-fenced as PROSE would take its `VERDICT:` line from outside the object and miss every
# severity the object spells with an escape, which is the defect a detection that cannot fail was
# written to end. So it is a refusal, not a PASS.
printf 'Reviewed head: %s\n\n~~~json\n{"reviewed_sha":"%s","verdict":"CHANGES_REQUIRED","findings":[{"id":"TILDE","severity":"P\\u0031"}]}\n~~~\n\nVERDICT: PASS\n' \
  "$revived_head" "$revived_head" > "$tmp/tilde-only.md"
got="$(review_rows "$tmp/tilde-only.md")"
[[ "$got" == 0\|* ]] \
  && error "MUT-VERDICT-REVIVED-BY-INDENT: a tilde-fenced verdict was given a verdict, got [$got]"
[[ "$got" == *PASS* ]] \
  && error "MUT-VERDICT-REVIVED-BY-INDENT: a tilde-fenced review was read as prose and passed, got [$got]"
# The older bare form is held to the same rule: what follows the object it was read from may not be
# another block's material either.
printf 'Reviewed head: %s\n\n{"role_understanding":"x","reviewed_sha":"%s","verdict":"CHANGES_REQUIRED","findings":[{"id":"BARE","severity":"P2"}]}\n' \
  "$revived_head" "$revived_head" > "$tmp/bare-whole.md"
expect MUT-VERDICT-REVIVED-BY-INDENT "$(review_rows "$tmp/bare-whole.md")" \
  "0|json/$revived_head/CHANGES_REQUIRED/-/-;P2:BARE:0"
{ cat "$tmp/bare-whole.md"; printf '\n```text\nA note appended under the verdict.\n```\n'; } \
  > "$tmp/bare-tail.md"
got="$(review_rows "$tmp/bare-tail.md")"
[[ "$got" == 0\|* ]] \
  && error "MUT-VERDICT-REVIVED-BY-INDENT: a bare verdict with block material after it parsed, got [$got]"

# THE FORM IS THE COMMENT'S OWN MARKER, and neither parser is what is left when the other fails.
# A comment carrying the prose form's marker AND a verdict block claims to be both forms, and the
# two do not agree about the same review: this is finding 2's shape with the fence spelled so the
# recogniser DOES see it, which is the half that must refuse for the same reason the half it
# cannot see does.
printf '<!-- upstroke-frontier-review pr=232 head=%s -->\n\n```json\n{"reviewed_sha":"%s","verdict":"CHANGES_REQUIRED","findings":[{"id":"BOTH","severity":"P\\u0031"}]}\n```\n\nVERDICT: PASS\n' \
  "$revived_head" "$revived_head" > "$tmp/both-forms.md"
got="$(review_rows "$tmp/both-forms.md")"
[[ "$got" == 0\|* ]] \
  && error "MUT-VERDICT-AMBIGUOUS-BLOCK-CHOSEN: a comment claiming both forms parsed, got [$got]"
[[ "$got" == *PASS* ]] \
  && error "MUT-VERDICT-AMBIGUOUS-BLOCK-CHOSEN: a comment claiming both forms was read as prose, got [$got]"
# And the other half of "detection is not a fall back": a comment with NO prose marker and no
# verdict block is the workflow form with nothing to read, which is a refusal. "I could not read
# the JSON, so I will try prose" is how a blocking review became a PASS, and there is no longer a
# road from zero blocks to the prose parser.
printf 'Reviewed head: %s\n\nNo verdict block anywhere.\n\nVERDICT: PASS\n' \
  "$revived_head" > "$tmp/no-block-no-marker.md"
got="$(review_rows "$tmp/no-block-no-marker.md")"
[[ "$got" == 0\|* ]] \
  && error "MUT-VERDICT-AMBIGUOUS-BLOCK-CHOSEN: a workflow comment with no verdict block parsed, got [$got]"
[[ "$got" == *PASS* ]] \
  && error "MUT-VERDICT-AMBIGUOUS-BLOCK-CHOSEN: a comment with no verdict block fell back to prose, got [$got]"

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
expect MUT-JSON-SPLIT-BY-REGEX "$(review_rows "$tmp/pretty.md")" \
  '0|json/4ad962f000000000000000000000000000000001/CHANGES_REQUIRED/-/-;P3:FIRST:0;P1:SECOND:0'

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
expect "MUT-PROSE-HEADING-FINDING/MUT-PROSE-LAST-VERDICT/MUT-QUOTED-JSON-IS-JSON" \
  "$(review_rows "$tmp/prose.md")" \
  '0|prose/c3a6665000000000000000000000000000000003/CHANGES_REQUIRED/-/MUST/P1;P2:-:0;P3:-:0'

# A prose review that quotes a JSON object stays prose: the fence has to open a line of its own,
# and the comment says which form it is in by carrying the prose form's marker.
printf '<!-- upstroke-frontier-review pr=1 -->\nReviewed head: %s\nThe object {"verdict":"PASS","findings":[]} is an example.\nVERDICT: CHANGES_REQUIRED\n' \
  "4ad962f000000000000000000000000000000001" > "$tmp/quoted.md"
expect MUT-QUOTED-JSON-IS-JSON "$(review_rows "$tmp/quoted.md")" \
  '0|prose/4ad962f000000000000000000000000000000001/CHANGES_REQUIRED/-/-'

# No field is ever empty, and no field is ever missing. `-` is how the parser says "the review did
# not record this"; a tab-separated protocol folded an empty field and shifted the verdict into the
# commit's column, so the audit reported a reviewed commit of `PASS`.
printf 'Reviewed head: %s\n\n```json\n{"verdict":"PASS","findings":[]}\n```\n' \
  4ad962f000000000000000000000000000000001 > "$tmp/no-sha.md"
expect MUT-META-FIELD-COLLAPSE "$(review_rows "$tmp/no-sha.md")" '0|json/-/PASS/-/-'
expect MUT-META-FIELD-COLLAPSE "$(parse_nul review "$tmp/no-sha.md")" '0|review|json|-|PASS|-|-|0|'
printf '<!-- upstroke-frontier-review pr=1 -->\nno reviewed head anywhere\n' > "$tmp/no-head.md"
expect MUT-META-FIELD-COLLAPSE "$(review_rows "$tmp/no-head.md")" '0|prose/-/-/-/-'
# A verdict block holding JSON that is not a verdict object is a COMPLETE description of a review
# that records nothing -- one finding the audit cannot judge, and every field absent. It is a
# result rather than a refusal because the audit blocks on every `ERR` row it reads, and "there is
# a review and it says nothing readable" is what a person has to be told.
printf '```json\n[]\n```\n' > "$tmp/not-an-object.md"
expect MUT-BAD-SEVERITY-PASSES "$(review_rows "$tmp/not-an-object.md")" '0|json/-/-/-/-;ERR:unparsed:0'
got="$(STUB_REVIEW_BODY="$tmp/not-an-object.md" STUB_COMMENTS_STATUS=0 run_stub 999)"
contains MUT-BAD-SEVERITY-PASSES "$got" "NOT-READY"
contains MUT-BAD-SEVERITY-PASSES "$got" "findings-unparsed:unparsed"

# A review string that carries a separator writes no record. The base is not a commit and is
# refused as one -- not trimmed to the part of it that is -- and the finding survives.
printf 'Reviewed head: %s\n\n```json\n{"reviewed_sha":"%s","base_sha":"%s\\nEND\\t-\\t0","verdict":"PASS","findings":[{"id":"A-DEFERRABLE","severity":"P3"}]}\n```\n' \
  4ad962f000000000000000000000000000000001 4ad962f000000000000000000000000000000001 \
  5157509000000000000000000000000000000002 > "$tmp/forged-base.md"
expect MUT-REVIEW-FORGES-PROTOCOL "$(review_rows "$tmp/forged-base.md")" \
  '0|json/4ad962f000000000000000000000000000000001/PASS/-/-;P3:A-DEFERRABLE:0'
# A finding id is a field of the same protocol and is held to the same rule. It is refused rather
# than trimmed to "no id": no id is a MANUAL line for a person, and this is an id that wrote rows.
printf '```json\n{"verdict":"PASS","findings":[{"id":"A\\tEND\\t-\\t0","severity":"P3"}]}\n```\n' \
  > "$tmp/forged-id.md"
expect MUT-REVIEW-FORGES-PROTOCOL "$(review_rows "$tmp/forged-id.md")" '0|json/-/PASS/-/-;ERR:bad-id:0'
# And the separator this protocol actually uses. A NUL cannot appear in a field the parser emits;
# where the review puts one there, the parse REFUSES rather than emitting a field that would make
# a record boundary of its own.
"$parser_python" - "$tmp/nul-verdict.md" <<'PY'
import sys
open(sys.argv[1], "wb").write(
    b"<!-- upstroke-frontier-review pr=1 head=c3a6665000000000000000000000000000000003 -->"
    b"\n\nVERDICT: PA\x00SS\n"
)
PY
got="$(parse_nul review "$tmp/nul-verdict.md")"
[[ "$got" == 0\|* ]] \
  && error "MUT-REVIEW-FORGES-PROTOCOL: a field holding the record separator was emitted, got [$got]"
expect MUT-REVIEW-FORGES-PROTOCOL "${got#*|}" ''

# --- the parse is whole or it is nothing --------------------------------------------------------
# THE FINDING THIS ROUND WAS WRITTEN FOR. The old parser printed one record per finding and
# checked none of those writes: with `write()` made to return EIO on the row carrying a P1, the
# function around it still returned 0, `END` printed after the gap, and a review that blocks
# audited as a clean PASS -- READY, and a merge call. There is one write now, it is confirmed
# before the exit status is decided, and the destination is renamed into place only after the
# bytes are on the device. `/dev/full` fails every write on any machine, with no ptrace needed;
# the pull request carries the `strace`-injected EIO as well.
printf '<!-- upstroke-frontier-review pr=232 head=%s -->\n\n1. **P1 - a thing that blocks.** Detail.\n\nVERDICT: PASS\n' \
  c3a6665000000000000000000000000000000003 > "$tmp/prose-numbered-p1.md"
expect MUT-PARSE-WRITE-UNCHECKED "$(review_rows "$tmp/prose-numbered-p1.md")" \
  '0|prose/c3a6665000000000000000000000000000000003/PASS/-/-;P1:-:0'
# `--out`, which is how the audit runs it, with the write denied by `ulimit -f 0` -- a real
# `write()` that fails, on any machine, with no ptrace needed. Two assertions, and the second is
# the one the finding was about: the status says the parse failed, AND THE DESTINATION IS NOT
# WRITTEN, so there is no half a result for a caller to read even if it ignored the status.
: > "$tmp/denied.out"
write_status=0
( trap '' XFSZ; ulimit -f 0
  "$parser_python" scripts/pr-review-parse.py review --nul --out "$tmp/denied.out" \
    "$tmp/prose-numbered-p1.md" 2>/dev/null ) || write_status=$?
((write_status != 0)) \
  || error "MUT-PARSE-WRITE-UNCHECKED: a parse whose result could not be written reported success"
expect MUT-PARSE-WRITE-UNCHECKED "$(wc -c < "$tmp/denied.out")" 0
expect MUT-PARSE-WRITE-UNCHECKED "$(ls "$tmp/denied.out".* 2>/dev/null | wc -l)" 0
# The same call with the write allowed writes the whole payload, so the case above is about the
# denied write and not about the fixture.
expect MUT-PARSE-WRITE-UNCHECKED "$(parse_nul review "$tmp/prose-numbered-p1.md" "$tmp/denied.out")" \
  '0|review|prose|c3a6665000000000000000000000000000000003|PASS|-|-|1|P1|-|0|'
# --- the staging file belongs to the invocation that created it ---------------------------------
# `OUT + ".part"` is ONE PATHNAME SHARED BY EVERY INVOCATION writing to one destination, and two
# of them overlapping publishes one's bytes under the other's exit 0: A flushed a blocking review
# and paused in `fsync`, B truncated the same staging file and wrote a clean `PASS` over it, A woke
# and renamed B's bytes into place -- A exit 0 publishing PASS with no findings, from an input
# carrying CHANGES_REQUIRED and a P1; B then exit 1, its staging pathname gone. Standards section 8
# says it directly: a unique staging path, never a fixed temporary name concurrent writers collide
# on. The pull request carries the `strace`-timed race; this is the same property without timing.
#
# A DIRECTORY standing at the fixed pathname is the whole case: a parse that still reaches for
# `OUT.part` cannot open it and exits non-zero, and one that creates its own staging name never
# looks. It needs no permission trick, so it is the same case for a run as root.
mkdir -p "$tmp/stage"
staged_out="$tmp/stage/payload.nul"
mkdir -p "$staged_out.part"
expect MUT-STAGING-PATH-SHARED "$(parse_nul review "$tmp/prose-numbered-p1.md" "$staged_out")" \
  '0|review|prose|c3a6665000000000000000000000000000000003|PASS|-|-|1|P1|-|0|'
[[ -d "$staged_out.part" ]] \
  || error "MUT-STAGING-PATH-SHARED: the parse wrote through the fixed staging pathname"
# and it leaves nothing behind beside the destination: a staging file per invocation is not a
# staging file per invocation left lying there.
expect MUT-STAGING-PATH-SHARED "$(ls "$tmp/stage" | grep -vcx 'payload.nul\|payload.nul.part')" 0
rmdir "$staged_out.part"
# The published bytes are this invocation's own, and its permissions are what a plain create
# leaves: making the staging name unique is not also a change to what the caller reads.
expect MUT-STAGING-PATH-SHARED "$(stat -c %a "$staged_out")" "$(
  : > "$tmp/stage/plain"; stat -c %a "$tmp/stage/plain")"

# And the destination the parse could not open at all: nothing is written there either.
mkdir -p "$tmp/unwritable"
: > "$tmp/unwritable/out"
chmod a-w "$tmp/unwritable"
out_status=0
"$parser_python" scripts/pr-review-parse.py review --nul --out "$tmp/unwritable/out" \
  "$tmp/prose-numbered-p1.md" 2>/dev/null || out_status=$?
chmod u+w "$tmp/unwritable"
if ((EUID != 0)); then    # root writes into a directory with no write bit, so the case is not one
  ((out_status != 0)) \
    || error "MUT-PARSE-WRITE-UNCHECKED: a parse that could not open its destination reported success"
  expect MUT-PARSE-WRITE-UNCHECKED "$(wc -c < "$tmp/unwritable/out")" 0
fi
# The rendering the gate itself reads, to stdout, held to the same rule.
write_status=0
"$parser_python" scripts/pr-review-parse.py review --nul "$tmp/prose-numbered-p1.md" \
  > /dev/full 2>/dev/null || write_status=$?
((write_status != 0)) \
  || error "MUT-PARSE-WRITE-UNCHECKED: a parse whose result went nowhere reported success"

# --- a write that returned a smaller number is not a write ---------------------------------------
# `write()` RETURNS A BYTE COUNT AND IT IS NOT ALWAYS THE WHOLE PAYLOAD. Under `PYTHONUNBUFFERED=1`
# `sys.stdout.buffer` is the raw file object, whose `write` does one `write(2)` and hands back what
# the kernel took. With the process's `RLIMIT_FSIZE` at 1,024 bytes, stdout on a file and a review
# of 499 numbered findings rendering to 3,560 bytes, `strace` recorded
# `write(1, ..., 3560) = 1024` and then `+++ exited with 0 +++`: the count discarded, a flush of an
# unbuffered stream finishing nothing, and a findings list 2,536 bytes short of itself announced as
# whole -- in both renderings, and with the record count the shell checks still declaring every
# record the result was built with. That is the seventh round's unchecked write in the one place
# that was left.
{ printf '<!-- upstroke-frontier-review pr=232 head=%s -->\n\n' c3a6665000000000000000000000000000000003
  for ((n = 1; n <= 499; n++)); do printf '%d. **P1 - a thing that blocks.** Detail.\n\n' "$n"; done
  printf 'VERDICT: PASS\n'; } > "$tmp/many-findings.md"
for rendering in --nul --json; do
  # the control first: with no limit the whole payload is written and the parse exits 0
  whole_status=0
  "$parser_python" scripts/pr-review-parse.py review "$rendering" "$tmp/many-findings.md" \
    > "$tmp/many.whole" 2>/dev/null || whole_status=$?
  expect "MUT-PARSE-SHORT-WRITE [$rendering] control" "$whole_status" 0
  whole="$(wc -c < "$tmp/many.whole")"
  ((whole > 1024)) \
    || error "MUT-PARSE-SHORT-WRITE [$rendering]: the fixture renders to [$whole] bytes, which the limit would not cut"
  short_status=0
  ( trap '' XFSZ; ulimit -f 1
    PYTHONUNBUFFERED=1 "$parser_python" scripts/pr-review-parse.py review "$rendering" \
      "$tmp/many-findings.md" > "$tmp/many.short" 2>/dev/null ) || short_status=$?
  ((short_status != 0)) \
    || error "MUT-PARSE-SHORT-WRITE [$rendering]: a result whose write stopped part way reported success"
  # and the write really did stop part way, so the case is about the count and not about a refusal
  # somewhere else: what landed is some of the payload and not all of it.
  landed="$(wc -c < "$tmp/many.short")"
  ((landed > 0 && landed < whole)) \
    || error "MUT-PARSE-SHORT-WRITE [$rendering]: no short write happened, [$landed] of [$whole] bytes"
done
# The class, not the site. The only `write` call in the parser is the one inside `write_all`, which
# reads what it returns, and the stderr diagnostic, which is not the result channel: any other is a
# count nobody read. Asked of the SYNTAX TREE, so a sentence in a comment cannot answer for code.
expect MUT-PARSE-SHORT-WRITE "$("$parser_python" - scripts/pr-review-parse.py <<'WRITESHAPE'
import ast, sys


def dotted(node):
    parts = []
    while isinstance(node, ast.Attribute):
        parts.append(node.attr)
        node = node.value
    if isinstance(node, ast.Name):
        parts.append(node.id)
    return ".".join(reversed(parts))


loose = 0
for node in ast.walk(ast.parse(open(sys.argv[1]).read())):
    if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute) \
            and node.func.attr == "write" \
            and dotted(node.func) not in ("stream.write", "sys.stderr.buffer.write"):
        loose += 1
print(loose)
WRITESHAPE
)" 0
# An input the parse cannot read is not an empty review, and it is not a review in the other
# format either: FORMAT DETECTION IS PART OF THE PARSE. `if grep ...; then json; else prose; fi`
# made a file it could not read into "prose" and CHOSE A PARSER on it -- and the prose parser
# reads a JSON review's `VERDICT:` line from outside its verdict object and misses every severity
# the object spells with a JSON escape (`"P1"` holds no `P1` to find).
printf '```json\n{"verdict":"CHANGES_REQUIRED","findings":[{"id":"CRITICAL","severity":"P\\u0031"}]}\n```\n' \
  > "$tmp/escaped-severity.md"
expect MUT-REVIEW-KIND-UNREADABLE-IS-PROSE "$(review_rows "$tmp/escaped-severity.md")" \
  '0|json/-/CHANGES_REQUIRED/-/-;P1:CRITICAL:0'
# The same review with a `VERDICT: PASS` line written under it is a comment that says both things,
# and the prose reading of it is the clean PASS with no findings -- the escape hides the P1 from
# the stray scan. It is not read either way.
{ cat "$tmp/escaped-severity.md"; printf '\nVERDICT: PASS\n'; } > "$tmp/escaped-severity-both.md"
got="$(review_rows "$tmp/escaped-severity-both.md")"
[[ "$got" == 0\|* ]] \
  && error "MUT-REVIEW-KIND-UNREADABLE-IS-PROSE: a comment saying CHANGES_REQUIRED and PASS parsed, got [$got]"
[[ "$got" == *PASS* ]] \
  && error "MUT-REVIEW-KIND-UNREADABLE-IS-PROSE: the prose PASS was taken over the object, got [$got]"
#
# It is also where MUT-PROSE-READ-SUPPRESSED now lives. That case was a read INSIDE the prose
# parser whose failure was reported as "nothing matched", with the completeness marker printed
# over it: a review carrying a P3 under a PASS parsed as a clean PASS with no findings. There is
# no read inside the parse to suppress a status on any more -- the input is read once, whole, and
# a read that failed is a parse that failed, which is these cases.
for unreadable in "$tmp/no-such-review.md" "$tmp"; do
  got="$(review_rows "$unreadable")"
  [[ "$got" == 0\|* ]] \
    && error "MUT-REVIEW-KIND-UNREADABLE-IS-PROSE/MUT-PROSE-READ-SUPPRESSED: an input the parse could not read was given a format, got [$got]"
done
# Bytes that are not UTF-8 are not text with the bad bytes dropped, for the same reason.
printf '```json\n{"verdict":"PASS","findings":[]}\n```\n\xff\xfe\n' > "$tmp/not-utf8.md"
got="$(review_rows "$tmp/not-utf8.md")"
[[ "$got" == 0\|* ]] \
  && error "MUT-REVIEW-KIND-UNREADABLE-IS-PROSE/MUT-PROSE-READ-SUPPRESSED: bytes that are not UTF-8 parsed as a review, got [$got]"
# Through main, because the helper refusing is only half of it: the audit must block on the status
# rather than reading whatever the file holds.
got="$(STUB_REVIEW_BODY="$tmp/not-utf8.md" STUB_COMMENTS_STATUS=0 run_stub 999)"
contains MUT-REVIEW-KIND-UNREADABLE-IS-PROSE "$got" "review-parse-failed"
contains MUT-REVIEW-KIND-UNREADABLE-IS-PROSE "$got" "NOT-READY"
[[ "$got" == *"verdict=PASS"* ]] \
  && error "MUT-REVIEW-KIND-UNREADABLE-IS-PROSE: a verdict was read out of a review the parser refused"
# The same review readable is judged by the JSON parser and blocks on its P1, so the case above is
# about the refusal and not about the review.
got="$(STUB_REVIEW_BODY="$tmp/escaped-severity.md" STUB_COMMENTS_STATUS=0 run_stub 999)"
contains MUT-REVIEW-KIND-UNREADABLE-IS-PROSE "$got" "open-P1:CRITICAL"
[[ "$got" == *review-parse-failed* ]] \
  && error "MUT-REVIEW-KIND-UNREADABLE-IS-PROSE: a readable review was reported unreadable"

# A PAYLOAD THAT STOPPED ARRIVING IS NOT A SHORTER FINDINGS LIST. `read` ends a loop at end of
# input and at a failed read alike, so the audit cannot tell the two apart by looking; it checks
# what arrived against the record count the parser computed. Truncating the payload after the
# first finding is what a short read looks like from inside the loop.
"$parser_python" scripts/pr-review-parse.py review --nul --out "$tmp/short.payload" "$tmp/json.md"
truncate -s 60 "$tmp/short.payload"
short_status=0
( fields=(); read_parser_fields "$tmp/short.payload" review 7 3 ) || short_status=$?
((short_status != 0)) \
  || error "MUT-REVIEW-PARSE-TRUNCATED: a payload that stopped arriving was read as a whole one"
# The whole payload passes the same check, so the case above is about the truncation.
expect MUT-REVIEW-PARSE-TRUNCATED "$(
  "$parser_python" scripts/pr-review-parse.py review --nul --out "$tmp/whole.payload" "$tmp/json.md"
  fields=(); read_parser_fields "$tmp/whole.payload" review 7 3 && echo "${#fields[@]}"
)" 22
# A payload of another kind, and one whose declared count does not match what it carries, are both
# refused: the tag and the count are checked before a field is read as anything.
printf 'ledger\0' > "$tmp/wrong-tag.payload"
( fields=(); read_parser_fields "$tmp/wrong-tag.payload" review 7 3 ) \
  && error "MUT-REVIEW-PARSE-TRUNCATED: a payload of the wrong kind was read as a review"
printf 'review\0json\0-\0PASS\0-\0-\09\0' > "$tmp/lying-count.payload"
( fields=(); read_parser_fields "$tmp/lying-count.payload" review 7 3 ) \
  && error "MUT-REVIEW-PARSE-TRUNCATED: a payload declaring more records than it carries was read"

# --- a check that failed is the answer, not the input to another attempt --------------------------
# A verdict is a whole token, in every locale. `[A-Z_]+$` matches the TAIL of a dirty token, and
# `[A-Z_]` inside a grep is whatever the locale's collating order says it is: under en_US.utf8 the
# grep carried `FAIL\xc3\x89PASS` through and the check took the clean `PASS` off its end, so a
# review that says FAIL approved the change -- while the same file under `C` blocked. The parser is
# a program now and its character classes are code-point ranges, which mean the same thing
# everywhere; the token that is not a verdict is still carried whole, `VERDICT:` and all, so the
# blocker names it.
printf '<!-- upstroke-frontier-review pr=232 head=%s -->\n\nVERDICT: FAIL\xc3\x89PASS\n' \
  c3a6665000000000000000000000000000000003 > "$tmp/prose-suffix.md"
suffix_want="$(printf '0|prose/c3a6665000000000000000000000000000000003/VERDICT: FAIL\xc3\x89PASS/-/-')"
expect MUT-PROSE-VERDICT-SUFFIX "$(review_rows "$tmp/prose-suffix.md")" "$suffix_want"
for loc in "${locales[@]}"; do
  got="$(LC_ALL="$loc" review_rows "$tmp/prose-suffix.md")"
  expect "MUT-PROSE-VERDICT-SUFFIX under $loc" "$got" "$suffix_want"
done
# A verdict wrapped in the emphasis a frontier review writes is still that verdict, and the last
# line still wins: the whole-token rule must not refuse the form the reviews actually use.
printf '<!-- upstroke-frontier-review pr=1 -->\nhead=%s\n**VERDICT: CHANGES_REQUIRED**\n**VERDICT: PASS**\n' \
  c3a6665000000000000000000000000000000003 > "$tmp/prose-emphasis.md"
expect MUT-PROSE-VERDICT-SUFFIX "$(review_rows "$tmp/prose-emphasis.md")" \
  '0|prose/c3a6665000000000000000000000000000000003/PASS/-/-'
# The head is the FIRST marker, and a first marker that does not read whole is not a licence to
# take the second: the audit would then check the pull request's head against a commit the review
# never named. `-` blocks; the later line does not stand in for it.
printf '<!-- upstroke-frontier-review pr=1 -->\nhead=c3a6665\xc3\x8900000000000000000000000000000003\nReviewed head: %s\n' \
  4ad962f000000000000000000000000000000001 > "$tmp/prose-head.md"
for loc in "${locales[@]}"; do
  got="$(LC_ALL="$loc" review_rows "$tmp/prose-head.md")"
  expect "MUT-PROSE-HEAD-NOT-FIRST under $loc" "$got" '0|prose/-/-/-/-'
done
# The whole-token check refuses `VERDICT: ::PASS`, and the branch it fell into used to strip the
# leading colons and asterisks off what it had just refused -- so the rejecting branch minted the
# `PASS` the check exists to withhold. Nothing is stripped: the whole matched run stands, and
# because every one of these begins with `VERDICT:` it cannot be the token the audit lets through.
for junk in ': ::PASS' ': :PASS' ': **PASS' ': *PASS'; do
  printf '<!-- upstroke-frontier-review pr=232 head=%s -->\n\nVERDICT%s\n' \
    c3a6665000000000000000000000000000000003 "$junk" > "$tmp/prose-salvage.md"
  for loc in ambient "${locales[@]}"; do
    if [[ "$loc" == ambient ]]; then got="$(review_rows "$tmp/prose-salvage.md")"
    else got="$(LC_ALL="$loc" review_rows "$tmp/prose-salvage.md")"; fi
    [[ "$got" == *'/PASS/'* ]] \
      && error "MUT-PROSE-VERDICT-SALVAGE under $loc: [VERDICT$junk] was salvaged into PASS, got [$got]"
    contains MUT-PROSE-VERDICT-SALVAGE "$got" '/VERDICT'
  done
done

# --- a review too big for a pipe buffer ---------------------------------------------------------
# `<<<` is a here-document, and bash writes one to a TEMPORARY FILE once it outgrows a pipe
# buffer; a temporary file it cannot create is a redirection that failed, so the command never
# runs and the shell returns 1 -- `grep`'s "no match" -- and a compound command fed by one is
# skipped outright, with a status of 0 that neither a captured status nor `set -e` can see. Both
# shapes were live in the prose parser: one lost the only `P1` in a 40,000-character review and
# printed `END` over it, the other would have dropped a findings list whole. There is no
# here-string in the parse path at all now, and `ulimit -f` denies the temporary file these used
# to need, so a parser that still wanted one would be caught here.
{ printf '<!-- upstroke-frontier-review pr=232 head=%s -->\n\nVERDICT: PASS\n\n' \
    c3a6665000000000000000000000000000000003
  for _ in $(seq 1 400); do printf '\xc3\xa9%.0s' $(seq 1 100); printf '\n'; done
  printf '\nA standalone P1 in running text.\n'; } > "$tmp/prose-big-stray.md"
big_want='0|prose/c3a6665000000000000000000000000000000003/PASS/-/P1'
expect MUT-HERESTRING-FAILURE-AS-ANSWER "$(review_rows "$tmp/prose-big-stray.md")" "$big_want"
expect MUT-HERESTRING-FAILURE-AS-ANSWER \
  "$(trap '' XFSZ; ulimit -f 1; review_rows "$tmp/prose-big-stray.md")" "$big_want"
{ printf '<!-- upstroke-frontier-review pr=232 head=%s -->\n\n' \
    c3a6665000000000000000000000000000000003
  for i in $(seq 1 8000); do printf '%d. **P3 - a deferrable thing.** Detail.\n' "$i"; done
  printf '\nVERDICT: PASS\n'; } > "$tmp/prose-many-findings.md"
expect MUT-HERESTRING-FAILURE-AS-ANSWER \
  "$(trap '' XFSZ; ulimit -f 1; review_rows "$tmp/prose-many-findings.md" | grep -o 'P3:-:0' | wc -l)" 8000
expect MUT-HERESTRING-FAILURE-AS-ANSWER \
  "$(review_rows "$tmp/prose-many-findings.md" | grep -o 'P3:-:0' | wc -l)" 8000

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

# --- the state comes from the blockers, and from nothing that can fail --------------------------
# THE SECOND OF THIS ROUND'S TWO P1s. `printf '%s\n' "${blockers[@]:-}" | grep -q '^manual:'` in
# the `elif` gave a manual blocker two ways to be absent: it was not there, or the scan did not
# happen. With `grep` made to exit 2 a pull request whose only blocker was
# `manual:P1-outside-the-verdict-object` printed `READY ... blockers=manual:P1-outside-the-verdict-object`,
# was enqueued, and exited 0.
#
# `audit_state` runs no command at all, so these cases run it with an EMPTY PATH: not `grep`
# broken, but no external command reachable from the function under test. A rewrite that reaches
# for one fails here whatever it reaches for.
empty_path="$tmp/empty-path"
mkdir -p "$empty_path"
state_of() {  # state_of MOVED BLOCKER...: the state those blockers put a pull request in
  local moved="$1"; shift
  blockers=("$@")
  state=""
  audit_state "$moved"
  printf '%s' "$state"
}
expect MUT-MANUAL-SCAN-FAILS-OPEN "$(PATH="$empty_path" state_of "" manual:P1-outside-the-verdict-object)" MANUAL
expect MUT-MANUAL-SCAN-FAILS-OPEN "$(PATH="$empty_path" state_of "" manual:a manual:b)" MANUAL
expect MUT-MANUAL-SCAN-FAILS-OPEN "$(PATH="$empty_path" state_of "")" READY
expect MUT-MANUAL-SCAN-FAILS-OPEN "$(PATH="$empty_path" state_of "" "")" READY
expect MUT-MANUAL-SCAN-FAILS-OPEN "$(PATH="$empty_path" state_of "" no-review)" NOT-READY
expect MUT-MANUAL-SCAN-FAILS-OPEN "$(PATH="$empty_path" state_of "" manual:a no-review)" NOT-READY
expect MUT-MANUAL-SCAN-FAILS-OPEN "$(PATH="$empty_path" state_of repairs manual:a)" NEEDS-ATTEST
expect MUT-MANUAL-SCAN-FAILS-OPEN "$(PATH="$empty_path" state_of merge-edits:abc1234 manual:a)" NEEDS-ATTEST
expect MUT-MANUAL-SCAN-FAILS-OPEN "$(PATH="$empty_path" state_of repairs no-review)" NOT-READY
expect MUT-MANUAL-SCAN-FAILS-OPEN "$(PATH="$empty_path" state_of ledger-only)" READY
# A hard blocker that merely CONTAINS the manual prefix is still hard: the test is the prefix, on
# the whole element, and `grep '^manual:'` over a printed list was matching line starts in text
# the elements themselves could have supplied.
expect MUT-MANUAL-SCAN-FAILS-OPEN "$(PATH="$empty_path" state_of "" "open-P1:a-manual:thing")" NOT-READY
expect MUT-MANUAL-SCAN-FAILS-OPEN "$(PATH="$empty_path" state_of "" "verdict:x
manual:y")" NOT-READY

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
ledger_rows() {  # ledger_rows FILE: "<status>|<id>:<disposition>;..."
  local out status=0
  out="$("$parser_python" scripts/pr-review-parse.py ledger "$1" 2>/dev/null)" || status=$?
  ((status == 0)) || { printf '%s|' "$status"; return 0; }
  printf '%s|%s' "$status" "$(printf '%s' "$out" | jq -j '[.rows[] | "\(.id):\(.disposition)"] | join(";")')"
}
expect MUT-LEDGER-HEADER-AS-ROW "$(ledger_rows "$tmp/body.md")" \
  '0|A-DEFERRABLE:deferred;B-FIXED:fixed'
# The payload the audit reads, whose second field is the row count: the same rule as the review's,
# so a ledger that stopped arriving is not a pull request with fewer rows in it.
expect MUT-PARSE-PAYLOAD-COUNT "$(parse_nul ledger "$tmp/body.md")" \
  '0|ledger|2|A-DEFERRABLE|deferred|B-FIXED|fixed|'
# A body the audit could not fetch is not a body with no ledger in it. It used to be `gh pr view
# ... | ledger_rows_from_body` captured into a variable, so a failed fetch ended the whole run
# through `set -e` -- not a decision about this pull request, just an exit -- and any rewrite of
# that line into a condition would have made it "no ledger rows", which is `no-row` for a finding
# that has a row. It is a blocker on this pull request now, and it is stated.
got="$(STUB_REVIEW_BODY="$tmp/truncating-review.md" STUB_COMMENTS_STATUS=0 STUB_BODY_STATUS=1 run_stub 999)"
contains MUT-LEDGER-LOOKUP-SUPPRESSED "$got" "ledger-lookup-failed"
contains MUT-LEDGER-LOOKUP-SUPPRESSED "$got" "NOT-READY"
# and the same run with the body readable does not report the failure, so the case above is about
# the fetch and not about the review.
got="$(STUB_REVIEW_BODY="$tmp/truncating-review.md" STUB_COMMENTS_STATUS=0 run_stub 999)"
[[ "$got" == *ledger-lookup-failed* ]] \
  && error "MUT-LEDGER-LOOKUP-SUPPRESSED: a readable body was reported unreadable"

if ((failed)); then
  echo "test-pr-ready-audit: FAILED" >&2
  exit 1
fi
echo "test-pr-ready-audit: ok"
