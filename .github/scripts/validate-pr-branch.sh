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
# exactly one filed finding" AGAINST THE LISTINGS IT IS HANDED, which are the
# ledger as it stands when the check runs. It is not a statement about the head,
# and no way of choosing the boundary would make it one.
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
# tree listings refused it at exit 1. So for each name `ls` reports, a directory
# listing takes the mode git records for it, which is the filter
# findings-in-range.sh applies to the very same entry. The filesystem decides
# only for an entry git does not track, where there is no recorded mode and a
# symlink is skipped by -L.
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

# legacy_exempt: is THIS pull request one the migration list names? Both the
# number and the branch must match the same line. The fields are compared with
# `==` and never handed to a pattern matcher, so a name beginning with a dash is
# a name and not a set of options.
legacy_exempt() {
  local pr="${PR_NUMBER:-}" listed_pr listed_branch
  [[ -f "$legacy_file" ]] || return 1
  [[ "$pr" =~ ^[0-9]+$ ]] || return 1
  while read -r listed_pr listed_branch _; do
    listed_branch="${listed_branch%$'\r'}"
    if [[ -z "$listed_pr" || "$listed_pr" == \#* ]]; then
      continue
    fi
    if [[ "$listed_pr" == "$pr" && "$listed_branch" == "$branch" ]]; then
      return 0
    fi
  done < "$legacy_file"
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

# git_index_entries <directory>: `<mode> <object> <stage><TAB><name>` for every
# entry git RECORDS in that directory, one per line, and NOTHING AT ALL when the
# directory is not inside a work tree -- which is every by-hand listing that is
# not a checkout, and where the filesystem is all there is to go on.
#
# A failure INSIDE a work tree is propagated: an index this cannot read is the
# same refusal as a listing it cannot read, and for the same reason.
#
# AND SO IS A FAILURE OF DISCOVERY ITSELF. "There is no repository here" and
# "git could not tell me" are different answers, and only the first may fall
# back to the filesystem. Suppressing git's status made them one answer and the
# answer was the fallback: with `.git/config` unreadable, discovery exits 128,
# that read as "no repository", and a committed symlink materialised as a
# regular file -- the exact case the recorded mode was added to catch --
# conformed at exit 0 with no diagnostic at all, on the commit the tree listings
# refuse at exit 1. AN UNREADABLE REPOSITORY IS NOT PERMISSION TO DISREGARD ITS
# RECORDED MODES. Where git's own discovery cannot separate the two it says "not
# a git repository" -- an unreadable `.git` DIRECTORY reads that way to git
# itself -- and this script has nothing available to it that git has not.
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
git_index_entries() {
  local dir="$1" probe status=0 record
  command -v git >/dev/null 2>&1 || return 0
  # Taken with stderr, so git's own words reach the refusal below, and in the C
  # locale so the sentence git uses for "no repository here" is the one this
  # reads. On success the answer is the LAST line: a warning about some other
  # file it could not read may precede it.
  probe="$(LC_ALL=C git -C "$dir" rev-parse --is-inside-work-tree 2>&1)" || status=$?
  if [[ "$status" -ne 0 ]]; then
    case "$probe" in
      *'not a git repository'*) return 0 ;;
    esac
    echo "branch-name-policy: git could not say what it records for '$dir':" >&2
    sed 's/^/  /' <<< "$probe" >&2
    echo "  A listing inside a repository this cannot read is refused rather than" >&2
    echo "  judged by the filesystem, which cannot see a recorded mode at all." >&2
    return 1
  fi
  [[ "${probe##*$'\n'}" == true ]] || return 0
  # `pipefail` is what carries a failed `ls-files` out of this pipeline, so the
  # loop costs nothing by running in its subshell: it only prints.
  git -C "$dir" ls-files -sz -- . | while IFS= read -r -d '' record; do
    case "${record#*$'\t'}" in
      *$'\n'*) continue ;;
    esac
    printf '%s\n' "$record"
  done
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
read_listing() {
  local listing="$1" out entry path recorded record mode name recorded_regular recorded_other
  if [[ -d "$listing" ]]; then
    ls -1 -- "$listing" >/dev/null || return 1
    # WHAT GIT RECORDS DECIDES A TRACKED ENTRY, NOT WHAT THE CHECKOUT
    # MATERIALISED, because only that answers the same question the workflow's
    # mode filter answers. Under core.symlinks=false a committed symlink is
    # checked out as a REGULAR FILE holding the link target -- `git status`
    # empty, recorded mode still 120000 -- so -L sees nothing, and the very
    # commit the tree listings refused at exit 1 conformed here at exit 0.
    recorded="$(git_index_entries "$listing")" || {
      echo "branch-name-policy: git could not report what it records for '$listing'" >&2
      return 1
    }
    # Two sets rather than a lookup per name: bash 3.2 has no associative array
    # and this file runs wherever the suite is run by hand. A name is wrapped in
    # newlines on both sides, so a membership test is exact and not a prefix --
    # WHICH HOLDS ONLY BECAUSE NO NAME IN EITHER SET CARRIES A NEWLINE, and
    # git_index_entries drops the ones that do. A recorded symlink named
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
    done <<< "$recorded"
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
    out="$(cat -- "$listing")" || return 1
    # CRLF IS A LINE ENDING HERE AND NEVER PART OF A NAME. A listing written on
    # Windows leaves a carriage return on the end of every name, none of them
    # matches a finding, and the set NARROWS IN SILENCE -- which is precisely
    # how an ambiguous name becomes an accepted one. Measured on two listings
    # naming one description: exit 1 `names 2 findings` with LF throughout, exit
    # 0 `conforms` with the second listing converted to CRLF, the twin gone and
    # nothing said. So a CRLF listing is the same listing, as
    # .github/legacy-branches.txt is already read either way above. The trailing
    # strip is the LAST line's ending: `$(...)` has eaten its newline already
    # and left the carriage return behind.
    out="${out//$'\r\n'/$'\n'}"
    out="${out%$'\r'}"
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
