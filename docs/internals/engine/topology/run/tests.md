# `src/engine/topology/run/tests.rs`

Repository source for these notes: [`src/engine/topology/run/tests.rs`](../../../../../src/engine/topology/run/tests.rs).
[Source on GitHub](https://github.com/sourcemaps/upstroke/blob/master/src/engine/topology/run/tests.rs).
The relative link works in a checkout or on GitHub; the GitHub link also works from the published site.

The code is the authority for what it does. The explanatory prose is preserved below.
Each backticked part of a section heading is an exact source excerpt. Search for the final
excerpt within the preceding item when a heading names both an item and a line inside it.

## Module

The loop's branches, checked against the packet's list rather than against
the implementation.

## `fn the_transcribed_loop_branches_are_the_packets_seven() {`

The transcribed list is the packet's list — seven branches, these labels, in
this order.

`decisions.sequential_substrate.loop` names them in one sentence, split on
`->`. A branch dropped from [`LoopBranch::ALL`] would make every other test
in this file pass by asking for less, which is exactly how step (g) survived
two review rounds in `recover.rs`.

## `fn every_branch_states_what_this_build_does_with_it() {`

Every branch this build does not perform says which, and why, in the type.

**The point of this test is the third disposition.** `RefusedByCheckpoint`
is a decision the packet licenses; `NotYetImplemented` is debt. A build that
conflated them would be indistinguishable from one that had quietly dropped
a branch — and "quietly dropped a branch" is the defect this whole module
exists because of.

## `fn every_branch_states_what_this_build_does_with_it()` › `let owed: Vec<&str> = LoopBranch::ALL`

And the debt, named rather than implied. This assertion is expected to
shrink as branches land; it must never grow.

## `fn every_branch_states_what_this_build_does_with_it()` › `let elsewhere: Vec<&str> = LoopBranch::ALL`

**What is another slice's, cited rather than owed — and today nothing
is.** `ingest answers` carried the contract passage that assigned it to
PR9 for as long as it was PR9's; PR9 performs it, so the set is asserted
empty. A branch that returns to another slice must come back with the
passage that assigns it, not as debt and not as a checkpoint refusal,
of which the packet authorises exactly one (run end).

## `fn every_branch_states_what_this_build_does_with_it()` › `assert_eq!(`

The half-built one, and both halves in the branch's own words. A branch
that performs a durable append and reports `NotYetImplemented` would be
claiming the log is untouched when it is not; one that reported
`Performed` would be claiming an attempt ran.

## `fn every_step_belongs_to_one_branch_or_to_none_for_a_reason() {`

Every `Step` a selection can produce maps to exactly one branch, or to none
for a stated reason.

The mapping is total by construction — `LoopBranch::of` matches on `Step`
exhaustively, so a new variant does not compile until someone decides which
branch it belongs to. What this test adds is the *two `None` arms*, which a
compiler cannot check: they are the claim that neither is a branch of the
loop, and each is wrong in a different and specific way if the claim slips.

## `fn a_refusal_names_the_branch_and_says_whether_anything_happened() {`

A refusal says which branch, and — this is the part that matters — whether
anything happened.

**The two messages must not be interchangeable.** A branch that performed
nothing says so, and an operator reading it knows the log is untouched. A
branch that appended and then stopped says what it did, because an operator
told "not implemented" after a durable `task_dispatched` would go looking
for a run directory that does not match the message.

## `fn a_refusal_names_the_branch_and_says_whether_anything_happened() {` › `assert!(`

**No branch is `PartlyImplemented` today**, and that is a statement about
this build rather than about the type. `ReadyRetry` was the last one and
became `Performed` when its second half landed. The variant stays because
the next branch built in halves will need it, and this assertion is what
says so out loud the moment one appears — a half-built branch is the one
shape whose refusal has to say what it already did, because by then
`attempt_started` or `task_dispatched` is durable.

## `fn a_refusal_names_the_branch_and_says_whether_anything_happened() {` › `for branch in LoopBranch::ALL {`

Every refusal names its own branch, whatever its disposition. A message
that named the wrong one would send an operator to the wrong lane.

## `fn blanked_bytes(source: &str, code: &str) -> usize {`

The bytes [`crate::effects::production_code`] blanked out of `source`.

The blanker is position-preserving, so the count is also the number of
source positions the region no longer offers a needle. A scan is only
meaningful over a region where this is non-zero: a comment or a literal left
standing is text a substring search reads as production code.

It panics when the region is not its source's length. That is the other half
of the same contract — an offset into a region that has changed length names
a different line of the file the census is reporting on.

## `fn assert_blanked_region(file: &str, source: &str, code: &str, retained_floor: usize) {`

The region guard the source censuses in this file share, and what replaced
the ratio each of them used to open with.

Each opened with `code.len() * n > source.len()`. That guard was written for
a **truncating** region, where a short result really does mean "a census over
a fraction of a file reports zero for the part it never read".
[`crate::effects::production_code`] does not truncate — it overwrites
comments, literals and `#[cfg(test)]` items with spaces and keeps every
newline — so `code.len() == source.len()` whatever it removed, and the ratio
was already true before the blanker ran. It could not tell a working blanker
from one that had stopped removing anything, and over unblanked source a
needle quoted in a doc comment is counted as a call site.

So the two halves are asserted apart: something was blanked, and enough was
left to scan. `CODING_STANDARDS.md` §12 requires the first of every scan and
the length contract of the blanker;
[`the_blanked_region_count_falls_to_zero_when_nothing_was_removable`] is the
control that this number can reach zero, which is what the ratio could not.

`retained_floor` is each census's own tolerance, carried across unchanged:
the unblanked remainder must exceed one `retained_floor`th of the file.

It panics when the region changed length, blanked nothing, or retained less
than its floor.

## `fn the_blanked_region_count_falls_to_zero_when_nothing_was_removable() {`

The blanked-region count reaches zero, which is what the ratio it replaced
could not.

[`assert_blanked_region`] is the guard three source censuses in this file
open with, and a guard that cannot fail is not a guard. These two fixtures
are the control in both directions: one carries a removable region of each
kind the region function knows, the other carries none, and the retired
ratio is asserted here to be satisfied by the second.

## `fn every_driver_append_propagates_its_error() {`

**Every append the driver makes propagates its error.**

The append-error protocol is five obligations, and all five begin with the
error *reaching* the protocol. A `let _ = self.emit(..)` reaches none of
them: the fold is not poisoned, no reservation or invocation is cancelled,
and the command reports success for a run whose log does not contain the
line it just claimed to write.

Catalogue entry `PR7-SELECT-026` did exactly that to the
`Admitted::BudgetExceeded` arm and the whole suite stayed green, because the
arms whose append failure *is* armed by a fixture are not that one.

A **census rather than a fixture per arm**, for the reason the other four
single-authority censuses exist: a per-arm test proves the arm it names and
says nothing about the arm added next week. This proves the property over
every append site the driver has, including the ones not yet written.

The region is [`crate::effects::production_code`], which blanks comments and
strings — a `let _ = self.emit(` quoted in a doc comment must not fail this,
and a truncating region would let a site below the cut through, which is
`PR4-CENSUS-COMMENT-ORACLE` and is how the barrier census scanned 4.7% of
this very file.

The guard on that region is [`assert_blanked_region`], which counts what was
blanked. The length ratio this test used to open with could not: the region
function preserves length by contract, so the ratio held of a blanker that
had removed nothing and left every quoted `self.emit(` as a call site.

## `fn every_driver_append_propagates_its_error()` › `let mut depth = 0_i32;`

Walk to the matching close paren, then check what follows it.

## `fn the_loop_selects_through_one_function() {`

**The loop chooses its branch through one selector.**

`decisions.sequential_substrate.loop` gives seven branches in one order, and
`select` is where that order lives. Catalogue entry `PR7-SELECT-015` added a
**second** selector — `select_rescan`, ordered Dispatch/Retry/Integrate
instead of Integrate/Retry/Dispatch — pointed `TopologyRun::step` at it, and
left canonical `select` untouched with every one of its tests still passing.
The whole suite was green.

That is the seams category in its purest form: `select.rs` is coherent,
`run.rs` is coherent, and the branch order the packet specifies is not the
one the run takes. No per-function test can see it, because each function is
right about itself.

The fifth single-authority census this slice owns, and the cheapest: the
driver reaches its branch order through exactly one call, and `checkpoint`
guards exactly that call's result. A second selector makes this count zero,
not two — which is why the assertion is on the **canonical** name rather than
on a total.

The region carries [`assert_blanked_region`] for the reason the append census
above does: the ratio both used to open with is true of a region that blanked
nothing, and a `select(` in a doc comment would then be counted as the call.

## `fn the_loop_selects_through_one_function()` › `let calls = |needle: &str| {`

Calls, not definitions — neither is defined here, but the filter is the
one the barrier census learned to use and costs nothing.

## `fn the_frozen_pool_table_is_read_through_one_seam() {`

**The frozen pool table is read through one seam.**

`AttemptPlans::pool_for` exists so that the plan builder, the reviewer
profile and the driver's `RetryRequest` reach one answer. `79cd9c8` said it
gave the rule "one production implementation" and it did not: `assembly.rs`
called `crate::capacity::pool_for` from three places, two of them
character-for-character copies of the seam's body, and the seam's only caller
was `run.rs`. `reviews/FINDINGS.md` §19, claim (4).

**The needle is a free call to `pool_for`**, through the shared
[`crate::effects::census_domain::production_calls`]. It was the literal
`capacity::pool_for(`, which reasons about one direction only — a longer
identifier colliding with it — and not about the other: `use
crate::capacity::pool_for;` followed by a bare `pool_for(...)` is the
ordinary way to write a second implementation and that literal does not
match it. Both spellings are already live in this tree. `R5-SEAMS-002`.

**What it still cannot see, stated rather than left to be found**: a second
resolution that never names the function. `capacity::pool_for` is
`pools.iter().find(…)`, and a caller walking `self.pools` inline is a second
implementation of the rule with no `pool_for` in it. A name census cannot
reach that, so what this asserts is **one named resolution**, not one
resolution.

**The count is one and not zero.** Zero would mean the seam had been rewritten
to resolve pools some other way, which is the same defect from the other
side, so the assertion is an equality.

The needle controls at the end are controls on *identifier matching*; the
premise underneath them — that the region was blanked at all — is
[`assert_blanked_region`]'s, because the ratio this census used to open with
held of a blanker that had removed nothing.

## `fn the_frozen_pool_table_is_read_through_one_seam()` › `use crate::effects::census_domain::{Call, production_calls};`

**Free calls to `pool_for`, not the qualified spelling.** The needle was
the literal `capacity::pool_for(`, which does not match the ordinary way
to write a second implementation — `use crate::capacity::pool_for;` and
then a bare `pool_for(...)`. Both idioms are live in this tree
(`config.rs` writes the qualified form, `capacity.rs` the bare one), so
it is not a hypothetical spelling. `R5-SEAMS-002`, `PR7-R5-ATT-002`.

`Call::Free` is what separates a second implementation from the seam's
own callers: the plan builder and the reviewer profile ask
`self.pool_for(...)`, a method call, and the trait method's definition is
filtered as a definition.

## `fn the_frozen_pool_table_is_read_through_one_seam()` › `assert_eq!(`

Controls on the needle itself, both directions, because a needle that has
stopped matching reads exactly like a clean file.

## `struct AttemptStartedSite {`

A production `AttemptStarted4` struct expression: the line it opens on, and
the expression its top-level `pool` field is initialised with.

## `fn is_name_char(ch: char) -> bool {`

A character an identifier may be spelled with, and therefore one that must
not be touching a name for the match to be that name.

## `fn opens_a_struct_expression(before: &str) -> bool {`

Whether the text ending where an `AttemptStarted4` begins opens a **struct
expression**, rather than a declaration or a return type.

The path this name may be the last segment of is skipped first:
`events::AttemptStarted4 { … }` is the same expression as a bare one, and the
keyword that decides the context sits before the whole path rather than
before its last segment.

What remains is read for the forms that are certainly **not** an expression —
a return type, and the item headers that introduce a name followed by a brace
of their own. `fn build() -> AttemptStarted4 {` is the one this census was
measured to mis-read: the exact-byte needle it used counted a function's
signature as a construction, and then failed looking for a `pool` field in a
function body.

Everything else is read as an expression or a pattern. That is the safe
direction, and the one [`crate::effects::production_code`] argues for about
its own region: a domain that is too large makes the census report more,
never less. A struct *pattern* naming `pool: None` is reported rather than
skipped, which is a decision someone is asked to make rather than one the
instrument makes silently.

## `fn matching_delimiter(code: &str, open: usize) -> Option<usize> {`

The offset of the delimiter closing the one opened at `open`, or `None` when
what lies between them does not nest.

`{`, `(` and `[` are all tracked, and a closer that does not match its opener
ends the walk without an answer. Counting braces alone cannot tell a body
that ends from one whose delimiters cross, and the second is a region the
scanner has lost rather than one it has read.

## `fn top_level_field(body: &str, name: &str) -> Option<String> {`

The expression the field `name` is initialised with at the **top level** of a
struct expression's `body`, or `None` when it has no such field.

Never a field of the same name inside a nested literal. The body is split on
its own commas — the ones outside every nested `{}`, `()` and `[]` — because
the line-oriented rule this replaces read
`binding: Binding {\n    pool: None,\n}` as this literal's own `pool` and
reported a value the event never carried.

## `fn top_level_field(body: &str, name: &str) -> Option<String> {` › `match rest.strip_prefix(':') {`

The two forms a field may take, and the path that is neither. `pool:
<expression>` is the initialised field; the shorthand `pool` names the
binding of that name; and a `pool::…` is a path rather than this field, which
is why the first arm refuses a second colon.

## `fn expression_tokens(text: &str) -> Vec<String> {`

`text` as the sequence of tokens it is written from: a run of identifier
characters is one token, and every other non-whitespace character is its own.

Formatting is not part of the authority a site names — `plan.pool.clone()`
and the same expression broken across lines are the same expression — and
tokenising rather than stripping whitespace is what keeps `mut pool` and
`mutpool` apart while doing it.

## `fn is_the_declared_authority(found: &str, expected: &str) -> bool {`

Whether `found` is the `expected` authority expression.

**An allowlist of one, not a denylist of spellings.** The oracle this
replaces asked whether the value began with `None`, which is a denylist with
two holes in it and both are reachable. It admitted every other way of
writing absence — `Option::None`, `None::<String>`, `Default::default()`,
`<_>::default()` — as an authority, and it called any authority whose *name*
began with `None` an invention. Naming the expression each site is supposed
to carry closes both at once: there is nothing to enumerate, and a name is
only ever read as a name.

## `fn attempt_started_sites(code: &str) -> Vec<AttemptStartedSite> {`

Every production `AttemptStarted4` struct expression in `code`.

`code` is a blanked region, so a brace inside a comment or a string literal
is already a space and can neither open a body nor close one — and a comment
*between* the name and its brace is whitespace for the same reason, because
the blanker preserves position. `AttemptStarted4 /* the retry arm */ {` is
one of this type's spellings and the exact-byte needle this replaces did not
see it, which put a whole construction site outside the domain.

The three questions are asked apart: is the match this type's name and not
part of a longer one; is a brace what follows it across whitespace; and is
the context an expression rather than a declaration or a return type.

It panics when a literal's delimiters do not nest, or when one carries no
top-level `pool` field. Both are the census losing its subject, which is not
the same answer as finding it clean.

## `fn the_attempt_started_scanner_reads_expressions_and_not_return_types() {`

The scanner reads struct expressions, and reads return types and
declarations as neither.

[`attempt_started_sites`] is the domain of
[`both_attempt_started_arms_take_their_pool_from_an_authority`], and a domain
derived by an exact-byte needle is a domain that both misses members and
invents them. Each fixture here is one of the two directions, measured on the
needle this replaces.

## `fn the_attempt_started_scanner_reads_expressions_and_not_return_types()` › `const COMMENT_SEPARATED`

**Missed.** A comment between the name and its brace is legal Rust and a
needle of `AttemptStarted4 {` does not match it. Blanked in place it is
whitespace, so the scan is over `production_code`'s region rather than the
raw fixture — the comment must really have been blanked for the gap to be
whitespace at all.

## `fn the_attempt_started_scanner_reads_expressions_and_not_return_types()` › `const NOT_CONSTRUCTIONS`

**Invented.** A return type, the type's own declaration, an inherent
`impl` and a trait `impl` all put this name in front of a brace, and none
of them constructs anything. The one expression nested inside them is what
the scan is for, and finding it is the half that proves the rejections are
not just a scan that stopped early.

## `fn the_attempt_started_scanner_reads_expressions_and_not_return_types()` › `const LONGER_NAMES`

**The name, not a name it is inside of.** Both directions, because the
boundary is two checks and one of them passing reads exactly like both.

## `fn the_attempt_started_scanner_reads_expressions_and_not_return_types()` › `const NESTED_FIRST`

**The outer field, not a nested one of the same name.** The rule this
replaces took the first line whose trimmed text began `pool:`, so a nested
literal spelled across lines supplied the answer. Both orders, because the
defect is only visible in one of them.

## `fn the_pool_authority_oracle_names_the_expression_rather_than_absence() {`

The authority oracle names the expression a site is supposed to carry,
rather than spelling out the ways a value can be absent.

[`is_the_declared_authority`] is what
[`both_attempt_started_arms_take_their_pool_from_an_authority`] judges each
site with. The rule it replaces — "the value begins with `None`" — is a
denylist, and the two holes below are both reachable in ordinary Rust.

## `fn the_pool_authority_oracle_names_the_expression_rather_than_absence()` › `for invention in [`

The first hole: every other way to write "no pool", none of which begins
with `None` except the one that does.

## `fn the_pool_authority_oracle_names_the_expression_rather_than_absence()` › `"NonePool::resolve(agent)"`

The second hole, in the other direction: a name is a name, and one that
begins with `None` is not an absence.

## `fn the_pool_authority_oracle_names_the_expression_rather_than_absence()` › `"mut pool", "mutpool"`

Formatting is not the expression — `cargo fmt` breaking a line must not move
a site out of conformance — but whitespace between tokens is not nothing,
which is what a rule that simply stripped it would have made it. The wrapped
authority above and this pair are the two directions of that. The conforming
case closes the test, so that a green result here is a claim about an oracle
that accepts something.

## `fn both_attempt_started_arms_take_their_pool_from_an_authority() {`

**Both arms of `attempt_started` get their pool from an authority.**

`attempt_started` is appended from two places and they reach it differently:
the dispatch arm builds its plan first and reads `plan.pool`; the retry arm
appends **before** its plan exists, because `settle::retry` produces the
event and the plan is built after. Sol's `R3-SEAMS-001` is what that
asymmetry produced — the retry passed `pool: None`, so a resumed run's ledger
recorded no pool while the plan it then built resolved one, and the two
disagreed about the same attempt.

**Each site names the expression it is supposed to carry**, and
[`is_the_declared_authority`] compares against that rather than against a
list of ways to write absence. The rule this replaces asked whether the value
began with `None`: it admitted `Option::None` and `Default::default()` as
authorities, and it called an authority whose name began with `None` an
invention. A census's claim is only as narrow as its oracle, and "not
invented here" was never what that oracle asked.

**The domain is one struct expression per site, and that count is asserted.**
[`attempt_started_sites`] reads the type's name in expression context rather
than the bytes `AttemptStarted4 {`, because that needle both missed
constructions — a comment between the name and its brace — and invented them
— `-> AttemptStarted4 {`. The scan this all replaces read the *first* literal
in each file and stopped, so a second construction site lay outside the
scanned domain while `checked == SITES.len()` still read as full coverage.
The control at the end of the test is that second-position violation, written
in both of the spellings the byte needle could not reach.

### Two corrections to what this test was said to be

**It is not the only witness available, and the claim that it was is false.**
`79cd9c8`'s message argued a source census was structurally necessary because
"a retry is only reachable *within* one process … and **no driver fixture can
reach the arm**". One does: the fixture is
`recover::tests::the_retaining_incarnation_retries_in_place`, and it exists —
**named, not cited by line**. The first draft of this block quoted
`recover/tests.rs:5488` as terminal output — correct **at `c01a844`** — and the
very next commit inserted nineteen lines above it. `PR7-R6-ATT-003`, and
the rule it gives: a doc comment names an item, because a line number is a
claim about a version of a file and decays silently. The doc-comment filter
(`| grep -v '///'`) is the other half — a needle quoted here would otherwise
match its own quotation, `reviews/FINDINGS.md` §4.

It drives `TopologyRun::step` twice in one process and the second iteration
**is** the retained-generation retry. It now asserts the pool on both
`attempt_started` appends, which is the behavioural witness this census was
offered in place of. `reviews/FINDINGS.md` §19, claim (3).

**And this census does not read the file the defect was in.** The two sites
below are `attempt.rs` and `settle.rs`; the literal `None` that
`R3-SEAMS-001` found was in **`run.rs`**, which fills `settle::retry`'s
`RetryRequest`, and `settle.rs`'s own literal reads `request.pool` and was
correct throughout. Measured at `5a08f19`: restoring `pool: None` in
`run.rs` leaves this census green **and the entire suite green** — 1698 + 8
passed, 0 failed. The behavioural assertion above is what kills it. §19,
claim (2).

So this census keeps a real and narrower job: the two *literals* name the
authority each is supposed to name. It is not a witness that the value
arriving at them is right.

### The control at the end of the test

**Written the three ways the rules this replaces could not read.** The second
construction site is past the one `.find` stopped at; it is spelled with a
comment between the name and its brace, which the byte needle did not match;
and its pool is `Option::default()`, which the `None` prefix test admitted as
an authority. `CODING_STANDARDS.md` §12: a positive control inside a
truncated domain does not prove that the whole named domain was scanned.

## `fn the_settled_notes_separate_the_successful_and_the_failed_settlement() {`

`PR160-NOTES-SUCCESS-SETTLEMENT`. The `Progress::Settled` section of
`docs/internals/engine/topology/run.md` summarised the whole ready-dispatch
branch as ending in `attempt_finished`. That is only the rejected half.
[`TopologyRun::settle`] opens with `let Some(failure) = judgement.failure ...
else`, so an attempt nothing rejected takes [`TopologyRun::promote_candidate`]
before any failure settlement is built, and that path appends
`candidate_prepared` then `task_candidate_created` and no `attempt_finished`
at all. The fold enforces the same rule — `check_attempt_finished` refuses
`SettlementTransition::Succeeded` outright — and so do
`design/15_design_event_log_resume_run_layout.md` and
`design/26_design_merge_queue_protocol.md` §26.

So a reader using that contract to reconstruct a successful attempt's durable
record was sent looking for an event that is never written. This pins the two
settlements separately, and refuses the retired sentence by name so the claim
cannot come back under a reflow.

**It is a text pin and only a text pin.** The behaviour it describes is held
elsewhere — `recover::tests::the_driver_carries_an_accepted_attempt_through_the_candidate_sequence`
for the successful durable sequence, and the fold's
`candidate_prepared_is_the_sole_successful_settlement` for the settlement
contract. This one asserts that the prose agrees with them; `src/export.rs` and
`agent/proc/tests.rs` pin the sentences they own the same way.

## `fn the_settled_notes_separate_the_successful_and_the_failed_settlement()` › `let settled = settled.split_whitespace().collect::<Vec<_>>().join(" ");`

Match on the prose, not on where its line breaks fall: a reflow must not break
the pin, only a changed claim.

## `fn the_ready_branch_notes_do_not_owe_the_attempt_the_branch_runs() {`

`PR160-NOTES-INCOMPLETE-BRANCHES`. Both ready branches' sections of
`docs/internals/engine/topology/run.md` opened with the intermediate build's
paragraph and closed with the current one, so each described the branch as
half-built and then, two sentences later, as whole. The ready-dispatch
section said the first three of its four clauses were performed and the run
was left at `OpenNoAttempt`; the ready-retry section said running and
settling the retry was "the half still owed". Neither has been true since
`0eaf6b07` and `59683dc3`: [`TopologyRun::step`] calls
[`TopologyRun::attempt`] then [`TopologyRun::settle`] before returning
`Progress::Settled`, and [`TopologyRun::retry_ready`] reaches the same two on
`RetryOutcome::Start`. Both commits added the corrected paragraph and left
the one it superseded standing, and the migration into these notes carried
both across.

A reader taking either section as the contract would look for a state the
branch does not stop in, and — the sharper cost — would conclude the driver
does not settle, which is the one thing `decisions.sequential_substrate`
requires of it.

**It is a text pin and only a text pin.** The behaviour is held elsewhere:
`recover::tests::the_driver_takes_over_from_the_recovery_order_and_steps` is
the dispatch branch running an attempt through to `Progress::Settled`, and
`recover::tests::the_retaining_incarnation_retries_in_place` is the retry
branch doing the same over two iterations of the loop. This asserts that the
prose agrees with them, and refuses each retired sentence by name so the
claim cannot come back under a reflow — the same shape as
`the_settled_notes_separate_the_successful_and_the_failed_settlement` and as
the sentence pins in `src/export.rs`.

The `PartlyImplemented` section is pinned with them because it carried the
third instance of the same class: it gave the ready-dispatch branch as a
*present* example of an honestly half-built branch, while
`a_refusal_names_the_branch_and_says_whether_anything_happened` two hundred
lines above asserts that no branch is `PartlyImplemented` at all.

Measured over the notes as this commit leaves them: six mutations, one per
pin — each retired sentence restored, each stated proposition removed — and
all six killed, against an unmutated control that passes.

## `fn the_ready_branch_notes_do_not_owe_the_attempt_the_branch_runs()` › `assert_eq!(`

**The pins are conditioned on the code, not asserted beside it.** A section
is only required to describe a whole branch while its arm reads
`Disposition::Performed`; a branch that became half-built again would need
the opposite prose, and this says so at the point where the two claims
diverge rather than leaving a stale pin to fail with a message about
Markdown.
