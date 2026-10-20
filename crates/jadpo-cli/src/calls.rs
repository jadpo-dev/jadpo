//! `jadpo calls` (area 17, "Provider calls in development: `jadpo
//! calls`"; area 11; chunk 7.2d): the provider calls `jadpo dev` made,
//! faked or let through to the sandbox, from the call log it keeps in
//! `build/dev/`.
//!
//! - `jadpo calls` lists them, oldest first, filtered by `--connection`,
//!   `--operation` or `--request`: each call's id, operation, where its
//!   answer came from, how it ended, how long it took and its request.
//! - `jadpo calls show <id>` prints one in full: what was sent and what
//!   came back, values and all, never a credential.
//! - `jadpo calls script <operation> <answer>` sets the next answer of an
//!   operation `jadpo dev` fakes: one of its failures by its code, or a
//!   condition (`unavailable`, `lost`, `in_use`, `misconfigured`), as a
//!   test's `fakes:` writes it (area 13), so an agent walks a failure
//!   path by hand; and of a prompt asked through a package filling `ai`,
//!   `ai.<prompt>`, one of `ai.generate`'s five conditions (area 21;
//!   chunk 5.5v3b).

use std::path::{Path, PathBuf};

use jadpo_diag::{Diagnostic, rule};
use jadpo_graph::body::FakeCondition;
use jadpo_term::{Block, Cell, Fact, Json, Outcome, Screen, Symbol, Table, Token};

use crate::dev_calls::{self, Entry};
use crate::run::{Call, Reply};

pub(crate) const ACTIONS: &[crate::table::Value] = &[
    crate::table::Value {
        name: "list",
        description: "List the calls, oldest first",
    },
    crate::table::Value {
        name: "show",
        description: "Show one call in full",
    },
    crate::table::Value {
        name: "script",
        description: "Set an operation's next answer",
    },
];

pub fn calls(call: &Call) -> Reply {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return crate::fmt::project_not_found("calls");
    };
    match call.arguments.first().map(String::as_str) {
        None | Some("list") => {
            if call.arguments.len() > 1 {
                return Reply::failed(arguments(
                    "`jadpo calls` lists the calls and takes nothing more".to_string(),
                    format!("It was given `{}`.", call.arguments[1..].join(" ")),
                    "Filter with `--connection`, `--operation` or `--request`".to_string(),
                ));
            }
            list(call, &root)
        }
        Some("show") => match call.arguments.get(1) {
            Some(id) if call.arguments.len() == 2 => show(&root, id),
            _ => Reply::failed(arguments(
                "`jadpo calls show` takes one call's id".to_string(),
                "Each call has an id, `call_31`, which `jadpo calls` lists.".to_string(),
                "Run `jadpo calls show <id>` with an id `jadpo calls` lists".to_string(),
            )),
        },
        Some("script") => match (call.arguments.get(1), call.arguments.get(2)) {
            (Some(operation), Some(answer)) if call.arguments.len() == 3 => {
                script(&root, operation, answer)
            }
            _ => Reply::failed(arguments(
                "`jadpo calls script` takes an operation and its next answer".to_string(),
                "It scripts the next answer of one provider operation `jadpo dev` fakes: `jadpo calls script <connection>.<resource>.<operation> <answer>`.".to_string(),
                "Run `jadpo calls script <connection>.<resource>.<operation> <answer>`".to_string(),
            )),
        },
        Some(other) => {
            let names: Vec<&str> = ACTIONS.iter().map(|action| action.name).collect();
            let near = jadpo_diag::nearest(other, names.iter().copied());
            Reply::failed(arguments(
                format!("`{other}` isn't something `jadpo calls` does"),
                format!(
                    "`jadpo calls` lists calls, shows one or scripts an answer: {}.{}",
                    crate::run::either(&crate::run::quoted(names.iter())),
                    near.map(|near| format!(" Did you mean `{near}`?"))
                        .unwrap_or_default()
                ),
                match near {
                    Some(near) => format!("Use `jadpo calls {near}`"),
                    None => "Run `jadpo calls` to list the calls".to_string(),
                },
            ))
        }
    }
}

fn arguments(headline: String, reason: String, next: String) -> Diagnostic {
    Diagnostic::new(&rule::CLI_CALLS_ARGUMENTS, headline)
        .reason(reason)
        .next(next)
}

// --- list ---------------------------------------------------------------------

fn list(call: &Call, root: &Path) -> Reply {
    let all = dev_calls::read(root);
    let connection = call.value("--connection");
    let operation = call.value("--operation");
    let request = call.value("--request");
    let shown: Vec<&Entry> = all
        .iter()
        .filter(|entry| connection.is_none_or(|wanted| entry.connection == wanted))
        .filter(|entry| {
            operation.is_none_or(|wanted| entry.name() == wanted || entry.operation == wanted)
        })
        .filter(|entry| {
            request.is_none_or(|wanted| {
                entry
                    .request_id
                    .as_deref()
                    .is_some_and(|id| id.starts_with(wanted))
            })
        })
        .collect();
    let data = Json::object([(
        "calls",
        Json::array(shown.iter().map(|entry| entry_json(entry))),
    )]);
    let mut screen = Screen::new();
    if shown.is_empty() {
        if all.is_empty() {
            screen.push(Block::summary(
                Outcome::Success,
                "No calls yet".to_string(),
                Vec::new(),
            ));
            screen.push(Block::hint(
                "jadpo dev",
                "runs the app, logging each provider call",
            ));
        } else {
            let filters: Vec<Fact> = [
                ("--connection", connection),
                ("--operation", operation),
                ("--request", request),
            ]
            .into_iter()
            .filter_map(|(flag, value)| Some(Fact::plain(format!("{flag} {}", value?))))
            .collect();
            screen.push(Block::summary(
                Outcome::Success,
                "No calls match".to_string(),
                filters,
            ));
        }
        return Reply::screen(screen, data);
    }
    let mut table = Table::new(["call", "operation", "source", "answer", "time", "request"])
        .right(4)
        .cut(1);
    for entry in &shown {
        table = table.row(vec![
            Cell::text(entry.id.clone()),
            Cell::text(entry.name()),
            Cell::muted(entry.source.clone()),
            answer_cell(entry),
            Cell::muted(
                entry
                    .ms
                    .map_or_else(|| "–".to_string(), |ms| format!("{ms} ms")),
            ),
            Cell::muted(
                entry
                    .request_id
                    .as_deref()
                    .map_or_else(|| "–".to_string(), short_request),
            ),
        ]);
    }
    screen.push(Block::Table(table));
    let count = |ended: &[&str]| {
        shown
            .iter()
            .filter(|entry| ended.contains(&entry.ended.as_str()))
            .count()
    };
    let mut facts = vec![Fact::plain(format!("{} answered", count(&["answered"])))];
    let failed = count(&["failed", "unavailable", "misconfigured", "refused"]);
    if failed > 0 {
        facts.push(Fact {
            text: format!("{failed} failed"),
            tone: Some(Token::Failure),
        });
    }
    let unknown = count(&["outcome_unknown", "in_flight"]);
    if unknown > 0 {
        facts.push(Fact {
            text: format!("{unknown} outcome unknown"),
            tone: Some(Token::Decision),
        });
    }
    screen.push(
        Block::summary(
            Outcome::Success,
            match shown.len() {
                1 => "1 call".to_string(),
                n => format!("{n} calls"),
            },
            facts,
        )
        .with_plain_headline("calls"),
    );
    if let Some(last) = shown.last() {
        screen.push(Block::hint(
            format!("jadpo calls show {}", last.id),
            "shows what was sent and what came back",
        ));
    }
    Reply::screen(screen, data)
}

/// How a call ended, as a state: answered, or what came back instead.
fn answer_cell(entry: &Entry) -> Cell {
    match entry.ended.as_str() {
        "answered" => Cell::state(Symbol::Success, "answered"),
        "failed" => Cell::state(
            Symbol::Failure,
            entry.code.clone().unwrap_or_else(|| "failed".to_string()),
        ),
        "unavailable" => Cell::state(Symbol::Warning, "unavailable"),
        "outcome_unknown" => Cell::state(Symbol::Decision, "outcome unknown"),
        "misconfigured" => Cell::state(Symbol::Failure, "misconfigured"),
        "in_flight" => Cell::state(Symbol::Running, "in flight"),
        other => Cell::state(Symbol::Skipped, other.replace('_', " ")),
    }
}

/// A request id as `jadpo dev`'s request lines show it: `req_739b`.
fn short_request(id: &str) -> String {
    id.chars().take(8).collect()
}

fn entry_json(entry: &Entry) -> Json {
    let text = |value: Option<&str>| value.map_or(Json::Null, Json::from);
    Json::object([
        ("id", Json::from(entry.id.as_str())),
        ("at", Json::from(entry.at.as_str())),
        ("connection", Json::from(entry.connection.as_str())),
        ("operation", Json::from(entry.operation.as_str())),
        ("source", Json::from(entry.source.as_str())),
        ("requestId", text(entry.request_id.as_deref())),
        (
            "ms",
            entry.ms.map_or(Json::Null, |ms| {
                Json::from(i64::try_from(ms).unwrap_or(i64::MAX))
            }),
        ),
        ("method", Json::from(entry.method.as_str())),
        ("url", Json::from(entry.url.as_str())),
        ("body", text(entry.body.as_deref())),
        ("answer", text(entry.answer.as_deref())),
        ("ended", Json::from(entry.ended.as_str())),
        ("code", text(entry.code.as_deref())),
    ])
}

// --- show ---------------------------------------------------------------------

fn show(root: &Path, id: &str) -> Reply {
    let all = dev_calls::read(root);
    let Some(entry) = all.iter().find(|entry| entry.id == id) else {
        let near = jadpo_diag::nearest(id, all.iter().map(|entry| entry.id.as_str()));
        return Reply::failed(arguments(
            format!("There is no call `{id}`"),
            format!(
                "The log keeps the last {} calls `jadpo dev` made{}.",
                dev_calls::KEPT,
                match all.last() {
                    Some(last) => format!(", the newest `{}`", last.id),
                    None => ", and holds none yet".to_string(),
                }
            ),
            match near {
                Some(near) => format!("Use `jadpo calls show {near}`"),
                None => "Run `jadpo calls` to see their ids".to_string(),
            },
        ));
    };
    let outcome = if entry.ended == "answered" {
        Outcome::Success
    } else {
        Outcome::Failure
    };
    let ended = match (entry.ended.as_str(), &entry.code) {
        ("failed", Some(code)) => format!("failed with `{code}`"),
        ("outcome_unknown", _) => "may have acted: its outcome is unknown".to_string(),
        ("in_flight", _) => "was still out when its run ended".to_string(),
        (other, _) => other.replace('_', " "),
    };
    let mut rows = vec![
        ("call".to_string(), entry.id.clone()),
        ("source".to_string(), entry.source.clone()),
    ];
    if let Some(request) = &entry.request_id {
        rows.push(("request".to_string(), request.clone()));
    }
    rows.push(("at".to_string(), entry.at.clone()));
    if let Some(ms) = entry.ms {
        rows.push(("time".to_string(), format!("{ms} ms")));
    }
    let mut screen = Screen::new().with(Block::Status {
        outcome,
        text: format!("`{}` {ended}", entry.name()),
        rows,
    });
    let mut sent = vec![format!("{} {}", entry.method, entry.url)];
    if let Some(body) = &entry.body {
        sent.extend(pretty(body));
    }
    screen.push(Block::Section {
        heading: "sent".to_string(),
        lines: sent,
    });
    if let Some(answer) = &entry.answer {
        screen.push(Block::Section {
            heading: "answer".to_string(),
            lines: pretty(answer),
        });
    }
    Reply::screen(screen, entry_json(entry))
}

/// JSON text laid out a member a line, or as it is when it isn't JSON.
fn pretty(text: &str) -> Vec<String> {
    match serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|json| serde_json::to_string_pretty(&json).ok())
    {
        Some(laid_out) => laid_out.lines().map(str::to_string).collect(),
        None => text.lines().map(str::to_string).collect(),
    }
}

// --- script -------------------------------------------------------------------

fn script(root: &Path, operation: &str, answer: &str) -> Reply {
    let loaded = jadpo_db::load(root);
    let prepared = match jadpo_interp::Prepared::load(&loaded.db, loaded.project) {
        Ok(prepared) => prepared,
        Err(_) => {
            return Reply::failed(
                Diagnostic::new(
                    &rule::CLI_CALLS_ARGUMENTS,
                    "The project has errors, so its operations can't be read",
                )
                .reason("`jadpo calls script` checks the operation and its answer against the app's packages, which needs the project to check cleanly.")
                .next("Run `jadpo check`, fix what it reports, then script the answer again"),
            );
        }
    };
    // A prompt asked through a package filling `ai` (chunk 5.5v3b).
    if let Some(prompt) = operation.strip_prefix("ai.") {
        return script_prompt(root, &prepared, prompt, answer);
    }
    let plan = prepared.plan().providers();
    let known: Vec<String> = plan
        .operations
        .iter()
        .filter_map(|op| {
            let connection = plan.connections.get(op.connection as usize)?;
            Some(format!("{}.{}", connection.name, op.name))
        })
        .collect();
    let Some(op) = plan.operations.iter().find(|op| {
        plan.connections
            .get(op.connection as usize)
            .is_some_and(|connection| format!("{}.{}", connection.name, op.name) == operation)
    }) else {
        let near = jadpo_diag::nearest(operation, known.iter().map(String::as_str));
        return Reply::failed(arguments(
            format!("`{operation}` isn't one of the app's provider operations"),
            format!(
                "An answer is scripted for an operation as the code calls it, `<connection>.<resource>.<operation>`: {}.",
                if known.is_empty() {
                    "the app calls none".to_string()
                } else {
                    crate::run::either(&crate::run::quoted(known.iter()))
                }
            ),
            match near {
                Some(near) => format!("Use `jadpo calls script {near} {answer}`"),
                None => "Name one of the operations listed".to_string(),
            },
        ));
    };
    let failures = prepared.plan().types().failures();
    let codes: Vec<String> = op
        .failures
        .iter()
        .filter_map(|key| failures.iter().find(|plan| plan.key == *key))
        .map(|plan| plan.code.clone())
        .collect();
    if !codes.iter().any(|code| code == answer) && FakeCondition::named(answer).is_none() {
        let mut allowed = codes.clone();
        allowed.extend(
            FakeCondition::ALL
                .iter()
                .map(|condition| condition.word().to_string()),
        );
        let near = jadpo_diag::nearest(answer, allowed.iter().map(String::as_str));
        return Reply::failed(arguments(
            format!("`{answer}` isn't an answer `{operation}` gives"),
            format!(
                "A scripted answer is one of `{operation}`'s failures by its code, or a condition: {}.",
                crate::run::either(&crate::run::quoted(allowed.iter()))
            ),
            match near {
                Some(near) => format!("Use `jadpo calls script {operation} {near}`"),
                None => "Script one of the answers listed".to_string(),
            },
        ));
    }
    scripted(root, operation, operation, answer)
}

/// Scripts `answer`, a condition of `ai.generate`'s, for the next call of
/// prompt `prompt` through the package's connection that fills `ai` (area
/// 21; chunk 5.5v3b), as a test's `fakes: { ai.<prompt>: }` names it.
fn script_prompt(
    root: &Path,
    prepared: &jadpo_interp::Prepared,
    prompt: &str,
    answer: &str,
) -> Reply {
    let written = format!("ai.{prompt}");
    let Some(ai) = prepared.plan().ai() else {
        return Reply::failed(arguments(
            format!("`{written}` names a prompt, and the app asks no model"),
            "A prompt's next answer is scripted for the model `ai:` names in `app.jadpo`, and the app names none.".to_string(),
            "Script one of the app's provider operations, `<connection>.<resource>.<operation>`".to_string(),
        ));
    };
    if !matches!(ai.driver, jadpo_std::ai::AiDriver::Package(_)) {
        return Reply::failed(arguments(
            format!(
                "`{written}` is asked of a model on this machine, which `jadpo dev` doesn't fake"
            ),
            format!(
                "`ai:` names `{}`, a server that speaks OpenAI's API through `Driver.openai_compatible`; `jadpo dev` sends it every call, so there is nothing to script. A package's connection that fills `ai` is faked, and its prompts take a scripted answer.",
                ai.connection
            ),
            "Have the model answer as you need it, or name a package's connection in `ai:`"
                .to_string(),
        ));
    }
    let known: Vec<String> = prepared
        .plan()
        .prompts()
        .entries
        .iter()
        .map(|entry| format!("ai.{}", entry.key))
        .collect();
    if !known.contains(&written) {
        let near = jadpo_diag::nearest(&written, known.iter().map(String::as_str));
        return Reply::failed(arguments(
            format!("`{written}` isn't one of the app's prompts"),
            format!(
                "A prompt's next answer is scripted by its name, as a test's `fakes:` writes it: {}.",
                if known.is_empty() {
                    "the app has none".to_string()
                } else {
                    crate::run::either(&crate::run::quoted(known.iter()))
                }
            ),
            match near {
                Some(near) => format!("Use `jadpo calls script {near} {answer}`"),
                None => "Name one of the prompts listed".to_string(),
            },
        ));
    }
    let conditions: Vec<&str> = jadpo_std::ai::CONDITIONS
        .iter()
        .map(|condition| condition.word())
        .collect();
    if !conditions.contains(&answer) {
        let near = jadpo_diag::nearest(answer, conditions.iter().copied());
        return Reply::failed(arguments(
            format!("`{answer}` isn't an answer `{written}` gives"),
            format!(
                "A prompt's scripted answer is one of `ai.generate`'s conditions: {}. Anything else is a generated value of the call's type.",
                crate::run::either(&crate::run::quoted(conditions.iter()))
            ),
            match near {
                Some(near) => format!("Use `jadpo calls script {written} {near}`"),
                None => "Script one of the answers listed".to_string(),
            },
        ));
    }
    scripted(
        root,
        &written,
        &format!("{}.{prompt}", ai.connection),
        answer,
    )
}

/// Keeps `answer` for the next call named `call`, which the user wrote as
/// `written`, and says so.
fn scripted(root: &Path, written: &str, call: &str, answer: &str) -> Reply {
    let operation = written;
    if let Err(error) = dev_calls::script(root, call, answer) {
        return Reply::failed(
            Diagnostic::new(
                &rule::CLI_CALLS_ARGUMENTS,
                "The answer couldn't be kept for `jadpo dev`",
            )
            .reason(format!("Writing it under `build/dev/` failed: {error}."))
            .next("Check that the project's `build/` folder can be written, then run it again"),
        );
    }
    let waiting = dev_calls::scripted(root)
        .iter()
        .filter(|(scripted, _)| scripted == call)
        .count();
    let text = if waiting > 1 {
        format!(
            "Scripted `{answer}` for `{operation}`, after {} answers scripted before it",
            waiting - 1
        )
    } else {
        format!("Scripted `{answer}` for the next call to `{operation}`")
    };
    let screen = Screen::new()
        .with(Block::Status {
            outcome: Outcome::Success,
            text,
            rows: Vec::new(),
        })
        .with(Block::hint(
            "jadpo calls",
            "shows the call once the app makes it",
        ));
    Reply::screen(
        screen,
        Json::object([
            ("operation", Json::from(operation)),
            ("answer", Json::from(answer)),
            (
                "waiting",
                Json::from(i64::try_from(waiting).unwrap_or(i64::MAX)),
            ),
        ]),
    )
}
