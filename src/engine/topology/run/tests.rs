//! Extended notes: `docs/internals/engine/topology/run/tests.md`

use super::*;
use crate::topology::events::{AttemptNumber, DerivedOutcome, GenerationId};
use crate::topology::registry::TaskKey;

#[test]
fn the_transcribed_loop_branches_are_the_packets_seven() {
    assert_eq!(
        LoopBranch::ALL
            .iter()
            .map(|branch| branch.label())
            .collect::<Vec<_>>(),
        vec![
            "ingest answers",
            "integration",
            "ready_retry",
            "ready dispatch",
            "defer backoff",
            "hard block",
            "run-end closure",
        ],
        "transcribed from `decisions.sequential_substrate.loop`, in its order"
    );
}

#[test]
fn every_branch_states_what_this_build_does_with_it() {
    let refused: Vec<&str> = LoopBranch::ALL
        .iter()
        .filter(|branch| branch.disposition() == Disposition::RefusedByCheckpoint)
        .map(|branch| branch.label())
        .collect();
    assert_eq!(
        refused,
        vec!["integration", "run-end closure"],
        "`checkpoint_refusals` names exactly these two for PR7: \"integration \
         and run end beyond refusal\". A third refusal here is a build refusing \
         something the packet did not let it refuse"
    );

    let owed: Vec<&str> = LoopBranch::ALL
        .iter()
        .filter(|branch| branch.disposition() == Disposition::NotYetImplemented)
        .map(|branch| branch.label())
        .collect();
    assert!(
        owed.is_empty(),
        "the branches this build has not written. Every one of them is carried \
         in the type so that no instrument here has to notice its absence. \
         `defer backoff` left this list when `TopologyRun::step` grew its arm, \
         which is the shape every entry here is expected to leave by. It is \
         empty now: {owed:?}"
    );

    let elsewhere: Vec<&str> = LoopBranch::ALL
        .iter()
        .filter(|branch| matches!(branch.disposition(), Disposition::NotThisSlice { .. }))
        .map(|branch| branch.label())
        .collect();
    assert_eq!(
        elsewhere,
        vec!["ingest answers"],
        "a branch left this build's scope without saying which slice took it"
    );

    assert_eq!(
        LoopBranch::ReadyDispatch.disposition(),
        Disposition::Performed,
        "`loop` states this branch as four clauses and this build performs \
         three; the type says which three"
    );
}

#[test]
fn every_step_belongs_to_one_branch_or_to_none_for_a_reason() {
    let cases: Vec<(Step, Option<LoopBranch>)> = vec![
        (Step::Poisoned, None),
        (
            Step::Retry {
                key: TaskKey(0),
                generation: GenerationId(0),
                attempt: AttemptNumber(1),
            },
            Some(LoopBranch::ReadyRetry),
        ),
        (
            Step::Dispatch {
                key: TaskKey(0),
                generation: GenerationId(0),
                continuing: false,
            },
            Some(LoopBranch::ReadyDispatch),
        ),
        (Step::Backoff, Some(LoopBranch::DeferBackoff)),
        (
            Step::HardBlock {
                questions: Vec::new(),
            },
            Some(LoopBranch::HardBlock),
        ),
        (
            Step::Closure(DerivedOutcome::NotEnding),
            Some(LoopBranch::Closure),
        ),
    ];
    for (step, expected) in cases {
        assert_eq!(
            LoopBranch::of(&step),
            expected,
            "`{step:?}` maps to the wrong branch"
        );
    }
}

#[test]
fn a_refusal_names_the_branch_and_says_whether_anything_happened() {
    let untouched = LoopBranch::HardBlock.unimplemented().to_string();
    assert!(
        untouched.contains("hard block"),
        "the refusal names the branch: {untouched}"
    );
    assert!(
        untouched.contains("no effect was performed")
            && untouched.contains("no event was appended"),
        "and says the run is untouched: {untouched}"
    );

    assert!(
        LoopBranch::ALL
            .iter()
            .all(|branch| !matches!(branch.disposition(), Disposition::PartlyImplemented { .. })),
        "a branch is partly built again — assert its `performed ... does not ...` \
         message here, because an operator reading `not implemented` would look \
         for a run that had not started"
    );

    for branch in LoopBranch::ALL {
        let refusal = branch.unimplemented().to_string();
        assert!(
            refusal.contains(branch.label()),
            "`{}`'s refusal does not name it: {refusal}",
            branch.label()
        );
    }
}

fn blanked_bytes(source: &str, code: &str) -> usize {
    assert_eq!(
        code.len(),
        source.len(),
        "a production region of {} bytes against {} of source did not blank in place, so \
         every line number derived from an offset into it names a different line",
        code.len(),
        source.len()
    );
    source
        .as_bytes()
        .iter()
        .zip(code.as_bytes())
        .filter(|(from, to)| from != to)
        .count()
}

fn assert_blanked_region(file: &str, source: &str, code: &str, retained_floor: usize) {
    let blanked = blanked_bytes(source, code);
    assert!(
        blanked > 0,
        "nothing was blanked out of {file}'s {} bytes. Either the file carries no comment \
         and no literal, or the blanker has stopped removing them — and the second reads \
         exactly like a clean file to every needle below",
        source.len()
    );
    let retained = source.len() - blanked;
    assert!(
        retained * retained_floor > source.len(),
        "{retained} of {file}'s {} bytes survived blanking, under one {retained_floor}th of \
         it — a census over a fraction of a file reports zero for the part it never read",
        source.len()
    );
}

#[test]
fn the_blanked_region_count_falls_to_zero_when_nothing_was_removable() {
    const REMOVABLE: &str = "// a line comment\n\
                             /* a block comment */\n\
                             fn go() -> usize {\n\
                             let quoted = \"a string literal\";\n\
                             quoted.len()\n\
                             }\n\
                             #[cfg(test)]\n\
                             mod fixture {\n\
                             fn one() -> usize { 1 }\n\
                             }\n";
    const NOTHING_REMOVABLE: &str = "fn go() -> usize {\n\
                                     1 + 1\n\
                                     }\n";

    let removable = crate::effects::production_code(REMOVABLE);
    assert_eq!(
        removable.len(),
        REMOVABLE.len(),
        "the region function is length-preserving by contract, which is the whole reason a \
         length ratio cannot report what it removed"
    );
    assert!(
        blanked_bytes(REMOVABLE, &removable) > 0,
        "a comment, a literal and a `#[cfg(test)]` item were left standing: {removable:?}"
    );

    let nothing = crate::effects::production_code(NOTHING_REMOVABLE);
    assert_eq!(
        blanked_bytes(NOTHING_REMOVABLE, &nothing),
        0,
        "a source with nothing removable in it must count zero, or the guard the censuses \
         open with is true of every input and proves nothing: {nothing:?}"
    );
    assert!(
        nothing.len() * 10 > NOTHING_REMOVABLE.len(),
        "the ratio these censuses used to carry is satisfied by a region that blanked \
         nothing, which is why it could not stand in for a blanked-region count"
    );
}

#[test]
fn every_driver_append_propagates_its_error() {
    const FILE: &str = "src/engine/topology/run.rs";

    let source =
        std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(FILE))
            .expect("the driver's own source");
    let code = crate::effects::production_code(&source);

    assert_blanked_region(FILE, &source, &code, 10);

    let needle = "self.emit(";
    let mut sites = 0;
    let mut unpropagated = Vec::new();
    for (at, _) in code.match_indices(needle) {
        sites += 1;
        let mut depth = 0_i32;
        let mut end = None;
        for (offset, ch) in code[at + needle.len() - 1..].char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(at + needle.len() - 1 + offset + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(end) = end else {
            unpropagated.push(format!("unbalanced call at byte {at}"));
            continue;
        };
        if !code[end..].trim_start().starts_with('?') {
            let line = code[..at].matches('\n').count() + 1;
            unpropagated.push(format!("line {line} (of the blanked region)"));
        }
    }

    assert!(
        sites >= 4,
        "only {sites} append sites found, so a green result here would prove nothing"
    );
    assert!(
        unpropagated.is_empty(),
        "these driver appends do not propagate their error, so the append-error \
         protocol never runs for them: {unpropagated:?}"
    );
}

#[test]
fn the_loop_selects_through_one_function() {
    const FILE: &str = "src/engine/topology/run.rs";

    let source =
        std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(FILE))
            .expect("the driver's own source");
    let code = crate::effects::production_code(&source);

    assert_blanked_region(FILE, &source, &code, 10);

    let calls = |needle: &str| {
        code.match_indices(needle)
            .filter(|(at, _)| !code[..*at].trim_end().ends_with("fn"))
            .count()
    };

    assert_eq!(
        calls("select("),
        1,
        "the driver reaches its branch order through {} calls to `select`. Zero \
         means a second selector was written and this one bypassed — the branch \
         order the packet specifies is then not the order the run takes, and \
         `select`'s own tests still pass",
        calls("select(")
    );
    assert_eq!(
        calls("checkpoint("),
        1,
        "`checkpoint` refuses the terminals this build does not implement. One \
         selector guarded by one checkpoint is the pair; a selected step that \
         reached the loop unguarded is `INV-07`'s failure"
    );
}

#[test]
fn the_frozen_pool_table_is_read_through_one_seam() {
    const FILE: &str = "src/engine/assembly.rs";

    let source =
        std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(FILE))
            .expect("a source file");
    let code = crate::effects::production_code(&source);
    assert_blanked_region(FILE, &source, &code, 2);

    use crate::effects::census_domain::{Call, production_calls};

    let calls = production_calls(&code, "pool_for", Call::Free);
    assert_eq!(
        calls, 1,
        "{FILE} resolves an agent's pool from the frozen table in {calls} places. One is \
         `AttemptPlans::pool_for`, which is the seam every caller is supposed to ask; a second is \
         a rule with two implementations, and `wrong_internal_assumption` is how this project \
         pays for those"
    );

    assert_eq!(
        production_calls(
            "use crate::capacity::pool_for;\nfn second() { pool_for(agent, pools); }\n",
            "pool_for",
            Call::Free,
        ),
        1,
        "the needle this census reads {FILE} with does not see a bare `pool_for(` behind a \
         `use`, which is how a second implementation is ordinarily written"
    );
    assert_eq!(
        production_calls(
            "fn asks() { self.pool_for(agent); }\n",
            "pool_for",
            Call::Free
        ),
        0,
        "the needle counts the seam's own callers, so every caller asking correctly would be \
         reported as a second implementation"
    );
}

#[derive(Debug)]
struct AttemptStartedSite {
    line: usize,
    pool: String,
}

fn is_name_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn opens_a_struct_expression(before: &str) -> bool {
    const NOT_EXPRESSIONS: &[&str] = &["struct", "enum", "union", "trait", "impl", "for"];

    let mut head = before.trim_end();
    while let Some(rest) = head.strip_suffix("::") {
        head = rest.trim_end().trim_end_matches(is_name_char).trim_end();
    }

    if head.ends_with("->") {
        return false;
    }
    !NOT_EXPRESSIONS.iter().any(|keyword| {
        head.strip_suffix(keyword)
            .is_some_and(|rest| !rest.ends_with(is_name_char))
    })
}

fn matching_delimiter(code: &str, open: usize) -> Option<usize> {
    let mut stack = Vec::new();
    for (offset, ch) in code[open..].char_indices() {
        match ch {
            '{' | '(' | '[' => stack.push(ch),
            '}' | ')' | ']' => {
                let opened = stack.pop()?;
                if !matches!((opened, ch), ('{', '}') | ('(', ')') | ('[', ']')) {
                    return None;
                }
                if stack.is_empty() {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn top_level_field(body: &str, name: &str) -> Option<String> {
    let mut depth = 0_usize;
    let mut start = 0_usize;
    let mut fields = Vec::new();
    for (offset, ch) in body.char_indices() {
        match ch {
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                fields.push(&body[start..offset]);
                start = offset + 1;
            }
            _ => {}
        }
    }
    fields.push(&body[start..]);

    fields.iter().find_map(|field| {
        let field = field.trim();
        let label: String = field.chars().take_while(|ch| is_name_char(*ch)).collect();
        if label != name {
            return None;
        }
        let rest = field[label.len()..].trim_start();
        match rest.strip_prefix(':') {
            Some(value) if !value.starts_with(':') => Some(value.trim().to_owned()),
            _ if rest.is_empty() => Some(label),
            _ => None,
        }
    })
}

fn expression_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut word = String::new();
    for ch in text.chars() {
        if is_name_char(ch) {
            word.push(ch);
            continue;
        }
        if !word.is_empty() {
            tokens.push(std::mem::take(&mut word));
        }
        if !ch.is_whitespace() {
            tokens.push(ch.to_string());
        }
    }
    if !word.is_empty() {
        tokens.push(word);
    }
    tokens
}

fn is_the_declared_authority(found: &str, expected: &str) -> bool {
    expression_tokens(found) == expression_tokens(expected)
}

fn attempt_started_sites(code: &str) -> Vec<AttemptStartedSite> {
    const TYPE: &str = "AttemptStarted4";

    let mut found = Vec::new();
    for (at, _) in code.match_indices(TYPE) {
        let before = &code[..at];
        let after = &code[at + TYPE.len()..];
        if before.ends_with(is_name_char) || after.starts_with(is_name_char) {
            continue;
        }
        let gap = after.len() - after.trim_start().len();
        if !after[gap..].starts_with('{') {
            continue;
        }
        if !opens_a_struct_expression(before) {
            continue;
        }

        let line = before.matches('\n').count() + 1;
        let open = at + TYPE.len() + gap;
        let Some(close) = matching_delimiter(code, open) else {
            panic!("the `AttemptStarted4` at line {line} does not close on balanced delimiters");
        };
        let pool = top_level_field(&code[open + 1..close], "pool").unwrap_or_else(|| {
            panic!("the `AttemptStarted4` at line {line} has no top-level `pool` field")
        });
        found.push(AttemptStartedSite { line, pool });
    }
    found
}

#[test]
fn the_attempt_started_scanner_reads_expressions_and_not_return_types() {
    const COMMENT_SEPARATED: &str = "fn dispatch() {\n\
                                     let started = AttemptStarted4 /* the arm */ {\n\
                                     pool: plan.pool.clone(),\n\
                                     };\n\
                                     let retried = AttemptStarted4 // the other arm\n\
                                     {\n\
                                     pool: request.pool.clone(),\n\
                                     };\n\
                                     }\n";
    let separated = attempt_started_sites(&crate::effects::production_code(COMMENT_SEPARATED));
    assert_eq!(
        separated.len(),
        2,
        "a comment between the name and its brace hid a construction site from the scan, \
         which is a whole arm outside the domain the census reports on: {separated:?}"
    );
    assert!(
        is_the_declared_authority(&separated[0].pool, "plan.pool.clone()")
            && is_the_declared_authority(&separated[1].pool, "request.pool.clone()"),
        "the sites were found but read the wrong field: {separated:?}"
    );

    const NOT_CONSTRUCTIONS: &str = "struct AttemptStarted4 {\n\
                                     pool: Option<String>,\n\
                                     }\n\
                                     impl AttemptStarted4 {\n\
                                     fn build(plan: &Plan) -> AttemptStarted4 {\n\
                                     AttemptStarted4 {\n\
                                     pool: plan.pool.clone(),\n\
                                     }\n\
                                     }\n\
                                     }\n\
                                     impl Debug for AttemptStarted4 {\n\
                                     fn fmt(&self) {}\n\
                                     }\n\
                                     enum Wrapped {\n\
                                     Started(AttemptStarted4),\n\
                                     }\n";
    let constructions = attempt_started_sites(&crate::effects::production_code(NOT_CONSTRUCTIONS));
    assert_eq!(
        constructions.len(),
        1,
        "a declaration, an `impl` header or a return type was counted as a construction. The \
         needle this replaces counted `-> AttemptStarted4 {{` and then failed looking for a \
         `pool` field in a function body: {constructions:?}"
    );
    assert!(
        is_the_declared_authority(&constructions[0].pool, "plan.pool.clone()"),
        "the one real expression in that fixture was not the one read: {constructions:?}"
    );

    const LONGER_NAMES: &str = "fn go() {\n\
                                let a = AttemptStarted4Extended {\n\
                                pool: None,\n\
                                };\n\
                                let b = OuterAttemptStarted4 {\n\
                                pool: None,\n\
                                };\n\
                                }\n";
    assert!(
        attempt_started_sites(&crate::effects::production_code(LONGER_NAMES)).is_empty(),
        "a longer identifier ending or beginning with this type's name was read as the type"
    );

    const NESTED_FIRST: &str = "fn go() {\n\
                                let started = AttemptStarted4 {\n\
                                binding: Binding {\n\
                                pool: None,\n\
                                },\n\
                                pool: plan.pool.clone(),\n\
                                };\n\
                                }\n";
    const NESTED_LAST: &str = "fn go() {\n\
                               let started = AttemptStarted4 {\n\
                               pool: plan.pool.clone(),\n\
                               binding: Binding {\n\
                               pool: None,\n\
                               },\n\
                               };\n\
                               }\n";
    for (label, fixture) in [("nested first", NESTED_FIRST), ("nested last", NESTED_LAST)] {
        let sites = attempt_started_sites(&crate::effects::production_code(fixture));
        assert_eq!(sites.len(), 1, "{label}: {sites:?}");
        assert!(
            is_the_declared_authority(&sites[0].pool, "plan.pool.clone()"),
            "{label}: a `pool` inside a nested literal was read as this literal's own, so the \
             census reports a value the event never carried: {sites:?}"
        );
    }
}

#[test]
fn the_pool_authority_oracle_names_the_expression_rather_than_absence() {
    const AUTHORITY: &str = "plan.pool.clone()";

    for invention in [
        "None",
        "Option::None",
        "None::<String>",
        "Default::default()",
        "<_>::default()",
        "core::option::Option::None",
        "Option::default()",
    ] {
        assert!(
            !is_the_declared_authority(invention, AUTHORITY),
            "`{invention}` was accepted as this site's authority. The oracle this replaces \
             admitted every one of these that does not begin with `None`, which is a ledger \
             recording no pool while the plan resolves one"
        );
    }

    assert!(
        is_the_declared_authority("NonePool::resolve(agent)", "NonePool::resolve(agent)"),
        "an authority whose name begins with `None` was read as an invention, which is the \
         false positive a prefix test buys with the false negatives above"
    );

    assert!(
        is_the_declared_authority("plan\n            .pool\n            .clone()", AUTHORITY),
        "the same expression, wrapped, was read as a different one"
    );

    assert!(
        !is_the_declared_authority("mut pool", "mutpool"),
        "two tokens were run together into one, so expressions that differ compare equal"
    );

    assert!(
        is_the_declared_authority("plan.pool.clone()", AUTHORITY),
        "the authority a site actually carries was not accepted, so every site is an offender"
    );
}

#[test]
fn both_attempt_started_arms_take_their_pool_from_an_authority() {
    const SITES: &[(&str, &str, &str)] = &[
        (
            "src/engine/topology/attempt.rs",
            "plan.pool.clone()",
            "the dispatch arm: `plan.pool`, resolved by the assembler that owns the pool table",
        ),
        (
            "src/engine/topology/settle.rs",
            "request.pool.clone()",
            "the retry arm: `request.pool`, which the driver fills from `AttemptPlans::pool_for` \
             — the same authority, asked one step earlier",
        ),
    ];

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut off_authority: Vec<String> = Vec::new();
    let mut checked = 0_usize;
    for (file, authority, why) in SITES {
        let source = std::fs::read_to_string(root.join(file)).expect("a source file");
        let code = crate::effects::production_code(&source);
        assert_blanked_region(file, &source, &code, 10);

        let sites = attempt_started_sites(&code);
        assert_eq!(
            sites.len(),
            1,
            "{file} builds {} production `AttemptStarted4` expressions and this census claims \
             one arm per site. Zero means it no longer constructs one and the site has \
             moved; a second is a third arm, and it needs its own `SITES` entry naming the \
             authority it reads rather than a scan that stops at the first",
            sites.len()
        );
        for site in sites {
            checked += 1;
            if !is_the_declared_authority(&site.pool, authority) {
                off_authority.push(format!(
                    "{file}:{} initialises `pool` with `{}`, and this site's authority is \
                     `{authority}` — {why}",
                    site.line, site.pool
                ));
            }
        }
    }

    assert_eq!(
        checked,
        SITES.len(),
        "the per-site count above pins each file's boundary; this is the domain's size. Two \
         arms are the whole of what this census claims, and it inspected {checked} \
         expressions"
    );
    assert!(
        off_authority.is_empty(),
        "these append `attempt_started` with a `pool` that is not the expression the site is \
         supposed to carry, so the ledger and the plan can disagree about which pool the \
         attempt drained: {off_authority:?}"
    );

    const SECOND_ARM_INVENTS_ITS_POOL: &str = "fn dispatch() {\n\
                                               let started = AttemptStarted4 {\n\
                                               pool: plan.pool.clone(),\n\
                                               };\n\
                                               }\n\
                                               fn retry() {\n\
                                               let started = AttemptStarted4 /* past it */ {\n\
                                               pool: Option::default(),\n\
                                               };\n\
                                               }\n";
    let control = attempt_started_sites(&crate::effects::production_code(
        SECOND_ARM_INVENTS_ITS_POOL,
    ));
    assert_eq!(
        control.len(),
        2,
        "a construction site after the first, spelled with a comment before its brace, is \
         outside the domain this census reports on: {control:?}"
    );
    assert!(
        is_the_declared_authority(&control[0].pool, "plan.pool.clone()"),
        "the control's first site carries its authority, so reporting it would make every \
         correct arm an offender and the census's greens meaningless: {control:?}"
    );
    assert!(
        !is_the_declared_authority(&control[1].pool, "plan.pool.clone()"),
        "an invented pool in the second site is what this census exists to catch, and the \
         scan did not see it: {control:?}"
    );
}

#[test]
fn the_settled_notes_separate_the_successful_and_the_failed_settlement() {
    const NOTES: &str = include_str!("../../../../docs/internals/engine/topology/run.md");

    let settled = NOTES
        .split("\n## ")
        .find(|section| section.starts_with("`pub enum Progress` › `Settled {`"))
        .expect("the notes carry the `Settled {` heading the branch summary sits under");
    let settled = settled.split_whitespace().collect::<Vec<_>>().join(" ");

    for (proposition, pin) in [
        (
            "which settlement is appended depends on `accepted`",
            "depends on `accepted`",
        ),
        (
            "a rejected attempt settles with `attempt_finished`",
            "rejected attempt ends at `attempt_finished`",
        ),
        (
            "an accepted attempt appends no `attempt_finished`",
            "never appends `attempt_finished`",
        ),
        (
            "an accepted attempt settles at `candidate_prepared`",
            "`candidate_prepared`",
        ),
        (
            "and `task_candidate_created` follows it",
            "`task_candidate_created`",
        ),
    ] {
        assert!(
            settled.contains(pin),
            "the `Settled {{` summary must state that {proposition}; looked for {pin:?} in:\n{settled}"
        );
    }

    assert!(
        !settled.contains("the attempt through the Runner, and `attempt_finished`."),
        "the retired claim that the whole ready-dispatch branch ends in \
         `attempt_finished` must not come back:\n{settled}"
    );
}

#[test]
fn the_ready_branch_notes_do_not_owe_the_attempt_the_branch_runs() {
    const NOTES: &str = include_str!("../../../../docs/internals/engine/topology/run.md");
    const ARM: &str = "`pub const fn disposition(self) -> Disposition` › `Self::";

    let section = |heading: &str| -> String {
        NOTES
            .split("\n## ")
            .find(|section| section.starts_with(heading))
            .map(|section| section.split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap_or_else(|| panic!("the notes carry no {heading:?} heading"))
    };

    for (branch, variant, states, retired) in [
        (
            LoopBranch::ReadyDispatch,
            "ReadyDispatch",
            &[
                (
                    "the branch performs all four of its clauses",
                    "All four clauses",
                ),
                (
                    "the fourth of which is the attempt and its settlement",
                    "run one attempt through the Runner and settle",
                ),
            ][..],
            &[
                (
                    "only the first three clauses are performed",
                    "The first three are here",
                ),
                (
                    "the branch stops before the attempt, at `OpenNoAttempt`",
                    "leaves instead is `OpenNoAttempt`",
                ),
            ][..],
        ),
        (
            LoopBranch::ReadyRetry,
            "ReadyRetry",
            &[
                (
                    "the branch performs its clause whole",
                    "generation\", whole:",
                ),
                (
                    "the attempt and its settlement included",
                    "the attempt itself and its settlement",
                ),
                (
                    "reached through the ready-dispatch branch's own machinery",
                    "the same `attempt` and `settle`",
                ),
            ][..],
            &[(
                "running and settling the retry is still owed",
                "half still owed",
            )][..],
        ),
    ] {
        assert_eq!(
            branch.disposition(),
            Disposition::Performed,
            "`{}` is no longer `Performed`; these pins describe a branch that \
             runs its attempt and settles it, so they are the wrong assertions \
             for whatever it does now",
            branch.label()
        );

        let notes = section(&format!("{ARM}{variant} => Disposition::Performed,`"));
        for (proposition, pin) in states {
            assert!(
                notes.contains(pin),
                "the `{variant}` section must state that {proposition}; looked \
                 for {pin:?} in:\n{notes}"
            );
        }
        for (claim, pin) in retired {
            assert!(
                !notes.contains(pin),
                "the retired claim that {claim} must not come back — \
                 `TopologyRun::step` and `TopologyRun::retry_ready` both reach \
                 `attempt` and `settle`; found {pin:?} in:\n{notes}"
            );
        }
    }

    let partly = section("`pub enum Disposition` › `PartlyImplemented {`");
    assert!(
        partly.contains("No branch is `PartlyImplemented` today"),
        "the `PartlyImplemented` section must say the variant has no \
         inhabitants, because its example is a build that no longer \
         exists:\n{partly}"
    );
    assert!(
        !partly.contains("the ready-dispatch branch's first three clauses are"),
        "the retired claim that the ready-dispatch branch is presently \
         half-built must not come back:\n{partly}"
    );
}
