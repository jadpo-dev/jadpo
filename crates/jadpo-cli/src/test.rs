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

/// The `--kind` that runs a contract run (chunk K4c): a selection of
/// the tests of every kind that reach a provider, not a kind of its own.
const CONTRACT: &str = "contract";

pub fn test(call: &Call) -> Reply {
    let contract = call.value("--kind") == Some(CONTRACT);
    let kind = match call.value("--kind") {
        None => None,
        Some(CONTRACT) => None,
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
        contract: None,
    };
    if call.flag("--interactive") {
        return interactive(call, options);
    }
    if contract && call.flag("--watch") {
        return Reply::failed(
            Diagnostic::new(
                &rule::CLI_TEST_ARGUMENTS,
                "`--kind contract` doesn't watch",
            )
            .reason("A contract run sends its cases' calls to the provider's sandbox, so it runs once, as a scheduled job runs it, rather than on every save.")
            .next("Leave out `--watch`, or run `jadpo test --watch` for the tests that send nothing"),
        );
    }
    let contract_root = if contract {
        let Some(root) = project_root(&options) else {
            return project_not_found("test");
        };
        // The connections' settings, read as `jadpo dev` reads them.
        match crate::config::Layered::new(&root, &call.environment) {
            Ok(layered) => {
                options.contract = Some(Arc::new(jadpo_test::contract::Contract {
                    variables: Box::new(layered),
                }));
            }
            Err(problem) => return Reply::failed(*problem),
        }
        Some(root)
    } else {
        None
    };
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
    let ran = jadpo_test::run(options);
    match contract_root {
        Some(root) => contract_reply(ran, call, &root),
        None => reply(ran),
    }
}

/// A contract run's reply (area 11, "Tests and fakes"; chunk K4c): its
/// cases' reports as `jadpo test` shows them, each operation no case sent
/// to the sandbox, then each suite of the app's own integrations taken
/// through `jadpo package conform --live`, and one footer for it all.
fn contract_reply(ran: Ran, call: &Call, root: &std::path::Path) -> Reply {
    let Ran::Done(run) = ran else {
        return reply(ran);
    };
    let Some(found) = run.contract.clone() else {
        return reply(Ran::Done(run));
    };
    let (failed, passed, cases) = (run.failed(), run.passed(), run.results.len());
    let started = std::time::Instant::now();
    let elapsed = run.elapsed;
    let root_shown = run.root.clone();
    let (screen, data, mut diagnostics, mut sources) = view(Ran::Done(run));
    // The run's own footer gives way to the contract run's.
    let mut blocks = screen.into_blocks();
    if let Some(at) = blocks
        .iter()
        .rposition(|block| matches!(block, Block::Summary { .. }))
    {
        blocks.truncate(at);
    }
    let mut screen = Screen::new();
    for block in blocks {
        screen.push(block);
    }
    let uncovered: Vec<Diagnostic> = found
        .uncovered
        .iter()
        .map(jadpo_test::contract::Uncovered::diagnostic)
        .collect();
    for diagnostic in &uncovered {
        screen.push(Block::diagnostic_from(
            diagnostic,
            &found.sources,
            root_shown.as_deref(),
        ));
    }
    sources.extend(found.sources.clone());
    diagnostics.extend(uncovered);
    // Each suite, through the app's own connection.
    let mut suites = Vec::new();
    for suite in &found.suites {
        let asked = Call {
            arguments: vec![
                "conform".to_string(),
                suite.package.clone(),
                suite.capability.clone(),
            ],
            flags: vec!["--live"],
            values: Vec::new(),
            cwd: Some(root.to_path_buf()),
            environment: call.environment.clone(),
        };
        let (data, problems, frames) = match crate::package_conform::conform(&asked) {
            Reply::Report {
                data,
                diagnostics,
                sources,
                ..
            } => (data, diagnostics, sources),
            Reply::Failed(problem) => (Json::Null, vec![*problem], BTreeMap::new()),
            _ => (Json::Null, Vec::new(), BTreeMap::new()),
        };
        let conformed =
            crate::package_conform::conformed_from(suite.capability.clone(), &data);
        let conforms = conformed.conforms && problems.is_empty();
        for problem in &problems {
            screen.push(Block::diagnostic_from(problem, &frames, None));
        }
        sources.extend(frames);
        diagnostics.extend(problems);
        suites.push((suite, conformed, conforms));
    }
    // The footer: the cases, the operations no case sent, each suite.
    let mut facts = Vec::new();
    if failed > 0 && passed > 0 {
        facts.push(Fact::plain(format!("{passed} passed")));
    }
    facts.push(Fact::plain(count(cases, "contract case", "contract cases")));
    if !found.uncovered.is_empty() {
        facts.push(Fact::problem(count(
            found.uncovered.len(),
            "operation with no case",
            "operations with no case",
        )));
    }
    for (suite, _, conforms) in &suites {
        facts.push(if *conforms {
            Fact::plain(format!("{} conforms live", suite.capability))
        } else {
            Fact::problem(format!("{} doesn't conform live", suite.capability))
        });
    }
    let took = elapsed + started.elapsed();
    if took >= std::time::Duration::from_millis(100)
        && let Some(last) = facts.last_mut()
    {
        last.text.push_str(&format!(" in {:.1} s", took.as_secs_f64()));
    }
    let problems = !found.uncovered.is_empty() || suites.iter().any(|(_, _, ok)| !ok);
    let footer = if failed > 0 {
        Block::summary(Outcome::Failure, format!("{failed} failed"), facts)
            .with_plain_headline("test failed")
    } else if problems {
        Block::summary(Outcome::Failure, "contract run failed", facts)
            .with_plain_headline("test failed")
    } else if cases == 0 {
        Block::summary(Outcome::Success, "contract run passed", facts)
            .with_plain_headline("tested")
    } else {
        Block::summary(Outcome::Success, format!("{passed} passed"), facts)
            .with_plain_headline("tested")
    };
    screen.push(footer);
    let contract = Json::object([
        ("cases", cases.into()),
        ("whole", found.whole.into()),
        (
            "uncovered",
            Json::array(found.uncovered.iter().map(|operation| {
                Json::object([
                    ("call", Json::from(operation.call.as_str())),
                    (
                        "file",
                        operation
                            .at
                            .as_ref()
                            .map_or(Json::Null, |(file, _, _)| Json::from(file.as_str())),
                    ),
                ])
            })),
        ),
        (
            "suites",
            Json::array(suites.iter().map(|(suite, conformed, conforms)| {
                let mut json = conformed.json();
                if let Json::Object(entries) = &mut json {
                    entries.retain(|(key, _)| key != "digest" && key != "against");
                    for (key, value) in entries.iter_mut() {
                        if key == "conforms" {
                            *value = (*conforms).into();
                        }
                    }
                    entries.insert(0, ("package".to_string(), Json::from(suite.package.as_str())));
                    entries.insert(
                        1,
                        ("connection".to_string(), Json::from(suite.connection.as_str())),
                    );
                    entries.push(("against".to_string(), Json::from("live")));
                }
                json
            })),
        ),
    ]);
    let data = match data {
        Json::Object(mut entries) => {
            entries.push(("contract".to_string(), contract));
            Json::Object(entries)
        }
        _ => Json::object([("contract", contract)]),
    };
    Reply::Report {
        screen,
        data,
        diagnostics,
        sources,
    }
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
        ("`--kind`", call.value("--kind").is_some()),
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
    let mut words: Vec<&str> = TestKind::ALL.iter().map(|kind| kind.word()).collect();
    words.push(CONTRACT);
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
