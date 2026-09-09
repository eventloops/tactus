//! Extended notes: `docs/internals/engine/topology/attempt/tests.md`

use std::path::{Path, PathBuf};

use super::*;
use crate::engine::topology::dispatch::DispatchKind;
use crate::engine::topology::identity::{ReservationKind, Reservations};
use crate::engine::topology::scaffold::{
    AGENT, ALPHA, RETAINED_SESSION, Run, kill_child_and_adopt, kill_child_environment, kill_dir,
};
use crate::engine::topology::settle::{self, ManagedWorktrees, RetryOutcome, RetryRequest};
use crate::topology::effects::{
    EffectSiteId, EventSite, HookPhase, Injection, InjectionMode, ObjectResidue, ObjectSite,
    ResidueElement, SnapshotSite, SubEffectPoint, WorktreeSite,
};
use crate::topology::events::{GenerationCloseReason, GenerationId, SessionId};
use crate::topology::fold::{GenerationClass, TaskState};
use crate::workspace_manager::fixture::Fixture;
use crate::workspace_manager::fixture::{
    KillableGitChild, died_by_kill, git, remove_file, time_git, write_file,
};
use crate::workspace_manager::{
    NoHooks, ResidueTarget, VerifyFailure, classify_object_residue, object_directory,
    observed_residue_elements, temporary_object_files, unreachable_objects,
};

const APPEND: EffectSiteId = EffectSiteId::Event(EventSite::Append);
const STAGE: EffectSiteId = EffectSiteId::Object(ObjectSite::CandidateStage);
const WRITE_TREE: EffectSiteId = EffectSiteId::Object(ObjectSite::CandidateWriteTree);
const SNAPSHOT_COMMIT: EffectSiteId = EffectSiteId::Object(ObjectSite::SnapshotCommitTree);
const SNAPSHOT_INTENT: EffectSiteId = EffectSiteId::Snapshot(SnapshotSite::WriteIntent);
const SNAPSHOT_ADD: EffectSiteId = EffectSiteId::Snapshot(SnapshotSite::Add);
const SNAPSHOT_REMOVE: EffectSiteId = EffectSiteId::Snapshot(SnapshotSite::Remove);
const VERIFY: EffectSiteId = EffectSiteId::Worktree(WorktreeSite::Verify);
const SCRUB: EffectSiteId = EffectSiteId::Worktree(WorktreeSite::Remove);

const GENERATION: GenerationId = GenerationId(0);

const WORKED: &[u8] = b"the agent edited this, and the capture stages it\n";
const WORKED_PATH: &str = "worked.txt";

fn agent_edits(worktree: &Path) {
    write_file(&worktree.join(WORKED_PATH), WORKED);
}

struct Process {
    slots: SlotAssertion,
    ledger: InvocationLedger,
}

impl Process {
    fn new() -> Self {
        Self {
            slots: SlotAssertion::new(),
            ledger: InvocationLedger::new(),
        }
    }

    fn balances(&self) -> bool {
        self.slots.balances() && self.ledger.balances()
    }
}

macro_rules! context {
    ($run:expr, $process:expr) => {
        AttemptContext {
            manager: &$run.fixture.manager,
            hooks: &mut $run.hooks,
            emitter: &mut $run.emitter,
            runner: &$run.runner,
            slots: &mut $process.slots,
            ledger: &mut $process.ledger,
            adapters: &crate::engine::topology::scaffold::ScaffoldAdapters::new(),
            paths: &$run.paths,
            reviews: &crate::engine::attempt::LegacyReviewPasses,
            input_policy: &crate::engine::attempt::LegacyReviewInputPolicy,
        }
    };
}

fn index_blobs(worktree: &Path) -> Vec<String> {
    git(worktree, &["ls-files", "-s"])
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1).map(str::to_owned))
        .collect()
}

fn unreachable_ephemeral_commits(base: &Path) -> Vec<String> {
    unreachable_objects(base)
        .expect("fsck")
        .into_iter()
        .filter(|id| {
            git(base, &["cat-file", "-t", id]) == "commit"
                && git(base, &["log", "-1", "--format=%s", id])
                    == "upstroke: ephemeral snapshot input"
        })
        .collect()
}

#[test]
fn attempt_started_is_durable_before_any_spawn() {
    let mut run = Run::started("o23");
    let dispatched = run.dispatch(ALPHA, 0);
    let plan = run.attempt_plan(ALPHA, 1);
    let mut process = Process::new();

    let mark = run.mark();
    let started = context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("the attempt starts");

    assert_eq!(
        run.emitter.durable_kinds(),
        vec!["run_started", "task_dispatched", "attempt_started"],
        "O23: the attempt is durable"
    );
    let events = run.emitter.durable_events();
    let crate::topology::events::TopologyEventBody::AttemptStarted { data } = &events[2].body
    else {
        panic!("the third durable event is not an attempt");
    };
    assert_eq!(data.attempt, plan.attempt);
    assert_eq!(
        data.binding, plan.binding,
        "INV-19: the frozen rung binding"
    );
    assert!(
        data.resume_session.is_none() && data.materialization_observed.is_none(),
        "a fresh ordinary attempt resumes nothing and materializes nothing"
    );

    let ran = run.runner.ran();
    assert_eq!(ran.len(), 1, "one worker, and nothing else yet");
    assert_eq!(ran[0].invocation, started.identities.worker());
    assert_eq!(
        ran[0].workspace, dispatched.worktree,
        "the worker runs in the task worktree"
    );
    assert_eq!(
        ran[0].durable_at_spawn,
        vec!["run_started", "task_dispatched", "attempt_started"],
        "O23: `attempt_started` must already be on disk when the worker is asked for"
    );
    assert_eq!(
        run.count_after(mark, APPEND, HookPhase::After),
        1,
        "one append for the attempt, and it is the one the worker ran after"
    );

    assert_eq!(
        run.emitter.generation_class(ALPHA, GENERATION),
        GenerationClass::InFlight {
            attempt: plan.attempt
        }
    );
    assert!(
        process.balances(),
        "the worker's slot and registration settled"
    );
}

#[test]
fn every_process_of_an_attempt_is_recorded_reviewers_included() {
    let mut run = Run::started("ledger-covers-reviews");
    let dispatched = run.dispatch(ALPHA, 0);
    let plan = run.attempt_plan(ALPHA, 1);
    let mut process = Process::new();

    let expected = 1 + plan.gates.len() + plan.reviewers.len();
    assert!(
        plan.reviewers.len() >= 2,
        "this fixture plans {} reviewer(s); with none the count below would \
         pass without covering the case it exists for",
        plan.reviewers.len()
    );

    let started = context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("start");
    agent_edits(&dispatched.worktree);
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("capture");
    let review_inputs = run.review_inputs();
    let assessed = context!(run, process)
        .assess(
            dispatched.site(),
            &plan,
            &started,
            &capture,
            &review_inputs.diff,
            crate::ir::TaskKind::Implement,
        )
        .expect("the scaffold's adapter parses its own worker output");
    context!(run, process)
        .judge(
            dispatched.site(),
            &plan,
            Judging {
                run: &started,
                capture: &capture,
                assessed: &assessed,
            },
            &review_inputs,
            &|pass| crate::review::ReviewInvocations {
                pass: started.identities.review_pass(pass, 0),
                reask: started.identities.review_reask(pass, 0),
            },
        )
        .expect("judge");

    assert_eq!(
        process.ledger.completed(),
        expected,
        "the ledger holds {} settled invocation(s) for an attempt that ran one \
         worker, {} gate(s) and {} reviewer(s)",
        process.ledger.completed(),
        plan.gates.len(),
        plan.reviewers.len()
    );
    assert!(process.balances(), "and every one of them settled");
}

#[test]
fn a_refused_gate_ends_the_set_and_its_cause_survives() {
    let mut run = Run::started("gate-short-circuit");
    let dispatched = run.dispatch(ALPHA, 0);
    let mut plan = run.attempt_plan(ALPHA, 1);
    let second = plan.gates[0].clone();
    plan.gates.push(second);
    run.runner.set_codes(vec![0, 2, 127]);
    let mut process = Process::new();

    let started = context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("start");
    agent_edits(&dispatched.worktree);
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("capture");
    let review_inputs = run.review_inputs();
    let assessed = context!(run, process)
        .assess(
            dispatched.site(),
            &plan,
            &started,
            &capture,
            &review_inputs.diff,
            crate::ir::TaskKind::Implement,
        )
        .expect("the scaffold's adapter parses its own worker output");
    let judgement = context!(run, process)
        .judge(
            dispatched.site(),
            &plan,
            Judging {
                run: &started,
                capture: &capture,
                assessed: &assessed,
            },
            &review_inputs,
            &|pass| crate::review::ReviewInvocations {
                pass: started.identities.review_pass(pass, 0),
                reask: started.identities.review_reask(pass, 0),
            },
        )
        .expect("judge");

    assert_eq!(
        judgement.gates.len(),
        1,
        "the second gate ran after the first refused, so a rejected diff bought \
         a gate that could not change the verdict"
    );
    let failure = judgement
        .failure
        .as_ref()
        .expect("a refused gate fails the attempt");
    assert_eq!(
        failure.kind,
        crate::ladder::FailureKind::GateFailed,
        "the first cause did not survive: the attempt is recorded as {:?}, \
         which is what the SECOND gate would have produced",
        failure.kind
    );
    assert!(
        !judgement.accepted(),
        "an attempt whose gate refused was accepted"
    );

    assert_eq!(
        failure.feedback.as_deref().map(str::trim),
        Some(
            format!(
                "{} (exit 2)",
                crate::engine::topology::scaffold::GATE_DIAGNOSTIC
            )
            .as_str()
        ),
        "the gate's output did not reach the failure's feedback: {:?}",
        failure.feedback
    );
    assert!(
        failure.reason.contains("scaffold"),
        "the failure does not name the gate: {}",
        failure.reason
    );
}

#[test]
fn capture_precedes_the_snapshots_and_every_snapshot_commits_before_its_intent() {
    let mut run = Run::started("o25-27");
    let dispatched = run.dispatch(ALPHA, 0);
    let plan = run.attempt_plan(ALPHA, 1);
    let mut process = Process::new();

    let started = context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("start");
    agent_edits(&dispatched.worktree);
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("capture");
    let mark = run.mark();
    let review_inputs = run.review_inputs();
    let assessed = context!(run, process)
        .assess(
            dispatched.site(),
            &plan,
            &started,
            &capture,
            &review_inputs.diff,
            crate::ir::TaskKind::Implement,
        )
        .expect("the scaffold's adapter parses its own worker output");
    let judgement = context!(run, process)
        .judge(
            dispatched.site(),
            &plan,
            Judging {
                run: &started,
                capture: &capture,
                assessed: &assessed,
            },
            &review_inputs,
            &|pass| crate::review::ReviewInvocations {
                pass: started.identities.review_pass(pass, 0),
                reask: started.identities.review_reask(pass, 0),
            },
        )
        .expect("judge");

    let stage = run.must_order_of(STAGE, HookPhase::Before);
    let write_tree = run.must_order_of(WRITE_TREE, HookPhase::Before);
    let commit = run.must_order_of(SNAPSHOT_COMMIT, HookPhase::Before);
    assert!(
        stage < write_tree && write_tree < commit,
        "O25: capture (stage={stage}, write-tree={write_tree}) precedes the snapshots \
         (commit={commit})"
    );

    let snapshots = 1 + plan.reviewers.len();
    for (site, what) in [
        (SNAPSHOT_COMMIT, "ephemeral commit"),
        (SNAPSHOT_INTENT, "intent"),
        (SNAPSHOT_ADD, "add"),
    ] {
        assert_eq!(
            run.count_after(mark, site, HookPhase::Before),
            snapshots,
            "one {what} per snapshot, and there are {snapshots} of them"
        );
    }
    let mut fence = mark;
    for index in 0..snapshots {
        let commit = run.order_after(fence, SNAPSHOT_COMMIT, HookPhase::Before);
        let intent = run.order_after(fence, SNAPSHOT_INTENT, HookPhase::Before);
        let add = run.order_after(fence, SNAPSHOT_ADD, HookPhase::Before);
        assert!(
            commit < intent && intent < add,
            "O26, snapshot {index} of {snapshots}: the order was commit={commit}, \
             intent={intent}, add={add}"
        );
        fence = add + 1;
    }

    assert!(judgement.accepted());
    assert_eq!(judgement.gates.len(), 1);
    assert_eq!(judgement.reviews.len(), 2);
    assert!(
        !run.observed(
            EffectSiteId::Object(ObjectSite::CandidateCommitTree),
            HookPhase::Before
        ),
        "O27: the candidate commit is `candidate.rs`'s and nothing here may write one"
    );

    assert_eq!(
        capture.tree,
        git(&dispatched.worktree, &["write-tree"]),
        "the recorded tree is the worktree's index"
    );
    assert_eq!(capture.parent, dispatched.base.0);
}

#[test]
fn gates_and_reviewers_run_on_fresh_exact_snapshots_and_never_in_the_task_worktree() {
    let mut run = Run::started("snapshots");
    let dispatched = run.dispatch(ALPHA, 0);
    let plan = run.attempt_plan(ALPHA, 1);
    let mut process = Process::new();

    let started = context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("start");
    agent_edits(&dispatched.worktree);
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("capture");
    let review_inputs = run.review_inputs();
    let assessed = context!(run, process)
        .assess(
            dispatched.site(),
            &plan,
            &started,
            &capture,
            &review_inputs.diff,
            crate::ir::TaskKind::Implement,
        )
        .expect("the scaffold's adapter parses its own worker output");
    let judgement = context!(run, process)
        .judge(
            dispatched.site(),
            &plan,
            Judging {
                run: &started,
                capture: &capture,
                assessed: &assessed,
            },
            &review_inputs,
            &|pass| crate::review::ReviewInvocations {
                pass: started.identities.review_pass(pass, 0),
                reask: started.identities.review_reask(pass, 0),
            },
        )
        .expect("judge");

    let workspaces: Vec<PathBuf> = run
        .runner
        .ran()
        .into_iter()
        .filter(|ran| {
            matches!(
                ran.role,
                crate::runner::ExecutionRole::Gate | crate::runner::ExecutionRole::Review
            )
        })
        .map(|ran| ran.workspace)
        .collect();
    for workspace in &workspaces {
        assert_ne!(
            *workspace, dispatched.worktree,
            "a verification process ran in the worker's worktree"
        );
        assert!(
            workspace.starts_with(run.fixture.manager.execution_root().join("snapshots")),
            "{} is not an exact snapshot",
            workspace.display()
        );
    }
    let distinct: std::collections::BTreeSet<&PathBuf> = workspaces.iter().collect();
    assert_eq!(
        distinct.len(),
        3,
        "one snapshot for the gate set and one fresh per reviewer, never shared across roles \
         or between reviewers: {workspaces:?}"
    );
    assert_eq!(
        judgement.reviews.len(),
        2,
        "both passes produced a record; `AttemptRecord.reviews` being empty \
         MEANS nothing was reviewed, so a pass that ran and recorded nothing \
         would write a false statement into the log"
    );

    assert!(
        run.fixture.manager.intents().expect("intents").len() == 1,
        "only the task worktree's intent is left"
    );
    for workspace in &distinct {
        assert!(
            !workspace.exists(),
            "{} survived its judgement",
            workspace.display()
        );
    }
    assert_eq!(
        run.harness
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .count(SNAPSHOT_REMOVE, HookPhase::After),
        3,
        "three snapshots created, three removed"
    );
    assert!(process.balances());
}

#[test]
fn gates_take_no_slot_and_the_worker_and_reviewers_do() {
    let mut run = Run::started("slots");
    let dispatched = run.dispatch(ALPHA, 0);
    let plan = run.attempt_plan(ALPHA, 1);
    let mut process = Process::new();

    let started = context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("start");
    assert!(is_slotted(&started.identities.worker()));
    assert!(is_slotted(&started.identities.review_pass(0, 0)));
    assert!(!is_slotted(&started.identities.gate(0, 0)));

    let refusal = process
        .slots
        .acquire(
            &started.identities.gate(0, 0),
            SlotPair {
                agent: "claude-code".to_owned(),
                pool: None,
            },
        )
        .expect_err("a gate takes no pair");
    assert!(
        refusal.to_string().contains("acquires no slot"),
        "{refusal}"
    );

    agent_edits(&dispatched.worktree);
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("capture");
    let review_inputs = run.review_inputs();
    let assessed = context!(run, process)
        .assess(
            dispatched.site(),
            &plan,
            &started,
            &capture,
            &review_inputs.diff,
            crate::ir::TaskKind::Implement,
        )
        .expect("the scaffold's adapter parses its own worker output");
    context!(run, process)
        .judge(
            dispatched.site(),
            &plan,
            Judging {
                run: &started,
                capture: &capture,
                assessed: &assessed,
            },
            &review_inputs,
            &|pass| crate::review::ReviewInvocations {
                pass: started.identities.review_pass(pass, 0),
                reask: started.identities.review_reask(pass, 0),
            },
        )
        .expect("judge");

    let ran = run.runner.ran();
    assert_eq!(ran.len(), 4, "one worker, one gate, two reviewers");
    assert_eq!(ran[0].role, crate::runner::ExecutionRole::Implement);
    assert_eq!(
        ran[0].command, plan.worker,
        "the worker's request carries the plan's command, not a rebuilt one"
    );
    assert_eq!(ran[1].command, plan.gates[0].command);
    assert_eq!(ran[1].role, crate::runner::ExecutionRole::Gate);
    assert!(
        ran[1].agent.is_none(),
        "a gate runs no agent CLI and is bound to no agent"
    );
    assert_eq!(ran[2].role, crate::runner::ExecutionRole::Review);
    assert_eq!(ran[3].role, crate::runner::ExecutionRole::Review);
    assert_ne!(
        ran[2].agent, ran[3].agent,
        "the two reviewers are two agents, so a pair taken for one is not the other's"
    );
    assert!(process.balances(), "every pair and registration settled");
}

fn retained_tree(worktree: &Path) -> String {
    agent_edits(worktree);
    git(worktree, &["add", "-A"]);
    git(worktree, &["write-tree"])
}

fn settle_retry(
    run: &mut Run,
    reservations: &mut Reservations,
    dispatched: &Dispatched,
    retained: &str,
) -> RetryOutcome {
    let plan = run.attempt_plan(ALPHA, 2);
    let request = RetryRequest {
        key: ALPHA,
        slot: dispatched.slot.clone(),
        retained_tree: retained.to_owned(),
        binding: plan.binding.clone(),
        rung: plan.rung,
        pool: plan.pool.clone(),
        materialization: None,
    };
    settle::retry(
        run.emitter.fold(),
        reservations,
        &ManagedWorktrees::new(&run.fixture.manager),
        run.hooks.effects(),
        &request,
    )
    .expect("a worktree that does not verify is a decision, not an error")
}

fn authorized_plan(run: &Run, authorized: &AttemptStarted4) -> AttemptPlan {
    AttemptPlan {
        attempt: authorized.attempt,
        rung: authorized.rung,
        binding: authorized.binding.clone(),
        pool: authorized.pool.clone(),
        resume_session: authorized.resume_session.clone(),
        materialization_observed: authorized.materialization_observed,
        ..run.attempt_plan(ALPHA, authorized.attempt.0)
    }
}

#[test]
fn a_retry_verifies_once_then_appends_then_spawns() {
    let mut run = Run::started("o24");
    let dispatched = run.dispatch(ALPHA, 0);
    let mut process = Process::new();

    let plan = run.attempt_plan(ALPHA, 1);
    context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("the first attempt");
    let tree = retained_tree(&dispatched.worktree);
    assert_ne!(
        tree,
        git(
            &run.fixture.base,
            &["rev-parse", &format!("{}^{{tree}}", dispatched.base.0)]
        ),
        "the retained tree must differ from the base's, or `HoldsTree` and `AtBase` \
         cannot be told apart"
    );
    run.retain(ALPHA, GENERATION, 1);
    assert!(matches!(
        run.emitter.generation_class(ALPHA, GENERATION),
        GenerationClass::RetainedIdle { .. }
    ));

    let mark = run.mark();
    let mut reservations = Reservations::new();
    let outcome = settle_retry(&mut run, &mut reservations, &dispatched, &tree);
    let RetryOutcome::Start(authorized) = outcome else {
        panic!("a retained worktree that verifies starts the attempt");
    };
    assert!(
        !reservations.is_empty(),
        "`permits.provisional_reservations` calls the reservation the bridge between the \
         selection and its first append, and nothing is holding it"
    );
    assert_eq!(
        run.count_after(mark, VERIFY, HookPhase::Before),
        1,
        "the retry observed the worktree exactly once"
    );

    let retry = authorized_plan(&run, &authorized);
    let started = context!(run, process)
        .start(dispatched.site(), &retry)
        .expect("the retry starts");
    reservations
        .convert(ALPHA, ReservationKind::Retry)
        .expect("converted at `attempt_started(retry)`");
    assert!(
        reservations.balances(),
        "the reservation was taken once and settled once"
    );

    assert_eq!(
        run.count_after(mark, VERIFY, HookPhase::Before),
        1,
        "and still exactly once after the append: the second half of O24 re-observes nothing"
    );

    assert_eq!(
        run.count_after(mark, SCRUB, HookPhase::Before),
        0,
        "the retained worktree is reused, not rebuilt"
    );
    assert_eq!(
        git(&dispatched.worktree, &["write-tree"]),
        tree,
        "and it still holds the retained cumulative tree"
    );

    let verify = run.order_after(mark, VERIFY, HookPhase::Before);
    let append = run.order_after(mark, APPEND, HookPhase::Before);
    assert!(
        verify < append,
        "O24: this retry's verification is at {verify} and its append at {append}, and the \
         verification must come first"
    );
    assert_eq!(
        run.count_after(mark, APPEND, HookPhase::Before),
        1,
        "exactly one append for the retry"
    );

    let durable = run.emitter.durable_events();
    assert_eq!(
        durable.last().map(|event| &event.body),
        Some(
            &crate::topology::events::TopologyEventBody::AttemptStarted {
                data: (*authorized).clone(),
            }
        ),
        "the appended event is not the one the verification authorized"
    );
    assert_eq!(authorized.generation, GENERATION);
    assert_eq!(
        authorized.attempt,
        crate::topology::events::AttemptNumber(2),
        "a retry is a new attempt number"
    );
    assert_eq!(
        authorized.resume_session,
        Some(SessionId(RETAINED_SESSION.to_owned())),
        "and it resumes the session the generation retained"
    );

    assert_eq!(
        run.runner.ran().len(),
        2,
        "the retry's worker is the second process, and it ran after the append"
    );
    assert_eq!(
        run.runner.ran()[1].invocation,
        started.identities.worker(),
        "INV-20: a retry is a new attempt number, so its worker is a new identity"
    );
    assert_eq!(
        run.runner.ran()[1]
            .durable_at_spawn
            .last()
            .map(String::as_str),
        Some("attempt_started"),
        "O24's fourth step: the retry's append is on disk when its worker is asked for"
    );
    assert_ne!(
        run.runner.ran()[0].invocation,
        run.runner.ran()[1].invocation
    );
    assert!(process.balances());
}

fn git_dir(worktree: &Path) -> PathBuf {
    PathBuf::from(git(worktree, &["rev-parse", "--absolute-git-dir"]))
}

#[test]
fn a_retry_whose_retained_worktree_fails_verification_closes_and_destroys_nothing() {
    let mut run = Run::started("inv06");
    let dispatched = run.dispatch(ALPHA, 0);
    let mut process = Process::new();

    let plan = run.attempt_plan(ALPHA, 1);
    context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("the first attempt");
    let tree = retained_tree(&dispatched.worktree);
    let base_tree = git(
        &run.fixture.base,
        &["rev-parse", &format!("{}^{{tree}}", dispatched.base.0)],
    );
    assert_ne!(
        tree, base_tree,
        "the retained tree must differ from the base's, or a recreate at the base would \
         reproduce it and this test could not tell the two apart"
    );
    run.retain(ALPHA, GENERATION, 1);

    let lock = git_dir(&dispatched.worktree).join("index.lock");
    write_file(&lock, b"");

    let durable_before = run.emitter.durable_kinds();
    let mark = run.mark();
    let mut reservations = Reservations::new();
    let outcome = settle_retry(&mut run, &mut reservations, &dispatched, &tree);

    let RetryOutcome::Close { closed, failure } = outcome else {
        panic!("a retained worktree that does not verify is not retried into");
    };
    assert_eq!(
        failure,
        VerifyFailure::Residue(ResidueElement::IndexLock),
        "what `Worktree.Verify` actually observed"
    );
    assert_eq!(closed.reason, GenerationCloseReason::WorktreeMissing);
    assert_eq!(closed.key, ALPHA);
    assert_eq!(closed.generation, GENERATION);
    assert!(
        reservations.is_empty() && reservations.balances(),
        "a pre-append failure left the provisional `{{pipeline}}` reservation held"
    );

    assert_eq!(
        run.count_after(mark, VERIFY, HookPhase::Before),
        1,
        "the retry verified exactly once"
    );
    assert_eq!(
        run.count_after(mark, SCRUB, HookPhase::Before),
        0,
        "INV-06: a retained worktree is never removed by a retry"
    );
    assert_eq!(
        run.count_after(mark, APPEND, HookPhase::Before),
        0,
        "O24: nothing durable follows a verification that failed"
    );
    assert_eq!(
        run.emitter.durable_kinds(),
        durable_before,
        "and in particular no `attempt_started(retry)` claiming the retained session"
    );
    assert_eq!(
        run.emitter.generation_class(ALPHA, GENERATION),
        GenerationClass::RetainedIdle {
            session: SessionId(RETAINED_SESSION.to_owned()),
            incarnation: crate::topology::events::Epoch(0),
        },
        "the generation closes at the closure's append and not before it"
    );
    assert!(
        run.runner.ran().len() == 1,
        "only the first attempt's worker ever ran; the retry asked for no process"
    );

    run.emitter
        .emit(
            TopologyEventBody::GenerationClosed { data: closed },
            &mut run.hooks,
        )
        .expect("`generation_closed{WorktreeMissing}` is the tabled recovery");
    assert!(
        matches!(
            run.emitter.generation_class(ALPHA, GENERATION),
            GenerationClass::Closed
        ),
        "the retained generation is closed, not rebuilt"
    );
    assert_eq!(
        run.count_after(mark, SCRUB, HookPhase::Before),
        0,
        "and closing it removed nothing either"
    );

    assert!(
        dispatched.worktree.is_dir(),
        "the retained worktree is still there"
    );
    remove_file(&lock);
    assert_eq!(
        git(&dispatched.worktree, &["write-tree"]),
        tree,
        "and it still holds the cumulative tree the generation retained, byte for byte"
    );
    assert!(process.balances());
}

#[test]
fn a_refused_slot_acquisition_settles_the_registration_it_took() {
    let mut run = Run::started("slotrefusal");
    let dispatched = run.dispatch(ALPHA, 0);
    let plan = run.attempt_plan(ALPHA, 1);
    let mut process = Process::new();

    let squatter = AttemptIdentities::new(ALPHA, GenerationId(9), AttemptNumber(9)).worker();
    process
        .slots
        .acquire(
            &squatter,
            SlotPair {
                agent: AGENT.to_owned(),
                pool: Some("scaffold-pool".to_owned()),
            },
        )
        .expect("the pair a worker's role takes");

    let worker = AttemptIdentities::new(dispatched.key, dispatched.generation, plan.attempt)
        .worker()
        .render();
    let error = context!(run, process)
        .start(dispatched.site(), &plan)
        .expect_err("a second slotted invocation is refused at max_parallel = 1");
    assert!(
        error.to_string().contains("asked for a slot pair while"),
        "the refusal must be the slot assertion's, and said: {error}"
    );

    assert!(
        !process.ledger.running().contains(&worker.as_str()),
        "the worker's registration was abandoned `Running`: {:?}",
        process.ledger.running()
    );
    assert!(
        process.ledger.balances(),
        "and the ledger no longer balances, so a real leak would be indistinguishable from \
         this: {:?}",
        process.ledger.running()
    );
    assert_eq!(
        process.ledger.cancelled(),
        1,
        "cancelled, not completed: no process ran"
    );
    assert_eq!(process.ledger.completed(), 0);
    assert_eq!(
        process.ledger.duplicates(),
        0,
        "and it was settled exactly once"
    );

    assert_eq!(
        run.emitter.durable_kinds().last().copied(),
        Some("attempt_started"),
        "the refusal is after O23's append, which is where the register sits"
    );
    assert!(
        run.runner.ran().is_empty(),
        "and the Runner was never reached"
    );
}

#[test]
#[ignore = "spawned as a subprocess by the T-ATTEMPT kill tests"]
fn attempt_kill_child() {
    let (dir, which) = kill_child_environment();
    let mut run = Run::started("killattempt");
    run.hand_off(&dir);
    let dispatched = run.dispatch(ALPHA, 0);
    let plan = run.attempt_plan(ALPHA, 1);
    let mut process = Process::new();
    let started = context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("start");

    if which == "in_attempt" {
        run.arm(STAGE, HookPhase::Before, Injection::Kill);
        let _ = context!(run, process).capture(dispatched.site());
        unreachable!("the kill must have taken this process");
    }

    if which == "retry" {
        let tree = retained_tree(&dispatched.worktree);
        run.retain(ALPHA, GENERATION, 1);
        let mut reservations = Reservations::new();
        let RetryOutcome::Start(authorized) =
            settle_retry(&mut run, &mut reservations, &dispatched, &tree)
        else {
            panic!("the retained worktree of this prefix verifies");
        };
        let retry = authorized_plan(&run, &authorized);
        run.arm(STAGE, HookPhase::Before, Injection::Kill);
        context!(run, process)
            .start(dispatched.site(), &retry)
            .expect("the retry starts");
        reservations
            .convert(ALPHA, ReservationKind::Retry)
            .expect("converted at `attempt_started(retry)`");
        let _ = context!(run, process).capture(dispatched.site());
        unreachable!("the kill must have taken this process");
    }

    agent_edits(&dispatched.worktree);
    if which == "after_capture" {
        run.arm(WRITE_TREE, HookPhase::After, Injection::Kill);
        let _ = context!(run, process).capture(dispatched.site());
        unreachable!("the kill must have taken this process");
    }

    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("capture");
    match which.as_str() {
        "after_snapshot_commit" => run.arm(SNAPSHOT_COMMIT, HookPhase::After, Injection::Kill),
        "id_unread" => run.arm_point(
            SNAPSHOT_COMMIT,
            SubEffectPoint::IdUnread,
            InjectionMode::Kill,
        ),
        "after_snapshot_add" => run.arm(SNAPSHOT_ADD, HookPhase::After, Injection::Kill),
        other => panic!("unknown site `{other}`"),
    }
    let review_inputs = run.review_inputs();
    let assessed = context!(run, process)
        .assess(
            dispatched.site(),
            &plan,
            &started,
            &capture,
            &review_inputs.diff,
            crate::ir::TaskKind::Implement,
        )
        .expect("the scaffold's adapter parses its own worker output");
    let _ = context!(run, process).judge(
        dispatched.site(),
        &plan,
        Judging {
            run: &started,
            capture: &capture,
            assessed: &assessed,
        },
        &review_inputs,
        &|pass| crate::review::ReviewInvocations {
            pass: started.identities.review_pass(pass, 0),
            reask: started.identities.review_reask(pass, 0),
        },
    );
    unreachable!("the kill must have taken this process");
}

fn adopted_generation(run: &Run) -> Dispatched {
    Dispatched {
        key: ALPHA,
        generation: GENERATION,
        base: run.base(),
        slot: crate::engine::topology::dispatch::task_slot(ALPHA, GENERATION),
        worktree: run
            .fixture
            .manager
            .slot_path(&crate::engine::topology::dispatch::task_slot(
                ALPHA, GENERATION,
            )),
        kind: DispatchKind::Ordinary {
            paths: run.predicted(ALPHA),
        },
        materialized: None,
    }
}

const CHILD: &str = "engine::topology::attempt::tests::attempt_kill_child";

#[test]
fn kill_during_attempt_settles_interrupted_and_redispatches_new_generation() {
    let dir = kill_dir("killattempt");
    let mut run = kill_child_and_adopt(CHILD, &dir, "in_attempt");
    let dispatched = adopted_generation(&run);
    let mut process = Process::new();

    assert_eq!(
        run.emitter.durable_kinds(),
        vec!["run_started", "task_dispatched", "attempt_started"],
        "the child died in flight"
    );
    assert_eq!(
        run.emitter.generation_class(ALPHA, GENERATION),
        GenerationClass::InFlight {
            attempt: crate::topology::events::AttemptNumber(1)
        }
    );

    context!(run, process)
        .settle_interrupted(
            &dispatched,
            crate::topology::events::AttemptNumber(1),
            AttemptOutcome::Interrupted,
        )
        .expect("settle");

    let events = run.emitter.durable_events();
    let crate::topology::events::TopologyEventBody::AttemptInterrupted { data } =
        &events.last().expect("a terminal").body
    else {
        panic!("the last durable event is not an interruption");
    };
    assert_eq!(
        data.lease,
        crate::topology::events::LeaseDisposition::PredictedReleased,
        "an ordinary generation releases its predicted region when it closes"
    );
    assert!(data.detail.contains("unknown"), "{}", data.detail);
    assert_eq!(
        run.emitter.generation_class(ALPHA, GENERATION),
        GenerationClass::Closed
    );
    assert_eq!(run.task_state(ALPHA), TaskState::Pending);
    assert!(
        !dispatched.worktree.exists(),
        "the task worktree is scrubbed with force"
    );
    assert!(
        run.fixture.manager.intents().expect("intents").is_empty(),
        "and its intent left with it"
    );

    let next = run.dispatch(ALPHA, 1);
    assert_eq!(next.generation, GenerationId(1));
    assert_eq!(
        run.emitter.generation_class(ALPHA, GenerationId(1)),
        GenerationClass::OpenNoAttempt
    );
    assert!(next.worktree.is_dir());
    assert_ne!(
        next.worktree, dispatched.worktree,
        "a new generation, a new worktree"
    );
}

#[test]
fn kill_after_capture_leaves_index_referenced_objects_then_scrub_releases_them() {
    let dir = kill_dir("killcapture");
    let mut run = kill_child_and_adopt(CHILD, &dir, "after_capture");
    let dispatched = adopted_generation(&run);
    let mut process = Process::new();

    let staged = index_blobs(&dispatched.worktree);
    assert!(
        !staged.is_empty(),
        "the child's capture left an index with staged blobs"
    );
    let worked = git(&dispatched.worktree, &["hash-object", WORKED_PATH]);
    assert!(
        staged.contains(&worked),
        "the agent's file is staged: {staged:?} does not hold {worked}"
    );

    let before = unreachable_objects(&run.fixture.base).expect("fsck");
    assert!(
        !before.contains(&worked),
        "R9: an object the task index holds is reachable, and fsck reported it unreachable"
    );

    context!(run, process)
        .settle_interrupted(
            &dispatched,
            crate::topology::events::AttemptNumber(1),
            AttemptOutcome::Interrupted,
        )
        .expect("settle");

    assert!(!dispatched.worktree.exists());
    let after = unreachable_objects(&run.fixture.base).expect("fsck");
    assert!(
        after.contains(&worked),
        "R27: the scrub released the staged object to Git, and it is still referenced"
    );
    assert!(
        run.observed(SCRUB, HookPhase::After),
        "the release is the forced scrub's, not something else's"
    );
}

#[test]
fn kill_after_ephemeral_snapshot_commit_before_worktree_leaves_gc_owned_object() {
    let dir = kill_dir("killephemeral");
    let mut run = kill_child_and_adopt(CHILD, &dir, "after_snapshot_commit");
    let dispatched = adopted_generation(&run);
    let mut process = Process::new();

    let orphans = unreachable_ephemeral_commits(&run.fixture.base);
    assert_eq!(
        orphans.len(),
        1,
        "exactly one ephemeral commit, unreferenced: {orphans:?}"
    );
    assert!(
        run.fixture
            .manager
            .intents()
            .expect("intents")
            .iter()
            .all(|slot| !matches!(slot, crate::workspace_manager::Slot::Snapshot { .. })),
        "nothing durable claims it"
    );
    assert!(
        !run.fixture
            .manager
            .worktree_records()
            .expect("worktree records")
            .iter()
            .any(|record| record
                .path()
                .starts_with(run.fixture.manager.execution_root().join("snapshots"))),
        "and no snapshot worktree was ever registered"
    );

    context!(run, process)
        .settle_interrupted(
            &dispatched,
            crate::topology::events::AttemptNumber(1),
            AttemptOutcome::Interrupted,
        )
        .expect("settle");

    assert_eq!(
        unreachable_ephemeral_commits(&run.fixture.base),
        orphans,
        "the recovery leaves an unreferenced object to Git rather than pruning it"
    );
}

#[test]
fn kill_at_snapshot_commit_id_unread_point_leaves_gc_owned_object() {
    let dir = kill_dir("killidunread");
    let run = kill_child_and_adopt(CHILD, &dir, "id_unread");

    let orphans = unreachable_ephemeral_commits(&run.fixture.base);
    assert_eq!(
        orphans.len(),
        1,
        "the object was written before the coordinator could record its id: {orphans:?}"
    );
    assert!(
        run.fixture
            .manager
            .intents()
            .expect("intents")
            .iter()
            .all(|slot| !matches!(slot, crate::workspace_manager::Slot::Snapshot { .. })),
        "and nothing durable names it"
    );
    let refusal = run
        .harness
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .arm(
            SNAPSHOT_COMMIT,
            SubEffectPoint::IdUnread,
            InjectionMode::ErrorReturn,
        )
        .expect_err("IdUnread supports Kill only");
    assert!(
        refusal.to_string().contains("ErrorReturn"),
        "the refusal must name the mode it will not arm: {refusal}"
    );
}

#[test]
fn kill_after_snapshot_add_reclaims_snapshot_and_releases_its_commit() {
    let dir = kill_dir("killsnapshotadd");
    let mut run = kill_child_and_adopt(CHILD, &dir, "after_snapshot_add");
    let dispatched = adopted_generation(&run);
    let mut process = Process::new();

    let snapshots: Vec<crate::workspace_manager::Slot> = run
        .fixture
        .manager
        .intents()
        .expect("intents")
        .into_iter()
        .filter(|slot| matches!(slot, crate::workspace_manager::Slot::Snapshot { .. }))
        .collect();
    assert_eq!(
        snapshots.len(),
        1,
        "one snapshot intent survives: {snapshots:?}"
    );
    let path = run.fixture.manager.slot_path(&snapshots[0]);
    assert!(path.is_dir(), "and its worktree was added");

    let head = git(&path, &["rev-parse", "HEAD"]);
    assert!(
        !unreachable_objects(&run.fixture.base)
            .expect("fsck")
            .contains(&head),
        "R24: the snapshot's HEAD keeps its ephemeral commit reachable"
    );
    assert_eq!(
        git(&run.fixture.base, &["log", "-1", "--format=%s", &head]),
        "upstroke: ephemeral snapshot input"
    );

    context!(run, process)
        .settle_interrupted(
            &dispatched,
            crate::topology::events::AttemptNumber(1),
            AttemptOutcome::Interrupted,
        )
        .expect("settle");

    assert!(!path.exists(), "the snapshot worktree was reclaimed");
    assert!(
        run.fixture.manager.intents().expect("intents").is_empty(),
        "with its intent"
    );
    assert!(
        unreachable_objects(&run.fixture.base)
            .expect("fsck")
            .contains(&head),
        "R27: and the ephemeral commit went back to Git"
    );
}

#[test]
fn kill_during_retry_attempt_closes_generation() {
    let dir = kill_dir("killretry");
    let mut run = kill_child_and_adopt(CHILD, &dir, "retry");
    let dispatched = adopted_generation(&run);
    let mut process = Process::new();

    assert_eq!(
        run.emitter.durable_kinds(),
        vec![
            "run_started",
            "task_dispatched",
            "attempt_started",
            "attempt_finished",
            "attempt_started"
        ],
        "the child retained and then started a retry"
    );
    assert_eq!(
        run.emitter.generation_class(ALPHA, GENERATION),
        GenerationClass::InFlight {
            attempt: crate::topology::events::AttemptNumber(2)
        },
        "the retry re-entered the same generation"
    );

    context!(run, process)
        .settle_interrupted(
            &dispatched,
            crate::topology::events::AttemptNumber(2),
            AttemptOutcome::Interrupted,
        )
        .expect("settle");

    let events = run.emitter.durable_events();
    let crate::topology::events::TopologyEventBody::AttemptInterrupted { data } =
        &events.last().expect("a terminal").body
    else {
        panic!("the last durable event is not an interruption");
    };
    assert_eq!(
        data.attempt,
        crate::topology::events::AttemptNumber(2),
        "the terminal names the retry, not the attempt that retained"
    );
    assert_eq!(
        run.emitter.generation_class(ALPHA, GENERATION),
        GenerationClass::Closed,
        "a generation does not survive an interruption"
    );
    assert_eq!(run.task_state(ALPHA), TaskState::Pending);
    assert!(!dispatched.worktree.exists());
}

#[test]
fn halt_cancels_in_flight_attempt() {
    let mut run = Run::started("halt");
    let dispatched = run.dispatch(ALPHA, 0);
    let plan = run.attempt_plan(ALPHA, 1);
    let mut process = Process::new();

    let started = context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("start");

    let reviewer = started.identities.review_pass(0, 0);
    process.ledger.register(&reviewer).expect("register");
    process
        .slots
        .acquire(
            &reviewer,
            SlotPair {
                agent: "claude-code".to_owned(),
                pool: Some("scaffold-pool".to_owned()),
            },
        )
        .expect("the pair its role takes");
    assert!(!process.balances(), "the run is genuinely in flight");
    assert_eq!(process.ledger.running(), vec![reviewer.render()]);

    let cancelled = context!(run, process)
        .cancel_in_flight(&dispatched, crate::topology::events::AttemptNumber(1))
        .expect("halt");

    assert_eq!(cancelled, 1, "the in-flight invocation was cancelled");
    assert!(
        process.balances(),
        "and both ledgers balance: slots={:?} running={:?}",
        process.slots.is_empty(),
        process.ledger.running()
    );

    let events = run.emitter.durable_events();
    let crate::topology::events::TopologyEventBody::AttemptInterrupted { data } =
        &events.last().expect("a terminal").body
    else {
        panic!("a halt appends the same terminal an interruption does");
    };
    assert!(data.detail.contains("halted"), "{}", data.detail);
    assert_eq!(
        run.emitter.generation_class(ALPHA, GENERATION),
        GenerationClass::Closed
    );
    assert_eq!(run.task_state(ALPHA), TaskState::Pending);
    assert!(
        !dispatched.worktree.exists(),
        "and the residue is discarded, the same way an interruption discards it"
    );
}

fn stage_elements() -> Vec<ResidueElement> {
    STAGE.residue_elements().to_vec()
}

fn unstaged_work(worktree: &Path) {
    write_file(
        &worktree.join("staging.txt"),
        b"work the interrupted `git add` never finished staging\n",
    );
}

fn plant_stage_residue(base: &Path, worktree: &Path, element: ResidueElement) {
    match element {
        ResidueElement::UnreferencedObject => {
            write_file(
                &worktree.join("orphan.txt"),
                b"an object nothing references\n",
            );
            let id = git(worktree, &["hash-object", "-w", "orphan.txt"]);
            assert!(
                unreachable_objects(base).expect("fsck").contains(&id),
                "the planted blob must really be unreachable"
            );
        }
        ResidueElement::TemporaryObjectFile => {
            let objects = object_directory(worktree).expect("the object directory");
            write_file(&objects.join("tmp_obj_synthetic"), b"half an object\n");
            assert!(temporary_object_files(worktree).expect("temp files"));
        }
        ResidueElement::IndexLock => {
            let dir = PathBuf::from(git(worktree, &["rev-parse", "--absolute-git-dir"]));
            write_file(&dir.join("index.lock"), b"");
        }
        other => panic!("`{other:?}` is not registered for Object.CandidateStage"),
    }
}

#[test]
fn synthetic_git_add_residue_unreferenced_objects_and_index_lock_then_forced_scrub_converges() {
    let elements = stage_elements();
    assert_eq!(
        elements.len(),
        3,
        "Object.CandidateStage registers three elements and this test constructs each: \
         {elements:?}"
    );

    for element in &elements {
        let fixture = Fixture::created("synthetic-stage");
        let manager = &fixture.manager;
        let slot = crate::workspace_manager::Slot::Task {
            key: "synth".to_owned(),
            generation: 0,
        };
        manager.write_intent(&mut NoHooks, &slot).expect("intent");
        let worktree = manager
            .add_worktree(&mut NoHooks, &slot, &fixture.head)
            .expect("worktree");

        let target = ResidueTarget::new(&fixture.base).at(&worktree);
        assert_eq!(
            classify_object_residue(STAGE, &target).expect("classify"),
            ObjectResidue::After,
            "{element:?}: a worktree whose index reflects its tree is the *finished* state"
        );
        unstaged_work(&worktree);
        assert_eq!(
            classify_object_residue(STAGE, &target).expect("classify"),
            ObjectResidue::None,
            "{element:?}: the after-phase reference is now absent and no element is present \
             yet, which is neither class"
        );

        plant_stage_residue(&fixture.base, &worktree, *element);
        assert_eq!(
            observed_residue_elements(STAGE, &target).expect("observe"),
            vec![*element],
            "{element:?}: exactly this element is present, so the classification below is \
             about it and not about a neighbour"
        );
        assert_eq!(
            classify_object_residue(STAGE, &target).expect("classify"),
            ObjectResidue::Internal,
            "{element:?}: the objects-written-reference-unpublished prefix is `Internal`"
        );

        manager
            .remove_worktree(&mut NoHooks, &slot)
            .expect("forced removal converges");
        manager
            .remove_intent(&mut NoHooks, &slot)
            .expect("intent removal converges");
        assert!(!worktree.exists(), "{element:?}: the worktree is gone");
        assert!(
            !manager
                .worktree_records()
                .expect("records")
                .iter()
                .any(|record| crate::util::same_path(record.path(), &worktree)),
            "{element:?}: and it is no longer registered"
        );
        assert!(
            !manager.intent_path(&slot).exists(),
            "{element:?}: and its durable intent left with it"
        );
        manager
            .remove_worktree(&mut NoHooks, &slot)
            .expect("a second removal converges");
        manager
            .remove_intent(&mut NoHooks, &slot)
            .expect("a second intent removal converges");
    }

    let fixture = Fixture::created("synthetic-stage-all");
    let manager = &fixture.manager;
    let slot = crate::workspace_manager::Slot::Task {
        key: "synth".to_owned(),
        generation: 0,
    };
    manager.write_intent(&mut NoHooks, &slot).expect("intent");
    let worktree = manager
        .add_worktree(&mut NoHooks, &slot, &fixture.head)
        .expect("worktree");
    unstaged_work(&worktree);
    for element in &elements {
        plant_stage_residue(&fixture.base, &worktree, *element);
    }
    let target = ResidueTarget::new(&fixture.base).at(&worktree);
    let mut observed = observed_residue_elements(STAGE, &target).expect("observe");
    observed.sort();
    let mut expected = elements.clone();
    expected.sort();
    assert_eq!(observed, expected, "every registered element is present");
    assert_eq!(
        classify_object_residue(STAGE, &target).expect("classify"),
        ObjectResidue::Internal
    );

    let orphan = git(&worktree, &["hash-object", "orphan.txt"]);
    manager
        .remove_worktree(&mut NoHooks, &slot)
        .expect("forced removal converges");
    manager
        .remove_intent(&mut NoHooks, &slot)
        .expect("intent removal converges");
    assert!(!worktree.exists());
    assert!(
        unreachable_objects(&fixture.base)
            .expect("fsck")
            .contains(&orphan),
        "the orphan blob is R27 and is Git's to prune, not the engine's"
    );
    assert!(
        temporary_object_files(&fixture.base).expect("temp files"),
        "and so is the temporary object file"
    );
}

const SAMPLED: [EffectSiteId; 2] = [STAGE, WRITE_TREE];

const SAMPLING_N: u32 = 8;

const HISTOGRAM: &str = "effects/attempt-residue-histogram.json";

struct Sample {
    argv: Vec<String>,
    after: std::time::Duration,
    ran: Option<std::time::Duration>,
    fired: Option<std::time::Duration>,
    killed: bool,
    failed: Option<i32>,
    class: Option<ObjectResidue>,
    recovered: bool,
}

fn bulk(worktree: &Path) {
    for directory in 0..60 {
        for index in 0..20 {
            write_file(
                &worktree.join(format!("bulk{directory}/f{index}.txt")),
                format!("{directory}-{index}-{}", "x".repeat(2048)).as_bytes(),
            );
        }
    }
}

fn sampled_argv(site: EffectSiteId) -> Vec<String> {
    let fixed = |argv: &[&str]| -> Vec<String> { argv.iter().map(|a| (*a).to_owned()).collect() };
    match site {
        STAGE => fixed(&crate::workspace_manager::WorkspaceManager::CANDIDATE_STAGE_ARGV),
        WRITE_TREE => fixed(&crate::workspace_manager::WorkspaceManager::CANDIDATE_WRITE_TREE_ARGV),
        other => panic!("`{other}` is not one of the two capture commands"),
    }
}

fn populate_for(site: EffectSiteId, worktree: &Path) {
    bulk(worktree);
    if site == WRITE_TREE {
        git(worktree, &["add", "-A"]);
    }
}

fn sample_slot(generation: u32) -> crate::workspace_manager::Slot {
    crate::workspace_manager::Slot::Task {
        key: "sample".to_owned(),
        generation,
    }
}

fn measure_budget(site: EffectSiteId, fixture: &Fixture) -> std::time::Duration {
    const PROBE_SLOTS: [u32; 4] = [9_996, 9_997, 9_998, 9_999];

    let mut measured = Vec::with_capacity(PROBE_SLOTS.len());
    for slot_id in PROBE_SLOTS {
        let probe = sample_slot(slot_id);
        fixture
            .manager
            .write_intent(&mut NoHooks, &probe)
            .expect("probe intent");
        let path = fixture
            .manager
            .add_worktree(&mut NoHooks, &probe, &fixture.head)
            .expect("probe worktree");
        populate_for(site, &path);
        measured.push(time_git(&path, &sampled_argv(site)));
        fixture
            .manager
            .remove_worktree(&mut NoHooks, &probe)
            .expect("remove the probe");
        fixture
            .manager
            .remove_intent(&mut NoHooks, &probe)
            .expect("remove the probe intent");
    }
    median(&measured[1..])
}

fn median(durations: &[std::time::Duration]) -> std::time::Duration {
    let mut sorted = durations.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2].max(std::time::Duration::from_micros(200))
}

fn sample(site: EffectSiteId) -> Vec<Sample> {
    let fixture = Fixture::created("sampler");
    let budget = measure_budget(site, &fixture);
    let first = sample_once(site, &fixture, budget, 0);
    if first.iter().any(|sample| sample.killed) {
        return first;
    }

    let observed: Vec<std::time::Duration> = first.iter().filter_map(|sample| sample.ran).collect();
    if observed.is_empty() {
        return first;
    }
    sample_once(site, &fixture, median(&observed), SAMPLING_N)
}

fn sample_once(
    site: EffectSiteId,
    fixture: &Fixture,
    budget: std::time::Duration,
    slot_base: u32,
) -> Vec<Sample> {
    let mut samples = Vec::new();

    for run in 0..SAMPLING_N {
        let slot = sample_slot(slot_base + run);
        fixture
            .manager
            .write_intent(&mut NoHooks, &slot)
            .expect("intent");
        let path = fixture
            .manager
            .add_worktree(&mut NoHooks, &slot, &fixture.head)
            .expect("worktree");
        populate_for(site, &path);

        let argv = sampled_argv(site);
        let after = budget.mul_f64(f64::from(run + 1) / f64::from(SAMPLING_N + 1));
        let mut child = KillableGitChild::spawn(&path, &argv);
        let deadline = std::time::Instant::now() + after;
        let mut ran = None;
        while std::time::Instant::now() < deadline {
            if ran.is_none() {
                ran = child.exited();
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        child.kill();
        let status = child.wait();

        let target = ResidueTarget::new(&fixture.base).at(&path);
        let class = classify_object_residue(site, &target).ok();

        fixture
            .manager
            .remove_worktree(&mut NoHooks, &slot)
            .expect("forced removal converges");
        fixture
            .manager
            .remove_intent(&mut NoHooks, &slot)
            .expect("intent removal converges");
        let recovered = !path.exists()
            && !fixture
                .manager
                .worktree_records()
                .expect("records")
                .iter()
                .any(|record| crate::util::same_path(record.path(), &path));

        samples.push(Sample {
            argv,
            after,
            ran,
            fired: child.fired(),
            killed: died_by_kill(&status),
            failed: (!status.success() && !died_by_kill(&status))
                .then(|| status.code())
                .flatten(),
            class,
            recovered,
        });
    }
    samples
}

#[test]
fn sampled_git_add_and_write_tree_child_kills_every_residue_classified_and_recovered() {
    let mut per_site = Vec::new();
    for site in SAMPLED {
        let samples = sample(site);
        assert_eq!(
            samples.len(),
            SAMPLING_N as usize,
            "{site}: one observation per sample"
        );

        let counted = |wanted: ObjectResidue| -> u32 {
            u32::try_from(
                samples
                    .iter()
                    .filter(|sample| sample.class == Some(wanted))
                    .count(),
            )
            .expect("a sample count fits in u32")
        };
        let (none, internal, after) = (
            counted(ObjectResidue::None),
            counted(ObjectResidue::Internal),
            counted(ObjectResidue::After),
        );
        let unclassified = u32::try_from(
            samples
                .iter()
                .filter(|sample| sample.class.is_none())
                .count(),
        )
        .expect("a sample count fits in u32");
        assert_eq!(
            none + internal + after + unclassified,
            SAMPLING_N,
            "{site}: every sample is accounted for by exactly one class"
        );
        assert_eq!(
            unclassified, 0,
            "{site}: an unclassifiable residue is durable state no tabled action recovers"
        );
        assert!(
            samples.iter().all(|sample| sample.recovered),
            "{site}: every sample recovered by its classified action"
        );

        let failed: Vec<Option<i32>> = samples.iter().filter_map(|s| s.failed.map(Some)).collect();
        assert!(
            failed.is_empty(),
            "{site}: a sampled child neither died by the kill nor reached its own successful \
             exit (codes {failed:?}), so what the classifier saw is this fixture's failure"
        );

        let shape: Vec<&Sample> = samples
            .iter()
            .filter(|sample| sample.argv == sampled_argv(site))
            .collect();
        assert_eq!(
            shape.len(),
            SAMPLING_N as usize,
            "{site}: every sample ran this command and not a neighbouring one"
        );
        let delays: Vec<std::time::Duration> = shape.iter().map(|sample| sample.after).collect();
        assert!(
            delays.windows(2).all(|rungs| rungs[0] < rungs[1]),
            "{site}: the N kills must be aimed at N distinct, increasing points through the \
             command, not at one point N times: {delays:?}"
        );

        for sample in &shape {
            let fired = sample.fired.unwrap_or_else(|| {
                panic!(
                    "{site}: a sampled child was never fired at, so no count over these \
                        samples is about kills"
                )
            });
            assert!(
                fired >= sample.after,
                "{site}: a kill fired {fired:?} after its child was spawned, sooner than the \
                 {:?} rung it was aimed at",
                sample.after
            );
        }

        per_site.push((site, none, internal, after, unclassified, samples));
    }
    assert_eq!(per_site.len(), 2, "the two commands sub-prefix (b') names");

    let landed: usize = per_site
        .iter()
        .map(|(_, _, _, _, _, samples)| samples.iter().filter(|sample| sample.killed).count())
        .sum();
    assert!(
        landed > 0,
        "not one of the {} sampled Git children died by the kill — this harness then sampled \
         the residue its commands left when they FINISHED, and every other assertion here \
         accepts that residue. `sample` has already recalibrated from the durations the runs \
         actually took and retried once, so this is the kill failing to land rather than an \
         unrepresentative probe",
        2 * SAMPLING_N
    );

    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(HISTOGRAM);
    let emitted = serde_json::to_string_pretty(&serde_json::json!({
        "note": "decisions.effect_site_inventory.outputs, the observed-class histogram half, \
                 for T-ATTEMPT's capture commands. Written by engine::topology::attempt::tests\
                 ::sampled_git_add_and_write_tree_child_kills_every_residue_classified_and_\
                 recovered on every run. Machine-varying by construction -- which class a \
                 sample lands in is a race between the kill and Git -- so it is emitted here \
                 rather than pinned into effects/residue-classes.json.",
        "sampling_n": SAMPLING_N,
        "sites": per_site
            .iter()
            .map(|(site, none, internal, after, unclassified, samples)| serde_json::json!({
                "site": site.name(),
                "n": SAMPLING_N,
                "none": none,
                "internal": internal,
                "after": after,
                "unclassified": unclassified,
                "killed": samples.iter().filter(|sample| sample.killed).count(),
                "recovered": samples.iter().all(|sample| sample.recovered),
                "ladder_us": samples
                    .iter()
                    .map(|sample| u64::try_from(sample.after.as_micros()).unwrap_or(u64::MAX))
                    .collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>(),
    }))
    .expect("the histogram serializes");
    write_file(&path, (emitted + "\n").as_bytes());

    let back: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read the histogram back"))
            .expect("the emitted histogram parses");
    let sites = back["sites"].as_array().expect("a sites array");
    assert_eq!(sites.len(), 2, "one histogram per sampled command");
    for (entry, (site, ..)) in sites.iter().zip(&per_site) {
        assert_eq!(
            entry["site"],
            site.name(),
            "the sites are in sampling order"
        );
        let total = ["none", "internal", "after", "unclassified"]
            .iter()
            .map(|class| entry[*class].as_u64().expect("a count"))
            .sum::<u64>();
        assert_eq!(
            total,
            u64::from(SAMPLING_N),
            "{site}: the written histogram accounts for every sample"
        );
    }
}

#[test]
fn a_failing_gate_rejects_the_judgement_and_its_snapshot_is_still_cleaned() {
    let mut run = Run::started("rejected");
    run.runner = crate::engine::topology::scaffold::RecordingRunner::failing_with(vec![0, 2]);
    let dispatched = run.dispatch(ALPHA, 0);
    let plan = run.attempt_plan(ALPHA, 1);
    let mut process = Process::new();

    let started = context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("start");
    agent_edits(&dispatched.worktree);
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("capture");
    let review_inputs = run.review_inputs();
    let assessed = context!(run, process)
        .assess(
            dispatched.site(),
            &plan,
            &started,
            &capture,
            &review_inputs.diff,
            crate::ir::TaskKind::Implement,
        )
        .expect("the scaffold's adapter parses its own worker output");
    let judgement = context!(run, process)
        .judge(
            dispatched.site(),
            &plan,
            Judging {
                run: &started,
                capture: &capture,
                assessed: &assessed,
            },
            &review_inputs,
            &|pass| crate::review::ReviewInvocations {
                pass: started.identities.review_pass(pass, 0),
                reask: started.identities.review_reask(pass, 0),
            },
        )
        .expect("judge");

    assert!(!judgement.gates[0].passed(), "the gate exited 2");
    assert_eq!(judgement.gates[0].code, Some(2));
    assert!(!judgement.accepted(), "a failed gate rejects the attempt");

    assert!(
        judgement.reviews.is_empty(),
        "no reviewer runs after a gate has already rejected the work"
    );
    assert!(
        run.runner
            .ran()
            .iter()
            .all(|ran| !matches!(ran.role, crate::runner::ExecutionRole::Review)),
        "and none was spawned — asserted on the runner's record, because an \
         empty `reviews` list could also mean a pass that ran and recorded \
         nothing, which is the one thing `AttemptRecord.reviews` must never be \
         ambiguous about"
    );
    assert!(
        run.fixture
            .manager
            .intents()
            .expect("intents")
            .iter()
            .all(|slot| !matches!(slot, crate::workspace_manager::Slot::Snapshot { .. })),
        "every snapshot was cleaned on completion, pass or fail"
    );
    assert!(process.balances());
}

#[test]
fn a_malformed_captured_id_is_a_git_error_naming_where_the_value_came_from() {
    let malformed = "not-an-object-id".to_owned();
    let error = captured_object_id("`git write-tree`", malformed.clone())
        .expect_err("a value that is not an object id");
    let UpstrokeError::Git { message } = &error else {
        panic!("the engine's own malformed value is a Git error, not a refusal: {error}");
    };
    assert!(
        message.contains("`git write-tree` did not yield an object id")
            && message.contains(&malformed),
        "the message names the source and the value: {message}"
    );

    let good = "0123456789abcdef0123456789abcdef01234567".to_owned();
    assert_eq!(
        captured_object_id("the recorded base commit", good.clone())
            .expect("a full hexadecimal id")
            .as_str(),
        good,
        "and a well-formed id passes through unchanged"
    );
}

fn repair_at(run: &mut Run, base: &str) -> (Dispatched, AttemptPlan) {
    let side = run.fixture.side.clone();
    repair_from(run, base, &side)
}

/// A repair dispatched at `base`, materialized from `source_commit` as its
/// protected candidate.
fn repair_from(run: &mut Run, base: &str, source_commit: &str) -> (Dispatched, AttemptPlan) {
    use crate::engine::topology::dispatch::{DispatchRequest, dispatch};

    let source = run.protect_candidate(source_commit);
    let repair = run.spawn_repair(ALPHA);
    let request = DispatchRequest {
        key: repair,
        generation: GENERATION,
        base: crate::topology::events::CommitSha(base.to_owned()),
        kind: DispatchKind::Repair {
            root: ALPHA,
            source,
        },
    };
    let dispatched = dispatch(
        &run.fixture.manager,
        &mut run.hooks,
        &mut run.emitter,
        &request,
    )
    .expect("the repair dispatches");
    let mut plan = run.attempt_plan(repair, 1);
    plan.materialization_observed = dispatched.materialized;
    (dispatched, plan)
}

fn tree_of(run: &Run, commit: &str) -> String {
    git(
        &run.fixture.base,
        &["rev-parse", &format!("{commit}^{{tree}}")],
    )
}

/// The worker's whole conflict-resolution vocabulary: a file write. It
/// resolves a conflicted file with its file tools and writes the resolution
/// manifest with the same tools; it runs no git command, because an edit
/// profile has none to run (DESIGN §16, §20) and the engine owns git (§4).
fn declare(worktree: &Path, manifest: &str) {
    write_file(
        &worktree.join(crate::workspace_manager::RESOLUTION_MANIFEST),
        manifest.as_bytes(),
    );
}

fn tree_names(worktree: &Path, tree: &str) -> Vec<String> {
    git(worktree, &["ls-tree", "-r", "--name-only", tree])
        .lines()
        .map(str::to_owned)
        .collect()
}

fn unmerged_paths(worktree: &Path) -> Vec<String> {
    let mut paths: Vec<String> = git(worktree, &["diff-files", "--name-only", "--diff-filter=U"])
        .lines()
        .map(str::to_owned)
        .collect();
    paths.sort();
    paths.dedup();
    paths
}

#[test]
fn an_unresolved_conflict_fails_the_capture_before_any_gate_and_a_declared_one_is_staged_by_the_engine()
 {
    use crate::ladder::{FailureKind, FailureOrigin};
    use crate::topology::events::Materialization;
    use crate::workspace_manager::{DELETED_KEYWORD, RESOLUTION_MANIFEST, RESOLVED_KEYWORD};

    let mut run = Run::started("unresolved-conflict");
    let head = run.fixture.head.clone();
    let conflicting = run.commit_with(&head, "c.txt", "not the side's content\n", "c-other");
    let (dispatched, plan) = repair_at(&mut run, &conflicting);
    assert_eq!(dispatched.materialized, Some(Materialization::Conflict));
    let mut process = Process::new();
    let started = context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("the repair's attempt starts");

    let mark = run.mark();
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("the capture reads the conflict rather than failing");
    assert_eq!(
        capture.unresolved,
        vec!["c.txt".to_owned()],
        "the worker touched nothing and declared nothing, so the conflicted path is refused"
    );
    assert_eq!(
        run.count_after(mark, STAGE, HookPhase::Before),
        0,
        "nothing was staged: `git add -A` would have recorded the markers as the resolution"
    );
    assert_eq!(run.count_after(mark, WRITE_TREE, HookPhase::Before), 0);
    assert_eq!(
        capture.tree,
        tree_of(&run, &conflicting),
        "an unresolved capture names the tree the worktree started from"
    );
    assert!(
        git(&dispatched.worktree, &["ls-files", "--unmerged"])
            .lines()
            .count()
            >= 2,
        "and leaves the index unmerged"
    );

    let diff = run
        .fixture
        .manager
        .candidate_diff(&dispatched.slot, &capture.parent, &capture.tree)
        .expect("diff");
    let assessed = context!(run, process)
        .assess(
            dispatched.site(),
            &plan,
            &started,
            &capture,
            &diff,
            crate::ir::TaskKind::Fix,
        )
        .expect("assessed");
    let failure = assessed
        .failure
        .clone()
        .expect("an unresolved conflict fails the attempt before any gate");
    assert_eq!(failure.kind, FailureKind::AgentError);
    assert_eq!(failure.origin, FailureOrigin::Worker);
    assert!(
        failure.reason.contains("c.txt") && failure.reason.contains("unresolved"),
        "the failure names the paths: {}",
        failure.reason
    );
    let feedback = failure.feedback.clone().unwrap_or_default();
    assert!(
        feedback.contains(RESOLUTION_MANIFEST)
            && feedback.contains(&format!("`{RESOLVED_KEYWORD} <path>`"))
            && feedback.contains(&format!("`{DELETED_KEYWORD} <path>`"))
            && feedback.contains("Run no git command"),
        "the worker is told what to do next time — resolve with file tools and declare in the \
         manifest, never a git command: {feedback}"
    );
    assert!(
        !feedback.contains("git add") && !feedback.contains("git rm"),
        "no worker can run either (PR #249, second round): {feedback}"
    );

    let review_inputs = run.review_inputs();
    let judgement = context!(run, process)
        .judge(
            dispatched.site(),
            &plan,
            Judging {
                run: &started,
                capture: &capture,
                assessed: &assessed,
            },
            &review_inputs,
            &|pass| crate::review::ReviewInvocations {
                pass: started.identities.review_pass(pass, 0),
                reask: started.identities.review_reask(pass, 0),
            },
        )
        .expect("judged");
    assert!(
        judgement.gates.is_empty() && judgement.reviews.is_empty(),
        "no gate and no reviewer ran on an unresolved capture"
    );
    assert_eq!(
        run.runner.ran().len(),
        1,
        "the worker was the only process: {:?}",
        run.runner.ran()
    );

    write_file(
        &dispatched.worktree.join("c.txt"),
        b"resolved by the worker\n",
    );
    let mark = run.mark();
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("an edited but undeclared conflict still captures as unresolved");
    assert_eq!(
        capture.unresolved,
        vec!["c.txt".to_owned()],
        "a file rewritten without its markers is still an unmerged index entry until the \
         worker declares it resolved: the rule is the index's and the manifest's, not the \
         file's — a `-merge` binary resolved keep-ours is byte-identical to one abandoned"
    );
    assert_eq!(run.count_after(mark, STAGE, HookPhase::Before), 0);

    declare(&dispatched.worktree, "resolved c.txt\n");
    let mark = run.mark();
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("a declared resolution captures");
    assert!(
        capture.unresolved.is_empty(),
        "the engine staged the resolution the worker declared"
    );
    assert_eq!(
        run.count_after(mark, STAGE, HookPhase::After),
        1,
        "one `Object.CandidateStage` execution: the declared `git add` and the `add -A` are \
         children of the same funnel invocation"
    );
    assert_eq!(
        git(&dispatched.worktree, &["ls-files", "--unmerged"]),
        "",
        "and the index holds no unmerged entry any more"
    );
    assert_eq!(
        git(
            &dispatched.worktree,
            &["cat-file", "-p", &format!("{}:c.txt", capture.tree)]
        ),
        "resolved by the worker",
        "the captured tree carries the resolution"
    );
    let names = tree_names(&dispatched.worktree, &capture.tree);
    assert!(
        !names.iter().any(|name| name == RESOLUTION_MANIFEST),
        "the manifest is the engine's protocol file, never part of a candidate: {names:?}"
    );
    assert!(
        dispatched.worktree.join(RESOLUTION_MANIFEST).is_file(),
        "and it stays where the worker wrote it, untracked"
    );
    assert!(process.balances());
    run.replay_twice_equal();
}

#[test]
fn a_declared_deletion_resolves_a_conflict_and_an_undeclared_missing_file_does_not() {
    use crate::topology::events::Materialization;

    let mut run = Run::started("resolved-by-deletion");
    let head = run.fixture.head.clone();
    let conflicting = run.commit_with(&head, "c.txt", "not the side's content\n", "c-other");
    let (dispatched, plan) = repair_at(&mut run, &conflicting);
    assert_eq!(dispatched.materialized, Some(Materialization::Conflict));
    let mut process = Process::new();
    context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("the repair's attempt starts");

    remove_file(&dispatched.worktree.join("c.txt"));
    let mark = run.mark();
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("capture");
    assert_eq!(
        capture.unresolved,
        vec!["c.txt".to_owned()],
        "a conflicted path whose file is gone is still unmerged: nothing has said the \
         absence is the resolution"
    );
    assert_eq!(run.count_after(mark, STAGE, HookPhase::Before), 0);

    declare(&dispatched.worktree, "deleted c.txt\n");
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("capture");
    assert!(
        capture.unresolved.is_empty(),
        "a deletion the worker declared is staged by the engine's `git rm`"
    );
    assert_eq!(git(&dispatched.worktree, &["ls-files", "--unmerged"]), "");
    assert!(
        !tree_names(&dispatched.worktree, &capture.tree)
            .iter()
            .any(|name| name == "c.txt"),
        "the captured tree records the deletion"
    );
    assert!(process.balances());
}

/// PR #249's conformance review, finding 2: a conflict rendered with a
/// legitimate `conflict-marker-size` is a conflict all the same. The worker
/// edits another file and leaves the unmerged path alone, and the capture
/// refuses it before staging anything, whatever the markers look like.
#[test]
fn a_conflict_rendered_with_a_longer_marker_size_is_unresolved_at_capture() {
    use crate::topology::events::Materialization;

    let mut run = Run::started("unresolved-marker-size");
    let head = run.fixture.head.clone();
    let with_attributes = run.commit_with(
        &head,
        ".gitattributes",
        "c.txt conflict-marker-size=8\n",
        "attrs-marker-size",
    );
    let conflicting = run.commit_with(
        &with_attributes,
        "c.txt",
        "not the side's content\n",
        "c-other-marker-size",
    );
    let (dispatched, plan) = repair_at(&mut run, &conflicting);
    assert_eq!(dispatched.materialized, Some(Materialization::Conflict));
    let rendered =
        std::fs::read_to_string(dispatched.worktree.join("c.txt")).expect("the conflicted file");
    assert!(
        rendered.lines().any(|line| line.starts_with("<<<<<<<< ")),
        "the attribute rendered eight-character markers, or this test proves nothing:\n{rendered}"
    );
    let mut process = Process::new();
    context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("the repair's attempt starts");
    agent_edits(&dispatched.worktree);

    let mark = run.mark();
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("the capture reads the conflict rather than failing");
    assert_eq!(
        capture.unresolved,
        vec!["c.txt".to_owned()],
        "the unmerged index entry is what makes the path unresolved, not the width of \
         its markers"
    );
    assert_eq!(
        run.count_after(mark, STAGE, HookPhase::Before),
        0,
        "nothing was staged: the worker's other edit does not carry an unresolved \
         conflict into a candidate"
    );
    assert_eq!(
        capture.tree,
        tree_of(&run, &conflicting),
        "an unresolved capture names the tree the worktree started from"
    );

    write_file(&dispatched.worktree.join("c.txt"), b"resolved\n");
    declare(&dispatched.worktree, "resolved c.txt\n");
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("a declared resolution captures");
    assert!(capture.unresolved.is_empty());
    assert_eq!(
        git(
            &dispatched.worktree,
            &["cat-file", "-p", &format!("{}:c.txt", capture.tree)]
        ),
        "resolved",
        "the captured tree carries the resolution"
    );
    assert!(process.balances());
}

/// PR #249's conformance review, finding 2, the other format: a path whose
/// `-merge` attribute makes Git leave the current side in place with no
/// marker at all. Untouched, it is as unresolved as a text conflict, and an
/// oracle that reads markers would pass it and publish the wrong side. And
/// the resolution a worker most often wants for it — keep the published
/// side — leaves the bytes exactly as abandonment does, which is why the
/// declaration and not the content carries the intent.
#[test]
fn an_untouched_binary_conflict_is_unresolved_at_capture_and_a_declared_keep_ours_is_staged() {
    use crate::topology::events::Materialization;

    let mut run = Run::started("unresolved-binary");
    let head = run.fixture.head.clone();
    let with_attributes =
        run.commit_with(&head, ".gitattributes", "c.bin -merge\n", "attrs-binary");
    let base = run.commit_with(
        &with_attributes,
        "c.bin",
        "\u{0}\u{1}the published side\n",
        "bin-published",
    );
    let source_commit = run.commit_with(
        &head,
        "c.bin",
        "\u{0}\u{1}the candidate side\n",
        "bin-candidate",
    );
    let (dispatched, plan) = repair_from(&mut run, &base, &source_commit);
    assert_eq!(dispatched.materialized, Some(Materialization::Conflict));
    let published = b"\0\x01the published side\n";
    assert_eq!(
        std::fs::read(dispatched.worktree.join("c.bin")).expect("the conflicted file"),
        published,
        "Git leaves the current side in the working tree, with no marker to read"
    );
    let mut process = Process::new();
    context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("the repair's attempt starts");
    agent_edits(&dispatched.worktree);

    let mark = run.mark();
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("the capture reads the conflict rather than failing");
    assert_eq!(
        capture.unresolved,
        vec!["c.bin".to_owned()],
        "an unmerged binary path is unresolved until the worker declares it resolved"
    );
    assert_eq!(run.count_after(mark, STAGE, HookPhase::Before), 0);
    assert_eq!(capture.tree, tree_of(&run, &base));

    // Keep-ours: the worker changes not one byte and declares the path
    // resolved. Abandonment and this resolution are the same bytes on disk;
    // only the declaration tells them apart.
    declare(&dispatched.worktree, "resolved c.bin\n");
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("a declared resolution captures");
    assert!(capture.unresolved.is_empty());
    assert_eq!(
        crate::workspace_manager::fixture::git_out(
            &dispatched.worktree,
            &["cat-file", "-p", &format!("{}:c.bin", capture.tree)]
        )
        .stdout,
        published,
        "the captured tree carries the side the worker chose, which is the published one"
    );
    assert!(process.balances());
}

/// The four conflict shapes the design names at once — default markers,
/// `conflict-marker-size=8`, a `-merge` binary, and a delete/modify — in one
/// worktree: the base changes four files the ancestor had, and the one
/// candidate commit changes three of them differently and deletes the fourth.
fn four_shapes(run: &mut Run) -> (String, String) {
    let head = run.fixture.head.clone();
    let attributes = run.commit_with(
        &head,
        ".gitattributes",
        "c.bin -merge\nm8.txt conflict-marker-size=8\n",
        "four-attrs",
    );
    let mut ancestor = attributes;
    for (file, content) in [
        ("c.txt", "shared\n"),
        ("m8.txt", "shared\n"),
        ("c.bin", "\u{0}\u{1}shared\n"),
        ("d.txt", "shared\n"),
    ] {
        ancestor = run.commit_with(&ancestor, file, content, &format!("four-ancestor-{file}"));
    }
    let mut base = ancestor.clone();
    for (file, content) in [
        ("c.txt", "published\n"),
        ("m8.txt", "published\n"),
        ("c.bin", "\u{0}\u{1}published\n"),
        ("d.txt", "published\n"),
    ] {
        base = run.commit_with(&base, file, content, &format!("four-base-{file}"));
    }
    let source = run.commit_changing(
        &ancestor,
        &[
            ("c.txt", Some("candidate\n")),
            ("m8.txt", Some("candidate\n")),
            ("c.bin", Some("\u{0}\u{1}candidate\n")),
            ("d.txt", None),
        ],
        "four-source",
    );
    (base, source)
}

const FOUR_PATHS: [&str; 4] = ["c.bin", "c.txt", "d.txt", "m8.txt"];

fn four_shape_conflict(run: &mut Run) -> (Dispatched, AttemptPlan) {
    use crate::topology::events::Materialization;

    let (base, source) = four_shapes(run);
    let (dispatched, plan) = repair_from(run, &base, &source);
    assert_eq!(dispatched.materialized, Some(Materialization::Conflict));
    assert_eq!(
        unmerged_paths(&dispatched.worktree),
        FOUR_PATHS.map(str::to_owned).to_vec(),
        "all four shapes conflicted"
    );
    let m8 = std::fs::read_to_string(dispatched.worktree.join("m8.txt")).expect("m8.txt");
    assert!(
        m8.lines().any(|line| line.starts_with("<<<<<<<< ")),
        "m8.txt carries eight-character markers:\n{m8}"
    );
    assert_eq!(
        std::fs::read(dispatched.worktree.join("c.bin")).expect("c.bin"),
        b"\0\x01published\n",
        "the binary is left as the published side, with no marker"
    );
    assert_eq!(
        std::fs::read_to_string(dispatched.worktree.join("d.txt")).expect("d.txt"),
        "published\n",
        "modify/delete leaves the modified side in the working tree"
    );
    (dispatched, plan)
}

#[test]
fn four_conflict_shapes_declared_at_once_are_staged_by_the_engine_into_one_tree() {
    use crate::workspace_manager::RESOLUTION_MANIFEST;

    let mut run = Run::started("four-shapes-declared");
    let (dispatched, plan) = four_shape_conflict(&mut run);
    let mut process = Process::new();
    context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("the repair's attempt starts");

    // The worker: two files rewritten, the binary kept as it is, the deleted
    // side chosen for d.txt — and every one of them declared. File writes and
    // nothing else; its tools cannot even remove d.txt.
    write_file(&dispatched.worktree.join("c.txt"), b"resolved c\n");
    write_file(&dispatched.worktree.join("m8.txt"), b"resolved m8\n");
    declare(
        &dispatched.worktree,
        "# the paths this repair resolved\nresolved c.txt\nresolved m8.txt\nresolved c.bin\ndeleted d.txt\n",
    );

    let mark = run.mark();
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("four declared resolutions capture");
    assert!(capture.unresolved.is_empty(), "{:?}", capture.unresolved);
    assert_eq!(
        run.count_after(mark, STAGE, HookPhase::After),
        1,
        "three `git add`, one `git rm` and the `add -A`: one funnel invocation"
    );
    assert_eq!(unmerged_paths(&dispatched.worktree), Vec::<String>::new());
    assert!(
        !dispatched.worktree.join("d.txt").exists(),
        "the engine's `git rm` removed the file the worker declared deleted"
    );
    let names = tree_names(&dispatched.worktree, &capture.tree);
    assert!(
        names.iter().any(|name| name == "c.txt")
            && names.iter().any(|name| name == "m8.txt")
            && names.iter().any(|name| name == "c.bin")
            && !names.iter().any(|name| name == "d.txt")
            && !names.iter().any(|name| name == RESOLUTION_MANIFEST),
        "the tree carries the three kept paths, not the deleted one and not the manifest: \
         {names:?}"
    );
    for (path, content) in [
        ("c.txt", &b"resolved c\n"[..]),
        ("m8.txt", b"resolved m8\n"),
        ("c.bin", b"\0\x01published\n"),
    ] {
        assert_eq!(
            crate::workspace_manager::fixture::git_out(
                &dispatched.worktree,
                &["cat-file", "-p", &format!("{}:{path}", capture.tree)]
            )
            .stdout,
            content,
            "{path}"
        );
    }
    assert!(process.balances());
    run.replay_twice_equal();
}

#[test]
fn four_conflict_shapes_undeclared_are_all_refused_before_any_gate_and_nothing_is_staged() {
    use crate::ladder::{FailureKind, FailureOrigin};
    use crate::workspace_manager::RESOLUTION_MANIFEST;

    let mut run = Run::started("four-shapes-undeclared");
    let (dispatched, plan) = four_shape_conflict(&mut run);
    let base_tree = git(&dispatched.worktree, &["rev-parse", "HEAD^{tree}"]);
    let mut process = Process::new();
    let started = context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("the repair's attempt starts");

    // A worker that resolves nothing and declares nothing: refused on all four.
    agent_edits(&dispatched.worktree);
    let mark = run.mark();
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("capture");
    assert_eq!(capture.unresolved, FOUR_PATHS.map(str::to_owned).to_vec());
    assert_eq!(run.count_after(mark, STAGE, HookPhase::Before), 0);
    assert_eq!(capture.tree, base_tree);
    let diff = run
        .fixture
        .manager
        .candidate_diff(&dispatched.slot, &capture.parent, &capture.tree)
        .expect("diff");
    let assessed = context!(run, process)
        .assess(
            dispatched.site(),
            &plan,
            &started,
            &capture,
            &diff,
            crate::ir::TaskKind::Fix,
        )
        .expect("assessed");
    let failure = assessed.failure.clone().expect("refused before any gate");
    assert_eq!(failure.kind, FailureKind::AgentError);
    assert_eq!(failure.origin, FailureOrigin::Worker);
    for path in FOUR_PATHS {
        assert!(
            failure.reason.contains(path),
            "the refusal names `{path}`: {}",
            failure.reason
        );
    }
    let review_inputs = run.review_inputs();
    let judgement = context!(run, process)
        .judge(
            dispatched.site(),
            &plan,
            Judging {
                run: &started,
                capture: &capture,
                assessed: &assessed,
            },
            &review_inputs,
            &|pass| crate::review::ReviewInvocations {
                pass: started.identities.review_pass(pass, 0),
                reask: started.identities.review_reask(pass, 0),
            },
        )
        .expect("judged");
    assert!(judgement.gates.is_empty() && judgement.reviews.is_empty());
    assert_eq!(run.runner.ran().len(), 1, "only the worker ran");

    // A worker that resolved three of the four and forgot to declare the
    // fourth: the capture is all or nothing, so the three are not staged and
    // the one undeclared path is what it is told about.
    write_file(&dispatched.worktree.join("c.txt"), b"resolved c\n");
    write_file(&dispatched.worktree.join("m8.txt"), b"resolved m8\n");
    declare(
        &dispatched.worktree,
        "resolved c.txt\nresolved m8.txt\ndeleted d.txt\n",
    );
    let mark = run.mark();
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("capture");
    assert_eq!(
        capture.unresolved,
        vec!["c.bin".to_owned()],
        "only the undeclared path is refused, and it is named"
    );
    assert_eq!(
        run.count_after(mark, STAGE, HookPhase::Before),
        0,
        "an unresolved capture stages nothing, declared paths included"
    );
    assert_eq!(
        unmerged_paths(&dispatched.worktree),
        FOUR_PATHS.map(str::to_owned).to_vec()
    );
    assert!(
        dispatched.worktree.join("d.txt").exists(),
        "and removes nothing: the declared deletion waits for a capture that proceeds"
    );

    // A manifest with a line the grammar does not admit: nothing is staged
    // from it, every unmerged path is refused, and the line is quoted back.
    declare(
        &dispatched.worktree,
        "resolved c.txt\nresolved m8.txt\nfixed c.bin\ndeleted d.txt\n",
    );
    let mark = run.mark();
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("capture");
    assert_eq!(capture.unresolved.len(), 5, "{:?}", capture.unresolved);
    assert_eq!(&capture.unresolved[..4], &FOUR_PATHS.map(str::to_owned)[..]);
    assert!(
        capture.unresolved[4].contains("line 3")
            && capture.unresolved[4].contains(RESOLUTION_MANIFEST)
            && capture.unresolved[4].contains("fixed c.bin"),
        "the manifest's problem is one refused entry naming the line: {}",
        capture.unresolved[4]
    );
    assert_eq!(run.count_after(mark, STAGE, HookPhase::Before), 0);
    assert!(process.balances());
}

/// PR #249's regression review, the P1, in the reviewer's own shape: the
/// production worker command for a Claude Code edit profile is assembled, its
/// generated permissions read back, Copilot's checked beside it — neither
/// admits a git command — and the conflict repair is then completed through
/// the one kind of operation both admit, a file write. No privileged helper
/// stands in for anything the real worker cannot do.
#[test]
fn an_edit_only_worker_completes_a_conflict_repair_through_file_writes_alone() {
    use crate::engine::assembly::{
        ImplementerBinding, WorkerAssembly, WorkerSubject, implementer_profile,
    };
    use crate::topology::events::Materialization;
    use crate::workspace_manager::RESOLUTION_MANIFEST;

    let mut run = Run::started("edit-only-worker");
    let head = run.fixture.head.clone();
    let conflicting = run.commit_with(&head, "c.txt", "conflicting current side\n", "c-current");
    let (dispatched, plan) = repair_at(&mut run, &conflicting);
    assert_eq!(dispatched.materialized, Some(Materialization::Conflict));

    let profile = implementer_profile(ImplementerBinding::of_frozen(&plan.binding), None);
    let gates = vec!["cargo test".to_owned()];
    let registry = run.emitter.fold().registry().expect("registry");
    let entry = registry.get(dispatched.key).expect("the repair's entry");
    let command = WorkerAssembly {
        adapter: &crate::agent::claude::ClaudeCodeAdapter,
        profile: &profile,
        task: WorkerSubject::of_frozen(&entry.spec),
        gate_cmds: &gates,
        paths: &run.paths,
        stem: "edit-only-worker",
        attempt: 1,
        retry: None,
        workspace: &dispatched.worktree,
        resume_session: None,
    }
    .command()
    .expect("the production worker command and its permission file");
    assert!(
        command
            .args
            .windows(2)
            .any(|args| args == ["--permission-mode", "dontAsk"])
    );
    let settings_path = command
        .args
        .windows(2)
        .find(|args| args[0] == "--settings")
        .expect("the settings flag")[1]
        .clone();
    let settings: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&settings_path).expect("the materialized settings"))
            .expect("settings JSON");
    let allow: Vec<&str> = settings["permissions"]["allow"]
        .as_array()
        .expect("allow list")
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect();
    assert_eq!(
        allow,
        vec![
            "Read",
            "Glob",
            "Grep",
            "Edit",
            "Write",
            "NotebookEdit",
            "Bash(cargo test)"
        ],
        "file tools and the gate, nothing else: no rule admits a git command"
    );
    assert!(allow.iter().all(|rule| !rule.contains("git")), "{allow:?}");
    let denied: Vec<&str> = settings["permissions"]["deny"]
        .as_array()
        .expect("deny list")
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect();
    assert!(
        !RESOLUTION_MANIFEST.contains('/')
            && denied.iter().all(|rule| {
                // `Write(.upstroke/**)` and its siblings deny directories; the
                // manifest is a root-level file under none of them.
                let prefix = rule
                    .trim_start_matches("Write(")
                    .trim_start_matches("Edit(")
                    .trim_start_matches("Read(")
                    .trim_start_matches("**/")
                    .trim_end_matches(')')
                    .trim_end_matches("**");
                prefix.is_empty() || !RESOLUTION_MANIFEST.starts_with(prefix)
            }),
        "the worker can write the manifest under the production deny list {denied:?}"
    );
    assert_eq!(
        crate::agent::copilot::permission_args(&profile, &gates),
        vec!["--allow-tool=write", "--allow-tool=shell(cargo test)"],
        "Copilot's grant is the same shape: writes and the gate"
    );
    let prompt = String::from_utf8(command.stdin).expect("the prompt");
    assert!(
        prompt.contains("any other shell command is denied")
            && prompt.contains("NEVER run git commit, branch, merge, push, or reset")
    );

    // What such a worker can do, and all it needs to do.
    let mut process = Process::new();
    context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("the attempt starts");
    write_file(
        &dispatched.worktree.join("c.txt"),
        b"resolved by permitted file edit\n",
    );
    declare(&dispatched.worktree, "resolved c.txt\n");
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("capture");
    assert!(
        capture.unresolved.is_empty(),
        "a worker with the generated allow list completed the repair through file writes, \
         and capture refused {:?}",
        capture.unresolved
    );
    assert_eq!(
        git(&dispatched.worktree, &["show", ":c.txt"]),
        "resolved by permitted file edit"
    );
    assert!(process.balances());
}

/// The index spells a path; the manifest may spell it a little differently,
/// and the staging command must name it exactly.
#[test]
fn a_declared_path_is_staged_literally_whatever_characters_it_holds() {
    use crate::topology::events::Materialization;

    let mut run = Run::started("literal-paths");
    let head = run.fixture.head.clone();
    let ancestor = run.commit_with(&head, "a[1].txt", "shared\n", "glob-ancestor");
    let ancestor = run.commit_with(&ancestor, "sp ace.txt", "shared\n", "space-ancestor");
    let base = run.commit_with(&ancestor, "a[1].txt", "published\n", "glob-base");
    let base = run.commit_with(&base, "sp ace.txt", "published\n", "space-base");
    let source = run.commit_changing(
        &ancestor,
        &[
            ("a[1].txt", Some("candidate\n")),
            ("sp ace.txt", Some("candidate\n")),
        ],
        "literal-source",
    );
    let (dispatched, plan) = repair_from(&mut run, &base, &source);
    assert_eq!(dispatched.materialized, Some(Materialization::Conflict));
    assert_eq!(
        unmerged_paths(&dispatched.worktree),
        vec!["a[1].txt".to_owned(), "sp ace.txt".to_owned()]
    );
    let mut process = Process::new();
    context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("the attempt starts");

    write_file(&dispatched.worktree.join("a[1].txt"), b"resolved glob\n");
    // Quoted, `./`-prefixed, a Windows worker's CRLF: the grammar's tolerances.
    declare(
        &dispatched.worktree,
        "resolved `./a[1].txt`\r\ndeleted \"sp ace.txt\"\r\n",
    );
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("capture");
    assert!(capture.unresolved.is_empty(), "{:?}", capture.unresolved);
    assert_eq!(unmerged_paths(&dispatched.worktree), Vec::<String>::new());
    assert_eq!(
        git(&dispatched.worktree, &["show", ":a[1].txt"]),
        "resolved glob",
        "`a[1].txt` was staged as itself, not as the glob `a[1].txt` would be"
    );
    assert!(
        !tree_names(&dispatched.worktree, &capture.tree)
            .iter()
            .any(|name| name == "sp ace.txt")
    );
    assert!(process.balances());
}

#[test]
fn a_resolution_manifest_written_where_nothing_conflicted_is_ignored_and_stays_out_of_the_candidate()
 {
    use crate::workspace_manager::{RESOLUTION_MANIFEST, ResolutionManifest};

    let mut run = Run::started("manifest-without-conflict");
    let dispatched = run.dispatch(ALPHA, 0);
    let plan = run.attempt_plan(ALPHA, 1);
    let mut process = Process::new();
    context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("the attempt starts");
    agent_edits(&dispatched.worktree);
    declare(&dispatched.worktree, "resolved worked.txt\n");
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("capture");
    assert!(capture.unresolved.is_empty());
    let names = tree_names(&dispatched.worktree, &capture.tree);
    assert!(
        names.iter().any(|name| name == WORKED_PATH)
            && !names.iter().any(|name| name == RESOLUTION_MANIFEST),
        "the work is captured and the manifest is not: {names:?}"
    );

    // The same with a manifest the grammar refuses: the index holds nothing
    // the manifest governs, so it is not read, and what it says — well-formed
    // or not — has no effect. "A malformed manifest stages nothing" is a
    // promise about a capture that reads it (PR #249's third-round
    // manifest-contract review, finding 6).
    declare(&dispatched.worktree, "fixed worked.txt\n");
    assert!(
        matches!(
            run.fixture
                .manager
                .resolution_manifest(&dispatched.slot)
                .expect("the manifest reads"),
            ResolutionManifest::Malformed { .. }
        ),
        "read on its own, the file is malformed"
    );
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("an ordinary capture does not read the manifest");
    assert!(capture.unresolved.is_empty());
    let names = tree_names(&dispatched.worktree, &capture.tree);
    assert!(
        names.iter().any(|name| name == WORKED_PATH)
            && !names.iter().any(|name| name == RESOLUTION_MANIFEST),
        "{names:?}"
    );
    assert!(process.balances());
}

#[test]
fn the_resolution_manifest_grammar_reads_what_a_worker_writes_and_refuses_the_rest() {
    use crate::workspace_manager::{
        Declaration, DeclaredResolution, ResolutionKind, ResolutionManifest,
    };

    let declared = |text: &str| match ResolutionManifest::parse(text) {
        ResolutionManifest::Declared(declared) => declared,
        other => panic!("{text:?} parsed as {other:?}"),
    };
    let entry = |path: &str, kind| Declaration {
        path: path.to_owned(),
        kind,
    };
    assert_eq!(
        declared("resolved c.txt\n"),
        vec![entry("c.txt", ResolutionKind::Resolved)]
    );
    assert_eq!(
        declared("# comment\n\nresolved c.txt\r\ndeleted d.txt\r\n"),
        vec![
            entry("c.txt", ResolutionKind::Resolved),
            entry("d.txt", ResolutionKind::Deleted),
        ],
        "CRLF, comments and blank lines"
    );
    assert_eq!(
        declared("- resolved: `c.txt`\n* deleted \"sp ace.txt\"\n  resolved ./dir/e.txt  \n"),
        vec![
            entry("c.txt", ResolutionKind::Resolved),
            entry("sp ace.txt", ResolutionKind::Deleted),
            entry("dir/e.txt", ResolutionKind::Resolved),
        ],
        "bullets, a colon, quotes, `./` and surrounding whitespace"
    );
    assert_eq!(
        declared("resolved \" c.txt \"\ndeleted `\t d.txt`\nresolved \"./e.txt\"\n"),
        vec![
            entry(" c.txt ", ResolutionKind::Resolved),
            entry("\t d.txt", ResolutionKind::Deleted),
            entry("e.txt", ResolutionKind::Resolved),
        ],
        "what is inside the quotes is the path, whitespace included; `./` still comes off"
    );
    assert_eq!(
        declared(""),
        Vec::<Declaration>::new(),
        "an empty file declares nothing"
    );
    for (text, line) in [
        ("fixed c.txt\n", 1),
        ("resolved c.txt\nresolved\n", 2),
        ("resolved c.txt\n\nc.txt\n", 3),
        ("resolved ``\n", 1),
        ("resolved::: c.txt\n", 1),
        ("resolved:: c.txt\n", 1),
        ("resolved c.txt\ndeleted:  \n", 2),
    ] {
        let ResolutionManifest::Malformed { detail } = ResolutionManifest::parse(text) else {
            panic!("{text:?} parsed");
        };
        assert!(
            detail.contains(&format!("line {line} of")),
            "{text:?}: {detail}"
        );
    }

    let names =
        |declared: &str, index: &str| entry(declared, ResolutionKind::Resolved).names(index);
    assert!(names("dir/f.txt", "dir/f.txt"));
    assert!(!names("dir/g.txt", "dir/f.txt"));
    assert!(!names("f.txt", "dir/f.txt"));
    assert_eq!(
        names("dir\\f.txt", "dir/f.txt"),
        cfg!(windows),
        "a backslash is a separator exactly where the platform's Git reads it as one"
    );
    let another_case = |declared: &str, index: &str| {
        entry(declared, ResolutionKind::Resolved).names_in_another_case(index)
    };
    assert!(another_case("dir/c.txt", "Dir/C.txt"));
    assert!(another_case("DIR/C.TXT", "Dir/C.txt"));
    assert!(
        !another_case("Dir/C.txt", "Dir/C.txt") && !another_case("./Dir/C.txt/", "Dir/C.txt"),
        "the index's own spelling, however decorated, is not another case of itself"
    );
    assert!(!another_case("dir/d.txt", "Dir/C.txt"));
    assert!(
        !another_case("cafe\u{301}.txt", "caf\u{e9}.txt"),
        "a normalization form is not a case: the boundary `PR249-MANIFEST-NORMALIZATION-ALIAS` records"
    );

    let unmerged = ["c.txt".to_owned(), "d.txt".to_owned()];
    assert_eq!(
        plan_resolutions(&unmerged, &[], &ResolutionManifest::Absent),
        ResolutionPlan {
            staged: Vec::new(),
            refused: unmerged.to_vec(),
        },
        "no manifest: every unmerged entry is refused"
    );
    assert_eq!(
        plan_resolutions(
            &unmerged,
            &[],
            &ResolutionManifest::Declared(vec![
                entry("c.txt", ResolutionKind::Resolved),
                entry("d.txt", ResolutionKind::Deleted),
                entry("unrelated.txt", ResolutionKind::Deleted),
            ])
        ),
        ResolutionPlan {
            staged: vec![
                DeclaredResolution {
                    path: "c.txt".to_owned(),
                    kind: ResolutionKind::Resolved,
                },
                DeclaredResolution {
                    path: "d.txt".to_owned(),
                    kind: ResolutionKind::Deleted,
                },
            ],
            refused: Vec::new(),
        },
        "each unmerged entry takes the kind declared for it; a declaration of a path that \
         is not unmerged is nothing"
    );
    let plan = plan_resolutions(
        &unmerged,
        &[],
        &ResolutionManifest::Declared(vec![
            entry("c.txt", ResolutionKind::Resolved),
            entry("c.txt", ResolutionKind::Deleted),
        ]),
    );
    assert!(plan.staged.is_empty());
    assert_eq!(plan.refused.len(), 2);
    assert!(
        plan.refused[0].starts_with("c.txt (declared both"),
        "a path declared both ways is refused, not guessed: {:?}",
        plan.refused
    );
    assert_eq!(plan.refused[1], "d.txt");
    let plan = plan_resolutions(
        &unmerged,
        &[],
        &ResolutionManifest::Malformed {
            detail: "line 1 is odd".to_owned(),
        },
    );
    assert!(plan.staged.is_empty());
    assert_eq!(
        plan.refused,
        vec![
            "c.txt".to_owned(),
            "d.txt".to_owned(),
            "(line 1 is odd)".to_owned()
        ],
        "a malformed manifest refuses everything and says why"
    );

    // Another case of a governed path: refused, never matched.
    let upper = ["Dir/C.txt".to_owned()];
    let plan = plan_resolutions(
        &upper,
        &[],
        &ResolutionManifest::Declared(vec![
            entry("Dir/C.txt", ResolutionKind::Resolved),
            entry("dir/c.txt", ResolutionKind::Deleted),
        ]),
    );
    assert!(plan.staged.is_empty());
    assert_eq!(
        plan.refused,
        vec![
            "Dir/C.txt (declared `resolved`, and `deleted` as `dir/c.txt`, a spelling that \
             differs only by case)"
                .to_owned()
        ],
        "a contradiction in two cases is refused on every platform"
    );
    let plan = plan_resolutions(
        &upper,
        &[],
        &ResolutionManifest::Declared(vec![entry("dir/c.txt", ResolutionKind::Deleted)]),
    );
    assert_eq!(
        plan.refused,
        vec![
            "Dir/C.txt (declared `deleted` only as `dir/c.txt`, which differs from the \
             index's spelling by case alone; spell the path as the index does)"
                .to_owned()
        ],
        "a lone declaration in another case is refused naming the index's spelling"
    );
    let both_spellings = ["Dir/C.txt".to_owned(), "dir/c.txt".to_owned()];
    let plan = plan_resolutions(
        &both_spellings,
        &[],
        &ResolutionManifest::Declared(vec![
            entry("Dir/C.txt", ResolutionKind::Resolved),
            entry("dir/c.txt", ResolutionKind::Deleted),
        ]),
    );
    assert!(
        plan.refused.is_empty() && plan.staged.len() == 2,
        "two index entries that differ only by case are two files, and each takes its own \
         declaration: {plan:?}"
    );
    let plan = plan_resolutions(
        &["caf\u{e9}.txt".to_owned()],
        &[],
        &ResolutionManifest::Declared(vec![
            entry("caf\u{e9}.txt", ResolutionKind::Resolved),
            entry("cafe\u{301}.txt", ResolutionKind::Deleted),
        ]),
    );
    assert!(
        plan.refused.is_empty() && plan.staged.len() == 1,
        "a pair that differs only in normalization form is not read as a contradiction — \
         the recorded boundary, `PR249-MANIFEST-NORMALIZATION-ALIAS`: {plan:?}"
    );

    // What a previous capture of the generation resolved: governed while the
    // index holds it, revisable, and left alone when undeclared.
    let resolved = ["c.txt".to_owned()];
    assert_eq!(
        plan_resolutions(&[], &resolved, &ResolutionManifest::Absent),
        ResolutionPlan::default(),
        "a resolved path with no manifest stays as the previous capture left it"
    );
    assert_eq!(
        plan_resolutions(
            &[],
            &resolved,
            &ResolutionManifest::Declared(vec![entry("c.txt", ResolutionKind::Deleted)])
        ),
        ResolutionPlan {
            staged: vec![DeclaredResolution {
                path: "c.txt".to_owned(),
                kind: ResolutionKind::Deleted,
            }],
            refused: Vec::new(),
        },
        "a retained retry revises the resolution to a deletion"
    );
    let plan = plan_resolutions(
        &[],
        &resolved,
        &ResolutionManifest::Declared(vec![
            entry("c.txt", ResolutionKind::Resolved),
            entry("c.txt", ResolutionKind::Deleted),
        ]),
    );
    assert!(
        plan.staged.is_empty() && plan.refused[0].starts_with("c.txt (declared both"),
        "{plan:?}"
    );
    let plan = plan_resolutions(
        &[],
        &resolved,
        &ResolutionManifest::Malformed {
            detail: "line 1 is odd".to_owned(),
        },
    );
    assert_eq!(
        plan.refused,
        vec!["c.txt".to_owned(), "(line 1 is odd)".to_owned()],
        "a malformed manifest refuses what it governs, resolved paths included"
    );
    assert_eq!(
        plan_resolutions(&resolved, &resolved, &ResolutionManifest::Absent).refused,
        vec!["c.txt".to_owned()],
        "a path in both lists is governed once, as unmerged"
    );
}

#[test]
fn an_already_present_source_proceeds_as_an_ordinary_attempt_whose_empty_diff_fails_honestly() {
    use crate::ladder::FailureKind;
    use crate::topology::events::Materialization;

    let mut run = Run::started("already-present-source");
    let side = run.fixture.side.clone();
    let (dispatched, plan) = repair_at(&mut run, &side);
    assert_eq!(
        dispatched.materialized,
        Some(Materialization::Empty),
        "the candidate's change is already present on this base"
    );
    let mut process = Process::new();
    let started = context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("`repairs.empty_source`: an Empty observation proceeds as an ordinary attempt");
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("capture");
    assert!(capture.unresolved.is_empty());
    assert_eq!(
        capture.tree,
        tree_of(&run, &side),
        "the worker changed nothing, so the captured tree is the base's"
    );
    let diff = run
        .fixture
        .manager
        .candidate_diff(&dispatched.slot, &capture.parent, &capture.tree)
        .expect("diff");
    assert!(diff.trim().is_empty(), "there is nothing to judge: {diff}");
    let assessed = context!(run, process)
        .assess(
            dispatched.site(),
            &plan,
            &started,
            &capture,
            &diff,
            crate::ir::TaskKind::Fix,
        )
        .expect("assessed");
    assert_eq!(
        assessed.failure.as_ref().map(|failure| failure.kind),
        Some(FailureKind::EmptyDiff),
        "an empty diff fails under the existing rule; no special no-candidate settlement exists \
         (it is a deferred decision)"
    );
    assert!(process.balances());
}

/// A retained retry of a repair generation, reserved, verified against
/// `retained` and started as attempt `attempt`, the way `run::retry_ready`
/// does it; the reservation is returned so the caller can keep it alive.
fn retry_in_place(
    run: &mut Run,
    process: &mut Process,
    dispatched: &Dispatched,
    retained: &str,
    attempt: u32,
) -> Reservations {
    use crate::topology::events::Materialization;

    run.retain(dispatched.key, GENERATION, attempt - 1);
    let mut reservations = Reservations::new();
    let mut plan = run.attempt_plan(dispatched.key, attempt);
    let outcome = settle::retry(
        run.emitter.fold(),
        &mut reservations,
        &ManagedWorktrees::new(&run.fixture.manager),
        run.hooks.effects(),
        &RetryRequest {
            key: dispatched.key,
            slot: dispatched.slot.clone(),
            retained_tree: retained.to_owned(),
            binding: plan.binding.clone(),
            rung: plan.rung,
            pool: plan.pool.clone(),
            materialization: Some(Materialization::Retained),
        },
    )
    .expect("the retry decision reads");
    let RetryOutcome::Start(authorized) = outcome else {
        panic!("a retained repair generation retries in place");
    };
    plan.attempt = authorized.attempt;
    plan.resume_session = authorized.resume_session.clone();
    plan.materialization_observed = authorized.materialization_observed;
    context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("the retained retry starts");
    reservations
        .convert(dispatched.key, ReservationKind::Retry)
        .expect("the retry's reservation converts");
    reservations
}

/// The bytes of `path` in `tree`.
fn blob_in(worktree: &Path, tree: &str, path: &str) -> Vec<u8> {
    crate::workspace_manager::fixture::git_out(
        worktree,
        &["cat-file", "-p", &format!("{tree}:{path}")],
    )
    .stdout
}

#[test]
fn a_retained_retry_revises_a_declared_resolution_to_a_deletion_and_a_settled_deletion_is_not_reapplied()
 {
    use crate::workspace_manager::RESOLUTION_MANIFEST;

    let mut run = Run::started("retained-revision");
    let head = run.fixture.head.clone();
    let conflicting = run.commit_with(&head, "c.txt", "other side\n", "revision-other");
    let (dispatched, plan) = repair_at(&mut run, &conflicting);
    let mut process = Process::new();
    context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("the repair's first attempt starts");
    assert_eq!(
        run.fixture
            .manager
            .resolved_conflicts(&dispatched.slot)
            .expect("the resolve-undo read"),
        Vec::<String>::new(),
        "a fresh materialization has resolved nothing"
    );
    write_file(&dispatched.worktree.join("c.txt"), b"first resolution\n");
    declare(&dispatched.worktree, "resolved c.txt\n");
    let first = context!(run, process)
        .capture(dispatched.site())
        .expect("the first capture stages the declared resolution");
    assert!(first.unresolved.is_empty());
    assert_eq!(
        run.fixture
            .manager
            .resolved_conflicts(&dispatched.slot)
            .expect("the resolve-undo read"),
        vec!["c.txt".to_owned()],
        "the index records what the capture resolved from unmerged stages \
         (`git ls-files --resolve-undo`), and the read finds it"
    );

    // Attempt 2, in the retained worktree: nothing is unmerged any more, and
    // the worker corrects itself to a deletion its tools cannot perform.
    // PR #249's third-round regression review found this declaration went
    // unread, the tree kept `c.txt` and the capture reported success.
    let _second_attempt = retry_in_place(&mut run, &mut process, &dispatched, &first.tree, 2);
    declare(&dispatched.worktree, "deleted c.txt\n");
    let mark = run.mark();
    let second = context!(run, process)
        .capture(dispatched.site())
        .expect("the retry's capture");
    assert!(second.unresolved.is_empty(), "{:?}", second.unresolved);
    assert_eq!(run.count_after(mark, STAGE, HookPhase::After), 1);
    let names = tree_names(&dispatched.worktree, &second.tree);
    assert!(
        !names.iter().any(|name| name == "c.txt")
            && !names.iter().any(|name| name == RESOLUTION_MANIFEST)
            && names.iter().any(|name| name == "a.txt"),
        "the revised resolution is a deletion, applied by the engine: {names:?}"
    );
    assert!(
        !dispatched.worktree.join("c.txt").exists(),
        "the engine's `git rm --force` removed the file whose staged content `rm` would \
         otherwise have refused to touch"
    );
    assert_ne!(first.tree, second.tree);
    assert_eq!(
        run.fixture
            .manager
            .resolved_conflicts(&dispatched.slot)
            .expect("the resolve-undo read"),
        Vec::<String>::new(),
        "a path resolved by deletion has no index entry left for a declaration to govern"
    );

    // Attempt 3, the manifest left as it was: the deletion stands, the
    // declaration governs nothing, and no `git rm` runs against a pathspec
    // that matches nothing. The worker's other edit is captured as usual.
    let _third_attempt = retry_in_place(&mut run, &mut process, &dispatched, &second.tree, 3);
    agent_edits(&dispatched.worktree);
    let third = context!(run, process)
        .capture(dispatched.site())
        .expect("a settled deletion is not re-applied");
    assert!(third.unresolved.is_empty());
    let names = tree_names(&dispatched.worktree, &third.tree);
    assert!(
        names.iter().any(|name| name == WORKED_PATH) && !names.iter().any(|name| name == "c.txt"),
        "{names:?}"
    );
    assert!(process.balances());
    run.replay_twice_equal();
}

#[test]
fn a_retained_retry_reads_the_manifest_while_the_index_holds_what_a_capture_resolved() {
    let mut run = Run::started("retained-governed");
    let head = run.fixture.head.clone();
    let conflicting = run.commit_with(&head, "c.txt", "other side\n", "governed-other");
    let (dispatched, plan) = repair_at(&mut run, &conflicting);
    let mut process = Process::new();
    context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("the repair's first attempt starts");
    write_file(&dispatched.worktree.join("c.txt"), b"first resolution\n");
    declare(&dispatched.worktree, "resolved c.txt\n");
    let first = context!(run, process)
        .capture(dispatched.site())
        .expect("the first capture");
    assert!(first.unresolved.is_empty());

    // A manifest the grammar refuses, in the retry: the index still holds the
    // resolved entry, so the manifest is read and the capture refuses,
    // staging nothing — the malformed-manifest promise, in the one further
    // state it holds in (PR #249's third-round manifest-contract review,
    // finding 6, asked for the condition to be stated).
    let _second_attempt = retry_in_place(&mut run, &mut process, &dispatched, &first.tree, 2);
    write_file(&dispatched.worktree.join("c.txt"), b"second resolution\n");
    declare(&dispatched.worktree, "fixed c.txt\n");
    let mark = run.mark();
    let refused = context!(run, process)
        .capture(dispatched.site())
        .expect("a refusal is a capture, not an error");
    assert_eq!(refused.unresolved.len(), 2, "{:?}", refused.unresolved);
    assert_eq!(refused.unresolved[0], "c.txt");
    assert!(
        refused.unresolved[1].contains("line 1 of"),
        "{:?}",
        refused.unresolved
    );
    assert_eq!(run.count_after(mark, STAGE, HookPhase::After), 0);
    assert_eq!(refused.tree, tree_of(&run, &conflicting));
    assert_eq!(
        git(&dispatched.worktree, &["show", ":c.txt"]),
        "first resolution",
        "nothing was staged"
    );

    // The worker corrects the manifest: the resolution is re-staged with the
    // content the file now holds.
    declare(&dispatched.worktree, "resolved c.txt\n");
    let revised = context!(run, process)
        .capture(dispatched.site())
        .expect("the corrected manifest captures");
    assert!(revised.unresolved.is_empty());
    assert_eq!(
        git(&dispatched.worktree, &["show", ":c.txt"]),
        "second resolution"
    );

    // The manifest removed: nothing governs the entry any differently, and
    // an ordinary capture stages what the tree holds.
    remove_file(
        &dispatched
            .worktree
            .join(crate::workspace_manager::RESOLUTION_MANIFEST),
    );
    write_file(&dispatched.worktree.join("c.txt"), b"third resolution\n");
    let plain = context!(run, process)
        .capture(dispatched.site())
        .expect("no manifest, nothing unmerged: an ordinary capture");
    assert!(plain.unresolved.is_empty());
    assert_eq!(
        blob_in(&dispatched.worktree, &plain.tree, "c.txt"),
        b"third resolution\n"
    );
    assert!(process.balances());
    run.replay_twice_equal();
}

#[test]
fn a_tracked_file_of_the_manifests_name_is_the_repositorys_and_a_conflict_repair_there_is_refused()
{
    use crate::workspace_manager::RESOLUTION_MANIFEST;

    // An ordinary attempt in a repository that tracks the name: the worker's
    // edit to that file is captured like any other. The second round's
    // unconditional exclusion kept the file's old content in the tree with no
    // refusal (PR #249's third-round regression review, finding 1).
    let mut run = Run::started("tracked-name-ordinary");
    let head = run.fixture.head.clone();
    let tracking = run.commit_with(
        &head,
        RESOLUTION_MANIFEST,
        "old application data\n",
        "tracks-the-name",
    );
    run.fixture.head = tracking;
    let dispatched = run.dispatch(ALPHA, 0);
    let plan = run.attempt_plan(ALPHA, 1);
    let mut process = Process::new();
    context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("the attempt starts");
    write_file(
        &dispatched.worktree.join(RESOLUTION_MANIFEST),
        b"new application data\n",
    );
    agent_edits(&dispatched.worktree);
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("an ordinary capture");
    assert!(capture.unresolved.is_empty());
    assert_eq!(
        blob_in(&dispatched.worktree, &capture.tree, RESOLUTION_MANIFEST),
        b"new application data\n",
        "the repository's file is captured with the worker's edit"
    );
    assert!(
        tree_names(&dispatched.worktree, &capture.tree)
            .iter()
            .any(|name| name == WORKED_PATH)
    );
    assert!(process.balances());

    // A conflict repair materialized from a candidate that carries a file of
    // the name (the third-round record review's witness): the pick puts it in
    // the index, and the capture that needs the manifest refuses before it
    // stages anything, naming the collision — the repository's bytes are not
    // declarations, and a repository that has taken the name cannot run the
    // protocol.
    let mut run = Run::started("tracked-name-repair");
    let head = run.fixture.head.clone();
    let conflicting = run.commit_with(&head, "c.txt", "published\n", "tracked-published");
    let source = run.commit_changing(
        &head,
        &[
            ("c.txt", Some("candidate\n")),
            (RESOLUTION_MANIFEST, Some("the candidate's own file\n")),
        ],
        "tracked-candidate",
    );
    let (dispatched, plan) = repair_from(&mut run, &conflicting, &source);
    assert_eq!(unmerged_paths(&dispatched.worktree), ["c.txt"]);
    assert_eq!(
        git(
            &dispatched.worktree,
            &["ls-files", "--", RESOLUTION_MANIFEST]
        ),
        RESOLUTION_MANIFEST,
        "the pick placed the candidate's file in the index"
    );
    let mut process = Process::new();
    context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("the repair's attempt starts");
    write_file(&dispatched.worktree.join("c.txt"), b"resolved\n");
    declare(&dispatched.worktree, "resolved c.txt\n");
    let mark = run.mark();
    let refusal = context!(run, process)
        .capture(dispatched.site())
        .expect_err("a conflict repair cannot be declared where the name is tracked");
    assert!(
        matches!(&refusal, UpstrokeError::Refused { message }
            if message.contains("tracks `.upstroke-resolved`")
                && message.contains("cannot be declared")),
        "{refusal}"
    );
    assert_eq!(run.count_after(mark, STAGE, HookPhase::After), 0);
    assert_eq!(
        unmerged_paths(&dispatched.worktree),
        ["c.txt"],
        "nothing was staged"
    );
    assert!(process.balances());

    // The same candidate picked cleanly: no entry for the manifest to govern,
    // so it is not read, and the candidate's file is captured as the data it
    // is — "the manifest is never part of a candidate" is a statement about
    // the worker's file, not about the name.
    let mut run = Run::started("tracked-name-clean");
    let head = run.fixture.head.clone();
    let source = run.commit_changing(
        &head,
        &[
            ("c.txt", Some("candidate\n")),
            (RESOLUTION_MANIFEST, Some("the candidate's own file\n")),
        ],
        "tracked-clean-candidate",
    );
    let (dispatched, plan) = repair_from(&mut run, &head, &source);
    assert_eq!(unmerged_paths(&dispatched.worktree), Vec::<String>::new());
    let mut process = Process::new();
    context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("the repair's attempt starts");
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("a clean pick captures like an ordinary attempt");
    assert!(capture.unresolved.is_empty());
    assert_eq!(
        blob_in(&dispatched.worktree, &capture.tree, RESOLUTION_MANIFEST),
        b"the candidate's own file\n"
    );
    assert!(process.balances());
}

#[test]
fn an_ignored_manifest_is_read_and_kept_out_of_the_candidate_by_the_ignore_rules_alone() {
    use crate::workspace_manager::RESOLUTION_MANIFEST;

    // The repository ignores the engine's bookkeeping file — a reasonable
    // thing to do — and a required clean filter is declared for it too, so
    // that any `add` reaching the file fails loudly. The second round's
    // exclusion, exactly naming an ignored path, made `git add -A` exit 1
    // (PR #249's third-round manifest-contract review, finding 1).
    let mut run = Run::started("ignored-manifest");
    let head = run.fixture.head.clone();
    let ignoring = run.commit_with(
        &head,
        ".gitignore",
        &format!("{RESOLUTION_MANIFEST}\n"),
        "ignores-the-name",
    );
    let conflicting = run.commit_with(&ignoring, "c.txt", "published\n", "ignored-published");
    let (dispatched, plan) = repair_at(&mut run, &conflicting);
    assert_eq!(unmerged_paths(&dispatched.worktree), ["c.txt"]);
    let mut process = Process::new();
    context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("the repair's attempt starts");
    write_file(
        &dispatched.worktree.join(".gitattributes"),
        format!("{RESOLUTION_MANIFEST} filter=manifest_probe\n").as_bytes(),
    );
    git(
        &dispatched.worktree,
        &["config", "filter.manifest_probe.clean", "false"],
    );
    git(
        &dispatched.worktree,
        &["config", "filter.manifest_probe.required", "true"],
    );
    write_file(&dispatched.worktree.join("c.txt"), b"resolved\n");
    declare(&dispatched.worktree, "resolved c.txt\n");
    assert_eq!(
        git(
            &dispatched.worktree,
            &["check-ignore", "--", RESOLUTION_MANIFEST]
        ),
        RESOLUTION_MANIFEST,
        "the manifest is ignored"
    );
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("an ignored manifest is read, and nothing tries to add it");
    assert!(capture.unresolved.is_empty(), "{:?}", capture.unresolved);
    let names = tree_names(&dispatched.worktree, &capture.tree);
    assert!(
        names.iter().any(|name| name == "c.txt")
            && names.iter().any(|name| name == ".gitattributes")
            && !names.iter().any(|name| name == RESOLUTION_MANIFEST),
        "{names:?}"
    );
    assert!(dispatched.worktree.join(RESOLUTION_MANIFEST).is_file());
    assert!(process.balances());
}

#[test]
fn a_directory_of_the_manifests_name_is_the_repositorys_and_its_contents_are_captured() {
    use crate::workspace_manager::RESOLUTION_MANIFEST;

    let data = format!("{RESOLUTION_MANIFEST}/data.txt");

    // Untracked: the directory's contents are new files of the candidate.
    let mut run = Run::started("directory-untracked");
    let dispatched = run.dispatch(ALPHA, 0);
    let plan = run.attempt_plan(ALPHA, 1);
    let mut process = Process::new();
    context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("the attempt starts");
    agent_edits(&dispatched.worktree);
    write_file(
        &dispatched
            .worktree
            .join(RESOLUTION_MANIFEST)
            .join("data.txt"),
        b"ordinary project data\n",
    );
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("an ordinary capture");
    let names = tree_names(&dispatched.worktree, &capture.tree);
    assert!(
        names.iter().any(|name| name == WORKED_PATH) && names.contains(&data),
        "a directory is not the root-level protocol file, and its contents are captured: \
         {names:?}"
    );
    assert!(process.balances());

    // Tracked: an edit under it is captured. The second round's pathspec
    // excluded every descendant of the name and kept the old content
    // (PR #249's third-round manifest-contract review, finding 2).
    let mut run = Run::started("directory-tracked");
    let head = run.fixture.head.clone();
    let tracking = run.commit_with(&head, &data, "old project data\n", "tracks-a-directory");
    run.fixture.head = tracking;
    let dispatched = run.dispatch(ALPHA, 0);
    let plan = run.attempt_plan(ALPHA, 1);
    let mut process = Process::new();
    context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("the attempt starts");
    write_file(
        &dispatched
            .worktree
            .join(RESOLUTION_MANIFEST)
            .join("data.txt"),
        b"new project data\n",
    );
    agent_edits(&dispatched.worktree);
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("an ordinary capture");
    assert_eq!(
        blob_in(&dispatched.worktree, &capture.tree, &data),
        b"new project data\n"
    );
    assert!(process.balances());

    // With a conflict to declare: the name is taken by a directory, the
    // worker cannot write the manifest there, and the capture refuses naming
    // what holds the name rather than failing on an I/O error.
    let mut run = Run::started("directory-conflict");
    let head = run.fixture.head.clone();
    let tracking = run.commit_with(&head, &data, "project data\n", "tracks-a-directory-too");
    let conflicting = run.commit_with(&tracking, "c.txt", "published\n", "directory-published");
    let (dispatched, plan) = repair_at(&mut run, &conflicting);
    assert_eq!(unmerged_paths(&dispatched.worktree), ["c.txt"]);
    let mut process = Process::new();
    context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("the repair's attempt starts");
    write_file(&dispatched.worktree.join("c.txt"), b"resolved\n");
    let mark = run.mark();
    let refusal = context!(run, process)
        .capture(dispatched.site())
        .expect_err("the name is taken");
    assert!(
        matches!(&refusal, UpstrokeError::Refused { message }
            if message.contains("is a directory") && message.contains("cannot be declared")),
        "{refusal}"
    );
    assert_eq!(run.count_after(mark, STAGE, HookPhase::After), 0);
    assert!(process.balances());
}

#[test]
fn a_declaration_in_another_case_is_refused_and_the_checkout_says_whether_it_named_the_file() {
    // `Dir/C.txt` conflicted. On a case-insensitive filesystem `dir/c.txt`
    // is that file; on a case-sensitive one it is nothing. The rule is the
    // same on both: the pair is a contradiction, the lone respelling names
    // nothing the index spells that way, and each is refused with the
    // index's spelling (PR #249's third-round manifest-contract review,
    // finding 3, whose filesystem half was reasoned; this test establishes it
    // on each platform the suite runs on).
    let mut run = Run::started("case-alias");
    let head = run.fixture.head.clone();
    let ancestor = run.commit_with(&head, "Dir/C.txt", "shared\n", "case-shared");
    let base = run.commit_with(&ancestor, "Dir/C.txt", "published\n", "case-published");
    let source = run.commit_with(&ancestor, "Dir/C.txt", "candidate\n", "case-candidate");
    let (dispatched, plan) = repair_from(&mut run, &base, &source);
    assert_eq!(unmerged_paths(&dispatched.worktree), ["Dir/C.txt"]);
    let mut process = Process::new();
    context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("the repair's attempt starts");
    let folds_case = dispatched.worktree.join("dir").join("c.txt").is_file();
    assert_eq!(
        folds_case,
        cfg!(any(windows, target_os = "macos")),
        "whether `dir/c.txt` names the conflicted `Dir/C.txt` is the checkout's filesystem's \
         answer, and this records it for the platform the suite runs on: Windows and macOS \
         fold case, Linux does not; a checkout on a volume that answers otherwise is not one \
         CI runs on"
    );
    write_file(
        &dispatched.worktree.join("Dir").join("C.txt"),
        b"resolved\n",
    );

    declare(
        &dispatched.worktree,
        "resolved Dir/C.txt\ndeleted dir/c.txt\n",
    );
    let mark = run.mark();
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("a refusal is a capture");
    assert_eq!(
        capture.unresolved,
        vec![
            "Dir/C.txt (declared `resolved`, and `deleted` as `dir/c.txt`, a spelling that \
             differs only by case)"
                .to_owned()
        ],
        "the pair is refused whether or not the filesystem reads it as one file (here it {})",
        if folds_case { "does" } else { "does not" }
    );
    assert_eq!(run.count_after(mark, STAGE, HookPhase::After), 0);
    assert_eq!(unmerged_paths(&dispatched.worktree), ["Dir/C.txt"]);

    declare(&dispatched.worktree, "resolved dir/c.txt\n");
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("a refusal is a capture");
    assert_eq!(
        capture.unresolved,
        vec![
            "Dir/C.txt (declared `resolved` only as `dir/c.txt`, which differs from the \
             index's spelling by case alone; spell the path as the index does)"
                .to_owned()
        ]
    );
    assert_eq!(run.count_after(mark, STAGE, HookPhase::After), 0);

    declare(&dispatched.worktree, "resolved Dir/C.txt\n");
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("the index's spelling captures");
    assert!(capture.unresolved.is_empty());
    assert_eq!(
        blob_in(&dispatched.worktree, &capture.tree, "Dir/C.txt"),
        b"resolved\n"
    );
    assert!(process.balances());

    // Unicode normalization forms are not folded: the boundary
    // `PR249-MANIFEST-NORMALIZATION-ALIAS` records. `café.txt` (composed)
    // conflicted; the decomposed spelling names that file on a
    // normalization-insensitive volume (macOS) and nothing elsewhere, and the
    // engine reads it as nothing everywhere: the pair is not refused and the
    // composed declaration is staged. This pins the boundary so that folding
    // it moves this test with it.
    let mut run = Run::started("normalization-alias");
    let head = run.fixture.head.clone();
    let composed = "caf\u{e9}.txt";
    let decomposed = "cafe\u{301}.txt";
    let ancestor = run.commit_with(&head, composed, "shared\n", "nfc-shared");
    let base = run.commit_with(&ancestor, composed, "published\n", "nfc-published");
    let source = run.commit_with(&ancestor, composed, "candidate\n", "nfc-candidate");
    let (dispatched, plan) = repair_from(&mut run, &base, &source);
    assert_eq!(
        run.fixture
            .manager
            .unresolved_conflicts(&dispatched.slot)
            .expect("the unmerged read"),
        [composed]
    );
    let mut process = Process::new();
    context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("the repair's attempt starts");
    let folds_normalization = dispatched.worktree.join(decomposed).is_file();
    assert_eq!(
        folds_normalization,
        cfg!(target_os = "macos"),
        "whether the decomposed spelling names the composed file is the checkout's \
         filesystem's answer, recorded for the platform the suite runs on"
    );
    write_file(&dispatched.worktree.join(composed), b"resolved\n");
    declare(
        &dispatched.worktree,
        &format!("resolved {composed}\ndeleted {decomposed}\n"),
    );
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("capture");
    assert!(
        capture.unresolved.is_empty(),
        "the boundary: a normalization-form respelling is not read as an alias, so the \
         pair is not refused (the filesystem here {} read the two as one file): {:?}",
        if folds_normalization {
            "did"
        } else {
            "did not"
        },
        capture.unresolved
    );
    assert_eq!(
        blob_in(&dispatched.worktree, &capture.tree, composed),
        b"resolved\n"
    );
    assert!(process.balances());
}

/// A file whose name begins and ends with a space, which Windows cannot hold
/// under its usual rules; the quoted form of the grammar exists for such names.
#[cfg(unix)]
#[test]
fn a_quoted_declaration_names_an_entry_exactly_whitespace_included() {
    let mut run = Run::started("quoted-whitespace");
    let head = run.fixture.head.clone();
    let spaced = " c.txt ";
    let ancestor = run.commit_with(&head, spaced, "shared\n", "space-shared");
    let base = run.commit_with(&ancestor, spaced, "published\n", "space-published");
    let source = run.commit_with(&ancestor, spaced, "candidate\n", "space-candidate");
    let (dispatched, plan) = repair_from(&mut run, &base, &source);
    assert_eq!(
        run.fixture
            .manager
            .unresolved_conflicts(&dispatched.slot)
            .expect("the unmerged read"),
        [spaced]
    );
    let mut process = Process::new();
    context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("the repair's attempt starts");
    write_file(&dispatched.worktree.join(spaced), b"resolved\n");

    // Unquoted, the whitespace is trimmed away and the entry stays undeclared.
    declare(&dispatched.worktree, "resolved  c.txt \n");
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("a refusal is a capture");
    assert_eq!(capture.unresolved, [spaced]);

    // Quoted, the path is what the quotes hold. The second round's parser
    // trimmed inside the quotes as well, and PR #249's third-round
    // manifest-contract review (finding 4) found the real entry refused.
    declare(&dispatched.worktree, "resolved \" c.txt \"\n");
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("the quoted spelling captures");
    assert!(capture.unresolved.is_empty(), "{:?}", capture.unresolved);
    assert_eq!(
        blob_in(&dispatched.worktree, &capture.tree, spaced),
        b"resolved\n"
    );
    assert!(process.balances());
}

#[test]
fn a_declared_resolution_reaches_the_configured_checks_which_decide_what_they_detect() {
    // The worker declares a file resolved and leaves the markers in it. The
    // capture stages what is declared — the declaration is the worker's
    // signal, and no content read stands in for it — and what happens next
    // is the configured validation's: with no gate and no reviewer, nothing
    // detects the markers and the judgment accepts. `design/26` §26.4 says a
    // wrong declaration *reaches* the ordinary gates and review; it once said
    // it was *caught* by them, and PR #249's third-round record and
    // manifest-contract reviews both executed this shape against that word.
    let mut run = Run::started("wrong-declaration-reaches-judgment");
    let head = run.fixture.head.clone();
    let conflicting = run.commit_with(&head, "c.txt", "published\n", "wrong-published");
    let (dispatched, mut plan) = repair_at(&mut run, &conflicting);
    plan.gates.clear();
    plan.reviewers.clear();
    let mut process = Process::new();
    let started = context!(run, process)
        .start(dispatched.site(), &plan)
        .expect("the repair's attempt starts");
    let bytes = std::fs::read_to_string(dispatched.worktree.join("c.txt")).expect("c.txt");
    assert!(
        bytes.contains("<<<<<<<"),
        "the markers are in place:\n{bytes}"
    );
    declare(&dispatched.worktree, "resolved c.txt\n");
    let capture = context!(run, process)
        .capture(dispatched.site())
        .expect("the declaration is staged, markers and all");
    assert!(capture.unresolved.is_empty());
    assert!(
        String::from_utf8_lossy(&blob_in(&dispatched.worktree, &capture.tree, "c.txt"))
            .contains("<<<<<<<")
    );
    let diff = run
        .fixture
        .manager
        .candidate_diff(&dispatched.slot, &capture.parent, &capture.tree)
        .expect("diff");
    let assessed = context!(run, process)
        .assess(
            dispatched.site(),
            &plan,
            &started,
            &capture,
            &diff,
            crate::ir::TaskKind::Fix,
        )
        .expect("assessed");
    assert!(
        assessed.failure.is_none(),
        "no cheap rung reads the markers: {:?}",
        assessed.failure
    );
    let inputs = run.review_inputs();
    let judgement = context!(run, process)
        .judge(
            dispatched.site(),
            &plan,
            Judging {
                run: &started,
                capture: &capture,
                assessed: &assessed,
            },
            &inputs,
            &|pass| crate::review::ReviewInvocations {
                pass: started.identities.review_pass(pass, 0),
                reask: started.identities.review_reask(pass, 0),
            },
        )
        .expect("judged");
    assert!(
        judgement.accepted(),
        "with no gate and no reviewer configured, the declared resolution is accepted: what \
         a wrong declaration reaches is the configured validation, which decides what it \
         detects"
    );
    assert!(process.balances());
}
