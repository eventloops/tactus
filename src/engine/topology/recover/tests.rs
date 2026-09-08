//! Extended notes: `docs/internals/engine/topology/recover/tests.md`

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use super::*;
use crate::agent::{AdapterSource, AgentAdapter, Caps, ProcessOutput, TaskRun};
use crate::config::RunnerSelection;
use crate::events::log::{BarrierStep, EventLog, TopologyLine};
use crate::events::{AttemptRecord, BindingSummary, BudgetKind, ChainSummary, GateSummary};
use crate::gates::ShellKind;
use crate::ir::Outcome;
use crate::ir::{
    Artifact, ArtifactId, Effort, Plan, PlanSource, ResolvedEffortPolicy, Task, TaskId, TaskKind,
    Tier,
};
use crate::review::{PassBinding, ReviewPlan};
use crate::rundir::{
    self, CommitRecord, CreatingMarker, NoHooks, OwnerRecord, RepoKey, RetainReason,
};
use crate::runner::container::resolve::RunnerPreflight;
use crate::runner::container::runtime::{ContainerRuntime, ContainerTrace};
use crate::runner::container::{DisposableDirView, FakeOwnerLiveness, FakeRuntime};
use crate::runner::policy::runner_policy_sha256;
use crate::runner::{CommandSpec, Runner, RunnerError, RunnerRequest};
use crate::topology::effects::EventSite;
use crate::topology::effects::{
    EffectSiteId, HookHarness, HookPhase, Injection, InjectionMode, LockSite, ObjectSite, RefSite,
    RunDirSite, SubEffectPoint, WorktreeSite,
};
use crate::topology::events::{
    AttemptFinished4, AttemptSettlement, AttemptStarted4, BudgetExceeded4, CommitSha, Epoch,
    GitRef, ImageIdentity, IncarnationId, LeaseGrant, RunFinished4, RunStarted4, RungBinding,
    RunnerContract, RunnerKind, RunnerPolicy, SessionId, SettlementTransition, TaskDispatched,
    TopologyEvent, TopologyLimits,
};
use crate::topology::fold::{FrozenInputs, TaskState};
use crate::topology::paths::{GitPath, PathSet};
use crate::topology::paths::{PathGrammar, PathPolicy, PathPolicyVersion};
use crate::topology::registry::{TaskKey, TaskRegistry};
use crate::topology::schema::TOPOLOGY_SCHEMA;

use crate::workspace_manager::Refusal;

use crate::engine::topology::identity::{InvocationLedger, ReservationKind, Reservations};
use crate::engine::topology::preflight::RunPreflight;
use crate::engine::topology::run::Progress;
use crate::engine::topology::seams::{HarnessTopologyHooks, TimeSource, TopologyHooks};
use crate::engine::topology::startup::{FailedStep, RunDirOutcome};

const RUN_ID: &str = "01KZTPR7E00000000000000001";
const CREATOR: &str = "01KZTAAAAAAAAAAAAAAAAAAAAA";
const RESUMER: &str = "01KZTBBBBBBBBBBBBBBBBBBBBB";
const TS: &str = "2026-08-23T09:41:02Z";
const IMAGE_REF: &str = "ghcr.io/example/upstroke-runner:1.4";
const IMAGE_ID: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
const VOLUME: &str = "upstroke-creds-claude";
const AGENT: &str = "claude-code";
const CREATOR_PID: u32 = 4242;

#[derive(Debug, Clone, Copy)]
struct Frozen;

impl TimeSource for Frozen {
    fn now_rfc3339(&self) -> String {
        TS.to_owned()
    }
}

fn fixture_root(tag: &str) -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let ordinal = NEXT.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "upstroke-pr7e-{}-{tag}-{ordinal}",
        std::process::id()
    ))
}

fn mkdir(path: &Path) {
    rundir::create_public_dir(path, &mut NoHooks).expect("the run-directory funnel creates a dir");
}

struct Fixture {
    root: PathBuf,
    base_sha: CommitSha,
    repo_root: PathBuf,
    git_dir: PathBuf,
    private_root: PathBuf,
    repo_key: RepoKey,
    started: RunStarted4,
    first_line: Vec<u8>,
    plan: Plan,
}

#[derive(Default)]
struct Damage {
    no_private_half: bool,
    no_owner_record: bool,
    owner: Option<fn(&mut OwnerRecord)>,
    commit: Option<fn(&mut CommitRecord)>,
    locator: Option<String>,
    host_runner: bool,
    extra: Vec<TopologyEventBody>,
    open_generation: bool,
    two_tasks: bool,
    two_tier: bool,
    deep_ladder: bool,
    alternative_reviewer: bool,
    integration_second_opinion: bool,
    no_automatic_repairs: bool,
    alpha_kind: Option<TaskKind>,
    host_gate: Option<&'static str>,
    beta_depends_on_alpha: bool,
    /// Every task's chain is the one Small rung: a merge repair's Mid floor
    /// then intersects it empty, and the repair is admitted `HumanBinding`.
    small_only: bool,
}

impl Fixture {
    fn manager(&self) -> crate::workspace_manager::WorkspaceManager {
        crate::workspace_manager::WorkspaceManager::derive(
            &self.repo_root,
            &self.private_root,
            &self.started.run_id,
            &self.started.incarnation.0,
        )
        .expect("the fixture's repository and private root are real directories")
    }

    fn build(tag: &str, damage: Damage) -> Self {
        let root = fixture_root(tag);
        let repo_root = root.join("repo");
        let git_dir = repo_root.join(".git");
        let private_root = root.join("private");
        mkdir(&repo_root);
        crate::workspace_manager::fixture::git(&repo_root, &["init", "-q", "-b", "main"]);
        for setting in [
            ["config", "user.email", "tests@upstroke.local"],
            ["config", "user.name", "upstroke tests"],
            ["config", "core.logAllRefUpdates", "true"],
            ["config", "core.autocrlf", "false"],
            ["config", "core.eol", "lf"],
        ] {
            crate::workspace_manager::fixture::git(&repo_root, &setting);
        }
        crate::workspace_manager::fixture::write_file(&repo_root.join("seed.txt"), b"seed\n");
        crate::workspace_manager::fixture::git(&repo_root, &["add", "-A"]);
        crate::workspace_manager::fixture::git(&repo_root, &["commit", "-q", "-m", "seed"]);
        let base_sha = CommitSha(crate::workspace_manager::fixture::git(
            &repo_root,
            &["rev-parse", "HEAD"],
        ));
        mkdir(&private_root);
        let repo_key = RepoKey::v1(&std::fs::canonicalize(&git_dir).expect("the git dir exists"));

        let public = rundir::public_dir(&repo_root, RUN_ID);
        mkdir(&public);
        let private_dir = private_root.join("runs").join(RUN_ID);
        if !damage.no_private_half {
            mkdir(&private_dir);
        }

        let mut plan = plan_with(damage.two_tasks);
        if damage.beta_depends_on_alpha {
            if let Some(beta) = plan.tasks.get_mut(1) {
                beta.depends_on = vec![TaskId::from("alpha")];
            }
        }
        if let Some(kind) = damage.alpha_kind {
            if let Some(alpha) = plan.tasks.first_mut() {
                alpha.kind = kind;
            }
        }
        let recorded_locator = damage
            .locator
            .clone()
            .unwrap_or_else(|| private_dir.display().to_string());
        let runner = if damage.host_runner {
            host_runner()
        } else {
            container_runner()
        };
        let started = run_started(&plan, &recorded_locator, runner, &base_sha, &damage);

        let marker = CreatingMarker {
            run_id: RUN_ID.to_owned(),
            repo_key: repo_key.as_str().to_owned(),
            private_dir: recorded_locator.clone(),
            incarnation: CREATOR.to_owned(),
            pid: CREATOR_PID,
            runner_policy_sha256: runner_policy_sha256(&started.runner),
        };
        rundir::stage_marker(&public, &marker, &mut NoHooks).expect("P1a stages the marker");
        rundir::publish_marker(&public, &mut NoHooks).expect("P1b publishes it");

        let mut warnings = Vec::new();
        let mut log = EventLog::open(
            EventSite::OpenLog,
            &public.join(rundir::EVENT_LOG),
            &mut warnings,
        )
        .expect("the Event funnel opens a fresh log");
        let (line, _) = TopologyLine::round_trip(&event(TopologyEventBody::RunStarted {
            data: Box::new(started.clone()),
        }))
        .expect("run_started survives its own wire format");
        log.append_topology(EventSite::AppendFirst, &line)
            .expect("the commitment boundary");
        let first_line = line.committed_bytes()[..line.committed_bytes().len() - 1].to_vec();
        let mut later: Vec<TopologyEventBody> = Vec::new();
        if damage.open_generation {
            later.push(dispatched_at(&base_sha));
        }
        later.extend(damage.extra.iter().cloned());
        for body in &later {
            let site = crate::events::log::site_for(body);
            let (line, _) =
                TopologyLine::round_trip(&event(body.clone())).expect("a valid later event");
            log.append_topology(site, &line).expect("a later append");
        }
        drop(log);

        if !damage.no_private_half {
            if !damage.no_owner_record {
                let mut owner = OwnerRecord {
                    run_id: RUN_ID.to_owned(),
                    repo_key: repo_key.as_str().to_owned(),
                    public_dir: canonical(&public),
                    incarnation: CREATOR.to_owned(),
                    runner: started.runner.clone(),
                };
                if let Some(damage) = damage.owner {
                    damage(&mut owner);
                }
                rundir::stage_owner_record(&private_dir, &owner, &mut NoHooks)
                    .expect("P3a stages the owner record");
                rundir::publish_owner_record(&private_dir, &mut NoHooks).expect("P3b publishes it");
            }
            let mut commit = CommitRecord {
                run_id: RUN_ID.to_owned(),
                repo_key: repo_key.as_str().to_owned(),
                public_dir: canonical(&public),
                incarnation: CREATOR.to_owned(),
                run_started_sha256: rundir::run_started_sha256(&first_line),
            };
            if let Some(damage) = damage.commit {
                damage(&mut commit);
            }
            rundir::stage_commit_record(&private_dir, &commit, &mut NoHooks)
                .expect("P5a stages the commit record");
            rundir::publish_commit_record(&private_dir, &mut NoHooks).expect("P5b publishes it");
        }

        Self {
            root,
            base_sha,
            repo_root,
            git_dir,
            private_root,
            repo_key,
            started,
            first_line,
            plan,
        }
    }

    fn healthy(tag: &str) -> Self {
        Self::build(tag, Damage::default())
    }

    fn two_tasks(tag: &str) -> Self {
        Self::build(
            tag,
            Damage {
                two_tasks: true,
                ..Damage::default()
            },
        )
    }

    fn public(&self) -> PathBuf {
        rundir::public_dir(&self.repo_root, RUN_ID)
    }

    fn log(&self) -> PathBuf {
        self.public().join(rundir::EVENT_LOG)
    }

    fn log_bytes(&self) -> Vec<u8> {
        crate::util::read_file_bounded(&self.log()).unwrap_or_default()
    }

    fn inputs(&self) -> FrozenInputs {
        FrozenInputs {
            plan: self.plan.clone(),
            normalized_plan_digest: self.started.normalized_plan_digest.clone(),
        }
    }

    fn worktree_lock_file(&self) -> PathBuf {
        self.git_dir.join("upstroke-worktree.lock")
    }

    fn derive(&self, explicit: Option<&Path>) -> Result<RootDerived, UpstrokeError> {
        RootDerived::derive_with(&self.repo_root, RUN_ID, explicit, TOPOLOGY_SCHEMA)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = rundir::remove_public_husk(&self.root, &mut NoHooks);
    }
}

struct PlantedHusk {
    public: PathBuf,
    private: PathBuf,
}

fn plant_husk(fixture: &Fixture, run_id: &str, committed: bool) -> PlantedHusk {
    let public = rundir::public_dir(&fixture.repo_root, run_id);
    let private = fixture.private_root.join("runs").join(run_id);
    mkdir(&public);
    mkdir(&private);

    let runner = container_runner();
    let marker = CreatingMarker {
        run_id: run_id.to_owned(),
        repo_key: fixture.repo_key.as_str().to_owned(),
        private_dir: private.display().to_string(),
        incarnation: CREATOR.to_owned(),
        pid: CREATOR_PID,
        runner_policy_sha256: runner_policy_sha256(&runner),
    };
    rundir::stage_marker(&public, &marker, &mut NoHooks).expect("P1a stages the husk's marker");
    rundir::publish_marker(&public, &mut NoHooks).expect("P1b publishes it");

    let owner = OwnerRecord {
        run_id: run_id.to_owned(),
        repo_key: fixture.repo_key.as_str().to_owned(),
        public_dir: canonical(&public),
        incarnation: CREATOR.to_owned(),
        runner,
    };
    rundir::stage_owner_record(&private, &owner, &mut NoHooks)
        .expect("P3a stages the owner record");
    rundir::publish_owner_record(&private, &mut NoHooks).expect("P3b publishes it");

    if committed {
        let record = CommitRecord {
            run_id: run_id.to_owned(),
            repo_key: fixture.repo_key.as_str().to_owned(),
            public_dir: canonical(&public),
            incarnation: CREATOR.to_owned(),
            run_started_sha256: rundir::run_started_sha256(b"a first line of its own"),
        };
        rundir::stage_commit_record(&private, &record, &mut NoHooks)
            .expect("P5a stages the commit record");
        rundir::publish_commit_record(&private, &mut NoHooks).expect("P5b publishes it");
    }

    PlantedHusk { public, private }
}

fn tree_bytes(root: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    let mut out = std::collections::BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
            out.insert(
                relative,
                crate::util::read_file_bounded(&path).unwrap_or_default(),
            );
        }
    }
    out
}

fn event_kinds(log: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(log)
        .lines()
        .filter_map(|line| {
            let at = line.find("\"event\":\"")? + "\"event\":\"".len();
            let rest = line.get(at..)?;
            let end = rest.find('"')?;
            rest.get(..end).map(std::borrow::ToOwned::to_owned)
        })
        .collect()
}

fn canonical(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

fn event(body: TopologyEventBody) -> TopologyEvent {
    TopologyEvent {
        ts: TS.to_owned(),
        body,
    }
}

fn plan_with(two_tasks: bool) -> Plan {
    let mut plan = plan();
    if two_tasks {
        let alpha = plan.tasks[0].clone();
        plan.tasks.push(Task {
            id: TaskId::from("beta"),
            title: "beta".to_owned(),
            body: "beta body".to_owned(),
            acceptance: vec!["beta passes".to_owned()],
            path_hints: vec!["src/beta/*.rs".to_owned()],
            artifacts_out: vec![ArtifactId::from("beta-out")],
            ..alpha
        });
        plan.artifacts.push(Artifact {
            id: ArtifactId::from("beta-out"),
            produced_by: Some(TaskId::from("beta")),
        });
    }
    plan
}

fn plan() -> Plan {
    Plan {
        source: PlanSource {
            adapter: "markdown".to_owned(),
            hash: "frozen-plan-hash".to_owned(),
        },
        tasks: vec![Task {
            id: TaskId::from("alpha"),
            kind: TaskKind::Refactor,
            title: "alpha".to_owned(),
            body: "alpha body".to_owned(),
            depends_on: Vec::new(),
            acceptance: vec!["alpha passes".to_owned()],
            path_hints: vec!["src/alpha/*.rs".to_owned()],
            suggested_tier: None,
            min_tier: None,
            artifacts_in: Vec::new(),
            artifacts_out: vec![ArtifactId::from("alpha-out")],
        }],
        artifacts: vec![Artifact {
            id: ArtifactId::from("alpha-out"),
            produced_by: Some(TaskId::from("alpha")),
        }],
    }
}

fn container_runner() -> RunnerPolicy {
    RunnerPolicy {
        kind: RunnerKind::Container,
        policy: RunnerContract::ContainerV1,
        image: Some(ImageIdentity {
            reference: IMAGE_REF.to_owned(),
            id: IMAGE_ID.to_owned(),
            digest: Some("sha256:2222".to_owned()),
        }),
        credential_volumes: Some(
            [(AGENT.to_owned(), VOLUME.to_owned())]
                .into_iter()
                .collect(),
        ),
    }
}

fn host_runner() -> RunnerPolicy {
    RunnerPolicy {
        kind: RunnerKind::Host,
        policy: RunnerContract::HostV1,
        image: None,
        credential_volumes: None,
    }
}

fn chain() -> ChainSummary {
    ChainSummary {
        task: "alpha".to_owned(),
        tiers: vec![Tier::Mid],
        attempts_per: 2,
        bindings: Some(vec![BindingSummary {
            tier: Tier::Mid,
            agent: AGENT.to_owned(),
            model: "claude-opus-5".to_owned(),
            pinned: false,
        }]),
    }
}

fn escalating_chain() -> ChainSummary {
    ChainSummary {
        task: "alpha".to_owned(),
        tiers: vec![Tier::Mid, Tier::Frontier],
        attempts_per: 1,
        bindings: Some(vec![
            BindingSummary {
                tier: Tier::Mid,
                agent: AGENT.to_owned(),
                model: "claude-opus-5".to_owned(),
                pinned: false,
            },
            BindingSummary {
                tier: Tier::Frontier,
                agent: AGENT.to_owned(),
                model: "claude-fable-5".to_owned(),
                pinned: false,
            },
        ]),
    }
}

fn small_chain() -> ChainSummary {
    ChainSummary {
        task: "alpha".to_owned(),
        tiers: vec![Tier::Small],
        attempts_per: 2,
        bindings: Some(vec![BindingSummary {
            tier: Tier::Small,
            agent: AGENT.to_owned(),
            model: "claude-haiku-4-5".to_owned(),
            pinned: false,
        }]),
    }
}

fn deep_chain() -> ChainSummary {
    ChainSummary {
        attempts_per: 2,
        ..escalating_chain()
    }
}

fn review_plan() -> ReviewPlan {
    ReviewPlan {
        enabled: Some(true),
        alternative_available: Some(false),
        pass_timeout_secs: Some(600),
        primary: Some(PassBinding::new(AGENT, "claude-opus-5")),
        alternative: None,
        second_opinion: vec![None],
    }
}

fn run_started(
    plan: &Plan,
    private_dir: &str,
    runner: RunnerPolicy,
    base: &CommitSha,
    damage: &Damage,
) -> RunStarted4 {
    let unauthenticated = RunStarted4 {
        schema: TOPOLOGY_SCHEMA,
        upstroke_version: env!("CARGO_PKG_VERSION").to_owned(),
        run_id: RUN_ID.to_owned(),
        incarnation: IncarnationId(CREATOR.to_owned()),
        runner,
        probed_agents: if damage.integration_second_opinion {
            vec![
                AGENT.to_owned(),
                crate::engine::topology::scaffold::REVIEW_AGENT.to_owned(),
            ]
        } else {
            vec![AGENT.to_owned()]
        },
        branch: "upstroke/run".to_owned(),
        integration_ref: GitRef(format!("refs/upstroke/runs/{RUN_ID}/integration")),
        base_sha: base.clone(),
        execution_root: "/does/not/matter".to_owned(),
        private_dir: private_dir.to_owned(),
        plan_path: "PLAN.md".to_owned(),
        config_path: Some("upstroke.toml".to_owned()),
        plan_hash: "frozen-plan-hash".to_owned(),
        normalized_plan_digest: "sha256:aaaa".to_owned(),
        registry_digest: String::new(),
        path_policy: PathPolicy {
            version: PathPolicyVersion::V2,
            case_fold: false,
            grammar: PathGrammar::Globset,
        },
        limits: TopologyLimits {
            max_parallel: 1,
            max_defers: 3,
            max_merge_repairs: u32::from(!damage.no_automatic_repairs),
        },
        gates: vec!["clippy".to_owned()],
        gates_from_config: true,
        gate_cmds: vec![damage.host_gate.map_or_else(
            || GateSummary {
                name: "clippy".to_owned(),
                cmd: "cargo clippy".to_owned(),
                timeout: Duration::from_secs(600),
                shell: ShellKind::Bash,
            },
            |cmd| GateSummary {
                name: "clippy".to_owned(),
                cmd: cmd.to_owned(),
                timeout: Duration::from_secs(60),
                shell: ShellKind::Sh,
            },
        )],

        interaction_mode: "attached".to_owned(),
        chains: {
            let first = if damage.small_only {
                small_chain()
            } else if damage.deep_ladder {
                deep_chain()
            } else if damage.two_tier {
                escalating_chain()
            } else {
                chain()
            };
            let mut chains = vec![first.clone()];
            if damage.two_tasks {
                chains.push(ChainSummary {
                    task: "beta".to_owned(),
                    ..first
                });
            }
            chains
        },
        effort_policy: ResolvedEffortPolicy {
            small: Effort::Low,
            mid: Effort::High,
            frontier: Effort::Max,
            review: Effort::Medium,
        },
        reviews: {
            let mut reviews = review_plan();
            if damage.two_tasks {
                reviews.second_opinion.push(None);
            }
            if damage.integration_second_opinion {
                if let Some(first) = reviews.second_opinion.first_mut() {
                    *first = Some(PassBinding::new(
                        crate::engine::topology::scaffold::REVIEW_AGENT,
                        "gpt-5.6",
                    ));
                }
            }
            if damage.alternative_reviewer {
                reviews.alternative = Some(PassBinding::new(AGENT, "claude-fable-5"));
                reviews.alternative_available = Some(true);
            }
            reviews
        },
    };
    let registry_digest = TaskRegistry::originals_with_agents(
        plan,
        &unauthenticated.registry_record(),
        &unauthenticated.probed_agents,
    )
    .expect("the fixture record derives a registry")
    .digest();
    RunStarted4 {
        registry_digest,
        ..unauthenticated
    }
}

fn runtime_holding_the_record() -> FakeRuntime {
    let runtime = FakeRuntime::new(ContainerTrace::default());
    runtime.add_image(IMAGE_ID, Some("sha256:2222"));
    runtime.tag(IMAGE_REF, IMAGE_ID);
    runtime.add_volume(VOLUME);
    runtime
}

#[derive(Debug, Default)]
struct RecordingRunner {
    seen: Mutex<Vec<RunnerRequest>>,
    failing: Mutex<Option<String>>,
    filters: Mutex<bool>,
    edits: Mutex<bool>,
}

impl RecordingRunner {
    fn filtering() -> Self {
        let runner = Self::editing();
        *runner
            .filters
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = true;
        runner
    }

    fn editing() -> Self {
        let runner = Self::default();
        *runner.edits.lock().unwrap_or_else(PoisonError::into_inner) = true;
        runner
    }

    fn failing(program: &str) -> Self {
        let runner = Self::default();
        *runner
            .failing
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(program.to_owned());
        runner
    }

    fn requests(&self) -> Vec<RunnerRequest> {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Runner for RecordingRunner {
    fn run(&self, request: &RunnerRequest) -> Result<ProcessOutput, RunnerError> {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(request.clone());
        let failing = self
            .failing
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let code = if failing.as_deref() == Some(request.command.program.as_str()) {
            127
        } else {
            0
        };
        if code == 0
            && request.role == crate::runner::ExecutionRole::Implement
            && *self.edits.lock().unwrap_or_else(PoisonError::into_inner)
        {
            crate::workspace_manager::fixture::write_file(
                &request.workspace.join("worker.txt"),
                b"the worker's edit\n",
            );
            if *self.filters.lock().unwrap_or_else(PoisonError::into_inner) {
                crate::workspace_manager::fixture::write_file(
                    &request.workspace.join(".gitattributes"),
                    b"* filter=upstroke-test\n",
                );
            }
        }
        Ok(ProcessOutput {
            code: Some(code),
            stdout: "1.2.3".to_owned(),
            stderr: String::new(),
            duration: Duration::from_millis(1),
            timed_out: false,
            output_limited: false,
        })
    }
}

#[derive(Debug)]
struct StubAdapter;

impl AgentAdapter for StubAdapter {
    fn id(&self) -> &'static str {
        AGENT
    }

    fn probe(&self, runner: &dyn Runner) -> Result<Caps, UpstrokeError> {
        let request = crate::agent::probe_request(
            AGENT,
            CommandSpec::new("claude").arg("--version"),
            0,
            Duration::from_secs(30),
        )?;
        let output = runner.run(&request)?;
        if output.code != Some(0) {
            return Err(UpstrokeError::Agent {
                message: format!("`claude --version` exited {:?}", output.code),
            });
        }
        Ok(Caps {
            version: output.stdout.trim().to_owned(),
            json_output: true,
            session_resume: true,
            cost_reporting: true,
            read_only_mode: true,
            acp: false,
            model_list: true,
        })
    }

    fn build(&self, _run: &TaskRun) -> Result<CommandSpec, UpstrokeError> {
        Ok(CommandSpec::new("claude"))
    }

    fn parse(&self, _out: &ProcessOutput) -> Result<Outcome, UpstrokeError> {
        Err(UpstrokeError::Agent {
            message: "the fixture adapter runs no attempt".to_owned(),
        })
    }
}

struct StubAdapters;

impl AdapterSource for StubAdapters {
    fn get(&self, id: &str) -> Option<&dyn AgentAdapter> {
        (id == AGENT).then_some(&StubAdapter as &dyn AgentAdapter)
    }
}

struct AlwaysCertifies;

impl RunnerPreflight for AlwaysCertifies {
    fn certify(&self, _policy: &RunnerPolicy) -> Result<(), UpstrokeError> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RefShape {
    Direct,
    Symbolic,
    CheckedOut,
}

struct RecordingRefs {
    log: PathBuf,
    shape: RefShape,
    at: Mutex<Option<String>>,
    created: Mutex<Vec<(String, String)>>,
    targets_read: Mutex<usize>,
    entered: Mutex<Vec<Vec<u8>>>,
}

impl RecordingRefs {
    fn with_log(log: &Path, shape: RefShape, at: Option<String>) -> Self {
        Self {
            log: log.to_path_buf(),
            shape,
            at: Mutex::new(at),
            created: Mutex::new(Vec::new()),
            targets_read: Mutex::new(0),
            entered: Mutex::new(Vec::new()),
        }
    }

    fn absent(fixture: &Fixture) -> Self {
        Self::with_log(&fixture.log(), RefShape::Direct, None)
    }

    fn at(fixture: &Fixture, sha: &str) -> Self {
        Self::with_log(&fixture.log(), RefShape::Direct, Some(sha.to_owned()))
    }

    fn shaped(fixture: &Fixture, shape: RefShape) -> Self {
        Self::with_log(&fixture.log(), shape, None)
    }

    fn created(&self) -> Vec<(String, String)> {
        self.created
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn target(&self) -> Option<String> {
        self.at
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn targets_read(&self) -> usize {
        *self
            .targets_read
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn log_bytes_at_entries(&self) -> Vec<Vec<u8>> {
        self.entered
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn log_kinds_at_entries(&self) -> Vec<Vec<String>> {
        self.log_bytes_at_entries()
            .iter()
            .map(|bytes| event_kinds(bytes))
            .collect()
    }
}

impl IntegrationRefs for RecordingRefs {
    fn assert_publishable(&self, refname: &str) -> Result<(), UpstrokeError> {
        match self.shape {
            RefShape::Direct => Ok(()),
            RefShape::Symbolic => Err(Refusal::SymbolicRef {
                refname: refname.to_owned(),
                target: "refs/heads/somebody-elses-branch".to_owned(),
            }
            .into()),
            RefShape::CheckedOut => Err(Refusal::CheckedOutRef {
                refname: refname.to_owned(),
                worktree: PathBuf::from("worktrees").join("alpha"),
            }
            .into()),
        }
    }

    fn direct_target(&self, refname: &str) -> Result<Option<String>, UpstrokeError> {
        *self
            .targets_read
            .lock()
            .unwrap_or_else(PoisonError::into_inner) += 1;
        if self.shape == RefShape::Symbolic {
            return Err(Refusal::SymbolicRef {
                refname: refname.to_owned(),
                target: "refs/heads/somebody-elses-branch".to_owned(),
            }
            .into());
        }
        Ok(self.target())
    }

    fn create_zero_old(
        &self,
        hooks: &mut dyn crate::workspace_manager::EffectHooks,
        refname: &str,
        new: &str,
    ) -> Result<(), UpstrokeError> {
        crate::workspace_manager::refuse_new(refname, new)?;
        let site = EffectSiteId::Ref(RefSite::CreateIntegration);
        self.entered
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(crate::util::read_file_bounded(&self.log).unwrap_or_default());
        injected(
            hooks.phase(site, HookPhase::Before),
            site,
            HookPhase::Before,
        )?;
        {
            let mut at = self.at.lock().unwrap_or_else(PoisonError::into_inner);
            if at.is_some() {
                return Err(UpstrokeError::Git {
                    message: format!("`{refname}` already exists; zero-old refuses"),
                });
            }
            *at = Some(new.to_owned());
        }
        self.created
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((refname.to_owned(), new.to_owned()));
        injected(hooks.phase(site, HookPhase::After), site, HookPhase::After)
    }
}

#[test]
fn the_recording_refs_refuse_a_null_new_value_as_the_real_primitive_does() {
    let refs = RecordingRefs::with_log(Path::new("no-log"), RefShape::Direct, None);
    let null = "0".repeat(40);
    let error = refs
        .create_zero_old(
            &mut crate::workspace_manager::NoHooks,
            "refs/heads/upstroke/run-1",
            &null,
        )
        .expect_err("the double refuses a null new value");
    assert!(
        error.to_string().contains("null object id"),
        "the refusal must name its reason: {error}"
    );
    assert_eq!(refs.created(), Vec::<(String, String)>::new());
    assert_eq!(refs.target(), None);
    assert_eq!(
        refs.log_bytes_at_entries(),
        Vec::<Vec<u8>>::new(),
        "the funnel was not entered"
    );
}

fn injected(
    injection: Injection,
    site: EffectSiteId,
    phase: HookPhase,
) -> Result<(), UpstrokeError> {
    match injection {
        Injection::Proceed => Ok(()),
        Injection::Kill => std::process::abort(),
        Injection::Error => Err(UpstrokeError::Refused {
            message: format!("the `{site}` funnel was made to fail at its `{phase}` phase"),
        }),
    }
}

fn container_selection() -> RunnerSelection {
    RunnerSelection {
        kind: RunnerKind::Container,
        image: Some(IMAGE_REF.to_owned()),
        credential_volumes: [(AGENT.to_owned(), VOLUME.to_owned())]
            .into_iter()
            .collect(),
        mounts: Vec::new(),
        from_config: true,
    }
}

struct ArmedHooks {
    inner: HarnessTopologyHooks,
    rundir: ArmedRunDir,
}

struct ArmedRunDir {
    harness: Arc<Mutex<HookHarness>>,
    site: RunDirSite,
    phase: HookPhase,
    nth: usize,
    seen: usize,
}

impl ArmedHooks {
    fn new(
        harness: &Arc<Mutex<HookHarness>>,
        (site, phase, nth): (RunDirSite, HookPhase, usize),
    ) -> Self {
        Self {
            inner: HarnessTopologyHooks::new(Arc::clone(harness)),
            rundir: ArmedRunDir {
                harness: Arc::clone(harness),
                site,
                phase,
                nth,
                seen: 0,
            },
        }
    }
}

impl rundir::RunDirHooks for ArmedRunDir {
    fn hook(&mut self, site: EffectSiteId, phase: HookPhase) -> Injection {
        self.harness
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .hook(site, phase);
        if site != EffectSiteId::RunDir(self.site) || phase != self.phase {
            return Injection::Proceed;
        }
        self.seen += 1;
        if self.seen == self.nth {
            Injection::Error
        } else {
            Injection::Proceed
        }
    }
}

impl TopologyHooks for ArmedHooks {
    fn effects(&mut self) -> &mut dyn crate::workspace_manager::EffectHooks {
        self.inner.effects()
    }

    fn rundir(&mut self) -> &mut dyn rundir::RunDirHooks {
        &mut self.rundir
    }

    fn events(&mut self) -> &mut dyn crate::events::log::EventHooks {
        self.inner.events()
    }

    fn container(&mut self) -> &mut dyn crate::runner::container::ContainerHooks {
        self.inner.container()
    }

    fn spawn(&mut self) -> &mut dyn crate::agent::proc::SpawnHooks {
        self.inner.spawn()
    }
}

struct Given<'a> {
    runtime: &'a dyn ContainerRuntime,
    preflight: &'a dyn RunnerPreflight,
    today: RunnerSelection,
    inputs: FrozenInputs,
    explicit_root: Option<PathBuf>,
    refs: RecordingRefs,
}

impl<'a> Given<'a> {
    fn healthy(
        fixture: &Fixture,
        runtime: &'a FakeRuntime,
        preflight: &'a dyn RunnerPreflight,
    ) -> Self {
        Self {
            runtime,
            preflight,
            today: container_selection(),
            inputs: fixture.inputs(),
            explicit_root: None,
            refs: RecordingRefs::absent(fixture),
        }
    }
}

fn resume(
    fixture: &Fixture,
    harness: &Arc<Mutex<HookHarness>>,
    given: &Given<'_>,
) -> (Result<Recovered, UpstrokeError>, Vec<String>) {
    let (outcome, warnings) = resume_holding(fixture, harness, given);
    (outcome.map(|(recovered, _handle)| recovered), warnings)
}

fn resume_holding(
    fixture: &Fixture,
    harness: &Arc<Mutex<HookHarness>>,
    given: &Given<'_>,
) -> (Result<(Recovered, RunHandle), UpstrokeError>, Vec<String>) {
    let mut hooks = HarnessTopologyHooks::new(Arc::clone(harness)).recording_durability();
    resume_with(fixture, &mut hooks, given)
}

fn resume_with(
    fixture: &Fixture,
    hooks: &mut dyn TopologyHooks,
    given: &Given<'_>,
) -> (Result<(Recovered, RunHandle), UpstrokeError>, Vec<String>) {
    let liveness = FakeOwnerLiveness::new();
    let view = DisposableDirView::new(ContainerTrace::default());
    let incarnation = IncarnationId(RESUMER.to_owned());
    let manager = fixture.manager();
    let mut warnings = Vec::new();
    let outcome = fixture
        .derive(given.explicit_root.as_deref())
        .and_then(|root| {
            run_recovery_order(
                root,
                &ResumeSeams {
                    repo_root: &fixture.repo_root,
                    worktree_git_dir: &fixture.git_dir,
                    repo_key: &fixture.repo_key,
                    incarnation: &incarnation,
                    inputs: given.inputs.clone(),
                    today: &given.today,
                    runtime: given.runtime,
                    liveness: &liveness,
                    view: &view,
                    preflight: given.preflight,
                    refs: &given.refs,
                    manager: &manager,
                    clock: &Frozen,
                },
                hooks,
                &mut warnings,
            )
        });
    (outcome, warnings)
}

fn harness() -> Arc<Mutex<HookHarness>> {
    Arc::new(Mutex::new(HookHarness::new()))
}

fn any_lock_site_ran(harness: &Arc<Mutex<HookHarness>>) -> Vec<&'static str> {
    let seen = harness.lock().unwrap_or_else(PoisonError::into_inner);
    LockSite::ALL
        .iter()
        .copied()
        .filter(|site| seen.touched(EffectSiteId::Lock(*site)))
        .map(LockSite::name)
        .collect()
}

fn first_observation(harness: &Arc<Mutex<HookHarness>>, site: EffectSiteId) -> Option<usize> {
    let seen = harness.lock().unwrap_or_else(PoisonError::into_inner);
    seen.coverage()
        .iter()
        .position(|observation| observation.site == site)
}

fn message(error: &UpstrokeError) -> String {
    error.to_string()
}

#[test]
fn resume_with_explicit_private_root_mismatch_refused_before_any_lock() {
    let fixture = Fixture::healthy("explicit-root");
    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let mut given = Given::healthy(&fixture, &runtime, &certifies);
    given.explicit_root = Some(fixture.root.join("somewhere-else"));

    let (outcome, _) = resume(&fixture, &harness, &given);

    let error = outcome.expect_err("a root the run did not record is refused");
    let text = message(&error);
    assert!(
        text.contains(&fixture.private_root.display().to_string()),
        "the refusal must name the recorded root: {text}"
    );
    assert!(
        text.contains("somewhere-else"),
        "the refusal must name the root that was asked for: {text}"
    );
    assert!(
        any_lock_site_ran(&harness).is_empty(),
        "a refusal at (a0) precedes Lock.AcquireWorktree, so no R17 hold is taken: {:?}",
        any_lock_site_ran(&harness)
    );
    assert!(
        !fixture.worktree_lock_file().exists(),
        "no R25 lock file is created by a refusal that precedes the acquisition"
    );
}

#[test]
fn malformed_recorded_locator_refused_before_any_lock() {
    for (tag, locator) in [
        ("no-runs", format!("/tmp/upstroke-pr7e-root/{RUN_ID}")),
        (
            "wrong-tail",
            "/tmp/upstroke-pr7e-root/runs/another".to_owned(),
        ),
        (
            "escapes",
            format!("/tmp/upstroke-pr7e-root/runs/other/../runs/{RUN_ID}"),
        ),
    ] {
        let fixture = Fixture::build(
            &format!("locator-{tag}"),
            Damage {
                locator: Some(locator.clone()),
                ..Damage::default()
            },
        );
        let harness = harness();
        let runtime = runtime_holding_the_record();
        let certifies = AlwaysCertifies;
        let given = Given::healthy(&fixture, &runtime, &certifies);

        let (outcome, _) = resume(&fixture, &harness, &given);

        let error = outcome.expect_err("a locator of another shape is refused");
        let text = message(&error);
        assert!(
            text.contains(&locator) && text.contains("is not of the shape"),
            "the refusal must quote the locator it refused ({tag}): {text}"
        );
        assert!(
            any_lock_site_ran(&harness).is_empty(),
            "no R17 hold is taken for a locator refusal ({tag}): {:?}",
            any_lock_site_ran(&harness)
        );
        assert!(
            !fixture.worktree_lock_file().exists(),
            "no R25 lock file is created for a locator refusal ({tag})"
        );
    }
}

#[test]
fn resume_derives_private_root_from_record_when_default_changed() {
    let fixture = Fixture::healthy("nondefault-root");
    let root = fixture.derive(None).expect("(a0) derives");

    let canonical =
        |path: &std::path::Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    assert_eq!(
        canonical(root.private_root()),
        canonical(&fixture.private_root),
        "the authorized root is the one `run_started.private_dir` names"
    );
    assert_ne!(
        root.private_root(),
        rundir::default_private_root(),
        "the fixture must not accidentally be the default root, or this test proves nothing"
    );
    assert_eq!(root.run_id(), RUN_ID);
    assert_eq!(root.first_line(), fixture.first_line.as_slice());
}

#[test]
fn resume_refuses_missing_private_half() {
    let fixture = Fixture::build(
        "no-private-half",
        Damage {
            no_private_half: true,
            ..Damage::default()
        },
    );
    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);

    let (outcome, _) = resume(&fixture, &harness, &given);

    let text = message(&outcome.expect_err("a missing private half refuses"));
    assert!(
        text.contains("is not recreated"),
        "the refusal says the half is not recreated: {text}"
    );
    assert!(
        !fixture.private_root.join("runs").join(RUN_ID).exists(),
        "the private half must still be absent: nothing recreates it"
    );
}

#[test]
fn resume_refuses_missing_or_disagreeing_owner_record() {
    let cases: Vec<(&str, Damage, &str)> = vec![
        (
            "absent",
            Damage {
                no_owner_record: true,
                ..Damage::default()
            },
            "owner.json",
        ),
        (
            "run-id",
            Damage {
                owner: Some(|owner| owner.run_id = "01KZTPR7E00000000000000009".to_owned()),
                ..Damage::default()
            },
            "run id",
        ),
        (
            "repo-key",
            Damage {
                owner: Some(|owner| owner.repo_key = "0123456789abcdef".to_owned()),
                ..Damage::default()
            },
            "repo key",
        ),
        (
            "public-dir",
            Damage {
                owner: Some(|owner| owner.public_dir = "/elsewhere/runs/x".to_owned()),
                ..Damage::default()
            },
            "public directory",
        ),
        (
            "incarnation",
            Damage {
                owner: Some(|owner| owner.incarnation = RESUMER.to_owned()),
                ..Damage::default()
            },
            "incarnation",
        ),
    ];
    for (tag, damage, expected) in cases {
        let fixture = Fixture::build(&format!("owner-{tag}"), damage);
        let harness = harness();
        let runtime = runtime_holding_the_record();
        let certifies = AlwaysCertifies;
        let given = Given::healthy(&fixture, &runtime, &certifies);

        let (outcome, _) = resume(&fixture, &harness, &given);

        let text = message(&outcome.expect_err("a disagreeing owner record refuses"));
        assert!(
            text.contains(expected),
            "the refusal for `{tag}` must name `{expected}`: {text}"
        );
        let private = fixture.private_root.join("runs").join(RUN_ID);
        assert!(
            !private.join("questions").exists() && !private.join("report.json").exists(),
            "a record refusal precedes every private write ({tag})"
        );
    }
}

#[test]
fn resume_refuses_commit_record_digest_mismatch() {
    let fixture = Fixture::build(
        "commit-digest",
        Damage {
            commit: Some(|commit| {
                commit.run_started_sha256 = format!("sha256:{}", "0".repeat(64));
            }),
            ..Damage::default()
        },
    );
    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);

    let (outcome, _) = resume(&fixture, &harness, &given);

    let text = message(&outcome.expect_err("a commit record that names another line refuses"));
    let actual = rundir::run_started_sha256(&fixture.first_line);
    assert!(
        text.contains(&format!("sha256:{}", "0".repeat(64))) && text.contains(&actual),
        "the refusal quotes what the record says and what the line digests: {text}"
    );
    assert!(
        !harness
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .touched(EffectSiteId::Event(EventSite::OpenLog)),
        "a commit-record refusal is at (a) and precedes the barrier's Event.OpenLog"
    );
}

#[test]
fn resume_refuses_owner_record_runner_mismatch() {
    let fixture = Fixture::build(
        "owner-runner",
        Damage {
            owner: Some(|owner| {
                if let Some(image) = owner.runner.image.as_mut() {
                    image.reference = "ghcr.io/example/another:9.9".to_owned();
                }
            }),
            ..Damage::default()
        },
    );
    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);

    let (outcome, _) = resume(&fixture, &harness, &given);

    let text = message(&outcome.expect_err("a runner the two records disagree on refuses"));
    assert!(
        text.contains("image reference"),
        "the refusal names which field moved: {text}"
    );
    assert!(
        !harness
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .touched(EffectSiteId::Event(EventSite::OpenLog)),
        "the runner comparison is at (a), before the barrier"
    );
}

#[test]
fn resume_refuses_digest_mismatch() {
    let fixture = Fixture::healthy("digest-mismatch");
    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let mut given = Given::healthy(&fixture, &runtime, &certifies);
    given.inputs.normalized_plan_digest = "sha256:not-the-recorded-one".to_owned();

    let before = fixture.log_bytes();
    let (outcome, _) = resume(&fixture, &harness, &given);

    let text = message(&outcome.expect_err("a moved plan digest refuses"));
    assert!(
        text.contains(BarrierStep::CheckedReplay.name()),
        "the refusal names the barrier step: {text}"
    );
    assert!(
        text.contains("normalized plan"),
        "and the digest that disagreed: {text}"
    );
    assert_eq!(
        fixture.log_bytes(),
        before,
        "a barrier refusal appends nothing"
    );
    let seen = harness.lock().unwrap_or_else(PoisonError::into_inner);
    assert!(
        !seen.touched(EffectSiteId::RunDir(RunDirSite::RemoveMarker)),
        "no census effect follows a refused replay"
    );
}

#[test]
fn resume_establishes_stable_prefix_barrier_before_any_fold_derived_effect() {
    let fixture = Fixture::healthy("barrier-order");
    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);

    assert_eq!(
        rundir::run_dir_names(&fixture.repo_root),
        vec![RUN_ID.to_owned()],
        "the anchor is the census's first effect only while this run's \
         directory is the only one in the tree"
    );

    let (outcome, _) = resume(&fixture, &harness, &given);
    outcome.expect("the healthy resume completes");

    let marker = first_observation(&harness, EffectSiteId::RunDir(RunDirSite::RemoveMarker))
        .expect("the census removes this run's stale marker");
    let open = first_observation(&harness, EffectSiteId::Event(EventSite::OpenLog))
        .expect("Event.OpenLog ran");
    let proven = first_observation(&harness, EffectSiteId::Event(EventSite::ProvePrefixStable))
        .expect("Event.ProvePrefixStable ran");
    let append = first_observation(&harness, EffectSiteId::Event(EventSite::Append))
        .or_else(|| first_observation(&harness, EffectSiteId::Event(EventSite::AppendFirst)));

    assert!(
        open < marker,
        "Event.OpenLog ({open}) before the census ({marker})"
    );
    assert!(
        proven < marker,
        "Event.ProvePrefixStable ({proven}) before the census ({marker})"
    );
    if let Some(append) = append {
        assert!(
            proven < append,
            "the barrier ({proven}) before every recovery event ({append}) — O33 and O18"
        );
    }
    let seen = harness.lock().unwrap_or_else(PoisonError::into_inner);
    assert!(
        seen.reached_point(
            EffectSiteId::Event(EventSite::OpenLog),
            SubEffectPoint::SyncPrefix,
            InjectionMode::ErrorReturn
        ),
        "the SyncPrefix point is consulted, which is what makes it armable"
    );
}

#[test]
fn resume_refuses_before_any_fold_derived_effect_when_prefix_sync_fails() {
    let fixture = Fixture::healthy("sync-fails");
    let harness = harness();
    harness
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .arm(
            EffectSiteId::Event(EventSite::OpenLog),
            SubEffectPoint::SyncPrefix,
            InjectionMode::ErrorReturn,
        )
        .expect("SyncPrefix supports an error return");
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);

    let before = fixture.log_bytes();
    let (outcome, _) = resume(&fixture, &harness, &given);

    let text = message(&outcome.expect_err("a failed SyncPrefix refuses"));
    assert!(
        text.contains(BarrierStep::SyncPrefix.name()),
        "the refusal names the failed step: {text}"
    );
    assert_eq!(fixture.log_bytes(), before, "no append handle was used");
    let seen = harness.lock().unwrap_or_else(PoisonError::into_inner);
    assert!(
        !seen.touched(EffectSiteId::RunDir(RunDirSite::RemoveMarker)),
        "no census reclaim follows a failed sync"
    );
    assert!(
        !seen.touched(EffectSiteId::Event(EventSite::Append)),
        "and no recovery event"
    );
}

const ALPHA: TaskKey = TaskKey(0);
const GEN: GenerationId = GenerationId(0);

fn dispatched_at(base: &CommitSha) -> TopologyEventBody {
    let TopologyEventBody::TaskDispatched { mut data } = dispatched() else {
        unreachable!("`dispatched` builds a `TaskDispatched`")
    };
    data.base_sha = base.clone();
    TopologyEventBody::TaskDispatched { data }
}

fn dispatched() -> TopologyEventBody {
    TopologyEventBody::TaskDispatched {
        data: TaskDispatched {
            key: ALPHA,
            generation: GEN,
            base_sha: CommitSha("a".repeat(40)),
            worktree_path: "wt/g0".to_owned(),
            lease: LeaseGrant::Predicted {
                paths: PathSet::Prefixes {
                    paths: vec![GitPath("src/alpha".to_owned())],
                },
            },
            source_candidate: None,
        },
    }
}

fn for_task(key: TaskKey, prefix: &str, body: TopologyEventBody) -> TopologyEventBody {
    match body {
        TopologyEventBody::TaskDispatched { mut data } => {
            data.key = key;
            data.worktree_path = format!("wt/{prefix}-g0");
            data.lease = LeaseGrant::Predicted {
                paths: PathSet::Prefixes {
                    paths: vec![GitPath(format!("src/{prefix}"))],
                },
            };
            TopologyEventBody::TaskDispatched { data }
        }
        TopologyEventBody::AttemptStarted { mut data } => {
            data.key = key;
            TopologyEventBody::AttemptStarted { data }
        }
        TopologyEventBody::AttemptFinished { mut data } => {
            data.key = key;
            TopologyEventBody::AttemptFinished { data }
        }
        other => other,
    }
}

fn in_generation(generation: GenerationId, body: TopologyEventBody) -> TopologyEventBody {
    match body {
        TopologyEventBody::TaskDispatched { mut data } => {
            data.generation = generation;
            data.worktree_path = format!("wt/g{}", generation.0);
            TopologyEventBody::TaskDispatched { data }
        }
        TopologyEventBody::AttemptStarted { mut data } => {
            data.generation = generation;
            TopologyEventBody::AttemptStarted { data }
        }
        TopologyEventBody::AttemptFinished { mut data } => {
            data.generation = generation;
            TopologyEventBody::AttemptFinished { data }
        }
        other => other,
    }
}

fn attempt_started(attempt: u32) -> TopologyEventBody {
    TopologyEventBody::AttemptStarted {
        data: AttemptStarted4 {
            key: ALPHA,
            generation: GEN,
            attempt: AttemptNumber(attempt),
            rung: 0,
            binding: RungBinding {
                tier: Tier::Mid,
                agent: AGENT.to_owned(),
                model: "claude-opus-5".to_owned(),
                pinned: false,
                effort: Effort::High,
            },
            pool: None,
            resume_session: None,
            materialization_observed: None,
        },
    }
}

/// The binding the fixture's own chain freezes for rung 0, so a planted
/// attempt agrees with whatever chain `Damage` selected.
fn planted_binding(fixture: &Fixture) -> RungBinding {
    let binding = fixture
        .started
        .chains
        .first()
        .and_then(|chain| chain.bindings.as_ref())
        .and_then(|bindings| bindings.first())
        .expect("the fixture's chain records a binding");
    RungBinding {
        tier: binding.tier,
        agent: binding.agent.clone(),
        model: binding.model.clone(),
        pinned: binding.pinned,
        effort: fixture
            .started
            .effort_policy
            .implementation_for(binding.tier),
    }
}

fn attempt_started_in(fixture: &Fixture, attempt: u32) -> TopologyEventBody {
    let TopologyEventBody::AttemptStarted { mut data } = attempt_started(attempt) else {
        panic!("attempt_started builds an attempt_started")
    };
    data.binding = planted_binding(fixture);
    TopologyEventBody::AttemptStarted { data }
}

fn attempt_record(attempt: u32) -> AttemptRecord {
    AttemptRecord {
        attempt,
        tier: "mid".to_owned(),
        model: "claude-opus-5".to_owned(),
        pool: None,
        resumed: false,
        duration: Duration::from_millis(5),
        cost_usd: Some(0.5),
        reviews: Vec::new(),
        session_id: None,
        usage: None,
        failure: None,
    }
}

fn attempt_finished(attempt: u32, settlement: AttemptSettlement) -> TopologyEventBody {
    let mut record = attempt_record(attempt);
    record.failure = Some(crate::events::FailureRecord {
        kind: crate::ladder::FailureKind::GateFailed,
        origin: crate::ladder::FailureOrigin::Worker,
        reason: "the fixture's judged failure".to_owned(),
        detail: None,
    });
    if let AttemptSettlement::Retained {
        retained_session, ..
    } = &settlement
    {
        record.session_id = Some(retained_session.0.clone());
    }
    TopologyEventBody::AttemptFinished {
        data: Box::new(AttemptFinished4 {
            key: ALPHA,
            generation: GEN,
            attempt: AttemptNumber(attempt),
            record: Box::new(record),
            settlement,
        }),
    }
}

fn attempt_finished_failing(
    attempt: u32,
    kind: crate::ladder::FailureKind,
    reason: &str,
    detail: &str,
    settlement: AttemptSettlement,
) -> TopologyEventBody {
    let TopologyEventBody::AttemptFinished { mut data } = attempt_finished(attempt, settlement)
    else {
        unreachable!("attempt_finished builds an attempt_finished")
    };
    data.record.failure = Some(crate::events::FailureRecord {
        kind,
        origin: crate::ladder::FailureOrigin::Worker,
        reason: reason.to_owned(),
        detail: Some(detail.to_owned()),
    });
    TopologyEventBody::AttemptFinished { data }
}

fn budget_exceeded(epoch: u32) -> TopologyEventBody {
    TopologyEventBody::BudgetExceeded {
        data: BudgetExceeded4 {
            epoch: Epoch(epoch),
            budget: BudgetKind::Run,
            limit_usd: 1.0,
            spent_usd: 2.0,
            key: Some(ALPHA),
        },
    }
}

fn run_finished(outcome: RunOutcome, halted_at: Option<TaskKey>) -> TopologyEventBody {
    TopologyEventBody::RunFinished {
        data: RunFinished4 {
            outcome,
            halted_at,
            merged: 0,
            parked: 0,
        },
    }
}

#[test]
fn resume_finalizes_halted_then_refuses() {
    for (tag, outcome, extra) in [
        (
            "halted",
            RunOutcome::Halted,
            vec![
                dispatched(),
                attempt_started(1),
                attempt_finished(
                    1,
                    AttemptSettlement::Closed {
                        transition: SettlementTransition::Failed {
                            halts_run: true,
                            reason: "the ladder ran out".to_owned(),
                        },
                        lease: LeaseDisposition::PredictedReleased,
                    },
                ),
                run_finished(RunOutcome::Halted, Some(ALPHA)),
            ],
        ),
        (
            "complete",
            RunOutcome::Complete,
            vec![
                dispatched(),
                attempt_started(1),
                attempt_finished(
                    1,
                    AttemptSettlement::Closed {
                        transition: SettlementTransition::Failed {
                            halts_run: false,
                            reason: "the ladder ran out and the policy does not halt".to_owned(),
                        },
                        lease: LeaseDisposition::PredictedReleased,
                    },
                ),
                run_finished(RunOutcome::Complete, None),
            ],
        ),
    ] {
        let fixture = Fixture::build(
            &format!("finished-{tag}"),
            Damage {
                extra,
                ..Damage::default()
            },
        );
        let harness = harness();
        let runtime = runtime_holding_the_record();
        let certifies = AlwaysCertifies;
        let given = Given::healthy(&fixture, &runtime, &certifies);

        let before = fixture.log_bytes();
        let (result, _) = resume(&fixture, &harness, &given);

        let text = message(&result.expect_err("a finished run does not continue"));
        assert!(
            text.contains("already finished"),
            "the refusal says the run is over ({tag}): {text}"
        );
        assert!(
            text.contains(match outcome {
                RunOutcome::Halted => "halted",
                _ => "complete",
            }),
            "and names the outcome ({tag}): {text}"
        );
        assert_eq!(
            fixture.log_bytes(),
            before,
            "a (b) refusal appends nothing ({tag})"
        );
        assert!(
            !fixture.public().join("report.json").exists(),
            "PR7 does not finalize: `RunDir.WriteReport` is `t_finalize`, out of this slice's \
             rows ({tag})"
        );
        assert!(
            !harness
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .touched(EffectSiteId::RunDir(RunDirSite::WriteReport)),
            "and the report site never ran ({tag})"
        );
    }
}

#[test]
fn resume_rebuilds_runner_from_record_and_warns_on_config_drift() {
    let fixture = Fixture::healthy("config-drift");
    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let mut given = Given::healthy(&fixture, &runtime, &certifies);
    given.today.credential_volumes = [(AGENT.to_owned(), "somebody-elses-volume".to_owned())]
        .into_iter()
        .collect();
    runtime.add_volume("somebody-elses-volume");

    let (result, _) = resume(&fixture, &harness, &given);
    let recovered = result.expect("a config that differs is a warning, not a refusal");

    assert!(
        recovered
            .warnings
            .iter()
            .any(|warning| warning.contains("credential volume set")),
        "the warning names which field differs: {:?}",
        recovered.warnings
    );
    let log = String::from_utf8(fixture.log_bytes()).expect("the log is utf-8");
    let resumed = log.lines().last().expect("run_resumed is last");
    assert!(
        resumed.contains(VOLUME) && !resumed.contains("somebody-elses-volume"),
        "run_resumed records the recorded runner, not today's config: {resumed}"
    );
}

#[test]
fn resume_warns_when_reference_moved_and_uses_recorded_image_id() {
    let fixture = Fixture::healthy("moved-reference");
    let harness = harness();
    let runtime = runtime_holding_the_record();
    let other = format!("sha256:{}", "3".repeat(64));
    runtime.add_image(&other, None);
    runtime.move_tag(IMAGE_REF, &other);
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);

    let (result, _) = resume(&fixture, &harness, &given);
    let recovered = result.expect("a moved reference is a warning, not a refusal");

    assert!(
        recovered
            .warnings
            .iter()
            .any(|warning| warning.contains(IMAGE_REF) && warning.contains(&other)),
        "the warning names the reference and where it now points: {:?}",
        recovered.warnings
    );
    let log = String::from_utf8(fixture.log_bytes()).expect("the log is utf-8");
    let resumed = log.lines().last().expect("run_resumed is last");
    assert!(
        resumed.contains(IMAGE_ID) && !resumed.contains(&other),
        "the run continues from its recorded image id: {resumed}"
    );
}

#[test]
fn resume_refuses_by_inspection_before_any_spawn_when_runtime_image_id_or_volume_absent() {
    struct NeverRuns;

    impl RunnerPreflight for NeverRuns {
        fn certify(&self, _policy: &RunnerPolicy) -> Result<(), UpstrokeError> {
            unreachable!("an inspection refusal precedes every spawn");
        }
    }

    type Damage = fn(&FakeRuntime);
    let cases: [(&str, Damage, &str); 3] = [
        (
            "runtime",
            |runtime| runtime.set_all_unreachable(),
            "cannot be reached",
        ),
        (
            "image-id",
            |runtime| runtime.move_tag(IMAGE_REF, "sha256:absent"),
            "no longer holds the recorded image id",
        ),
        (
            "volume",
            |runtime| runtime.remove_volume(VOLUME),
            "credential volume",
        ),
    ];
    for (tag, damage, expected) in cases {
        let fixture = Fixture::healthy(&format!("inspection-{tag}"));
        let harness = harness();
        let runtime = runtime_holding_the_record();
        if tag == "image-id" {
            runtime.add_image("sha256:absent", None);
        }
        damage(&runtime);
        if tag == "image-id" {
            let fresh = FakeRuntime::new(ContainerTrace::default());
            fresh.add_image("sha256:absent", None);
            fresh.tag(IMAGE_REF, "sha256:absent");
            fresh.add_volume(VOLUME);
            let never = NeverRuns;
            let mut given = Given::healthy(&fixture, &runtime, &never);
            given.runtime = &fresh;
            let (result, _) = resume(&fixture, &harness, &given);
            let text = message(&result.expect_err("an absent recorded id refuses"));
            assert!(
                text.contains(expected) && text.contains(IMAGE_ID),
                "the refusal names the recorded id ({tag}): {text}"
            );
            continue;
        }
        let never = NeverRuns;
        let given = Given::healthy(&fixture, &runtime, &never);
        let (result, _) = resume(&fixture, &harness, &given);
        let text = message(&result.expect_err("an inspection refusal"));
        assert!(
            text.contains(expected),
            "the refusal names what could not be re-established ({tag}): {text}"
        );
    }
}

fn chain_to_census(
    fixture: &Fixture,
    harness: &Arc<Mutex<HookHarness>>,
    runtime: &dyn ContainerRuntime,
    incarnation: &IncarnationId,
) -> Result<ResumeCensused, UpstrokeError> {
    let mut hooks = HarnessTopologyHooks::new(Arc::clone(harness));
    chain_to_census_with(fixture, &mut hooks, runtime, incarnation)
}

fn chain_to_census_with(
    fixture: &Fixture,
    hooks: &mut dyn TopologyHooks,
    runtime: &dyn ContainerRuntime,
    incarnation: &IncarnationId,
) -> Result<ResumeCensused, UpstrokeError> {
    let liveness = FakeOwnerLiveness::new();
    let view = DisposableDirView::new(ContainerTrace::default());
    let mut warnings = Vec::new();
    let root = fixture.derive(None)?;
    let locks = LocksHeld::take(root, &fixture.repo_root, &fixture.git_dir, hooks.rundir())?;
    let records = RecordsVerified::verify(locks, &fixture.repo_key)?;
    let log_path = records.locks().root().log_path();
    let committed = records.commit().run_started_sha256.clone();
    let prefix = crate::events::log::establish_stable_prefix(
        &log_path,
        fixture.inputs(),
        Some(&committed),
        &mut warnings,
        hooks.events(),
    )?;
    let barrier = BarrierHeld::from(records, prefix)?;
    ResumeCensused::census(
        barrier,
        &CensusSeams {
            incarnation,
            repo_root: &fixture.repo_root,
            repo_key: &fixture.repo_key,
            runtime,
            liveness: &liveness,
            view: &view,
        },
        hooks,
    )
}

#[test]
fn resume_of_nondefault_root_run_reclaims_earlier_incarnation_intents_in_recorded_root() {
    let fixture = Fixture::healthy("earlier-incarnation");
    let harness = harness();
    let runtime = runtime_holding_the_record();

    let invocation = crate::runner::InvocationId::probe(
        crate::runner::ProbeTarget::Agent(crate::runner::AgentId::new(AGENT)),
        0,
    )
    .expect("the agent probe identity");
    let name = crate::runner::container::intent::ContainerName::new(
        fixture.repo_key.as_str(),
        RUN_ID,
        CREATOR,
        &invocation,
    )
    .expect("a container name for the creator incarnation");
    let record = crate::runner::container::intent::ContainerIntent::new(
        RUN_ID.to_owned(),
        &fixture.public(),
        CREATOR.to_owned(),
        fixture.repo_key.as_str().to_owned(),
        invocation.render(),
        crate::runner::policy::runner_policy_sha256(&fixture.started.runner),
    );
    let mut container_hooks = crate::runner::container::NoHooks;
    crate::runner::container::write_intent(
        &mut container_hooks,
        crate::topology::effects::ContainerSite::WriteIntent,
        &fixture.private_root,
        &name,
        &record,
    )
    .expect("the container funnel writes the intent");
    runtime.seed_container(
        name.as_str(),
        record.labels(&fixture.private_root),
        IMAGE_ID,
        IMAGE_ID,
        crate::runner::container::runtime::Liveness::Running,
    );

    let censused = chain_to_census(
        &fixture,
        &harness,
        &runtime,
        &IncarnationId(RESUMER.to_owned()),
    )
    .expect("the census completes");
    let report = censused.containers();

    assert_eq!(
        report.private_root, fixture.private_root,
        "the census scanned the recorded root, not today's default"
    );
    assert!(
        report
            .reclaimed
            .iter()
            .any(|entry| entry.name == name && entry.incarnation == CREATOR),
        "the creator incarnation's container is dead by construction and is reclaimed: {:?}",
        report.reclaimed
    );
    assert!(
        !runtime
            .container_names()
            .contains(&name.as_str().to_owned()),
        "and it is gone from the runtime"
    );
}

#[test]
fn resume_reclaims_a_provable_husk_beside_the_run_and_retains_a_possibly_committed_one() {
    const RECLAIMED: &str = "01KZTHUSK00000000000000002";
    const RETAINED: &str = "01KZTKEEP00000000000000003";

    let fixture = Fixture::healthy("husk-beside");
    let harness = harness();
    let runtime = runtime_holding_the_record();

    let reclaimed = plant_husk(&fixture, RECLAIMED, false);
    let retained = plant_husk(&fixture, RETAINED, true);
    let retained_before = tree_bytes(&retained.private);
    assert!(
        !retained_before.is_empty(),
        "the retained husk must have a private half, or its comparison proves nothing"
    );

    let censused = chain_to_census(
        &fixture,
        &harness,
        &runtime,
        &IncarnationId(RESUMER.to_owned()),
    )
    .expect("the census completes");
    let report = censused.run_dirs();

    assert_eq!(
        report
            .of(RECLAIMED)
            .expect("the provable husk is censused")
            .outcome,
        RunDirOutcome::ReclaimedBothHalves,
        "a resume reclaims under the ownership proof; it does not merely report"
    );
    assert!(!reclaimed.private.exists(), "the private half is gone");
    assert!(!reclaimed.public.exists(), "and so is the public directory");

    let private_at = first_observation(
        &harness,
        EffectSiteId::RunDir(RunDirSite::RemovePrivateHusk),
    )
    .expect("the private half went through the proof-token funnel");
    let public_at = first_observation(&harness, EffectSiteId::RunDir(RunDirSite::RemovePublicHusk))
        .expect("and the public directory through its own");
    assert!(
        private_at < public_at,
        "the private half first ({private_at}), the public directory with the marker last \
         ({public_at})"
    );

    assert_eq!(
        report
            .of(RETAINED)
            .expect("the retained husk is censused")
            .outcome,
        RunDirOutcome::Retained(RetainReason::PossiblyCommitted),
    );
    assert_eq!(
        tree_bytes(&retained.private),
        retained_before,
        "nothing private that carries a commit record is deleted by any census"
    );
    assert!(retained.public.exists(), "nor is its public half");

    assert_eq!(
        report
            .of(RUN_ID)
            .expect("the resuming run is censused too")
            .outcome,
        RunDirOutcome::RepairedStaleMarker,
    );
    assert!(!fixture.public().join(rundir::MARKER).exists());
    assert!(fixture.log().exists(), "and the run itself is untouched");
}

#[test]
fn resume_completes_past_a_husk_whose_private_half_cannot_be_removed() {
    const STUCK: &str = "01AAAASTUCK000000000000000";
    assert!(STUCK < RUN_ID, "the husk must sort before this run's id");

    let fixture = Fixture::healthy("husk-unreclaimable");
    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);

    let stuck = plant_husk(&fixture, STUCK, false);
    let before = tree_bytes(&stuck.private);
    assert!(
        !before.is_empty(),
        "the husk must have a private half, or its comparison proves nothing"
    );
    assert!(
        fixture.public().join(rundir::MARKER).exists(),
        "this run's own stale marker must be there, or the second claim is vacuous"
    );

    let mut hooks = ArmedHooks::new(
        &harness,
        (RunDirSite::RemovePrivateHusk, HookPhase::Before, 1),
    );
    let (outcome, _) = resume_with(&fixture, &mut hooks, &given);

    outcome.expect("a husk beside the run cannot end the resume");

    assert!(stuck.public.exists(), "the public half was removed anyway");
    assert!(
        stuck.public.join(rundir::MARKER).exists(),
        "`.creating` is the private half's only locator and it is gone"
    );
    assert_eq!(
        tree_bytes(&stuck.private),
        before,
        "the arming is `Before`, so the removal never ran"
    );

    assert!(
        !fixture.public().join(rundir::MARKER).exists(),
        "the own-run stale-marker repair was skipped because a husk sorting \
         earlier could not be reclaimed"
    );
    let seen = harness.lock().unwrap_or_else(PoisonError::into_inner);
    assert!(
        seen.touched(EffectSiteId::RunDir(RunDirSite::RemoveMarker)),
        "recovery step (a1)'s own repair never reached its funnel"
    );
    assert!(
        !seen.touched(EffectSiteId::RunDir(RunDirSite::RemovePublicHusk)),
        "the public half was removed after the private removal refused, which \
         orphans the private half permanently"
    );
}

#[test]
fn the_resume_census_reports_the_husk_it_could_not_reclaim() {
    const STUCK: &str = "01AAAASTUCK000000000000000";

    let fixture = Fixture::healthy("husk-unreclaimable-report");
    let harness = harness();
    let runtime = runtime_holding_the_record();
    let stuck = plant_husk(&fixture, STUCK, false);

    let mut hooks = ArmedHooks::new(
        &harness,
        (RunDirSite::RemovePrivateHusk, HookPhase::Before, 1),
    );
    let censused = chain_to_census_with(
        &fixture,
        &mut hooks,
        &runtime,
        &IncarnationId(RESUMER.to_owned()),
    )
    .expect("the census completes over a husk it could not reclaim");
    let report = censused.run_dirs();

    let entry = report.of(STUCK).expect("the husk is still an entry");
    let RunDirOutcome::Unreclaimable { step, detail } = &entry.outcome else {
        panic!("the failure is not an outcome: {:?}", entry.outcome);
    };
    assert_eq!(*step, FailedStep::PrivateHalf);
    assert!(!detail.is_empty(), "the error was dropped");
    assert!(
        !entry.outcome.deleted_a_private_half(),
        "a removal that returned an error claims the half is gone"
    );
    assert!(
        entry.outcome.may_have_deleted_a_private_half(),
        "a removal that may have emptied the tree reports it untouched"
    );
    assert_eq!(
        entry.locator.as_deref(),
        Some(stuck.private.as_path()),
        "retained and reported **with its locator**"
    );
    assert_eq!(report.unreclaimable().len(), 1);

    assert_eq!(
        report
            .of(RUN_ID)
            .expect("the resuming run is censused too")
            .outcome,
        RunDirOutcome::RepairedStaleMarker,
    );
}

#[test]
fn resume_refused_while_reaper_hold_observed_then_succeeds() {
    let fixture = Fixture::healthy("reaper-hold");

    #[cfg(unix)]
    {
        let cleanup = fixture.public().join("cleanup.lock");
        mkdir(&cleanup);
        let harness = harness();
        let runtime = runtime_holding_the_record();
        let certifies = AlwaysCertifies;
        let given = Given::healthy(&fixture, &runtime, &certifies);
        let (result, _) = resume(&fixture, &harness, &given);
        let text = message(&result.expect_err("a surviving reaper hold refuses"));
        assert!(
            text.contains("still cleaning agent processes"),
            "the refusal names the hold it observed: {text}"
        );
        assert!(
            harness
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .observed(
                    EffectSiteId::Lock(LockSite::ObserveCleanupHold),
                    HookPhase::Before
                ),
            "R28 is observed, never owned — and the site says so"
        );
        rundir::remove_public_husk(&cleanup, &mut NoHooks).expect("the reaper released its hold");
    }

    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);
    let (result, _) = resume(&fixture, &harness, &given);
    result.expect("with no hold observed, the resume proceeds");
    let seen = harness.lock().unwrap_or_else(PoisonError::into_inner);
    assert!(
        seen.observed(
            EffectSiteId::Lock(LockSite::ObserveCleanupHold),
            HookPhase::Before
        ),
        "the hold is observed on every resume, not only when one is held"
    );
    assert!(
        seen.observed(
            EffectSiteId::Lock(LockSite::AcquireWorktree),
            HookPhase::Before
        ) && seen.observed(EffectSiteId::Lock(LockSite::AcquireRun), HookPhase::Before),
        "and both R17 holds were taken"
    );
}

fn replayed(fixture: &Fixture) -> TopologyFold {
    let bytes = fixture.log_bytes();
    let events = TopologyFold::parse_log(&bytes).expect("the log parses");
    TopologyFold::replay(fixture.inputs(), &events).expect("and folds")
}

#[test]
fn resume_clears_budget_stop_and_wakes_deferred() {
    let fixture = Fixture::build(
        "budget-stop",
        Damage {
            extra: vec![
                dispatched(),
                attempt_started(1),
                attempt_finished(
                    1,
                    AttemptSettlement::Closed {
                        transition: SettlementTransition::Deferred {
                            defers: 1,
                            reason: "the pool was exhausted".to_owned(),
                        },
                        lease: LeaseDisposition::PredictedReleased,
                    },
                ),
                budget_exceeded(0),
            ],
            ..Damage::default()
        },
    );

    let before = replayed(&fixture);
    assert!(
        before.budget_stop().is_some(),
        "the fixture must carry a stop, or this test proves nothing"
    );
    assert_eq!(
        before.task_state(ALPHA),
        Some(TaskState::Deferred),
        "and a deferred task"
    );

    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);
    let (result, _) = resume(&fixture, &harness, &given);
    let recovered = result.expect("a budget-stopped run resumes");

    assert!(recovered.resumed.budget_stop_cleared);
    assert_eq!(
        recovered.resumed.epoch, 1,
        "the resume opens the next epoch"
    );
    let after = replayed(&fixture);
    assert!(
        after.budget_stop().is_none(),
        "the stop belongs to the epoch that hit the old ceiling"
    );
    assert_eq!(
        after.task_state(ALPHA),
        Some(TaskState::Pending),
        "and every Deferred task is woken by the resume"
    );
}

#[test]
fn steps_d_and_e_reach_every_generation_not_the_first() {
    const BETA: TaskKey = TaskKey(1);

    let fixture = Fixture::build(
        "loops-reach-every",
        Damage {
            two_tasks: true,
            extra: vec![
                dispatched(),
                attempt_started(1),
                attempt_finished(
                    1,
                    AttemptSettlement::Retained {
                        retained_session: SessionId("alpha-session".to_owned()),
                        retained_incarnation: Epoch(0),
                    },
                ),
                for_task(BETA, "beta", dispatched()),
                for_task(BETA, "beta", attempt_started(1)),
                for_task(
                    BETA,
                    "beta",
                    attempt_finished(
                        1,
                        AttemptSettlement::Retained {
                            retained_session: SessionId("beta-session".to_owned()),
                            retained_incarnation: Epoch(0),
                        },
                    ),
                ),
            ],
            ..Damage::default()
        },
    );

    let before = replayed(&fixture);
    assert!(
        before.ready_retry(ALPHA) && before.ready_retry(BETA),
        "both tasks must be retryable before the resume, or a `.take(1)` would \
         pass this test by accident"
    );

    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);
    let (result, _) = resume(&fixture, &harness, &given);
    let recovered = result.expect("a run with two retained sessions resumes");

    assert_eq!(
        recovered.retained_closed, 2,
        "step (e) closed {} of two retained generations — a loop that stops at \
         the first leaves the rest holding their entitlements for the whole run",
        recovered.retained_closed
    );

    let after = replayed(&fixture);
    for (key, name) in [(ALPHA, "alpha"), (BETA, "beta")] {
        assert!(
            !after.ready_retry(key),
            "{name}'s retained generation survived the resume"
        );
    }
}

/// `T-ATTEMPT.resume_action`: "the task worktree scrubbed with force". Step
/// (d) closes the interrupted attempt's generation; the worktree that
/// generation owned (R9) goes with the close.
#[test]
fn an_interrupted_attempts_worktree_and_intent_are_reclaimed_by_recovery() {
    let fixture = Fixture::build(
        "interrupted-worktree-reclaimed",
        Damage {
            open_generation: true,
            extra: vec![attempt_started(1)],
            ..Damage::default()
        },
    );
    let worktree = plant_task_worktree(&fixture, ALPHA, fixture.base_sha.as_str());
    let slot = crate::engine::topology::dispatch::task_slot(ALPHA, GEN);
    assert!(
        worktree.exists()
            && fixture
                .manager()
                .intents()
                .expect("intents")
                .contains(&slot),
        "the dead incarnation left the in-flight attempt's worktree and intent"
    );

    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);
    let (result, _) = resume(&fixture, &harness, &given);
    let recovered = result.expect("a run with an in-flight attempt resumes");
    assert_eq!(
        recovered.interrupted, 1,
        "(d) settles the attempt interrupted"
    );
    assert_eq!(
        replayed(&fixture).task_state(ALPHA),
        Some(TaskState::Pending),
        "and the task returns to Pending for a fresh generation"
    );
    assert!(
        !worktree.exists()
            && !fixture
                .manager()
                .intents()
                .expect("intents")
                .contains(&slot),
        "the closed generation's worktree and intent are reclaimed (worktree={}, intent={})",
        worktree.exists(),
        fixture
            .manager()
            .intents()
            .expect("intents")
            .contains(&slot)
    );
}

#[test]
fn retry_refused_after_resume() {
    let fixture = Fixture::build(
        "retained-retry",
        Damage {
            extra: vec![
                dispatched(),
                attempt_started(1),
                attempt_finished(
                    1,
                    AttemptSettlement::Retained {
                        retained_session: SessionId("session-of-the-dead-incarnation".to_owned()),
                        retained_incarnation: Epoch(0),
                    },
                ),
            ],
            ..Damage::default()
        },
    );

    let before = replayed(&fixture);
    assert!(
        before.ready_retry(ALPHA),
        "before the resume the retained generation is retryable, or this test proves nothing"
    );

    let retained_worktree = plant_task_worktree(&fixture, ALPHA, fixture.base_sha.as_str());
    let retained_slot = crate::engine::topology::dispatch::task_slot(ALPHA, GEN);

    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);
    let (result, _) = resume(&fixture, &harness, &given);
    let recovered = result.expect("a run with a retained session resumes");

    assert_eq!(
        recovered.retained_closed, 1,
        "step (e) closes every RetainedIdle generation"
    );
    let after = replayed(&fixture);
    assert!(
        !after.ready_retry(ALPHA),
        "the retained session is gone, so there is no same-session retry to take"
    );
    assert!(
        !retained_worktree.exists()
            && !fixture
                .manager()
                .intents()
                .expect("intents")
                .contains(&retained_slot),
        "an ordinary generation (e) closes is reclaimed like a repair's: the class, not \
         the instance the review walked (worktree={}, intent={})",
        retained_worktree.exists(),
        fixture
            .manager()
            .intents()
            .expect("intents")
            .contains(&retained_slot)
    );
    let refused = after
        .plan_transition(&event(attempt_started(2)))
        .expect_err("a retry into a closed generation is refused");
    assert!(
        format!("{refused}").contains("generation"),
        "the refusal is about the generation: {refused}"
    );
}

#[test]
fn run_resumed_records_identical_runner_identity() {
    let fixture = Fixture::healthy("identical-runner");
    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);

    let (result, _) = resume(&fixture, &harness, &given);
    result.expect("the healthy resume completes");

    let bytes = fixture.log_bytes();
    let events = TopologyFold::parse_log(&bytes).expect("the log parses");
    let resumed = events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            TopologyEventBody::RunResumed { data } => Some(data.clone()),
            _ => None,
        })
        .expect("the log ends with a run_resumed");

    assert_eq!(
        fixture.started.runner.difference(&resumed.runner),
        None,
        "the incarnation established exactly the recorded runner"
    );
    assert_eq!(resumed.incarnation.0, RESUMER, "and recorded its own id");
    assert_eq!(
        resumed.probed_agents, fixture.started.probed_agents,
        "and the agents its pre-flight certified"
    );
}

#[test]
fn forged_run_resumed_with_different_runner_identity_refused_on_replay() {
    let fixture = Fixture::healthy("forged-runner");
    let mut forged = fixture.started.runner.clone();
    if let Some(image) = forged.image.as_mut() {
        image.id = format!("sha256:{}", "9".repeat(64));
    }
    let mut warnings = Vec::new();
    let mut log =
        EventLog::open(EventSite::OpenLog, &fixture.log(), &mut warnings).expect("the log reopens");
    let (line, _) = TopologyLine::round_trip(&event(TopologyEventBody::RunResumed {
        data: Box::new(RunResumed4 {
            incarnation: IncarnationId(RESUMER.to_owned()),
            runner: forged,
            probed_agents: vec![AGENT.to_owned()],
            upstroke_version: env!("CARGO_PKG_VERSION").to_owned(),
        }),
    }))
    .expect("the forged event serializes — the wire format is not the check");
    log.append_topology(EventSite::Append, &line)
        .expect("nothing stops a forged line reaching the file");
    drop(log);

    let bytes = fixture.log_bytes();
    let events = TopologyFold::parse_log(&bytes).expect("the forged log still parses");
    let error = TopologyFold::replay(fixture.inputs(), &events)
        .expect_err("the checked fold refuses the forged identity");
    assert!(
        format!("{error}").contains("image id"),
        "the refusal names which field moved: {error}"
    );

    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);
    let (result, _) = resume(&fixture, &harness, &given);
    let text = message(&result.expect_err("the barrier refuses a forged prefix"));
    assert!(
        text.contains(BarrierStep::CheckedReplay.name()),
        "the refusal names the barrier step: {text}"
    );
}

#[test]
fn resume_after_append_error_follows_surviving_prefix() {
    let fixture = Fixture::healthy("append-error");
    let first = harness();
    first
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .arm(
            EffectSiteId::Event(EventSite::Append),
            SubEffectPoint::Synced,
            InjectionMode::ErrorReturn,
        )
        .expect("the Synced point supports an error return");
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);

    let lines_before = fixture.log_bytes().iter().filter(|b| **b == b'\n').count();
    let (result, _) = resume(&fixture, &first, &given);
    let text = message(&result.expect_err("an errored append ends the command"));
    assert!(
        text.contains(crate::events::log::INJECTED_PREFIX),
        "the error is the funnel's own: {text}"
    );
    assert!(
        !text.contains(crate::events::log::POISONED_PREFIX),
        "the command ended at the errored append; it did not attempt a second one: {text}"
    );
    let lines_after = fixture.log_bytes().iter().filter(|b| **b == b'\n').count();
    assert_eq!(
        lines_after,
        lines_before + 1,
        "the line is durable — this is the after-append order of T-APPEND (e-s)"
    );

    assert!(text.contains(RUN_ID), "the report names the run: {text}");
    assert!(
        text.contains("run_resumed"),
        "and the event kind whose outcome is unknown: {text}"
    );
    assert!(
        text.contains("Event.Append"),
        "and the site it was filed at: {text}"
    );
    assert!(
        text.contains("the proven prefix contains the line"),
        "and whether the proven prefix contains the line. Present here, and asserted as the \
         sentence rather than as \"some outcome\": the injection is at `Synced`, after the bytes \
         reached the file, so a protocol that reported `absent` would be wrong in the direction \
         that loses a durable transition: {text}"
    );
    assert!(
        text.contains("resumable"),
        "and the run is reported resumable, which is what makes ending here safe: {text}"
    );

    let seen = first.lock().unwrap_or_else(PoisonError::into_inner);
    assert_eq!(
        seen.count(EffectSiteId::Event(EventSite::OpenLog), HookPhase::Before),
        2,
        "`Event.OpenLog` twice: recovery step (a1)'s barrier, then the protocol's reopen after \
         the failed append. Once means no reopen happened and the outcome was never established."
    );
    assert_eq!(
        seen.count(
            EffectSiteId::Event(EventSite::ProvePrefixStable),
            HookPhase::Before
        ),
        2,
        "and the stable-prefix barrier is re-established over the reopened log before anything is \
         reported"
    );
    assert_eq!(
        seen.count(EffectSiteId::Event(EventSite::Append), HookPhase::Before),
        1,
        "and the append itself is never retried"
    );
    drop(seen);

    let second = harness();
    let runtime = runtime_holding_the_record();
    let given = Given::healthy(&fixture, &runtime, &certifies);
    let (result, _) = resume(&fixture, &second, &given);
    let recovered = result.expect("the next resume establishes its own barrier and continues");
    assert_eq!(
        recovered.resumed.epoch, 2,
        "the surviving prefix already carried one resume, so this is the second epoch"
    );
    assert!(
        first_observation(&second, EffectSiteId::Event(EventSite::ProvePrefixStable)).is_some(),
        "and it proved the prefix before acting on it"
    );
}

#[test]
fn an_append_error_during_recovery_cancels_the_reservation_and_every_running_invocation() {
    let fixture = Fixture::healthy("append-error-ledgers");
    let harness = harness();
    harness
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .arm(
            EffectSiteId::Event(EventSite::Append),
            SubEffectPoint::Synced,
            InjectionMode::ErrorReturn,
        )
        .expect("the Synced point supports an error return");
    let runtime = runtime_holding_the_record();
    let incarnation = IncarnationId(RESUMER.to_owned());

    let censused =
        chain_to_census(&fixture, &harness, &runtime, &incarnation).expect("the census completes");
    let rebuilt = RunnerRebuilt::rebuild(censused, &container_selection(), Some(&runtime))
        .expect("the recorded runner rebuilds by inspection");
    let certified =
        PreflightCertified::certify(rebuilt, &AlwaysCertifies).expect("the pre-flight certifies");

    let mut reservations = Reservations::new();
    reservations
        .take(ALPHA, ReservationKind::Dispatch)
        .expect("a provisional reservation is held");
    let mut invocations = InvocationLedger::new();
    let invocation = crate::runner::InvocationId::probe(crate::runner::ProbeTarget::Shell, 11)
        .expect("an invocation identity");
    invocations
        .register(&invocation)
        .expect("and one invocation is running");
    assert!(
        !reservations.is_empty() && invocations.running().len() == 1,
        "the ledgers must be non-empty, or this test proves nothing"
    );

    let mut hooks = HarnessTopologyHooks::new(Arc::clone(&harness));
    let mut warnings = Vec::new();
    let mut context = EmitContext {
        clock: &Frozen,
        hooks: &mut hooks,
        inputs: fixture.inputs(),
        reservations: &mut reservations,
        invocations: &mut invocations,
        warnings: &mut warnings,
    };
    let error = run_resumed(certified, &mut context, &incarnation)
        .expect_err("the injected append error ends the command");
    let text = message(&error);
    assert!(
        text.contains(crate::events::log::INJECTED_PREFIX),
        "the report carries the funnel's own error as its cause: {text}"
    );

    assert!(
        reservations.is_empty(),
        "obligation (2): whatever reservation was held is cancelled"
    );
    assert!(
        reservations.balances(),
        "and the reservation ledger balances — taken once, cancelled once"
    );
    assert_eq!(
        invocations.cancelled(),
        1,
        "obligation (3): every still-running invocation is cancelled"
    );
    assert!(
        invocations.running().is_empty() && invocations.balances(),
        "and the invocation ledger balances: no entry is left running"
    );
}

fn real_preflight<'a>(
    runner: &'a dyn Runner,
    adapters: &'a StubAdapters,
    fixture: &Fixture,
) -> RunPreflight<'a> {
    RunPreflight::new(
        runner,
        adapters,
        ShellKind::Bash,
        &fixture.repo_root,
        fixture.started.probed_agents.clone(),
    )
}

#[test]
fn resume_refuses_by_preflight_probe_when_shell_or_cli_fails_before_any_recovery_event() {
    for (tag, program, expected) in [
        ("shell", "bash", "the recorded shell"),
        ("cli", "claude", "the `claude-code` CLI"),
    ] {
        let fixture = Fixture::healthy(&format!("probe-{tag}"));
        let harness = harness();
        let runtime = runtime_holding_the_record();
        let runner = RecordingRunner::failing(program);
        let adapters = StubAdapters;
        let preflight = real_preflight(&runner, &adapters, &fixture);
        let given = Given::healthy(&fixture, &runtime, &preflight);

        let before = fixture.log_bytes();
        let (result, _) = resume(&fixture, &harness, &given);

        let text = message(&result.expect_err("a failing probe refuses"));
        assert!(
            text.contains(expected),
            "the refusal names what did not answer ({tag}): {text}"
        );
        assert!(
            text.contains(IMAGE_REF),
            "and the image it was probed inside ({tag}): {text}"
        );
        assert_eq!(
            fixture.log_bytes(),
            before,
            "a probe refusal precedes every recovery event ({tag})"
        );
        assert!(
            preflight.ledgers_balance(),
            "every probe invocation is settled and every slot released ({tag}); still running: \
             {:?}",
            preflight.running()
        );
        let programs: Vec<String> = runner
            .requests()
            .into_iter()
            .map(|request| request.command.program)
            .collect();
        if tag == "shell" {
            assert_eq!(
                programs,
                vec!["bash".to_owned()],
                "no agent is probed after the shell fails"
            );
        } else {
            assert_eq!(
                programs,
                vec!["bash".to_owned(), "claude".to_owned()],
                "the shell probe runs first and the agent probe second"
            );
        }
    }
}

#[test]
fn ledgers_empty_after_resume() {
    let fixture = Fixture::healthy("ledgers-empty");
    let harness = harness();
    let runtime = runtime_holding_the_record();
    let runner = RecordingRunner::default();
    let adapters = StubAdapters;
    let preflight = real_preflight(&runner, &adapters, &fixture);
    let given = Given::healthy(&fixture, &runtime, &preflight);

    let (result, _) = resume(&fixture, &harness, &given);
    result.expect("the healthy resume completes");

    assert!(
        preflight.ledgers_balance(),
        "R3 and R4 balance at the end of the pre-flight"
    );
    assert!(
        preflight.running().is_empty(),
        "no invocation is still registered as running: {:?}",
        preflight.running()
    );
    let roles: Vec<(String, bool)> = runner
        .requests()
        .into_iter()
        .map(|request| {
            (
                request.command.program.clone(),
                crate::engine::topology::identity::is_slotted(&request.invocation),
            )
        })
        .collect();
    assert_eq!(
        roles,
        vec![("bash".to_owned(), false), ("claude".to_owned(), true)],
        "the shell probe is non-slotted and the agent probe is slotted"
    );
    assert!(crate::engine::topology::identity::Reservations::new().is_empty());
    assert!(crate::engine::topology::identity::SlotAssertion::new().is_empty());
    assert!(crate::engine::topology::identity::InvocationLedger::new().balances());
}

struct ProbeContainerRunner<'a> {
    runtime: &'a dyn ContainerRuntime,
    private_root: PathBuf,
    run_dir: PathBuf,
    repo_key: String,
    incarnation: String,
    policy_digest: String,
    failing: String,
}

impl Runner for ProbeContainerRunner<'_> {
    fn run(&self, request: &RunnerRequest) -> Result<ProcessOutput, RunnerError> {
        use crate::runner::container::intent::{ContainerIntent, ContainerName};
        use crate::runner::container::runtime::CreateSpec;
        use crate::runner::container::{
            GitViewRequest, NoHooks as ContainerNoHooks, launch, release,
        };

        let name = ContainerName::new(
            &self.repo_key,
            RUN_ID,
            &self.incarnation,
            &request.invocation,
        )
        .map_err(|error| RunnerError::never_started(&request.invocation, error))?;
        let intent = ContainerIntent::new(
            RUN_ID.to_owned(),
            &self.run_dir,
            self.incarnation.clone(),
            self.repo_key.clone(),
            request.invocation.render(),
            self.policy_digest.clone(),
        );
        let plan = crate::runner::container::LaunchPlan {
            private_root: self.private_root.clone(),
            name: name.clone(),
            invocation: request.invocation.clone(),
            intent: intent.clone(),
            spec: CreateSpec {
                name: name.as_str().to_owned(),
                image_id: IMAGE_ID.to_owned(),
                labels: intent.labels(&self.private_root),
                mounts: Vec::new(),
                env: Vec::new(),
                command: std::iter::once(request.command.program.clone())
                    .chain(request.command.args.iter().cloned())
                    .collect(),
                workdir: Some("/".to_owned()),
                read_only_root: true,
            },
            view: GitViewRequest {
                path: crate::runner::container::exec::view_dir(&self.private_root, &name),
                workspace: request.workspace.clone(),
                head: None,
            },
        };
        let mut hooks = ContainerNoHooks;
        let view = DisposableDirView::new(ContainerTrace::off());
        let launched = launch(&mut hooks, self.runtime, &view, &plan)
            .map_err(|error| RunnerError::never_started(&request.invocation, error))?;
        let code = if request.command.program == self.failing {
            127
        } else {
            0
        };
        release(
            &mut hooks,
            self.runtime,
            &view,
            &self.private_root,
            &launched,
        )
        .map_err(|error| RunnerError::unresolved(&request.invocation, error))?;
        Ok(ProcessOutput {
            code: Some(code),
            stdout: String::new(),
            stderr: "the recorded shell is not in this image".to_owned(),
            duration: Duration::from_millis(1),
            timed_out: false,
            output_limited: false,
        })
    }
}

#[test]
fn resume_preflight_probe_containers_reclaimed_after_refusal() {
    let fixture = Fixture::healthy("probe-reclaim");
    let harness = harness();
    let runtime = runtime_holding_the_record();
    let runner = ProbeContainerRunner {
        runtime: &runtime,
        private_root: fixture.private_root.clone(),
        run_dir: fixture.public(),
        repo_key: fixture.repo_key.as_str().to_owned(),
        incarnation: RESUMER.to_owned(),
        policy_digest: crate::runner::policy::runner_policy_sha256(&fixture.started.runner),
        failing: "bash".to_owned(),
    };
    let adapters = StubAdapters;
    let preflight = real_preflight(&runner, &adapters, &fixture);
    let given = Given::healthy(&fixture, &runtime, &preflight);

    let before = fixture.log_bytes();
    let (result, _) = resume(&fixture, &harness, &given);

    let text = message(&result.expect_err("a shell that fails inside the image refuses"));
    assert!(text.contains("the recorded shell"), "{text}");
    assert_eq!(
        fixture.log_bytes(),
        before,
        "the refusal precedes every recovery event"
    );
    assert!(
        runtime.container_names().is_empty(),
        "every probe container is reclaimed: {:?}",
        runtime.container_names()
    );
    assert!(
        crate::runner::container::list_intents(&fixture.private_root)
            .expect("the namespace scans")
            .is_empty(),
        "and its intent went with it"
    );
    assert!(
        preflight.ledgers_balance(),
        "and the probe invocations are settled: {:?}",
        preflight.running()
    );
}

fn create_ref_entries(harness: &Arc<Mutex<HookHarness>>) -> u32 {
    harness
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .count(
            EffectSiteId::Ref(RefSite::CreateIntegration),
            HookPhase::Before,
        )
}

fn cas_integration_entries(harness: &Arc<Mutex<HookHarness>>) -> u32 {
    harness
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .count(
            EffectSiteId::Ref(RefSite::CompareAndSwapIntegration),
            HookPhase::Before,
        )
}

#[test]
fn kill_after_run_started_creates_integration_ref() {
    let fixture = Fixture::healthy("ref-p78-create");
    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);

    let committed = fixture.log_bytes();
    assert_eq!(
        given.refs.target(),
        None,
        "the fixture is the P6/P7 prefix: nothing created the ref"
    );

    let (result, _) = resume(&fixture, &harness, &given);
    result.expect("a resume of a run killed before P8 completes");

    assert_eq!(
        given.refs.created(),
        vec![(
            fixture.started.integration_ref.as_str().to_owned(),
            fixture.started.base_sha.as_str().to_owned(),
        )],
        "the ref is created once, at the name and base the record carries"
    );
    assert_eq!(
        create_ref_entries(&harness),
        1,
        "and the funnel was entered exactly once"
    );

    assert_eq!(
        given.refs.log_kinds_at_entries(),
        vec![vec!["run_started".to_owned()]],
        "the ref was created after a recovery event had already been appended"
    );
    assert_eq!(
        given.refs.log_bytes_at_entries(),
        vec![committed.clone()],
        "the log the funnel saw was not byte-identical to the committed prefix"
    );
    let after = fixture.log_bytes();
    assert!(
        after.len() > committed.len()
            && String::from_utf8_lossy(&after).contains("\"run_resumed\""),
        "the resume did not reach (h), so `before any recovery event` proves nothing"
    );
}

#[test]
fn a_resume_adopts_an_integration_ref_already_at_the_recorded_base() {
    {
        let fixture = Fixture::healthy("ref-p78-adopt");
        let harness = harness();
        let runtime = runtime_holding_the_record();
        let certifies = AlwaysCertifies;
        let mut given = Given::healthy(&fixture, &runtime, &certifies);
        given.refs = RecordingRefs::at(&fixture, fixture.started.base_sha.as_str());

        let (result, _) = resume(&fixture, &harness, &given);
        result.expect("present == base continues");

        assert_eq!(
            create_ref_entries(&harness),
            0,
            "the funnel was entered for a ref that was already at the base"
        );
        assert!(
            given.refs.created().is_empty(),
            "and nothing was created: {:?}",
            given.refs.created()
        );
        assert_eq!(
            given.refs.target().as_deref(),
            Some(fixture.started.base_sha.as_str()),
            "the ref still names the recorded base"
        );
    }

    {
        let fixture = Fixture::healthy("ref-p78-twice");
        let runtime = runtime_holding_the_record();
        let certifies = AlwaysCertifies;
        let given = Given::healthy(&fixture, &runtime, &certifies);

        let first = harness();
        let (result, _) = resume(&fixture, &first, &given);
        let opened = result.expect("the first resume completes").resumed.epoch;
        assert_eq!(create_ref_entries(&first), 1, "the first resume creates it");

        let second = harness();
        let (result, _) = resume(&fixture, &second, &given);
        let reopened = result.expect("the second resume completes").resumed.epoch;

        assert_eq!(
            create_ref_entries(&second),
            0,
            "the second resume entered `Ref.CreateIntegration` again; `no spend repeats` is not \
             held"
        );
        assert_eq!(
            given.refs.created().len(),
            1,
            "the ref was created twice: {:?}",
            given.refs.created()
        );
        assert!(
            reopened > opened,
            "the second resume did not open an epoch of its own ({opened} then {reopened}), so \
             it never reached the step this test is about"
        );
    }
}

#[test]
fn a_resume_refuses_an_integration_ref_at_another_sha_before_touching_anything() {
    let fixture = Fixture::healthy("ref-p78-elsewhere");
    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let mut given = Given::healthy(&fixture, &runtime, &certifies);
    let elsewhere = "b".repeat(40);
    given.refs = RecordingRefs::at(&fixture, &elsewhere);

    let committed = fixture.log_bytes();
    let (result, _) = resume(&fixture, &harness, &given);

    let text = message(&result.expect_err("a ref at another commit refuses"));
    assert!(
        text.contains(fixture.started.integration_ref.as_str())
            && text.contains(&elsewhere)
            && text.contains(fixture.started.base_sha.as_str()),
        "the refusal names the ref, where it is, and where the record says it should be: {text}"
    );
    assert_eq!(
        create_ref_entries(&harness),
        0,
        "the funnel was entered for a ref the step must have refused on sight"
    );
    assert!(given.refs.created().is_empty());
    assert_eq!(
        given.refs.target().as_deref(),
        Some(elsewhere.as_str()),
        "the ref was moved to make room for the run"
    );
    assert_eq!(
        fixture.log_bytes(),
        committed,
        "a P7/P8 refusal precedes every recovery event"
    );
}

#[test]
fn a_resume_refuses_a_symbolic_or_checked_out_integration_ref() {
    for (tag, shape, expected) in [
        ("symbolic", RefShape::Symbolic, "it is a symbolic ref"),
        (
            "checked-out",
            RefShape::CheckedOut,
            "it is checked out in the worktree",
        ),
    ] {
        let fixture = Fixture::healthy(&format!("ref-p78-{tag}"));
        let harness = harness();
        let runtime = runtime_holding_the_record();
        let certifies = AlwaysCertifies;
        let mut given = Given::healthy(&fixture, &runtime, &certifies);
        given.refs = RecordingRefs::shaped(&fixture, shape);

        let committed = fixture.log_bytes();
        let (result, _) = resume(&fixture, &harness, &given);

        let text = message(&result.expect_err("an unpublishable ref refuses"));
        assert!(
            text.contains(expected),
            "the refusal says which shape it found ({tag}): {text}"
        );
        assert!(
            text.contains(fixture.started.integration_ref.as_str()),
            "and names the recorded ref ({tag}): {text}"
        );
        assert_eq!(
            create_ref_entries(&harness),
            0,
            "the funnel ran for an unpublishable ref ({tag})"
        );
        assert_eq!(
            given.refs.target(),
            None,
            "and nothing was written to it ({tag})"
        );
        assert_eq!(
            given.refs.targets_read(),
            0,
            "`assert_publishable` did not refuse first: the target was read for an \
             unpublishable ref ({tag})"
        );
        assert_eq!(
            fixture.log_bytes(),
            committed,
            "a P7/P8 refusal precedes every recovery event ({tag})"
        );
    }
}

#[test]
fn the_p7_p8_step_runs_after_the_refusals_that_bound_it() {
    {
        let fixture = Fixture::build(
            "ref-p78-after-b",
            Damage {
                extra: vec![
                    dispatched(),
                    attempt_started(1),
                    attempt_finished(
                        1,
                        AttemptSettlement::Closed {
                            transition: SettlementTransition::Failed {
                                halts_run: true,
                                reason: "the ladder ran out".to_owned(),
                            },
                            lease: LeaseDisposition::PredictedReleased,
                        },
                    ),
                    run_finished(RunOutcome::Halted, Some(ALPHA)),
                ],
                ..Damage::default()
            },
        );
        let harness = harness();
        let runtime = runtime_holding_the_record();
        let certifies = AlwaysCertifies;
        let given = Given::healthy(&fixture, &runtime, &certifies);

        let text = message(
            &resume(&fixture, &harness, &given)
                .0
                .expect_err("a finished run does not continue"),
        );
        assert!(text.contains("already finished"), "{text}");
        assert_eq!(
            create_ref_entries(&harness),
            0,
            "(b) refused and the ref was published anyway"
        );
        assert_eq!(given.refs.target(), None);
    }

    {
        let fixture = Fixture::healthy("ref-p78-after-c");
        let harness = harness();
        let runtime = runtime_holding_the_record();
        let runner = RecordingRunner::failing("bash");
        let adapters = StubAdapters;
        let preflight = real_preflight(&runner, &adapters, &fixture);
        let given = Given::healthy(&fixture, &runtime, &preflight);

        let text = message(
            &resume(&fixture, &harness, &given)
                .0
                .expect_err("a failing probe refuses"),
        );
        assert!(text.contains("the recorded shell"), "{text}");
        assert_eq!(
            create_ref_entries(&harness),
            0,
            "(c) refused and the ref was published anyway"
        );
        assert_eq!(given.refs.target(), None);
    }
}

#[test]
#[ignore = "spawned as a subprocess by kill_during_recovery_repeats_recovery"]
fn recovery_kill_child() {
    let repo_root = PathBuf::from(
        std::env::var("UPSTROKE_TEST_KILL_REPO").expect("the parent names the repository"),
    );
    let git_dir = PathBuf::from(
        std::env::var("UPSTROKE_TEST_KILL_GITDIR").expect("the parent names the git dir"),
    );
    let repo_key = RepoKey::v1(&std::fs::canonicalize(&git_dir).expect("the git dir exists"));

    let harness = harness();
    harness
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .arm(
            EffectSiteId::Event(EventSite::Append),
            SubEffectPoint::Written,
            InjectionMode::Kill,
        )
        .expect("the Written point supports a kill");
    let mut hooks = HarnessTopologyHooks::new(harness);
    let runtime = runtime_holding_the_record();
    let liveness = FakeOwnerLiveness::new();
    let view = DisposableDirView::new(ContainerTrace::default());
    let certifies = AlwaysCertifies;
    let incarnation = IncarnationId(RESUMER.to_owned());
    let today = container_selection();
    let refs = RecordingRefs::with_log(
        &rundir::public_dir(&repo_root, RUN_ID).join(rundir::EVENT_LOG),
        RefShape::Direct,
        None,
    );
    let mut warnings = Vec::new();

    let root = RootDerived::derive_with(&repo_root, RUN_ID, None, TOPOLOGY_SCHEMA)
        .expect("(a0) derives in the child");
    let manager = crate::workspace_manager::WorkspaceManager::derive(
        &repo_root,
        root.private_root(),
        RUN_ID,
        RESUMER,
    )
    .expect("the child's repository and private root are real directories");
    let _ = run_recovery_order(
        root,
        &ResumeSeams {
            repo_root: &repo_root,
            worktree_git_dir: &git_dir,
            repo_key: &repo_key,
            incarnation: &incarnation,
            inputs: FrozenInputs {
                plan: plan(),
                normalized_plan_digest: "sha256:aaaa".to_owned(),
            },
            today: &today,
            runtime: &runtime,
            liveness: &liveness,
            view: &view,
            preflight: &certifies,
            refs: &refs,
            manager: &manager,
            clock: &Frozen,
        },
        &mut hooks,
        &mut warnings,
    );
    unreachable!("the kill must have taken this process");
}

#[test]
fn kill_during_recovery_repeats_recovery() {
    let fixture = Fixture::healthy("kill-recovery");
    let before = fixture.log_bytes();

    let exe = std::env::current_exe().expect("the test binary knows where it is");
    let request = RunnerRequest {
        command: CommandSpec {
            program: exe.display().to_string(),
            args: vec![
                "--exact".to_owned(),
                "engine::topology::recover::tests::recovery_kill_child".to_owned(),
                "--ignored".to_owned(),
                "--test-threads".to_owned(),
                "1".to_owned(),
            ],
            env: vec![
                (
                    "UPSTROKE_TEST_KILL_REPO".to_owned(),
                    fixture.repo_root.display().to_string(),
                ),
                (
                    "UPSTROKE_TEST_KILL_GITDIR".to_owned(),
                    fixture.git_dir.display().to_string(),
                ),
            ],
            stdin: Vec::new(),
        },
        workspace: fixture.repo_root.clone(),
        role: crate::runner::ExecutionRole::Gate,
        timeout: Duration::from_secs(120),
        agent: None,
        invocation: crate::runner::InvocationId::probe(crate::runner::ProbeTarget::Shell, 7)
            .expect("a probe identity for the spawned child"),
    };
    let output = crate::runner::host::HostRunner::new()
        .run(&request)
        .expect("the child runs");
    assert_ne!(
        output.code,
        Some(0),
        "the child must have died rather than finished: {output:?}"
    );
    assert!(
        !output.stdout.contains("test result:"),
        "the child printed a test result, so it finished rather than dying: {}",
        output.stdout
    );
    assert!(
        !output
            .stdout
            .contains("the kill must have taken this process"),
        "the child reached its `unreachable!`, so the injection did not kill it: {}",
        output.stdout
    );

    let after_kill = fixture.log_bytes();
    assert!(
        after_kill.len() > before.len(),
        "the kill is at `Written`, after the bytes reached the file"
    );

    const AFTER_THE_KILL: &str = "01KZTKILL00000000000000004";
    let husk = plant_husk(&fixture, AFTER_THE_KILL, false);
    assert!(husk.private.exists() && husk.public.exists());

    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);
    let (result, _) = resume(&fixture, &harness, &given);
    let recovered = result.expect("the next process recovers");

    let seen = harness.lock().unwrap_or_else(PoisonError::into_inner);
    for site in [
        EffectSiteId::Lock(LockSite::AcquireWorktree),
        EffectSiteId::Lock(LockSite::AcquireRun),
        EffectSiteId::Event(EventSite::OpenLog),
        EffectSiteId::Event(EventSite::ProvePrefixStable),
        EffectSiteId::RunDir(RunDirSite::RemovePrivateHusk),
        EffectSiteId::RunDir(RunDirSite::RemovePublicHusk),
    ] {
        assert!(
            seen.observed(site, HookPhase::Before),
            "the repeat runs `{site}` again — a kill repeats from (a0), it does not resume from a \
             checkpoint"
        );
    }
    drop(seen);
    assert!(
        !husk.private.exists() && !husk.public.exists(),
        "and the repeat's census reclaimed the husk it found, both halves"
    );
    assert_eq!(
        recovered.resumed.epoch, 2,
        "the killed process's `run_resumed` line survived, so this resume opens the epoch after it"
    );
}

#[test]
fn the_barrier_is_the_only_topology_route_from_a_proven_prefix_to_an_append_handle() {
    const ENTRY: &str = "into_log_and_fold(";
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let test_modules = {
        let mut all = Vec::new();
        let mut stack = vec![src.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("src is readable") {
                let path = entry.expect("a directory entry").path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    all.push(path);
                }
            }
        }
        crate::effects::census_domain::whole_file_test_modules(&src, &all, 13)
    };
    let mut stack = vec![src.clone()];
    let mut callers: Vec<(String, usize)> = Vec::new();
    let mut regions: Vec<(String, usize, usize)> = Vec::new();
    let mut scanned = 0_usize;
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("src is readable") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_none_or(|ext| ext != "rs") {
                continue;
            }
            let relative = path
                .strip_prefix(&src)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if !relative.starts_with("engine/topology") {
                continue;
            }
            if test_modules.contains(&path) {
                continue;
            }
            scanned += 1;
            let source = std::fs::read_to_string(&path).expect("a source file");
            let production = crate::effects::production_code(&source);
            let production = production.as_str();
            regions.push((relative.clone(), production.len(), source.len()));

            let count = production
                .match_indices(ENTRY)
                .filter(|(at, _)| !production[..*at].trim_end().ends_with("fn"))
                .count();
            if count > 0 {
                callers.push((relative, count));
            }
        }
    }
    callers.sort();

    assert!(
        scanned >= 4,
        "the walk found only {scanned} topology sources, so its zero counts would prove nothing"
    );
    for (file, region, whole) in &regions {
        assert!(
            *region * 10 > *whole,
            "{file}'s production region is {region} of {whole} bytes. A census over a fraction \
             of a file reports zero for the part it never read — this is `PR4-CENSUS-COMMENT-ORACLE`, \
             and it is how the driver was scanned at 4.7% while reading as a pass"
        );
    }
    assert_eq!(
        callers,
        vec![("engine/topology/recover.rs".to_owned(), 1)],
        "a proven prefix becomes an append handle in exactly one production place in the topology \
         engine, and that place is `BarrierHeld::from`"
    );
}

#[test]
fn the_recovery_order_performs_every_step_the_packet_names() {
    let fixture = Fixture::healthy("every-step");
    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);

    let (outcome, _) = resume(&fixture, &harness, &given);
    let recovered = outcome.expect("the healthy resume completes");

    let owed: Vec<RecoveryStep> = RecoveryStep::ALL
        .into_iter()
        .filter(|step| step.performer() == Performer::ThisOrder)
        .collect();

    let mut performed = recovered.steps.clone();
    performed.sort_unstable();
    let mut expected = owed.clone();
    expected.sort_unstable();
    assert_eq!(
        performed,
        expected,
        "the order performed {:?} and the packet names {:?} for it; a step in \
         the second list and not the first is a step no code performs, which \
         is the defect this test exists for",
        recovered
            .steps
            .iter()
            .map(|step| step.label())
            .collect::<Vec<_>>(),
        owed.iter().map(|step| step.label()).collect::<Vec<_>>()
    );

    let packet_order: Vec<RecoveryStep> = owed
        .iter()
        .copied()
        .filter(|step| step.position_override().is_none())
        .collect();
    let performed_order: Vec<RecoveryStep> = recovered
        .steps
        .iter()
        .copied()
        .filter(|step| step.position_override().is_none())
        .collect();
    assert_eq!(
        performed_order, packet_order,
        "the steps the packet alone positions ran out of order; a step that \
         must move carries the clause that moves it"
    );

    let at = |step: RecoveryStep| {
        recovered
            .steps
            .iter()
            .position(|performed| *performed == step)
            .expect("every owed step was performed")
    };
    assert!(
        at(RecoveryStep::D) < at(RecoveryStep::F) && at(RecoveryStep::E) < at(RecoveryStep::F),
        "(f)'s converging half appends, so it belongs with (d) and (e) rather \
         than before them. Its refusing half is unmarked because a refusal ends \
         the command and records no step"
    );
    assert!(
        at(RecoveryStep::F) < at(RecoveryStep::G) && at(RecoveryStep::G) < at(RecoveryStep::H),
        "and it stays in the packet's position: after (e), before (g) and (h)"
    );
}

#[test]
fn the_transcribed_recovery_steps_are_the_packets_eleven() {
    assert_eq!(
        RecoveryStep::ALL
            .iter()
            .map(|step| step.label())
            .collect::<Vec<_>>(),
        vec!["a0", "a", "a1", "b", "c", "d", "e", "f", "g", "h", "i"],
        "transcribed from `decisions.sequential_substrate.recovery_order`"
    );
    assert_eq!(
        RecoveryStep::ALL
            .iter()
            .filter(|step| step.performer() != Performer::ThisOrder)
            .map(|step| (step.label(), step.performer()))
            .collect::<Vec<_>>(),
        vec![("a0", Performer::CallerBefore), ("i", Performer::LoopAfter)],
        "exactly two steps are delegated, and each is delegated to a named \
         performer with a reason: a step whose performer nobody states is \
         indistinguishable from a step nobody performs"
    );
}

#[test]
fn resume_recreates_an_open_no_attempt_worktree_at_its_base() {
    let fixture = Fixture::build(
        "step-g-recreate",
        Damage {
            open_generation: true,
            ..Damage::default()
        },
    );
    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);

    let manager = fixture.manager();
    let slot = crate::engine::topology::dispatch::task_slot(ALPHA, GEN);
    let worktree = manager.slot_path(&slot);
    assert!(
        !worktree.exists(),
        "the fixture leaves the generation open with no worktree, which is what \
         the kill leaves and what (g) has to answer"
    );

    let (outcome, _) = resume(&fixture, &harness, &given);
    let recovered = outcome.expect("the resume completes");

    assert_eq!(
        recovered
            .recreated
            .iter()
            .map(|(key, generation, _)| (*key, *generation))
            .collect::<Vec<_>>(),
        vec![(ALPHA, GEN)],
        "(g) acts on exactly the open generation, and on nothing else"
    );
    assert!(
        worktree.exists(),
        "(g) recreates the worktree the generation records; without it the \
         resumed loop has a dispatched generation whose checkout does not exist"
    );
    assert_eq!(
        crate::workspace_manager::fixture::git(&worktree, &["rev-parse", "HEAD"]),
        fixture.base_sha.0,
        "at its **base** — `recovery_order` (g) says where, and a worktree cut \
         anywhere else silently changes what the next attempt starts from"
    );
}

#[test]
fn an_inherited_lease_on_an_ordinary_task_is_refused_at_the_barrier_before_step_g() {
    let repair = {
        let TopologyEventBody::TaskDispatched { mut data } = dispatched() else {
            unreachable!("`dispatched` builds a `TaskDispatched`")
        };
        data.lease = LeaseGrant::InheritedLineage { root: TaskKey(1) };
        TopologyEventBody::TaskDispatched { data }
    };
    let fixture = Fixture::build(
        "step-g-repair",
        Damage {
            extra: vec![repair],
            ..Damage::default()
        },
    );
    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);

    let (outcome, _) = resume(&fixture, &harness, &given);
    let text = outcome
        .expect_err("the fold refuses the shape before any step sees it")
        .to_string();
    assert!(
        text.contains("an ordinary task belongs to no lineage and cannot inherit one's lease"),
        "the refusal is the fold's, at the replay, and not step (g)'s: {text}"
    );
    assert!(
        text.contains("stable-prefix barrier"),
        "and it lands at the barrier, so nothing fold-derived was acted on: {text}"
    );

    let registry = TaskRegistry::originals_with_agents(
        &fixture.plan,
        &fixture.started.registry_record(),
        &fixture.started.probed_agents,
    )
    .expect("the fixture's plan registers");
    assert!(
        !registry.entries().is_empty(),
        "a registry with no entries would satisfy the next assertion by having \
         nothing to check"
    );
    assert!(
        registry
            .entries()
            .iter()
            .all(|entry| entry.lineage.is_none()),
        "no original entry descends from a lineage; lineage members enter the registry \
         only through `merge_rejected`, so an inherited lease on an original is never valid \
         (a repair generation's own path through (g) is \
         `a_repair_dispatch_interrupted_before_its_attempt_is_recreated_at_its_base_and_materialized_once`)"
    );
}

#[test]
fn the_recovery_order_hands_the_run_on_rather_than_dropping_it() {
    let fixture = Fixture::healthy("hand-on");
    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);

    let (outcome, _) = resume_holding(&fixture, &harness, &given);
    let (recovered, handle) = outcome.expect("the healthy resume completes");

    assert_eq!(
        handle.started.run_id, RUN_ID,
        "the handle names the run the order recovered"
    );
    assert!(
        !handle.fold.is_poisoned(),
        "and hands on a fold that may still be transitioned"
    );
    assert_eq!(
        handle.fold.epoch().map(|epoch| epoch.0),
        Some(recovered.resumed.epoch),
        "the fold in the handle is the one `(h)` incremented, not a second \
         derivation of the same log — a rebuilt fold is a rule that can \
         disagree with the one the barrier proved"
    );

    let contested = rundir::RunLock::acquire(&rundir::public_dir(&fixture.repo_root, RUN_ID));
    assert!(
        contested.is_err(),
        "the run lock is still held by the handle; a loop that had to retake \
         it would be racing itself"
    );

    drop(handle);
    rundir::RunLock::acquire(&rundir::public_dir(&fixture.repo_root, RUN_ID))
        .expect("dropping the handle releases the run lock");
}

#[test]
fn the_driver_takes_over_from_the_recovery_order_and_steps() {
    use crate::engine::topology::run::{Progress, RunSeams, TopologyRun};
    use crate::engine::topology::select::Ceiling;

    let fixture = Fixture::healthy("driver-steps");
    let harness = harness();
    let (_recovered, handle) =
        resume_with_real_refs(&fixture, &harness).expect("the healthy resume completes");

    let mut run = TopologyRun::resumed(handle, fixture.inputs(), Ceiling::unlimited());
    let mut hooks = HarnessTopologyHooks::new(Arc::clone(&harness));
    let sleeper = RecordingSleeper::default();
    let manager = fixture.manager();
    let runner = RecordingRunner::default();
    let adapters = crate::engine::topology::scaffold::ScaffoldAdapters::new();
    let paths = crate::rundir::RunPaths::with_private_root(
        &fixture.repo_root,
        &fixture.started.run_id,
        &fixture.private_root,
    );
    paths.create().expect("the run directories are creatable");
    let plans = crate::engine::assembly::FrozenPlans {
        adapters: &adapters,
        paths: &paths,
        gates: &[],
        pools: &[],
        caps: &[],
        worker_timeout: std::time::Duration::from_secs(300),
        decisions: &[],
    };
    let seams = RunSeams {
        manager: &manager,
        clock: &Frozen,
        sleeper: &sleeper,
        runner: &runner,
        adapters: &adapters,
        paths: &paths,
        plans: &plans,
        reviews: &crate::engine::attempt::LegacyReviewPasses,
        input_policy: &crate::engine::attempt::LegacyReviewInputPolicy,
        answers: &crate::interaction::UnattendedAnswers,
        ids: &FixedIds,
        halts_run: false,
    };

    let kinds_before = durable_kinds(&fixture);
    let progress = run
        .step(&seams, &mut hooks)
        .expect("the branch performs its first four clauses");

    let Progress::Settled {
        key,
        accepted,
        spent_attempt,
    } = progress
    else {
        panic!("the ready-dispatch branch did not run an attempt: {progress:?}");
    };
    assert_eq!(key, TaskKey(0));

    assert!(
        !accepted,
        "a worker that edited nothing was judged acceptable, which means the \
         cheap rungs of the verification ladder did not run"
    );

    assert_eq!(
        durable_kinds(&fixture),
        {
            let mut expected = kinds_before.clone();
            expected.push("task_dispatched".to_owned());
            expected.push("attempt_started".to_owned());
            expected.push("attempt_finished".to_owned());
            expected
        },
        "the whole branch, in order: the dispatch, the attempt, the settlement"
    );

    assert!(
        spent_attempt,
        "an attempt whose worker ran and produced a diff to judge did not spend \
         one of its rung's attempts"
    );

    assert!(
        !runner.requests().is_empty(),
        "the attempt appended `attempt_started` and never spawned anything"
    );

    let recorded = TopologyFold::parse_log(&fixture.log_bytes())
        .expect("the log parses")
        .into_iter()
        .find_map(|event| match event.body {
            TopologyEventBody::TaskDispatched { data } => Some(data.lease),
            _ => None,
        })
        .expect("the dispatch is durable");
    let LeaseGrant::Predicted { paths: recorded } = recorded else {
        panic!("an ordinary dispatch takes a predicted lease")
    };
    assert_eq!(
        Some(recorded),
        run.fold().predicted_region(ALPHA),
        "the region in the log is the one the fold admitted on. Compared \
         against the fold rather than against a literal, because a literal \
         would agree with whichever derivation this test happened to use"
    );

    assert_eq!(
        run.entitlements_held(),
        0,
        "the dispatch reservation was converted at `task_dispatched`, not left \
         held across the refusal. A leaked entitlement here is
         `PR7-INTEGRATION-NO-ENTITLEMENT`'s failure wearing a different hat: at \
         the only width production creates, one held entitlement is a full \
         pipeline and nothing is ever selected again"
    );
}

#[test]
fn the_driver_carries_an_accepted_attempt_through_the_candidate_sequence() {
    use crate::engine::topology::run::{Progress, RunSeams, TopologyRun};
    use crate::engine::topology::select::Ceiling;

    let fixture = Fixture::healthy("driver-promotes");
    let harness = harness();
    let (_recovered, handle) =
        resume_with_real_refs(&fixture, &harness).expect("the healthy resume completes");

    let mut run = TopologyRun::resumed(handle, fixture.inputs(), Ceiling::unlimited());
    let mut hooks = TracedHooks::new(&harness);
    let sleeper = RecordingSleeper::default();
    let manager = fixture.manager();
    let runner = RecordingRunner::editing();
    let adapters = crate::engine::topology::scaffold::ScaffoldAdapters::new();
    let paths = crate::rundir::RunPaths::with_private_root(
        &fixture.repo_root,
        &fixture.started.run_id,
        &fixture.private_root,
    );
    paths.create().expect("the run directories are creatable");
    let plans = crate::engine::assembly::FrozenPlans {
        adapters: &adapters,
        paths: &paths,
        gates: &[],
        pools: &[],
        caps: &[],
        worker_timeout: std::time::Duration::from_secs(300),
        decisions: &[],
    };
    let seams = RunSeams {
        manager: &manager,
        clock: &Frozen,
        sleeper: &sleeper,
        runner: &runner,
        adapters: &adapters,
        paths: &paths,
        plans: &plans,
        reviews: &crate::engine::attempt::LegacyReviewPasses,
        input_policy: &crate::engine::attempt::LegacyReviewInputPolicy,
        answers: &crate::interaction::UnattendedAnswers,
        ids: &FixedIds,
        halts_run: false,
    };

    let kinds_before = durable_kinds(&fixture);
    let progress = run
        .step(&seams, &mut hooks)
        .expect("the branch performs its first four clauses");

    let Progress::Settled { key, accepted, .. } = progress else {
        panic!("the ready-dispatch branch did not run an attempt: {progress:?}");
    };
    assert_eq!(key, TaskKey(0));
    assert!(
        accepted,
        "the worker left a change and the plan configures no gates or reviewers, \
     so nothing could reject it"
    );

    assert_eq!(
        durable_kinds(&fixture),
        {
            let mut expected = kinds_before.clone();
            expected.push("task_dispatched".to_owned());
            expected.push("attempt_started".to_owned());
            expected.push("candidate_prepared".to_owned());
            expected.push("task_candidate_created".to_owned());
            expected
        },
        "the whole branch, in the order the packet specifies"
    );

    assert_eq!(
        hooks.timeline.order(&[
            EffectSiteId::Object(ObjectSite::CandidateCommitTree),
            EffectSiteId::Ref(RefSite::PinCandidatePrepared),
            EffectSiteId::Event(EventSite::Append),
            EffectSiteId::Ref(RefSite::CreateCandidates),
            EffectSiteId::Ref(RefSite::DeleteCandidatePin),
            EffectSiteId::Worktree(WorktreeSite::Remove),
        ]),
        vec![
            "Event.Append".to_owned(),
            "Event.Append".to_owned(),
            "Object.CandidateCommitTree".to_owned(),
            "Ref.PinCandidatePrepared".to_owned(),
            "Event.Append".to_owned(),
            "Ref.CreateCandidates".to_owned(),
            "Event.Append".to_owned(),
            "Ref.DeleteCandidatePin".to_owned(),
            "Worktree.Remove".to_owned(),
        ],
        "the driver's candidate sequence, as one observed order over both families"
    );
}

#[test]
fn a_runs_spend_is_the_same_live_as_on_replay() {
    use crate::engine::topology::run::{RunSeams, TopologyRun};
    use crate::engine::topology::select::{Ceiling, Spend};

    let fixture = Fixture::healthy("spend-parity");
    let harness = harness();
    let (_recovered, handle) =
        resume_with_real_refs(&fixture, &harness).expect("the healthy resume completes");

    let mut run = TopologyRun::resumed(handle, fixture.inputs(), Ceiling::unlimited());
    let mut hooks = HarnessTopologyHooks::new(Arc::clone(&harness));
    let sleeper = RecordingSleeper::default();
    let manager = fixture.manager();
    let runner = RecordingRunner::editing();
    let adapters = crate::engine::topology::scaffold::ScaffoldAdapters::new();
    let paths = crate::rundir::RunPaths::with_private_root(
        &fixture.repo_root,
        &fixture.started.run_id,
        &fixture.private_root,
    );
    paths.create().expect("the run directories are creatable");
    let plans = crate::engine::assembly::FrozenPlans {
        adapters: &adapters,
        paths: &paths,
        gates: &[],
        pools: &[],
        caps: &[],
        worker_timeout: std::time::Duration::from_secs(300),
        decisions: &[],
    };
    let seams = RunSeams {
        manager: &manager,
        clock: &Frozen,
        sleeper: &sleeper,
        runner: &runner,
        adapters: &adapters,
        paths: &paths,
        plans: &plans,
        reviews: &crate::engine::attempt::LegacyReviewPasses,
        input_policy: &crate::engine::attempt::LegacyReviewInputPolicy,
        answers: &crate::interaction::UnattendedAnswers,
        ids: &FixedIds,
        halts_run: false,
    };

    run.step(&seams, &mut hooks)
        .expect("the accepted attempt runs the candidate sequence");

    let live = run.spend().run_total();

    let events = TopologyFold::parse_log(&fixture.log_bytes()).expect("the log parses");
    let replayed = Spend::replay(&events).run_total();

    assert!(
        live > 0.0,
        "the fixture priced nothing, so this asserts two zeroes and proves \
         nothing: give the scaffold adapter a cost"
    );
    assert!(
        (live - replayed).abs() < 1e-9,
        "a live run and a replay of its own log price it differently: live \
         {live}, replay {replayed}. A resumed run would refuse work it could \
         afford, or buy work it could not"
    );
}

#[test]
fn the_driver_settles_an_outage_from_the_folds_deferral_count() {
    use crate::engine::topology::run::{Progress, RunSeams, TopologyRun};
    use crate::engine::topology::select::Ceiling;

    let fixture = Fixture::build(
        "driver-outage",
        Damage {
            extra: vec![
                dispatched(),
                attempt_started(1),
                attempt_finished(
                    1,
                    AttemptSettlement::Closed {
                        transition: SettlementTransition::Deferred {
                            defers: 1,
                            reason: "the pool was exhausted".to_owned(),
                        },
                        lease: LeaseDisposition::PredictedReleased,
                    },
                ),
            ],
            ..Damage::default()
        },
    );
    let harness = harness();
    let (_recovered, handle) =
        resume_with_real_refs(&fixture, &harness).expect("the healthy resume completes");

    let mut run = TopologyRun::resumed(handle, fixture.inputs(), Ceiling::unlimited());
    let mut hooks = HarnessTopologyHooks::new(Arc::clone(&harness));
    let sleeper = RecordingSleeper::default();
    let manager = fixture.manager();
    let runner = RecordingRunner::editing();
    let adapters = crate::engine::topology::scaffold::ScaffoldAdapters::rate_limiting();
    let paths = crate::rundir::RunPaths::with_private_root(
        &fixture.repo_root,
        &fixture.started.run_id,
        &fixture.private_root,
    );
    paths.create().expect("the run directories are creatable");
    let plans = crate::engine::assembly::FrozenPlans {
        adapters: &adapters,
        paths: &paths,
        gates: &[],
        pools: &[],
        caps: &[],
        worker_timeout: std::time::Duration::from_secs(300),
        decisions: &[],
    };
    let seams = RunSeams {
        manager: &manager,
        clock: &Frozen,
        sleeper: &sleeper,
        runner: &runner,
        adapters: &adapters,
        paths: &paths,
        plans: &plans,
        reviews: &crate::engine::attempt::LegacyReviewPasses,
        input_policy: &crate::engine::attempt::LegacyReviewInputPolicy,
        answers: &crate::interaction::UnattendedAnswers,
        ids: &FixedIds,
        halts_run: false,
    };

    let progress = run
        .step(&seams, &mut hooks)
        .expect("the branch settles an outage");

    let Progress::Settled {
        accepted,
        spent_attempt,
        ..
    } = progress
    else {
        panic!("the ready-dispatch branch did not settle: {progress:?}");
    };
    assert!(
        !accepted,
        "a rate-limited worker produced nothing to accept"
    );

    assert!(
        !spent_attempt,
        "an outage spent one of the rung's attempts, which is the cell \
         `ladder::spends_allowance` exists to get right"
    );

    let settlements: Vec<u32> = TopologyFold::parse_log(&fixture.log_bytes())
        .expect("the log parses")
        .into_iter()
        .filter_map(|event| match event.body {
            TopologyEventBody::AttemptFinished { data } => match data.settlement {
                AttemptSettlement::Closed {
                    transition: SettlementTransition::Deferred { defers, .. },
                    ..
                } => Some(defers),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(
        settlements,
        vec![1, 2],
        "the second deferral did not continue the first. A driver reading a \
         process-local zero records `1` here and defers forever"
    );
}

#[test]
fn the_driver_parks_an_attempt_with_the_question_it_raised() {
    use crate::engine::topology::run::{Progress, RunSeams, TopologyRun};
    use crate::engine::topology::select::Ceiling;

    let fixture = Fixture::healthy("driver-parks");
    let harness = harness();
    let (_recovered, handle) =
        resume_with_real_refs(&fixture, &harness).expect("the healthy resume completes");

    let mut run = TopologyRun::resumed(handle, fixture.inputs(), Ceiling::unlimited());
    let mut hooks = HarnessTopologyHooks::new(Arc::clone(&harness));
    let sleeper = RecordingSleeper::default();
    let manager = fixture.manager();
    let runner = RecordingRunner::editing();
    let adapters = crate::engine::topology::scaffold::ScaffoldAdapters::asking();
    let paths = crate::rundir::RunPaths::with_private_root(
        &fixture.repo_root,
        &fixture.started.run_id,
        &fixture.private_root,
    );
    paths.create().expect("the run directories are creatable");
    let plans = crate::engine::assembly::FrozenPlans {
        adapters: &adapters,
        paths: &paths,
        gates: &[],
        pools: &[],
        caps: &[],
        worker_timeout: std::time::Duration::from_secs(300),
        decisions: &[],
    };
    let seams = RunSeams {
        manager: &manager,
        clock: &Frozen,
        sleeper: &sleeper,
        runner: &runner,
        adapters: &adapters,
        paths: &paths,
        plans: &plans,
        reviews: &crate::engine::attempt::LegacyReviewPasses,
        input_policy: &crate::engine::attempt::LegacyReviewInputPolicy,
        answers: &crate::interaction::UnattendedAnswers,
        ids: &FixedIds,
        halts_run: false,
    };

    let progress = run.step(&seams, &mut hooks).expect("the branch parks");

    let Progress::Settled {
        accepted,
        spent_attempt,
        ..
    } = progress
    else {
        panic!("the ready-dispatch branch did not settle: {progress:?}");
    };
    assert!(
        !accepted,
        "an agent that asked a question produced no verdict"
    );

    assert!(
        !spent_attempt,
        "a park spent one of the rung's attempts, which is the cell the \
         allowance fix exists for"
    );

    let parked = TopologyFold::parse_log(&fixture.log_bytes())
        .expect("the log parses")
        .into_iter()
        .find_map(|event| match event.body {
            TopologyEventBody::AttemptFinished { data } => match data.settlement {
                AttemptSettlement::Closed {
                    transition: SettlementTransition::Parked { question },
                    ..
                } => Some(question),
                _ => None,
            },
            _ => None,
        })
        .expect("a parking settlement is durable");

    assert_eq!(parked.id, crate::ir::QuestionId("q-park-fixed".to_owned()));
    assert_eq!(parked.key, TaskKey(0));
    assert_eq!(parked.kind, crate::ir::QuestionKind::Clarify);

    assert!(
        parked.context.contains("stopped and asked for a decision"),
        "the context is not `question_context`'s: {}",
        parked.context
    );
    assert!(
        parked
            .context
            .contains("two incompatible \\\n                      formats")
            || parked.context.contains("incompatible"),
        "the agent's own words are not quoted back: {}",
        parked.context
    );
    assert_eq!(
        parked.options,
        crate::engine::coordinator::topology_question_options(crate::ir::QuestionKind::Clarify),
        "the options are the schema-4 list, which promises nothing about typed text"
    );
}

#[test]
fn the_driver_refuses_a_tree_a_filter_has_transformed() {
    use crate::engine::topology::run::{Progress, RunSeams, TopologyRun};
    use crate::engine::topology::select::Ceiling;

    let fixture = Fixture::healthy("driver-filtered");
    let harness = harness();
    let (_recovered, handle) =
        resume_with_real_refs(&fixture, &harness).expect("the healthy resume completes");

    let mut run = TopologyRun::resumed(handle, fixture.inputs(), Ceiling::unlimited());
    let mut hooks = HarnessTopologyHooks::new(Arc::clone(&harness));
    let sleeper = RecordingSleeper::default();
    let manager = fixture.manager();
    let runner = RecordingRunner::filtering();
    let adapters = crate::engine::topology::scaffold::ScaffoldAdapters::new();
    let paths = crate::rundir::RunPaths::with_private_root(
        &fixture.repo_root,
        &fixture.started.run_id,
        &fixture.private_root,
    );
    paths.create().expect("the run directories are creatable");
    let plans = crate::engine::assembly::FrozenPlans {
        adapters: &adapters,
        paths: &paths,
        gates: &[],
        pools: &[],
        caps: &[],
        worker_timeout: std::time::Duration::from_secs(300),
        decisions: &[],
    };
    let seams = RunSeams {
        manager: &manager,
        clock: &Frozen,
        sleeper: &sleeper,
        runner: &runner,
        adapters: &adapters,
        paths: &paths,
        plans: &plans,
        reviews: &crate::engine::attempt::LegacyReviewPasses,
        input_policy: &crate::engine::attempt::LegacyReviewInputPolicy,
        answers: &crate::interaction::UnattendedAnswers,
        ids: &FixedIds,
        halts_run: false,
    };

    let progress = run
        .step(&seams, &mut hooks)
        .expect("the branch settles the refusal");

    let Progress::Settled { accepted, .. } = progress else {
        panic!("the ready-dispatch branch did not settle: {progress:?}");
    };
    assert!(
        !accepted,
        "a tree a filter has transformed was accepted, so the ladder's third \
         cheap rung did not run"
    );

    let failure = TopologyFold::parse_log(&fixture.log_bytes())
        .expect("the log parses")
        .into_iter()
        .find_map(|event| match event.body {
            TopologyEventBody::AttemptFinished { data } => data.record.failure.clone(),
            _ => None,
        })
        .expect("the settlement records a failure");

    assert_eq!(failure.kind, crate::ladder::FailureKind::ReviewInputOpaque);
    assert_eq!(failure.origin, crate::ladder::FailureOrigin::Reviewer);
    assert!(
        failure.reason.contains("filter"),
        "the reason is not the policy's: {}",
        failure.reason
    );
}

#[test]
fn the_retaining_incarnation_retries_in_place() {
    use crate::engine::topology::run::{Progress, RunSeams, TopologyRun};
    use crate::engine::topology::select::Ceiling;

    const RETRY_POOL: &str = "the-retrying-agents-pool";

    let fixture = Fixture::healthy("driver-retries");
    let caps = vec![(
        crate::engine::topology::scaffold::AGENT.to_owned(),
        crate::agent::Caps {
            version: "1.2.3".to_owned(),
            json_output: true,
            session_resume: true,
            cost_reporting: true,
            read_only_mode: true,
            acp: false,
            model_list: false,
        },
    )];
    let harness = harness();
    let (_recovered, handle) =
        resume_with_real_refs(&fixture, &harness).expect("the healthy resume completes");

    let mut run = TopologyRun::resumed(handle, fixture.inputs(), Ceiling::unlimited());
    let mut hooks = HarnessTopologyHooks::new(Arc::clone(&harness));
    let sleeper = RecordingSleeper::default();
    let manager = fixture.manager();
    let runner = RecordingRunner::editing();
    let adapters = crate::engine::topology::scaffold::ScaffoldAdapters::erroring();
    let paths = crate::rundir::RunPaths::with_private_root(
        &fixture.repo_root,
        &fixture.started.run_id,
        &fixture.private_root,
    );
    paths.create().expect("the run directories are creatable");
    let pools = vec![crate::capacity::Pool::discovered(
        RETRY_POOL,
        crate::capacity::PoolKind::SubscriptionWindow,
        crate::engine::topology::scaffold::AGENT,
        vec![crate::capacity::Source::Signals],
    )];
    let plans = crate::engine::assembly::FrozenPlans {
        adapters: &adapters,
        paths: &paths,
        gates: &[],
        pools: &pools,
        caps: &caps,
        worker_timeout: std::time::Duration::from_secs(300),
        decisions: &[],
    };
    let seams = RunSeams {
        manager: &manager,
        clock: &Frozen,
        sleeper: &sleeper,
        runner: &runner,
        adapters: &adapters,
        paths: &paths,
        plans: &plans,
        reviews: &crate::engine::attempt::LegacyReviewPasses,
        input_policy: &crate::engine::attempt::LegacyReviewInputPolicy,
        answers: &crate::interaction::UnattendedAnswers,
        ids: &FixedIds,
        halts_run: false,
    };

    let first = run
        .step(&seams, &mut hooks)
        .expect("the first attempt settles");
    let Progress::Settled { accepted, .. } = first else {
        panic!("the first iteration did not settle: {first:?}");
    };
    assert!(!accepted, "an agent error is not an acceptable attempt");

    let retained = TopologyFold::parse_log(&fixture.log_bytes())
        .expect("the log parses")
        .into_iter()
        .filter_map(|event| match event.body {
            TopologyEventBody::AttemptFinished { data } => Some(data.settlement),
            _ => None,
        })
        .next_back()
        .expect("the first attempt settled");
    assert!(
        matches!(retained, AttemptSettlement::Retained { .. }),
        "the generation did not retain its session: {retained:?}"
    );

    let second = run
        .step(&seams, &mut hooks)
        .expect("the retry runs in the retained generation");
    let Progress::Settled { key, .. } = second else {
        panic!("the second iteration did not run a retry: {second:?}");
    };
    assert_eq!(key, TaskKey(0));

    let starts: Vec<(u32, bool, Option<String>)> = TopologyFold::parse_log(&fixture.log_bytes())
        .expect("the log parses")
        .into_iter()
        .filter_map(|event| match event.body {
            TopologyEventBody::AttemptStarted { data } => Some((
                data.attempt.0,
                data.resume_session.is_some(),
                data.pool.clone(),
            )),
            _ => None,
        })
        .collect();
    assert_eq!(
        starts,
        vec![
            (1, false, Some(RETRY_POOL.to_owned())),
            (2, true, Some(RETRY_POOL.to_owned())),
        ],
        "the retry is not the same generation's second attempt on a resumed session, drawing on \
         the pool the assembler resolves for its agent. A `None` here is the ledger and the plan \
         disagreeing about which subscription one attempt drained"
    );
    assert_eq!(
        durable_kinds(&fixture)
            .iter()
            .filter(|kind| *kind == "task_dispatched")
            .count(),
        1,
        "the retry opened a fresh generation instead of continuing the retained one"
    );

    let briefed = runner
        .requests()
        .iter()
        .filter(|request| request.role == crate::runner::ExecutionRole::Implement)
        .filter(|request| {
            request
                .command
                .args
                .iter()
                .any(|arg| arg.contains("agent error"))
        })
        .count();
    assert_eq!(
        briefed, 1,
        "exactly one of the two worker prompts should carry the previous \
         attempt's failure, and it is the second"
    );

    assert!(
        run.invocations_balance(),
        "the invocation ledger does not balance, so some process was \
         registered and never settled"
    );

    let resumed = runner
        .requests()
        .iter()
        .filter(|request| request.role == crate::runner::ExecutionRole::Implement)
        .filter(|request| request.command.args.iter().any(|arg| arg == "--resume"))
        .count();
    assert_eq!(
        resumed, 1,
        "exactly one of the two worker invocations should carry a session to \
         resume, and it is the second"
    );
}

#[test]
fn a_refused_step_leaves_no_entitlement_held() {
    use crate::engine::topology::run::{Progress, RunSeams, TopologyRun};
    use crate::engine::topology::select::Ceiling;

    let fixture = Fixture::build(
        "refused-entitlement",
        Damage {
            extra: vec![
                dispatched(),
                attempt_started(1),
                attempt_finished(
                    1,
                    AttemptSettlement::Closed {
                        transition: SettlementTransition::Retry,
                        lease: LeaseDisposition::PredictedReleased,
                    },
                ),
            ],
            ..Damage::default()
        },
    );

    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);
    let (outcome, _) = resume_holding(&fixture, &harness, &given);
    let (_recovered, handle) = outcome.expect("the healthy resume completes");

    let mut run = TopologyRun::resumed(
        handle,
        fixture.inputs(),
        Ceiling {
            run_usd: Some(0.000_001),
            task_usd: None,
        },
    );
    assert!(
        run.spend().run_total() > 0.000_001,
        "the seeded attempt must cost more than the ceiling, or this test drives \
         an ordinary dispatch and asserts nothing about a refusal"
    );

    let mut hooks = HarnessTopologyHooks::new(Arc::clone(&harness));
    let sleeper = RecordingSleeper::default();
    let manager = fixture.manager();
    let runner = RecordingRunner::editing();
    let adapters = crate::engine::topology::scaffold::ScaffoldAdapters::new();
    let paths = crate::rundir::RunPaths::with_private_root(
        &fixture.repo_root,
        &fixture.started.run_id,
        &fixture.private_root,
    );
    paths.create().expect("the run directories are creatable");
    let plans = crate::engine::assembly::FrozenPlans {
        adapters: &adapters,
        paths: &paths,
        gates: &[],
        pools: &[],
        caps: &[],
        worker_timeout: std::time::Duration::from_secs(300),
        decisions: &[],
    };
    let seams = RunSeams {
        manager: &manager,
        clock: &Frozen,
        sleeper: &sleeper,
        runner: &runner,
        adapters: &adapters,
        paths: &paths,
        plans: &plans,
        reviews: &crate::engine::attempt::LegacyReviewPasses,
        input_policy: &crate::engine::attempt::LegacyReviewInputPolicy,
        answers: &crate::interaction::UnattendedAnswers,
        ids: &FixedIds,
        halts_run: false,
    };

    let progress = run.step(&seams, &mut hooks).expect("the ceiling refuses");
    assert!(
        matches!(progress, Progress::BudgetExceeded),
        "the seeded spend did not breach the ceiling: {progress:?}"
    );
    assert!(
        !run.holds_entitlement(),
        "the refused step is still holding a pipeline entitlement. At \
         `max_parallel = 1` that is the whole pipeline, held by a step that \
         did nothing"
    );
}

#[test]
fn a_retried_worker_is_told_what_the_last_attempt_failed_on() {
    use crate::engine::topology::run::{RunSeams, TopologyRun};
    use crate::engine::topology::select::Ceiling;

    let fixture = Fixture::build(
        "driver-brief",
        Damage {
            two_tier: true,
            ..Damage::default()
        },
    );

    let harness = harness();
    let (_recovered, handle) =
        resume_with_real_refs(&fixture, &harness).expect("the healthy resume completes");

    let mut run = TopologyRun::resumed(handle, fixture.inputs(), Ceiling::unlimited());
    let mut hooks = HarnessTopologyHooks::new(Arc::clone(&harness));
    let sleeper = RecordingSleeper::default();
    let manager = fixture.manager();
    let runner = RecordingRunner::editing();
    let adapters = crate::engine::topology::scaffold::ScaffoldAdapters::erroring();
    let paths = crate::rundir::RunPaths::with_private_root(
        &fixture.repo_root,
        &fixture.started.run_id,
        &fixture.private_root,
    );
    paths.create().expect("the run directories are creatable");
    let plans = crate::engine::assembly::FrozenPlans {
        adapters: &adapters,
        paths: &paths,
        gates: &[],
        pools: &[],
        caps: &[],
        worker_timeout: std::time::Duration::from_secs(300),
        decisions: &[],
    };
    let seams = RunSeams {
        manager: &manager,
        clock: &Frozen,
        sleeper: &sleeper,
        runner: &runner,
        adapters: &adapters,
        paths: &paths,
        plans: &plans,
        reviews: &crate::engine::attempt::LegacyReviewPasses,
        input_policy: &crate::engine::attempt::LegacyReviewInputPolicy,
        answers: &crate::interaction::UnattendedAnswers,
        ids: &FixedIds,
        halts_run: false,
    };

    run.step(&seams, &mut hooks)
        .expect("the first attempt settles");
    run.step(&seams, &mut hooks)
        .expect("the second attempt settles");

    let prompts: Vec<String> = runner
        .requests()
        .into_iter()
        .filter(|request| request.role == crate::runner::ExecutionRole::Implement)
        .map(|request| String::from_utf8_lossy(&request.command.stdin).into_owned())
        .collect();
    assert!(
        prompts.len() >= 2,
        "the fixture ran {} implementer(s); this test needs a second attempt to \
         have a prompt at all",
        prompts.len()
    );
    assert!(
        prompts[1].len() > prompts[0].len(),
        "the second worker's prompt is no longer than the first's, so nothing \
         was carried forward:\n--- first ---\n{}\n--- second ---\n{}",
        prompts[0],
        prompts[1]
    );

    let settled: Vec<serde_json::Value> = TopologyFold::parse_log(&fixture.log_bytes())
        .expect("the log parses")
        .into_iter()
        .filter_map(|event| match event.body {
            TopologyEventBody::AttemptFinished { data } => {
                serde_json::to_value(data.record.failure.as_ref()?).ok()
            }
            _ => None,
        })
        .collect();
    assert!(
        !settled.is_empty(),
        "no failed settlement in the log, so the assertion below is vacuous"
    );
    assert!(
        settled
            .iter()
            .any(|failure| failure["detail"].as_str().is_some_and(|d| !d.is_empty())),
        "every failed attempt this run settled carries `detail: null`, so the \
         schema-4 driver is asking for the legacy carrier and §11.4's feedback \
         is durable nowhere: {settled:?}"
    );
}

#[derive(Clone, Default)]
struct Timeline(Arc<Mutex<Vec<String>>>);

impl Timeline {
    fn push(&self, site: EffectSiteId, phase: HookPhase) {
        if phase != HookPhase::Before {
            return;
        }
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(site.to_string());
    }

    fn order(&self, of_interest: &[EffectSiteId]) -> Vec<String> {
        let names: Vec<String> = of_interest.iter().map(ToString::to_string).collect();
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|seen| names.contains(seen))
            .cloned()
            .collect()
    }
}

struct TracedEffects {
    inner: crate::workspace_manager::HarnessEffects,
    timeline: Timeline,
}

impl crate::workspace_manager::EffectHooks for TracedEffects {
    fn phase(
        &mut self,
        site: EffectSiteId,
        phase: HookPhase,
    ) -> crate::topology::effects::Injection {
        let answered = self.inner.phase(site, phase);
        self.timeline.push(site, phase);
        answered
    }

    fn durability_ledger(&self) -> crate::util::DurabilityLedger {
        self.inner.durability_ledger()
    }

    fn refusal_cause(&self) -> Option<String> {
        self.inner.refusal_cause()
    }
}

struct TracedEvents {
    inner: crate::events::log::HarnessEventHooks,
    timeline: Timeline,
}

impl crate::events::log::EventHooks for TracedEvents {
    fn phase(&mut self, site: crate::topology::effects::EventSite, phase: HookPhase) {
        self.inner.phase(site, phase);
        self.timeline.push(EffectSiteId::Event(site), phase);
    }
}

struct TracedHooks {
    effects: TracedEffects,
    events: TracedEvents,
    rest: HarnessTopologyHooks,
    timeline: Timeline,
}

impl TracedHooks {
    fn new(harness: &Arc<Mutex<HookHarness>>) -> Self {
        let timeline = Timeline::default();
        Self {
            effects: TracedEffects {
                inner: crate::workspace_manager::HarnessEffects::new(Arc::clone(harness)),
                timeline: timeline.clone(),
            },
            events: TracedEvents {
                inner: crate::events::log::HarnessEventHooks::new(Arc::clone(harness)),
                timeline: timeline.clone(),
            },
            rest: HarnessTopologyHooks::new(Arc::clone(harness)),
            timeline,
        }
    }
}

impl TopologyHooks for TracedHooks {
    fn effects(&mut self) -> &mut dyn crate::workspace_manager::EffectHooks {
        &mut self.effects
    }

    fn rundir(&mut self) -> &mut dyn crate::rundir::RunDirHooks {
        self.rest.rundir()
    }

    fn events(&mut self) -> &mut dyn crate::events::log::EventHooks {
        &mut self.events
    }

    fn container(&mut self) -> &mut dyn crate::runner::container::ContainerHooks {
        self.rest.container()
    }

    fn spawn(&mut self) -> &mut dyn crate::agent::proc::SpawnHooks {
        self.rest.spawn()
    }
}

#[test]
fn the_driver_escalates_onto_the_rung_above() {
    use crate::engine::topology::run::{RunSeams, TopologyRun};
    use crate::engine::topology::select::Ceiling;

    let fixture = Fixture::build(
        "driver-escalates",
        Damage {
            two_tier: true,
            ..Damage::default()
        },
    );

    let harness = harness();
    let (_recovered, handle) =
        resume_with_real_refs(&fixture, &harness).expect("the healthy resume completes");

    let mut run = TopologyRun::resumed(handle, fixture.inputs(), Ceiling::unlimited());
    let mut hooks = HarnessTopologyHooks::new(Arc::clone(&harness));
    let sleeper = RecordingSleeper::default();
    let manager = fixture.manager();
    let runner = RecordingRunner::editing();
    let adapters = crate::engine::topology::scaffold::ScaffoldAdapters::erroring();
    let paths = crate::rundir::RunPaths::with_private_root(
        &fixture.repo_root,
        &fixture.started.run_id,
        &fixture.private_root,
    );
    paths.create().expect("the run directories are creatable");
    let plans = crate::engine::assembly::FrozenPlans {
        adapters: &adapters,
        paths: &paths,
        gates: &[],
        pools: &[],
        caps: &[],
        worker_timeout: std::time::Duration::from_secs(300),
        decisions: &[],
    };
    let seams = RunSeams {
        manager: &manager,
        clock: &Frozen,
        sleeper: &sleeper,
        runner: &runner,
        adapters: &adapters,
        paths: &paths,
        plans: &plans,
        reviews: &crate::engine::attempt::LegacyReviewPasses,
        input_policy: &crate::engine::attempt::LegacyReviewInputPolicy,
        answers: &crate::interaction::UnattendedAnswers,
        ids: &FixedIds,
        halts_run: false,
    };

    run.step(&seams, &mut hooks)
        .expect("the first attempt settles");

    let escalated = TopologyFold::parse_log(&fixture.log_bytes())
        .expect("the log parses")
        .into_iter()
        .find_map(|event| match event.body {
            TopologyEventBody::AttemptFinished { data } => match data.settlement {
                AttemptSettlement::Closed {
                    transition: SettlementTransition::Escalated { rung },
                    ..
                } => Some(rung),
                _ => None,
            },
            _ => None,
        })
        .expect("the exhausted rung escalates");

    assert_eq!(
        escalated, 1,
        "the driver recorded an escalation onto rung {escalated}, the rung it is \
         leaving. The fold assigns `task.rung` from this number and resets the \
         allowance, so the task is selected again at the same tier and loops \
         forever — never reaching the tier its chain escalated it to"
    );

    run.step(&seams, &mut hooks)
        .expect("the second attempt settles");
    let ran_at = TopologyFold::parse_log(&fixture.log_bytes())
        .expect("the log parses")
        .into_iter()
        .filter_map(|event| match event.body {
            TopologyEventBody::AttemptStarted { data } => Some(data.binding.model.clone()),
            _ => None,
        })
        .next_back()
        .expect("the driver started a second attempt");
    assert_eq!(
        ran_at, "claude-fable-5",
        "the escalated task ran at {ran_at}, which is rung 0's model"
    );

    run.step(&seams, &mut hooks)
        .expect("the exhausted chain settles");
    let parked = TopologyFold::parse_log(&fixture.log_bytes())
        .expect("the log parses")
        .into_iter()
        .find_map(|event| match event.body {
            TopologyEventBody::AttemptFinished { data } => match data.settlement {
                AttemptSettlement::Closed {
                    transition: SettlementTransition::Parked { question },
                    ..
                } => Some(question),
                _ => None,
            },
            _ => None,
        })
        .expect("the exhausted chain parks a question");

    assert!(
        parked.context.contains("2 attempt(s) across 2 rung(s)"),
        "the human is told the wrong history of this task. Two attempts across \
         two rungs failed; the question says:\n{}",
        parked.context
    );
}

#[test]
fn the_driver_dispatches_at_the_rung_the_log_records() {
    use crate::engine::topology::run::{RunSeams, TopologyRun};
    use crate::engine::topology::select::Ceiling;

    let fixture = Fixture::build(
        "driver-rung",
        Damage {
            two_tier: true,
            extra: vec![
                dispatched(),
                attempt_started(1),
                attempt_finished(
                    1,
                    AttemptSettlement::Closed {
                        transition: SettlementTransition::Escalated { rung: 1 },
                        lease: LeaseDisposition::PredictedReleased,
                    },
                ),
            ],
            ..Damage::default()
        },
    );

    let harness = harness();
    let (_recovered, handle) =
        resume_with_real_refs(&fixture, &harness).expect("the healthy resume completes");

    let mut run = TopologyRun::resumed(handle, fixture.inputs(), Ceiling::unlimited());
    let mut hooks = HarnessTopologyHooks::new(Arc::clone(&harness));
    let sleeper = RecordingSleeper::default();
    let manager = fixture.manager();
    let runner = RecordingRunner::editing();
    let adapters = crate::engine::topology::scaffold::ScaffoldAdapters::erroring();
    let paths = crate::rundir::RunPaths::with_private_root(
        &fixture.repo_root,
        &fixture.started.run_id,
        &fixture.private_root,
    );
    paths.create().expect("the run directories are creatable");
    let plans = crate::engine::assembly::FrozenPlans {
        adapters: &adapters,
        paths: &paths,
        gates: &[],
        pools: &[],
        caps: &[],
        worker_timeout: std::time::Duration::from_secs(300),
        decisions: &[],
    };
    let seams = RunSeams {
        manager: &manager,
        clock: &Frozen,
        sleeper: &sleeper,
        runner: &runner,
        adapters: &adapters,
        paths: &paths,
        plans: &plans,
        reviews: &crate::engine::attempt::LegacyReviewPasses,
        input_policy: &crate::engine::attempt::LegacyReviewInputPolicy,
        answers: &crate::interaction::UnattendedAnswers,
        ids: &FixedIds,
        halts_run: false,
    };

    run.step(&seams, &mut hooks).expect("the attempt settles");

    let ran_at = TopologyFold::parse_log(&fixture.log_bytes())
        .expect("the log parses")
        .into_iter()
        .filter_map(|event| match event.body {
            TopologyEventBody::AttemptStarted { data } => Some(data.binding.model.clone()),
            _ => None,
        })
        .next_back()
        .expect("the driver started an attempt");

    assert_eq!(
        ran_at, "claude-fable-5",
        "the task escalated onto rung 1 and the driver ran it at {ran_at}, which \
         is rung 0's model. An escalated task dispatched at rung 0 never reaches \
         the tier its chain escalated it to, and the only symptom is a task that \
         never gets better"
    );
}

#[test]
fn a_crash_does_not_erase_what_the_last_attempt_was_told_to_fix() {
    const TAIL: &str = "error[E0308]: mismatched types\n  --> src/alpha.rs:12:9\n   \
                        expected `u32`, found `&str`";

    let fixture = Fixture::build(
        "driver-brief-resume",
        Damage {
            extra: vec![
                dispatched(),
                attempt_started(1),
                attempt_finished_failing(
                    1,
                    crate::ladder::FailureKind::GateFailed,
                    "gate `cargo test` failed: 1 failed",
                    TAIL,
                    AttemptSettlement::Closed {
                        transition: SettlementTransition::Retry,
                        lease: LeaseDisposition::PredictedReleased,
                    },
                ),
            ],
            ..Damage::default()
        },
    );

    let runner = RecordingRunner::editing();
    let prompts = drive_one_attempt(&fixture, &runner);

    assert_eq!(
        prompts.len(),
        1,
        "the resumed run dispatched {} implementer(s); this test needs exactly \
         the one the log entitles it to",
        prompts.len()
    );
    assert!(
        prompts[0].contains(TAIL),
        "the retry after the crash was not told what the gate printed. §11.4 \
         sends the gate log back to the same rung, and this prompt carries none \
         of it:\n--- prompt ---\n{}",
        prompts[0]
    );
}

#[test]
fn an_escalation_after_a_crash_carries_the_accumulated_feedback() {
    const FIRST_SUMMARY: &str = "review failed: the parser accepts a trailing comma";
    const SECOND_SUMMARY: &str = "review failed: the empty list still panics";
    const SECOND_DETAIL: &str = "- reject a trailing comma in `parse_list`\n\
                                 - the empty list must round-trip";

    let fixture = Fixture::build(
        "driver-brief-escalate",
        Damage {
            deep_ladder: true,
            extra: vec![
                dispatched(),
                attempt_started(1),
                attempt_finished_failing(
                    1,
                    crate::ladder::FailureKind::ReviewFailed,
                    FIRST_SUMMARY,
                    "- reject a trailing comma in `parse_list`",
                    AttemptSettlement::Closed {
                        transition: SettlementTransition::Retry,
                        lease: LeaseDisposition::PredictedReleased,
                    },
                ),
                in_generation(GenerationId(1), dispatched()),
                in_generation(GenerationId(1), attempt_started(1)),
                in_generation(
                    GenerationId(1),
                    attempt_finished_failing(
                        1,
                        crate::ladder::FailureKind::ReviewFailed,
                        SECOND_SUMMARY,
                        SECOND_DETAIL,
                        AttemptSettlement::Closed {
                            transition: SettlementTransition::Escalated { rung: 1 },
                            lease: LeaseDisposition::PredictedReleased,
                        },
                    ),
                ),
            ],
            ..Damage::default()
        },
    );

    let runner = RecordingRunner::editing();
    let prompts = drive_one_attempt(&fixture, &runner);
    assert_eq!(
        prompts.len(),
        1,
        "the resumed run dispatched {} implementer(s); this test needs the \
         escalation the log entitles it to",
        prompts.len()
    );
    let prompt = &prompts[0];

    for summary in [FIRST_SUMMARY, SECOND_SUMMARY] {
        assert!(
            prompt.contains(summary),
            "the escalated worker was not told `{summary}`. §11.4 carries the \
             accumulated feedback onto the next rung, and this prompt carries \
             part of it at best:\n--- prompt ---\n{prompt}"
        );
    }
    assert!(
        prompt.contains(SECOND_DETAIL),
        "the escalated worker was not given the reviewer's required changes \
         verbatim. §11.2 is what the retry gets back, and after a crash it \
         reached this prompt as a summary or not at \
         all:\n--- prompt ---\n{prompt}"
    );
    assert!(
        prompt.contains("Earlier attempts at this task failed"),
        "the escalated worker's prompt has no accumulated section, so at most \
         one record below its rung reached it:\n--- prompt ---\n{prompt}"
    );
}

#[test]
fn a_log_predating_the_detail_field_folds_and_resumes() {
    let fixture = Fixture::build(
        "driver-brief-oldlog",
        Damage {
            extra: vec![
                dispatched(),
                attempt_started(1),
                attempt_finished_failing(
                    1,
                    crate::ladder::FailureKind::GateFailed,
                    "gate `cargo test` failed: 1 failed",
                    "error[E0308]: mismatched types",
                    AttemptSettlement::Closed {
                        transition: SettlementTransition::Retry,
                        lease: LeaseDisposition::PredictedReleased,
                    },
                ),
            ],
            ..Damage::default()
        },
    );

    let current = String::from_utf8(fixture.log_bytes()).expect("the log is utf-8");
    let aged: String = current
        .lines()
        .enumerate()
        .map(|(position, line)| {
            if position == 0 {
                return format!("{line}\n");
            }
            let mut value: serde_json::Value =
                serde_json::from_str(line).expect("every log line is a json object");
            if let Some(failure) = value.pointer_mut("/data/record/failure") {
                if let Some(object) = failure.as_object_mut() {
                    object.remove("detail");
                }
            }
            format!("{value}\n")
        })
        .collect();
    assert!(
        !aged.contains("\"detail\""),
        "the aged log still carries a detail key, so this test is reading the \
         current shape and proving nothing about the older one"
    );
    assert!(
        aged.contains("attempt_finished"),
        "the aged log has no settlement in it, so the field being absent is \
         vacuous"
    );

    let events = TopologyFold::parse_log(aged.as_bytes()).expect(
        "a log written before the detail field existed still parses — if this \
         refuses, the field is not additive and SCHEMA_VERSION had to move",
    );
    let details: Vec<Option<String>> = events
        .iter()
        .filter_map(|event| match &event.body {
            TopologyEventBody::AttemptFinished { data } => {
                Some(data.record.failure.as_ref()?.detail.clone())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        details,
        vec![None],
        "an absent detail key must read back as None; anything else means an \
         older log folds to a different value than it was written with"
    );

    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);
    crate::workspace_manager::fixture::write_file(&fixture.log(), aged.as_bytes());
    let (outcome, _) = resume_holding(&fixture, &harness, &given);
    let (_recovered, handle) = outcome.expect("a run whose log predates the field still resumes");

    let brief = crate::engine::topology::run::Brief::replay(&handle.events);
    let lines = brief.lines(ALPHA);
    assert_eq!(
        lines.len(),
        1,
        "an older log's failure must still contribute its summary; the brief holds \
         {} line(s): {lines:?}",
        lines.len()
    );
    assert_eq!(
        lines[0].summary, "gate `cargo test` failed: 1 failed",
        "the summary is what an older log preserved and it must reach the next worker"
    );
    assert_eq!(
        lines[0].detail, None,
        "a log that never recorded the tail cannot produce one"
    );
}

fn drive_one_attempt(fixture: &Fixture, runner: &RecordingRunner) -> Vec<String> {
    use crate::engine::topology::run::{RunSeams, TopologyRun};
    use crate::engine::topology::select::Ceiling;

    let harness = harness();
    let (_recovered, handle) =
        resume_with_real_refs(fixture, &harness).expect("the healthy resume completes");

    let mut run = TopologyRun::resumed(handle, fixture.inputs(), Ceiling::unlimited());
    let mut hooks = HarnessTopologyHooks::new(Arc::clone(&harness));
    let sleeper = RecordingSleeper::default();
    let manager = fixture.manager();
    let adapters = crate::engine::topology::scaffold::ScaffoldAdapters::erroring();
    let paths = crate::rundir::RunPaths::with_private_root(
        &fixture.repo_root,
        &fixture.started.run_id,
        &fixture.private_root,
    );
    paths.create().expect("the run directories are creatable");
    let plans = crate::engine::assembly::FrozenPlans {
        adapters: &adapters,
        paths: &paths,
        gates: &[],
        pools: &[],
        caps: &[],
        worker_timeout: std::time::Duration::from_secs(300),
        decisions: &[],
    };
    let seams = RunSeams {
        manager: &manager,
        clock: &Frozen,
        sleeper: &sleeper,
        runner,
        adapters: &adapters,
        paths: &paths,
        plans: &plans,
        reviews: &crate::engine::attempt::LegacyReviewPasses,
        input_policy: &crate::engine::attempt::LegacyReviewInputPolicy,
        answers: &crate::interaction::UnattendedAnswers,
        ids: &FixedIds,
        halts_run: false,
    };

    run.step(&seams, &mut hooks)
        .expect("the resumed attempt settles");

    runner
        .requests()
        .into_iter()
        .filter(|request| request.role == crate::runner::ExecutionRole::Implement)
        .map(|request| String::from_utf8_lossy(&request.command.stdin).into_owned())
        .collect()
}

#[test]
fn the_driver_spends_the_allowance_the_log_records() {
    use crate::engine::topology::run::{Progress, RunSeams, TopologyRun};
    use crate::engine::topology::select::Ceiling;

    let fixture = Fixture::build(
        "driver-allowance",
        Damage {
            extra: vec![
                dispatched(),
                attempt_started(1),
                attempt_finished(
                    1,
                    AttemptSettlement::Closed {
                        transition: SettlementTransition::Retry,
                        lease: LeaseDisposition::PredictedReleased,
                    },
                ),
            ],
            ..Damage::default()
        },
    );
    let harness = harness();
    let (_recovered, handle) =
        resume_with_real_refs(&fixture, &harness).expect("the healthy resume completes");

    let mut run = TopologyRun::resumed(handle, fixture.inputs(), Ceiling::unlimited());
    let mut hooks = HarnessTopologyHooks::new(Arc::clone(&harness));
    let sleeper = RecordingSleeper::default();
    let manager = fixture.manager();
    let runner = RecordingRunner::editing();
    let adapters = crate::engine::topology::scaffold::ScaffoldAdapters::erroring();
    let paths = crate::rundir::RunPaths::with_private_root(
        &fixture.repo_root,
        &fixture.started.run_id,
        &fixture.private_root,
    );
    paths.create().expect("the run directories are creatable");
    let plans = crate::engine::assembly::FrozenPlans {
        adapters: &adapters,
        paths: &paths,
        gates: &[],
        pools: &[],
        caps: &[],
        worker_timeout: std::time::Duration::from_secs(300),
        decisions: &[],
    };
    let seams = RunSeams {
        manager: &manager,
        clock: &Frozen,
        sleeper: &sleeper,
        runner: &runner,
        adapters: &adapters,
        paths: &paths,
        plans: &plans,
        reviews: &crate::engine::attempt::LegacyReviewPasses,
        input_policy: &crate::engine::attempt::LegacyReviewInputPolicy,
        answers: &crate::interaction::UnattendedAnswers,
        ids: &FixedIds,
        halts_run: false,
    };

    let progress = run
        .step(&seams, &mut hooks)
        .expect("the second attempt settles");
    let Progress::Settled { accepted, .. } = progress else {
        panic!("the ready-dispatch branch did not settle: {progress:?}");
    };
    assert!(!accepted, "an agent error is not an acceptable attempt");

    let last = TopologyFold::parse_log(&fixture.log_bytes())
        .expect("the log parses")
        .into_iter()
        .filter_map(|event| match event.body {
            TopologyEventBody::AttemptFinished { data } => Some(data.settlement),
            _ => None,
        })
        .next_back()
        .expect("the attempt settled");

    let AttemptSettlement::Closed { transition, .. } = last else {
        panic!("the second attempt did not close its generation: {last:?}");
    };
    let SettlementTransition::Parked { question } = transition else {
        panic!(
            "the second attempt on a two-attempt rung with nowhere to escalate \
             settled as {transition:?}. A driver reading a constant \
             `attempts_on_rung: 1` gets `Retry` here and the task retries forever"
        );
    };

    assert!(
        question.context.contains("2 attempt(s)"),
        "the question quotes the wrong attempt count: {}",
        question.context
    );
}

#[test]
fn the_loop_continues_an_attempt_recovery_recreated() {
    use crate::engine::topology::run::{Progress, RunSeams, TopologyRun};
    use crate::engine::topology::select::Ceiling;

    let fixture = Fixture::build(
        "continue-open",
        Damage {
            open_generation: true,
            ..Damage::default()
        },
    );
    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);

    let (outcome, _) = resume_holding(&fixture, &harness, &given);
    let (_recovered, handle) = outcome.expect("the healthy resume completes");

    let mut run = TopologyRun::resumed(handle, fixture.inputs(), Ceiling::unlimited());
    let mut hooks = HarnessTopologyHooks::new(Arc::clone(&harness));
    let sleeper = RecordingSleeper::default();
    let manager = fixture.manager();
    let runner = RecordingRunner::editing();
    let adapters = crate::engine::topology::scaffold::ScaffoldAdapters::erroring();
    let paths = crate::rundir::RunPaths::with_private_root(
        &fixture.repo_root,
        &fixture.started.run_id,
        &fixture.private_root,
    );
    paths.create().expect("the run directories are creatable");
    let plans = crate::engine::assembly::FrozenPlans {
        adapters: &adapters,
        paths: &paths,
        gates: &[],
        pools: &[],
        caps: &[],
        worker_timeout: std::time::Duration::from_secs(300),
        decisions: &[],
    };
    let seams = RunSeams {
        manager: &manager,
        clock: &Frozen,
        sleeper: &sleeper,
        runner: &runner,
        adapters: &adapters,
        paths: &paths,
        plans: &plans,
        reviews: &crate::engine::attempt::LegacyReviewPasses,
        input_policy: &crate::engine::attempt::LegacyReviewInputPolicy,
        answers: &crate::interaction::UnattendedAnswers,
        ids: &FixedIds,
        halts_run: false,
    };

    let before = durable_kinds(&fixture);
    assert_eq!(
        before.iter().filter(|k| *k == "task_dispatched").count(),
        1,
        "the fixture must leave exactly one dispatch for the continuation to reuse"
    );

    let progress = run
        .step(&seams, &mut hooks)
        .expect("the loop continues the attempt rather than stalling");
    let Progress::Settled { key, .. } = progress else {
        panic!("the ready-dispatch branch did not continue the attempt: {progress:?}");
    };
    assert_eq!(key, TaskKey(0));

    let after = durable_kinds(&fixture);
    assert_eq!(
        after.iter().filter(|k| *k == "task_dispatched").count(),
        1,
        "the continuation opened a fresh generation instead of continuing the \
         one recovery recreated — `T-DISPATCH` says continue attempt, no spend \
         repeats"
    );
    assert_eq!(
        after.iter().filter(|k| *k == "attempt_started").count(),
        1,
        "the continuation started no attempt, so the entitlement is still held \
         by a generation nothing can drive"
    );
}

#[test]
fn a_reviewer_runs_at_the_review_effort_not_the_implementers() {
    let fixture = Fixture::healthy("review-effort");
    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);
    let (outcome, _) = resume_holding(&fixture, &harness, &given);
    let (_recovered, handle) = outcome.expect("the healthy resume completes");

    let entry = handle
        .fold
        .registry()
        .and_then(|registry| registry.get(TaskKey(0)))
        .expect("the fixture registers alpha");
    assert_eq!(
        entry.ladder.effort.review,
        Effort::Medium,
        "the fixture's review axis moved; this test needs it to differ from the rung's"
    );
    assert_eq!(
        entry
            .ladder
            .effort
            .implementation_for(entry.ladder.rungs[0].tier),
        Effort::High,
        "the fixture's Mid rung moved; this test needs it to differ from review"
    );

    let manager = fixture.manager();
    let adapters = crate::engine::topology::scaffold::ScaffoldAdapters::new();
    let paths = crate::rundir::RunPaths::with_private_root(
        &fixture.repo_root,
        &fixture.started.run_id,
        &fixture.private_root,
    );
    paths.create().expect("the run directories are creatable");
    use crate::engine::topology::scaffold::REVIEW_AGENT;

    let mut reviewer_bound = entry.clone();
    reviewer_bound.reviews.primary = Some(crate::review::PassBinding::new(REVIEW_AGENT, "gpt"));
    let entry = &reviewer_bound;
    let pools = vec![
        crate::capacity::Pool::discovered(
            "the-implementers-pool",
            crate::capacity::PoolKind::SubscriptionWindow,
            AGENT,
            vec![crate::capacity::Source::Signals],
        ),
        crate::capacity::Pool::discovered(
            "the-reviewers-own-pool",
            crate::capacity::PoolKind::SubscriptionWindow,
            REVIEW_AGENT,
            vec![crate::capacity::Source::Signals],
        ),
    ];
    let plans = crate::engine::assembly::FrozenPlans {
        adapters: &adapters,
        paths: &paths,
        gates: &[],
        pools: &pools,
        caps: &[],
        worker_timeout: std::time::Duration::from_secs(300),
        decisions: &[],
    };
    let binding = handle
        .fold
        .frozen_rung_binding(TaskKey(0), 0)
        .expect("rung 0 is frozen");
    let plan = crate::engine::topology::attempt::AttemptPlans::plan(
        &plans,
        &crate::engine::topology::attempt::PlanRequest {
            key: TaskKey(0),
            entry,
            attempt: crate::topology::events::AttemptNumber(1),
            rung: 0,
            binding,
            workspace: &fixture.repo_root,
            resume_session: None,
            feedback: Vec::new(),
            materialization_observed: None,
        },
    )
    .expect("the plan assembles");

    assert!(
        !plan.reviewers.is_empty(),
        "this fixture plans no reviewer, so the effort below is unasserted"
    );
    assert_eq!(
        plan.pool.as_deref(),
        Some("the-implementers-pool"),
        "the implementer did not resolve its own agent's pool, so a reviewer carrying that value \
         would not tell us anything"
    );
    for reviewer in &plan.reviewers {
        assert_eq!(
            reviewer.agent.as_str(),
            REVIEW_AGENT,
            "reviewer `{}` runs on the implementer's agent, so its pool lookup and the \
             implementer's are one lookup and both behaviours pass",
            reviewer.lens.name()
        );
        assert_eq!(
            reviewer.profile.effort,
            Some(Effort::Medium),
            "reviewer `{}` runs at the implementer's effort",
            reviewer.lens.name()
        );

        assert_eq!(
            reviewer.profile.pool,
            "the-reviewers-own-pool",
            "reviewer `{}` carries pool `{}`",
            reviewer.lens.name(),
            reviewer.profile.pool
        );
    }
    let _ = manager;
}

#[test]
fn the_loop_inherits_the_committed_digest_recovery_verified() {
    let fixture = Fixture::healthy("digest-inherited");
    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);

    let (outcome, _) = resume_holding(&fixture, &harness, &given);
    let (_recovered, handle) = outcome.expect("the healthy resume completes");

    let expected = crate::rundir::run_started_sha256(&fixture.first_line);

    assert_eq!(
        handle.committed_first_line_sha256, expected,
        "the handle carries a digest that is not the committed first line's"
    );
    assert!(
        !handle.committed_first_line_sha256.is_empty(),
        "an empty digest asserts nothing: this fixture must publish a commit \
         record for the comparison to mean anything"
    );

    let run = crate::engine::topology::run::TopologyRun::resumed(
        handle,
        fixture.inputs(),
        crate::engine::topology::select::Ceiling::unlimited(),
    );
    assert_eq!(
        run.commitment_digest(),
        Some(expected.as_str()),
        "the loop's appends cannot prove their committed first line"
    );
}

#[test]
fn a_prepared_pin_without_a_candidate_record_is_orphan_residue() {
    let fixture = Fixture::build(
        "e6-orphan",
        Damage {
            open_generation: true,
            extra: vec![attempt_started(1)],
            ..Damage::default()
        },
    );
    let commit = seed_candidate_commit(&fixture, 0);

    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);
    let (outcome, _) = resume_holding(&fixture, &harness, &given);
    let (recovered, _handle) = outcome.expect("the resume completes rather than refusing");

    assert_eq!(
        recovered.interrupted, 1,
        "the attempt was running and nothing settled it, so the resume settles it \
         interrupted"
    );
    assert!(
        recovered.finished.is_empty(),
        "there is no candidate record, so there is no promotion to carry through: {:?}",
        recovered.finished
    );

    let prepared: Vec<_> = TopologyFold::parse_log(&fixture.log_bytes())
        .expect("the log parses")
        .into_iter()
        .filter(|event| matches!(event.body, TopologyEventBody::CandidatePrepared { .. }))
        .collect();
    assert!(
        prepared.is_empty(),
        "recovery synthesised a candidate around the pinned commit {commit:?}; with one \
         atomic settlement a pin without a record is residue, not authorization"
    );
}

fn seed_candidate_commit(fixture: &Fixture, generation: u32) -> String {
    use crate::workspace_manager::fixture::{git, write_file};

    let repo = &fixture.repo_root;
    write_file(&repo.join("candidate.txt"), b"the worker's edit\n");
    git(repo, &["add", "--", "candidate.txt"]);
    let tree = git(repo, &["write-tree"]);
    let commit = git(
        repo,
        &[
            "commit-tree",
            &tree,
            "-p",
            fixture.base_sha.as_str(),
            "-m",
            "upstroke: alpha attempt 1",
        ],
    );
    git(repo, &["rm", "-q", "-f", "--", "candidate.txt"]);
    let pin = crate::engine::topology::candidate::candidate_pin_ref(
        RUN_ID,
        TaskKey(0),
        GenerationId(generation),
    );
    git(repo, &["update-ref", pin.as_str(), &commit]);
    commit
}

struct PlantedTransaction {
    candidate: crate::topology::events::CandidateRef,
    commit: CommitSha,
    tree: CommitSha,
}

fn append_events(fixture: &Fixture, bodies: &[TopologyEventBody]) {
    let mut warnings = Vec::new();
    let mut log = EventLog::open(EventSite::OpenLog, &fixture.log(), &mut warnings)
        .expect("reopen the fixture log");
    for body in bodies {
        let site = crate::events::log::site_for(body);
        let (line, _) =
            TopologyLine::round_trip(&event(body.clone())).expect("a valid later event");
        log.append_topology(site, &line).expect("a later append");
    }
}

fn alpha_commit(fixture: &Fixture) -> (CommitSha, CommitSha) {
    use crate::workspace_manager::fixture::{git, write_file};
    let repo = &fixture.repo_root;
    write_file(&repo.join("candidate.txt"), b"the candidate edit\n");
    git(repo, &["add", "--", "candidate.txt"]);
    let tree = git(repo, &["write-tree"]);
    let commit = git(
        repo,
        &[
            "commit-tree",
            &tree,
            "-p",
            fixture.base_sha.as_str(),
            "-m",
            "upstroke: alpha attempt 1",
        ],
    );
    git(repo, &["rm", "-q", "-f", "--", "candidate.txt"]);
    (CommitSha(commit), CommitSha(tree))
}

fn obliged_reviews_for(fixture: &Fixture, key: TaskKey) -> Vec<crate::events::ReviewRecord> {
    let events = TopologyFold::parse_log(&fixture.log_bytes()).expect("the log parses");
    let fold = TopologyFold::replay(fixture.inputs(), &events).expect("the log replays");
    fold.registry()
        .expect("a registry")
        .get(key)
        .expect("the task is registered")
        .reviews
        .obliged_lenses()
        .into_iter()
        .map(|lens| crate::events::ReviewRecord {
            pass: lens.name().to_owned(),
            agent: AGENT.to_owned(),
            model: "claude-opus-5".to_owned(),
            adapter: None,
            preflight_cli_version: None,
            effort: None,
            pool: None,
            cost_usd: Some(0.1),
            outcome: crate::events::ReviewPassOutcome::Passed,
        })
        .collect()
}

fn alpha_candidate_prepared(
    fixture: &Fixture,
    commit: &CommitSha,
    tree: &CommitSha,
    names: &crate::engine::topology::candidate::CandidateNames,
) -> TopologyEventBody {
    candidate_prepared_for(fixture, ALPHA, commit, tree, names, "candidate.txt")
}

fn candidate_prepared_for(
    fixture: &Fixture,
    key: TaskKey,
    commit: &CommitSha,
    tree: &CommitSha,
    names: &crate::engine::topology::candidate::CandidateNames,
    path: &str,
) -> TopologyEventBody {
    let paths = PathSet::Prefixes {
        paths: vec![GitPath(path.to_owned())],
    };
    let mut attempt = attempt_record(1);
    attempt.reviews = obliged_reviews_for(fixture, key);
    TopologyEventBody::CandidatePrepared {
        data: Box::new(crate::topology::events::CandidatePrepared {
            key,
            generation: GEN,
            attempt: Box::new(attempt),
            base_sha: fixture.base_sha.clone(),
            parent_sha: fixture.base_sha.clone(),
            tree_sha: tree.clone(),
            commit_sha: commit.clone(),
            message: format!("upstroke: task {} attempt 1", key.0),
            prepared_ref: names.prepared_ref.clone(),
            candidate_ref: names.candidate_ref.clone(),
            actual_paths: paths.clone(),
            lease_effect: crate::topology::events::CandidateLeaseEffect::ReplacesPredicted {
                paths,
            },
        }),
    }
}

fn plant_queued_candidate(fixture: &Fixture) -> PlantedTransaction {
    crate::workspace_manager::fixture::git(
        &fixture.repo_root,
        &[
            "update-ref",
            fixture.started.integration_ref.as_str(),
            fixture.base_sha.as_str(),
        ],
    );
    plant_queued_candidate_events(fixture)
}

fn plant_queued_candidate_events(fixture: &Fixture) -> PlantedTransaction {
    use crate::workspace_manager::fixture::git;
    let (commit, tree) = alpha_commit(fixture);
    let names = crate::engine::topology::candidate::CandidateNames::of(RUN_ID, ALPHA, GEN);
    git(
        &fixture.repo_root,
        &["update-ref", names.prepared_ref.as_str(), commit.as_str()],
    );
    git(
        &fixture.repo_root,
        &["update-ref", names.candidate_ref.as_str(), commit.as_str()],
    );
    let candidate = crate::topology::events::CandidateRef {
        key: ALPHA,
        generation: GEN,
        commit_sha: commit.clone(),
        candidate_ref: names.candidate_ref.clone(),
    };
    append_events(
        fixture,
        &[
            dispatched_at(&fixture.base_sha),
            attempt_started_in(fixture, 1),
            alpha_candidate_prepared(fixture, &commit, &tree, &names),
            TopologyEventBody::TaskCandidateCreated {
                data: crate::topology::events::TaskCandidateCreated {
                    candidate: candidate.clone(),
                },
            },
        ],
    );
    PlantedTransaction {
        candidate,
        commit,
        tree,
    }
}

fn fast_prepared(fixture: &Fixture, planted: &PlantedTransaction) -> TopologyEventBody {
    TopologyEventBody::MergePrepared {
        data: Box::new(crate::topology::events::MergePrepared {
            sequence: crate::topology::events::SequenceId(0),
            disposition: crate::topology::events::PreparedDisposition::Fast,
            expected_head: fixture.base_sha.clone(),
            proposed_sha: planted.commit.clone(),
            key: ALPHA,
            generation: GEN,
            candidate_sha: planted.commit.clone(),
            candidate_ref: planted.candidate.candidate_ref.clone(),
            prepared_ref: None,
            verification_source: crate::topology::events::VerificationSource::CandidatePrepared {
                key: ALPHA,
                generation: GEN,
            },
            verification: None,
            satisfies: vec![ALPHA],
        }),
    }
}

fn plant_prepared_fast(fixture: &Fixture) -> PlantedTransaction {
    let planted = plant_queued_candidate(fixture);
    append_events(fixture, &[fast_prepared(fixture, &planted)]);
    planted
}

fn ref_target(fixture: &Fixture, refname: &str) -> Option<String> {
    fixture
        .manager()
        .direct_ref_target(refname)
        .expect("read a ref")
}

fn merged_sequences(fixture: &Fixture) -> Vec<u32> {
    TopologyFold::parse_log(&fixture.log_bytes())
        .expect("the log parses")
        .into_iter()
        .filter_map(|event| match event.body {
            TopologyEventBody::TaskMerged { data } => Some(data.sequence.0),
            _ => None,
        })
        .collect()
}

#[test]
fn a_resume_completes_a_prepared_fast_transaction_through_the_barrier_and_cas() {
    let fixture = Fixture::healthy("finish-fast");
    let planted = plant_prepared_fast(&fixture);

    assert_eq!(
        ref_target(&fixture, fixture.started.integration_ref.as_str()).as_deref(),
        Some(fixture.base_sha.as_str()),
        "the fixture is the pre-CAS state: the ref is still at the base"
    );
    assert!(
        merged_sequences(&fixture).is_empty(),
        "and no task_merged was durable before the crash"
    );

    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);
    let (outcome, _) = resume_holding(&fixture, &harness, &given);
    outcome.expect("the resume completes the authorized publication rather than refusing");

    assert_eq!(
        ref_target(&fixture, fixture.started.integration_ref.as_str()).as_deref(),
        Some(planted.commit.as_str()),
        "the compare-and-swap moved the integration ref to the proposal"
    );
    assert_eq!(
        merged_sequences(&fixture),
        vec![0],
        "recovery appended exactly one task_merged, for the open transaction's sequence"
    );

    let fold = {
        let events = TopologyFold::parse_log(&fixture.log_bytes()).expect("parses");
        TopologyFold::replay(fixture.inputs(), &events).expect("replays")
    };
    assert!(
        fold.transaction().is_none(),
        "the transaction resolved: a completed publication leaves none open"
    );
    assert_eq!(
        fold.task_state(planted.candidate.key),
        Some(TaskState::Merged),
        "the candidate's task is merged"
    );
    let _ = planted.tree;
}

fn resume_with_real_refs(
    fixture: &Fixture,
    harness: &Arc<Mutex<HookHarness>>,
) -> Result<(Recovered, RunHandle), UpstrokeError> {
    let mut hooks = HarnessTopologyHooks::new(Arc::clone(harness)).recording_durability();
    resume_with_real_refs_hooked(fixture, &mut hooks)
}

fn resume_with_real_refs_hooked(
    fixture: &Fixture,
    hooks: &mut dyn TopologyHooks,
) -> Result<(Recovered, RunHandle), UpstrokeError> {
    resume_as(fixture, RESUMER, &runtime_holding_the_record(), hooks)
}

fn resume_as(
    fixture: &Fixture,
    incarnation: &str,
    runtime: &dyn ContainerRuntime,
    hooks: &mut dyn TopologyHooks,
) -> Result<(Recovered, RunHandle), UpstrokeError> {
    let liveness = FakeOwnerLiveness::new();
    let view = DisposableDirView::new(ContainerTrace::default());
    let incarnation = IncarnationId(incarnation.to_owned());
    let manager = fixture.manager();
    let mut warnings = Vec::new();
    let root = fixture.derive(None)?;
    run_recovery_order(
        root,
        &ResumeSeams {
            repo_root: &fixture.repo_root,
            worktree_git_dir: &fixture.git_dir,
            repo_key: &fixture.repo_key,
            incarnation: &incarnation,
            inputs: fixture.inputs(),
            today: &container_selection(),
            runtime,
            liveness: &liveness,
            view: &view,
            preflight: &AlwaysCertifies,
            refs: &manager,
            manager: &manager,
            clock: &Frozen,
        },
        hooks,
        &mut warnings,
    )
}

#[test]
fn a_resume_after_a_completed_publication_accepts_its_own_head() {
    let fixture = Fixture::healthy("published-head");
    let planted = plant_prepared_fast(&fixture);

    let first = resume_with_real_refs(&fixture, &harness())
        .expect("the first resume completes the authorized publication");
    assert_eq!(merged_sequences(&fixture), vec![0]);
    assert_eq!(
        ref_target(&fixture, fixture.started.integration_ref.as_str()).as_deref(),
        Some(planted.commit.as_str())
    );
    drop(first);

    let (_, handle) = resume_with_real_refs(&fixture, &harness())
        .expect("a second resume accepts the head its own publication put there");
    assert_eq!(
        merged_sequences(&fixture),
        vec![0],
        "the publication was recorded once and nothing was published again"
    );
    assert_eq!(
        ref_target(&fixture, fixture.started.integration_ref.as_str()).as_deref(),
        Some(planted.commit.as_str()),
        "the ref stays where the publication put it"
    );
    assert!(handle.fold.transaction().is_none());
    assert_eq!(handle.fold.task_state(ALPHA), Some(TaskState::Merged));
}

#[test]
fn a_resume_after_a_publication_refuses_a_ref_that_disagrees_with_the_log() {
    let fixture = Fixture::healthy("published-disagrees");
    let planted = plant_prepared_fast(&fixture);
    drop(
        resume_with_real_refs(&fixture, &harness())
            .expect("the first resume completes the authorized publication"),
    );

    let elsewhere = commit_on(
        &fixture,
        planted.commit.as_str(),
        "elsewhere.txt",
        "someone else\n",
        "upstroke: elsewhere",
    );
    crate::workspace_manager::fixture::git(
        &fixture.repo_root,
        &[
            "update-ref",
            fixture.started.integration_ref.as_str(),
            elsewhere.as_str(),
        ],
    );
    let moved = harness();
    let text = message(
        &resume_with_real_refs(&fixture, &moved)
            .expect_err("a ref that disagrees with the recorded publication refuses"),
    );
    assert!(
        text.contains(elsewhere.as_str()) && text.contains(planted.commit.as_str()),
        "the refusal names what it found and what the log published: {text}"
    );
    assert_eq!(
        ref_target(&fixture, fixture.started.integration_ref.as_str()).as_deref(),
        Some(elsewhere.as_str()),
        "the foreign ref is left exactly as it was"
    );
    assert_eq!(
        cas_integration_entries(&moved) + create_ref_entries(&moved),
        0,
        "nothing swapped or created a ref"
    );

    crate::workspace_manager::fixture::git(
        &fixture.repo_root,
        &["update-ref", "-d", fixture.started.integration_ref.as_str()],
    );
    let absent = harness();
    let text = message(
        &resume_with_real_refs(&fixture, &absent)
            .expect_err("an absent ref after a publication refuses rather than being recreated"),
    );
    assert!(
        text.contains(planted.commit.as_str()),
        "the refusal names the head the log published: {text}"
    );
    assert_eq!(
        ref_target(&fixture, fixture.started.integration_ref.as_str()),
        None,
        "the P7/P8 repair is for a run killed at run start, not for a published run"
    );
    assert_eq!(create_ref_entries(&absent), 0);
    assert_eq!(merged_sequences(&fixture), vec![0]);
}

struct FixedIds;

impl crate::engine::topology::seams::IdSource for FixedIds {
    fn run_id(&self) -> String {
        RUN_ID.to_owned()
    }

    fn incarnation(&self) -> crate::topology::events::IncarnationId {
        crate::topology::events::IncarnationId("inc-fixed".to_owned())
    }

    fn pid(&self) -> u32 {
        4242
    }

    fn question_id(&self) -> crate::ir::QuestionId {
        crate::ir::QuestionId("q-park-fixed".to_owned())
    }
}

fn durable_kinds(fixture: &Fixture) -> Vec<String> {
    TopologyFold::parse_log(&fixture.log_bytes())
        .expect("the log parses")
        .iter()
        .map(|event| event.body.kind().to_owned())
        .collect()
}

#[derive(Default)]
struct RecordingSleeper {
    slept: std::sync::Mutex<Vec<Duration>>,
}

impl crate::interaction::Sleeper for RecordingSleeper {
    fn sleep(&self, duration: Duration) {
        self.slept
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(duration);
    }
}

fn commit_on(
    fixture: &Fixture,
    parent: &str,
    file: &str,
    content: &str,
    message: &str,
) -> CommitSha {
    use crate::workspace_manager::fixture::{git, write_file};
    let repo = &fixture.repo_root;
    write_file(&repo.join(file), content.as_bytes());
    git(repo, &["add", "--", file]);
    let tree = git(repo, &["write-tree"]);
    let commit = git(repo, &["commit-tree", &tree, "-p", parent, "-m", message]);
    git(repo, &["rm", "-q", "-f", "--", file]);
    CommitSha(commit)
}

fn plant_staging_intent(fixture: &Fixture, sequence: u32) {
    let manager = fixture.manager();
    let mut hooks = HarnessTopologyHooks::new(harness()).recording_durability();
    manager
        .write_intent(
            hooks.effects(),
            &crate::engine::topology::integrate::staging_slot(crate::topology::events::SequenceId(
                sequence,
            )),
        )
        .expect("plant a staging intent");
}

fn plant_staging_worktree(fixture: &Fixture, sequence: u32, head: &str) -> PathBuf {
    plant_staging_intent(fixture, sequence);
    let manager = fixture.manager();
    let mut hooks = HarnessTopologyHooks::new(harness()).recording_durability();
    manager
        .add_worktree(
            hooks.effects(),
            &crate::engine::topology::integrate::staging_slot(crate::topology::events::SequenceId(
                sequence,
            )),
            head,
        )
        .expect("plant a staging worktree")
}

fn plant_snapshot(fixture: &Fixture, sequence: u64, commit: &str) -> PathBuf {
    let snapshot = fixture
        .manager()
        .add_snapshot(
            &mut crate::workspace_manager::NoHooks,
            &crate::workspace_manager::SnapshotName::integration(sequence),
            &crate::workspace_manager::SnapshotInput::Commit(
                crate::workspace_manager::ObjectId::new(commit.to_owned()).expect("an object id"),
            ),
        )
        .expect("plant a snapshot");
    snapshot.path().to_path_buf()
}

fn plant_stale_verification(
    fixture: &Fixture,
) -> (crate::topology::events::CandidateRef, CommitSha, GitRef) {
    use crate::workspace_manager::fixture::git;
    let head = plant_published_beta(fixture);
    let planted = plant_queued_candidate_events(fixture);

    let proposal = commit_on(
        fixture,
        head.as_str(),
        "candidate.txt",
        "the candidate edit\n",
        "upstroke: proposal s1",
    );
    let pin = crate::engine::topology::integrate::prepared_pin_ref(
        RUN_ID,
        crate::topology::events::SequenceId(1),
    );
    git(
        &fixture.repo_root,
        &["update-ref", pin.as_str(), proposal.as_str()],
    );
    plant_staging_worktree(fixture, 1, proposal.as_str());

    append_events(
        fixture,
        &[TopologyEventBody::MergeVerificationStarted {
            data: crate::topology::events::MergeVerificationStarted {
                sequence: crate::topology::events::SequenceId(1),
                candidate: planted.candidate.clone(),
                basis: crate::topology::events::VerificationBasis::StaleClean {
                    prepared_ref: pin.clone(),
                },
                expected_head: head.clone(),
                proposed_sha: proposal.clone(),
            },
        }],
    );
    (planted.candidate, head, pin)
}
fn interrupted_sequences(fixture: &Fixture) -> Vec<u32> {
    TopologyFold::parse_log(&fixture.log_bytes())
        .expect("the log parses")
        .into_iter()
        .filter_map(|event| match event.body {
            TopologyEventBody::MergeVerificationInterrupted { data } => Some(data.sequence.0),
            _ => None,
        })
        .collect()
}

fn append_event_hooked(
    fixture: &Fixture,
    body: TopologyEventBody,
    hooks: &mut HarnessTopologyHooks,
) -> Result<(), UpstrokeError> {
    let mut warnings = Vec::new();
    let mut log = EventLog::open_hooked(
        EventSite::OpenLog,
        &fixture.log(),
        &mut warnings,
        hooks.events(),
    )?;
    let site = crate::events::log::site_for(&body);
    let (line, _) = TopologyLine::round_trip(&event(body)).expect("a valid event");
    log.append_topology_hooked(site, &line, hooks.events())
}

fn proven_durable_len(hooks: &HarnessTopologyHooks, fixture: &Fixture) -> u64 {
    hooks
        .event_observer()
        .ledger()
        .records_for(&fixture.log())
        .into_iter()
        .filter(|record| {
            matches!(
                record.step,
                crate::util::DurableStep::SyncedData | crate::util::DurableStep::SyncedFile
            )
        })
        .map(|record| record.len)
        .next_back()
        .expect("a sync was recorded")
}

fn lose_unsynced_writes(fixture: &Fixture, durable: u64) {
    let bytes = fixture.log_bytes();
    let keep = usize::try_from(durable).expect("a small fixture log");
    assert!(
        keep <= bytes.len(),
        "the ledger proved more durable than exists"
    );
    crate::workspace_manager::fixture::write_file(&fixture.log(), &bytes[..keep]);
}

fn crash_with_unsynced_merge_prepared(
    fixture: &Fixture,
    planted: &PlantedTransaction,
) -> HarnessTopologyHooks {
    let durable_before = fixture.log_bytes().len();
    let harness = harness();
    harness
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .arm(
            EffectSiteId::Event(EventSite::Append),
            SubEffectPoint::WrittenFull,
            InjectionMode::ErrorReturn,
        )
        .expect("`Event.Append` exposes `WrittenFull` with an error contract");
    let mut hooks = HarnessTopologyHooks::new(harness).recording_durability();
    let error = append_event_hooked(fixture, fast_prepared(fixture, planted), &mut hooks)
        .expect_err("the append was made to fail after the full line was written");
    assert!(
        message(&error).contains("WrittenFull"),
        "the failure is the injected one: {}",
        message(&error)
    );
    let bytes = fixture.log_bytes();
    assert!(
        bytes.len() > durable_before
            && bytes.ends_with(b"\n")
            && String::from_utf8_lossy(&bytes).contains("merge_prepared"),
        "the complete merge_prepared line reached the file"
    );
    let last = hooks
        .event_observer()
        .ledger()
        .records_for(&fixture.log())
        .pop()
        .expect("the ledger recorded the append");
    assert_eq!(
        last.step,
        crate::util::DurableStep::Wrote,
        "the write was the last thing recorded: nothing flushed or synced it"
    );
    assert_eq!(
        proven_durable_len(&hooks, fixture),
        u64::try_from(durable_before).expect("a small fixture log"),
        "the ledger proves durable exactly the prefix before the unsynced line"
    );
    hooks
}

struct ReportingHooks {
    inner: HarnessTopologyHooks,
    effects: ReportingEffects,
    events: ReportingEvents,
}

struct ReportingEffects {
    inner: crate::workspace_manager::HarnessEffects,
    report: PathBuf,
}

struct ReportingEvents {
    inner: crate::events::log::HarnessEventHooks,
    report: PathBuf,
}

fn report(path: &Path, line: &str) {
    let mut content = std::fs::read_to_string(path).unwrap_or_default();
    content.push_str(line);
    content.push('\n');
    crate::workspace_manager::fixture::write_file(path, content.as_bytes());
}

impl ReportingHooks {
    fn new(harness: Arc<Mutex<HookHarness>>, report: &Path) -> Self {
        Self {
            inner: HarnessTopologyHooks::new(Arc::clone(&harness)),
            effects: ReportingEffects {
                inner: crate::workspace_manager::HarnessEffects::new(Arc::clone(&harness)),
                report: report.to_path_buf(),
            },
            events: ReportingEvents {
                inner: crate::events::log::HarnessEventHooks::new(harness),
                report: report.to_path_buf(),
            },
        }
    }
}

impl crate::workspace_manager::EffectHooks for ReportingEffects {
    fn phase(&mut self, site: EffectSiteId, phase: HookPhase) -> Injection {
        if site == EffectSiteId::Ref(RefSite::CompareAndSwapIntegration)
            && phase == HookPhase::Before
        {
            report(&self.report, "cas");
        }
        self.inner.phase(site, phase)
    }

    fn durability_ledger(&self) -> crate::util::DurabilityLedger {
        self.inner.durability_ledger()
    }

    fn refusal_cause(&self) -> Option<String> {
        self.inner.refusal_cause()
    }
}

impl crate::events::log::EventHooks for ReportingEvents {
    fn phase(&mut self, site: EventSite, phase: HookPhase) {
        self.inner.phase(site, phase);
    }

    fn point(&mut self, site: EventSite, point: SubEffectPoint, mode: InjectionMode) -> Injection {
        self.inner.point(site, point, mode)
    }

    fn written_kill_shape(&mut self, site: EventSite) -> crate::events::log::WrittenShape {
        self.inner.written_kill_shape(site)
    }

    fn durability_ledger(&self) -> crate::util::DurabilityLedger {
        self.inner.durability_ledger()
    }

    fn synced(&mut self, record: &crate::events::log::SyncRecord) {
        if record.target == crate::events::log::SyncTarget::LogFile {
            report(&self.report, &format!("synced {}", record.len));
        }
        self.inner.synced(record);
    }
}

impl TopologyHooks for ReportingHooks {
    fn effects(&mut self) -> &mut dyn crate::workspace_manager::EffectHooks {
        &mut self.effects
    }

    fn rundir(&mut self) -> &mut dyn crate::rundir::RunDirHooks {
        self.inner.rundir()
    }

    fn events(&mut self) -> &mut dyn crate::events::log::EventHooks {
        &mut self.events
    }

    fn container(&mut self) -> &mut dyn crate::runner::container::ContainerHooks {
        self.inner.container()
    }

    fn spawn(&mut self) -> &mut dyn crate::agent::proc::SpawnHooks {
        self.inner.spawn()
    }
}

#[test]
#[ignore = "spawned as a subprocess by the two-crash proof"]
fn two_crash_kill_child() {
    let repo_root = PathBuf::from(
        std::env::var("UPSTROKE_TEST_KILL_REPO").expect("the parent names the repository"),
    );
    let git_dir = PathBuf::from(
        std::env::var("UPSTROKE_TEST_KILL_GITDIR").expect("the parent names the git dir"),
    );
    let report_path = PathBuf::from(
        std::env::var("UPSTROKE_TEST_KILL_REPORT").expect("the parent names the report"),
    );
    let repo_key = RepoKey::v1(&std::fs::canonicalize(&git_dir).expect("the git dir exists"));

    let harness = harness();
    harness
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .arm(
            EffectSiteId::Event(EventSite::Append),
            SubEffectPoint::Written,
            InjectionMode::Kill,
        )
        .expect("the Written point supports a kill");
    let mut hooks = ReportingHooks::new(harness, &report_path);
    let runtime = runtime_holding_the_record();
    let liveness = FakeOwnerLiveness::new();
    let view = DisposableDirView::new(ContainerTrace::default());
    let certifies = AlwaysCertifies;
    let incarnation = IncarnationId(RESUMER.to_owned());
    let today = container_selection();
    let mut warnings = Vec::new();

    let root = RootDerived::derive_with(&repo_root, RUN_ID, None, TOPOLOGY_SCHEMA)
        .expect("(a0) derives in the child");
    let manager = crate::workspace_manager::WorkspaceManager::derive(
        &repo_root,
        root.private_root(),
        RUN_ID,
        RESUMER,
    )
    .expect("the child's repository and private root are real directories");
    let outcome = run_recovery_order(
        root,
        &ResumeSeams {
            repo_root: &repo_root,
            worktree_git_dir: &git_dir,
            repo_key: &repo_key,
            incarnation: &incarnation,
            inputs: FrozenInputs {
                plan: plan(),
                normalized_plan_digest: "sha256:aaaa".to_owned(),
            },
            today: &today,
            runtime: &runtime,
            liveness: &liveness,
            view: &view,
            preflight: &certifies,
            refs: &manager,
            manager: &manager,
            clock: &Frozen,
        },
        &mut hooks,
        &mut warnings,
    );
    let returned = match &outcome {
        Ok(_) => "recovery returned Ok past the armed kill".to_owned(),
        Err(error) => format!("recovery returned before the armed append: {error}"),
    };
    report(&report_path, &returned);
    panic!("the kill armed at the task_merged write must have taken this process; {returned}");
}

#[test]
fn unsynced_merge_prepared_two_crash_barrier_before_cas_then_power_loss_keeps_log_and_ref_agreeing()
{
    let fixture = Fixture::healthy("two-crash");
    let planted = plant_queued_candidate(&fixture);
    let first = crash_with_unsynced_merge_prepared(&fixture, &planted);
    let prefix_with_prepared = u64::try_from(fixture.log_bytes().len()).expect("a small log");
    drop(first);

    let report_path = fixture.root.join("two-crash-report");
    let status = crate::workspace_manager::fixture::run_kill_child(
        "engine::topology::recover::tests::two_crash_kill_child",
        &[
            ("UPSTROKE_TEST_KILL_REPO", fixture.repo_root.as_os_str()),
            ("UPSTROKE_TEST_KILL_GITDIR", fixture.git_dir.as_os_str()),
            ("UPSTROKE_TEST_KILL_REPORT", report_path.as_os_str()),
        ],
    );
    assert!(
        crate::workspace_manager::fixture::died_by_abort(&status),
        "the child must have died at the task_merged write, and it ended {status:?}; it \
         reported: {}",
        std::fs::read_to_string(&report_path).unwrap_or_default()
    );

    let reported = std::fs::read_to_string(&report_path).expect("the child reported");
    assert_eq!(
        reported.lines().collect::<Vec<_>>(),
        vec![format!("synced {prefix_with_prepared}").as_str(), "cas"],
        "the barrier's sync covered the merge_prepared line and preceded the swap"
    );
    assert_eq!(
        ref_target(&fixture, fixture.started.integration_ref.as_str()).as_deref(),
        Some(planted.commit.as_str()),
        "the swap moved the ref to the proposal"
    );
    let killed = fixture.log_bytes();
    assert!(
        u64::try_from(killed.len()).expect("a small log") > prefix_with_prepared
            && killed.ends_with(b"\n")
            && String::from_utf8_lossy(&killed).contains("task_merged"),
        "the complete task_merged line reached the file before the kill"
    );

    lose_unsynced_writes(&fixture, prefix_with_prepared);
    let surviving = String::from_utf8_lossy(&fixture.log_bytes()).into_owned();
    assert!(
        surviving.contains("merge_prepared") && !surviving.contains("task_merged"),
        "the log still contains merge_prepared and lost task_merged"
    );
    assert_eq!(
        ref_target(&fixture, fixture.started.integration_ref.as_str()).as_deref(),
        Some(planted.commit.as_str()),
        "the ref is at proposed_sha: the log and the ref agree on an authorized, completed swap"
    );

    let third = harness();
    let (_, handle) = resume_with_real_refs(&fixture, &third)
        .expect("the ref already at the proposal is recorded, not swapped again");
    assert_eq!(merged_sequences(&fixture), vec![0]);
    assert_eq!(cas_integration_entries(&third), 0);
    assert!(handle.fold.transaction().is_none());
    assert_eq!(handle.fold.task_state(ALPHA), Some(TaskState::Merged));
    drop(handle);

    let events = TopologyFold::parse_log(&fixture.log_bytes()).expect("parses");
    let once = TopologyFold::replay(fixture.inputs(), &events).expect("replays");
    let twice = TopologyFold::replay(fixture.inputs(), &events).expect("replays again");
    assert_eq!(once.state(), twice.state(), "replay twice equal");
}

#[test]
fn barrier_sync_failure_before_cas_issues_no_cas_and_converges_after_loss() {
    let fixture = Fixture::healthy("barrier-sync-fails");
    let planted = plant_queued_candidate(&fixture);
    let first = crash_with_unsynced_merge_prepared(&fixture, &planted);
    let durable = proven_durable_len(&first, &fixture);
    drop(first);
    let before = fixture.log_bytes();

    let harness = harness();
    harness
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .arm(
            EffectSiteId::Event(EventSite::OpenLog),
            SubEffectPoint::SyncPrefix,
            InjectionMode::ErrorReturn,
        )
        .expect("`Event.OpenLog` exposes `SyncPrefix` with an error contract");
    let mut hooks = HarnessTopologyHooks::new(Arc::clone(&harness)).recording_durability();
    let error = resume_with_real_refs_hooked(&fixture, &mut hooks)
        .expect_err("a barrier whose sync fails ends the command");
    let text = message(&error);
    assert!(
        text.contains("SyncPrefix"),
        "the refusal names the barrier step that failed: {text}"
    );
    assert_eq!(cas_integration_entries(&harness), 0, "no CAS was issued");
    assert_eq!(
        ref_target(&fixture, fixture.started.integration_ref.as_str()).as_deref(),
        Some(fixture.base_sha.as_str()),
        "the ref did not move"
    );
    assert_eq!(fixture.log_bytes(), before, "nothing was appended");
    assert!(
        hooks
            .event_observer()
            .ledger()
            .records_for(&fixture.log())
            .is_empty(),
        "the failed barrier synced nothing, so the unsynced line is still unsynced"
    );

    lose_unsynced_writes(&fixture, durable);
    assert!(
        !String::from_utf8_lossy(&fixture.log_bytes()).contains("merge_prepared"),
        "the unsynced line was lost"
    );
    let driven = drive(&fixture, &DriveSeams::default(), 1);
    assert!(
        matches!(
            driven.progress.first(),
            Some(Ok(Progress::Integrated {
                key: ALPHA,
                sequence: crate::topology::events::SequenceId(0),
                ..
            }))
        ),
        "the candidate was still queued and integrated under sequence 0: {:?}",
        driven.progress
    );
    assert_eq!(merged_sequences(&fixture), vec![0]);
    assert_eq!(
        ref_target(&fixture, fixture.started.integration_ref.as_str()).as_deref(),
        Some(planted.commit.as_str())
    );
}

#[test]
fn a_resume_of_a_prepared_transaction_whose_ref_moved_elsewhere_refuses_a_third_sha() {
    let fixture = Fixture::healthy("finish-third-sha");
    let planted = plant_prepared_fast(&fixture);
    let elsewhere = commit_on(
        &fixture,
        fixture.base_sha.as_str(),
        "elsewhere.txt",
        "someone else\n",
        "upstroke: elsewhere",
    );
    crate::workspace_manager::fixture::git(
        &fixture.repo_root,
        &[
            "update-ref",
            fixture.started.integration_ref.as_str(),
            elsewhere.as_str(),
        ],
    );

    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);
    let (outcome, _) = resume_holding(&fixture, &harness, &given);
    let text = message(&outcome.expect_err("a third sha under the integration ref refuses"));
    assert!(
        text.contains(elsewhere.as_str()) && text.contains(planted.commit.as_str()),
        "the refusal names what it found and what it would have published: {text}"
    );
    assert!(
        merged_sequences(&fixture).is_empty(),
        "a refusal before the compare-and-swap appends no task_merged"
    );
    assert_eq!(
        ref_target(&fixture, fixture.started.integration_ref.as_str()).as_deref(),
        Some(elsewhere.as_str()),
        "the third writer's ref is left exactly as it was"
    );
    let _ = planted.tree;
}

#[test]
fn a_resume_completes_a_prepared_transaction_whose_cas_already_ran_by_recording_the_merge() {
    let fixture = Fixture::healthy("finish-cas-done");
    let planted = plant_prepared_fast(&fixture);
    crate::workspace_manager::fixture::git(
        &fixture.repo_root,
        &[
            "update-ref",
            fixture.started.integration_ref.as_str(),
            planted.commit.as_str(),
        ],
    );

    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);
    let mark_before = create_ref_entries(&harness);
    let (outcome, _) = resume_holding(&fixture, &harness, &given);
    outcome.expect("the resume records the merge rather than refusing a no-op swap");
    let _ = mark_before;

    assert_eq!(
        merged_sequences(&fixture),
        vec![0],
        "recovery appended the task_merged the crash withheld"
    );
    assert_eq!(
        ref_target(&fixture, fixture.started.integration_ref.as_str()).as_deref(),
        Some(planted.commit.as_str()),
        "the ref stayed at the proposal it already named"
    );
    assert!(
        cas_integration_entries(&harness) == 0,
        "a ref already at the proposal is recorded, never swapped a second time"
    );
}

#[test]
fn a_resume_settles_an_interrupted_stale_verification_and_reclaims_its_residue() {
    let fixture = Fixture::two_tasks("finish-interrupt");
    let (_candidate, head, pin) = plant_stale_verification(&fixture);

    assert!(
        ref_target(&fixture, pin.as_str()).is_some(),
        "the fixture pinned the proposal under prepared/1"
    );
    assert!(
        fixture.manager().intents().expect("intents").contains(
            &crate::engine::topology::integrate::staging_slot(crate::topology::events::SequenceId(
                1
            ))
        ),
        "the fixture left a staging intent"
    );

    resume_with_real_refs(&fixture, &harness())
        .expect("the resume settles the interrupted verification rather than refusing");

    assert_eq!(
        interrupted_sequences(&fixture),
        vec![1],
        "recovery appended exactly one merge_verification_interrupted, for the open transaction"
    );
    assert_eq!(
        merged_sequences(&fixture),
        vec![0],
        "an interrupted verification is not a publication: only BETA's sequence 0 merged"
    );
    assert_eq!(
        ref_target(&fixture, fixture.started.integration_ref.as_str()).as_deref(),
        Some(head.as_str()),
        "the integration ref did not move: an interrupt authorizes no compare-and-swap"
    );
    assert_eq!(
        ref_target(&fixture, pin.as_str()),
        None,
        "the proposal pin was pruned"
    );
    let staging =
        crate::engine::topology::integrate::staging_slot(crate::topology::events::SequenceId(1));
    assert!(
        !fixture
            .manager()
            .intents()
            .expect("intents")
            .contains(&staging),
        "the staging intent was reclaimed"
    );
    assert!(
        !fixture.manager().slot_path(&staging).exists(),
        "the staging worktree itself was removed, not only its intent"
    );

    let fold = {
        let events = TopologyFold::parse_log(&fixture.log_bytes()).expect("parses");
        TopologyFold::replay(fixture.inputs(), &events).expect("replays")
    };
    assert!(
        fold.transaction().is_none(),
        "the interrupt released the transaction so the candidate can re-verify under a new sequence"
    );

    let driven = drive(&fixture, &DriveSeams::default(), 1);
    assert!(
        matches!(
            driven.progress.first(),
            Some(Ok(Progress::Integrated {
                key: ALPHA,
                sequence: crate::topology::events::SequenceId(2),
                ..
            }))
        ),
        "the candidate re-verified and published under the next sequence: {:?}",
        driven.progress
    );
    assert_eq!(merged_sequences(&fixture), vec![0, 2]);
    assert!(
        driven.invocations_balance && driven.entitlements_held == 0,
        "the re-verification's invocations settled and its holdings were released"
    );
}

#[test]
fn a_resume_reclaims_an_interrupted_verifications_snapshots_after_settling_it() {
    let fixture = Fixture::two_tasks("interrupted-snapshot");
    let (_candidate, _head, pin) = plant_stale_verification(&fixture);
    let proposed = ref_target(&fixture, pin.as_str()).expect("the recorded proposal");
    let snapshot = plant_snapshot(&fixture, 1, &proposed);
    let slot = crate::workspace_manager::Slot::Snapshot {
        name: crate::workspace_manager::SnapshotName::integration(1),
    };
    assert!(
        fixture
            .manager()
            .intents()
            .expect("intents")
            .contains(&slot)
            && snapshot.exists(),
        "the fixture left the snapshot and its intent"
    );

    let harness = harness();
    resume_with_real_refs(&fixture, &harness)
        .expect("the resume settles the interrupted verification");
    assert_eq!(interrupted_sequences(&fixture), vec![1]);
    assert!(
        !fixture
            .manager()
            .intents()
            .expect("intents")
            .contains(&slot)
            && !snapshot.exists(),
        "the interrupted verification's snapshot and its intent were reclaimed"
    );

    let seen = harness.lock().unwrap_or_else(PoisonError::into_inner);
    let position = |site: EffectSiteId, phase: HookPhase| {
        seen.coverage()
            .iter()
            .position(|observation| observation.site == site && observation.phase == phase)
            .unwrap_or_else(|| panic!("`{site}` `{phase}` was never observed"))
    };
    let terminal = position(EffectSiteId::Event(EventSite::Append), HookPhase::After);
    let removed = position(
        EffectSiteId::Snapshot(crate::topology::effects::SnapshotSite::Remove),
        HookPhase::Before,
    );
    assert!(
        terminal < removed,
        "the snapshot was removed (at {removed}) before the interrupted terminal (at {terminal})"
    );
}

#[test]
fn a_resume_reclaims_the_orphan_pin_at_the_next_sequence_and_orphan_staging() {
    let fixture = Fixture::healthy("finish-orphan");
    let orphan_commit = commit_on(
        &fixture,
        fixture.base_sha.as_str(),
        "orphan.txt",
        "orphan\n",
        "upstroke: orphan",
    );
    let orphan_pin = crate::engine::topology::integrate::prepared_pin_ref(
        RUN_ID,
        crate::topology::events::SequenceId(0),
    );
    crate::workspace_manager::fixture::git(
        &fixture.repo_root,
        &["update-ref", orphan_pin.as_str(), orphan_commit.as_str()],
    );
    let staging = plant_staging_worktree(&fixture, 0, orphan_commit.as_str());
    plant_staging_intent(&fixture, 7);
    let snapshot = plant_snapshot(&fixture, 0, orphan_commit.as_str());
    assert!(
        ref_target(&fixture, orphan_pin.as_str()).is_some()
            && staging.exists()
            && snapshot.exists(),
        "the fixture left the orphan pin, the staging worktree and the snapshot"
    );

    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);
    let (outcome, _) = resume_holding(&fixture, &harness, &given);
    outcome.expect("the resume reclaims the residue rather than refusing on an unexpected ref");

    assert_eq!(
        ref_target(&fixture, orphan_pin.as_str()),
        None,
        "the orphan pin at the next sequence was deleted so the sequence can take the name"
    );
    assert!(
        fixture.manager().intents().expect("intents").is_empty()
            && !staging.exists()
            && !snapshot.exists(),
        "the staging and snapshot residue was reclaimed with force"
    );
    assert!(
        crate::workspace_manager::fixture::git_out(
            &fixture.repo_root,
            &["cat-file", "-e", orphan_commit.as_str()]
        )
        .status
        .success(),
        "the proposal object is Git's once unreferenced, never deleted by recovery"
    );
    assert!(
        merged_sequences(&fixture).is_empty() && interrupted_sequences(&fixture).is_empty(),
        "no transaction was open, so no terminal was appended"
    );
}

#[test]
fn a_resume_refuses_a_prepared_pin_outside_the_sequences_the_log_pinned() {
    let fixture = Fixture::healthy("orphan-outside");
    let pin = crate::engine::topology::integrate::prepared_pin_ref(
        RUN_ID,
        crate::topology::events::SequenceId(3),
    );
    let (object, _) = alpha_commit(&fixture);
    crate::workspace_manager::fixture::git(
        &fixture.repo_root,
        &["update-ref", pin.as_str(), object.as_str()],
    );
    let before = fixture.log_bytes();

    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);
    let (outcome, _) = resume_holding(&fixture, &harness, &given);
    let text = message(&outcome.expect_err("prepared/3 is not the next sequence, 0"));
    assert!(
        text.contains(pin.as_str()),
        "the refusal names the unexpected ref: {text}"
    );
    assert_eq!(
        ref_target(&fixture, pin.as_str()).as_deref(),
        Some(object.as_str()),
        "a ref recovery refuses is never deleted"
    );
    assert_eq!(fixture.log_bytes(), before, "refused before any append");
}

#[test]
fn a_resume_refuses_a_substituted_verification_pin_before_settling_it() {
    let fixture = Fixture::two_tasks("substituted-pin");
    let (_candidate, _head, pin) = plant_stale_verification(&fixture);
    crate::workspace_manager::fixture::git(
        &fixture.repo_root,
        &["update-ref", pin.as_str(), fixture.base_sha.as_str()],
    );
    let before = fixture.log_bytes();

    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);
    let (outcome, _) = resume_holding(&fixture, &harness, &given);
    let text = message(&outcome.expect_err("a pin that differs from its record refuses"));
    assert!(
        text.contains(fixture.base_sha.as_str()) && text.contains(pin.as_str()),
        "the refusal names the pin and what it found: {text}"
    );
    assert_eq!(
        ref_target(&fixture, pin.as_str()).as_deref(),
        Some(fixture.base_sha.as_str()),
        "the substituted pin is neither adopted nor deleted"
    );
    assert!(
        interrupted_sequences(&fixture).is_empty() && fixture.log_bytes() == before,
        "refused before the interrupted terminal, before any append"
    );
    assert!(
        fixture.manager().intents().expect("intents").contains(
            &crate::engine::topology::integrate::staging_slot(crate::topology::events::SequenceId(
                1
            ))
        ),
        "the open transaction's staging is resumably open and untouched"
    );
}

fn stale_clean_prepared(
    candidate: &crate::topology::events::CandidateRef,
    head: &CommitSha,
    proposal: &CommitSha,
    pin: &GitRef,
) -> TopologyEventBody {
    TopologyEventBody::MergePrepared {
        data: Box::new(crate::topology::events::MergePrepared {
            sequence: crate::topology::events::SequenceId(1),
            disposition: crate::topology::events::PreparedDisposition::StaleClean,
            expected_head: head.clone(),
            proposed_sha: proposal.clone(),
            key: candidate.key,
            generation: candidate.generation,
            candidate_sha: candidate.commit_sha.clone(),
            candidate_ref: candidate.candidate_ref.clone(),
            prepared_ref: Some(pin.clone()),
            verification_source: crate::topology::events::VerificationSource::Verification {
                sequence: crate::topology::events::SequenceId(1),
            },
            verification: Some(passed_verification()),
            satisfies: vec![ALPHA],
        }),
    }
}

fn passed_verification() -> crate::topology::events::VerificationRecord {
    crate::topology::events::VerificationRecord {
        verdict: crate::topology::events::VerificationVerdict::Passed,
        gates_passed: true,
        reviews: Vec::new(),
        detail: "the integration verification passed".to_owned(),
    }
}

#[test]
fn a_resume_keeps_a_prepared_transactions_pin_when_publication_refuses() {
    let fixture = Fixture::two_tasks("prepared-pin-kept");
    let (candidate, head, pin) = plant_stale_verification(&fixture);
    let proposal = CommitSha(ref_target(&fixture, pin.as_str()).expect("the pinned proposal"));
    append_events(
        &fixture,
        &[stale_clean_prepared(&candidate, &head, &proposal, &pin)],
    );
    let staging =
        crate::engine::topology::integrate::staging_slot(crate::topology::events::SequenceId(1));

    {
        let events = TopologyFold::parse_log(&fixture.log_bytes()).expect("parses");
        let fold = TopologyFold::replay(fixture.inputs(), &events).expect("replays");
        let recovered = crate::engine::topology::integrate::Authorized::from_fold(&fold)
            .expect("read")
            .expect("merge_prepared authorized a publication");
        assert_eq!(
            recovered,
            crate::engine::topology::integrate::Authorized {
                sequence: crate::topology::events::SequenceId(1),
                key: ALPHA,
                expected_head: head.clone(),
                proposed_sha: proposal.clone(),
                satisfies: vec![ALPHA],
                lease_release: crate::topology::events::MergeLeaseRelease::Candidate {
                    key: ALPHA,
                    generation: GEN,
                },
                integration_ref: fixture.started.integration_ref.clone(),
                pin: Some(pin.clone()),
                staging: Some(staging.clone()),
            }
        );
    }

    crate::workspace_manager::fixture::git(
        &fixture.repo_root,
        &[
            "update-ref",
            fixture.started.integration_ref.as_str(),
            fixture.base_sha.as_str(),
        ],
    );
    let harness = harness();
    let runtime = runtime_holding_the_record();
    let certifies = AlwaysCertifies;
    let given = Given::healthy(&fixture, &runtime, &certifies);
    let (outcome, _) = resume_holding(&fixture, &harness, &given);
    let text = message(&outcome.expect_err("a third SHA under the integration ref refuses"));
    assert!(text.contains("third SHA"), "{text}");
    assert_eq!(
        ref_target(&fixture, pin.as_str()).as_deref(),
        Some(proposal.as_str()),
        "the still-authorized proposal kept its pin"
    );
    assert!(
        fixture
            .manager()
            .intents()
            .expect("intents")
            .contains(&staging)
            && fixture.manager().slot_path(&staging).exists(),
        "the still-open transaction kept its staging worktree"
    );
    assert_eq!(merged_sequences(&fixture), vec![0]);
}

#[test]
fn a_resume_prunes_a_resolved_sequences_pin_at_its_recorded_proposal_and_refuses_it_elsewhere() {
    for substituted in [false, true] {
        let fixture = Fixture::two_tasks(if substituted {
            "resolved-pin-substituted"
        } else {
            "resolved-pin-pruned"
        });
        let (candidate, head, pin) = plant_stale_verification(&fixture);
        let proposal = CommitSha(ref_target(&fixture, pin.as_str()).expect("the pinned proposal"));
        append_events(
            &fixture,
            &[
                stale_clean_prepared(&candidate, &head, &proposal, &pin),
                TopologyEventBody::TaskMerged {
                    data: crate::topology::events::TaskMerged {
                        sequence: crate::topology::events::SequenceId(1),
                        merged_sha: proposal.clone(),
                        satisfies: vec![ALPHA],
                        lease_release: crate::topology::events::MergeLeaseRelease::Candidate {
                            key: ALPHA,
                            generation: GEN,
                        },
                    },
                },
            ],
        );
        crate::workspace_manager::fixture::git(
            &fixture.repo_root,
            &[
                "update-ref",
                fixture.started.integration_ref.as_str(),
                proposal.as_str(),
            ],
        );
        if substituted {
            crate::workspace_manager::fixture::git(
                &fixture.repo_root,
                &["update-ref", pin.as_str(), fixture.base_sha.as_str()],
            );
        }

        let outcome = resume_with_real_refs(&fixture, &harness());
        if substituted {
            let text = message(&outcome.expect_err("a resolved pin at another SHA refuses"));
            assert!(
                text.contains(pin.as_str()) && text.contains(fixture.base_sha.as_str()),
                "{text}"
            );
            assert_eq!(
                ref_target(&fixture, pin.as_str()).as_deref(),
                Some(fixture.base_sha.as_str()),
                "the substitution stays visible rather than being deleted"
            );
        } else {
            outcome.expect("a resolved sequence's pin at its recorded proposal is pruned");
            assert_eq!(
                ref_target(&fixture, pin.as_str()),
                None,
                "the pin of a published sequence was pruned"
            );
        }
        assert_eq!(merged_sequences(&fixture), vec![0, 1]);
    }
}

#[test]
fn a_resume_completes_an_already_present_publication_at_the_candidate_commit_and_reclaims_its_staging()
 {
    let fixture = Fixture::healthy("already-present-at-candidate");
    let planted = plant_queued_candidate(&fixture);
    crate::workspace_manager::fixture::git(
        &fixture.repo_root,
        &[
            "update-ref",
            fixture.started.integration_ref.as_str(),
            planted.commit.as_str(),
        ],
    );
    let staging = plant_staging_worktree(&fixture, 0, planted.commit.as_str());
    append_events(
        &fixture,
        &[
            TopologyEventBody::MergeVerificationStarted {
                data: crate::topology::events::MergeVerificationStarted {
                    sequence: crate::topology::events::SequenceId(0),
                    candidate: planted.candidate.clone(),
                    basis: crate::topology::events::VerificationBasis::AlreadyPresent,
                    expected_head: planted.commit.clone(),
                    proposed_sha: planted.commit.clone(),
                },
            },
            TopologyEventBody::MergePrepared {
                data: Box::new(crate::topology::events::MergePrepared {
                    sequence: crate::topology::events::SequenceId(0),
                    disposition: crate::topology::events::PreparedDisposition::AlreadyPresent,
                    expected_head: planted.commit.clone(),
                    proposed_sha: planted.commit.clone(),
                    key: ALPHA,
                    generation: GEN,
                    candidate_sha: planted.commit.clone(),
                    candidate_ref: planted.candidate.candidate_ref.clone(),
                    prepared_ref: None,
                    verification_source:
                        crate::topology::events::VerificationSource::Verification {
                            sequence: crate::topology::events::SequenceId(0),
                        },
                    verification: Some(passed_verification()),
                    satisfies: vec![ALPHA],
                }),
            },
        ],
    );

    let harness = harness();
    resume_with_real_refs(&fixture, &harness).expect("record the already-present publication");
    assert_eq!(merged_sequences(&fixture), vec![0]);
    assert_eq!(
        ref_target(&fixture, fixture.started.integration_ref.as_str()).as_deref(),
        Some(planted.commit.as_str()),
        "the ref did not move"
    );
    assert_eq!(
        cas_integration_entries(&harness),
        1,
        "the validation-only swap ran once, expected-old at the head"
    );
    assert!(
        !fixture
            .manager()
            .intents()
            .expect("intents")
            .iter()
            .any(|slot| matches!(slot, crate::workspace_manager::Slot::Staging { .. }))
            && !staging.exists(),
        "recovery read the disposition rather than inferring fast, and reclaimed the staging"
    );
}

#[derive(Default)]
struct DriveSeams {
    gate_fails: Option<crate::error::ProcessFate>,
    review_fails: Option<crate::error::ProcessFate>,
    gate_times_out: bool,
    gate_exit_code: Option<i32>,
    input_rejected: bool,
    input_git_error: bool,
    review_needs_human: bool,
    review_cost_usd: Option<f64>,
    run_ceiling_usd: Option<f64>,
    answer: Option<crate::ir::Answer>,
    /// `RunSeams::halts_run`: a declined question halts the run.
    halts_run: bool,
    /// Answers come from the run directory's `answers/` through the production
    /// `EventLogAnswers` with a zero wait, not from `answer`.
    answers_from_run_dir: bool,
}

struct Driven {
    progress: Vec<Result<crate::engine::topology::run::Progress, UpstrokeError>>,
    implementers: Vec<PassBinding>,
    reviewer_models: Vec<String>,
    gate_heads: Vec<(crate::runner::InvocationId, Option<String>)>,
    runs: Vec<DrivenRun>,
    spend_before: f64,
    spend_after: f64,
    invocations_balance: bool,
    entitlements_held: u32,
    reservations_cancelled: u32,
    transaction_open: bool,
    pipeline_held: usize,
    log: Vec<TopologyEvent>,
}

struct DrivenPlans<'a> {
    frozen: crate::engine::assembly::FrozenPlans<'a>,
    implementers: std::cell::RefCell<Vec<PassBinding>>,
}

impl crate::engine::topology::attempt::AttemptPlans for DrivenPlans<'_> {
    fn inputs(
        &self,
        request: &crate::engine::topology::attempt::InputsRequest<'_>,
    ) -> Result<crate::engine::topology::attempt::ReviewInputs, UpstrokeError> {
        self.frozen.inputs(request)
    }

    fn pool_for(&self, agent: &str) -> Option<String> {
        self.frozen.pool_for(agent)
    }

    fn plan(
        &self,
        request: &crate::engine::topology::attempt::PlanRequest<'_>,
    ) -> Result<crate::engine::topology::attempt::AttemptPlan, UpstrokeError> {
        self.frozen.plan(request)
    }

    fn verification(
        &self,
        request: &crate::engine::topology::attempt::VerificationRequest<'_>,
    ) -> Result<crate::engine::topology::attempt::VerificationPlan, UpstrokeError> {
        self.implementers
            .borrow_mut()
            .push(request.implementer.clone());
        self.frozen.verification(request)
    }
}

struct DrivenReviews {
    needs_human: bool,
    cost_usd: Option<f64>,
    models: Mutex<Vec<String>>,
}

impl crate::engine::topology::attempt::ReviewPasses for DrivenReviews {
    fn run(
        &self,
        cx: &crate::review::ReviewCx<'_>,
        runner: &dyn Runner,
        invocations: &crate::review::ReviewInvocations,
    ) -> Result<crate::review::ReviewOutcome, UpstrokeError> {
        self.models
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(cx.profile.model.clone());
        let request = crate::runner::review_request(
            CommandSpec::new(cx.adapter.id()).arg("--review"),
            cx.workspace.to_path_buf(),
            crate::runner::AgentId::new(cx.adapter.id()),
            cx.timeout,
            invocations.pass.clone(),
        );
        if let Err(error) = runner.run(&request) {
            if error.fate.is_unresolved() {
                return Err(error.into());
            }
            let never_started = matches!(error.fate, crate::error::ProcessFate::NeverStarted);
            return Ok(crate::review::ReviewOutcome {
                result: crate::review::ReviewResult::Unavailable {
                    status: crate::ir::OutcomeStatus::AgentError,
                    detail: format!("review process failed: {error}"),
                },
                cost_usd: None,
                invocations: 0,
                transcript: PathBuf::from("driven-review"),
                never_started,
            });
        }
        Ok(crate::review::ReviewOutcome {
            result: crate::review::ReviewResult::Judged(crate::ir::Verdict {
                pass: !self.needs_human,
                reasons: if self.needs_human {
                    vec!["a person must decide this integration".to_owned()]
                } else {
                    Vec::new()
                },
                required_changes: Vec::new(),
                needs_human: self.needs_human,
            }),
            cost_usd: self.cost_usd,
            invocations: 1,
            transcript: PathBuf::from("driven-review"),
            never_started: false,
        })
    }
}

#[derive(Debug, Clone)]
struct DrivenRun {
    invocation: crate::runner::InvocationId,
    role: crate::runner::ExecutionRole,
    workspace: PathBuf,
    head: Option<String>,
    checkout: BTreeMap<String, String>,
}

fn checkout_of(workspace: &Path) -> BTreeMap<String, String> {
    let listed = crate::workspace_manager::fixture::git_out(workspace, &["ls-files"]);
    if !listed.status.success() {
        return BTreeMap::new();
    }
    String::from_utf8_lossy(&listed.stdout)
        .lines()
        .filter_map(|name| {
            std::fs::read_to_string(workspace.join(name))
                .ok()
                .map(|content| (name.to_owned(), content))
        })
        .collect()
}

struct DrivenRunner {
    fails: Option<crate::error::ProcessFate>,
    review_fails: Option<crate::error::ProcessFate>,
    times_out: bool,
    exit_code: i32,
    runs: Mutex<Vec<DrivenRun>>,
}

impl DrivenRunner {
    fn runs(&self) -> Vec<DrivenRun> {
        self.runs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Runner for DrivenRunner {
    fn run(&self, request: &RunnerRequest) -> Result<ProcessOutput, RunnerError> {
        let head = {
            let output = crate::workspace_manager::fixture::git_out(
                &request.workspace,
                &["rev-parse", "--verify", "--quiet", "HEAD"],
            );
            output
                .status
                .success()
                .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        };
        self.runs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(DrivenRun {
                invocation: request.invocation.clone(),
                role: request.role.clone(),
                workspace: request.workspace.clone(),
                head,
                checkout: checkout_of(&request.workspace),
            });
        let fate = self.fails.or_else(|| {
            (request.role == crate::runner::ExecutionRole::Review)
                .then_some(self.review_fails)
                .flatten()
        });
        if let Some(fate) = fate {
            return Err(RunnerError::new(
                &request.invocation,
                fate,
                UpstrokeError::Io {
                    path: PathBuf::from("missing-gate-executable"),
                    source: std::io::Error::from(std::io::ErrorKind::NotFound),
                },
            ));
        }
        Ok(ProcessOutput {
            code: if self.times_out {
                None
            } else {
                Some(self.exit_code)
            },
            stdout: String::new(),
            stderr: String::new(),
            duration: Duration::from_millis(1),
            timed_out: self.times_out,
            output_limited: false,
        })
    }
}

struct DrivenPolicy {
    reject: bool,
    git_error: bool,
}

impl crate::engine::topology::attempt::ReviewInputPolicy for DrivenPolicy {
    fn problem(&self, worktree: &Path, tree: &str) -> Result<Option<String>, UpstrokeError> {
        if self.git_error {
            crate::workspace_manager::fixture::write_file(
                &worktree_git_dir(worktree).join("index"),
                b"a corrupt index",
            );
            return crate::workspace::Workspace::open(worktree)?
                .review_input_problem_for_tree(tree);
        }
        Ok(self
            .reject
            .then(|| "the proposed tree has opaque review inputs".to_owned()))
    }
}

struct DrivenAnswers {
    answer: Option<crate::ir::Answer>,
}

impl crate::interaction::AnswerSource for DrivenAnswers {
    fn id(&self) -> &'static str {
        "driven"
    }

    fn resolve(&self, _question: &crate::ir::Question) -> Result<crate::ir::Answer, UpstrokeError> {
        Ok(self.answer.clone().unwrap_or(crate::ir::Answer::Unanswered))
    }
    fn poll(&self, question: &crate::ir::Question) -> Result<crate::ir::Answer, UpstrokeError> {
        self.resolve(question)
    }
}

fn drive(fixture: &Fixture, seams: &DriveSeams, steps: usize) -> Driven {
    let mut hooks = HarnessTopologyHooks::new(harness());
    drive_hooked(fixture, seams, steps, &mut hooks)
}

fn drive_hooked(
    fixture: &Fixture,
    seams: &DriveSeams,
    steps: usize,
    hooks: &mut dyn TopologyHooks,
) -> Driven {
    let runner = driven_runner(seams);
    let mut driven = drive_with(fixture, seams, steps, &runner, hooks);
    let runs = runner.runs();
    driven.gate_heads = runs
        .iter()
        .filter(|run| run.role == crate::runner::ExecutionRole::Gate)
        .map(|run| (run.invocation.clone(), run.head.clone()))
        .collect();
    driven.runs = runs;
    driven
}

struct SnapshotRemovalObserver {
    log: PathBuf,
    last_event_at_removal: Vec<String>,
}

impl crate::workspace_manager::EffectHooks for SnapshotRemovalObserver {
    fn phase(&mut self, site: EffectSiteId, phase: HookPhase) -> Injection {
        if site == EffectSiteId::Snapshot(crate::topology::effects::SnapshotSite::Remove)
            && phase == HookPhase::Before
        {
            let last = std::fs::read(&self.log)
                .ok()
                .and_then(|bytes| TopologyFold::parse_log(&bytes).ok())
                .and_then(|events| events.last().map(|event| event.body.kind().to_owned()))
                .unwrap_or_default();
            self.last_event_at_removal.push(last);
        }
        Injection::Proceed
    }

    fn refusal_cause(&self) -> Option<String> {
        None
    }
}

struct SnapshotOrderHooks {
    rest: HarnessTopologyHooks,
    effects: SnapshotRemovalObserver,
}

impl TopologyHooks for SnapshotOrderHooks {
    fn effects(&mut self) -> &mut dyn crate::workspace_manager::EffectHooks {
        &mut self.effects
    }

    fn rundir(&mut self) -> &mut dyn rundir::RunDirHooks {
        self.rest.rundir()
    }

    fn events(&mut self) -> &mut dyn crate::events::log::EventHooks {
        self.rest.events()
    }

    fn container(&mut self) -> &mut dyn crate::runner::container::ContainerHooks {
        self.rest.container()
    }

    fn spawn(&mut self) -> &mut dyn crate::agent::proc::SpawnHooks {
        self.rest.spawn()
    }
}

fn drive_with(
    fixture: &Fixture,
    seams: &DriveSeams,
    steps: usize,
    runner: &dyn Runner,
    hooks: &mut dyn TopologyHooks,
) -> Driven {
    drive_as(
        fixture,
        RESUMER,
        &runtime_holding_the_record(),
        seams,
        steps,
        runner,
        hooks,
    )
}

fn driven_runner(seams: &DriveSeams) -> DrivenRunner {
    DrivenRunner {
        fails: seams.gate_fails,
        review_fails: seams.review_fails,
        times_out: seams.gate_times_out,
        exit_code: seams.gate_exit_code.unwrap_or(0),
        runs: Mutex::new(Vec::new()),
    }
}

fn drive_as(
    fixture: &Fixture,
    incarnation: &str,
    resume_runtime: &dyn ContainerRuntime,
    seams: &DriveSeams,
    steps: usize,
    runner: &dyn Runner,
    hooks: &mut dyn TopologyHooks,
) -> Driven {
    let (_, handle) = resume_as(fixture, incarnation, resume_runtime, hooks)
        .expect("the resume settles the planted state");
    drive_handle(fixture, handle, seams, steps, runner, hooks)
}

/// The steps of [`drive_as`] on a handle the caller already resumed, so a
/// test can read the `Recovered` the resume produced before the loop moves.
fn drive_handle(
    fixture: &Fixture,
    handle: RunHandle,
    seams: &DriveSeams,
    steps: usize,
    runner: &dyn Runner,
    hooks: &mut dyn TopologyHooks,
) -> Driven {
    use crate::engine::topology::run::{RunSeams, TopologyRun};

    let mut run = TopologyRun::resumed(
        handle,
        fixture.inputs(),
        crate::engine::topology::select::Ceiling {
            run_usd: seams.run_ceiling_usd,
            task_usd: None,
        },
    );
    let sleeper = RecordingSleeper::default();
    let manager = fixture.manager();
    let input_policy = DrivenPolicy {
        reject: seams.input_rejected,
        git_error: seams.input_git_error,
    };
    let adapters = crate::engine::topology::scaffold::ScaffoldAdapters::new();
    let paths = crate::rundir::RunPaths::with_private_root(
        &fixture.repo_root,
        &fixture.started.run_id,
        &fixture.private_root,
    );
    paths.create().expect("the run directories are creatable");
    let gates: Vec<crate::gates::ShellGate> = fixture
        .started
        .gate_cmds
        .iter()
        .map(crate::gates::ShellGate::from_record)
        .collect();
    let plans = DrivenPlans {
        frozen: crate::engine::assembly::FrozenPlans {
            adapters: &adapters,
            paths: &paths,
            gates: &gates,
            pools: &[],
            caps: &[],
            worker_timeout: Duration::from_secs(300),
            decisions: &[],
        },
        implementers: std::cell::RefCell::new(Vec::new()),
    };
    let reviews = DrivenReviews {
        needs_human: seams.review_needs_human,
        cost_usd: seams.review_cost_usd,
        models: Mutex::new(Vec::new()),
    };
    let driven_answers = DrivenAnswers {
        answer: seams.answer.clone(),
    };
    let run_dir_answers = crate::interaction::EventLogAnswers::with_poll(
        paths.answers(),
        Duration::ZERO,
        Duration::from_millis(1),
        &sleeper,
    );
    let answers: &dyn crate::interaction::AnswerSource = if seams.answers_from_run_dir {
        &run_dir_answers
    } else {
        &driven_answers
    };
    let run_seams = RunSeams {
        manager: &manager,
        clock: &Frozen,
        sleeper: &sleeper,
        runner,
        adapters: &adapters,
        paths: &paths,
        plans: &plans,
        reviews: &reviews,
        input_policy: &input_policy,
        answers,
        ids: &FixedIds,
        halts_run: seams.halts_run,
    };
    let spend_before = run.spend().run_total();
    let progress = (0..steps)
        .map(|_| run.step(&run_seams, &mut *hooks))
        .collect();
    Driven {
        progress,
        implementers: plans.implementers.into_inner(),
        reviewer_models: reviews
            .models
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner),
        gate_heads: Vec::new(),
        runs: Vec::new(),
        spend_before,
        spend_after: run.spend().run_total(),
        invocations_balance: run.invocations_balance(),
        entitlements_held: run.entitlements_held(),
        reservations_cancelled: run.reservations_cancelled(),
        transaction_open: run.fold().transaction().is_some(),
        pipeline_held: run.fold().pipeline_held(),
        log: TopologyFold::parse_log(&fixture.log_bytes()).expect("the result log parses"),
    }
}

fn unavailable_terminals(
    log: &[TopologyEvent],
) -> Vec<crate::topology::events::MergeVerificationUnavailable> {
    log.iter()
        .filter_map(|event| match &event.body {
            TopologyEventBody::MergeVerificationUnavailable { data } => Some(data.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn an_integration_review_is_selected_against_the_candidates_recorded_implementer() {
    let fixture = Fixture::build(
        "recorded-implementer",
        Damage {
            two_tasks: true,
            two_tier: true,
            alternative_reviewer: true,
            ..Damage::default()
        },
    );
    plant_stale_verification(&fixture);
    let driven = drive(&fixture, &DriveSeams::default(), 1);
    driven
        .progress
        .first()
        .expect("one step")
        .as_ref()
        .expect("the re-verification publishes");
    assert_eq!(
        driven.implementers,
        vec![PassBinding::new(AGENT, "claude-opus-5")],
        "the verification plan was requested against the binding the candidate ran under"
    );
    assert_eq!(
        driven.reviewer_models,
        vec!["claude-fable-5".to_owned()],
        "the alternative reviewed the candidate; its own model did not"
    );
    let recorded: Vec<String> = driven
        .log
        .iter()
        .filter_map(|event| match &event.body {
            TopologyEventBody::MergePrepared { data } => data.verification.clone(),
            _ => None,
        })
        .flat_map(|verification| verification.reviews)
        .map(|review| review.model)
        .collect();
    assert_eq!(
        recorded,
        vec!["claude-fable-5".to_owned()],
        "and the durable merge_prepared records that reviewer"
    );
}

#[test]
fn a_gate_spawn_failure_during_integration_verification_defers_inside_max_defers() {
    let fixture = Fixture::two_tasks("gate-spawn-outage");
    plant_stale_verification(&fixture);
    let driven = drive(
        &fixture,
        &DriveSeams {
            gate_fails: Some(crate::error::ProcessFate::NeverStarted),
            ..DriveSeams::default()
        },
        5,
    );
    let shapes: Vec<String> = driven
        .progress
        .iter()
        .map(|step| match step {
            Ok(Progress::Unavailable {
                parked, sequence, ..
            }) => {
                format!("unavailable(s{}, parked={parked})", sequence.0)
            }
            Ok(Progress::Waited { .. }) => "waited".to_owned(),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(
        shapes,
        vec![
            "unavailable(s2, parked=false)",
            "waited",
            "unavailable(s3, parked=false)",
            "waited",
            "unavailable(s4, parked=true)",
        ],
        "each outage deferred inside the allowance and the third parked at it"
    );
    let terminals = unavailable_terminals(&driven.log);
    assert_eq!(terminals.len(), 3);
    for (index, terminal) in terminals.iter().enumerate() {
        assert!(
            matches!(
                terminal.cause,
                crate::topology::events::UnavailableCause::Infrastructure {
                    kind: crate::topology::events::InfrastructureKind::RunnerSpawnFailure
                }
            ),
            "terminal {index} names the runner: {:?}",
            terminal.cause
        );
    }
    assert!(matches!(
        terminals[0].outcome,
        crate::topology::events::UnavailableOutcome::Deferred { defers: 1 }
    ));
    assert!(matches!(
        terminals[1].outcome,
        crate::topology::events::UnavailableOutcome::Deferred { defers: 2 }
    ));
    let crate::topology::events::UnavailableOutcome::Parked { question } = &terminals[2].outcome
    else {
        panic!("the third outage parks: {:?}", terminals[2].outcome);
    };
    assert!(
        question.context.contains("missing-gate-executable"),
        "the park question carries what the Runner reported: {}",
        question.context
    );
    assert!(
        fixture.manager().intents().expect("intents").is_empty(),
        "every sequence's staging and snapshots were reclaimed at its terminal"
    );
    assert!(
        fixture
            .manager()
            .refs_under(&format!(
                "{}/{RUN_ID}/prepared/",
                crate::engine::topology::candidate::RUN_REF_ROOT
            ))
            .expect("refs")
            .is_empty(),
        "every sequence's pin was deleted at its terminal"
    );
    assert!(
        driven.invocations_balance && driven.entitlements_held == 0,
        "the failed invocations were cancelled and both entitlements released"
    );
}

fn production_container_runner(
    fixture: &Fixture,
    fake: &FakeRuntime,
) -> crate::runner::container::exec::ContainerRunner {
    crate::runner::container::exec::ContainerRunner::new(
        fixture.started.runner.clone(),
        crate::runner::container::exec::RunIdentity {
            private_root: fixture.private_root.clone(),
            run_id: RUN_ID.to_owned(),
            run_dir: fixture.public(),
            incarnation: RESUMER.to_owned(),
            repo_key: fixture.repo_key.as_str().to_owned(),
        },
        &fixture.repo_root,
        crate::runner::container::env::ContainerEnvironment::from_image(vec![
            ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
            ("HOME".to_owned(), "/root".to_owned()),
        ]),
        Box::new(fake.clone()),
    )
    .expect("the fixture records a container policy")
    .with_view(Box::new(DisposableDirView::new(ContainerTrace::default())))
    .with_poll(Duration::ZERO)
}

const RUNTIME_LOST_MID_GATE: [crate::runner::container::runtime::RuntimeOp; 3] = [
    crate::runner::container::runtime::RuntimeOp::Observe,
    crate::runner::container::runtime::RuntimeOp::Stop,
    crate::runner::container::runtime::RuntimeOp::Remove,
];

fn last_event_kind(fixture: &Fixture) -> String {
    TopologyFold::parse_log(&fixture.log_bytes())
        .expect("the log parses")
        .last()
        .map(|event| event.body.kind().to_owned())
        .expect("the log has events")
}

fn snapshot_intents(fixture: &Fixture) -> Vec<crate::workspace_manager::Slot> {
    fixture
        .manager()
        .intents()
        .expect("intents")
        .into_iter()
        .filter(|slot| matches!(slot, crate::workspace_manager::Slot::Snapshot { .. }))
        .collect()
}

#[test]
fn a_runner_that_loses_track_of_a_running_gate_refuses_resumably_and_reclaims_nothing() {
    let fixture = Fixture::two_tasks("lost-gate");
    plant_stale_verification(&fixture);
    let fake = runtime_holding_the_record();
    for op in RUNTIME_LOST_MID_GATE {
        fake.set_unreachable(op);
    }
    let runner = production_container_runner(&fixture, &fake);
    let mut hooks = HarnessTopologyHooks::new(harness());
    let driven = drive_with(&fixture, &DriveSeams::default(), 1, &runner, &mut hooks);

    let text = message(
        driven
            .progress
            .first()
            .expect("one step")
            .as_ref()
            .expect_err("a gate whose process may still be running ends the command"),
    );
    assert!(
        text.contains("may still be running") && text.contains("gate"),
        "the refusal says what the Runner could not establish: {text}"
    );
    let names = fake.container_names();
    let survivor = names
        .first()
        .and_then(|name| fake.container(name))
        .expect("the gate's container was created and survives");
    assert_eq!(names.len(), 1, "exactly the one gate container: {names:?}");
    assert_eq!(
        survivor.state,
        crate::runner::container::runtime::Liveness::Running,
        "the runtime never confirmed the gate stopped"
    );
    assert!(
        unavailable_terminals(&driven.log).is_empty(),
        "no terminal authorized cleanup or readmission while the gate may run"
    );
    assert_eq!(
        last_event_kind(&fixture),
        "merge_verification_started",
        "the verification stays open for recovery to settle"
    );
    assert_eq!(
        snapshot_intents(&fixture).len(),
        1,
        "the gate's snapshot is retained: the container has it mounted"
    );
}

#[cfg(unix)]
#[test]
fn a_host_integration_reaper_holds_the_runs_cleanup_lease() {
    struct LeaseObserver {
        public: PathBuf,
        holds: Arc<Mutex<Vec<bool>>>,
    }

    impl crate::agent::proc::SpawnHooks for LeaseObserver {
        fn point(&mut self, point: SubEffectPoint) -> Injection {
            if point == SubEffectPoint::ReaperStarted {
                self.holds
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(rundir::observe_cleanup_hold(&self.public, &mut NoHooks));
            }
            Injection::Proceed
        }
    }

    let fixture = Fixture::build(
        "host-reaper-lease",
        Damage {
            two_tasks: true,
            host_runner: true,
            host_gate: Some("exit 1"),
            ..Damage::default()
        },
    );
    plant_stale_verification(&fixture);
    let holds = Arc::new(Mutex::new(Vec::new()));
    let runner = crate::runner::host::HostRunner::new().with_hooks(Box::new(LeaseObserver {
        public: fixture.public(),
        holds: Arc::clone(&holds),
    }));
    let driven = drive_with(
        &fixture,
        &DriveSeams::default(),
        1,
        &runner,
        &mut HarnessTopologyHooks::new(harness()),
    );
    assert!(
        matches!(driven.progress.first(), Some(Ok(Progress::Rejected { .. }))),
        "the re-verification ran its gate through the production host runner and the gate's \
         exit 1 rejected it: {:?}",
        driven.progress
    );
    let observed = holds.lock().unwrap_or_else(PoisonError::into_inner).clone();
    assert!(
        !observed.is_empty(),
        "no reaper was started for the gate, so nothing was observed"
    );
    assert!(
        observed.iter().all(|held| *held),
        "at ReaperStarted the gate's reaper held no lease on this run's cleanup.lock (R28), so a \
         resume after the coordinator's death would take the exclusive side while the reaper \
         still reclaims the group: {observed:?}"
    );
    assert!(
        !rundir::observe_cleanup_hold(&fixture.public(), &mut NoHooks),
        "the hold outlived the reaper that took it"
    );
}

#[test]
fn a_removal_another_reclaimer_holds_is_not_proof_the_gate_is_gone() {
    use crate::runner::container::runtime::RuntimeOp;

    let fixture = Fixture::two_tasks("removal-in-progress");
    plant_stale_verification(&fixture);
    let fake = runtime_holding_the_record();
    for op in [RuntimeOp::Observe, RuntimeOp::Stop] {
        fake.set_unreachable(op);
    }
    fake.set_docker_stderr(
        RuntimeOp::Remove,
        "Error response from daemon: removal of container upstroke-gate is already in progress",
    );
    let runner = production_container_runner(&fixture, &fake);
    let mut hooks = HarnessTopologyHooks::new(harness());
    let driven = drive_with(&fixture, &DriveSeams::default(), 1, &runner, &mut hooks);

    let text = message(
        driven
            .progress
            .first()
            .expect("one step")
            .as_ref()
            .expect_err(
                "a removal another reclaimer holds establishes nothing, so the command ends",
            ),
    );
    assert!(
        text.contains("may still be running") && text.contains("already in progress"),
        "the refusal says the process may run and why the removal answer is not evidence: {text}"
    );
    let names = fake.container_names();
    let survivor = names
        .first()
        .and_then(|name| fake.container(name))
        .expect("the gate's container was created and survives the other reclaimer's flag");
    assert_eq!(names.len(), 1, "exactly the one gate container: {names:?}");
    assert_eq!(
        survivor.state,
        crate::runner::container::runtime::Liveness::Running,
        "the daemon sets its removal-in-progress flag before it kills, so the gate still runs"
    );
    assert!(
        unavailable_terminals(&driven.log).is_empty(),
        "no terminal authorized cleanup or readmission over the running gate"
    );
    assert_eq!(
        last_event_kind(&fixture),
        "merge_verification_started",
        "the verification stays open for recovery to settle"
    );
    assert_eq!(
        snapshot_intents(&fixture).len(),
        1,
        "the gate's snapshot is retained: the container has it mounted"
    );
    assert!(
        driven.transaction_open && driven.pipeline_held == 1,
        "the open transaction keeps its entitlements"
    );
}

#[test]
fn a_lost_gate_container_is_reclaimed_by_the_next_resume_before_the_verification_is_settled() {
    use crate::topology::effects::ContainerSite;

    let fixture = Fixture::two_tasks("lost-gate-reclaimed");
    plant_stale_verification(&fixture);
    let fake = runtime_holding_the_record();
    for op in RUNTIME_LOST_MID_GATE {
        fake.set_unreachable(op);
    }
    let runner = production_container_runner(&fixture, &fake);
    let driven = drive_with(
        &fixture,
        &DriveSeams::default(),
        1,
        &runner,
        &mut HarnessTopologyHooks::new(harness()),
    );
    assert!(
        driven.progress.first().is_some_and(Result::is_err),
        "the command ended resumably: {:?}",
        driven.progress
    );
    let after_loss = fixture.log_bytes();
    let survivor = fake.container_names();
    assert_eq!(survivor.len(), 1);

    let refused = message(
        &resume_as(
            &fixture,
            "resumer-2",
            &fake,
            &mut HarnessTopologyHooks::new(harness()),
        )
        .expect_err("a resume cannot reclaim the container while the runtime is unreachable"),
    );
    assert_eq!(
        fixture.log_bytes(),
        after_loss,
        "the refusal appended nothing: {refused}"
    );
    assert_eq!(
        fake.container_names(),
        survivor,
        "and touched no container: {refused}"
    );

    for op in RUNTIME_LOST_MID_GATE {
        fake.set_reachable(op);
    }
    let recovery = harness();
    resume_as(
        &fixture,
        "resumer-2",
        &fake,
        &mut HarnessTopologyHooks::new(Arc::clone(&recovery)),
    )
    .expect("with the runtime back the resume converges");
    assert!(
        fake.container_names().is_empty(),
        "the census reclaimed the earlier incarnation's container"
    );
    assert_eq!(
        interrupted_sequences(&fixture),
        vec![1, 2],
        "the lost verification was settled interrupted after the planted one"
    );
    let stopped = first_observation(&recovery, EffectSiteId::Container(ContainerSite::Stop))
        .expect("the container was stopped through its funnel");
    let appended = first_observation(&recovery, EffectSiteId::Event(EventSite::Append))
        .expect("the interrupted terminal was appended");
    assert!(
        stopped < appended,
        "the container was reclaimed (at {stopped}) before any recovery event (at {appended})"
    );
    assert!(
        snapshot_intents(&fixture).is_empty(),
        "the snapshot left after the terminal, once nothing could be running in it"
    );

    let driven = drive(&fixture, &DriveSeams::default(), 1);
    assert!(
        matches!(
            driven.progress.first(),
            Some(Ok(Progress::Integrated {
                key: ALPHA,
                sequence: crate::topology::events::SequenceId(3),
                ..
            }))
        ),
        "the candidate re-verifies and publishes under the next sequence: {:?}",
        driven.progress
    );
}

#[test]
fn a_gate_whose_runner_lost_it_after_start_and_reclaimed_it_defers_as_an_outage() {
    let fixture = Fixture::two_tasks("gate-gone");
    plant_stale_verification(&fixture);
    let driven = drive(
        &fixture,
        &DriveSeams {
            gate_fails: Some(crate::error::ProcessFate::Gone),
            ..DriveSeams::default()
        },
        1,
    );
    assert!(
        matches!(
            driven.progress.first(),
            Some(Ok(Progress::Unavailable { parked: false, .. }))
        ),
        "a process the Runner established gone is an observed outage: {:?}",
        driven.progress
    );
    let terminals = unavailable_terminals(&driven.log);
    let crate::topology::events::UnavailableCause::Infrastructure {
        kind: crate::topology::events::InfrastructureKind::Other { detail },
    } = &terminals[0].cause
    else {
        panic!(
            "not a spawn failure, an outage of its own: {:?}",
            terminals[0].cause
        );
    };
    assert!(
        detail.contains("gone") && detail.contains("gate"),
        "the terminal says the Runner lost a started gate: {detail}"
    );
    assert!(
        fixture.manager().intents().expect("intents").is_empty(),
        "with no process alive the snapshot and staging were reclaimed at the terminal"
    );
}

#[test]
fn an_unresolved_verification_leaves_its_entitlements_with_the_open_transaction() {
    let fixture = Fixture::two_tasks("unresolved-entitlements");
    plant_stale_verification(&fixture);
    let fake = runtime_holding_the_record();
    for op in RUNTIME_LOST_MID_GATE {
        fake.set_unreachable(op);
    }
    let runner = production_container_runner(&fixture, &fake);
    let driven = drive_with(
        &fixture,
        &DriveSeams::default(),
        2,
        &runner,
        &mut HarnessTopologyHooks::new(harness()),
    );
    assert!(
        driven.progress.first().is_some_and(Result::is_err),
        "the first step ends the command resumably: {:?}",
        driven.progress
    );
    assert_eq!(fake.container_names().len(), 1, "the running gate survives");
    assert!(
        unavailable_terminals(&driven.log).is_empty(),
        "no terminal was appended over the running gate"
    );
    assert!(
        driven.transaction_open,
        "the verification transaction is still open in the fold"
    );
    assert_eq!(
        driven.pipeline_held, 1,
        "the open transaction is what holds the pipeline entitlement now"
    );
    assert_eq!(
        (driven.entitlements_held, driven.reservations_cancelled),
        (0, 0),
        "the provisional reservation converted at merge_verification_started, before any \
         process ran, exactly as C.side_effect_vs_event_ordering has it, and nothing cancelled \
         it; it is not what holds the entitlements after the append, so a count of it says \
         nothing about a release"
    );
    assert_eq!(
        driven.log.len(),
        TopologyFold::parse_log(&fixture.log_bytes())
            .expect("the log parses")
            .len(),
        "the second step in the same incarnation appended nothing"
    );
    assert!(
        !matches!(
            driven.progress.get(1),
            Some(Ok(Progress::Integrated { .. }
                | Progress::Rejected { .. }
                | Progress::Unavailable { .. }))
        ),
        "the second step admitted no sequence beside the open transaction: {:?}",
        driven.progress
    );
    assert_eq!(
        fake.container_names().len(),
        1,
        "and started no second gate beside the surviving one"
    );
    assert_eq!(
        last_event_kind(&fixture),
        "merge_verification_started",
        "the transaction is the next resume's to settle"
    );
}

#[test]
fn repeated_container_launch_outages_before_start_consume_defers_through_the_production_runner() {
    use crate::runner::container::ContainerHooks;
    use crate::runner::container::runtime::RuntimeOp;
    use crate::topology::effects::ContainerSite;

    struct RuntimeLostAtCreate {
        fake: FakeRuntime,
        trace: ContainerTrace,
    }

    impl ContainerHooks for RuntimeLostAtCreate {
        fn phase(&mut self, site: EffectSiteId, phase: HookPhase) -> Injection {
            if site == EffectSiteId::Container(ContainerSite::Create) && phase == HookPhase::Before
            {
                for op in [RuntimeOp::Create, RuntimeOp::Stop, RuntimeOp::Remove] {
                    self.fake.set_unreachable(op);
                }
            }
            Injection::Proceed
        }

        fn trace(&self) -> ContainerTrace {
            self.trace.clone()
        }
    }

    let fixture = Fixture::two_tasks("container-launch-outages");
    plant_stale_verification(&fixture);
    let fake = runtime_holding_the_record();
    let mut shapes = Vec::new();
    for (restart, incarnation) in ["resumer-1", "resumer-2", "resumer-3"]
        .into_iter()
        .enumerate()
    {
        for op in [RuntimeOp::Create, RuntimeOp::Stop, RuntimeOp::Remove] {
            fake.set_reachable(op);
        }
        let runner = production_container_runner(&fixture, &fake).with_hooks(Box::new(
            RuntimeLostAtCreate {
                fake: fake.clone(),
                trace: ContainerTrace::default(),
            },
        ));
        let driven = drive_as(
            &fixture,
            incarnation,
            &fake,
            &DriveSeams::default(),
            1,
            &runner,
            &mut HarnessTopologyHooks::new(harness()),
        );
        shapes.push(match driven.progress.first() {
            Some(Ok(Progress::Unavailable {
                parked, sequence, ..
            })) => format!("unavailable(s{}, parked={parked})", sequence.0),
            other => format!("restart {restart}: {other:?}"),
        });
        assert!(
            fake.container_names().is_empty(),
            "restart {restart}: no container exists after an outage before start"
        );
        assert!(
            driven.entitlements_held == 0 && driven.invocations_balance,
            "restart {restart}: the terminal released the sequence's holdings"
        );
    }
    assert_eq!(
        shapes,
        vec![
            "unavailable(s2, parked=false)",
            "unavailable(s3, parked=false)",
            "unavailable(s4, parked=true)",
        ],
        "each observed pre-start outage consumed a defer and the third parked at the limit"
    );
    assert!(
        !fake.calls().contains(&RuntimeOp::Start),
        "no gate process was ever started: {:?}",
        fake.calls()
    );
    let terminals = unavailable_terminals(
        &TopologyFold::parse_log(&fixture.log_bytes()).expect("the log parses"),
    );
    assert_eq!(terminals.len(), 3);
    for (index, terminal) in terminals.iter().enumerate() {
        assert!(
            matches!(
                terminal.cause,
                crate::topology::events::UnavailableCause::Infrastructure {
                    kind: crate::topology::events::InfrastructureKind::RunnerSpawnFailure
                }
            ),
            "terminal {index} is the spawn failure it was: {:?}",
            terminal.cause
        );
    }
    assert!(matches!(
        terminals[0].outcome,
        crate::topology::events::UnavailableOutcome::Deferred { defers: 1 }
    ));
    assert!(matches!(
        terminals[1].outcome,
        crate::topology::events::UnavailableOutcome::Deferred { defers: 2 }
    ));
    assert!(matches!(
        terminals[2].outcome,
        crate::topology::events::UnavailableOutcome::Parked { .. }
    ));
    assert_eq!(
        interrupted_sequences(&fixture),
        vec![1],
        "only the planted stale verification was settled interrupted; none of the three \
         outages was mistaken for one"
    );
}

#[test]
fn a_git_error_observed_by_the_verification_settles_an_infrastructure_outage() {
    let fixture = Fixture::two_tasks("git-state");
    plant_stale_verification(&fixture);
    let driven = drive(
        &fixture,
        &DriveSeams {
            input_git_error: true,
            ..DriveSeams::default()
        },
        1,
    );
    assert!(
        matches!(
            driven.progress.first(),
            Some(Ok(Progress::Unavailable { parked: false, .. }))
        ),
        "foreign Git state under the review-input reader is an outage of the sequence, \
         not an interruption: {:?}",
        driven.progress
    );
    let terminals = unavailable_terminals(&driven.log);
    let crate::topology::events::UnavailableCause::Infrastructure {
        kind: crate::topology::events::InfrastructureKind::Other { detail },
    } = &terminals[0].cause
    else {
        panic!(
            "an infrastructure outage of its own kind: {:?}",
            terminals[0].cause
        );
    };
    assert!(
        detail.contains("foreign Git state") && detail.contains("index"),
        "the terminal carries what Git reported: {detail}"
    );
    assert!(
        matches!(
            terminals[0].outcome,
            crate::topology::events::UnavailableOutcome::Deferred { defers: 1 }
        ),
        "deferred inside the allowance: {:?}",
        terminals[0].outcome
    );
    assert!(
        driven.reviewer_models.is_empty(),
        "no reviewer ran on a tree the verification could not read"
    );
    assert!(
        fixture.manager().intents().expect("intents").is_empty(),
        "the staging worktree with the corrupt index was reclaimed with force at the terminal"
    );
}

#[test]
fn a_gate_that_times_out_during_integration_verification_defers_instead_of_registering_a_repair() {
    let fixture = Fixture::two_tasks("gate-timeout");
    plant_stale_verification(&fixture);
    let driven = drive(
        &fixture,
        &DriveSeams {
            gate_times_out: true,
            ..DriveSeams::default()
        },
        1,
    );
    assert!(
        matches!(
            driven.progress.first(),
            Some(Ok(Progress::Unavailable { parked: false, .. }))
        ),
        "a timeout is not a repair (decisions.repairs.not_repairs): {:?}",
        driven.progress
    );
    assert!(
        driven
            .log
            .iter()
            .all(|event| !matches!(event.body, TopologyEventBody::MergeRejected { .. })),
        "no repair was registered for a gate that produced no verdict"
    );
    let terminals = unavailable_terminals(&driven.log);
    let crate::topology::events::UnavailableCause::Infrastructure {
        kind: crate::topology::events::InfrastructureKind::Other { detail },
    } = &terminals[0].cause
    else {
        panic!("an outage of its own kind: {:?}", terminals[0].cause);
    };
    assert!(
        detail.contains("timed out") && detail.contains("gate"),
        "the terminal names the timed-out gate: {detail}"
    );
    assert!(
        driven.reviewer_models.is_empty(),
        "no reviewer ran after a gate that produced no verdict"
    );
}

fn plant_stale_verification_of_a_test_candidate(
    fixture: &Fixture,
) -> (crate::topology::events::CandidateRef, CommitSha, CommitSha) {
    use crate::workspace_manager::fixture::{git, write_file};
    const SHARED_TEST: &str = "#[test]\nfn shared() {\n    assert!(true);\n}\n";
    let head = plant_published_beta_editing(fixture, "tests/shared.rs", SHARED_TEST);

    let repo = &fixture.repo_root;
    write_file(&repo.join("tests/shared.rs"), SHARED_TEST.as_bytes());
    write_file(&repo.join("support.rs"), b"pub fn helper() {}\n");
    git(repo, &["add", "--", "tests/shared.rs", "support.rs"]);
    let tree = CommitSha(git(repo, &["write-tree"]));
    let commit = CommitSha(git(
        repo,
        &[
            "commit-tree",
            tree.as_str(),
            "-p",
            fixture.base_sha.as_str(),
            "-m",
            "upstroke: alpha attempt 1",
        ],
    ));
    git(
        repo,
        &["rm", "-q", "-f", "--", "tests/shared.rs", "support.rs"],
    );
    let names = crate::engine::topology::candidate::CandidateNames::of(RUN_ID, ALPHA, GEN);
    for refname in [&names.prepared_ref, &names.candidate_ref] {
        git(repo, &["update-ref", refname.as_str(), commit.as_str()]);
    }
    let candidate = crate::topology::events::CandidateRef {
        key: ALPHA,
        generation: GEN,
        commit_sha: commit.clone(),
        candidate_ref: names.candidate_ref.clone(),
    };
    append_events(
        fixture,
        &[
            dispatched_at(&fixture.base_sha),
            attempt_started(1),
            candidate_prepared_for(fixture, ALPHA, &commit, &tree, &names, "support.rs"),
            TopologyEventBody::TaskCandidateCreated {
                data: crate::topology::events::TaskCandidateCreated {
                    candidate: candidate.clone(),
                },
            },
        ],
    );

    let proposal = commit_on(
        fixture,
        head.as_str(),
        "support.rs",
        "pub fn helper() {}\n",
        "upstroke: proposal s1",
    );
    let pin = crate::engine::topology::integrate::prepared_pin_ref(
        RUN_ID,
        crate::topology::events::SequenceId(1),
    );
    git(repo, &["update-ref", pin.as_str(), proposal.as_str()]);
    plant_staging_worktree(fixture, 1, proposal.as_str());
    append_events(
        fixture,
        &[TopologyEventBody::MergeVerificationStarted {
            data: crate::topology::events::MergeVerificationStarted {
                sequence: crate::topology::events::SequenceId(1),
                candidate: candidate.clone(),
                basis: crate::topology::events::VerificationBasis::StaleClean { prepared_ref: pin },
                expected_head: head.clone(),
                proposed_sha: proposal.clone(),
            },
        }],
    );
    (candidate, head, proposal)
}

#[test]
fn a_test_candidate_whose_test_was_already_published_is_verified_not_rejected_for_provenance() {
    let fixture = Fixture::build(
        "test-provenance",
        Damage {
            two_tasks: true,
            alpha_kind: Some(TaskKind::Test),
            ..Damage::default()
        },
    );
    let (candidate, head, proposal) = plant_stale_verification_of_a_test_candidate(&fixture);

    let manager = fixture.manager();
    let own_diff = manager
        .candidate_diff(
            &crate::engine::topology::integrate::staging_slot(crate::topology::events::SequenceId(
                1,
            )),
            fixture.base_sha.as_str(),
            candidate.commit_sha.as_str(),
        )
        .expect("the candidate's own diff");
    let integration_diff = manager
        .candidate_diff(
            &crate::engine::topology::integrate::staging_slot(crate::topology::events::SequenceId(
                1,
            )),
            head.as_str(),
            proposal.as_str(),
        )
        .expect("the integration diff");
    assert!(
        crate::engine::classify::diff_failure(&own_diff, TaskKind::Test, true).is_none(),
        "the candidate's own diff passes the Test-provenance rule"
    );
    assert!(
        crate::engine::classify::diff_failure(&integration_diff, TaskKind::Test, true)
            .is_some_and(|failure| failure.kind == crate::ladder::FailureKind::TestProvenance),
        "the integration diff, the candidate cherry-picked onto a head that already holds its \
         test, would fail that rule: the witness discriminates"
    );

    let driven = drive(&fixture, &DriveSeams::default(), 1);
    assert!(
        matches!(
            driven.progress.first(),
            Some(Ok(Progress::Integrated { key: ALPHA, .. }))
        ),
        "the integration judges size and opacity only, and the candidate publishes: {:?}",
        driven.progress
    );
    assert!(
        driven
            .log
            .iter()
            .all(|event| !matches!(event.body, TopologyEventBody::MergeRejected { .. })),
        "no repair was registered for a test the tree already contains"
    );
    assert_eq!(
        driven.reviewer_models.len(),
        1,
        "the reviewer judged the proposal"
    );
}

#[test]
fn the_production_verifier_judges_the_recorded_proposal_and_removes_its_snapshots_after_the_terminal()
 {
    for (label, seams, terminal) in [
        ("prepared", DriveSeams::default(), "merge_prepared"),
        (
            "rejected",
            DriveSeams {
                gate_exit_code: Some(1),
                ..DriveSeams::default()
            },
            "merge_rejected",
        ),
        (
            "parked",
            DriveSeams {
                review_needs_human: true,
                ..DriveSeams::default()
            },
            "merge_verification_unavailable",
        ),
    ] {
        let fixture = Fixture::two_tasks(&format!("verifier-oracle-{label}"));
        let (candidate, _, _) = plant_stale_verification(&fixture);
        let mut hooks = SnapshotOrderHooks {
            rest: HarnessTopologyHooks::new(harness()),
            effects: SnapshotRemovalObserver {
                log: fixture.log(),
                last_event_at_removal: Vec::new(),
            },
        };
        let driven = drive_hooked(&fixture, &seams, 1, &mut hooks);
        assert!(
            driven.progress.first().is_some_and(Result::is_ok),
            "{label}: the sequence reached its terminal: {:?}",
            driven.progress
        );

        let proposed = driven
            .log
            .iter()
            .filter_map(|event| match &event.body {
                TopologyEventBody::MergeVerificationStarted { data }
                    if data.sequence == crate::topology::events::SequenceId(2) =>
                {
                    Some(data.proposed_sha.clone())
                }
                _ => None,
            })
            .next()
            .expect("the re-verification under sequence 2 recorded its proposal");
        assert_ne!(
            proposed, candidate.commit_sha,
            "{label}: a stale proposal is a new commit"
        );
        let gates: Vec<&(crate::runner::InvocationId, Option<String>)> = driven
            .gate_heads
            .iter()
            .filter(|(invocation, _)| {
                matches!(invocation, crate::runner::InvocationId::Sequence { .. })
            })
            .collect();
        assert_eq!(
            gates.len(),
            1,
            "{label}: one gate ran: {:?}",
            driven.gate_heads
        );
        assert!(
            gates
                .iter()
                .all(|(_, head)| head.as_deref() == Some(proposed.as_str())),
            "{label}: the gate judged a checkout whose HEAD is the recorded proposal {proposed}, \
             never the candidate commit {}: {gates:?}",
            candidate.commit_sha
        );

        let reviews: Vec<&DrivenRun> = driven
            .runs
            .iter()
            .filter(|run| run.role == crate::runner::ExecutionRole::Review)
            .collect();
        let gate_workspace = driven
            .runs
            .iter()
            .find(|run| run.role == crate::runner::ExecutionRole::Gate)
            .map(|run| run.workspace.clone())
            .expect("the gate ran");
        let staging =
            fixture
                .manager()
                .slot_path(&crate::engine::topology::integrate::staging_slot(
                    crate::topology::events::SequenceId(2),
                ));
        if label == "rejected" {
            assert!(
                reviews.is_empty(),
                "{label}: no reviewer runs after a failed gate"
            );
        } else {
            assert_eq!(
                reviews.len(),
                1,
                "{label}: one reviewer ran: {:?}",
                driven.runs
            );
            let review = reviews[0];
            assert_eq!(
                review.head.as_deref(),
                Some(proposed.as_str()),
                "{label}: the reviewer judged a checkout whose HEAD is the recorded proposal"
            );
            assert_ne!(
                review.workspace, staging,
                "{label}: the reviewer ran in the staging worktree"
            );
            assert_ne!(
                review.workspace, gate_workspace,
                "{label}: the reviewer shared the gate's snapshot"
            );
            assert_eq!(
                review.workspace,
                fixture
                    .manager()
                    .slot_path(&crate::workspace_manager::Slot::Snapshot {
                        name: crate::workspace_manager::SnapshotName::integration_review(2, 0),
                    }),
                "{label}: the reviewer's checkout is its own exact snapshot of the proposal"
            );
        }
        assert!(
            driven.runs.iter().all(|run| run.workspace != staging),
            "{label}: a process ran in the staging worktree: {:?}",
            driven.runs
        );

        let removals = &hooks.effects.last_event_at_removal;
        assert_eq!(
            removals.len(),
            if label == "rejected" { 1 } else { 2 },
            "{label}: every snapshot of the sequence was removed once: {removals:?}"
        );

        assert!(
            removals.iter().all(|last| last == terminal),
            "{label}: a snapshot removal began while the log ended in {removals:?} rather than \
             the terminal `{terminal}`"
        );
    }
}

#[test]
fn a_paid_review_that_parks_is_charged_live_and_its_replay_loss_is_the_deferred_vocabulary_gap() {
    let options =
        crate::engine::coordinator::topology_question_options(crate::ir::QuestionKind::Clarify);
    let fixture = Fixture::two_tasks("paid-park");
    plant_stale_verification(&fixture);
    let driven = drive(
        &fixture,
        &DriveSeams {
            review_needs_human: true,
            review_cost_usd: Some(2.5),
            run_ceiling_usd: Some(2.2),
            answer: Some(crate::ir::Answer::Answered {
                text: options[0].clone(),
            }),
            ..DriveSeams::default()
        },
        3,
    );
    let shapes: Vec<String> = driven
        .progress
        .iter()
        .map(|step| match step {
            Ok(Progress::Unavailable { parked, .. }) => format!("unavailable(parked={parked})"),
            Ok(Progress::Answered { declined, .. }) => format!("answered(declined={declined})"),
            Ok(Progress::BudgetExceeded) => "budget_exceeded".to_owned(),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(
        shapes,
        vec![
            "unavailable(parked=true)",
            "answered(declined=false)",
            "budget_exceeded"
        ],
        "the paid review parks, the answer returns the candidate, and the live total refuses \
         the next integration under the ceiling"
    );
    assert!(
        (driven.spend_after - (driven.spend_before + 2.5)).abs() < 1e-9,
        "the parked review was charged live: {} -> {}",
        driven.spend_before,
        driven.spend_after
    );
    assert!(
        driven.spend_before < 2.2 && driven.spend_after > 2.2,
        "the ceiling sits between the replayed and the live total: {} < 2.2 < {}",
        driven.spend_before,
        driven.spend_after
    );

    let replayed = crate::engine::topology::select::Spend::replay(&driven.log).run_total();
    assert!(
        (replayed - driven.spend_before).abs() < 1e-9,
        "PR8-R2-SPEND-REPLAY (deferred): the frozen `merge_verification_unavailable` carries no \
         review record, so a replay restores {replayed} where the incarnation that parked the \
         candidate had reached {}; `decisions.coordinator_integration.dispositions` requires the \
         spend recorded and the vocabulary cannot carry it (Class C). When the terminal gains its \
         record this assertion fails and the ledger row closes",
        driven.spend_after
    );
}

#[test]
fn an_unjudgeable_proposal_parks_the_candidate_for_a_person() {
    let fixture = Fixture::two_tasks("input-rejected");
    plant_stale_verification(&fixture);
    let driven = drive(
        &fixture,
        &DriveSeams {
            input_rejected: true,
            ..DriveSeams::default()
        },
        1,
    );
    assert!(
        matches!(
            driven.progress.first(),
            Some(Ok(Progress::Unavailable { parked: true, .. }))
        ),
        "the policy's refusal parks the candidate: {:?}",
        driven.progress
    );
    let terminals = unavailable_terminals(&driven.log);
    let crate::topology::events::UnavailableCause::HumanRequired { verdict } = &terminals[0].cause
    else {
        panic!("a person is required: {:?}", terminals[0].cause);
    };
    assert!(
        verdict.contains("opaque review inputs"),
        "the terminal carries the policy's problem: {verdict}"
    );
    assert!(
        driven.reviewer_models.is_empty(),
        "no reviewer was invoked on an input the policy refused"
    );
    assert_eq!(merged_sequences(&fixture), vec![0], "only BETA merged");
}

#[test]
fn an_integration_reviews_cost_reaches_the_run_spend() {
    let fixture = Fixture::two_tasks("integration-spend");
    plant_stale_verification(&fixture);
    let driven = drive(
        &fixture,
        &DriveSeams {
            review_cost_usd: Some(2.5),
            ..DriveSeams::default()
        },
        1,
    );
    driven
        .progress
        .first()
        .expect("one step")
        .as_ref()
        .expect("the re-verification publishes");
    assert!(
        (driven.spend_after - (driven.spend_before + 2.5)).abs() < 1e-9,
        "the integration review charged 2.5 and spend went {} -> {}",
        driven.spend_before,
        driven.spend_after
    );
    let replayed = crate::engine::topology::select::Spend::replay(&driven.log).run_total();
    assert!(
        (replayed - driven.spend_after).abs() < 1e-9,
        "a replay of the log charges what the live run charged: {replayed} vs {}",
        driven.spend_after
    );
}

#[test]
fn a_verification_park_answer_is_ingested_and_the_candidate_re_verifies() {
    let options =
        crate::engine::coordinator::topology_question_options(crate::ir::QuestionKind::Clarify);
    let fixture = Fixture::two_tasks("park-answered");
    plant_stale_verification(&fixture);
    append_events(
        &fixture,
        &[TopologyEventBody::MergeVerificationUnavailable {
            data: crate::topology::events::MergeVerificationUnavailable {
                sequence: crate::topology::events::SequenceId(1),
                cause: crate::topology::events::UnavailableCause::HumanRequired {
                    verdict: "a person must decide this integration".to_owned(),
                },
                outcome: crate::topology::events::UnavailableOutcome::Parked {
                    question: crate::topology::events::FrozenQuestion {
                        id: crate::ir::QuestionId("q-park-planted".to_owned()),
                        key: ALPHA,
                        kind: crate::ir::QuestionKind::Clarify,
                        context: "integration verification needs a person".to_owned(),
                        options: options.clone(),
                    },
                },
            },
        }],
    );
    let driven = drive(
        &fixture,
        &DriveSeams {
            answer: Some(crate::ir::Answer::Answered {
                text: options[0].clone(),
            }),
            ..DriveSeams::default()
        },
        2,
    );
    assert!(
        matches!(
            driven.progress.first(),
            Some(Ok(Progress::Answered {
                key: ALPHA,
                declined: false,
                ..
            }))
        ),
        "the verification-park answer was ingested: {:?}",
        driven.progress
    );
    assert!(
        matches!(
            driven.progress.get(1),
            Some(Ok(Progress::Integrated {
                key: ALPHA,
                sequence: crate::topology::events::SequenceId(2),
                ..
            }))
        ),
        "an answered park returns the candidate to the queue and it re-verifies: {:?}",
        driven.progress
    );
}

fn plant_over_limit_repair(fixture: &Fixture) -> (crate::topology::events::MergeRejected, TaskKey) {
    let (rejection, key) = plant_rejected_repair(fixture);
    assert!(
        matches!(
            rejection.repair.admission,
            crate::topology::events::SpawnAdmission::HumanRequired { limit: 0, .. }
        ),
        "with no automatic repairs the first repair asks a person"
    );
    (rejection, key)
}

/// A stale verification of alpha's candidate at beta's published head,
/// rejected by review: the rejection registers alpha's first repair, admitted
/// as the fixture's `max_merge_repairs` decides (`Runnable` at the default of
/// one, `HumanRequired` under `no_automatic_repairs`).
fn plant_rejected_repair(fixture: &Fixture) -> (crate::topology::events::MergeRejected, TaskKey) {
    let (candidate, head, _pin) = plant_stale_verification(fixture);
    let rejection = {
        let events = TopologyFold::parse_log(&fixture.log_bytes()).expect("parses");
        let fold = TopologyFold::replay(fixture.inputs(), &events).expect("replays");
        crate::engine::topology::repair::merge_rejected(
            &fold,
            &FixedIds,
            &candidate,
            head,
            crate::topology::events::SequenceId(1),
            crate::topology::events::RejectionDisposition::CodeRejected {
                verification: crate::engine::topology::repair::code_rejection_record(
                    true,
                    Vec::new(),
                    "the reviewer rejected the proposal".to_owned(),
                ),
            },
            PathSet::Prefixes {
                paths: vec![GitPath("candidate.txt".to_owned())],
            },
        )
        .expect("the rejection registers a repair")
    };
    let key = rejection.repair.key;
    append_events(
        fixture,
        &[TopologyEventBody::MergeRejected {
            data: Box::new(rejection.clone()),
        }],
    );
    (rejection, key)
}

#[test]
fn an_over_limit_repair_spends_nothing_until_its_answer_activates_it() {
    let fixture = Fixture::build(
        "over-limit-waits",
        Damage {
            two_tasks: true,
            no_automatic_repairs: true,
            ..Damage::default()
        },
    );
    let (_rejection, repair) = plant_over_limit_repair(&fixture);
    let kinds_before = durable_kinds(&fixture);

    let driven = drive(&fixture, &DriveSeams::default(), 2);
    for (index, progress) in driven.progress.iter().enumerate() {
        assert!(
            matches!(progress, Ok(Progress::Blocked { questions: 1 })),
            "step {index}: an over-limit repair is `AwaitingInput` and the run hard-blocks on \
             its one question with nothing to spend on: {progress:?}"
        );
    }
    assert_eq!(
        durable_kinds(&fixture),
        {
            let mut expected = kinds_before;
            expected.push("run_resumed".to_owned());
            expected
        },
        "beyond the resume's own `run_resumed`, nothing was appended while the question stood: \
         no dispatch, no attempt, no spend"
    );
    assert!(
        driven.runs.is_empty(),
        "no process ran for a repair a person has not approved: {:?}",
        driven.runs
    );
    assert!(
        (driven.spend_after - driven.spend_before).abs() < 1e-9,
        "and no spend was recorded"
    );
    let fold = replayed(&fixture);
    assert_eq!(fold.task_state(repair), Some(TaskState::AwaitingInput));
    assert_eq!(fold.task_state(ALPHA), Some(TaskState::AwaitingRepair));
}

#[test]
fn a_repair_admission_answer_activates_the_repair_which_materializes_and_merges_through_the_queue()
{
    let fixture = Fixture::build(
        "repair-admission-answer",
        Damage {
            two_tasks: true,
            no_automatic_repairs: true,
            ..Damage::default()
        },
    );
    let (rejection, repair) = plant_over_limit_repair(&fixture);
    let candidate = rejection.candidate.clone();
    let question_options = rejection
        .repair
        .admission
        .question()
        .expect("a human admission carries its question")
        .options
        .clone();
    let head_before =
        ref_target(&fixture, fixture.started.integration_ref.as_str()).expect("the ref exists");

    let driven = drive(
        &fixture,
        &DriveSeams {
            answer: Some(crate::ir::Answer::Answered {
                text: question_options[0].clone(),
            }),
            ..DriveSeams::default()
        },
        3,
    );
    assert!(
        matches!(
            driven.progress.first(),
            Some(Ok(Progress::Answered {
                key,
                declined: false,
                ..
            })) if *key == repair
        ),
        "step 1 ingests the over-limit activation: {:?}",
        driven.progress
    );
    assert!(
        matches!(
            driven.progress.get(1),
            Some(Ok(Progress::Settled {
                key,
                accepted: true,
                ..
            })) if *key == repair
        ),
        "step 2 dispatches the activated repair, materializes its source candidate and runs its \
         first attempt through to a candidate: {:?}",
        driven.progress
    );
    assert!(
        matches!(
            driven.progress.get(2),
            Some(Ok(Progress::Integrated {
                key,
                sequence: crate::topology::events::SequenceId(2),
                ..
            })) if *key == repair
        ),
        "step 3 integrates the lineage candidate through the same queue: {:?}",
        driven.progress
    );

    let kinds: Vec<&str> = driven.log.iter().map(|event| event.body.kind()).collect();
    let position = |kind: &str| {
        kinds
            .iter()
            .rposition(|seen| *seen == kind)
            .unwrap_or_else(|| panic!("`{kind}` is not in the log: {kinds:?}"))
    };
    assert!(
        position("question_answered") < position("task_dispatched"),
        "`question_answered` before dispatch of an activated repair \
         (`side_effect_vs_event_ordering`): {kinds:?}"
    );

    let dispatched = driven
        .log
        .iter()
        .find_map(|event| match &event.body {
            TopologyEventBody::TaskDispatched { data } if data.key == repair => Some(data),
            _ => None,
        })
        .expect("the repair was dispatched");
    assert_eq!(
        dispatched.lease,
        LeaseGrant::InheritedLineage { root: ALPHA },
        "a repair executes inside its root's lineage lease"
    );
    assert_eq!(
        dispatched.source_candidate.as_ref(),
        Some(&candidate),
        "and records the rejected candidate as the source it was materialized from"
    );
    assert_eq!(
        dispatched.base_sha.0, head_before,
        "at the integration head current at its actual dispatch"
    );

    let started = driven
        .log
        .iter()
        .find_map(|event| match &event.body {
            TopologyEventBody::AttemptStarted { data } if data.key == repair => Some(data),
            _ => None,
        })
        .expect("the repair's attempt started");
    assert_eq!(
        started.materialization_observed,
        Some(crate::topology::events::Materialization::Clean),
        "the candidate's change applied cleanly onto the moved head, and the observation is \
         recorded before the spawn"
    );
    assert_eq!(started.resume_session, None, "a first attempt, not a retry");

    let prepared = driven
        .log
        .iter()
        .find_map(|event| match &event.body {
            TopologyEventBody::CandidatePrepared { data } if data.key == repair => Some(data),
            _ => None,
        })
        .expect("the repair's candidate was prepared");
    assert!(
        matches!(
            &prepared.lease_effect,
            crate::topology::events::CandidateLeaseEffect::WidensLineage { root, .. } if *root == ALPHA
        ),
        "a lineage member's candidate widens its lineage and takes no lease of its own: {:?}",
        prepared.lease_effect
    );

    let merged = driven
        .log
        .iter()
        .find_map(|event| match &event.body {
            TopologyEventBody::MergePrepared { data } if data.key == repair => Some(data),
            _ => None,
        })
        .expect("the lineage candidate was published");
    assert_eq!(
        merged.disposition,
        crate::topology::events::PreparedDisposition::Fast,
        "the repair's base is the head it merges onto, so the publication is exact-base"
    );
    assert_eq!(
        merged.satisfies,
        vec![ALPHA, repair],
        "`satisfies` is the canonical ordered closure: the root and its repair"
    );
    let task_merged = driven
        .log
        .iter()
        .find_map(|event| match &event.body {
            TopologyEventBody::TaskMerged { data }
                if data.sequence == crate::topology::events::SequenceId(2) =>
            {
                Some(data)
            }
            _ => None,
        })
        .expect("task_merged");
    assert_eq!(
        task_merged.lease_release,
        crate::topology::events::MergeLeaseRelease::Lineage { root: ALPHA },
        "the publication that settles the root releases the lineage lease"
    );

    let fold = replayed(&fixture);
    assert_eq!(fold.task_state(ALPHA), Some(TaskState::Merged));
    assert_eq!(fold.task_state(repair), Some(TaskState::Merged));
    assert!(
        !fold.leases().expect("started").any_candidate_or_lineage(),
        "no lineage lease survives the publication"
    );
    assert!(fold.queue().expect("started").is_empty());
    assert!(!driven.transaction_open);
    assert_eq!(driven.pipeline_held, 0);
    assert_eq!(driven.entitlements_held, 0);
    assert!(driven.invocations_balance);
    assert!(
        ref_target(&fixture, candidate.candidate_ref.as_str()).is_some(),
        "R11: the rejected candidate's candidates ref is never pruned while the run can resume"
    );
    assert_eq!(
        ref_target(&fixture, fixture.started.integration_ref.as_str()).as_deref(),
        Some(merged.proposed_sha.as_str()),
        "the integration ref is at the lineage candidate the publication named"
    );
}

const BETA: TaskKey = TaskKey(1);

fn plant_published_beta(fixture: &Fixture) -> CommitSha {
    plant_published_beta_editing(fixture, "other.txt", "another task\n")
}

fn plant_published_beta_editing(fixture: &Fixture, file: &str, content: &str) -> CommitSha {
    use crate::workspace_manager::fixture::git;
    let commit = commit_on(
        fixture,
        fixture.base_sha.as_str(),
        file,
        content,
        "upstroke: task 1 attempt 1",
    );
    let tree = CommitSha(git(
        &fixture.repo_root,
        &["rev-parse", &format!("{commit}^{{tree}}")],
    ));
    let names = crate::engine::topology::candidate::CandidateNames::of(RUN_ID, BETA, GEN);
    for refname in [&names.prepared_ref, &names.candidate_ref] {
        git(
            &fixture.repo_root,
            &["update-ref", refname.as_str(), commit.as_str()],
        );
    }
    let candidate = crate::topology::events::CandidateRef {
        key: BETA,
        generation: GEN,
        commit_sha: commit.clone(),
        candidate_ref: names.candidate_ref.clone(),
    };
    append_events(
        fixture,
        &[
            for_task(BETA, "beta", dispatched_at(&fixture.base_sha)),
            for_task(BETA, "beta", attempt_started_in(fixture, 1)),
            candidate_prepared_for(fixture, BETA, &commit, &tree, &names, file),
            TopologyEventBody::TaskCandidateCreated {
                data: crate::topology::events::TaskCandidateCreated {
                    candidate: candidate.clone(),
                },
            },
            TopologyEventBody::MergePrepared {
                data: Box::new(crate::topology::events::MergePrepared {
                    sequence: crate::topology::events::SequenceId(0),
                    disposition: crate::topology::events::PreparedDisposition::Fast,
                    expected_head: fixture.base_sha.clone(),
                    proposed_sha: commit.clone(),
                    key: BETA,
                    generation: GEN,
                    candidate_sha: commit.clone(),
                    candidate_ref: names.candidate_ref.clone(),
                    prepared_ref: None,
                    verification_source:
                        crate::topology::events::VerificationSource::CandidatePrepared {
                            key: BETA,
                            generation: GEN,
                        },
                    verification: None,
                    satisfies: vec![BETA],
                }),
            },
            TopologyEventBody::TaskMerged {
                data: crate::topology::events::TaskMerged {
                    sequence: crate::topology::events::SequenceId(0),
                    merged_sha: commit.clone(),
                    satisfies: vec![BETA],
                    lease_release: crate::topology::events::MergeLeaseRelease::Candidate {
                        key: BETA,
                        generation: GEN,
                    },
                },
            },
        ],
    );
    git(
        &fixture.repo_root,
        &[
            "update-ref",
            fixture.started.integration_ref.as_str(),
            commit.as_str(),
        ],
    );
    commit
}

fn plant_stale_queued_candidate(fixture: &Fixture) -> (PlantedTransaction, CommitSha) {
    let head = plant_published_beta(fixture);
    let planted = plant_queued_candidate_events(fixture);
    (planted, head)
}

fn worktree_git_dir(worktree: &Path) -> PathBuf {
    PathBuf::from(crate::workspace_manager::fixture::git(
        worktree,
        &["rev-parse", "--absolute-git-dir"],
    ))
}

fn assert_staging_residue_reclaimed(fixture: &Fixture, staging: &Path, handle: &RunHandle) {
    assert!(
        !staging.exists(),
        "the staging worktree was removed with force"
    );
    assert!(
        fixture.manager().intents().expect("intents").is_empty(),
        "the staging intent was reclaimed"
    );
    assert!(
        fixture
            .manager()
            .refs_under(&format!(
                "{}/{RUN_ID}/prepared/",
                crate::engine::topology::candidate::RUN_REF_ROOT
            ))
            .expect("refs")
            .is_empty(),
        "no pin was created for a sequence that never started"
    );
    assert!(handle.fold.transaction().is_none());
    assert_eq!(
        handle.fold.task_state(ALPHA),
        Some(TaskState::AwaitingMerge),
        "the candidate is still queued: nothing was recorded for the interrupted pick"
    );
    assert!(
        merged_sequences(fixture) == vec![0] && interrupted_sequences(fixture).is_empty(),
        "only BETA's publication is recorded"
    );
}

#[test]
fn synthetic_cherry_pick_residue_unreferenced_objects_and_cherry_pick_head_then_forced_reclaim_converges()
 {
    let fixture = Fixture::build(
        "synthetic-residue",
        Damage {
            two_tasks: true,
            ..Damage::default()
        },
    );
    let (planted, head) = plant_stale_queued_candidate(&fixture);
    let staging = plant_staging_worktree(&fixture, 1, head.as_str());
    let git_dir = worktree_git_dir(&staging);

    let orphan_file = fixture.root.join("orphan-bytes");
    crate::workspace_manager::fixture::write_file(&orphan_file, b"orphaned by a killed pick\n");
    let blob = crate::workspace_manager::fixture::git(
        &staging,
        &[
            "hash-object",
            "-w",
            orphan_file.to_str().expect("utf-8 scratch path"),
        ],
    );
    let tree = crate::workspace_manager::fixture::git(&staging, &["rev-parse", "HEAD^{tree}"]);
    let dangling = crate::workspace_manager::fixture::git(
        &staging,
        &[
            "commit-tree",
            &tree,
            "-p",
            head.as_str(),
            "-m",
            "upstroke: a proposal the pick never published",
        ],
    );
    for (name, content) in [
        ("CHERRY_PICK_HEAD", format!("{}\n", planted.commit)),
        ("MERGE_MSG", "upstroke: alpha attempt 1\n".to_owned()),
        ("index.lock", String::new()),
        ("sequencer/todo", format!("pick {} alpha\n", planted.commit)),
    ] {
        crate::workspace_manager::fixture::write_file(&git_dir.join(name), content.as_bytes());
    }

    let site = EffectSiteId::Object(ObjectSite::ProposalCherryPick);
    let target = crate::workspace_manager::ResidueTarget::new(&fixture.repo_root)
        .at(&staging)
        .from_base(head.as_str());
    assert_eq!(
        crate::workspace_manager::classify_object_residue(site, &target).expect("classified"),
        crate::topology::effects::ObjectResidue::Internal
    );
    let elements = crate::workspace_manager::observed_residue_elements(site, &target)
        .expect("the elements are observable");
    for element in [
        crate::topology::effects::ResidueElement::CherryPickHead,
        crate::topology::effects::ResidueElement::MergeMsg,
        crate::topology::effects::ResidueElement::IndexLock,
        crate::topology::effects::ResidueElement::SequencerState,
    ] {
        assert!(
            elements.contains(&element),
            "{element:?} was planted and not observed: {elements:?}"
        );
    }

    let (_, handle) = resume_with_real_refs(&fixture, &harness())
        .expect("the resume reclaims the residue rather than refusing");
    assert_staging_residue_reclaimed(&fixture, &staging, &handle);
    drop(handle);
    for object in [&blob, &dangling] {
        assert!(
            crate::workspace_manager::fixture::git_out(
                &fixture.repo_root,
                &["cat-file", "-e", object]
            )
            .status
            .success(),
            "an object the pick wrote is Git's once unreferenced, never deleted by recovery"
        );
    }

    let driven = drive(&fixture, &DriveSeams::default(), 1);
    assert!(
        matches!(
            driven.progress.first(),
            Some(Ok(Progress::Integrated {
                key: ALPHA,
                sequence: crate::topology::events::SequenceId(1),
                ..
            }))
        ),
        "the candidate integrates under the sequence the residue held: {:?}",
        driven.progress
    );
    let published =
        ref_target(&fixture, fixture.started.integration_ref.as_str()).expect("the ref moved");
    assert_eq!(
        crate::workspace_manager::fixture::git(
            &fixture.repo_root,
            &["rev-parse", &format!("{published}^")]
        ),
        head.0,
        "the fresh pick's proposal sits on the moved head, not on the residue"
    );
}

fn remove_git_ref_lock_residue(git_dir: &Path) -> Vec<PathBuf> {
    let mut removed = Vec::new();
    let packed = git_dir.join("packed-refs.lock");
    if packed.exists() {
        crate::workspace_manager::fixture::remove_file(&packed);
        removed.push(packed);
    }
    let mut stack = vec![git_dir.join("refs")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "lock")
            {
                crate::workspace_manager::fixture::remove_file(&path);
                removed.push(path);
            }
        }
    }
    removed
}

#[test]
fn sampled_cherry_pick_child_kills_every_residue_classified_and_recovered() {
    const SAMPLING_N: u32 = 8;
    let site = EffectSiteId::Object(ObjectSite::ProposalCherryPick);

    let two_tasks = || Damage {
        two_tasks: true,
        ..Damage::default()
    };
    let budget = {
        let probe = Fixture::build("sample-probe", two_tasks());
        let (planted, head) = plant_stale_queued_candidate(&probe);
        let staging = plant_staging_worktree(&probe, 1, head.as_str());
        let started = std::time::Instant::now();
        let output = crate::workspace_manager::fixture::git_out(
            &staging,
            &["cherry-pick", planted.commit.as_str()],
        );
        assert!(
            output.status.success(),
            "the probe pick must really run: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        started.elapsed().max(Duration::from_micros(200))
    };

    let mut observed = Vec::new();
    let mut refusals = Vec::new();
    let mut killed_while_running = 0_u32;
    for run in 0..SAMPLING_N {
        let fixture = Fixture::build(&format!("sample-{run}"), two_tasks());
        let (planted, head) = plant_stale_queued_candidate(&fixture);
        let staging = plant_staging_worktree(&fixture, 1, head.as_str());

        let mut child = crate::workspace_manager::fixture::KillableGitChild::spawn(
            &staging,
            &["cherry-pick".to_owned(), planted.commit.0.clone()],
        );
        std::thread::sleep(budget.mul_f64(f64::from(run + 1) / f64::from(SAMPLING_N + 1)));
        let running_at_kill = child.exited().is_none();
        child.kill();
        let status = child.wait();
        let died_by_kill = crate::workspace_manager::fixture::died_by_kill(&status);
        assert!(
            died_by_kill || status.success(),
            "run {run}: the child ended {status:?}, which is neither the kill's signature nor \
             a completed pick; the sample says nothing about a kill"
        );
        if died_by_kill {
            killed_while_running += 1;
        } else {
            assert!(
                !running_at_kill || child_outran_the_kill(&status),
                "run {run}: the child was running when the kill fired and yet exited \
                 {status:?}"
            );
        }
        let _ = remove_git_ref_lock_residue(&fixture.git_dir);

        let target = crate::workspace_manager::ResidueTarget::new(&fixture.repo_root)
            .at(&staging)
            .from_base(head.as_str());
        match crate::workspace_manager::classify_object_residue(site, &target) {
            Ok(class) => observed.push((class, status)),
            Err(error) => refusals.push(format!("run {run}: {error}")),
        }

        let (_, handle) = resume_with_real_refs(&fixture, &harness())
            .unwrap_or_else(|error| panic!("run {run}: the resume did not converge: {error}"));
        assert_staging_residue_reclaimed(&fixture, &staging, &handle);
        drop(handle);
        let driven = drive(&fixture, &DriveSeams::default(), 1);
        assert!(
            matches!(
                driven.progress.first(),
                Some(Ok(Progress::Integrated {
                    key: ALPHA,
                    sequence: crate::topology::events::SequenceId(1),
                    ..
                }))
            ),
            "run {run}: the candidate integrates after the reclaim under sequence 1: {:?}",
            driven.progress
        );
    }

    assert!(
        refusals.is_empty(),
        "the classifier refused {} of {SAMPLING_N} samples: {refusals:?}",
        refusals.len()
    );
    assert_eq!(
        observed.len(),
        SAMPLING_N as usize,
        "every sample was classified into one of the site's classes and recovered"
    );
    assert!(
        killed_while_running >= 1,
        "no sample died by the kill: the ladder's first rung fires at one ninth of a measured \
         pick, so a kill that reaches a running child cannot leave it exit 0, and a run in which \
         every child exited cleanly is a run in which nothing was killed — the evidence of \
         {SAMPLING_N} samples was of completed picks, not of kills: {observed:?}"
    );
}

fn child_outran_the_kill(status: &std::process::ExitStatus) -> bool {
    status.success()
}

#[test]
fn a_ref_lock_left_by_a_killed_compare_and_swap_refuses_resumably_until_removed() {
    let fixture = Fixture::healthy("cas-lock");
    let planted = plant_prepared_fast(&fixture);
    let lock = fixture
        .git_dir
        .join(fixture.started.integration_ref.as_str())
        .with_extension("lock");
    crate::workspace_manager::fixture::write_file(&lock, b"");
    let before = fixture.log_bytes();

    let locked = harness();
    let text = message(
        &resume_with_real_refs(&fixture, &locked)
            .expect_err("Git refuses the swap while the lock file exists"),
    );
    assert!(
        text.contains("integration.lock"),
        "the refusal names the lock Git could not take: {text}"
    );
    assert_eq!(
        cas_integration_entries(&locked),
        1,
        "the swap was attempted once, as T-FAST's resume action asks"
    );
    assert_eq!(
        ref_target(&fixture, fixture.started.integration_ref.as_str()).as_deref(),
        Some(fixture.base_sha.as_str()),
        "the ref is unchanged"
    );
    assert_eq!(
        fixture.log_bytes(),
        before,
        "nothing was appended: resumable"
    );
    assert!(
        lock.exists(),
        "recovery deleted a lock file no residue class authorizes it to"
    );

    crate::workspace_manager::fixture::remove_file(&lock);
    resume_with_real_refs(&fixture, &harness())
        .expect("with the lock gone the authorized publication completes");
    assert_eq!(merged_sequences(&fixture), vec![0]);
    assert_eq!(
        ref_target(&fixture, fixture.started.integration_ref.as_str()).as_deref(),
        Some(planted.commit.as_str())
    );
}

#[test]
fn a_call_census_needle_is_not_satisfied_by_a_longer_name_ending_in_it() {
    assert_eq!(
        crate::effects::census_domain::production_calls(
            "            .refuse_unexpected_refs(&namespace, &expected)?;\n",
            "expected_refs",
            crate::effects::census_domain::Call::Free
        ),
        0,
        "a longer identifier ending in the entry's name satisfied its census entry"
    );
    assert_eq!(
        crate::effects::census_domain::production_calls(
            "        let expected = crate::engine::topology::candidate::expected_refs(&r, f);\n",
            "expected_refs",
            crate::effects::census_domain::Call::Free
        ),
        1,
        "a genuine call through a path was rejected: `:` is not an identifier byte"
    );
    assert_eq!(
        crate::effects::census_domain::production_calls(
            "    let e = expected_refs(&run_id, fold);\n",
            "expected_refs",
            crate::effects::census_domain::Call::Free
        ),
        1,
        "a genuine bare call was rejected"
    );
    assert_eq!(
        crate::effects::census_domain::production_calls(
            "pub fn expected_refs(run_id: &str, fold: &TopologyFold) -> Vec<String> {\n",
            "expected_refs",
            crate::effects::census_domain::Call::Free
        ),
        0,
        "a definition is not a call: a function that calls only itself is what this census exists \
         to catch"
    );

    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source =
        std::fs::read_to_string(manifest.join("src/workspace_manager.rs")).expect("a source file");
    let moved = std::fs::read_to_string(manifest.join("src/workspace_manager/tests.rs"))
        .expect("a source file");
    let code = crate::effects::production_code(&source);
    let whole = source.matches("expected_refs(").count() + moved.matches("expected_refs(").count();
    let region = code.matches("expected_refs(").count();
    assert!(
        whole >= 4 && region >= 1,
        "`workspace_manager` no longer carries the substring this test is about ({whole} in \
         the module, {region} in the production region), so the zero below proves nothing"
    );
    assert_eq!(
        crate::effects::census_domain::production_calls(
            &code,
            "expected_refs",
            crate::effects::census_domain::Call::Free
        ),
        0,
        "the production region of `workspace_manager.rs` has {region} occurrence(s) of \
         `expected_refs(` and every one of them belongs to `refuse_unexpected_refs`; counting \
         them is how a census entry gets proved by a function that is not the one it names"
    );
}

#[test]
fn every_packet_named_recovery_action_has_a_production_caller() {
    const CLAUSES: &[(&str, crate::effects::census_domain::Call, &str)] = &[
        (
            "prune_orphan_pin",
            crate::effects::census_domain::Call::Free,
            "transaction_fault_matrix[T-CAND-OBJ].resume_action (b): delete the exact orphan pin \
             expected-old",
        ),
        (
            "refuse_unexpected_refs",
            crate::effects::census_domain::Call::Method,
            "transaction_fault_matrix[T-CAND-OBJ].refusal_condition: an unexpected ref under the \
             run namespace",
        ),
        (
            "expected_refs",
            crate::effects::census_domain::Call::Free,
            "the entitlement `refuse_unexpected_refs` refuses against, derived from the fold",
        ),
        (
            "finish_promotions",
            crate::effects::census_domain::Call::Free,
            "transaction_fault_matrix[T-CAND-REF].resume_action: verify, create the ref, append \
             task_candidate_created, prune the pin",
        ),
        (
            "recreate_open_no_attempt",
            crate::effects::census_domain::Call::Free,
            "transaction_fault_matrix[T-DISPATCH].resume_action: verify the worktree or recreate it",
        ),
        (
            "settle_interrupted",
            crate::effects::census_domain::Call::Free,
            "transaction_fault_matrix[T-ATTEMPT].resume_action: append attempt_interrupted",
        ),
        (
            "close_retained_idle",
            crate::effects::census_domain::Call::Free,
            "transaction_fault_matrix[T-RETAINED].resume_action: a fresh process closes it in \
             recovery",
        ),
        (
            "ensure_recorded_integration_ref",
            crate::effects::census_domain::Call::Free,
            "transaction_fault_matrix[T-RUNSTART].resume_action: P7/P8 create the ref zero-old at \
             the recorded base",
        ),
        (
            "finish_integration",
            crate::effects::census_domain::Call::Free,
            "transaction_fault_matrix[T-FAST/T-PREPARED/T-VERIFY].resume_action: complete an \
             authorized publication after the barrier, or settle a verification interrupted",
        ),
        (
            "resume_open_no_attempt",
            crate::effects::census_domain::Call::Free,
            "transaction_fault_matrix[T-DISPATCH].resume_action: continue attempt (no spend \
             repeats)",
        ),
    ];

    let mut test_files_skipped = 0_usize;
    let sources: Vec<(String, String)> = {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut all = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("src is readable") {
                let path = entry.expect("a directory entry").path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    all.push(path);
                }
            }
        }
        let test_modules = crate::effects::census_domain::whole_file_test_modules(&root, &all, 13);
        let mut out = Vec::new();
        {
            for path in all {
                if test_modules.contains(&path) {
                    test_files_skipped += 1;
                    continue;
                }
                let relative = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let source = std::fs::read_to_string(&path).expect("a source file");
                out.push((relative, crate::effects::production_code(&source)));
            }
        }
        out
    };
    assert!(
        sources.len() > 20,
        "the walk found {} sources, so its zero counts would prove nothing",
        sources.len()
    );
    assert!(
        test_files_skipped >= crate::effects::tests::cfg::WHOLE_FILE_TEST_MODULES.len()
            && sources.iter().all(|(rel, _)| !rel.ends_with("tests.rs")
                && !rel.ends_with("scaffold.rs")
                && !rel.ends_with("premove.rs")
                && !rel.ends_with("fake.rs")
                && !rel.ends_with("fixture.rs")
                && !rel.ends_with("scratch_tree.rs")
                && !rel.ends_with("readiness.rs")),
        "the out-of-line test modules are not being skipped ({test_files_skipped} skipped of all \
         the crate declares), so a fixture's call can satisfy a clause on production's behalf. \
         The six named here are the ones a file-name rule misses, and they are named rather \
         than counted because the count above cannot see a substitution: a skip set of the \
         right size that dropped one of these and gained an unrelated production file \
         satisfies it, and one of these carrying a needle then answers a census on \
         production's behalf"
    );

    let mut uncalled: Vec<String> = Vec::new();
    let mut undefined: Vec<String> = Vec::new();
    for (name, form, clause) in CLAUSES {
        let defined: usize = sources
            .iter()
            .map(|(_, code)| code.matches(&format!("fn {name}(")).count())
            .sum();
        if defined == 0 {
            undefined.push((*name).to_owned());
        }
        let calls: usize = sources
            .iter()
            .map(|(_, code)| crate::effects::census_domain::production_calls(code, name, *form))
            .sum();
        if calls == 0 {
            uncalled.push(format!("`{name}` performs `{clause}`"));
        }
    }

    assert!(
        undefined.is_empty(),
        "these are named as performing a packet clause and no production item of that name \
         exists, so the row below cannot fail for the right reason and has been passing on \
         somebody else's call sites: {undefined:?}"
    );

    assert!(
        uncalled.is_empty(),
        "these implement a packet clause and nothing in production calls them, so the clause is \
         not performed by any run — which is how this slice shipped a converged promotion that \
         stalled forever and a resumed run that forgot its spend:\n  {}",
        uncalled.join("\n  ")
    );
}

struct BlockNthSnapshotAdd {
    rest: HarnessTopologyHooks,
    blocked: PathBuf,
    at: usize,
    adds: usize,
}

impl crate::workspace_manager::EffectHooks for BlockNthSnapshotAdd {
    fn phase(&mut self, site: EffectSiteId, phase: HookPhase) -> Injection {
        if site == EffectSiteId::Snapshot(crate::topology::effects::SnapshotSite::Add)
            && phase == HookPhase::Before
        {
            self.adds += 1;
            if self.adds == self.at {
                crate::workspace_manager::fixture::write_file(
                    &self.blocked.join("occupied"),
                    b"a foreign non-empty directory",
                );
            }
        }
        Injection::Proceed
    }

    fn refusal_cause(&self) -> Option<String> {
        None
    }
}

impl TopologyHooks for BlockNthSnapshotAdd {
    fn effects(&mut self) -> &mut dyn crate::workspace_manager::EffectHooks {
        self
    }

    fn rundir(&mut self) -> &mut dyn crate::rundir::RunDirHooks {
        self.rest.rundir()
    }

    fn events(&mut self) -> &mut dyn crate::events::log::EventHooks {
        self.rest.events()
    }

    fn container(&mut self) -> &mut dyn crate::runner::container::ContainerHooks {
        self.rest.container()
    }

    fn spawn(&mut self) -> &mut dyn crate::agent::proc::SpawnHooks {
        self.rest.spawn()
    }
}

#[test]
fn a_completed_integration_review_is_charged_when_the_next_reviewers_snapshot_fails() {
    let fixture = Fixture::build(
        "paid-review-then-failed-snapshot",
        Damage {
            two_tasks: true,
            integration_second_opinion: true,
            ..Damage::default()
        },
    );
    plant_stale_verification(&fixture);
    let blocked = fixture
        .manager()
        .slot_path(&crate::workspace_manager::Slot::Snapshot {
            name: crate::workspace_manager::SnapshotName::integration_review(2, 1),
        });
    let mut hooks = BlockNthSnapshotAdd {
        rest: HarnessTopologyHooks::new(harness()),
        blocked,
        at: 3,
        adds: 0,
    };
    let driven = drive_hooked(
        &fixture,
        &DriveSeams {
            review_cost_usd: Some(2.5),
            run_ceiling_usd: Some(2.2),
            ..DriveSeams::default()
        },
        3,
        &mut hooks,
    );

    let unavailable = unavailable_terminals(&driven.log);
    assert_eq!(
        unavailable.len(),
        1,
        "the failed snapshot settles one unavailable terminal: {unavailable:?}"
    );
    assert!(
        matches!(
            driven.progress.first(),
            Some(Ok(crate::engine::topology::run::Progress::Unavailable {
                parked: false,
                ..
            }))
        ),
        "the second snapshot's Git error is an infrastructure deferral, not a park and not the \
         end of the command: {:?}",
        driven.progress
    );
    assert!(
        (driven.spend_after - driven.spend_before - 2.5).abs() < 1e-9,
        "the review that returned is charged live even though the judgement it belonged to \
         failed afterwards: {} -> {}",
        driven.spend_before,
        driven.spend_after
    );
    assert!(
        driven.spend_before < 2.2 && driven.spend_after > 2.2,
        "the ceiling sits between the replayed and the live total: {} < 2.2 < {}",
        driven.spend_before,
        driven.spend_after
    );
    assert_eq!(
        driven.reviewer_models.len(),
        1,
        "one review crossed the ceiling, so this incarnation admits no further review: {:?}",
        driven.reviewer_models
    );
}

#[test]
fn a_reviewer_whose_process_never_started_is_a_runner_spawn_failure() {
    use crate::error::ProcessFate;
    use crate::topology::events::{InfrastructureKind, UnavailableCause, UnavailableOutcome};

    let cause = |tag: &str, fate: ProcessFate| {
        let fixture = Fixture::build(
            tag,
            Damage {
                two_tasks: true,
                ..Damage::default()
            },
        );
        plant_stale_verification(&fixture);
        let driven = drive(
            &fixture,
            &DriveSeams {
                review_fails: Some(fate),
                ..DriveSeams::default()
            },
            1,
        );
        let terminals = unavailable_terminals(&driven.log);
        assert_eq!(
            terminals.len(),
            1,
            "the reviewer's runner failure settles exactly one terminal: {terminals:?}"
        );
        let terminal = terminals
            .first()
            .expect("the terminal just counted")
            .clone();
        assert!(
            matches!(terminal.outcome, UnavailableOutcome::Deferred { .. }),
            "containment and deferral are unchanged by the attribution: {:?}",
            terminal.outcome
        );
        terminal.cause
    };

    assert_eq!(
        cause("reviewer-never-started", ProcessFate::NeverStarted),
        UnavailableCause::Infrastructure {
            kind: InfrastructureKind::RunnerSpawnFailure
        },
        "a reviewer's process that was never started is the runner's failure, not the \
         reviewer's answer"
    );
    assert_eq!(
        cause("reviewer-gone", ProcessFate::Gone),
        UnavailableCause::Infrastructure {
            kind: InfrastructureKind::ReviewUnavailable
        },
        "and a reviewer whose process started and is now gone is still an unavailable review: \
         INV-23 names the never-started fate and no other"
    );
}

#[test]
fn a_dependent_task_is_dispatched_into_its_dependencys_merged_work() {
    let fixture = Fixture::build(
        "dependent-dispatch",
        Damage {
            two_tasks: true,
            beta_depends_on_alpha: true,
            ..Damage::default()
        },
    );
    let planted = plant_queued_candidate(&fixture);
    let planned = TopologyFold::replay(
        fixture.inputs(),
        &TopologyFold::parse_log(&fixture.log_bytes()).expect("the planted log parses"),
    )
    .expect("the planted log replays");
    assert!(
        !planned.ready(BETA),
        "beta waits on alpha, so its dispatch is the one that follows a publication"
    );

    let driven = drive(&fixture, &DriveSeams::default(), 2);

    assert!(
        matches!(
            driven.progress.first(),
            Some(Ok(Progress::Integrated { key: ALPHA, .. }))
        ),
        "the first step publishes alpha: {:?}",
        driven.progress
    );
    assert_eq!(
        ref_target(&fixture, fixture.started.integration_ref.as_str()).as_deref(),
        Some(planted.commit.as_str()),
        "and the integration ref carries that publication"
    );

    let worker = driven
        .runs
        .iter()
        .find(|run| {
            matches!(
                run.invocation,
                crate::runner::InvocationId::Attempt {
                    key: BETA,
                    role: crate::runner::invocation::AttemptRole::Worker,
                    ..
                }
            )
        })
        .expect("the second step starts beta's worker");
    assert_eq!(
        worker.checkout.get("candidate.txt").map(String::as_str),
        Some("the candidate edit\n"),
        "beta's agent reads what alpha merged, in the worktree it was handed: DESIGN §26 \
         verdict 1 dispatches at the run's integration head at dispatch, and beta is the first \
         task this run dispatches after a publication moved that head. The checkout held {:?}",
        worker.checkout.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        worker.head.as_deref(),
        Some(planted.commit.as_str()),
        "which is the head the log authorizes and not the base the run started at ({})",
        fixture.base_sha.as_str()
    );

    let dispatched = driven
        .log
        .iter()
        .find_map(|event| match &event.body {
            TopologyEventBody::TaskDispatched { data } if data.key == BETA => Some(data),
            _ => None,
        })
        .expect("beta's dispatch is durable");
    assert_eq!(
        dispatched.base_sha, planted.commit,
        "and the durable record names the base the worktree was actually created at, which is \
         what a resume rebuilds it from"
    );

    let events = TopologyFold::parse_log(&fixture.log_bytes()).expect("the result log parses");
    let once = TopologyFold::replay(fixture.inputs(), &events).expect("replays");
    let twice = TopologyFold::replay(fixture.inputs(), &events).expect("replays again");
    assert_eq!(once.state(), twice.state(), "replay twice equal");
}

#[test]
fn a_dispatch_recorded_before_this_rule_resumes_at_the_base_it_recorded() {
    let fixture = Fixture::two_tasks("old-log-dispatch-base");
    let published = plant_published_beta(&fixture);
    append_events(&fixture, &[dispatched_at(&fixture.base_sha)]);
    assert_ne!(
        published, fixture.base_sha,
        "the publication moved the head away from the run's starting base"
    );

    let old_log = TopologyFold::parse_log(&fixture.log_bytes()).expect("the old log parses");
    let replayed = TopologyFold::replay(fixture.inputs(), &old_log).expect("the old log replays");
    assert_eq!(
        replayed
            .task(ALPHA)
            .and_then(|task| task.generations.first())
            .map(|generation| generation.base_sha.clone()),
        Some(fixture.base_sha.clone()),
        "the fold takes the open generation's base from `task_dispatched`, which is where a log \
         written before the dispatch rule changed put the run's starting base"
    );

    let driven = drive(&fixture, &DriveSeams::default(), 1);

    assert_eq!(
        ref_target(&fixture, fixture.started.integration_ref.as_str()).as_deref(),
        Some(published.as_str()),
        "the integration head is elsewhere throughout"
    );
    let worker = driven
        .runs
        .iter()
        .find(|run| {
            matches!(
                run.invocation,
                crate::runner::InvocationId::Attempt {
                    key: ALPHA,
                    role: crate::runner::invocation::AttemptRole::Worker,
                    ..
                }
            )
        })
        .expect("the step continues alpha's open generation");
    assert_eq!(
        worker.head.as_deref(),
        Some(fixture.base_sha.as_str()),
        "a resume rebuilds the worktree at the base the dispatch recorded, not at the head that \
         has since been published: the durable record decides, so an old log replays to the \
         decisions it recorded"
    );
    assert!(
        !worker.checkout.contains_key("other.txt"),
        "and nothing merged after that dispatch appears in it: {:?}",
        worker.checkout.keys().collect::<Vec<_>>()
    );

    let dispatches: Vec<&CommitSha> = driven
        .log
        .iter()
        .filter_map(|event| match &event.body {
            TopologyEventBody::TaskDispatched { data } if data.key == ALPHA => Some(&data.base_sha),
            _ => None,
        })
        .collect();
    assert_eq!(
        dispatches,
        vec![&fixture.base_sha],
        "a continuation appends no second `task_dispatched` and rewrites no base"
    );

    let events = TopologyFold::parse_log(&fixture.log_bytes()).expect("the result log parses");
    let once = TopologyFold::replay(fixture.inputs(), &events).expect("replays");
    let twice = TopologyFold::replay(fixture.inputs(), &events).expect("replays again");
    assert_eq!(once.state(), twice.state(), "replay twice equal");
}

// ---------------------------------------------------------------------------
// PR9: a repair across a process boundary.
// ---------------------------------------------------------------------------

const REPAIR_MATERIALIZE: EffectSiteId = EffectSiteId::Object(ObjectSite::RepairMaterialize);

/// The repair's dispatch as the creator would have appended it: at the head
/// that rejected the candidate, inside its root's lineage lease, naming the
/// rejected candidate as its source.
fn repair_dispatched(
    rejection: &crate::topology::events::MergeRejected,
    key: TaskKey,
    generation: GenerationId,
) -> TopologyEventBody {
    repair_dispatched_in(rejection, key, generation, ALPHA)
}

fn repair_dispatched_in(
    rejection: &crate::topology::events::MergeRejected,
    key: TaskKey,
    generation: GenerationId,
    root: TaskKey,
) -> TopologyEventBody {
    TopologyEventBody::TaskDispatched {
        data: TaskDispatched {
            key,
            generation,
            base_sha: rejection.rejecting_head.clone(),
            worktree_path: format!("wt/repair-g{}", generation.0),
            lease: LeaseGrant::InheritedLineage { root },
            source_candidate: Some(rejection.candidate.clone()),
        },
    }
}

/// A repair's first attempt at generation 0: the root's one rung, and the
/// observation a lineage member's attempt must record.
fn repair_attempt_started(key: TaskKey, attempt: u32) -> TopologyEventBody {
    TopologyEventBody::AttemptStarted {
        data: AttemptStarted4 {
            key,
            generation: GEN,
            attempt: AttemptNumber(attempt),
            rung: 0,
            binding: RungBinding {
                tier: Tier::Mid,
                agent: AGENT.to_owned(),
                model: "claude-opus-5".to_owned(),
                pinned: false,
                effort: Effort::High,
            },
            pool: None,
            resume_session: None,
            materialization_observed: Some(crate::topology::events::Materialization::Clean),
        },
    }
}

fn retained_by_the_creator(session: &str) -> AttemptSettlement {
    AttemptSettlement::Retained {
        retained_session: SessionId(session.to_owned()),
        retained_incarnation: Epoch(0),
    }
}

/// The repair's worktree as the killed creator left it: registered at the
/// slot the manager derives, at the recorded base, with nothing done in it.
fn plant_repair_worktree(fixture: &Fixture, key: TaskKey, base: &str) -> PathBuf {
    plant_task_worktree(fixture, key, base)
}

/// Any task's generation-0 worktree as a dead incarnation left it: intent
/// written, worktree added at `base`, nothing done in it.
fn plant_task_worktree(fixture: &Fixture, key: TaskKey, base: &str) -> PathBuf {
    let manager = fixture.manager();
    let slot = crate::engine::topology::dispatch::task_slot(key, GEN);
    manager
        .write_intent(&mut crate::workspace_manager::NoHooks, &slot)
        .expect("the repair's intent");
    manager
        .add_worktree(&mut crate::workspace_manager::NoHooks, &slot, base)
        .expect("the repair's worktree")
}

fn repair_dispatches(log: &[TopologyEvent], key: TaskKey) -> Vec<&TaskDispatched> {
    log.iter()
        .filter_map(|event| match &event.body {
            TopologyEventBody::TaskDispatched { data } if data.key == key => Some(data),
            _ => None,
        })
        .collect()
}

fn attempt_starts_of(log: &[TopologyEvent], key: TaskKey) -> Vec<&AttemptStarted4> {
    log.iter()
        .filter_map(|event| match &event.body {
            TopologyEventBody::AttemptStarted { data } if data.key == key => Some(data),
            _ => None,
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
enum InterruptedRepairPrefix {
    /// `task_dispatched` durable, the process killed before `Worktree.Add`.
    NoWorktree,
    /// The worktree added and the materialization killed inside its message
    /// write: the held `MERGE_MSG.lock`, the form the kill sampler first found.
    HeldMessageLock,
    /// The materialization completed, its state files cleared by the funnel,
    /// and the process killed before `attempt_started`: a merged index at the
    /// base, which the quiescence rule reads as reusable.
    CompletedMaterialization,
}

/// `T-REPAIR-DISPATCH`: whatever the kill left between the repair's dispatch
/// and its first attempt, recovery (g) verifies the worktree at its recorded
/// base, recreates it when the verification fails, and materializes nothing
/// (`R6`); the resumed loop continues the same generation and materializes
/// exactly once — onto a fresh worktree, or as a no-op onto the index a
/// completed pick already left — so the observation recorded before the spawn
/// is the one the attempt ran on.
#[test]
fn a_repair_dispatch_interrupted_before_its_attempt_is_recreated_at_its_base_and_materialized_once()
{
    use crate::engine::topology::dispatch::Reuse;
    use crate::topology::effects::ResidueElement;
    use crate::workspace_manager::VerifyFailure;

    for prefix in [
        InterruptedRepairPrefix::NoWorktree,
        InterruptedRepairPrefix::HeldMessageLock,
        InterruptedRepairPrefix::CompletedMaterialization,
    ] {
        let fixture = Fixture::build(
            &format!("repair-prefix-{prefix:?}"),
            Damage {
                two_tasks: true,
                ..Damage::default()
            },
        );
        let (rejection, repair) = plant_rejected_repair(&fixture);
        assert!(
            matches!(
                rejection.repair.admission,
                crate::topology::events::SpawnAdmission::Runnable
            ),
            "{prefix:?}: one automatic repair is allowed, so the first is runnable"
        );
        append_events(&fixture, &[repair_dispatched(&rejection, repair, GEN)]);
        let head = rejection.rejecting_head.clone();
        let slot = crate::engine::topology::dispatch::task_slot(repair, GEN);
        let worktree = fixture.manager().slot_path(&slot);
        match prefix {
            InterruptedRepairPrefix::NoWorktree => {
                assert!(!worktree.exists());
            }
            InterruptedRepairPrefix::HeldMessageLock => {
                let worktree = plant_repair_worktree(&fixture, repair, head.as_str());
                crate::workspace_manager::fixture::write_file(
                    &worktree_git_dir(&worktree).join("MERGE_MSG.lock"),
                    b"side\n",
                );
            }
            InterruptedRepairPrefix::CompletedMaterialization => {
                plant_repair_worktree(&fixture, repair, head.as_str());
                fixture
                    .manager()
                    .repair_materialize(
                        &mut crate::workspace_manager::NoHooks,
                        &slot,
                        rejection.candidate.commit_sha.as_str(),
                    )
                    .expect("the creator's materialization completed");
            }
        }

        let harness = harness();
        let mut hooks = HarnessTopologyHooks::new(Arc::clone(&harness));
        let (recovered, handle) =
            resume_as(&fixture, RESUMER, &runtime_holding_the_record(), &mut hooks)
                .unwrap_or_else(|error| panic!("{prefix:?}: the resume converges: {error}"));
        let expected_reuse = match prefix {
            InterruptedRepairPrefix::NoWorktree => Reuse::Recreated {
                failure: VerifyFailure::NotRegistered,
            },
            InterruptedRepairPrefix::HeldMessageLock => Reuse::Recreated {
                failure: VerifyFailure::Residue(ResidueElement::MergeMsg),
            },
            InterruptedRepairPrefix::CompletedMaterialization => Reuse::Verified,
        };
        assert_eq!(
            recovered.recreated,
            vec![(repair, GEN, expected_reuse)],
            "{prefix:?}: (g) acts on exactly the repair's open generation: recreated when \
             the kill left it non-quiescent, reused when the completed pick left it at its \
             base with no state file"
        );
        assert_eq!(
            crate::workspace_manager::fixture::git(&worktree, &["rev-parse", "HEAD"]),
            head.0,
            "{prefix:?}: at its recorded base, the head that rejected the candidate"
        );
        assert_eq!(
            harness
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .count(REPAIR_MATERIALIZE, HookPhase::Before),
            0,
            "{prefix:?}: recovery materializes nothing (`R6`): a recreated repair worktree is \
             at its base and the continuation is what materializes it"
        );

        let seams = DriveSeams::default();
        let runner = driven_runner(&seams);
        let driven = drive_handle(&fixture, handle, &seams, 2, &runner, &mut hooks);
        assert!(
            matches!(
                driven.progress.first(),
                Some(Ok(Progress::Settled {
                    key,
                    accepted: true,
                    ..
                })) if *key == repair
            ),
            "{prefix:?}: the loop continues the recreated generation through its first attempt \
             to a candidate: {:?}",
            driven.progress
        );
        assert!(
            matches!(
                driven.progress.get(1),
                Some(Ok(Progress::Integrated {
                    key,
                    sequence: crate::topology::events::SequenceId(2),
                    ..
                })) if *key == repair
            ),
            "{prefix:?}: and the lineage candidate merges: {:?}",
            driven.progress
        );
        let dispatches = repair_dispatches(&driven.log, repair);
        assert_eq!(
            dispatches.len(),
            1,
            "{prefix:?}: `T-REPAIR-DISPATCH` continues the dispatched generation; a second \
             dispatch would be a fresh generation the kill did not earn"
        );
        let starts = attempt_starts_of(&driven.log, repair);
        assert_eq!(starts.len(), 1, "{prefix:?}: one attempt");
        assert_eq!(
            starts[0].materialization_observed,
            Some(crate::topology::events::Materialization::Clean),
            "{prefix:?}: the observation recorded before the spawn is the continuation's"
        );
        assert_eq!(
            harness
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .count(REPAIR_MATERIALIZE, HookPhase::Before),
            1,
            "{prefix:?}: the continuation materializes exactly once, whatever the kill left"
        );
        let fold = replayed(&fixture);
        assert_eq!(
            fold.task_state(ALPHA),
            Some(TaskState::Merged),
            "{prefix:?}"
        );
        assert_eq!(
            fold.task_state(repair),
            Some(TaskState::Merged),
            "{prefix:?}"
        );
        assert!(driven.invocations_balance, "{prefix:?}");
        assert_eq!(driven.entitlements_held, 0, "{prefix:?}");
    }
}

/// `ST-11` for a repair: a fresh incarnation closes the retained generation
/// with `LineageHeld` (the lineage keeps its region; the generation held no
/// lease of its own) and the loop opens the next generation, which is
/// materialized again from the same recorded source.
#[test]
fn a_fresh_incarnation_closes_a_retained_repair_generation_lineage_held_and_the_next_materializes_again()
 {
    let fixture = Fixture::build(
        "retained-repair",
        Damage {
            two_tasks: true,
            ..Damage::default()
        },
    );
    let (rejection, repair) = plant_rejected_repair(&fixture);
    let retained_worktree =
        plant_repair_worktree(&fixture, repair, rejection.rejecting_head.as_str());
    let retained_slot = crate::engine::topology::dispatch::task_slot(repair, GEN);
    append_events(
        &fixture,
        &[
            repair_dispatched(&rejection, repair, GEN),
            repair_attempt_started(repair, 1),
            for_task(
                repair,
                "repair",
                attempt_finished(1, retained_by_the_creator("the-repairs-session")),
            ),
        ],
    );
    assert!(
        replayed(&fixture).ready_retry(repair),
        "the retained repair generation is retryable by its own incarnation, or the close \
         below closes nothing"
    );
    assert!(
        retained_worktree.exists()
            && fixture
                .manager()
                .intents()
                .expect("intents")
                .contains(&retained_slot),
        "the dead incarnation left the retained generation's worktree and intent, or the \
         reclaim below reclaims nothing"
    );

    let harness = harness();
    let mut hooks = HarnessTopologyHooks::new(Arc::clone(&harness));
    let (recovered, handle) =
        resume_as(&fixture, RESUMER, &runtime_holding_the_record(), &mut hooks)
            .expect("a run with a retained repair session resumes");
    assert_eq!(
        recovered.retained_closed, 1,
        "(e) closes the retained repair generation"
    );
    assert!(
        !retained_worktree.exists(),
        "PR #249's crash review, finding 2: a generation (e) closes owns nothing (R9), so \
         its worktree is reclaimed with the close rather than kept for the life of the run"
    );
    assert!(
        !fixture
            .manager()
            .intents()
            .expect("intents")
            .contains(&retained_slot),
        "and its durable intent with it"
    );
    let closed = TopologyFold::parse_log(&fixture.log_bytes())
        .expect("parses")
        .into_iter()
        .find_map(|event| match event.body {
            TopologyEventBody::GenerationClosed { data } if data.key == repair => Some(data),
            _ => None,
        })
        .expect("the repair's generation is closed");
    assert_eq!(
        closed.reason,
        crate::topology::events::GenerationCloseReason::ResumeDiscardsRetainedSession
    );
    assert_eq!(
        closed.lease,
        crate::topology::events::LeaseDisposition::LineageHeld,
        "a lineage member's generation closes with the lineage still holding its region"
    );
    assert!(
        !replayed(&fixture).ready_retry(repair),
        "the retained session is gone with the incarnation that held it"
    );

    let seams = DriveSeams::default();
    let runner = driven_runner(&seams);
    let driven = drive_handle(&fixture, handle, &seams, 2, &runner, &mut hooks);
    assert!(
        matches!(
            driven.progress.first(),
            Some(Ok(Progress::Settled {
                key,
                accepted: true,
                ..
            })) if *key == repair
        ),
        "the next generation of the repair runs to a candidate: {:?}",
        driven.progress
    );
    assert!(
        matches!(
            driven.progress.get(1),
            Some(Ok(Progress::Integrated { key, .. })) if *key == repair
        ),
        "and merges: {:?}",
        driven.progress
    );
    let dispatches = repair_dispatches(&driven.log, repair);
    assert_eq!(
        dispatches
            .iter()
            .map(|dispatched| dispatched.generation)
            .collect::<Vec<_>>(),
        vec![GEN, GenerationId(1)],
        "a closed generation is never recreated; the repair opens its next one"
    );
    for dispatched in &dispatches {
        assert_eq!(
            dispatched.source_candidate.as_ref(),
            Some(&rejection.candidate),
            "every generation of the repair is materialized from the rejected candidate"
        );
        assert_eq!(
            dispatched.lease,
            LeaseGrant::InheritedLineage { root: ALPHA }
        );
    }
    let starts = attempt_starts_of(&driven.log, repair);
    assert_eq!(
        starts.len(),
        2,
        "the planted attempt and the fresh generation's"
    );
    let fresh = starts[1];
    assert_eq!(fresh.generation, GenerationId(1));
    assert_eq!(fresh.attempt, AttemptNumber(1));
    assert_eq!(
        fresh.resume_session, None,
        "a fresh generation resumes no session: the retained one died with its incarnation"
    );
    assert_eq!(
        fresh.materialization_observed,
        Some(crate::topology::events::Materialization::Clean),
        "and it is materialized again, observing the pick onto its base"
    );
    assert_eq!(
        harness
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .count(REPAIR_MATERIALIZE, HookPhase::Before),
        1,
        "the closed generation is never re-materialized; the fresh one is, once"
    );
    let fold = replayed(&fixture);
    assert_eq!(fold.task_state(ALPHA), Some(TaskState::Merged));
    assert_eq!(fold.task_state(repair), Some(TaskState::Merged));
    assert!(driven.invocations_balance);
    assert_eq!(driven.entitlements_held, 0);
    let (again, _handle) = resume_as(
        &fixture,
        "01KZTCCCCCCCCCCCCCCCCCCCCC",
        &runtime_holding_the_record(),
        &mut hooks,
    )
    .expect("another complete resume after the replacement generation merged");
    assert_eq!(
        again.retained_closed, 0,
        "the prior generation is already closed in the log"
    );
    assert!(
        !retained_worktree.exists()
            && !fixture
                .manager()
                .intents()
                .expect("intents")
                .contains(&retained_slot),
        "the closed generation stays reclaimed across a later resume"
    );
}

/// `R11` across a budget stop: the rejected candidate's ref is what keeps the
/// repair's source reachable, the stop and the resume prune nothing of it, and
/// the repair dispatches from it once the next epoch opens.
#[test]
fn a_rejected_candidates_ref_survives_a_budget_stop_and_the_repair_dispatches_after_the_resume() {
    let fixture = Fixture::build(
        "budget-lineage",
        Damage {
            two_tasks: true,
            ..Damage::default()
        },
    );
    let (rejection, repair) = plant_rejected_repair(&fixture);
    append_events(
        &fixture,
        &[TopologyEventBody::BudgetExceeded {
            data: BudgetExceeded4 {
                epoch: Epoch(0),
                budget: BudgetKind::Run,
                limit_usd: 1.0,
                spent_usd: 2.0,
                key: Some(repair),
            },
        }],
    );
    let candidate_ref = rejection.candidate.candidate_ref.clone();
    assert!(
        replayed(&fixture).budget_stop().is_some(),
        "the fixture must carry a stop, or this test proves nothing"
    );
    assert_eq!(
        ref_target(&fixture, candidate_ref.as_str()).as_deref(),
        Some(rejection.candidate.commit_sha.as_str()),
        "the rejected candidate is protected by its candidates ref when the run stops"
    );

    let driven = drive(&fixture, &DriveSeams::default(), 1);
    assert!(
        matches!(
            driven.progress.first(),
            Some(Ok(Progress::Settled {
                key,
                accepted: true,
                ..
            })) if *key == repair
        ),
        "the resume opens the next epoch and the repair dispatches from the protected \
         candidate: {:?}",
        driven.progress
    );
    assert_eq!(
        ref_target(&fixture, candidate_ref.as_str()).as_deref(),
        Some(rejection.candidate.commit_sha.as_str()),
        "and the ref is still there afterwards: it is pruned only by finalization"
    );
    let dispatches = repair_dispatches(&driven.log, repair);
    assert_eq!(dispatches.len(), 1);
    assert_eq!(
        dispatches[0].source_candidate.as_ref(),
        Some(&rejection.candidate)
    );
    assert!(replayed(&fixture).budget_stop().is_none());
}

fn answers_of(
    log: &[TopologyEvent],
    key: TaskKey,
) -> Vec<&crate::topology::events::QuestionAnswered4> {
    log.iter()
        .filter_map(|event| match &event.body {
            TopologyEventBody::QuestionAnswered { data } if data.key == key => Some(data),
            _ => None,
        })
        .collect()
}

/// `ST-12` at the loop: a repair whose Mid floor intersects its root's frozen
/// ladder empty is admitted `HumanBinding`; the person names an agent, the
/// answer carries the one-off binding derived once at ingest (E2, `R1`/`R2`:
/// the repair ladder's floor, pinned, the catalogue's lowest model at or above
/// it, the policy's effort for that tier), and the repair runs under exactly
/// that binding.
#[test]
fn a_one_off_binding_answer_activates_a_repair_no_frozen_rung_can_run() {
    let fixture = Fixture::build(
        "one-off-binding",
        Damage {
            two_tasks: true,
            small_only: true,
            ..Damage::default()
        },
    );
    let (rejection, repair) = plant_rejected_repair(&fixture);
    let crate::topology::events::SpawnAdmission::HumanBinding { options, question } =
        &rejection.repair.admission
    else {
        panic!(
            "a Small-only root leaves the repair's Mid floor with no rung: {:?}",
            rejection.repair.admission
        );
    };
    assert_eq!(
        options,
        &vec![AGENT.to_owned()],
        "the options are the root's allowed agents"
    );

    let driven = drive(
        &fixture,
        &DriveSeams {
            answer: Some(crate::ir::Answer::Answered {
                text: AGENT.to_owned(),
            }),
            ..DriveSeams::default()
        },
        3,
    );
    assert!(
        matches!(
            driven.progress.first(),
            Some(Ok(Progress::Answered {
                key,
                declined: false,
                ..
            })) if *key == repair
        ),
        "step 1 ingests the binding answer: {:?}",
        driven.progress
    );
    assert!(
        matches!(
            driven.progress.get(1),
            Some(Ok(Progress::Settled {
                key,
                accepted: true,
                ..
            })) if *key == repair
        ),
        "step 2 runs the repair under the one-off binding to a candidate: {:?}",
        driven.progress
    );
    assert!(
        matches!(
            driven.progress.get(2),
            Some(Ok(Progress::Integrated { key, .. })) if *key == repair
        ),
        "step 3 merges the lineage: {:?}",
        driven.progress
    );

    let answers = answers_of(&driven.log, repair);
    assert_eq!(answers.len(), 1);
    assert_eq!(
        answers[0].answer,
        crate::topology::events::Answer4::Answered {
            option_index: 0,
            binding_override: Some(crate::topology::events::BindingOverride {
                key: repair,
                question: question.id.clone(),
                option_index: 0,
                agent: AGENT.to_owned(),
                model: "claude-sonnet-4-5".to_owned(),
                effort: Effort::High,
            }),
        },
        "the answer carries the five-field override, derived once at ingest: the \
         catalogue's lowest `{AGENT}` model at or above the repair ladder's Mid floor and \
         the policy's Mid effort"
    );
    let starts = attempt_starts_of(&driven.log, repair);
    assert_eq!(starts.len(), 1);
    assert_eq!(starts[0].rung, 0);
    assert_eq!(
        starts[0].binding,
        RungBinding {
            tier: Tier::Mid,
            agent: AGENT.to_owned(),
            model: "claude-sonnet-4-5".to_owned(),
            pinned: true,
            effort: Effort::High,
        },
        "the attempt runs under the override at the floor, pinned: not under the root's \
         Small rung, which the repair's floor excludes"
    );
    assert_eq!(
        starts[0].materialization_observed,
        Some(crate::topology::events::Materialization::Clean)
    );
    let fold = replayed(&fixture);
    assert_eq!(fold.task_state(ALPHA), Some(TaskState::Merged));
    assert_eq!(fold.task_state(repair), Some(TaskState::Merged));
    assert!(driven.invocations_balance);
    assert_eq!(driven.entitlements_held, 0);
}

/// PR #249's conformance review, finding 1: with two agents offered, the
/// person picks the second by number at the production parser. That is a
/// one-off binding to the second agent — `option_index: 1`, the catalogue's
/// lowest model for it at or above the floor — and not a decline of the
/// lineage, which is what the parser's last-option rule made of it.
#[test]
fn picking_the_last_of_two_offered_agents_binds_the_repair_to_it_rather_than_declining() {
    let fixture = Fixture::build(
        "one-off-binding-second-agent",
        Damage {
            two_tasks: true,
            small_only: true,
            integration_second_opinion: true,
            ..Damage::default()
        },
    );
    let (_rejection, repair) = plant_rejected_repair(&fixture);
    let fold = replayed(&fixture);
    let open = fold
        .open_questions()
        .expect("started")
        .values()
        .find(|open| open.question.key == repair)
        .expect("the repair's admission question is open")
        .clone();
    let second_agent = crate::engine::topology::scaffold::REVIEW_AGENT;
    assert_eq!(
        open.question.options,
        vec![AGENT.to_owned(), second_agent.to_owned()],
        "the options are the root's two allowed agents, or this test proves nothing"
    );
    let rendered = crate::ir::Question {
        id: open.question.id.clone(),
        kind: open.question.kind,
        affected_tasks: Vec::new(),
        context: open.question.context.clone(),
        options: open.question.options.clone(),
    };
    let typed = crate::interaction::interpret(&rendered, "2\n");
    assert_eq!(
        typed,
        crate::ir::Answer::Answered {
            text: second_agent.to_owned()
        },
        "`2` at the prompt is the second agent, not a decline"
    );

    let driven = drive(
        &fixture,
        &DriveSeams {
            answer: Some(typed),
            ..DriveSeams::default()
        },
        3,
    );
    assert!(
        matches!(
            driven.progress.first(),
            Some(Ok(Progress::Answered {
                key,
                declined: false,
                ..
            })) if *key == repair
        ),
        "step 1 ingests a binding, not a decline: {:?}",
        driven.progress
    );
    let answers = answers_of(&driven.log, repair);
    assert_eq!(answers.len(), 1);
    let crate::topology::events::Answer4::Answered {
        option_index,
        binding_override: Some(binding),
    } = &answers[0].answer
    else {
        panic!("the answer is a one-off binding: {:?}", answers[0].answer);
    };
    assert_eq!(*option_index, 1, "the second option");
    assert_eq!(binding.option_index, 1);
    assert_eq!(binding.agent, second_agent);
    assert_eq!(
        binding.model,
        crate::catalog::CATALOG
            .iter()
            .filter(|entry| entry.agent == second_agent && entry.tier >= Tier::Mid)
            .min_by_key(|entry| entry.tier)
            .map(|entry| entry.model.to_owned())
            .expect("the catalogue has a model for the second agent at or above Mid"),
        "the catalogue's lowest model for the chosen agent at or above the floor"
    );
    assert!(
        matches!(
            driven.progress.get(1),
            Some(Ok(Progress::Settled {
                key,
                accepted: true,
                ..
            })) if *key == repair
        ),
        "step 2 runs the repair under the second agent: {:?}",
        driven.progress
    );
    let starts = attempt_starts_of(&driven.log, repair);
    assert_eq!(starts.len(), 1);
    assert_eq!(starts[0].binding.agent, second_agent);
    assert!(starts[0].binding.pinned);
    assert_eq!(starts[0].binding.tier, Tier::Mid);
    let fold = replayed(&fixture);
    assert_eq!(
        fold.task_state(ALPHA),
        Some(TaskState::Merged),
        "the lineage merges instead of failing: {:?}",
        driven.progress
    );
    assert_eq!(fold.task_state(repair), Some(TaskState::Merged));
    assert!(driven.invocations_balance);
    assert_eq!(driven.entitlements_held, 0);
}

/// A one-off binding activates only through an option the spawn froze: text
/// that names no option is refused before anything is appended.
#[test]
fn a_binding_answer_naming_no_frozen_option_is_refused_before_any_append() {
    let fixture = Fixture::build(
        "one-off-binding-refused",
        Damage {
            two_tasks: true,
            small_only: true,
            ..Damage::default()
        },
    );
    let (_rejection, repair) = plant_rejected_repair(&fixture);
    let kinds_before = durable_kinds(&fixture);
    let driven = drive(
        &fixture,
        &DriveSeams {
            answer: Some(crate::ir::Answer::Answered {
                text: "gpt-5".to_owned(),
            }),
            ..DriveSeams::default()
        },
        1,
    );
    let refusal = match driven.progress.first() {
        Some(Err(error)) => error.to_string(),
        other => panic!("an answer naming no option was not refused: {other:?}"),
    };
    assert!(
        refusal.contains("none of the") && refusal.contains(&format!("task {}", repair.0)),
        "the refusal names the task and the offered options: {refusal}"
    );
    let mut expected = kinds_before;
    expected.push("run_resumed".to_owned());
    assert_eq!(
        durable_kinds(&fixture),
        expected,
        "nothing beyond the resume's own record was appended"
    );
    assert_eq!(
        replayed(&fixture).task_state(repair),
        Some(TaskState::AwaitingInput),
        "the question is still open"
    );
}

/// Declining a repair's admission question fails its lineage: the repair and
/// its root are terminal, the lineage lease is released, and with
/// `halts_run` the decline is also the run's halt.
#[test]
fn declining_a_repairs_admission_fails_its_lineage_and_halts_the_run_only_when_asked_to() {
    for halts_run in [false, true] {
        let fixture = Fixture::build(
            &format!("decline-lineage-halts-{halts_run}"),
            Damage {
                two_tasks: true,
                small_only: true,
                ..Damage::default()
            },
        );
        let (_rejection, repair) = plant_rejected_repair(&fixture);
        let driven = drive(
            &fixture,
            &DriveSeams {
                answer: Some(crate::ir::Answer::Declined),
                halts_run,
                ..DriveSeams::default()
            },
            2,
        );
        assert!(
            matches!(
                driven.progress.first(),
                Some(Ok(Progress::Answered {
                    key,
                    declined: true,
                    ..
                })) if *key == repair
            ),
            "halts_run={halts_run}: the decline is ingested: {:?}",
            driven.progress
        );
        let answers = answers_of(&driven.log, repair);
        assert_eq!(
            answers.first().map(|answered| &answered.answer),
            Some(&crate::topology::events::Answer4::Declined {
                decline_halts_run: halts_run
            }),
            "halts_run={halts_run}: the record says whether the decline halts"
        );
        let fold = replayed(&fixture);
        assert_eq!(
            fold.task_state(repair),
            Some(TaskState::Failed),
            "halts_run={halts_run}: the declined repair fails"
        );
        assert_eq!(
            fold.task_state(ALPHA),
            Some(TaskState::Failed),
            "halts_run={halts_run}: and its root with it: a lineage that a person declines \
             has no further repair to wait for"
        );
        assert!(
            !fold.leases().expect("started").any_candidate_or_lineage(),
            "halts_run={halts_run}: the lineage lease is released"
        );
        assert_eq!(
            fold.run_is_ending(),
            halts_run,
            "halts_run={halts_run}: the run ends on a decline exactly when the seam says so"
        );
        let closure = match driven.progress.get(1) {
            Some(Err(error)) => error.to_string(),
            other => panic!(
                "halts_run={halts_run}: with alpha failed and beta merged the run has only \
                 its closure left, which this build refuses: {other:?}"
            ),
        };
        assert!(
            closure.contains("closure derives"),
            "halts_run={halts_run}: {closure}"
        );
        assert_eq!(driven.entitlements_held, 0, "halts_run={halts_run}");
    }
}

/// `ST-15` for a repair: the incarnation that retained a repair attempt's
/// session retries it in place, in the same generation, without
/// materializing again, and the retry records `Retained` as what it observed.
#[test]
fn a_repairs_same_session_retry_records_retained_and_is_not_materialized_again() {
    use crate::engine::topology::run::{RunSeams, TopologyRun};
    use crate::engine::topology::select::Ceiling;

    let fixture = Fixture::build(
        "repair-retry-in-place",
        Damage {
            two_tasks: true,
            ..Damage::default()
        },
    );
    let (rejection, repair) = plant_rejected_repair(&fixture);
    let caps = vec![(
        AGENT.to_owned(),
        Caps {
            version: "1.2.3".to_owned(),
            json_output: true,
            session_resume: true,
            cost_reporting: true,
            read_only_mode: true,
            acp: false,
            model_list: false,
        },
    )];
    let harness = harness();
    let (_recovered, handle) =
        resume_with_real_refs(&fixture, &harness).expect("the healthy resume completes");
    let mut run = TopologyRun::resumed(handle, fixture.inputs(), Ceiling::unlimited());
    let mut hooks = HarnessTopologyHooks::new(Arc::clone(&harness));
    let sleeper = RecordingSleeper::default();
    let manager = fixture.manager();
    let runner = RecordingRunner::editing();
    let adapters = crate::engine::topology::scaffold::ScaffoldAdapters::erroring();
    let paths = crate::rundir::RunPaths::with_private_root(
        &fixture.repo_root,
        &fixture.started.run_id,
        &fixture.private_root,
    );
    paths.create().expect("the run directories are creatable");
    let plans = crate::engine::assembly::FrozenPlans {
        adapters: &adapters,
        paths: &paths,
        gates: &[],
        pools: &[],
        caps: &caps,
        worker_timeout: std::time::Duration::from_secs(300),
        decisions: &[],
    };
    let seams = RunSeams {
        manager: &manager,
        clock: &Frozen,
        sleeper: &sleeper,
        runner: &runner,
        adapters: &adapters,
        paths: &paths,
        plans: &plans,
        reviews: &crate::engine::attempt::LegacyReviewPasses,
        input_policy: &crate::engine::attempt::LegacyReviewInputPolicy,
        answers: &crate::interaction::UnattendedAnswers,
        ids: &FixedIds,
        halts_run: false,
    };

    let first = run
        .step(&seams, &mut hooks)
        .expect("the repair dispatches, materializes and its first attempt settles");
    assert!(
        matches!(first, Progress::Settled { key, accepted: false, .. } if key == repair),
        "an agent error is not an acceptable attempt: {first:?}"
    );
    let second = run
        .step(&seams, &mut hooks)
        .expect("the retry runs in the retained generation");
    assert!(
        matches!(second, Progress::Settled { key, accepted: false, .. } if key == repair),
        "{second:?}"
    );

    let log = TopologyFold::parse_log(&fixture.log_bytes()).expect("the log parses");
    let dispatches = repair_dispatches(&log, repair);
    assert_eq!(
        dispatches.len(),
        1,
        "the retry is the same generation's second attempt, not a fresh dispatch"
    );
    assert_eq!(
        dispatches[0].source_candidate.as_ref(),
        Some(&rejection.candidate)
    );
    let starts: Vec<(u32, bool, Option<crate::topology::events::Materialization>)> =
        attempt_starts_of(&log, repair)
            .iter()
            .map(|started| {
                (
                    started.attempt.0,
                    started.resume_session.is_some(),
                    started.materialization_observed,
                )
            })
            .collect();
    assert_eq!(
        starts,
        vec![
            (
                1,
                false,
                Some(crate::topology::events::Materialization::Clean)
            ),
            (
                2,
                true,
                Some(crate::topology::events::Materialization::Retained)
            ),
        ],
        "the first attempt observed the pick; the same-session retry resumes the session \
         and records `Retained`: the worktree it re-enters is the one the first attempt left"
    );
    assert_eq!(
        harness
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .count(REPAIR_MATERIALIZE, HookPhase::Before),
        1,
        "the materialization ran once, for the generation; the retry re-runs none of it"
    );
    let resumed = runner
        .requests()
        .iter()
        .filter(|request| request.role == crate::runner::ExecutionRole::Implement)
        .filter(|request| request.command.args.iter().any(|arg| arg == "--resume"))
        .count();
    assert_eq!(
        resumed, 1,
        "exactly the second worker invocation resumes a session"
    );
    assert!(run.invocations_balance());
}

/// `T-ANSWER` through the production reader: a person publishes the answer into
/// the run directory's `answers/` while the engine is away, and the next step of
/// the next incarnation ingests it before selecting anything else.
#[test]
fn an_answer_published_into_the_run_directory_is_ingested_by_the_next_incarnations_first_step() {
    let fixture = Fixture::build(
        "answer-file",
        Damage {
            two_tasks: true,
            no_automatic_repairs: true,
            ..Damage::default()
        },
    );
    let (rejection, repair) = plant_over_limit_repair(&fixture);
    let question = rejection
        .repair
        .admission
        .question()
        .expect("a human admission carries its question")
        .clone();
    let seams = DriveSeams {
        answers_from_run_dir: true,
        ..DriveSeams::default()
    };
    let blocked = drive(&fixture, &seams, 1);
    assert!(
        matches!(
            blocked.progress.first(),
            Some(Ok(Progress::Blocked { questions: 1 }))
        ),
        "with no answer file the run hard-blocks: {:?}",
        blocked.progress
    );

    let paths = crate::rundir::RunPaths::with_private_root(
        &fixture.repo_root,
        &fixture.started.run_id,
        &fixture.private_root,
    );
    let component = crate::util::filename_component(question.id.as_str());
    crate::rundir::stage_answer(
        &paths.answers(),
        &component,
        &crate::ir::Answer::Answered {
            text: question.options.first().expect("an option").clone(),
        },
        &mut crate::rundir::NoHooks,
    )
    .expect("the answer is staged");
    crate::rundir::publish_answer(&paths.answers(), &component, &mut crate::rundir::NoHooks)
        .expect("and published");

    let driven = drive(&fixture, &seams, 2);
    assert!(
        matches!(
            driven.progress.first(),
            Some(Ok(Progress::Answered {
                key,
                declined: false,
                ..
            })) if *key == repair
        ),
        "the next incarnation's first step ingests the published answer: {:?}",
        driven.progress
    );
    assert!(
        matches!(
            driven.progress.get(1),
            Some(Ok(Progress::Settled {
                key,
                accepted: true,
                ..
            })) if *key == repair
        ),
        "and the activated repair runs: {:?}",
        driven.progress
    );
    let answers = answers_of(&driven.log, repair);
    assert_eq!(answers.len(), 1);
    assert_eq!(
        answers[0].via, "event-log",
        "the record names the production reader that ingested it"
    );
    assert_eq!(answers[0].question, question.id);
}

/// Beta's candidate queued at the base, unmerged, so that a second lineage
/// can be rooted at beta.
fn plant_queued_beta(fixture: &Fixture) -> crate::topology::events::CandidateRef {
    use crate::workspace_manager::fixture::git;
    let commit = commit_on(
        fixture,
        fixture.base_sha.as_str(),
        "other.txt",
        "another task\n",
        "upstroke: task 1 attempt 1",
    );
    let tree = CommitSha(git(
        &fixture.repo_root,
        &["rev-parse", &format!("{commit}^{{tree}}")],
    ));
    let names = crate::engine::topology::candidate::CandidateNames::of(RUN_ID, BETA, GEN);
    for refname in [&names.prepared_ref, &names.candidate_ref] {
        git(
            &fixture.repo_root,
            &["update-ref", refname.as_str(), commit.as_str()],
        );
    }
    let candidate = crate::topology::events::CandidateRef {
        key: BETA,
        generation: GEN,
        commit_sha: commit.clone(),
        candidate_ref: names.candidate_ref.clone(),
    };
    append_events(
        fixture,
        &[
            for_task(BETA, "beta", dispatched_at(&fixture.base_sha)),
            for_task(BETA, "beta", attempt_started_in(fixture, 1)),
            candidate_prepared_for(fixture, BETA, &commit, &tree, &names, "other.txt"),
            TopologyEventBody::TaskCandidateCreated {
                data: crate::topology::events::TaskCandidateCreated {
                    candidate: candidate.clone(),
                },
            },
        ],
    );
    candidate
}

/// A lineage member's prepared candidate: it widens its lineage by the region
/// its diff touched and takes no lease of its own.
fn lineage_candidate_prepared(
    fixture: &Fixture,
    key: TaskKey,
    root: TaskKey,
    commit: &CommitSha,
    tree: &CommitSha,
    names: &crate::engine::topology::candidate::CandidateNames,
    paths: &[&str],
) -> TopologyEventBody {
    let first = paths.first().expect("a candidate touches a path");
    let TopologyEventBody::CandidatePrepared { mut data } =
        candidate_prepared_for(fixture, key, commit, tree, names, first)
    else {
        panic!("candidate_prepared_for builds a candidate_prepared")
    };
    let region = PathSet::Prefixes {
        paths: paths
            .iter()
            .map(|path| GitPath((*path).to_owned()))
            .collect(),
    };
    data.actual_paths = region.clone();
    data.lease_effect = crate::topology::events::CandidateLeaseEffect::WidensLineage {
        root,
        paths: region,
    };
    TopologyEventBody::CandidatePrepared { data }
}

/// A commit on `parent` editing several files: `commit_on` for more than one.
fn commit_editing(
    fixture: &Fixture,
    parent: &str,
    files: &[(&str, &str)],
    message: &str,
) -> CommitSha {
    use crate::workspace_manager::fixture::{git, write_file};
    let repo = &fixture.repo_root;
    for (file, content) in files {
        write_file(&repo.join(file), content.as_bytes());
        git(repo, &["add", "--", file]);
    }
    let tree = git(repo, &["write-tree"]);
    let commit = git(repo, &["commit-tree", &tree, "-p", parent, "-m", message]);
    for (file, _) in files {
        git(repo, &["rm", "-q", "-f", "--", file]);
    }
    CommitSha(commit)
}

/// Two lineages at runtime, overlapping on one path: the younger lineage's
/// repair has already queued its candidate when the older lineage's repair has
/// not started. The loop dispatches the older repair first (the younger
/// candidate is queued but ineligible behind the older lineage it overlaps),
/// publishes the older lineage, and only then the younger — lineage order,
/// not queue position.
#[test]
fn two_lineages_publish_in_lineage_order_and_the_younger_candidate_waits_behind_the_older() {
    use crate::workspace_manager::fixture::git;

    let fixture = Fixture::two_tasks("two-lineages");
    let alpha = plant_queued_candidate(&fixture);
    let beta = plant_queued_beta(&fixture);
    let base = fixture.base_sha.clone();

    let reject =
        |candidate: &crate::topology::events::CandidateRef, sequence: u32, contended: &[&str]| {
            let events = TopologyFold::parse_log(&fixture.log_bytes()).expect("parses");
            let fold = TopologyFold::replay(fixture.inputs(), &events).expect("replays");
            let paths = PathSet::Prefixes {
                paths: contended
                    .iter()
                    .map(|path| GitPath((*path).to_owned()))
                    .collect(),
            };
            let rejection = crate::engine::topology::repair::merge_rejected(
                &fold,
                &FixedIds,
                candidate,
                base.clone(),
                crate::topology::events::SequenceId(sequence),
                crate::topology::events::RejectionDisposition::Conflict {
                    paths: paths.clone(),
                },
                paths,
            )
            .expect("the conflict registers a repair");
            assert!(
                matches!(
                    rejection.repair.admission,
                    crate::topology::events::SpawnAdmission::Runnable
                ),
                "sequence {sequence}: the first repair of a lineage is runnable"
            );
            append_events(
                &fixture,
                &[TopologyEventBody::MergeRejected {
                    data: Box::new(rejection.clone()),
                }],
            );
            rejection
        };
    // Both conflicts contend `shared.txt`, so the two lineage regions overlap
    // and the younger lineage's candidate, which touches it, queues behind the
    // older lineage.
    let rejection_a = reject(&alpha.candidate, 0, &["candidate.txt", "shared.txt"]);
    let repair_a = rejection_a.repair.key;
    let rejection_b = reject(&beta, 1, &["other.txt", "shared.txt"]);
    let repair_b = rejection_b.repair.key;
    assert!(
        repair_a < repair_b,
        "the older lineage's repair registered first"
    );

    // The younger lineage's repair already ran to a candidate.
    let repaired_b = commit_editing(
        &fixture,
        base.as_str(),
        &[
            ("other.txt", "another task, repaired\n"),
            ("shared.txt", "the younger lineage's resolution\n"),
        ],
        "upstroke: task 3 attempt 1",
    );
    let tree_b = CommitSha(git(
        &fixture.repo_root,
        &["rev-parse", &format!("{repaired_b}^{{tree}}")],
    ));
    let names_b = crate::engine::topology::candidate::CandidateNames::of(RUN_ID, repair_b, GEN);
    for refname in [&names_b.prepared_ref, &names_b.candidate_ref] {
        git(
            &fixture.repo_root,
            &["update-ref", refname.as_str(), repaired_b.as_str()],
        );
    }
    let candidate_b = crate::topology::events::CandidateRef {
        key: repair_b,
        generation: GEN,
        commit_sha: repaired_b.clone(),
        candidate_ref: names_b.candidate_ref.clone(),
    };
    append_events(
        &fixture,
        &[
            repair_dispatched_in(&rejection_b, repair_b, GEN, BETA),
            repair_attempt_started(repair_b, 1),
            lineage_candidate_prepared(
                &fixture,
                repair_b,
                BETA,
                &repaired_b,
                &tree_b,
                &names_b,
                &["other.txt", "shared.txt"],
            ),
            TopologyEventBody::TaskCandidateCreated {
                data: crate::topology::events::TaskCandidateCreated {
                    candidate: candidate_b,
                },
            },
        ],
    );
    let before = replayed(&fixture);
    assert!(
        before
            .queue()
            .expect("started")
            .get(repair_b, GEN)
            .is_some(),
        "the younger lineage's candidate is queued"
    );
    assert!(
        before.eligible_integration_candidate().is_none(),
        "and ineligible: it overlaps the older lineage's region, and that lineage has no \
         candidate yet"
    );

    let driven = drive(&fixture, &DriveSeams::default(), 3);
    assert!(
        matches!(
            driven.progress.first(),
            Some(Ok(Progress::Settled {
                key,
                accepted: true,
                ..
            })) if *key == repair_a
        ),
        "step 1 dispatches the older lineage's repair rather than integrating the younger's \
         queued candidate: {:?}",
        driven.progress
    );
    assert!(
        matches!(
            driven.progress.get(1),
            Some(Ok(Progress::Integrated {
                key,
                sequence: crate::topology::events::SequenceId(2),
                ..
            })) if *key == repair_a
        ),
        "step 2 publishes the older lineage: {:?}",
        driven.progress
    );
    assert!(
        matches!(
            driven.progress.get(2),
            Some(Ok(Progress::Integrated {
                key,
                sequence: crate::topology::events::SequenceId(3),
                ..
            })) if *key == repair_b
        ),
        "step 3 publishes the younger, re-verified onto the head the older left: {:?}",
        driven.progress
    );
    let merged: Vec<(
        u32,
        Vec<TaskKey>,
        crate::topology::events::MergeLeaseRelease,
    )> = driven
        .log
        .iter()
        .filter_map(|event| match &event.body {
            TopologyEventBody::TaskMerged { data } => Some((
                data.sequence.0,
                data.satisfies.clone(),
                data.lease_release.clone(),
            )),
            _ => None,
        })
        .collect();
    assert_eq!(
        merged,
        vec![
            (
                2,
                vec![ALPHA, repair_a],
                crate::topology::events::MergeLeaseRelease::Lineage { root: ALPHA },
            ),
            (
                3,
                vec![BETA, repair_b],
                crate::topology::events::MergeLeaseRelease::Lineage { root: BETA },
            ),
        ],
        "each publication satisfies its lineage's closure and releases its lineage lease, \
         in lineage order"
    );
    let fold = replayed(&fixture);
    for key in [ALPHA, BETA, repair_a, repair_b] {
        assert_eq!(fold.task_state(key), Some(TaskState::Merged), "task {key}");
    }
    assert!(!fold.leases().expect("started").any_candidate_or_lineage());
    assert!(fold.queue().expect("started").is_empty());
    assert!(driven.invocations_balance);
    assert_eq!(driven.entitlements_held, 0);
}
