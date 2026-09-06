//! The integration transaction of one queued candidate.
//!
//! `decisions.coordinator_integration.integration_sequence`: the loop selects
//! the first eligible candidate, checks the ceiling, takes the provisional
//! `{pipeline, merge}` reservation, and then — **before any staging effect** —
//! asserts the integration ref publishable and reads its head. That read is
//! the exact-base decision: a head equal to the base `candidate_prepared`
//! recorded publishes the immutable candidate commit itself; any other head
//! takes the staging path. Everything after the decision is a terminal of the
//! transaction it opened, and every authorized publication is completed
//! through one function, [`publish`], on the live path and on recovery alike.
//!
//! The module performs no append of its own: every event goes through the
//! caller's [`IntegrationJournal`], which is the run's emitter and its fold,
//! so a live run and a replay reach the same state by the same checks. What
//! it owns is the *order* — reservation, then the head read, then the fast
//! `merge_prepared`, then the compare-and-swap, then `task_merged` — and the
//! refusals that order rests on.

use thiserror::Error;

use crate::error::UpstrokeError;
use crate::topology::effects::RefSite;
use crate::topology::events::{
    CandidateRef, CommitSha, GitRef, MergeLeaseRelease, MergePrepared, PreparedDisposition,
    SequenceId, TaskMerged, TopologyEventBody, VerificationSource,
};
use crate::topology::fold::{TopologyFold, TransactionClass};
use crate::topology::registry::TaskKey;
use crate::workspace_manager::{Slot, WorkspaceManager};

use super::candidate::RUN_REF_ROOT;
use super::seams::TopologyHooks;

/// `refs/upstroke/runs/<run>/prepared/<sequence>`: the pin that keeps a stale
/// candidate's proposal commit reachable while its verification runs (R12).
#[must_use]
pub fn prepared_pin_ref(run_id: &str, sequence: SequenceId) -> GitRef {
    GitRef(format!("{RUN_REF_ROOT}/{run_id}/prepared/{}", sequence.0))
}

/// The staging worktree of a stale transaction, `merge/s<sequence>` (R10).
#[must_use]
pub fn staging_slot(sequence: SequenceId) -> Slot {
    Slot::Staging {
        sequence: u64::from(sequence.0),
    }
}

/// What the integration sequence appends through, reads its state from, and
/// performs its effects with.
///
/// One trait rather than three parameters because the three are one object
/// on the live path: the run's emitter owns the fold it checks against, the
/// append handle, the hook bundle, and the reservation ledger the first
/// append converts. The sequence calls them in the order the packet fixes and
/// never holds two of them across a call.
pub trait IntegrationJournal {
    /// Append `body` through the run's emitter: checked against the fold,
    /// written and synced, then applied.
    ///
    /// # Errors
    ///
    /// The fold's refusal, or the append-error protocol's report.
    fn emit(&mut self, body: TopologyEventBody) -> Result<(), UpstrokeError>;

    /// The hook bundle the funnels take.
    fn hooks(&mut self) -> &mut dyn TopologyHooks;

    /// The provisional integration reservation of `key` converted to a
    /// fold-derived holding: called exactly once, right after the first
    /// append of the sequence.
    ///
    /// # Errors
    ///
    /// The reservation ledger's refusal when no such reservation is held.
    fn converted(&mut self, key: TaskKey) -> Result<(), UpstrokeError>;
}

/// Why the sequence refused, each naming the record and the value it
/// disagreed with.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Refusal {
    #[error(
        "refusing to integrate task {key} generation {generation}: no `candidate_prepared` in \
         the proven prefix records that candidate, and an integration publishes only a commit \
         the log judged"
    )]
    NoCandidateRecord { key: u32, generation: u32 },

    #[error(
        "the integration ref `{refname}` names nothing; a run publishes onto the ref \
         `run_started` recorded, and this one has no target to decide the exact-base case \
         against"
    )]
    IntegrationRefAbsent { refname: String },

    #[error(
        "refusing to publish sequence {sequence}: the integration ref `{refname}` is at {found}, \
         and the authorization expects {expected} before the move and {proposed} after it; a \
         third SHA is foreign history and is never adopted"
    )]
    ThirdSha {
        sequence: u32,
        refname: String,
        found: String,
        expected: String,
        proposed: String,
    },

    #[error(
        "refusing to prune the pin `{refname}`: it is at {found} and the publication it pinned \
         proposed {expected}; the substitution stays visible rather than being deleted"
    )]
    PinAtAnotherSha {
        refname: String,
        found: String,
        expected: String,
    },

    #[error(
        "refusing to integrate task {key} generation {generation}: the integration ref is at \
         {head} and the candidate's base is {base}, so the candidate is stale; this build \
         publishes exact-base candidates only and refuses a stale one before any staging effect"
    )]
    StaleNotImplemented {
        key: u32,
        generation: u32,
        head: String,
        base: String,
    },
}

impl From<Refusal> for UpstrokeError {
    fn from(refusal: Refusal) -> Self {
        Self::Refused {
            message: refusal.to_string(),
        }
    }
}

fn refused(message: &str) -> UpstrokeError {
    UpstrokeError::Refused {
        message: message.to_owned(),
    }
}

/// What the fold recorded about the candidate the loop selected, read once
/// before any effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrationRequest {
    pub candidate: CandidateRef,
    /// The sequence this transaction opens under: the fold's next dense one.
    pub sequence: SequenceId,
    /// The base `candidate_prepared` recorded — the head the exact-base
    /// decision compares against.
    pub base_sha: CommitSha,
    /// The closure this publication settles, as the fold derives it.
    pub satisfies: Vec<TaskKey>,
    /// The lease `task_merged` releases: the candidate's own, or its lineage's.
    pub lease_release: MergeLeaseRelease,
    /// The ref this run publishes onto, as `run_started` recorded it.
    pub integration_ref: GitRef,
}

impl IntegrationRequest {
    /// Read the request from the fold that selected `candidate`.
    ///
    /// # Errors
    ///
    /// [`Refusal::NoCandidateRecord`] when the fold holds no
    /// `candidate_prepared` for the candidate, or a refusal when the run has
    /// not started.
    pub fn from_fold(fold: &TopologyFold, candidate: &CandidateRef) -> Result<Self, UpstrokeError> {
        let started = fold
            .started()
            .ok_or_else(|| refused("the run has not started, so nothing can be integrated"))?;
        let sequence = fold
            .next_sequence()
            .ok_or_else(|| refused("the run has not started, so no sequence can be opened"))?;
        let satisfies = fold
            .satisfies_closure(candidate.key)
            .ok_or_else(|| refused("the run has not started, so nothing can be satisfied"))?;
        let base_sha = prepared_base(fold, candidate)?;
        Ok(Self {
            candidate: candidate.clone(),
            sequence,
            base_sha,
            satisfies,
            lease_release: lease_release(fold, candidate),
            integration_ref: started.integration_ref.clone(),
        })
    }
}

/// The base the candidate's `candidate_prepared` recorded.
fn prepared_base(
    fold: &TopologyFold,
    candidate: &CandidateRef,
) -> Result<CommitSha, UpstrokeError> {
    fold.task(candidate.key)
        .and_then(|task| {
            task.generations
                .iter()
                .find(|generation| generation.id == candidate.generation)
        })
        .and_then(|generation| generation.candidate.as_ref())
        .filter(|prepared| prepared.candidate == *candidate)
        .map(|prepared| prepared.base_sha.clone())
        .ok_or_else(|| {
            Refusal::NoCandidateRecord {
                key: candidate.key.0,
                generation: candidate.generation.0,
            }
            .into()
        })
}

/// The lease a publication of `candidate` releases.
///
/// `decisions.admission_and_leases.leases`: "task_merged releases the
/// Candidate lease or, when satisfies contains root, the lineage lease". A
/// lineage member's closure always reaches its root, so the release is the
/// lineage's exactly when the candidate's task descends from one.
fn lease_release(fold: &TopologyFold, candidate: &CandidateRef) -> MergeLeaseRelease {
    fold.registry()
        .and_then(|registry| registry.get(candidate.key))
        .and_then(|entry| entry.lineage)
        .map_or(
            MergeLeaseRelease::Candidate {
                key: candidate.key,
                generation: candidate.generation,
            },
            |lineage| MergeLeaseRelease::Lineage { root: lineage.root },
        )
}

/// The exact-base decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExactBase {
    /// The head is the candidate's base: publish the candidate commit itself.
    Fast,
    /// The head moved: cherry-pick and re-verify.
    Stale,
}

/// The head the integration ref was read at, and what it decided.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "a decision is followed by the sequence it selects"]
pub struct Decided {
    pub head: CommitSha,
    pub exact_base: ExactBase,
}

/// The exact-base decision: `assert_publishable`, then the head read.
///
/// Read-only, and every staging effect of the sequence comes after it — that
/// is INV-09's "the exact-base decision is made from the integration ref head
/// before any staging effect", and the reason this is a separate function
/// from the paths it selects between.
///
/// # Errors
///
/// A symbolic or checked-out integration ref (`assert_publishable`),
/// [`Refusal::IntegrationRefAbsent`], or a Git error reading the ref.
pub fn decide(
    manager: &WorkspaceManager,
    request: &IntegrationRequest,
) -> Result<Decided, UpstrokeError> {
    let refname = request.integration_ref.as_str();
    manager.assert_publishable(refname)?;
    let head =
        manager
            .direct_ref_target(refname)?
            .ok_or_else(|| Refusal::IntegrationRefAbsent {
                refname: refname.to_owned(),
            })?;
    let head = CommitSha(head);
    let exact_base = if head == request.base_sha {
        ExactBase::Fast
    } else {
        ExactBase::Stale
    };
    Ok(Decided { head, exact_base })
}

/// A publication the log has authorized and the ref move still owes.
///
/// Built on the live path by the `merge_prepared` that authorized it, and on
/// recovery from the fold's `Prepared` transaction. Either way [`publish`]
/// completes it: INV-09's "an authorized publication is always completed
/// (recovery or run-end closure), never abandoned".
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "an authorized publication is always completed, never abandoned"]
pub struct Authorized {
    pub sequence: SequenceId,
    pub key: TaskKey,
    pub expected_head: CommitSha,
    pub proposed_sha: CommitSha,
    pub satisfies: Vec<TaskKey>,
    pub lease_release: MergeLeaseRelease,
    pub integration_ref: GitRef,
    /// The `prepared/<seq>` pin of a stale publication, pruned after
    /// `task_merged`; `None` for a fast one, which pins nothing.
    pub pin: Option<GitRef>,
    /// The staging worktree of a stale publication, removed with force after
    /// `task_merged`; `None` for a fast one, which stages nothing.
    pub staging: Option<Slot>,
}

impl Authorized {
    /// The publication the fold's unresolved transaction authorizes, if the
    /// transaction has reached `merge_prepared`.
    ///
    /// `None` when there is no transaction or it is still verifying; the
    /// latter is `T-VERIFY`'s row, settled elsewhere.
    ///
    /// # Errors
    ///
    /// A refusal when the run has not started.
    pub fn from_fold(fold: &TopologyFold, run_id: &str) -> Result<Option<Self>, UpstrokeError> {
        let Some(transaction) = fold.transaction() else {
            return Ok(None);
        };
        let TransactionClass::Prepared {
            expected_head,
            proposed_sha,
            satisfies,
        } = &transaction.class
        else {
            return Ok(None);
        };
        let started = fold
            .started()
            .ok_or_else(|| refused("the proven prefix records a transaction and no run"))?;
        let candidate = &transaction.candidate;
        // A fast publication proposes the candidate commit itself and neither
        // pinned nor staged anything; every other disposition did both, and
        // what it left is pruned after the ref moves.
        let staged = *proposed_sha != candidate.commit_sha;
        Ok(Some(Self {
            sequence: transaction.sequence,
            key: candidate.key,
            expected_head: expected_head.clone(),
            proposed_sha: proposed_sha.clone(),
            satisfies: satisfies.clone(),
            lease_release: lease_release(fold, candidate),
            integration_ref: started.integration_ref.clone(),
            pin: staged.then(|| prepared_pin_ref(run_id, transaction.sequence)),
            staging: staged.then(|| staging_slot(transaction.sequence)),
        }))
    }
}

/// A completed publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    pub sequence: SequenceId,
    pub key: TaskKey,
    pub merged_sha: CommitSha,
    pub satisfies: Vec<TaskKey>,
}

/// The fast path: `merge_prepared(fast)` for a candidate whose base is the
/// head that was just read.
///
/// The record names the candidate's own `candidate_prepared` as its
/// verification source, proposes the candidate commit and pins nothing, and
/// the fold refuses every other shape (`check_merge_prepared`). The provisional
/// reservation converts at this append, its first.
///
/// # Errors
///
/// The fold's refusal or the append-error protocol's report; the reservation
/// ledger's refusal after the append.
pub fn prepare_fast(
    journal: &mut dyn IntegrationJournal,
    request: &IntegrationRequest,
    head: CommitSha,
) -> Result<Authorized, UpstrokeError> {
    let candidate = &request.candidate;
    let prepared = MergePrepared {
        sequence: request.sequence,
        disposition: PreparedDisposition::Fast,
        expected_head: head.clone(),
        proposed_sha: candidate.commit_sha.clone(),
        key: candidate.key,
        generation: candidate.generation,
        candidate_sha: candidate.commit_sha.clone(),
        candidate_ref: candidate.candidate_ref.clone(),
        prepared_ref: None,
        verification_source: VerificationSource::CandidatePrepared {
            key: candidate.key,
            generation: candidate.generation,
        },
        verification: None,
        satisfies: request.satisfies.clone(),
    };
    journal.emit(TopologyEventBody::MergePrepared {
        data: Box::new(prepared),
    })?;
    journal.converted(candidate.key)?;
    Ok(Authorized {
        sequence: request.sequence,
        key: candidate.key,
        expected_head: head,
        proposed_sha: candidate.commit_sha.clone(),
        satisfies: request.satisfies.clone(),
        lease_release: request.lease_release.clone(),
        integration_ref: request.integration_ref.clone(),
        pin: None,
        staging: None,
    })
}

/// Complete an authorized publication: the compare-and-swap, then
/// `task_merged`, then the pruning the terminal permits.
///
/// `decisions.coordinator_integration.publish` and `cas_recovery`, as one
/// function for both: `assert_publishable`, read the ref, and then
///
/// * at `expected_head`: `update-ref --no-deref <ref> <proposed> <expected>`
///   through `Ref.CompareAndSwapIntegration` — for an already-present
///   publication the two are equal and Git validates the expected old without
///   moving anything;
/// * at `proposed_sha`: the move already happened and only the record is
///   owed;
/// * at anything else: refuse. A third SHA is foreign history.
///
/// `task_merged` is appended only after the ref is at `proposed_sha`, which
/// is INV-09's "CAS before `task_merged`". The pin and the staging worktree
/// of a stale publication go afterwards, because both keep the proposal
/// reachable until the integration ref does.
///
/// # Errors
///
/// A symbolic or checked-out ref, [`Refusal::IntegrationRefAbsent`],
/// [`Refusal::ThirdSha`], a Git error from the swap, the fold's refusal of
/// `task_merged`, the append-error protocol's report, or
/// [`Refusal::PinAtAnotherSha`].
pub fn publish(
    journal: &mut dyn IntegrationJournal,
    manager: &WorkspaceManager,
    authorized: Authorized,
) -> Result<Published, UpstrokeError> {
    let refname = authorized.integration_ref.as_str();
    manager.assert_publishable(refname)?;
    let found =
        manager
            .direct_ref_target(refname)?
            .ok_or_else(|| Refusal::IntegrationRefAbsent {
                refname: refname.to_owned(),
            })?;
    if found == authorized.expected_head.0 {
        manager.compare_and_swap_ref(
            journal.hooks().effects(),
            RefSite::CompareAndSwapIntegration,
            refname,
            authorized.expected_head.as_str(),
            authorized.proposed_sha.as_str(),
        )?;
    } else if found != authorized.proposed_sha.0 {
        return Err(Refusal::ThirdSha {
            sequence: authorized.sequence.0,
            refname: refname.to_owned(),
            found,
            expected: authorized.expected_head.0.clone(),
            proposed: authorized.proposed_sha.0.clone(),
        }
        .into());
    }

    journal.emit(TopologyEventBody::TaskMerged {
        data: TaskMerged {
            sequence: authorized.sequence,
            merged_sha: authorized.proposed_sha.clone(),
            satisfies: authorized.satisfies.clone(),
            lease_release: authorized.lease_release.clone(),
        },
    })?;

    if let Some(pin) = &authorized.pin {
        prune_pin(journal.hooks(), manager, pin, &authorized.proposed_sha)?;
    }
    if let Some(staging) = &authorized.staging {
        manager.remove_worktree(journal.hooks().effects(), staging)?;
        manager.remove_intent(journal.hooks().effects(), staging)?;
    }

    Ok(Published {
        sequence: authorized.sequence,
        key: authorized.key,
        merged_sha: authorized.proposed_sha,
        satisfies: authorized.satisfies,
    })
}

/// Delete a prepared pin expected-old at the proposal it recorded.
///
/// Absent is fine — a kill between two resumes may have pruned it already —
/// and a pin at another object refuses rather than deleting the evidence.
///
/// # Errors
///
/// [`Refusal::PinAtAnotherSha`] or a Git error.
pub fn prune_pin(
    hooks: &mut dyn TopologyHooks,
    manager: &WorkspaceManager,
    pin: &GitRef,
    proposed: &CommitSha,
) -> Result<(), UpstrokeError> {
    let Some(found) = manager.direct_ref_target(pin.as_str())? else {
        return Ok(());
    };
    if found != proposed.0 {
        return Err(Refusal::PinAtAnotherSha {
            refname: pin.0.clone(),
            found,
            expected: proposed.0.clone(),
        }
        .into());
    }
    manager.delete_ref_expected_old(
        hooks.effects(),
        RefSite::DeletePreparedPin,
        pin.as_str(),
        &found,
    )
}

/// One integration, from the decision to its terminal.
///
/// The reservation is the caller's: taken before this is entered and
/// cancelled by the caller if this returns before the first append converted
/// it.
///
/// # Errors
///
/// Any refusal of the sequence it runs, and — in this build —
/// [`Refusal::StaleNotImplemented`] for a candidate whose base is no longer
/// the head, refused before any staging effect.
pub fn integrate(
    journal: &mut dyn IntegrationJournal,
    manager: &WorkspaceManager,
    request: &IntegrationRequest,
) -> Result<Published, UpstrokeError> {
    let decided = decide(manager, request)?;
    match decided.exact_base {
        ExactBase::Fast => {
            let authorized = prepare_fast(journal, request, decided.head)?;
            publish(journal, manager, authorized)
        }
        ExactBase::Stale => Err(Refusal::StaleNotImplemented {
            key: request.candidate.key.0,
            generation: request.candidate.generation.0,
            head: decided.head.0,
            base: request.base_sha.0.clone(),
        }
        .into()),
    }
}

#[cfg(test)]
mod tests;
