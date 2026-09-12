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
