//! `jadpo package conform <package> email` and `receiving` (area 11,
//! "Proving a package works"; area 14, "`jadpo package conform`"; chunk
//! K4): the suites of the capabilities a package fills besides `ai`'s
//! ([`crate::package_conform`]), run the same way. Each runs in a scratch
//! app holding the package as the app installed it, connected as
//! `provider` and named by the capability, so every case goes through the
//! package's mapping, its request, the package driver's HTTP and its own
//! types, as the app's own sending or receiving does. By default it runs
//! against a local fake on this machine's loopback address that answers
//! from the package's recordings, every request sent to it whatever host
//! the package names, so a package with fixed `hosts:` runs too.
//!
//! `email`'s four cases:
//!
//! ```text
//! Conformance suite
//! ✓ message     passed  sent, its receipt read
//! ✓ text only   passed  sent with no HTML
//! ✓ attachment  passed  1 file, its bytes sent
//! ✓ rejection   passed  5 failures, each read as mapped
//!
//! ✓ email conforms  Resend.v1 · 4 passed · its recordings and a local fake
//! → jadpo package conform resend email --live  sends one email to the app's own sender through the provider's API
//! ```
//!
//! `--live` sends through the app's own connection, the one `email:`
//! names, to the app's own sender address, so no one else gets mail; a
//! provider can't be made to reject on request, so the rejection is left
//! to the run against the recordings.
//!
//! `receiving`'s three cases: a batch of two messages read, each body
//! and handle where the mapping says; an empty batch read as no messages;
//! a message acknowledged by its handle. `receiving` has no live run: it
//! would take the app's own messages off its queue.

use std::path::Path;
use std::rc::Rc;

use jadpo_diag::{Diagnostic, rule};
use jadpo_interp::Program;
use jadpo_runtime::{Ctx, Env, Outcome as Sent};
use jadpo_std::http::CodecId;
use jadpo_term::{Environment, RowState};
use jadpo_value::Value;
use serde_json::Value as J;

use crate::conform_fake::{self, Answer, LocalFake, Step};
use crate::package::Scratch;
use crate::package_conform::{
    Against, CONNECTION, Case, Found, Live, Ran, defect, fake_environment, live_reach, mount,
    not_run, place, reach_line, reach_setting, recorded, refused, report, supplies,
};
use crate::package_record::NextMarked;
use crate::run::Reply;

/// What the email cases send.
const SENDER: &str = "conformance@jadpo.invalid";
const SUBJECT: &str = "Jadpo conformance check";
const HTML: &str = "<p>Jadpo conformance check</p>";
const TEXT: &str = "Jadpo conformance check.";
const FILE_NAME: &str = "conformance.txt";
const FILE_BYTES: &[u8] = b"Jadpo conformance check, attached.";
/// The provider's id for the message, as the local fake answers it.
const RECEIPT: &str = "conformance-receipt";

/// The messages the local fake hands the receiving cases, each its body
/// and handle.
const MESSAGES: [(&str, &str); 2] = [
    (
        "{\"id\":\"conformance-1\",\"note\":\"first\"}",
        "conformance-handle-1",
    ),
    (
        "{\"id\":\"conformance-2\",\"note\":\"second\"}",
        "conformance-handle-2",
    ),
];

// --- email -------------------------------------------------------------------

pub(crate) fn email(found: &Found<'_>) -> Reply {
    if !jadpo_check::mail::mappings(found.db, found.project)
        .of
        .contains_key(&found.home.folder)
    {
        return unread(found.package, jadpo_std::email::NAMESPACE);
    }
    if found.call.flag("--live") {
        return email_live(found);
    }
    let fake = match LocalFake::start() {
        Ok(fake) => fake,
        Err(error) => return fake_failed(&error),
    };
    let scratch = match scratch_app(found, jadpo_std::email::NAMESPACE) {
        Ok(scratch) => scratch,
        Err(reply) => return *reply,
    };
    let env = scratch_env(found, &fake);
    let ready = match crate::dev::prepare(&scratch.0, &env, true) {
        Ok(ready) => ready,
        Err((diagnostics, sources)) => return not_run(diagnostics, sources),
    };
    let against = Against::Fake(fake);
    let program = Program::new(std::sync::Arc::clone(&ready.prepared)).with_config(&ready.config);
    let recordings = scratch
        .0
        .join(mount(found.home))
        .join(jadpo_runtime::recorded::FOLDER);
    run_email(
        found,
        &program,
        CONNECTION,
        SENDER,
        &against,
        Some(&recordings),
    )
}

/// The live run: through the app's own connection, the one `email:`
/// names, to the app's own sender.
fn email_live(found: &Found<'_>) -> Reply {
    let ready = match crate::dev::prepare(found.root, &found.call.environment, true) {
        Ok(ready) => ready,
        Err((diagnostics, sources)) => return not_run(diagnostics, sources),
    };
    let plan = ready.prepared.plan();
    let connection = plan
        .package_connections()
        .find(|(_, _, known)| known.package == found.package)
        .map(|(_, name, known)| (name, known));
    let (
        name,
        Live {
            host,
            key,
            environment,
        },
    ) = match live_reach(&ready, connection, found.package, found.fixed) {
        Ok(reached) => reached,
        Err(refusal) => return Reply::failed(*refusal),
    };
    let program = Program::new(std::sync::Arc::clone(&ready.prepared)).with_config(&ready.config);
    let emails = program.runtime_plan().events.map(|events| &events.emails);
    let through = emails.is_some_and(|emails| {
        emails.connection.as_deref() == Some(name) && emails.package.is_some()
    });
    if !through {
        return Reply::failed(*refused(
            format!("`email:` doesn't send through `{name}`"),
            "`--live` sends one email through the connection the app's `email:` names, so it proves what the app will send, and `email:` names another connection or none.".to_string(),
            format!("Name `{name}` in `email: {{ connection: }}`, then run it again (agent)"),
        ));
    }
    let sender = emails
        .and_then(|emails| emails.from.as_ref())
        .and_then(|from| match from {
            jadpo_std::email::Sender::Address(address) => Some(address.clone()),
            jadpo_std::email::Sender::Setting(at) => ready.config.text_at(*at).map(str::to_string),
        })
        .filter(|sender| !sender.is_empty());
    let Some(sender) = sender else {
        return Reply::failed(*refused(
            "The app's `email:` has no sender set".to_string(),
            "`--live` sends one email to the app's own sender address, `email: { from: }`, so no one else gets it, and its setting is empty.".to_string(),
            "Set the sender's setting in `.env.local`, then run it again (human)".to_string(),
        ));
    };
    let against = Against::Live {
        host,
        key,
        environment,
    };
    run_email(found, &program, name, &sender, &against, None)
}

fn run_email(
    found: &Found<'_>,
    program: &Program,
    connection: &str,
    sender: &str,
    against: &Against,
    recordings: Option<&Path>,
) -> Reply {
    let Some(package) = program
        .runtime_plan()
        .events
        .and_then(|events| events.emails.package.clone())
    else {
        return defect("The suite's app sends no email through the package");
    };
    let log = Rc::new(crate::package_conform::Log::default());
    let host = jadpo_host_native::NativeHost::new()
        .with_drivers(
            jadpo_host_native::Drivers::build(
                program.runtime_plan().connections,
                program.started(),
                &supplies(against),
            )
            .keeping(|asked| asked == connection),
        )
        .with_call_log(Rc::clone(&log) as Rc<dyn jadpo_runtime::call_log::CallLog>);
    let runtime = match jadpo_host_native::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => return runtime_failed(&error.to_string()),
    };
    let run = Run {
        program,
        host: &host,
        runtime: &runtime,
        log: &log,
        package: found.package,
        capability: jadpo_std::email::NAMESPACE,
        recordings,
        against,
    };
    let mail = Mail {
        run: &run,
        plan: &package,
        connection,
        sender,
    };
    let cases = vec![
        mail.message(),
        mail.text_only(),
        mail.attachment(),
        mail.rejection(),
    ];
    report(
        &Ran {
            capability: jadpo_std::email::NAMESPACE,
            package: found.package,
            name: found.wanted,
            against,
            live_words: Some("sends one email to the app's own sender through the provider's API"),
        },
        cases,
    )
}

/// The email cases.
struct Mail<'r> {
    run: &'r Run<'r>,
    plan: &'r jadpo_std::email::PackageEmail,
    connection: &'r str,
    sender: &'r str,
}

impl Mail<'_> {
    fn message_value(&self, html: bool, attached: bool) -> Value {
        let text = |value: &str| Value::Text(Rc::from(value));
        let attachments: Vec<Value> = if attached {
            vec![Value::record(
                "EmailAttachment",
                vec![
                    (Rc::from("name"), text(FILE_NAME)),
                    (Rc::from("content"), Value::Bytes(Rc::from(FILE_BYTES))),
                    (Rc::from("content_type"), text("text/plain")),
                ],
            )]
        } else {
            Vec::new()
        };
        Value::record(
            "EmailMessage",
            vec![
                (Rc::from("from"), text(self.sender)),
                (Rc::from("to"), text(self.sender)),
                (Rc::from("subject"), text(SUBJECT)),
                (
                    Rc::from("html"),
                    if html { text(HTML) } else { Value::None },
                ),
                (Rc::from("text"), text(TEXT)),
                (Rc::from("attachments"), Value::List(attachments.into())),
            ],
        )
    }

    /// Sends `message`, the fake answering `answer`: what the send ended
    /// in, and the request's body as sent.
    fn send(&self, message: &Value, answer: Option<Answer>) -> (Sent, Option<J>) {
        if let (Some(fake), Some(answer)) = (self.run.fake(), answer) {
            fake.answer(answer);
        }
        self.run.log.0.borrow_mut().clear();
        let mut ctx = self.run.ctx();
        let outcome = self
            .run
            .runtime
            .block_on(jadpo_runtime::email::send_package(
                self.run.program,
                &mut ctx,
                self.connection,
                self.plan,
                message,
            ));
        let body = self
            .run
            .log
            .0
            .borrow()
            .last()
            .and_then(|exchange| exchange.body.clone())
            .and_then(|body| serde_json::from_str::<J>(&body).ok());
        (outcome, body)
    }

    /// The send operation's recording, its receipt where `receipt:` reads
    /// it.
    fn sent_answer(&self) -> Result<Option<Answer>, String> {
        if self.run.fake().is_none() {
            return Ok(None);
        }
        let operation = &self.plan.operation;
        let (mut answer, at, codec) = self.run.recording(operation)?;
        if let Some(receipt) = &self.plan.receipt {
            let place = self.run.wire(codec, at.as_deref(), receipt);
            if !place.is_some_and(|place| conform_fake::put(&mut answer, &place, &J::from(RECEIPT)))
            {
                return Err(format!(
                    "the recording of `{operation}` has no `{}`, where `receipt:` reads the provider's id",
                    receipt.join(".")
                ));
            }
        }
        Ok(Some(Answer::json(answer.to_string())))
    }

    fn sent_whole(&self, name: &'static str, outcome: &Sent) -> Option<Case> {
        match outcome {
            Sent::Value(_) => None,
            other => Some(self.run.misread(
                name,
                "not sent".into(),
                format!("The email ended in {}.", ended(other)),
                "email",
            )),
        }
    }

    fn message(&self) -> Case {
        const NAME: &str = "message";
        let answer = match self.sent_answer() {
            Ok(answer) => answer,
            Err(why) => return self.run.unrecorded(NAME, why),
        };
        let (outcome, body) = self.send(&self.message_value(true, false), answer);
        if let Some(failed) = self.sent_whole(NAME, &outcome) {
            return failed;
        }
        let texts = body.as_ref().map(strings).unwrap_or_default();
        let missing: Vec<&str> = [
            ("from", self.sender),
            ("subject", SUBJECT),
            ("html", HTML),
            ("text", TEXT),
        ]
        .into_iter()
        .filter(|(_, value)| !texts.iter().any(|text| text == value))
        .map(|(part, _)| part)
        .collect();
        if !missing.is_empty() {
            return self.run.misread(
                NAME,
                "a part not sent".into(),
                format!(
                    "The request didn't carry the message's {}, which `input:` maps.",
                    crate::package::and_list(
                        &missing
                            .iter()
                            .map(|part| format!("`{part}`"))
                            .collect::<Vec<_>>()
                    )
                ),
                "email",
            );
        }
        match (&self.plan.receipt, &outcome) {
            (None, _) => Case::passed(NAME, "sent; it maps no receipt:"),
            (Some(_), Sent::Value(Value::Text(text)))
                if self.run.fake().is_none() || &**text == RECEIPT =>
            {
                Case::passed(NAME, "sent, its receipt read")
            }
            (Some(path), _) => self.run.misread(
                NAME,
                "no receipt read".into(),
                format!(
                    "The email was sent, but no text was read at `{}`, where `receipt:` reads the provider's id for it.",
                    path.join(".")
                ),
                "email",
            ),
        }
    }

    fn text_only(&self) -> Case {
        const NAME: &str = "text only";
        let answer = match self.sent_answer() {
            Ok(answer) => answer,
            Err(why) => return self.run.unrecorded(NAME, why),
        };
        let (outcome, body) = self.send(&self.message_value(false, false), answer);
        if let Some(failed) = self.sent_whole(NAME, &outcome) {
            return failed;
        }
        if body.as_ref().is_some_and(has_null) {
            return self.run.misread(
                NAME,
                "null sent".into(),
                "A message with no HTML was sent with `null` in its place, which a provider needn't take: a part the message doesn't have is left out.".into(),
                "email",
            );
        }
        Case::passed(NAME, "sent with no HTML")
    }

    fn attachment(&self) -> Case {
        const NAME: &str = "attachment";
        let answer = match self.sent_answer() {
            Ok(answer) => answer,
            Err(why) => return self.run.unrecorded(NAME, why),
        };
        let (outcome, body) = self.send(&self.message_value(true, true), answer);
        if let Some(failed) = self.sent_whole(NAME, &outcome) {
            return failed;
        }
        let encoded = jadpo_value::encoding::to_base64(FILE_BYTES);
        let texts = body.as_ref().map(strings).unwrap_or_default();
        let sent = |value: &str| texts.iter().any(|text| text == value);
        if !sent(&encoded) || !sent(FILE_NAME) {
            return self.run.misread(
                NAME,
                "file not sent".into(),
                "The request didn't carry the attachment's name and its bytes in base64, which `each: message.attachments` maps.".into(),
                "email",
            );
        }
        Case::passed(NAME, "1 file, its bytes sent")
    }

    fn rejection(&self) -> Case {
        const NAME: &str = "rejection";
        let Some(fake) = self.run.fake() else {
            return Case::skipped(NAME, "a provider can't be made to reject on request");
        };
        if self.plan.failures.is_empty() {
            return Case::skipped(NAME, format!("{} maps no failures:", self.run.package));
        }
        let plan = self.run.program.plan();
        let Some((connection, op)) =
            plan.providers()
                .operation(plan.connections(), CONNECTION, &self.plan.operation)
        else {
            return defect_case(NAME);
        };
        let aws = connection.protocol == jadpo_std::provider::Protocol::AwsJson;
        for (code, mapped) in &self.plan.failures {
            let failure = op
                .failures
                .iter()
                .filter_map(|key| {
                    self.run
                        .program
                        .runtime_plan()
                        .failures
                        .iter()
                        .find(|failure| failure.key == *key)
                })
                .find(|failure| failure.code == *code);
            let status = failure
                .and_then(|failure| failure.wire_status)
                .unwrap_or(422);
            let text = failure
                .and_then(|failure| failure.wire_name.clone())
                .unwrap_or_else(|| {
                    if aws {
                        format!("com.amazonaws.conformance#{}", pascal(code))
                    } else {
                        code.clone()
                    }
                });
            let mut body = J::Object(serde_json::Map::new());
            if let Some(at) = &connection.error_code {
                conform_fake::put_new(&mut body, &conform_fake::dotted(at), &J::from(text));
            }
            fake.answer(Answer {
                status,
                content_type: "application/json",
                body: body.to_string(),
            });
            let (outcome, _) = self.send(&self.message_value(true, false), None);
            let read = match &outcome {
                Sent::Failed(failure) => Some(failure.code.clone()),
                _ => None,
            };
            if read.as_deref() != Some(mapped.as_str()) {
                return self.run.misread(
                    NAME,
                    format!("{code} not read"),
                    format!(
                        "The provider's `{code}` was answered, and the email ended in {}, not `EmailFailure.{mapped}`, which `failures:` maps it to.",
                        ended(&outcome)
                    ),
                    "email",
                );
            }
        }
        Case::passed(
            NAME,
            format!(
                "{}, each read as mapped",
                crate::package::plural(self.plan.failures.len(), "failure", "failures")
            ),
        )
    }
}

// --- receiving ---------------------------------------------------------------

pub(crate) fn receiving(found: &Found<'_>) -> Reply {
    const CAPABILITY: &str = "receiving";
    if !jadpo_check::receiving::mappings(found.db, found.project)
        .of
        .contains_key(&found.home.folder)
    {
        return unread(found.package, CAPABILITY);
    }
    if found.call.flag("--live") {
        return Reply::failed(*refused(
            "`receiving` has no live run".to_string(),
            "Receiving takes messages off a queue and acknowledges them, so a live run through the app's own connection would take the app's own messages from it. The suite runs against the package's recordings and a local fake, where nothing leaves this machine.".to_string(),
            format!(
                "Run `jadpo package conform {} receiving` without `--live` (agent)",
                found.home.name
            ),
        ));
    }
    let fake = match LocalFake::start() {
        Ok(fake) => fake,
        Err(error) => return fake_failed(&error),
    };
    let scratch = match scratch_app(found, CAPABILITY) {
        Ok(scratch) => scratch,
        Err(reply) => return *reply,
    };
    // Its events' store needs a first migration, as an app's does.
    let _ = crate::run::run(
        &[
            "migrate".to_string(),
            "new".to_string(),
            "initial".to_string(),
        ],
        &Environment::empty().with_cwd(&scratch.0),
    );
    let env = scratch_env(found, &fake).with_var(
        "DATABASE_URL",
        "postgres://conformance@127.0.0.1:9/conformance",
    );
    let ready = match crate::dev::prepare(&scratch.0, &env, true) {
        Ok(ready) => ready,
        Err((diagnostics, sources)) => return not_run(diagnostics, sources),
    };
    let against = Against::Fake(fake);
    let program = Program::new(std::sync::Arc::clone(&ready.prepared)).with_config(&ready.config);
    let recordings = scratch
        .0
        .join(mount(found.home))
        .join(jadpo_runtime::recorded::FOLDER);
    let log = Rc::new(crate::package_conform::Log::default());
    let host = jadpo_host_native::NativeHost::new()
        .with_drivers(
            jadpo_host_native::Drivers::build(
                program.runtime_plan().connections,
                program.started(),
                &supplies(&against),
            )
            .keeping(|asked| asked == CONNECTION),
        )
        .with_call_log(Rc::clone(&log) as Rc<dyn jadpo_runtime::call_log::CallLog>);
    let runtime = match jadpo_host_native::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => return runtime_failed(&error.to_string()),
    };
    let run = Run {
        program: &program,
        host: &host,
        runtime: &runtime,
        log: &log,
        package: found.package,
        capability: CAPABILITY,
        recordings: Some(&recordings),
        against: &against,
    };
    let Some(receiving) = program.runtime_plan().receiving() else {
        return defect("The suite's app receives no queue");
    };
    let Some(queue) = receiving.queues.first() else {
        return defect("The suite's app receives no queue");
    };
    let jadpo_std::receiving::QueueSource::Package {
        receive,
        acknowledge,
    } = &queue.source
    else {
        return defect("The suite's queue isn't read through the package");
    };
    let queued = Queued {
        run: &run,
        receiving: &receiving,
        queue,
        receive,
        acknowledge,
    };
    let cases = vec![queued.batch(), queued.empty(), queued.acknowledged()];
    report(
        &Ran {
            capability: CAPABILITY,
            package: found.package,
            name: found.wanted,
            against: &against,
            live_words: None,
        },
        cases,
    )
}

/// The receiving cases.
struct Queued<'r> {
    run: &'r Run<'r>,
    receiving: &'r jadpo_runtime::receiving::Receiving<'r>,
    queue: &'r jadpo_std::receiving::QueuePlan,
    receive: &'r jadpo_std::receiving::ReceivePlan,
    acknowledge: &'r jadpo_std::receiving::AcknowledgePlan,
}

impl Queued<'_> {
    /// The receive operation's recording with `messages` where `messages:`
    /// reads them, each made from the recording's first.
    fn batch_answer(&self, messages: &[(&str, &str)]) -> Result<Answer, String> {
        let operation = &self.receive.operation;
        let (mut answer, at, codec) = self.run.recording(operation)?;
        let list_path = &self.receive.messages;
        let Some(list) = self.run.wire(codec, at.as_deref(), list_path) else {
            return Err(format!(
                "the recording of `{operation}` has no `{}`, where `messages:` reads them",
                list_path.join(".")
            ));
        };
        let item_steps = |path: &[String]| {
            let mut full = list_path.clone();
            full.push("0".to_string());
            full.extend(path.iter().cloned());
            let steps = self.run.wire(codec, at.as_deref(), &full)?;
            steps.get(list.len() + 1..).map(<[Step]>::to_vec)
        };
        let (Some(body), Some(handle)) = (
            item_steps(&self.receive.body),
            item_steps(&self.receive.handle),
        ) else {
            return Err(format!(
                "the recording of `{operation}` has no message with `{}` and `{}`, where `body:` and `handle:` read them",
                self.receive.body.join("."),
                self.receive.handle.join(".")
            ));
        };
        let first = match conform_fake::get(&answer, &list) {
            Some(J::Array(items)) => items.first().cloned(),
            _ => None,
        };
        let Some(first) = first else {
            return Err(format!(
                "the recording of `{operation}` has no message at `{}` to make others from",
                list_path.join(".")
            ));
        };
        let mut items = Vec::new();
        for (text, handed) in messages {
            let mut item = first.clone();
            if !conform_fake::put(&mut item, &body, &J::from(*text))
                || !conform_fake::put(&mut item, &handle, &J::from(*handed))
            {
                return Err(format!(
                    "the recording of `{operation}`'s message has no `{}` or `{}`",
                    self.receive.body.join("."),
                    self.receive.handle.join(".")
                ));
            }
            items.push(item);
        }
        conform_fake::put(&mut answer, &list, &J::Array(items));
        Ok(Answer::json(answer.to_string()))
    }

    fn fetch(&self) -> Result<Vec<(String, String)>, String> {
        let mut ctx = self.run.ctx();
        self.run
            .runtime
            .block_on(jadpo_runtime::receiving::fetch(
                self.run.program,
                self.receiving,
                self.queue,
                self.receive,
                &mut ctx,
            ))
            .map_err(|error| said(&error))
    }

    fn batch(&self) -> Case {
        const NAME: &str = "messages";
        match self.batch_answer(&MESSAGES) {
            Ok(answer) => self.run.answer(answer),
            Err(why) => return self.run.unrecorded(NAME, why),
        }
        let read = match self.fetch() {
            Ok(read) => read,
            Err(why) => {
                return self.run.misread(
                    NAME,
                    "not read".into(),
                    format!("Receiving a batch of two messages ended in {why}."),
                    "receiving",
                );
            }
        };
        let expected: Vec<(String, String)> = MESSAGES
            .iter()
            .map(|(body, handle)| (body.to_string(), handle.to_string()))
            .collect();
        if read != expected {
            return self.run.misread(
                NAME,
                format!("{} of 2 read", read.len()),
                format!(
                    "The local fake answered two messages, each its body at `{}` and its handle at `{}`, and {} came back as sent.",
                    self.receive.body.join("."),
                    self.receive.handle.join("."),
                    read.iter().filter(|one| expected.contains(one)).count()
                ),
                "receiving",
            );
        }
        Case::passed(NAME, "2 read, each body and handle")
    }

    fn empty(&self) -> Case {
        const NAME: &str = "empty queue";
        match self.batch_answer(&MESSAGES).map(|_| ()) {
            Ok(()) => {}
            Err(why) => return self.run.unrecorded(NAME, why),
        }
        let operation = &self.receive.operation;
        let answer = self
            .run
            .recording(operation)
            .and_then(|(mut answer, at, codec)| {
                let list = self
                    .run
                    .wire(codec, at.as_deref(), &self.receive.messages)
                    .ok_or_else(|| {
                        format!(
                            "the recording of `{operation}` has no `{}`",
                            self.receive.messages.join(".")
                        )
                    })?;
                conform_fake::put(&mut answer, &list, &J::Array(Vec::new()));
                Ok(answer)
            });
        match answer {
            Ok(answer) => self.run.answer(Answer::json(answer.to_string())),
            Err(why) => return self.run.unrecorded(NAME, why),
        }
        match self.fetch() {
            Ok(read) if read.is_empty() => Case::passed(NAME, "read as no messages"),
            Ok(read) => self.run.misread(
                NAME,
                format!("{} read", read.len()),
                "An answer with no messages was read as having some.".into(),
                "receiving",
            ),
            Err(why) => self.run.misread(
                NAME,
                "not read".into(),
                format!("An answer with no messages ended in {why}."),
                "receiving",
            ),
        }
    }

    fn acknowledged(&self) -> Case {
        const NAME: &str = "acknowledge";
        let operation = &self.acknowledge.operation;
        let plan = self.run.program.plan();
        let output = plan
            .providers()
            .operation(plan.connections(), CONNECTION, operation)
            .and_then(|(_, op)| op.output);
        let answer = if output.is_some() {
            match self.run.recording(operation) {
                Ok((answer, _, _)) => Answer::json(answer.to_string()),
                Err(why) => return self.run.unrecorded(NAME, why),
            }
        } else {
            Answer::json(String::new())
        };
        self.run.answer(answer);
        let handle = MESSAGES[0].1;
        self.run.log.0.borrow_mut().clear();
        let mut ctx = self.run.ctx();
        let done = self
            .run
            .runtime
            .block_on(jadpo_runtime::receiving::acknowledged(
                self.run.program,
                self.receiving,
                self.queue,
                self.acknowledge,
                &mut ctx,
                handle,
            ));
        if let Err(why) = done {
            return self.run.misread(
                NAME,
                "not acknowledged".into(),
                format!(
                    "Acknowledging a message by its handle ended in {}.",
                    said(&why)
                ),
                "receiving",
            );
        }
        let sent = self
            .run
            .log
            .0
            .borrow()
            .last()
            .and_then(|exchange| exchange.body.clone())
            .and_then(|body| serde_json::from_str::<J>(&body).ok())
            .map(|body| strings(&body))
            .unwrap_or_default();
        if !sent.iter().any(|text| text == handle) {
            return self.run.misread(
                NAME,
                "no handle sent".into(),
                "The request didn't carry the message's handle, which `acknowledge:` maps as `message.handle`.".into(),
                "receiving",
            );
        }
        Case::passed(NAME, "by its handle")
    }
}

// --- shared ------------------------------------------------------------------

/// What a suite runs with.
struct Run<'r> {
    program: &'r Program,
    host: &'r jadpo_host_native::NativeHost,
    runtime: &'r jadpo_host_native::Runtime,
    log: &'r crate::package_conform::Log,
    package: &'r str,
    capability: &'r str,
    /// The package's recordings, against the local fake.
    recordings: Option<&'r Path>,
    against: &'r Against,
}

impl Run<'_> {
    fn fake(&self) -> Option<&LocalFake> {
        match self.against {
            Against::Fake(fake) => Some(fake),
            Against::Live { .. } => None,
        }
    }

    fn answer(&self, answer: Answer) {
        if let Some(fake) = self.fake() {
            fake.answer(answer);
        }
    }

    fn ctx(&self) -> Ctx<'_> {
        let mut ctx = Ctx::of(self.program, self.host, Env::default());
        ctx.allow_standalone_calls();
        ctx
    }

    fn recording(&self, operation: &str) -> Result<(J, Option<String>, Option<CodecId>), String> {
        let Some(recordings) = self.recordings else {
            return Err("the live run reads no recording".to_string());
        };
        recorded(self.program, recordings, operation)
    }

    /// Places by field names in `codec`'s type, as places by spellings on
    /// the wire.
    fn wire(&self, codec: Option<CodecId>, at: Option<&str>, path: &[String]) -> Option<Vec<Step>> {
        let json = self.program.stored_json();
        let steps = conform_fake::by_fields(&json.codecs, codec?, path)?;
        Some(conform_fake::under(at, steps))
    }

    fn failed(&self, name: &'static str, detail: String, reason: String, next: String) -> Case {
        let problem = Diagnostic::new(
            &rule::CONFORM_FAILED,
            format!(
                "`{}`'s {name} doesn't conform to `{}`",
                self.package, self.capability
            ),
        )
        .reason(reason)
        .next_marked(next);
        Case {
            name,
            state: RowState::Failed,
            detail,
            problem: Some(problem),
        }
    }

    /// A case the mapping got wrong.
    fn misread(&self, name: &'static str, detail: String, reason: String, file: &str) -> Case {
        let next = if self.against.live() {
            format!(
                "Check `provides/{file}.jadpo` against the provider's documentation and its answer, then run it again (agent)"
            )
        } else {
            format!(
                "Correct `provides/{file}.jadpo` against the provider's documentation and the package's recordings, then run it again (agent)"
            )
        };
        self.failed(name, detail, reason, next)
    }

    /// A case the local fake has no recording to answer from.
    fn unrecorded(&self, name: &'static str, why: String) -> Case {
        self.failed(
            name,
            "not in its recordings".to_string(),
            format!("The local fake answers from the package's recordings, and {why}."),
            "Record it with `jadpo package record <connection> <operation>` against the provider's sandbox, then run it again (human)".to_string(),
        )
    }
}

/// The scratch app a suite runs in: the package as the app holds it,
/// connected as `provider` with stand-ins for its credential and
/// settings, and `capability` going through it.
fn scratch_app(found: &Found<'_>, capability: &str) -> Result<Scratch, Box<Reply>> {
    let scratch = Scratch::new().map_err(|error| {
        Box::new(crate::package::write_failed(
            "package conform",
            "a scratch folder",
            &error.to_string(),
        ))
    })?;
    let reach = jadpo_check::connections::reach(found.db, found.project, found.home);
    let credential = match reach.as_ref().and_then(|reach| reach.credential.as_deref()) {
        Some("aws_sigv4") => {
            "            credential: {\n                key_id: config.conformance_key_id\n                secret: config.conformance_key\n            }\n"
        }
        Some(_) => "            credential: config.conformance_key\n",
        None => "",
    };
    let credential_settings = match reach.as_ref().and_then(|reach| reach.credential.as_deref()) {
        Some("aws_sigv4") => {
            "    conformance_key_id: Text\n    conformance_key: Text {\n        secret: true\n    }\n"
        }
        Some(_) => "    conformance_key: Text {\n        secret: true\n    }\n",
        None => "",
    };
    // Each of the package's settings that has no default, given a
    // stand-in of its type: a label, so one filling a host's placeholder
    // is one DNS label.
    let settings: String = reach
        .as_ref()
        .map(|reach| {
            reach
                .settings
                .iter()
                .filter(|(name, _)| !reach.defaults.iter().any(|(given, _)| given == name))
                .map(|(name, ty)| {
                    let value = match ty.name() {
                        "Int" => "1",
                        "Decimal" => "1.0",
                        "Bool" => "false",
                        "Duration" => "1s",
                        _ => "\"conformance\"",
                    };
                    format!("            {name}: {value}\n")
                })
                .collect()
        })
        .unwrap_or_default();
    let connection = format!(
        "        {CONNECTION}: {{\n            package: {}\n{}{credential}            {}\n{settings}        }}\n",
        found.package,
        if capability == jadpo_std::email::NAMESPACE {
            ""
        } else {
            "            domains: [conformance]\n"
        },
        reach_line(found.fixed),
    );
    let mut files = Vec::new();
    if capability == jadpo_std::email::NAMESPACE {
        files.push((
            "app.jadpo".to_string(),
            format!(
                "application {{\n    connections: {{\n{connection}    }}\n    email: {{\n        connection: {CONNECTION}\n        from: config.conformance_sender\n    }}\n}}\n"
            ),
        ));
        files.push((
            "config.jadpo".to_string(),
            format!(
                "config {{\n    conformance_sender: Email {{\n        default: \"{SENDER}\"\n    }}\n{}{credential_settings}}}\n",
                reach_setting(found.fixed)
            ),
        ));
    } else {
        files.push((
            "app.jadpo".to_string(),
            format!(
                "use conformance/events {{ ConformanceReceived }}\n\napplication {{\n    connections: {{\n        main: {{\n            driver: Driver.postgres\n            url: config.database_url\n        }}\n{connection}    }}\n    events: {{\n        store: main\n    }}\n    receiving: {{\n        {CONNECTION}: {{\n            id_at: \"id\"\n            events: [ConformanceReceived]\n        }}\n    }}\n}}\n"
            ),
        ));
        files.push((
            "config.jadpo".to_string(),
            format!(
                "config {{\n    database_url: PostgresUrl\n{}{credential_settings}}}\n",
                reach_setting(found.fixed)
            ),
        ));
        files.push((
            "domains/conformance/events/conformance_received.jadpo".to_string(),
            "/// A message the suite receives.\ntype ConformanceReceived = Event {\n    note: Text {\n        max_length: 200\n    }\n}\n".to_string(),
        ));
        files.push((
            "domains/conformance/entities/conformance_note.jadpo".to_string(),
            "/// What the suite's app keeps, so its events have a store.\nentity ConformanceNote {\n    id: Uuid\n    note: Text {\n        max_length: 200\n    }\n}\n".to_string(),
        ));
    }
    place(
        &scratch.0,
        found.source,
        found.home,
        found.lock,
        found.package,
        files,
    )
    .map_err(|error| {
        Box::new(crate::package::write_failed(
            "package conform",
            "the scratch app the suite runs in",
            &error,
        ))
    })?;
    Ok(scratch)
}

/// The scratch app's settings against the local fake: stand-ins, never
/// the app's own credential.
fn scratch_env(found: &Found<'_>, fake: &LocalFake) -> Environment {
    let env = Environment::empty()
        .with_var("CONFORMANCE_KEY", "conformance-stand-in")
        .with_var("CONFORMANCE_KEY_ID", "conformance-stand-in");
    if found.fixed {
        let provider =
            std::fs::read_to_string(found.source.join(jadpo_db::PROVIDER_FILE)).unwrap_or_default();
        env.with_var("CONFORMANCE_ENVIRONMENT", fake_environment(&provider))
    } else {
        env.with_var("CONFORMANCE_HOST", &fake.url)
    }
}

/// Why receiving or acknowledging stopped, in a few words: never a
/// value.
fn said(error: &jadpo_runtime::StoreError) -> String {
    match error {
        jadpo_runtime::StoreError::Fault(why) => why.clone(),
        jadpo_runtime::StoreError::Unavailable => "the provider being unavailable".to_string(),
        other => format!("{other:?}"),
    }
}

/// Every text in `json`, however deep.
fn strings(json: &J) -> Vec<String> {
    match json {
        J::String(text) => vec![text.clone()],
        J::Array(items) => items.iter().flat_map(strings).collect(),
        J::Object(members) => members.values().flat_map(strings).collect(),
        _ => Vec::new(),
    }
}

/// Whether `json` holds a `null` anywhere.
fn has_null(json: &J) -> bool {
    match json {
        J::Null => true,
        J::Array(items) => items.iter().any(has_null),
        J::Object(members) => members.values().any(has_null),
        _ => false,
    }
}

/// `recipient_rejected` as `RecipientRejected`.
fn pascal(code: &str) -> String {
    code.split('_')
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + chars.as_str()
            })
        })
        .collect()
}

/// What a send ended in, in a few words.
fn ended(outcome: &Sent) -> String {
    match outcome {
        Sent::Value(_) => "an answer".to_string(),
        Sent::Failed(failure) => format!("the failure `{}`", failure.code),
        Sent::Fault(fault) => format!("a fault, {}", fault.kind.classification()),
        other => format!("{other:?}"),
    }
}

fn unread(package: &str, capability: &str) -> Reply {
    Reply::failed(
        Diagnostic::new(
            &rule::CONFORM_FAILED,
            format!("`{package}`'s `provide {capability}` doesn't read"),
        )
        .reason(
            "The suite goes through the package's mapping, and `jadpo check` finds problems in it.",
        )
        .next_marked(format!(
            "Run `jadpo check`, correct `provides/{capability}.jadpo`, then run it again (agent)"
        )),
    )
}

fn fake_failed(error: &std::io::Error) -> Reply {
    Reply::failed(
        Diagnostic::new(&rule::CONFORM_FAILED, "The local fake couldn't start")
            .reason(format!(
                "The suite answers from a server on this machine's loopback address, and listening there failed: {error}."
            ))
            .next_marked("Check nothing stops programs listening on 127.0.0.1, then run it again (human)"),
    )
}

fn runtime_failed(error: &str) -> Reply {
    Reply::failed(
        Diagnostic::new(&rule::CONFORM_FAILED, "The conformance suite couldn't run")
            .reason(format!(
                "The runtime that sends its calls couldn't start: {error}."
            ))
            .next_marked("Free some memory, then run it again (human)"),
    )
}

fn defect_case(name: &'static str) -> Case {
    Case {
        name,
        state: RowState::Failed,
        detail: "not run".into(),
        problem: Some(
            Diagnostic::new(
                &rule::CONFORM_FAILED,
                "The suite's app doesn't have the package's operation".to_string(),
            )
            .reason("The suite's own app didn't compile as it should.")
            .next_marked("Report it to Jadpo's maintainers with this report (human)"),
        ),
    }
}
