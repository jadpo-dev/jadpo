//! `jadpo test`: runs the project's tests through `jadpo-test` and prints
//! the run in the CLI design language. A project that doesn't check is
//! refused with its errors; nothing to run is `test.no_tests`; failed
//! tests make the run `test.failed`, exit 1.

use std::collections::BTreeMap;
use std::path::PathBuf;

use jadpo_diag::{Diagnostic, Severity, nearest, rule};
use jadpo_term::{Block, ColorChoice, Fact, Json, Outcome, Screen};
use jadpo_test::{Options, Ran, TestKind};

use crate::fmt::project_not_found;
use crate::run::{Call, Reply, Session, either, quoted};
use crate::watch::WatchArgs;

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
        keep_failed: call.flag("--keep-failed"),
    };
    if call.flag("--watch") {
        // Mistakes in what to run end the command before it starts
        // watching; everything after (errors in the code, no tests yet)
        // is shown and waited out.
        return match jadpo_test::watch::validate(&options) {
            Some(ran) => reply(ran),
            None => Reply::Session(Session::Watch(Box::new(WatchArgs {
                options,
                json: false,
                choice: ColorChoice::Auto,
            }))),
        };
    }
    reply(jadpo_test::run(options))
}

/// What a request to run tests ended in, as a reply.
pub(crate) fn reply(ran: Ran) -> Reply {
    match ran {
        Ran::NoProject => project_not_found("test"),
        Ran::Stopped(diagnostic) if diagnostic.rule.id.starts_with("cli.") => {
            Reply::Failed(diagnostic)
        }
        ran => {
            let (screen, data, diagnostics, sources) = view(ran);
            Reply::Report {
                screen,
                data,
                diagnostics,
                sources,
            }
        }
    }
}

/// A run, or why nothing ran, as a screen, the data for `--json`, and the
/// diagnostics with their sources.
pub(crate) fn view(ran: Ran) -> (Screen, Json, Vec<Diagnostic>, BTreeMap<String, String>) {
    match ran {
        Ran::NoProject => (Screen::new(), Json::Null, Vec::new(), BTreeMap::new()),
        Ran::Refused {
            diagnostics,
            sources,
            files,
            root,
        } => refused(diagnostics, sources, files, root),
        Ran::Stopped(diagnostic) => (
            Screen::new().with(Block::diagnostic(&diagnostic, &())),
            Json::Null,
            vec![*diagnostic],
            BTreeMap::new(),
        ),
        Ran::NothingToRerun { gone } => (
            Screen::new().with(Block::summary(
                Outcome::Success,
                "nothing to rerun",
                [Fact::plain(match gone {
                    0 => "no test failed last time".to_string(),
                    1 => "the test that failed last time is gone".to_string(),
                    n => format!("the {n} tests that failed last time are gone"),
                })],
            )),
            Json::object([
                ("selection", Json::object([("failed", true.into())])),
                ("gone", gone.into()),
                ("tests", Json::Array(Vec::new())),
            ]),
            Vec::new(),
            BTreeMap::new(),
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
            (
                jadpo_test::screen(&run),
                jadpo_test::json(&run),
                diagnostics,
                BTreeMap::new(),
            )
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
fn refused(
    diagnostics: Vec<Diagnostic>,
    sources: BTreeMap<String, String>,
    files: usize,
    root: Option<String>,
) -> (Screen, Json, Vec<Diagnostic>, BTreeMap<String, String>) {
    let errors = diagnostics
        .iter()
        .filter(|d| d.severity() == Severity::Error)
        .count();
    let warnings = diagnostics.len() - errors;
    let mut screen = Screen::new();
    for diagnostic in &diagnostics {
        screen.push(Block::diagnostic_from(
            diagnostic,
            &sources,
            root.as_deref(),
        ));
    }
    let mut facts = vec![Fact::problem(count(errors, "error", "errors"))];
    if warnings > 0 {
        facts.push(Fact::warning(count(warnings, "warning", "warnings")));
    }
    facts.push(Fact::plain(count(files, "file", "files")));
    screen.push(Block::summary(Outcome::Failure, "tests not run", facts));
    let data = Json::object([
        ("ran", false.into()),
        ("root", Json::from(root.as_deref().unwrap_or("."))),
        ("fileCount", files.into()),
        ("errors", errors.into()),
        ("warnings", warnings.into()),
    ]);
    (screen, data, diagnostics, sources)
}
