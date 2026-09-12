//! Extended notes: `docs/internals/engine/topology/integrate.md`

use thiserror::Error;

use crate::error::UpstrokeError;
use crate::ladder::{AttemptFailure, FailureKind};
use crate::topology::effects::RefSite;
use crate::topology::events::{
    CandidateRef, CommitSha, FrozenQuestion, GitRef, InfrastructureKind, MergeLeaseRelease,
    MergePrepared, MergeVerificationStarted, MergeVerificationUnavailable, PreparedDisposition,
    RejectionDisposition, RunStarted4, SequenceId, TaskMerged, TopologyEvent, TopologyEventBody,
    UnavailableCause, UnavailableOutcome, VerificationBasis, VerificationRecord,
    VerificationSource, VerificationVerdict,
};
use crate::topology::fold::{TopologyFold, TransactionClass};
use crate::topology::paths::PathSet;
use crate::topology::registry::TaskKey;
use crate::workspace_manager::{ProposalState, Slot, WorkspaceManager};

use super::attempt::Judgement;
use super::candidate::RUN_REF_ROOT;
use super::create::IntegrationRefs;
use super::seams::{IdSource, TopologyHooks};

#[must_use]
pub fn prepared_pin_ref(run_id: &str, sequence: SequenceId) -> GitRef {
    GitRef(format!("{RUN_REF_ROOT}/{run_id}/prepared/{}", sequence.0))
}

#[must_use]
pub fn staging_slot(sequence: SequenceId) -> Slot {
    Slot::Staging {
        sequence: u64::from(sequence.0),
    }
}

pub trait IntegrationJournal {
    fn emit(&mut self, body: TopologyEventBody) -> Result<(), UpstrokeError>;

    fn fold(&self) -> &TopologyFold;

    fn hooks(&mut self) -> &mut dyn TopologyHooks;

    fn converted(&mut self, key: TaskKey) -> Result<(), UpstrokeError>;
}

pub trait Verification {
    fn verify(&mut self, request: &VerifyRequest<'_>) -> Result<Verified, UpstrokeError>;

    fn ids(&self) -> &dyn IdSource;
}

pub enum Verified {
    Judged(Judgement),
    Unavailable {
        kind: InfrastructureKind,
        detail: String,
        reviews: Vec<crate::events::ReviewRecord>,
    },
}

pub struct VerifyRequest<'a> {
    pub candidate: &'a CandidateRef,
    pub sequence: SequenceId,
    pub staging: &'a Slot,
    pub head: &'a CommitSha,
    pub proposed: &'a CommitSha,
    pub already_present: bool,
}

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
        "refusing to open sequence {sequence}: the integration ref `{refname}` is at {found}, and \
         the log authorizes {authorized} ({authority}); a head the log did not put there is \
         foreign integration state, so nothing is decided, staged or appended against it, and \
         the ref is neither moved nor recreated \
         (decisions.coordinator_integration.integration_sequence; DESIGN §26)"
    )]
    ForeignHead {
        sequence: u32,
        refname: String,
        found: String,
        authorized: String,
        authority: String,
    },

    #[error(
        "refusing to dispatch task {key}: the integration ref `{refname}` is at {found}, and the \
         log authorizes {authorized} ({authority}); a dispatch takes the run's integration head \
         at dispatch, and a head the log did not put there is foreign integration state, so no \
         worktree is created and nothing is appended against it \
         (DESIGN §26 verdict 1; decisions.coordinator_integration.integration_sequence)"
    )]
    DispatchHeadForeign {
        key: u32,
        refname: String,
        found: String,
        authorized: String,
        authority: String,
    },

    #[error(
        "refusing to dispatch task {key}: the integration ref `{refname}` names nothing, and the \
         log authorizes {authorized} ({authority}); a dispatch takes the run's integration head \
         at dispatch and never recreates the ref to find one, so no worktree is created and \
         nothing is appended (DESIGN §26 verdict 1)"
    )]
    DispatchHeadAbsent {
        key: u32,
        refname: String,
        authorized: String,
        authority: String,
    },

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
        "refusing to resume sequence {sequence}: its pin `{refname}` is at {found} and the \
         verification recorded the proposal {expected}; a substituted ref is neither adopted nor \
         deleted, and the transaction stays open until the pin names its record again"
    )]
    PinSubstituted {
        sequence: u32,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrationRequest {
    pub candidate: CandidateRef,
    pub sequence: SequenceId,
    pub base_sha: CommitSha,
    pub authorized: AuthorizedHead,
    pub satisfies: Vec<TaskKey>,
    pub lease_release: MergeLeaseRelease,
    pub integration_ref: GitRef,
}

impl IntegrationRequest {
    pub fn from_log(
        fold: &TopologyFold,
        events: &[TopologyEvent],
        candidate: &CandidateRef,
    ) -> Result<Self, UpstrokeError> {
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
            authorized: authorized_head(started, events),
            satisfies,
            lease_release: lease_release(fold, candidate),
            integration_ref: started.integration_ref.clone(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedHead {
    pub head: CommitSha,
    pub published_by: Option<SequenceId>,
}

impl AuthorizedHead {
    #[must_use]
    pub fn describe(&self) -> String {
        match self.published_by {
            Some(sequence) => format!("the publication of sequence {} put it there", sequence.0),
            None => "the run recorded it as its base and has published nothing yet".to_owned(),
        }
    }
}

#[must_use]
pub fn authorized_head(started: &RunStarted4, events: &[TopologyEvent]) -> AuthorizedHead {
    events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            TopologyEventBody::TaskMerged { data } => Some(AuthorizedHead {
                head: data.merged_sha.clone(),
                published_by: Some(data.sequence),
            }),
            _ => None,
        })
        .unwrap_or_else(|| AuthorizedHead {
            head: started.base_sha.clone(),
            published_by: None,
        })
}

pub fn dispatch_head(
    refs: &dyn IntegrationRefs,
    started: &RunStarted4,
    events: &[TopologyEvent],
    key: TaskKey,
) -> Result<CommitSha, UpstrokeError> {
    let authorized = authorized_head(started, events);
    let authority = authorized.describe();
    let refname = started.integration_ref.as_str();
    refs.assert_publishable(refname)?;
    match refs.direct_target(refname)? {
        Some(found) if found == authorized.head.0 => Ok(authorized.head),
        Some(found) => Err(Refusal::DispatchHeadForeign {
            key: key.0,
            refname: refname.to_owned(),
            found,
            authorized: authorized.head.0,
            authority,
        }
        .into()),
        None => Err(Refusal::DispatchHeadAbsent {
            key: key.0,
            refname: refname.to_owned(),
            authorized: authorized.head.0,
            authority,
        }
        .into()),
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExactBase {
    Fast,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "a decision is followed by the sequence it selects"]
pub struct Decided {
    pub head: CommitSha,
    pub exact_base: ExactBase,
}

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
    if head != request.authorized.head {
        return Err(Refusal::ForeignHead {
            sequence: request.sequence.0,
            refname: refname.to_owned(),
            found: head.0,
            authorized: request.authorized.head.0.clone(),
            authority: request.authorized.describe(),
        }
        .into());
    }
    let exact_base = if head == request.base_sha {
        ExactBase::Fast
    } else {
        ExactBase::Stale
    };
    Ok(Decided { head, exact_base })
}

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
    pub pin: Option<GitRef>,
    pub staging: Option<Slot>,
}

impl Authorized {
    pub fn from_fold(fold: &TopologyFold) -> Result<Option<Self>, UpstrokeError> {
        let Some(transaction) = fold.transaction() else {
            return Ok(None);
        };
        let TransactionClass::Prepared {
            expected_head,
            proposed_sha,
            satisfies,
            disposition,
            prepared_ref,
        } = &transaction.class
        else {
            return Ok(None);
        };
        let started = fold
            .started()
            .ok_or_else(|| refused("the proven prefix records a transaction and no run"))?;
        let candidate = &transaction.candidate;
        let staged = *disposition != PreparedDisposition::Fast;
        Ok(Some(Self {
            sequence: transaction.sequence,
            key: candidate.key,
            expected_head: expected_head.clone(),
            proposed_sha: proposed_sha.clone(),
            satisfies: satisfies.clone(),
            lease_release: lease_release(fold, candidate),
            integration_ref: started.integration_ref.clone(),
            pin: prepared_ref.clone(),
            staging: staged.then(|| staging_slot(transaction.sequence)),
        }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    pub sequence: SequenceId,
    pub key: TaskKey,
    pub merged_sha: CommitSha,
    pub satisfies: Vec<TaskKey>,
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Terminal {
    Merged(Published),
    Rejected {
        sequence: SequenceId,
        key: TaskKey,
    },
    Unavailable {
        sequence: SequenceId,
        key: TaskKey,
        parked: bool,
    },
}

enum Picked {
    Clean { proposal: CommitSha },
    Conflict { paths: PathSet },
    Empty,
    Unclassified { detail: String },
}

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
    let judgement = match journal.verify(&VerifyRequest {
        candidate,
        sequence: request.sequence,
        staging,
        head,
        proposed,
        already_present,
    })? {
        Verified::Judged(judgement) => judgement,
        Verified::Unavailable {
            kind,
            detail,
            reviews,
        } => {
            return unavailable(
                journal,
                manager,
                request,
                staging,
                pin,
                proposed,
                UnavailableCause::Infrastructure { kind },
                Some(detail),
                reviews,
            );
        }
    };

    if let Some(verdict) = judgement.timed_out_gate() {
        let detail = format!(
            "gate `{}` timed out and produced no verdict; a timeout is not a repair \
             (decisions.repairs.not_repairs)",
            verdict.invocation
        );
        return unavailable(
            journal,
            manager,
            request,
            staging,
            pin,
            proposed,
            UnavailableCause::Infrastructure {
                kind: InfrastructureKind::Other {
                    detail: detail.clone(),
                },
            },
            Some(detail),
            judgement.reviews.clone(),
        );
    }

    match judgement.failure.clone() {
        None => {
            let record = passing_record(&judgement);
            let authorized =
                prepare_verified(journal, request, head, proposed, pin, disposition, record)?;
            reclaim_snapshots(journal, manager)?;
            Ok(Terminal::Merged(publish(journal, manager, authorized)?))
        }
        Some(failure) if failure.is_outage() => unavailable(
            journal,
            manager,
            request,
            staging,
            pin,
            proposed,
            infrastructure(&failure),
            Some(failure.reason.clone()),
            judgement.reviews.clone(),
        ),
        Some(failure) if needs_human(&failure) => unavailable(
            journal,
            manager,
            request,
            staging,
            pin,
            proposed,
            UnavailableCause::HumanRequired {
                verdict: failure.reason.clone(),
            },
            None,
            judgement.reviews.clone(),
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
            reclaim_staging(
                journal,
                manager,
                staging,
                pin.as_ref().map(|pin| (pin, proposed)),
            )?;
            Ok(Terminal::Rejected { sequence, key })
        }
    }
}

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

#[allow(clippy::too_many_arguments)]
fn unavailable<J: IntegrationJournal + Verification>(
    journal: &mut J,
    manager: &WorkspaceManager,
    request: &IntegrationRequest,
    staging: &Slot,
    pin: Option<GitRef>,
    proposed: &CommitSha,
    cause: UnavailableCause,
    detail: Option<String>,
    reviews: Vec<crate::events::ReviewRecord>,
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
            question: park_question(journal, candidate.key, &cause, detail.as_deref()),
        }
    };
    let parked = matches!(outcome, UnavailableOutcome::Parked { .. });
    journal.emit(TopologyEventBody::MergeVerificationUnavailable {
        data: MergeVerificationUnavailable {
            sequence: request.sequence,
            cause,
            outcome,
            reviews,
        },
    })?;
    reclaim_staging(
        journal,
        manager,
        staging,
        pin.as_ref().map(|pin| (pin, proposed)),
    )?;
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
    detail: Option<&str>,
) -> FrozenQuestion {
    let (kind, context) = match cause {
        UnavailableCause::HumanRequired { verdict } => (
            crate::ir::QuestionKind::Clarify,
            format!("integration verification needs a person: {verdict}"),
        ),
        UnavailableCause::Infrastructure { kind } => (
            crate::ir::QuestionKind::Unblock,
            format!(
                "integration verification kept failing on infrastructure ({kind:?}{}) and has \
                 exhausted its deferrals; retry it or decline the task",
                detail
                    .map(|detail| format!(": {detail}"))
                    .unwrap_or_default()
            ),
        ),
    };
    FrozenQuestion {
        id: journal.ids().question_id(),
        key,
        kind,
        context,
        options: crate::engine::coordinator::topology_question_options(kind),
    }
}

fn infrastructure(failure: &AttemptFailure) -> UnavailableCause {
    if failure.never_started {
        return UnavailableCause::Infrastructure {
            kind: InfrastructureKind::RunnerSpawnFailure,
        };
    }
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

pub(super) fn code_record(judgement: &Judgement, failure: &AttemptFailure) -> VerificationRecord {
    let gates_passed = !matches!(failure.kind, FailureKind::GateFailed);
    super::repair::code_rejection_record(
        gates_passed,
        judgement.reviews.clone(),
        rejection_detail(failure),
    )
}

fn rejection_detail(failure: &AttemptFailure) -> String {
    let Some(feedback) = failure
        .feedback
        .as_deref()
        .map(str::trim)
        .filter(|feedback| !feedback.is_empty())
    else {
        return failure.reason.clone();
    };
    format!(
        "{}\n\n{}",
        failure.reason,
        crate::util::tail(feedback, crate::gates::FEEDBACK_TAIL_BYTES)
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

fn reclaim_snapshots(
    journal: &mut dyn IntegrationJournal,
    manager: &WorkspaceManager,
) -> Result<(), UpstrokeError> {
    for slot in manager.intents()? {
        if matches!(slot, Slot::Snapshot { .. }) {
            manager.remove_worktree(journal.hooks().effects(), &slot)?;
            manager.remove_intent(journal.hooks().effects(), &slot)?;
        }
    }
    Ok(())
}

fn reclaim_staging(
    journal: &mut dyn IntegrationJournal,
    manager: &WorkspaceManager,
    staging: &Slot,
    pinned: Option<(&GitRef, &CommitSha)>,
) -> Result<(), UpstrokeError> {
    reclaim_snapshots(journal, manager)?;
    manager.remove_worktree(journal.hooks().effects(), staging)?;
    manager.remove_intent(journal.hooks().effects(), staging)?;
    if let Some((pin, proposed)) = pinned {
        prune_pin(journal.hooks(), manager, pin, proposed)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
