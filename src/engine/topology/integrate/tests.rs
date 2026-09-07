//! Tests for the integration transaction.

use super::*;
use crate::engine::topology::identity::ReservationKind;
use crate::engine::topology::scaffold::{ALPHA, BETA, Run, VerifyReview};
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
fn integrate_through(run: &mut Run, candidate: &CandidateRef) -> Result<Terminal, UpstrokeError> {
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

#[track_caller]
fn published(terminal: Terminal) -> Published {
    match terminal {
        Terminal::Merged(published) => published,
        other => panic!("expected a publication, reached {other:?}"),
    }
}

fn merge_prepared_of_sequence(run: &Run, sequence: SequenceId) -> MergePrepared {
    run.emitter
        .durable_events()
        .into_iter()
        .find_map(|event| match event.body {
            TopologyEventBody::MergePrepared { data } if data.sequence == sequence => Some(*data),
            _ => None,
        })
        .expect("a durable merge_prepared for the sequence")
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
    let published =
        published(integrate_through(&mut run, &candidate).expect("the fast path publishes"));
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
fn an_append_failure_at_merge_prepared_issues_no_cas_and_leaves_the_integration_ref() {
    // The two-crash proof, live half. The compare-and-swap follows the
    // merge_prepared append in `integrate`, so an append that does not complete
    // — the stable-prefix barrier's sync failing before it returns is one way —
    // aborts the sequence through `?` before any CAS, so no ref moves. What a
    // resume then derives from the durable prefix — a lost unsynced line, or a
    // kept one — is `recover`'s `unsynced_merge_prepared_lost_to_power_failure`
    // and `events::log`'s barrier tests; here the point is only that the ref
    // never ran ahead of the append.
    let mut run = Run::started("append-fails-no-cas");
    let candidate = run.queue_candidate(ALPHA);
    let base = run.base();

    // A sync failure at the append is the barrier failing before it can prove
    // the line durable; `merge_prepared` is the first append after the
    // candidate is queued.
    run.arm_point(
        EffectSiteId::Event(crate::topology::effects::EventSite::Append),
        crate::topology::effects::SubEffectPoint::Synced,
        crate::topology::effects::InjectionMode::ErrorReturn,
    );
    integrate_through(&mut run, &candidate)
        .expect_err("the merge_prepared append's sync was made to fail");

    assert!(
        !run.observed(
            EffectSiteId::Ref(RefSite::CompareAndSwapIntegration),
            HookPhase::Before
        ),
        "a failed merge_prepared append still reached the compare-and-swap"
    );
    assert_eq!(
        run.head().as_deref(),
        Some(base.as_str()),
        "the integration ref moved though the append failed"
    );
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
fn stale_candidate_takes_staging_path_and_publishes_pinned_proposal() {
    let mut run = Run::started("stale-clean");
    run.verify_reviewers.push(passing_reviewer());
    let first = run.queue_candidate(ALPHA);
    let second = run.queue_candidate(BETA);
    published(integrate_through(&mut run, &first).expect("alpha is exact-base"));
    let head_after_first = run.head().expect("the head moved");
    assert_ne!(
        head_after_first,
        run.base().0,
        "the head is no longer beta's base"
    );

    let terminal = integrate_through(&mut run, &second).expect("beta takes the staging path");
    let published = published(terminal);
    assert_eq!(published.key, BETA);
    assert_eq!(published.sequence, SequenceId(1));

    let proposal = run.head().expect("the head moved again");
    assert_ne!(
        proposal, second.commit_sha.0,
        "a stale merge does not publish the candidate itself"
    );
    assert_eq!(published.merged_sha.0, proposal);
    assert_eq!(
        git(&run.fixture.base, &["rev-parse", &format!("{proposal}^")]),
        head_after_first,
        "the proposal's parent is the head it was cherry-picked onto"
    );

    let prepared = merge_prepared_of_sequence(&run, SequenceId(1));
    assert_eq!(prepared.disposition, PreparedDisposition::StaleClean);
    assert_eq!(prepared.expected_head.0, head_after_first);
    assert_eq!(prepared.proposed_sha.0, proposal);
    assert!(
        prepared.prepared_ref.is_some(),
        "a stale_clean publication pins its proposal"
    );
    assert!(
        prepared
            .verification
            .as_ref()
            .is_some_and(crate::topology::events::VerificationRecord::passed)
    );

    assert!(
        run.fixture
            .manager
            .intents()
            .expect("intents")
            .iter()
            .all(|slot| !matches!(slot, crate::workspace_manager::Slot::Staging { .. })),
        "the staging intent survived the terminal"
    );
    assert!(
        run.fixture
            .manager
            .direct_ref_target(prepared.prepared_ref.as_ref().expect("a pin").as_str())
            .expect("read the pin")
            .is_none(),
        "the prepared pin survived task_merged"
    );
    assert!(
        run.observed(
            EffectSiteId::Object(ObjectSite::ProposalCherryPick),
            HookPhase::After
        ),
        "the proposal was never cherry-picked"
    );
    assert_eq!(run.task_state(BETA), TaskState::Merged);
    assert!(run.reservations.balances());
    run.replay_twice_equal();
}

fn passing_reviewer() -> crate::engine::topology::attempt::ReviewerPlan {
    crate::engine::topology::attempt::ReviewerPlan {
        agent: crate::runner::AgentId::new(crate::engine::topology::scaffold::REVIEW_AGENT),
        profile: crate::review::profile_for(
            crate::engine::topology::scaffold::REVIEW_AGENT,
            "review-model",
            "review",
            crate::ir::Effort::High,
        ),
        lens: crate::review::Lens::Acceptance,
        preflight_cli_version: None,
        timeout: std::time::Duration::from_secs(120),
    }
}
#[test]
fn a_recovered_authorization_is_the_live_one_and_completes_through_the_same_publish() {
    let mut run = Run::started("recovered-authorization");
    let candidate = run.queue_candidate(ALPHA);
    let request = IntegrationRequest::from_fold(run.emitter.fold(), &candidate).expect("request");
    let manager = run.fixture.manager.clone();

    assert_eq!(
        Authorized::from_fold(run.emitter.fold()).expect("read"),
        None,
        "nothing is authorized before merge_prepared"
    );

    run.reservations
        .take(ALPHA, ReservationKind::Integration)
        .expect("reserve");
    let decided = decide(&manager, &request).expect("decide");
    let live = prepare_fast(&mut run, &request, decided.head).expect("prepared");

    let recovered = Authorized::from_fold(run.emitter.fold())
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
        Authorized::from_fold(run.emitter.fold()).expect("read"),
        None,
        "a completed publication authorizes nothing further"
    );
    run.replay_twice_equal();
}

/// Every terminal shape a `merge_verification_started` transaction can reach
/// through the integrate path.
#[derive(Debug, Clone, Copy)]
enum Shape {
    Fast,
    StaleClean,
    AlreadyPresent,
    Conflict,
    CodeRejected,
    Deferred,
    Parked,
}

const EVERY_INTEGRATE_SHAPE: [Shape; 7] = [
    Shape::Fast,
    Shape::StaleClean,
    Shape::AlreadyPresent,
    Shape::Conflict,
    Shape::CodeRejected,
    Shape::Deferred,
    Shape::Parked,
];

#[test]
fn terminal_shape_coverage_table_drives_every_shape_and_each_converges_on_replay() {
    // The eight-shape coverage table. Seven shapes are reachable through the
    // integrate path and driven here end to end; each is asserted at its
    // terminal and then replayed twice for equality. The remaining two live in
    // other harnesses because they are not integrate terminals:
    // Declined-after-park is `fold`'s
    // `declined_parked_verification_fails_task_consumes_queue_position_releases_lease_and_halts_per_policy`
    // (a question answer, not a verification outcome), and Interrupted is
    // `recover`'s `a_resume_settles_an_interrupted_stale_verification_and_reclaims_its_residue`
    // (a resume terminal, never a live one).
    for shape in EVERY_INTEGRATE_SHAPE {
        let tag = format!("table-{}", format!("{shape:?}").to_lowercase());
        let mut run = match shape {
            Shape::Deferred => Run::started_with_max_defers(&tag, 2),
            _ => Run::started(&tag),
        };
        match shape {
            Shape::Fast | Shape::Conflict => {}
            Shape::StaleClean | Shape::AlreadyPresent => {
                run.verify_reviewers.push(passing_reviewer());
            }
            Shape::CodeRejected => {
                run.verify_reviewers.push(passing_reviewer());
                run.verify_review = VerifyReview::NeedsChanges;
            }
            Shape::Parked => {
                run.verify_reviewers.push(passing_reviewer());
                run.verify_review = VerifyReview::NeedsHuman;
            }
            Shape::Deferred => {
                run.verify_reviewers.push(passing_reviewer());
                run.verify_review =
                    VerifyReview::Unavailable(crate::ir::OutcomeStatus::RateLimited);
            }
        }

        let terminal = if let Shape::Fast = shape {
            let candidate = run.queue_candidate(ALPHA);
            integrate_through(&mut run, &candidate).expect("fast is exact-base")
        } else {
            let (a_path, b_path, a_body, b_body) = match shape {
                Shape::AlreadyPresent => (
                    "shared.txt",
                    "shared.txt",
                    "the shared change
",
                    "the shared change
",
                ),
                Shape::Conflict => (
                    "shared.txt",
                    "shared.txt",
                    "alpha's line
",
                    "beta's line
",
                ),
                _ => (
                    "a.txt", "b.txt", "alpha
", "beta
",
                ),
            };
            let first = run.queue_candidate_editing(ALPHA, a_path, a_body);
            let second = run.queue_candidate_editing(BETA, b_path, b_body);
            published(integrate_through(&mut run, &first).expect("alpha is exact-base"));
            integrate_through(&mut run, &second).expect("beta reaches its terminal")
        };

        match shape {
            Shape::Fast => {
                published(terminal);
                assert_eq!(
                    merge_prepared_of_sequence(&run, SequenceId(0)).disposition,
                    PreparedDisposition::Fast,
                    "fast"
                );
            }
            Shape::StaleClean => {
                published(terminal);
                assert_eq!(
                    merge_prepared_of_sequence(&run, SequenceId(1)).disposition,
                    PreparedDisposition::StaleClean,
                    "stale_clean"
                );
            }
            Shape::AlreadyPresent => {
                published(terminal);
                assert_eq!(
                    merge_prepared_of_sequence(&run, SequenceId(1)).disposition,
                    PreparedDisposition::AlreadyPresent,
                    "already_present"
                );
            }
            Shape::Conflict => {
                assert!(
                    matches!(terminal, Terminal::Rejected { .. }),
                    "conflict rejects"
                );
                assert!(
                    matches!(
                        rejected_of(&run).disposition,
                        crate::topology::events::RejectionDisposition::Conflict { .. }
                    ),
                    "the rejection is a conflict"
                );
            }
            Shape::CodeRejected => {
                assert!(
                    matches!(terminal, Terminal::Rejected { .. }),
                    "code rejection rejects"
                );
                assert!(
                    matches!(
                        rejected_of(&run).disposition,
                        crate::topology::events::RejectionDisposition::CodeRejected { .. }
                    ),
                    "the rejection is code-attributed"
                );
            }
            Shape::Deferred => {
                assert!(
                    matches!(terminal, Terminal::Unavailable { parked: false, .. }),
                    "an infrastructure outage inside the allowance defers"
                );
                assert!(matches!(
                    unavailable_of(&run).outcome,
                    crate::topology::events::UnavailableOutcome::Deferred { .. }
                ));
            }
            Shape::Parked => {
                assert!(
                    matches!(terminal, Terminal::Unavailable { parked: true, .. }),
                    "a human-required verdict parks"
                );
                assert!(matches!(
                    unavailable_of(&run).outcome,
                    crate::topology::events::UnavailableOutcome::Parked { .. }
                ));
            }
        }
        run.replay_twice_equal();
    }
}

#[test]
fn an_already_present_candidate_settles_without_an_empty_commit() {
    let mut run = Run::started("already-present");
    run.verify_reviewers.push(passing_reviewer());
    // Both candidates make the same change to the same path, so beta's
    // cherry-pick onto the merged head is empty.
    let first = run.queue_candidate_editing(ALPHA, "shared.txt", "the shared change\n");
    let second = run.queue_candidate_editing(BETA, "shared.txt", "the shared change\n");
    published(integrate_through(&mut run, &first).expect("alpha is exact-base"));
    let head = run.head().expect("the head moved");

    let objects_before = count_objects(&run);
    let published =
        published(integrate_through(&mut run, &second).expect("beta is already present"));
    assert_eq!(
        published.merged_sha.0, head,
        "already_present publishes the unchanged head"
    );
    assert_eq!(
        run.head().as_deref(),
        Some(head.as_str()),
        "the ref did not move"
    );
    assert_eq!(
        count_objects(&run),
        objects_before,
        "no proposal commit was manufactured for an already-present candidate"
    );

    let prepared = merge_prepared_of_sequence(&run, SequenceId(1));
    assert_eq!(prepared.disposition, PreparedDisposition::AlreadyPresent);
    assert_eq!(prepared.expected_head.0, head);
    assert_eq!(prepared.proposed_sha.0, head);
    assert_eq!(prepared.prepared_ref, None, "already_present pins nothing");
    assert_eq!(run.task_state(BETA), TaskState::Merged);
    run.replay_twice_equal();
}

#[test]
fn a_conflicting_candidate_is_rejected_with_an_atomic_repair_before_any_repair_effect() {
    let mut run = Run::started("conflict");
    let first = run.queue_candidate_editing(ALPHA, "shared.txt", "alpha's line\n");
    let second = run.queue_candidate_editing(BETA, "shared.txt", "beta's line\n");
    published(integrate_through(&mut run, &first).expect("alpha is exact-base"));
    let head = run.head().expect("head moved");

    let terminal = integrate_through(&mut run, &second).expect("beta conflicts");
    let Terminal::Rejected { sequence, key } = terminal else {
        panic!("a conflict must reject, reached {terminal:?}");
    };
    assert_eq!((sequence, key), (SequenceId(1), BETA));

    // The rejection registered the repair atomically: beta is AwaitingRepair,
    // a new Pending repair task exists, and the lineage lease is held.
    assert_eq!(run.task_state(BETA), TaskState::AwaitingRepair);
    let repair = TaskKey(
        u32::try_from(run.emitter.fold().registry().expect("registry").len() - 1)
            .expect("a small fixture registry"),
    );
    assert_eq!(run.task_state(repair), TaskState::Pending);
    let rejected = run
        .emitter
        .durable_events()
        .into_iter()
        .find_map(|event| match event.body {
            TopologyEventBody::MergeRejected { data } => Some(*data),
            _ => None,
        })
        .expect("a durable merge_rejected");
    assert!(matches!(
        rejected.disposition,
        crate::topology::events::RejectionDisposition::Conflict { .. }
    ));
    assert_eq!(rejected.rejecting_head.0, head);
    assert_eq!(
        rejected.repair.entry.origin,
        crate::topology::registry::Origin::MergeRepair
    );
    assert_eq!(
        rejected.repair.entry.lineage,
        Some(crate::topology::registry::Lineage {
            root: BETA,
            parent: BETA,
            index: 0
        })
    );
    assert!(
        matches!(
            rejected.repair.admission,
            crate::topology::events::SpawnAdmission::Runnable
        ),
        "the first repair of a run with automatic repairs is runnable"
    );

    // No repair was dispatched: no task_dispatched for the repair, and no
    // repair worktree exists — merge_rejected is before any repair effect.
    assert!(
        !run.emitter
            .durable_kinds()
            .iter()
            .rev()
            .take(1)
            .any(|k| *k == "task_dispatched"),
        "the terminal is merge_rejected, not a dispatch"
    );
    // The staging worktree of the rejected transaction is gone.
    assert!(
        run.fixture
            .manager
            .intents()
            .expect("intents")
            .iter()
            .all(|slot| !matches!(slot, crate::workspace_manager::Slot::Staging { .. })),
        "the staging worktree survived the rejection"
    );
    run.replay_twice_equal();
}

#[test]
fn a_code_rejected_candidate_registers_a_repair() {
    let mut run = Run::started("code-rejected");
    run.verify_reviewers.push(passing_reviewer());
    run.verify_review = VerifyReview::NeedsChanges;
    let first = run.queue_candidate_editing(ALPHA, "a.txt", "alpha\n");
    let second = run.queue_candidate_editing(BETA, "b.txt", "beta\n");
    published(integrate_through(&mut run, &first).expect("alpha is exact-base"));

    let terminal = integrate_through(&mut run, &second).expect("beta is stale then rejected");
    let Terminal::Rejected { .. } = terminal else {
        panic!("a review rejection must reject, reached {terminal:?}");
    };
    let rejected = run
        .emitter
        .durable_events()
        .into_iter()
        .find_map(|event| match event.body {
            TopologyEventBody::MergeRejected { data } => Some(*data),
            _ => None,
        })
        .expect("a durable merge_rejected");
    let crate::topology::events::RejectionDisposition::CodeRejected { verification } =
        &rejected.disposition
    else {
        panic!("a review rejection is code-attributed");
    };
    assert_eq!(
        verification.verdict,
        crate::topology::events::VerificationVerdict::Rejected
    );
    assert!(
        verification.gates_passed,
        "the gate set passed; the reviewer rejected"
    );
    assert_eq!(run.task_state(BETA), TaskState::AwaitingRepair);
    run.replay_twice_equal();
}

#[test]
fn a_human_required_verdict_parks_the_task() {
    let mut run = Run::started("human-required");
    run.verify_reviewers.push(passing_reviewer());
    run.verify_review = VerifyReview::NeedsHuman;
    let first = run.queue_candidate_editing(ALPHA, "a.txt", "alpha\n");
    let second = run.queue_candidate_editing(BETA, "b.txt", "beta\n");
    published(integrate_through(&mut run, &first).expect("alpha is exact-base"));

    let terminal = integrate_through(&mut run, &second).expect("beta parks");
    let Terminal::Unavailable { parked, .. } = terminal else {
        panic!("a human-required verdict is unavailable, reached {terminal:?}");
    };
    assert!(parked, "a human-required verdict always parks");
    let unavailable = unavailable_of(&run);
    assert!(matches!(
        unavailable.cause,
        crate::topology::events::UnavailableCause::HumanRequired { .. }
    ));
    assert!(matches!(
        unavailable.outcome,
        crate::topology::events::UnavailableOutcome::Parked { .. }
    ));
    assert_eq!(run.task_state(BETA), TaskState::AwaitingInput);
    assert_eq!(
        run.emitter.fold().open_questions().expect("started").len(),
        1
    );
    // The pin and staging are reclaimed at the terminal.
    assert!(
        run.fixture
            .manager
            .intents()
            .expect("intents")
            .iter()
            .all(|slot| !matches!(slot, crate::workspace_manager::Slot::Staging { .. })),
        "the staging worktree survived the park"
    );
    run.replay_twice_equal();
}

#[test]
fn infrastructure_failure_defers_then_parks_at_max_defers() {
    let mut run = Run::started_with_max_defers("infra-defer", 2);
    run.verify_reviewers.push(passing_reviewer());
    run.verify_review = VerifyReview::Unavailable(crate::ir::OutcomeStatus::RateLimited);
    let first = run.queue_candidate_editing(ALPHA, "a.txt", "alpha\n");
    let second = run.queue_candidate_editing(BETA, "b.txt", "beta\n");
    published(integrate_through(&mut run, &first).expect("alpha is exact-base"));

    // First outage: deferred, inside the allowance.
    let terminal = integrate_through(&mut run, &second).expect("beta defers");
    assert!(matches!(
        terminal,
        Terminal::Unavailable { parked: false, .. }
    ));
    let unavailable = unavailable_of(&run);
    assert!(matches!(
        unavailable.outcome,
        crate::topology::events::UnavailableOutcome::Deferred { defers: 1 }
    ));
    assert_eq!(
        run.task_state(BETA),
        TaskState::AwaitingMerge,
        "a deferred task stays awaiting merge"
    );
    assert!(
        !run.emitter.fold().integration_admissible(),
        "a deferred candidate is not eligible until the wake"
    );

    // Wake it, then the second outage parks at max_defers.
    run.wake_deferred();
    assert!(
        run.emitter.fold().integration_admissible(),
        "the wake re-enabled the candidate"
    );
    let terminal = integrate_through(&mut run, &second).expect("beta parks at the limit");
    assert!(matches!(
        terminal,
        Terminal::Unavailable { parked: true, .. }
    ));
    let unavailable = run
        .emitter
        .durable_events()
        .into_iter()
        .filter_map(|event| match event.body {
            TopologyEventBody::MergeVerificationUnavailable { data } => Some(data),
            _ => None,
        })
        .next_back()
        .expect("the second unavailable");
    assert!(matches!(
        unavailable.outcome,
        crate::topology::events::UnavailableOutcome::Parked { .. }
    ));
    assert_eq!(run.task_state(BETA), TaskState::AwaitingInput);
    run.replay_twice_equal();
}

/// One gate the scaffold's recording runner answers with exit 0.
fn passing_gate() -> crate::engine::topology::attempt::GatePlan {
    crate::engine::topology::attempt::GatePlan {
        name: "scaffold-gate".to_owned(),
        command: crate::runner::CommandSpec::new("gate").arg("--check"),
        timeout: std::time::Duration::from_secs(60),
    }
}

#[test]
fn verification_snapshots_are_removed_only_after_the_terminal() {
    // `side_effect_vs_event_ordering`: "staging and snapshot removal (forced)
    // after terminal (incl. Deferred/Parked)". For each terminal a verification
    // can reach — merge_prepared, merge_rejected, merge_verification_unavailable
    // — every Snapshot.Remove the sequence performs comes after the terminal's
    // append, and the sequence leaves no snapshot behind.
    for (label, review) in [
        ("prepared", VerifyReview::Passed),
        ("rejected", VerifyReview::NeedsChanges),
        ("parked", VerifyReview::NeedsHuman),
    ] {
        let mut run = Run::started(&format!("snapshots-after-{label}"));
        run.verify_gates.push(passing_gate());
        run.verify_reviewers.push(passing_reviewer());
        run.verify_review = review;
        let first = run.queue_candidate_editing(ALPHA, "a.txt", "alpha\n");
        let second = run.queue_candidate_editing(BETA, "b.txt", "beta\n");
        published(integrate_through(&mut run, &first).expect("alpha is exact-base"));
        let mark = run.mark();
        integrate_through(&mut run, &second).expect("beta reaches its terminal");

        let appends: Vec<usize> = run
            .timeline
            .positions(
                EffectSiteId::Event(crate::topology::effects::EventSite::Append),
                HookPhase::After,
            )
            .into_iter()
            .filter(|position| *position > mark)
            .collect();
        // The first append after the mark is merge_verification_started; the
        // second is the verification's terminal, whichever shape it took.
        let terminal = *appends
            .get(1)
            .unwrap_or_else(|| panic!("{label}: the sequence appended fewer than two events"));
        let removals: Vec<usize> = run
            .timeline
            .positions(
                EffectSiteId::Snapshot(crate::topology::effects::SnapshotSite::Remove),
                HookPhase::Before,
            )
            .into_iter()
            .filter(|position| *position > mark)
            .collect();
        assert_eq!(
            removals.len(),
            2,
            "{label}: one gate snapshot and one reviewer snapshot were removed: {removals:?}"
        );
        assert!(
            removals.iter().all(|position| *position > terminal),
            "{label}: a snapshot was removed at {removals:?}, before the terminal at {terminal}"
        );
        assert!(
            run.fixture
                .manager
                .intents()
                .expect("intents")
                .iter()
                .all(|slot| !matches!(slot, crate::workspace_manager::Slot::Snapshot { .. })),
            "{label}: a snapshot intent survived the terminal"
        );
        run.replay_twice_equal();
    }
}

fn rejected_of(run: &Run) -> crate::topology::events::MergeRejected {
    run.emitter
        .durable_events()
        .into_iter()
        .find_map(|event| match event.body {
            TopologyEventBody::MergeRejected { data } => Some(*data),
            _ => None,
        })
        .expect("a durable merge_rejected")
}

fn unavailable_of(run: &Run) -> crate::topology::events::MergeVerificationUnavailable {
    run.emitter
        .durable_events()
        .into_iter()
        .find_map(|event| match event.body {
            TopologyEventBody::MergeVerificationUnavailable { data } => Some(data),
            _ => None,
        })
        .expect("a durable merge_verification_unavailable")
}
