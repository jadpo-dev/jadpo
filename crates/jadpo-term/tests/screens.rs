//! The grid, symbols, colour tokens and diagnostics, checked against the
//! screens drawn in the CLI design language and area 14's plain error form.

use std::collections::BTreeMap;

use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_term::{Block, Depth, Fact, Mode, Outcome, Row, Screen, Style, Symbols, Theme, Token};

const TITLES: &str = "domains/planning/libraries/titles.jadpo";

fn sources() -> BTreeMap<String, String> {
    let text = "type Title = String {\n    max_length: 80\n}\n\nfunction title_of(title: Title) -> Title {\n    return titel\n}\n";
    BTreeMap::from([(TITLES.to_string(), text.to_string())])
}

fn unknown_value() -> Diagnostic {
    let text = &sources()[TITLES];
    let start = text.find("titel").expect("titel") as u32;
    // The catalogue has no source rules yet, so a command rule stands in; the
    // layout under test does not depend on which rule it is.
    Diagnostic::new(
        &rule::CLI_UNKNOWN_COMMAND,
        "`titel` isn't defined in this scope",
    )
    .at(Span::new(TITLES, start, start + 5), "not defined")
    .reason("Jadpo could not find this name. Did you mean `title`?")
    .next("Use `title` instead of `titel`")
}

/// Rich layout and symbols with no escape codes, so snapshots read as text.
fn rich() -> Style {
    Style {
        color: None,
        bold: false,
        ..Style::new(Mode::Rich)
    }
}

#[test]
fn plain_diagnostic_matches_area_14() {
    let screen = Screen::new().with(Block::diagnostic(&unknown_value(), &sources()));
    assert_eq!(
        screen.render(&Style::new(Mode::Plain)),
        "error: `titel` isn't defined in this scope
 --> domains/planning/libraries/titles.jadpo:6:12
  |
6 |     return titel
  |            ^^^^^ not defined
  reason: Jadpo could not find this name. Did you mean `title`?
  next: Use `title` instead of `titel` (agent)
  rule: cli.unknown_command
  help: https://jadpo.dev/docs/errors/cli.unknown_command
"
    );
}

#[test]
fn rich_diagnostic_matches_the_design_language() {
    let screen = Screen::new().with(Block::diagnostic(&unknown_value(), &sources()));
    assert_eq!(
        screen.render(&rich()),
        "✗ error  `titel` isn't defined in this scope
  domains/planning/libraries/titles.jadpo:6:12
    │
  6 │     return titel
    │            ^^^^^ not defined
    │
  reason    Jadpo could not find this name. Did you mean `title`?
  next      Use `title` instead of `titel` (agent)
  rule      cli.unknown_command
  help      https://jadpo.dev/docs/errors/cli.unknown_command
"
    );
}

/// The names in use a rename should see (area 1,
/// `semantic.duplicate_declaration`), in two columns after `next`.
fn with_names_in_use() -> Diagnostic {
    unknown_value().in_use(vec![
        (
            "Title".to_string(),
            "domains/planning/libraries/titles.jadpo".to_string(),
        ),
        (
            "TitleCase".to_string(),
            "shared/types/title_case.jadpo".to_string(),
        ),
    ])
}

#[test]
fn names_in_use_follow_next_in_two_columns() {
    let screen = Screen::new().with(Block::diagnostic(&with_names_in_use(), &sources()));
    let plain = screen.render(&Style::new(Mode::Plain));
    assert!(
        plain.contains(
            "  next: Use `title` instead of `titel` (agent)
  in use:
    Title      domains/planning/libraries/titles.jadpo
    TitleCase  shared/types/title_case.jadpo
  rule: cli.unknown_command
"
        ),
        "{plain}"
    );
    let rich = screen.render(&rich());
    assert!(
        rich.contains(
            "  next      Use `title` instead of `titel` (agent)
  in use    Title      domains/planning/libraries/titles.jadpo
            TitleCase  shared/types/title_case.jadpo
  rule      cli.unknown_command
"
        ),
        "{rich}"
    );
}

/// The help line is a link (CLI design language, "Type"): its address
/// printed in full and underlined where text has a look, and plain text
/// in plain output.
#[test]
fn the_help_line_is_the_error_pages_address_underlined() {
    let screen = Screen::new().with(Block::diagnostic(&unknown_value(), &sources()));
    let styled = screen.render(&Style {
        bold: true,
        ..rich()
    });
    assert!(
        styled.contains(
            "help      \x1b[4mhttps://jadpo.dev/docs/errors/cli.unknown_command\x1b[0m\n"
        ),
        "{styled}"
    );
    let plain = screen.render(&Style::new(Mode::Plain));
    assert!(
        plain.ends_with("  help: https://jadpo.dev/docs/errors/cli.unknown_command\n"),
        "{plain}"
    );
}

#[test]
fn ascii_symbols_replace_unicode_ones() {
    let style = Style {
        symbols: Symbols::Ascii,
        ..rich()
    };
    let out = Screen::new()
        .with(Block::diagnostic(&unknown_value(), &sources()))
        .render(&style);
    assert!(out.starts_with("x error  `titel`"), "{out}");
    assert!(out.contains("  6 |     return titel\n"), "{out}");
}

#[test]
fn a_diagnostic_without_a_place_has_no_location_or_frame() {
    let diagnostic = Diagnostic::new(&rule::CLI_UNKNOWN_COMMAND, "`hlep` isn't a jadpo command")
        .reason("Jadpo has no command with this name. Did you mean `help`?")
        .next("Use `jadpo help` instead of `jadpo hlep`");
    let out = Screen::new()
        .with(Block::diagnostic(&diagnostic, &()))
        .render(&Style::new(Mode::Plain));
    assert_eq!(
        out,
        "error: `hlep` isn't a jadpo command
  reason: Jadpo has no command with this name. Did you mean `help`?
  next: Use `jadpo help` instead of `jadpo hlep` (agent)
  rule: cli.unknown_command
  help: https://jadpo.dev/docs/errors/cli.unknown_command
"
    );
}

#[test]
fn colour_sits_on_symbols_severity_words_carets_and_labels_only() {
    let style = Style {
        color: Some(Depth::Ansi16),
        ..Style::new(Mode::Rich)
    };
    let out = Screen::new()
        .with(Block::diagnostic(&unknown_value(), &sources()))
        .render(&style);
    let first = out.lines().next().expect("headline");
    assert_eq!(
        first,
        "\x1b[1;31m✗ error\x1b[0m  \x1b[1m`titel` isn't defined in this scope\x1b[0m"
    );
    assert!(out.contains("\x1b[1;31m^^^^^\x1b[0m"), "{out}");
    assert!(
        out.contains("  \x1b[2mreason\x1b[0m    Jadpo could not find this name."),
        "{out}"
    );
    assert!(
        out.contains("(agent)") && out.contains("\x1b[2m(agent)\x1b[0m"),
        "{out}"
    );
}

#[test]
fn human_decisions_take_the_decision_colour() {
    let diagnostic = Diagnostic::new(&rule::CLI_UNKNOWN_COMMAND, "a person decides")
        .next_for("Choose a backfill", jadpo_diag::Decider::Human);
    let style = Style {
        color: Some(Depth::Ansi16),
        ..Style::new(Mode::Rich)
    };
    let out = Screen::new()
        .with(Block::diagnostic(&diagnostic, &()))
        .render(&style);
    assert!(out.contains("\x1b[35m(human)\x1b[0m"), "{out}");
}

#[test]
fn tokens_use_exact_values_only_when_the_theme_is_known() {
    assert_eq!(Token::Failure.sgr(Depth::TrueColor, None), "31");
    assert_eq!(Token::Muted.sgr(Depth::Ansi256, None), "2");
    assert_eq!(
        Token::Failure.sgr(Depth::TrueColor, Some(Theme::Dark)),
        "38;2;255;123;112"
    );
    assert_eq!(
        Token::Accent.sgr(Depth::Ansi256, Some(Theme::Light)),
        "38;5;24"
    );
    assert_eq!(
        Token::Muted.sgr(Depth::Ansi256, Some(Theme::Dark)),
        "38;5;247"
    );
    assert_eq!(Token::Success.sgr(Depth::Ansi16, Some(Theme::Dark)), "32");
    assert_eq!(
        Token::Decision.sgr(Depth::TrueColor, Some(Theme::Light)),
        "38;2;139;61;184"
    );
    assert_eq!(
        Token::Warning.sgr(Depth::Ansi256, Some(Theme::Light)),
        "38;5;94"
    );
}

fn help_screen() -> Screen {
    Screen::new()
        .with(Block::header(
            "jadpo 0.1.0",
            ["a programming environment for coding agents"],
        ))
        .with(Block::list(
            "Set up",
            vec![Row::new("completions", "Turn on Tab completion").extra(["zsh", "bash", "fish"])],
        ))
        .with(Block::list(
            "Help",
            vec![
                Row::new("help", "Show commands, flags and examples"),
                Row::new("version", "Print the version"),
            ],
        ))
        .with(Block::hint(
            "jadpo help completions",
            "shows how to turn on Tab completion",
        ))
}

#[test]
fn lists_share_one_grid_and_blocks_one_blank_line() {
    assert_eq!(
        help_screen().render(&rich()),
        "jadpo 0.1.0 · a programming environment for coding agents

Set up
  completions   Turn on Tab completion        zsh · bash · fish

Help
  help          Show commands, flags and examples
  version       Print the version

→ jadpo help completions  shows how to turn on Tab completion
"
    );
}

#[test]
fn plain_lists_drop_symbols() {
    assert_eq!(
        help_screen().render(&Style::new(Mode::Plain)),
        "jadpo 0.1.0, a programming environment for coding agents

Set up
  completions   Turn on Tab completion        zsh, bash, fish

Help
  help          Show commands, flags and examples
  version       Print the version

next: jadpo help completions  shows how to turn on Tab completion
"
    );
}

#[test]
fn the_extra_column_is_hidden_below_eighty_columns() {
    let style = Style {
        width: 60,
        ..rich()
    };
    let out = help_screen().render(&style);
    assert!(
        out.contains("  completions   Turn on Tab completion\n"),
        "{out}"
    );
}

#[test]
fn coloured_lists_bold_headings_and_mute_secondary_text() {
    let style = Style {
        color: Some(Depth::Ansi16),
        ..Style::new(Mode::Rich)
    };
    let out = help_screen().render(&style);
    assert!(out.starts_with("\x1b[1mjadpo 0.1.0\x1b[0m\x1b[2m · a programming environment for coding agents\x1b[0m\n"), "{out}");
    assert!(out.contains("\n\x1b[1mSet up\x1b[0m\n"), "{out}");
    assert!(out.contains("\x1b[2mzsh · bash · fish\x1b[0m"), "{out}");
    assert!(out.ends_with("\x1b[36m→ jadpo help completions\x1b[0m  \x1b[2mshows how to turn on Tab completion\x1b[0m\n"), "{out}");
}

#[test]
fn long_descriptions_are_cut_with_an_ellipsis() {
    let screen = Screen::new().with(Block::list(
        "Group",
        vec![Row::new("name", "word ".repeat(30))],
    ));
    let out = screen.render(&rich());
    let line = out.lines().nth(1).expect("row");
    assert_eq!(line.chars().count(), 80, "{line}");
    assert!(line.ends_with('…'), "{line}");
    let ascii = screen.render(&Style {
        symbols: Symbols::Ascii,
        ..rich()
    });
    assert!(ascii.lines().nth(1).expect("row").ends_with("..."));
}

#[test]
fn fields_wrap_prose_at_the_value_column() {
    let screen = Screen::new().with(Block::fields(vec![(
        "reason",
        "word ".repeat(20).trim_end().to_string(),
    )]));
    let out = screen.render(&rich());
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 2, "{out}");
    assert!(lines[0].starts_with("  reason    word"));
    assert!(lines[1].starts_with("            word"));
    assert!(lines.iter().all(|line| line.chars().count() <= 80));
}

#[test]
fn commands_are_never_cut() {
    let long = format!("jadpo {}", "x".repeat(100));
    let screen = Screen::new().with(Block::examples(vec![("Run something long", long.clone())]));
    let out = screen.render(&rich());
    assert!(out.contains(&long), "{out}");
}

#[test]
fn examples_put_each_command_on_its_own_line() {
    let screen = Screen::new().with(Block::examples(vec![
        (
            "Turn on Tab completion in zsh",
            "jadpo completions zsh".to_string(),
        ),
        (
            "Turn on Tab completion in fish",
            "jadpo completions fish".to_string(),
        ),
    ]));
    assert_eq!(
        screen.render(&rich()),
        "Examples
  Turn on Tab completion in zsh
    jadpo completions zsh

  Turn on Tab completion in fish
    jadpo completions fish
"
    );
}

fn failed_summary() -> Screen {
    Screen::new().with(Block::summary(
        Outcome::Failure,
        "fmt failed",
        [
            Fact::problem("2 not formatted"),
            Fact::plain("12 formatted"),
        ],
    ))
}

#[test]
fn a_summary_footer_leads_with_the_outcome() {
    assert_eq!(
        failed_summary().render(&rich()),
        "✗ fmt failed  2 not formatted · 12 formatted\n"
    );
    assert_eq!(
        Screen::new()
            .with(Block::summary(
                Outcome::Success,
                "formatted",
                [Fact::plain("14 unchanged")]
            ))
            .render(&rich()),
        "✓ formatted  14 unchanged\n"
    );
}

#[test]
fn a_plain_summary_reads_outcome_colon_facts() {
    assert_eq!(
        failed_summary().render(&Style::new(Mode::Plain)),
        "fmt failed: 2 not formatted, 12 formatted\n"
    );
}

#[test]
fn summary_colour_is_on_the_outcome_and_problem_counts() {
    let style = Style {
        color: Some(Depth::Ansi16),
        ..Style::new(Mode::Rich)
    };
    assert_eq!(
        failed_summary().render(&style),
        "\x1b[1;31m✗ fmt failed\x1b[0m  \x1b[31m2 not formatted\x1b[0m\x1b[2m · 12 formatted\x1b[0m\n"
    );
}

/// Inline code in a label's value is never split across lines: the line
/// breaks before it, and a span longer than the line overflows whole
/// (CLI design language, "Layout": commands are never broken).
#[test]
fn inline_code_in_a_value_wraps_whole() {
    let diagnostic = Diagnostic::new(&rule::TOOLCHAIN_PROJECT_NOT_FOUND, "No project here")
        .reason("Jadpo looked for `app.jadpo` in this folder and every folder above it.")
        .next("Run it inside a project, or give a file or folder: `jadpo check <path>`");
    let screen = Screen::new().with(Block::diagnostic(&diagnostic, &()));
    let out = screen.render(&Style {
        width: 80,
        ..rich()
    });
    assert_eq!(
        out,
        "✗ error  No project here
  reason    Jadpo looked for `app.jadpo` in this folder and every folder above
            it.
  next      Run it inside a project, or give a file or folder:
            `jadpo check <path>` (agent)
  rule      toolchain.project_not_found
  help      https://jadpo.dev/docs/errors/toolchain.project_not_found
"
    );
    let long = Diagnostic::new(&rule::TOOLCHAIN_PROJECT_NOT_FOUND, "No project here")
        .next(format!("Run `jadpo check {}`", "x".repeat(90)));
    let out = Screen::new()
        .with(Block::diagnostic(&long, &()))
        .render(&Style {
            width: 80,
            ..rich()
        });
    assert!(
        out.contains(&format!("\n            `jadpo check {}`", "x".repeat(90))),
        "{out}"
    );
}

/// A watch rule: one line when its long form fits, otherwise the short form
/// and the change list, at most five changes and a count of the rest.
#[test]
fn a_rule_fits_or_lists_its_changes() {
    use jadpo_term::{Change, ChangeMark};
    let change = |mark, path: &str| Change {
        mark,
        path: path.to_string(),
    };
    let rule = |long: &[&str], changes: Vec<Change>| Block::Rule {
        long: long.iter().map(|fact| fact.to_string()).collect(),
        short: vec![
            "14:02:31".to_string(),
            format!("{} files changed", changes.len()),
        ],
        changes,
        plain: "watch".to_string(),
    };
    let fits = Screen::new().with(rule(
        &["14:02:31", "saved app.jadpo", "6 tests reach it"],
        vec![change(ChangeMark::Changed, "app.jadpo")],
    ));
    assert_eq!(
        fits.render(&rich()),
        "── 14:02:31 · saved app.jadpo · 6 tests reach it ──\n"
    );
    assert_eq!(
        fits.render(&Style::new(Mode::Plain)),
        "watch: 14:02:31, saved app.jadpo, 6 tests reach it\n"
    );
    let ascii = Style {
        symbols: Symbols::Ascii,
        ..rich()
    };
    assert_eq!(
        fits.render(&ascii),
        "-- 14:02:31, saved app.jadpo, 6 tests reach it --\n"
    );
    let many: Vec<Change> = (0..7)
        .map(|i| {
            let mark = [ChangeMark::Added, ChangeMark::Changed, ChangeMark::Removed][i % 3];
            change(mark, &format!("domains/stock/actions/a_{i}.jadpo"))
        })
        .collect();
    assert_eq!(
        Screen::new().with(rule(&[], many)).render(&rich()),
        "── 14:02:31 · 7 files changed ──
  + domains/stock/actions/a_0.jadpo
  ~ domains/stock/actions/a_1.jadpo
  - domains/stock/actions/a_2.jadpo
  + domains/stock/actions/a_3.jadpo
  ~ domains/stock/actions/a_4.jadpo
  and 2 more
"
    );
}

/// The live line: a muted label and facts, a problem count in its colour,
/// each key bold; plain output spells the keys out.
#[test]
fn a_key_bar_names_the_keys_that_work() {
    let bar = Screen::new().with(Block::KeyBar {
        label: "watching".to_string(),
        facts: vec![Fact::plain("41 files"), Fact::problem("2 failing")],
        keys: vec![("ctrl-c".to_string(), "quit".to_string())],
    });
    assert_eq!(
        bar.render(&rich()),
        "watching 41 files · 2 failing  ctrl-c quit\n"
    );
    assert_eq!(
        bar.render(&Style::new(Mode::Plain)),
        "watching: 41 files, 2 failing, press ctrl-c to quit\n"
    );
    let coloured = bar.render(&Style {
        color: Some(Depth::Ansi16),
        bold: true,
        ..Style::new(Mode::Rich)
    });
    assert!(
        coloured.contains("\x1b[31m2 failing\x1b[0m"),
        "{coloured:?}"
    );
    assert!(coloured.contains("\x1b[1mctrl-c\x1b[0m"), "{coloured:?}");
}
