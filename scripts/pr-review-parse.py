#!/usr/bin/env python3
"""pr-review-parse.py: the whole of a review, or nothing at all.

    scripts/pr-review-parse.py review [--nul] [--out FILE] COMMENT-BODY-FILE
    scripts/pr-review-parse.py ledger [--nul] [--out FILE] PULL-REQUEST-BODY-FILE

Each subcommand reads one file and has exactly two outcomes. Either it builds the whole result,
writes it, confirms the write, and exits 0; or it writes no result at all, says why on stderr, and
exits non-zero. There is no third outcome and no partial one, and that is the entire point of this
program existing.

WHY THIS IS A PROGRAM AND NOT A SHELL PIPELINE
----------------------------------------------

`scripts/pr-ready-audit.sh` decides whether a pull request may be enqueued for merge. Its review
parsing lived in bash -- greps, an awk, an embedded python printing tab-separated records, and an
`END` record to say the stream was whole -- and seven consecutive rounds of frontier review found
the same defect seven times, each time in a stage nobody had armoured yet:

  * `|| true` on a read, so "this grep failed" and "this grep matched nothing" were one answer;
  * a pipeline under `pipefail`, whose status is its RIGHTMOST non-zero one, so a reader that died
    (2) stood behind a matcher's ordinary "no match" (1);
  * a here-string, which bash spills to a temporary file once it outgrows a pipe buffer -- a file
    it cannot create is a redirection that failed, the command never runs, and the shell returns
    1, which for `grep` is an answer;
  * a redirection that fails before its command runs at all, so the command's own vocabulary
    answered for it;
  * `read`, whose end-of-input and failed-read are one status, so a list that stopped arriving was
    counted as a list that ended;
  * an in-band completeness marker, which the review's own strings could write, and one did;
  * and finally an unchecked `printf`: a protocol write that returned EIO while the function
    around it returned 0, so one finding vanished from a findings list that still announced itself
    complete. READY, and a merge call, out of a review that blocks.

The root cause is one sentence: IN BASH, FAILURE IS REPRESENTABLE AS SUCCESS. A status that must
be remembered, a stream with no end-to-end integrity, `errexit` suspended inside `||`. Armouring
each site closed that site; it could not close the class, because each round's site was a new
SHAPE rather than a new instance of a known one, and a gate can only test shapes someone has
already imagined.

So the review protocol is not represented in bash any more. In this program a failed read raises,
a failed write raises, a malformed field raises, and an unexpected exception raises; nothing has
to remember to check a status, and there is no arrangement of the code that emits half a result
and returns 0. The caller's whole contract is: RUN IT; IF ITS EXIT STATUS IS NOT 0, BLOCK;
OTHERWISE READ ITS OUTPUT FROM THE FILE IT WROTE.

Three rules keep that true.

* **The result is built and rendered entirely in memory, and the single write happens after the
  last check.** A parse that dies has written nothing, so "the caller has a result" and "the parse
  finished" are the same fact rather than two facts joined by a marker.

* **The write is confirmed before the exit status is decided.** `write()` returning EIO is the
  defect the seventh round found, and a buffered write that is never flushed is that defect
  waiting to happen. The payload is written to a neighbouring `.part` file, flushed, `fsync`ed and
  CLOSED -- each of which raises rather than returning a status -- and only then renamed over the
  destination. A destination file therefore never exists in a partial state, whatever the caller
  does with the exit code.

* **Completeness is not signalled in the data.** It is the exit status, which review content
  cannot reach, and -- for the flat rendering the shell reads -- a field count the shell checks
  against the record count the payload declares. The old protocol's `END` row travelled in the
  same tab-and-newline channel as the data it certified, so a reviewer-supplied `base_sha` holding
  "<a real commit>\\nEND\\t-\\t0" printed a valid-looking completeness marker of its own.

FORMAT DETECTION IS PART OF THE PARSE, for the same reason. There are two review forms -- the
workflow's fenced JSON verdict and the `<!-- upstroke-frontier-review -->` prose -- and they do
not agree about the same review: a JSON review whose object says CHANGES_REQUIRED and carries a P1
reads, to the prose parser, as the `VERDICT: PASS` line sitting outside the object with no
findings at all. Choosing between them on a detection that FAILED is choosing a verdict at random.
Here the detection happens after the file is in memory and cannot fail on its own; there is one
success condition for both forms, and no fall back to whichever parser approves.

THE TWO RENDERINGS are built from the same validated result:

* JSON (the default) -- for a person, and for the gate to assert against;
* `--nul`, a flat NUL-terminated field sequence -- for the shell, which has no JSON parser and
  would need a second program, and so a second success condition, to get one.

NUL is the separator because the review is text and text has no NUL in it -- and where it does,
this program refuses to emit rather than emitting a field that could forge a record boundary.
`$(...)` drops NUL bytes, so the shell reads this through a file and never a command substitution.

EVERYTHING THE OLD PARSERS DECIDED IS DECIDED HERE, to the character. The rules below are ported
one for one from the shapes those parsers had reached after seven rounds of repair -- the head is
the FIRST marker or nothing, a verdict that fails the whole-token check is carried whole rather
than trimmed to the part that passes, a severity outside P0-P3 is a finding the audit cannot
judge rather than one it ignores -- because a rewrite of the parsing path must change no verdict
it should not change, and the corpus of every frontier review in this repository is parsed before
and after to establish that it changed none.
"""

import json
import os
import re
import sys

# ---- what a field may be ------------------------------------------------------------------------

# The one channel rule. Every other rule below is about MEANING; this one is about the separator,
# and it is checked at the moment of emission rather than assumed from the rules that built the
# value. Nothing a review can write may add, remove or forge a record.
NUL = "\0"

# The review forms' own shapes, ported from the parsers this replaces.
#
# THESE RANGES ARE RANGES, which the shell's were not. A range inside a POSIX regex is resolved by
# the LOCALE'S COLLATING ORDER and not by ASCII, so `[0-9a-f]` under en_US.utf8 is not the hex
# alphabet it looks like and `[A-Z_]` is not the upper-case one: `VERDICT: FAILÉPASS` parsed as
# PASS under one locale and as FAIL under `C`, out of one file. The audit's login check answers
# that by writing its set out character by character, and the prose parser answered it by running
# every read under `LC_ALL=C`. Python's ranges are code-point ranges and mean the same thing in
# every environment, so they are written as ranges and the gate still runs each case under every
# locale this machine has.
SHA = re.compile(r"[0-9a-fA-F]{7,40}\Z")
VERDICT_WORD = re.compile(r"[A-Za-z][A-Za-z0-9_-]{0,39}\Z")
SEVERITY = re.compile(r"P[0-3]\Z")
# A field of the protocol holds no control character. This is the old parsers' `field()` rule and
# it is kept as it was: an id or a commit that carries one is not narrowed to "absent", because
# absent is a MANUAL line for a person to read and this is a value that would have written rows.
CONTROL = re.compile(r"[\x00-\x1f\x7f]")

# The severity and MUST tokens a person may have written outside the findings. They are reported
# so the audit can send the review to a person; they are never judged here.
#
# `re.ASCII`, so `\b` is the ASCII word boundary GNU grep uses under `LC_ALL=C` -- the locale the
# prose parser ran every read in. Without it Python's `\b` is Unicode-aware, `é` is a word
# character, and a standalone `P1` written between accented characters stops being a token. The
# safe direction for a stray-token scan is to find MORE of them, because each one is a blocker.
STRAY_TOKEN = re.compile(r"\b(?:P[0-3]|MUST)\b", re.ASCII)

# A finding carrying any of these blocks in every lane (MAINTAINING step 5): the deferring
# implementor's ledger row asserts there is no witness, and a witness the review recorded
# contradicts it.
WITNESS_KEYS = ("witness", "reproduction", "repro", "failing_test", "mutation", "mutation_witness")
# A MUST deviation is fixed whatever its label: a field whose NAME says mandatory/deviation/must_,
# or any string field naming MUST as a word.
MUST_KEY = re.compile(r"(mandatory|deviation|must_)", re.I)
MUST_WORD = re.compile(r"\bMUST\b", re.ASCII)

# The review forms.
#
# `^```json` at the start of a line, or the older bare `role_understanding` object anywhere: a
# prose review that merely QUOTES a JSON object is prose, because the fence has to open a line of
# its own.
IS_JSON_FORM = re.compile(r"^```json|\"role_understanding\"", re.M)
FENCED_OBJECT = re.compile(r"```json\s*(\{.*?\})\s*```", re.S)
BARE_OBJECT = re.compile(r"(\{\"role_understanding.*\})", re.S)

# The prose form, read exactly as the shell read it. `[^ \t\n\r\f\v]` is POSIX `[^[:space:]]` in
# the `C` locale, which is what the greps were given.
NOT_SPACE = r"[^ \t\n\r\f\v]"
# The marker AND THE WHOLE RUN AFTER IT, not the part of that run that looks like a commit: a read
# that matches only what is well formed hands back a listing its own failures have been dropped
# from, and the FIRST line of that listing is then not the first marker in the file. A first
# marker that does not read whole is not a licence to take the second.
HEAD_RUN = re.compile(r"(?:head=|Reviewed head: )" + NOT_SPACE + r"*")
HEAD_WHOLE = re.compile(r"(?:head=|Reviewed head: )([0-9a-f]{40})\Z")
# The same rule for the verdict, and the last one in the file wins.
VERDICT_RUN = re.compile(r"VERDICT:\**:? *" + NOT_SPACE + r"*")
VERDICT_WHOLE = re.compile(r"VERDICT:[*]*:? *([A-Z_]+)[*]*\Z")
NUMBERED_FINDING = re.compile(r"[0-9]+\. \*\*(P[0-3])")

USAGE = "usage: pr-review-parse.py {review|ledger} [--nul] [--out FILE] FILE"


class Unparsed(Exception):
    """The parse cannot be completed. Nothing has been written; nothing will be."""


def read_input(path):
    """The bytes of one input, decoded as UTF-8. Unreadable and undecodable both fail.

    A file the parse cannot read is not an empty file, and bytes that are not UTF-8 are not text
    with the bad bytes dropped: either would be a failure arriving as a smaller answer, which is
    the whole class of defect this program exists to end.
    """
    try:
        with open(path, "rb") as handle:
            data = handle.read()
    except OSError as exc:
        raise Unparsed("cannot read %s: %s" % (path, exc))
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise Unparsed("%s is not valid UTF-8: %s" % (path, exc))


def clean(value):
    """VALUE as a stripped string, or None when it is absent or cannot be a field.

    `None`, a container and a boolean are not values a review recorded; a string carrying a
    control character is a value that would have written rows of the protocol's own language, and
    it is refused rather than repaired. This is the old parser's `field()`, unchanged.
    """
    if value is None or isinstance(value, (dict, list, bool)):
        return None
    text = str(value).strip()
    if not text or CONTROL.search(text):
        return None
    return text


def matching(value, pattern):
    """VALUE when it matches PATTERN whole, and None otherwise.

    WHAT DOES NOT VALIDATE WHOLE IS NEVER TRIMMED TO THE PART THAT DOES. `[A-Z_]+$` matches the
    tail of a dirty token and reads `FAILÉPASS` as PASS, which is a way to approve a change;
    a commit field holds a commit or it holds nothing.
    """
    text = clean(value)
    return text if text is not None and pattern.match(text) else None


def present(value):
    """Whether a finding's field is filled in. null, false, "" and empty containers are not."""
    return value not in (None, False, "", [], {}) and str(value).strip() != ""


def stray_summary(outside):
    """The severity and MUST tokens found outside the findings, as one field, or None.

    Sorted and joined exactly as `sort -u | tr '\\n' '/'` joined them: both orders are by code
    point, because `sort` ran under `LC_ALL=C` too.
    """
    tokens = sorted(set(STRAY_TOKEN.findall(outside)))
    return "/".join(tokens) if tokens else None


# ---- the review ---------------------------------------------------------------------------------


def finding(one):
    """One finding of the workflow form.

    A finding this cannot judge is reported AS a finding the audit cannot judge -- severity `ERR`
    and a reason -- rather than dropped or raised over. That distinction is deliberate and it is
    the one place where "I could not" is a result rather than a failure: the exit status says
    THERE IS NO COMPLETE RESULT, and a review carrying one unreadable finding among five has a
    complete result that names which one. The audit blocks on every `ERR` row it reads.
    """
    if not isinstance(one, dict):
        return {"severity": "ERR", "id": "unparsed", "flags": 0}
    severity = str(one.get("severity", "")).strip()
    raw_id = one.get("id")
    identifier = clean(raw_id)
    if identifier is None and raw_id is not None and str(raw_id).strip():
        # An id that cannot be a protocol field is not narrowed to "no id": no id is a MANUAL line
        # for a person to read, and this is an id that would have written rows.
        return {"severity": "ERR", "id": "bad-id", "flags": 0}
    if not SEVERITY.match(severity):
        return {"severity": "ERR", "id": "bad-severity:" + (identifier or "-"), "flags": 0}
    witness = any(present(one.get(key)) for key in WITNESS_KEYS)
    must = False
    for key, value in one.items():
        if MUST_KEY.search(str(key)) and present(value):
            must = True
        if isinstance(value, str) and MUST_WORD.search(value):
            must = True
    return {
        "severity": severity,
        "id": identifier,
        "flags": int(witness) + 2 * int(must),
    }


def parse_json_review(text):
    """The workflow form: the last fenced JSON object is the verdict, and it is the only source.

    Anything in the comment outside that object which looks like a finding is for a person, and is
    reported as a stray token rather than counted as a finding.
    """
    found = FENCED_OBJECT.findall(text) or BARE_OBJECT.findall(text)
    try:
        verdict = json.loads(found[-1]) if found else None
    except ValueError:
        verdict = None
    if not isinstance(verdict, dict) or not isinstance(verdict.get("findings"), list):
        # The comment announces the workflow form and carries no verdict object this can read.
        # That is a complete description of the review -- it records nothing, and it carries one
        # finding the audit cannot judge -- so it is a result, and every part of it blocks.
        return {
            "kind": "json",
            "reviewed_sha": None,
            "verdict": None,
            "base_sha": None,
            "stray": stray_summary(text),
            "findings": [{"severity": "ERR", "id": "unparsed", "flags": 0}],
        }
    outside = text.replace(found[-1], "")
    return {
        "kind": "json",
        "reviewed_sha": matching(verdict.get("reviewed_sha"), SHA),
        "verdict": matching(verdict.get("verdict"), VERDICT_WORD),
        "base_sha": matching(verdict.get("base_sha"), SHA),
        "stray": stray_summary(outside),
        "findings": [finding(one) for one in verdict["findings"]],
    }


def parse_prose_review(text):
    """The frontier form, read conservatively: numbered `N. **P<n>` findings and the last VERDICT.

    A severity written any other way -- a heading, a sentence -- is a stray token and sends the
    review to a person. The prose form records no base commit, and says so with a null.
    """
    head = None
    first = HEAD_RUN.search(text)
    if first is not None:
        whole = HEAD_WHOLE.match(first.group(0))
        head = whole.group(1) if whole is not None else None
    verdict = None
    runs = VERDICT_RUN.findall(text)
    if runs:
        whole = VERDICT_WHOLE.match(runs[-1])
        # A CHECK THAT FAILED IS THE ANSWER, NEVER THE INPUT TO ANOTHER ATTEMPT. Not one character
        # is stripped off a token that failed the whole-token check: the run stands exactly as it
        # was read, `VERDICT:` and all, so it is not PASS, cannot be turned into PASS, and names
        # itself where the audit prints the blocker. Salvaging the clean word out of
        # `VERDICT: ::PASS` was a new way to approve a change, minted by the branch that had just
        # rejected it.
        verdict = whole.group(1) if whole is not None else runs[-1]
    numbered = [
        {"severity": found.group(1), "id": None, "flags": 0}
        for found in (NUMBERED_FINDING.match(line) for line in text.split("\n"))
        if found is not None
    ]
    outside = "\n".join(
        line for line in text.split("\n") if NUMBERED_FINDING.match(line) is None
    )
    return {
        "kind": "prose",
        "reviewed_sha": head,
        "verdict": verdict,
        "base_sha": None,
        "stray": stray_summary(outside),
        "findings": numbered,
    }


def review_result(args):
    """review FILE: the one review this pull request is judged on, in whichever form it is in."""
    if len(args) != 1:
        raise Unparsed("review takes one file")
    text = read_input(args[0])
    result = parse_json_review(text) if IS_JSON_FORM.search(text) else parse_prose_review(text)
    result["tag"] = "review"
    return result


def review_fields(result):
    """The review as flat fields: seven, the last of which is the finding count, then three each.

    The count is what lets the shell tell a payload that arrived whole from one that stopped part
    way -- `read` ends a loop at end of input and at a failed read alike -- and it is computed
    here, from the result, rather than being a marker any content could write.
    """
    fields = [
        "review",
        result["kind"],
        result["reviewed_sha"] or "-",
        result["verdict"] or "-",
        result["base_sha"] or "-",
        result["stray"] or "-",
        str(len(result["findings"])),
    ]
    for one in result["findings"]:
        fields += [one["severity"], one["id"] or "-", str(one["flags"])]
    return fields


# ---- the ledger ---------------------------------------------------------------------------------


LEDGER_HEADING = "## Review finding ledger"
LEDGER_ID_HEADER = re.compile(r" *ID *\Z")
LEDGER_SEPARATOR = re.compile(r"-+\Z")


def ledger_result(args):
    """ledger FILE: the id and disposition of every row of the pull-request body's finding ledger.

    The header row, the separator and the canonical `None yet` row are not rows, exactly as the
    awk this replaces judged them -- on the untrimmed cell, which is why the patterns carry their
    own spaces. Everything else in the section is a row and is reported as one; a cell holding
    something no finding id could equal can neither match a finding nor hide a match, so there is
    nothing here for a malformed row to gain.
    """
    if len(args) != 1:
        raise Unparsed("ledger takes one file")
    rows = []
    in_ledger = False
    for line in read_input(args[0]).split("\n"):
        if line.startswith(LEDGER_HEADING):
            in_ledger = True
            continue
        if line.startswith("## "):
            in_ledger = False
        if not in_ledger or not line.startswith("|"):
            continue
        cells = line.split("|")
        identifier = cells[1] if len(cells) > 1 else ""
        if LEDGER_ID_HEADER.match(identifier) or LEDGER_SEPARATOR.match(identifier):
            continue
        if "None yet" in identifier:
            continue
        rows.append(
            {
                "id": identifier.strip(" "),
                "disposition": (cells[9] if len(cells) > 9 else "").strip(" "),
            }
        )
    return {"tag": "ledger", "rows": rows}


def ledger_fields(result):
    """The ledger as flat fields: two, the second of which is the row count, then two each."""
    fields = ["ledger", str(len(result["rows"]))]
    for row in result["rows"]:
        fields += [row["id"], row["disposition"]]
    return fields


# ---- rendering and the one write ----------------------------------------------------------------

SUBCOMMANDS = {
    "review": (review_result, review_fields),
    "ledger": (ledger_result, ledger_fields),
}


def rendered(subcommand, result, nul):
    """The bytes to write, complete, or a failure before anything is written."""
    if not nul:
        return (json.dumps(result, indent=2, sort_keys=True, ensure_ascii=False) + "\n").encode(
            "utf-8"
        )
    fields = SUBCOMMANDS[subcommand][1](result)
    for one in fields:
        # The last check before a field becomes a record boundary's neighbour. Nothing that
        # reaches here can hold the separator, and this is where that is established rather than
        # assumed from the rules that built it.
        if NUL in one:
            raise Unparsed("refusing to emit a field holding the record separator")
    return "".join(one + NUL for one in fields).encode("utf-8")


def write_result(payload, out):
    """The single write, confirmed before it counts as one.

    Every step here raises rather than returning a status, and the destination is renamed into
    place only once the bytes are on the device: a `write()` that returns EIO, a `flush()` that
    finds the device full, a `close()` that fails -- each ends the program non-zero with no
    destination file to read. That is the finding this program was written for, and it is closed
    by construction rather than by a check somebody has to remember.
    """
    if out is None:
        sys.stdout.buffer.write(payload)
        sys.stdout.buffer.flush()
        # Closed explicitly: a buffered write that failed must raise HERE, where the exit status
        # is still being decided, and not in an interpreter shutdown handler after this function
        # has already reported success.
        sys.stdout.close()
        return
    part = out + ".part"
    try:
        with open(part, "wb") as handle:
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(part, out)
    except OSError:
        try:
            os.unlink(part)
        except OSError:
            pass
        raise


def complain(message):
    """Say why on stderr, through the byte layer: the environment does not choose this encoding.

    `print` would encode through a text layer whose encoding comes from the environment, and
    `PYTHONIOENCODING=ascii` with one non-ASCII character in a finding id is exactly how the
    parser this replaces was made to die halfway through its output.
    """
    sys.stderr.buffer.write(("pr-review-parse: " + message + "\n").encode("utf-8", "replace"))
    sys.stderr.buffer.flush()


def main(argv):
    nul = False
    out = None
    rest = []
    expecting_out = False
    for arg in argv:
        if expecting_out:
            # `--out` will not take the next option as its value. The audit refuses the same shape
            # for the same reason: consuming an option both misnames the value and silently
            # switches off the flag it swallowed, and here it would send the result somewhere the
            # caller is not reading.
            if arg.startswith("-"):
                complain("--out needs a path, got the option [%s]" % arg)
                return 2
            out = arg
            expecting_out = False
        elif arg == "--nul":
            nul = True
        elif arg == "--json":
            nul = False
        elif arg == "--out":
            expecting_out = True
        elif arg in ("-h", "--help"):
            complain(USAGE)
            return 0
        else:
            rest.append(arg)
    if expecting_out or not rest or rest[0] not in SUBCOMMANDS:
        complain(USAGE)
        return 2
    subcommand = rest[0]
    try:
        result = SUBCOMMANDS[subcommand][0](rest[1:])
        payload = rendered(subcommand, result, nul)
    except Unparsed as exc:
        complain("%s: %s" % (subcommand, exc))
        return 1
    except OSError as exc:
        complain("%s: %s" % (subcommand, exc))
        return 1
    try:
        write_result(payload, out)
    except (OSError, ValueError) as exc:
        complain("%s: writing the result failed: %s" % (subcommand, exc))
        return 1
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv[1:]))
    except SystemExit:
        raise
    except BaseException as exc:  # any failure at all is a failed parse, never a result
        complain("%s: %s" % (type(exc).__name__, exc))
        sys.exit(1)
