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
unset UPSTROKE_AUDIT_REVIEWER

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
contains MUT-REVIEWER-ARG-EATEN "$got" "2|refusing: --reviewer [--enqueue] is not a GitHub login."
[[ "$got" == *GH-REACHED* ]] && error "MUT-REVIEWER-ARG-EATEN: --reviewer --enqueue reached GitHub"
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
