//! `jadpo ai record <prompt> --as <name>` (area 21, "Tests"; chunk
//! 5.5v4b): asks the configured model a prompt once, with the values the
//! prompt's last `jadpo dev` call gave it, or a fixture's with
//! `--fixture`, as the first `ai.generate` call of it asks, and keeps the
//! answer as the prompt's recording, `recordings/<prompt>/<name>.json`
//! beside it: the answer as `json.write` writes the call's type, its plain
//! values placeholders ([`jadpo_runtime::recorded::kept_answer`]), and the
//! prompt's version. A test answers with it through `recorded("<name>")`.
//!
//! It is `jadpo package record`'s recorder (chunk 7.2d2) for a model: the
//! same refusals before anything is sent, through a package's connection
//! never live and never with a stand-in credential; the same placeholders,
//! so no personal data the values carried is committed; and nothing kept
//! when the model refuses, gives no answer, or one that doesn't decode.
//!
//! ```text
//! ✓ Recorded `answer_question`'s answer as `sesame`
//!     asked     local/llama3.2 through local_model
//!     from      the last `jadpo dev` call, call_3
//!     answer    decodes as SupportAnswer
//!     values    1 placeholder, 1 kept as given
//!     file      domains/support/prompts/recordings/answer_question/sesame.json
//!
//! ✓ answer recorded  answer_question@864ad76b · sesame
//!
//! → jadpo test  decodes the recording and answers tests with it
//! ```

use std::path::PathBuf;
use std::rc::Rc;

use jadpo_diag::{Diagnostic, rule};
use jadpo_interp::Program;
use jadpo_runtime::ai::Done;
use jadpo_runtime::call_log::CallLog;
use jadpo_runtime::{Ctx, Env, Outcome as Ran};
use jadpo_std::RuleKey;
use jadpo_std::ai::AiDriver;
use jadpo_term::{Block, Fact, Json, Outcome, Screen};
use jadpo_value::Value;

use crate::fmt::project_needed;
use crate::package_record::{Kept, NextMarked, counts, refused_by_errors, shown_path};
use crate::run::{Call, Reply, both, quoted};
use crate::suggest::nearest;

const COMMAND: &str = "jadpo ai record";

pub fn record(call: &Call) -> Reply {
    match jadpo_interp::on_large_stack(|| recorded(call)) {
        Ok(reply) => reply,
        Err(error) => Reply::failed(
            Diagnostic::new(&rule::RECORDING_FAILED, "The model couldn't be asked")
                .reason(format!(
                    "The interpreter runs on a thread with a large stack, and starting it failed: {error}."
                ))
                .next_marked("Free some memory, then run it again (human)"),
        ),
    }
}

fn arguments(headline: String, reason: String, next: String) -> Diagnostic {
    Diagnostic::new(&rule::CLI_AI_ARGUMENTS, headline)
        .reason(reason)
        .next(next)
}

/// Whether `name` can name a recording: lower_snake_case, as
/// `recorded("<name>")` writes it and its file is named.
fn a_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|first| first.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && !name.ends_with('_')
        && !name.contains("__")
}

fn recorded(call: &Call) -> Reply {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return project_needed("ai record");
    };
    let Some(written) = call.arguments.get(1) else {
        return Reply::failed(arguments(
            "`jadpo ai record` needs a prompt's name".to_string(),
            "It asks the model one prompt and keeps the answer as one of its recordings."
                .to_string(),
            "Run `jadpo ai record <prompt> --as <name>`".to_string(),
        ));
    };
    if let Some(extra) = call.arguments.get(2) {
        return Reply::failed(arguments(
            format!("`jadpo ai record` takes one prompt, not `{extra}` as well"),
            "It records one prompt's answer at a time.".to_string(),
            format!("Run `jadpo ai record {written} --as <name>`"),
        ));
    }
    let key = jadpo_db::to_snake(written);
    let Some(name) = call.value("--as") else {
        return Reply::failed(arguments(
            "`jadpo ai record` needs a name for the recording".to_string(),
            "`--as` names it: the file it is kept in, and what a test's `recorded(\"<name>\")` answers with.".to_string(),
            format!("Run `jadpo ai record {key} --as <name>`, such as `--as sesame`"),
        ));
    };
    if !a_name(name) {
        return Reply::failed(arguments(
            format!("`{name}` can't name a recording"),
            "A recording's name is lower_snake_case, as `recorded(\"<name>\")` writes it and its file is named.".to_string(),
            format!(
                "Run `jadpo ai record {key} --as {}`",
                jadpo_db::to_snake(name).trim_matches('_')
            ),
        ));
    }
    let ready = match crate::dev::prepare(&root, &call.environment, true) {
        Ok(ready) => ready,
        Err((diagnostics, sources)) => return refused_by_errors(diagnostics, sources),
    };
    let loaded = jadpo_db::load(&root);
    let analysis = jadpo_check::prompts::analysis(&loaded.db, loaded.project);
    let keys: Vec<&str> = analysis
        .prompts
        .iter()
        .map(|facts| facts.key.as_str())
        .collect();
    let Some(facts) = analysis.prompt(&key) else {
        let diagnostic = arguments(
            format!("`{written}` isn't a prompt in this project"),
            String::new(),
            String::new(),
        );
        return Reply::failed(match nearest(&key, keys.iter().copied()) {
            Some(near) => diagnostic
                .reason(format!(
                    "No prompt is called `{key}`; the closest is `{near}`."
                ))
                .next(format!("Run `jadpo ai record {near} --as {name}`")),
            None if keys.is_empty() => diagnostic
                .reason("This project declares no prompt.")
                .next("Declare a prompt in a domain's `prompts/` folder"),
            None => diagnostic
                .reason(format!("Its prompts are {}.", both(&quoted(&keys))))
                .next(format!("Run `jadpo ai record {} --as {name}`", keys[0])),
        });
    };
    let program = Program::new(std::sync::Arc::clone(&ready.prepared)).with_config(&ready.config);
    let (Some(site), Some(ai)) = (program.generate_site_of(&key), program.ai()) else {
        return Reply::failed(
            Diagnostic::new(&rule::RECORDING_REFUSED, format!("No call asks `{key}`"))
                .reason("A recording answers an `ai.generate` call of its prompt in a test, and is decoded as the type the call asks for, and no action asks this prompt.")
                .next_marked(format!(
                    "Ask `{key}` from an action with `ai.generate`, then record it (agent)"
                )),
        );
    };
    let Some(entry) = ai.prompts.entries.get(site.entry as usize) else {
        return Reply::failed(
            Diagnostic::new(
                &rule::RECORDING_FAILED,
                format!("`{key}` couldn't be asked"),
            )
            .reason("The program has no prompt of the call's.")
            .next_marked("Report it to Jadpo's maintainers with this report (human)"),
        );
    };
    let connection = ai.plan.connection.clone();
    // A package's connection: never live, never a stand-in credential.
    if matches!(ai.plan.driver, AiDriver::Package(_))
        && let Some(chosen) = ready.prepared.plan().package_connection(&connection)
        && let Some(refused) = crate::package_record::refusal(&ready, &connection, chosen, COMMAND)
    {
        return Reply::failed(*refused);
    }

    // What it is asked with.
    let (value, from, from_json) = match call.value("--fixture") {
        Some(fixture) => match from_fixture(&program, fixture, &entry.type_name) {
            Ok(value) => (
                value,
                format!("fixture `{fixture}`"),
                Json::object([
                    ("kind", Json::from("fixture")),
                    ("fixture", Json::from(fixture)),
                ]),
            ),
            Err(diagnostic) => return Reply::failed(*diagnostic),
        },
        None => {
            let last = crate::dev_calls::read(&root)
                .into_iter()
                .rev()
                .find(|entry| {
                    entry.connection == connection
                        && entry.operation == key
                        && entry.values.is_some()
                });
            let Some(last) = last else {
                return Reply::failed(
                    Diagnostic::new(
                        &rule::RECORDING_REFUSED,
                        format!("Nothing to ask `{key}` with"),
                    )
                    .reason(format!(
                        "`{COMMAND}` asks with the values of the prompt's last call in `jadpo dev`, or a fixture's, and `jadpo dev` hasn't asked `{key}` since its call log was cleared."
                    ))
                    .next_marked(format!(
                        "Ask it once in `jadpo dev`, or run `jadpo ai record {key} --as {name} --fixture <fixture>` (agent)"
                    )),
                );
            };
            let values = last.values.as_deref().unwrap_or_default();
            match jadpo_runtime::prompt_recordings::values_read(&ai, site, values) {
                Ok(value) => (
                    value,
                    format!("the last `jadpo dev` call, {}", last.id),
                    Json::object([
                        ("kind", Json::from("dev_call")),
                        ("call", Json::from(last.id.as_str())),
                    ]),
                ),
                Err(problem) => {
                    let at = if problem.field.is_empty() {
                        String::new()
                    } else {
                        format!(" on `{}`", problem.field)
                    };
                    return Reply::failed(
                        Diagnostic::new(
                            &rule::RECORDING_REFUSED,
                            format!(
                                "The last `jadpo dev` call's values aren't `{}`'s now",
                                entry.type_name
                            ),
                        )
                        .reason(format!(
                            "Reading {}'s values found `{}`{at}: the prompt has changed since `jadpo dev` asked it.",
                            last.id, problem.rule
                        ))
                        .next_marked(format!(
                            "Ask `{key}` again in `jadpo dev`, then record it (agent)"
                        )),
                    );
                }
            }
        }
    };

    // The model asked once, through the connection, as the call asks.
    let seen = Rc::new(Kept::default());
    let host = jadpo_host_native::NativeHost::new()
        .with_drivers(
            jadpo_host_native::Drivers::build(
                program.runtime_plan().connections,
                program.started(),
                &jadpo_host_native::supplies(),
            )
            .keeping(|asked| asked == connection.as_str()),
        )
        .with_call_log(Rc::clone(&seen) as Rc<dyn CallLog>);
    let runtime = match jadpo_host_native::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            return failed(
                format!("`{key}` couldn't be asked"),
                format!("The runtime that sends the call couldn't start: {error}."),
                "Free some memory, then run it again (human)".to_string(),
            );
        }
    };
    let mut ctx = Ctx::of(&program, &host, Env::default());
    ctx.allow_standalone_calls();
    let Some(done) = runtime.block_on(program.ask_model(&mut ctx, &key, value)) else {
        return failed(
            format!("`{key}` couldn't be asked"),
            "No call asks the prompt.".to_string(),
            "Report it to Jadpo's maintainers with this report (human)".to_string(),
        );
    };
    let model = ai
        .plan
        .model(&site.model)
        .and_then(|at| ready.config.text_at(at))
        .map(str::to_string)
        .unwrap_or_else(|| site.model.clone());
    let answer = match done {
        Done::Value(answer) => answer,
        Done::Broke { rule, problem } => {
            let (headline, reason, next) = broke(&key, &connection, rule, problem.as_ref());
            return failed(headline, format!("{reason} Nothing was recorded."), next);
        }
        Done::Failed(failure) if failure.code == "misconfigured" => {
            return failed(
                format!("`{connection}` refused the model or its address"),
                format!(
                    "Asking `{model}` at `{connection}`'s address gave an answer as a server does a model it doesn't have, or the address isn't one Jadpo sends to. Nothing was recorded."
                ),
                "Check `ai: { models: }` and the connection's address in `.env.local`, then run it again (human)".to_string(),
            );
        }
        Done::Failed(_) => {
            return failed(
                format!("The model at `{connection}` didn't answer"),
                "It couldn't be reached, or it said it was busy and asked to be called later. Nothing was recorded.".to_string(),
                "Check the model is running at the connection's address, then run it again (human)".to_string(),
            );
        }
        Done::Fault(kind) => {
            return failed(
                format!("The model's answer to `{key}` couldn't be read"),
                format!(
                    "Reading it stopped with a {} fault. Nothing was recorded.",
                    kind.classification()
                ),
                "Check the connection's address is the model's, then run it again (human)"
                    .to_string(),
            );
        }
    };
    let version = entry.version();
    let Some(text) = jadpo_runtime::prompt_recordings::recording_text(
        &mut ctx, &ai.json, site, &answer, &version,
    ) else {
        return failed(
            format!("The model's answer to `{key}` couldn't be kept"),
            "It decoded as the call's type, but couldn't be written again as its recording."
                .to_string(),
            "Report it to Jadpo's maintainers with this report (human)".to_string(),
        );
    };
    let path = jadpo_check::prompt_recordings::path(&facts.path, &key, name);
    let file = root.join(&path);
    let before = std::fs::read_to_string(&file).ok();
    let changed = before.as_deref() != Some(text.as_str());
    if changed {
        let saved = file
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&file, &text));
        if let Err(error) = saved {
            return crate::package::write_failed("ai record", &path, &error.to_string());
        }
    }
    let shown = shown_path(&root, &file);
    // Of the answer alone: the version is the prompt's, not the model's.
    let answer_text = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|document| {
            document
                .get(jadpo_std::embed::ANSWER)
                .map(ToString::to_string)
        })
        .unwrap_or_default();
    let (placeholders, as_given) = counts(&answer_text);
    let mut values = crate::package::plural(placeholders, "placeholder", "placeholders");
    if as_given > 0 {
        values.push_str(&format!(", {as_given} kept as given"));
    }
    let type_name = match &answer {
        Value::Record(record) => Some(record.ty.to_string()),
        Value::Variant(variant) => Some(variant.ty.to_string()),
        _ => None,
    };
    let kept = seen.0.borrow().clone();
    let subject = format!("`{key}`'s answer as `{name}`");
    let mut screen = Screen::new().with(Block::Status {
        outcome: Outcome::Success,
        text: match (changed, before.is_some()) {
            (true, false) => format!("Recorded {subject}"),
            (true, true) => format!("Recorded {subject} again"),
            (false, _) => format!("Recorded {subject}, as before"),
        },
        rows: vec![
            ("asked".to_string(), format!("{model} through {connection}")),
            ("from".to_string(), from),
            (
                "answer".to_string(),
                match &type_name {
                    Some(type_name) => format!("decodes as {type_name}"),
                    None => "decodes as the call's type".to_string(),
                },
            ),
            ("values".to_string(), values),
            ("file".to_string(), shown.clone()),
        ],
    });
    screen.push(Block::summary(
        Outcome::Success,
        "answer recorded",
        [Fact::plain(version.clone()), Fact::plain(name.to_string())],
    ));
    screen.push(Block::hint(
        "jadpo test",
        "decodes the recording and answers tests with it",
    ));
    Reply::screen(
        screen,
        Json::object([
            ("prompt", Json::from(key.as_str())),
            ("name", Json::from(name)),
            ("version", Json::from(version.as_str())),
            ("file", Json::from(shown.as_str())),
            ("written", changed.into()),
            ("connection", Json::from(connection.as_str())),
            ("model", Json::from(model.as_str())),
            ("from", from_json),
            ("placeholders", Json::from(placeholders as i64)),
            ("keptAsGiven", Json::from(as_given as i64)),
            (
                "sent",
                Json::object([
                    ("method", Json::from(kept.method.as_str())),
                    ("url", Json::from(kept.url.as_str())),
                ]),
            ),
        ]),
    )
}

fn failed(headline: String, reason: String, next: String) -> Reply {
    Reply::failed(
        Diagnostic::new(&rule::RECORDING_FAILED, headline)
            .reason(reason)
            .next_marked(next),
    )
}

/// What a condition the call ended in says: headline, reason and next.
fn broke(
    key: &str,
    connection: &str,
    rule: RuleKey,
    problem: Option<&Value>,
) -> (String, String, String) {
    match rule {
        RuleKey::Refused => (
            format!("The model declined to answer `{key}`"),
            "A recording keeps an answer; a refusal is one of the call's conditions, which a test scripts as `refused` instead.".to_string(),
            "Ask with other values, or script `refused` in the test (agent)".to_string(),
        ),
        RuleKey::Malformed => {
            let field = |name: &str| match problem {
                Some(Value::Record(record)) => match record.get(name) {
                    Some(Value::Text(text)) => text.to_string(),
                    _ => String::new(),
                },
                _ => String::new(),
            };
            let (at, broke) = (field("field"), field("rule"));
            let found = if at.is_empty() {
                format!("`{broke}`")
            } else {
                format!("`{broke}` on `{at}`")
            };
            (
                format!("The model's answer to `{key}` doesn't decode as the call's type"),
                format!("The last of the attempts the call allows found {found}."),
                "Run it again, or make the prompt clearer about the answer, then record it (agent)"
                    .to_string(),
            )
        }
        RuleKey::OverBudget => (
            format!("The model's answer to `{key}` was cut off"),
            "It reached the call's `max_output:` before it ended.".to_string(),
            "Raise the call's `max_output:`, then run it again (agent)".to_string(),
        ),
        RuleKey::TooLong => (
            format!("`{key}` is longer than the model takes"),
            "With these values the prompt is longer than the model's context.".to_string(),
            "Ask with shorter values, then run it again (agent)".to_string(),
        ),
        _ => (
            format!("The model at `{connection}` gave no answer to `{key}` in time"),
            "No whole answer came within the call's `timeout:`.".to_string(),
            "Check the model is running, then run it again (human)".to_string(),
        ),
    }
}

/// The prompt value fixture `named` makes (`--fixture`): a fixture of
/// the project that takes nothing and returns a `type_name`, run as a test
/// calls one, with no store and every outside call faked.
fn from_fixture(program: &Program, named: &str, type_name: &str) -> Result<Value, Box<Diagnostic>> {
    let fixtures: Vec<(u32, &str)> = program
        .plan()
        .callables()
        .iter()
        .enumerate()
        .filter(|(_, callable)| callable.key.kind() == Some(jadpo_graph::Kind::Fixture))
        .filter_map(|(at, callable)| Some((u32::try_from(at).ok()?, callable.key.name())))
        .collect();
    let own = |name: &str| name.rsplit('.').next().unwrap_or(name).to_string();
    let found = fixtures
        .iter()
        .find(|(_, name)| *name == named || own(name) == named);
    let Some((id, _)) = found else {
        let names: Vec<String> = fixtures.iter().map(|(_, name)| own(name)).collect();
        let diagnostic = arguments(
            format!("`{named}` isn't a fixture in this project"),
            String::new(),
            String::new(),
        );
        return Err(Box::new(match nearest(named, names.iter().map(String::as_str)) {
            Some(near) => diagnostic
                .reason(format!("No fixture is called `{named}`; the closest is `{near}`."))
                .next(format!("Use `--fixture {near}`")),
            None => diagnostic
                .reason(format!(
                    "`--fixture` names a fixture that takes nothing and returns a `{type_name}`, in a domain's `fixtures/` folder."
                ))
                .next(format!(
                    "Write a fixture that returns a `{type_name}`, then run it again"
                )),
        }));
    };
    let wrong = |reason: String| {
        Box::new(arguments(
            format!("Fixture `{named}` doesn't make a `{type_name}`"),
            reason,
            format!("Give `--fixture` a fixture that takes nothing and returns a `{type_name}`"),
        ))
    };
    let takes = program
        .plan()
        .callable(*id)
        .map_or(0, |callable| callable.body.params.len());
    if takes > 0 {
        return Err(wrong(format!(
            "It takes {}, and `jadpo ai record` has nothing to give it.",
            crate::package::plural(takes, "parameter", "parameters")
        )));
    }
    match program.call_id(*id, Vec::new()) {
        Ok(Ran::Value(Value::Record(record))) if &*record.ty == type_name => {
            Ok(Value::Record(record))
        }
        Ok(Ran::Value(_)) => Err(wrong(format!("It returns something other than a `{type_name}`."))),
        Ok(Ran::Failed(failure)) => Err(Box::new(
            Diagnostic::new(
                &rule::RECORDING_FAILED,
                format!("Fixture `{named}` failed with `{}`", failure.code),
            )
            .reason("It ran as a test calls it, with no database and every outside call faked, and a failure left it. Nothing was asked.")
            .next_marked(format!("Run a test that calls `{named}` to see why (agent)")),
        )),
        Ok(Ran::Fault(fault)) => Err(Box::new(
            Diagnostic::new(
                &rule::RECORDING_FAILED,
                format!("Fixture `{named}` stopped with a fault"),
            )
            .reason(format!(
                "It ran as a test calls it, with no database and every outside call faked, and stopped with a {} fault: a fixture that reads or writes records can't make the values here. Nothing was asked.",
                fault.kind.classification()
            ))
            .next_marked(format!(
                "Build the `{type_name}` without the database in a fixture of its own, then run it again (agent)"
            )),
        )),
        Ok(Ran::Assertion(_)) | Err(_) => Err(wrong("It didn't give a value.".to_string())),
    }
}
