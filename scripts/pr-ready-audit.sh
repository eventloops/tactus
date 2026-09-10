#!/usr/bin/env bash
# pr-ready-audit.sh: decide, per open pull request, whether it is ready to enqueue for merge
# under the three-lane finding policy, and optionally maintain the lane and ready labels.
#
#   scripts/pr-ready-audit.sh [--apply] [--enqueue] [--ready-label NAME] [--reviewer LOGIN] [PR ...]
#
# NAME must not be a lane:* label. The ready label is advisory: it reports that this audit found
# the head it read READY. A GitHub label is not bound to a commit, so the head is read again
# before the label is written and again after, and a label written across a push is removed
# (HEAD-MOVED); that narrows the race, it cannot close it. The act bound to the audited head is
# the enqueue, through `gh pr merge --match-head-commit`, and the base is read again just before
# it (BASE-MOVED); nothing may treat the label alone as permission to merge.
#
# --apply maintains the lane:* and ready label on each pull request. --enqueue adds every READY
# pull request to the merge queue (`gh pr merge --merge --auto`) in the order the arguments give,
# so the caller states the priority; with no arguments it walks the open pull requests in the
# API's order, which is not a priority. It implies --apply.
#
# --ready-label NAME uses an existing label of that name as it is (the audit adds and removes it
# on pull requests and never recolours or redescribes it) and creates it only when absent.
#
# --reviewer LOGIN, or UPSTROKE_REVIEW_AUTHOR, names the account whose review comments this audit
# trusts. Whose word counts is a trust decision, so it is stated, not deduced. When neither is
# given the repository's own owner is used, which is right only while the repository belongs to
# the person who reviews it: on 2026-09-06 `upstroke` moved into the `sourcemaps` organization,
# that field began resolving to the organization, no comment on earth is authored by an
# organization, and every pull request silently audited as `no-review` -- READY became
# unreachable and --enqueue enqueued nothing. It failed closed, and it failed invisibly, which is
# why an organization owner is now refused outright rather than carried into the query.
#
# LOGIN must be shaped like a GitHub account name -- 1 to 39 ASCII letters and digits with single
# hyphens between them -- and anything else is refused, never tried: an account that cannot exist
# reads as `no-review` on every pull request, which is the silence this flag exists to end. The
# value reaches the comment query as string data and is never part of the query's text, so a
# string shaped like a query is compared as a login and matches nothing. Spelling is compared
# without regard to case, as GitHub compares it: `EventLoops` and `eventloops` are one account.
# A bot login of the form `name[bot]` is not a GitHub account name and is refused; no such account
# posts reviews here today, and admitting one is a change to whom this audit trusts, not a widened
# pattern. The character set is spelled out rather than written as a range, because a range in a
# bash regex is resolved by the locale's collation and not by ASCII: under en_US.utf8 `[A-Za-z]`
# admits `é` and the Kelvin sign, so a check whose comment promised ASCII passed `evéntloops`
# through to the API as a login nobody has.
#
# Which source supplies LOGIN is settled before the value is checked, and only the value this run
# will trust is checked: the flag replaces the environment variable rather than being read after
# it, so an inherited UPSTROKE_REVIEW_AUTHOR this run does not use cannot refuse the run that
# overrode it. Neither --reviewer nor --ready-label will take the next option as its value: an
# argument beginning with a hyphen is refused, never consumed. The test is the hyphen and not a
# list of today's options, because a guard written as that list drifted from the options there
# are and let `-h` through; consuming an option both misnames the value and silently switches
# off the flag it swallowed.
#
# Lanes, decided by the branch prefix and nothing else (a lane:* label is output, never input;
# a wrong one is corrected by --apply and reported as lane-label-mismatch):
#   lane:findings-p3     codex/findings-p3-*   must fix everything; ready only on a PASS verdict
#   lane:findings-p1p2   codex/findings-*      must fix P0-P2; P3 may be filed and deferred
#   lane:feature         everything else       must fix P0-P1; P2-P3 may be filed and deferred
#
# A pull request is READY when all of these hold on its current head and base:
#   - not a draft; GitHub reports it mergeable: DIRTY, UNKNOWN and BLOCKED fail closed, and
#     BEHIND fails closed while the default-branch ruleset requires an up-to-date branch (the
#     ruleset is read; once the merge queue replaces that requirement, BEHIND is what the queue
#     exists to handle and no longer blocks)
#   - the newest `upstroke-ci` and `upstroke-pr-policy` check runs on the head succeeded
#   - the latest review comment by the trusted reviewer above (the `Reviewed head:` workflow form
#     with its fenced JSON verdict, or the `<!-- upstroke-frontier-review -->` prose form) reviewed
#     the head itself, or a commit the head differs from only by clean merge commits (git's own
#     merge of the two parents, the branch diff byte-identical before and after, and no gate
#     edited by the pull request) and pushes confined to reviews/findings/ or reviews/FINDINGS.md
#     (MAINTAINING step 5 keeps the review across both)
#   - the review is against the pull request's own base: the workflow form must record its base
#     commit, which must lie on the current base branch (and, for a base other than master, not
#     on master); no base change may be recorded on the pull request after the review was
#     posted, which is checked again just before an enqueue; the prose form records no base, so
#     it is bound to the base only by that timeline check and counts only on a master-based
#     pull request
#   - no finding in that review has a severity the lane must fix
#   - every allowed finding has a ledger row in the body whose disposition is deferred, with
#     exactly one file under reviews/findings/ on the branch whose YAML frontmatter (the block
#     between the opening --- and the next) carries `id: <the finding id>`; a rejected or
#     accepted-risk row is the owner's call and sends the pull request to MANUAL
#
# This script decides whether to merge, so every uncertainty resolves to NOT-READY. A read that
# failed, a read that may be short, and a field the review did not record are each an uncertainty,
# and none of them is an empty result: an unreadable comment page blocks as `review-lookup-failed`,
# an unreadable timeline as `timeline-lookup-failed`, an unreadable pull request as
# `pr-lookup-failed`, a review the parser did not finish reading as `review-parse-incomplete`, a
# review that records no reviewed commit as `review-records-no-reviewed-sha`, a finding-file
# listing that errored as `finding-file-lookup-failed`, and a gate-edit check that could not run as
# `gate-edit-check-failed`; an unreadable ruleset list or open-pull-request list refuses the whole
# run before the first pull request is judged. Every list request is paginated, because a first
# page is not a list and 30 rows of nothing hid an active ruleset on page two.
#
# Reading "I could not look" as "there is nothing there" is precisely how a blocked pull request
# enqueues, and it has happened here more than once: a comment page whose failure was swallowed
# dropped the newest review and let an older PASS win; an unpaginated ruleset list dropped the
# BEHIND blocker; an empty `--reviewer` moved trust to the repository's owner. Each was one line,
# and each called merge.
#
# Other states: NEEDS-ATTEST (the head moved past the reviewed commit by more than clean merges
# and ledger pushes: a repair-only push the owner reads and attests under step 5, a merge commit
# that is not git's own merge of its parents, or a new change that needs another pass), MANUAL
# (the review is prose the audit cannot judge: findings without ids, or a severity token outside
# the numbered findings, so a person reads it), and NOT-READY with the blockers listed.
#
# Exactly one account's review comments count, and anyone else's comment carrying the markers is
# ignored, so a contributor cannot mint a PASS. Which account that is comes from the caller --
# --reviewer or UPSTROKE_REVIEW_AUTHOR -- and only when neither is given does a User owner stand
# in. The override moves that trust; it never widens it, and there is no value of it that admits
# a second author. MAINTAINING names the trusted writer, and pointing this audit at anyone else
# is a decision the caller states on the command line, where it is visible.
#
# Limits, stated so nobody reads more into READY than it says: the audit sees severities and the
# fields the review JSON carries. A finding whose object carries a witness, reproduction, repro,
# failing_test or mutation field that is not null, false or empty blocks in every lane, and so
# does one whose fields name a MUST deviation (a field named mandatory/deviation/must_*, or the
# word MUST in any string field); a witness or deviation that exists only in prose does not
# reach the audit, and the deferring implementor's row asserts there is none (MAINTAINING step 5
# binds them). A merge-in is checked on step 5's terms: git's own merge of its parents, the
# branch diff byte-identical before and after, no gate edited by the pull request, and no branch
# commit outside the ledger.
#
# The pure parts (lane, severity sets, the two review parsers, the frontmatter id match, the
# newest-check-run choice, the ledger-row parse) are functions, exercised by
# .github/scripts/test-pr-ready-audit.sh; sourcing this file with PR_READY_AUDIT_LIBRARY=1
# defines them without running the audit.
#
# Needs: bash, git (a checkout with `origin` pointing at the repository), gh (its built-in --jq
# does the API-side JSON work), and python3 or python for the verdict JSON; without a python
# the JSON form fails closed.

set -euo pipefail

# ---- pure helpers -------------------------------------------------------------------------------

lane_for() {
  case "$1" in
    codex/findings-p3-*) echo findings-p3 ;;
    codex/findings-*) echo findings-p1p2 ;;
    *) echo feature ;;
  esac
}

must_fix_for() {
  case "$1" in
    findings-p3) echo "P0 P1 P2 P3" ;;
    findings-p1p2) echo "P0 P1 P2" ;;
    feature) echo "P0 P1" ;;
  esac
}

# review_kind FILE: "json" when the comment carries a fenced ```json verdict (or the older bare
# role_understanding object), "prose" otherwise. A prose review that merely quotes JSON is prose.
review_kind() {
  if grep -qE '^```json|"role_understanding"' "$1"; then echo json; else echo prose; fi
}

# parse_verdict_json FILE: the workflow form. Prints tab-separated lines:
#   META <reviewed_sha or -> <verdict or -> <base_sha or ->  the one object the findings come from
#   STRAY <tokens> 0                            severity or MUST tokens found outside that object
#   <severity> <id or -> <bits>                 one per finding; bits: 1 = witness field, 2 = MUST
#   ERR <reason> 0                              an object or finding the parser cannot judge
#   END - 0                                     the parser reached the end of the findings
#
# END is the whole point of the contract. Every other line is something the parser found, and a
# parser that dies partway prints fewer of them -- not an error the caller can see, just a shorter
# findings list, which is a weaker verdict. `PYTHONIOENCODING=ascii` in the environment and one
# non-ASCII character in a finding id is enough: META prints, the finding does not, and a PASS
# carrying a deferred finding audits as a PASS carrying none, which is READY and a merge call.
# No field is ever empty either: `read` with IFS=tab folds runs of tabs together, so an empty
# reviewed_sha shifted the verdict into its column and the base into the verdict's.
parse_verdict_json() {
  local py
  py="$(command -v python3 || command -v python || true)"
  if [[ -z "$py" ]]; then
    printf 'ERR\tno-python\t0\nEND\t-\t0\n'
    return 0
  fi
  "$py" - "$1" <<'PY'
import json, re, sys
sys.stdout.reconfigure(newline=chr(10))  # a Windows python writes CRLF to a pipe, and bash would read the CR into the last field
text = open(sys.argv[1], encoding="utf-8").read()
# The verdict is the last fenced JSON object; the older bare form has no fence.
found = re.findall(r"```json\s*(\{.*?\})\s*```", text, re.S) or re.findall(r"(\{\"role_understanding.*\})", text, re.S)
try:
    verdict = json.loads(found[-1]) if found else None
except ValueError:
    verdict = None
if not isinstance(verdict, dict) or not isinstance(verdict.get("findings"), list):
    print("ERR\tunparsed\t0")
    sys.exit(0)
# Identity, verdict, base and findings from this one object. Anything in the comment outside the
# object that looks like a finding is for a person, not for the parser.
print("META\t" + (str(verdict.get("reviewed_sha", "")).strip() or "-") + "\t" + (str(verdict.get("verdict", "")).strip() or "-") + "\t" + (str(verdict.get("base_sha", "")).strip() or "-"))
outside = text.replace(found[-1], "")
stray = sorted(set(re.findall(r"\b(?:P[0-3]|MUST)\b", outside)))
if stray:
    print("STRAY\t" + "/".join(stray) + "\t0")
def present(v):
    return v not in (None, False, "", [], {}) and str(v).strip() != ""
for f in verdict["findings"]:
    if not isinstance(f, dict):
        print("ERR\tunparsed\t0")
        continue
    sev = str(f.get("severity", "")).strip()
    fid = str(f.get("id", "")).strip() or "-"
    wit = int(any(present(f.get(k)) for k in ("witness", "reproduction", "repro", "failing_test", "mutation", "mutation_witness")))
    # A MUST deviation is fixed whatever its label (MAINTAINING step 5): any field of the finding
    # naming MUST as a word, or a field whose name says mandatory/deviation, marks it.
    must = 0
    for k, v in f.items():
        if re.search(r"(mandatory|deviation|must_)", str(k), re.I) and present(v):
            must = 1
        if isinstance(v, str) and re.search(r"\bMUST\b", v):
            must = 1
    if not re.fullmatch(r"P[0-3]", sev):
        print("ERR\tbad-severity:" + fid + "\t0")
        continue
    print(sev + "\t" + fid + "\t" + str(wit + 2 * must))
print("END\t-\t0")
PY
}

# parse_prose_review FILE: the frontier form, read conservatively. Prints the same shape, with the
# same END contract and the same rule that no field is ever empty:
#   META <head from the marker or Reviewed-head line, or -> <last VERDICT or -> -
#   <severity> - 0     one per numbered "N. **P<n>" finding
#   STRAY <tokens> 0   any P0-P3 or MUST token outside the numbered findings, PASS included
#   END - 0            the parser reached the end
parse_prose_review() {
  local f="$1" head verdict stray
  head="$(grep -oE '(head=|Reviewed head: )[0-9a-f]{40}' "$f" | head -1 | grep -oE '[0-9a-f]{40}' || true)"
  verdict="$(grep -oE 'VERDICT:\**:? *[A-Z_]+' "$f" | tail -1 | grep -oE '[A-Z_]+$' || true)"
  printf 'META\t%s\t%s\t-\n' "${head:-"-"}" "${verdict:-"-"}"
  grep -oE '^[0-9]+\. \*\*P[0-3]' "$f" | grep -oE 'P[0-3]' | sed 's/$/\t-\t0/' || true
  stray="$(grep -vE '^[0-9]+\. \*\*P[0-3]' "$f" | grep -oE '\bP[0-3]\b|\bMUST\b' | sort -u | tr '\n' '/' | sed 's#/$##' || true)"
  [[ -n "$stray" ]] && printf 'STRAY\t%s\t0\n' "$stray"
  printf 'END\t-\t0\n'
  return 0
}

# frontmatter_has_id ID: reads a finding file on stdin and succeeds only when its YAML
# frontmatter, the block between the opening `---` on line 1 and the next `---`, carries the
# line `id: ID` (README: the id lives in the frontmatter, not the name). The same line in prose
# or a code block further down is not a frontmatter id. The id is a fixed string, whole line.
frontmatter_has_id() {
  awk 'NR == 1 { if ($0 != "---") exit; next } $0 == "---" { exit } { print }' | grep -qxF "id: $1"
}

# finding_file_count ID TREEISH: how many files under reviews/findings/ in TREEISH carry `id: ID`
# in their YAML frontmatter. A non-zero status means the listing, or one of the files it named,
# could not be read -- which is not a count of zero. This rule wants exactly one file, so a read
# that quietly does not count can turn two files into one as easily as one into none, and `git
# grep` exits 1 for "nothing matched" and above 1 for an error, which `|| true` read as one answer.
#
# The names arrive NUL-separated through a file rather than a command substitution: `$(...)` drops
# NUL bytes, so the separators would vanish and the loop would read nothing at all -- a count of
# zero for every finding, on a tree that holds the file. The gate builds a small repository and
# counts in it, because that is the mistake a shape rule does not catch.
finding_file_count() {
  local id="$1" treeish="$2" cand_file blob_file status=0 n=0 cand
  cand_file="$(mktemp)"
  blob_file="$(mktemp)"
  git grep -l -z -F -e "id: $id" "$treeish" -- 'reviews/findings/*.md' > "$cand_file" 2>/dev/null \
    || status=$?
  if ((status > 1)); then rm -f "$cand_file" "$blob_file"; return 1; fi
  # NUL-separated names, so a path with whitespace stays one candidate.
  while IFS= read -r -d '' cand; do
    cand="${cand#*:}"   # git grep prefixes each name with "<treeish>:"
    if ! git show "$treeish:$cand" > "$blob_file" 2>/dev/null; then
      rm -f "$cand_file" "$blob_file"
      return 1
    fi
    if frontmatter_has_id "$id" < "$blob_file"; then n=$((n + 1)); fi
  done < "$cand_file"
  rm -f "$cand_file" "$blob_file"
  printf '%s' "$n"
}

# newest_per_name: reads "name<TAB>id<TAB>conclusion-or-status" lines, one per check run across
# every page, and prints "name=value " for the highest id per name. GitHub assigns check-run
# ids in creation order, so the highest id is the newest run whether or not it ever started.
newest_per_name() {
  sort -t $'\t' -k1,1 -k2,2n | awk -F'\t' '{ last[$1] = $3 } END { for (n in last) printf "%s=%s ", n, last[n] }'
}

# ledger_rows_from_body: reads a pull-request body on stdin and prints one line per ledger row,
# "ID<TAB>severity<TAB>sha-or-location<TAB>disposition".
ledger_rows_from_body() {
  awk -F'|' '
    /^## Review finding ledger/ { inledger = 1; next }
    /^## / { inledger = 0 }
    inledger && /^\|/ && $2 !~ /^ *ID *$/ && $2 !~ /^-+$/ && $2 !~ /None yet/ {
      gsub(/^ +| +$/, "", $2); gsub(/^ +| +$/, "", $3); gsub(/^ +| +$/, "", $4); gsub(/^ +| +$/, "", $10)
      print $2 "\t" $3 "\t" $4 "\t" $10
    }'
}

# ---- GitHub-facing helpers ----------------------------------------------------------------------

# Creates a label only when the repository has none of that name: an existing label, including
# one handed in as --ready-label, keeps its colour and description.
ensure_labels() {
  local existing
  existing="$(gh api "repos/$repo/labels?per_page=100" --paginate --jq '.[].name')"
  create() {
    grep -qxF "$1" <<< "$existing" \
      || gh label create "$1" --repo "$repo" --color "$2" --description "$3" >/dev/null
  }
  create lane:feature 0e8a16 "feature or sweep work: fix P0-P1, file P2-P3"
  create lane:findings-p1p2 fbca04 "P1/P2 findings workflow: fix P0-P2, file P3"
  create lane:findings-p3 d93f0b "P3 findings workflow: ready only on PASS"
  create "$ready_label" 5319e7 "audit passed: enqueue for merge"
}

# ruleset_state: prints "<strict> <queue>", 1 or 0 each: whether an active branch ruleset still
# requires an up-to-date branch, and whether one carries the merge-queue rule. Every active
# branch ruleset is read, which over-approximates on the safe side.
# Non-zero means the rulesets could not be read, which is not the same answer as "there are
# none": `for id in $(gh api ...)` runs zero times either way, and zero active rulesets reads as
# "nothing requires an up-to-date branch", which drops the BEHIND blocker. The list is fetched
# first and its status checked before the loop, so a failure refuses instead of relaxing.
#
# `--paginate`, and it is not decoration. This endpoint defaults to 30 per page, so 30 rulesets in
# any state hid an active strict one on page two: the audit read "no ruleset requires an up-to-date
# branch", a BEHIND pull request with a clean review went READY, and merge was called. A page-two
# HTTP 500 gave the identical answer, because a list the request never asked for and a list the
# request failed to get are the same missing rows. A first page is not a list.
ruleset_state() {
  local strict=0 queue=0 id ids rules
  ids="$(gh api "repos/$repo/rulesets?targets=branch&per_page=100" --paginate \
    --jq '.[] | select(.enforcement == "active") | .id')" || return 1
  for id in $ids; do
    rules="$(gh api "repos/$repo/rulesets/$id" --jq '.rules[] | "\(.type)=\(.parameters.strict_required_status_checks_policy // "")"')" || return 1
    grep -q '^merge_queue=' <<< "$rules" && queue=1
    grep -q '^required_status_checks=true$' <<< "$rules" && strict=1
  done
  echo "$strict $queue"
}

# valid_login LOGIN: whether LOGIN is shaped like a GitHub account name -- 1 to 39 characters of
# ASCII letters and digits, single hyphens between them and none at either end. Nothing that fails
# this is widened to fit. The value is a trust decision and it reaches a jq program, so a string
# GitHub cannot issue as a login is a typo, another option read by mistake, or an injection
# attempt; none of the three is an account, and each is refused rather than carried.
#
# The set is written out character by character. A range inside a bash regex is resolved by the
# locale's collating order rather than by ASCII, so `[A-Za-z0-9]` is not the ASCII alphabet it
# looks like: under en_US.utf8 it admits `é` and U+212A KELVIN SIGN, and `--reviewer evéntloops`
# reached the API as a login nobody has -- the silent `no-review` this flag exists to end, let
# through by the check that promised to stop it. `[[:alnum:]]` has the same defect for the same
# reason. An explicit list means one thing in every locale, and the gate runs it under two.
valid_login() {
  local ascii_alnum='[0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz]'
  [[ "$1" =~ ^$ascii_alnum($ascii_alnum|-$ascii_alnum)*$ ]] && ((${#1} <= 39))
}

# reviewer_login STATE OVERRIDE OWNER_LOGIN OWNER_TYPE: the account whose reviews count, or
# nothing and a non-zero status when it cannot be settled. STATE is the word `given` or `absent`
# and nothing else. Otherwise the repository's owner stands in, but only when that owner is a
# User: an Organization owns no voice and cannot have written a review, so inheriting it produces
# a filter that matches nothing. Refusing here turns a fleet-wide silent `no-review` into one loud
# message at startup. Whichever side supplies it, the answer must be a login: this is the last
# gate before the value becomes the audit's notion of who may say PASS, and it holds even where
# main checked first.
#
# STATE exists because an empty override is not an absent one and only the caller knows which it
# is. This used to fall back to the owner whenever the override was empty, so `--reviewer ""` --
# an unset shell variable expanded into the flag -- silently moved trust to the repository's owner:
# on a User-owned repository whose owner had posted PASS and whose named reviewer had posted a
# newer block, adding that one empty flag turned NOT-READY into READY and called merge. A supplied
# value is validated whatever it holds; the owner stands in only when nothing was supplied at all.
# A STATE that is neither word is a caller that cannot say, and that refuses too: there is no
# default here, because every default here is a decision about whose approval counts.
reviewer_login() {
  local state="$1" override="$2" owner_login="$3" owner_type="$4"
  case "$state" in
    given)
      valid_login "$override" || return 1
      printf '%s' "$override"
      return 0 ;;
    absent) ;;
    *) return 1 ;;
  esac
  if [[ "$owner_type" == "User" ]] && valid_login "$owner_login"; then
    printf '%s' "$owner_login"
    return 0
  fi
  return 1
}

# review_comment_filter: the jq program latest_review_id runs, held here rather than inline so the
# gate can run the exact text the audit runs. The login is not in that text: it arrives as string
# data in UPSTROKE_AUDIT_REVIEWER, which the call sets and so always overrides whatever the
# environment held (`gh api --jq` takes no --arg, and env is its way to pass a value in). A string
# shaped like jq is therefore compared as a login and matches nothing, rather than becoming part
# of the predicate. Both sides are lowered first because GitHub resolves `EventLoops` and
# `eventloops` to one account and an audit that split them would report the correct reviewer's
# blocking review as `no-review`. An unset variable is an error here, not an empty match: this
# audit's failures must be loud, and a filter that quietly matches nobody is the defect it exists
# to prevent.
#
# The two type guards come first and they are not defensive decoration. GitHub's REST schema lets
# an issue comment's `user` be null -- a deleted account -- and `ascii_downcase` raises on null,
# which fails the whole page, not the one comment: `--paginate` runs this program once per page,
# so one unrelated comment by a deleted account removed every review on its page from the
# comparison and let an older PASS win. A comment whose author or body is not a string is not
# this reviewer's review; it is dropped, and the fetch's own status is what reports failure.
review_comment_filter() {
  cat <<'JQ'
[ .[]
  | select((.user.login? | type) == "string")
  | select((.body? | type) == "string")
  | select((.user.login | ascii_downcase) == (env.UPSTROKE_AUDIT_REVIEWER | ascii_downcase))
  | select(.body | test("<!-- upstroke-frontier-review|Reviewed head: [0-9a-f]{40}"))
] | last | select(. != null) | "\(.created_at) \(.id)"
JQ
}

# latest_review_id PR: the id of the newest review comment posted by the trusted reviewer.
# Empty output and status 0 mean the reviewer has posted none. Non-zero means the lookup itself
# did not complete -- a page `gh` could not read or parse, an API error -- and the answer is
# unknown; the caller must not read that as "none".
#
# `--paginate` hands `--jq` each page separately, so `last` is per page: each page yields its
# newest match as "<created_at> <id>" and the newest across pages wins by timestamp. That is
# exactly why a suppressed failure is not a conservative default here. A lost page removes
# candidates from a comparison that takes the newest, so losing the page holding the newest
# review promotes an older one -- and the older one may be the PASS that the newest revoked.
# This function used to end in `return 0`, which discarded that status along with the pipeline's
# own, and a blocked pull request enqueued on a stale PASS. Fetch first, check, then reduce.
latest_review_id() {
  local matches
  matches="$(UPSTROKE_AUDIT_REVIEWER="$reviewer" \
    gh api "repos/$repo/issues/$1/comments?per_page=100" --paginate --jq "$(review_comment_filter)")" \
    || return 1
  [[ -n "$matches" ]] || return 0
  sort <<< "$matches" | tail -1 | awk '{print $2}'
}

# base_changed_after PR ISO-TIME: prints `yes` when the pull request's base was changed after
# that moment -- a diff the review posted before it cannot have seen -- and `no` when it was not.
# Non-zero means the timeline could not be read, and that is a third answer, not `no`: this
# gates the enqueue, and `for when in $(gh api ...)` iterates zero times whether the timeline
# holds no base change or the request failed, so a swallowed failure would enqueue a pull request
# retargeted since its review. Same defect as the comment lookup above, same consequence.
base_changed_after() {
  local timeline when
  timeline="$(gh api "repos/$repo/issues/$1/timeline?per_page=100" --paginate \
    --jq '.[] | select(.event == "base_ref_changed") | .created_at')" || return 1
  for when in $timeline; do
    [[ "$when" > "$2" ]] && { echo yes; return 0; }
  done
  echo no
}

# ---- the audit ----------------------------------------------------------------------------------

# The same sentence ends every malformed-login refusal: the reason a near-miss is refused rather
# than tried is that trying it is what silence looks like.
bad_login_why="Naming an account that cannot exist reads as no-review on every pull request."

# option_like VALUE: whether VALUE is an option rather than a value for one. The test is the
# leading hyphen, not a list of the options the `case` below takes: the guard that rejected only
# `--*` was such a list, it had already drifted from the parser, and `--ready-label -h` consumed
# `-h` and ran a whole audit. Nothing this script takes as an option's value begins with a hyphen
# -- a GitHub login cannot, and a label name that does is refused rather than swallowed, which is
# the safe direction for a guard whose only job is to not eat the next flag. `-*`, not `-?*`: the
# shorter pattern required a character after the hyphen, so a lone `-` was not an option to it and
# `--ready-label -` labelled a pull request `-` and enqueued it.
option_like() { [[ "$1" == -* ]]; }

main() {
  apply=0
  enqueue=0
  ready_label="ready-to-merge"
  prs=()
  local reviewer_flag="" reviewer_given=0 reviewer_state reviewer_override reviewer_said
  while (($#)); do
    case "$1" in
      --apply) apply=1 ;;
      --enqueue) apply=1; enqueue=1 ;;
      --ready-label)
        # $2 is read only once it is known to exist and to not be another option: an absent
        # argument aborted on `$2: unbound variable`, and a following flag was consumed as the
        # value, which switched that flag off without saying so.
        (($# >= 2)) || { echo "refusing: --ready-label needs a label name" >&2; exit 2; }
        option_like "$2" && { echo "refusing: --ready-label needs a label name, got the option [$2]" >&2; exit 2; }
        [[ "$2" == lane:* ]] && { echo "refusing: --ready-label must not be a lane:* label" >&2; exit 2; }
        ready_label="$2"; shift ;;
      --reviewer)
        (($# >= 2)) || { echo "refusing: --reviewer needs a login" >&2; exit 2; }
        option_like "$2" && { echo "refusing: --reviewer needs a login, got the option [$2]" >&2; exit 2; }
        # The value is kept, not yet judged: which source wins is settled after the loop, and only
        # the value that wins is checked. Checking the environment before the loop meant an
        # inherited UPSTROKE_REVIEW_AUTHOR this run replaces still had to be a valid login for the
        # run to start, and even --help refused.
        reviewer_flag="$2"; reviewer_given=1; shift ;;
      -h|--help) sed -n '2,20p' "$0"; exit 0 ;;
      *) prs+=("$1") ;;
    esac
    shift
  done

  # Source precedence first, then validation of the one value this run will trust -- and the
  # validation is not conditional on the value being non-empty. `--reviewer ""` used to skip it for
  # being falsy and fall through to the owner. `${VAR+given}` rather than `${VAR:-}` for the same
  # reason: a variable that is set and empty was supplied, by a wrapper whose own variable was
  # unset, and reading it as "nothing was supplied" hands the run to the repository's owner.
  if ((reviewer_given)); then
    reviewer_state=given
    reviewer_override="$reviewer_flag"
    reviewer_said="--reviewer [$reviewer_override]"
  elif [[ -n "${UPSTROKE_REVIEW_AUTHOR+given}" ]]; then
    reviewer_state=given
    reviewer_override="$UPSTROKE_REVIEW_AUTHOR"
    reviewer_said="UPSTROKE_REVIEW_AUTHOR=[$reviewer_override]"
  else
    reviewer_state=absent
    reviewer_override=""
    reviewer_said="no reviewer"
  fi
  if [[ "$reviewer_state" == given ]] && ! valid_login "$reviewer_override"; then
    echo "refusing: $reviewer_said is not a GitHub login." >&2
    echo "  A login is 1-39 letters, digits and single interior hyphens. $bad_login_why" >&2
    exit 2
  fi

  repo="$(gh repo view --json nameWithOwner --jq .nameWithOwner)"
  local owner_login owner_type
  owner_login="$(gh api "repos/$repo" --jq .owner.login)"
  owner_type="$(gh api "repos/$repo" --jq .owner.type)"
  if ! reviewer="$(reviewer_login "$reviewer_state" "$reviewer_override" "$owner_login" "$owner_type")"; then
    if [[ "$reviewer_state" == given ]]; then
      echo "refusing: [$reviewer_override] is not a GitHub login. $bad_login_why" >&2
    else
      echo "refusing: $repo is owned by $owner_login (type $owner_type), which authors no review comments." >&2
      echo "  Name the account whose reviews count: --reviewer LOGIN, or UPSTROKE_REVIEW_AUTHOR=LOGIN." >&2
      echo "  Without it every pull request reads no-review and nothing is ever READY." >&2
    fi
    exit 2
  fi
  local rulesets
  # `read <<< "$(...)"` cannot see the failure of what it reads, and an unread ruleset would read
  # as "no ruleset requires an up-to-date branch", quietly dropping the BEHIND blocker for every
  # pull request in the run. This is one fact for the whole run, so a failure refuses the run.
  if ! rulesets="$(ruleset_state)"; then
    echo "refusing: could not read $repo's branch rulesets." >&2
    echo "  Their state decides whether a BEHIND pull request blocks; unread, every one of them" >&2
    echo "  would audit as though no ruleset required an up-to-date branch." >&2
    exit 2
  fi
  read -r strict_up_to_date has_queue <<< "$rulesets"

  if ((${#prs[@]} == 0)); then
    # Captured and checked rather than read through `< <(...)`, whose status nothing can see. An
    # unread list becomes an empty one, and an empty one is a run that prints a header, audits
    # nothing and exits 0 -- which is exactly what "there is nothing to do" looks like. This audit
    # exists because that pair was indistinguishable once already.
    local open_prs
    if ! open_prs="$(gh api "repos/$repo/pulls?state=open&per_page=100" --paginate --jq '.[].number')"; then
      echo "refusing: could not list $repo's open pull requests." >&2
      echo "  An unread list is not an empty one. Continuing would print a table with nothing in" >&2
      echo "  it and exit 0, which reads as: every pull request was audited and none was ready." >&2
      exit 2
    fi
    [[ -n "$open_prs" ]] && mapfile -t prs <<< "$open_prs"
  fi
  if ((apply)); then ensure_labels; fi

  printf '%-5s %-14s %-8s %-13s %s\n' PR LANE HEAD STATE DETAIL
  local pr
  for pr in "${prs[@]}"; do
    audit_one "$pr"
  done
}

audit_one() {
  local pr="$1"
  local meta meta_raw meta_status branch head draft merge_state labels base base_oid attempt
  # GitHub computes mergeability lazily and answers UNKNOWN until it has; asking again after a
  # pause usually settles it, and an UNKNOWN that survives three asks fails closed below. A read
  # that fails outright gets the same three attempts and then says so: read through `< <(...)` its
  # status was invisible, the fields came back empty, and the run died further down on an empty
  # head with no line printed and no reason given -- safe, but neither complete nor legible.
  for attempt in 1 2 3; do
    meta=()
    meta_status=0
    # One field per line, read with mapfile: a tab-separated read would collapse an empty field
    # (no labels) and shift every field after it.
    meta_raw="$(gh pr view "$pr" --repo "$repo" \
      --json headRefName,headRefOid,isDraft,mergeStateStatus,labels,baseRefName,baseRefOid \
      --jq '.headRefName, .headRefOid, (.isDraft|tostring), .mergeStateStatus, ([.labels[].name]|join(" ")), .baseRefName, .baseRefOid')" \
      || meta_status=$?
    ((meta_status == 0)) && mapfile -t meta <<< "$meta_raw"
    branch="${meta[0]:-}"; head="${meta[1]:-}"; draft="${meta[2]:-}"; merge_state="${meta[3]:-}"
    labels="${meta[4]:-}"; base="${meta[5]:-}"; base_oid="${meta[6]:-}"
    [[ ( "$merge_state" == UNKNOWN || $meta_status -ne 0 ) && $attempt -lt 3 ]] || break
    sleep 3
  done
  if ((meta_status != 0)); then
    # Nothing below can be judged without the head, the base and the draft flag, so nothing below
    # is attempted: this pull request is reported unaudited rather than audited on empty fields.
    printf '%-5s %-14s %-8s %-13s %s\n' "#$pr" "-" "-" NOT-READY "blockers=pr-lookup-failed"
    return 0
  fi
  local lane must_fix state
  lane="$(lane_for "$branch")"
  must_fix="$(must_fix_for "$lane")"
  blockers=()
  state=READY

  [[ "$draft" == "true" ]] && blockers+=("draft")
  case "$merge_state" in
    DIRTY) blockers+=("conflicts") ;;
    UNKNOWN|"") blockers+=("mergeability-unknown") ;;   # GitHub has not computed it yet
    BLOCKED) blockers+=("blocked-by-rules") ;;          # a ruleset requirement is unmet, e.g. an unresolved conversation
    BEHIND)                                             # out of date: a blocker only while the ruleset demands an update
      ((strict_up_to_date)) && blockers+=("behind-base:ruleset-requires-up-to-date") ;;
  esac

  # The newest check run per required context on the head, chosen across every page in the
  # shell (`--paginate` runs the jq filter per page).
  local checks ctx
  checks="$(gh api "repos/$repo/commits/$head/check-runs?per_page=100" --paginate \
    --jq '.check_runs[] | select(.name == "upstroke-ci" or .name == "upstroke-pr-policy") | "\(.name)\t\(.id)\t\(.conclusion // .status)"' \
    | newest_per_name)"
  for ctx in upstroke-ci upstroke-pr-policy; do
    case " $checks " in
      *" $ctx=success "*) ;;
      *" $ctx="*) blockers+=("$ctx:${checks##*"$ctx="}"); blockers[-1]="${blockers[-1]%% *}" ;;
      *) blockers+=("$ctx:missing") ;;
    esac
  done

  # The latest review by the trusted account: its posting time, its kind, and its parse.
  local review_id review_at review_file kind reviewed="" verdict="" review_base="-"
  findings=()
  # Three outcomes, kept apart. A lookup that failed is not a pull request without a review: the
  # audit does not know what the reviewer said, so it says so and blocks, rather than proceeding
  # on whatever survived the failure.
  if ! review_id="$(latest_review_id "$pr")"; then
    blockers+=("review-lookup-failed")
  elif [[ -z "$review_id" ]]; then
    blockers+=("no-review")
  else
    review_at="$(gh api "repos/$repo/issues/comments/$review_id" --jq '.created_at')"
    review_file="$(mktemp)"
    gh api "repos/$repo/issues/comments/$review_id" --jq '.body' > "$review_file"
    kind="$(review_kind "$review_file")"
    local sev id wit parse_complete=0
    while IFS=$'\t' read -r sev id wit extra; do
      case "$sev" in
        END) parse_complete=1 ;;
        META) reviewed="$id"; verdict="$wit"; review_base="${extra:-"-"}"
              # `-` is how the parsers say "the object did not record this"; it is not a value.
              [[ "$reviewed" == "-" ]] && reviewed=""
              [[ "$verdict" == "-" ]] && verdict="" ;;
        STRAY) blockers+=("manual:$id-outside-the-$([[ "$kind" == json ]] && echo verdict-object || echo numbered-findings)") ;;
        "") ;;
        *) findings+=("$sev"$'\t'"$id"$'\t'"$wit") ;;
      esac
    done < <(if [[ "$kind" == json ]]; then parse_verdict_json "$review_file"; else parse_prose_review "$review_file"; fi)
    rm -f "$review_file"

    # The parser's own END. Without it the stream is whatever the parser managed to print before it
    # died, which is a findings list that is short by an unknown amount -- and every finding it
    # failed to print is a blocker this audit will not raise. A short list is not a clean review.
    ((parse_complete)) || blockers+=("review-parse-incomplete")
    # A review that does not say which commit it reviewed cannot be checked against the head. This
    # used to land on a blocker only because `read` with IFS=tab folded the empty field and shifted
    # a later column into `verdict`; it is stated now rather than inherited from a quirk.
    [[ -z "$reviewed" ]] && blockers+=("review-records-no-reviewed-sha")

    case "$verdict" in
      PASS|CHANGES_REQUIRED) ;;
      "") blockers+=("no-verdict") ;;
      *) blockers+=("verdict:$verdict") ;;
    esac
    [[ "$lane" == findings-p3 && "$verdict" != PASS ]] && blockers+=("verdict-not-pass")
    [[ "$verdict" == PASS && ${#findings[@]} -gt 0 ]] && blockers+=("pass-with-findings")
    [[ "$verdict" == CHANGES_REQUIRED && ${#findings[@]} -eq 0 ]] && blockers+=("findings-unparsed:changes-required-lists-none")

    # The review must be against this pull request's own base (MAINTAINING step 4): a base
    # changed after the review was posted is a diff the review never saw, and the workflow
    # form's base commit must lie on the current base branch (and, off master, not on master,
    # since the integration branch carries master's history too).
    local retargeted
    if ! retargeted="$(base_changed_after "$pr" "$review_at")"; then
      blockers+=("timeline-lookup-failed")
    elif [[ "$retargeted" == yes ]]; then
      blockers+=("retargeted-after-review")
    fi
  fi

  # Head movement since the reviewed commit, and the base the review was made against.
  moved=""
  if [[ -n "$reviewed" ]]; then
    # refs/pull/N/head is the head whatever repository it lives in; a fork's branch is not on
    # origin, so fetching by branch name would leave the head and the reviewed commit unknown.
    git fetch -q origin "refs/pull/$pr/head" "$base" master 2>/dev/null || true
    if [[ "$kind" == json && "$review_base" == "-" ]]; then
      blockers+=("review-records-no-base")          # the workflow form always records base_sha; one without it is not judged
    elif [[ "$review_base" != "-" ]]; then
      if ! git cat-file -e "$review_base^{commit}" 2>/dev/null \
        || ! git merge-base --is-ancestor "$review_base" "origin/$base"; then
        blockers+=("review-base-not-on-$base:${review_base:0:7}")
      elif [[ "$base" != master ]] && git merge-base --is-ancestor "$review_base" origin/master; then
        blockers+=("review-base-on-master-not-$base:${review_base:0:7}")
      fi
    elif [[ "$base" != master ]]; then
      blockers+=("manual:prose-review-records-no-base-and-base-is-$base")
    fi
    if ! git cat-file -e "$reviewed^{commit}" 2>/dev/null; then
      blockers+=("reviewed-sha-unknown:${reviewed:0:7}")
    elif ! git merge-base --is-ancestor "$reviewed" "$head"; then
      blockers+=("reviewed-not-ancestor:${reviewed:0:7}")
    elif [[ "$reviewed" != "$head" ]]; then
      # A merge-in keeps the review only on MAINTAINING step 5's terms: the merge commit is
      # exactly what git produces from its two parents on its own (a hand edit, a conflict
      # resolution or a third parent is a new change that `--no-merges` below would hide), the
      # branch's diff against its base is byte-identical before and after the merge, and the pull
      # request edits no gate. Anything wider is reviewed again.
      local merge_edits="" merges m expected before after touched
      merges="$(git rev-list --merges "$reviewed..$head" --not "origin/$base")"
      for m in $merges; do
        local parents
        if ! parents="$(git rev-list --parents -n 1 "$m")"; then merge_edits="$m"; break; fi
        if (($(wc -w <<< "$parents") != 3)); then merge_edits="$m"; break; fi
        if ! expected="$(git merge-tree --write-tree "$m^1" "$m^2" 2>/dev/null)"; then
          merge_edits="$m"; break   # a conflict, or a git too old for --write-tree: fail closed
        fi
        [[ "$(git rev-parse "$m^{tree}")" == "$expected" ]] || { merge_edits="$m"; break; }
        # Byte-identical branch diff: the branch side against its base before the merge, and the
        # merge against the side it merged in, must be the same patch.
        before="$(git diff "$(git merge-base "$m^1" "$m^2")" "$m^1" | git hash-object --stdin)"
        after="$(git diff "$m^2" "$m" | git hash-object --stdin)"
        [[ "$before" == "$after" ]] || { merge_edits="$m"; break; }
      done
      if [[ -n "$merges" && -z "$merge_edits" ]]; then
        # Read into a variable first: as a pipeline inside the condition, a `git diff` that failed
        # made the condition false, which is the same as "this pull request edits no gate" -- the
        # exemption granted by a read that did not happen.
        local gate_edits
        if ! gate_edits="$(git diff --name-only "origin/$base...$head" -- .github/workflows .github/scripts)"; then
          blockers+=("gate-edit-check-failed")
        elif [[ -n "$gate_edits" ]]; then
          blockers+=("review-stale:gate-edit-with-merge-in")   # step 5: no exemption for a gate-editing pull request
        fi
      fi
      # Commits the base already has arrived through a merge-in; only the branch's own count.
      touched="$(git log --no-merges --name-only --format= "$reviewed..$head" --not "origin/$base" | sort -u)"
      if [[ -n "$merge_edits" ]]; then
        moved="merge-edits:${merge_edits:0:7}"
      elif [[ -z "$touched" ]]; then
        moved="merges-only"
      elif ! grep -vE '^reviews/(findings/|FINDINGS\.md$)' <<< "$touched" >/dev/null; then
        moved="ledger-only"
      else
        moved="repairs"
      fi
    fi
  fi

  local rows f disposition nfiles
  rows="$(gh pr view "$pr" --repo "$repo" --json body --jq .body | ledger_rows_from_body)"
  for f in "${findings[@]}"; do
    IFS=$'\t' read -r sev id wit <<< "$f"
    [[ "$id" == "-" ]] && id=""   # "-" stands in for "no id": a tab-separated read collapses empty fields
    if [[ "$sev" == ERR ]]; then
      blockers+=("findings-unparsed:$id")
      continue
    fi
    # wit is a bit field from the parser: 1 = a witness field is present, 2 = a MUST deviation.
    if (( wit & 2 )); then
      blockers+=("must-deviation:${id:-unnamed}")
      continue
    fi
    if (( wit & 1 )); then
      blockers+=("witnessed:${id:-unnamed}")
      continue
    fi
    if [[ " $must_fix " == *" $sev "* ]]; then
      blockers+=("open-$sev:${id:-unnamed}")
      continue
    fi
    if [[ -z "$id" ]]; then
      blockers+=("manual:$sev-without-id")
      continue
    fi
    disposition="$(awk -F'\t' -v id="$id" '$1 == id { print $4; exit }' <<< "$rows")"
    case "$disposition" in
      deferred)   # the lane rule: an allowed finding is filed and deferred, one file per finding
        if ! nfiles="$(finding_file_count "$id" "$head")"; then
          blockers+=("finding-file-lookup-failed:$id")
          continue
        fi
        case "$nfiles" in
          1) ;;
          0) blockers+=("no-file:$id") ;;
          *) blockers+=("duplicate-file:$id") ;;
        esac ;;
      rejected|accepted-risk) blockers+=("manual:disposition-$disposition:$id") ;;   # the owner's call, not the audit's
      fixed)
        [[ "$moved" == repairs || "$moved" == merge-edits:* ]] || blockers+=("fixed-but-head-unmoved:$id") ;;
      "") blockers+=("no-row:$id") ;;
      *) blockers+=("bad-disposition:$id=$disposition") ;;
    esac
  done

  # Hard blockers decide first; a repair push only matters once nothing else stands in the way.
  # A draft is NOT-READY: READY means enqueueable as it stands.
  local hard=() b
  for b in "${blockers[@]:-}"; do
    [[ -n "$b" && "$b" != manual:* ]] && hard+=("$b")
  done
  if ((${#hard[@]})); then
    state=NOT-READY
  elif [[ "$moved" == repairs || "$moved" == merge-edits:* ]]; then
    state=NEEDS-ATTEST
  elif printf '%s\n' "${blockers[@]:-}" | grep -q '^manual:'; then
    state=MANUAL
  fi

  local detail l
  detail="verdict=${verdict:-none} reviewed=${reviewed:0:7}${moved:+ moved=$moved}"
  [[ "$base" != master ]] && detail+=" base=$base"
  for l in lane:feature lane:findings-p1p2 lane:findings-p3; do
    [[ "$l" != "lane:$lane" && " $labels " == *" $l "* ]] && detail+=" lane-label-mismatch=$l"
  done
  ((${#blockers[@]})) && detail+=" blockers=$(IFS=,; echo "${blockers[*]}")"
  printf '%-5s %-14s %-8s %-13s %s\n' "#$pr" "$lane" "${head:0:7}" "$state" "$detail"

  if ((apply)); then
    for l in lane:feature lane:findings-p1p2 lane:findings-p3; do
      [[ "$l" != "lane:$lane" && " $labels " == *" $l "* ]] && gh pr edit "$pr" --repo "$repo" --remove-label "$l" >/dev/null
    done
    [[ " $labels " != *" lane:$lane "* ]] && gh pr edit "$pr" --repo "$repo" --add-label "lane:$lane" >/dev/null
    # The ready label is a report of this audit at $head, not an authorisation: GitHub labels are
    # not bound to a commit, so a push can always land between the audit and the label write.
    # The head is read again before the write and again after it, and a label written across a
    # move is removed; that narrows the window, it cannot close it. The act that is bound to the
    # audited head is the enqueue, through --match-head-commit, and nothing may treat the label
    # alone as permission to merge.
    local now after_write
    if [[ "$state" == READY ]]; then
      now="$(gh pr view "$pr" --repo "$repo" --json headRefOid --jq .headRefOid)"
      if [[ "$now" != "$head" ]]; then
        echo "      head moved to ${now:0:7} since the audit read ${head:0:7}: not labelled, not enqueued"
        state=HEAD-MOVED
      fi
    fi
    if [[ "$state" == READY ]]; then
      [[ " $labels " != *" $ready_label "* ]] && gh pr edit "$pr" --repo "$repo" --add-label "$ready_label" >/dev/null
      after_write="$(gh pr view "$pr" --repo "$repo" --json headRefOid --jq .headRefOid)"
      if [[ "$after_write" != "$head" ]]; then
        gh pr edit "$pr" --repo "$repo" --remove-label "$ready_label" >/dev/null
        echo "      head moved to ${after_write:0:7} while labelling ${head:0:7}: label removed, not enqueued"
        state=HEAD-MOVED
      fi
    fi
    if [[ "$state" == READY ]]; then
      if ((enqueue)); then
        # The base is read again and the timeline re-checked just before the call: a base changed
        # since the audit is a different diff, and --match-head-commit binds only the head. That
        # narrows the base window to the call itself; a retarget after the enqueue lands in the
        # queue on the new base with both contexts re-run there but without a review of that
        # diff, and it is visible on the pull request's timeline as a base change after the
        # review, which the next audit reports.
        local base_now retargeted_now
        base_now="$(gh pr view "$pr" --repo "$repo" --json baseRefName --jq .baseRefName)"
        if ! retargeted_now="$(base_changed_after "$pr" "$review_at")"; then
          echo "      could not re-read the timeline to check the base: not enqueued"
          state=BASE-MOVED
        elif [[ "$base_now" != "$base" || "$retargeted_now" == yes ]]; then
          echo "      base changed since the audit read $base: not enqueued"
          state=BASE-MOVED
        fi
      fi
    fi
    if [[ "$state" == READY ]]; then
      if ((enqueue)); then
        # --match-head-commit binds the enqueue to the head this audit judged: a push that
        # lands between the audit and this call makes GitHub refuse, never enqueue the newcomer.
        if gh pr merge "$pr" --repo "$repo" --merge --auto --match-head-commit "$head" >/dev/null 2>&1; then
          echo "      enqueued #$pr at ${head:0:7}"
        else
          echo "      could not enqueue #$pr at ${head:0:7} (head moved, already queued, or the ruleset has no merge queue yet)"
        fi
      fi
    else
      [[ " $labels " == *" $ready_label "* ]] && gh pr edit "$pr" --repo "$repo" --remove-label "$ready_label" >/dev/null
    fi
  fi
  return 0
}

if [[ "${PR_READY_AUDIT_LIBRARY:-0}" != 1 ]]; then
  main "$@"
fi
