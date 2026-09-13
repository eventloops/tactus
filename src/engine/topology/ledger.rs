//! Extended notes: `docs/internals/engine/topology/ledger.md`

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;

use crate::topology::fold::{GenerationClass, TaskState, TopologyFold};
use crate::topology::leases::LeaseOwner;
use crate::topology::registry::{Origin, TaskKey, TaskRegistry};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Row {
    R1,
    R2,
    R3,
    R4,
    R5,
    R6,
    R7,
    R8,
    R9,
    R10,
    R11,
    R12,
    R13,
    R14,
    R15,
    R16,
    R17,
    R18,
    R19,
    R20,
    R21,
    R22,
    R23,
    R24,
    R25,
    R26,
    R27,
    R28,
}

impl Row {
    pub const ALL: [Self; 28] = [
        Self::R1,
        Self::R2,
        Self::R3,
        Self::R4,
        Self::R5,
        Self::R6,
        Self::R7,
        Self::R8,
        Self::R9,
        Self::R10,
        Self::R11,
        Self::R12,
        Self::R13,
        Self::R14,
        Self::R15,
        Self::R16,
        Self::R17,
        Self::R18,
        Self::R19,
        Self::R20,
        Self::R21,
        Self::R22,
        Self::R23,
        Self::R24,
        Self::R25,
        Self::R26,
        Self::R27,
        Self::R28,
    ];

    #[must_use]
    pub const fn resource(self) -> &'static str {
        match self {
            Self::R1 => "pipeline entitlement",
            Self::R2 => "merge entitlement",
            Self::R3 => "agent slot + pool slot pair",
            Self::R4 => "invocation registration",
            Self::R5 => "predicted lease",
            Self::R6 => "queue position",
            Self::R7 => "candidate actual lease",
            Self::R8 => "lineage lease",
            Self::R9 => "task worktree + intent",
            Self::R10 => "staging worktree + intent",
            Self::R11 => "candidates ref",
            Self::R12 => "prepared/<seq> proposal pin",
            Self::R13 => "provisional reservation",
            Self::R14 => "consumed counters",
            Self::R15 => "question logical state",
            Self::R16 => "binding override",
            Self::R17 => "the coordinator's own lock holds",
            Self::R18 => "execution root directory",
            Self::R19 => "disposable Git view directory",
            Self::R20 => "per-agent credential volume",
            Self::R21 => "integration ref and run-directory contents",
            Self::R22 => "host process handle / job object",
            Self::R23 => "candidate-prepared pin",
            Self::R24 => "gate/review snapshot worktree + intent",
            Self::R25 => "the worktree lock file itself",
            Self::R26 => "container invocation + global intent",
            Self::R27 => "engine-created Git objects nothing references",
            Self::R28 => "a surviving reaper's shared cleanup hold",
        }
    }

    #[must_use]
    pub const fn domain(self) -> Domain {
        match self {
            Self::R1
            | Self::R2
            | Self::R3
            | Self::R4
            | Self::R5
            | Self::R6
            | Self::R7
            | Self::R8
            | Self::R13
            | Self::R14
            | Self::R15
            | Self::R16 => Domain::LogicalFoldBroker,
            Self::R17 | Self::R22 | Self::R28 => Domain::ProcessLocalOs,
            Self::R9
            | Self::R10
            | Self::R11
            | Self::R12
            | Self::R18
            | Self::R19
            | Self::R21
            | Self::R23
            | Self::R24
            | Self::R25
            | Self::R26
            | Self::R27 => Domain::ExternalPhysical,
            Self::R20 => Domain::OperatorOwned,
        }
    }
}

impl fmt::Display for Row {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Domain {
    LogicalFoldBroker,
    ProcessLocalOs,
    ExternalPhysical,
    OperatorOwned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Class {
    Released,
    Consumed,
    PersistentOutput,
    Pruned,
    ResumablyOpen,
    Void,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Outcome {
    Complete,
    Parked,
    Halted,
    BudgetExceeded,
    NoRunFinished,
}

impl Outcome {
    pub const ALL: [Self; 5] = [
        Self::Complete,
        Self::Parked,
        Self::Halted,
        Self::BudgetExceeded,
        Self::NoRunFinished,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Fact {
    Zero,
    Held(u32),
    Absent,
    Present(u32),
    Balanced,
    Unbalanced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Requirement {
    Zero,
    Absent,
    Present,
    Retained,
    Balanced,
    Monotone,
    Any,
}

impl Requirement {
    #[must_use]
    pub const fn admits(self, before: Fact, after: Fact) -> bool {
        match self {
            Self::Zero | Self::Absent => matches!(after, Fact::Zero | Fact::Absent),
            Self::Present => matches!(after, Fact::Present(_) | Fact::Held(_)),
            Self::Retained => {
                before.count() == after.count() && before.is_zero() == after.is_zero()
            }
            Self::Balanced => matches!(after, Fact::Balanced),
            Self::Monotone => after.count() >= before.count(),
            Self::Any => true,
        }
    }
}

impl Fact {
    #[must_use]
    pub const fn count(self) -> u32 {
        match self {
            Self::Zero | Self::Absent | Self::Unbalanced => 0,
            Self::Held(n) | Self::Present(n) => n,
            Self::Balanced => 1,
        }
    }

    #[must_use]
    pub const fn is_zero(self) -> bool {
        matches!(self, Self::Zero | Self::Absent)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Part {
    pub name: &'static str,
    pub fact: Fact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Observation {
    pub row: Row,
    pub fact: Fact,
    pub parts: Vec<Part>,
    pub evidence: String,
}

impl Observation {
    #[must_use]
    pub fn part(&self, name: &str) -> Option<Fact> {
        self.parts
            .iter()
            .find(|part| part.name == name)
            .map(|part| part.fact)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Expectation {
    pub row: Row,
    pub class: Class,
    pub holds: Requirement,
    pub parts: Vec<(&'static str, Requirement)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Disagreement {
    pub row: Row,
    pub part: Option<&'static str>,
    pub class: Class,
    pub expected: Requirement,
    pub before: Fact,
    pub after: Fact,
    pub evidence: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcessLocal {
    pub invocations_balanced: bool,
    pub entitlements_held: u32,
    pub run_lock_held: bool,
    pub cleanup_hold_observed: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PhysicalInventory {
    pub task_slots: u32,
    pub staging_slots: u32,
    pub snapshot_slots: u32,
    pub candidates_refs: u32,
    pub prepared_pins: u32,
    pub candidate_pins: u32,
    pub integration_ref_present: bool,
    pub execution_root_present: bool,
    pub event_log_present: bool,
    pub plan_present: bool,
    pub report_present: bool,
    pub question_payloads: u32,
    pub answer_files: u32,
    pub partial_files: u32,
    pub marker_present: bool,
    pub owner_record_present: bool,
    pub commit_record_present: bool,
    pub private_artifacts: u32,
    pub run_lock_file_present: bool,
    pub cleanup_lock_file_present: bool,
    pub worktree_lock_file_present: bool,
    pub container_intents: u32,
    pub volumes_unchanged: bool,
    pub unreachable_objects: u32,
    pub store_objects: u32,
    pub store_objects_missing: u32,
    pub released_objects_checked: u32,
    pub released_objects_missing: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Ledger {
    pub observations: Vec<Observation>,
}

impl Ledger {
    #[must_use]
    pub fn observation(&self, row: Row) -> Option<&Observation> {
        self.observations
            .iter()
            .find(|observation| observation.row == row)
    }

    #[must_use]
    pub fn fact(&self, row: Row) -> Option<Fact> {
        self.observation(row).map(|observation| observation.fact)
    }
}

fn count(n: usize) -> Fact {
    match u32::try_from(n).unwrap_or(u32::MAX) {
        0 => Fact::Zero,
        held => Fact::Held(held),
    }
}

fn presence(n: u32) -> Fact {
    if n == 0 {
        Fact::Absent
    } else {
        Fact::Present(n)
    }
}

fn present(is: bool) -> Fact {
    if is { Fact::Present(1) } else { Fact::Absent }
}

fn balanced(is: bool) -> Fact {
    if is { Fact::Balanced } else { Fact::Unbalanced }
}

#[must_use]
pub fn observe(
    fold: &TopologyFold,
    physical: &PhysicalInventory,
    process: &ProcessLocal,
) -> Ledger {
    let keys = || (0..u32::try_from(fold.task_count()).unwrap_or(u32::MAX)).map(TaskKey);
    let leases = fold.leases();

    let pipeline = fold.pipeline_held();
    let mut predicted = 0;
    let mut candidate_leases = 0;
    let mut open_generations = Vec::new();
    for key in keys() {
        let Some(task) = fold.task(key) else { continue };
        for generation in &task.generations {
            let owner = LeaseOwner::Generation {
                key,
                generation: generation.id,
            };
            if leases.is_some_and(|table| table.holds(owner)) {
                predicted += 1;
            }
            if leases.is_some_and(|table| {
                table.holds(LeaseOwner::Candidate {
                    key,
                    generation: generation.id,
                })
            }) {
                candidate_leases += 1;
            }
            match generation.class {
                GenerationClass::OpenNoAttempt
                | GenerationClass::InFlight { .. }
                | GenerationClass::Promoting => {
                    open_generations.push(format!("k{} g{}", key.0, generation.id.0));
                }
                GenerationClass::RetainedIdle { .. } | GenerationClass::Closed => {}
            }
        }
    }
    let queued = fold.queue().map_or(0, |queue| queue.len());
    let lineages = leases.map_or(0, |table| table.lineages().len());
    let questions = fold.open_questions().map_or(0, BTreeMap::len);
    let overrides = keys()
        .filter(|key| fold.binding_override(*key).is_some())
        .count();
    let deferred = keys()
        .filter(|key| fold.task_state(*key) == Some(TaskState::Deferred))
        .count();
    let next_sequence = fold.next_sequence().map_or(0, |sequence| sequence.0);
    let generations = keys()
        .filter_map(|key| fold.task(key))
        .map(|task| u32::try_from(task.generations.len()).unwrap_or(u32::MAX))
        .fold(0u32, u32::saturating_add);
    let attempts = keys()
        .filter_map(|key| fold.task(key))
        .flat_map(|task| {
            task.generations
                .iter()
                .map(|generation| generation.attempts)
        })
        .fold(0u32, u32::saturating_add);
    let defers = keys()
        .filter_map(|key| fold.task(key))
        .map(|task| task.defers)
        .fold(0u32, u32::saturating_add);
    let entries = fold
        .registry()
        .map(TaskRegistry::entries)
        .unwrap_or_default();
    let display_ids = u32::try_from(entries.len()).unwrap_or(u32::MAX);
    let repairs = u32::try_from(
        entries
            .iter()
            .filter(|entry| entry.origin == Origin::MergeRepair)
            .count(),
    )
    .unwrap_or(u32::MAX);
    let lineage_index = entries
        .iter()
        .filter_map(|entry| entry.lineage.map(|lineage| lineage.index))
        .max()
        .unwrap_or(0);

    let observations = vec![
        Observation {
            row: Row::R1,
            parts: Vec::new(),
            fact: count(pipeline),
            evidence: format!(
                "{pipeline} held by the fold's count: open generations [{}], {} unresolved \
                 transaction",
                open_generations.join(", "),
                usize::from(fold.transaction().is_some())
            ),
        },
        Observation {
            row: Row::R2,
            parts: Vec::new(),
            fact: count(usize::from(fold.transaction().is_some())),
            evidence: fold.transaction().map_or_else(
                || "no unresolved integration transaction".to_owned(),
                |transaction| format!("sequence {} unresolved", transaction.sequence.0),
            ),
        },
        Observation {
            row: Row::R3,
            parts: Vec::new(),
            fact: balanced(process.invocations_balanced),
            evidence: "the slot pairs settle with the invocation ledger; certified as a \
                       process-local row at G6"
                .to_owned(),
        },
        Observation {
            row: Row::R4,
            parts: Vec::new(),
            fact: balanced(process.invocations_balanced),
            evidence: format!(
                "the invocation ledger {}",
                if process.invocations_balanced {
                    "balances"
                } else {
                    "does not balance"
                }
            ),
        },
        Observation {
            row: Row::R5,
            parts: Vec::new(),
            fact: count(predicted),
            evidence: format!("{predicted} generation lease(s) held"),
        },
        Observation {
            row: Row::R6,
            parts: Vec::new(),
            fact: count(queued),
            evidence: format!("{queued} queue position(s) open"),
        },
        Observation {
            row: Row::R7,
            parts: Vec::new(),
            fact: count(candidate_leases),
            evidence: format!("{candidate_leases} candidate lease(s) held"),
        },
        Observation {
            row: Row::R8,
            parts: Vec::new(),
            fact: count(lineages),
            evidence: format!("{lineages} lineage lease(s) held"),
        },
        Observation {
            row: Row::R9,
            parts: Vec::new(),
            fact: presence(physical.task_slots),
            evidence: format!("{} task intent(s) or worktree(s)", physical.task_slots),
        },
        Observation {
            row: Row::R10,
            parts: Vec::new(),
            fact: presence(physical.staging_slots),
            evidence: format!(
                "{} staging intent(s) or worktree(s)",
                physical.staging_slots
            ),
        },
        Observation {
            row: Row::R11,
            parts: Vec::new(),
            fact: presence(physical.candidates_refs),
            evidence: format!("{} candidates ref(s)", physical.candidates_refs),
        },
        Observation {
            row: Row::R12,
            parts: Vec::new(),
            fact: presence(physical.prepared_pins),
            evidence: format!("{} prepared/<seq> pin(s)", physical.prepared_pins),
        },
        Observation {
            row: Row::R13,
            parts: Vec::new(),
            fact: count(usize::try_from(process.entitlements_held).unwrap_or(usize::MAX)),
            evidence: format!(
                "{} provisional reservation(s) held at process end",
                process.entitlements_held
            ),
        },
        Observation {
            row: Row::R14,
            parts: vec![
                Part {
                    name: "merge_sequence",
                    fact: Fact::Present(next_sequence),
                },
                Part {
                    name: "task_keys",
                    fact: Fact::Present(u32::try_from(fold.task_count()).unwrap_or(u32::MAX)),
                },
                Part {
                    name: "display_ids",
                    fact: Fact::Present(display_ids),
                },
                Part {
                    name: "generations",
                    fact: Fact::Present(generations),
                },
                Part {
                    name: "attempts",
                    fact: Fact::Present(attempts),
                },
                Part {
                    name: "lineage_indexes",
                    fact: Fact::Present(lineage_index),
                },
                Part {
                    name: "repair_budget_units",
                    fact: Fact::Present(repairs),
                },
                Part {
                    name: "verification_defers",
                    fact: Fact::Present(defers),
                },
                Part {
                    name: "override_slots",
                    fact: Fact::Present(u32::try_from(overrides).unwrap_or(u32::MAX)),
                },
            ],
            fact: Fact::Present(next_sequence),
            evidence: format!(
                "next sequence {next_sequence}, {} task key(s), {display_ids} display id(s), \
                 {generations} generation(s), {attempts} attempt(s), lineage index {lineage_index}, \
                 {repairs} repair unit(s), {defers} verification defer(s), {overrides} override \
                 slot(s); numbers are consumed, never reused",
                fold.task_count()
            ),
        },
        Observation {
            row: Row::R15,
            parts: Vec::new(),
            fact: count(questions),
            evidence: format!("{questions} open question(s), {deferred} deferred task(s)"),
        },
        Observation {
            row: Row::R16,
            parts: Vec::new(),
            fact: presence(u32::try_from(overrides).unwrap_or(u32::MAX)),
            evidence: format!("{overrides} binding override(s) recorded"),
        },
        Observation {
            row: Row::R17,
            parts: Vec::new(),
            fact: present(process.run_lock_held),
            evidence: format!(
                "the run lock is {} after the process released it",
                if process.run_lock_held {
                    "held"
                } else {
                    "free"
                }
            ),
        },
        Observation {
            row: Row::R18,
            parts: Vec::new(),
            fact: present(physical.execution_root_present),
            evidence: format!(
                "the execution root is {}",
                if physical.execution_root_present {
                    "present"
                } else {
                    "pruned"
                }
            ),
        },
        Observation {
            row: Row::R19,
            parts: Vec::new(),
            fact: presence(physical.container_intents),
            evidence: "a Git view lives and dies with its container invocation".to_owned(),
        },
        Observation {
            row: Row::R20,
            parts: Vec::new(),
            fact: balanced(physical.volumes_unchanged),
            evidence: "the credential volumes the run started with are the ones it ends with"
                .to_owned(),
        },
        Observation {
            row: Row::R21,
            parts: vec![
                Part {
                    name: "integration_ref",
                    fact: present(physical.integration_ref_present),
                },
                Part {
                    name: "events_jsonl",
                    fact: present(physical.event_log_present),
                },
                Part {
                    name: "plan_normalized",
                    fact: present(physical.plan_present),
                },
                Part {
                    name: "report",
                    fact: present(physical.report_present),
                },
                Part {
                    name: "question_payloads",
                    fact: presence(physical.question_payloads),
                },
                Part {
                    name: "answer_files",
                    fact: presence(physical.answer_files),
                },
                Part {
                    name: "partial_files",
                    fact: presence(physical.partial_files),
                },
                Part {
                    name: "marker",
                    fact: present(physical.marker_present),
                },
                Part {
                    name: "run_lock_file",
                    fact: present(physical.run_lock_file_present),
                },
                Part {
                    name: "cleanup_lock_file",
                    fact: present(physical.cleanup_lock_file_present),
                },
                Part {
                    name: "owner_record",
                    fact: present(physical.owner_record_present),
                },
                Part {
                    name: "commit_record",
                    fact: present(physical.commit_record_present),
                },
                Part {
                    name: "private_artifacts",
                    fact: presence(physical.private_artifacts),
                },
            ],
            fact: present(
                physical.event_log_present
                    && physical.plan_present
                    && physical.owner_record_present
                    && physical.commit_record_present
                    && physical.integration_ref_present
                    && physical.run_lock_file_present
                    && !physical.marker_present,
            ),
            evidence: format!(
                "integration ref {}, events.jsonl {}, plan.normalized.json {}, report {}, {} \
                 question payload(s), {} answer file(s), {} .partial, marker {}, run.lock file \
                 {}, cleanup.lock file {}, owner record {}, commit record {}, {} private \
                 artifact(s)",
                yes(physical.integration_ref_present),
                yes(physical.event_log_present),
                yes(physical.plan_present),
                yes(physical.report_present),
                physical.question_payloads,
                physical.answer_files,
                physical.partial_files,
                yes(physical.marker_present),
                yes(physical.run_lock_file_present),
                yes(physical.cleanup_lock_file_present),
                yes(physical.owner_record_present),
                yes(physical.commit_record_present),
                physical.private_artifacts,
            ),
        },
        Observation {
            row: Row::R22,
            parts: Vec::new(),
            fact: balanced(process.invocations_balanced),
            evidence: "every host process the run registered was settled".to_owned(),
        },
        Observation {
            row: Row::R23,
            parts: Vec::new(),
            fact: presence(physical.candidate_pins),
            evidence: format!("{} candidate-prepared pin(s)", physical.candidate_pins),
        },
        Observation {
            row: Row::R24,
            parts: Vec::new(),
            fact: presence(physical.snapshot_slots),
            evidence: format!(
                "{} snapshot intent(s) or worktree(s)",
                physical.snapshot_slots
            ),
        },
        Observation {
            row: Row::R25,
            parts: Vec::new(),
            fact: present(physical.worktree_lock_file_present),
            evidence: "the repository-scoped worktree lock file spans runs".to_owned(),
        },
        Observation {
            row: Row::R26,
            parts: Vec::new(),
            fact: presence(physical.container_intents),
            evidence: format!("{} container intent(s)", physical.container_intents),
        },
        Observation {
            row: Row::R27,
            parts: vec![
                Part {
                    name: "released_objects_missing",
                    fact: count(
                        usize::try_from(physical.released_objects_missing).unwrap_or(usize::MAX),
                    ),
                },
                Part {
                    name: "store_objects",
                    fact: Fact::Present(physical.store_objects),
                },
                Part {
                    name: "store_objects_missing",
                    fact: count(
                        usize::try_from(physical.store_objects_missing).unwrap_or(usize::MAX),
                    ),
                },
                Part {
                    name: "unreachable_objects",
                    fact: Fact::Present(physical.unreachable_objects),
                },
            ],
            fact: balanced(
                physical.released_objects_missing == 0 && physical.store_objects_missing == 0,
            ),
            evidence: format!(
                "{} object(s) released by pruned refs, pins and worktrees checked, {} missing \
                 from Git's store; {} object(s) in the store, {} of those present before and \
                 missing now; fsck reports {} unreachable object(s) left to Git",
                physical.released_objects_checked,
                physical.released_objects_missing,
                physical.store_objects,
                physical.store_objects_missing,
                physical.unreachable_objects
            ),
        },
        Observation {
            row: Row::R28,
            parts: Vec::new(),
            fact: present(process.cleanup_hold_observed),
            evidence: format!(
                "a surviving cleanup hold is {}",
                if process.cleanup_hold_observed {
                    "observed"
                } else {
                    "not observed"
                }
            ),
        },
    ];
    Ledger { observations }
}

fn yes(is: bool) -> &'static str {
    if is { "present" } else { "absent" }
}

#[must_use]
pub fn equation(outcome: Outcome) -> Vec<Expectation> {
    use Class::{Consumed, PersistentOutput, Pruned, Released, ResumablyOpen, Void};
    use Requirement::{Absent, Any, Balanced, Monotone, Present, Retained, Zero};
    let expect = |row, class, holds| Expectation {
        row,
        class,
        holds,
        parts: Vec::new(),
    };
    let consumed = Expectation {
        row: Row::R14,
        class: Consumed,
        holds: Monotone,
        parts: vec![
            ("merge_sequence", Monotone),
            ("task_keys", Monotone),
            ("display_ids", Monotone),
            ("generations", Monotone),
            ("attempts", Monotone),
            ("lineage_indexes", Monotone),
            ("repair_budget_units", Monotone),
            ("verification_defers", Monotone),
            ("override_slots", Monotone),
        ],
    };
    let outputs = Expectation {
        row: Row::R21,
        class: PersistentOutput,
        holds: Present,
        parts: vec![
            ("integration_ref", Present),
            ("events_jsonl", Present),
            ("plan_normalized", Present),
            (
                "report",
                if outcome == Outcome::NoRunFinished {
                    Any
                } else {
                    Present
                },
            ),
            ("question_payloads", Retained),
            ("answer_files", Retained),
            ("partial_files", Retained),
            ("marker", Absent),
            ("run_lock_file", Present),
            ("cleanup_lock_file", if cfg!(unix) { Present } else { Any }),
            ("owner_record", Present),
            ("commit_record", Present),
            ("private_artifacts", Retained),
        ],
    };
    let released = Expectation {
        row: Row::R27,
        class: Released,
        holds: Balanced,
        parts: vec![
            ("released_objects_missing", Zero),
            ("store_objects", Monotone),
            ("store_objects_missing", Zero),
            ("unreachable_objects", Monotone),
        ],
    };
    let mut rows = vec![
        expect(Row::R3, Released, Balanced),
        expect(Row::R4, Released, Balanced),
        expect(Row::R13, Released, Zero),
        consumed,
        expect(Row::R16, PersistentOutput, Retained),
        expect(Row::R17, Released, Absent),
        expect(Row::R20, PersistentOutput, Balanced),
        outputs,
        expect(Row::R22, Released, Balanced),
        expect(Row::R25, PersistentOutput, Retained),
        released,
    ];
    let ended = [
        expect(Row::R1, Released, Zero),
        expect(Row::R2, Released, Zero),
        expect(Row::R9, Pruned, Absent),
        expect(Row::R10, Pruned, Absent),
        expect(Row::R12, Pruned, Absent),
        expect(Row::R18, Pruned, Absent),
        expect(Row::R19, Pruned, Absent),
        expect(Row::R23, Pruned, Absent),
        expect(Row::R24, Pruned, Absent),
        expect(Row::R26, Released, Absent),
        expect(Row::R28, Released, Absent),
    ];
    match outcome {
        Outcome::Complete => rows.extend(ended.into_iter().chain([
            expect(Row::R5, Released, Zero),
            expect(Row::R6, Consumed, Zero),
            expect(Row::R7, Released, Zero),
            expect(Row::R8, Released, Zero),
            expect(Row::R11, Pruned, Absent),
            expect(Row::R15, Consumed, Zero),
        ])),
        Outcome::Parked => rows.extend(ended.into_iter().chain([
            expect(Row::R5, Released, Zero),
            expect(Row::R6, ResumablyOpen, Retained),
            expect(Row::R7, ResumablyOpen, Retained),
            expect(Row::R8, ResumablyOpen, Retained),
            expect(Row::R11, ResumablyOpen, Retained),
            expect(Row::R15, ResumablyOpen, Present),
        ])),
        Outcome::Halted => rows.extend(ended.into_iter().chain([
            expect(Row::R5, Void, Zero),
            expect(Row::R6, Void, Any),
            expect(Row::R7, Void, Any),
            expect(Row::R8, Void, Any),
            expect(Row::R11, PersistentOutput, Retained),
            expect(Row::R15, Void, Any),
        ])),
        Outcome::BudgetExceeded => rows.extend(ended.into_iter().chain([
            expect(Row::R5, Released, Zero),
            expect(Row::R6, ResumablyOpen, Retained),
            expect(Row::R7, ResumablyOpen, Retained),
            expect(Row::R8, ResumablyOpen, Retained),
            expect(Row::R11, ResumablyOpen, Retained),
            expect(Row::R15, ResumablyOpen, Retained),
        ])),
        Outcome::NoRunFinished => rows.extend([
            expect(Row::R1, ResumablyOpen, Retained),
            expect(Row::R2, ResumablyOpen, Retained),
            expect(Row::R5, ResumablyOpen, Retained),
            expect(Row::R6, ResumablyOpen, Retained),
            expect(Row::R7, ResumablyOpen, Retained),
            expect(Row::R8, ResumablyOpen, Retained),
            expect(Row::R9, ResumablyOpen, Retained),
            expect(Row::R10, ResumablyOpen, Retained),
            expect(Row::R11, ResumablyOpen, Retained),
            expect(Row::R12, ResumablyOpen, Retained),
            expect(Row::R15, ResumablyOpen, Retained),
            expect(Row::R18, ResumablyOpen, Present),
            expect(Row::R19, ResumablyOpen, Retained),
            expect(Row::R23, ResumablyOpen, Retained),
            expect(Row::R24, ResumablyOpen, Retained),
            expect(Row::R26, ResumablyOpen, Retained),
            expect(Row::R28, ResumablyOpen, Any),
        ]),
    }
    rows.sort_by_key(|expectation| expectation.row);
    rows
}

#[must_use]
pub fn check(before: &Ledger, after: &Ledger, outcome: Outcome) -> Vec<Disagreement> {
    let mut disagreements = Vec::new();
    for expectation in equation(outcome) {
        let (Some(earlier), Some(later)) = (
            before.observation(expectation.row),
            after.observation(expectation.row),
        ) else {
            disagreements.push(Disagreement {
                row: expectation.row,
                part: None,
                class: expectation.class,
                expected: expectation.holds,
                before: Fact::Unbalanced,
                after: Fact::Unbalanced,
                evidence: "no observation".to_owned(),
            });
            continue;
        };
        if !expectation.holds.admits(earlier.fact, later.fact) {
            disagreements.push(Disagreement {
                row: expectation.row,
                part: None,
                class: expectation.class,
                expected: expectation.holds,
                before: earlier.fact,
                after: later.fact,
                evidence: later.evidence.clone(),
            });
        }
        for (name, holds) in &expectation.parts {
            let (Some(before_part), Some(after_part)) = (earlier.part(name), later.part(name))
            else {
                disagreements.push(Disagreement {
                    row: expectation.row,
                    part: Some(name),
                    class: expectation.class,
                    expected: *holds,
                    before: Fact::Unbalanced,
                    after: Fact::Unbalanced,
                    evidence: format!("no observation of `{name}`"),
                });
                continue;
            };
            if !holds.admits(before_part, after_part) {
                disagreements.push(Disagreement {
                    row: expectation.row,
                    part: Some(name),
                    class: expectation.class,
                    expected: *holds,
                    before: before_part,
                    after: after_part,
                    evidence: later.evidence.clone(),
                });
            }
        }
    }
    disagreements
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LedgerRecord {
    pub outcome: Outcome,
    pub rows: Vec<LedgerRecordRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LedgerRecordPart {
    pub name: &'static str,
    pub expected: Requirement,
    pub before: Fact,
    pub after: Fact,
    pub holds: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LedgerRecordRow {
    pub row: Row,
    pub resource: &'static str,
    pub domain: Domain,
    pub class: Class,
    pub expected: Requirement,
    pub before: Fact,
    pub after: Fact,
    pub holds: bool,
    pub parts: Vec<LedgerRecordPart>,
    pub evidence: String,
}

#[must_use]
pub fn record(before: &Ledger, after: &Ledger, outcome: Outcome) -> LedgerRecord {
    let rows = equation(outcome)
        .into_iter()
        .filter_map(|expectation| {
            let earlier = before.observation(expectation.row)?;
            let later = after.observation(expectation.row)?;
            let parts: Vec<LedgerRecordPart> = expectation
                .parts
                .iter()
                .map(|(name, holds)| {
                    let before_part = earlier.part(name).unwrap_or(Fact::Unbalanced);
                    let after_part = later.part(name).unwrap_or(Fact::Unbalanced);
                    LedgerRecordPart {
                        name,
                        expected: *holds,
                        before: before_part,
                        after: after_part,
                        holds: earlier.part(name).is_some()
                            && later.part(name).is_some()
                            && holds.admits(before_part, after_part),
                    }
                })
                .collect();
            Some(LedgerRecordRow {
                row: expectation.row,
                resource: expectation.row.resource(),
                domain: expectation.row.domain(),
                class: expectation.class,
                expected: expectation.holds,
                before: earlier.fact,
                after: later.fact,
                holds: expectation.holds.admits(earlier.fact, later.fact)
                    && parts.iter().all(|part| part.holds),
                parts,
                evidence: later.evidence.clone(),
            })
        })
        .collect();
    LedgerRecord { outcome, rows }
}

impl LedgerRecord {
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = "| Row | Resource | Class | Expected | Before | After | Holds | Evidence |\n\
                       |---|---|---|---|---|---|---|---|\n"
            .to_owned();
        for row in &self.rows {
            out.push_str(&format!(
                "| {} | {} | {:?} | {:?} | {:?} | {:?} | {} | {} |\n",
                row.row,
                row.resource,
                row.class,
                row.expected,
                row.before,
                row.after,
                if row.holds { "yes" } else { "NO" },
                row.evidence.replace('|', "\\|"),
            ));
            for part in &row.parts {
                out.push_str(&format!(
                    "| {} › {} | | | {:?} | {:?} | {:?} | {} | |\n",
                    row.row,
                    part.name,
                    part.expected,
                    part.before,
                    part.after,
                    if part.holds { "yes" } else { "NO" },
                ));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests;
