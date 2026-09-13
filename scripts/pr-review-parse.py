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

* **The write is confirmed before the exit status is decided, AND ITS COUNT IS READ.** `write()`
  returning EIO is the defect the seventh round found; `write()` returning a SMALLER NUMBER is the
  same defect without an error, and it is the one the twelfth found -- an unbuffered stdout took
  1,024 bytes of a 3,560-byte result, said so in its return value, and the program exited 0 over a
  findings list with 2,536 bytes missing. Every write here loops on what it was told was written
  until the payload is gone or something raises. The payload is written to a staging file THIS
  INVOCATION CREATED, beside the destination, flushed, `fsync`ed and CLOSED -- each of which raises
  rather than returning a status -- and only then renamed over the destination. A destination file
  therefore never exists in a partial state, whatever the caller does with the exit code. The
  staging name is unique because a fixed one is shared: two parses publishing to one destination
  overlapped on `OUT.part`, and the slower one renamed the faster one's bytes into place under its
  own exit 0.

* **Completeness is not signalled in the data.** It is the exit status, which review content
  cannot reach, and -- for the flat rendering the shell reads -- a field count the shell checks
  against the record count the payload declares. The old protocol's `END` row travelled in the
  same tab-and-newline channel as the data it certified, so a reviewer-supplied `base_sha` holding
  "<a real commit>\\nEND\\t-\\t0" printed a valid-looking completeness marker of its own.

AMBIGUITY REFUSES. That is the rule this program reaches a verdict by, and it replaces the
search that used to reach one. There are two review forms -- the workflow's fenced JSON verdict
and the `<!-- upstroke-frontier-review -->` prose -- and they do not agree about the same review:
a JSON review whose object says CHANGES_REQUIRED and carries a P1 reads, to the prose parser, as
the `VERDICT: PASS` line sitting outside the object with no findings at all. So every question
below is answered by counting rather than by finding, and every count that is not one is a
refusal:

  * the comment holds EXACTLY ONE place a verdict could be read from, or there is no result.
    Zero is not a reason to try the other parser; two is not a reason to pick one of them --
    whether the second is a quoted example, a block inside a blockquote, or a block inside an
    HTML comment;
  * every run of three or more backticks or tildes in the comment was consumed by the structure
    scan as a fence of a block that scan found. One that was not means the comment's block
    structure is NOT what this program thinks it is, and there is no result;
  * and no block the verdict was not read from holds a `json` fence line or a bare verdict
    opener. Accounting for every run says each one was consumed as a fence; it says nothing about
    WHICH block it was consumed into, and CommonMark suppresses a fence inside a raw-HTML block
    where this scan does not -- so a hidden `` ```text `` line swallowed a real blocking verdict
    as its content, left every run accounted for, and let a `PASS` appended after it stand as the
    only candidate. What such a block holds is material, on the same terms as what lies outside
    one;
  * nothing but whitespace follows the block the verdict is read from, and no `VERDICT:` line
    stands outside it. Not "nothing this program recognises as a block" -- nothing;
  * no object the verdict is read from names anything twice, AT ANY DEPTH. `json.loads` keeps
    the LAST occurrence of a repeated name, so an object carrying a `findings` array with a P1 in
    it and then a second `"findings":[]` deserialises to a clean PASS with no findings -- one
    document, two readings, and the reading this program would have taken is the one a reader of
    the comment does not see;
  * and the form is not chosen by which parser gets an answer. A comment carrying the prose
    form's marker is the prose form, and one carrying a verdict block as well is a comment
    claiming to be both, which is a refusal rather than a choice.

Three rounds of review put three more spellings of one fenced block through the recogniser this
file used to trust: one leading space, then `> ` before the fence, then the fence hidden in an
HTML comment. A recogniser made cleverer each time is a recogniser that is wrong in a way nobody
has thought of yet. A count does not have to be clever: what it cannot resolve, it refuses, and a
refusal costs the reviewer one re-post where a missed CHANGES_REQUIRED costs a merged P1.

A REAL COMMONMARK PARSE WOULD BE BETTER, and there is none to use: Python's standard library ships
no Markdown parser, `markdown-it-py` and the rest are dependencies, and this repository takes none
for a gate script. What is here is therefore NOT an approximation of the specification -- two of
those have now failed -- but a structure scan whose every unresolved character is a refusal.

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

import collections
import json
import os
import re
import sys
import tempfile

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

# AND THE SPELLING A DECODER READS, NOT ONLY THE ONE THE COMMENT WRITES. `"severity":"P\u0031"` is
# `P1` to `json.loads`, to GitHub's renderer and to the person reading the comment, and it is
# nothing at all to a regex run over the characters the comment spells it with. Every witness in
# this family carried its blocking severity past this scan that way -- the truncation revival, the
# indented fence, the repeated name, the HTML comment and the fenced object all spell it `P\u0031`
# -- because the scan that exists to catch a severity outside the verdict object could not read the
# only spelling those witnesses use. `\uXXXX` and JSON's two-character escapes are resolved below;
# a doubled backslash is consumed as the one character it is, so `\\u0031` is not read as an escape
# it is not. Each `\uXXXX` is resolved on its own, which is enough for every spelling of an ASCII
# token, and `P0`-`P3` and `MUST` are ASCII.
JSON_ESCAPE = re.compile(r"\\u([0-9a-fA-F]{4})|\\(.)", re.S)
SIMPLE_ESCAPE = {'"': '"', "\\": "\\", "/": "/", "b": "\b", "f": "\f",
                 "n": "\n", "r": "\r", "t": "\t"}

# A finding carrying any of these blocks in every lane (MAINTAINING step 5): the deferring
# implementor's ledger row asserts there is no witness, and a witness the review recorded
# contradicts it.
WITNESS_KEYS = ("witness", "reproduction", "repro", "failing_test", "mutation", "mutation_witness")
# A MUST deviation is fixed whatever its label: a field whose NAME says mandatory/deviation/must_,
# or any string field naming MUST as a word.
MUST_KEY = re.compile(r"(mandatory|deviation|must_)", re.I)
MUST_WORD = re.compile(r"\bMUST\b", re.ASCII)

# THE FORM IS THE COMMENT'S OWN MARKER, not whichever parser gets an answer out of it. The prose
# form writes this marker, every prose review in the repository carries it, and no workflow-form
# review does; a comment carrying it AND a verdict block is a comment claiming to be both forms,
# which is a refusal. "I could not read the JSON, so I will try prose" is how a blocking review
# became a PASS: the object said CHANGES_REQUIRED with a P1 spelled `"P\u0031"`, and the prose
# parser read the `VERDICT: PASS` line sitting outside it with no findings at all.
PROSE_MARKER = re.compile(r"<!-- upstroke-frontier-review")
# The prose form's verdict, looked for OUTSIDE a workflow verdict block. No workflow-form review in
# the repository carries one, and one that did would be a comment saying two different things.
PROSE_VERDICT = re.compile(r"VERDICT:")

# The older bare form's object opener, with whatever a writer left between the brace and the key.
# `\{\"role_understanding` was this pattern without the `\s*`, and ONE SPACE walked past it: a
# comment holding a complete fenced `PASS` example and then a real `{ "role_understanding":...}`
# object saying CHANGES_REQUIRED read as the example, with the object's `"P\u0031"` invisible to
# the stray scan. The bare form has no fence, so its object runs from the opener to the last `}`.
BARE_OBJECT_OPEN = re.compile(r"\{\s*\"role_understanding\"")

# A FENCE IS A LINE, AND THE LINES ARE READ IN ORDER, BY COMMONMARK'S RULES -- which are the rules
# GitHub renders these comments by, so they are the rules that decide what a reader of the comment
# sees as the verdict.
#
#   * an opening fence is three or more backticks, or three or more tildes, INDENTED BY UP TO THREE
#     SPACES, followed by an info string naming the language (a backtick fence's info string holds
#     no backtick);
#   * a closing fence is the same character, AT LEAST AS LONG, indented by up to three spaces
#     INDEPENDENTLY OF THE OPENING FENCE, and holds nothing after it but spaces and tabs -- so a
#     content line opening ```` ```a code span``` ```` closes nothing, which is how a
#     `failure_sequence` quoting a code span stays inside its own block. A carriage return is
#     allowed there too: a comment body GitHub stored with CRLF endings is the same comment, and
#     the fence the reader saw closed the block;
#   * the opening fence's indentation is stripped from each line of the content;
#   * a fence that is never closed runs to the end of the comment, and is NOT closed.
#
# https://spec.commonmark.org/0.31.2/#fenced-code-blocks. `^```json` was none of this: one leading
# space and the real verdict stopped being a block at all, so the block before it -- a `PASS` the
# comment quoted as an example -- was selected instead, with the real object's escaped
# `"severity":"P\u0031"` invisible to the stray scan. That is the same revival round 11 closed for
# a truncated block, arriving through the recogniser instead of through the completeness check,
# which is why the rule below does not rest on the recogniser being right.
FENCE_OPEN = re.compile(r"( {0,3})(`{3,}|~{3,})([^\n]*)\Z")
FENCE_CLOSE = re.compile(r" {0,3}(`{3,}|~{3,})[ \t\r]*\Z")
# WHAT A FENCE IS MADE OF, ASKED IN CHARACTERS. Three or more backticks or tildes is the only way
# CommonMark opens a fenced block, so this run is present wherever a fence is -- whatever the rest
# of the line looks like: indented past three spaces, behind a blockquote's `> `, behind a list
# marker, inside an HTML comment. It is what the structure scan is CHECKED AGAINST rather than
# trusted about: every one of these runs must have been consumed by that scan as a fence of a block
# it found, and one that was not means the comment's block structure is not what this program
# thinks it is. That check cannot be written in the recogniser's own vocabulary, or it agrees with
# the recogniser by construction -- which is how a tail check enumerating
# `\{\"role_understanding` missed `{ "role_understanding` and let an earlier PASS stand as the
# verdict.
FENCE_RUN = re.compile(r"`{3,}|~{3,}")
# AND A LINE OF A BLOCK'S CONTENT THAT WOULD OPEN ONE, asked the same way. A fence run this scan
# consumed AS A FENCE is accounted for by `FENCE_RUN` above whether or not it opened the block the
# reader sees -- which is the hole `unresolved_material` was blind to, because accounting for every
# run says nothing about which block each one belongs to. CommonMark suppresses a fence inside a
# raw-HTML block -- an unclosed `<!--`, a `<div>`, a `<pre>`, any complete tag on a line of its own
# (https://spec.commonmark.org/0.31.2/#html-blocks) -- and this scan does not, so a fence opened
# inside one swallows the real verdict as its content and leaves every run accounted for. What such
# a block's content holds is therefore material too, and a line of it SHAPED LIKE THE OPENING FENCE
# of a `json` block is the material that matters: that is the one shape the verdict is ever read
# from, so a block whose content carries one is a block this program cannot be sure it is reading
# past. The info string is read the way `names_json` reads every other one; a line this matches and
# CommonMark would not -- a backtick fence whose info string holds a backtick -- is a refusal rather
# than a reading, which is the direction every rule in this file is wrong in when it is wrong.
CONTENT_FENCE_OPEN = re.compile(r"^ {0,3}(?:`{3,}|~{3,})([^\n]*)$", re.M)

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


def decoded_spelling(text):
    """TEXT with JSON's string escapes resolved, so an encoded token is a token to the scan above.

    THE SCAN READS THE SEVERITY THE PARSER READS. `"severity":"P\\u0031"` decodes to `P1` -- for
    `json.loads`, for GitHub's renderer and for the person reading the comment -- and carries no
    `P1` at all for a regex over the characters the comment spells it with. That one gap is what
    every witness in this family used to get a blocking severity past `stray_summary`: the severity
    only exists after JSON decoding, and the scan ran over raw text.

    Resolved the way a JSON decoder resolves them, and in one left-to-right pass, so a doubled
    backslash is consumed as the one character it is: `\\\\u0031` is a backslash followed by
    `u0031` and is NOT an escape, which a pass that resolved `\\uXXXX` anywhere would read as one.
    An escape this cannot resolve -- `\\uZZZZ` is not four hex digits, and `\\q` is not an escape
    JSON has -- is left exactly as it was written rather than dropped: a token must not be able to
    hide in the gap between what this understands and what it discards.
    """
    def resolved(match):
        point, simple = match.group(1), match.group(2)
        if point is not None:
            return chr(int(point, 16))
        return SIMPLE_ESCAPE.get(simple, match.group(0))
    return JSON_ESCAPE.sub(resolved, text)


def stray_summary(outside):
    """The severity and MUST tokens found outside the findings, as one field, or None.

    Sorted and joined exactly as `sort -u | tr '\\n' '/'` joined them: both orders are by code
    point, because `sort` ran under `LC_ALL=C` too.

    BOTH SPELLINGS ARE SCANNED -- what the comment writes and what a decoder reads -- and the two
    are not the same text. Reading both is the safe direction for this scan, the same direction
    `re.ASCII` is chosen for above: every token it finds is a blocker that sends the review to a
    person, so one found in both spellings costs nothing and one found in neither is the defect.
    """
    tokens = sorted(set(STRAY_TOKEN.findall(outside))
                    | set(STRAY_TOKEN.findall(decoded_spelling(outside))))
    return "/".join(tokens) if tokens else None


# ---- the review ---------------------------------------------------------------------------------

# One fenced block of a comment: the character its fence is made of, the info string naming its
# language, its content with the opening fence's indentation removed, THE OFFSET ITS OPENING FENCE
# LINE STARTS AT, the raw span that content occupies in the comment, the offset just past the
# block, and whether it was ever closed. `outer` is what lets the whole block -- its fences as well
# as its content -- be taken out of the comment, so that what is left is exactly the material the
# structure scan did not account for.
Block = collections.namedtuple("Block", "fence info content outer start end after closed")


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


def without_indent(lines, indent):
    """LINES with up to INDENT leading spaces removed from each, as CommonMark strips them."""
    cut = []
    for line in lines:
        taken = 0
        while taken < indent and taken < len(line) and line[taken] == " ":
            taken += 1
        cut.append(line[taken:])
    return "\n".join(cut)


def fenced_blocks(text):
    """Every fenced code block of TEXT, in order, with EVERY LINE OF THE COMMENT ACCOUNTED FOR.

    One pass, left to right, holding one piece of state: the fence that is open. That is what makes
    this a structure rather than a search. A regex that jumps to the last opening fence has no
    opinion about the lines it flew over, so a fence it does not recognise is a fence that is not
    there -- and the block before it becomes "the last block". Here a line is either a fence or it
    is content of the block a fence opened, there is no third thing a line can be, and a fence
    inside an open block is content because that is where the scan has got to.

    Each block is reported as the character its fence is made of, its info string, its content with
    the opening fence's indentation removed, THE RAW SPAN THAT CONTENT OCCUPIES IN THE COMMENT --
    so what is outside the verdict can be taken out of the comment by position rather than by
    matching the block's text back against it, which removes every copy of it and not the one that
    was read -- the offset just past the block, and whether it was ever closed.
    """
    blocks = []
    fence_char = None
    fence_len = indent = 0
    info = ""
    content_from = 0
    outer = 0
    content_start = 0
    lines = text.split("\n")
    offset = 0
    for index, line in enumerate(lines):
        line_start = offset
        offset += len(line) + 1
        if fence_char is None:
            opening = FENCE_OPEN.match(line)
            if opening is None:
                continue
            if opening.group(2)[0] == "`" and "`" in opening.group(3):
                continue          # a backtick fence's info string may hold no backtick
            indent = len(opening.group(1))
            fence_char = opening.group(2)[0]
            fence_len = len(opening.group(2))
            info = opening.group(3).strip()
            content_from = index + 1
            outer = line_start
            content_start = line_start + len(line)
            continue
        closing = FENCE_CLOSE.match(line)
        if (closing is not None and closing.group(1)[0] == fence_char
                and len(closing.group(1)) >= fence_len):
            blocks.append(
                Block(fence_char, info, without_indent(lines[content_from:index], indent),
                      outer, content_start, line_start, offset, True)
            )
            fence_char = None
    if fence_char is not None:
        blocks.append(
            Block(fence_char, info, without_indent(lines[content_from:], indent),
                  outer, content_start, len(text), len(text), False)
        )
    return blocks


def outside_block(text, block):
    """TEXT with the verdict block's content taken out of it, for the stray-token scan.

    BY POSITION, not by matching the block's text back against the comment: `text.replace(block,
    "")` removes EVERY copy of that text and not the one that was read, and a block whose content
    was de-indented is no longer a substring of the comment at all, so a review that had its fence
    indented would have had its whole object scanned for stray severities. The span is narrowed the
    way `strip()` would have narrowed the text, so what stays behind is what stayed behind before.
    """
    start, end = block.start, block.end
    raw = text[start:end]
    lead = len(raw) - len(raw.lstrip())
    trail = len(raw) - len(raw.rstrip())
    return text[:start + lead] + text[end - trail:]


def names_json(info):
    """Whether an info string names the verdict's language: CommonMark's first word, folded."""
    words = info.split()
    return bool(words) and words[0].lower() == "json"


def spans_outside(text, blocks):
    """The spans of TEXT that no block the structure scan found covers -- FENCE LINES INCLUDED.

    A block's fence lines belong to the block, not to the comment around it, so they are taken out
    with its content. What is left is exactly the material that scan did not account for, and it is
    where everything below looks: a fence run there is a fence this program did not read as one.
    """
    spans = []
    pos = 0
    for one in blocks:
        if one.outer > pos:
            spans.append((pos, one.outer))
        pos = max(pos, one.after)
    spans.append((pos, len(text)))
    return spans


def unresolved_material(text, blocks):
    """Every piece of fence material the structure scan did not turn into a block of its own.

    THIS IS THE CHECK THAT HOLDS WHEN THE RECOGNISER IS WRONG AGAIN, and it is the whole of this
    program's answer to three rounds that each found one more spelling of a fenced block. One
    leading space; then `> ` in front of the fence, which is a fence inside a CommonMark blockquote
    and was no fence at all here; then the fence inside an HTML comment. Each time the recogniser
    was made to understand one more shape, and each time the next round found the next shape.

    So the recogniser is not asked to be right -- it is asked to ACCOUNT FOR EVERY FENCE RUN IN THE
    COMMENT. Three or more backticks or tildes is the only way a fenced block opens, so a run of
    them that the scan did not consume as a fence of some block means this program's idea of where
    the blocks are is not the reader's. There is no result from a comment like that.

    A tilde-fenced `json` block is material for the same reason from the other side: it is a block,
    because a ``` inside one would otherwise be read as a fence of the comment's own, but it is
    never the verdict -- the workflow writes backticks -- so a comment carrying one is a comment
    whose verdict this program cannot be sure it is reading.

    AND WHAT SUCH A BLOCK HOLDS IS MATERIAL, WHICH IS WHERE ACCOUNTING FOR EVERY RUN WAS NOT
    ENOUGH -- the hole `HTML-COMMENT-FENCE-SWALLOWS-VERDICT` and
    `PROSE-CANDIDATE-ZERO-PERMITS-PROSE` both came through. Accounting for every run says each one
    WAS consumed as a fence; it says nothing about WHICH BLOCK it was consumed into. CommonMark
    suppresses a fence inside a raw-HTML block and this scan does not, so `<!--`, a `` ```text ``
    line, `-->`, and then the real verdict, is a `text` block that SWALLOWS that verdict as its
    content -- every run accounted for, the swallowed object invisible to the candidate count, and a
    clean `PASS` appended after it the only candidate left. Measured: the audit reports READY, out
    of a comment a CommonMark parse sees exactly one fenced verdict in, and that one says
    CHANGES_REQUIRED with a P1. An unclosed `<!--` is not the only way; `<div>`, `<pre>` and any
    complete tag on a line of its own start HTML blocks too, so the shape is not a list of tags and
    is not answered by learning one more of them.

    So a block this program did not read the verdict FROM is not inert. Two things inside one are
    material, and they are the two shapes a verdict is ever read from anywhere else in this file:
    a line SHAPED LIKE THE OPENING FENCE of a `json` block, and the older bare form's OBJECT OPENER
    -- which is the whole of the second finding, whose `text`-fenced `role_understanding` object
    left the candidate count at zero and sent a self-contradictory comment to the prose parser. A
    comment whose `text` block holds either has no result -- which is the same sentence as the one
    above it, asked of the inside of a block rather than of the outside of one.
    """
    stale = []
    for start, end in spans_outside(text, blocks):
        for run in FENCE_RUN.finditer(text, start, end):
            stale.append("%s at line %d" % (run.group(0)[:8], text.count("\n", 0, run.start()) + 1))
    for one in blocks:
        if one.fence == "`" and names_json(one.info):
            # The verdict's own shape. What is inside THIS is read by `json.loads`, which is a
            # whole-document check no fence run can pass: a line-initial run inside a valid JSON
            # object would have to be inside a string, and a JSON string holds no newline.
            continue
        at = text.count("\n", 0, one.outer) + 1
        for run in CONTENT_FENCE_OPEN.finditer(one.content):
            if names_json(run.group(1)):
                stale.append("%s inside the block at line %d" % (run.group(0).strip()[:16], at))
        if BARE_OBJECT_OPEN.search(one.content) is not None:
            stale.append("a verdict object inside the block at line %d" % at)
        if one.fence != "`" and names_json(one.info):
            stale.append("%s%s at line %d" % (one.fence * 3, one.info[:16], at))
    return stale


def verdict_candidates(text, blocks):
    """Every place in TEXT a workflow verdict could be read from, in the order they appear.

    A candidate is a backtick-fenced block whose info string names `json` -- the shape the review
    workflow writes -- or the older bare form's object, which has no fence and runs from its opener
    to the last `}` in the comment. Both are looked for over the WHOLE comment and not from one end
    of it: the caller requires exactly one, so there is no last, no first, and no earlier block to
    fall back to. An opener inside a block the scan found is that block's content and not a
    candidate, which is why a workflow verdict object -- whose own first key is `role_understanding`
    -- is one candidate rather than two.
    """
    found = [one for one in blocks if one.fence == "`" and names_json(one.info)]
    close = text.rfind("}")
    for start, end in spans_outside(text, blocks):
        for opener in BARE_OBJECT_OPEN.finditer(text, start, end):
            at = opener.start()
            found.append(Block("`", "json", text[at:close + 1], at, at, close + 1, close + 1,
                               close >= at))
    found.sort(key=lambda one: one.outer)
    return found


def the_verdict_block(text, candidates):
    """The ONE block this comment's verdict is read from, or a refusal. There is no third answer.

    EXACTLY ONE, AND NOTHING LOOSE AROUND IT. Round 11 made a block that does not close a failed
    parse rather than a licence to take the block before it; round 12 found the same revival
    arriving through the recogniser, and closed it by taking the comment's LAST block; round 13
    found three more ways to make the last block the wrong one -- a bare object the tail check's
    enumeration missed, a real block hidden from the recogniser inside a blockquote, and a second
    block hidden from a reader inside an HTML comment.

    Taking the last block was still a search. This is not: the comment either holds one place a
    verdict could be read from, or it does not read. A second place is a refusal whether it is a
    quoted example, a blockquoted block or a hidden one -- this program does not have to tell them
    apart, and every attempt to tell them apart has been the defect.

    The two checks after the count are the same rule pointed outwards. Nothing but WHITESPACE may
    follow the block -- not "nothing this program recognises as a block", which is the enumeration
    that failed; and no `VERDICT:` may stand outside it, because a comment carrying a verdict object
    and a verdict line says two things and this program would be choosing between them.
    """
    if len(candidates) != 1:
        raise Unparsed(
            "the review carries %d places a verdict could be read from, and exactly one is read"
            % len(candidates)
        )
    block = candidates[0]
    if not block.closed:
        raise Unparsed("the review's verdict block does not close")
    tail = text[block.after:]
    if tail.strip():
        raise Unparsed(
            "the review carries [%s] after the block its verdict is read from"
            % tail.strip()[:24]
        )
    if PROSE_VERDICT.search(text[:block.outer]) is not None:
        raise Unparsed(
            "the review carries a VERDICT: line outside the block its verdict is read from"
        )
    return block


class RepeatedName(ValueError):
    """A JSON object named the same thing twice, so the document has two readings.

    A `ValueError`, because that is the vocabulary every other unreadable verdict object arrives
    in and the decode below has exactly two outcomes either way: a whole object, or a refusal.
    Subclassing it means a rewrite that stops naming this class by hand still cannot turn a
    repeated name into a result -- the `except ValueError` it falls into is a refusal too.
    """

    def __init__(self, name):
        # The name is REVIEW CONTENT and it is rendered as `repr` rather than as itself: a name
        # holding a newline would otherwise write a second diagnostic line of its own, which is
        # the in-band-marker shape this file exists to keep out of its own channels. `repr`
        # escapes every control character and every lone surrogate, and the length is bounded the
        # way every other quoted fragment here is bounded.
        super().__init__("names %.40r twice" % name)


def one_reading(pairs):
    """One JSON object built from PAIRS -- or a refusal, because A REPEATED NAME IS NOT A VALUE.

    `json.loads` KEEPS THE LAST OCCURRENCE OF A REPEATED NAME
    (https://docs.python.org/3/library/json.html#repeated-names-within-an-object), and that is a
    verdict this program would be choosing rather than reading. One fenced object with a valid
    head and base, `"verdict":"PASS"`, a `findings` array carrying a P1, and then a second
    `"findings":[]`, deserialised to a clean PASS with no findings AT ALL -- the blocking array
    discarded before validation ever saw it, and the stray scan silent because the object is the
    recognised verdict block and its severities are inside it. READY, and a merge call, out of a
    review that blocks. The same shape one level down erases a witness instead: a finding with
    `"witness":"a failing test"` and then `"witness":null` is a finding the audit stops holding
    in every lane.
    """
    seen = set()
    for name, _ in pairs:
        if name in seen:
            raise RepeatedName(name)
        seen.add(name)
    return dict(pairs)


def parse_json_review(text, candidates):
    """The workflow form: the comment's one verdict object, and it is the only source.

    Anything in the comment outside that object which looks like a finding is for a person, and is
    reported as a stray token rather than counted as a finding.
    """
    block = the_verdict_block(text, candidates)
    try:
        verdict = json.loads(block.content, object_pairs_hook=one_reading)
    except RepeatedName as exc:
        # AMBIGUITY REFUSES, and a repeated name is the ambiguity this file's own rule was written
        # about: two readers read two verdicts out of one document. The hook is the object's
        # CONSTRUCTOR, so the refusal is not a check run over a decoded object that could be
        # fooled about what the text said -- the object is never built, at any depth, and there is
        # no value for a later reader to disagree about.
        raise Unparsed("the review's verdict object %s, so it has two readings" % exc)
    except ValueError as exc:
        # THE ONE CANDIDATE IS THE VERDICT, and this one does not read as a whole object. Reaching
        # past it to another block is the defect; reporting it as a review with no findings is the
        # same defect wearing the other hat. There is no complete result here.
        raise Unparsed("the review's verdict object is not whole: %s" % exc)
    if not isinstance(verdict, dict) or not isinstance(verdict.get("findings"), list):
        # The comment carries a verdict block and it holds no verdict object this can read. That
        # is a complete description of the review -- it records nothing, and it carries one finding
        # the audit cannot judge -- so it is a result, and every part of it blocks.
        return {
            "kind": "json",
            "reviewed_sha": None,
            "verdict": None,
            "base_sha": None,
            "stray": stray_summary(text),
            "findings": [{"severity": "ERR", "id": "unparsed", "flags": 0}],
        }
    outside = outside_block(text, block)
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
    """review FILE: the one review this pull request is judged on, in whichever form it is in.

    THE ORDER HERE IS THE POINT. The comment's block structure is established first and checked
    against the comment's own characters; only then is a form chosen, and the form is chosen by the
    comment's marker rather than by which parser gets an answer. Every branch that cannot be
    resolved ends in a refusal, and none of them ends in the other parser:

      * material this program could not account for as a block -> no result. A fence run outside
        every block it found, and -- because accounting for every run says nothing about which
        block each one went into -- a `json` fence line or a bare verdict opener INSIDE a block it
        did not read the verdict from. The structure is not what it thinks it is, so nothing built
        on that structure can be trusted;
      * the prose form's marker AND a verdict block -> no result. The comment claims to be both
        forms, and they do not agree about the same review;
      * the prose form's marker and nothing else -> the prose parser, which is the form the comment
        says it is rather than the one left over;
      * no marker -> the workflow form, which must yield exactly one verdict block. ZERO IS A
        REFUSAL, not a reason to try prose: that fall back is how a real CHANGES_REQUIRED hidden
        from the recogniser -- one leading space, a `> ` before the fence -- became the
        `VERDICT: PASS` line written outside it.
    """
    if len(args) != 1:
        raise Unparsed("review takes one file")
    text = read_input(args[0])
    blocks = fenced_blocks(text)
    stale = unresolved_material(text, blocks)
    if stale:
        raise Unparsed(
            "the review carries material this parse could not account for as a block: [%s]"
            % stale[0]
        )
    candidates = verdict_candidates(text, blocks)
    if PROSE_MARKER.search(text) is not None:
        if candidates:
            raise Unparsed(
                "the review carries the prose form's marker and %d verdict block(s)"
                % len(candidates)
            )
        result = parse_prose_review(text)
    else:
        result = parse_json_review(text, candidates)
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


def write_all(stream, payload):
    """All of PAYLOAD through STREAM, or a failure. Never some of it and a return.

    `write()` RETURNS A BYTE COUNT, AND IT IS NOT ALWAYS THE WHOLE PAYLOAD. Under
    `PYTHONUNBUFFERED=1` `sys.stdout.buffer` is the raw file object, whose `write` does one
    `write(2)` and hands back what the kernel took: with the caller's `RLIMIT_FSIZE` at 1024 and a
    3,560-byte result, `write(1, ..., 3560) = 1024` -- and the count discarded, a flush of an
    unbuffered stream finishing nothing, the program exited 0 over a payload with 2,536 bytes
    missing. That is the seventh round's unchecked `printf` again: a short write is a failure that
    looks exactly like a success, and it truncates a findings list without shortening the count the
    list declares.

    So the count is read. Every byte is accounted for before this returns, and a write that stops
    making progress raises rather than reporting what it managed.
    """
    view = memoryview(payload)
    while view:
        written = stream.write(view)
        if written is None:
            raise OSError("the stream would not say how many bytes of the result it wrote")
        if written <= 0:
            raise OSError(
                "the result stopped being written with %d of %d bytes left"
                % (len(view), len(payload))
            )
        view = view[written:]


def write_result(payload, out):
    """The single write, confirmed before it counts as one.

    Every step here raises rather than returning a status, and the destination is renamed into
    place only once the bytes are on the device: a `write()` that returns EIO, a `flush()` that
    finds the device full, a `close()` that fails -- each ends the program non-zero with no
    destination file to read. That is the finding this program was written for, and it is closed
    by construction rather than by a check somebody has to remember. The one step that does NOT
    raise is a `write()` that took some of the payload and said so, which is why both writes go
    through `write_all` and neither is the bare `.write()` whose count nobody reads.

    THE STAGING FILE IS CREATED BY THIS INVOCATION AND BELONGS TO IT. `OUT + ".part"` is one
    pathname shared by every process writing to one destination, and two of them overlapping is not
    a theoretical shape: A flushes its blocking review and pauses in `fsync`, B truncates the same
    staging file and writes a clean `PASS` over it, A wakes and renames B's bytes into A's
    destination -- exit 0, publishing a verdict out of an input it never read. `mkstemp` creates
    with `O_EXCL` under a name nothing else holds, in the destination's own directory so the rename
    stays within one filesystem and stays atomic. Standards section 8: a unique staging path, never
    a fixed temporary name concurrent writers can collide on.
    """
    if out is None:
        write_all(sys.stdout.buffer, payload)
        sys.stdout.buffer.flush()
        # Closed explicitly: a buffered write that failed must raise HERE, where the exit status
        # is still being decided, and not in an interpreter shutdown handler after this function
        # has already reported success.
        sys.stdout.close()
        return
    handle_fd, part = tempfile.mkstemp(
        dir=os.path.dirname(out) or os.curdir, prefix=os.path.basename(out) + ".", suffix=".part"
    )
    try:
        with os.fdopen(handle_fd, "wb") as handle:
            write_all(handle, payload)
            handle.flush()
            os.fsync(handle.fileno())
        # `mkstemp` creates 0600, and the destination is what a plain create would have left there.
        # Making the staging path unique is not also a licence to change what the caller reads.
        mask = os.umask(0)
        os.umask(mask)
        os.chmod(part, 0o666 & ~mask)
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
