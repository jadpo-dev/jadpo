//! `jadpo package record <connection> <operation>` (area 11, "Provenance
//! and confirmation"; area 14; chunk 7.2d2): calls a provider's sandbox
//! once through the connection, as the app's code would, checks the
//! operation's output type against the answer, and keeps the answer as
//! the operation's recording, `recordings/<resource>.<operation>.json` in
//! its package, its plain values replaced by placeholders
//! ([`jadpo_runtime::recorded`]), so the operation is confirmed and every
//! decoder reads it from then on.
//!
//! It records into the app's own integration, or a linked package's
//! checkout, never an installed package, whose files its lock's digest
//! holds; it refuses a connection whose environment is live, or whose
//! credential nothing sets. The operation's input is given as JSON, as a
//! route's body is written, with `--input`.
//!
//! ```text
//! ✓ Recorded `couriers.pickups.create` from the sandbox
//!     sent      POST https://sandbox.swift-couriers.example/v1/pickups
//!     answer    decodes as SwiftCouriers.Pickup
//!     values    2 placeholders, 1 kept as sent
//!     file      shared/providers/swift_couriers/v1/recordings/pickups.create.json
//!
//! ✓ operation recorded  SwiftCouriers.v1 pickups.create · confirmed
//!
//! → jadpo test  decodes the recording with every decoder
//! ```

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use jadpo_diag::{Decider, Diagnostic, nearest, rule};
use jadpo_interp::Program;
use jadpo_runtime::call_log::{CallLog, Seen};
use jadpo_runtime::provider::Done;
use jadpo_runtime::recorded::{self, Unread};
use jadpo_runtime::{Ctx, Env};
use jadpo_std::provider::Address;
use jadpo_term::{Block, Fact, Json, Outcome, Screen};

use crate::fmt::project_needed;
use crate::package::arguments;
use crate::run::{Call, Reply, either, quoted};

pub fn record(call: &Call) -> Reply {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return project_needed("package record");
    };
    let (Some(connection), Some(operation)) = (call.arguments.get(1), call.arguments.get(2)) else {
        return Reply::failed(arguments(
            "`jadpo package record` needs a connection and an operation".to_string(),
            "It calls one operation of a provider's sandbox through one of the app's connections, and keeps the answer as the operation's recording.".to_string(),
            "Run `jadpo package record <connection> <resource>.<operation>`".to_string(),
        ));
    };
    if let Some(extra) = call.arguments.get(3) {
        return Reply::failed(arguments(
            format!("`jadpo package record` takes one operation, not `{extra}`"),
            "It records one operation of one connection at a time.".to_string(),
            "Run `jadpo package record <connection> <resource>.<operation>`".to_string(),
        ));
    }
    let ready = match crate::dev::prepare(&root, &call.environment, true) {
        Ok(ready) => ready,
        Err((diagnostics, sources)) => return refused_by_errors(diagnostics, sources),
    };
    let Some(chosen) = ready.prepared.plan().package_connection(connection) else {
        let names: Vec<&str> = ready
            .prepared
            .plan()
            .package_connections()
            .map(|(_, name, _)| name)
            .collect();
        let reason = if names.is_empty() {
            "The app has no connection to a provider package.".to_string()
        } else {
            format!(
                "The app's connections to provider packages are {}.",
                either(&quoted(&names))
            )
        };
        return Reply::failed(match nearest(connection, names.iter().copied()) {
            Some(near) => arguments(
                format!("`{connection}` isn't a connection to a provider"),
                format!("{reason} Did you mean `{near}`?"),
                format!("Use `jadpo package record {near} {operation}`"),
            ),
            None => arguments(
                format!("`{connection}` isn't a connection to a provider"),
                reason,
                "Name one of them".to_string(),
            ),
        });
    };
    let plan = ready.prepared.plan().providers();
    let table = ready.prepared.plan().connections();
    let Some((_, op)) = plan.operation(table, connection, operation) else {
        let names: Vec<&str> = plan
            .operations
            .iter()
            .filter(|op| {
                ready.prepared.plan().connection_name(op.connection) == Some(connection.as_str())
            })
            .map(|op| op.name.as_str())
            .collect();
        let reason = format!(
            "`{connection}` calls `{}`, whose public operations are {}.",
            chosen.package,
            either(&quoted(&names))
        );
        return Reply::failed(match nearest(operation, names.iter().copied()) {
            Some(near) => arguments(
                format!("`{operation}` isn't an operation of `{connection}`"),
                format!("{reason} Did you mean `{near}`?"),
                format!("Use `jadpo package record {connection} {near}`"),
            ),
            None => arguments(
                format!("`{operation}` isn't an operation of `{connection}`"),
                reason,
                "Name one of them as `<resource>.<operation>`".to_string(),
            ),
        });
    };
    let name = format!("{connection}.{operation}");
    // Where the recording goes: the app's own integration, or a linked
    // package's checkout; never an installed package.
    let folder = match package_folder(&root, &chosen.package) {
        Ok(folder) => folder,
        Err(diagnostic) => return Reply::failed(*diagnostic),
    };
    if op.output.is_none() {
        return Reply::failed(
            Diagnostic::new(
                &rule::TOOLCHAIN_NOT_BUILT_YET,
                format!("`{name}` has no `output:` to check its answer against"),
            )
            .reason("Area 11 decides that `jadpo package record` writes an operation's output type from the answer when it has none, and that comes with mapping imported types to standard types (chunk 7.2e).")
            .next_marked(format!(
                "Give `{operation}` an `output:` from the provider's documentation, then record it (agent)"
            )),
        );
    }
    let program = Program::new(std::sync::Arc::clone(&ready.prepared)).with_config(&ready.config);
    let json = program.stored_json();
    // Its input, as a route's body is written.
    let input = match (op.input, call.value("--input")) {
        (None, None) => None,
        (None, Some(_)) => {
            return Reply::failed(arguments(
                format!("`{name}` takes no input"),
                "`--input` gives an operation's input, and this one has none.".to_string(),
                format!("Run `jadpo package record {connection} {operation}`"),
            ));
        }
        (Some(_), None) => {
            return Reply::failed(arguments(
                format!("`{name}` needs its input"),
                format!(
                    "`{operation}` takes an input, which `--input` gives as JSON, written as a route's body is."
                ),
                format!("Run `jadpo package record {connection} {operation} --input '{{...}}'`"),
            ));
        }
        (Some(number), Some(text)) => {
            let read = serde_json::from_str::<serde_json::Value>(text)
                .ok()
                .and_then(|given| {
                    let codec = json.codec_of(number)?;
                    let mut problems = Vec::new();
                    let value = json.codecs.read(
                        codec,
                        &given,
                        &jadpo_value::json::Path::default(),
                        &mut problems,
                    );
                    Some((value, problems))
                });
            match read {
                Some((Some(value), problems)) if problems.is_empty() => Some(value),
                Some((_, problems)) if !problems.is_empty() => {
                    let problem = &problems[0];
                    let at = if problem.field.is_empty() {
                        String::new()
                    } else {
                        format!(" on `{}`", problem.field)
                    };
                    return Reply::failed(arguments(
                        format!("`--input` isn't `{name}`'s input: `{}`{at}", problem.rule),
                        "`--input` is the operation's input as JSON, written as a route's body is, and it has to keep its type's rules.".to_string(),
                        "Correct the JSON, then run it again".to_string(),
                    ));
                }
                _ => {
                    return Reply::failed(arguments(
                        "`--input` isn't JSON".to_string(),
                        "`--input` is the operation's input as one JSON document, written as a route's body is.".to_string(),
                        "Quote the JSON in single quotes so the shell passes it whole: `--input '{...}'`".to_string(),
                    ));
                }
            }
        }
    };

    // Never live, and never with a stand-in for its credential.
    if let Some(refused) = refusal(&ready, connection, chosen, "jadpo package record") {
        return Reply::failed(*refused);
    }
    // The call, through the package driver, its answer as sent kept.
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
            return Reply::failed(
                Diagnostic::new(
                    &rule::RECORDING_FAILED,
                    format!("`{name}` couldn't be called"),
                )
                .reason(format!(
                    "The runtime that sends the call couldn't start: {error}."
                ))
                .next_marked("Free some memory, then run it again (human)"),
            );
        }
    };
    let mut ctx = Ctx::of(&program, &host, Env::default());
    // A keyed operation's key is derived as any call's, from this run's
    // randomness, so each recording is a request of its own (area 12,
    // "Keys").
    ctx.allow_standalone_calls();
    let done = runtime.block_on(program.call_provider(&mut ctx, connection, operation, input));
    let kept = seen.0.borrow().clone();
    let failed = |headline: String, reason: String, next: String| {
        Reply::failed(
            Diagnostic::new(&rule::RECORDING_FAILED, headline)
                .reason(reason)
                .next_marked(next),
        )
    };
    let output_name = output_type_name(&root, &chosen.package, operation);
    let answer = match (done, kept.answer) {
        (Done::Value(_), Some(answer)) => answer,
        (Done::Fault(_), Some(answer)) => {
            // Why the operation's types refuse the answer, field by field.
            let why = match recorded::check(&mut ctx, &json, op, &answer, &program) {
                Err(Unread::Undecoded { field, rule }) if field.is_empty() => {
                    format!("`{rule}`")
                }
                Err(Unread::Undecoded { field, rule }) => format!("`{rule}` on `{field}`"),
                Err(Unread::NothingAt(path)) => format!("nothing at `{path}`"),
                Err(Unread::DecoderFailed(how)) => format!("its `decode:` ending with {how}"),
                Err(Unread::NotJson) => "it isn't JSON".to_string(),
                _ => "its decoding".to_string(),
            };
            return failed(
                format!("The sandbox's answer to `{name}` doesn't decode as `{output_name}`"),
                format!(
                    "Reading it found {why}: the operation's types and the provider disagree, so a real call would fault. Nothing was recorded."
                ),
                format!(
                    "Correct `{output_name}` against the answer and the provider's documentation, then run it again (agent)"
                ),
            );
        }
        (Done::Failed(failure), _) => {
            let (headline, reason, next) = match failure.code.as_str() {
                "unavailable" => (
                    format!("The sandbox didn't answer `{name}`"),
                    "It couldn't be reached, or it said it did nothing and asked to be called later.".to_string(),
                    "Check the sandbox is up, then run it again (human)".to_string(),
                ),
                "outcome_unknown" => (
                    format!("The sandbox gave no answer to `{name}`"),
                    "The call went out and no answer came back, so it may have acted.".to_string(),
                    "Check the sandbox for what it did, then run it again (human)".to_string(),
                ),
                "misconfigured" => (
                    format!("The sandbox refused `{connection}`'s credential"),
                    "It answered as a provider does a credential it doesn't take, or the connection's address isn't one Jadpo sends to.".to_string(),
                    "Check the sandbox's key in `.env.local`, then run it again (human)".to_string(),
                ),
                code => (
                    format!("The sandbox refused `{name}` with `{code}`"),
                    "A recording keeps an answer the operation gives; a refusal is one of its failures, which a test scripts by its code instead.".to_string(),
                    "Change `--input` so the sandbox accepts it, then run it again (agent)".to_string(),
                ),
            };
            return failed(headline, reason, next);
        }
        (_, None) => {
            return failed(
                format!("`{name}` gave no answer to record"),
                "The call ended without an answer from the sandbox.".to_string(),
                "Run it again; if it goes on, report it to Jadpo's maintainers with this report (human)".to_string(),
            );
        }
    };
    let Ok(text) = recorded::keep(&mut ctx, &json, op, &answer) else {
        return failed(
            format!("The sandbox's answer to `{name}` couldn't be kept"),
            "It decoded as the operation's output, but couldn't be written again as its recording."
                .to_string(),
            "Report it to Jadpo's maintainers with this report (human)".to_string(),
        );
    };
    let file = folder.join(recorded::file_name(operation));
    let before = std::fs::read_to_string(&file).ok();
    let written = before.as_deref() != Some(text.as_str());
    if written {
        let saved = file
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&file, &text));
        if let Err(error) = saved {
            return crate::package::write_failed(
                "package record",
                &recorded::file_name(operation),
                &error.to_string(),
            );
        }
    }
    let shown = shown_path(&root, &file);
    let (placeholders, as_sent) = counts(&text);
    let mut values = crate::package::plural(placeholders, "placeholder", "placeholders");
    if as_sent > 0 {
        values.push_str(&format!(", {as_sent} kept as sent"));
    }
    let request = format!("{} {}", kept.method, kept.url);
    let mut screen = Screen::new().with(Block::Status {
        outcome: Outcome::Success,
        text: match (written, before.is_some()) {
            (true, false) => format!("Recorded `{name}` from the sandbox"),
            (true, true) => format!("Recorded `{name}` again from the sandbox"),
            (false, _) => format!("Recorded `{name}` from the sandbox, as before"),
        },
        rows: vec![
            ("sent".to_string(), request.clone()),
            ("answer".to_string(), format!("decodes as {output_name}")),
            ("values".to_string(), values),
            ("file".to_string(), shown.clone()),
        ],
    });
    screen.push(Block::summary(
        Outcome::Success,
        "operation recorded",
        [
            Fact::plain(format!("{} {operation}", chosen.package)),
            Fact::plain("confirmed"),
        ],
    ));
    screen.push(Block::hint(
        "jadpo test",
        "decodes the recording with every decoder",
    ));
    Reply::screen(
        screen,
        Json::object([
            ("connection", Json::from(connection.as_str())),
            ("operation", Json::from(operation.as_str())),
            ("package", Json::from(chosen.package.as_str())),
            ("file", Json::from(shown.as_str())),
            ("written", written.into()),
            ("placeholders", Json::from(placeholders as i64)),
            ("keptAsSent", Json::from(as_sent as i64)),
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

/// Why `command` won't call through provider connection `chosen` (area
/// 11; chunks 7.2d2 and 5.5v4b): nothing set for its credential,
/// environment or address, so `jadpo dev`'s stand-in would reach nobody;
/// an environment that chooses `live`, since a recording is made against a
/// sandbox, never the live API; or no address at all.
pub(crate) fn refusal(
    ready: &crate::dev::Ready,
    connection: &str,
    chosen: &jadpo_std::provider::ConnectionPlan,
    command: &str,
) -> Option<Box<Diagnostic>> {
    let config = ready.prepared.plan().config();
    let variable = |at: u32| {
        config
            .fields
            .get(at as usize)
            .map_or_else(String::new, |field| field.variable.clone())
    };
    if let Some(unset) = ready.unset.get(connection)
        && !unset.is_empty()
    {
        let variables: Vec<String> = unset.iter().map(|name| format!("`{name}`")).collect();
        return Some(Box::new(
            Diagnostic::new(
                &rule::RECORDING_REFUSED,
                format!("`{connection}` has nothing set for its sandbox"),
            )
            .reason(format!(
                "`{command}` calls the provider's sandbox through `{connection}`, which reads {}, and nothing sets {}.",
                crate::package::and_list(&variables),
                if variables.len() == 1 { "it" } else { "them" }
            ))
            .next_marked(format!(
                "Set {} in `.env.local` to the sandbox's, then run it again (human)",
                crate::package::and_list(&variables)
            )),
        ));
    }
    match &chosen.address {
        Address::Hosts { environment, .. } => {
            let chosen_now = ready.config.text_at(*environment);
            if chosen_now.is_some_and(|chosen| chosen.eq_ignore_ascii_case("live")) {
                let variable = variable(*environment);
                return Some(Box::new(
                    Diagnostic::new(
                        &rule::RECORDING_REFUSED,
                        format!("`{connection}` calls its provider's live API"),
                    )
                    .reason(format!(
                        "`{command}` calls a provider's sandbox, never its live API, and `{variable}` chooses `live`."
                    ))
                    .next_marked(format!(
                        "Set `{variable}` to `sandbox` in `.env.local`, then run it again (human)"
                    )),
                ));
            }
            None
        }
        Address::Setting(_) => None,
        Address::Unknown => Some(Box::new(
            Diagnostic::new(
                &rule::RECORDING_REFUSED,
                format!("`{connection}` has no address to call"),
            )
            .reason(format!(
                "`{}` gives no `hosts:` its connection's environment chooses between, and the connection gives no `host:`.",
                chosen.package
            ))
            .next_marked(format!(
                "Give `{connection}` an `environment:` or a `host:` in `app.jadpo` (agent)"
            )),
        )),
    }
}

/// The answer the call's site handed the log, with the request it went
/// out as.
#[derive(Clone, Debug, Default)]
pub(crate) struct Exchange {
    pub(crate) method: String,
    pub(crate) url: String,
    pub(crate) answer: Option<String>,
}

/// Keeps the one call's request and answer as sent.
#[derive(Default)]
pub(crate) struct Kept(pub(crate) RefCell<Exchange>);

impl CallLog for Kept {
    fn seen(&self, seen: &Seen<'_>) {
        *self.0.borrow_mut() = Exchange {
            method: seen.method.to_string(),
            url: seen.url.to_string(),
            answer: seen.answer.map(str::to_string),
        };
    }
}

/// Where package `package` (`AcmeCouriers.v1`) keeps its recordings: the
/// app's own integration, or a linked package's checkout; refused for an
/// installed package, whose files its lock holds to its digest.
fn package_folder(root: &Path, package: &str) -> Result<PathBuf, Box<Diagnostic>> {
    let (code, version) = package.rsplit_once('.').unwrap_or((package, ""));
    let name = jadpo_db::to_snake(code);
    let own = root
        .join("shared")
        .join(jadpo_db::PROVIDERS)
        .join(&name)
        .join(version);
    if own.join(jadpo_db::PACKAGE_INDEX).is_file() {
        return Ok(own);
    }
    let linked = std::fs::read_to_string(root.join(jadpo_db::LOCK))
        .ok()
        .and_then(|text| jadpo_db::parse_lock(&text).ok())
        .and_then(|lines| {
            lines.into_iter().find_map(|line| match line.source {
                jadpo_db::Source::Link { path } if line.package == package => Some(path),
                _ => None,
            })
        });
    if let Some(path) = linked {
        return Ok(root.join(path));
    }
    Err(Box::new(
        Diagnostic::new(
            &rule::RECORDING_REFUSED,
            format!("`{package}` is installed, so its recordings don't change here"),
        )
        .reason(format!(
            "An installed package's files are held to its digest in `{}`; its recordings are made where it is developed.",
            jadpo_db::LOCK
        ))
        .next_marked("Link its checkout with `jadpo package link <folder>`, then record it again (human)"),
    ))
}

/// How many placeholders a recording holds, and how many texts and
/// numbers it keeps as sent.
pub(crate) fn counts(text: &str) -> (usize, usize) {
    fn walk(json: &serde_json::Value, out: &mut (usize, usize)) {
        match json {
            serde_json::Value::String(text) if recorded::is_placeholder(text) => out.0 += 1,
            serde_json::Value::String(_) | serde_json::Value::Number(_) => out.1 += 1,
            serde_json::Value::Array(items) => items.iter().for_each(|item| walk(item, out)),
            serde_json::Value::Object(members) => {
                members.values().for_each(|value| walk(value, out));
            }
            _ => {}
        }
    }
    let mut out = (0, 0);
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(text) {
        walk(&json, &mut out);
    }
    out
}

/// `path` from `root`, with `/`.
pub(crate) fn shown_path(root: &Path, path: &Path) -> String {
    let real = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let (root, path) = (real(root), real(path));
    match path.strip_prefix(&root) {
        Ok(inside) => inside
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/"),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}

/// The name of `package`'s operation `operation`'s output type, as the
/// package writes it: `Pickup`.
fn output_type_name(root: &Path, package: &str, operation: &str) -> String {
    let loaded = jadpo_db::load(root);
    let (db, project) = (&loaded.db, loaded.project);
    let (code, version) = package.rsplit_once('.').unwrap_or((package, ""));
    let name = jadpo_db::to_snake(code);
    [
        format!("shared/{}/{name}/{version}", jadpo_db::PROVIDERS),
        format!("{}/{name}/{version}", jadpo_db::PACKAGES),
    ]
    .iter()
    .filter_map(|folder| jadpo_check::surface::surface(db, project, folder))
    .flat_map(|surface| surface.declarations)
    .find_map(|declaration| match declaration.what {
        jadpo_check::surface::What::Operation { output, .. } if declaration.name == operation => {
            output
        }
        _ => None,
    })
    .unwrap_or_else(|| "its output".to_string())
}

/// The project's errors, which stop it calling anything.
pub(crate) fn refused_by_errors(
    diagnostics: Vec<Diagnostic>,
    sources: std::collections::BTreeMap<String, String>,
) -> Reply {
    let mut screen = Screen::new();
    for diagnostic in &diagnostics {
        screen.push(Block::diagnostic_from(diagnostic, &sources, None));
    }
    screen.push(Block::summary(
        Outcome::Failure,
        "nothing recorded",
        [Fact::problem(crate::package::plural(
            diagnostics.len(),
            "error",
            "errors",
        ))],
    ));
    Reply::Report {
        screen,
        data: Json::object([("recorded", false.into())]),
        diagnostics,
        sources,
    }
}

/// A diagnostic's `next`, its decider from its closing `(agent)` or
/// `(human)`.
pub(crate) trait NextMarked {
    fn next_marked(self, text: impl Into<String>) -> Self;
}

impl NextMarked for Diagnostic {
    fn next_marked(self, text: impl Into<String>) -> Diagnostic {
        let text = text.into();
        if let Some(text) = text.strip_suffix(" (human)") {
            self.next_for(text, Decider::Human)
        } else if let Some(text) = text.strip_suffix(" (agent)") {
            self.next_for(text, Decider::Agent)
        } else {
            self.next(text)
        }
    }
}
