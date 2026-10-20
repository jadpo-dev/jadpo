//! A run as people read it (CLI design language, "A failed test report"):
//! each failed test's report in the order the tests ran, then one footer.
//! Passing tests are counted, not listed, so a passing run is one line.
//! The order seed is on every failure's rerun command; `--json` always has
//! it.

use std::time::Duration;

use jadpo_term::{
    Block, Diagram, DiagramLine, Fact, Field, Frame, Item, Mark, Outcome, Report, Screen, Text,
    Token, shown_path,
};

use jadpo_runtime::fields_text;

use crate::outcome::{AssertionFailure, ChangeMark, Detail, FailureInfo, FaultInfo, Located};
use crate::{Run, Test, TestFailure, TestKind};

/// Elapsed time joins the footer only when someone would notice it.
const SHOW_TIME_FROM: Duration = Duration::from_millis(100);

/// `a`, `a and b`, `a, b and c`.
pub(crate) fn either_and(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [only] => only.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The run's screen: every failure's report, then the footer.
pub fn screen(run: &Run) -> Screen {
    let mut screen = Screen::new();
    let root = run.root.as_deref();
    for result in &run.results {
        if let Some(failure) = &result.failure {
            screen.push(Block::report(report(&result.test, failure, root)));
        }
    }
    screen.push(footer(run));
    screen
}

fn footer(run: &Run) -> Block {
    let failed = run.failed();
    let passed = run.passed();
    let ran = run.results.len();
    let mut facts = Vec::new();
    if failed > 0 && passed > 0 {
        facts.push(Fact::plain(format!("{passed} passed")));
    }
    let mut kinds: Vec<(TestKind, usize)> = Vec::new();
    for result in &run.results {
        match kinds.iter_mut().find(|(kind, _)| *kind == result.test.kind) {
            Some((_, n)) => *n += 1,
            None => kinds.push((result.test.kind, 1)),
        }
    }
    kinds.sort();
    let mut counts: Vec<String> = if ran < run.total() {
        vec![format!("{ran} of {}", count(run.total(), "test", "tests"))]
    } else {
        kinds
            .iter()
            .map(|(kind, n)| {
                count(
                    *n,
                    &format!("{} test", kind.word()),
                    &format!("{} tests", kind.word()),
                )
            })
            .collect()
    };
    if run.elapsed >= SHOW_TIME_FROM
        && let Some(last) = counts.last_mut()
    {
        last.push_str(&format!(" in {:.1} s", run.elapsed.as_secs_f64()));
    }
    facts.extend(counts.into_iter().map(Fact::plain));
    if failed > 0 {
        Block::summary(Outcome::Failure, format!("{failed} failed"), facts)
            .with_plain_headline("test failed")
    } else {
        Block::summary(Outcome::Success, format!("{passed} passed"), facts)
            .with_plain_headline("tested")
    }
}

fn report(test: &Test, failure: &TestFailure, root: Option<&str>) -> Report {
    let mut groups = Vec::new();
    // A subscriber's delivery that failed for good between the test's
    // steps: which one, before what it failed with (chunk 6.11).
    if let Some(delivery) = &failure.delivery {
        groups.push(vec![Item::Field(Field::new(
            "delivery",
            vec![Text::Prose(format!(
                "`{}` to subscriber `{}` failed for good on attempt {} of {}",
                delivery.event, delivery.subscriber, delivery.attempt, delivery.attempts
            ))],
        ))]);
    }
    // What it failed with follows in the same block.
    let joined = !groups.is_empty() && !matches!(failure.detail, Detail::Assertion(_));
    match &failure.detail {
        Detail::Assertion(assertion) => {
            groups.push(vec![Item::Diagram(diagram(assertion))]);
            let values = assertion_fields(assertion, root);
            if !values.is_empty() {
                groups.push(values);
            }
        }
        Detail::Failure(info) => {
            // What happened names a callable, so it wraps rather than
            // being cut.
            let mut items = vec![Item::Field(Field::new(
                "failure",
                vec![Text::Prose(info.subject.clone())],
            ))];
            items.extend(failure_items(info, root));
            groups.push(items);
        }
        Detail::Fault(info) => groups.push(fault_items(info, root)),
        Detail::Fake(info) => groups.push(vec![
            Item::Field(Field::new("fake", vec![Text::Prose(info.subject())])),
            Item::Field(Field::new("reason", vec![Text::Prose(info.reason())])),
            Item::Field(Field::new("next", vec![Text::Prose(info.next())])),
        ]),
        Detail::Recording(info) => {
            let mut items = vec![Item::Field(Field::new(
                "decode",
                vec![Text::Prose(info.subject.clone())],
            ))];
            if let (Some(recorded), Some(written)) = (&info.recorded, &info.written) {
                items.push(Item::Field(Field::value("recorded", recorded.as_str())));
                items.push(Item::Field(Field::new(
                    "written",
                    vec![Text::value_in(Token::Failure, written.clone())],
                )));
            }
            items.push(Item::Field(Field::new(
                "reason",
                vec![Text::Prose(info.reason.clone())],
            )));
            items.push(Item::Field(Field::new(
                "next",
                vec![Text::Prose(info.next.clone())],
            )));
            groups.push(items);
        }
    }
    if joined && groups.len() > 1 {
        let rest = groups.remove(1);
        groups[0].extend(rest);
    }
    let mut last = Vec::new();
    if !failure.kept.is_empty() {
        last.push(Item::Field(Field::new(
            "data",
            vec![Text::Prose(format!(
                "kept as {} (--keep-failed)",
                failure.kept.join(", ")
            ))],
        )));
    }
    last.extend([
        Item::Field(Field::value("rule", failure.rule.id)),
        Item::Field(Field::new(
            "rerun",
            vec![Text::Command(failure.rerun.clone())],
        )),
    ]);
    groups.push(last);
    Report {
        subject: test.name.clone(),
        location: Some(format!(
            "{}:{}",
            shown_path(root, &failure.location.path),
            failure.location.line
        )),
        groups,
    }
}

fn diagram(assertion: &AssertionFailure) -> Diagram {
    Diagram {
        lines: assertion
            .lines
            .iter()
            .map(|line| DiagramLine {
                source: line.source.clone(),
                continues: line.continues,
                marks: line
                    .marks
                    .iter()
                    .map(|(column, value, root)| Mark {
                        column: *column,
                        value: value.clone(),
                        tone: root.then_some(Token::Failure),
                    })
                    .collect(),
            })
            .collect(),
    }
}

fn assertion_fields(assertion: &AssertionFailure, root: Option<&str>) -> Vec<Item> {
    let mut items = Vec::new();
    if let Some(expected) = &assertion.expected {
        items.push(Item::Field(Field::value("expected", expected.as_str())));
    }
    if let Some(actual) = &assertion.actual {
        // Only the wrong value takes the failure colour.
        let runs = match &assertion.returned {
            Some(value) => vec![
                ("returned ".to_string(), None),
                (value.clone(), Some(Token::Failure)),
            ],
            None => vec![(actual.clone(), Some(Token::Failure))],
        };
        items.push(Item::Field(Field::new("actual", vec![Text::Value(runs)])));
    }
    if !assertion.diff.is_empty() {
        let lines = assertion
            .diff
            .iter()
            .map(|change| {
                let (tone, rest) = match change.mark {
                    ChangeMark::Changed => (
                        Token::Warning,
                        vec![
                            (
                                format!(
                                    " {}: expected {}, actual ",
                                    change.path,
                                    change.expected.as_deref().unwrap_or_default()
                                ),
                                None,
                            ),
                            (
                                change.actual.clone().unwrap_or_default(),
                                Some(Token::Failure),
                            ),
                        ],
                    ),
                    ChangeMark::Extra => (
                        Token::Success,
                        vec![
                            (format!(" {}: ", change.path), None),
                            (
                                change.actual.clone().unwrap_or_default(),
                                Some(Token::Failure),
                            ),
                            (", only in actual".to_string(), None),
                        ],
                    ),
                    ChangeMark::Missing => (
                        Token::Failure,
                        vec![(
                            format!(
                                " {}: {}, only in expected",
                                change.path,
                                change.expected.as_deref().unwrap_or_default()
                            ),
                            None,
                        )],
                    ),
                };
                let mut runs = vec![(change.mark.symbol().to_string(), Some(tone))];
                runs.extend(rest);
                Text::Value(runs)
            })
            .collect();
        items.push(Item::Field(Field::new("diff", lines)));
    }
    if let Some(instead) = &assertion.instead {
        items.extend(failure_items(instead, root));
    }
    items
}

fn at(items: &mut Vec<Item>, located: &Option<Located>, root: Option<&str>) {
    if let Some(located) = located {
        // A path is never cut: one too long for its line gets its own.
        items.push(Item::Field(Field::new(
            "at",
            vec![Text::Place(format!(
                "{}:{}:{}",
                shown_path(root, &located.path),
                located.line,
                located.column
            ))],
        )));
        if !located.source.is_empty() {
            items.push(Item::Frame(Frame {
                line: located.line,
                source: located.source.clone(),
                column: located.column.saturating_sub(1),
                length: located.length,
                label: located.label.clone(),
            }));
        }
    }
}

fn failure_items(info: &FailureInfo, root: Option<&str>) -> Vec<Item> {
    let mut items = Vec::new();
    at(&mut items, &info.at, root);
    if let Some(reason) = &info.reason {
        items.push(Item::Field(Field::new(
            "reason",
            vec![Text::Prose(reason.clone())],
        )));
    }
    items.push(Item::Field(Field::value("code", info.code.as_str())));
    if let Some(kind) = &info.kind {
        items.push(Item::Field(Field::value(
            "kind",
            format!("FailureKind.{kind}"),
        )));
    }
    if let Some(message) = &info.message {
        items.push(Item::Field(Field::new(
            "message",
            vec![Text::Prose(message.clone())],
        )));
    }
    if !info.client.is_empty() {
        items.push(Item::Field(Field::value(
            "client",
            fields_text(info.client.iter().map(|(name, value)| (name, value))),
        )));
    }
    if !info.internal.is_empty() {
        items.push(Item::Field(Field::value(
            "internal",
            fields_text(info.internal.iter().map(|(name, value)| (name, value))),
        )));
    }
    if !info.cause.is_empty() {
        items.push(Item::Field(Field::new(
            "cause",
            info.cause.iter().cloned().map(Text::Prose).collect(),
        )));
    }
    if !info.path.is_empty() {
        items.push(Item::Field(Field::new(
            "path",
            vec![Text::Path(info.path.clone())],
        )));
    }
    items
}

fn fault_items(info: &FaultInfo, root: Option<&str>) -> Vec<Item> {
    let mut items = vec![Item::Field(Field::new(
        "fault",
        vec![Text::Prose(info.subject.clone())],
    ))];
    at(&mut items, &info.at, root);
    if let Some(reason) = &info.reason {
        items.push(Item::Field(Field::new(
            "reason",
            vec![Text::Prose(reason.clone())],
        )));
    }
    if let Some(next) = &info.next {
        items.push(Item::Field(Field::new(
            "next",
            vec![Text::Prose(next.clone())],
        )));
    }
    if !info.stack.is_empty() {
        items.push(Item::Field(Field::new(
            "stack",
            info.stack
                .iter()
                .map(|frame| {
                    let place = format!(
                        "{}:{}",
                        shown_path(root, &frame.location.path),
                        frame.location.line
                    );
                    let callable = match frame.repeats {
                        1 => frame.callable.clone(),
                        n => format!("{} ({n} times)", frame.callable),
                    };
                    Text::Pair(place, callable)
                })
                .collect(),
        )));
    }
    items
}
