//! The documentation's examples as parser tests (docs/compiler-runtime.md
//! §2.9, chunk 1.2): every example of the decided syntax parses without a
//! syntax error, and every example of any age parses into a tree that holds
//! its text exactly.

use crate::syntax_examples as examples;

use jadpo_syntax::{parse_file, parse_snippet};

/// Decided examples that are meant not to parse, show syntax no page has
/// decided, or show syntax decided but not parsed yet (naming the chunk
/// that adds it). Each is (file, a line of the block, why).
const EXPECTED_TO_FAIL: &[(&str, &str, &str)] = &[
    (
        "docs/language/02-values-and-types.md",
        "type OrderId = Uuid       // rejected",
        "area 2 question 6 shows the block-less type it rejects",
    ),
    (
        "docs/language/02-values-and-types.md",
        "price as Int",
        "area 2 question 7 shows the cast it rejects",
    ),
    (
        "docs/language/03-logic.md",
        "Pence(attempt price(a) + attempt price(b))",
        "area 3 question 11 shows `attempt` inside an expression, which it rejects",
    ),
    (
        "docs/language/06-reads-and-writes.md",
        "route GET /todos {",
        "a lone `route` outside a `routes` group is not decided syntax (area 7)",
    ),
    (
        "docs/language/16-built-in-services.md",
        "\"welcome_email.subject\"",
        "the block also shows a JSON translations file",
    ),
    (
        "docs/language/03-logic.md",
        "each: row -> save_product(row, account: import.account_id),",
        "an inline function as a named argument (`each:`), decided 2026-10-16 (area 3), parses with batches (chunk 6.4)",
    ),
    (
        "docs/language/12-jobs-and-events.md",
        "each: row -> save_product(row, account: import.account_id),",
        "an inline function as a named argument (`each:`), decided 2026-10-16 (area 3), parses with batches (chunk 6.4)",
    ),
    (
        "docs/language/16-built-in-services.md",
        "free_space_floor: 5%",
        "`5%` as a percentage literal is carried to area 2 and not decided yet",
    ),
    (
        "docs/language/16-built-in-services.md",
        "metric checkouts_completed = Counter {",
        "`metric` declarations, decided 2026-10-16 (area 16), parse once telemetry is built; the block also shows a bare statement",
    ),
];

fn expected_failure(example: &examples::Example) -> Option<&'static str> {
    EXPECTED_TO_FAIL
        .iter()
        .find(|(file, line, _)| example.place.starts_with(file) && example.source.contains(line))
        .map(|(_, _, why)| *why)
}

#[test]
fn decided_examples_parse() {
    let mut failures = Vec::new();
    let mut checked = 0;
    for example in examples::all()
        .into_iter()
        .filter(examples::Example::decided)
    {
        let parse = parse_snippet(&example.source);
        let expected = expected_failure(&example);
        match (parse.errors().first(), expected) {
            (None, None) => checked += 1,
            (Some(error), None) => {
                let (line, column) = position(&example.source, error.start);
                failures.push(format!(
                    "{} (block line {line}:{column}): {} [{}]\n{}",
                    example.place,
                    error.message,
                    error.rule.id,
                    numbered(&example.source)
                ));
            }
            (None, Some(why)) => failures.push(format!(
                "{} now parses: remove it from EXPECTED_TO_FAIL ({why})",
                example.place
            )),
            (Some(_), Some(_)) => {}
        }
    }
    assert!(
        failures.is_empty(),
        "{} decided examples don't parse:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    // Fragments become `check` blocks as chunks build what they show, so
    // this only guards against finding none at all.
    assert!(checked > 50, "only {checked} decided examples were found");
}

#[test]
fn every_example_round_trips_through_the_tree() {
    for example in examples::all() {
        for parse in [parse_snippet(&example.source), parse_file(&example.source)] {
            assert_eq!(
                parse.syntax().to_string(),
                example.source,
                "{}: the tree must hold the text exactly",
                example.place
            );
        }
    }
}

fn position(text: &str, offset: u32) -> (usize, usize) {
    let before = &text[..offset as usize];
    let line = before.matches('\n').count() + 1;
    let column = before.len() - before.rfind('\n').map_or(0, |i| i + 1) + 1;
    (line, column)
}

fn numbered(text: &str) -> String {
    text.lines()
        .enumerate()
        .map(|(i, line)| format!("  {:>3} | {line}", i + 1))
        .collect::<Vec<_>>()
        .join("\n")
}
