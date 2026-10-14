//! `jadpo test`: runs the project's tests through `jadpo-test` and prints
//! the run in the CLI design language. A project that doesn't check is
//! refused with its errors; nothing to run is `test.no_tests`; failed
//! tests make the run `test.failed`, exit 1.

use std::collections::BTreeMap;
use std::path::PathBuf;

use jadpo_diag::{Diagnostic, Severity, nearest, rule};
use jadpo_term::{Block, Fact, Json, Outcome, Screen};
use jadpo_test::{Options, Ran, TestKind};

use crate::fmt::project_not_found;
use crate::run::{Call, Reply, either, quoted};

pub fn test(call: &Call) -> Reply {
    let kind = match call.value("--kind") {
        None => None,
        Some(word) => match TestKind::from_word(word) {
            Some(kind) => Some(kind),
            None => return Reply::failed(unknown_kind(word)),
        },
    };
    let seed = match call.value("--seed") {
        None => None,
        Some(text) => match text.parse::<u64>() {
            Ok(seed) => Some(seed),
            Err(_) => {
                return Reply::failed(
                    Diagnostic::new(
                        &rule::CLI_TEST_ARGUMENTS,
                        format!("`{text}` isn't an order seed"),
                    )
                    .reason("An order seed is a whole number of 0 or more, as a failed test's rerun command shows it.")
                    .next("Give the seed from the rerun command, such as `--seed 48213`"),
                );
            }
        },
    };
    let name = match call.value("--name") {
        Some(name) if name.trim().is_empty() => {
            return Reply::failed(
                Diagnostic::new(&rule::CLI_TEST_ARGUMENTS, "`--name` needs some text")
                    .reason("`--name` runs the tests whose names contain the text it is given.")
                    .next("Give part of a test's name after `--name`"),
            );
        }
        name => name.map(str::to_string),
    };
    let options = Options {
        cwd: call.cwd.clone().unwrap_or_else(|| PathBuf::from(".")),
        target: call.arguments.first().cloned(),
        name,
        kind,
        failed: call.flag("--failed"),
        seed,
    };
    match jadpo_test::run(options) {
        Ran::NoProject => project_not_found("test"),
        Ran::Refused {
            diagnostics,
            sources,
            files,
        } => refused(diagnostics, sources, files),
        Ran::Stopped(diagnostic) if diagnostic.rule.id.starts_with("cli.") => {
            Reply::Failed(diagnostic)
        }
        Ran::Stopped(diagnostic) => Reply::Report {
            screen: Screen::new().with(Block::diagnostic(&diagnostic, &())),
            data: Json::Null,
            diagnostics: vec![*diagnostic],
            sources: BTreeMap::new(),
        },
        Ran::NothingToRerun => Reply::screen(
            Screen::new().with(Block::summary(
                Outcome::Success,
                "nothing to rerun",
                [Fact::plain("no test failed last time")],
            )),
            Json::object([
                ("selection", Json::object([("failed", true.into())])),
                ("tests", Json::Array(Vec::new())),
            ]),
        ),
        Ran::Done(run) => {
            let failed = run.failed();
            let diagnostics = if failed > 0 {
                let ran = run.results.len();
                vec![
                    Diagnostic::new(
                        &rule::TEST_FAILED,
                        format!(
                            "{failed} of {ran} {} failed",
                            if ran == 1 { "test" } else { "tests" }
                        ),
                    )
                    .reason("Each failed test's report is in `data.tests`: its code, the values in its assertion or what happened instead, and the command that reruns it.")
                    .next("Fix each failure, then run `jadpo test --failed`"),
                ]
            } else {
                Vec::new()
            };
            Reply::Report {
                screen: jadpo_test::screen(&run),
                data: jadpo_test::json(&run),
                diagnostics,
                sources: BTreeMap::new(),
            }
        }
    }
}

fn unknown_kind(word: &str) -> Diagnostic {
    let words: Vec<&str> = TestKind::ALL.iter().map(|kind| kind.word()).collect();
    let all = either(&quoted(&words));
    let diagnostic = Diagnostic::new(
        &rule::CLI_TEST_ARGUMENTS,
        format!("`{word}` isn't a kind of test"),
    );
    match nearest(word, words.iter().copied()) {
        Some(near) => diagnostic
            .reason(format!("A test is {all}. Did you mean `{near}`?"))
            .next(format!("Use `--kind {near}`")),
        None => diagnostic
            .reason(format!("A test is {all}, from what it calls."))
            .next("Use `--kind pure` or `--kind action`"),
    }
}

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The project's errors, then why nothing ran.
fn refused(diagnostics: Vec<Diagnostic>, sources: BTreeMap<String, String>, files: usize) -> Reply {
    let errors = diagnostics
        .iter()
        .filter(|d| d.severity() == Severity::Error)
        .count();
    let warnings = diagnostics.len() - errors;
    let mut screen = Screen::new();
    for diagnostic in &diagnostics {
        screen.push(Block::diagnostic(diagnostic, &sources));
    }
    let mut facts = vec![Fact::problem(count(errors, "error", "errors"))];
    if warnings > 0 {
        facts.push(Fact::warning(count(warnings, "warning", "warnings")));
    }
    facts.push(Fact::plain(count(files, "file", "files")));
    screen.push(Block::summary(Outcome::Failure, "tests not run", facts));
    let data = Json::object([
        ("ran", false.into()),
        ("files", files.into()),
        ("errors", errors.into()),
        ("warnings", warnings.into()),
    ]);
    Reply::Report {
        screen,
        data,
        diagnostics,
        sources,
    }
}
