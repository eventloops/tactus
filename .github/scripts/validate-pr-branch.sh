#!/usr/bin/env bash
# validate-pr-branch.sh <branch-name> [merge-base-findings [head-findings [range-findings]]]
#
# Refuse a pull request whose head branch is not in the branch vocabulary.
#
# WHY A BRANCH NAME IS POLICY AND NOT TASTE. The prefix already decides two
# things that bind a merge: the effort the frontier review is run at
# (review-poller.sh's lane_of and effort_for_lane) and the severities the pull
# request must fix before it is ready (scripts/pr-ready-audit.sh's lane_for and
# must_fix_for, the rule MAINTAINING.md states). Until this validator existed
# every prefix outside two `codex/` shapes fell into a `feature` catch-all, so a
# branch nobody had thought about was silently assigned the most expensive
# review and the loosest fix set, and nothing said so. An unrecognised prefix
# must fail here rather than default; that refusal is the point of this file,
# and the vocabulary below is the part that has to stay true.
#
# A lane:* label is an OUTPUT of that rule and never an input. Nothing may read
# a label to decide effort or must-fix: a label is not bound to a commit, so
# editing one would otherwise change which gates apply to a merge.
#
# THE VOCABULARY.
#
#   feature/<slug>                  new behaviour
#   refactor/<slug>                 behaviour-preserving change
#   docs/<slug>                     documentation only
#   standards/<slug>                a standards/ section, or a sweep under one
#   ci/<slug>                       gates, workflows, scripts/ and .github/
#   gate/<slug>                     a cumulative review-gate report
#   findings/<slug>                 reviews/findings/ alone: filing or curating
#   fix-P<n>/<category>_<desc>      exactly one finding, n in 0..3
#   bulk-fix-P<n>/<slug>            a batch of findings, n in 2..3
#
# <slug> and <desc> are lower-case words joined by single hyphens. <category>
# is one of the eight the finding filenames and validate-pr-body.sh use; the
# list is duplicated there and the two must move together.
#
# WHY THERE IS NO fix/ PREFIX. There was one, for "a bug that is not a filed
# finding", and it is retired. It was a second way to repair something, and the
# two disagreed about whether the repair was on the record: a fix-P*/ branch
# named the finding it closed, a fix/ branch named nothing and left the reviewer
# to work out what it was for. A bug worth a branch is worth a finding, so every
# repair now goes through fix-P<n>/ and the finding exists: filed by an earlier
# pull request, or filed by this one. Resolving the name across the whole pull
# request rather than at one end is what makes the second case work, and without
# it retiring fix/ would have forced one pull request to file a finding and a
# second to repair it.
#
# findings/<slug> IS NOT ITS REPLACEMENT. It is for a pull request that touches
# reviews/findings/ and nothing else: filing what a review produced, or curating
# what is already there. It repairs no code. A branch that files a finding AND
# repairs it is a fix-P<n>/ branch, because the repair is the half that binds a
# merge and it is the half the name has to declare.
#
# WHY fix-P*/ AND bulk-fix-P*/ ARE DIFFERENT PREFIXES. A fix-P*/ branch repairs
# exactly one finding and its name resolves to that finding's file, so the
# mapping is total and this validator checks it. A batch holds many findings and
# can name none of them. Sharing one prefix would leave the check guessing which
# shape it was looking at, so they are separated and each prefix's rule is
# exact. P0 and P1 never batch, which is why bulk-fix stops at P2.
#
# THE PER-FINDING BRANCH IS A LOCK. For P2 and P3 the fix is done on the
# per-finding branch and cherry-picked into the batch; the branch existing is
# what advertises that the finding is taken, so nothing else picks it up. Those
# branches are not expected to open a pull request, but they are validated here
# anyway because one may.
#
# THE FINDING IS LOOKED FOR ACROSS THE WHOLE PULL REQUEST, NOT AT ITS TWO ENDS.
# The claim a fix-P*/ name makes is "this pull request repairs that finding".
# The finding therefore has to have EXISTED somewhere in the pull request;
# requiring it at an endpoint is a different, stricter claim, and both endpoints
# refuse a pull request that is doing exactly the right thing:
#
#   The branch point alone fails the pull request that files its own finding.
#   With no fix/ prefix, a bug that was never filed has to become a finding
#   before it can have a branch name, and a branch-point-only rule means the
#   finding has to be on master before the branch exists: one pull request to
#   file it, a second to fix it.
#
#   The head alone fails the pull request that did its job. Repairing a finding
#   DELETES its file: master carries 69 such deletions. A fix-P*/ pull request
#   that has landed its repair has no finding file left in its own tree.
#
#   BOTH ENDPOINTS TOGETHER STILL FAIL THE ONE PULL REQUEST THE PREFIX EXISTS
#   FOR: one that files the finding in commit A and repairs it in commit B,
#   deleting the file as reviews/findings/README.md requires. The branch point
#   has no finding, the head has no finding, and the file existed only in
#   between. Keeping the file to get past the check is not the answer either:
#   that leaves finished work sitting in the outstanding queue, which the ledger
#   forbids.
#
# So the caller passes the MERGE-BASE tree, the head tree, and the pull
# request's own commits; this script judges the listings it is handed and goes
# looking for no finding of its own. Each listing is a file of finding
# filenames, one per line -- LF or CRLF, a carriage return being a line ending
# and never part of a name -- or a directory to list when running by hand. They
# are taken as one SET: a filename in more than one of them is one finding and
# not two, a name that resolves in any of them resolves, and a name that matches
# two distinct findings across them is still ambiguous and still refused.
#
# THE FIRST LISTING IS THE MERGE BASE AND NOT THE TARGET BRANCH'S CURRENT HEAD.
# Rooting it at the target's current head let master decide the verdict in the
# ordinary case: two findings sharing a description at the branch point, one
# repaired by the pull request and the same one independently deleted on master,
# and an ambiguous name that was refused at exit 1 before master moved CONFORMED
# at exit 0 after it -- same head, same diff. The merge base holds under that.
#
# THAT IS A DEFAULT AND NOT A GUARANTEE, AND THIS SCRIPT PROMISES NOTHING ABOUT
# AN UNCHANGED HEAD. The merge base itself moves when master absorbs a commit
# this branch also contains, and an ambiguous name can become unambiguous with
# no push to the branch. That is the RIGHT answer: whether a description picks
# out one finding or two is a property of the LEDGER, which other pull requests
# legitimately change, and this script answers "does this name resolve to
# exactly one filed finding" AGAINST THE LISTINGS IT IS HANDED AND AGAINST
# NOTHING ELSE.
#
# THOSE LISTINGS ARE NOT THE LEDGER AS IT STANDS, and calling them that
# overstates every verdict taken here. They are the pull request's three: the
# merge-base tree, the head tree, and its own commits. A finding the TARGET
# filed after the branch point is in none of them, on a synchronize and on the
# queue entry alike, because the tree of the commit being merged is never
# listed -- replayed on a queue merge whose target had added two
# same-description findings, the name conformed at exit 0 while the queue
# commit's own reviews/findings/ named 2 findings at exit 1. So the answer is
# the narrow one: the name resolved to exactly one filed finding in the
# listings handed over, at the moment they were built. It is not a statement
# about the head, it is not a statement about the ledger a merge lands on, and
# no way of choosing the boundary would make it either.
# .github/scripts/findings-in-range.sh states the whole argument.
#
# WHY ALL THREE AND NOT JUST THE PULL REQUEST'S COMMITS. A finding the pull
# request never touched appears in none of its commits, so the merge-base tree
# is needed. The head tree looks redundant next to merge-base + commits and very
# nearly is -- but it is what a listing built from per-commit DIFFS would miss
# for a finding that entered this branch through a merge rather than through a
# commit of its own. It costs one ls-tree, and the expensive failure of this
# rule is a FALSE REFUSAL of a legitimate branch.
#
# A FINDING IS A REGULAR FILE, AND A SYMLINK IS NOT ONE. A committed DIRECTORY
# named reviews/findings/P2_correctness_<timestamp>_<description>.md/ is a tree
# and not a finding, and it satisfied fix-P2/correctness_<description> with no
# finding file in existence. A committed SYMLINK wearing that name is a
# `120000 blob` and not a finding either. findings-in-range.sh's mode filter
# refuses both; the DIRECTORY-listing path here did not, because bash's -e and
# -f FOLLOW a symlink, so a link to any regular file resolved at exit 0 exactly
# what the workflow's path refused at exit 1.
#
# THE MODE GIT RECORDS DECIDES A TRACKED ENTRY, AND NOT WHAT THE CHECKOUT
# MATERIALISED. That is what makes the two APIs answer alike, and a filesystem
# test cannot: under core.symlinks=false -- git's own setting, and what git uses
# wherever the filesystem will not carry a link -- a committed symlink is
# checked out AS A REGULAR FILE holding the link target, `git status` stays
# empty and the recorded mode stays 120000. `-L` has nothing left to see, and
# the same commit conformed at exit 0 through the directory listing while the
# tree listings refused it at exit 1. So a directory listing takes the mode git
# records for an entry, which is the filter findings-in-range.sh applies to the
# very same entry. The filesystem decides only for an entry git does not track,
# where there is no recorded mode and a symlink is skipped by -L.
#
# AND THE NAMES COME FROM THE INDEX AS WELL AS FROM THE DIRECTORY, because a
# TRACKED finding need not be in the checkout at all. A SPARSE checkout leaves
# `git status` empty while the index still records every excluded path, so a
# candidate set taken from the directory alone lost one of two findings sharing
# a description and the ambiguous name conformed at exit 0 on the commit the
# tree listings refused at exit 1 -- the same two-API disagreement as the
# materialised symlink, one layer earlier, in WHICH NAMES ARE ASKED ABOUT
# rather than in what each name is. The candidates are therefore the union:
# every name the index records directly in the directory, and every name the
# directory holds. Taking both can only ADD names, and `sort -u` makes a name
# in both one finding rather than two.
#
# THE DIRECTORY'S HALF IS WHERE THE TWO APIS STILL PART, AND IT IS THE ONLY
# PLACE LEFT THAT IS KNOWN TO. A file the checkout holds and git does not TRACK
# is a candidate here and is in no tree listing: measured on one commit, an
# untracked twin beside a committed finding is exit 1 `names 2 findings`
# through the directory and exit 0 `conforms` through the trees. It is left
# that way deliberately. A listing directory with no repository over it has
# nothing but the filesystem to be read from, which is how this script is run
# against a scratch directory and how most of its fixtures run; and the
# disagreement cannot reach a merge, because the workflow always builds the
# three listings and never hands over a directory. The rule that would close it
# is "where there is an index, the index alone says which names exist", and the
# price of that rule is a by-hand run refusing the finding its author has
# written and not yet committed.
#
# A REPOSITORY GIT CANNOT READ IS A REFUSAL AND NOT A REPOSITORY THAT IS NOT
# THERE. Reading the recorded mode means asking git two questions, and the
# second -- "is this inside a work tree?" -- has three answers and not two: yes,
# no, and "I could not tell you". Collapsing the third into the second is how
# the filesystem fallback came back: `.git/config` unreadable, discovery exiting
# 128, the status thrown away, and the materialised symlink two paragraphs above
# conforming at exit 0 again with nothing said. Only "there is no repository
# here" falls back to the filesystem.
#
# WHICH OF THE THREE IT IS, IS DECIDED BY EXIT STATUS AND NEVER BY THE WORDS GIT
# USED. Git's diagnostics quote the paths it was working on, and a path is the
# caller's to choose. A repository at `.../not a git repository - fixture` whose
# `.git/config` is unreadable fails discovery with `unable to access '.../not a
# git repository - fixture/.git/config': Permission denied`, a substring test
# for git's own no-repository sentence matched inside that PATHNAME, and the
# materialised symlink conformed at exit 0 once more with empty stderr. So
# discovery failing is only permission to judge by the filesystem where there is
# no repository for it to have failed ABOUT -- where no index exists, and the
# filesystem is the whole of the evidence whatever git's reason was. That is
# asked as a second question, of `git rev-parse --resolve-git-dir`, which
# answers by exit status, reads no config, and so still says YES for the
# repository whose config it cannot read. A directory named `.git` holding
# nothing is not a repository to it, which is why the question is put to git
# rather than to `[[ -e ]]`: a stray `/tmp/.git` would otherwise turn every
# by-hand listing under /tmp red.
#
# A NAME THAT CANNOT BE HELD ON A LINE IS NOT A FINDING. A newline is legal in a
# filename and is the separator every listing here is built from, so the one
# separator that cannot occur in a name -- NUL -- is what git is asked for and
# what is read back; nothing converts one into the other. A name carrying a
# newline is then dropped, with a note, rather than split into two: it can match
# no P<n>_<category>_<timestamp>_<description>.md, so it is no finding and can
# make no other name ambiguous, and `git ls-tree` C-quotes it into a name that
# matches nothing in the tree listings either.
#
# With no listing at all only the grammar is checked, which is how the fixtures
# exercise it without a repository. With one listing, that listing alone is the
# set: a caller that has only the merge base gets the stricter rule and says so
# by passing only the merge base.
#
# A LISTING THIS SCRIPT CANNOT READ IS A REFUSAL AND NEVER AN EMPTY SET. An
# unreadable listing that read as "no findings here" would silently narrow the
# set, and narrowing it can turn a refusal into an acceptance: two findings match
# a description and the name is ambiguous, one listing goes unreadable, one match
# is left and the name "conforms". Read failures are propagated, separately from
# grep's ordinary no-match status.
#
# A LISTING THIS SCRIPT CANNOT READ WITHOUT LOSING SOMETHING IS THE SAME
# REFUSAL, AND IT IS MADE BEFORE THE READ. A file listing holding a NUL is not a
# listing: no filename can contain one, so it is a caller who wrote records
# where lines were asked for. `$(cat …)` DISCARDS a NUL rather than failing on
# it, concatenating the record after it onto the one before -- one twin stopped
# matching, and an ambiguous name conformed at exit 0 where the same two names
# LF-delimited refused it at exit 1, over a bash warning nothing acted on. The
# bytes are counted before the file is read, because after the read there is
# nothing left to see.
#
# GRANDFATHERING, AND WHY AN EXEMPTION IS A PULL REQUEST AND NOT A NAME.
# .github/legacy-branches.txt lists the pull requests that predate this rule,
# `<number> <head branch>`, and BOTH fields must match. A listed pull request is
# accepted with a warning; the file reaching zero entries is the signal the
# migration finished.
#
# Keying on the branch name alone exempted anybody who typed it. Nothing stops a
# fork creating `codex/findings-p3-1a57a2730a12` today and opening a new pull
# request, and a lookup that is handed only that name cannot tell it from the
# pull request the entry was written for. A migration list whose CONTENTS no
# longer decide who is exempt is not a migration list, and the population would
# no longer be the 24 pull requests it claims to describe. The number bounds it.
#
# PR_NUMBER carries that identity. With no PR_NUMBER there is nothing to match
# and no exemption is granted -- deliberately: a caller checking a name by hand
# is not judging a pull request, and the safe answer to an unidentified caller
# is the rule itself.

#
# EVERY FALSE ACCEPTANCE THIS FILE HAS GIVEN WAS A FAILED PROBE ANSWERED AS AN
# ABSENCE, AND THAT IS WHY THERE ARE NOW EXACTLY TWO WAYS OUT OF IT. Four review
# rounds each found more of one defect than the round before -- a git command or
# a file read whose status was discarded, and whose failure was then read as
# "nothing there", "empty" or "end of input", every one of which CONFORMS. The
# shapes were all different and the class was one:
#
#   `nul=$?` inside `{ ...; nul=$?; }` makes the GROUP succeed, so the `||` that
#   was meant to catch a failed open never ran and every non-zero read status
#   became end-of-file: `/proc/self/mem` as a listing was exit 0 `conforms`.
#
#   A process substitution's status is not the command's, so `done < <(git
#   ls-files …)` followed by `return 0` read an unreadable `.git/index` as "git
#   records nothing at this path". The filesystem then decided, a committed
#   symlink materialised as a regular file was read as a FILE LISTING, and the
#   link's target text was resolved as a finding NOBODY HAS FILED.
#
#   And testing that `.git/.` is accessible says nothing about the metadata
#   inside it, so `chmod 000 .git/HEAD` -- or `.git/objects`, or `.git/refs` --
#   made both discovery probes exit 128, "no repository" was inferred, and the
#   filesystem fallback conformed with empty stderr.
#
# Armouring each call site produced another crop each round, so the call sites
# are gone. `git_probe` is the only place this file runs git and `read_file` is
# the only place it opens a file, both are a dozen lines long, and
# .github/scripts/test-pr-policy.sh FAILS THE BUILD if a bare `git` or a bare
# `<` redirection appears anywhere outside them. A gate can only test the
# instances somebody imagined; a shape rule refuses the ones nobody did.
#
# `git_probe`'s contract is the part that matters: the caller ENUMERATES the exit
# statuses it is prepared to read as answers, and any other status refuses the
# whole run. "There is no repository here" is an answer at a call site that says
# so; 128 from metadata git could not read never is, and the two are the same
# status. Distinguishing them is the caller's job and this makes the caller do
# it, because a helper that returned "no records" for both is the defect above.
#
# `read_file`'s contract is the other half: the caller's path is OPENED ONCE, by
# this shell, and copied to a private file whose read status is taken from the
# copying command rather than from a builtin that reports end-of-input and a
# read error with the same 1. Nothing parses the caller's path a second time --
# a path is not a value and can hold different bytes at every open -- and the
# copy is in a directory this script made, so no rename can land between the
# check and the parse.
#
# A PATH IS NORMALISED BEFORE IT IS JUDGED, AND EVERY RECORDED COMPONENT OF IT
# COUNTS, NOT JUST THE LAST. `reviews/findings`, `reviews/findings/`,
# `reviews/findings/.`, `reviews//findings` and `reviews/./findings` are one
# listing, and they answered differently: appending `/.` moved the question from
# `findings` to `.`, and a committed symlink at `reviews/findings` that the plain
# spelling refused at exit 1 conformed at exit 0 with three characters added. And
# asking git from the listing's PARENT already follows a link one level up: with
# `reviews -> elsewhere` COMMITTED and the finding under `elsewhere/`, `git -C
# reviews` answered for `elsewhere`, and the directory listing resolved at exit 0
# a name the tree listings refuse at exit 1. So the spelling is reduced to
# components first, and EVERY component is looked at -- with `-L`, which sees a
# link rather than following it -- before git is asked from any of them. A
# component inside a work tree that is a link makes the path one no tree entry
# and no index entry can name, and a path like that is not the ledger's findings
# directory. A link ABOVE the work tree is not one of these and is walked past:
# `/tmp` is a symlink on macOS and every by-hand listing there goes through it,
# and nothing above a work tree is recorded anywhere to disagree.
set -euo pipefail
export PATH="/usr/bin:/bin:$PATH"

# The entries of a DIRECTORY listing are read with a GLOB, because a newline is
# legal in a filename and `ls` writes one name per line. These two settings are
# what keep that glob honest, and it is the only one in this file: a pattern
# that matches nothing has to be an empty list rather than the pattern itself --
# `dir/*` standing for itself is a name that is not there -- and a GLOBIGNORE
# that matches drops names from the expansion in silence, which is the narrowing
# every rule below refuses.
#
# Measured on bash 5.2.21 rather than assumed, because the obvious sentence is
# wrong: a GLOBIGNORE INHERITED from the environment is INERT, and stays inert
# until something assigns it -- `GLOBIGNORE="$GLOBIGNORE"` is enough to wake it,
# and then `dir/*` loses every name the pattern matches. The value is inherited
# whether or not it is honoured, so it is unset here rather than left lying
# where an assignment could wake it. `unset` leaves the glob complete; that was
# executed too.
shopt -s nullglob
unset GLOBIGNORE

# `dirname` and not "${BASH_SOURCE[0]%/*}": that expansion strips nothing when
# the script is invoked by bare name from inside its own directory, which is the
# bug .github/scripts/test-pr-policy.sh carries and CLAUDE.md warns about.
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
legacy_file="${LEGACY_BRANCHES:-$script_dir/../legacy-branches.txt}"

branch="${1:-}"
merge_base_findings="${2:-}"
head_findings="${3:-}"
range_findings="${4:-}"

slug_re='[a-z0-9]+(-[a-z0-9]+)*'
# Keep in step with .github/scripts/validate-pr-body.sh's category case.
category_re='(correctness|crash-consistency|security-trust|portability|liveness|performance|compatibility|docs-contract)'

vocabulary() {
  cat >&2 <<'EOF'

The head branch must be one of:

  feature/<slug>                  new behaviour
  refactor/<slug>                 behaviour-preserving change
  docs/<slug>                     documentation only
  standards/<slug>                a standards/ section, or a sweep under one
  ci/<slug>                       gates, workflows, scripts/ and .github/
  gate/<slug>                     a cumulative review-gate report
  findings/<slug>                 reviews/findings/ alone: filing or curating
  fix-P<n>/<category>_<desc>      exactly one finding, n in 0..3
  bulk-fix-P<n>/<slug>            a batch of findings, n in 2..3

<slug> and <desc> are lower-case words joined by single hyphens, e.g.
`feature/pr9-repair-execution`. <category> is one of correctness,
crash-consistency, security-trust, portability, liveness, performance,
compatibility, docs-contract, and a fix-P*/ branch names one finding:

  reviews/findings/P1_correctness_202609040301_pid-identity-under-a-host-wildcard-waiter.md
  fix-P1/correctness_pid-identity-under-a-host-wildcard-waiter

That finding is looked for anywhere in this pull request, not just at its two
ends: the merge-base tree, the head tree, and every commit between them. A
repair that deleted the file is resolved by the merge base; a pull request that
files the finding and repairs it in one go is resolved by its own commits,
whichever one the file lived in. Only a regular file is a finding: a directory
or a symlink carrying a finding's name is not one.

There is no fix/<slug> prefix. A bug worth a branch is worth a finding, so file
the finding and branch fix-P<n>/ after it. findings/<slug> is for a pull request
that touches reviews/findings/ and nothing else; it is not somewhere to put a
repair.

There is deliberately no prefix for a `test`, `chore`, `perf`, `security` or
`build` change even though those are valid title types. If you need one, that is
a gap in the vocabulary to raise rather than a name to work around: say so and
the rule changes in MAINTAINING.md.
EOF
}

fail() {
  echo "branch-name-policy: $*" >&2
  vocabulary
  exit 1
}

[[ -n "$branch" ]] || fail 'no branch name was given'

# ==== AUDITED HELPERS BEGIN ===================================================
#
# The only two places in this file that run git or open a file for reading.
# .github/scripts/test-pr-policy.sh asserts that, over the rest of the file, by
# shape and not by case: a bare `git`, a `<` redirection from a path, a `<(...)`
# or a `cat` with an input anywhere below AUDITED HELPERS END fails the gate.
# Keep this region small enough to read in one sitting; that is the whole of its
# value.

# Where a probe's output goes. It is this script's own directory, mode 700 from
# mktemp, so a copy taken here cannot be replaced by anybody between the check
# and the parse -- which is the defect a caller's path carries and a private file
# does not.
probe_dir=''
remove_probe_dir() {
  [[ -z "$probe_dir" ]] || rm -rf -- "$probe_dir"
}
trap remove_probe_dir EXIT
probe_dir="$(mktemp -d "${TMPDIR:-/tmp}/branch-name-policy.XXXXXX")" || {
  echo "branch-name-policy: no writable temporary directory, so git's output and" >&2
  echo "  a listing's bytes cannot be captured with their statuses. Refusing" >&2
  echo "  rather than deciding '$branch' from probes whose failures cannot be" >&2
  echo "  separated from their answers." >&2
  exit 1
}

# git_probe's answers. `probe_status` is git's exit status and one the caller
# enumerated; `probe_records` is its stdout split on NUL, which is the one byte
# no pathname holds; `probe_text` is the first record with its line ending
# removed, for the probes that answer in one word; `probe_stderr` is what git
# said, kept so a refusal can QUOTE it without any decision being taken from it.
probe_status=0
probe_text=''
probe_stderr=''
probe_records=()

# git_probe <expected-statuses> -- <git arguments>...
#
# <expected-statuses> is a comma-separated list of the exit statuses THIS CALLER
# READS AS ANSWERS. Any other status refuses the whole run: a status nobody
# enumerated is not an absence, an empty set or an end of input, and reading it
# as one of those is every wrong acceptance this gate has given.
#
# Stdout goes to a FILE and the status is taken from git itself. Neither
# `$(...)` -- which discards the NUL that delimits `-z` records, so two names
# arrive as one -- nor `<(...)`, whose status is not the command's and is what
# made an unreadable index read as an empty index.
git_probe() {
  local expected="$1" record
  shift
  [[ "${1:-}" == -- ]] || {
    echo "branch-name-policy: internal error: git_probe was called without --" >&2
    exit 1
  }
  shift
  probe_status=0
  probe_text=''
  probe_stderr=''
  probe_records=()
  command -v git >/dev/null 2>&1 || {
    echo "branch-name-policy: git is not on PATH, so what it records for a listing" >&2
    echo "  cannot be read at all. That is refused rather than judged by the" >&2
    echo "  filesystem, which cannot see a recorded mode. '$branch' was not judged." >&2
    exit 1
  }
  git "$@" > "$probe_dir/git.out" 2> "$probe_dir/git.err" || probe_status=$?
  # Both files are this script's own, written by the command that just finished,
  # so reading them cannot be a caller's path changing under the read.
  IFS= read -r -d '' probe_stderr < "$probe_dir/git.err" || true
  probe_stderr="${probe_stderr%$'\n'}"
  record=''
  while IFS= read -r -d '' record || [[ -n "$record" ]]; do
    probe_records[${#probe_records[@]}]="$record"
    record=''
  done < "$probe_dir/git.out"
  if (( ${#probe_records[@]} > 0 )); then
    probe_text="${probe_records[0]}"
    probe_text="${probe_text%$'\n'}"
  fi
  case ",$expected," in
    *",$probe_status,"*) return 0 ;;
  esac
  echo "branch-name-policy: git exited $probe_status, which this call does not read" >&2
  echo "  as an answer. It reads $expected and nothing else." >&2
  echo "    git $*" >&2
  echo "  A status nobody enumerated is refused rather than read as 'nothing" >&2
  echo "  recorded': that reading is what let an unreadable index, an unreadable" >&2
  echo "  config and an unreadable HEAD each conform. '$branch' was not judged." >&2
  if [[ -n "$probe_stderr" ]]; then
    printf '%s\n' "$probe_stderr" | sed 's/^/  /' >&2
  fi
  exit 1
}

# read_file's answers, valid until the next call: `file_bytes` is the whole file
# with its final line ending kept, and `file_has_nul` says a NUL was found --
# which is a refusal at every call site, because no filename holds one and no
# line ending is one.
file_bytes=''
file_has_nul=0
file_error=''

# read_file <path>: 0 the whole file was read, 1 it could not be read to the end,
# 2 it could not be opened.
#
# ONE OPEN OF THE CALLER'S PATH, BY THIS SHELL, and everything parsed afterwards
# is the private copy. A path is not a value: it can hold different bytes at
# every open, and a check on bytes that are then re-read is a check on bytes
# nobody parsed -- measured on a listing replaced by rename between the two.
#
# AND END-OF-INPUT IS SEPARATED FROM A READ ERROR, which bash's `read` reports
# with the same status 1. So the copy is made by a command whose exit status says
# which happened -- `cat` from the descriptor this shell already opened, opening
# nothing itself -- and the refusal comes from that status. `/proc/self/mem` is
# the case: openable, readable to `-r`, and every read of it fails. Read through
# `read`'s status alone it was an empty listing, which NARROWS the candidate set,
# and a narrowed set turns an ambiguous name into an accepted one.
read_file() {
  local path="$1" copy_status=0 nul=1
  file_bytes=''
  file_has_nul=0
  file_error=''
  { exec 9< "$path"; } 2>/dev/null || return 2
  cat <&9 > "$probe_dir/slurp" 2> "$probe_dir/slurp.err" || copy_status=$?
  exec 9<&-
  if (( copy_status != 0 )); then
    IFS= read -r -d '' file_error < "$probe_dir/slurp.err" || true
    file_error="${file_error%$'\n'}"
    return 1
  fi
  # `|| nul=$?` and never `{ read; nul=$?; }`: the group's status is the
  # ASSIGNMENT's, which always succeeds, so the `||` that was meant to catch a
  # failed read never ran. Here `nul` is 0 only when the delimiter was FOUND.
  nul=0
  IFS= read -r -d '' file_bytes < "$probe_dir/slurp" || nul=$?
  if (( nul == 0 )); then
    file_has_nul=1
  fi
  return 0
}
# ==== AUDITED HELPERS END =====================================================

# normalise_listing_path <path>: the same path written one way, in
# `normalised_listing`. Empty components, `.` components and a trailing separator
# are removed, because `reviews/findings`, `reviews/findings/`,
# `reviews/findings/.`, `reviews//findings` and `reviews/./findings` are one
# listing and gave two answers: the last component decides what the path IS, and
# with `/.` appended the last component was `.`, so a committed symlink at
# `reviews/findings` conformed at exit 0 where the plain spelling refused it at
# exit 1.
#
# A `..` is NOT collapsed and is refused where it could matter. `a/b/..` is `a`
# only when `b` is a directory; when `b` is a symlink it is the link's parent, so
# reducing it lexically would answer about a path the caller did not name and
# resolving it on the filesystem would follow the link this rule exists to
# refuse. A LEADING run of `..` is kept and allowed: it names the starting
# directory, which is above every component this judges.
normalised_listing=''
normalise_listing_path() {
  local path="$1" prefix='' parts='' component named=0
  normalised_listing=''
  [[ -n "$path" ]] || return 0
  if [[ "$path" == /* ]]; then
    prefix='/'
  fi
  while [[ -n "$path" ]]; do
    component="${path%%/*}"
    if [[ "$component" == "$path" ]]; then
      path=''
    else
      path="${path#*/}"
    fi
    case "$component" in
      ''|'.')
        continue
        ;;
      '..')
        if (( named )); then
          fail "findings listing '$1' holds a '..' after a named component.
  What that path names depends on whether the component before it is a
  directory or a symlink, so it is refused rather than guessed at: give the
  listing without '..', or as a path that begins with it."
        fi
        ;;
      *)
        named=1
        ;;
    esac
    if [[ -n "$parts" ]]; then
      parts="$parts/$component"
    else
      parts="$component"
    fi
  done
  if [[ -z "$parts" ]]; then
    if [[ -n "$prefix" ]]; then
      normalised_listing='/'
    else
      normalised_listing='.'
    fi
    return 0
  fi
  normalised_listing="$prefix$parts"
}

normalise_listing_path "$merge_base_findings"
merge_base_findings="$normalised_listing"
normalise_listing_path "$head_findings"
head_findings="$normalised_listing"
normalise_listing_path "$range_findings"
range_findings="$normalised_listing"

# legacy_exempt: is THIS pull request one the migration list names? Both the
# number and the branch must match the same line. The fields are compared with
# `==` and never handed to a pattern matcher, so a name beginning with a dash is
# a name and not a set of options.
#
# The list goes through read_file like every other file: a migration list that
# cannot be read is a refusal and not an empty list. Failing closed here refuses
# a pull request that IS exempt rather than exempting one that is not, so the
# wrong answer is cheap -- but it is still the wrong answer, and "the file was
# there and unreadable" is not "the file lists nobody".
legacy_exempt() {
  local pr="${PR_NUMBER:-}" listed_pr listed_branch read_status=0
  [[ -f "$legacy_file" ]] || return 1
  [[ "$pr" =~ ^[0-9]+$ ]] || return 1
  read_file "$legacy_file" || read_status=$?
  if (( read_status != 0 )); then
    fail "the migration list '$legacy_file' could not be read, so whether pull
  request #$pr is exempt from the branch vocabulary is not known. A gate that
  cannot see its input refuses rather than deciding it saw nothing."
  fi
  if (( file_has_nul )); then
    fail "the migration list '$legacy_file' holds a NUL byte, so its lines cannot
  be read as '<pull-request number> <head branch>' records."
  fi
  while read -r listed_pr listed_branch _; do
    listed_branch="${listed_branch%$'\r'}"
    if [[ -z "$listed_pr" || "$listed_pr" == \#* ]]; then
      continue
    fi
    if [[ "$listed_pr" == "$pr" && "$listed_branch" == "$branch" ]]; then
      return 0
    fi
  done <<< "$file_bytes"
  return 1
}

# A listed pull request is accepted, loudly, so the exemption is visible in the
# check's log rather than silent.
if legacy_exempt; then
  echo "branch-name-policy: pull request #${PR_NUMBER} predates the branch" >&2
  echo "  vocabulary and is listed in ${legacy_file##*/} as '$branch'." >&2
  echo "  DO NOT RENAME THE HEAD BRANCH. GitHub CLOSES a pull request when its" >&2
  echo "  head branch is renamed, and it cannot be reopened until the old name" >&2
  echo "  is restored -- at which point the branch is back under the name this" >&2
  echo "  rule refuses. An entry leaves the list when its pull request merges," >&2
  echo "  or by way of a replacement pull request opened on a conforming name" >&2
  echo "  carrying the same head commit, quoting this number and its review" >&2
  echo "  evidence. MAINTAINING.md states this." >&2
  exit 0
fi

# A listing is checked here as well as where it is read: this catches a caller's
# mistake even for a branch that needs no resolution at all, and it reports the
# listing rather than leaving the reader to infer it from a missing finding.
check_listing() {
  if [[ -z "$1" ]]; then
    return 0
  fi
  if [[ ! -e "$1" ]]; then
    fail "findings listing '$1' is neither a file nor a directory"
  fi
  if [[ ! -r "$1" ]]; then
    fail "findings listing '$1' cannot be read"
  fi
  # A directory also has to be SEARCHABLE, or every entry in it fails the
  # regular-file test below and the listing narrows to nothing in silence.
  if [[ -d "$1" && ! -x "$1" ]]; then
    fail "findings listing '$1' is a directory that cannot be searched"
  fi
}
check_listing "$merge_base_findings"
check_listing "$head_findings"
check_listing "$range_findings"

# The `.git` the walk below could not look at, named so the refusal can quote
# it. Set only on the way out at status 2, and read only there.
unexaminable_git=''

# gitdir_shaped <directory>: does it hold any of the three things a repository
# keeps and an empty directory named `.git` does not? This is asked only where
# git has ALREADY REFUSED to resolve the path, and it is what separates "there
# was nothing here to read" from "there is a repository here and its metadata
# cannot be read". `chmod 000 .git/HEAD` -- or `.git/objects`, or `.git/refs` --
# fails both discovery probes with the same 128 git uses for a directory that is
# not a repository at all, and the old test, that `.git/.` was accessible, was
# true throughout: the walk concluded "no repository", the filesystem fallback
# decided the entry, and a commit refused at exit 1 with readable metadata
# conformed at exit 0 without it. An entry is looked for with `-e` OR `-L`, so a
# file this cannot read and a link with nothing at the end of it both count as
# there.
gitdir_shaped() {
  local dir="$1" entry
  for entry in HEAD objects refs; do
    if [[ -e "$dir/$entry" || -L "$dir/$entry" ]]; then
      return 0
    fi
  done
  return 1
}

# gitdir_pointer <bytes>: the path out of a `.git` FILE's `gitdir: <path>` line,
# or nothing when the file is not one. A carriage return is a line ending here
# for the same reason it is one everywhere else in this file.
gitdir_pointer() {
  local first="${1%%$'\n'*}"
  first="${first%$'\r'}"
  case "$first" in
    'gitdir: '*) printf '%s\n' "${first#gitdir: }" ;;
    *) ;;
  esac
}

# repository_above <directory>: is there a repository for git to have failed
# ABOUT? A `.git` at that directory or at any ancestor, and `git rev-parse
# --resolve-git-dir` -- which answers by EXIT STATUS, prints what this never
# reads, and needs no config, so it still says yes for the repository whose
# config git could not read -- deciding whether each one is a repository at all.
# Asking git rather than `[[ -e ]]` is what keeps an empty directory named
# `.git` from being one: a stray /tmp/.git would otherwise refuse every by-hand
# listing under /tmp, and that is a false red on a legitimate branch.
#
# THE ANSWER IS ONE OF THREE AND NEVER ONE OF TWO: 0 a repository, 1 none, and
# 2 "there is a repository here and this cannot examine it". Treating every
# unsuccessful resolution as an absence is the discarded read failure one level
# up, one level down, and it has now been measured three ways: an unreadable
# `.git` FILE on a linked worktree, an unreadable `.git` DIRECTORY, and an
# unreadable `HEAD`, `objects` or `refs` INSIDE a `.git` directory this can enter
# perfectly well. Metadata that is MISSING may fall back; metadata that CANNOT BE
# EXAMINED must refuse.
#
# WHICH OF THOSE THREE IT IS, IS NOT READ OUT OF GIT'S MESSAGE. Every failure
# here is exit 128 whatever the reason -- `not a gitdir` for a nonexistent
# `.git`, for an empty directory named `.git`, for an unreadable `.git` DIRECTORY
# and for a `.git` whose HEAD cannot be read alike -- and git's words quote the
# caller's own pathname, which is how an earlier repair was defeated. So the
# third answer comes from THIS asking the filesystem the two questions it can
# answer for itself: can the thing named `.git` be looked into or read at all,
# and if it can, does it hold what a repository holds?
#
# A GIT_DIR or GIT_WORK_TREE in the environment points git at an index this walk
# cannot reach, so it counts as a repository in play and the answer is yes.
repository_above() {
  local dir="$1" gitdir pointer target
  [[ -z "${GIT_DIR:-}" && -z "${GIT_WORK_TREE:-}" ]] || return 0
  # The physical path, which is the one git's own discovery walks. A directory
  # this cannot even enter is not one to answer "no repository" about.
  dir="$(CDPATH= cd -P -- "$dir" 2>/dev/null && pwd)" || return 0
  while :; do
    git_probe '0,128' -- rev-parse --resolve-git-dir "$dir/.git"
    if (( probe_status == 0 )); then
      return 0
    fi
    # Git said no. This level is an ABSENCE only where this can see that there
    # was nothing to read; a `.git` it cannot examine, and a `.git` that holds a
    # repository's own files, each leave the question open, and an open question
    # is not an absence. The tests are attempts and not permission bits: `-e
    # <dir>/.` needs search on the directory, and the open needs read on the
    # file, which is what git needed and did not get.
    gitdir="$dir/.git"
    if [[ -d "$gitdir" ]]; then
      if [[ ! -e "$gitdir/." ]]; then
        unexaminable_git="$gitdir"
        return 2
      fi
      if gitdir_shaped "$gitdir"; then
        unexaminable_git="$gitdir"
        return 2
      fi
    elif [[ -f "$gitdir" ]]; then
      if ! read_file "$gitdir"; then
        unexaminable_git="$gitdir"
        return 2
      fi
      # A readable `.git` FILE that git would not resolve either names no gitdir
      # at all -- garbage, and nothing to examine -- or names one this must look
      # at before calling the level empty. Same rule, one indirection along.
      pointer="$(gitdir_pointer "$file_bytes")"
      if [[ -n "$pointer" ]]; then
        case "$pointer" in
          /*) target="$pointer" ;;
          *) target="$dir/$pointer" ;;
        esac
        if [[ -d "$target" ]]; then
          if [[ ! -e "$target/." ]] || gitdir_shaped "$target"; then
            unexaminable_git="$target"
            return 2
          fi
        fi
      fi
    fi
    if [[ "$dir" == / ]]; then
      return 1
    fi
    dir="${dir%/*}"
    [[ -n "$dir" ]] || dir=/
  done
}

# repository_state <directory>: `work-tree` when git says that directory is
# inside one, `none` when there is no repository over it, and a REFUSAL at
# status 1 when git could not say which -- the three answers, never two.
#
# THE ANSWER IS GIT'S EXIT STATUS AND NEVER THE TEXT OF A MESSAGE: git's
# diagnostics quote the caller's own pathname, and a repository whose path holds
# the string `not a git repository` read as "there is no repository here" and
# fell straight through to the filesystem at exit 0. Git failing is permission to
# judge by the filesystem in exactly one case: there is no repository here, so
# there is no index it could have read and no recorded mode to disregard.
# Anything else -- a config it cannot read, a repository it will not touch, a
# `.git` this cannot examine either -- is a refusal, whatever it said.
repo_state=none
repository_state() {
  local dir="$1" said above=0
  repo_state=none
  git_probe '0,128' -- -C "$dir" rev-parse --is-inside-work-tree
  if (( probe_status != 0 )); then
    # git_probe is about to be called again and its answers are one set, so
    # git's words are kept here or lost.
    said="$probe_stderr"
    repository_above "$dir" || above=$?
    if (( above == 1 )); then
      return 0
    fi
    echo "branch-name-policy: git could not say what it records for '$dir':" >&2
    if [[ -n "$said" ]]; then
      printf '%s\n' "$said" | sed 's/^/  /' >&2
    fi
    if (( above == 2 )); then
      echo "  '$unexaminable_git' is there and cannot be examined, so whether this" >&2
      echo "  listing is inside a repository is not known either. Metadata that" >&2
      echo "  cannot be read is refused rather than read as metadata that is not" >&2
      echo "  there, because only the second may be judged by the filesystem --" >&2
      echo "  which cannot see a recorded mode at all." >&2
    else
      echo "  A listing inside a repository this cannot read is refused rather than" >&2
      echo "  judged by the filesystem, which cannot see a recorded mode at all." >&2
    fi
    return 1
  fi
  # Stdout alone, so the answer is `true` or `false` and nothing else: a warning
  # about some other file git could not read is on stderr and is not an answer.
  [[ "$probe_text" == true ]] || return 0
  repo_state=work-tree
}

# index_entries_of <directory>: `<mode> <object> <stage><TAB><name>` for every
# entry git RECORDS in that directory, one per line in `index_entry_lines`, and
# NOTHING AT ALL when the directory is not inside a work tree -- which is every
# by-hand listing that is not a checkout, and where the filesystem is all there
# is to go on. A failure is a refusal at status 1: an index this cannot read is
# the same refusal as a listing it cannot read, and for the same reason.
#
# `ls-files` is enumerated as answering 0 AND NOTHING ELSE, because by here git
# has already said this directory is inside a work tree: a 128 after that is an
# index it could not read, never an absence, and the two were one answer while
# the status was discarded.
#
# `-z` so a name is never quoted or escaped, and the records are READ as
# NUL-delimited records rather than CONVERTED to lines. A newline is legal in a
# filename and NUL is the one byte that is not, so turning the separator into a
# newline is what let `noise<LF>P2_<category>_<ts>_<desc>.md` arrive as two
# records: the second carried no mode and no tab, the real finding of that name
# landed among the NON-regular entries, and the name it should have made
# ambiguous conformed at exit 0 while the tree listings refused it at exit 1.
#
# A NAME HOLDING A NEWLINE IS DROPPED HERE rather than split, and nothing is
# lost by it: no such name can match P<n>_<category>_<timestamp>_<description>.md
# whatever the set is built from, so it is not a finding and cannot make another
# name ambiguous. The entry loop in read_listing reports it once for the listing
# as a whole. `git ls-tree` C-QUOTES the same name in the tree listings, where
# the quoted form matches no finding either, so the two APIs drop it alike.
#
# An entry below a subdirectory comes out as `sub/name` and matches no name in
# the listing, which is the right answer twice over: the subdirectory is not a
# finding whatever it holds, and what it holds is not in this listing.
index_entry_lines=''
index_entries_of() {
  local dir="$1" record
  index_entry_lines=''
  repository_state "$dir" || return 1
  [[ "$repo_state" == work-tree ]] || return 0
  git_probe '0' -- -C "$dir" ls-files -sz -- .
  if (( ${#probe_records[@]} > 0 )); then
    for record in "${probe_records[@]}"; do
      case "${record#*$'\t'}" in
        *$'\n'*) continue ;;
      esac
      index_entry_lines="$index_entry_lines$record"$'\n'
    done
  fi
}

# path_through_symlink <normalised path>: did this path reach its destination
# through a symlink INSIDE a work tree? 0 yes, with the component in
# `symlink_ancestor`; 1 no; 2 the question could not be answered and the run is
# refused.
#
# THIS IS THE ANCESTOR HALF OF THE SAME RULE, AND IT IS WHY THE LAST COMPONENT IS
# NOT ENOUGH. Asking the index from the listing's PARENT is already a traversal:
# `git -C reviews` chdirs, and where `reviews` is a COMMITTED SYMLINK to
# `elsewhere` it answers for `elsewhere` -- so a finding under `elsewhere/`
# resolved `reviews/findings` at exit 0 while the tree listings, where no index
# entry's name traverses a link, refused the same commit at exit 1. The listing
# path's own recorded type was innocent; the path above it was not.
#
# EVERY component is lstatted, top down, and only a `-L` costs anything: in the
# ordinary case this is a handful of stats and no probe at all. Where one IS a
# link, the question asked of git is about its PARENT, which the walk has already
# established holds no link, so that probe cannot be following anything either.
#
# A LINK ABOVE THE WORK TREE IS NOT ONE OF THESE, and the walk goes past it. On
# macOS `/tmp` and `/var` are both symlinks and `mktemp -d` hands back a path
# through them, so a blanket "no symlink anywhere above" would answer the empty
# set for every by-hand listing on that platform -- a false refusal of a
# legitimate branch, which is the expensive failure of this rule. Nothing above a
# work tree is recorded anywhere, so nothing above one can disagree with a tree
# listing. Inside one, a component that is a link makes the path unnameable by
# any tree entry whether or not git tracks the link itself, and the tree listings
# hold no finding for it either way: the two APIs agree at the empty set.
symlink_ancestor=''
path_through_symlink() {
  local path="$1" prefix='' rest component parent
  symlink_ancestor=''
  if [[ "$path" == /* ]]; then
    prefix='/'
    rest="${path#/}"
  else
    rest="$path"
  fi
  while [[ "$rest" == */* ]]; do
    component="${rest%%/*}"
    rest="${rest#*/}"
    if [[ -z "$prefix" ]]; then
      prefix="$component"
    elif [[ "$prefix" == / ]]; then
      prefix="/$component"
    else
      prefix="$prefix/$component"
    fi
    [[ -L "$prefix" ]] || continue
    parent="${prefix%/*}"
    if [[ "$parent" == "$prefix" ]]; then
      parent='.'
    elif [[ -z "$parent" ]]; then
      parent=/
    fi
    repository_state "$parent" || return 2
    if [[ "$repo_state" == work-tree ]]; then
      symlink_ancestor="$prefix"
      return 0
    fi
  done
  return 1
}

# recorded_path_mode <path>: what git RECORDS AT THAT PATH ITSELF, in
# `recorded_mode` -- a mode for a blob, `tree` for a path it records things
# under, and nothing at all for a path it records nothing about, which is an
# untracked one or no repository. `reached_through_symlink` is 1 when the path
# reached its destination through a link inside the work tree, in which case git
# records nothing at the path that was NAMED and no mode is asked for.
#
# The index is asked from the path's PARENT with the last component as the
# pathspec, because asking from INSIDE the path is already following it: `git -C
# <symlink> ls-files` answers for wherever the link points. Whether the parent
# ITSELF holds a link is path_through_symlink's question, and it is asked FIRST,
# because a record read through a link is a record about somebody else's path.
#
# One `ls-files` answers both remaining questions: a record whose name is exactly
# the last component is a blob AT the path, and a record named `<base>/...` is
# something recorded UNDER it, so the path is a directory in the ledger. The
# exact name sorts before anything under it, so a blob is seen first.
#
# `:(literal)` because a pathname is the CALLER'S to choose and a pathspec is
# not a pathname: a listing at `:weird` is read by git as pathspec magic, and
# plain `-- ':weird'` matched nothing at all where the literal form matches the
# path -- a listing silently read as recording nothing, which is the narrowing
# every rule here refuses.
recorded_mode=''
reached_through_symlink=0
recorded_path_mode() {
  local path="$1" parent base record name through=0
  recorded_mode=''
  reached_through_symlink=0
  base="${path##*/}"
  [[ -n "$base" ]] || return 0
  parent="${path%/*}"
  [[ "$parent" != "$path" ]] || parent='.'
  [[ -n "$parent" ]] || parent=/
  path_through_symlink "$path" || through=$?
  if (( through == 2 )); then
    return 1
  fi
  if (( through == 0 )); then
    reached_through_symlink=1
    return 0
  fi
  repository_state "$parent" || return 1
  [[ "$repo_state" == work-tree ]] || return 0
  git_probe '0' -- -C "$parent" ls-files -sz -- ":(literal)$base"
  if (( ${#probe_records[@]} > 0 )); then
    for record in "${probe_records[@]}"; do
      name="${record#*$'\t'}"
      if [[ "$name" == "$base" ]]; then
        recorded_mode="${record%% *}"
        return 0
      fi
      if [[ "$name" == "$base"/* ]]; then
        recorded_mode=tree
        return 0
      fi
    done
  fi
}

# read_listing <listing>: the finding filenames in one listing, one per line,
# from whichever form the caller passed. A read that FAILS returns non-zero and
# never an empty set; the existence test above cannot stand in for this, because
# a listing can be readable when it is checked and unreadable when it is read,
# and a directory can be readable and still not listable.
#
# A DIRECTORY IS LISTED DOWN TO ITS REGULAR FILES, AND A SYMLINK IS NOT ONE.
# A directory listing names a subdirectory, and a symlink, exactly as it names a
# file, so reviews/findings/P2_correctness_<ts>_<desc>.md/ -- a directory,
# holding no finding -- and a symlink of the same name each resolved
# fix-P2/correctness_<desc>. WHICH NAMES ARE IN THE LISTING is one question and
# WHAT EACH NAME IS is another; the second is decided below, and the first is
# asked twice on purpose. `ls` is run for its STATUS, because an unlistable
# directory must fail rather than read as empty and a glob that matched nothing
# cannot say which of those happened. The NAMES come from the glob, because `ls`
# writes one name per line and A NEWLINE IS LEGAL IN A FILENAME: from `ls`,
# `noise<LF>P2_<category>_<ts>_<desc>.md` arrives as two names, and the second is
# either a name that is not in the directory at all -- a refusal where the tree
# listings conform -- or one that is, which is a name that is no finding
# answering for one. Both skip a name beginning with a dot, and both are right
# to: a finding's name begins with `P`.
#
# AND THE LISTING'S OWN RECORDED TYPE IS READ BEFORE THE PATH IS FOLLOWED, AND SO
# IS EVERY RECORDED COMPONENT ABOVE IT. The entries were already decided by the
# mode git records; the DIRECTORY ITSELF was not, and it is a tracked entry like
# any other. With `reviews/findings` a committed symlink to a sibling directory --
# a `120000 blob`, a clean checkout, `git status` empty -- the tree listings hold
# no finding under reviews/findings/ and refuse at exit 1, while handing that path
# straight in FOLLOWED the link and resolved a name out of files no ledger holds,
# at exit 0. With `reviews` the committed symlink instead, the same thing happened
# one level up and the listing path's own recorded type was innocent: git was
# ASKED FROM inside the link. It is one rule at every level: a symlink is not a
# finding, a symlink is not the findings directory, and a path that reached the
# findings directory through a symlink is not the findings directory either.
#
# The answer is the EMPTY SET and not a refusal, because the empty set is what
# the tree listings give for that commit -- a refusal here would part the two
# APIs again, the other way round, wherever another listing resolves the name.
#
# Under core.symlinks=false the same commit materialises that link as a REGULAR
# FILE holding `../elsewhere`, which is not a directory at all and would be read
# as a file listing naming one absent finding -- or, where the link's target text
# is itself a finding's filename, as a file listing RESOLVING a finding nobody
# has filed. The recorded mode catches both, because it is the same 120000 either
# way, and that is why a failure to read it is a refusal and not a shrug.
#
# The other way round -- git records a DIRECTORY and the checkout holds a link
# or a file in its place -- is a listing this cannot read rather than one that
# is empty: the findings are the entries the index records under that path, and
# what the checkout put there is not the ledger. It refuses and says so, which
# is the one answer that is neither a false green nor a silent narrowing.
read_listing() {
  local listing="$1" out='' entry path record mode name recorded_regular recorded_other
  local read_status=0
  # A failure here is a refusal and never "git records nothing about it": read
  # as nothing, an unreadable index would put the listing back on the
  # filesystem, which is the fallback every rule above refuses.
  recorded_path_mode "$listing" || return 1
  case "$recorded_mode" in
    100644|100755) ;;
    tree)
      # Git records a DIRECTORY here, so the ledger's findings are the entries
      # it records under it -- and they are reached by walking that directory,
      # which needs the checkout to hold one. Where it holds a link or a file
      # instead, the entries are in the index and not at the end of whatever is
      # there, and a listing this cannot read is a refusal and never an empty
      # set.
      if [[ ! -d "$listing" || -L "$listing" ]]; then
        echo "branch-name-policy: git records '$listing' as a directory and the" >&2
        echo "  checkout does not hold one there, so its findings cannot be read from" >&2
        echo "  it. That is refused rather than read from whatever the checkout put" >&2
        echo "  in its place, which is not the ledger." >&2
        return 1
      fi
      ;;
    '')
      # Git records nothing at this path: an untracked one, a path that reached
      # its destination through a link, or no repository at all. A link is asked
      # about with `-L`, and never `-e` or `-d`, which FOLLOW one rather than
      # seeing it.
      if [[ -L "$listing" ]]; then
        echo "branch-name-policy: findings listing '$listing' is a symlink, so it is" >&2
        echo "  not a findings directory and holds no finding of its own. A tree" >&2
        echo "  listing of the same commit holds none for it either." >&2
        return 0
      fi
      if (( reached_through_symlink )) && [[ -d "$listing" ]]; then
        echo "branch-name-policy: findings listing '$listing' is reached through a" >&2
        echo "  symlink inside the repository, so no index entry and no tree entry is" >&2
        echo "  named by that path: what is at the end of the link is recorded" >&2
        echo "  somewhere else and is not this directory's ledger. It holds no" >&2
        echo "  finding, which is what a tree listing of the same commit gives for" >&2
        echo "  it too." >&2
        return 0
      fi
      ;;
    *)
      echo "branch-name-policy: git records '$listing' as mode $recorded_mode, which is" >&2
      echo "  neither a regular file nor a directory: it is not a findings listing," >&2
      echo "  and it holds no finding. A tree listing of the same commit holds none" >&2
      echo "  for it either." >&2
      return 0
      ;;
  esac
  if [[ -d "$listing" ]]; then
    ls -1 -- "$listing" >/dev/null || return 1
    # WHAT GIT RECORDS DECIDES A TRACKED ENTRY, NOT WHAT THE CHECKOUT
    # MATERIALISED, because only that answers the same question the workflow's
    # mode filter answers. Under core.symlinks=false a committed symlink is
    # checked out as a REGULAR FILE holding the link target -- `git status`
    # empty, recorded mode still 120000 -- so -L sees nothing, and the very
    # commit the tree listings refused at exit 1 conformed here at exit 0.
    #
    # The directory is asked from INSIDE itself, which is safe only because
    # everything above has already established that this path is a real
    # directory that no link was followed to reach.
    index_entries_of "$listing" || {
      echo "branch-name-policy: git could not report what it records for '$listing'" >&2
      return 1
    }
    # Two sets rather than a lookup per name: bash 3.2 has no associative array
    # and this file runs wherever the suite is run by hand. A name is wrapped in
    # newlines on both sides, so a membership test is exact and not a prefix --
    # WHICH HOLDS ONLY BECAUSE NO NAME IN EITHER SET CARRIES A NEWLINE, and
    # index_entries_of drops the ones that do. A recorded symlink named
    # `noise<LF>P2_<category>_<ts>_<desc>.md` would otherwise put that wrapped
    # newline inside a set member, and the real finding of the second name would
    # test as a member of the NON-regular set and be dropped: one twin gone, an
    # ambiguous name conforming at exit 0, and the tree listings refusing the
    # same commit at exit 1. Reading the records whole is not enough on its own;
    # the sets are lines too. A
    # CONFLICTED entry is recorded at SEVERAL stages: an ordinary content
    # conflict is a regular blob at every stage and stays a finding, while a
    # regular file conflicting with a symlink is recorded at both kinds, lands
    # in both sets, and is not a finding -- the non-regular set is tested first.
    recorded_regular=$'\n'
    recorded_other=$'\n'
    while IFS= read -r record; do
      [[ -n "$record" ]] || continue
      mode="${record%% *}"
      name="${record#*$'\t'}"
      case "$mode" in
        100644|100755) recorded_regular+="$name"$'\n' ;;
        *) recorded_other+="$name"$'\n' ;;
      esac
    done <<< "$index_entry_lines"
    # THE CANDIDATE NAMES ARE THE INDEX'S AND THE DIRECTORY'S TOGETHER, and this
    # is the index's half. A TRACKED finding need not be in the checkout at all:
    # a SPARSE checkout leaves `git status` empty with the index still recording
    # every excluded path, and a candidate set taken from the glob alone lost
    # one of two findings sharing a description -- exit 0, `conforms`, on the
    # commit the tree listings refused at exit 1. A name recorded in BOTH is
    # printed twice and `sort -u` in candidate_names makes it one finding again,
    # which is the same dedup a filename in two listings already relies on.
    #
    # An entry below a subdirectory is `sub/name` and is in no listing here: the
    # subdirectory is not a finding whatever it holds, and what it holds is not
    # this directory's. A recorded mode that is not a regular file's is not a
    # finding either -- the non-regular set is tested, not the mode on this
    # record, so a CONFLICTED entry recorded at both kinds stays out.
    while IFS= read -r record; do
      [[ -n "$record" ]] || continue
      name="${record#*$'\t'}"
      case "$name" in
        */*) continue ;;
      esac
      if [[ "$recorded_other" == *$'\n'"$name"$'\n'* ]]; then
        continue
      fi
      printf '%s\n' "$name"
    done <<< "$index_entry_lines"
    # And the directory's half, which is the only half for an entry git does not
    # track -- and the whole of it where there is no repository at all.
    for path in "$listing"/*; do
      entry="${path##*/}"
      # A NAME THAT CANNOT BE HELD ON A LINE IS NOT A FINDING, AND IS SAID SO
      # RATHER THAN SPLIT. The set this resolves against is a set of lines, and
      # a finding's name is P<n>_<category>_<timestamp>_<description>.md, which
      # no newline fits any part of. So such a name matches nothing and can make
      # no other name ambiguous, whichever API asks: the tree listings meet it
      # C-quoted by `git ls-tree` and it matches nothing there either. A
      # CARRIAGE RETURN is different and is left alone -- a directory entry has
      # no line endings, so a carriage return there is part of the name, and it
      # keeps the name off every match without costing it its boundary.
      case "$entry" in
        *$'\n'*)
          echo "branch-name-policy: a name in '$listing' holds a newline, so it is" >&2
          echo "  no finding's name and is not in the candidate set:" >&2
          printf '  %q\n' "$entry" >&2
          continue
          ;;
      esac
      # A 120000 blob, a 040000 tree or a 160000 submodule is not a finding,
      # whatever the checkout put there.
      if [[ "$recorded_other" == *$'\n'"$entry"$'\n'* ]]; then
        continue
      fi
      # And a 100644 or 100755 blob IS one, which is the other half of agreeing
      # with the tree listings: the ledger is what was committed.
      if [[ "$recorded_regular" == *$'\n'"$entry"$'\n'* ]]; then
        printf '%s\n' "$entry"
        continue
      fi
      # An UNTRACKED name has no recorded mode -- git knows nothing about it, or
      # there is no repository at all -- so the filesystem is the only witness
      # left. A SYMLINK IS NOT A FINDING, and it is tested FIRST because -e and
      # -f both follow one: a link named like a finding and pointing at any
      # regular file resolved a fix-P*/ branch here while git's mode filter
      # refused the identical commit. Testing -L first also keeps a DANGLING
      # link a non-finding rather than a read failure, which is what git says
      # about it too -- a 120000 blob is a 120000 blob whether or not anything
      # is at the other end.
      if [[ -L "$path" ]]; then
        continue
      fi
      # An entry the listing named and this cannot stat is a READ FAILURE and
      # not a non-finding: a directory can be readable and still not searchable,
      # and every name in it would otherwise be dropped in silence.
      if [[ ! -e "$path" ]]; then
        echo "branch-name-policy: '$path' is listed and cannot be examined" >&2
        return 1
      fi
      [[ -f "$path" ]] || continue
      printf '%s\n' "$entry"
    done
    return 0
  elif [[ -f "$listing" ]]; then
    # THE FILE IS READ ONCE, AND WHAT IS CHECKED IS WHAT IS PARSED -- read_file
    # opens the caller's path exactly once and everything below parses the
    # private copy it made. Measured on this listing: the NUL check counted the
    # file twice and `cat` read it a third time, a replacement landed in the
    # window between the counts and the read -- atomically, by rename -- and the
    # ambiguous name conformed at exit 0 over `warning: command substitution:
    # ignored null byte in input`, where the unreplaced file refuses it at exit 1.
    #
    # AND A READ THAT FAILED IS NOT THE END OF THE FILE. `read -d ''` reports
    # end-of-input and an I/O error with the same status 1, and the group that
    # was meant to catch the second -- `{ read …; nul=$?; }` -- always succeeds,
    # because a group's status is the last ASSIGNMENT's. So `/proc/self/mem`,
    # which exists, is readable to `-r`, and fails every read, was an EMPTY
    # LISTING at exit 0: the candidate set silently narrowed, and a narrowed set
    # turns an ambiguous name into an accepted one. read_file takes the status
    # from the command that copied the bytes, which distinguishes the two.
    read_file "$listing" || read_status=$?
    if (( read_status == 2 )); then
      echo "branch-name-policy: findings listing '$listing' could not be opened" >&2
      return 1
    fi
    if (( read_status != 0 )); then
      echo "branch-name-policy: findings listing '$listing' could not be read to the" >&2
      echo "  end, so the names in it are not known. That is refused rather than read" >&2
      echo "  as a listing with nothing in it: a narrowed candidate set turns an" >&2
      echo "  ambiguous name into an accepted one." >&2
      if [[ -n "$file_error" ]]; then
        printf '%s\n' "$file_error" | sed 's/^/  /' >&2
      fi
      return 1
    fi
    # A NUL IS NOT A SEPARATOR AND NOT PART OF A NAME, so a listing holding one
    # is a caller who wrote records where lines were asked for. `$(cat …)`
    # DISCARDED it and concatenated the records either side
    # (`P2_…_shared-name.md<NUL>README.md` read as one name ending
    # `.mdREADME.md`), so a twin stopped matching, one match was left, and an
    # ambiguous name conformed at exit 0 where the same two names LF-delimited
    # refused it at exit 1.
    if (( file_has_nul )); then
      echo "branch-name-policy: findings listing '$listing' holds a NUL byte, which" >&2
      echo "  no filename can contain and no line ending is, so its records cannot be" >&2
      echo "  read as names. Write it with LF or CRLF line endings, one finding" >&2
      echo "  filename per line." >&2
      return 1
    fi
    out="$file_bytes"
    # CRLF IS A LINE ENDING HERE AND NEVER PART OF A NAME. A listing written on
    # Windows leaves a carriage return on the end of every name, none of them
    # matches a finding, and the set NARROWS IN SILENCE -- which is precisely
    # how an ambiguous name becomes an accepted one. Measured on two listings
    # naming one description: exit 1 `names 2 findings` with LF throughout, exit
    # 0 `conforms` with the second listing converted to CRLF, the twin gone and
    # nothing said. So a CRLF listing is the same listing, as
    # .github/legacy-branches.txt is already read either way above. The bytes
    # keep the LAST line's ending, where `$(...)` used to eat it, so the pairs
    # are converted first and the final newline goes after them; a blank line
    # left anywhere is a name no finding has and matches nothing.
    out="${out//$'\r\n'/$'\n'}"
    out="${out%$'\n'}"
    # A carriage return that is NOT a line ending is neither a line ending nor
    # part of a name this could match, and guessing which would narrow the set
    # again. A listing this cannot read is a refusal.
    if [[ "$out" == *$'\r'* ]]; then
      echo "branch-name-policy: findings listing '$listing' holds a carriage return" >&2
      echo "  that is not a CRLF line ending, so its names cannot be read. Write it" >&2
      echo "  with LF or CRLF line endings, one finding filename per line." >&2
      return 1
    fi
  else
    echo "branch-name-policy: findings listing '$listing' is neither a file nor a directory" >&2
    return 1
  fi
  [[ -z "$out" ]] || printf '%s\n' "$out"
}

# candidate_names: the merge base, the head and the pull request's own commits
# taken as one set. `sort -u` is what makes a file that appears in more than one
# listing one finding rather than several, which is every fix-P*/ pull request
# that has not touched reviews/findings/ yet.
#
# `|| exit 1` and not `|| return 1`: this block is the left-hand side of a
# pipeline and so a subshell, and a bare `return` from the middle of it would
# leave the LAST listing's status as the block's, hiding a failure on an earlier
# one. `exit` ends the subshell there and `pipefail` carries it out.
candidate_names() {
  {
    if [[ -n "$merge_base_findings" ]]; then read_listing "$merge_base_findings" || exit 1; fi
    if [[ -n "$head_findings" ]]; then read_listing "$head_findings" || exit 1; fi
    if [[ -n "$range_findings" ]]; then read_listing "$range_findings" || exit 1; fi
  } | sort -u
}

# resolve_finding <severity-digit> <category> <description>: the branch claims
# to repair one filed finding. Exactly one filename in that set must carry the
# severity, category and description; the timestamp between them is free.
resolve_finding() {
  local n="$1" cat="$2" desc="$3" names matches count
  [[ -n "$merge_base_findings$head_findings$range_findings" ]] || return 0
  # The set is built FIRST, and a failure to build it refuses. Folding this into
  # the `grep` pipeline below would put a read error and an ordinary no-match on
  # the same footing, and `|| true` would then read an unreadable listing as a
  # listing with nothing in it.
  names="$(candidate_names)" \
    || fail "'$branch' could not be checked: a findings listing could not be read.
  The error is above. A gate that cannot see its input refuses rather than
  deciding it saw nothing, because a narrowed set turns an ambiguous name into
  an accepted one."
  matches="$(grep -E "^P${n}_${cat}_[0-9]+_${desc}\.md$" <<< "$names" || true)"
  count="$(grep -c . <<< "${matches:-}" || true)"
  [[ -n "$matches" ]] || count=0
  case "$count" in
    1) return 0 ;;
    0) fail "'$branch' names no finding anywhere in this pull request:
  expected exactly one P${n}_${cat}_<timestamp>_${desc}.md, as a regular file
  A fix-P*/ branch repairs one filed finding and mirrors its severity, category
  and description. If this bug was never filed, file it in this pull request:
  every commit of it is read, so filing it in one commit and repairing it in the
  next resolves the name even though the repair deletes the file again." ;;
    *) fail "'$branch' names $count findings, which is ambiguous:
$(sed 's/^/  /' <<< "$matches")" ;;
  esac
}

rest="${branch#*/}"
[[ "$rest" != "$branch" ]] || fail "'$branch' has no prefix; it must be <prefix>/<name>"

case "$branch" in
  fix-P[0-3]/*)
    n="${branch#fix-P}"; n="${n%%/*}"
    [[ "$rest" =~ ^${category_re}_${slug_re}$ ]] \
      || fail "'$branch' is not <category>_<description> after the prefix"
    cat="${rest%%_*}"
    desc="${rest#*_}"
    resolve_finding "$n" "$cat" "$desc"
    ;;
  bulk-fix-P[23]/*)
    [[ "$rest" =~ ^${slug_re}$ ]] \
      || fail "'$branch' must be lower-case words joined by single hyphens after the prefix"
    ;;
  bulk-fix-P[01]/*)
    fail "'$branch' batches a severity that is never batched: a P0 or P1 finding
  is repaired on its own fix-P<n>/<category>_<description> branch."
    ;;
  fix/*)
    # A retired prefix says so. It was in the vocabulary until the finding was
    # allowed to be filed by the pull request that repairs it, and a bare "not a
    # known prefix" would send its author looking for a typo.
    fail "'fix/' was retired from the vocabulary: a repair names the finding it
  closes. File the finding under reviews/findings/ if it is not filed already,
  and branch fix-P<n>/<category>_<description> after it. The finding is looked
  for at the merge base, at the head AND in this pull request's own commits, so
  filing and repairing it in one pull request works.
  A pull request that only files or curates findings is findings/<slug>."
    ;;
  feature/*|refactor/*|docs/*|standards/*|ci/*|gate/*|findings/*)
    [[ "$rest" =~ ^${slug_re}$ ]] \
      || fail "'$branch' must be lower-case words joined by single hyphens after the prefix"
    ;;
  *)
    fail "'${branch%%/*}/' is not a known branch prefix"
    ;;
esac

echo "branch-name-policy: '$branch' conforms"
