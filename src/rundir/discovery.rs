//! Run discovery: the readers `startup_census` names, and the husk report.
//!
//! `startup_census`: "every reader (`list_runs`, `latest_run`,
//! `resolve_run_id`, `find_question`, `status`) returns Committed directories
//! only, **whether or not a marker is present**". Every function here is one of
//! those readers or the census surface `upstroke status` renders, and every one
//! of them is read-only: the census *decides* here and *acts* in the parent,
//! where the deletion sites are.

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

use std::path::{Path, PathBuf};

use crate::error::UpstrokeError;

use super::{
    CreatingMarker, EVENT_LOG, MARKER, PrivateHalfOwnership, RepoKey, RetainReason, RunDirClass,
    UnboundShape, classify_run_dir, fs, prove_private_half_ownership, public_dir, runs_root,
};

/// Every run in this repo, oldest first.
///
/// Run ids are ULIDs with the millisecond timestamp in the high bits and
/// Crockford base32's digits-before-letters ordering, so a plain lexicographic
/// sort is chronological — no directory timestamps, which copying a repo would
/// scramble.
///
/// **Committed directories only.** `startup_census`: "every reader
/// (`list_runs`, `latest_run`, `resolve_run_id`, `find_question`, `status`)
/// returns Committed directories only, **whether or not a marker is present**",
/// and `run_creation` says it from the other side: "readers never return a
/// directory without a committed `run_started` and never hide one because of a
/// marker". Both halves are load-bearing and each is a separate test.
///
/// This is the slice's only change in behaviour: a legacy husk that today
/// shadows [`latest_run`] is no longer listed. A run whose log committed is
/// listed exactly as before, marker or no marker.
///
/// **A directory the probe could not classify is not listed either**
/// ([`RunDirClass::Indeterminate`], `SWEEP-CLASSIFY-001`): this returns
/// `Committed` directories and that is not one. It is not returned by
/// [`list_husks`] either, so nothing offers it for reclaim — which is the point
/// of the class. [`resolve_run_id`] says which of the two it met.
pub fn list_runs(repo_root: &Path) -> Vec<String> {
    let mut runs: Vec<String> = run_dir_names(repo_root)
        .into_iter()
        .filter(|run_id| listed_as_run(classify_run_dir(&public_dir(repo_root, run_id))))
        .collect();
    runs.sort();
    runs
}

/// Which classifications [`list_runs`] returns.
///
/// A named predicate rather than an inline `==`, and the reason is that the
/// fact it states cannot be measured over a directory: no fixture in this
/// suite classifies [`RunDirClass::Indeterminate`], because nothing here
/// arranges the signal that interrupts a read of an ordinary file, so the only
/// way to assert that this reader does not return one is to ask the predicate.
/// See
/// `a_directory_that_did_not_classify_is_neither_a_run_nor_a_husk`, which
/// crosses all three of these against all three classifications.
const fn listed_as_run(class: RunDirClass) -> bool {
    matches!(class, RunDirClass::Committed)
}

/// Which classifications [`list_husks`] returns. See [`listed_as_run`].
const fn listed_as_husk(class: RunDirClass) -> bool {
    matches!(class, RunDirClass::Husk)
}

/// Which classifications [`unclassified_matching`] reports. See
/// [`listed_as_run`].
const fn unclassified(class: RunDirClass) -> bool {
    matches!(class, RunDirClass::Indeterminate)
}

/// Every directory under `<repo>/.upstroke/runs`, committed or not, oldest first.
///
/// Not a reader in `startup_census`'s sense and deliberately not filtered by
/// commitment: this is the enumeration a census walks and the one the worktree
/// lease's R28 check scans. A crashed run whose log never committed is exactly
/// the run whose reaper is most likely still holding its cleanup lease, so
/// filtering here would hide the hold that check exists to observe.
#[must_use]
pub fn run_dir_names(repo_root: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(runs_root(repo_root)) else {
        return Vec::new();
    };
    let mut runs: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    runs.sort();
    runs
}

/// Every husk under `<repo>/.upstroke/runs`, oldest first.
///
/// `Husk` and nothing else: a directory whose classification did not finish is
/// [`RunDirClass::Indeterminate`] and is absent from this list, so the two
/// readers of it — `status`'s husk answer and [`resolve_run_id`]'s refusal
/// message — cannot call it a husk, and neither can a caller added later that
/// reclaims from it. Those are the two readers in this repository at this SHA,
/// by `grep -rn 'list_husks' src/`.
#[must_use]
pub fn list_husks(repo_root: &Path) -> Vec<String> {
    run_dir_names(repo_root)
        .into_iter()
        .filter(|run_id| listed_as_husk(classify_run_dir(&public_dir(repo_root, run_id))))
        .collect()
}

/// What `status` says about a husk id it was asked for by name.
///
/// `startup_census`: "status is read-only: it ignores husks and, asked
/// explicitly for a husk id, reports an unstarted husk that the next write
/// command reclaims, a retained husk with its reason and locator, or a possibly
/// committed run whose public log has no valid committed first line".
#[derive(Debug)]
pub struct HuskReport {
    pub run_id: String,
    pub public: PathBuf,
    /// The private locator the marker records, when a marker parses.
    pub locator: Option<PathBuf>,
    pub disposition: HuskDisposition,
}

/// What the next write command's census would do with a husk it may reclaim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reclaimable {
    /// Nothing private is bound, so the public half alone is reclaimed.
    PublicOnly(UnboundShape),
    /// The ownership proof holds and no commit record exists: the private half
    /// is reclaimed through the proof-token funnel, then the public directory
    /// with the marker last.
    BothHalves,
}

/// The trichotomy `status` reports a husk id by.
#[derive(Debug)]
pub enum HuskDisposition {
    /// Nothing has started here: the next write command reclaims it.
    Unstarted(Reclaimable),
    /// Retained and reported until the deferred prune command removes it.
    /// [`RetainReason::PossiblyCommitted`] is the third of the three sentences.
    Retained(RetainReason),
}

impl HuskDisposition {
    /// The operator-facing sentence, which names which of the three this is.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Unstarted(Reclaimable::BothHalves) => "an unstarted husk, bound to a private \
                 half that never committed, that the next write command reclaims"
                .to_owned(),
            Self::Unstarted(Reclaimable::PublicOnly(shape)) => format!(
                "an unstarted husk ({}) that the next write command reclaims",
                match shape {
                    UnboundShape::Bare => "a bare directory",
                    UnboundShape::StagedMarkerOnly => "only a staged marker",
                    UnboundShape::TargetAbsent => "its recorded private half is gone",
                }
            ),
            Self::Retained(RetainReason::PossiblyCommitted) => {
                "a possibly committed run whose public log has no valid committed first line; \
                 nothing is deleted"
                    .to_owned()
            }
            Self::Retained(reason) => format!("a retained husk: {reason}"),
        }
    }
}

/// Report a husk by id, for `status` and for the census report.
///
/// Read-only from end to end. The authorized private root is the one the
/// command is configured with, which for a read-only `status` is the default.
#[must_use]
pub fn husk_report(
    repo_root: &Path,
    run_id: &str,
    repo_key: &RepoKey,
    authorized_root: &Path,
) -> HuskReport {
    let public = public_dir(repo_root, run_id);
    let locator = fs::read_to_string(public.join(MARKER))
        .ok()
        .and_then(|text| serde_json::from_str::<CreatingMarker>(&text).ok())
        .map(|marker| PathBuf::from(marker.private_dir));
    let disposition = match prove_private_half_ownership(&public, repo_key, authorized_root) {
        // A token means the husk is provably this run's and never committed —
        // reclaimable, both halves, by the next write command. The token is
        // dropped unspent: `status` is read-only.
        PrivateHalfOwnership::Proven(_) => HuskDisposition::Unstarted(Reclaimable::BothHalves),
        PrivateHalfOwnership::NothingBound(shape) => {
            HuskDisposition::Unstarted(Reclaimable::PublicOnly(shape))
        }
        PrivateHalfOwnership::Retained(reason) => HuskDisposition::Retained(reason),
    };
    HuskReport {
        run_id: run_id.to_owned(),
        public,
        locator,
        disposition,
    }
}

/// The most recent run — what `upstroke status` reports when given no id.
pub fn latest_run(repo_root: &Path) -> Option<String> {
    list_runs(repo_root).pop()
}

/// Resolve a run id from any unambiguous prefix, so an operator can type the
/// first few characters of a 26-character ULID.
///
/// An exact match wins outright rather than being treated as one candidate
/// among several: a full id is never ambiguous, even if some other run happens
/// to extend it.
pub fn resolve_run_id(repo_root: &Path, wanted: &str) -> Result<String, UpstrokeError> {
    let runs = list_runs(repo_root);
    let wanted_upper = wanted.to_ascii_uppercase();
    // The entry as it exists on disk, not the uppercased input. The comparison
    // is case-insensitive because a run directory can arrive from a
    // case-insensitive filesystem, and on a case-sensitive one only the real
    // name builds a path that opens — everything downstream joins this id.
    if let Some(matched) = runs.iter().find(|id| id.eq_ignore_ascii_case(wanted)) {
        return Ok(matched.clone());
    }
    let matches: Vec<&String> = runs
        .iter()
        .filter(|id| id.to_ascii_uppercase().starts_with(&wanted_upper))
        .collect();
    match matches.as_slice() {
        [only] => Ok((*only).clone()),
        [] => Err(UpstrokeError::Refused {
            message: match husk_matching(repo_root, wanted) {
                // A directory is there, and it holds no committed `run_started`.
                // Saying "no run matches that id" of a directory the operator
                // can see is the answer that sends them looking for a bug.
                Some(husk) => format!(
                    "`{husk}` never recorded a committed run_started, so there is no run to open \
                     there — ask `upstroke status {husk}` for what it is and what happens to it"
                ),
                // A directory is there and the probe could not read it. The
                // husk sentence above would be a *claim about its contents*
                // that nothing established, and it is the sentence `status`
                // prints too, so an operator would be told a run never started
                // when what happened is that the log could not be read.
                // `upstroke status` is not offered here because it resolves
                // through this same function and would print this same line.
                None => match unclassified_matching(repo_root, wanted) {
                    Some(unread) => format!(
                        "`{unread}` could not be classified: its {EVENT_LOG} could not be read to \
                         a first line, so whether a run committed there is unknown — nothing has \
                         been deleted and the next census retains it"
                    ),
                    None if runs.is_empty() => {
                        format!("no runs found under {}", runs_root(repo_root).display())
                    }
                    None => format!("no run matches that id; known runs: {}", runs.join(", ")),
                },
            },
        }),
        several => Err(UpstrokeError::Refused {
            message: format!(
                "that prefix matches {} runs ({}); use more characters",
                several.len(),
                several
                    .iter()
                    .map(|id| id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }),
    }
}

/// The husk a wanted id names, exactly or by unambiguous prefix.
///
/// Only used to explain a refusal, so an ambiguous prefix answers `None`: the
/// operator is told to use more characters by the branch above, not sent to a
/// husk that merely happens to be one of the matches.
fn husk_matching(repo_root: &Path, wanted: &str) -> Option<String> {
    matching(&list_husks(repo_root), wanted)
}

/// The directory a wanted id names whose classification did not finish, exactly
/// or by unambiguous prefix.
///
/// Consulted only after [`husk_matching`] has answered `None`, so the husk
/// sentence and the set it is drawn from are exactly what they were: this adds
/// a branch below the existing one rather than widening it.
fn unclassified_matching(repo_root: &Path, wanted: &str) -> Option<String> {
    let unread: Vec<String> = run_dir_names(repo_root)
        .into_iter()
        .filter(|run_id| unclassified(classify_run_dir(&public_dir(repo_root, run_id))))
        .collect();
    matching(&unread, wanted)
}

/// The one id in `names` a wanted id names, exactly or by unambiguous prefix.
///
/// Extracted from [`husk_matching`] unchanged so the second caller cannot drift
/// from the first on what "unambiguous" means.
fn matching(names: &[String], wanted: &str) -> Option<String> {
    let wanted_upper = wanted.to_ascii_uppercase();
    if let Some(exact) = names.iter().find(|id| id.eq_ignore_ascii_case(wanted)) {
        return Some(exact.clone());
    }
    let mut prefixed = names
        .iter()
        .filter(|id| id.to_ascii_uppercase().starts_with(&wanted_upper));
    let first = prefixed.next()?;
    prefixed.next().is_none().then(|| first.clone())
}

/// A question id resolved to the run that raised it.
#[derive(Debug)]
pub struct FoundQuestion {
    pub run_id: String,
    /// The run's public directory — everything `upstroke answer` touches.
    pub public: PathBuf,
    /// The full question id, expanded from whatever prefix was typed.
    pub question_id: String,
}

/// Find the run holding a question, by full id or unambiguous prefix.
///
/// Scans every run rather than requiring the operator to remember which one
/// asked: the notifier hands them a question id, not a run id, so a question
/// id is what the command has to accept.
pub fn find_question(repo_root: &Path, wanted: &str) -> Result<FoundQuestion, UpstrokeError> {
    let wanted_upper = wanted.to_ascii_uppercase();
    let mut exact: Option<FoundQuestion> = None;
    let mut matches: Vec<FoundQuestion> = Vec::new();
    for run_id in list_runs(repo_root) {
        let public = public_dir(repo_root, &run_id);
        let Ok(entries) = fs::read_dir(public.join("questions")) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(question_id) = name.strip_suffix(".json") else {
                continue;
            };
            let found = FoundQuestion {
                run_id: run_id.clone(),
                public: public.clone(),
                question_id: question_id.to_owned(),
            };
            if question_id.eq_ignore_ascii_case(wanted) {
                exact = Some(found);
            } else if question_id.to_ascii_uppercase().starts_with(&wanted_upper) {
                matches.push(found);
            }
        }
    }
    if let Some(found) = exact {
        return Ok(found);
    }
    match matches.len() {
        1 => matches.pop().ok_or_else(|| UpstrokeError::Refused {
            message: "question vanished while resolving it".to_owned(),
        }),
        0 => Err(UpstrokeError::Refused {
            message: format!(
                "no question with that id under {}",
                runs_root(repo_root).display()
            ),
        }),
        several => Err(UpstrokeError::Refused {
            message: format!(
                "that prefix matches {several} questions ({}); use more characters",
                matches
                    .iter()
                    .map(|found| found.question_id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{RunDirClass, listed_as_husk, listed_as_run, unclassified};

    /// The three classifications, crossed with the three readers that filter on
    /// them.
    ///
    /// **This table is the only driver these readers have for the third
    /// class.** No directory a test here can build classifies
    /// `Indeterminate`: that class comes from a read a signal interrupted, and
    /// nothing in this suite arranges a signal — a fixture's `events.jsonl` is
    /// an ordinary regular file whose reads deliver bytes or end. So
    /// `list_runs` and `list_husks` are measured over the classification rather
    /// than over a fixture, through the predicates they filter by — which is
    /// why those predicates are named functions.
    ///
    /// The property is the finding's: an observation that could not be
    /// completed is returned by **neither** reader. Not as a run, which would
    /// resume from a log nothing managed to read; and not as a husk, which is
    /// what `husk_report` and the census's reclaim arm are fed from, and
    /// therefore the answer that can end in a deletion.
    #[test]
    fn a_directory_that_did_not_classify_is_neither_a_run_nor_a_husk() {
        let mut seen = Vec::new();
        for class in [
            RunDirClass::Committed,
            RunDirClass::Husk,
            RunDirClass::Indeterminate,
        ] {
            // Exhaustive on purpose: a fourth classification stops compiling
            // here rather than passing this test without a row.
            let name = match class {
                RunDirClass::Committed => "committed",
                RunDirClass::Husk => "husk",
                RunDirClass::Indeterminate => "indeterminate",
            };
            let answers = [
                listed_as_run(class),
                listed_as_husk(class),
                unclassified(class),
            ];
            assert_eq!(
                answers.iter().filter(|answered| **answered).count(),
                1,
                "{name}: exactly one of the three readers claims a classification"
            );
            seen.push((name, answers));
        }
        assert_eq!(
            seen,
            vec![
                ("committed", [true, false, false]),
                ("husk", [false, true, false]),
                ("indeterminate", [false, false, true]),
            ],
            "SWEEP-CLASSIFY-001: an unfinished observation is neither listed as a run nor \
             offered as a husk"
        );
    }
}
