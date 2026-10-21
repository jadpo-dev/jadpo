//! Provider calls (area 11, "Calling" and "Decoding"; chunk 7.2c): a
//! provider package's operation called through its connection, on the
//! one outside-call path ([`crate::outside`]), so it is refused inside an
//! open transaction, logged without its values, and recorded and replayed
//! with the other effects.
//!
//! [`call`] is what both executors run at the call's site. It writes the
//! operation's input as the package's JSON, a provider package's
//! `wire_name:` spellings included, and makes the request the package
//! driver sends ([`Request`]): its address from the connection's `hosts:`
//! and environment, or its `host:`, held to HTTPS but on this machine; its
//! path with each placeholder filled from the input's member of that
//! spelling; the rest of the input as the query of a `GET` or `DELETE`
//! and as the JSON body of anything else; its credential as the package's
//! scheme sends it; and, for an operation that takes its package's key, a
//! key derived from the application, the environment, the connection, the
//! caller's scope, the action's key, the call's step and the key's
//! generation ([`key`]), so two callers who send one `Idempotency-Key`
//! never meet at a provider that keys by account.
//!
//! The driver ([`classify`]) reads what the provider answered: a `2xx`
//! as sent, unless `failed_when:` says it is a failure; the provider's
//! error code from `error_code:` for a refusal, which the call's site
//! raises as the package failure with that code, or a fault where the
//! operation declares none; a rejected credential as `misconfigured`; a
//! refusal that did nothing (`408`, `429`, the key still in use) as
//! `unavailable`; a server's error as an answer that may have acted. The
//! site decodes an answer from `output_at:` into the operation's output
//! with its codec; one that doesn't decode is a fault naming the
//! operation, the field and the rule, never the value.
//!
//! Since chunk 7.2c2 the input and the answer go through the codecs on the
//! provider's wire ([`jadpo_value::json::Coder`]): a field in a built-in
//! encoding, `wire: Wire.unix_seconds`, is written and read as the
//! provider sends it, and one with its package's own `decode:` and
//! `encode:` by those functions, which the executor runs at the call's
//! site ([`Functions`]); an operation's `wire_output:` is read whole and
//! turned into its output by its `decode:`. Only the fields of the answer
//! its site reads are decoded ([`CallSite`]). A call that may be sent again
//! is retried as its connection's `retry:` allows ([`retry_wait`]), by the
//! one retrying send every outside call takes ([`crate::outside::send`]),
//! under its connection's `max_concurrent:`; and one safe to repeat that
//! gets no answer in time is its site's `timed_out` failure where its
//! `attempt` maps it.

use std::fmt;
use std::rc::Rc;

use std::cell::RefCell;

use jadpo_std::HttpMethod;
use jadpo_std::provider::{
    Address, CallSite, ConnectionPlan, CredentialScheme, OperationPlan, PathPart, Protocol,
    ProviderPlan, RETRY_BASE_MS,
};
use jadpo_std::subscriber::Repeat;
use jadpo_std::types::FailurePlan;
use jadpo_value::Value;

use crate::capability::{Answer, Call};
use crate::effects::{Given, Unanswered};
use crate::{Ctx, Failure, FaultKind, JsonCodecs};

/// The most bytes of an answer read: past it, the answer is refused as one
/// that may have acted.
pub const MOST_ANSWER: usize = 10 << 20;

/// What a provider's answer is read for (the driver's part of decoding,
/// area 11): a `200` that is really a failure, where its code is, and the
/// provider's codes for a key in use or reused.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reading {
    /// The place, dotted, and the JSON value that marks a failure.
    pub failed_when: Option<(String, String)>,
    /// Where a failed answer's code is, dotted.
    pub error_code: Option<String>,
    pub in_use: Option<String>,
    pub reused: Option<String>,
    /// Whether the code is AWS's error type, `com.amazonaws.sqs#QueueDoesNotExist`,
    /// read as the failure's code it names, `queue_does_not_exist` (chunk
    /// 7.7c).
    pub aws: bool,
}

/// An AWS error type as the failure code it names: what follows its
/// namespace's `#`, in lower_snake_case.
pub fn aws_code(written: &str) -> String {
    let name = written.rsplit('#').next().unwrap_or(written);
    let name = name.split(':').next().unwrap_or(name);
    let mut out = String::new();
    for (at, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if at > 0 && !out.ends_with('_') {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else if ch == '.' || ch == '-' {
            out.push('_');
        } else {
            out.push(ch);
        }
    }
    out
}

/// A provider operation's request, as the package driver sends it.
#[derive(Clone, PartialEq, Eq)]
pub struct Request {
    pub method: HttpMethod,
    /// The whole address: scheme, host, path and query.
    pub url: String,
    /// Each header's name and value: the credential among them.
    pub headers: Vec<(String, String)>,
    /// The JSON body, for an operation that isn't a `GET` or `DELETE`.
    pub body: Option<String>,
    /// How long it waits for its answer.
    pub timeout_ms: u64,
    pub reading: Reading,
}

impl Request {
    /// The value of header `name`, ignoring case.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header, _)| header.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

impl fmt::Debug for Request {
    /// Its method and path only: never its query, headers or body, which
    /// hold the credential and the values sent.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let path = self.url.split('?').next().unwrap_or_default();
        f.debug_struct("Request")
            .field("method", &self.method.word())
            .field("url", &path)
            .finish_non_exhaustive()
    }
}

/// What a provider answered.
#[derive(Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl fmt::Debug for Response {
    /// Never its body.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Response")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

/// What a provider's answer is to the call ([`Answer`]), as the package
/// driver classifies it.
pub fn classify(request: &Request, response: &Response) -> Answer {
    if response.body.len() > MOST_ANSWER {
        return Answer::Unknown;
    }
    let text = String::from_utf8_lossy(&response.body);
    let json = if text.trim().is_empty() {
        Some(serde_json::Value::Null)
    } else {
        serde_json::from_str::<serde_json::Value>(&text).ok()
    };
    let reading = &request.reading;
    let code = || {
        let json = json.as_ref()?;
        let at = at_path(json, reading.error_code.as_deref()?)?;
        match at {
            serde_json::Value::String(code) if reading.aws => Some(aws_code(code)),
            serde_json::Value::String(code) => Some(code.clone()),
            serde_json::Value::Number(code) => Some(code.to_string()),
            _ => None,
        }
    };
    match response.status {
        200..=299 => {
            if let Some((path, equals)) = &reading.failed_when {
                let marked = serde_json::from_str::<serde_json::Value>(equals).ok();
                let found = json.as_ref().and_then(|json| at_path(json, path));
                if marked.is_some() && found == marked.as_ref() {
                    return Answer::Failed(code().unwrap_or_default());
                }
            }
            if json.is_none() {
                // Not JSON: decoding names it.
                return Answer::Wire(Rc::from(text.as_ref()));
            }
            Answer::Wire(Rc::from(if text.trim().is_empty() {
                "null"
            } else {
                text.as_ref()
            }))
        }
        // A rejected credential, and a redirect, which is never followed
        // (area 11, question 9): the connection's settings are wrong.
        300..=399 | 401 | 403 => Answer::Misconfigured,
        // It did nothing: sending it again is safe, after the wait it
        // asks for, if any (chunk 7.2c2).
        408 | 429 => match retry_after(response) {
            Some(millis) => Answer::RetryAfter(millis),
            None => Answer::Unavailable,
        },
        400..=499 => match code() {
            Some(code) if reading.in_use.as_deref() == Some(code.as_str()) => Answer::Unavailable,
            Some(code) => Answer::Failed(code),
            None => Answer::Failed(String::new()),
        },
        // A server's error may come after it acted.
        _ => Answer::Unknown,
    }
}

/// The wait a refusal's `Retry-After` asks for, in milliseconds: a whole
/// number of seconds (RFC 9110 §10.2.3); a date, which says when rather
/// than how long, is read as no wait given.
fn retry_after(response: &Response) -> Option<u64> {
    let (_, value) = response
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("retry-after"))?;
    let seconds: u64 = value.trim().parse().ok()?;
    seconds.checked_mul(1000)
}

/// How long to wait before sending a call again after attempt `attempt`
/// (1 for the first), with `left` milliseconds left of its `retry:` and of
/// its invocation's time, whichever ends sooner (chunk 7.9d): the
/// provider's `Retry-After` when it gave one, otherwise [`RETRY_BASE_MS`]
/// doubled for each attempt before, drawn evenly from zero to that ceiling
/// with `random`'s eight bytes (full jitter), or the ceiling itself without
/// them. `None` when the wait would outlast what is left.
pub fn retry_wait(
    attempt: u32,
    asked: Option<u64>,
    left: u64,
    random: Option<[u8; 8]>,
) -> Option<u64> {
    let wait = match asked {
        Some(asked) => asked,
        None => {
            let ceiling = RETRY_BASE_MS.saturating_mul(1 << attempt.saturating_sub(1).min(20));
            match random {
                Some(bytes) => u64::from_be_bytes(bytes) % (ceiling + 1),
                None => ceiling,
            }
        }
    };
    (wait < left).then_some(wait)
}

/// The value at `path`, dotted from the root of `json`: object members by
/// name, list items by number.
pub fn at_path<'j>(json: &'j serde_json::Value, path: &str) -> Option<&'j serde_json::Value> {
    path.split('.').try_fold(json, |at, step| match at {
        serde_json::Value::Object(members) => members.get(step),
        serde_json::Value::Array(items) => items.get(step.parse::<usize>().ok()?),
        _ => None,
    })
}

// --- the key ------------------------------------------------------------------

/// The idempotency key a keyed call sends (area 11, "Calling"; area 12,
/// "Keys"): `jd1_` and 32 base32 characters of a SHA-256 over the
/// application, the environment, the connection, the caller's scope, the
/// action's key, the step and the key's generation, each given by its
/// length so no two sets of parts run together; cut to `max_length` where
/// the provider takes fewer than its 36 characters.
pub fn key(parts: &KeyParts<'_>, max_length: Option<u32>) -> String {
    let mut hashed = Vec::new();
    for part in [
        parts.application,
        parts.environment,
        parts.connection,
        parts.scope,
        parts.action,
    ] {
        hashed.extend_from_slice(&(part.len() as u64).to_be_bytes());
        hashed.extend_from_slice(part.as_bytes());
    }
    hashed.extend_from_slice(&u64::from(parts.step).to_be_bytes());
    hashed.extend_from_slice(&u64::from(parts.generation).to_be_bytes());
    let digest = jadpo_value::hash::sha256(&hashed);
    let mut key = String::from("jd1_");
    key.push_str(&base32(digest.get(..20).unwrap_or(&digest)));
    if let Some(most) = max_length.and_then(|most| usize::try_from(most).ok())
        && key.len() > most
    {
        key.truncate(most);
    }
    key
}

/// What a key is derived from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyParts<'a> {
    pub application: &'a str,
    /// The provider's environment, `live` or `sandbox`, or the address a
    /// connection's `host:` gives.
    pub environment: &'a str,
    pub connection: &'a str,
    pub scope: &'a str,
    pub action: &'a str,
    /// The call's place among the action's keyed calls, from 1.
    pub step: u32,
    /// How many times its key was replaced (a provider's `new_key_on:`,
    /// with multi-step calls): 0.
    pub generation: u32,
}

/// RFC 4648's base32 in lower case, without padding.
fn base32(bytes: &[u8]) -> String {
    const LETTERS: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut out = String::with_capacity(bytes.len().div_ceil(5) * 8);
    let mut held: u32 = 0;
    let mut bits = 0;
    for byte in bytes {
        held = (held << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let at = ((held >> bits) & 31) as usize;
            out.push(char::from(*LETTERS.get(at).unwrap_or(&b'a')));
        }
    }
    if bits > 0 {
        let at = ((held << (5 - bits)) & 31) as usize;
        out.push(char::from(*LETTERS.get(at).unwrap_or(&b'a')));
    }
    out
}

// --- credentials --------------------------------------------------------------

/// Each connection's credential, by its place in the provider plan: the
/// secret setting's text, held apart from the values code reads, since
/// code never reads a credential (area 10, "Secrets"). Beside them, each
/// webhook's secret, by its place in the plan's webhooks, with the value
/// it replaced while a deployment rotates it (chunk 7.7).
#[derive(Clone, Default)]
pub struct Credentials {
    held: Vec<Option<String>>,
    /// Each AWS connection's access key id, beside its secret (chunk 7.7c).
    key_ids: Vec<Option<String>>,
    webhooks: Vec<Option<(String, Option<String>)>>,
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field(
                "held",
                &self.held.iter().filter(|held| held.is_some()).count(),
            )
            .field(
                "webhooks",
                &self.webhooks.iter().filter(|held| held.is_some()).count(),
            )
            .finish()
    }
}

impl Credentials {
    /// The credentials `values` hold for the package connections in
    /// `table`, the plan's connections table, by their numbers there, and
    /// the secrets for `plan`'s webhooks.
    pub fn read(
        table: &[jadpo_std::connections::Connection],
        plan: Option<&ProviderPlan>,
        values: &crate::config::ConfigValues,
    ) -> Credentials {
        let package = |at: usize| {
            jadpo_std::connections::package(table, u32::try_from(at).unwrap_or(u32::MAX))
        };
        Credentials {
            held: (0..table.len())
                .map(|at| {
                    let (_, at) = package(at)?.credential?;
                    values.text_at(at).map(str::to_string)
                })
                .collect(),
            key_ids: (0..table.len())
                .map(|at| match package(at)?.credential? {
                    (CredentialScheme::AwsSigV4 { key_id }, _) => {
                        values.text_at(key_id).map(str::to_string)
                    }
                    _ => None,
                })
                .collect(),
            webhooks: plan
                .map(|plan| plan.webhooks.as_slice())
                .unwrap_or_default()
                .iter()
                .map(|webhook| {
                    let current = values.text_at(webhook.secret)?.to_string();
                    let previous = values.previous_at(webhook.secret).map(str::to_string);
                    Some((current, previous))
                })
                .collect(),
        }
    }

    /// The credential of the connection at `at` in the plan.
    pub fn get(&self, at: u32) -> Option<&str> {
        self.held.get(at as usize)?.as_deref()
    }

    /// The access key id of the AWS connection at `at` in the plan.
    pub fn key_id(&self, at: u32) -> Option<&str> {
        self.key_ids.get(at as usize)?.as_deref()
    }

    /// The secrets the webhook at `at` in the plan is checked with: its
    /// current one, then the one it replaced while that is still given.
    pub fn webhook(&self, at: usize) -> Vec<&str> {
        match self.webhooks.get(at) {
            Some(Some((current, previous))) => std::iter::once(current.as_str())
                .chain(previous.as_deref())
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Credentials given as they are, by the connection's place: for a
    /// test.
    pub fn given(held: Vec<Option<String>>) -> Credentials {
        Credentials {
            held,
            key_ids: Vec::new(),
            webhooks: Vec::new(),
        }
    }

    /// Access key ids given as they are, by the connection's place: for a
    /// test.
    pub fn with_key_ids(mut self, key_ids: Vec<Option<String>>) -> Credentials {
        self.key_ids = key_ids;
        self
    }

    /// Webhook secrets given as they are, by the webhook's place: for a
    /// test.
    pub fn with_webhooks(mut self, secrets: Vec<Option<String>>) -> Credentials {
        self.webhooks = secrets
            .into_iter()
            .map(|secret| secret.map(|secret| (secret, None)))
            .collect();
        self
    }
}

// --- the call -----------------------------------------------------------------

/// What runs a package's own functions at a call's site (area 11,
/// "Decoding"; chunk 7.2c2): a field's `decode:` and `encode:`, and an
/// operation's whole-answer `decode:`, each pure. The executor, which runs
/// every other function: each finds the function by its key through its
/// plan and runs it ([`crate::Executor::function`], chunk 8.1e).
pub trait Functions {
    /// Runs the package's function with identity key `function` on `args`,
    /// from the site numbered `at` in the frame on top of `ctx`: what it
    /// gives, or a description of how it ended instead, its failure's code
    /// or its fault, never a value.
    fn function(
        &self,
        ctx: &mut Ctx<'_>,
        function: &str,
        args: Vec<Value>,
        at: u32,
    ) -> Result<Value, String>;
}

impl<E: crate::Executor + ?Sized> Functions for E {
    fn function(
        &self,
        ctx: &mut Ctx<'_>,
        function: &str,
        args: Vec<Value>,
        at: u32,
    ) -> Result<Value, String> {
        match self.plan().callable(function) {
            Some(callable) => crate::Executor::function(self, ctx, callable, args, at),
            None => Err(format!("`{function}`, which the plan doesn't have")),
        }
    }
}

/// An executor reached through a reference, as the package's functions'
/// runner (chunk 8.1e).
pub(crate) struct Through<'e>(pub(crate) &'e dyn crate::Executor);

impl Functions for Through<'_> {
    fn function(
        &self,
        ctx: &mut Ctx<'_>,
        function: &str,
        args: Vec<Value>,
        at: u32,
    ) -> Result<Value, String> {
        Functions::function(self.0, ctx, function, args, at)
    }
}

/// The site a call is made from: what it reads of the answer, whether it
/// maps `timed_out:`, and what runs the package's functions there.
#[derive(Clone, Copy)]
pub struct Site<'a> {
    pub call: &'a CallSite,
    pub functions: &'a dyn Functions,
    /// The site's number in the frame on top of the context.
    pub at: u32,
}

impl fmt::Debug for Site<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Site")
            .field("call", self.call)
            .field("at", &self.at)
            .finish_non_exhaustive()
    }
}

/// The package's field decoders and encoders run at a call's site, for
/// the codecs on the provider's wire ([`jadpo_value::json::Coder`]),
/// keeping how the first that ended without a value ended.
pub(crate) struct SiteCoder<'c, 'x> {
    pub(crate) ctx: RefCell<&'c mut Ctx<'x>>,
    pub(crate) functions: &'c dyn Functions,
    pub(crate) at: u32,
    pub(crate) ended: RefCell<Option<String>>,
}

impl fmt::Debug for SiteCoder<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SiteCoder")
            .field("at", &self.at)
            .finish_non_exhaustive()
    }
}

impl SiteCoder<'_, '_> {
    pub(crate) fn run(&self, function: &str, value: Value) -> Option<Value> {
        let mut ctx = self.ctx.borrow_mut();
        match self
            .functions
            .function(&mut ctx, function, vec![value], self.at)
        {
            Ok(value) => Some(value),
            Err(how) => {
                self.ended.borrow_mut().get_or_insert(how);
                None
            }
        }
    }
}

impl jadpo_value::json::Coder for SiteCoder<'_, '_> {
    fn decode(&self, function: &str, wire: Value) -> Option<Value> {
        self.run(function, wire)
    }

    fn encode(&self, function: &str, value: Value) -> Option<Value> {
        self.run(function, value)
    }

    fn unwritten(&self, rule: &str) {
        self.ended
            .borrow_mut()
            .get_or_insert_with(|| format!("`{rule}`"));
    }
}

/// What a provider call runs with besides its input: the program's
/// provider plan, the codecs its operations' types are written and read
/// with, and its failures.
#[derive(Clone, Copy)]
pub struct Providers<'a> {
    pub plan: &'a ProviderPlan,
    /// The plan's connections table, whose package connections the
    /// operations are called through (chunk 8.1d).
    pub connections: &'a [jadpo_std::connections::Connection],
    pub json: JsonCodecs<'a>,
    pub failures: &'a [FailurePlan],
    pub credentials: &'a Credentials,
    /// The application's name, which every key is derived from.
    pub application: &'a str,
    /// Each operation's recording's text, by its number, from the plan's
    /// dev and test section (chunk 8.1e): what `jadpo dev`'s fake answers
    /// from.
    #[cfg(feature = "testing")]
    pub recordings: &'a std::collections::BTreeMap<u32, String>,
}

impl<'a> Providers<'a> {
    /// The number of connection `connection` in the plan's connections
    /// table; `u32::MAX` for one it doesn't have.
    pub fn number(&self, connection: &str) -> u32 {
        jadpo_std::connections::named(self.connections, connection).map_or(u32::MAX, |(at, _)| at)
    }

    /// The operation numbered `number`, with how calls through its
    /// connection are made and the connection's name.
    pub fn numbered(
        &self,
        number: u32,
    ) -> Option<(&'a ConnectionPlan, &'a OperationPlan, &'a str)> {
        let (plan, op) = self.plan.numbered(self.connections, number)?;
        let connection = self.connections.get(op.connection as usize)?;
        Some((plan, op, connection.name.as_str()))
    }

    /// Operation `operation` (`bookings.create`) of connection
    /// `connection`, with how calls through the connection are made.
    pub fn operation(
        &self,
        connection: &str,
        operation: &str,
    ) -> Option<(&'a ConnectionPlan, &'a OperationPlan)> {
        self.plan.operation(self.connections, connection, operation)
    }
}

impl fmt::Debug for Providers<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Providers")
            .field("operations", &self.plan.operations.len())
            .finish_non_exhaustive()
    }
}

/// What a provider call gives the site that made it.
#[derive(Debug)]
pub enum Done {
    Value(Value),
    /// The operation's own failure, which the call's `attempt` maps, or
    /// what the runtime raises in its place: `unavailable`,
    /// `misconfigured`, `outcome_unknown`.
    Failed(Box<Failure>),
    Fault(FaultKind),
}

/// The settings a call through `connection` reads, by number: its
/// environment's, or its `host:`'s, whose values [`call`] reads at the
/// call's context.
pub fn settings_read(connection: &ConnectionPlan) -> Vec<u32> {
    match &connection.address {
        Address::Hosts { environment, .. } => vec![*environment],
        Address::Setting(at) => vec![*at],
        Address::Unknown => Vec::new(),
    }
}

/// A provider call's site in a program, as an executor hands it to
/// [`call`] (chunk 7.9e): the plan it is made in and its site, which names
/// the operation it calls by number (chunk 8.1d2).
#[derive(Clone, Copy)]
pub struct Calling<'a> {
    pub plan: crate::Plan<'a>,
    pub site: Site<'a>,
}

impl fmt::Debug for Calling<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Calling")
            .field("site", &self.site)
            .finish_non_exhaustive()
    }
}

/// A call's site that reads the whole answer of the operation numbered
/// `number`: the runtime's own calls' (an email through a mail package, a
/// queue's receive and acknowledgement, a poll) and `jadpo package
/// record`'s.
pub fn whole_site(number: u32) -> CallSite {
    CallSite {
        called: jadpo_std::provider::Called::Operation(number),
        ..CallSite::default()
    }
}

/// Calls the operation `calling.site` names with `input`, from `ctx` at
/// that site: what both executors run at a call's site, and the runtime's
/// own calls too (an email through a mail package, a queue's, a poll's).
/// A package action's own operation goes through the connection the
/// action was called through (`Ctx::package_connection`). The program's
/// connections come from `calling.plan`, their credentials and the values
/// of the settings its address reads from what `ctx` reaches: a test's
/// own, else the ones the program started with (chunk 7.9e).
pub async fn call(ctx: &mut Ctx<'_>, calling: Calling<'_>, input: Option<Value>) -> Done {
    let started = ctx.started();
    let Some(providers) = calling.plan.providers(&started) else {
        return Done::Fault(FaultKind::Defect(
            "a provider called in a program with no provider plan".into(),
        ));
    };
    let through = ctx
        .package_connection()
        .and_then(|connection| jadpo_std::connections::named(providers.connections, connection))
        .map(|(at, _)| at);
    let Some(number) = providers.plan.called(calling.site.call.called, through) else {
        return Done::Fault(FaultKind::Defect(
            "a provider's operation the plan doesn't have".into(),
        ));
    };
    let settings = match providers.numbered(number) {
        Some((plan, _, _)) => ctx.settings_of(settings_read(plan)),
        None => Vec::new(),
    };
    made(ctx, &providers, number, input, &settings, calling.site).await
}

/// [`call`] of the operation numbered `number`, with `providers` and the
/// values `settings` gives the settings [`settings_read`] names.
async fn made(
    ctx: &mut Ctx<'_>,
    providers: &Providers<'_>,
    number: u32,
    input: Option<Value>,
    settings: &[(u32, Value)],
    site: Site<'_>,
) -> Done {
    let Some((plan, op, connection)) = providers.numbered(number) else {
        return Done::Fault(FaultKind::Defect(
            "a provider's operation the plan doesn't have".into(),
        ));
    };
    let operation = op.name.as_str();
    let name = format!("{connection}.{operation}");
    let at = op.connection;
    let made = Call::new(
        connection,
        operation,
        input.clone().unwrap_or(Value::Unit),
        op.repeat,
    );
    let Made {
        mut request,
        fields,
        keying,
    } = match request(
        ctx,
        providers,
        (connection, plan, op, at),
        input.as_ref(),
        settings,
        site,
        None,
    ) {
        Ok(made) => made,
        Err(Unready::Misconfigured) => {
            let now = ctx.now();
            ctx.capabilities_mut().refused(&made, now);
            return Done::Failed(Box::new(crate::misconfigured()));
        }
        Err(Unready::Fault(kind)) => return Done::Fault(kind),
    };
    // A step of an action with several changing calls (area 12; chunk
    // 7.5b): given its recorded answer back, or sent at its place.
    let did_nothing = did_nothing(providers, op);
    let comes = crate::outside::comes_from(
        ctx,
        crate::outside::Asking {
            step: Some(crate::steps::Proposed {
                operation: &name,
                fields,
                repeat: op.repeat,
                changes: op.changes,
                window_ms: plan
                    .idempotency
                    .as_ref()
                    .and_then(|idempotency| idempotency.window_ms),
                did_nothing: &did_nothing,
            }),
            rerun: None,
        },
    )
    .await;
    let step = match comes {
        Ok(crate::outside::Comes::Send(place)) => place.step,
        Ok(crate::outside::Comes::Recorded { answered, at }) => {
            // Logged as the step it is (chunk 7.5f).
            let mut made = made;
            made.step = at;
            return replayed(ctx, providers, op, &name, made, answered, site);
        }
        Ok(crate::outside::Comes::Reused(_) | crate::outside::Comes::Opened(_)) => {
            return Done::Fault(FaultKind::Defect(
                "a provider's call given back a model's answer".into(),
            ));
        }
        Err(Unanswered::Failed(code)) => return failed(providers, op, &name, &code),
        Err(Unanswered::Failure(failure)) => return Done::Failed(failure),
        Err(Unanswered::Fault(kind)) => return Done::Fault(kind),
    };
    // The key it goes under, which a fake keeps its answer under as the
    // provider would (chunk 7.5d).
    let mut sent_key = None;
    if let Some(keying) = &keying
        && let Some(header) = keying.header(providers, plan, step.map_or(0, |at| at.key_generation))
    {
        sent_key = Some(crate::capability::SentKey {
            key: header.1.clone(),
            reused: plan
                .idempotency
                .as_ref()
                .and_then(|idempotency| idempotency.reused.clone()),
            did_nothing: did_nothing.clone(),
        });
        request.headers.push(header);
    }
    // What the development call log shows of the request (chunk 7.2d).
    let shown = ctx.host().call_log().map(|_| {
        (
            request.method.word(),
            request.url.clone(),
            request.body.clone(),
        )
    });
    let mut made = made.with_request(request);
    made.step = step;
    made.key = sent_key;
    let mut step = step;
    let sending = crate::outside::Sending::provider(plan, ctx.host());
    let mut answer = match crate::outside::send(ctx, &made, sending).await {
        Ok(sent) => sent.whole(),
        Err(Unanswered::Failed(code)) => return failed(providers, op, &name, &code),
        Err(Unanswered::Failure(failure)) => return Done::Failed(failure),
        Err(Unanswered::Fault(kind)) => return Done::Fault(kind),
    };
    // The provider's "use a new key" (its package's `new_key_on:`): the
    // key can't be used again, so the call goes again, once, under the
    // next key generation, which a step's record keeps first so a rerun
    // sends under it too (area 11, "Keys and windows"; chunk 7.5c).
    let new_key_on = plan
        .idempotency
        .as_ref()
        .and_then(|idempotency| idempotency.new_key_on.as_deref());
    if let (Answer::Failed(code), Some(keying), Some(marker)) = (&answer, &keying, new_key_on)
        && code == marker
    {
        let next = match step {
            Some(at) => match crate::steps::new_key(ctx, at).await {
                Ok(next) => Some(next),
                Err(Unanswered::Failed(code)) => return failed(providers, op, &name, &code),
                Err(Unanswered::Failure(failure)) => return Done::Failed(failure),
                Err(Unanswered::Fault(kind)) => return Done::Fault(kind),
            },
            None => None,
        };
        let generation = next.map_or(1, |at| at.key_generation);
        if let Some((header, key)) = keying.header(providers, plan, generation)
            && let Some(request) = made.request.as_mut()
        {
            request
                .headers
                .retain(|(name, _)| !name.eq_ignore_ascii_case(&header));
            if let Some(sent) = made.key.as_mut() {
                sent.key.clone_from(&key);
            }
            request.headers.push((header, key));
        }
        step = next;
        made.step = next;
        let sending = crate::outside::Sending::provider(plan, ctx.host());
        answer = match crate::outside::send(ctx, &made, sending).await {
            Ok(sent) => sent.whole(),
            Err(Unanswered::Failed(code)) => return failed(providers, op, &name, &code),
            Err(Unanswered::Failure(failure)) => return Done::Failed(failure),
            Err(Unanswered::Fault(kind)) => return Done::Fault(kind),
        };
    }
    // A fake's generated default or recording, made as the provider
    // would send it, is an answer as sent from here: a step keeps it as it
    // keeps any other.
    let answer = match answer {
        Answer::Generated => Answer::Wire(crate::fakes::default_answer(ctx, providers, op)),
        Answer::Recorded => Answer::Wire(recorded_answer(ctx, providers, op, number)),
        other => other,
    };
    if let Some(at) = step
        && let Err(unanswered) = keep(ctx, plan, op, at, &answer, &did_nothing).await
    {
        return match unanswered {
            Unanswered::Failed(code) => failed(providers, op, &name, &code),
            Unanswered::Failure(failure) => Done::Failed(failure),
            Unanswered::Fault(kind) => Done::Fault(kind),
        };
    }
    let mut sent: Option<Rc<str>> = None;
    // No answer in time from a call safe to send again is the site's
    // `timed_out`, which its `attempt` maps (chunk 7.2c2).
    let done = if answer == Answer::TimedOut && site.call.timed_out && op.repeat != Repeat::Unkeyed
    {
        Done::Failed(Box::new(timed_out()))
    } else {
        match crate::effects::answered(op.repeat, answer) {
            Ok(Given::Value(value)) => Done::Value(value),
            Ok(Given::Wire(text)) => {
                let done = decode(ctx, providers, op, &name, &text, site);
                sent = Some(text);
                done
            }
            // A package's fake answering with the operation's generated
            // default, made as the provider would send it and read back as
            // any answer is (area 11, "Tests and fakes"; chunk 7.2d).
            Ok(Given::Generated) => {
                let text = crate::fakes::default_answer(ctx, providers, op);
                let done = decode(ctx, providers, op, &name, &text, site);
                sent = Some(text);
                done
            }
            // A package's fake answering from the operation's recording,
            // its placeholders filled with generated values of their types
            // (area 11, "Provider calls in development"; chunk 7.2d2).
            Ok(Given::Recorded) => {
                let text = recorded_answer(ctx, providers, op, number);
                let done = decode(ctx, providers, op, &name, &text, site);
                sent = Some(text);
                done
            }
            Err(Unanswered::Failed(code)) => failed(providers, op, &name, &code),
            Err(Unanswered::Failure(failure)) => Done::Failed(failure),
            Err(Unanswered::Fault(kind)) => Done::Fault(kind),
        }
    };
    if let (Some((method, url, body)), Some(log)) = (shown, ctx.host().call_log())
        && let Some(logged) = ctx.capabilities().calls().last()
    {
        log.seen(&crate::call_log::Seen {
            connection,
            operation,
            request_id: ctx.request_id(),
            at: logged.at,
            source: logged.source,
            took_ms: logged.took_ms,
            method,
            url: &url,
            body: body.as_deref(),
            answer: sent.as_deref(),
            ended: &logged.ended,
            values: None,
        });
    }
    done
}

/// The codes of `op`'s own failures whose kind says it did nothing,
/// `unavailable` or `rate_limited`: one answered so is sent again under its
/// key, never kept as its answer (area 12, "A step in doubt"; chunk 7.9a).
fn did_nothing(providers: &Providers<'_>, op: &OperationPlan) -> Vec<String> {
    op.failures
        .iter()
        .filter_map(|key| providers.failures.iter().find(|plan| plan.key == *key))
        .filter(|plan| matches!(plan.kind.as_deref(), Some("unavailable" | "rate_limited")))
        .map(|plan| plan.code.clone())
        .collect()
}

/// Keeps what step `at`, a call of `op` through `plan`, was answered
/// (chunk 7.5b): an answer as sent, or one of its own failures, which a
/// rerun gives back; nothing for an answer that may not be the last, or a
/// failure whose code is among `did_nothing` (chunk 7.9a), so the step
/// stays in doubt and is sent again under its key. A provider that says
/// the key came with a different request stops the action as `diverged`;
/// one that replays a failed answer under its key parks a server's error
/// as `outcome_unknown`, since sending it again only repeats it.
async fn keep(
    ctx: &mut Ctx<'_>,
    plan: &ConnectionPlan,
    op: &OperationPlan,
    at: crate::steps::StepAt,
    answer: &Answer,
    did_nothing: &[String],
) -> Result<(), Unanswered> {
    use crate::steps::Answered;
    let idempotency = plan.idempotency.as_ref();
    match answer {
        Answer::Wire(text) => crate::steps::answered(ctx, at, Answered::Wire(text.clone())).await,
        Answer::Failed(code)
            if idempotency.and_then(|idempotency| idempotency.reused.as_deref())
                == Some(code.as_str()) =>
        {
            Err(crate::steps::reused(ctx, at).await)
        }
        Answer::Failed(code) if did_nothing.contains(code) => Ok(()),
        Answer::Failed(code) => {
            crate::steps::answered(ctx, at, Answered::Failed(code.clone())).await
        }
        Answer::Unknown
            if op.repeat == Repeat::Keyed
                && idempotency.is_some_and(|idempotency| idempotency.replays_failures) =>
        {
            Err(Unanswered::Failure(Box::new(crate::outcome_unknown())))
        }
        _ => Ok(()),
    }
}

/// What step `answered`, a call of `op` its record answered before, gives
/// its site without being sent (chunk 7.5b): logged as replayed, then
/// decoded with the current package as any answer is, or raised as the
/// failure it was.
fn replayed(
    ctx: &mut Ctx<'_>,
    providers: &Providers<'_>,
    op: &OperationPlan,
    name: &str,
    made: Call,
    answered: crate::steps::Answered,
    site: Site<'_>,
) -> Done {
    use crate::steps::Answered;
    let answer = match &answered {
        Answered::Wire(text) => Answer::Wire(text.clone()),
        Answered::Failed(code) => Answer::Failed(code.clone()),
        Answered::Done => Answer::Value(Value::Unit),
    };
    crate::outside::given_back(ctx, &made, &answer);
    match answered {
        Answered::Wire(text) => decode(ctx, providers, op, name, &text, site),
        Answered::Failed(code) => failed(providers, op, name, &code),
        Answered::Done => Done::Value(Value::Unit),
    }
}

/// Operation `op`'s recording with its placeholders filled, as its
/// provider would send it; its generated default when it keeps none, or
/// one that isn't JSON.
fn recorded_answer(
    ctx: &mut Ctx<'_>,
    providers: &Providers<'_>,
    op: &OperationPlan,
    number: u32,
) -> Rc<str> {
    let now = ctx.now();
    let filled = providers.recordings.get(&number).and_then(|text| {
        let mut uuid = || crate::fakes::uuid_text(ctx.new_id().unwrap_or_default());
        let mut making = crate::fakes::Making {
            now,
            uuid: &mut uuid,
        };
        crate::recorded::filled(&providers.json, op, text, &mut making)
    });
    match filled {
        Some(text) => Rc::from(text),
        None => crate::fakes::default_answer(ctx, providers, op),
    }
}

/// What the runtime raises when a call safe to send again gets no answer
/// within its timeout, for its site to map (area 4: `timed_out`, 504, "a
/// dependency or deadline ran out of time"; chunk 7.2c2).
pub fn timed_out() -> Failure {
    let mut failure = Failure::new("runtime timed_out", "TimedOut", "timed_out");
    failure.kind = Some("timed_out".to_string());
    failure.status = Some(504);
    failure
}

/// Why a request couldn't be made.
pub(crate) enum Unready {
    /// The connection's address or credential isn't usable.
    Misconfigured,
    Fault(FaultKind),
}

/// `value` written at `place`, dotted, in `members`, each object on the
/// way made where it isn't there.
fn add_at(
    members: &mut serde_json::Map<String, serde_json::Value>,
    place: &str,
    value: serde_json::Value,
) {
    let mut steps: Vec<&str> = place.split('.').collect();
    let Some(last) = steps.pop() else {
        return;
    };
    let mut at = members;
    for step in steps {
        let next = at
            .entry(step.to_string())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        if !next.is_object() {
            *next = serde_json::Value::Object(serde_json::Map::new());
        }
        let serde_json::Value::Object(inner) = next else {
            return;
        };
        at = inner;
    }
    at.insert(last.to_string(), value);
}

/// The request the operation numbered `number` sends with `input`,
/// `added`'s value written at its place beside it (area 21,
/// "How a provider fills it"; chunk 5.5v3b): what a package filling `ai`
/// is sent, through the capability seam by the `ai` call's own site.
pub(crate) fn request_adding(
    ctx: &mut Ctx<'_>,
    providers: &Providers<'_>,
    number: u32,
    input: Option<&Value>,
    settings: &[(u32, Value)],
    site: Site<'_>,
    added: Option<(&str, serde_json::Value)>,
) -> Result<Request, Unready> {
    let Some((plan, op, connection)) = providers.numbered(number) else {
        return Err(Unready::Fault(FaultKind::Defect(
            "a provider's operation the plan doesn't have".into(),
        )));
    };
    let at = op.connection;
    request(
        ctx,
        providers,
        (connection, plan, op, at),
        input,
        settings,
        site,
        added,
    )
    .map(|made| made.request)
}

/// The request `op` sends with `input`, its fields written as the
/// provider takes them, each field's own encoder run at `site`.
fn request(
    ctx: &mut Ctx<'_>,
    providers: &Providers<'_>,
    (name, plan, op, at): (&str, &ConnectionPlan, &OperationPlan, u32),
    input: Option<&Value>,
    settings: &[(u32, Value)],
    site: Site<'_>,
    added: Option<(&str, serde_json::Value)>,
) -> Result<Made, Unready> {
    let setting = |number: u32| {
        settings
            .iter()
            .find(|(at, _)| *at == number)
            .map(|(_, value)| value)
    };
    let (base, environment) = match &plan.address {
        Address::Hosts {
            live,
            sandbox,
            environment,
        } => {
            let chosen = match setting(*environment) {
                Some(Value::Variant(variant)) => variant.name.to_string(),
                _ => return Err(Unready::Misconfigured),
            };
            let host = match chosen.as_str() {
                "live" => live.clone(),
                "sandbox" => sandbox.clone(),
                _ => None,
            };
            (host.ok_or(Unready::Misconfigured)?, chosen)
        }
        Address::Setting(number) => match setting(*number) {
            Some(Value::Text(text)) => (text.to_string(), text.to_string()),
            _ => return Err(Unready::Misconfigured),
        },
        Address::Unknown => return Err(Unready::Misconfigured),
    };
    let base = sendable(&base).ok_or(Unready::Misconfigured)?;
    // The input as the package's JSON, its members by their spellings on
    // the wire, each number as written so no place is lost (chunk 7.2c2).
    let mut numbers = jadpo_value::json::Numbers::default();
    let mut members = match (op.input, input) {
        (Some(number), Some(value)) => {
            let Some(codec) = providers.json.codec_of(number) else {
                return Err(Unready::Fault(FaultKind::Defect(
                    "a provider's input the plan gave no codec".into(),
                )));
            };
            let coder = SiteCoder {
                ctx: RefCell::new(&mut *ctx),
                functions: site.functions,
                at: site.at,
                ended: RefCell::new(None),
            };
            let codecs = jadpo_value::json::Codecs {
                coder: Some(&coder),
                ..providers.json.codecs
            };
            let written = codecs.write_value(codec, value);
            if let Some(how) = coder.ended.into_inner() {
                return Err(Unready::Fault(FaultKind::Provider(format!(
                    "`{}.{}`'s input doesn't encode: {how}",
                    name, op.name
                ))));
            }
            match jadpo_value::json::parse(written.as_bytes()) {
                Some((serde_json::Value::Object(members), written)) => {
                    numbers = written;
                    members
                }
                _ => {
                    return Err(Unready::Fault(FaultKind::Defect(
                        "a provider's input that isn't an object".into(),
                    )));
                }
            }
        }
        _ => serde_json::Map::new(),
    };
    // What the call's site writes beside its input, at its place: a
    // model's structured-output setting (area 21; chunk 5.5v3b).
    if let Some((place, value)) = added {
        add_at(&mut members, place, value);
    }
    // Each top-level field as written, for the fingerprint a step of an
    // action with several changing calls keeps (chunk 7.5b).
    let fields: Vec<(String, Vec<u8>)> = members
        .iter()
        .map(|(name, value)| {
            let text = json_text(value, &numbers, &mut vec![name.clone()]);
            (name.clone(), text.into_bytes())
        })
        .collect();
    let mut url = base.trim_end_matches('/').to_string();
    for part in &op.path {
        match part {
            PathPart::Fixed(text) => url.push_str(text),
            PathPart::Field(name) => match members.remove(name) {
                Some(value) => match plain(&value, &numbers, name) {
                    Some(text) => url.push_str(&jadpo_value::encoding::to_url_component(&text)),
                    None => {
                        return Err(Unready::Fault(FaultKind::Defect(format!(
                            "`{}`'s placeholder `{{{name}}}` given a value that isn't text or a number",
                            op.name
                        ))));
                    }
                },
                None => {
                    return Err(Unready::Fault(FaultKind::Defect(format!(
                        "`{}`'s placeholder `{{{name}}}` given no value",
                        op.name
                    ))));
                }
            },
        }
    }
    let body = if op.method.reads_query() {
        let mut query = Vec::new();
        for (member, value) in &members {
            let name = jadpo_value::encoding::to_url_component(member);
            let mut at = vec![member.clone()];
            match value {
                serde_json::Value::Null => {}
                serde_json::Value::Array(items) => {
                    for (place, item) in items.iter().enumerate() {
                        at.push(place.to_string());
                        let text = plain_at(item, &numbers, &at)
                            .unwrap_or_else(|| json_text(item, &numbers, &mut at));
                        at.pop();
                        query.push(format!(
                            "{name}={}",
                            jadpo_value::encoding::to_url_component(&text)
                        ));
                    }
                }
                other => {
                    let text = plain_at(other, &numbers, &at)
                        .unwrap_or_else(|| json_text(other, &numbers, &mut at));
                    query.push(format!(
                        "{name}={}",
                        jadpo_value::encoding::to_url_component(&text)
                    ));
                }
            }
        }
        if !query.is_empty() {
            url.push('?');
            url.push_str(&query.join("&"));
        }
        None
    } else {
        Some(json_text(
            &serde_json::Value::Object(members),
            &numbers,
            &mut Vec::new(),
        ))
    };
    let aws_json = plan.protocol == Protocol::AwsJson;
    let mut headers = vec![
        ("Accept".to_string(), "application/json".to_string()),
        ("user-agent".to_string(), "jadpo".to_string()),
    ];
    // The package's own fixed headers (chunk 5.5v7), as its API's version.
    headers.extend(plan.headers.iter().cloned());
    if aws_json {
        // AWS's JSON protocol names the operation in a header, its body
        // always sent (chunk 7.7c).
        let target = plan
            .aws
            .as_ref()
            .and_then(|aws| aws.target.as_deref())
            .unwrap_or_default();
        let operation = op.name.rsplit('.').next().unwrap_or(&op.name);
        headers.push((
            "Content-Type".to_string(),
            "application/x-amz-json-1.0".to_string(),
        ));
        headers.push((
            "X-Amz-Target".to_string(),
            format!("{target}.{}", pascal(operation)),
        ));
    } else if body.is_some() {
        headers.push(("Content-Type".to_string(), "application/json".to_string()));
    }
    let body = if aws_json && body.is_none() {
        Some("{}".to_string())
    } else {
        body
    };
    if let Some((scheme, _)) = plan.credential {
        let Some(secret) = providers.credentials.get(at) else {
            return Err(Unready::Misconfigured);
        };
        match scheme {
            CredentialScheme::Bearer => {
                headers.push(("Authorization".to_string(), format!("Bearer {secret}")));
            }
            CredentialScheme::Header => {
                let Some(name) = plan.credential_header.as_ref() else {
                    return Err(Unready::Misconfigured);
                };
                headers.push((name.clone(), secret.to_string()));
            }
            CredentialScheme::Basic => headers.push((
                "Authorization".to_string(),
                format!(
                    "Basic {}",
                    jadpo_value::encoding::to_base64(secret.as_bytes())
                ),
            )),
            CredentialScheme::AwsSigV4 { .. } => {
                let (Some(key_id), Some(aws)) = (providers.credentials.key_id(at), &plan.aws)
                else {
                    return Err(Unready::Misconfigured);
                };
                let region = crate::sigv4::region_of(&base);
                let signer = crate::sigv4::Signer {
                    key_id,
                    secret,
                    region: &region,
                    service: &aws.service,
                };
                // Signed with every header but its client's name, which
                // something on the way may change.
                let signed: Vec<(String, String)> = headers
                    .iter()
                    .filter(|(name, _)| !name.eq_ignore_ascii_case("user-agent"))
                    .cloned()
                    .collect();
                let unsigned = crate::sigv4::Unsigned {
                    method: op.method.word(),
                    url: &url,
                    headers: &signed,
                    body: body.as_deref().unwrap_or_default().as_bytes(),
                };
                // Signed at the host's time, which AWS checks, never a
                // record's fixed `clock.now` (chunk 7.9a).
                let now = crate::steps::wall(ctx);
                let Some(added) = crate::sigv4::sign(&unsigned, &signer, now) else {
                    return Err(Unready::Misconfigured);
                };
                headers.extend(added);
            }
        }
    }
    let mut reading = Reading {
        failed_when: op.failed_when.clone(),
        error_code: plan.error_code.clone(),
        in_use: None,
        reused: None,
        aws: aws_json,
    };
    let mut keying = None;
    if let Some(idempotency) = &plan.idempotency {
        reading.in_use = idempotency.in_use.clone();
        reading.reused = idempotency.reused.clone();
        if op.repeat == Repeat::Keyed && !idempotency.header.is_empty() {
            let (Some(step), Some(call_key)) = (ctx.next_step(), ctx.call_key()) else {
                return Err(Unready::Fault(FaultKind::Defect(
                    "a keyed provider call outside an action, or with no randomness for its key"
                        .into(),
                )));
            };
            keying = Some(Keying {
                connection: name.to_string(),
                environment,
                scope: call_key.scope,
                action: call_key.key,
                step,
            });
        }
    }
    Ok(Made {
        request: Request {
            method: op.method,
            url,
            headers,
            body,
            timeout_ms: plan.timeout_ms,
            reading,
        },
        fields,
        keying,
    })
}

/// `receive_message` as AWS names an operation, `ReceiveMessage`.
fn pascal(name: &str) -> String {
    name.split('_')
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_ascii_uppercase().to_string() + chars.as_str()
            })
        })
        .collect()
}

/// A request as made, before its key: what its fingerprint is made of and
/// what its key is derived from.
struct Made {
    request: Request,
    fields: Vec<(String, Vec<u8>)>,
    keying: Option<Keying>,
}

/// What a keyed call's key is derived from besides the application, the
/// connection and the generation ([`key`]).
struct Keying {
    /// The connection's name, which the key is derived from.
    connection: String,
    environment: String,
    scope: String,
    action: String,
    step: u32,
}

impl Keying {
    /// The header the key goes in and the key, at key generation
    /// `generation`.
    fn header(
        &self,
        providers: &Providers<'_>,
        plan: &ConnectionPlan,
        generation: u32,
    ) -> Option<(String, String)> {
        let idempotency = plan.idempotency.as_ref()?;
        let derived = key(
            &KeyParts {
                application: providers.application,
                environment: &self.environment,
                connection: &self.connection,
                scope: &self.scope,
                action: &self.action,
                step: self.step,
                generation,
            },
            idempotency.max_length,
        );
        Some((idempotency.header.clone(), derived))
    }
}

/// The input's member `name` as text in a path: a string as it is, a
/// number as written, a flag as JSON writes it.
fn plain(
    value: &serde_json::Value,
    numbers: &jadpo_value::json::Numbers,
    name: &str,
) -> Option<String> {
    plain_at(value, numbers, &[name.to_string()])
}

/// A JSON value at `at` as text in a path or a query: a string as it is, a
/// number as the input wrote it, a flag as JSON writes it.
fn plain_at(
    value: &serde_json::Value,
    numbers: &jadpo_value::json::Numbers,
    at: &[String],
) -> Option<String> {
    match value {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Number(number) => Some(
            numbers
                .get(at)
                .map_or_else(|| number.to_string(), str::to_string),
        ),
        serde_json::Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

/// `value`, standing at `at` in the input, as compact JSON, each number as
/// the input wrote it (`2.50`, never `2.5`), members in the order held.
fn json_text(
    value: &serde_json::Value,
    numbers: &jadpo_value::json::Numbers,
    at: &mut Vec<String>,
) -> String {
    let mut out = String::new();
    write_json(value, numbers, at, &mut out);
    out
}

fn write_json(
    value: &serde_json::Value,
    numbers: &jadpo_value::json::Numbers,
    at: &mut Vec<String>,
    out: &mut String,
) {
    match value {
        serde_json::Value::Number(number) => match numbers.get(at) {
            Some(written) => out.push_str(written),
            None => out.push_str(&number.to_string()),
        },
        serde_json::Value::Array(items) => {
            out.push('[');
            for (place, item) in items.iter().enumerate() {
                if place > 0 {
                    out.push(',');
                }
                at.push(place.to_string());
                write_json(item, numbers, at, out);
                at.pop();
            }
            out.push(']');
        }
        serde_json::Value::Object(members) => {
            out.push('{');
            for (place, (name, member)) in members.iter().enumerate() {
                if place > 0 {
                    out.push(',');
                }
                jadpo_value::json::write_string(name, out);
                out.push(':');
                at.push(name.clone());
                write_json(member, numbers, at, out);
                at.pop();
            }
            out.push('}');
        }
        other => out.push_str(&other.to_string()),
    }
}

/// `address` when a call may be sent to it (area 11, question 9): HTTPS,
/// or plain HTTP to this machine, which is how a test reaches its server;
/// a host and an optional port and path, with no credentials, query or
/// fragment of its own.
pub fn sendable(address: &str) -> Option<String> {
    jadpo_std::provider::sendable(address)
}

/// The value `text`, an answer as sent, holds as `op`'s output, read at
/// `site`: only the fields it reads, or, for an operation with
/// `wire_output:`, the whole answer as sent, given to its `decode:`.
pub(crate) fn decode(
    ctx: &mut Ctx<'_>,
    providers: &Providers<'_>,
    op: &OperationPlan,
    name: &str,
    text: &str,
    site: Site<'_>,
) -> Done {
    let Some(number) = op.output else {
        return Done::Value(Value::Unit);
    };
    let unreadable =
        |why: String| Done::Fault(FaultKind::Provider(format!("`{name}`'s answer {why}")));
    // Each number as the provider wrote it, so a decimal sent as a number
    // loses no place (chunk 7.2c2).
    let Some((json, numbers)) = jadpo_value::json::parse(text.as_bytes()) else {
        return unreadable("isn't JSON".to_string());
    };
    let (at, numbers) = match &op.output_at {
        Some(path) => match at_path(&json, path) {
            Some(at) => {
                let prefix: Vec<String> = path.split('.').map(str::to_string).collect();
                (at, numbers.within(&prefix))
            }
            None => return unreadable(format!("has nothing at `{path}`")),
        },
        None => (&json, numbers),
    };
    // The answer as sent, read as `wire_output:`, then given whole to the
    // package's `decode:` (chunk 7.2c2).
    let read_as = op.decode.as_ref().map_or(number, |(wire, _)| *wire);
    let Some(codec) = providers.json.codec_of(read_as) else {
        return Done::Fault(FaultKind::Defect(
            "a provider's output the plan gave no codec".into(),
        ));
    };
    let mut problems = Vec::new();
    let coder = SiteCoder {
        ctx: RefCell::new(&mut *ctx),
        functions: site.functions,
        at: site.at,
        ended: RefCell::new(None),
    };
    let codecs = jadpo_value::json::Codecs {
        coder: Some(&coder),
        ..providers.json.codecs
    };
    let path = jadpo_value::json::Path::default().numbered(&numbers);
    let read = match (&site.call.reads, &op.decode) {
        (Some(reads), None) => codecs.read_fields(codec, at, &path, &mut problems, reads),
        _ => codecs.read(codec, at, &path, &mut problems),
    };
    let value = match (read, problems.first()) {
        (Some(value), None) => value,
        (_, Some(problem)) if problem.field.is_empty() => {
            return unreadable(format!("doesn't decode: `{}`", problem.rule));
        }
        (_, Some(problem)) => {
            return unreadable(format!(
                "doesn't decode: `{}` on `{}`",
                problem.rule, problem.field
            ));
        }
        (None, None) => return unreadable("doesn't decode".to_string()),
    };
    let Some((_, function)) = &op.decode else {
        return Done::Value(value);
    };
    match coder.run(function, value) {
        Some(value) => Done::Value(value),
        None => unreadable(format!(
            "doesn't decode: {}",
            coder.ended.into_inner().unwrap_or_default()
        )),
    }
}

/// The failure a provider's refusal with `code` is: one of `op`'s, which
/// the call's `attempt` maps; a fault for any other.
fn failed(providers: &Providers<'_>, op: &OperationPlan, name: &str, code: &str) -> Done {
    let found = op.failures.iter().find_map(|key| {
        providers
            .failures
            .iter()
            .find(|plan| plan.key == *key && plan.code == code)
    });
    match found {
        Some(plan) => {
            let mut failure =
                Failure::new(plan.key.as_str(), plan.name.as_str(), plan.code.as_str());
            failure.kind = plan.kind.clone();
            failure.status = plan.status;
            failure.message = plan.message.clone();
            Done::Failed(Box::new(failure))
        }
        None if code.is_empty() => Done::Fault(FaultKind::Provider(format!(
            "`{name}` was refused without an error code"
        ))),
        None => Done::Fault(FaultKind::Provider(format!(
            "`{name}` was refused with `{code}`, which it doesn't declare in `failures:`"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(scope: &str, action: &str, step: u32) -> KeyParts<'static> {
        KeyParts {
            application: "shop",
            environment: "sandbox",
            connection: "couriers",
            scope: Box::leak(scope.to_string().into_boxed_str()),
            action: Box::leak(action.to_string().into_boxed_str()),
            step,
            generation: 0,
        }
    }

    /// A derived key is `jd1_` and 32 base32 characters, the same for the
    /// same parts and different for any other: two callers who send one
    /// `Idempotency-Key`, or two steps of one action, never share a key.
    #[test]
    fn a_key_is_derived_from_every_part_and_cut_to_the_providers_length() {
        let first = key(&parts("user:ada", "k1", 1), None);
        assert_eq!(first.len(), 36, "{first}");
        assert!(first.starts_with("jd1_"), "{first}");
        assert!(
            first[4..]
                .bytes()
                .all(|b| b.is_ascii_lowercase() || (b'2'..=b'7').contains(&b)),
            "{first}"
        );
        assert_eq!(first, key(&parts("user:ada", "k1", 1), None));
        assert_ne!(first, key(&parts("user:bob", "k1", 1), None));
        assert_ne!(first, key(&parts("user:ada", "k1", 2), None));
        assert_ne!(first, key(&parts("user:ada", "k2", 1), None));
        // Parts never run together: `ab` + `c` isn't `a` + `bc`.
        assert_ne!(
            key(&parts("ab", "c", 1), None),
            key(&parts("a", "bc", 1), None)
        );
        assert_eq!(key(&parts("user:ada", "k1", 1), Some(20)).len(), 20);
        assert_eq!(base32(b"f"), "my");
        assert_eq!(base32(b"foobar"), "mzxw6ytboi");
    }

    fn request(reading: Reading) -> Request {
        Request {
            method: HttpMethod::Post,
            url: "https://api.example/v1/bookings?q=secret".into(),
            headers: vec![("Authorization".into(), "Bearer sk_live".into())],
            body: Some("{\"postcode\":\"N1\"}".into()),
            timeout_ms: 1000,
            reading,
        }
    }

    fn answer(status: u16, body: &str) -> Response {
        Response {
            status,
            headers: Vec::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    /// The driver reads a provider's answer: a success as sent, unless
    /// `failed_when:` marks it a failure, whose code `error_code:` gives; a
    /// refusal's code; a rejected credential; a refusal that did nothing;
    /// and a server's error, which may have acted.
    #[test]
    fn a_providers_answer_is_classified_by_its_status_and_its_reading() {
        let reading = Reading {
            failed_when: Some(("ok".into(), "false".into())),
            error_code: Some("error.code".into()),
            in_use: Some("key_in_use".into()),
            reused: Some("key_reused".into()),
            aws: false,
        };
        let sent = request(reading);
        assert_eq!(
            classify(&sent, &answer(200, r#"{"ok": true, "data": {"id": "b1"}}"#)),
            Answer::Wire(Rc::from(r#"{"ok": true, "data": {"id": "b1"}}"#))
        );
        assert_eq!(
            classify(
                &sent,
                &answer(200, r#"{"ok": false, "error": {"code": "slot_taken"}}"#)
            ),
            Answer::Failed("slot_taken".into())
        );
        assert_eq!(
            classify(&sent, &answer(409, r#"{"error": {"code": "slot_taken"}}"#)),
            Answer::Failed("slot_taken".into())
        );
        assert_eq!(
            classify(&sent, &answer(409, r#"{"error": {"code": "key_in_use"}}"#)),
            Answer::Unavailable
        );
        assert_eq!(
            classify(&sent, &answer(400, "not json")),
            Answer::Failed(String::new())
        );
        assert_eq!(classify(&sent, &answer(401, "")), Answer::Misconfigured);
        assert_eq!(classify(&sent, &answer(302, "")), Answer::Misconfigured);
        assert_eq!(classify(&sent, &answer(429, "")), Answer::Unavailable);
        assert_eq!(classify(&sent, &answer(502, "")), Answer::Unknown);
        assert_eq!(
            classify(&sent, &answer(204, "")),
            Answer::Wire(Rc::from("null"))
        );
        // Its debug output never shows its query, headers or body.
        let shown = format!("{sent:?}");
        assert!(
            !shown.contains("secret") && !shown.contains("sk_live"),
            "{shown}"
        );
    }

    /// A retry waits twice as long each time, drawn up to that ceiling,
    /// or as long as the provider's `Retry-After` asks, and never past
    /// the end of its `within:` (chunk 7.2c2).
    #[test]
    fn a_retry_waits_longer_each_time_and_never_past_its_window() {
        assert_eq!(retry_wait(1, None, 30_000, None), Some(200));
        assert_eq!(retry_wait(2, None, 30_000, None), Some(400));
        assert_eq!(retry_wait(3, None, 30_000, None), Some(800));
        let drawn = retry_wait(3, None, 30_000, Some([0xff; 8])).expect("a wait");
        assert!(drawn <= 800, "{drawn}");
        assert_eq!(retry_wait(1, Some(5_000), 30_000, None), Some(5_000));
        assert_eq!(retry_wait(1, Some(40_000), 30_000, None), None);
        assert_eq!(retry_wait(5, None, 1_000, None), None);
        let mut refused = answer(429, "");
        refused.headers = vec![("Retry-After".into(), "2".into())];
        assert_eq!(
            classify(&request(Reading::default()), &refused),
            Answer::RetryAfter(2_000)
        );
    }

    /// A call goes to HTTPS, or to plain HTTP only on this machine.
    #[test]
    fn a_call_goes_to_https_or_to_this_machine() {
        assert!(sendable("https://api.example").is_some());
        assert!(sendable("https://api.example:8443/v2").is_some());
        assert!(sendable("http://127.0.0.1:4000").is_some());
        assert!(sendable("http://localhost:4000/").is_some());
        assert!(sendable("http://[::1]:4000").is_some());
        assert!(sendable("http://api.example").is_none());
        assert!(sendable("http://127.example.com").is_none());
        assert!(sendable("https://user:pass@api.example").is_none());
        assert!(sendable("https://api.example/?debug=1").is_none());
        assert!(sendable("ftp://api.example").is_none());
    }

    #[test]
    fn a_path_reads_members_and_items() {
        let json: serde_json::Value =
            serde_json::from_str(r#"{"data": {"items": [{"id": "a"}]}}"#).unwrap();
        assert_eq!(
            at_path(&json, "data.items.0.id"),
            Some(&serde_json::Value::String("a".into()))
        );
        assert_eq!(at_path(&json, "data.missing"), None);
    }

    /// AWS's error types are read as the codes of the failures they name.
    #[test]
    fn an_aws_error_type_is_read_as_its_failures_code() {
        assert_eq!(
            aws_code("com.amazonaws.sqs#QueueDoesNotExist"),
            "queue_does_not_exist"
        );
        assert_eq!(
            aws_code("ReceiptHandleIsInvalid"),
            "receipt_handle_is_invalid"
        );
        assert_eq!(pascal("receive_message"), "ReceiveMessage");
        let reading = Reading {
            error_code: Some("__type".into()),
            aws: true,
            ..Reading::default()
        };
        assert_eq!(
            classify(
                &request(reading),
                &answer(
                    400,
                    r#"{"__type": "com.amazonaws.sqs#QueueDoesNotExist", "message": "no"}"#
                )
            ),
            Answer::Failed("queue_does_not_exist".into())
        );
    }
}
