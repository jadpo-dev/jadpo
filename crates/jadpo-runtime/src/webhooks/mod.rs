//! Webhooks as events (area 11, "Receiving from outside"; area 12,
//! "Events from outside"; chunk 7.7): a connection given a webhook secret
//! has `POST <prefix>/<connection>`, which the runtime answers itself.
//!
//! - The signature is checked over the raw body with the secret, or the
//!   one it replaced while a deployment rotates it, as the package's
//!   scheme says ([`check`]): Stripe's, Standard Webhooks', an HMAC in a
//!   header the provider names, or basic authentication. A signed time
//!   more than five minutes from the host's clock is refused, so an old
//!   request replayed is.
//! - The body is JSON; the provider's event id and type are read from the
//!   places `event_id:` and `event_type:` name. A type the package doesn't
//!   declare is acknowledged and not kept, since the provider sends every
//!   type the account subscribes to.
//! - The event's fields are read from the body with its type's codec on
//!   the provider's wire, `occurred_at` filled by the runtime: from the
//!   place `occurred_at:` names (seconds since 1970, or RFC 3339 text),
//!   else the signature's time, else when it arrived. A body its event
//!   doesn't read is `422 invalid_input`, so the provider keeps it and
//!   sends it again once the package is fixed.
//! - In one transaction in the events' store, the event's id is recorded
//!   under its source (`jadpo_received_events`), and a repeat, found
//!   there, is acknowledged without anything kept; otherwise its outbox
//!   row and a delivery for each subscriber are written, as an action's
//!   events are ([`crate::outbox`]), and delivered after the response.
//!
//! Each accepted request is answered `204`, a repeat as the first was. An
//! event whose payload is over 64 KB, every event's limit, is `413`.
//!
//! Since chunk 7.7b: Twilio's HMAC-SHA1 over its form body and Slack's
//! scheme ([`check`]), AWS SNS's certificates ([`sns`]); the handshakes a
//! package names ([`handshake`]); and a webhook that needs a reply,
//! answered by the app's action inside the request ([`replies`]).

mod handshake;
pub mod replies;
pub mod sns;

use jadpo_std::provider::{SignatureScheme, WebhookHandshake, WebhookPlan, WireEncoding};
use jadpo_value::Value;
use jadpo_value::json::{Path, Problem};

use crate::http::{Request, Response};
use crate::outbox::{Delivery, Sealed};
use crate::provider::Credentials;
use crate::{Ctx, Executor, Isolation, JsonCodecs, SqlValue, StoreError};

/// How far a signed time may be from the host's clock, either way.
pub const TOLERANCE_MS: i64 = 5 * 60 * 1000;

/// How long an event's id is kept once received, so a provider sending it
/// again is answered as the first time: longer than any provider retries
/// for (Stripe's three days, Standard Webhooks' few), deleted by the tick
/// after.
pub const RECEIVED_KEPT_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// Records the id of an event received, unless it was already:
/// `RETURNING` a row only the first time.
pub const RECEIVE: &str = "INSERT INTO \"jadpo_received_events\" (\"source\", \"event_id\", \"received_at\") VALUES ($1, $2, $3) ON CONFLICT DO NOTHING RETURNING 1";

/// Deletes the ids received before `$1`, `$2` at a time, for the tick.
pub const OLD_RECEIVED: &str = "DELETE FROM \"jadpo_received_events\" WHERE (\"source\", \"event_id\") IN (SELECT \"source\", \"event_id\" FROM \"jadpo_received_events\" WHERE \"received_at\" <= $1 LIMIT $2) RETURNING 1";

/// What the runtime receives a program's webhooks with: the plan's, the
/// secrets each is checked with, and the codecs their events are read and
/// kept with.
#[derive(Clone, Copy, Debug)]
pub struct Webhooks<'a> {
    pub plan: &'a [WebhookPlan],
    pub credentials: &'a Credentials,
    pub json: JsonCodecs<'a>,
}

impl<'a> Webhooks<'a> {
    /// The webhook whose route is `segments`, with its place in the plan.
    pub fn route(&self, segments: &[String]) -> Option<(usize, &'a WebhookPlan)> {
        self.plan
            .iter()
            .enumerate()
            .find(|(_, plan)| plan.segments == segments)
    }
}

/// Why a webhook's signature was refused: for the host's log, never sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// It carries no signature the scheme reads.
    Unsigned,
    /// Its signed time is more than [`TOLERANCE_MS`] from the host's clock.
    Stale,
    /// No signature matches any secret.
    Mismatch,
    /// The deployment gives no secret to check it with.
    NoSecret,
}

impl Refusal {
    pub fn word(self) -> &'static str {
        match self {
            Refusal::Unsigned => "unsigned",
            Refusal::Stale => "stale",
            Refusal::Mismatch => "mismatch",
            Refusal::NoSecret => "no secret",
        }
    }
}

/// Checks `request`'s signature as `scheme` reads it against `secrets`
/// at `now`: the time it was signed at, when the scheme signs one, or why
/// it is refused.
pub fn check(
    scheme: &SignatureScheme,
    secrets: &[&str],
    request: &Request,
    now: i64,
) -> Result<Option<i64>, Refusal> {
    if secrets.is_empty() {
        return Err(Refusal::NoSecret);
    }
    match scheme {
        SignatureScheme::Stripe => stripe(secrets, request, now).map(Some),
        SignatureScheme::StandardWebhooks => standard(secrets, request, now).map(Some),
        SignatureScheme::HmacSha256 { header } => hmac(secrets, request, header).map(|()| None),
        SignatureScheme::Basic => basic(secrets, request).map(|()| None),
        SignatureScheme::Twilio => twilio(secrets, request).map(|()| None),
        SignatureScheme::Slack => slack(secrets, request, now).map(Some),
        // Checked against its certificate, which is fetched ([`sns::check`]).
        SignatureScheme::Sns => Err(Refusal::Unsigned),
    }
}

/// A time in seconds since 1970, as a signature sends it, in milliseconds
/// and within [`TOLERANCE_MS`] of `now`.
fn fresh(seconds: &str, now: i64) -> Result<i64, Refusal> {
    let seconds: i64 = seconds.trim().parse().map_err(|_| Refusal::Unsigned)?;
    let at = seconds.checked_mul(1000).ok_or(Refusal::Unsigned)?;
    if now.abs_diff(at) > TOLERANCE_MS.unsigned_abs() {
        return Err(Refusal::Stale);
    }
    Ok(at)
}

/// `Stripe-Signature: t=<seconds>,v1=<hex>[,v1=<hex>]`, over `<t>.<body>`.
fn stripe(secrets: &[&str], request: &Request, now: i64) -> Result<i64, Refusal> {
    let header = request
        .header("stripe-signature")
        .ok_or(Refusal::Unsigned)?;
    let mut time = None;
    let mut macs = Vec::new();
    for part in header.split(',') {
        match part.trim().split_once('=') {
            Some(("t", value)) => time = Some(value.trim()),
            Some(("v1", value)) => {
                if let Some(mac) = jadpo_value::encoding::from_hex(value.trim()) {
                    macs.push(mac);
                }
            }
            _ => {}
        }
    }
    let (Some(time), false) = (time, macs.is_empty()) else {
        return Err(Refusal::Unsigned);
    };
    let at = fresh(time, now)?;
    let signed = [time.as_bytes(), b".", &request.body];
    any(secrets, &macs, |secret| secret.as_bytes().to_vec(), &signed)?;
    Ok(at)
}

/// Standard Webhooks: `webhook-id`, `webhook-timestamp` and
/// `webhook-signature: v1,<base64> ...` (or `svix-` for `webhook-`), over
/// `<id>.<timestamp>.<body>`, under the Base64 after the secret's
/// `whsec_`.
fn standard(secrets: &[&str], request: &Request, now: i64) -> Result<i64, Refusal> {
    let header = |name: &str| {
        request
            .header(&format!("webhook-{name}"))
            .or_else(|| request.header(&format!("svix-{name}")))
    };
    let (Some(id), Some(time), Some(signatures)) =
        (header("id"), header("timestamp"), header("signature"))
    else {
        return Err(Refusal::Unsigned);
    };
    let macs: Vec<Vec<u8>> = signatures
        .split(' ')
        .filter_map(|signature| signature.trim().strip_prefix("v1,"))
        .filter_map(jadpo_value::encoding::from_base64)
        .collect();
    if macs.is_empty() {
        return Err(Refusal::Unsigned);
    }
    let at = fresh(time, now)?;
    let signed = [
        id.as_bytes(),
        b".",
        time.trim().as_bytes(),
        b".",
        &request.body,
    ];
    let key = |secret: &str| {
        secret
            .strip_prefix("whsec_")
            .and_then(jadpo_value::encoding::from_base64)
            .unwrap_or_else(|| secret.as_bytes().to_vec())
    };
    any(secrets, &macs, key, &signed)?;
    Ok(at)
}

/// The body's HMAC-SHA-256 in `header`, as hex or Base64, with or without
/// a leading `sha256=`.
fn hmac(secrets: &[&str], request: &Request, header: &str) -> Result<(), Refusal> {
    let value = request.header(header).ok_or(Refusal::Unsigned)?.trim();
    let value = value
        .strip_prefix("sha256=")
        .or_else(|| value.strip_prefix("SHA256="))
        .unwrap_or(value);
    let mac = if value.len() == 64 {
        jadpo_value::encoding::from_hex(value)
    } else {
        jadpo_value::encoding::from_base64(value)
    }
    .ok_or(Refusal::Unsigned)?;
    any(
        secrets,
        &[mac],
        |secret| secret.as_bytes().to_vec(),
        &[&request.body],
    )
}

/// `Authorization: Basic <base64 of user:password>`, the secret as
/// `user:password`.
fn basic(secrets: &[&str], request: &Request) -> Result<(), Refusal> {
    let header = request.header("authorization").ok_or(Refusal::Unsigned)?;
    let (scheme, encoded) = header.trim().split_once(' ').ok_or(Refusal::Unsigned)?;
    if !scheme.eq_ignore_ascii_case("basic") {
        return Err(Refusal::Unsigned);
    }
    let given = jadpo_value::encoding::from_base64(encoded.trim()).ok_or(Refusal::Unsigned)?;
    // Every secret is compared, so the time says nothing of which matched.
    let mut matched = false;
    for secret in secrets {
        matched |= jadpo_value::crypto::same_secret(&given, secret.as_bytes());
    }
    if matched {
        Ok(())
    } else {
        Err(Refusal::Mismatch)
    }
}

/// Twilio's `X-Twilio-Signature`: the Base64 of the HMAC-SHA1, under the
/// auth token, of the URL Twilio requested and then each of the form
/// body's members, name then value, sorted by name (and by value for a
/// name sent twice). The URL is `https://`, the request's `Host`, its path
/// and its query: Twilio sends only to HTTPS, and a proxy in front of the
/// host ends the TLS. With the port `:443` written and without, as Twilio's
/// own libraries try both.
fn twilio(secrets: &[&str], request: &Request) -> Result<(), Refusal> {
    let given = request
        .header("x-twilio-signature")
        .ok_or(Refusal::Unsigned)?;
    let mac = jadpo_value::encoding::from_base64(given.trim()).ok_or(Refusal::Unsigned)?;
    let host = request
        .header("host")
        .map(str::trim)
        .filter(|host| !host.is_empty())
        .ok_or(Refusal::Unsigned)?;
    let mut members = form_pairs(&request.body).ok_or(Refusal::Unsigned)?;
    members.sort();
    let mut signed = String::new();
    for (name, value) in &members {
        signed.push_str(name);
        signed.push_str(value);
    }
    let query = request
        .query
        .as_deref()
        .map(|query| format!("?{query}"))
        .unwrap_or_default();
    let bare = host.strip_suffix(":443").unwrap_or(host);
    let mut matched = false;
    for written in [bare.to_string(), format!("{bare}:443")] {
        let url = format!("https://{written}{}{query}", request.path);
        for secret in secrets {
            matched |= jadpo_value::crypto::signed_by_sha1(
                secret.as_bytes(),
                &[url.as_bytes(), signed.as_bytes()],
                &mac,
            );
        }
    }
    if matched {
        Ok(())
    } else {
        Err(Refusal::Mismatch)
    }
}

/// Slack's `X-Slack-Signature: v0=<hex>`, HMAC-SHA-256 of
/// `v0:<X-Slack-Request-Timestamp>:<body>` under the signing secret.
fn slack(secrets: &[&str], request: &Request, now: i64) -> Result<i64, Refusal> {
    let (Some(signature), Some(time)) = (
        request.header("x-slack-signature"),
        request.header("x-slack-request-timestamp"),
    ) else {
        return Err(Refusal::Unsigned);
    };
    let mac = signature
        .trim()
        .strip_prefix("v0=")
        .and_then(jadpo_value::encoding::from_hex)
        .ok_or(Refusal::Unsigned)?;
    let at = fresh(time, now)?;
    let signed = [b"v0:", time.trim().as_bytes(), b":", &request.body];
    any(
        secrets,
        &[mac],
        |secret| secret.as_bytes().to_vec(),
        &signed,
    )?;
    Ok(at)
}

/// A form body's members, each name and value decoded, in the order sent;
/// `None` when it isn't a form.
fn form_pairs(body: &[u8]) -> Option<Vec<(String, String)>> {
    let text = std::str::from_utf8(body).ok()?;
    let mut out = Vec::new();
    for pair in text.split('&').filter(|pair| !pair.is_empty()) {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        out.push((
            crate::http::percent_decode(name, true)?,
            crate::http::percent_decode(value, true)?,
        ));
    }
    Some(out)
}

/// A form body as the JSON its event is read from: each member's text,
/// and a name sent more than once a list of them.
fn form_json(body: &[u8]) -> Option<serde_json::Value> {
    let mut members = serde_json::Map::new();
    for (name, value) in form_pairs(body)? {
        let value = serde_json::Value::String(value);
        match members.get_mut(&name) {
            None => {
                members.insert(name, value);
            }
            Some(serde_json::Value::Array(items)) => items.push(value),
            Some(first) => *first = serde_json::Value::Array(vec![first.clone(), value]),
        }
    }
    Some(serde_json::Value::Object(members))
}

/// Whether any of `macs` is the HMAC-SHA-256 of `signed` under any of
/// `secrets`, each made a key by `key`: every pair is compared, in
/// constant time, so the time says nothing of which matched.
fn any(
    secrets: &[&str],
    macs: &[Vec<u8>],
    key: impl Fn(&str) -> Vec<u8>,
    signed: &[&[u8]],
) -> Result<(), Refusal> {
    let mut matched = false;
    for secret in secrets {
        let key = key(secret);
        for mac in macs {
            matched |= jadpo_value::crypto::signed_by(&key, signed, mac);
        }
    }
    if matched {
        Ok(())
    } else {
        Err(Refusal::Mismatch)
    }
}

// --- answering ------------------------------------------------------------------

/// What the runtime answers a webhook with itself (area 7, "How failures
/// reach the client").
const SIGNATURE: (u16, &str, &str) = (
    401,
    "unauthenticated",
    "The webhook's signature doesn't match.",
);
const NOT_JSON: (u16, &str, &str) = (400, "invalid_request", "A webhook's body is JSON.");
const NOT_FORM: (u16, &str, &str) = (400, "invalid_request", "A webhook's body is a form.");
const NO_TOKEN: (u16, &str, &str) = (
    400,
    "invalid_request",
    "The webhook's validation token can't be answered.",
);
const EVENT_TOO_LARGE: (u16, &str, &str) = (
    413,
    "request_too_large",
    "The webhook's event is over 64 KB.",
);
const NO_EVENT: (u16, &str, &str) = (
    400,
    "invalid_request",
    "The webhook's body has no event id, type or time where its package says.",
);

/// Answers `request` to webhook `plan`, at place `at` in `webhooks`, in
/// `ctx`: the request's path is the webhook's route.
pub(crate) async fn answer(
    app: &crate::http::App<'_>,
    executor: &dyn Executor,
    webhooks: &Webhooks<'_>,
    (at, plan): (usize, &WebhookPlan),
    ctx: &mut Ctx<'_>,
    request: &Request,
    request_id: &str,
) -> Response {
    use crate::http::{METHOD_NOT_ALLOWED, runtime_failure as failure};
    let mut response = match request.method.as_str() {
        "POST" => {
            receive(
                app,
                executor,
                webhooks,
                (at, plan),
                ctx,
                request,
                request_id,
            )
            .await
        }
        "OPTIONS" => Response {
            status: 204,
            headers: vec![("allow".to_string(), "OPTIONS, POST".to_string())],
            ..Response::default()
        },
        _ => {
            let mut refused = failure(METHOD_NOT_ALLOWED, request_id, None);
            refused
                .headers
                .push(("allow".to_string(), "OPTIONS, POST".to_string()));
            refused
        }
    };
    response.route = Some(format!("POST {}", path(plan)));
    response
}

/// `/webhooks/payments`.
fn path(plan: &WebhookPlan) -> String {
    plan.segments
        .iter()
        .map(|segment| format!("/{segment}"))
        .collect()
}

async fn receive(
    app: &crate::http::App<'_>,
    executor: &dyn Executor,
    webhooks: &Webhooks<'_>,
    (at, plan): (usize, &WebhookPlan),
    ctx: &mut Ctx<'_>,
    request: &Request,
    request_id: &str,
) -> Response {
    use crate::http::{REQUEST_TOO_LARGE, UNAVAILABLE, invalid_input, runtime_failure as failure};
    if request.too_large || request.body.len() > crate::http::BODY_LIMIT {
        return failure(REQUEST_TOO_LARGE, request_id, None);
    }
    // Microsoft Graph checks an address with a request it doesn't sign, so
    // its validation is answered before any signature is looked for.
    if plan.handshake == Some(WebhookHandshake::MicrosoftGraph)
        && let Some(answered) = handshake::graph(request, request_id)
    {
        return answered;
    }
    let now = ctx.now();
    let secrets = webhooks.credentials.webhook(at);
    let signed = match &plan.scheme {
        SignatureScheme::Sns => {
            match sns::check(ctx.host().outbound(), &secrets, request, now).await {
                Ok(message) => {
                    // A confirmation, signed as every message is, from the
                    // connection's topic; anything but a notification carries
                    // no event.
                    if message.kind != "Notification" {
                        return handshake::sns(ctx, plan, &message, request_id).await;
                    }
                    Ok(Some(message.signed_at))
                }
                Err(sns::Unchecked::Refused(refusal)) => Err(refusal),
                Err(sns::Unchecked::Unreachable) => {
                    let mut unavailable = failure(UNAVAILABLE, request_id, None);
                    unavailable.code = Some("unavailable (certificate)".into());
                    return unavailable;
                }
            }
        }
        scheme => check(scheme, &secrets, request, now),
    };
    let signed = match signed {
        Ok(signed) => signed,
        Err(refusal) => {
            let mut refused = failure(SIGNATURE, request_id, None);
            refused.code = Some(format!("unauthenticated ({})", refusal.word()));
            return refused;
        }
    };
    let (mut json, numbers) = if plan.scheme.form_body() {
        match form_json(&request.body) {
            Some(json) => (json, jadpo_value::json::Numbers::default()),
            None => return failure(NOT_FORM, request_id, None),
        }
    } else {
        match jadpo_value::json::parse(&request.body) {
            Some(parsed) => parsed,
            None => return failure(NOT_JSON, request_id, None),
        }
    };
    if plan.scheme == SignatureScheme::Sns {
        sns_message(&mut json);
    }
    if plan.handshake == Some(WebhookHandshake::Slack)
        && let Some(answered) = handshake::slack(&json)
    {
        return answered;
    }
    let id = match crate::provider::at_path(&json, &plan.event_id) {
        Some(serde_json::Value::String(text)) if !text.is_empty() => text.clone(),
        Some(serde_json::Value::Number(number)) if number.is_u64() || number.is_i64() => {
            number.to_string()
        }
        _ => return failure(NO_EVENT, request_id, None),
    };
    let Some(serde_json::Value::String(kind)) = crate::provider::at_path(&json, &plan.event_type)
    else {
        return failure(NO_EVENT, request_id, None);
    };
    let Some(event) = plan.events.iter().find(|event| event.wire_name == *kind) else {
        // A type the package doesn't declare: the provider sends every type
        // the account subscribes to, so it is acknowledged, not kept.
        return accepted();
    };
    let occurred = match &plan.occurred_at {
        Some(place) => crate::provider::at_path(&json, place).and_then(instant),
        None => jadpo_value::json::from_wire(
            WireEncoding::UnixMilliseconds,
            &serde_json::Value::from(signed.unwrap_or(now)),
        )
        .ok(),
    };
    let Some(occurred) = occurred else {
        return failure(NO_EVENT, request_id, None);
    };
    // The runtime fills `occurred_at`, whatever the body holds of that
    // name.
    if let serde_json::Value::Object(members) = &mut json {
        members.insert(crate::webhooks::OCCURRED_AT.to_string(), occurred);
    }
    let value = match read(webhooks.json, event.number, &json, &numbers) {
        Ok(value) => value,
        Err(problems) => return invalid_input(request_id, &problems),
    };
    // A webhook that needs a reply is answered by the app's action, inside
    // the request, and not kept for subscribers (chunk 7.7b).
    if let Some(reply) = &event.reply {
        return replies::answer(
            app,
            executor,
            webhooks,
            plan,
            reply,
            ctx,
            request_id,
            (&id, value),
        )
        .await;
    }
    let payload = match webhooks.json.write(event.number, &value) {
        Ok(SqlValue::Json(text)) => text,
        _ => return internal(request_id, "a webhook's event that doesn't write as JSON"),
    };
    // Every event is held to one size, as an action's are (area 12,
    // "Limits").
    if payload.len() > crate::outbox::MAX_EVENT_BYTES {
        return failure(EVENT_TOO_LARGE, request_id, None);
    }
    let kept = Kept {
        store: &plan.store,
        source: source(plan),
        event: &event.key,
        connection: Some(&plan.connection),
    };
    match keep(executor, &kept, ctx, &id, value, payload).await {
        Ok(deliveries) => {
            let deliveries = deliveries.unwrap_or_default();
            if !deliveries.is_empty() {
                ctx.defer(crate::runner::Deferred::Deliveries(deliveries));
            }
            accepted()
        }
        Err(StoreError::Fault(reason)) => internal(request_id, &reason),
        Err(_) => failure(UNAVAILABLE, request_id, None),
    }
}

/// An SNS notification's `Message`, read as the JSON it holds when it
/// holds JSON, so the package's places reach inside it
/// (`"Message.order_id"`); left as its text otherwise.
fn sns_message(json: &mut serde_json::Value) {
    let serde_json::Value::Object(members) = json else {
        return;
    };
    let Some(serde_json::Value::String(message)) = members.get("Message") else {
        return;
    };
    if let Some((inner, _)) = jadpo_value::json::parse(message.as_bytes()) {
        members.insert("Message".to_string(), inner);
    }
}

/// The field of every webhook event the runtime fills.
pub const OCCURRED_AT: &str = "occurred_at";

/// `204`, nothing in the body.
fn accepted() -> Response {
    Response {
        status: 204,
        ..Response::default()
    }
}

/// `500 internal_fault`, with the fault for the host's log.
pub(crate) fn internal(request_id: &str, reason: &str) -> Response {
    let mut response = crate::http::runtime_failure(crate::http::INTERNAL_FAULT, request_id, None);
    response.fault = Some(Box::new(crate::Fault {
        kind: crate::FaultKind::Defect(reason.to_string()),
        frames: Vec::new(),
    }));
    response
}

/// A time the provider sends, as the contract writes an `Instant`: a
/// whole number of seconds since 1970, or RFC 3339 text.
fn instant(sent: &serde_json::Value) -> Option<serde_json::Value> {
    let encoding = match sent {
        serde_json::Value::Number(_) => WireEncoding::UnixSeconds,
        serde_json::Value::String(_) => WireEncoding::Iso8601,
        _ => return None,
    };
    jadpo_value::json::from_wire(encoding, sent).ok()
}

/// The value `json`, a webhook's body or a queue's message, holds as the
/// event numbered `number`, read with its codec on the provider's wire;
/// each problem by field and rule otherwise.
pub(crate) fn read(
    json: JsonCodecs<'_>,
    number: u32,
    body: &serde_json::Value,
    numbers: &jadpo_value::json::Numbers,
) -> Result<Value, Vec<Problem>> {
    let Some(codec) = json.codec_of(number) else {
        return Err(vec![Problem {
            field: String::new(),
            rule: "type".into(),
        }]);
    };
    let wire = Wire;
    let codecs = jadpo_value::json::Codecs {
        coder: Some(&wire),
        ..json.codecs
    };
    let mut problems = Vec::new();
    let read = codecs.read(
        codec,
        body,
        &Path::default().numbered(numbers),
        &mut problems,
    );
    match (read, problems.is_empty()) {
        (Some(value), true) => Ok(value),
        _ if problems.is_empty() => Err(vec![Problem {
            field: String::new(),
            rule: "type".into(),
        }]),
        _ => Err(problems),
    }
}

/// The provider's wire for a webhook's body: the built-in encodings, and
/// no package function, which the checker keeps from a webhook's event.
#[derive(Debug)]
pub(crate) struct Wire;

impl jadpo_value::json::Coder for Wire {
    fn decode(&self, _function: &str, _wire: Value) -> Option<Value> {
        None
    }

    fn encode(&self, _function: &str, _value: Value) -> Option<Value> {
        None
    }

    fn unwritten(&self, _rule: &str) {}
}

/// Where an event received from outside is kept: its store, the source
/// its id is recorded under, its event type's key, and the connection
/// whose webhook it came in on, whose package's own subscribers hear it
/// (chunk 7.6b); `None` for a queue's message.
pub(crate) struct Kept<'a> {
    pub(crate) store: &'a str,
    pub(crate) source: String,
    pub(crate) event: &'a str,
    pub(crate) connection: Option<&'a str>,
}

/// Keeps event `value` with the sender's id `id` as `kept` says: its id
/// recorded, then its outbox row and a delivery for each subscriber, in
/// one transaction; nothing for a repeat. The deliveries to make once it
/// commits.
pub(crate) async fn keep(
    executor: &dyn Executor,
    kept: &Kept<'_>,
    ctx: &mut Ctx<'_>,
    id: &str,
    value: Value,
    payload: String,
) -> Result<Option<Vec<Delivery>>, StoreError> {
    let now = ctx.now();
    let claim = vec![
        SqlValue::Text(kept.source.clone()),
        SqlValue::Text(id.to_string()),
        crate::outbox::micros(now),
    ];
    keep_claimed(executor, kept, ctx, (RECEIVE, claim), value, payload).await
}

/// [`keep`], the event claimed first by `claim`, a statement that returns
/// a row when the event is to be kept and none when it isn't: a webhook's
/// id recorded, or a poll's answer seen to change (chunk 7.6c).
pub(crate) async fn keep_claimed(
    executor: &dyn Executor,
    kept: &Kept<'_>,
    ctx: &mut Ctx<'_>,
    (claim, params): (&str, Vec<SqlValue>),
    value: Value,
    payload: String,
) -> Result<Option<Vec<Delivery>>, StoreError> {
    let now = ctx.now();
    let trace = ctx.trace();
    let subscribers: Vec<String> = executor
        .events()
        .map(|events| {
            events
                .plan
                .hearing(kept.event, kept.connection)
                .map(|subscriber| subscriber.key.clone())
                .collect()
        })
        .unwrap_or_default();
    let Some(outbox) = ctx.new_id() else {
        return Err(StoreError::Fault(
            "no random bytes for an event's identity".into(),
        ));
    };
    let mut sealed = Sealed {
        outbox: vec![(outbox, kept.event.to_string(), payload)],
        deliveries: Vec::with_capacity(subscribers.len()),
    };
    for subscriber in subscribers {
        let Some(delivery) = ctx.new_id() else {
            return Err(StoreError::Fault(
                "no random bytes for a delivery's identity".into(),
            ));
        };
        sealed.deliveries.push(Delivery {
            store: kept.store.to_string(),
            id: delivery,
            outbox,
            subscriber,
            event: kept.event.to_string(),
            value: value.clone(),
            emitted_at: now,
            trace,
            depth: 0,
        });
    }
    let database = ctx
        .capabilities()
        .database(kept.store)
        .ok_or_else(|| StoreError::Fault(format!("no store called `{}`", kept.store)))?;
    let mut transaction = database.begin_at(Isolation::ReadCommitted).await?;
    let first = transaction.query(claim, &params).await;
    match first {
        Ok(rows) if rows.is_empty() => {
            transaction.rollback().await?;
            return Ok(None);
        }
        Ok(_) => {}
        Err(error) => {
            let _ = transaction.rollback().await;
            return Err(error);
        }
    }
    if let Err(error) = crate::outbox::write(transaction.as_mut(), &sealed, now, trace, 0).await {
        let _ = transaction.rollback().await;
        return Err(error);
    }
    transaction.commit().await?;
    Ok(Some(sealed.deliveries))
}

/// Where a webhook's events come from, as their ids are recorded:
/// `webhook payments`.
pub fn source(plan: &WebhookPlan) -> String {
    format!("webhook {}", plan.connection)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_614_265_330_000;

    fn request(headers: &[(&str, &str)], body: &str) -> Request {
        Request {
            method: "POST".into(),
            path: "/webhooks/payments".into(),
            headers: headers
                .iter()
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .collect(),
            body: body.as_bytes().to_vec(),
            ..Request::default()
        }
    }

    /// The Standard Webhooks specification's own example: its secret,
    /// message id, time, body and signature.
    #[test]
    fn a_standard_webhook_is_checked_as_its_specification_signs_one() {
        let secret = "whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw";
        let body = r#"{"test": 2432232314}"#;
        let headers = [
            ("webhook-id", "msg_p5jXN8AQM9LWM0D4loKWxJek"),
            ("webhook-timestamp", "1614265330"),
            (
                "webhook-signature",
                "v1,g0hM9SsE+OTPJTGt/tmIKtSyZlE3uFJELVlNIOLJ1OE=",
            ),
        ];
        let scheme = SignatureScheme::StandardWebhooks;
        assert_eq!(
            check(&scheme, &[secret], &request(&headers, body), NOW),
            Ok(Some(NOW))
        );
        // Svix's own headers say the same.
        let svix: Vec<(&str, &str)> = headers
            .iter()
            .map(|(name, value)| {
                (
                    match *name {
                        "webhook-id" => "svix-id",
                        "webhook-timestamp" => "svix-timestamp",
                        _ => "svix-signature",
                    },
                    *value,
                )
            })
            .collect();
        assert_eq!(
            check(&scheme, &[secret], &request(&svix, body), NOW),
            Ok(Some(NOW))
        );
        // Another signature among several still matches; a changed body,
        // another secret, a time six minutes off and no signature don't.
        let several = [
            headers[0],
            headers[1],
            (
                "webhook-signature",
                "v1,AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA= v1,g0hM9SsE+OTPJTGt/tmIKtSyZlE3uFJELVlNIOLJ1OE=",
            ),
        ];
        assert!(check(&scheme, &[secret], &request(&several, body), NOW).is_ok());
        assert_eq!(
            check(
                &scheme,
                &[secret],
                &request(&headers, r#"{"test": 1}"#),
                NOW
            ),
            Err(Refusal::Mismatch)
        );
        assert_eq!(
            check(&scheme, &["whsec_c2VjcmV0"], &request(&headers, body), NOW),
            Err(Refusal::Mismatch)
        );
        // The secret it replaced still checks it while it is given.
        assert!(
            check(
                &scheme,
                &["whsec_c2VjcmV0", secret],
                &request(&headers, body),
                NOW
            )
            .is_ok()
        );
        assert_eq!(
            check(
                &scheme,
                &[secret],
                &request(&headers, body),
                NOW + 6 * 60 * 1000
            ),
            Err(Refusal::Stale)
        );
        assert_eq!(
            check(
                &scheme,
                &[secret],
                &request(&headers, body),
                NOW - 6 * 60 * 1000
            ),
            Err(Refusal::Stale)
        );
        assert_eq!(
            check(&scheme, &[secret], &request(&headers[..2], body), NOW),
            Err(Refusal::Unsigned)
        );
        assert_eq!(
            check(&scheme, &[], &request(&headers, body), NOW),
            Err(Refusal::NoSecret)
        );
    }

    /// Stripe signs `<t>.<body>` with the secret as written, in hex.
    #[test]
    fn a_stripe_webhook_is_checked_over_its_time_and_body() {
        let secret = "whsec_test_secret";
        let body = r#"{"id":"evt_1","type":"payment_intent.succeeded"}"#;
        let t = (NOW / 1000).to_string();
        let good = hex(&hmac_sha256(secret.as_bytes(), &format!("{t}.{body}")));
        let header = format!("t={t},v1=deadbeef,v1={good}");
        let scheme = SignatureScheme::Stripe;
        let signed = request(&[("Stripe-Signature", &header)], body);
        assert_eq!(check(&scheme, &[secret], &signed, NOW), Ok(Some(NOW)));
        assert_eq!(
            check(&scheme, &["whsec_other"], &signed, NOW),
            Err(Refusal::Mismatch)
        );
        assert_eq!(
            check(&scheme, &[secret], &signed, NOW + 301_000),
            Err(Refusal::Stale)
        );
        let untimed = request(&[("Stripe-Signature", &format!("v1={good}"))], body);
        assert_eq!(
            check(&scheme, &[secret], &untimed, NOW),
            Err(Refusal::Unsigned)
        );
    }

    /// An HMAC in a named header, as hex with `sha256=` (GitHub's) or as
    /// Base64 (Shopify's); basic authentication against `user:password`.
    #[test]
    fn an_hmac_in_a_header_and_basic_authentication_are_checked() {
        let secret = "s3cret";
        let body = r#"{"id":1}"#;
        let mac = hmac_sha256(secret.as_bytes(), body);
        let scheme = SignatureScheme::HmacSha256 {
            header: "X-Signature".into(),
        };
        let as_hex = format!("sha256={}", hex(&mac));
        let as_base64 = jadpo_value::encoding::to_base64(&mac);
        for value in [as_hex.as_str(), as_base64.as_str(), &hex(&mac)] {
            let signed = request(&[("x-signature", value)], body);
            assert_eq!(check(&scheme, &[secret], &signed, NOW), Ok(None), "{value}");
        }
        assert_eq!(
            check(
                &scheme,
                &[secret],
                &request(&[("x-signature", &as_hex)], "{}"),
                NOW
            ),
            Err(Refusal::Mismatch)
        );
        assert_eq!(
            check(&scheme, &[secret], &request(&[], body), NOW),
            Err(Refusal::Unsigned)
        );
        let basic = SignatureScheme::Basic;
        let given = format!(
            "Basic {}",
            jadpo_value::encoding::to_base64(b"postmark:hunter2")
        );
        assert_eq!(
            check(
                &basic,
                &["postmark:hunter2"],
                &request(&[("Authorization", &given)], body),
                NOW
            ),
            Ok(None)
        );
        assert_eq!(
            check(
                &basic,
                &["postmark:other"],
                &request(&[("Authorization", &given)], body),
                NOW
            ),
            Err(Refusal::Mismatch)
        );
        assert_eq!(
            check(&basic, &["postmark:hunter2"], &request(&[], body), NOW),
            Err(Refusal::Unsigned)
        );
    }

    /// Twilio's own example from its security documentation and its
    /// libraries' tests: the auth token `12345`, the URL and form below,
    /// and the signature they give.
    #[test]
    fn a_twilio_webhook_is_checked_as_twilios_example_signs_one() {
        let body = "CallSid=CA1234567890ABCDE&Caller=%2B14158675309&Digits=1234&From=%2B14158675309&To=%2B18005551212";
        let signed = |signature: &str, host: &str| Request {
            method: "POST".into(),
            path: "/myapp.php".into(),
            query: Some("foo=1&bar=2".into()),
            headers: vec![
                ("Host".into(), host.into()),
                ("X-Twilio-Signature".into(), signature.into()),
            ],
            body: body.as_bytes().to_vec(),
            ..Request::default()
        };
        let scheme = SignatureScheme::Twilio;
        let good = "RSOYDt4T1cUTdK1PDd93/VVr8B8=";
        assert_eq!(
            check(&scheme, &["12345"], &signed(good, "mycompany.com"), NOW),
            Ok(None)
        );
        // With the port written, as a proxy may pass it on.
        assert_eq!(
            check(&scheme, &["12345"], &signed(good, "mycompany.com:443"), NOW),
            Ok(None)
        );
        assert_eq!(
            check(&scheme, &["54321"], &signed(good, "mycompany.com"), NOW),
            Err(Refusal::Mismatch)
        );
        assert_eq!(
            check(&scheme, &["12345"], &signed(good, "evil.example"), NOW),
            Err(Refusal::Mismatch)
        );
        let mut changed = signed(good, "mycompany.com");
        changed.body = body.replace("1234&", "9999&").into_bytes();
        assert_eq!(
            check(&scheme, &["12345"], &changed, NOW),
            Err(Refusal::Mismatch)
        );
        let mut unsigned = signed(good, "mycompany.com");
        unsigned
            .headers
            .retain(|(name, _)| name != "X-Twilio-Signature");
        assert_eq!(
            check(&scheme, &["12345"], &unsigned, NOW),
            Err(Refusal::Unsigned)
        );
    }

    /// Slack's own example from "Verifying requests from Slack": its
    /// signing secret, time, body and signature.
    #[test]
    fn a_slack_webhook_is_checked_as_slacks_example_signs_one() {
        let body = "token=xyzz0WbapA4vBCDEFasx0q6G&team_id=T1DC2JH3J&team_domain=testteamnow&channel_id=G8PSS9T3V&channel_name=foobar&user_id=U2CERLKJA&user_name=roadrunner&command=%2Fwebhook-collect&text=&response_url=https%3A%2F%2Fhooks.slack.com%2Fcommands%2FT1DC2JH3J%2F397700885554%2F96rGlfmibIGlgcZRskXaIFfN&trigger_id=398738663015.47445629121.803a0bc887a14d10d2c447fce8b6703c";
        let secret = "8f742231b10e8888abcd99yyyzzz85a5";
        let headers = [
            ("X-Slack-Request-Timestamp", "1531420618"),
            (
                "X-Slack-Signature",
                "v0=a2114d57b48eac39b9ad189dd8316235a7b4a8d21a10bd27519666489c69b503",
            ),
        ];
        let at = 1_531_420_618_000;
        let scheme = SignatureScheme::Slack;
        assert_eq!(
            check(&scheme, &[secret], &request(&headers, body), at + 60_000),
            Ok(Some(at))
        );
        assert_eq!(
            check(&scheme, &[secret], &request(&headers, body), at + 360_000),
            Err(Refusal::Stale)
        );
        assert_eq!(
            check(&scheme, &["other"], &request(&headers, body), at),
            Err(Refusal::Mismatch)
        );
        assert_eq!(
            check(&scheme, &[secret], &request(&headers[..1], body), at),
            Err(Refusal::Unsigned)
        );
    }

    /// A form body is read as text members, a name sent twice a list.
    #[test]
    fn a_form_body_is_read_as_its_members() {
        assert_eq!(
            form_json(b"From=%2B4420&Body=Hello+there&Media=a&Media=b"),
            Some(serde_json::json!({
                "From": "+4420",
                "Body": "Hello there",
                "Media": ["a", "b"],
            }))
        );
        assert_eq!(form_json(b"Bad=%FF"), None);
    }

    fn hmac_sha256(key: &[u8], message: &str) -> Vec<u8> {
        use hmac_check::*;
        mac(key, message.as_bytes())
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// HMAC-SHA-256 written out from RFC 2104 over `jadpo-value`'s
    /// SHA-256, so the tests don't check the runtime with itself.
    mod hmac_check {
        pub fn mac(key: &[u8], message: &[u8]) -> Vec<u8> {
            let sha = |data: &[u8]| jadpo_value::hash::sha256(data).to_vec();
            let mut block = if key.len() > 64 {
                sha(key)
            } else {
                key.to_vec()
            };
            block.resize(64, 0);
            let inner: Vec<u8> = block.iter().map(|b| b ^ 0x36).collect();
            let outer: Vec<u8> = block.iter().map(|b| b ^ 0x5c).collect();
            let first = sha(&[inner, message.to_vec()].concat());
            sha(&[outer, first].concat())
        }
    }
}
