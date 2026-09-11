#!/usr/bin/env bash
# changed-in-range.sh <target-sha> <head-sha> <out-dir>
#
# Write the two listings .github/scripts/validate-pr-branch.sh judges a pull request's DIFF
# against, from the repository in the current directory:
#
#   <out-dir>/changed-paths     every path the pull request changes, one per line, as `git diff
#                               --name-only` spells it.
#   <out-dir>/added-findings    one line per file the pull request ADDS or RENAMES under
#                               reviews/findings/: `<severity><TAB><path>`, where <severity> is the
#                               value of the `severity:` line in the file's YAML frontmatter AT THE
#                               HEAD, or `-` where the frontmatter carries none.
#
# THIS IS A SCRIPT AND NOT FOUR LINES OF WORKFLOW BECAUSE IT HAS TO BE TESTED, and because the
# validator may not build it. Every external probe, file read and directory listing in
# validate-pr-branch.sh goes through three audited helpers, and .github/scripts/test-pr-policy.sh
# fails the build if anything below its AUDITED HELPERS END marker runs a command that is not a
# shell builtin or redirects from a path -- so the validator cannot run `git` or open a blob, and
# consumes these two files as LISTINGS through the same `read_file` it consumes the three finding
# listings with. The precedent is .github/scripts/findings-in-range.sh, which exists for exactly
# this reason: it was four lines of workflow, the fixtures could not reach it, and three wrong ways
# of building the candidate set each survived a frontier review.
#
# THE BOUNDARY IS THE MERGE BASE, AND EVERY MERGE BASE IS TAKEN. findings-in-range.sh argues the
# choice at length and the argument is the same one: the event's base SHA is the target branch's
# current head, which moves for reasons that have nothing to do with this pull request. Where the
# histories criss-cross there is more than one best common ancestor, and each one's diff is taken
# and the results are UNIONED -- which is the conservative direction for both listings here, since
# a wider set of changed paths can only add a path outside reviews/findings/, and a wider set of
# added files can only add a file to check.
#
# AN END THAT WILL NOT RESOLVE FAILS CLOSED rather than being dropped. A listing that is short is a
# listing that says a pull request touches less than it does, which is the direction that turns a
# refusal into an acceptance.
#
# THE TWO LISTINGS SPELL A PATH DIFFERENTLY, ON PURPOSE.
#
#   changed-paths is git's own `--name-only` output, which C-QUOTES a path holding a control
#   character or a byte outside ASCII: `"reviews/findings/\303\244.md"`, quotes and all. That keeps
#   the guarantee the listing needs -- ONE PATH PER LINE, whatever the path holds, since a newline
#   in a name is escaped rather than written. A quoted path does not begin `reviews/findings/`, so
#   the validator reads it as a path OUTSIDE the ledger and a `findings/` branch carrying one is
#   refused. That is a false refusal for a finding whose name is not ASCII, and it is the safe
#   direction: a finding's name is `P<n>_<category>_<timestamp>_<description>.md`, every byte of
#   which is ASCII, so no name this repository can hold reaches it.
#
#   added-findings carries the path RAW, because the severity comes from the file's own bytes and
#   `git cat-file blob <head>:<path>` needs the path git records rather than a quoted rendering of
#   it. A raw path can hold a newline, and one line per record is what the listing promises, so a
#   newline in an ADDED path under reviews/findings/ is refused here rather than written out to be
#   split into two records downstream.
#
# ONLY THE FILES THE DIFF ADDS OR RENAMES ARE LISTED. An existing finding must never turn somebody
# else's pull request red: a rule over the whole directory would refuse every pull request in the
# repository the day a badly named file landed, and the pull request that has to fix it along with
# them. `-M` asks for rename detection explicitly rather than inheriting `diff.renames` from
# whatever config the runner has; where it does not fire -- a rename whose content changed too much,
# or a diff over `diff.renameLimit` -- the rename is reported as an add and a delete, and the add is
# checked, which is the same answer the long way round. Copy detection is off, so a copied file is
# an add here too.

set -euo pipefail
export PATH="/usr/bin:/bin:$PATH"

target="${1:-}"
head="${2:-}"
out="${3:-}"

if [[ -z "$target" || -z "$head" || -z "$out" ]]; then
  echo "usage: changed-in-range.sh <target-sha> <head-sha> <out-dir>" >&2
  exit 2
fi

git rev-parse --verify --quiet "$target^{commit}" >/dev/null \
  || { echo "target commit $target is not in this checkout" >&2; exit 1; }
git rev-parse --verify --quiet "$head^{commit}" >/dev/null \
  || { echo "head commit $head is not in this checkout" >&2; exit 1; }

mkdir -p "$out"

# No common ancestor is not an empty diff either: it means the two ends are unrelated histories and
# nothing here can be compared.
git merge-base --all "$target" "$head" > "$out/merge-bases" \
  || { echo "no merge base between $target and $head" >&2; exit 1; }
[[ -s "$out/merge-bases" ]] \
  || { echo "no merge base between $target and $head" >&2; exit 1; }

while read -r merge_base; do
  git diff --name-only "$merge_base" "$head" || exit 1
done < "$out/merge-bases" | sort -u > "$out/changed-paths"

# severity_of <path>: the `severity:` value in the YAML frontmatter of that path at the head -- the
# block between the opening `---` on line 1 and the next `---` -- or `-` where there is no such
# line. The same block scripts/pr-ready-audit.sh reads an `id:` out of, and for the same reason:
# the same line written in prose or in a code block further down the file is not frontmatter.
#
# A file with no opening fence has no frontmatter and answers `-`. The FIRST severity line in the
# block wins, so a second one cannot overwrite the first with something acceptable.
severity_of() {
  git cat-file blob "$head:$1" | awk '
    NR == 1     { if ($0 != "---") { print "-"; answered = 1; exit 0 }
                  next }
    $0 == "---" { print (sev == "" ? "-" : sev); answered = 1; exit 0 }
    sev == "" && /^severity:[ \t]/ {
                  value = $0
                  sub(/^severity:[ \t]+/, "", value)
                  sub(/[ \t\r]+$/, "", value)
                  if (value != "") { sev = value } }
    END         { if (!answered) print (sev == "" ? "-" : sev) }
  '
}

# The added and renamed paths, NUL-delimited so a name is never quoted: `--name-status -z` writes
# `<status>\0<path>\0` for an add and `<status>\0<old>\0<new>\0` for a rename, so after a status
# beginning `R` the next record is the old path and the one after it is the new one. The NEW path is
# what is checked, because that is the name the pull request leaves behind.
#
# Read from a FILE and not from a pipe or a process substitution. git's status is taken from the
# command that wrote the file, where it is a status; through `< <(git ...)` it belongs to nobody,
# and a diff that failed would read as a pull request that adds nothing. Reading from a file also
# keeps the loop in THIS shell, so the duplicate set below survives it.
seen=$'\n'
: > "$out/added-findings"
while read -r merge_base; do
  git diff --name-status -M --diff-filter=AR -z "$merge_base" "$head" -- reviews/findings/ \
    > "$out/added-status" || exit 1
  status=''
  expect_old=0
  while IFS= read -r -d '' record; do
    if [[ -z "$status" ]]; then
      status="$record"
      case "$status" in
        R*) expect_old=1 ;;
        *) expect_old=0 ;;
      esac
      continue
    fi
    if (( expect_old )); then
      expect_old=0
      continue
    fi
    path="$record"
    status=''
    case "$path" in
      *$'\n'*)
        echo "a path this pull request adds under reviews/findings/ holds a newline, and a" >&2
        echo "  listing line cannot carry one. Refusing rather than writing a record that" >&2
        echo "  would be split in two downstream:" >&2
        printf '  %q\n' "$path" >&2
        exit 1
        ;;
    esac
    # One path from two merge bases is one added file. Exact, because a name carrying a newline was
    # refused above, so no name in the set spans two lines of it.
    case "$seen" in
      *$'\n'"$path"$'\n'*) continue ;;
    esac
    seen="$seen$path"$'\n'
    # The severity is a WRITE, and a write can fail: taken through `$( )` inside the `printf`
    # below, a `git cat-file` that could not read the blob would have handed the listing an empty
    # field with nothing to say so.
    sev="$(severity_of "$path")" \
      || { echo "could not read $path at $head, so its severity is not known" >&2; exit 1; }
    [[ -n "$sev" ]] \
      || { echo "no severity could be read for $path at $head" >&2; exit 1; }
    printf '%s\t%s\n' "$sev" "$path" >> "$out/added-findings"
  done < "$out/added-status"
  # A status with no path after it is a diff that arrived in part.
  [[ -z "$status" ]] \
    || { echo "the diff of $merge_base..$head ended after a status with no path" >&2; exit 1; }
done < "$out/merge-bases"
