//! A failed test's report on the grid every error shares (CLI design
//! language, "A failed test report"; area 13, "What a failed test
//! reports"): the headline and location, the assertion with each part's
//! value hung beneath it, labelled values, code frames, and the command
//! that reruns it. Rich output is shown without escape codes.

use jadpo_term::{
    Block, Diagram, DiagramLine, Fact, Field, Frame, Item, Mark, Mode, Outcome, Report, Screen,
    Style, Text, Token,
};

fn rich() -> Style {
    Style {
        color: None,
        bold: false,
        ..Style::new(Mode::Rich)
    }
}

fn plain() -> Style {
    Style::new(Mode::Plain)
}

fn mark(column: usize, value: &str) -> Mark {
    Mark {
        column,
        value: value.to_string(),
        tone: None,
    }
}

fn assertion() -> Report {
    Report {
        subject: "adds wrongly".into(),
        location: Some("sizes.test.jadpo:7".into()),
        groups: vec![
            vec![Item::Diagram(Diagram {
                lines: vec![DiagramLine {
                    source: "assert add(2, 3) == add(1, 1) + 1".into(),
                    marks: vec![mark(7, "5"), mark(20, "2"), mark(30, "3")],
                    continues: false,
                }],
            })],
            vec![
                Item::Field(Field::value("expected", "3")),
                Item::Field(Field::new(
                    "actual",
                    vec![Text::value_in(Token::Failure, "5")],
                )),
            ],
            vec![
                Item::Field(Field::value("rule", "test.assertion_failed")),
                Item::Field(Field::new(
                    "rerun",
                    vec![Text::Command(
                        "jadpo test sizes.test.jadpo:7 --seed 48213".into(),
                    )],
                )),
            ],
        ],
    }
}

#[test]
fn an_assertion_hangs_each_value_under_its_part() {
    let screen = Screen::new().with(Block::report(assertion()));
    assert_eq!(
        screen.render(&rich()),
        "\
✗ failed  adds wrongly
  sizes.test.jadpo:7

  assert add(2, 3) == add(1, 1) + 1
         │            │         │
         5            2         3

  expected  3
  actual    5

  rule      test.assertion_failed
  rerun     jadpo test sizes.test.jadpo:7 --seed 48213
"
    );
    assert_eq!(
        screen.render(&plain()),
        "\
FAIL  adds wrongly
 --> sizes.test.jadpo:7

  assert add(2, 3) == add(1, 1) + 1
         |            |         |
         5            2         3

  expected: 3
  actual: 5

  rule: test.assertion_failed
  rerun: jadpo test sizes.test.jadpo:7 --seed 48213
"
    );
}

#[test]
fn a_value_that_would_run_into_the_next_drops_a_row_and_long_ones_are_cut() {
    let report = Report {
        subject: "a todo due yesterday is overdue".into(),
        location: Some("domains/planning/libraries/due_dates.test.jadpo:4".into()),
        groups: vec![
            vec![Item::Diagram(Diagram {
                lines: vec![DiagramLine {
                    source: "assert due_dates.is_overdue(due, now)".into(),
                    marks: vec![
                        Mark {
                            column: 7,
                            value: "false".into(),
                            tone: Some(Token::Failure),
                        },
                        mark(28, "\"2026-01-14T12:00:00Z\""),
                        mark(33, "\"2026-01-13T12:00:00Z\""),
                    ],
                    continues: false,
                }],
            })],
            vec![Item::Field(Field::new(
                "rerun",
                vec![Text::Command(
                    "jadpo test domains/planning/libraries/due_dates.test.jadpo:4 --seed 48213"
                        .into(),
                )],
            ))],
        ],
    };
    let screen = Screen::new().with(Block::report(report));
    assert_eq!(
        screen.render(&rich()),
        "\
✗ failed  a todo due yesterday is overdue
  domains/planning/libraries/due_dates.test.jadpo:4

  assert due_dates.is_overdue(due, now)
         │                    │    │
         false                │    \"2026-01-13T12:00:00Z\"
                              \"2026-01-14T12:00:00Z\"

  rerun
  jadpo test domains/planning/libraries/due_dates.test.jadpo:4 --seed 48213
"
    );
    let long = Report {
        subject: "lists match".into(),
        location: None,
        groups: vec![vec![Item::Diagram(Diagram {
            lines: vec![DiagramLine {
                source: "assert names == expected".into(),
                marks: vec![mark(7, &format!("[{}]", "\"a name\", ".repeat(12)))],
                continues: false,
            }],
        })]],
    };
    let rendered = Screen::new().with(Block::report(long)).render(&rich());
    let last = rendered.lines().last().expect("a line");
    assert_eq!(last.chars().count(), 80, "{last}");
    assert!(last.ends_with('…'), "{last}");
    // Plain output keeps values whole, for logs and searching.
    let whole = Screen::new()
        .with(Block::report(Report {
            subject: "lists match".into(),
            location: None,
            groups: vec![vec![Item::Diagram(Diagram {
                lines: vec![DiagramLine {
                    source: "assert names == expected".into(),
                    marks: vec![mark(7, &"x".repeat(100))],
                    continues: true,
                }],
            })]],
        }))
        .render(&plain());
    assert!(whole.contains(&"x".repeat(100)));
    assert!(
        whole.contains("  assert names == expected ...\n"),
        "{whole}"
    );
}

#[test]
fn a_fault_shows_its_frame_wrapped_prose_and_aligned_frames() {
    let report = Report {
        subject: "a large order overflows".into(),
        location: Some("sizes.test.jadpo:25".into()),
        groups: vec![
            vec![
                Item::Field(Field::value("fault", "int_overflow in function line_total")),
                Item::Field(Field::value("at", "sizes.jadpo:11:30")),
                Item::Frame(Frame {
                    line: 11,
                    source: "    var total: Pence = Pence(price * quantity)".into(),
                    column: 29,
                    length: 16,
                    label: "out of range".into(),
                }),
                Item::Field(Field::new(
                    "reason",
                    vec![Text::Prose(
                        "`price * quantity` gives a whole number outside `Int`'s range, -9223372036854775808 to 9223372036854775807; Jadpo stops rather than wrap around."
                            .into(),
                    )],
                )),
                Item::Field(Field::new(
                    "stack",
                    vec![
                        Text::Pair("sizes.jadpo:11".into(), "function line_total".into()),
                        Text::Pair(
                            "sizes.test.jadpo:25".into(),
                            "test \"a large order overflows\"".into(),
                        ),
                    ],
                )),
                Item::Field(Field::new(
                    "path",
                    vec![Text::Path(vec![
                        "test \"a large order overflows\"".into(),
                        "function line_total".into(),
                    ])],
                )),
            ],
            vec![Item::Field(Field::value("rule", "test.fault"))],
        ],
    };
    let screen = Screen::new().with(Block::report(report));
    assert_eq!(
        screen.render(&rich()),
        "\
✗ failed  a large order overflows
  sizes.test.jadpo:25

  fault     int_overflow in function line_total
  at        sizes.jadpo:11:30
     │
  11 │     var total: Pence = Pence(price * quantity)
     │                              ^^^^^^^^^^^^^^^^ out of range
     │
  reason    `price * quantity` gives a whole number outside `Int`'s range,
            -9223372036854775808 to 9223372036854775807; Jadpo stops rather than
            wrap around.
  stack     sizes.jadpo:11       function line_total
            sizes.test.jadpo:25  test \"a large order overflows\"
  path      test \"a large order overflows\" → function line_total

  rule      test.fault
"
    );
    assert_eq!(
        screen.render(&plain()),
        "\
FAIL  a large order overflows
 --> sizes.test.jadpo:25

  fault: int_overflow in function line_total
  at: sizes.jadpo:11:30
   |
11 |     var total: Pence = Pence(price * quantity)
   |                              ^^^^^^^^^^^^^^^^ out of range
  reason: `price * quantity` gives a whole number outside `Int`'s range, -9223372036854775808 to 9223372036854775807; Jadpo stops rather than wrap around.
  stack: sizes.jadpo:11       function line_total
         sizes.test.jadpo:25  test \"a large order overflows\"
  path: test \"a large order overflows\" -> function line_total

  rule: test.fault
"
    );
}

#[test]
fn a_summary_can_say_its_outcome_in_words_when_plain() {
    let screen = Screen::new().with(
        Block::summary(
            Outcome::Failure,
            "1 failed",
            [Fact::plain("3 passed"), Fact::plain("4 tests")],
        )
        .with_plain_headline("test failed"),
    );
    assert_eq!(screen.render(&rich()), "✗ 1 failed  3 passed · 4 tests\n");
    assert_eq!(
        screen.render(&plain()),
        "test failed: 1 failed, 3 passed, 4 tests\n"
    );
}
