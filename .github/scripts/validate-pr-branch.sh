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
#
# A DIRECTORY LISTING IS ANSWERED OUT OF GIT'S RECORDS AND NOT OUT OF THE
# CHECKOUT. This is the rule the last five rounds were spent arriving at, and it
# is worth stating before anything else it replaces.
#
# The other way in -- the three listings the workflow builds -- reads
# `git ls-tree` and has been right every round. The directory way in used to
# answer from the filesystem, with git as a cross-check, and every round found
# another spelling of the same disagreement: a committed symlink named like a
# finding that `-f` followed; a committed symlink AT reviews/findings that `-d`
# followed; the same two under core.symlinks=false, where git materialises a
# 120000 blob as a REGULAR FILE and `-L` has nothing left to see; a sparse
# checkout whose index records findings the working tree does not hold; a
# checkout `reviews` renamed and replaced by a link with the index untouched;
# and a materialised link BEHIND another link, whose target text was read as a
# file listing and resolved a finding nobody has filed. Six shapes, one defect:
# the filesystem was being asked a question only the index can answer.
#
# So the directory input LOCATES the repository and the path within it, and
# then answers from `git ls-files -s` alone: which names exist, and what each
# one is. Nothing below reads `-d`, `-e`, `-L` or a glob for a path git records
# something about, and no materialised file's BYTES are read as a listing. The
# two ways in are now one code path from the index down, so the equivalence
# this gate claims holds by construction rather than by fixture.
#
# WHAT THAT COSTS, STATED PLAINLY. An UNTRACKED finding file inside a tracked
# reviews/findings/ no longer counts for the directory input: the ledger is what
# is committed, and a merge gate decides about commits and never about a work
# tree. A finding its author has written and not yet added is refused by a
# by-hand run, and the answer is `git add`. The disagreement this closes ran the
# other way -- an untracked twin beside a committed finding was exit 1 `names 2
# findings` through the directory and exit 0 `conforms` through the trees -- so
# the trade is one deliberate disagreement for none.
#
# AND WHAT IT LOOSENS, WHICH IS ONE CASE. Where git records a DIRECTORY at the
# listing path and the checkout holds a symlink or a file in its place, this
# used to refuse at exit 1 while the trees resolved the name at exit 0. It now
# RESOLVES, from the index, exactly as the trees do. That is a loosening and it
# is the point: the ledger is the index, the checkout is not the ledger, and the
# two ways in must not answer differently about one commit.
#
# THE FILESYSTEM IS STILL THE WHOLE OF THE EVIDENCE IN ONE PLACE: a listing with
# no repository over it, which is how this script is run against a scratch
# directory and how most of its fixtures run, and a path inside a work tree that
# git records nothing at, under OR ABOVE -- an ordinary untracked scratch
# directory, and the temporary files a caller builds the three listings in.
# A path git records nothing at BECAUSE A COMPONENT ABOVE IT IS A BLOB is not
# one of those: no index entry and no tree entry can be named by it, so it holds
# no finding, and that is what the trees say too.
#
# A REPOSITORY GIT CANNOT READ IS A REFUSAL AND NOT A REPOSITORY THAT IS NOT
# THERE. Reading the records means asking git "is this inside a work tree?", and
# that has three answers and not two: yes, no, and "I could not tell you".
# Collapsing the third into the second is how the filesystem fallback kept
# coming back: `.git/config` unreadable, discovery exiting 128, the status
# thrown away, and a materialised symlink conforming at exit 0 with nothing
# said. Only "there is no repository here" falls back to the filesystem.
#
# WHICH OF THE THREE IT IS, IS DECIDED BY EXIT STATUS AND NEVER BY THE WORDS GIT
# USED. Git's diagnostics quote the paths it was working on, and a path is the
# caller's to choose. A repository at `.../not a git repository - fixture` whose
# `.git/config` is unreadable fails discovery with `unable to access '.../not a
# git repository - fixture/.git/config': Permission denied`, a substring test
# for git's own no-repository sentence matched inside that PATHNAME, and the
# materialised symlink conformed at exit 0 once more with empty stderr. So
# discovery failing is only permission to judge by the filesystem where there is
# no repository for it to have failed ABOUT. That is asked as a second question,
# of `git rev-parse --resolve-git-dir`, which answers by exit status, reads no
# config, and so still says YES for the repository whose config it cannot read.
# A directory named `.git` holding nothing is not a repository to it, which is
# why the question is put to git rather than to `[[ -e ]]`: a stray `/tmp/.git`
# would otherwise turn every by-hand listing under /tmp red.
#
# AND "GIT SAID NO" IS ONLY AN ABSENCE WHERE THIS CAN SEE THERE WAS NOTHING TO
# READ. That test is now made at every level of the walk and for every shape a
# `.git` takes, because it was made for some of them and the ones it missed were
# each a round's P1: an unreadable `.git` FILE; an unreadable `.git` DIRECTORY;
# an unreadable `HEAD`, `objects` or `refs` inside a `.git` this can enter
# perfectly well; and -- last round -- a `.git` FILE that reads perfectly and
# names a gitdir behind an unsearchable `.git/worktrees`, where `[[ -d ]]` on
# the target could not tell an absent directory from one it was not allowed to
# look at, so the level was called empty. A `.git` file that names a gitdir and
# that git would not resolve is now unexaminable outright: git resolves a good
# one, so a refusal there is a repository this cannot read, never an absence.
#
# A NAME THAT CANNOT BE HELD ON A LINE IS NOT A FINDING. A newline is legal in a
# filename and is the separator every listing here is built from, so the one
# separator that cannot occur in a name -- NUL -- is what git is asked for and
# what is read back; nothing converts one into the other. A name carrying a
# newline is then dropped rather than split into two: it can match no
# P<n>_<category>_<timestamp>_<description>.md, so it is no finding and can make
# no other name ambiguous, and `git ls-tree` C-quotes it into a name that
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
# an ordinary no-match.
#
# A LISTING THIS SCRIPT CANNOT READ WITHOUT LOSING SOMETHING IS THE SAME
# REFUSAL. A file listing holding a NUL is not a listing: no filename can contain
# one, so it is a caller who wrote records where lines were asked for. `$(cat …)`
# DISCARDS a NUL rather than failing on it, concatenating the record after it
# onto the one before -- one twin stopped matching, and an ambiguous name
# conformed at exit 0 where the same two names LF-delimited refused it at exit 1.
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
# ABSENCE, AND THAT IS WHY THERE ARE NOW EXACTLY THREE WAYS OUT OF IT. Five
# review rounds each found more of one defect than the round before -- a git
# command, a file read or a directory listing whose status was discarded, and
# whose failure was then read as "nothing there", "empty" or "end of input",
# every one of which CONFORMS:
#
#   `nul=$?` inside `{ ...; nul=$?; }` makes the GROUP succeed, so the `||` that
#   was meant to catch a failed open never ran and every non-zero read status
#   became end-of-file: `/proc/self/mem` as a listing was exit 0 `conforms`.
#
#   A process substitution's status is not the command's, so `done < <(git
#   ls-files …)` followed by `return 0` read an unreadable `.git/index` as "git
#   records nothing at this path", and the filesystem then decided.
#
#   Testing that `.git/.` is accessible says nothing about the metadata inside
#   it, so `chmod 000 .git/HEAD` made both discovery probes exit 128, "no
#   repository" was inferred, and the fallback conformed with empty stderr.
#
#   And -- last round, in the helpers written to end this -- a private copy that
#   went UNREADABLE between the command that wrote it and the read that parsed
#   it. `cat` had copied a listing at exit 0; `read … < copy` then failed to
#   open, reported the same 1 it reports at end of input, and the helper returned
#   SUCCESS WITH EMPTY BYTES. Owning the file establishes nothing about reading
#   it.
#
# So there are three helpers and the call sites are gone. `git_probe` is the
# only place this file runs git, `read_file` the only place it opens a file for
# reading, and `list_dir` the only place it enumerates a directory.
# .github/scripts/test-pr-policy.sh FAILS THE BUILD if anything below the
# AUDITED HELPERS END marker runs a command that is not a shell builtin or a
# function defined in this file, or redirects from a path. That is a text scan
# over one file and it bounds what is WRITTEN here rather than what bash can be
# made to do; what it buys is that the reviewed surface is the audited region,
# and the region is capped at a size that can be read in one sitting.
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
# check and the parse. THAT THE COPY ITSELF WAS READ IS CHECKED and not assumed:
# every private file ends with a sentinel byte, and a read that does not reach it
# is a failure whoever owns the file.
#
# A PATH IS NORMALISED BEFORE IT IS JUDGED. `reviews/findings`,
# `reviews/findings/`, `reviews/findings/.`, `reviews//findings` and
# `reviews/./findings` are one listing, and they answered differently: appending
# `/.` moved the question from `findings` to `.`, and a committed symlink at
# `reviews/findings` that the plain spelling refused at exit 1 conformed at exit
# 0 with three characters added. The spelling is reduced to components first,
# and the path the ledger is asked about is built from THOSE components -- never
# from where the filesystem takes them, which is the whole of why an ancestor
# that is a link can no longer change the answer.
set -euo pipefail
export PATH="/usr/bin:/bin:$PATH"

# The entries of a DIRECTORY with no repository over it are read with a GLOB,
# because a newline is legal in a filename and `ls` writes one name per line.
# These two settings are what keep that glob honest, and it is the only one in
# this file: a pattern that matches nothing has to be an empty list rather than
# the pattern itself -- `dir/*` standing for itself is a name that is not there
# -- and a GLOBIGNORE that matches drops names from the expansion in silence,
# which is the narrowing every rule below refuses.
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

# The directory this script sits in, worked out with parameter expansion and
# `cd`, both of which are builtins: `dirname` is an external command and this
# file runs none outside its audited helpers. `${BASH_SOURCE[0]%/*}` alone is
# the bug .github/scripts/test-pr-policy.sh carries and CLAUDE.md warns about --
# it strips nothing when the script is invoked by bare name from inside its own
# directory -- so the no-separator case is spelled out instead.
self_path="${BASH_SOURCE[0]}"
case "$self_path" in
  */*) self_dir="${self_path%/*}" ;;
  *) self_dir='.' ;;
esac
script_dir="$self_dir"
script_dir="$(CDPATH= cd -P -- "$self_dir" && pwd)"
legacy_file="${LEGACY_BRANCHES:-$script_dir/../legacy-branches.txt}"

branch="${1:-}"
merge_base_findings="${2:-}"
head_findings="${3:-}"
range_findings="${4:-}"

slug_re='[a-z0-9]+(-[a-z0-9]+)*'
# Keep in step with .github/scripts/validate-pr-body.sh's category case.
category_re='(correctness|crash-consistency|security-trust|portability|liveness|performance|compatibility|docs-contract)'

# The vocabulary is held in a variable rather than written by `cat`, which is an
# external command; `read` and `printf` are builtins. `read -d ''` stops at end
# of input and reports it with 1, which is the whole heredoc and not a failure.
vocabulary_text=''
IFS= read -r -d '' vocabulary_text <<'VOCABULARY' || true

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
whichever one the file lived in. Only a regular file git RECORDS is a finding: a
directory or a symlink carrying a finding's name is not one, and neither is a
file the checkout holds and the index does not.

There is no fix/<slug> prefix. A bug worth a branch is worth a finding, so file
the finding and branch fix-P<n>/ after it. findings/<slug> is for a pull request
that touches reviews/findings/ and nothing else; it is not somewhere to put a
repair.

There is deliberately no prefix for a `test`, `chore`, `perf`, `security` or
`build` change even though those are valid title types. If you need one, that is
a gap in the vocabulary to raise rather than a name to work around: say so and
the rule changes in MAINTAINING.md.
VOCABULARY

vocabulary() {
  printf '%s' "$vocabulary_text" >&2
}

# Two spaces in front of every line of a captured message, so a refusal can
# QUOTE what git said without any decision being taken from it. A `while read`
# loop and not `sed`, which is an external command.
indent() {
  local line
  while IFS= read -r line; do
    printf '  %s\n' "$line" >&2
  done <<< "$1"
}

fail() {
  echo "branch-name-policy: $*" >&2
  vocabulary
  exit 1
}

[[ -n "$branch" ]] || fail 'no branch name was given'

# ==== AUDITED HELPERS BEGIN ===================================================
#
# The only three places in this file that run git, open a file for reading, or
# enumerate a directory. .github/scripts/test-pr-policy.sh asserts that over the
# rest of the file by shape: below AUDITED HELPERS END nothing may run a command
# that is not a shell builtin or a function defined here, and nothing may
# redirect from a path. Keep this region readable in one sitting; that is the
# whole of its value.

# Where a probe's output goes: this script's own directory, mode 700 from mktemp,
# so a copy taken here cannot be replaced between the check and the parse --
# which is the defect a caller's path carries and a private file does not.
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

# EVERY PRIVATE FILE THIS SCRIPT WRITES ENDS WITH ONE SENTINEL BYTE, and
# read_private is the only thing that reads one back. `read` reports end of input
# and a read error with the same 1, and a redirection whose OPEN fails reports
# nothing at all to its caller: taking read permission off a private copy AFTER
# the command that wrote it had succeeded left the old helpers returning success
# with empty bytes, and the validator conformed. Both halves are needed -- the
# open is `exec`, whose status is a status, and the sentinel is what says the
# bytes arrived, so a read that stopped early cannot pass for a file that ended.
# `private_records` is the NUL-delimited records; `private_tail` what followed
# the last NUL, sentinel removed.
private_records=()
private_tail=''
read_private() {
  local path="$1" record
  private_records=()
  private_tail=''
  { exec 8< "$path"; } 2>/dev/null || return 1
  record=''
  while IFS= read -r -d '' record <&8; do
    private_records[${#private_records[@]}]="$record"
    record=''
  done
  exec 8<&-
  [[ "$record" == *$'\001' ]] || return 1
  private_tail="${record%$'\001'}"
  return 0
}

# git_probe's answers: `probe_status` is git's exit status and one the caller
# enumerated; `probe_records` its stdout split on NUL, the one byte no pathname
# holds; `probe_text` the first record without its line ending, for the probes
# that answer in one word; `probe_stderr` what git said, kept so a refusal can
# QUOTE it without any decision being taken from it.
probe_status=0
probe_text=''
probe_stderr=''
probe_records=()

# git_probe <expected-statuses> -- <git arguments>...
#
# <expected-statuses> is a comma-separated list of the exit statuses THIS CALLER
# READS AS ANSWERS. Any other status refuses the whole run: a status nobody
# enumerated is not an absence, an empty set or an end of input, and reading it
# as one of those is every wrong acceptance this gate has given. Stdout goes to a
# FILE and the status is taken from git itself -- neither `$(...)`, which
# discards the NUL that delimits `-z` records so two names arrive as one, nor
# `<(...)`, whose status is not the command's and is what made an unreadable
# index read as an empty one.
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
  { git "$@" || probe_status=$?; printf '\001'; printf '\001' >&2; } \
    > "$probe_dir/git.out" 2> "$probe_dir/git.err"
  read_private "$probe_dir/git.err" || {
    echo "branch-name-policy: git ran and what it printed could not be read back," >&2
    echo "  so its answer is not known. '$branch' was not judged." >&2
    exit 1
  }
  if (( ${#private_records[@]} > 0 )); then
    probe_stderr="${private_records[0]}"
  else
    probe_stderr="$private_tail"
  fi
  probe_stderr="${probe_stderr%$'\n'}"
  read_private "$probe_dir/git.out" || {
    echo "branch-name-policy: git ran and its output could not be read back, so" >&2
    echo "  what it records is not known. That is refused rather than read as a" >&2
    echo "  repository that records nothing. '$branch' was not judged." >&2
    exit 1
  }
  if (( ${#private_records[@]} > 0 )); then
    probe_records=( "${private_records[@]}" )
  fi
  if [[ -n "$private_tail" ]]; then
    probe_records[${#probe_records[@]}]="$private_tail"
  fi
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
    indent "$probe_stderr"
  fi
  exit 1
}

# read_file's answers, valid until the next call: `file_bytes` is the whole file
# with its final line ending kept; `file_has_nul` says a NUL was found, which is
# a refusal at every call site because no filename holds one.
file_bytes=''
file_has_nul=0
file_error=''

# read_file <path>: 0 the whole file was read, 1 it could not be read to the end,
# 2 it could not be opened.
#
# ONE OPEN OF THE CALLER'S PATH, BY THIS SHELL, and everything parsed afterwards
# is the private copy. A path is not a value: it can hold different bytes at
# every open, and a check on bytes that are then re-read is a check on bytes
# nobody parsed -- measured on a listing replaced by rename between the two. And
# END-OF-INPUT IS SEPARATED FROM A READ ERROR, which `read` reports with the same
# 1: the copy is made by `cat` from the descriptor this shell already opened --
# it opens nothing itself -- and the refusal comes from ITS status.
# `/proc/self/mem` is the case: openable, readable to `-r`, every read fails.
read_file() {
  local path="$1" copy_status=0
  file_bytes=''
  file_has_nul=0
  file_error=''
  { exec 9< "$path"; } 2>/dev/null || return 2
  { cat <&9 || copy_status=$?; printf '\001'; printf '\001' >&2; } \
    > "$probe_dir/slurp" 2> "$probe_dir/slurp.err"
  exec 9<&-
  if (( copy_status != 0 )); then
    if read_private "$probe_dir/slurp.err"; then
      file_error="${private_tail%$'\n'}"
    fi
    return 1
  fi
  read_private "$probe_dir/slurp" || {
    file_error='the private copy of it could not be read back to its end'
    return 1
  }
  if (( ${#private_records[@]} > 0 )); then
    file_has_nul=1
    return 0
  fi
  file_bytes="$private_tail"
  return 0
}

# list_dir <directory>: its entry names, in `dir_entries`. Reached only where
# there is no repository over the listing, or where git records nothing at, under
# or above it. `ls` is run FOR ITS STATUS, because a directory that is readable
# and not listable must refuse rather than read as empty and a glob that matched
# nothing cannot say which happened. The NAMES come from the glob, because `ls`
# writes one name per line and A NEWLINE IS LEGAL IN A FILENAME: from `ls`,
# `noise<LF>P2_<category>_<ts>_<desc>.md` arrives as two names, the second either
# absent from the directory or a name that is no finding answering for one. Both
# skip a dot name, and both are right to: a finding's name begins with `P`.
dir_entries=()
list_dir() {
  local dir="$1" path
  dir_entries=()
  ls -1 -- "$dir" >/dev/null || return 1
  for path in "$dir"/*; do
    dir_entries[${#dir_entries[@]}]="${path##*/}"
  done
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
# resolving it on the filesystem would follow a link. A LEADING run of `..` is
# kept and allowed: it names the starting directory, which is above every
# component this judges.
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
# true throughout. An entry is looked for with `-e` OR `-L`, so a file this
# cannot read and a link with nothing at the end of it both count as there.
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
# up, one level down, and it has now been measured four ways: an unreadable
# `.git` FILE, an unreadable `.git` DIRECTORY, an unreadable `HEAD`, `objects` or
# `refs` INSIDE a `.git` this can enter perfectly well, and a `.git` FILE that
# reads perfectly and names a gitdir behind an unsearchable `.git/worktrees`.
# Metadata that is MISSING may fall back; metadata that CANNOT BE EXAMINED must
# refuse.
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
# A `.git` FILE THAT NAMES A GITDIR AND THAT GIT WOULD NOT RESOLVE IS
# UNEXAMINABLE OUTRIGHT. That is the fourth shape and the one `[[ -d ]]` could
# not see: with the main repository's `.git/worktrees` unsearchable, the linked
# worktree's `.git` file reads perfectly, names its gitdir, and `[[ -d <target>
# ]]` is FALSE -- not because the target is absent but because nothing may stat
# through the directory above it. The two cases are indistinguishable from here
# and only one of them is an absence, so neither is treated as one: git resolves
# a healthy `.git` file, and a refusal on one that names a gitdir is a repository
# this cannot read.
#
# A GIT_DIR or GIT_WORK_TREE in the environment points git at an index this walk
# cannot reach, so it counts as a repository in play and the answer is yes.
repository_above() {
  local dir="$1" gitdir pointer
  [[ -z "${GIT_DIR:-}" && -z "${GIT_WORK_TREE:-}" ]] || return 0
  # The path AS IT WAS WRITTEN, made absolute with `$PWD` so the walk has a top
  # to stop at. It is NOT resolved through `cd -P`: that costs a subshell, and
  # `/proc/self` -- a listing the fixtures use precisely because every read of it
  # fails -- resolves there to the SUBSHELL'S pid, a directory that is gone
  # before the next command runs, which turned a readable listing into "git could
  # not say".
  #
  # `-e`, `-d` and `-f` on `<level>/.git` FOLLOW a link at <level>, so a
  # repository at any level this path names is still seen. One above a link's
  # TARGET and not above the link itself is not -- and cannot matter: no index in
  # it can hold an entry named by this path, so locate_listing refuses that path
  # whether or not this walk found the repository.
  case "$dir" in
    /*) ;;
    *) dir="${PWD%/}/$dir" ;;
  esac
  while :; do
    git_probe '0,128' -- rev-parse --resolve-git-dir "$dir/.git"
    if (( probe_status == 0 )); then
      return 0
    fi
    # Git said no. This level is an ABSENCE only where this can see that there
    # was nothing to read. The tests are attempts and not permission bits: `-e
    # <dir>/.` needs search on the directory, and the open needs read on the
    # file, which is what git needed and did not get.
    if [[ ! -e "$dir/." ]]; then
      unexaminable_git="$dir"
      return 2
    fi
    gitdir="$dir/.git"
    if [[ -d "$gitdir" ]]; then
      if [[ ! -e "$gitdir/." ]] || gitdir_shaped "$gitdir"; then
        unexaminable_git="$gitdir"
        return 2
      fi
    elif [[ -f "$gitdir" ]]; then
      if ! read_file "$gitdir"; then
        unexaminable_git="$gitdir"
        return 2
      fi
      pointer="$(gitdir_pointer "$file_bytes")"
      if [[ -n "$pointer" ]]; then
        unexaminable_git="$pointer"
        return 2
      fi
    elif [[ -e "$gitdir" || -L "$gitdir" ]]; then
      # Something is there, git would not resolve it, and this cannot say what
      # it is. That is not an absence either.
      unexaminable_git="$gitdir"
      return 2
    fi
    if [[ "$dir" == / ]]; then
      return 1
    fi
    dir="${dir%/*}"
    [[ -n "$dir" ]] || dir=/
  done
}

# THE WORLD A LISTING IS ANSWERED IN, decided once per listing.
#
#   records     it is inside a work tree. `listing_relpath` names it FROM
#               `listing_toplevel`, and git's records are the whole answer.
#   filesystem  there is no repository over it, so the filesystem is the whole
#               of the evidence there is.
#
# THE REPOSITORY IS FOUND ON THE FILESYSTEM AND THE PATH IS NAMED LEXICALLY, and
# those two halves are why an ancestor that is a symlink can no longer change an
# answer. Finding it means entering a directory, which follows links; naming the
# listing means taking the caller's own components, which does not. With
# `reviews` a link to `saved-reviews` and the index untouched, entering
# `reviews/findings` lands in `saved-reviews/findings` -- where git records
# nothing -- while the path the caller NAMED is `reviews/findings`, which the
# index records a finding under. Asking git from inside the link answered the
# first and the trees answer the second, and that disagreement was a P1. The
# work tree's root is matched against the caller's components by INODE and not
# by name, so `/tmp` being a link on macOS, or a listing reached through a link
# ABOVE the repository, costs nothing: those are the same directory.
listing_world=''
listing_toplevel=''
listing_relpath=''

locate_listing() {
  local path="$1" anchor entered=0 said top spelled prefix rest component index above=0
  local prefixes rests
  listing_world=''
  listing_toplevel=''
  listing_relpath=''
  # An ANCHOR to ask git from: the deepest ancestor of the listing this can
  # enter, HANDED ON AS THE CALLER SPELLED IT. Entering is a test and nothing
  # else -- git chdirs for itself and resolves its own physical path -- so no
  # resolved path is carried between commands, which is what `/proc/self` breaks.
  anchor="$path"
  while :; do
    if ( CDPATH= cd -P -- "$anchor" ) 2>/dev/null; then
      entered=1
      break
    fi
    case "$anchor" in
      /|.) anchor='' ;;
      */*) anchor="${anchor%/*}"
           [[ -n "$anchor" ]] || anchor='/' ;;
      *) anchor='.' ;;
    esac
    [[ -n "$anchor" ]] || break
  done
  if (( ! entered )); then
    echo "branch-name-policy: no directory on the way to '$path' could be entered," >&2
    echo "  so whether it is inside a repository is not known. That is refused" >&2
    echo "  rather than judged by the filesystem, which cannot see a recorded mode." >&2
    return 1
  fi
  git_probe '0,128' -- -C "$anchor" rev-parse --is-inside-work-tree
  if (( probe_status != 0 )); then
    # git_probe is about to be called again and its answers are one set, so
    # git's words are kept here or lost.
    said="$probe_stderr"
    repository_above "$anchor" || above=$?
    if (( above == 1 )); then
      listing_world=filesystem
      return 0
    fi
    echo "branch-name-policy: git could not say what it records for '$path':" >&2
    if [[ -n "$said" ]]; then
      indent "$said"
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
  # `false` is a repository with no work tree over this path -- a bare one, or
  # the inside of a `.git` -- where no index entry can name the listing.
  if [[ "$probe_text" != true ]]; then
    listing_world=filesystem
    return 0
  fi
  git_probe '0' -- -C "$anchor" rev-parse --show-toplevel
  top="$probe_text"
  if [[ -z "$top" ]]; then
    echo "branch-name-policy: git says '$path' is inside a work tree and did not say" >&2
    echo "  where its root is, so the path cannot be named in the index. '$branch'" >&2
    echo "  was not judged." >&2
    return 1
  fi
  # The caller's own components, DEEPEST FIRST, until one of them IS the work
  # tree's root. What is left is the listing's name in the index, and no part of
  # it has been resolved on the filesystem.
  #
  # A RELATIVE PATH IS EXTENDED BY `$PWD` FIRST, and lexically: the components a
  # caller names are the ones they typed PLUS the ones the shell is standing in,
  # and a chain that starts at `.` cannot reach a root above it. Run from
  # `reviews/`, the listing `findings` is `reviews/findings` in the index and
  # nothing else -- and a chain of `.` then `findings` matches no root, which
  # refused an ordinary by-hand invocation the previous head accepted. `$PWD` is
  # the shell's own spelling of where it is, so this stays the caller's
  # components throughout; the INODE match below is what lets that spelling and
  # the physical root git reports be the same directory.
  #
  # Deepest first rather than shallowest, because the shortest name is the one
  # the index can hold: `../../repo/reviews/findings` run from `repo/reviews`
  # meets the root at `..` on the way down and would be named
  # `../repo/reviews/findings`, which is no index entry and which git refuses as
  # a pathspec leaving the work tree -- a false red on a path that is simply
  # spelled the long way round. It also takes the name the checkout shows where a
  # link points back inside the same work tree.
  prefixes=()
  rests=()
  case "$path" in
    /*) spelled="$path" ;;
    .) spelled="${PWD:-.}" ;;
    *) spelled="${PWD:-.}/$path" ;;
  esac
  if [[ "$spelled" == /* ]]; then
    prefix='/'
    rest="${spelled#/}"
  else
    prefix='.'
    rest="$spelled"
  fi
  while :; do
    prefixes[${#prefixes[@]}]="$prefix"
    rests[${#rests[@]}]="$rest"
    [[ -n "$rest" ]] || break
    component="${rest%%/*}"
    if [[ "$component" == "$rest" ]]; then
      rest=''
    else
      rest="${rest#*/}"
    fi
    case "$prefix" in
      /) prefix="/$component" ;;
      .) prefix="$component" ;;
      *) prefix="$prefix/$component" ;;
    esac
  done
  index=${#prefixes[@]}
  while (( index > 0 )); do
    index=$(( index - 1 ))
    if [[ "${prefixes[index]}" -ef "$top" ]]; then
      listing_world=records
      listing_toplevel="$top"
      listing_relpath="${rests[index]}"
      return 0
    fi
  done
  echo "branch-name-policy: '$path' is inside the work tree at '$top' and no part" >&2
  echo "  of the path as it was written names that root, so what the index records" >&2
  echo "  for it cannot be worked out. Give the listing as a path that goes through" >&2
  echo "  the repository's own directory. '$branch' was not judged." >&2
  return 1
}

# WHAT GIT RECORDS FOR THE LISTING, and nothing about what the checkout holds.
#
#   tree        it records entries UNDER the path. `recorded_children` holds the
#               immediate ones, `<mode><TAB><name>` per line.
#   blob        it records the path ITSELF, at `recorded_mode`.
#   unnameable  a component ABOVE the path is a blob, so no index entry and no
#               tree entry can be named by this path at all.
#   absent      it records nothing at it, under it, or above it.
#
# `:(literal)` because a pathname is the CALLER'S to choose and a pathspec is
# not a pathname: a listing at `:weird` is read by git as pathspec magic, and
# plain `-- ':weird'` matched nothing at all where the literal form matches the
# path -- a listing silently read as recording nothing, which is the narrowing
# every rule here refuses.
#
# `-z` so a name is never quoted or escaped, and the records are READ as
# NUL-delimited records rather than CONVERTED to lines. A newline is legal in a
# filename and NUL is the one byte that is not, so turning the separator into a
# newline is what let `noise<LF>P2_<category>_<ts>_<desc>.md` arrive as two
# records: the second carried no mode and no tab, the real finding of that name
# was dropped, and the name it should have made ambiguous conformed at exit 0
# while the tree listings refused it at exit 1. A name holding a newline is
# dropped here rather than split, and nothing is lost by it: no such name can
# match P<n>_<category>_<timestamp>_<description>.md, so it is no finding and
# cannot make another name ambiguous. `git ls-tree` C-QUOTES the same name in
# the tree listings, where the quoted form matches no finding either.
#
# `ls-files` is enumerated as answering 0 AND NOTHING ELSE, because by here git
# has already said this path is inside a work tree: a 128 after that is an index
# it could not read, never an absence, and the two were one answer while the
# status was discarded.
#
# THE ANCESTORS ARE ASKED ABOUT ONLY WHERE THE PATH ITSELF RECORDS NOTHING, and
# deepest first, so the query that answers is the narrowest one that can. The
# first ancestor git records anything for settles it: a record wearing that
# ancestor's exact name is a blob, and the path is unnameable; anything else
# means the ancestor is a directory in the ledger and the path below it is
# simply untracked.
recorded_kind=''
recorded_mode=''
recorded_children=''
unnameable_component=''
recorded_kind_of() {
  local rel="$1" record name rest anc
  recorded_kind=''
  recorded_mode=''
  recorded_children=''
  unnameable_component=''
  if [[ -z "$rel" ]]; then
    git_probe '0' -- -C "$listing_toplevel" ls-files -sz
  else
    git_probe '0' -- -C "$listing_toplevel" ls-files -sz -- ":(literal)$rel"
  fi
  if (( ${#probe_records[@]} > 0 )); then
    for record in "${probe_records[@]}"; do
      name="${record#*$'\t'}"
      if [[ -n "$rel" && "$name" == "$rel" ]]; then
        recorded_kind=blob
        recorded_mode="${record%% *}"
        return 0
      fi
    done
    recorded_kind=tree
    for record in "${probe_records[@]}"; do
      name="${record#*$'\t'}"
      if [[ -n "$rel" ]]; then
        case "$name" in
          "$rel"/*) rest="${name#"$rel"/}" ;;
          *) continue ;;
        esac
      else
        rest="$name"
      fi
      # An entry below a subdirectory is `sub/name` and is in no listing here:
      # the subdirectory is not a finding whatever it holds, and what it holds is
      # not this directory's.
      case "$rest" in
        */*) continue ;;
        *$'\n'*) continue ;;
      esac
      recorded_children="$recorded_children${record%% *}"$'\t'"$rest"$'\n'
    done
    return 0
  fi
  anc="$rel"
  while [[ "$anc" == */* ]]; do
    anc="${anc%/*}"
    git_probe '0' -- -C "$listing_toplevel" ls-files -sz -- ":(literal)$anc"
    if (( ${#probe_records[@]} > 0 )); then
      for record in "${probe_records[@]}"; do
        name="${record#*$'\t'}"
        if [[ "$name" == "$anc" ]]; then
          recorded_kind=unnameable
          recorded_mode="${record%% *}"
          unnameable_component="$anc"
          return 0
        fi
      done
      recorded_kind=absent
      return 0
    fi
  done
  recorded_kind=absent
  return 0
}

# The candidate names, one per line and in the order they were read. `sort -u`
# used to collect them and is an external command; the dedup it did is done at
# the match instead of here, because a membership test per name is quadratic in
# the size of the ledger -- 285 findings in three listings is 855 tests against a
# string that grows to 20 KB -- and what the rule needs is only that a filename
# in more than one listing counts ONCE, which is a property of the handful of
# names that MATCH. Order is kept so an ambiguity is reported in the order the
# listings were given.
candidate_lines=$'\n'
add_candidate() {
  candidate_lines="$candidate_lines$1"$'\n'
}

# read_listing <listing>: add every finding filename in one listing to the
# candidate set. A read that FAILS returns non-zero and never an empty set.
#
# THE RECORDS DECIDE WHEREVER THERE ARE ANY, and the filesystem is not consulted
# at all in that case -- not for which names are there, not for what each name
# is, and not for whether the path is a directory. That is what makes this answer
# the same question `git ls-tree` answers for the same commit: a committed
# symlink named like a finding is a `120000 blob` here and there; a sparse
# checkout's excluded finding is an index entry here and a tree entry there; a
# `reviews/findings` the checkout replaced with a link is still the directory the
# index records; and a materialised link's BYTES are never read as a listing,
# whatever the checkout put in its place.
read_listing() {
  local listing="$1" out='' entry record mode name read_status=0 is_file=0
  local regular other
  locate_listing "$listing" || return 1
  if [[ "$listing_world" == records ]]; then
    recorded_kind_of "$listing_relpath"
    case "$recorded_kind" in
      tree)
        # Two sets rather than a lookup per name: bash 3.2 has no associative
        # array and this file runs wherever the suite is run by hand. A name is
        # wrapped in newlines on both sides, so a membership test is exact and
        # not a prefix -- WHICH HOLDS ONLY BECAUSE NO NAME IN EITHER SET CARRIES
        # A NEWLINE. A CONFLICTED entry is recorded at SEVERAL stages: an
        # ordinary content conflict is a regular blob at every stage and stays a
        # finding, while a regular file conflicting with a symlink is recorded at
        # both kinds, lands in both sets, and is not a finding -- the non-regular
        # set is tested first.
        regular=$'\n'
        other=$'\n'
        while IFS= read -r record; do
          [[ -n "$record" ]] || continue
          mode="${record%%$'\t'*}"
          name="${record#*$'\t'}"
          case "$mode" in
            100644|100755) regular="$regular$name"$'\n' ;;
            *) other="$other$name"$'\n' ;;
          esac
        done <<< "$recorded_children"
        while IFS= read -r record; do
          [[ -n "$record" ]] || continue
          name="${record#*$'\t'}"
          case "$other" in
            *$'\n'"$name"$'\n'*) continue ;;
          esac
          case "$regular" in
            *$'\n'"$name"$'\n'*) add_candidate "$name" ;;
          esac
        done <<< "$recorded_children"
        return 0
        ;;
      blob)
        case "$recorded_mode" in
          100644|100755)
            # A tracked regular file is a FILE LISTING: a list of names a caller
            # wrote, which is a different input from the ledger's directory and
            # is read as one wherever it lives. What is read is a real regular
            # file and never a link standing in for one, because a link's target
            # text read as a listing is how a finding nobody filed was invented.
            if [[ -L "$listing" || ! -f "$listing" ]]; then
              echo "branch-name-policy: git records '$listing' as a regular file and the" >&2
              echo "  checkout does not hold one there, so the names in it are not known." >&2
              echo "  That is refused rather than read from whatever the checkout put in its" >&2
              echo "  place." >&2
              return 1
            fi
            is_file=1
            ;;
          *)
            echo "branch-name-policy: git records '$listing' as mode $recorded_mode, which is" >&2
            echo "  neither a regular file nor a directory: it is not a findings listing," >&2
            echo "  and it holds no finding. A tree listing of the same commit holds none" >&2
            echo "  for it either." >&2
            return 0
            ;;
        esac
        ;;
      unnameable)
        echo "branch-name-policy: git records '$unnameable_component' as mode $recorded_mode, so" >&2
        echo "  no index entry and no tree entry is named by '$listing': what is at the" >&2
        echo "  end of it is recorded somewhere else and is not this path's ledger. It" >&2
        echo "  holds no finding, which is what a tree listing of the same commit gives" >&2
        echo "  for it too." >&2
        return 0
        ;;
      *)
        # Git records nothing at this path, under it or above it -- it is
        # untracked, and inside a work tree UNTRACKED NAMES ARE NOT THE LEDGER.
        # A directory of untracked files is the empty set here, which is what a
        # tree listing of the same commit gives for it: counting them is the
        # disagreement this whole rule exists to close, and it ran the expensive
        # way round -- an untracked twin beside a committed finding was exit 1
        # `names 2 findings` through the directory and exit 0 `conforms` through
        # the trees.
        #
        # A REGULAR FILE IS STILL READ, because a file listing is a different
        # input: a list of names the caller wrote, which the workflow builds in
        # RUNNER_TEMP and a maintainer may build anywhere, tracked or not. It is
        # the ledger's DIRECTORY that the records answer for.
        if [[ -L "$listing" ]]; then
          echo "branch-name-policy: findings listing '$listing' is a symlink, so it is" >&2
          echo "  not a findings directory and holds no finding. A tree listing of the" >&2
          echo "  same commit holds none for it either." >&2
          return 0
        fi
        if [[ -f "$listing" ]]; then
          is_file=1
        elif [[ -d "$listing" ]]; then
          echo "branch-name-policy: git records nothing at '$listing', under it or above" >&2
          echo "  it, so the ledger holds no finding there whatever the checkout does." >&2
          echo "  An untracked file is not a filed finding: a tree listing of the same" >&2
          echo "  commit holds none for this path either. Commit the finding to file it." >&2
          return 0
        else
          echo "branch-name-policy: findings listing '$listing' is neither a file nor a directory" >&2
          return 1
        fi
        ;;
    esac
  fi
  # A listing with no repository over it: the filesystem is the whole of the
  # evidence, and a symlink is still not a findings directory.
  if (( ! is_file )) && [[ "$listing_world" != records ]] && [[ -L "$listing" ]]; then
    echo "branch-name-policy: findings listing '$listing' is a symlink, so it is" >&2
    echo "  not a findings directory and holds no finding of its own." >&2
    return 0
  fi
  if (( ! is_file )) && [[ -d "$listing" ]]; then
    list_dir "$listing" || {
      echo "branch-name-policy: findings listing '$listing' is a directory whose entries" >&2
      echo "  could not be listed, so the names in it are not known. That is refused" >&2
      echo "  rather than read as a directory with nothing in it." >&2
      return 1
    }
    if (( ${#dir_entries[@]} > 0 )); then
      for entry in "${dir_entries[@]}"; do
        # A NAME THAT CANNOT BE HELD ON A LINE IS NOT A FINDING, AND IS SAID SO
        # RATHER THAN SPLIT. The set this resolves against is a set of lines, and
        # a finding's name is P<n>_<category>_<timestamp>_<description>.md, which
        # no newline fits any part of. A CARRIAGE RETURN is different and is left
        # alone -- a directory entry has no line endings, so a carriage return
        # there is part of the name.
        case "$entry" in
          *$'\n'*)
            echo "branch-name-policy: a name in '$listing' holds a newline, so it is" >&2
            echo "  no finding's name and is not in the candidate set:" >&2
            printf '  %q\n' "$entry" >&2
            continue
            ;;
        esac
        # A SYMLINK IS NOT A FINDING, and it is tested FIRST because -e and -f
        # both follow one: a link named like a finding and pointing at any
        # regular file resolved a fix-P*/ branch here while git's mode filter
        # refused the identical commit. Testing -L first also keeps a DANGLING
        # link a non-finding rather than a read failure, which is what git says
        # about it too.
        if [[ -L "$listing/$entry" ]]; then
          continue
        fi
        # An entry the listing named and this cannot stat is a READ FAILURE and
        # not a non-finding: a directory can be readable and still not
        # searchable, and every name in it would otherwise be dropped in silence.
        if [[ ! -e "$listing/$entry" ]]; then
          echo "branch-name-policy: '$listing/$entry' is listed and cannot be examined" >&2
          return 1
        fi
        [[ -f "$listing/$entry" ]] || continue
        add_candidate "$entry"
      done
    fi
    return 0
  fi
  if (( is_file )) || [[ -f "$listing" ]]; then
    # THE FILE IS READ ONCE, AND WHAT IS CHECKED IS WHAT IS PARSED -- read_file
    # opens the caller's path exactly once and everything below parses the
    # private copy it made. Measured on this listing: the NUL check counted the
    # file twice and `cat` read it a third time, a replacement landed in the
    # window between the counts and the read -- atomically, by rename -- and the
    # ambiguous name conformed at exit 0 where the unreplaced file refuses it at
    # exit 1.
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
        indent "$file_error"
      fi
      return 1
    fi
    # A NUL IS NOT A SEPARATOR AND NOT PART OF A NAME, so a listing holding one
    # is a caller who wrote records where lines were asked for.
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
    # 0 `conforms` with the second listing converted to CRLF.
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
    # The whole listing at once rather than a call per name: its records are
    # already one name per line, a blank line is a name no finding has, and a
    # loop over 285 names costs ten times what this does.
    candidate_lines="$candidate_lines$out"$'\n'
    return 0
  fi
  echo "branch-name-policy: findings listing '$listing' is neither a file nor a directory" >&2
  return 1
}


# collect_candidates: the merge base, the head and the pull request's own
# commits taken as one set. A filename in more than one of them is one finding
# and not several, which is every fix-P*/ pull request that has not touched
# reviews/findings/ yet.
#
# Each listing is read in THIS shell and appends to the set, so a failure on one
# is a failure here: read through a pipeline it would have been a subshell's, and
# the last listing's status would have stood for all three.
#
# EACH LISTING IS LOCATED AND READ EXACTLY ONCE, AND FOR EVERY BRANCH. A separate
# early check used to look at the listings with the filesystem before this ran,
# which located each one twice -- twelve extra `git` invocations on the three
# listings the workflow passes -- and it could not see a listing that is right:
# a sparse checkout records findings under a path the checkout does not hold, so
# a filesystem existence test on it is a false red on the listing that is most
# certainly correct. So a caller's mistake is reported by the read itself, which
# is the only thing that can tell a missing listing from a recorded one. Reading
# them for a branch that needs no resolution costs one pass and catches a bad
# listing whatever the prefix is.
collect_candidates() {
  if [[ -n "$merge_base_findings" ]]; then read_listing "$merge_base_findings" || return 1; fi
  if [[ -n "$head_findings" ]]; then read_listing "$head_findings" || return 1; fi
  if [[ -n "$range_findings" ]]; then read_listing "$range_findings" || return 1; fi
  return 0
}

if [[ -n "$merge_base_findings$head_findings$range_findings" ]]; then
  collect_candidates \
    || fail "'$branch' could not be checked: a findings listing could not be read.
  The error is above. A gate that cannot see its input refuses rather than
  deciding it saw nothing, because a narrowed set turns an ambiguous name into
  an accepted one."
fi

# resolve_finding <severity-digit> <category> <description>: the branch claims
# to repair one filed finding. Exactly one filename in that set must carry the
# severity, category and description; the timestamp between them is free.
#
# The match is bash's own, not `grep`'s, which is an external command. The
# pattern is built out of a digit, a category from `category_re` and a
# description already matched against `slug_re`, so every byte of it is
# `[a-z0-9-]` or a digit and none of it can be a metacharacter the caller chose.
resolve_finding() {
  local n="$1" cat="$2" desc="$3" re line seen count=0 matches=''
  [[ -n "$merge_base_findings$head_findings$range_findings" ]] || return 0
  # The set was built above, and a failure to build it refused there. Folding
  # that into the match below would put a read error and an ordinary no-match on
  # the same footing.
  re="^P${n}_${cat}_[0-9]+_${desc}\.md\$"
  # One filename in two listings is ONE finding: the same name matching twice is
  # counted once, which is every fix-P*/ pull request that has not touched
  # reviews/findings/ yet. A name matching two DISTINCT findings is ambiguous and
  # is refused, which is the whole point of counting.
  seen=$'\n'
  while IFS= read -r line; do
    [[ -n "$line" ]] || continue
    [[ "$line" =~ $re ]] || continue
    case "$seen" in
      *$'\n'"$line"$'\n'*) continue ;;
    esac
    seen="$seen$line"$'\n'
    count=$(( count + 1 ))
    matches="$matches  $line"$'\n'
  done <<< "$candidate_lines"
  case "$count" in
    1) return 0 ;;
    0) fail "'$branch' names no finding anywhere in this pull request:
  expected exactly one P${n}_${cat}_<timestamp>_${desc}.md, as a regular file
  A fix-P*/ branch repairs one filed finding and mirrors its severity, category
  and description. If this bug was never filed, file it in this pull request:
  every commit of it is read, so filing it in one commit and repairing it in the
  next resolves the name even though the repair deletes the file again." ;;
    *) fail "'$branch' names $count findings, which is ambiguous:
${matches%$'\n'}" ;;
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
