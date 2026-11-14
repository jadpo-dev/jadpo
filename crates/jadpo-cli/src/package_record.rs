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
//! route's body is written, with `--input`. An operation with no
//! `output:` has its output type written from the answer first (chunk
//! K5, [`crate::package_import::answer`]), and every file written is put
//! back when the answer isn't recorded.
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
    // An operation with no `output:` has its type written from the
    // sandbox's answer (area 11, "Provenance and confirmation"; chunk K5).
    let writing = op.output.is_none();
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
    let output_name = output_type_name(&root, &chosen.package, operation);
    let answer = match (done, kept.answer.clone()) {
        (Done::Value(_), Some(answer)) => answer,
        (Done::Fault(_), Some(answer)) => {
            // Why the operation's types refuse the answer, field by field.
            let why = match recorded::check(&mut ctx, &json, op, &answer, &program) {
                Err(unread) => unread_text(&unread),
                Ok(_) => "its decoding".to_string(),
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
    let found = Found {
        root: &root,
        connection,
        operation,
        package: &chosen.package,
        folder: &folder,
        exchange: &kept,
        answer: &answer,
    };
    if !writing {
        return found.kept(&ready, None).unwrap_or_else(|reply| reply);
    }
    let output_at = op.output_at.clone();
    drop(ctx);
    // Its output type, written from the answer, then the answer kept as
    // any answer is; nothing written stays when it can't be.
    let wrote = match write_output(&found, output_at.as_deref()) {
        Ok(wrote) => wrote,
        Err(reply) => return reply,
    };
    let written: Vec<String> = wrote.saved.iter().map(|(path, _)| path.clone()).collect();
    let again = crate::dev::prepare_allowing(&root, &call.environment, true, &|diagnostic| {
        // Its judgement calls, which `jadpo check` shows, don't stop it.
        diagnostic.rule.declinable
            && diagnostic
                .primary
                .as_ref()
                .is_some_and(|label| written.iter().any(|path| label.span.path.ends_with(path)))
    });
    let again = match again {
        Ok(again) => again,
        Err((diagnostics, sources)) => {
            // Shown as the files were written, then put back.
            let reply = refused_by_errors(diagnostics, sources);
            wrote.undo();
            return reply;
        }
    };
    match found.kept(&again, Some(&wrote)) {
        Ok(reply) => reply,
        Err(reply) => {
            wrote.undo();
            reply
        }
    }
}

/// Why an answer isn't one of its operation's, as a reason says it.
fn unread_text(unread: &Unread) -> String {
    match unread {
        Unread::Undecoded { field, rule } if field.is_empty() => format!("`{rule}`"),
        Unread::Undecoded { field, rule } => format!("`{rule}` on `{field}`"),
        Unread::NothingAt(path) => format!("nothing at `{path}`"),
        Unread::DecoderFailed(how) => format!("its `decode:` ending with {how}"),
        Unread::NotJson => "it isn't JSON".to_string(),
        _ => "its decoding".to_string(),
    }
}

/// A `recording.failed` refusal.
fn failed(headline: String, reason: String, next: String) -> Reply {
    Reply::failed(
        Diagnostic::new(&rule::RECORDING_FAILED, headline)
            .reason(reason)
            .next_marked(next),
    )
}

/// The sandbox's answer to one operation, and where it is recorded.
struct Found<'a> {
    root: &'a Path,
    connection: &'a str,
    /// `bookings.create`.
    operation: &'a str,
    /// `AcmeCouriers.v1`.
    package: &'a str,
    folder: &'a Path,
    exchange: &'a Exchange,
    answer: &'a str,
}

/// An output type written from an answer: what was written, and each
/// file's text before, to put back when the answer isn't kept.
struct Wrote {
    written: crate::package_import::answer::Written,
    /// Each file saved, its path from the project's root, and its text
    /// before (`None` for a new file).
    saved: Vec<(String, Option<String>)>,
    root: PathBuf,
}

impl Wrote {
    /// Puts every file back as it was.
    fn undo(&self) {
        for (path, before) in &self.saved {
            let full = self.root.join(path);
            let _ = match before {
                Some(text) => std::fs::write(&full, text),
                None => std::fs::remove_file(&full),
            };
        }
    }
}

/// Writes operation `found.operation`'s output type from its answer, at
/// `output_at` in it: the types in the package's `types/` and `output:`
/// on the operation.
fn write_output(found: &Found<'_>, output_at: Option<&str>) -> Result<Wrote, Reply> {
    let name = format!("{}.{}", found.connection, found.operation);
    let unwritten = |why: String| {
        failed(
            format!("No output type could be written from the sandbox's answer to `{name}`"),
            format!("{why} Nothing was written or recorded."),
            format!(
                "Give `{}` an `output:` from the provider's documentation, then run it again (agent)",
                found.operation
            ),
        )
    };
    let Ok(document) = serde_json::from_str::<serde_json::Value>(found.answer) else {
        return Err(unwritten("The answer isn't JSON.".to_string()));
    };
    let value = match output_at {
        Some(path) => match jadpo_runtime::provider::at_path(&document, path) {
            Some(value) => value,
            None => {
                return Err(unwritten(format!(
                    "The answer has nothing at `{path}`, where `output_at:` says the output is."
                )));
            }
        },
        None => &document,
    };
    let (resource, operation) = found
        .operation
        .split_once('.')
        .unwrap_or((found.operation, ""));
    let written =
        match crate::package_import::answer::written(found.folder, resource, operation, value) {
            Ok(written) => written,
            Err(crate::package_import::answer::Unwritten(why)) => return Err(unwritten(why)),
        };
    let shown_folder = shown_path(found.root, found.folder);
    let mut wrote = Wrote {
        written,
        saved: Vec::new(),
        root: found.root.to_path_buf(),
    };
    for (path, text) in wrote.written.files.clone() {
        let full = found.folder.join(&path);
        let before = std::fs::read_to_string(&full).ok();
        let saved = full
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&full, &text));
        wrote.saved.push((format!("{shown_folder}/{path}"), before));
        if let Err(error) = saved {
            wrote.undo();
            return Err(crate::package::write_failed(
                "package record",
                &path,
                &error.to_string(),
            ));
        }
    }
    Ok(wrote)
}

impl Found<'_> {
    /// Keeps the answer as the operation's recording, through `ready`'s
    /// program, once its output type reads it, and says so; `Err` when
    /// nothing was recorded.
    fn kept(&self, ready: &crate::dev::Ready, wrote: Option<&Wrote>) -> Result<Reply, Reply> {
        let (connection, operation) = (self.connection, self.operation);
        let name = format!("{connection}.{operation}");
        let program =
            Program::new(std::sync::Arc::clone(&ready.prepared)).with_config(&ready.config);
        let json = program.stored_json();
        let plan = ready.prepared.plan().providers();
        let table = ready.prepared.plan().connections();
        let Some((_, op)) = plan.operation(table, connection, operation) else {
            return Err(failed(
                format!("`{name}` is gone from the package"),
                "The operation the answer came from isn't in the package as it now reads."
                    .to_string(),
                "Report it to Jadpo's maintainers with this report (human)".to_string(),
            ));
        };
        let host = jadpo_host_native::NativeHost::new();
        let mut ctx = Ctx::of(&program, &host, Env::default());
        let output_name = match wrote {
            Some(wrote) => wrote.written.output.clone(),
            None => output_type_name(self.root, self.package, operation),
        };
        // The type written from the answer reads it, as any answer is
        // read, before anything is kept.
        if wrote.is_some()
            && let Err(unread) = recorded::check(&mut ctx, &json, op, self.answer, &program)
        {
            return Err(failed(
                format!(
                    "The sandbox's answer to `{name}` doesn't decode as `{output_name}`, written from it"
                ),
                format!(
                    "Reading it found {}. Nothing was written or recorded.",
                    unread_text(&unread)
                ),
                format!(
                    "Give `{operation}` an `output:` from the provider's documentation, then run it again (agent)"
                ),
            ));
        }
        let Ok(text) = recorded::keep(&mut ctx, &json, op, self.answer) else {
            return Err(failed(
                format!("The sandbox's answer to `{name}` couldn't be kept"),
                "It decoded as the operation's output, but couldn't be written again as its recording."
                    .to_string(),
                "Report it to Jadpo's maintainers with this report (human)".to_string(),
            ));
        };
        let file = self.folder.join(recorded::file_name(operation));
        let before = std::fs::read_to_string(&file).ok();
        let written = before.as_deref() != Some(text.as_str());
        if written {
            let saved = file
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(&file, &text));
            if let Err(error) = saved {
                return Err(crate::package::write_failed(
                    "package record",
                    &recorded::file_name(operation),
                    &error.to_string(),
                ));
            }
        }
        // When it was recorded, which the package's facts give as its last
        // contract run (area 11, "What a release shows"; Richard, 2026-11-07).
        let when = self.folder.join(jadpo_check::release::RECORDED);
        let kept_when = recorded_at(
            std::fs::read_to_string(&when).ok().as_deref(),
            operation,
            &instant_text(crate::registry::now_seconds()),
        );
        if let Err(error) = std::fs::write(&when, kept_when) {
            return Err(crate::package::write_failed(
                "package record",
                jadpo_check::release::RECORDED,
                &error.to_string(),
            ));
        }
        Ok(self.shown(&file, &text, written, before.is_some(), &output_name, wrote))
    }

    /// The screen: what was sent, the type written and the recording.
    fn shown(
        &self,
        file: &Path,
        text: &str,
        written: bool,
        again: bool,
        output_name: &str,
        wrote: Option<&Wrote>,
    ) -> Reply {
        let (connection, operation) = (self.connection, self.operation);
        let name = format!("{connection}.{operation}");
        let shown = shown_path(self.root, file);
        let (placeholders, as_sent) = counts(text);
        let mut values = crate::package::plural(placeholders, "placeholder", "placeholders");
        if as_sent > 0 {
            values.push_str(&format!(", {as_sent} kept as sent"));
        }
        let request = format!("{} {}", self.exchange.method, self.exchange.url);
        let mut rows = vec![("sent".to_string(), request.clone())];
        if let Some(wrote) = wrote {
            rows.push((
                "output".to_string(),
                format!(
                    "written from the answer, {}",
                    crate::package::plural(wrote.written.types.len(), "type", "types")
                ),
            ));
        }
        rows.push(("answer".to_string(), format!("decodes as {output_name}")));
        rows.push(("values".to_string(), values));
        rows.push(("file".to_string(), shown.clone()));
        let mut screen = Screen::new().with(Block::Status {
            outcome: Outcome::Success,
            text: match (written, again) {
                (true, false) => format!("Recorded `{name}` from the sandbox"),
                (true, true) => format!("Recorded `{name}` again from the sandbox"),
                (false, _) => format!("Recorded `{name}` from the sandbox, as before"),
            },
            rows,
        });
        let mut facts = vec![
            Fact::plain(format!("{} {operation}", self.package)),
            Fact::plain("confirmed"),
        ];
        if let Some(wrote) = wrote {
            screen.push(Block::Section {
                heading: "Written from the answer".to_string(),
                lines: wrote.saved.iter().map(|(path, _)| path.clone()).collect(),
            });
            if let Some(block) = crate::package_import::to_check(&wrote.written.findings) {
                screen.push(block);
            }
            facts.push(Fact::plain(format!(
                "{} written",
                crate::package::plural(wrote.written.types.len(), "type", "types")
            )));
            if !wrote.written.findings.is_empty() {
                facts.push(Fact::plain(format!(
                    "{} to check",
                    wrote.written.findings.len()
                )));
            }
        }
        screen.push(Block::summary(
            Outcome::Success,
            "operation recorded",
            facts,
        ));
        screen.push(match wrote {
            Some(_) => Block::hint("jadpo check", "shows what to review in the types written"),
            None => Block::hint("jadpo test", "decodes the recording with every decoder"),
        });
        let mut data = vec![
            ("connection", Json::from(connection)),
            ("operation", Json::from(operation)),
            ("package", Json::from(self.package)),
            ("file", Json::from(shown.as_str())),
            ("written", written.into()),
            ("placeholders", Json::from(placeholders as i64)),
            ("keptAsSent", Json::from(as_sent as i64)),
            (
                "sent",
                Json::object([
                    ("method", Json::from(self.exchange.method.as_str())),
                    ("url", Json::from(self.exchange.url.as_str())),
                ]),
            ),
        ];
        if let Some(wrote) = wrote {
            let written = &wrote.written;
            data.push((
                "output",
                Json::object([
                    ("type", Json::from(written.output.as_str())),
                    (
                        "types",
                        Json::array(written.types.iter().map(|ty| Json::from(ty.as_str()))),
                    ),
                    (
                        "files",
                        Json::array(
                            wrote
                                .saved
                                .iter()
                                .map(|(path, _)| Json::from(path.as_str())),
                        ),
                    ),
                    (
                        "mapped",
                        Json::array(written.mapped.iter().map(|mapped| {
                            Json::object([
                                ("at", Json::from(mapped.at.as_str())),
                                ("standard", Json::from(mapped.standard.as_str())),
                                ("by", Json::from(mapped.by)),
                            ])
                        })),
                    ),
                    (
                        "findings",
                        Json::array(written.findings.iter().map(|finding| {
                            Json::object([
                                ("at", Json::from(finding.at.as_str())),
                                ("next", Json::from(finding.next.as_str())),
                                ("decides", Json::from("agent")),
                            ])
                        })),
                    ),
                ]),
            ));
        }
        Reply::screen(screen, Json::object(data))
    }
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
        // The setting that chooses the environment is set to `sandbox`;
        // a credential or address, to the sandbox's own.
        let chooser = match &chosen.address {
            Address::Hosts { environment, .. } => Some(variable(*environment)),
            _ => None,
        };
        let (chooses, others): (Vec<&String>, Vec<&String>) = unset
            .iter()
            .partition(|name| chooser.as_deref() == Some(name.as_str()));
        let others: Vec<String> = others.iter().map(|name| format!("`{name}`")).collect();
        let set = match (chooses.first(), others.is_empty()) {
            (Some(chooser), true) => format!("Set `{chooser}` to `sandbox` in `.env.local`"),
            (Some(chooser), false) => format!(
                "Set {} to the sandbox's and `{chooser}` to `sandbox` in `.env.local`",
                crate::package::and_list(&others)
            ),
            (None, _) => format!(
                "Set {} in `.env.local` to the sandbox's",
                crate::package::and_list(&others)
            ),
        };
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
            .next_marked(format!("{set}, then run it again (human)")),
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
/// What `recordings/recorded.jadpo_record` holds once `operation` is
/// recorded `at`: every other operation's line kept, its own written anew,
/// in operation order.
pub(crate) fn recorded_at(before: Option<&str>, operation: &str, at: &str) -> String {
    let mut lines: Vec<(String, String)> = before
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            (words.next()? == "recorded").then_some(())?;
            let named = words.next()?;
            let when = words.next()?;
            (named != operation).then(|| (named.to_string(), when.to_string()))
        })
        .collect();
    lines.push((operation.to_string(), at.to_string()));
    lines.sort();
    let mut out = String::from(
        "// Written by `jadpo package record`: when each recording was made.\nrecord recordings 1\n\n",
    );
    for (named, when) in lines {
        out.push_str(&format!("recorded {named} {when}\n"));
    }
    out
}

/// `seconds` since 1970 as RFC 3339 in UTC, to the second.
fn instant_text(seconds: i64) -> String {
    let day = crate::registry::day_from(seconds).unwrap_or_default();
    let of_day = seconds.rem_euclid(86_400);
    format!(
        "{day}T{:02}:{:02}:{:02}Z",
        of_day / 3600,
        of_day % 3600 / 60,
        of_day % 60
    )
}

#[cfg(test)]
mod tests {
    use super::{instant_text, recorded_at};

    #[test]
    fn a_recording_keeps_when_it_was_made_beside_the_others() {
        let first = recorded_at(None, "pickups.create", "2026-11-07T21:30:00Z");
        assert_eq!(
            first,
            "// Written by `jadpo package record`: when each recording was made.\nrecord recordings 1\n\nrecorded pickups.create 2026-11-07T21:30:00Z\n"
        );
        let both = recorded_at(Some(&first), "labels.get", "2026-11-08T08:00:00Z");
        assert!(both.ends_with(
            "recorded labels.get 2026-11-08T08:00:00Z\nrecorded pickups.create 2026-11-07T21:30:00Z\n"
        ));
        let again = recorded_at(Some(&both), "pickups.create", "2026-11-09T10:00:00Z");
        assert!(again.ends_with("recorded pickups.create 2026-11-09T10:00:00Z\n"));
        assert_eq!(
            jadpo_check::release::newest_recording(&again).as_deref(),
            Some("2026-11-09T10:00:00Z")
        );
    }

    #[test]
    fn an_instant_is_written_in_utc_to_the_second() {
        assert_eq!(instant_text(1_794_087_000), "2026-11-07T21:30:00Z");
        assert_eq!(instant_text(0), "1970-01-01T00:00:00Z");
    }
}
