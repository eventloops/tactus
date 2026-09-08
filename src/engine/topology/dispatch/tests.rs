//! Extended notes: `docs/internals/engine/topology/dispatch/tests.md`

use std::path::{Path, PathBuf};

use super::*;
use crate::engine::topology::scaffold::{
    ALPHA, BETA, OUTCOME, Run, kill_child_and_adopt, kill_child_environment, kill_dir,
};
use crate::topology::effects::{
    EffectSiteId, HookPhase, Injection, ObjectSite, RefSite, ResidueElement, WorktreeSite,
};
use crate::topology::events::{CandidateRef, GitRef};
use crate::topology::fold::{GenerationClass, TaskState};
use crate::workspace_manager::WorkspaceManager;
use crate::workspace_manager::fixture::{git, remove_file, write_file};

const APPEND: EffectSiteId = EffectSiteId::Event(crate::topology::effects::EventSite::Append);
const INTENT: EffectSiteId = EffectSiteId::Worktree(WorktreeSite::WriteIntent);
const ADD: EffectSiteId = EffectSiteId::Worktree(WorktreeSite::Add);
const VERIFY: EffectSiteId = EffectSiteId::Worktree(WorktreeSite::Verify);
const REMOVE: EffectSiteId = EffectSiteId::Worktree(WorktreeSite::Remove);
const MATERIALIZE: EffectSiteId = EffectSiteId::Object(ObjectSite::RepairMaterialize);

fn git_dir(worktree: &Path) -> PathBuf {
    PathBuf::from(git(worktree, &["rev-parse", "--absolute-git-dir"]))
}

fn plant(worktree: &Path, element: ResidueElement) {
    let dir = git_dir(worktree);
    match element {
        ResidueElement::IndexLock => write_file(&dir.join("index.lock"), b""),
        ResidueElement::CherryPickHead => write_file(&dir.join("CHERRY_PICK_HEAD"), b"abc\n"),
        ResidueElement::MergeHead => write_file(&dir.join("MERGE_HEAD"), b"abc\n"),
        ResidueElement::MergeMsg => write_file(&dir.join("MERGE_MSG"), b"interrupted\n"),
        ResidueElement::SequencerState => write_file(&dir.join("sequencer/todo"), b"pick abc\n"),
        other => panic!("`{other:?}` is not administrative residue of the owning git dir"),
    }
}

const ADMINISTRATIVE: [ResidueElement; 5] = [
    ResidueElement::IndexLock,
    ResidueElement::CherryPickHead,
    ResidueElement::MergeHead,
    ResidueElement::MergeMsg,
    ResidueElement::SequencerState,
];

fn healthy_at(manager: &WorkspaceManager, worktree: &Path, base: &str) -> bool {
    let registered = manager
        .worktree_records()
        .expect("worktree records")
        .into_iter()
        .any(|record| crate::util::same_path(record.path(), worktree));
    registered && git(worktree, &["rev-parse", "HEAD"]) == base
}

#[test]
fn task_dispatched_is_durable_before_the_intent_and_the_add() {
    let mut run = Run::started("o21");
    let dispatched = run.dispatch(ALPHA, 0);

    let append = run.must_order_of(APPEND, HookPhase::Before);
    let intent = run.must_order_of(INTENT, HookPhase::Before);
    let add = run.must_order_of(ADD, HookPhase::Before);
    assert!(
        append < intent && intent < add,
        "O21: the order was append={append}, intent={intent}, add={add}, and it must be \
         task_dispatched -> Worktree.WriteIntent -> Worktree.Add"
    );

    assert_eq!(
        run.emitter.durable_kinds(),
        vec!["run_started", "task_dispatched"],
        "the dispatch is on disk, not in a buffer"
    );
    let events = run.emitter.durable_events();
    let crate::topology::events::TopologyEventBody::TaskDispatched { data } = &events[1].body
    else {
        panic!(
            "the second durable event is not a dispatch: {:?}",
            events[1]
        );
    };
    assert_eq!(data.key, ALPHA);
    assert_eq!(data.generation, dispatched.generation);
    assert_eq!(data.base_sha, dispatched.base);
    assert_eq!(
        PathBuf::from(&data.worktree_path),
        dispatched.worktree,
        "the recorded worktree path is the one the add returned: the event's string is derived \
         from the slot before the append, because O21 puts the append first, and this field is \
         what `Worktree.Add` answered — two derivations rather than one local compared to \
         itself, so a dispatch that named one directory and created another fails here. What \
         it does not catch is Git creating a third: both sides re-derive from the slot, and \
         nothing reads the checkout's location back"
    );
    assert!(
        data.source_candidate.is_none(),
        "an ordinary dispatch records no source candidate"
    );

    assert!(
        healthy_at(
            &run.fixture.manager,
            &dispatched.worktree,
            &dispatched.base.0
        ),
        "and the worktree the event promised exists, at the recorded base"
    );
    assert_eq!(
        run.emitter.generation_class(ALPHA, dispatched.generation),
        GenerationClass::OpenNoAttempt
    );
}

#[test]
fn a_containment_condition_that_fails_mid_run_refuses_before_the_append() {
    let mut run = Run::started("containment");
    let durable_before = run.emitter.durable_kinds();

    let foreign = run.manager().execution_root().join("foreign");
    let foreign_arg = foreign.to_string_lossy().into_owned();
    let head = run.fixture.head.clone();
    git(
        &run.fixture.base,
        &[
            "worktree",
            "add",
            "--detach",
            "--quiet",
            &foreign_arg,
            &head,
        ],
    );

    let mark = run.mark();
    let error = run
        .try_dispatch(ALPHA, 0)
        .expect_err("a foreign worktree inside the execution root is a containment refusal");
    assert!(
        error.to_string().contains("is inside it"),
        "the refusal must be the containment one, and said: {error}"
    );

    assert_eq!(
        run.count_after(mark, APPEND, HookPhase::Before),
        0,
        "the refusal must arrive before `task_dispatched`, not after it"
    );
    assert_eq!(
        run.emitter.durable_kinds(),
        durable_before,
        "so the log carries no generation whose worktree can never be built"
    );
    assert!(
        !run.observed(INTENT, HookPhase::Before),
        "and nothing on disk was attempted either"
    );

    git(
        &run.fixture.base,
        &["worktree", "remove", "--force", &foreign_arg],
    );
    let dispatched = run.dispatch(ALPHA, 0);
    assert_eq!(
        run.emitter.durable_kinds().len(),
        durable_before.len() + 1,
        "the control dispatch appends exactly one event"
    );
    assert!(healthy_at(
        &run.fixture.manager,
        &dispatched.worktree,
        &dispatched.base.0
    ));
}

#[test]
fn a_fresh_dispatch_never_verifies_a_worktree_it_is_about_to_create() {
    let mut run = Run::started("o22-fresh");
    let dispatched = run.dispatch(ALPHA, 0);
    assert!(
        !run.observed(VERIFY, HookPhase::Before),
        "a fresh dispatch verified a worktree that did not exist when it started"
    );

    verify_or_recreate(
        &run.fixture.manager,
        &mut run.hooks,
        &dispatched.open_generation(),
        &dispatched.quiescence(),
    )
    .expect("reuse");
    assert!(
        run.observed(VERIFY, HookPhase::Before),
        "and a reuse must verify, or the assertion above is about a site nothing drives"
    );
}

#[test]
fn residue_carrying_worktree_fails_verify_and_is_recreated() {
    let mut run = Run::started("residue");
    let dispatched = run.dispatch(ALPHA, 0);
    let worktree = dispatched.worktree.clone();

    for element in ADMINISTRATIVE {
        plant(&worktree, element);
        assert!(
            !&run
                .fixture
                .manager
                .quiescence(&worktree, &dispatched.quiescence())
                .expect("observe")
                .is_ok(),
            "{element:?}: the worktree must not be quiescent once it is planted"
        );

        let reuse = verify_or_recreate(
            &run.fixture.manager,
            &mut run.hooks,
            &dispatched.open_generation(),
            &dispatched.quiescence(),
        )
        .expect("recreate converges");
        assert_eq!(
            reuse,
            Reuse::Recreated {
                failure: crate::workspace_manager::VerifyFailure::Residue(element)
            },
            "{element:?}: it must be recreated, and for this reason"
        );
        assert!(
            healthy_at(&run.fixture.manager, &worktree, &dispatched.base.0),
            "{element:?}: the recreated worktree is registered and at the recorded base"
        );
        assert!(
            &run.fixture
                .manager
                .quiescence(&worktree, &dispatched.quiescence())
                .expect("observe")
                .is_ok(),
            "{element:?}: and the residue left with the worktree that carried it"
        );
    }

    git(
        &worktree,
        &["checkout", "-q", "--detach", &run.fixture.seed],
    );
    let reuse = verify_or_recreate(
        &run.fixture.manager,
        &mut run.hooks,
        &dispatched.open_generation(),
        &dispatched.quiescence(),
    )
    .expect("recreate converges");
    assert!(
        matches!(
            reuse,
            Reuse::Recreated {
                failure: crate::workspace_manager::VerifyFailure::HeadMismatch { .. }
            }
        ),
        "a worktree at another commit is not the one the generation recorded: {reuse:?}"
    );
    assert!(healthy_at(
        &run.fixture.manager,
        &worktree,
        &dispatched.base.0
    ));

    run.fixture
        .manager
        .remove_worktree(run.hooks.effects(), &dispatched.slot)
        .expect("scrub");
    let reuse = verify_or_recreate(
        &run.fixture.manager,
        &mut run.hooks,
        &dispatched.open_generation(),
        &dispatched.quiescence(),
    )
    .expect("recreate converges");
    assert_eq!(
        reuse,
        Reuse::Recreated {
            failure: crate::workspace_manager::VerifyFailure::NotRegistered
        }
    );
    assert!(healthy_at(
        &run.fixture.manager,
        &worktree,
        &dispatched.base.0
    ));

    for site in [REMOVE, INTENT, ADD] {
        assert!(
            run.observed(site, HookPhase::After),
            "`{site}` never executed, so nothing was actually recreated"
        );
    }
}

#[test]
fn a_quiescent_worktree_is_reused_rather_than_rebuilt() {
    let mut run = Run::started("reuse");
    let dispatched = run.dispatch(ALPHA, 0);
    let sentinel = dispatched.worktree.join("sentinel.txt");
    write_file(&sentinel, b"an untracked file the reuse must not destroy\n");

    let reuse = verify_or_recreate(
        &run.fixture.manager,
        &mut run.hooks,
        &dispatched.open_generation(),
        &dispatched.quiescence(),
    )
    .expect("verify succeeds");
    assert_eq!(reuse, Reuse::Verified);
    assert!(reuse.reused());
    assert!(
        sentinel.is_file(),
        "a reuse that rebuilt the worktree would have taken this file with it"
    );
    assert!(
        !run.observed(REMOVE, HookPhase::Before),
        "a verified worktree must not be removed"
    );
}

#[test]
#[ignore = "spawned as a subprocess by kill_after_dispatch_recreates_worktree_without_spend"]
fn dispatch_kill_child() {
    let (dir, which) = kill_child_environment();
    let mut run = Run::started("killdispatch");
    run.hand_off(&dir);
    match which.as_str() {
        "before_intent" => run.arm(INTENT, HookPhase::Before, Injection::Kill),
        "after_add" => run.arm(ADD, HookPhase::After, Injection::Kill),
        other => panic!("unknown site `{other}`"),
    }
    let _ = run.try_dispatch(ALPHA, 0);
    unreachable!("the kill must have taken this process");
}

#[test]
fn kill_after_dispatch_recreates_worktree_without_spend() {
    for site in ["before_intent", "after_add"] {
        let dir = kill_dir("killdispatch");
        let mut run = kill_child_and_adopt(
            "engine::topology::dispatch::tests::dispatch_kill_child",
            &dir,
            site,
        );

        assert_eq!(
            run.emitter.durable_kinds(),
            vec!["run_started", "task_dispatched"],
            "`{site}`: the dispatch is durable and nothing was spent"
        );
        assert_eq!(
            run.emitter
                .generation_class(ALPHA, crate::topology::events::GenerationId(0)),
            GenerationClass::OpenNoAttempt,
            "`{site}`"
        );
        assert_eq!(run.task_state(ALPHA), TaskState::Pending, "`{site}`");

        let dispatched = Dispatched {
            key: ALPHA,
            generation: crate::topology::events::GenerationId(0),
            base: run.base(),
            slot: task_slot(ALPHA, crate::topology::events::GenerationId(0)),
            worktree: run
                .fixture
                .manager
                .slot_path(&task_slot(ALPHA, crate::topology::events::GenerationId(0))),
            kind: DispatchKind::Ordinary {
                paths: run.predicted(ALPHA),
            },
            materialized: None,
        };

        let existed = dispatched.worktree.is_dir();
        assert_eq!(
            existed,
            site == "after_add",
            "`{site}`: the child left the wrong prefix on disk"
        );

        let reuse = resume_open_no_attempt(
            &run.fixture.manager,
            &mut run.hooks,
            &dispatched.open_generation(),
        )
        .expect("recover")
        .reuse;
        assert_eq!(
            reuse.reused(),
            site == "after_add",
            "`{site}`: an existing quiescent worktree is reused and an absent one is rebuilt \
             ({reuse:?})"
        );
        assert!(
            healthy_at(
                &run.fixture.manager,
                &dispatched.worktree,
                &dispatched.base.0
            ),
            "`{site}`: the worktree is at the recorded base after recovery"
        );
        assert_eq!(
            run.emitter.durable_kinds(),
            vec!["run_started", "task_dispatched"],
            "`{site}`: recovery of an OpenNoAttempt generation appends nothing and spends nothing"
        );
    }
}

fn protected_candidate(run: &mut Run) -> CandidateRef {
    let refname = "refs/upstroke/runs/run-1/candidates/k0/0".to_owned();
    let commit = run.fixture.side.clone();
    run.fixture
        .manager
        .create_ref_zero_old(
            run.hooks.effects(),
            RefSite::CreateCandidates,
            &refname,
            &commit,
        )
        .expect("the authoritative candidates ref");
    CandidateRef {
        key: ALPHA,
        generation: crate::topology::events::GenerationId(0),
        commit_sha: crate::topology::events::CommitSha(commit),
        candidate_ref: GitRef(refname),
    }
}

#[test]
#[ignore = "spawned as a subprocess by repair_materialization_reproduced_after_kill"]
fn repair_kill_child() {
    let (dir, which) = kill_child_environment();
    let mut run = Run::started("killrepair");
    run.hand_off(&dir);
    let source = protected_candidate(&mut run);
    let repair = run.spawn_repair(ALPHA);
    let phase = match which.as_str() {
        "before_materialize" => HookPhase::Before,
        "after_materialize" => HookPhase::After,
        other => panic!("unknown site `{other}`"),
    };
    run.arm(MATERIALIZE, phase, Injection::Kill);
    let request = DispatchRequest {
        key: repair,
        generation: crate::topology::events::GenerationId(0),
        base: run.base(),
        kind: DispatchKind::Repair {
            root: ALPHA,
            source,
        },
    };
    let _ = dispatch(
        &run.fixture.manager,
        &mut run.hooks,
        &mut run.emitter,
        &request,
    );
    unreachable!("the kill must have taken this process");
}

#[test]
fn repair_materialization_reproduced_after_kill() {
    for site in ["before_materialize", "after_materialize"] {
        let dir = kill_dir("killrepair");
        let mut run = kill_child_and_adopt(
            "engine::topology::dispatch::tests::repair_kill_child",
            &dir,
            site,
        );
        let repair = crate::topology::registry::TaskKey(2);
        let generation = crate::topology::events::GenerationId(0);

        assert_eq!(
            run.emitter.durable_kinds(),
            vec!["run_started", "task_spawned", "task_dispatched"],
            "`{site}`: the repair's dispatch is durable"
        );
        let events = run.emitter.durable_events();
        let crate::topology::events::TopologyEventBody::TaskDispatched { data } = &events[2].body
        else {
            panic!("`{site}`: the third durable event is not a dispatch");
        };
        let source = data
            .source_candidate
            .clone()
            .unwrap_or_else(|| panic!("`{site}`: a repair records its source candidate"));
        assert_eq!(
            source.commit_sha.0, run.fixture.side,
            "`{site}`: and it is the protected candidate"
        );

        let dispatched = Dispatched {
            key: repair,
            generation,
            base: run.base(),
            slot: task_slot(repair, generation),
            worktree: run
                .fixture
                .manager
                .slot_path(&task_slot(repair, generation)),
            kind: DispatchKind::Repair {
                root: ALPHA,
                source: source.clone(),
            },
            materialized: None,
        };

        resume_open_no_attempt(
            &run.fixture.manager,
            &mut run.hooks,
            &dispatched.open_generation(),
        )
        .expect("`{site}`: the repair's resume converges");

        let control = Dispatched {
            key: BETA,
            generation: crate::topology::events::GenerationId(0),
            base: run.base(),
            slot: task_slot(BETA, crate::topology::events::GenerationId(0)),
            worktree: run
                .manager()
                .slot_path(&task_slot(BETA, crate::topology::events::GenerationId(0))),
            kind: DispatchKind::Repair {
                root: ALPHA,
                source,
            },
            materialized: None,
        };
        run.fixture
            .manager
            .write_intent(run.hooks.effects(), &control.slot)
            .expect("control intent");
        run.fixture
            .manager
            .add_worktree(run.hooks.effects(), &control.slot, &control.base.0)
            .expect("control worktree");
        materialize_repair(
            &run.fixture.manager,
            &mut run.hooks,
            &control.open_generation(),
        )
        .expect("control materialize");

        assert_eq!(
            git(&dispatched.worktree, &["write-tree"]),
            git(&control.worktree, &["write-tree"]),
            "`{site}`: the reproduced materialization must be the tree an uninterrupted one \
             produces"
        );
        assert!(
            run.observed(MATERIALIZE, HookPhase::After),
            "`{site}`: the recovery re-ran the recorded materialization"
        );
    }
}

#[test]
fn a_repair_whose_source_candidate_is_missing_is_refused_before_any_append() {
    let mut run = Run::started("repairrefusal");
    let source = protected_candidate(&mut run);
    let repair = run.spawn_repair(ALPHA);
    let before = run.emitter.durable_kinds().len();

    let absent_object = CandidateRef {
        commit_sha: crate::topology::events::CommitSha(
            "0123456789abcdef0123456789abcdef01234567".to_owned(),
        ),
        ..source.clone()
    };
    let absent_ref = CandidateRef {
        candidate_ref: GitRef("refs/upstroke/runs/run-1/candidates/k9/9".to_owned()),
        ..source.clone()
    };
    let wrong_target = CandidateRef {
        commit_sha: crate::topology::events::CommitSha(run.fixture.seed.clone()),
        ..source.clone()
    };

    for (what, candidate, expected) in [
        ("an absent object", absent_object, "is not an object"),
        ("an absent ref", absent_ref, "does not exist"),
        ("a ref that names another commit", wrong_target, "names"),
    ] {
        let request = DispatchRequest {
            key: repair,
            generation: crate::topology::events::GenerationId(0),
            base: run.base(),
            kind: DispatchKind::Repair {
                root: ALPHA,
                source: candidate,
            },
        };
        let error = dispatch(
            &run.fixture.manager,
            &mut run.hooks,
            &mut run.emitter,
            &request,
        )
        .expect_err(what);
        let message = error.to_string();
        assert!(
            message.contains(expected),
            "{what}: the refusal must name the reason, and said: {message}"
        );
        assert_eq!(
            run.emitter.durable_kinds().len(),
            before,
            "{what}: the refusal appended something"
        );
        assert!(
            !run.observed(INTENT, HookPhase::Before),
            "{what}: the refusal wrote an intent"
        );
    }

    let request = DispatchRequest {
        key: repair,
        generation: crate::topology::events::GenerationId(0),
        base: run.base(),
        kind: DispatchKind::Repair {
            root: ALPHA,
            source,
        },
    };
    dispatch(
        &run.fixture.manager,
        &mut run.hooks,
        &mut run.emitter,
        &request,
    )
    .expect("the real candidate dispatches");
    assert_eq!(run.emitter.durable_kinds().len(), before + 1);
}

#[test]
fn reproducing_a_materialization_an_ordinary_dispatch_never_had_is_refused() {
    let mut run = Run::started("nomaterialize");
    let dispatched = run.dispatch(ALPHA, 0);
    let error = materialize_repair(
        &run.fixture.manager,
        &mut run.hooks,
        &dispatched.open_generation(),
    )
    .expect_err("an ordinary dispatch materializes nothing");
    assert!(
        error.to_string().contains("no recorded materialization"),
        "{error}"
    );
    assert!(!run.observed(MATERIALIZE, HookPhase::Before));
}

#[test]
fn open_no_attempt_closed_at_run_end() {
    let mut run = Run::started("runend");
    let dispatched = run.dispatch(ALPHA, 0);
    let intent = &run.fixture.manager.intent_path(&dispatched.slot);
    assert!(intent.is_file() && dispatched.worktree.is_dir());

    let mark = run.mark();
    close_at_run_end(
        &run.fixture.manager,
        &mut run.hooks,
        &mut run.emitter,
        &dispatched,
        OUTCOME,
    )
    .expect("close at run end");

    assert_eq!(
        run.emitter.durable_kinds(),
        vec!["run_started", "task_dispatched", "generation_closed"]
    );
    let events = run.emitter.durable_events();
    let crate::topology::events::TopologyEventBody::GenerationClosed { data } = &events[2].body
    else {
        panic!("the third durable event is not a closure");
    };
    assert_eq!(
        data.reason,
        crate::topology::events::GenerationCloseReason::RunEnding { outcome: OUTCOME }
    );
    assert_eq!(
        data.lease,
        crate::topology::events::LeaseDisposition::PredictedReleased,
        "an ordinary generation that closes releases the region it held"
    );
    assert_eq!(
        run.emitter.generation_class(ALPHA, dispatched.generation),
        GenerationClass::Closed
    );

    assert!(
        !dispatched.worktree.exists() && !intent.exists(),
        "the worktree and its intent left with the generation"
    );
    let close = run.order_after(mark, APPEND, HookPhase::After);
    let remove = run.order_after(mark, REMOVE, HookPhase::Before);
    assert!(
        close < remove,
        "the closure must be durable before the scrub: append={close}, remove={remove}"
    );
}

#[test]
fn a_repairs_run_end_closure_holds_the_lineage_lease() {
    let mut run = Run::started("runend-repair");
    let source = protected_candidate(&mut run);
    let repair = run.spawn_repair(ALPHA);
    let request = DispatchRequest {
        key: repair,
        generation: crate::topology::events::GenerationId(0),
        base: run.base(),
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
    assert!(
        run.observed(MATERIALIZE, HookPhase::After),
        "a repair materializes its source as part of the dispatch"
    );

    close_at_run_end(
        &run.fixture.manager,
        &mut run.hooks,
        &mut run.emitter,
        &dispatched,
        OUTCOME,
    )
    .expect("close at run end");
    let events = run.emitter.durable_events();
    let crate::topology::events::TopologyEventBody::GenerationClosed { data } =
        &events.last().expect("a closure").body
    else {
        panic!("the last durable event is not a closure");
    };
    assert_eq!(
        data.lease,
        crate::topology::events::LeaseDisposition::LineageHeld,
        "a repair never changes a lineage lease"
    );
}

#[test]
fn an_add_whose_intent_is_gone_is_refused_rather_than_leaking_a_worktree() {
    let mut run = Run::started("addwithoutintent");
    let dispatched = run.dispatch(ALPHA, 0);
    run.fixture
        .manager
        .remove_worktree(run.hooks.effects(), &dispatched.slot)
        .expect("scrub");
    remove_file(&run.fixture.manager.intent_path(&dispatched.slot));

    let error = run
        .fixture
        .manager
        .add_worktree(run.hooks.effects(), &dispatched.slot, &dispatched.base.0)
        .expect_err("an add without an intent is refused");
    assert!(error.to_string().contains("durable intent"), "{error}");
    assert!(!dispatched.worktree.exists());
}

fn dispatch_repair(
    run: &mut Run,
    key: TaskKey,
    generation: u32,
    base: &str,
    source: &CandidateRef,
) -> Dispatched {
    let root = run
        .emitter
        .fold()
        .registry()
        .and_then(|registry| registry.get(key))
        .and_then(|entry| entry.lineage)
        .map_or(ALPHA, |lineage| lineage.root);
    let request = DispatchRequest {
        key,
        generation: crate::topology::events::GenerationId(generation),
        base: crate::topology::events::CommitSha(base.to_owned()),
        kind: DispatchKind::Repair {
            root,
            source: source.clone(),
        },
    };
    dispatch(
        &run.fixture.manager,
        &mut run.hooks,
        &mut run.emitter,
        &request,
    )
    .expect("the repair dispatches")
}

fn unmerged_entries(worktree: &Path) -> usize {
    git(worktree, &["ls-files", "--unmerged"]).lines().count()
}

fn index_is_clean(worktree: &Path) -> bool {
    crate::workspace_manager::fixture::git_out(worktree, &["diff", "--cached", "--quiet"])
        .status
        .success()
}

#[test]
fn a_repair_dispatch_records_what_its_materialization_observed() {
    use crate::topology::events::Materialization;

    let mut run = Run::started("materialization-kinds");
    let source = protected_candidate(&mut run);
    let repair = run.spawn_repair(ALPHA);
    let head = run.fixture.head.clone();
    let side = run.fixture.side.clone();
    let conflicting = run.commit_with(&head, "c.txt", "not the side's content\n", "c-other");

    let clean = dispatch_repair(&mut run, repair, 0, &head, &source);
    assert_eq!(
        clean.materialized,
        Some(Materialization::Clean),
        "the candidate's change applies onto a head that lacks it"
    );
    assert_eq!(unmerged_entries(&clean.worktree), 0);
    assert!(
        !index_is_clean(&clean.worktree),
        "and the repair index carries the applied change"
    );
    close_at_run_end(
        &run.fixture.manager,
        &mut run.hooks,
        &mut run.emitter,
        &clean,
        OUTCOME,
    )
    .expect("close the clean generation");

    let empty = dispatch_repair(&mut run, repair, 1, &side, &source);
    assert_eq!(
        empty.materialized,
        Some(Materialization::Empty),
        "the candidate's change is already present on its own commit"
    );
    assert_eq!(unmerged_entries(&empty.worktree), 0);
    assert!(
        index_is_clean(&empty.worktree),
        "an already-present change stages nothing: `repairs.empty_source` proceeds as an \
         ordinary attempt whose empty diff then fails under the existing rule"
    );
    close_at_run_end(
        &run.fixture.manager,
        &mut run.hooks,
        &mut run.emitter,
        &empty,
        OUTCOME,
    )
    .expect("close the empty generation");

    let conflict = dispatch_repair(&mut run, repair, 2, &conflicting, &source);
    assert_eq!(
        conflict.materialized,
        Some(Materialization::Conflict),
        "a head that already carries another `c.txt` conflicts with the candidate's"
    );
    assert!(
        unmerged_entries(&conflict.worktree) >= 2,
        "the conflict is left in the index for the worker to resolve"
    );
    assert!(
        std::fs::read_to_string(conflict.worktree.join("c.txt"))
            .expect("the conflicted file")
            .contains("<<<<<<< "),
        "and in the working tree, with its markers"
    );

    let started: Vec<Option<Materialization>> = run
        .emitter
        .durable_events()
        .iter()
        .filter_map(|event| match &event.body {
            crate::topology::events::TopologyEventBody::TaskDispatched { data }
                if data.key == repair =>
            {
                Some(
                    data.source_candidate
                        .as_ref()
                        .map(|_| Materialization::Retained),
                )
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        started.len(),
        3,
        "three dispatches, each recording its source candidate"
    );
    assert_eq!(
        run.emitter
            .durable_events()
            .iter()
            .filter(|event| matches!(
                &event.body,
                crate::topology::events::TopologyEventBody::GenerationClosed { data }
                    if data.lease == crate::topology::events::LeaseDisposition::LineageHeld
            ))
            .count(),
        2,
        "each closed repair generation recorded `LineageHeld`"
    );
    run.replay_twice_equal();
}

#[test]
fn repair_materialization_synthetic_residue_recreated_after_forced_removal() {
    use crate::topology::effects::ObjectResidue;
    use crate::workspace_manager::{
        ResidueTarget, classify_object_residue, element_breaks_quiescence, object_directory,
        temporary_object_files, unreachable_objects,
    };

    let mut run = Run::started("synthetic-materialization");
    let source = protected_candidate(&mut run);
    let repair = run.spawn_repair(ALPHA);
    let head = run.fixture.head.clone();
    let dispatched = dispatch_repair(&mut run, repair, 0, &head, &source);
    let open = dispatched.open_generation();
    let worktree = dispatched.worktree.clone();

    let control_slot = task_slot(BETA, crate::topology::events::GenerationId(0));
    run.fixture
        .manager
        .write_intent(run.hooks.effects(), &control_slot)
        .expect("control intent");
    let control = run
        .fixture
        .manager
        .add_worktree(run.hooks.effects(), &control_slot, &head)
        .expect("control worktree");
    materialize_repair(
        &run.fixture.manager,
        &mut run.hooks,
        &OpenGeneration {
            key: BETA,
            generation: crate::topology::events::GenerationId(0),
            base: dispatched.base.clone(),
            slot: control_slot,
            source: Some(source.clone()),
        },
    )
    .expect("control materialize");
    let expected_tree = git(&control, &["write-tree"]);

    let elements = MATERIALIZE.residue_elements();
    assert_eq!(
        elements.len(),
        4,
        "Object.RepairMaterialize registers four elements and this test constructs each: \
         {elements:?}"
    );
    let mut planted_objects = Vec::new();
    for element in elements {
        // A fresh worktree at the base to plant into: the forced removal and
        // the add, which is what the tabled recovery does after a failed
        // verification (the intent the dispatch wrote is still there).
        run.fixture
            .manager
            .remove_worktree(run.hooks.effects(), &open.slot)
            .expect("forced removal");
        run.fixture
            .manager
            .add_worktree(run.hooks.effects(), &open.slot, &open.base.0)
            .expect("a fresh worktree at the base");

        let dir = git_dir(&worktree);
        match element {
            ResidueElement::UnreferencedObject => {
                let orphan = run.fixture.root.join(format!("orphan-{element:?}"));
                write_file(
                    &orphan,
                    b"an object a killed pick wrote and never published\n",
                );
                let id = git(
                    &worktree,
                    &["hash-object", "-w", orphan.to_str().expect("utf-8")],
                );
                assert!(
                    unreachable_objects(&run.fixture.base)
                        .expect("fsck")
                        .contains(&id),
                    "the planted object must really be unreachable"
                );
                planted_objects.push(id);
            }
            ResidueElement::TemporaryObjectFile => {
                let objects = object_directory(&worktree).expect("the object directory");
                write_file(&objects.join("tmp_obj_repair"), b"half an object\n");
                assert!(temporary_object_files(&worktree).expect("temp files"));
            }
            ResidueElement::IndexLock => write_file(&dir.join("index.lock"), b""),
            ResidueElement::CherryPickHead => {
                write_file(
                    &dir.join("CHERRY_PICK_HEAD"),
                    format!("{}\n", source.commit_sha).as_bytes(),
                );
            }
            other => panic!("`{other:?}` is not registered for Object.RepairMaterialize"),
        }

        let target = ResidueTarget::new(&run.fixture.base).at(&worktree);
        assert_eq!(
            classify_object_residue(MATERIALIZE, &target).expect("classified"),
            ObjectResidue::Internal,
            "{element:?}: objects written with the reference unpublished is the Internal class"
        );

        let mark = run.mark();
        let resumed = resume_open_no_attempt(&run.fixture.manager, &mut run.hooks, &open)
            .expect("the tabled recovery converges");
        assert_eq!(
            matches!(resumed.reuse, Reuse::Recreated { .. }),
            element_breaks_quiescence(*element),
            "{element:?}: administrative residue is recreated with force, and residue that \
             lives only in the shared object store is not what `Worktree.Verify` refuses on"
        );
        assert_eq!(
            resumed.materialized,
            Some(crate::topology::events::Materialization::Clean),
            "{element:?}: the materialization is reproduced deterministically"
        );
        assert_eq!(
            run.count_after(mark, MATERIALIZE, HookPhase::After),
            1,
            "{element:?}: exactly one cherry-pick reproduces it"
        );
        assert_eq!(
            git(&worktree, &["write-tree"]),
            expected_tree,
            "{element:?}: and it is the tree an uninterrupted materialization produces"
        );
        assert!(
            classify_object_residue(MATERIALIZE, &target).expect("classified")
                == ObjectResidue::After,
            "{element:?}: after the recovery the index references the merge objects"
        );
        if let Ok(objects) = object_directory(&worktree) {
            if objects.join("tmp_obj_repair").exists() {
                remove_file(&objects.join("tmp_obj_repair"));
            }
        }
    }
    for object in &planted_objects {
        assert!(
            crate::workspace_manager::fixture::git_out(
                &run.fixture.base,
                &["cat-file", "-e", object]
            )
            .status
            .success(),
            "R27: an object an interrupted materialization wrote is Git's, never deleted"
        );
    }
}

#[test]
fn sampled_repair_materialization_child_kills_every_residue_classified_and_recovered() {
    use crate::topology::effects::ObjectResidue;
    use crate::workspace_manager::fixture::{KillableGitChild, died_by_kill};
    use crate::workspace_manager::{ResidueTarget, classify_object_residue};

    const SAMPLING_N: u32 = 8;

    let mut run = Run::started("sampled-materialization");
    let source = protected_candidate(&mut run);
    let head = run.fixture.head.clone();
    let argv = vec![
        "cherry-pick".to_owned(),
        "--no-commit".to_owned(),
        source.commit_sha.0.clone(),
    ];

    let probe_slot = task_slot(TaskKey(80), crate::topology::events::GenerationId(0));
    run.fixture
        .manager
        .write_intent(run.hooks.effects(), &probe_slot)
        .expect("probe intent");
    let probe = run
        .fixture
        .manager
        .add_worktree(run.hooks.effects(), &probe_slot, &head)
        .expect("probe worktree");
    let budget = crate::workspace_manager::fixture::time_git(&probe, &argv)
        .max(std::time::Duration::from_micros(200));
    let expected_tree = git(&probe, &["write-tree"]);

    let mut observed = Vec::new();
    let mut refusals = Vec::new();
    let mut killed_while_running = 0_u32;
    for sample in 0..SAMPLING_N {
        let key = TaskKey(90 + sample);
        let slot = task_slot(key, crate::topology::events::GenerationId(0));
        run.fixture
            .manager
            .write_intent(run.hooks.effects(), &slot)
            .expect("sample intent");
        let worktree = run
            .fixture
            .manager
            .add_worktree(run.hooks.effects(), &slot, &head)
            .expect("sample worktree");

        let mut child = KillableGitChild::spawn(&worktree, &argv);
        std::thread::sleep(budget.mul_f64(f64::from(sample + 1) / f64::from(SAMPLING_N + 1)));
        let running_at_kill = child.exited().is_none();
        child.kill();
        let status = child.wait();
        let killed = died_by_kill(&status);
        assert!(
            killed || status.success(),
            "sample {sample}: the child ended {status:?}, which is neither the kill's signature \
             nor a completed pick"
        );
        if killed {
            killed_while_running += 1;
        } else {
            assert!(
                !running_at_kill || status.success(),
                "sample {sample}: the child was running when the kill fired and yet exited \
                 {status:?}"
            );
        }

        let target = ResidueTarget::new(&run.fixture.base).at(&worktree);
        match classify_object_residue(MATERIALIZE, &target) {
            Ok(class) => observed.push(class),
            Err(error) => refusals.push(format!("sample {sample}: {error}")),
        }

        let open = OpenGeneration {
            key,
            generation: crate::topology::events::GenerationId(0),
            base: crate::topology::events::CommitSha(head.clone()),
            slot: slot.clone(),
            source: Some(source.clone()),
        };
        let resumed = resume_open_no_attempt(&run.fixture.manager, &mut run.hooks, &open)
            .unwrap_or_else(|error| {
                panic!("sample {sample}: the recovery did not converge: {error}")
            });
        assert_eq!(
            resumed.materialized,
            Some(crate::topology::events::Materialization::Clean),
            "sample {sample}: the materialization is reproduced after whatever the kill left"
        );
        assert_eq!(
            git(&worktree, &["write-tree"]),
            expected_tree,
            "sample {sample}: deterministically"
        );
        assert_eq!(
            classify_object_residue(MATERIALIZE, &target).expect("classified after recovery"),
            ObjectResidue::After,
            "sample {sample}: the recovered worktree's index references the merge objects"
        );
        run.fixture
            .manager
            .remove_worktree(run.hooks.effects(), &slot)
            .expect("scrub the sample");
        run.fixture
            .manager
            .remove_intent(run.hooks.effects(), &slot)
            .expect("scrub the sample intent");
    }

    assert!(
        refusals.is_empty(),
        "the classifier refused {} of {SAMPLING_N} samples: {refusals:?}",
        refusals.len()
    );
    assert_eq!(
        observed.len(),
        SAMPLING_N as usize,
        "every sample was classified into one of the site's classes and recovered: {observed:?}"
    );
    assert!(
        killed_while_running >= 1,
        "no sample died by the kill, so the evidence of {SAMPLING_N} samples was of completed \
         picks, not of kills: {observed:?}"
    );
}

#[test]
fn repair_materialization_objects_released_to_git_on_scrub() {
    use crate::workspace_manager::unreachable_objects;

    let mut run = Run::started("materialization-objects");
    let seed = run.fixture.seed.clone();
    let candidate_commit = run.commit_with(
        &seed,
        "a.txt",
        "one\ntwo, from the candidate\n",
        "candidate-appends",
    );
    let repair_base = run.commit_with(&seed, "a.txt", "zero, merged first\none\n", "base-prepends");
    let source = run.protect_candidate(&candidate_commit);
    let repair = run.spawn_repair(ALPHA);

    let dispatched = dispatch_repair(&mut run, repair, 0, &repair_base, &source);
    assert_eq!(
        dispatched.materialized,
        Some(crate::topology::events::Materialization::Clean),
        "two edits to different lines merge cleanly"
    );
    let merged_blob = git(&dispatched.worktree, &["rev-parse", ":a.txt"]);
    assert_eq!(
        git(&dispatched.worktree, &["cat-file", "-p", &merged_blob]),
        "zero, merged first\none\ntwo, from the candidate",
        "the merge produced a blob neither side had"
    );
    let candidate_blob = git(
        &run.fixture.base,
        &["rev-parse", &format!("{candidate_commit}:a.txt")],
    );
    let base_blob = git(
        &run.fixture.base,
        &["rev-parse", &format!("{repair_base}:a.txt")],
    );
    assert!(
        merged_blob != candidate_blob && merged_blob != base_blob,
        "and it is a new object: {merged_blob} vs {candidate_blob} and {base_blob}"
    );
    assert!(
        !unreachable_objects(&run.fixture.base)
            .expect("fsck")
            .contains(&merged_blob),
        "R9: while the repair worktree exists its index references the merge object"
    );

    scrub(&run.fixture.manager, &mut run.hooks, &dispatched.slot).expect("scrub");
    assert!(
        unreachable_objects(&run.fixture.base)
            .expect("fsck")
            .contains(&merged_blob),
        "R27: once the worktree is gone nothing references the merge object"
    );
    assert!(
        crate::workspace_manager::fixture::git_out(
            &run.fixture.base,
            &["cat-file", "-e", &merged_blob]
        )
        .status
        .success(),
        "and it is Git's: released, never deleted by the run"
    );
    assert!(
        crate::workspace_manager::fixture::git_out(
            &run.fixture.base,
            &["cat-file", "-e", &candidate_commit]
        )
        .status
        .success(),
        "R11: the source candidate stays reachable through its ref"
    );
}

/// The two kill points of `cherry-pick --no-commit` that lie *after* its index
/// write, pinned deterministically (the sampler above reaches them only when
/// its timing does). Measured on git 2.43: the index lock is released by
/// `write_locked_index` before `write_message` creates `MERGE_MSG.lock` and
/// renames it to `MERGE_MSG`, so a kill in between leaves a merged index with
/// no state file at all (K2), and a kill inside `write_message` leaves the
/// merged index plus the held `MERGE_MSG.lock` (K3). K3 is what the sampler
/// first found: `Worktree.Verify` read only the committed `MERGE_MSG`, passed
/// the worktree, and the re-run pick died on `could not lock 'MERGE_MSG'`.
#[test]
fn a_materialization_killed_after_its_index_write_converges_from_both_of_its_states() {
    use crate::topology::effects::ResidueElement;
    use crate::topology::events::Materialization;

    let mut run = Run::started("materialization-late-kills");
    let source = protected_candidate(&mut run);
    let repair = run.spawn_repair(ALPHA);
    let head = run.fixture.head.clone();
    let dispatched = dispatch_repair(&mut run, repair, 0, &head, &source);
    assert_eq!(dispatched.materialized, Some(Materialization::Clean));
    let open = dispatched.open_generation();
    let worktree = dispatched.worktree.clone();
    let dir = git_dir(&worktree);
    let expected_tree = git(&worktree, &["write-tree"]);
    assert!(
        !dir.join("MERGE_MSG").exists() && !dir.join("AUTO_MERGE").exists(),
        "the funnel cleared the pick's state files once the pick had ended"
    );

    // K3: the message's lock held, the message itself not yet in place.
    write_file(&dir.join("MERGE_MSG.lock"), b"side\n");
    let resumed = resume_open_no_attempt(&run.fixture.manager, &mut run.hooks, &open)
        .expect("a held MERGE_MSG.lock is recovered from");
    assert_eq!(
        resumed.reuse,
        Reuse::Recreated {
            failure: crate::workspace_manager::VerifyFailure::Residue(ResidueElement::MergeMsg)
        },
        "the held lock is read as the message's residue and the worktree is recreated"
    );
    assert_eq!(resumed.materialized, Some(Materialization::Clean));
    assert_eq!(git(&worktree, &["write-tree"]), expected_tree);
    assert!(
        !git_dir(&worktree).join("MERGE_MSG.lock").exists(),
        "a recreated worktree has a fresh git dir"
    );

    // K2: the index written and released, no state file — which, since the
    // funnel clears the state files itself, is also the completed after phase.
    // The packet's quiescence rule (HEAD at base, index unlocked, no
    // cherry-pick state) is satisfied, so the worktree is reused as it stands;
    // the pick re-run on an index that already holds its change is a no-op
    // merge that reports the same observation, and the tree is the same tree.
    let dir = git_dir(&worktree);
    remove_file(&dir.join("MERGE_MSG"));
    remove_file(&dir.join("AUTO_MERGE"));
    assert!(
        !index_is_clean(&worktree),
        "the merged index is the state under test"
    );
    let resumed = resume_open_no_attempt(&run.fixture.manager, &mut run.hooks, &open)
        .expect("a merged index with no state file is recovered from");
    assert_eq!(resumed.reuse, Reuse::Verified);
    assert_eq!(resumed.materialized, Some(Materialization::Clean));
    assert_eq!(git(&worktree, &["write-tree"]), expected_tree);

    // K2 for a conflicting source: the unmerged entries are the state.
    let conflicting = run.commit_with(&head, "c.txt", "not the side's content\n", "c-other");
    // Rooted at BETA: the scaffold's repair display ids are per root.
    let conflict_key = run.spawn_repair(BETA);
    let conflicted = dispatch_repair(&mut run, conflict_key, 0, &conflicting, &source);
    assert_eq!(conflicted.materialized, Some(Materialization::Conflict));
    let conflicted_open = conflicted.open_generation();
    let dir = git_dir(&conflicted.worktree);
    let unmerged_before = unmerged_entries(&conflicted.worktree);
    assert!(unmerged_before > 0, "the conflict left unmerged entries");
    assert!(
        !dir.join("MERGE_MSG").exists(),
        "a conflicting pick's state files are cleared too"
    );
    let resumed = resume_open_no_attempt(&run.fixture.manager, &mut run.hooks, &conflicted_open)
        .expect("an unmerged index with no state file is recovered from");
    assert_eq!(resumed.reuse, Reuse::Verified);
    assert_eq!(
        resumed.materialized,
        Some(Materialization::Conflict),
        "the re-run pick refuses an unmerged index and the observation is still the conflict"
    );
    assert_eq!(
        unmerged_entries(&conflicted.worktree),
        unmerged_before,
        "and the unmerged entries the worker is to resolve are exactly the ones it left"
    );
    run.replay_twice_equal();
}
