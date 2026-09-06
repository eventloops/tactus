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
use crate::ladder::{AttemptFailure, FailureKind};
use crate::topology::effects::RefSite;
use crate::topology::events::{
    CandidateRef, CommitSha, FrozenQuestion, GitRef, InfrastructureKind, MergeLeaseRelease,
    MergePrepared, MergeVerificationStarted, MergeVerificationUnavailable, PreparedDisposition,
    RejectionDisposition, SequenceId, TaskMerged, TopologyEventBody, UnavailableCause,
    UnavailableOutcome, VerificationBasis, VerificationRecord, VerificationSource,
    VerificationVerdict,
};
use crate::topology::fold::{TopologyFold, TransactionClass};
use crate::topology::paths::PathSet;
use crate::topology::registry::TaskKey;
use crate::workspace_manager::{ProposalState, Slot, WorkspaceManager};

use super::attempt::Judgement;
use super::candidate::RUN_REF_ROOT;
use super::seams::{IdSource, TopologyHooks};

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

    /// The fold every append is checked against, read to derive a repair and a
    /// candidate's region.
    fn fold(&self) -> &TopologyFold;

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

/// What runs an integration verification and mints its park question.
///
/// Implemented by the same object as [`IntegrationJournal`], because both are
/// the run: the gates and reviewers execute through the run's own
/// [`super::attempt::Judge`] over its ledgers, and a park question is minted
/// from the run's [`IdSource`].
pub trait Verification {
    /// Run every recorded gate on one fresh exact snapshot of the proposed
    /// commit and every review pass on its own, reviewing it against the head,
    /// and say what they decided. `staging` is read only for the review diff;
    /// no gate or reviewer runs in it.
    ///
    /// # Errors
    ///
    /// A snapshot funnel refusal, a Runner error, or a plan the run cannot
    /// assemble.
    fn verify(&mut self, request: &VerifyRequest<'_>) -> Result<Judgement, UpstrokeError>;

    /// The id source a park question's identity comes from.
    fn ids(&self) -> &dyn IdSource;
}

/// One integration verification to run.
pub struct VerifyRequest<'a> {
    pub candidate: &'a CandidateRef,
    pub sequence: SequenceId,
    pub staging: &'a Slot,
    pub head: &'a CommitSha,
    pub proposed: &'a CommitSha,
    /// The proposal equals the head: gates rerun on the head and the review
    /// judges the head tree against the candidate's original patch.
    pub already_present: bool,
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
pub fn integrate<J: IntegrationJournal + Verification>(
    journal: &mut J,
    manager: &WorkspaceManager,
    request: &IntegrationRequest,
) -> Result<Terminal, UpstrokeError> {
    let decided = decide(manager, request)?;
    match decided.exact_base {
        ExactBase::Fast => {
            let authorized = prepare_fast(journal, request, decided.head)?;
            Ok(Terminal::Merged(publish(journal, manager, authorized)?))
        }
        ExactBase::Stale => integrate_stale(journal, manager, request, decided.head),
    }
}

/// The terminal one integration reached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Terminal {
    /// A publication: `merge_prepared`, the compare-and-swap, `task_merged`.
    Merged(Published),
    /// A conflict or a code-attributed rejection, with the repair registered.
    Rejected { sequence: SequenceId, key: TaskKey },
    /// The verification could not be run: deferred, or parked.
    Unavailable {
        sequence: SequenceId,
        key: TaskKey,
        parked: bool,
    },
}

/// What a cherry-pick left in the staging worktree.
enum Picked {
    Clean { proposal: CommitSha },
    Conflict { paths: PathSet },
    Empty,
    Unclassified { detail: String },
}

/// The stale path: cherry-pick the immutable candidate onto the head in a
/// staging worktree, classify what the pick left, and reach the terminal the
/// classification and (for a clean or empty pick) the verification decide.
fn integrate_stale<J: IntegrationJournal + Verification>(
    journal: &mut J,
    manager: &WorkspaceManager,
    request: &IntegrationRequest,
    head: CommitSha,
) -> Result<Terminal, UpstrokeError> {
    let candidate = &request.candidate;
    let staging = staging_slot(request.sequence);

    manager.write_intent(journal.hooks().effects(), &staging)?;
    manager.add_worktree(journal.hooks().effects(), &staging, head.as_str())?;

    let picked = manager.proposal_cherry_pick(
        journal.hooks().effects(),
        &staging,
        candidate.commit_sha.as_str(),
    );
    let classified = match &picked {
        Ok(proposal) => Picked::Clean {
            proposal: CommitSha(proposal.clone()),
        },
        Err(_) => match manager.proposal_state(&staging, head.as_str())? {
            ProposalState::Conflict { paths } => Picked::Conflict { paths },
            ProposalState::Empty => Picked::Empty,
            ProposalState::Unclassified { detail } => Picked::Unclassified { detail },
        },
    };

    match classified {
        Picked::Clean { proposal } => {
            let run_id = run_id_of(journal)?;
            let pin = prepared_pin_ref(&run_id, request.sequence);
            manager.create_ref_zero_old(
                journal.hooks().effects(),
                RefSite::PinPrepared,
                pin.as_str(),
                proposal.as_str(),
            )?;
            start_and_verify(
                journal,
                manager,
                request,
                &staging,
                &head,
                &proposal,
                Some(pin.clone()),
                VerificationBasis::StaleClean { prepared_ref: pin },
                PreparedDisposition::StaleClean,
            )
        }
        Picked::Empty => start_and_verify(
            journal,
            manager,
            request,
            &staging,
            &head,
            &head.clone(),
            None,
            VerificationBasis::AlreadyPresent,
            PreparedDisposition::AlreadyPresent,
        ),
        Picked::Conflict { paths } => {
            let rejected = super::repair::merge_rejected(
                journal.fold(),
                journal.ids(),
                candidate,
                head.clone(),
                request.sequence,
                RejectionDisposition::Conflict {
                    paths: paths.clone(),
                },
                paths,
            )?;
            let sequence = rejected.sequence;
            let key = candidate.key;
            journal.emit(TopologyEventBody::MergeRejected {
                data: Box::new(rejected),
            })?;
            journal.converted(key)?;
            reclaim_staging(journal, manager, &staging, None)?;
            Ok(Terminal::Rejected { sequence, key })
        }
        Picked::Unclassified { detail } => {
            reclaim_staging(journal, manager, &staging, None)?;
            Err(picked.err().unwrap_or_else(|| {
                refused(&format!(
                    "the proposal cherry-pick of candidate {} left an unclassifiable staging \
                     state: {detail}",
                    candidate.commit_sha
                ))
            }))
        }
    }
}

/// Append `merge_verification_started`, run the verification, and reach the
/// terminal the judgement decides.
#[allow(clippy::too_many_arguments)]
fn start_and_verify<J: IntegrationJournal + Verification>(
    journal: &mut J,
    manager: &WorkspaceManager,
    request: &IntegrationRequest,
    staging: &Slot,
    head: &CommitSha,
    proposed: &CommitSha,
    pin: Option<GitRef>,
    basis: VerificationBasis,
    disposition: PreparedDisposition,
) -> Result<Terminal, UpstrokeError> {
    let candidate = &request.candidate;
    journal.emit(TopologyEventBody::MergeVerificationStarted {
        data: MergeVerificationStarted {
            sequence: request.sequence,
            candidate: candidate.clone(),
            basis,
            expected_head: head.clone(),
            proposed_sha: proposed.clone(),
        },
    })?;
    journal.converted(candidate.key)?;

    let already_present = disposition == PreparedDisposition::AlreadyPresent;
    let judgement = journal.verify(&VerifyRequest {
        candidate,
        sequence: request.sequence,
        staging,
        head,
        proposed,
        already_present,
    })?;

    match judgement.failure.clone() {
        None => {
            let record = passing_record(&judgement);
            let authorized =
                prepare_verified(journal, request, head, proposed, pin, disposition, record)?;
            Ok(Terminal::Merged(publish(journal, manager, authorized)?))
        }
        Some(failure) if failure.is_outage() => unavailable(
            journal,
            manager,
            request,
            staging,
            pin,
            infrastructure(&failure),
        ),
        Some(failure) if needs_human(&failure) => unavailable(
            journal,
            manager,
            request,
            staging,
            pin,
            UnavailableCause::HumanRequired {
                verdict: failure.reason.clone(),
            },
        ),
        Some(failure) => {
            let record = code_record(&judgement, &failure);
            let rejected = super::repair::merge_rejected(
                journal.fold(),
                journal.ids(),
                candidate,
                head.clone(),
                request.sequence,
                RejectionDisposition::CodeRejected {
                    verification: record,
                },
                candidate_region(journal.fold(), candidate),
            )?;
            let sequence = rejected.sequence;
            let key = candidate.key;
            journal.emit(TopologyEventBody::MergeRejected {
                data: Box::new(rejected),
            })?;
            reclaim_staging(journal, manager, staging, pin.as_ref())?;
            Ok(Terminal::Rejected { sequence, key })
        }
    }
}

/// Authorize a verified publication, checked by the fold's `merge_prepared`
/// relations for stale_clean and already_present.
#[allow(clippy::too_many_arguments)]
fn prepare_verified(
    journal: &mut dyn IntegrationJournal,
    request: &IntegrationRequest,
    head: &CommitSha,
    proposed: &CommitSha,
    pin: Option<GitRef>,
    disposition: PreparedDisposition,
    record: VerificationRecord,
) -> Result<Authorized, UpstrokeError> {
    let candidate = &request.candidate;
    journal.emit(TopologyEventBody::MergePrepared {
        data: Box::new(MergePrepared {
            sequence: request.sequence,
            disposition,
            expected_head: head.clone(),
            proposed_sha: proposed.clone(),
            key: candidate.key,
            generation: candidate.generation,
            candidate_sha: candidate.commit_sha.clone(),
            candidate_ref: candidate.candidate_ref.clone(),
            prepared_ref: pin.clone(),
            verification_source: VerificationSource::Verification {
                sequence: request.sequence,
            },
            verification: Some(record),
            satisfies: request.satisfies.clone(),
        }),
    })?;
    Ok(Authorized {
        sequence: request.sequence,
        key: candidate.key,
        expected_head: head.clone(),
        proposed_sha: proposed.clone(),
        satisfies: request.satisfies.clone(),
        lease_release: request.lease_release.clone(),
        integration_ref: request.integration_ref.clone(),
        pin,
        staging: Some(staging_slot(request.sequence)),
    })
}

/// A verification outage: `merge_verification_unavailable`, deferred while the
/// candidate is inside its frozen allowance and parked at it, then the staging
/// worktree and the pin reclaimed.
fn unavailable<J: IntegrationJournal + Verification>(
    journal: &mut J,
    manager: &WorkspaceManager,
    request: &IntegrationRequest,
    staging: &Slot,
    pin: Option<GitRef>,
    cause: UnavailableCause,
) -> Result<Terminal, UpstrokeError> {
    let candidate = &request.candidate;
    let taken = candidate_defers(journal.fold(), candidate);
    let max = journal
        .fold()
        .started()
        .ok_or_else(|| refused("the run has not started"))?
        .limits
        .max_defers;
    let is_outage = matches!(cause, UnavailableCause::Infrastructure { .. });
    let outcome = if is_outage && taken.saturating_add(1) < max {
        UnavailableOutcome::Deferred {
            defers: taken.saturating_add(1),
        }
    } else {
        UnavailableOutcome::Parked {
            question: park_question(journal, candidate.key, &cause),
        }
    };
    let parked = matches!(outcome, UnavailableOutcome::Parked { .. });
    journal.emit(TopologyEventBody::MergeVerificationUnavailable {
        data: MergeVerificationUnavailable {
            sequence: request.sequence,
            cause,
            outcome,
        },
    })?;
    reclaim_staging(journal, manager, staging, pin.as_ref())?;
    Ok(Terminal::Unavailable {
        sequence: request.sequence,
        key: candidate.key,
        parked,
    })
}

fn park_question<J: Verification>(
    journal: &J,
    key: TaskKey,
    cause: &UnavailableCause,
) -> FrozenQuestion {
    let (kind, context) = match cause {
        UnavailableCause::HumanRequired { verdict } => (
            crate::ir::QuestionKind::Clarify,
            format!("integration verification needs a person: {verdict}"),
        ),
        UnavailableCause::Infrastructure { kind } => (
            crate::ir::QuestionKind::Unblock,
            format!(
                "integration verification kept failing on infrastructure ({kind:?}) and has \
                 exhausted its deferrals; retry it or decline the task"
            ),
        ),
    };
    FrozenQuestion {
        id: journal.ids().question_id(),
        key,
        kind,
        context,
        options: crate::engine::coordinator::question_options(kind),
    }
}

fn infrastructure(failure: &AttemptFailure) -> UnavailableCause {
    let kind = match failure.kind {
        FailureKind::RateLimited => InfrastructureKind::RateLimited,
        FailureKind::ReviewUnavailable => InfrastructureKind::ReviewUnavailable,
        FailureKind::Timeout => InfrastructureKind::ReviewerTimeout,
        _ => InfrastructureKind::Other {
            detail: failure.reason.clone(),
        },
    };
    UnavailableCause::Infrastructure { kind }
}

fn needs_human(failure: &AttemptFailure) -> bool {
    matches!(
        failure.kind,
        FailureKind::NeedsHuman | FailureKind::ReviewInputTooLarge | FailureKind::ReviewInputOpaque
    )
}

fn passing_record(judgement: &Judgement) -> VerificationRecord {
    VerificationRecord {
        verdict: VerificationVerdict::Passed,
        gates_passed: true,
        reviews: judgement.reviews.clone(),
        detail: "the integration verification passed".to_owned(),
    }
}

fn code_record(judgement: &Judgement, failure: &AttemptFailure) -> VerificationRecord {
    let gates_passed = !matches!(failure.kind, FailureKind::GateFailed);
    super::repair::code_rejection_record(
        gates_passed,
        judgement.reviews.clone(),
        failure.reason.clone(),
    )
}

fn candidate_region(fold: &TopologyFold, candidate: &CandidateRef) -> PathSet {
    fold.task(candidate.key)
        .and_then(|task| {
            task.generations
                .iter()
                .find(|generation| generation.id == candidate.generation)
        })
        .and_then(|generation| generation.candidate.as_ref())
        .map_or(PathSet::RepoWide, |prepared| prepared.paths.clone())
}

fn candidate_defers(fold: &TopologyFold, candidate: &CandidateRef) -> u32 {
    fold.queue()
        .and_then(|queue| queue.get(candidate.key, candidate.generation))
        .map_or(0, |entry| entry.defers)
}

fn run_id_of<J: IntegrationJournal>(journal: &J) -> Result<String, UpstrokeError> {
    Ok(journal
        .fold()
        .started()
        .ok_or_else(|| refused("the run has not started"))?
        .run_id
        .clone())
}

/// Remove a stale transaction's staging worktree with force, then its intent,
/// then delete its pin expected-old.
fn reclaim_staging(
    journal: &mut dyn IntegrationJournal,
    manager: &WorkspaceManager,
    staging: &Slot,
    pin: Option<&GitRef>,
) -> Result<(), UpstrokeError> {
    manager.remove_worktree(journal.hooks().effects(), staging)?;
    manager.remove_intent(journal.hooks().effects(), staging)?;
    if let Some(pin) = pin {
        if let Some(proposed) = manager.direct_ref_target(pin.as_str())? {
            manager.delete_ref_expected_old(
                journal.hooks().effects(),
                RefSite::DeletePreparedPin,
                pin.as_str(),
                &proposed,
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
