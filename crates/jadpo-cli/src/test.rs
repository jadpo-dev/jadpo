//! `jadpo test`: runs the project's tests through `jadpo-test` and prints
//! the run in the CLI design language. A project that doesn't check is
//! refused with its errors; nothing to run is `test.no_tests`; failed
//! tests make the run `test.failed`, exit 1.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use jadpo_diag::{Diagnostic, nearest, rule};
use jadpo_term::{Block, ColorChoice, Fact, Json, Outcome, Screen};
use jadpo_test::{Options, Ran, TestKind};

use crate::fmt::project_not_found;
use crate::interactive::InteractiveArgs;
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
    let mut options = Options {
        cwd: call.cwd.clone().unwrap_or_else(|| PathBuf::from(".")),
        target: call.arguments.first().cloned(),
        name,
        kind,
        failed: call.flag("--failed"),
        seed,
        keep_failed: call.flag("--keep-failed"),
        ci: crate::run::in_ci(&call.environment),
        record: false,
        full: call.flag("--full"),
        since: None,
    };
    if call.flag("--interactive") {
        return interactive(call, options);
    }
    if call.flag("--affected") {
        // A mistake in what to run is said before the base is looked for.
        if let Some(ran) = jadpo_test::watch::validate(&options) {
            return reply(ran);
        }
        let Some(root) = project_root(&options) else {
            return project_not_found("test");
        };
        match crate::before::since(&root) {
            Ok(since) => options.since = Some(Arc::new(since)),
            Err(before) => return Reply::failed(no_base(before)),
        }
    }
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
        // The test database's server lacks an extension the migrations
        // create: the same error `jadpo migrate apply` gives.
        Ran::Unmigrated { store, error } => {
            let diagnostic = crate::migrate::migrate_failed(&store, error);
            (
                Screen::new().with(Block::diagnostic(&diagnostic, &())),
                Json::Null,
                vec![diagnostic],
                BTreeMap::new(),
            )
        }
        Ran::NothingReached { since } => (
            Screen::new().with(Block::summary(
                Outcome::Success,
                "nothing to run",
                [Fact::plain(format!(
                    "no test reaches what changed since {since}"
                ))],
            )),
            Json::object([
                (
                    "selection",
                    Json::object([("affected", Json::from(since.as_str()))]),
                ),
                ("reached", 0.into()),
                ("tests", Json::Array(Vec::new())),
            ]),
            Vec::new(),
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
                let failed = Diagnostic::new(
                        &rule::TEST_FAILED,
                        format!(
                            "{failed} of {ran} {} failed",
                            if ran == 1 { "test" } else { "tests" }
                        ),
                    )
                    .reason("Each failed test's report is in `data.tests`: its code, the values in its assertion or what happened instead, and the command that reruns it.");
                vec![if run.needs_a_person() {
                    // A fault only a person can act on is said at the top,
                    // where an agent reading only this would otherwise be
                    // told to fix it (chunk A13).
                    failed.next_for(
                        "Ask your user about each failure whose `next` ends `(human)`, fix the rest, then run `jadpo test --failed`",
                        jadpo_diag::Decider::Human,
                    )
                } else {
                    failed.next("Fix each failure, then run `jadpo test --failed`")
                }]
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

/// The project the tests run in: the one around the folder the command
/// runs in, or around the path given.
fn project_root(options: &Options) -> Option<PathBuf> {
    jadpo_db::find_root(&options.cwd).or_else(|| {
        let written = options.target.as_deref()?;
        let path = written
            .rsplit_once(':')
            .filter(|(_, line)| line.chars().all(|c| c.is_ascii_digit()))
            .map_or(written, |(path, _)| path);
        jadpo_db::find_root(&options.cwd.join(path))
    })
}

/// Why `--affected` has no base to measure a change against.
fn no_base(before: jadpo_db::Before) -> Diagnostic {
    match before {
        jadpo_db::Before::Missing { base } => Diagnostic::new(
            &rule::CLI_TEST_ARGUMENTS,
            format!("The base to compare with, {base}, isn't in this clone"),
        )
        .reason("`--affected` runs the tests that reach what changed since the merge base with the default branch, and this clone was made without that commit.")
        .next("Fetch the history with `git fetch --unshallow`; in CI, check out with `fetch-depth: 0`"),
        other => {
            let reason = match other {
                jadpo_db::Before::Nothing(reason) => reason
                    .split("; name one with")
                    .next()
                    .unwrap_or_default()
                    .to_string(),
                _ => "git doesn't track the project".to_string(),
            };
            Diagnostic::new(
                &rule::CLI_TEST_ARGUMENTS,
                "`--affected` has nothing to compare with",
            )
            .reason(format!("It runs the tests that reach what changed since the merge base with the default branch, and there is none: {reason}."))
            .next("Run `jadpo test` to run every test")
        }
    }
}

/// `--interactive`: the keyboard view, which takes no selection of its
/// own; whether it has a terminal is decided where the output's form is
/// known ([`crate::run`]).
fn interactive(call: &Call, options: Options) -> Reply {
    let given: Vec<&str> = [
        ("a path", options.target.is_some()),
        ("`--name`", options.name.is_some()),
        ("`--kind`", options.kind.is_some()),
        ("`--failed`", options.failed),
        ("`--affected`", call.flag("--affected")),
    ]
    .into_iter()
    .filter_map(|(what, given)| given.then_some(what))
    .collect();
    if let Some(first) = given.first() {
        return Reply::failed(
            Diagnostic::new(
                &rule::CLI_TEST_ARGUMENTS,
                format!(
                    "`--interactive` takes no {}",
                    first.trim_start_matches("a ")
                ),
            )
            .reason("The keyboard view lists every test in the app, and searches them as you type.")
            .next(format!(
                "Leave out {}, and press `/` in the view to search",
                either(
                    &given
                        .iter()
                        .map(|what| what.to_string())
                        .collect::<Vec<_>>()
                )
            )),
        );
    }
    // Outside a project, it says so before the view opens.
    if let Some(ran) = jadpo_test::watch::validate(&options) {
        return reply(ran);
    }
    Reply::Session(Session::Interactive(Box::new(InteractiveArgs {
        options,
        watch: call.flag("--watch"),
        choice: ColorChoice::Auto,
    })))
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
    let errors = diagnostics.len();
    let mut screen = Screen::new();
    for diagnostic in &diagnostics {
        screen.push(Block::diagnostic_from(
            diagnostic,
            &sources,
            root.as_deref(),
        ));
    }
    let facts = vec![
        Fact::problem(count(errors, "error", "errors")),
        Fact::plain(count(files, "file", "files")),
    ];
    screen.push(Block::summary(Outcome::Failure, "tests not run", facts));
    let data = Json::object([
        ("ran", false.into()),
        ("root", Json::from(root.as_deref().unwrap_or("."))),
        ("fileCount", files.into()),
        ("errors", errors.into()),
    ]);
    (screen, data, diagnostics, sources)
}
