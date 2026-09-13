#!/usr/bin/env bash
# lane.sh: the branch prefix -> lane -> (review effort, must-fix severities) table, defined once.
#
# SOURCE THIS FILE; DO NOT RUN IT. It defines five functions and does nothing else: no `set`, no
# `trap`, no work at load. A caller's `set -e`, `set -u` and `pipefail` are whatever the caller
# chose, and sourcing this changes none of them.
#
# WHY THE TABLE IS IN A FILE OF ITS OWN. The lane a pull request is audited in was derived from its
# branch prefix in three places -- scripts/pr-ready-audit.sh's `lane_for`/`must_fix_for`, and a copy
# each in the box's review-poller.sh and make-fix-brief.sh -- and the three disagreed with each other
# and with the vocabulary .github/scripts/validate-pr-branch.sh enforces. All three read
# `codex/findings-p3-*`, `codex/findings-*` and a catch-all; the vocabulary has thirteen prefixes and
# none of them is `codex/`. Every prefix in it therefore fell into the catch-all, which is the most
# expensive review and the loosest fix set, and nothing said so. One table, sourced by each of them,
# is what stops the three drifting apart again.
#
# AN UNKNOWN PREFIX IS A REFUSAL AND NEVER A DEFAULT LANE. That catch-all is the defect, not a
# convenience: a branch nobody had thought about was silently given `max` effort and a P0-P1 fix
# set. `lane_for` exits non-zero on a name outside the vocabulary and prints the table, so a caller
# that forgets to check gets no lane at all rather than the loosest one.
#
#   prefix                     lane           review effort                    must fix before ready
#   -------------------------  -------------  -------------------------------  ---------------------
#   feature/<slug>             feature        max                              P0-P1
#   refactor/<slug>            refactor       max                              P0-P1
#   ci/<slug>                  ci             max                              P0-P1
#   gate/<slug>                gate           max                              P0-P1
#   standards/<slug>           standards      high                             P0-P2
#   docs/<slug>                docs           low                              P0-P1
#   findings/<slug>            findings       low                              P0-P1
#   fix-P0/<category>_<desc>   fix-p0p1       max                              P0-P1
#   fix-P1/<category>_<desc>   fix-p0p1       max                              P0-P1
#   fix-P2/<category>_<desc>   fix-p2         high                             P0-P2
#   bulk-fix-P2/<slug>         bulk-fix-p2    max                              P0-P2
#   fix-P3/<category>_<desc>   fix-p3         by category: docs-contract low,  P0-P2, plus the P3
#   bulk-fix-P3/<slug>         fix-p3           every other category max         rule below
#
# A `findings/` pull request touches findings/ and nothing else, which is what makes its
# `low` review safe -- the limit is enforced by validate-pr-branch.sh and not here. THE REVIEW OF A
# `findings/` BRANCH ASKS ONE QUESTION: whether any finding it files duplicates one already filed.
# That is the review's brief rather than an effort level, so it is stated here and carried by the
# script that writes the brief; `effort_for findings` answers `low` and nothing more.
#
# THE P3 RULE (owner, 2026-09-08). A P3 lane is ready when its review carries no P0, P1 or P2 and
# three P3s or fewer -- and a P3 carrying a failing test, reproduction or mutation witness is fixed
# whatever the count (MAINTAINING.md step 5), so the tolerance is three UNWITNESSED P3s. It replaces
# "ready only on a PASS", which could only ever be reached by looping reviews. It is a count and not
# a severity, so it is not in `must_fix_for`'s answer; scripts/pr-ready-audit.sh applies it to the
# `fix-p3` lane and is the only place it is applied.
#
# ELEVEN LANES, ONE PER ROW. `fix-P0/` and `fix-P1/` share a row and so share the lane `fix-p0p1`;
# `fix-P3/` and `bulk-fix-P3/` share a row and so share `fix-p3`. Thirteen prefixes, eleven lanes,
# eleven `lane:*` labels.
#
# A LANE LABEL IS AN OUTPUT AND NEVER AN INPUT. Nothing here reads a label, and nothing that sources
# this may read one to decide effort or must-fix: a label is not bound to a commit, so editing one
# would otherwise change which gates apply to a merge.

# lane_for <branch>: the lane, from the prefix and nothing else. Non-zero, with the table on stderr,
# for a branch outside the vocabulary.
lane_for() {
  case "${1:-}" in
    feature/*)              echo feature ;;
    refactor/*)             echo refactor ;;
    ci/*)                   echo ci ;;
    gate/*)                 echo gate ;;
    standards/*)            echo standards ;;
    docs/*)                 echo docs ;;
    findings/*)             echo findings ;;
    fix-P0/*|fix-P1/*)      echo fix-p0p1 ;;
    fix-P2/*)               echo fix-p2 ;;
    bulk-fix-P2/*)          echo bulk-fix-p2 ;;
    fix-P3/*|bulk-fix-P3/*) echo fix-p3 ;;
    *)
      echo "lane: '${1:-}' is not a known branch prefix, so it has no lane." >&2
      lane_vocabulary
      return 1
      ;;
  esac
}

# lane_list: the eleven lanes, one per line, in table order. The label list and both label
# reconciliation loops in scripts/pr-ready-audit.sh are driven from this, because all three used to
# name the lanes by hand and all three had drifted from the table by the time anyone looked.
lane_list() {
  echo feature
  echo refactor
  echo ci
  echo gate
  echo standards
  echo docs
  echo findings
  echo fix-p0p1
  echo fix-p2
  echo bulk-fix-p2
  echo fix-p3
}

# must_fix_for <lane>: the severities that must be fixed before the pull request is ready, as a
# space-separated list, tested by the caller as `[[ " $must_fix " == *" $sev "* ]]`.
#
# A LANE THIS DOES NOT KNOW IS NOT A LANE WITH NOTHING TO FIX IN IT. An empty must-fix set makes
# EVERY severity deferrable -- `[[ "  " == *" P1 "* ]]` is false -- which is the permissive answer,
# and the permissive answer has to be said rather than fallen into. Every lane `lane_for` returns is
# answered below; anything else is a caller that cannot be answered.
must_fix_for() {
  case "${1:-}" in
    feature|refactor|ci|gate|docs|findings|fix-p0p1) echo "P0 P1" ;;
    standards|fix-p2|bulk-fix-p2|fix-p3)             echo "P0 P1 P2" ;;
    *) return 1 ;;
  esac
}

# effort_for <lane> [branch]: the effort the frontier review is run at.
#
# THE BRANCH IS OPTIONAL AND IS CONSULTED FOR ONE LANE. Every other lane is answered from the lane
# alone, so `effort_for "$lane"` stays true for them; `fix-p3` is the one row whose effort the table
# makes a function of the NAME rather than of the lane, because a P3 in `docs-contract` is a
# documentation contract and a P3 anywhere else is code.
#
# FAIL EXPENSIVE, NEVER CHEAP. `effort_for fix-p3` with no branch answers `max`. A caller that
# forgets to pass the branch must not be CHEAPENED into a low review by its own omission; the only
# way to reach `low` here is to name a branch that says `docs-contract`.
#
# THE CATEGORY IS THE ONE THE NAME BEGINS WITH, and it is one rule for both P3 prefixes.
# `fix-P3/` names always carry a category -- the grammar is `<category>_<desc>` -- and a
# `bulk-fix-P3/` slug is hyphen-joined words that may begin with one and may not:
#
#   fix-P3/docs-contract_stale-comment     docs-contract   low
#   fix-P3/correctness_a-bad-thing         correctness     max
#   bulk-fix-P3/docs-contract-sweep        docs-contract   low
#   bulk-fix-P3/misc-cleanup               none            max
#
# THE CATEGORY ENDS AT A WORD BOUNDARY -- a `_`, a `-`, or the end of the name -- and not at an
# arbitrary byte. `<slug>` is lower-case words joined by single hyphens, so "begins with a category
# name" means its leading WORDS spell one: `bulk-fix-P3/docs-contract-sweep` begins with
# `docs-contract` and `bulk-fix-P3/docs-contractual-review` does not, it begins with the word
# `docs`. Matched as a bare byte prefix the second would take the cheap review off the back of a
# word nobody chose, and the cheap review is the answer that has to be earned.
#
# Only `docs-contract` is tested, because it is the only category the table treats differently: a
# name that begins with any of the other seven and a name that begins with none both answer `max`,
# and writing the seven out would be seven arms that change no answer. The eight are the ones
# .github/scripts/validate-pr-branch.sh and .github/scripts/validate-pr-body.sh already share --
# `correctness`, `crash-consistency`, `security-trust`, `portability`, `liveness`, `performance`,
# `compatibility`, `docs-contract` -- the list is duplicated in this third file, and the three must
# move together; each of them says so.
effort_for() {
  local lane="${1:-}" branch="${2:-}" rest
  case "$lane" in
    feature|refactor|ci|gate|fix-p0p1|bulk-fix-p2) echo max ;;
    standards|fix-p2)                              echo high ;;
    docs|findings)                                 echo low ;;
    fix-p3)
      if [[ -z "$branch" ]]; then
        echo max
        return 0
      fi
      rest="${branch#*/}"
      case "$rest" in
        docs-contract|docs-contract_*|docs-contract-*) echo low ;;
        *) echo max ;;
      esac
      ;;
    *) return 1 ;;
  esac
}

# lane_vocabulary: the thirteen prefixes and what each one is audited as, on stderr. Printed by
# `lane_for`'s refusal, because a name outside the vocabulary is usually a name nobody has written
# down rather than a typo, and the answer is the table.
lane_vocabulary() {
  printf '%s\n' >&2 \
    "  the branch prefixes, and the lane each one is audited in:" \
    "" \
    "    feature/<slug>             feature      review max   fix P0-P1" \
    "    refactor/<slug>            refactor     review max   fix P0-P1" \
    "    ci/<slug>                  ci           review max   fix P0-P1" \
    "    gate/<slug>                gate         review max   fix P0-P1" \
    "    standards/<slug>           standards    review high  fix P0-P2" \
    "    docs/<slug>                docs         review low   fix P0-P1" \
    "    findings/<slug>            findings     review low   fix P0-P1" \
    "    fix-P0/<category>_<desc>   fix-p0p1     review max   fix P0-P1" \
    "    fix-P1/<category>_<desc>   fix-p0p1     review max   fix P0-P1" \
    "    fix-P2/<category>_<desc>   fix-p2       review high  fix P0-P2" \
    "    bulk-fix-P2/<slug>         bulk-fix-p2  review max   fix P0-P2" \
    "    fix-P3/<category>_<desc>   fix-p3       review by category, below" \
    "    bulk-fix-P3/<slug>         fix-p3       review by category, below" \
    "" \
    "  A fix-P3 or bulk-fix-P3 name that BEGINS WITH the category docs-contract is reviewed at" \
    "  low effort; every other category, and a name carrying none, is reviewed at max. The" \
    "  category ends at a word boundary, so docs-contract-sweep begins with it and" \
    "  docs-contractual does not." \
    "" \
    "  Both P3 rows fix P0-P2, plus the P3 rule: a review carrying no P0, P1 or P2 and three" \
    "  UNWITNESSED P3s or fewer. A P3 carrying a witness is fixed whatever the count." \
    "" \
    "  There is no default lane. .github/scripts/validate-pr-branch.sh refuses a head branch" \
    "  outside this vocabulary, and MAINTAINING.md states the rule."
}
