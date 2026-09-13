//! Committed-or-Husk, or neither: the first-line probe `startup_census`
//! classifies by.
//!
//! `sequential_substrate.startup_census`: "every entry is classified by
//! `rundir::classify_run_dir` as **Committed** (`events.jsonl` exists and its
//! first newline-terminated line is a valid `run_started`) or **Husk**
//! (anything else)". Read-only, and bounded rather than total: the census
//! holds the physical worktree lock across it, so an entry that never
//! classifies is a lock held for ever, and every bound here exists for that.
//!
//! **A bound has to answer something, and that is why there is a third
//! classification** ([`RunDirClass::Indeterminate`], `SWEEP-CLASSIFY-001`). The
//! packet's two answers are what an observation that *finished* gives; an
//! interrupted read that ran out of [`INTERRUPTED_ALLOWANCE`] finished nothing,
//! and `Husk` — the answer a two-valued bound is forced into — is the
//! reclaiming one. Every bound in this file now answers the retaining one
//! instead. What that costs is stated on the variant: a directory retained and
//! hidden from `list_runs`, rather than a hang, and rather than a deletion.
//!
//! **What `startup_census` is, since this module cites it throughout.** It is a
//! sentence of the retired `sequential_substrate` packet. Neither `DESIGN.md`
//! nor any `design/` section states it at this SHA — measured, not assumed — so
//! every quotation of it in this file is the wording this module was built to
//! and this module's own reasoning for behaving that way, and none of it is
//! design authority. A citation below that reads as though the design required
//! something should be read as: this is what the module does, and this is why.
//! That the design carries no classification rule for a run directory is a gap
//! rather than a licence to invent one here; it is `SWEEP-CLASSIFY-013`, and
//! closing it is an owner-level design change rather than this pull request's.
//!
//! What is not bounded is named where it lives, and it is one syscall.
//! [`first_committed_line`] refuses to open anything whose *name* is not a
//! regular file, and the window between that check and the `open` is still
//! open, so a path swapped inside it for a writer-less fifo blocks in the
//! kernel before any bound on the read can apply.
//!
//! **Measured rather than restated, and then decided.** The block is real
//! (`SWEEP-CLASSIFY-003`, with the reproduction and its output in
//! `findings/`). The usual close on Unix — open with `O_NONBLOCK` and
//! take the file type from the descriptor — reaches a governed primitive:
//! `clippy.toml` denies `std::fs::File::options` and the `std::fs::OpenOptions`
//! it hands out, and `libc::open` and `libc::fcntl` beside them.
//!
//! **That is not a bar to writing it here, and an earlier version of this
//! paragraph wrongly said it was.** `standards/02` admits a per-site
//! `#[expect]` of a governed lint below module level in a file whose
//! `effects/allowlist.toml` row records the lint and the exact annotation
//! count, and `src/effects/tests.rs`'s placement census requires that file to
//! **deny** the lint at module level — so this module's deny posture is the
//! mechanism's precondition rather than the thing an allowance would replace.
//! The close is available locally, at the cost of an allowlist row.
//!
//! It is deferred anyway, as a **preference and not a constraint**: a governed
//! primitive belongs in the funnel parent as a site-taking non-blocking
//! read-only open, and the round that would have added it here is the round a
//! frontier pass found a P1 inside the machinery this file's previous round
//! added — which is the standing signal to narrow rather than to reach for one
//! more mechanism. `SWEEP-CLASSIFY-003` carries that reasoning and the
//! measurement it rests on.

// **This child states its own lint level and inherits nothing.** A Rust lint
// level is scoped by the module tree and not by the file, so an out-of-line
// child of `src/rundir.rs` would otherwise inherit that file's inner
// `#![allow(clippy::disallowed_methods, disallowed_types, disallowed_macros)]`
// -- `PR6-LANEF-004`, measured twice in the Container subtree and made again,
// independently, by two W1 pull requests. Nothing here reaches a governed
// primitive, so all three are DENIED rather than allowed, and this module takes
// no `effects/allowlist.toml` row: an allowance is what that file records, and
// this module takes none.
//
// **Measured, not believed.** A probe of three lines -- a `std::fs::write`, a
// `std::process::Command` and a `println!` -- is refused three times here, once
// per lint, with this attribute cited as the level; the identical three lines in
// `src/rundir.rs` emit no `disallowed_*` at all, under that file's own allow. So
// the deny is load-bearing rather than a restatement of an ambient rule.
#![deny(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::disallowed_macros
)]

use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::EVENT_LOG;

/// What a directory under `<repo>/.upstroke/runs` is.
///
/// `sequential_substrate.startup_census`: "every entry is classified by
/// `rundir::classify_run_dir` as **Committed** (`events.jsonl` exists and its
/// first newline-terminated line is a valid `run_started`) or **Husk**
/// (anything else)".
///
/// **[`Self::Indeterminate`] is not a third reading of that sentence; it is the
/// refusal to give either of its two** (`SWEEP-CLASSIFY-001`). The probe bounds
/// what it will spend on a source that answers `Interrupted`, and a bound has
/// to answer something. Neither of the packet's answers is true of an
/// observation that did not finish, and `Husk` is the *reclaiming* one -- so
/// answering it would turn a run the source would have delivered into one
/// `list_runs` hides, `resolve_run_id` refuses and the census may delete. This
/// variant is what the bound answers instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunDirClass {
    /// A run exists here and is resumable.
    Committed,
    /// No committed `run_started`. Nothing about a marker changes this, in
    /// either direction.
    Husk,
    /// The probe could not complete its observation of `events.jsonl`, so
    /// nothing here is known -- neither that a run committed nor that none
    /// did.
    ///
    /// **Retaining in every reader this repository has**: `list_runs` does not
    /// return it, so nothing resumes it; `list_husks` does not return it, so
    /// nothing offers it for reclaim; and `census_run_dirs` retains the
    /// directory with `RetainReason::ClassificationIncomplete` and deletes
    /// nothing. Those three are the readers and the one writer measured, by
    /// name, and no claim is made here about a reader added later.
    ///
    /// **It is reachable in production and not from a fixture, and the
    /// difference is the signal.** `EINTR` on a read is what a signal delivered
    /// to the reading thread does, and the census reads `events.jsonl` in a
    /// process that spawns agents and reaps them; that is the shape
    /// `SWEEP-CLASSIFY-001` is about. No test in this suite arranges one: a
    /// fixture's `events.jsonl` is an ordinary regular file served out of the
    /// page cache with no handler installed, so its reads deliver bytes or end.
    /// That is why every driver for this variant in this repository supplies a
    /// `Read` fixture or the classification itself rather than a path, and why
    /// the arms that produce it are split into functions a value can reach:
    /// see [`class_of`] and [`first_line_within`].
    Indeterminate,
}

/// What an attempt to observe part of a run directory established.
///
/// Three answers where the probe had two. `Option` holds "found it" and "did
/// not", so an observation that *could not be completed* had nowhere to go but
/// the second -- and the second is the reclaiming one. That fold is the whole
/// cost of `SWEEP-CLASSIFY-001`, and [`Self::Incomplete`] is the answer every
/// bound in this module gives when it runs out.
///
/// Deliberately not `Option<T>` and deliberately not `Result<Option<T>, E>`:
/// these are three *answers*, none of them an error a caller reports, and a
/// caller matching on this cannot collapse the third into the second without
/// writing the arm that does it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Observed<T> {
    /// The observation completed, and this is what it found.
    Found(T),
    /// The observation completed, and there is nothing here.
    Absent,
    /// The observation could not be completed, so neither of the others is
    /// known.
    Incomplete,
}

/// How much of `events.jsonl` the first-line probe reads in one go.
///
/// **A performance constant, not a classification bound.** A `run_started` line
/// records the plan path, the gate commands and the runner policy — kilobytes,
/// not megabytes — so a megabyte reaches the newline in a single read for every
/// log this project has ever written. A longer first line is still a first
/// line: `startup_census` defines `Committed` as "`events.jsonl` exists and its
/// first newline-terminated line is a valid `run_started`" and states no size
/// exception, so [`first_line`] falls back to a scan rather than answering
/// `Husk`. Changing this value changes how many syscalls the census makes and
/// nothing else — `classification_does_not_depend_on_the_probe_window` is the
/// assertion, and it is why the name no longer says "cap".
///
/// It exists at all because this runs once per directory in a census: reading
/// the whole file to look for a newline that is not there is the one shape that
/// must stay cheap, and [`newline_offset_from`] handles it in a fixed-size
/// buffer that never grows.
///
/// (`PR5-CORRECTNESS-002`: as a *cap* this was a classification bound, and a
/// valid `run_started` past it was hidden from every reader.)
pub(super) const FIRST_LINE_WINDOW: u64 = 1 << 20;

/// The fixed buffer [`newline_offset_from`] scans through. Never allocated per
/// byte read and never grown, so a log with no newline at all — the shape the
/// window exists for — costs one stack buffer however large the file is.
///
/// That is the *scan*'s memory, and not the probe's: a first line that is found
/// is then materialised at its own length by [`first_line_within`], which the
/// rule this module implements requires and this constant does not bound. See
/// that function.
pub(super) const SCAN_CHUNK: usize = 64 * 1024;

/// How many `Interrupted` answers one run of the first-line probe absorbs
/// before it stops and says the observation did not finish.
///
/// **This is the bound `SWEEP-CLASSIFY-001` asks for and PR #137 withdrew, and
/// what changed is not the number.** An interrupted read delivers no bytes, so
/// it spends none of the byte budget everything else here is bounded by:
/// without this, a source answering `Interrupted` is read for ever,
/// `classify_run_dir` never returns, and `startup_census` holds the physical
/// worktree lock while it does not. Measured against master's own copies of
/// [`first_line_within`] and [`newline_offset_from`], lifted verbatim into a
/// standalone binary: no return within 25 seconds at rustc 1.85.0 or 1.97.1,
/// after thousands of millions of reads. What PR #137 got wrong was the
/// *answer* on exhaustion, not the bound: see [`RunDirClass::Indeterminate`].
///
/// **Total across one run of the probe, not per read and not per unbroken run
/// of interruptions, and that is a deliberate divergence from the one other
/// bounded retry in this crate.** `agent::proc::drain`'s `INTERRUPTED_RETRIES`
/// is 64 *consecutive*, reset by any byte, which is right for a pipe a live
/// process is writing. Reset that way here, the allowance is spent again after
/// every byte, so a source alternating one byte with a burst is read `bound`
/// times over — finite, and a file this census walks can be gigabytes, so not a
/// bound anyone can state in seconds. Spent this way, the probe makes at most
/// this many more reads than an uninterrupted one would, whatever the source
/// does. The two numbers are different quantities and are not comparable: 64
/// interruptions with no progress on one pipe read, against every interruption
/// one whole classification meets.
///
/// The *shape* is the same as `drain`'s, and deliberately: an exhausted bound
/// there is a named `DrainError::Interrupted` rather than an end of stream, and
/// an exhausted allowance here is a classification of its own rather than a
/// husk. A bound whose exhaustion is indistinguishable from an ordinary answer
/// is the defect, in both places.
///
/// **Being wrong costs a directory kept, never one removed.** An exhausted
/// allowance answers [`RunDirClass::Indeterminate`], which retains, so a value
/// set too low hides a run from `list_runs` rather than offering it to the
/// reclaim path. That is why it is generous rather than tuned: the only thing
/// [`first_committed_line`] ever opens is a regular file, and reading one does
/// not answer `Interrupted` in ordinary operation at all.
const INTERRUPTED_ALLOWANCE: u32 = 1 << 16;

/// The `Interrupted` allowance one run of the probe spends, shared by every
/// read it makes.
struct Allowance(u32);

impl Allowance {
    /// A fresh allowance, for one run of the probe.
    const fn new() -> Self {
        Self(INTERRUPTED_ALLOWANCE)
    }

    /// Spend one interruption. `false` once there is none left, which is the
    /// observation that could not be completed.
    fn spend(&mut self) -> bool {
        match self.0.checked_sub(1) {
            Some(left) => {
                self.0 = left;
                true
            }
            None => false,
        }
    }
}

/// Classify one run directory. Read-only, and bounded rather than total.
///
/// The read is bounded; acquiring the handle is not. [`first_committed_line`]
/// refuses anything whose name is not a regular file, and a path swapped for a
/// writer-less fifo inside the window between that check and the `open` blocks
/// in the kernel — under the physical worktree lock the census holds across
/// this call. The module doc says what was measured, that the non-blocking open
/// which would close it *is* available here at the cost of an allowlist row,
/// and why the close is deferred to the funnel parent as a preference all the
/// same; this sentence asserts no totality it does not have.
///
/// **Every other way this answers `Husk` is bounded**, including the ones that
/// are failures rather than verdicts: an `events.jsonl` that cannot be stat'd,
/// opened or read is a `Husk`, which is `startup_census`'s "anything else" and
/// not a claim that nothing is there. What the census does with each of those
/// answers is in [`first_committed_line`].
///
/// **The one failure that is not folded that way is an unfinished read**
/// (`SWEEP-CLASSIFY-001`). A source that answers `Interrupted` past
/// [`INTERRUPTED_ALLOWANCE`] classifies [`RunDirClass::Indeterminate`], which
/// no reader treats as a husk. The other folds are unchanged and still answer
/// `Husk`; making *them* say so is `SWEEP-CLASSIFY-009` and the P3 row against
/// this file, and neither is taken here.
#[must_use]
pub fn classify_run_dir(public: &Path) -> RunDirClass {
    class_of(first_committed_line(public))
}

/// Which classification each of the probe's three answers is.
///
/// Split from [`classify_run_dir`], which is one call to it, because
/// [`Observed::Incomplete`] cannot be reached through a *path* **in this
/// suite**: a fixture's `events.jsonl` is an ordinary regular file and nothing
/// here arranges the signal that interrupts a read of one
/// ([`RunDirClass::Indeterminate`] says why that is a fact about fixtures and
/// not about production). So the arm that must never read `Husk` is driven
/// here, over a value, and there is nothing between the two functions for a
/// repair to drift through.
fn class_of<T>(observed: Observed<T>) -> RunDirClass {
    match observed {
        Observed::Found(_) => RunDirClass::Committed,
        Observed::Absent => RunDirClass::Husk,
        // NEVER `Husk`. `Husk` is the reclaiming answer and this observation
        // established nothing: `RunDirClass::Indeterminate` says why.
        Observed::Incomplete => RunDirClass::Indeterminate,
    }
}

/// The header of a committed first line, or the answer to give instead.
///
/// Deliberately not `events::started_of`: recovery step (a0) probes this
/// header and only *then* "select[s] the engine by schema", so classification
/// cannot be schema-specific — a schema-4 log must classify through the same
/// call as a schema-1 one, and each engine's own event type refuses the other's.
///
/// **[`Observed::Absent`] is still two different facts, and this signature now
/// holds two of three.** Two of the folds below are honest absence: a first
/// line that is not UTF-8 and one that is not this header are not a valid
/// `run_started`, which is what the packet asks. The rest — the stat, the
/// `open`, the fstat, the two reads' *errors* and the seek — fold an I/O
/// failure the filesystem declined to explain into the same `Absent`, so "I
/// could not read it" and "there is nothing here" still reach `startup_census`
/// as one answer. That is what `startup_census`'s "anything else" licenses, and
/// the paragraphs below say what the census then does with it, but the *reason*
/// is lost at this return and no caller can recover it. Giving the census
/// report a reason is `SWEEP-CLASSIFY-010`, a deferred row against
/// `src/engine/topology/startup.rs`.
///
/// **What has moved out of that set is the read that did not finish.** An
/// interrupted read past [`INTERRUPTED_ALLOWANCE`] reaches here as
/// [`Observed::Incomplete`] and is carried out unchanged rather than folded,
/// which is `SWEEP-CLASSIFY-001`'s repair and the only behaviour change in this
/// function. The other folds are master's.
fn first_committed_line(public: &Path) -> Observed<RunStartedHeader> {
    let path = public.join(EVENT_LOG);
    // `open(2)` runs *before* the read, and the read's bound cannot defend it
    // (`PR5-CONF-001`). `open` on a fifo with no writer blocks in the kernel and
    // never returns a handle at all, so `first_line`'s fstat bound — which is
    // taken on a handle this function has already been given — is not reached.
    // That is `PR5-RD-001`'s consequence one syscall earlier: `startup_census`
    // requires *every* entry to classify before a write command proceeds, and
    // the command holds the physical worktree lock across the census, so an
    // entry that never classifies is a lock held for ever.
    //
    // The guard is `symlink_metadata`, not `metadata`, and the difference is
    // deliberate. `stat(2)` on a fifo answers immediately, so either would
    // terminate; what following the link would leave open is a **swap of the
    // link's target** between the check and the open. Refusing the link itself
    // narrows the residual race to replacing a directory entry the census owns.
    // A symlinked `events.jsonl` is therefore a `Husk` whatever it points at,
    // which is this subsystem's stance wherever the filesystem is undecidable or
    // a path is a link — `super::CommitRecordPresence::Unknown` is treated as
    // `Present` by every caller, and `super::ownership` refuses a locator chain
    // that passes through a reparse point and takes only `NotFound` as proof
    // that `committed.json` is absent.
    //
    // **What makes `Husk` the safe direction here is the directory listing, not
    // the commit record**, and the sentence this one replaces had that wrong. Of
    // `super::PrivateHalfOwnership`'s three answers, two reclaim. `Proven`
    // reclaims both halves and does require `committed.json` to be absent
    // (conjunct 12), which a run that reached `run_started` published at P5b.
    // `NothingBound` reclaims the **public** half through
    // `super::remove_public_husk` with no commit-record check anywhere on the
    // path — and it is what the proof answers as soon as the marker cannot be
    // read, which every committed run past P7 is, since P7 is the step that
    // removes the marker.
    // What stops it is `super::ownership`'s `unbound_shape`, which reclaims only
    // a bare directory or one holding the staging file alone: an `events.jsonl`
    // this probe failed to read is an entry in that listing, so the answer is
    // `RetainReason::MarkerlessWithContent`. Both halves are pinned by
    // `proof_cases`' "marker-less husk carrying run-scoped content" and "bare
    // public directory".
    //
    // The residual is that the listing is a *second* observation of a directory
    // this probe already failed to read once, and `super::read_dir_names` folds
    // two different failures into `[]` — the reclaiming answer. It answers `[]`
    // when `read_dir` itself fails, and its `.flatten()` drops an entry whose
    // iteration step fails, so a directory that opened but could not be walked
    // past `events.jsonl` also reads as bare. A transient whole-process failure
    // (`EMFILE`, `ENFILE`) reaches the `open` below, the marker read and that
    // listing at the same moment, and `remove_public_husk` lists the directory
    // again once it has passed. Neither fold is in this file; both are
    // `SWEEP-CLASSIFY-009`, deferred against `src/rundir.rs` and
    // `src/rundir/ownership.rs`, and `read_dir_names` is PR #139's subject
    // while this is written — do not repair it from here.
    if !fs::symlink_metadata(&path).is_ok_and(|entry| entry.is_file()) {
        return Observed::Absent;
    }
    let Ok(mut file) = File::open(&path) else {
        return Observed::Absent;
    };
    header_of(first_line(&mut file))
}

/// The header a first line carries, or the answer to carry out in its place.
///
/// Split from [`first_committed_line`] for the reason [`class_of`] is: the
/// [`Observed::Incomplete`] arm is the one `SWEEP-CLASSIFY-001` turns on, and
/// no `events.jsonl` a test *here* can produce it -- nothing in this suite
/// arranges the signal that interrupts a read -- so the arm is driven over a
/// value. What it must not do is fold the third answer into the second on its
/// way past two honest absences — a first line that is not UTF-8 and one that
/// is not this header are absences, and an unfinished read is not.
fn header_of(line: Observed<Vec<u8>>) -> Observed<RunStartedHeader> {
    let line = match line {
        Observed::Found(line) => line,
        Observed::Absent => return Observed::Absent,
        Observed::Incomplete => return Observed::Incomplete,
    };
    let Ok(line) = std::str::from_utf8(&line) else {
        return Observed::Absent;
    };
    let Ok(header) = serde_json::from_str::<RunStartedHeader>(line) else {
        return Observed::Absent;
    };
    if header.event == "run_started" && header.data.schema >= 1 && !header.data.run_id.is_empty() {
        Observed::Found(header)
    } else {
        Observed::Absent
    }
}

/// The bytes of the first newline-terminated line, without its newline;
/// [`Observed::Absent`] when the file holds no newline at all, and
/// [`Observed::Incomplete`] when a read of it could not be finished.
///
/// The read is bounded by the file's **own length**, taken by `fstat` on the
/// handle that is about to be read (`PR5-RD-001`). Two properties have to hold
/// at once here and an earlier repair traded one for the other:
///
/// * **A committed run is never hidden.** `startup_census` defines `Committed`
///   as "`events.jsonl` exists and its first newline-terminated line is a valid
///   `run_started`" and states no size exception, so a first line past the
///   window must still be found. It is: the bound is the whole file, so the
///   scan reaches any newline a regular file actually contains, however far in.
/// * **Classification terminates.** Before this, the scan ran until a read
///   returned zero — which an endless source never does. A public run directory
///   whose `events.jsonl` was a symlink to `/dev/zero` was therefore never
///   classified at all, and since `startup_census` requires *every* entry to be
///   `Committed` or `Husk` before a write command proceeds, the command held
///   the worktree lock for ever. The file's own length is a bound the source
///   cannot argue with: a source that declares no length is read zero bytes.
///
/// The bound is the *read*, never the answer — the distinction the removed
/// `FIRST_LINE_CAP` got wrong.
///
/// It bounds a handle it is **given**, so it says nothing about how that handle
/// was obtained, and the earlier version of this comment overstated itself by
/// concluding "a device or a fifo … is a `Husk`" (`PR5-CONF-001`). That is true
/// of a device, whose `open` returns; it was never true of a writer-less fifo,
/// whose `open` does not. [`first_committed_line`] carries that half now, by
/// refusing to open anything that is not a regular file, and this function
/// still carries the endless-*device* half — both are measured together in
/// [`a_run_directory_whose_log_never_ends_is_still_classified`].
pub(super) fn first_line(file: &mut File) -> Observed<Vec<u8>> {
    let Ok(handle) = file.metadata() else {
        return Observed::Absent;
    };
    first_line_within(file, handle.len())
}

/// [`first_line`] over any source, with the byte budget given explicitly.
///
/// Split out so the budget is a *value a test can supply* rather than a
/// property of a file a test would have to construct. The endless source the
/// production bound defends against is `/dev/zero`, which exists on one of the
/// two platforms this ships on and cannot be built at all on the other; over
/// this signature the same source is a twenty-line reader, so the termination
/// claim is measured on every host rather than on Linux only.
///
/// **Every read here terminates on a source that answers `Interrupted`, and
/// that is `SWEEP-CLASSIFY-001`'s repair.** There are three of them — the
/// window read, the scan's reads in [`newline_offset_from`], and the re-read —
/// and all three go through [`read_step`], which is the one place in this
/// module that retries `Interrupted` at all. They share one
/// [`INTERRUPTED_ALLOWANCE`] for the whole call, and its exhaustion is
/// [`Observed::Incomplete`], never [`Observed::Absent`].
///
/// **Which is the part a bound alone gets wrong, and PR #137 wrote that bound
/// and withdrew it.** An exhausted bound has to answer something; when the only
/// answers were `Committed` and `Husk` it had to answer `Husk`, the
/// *reclaiming* one, so a burst of interruptions turned a run the source would
/// have delivered into one `list_runs` hides, resume refuses and the reclaim
/// path may delete. What makes the bound safe is [`RunDirClass::Indeterminate`]
/// being there to receive it, not the size of the allowance.
///
/// **Master's behaviour, so the claim above is falsifiable.** Both functions
/// lifted verbatim out of this file into a standalone binary and driven by a
/// reader that answers `Interrupted` unconditionally did not return within 25
/// seconds, at rustc 1.85.0 and at 1.97.1, with the `Take` limit untouched:
/// `std::io`'s `read_to_end` retries `Interrupted` without limit and an
/// interrupted read spends none of a `Take`'s byte budget.
///
/// **The scan is constant memory and the answer is not.** A source with no
/// newline in it costs one [`SCAN_CHUNK`] buffer however long it is, which is
/// the shape the window exists for; a first line that *is* found is then
/// materialised at its own length, because the rule this module implements
/// states no size exception and the parse needs the whole line. So the probe's
/// peak memory is
/// the length of the log's first line, and a census of a directory holding a
/// hostile one pays it. Bounding that is a decision about what the census may
/// spend rather than about what a first line is, so it is `SWEEP-CLASSIFY-012`,
/// a deferred row, rather than a bound invented here.
///
/// **Two reads, and the second is not assumed to agree with the first.** The
/// scan finds an offset in constant memory and then the line is re-read from
/// the start; a source that changed in between could hand the re-read bytes
/// that are not a first line at all. Every property the caller relies on is
/// therefore re-established on the re-read itself, at the site.
pub(super) fn first_line_within<R: Read + Seek>(source: &mut R, bound: u64) -> Observed<Vec<u8>> {
    let mut allowance = Allowance::new();
    let mut window = Vec::new();
    match read_up_to(
        source,
        FIRST_LINE_WINDOW.min(bound),
        &mut window,
        &mut allowance,
    ) {
        Observed::Found(()) => {}
        Observed::Absent => return Observed::Absent,
        Observed::Incomplete => return Observed::Incomplete,
    }
    if let Some(newline) = window.iter().position(|byte| *byte == b'\n') {
        window.truncate(newline);
        return Observed::Found(window);
    }
    // The cursor is at `window.len()`, so the scan continues from there rather
    // than re-reading what the window already proved newline-free, and spends
    // only what the window did not.
    let scanned = window.len() as u64;
    let length = match newline_offset_from(
        source,
        scanned,
        bound.saturating_sub(scanned),
        &mut allowance,
    ) {
        Observed::Found(length) => length,
        Observed::Absent => return Observed::Absent,
        Observed::Incomplete => return Observed::Incomplete,
    };
    // Re-read from the start and re-establish the whole contract on the bytes
    // this function is about to return, rather than carrying the scan's view of
    // a source that may have changed underneath it: `length + 1` bytes, the last
    // of them the terminator and none of the others one. A log that shrank has
    // fewer bytes; one rewritten so a newline lands earlier fails the third
    // check and one rewritten so it lands later fails the second. `Husk` is the
    // safe direction for all three, and it is the one the shrunk log already
    // took — the other two used to return bytes that were not a first line at
    // all, and the terminator requirement this module classifies by is exactly
    // what they broke.
    let want = length.saturating_add(1);
    if source.seek(SeekFrom::Start(0)).is_err() {
        return Observed::Absent;
    }
    let mut line = Vec::new();
    match read_up_to(source, want, &mut line, &mut allowance) {
        Observed::Found(()) => {}
        Observed::Absent => return Observed::Absent,
        Observed::Incomplete => return Observed::Incomplete,
    }
    if line.len() as u64 != want || line.pop() != Some(b'\n') {
        return Observed::Absent;
    }
    if line.contains(&b'\n') {
        Observed::Absent
    } else {
        Observed::Found(line)
    }
}

/// The absolute offset of the first `\n` at or after `offset`, in constant
/// memory; [`Observed::Absent`] when there is none within `budget` further
/// bytes, and [`Observed::Incomplete`] when a read could not be finished.
///
/// `source`'s cursor must already be at `offset`. The offset of the newline is
/// also the length of the line that precedes it, which is what the caller
/// wants.
///
/// **This loop terminates on a source that answers `Interrupted`, and the
/// reason is one function away rather than in this body** — the retry is
/// [`read_step`]'s, spent out of the `allowance` the whole probe shares.
/// Every iteration returns or spends a byte of `budget`, except an
/// interruption, which spends an interruption instead; both are finite, so the
/// loop is. `Interrupted` is *not* treated as an end of file, which would
/// classify a committed run as a husk, and running out does not answer `Husk`
/// either: it answers [`Observed::Incomplete`], which retains.
///
/// This function used to hold the second of `SWEEP-CLASSIFY-001`'s two
/// unbounded doors, in this module's own code rather than in `std::io`'s — the
/// one a repair that closed only [`first_line_within`]'s `read_to_end` calls
/// would have left open. There is now one retry site for both.
fn newline_offset_from<R: Read>(
    source: &mut R,
    mut offset: u64,
    mut budget: u64,
    allowance: &mut Allowance,
) -> Observed<u64> {
    let mut chunk = [0_u8; SCAN_CHUNK];
    while budget > 0 {
        let want = chunk_len(budget);
        let Some(buffer) = chunk.get_mut(..want) else {
            // Unreachable: `chunk_len` never answers more than `SCAN_CHUNK`,
            // which is this buffer's length. It answers the *retaining* class
            // rather than refusing, because a scan that could not be set up is
            // an observation that did not happen (§7: the arm decides, and it
            // decides the safe way).
            return Observed::Incomplete;
        };
        // A short read is normal, not an end: only zero means end of file.
        let read = match read_step(source, buffer, allowance) {
            // Clamped, so a `Read` implementation answering more than it was
            // given cannot index past the buffer or underflow the budget. No
            // claim is made about which implementations do that; the clamp
            // costs one comparison and the claim would have to be defended.
            Step::Read(read) => read.min(want),
            Step::End | Step::Failed => return Observed::Absent,
            Step::Incomplete => return Observed::Incomplete,
        };
        let Some(scanned) = chunk.get(..read) else {
            return Observed::Incomplete;
        };
        if let Some(at) = scanned.iter().position(|byte| *byte == b'\n') {
            return Observed::Found(offset + at as u64);
        }
        offset += read as u64;
        budget -= read as u64;
    }
    Observed::Absent
}

/// What one `read` of a source established.
///
/// Four answers rather than [`Observed`]'s three, because a read has an end and
/// an observation does not: [`Self::End`] and [`Self::Failed`] are different
/// facts to this module's callers even though both fold to
/// [`Observed::Absent`] today.
enum Step {
    /// Bytes were delivered. Never zero — that is [`Self::End`].
    Read(usize),
    /// End of the source.
    End,
    /// The shared allowance for `Interrupted` ran out, so this read did not
    /// finish and neither did the observation containing it.
    Incomplete,
    /// The source failed for a reason it named. Master folds this into `Husk`
    /// and this repair does not change that: it is the P3 row against this
    /// file, not `SWEEP-CLASSIFY-001`.
    Failed,
}

/// One `read`, with `Interrupted` retried against the probe's shared allowance
/// rather than for ever.
///
/// **This is the only place in this module that retries `Interrupted`**, and
/// `the_probe_has_exactly_one_interrupted_retry_site` in `rundir::tests` is the
/// assertion that it stays the only one. `SWEEP-CLASSIFY-001` had two
/// independent unbounded doors — this crate's own retry and `std::io`'s inside
/// `read_to_end` — and a repair that closed one of them would have looked
/// finished. Both reach this function now: [`newline_offset_from`] calls it,
/// and so does [`read_up_to`], which is why no `read_to_end` is left here.
fn read_step<R: Read>(source: &mut R, into: &mut [u8], allowance: &mut Allowance) -> Step {
    loop {
        match source.read(into) {
            Ok(0) => return Step::End,
            Ok(read) => return Step::Read(read),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                if !allowance.spend() {
                    return Step::Incomplete;
                }
            }
            Err(_) => return Step::Failed,
        }
    }
}

/// `Take::read_to_end`'s job, with the `Interrupted` allowance in this crate's
/// hands.
///
/// Appends at most `want` bytes to `into`, stopping early at the source's end,
/// which is what `by_ref().take(want).read_to_end(&mut into)` did. It is
/// written out because that spelling is where the first of
/// `SWEEP-CLASSIFY-001`'s doors was: `std::io` retries `Interrupted` inside
/// `read_to_end` without limit, and an interrupted read spends none of a
/// `Take`'s byte budget, so there was no bound to reach.
///
/// The buffer is the same fixed [`SCAN_CHUNK`] the scan uses; what grows is
/// `into`, which the caller sized by asking for `want`.
fn read_up_to<R: Read>(
    source: &mut R,
    want: u64,
    into: &mut Vec<u8>,
    allowance: &mut Allowance,
) -> Observed<()> {
    let mut chunk = [0_u8; SCAN_CHUNK];
    let mut left = want;
    while left > 0 {
        let take = chunk_len(left);
        let Some(buffer) = chunk.get_mut(..take) else {
            // Unreachable for the same reason as the scan's, and answered the
            // same retaining way.
            return Observed::Incomplete;
        };
        let read = match read_step(source, buffer, allowance) {
            // Clamped for the reason the scan clamps: a `Read` that answers
            // more than it was given must not be able to underflow `left` or
            // make this copy more bytes than it read.
            Step::Read(read) => read.min(take),
            // The source ended before `want`. That is not a failure and not an
            // unfinished observation: `read_to_end` stopped here too, and the
            // caller decides what a short read means.
            Step::End => return Observed::Found(()),
            Step::Incomplete => return Observed::Incomplete,
            Step::Failed => return Observed::Absent,
        };
        let Some(delivered) = chunk.get(..read) else {
            return Observed::Incomplete;
        };
        into.extend_from_slice(delivered);
        left -= read as u64;
    }
    Observed::Found(())
}

/// How much of the scan buffer a budget of `remaining` bytes may use.
///
/// The smaller of two `usize` values and not a conversion that can fail: the
/// `usize::try_from(..).ok()?` this replaces answered `Husk` for a case no
/// target this crate builds for can reach, which is a `?` that decided nothing
/// (§7).
fn chunk_len(remaining: u64) -> usize {
    match usize::try_from(remaining) {
        Ok(fits) if fits < SCAN_CHUNK => fits,
        _ => SCAN_CHUNK,
    }
}

/// The header of a committed first line: the envelope's tag, and the two
/// identifying fields inside its payload.
///
/// The wire is `{"ts": …, "event": "run_started", "data": {"schema": …,
/// "run_id": …, …}}` for every schema — `Event`/`TopologyEventBody` both tag on
/// `event` and both nest the record under `data`.
///
/// Unknown fields are allowed here and only here: this reads the *header* of a
/// line each schema's own type owns in full, and rejecting a schema-5 field
/// would classify a future run as a husk. What it does insist on is the shape
/// that makes the line a `run_started` at all — recovery step (a0) "probe[s]
/// the header of the committed first line" and then "select[s] the engine by
/// schema", so a line with no schema to select by is not one.
#[derive(Debug, Clone, Deserialize)]
pub(super) struct RunStartedHeader {
    pub(super) event: String,
    data: RunStartedIdentity,
}

#[derive(Debug, Clone, Deserialize)]
struct RunStartedIdentity {
    schema: u32,
    run_id: String,
}

/// The digest of a `run_started` line's exact bytes, for the commit record.
///
/// `run_creation`: "run_started_sha256 = the digest of the exact run_started
/// line bytes about to be appended".
#[must_use]
pub fn run_started_sha256(line: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(line))
}

#[cfg(test)]
mod tests {
    use super::{Allowance, INTERRUPTED_ALLOWANCE, Observed, RunDirClass, class_of, header_of};

    /// A valid committed first line, without its terminator, as
    /// [`super::first_line_within`] hands one over.
    fn committed_line() -> Vec<u8> {
        br#"{"ts":"2026-09-13T00:00:00Z","event":"run_started","data":{"schema":1,"run_id":"01AAA"}}"#
            .to_vec()
    }

    /// The observation-to-classification mapping, all three arms, as a table.
    ///
    /// **The middle row is the one this file exists for.** An observation that
    /// could not be completed must not reach a reader as `Husk`: `Husk` is what
    /// `list_husks` returns and what the census offers to the reclaim path, so
    /// the arm that answers it is a deletion decision. `classify_run_dir` is a
    /// single call to this function, and no fixture in this suite can drive the
    /// middle row through a path -- nothing here arranges the signal that
    /// interrupts a read -- which is why the table is here rather than over a
    /// directory.
    ///
    /// Both other rows are asserted beside it deliberately: a mapping that had
    /// collapsed to one answer would satisfy any single row on its own.
    #[test]
    fn an_observation_that_did_not_finish_is_never_a_husk() {
        assert_eq!(class_of(Observed::Found(())), RunDirClass::Committed);
        assert_eq!(
            class_of::<()>(Observed::Incomplete),
            RunDirClass::Indeterminate,
            "an observation that could not be completed must never be the \
             reclaiming classification"
        );
        assert_ne!(
            class_of::<()>(Observed::Incomplete),
            RunDirClass::Husk,
            "SWEEP-CLASSIFY-001: this is the arm PR #137 was withdrawn for"
        );
        assert_eq!(class_of::<()>(Observed::Absent), RunDirClass::Husk);
    }

    /// The allowance is spent, runs out, and stays out.
    ///
    /// The count is asserted against a literal rather than against
    /// `INTERRUPTED_ALLOWANCE`, because a test that derives its expectation
    /// from the constant it is checking grows with any mutation of it and
    /// catches none. The constant is named only in the floor: raising it is
    /// allowed, and removing the bound is what this refuses.
    #[test]
    fn the_interruption_allowance_is_finite_and_does_not_refill() {
        let mut allowance = Allowance::new();
        let mut spent = 0_u64;
        while allowance.spend() {
            spent += 1;
            assert!(
                spent <= 1_000_000,
                "the allowance did not run out within a million interruptions, \
                 so the probe is not bounded on an interrupting source"
            );
        }
        assert_eq!(spent, u64::from(INTERRUPTED_ALLOWANCE));
        assert!(!allowance.spend(), "an exhausted allowance does not refill");
    }

    /// An unfinished read is carried past the parse, not folded into it.
    ///
    /// [`header_of`] is where the two *honest* absences live — a first line
    /// that is not UTF-8 and one that is not this header — and it is the one
    /// place between the read and the classification that could turn an
    /// unfinished observation into one of them. The three other rows are here
    /// so the assertion is about the fold and not about a function that answers
    /// `Incomplete` to everything.
    #[test]
    fn an_unfinished_read_is_not_folded_into_the_parses_absences() {
        // `matches!` rather than `assert_eq!`: `RunStartedHeader` is a
        // deserialized wire shape and carries no `PartialEq`, and giving it one
        // for a test would be the test deciding the production type's traits.
        assert!(
            matches!(header_of(Observed::Incomplete), Observed::Incomplete),
            "SWEEP-CLASSIFY-001: the third answer is carried, never folded"
        );
        assert!(matches!(header_of(Observed::Absent), Observed::Absent));
        assert!(
            matches!(
                header_of(Observed::Found(committed_line())),
                Observed::Found(_)
            ),
            "a valid committed first line is still found, or the rows above measure a \
             function that refuses everything"
        );
        assert!(
            matches!(
                header_of(Observed::Found(b"not json".to_vec())),
                Observed::Absent
            ),
            "a first line that is not this header is an absence, which is a completed \
             observation"
        );
    }
}
