//! Provider calls (area 11, "Calling" and "Decoding"; chunk 7.2c): a
//! provider package's operation called through its connection, on the
//! capability seam ([`crate::effects::send`]), so it is refused inside an
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

use std::fmt;
use std::rc::Rc;

use jadpo_std::HttpMethod;
use jadpo_std::provider::{
    Address, ConnectionPlan, CredentialScheme, OperationPlan, PathPart, ProviderPlan,
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
        // It did nothing: sending it again is safe.
        408 | 429 => Answer::Unavailable,
        400..=499 => match code() {
            Some(code) if reading.in_use.as_deref() == Some(code.as_str()) => Answer::Unavailable,
            Some(code) => Answer::Failed(code),
            None => Answer::Failed(String::new()),
        },
        // A server's error may come after it acted.
        _ => Answer::Unknown,
    }
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
/// code never reads a credential (area 10, "Secrets").
#[derive(Clone, Default)]
pub struct Credentials(Vec<Option<String>>);

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("held", &self.0.iter().filter(|held| held.is_some()).count())
            .finish()
    }
}

impl Credentials {
    /// The credentials `values` hold for `plan`'s connections.
    pub fn read(plan: &ProviderPlan, values: &crate::config::ConfigValues) -> Credentials {
        Credentials(
            plan.connections
                .iter()
                .map(|connection| {
                    let (_, at) = connection.credential?;
                    values.text_at(at).map(str::to_string)
                })
                .collect(),
        )
    }

    /// The credential of the connection at `at` in the plan.
    pub fn get(&self, at: u32) -> Option<&str> {
        self.0.get(at as usize)?.as_deref()
    }

    /// Credentials given as they are, by the connection's place: for a
    /// test.
    pub fn given(held: Vec<Option<String>>) -> Credentials {
        Credentials(held)
    }
}

// --- the call -----------------------------------------------------------------

/// What a provider call runs with besides its input: the program's
/// provider plan, the codecs its operations' types are written and read
/// with, and its failures.
#[derive(Clone, Copy)]
pub struct Providers<'a> {
    pub plan: &'a ProviderPlan,
    pub json: JsonCodecs<'a>,
    pub failures: &'a [FailurePlan],
    pub credentials: &'a Credentials,
    /// The application's name, which every key is derived from.
    pub application: &'a str,
}

impl fmt::Debug for Providers<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Providers")
            .field("connections", &self.plan.connections.len())
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
/// environment's, or its `host:`'s. The executor gives their values to
/// [`call`].
pub fn settings_read(connection: &ConnectionPlan) -> Vec<u32> {
    match &connection.address {
        Address::Hosts { environment, .. } => vec![*environment],
        Address::Setting(at) => vec![*at],
        Address::Unknown => Vec::new(),
    }
}

/// Calls operation `operation` (`bookings.create`) of connection
/// `connection` with `input`, from `ctx`: `settings` gives the values of
/// the settings [`settings_read`] names.
pub async fn call(
    ctx: &mut Ctx<'_>,
    providers: &Providers<'_>,
    (connection, operation): (&str, &str),
    input: Option<Value>,
    settings: &[(u32, Value)],
) -> Done {
    let Some((plan, op)) = providers.plan.operation(connection, operation) else {
        return Done::Fault(FaultKind::Defect(format!(
            "`{connection}.{operation}`, which the plan doesn't have"
        )));
    };
    let name = format!("{connection}.{operation}");
    let at = providers
        .plan
        .connections
        .iter()
        .position(|known| known.name == plan.name)
        .and_then(|at| u32::try_from(at).ok())
        .unwrap_or(u32::MAX);
    let made = Call::new(
        connection,
        operation,
        input.clone().unwrap_or(Value::Unit),
        op.repeat,
    );
    let request = match request(ctx, providers, (plan, op, at), input.as_ref(), settings) {
        Ok(request) => request,
        Err(Unready::Misconfigured) => {
            let now = ctx.now();
            ctx.capabilities_mut().refused(&made, now);
            return Done::Failed(Box::new(crate::misconfigured()));
        }
        Err(Unready::Fault(kind)) => return Done::Fault(kind),
    };
    let sent = crate::effects::send(ctx, made.with_request(request)).await;
    match sent {
        Ok(Given::Value(value)) => Done::Value(value),
        Ok(Given::Wire(text)) => decode(providers, op, &name, &text),
        Err(Unanswered::Failed(code)) => failed(providers, op, &name, &code),
        Err(Unanswered::Failure(failure)) => Done::Failed(failure),
        Err(Unanswered::Fault(kind)) => Done::Fault(kind),
    }
}

/// Why a request couldn't be made.
enum Unready {
    /// The connection's address or credential isn't usable.
    Misconfigured,
    Fault(FaultKind),
}

/// The request `op` sends with `input`.
fn request(
    ctx: &mut Ctx<'_>,
    providers: &Providers<'_>,
    (plan, op, at): (&ConnectionPlan, &OperationPlan, u32),
    input: Option<&Value>,
    settings: &[(u32, Value)],
) -> Result<Request, Unready> {
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
    // the wire.
    let mut members = match (op.input, input) {
        (Some(number), Some(value)) => {
            let Some(codec) = providers.json.codec_of(number) else {
                return Err(Unready::Fault(FaultKind::Defect(
                    "a provider's input the plan gave no codec".into(),
                )));
            };
            let written = providers.json.codecs.write_value(codec, value);
            match serde_json::from_str::<serde_json::Value>(&written) {
                Ok(serde_json::Value::Object(members)) => members,
                _ => {
                    return Err(Unready::Fault(FaultKind::Defect(
                        "a provider's input that isn't an object".into(),
                    )));
                }
            }
        }
        _ => serde_json::Map::new(),
    };
    let mut url = base.trim_end_matches('/').to_string();
    for part in &op.path {
        match part {
            PathPart::Fixed(text) => url.push_str(text),
            PathPart::Field(name) => match members.remove(name) {
                Some(value) => match plain(&value) {
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
        for (name, value) in &members {
            let name = jadpo_value::encoding::to_url_component(name);
            match value {
                serde_json::Value::Null => {}
                serde_json::Value::Array(items) => {
                    for item in items {
                        if let Some(text) = plain(item).or_else(|| Some(item.to_string())) {
                            query.push(format!(
                                "{name}={}",
                                jadpo_value::encoding::to_url_component(&text)
                            ));
                        }
                    }
                }
                other => {
                    let text = plain(other).unwrap_or_else(|| other.to_string());
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
        Some(serde_json::Value::Object(members).to_string())
    };
    let mut headers = vec![
        ("Accept".to_string(), "application/json".to_string()),
        ("user-agent".to_string(), "jadpo".to_string()),
    ];
    if body.is_some() {
        headers.push(("Content-Type".to_string(), "application/json".to_string()));
    }
    if let Some((scheme, _)) = plan.credential {
        let Some(secret) = providers.credentials.get(at) else {
            return Err(Unready::Misconfigured);
        };
        let value = match scheme {
            CredentialScheme::Bearer => format!("Bearer {secret}"),
            CredentialScheme::Basic => format!(
                "Basic {}",
                jadpo_value::encoding::to_base64(secret.as_bytes())
            ),
        };
        headers.push(("Authorization".to_string(), value));
    }
    let mut reading = Reading {
        failed_when: op.failed_when.clone(),
        error_code: plan.error_code.clone(),
        in_use: None,
        reused: None,
    };
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
            let derived = key(
                &KeyParts {
                    application: providers.application,
                    environment: &environment,
                    connection: &plan.name,
                    scope: &call_key.scope,
                    action: &call_key.key,
                    step,
                    generation: 0,
                },
                idempotency.max_length,
            );
            headers.push((idempotency.header.clone(), derived));
        }
    }
    Ok(Request {
        method: op.method,
        url,
        headers,
        body,
        timeout_ms: plan.timeout_ms,
        reading,
    })
}

/// A JSON value as text in a path or a query: a string as it is, a number
/// or a flag as JSON writes it.
fn plain(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Number(number) => Some(number.to_string()),
        serde_json::Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

/// `address` when a call may be sent to it (area 11, question 9): HTTPS,
/// or plain HTTP to this machine, which is how a test reaches its server;
/// a host and an optional port and path, with no credentials, query or
/// fragment of its own.
pub fn sendable(address: &str) -> Option<String> {
    let address = address.trim();
    let (secure, rest) = if let Some(rest) = address.strip_prefix("https://") {
        (true, rest)
    } else {
        (false, address.strip_prefix("http://")?)
    };
    let authority = rest.split('/').next().unwrap_or_default();
    if authority.is_empty() || authority.contains('@') || rest.contains(['?', '#']) {
        return None;
    }
    let host = if let Some(inner) = authority.strip_prefix('[') {
        format!("[{}]", inner.split(']').next().unwrap_or_default())
    } else {
        authority
            .rsplit_once(':')
            .map_or(authority, |(host, _)| host)
            .to_string()
    };
    if host.is_empty() {
        return None;
    }
    let host = host.to_ascii_lowercase();
    let loopback = host == "localhost"
        || host == "[::1]"
        || host.starts_with("127.") && host.split('.').count() == 4;
    (secure || loopback).then(|| address.to_string())
}

/// The value `text`, an answer as sent, holds as `op`'s output.
fn decode(providers: &Providers<'_>, op: &OperationPlan, name: &str, text: &str) -> Done {
    let Some(number) = op.output else {
        return Done::Value(Value::Unit);
    };
    let unreadable =
        |why: String| Done::Fault(FaultKind::Provider(format!("`{name}`'s answer {why}")));
    let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else {
        return unreadable("isn't JSON".to_string());
    };
    let at = match &op.output_at {
        Some(path) => match at_path(&json, path) {
            Some(at) => at,
            None => return unreadable(format!("has nothing at `{path}`")),
        },
        None => &json,
    };
    let Some(codec) = providers.json.codec_of(number) else {
        return Done::Fault(FaultKind::Defect(
            "a provider's output the plan gave no codec".into(),
        ));
    };
    let mut problems = Vec::new();
    let read = providers.json.codecs.read(
        codec,
        at,
        &jadpo_value::json::Path::default(),
        &mut problems,
    );
    match (read, problems.first()) {
        (Some(value), None) => Done::Value(value),
        (_, Some(problem)) if problem.field.is_empty() => {
            unreadable(format!("doesn't decode: `{}`", problem.rule))
        }
        (_, Some(problem)) => unreadable(format!(
            "doesn't decode: `{}` on `{}`",
            problem.rule, problem.field
        )),
        (None, None) => unreadable("doesn't decode".to_string()),
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
}
