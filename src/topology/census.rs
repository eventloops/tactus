//! Extended notes: `docs/internals/topology/census.md`

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

use crate::topology::events::{DerivedOutcome, TopologyEvent};
use crate::topology::fold::TopologyFold;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CensusBounds {
    pub originals: u32,
    pub repairs: u32,
    pub generations_per_task: u32,
    pub attempts_per_generation: u32,
    pub sequences: u32,
    pub defers: u32,
    pub questions: u32,
    pub review_passes: u32,
    pub resumes: u32,
    pub max_trace: usize,
    pub max_states: usize,
}

impl CensusBounds {
    pub const fn dimensions(&self) -> [(&'static str, u32); 9] {
        [
            ("originals", self.originals),
            ("repairs", self.repairs),
            ("generations_per_task", self.generations_per_task),
            ("attempts_per_generation", self.attempts_per_generation),
            ("sequences", self.sequences),
            ("defers", self.defers),
            ("questions", self.questions),
            ("review_passes", self.review_passes),
            ("resumes", self.resumes),
        ]
    }
}

impl Default for CensusBounds {
    fn default() -> Self {
        Self {
            originals: 3,
            repairs: 2,
            generations_per_task: 2,
            attempts_per_generation: 2,
            sequences: 4,
            defers: 2,
            questions: 2,
            review_passes: 1,
            resumes: 2,
            max_trace: 48,
            max_states: 20_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub label: String,
    pub event: TopologyEvent,
}

impl Candidate {
    pub fn new(label: impl Into<String>, event: TopologyEvent) -> Self {
        Self {
            label: label.into(),
            event,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionOutcome {
    Accepted { to: usize },
    Refused { reason: Arc<str> },
    Truncated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CensusTransition {
    pub from: usize,
    pub label: Arc<str>,
    pub kind: &'static str,
    pub outcome: TransitionOutcome,
}

#[derive(Debug, Clone)]
pub struct CensusState {
    pub id: usize,
    pub trace: Vec<TopologyEvent>,
    pub fold: TopologyFold,
    pub outcome: DerivedOutcome,
}

#[derive(Debug, Clone)]
pub struct Census {
    bounds: CensusBounds,
    states: Vec<CensusState>,
    transitions: Vec<CensusTransition>,
    outgoing: Vec<std::ops::Range<usize>>,
    truncated: bool,
}

impl Census {
    pub fn explore<F>(
        start: TopologyFold,
        seed: Vec<TopologyEvent>,
        bounds: CensusBounds,
        classes: F,
    ) -> Self
    where
        F: Fn(&TopologyFold) -> Vec<Candidate>,
    {
        let mut states: Vec<CensusState> = Vec::new();
        let mut transitions: Vec<CensusTransition> = Vec::new();
        let mut outgoing: Vec<std::ops::Range<usize>> = Vec::new();
        let mut seen: BTreeMap<String, usize> = BTreeMap::new();
        let mut frontier: VecDeque<usize> = VecDeque::new();
        let mut truncated = false;
        let mut interned: BTreeMap<String, Arc<str>> = BTreeMap::new();

        seen.insert(fingerprint(&start), 0);
        states.push(CensusState {
            id: 0,
            outcome: start.derived_outcome(),
            trace: seed,
            fold: start,
        });
        outgoing.push(0..0);
        frontier.push_back(0);

        while let Some(id) = frontier.pop_front() {
            let first = transitions.len();
            if states[id].trace.len() >= bounds.max_trace {
                // The trace ceiling stops expansion here; if anything legal
                // was left to explore, the census says so, the way it says
                // so at the state ceiling.
                if classes(&states[id].fold)
                    .iter()
                    .any(|candidate| states[id].fold.plan_transition(&candidate.event).is_ok())
                {
                    truncated = true;
                }
                continue;
            }
            for candidate in classes(&states[id].fold) {
                let kind = candidate.event.body.kind();
                let label = intern(&mut interned, candidate.label);
                let outcome = match states[id].fold.plan_transition(&candidate.event) {
                    Err(error) => TransitionOutcome::Refused {
                        reason: intern(&mut interned, error.to_string()),
                    },
                    Ok(delta) => {
                        let mut next = states[id].fold.clone();
                        next.apply_delta(delta);
                        let key = fingerprint(&next);
                        match seen.get(&key) {
                            Some(existing) => TransitionOutcome::Accepted { to: *existing },
                            None => {
                                if states.len() >= bounds.max_states {
                                    truncated = true;
                                    transitions.push(CensusTransition {
                                        from: id,
                                        label,
                                        kind,
                                        outcome: TransitionOutcome::Truncated,
                                    });
                                    continue;
                                }
                                let to = states.len();
                                let mut trace = states[id].trace.clone();
                                trace.push(candidate.event.clone());
                                seen.insert(key, to);
                                states.push(CensusState {
                                    id: to,
                                    trace,
                                    outcome: next.derived_outcome(),
                                    fold: next,
                                });
                                outgoing.push(0..0);
                                frontier.push_back(to);
                                TransitionOutcome::Accepted { to }
                            }
                        }
                    }
                };
                transitions.push(CensusTransition {
                    from: id,
                    label,
                    kind,
                    outcome,
                });
            }
            if let Some(range) = outgoing.get_mut(id) {
                *range = first..transitions.len();
            }
        }

        Self {
            bounds,
            states,
            transitions,
            outgoing,
            truncated,
        }
    }

    pub fn bounds(&self) -> CensusBounds {
        self.bounds
    }

    pub fn states(&self) -> &[CensusState] {
        &self.states
    }

    pub fn transitions(&self) -> &[CensusTransition] {
        &self.transitions
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }

    pub fn outgoing(&self, id: usize) -> impl Iterator<Item = &CensusTransition> {
        self.outgoing
            .get(id)
            .and_then(|range| self.transitions.get(range.clone()))
            .unwrap_or(&[])
            .iter()
    }

    pub fn has_legal_transition(&self, id: usize) -> bool {
        self.outgoing(id).any(|transition| {
            matches!(
                transition.outcome,
                TransitionOutcome::Accepted { .. } | TransitionOutcome::Truncated
            )
        })
    }

    pub fn accepted_labels(&self) -> BTreeSet<&str> {
        self.labels(true)
    }

    pub fn refused_labels(&self) -> BTreeSet<&str> {
        self.labels(false)
    }

    fn labels(&self, accepted: bool) -> BTreeSet<&str> {
        self.transitions
            .iter()
            .filter(|transition| {
                matches!(transition.outcome, TransitionOutcome::Accepted { .. }) == accepted
            })
            .map(|transition| &*transition.label)
            .collect()
    }

    pub fn states_with(&self, outcome: &DerivedOutcome) -> Vec<&CensusState> {
        self.states
            .iter()
            .filter(|state| &state.outcome == outcome)
            .collect()
    }

    pub fn totality_audit(&self) -> TotalityAudit {
        TotalityAudit::over(&self.states)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TotalityAudit {
    pub evaluated: Vec<usize>,
    pub fold_errors: Vec<usize>,
    pub disagreements: Vec<usize>,
    pub not_ending: usize,
    pub ending: usize,
}

impl TotalityAudit {
    pub fn over(states: &[CensusState]) -> Self {
        let mut audit = Self {
            evaluated: Vec::with_capacity(states.len()),
            fold_errors: Vec::new(),
            disagreements: Vec::new(),
            not_ending: 0,
            ending: 0,
        };
        for state in states {
            audit.evaluated.push(state.id);
            let raw = state.fold.derived_outcome();
            if raw == DerivedOutcome::FoldError || state.outcome == DerivedOutcome::FoldError {
                audit.fold_errors.push(state.id);
            }
            if raw != state.outcome {
                audit.disagreements.push(state.id);
            }
            match raw {
                DerivedOutcome::NotEnding => audit.not_ending += 1,
                DerivedOutcome::Ending(_) => audit.ending += 1,
                DerivedOutcome::FoldError => {}
            }
        }
        audit
    }
}

fn fingerprint(fold: &TopologyFold) -> String {
    format!("{:?}|{:?}", fold.state(), fold.is_poisoned())
}

fn intern(table: &mut BTreeMap<String, Arc<str>>, text: String) -> Arc<str> {
    if let Some(held) = table.get(&text) {
        return Arc::clone(held);
    }
    let shared: Arc<str> = Arc::from(text.as_str());
    table.insert(text, Arc::clone(&shared));
    shared
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::OnceLock;
    use std::time::Duration;

    use super::*;
    use crate::events::{
        AttemptRecord, BindingSummary, BudgetKind, ChainSummary, GateSummary, ReviewPassOutcome,
        ReviewRecord, RunOutcome,
    };
    use crate::gates::ShellKind;
    use crate::ir::{
        Artifact, ArtifactId, Effort, Plan, PlanSource, QuestionId, QuestionKind,
        ResolvedEffortPolicy, Task, TaskId, TaskKind, Tier,
    };
    use crate::review::{PassBinding, ReviewPlan};
    use crate::topology::events::{
        AttemptFinished4, AttemptNumber, AttemptSettlement, AttemptStarted4, BudgetExceeded4,
        CandidateLeaseEffect, CandidatePrepared, CandidateRef, CommitSha, DeferWaitElapsed4,
        GenerationCloseReason, GenerationClosed, GenerationId, GitRef, ImageIdentity,
        IncarnationId, InfrastructureKind, LeaseDisposition, LeaseGrant, MergeLeaseRelease,
        MergePrepared, MergeVerificationStarted, MergeVerificationUnavailable, PreparedDisposition,
        RunStarted4, RungBinding, RunnerContract, RunnerKind, RunnerPolicy, SequenceId,
        SettlementTransition, TaskCandidateCreated, TaskDispatched, TaskMerged, TopologyEvent,
        TopologyEventBody, TopologyLimits, UnavailableCause, UnavailableOutcome, VerificationBasis,
        VerificationRecord, VerificationSource, VerificationVerdict,
    };
    use crate::topology::fold::{
        FrozenInputs, GenerationClass, PreparedCandidate, TaskState, TopologyFold, TransactionClass,
    };
    use crate::topology::paths::{GitPath, PathGrammar, PathPolicy, PathPolicyVersion, PathSet};
    use crate::topology::registry::{TaskKey, TaskRegistry};
    use crate::topology::schema::TOPOLOGY_SCHEMA;

    const RUN_ID: &str = "01CENSUS000000000000000009";
    const ALEPH: TaskKey = TaskKey(0);
    const BET: TaskKey = TaskKey(1);
    const GIMEL: TaskKey = TaskKey(2);

    /// The plan shapes the packet's bounds name. Three originals admit a
    /// chain, a fan-out and, as the diamond's join, a task after two
    /// independent ones; a four-node diamond needs a fourth original.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum PlanShape {
        Chain,
        FanOut,
        Join,
    }

    impl PlanShape {
        fn deps(self) -> [&'static [&'static str]; 3] {
            match self {
                Self::Chain => [&[], &["aleph"], &["bet"]],
                Self::FanOut => [&[], &["aleph"], &["aleph"]],
                Self::Join => [&[], &[], &["aleph", "bet"]],
            }
        }

        fn name(self) -> &'static str {
            match self {
                Self::Chain => "chain",
                Self::FanOut => "fan-out",
                Self::Join => "join",
            }
        }
    }

    /// The shape the shared census explores: aleph first, then bet and
    /// gimel interleaving under it.
    const MAIN_SHAPE: PlanShape = PlanShape::FanOut;

    fn sha(label: &str) -> CommitSha {
        let mut value = format!("{label:-<40}");
        value.truncate(40);
        CommitSha(value)
    }

    fn git_ref(name: &str) -> GitRef {
        GitRef(format!("refs/upstroke/census/{RUN_ID}/{name}"))
    }

    fn task_of(id: &str, deps: &[&str], hint: &str) -> Task {
        Task {
            id: TaskId::from(id),
            kind: match id {
                "aleph" => TaskKind::Refactor,
                "gimel" => TaskKind::Docs,
                _ => TaskKind::Test,
            },
            title: format!("  {id} — Ünicode title  "),
            body: format!("{id} body"),
            depends_on: deps.iter().copied().map(TaskId::from).collect(),
            acceptance: vec![format!("{id} holds")],
            path_hints: vec![hint.to_owned()],
            suggested_tier: if id == "aleph" {
                Some(Tier::Mid)
            } else {
                Some(Tier::Small)
            },
            min_tier: None,
            artifacts_in: Vec::new(),
            artifacts_out: vec![ArtifactId::from(format!("{id}-out").as_str())],
        }
    }

    fn plan_for(shape: PlanShape) -> Plan {
        let [aleph, bet, gimel] = shape.deps();
        Plan {
            source: PlanSource {
                adapter: "markdown".to_owned(),
                hash: "census-frozen-hash".to_owned(),
            },
            tasks: vec![
                task_of("aleph", aleph, "src/aleph/"),
                task_of("bet", bet, "src/bet/"),
                task_of("gimel", gimel, "src/gimel/"),
            ],
            artifacts: vec![Artifact {
                id: ArtifactId::from("aleph-out"),
                produced_by: Some(TaskId::from("aleph")),
            }],
        }
    }

    fn chain(task: &str) -> ChainSummary {
        let tiers = match task {
            "aleph" => vec![Tier::Mid, Tier::Frontier],
            "gimel" => vec![Tier::Small, Tier::Mid],
            _ => vec![Tier::Small],
        };
        ChainSummary {
            task: task.to_owned(),
            attempts_per: if task == "aleph" { 2 } else { 1 },
            bindings: Some(
                tiers
                    .iter()
                    .map(|tier| BindingSummary {
                        tier: *tier,
                        agent: format!("{task}-{tier}-agent"),
                        model: format!("{task}-{tier}-model"),
                        pinned: *tier == Tier::Frontier,
                    })
                    .collect(),
            ),
            tiers,
        }
    }

    const NORMALIZED_DIGEST: &str =
        "sha256:5555555555555555555555555555555555555555555555555555555555555555";

    fn path_policy() -> PathPolicy {
        PathPolicy {
            version: PathPolicyVersion::V2,
            case_fold: true,
            grammar: PathGrammar::Globset,
        }
    }

    fn inputs() -> FrozenInputs {
        inputs_for(MAIN_SHAPE)
    }

    fn inputs_for(shape: PlanShape) -> FrozenInputs {
        FrozenInputs {
            plan: plan_for(shape),
            normalized_plan_digest: NORMALIZED_DIGEST.to_owned(),
        }
    }

    fn probed_agents() -> Vec<String> {
        vec![
            "  Codex-CLI  ".to_owned(),
            "aleph-Mid-agent".to_owned(),
            "bet-Small-agent".to_owned(),
            "aleph-Frontier-agent".to_owned(),
            "gimel-Small-agent".to_owned(),
            "gimel-Mid-agent".to_owned(),
        ]
    }

    fn run_started_unauthenticated() -> RunStarted4 {
        RunStarted4 {
            schema: TOPOLOGY_SCHEMA,
            upstroke_version: "0.2.0-census".to_owned(),
            run_id: RUN_ID.to_owned(),
            incarnation: IncarnationId("01J8ZQKB2M7NC5PQR0TVWXYZ77".to_owned()),
            runner: RunnerPolicy {
                kind: RunnerKind::Container,
                policy: RunnerContract::ContainerV1,
                image: Some(ImageIdentity {
                    reference: "ghcr.io/example/census-runner:3.4".to_owned(),
                    id: "sha256:3333333333333333333333333333333333333333333333333333333333333333"
                        .to_owned(),
                    digest: Some(
                        "sha256:4444444444444444444444444444444444444444444444444444444444444444"
                            .to_owned(),
                    ),
                }),
                credential_volumes: Some(
                    [
                        (
                            "aleph-Mid-agent".to_owned(),
                            "upstroke-creds-Ünicode".to_owned(),
                        ),
                        (
                            "  Codex-CLI  ".to_owned(),
                            "upstroke-creds-codex".to_owned(),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
            },
            probed_agents: probed_agents(),
            branch: format!("upstroke/run-{RUN_ID}"),
            integration_ref: git_ref("integration"),
            base_sha: sha("base"),
            execution_root: "/var/lib/Upstroke/census execution roots".to_owned(),
            private_dir: "/var/lib/Upstroke/census private".to_owned(),
            plan_path: "docs/Census Plan.md".to_owned(),
            config_path: None,
            plan_hash: "census-frozen-hash".to_owned(),
            normalized_plan_digest: NORMALIZED_DIGEST.to_owned(),
            registry_digest: String::new(),
            path_policy: path_policy(),
            limits: TopologyLimits {
                max_parallel: 1,
                max_defers: 2,
                max_merge_repairs: 3,
            },
            gates: vec!["fmt".to_owned()],
            gates_from_config: false,
            gate_cmds: vec![GateSummary {
                name: "fmt".to_owned(),
                cmd: "cargo fmt --check".to_owned(),
                timeout: Duration::from_secs(451),
                shell: ShellKind::Bash,
            }],
            interaction_mode: "never".to_owned(),
            chains: vec![chain("aleph"), chain("bet"), chain("gimel")],
            effort_policy: ResolvedEffortPolicy {
                small: Effort::Low,
                mid: Effort::High,
                frontier: Effort::Max,
                review: Effort::Medium,
            },
            reviews: ReviewPlan {
                enabled: Some(true),
                alternative_available: Some(false),
                pass_timeout_secs: Some(97),
                primary: Some(PassBinding::new("aleph-Mid-agent", "aleph-Mid-model")),
                alternative: None,
                second_opinion: vec![None, None, None],
            },
        }
    }

    fn run_started() -> RunStarted4 {
        run_started_for(MAIN_SHAPE)
    }

    fn run_started_for(shape: PlanShape) -> RunStarted4 {
        let started = run_started_unauthenticated();
        let digest = TaskRegistry::originals_with_agents(
            &plan_for(shape),
            &started.registry_record(),
            &started.probed_agents,
        )
        .expect("the fixture derives a registry")
        .digest();
        RunStarted4 {
            registry_digest: digest,
            ..started
        }
    }

    fn ev(body: TopologyEventBody) -> TopologyEvent {
        TopologyEvent {
            ts: "2026-08-17T19:04:11Z".to_owned(),
            body,
        }
    }

    fn started() -> TopologyFold {
        started_for(MAIN_SHAPE)
    }

    fn started_for(shape: PlanShape) -> TopologyFold {
        let mut fold = TopologyFold::new(inputs_for(shape));
        let event = ev(TopologyEventBody::RunStarted {
            data: Box::new(run_started_for(shape)),
        });
        let delta = fold
            .plan_transition(&event)
            .expect("the fixture's run_started applies");
        fold.apply_delta(delta);
        fold
    }

    /// An original's region; a repair's is its lineage root's, and a key
    /// beyond the three originals is a repair of aleph unless the fold says
    /// otherwise (`region_of`).
    fn region(key: TaskKey) -> PathSet {
        PathSet::Prefixes {
            paths: vec![GitPath::from(match key {
                ALEPH => "src/aleph",
                BET => "src/bet",
                GIMEL => "src/gimel",
                _ => "src/aleph",
            })],
        }
    }

    fn region_of(fold: &TopologyFold, key: TaskKey) -> PathSet {
        let root = fold
            .registry()
            .and_then(|registry| registry.get(key))
            .and_then(|entry| entry.lineage)
            .map_or(key, |lineage| lineage.root);
        region(root)
    }

    fn overlap_region() -> PathSet {
        PathSet::Prefixes {
            paths: vec![GitPath::from("src/aleph"), GitPath::from("src/bet")],
        }
    }

    fn label(key: TaskKey) -> &'static str {
        match key {
            ALEPH => "aleph",
            BET => "bet",
            GIMEL => "gimel",
            TaskKey(3) => "r3",
            TaskKey(4) => "r4",
            _ => "r5",
        }
    }

    fn binding(fold: &TopologyFold, key: TaskKey, rung: usize) -> RungBinding {
        binding_of(fold, key, rung).expect("the task's ladder has this rung")
    }

    /// The rung's binding, or `None` for a ladder without rungs — a merge
    /// repair whose root's rungs all lie below the repair floor, which the
    /// registry admits only through a human binding.
    fn binding_of(fold: &TopologyFold, key: TaskKey, rung: usize) -> Option<RungBinding> {
        let registry = fold.registry().expect("started");
        let entry = registry.get(key).expect("a registered task");
        let frozen = entry.ladder.rungs.get(rung)?;
        Some(RungBinding::from_frozen(
            frozen,
            entry.ladder.effort.implementation_for(frozen.tier),
        ))
    }

    fn attempt_record(attempt: u32) -> AttemptRecord {
        AttemptRecord {
            attempt,
            tier: "mid".to_owned(),
            model: "aleph-Mid-model".to_owned(),
            pool: None,
            resumed: false,
            duration: Duration::from_millis(4_321),
            cost_usd: Some(0.75),
            reviews: vec![ReviewRecord {
                pass: "review".to_owned(),
                agent: "claude-code".to_owned(),
                model: "claude-opus-5".to_owned(),
                adapter: Some("claude-code".to_owned()),
                preflight_cli_version: None,
                effort: None,
                pool: None,
                cost_usd: None,
                outcome: ReviewPassOutcome::Passed,
            }],
            session_id: None,
            usage: None,
            failure: None,
        }
    }

    fn dispatch(key: TaskKey, generation: u32) -> TopologyEvent {
        dispatch_over(key, generation, region(key))
    }

    fn dispatch_over(key: TaskKey, generation: u32, paths: PathSet) -> TopologyEvent {
        dispatch_at(key, generation, paths, sha("base"))
    }

    fn dispatch_at(
        key: TaskKey,
        generation: u32,
        paths: PathSet,
        base: CommitSha,
    ) -> TopologyEvent {
        ev(TopologyEventBody::TaskDispatched {
            data: TaskDispatched {
                key,
                generation: GenerationId(generation),
                base_sha: base,
                worktree_path: format!("/tmp/census/{}", label(key)),
                lease: LeaseGrant::Predicted { paths },
                source_candidate: None,
            },
        })
    }

    /// A fresh attempt of `key`'s open generation. A repair's attempt records
    /// what its worktree was materialized from (the fold refuses one that
    /// does not) and an original's records nothing.
    fn attempt_started(
        fold: &TopologyFold,
        key: TaskKey,
        generation: u32,
        attempt: u32,
    ) -> TopologyEvent {
        ev(TopologyEventBody::AttemptStarted {
            data: AttemptStarted4 {
                key,
                generation: GenerationId(generation),
                attempt: AttemptNumber(attempt),
                rung: 0,
                binding: binding(fold, key, 0),
                pool: None,
                resume_session: None,
                materialization_observed: is_repair(fold, key)
                    .then_some(crate::topology::events::Materialization::Clean),
            },
        })
    }

    /// Whether the registry holds `key` as a merge repair (an entry with a
    /// lineage); a key it does not hold is no repair.
    fn is_repair(fold: &TopologyFold, key: TaskKey) -> bool {
        fold.registry()
            .and_then(|registry| registry.get(key))
            .is_some_and(|entry| entry.lineage.is_some())
    }

    fn settle(
        key: TaskKey,
        generation: u32,
        attempt: u32,
        transition: SettlementTransition,
        lease: LeaseDisposition,
    ) -> TopologyEvent {
        ev(TopologyEventBody::AttemptFinished {
            data: Box::new(AttemptFinished4 {
                key,
                generation: GenerationId(generation),
                attempt: AttemptNumber(attempt),
                record: Box::new({
                    let mut record = attempt_record(attempt);
                    record.failure = Some(crate::events::FailureRecord {
                        kind: crate::ladder::FailureKind::GateFailed,
                        origin: crate::ladder::FailureOrigin::Worker,
                        reason: "the fixture's judged failure".to_owned(),
                        detail: None,
                    });
                    record
                }),
                settlement: AttemptSettlement::Closed { transition, lease },
            }),
        })
    }

    fn candidate_of(key: TaskKey, generation: u32) -> CandidateRef {
        CandidateRef {
            key,
            generation: GenerationId(generation),
            commit_sha: sha(&format!("commit-{}-{generation}", label(key))),
            candidate_ref: git_ref(&format!("candidates/{}/{generation}", label(key))),
        }
    }

    fn candidate_prepared(key: TaskKey, generation: u32, attempt: u32) -> TopologyEvent {
        candidate_prepared_over(key, generation, attempt, region(key))
    }

    fn candidate_prepared_over(
        key: TaskKey,
        generation: u32,
        attempt: u32,
        paths: PathSet,
    ) -> TopologyEvent {
        candidate_prepared_at(
            key,
            generation,
            attempt,
            paths,
            sha("base"),
            candidate_of(key, generation).commit_sha,
        )
    }

    /// The candidate `key`'s in-flight attempt prepares over its region: an
    /// original's replaces the predicted region and a repair's widens its
    /// lineage, which is what the fold requires of each.
    fn candidate_prepared_for(
        fold: &TopologyFold,
        key: TaskKey,
        generation: u32,
        attempt: u32,
    ) -> TopologyEvent {
        let paths = region_of(fold, key);
        let root = fold
            .registry()
            .and_then(|registry| registry.get(key))
            .and_then(|entry| entry.lineage)
            .map(|lineage| lineage.root);
        let mut event = candidate_prepared_at(
            key,
            generation,
            attempt,
            paths.clone(),
            sha("base"),
            candidate_of(key, generation).commit_sha,
        );
        if let (Some(root), TopologyEventBody::CandidatePrepared { data }) = (root, &mut event.body)
        {
            data.lease_effect = CandidateLeaseEffect::WidensLineage { root, paths };
        }
        event
    }

    fn candidate_prepared_at(
        key: TaskKey,
        generation: u32,
        attempt: u32,
        paths: PathSet,
        base: CommitSha,
        commit: CommitSha,
    ) -> TopologyEvent {
        ev(TopologyEventBody::CandidatePrepared {
            data: Box::new(CandidatePrepared {
                key,
                generation: GenerationId(generation),
                attempt: Box::new(attempt_record(attempt)),
                base_sha: base.clone(),
                parent_sha: base,
                tree_sha: sha(&format!("tree-{}", label(key))),
                commit_sha: commit,
                message: format!("{}: census candidate", label(key)),
                prepared_ref: git_ref(&format!("prepared-candidate/{}", label(key))),
                candidate_ref: candidate_of(key, generation).candidate_ref,
                actual_paths: paths.clone(),
                lease_effect: CandidateLeaseEffect::ReplacesPredicted { paths },
            }),
        })
    }

    fn candidate_at(key: TaskKey, generation: u32, commit: CommitSha) -> CandidateRef {
        CandidateRef {
            commit_sha: commit,
            ..candidate_of(key, generation)
        }
    }

    fn candidate_created(key: TaskKey, generation: u32) -> TopologyEvent {
        candidate_created_of(candidate_of(key, generation))
    }

    fn candidate_created_of(candidate: CandidateRef) -> TopologyEvent {
        ev(TopologyEventBody::TaskCandidateCreated {
            data: TaskCandidateCreated { candidate },
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn merge_prepared(
        sequence: u32,
        key: TaskKey,
        generation: u32,
        disposition: PreparedDisposition,
        expected_head: CommitSha,
        proposed_sha: CommitSha,
        prepared_ref: Option<GitRef>,
        source: VerificationSource,
    ) -> TopologyEvent {
        merge_prepared_for(
            sequence,
            candidate_of(key, generation),
            disposition,
            expected_head,
            proposed_sha,
            prepared_ref,
            source,
        )
    }

    fn merge_prepared_for(
        sequence: u32,
        candidate: CandidateRef,
        disposition: PreparedDisposition,
        expected_head: CommitSha,
        proposed_sha: CommitSha,
        prepared_ref: Option<GitRef>,
        source: VerificationSource,
    ) -> TopologyEvent {
        let CandidateRef {
            key,
            generation,
            commit_sha,
            candidate_ref,
        } = candidate;
        ev(TopologyEventBody::MergePrepared {
            data: Box::new(MergePrepared {
                sequence: SequenceId(sequence),
                disposition,
                expected_head,
                proposed_sha,
                key,
                generation,
                candidate_sha: commit_sha,
                candidate_ref,
                prepared_ref,
                verification_source: source.clone(),
                verification: match &source {
                    VerificationSource::CandidatePrepared { .. } => None,
                    // A verified publication records its one review pass:
                    // the bounds' `review_passes`.
                    VerificationSource::Verification { .. } => Some(VerificationRecord {
                        verdict: VerificationVerdict::Passed,
                        gates_passed: true,
                        reviews: vec![crate::events::ReviewRecord {
                            pass: "primary".to_owned(),
                            agent: "aleph-Mid-agent".to_owned(),
                            model: "aleph-Mid-model".to_owned(),
                            adapter: None,
                            preflight_cli_version: None,
                            effort: None,
                            pool: None,
                            cost_usd: Some(0.1),
                            outcome: crate::events::ReviewPassOutcome::Passed,
                        }],
                        detail: "census verification".to_owned(),
                    }),
                },
                satisfies: vec![key],
            }),
        })
    }

    fn task_merged(
        fold: &TopologyFold,
        sequence: u32,
        key: TaskKey,
        generation: u32,
    ) -> TopologyEvent {
        let (merged_sha, satisfies) = match fold.transaction().map(|open| &open.class) {
            Some(TransactionClass::Prepared {
                proposed_sha,
                satisfies,
                ..
            }) => (proposed_sha.clone(), satisfies.clone()),
            _ => (candidate_of(key, generation).commit_sha, vec![key]),
        };
        ev(TopologyEventBody::TaskMerged {
            data: TaskMerged {
                sequence: SequenceId(sequence),
                merged_sha,
                satisfies,
                lease_release: MergeLeaseRelease::Candidate {
                    key,
                    generation: GenerationId(generation),
                },
            },
        })
    }

    fn run_finished(fold: &TopologyFold, outcome: RunOutcome) -> TopologyEvent {
        ev(TopologyEventBody::RunFinished {
            data: crate::topology::events::RunFinished4 {
                outcome,
                halted_at: fold.halted_at(),
                merged: 0,
                parked: 0,
            },
        })
    }

    /// A rejection of `key`'s candidate at `sequence`, registering the repair
    /// the production repair module derives — or nothing when the fold is
    /// not in a state the derivation accepts (a failed ancestor, a full
    /// registry), which is the generator's own bound on repairs.
    fn rejection_of(
        fold: &TopologyFold,
        key: TaskKey,
        generation: u32,
        sequence: u32,
        code_rejected: bool,
    ) -> Option<TopologyEvent> {
        let disposition = if code_rejected {
            crate::topology::events::RejectionDisposition::CodeRejected {
                verification: VerificationRecord {
                    verdict: VerificationVerdict::Rejected,
                    gates_passed: true,
                    reviews: Vec::new(),
                    detail: "census code rejection".to_owned(),
                },
            }
        } else {
            crate::topology::events::RejectionDisposition::Conflict {
                paths: region_of(fold, key),
            }
        };
        let rejected = crate::engine::topology::repair::merge_rejected(
            fold,
            &CensusIds,
            &candidate_of(key, generation),
            sha("moved-head"),
            SequenceId(sequence),
            disposition,
            region_of(fold, key),
        )
        .ok()?;
        Some(ev(TopologyEventBody::MergeRejected {
            data: Box::new(rejected),
        }))
    }

    /// A question a task raises on its own (`question_raised`), one per key.
    fn raised_question(key: TaskKey) -> TopologyEvent {
        ev(TopologyEventBody::QuestionRaised {
            data: crate::topology::events::QuestionRaised4 {
                question: crate::topology::events::FrozenQuestion {
                    id: QuestionId::from(format!("q-raised-{}", label(key)).as_str()),
                    key,
                    kind: QuestionKind::Clarify,
                    context: "  a question the task raised  ".to_owned(),
                    options: vec!["go on".to_owned(), "stop".to_owned()],
                },
            },
        })
    }

    fn answer(
        key: TaskKey,
        question: &QuestionId,
        answer: crate::topology::events::Answer4,
    ) -> TopologyEvent {
        ev(TopologyEventBody::QuestionAnswered {
            data: crate::topology::events::QuestionAnswered4 {
                key,
                question: question.clone(),
                answer,
                via: "census".to_owned(),
            },
        })
    }

    fn run_resumed(runner: RunnerPolicy) -> TopologyEvent {
        ev(TopologyEventBody::RunResumed {
            data: Box::new(crate::topology::events::RunResumed4 {
                incarnation: IncarnationId("01J8ZQKB2M7NC5PQR0TVWXYZ99".to_owned()),
                runner,
                probed_agents: probed_agents(),
                upstroke_version: "0.2.0-census".to_owned(),
            }),
        })
    }

    /// Every event class the packet's `event_payload_classes` names, offered
    /// at every state for every task the fold registers — originals and the
    /// repairs rejections registered — within the bounds: two generations,
    /// two attempts, a parked settlement or a verification park only while
    /// fewer than the bound's questions are open, a resume only while the
    /// epoch is below the bound's resumes. Refusals are offers too: every
    /// arm of `plan_transition` executes on something.
    fn classes(fold: &TopologyFold) -> Vec<Candidate> {
        let bounds = CensusBounds::default();
        let mut out = Vec::new();
        let sequence = fold.transaction().map_or_else(
            || fold.next_sequence().map_or(0, |next| next.0),
            |transaction| transaction.sequence.0,
        );
        let open_questions = fold.open_questions().map_or(0, BTreeMap::len);
        let may_ask = open_questions < usize::try_from(bounds.questions).unwrap_or(usize::MAX);
        // The sequences bound: an integration opened at the next sequence
        // beyond it is not offered; an open one is driven to its end.
        let may_integrate = fold.transaction().is_some() || sequence < bounds.sequences;
        let epoch = fold.epoch().map_or(0, |epoch| epoch.0);
        let entries: Vec<crate::topology::registry::TaskEntry> = fold
            .registry()
            .map(|registry| registry.entries().to_vec())
            .unwrap_or_default();
        // The repairs bound: a rejection or a spawn registers a repair, and
        // the fold registers as many as a run asks for.
        let repairs = entries
            .iter()
            .filter(|entry| entry.origin == crate::topology::registry::Origin::MergeRepair)
            .count();
        let may_repair = repairs < usize::try_from(bounds.repairs).unwrap_or(usize::MAX);

        out.push(Candidate::new("run_started", run_started_event()));

        for entry in &entries {
            let key = entry.key;
            let name = label(key);
            let has_rungs = binding_of(fold, key, 0).is_some();
            let released = if entry.lineage.is_some() {
                LeaseDisposition::LineageHeld
            } else {
                LeaseDisposition::PredictedReleased
            };
            let retained = if entry.lineage.is_some() {
                LeaseDisposition::LineageHeld
            } else {
                LeaseDisposition::PredictedRetained
            };
            for generation in 0..bounds.generations_per_task {
                // A dispatch is offered where the sequential run admits one:
                // the task ready (its dependencies merged) and the pipeline
                // reservable, which at `max_parallel = 1` is one open
                // generation at a time. The fold trusts its emitter here — it
                // refuses a dispatch of a task that is not Pending and
                // nothing else — so without the run's own rule the census
                // would explore interleavings no sequential run performs.
                let dispatchable = fold.ready(key) && fold.pipeline_reservable();
                let dispatch_event = match entry.lineage {
                    Some(lineage) => ev(TopologyEventBody::TaskDispatched {
                        data: TaskDispatched {
                            key,
                            generation: GenerationId(generation),
                            base_sha: sha("base"),
                            worktree_path: format!("/tmp/census/{name}"),
                            lease: LeaseGrant::InheritedLineage { root: lineage.root },
                            source_candidate: Some(candidate_of(lineage.parent, 0)),
                        },
                    }),
                    None => dispatch(key, generation),
                };
                if dispatchable {
                    out.push(Candidate::new(
                        format!("task_dispatched/{name}/g{generation}"),
                        dispatch_event,
                    ));
                }
                for attempt in 1..=bounds.attempts_per_generation {
                    if !has_rungs {
                        break;
                    }
                    out.push(Candidate::new(
                        format!("attempt_started/{name}/g{generation}/a{attempt}"),
                        attempt_started(fold, key, generation, attempt),
                    ));
                    out.push(Candidate::new(
                        format!("attempt_started/resumed/{name}/g{generation}/a{attempt}"),
                        resumed_attempt(fold, key, generation, attempt),
                    ));
                    let mut settlements = vec![
                        ("succeeded", SettlementTransition::Succeeded, retained),
                        ("retry", SettlementTransition::Retry, released),
                        (
                            "failed",
                            SettlementTransition::Failed {
                                halts_run: false,
                                reason: "census failure".to_owned(),
                            },
                            released,
                        ),
                        (
                            "halting",
                            SettlementTransition::Failed {
                                halts_run: true,
                                reason: "census halting failure".to_owned(),
                            },
                            released,
                        ),
                        (
                            "deferred",
                            SettlementTransition::Deferred {
                                // The record carries the count the fold
                                // expects next, so a second deferral is one.
                                defers: fold.task(key).map_or(1, |task| task.defers + 1),
                                reason: "census outage".to_owned(),
                            },
                            released,
                        ),
                    ];
                    if may_ask {
                        settlements.push((
                            "parked",
                            SettlementTransition::Parked {
                                question: crate::topology::events::FrozenQuestion {
                                    id: QuestionId::from(format!("q-{name}-{generation}").as_str()),
                                    key,
                                    kind: QuestionKind::Unblock,
                                    context: "  a question only a person settles  ".to_owned(),
                                    options: vec!["yes".to_owned(), "no".to_owned()],
                                },
                            },
                            released,
                        ));
                    }
                    for (tag, transition, lease) in settlements {
                        out.push(Candidate::new(
                            format!("attempt_finished/{tag}/{name}/g{generation}/a{attempt}"),
                            settle(key, generation, attempt, transition, lease),
                        ));
                    }
                    out.push(Candidate::new(
                        format!("attempt_finished/retained/{name}/g{generation}/a{attempt}"),
                        retained_settlement(key, generation, attempt),
                    ));
                    out.push(Candidate::new(
                        format!("attempt_interrupted/{name}/g{generation}/a{attempt}"),
                        ev(TopologyEventBody::AttemptInterrupted {
                            data: crate::topology::events::AttemptInterrupted4 {
                                key,
                                generation: GenerationId(generation),
                                attempt: AttemptNumber(attempt),
                                lease: released,
                                detail: "the census killed the worker".to_owned(),
                            },
                        }),
                    ));
                    out.push(Candidate::new(
                        format!("candidate_prepared/{name}/g{generation}/a{attempt}"),
                        candidate_prepared_for(fold, key, generation, attempt),
                    ));
                }
                out.push(Candidate::new(
                    format!("task_candidate_created/{name}/g{generation}"),
                    candidate_created(key, generation),
                ));
                // The close reasons, each offered for the class it is about:
                // run-ending for any open generation, a missing worktree for
                // one no attempt has started in, a discarded session for a
                // retained one. Every reason at every generation multiplies
                // the closed states by three for no new arm.
                let class = fold.task(key).and_then(|task| {
                    task.generations
                        .iter()
                        .find(|held| held.id.0 == generation)
                        .map(|held| held.class.clone())
                });
                // The run-ending close is the closure's, offered where the
                // closure performs it — a halt, a budget stop, or a derived
                // ending — with the outcome the closure's own precedence
                // selects; offered elsewhere it closes every open generation
                // at every state for no arm the closure does not already
                // execute.
                let mut reasons = Vec::new();
                if let Ok(outcome) = crate::engine::topology::closure::ending_outcome(fold) {
                    reasons.push(("run-ending", GenerationCloseReason::RunEnding { outcome }));
                }
                if matches!(class, Some(GenerationClass::OpenNoAttempt)) {
                    reasons.push(("worktree-missing", GenerationCloseReason::WorktreeMissing));
                }
                if matches!(class, Some(GenerationClass::RetainedIdle { .. })) {
                    reasons.push((
                        "discards-retained-session",
                        GenerationCloseReason::ResumeDiscardsRetainedSession,
                    ));
                }
                for (tag, reason) in reasons {
                    out.push(Candidate::new(
                        format!("generation_closed/{tag}/{name}/g{generation}"),
                        ev(TopologyEventBody::GenerationClosed {
                            data: GenerationClosed {
                                key,
                                generation: GenerationId(generation),
                                reason,
                                lease: released,
                            },
                        }),
                    ));
                }

                let candidate = candidate_of(key, generation);
                let source = VerificationSource::CandidatePrepared {
                    key,
                    generation: GenerationId(generation),
                };
                if !may_integrate {
                    continue;
                }
                out.push(Candidate::new(
                    format!("merge_prepared/fast/match/{name}/g{generation}"),
                    merge_prepared(
                        sequence,
                        key,
                        generation,
                        PreparedDisposition::Fast,
                        sha("base"),
                        candidate.commit_sha.clone(),
                        None,
                        source.clone(),
                    ),
                ));
                out.push(Candidate::new(
                    format!("merge_prepared/fast/moved-head/{name}/g{generation}"),
                    merge_prepared(
                        sequence,
                        key,
                        generation,
                        PreparedDisposition::Fast,
                        sha("moved-head"),
                        candidate.commit_sha.clone(),
                        None,
                        source.clone(),
                    ),
                ));
                out.push(Candidate::new(
                    format!("merge_prepared/fast/other-proposed/{name}/g{generation}"),
                    merge_prepared(
                        sequence,
                        key,
                        generation,
                        PreparedDisposition::Fast,
                        sha("base"),
                        sha("not-the-candidate"),
                        None,
                        source.clone(),
                    ),
                ));
                out.push(Candidate::new(
                    format!("merge_prepared/fast/with-pin/{name}/g{generation}"),
                    merge_prepared(
                        sequence,
                        key,
                        generation,
                        PreparedDisposition::Fast,
                        sha("base"),
                        candidate.commit_sha.clone(),
                        Some(git_ref(&format!("prepared/{sequence}"))),
                        source.clone(),
                    ),
                ));
                out.push(Candidate::new(
                    format!("merge_prepared/fast/with-verification/{name}/g{generation}"),
                    ev(TopologyEventBody::MergePrepared {
                        data: Box::new(MergePrepared {
                            sequence: SequenceId(sequence),
                            disposition: PreparedDisposition::Fast,
                            expected_head: sha("base"),
                            proposed_sha: candidate.commit_sha.clone(),
                            key,
                            generation: GenerationId(generation),
                            candidate_sha: candidate.commit_sha.clone(),
                            candidate_ref: candidate.candidate_ref.clone(),
                            prepared_ref: None,
                            verification_source: source.clone(),
                            verification: Some(VerificationRecord {
                                verdict: VerificationVerdict::Passed,
                                gates_passed: true,
                                reviews: vec![crate::events::ReviewRecord {
                                    pass: "primary".to_owned(),
                                    agent: "aleph-Mid-agent".to_owned(),
                                    model: "aleph-Mid-model".to_owned(),
                                    adapter: None,
                                    preflight_cli_version: None,
                                    effort: None,
                                    pool: None,
                                    cost_usd: Some(0.1),
                                    outcome: crate::events::ReviewPassOutcome::Passed,
                                }],
                                detail: "census verification".to_owned(),
                            }),
                            satisfies: vec![key],
                        }),
                    }),
                ));
                out.push(Candidate::new(
                    format!("merge_verification_started/stale/{name}/g{generation}"),
                    ev(TopologyEventBody::MergeVerificationStarted {
                        data: MergeVerificationStarted {
                            sequence: SequenceId(sequence),
                            candidate: candidate.clone(),
                            basis: VerificationBasis::StaleClean {
                                prepared_ref: git_ref(&format!("prepared/{sequence}")),
                            },
                            expected_head: sha("moved-head"),
                            proposed_sha: sha(&format!("proposal-{name}")),
                        },
                    }),
                ));
                out.push(Candidate::new(
                    format!("merge_verification_started/present/{name}/g{generation}"),
                    ev(TopologyEventBody::MergeVerificationStarted {
                        data: MergeVerificationStarted {
                            sequence: SequenceId(sequence),
                            candidate: candidate.clone(),
                            basis: VerificationBasis::AlreadyPresent,
                            expected_head: candidate.commit_sha.clone(),
                            proposed_sha: candidate.commit_sha.clone(),
                        },
                    }),
                ));
                let verified = VerificationSource::Verification {
                    sequence: SequenceId(sequence),
                };
                out.push(Candidate::new(
                    format!("merge_prepared/stale_clean/match/{name}/g{generation}"),
                    merge_prepared(
                        sequence,
                        key,
                        generation,
                        PreparedDisposition::StaleClean,
                        sha("moved-head"),
                        sha(&format!("proposal-{name}")),
                        Some(git_ref(&format!("prepared/{sequence}"))),
                        verified.clone(),
                    ),
                ));
                out.push(Candidate::new(
                    format!("merge_prepared/stale_clean/mismatch/{name}/g{generation}"),
                    merge_prepared(
                        sequence,
                        key,
                        generation,
                        PreparedDisposition::StaleClean,
                        sha("moved-head"),
                        sha("not-the-pinned-proposal"),
                        Some(git_ref(&format!("prepared/{sequence}"))),
                        verified.clone(),
                    ),
                ));
                out.push(Candidate::new(
                    format!("merge_prepared/already_present/match/{name}/g{generation}"),
                    merge_prepared(
                        sequence,
                        key,
                        generation,
                        PreparedDisposition::AlreadyPresent,
                        candidate.commit_sha.clone(),
                        candidate.commit_sha.clone(),
                        None,
                        verified.clone(),
                    ),
                ));
                out.push(Candidate::new(
                    format!("merge_prepared/already_present/mismatch/{name}/g{generation}"),
                    merge_prepared(
                        sequence,
                        key,
                        generation,
                        PreparedDisposition::AlreadyPresent,
                        candidate.commit_sha.clone(),
                        sha("not-the-head"),
                        None,
                        verified.clone(),
                    ),
                ));
                out.push(Candidate::new(
                    format!("task_merged/{name}/g{generation}"),
                    task_merged(fold, sequence, key, generation),
                ));
                // A rejection is of a queued candidate with no transaction
                // open, or of the candidate an open verification is about,
                // and the fold refuses any other; the derivation is the
                // costly part of an offer, so it is made where the fold can
                // accept it.
                let rejectable = match fold.transaction() {
                    None => fold.task_state(key) == Some(TaskState::AwaitingMerge),
                    Some(transaction) => {
                        transaction.candidate.key == key
                            && transaction.candidate.generation.0 == generation
                    }
                };
                for (tag, code_rejected) in [("conflict", false), ("code-rejected", true)] {
                    if !may_repair || !rejectable {
                        break;
                    }
                    if let Some(rejection) =
                        rejection_of(fold, key, generation, sequence, code_rejected)
                    {
                        out.push(Candidate::new(
                            format!("merge_rejected/{tag}/{name}/g{generation}"),
                            rejection,
                        ));
                    }
                }
                // The same spawn a person could make by hand, offered where
                // a person would: over merged work, once, before anything
                // else is dispatched. The fold registers a spawn at almost
                // any state, and a spawn at every state copies the space per
                // repair.
                let spawnable = may_repair
                    && generation == 0
                    && repairs == 0
                    && fold.transaction().is_none()
                    && fold.task_state(key) == Some(TaskState::Merged)
                    && entries.iter().all(|other| {
                        other.key == key
                            || fold
                                .task(other.key)
                                .is_some_and(|task| task.generations.is_empty())
                    });
                if spawnable {
                    if let Some(TopologyEvent {
                        body: TopologyEventBody::MergeRejected { data },
                        ..
                    }) = rejection_of(fold, key, generation, sequence, true)
                    {
                        out.push(Candidate::new(
                            format!("task_spawned/{name}/g{generation}"),
                            ev(TopologyEventBody::TaskSpawned {
                                data: Box::new(crate::topology::events::TaskSpawned {
                                    spawn: data.repair.clone(),
                                }),
                            }),
                        ));
                    }
                }
            }
            // One task raises questions of its own; a raised question for
            // every task would multiply every other state by the eight
            // combinations of three, and the questions bound is reached
            // through the parks the settlements and verifications record.
            // The fold parks a lineage's task only with nothing of the
            // lineage in flight or under integration; a candidate awaiting
            // its merge is where a task raises one on its own.
            let awaiting = fold.task_state(key) == Some(TaskState::AwaitingMerge)
                && fold.transaction().is_none();
            if may_ask && key == ALEPH && awaiting {
                out.push(Candidate::new(
                    format!("question_raised/{name}"),
                    raised_question(key),
                ));
            }
        }

        for defers in 1..=bounds.defers {
            out.push(Candidate::new(
                format!("merge_verification_unavailable/deferred/d{defers}"),
                verification_deferred_by_outage(sequence, defers),
            ));
        }
        if may_ask {
            out.push(Candidate::new(
                "merge_verification_unavailable/parked",
                verification_parked(sequence, ALEPH, "q-verification-park"),
            ));
        }
        out.push(Candidate::new(
            "merge_verification_interrupted",
            ev(TopologyEventBody::MergeVerificationInterrupted {
                data: crate::topology::events::MergeVerificationInterrupted {
                    sequence: SequenceId(sequence),
                    detail: "the census killed the verifier".to_owned(),
                },
            }),
        ));
        if let Some(questions) = fold.open_questions() {
            for (id, open) in questions {
                let key = open.question.key;
                out.push(Candidate::new(
                    format!("question_answered/answered/{id}"),
                    answer(
                        key,
                        id,
                        crate::topology::events::Answer4::Answered {
                            option_index: 0,
                            binding_override: None,
                        },
                    ),
                ));
                for halts in [false, true] {
                    out.push(Candidate::new(
                        format!("question_answered/declined/halts-{halts}/{id}"),
                        answer(
                            key,
                            id,
                            crate::topology::events::Answer4::Declined {
                                decline_halts_run: halts,
                            },
                        ),
                    ));
                }
            }
        }

        out.push(Candidate::new(
            "defer_wait_elapsed",
            ev(TopologyEventBody::DeferWaitElapsed {
                data: DeferWaitElapsed4 {
                    waited_ms: 30_000,
                    round: 1,
                },
            }),
        ));
        // The ceiling is consulted where the loop selects — with no
        // generation open and the run not over — and that is where the
        // sequential loop appends `budget_exceeded`; offered in flight it
        // doubles every attempt state for the same arm.
        if fold.pipeline_reservable() && fold.finished().is_none() {
            out.push(Candidate::new(
                "budget_exceeded",
                ev(TopologyEventBody::BudgetExceeded {
                    data: BudgetExceeded4 {
                        epoch: fold.epoch().unwrap_or(crate::topology::events::Epoch(0)),
                        budget: BudgetKind::Run,
                        limit_usd: 12.5,
                        spent_usd: 12.75,
                        key: Some(ALEPH),
                    },
                }),
            ));
        }
        for outcome in [
            RunOutcome::Complete,
            RunOutcome::Parked,
            RunOutcome::Halted,
            RunOutcome::BudgetExceeded,
        ] {
            out.push(Candidate::new(
                format!("run_finished/{outcome:?}"),
                run_finished(fold, outcome),
            ));
        }
        // A resume is offered where a run resumes: after a Parked or
        // budget-stopped end (the reopening resume) and over a retained
        // session (the resume that retries or discards it). Accepted
        // mid-run at every state it would copy the whole space once per
        // epoch; the classification of every mid-run state as a resume
        // action is the classifier tests' claim, over the same states.
        let resumes_here = matches!(
            fold.finished(),
            Some(RunOutcome::Parked | RunOutcome::BudgetExceeded)
        ) || entries.iter().any(|entry| {
            fold.task(entry.key).is_some_and(|task| {
                task.generations.iter().any(|generation| {
                    matches!(generation.class, GenerationClass::RetainedIdle { .. })
                })
            })
        });
        if epoch < bounds.resumes && resumes_here {
            out.push(Candidate::new(
                "run_resumed/identical",
                run_resumed(run_started_unauthenticated().runner),
            ));
        }
        let mut moved = run_started_unauthenticated().runner;
        moved.kind = RunnerKind::Host;
        out.push(Candidate::new(
            "run_resumed/moved-runner",
            run_resumed(moved),
        ));
        out.push(Candidate::new(
            "capacity_snapshot",
            ev(TopologyEventBody::CapacitySnapshot {
                data: crate::events::CapacitySnapshot {
                    strategy: "census".to_owned(),
                    pools: Vec::new(),
                },
            }),
        ));
        out.push(Candidate::new(
            "pool_exhausted",
            ev(TopologyEventBody::PoolExhausted {
                data: crate::events::PoolExhausted {
                    pool: "census-pool".to_owned(),
                    agent: "aleph-Mid-agent".to_owned(),
                    reset_at: None,
                    detail: "the census pool is exhausted".to_owned(),
                },
            }),
        ));
        out.push(Candidate::new(
            "design_defect",
            ev(TopologyEventBody::DesignDefect {
                data: crate::events::DesignDefect {
                    question: QuestionId::from("q-census-defect"),
                    context: "a census defect".to_owned(),
                    answer: "noted".to_owned(),
                },
            }),
        ));
        out
    }

    fn run_started_event() -> TopologyEvent {
        run_started_event_for(MAIN_SHAPE)
    }

    fn run_started_event_for(shape: PlanShape) -> TopologyEvent {
        ev(TopologyEventBody::RunStarted {
            data: Box::new(run_started_for(shape)),
        })
    }

    /// The classes of the integration path alone: what the deep census
    /// explores from a seed where two originals are merged, so that four
    /// sequences and two repairs are a few steps away rather than forty.
    fn integration_path_classes(fold: &TopologyFold) -> Vec<Candidate> {
        classes(fold)
            .into_iter()
            .filter(|candidate| {
                [
                    "task_dispatched/",
                    "attempt_started/",
                    "candidate_prepared/",
                    "task_candidate_created/",
                    "merge_prepared/fast/match/",
                    "task_merged/",
                    "merge_rejected/conflict/",
                    "run_finished/",
                ]
                .iter()
                .any(|prefix| candidate.label.starts_with(prefix))
                    && !candidate.label.starts_with("attempt_started/resumed/")
            })
            .collect()
    }

    /// A prefix with aleph and bet merged (sequences 0 and 1) and gimel's
    /// candidate created, applied event by event.
    fn two_merged_prefix() -> (TopologyFold, Vec<TopologyEvent>) {
        let mut fold = started();
        let mut trace = vec![run_started_event()];
        let apply =
            |fold: &mut TopologyFold, trace: &mut Vec<TopologyEvent>, event: TopologyEvent| {
                let delta = fold
                    .plan_transition(&event)
                    .unwrap_or_else(|error| panic!("the deep seed applies: {error}"));
                fold.apply_delta(delta);
                trace.push(event);
            };
        for (key, sequence) in [(ALEPH, 0), (BET, 1)] {
            let events = vec![
                dispatch(key, 0),
                attempt_started(&fold, key, 0, 1),
                candidate_prepared(key, 0, 1),
                candidate_created(key, 0),
            ];
            for event in events {
                apply(&mut fold, &mut trace, event);
            }
            let prepared = merge_prepared(
                sequence,
                key,
                0,
                PreparedDisposition::Fast,
                sha("base"),
                candidate_of(key, 0).commit_sha,
                None,
                VerificationSource::CandidatePrepared {
                    key,
                    generation: GenerationId(0),
                },
            );
            apply(&mut fold, &mut trace, prepared);
            let merged = task_merged(&fold, sequence, key, 0);
            apply(&mut fold, &mut trace, merged);
        }
        for event in [
            dispatch(GIMEL, 0),
            attempt_started(&fold, GIMEL, 0, 1),
            candidate_prepared(GIMEL, 0, 1),
            candidate_created(GIMEL, 0),
        ] {
            apply(&mut fold, &mut trace, event);
        }
        (fold, trace)
    }

    /// The deep census: from [`two_merged_prefix`], the integration path
    /// alone — gimel's candidate rejected and repaired, the repair rejected
    /// and repaired again — explored to closure, where the fourth sequence
    /// and the second repair are.
    fn deep_census() -> &'static Census {
        static DEEP: OnceLock<Census> = OnceLock::new();
        DEEP.get_or_init(|| {
            let (fold, trace) = two_merged_prefix();
            Census::explore(
                fold,
                trace,
                CensusBounds {
                    max_states: 5_000,
                    ..CensusBounds::default()
                },
                integration_path_classes,
            )
        })
    }

    /// What each census reached in every declared dimension, from its states
    /// and traces: the largest value any state holds.
    fn reached_dimensions(censuses: &[&Census]) -> BTreeMap<&'static str, u32> {
        let mut reached: BTreeMap<&'static str, u32> = CensusBounds::default()
            .dimensions()
            .iter()
            .map(|(name, _)| (*name, 0))
            .collect();
        let mut note = |name: &'static str, value: u32| {
            let held = reached.entry(name).or_insert(0);
            *held = (*held).max(value);
        };
        for census in censuses {
            for state in census.states() {
                let fold = &state.fold;
                if let Some(registry) = fold.registry() {
                    let originals = registry
                        .entries()
                        .iter()
                        .filter(|entry| entry.origin == crate::topology::registry::Origin::Original)
                        .count();
                    let repairs = registry.entries().len() - originals;
                    note("originals", u32::try_from(originals).unwrap_or(u32::MAX));
                    note("repairs", u32::try_from(repairs).unwrap_or(u32::MAX));
                    for entry in registry.entries() {
                        if let Some(task) = fold.task(entry.key) {
                            note(
                                "generations_per_task",
                                u32::try_from(task.generations.len()).unwrap_or(u32::MAX),
                            );
                            note("defers", task.defers);
                            for generation in &task.generations {
                                note("attempts_per_generation", generation.attempts);
                            }
                        }
                    }
                }
                note(
                    "sequences",
                    fold.next_sequence().map_or(0, |sequence| sequence.0),
                );
                if let Some(queue) = fold.queue() {
                    for entry in queue.entries() {
                        note("defers", entry.defers);
                    }
                }
                note(
                    "questions",
                    u32::try_from(fold.open_questions().map_or(0, BTreeMap::len))
                        .unwrap_or(u32::MAX),
                );
                note("resumes", fold.epoch().map_or(0, |epoch| epoch.0));
                for event in &state.trace {
                    if let TopologyEventBody::MergePrepared { data } = &event.body {
                        if let Some(verification) = &data.verification {
                            note(
                                "review_passes",
                                u32::try_from(verification.reviews.len()).unwrap_or(u32::MAX),
                            );
                        }
                    }
                }
            }
        }
        reached
    }

    /// The shared census: the fan-out plan under the packet's bounds, explored
    /// breadth-first to `CensusBounds::default().max_states` states. The
    /// bounded space is larger than that ceiling by orders of magnitude, so
    /// the census stops there and says so (`truncated`); every assertion over
    /// it is over the explored set, and the bounds the breadth-first prefix
    /// cannot reach — four integration sequences, two repairs — are reached by
    /// [`deep_census`].
    fn census() -> &'static Census {
        static CENSUS: OnceLock<Census> = OnceLock::new();
        CENSUS.get_or_init(|| {
            Census::explore(
                started(),
                vec![run_started_event()],
                CensusBounds::default(),
                classes,
            )
        })
    }

    /// Every key the fold registers: the originals and the repairs.
    fn every_key(fold: &TopologyFold) -> Vec<TaskKey> {
        fold.registry()
            .map(|registry| registry.entries().iter().map(|entry| entry.key).collect())
            .unwrap_or_default()
    }

    fn common(fold: &TopologyFold) -> bool {
        let no_open_generation = every_key(fold).iter().all(|key| {
            fold.task(*key).is_none_or(|task| {
                task.generations
                    .iter()
                    .all(|generation| generation.class == GenerationClass::Closed)
            })
        });
        no_open_generation && fold.transaction().is_none()
    }

    fn backoff_pending(fold: &TopologyFold) -> bool {
        let deferred_task = every_key(fold)
            .iter()
            .any(|key| fold.task_state(*key) == Some(TaskState::Deferred));
        let deferred_candidate = fold.queue().is_some_and(|queue| {
            queue
                .entries()
                .iter()
                .any(|entry| entry.verification_deferred)
        });
        deferred_task || deferred_candidate
    }

    fn questions_open(fold: &TopologyFold) -> bool {
        fold.open_questions()
            .is_some_and(|questions| !questions.is_empty())
    }

    /// Whether `key` can never run: one of its dependencies, transitively,
    /// has failed, so the task stays Pending and the run completes around it.
    fn blocked(fold: &TopologyFold, key: TaskKey) -> bool {
        fold.registry()
            .and_then(|registry| registry.get(key))
            .is_some_and(|entry| {
                entry.deps.iter().any(|dep| {
                    fold.task_state(*dep) == Some(TaskState::Failed) || blocked(fold, *dep)
                })
            })
    }

    fn complete_shape(fold: &TopologyFold) -> bool {
        let every_task_terminal = every_key(fold).iter().all(|key| {
            matches!(
                fold.task_state(*key),
                Some(TaskState::Merged | TaskState::Failed)
            ) || (fold.task_state(*key) == Some(TaskState::Pending) && blocked(fold, *key))
        });
        let queue_empty = fold.queue().is_none_or(|queue| queue.is_empty());
        let no_lease = fold
            .leases()
            .is_none_or(|leases| !leases.any_candidate_or_lineage());
        every_task_terminal && queue_empty && no_lease && !questions_open(fold)
    }

    #[test]
    fn the_derived_outcome_is_total_over_every_explored_state() {
        let census = census();
        assert!(!census.states().is_empty());
        let audit = census.totality_audit();

        let reached: BTreeSet<usize> =
            std::iter::once(0)
                .chain(census.transitions().iter().filter_map(
                    |transition| match transition.outcome {
                        TransitionOutcome::Accepted { to } => Some(to),
                        TransitionOutcome::Refused { .. } | TransitionOutcome::Truncated => None,
                    },
                ))
                .collect();
        assert_eq!(
            audit.evaluated,
            (0..census.states().len()).collect::<Vec<_>>(),
            "one evaluation per explored state, in order, and no more"
        );
        assert_eq!(
            audit.evaluated.iter().copied().collect::<BTreeSet<_>>(),
            reached,
            "the states that were evaluated and the states the transitions reach are not the \
             same set"
        );

        assert!(
            audit.fold_errors.is_empty(),
            "the arm the design argues is unreachable was reached at states {:?}, the first after \
             {:?}",
            audit.fold_errors,
            audit.fold_errors.first().map(|id| census.states()[*id]
                .trace
                .iter()
                .map(|event| event.body.kind())
                .collect::<Vec<_>>())
        );
        assert!(
            audit.disagreements.is_empty(),
            "the recorded outcome and a fresh evaluation of the same fold disagree at {:?}",
            audit.disagreements
        );
        assert_eq!(
            audit.not_ending + audit.ending,
            census.states().len(),
            "every explored state answered exactly one of the two"
        );
        let (not_ending, ending) = (audit.not_ending, audit.ending);
        assert!(not_ending > 0 && ending > 0, "{not_ending}/{ending}");

        for state in census.states() {
            let fold = &state.fold;
            let common = common(fold);
            let halting = fold.halted_at().is_some();
            let budget = fold
                .budget_stop()
                .is_some_and(|stop| Some(stop.epoch) == fold.epoch());
            if !common {
                assert_eq!(
                    state.outcome,
                    DerivedOutcome::NotEnding,
                    "state {}: a run with open work is not ending",
                    state.id
                );
            } else if halting {
                assert_eq!(
                    state.outcome,
                    DerivedOutcome::Ending(RunOutcome::Halted),
                    "state {}: halt outranks everything",
                    state.id
                );
            } else if budget {
                assert_eq!(
                    state.outcome,
                    DerivedOutcome::Ending(RunOutcome::BudgetExceeded),
                    "state {}: budget outranks parked and complete",
                    state.id
                );
            } else if backoff_pending(fold) {
                assert_eq!(
                    state.outcome,
                    DerivedOutcome::NotEnding,
                    "state {}: pending backoff blocks Parked and Complete",
                    state.id
                );
            } else if complete_shape(fold) {
                assert_eq!(
                    state.outcome,
                    DerivedOutcome::Ending(RunOutcome::Complete),
                    "state {}: nothing is open and nothing is asked",
                    state.id
                );
            }
            let kinds: Vec<&str> = state.trace.iter().map(|event| event.body.kind()).collect();
            match &state.outcome {
                DerivedOutcome::Ending(RunOutcome::Parked) => {
                    assert!(
                        questions_open(fold),
                        "state {}: parked with no question open: {kinds:?}",
                        state.id
                    );
                    assert!(!backoff_pending(fold), "state {}: {kinds:?}", state.id);
                    assert!(common, "state {}: {kinds:?}", state.id);
                }
                DerivedOutcome::Ending(RunOutcome::Complete) => {
                    assert!(
                        !questions_open(fold),
                        "state {}: complete with a question open: {kinds:?}",
                        state.id
                    );
                    assert!(complete_shape(fold), "state {}: {kinds:?}", state.id);
                }
                DerivedOutcome::Ending(RunOutcome::Halted) => {
                    assert!(halting, "state {}: {kinds:?}", state.id);
                }
                DerivedOutcome::Ending(RunOutcome::BudgetExceeded) => {
                    assert!(budget && !halting, "state {}: {kinds:?}", state.id);
                }
                DerivedOutcome::NotEnding | DerivedOutcome::FoldError => {}
            }
        }
    }

    #[test]
    fn a_state_with_admissible_work_and_no_budget_exceeded_classifies_not_ending() {
        let census = census();
        let mut before = 0;
        let mut after = 0;
        for state in census.states() {
            let fold = &state.fold;
            let has_record = fold.budget_stop().is_some();
            if !has_record && fold.halted_at().is_none() {
                assert_ne!(
                    state.outcome,
                    DerivedOutcome::Ending(RunOutcome::BudgetExceeded),
                    "state {}: a run that recorded no budget_exceeded cannot end for budget",
                    state.id
                );
            }
            let admissible_work = every_key(fold).iter().any(|key| {
                fold.task(*key).is_some_and(|task| {
                    task.generations
                        .iter()
                        .any(|generation| generation.class != GenerationClass::Closed)
                })
            });
            if admissible_work && !has_record {
                assert_eq!(
                    state.outcome,
                    DerivedOutcome::NotEnding,
                    "state {}",
                    state.id
                );
                before += 1;
            }
            if has_record && fold.halted_at().is_none() && common(fold) {
                assert_eq!(
                    state.outcome,
                    DerivedOutcome::Ending(RunOutcome::BudgetExceeded),
                    "state {}: once common holds, the record decides",
                    state.id
                );
                after += 1;
            }
        }
        assert!(before > 0, "no pre-budget_exceeded prefix was explored");
        assert!(after > 0, "no post-budget_exceeded state was explored");
    }

    #[test]
    fn every_deferred_state_has_a_legal_next_transition() {
        let census = census();
        let mut deferred_states = 0;
        let mut at_ceiling = 0;
        let mut below_ceiling = 0;
        let mut ceiling_wakes = 0;
        let mut ceiling_closes = 0;
        for state in census.states() {
            if !backoff_pending(&state.fold) || state.fold.finished().is_some() {
                continue;
            }
            deferred_states += 1;
            let at_the_ceiling = state.trace.len() >= census.bounds().max_trace;
            let accepted: BTreeSet<String> = classes(&state.fold)
                .into_iter()
                .filter(|candidate| state.fold.plan_transition(&candidate.event).is_ok())
                .map(|candidate| candidate.label)
                .collect();
            if at_the_ceiling {
                at_ceiling += 1;
                assert_eq!(
                    census.outgoing(state.id).count(),
                    0,
                    "state {} sits at the trace ceiling and was extended anyway",
                    state.id
                );
            } else {
                below_ceiling += 1;
                let recorded: BTreeSet<String> = census
                    .outgoing(state.id)
                    .filter(|transition| {
                        matches!(
                            transition.outcome,
                            TransitionOutcome::Accepted { .. } | TransitionOutcome::Truncated
                        )
                    })
                    .map(|transition| transition.label.to_string())
                    .collect();
                assert_eq!(
                    recorded, accepted,
                    "state {}: the recorded offers and the fold disagree about what is accepted \
                     here",
                    state.id
                );
                assert_eq!(
                    census.has_legal_transition(state.id),
                    !accepted.is_empty(),
                    "state {}: the accessor and the fold disagree about whether anything is \
                     accepted here",
                    state.id
                );
            }
            assert!(
                !accepted.is_empty(),
                "state {} has a deferred item and no way out: {:?}",
                state.id,
                state
                    .trace
                    .iter()
                    .map(|event| event.body.kind())
                    .collect::<Vec<_>>()
            );
            let halting = state.fold.halted_at().is_some();
            let stopped = state.fold.budget_stop().is_some();
            if !halting && !stopped {
                assert!(
                    accepted.contains("defer_wait_elapsed"),
                    "state {}: an unhalted, unstopped backoff wakes: {accepted:?}",
                    state.id
                );
                ceiling_wakes += usize::from(at_the_ceiling);
            } else {
                assert!(
                    !accepted.contains("defer_wait_elapsed"),
                    "state {}: halt and budget outrank backoff: {accepted:?}",
                    state.id
                );
                assert!(
                    accepted.iter().any(|label| {
                        [
                            "attempt_finished/",
                            "attempt_interrupted",
                            "candidate_prepared/",
                            "generation_closed/",
                            "task_candidate_created/",
                            "merge_prepared/",
                            "task_merged/",
                            "run_finished/",
                        ]
                        .iter()
                        .any(|closure| label.starts_with(closure))
                    }),
                    "state {}: a halted or stopped backoff closes: {accepted:?}\n  \
                     outcome={:?} halted={:?} stop={:?}\n  trace={:?}",
                    state.id,
                    state.outcome,
                    state.fold.halted_at(),
                    state.fold.budget_stop(),
                    state
                        .trace
                        .iter()
                        .map(|e| e.body.kind())
                        .collect::<Vec<_>>(),
                );
                ceiling_closes += usize::from(at_the_ceiling);
            }
        }
        assert!(deferred_states > 0, "no deferred state was explored");
        assert!(
            below_ceiling > 0,
            "every deferred state sat at the trace ceiling, so the recorded table was never \
             cross-checked against the fold"
        );
        assert_eq!(
            (at_ceiling, ceiling_wakes, ceiling_closes),
            (0, 0, 0),
            "the shared census stops at its state ceiling long before its trace ceiling; the \
             trace ceiling's own behaviour is `a_census_that_hits_its_ceiling_says_so`'s"
        );

        assert!(
            census.states().iter().any(|state| {
                state.fold.queue().is_some_and(|queue| {
                    queue
                        .entries()
                        .iter()
                        .any(|entry| entry.verification_deferred)
                })
            }),
            "the generator's verification deferrals reach a verification-deferred candidate in \
             the shared census; its way out is asserted above like every other deferred state's"
        );
    }

    fn deferral_classes(fold: &TopologyFold) -> Vec<Candidate> {
        let mut out = overlap_classes(fold);
        out.retain(|candidate| !candidate.label.starts_with("candidate_prepared/region-ab"));
        out.push(Candidate::new(
            "merge_verification_unavailable/deferred",
            verification_deferred_by_outage(0, 1),
        ));
        out.push(Candidate::new(
            "defer_wait_elapsed",
            ev(TopologyEventBody::DeferWaitElapsed {
                data: DeferWaitElapsed4 {
                    waited_ms: 30_000,
                    round: 1,
                },
            }),
        ));
        out
    }

    #[test]
    fn a_verification_deferred_candidate_is_a_deferred_state_with_a_way_out() {
        let census = Census::explore(
            started_for(PlanShape::Join),
            vec![run_started_event_for(PlanShape::Join)],
            CensusBounds::default(),
            deferral_classes,
        );
        assert!(!census.truncated());
        let deferred: Vec<&CensusState> = census
            .states()
            .iter()
            .filter(|state| {
                state.fold.queue().is_some_and(|queue| {
                    queue
                        .entries()
                        .iter()
                        .any(|entry| entry.verification_deferred)
                })
            })
            .collect();
        assert!(
            !deferred.is_empty(),
            "no candidate was verification-deferred"
        );
        for state in &deferred {
            assert!(state.trace.len() < census.bounds().max_trace);
            assert!(state.fold.halted_at().is_none());
            assert!(state.fold.budget_stop().is_none());
            assert!(
                census.has_legal_transition(state.id),
                "state {} defers a verification and has no way out",
                state.id
            );
            let accepted: BTreeSet<&str> = census
                .outgoing(state.id)
                .filter(|transition| {
                    matches!(transition.outcome, TransitionOutcome::Accepted { .. })
                })
                .map(|transition| &*transition.label)
                .collect();
            assert!(
                accepted.contains("defer_wait_elapsed"),
                "state {}: {accepted:?}",
                state.id
            );
            assert!(
                !accepted.contains("merge_verification_started/aleph/g0"),
                "state {}: a deferred candidate was re-offered for verification",
                state.id
            );
            assert_eq!(
                state.outcome,
                DerivedOutcome::NotEnding,
                "state {}",
                state.id
            );
        }
    }

    #[test]
    fn the_publication_relations_are_exercised_in_both_directions() {
        let census = census();
        let accepted = census.accepted_labels();
        let refused = census.refused_labels();

        for matching in [
            "merge_prepared/fast/match/aleph/g0",
            "merge_prepared/stale_clean/match/aleph/g0",
            "merge_prepared/already_present/match/aleph/g0",
        ] {
            assert!(
                accepted.contains(matching),
                "`{matching}` was never accepted: {:?}",
                accepted
                    .iter()
                    .filter(|label| label.starts_with("merge_prepared/"))
                    .collect::<Vec<_>>()
            );
        }
        for mismatching in [
            "merge_prepared/fast/moved-head/aleph/g0",
            "merge_prepared/fast/other-proposed/aleph/g0",
            "merge_prepared/fast/with-pin/aleph/g0",
            "merge_prepared/fast/with-verification/aleph/g0",
            "merge_prepared/stale_clean/mismatch/aleph/g0",
            "merge_prepared/already_present/mismatch/aleph/g0",
        ] {
            assert!(
                refused.contains(mismatching),
                "`{mismatching}` was never refused"
            );
            assert!(
                !accepted.contains(mismatching),
                "`{mismatching}` was accepted somewhere, and it names a relation the fold must refuse"
            );
        }
        assert!(census.transitions().iter().any(|transition| {
            &*transition.label == "merge_prepared/fast/with-pin/aleph/g0"
                && matches!(transition.outcome, TransitionOutcome::Refused { .. })
        }));
    }

    #[test]
    fn no_offer_is_unmapped_and_every_class_is_offered_everywhere() {
        let census = census();
        let at_root = classes(&started()).len();
        assert!(at_root > 100, "{at_root} classes is a thin census");
        let offered: usize = census
            .states()
            .iter()
            .filter(|state| state.trace.len() < census.bounds().max_trace)
            .map(|state| classes(&state.fold).len())
            .sum();
        assert_eq!(
            census.transitions().len(),
            offered,
            "an offer produced neither an acceptance, a refusal nor a truncation"
        );
        let mut truncated = 0usize;
        for transition in census.transitions() {
            match &transition.outcome {
                TransitionOutcome::Accepted { to } => assert!(*to < census.states().len()),
                TransitionOutcome::Refused { reason } => {
                    assert!(!reason.is_empty(), "{}", transition.label);
                }
                TransitionOutcome::Truncated => truncated += 1,
            }
        }
        assert!(!census.accepted_labels().is_empty());
        assert!(!census.refused_labels().is_empty());
        assert_eq!(
            census.truncated(),
            truncated > 0,
            "a truncated offer is what the census reports as its truncation, and nothing else is"
        );
        assert!(
            census.truncated(),
            "the bounded space is larger than the state ceiling; a census that closed under it \
             would mean the generator lost its classes"
        );
        assert_eq!(
            census.states().len(),
            census.bounds().max_states,
            "the census explored exactly to its state ceiling"
        );
    }
    #[test]
    fn replaying_every_explored_trace_reaches_the_state_it_was_explored_at() {
        let census = census();
        for state in census.states() {
            let replayed = TopologyFold::replay(inputs(), &state.trace)
                .unwrap_or_else(|error| panic!("state {} does not replay: {error}", state.id));
            assert!(
                replayed.state() == state.fold.state(),
                "state {} replays to a different state",
                state.id
            );
            assert_eq!(
                replayed.derived_outcome(),
                state.outcome,
                "state {} classifies differently live and on replay",
                state.id
            );
            let again = TopologyFold::replay(inputs(), &state.trace).expect("replays again");
            assert!(again.state() == replayed.state(), "state {}", state.id);
        }
    }

    #[test]
    fn the_census_reaches_every_outcome_and_says_what_it_did_not_reach() {
        let census = census();
        let reached: BTreeSet<String> = census
            .states()
            .iter()
            .filter_map(|state| match &state.outcome {
                DerivedOutcome::Ending(outcome) => Some(format!("{outcome:?}")),
                _ => None,
            })
            .collect();
        for outcome in ["Complete", "Halted", "BudgetExceeded", "Parked"] {
            assert!(
                reached.contains(outcome),
                "{outcome} unreached: {reached:?}"
            );
        }
        let mut compared = 0;
        for state in census.states() {
            if state.fold.finished().is_some() {
                for outcome in [
                    RunOutcome::Complete,
                    RunOutcome::Parked,
                    RunOutcome::Halted,
                    RunOutcome::BudgetExceeded,
                ] {
                    let event = run_finished(&state.fold, outcome.clone());
                    assert!(
                        state.fold.plan_transition(&event).is_err(),
                        "state {}: a run ends once",
                        state.id
                    );
                }
                continue;
            }
            for outcome in [
                RunOutcome::Complete,
                RunOutcome::Parked,
                RunOutcome::Halted,
                RunOutcome::BudgetExceeded,
            ] {
                let event = run_finished(&state.fold, outcome.clone());
                let accepted = state.fold.plan_transition(&event).is_ok();
                assert_eq!(
                    accepted,
                    state.outcome == DerivedOutcome::Ending(outcome.clone()),
                    "state {}: run_finished({outcome:?}) against {:?}",
                    state.id,
                    state.outcome
                );
                compared += 1;
            }
        }
        assert!(compared > 100, "only {compared} guards were compared");
    }

    #[test]
    fn the_census_runs_at_the_packets_bounds_and_says_where_it_stopped() {
        let bounds = CensusBounds::default();
        assert_eq!(bounds.originals, 3);
        assert_eq!(bounds.repairs, 2);
        assert_eq!(bounds.generations_per_task, 2);
        assert_eq!(bounds.attempts_per_generation, 2);
        assert_eq!(bounds.sequences, 4);
        assert_eq!(bounds.defers, 2);
        assert_eq!(bounds.questions, 2);
        assert_eq!(bounds.review_passes, 1);
        assert_eq!(bounds.resumes, 2);
        assert_eq!(bounds.max_states, 20_000);

        let census = census();
        let registry = started().registry().expect("started").len();
        assert_eq!(
            registry, 3,
            "the fixture plan is the bound's three originals"
        );
        assert_eq!(MAIN_SHAPE, PlanShape::FanOut);
        assert_eq!(
            started().registry().expect("started").entries()[2].deps,
            vec![ALEPH],
            "gimel fans out from aleph"
        );
        assert!(
            census
                .accepted_labels()
                .iter()
                .any(|label| label.starts_with("merge_rejected/")),
            "the census registers repairs through rejections"
        );
        assert!(
            census
                .accepted_labels()
                .iter()
                .any(|label| label.starts_with("task_dispatched/r3")),
            "and dispatches one"
        );
        assert!(
            census.states().iter().any(|state| {
                state
                    .fold
                    .leases()
                    .is_some_and(|leases| !leases.lineages().is_empty())
            }),
            "a lineage lease is held somewhere in the explored set"
        );
        assert!(
            census.truncated() && census.states().len() == bounds.max_states,
            "the space under these bounds does not close under the state ceiling, and the \
             census says so rather than reading as complete"
        );
    }
    #[test]
    fn the_fixture_varies_every_field_a_relation_reads() {
        let started = run_started();
        let limits = BTreeSet::from([
            started.limits.max_parallel,
            started.limits.max_defers,
            started.limits.max_merge_repairs,
        ]);
        assert_eq!(limits.len(), 3, "a fold reading one limit for another");
        let efforts = BTreeSet::from([
            format!("{:?}", started.effort_policy.small),
            format!("{:?}", started.effort_policy.mid),
            format!("{:?}", started.effort_policy.frontier),
            format!("{:?}", started.effort_policy.review),
        ]);
        assert_eq!(efforts.len(), 4);
        assert_ne!(started.chains[0].tiers.len(), started.chains[1].tiers.len());
        assert_ne!(
            started.chains[0].attempts_per,
            started.chains[1].attempts_per
        );
        assert_ne!(region(ALEPH), region(BET));
        let shas = BTreeSet::from([
            sha("base"),
            candidate_of(ALEPH, 0).commit_sha,
            candidate_of(BET, 0).commit_sha,
            sha("moved-head"),
            sha("proposal-aleph"),
            sha("proposal-bet"),
            sha("not-the-candidate"),
            sha("not-the-pinned-proposal"),
            sha("not-the-head"),
            sha("tree-aleph"),
        ]);
        assert_eq!(shas.len(), 10, "two roles share a literal");
        assert_ne!(started.registry_digest, String::new());
        assert_ne!(started.registry_digest, started.normalized_plan_digest);
    }

    #[test]
    fn a_census_that_hits_its_ceiling_says_so() {
        let tight = CensusBounds {
            max_states: 3,
            ..CensusBounds::default()
        };
        let stopped = Census::explore(started(), vec![run_started_event()], tight, classes);
        assert!(stopped.truncated());
        assert!(stopped.states().len() <= 3);
        assert!(
            census().truncated(),
            "the shared census stops at its state ceiling"
        );

        let shallow = CensusBounds {
            max_trace: 2,
            ..CensusBounds::default()
        };
        let shallow = Census::explore(started(), vec![run_started_event()], shallow, classes);
        assert!(
            shallow.truncated(),
            "the trace ceiling stopped states with legal continuations, and the census says so"
        );
        assert!(shallow.states().len() > 1);
        assert_eq!(
            shallow.transitions().len(),
            classes(&started()).len(),
            "only the root was extended"
        );
        for state in shallow.states().iter().skip(1) {
            assert_eq!(state.trace.len(), 2);
            assert_eq!(
                shallow.outgoing(state.id).count(),
                0,
                "state {} sits at the trace ceiling and was extended anyway",
                state.id
            );
        }

        let closed = Census::explore(
            started(),
            vec![run_started_event()],
            CensusBounds::default(),
            dispatch_once_then_dead,
        );
        assert!(
            !closed.truncated(),
            "a space that closes under both ceilings is not reported truncated"
        );
    }
    #[test]
    fn a_transaction_class_is_reachable_and_blocks_the_run_from_ending() {
        let census = census();
        let with_transaction: Vec<&CensusState> = census
            .states()
            .iter()
            .filter(|state| state.fold.transaction().is_some())
            .collect();
        assert!(
            !with_transaction.is_empty(),
            "no state held an unresolved transaction"
        );
        for state in &with_transaction {
            assert_eq!(
                state.outcome,
                DerivedOutcome::NotEnding,
                "state {}",
                state.id
            );
        }
        let classes_seen: BTreeSet<&'static str> = with_transaction
            .iter()
            .map(|state| {
                match state
                    .fold
                    .transaction()
                    .map(|transaction| &transaction.class)
                {
                    Some(TransactionClass::VerificationStarted { .. }) => "verification",
                    Some(TransactionClass::Prepared { .. }) => "prepared",
                    None => "none",
                }
            })
            .collect();
        assert_eq!(
            classes_seen,
            BTreeSet::from(["verification", "prepared"]),
            "{classes_seen:?}"
        );
    }

    fn state_at(id: usize, fold: TopologyFold, outcome: DerivedOutcome) -> CensusState {
        CensusState {
            id,
            trace: Vec::new(),
            fold,
            outcome,
        }
    }

    fn fold_with(outcome: &DerivedOutcome) -> TopologyFold {
        census()
            .states_with(outcome)
            .first()
            .unwrap_or_else(|| panic!("no census state is {outcome:?}"))
            .fold
            .clone()
    }

    #[test]
    fn the_totality_audit_reports_a_fold_error_a_normalisation_and_a_short_domain() {
        let ending = fold_with(&DerivedOutcome::Ending(RunOutcome::Complete));
        let not_ending = started();

        let sentinel = vec![
            state_at(0, not_ending.clone(), DerivedOutcome::NotEnding),
            state_at(1, ending.clone(), DerivedOutcome::FoldError),
        ];
        let audit = TotalityAudit::over(&sentinel);
        assert_eq!(audit.fold_errors, vec![1]);
        assert_eq!(audit.evaluated, vec![0, 1]);

        let normalised = vec![state_at(0, ending.clone(), DerivedOutcome::NotEnding)];
        let audit = TotalityAudit::over(&normalised);
        assert_eq!(audit.disagreements, vec![0]);
        assert!(audit.fold_errors.is_empty());
        assert_eq!((audit.not_ending, audit.ending), (0, 1));

        let short = vec![
            state_at(0, not_ending.clone(), DerivedOutcome::NotEnding),
            state_at(2, not_ending.clone(), DerivedOutcome::NotEnding),
        ];
        let audit = TotalityAudit::over(&short);
        assert_eq!(audit.evaluated, vec![0, 2]);
        assert_ne!(audit.evaluated, vec![0, 1]);

        let clean = vec![
            state_at(0, not_ending, DerivedOutcome::NotEnding),
            state_at(1, ending, DerivedOutcome::Ending(RunOutcome::Complete)),
        ];
        let audit = TotalityAudit::over(&clean);
        assert!(audit.fold_errors.is_empty() && audit.disagreements.is_empty());
        assert_eq!((audit.not_ending, audit.ending), (1, 1));
    }

    #[test]
    fn the_census_transition_table_is_reproducible_from_the_folds_alone() {
        let census = census();
        let mut rows = 0usize;
        for state in census.states() {
            let recorded: Vec<&CensusTransition> = census.outgoing(state.id).collect();
            if state.trace.len() >= census.bounds().max_trace {
                assert!(
                    recorded.is_empty(),
                    "state {} sits at the trace ceiling and was extended anyway",
                    state.id
                );
                assert!(
                    !census.has_legal_transition(state.id),
                    "state {} was never extended and reports a transition",
                    state.id
                );
                continue;
            }
            let offers = classes(&state.fold);
            assert_eq!(
                recorded.len(),
                offers.len(),
                "state {} recorded {} answers for {} offers",
                state.id,
                recorded.len(),
                offers.len()
            );
            let mut any_accepted = false;
            for (offer, row) in offers.iter().zip(&recorded) {
                assert_eq!(row.from, state.id);
                assert_eq!(&*row.label, offer.label, "state {}", state.id);
                match (state.fold.plan_transition(&offer.event), &row.outcome) {
                    (Err(error), TransitionOutcome::Refused { reason }) => {
                        assert_eq!(&**reason, error.to_string(), "state {}", state.id);
                    }
                    (Ok(_), TransitionOutcome::Truncated) => {
                        any_accepted = true;
                        assert!(
                            census.truncated(),
                            "state {}: a truncated offer in a census that does not say so",
                            state.id
                        );
                    }
                    (Ok(delta), TransitionOutcome::Accepted { to }) => {
                        any_accepted = true;
                        let mut next = state.fold.clone();
                        next.apply_delta(delta);
                        let landed = &census.states()[*to];
                        assert_eq!(
                            fingerprint(&next),
                            fingerprint(&landed.fold),
                            "state {} --{}--> {to} is not the state applying it reaches",
                            state.id,
                            offer.label
                        );
                        assert_eq!(
                            landed.outcome,
                            next.derived_outcome(),
                            "state {to} was recorded with an outcome its own fold does not give"
                        );
                    }
                    (Ok(_), answer) => panic!(
                        "state {}: the fold accepts `{}` and the census recorded {answer:?}",
                        state.id, offer.label
                    ),
                    (Err(error), answer) => panic!(
                        "state {}: the fold refuses `{}` with `{error}` and the census recorded \
                         {answer:?}",
                        state.id, offer.label
                    ),
                }
                rows += 1;
            }
            assert_eq!(
                census.has_legal_transition(state.id),
                any_accepted,
                "state {}",
                state.id
            );
        }
        assert_eq!(
            rows,
            census.transitions().len(),
            "the census holds a row no offer produced"
        );
    }

    #[test]
    fn the_seed_state_is_evaluated_rather_than_assumed_not_ending() {
        let ended = census()
            .states()
            .iter()
            .find(|state| {
                state.fold.finished().is_some()
                    && state.outcome == DerivedOutcome::Ending(RunOutcome::Complete)
            })
            .expect("the census reaches a completed run");
        let bounds = CensusBounds {
            max_trace: 0,
            ..CensusBounds::default()
        };
        let seeded = Census::explore(ended.fold.clone(), ended.trace.clone(), bounds, classes);
        assert_eq!(seeded.states().len(), 1, "nothing was extended");
        assert!(seeded.transitions().is_empty());
        assert!(
            !seeded.truncated(),
            "a completed run has no legal continuation for the zero trace ceiling to stop"
        );
        assert_eq!(
            seeded.states()[0].outcome,
            DerivedOutcome::Ending(RunOutcome::Complete),
            "the seed was assumed rather than evaluated"
        );
        let audit = seeded.totality_audit();
        assert_eq!(audit.evaluated, vec![0]);
        assert!(audit.disagreements.is_empty() && audit.fold_errors.is_empty());
        assert_eq!((audit.not_ending, audit.ending), (0, 1));
    }

    fn unresolvable_merge() -> TopologyEvent {
        ev(TopologyEventBody::TaskMerged {
            data: TaskMerged {
                sequence: SequenceId(0),
                merged_sha: sha("base"),
                satisfies: vec![ALEPH],
                lease_release: MergeLeaseRelease::Candidate {
                    key: ALEPH,
                    generation: GenerationId(0),
                },
            },
        })
    }

    fn only_refused(_: &TopologyFold) -> Vec<Candidate> {
        vec![Candidate::new(
            "task_merged/no-transaction",
            unresolvable_merge(),
        )]
    }

    fn dispatch_once_then_dead(fold: &TopologyFold) -> Vec<Candidate> {
        let mut out = only_refused(fold);
        if fold
            .task(ALEPH)
            .is_none_or(|task| task.generations.is_empty())
        {
            out.push(Candidate::new(
                "task_dispatched/aleph/g0",
                dispatch(ALEPH, 0),
            ));
        }
        out
    }

    #[test]
    fn has_legal_transition_is_local_to_the_state_and_excludes_refusals() {
        let refusals = Census::explore(
            started(),
            vec![run_started_event()],
            CensusBounds::default(),
            only_refused,
        );
        assert_eq!(refusals.states().len(), 1);
        assert_eq!(refusals.transitions().len(), 1);
        assert!(matches!(
            refusals.transitions()[0].outcome,
            TransitionOutcome::Refused { .. }
        ));
        assert!(
            !refusals.has_legal_transition(0),
            "every offer at this state was refused"
        );

        let mixed = Census::explore(
            started(),
            vec![run_started_event()],
            CensusBounds::default(),
            dispatch_once_then_dead,
        );
        assert_eq!(mixed.states().len(), 2, "one live state and one dead one");
        assert!(mixed.has_legal_transition(0), "the root dispatches");
        assert!(
            !mixed.has_legal_transition(1),
            "the dispatched state has no accepted offer of its own"
        );
        assert_eq!(mixed.outgoing(1).count(), 1);
        assert!(!mixed.has_legal_transition(2));
        assert!(!mixed.has_legal_transition(usize::MAX));
    }

    fn verification_started(
        sequence: u32,
        key: TaskKey,
        generation: u32,
        pin: &str,
        expected_head: CommitSha,
        proposed_sha: CommitSha,
    ) -> TopologyEvent {
        ev(TopologyEventBody::MergeVerificationStarted {
            data: MergeVerificationStarted {
                sequence: SequenceId(sequence),
                candidate: candidate_of(key, generation),
                basis: VerificationBasis::StaleClean {
                    prepared_ref: git_ref(pin),
                },
                expected_head,
                proposed_sha,
            },
        })
    }

    fn verification_parked(sequence: u32, key: TaskKey, id: &str) -> TopologyEvent {
        ev(TopologyEventBody::MergeVerificationUnavailable {
            data: MergeVerificationUnavailable {
                sequence: SequenceId(sequence),
                cause: UnavailableCause::HumanRequired {
                    verdict: "  a reviewer found something only a person decides  ".to_owned(),
                },
                outcome: UnavailableOutcome::Parked {
                    question: crate::topology::events::FrozenQuestion {
                        id: QuestionId::from(id),
                        key,
                        kind: QuestionKind::Unblock,
                        context: "  the verification could not run  ".to_owned(),
                        options: vec!["retry".to_owned(), "abandon".to_owned()],
                    },
                },
                reviews: Vec::new(),
            },
        })
    }

    fn verification_deferred_by_outage(sequence: u32, defers: u32) -> TopologyEvent {
        ev(TopologyEventBody::MergeVerificationUnavailable {
            data: MergeVerificationUnavailable {
                sequence: SequenceId(sequence),
                cause: UnavailableCause::Infrastructure {
                    kind: InfrastructureKind::RateLimited,
                },
                outcome: UnavailableOutcome::Deferred { defers },
                reviews: Vec::new(),
            },
        })
    }

    fn queued_candidate_trace(paths: PathSet) -> Vec<TopologyEvent> {
        let mut fold = started();
        let mut trace = vec![run_started_event()];
        for event in [
            dispatch(ALEPH, 0),
            attempt_started(&fold, ALEPH, 0, 1),
            candidate_prepared_over(ALEPH, 0, 1, paths.clone()),
            candidate_created(ALEPH, 0),
        ] {
            let delta = fold
                .plan_transition(&event)
                .unwrap_or_else(|error| panic!("the shared prefix applies: {error}"));
            fold.apply_delta(delta);
            trace.push(event);
        }
        trace
    }

    fn queued_candidate_at(base: CommitSha, commit: CommitSha) -> Vec<TopologyEvent> {
        let mut fold = started();
        let mut trace = vec![run_started_event()];
        for event in [
            dispatch_at(ALEPH, 0, region(ALEPH), base.clone()),
            attempt_started(&fold, ALEPH, 0, 1),
            candidate_prepared_at(ALEPH, 0, 1, region(ALEPH), base, commit.clone()),
            candidate_created_of(candidate_at(ALEPH, 0, commit)),
        ] {
            let delta = fold
                .plan_transition(&event)
                .unwrap_or_else(|error| panic!("a candidate-side prefix applies: {error}"));
            fold.apply_delta(delta);
            trace.push(event);
        }
        trace
    }

    fn fast_publication(base: CommitSha, commit: CommitSha) -> TopologyEvent {
        merge_prepared_for(
            0,
            candidate_at(ALEPH, 0, commit.clone()),
            PreparedDisposition::Fast,
            base,
            commit,
            None,
            VerificationSource::CandidatePrepared {
                key: ALEPH,
                generation: GenerationId(0),
            },
        )
    }

    fn prepared_record(fold: &TopologyFold) -> PreparedCandidate {
        fold.task(ALEPH)
            .and_then(|task| task.generations.first())
            .and_then(|generation| generation.candidate.clone())
            .expect("a candidate-side witness leg prepares a candidate")
    }

    fn replayed(trace: &[TopologyEvent]) -> TopologyFold {
        TopologyFold::replay(inputs(), trace)
            .unwrap_or_else(|error| panic!("a witness trace does not replay: {error}"))
    }

    enum WitnessShape {
        OneField { from: String, to: String },
        OneLabel { from: String, to: String },
        OneRegion,
        OneAppend,
        Reordered,
    }

    enum RecordedOperand {
        Base,
        Commit,
    }

    impl RecordedOperand {
        fn copied(
            &self,
            from: &PreparedCandidate,
            mut into: PreparedCandidate,
        ) -> PreparedCandidate {
            match self {
                Self::Base => into.base_sha = from.base_sha.clone(),
                Self::Commit => into.candidate.commit_sha = from.candidate.commit_sha.clone(),
            }
            into
        }
    }

    struct RelationWitness {
        relation: &'static str,
        left: Vec<TopologyEvent>,
        right: Vec<TopologyEvent>,
        shape: WitnessShape,
        opposed: Option<(TopologyEvent, TopologyEvent)>,
        recorded: Option<RecordedOperand>,
    }

    fn abstraction_witnesses() -> Vec<RelationWitness> {
        let base = queued_candidate_trace(region(ALEPH));
        let verification = |pin: &str, head: CommitSha, proposed: CommitSha| {
            let mut trace = base.clone();
            trace.push(verification_started(0, ALEPH, 0, pin, head, proposed));
            trace
        };
        let deferred = {
            let mut trace = verification("prepared/0", sha("moved-head"), sha("proposal-aleph"));
            trace.push(verification_deferred_by_outage(0, 1));
            trace
        };
        let mut woken = deferred.clone();
        woken.push(ev(TopologyEventBody::DeferWaitElapsed {
            data: DeferWaitElapsed4 {
                waited_ms: 30_000,
                round: 1,
            },
        }));

        let mut aleph_first = vec![run_started_event()];
        let mut bet_first = vec![run_started_event()];
        for key in [ALEPH, BET] {
            let mut fold = started();
            let mut leg = Vec::new();
            for event in [
                dispatch(key, 0),
                attempt_started(&fold, key, 0, 1),
                candidate_prepared(key, 0, 1),
                candidate_created(key, 0),
            ] {
                if let Ok(delta) = fold.plan_transition(&event) {
                    fold.apply_delta(delta);
                }
                leg.push(event);
            }
            if key == ALEPH {
                aleph_first.splice(1..1, leg.clone());
                bet_first.extend(leg);
            } else {
                aleph_first.extend(leg.clone());
                bet_first.splice(1..1, leg);
            }
        }

        let shared_commit = candidate_of(ALEPH, 0).commit_sha;
        let base_a = sha("candidate-base-a");
        let base_b = sha("candidate-base-b");
        let commit_one = sha("candidate-commit-one");
        let commit_two = sha("candidate-commit-two");

        vec![
            RelationWitness {
                relation: "the region a candidate's lease holds (A versus AB)",
                left: base.clone(),
                right: queued_candidate_trace(overlap_region()),
                shape: WitnessShape::OneRegion,
                opposed: None,
                recorded: None,
            },
            RelationWitness {
                relation: "merge_prepared: expected_head",
                left: verification("prepared/0", sha("moved-head"), sha("proposal-aleph")),
                right: verification("prepared/0", sha("other-head"), sha("proposal-aleph")),
                shape: WitnessShape::OneField {
                    from: sha("moved-head").0,
                    to: sha("other-head").0,
                },
                opposed: None,
                recorded: None,
            },
            RelationWitness {
                relation: "merge_prepared: proposed_sha",
                left: verification("prepared/0", sha("moved-head"), sha("proposal-aleph")),
                right: verification("prepared/0", sha("moved-head"), sha("other-proposal")),
                shape: WitnessShape::OneField {
                    from: sha("proposal-aleph").0,
                    to: sha("other-proposal").0,
                },
                opposed: None,
                recorded: None,
            },
            RelationWitness {
                relation: "merge_prepared: the pinned proposal ref",
                left: verification("prepared/0", sha("moved-head"), sha("proposal-aleph")),
                right: verification("prepared/9", sha("moved-head"), sha("proposal-aleph")),
                shape: WitnessShape::OneField {
                    from: git_ref("prepared/0").0,
                    to: git_ref("prepared/9").0,
                },
                opposed: None,
                recorded: None,
            },
            RelationWitness {
                relation: "verification_deferred on a queued candidate",
                left: deferred,
                right: woken,
                shape: WitnessShape::OneAppend,
                opposed: None,
                recorded: None,
            },
            RelationWitness {
                relation: "the queue's order",
                left: aleph_first,
                right: bet_first,
                shape: WitnessShape::Reordered,
                opposed: None,
                recorded: None,
            },
            RelationWitness {
                relation: "merge_prepared: the candidate's own base label",
                left: queued_candidate_at(base_a.clone(), shared_commit.clone()),
                right: queued_candidate_at(base_b.clone(), shared_commit.clone()),
                shape: WitnessShape::OneLabel {
                    from: base_a.0.clone(),
                    to: base_b.0.clone(),
                },
                opposed: Some((
                    fast_publication(base_a, shared_commit.clone()),
                    fast_publication(base_b, shared_commit),
                )),
                recorded: Some(RecordedOperand::Base),
            },
            RelationWitness {
                relation: "merge_prepared: the candidate's own commit label",
                left: queued_candidate_at(sha("base"), commit_one.clone()),
                right: queued_candidate_at(sha("base"), commit_two.clone()),
                shape: WitnessShape::OneLabel {
                    from: commit_one.0.clone(),
                    to: commit_two.0.clone(),
                },
                opposed: Some((
                    fast_publication(sha("base"), commit_one),
                    fast_publication(sha("base"), commit_two),
                )),
                recorded: Some(RecordedOperand::Commit),
            },
        ]
    }

    #[test]
    fn the_abstraction_key_separates_states_that_differ_in_one_retained_relation() {
        let witnesses = abstraction_witnesses();
        assert!(witnesses.len() >= 8);
        for witness in &witnesses {
            let left = replayed(&witness.left);
            let right = replayed(&witness.right);
            let name = witness.relation;

            match &witness.shape {
                WitnessShape::OneField { from, to } => {
                    assert_eq!(witness.left.len(), witness.right.len(), "{name}");
                    let differing: Vec<usize> = (0..witness.left.len())
                        .filter(|index| witness.left[*index] != witness.right[*index])
                        .collect();
                    assert_eq!(differing.len(), 1, "{name}: not one event");
                    let index = differing[0];
                    let before = format!("{:?}", witness.left[index].body);
                    let after = format!("{:?}", witness.right[index].body);
                    assert_ne!(before, after, "{name}");
                    assert_eq!(
                        before.replace(from.as_str(), to.as_str()),
                        after,
                        "{name}: more than one field moved"
                    );
                }
                WitnessShape::OneLabel { from, to } => {
                    assert_eq!(witness.left.len(), witness.right.len(), "{name}");
                    let differing = (0..witness.left.len())
                        .filter(|index| witness.left[*index] != witness.right[*index])
                        .count();
                    assert!(
                        differing > 1,
                        "{name}: one event moved, so `OneField` is the honest shape and the \
                         stricter check"
                    );
                    let rendered = |trace: &[TopologyEvent]| {
                        trace
                            .iter()
                            .map(|event| format!("{:?}", event.body))
                            .collect::<Vec<_>>()
                            .join("\n")
                    };
                    assert_eq!(
                        rendered(&witness.left).replace(from.as_str(), to.as_str()),
                        rendered(&witness.right),
                        "{name}: more than one label moved"
                    );
                }
                WitnessShape::OneRegion => {
                    assert_eq!(witness.left.len(), witness.right.len(), "{name}");
                    let differing = (0..witness.left.len())
                        .filter(|index| witness.left[*index] != witness.right[*index])
                        .count();
                    assert_eq!(differing, 1, "{name}: not one event");
                }
                WitnessShape::OneAppend => {
                    assert_eq!(witness.right.len(), witness.left.len() + 1, "{name}");
                    assert_eq!(
                        witness.right[..witness.left.len()],
                        witness.left[..],
                        "{name}"
                    );
                }
                WitnessShape::Reordered => {
                    assert_ne!(witness.left, witness.right, "{name}");
                    let sorted = |trace: &[TopologyEvent]| {
                        let mut rendered: Vec<String> = trace
                            .iter()
                            .map(|event| format!("{:?}", event.body))
                            .collect();
                        rendered.sort();
                        rendered
                    };
                    assert_eq!(sorted(&witness.left), sorted(&witness.right), "{name}");
                }
            }

            if let Some(operand) = &witness.recorded {
                let (kept_left, kept_right) = (prepared_record(&left), prepared_record(&right));
                assert_ne!(
                    kept_left, kept_right,
                    "{name}: the two legs record the same candidate"
                );
                assert_eq!(
                    operand.copied(&kept_left, kept_right),
                    kept_left,
                    "{name}: more than one recorded field moved"
                );
            }

            assert!(
                left.state() != right.state(),
                "{name}: the witness pair is one state, so it witnesses nothing"
            );
            assert_ne!(
                fingerprint(&left),
                fingerprint(&right),
                "the key does not read {name}"
            );

            if let Some((for_left, for_right)) = &witness.opposed {
                assert!(
                    left.plan_transition(for_left).is_ok(),
                    "{name}: the publication built from the left leg's own labels is refused \
                     there: {:?}",
                    left.plan_transition(for_left).err()
                );
                assert!(
                    right.plan_transition(for_left).is_err(),
                    "{name}: the left leg's publication is accepted at the right leg too, so the \
                     two states answer it alike"
                );
                assert!(
                    right.plan_transition(for_right).is_ok(),
                    "{name}: the publication built from the right leg's own labels is refused \
                     there: {:?}",
                    right.plan_transition(for_right).err()
                );
                assert!(
                    left.plan_transition(for_right).is_err(),
                    "{name}: the right leg's publication is accepted at the left leg too, so the \
                     two states answer it alike"
                );
            }
        }
        let named: BTreeSet<&str> = witnesses.iter().map(|witness| witness.relation).collect();
        assert_eq!(named.len(), witnesses.len());
        assert_eq!(
            witnesses
                .iter()
                .filter(|witness| witness.opposed.is_some() && witness.recorded.is_some())
                .count(),
            2,
            "the candidate's base and the candidate's commit each owe an opposed publication"
        );
    }

    fn overlap_classes(fold: &TopologyFold) -> Vec<Candidate> {
        vec![
            Candidate::new("task_dispatched/aleph/g0", dispatch(ALEPH, 0)),
            Candidate::new(
                "attempt_started/aleph/g0/a1",
                attempt_started(fold, ALEPH, 0, 1),
            ),
            Candidate::new(
                "attempt_finished/succeeded/aleph/g0/a1",
                settle(
                    ALEPH,
                    0,
                    1,
                    SettlementTransition::Succeeded,
                    LeaseDisposition::PredictedRetained,
                ),
            ),
            Candidate::new(
                "candidate_prepared/region-a/aleph/g0/a1",
                candidate_prepared_over(ALEPH, 0, 1, region(ALEPH)),
            ),
            Candidate::new(
                "candidate_prepared/region-ab/aleph/g0/a1",
                candidate_prepared_over(ALEPH, 0, 1, overlap_region()),
            ),
            Candidate::new(
                "task_candidate_created/aleph/g0",
                candidate_created(ALEPH, 0),
            ),
            Candidate::new(
                "merge_verification_started/aleph/g0",
                verification_started(
                    0,
                    ALEPH,
                    0,
                    "prepared/0",
                    sha("moved-head"),
                    sha("proposal-aleph"),
                ),
            ),
            Candidate::new(
                "merge_verification_unavailable/parked",
                verification_parked(0, ALEPH, "q-overlap-park"),
            ),
            Candidate::new(
                "run_finished/Parked",
                run_finished(fold, RunOutcome::Parked),
            ),
        ]
    }

    #[test]
    fn an_overlapping_region_is_explored_and_changes_a_transition_answer() {
        // The join shape: aleph and bet independent, so region A leaves bet
        // dispatchable while aleph is parked and region AB does not.
        let census = Census::explore(
            started_for(PlanShape::Join),
            vec![run_started_event_for(PlanShape::Join)],
            CensusBounds::default(),
            overlap_classes,
        );
        assert!(!census.truncated());

        let parked: Vec<&CensusState> = census
            .states()
            .iter()
            .filter(|state| {
                state
                    .fold
                    .open_questions()
                    .is_some_and(|open| !open.is_empty())
                    && state.fold.transaction().is_none()
                    && state.fold.finished().is_none()
            })
            .collect();
        assert_eq!(
            parked.len(),
            2,
            "A and AB reached {} parked state(s), not two",
            parked.len()
        );

        let holds_bet = |state: &CensusState| {
            state.trace.iter().any(|event| match &event.body {
                TopologyEventBody::CandidatePrepared { data } => {
                    data.actual_paths == overlap_region()
                }
                _ => false,
            })
        };
        let wide = parked
            .iter()
            .find(|state| holds_bet(state))
            .expect("one parked state took region AB");
        let narrow = parked
            .iter()
            .find(|state| !holds_bet(state))
            .expect("one parked state took region A");

        assert_eq!(wide.trace.len(), narrow.trace.len());
        let differing: Vec<usize> = (0..wide.trace.len())
            .filter(|index| wide.trace[*index] != narrow.trace[*index])
            .collect();
        assert_eq!(differing, vec![3], "more than the region moved");

        assert_ne!(wide.id, narrow.id);
        assert_eq!(narrow.outcome, DerivedOutcome::NotEnding);
        assert_eq!(wide.outcome, DerivedOutcome::Ending(RunOutcome::Parked));
        let answer = |state: &CensusState| {
            census
                .outgoing(state.id)
                .find(|transition| &*transition.label == "run_finished/Parked")
                .map(|transition| matches!(transition.outcome, TransitionOutcome::Accepted { .. }))
                .unwrap_or_else(|| panic!("state {} never offered run_finished", state.id))
        };
        assert!(!answer(narrow), "region A leaves bet dispatchable");
        assert!(answer(wide), "region AB blocks bet and the run parks");
        assert_ne!(region(ALEPH), overlap_region());
        assert_ne!(region(BET), overlap_region());
    }

    /// The arms of the production dispatch, read from the source of
    /// `check_started_run` (`src/topology/fold/start.rs`): every
    /// `TopologyEventBody::Variant` the match names, mapped to its wire kind
    /// through the `kind()` table in `src/topology/events.rs`.
    fn production_arms() -> BTreeSet<&'static str> {
        let start = include_str!("fold/start.rs");
        let body = start
            .split("fn check_started_run(")
            .nth(1)
            .expect("the production dispatch is defined there");
        let dispatch = body
            .split("let derived = match &event.body {")
            .nth(1)
            .expect("the dispatch matches on the event body");
        let arms: BTreeSet<&str> = dispatch
            .split("TopologyEventBody::")
            .skip(1)
            .map(|rest| {
                rest.split(|ch: char| !ch.is_ascii_alphanumeric())
                    .next()
                    .expect("a variant name")
            })
            .collect();
        let events = include_str!("events.rs");
        let table = events
            .split("pub fn kind(&self) -> &'static str {")
            .nth(1)
            .and_then(|rest| rest.split_once("\n    }\n"))
            .map(|(body, _)| body)
            .expect("the kind table");
        let kinds: BTreeMap<&str, &str> = table
            .split("Self::")
            .skip(1)
            .filter_map(|rest| {
                let variant = rest.split(|ch: char| !ch.is_ascii_alphanumeric()).next()?;
                let kind = rest.split('"').nth(1)?;
                Some((variant, kind))
            })
            .collect();
        arms.iter()
            .map(|variant| {
                *kinds
                    .get(variant)
                    .unwrap_or_else(|| panic!("`{variant}` has no kind in the wire table"))
            })
            .collect()
    }

    /// `coverage_assertions[0]`: every `plan_transition` arm executed at
    /// least once — the arms enumerated from the production source, the
    /// executions from the census's transitions, whose kinds the explorer
    /// records as it offers. Both ways: an arm no offer reached fails, and
    /// an offered kind the dispatch has no arm for fails.
    #[test]
    fn every_plan_transition_arm_is_executed_by_the_census() {
        let arms = production_arms();
        assert_eq!(arms.len(), 24, "{arms:?}");
        let census = census();
        let executed: BTreeSet<&'static str> = census
            .transitions()
            .iter()
            .map(|transition| transition.kind)
            .collect();
        let accepted: BTreeSet<&'static str> = census
            .transitions()
            .iter()
            .filter(|transition| {
                matches!(
                    transition.outcome,
                    TransitionOutcome::Accepted { .. } | TransitionOutcome::Truncated
                )
            })
            .map(|transition| transition.kind)
            .collect();
        assert_eq!(
            executed, arms,
            "the arms the census executed and the arms the dispatch has"
        );
        let mut never_accepted: Vec<&str> = arms.difference(&accepted).copied().collect();
        never_accepted.sort_unstable();
        assert_eq!(
            never_accepted,
            vec!["run_started"],
            "every arm but the started run's refusal is executed by an acceptance too"
        );
    }

    /// The packet's plan shapes: the chain and the join, explored under the
    /// same generator to a smaller ceiling, reach every outcome, never the
    /// fold's error arm, and say where they stopped; the fan-out is the
    /// shared census.
    #[test]
    fn every_plan_shape_is_explored() {
        for shape in [PlanShape::Chain, PlanShape::Join] {
            let census = Census::explore(
                started_for(shape),
                vec![run_started_event_for(shape)],
                CensusBounds {
                    max_states: 3_000,
                    ..CensusBounds::default()
                },
                classes,
            );
            assert!(
                census.truncated(),
                "{}: stops at its state ceiling",
                shape.name()
            );
            let audit = census.totality_audit();
            assert!(audit.fold_errors.is_empty(), "{}", shape.name());
            let reached: BTreeSet<String> = census
                .states()
                .iter()
                .filter_map(|state| match &state.outcome {
                    DerivedOutcome::Ending(outcome) => Some(format!("{outcome:?}")),
                    _ => None,
                })
                .collect();
            for outcome in ["Complete", "Halted", "BudgetExceeded", "Parked"] {
                assert!(
                    reached.contains(outcome),
                    "{}: {outcome} unreached: {reached:?}",
                    shape.name()
                );
            }
            let deps: Vec<Vec<TaskKey>> = started_for(shape)
                .registry()
                .expect("started")
                .entries()
                .iter()
                .map(|entry| entry.deps.clone())
                .collect();
            let expected: Vec<Vec<TaskKey>> = match shape {
                PlanShape::Chain => vec![vec![], vec![ALEPH], vec![BET]],
                PlanShape::FanOut => vec![vec![], vec![ALEPH], vec![ALEPH]],
                PlanShape::Join => vec![vec![], vec![], vec![ALEPH, BET]],
            };
            assert_eq!(deps, expected, "{}", shape.name());
            for state in census.states() {
                assert_eq!(
                    classify(&state.fold),
                    classify(
                        &TopologyFold::replay(inputs_for(shape), &state.trace).expect("replays")
                    ),
                    "{}: state {} classifies differently live and on replay",
                    shape.name(),
                    state.id
                );
            }
        }
    }

    #[test]
    fn every_declared_dimension_is_reached_at_its_bound() {
        let bounds = CensusBounds::default();
        let rendered = format!("{bounds:#?}");
        let fields: BTreeSet<&str> = rendered
            .lines()
            .filter_map(|line| line.trim().split_once(':'))
            .map(|(name, _)| name)
            .filter(|name| *name != "max_trace" && *name != "max_states")
            .collect();
        assert_eq!(
            fields,
            bounds
                .dimensions()
                .iter()
                .map(|(name, _)| *name)
                .collect::<BTreeSet<_>>(),
            "a bound the struct declares and `dimensions()` does not"
        );

        let shared = reached_dimensions(&[census()]);
        let deep = reached_dimensions(&[deep_census()]);
        let together = reached_dimensions(&[census(), deep_census()]);
        for (name, declared) in bounds.dimensions() {
            assert!(
                together[name] <= declared,
                "{name}: the censuses reached {} beyond the declared {declared}",
                together[name]
            );
            assert_eq!(
                together[name], declared,
                "{name}: declared {declared} and reached {} (shared census {}, deep census {}); a \
                 boundary the censuses did not reach is not evidence they explored it",
                together[name], shared[name], deep[name]
            );
        }
        for name in [
            "originals",
            "generations_per_task",
            "attempts_per_generation",
            "defers",
            "questions",
            "review_passes",
            "resumes",
        ] {
            assert_eq!(
                shared[name],
                bounds
                    .dimensions()
                    .iter()
                    .find(|(held, _)| *held == name)
                    .map(|(_, bound)| *bound)
                    .expect("declared"),
                "{name}: the shared census reaches this bound on its own"
            );
        }
        assert_eq!(
            deep["sequences"], 4,
            "the deep census consumes four sequences"
        );
        assert_eq!(deep["repairs"], 2, "and registers two repairs");
        assert!(
            !deep_census().truncated(),
            "the deep census closes under its ceilings: {} states",
            deep_census().states().len()
        );
    }
    fn merge_prepared_of(label: &str) -> MergePrepared {
        let candidate = classes(&started())
            .into_iter()
            .find(|candidate| candidate.label == label)
            .unwrap_or_else(|| panic!("the classes offer no `{label}`"));
        match candidate.event.body {
            TopologyEventBody::MergePrepared { data } => *data,
            other => panic!("`{label}` is a {:?}", other.kind()),
        }
    }

    fn merge_prepared_diff(left: &MergePrepared, right: &MergePrepared) -> Vec<&'static str> {
        let mut out = Vec::new();
        for (name, differs) in [
            ("sequence", left.sequence != right.sequence),
            ("disposition", left.disposition != right.disposition),
            ("expected_head", left.expected_head != right.expected_head),
            ("proposed_sha", left.proposed_sha != right.proposed_sha),
            ("key", left.key != right.key),
            ("generation", left.generation != right.generation),
            ("candidate_sha", left.candidate_sha != right.candidate_sha),
            ("candidate_ref", left.candidate_ref != right.candidate_ref),
            ("prepared_ref", left.prepared_ref != right.prepared_ref),
            (
                "verification_source",
                left.verification_source != right.verification_source,
            ),
            ("verification", left.verification != right.verification),
            ("satisfies", left.satisfies != right.satisfies),
        ] {
            if differs {
                out.push(name);
            }
        }
        out
    }

    #[test]
    fn every_publication_negative_differs_from_its_positive_in_exactly_one_field() {
        let fast = merge_prepared_of("merge_prepared/fast/match/aleph/g0");
        let stale = merge_prepared_of("merge_prepared/stale_clean/match/aleph/g0");
        let present = merge_prepared_of("merge_prepared/already_present/match/aleph/g0");

        let rendered = format!("{fast:#?}");
        let fields: BTreeSet<&str> = rendered
            .lines()
            .filter_map(|line| line.trim().split_once(':'))
            .map(|(name, _)| name)
            .filter(|name| {
                !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '_')
            })
            .collect();
        let mut every = fast.clone();
        every.sequence = SequenceId(97);
        every.disposition = PreparedDisposition::AlreadyPresent;
        every.expected_head = sha("nothing-alike");
        every.proposed_sha = sha("nothing-alike-either");
        every.key = BET;
        every.generation = GenerationId(9);
        every.candidate_sha = sha("nor-this");
        every.candidate_ref = git_ref("nor/this");
        every.prepared_ref = Some(git_ref("nor/that"));
        every.verification_source = VerificationSource::Verification {
            sequence: SequenceId(97),
        };
        every.verification = Some(VerificationRecord {
            verdict: VerificationVerdict::Passed,
            gates_passed: false,
            reviews: Vec::new(),
            detail: "different".to_owned(),
        });
        every.satisfies = vec![BET, ALEPH];
        assert_eq!(
            merge_prepared_diff(&fast, &every)
                .into_iter()
                .collect::<BTreeSet<_>>(),
            fields,
            "the field-by-field diff and the record's own fields are not the same list"
        );

        let census = census();
        let accepted = census.accepted_labels();
        let refused = census.refused_labels();
        for (positive, label, field) in [
            (
                &fast,
                "merge_prepared/fast/moved-head/aleph/g0",
                "expected_head",
            ),
            (
                &fast,
                "merge_prepared/fast/other-proposed/aleph/g0",
                "proposed_sha",
            ),
            (
                &fast,
                "merge_prepared/fast/with-pin/aleph/g0",
                "prepared_ref",
            ),
            (
                &fast,
                "merge_prepared/fast/with-verification/aleph/g0",
                "verification",
            ),
            (
                &stale,
                "merge_prepared/stale_clean/mismatch/aleph/g0",
                "proposed_sha",
            ),
            (
                &present,
                "merge_prepared/already_present/mismatch/aleph/g0",
                "proposed_sha",
            ),
        ] {
            let negative = merge_prepared_of(label);
            assert_eq!(
                merge_prepared_diff(positive, &negative),
                vec![field],
                "`{label}` is not its positive with one field moved"
            );
            assert!(refused.contains(label), "`{label}` was never refused");
            assert!(
                !accepted.contains(label),
                "`{label}` was accepted somewhere"
            );
        }
        for positive in [
            "merge_prepared/fast/match/aleph/g0",
            "merge_prepared/stale_clean/match/aleph/g0",
            "merge_prepared/already_present/match/aleph/g0",
        ] {
            assert!(
                accepted.contains(positive),
                "`{positive}` was never accepted"
            );
        }
    }

    // -----------------------------------------------------------------------
    // PR10: the resume classifier over every explored state, the fault rows,
    // the runner identity in both directions, and the summary (ST-14).
    // -----------------------------------------------------------------------

    use crate::engine::topology::reachability::{
        self, ResumeAction, classify, matches_row, rows_reached,
    };
    use crate::topology::effects::FaultRow;

    struct CensusIds;

    impl crate::engine::topology::seams::IdSource for CensusIds {
        fn run_id(&self) -> String {
            RUN_ID.to_owned()
        }

        fn incarnation(&self) -> IncarnationId {
            IncarnationId("01J8ZQKB2M7NC5PQR0TVWXYZ88".to_owned())
        }

        fn pid(&self) -> u32 {
            4242
        }

        fn question_id(&self) -> QuestionId {
            QuestionId::from("q-census-repair")
        }
    }

    #[test]
    fn every_explored_state_classifies_and_the_classification_is_the_same_live_and_on_replay() {
        let census = census();
        let (mut finalize, mut reopen, mut recover) = (0, 0, 0);
        for state in census.states() {
            let live = classify(&state.fold);
            let from_prefix = classify(&replayed(&state.trace));
            assert_eq!(
                live, from_prefix,
                "state {}: the live fold and the durable prefix classify differently",
                state.id
            );
            match (&live, state.fold.finished()) {
                (ResumeAction::FinalizeThenRefuse { outcome }, Some(finished)) => {
                    assert!(
                        matches!(finished, RunOutcome::Complete | RunOutcome::Halted),
                        "state {}",
                        state.id
                    );
                    assert_eq!(outcome, finished, "state {}", state.id);
                    finalize += 1;
                }
                (ResumeAction::Recover(plan), Some(finished)) => {
                    assert!(
                        matches!(finished, RunOutcome::Parked | RunOutcome::BudgetExceeded),
                        "state {}: a Complete or Halted run is finalized, never recovered",
                        state.id
                    );
                    assert_eq!(plan.reopens.as_ref(), Some(finished), "state {}", state.id);
                    reopen += 1;
                }
                (ResumeAction::Recover(plan), None) => {
                    assert!(plan.reopens.is_none(), "state {}", state.id);
                    assert_eq!(
                        plan.derived,
                        reachability::derived_label(&state.outcome),
                        "state {}",
                        state.id
                    );
                    recover += 1;
                }
                (ResumeAction::NotStarted, _) => {
                    panic!("state {}: every census state has a run", state.id)
                }
                (ResumeAction::FinalizeThenRefuse { .. }, None) => {
                    panic!(
                        "state {}: finalization needs a durable run_finished",
                        state.id
                    )
                }
            }
        }
        assert!(
            finalize > 0 && reopen > 0 && recover > 0,
            "finalize {finalize}, reopen {reopen}, recover {recover}: each kind is explored"
        );
    }

    #[test]
    fn every_fault_rows_durable_prefix_is_a_reachable_state_classified_as_its_resume_action() {
        let census = census();
        let mut reached: BTreeMap<FaultRow, usize> = BTreeMap::new();
        for state in census.states() {
            let action = classify(&state.fold);
            for row in rows_reached(&state.fold) {
                assert!(
                    matches_row(row, &state.fold, &action),
                    "state {}: a {} prefix classified as {}",
                    state.id,
                    reachability::row_name(row),
                    reachability::action_label(&action)
                );
                *reached.entry(row).or_insert(0) += 1;
            }
        }
        for row in [
            FaultRow::TRunstart,
            FaultRow::TDispatch,
            FaultRow::TAttempt,
            FaultRow::TCandObj,
            FaultRow::TCandRef,
            FaultRow::TScrub,
            FaultRow::TFailed,
            FaultRow::TFast,
            FaultRow::TProposal,
            FaultRow::TVerify,
            FaultRow::TPrepared,
            FaultRow::TAnswer,
            FaultRow::TFinish,
            FaultRow::TFinalize,
            FaultRow::TResume,
        ] {
            assert!(
                reached.get(&row).copied().unwrap_or(0) > 0,
                "{}: no explored state is this row's durable prefix",
                reachability::row_name(row)
            );
        }
        for row in [
            FaultRow::TReject,
            FaultRow::TRepairDispatch,
            FaultRow::TRetained,
            FaultRow::TRetry,
        ] {
            assert!(
                reached.get(&row).copied().unwrap_or(0) > 0,
                "{}: no explored state of the shared census is this row's durable prefix; the \
                 rejections, retained settlements and resumed attempts the generator offers \
                 reach it without a seed",
                reachability::row_name(row)
            );
        }
        for row in [FaultRow::TContainer, FaultRow::TAppend] {
            assert!(reachability::outside_the_fold(row));
            assert_eq!(reached.get(&row).copied().unwrap_or(0), 0);
        }
        assert_eq!(FaultRow::ALL.len(), 21);
    }

    fn conflict_rejection(fold: &TopologyFold) -> TopologyEvent {
        let rejected = crate::engine::topology::repair::merge_rejected(
            fold,
            &CensusIds,
            &candidate_of(ALEPH, 0),
            sha("base"),
            SequenceId(0),
            crate::topology::events::RejectionDisposition::Conflict {
                paths: region(ALEPH),
            },
            region(ALEPH),
        )
        .expect("a conflict rejection registers a repair of aleph");
        ev(TopologyEventBody::MergeRejected {
            data: Box::new(rejected),
        })
    }

    #[test]
    fn a_rejection_and_its_repairs_dispatch_are_reachable_prefixes_classified_as_tabled() {
        let mut trace = queued_candidate_trace(region(ALEPH));
        let fold = replayed(&trace);
        let rejection = conflict_rejection(&fold);
        trace.push(rejection.clone());
        let rejected = replayed(&trace);
        let repair = TaskKey(3);
        assert_eq!(rejected.task_state(ALEPH), Some(TaskState::AwaitingRepair));
        assert_eq!(rejected.task_state(repair), Some(TaskState::Pending));
        assert!(
            rejected
                .leases()
                .expect("started")
                .lineages()
                .iter()
                .any(|lineage| lineage.root == ALEPH),
            "the rejection creates the lineage lease"
        );
        let action = classify(&rejected);
        assert!(
            rows_reached(&rejected).contains(&FaultRow::TReject),
            "{}",
            reachability::action_label(&action)
        );
        assert!(matches_row(FaultRow::TReject, &rejected, &action));
        assert_eq!(action, classify(&replayed(&trace)), "live equals replay");

        let TopologyEventBody::MergeRejected { data } = &rejection.body else {
            unreachable!("built above");
        };
        let dispatch = ev(TopologyEventBody::TaskDispatched {
            data: TaskDispatched {
                key: repair,
                generation: GenerationId(0),
                base_sha: sha("base"),
                worktree_path: "/tmp/census/repair".to_owned(),
                lease: LeaseGrant::InheritedLineage { root: ALEPH },
                source_candidate: Some(data.candidate.clone()),
            },
        });
        trace.push(dispatch);
        let dispatched = replayed(&trace);
        let action = classify(&dispatched);
        assert!(rows_reached(&dispatched).contains(&FaultRow::TRepairDispatch));
        assert!(matches_row(FaultRow::TRepairDispatch, &dispatched, &action));
        assert!(
            !matches_row(FaultRow::TDispatch, &dispatched, &action),
            "a repair's open generation is not an ordinary dispatch's"
        );
        let ResumeAction::Recover(plan) = &action else {
            panic!("{action:?}");
        };
        assert_eq!(
            plan.recreate_open
                .iter()
                .map(|open| (open.key, open.generation, open.lineage))
                .collect::<Vec<_>>(),
            vec![(repair.0, 0, true)]
        );

        // Both prefixes are census states when explored from the seed.
        let seeded = Census::explore(
            rejected.clone(),
            trace[..trace.len() - 1].to_vec(),
            CensusBounds {
                max_trace: trace.len() + 1,
                max_states: 500,
                ..CensusBounds::default()
            },
            |fold| {
                let mut out = classes(fold);
                out.retain(|candidate| !candidate.label.starts_with("task_dispatched/"));
                out.push(Candidate::new(
                    "task_dispatched/repair/g0",
                    ev(TopologyEventBody::TaskDispatched {
                        data: TaskDispatched {
                            key: repair,
                            generation: GenerationId(0),
                            base_sha: sha("base"),
                            worktree_path: "/tmp/census/repair".to_owned(),
                            lease: LeaseGrant::InheritedLineage { root: ALEPH },
                            source_candidate: Some(candidate_of(ALEPH, 0)),
                        },
                    }),
                ));
                out
            },
        );
        // One step deep by construction, so the trace ceiling stops legal
        // continuations and the census says so; the state ceiling is not hit.
        assert!(seeded.truncated() && seeded.states().len() < 500);
        let mut reject = 0;
        let mut repair_dispatch = 0;
        for state in seeded.states() {
            let action = classify(&state.fold);
            assert_eq!(
                action,
                classify(&replayed(&state.trace)),
                "state {}",
                state.id
            );
            let rows = rows_reached(&state.fold);
            for row in &rows {
                assert!(
                    matches_row(*row, &state.fold, &action),
                    "state {}: {row:?}",
                    state.id
                );
            }
            reject += usize::from(rows.contains(&FaultRow::TReject));
            repair_dispatch += usize::from(rows.contains(&FaultRow::TRepairDispatch));
        }
        assert!(
            reject > 0 && repair_dispatch > 0,
            "{reject}/{repair_dispatch}"
        );
        assert!(
            seeded
                .accepted_labels()
                .contains("task_dispatched/repair/g0"),
            "the repair dispatches in the exploration"
        );
    }

    const RETAINED_SESSION: &str = "census-retained-session";

    fn retained_settlement(key: TaskKey, generation: u32, attempt: u32) -> TopologyEvent {
        ev(TopologyEventBody::AttemptFinished {
            data: Box::new(AttemptFinished4 {
                key,
                generation: GenerationId(generation),
                attempt: AttemptNumber(attempt),
                record: Box::new({
                    let mut record = attempt_record(attempt);
                    record.session_id = Some(RETAINED_SESSION.to_owned());
                    record.failure = Some(crate::events::FailureRecord {
                        kind: crate::ladder::FailureKind::GateFailed,
                        origin: crate::ladder::FailureOrigin::Worker,
                        reason: "the fixture's retained failure".to_owned(),
                        detail: None,
                    });
                    record
                }),
                settlement: AttemptSettlement::Retained {
                    retained_session: crate::topology::events::SessionId(
                        RETAINED_SESSION.to_owned(),
                    ),
                    retained_incarnation: crate::topology::events::Epoch(0),
                },
            }),
        })
    }

    fn resumed_attempt(
        fold: &TopologyFold,
        key: TaskKey,
        generation: u32,
        attempt: u32,
    ) -> TopologyEvent {
        let TopologyEventBody::AttemptStarted { mut data } =
            attempt_started(fold, key, generation, attempt).body
        else {
            unreachable!("attempt_started builds an attempt_started");
        };
        data.resume_session = Some(crate::topology::events::SessionId(
            RETAINED_SESSION.to_owned(),
        ));
        if data.materialization_observed.is_some() {
            data.materialization_observed =
                Some(crate::topology::events::Materialization::Retained);
        }
        ev(TopologyEventBody::AttemptStarted { data })
    }

    #[test]
    fn a_retained_generation_and_its_retry_are_reachable_prefixes_classified_as_tabled() {
        let started_fold = started();
        let mut trace = vec![
            run_started_event(),
            dispatch(ALEPH, 0),
            attempt_started(&started_fold, ALEPH, 0, 1),
            retained_settlement(ALEPH, 0, 1),
        ];
        let retained = replayed(&trace);
        assert!(
            retained.ready_retry(ALEPH),
            "the retaining incarnation may retry"
        );
        let action = classify(&retained);
        assert!(rows_reached(&retained).contains(&FaultRow::TRetained));
        assert!(matches_row(FaultRow::TRetained, &retained, &action));
        let ResumeAction::Recover(plan) = &action else {
            panic!("{action:?}");
        };
        assert_eq!(
            plan.close_retained
                .iter()
                .map(|closed| (closed.key, closed.generation))
                .collect::<Vec<_>>(),
            vec![(ALEPH.0, 0)]
        );

        trace.push(resumed_attempt(&retained, ALEPH, 0, 2));
        let retrying = replayed(&trace);
        let action = classify(&retrying);
        assert!(rows_reached(&retrying).contains(&FaultRow::TRetry));
        assert!(matches_row(FaultRow::TRetry, &retrying, &action));
        assert_eq!(action, classify(&replayed(&trace)), "live equals replay");

        let seeded = Census::explore(
            retained.clone(),
            trace[..trace.len() - 1].to_vec(),
            CensusBounds {
                max_trace: trace.len() + 1,
                max_states: 500,
                ..CensusBounds::default()
            },
            |fold| {
                let mut out = classes(fold);
                out.push(Candidate::new(
                    "attempt_started/resumed/aleph/g0/a2",
                    resumed_attempt(fold, ALEPH, 0, 2),
                ));
                out
            },
        );
        // One step deep by construction, so the trace ceiling stops legal
        // continuations and the census says so; the state ceiling is not hit.
        assert!(seeded.truncated() && seeded.states().len() < 500);
        let (mut retained_states, mut retry_states) = (0, 0);
        for state in seeded.states() {
            let action = classify(&state.fold);
            assert_eq!(
                action,
                classify(&replayed(&state.trace)),
                "state {}",
                state.id
            );
            let rows = rows_reached(&state.fold);
            for row in &rows {
                assert!(
                    matches_row(*row, &state.fold, &action),
                    "state {}: {row:?}",
                    state.id
                );
            }
            retained_states += usize::from(rows.contains(&FaultRow::TRetained));
            retry_states += usize::from(rows.contains(&FaultRow::TRetry));
        }
        assert!(
            retained_states > 0 && retry_states > 0,
            "{retained_states}/{retry_states}"
        );
        assert!(
            seeded
                .accepted_labels()
                .contains("attempt_started/resumed/aleph/g0/a2"),
            "the same-session retry is accepted in the exploration"
        );
        assert!(
            seeded
                .refused_labels()
                .contains("attempt_started/aleph/g0/a2"),
            "and a retry that resumes no session is refused on a retained generation"
        );
    }

    fn resumed_with(runner: RunnerPolicy) -> TopologyEvent {
        ev(TopologyEventBody::RunResumed {
            data: Box::new(crate::topology::events::RunResumed4 {
                incarnation: IncarnationId("01J8ZQKB2M7NC5PQR0TVWXYZ99".to_owned()),
                runner,
                probed_agents: probed_agents(),
                upstroke_version: "0.2.0-census-resume".to_owned(),
            }),
        })
    }

    fn runner_variants() -> Vec<(&'static str, RunnerPolicy)> {
        let recorded = run_started().runner;
        let image = || {
            recorded
                .image
                .clone()
                .expect("the fixture records an image")
        };
        let mut variants = Vec::new();
        variants.push(("kind", {
            let mut other = recorded.clone();
            other.kind = RunnerKind::Host;
            other
        }));
        variants.push(("policy", {
            let mut other = recorded.clone();
            other.policy = RunnerContract::HostV1;
            other
        }));
        variants.push(("reference", {
            let mut other = recorded.clone();
            let mut moved = image();
            moved.reference = "ghcr.io/example/census-runner:3.5".to_owned();
            other.image = Some(moved);
            other
        }));
        variants.push(("id", {
            let mut other = recorded.clone();
            let mut moved = image();
            moved.id = format!("sha256:{}", "9".repeat(64));
            other.image = Some(moved);
            other
        }));
        variants.push(("digest", {
            let mut other = recorded.clone();
            let mut moved = image();
            moved.digest = None;
            other.image = Some(moved);
            other
        }));
        variants.push(("volumes", {
            let mut other = recorded.clone();
            other.credential_volumes = Some(BTreeMap::new());
            other
        }));
        variants.push(("image presence", {
            let mut other = recorded.clone();
            other.image = None;
            other
        }));
        variants
    }

    #[test]
    fn run_resumed_is_accepted_with_an_identical_runner_and_refused_with_any_different_field() {
        let census = census();
        let identical = resumed_with(run_started().runner);
        let variants = runner_variants();
        assert_eq!(variants.len(), 7);
        let (mut accepted, mut over, mut refused) = (0, 0, 0);
        for state in census.states() {
            let over_already = matches!(
                state.fold.finished(),
                Some(RunOutcome::Complete | RunOutcome::Halted)
            );
            match state.fold.plan_transition(&identical) {
                Ok(delta) => {
                    assert!(!over_already, "state {}: a finished run resumed", state.id);
                    let mut next = state.fold.clone();
                    next.apply_delta(delta);
                    assert_eq!(
                        next.epoch().map(|epoch| epoch.0),
                        state.fold.epoch().map(|epoch| epoch.0 + 1),
                        "state {}",
                        state.id
                    );
                    assert!(next.budget_stop().is_none(), "state {}", state.id);
                    assert!(next.finished().is_none(), "state {}", state.id);
                    assert!(
                        [ALEPH, BET]
                            .iter()
                            .all(|key| next.task_state(*key) != Some(TaskState::Deferred)),
                        "state {}: the resume wakes every deferred task",
                        state.id
                    );
                    assert_eq!(
                        classify(&next),
                        classify(&{
                            let mut trace = state.trace.clone();
                            trace.push(identical.clone());
                            replayed(&trace)
                        }),
                        "state {}: the resumed state classifies alike live and on replay",
                        state.id
                    );
                    accepted += 1;
                }
                Err(error) => {
                    assert!(over_already, "state {}: {error}", state.id);
                    over += 1;
                }
            }
            for (field, runner) in &variants {
                let error = state
                    .fold
                    .plan_transition(&resumed_with(runner.clone()))
                    .expect_err("a different runner is refused");
                if !over_already {
                    let text = error.to_string();
                    assert!(
                        text.contains("runner")
                            || text.contains("image")
                            || text.contains("volume"),
                        "state {}: the {field} refusal names the identity: {text}",
                        state.id
                    );
                }
                refused += 1;
            }
        }
        assert!(accepted > 0 && over > 0, "{accepted}/{over}");
        assert_eq!(refused, census.states().len() * variants.len());
    }

    #[test]
    fn the_census_summary_names_every_fault_row_and_serializes() {
        let census = census();
        let summary = reachability::summarize(census, true);
        assert_eq!(summary.states, census.states().len());
        assert_eq!(summary.transitions, census.transitions().len());
        assert!(summary.accepted + summary.refused <= summary.transitions);
        assert!(
            summary.truncated,
            "the summary says the census stopped at its state ceiling"
        );
        assert_eq!(summary.fault_rows.len(), 21);
        for row in FaultRow::ALL {
            let name = reachability::row_name(*row);
            let entry = summary
                .fault_rows
                .get(&name)
                .unwrap_or_else(|| panic!("{name} is missing from the summary"));
            assert!(entry.every_reachable_state_classifies_as_tabled, "{name}");
            assert_eq!(entry.outside_the_fold, reachability::outside_the_fold(*row));
            if entry.outside_the_fold {
                assert_eq!(entry.reachable_states, 0, "{name}");
            }
        }
        assert!(summary.fault_rows["T-FINALIZE"].reachable_states > 0);
        assert_eq!(summary.outcomes.len(), 5, "{:?}", summary.outcomes.keys());
        assert!(summary.actions.len() >= 4, "{:?}", summary.actions.keys());
        assert_eq!(summary.bounds["max_trace"], 48);
        assert_eq!(summary.bounds["max_states"], 20_000);
        assert_eq!(summary.bounds["review_passes"], 1);
        let json = serde_json::to_string_pretty(&summary).expect("serializes");
        assert!(json.contains("\"T-RESUME\"") && json.contains("finalize then refuse"));
        if let Ok(path) = std::env::var("UPSTROKE_CENSUS_SUMMARY") {
            crate::workspace_manager::fixture::write_file(
                std::path::Path::new(&path),
                format!("{json}\n").as_bytes(),
            );
        }
    }
}
