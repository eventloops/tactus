//! Tests for the integration transaction.

use super::*;
use crate::engine::topology::identity::ReservationKind;
use crate::engine::topology::scaffold::{ALPHA, BETA, Run};
use crate::topology::effects::{EffectSiteId, HookPhase, ObjectSite, WorktreeSite};
use crate::topology::events::{GenerationId, TopologyEvent};
use crate::topology::fold::{TaskState, TopologyFold};
use crate::workspace_manager::fixture::git;

fn count_objects(run: &Run) -> u64 {
    let listing = git(&run.fixture.base, &["count-objects", "-v"]);
    listing
        .lines()
        .filter_map(|line| line.split_once(": "))
        .filter(|(name, _)| *name == "count" || *name == "in-pack")
        .map(|(_, value)| value.trim().parse::<u64>().expect("a count"))
        .sum()
}

/// Integrate `candidate` as the loop would: the reservation before any effect,
/// the sequence, and the reservation cancelled when the sequence ended before
/// its first append.
fn integrate_through(run: &mut Run, candidate: &CandidateRef) -> Result<Published, UpstrokeError> {
    let request = IntegrationRequest::from_fold(run.emitter.fold(), candidate)?;
    run.reservations
        .take(candidate.key, ReservationKind::Integration)?;
    let manager = run.fixture.manager.clone();
    let outcome = integrate(run, &manager, &request);
    if outcome.is_err() && !run.reservations.is_empty() {
        run.reservations
            .cancel(candidate.key, ReservationKind::Integration)?;
    }
    outcome
}

fn merge_prepared_of(run: &Run) -> MergePrepared {
    run.emitter
        .durable_events()
        .into_iter()
        .find_map(|event| match event.body {
            TopologyEventBody::MergePrepared { data } => Some(*data),
            _ => None,
        })
        .expect("a durable merge_prepared")
}

#[test]
fn fast_path_publishes_exact_candidate_without_staging_or_proposal_object() {
    let mut run = Run::started("fast-path");
    let candidate = run.queue_candidate(ALPHA);
    assert_eq!(run.head().as_deref(), Some(run.base().as_str()));
    let objects_before = count_objects(&run);
    let kinds_before = run.emitter.durable_kinds();
    let mark = run.mark();

    run.harness
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .begin_fast_sequence("s0");
    let published = integrate_through(&mut run, &candidate).expect("the fast path publishes");
    run.harness
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .end_fast_sequence();

    assert_eq!(published.sequence, SequenceId(0));
    assert_eq!(published.merged_sha, candidate.commit_sha);
    assert_eq!(published.satisfies, vec![ALPHA]);

    // The integration ref moved onto the very object gated at candidate_prepared.
    assert_eq!(run.head().as_deref(), Some(candidate.commit_sha.as_str()));
    assert_eq!(
        count_objects(&run),
        objects_before,
        "a fast publication creates no object: nothing was cherry-picked, snapshotted or pinned"
    );

    let mut expected = kinds_before;
    expected.push("merge_prepared");
    expected.push("task_merged");
    assert_eq!(run.emitter.durable_kinds(), expected);

    let prepared = merge_prepared_of(&run);
    assert_eq!(prepared.disposition, PreparedDisposition::Fast);
    assert_eq!(prepared.expected_head, run.base());
    assert_eq!(prepared.proposed_sha, candidate.commit_sha);
    assert_eq!(prepared.candidate_sha, candidate.commit_sha);
    assert_eq!(prepared.prepared_ref, None);
    assert_eq!(prepared.verification, None);
    assert_eq!(
        prepared.verification_source,
        VerificationSource::CandidatePrepared {
            key: ALPHA,
            generation: GenerationId(0)
        }
    );

    // The hook harness recorded no staging, cherry-pick or prepared-pin site
    // inside the fast sequence, and did record the swap.
    {
        let harness = run
            .harness
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let sequence = harness
            .fast_sequence("s0")
            .expect("the suite began the sequence");
        for site in [
            EffectSiteId::Worktree(WorktreeSite::WriteStagingIntent),
            EffectSiteId::Worktree(WorktreeSite::AddStaging),
            EffectSiteId::Object(ObjectSite::ProposalCherryPick),
            EffectSiteId::Ref(RefSite::PinPrepared),
        ] {
            assert!(
                !sequence.ran(site),
                "`{site}` executed inside the fast sequence"
            );
        }
        assert!(
            sequence.ran(EffectSiteId::Ref(RefSite::CompareAndSwapIntegration)),
            "the sequence never swapped the ref"
        );
    }
    assert!(
        run.fixture
            .manager
            .intents()
            .expect("intents")
            .iter()
            .all(|slot| !matches!(slot, crate::workspace_manager::Slot::Staging { .. })),
        "a `merge/s<seq>` intent was written for a fast sequence"
    );

    // merge_prepared before the CAS, the CAS before task_merged.
    let prepared_at = run.order_after(
        mark,
        EffectSiteId::Event(crate::topology::effects::EventSite::Append),
        HookPhase::After,
    );
    let swapped_at = run.order_after(
        mark,
        EffectSiteId::Ref(RefSite::CompareAndSwapIntegration),
        HookPhase::Before,
    );
    let merged_at = run.order_after(
        swapped_at,
        EffectSiteId::Event(crate::topology::effects::EventSite::Append),
        HookPhase::Before,
    );
    assert!(
        prepared_at < swapped_at && swapped_at < merged_at,
        "the order was prepared {prepared_at}, swap {swapped_at}, merged {merged_at}"
    );

    // Every task of the closure is Merged, the queue position is consumed, and
    // both entitlements are released exactly once.
    assert_eq!(run.task_state(ALPHA), TaskState::Merged);
    assert!(run.emitter.fold().transaction().is_none());
    assert!(run.emitter.fold().queue().expect("started").is_empty());
    assert_eq!(run.emitter.fold().pipeline_held(), 0);
    assert!(run.reservations.balances());
    assert!(
        !run.emitter
            .fold()
            .leases()
            .expect("started")
            .any_candidate_or_lineage(),
        "the candidate lease was released at task_merged"
    );
    run.replay_twice_equal();
}

#[test]
fn fast_dual_holding_released_once() {
    let mut run = Run::started("fast-dual-holding");
    let candidate = run.queue_candidate(ALPHA);
    let request = IntegrationRequest::from_fold(run.emitter.fold(), &candidate).expect("request");
    let manager = run.fixture.manager.clone();

    run.reservations
        .take(ALPHA, ReservationKind::Integration)
        .expect("the provisional pair");
    assert_eq!(run.reservations.entitlements_held(), 2);
    assert_eq!(
        run.emitter.fold().pipeline_held(),
        0,
        "a queued candidate holds no fold-derived entitlement"
    );

    let decided = decide(&manager, &request).expect("the head reads");
    assert_eq!(decided.exact_base, ExactBase::Fast);
    let authorized = prepare_fast(&mut run, &request, decided.head).expect("merge_prepared(fast)");

    // PreCAS: the pair converted at the append and both holdings are the fold's.
    assert!(
        run.reservations.is_empty(),
        "the provisional pair converts at merge_prepared(fast)"
    );
    assert_eq!(run.emitter.fold().pipeline_held(), 1, "the pipeline half");
    assert!(run.emitter.fold().transaction().is_some(), "the merge half");
    assert!(!run.emitter.fold().pipeline_reservable());

    let published = publish(&mut run, &manager, authorized).expect("publish");
    assert_eq!(published.merged_sha, candidate.commit_sha);
    assert_eq!(run.emitter.fold().pipeline_held(), 0);
    assert!(run.emitter.fold().transaction().is_none());
    assert!(run.reservations.balances(), "taken once, converted once");
    run.replay_twice_equal();
}

#[test]
fn merge_prepared_fast_with_moved_head_or_wrong_proposed_or_pin_refused_live_and_on_replay() {
    let mut run = Run::started("fast-relations");
    let candidate = run.queue_candidate(ALPHA);
    let request = IntegrationRequest::from_fold(run.emitter.fold(), &candidate).expect("request");
    let sibling = run.queue_candidate(BETA);

    let honest = MergePrepared {
        sequence: request.sequence,
        disposition: PreparedDisposition::Fast,
        expected_head: run.base(),
        proposed_sha: candidate.commit_sha.clone(),
        key: ALPHA,
        generation: GenerationId(0),
        candidate_sha: candidate.commit_sha.clone(),
        candidate_ref: candidate.candidate_ref.clone(),
        prepared_ref: None,
        verification_source: VerificationSource::CandidatePrepared {
            key: ALPHA,
            generation: GenerationId(0),
        },
        verification: None,
        satisfies: vec![ALPHA],
    };
    let event = |prepared: MergePrepared| TopologyEvent {
        ts: "2026-09-06T12:00:00Z".to_owned(),
        body: TopologyEventBody::MergePrepared {
            data: Box::new(prepared),
        },
    };
    run.emitter
        .fold()
        .plan_transition(&event(honest.clone()))
        .expect("the honest fast publication is accepted, so the refusals below are its relations");

    let forged: Vec<(&str, MergePrepared)> = vec![
        (
            "a moved head",
            MergePrepared {
                expected_head: sibling.commit_sha.clone(),
                ..honest.clone()
            },
        ),
        (
            "a proposed commit that is not the candidate",
            MergePrepared {
                proposed_sha: sibling.commit_sha.clone(),
                candidate_sha: sibling.commit_sha.clone(),
                ..honest.clone()
            },
        ),
        (
            "a prepared pin",
            MergePrepared {
                prepared_ref: Some(prepared_pin_ref(
                    "01SCAFFOLD00000000000000AA",
                    SequenceId(0),
                )),
                ..honest.clone()
            },
        ),
    ];
    let log = run.emitter.durable_events();
    let inputs = crate::topology::fold::FrozenInputs {
        plan: crate::engine::topology::scaffold::plan(),
        normalized_plan_digest: crate::engine::topology::scaffold::NORMALIZED_DIGEST.to_owned(),
    };
    for (label, prepared) in forged {
        let forged = event(prepared);
        let live = run
            .emitter
            .fold()
            .plan_transition(&forged)
            .expect_err(label);
        let mut replayed = log.clone();
        replayed.push(forged);
        let on_replay = TopologyFold::replay(inputs.clone(), &replayed)
            .err()
            .unwrap_or_else(|| panic!("{label}: a replay of the log plus the forgery must refuse"));
        assert_eq!(
            live, on_replay,
            "{label}: live and replay refuse differently"
        );
    }
    assert_eq!(
        run.head().as_deref(),
        Some(run.base().as_str()),
        "nothing moved the ref"
    );
    assert!(
        !run.observed(
            EffectSiteId::Ref(RefSite::CompareAndSwapIntegration),
            HookPhase::Before
        ),
        "no CAS was issued for a refused authorization"
    );
}

#[test]
fn a_symbolic_or_checked_out_integration_ref_refuses_before_any_append() {
    let mut run = Run::started("publishable");
    let candidate = run.queue_candidate(ALPHA);
    let refname = run.integration_ref();
    let kinds_before = run.emitter.durable_kinds();

    // Checked out in a worktree the run does not own: by the branch name, so
    // the worktree's HEAD is the branch rather than a detached commit.
    let branch = refname
        .as_str()
        .strip_prefix("refs/heads/")
        .expect("the integration ref is a branch")
        .to_owned();
    let checkout = run.fixture.root.join("operator-checkout");
    git(
        &run.fixture.base,
        &[
            "worktree",
            "add",
            "--quiet",
            checkout.to_str().expect("utf-8 scratch path"),
            &branch,
        ],
    );
    let error = integrate_through(&mut run, &candidate).expect_err("a checked-out ref refuses");
    assert!(
        error.to_string().contains("checked out"),
        "the refusal names the cause: {error}"
    );
    assert_eq!(
        run.emitter.durable_kinds(),
        kinds_before,
        "nothing was appended"
    );
    assert!(run.reservations.is_empty(), "the reservation was cancelled");
    git(
        &run.fixture.base,
        &[
            "worktree",
            "remove",
            "--force",
            checkout.to_str().expect("utf-8 scratch path"),
        ],
    );

    // Symbolic.
    git(
        &run.fixture.base,
        &["symbolic-ref", refname.as_str(), "refs/heads/elsewhere"],
    );
    let error = integrate_through(&mut run, &candidate).expect_err("a symbolic ref refuses");
    assert!(
        error.to_string().contains("symbolic"),
        "the refusal names the cause: {error}"
    );
    assert_eq!(
        run.emitter.durable_kinds(),
        kinds_before,
        "nothing was appended"
    );
    assert!(run.reservations.is_empty(), "the reservation was cancelled");
    assert!(
        !run.observed(
            EffectSiteId::Ref(RefSite::CompareAndSwapIntegration),
            HookPhase::Before
        ),
        "no CAS was issued"
    );
    assert_eq!(
        run.task_state(ALPHA),
        TaskState::AwaitingMerge,
        "the candidate stays queued"
    );
    assert!(run.reservations.balances());
}

#[test]
fn third_sha_refused_and_a_ref_already_at_the_proposal_only_records() {
    let mut run = Run::started("third-sha");
    let candidate = run.queue_candidate(ALPHA);
    let request = IntegrationRequest::from_fold(run.emitter.fold(), &candidate).expect("request");
    let manager = run.fixture.manager.clone();
    run.reservations
        .take(ALPHA, ReservationKind::Integration)
        .expect("reserve");
    let decided = decide(&manager, &request).expect("decide");
    let authorized = prepare_fast(&mut run, &request, decided.head).expect("prepared");
    let kinds_after_prepared = run.emitter.durable_kinds();

    // Foreign history: the ref is moved to a commit the log never recorded.
    let foreign = {
        let scratch = run.fixture.root.join("foreign");
        git(
            &run.fixture.base,
            &[
                "worktree",
                "add",
                "--quiet",
                "--detach",
                scratch.to_str().expect("utf-8 scratch path"),
                run.base().as_str(),
            ],
        );
        crate::workspace_manager::fixture::write_file(&scratch.join("foreign.txt"), b"foreign\n");
        git(&scratch, &["add", "-A"]);
        git(&scratch, &["commit", "-q", "-m", "foreign"]);
        let sha = git(&scratch, &["rev-parse", "HEAD"]);
        git(
            &run.fixture.base,
            &[
                "worktree",
                "remove",
                "--force",
                scratch.to_str().expect("utf-8 scratch path"),
            ],
        );
        sha
    };
    git(
        &run.fixture.base,
        &[
            "update-ref",
            "--no-deref",
            run.integration_ref().as_str(),
            &foreign,
            run.base().as_str(),
        ],
    );

    let error = publish(&mut run, &manager, authorized.clone()).expect_err("a third SHA refuses");
    assert!(
        error.to_string().contains("third SHA") && error.to_string().contains(&foreign),
        "the refusal names the foreign commit: {error}"
    );
    assert_eq!(
        run.emitter.durable_kinds(),
        kinds_after_prepared,
        "nothing was appended after the refusal"
    );
    assert_eq!(
        run.head().as_deref(),
        Some(foreign.as_str()),
        "the ref was not touched"
    );
    assert!(
        !run.observed(
            EffectSiteId::Ref(RefSite::CompareAndSwapIntegration),
            HookPhase::Before
        ),
        "no CAS was issued against a third SHA"
    );
    assert!(
        run.emitter.fold().transaction().is_some(),
        "the authorization is still owed: an authorized publication is never abandoned"
    );

    // Put the ref at the proposal, as a swap that died before task_merged
    // would have: only the record is owed, and no second swap is issued.
    git(
        &run.fixture.base,
        &[
            "update-ref",
            "--no-deref",
            run.integration_ref().as_str(),
            candidate.commit_sha.as_str(),
            &foreign,
        ],
    );
    let published = publish(&mut run, &manager, authorized).expect("the record completes");
    assert_eq!(published.merged_sha, candidate.commit_sha);
    assert!(
        !run.observed(
            EffectSiteId::Ref(RefSite::CompareAndSwapIntegration),
            HookPhase::Before
        ),
        "a ref already at the proposal is not swapped again"
    );
    assert_eq!(run.task_state(ALPHA), TaskState::Merged);
    assert!(run.emitter.fold().transaction().is_none());
    run.replay_twice_equal();
}

#[test]
fn a_stale_candidate_is_refused_before_any_staging_effect_in_this_build() {
    let mut run = Run::started("stale-refused");
    let first = run.queue_candidate(ALPHA);
    let second = run.queue_candidate(BETA);
    integrate_through(&mut run, &first).expect("the first candidate is exact-base");
    assert_ne!(
        run.head().as_deref(),
        Some(run.base().as_str()),
        "the second candidate's base is no longer the head"
    );

    let kinds_before = run.emitter.durable_kinds();
    let error = integrate_through(&mut run, &second).expect_err("stale refuses in this build");
    assert!(error.to_string().contains("stale"), "{error}");
    assert_eq!(
        run.emitter.durable_kinds(),
        kinds_before,
        "nothing was appended"
    );
    for site in [
        EffectSiteId::Worktree(WorktreeSite::WriteStagingIntent),
        EffectSiteId::Worktree(WorktreeSite::AddStaging),
        EffectSiteId::Object(ObjectSite::ProposalCherryPick),
        EffectSiteId::Ref(RefSite::PinPrepared),
    ] {
        assert!(!run.observed(site, HookPhase::Before), "`{site}` executed");
    }
    assert!(run.reservations.is_empty() && run.reservations.balances());
    assert_eq!(run.task_state(BETA), TaskState::AwaitingMerge);
}

#[test]
fn a_recovered_authorization_is_the_live_one_and_completes_through_the_same_publish() {
    let mut run = Run::started("recovered-authorization");
    let candidate = run.queue_candidate(ALPHA);
    let request = IntegrationRequest::from_fold(run.emitter.fold(), &candidate).expect("request");
    let manager = run.fixture.manager.clone();
    let run_id = run
        .emitter
        .fold()
        .started()
        .expect("started")
        .run_id
        .clone();

    assert_eq!(
        Authorized::from_fold(run.emitter.fold(), &run_id).expect("read"),
        None,
        "nothing is authorized before merge_prepared"
    );

    run.reservations
        .take(ALPHA, ReservationKind::Integration)
        .expect("reserve");
    let decided = decide(&manager, &request).expect("decide");
    let live = prepare_fast(&mut run, &request, decided.head).expect("prepared");

    let recovered = Authorized::from_fold(run.emitter.fold(), &run_id)
        .expect("read")
        .expect("merge_prepared authorized a publication");
    assert_eq!(
        recovered, live,
        "what a resume derives from the fold's Prepared transaction is exactly what the live \
         sequence authorized: a fast publication with no pin and no staging"
    );
    assert_eq!(recovered.pin, None);
    assert_eq!(recovered.staging, None);

    let published = publish(&mut run, &manager, recovered).expect("publish");
    assert_eq!(published.merged_sha, candidate.commit_sha);
    assert_eq!(run.head().as_deref(), Some(candidate.commit_sha.as_str()));
    assert_eq!(
        Authorized::from_fold(run.emitter.fold(), &run_id).expect("read"),
        None,
        "a completed publication authorizes nothing further"
    );
    run.replay_twice_equal();
}
