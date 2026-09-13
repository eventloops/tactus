use super::*;

/// `resource_accounting.completeness_rule`: one row per resource, every row
/// in every outcome's equation exactly once, and every row in exactly one
/// enforcement domain — the shape ST-09 iterates.
#[test]
fn every_outcome_equation_names_every_row_once() {
    for outcome in Outcome::ALL {
        let rows: Vec<Row> = equation(outcome)
            .iter()
            .map(|expectation| expectation.row)
            .collect();
        assert_eq!(rows, Row::ALL.to_vec(), "{outcome:?}");
    }
    let mut by_domain = std::collections::BTreeMap::<String, Vec<Row>>::new();
    for row in Row::ALL {
        by_domain
            .entry(format!("{:?}", row.domain()))
            .or_default()
            .push(row);
        assert!(!row.resource().is_empty());
    }
    assert_eq!(
        by_domain
            .iter()
            .map(|(domain, rows)| (domain.as_str(), rows.len()))
            .collect::<Vec<_>>(),
        vec![
            ("ExternalPhysical", 12),
            ("LogicalFoldBroker", 12),
            ("OperatorOwned", 1),
            ("ProcessLocalOs", 3),
        ],
        "the packet's four domains, twelve, twelve, one and three rows"
    );
}

/// `Requirement::admits` reads a before/after pair the way each word says.
#[test]
fn requirements_read_before_and_after_as_named() {
    use Fact::{Absent, Balanced, Held, Present, Unbalanced, Zero};
    assert!(Requirement::Zero.admits(Held(2), Zero));
    assert!(Requirement::Zero.admits(Held(2), Absent));
    assert!(!Requirement::Zero.admits(Zero, Held(1)));
    assert!(Requirement::Present.admits(Absent, Present(1)));
    assert!(!Requirement::Present.admits(Present(1), Absent));
    assert!(Requirement::Retained.admits(Present(2), Present(2)));
    assert!(Requirement::Retained.admits(Absent, Zero));
    assert!(!Requirement::Retained.admits(Present(2), Present(1)));
    assert!(!Requirement::Retained.admits(Present(1), Absent));
    assert!(Requirement::Balanced.admits(Unbalanced, Balanced));
    assert!(!Requirement::Balanced.admits(Balanced, Unbalanced));
    assert!(Requirement::Monotone.admits(Present(1), Present(3)));
    assert!(!Requirement::Monotone.admits(Present(3), Present(1)));
    assert!(Requirement::Any.admits(Unbalanced, Unbalanced));
}

/// The rows whose `resource_accounting` cell names several facts carry one
/// requirement per fact, and `check` reports a disagreement on the named
/// part: a report that went missing at Complete is R21's `report` part, not
/// a row that still reads Present because the other outputs are there.
#[test]
fn a_rows_named_facts_are_each_consumed_by_its_verdict() {
    let named: Vec<(Row, Vec<&str>)> = equation(Outcome::Complete)
        .iter()
        .filter(|expectation| !expectation.parts.is_empty())
        .map(|expectation| {
            (
                expectation.row,
                expectation.parts.iter().map(|(name, _)| *name).collect(),
            )
        })
        .collect();
    assert_eq!(
        named,
        vec![
            (
                Row::R14,
                vec![
                    "merge_sequence",
                    "task_keys",
                    "display_ids",
                    "generations",
                    "attempts",
                    "lineage_indexes",
                    "repair_budget_units",
                    "verification_defers",
                    "override_slots",
                ]
            ),
            (
                Row::R21,
                vec![
                    "integration_ref",
                    "events_jsonl",
                    "plan_normalized",
                    "report",
                    "question_payloads",
                    "answer_files",
                    "partial_files",
                    "marker",
                    "run_lock_file",
                    "cleanup_lock_file",
                    "owner_record",
                    "commit_record",
                    "private_artifacts",
                ]
            ),
            (
                Row::R27,
                vec![
                    "released_objects_missing",
                    "store_objects",
                    "store_objects_missing",
                    "unreachable_objects",
                ]
            ),
        ],
        "the three rows the packet's cells name several facts for, every fact named"
    );
    for outcome in Outcome::ALL {
        let report = equation(outcome)
            .into_iter()
            .find(|expectation| expectation.row == Row::R21)
            .and_then(|expectation| {
                expectation
                    .parts
                    .iter()
                    .find(|(name, _)| *name == "report")
                    .map(|(_, holds)| *holds)
            })
            .expect("R21 names the report");
        assert_eq!(
            report,
            if outcome == Outcome::NoRunFinished {
                Requirement::Any
            } else {
                Requirement::Present
            },
            "{outcome:?}: the report is present at every run end and regenerated on resume"
        );
    }

    // A ledger whose R21 row reads Present while its report part reads Absent.
    let observation = |parts: Vec<Part>| Observation {
        row: Row::R21,
        fact: Fact::Present(1),
        parts,
        evidence: "synthetic".to_owned(),
    };
    let whole = |report: bool| {
        let mut parts = vec![
            Part {
                name: "integration_ref",
                fact: Fact::Present(1),
            },
            Part {
                name: "events_jsonl",
                fact: Fact::Present(1),
            },
            Part {
                name: "plan_normalized",
                fact: Fact::Present(1),
            },
            Part {
                name: "report",
                fact: if report {
                    Fact::Present(1)
                } else {
                    Fact::Absent
                },
            },
            Part {
                name: "question_payloads",
                fact: Fact::Absent,
            },
            Part {
                name: "answer_files",
                fact: Fact::Absent,
            },
            Part {
                name: "partial_files",
                fact: Fact::Absent,
            },
            Part {
                name: "marker",
                fact: Fact::Absent,
            },
            Part {
                name: "run_lock_file",
                fact: Fact::Present(1),
            },
            Part {
                name: "cleanup_lock_file",
                fact: Fact::Present(1),
            },
            Part {
                name: "owner_record",
                fact: Fact::Present(1),
            },
            Part {
                name: "commit_record",
                fact: Fact::Present(1),
            },
        ];
        parts.push(Part {
            name: "private_artifacts",
            fact: Fact::Absent,
        });
        Ledger {
            observations: vec![observation(parts)],
        }
    };
    let before = whole(true);
    let after = whole(false);
    let disagreements: Vec<Disagreement> = check(&before, &after, Outcome::Complete)
        .into_iter()
        .filter(|disagreement| disagreement.row == Row::R21)
        .collect();
    assert_eq!(disagreements.len(), 1, "{disagreements:#?}");
    assert_eq!(disagreements[0].part, Some("report"));
    assert_eq!(disagreements[0].expected, Requirement::Present);
    assert_eq!(disagreements[0].after, Fact::Absent);
    assert!(
        check(&before, &before, Outcome::Complete)
            .iter()
            .all(|disagreement| disagreement.row != Row::R21),
        "with the report present the row and every part hold"
    );
    let record = record(&before, &after, Outcome::Complete);
    let row = record
        .rows
        .iter()
        .find(|row| row.row == Row::R21)
        .expect("R21 is recorded");
    assert!(!row.holds, "the row's verdict is its parts' verdict");
    assert_eq!(row.parts.len(), 13);
    assert!(record.render().contains("R21 › report"));
}
