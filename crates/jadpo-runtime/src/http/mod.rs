//! The runtime's HTTP component (area 7; docs/compiler-runtime.md §2.6):
//! a request in, a response out, the same for every host and executor. It
//! routes by the route table, decodes the placeholders and the one input
//! with the route's codecs, runs the action (through the executor, inside
//! the runtime's action boundary) or the query (through the store's entry
//! point), and answers with the output, or with the failure envelope and
//! the status its kind sets (area 4), always with a `request_id` in the
//! body and `x-request-id`. Compiled code reads and writes a route with
//! its typed entry ([`Entry`], chunk 8.1p) where the interpreter walks its
//! codecs; everything around them is the same code ([`read_request`]).
//!
//! The runtime answers `HEAD` (as `GET`, without the body) and `OPTIONS`
//! (with `Allow`) itself, and every route has the default rate limits,
//! which the host counts ([`crate::Host::rate_limit`]). A deployable, and
//! `jadpo dev`, hand each request to [`serve_request`], which makes its
//! context and request id and answers it; a route test's goes through
//! [`route_test`]. Its request guarantees (chunk 4.2):
//!
//! - a body over 1 MB is refused with 413 `request_too_large`;
//! - every route has a deadline: when it passes, whatever is running is
//!   dropped, its transaction with it, so nothing commits, and the answer
//!   is 504 `deadline_exceeded`. Work that never waits checks it as it runs
//!   ([`crate::Ctx::out_of_time`]), and a body that finishes past it is
//!   answered the same way (chunk 4.9). An action's commit happens after
//!   its body, outside the deadline ([`crate::Ctx::hold`]), so a request
//!   that committed is never answered 504; a concurrent change found as it
//!   commits runs the action again;
//! - a header the runtime can't read as printable ASCII is 400, a body is
//!   read only as JSON, and a query string or body a route doesn't read is
//!   422 `invalid_input` (chunk 4.9);
//! - `GET /health/live` and `GET /health/ready` ([`health`]);
//! - `Idempotency-Key` on every route but a `GET` ([`idempotency`]).
//!
//! Sign-in (chunk 4.3, [`identity`]): a route that needs a signed-in
//! caller authenticates the request before its input is read, and what it
//! runs gets the caller; a signed-in caller's requests count against its
//! own rate limit and keep their idempotency keys apart. Policy (chunk
//! 4.4) is compiled into the statements what a route runs sends, and an
//! action's gate is checked by the action boundary. Whoever signed in, or
//! no one, a request is never the system (chunk 6.1a): the caller only the
//! runner gives a schedule's run or a subscriber's delivery, which
//! `Access.system` grants.

pub mod address;
pub mod external_token;
#[cfg(test)]
mod guarantees;
mod health;
pub mod idempotency;
pub mod identity;
pub mod magic_links;
pub(crate) mod passkeys;
pub(crate) mod sign_in_events;
pub mod signed;
pub mod test_passkeys;
mod trie;

use std::cell::Cell;
use std::collections::BTreeMap;

pub use jadpo_std::http::BODY_LIMIT;
use jadpo_std::http::{
    ADDRESS_LIMIT, ArgRoot, CALLER_LIMIT, Codec, CodecId, RoutePlan, RouteTable, Run, Segment,
};
use jadpo_std::types::FailurePlan;
use jadpo_std::{HttpMethod, Idempotency};
use jadpo_value::Value;
use jadpo_value::json::reading::{At, InputReader, Reading, TextReader, Writer};
use jadpo_value::json::{Codecs, Doc, Members, Path, Problem, write_string};

use crate::race::{First, race};
use crate::ratelimit::Allowed;
use crate::{Clock, Ctx, Env, Executor, Failure, Fault, Frame, Host, Outcome, StoreOp, Stored};
use idempotency::{Claim, Guard};
pub use trie::{Found, Methods, Routes};

/// A request, as a host hands it over.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    /// The path as sent, percent-encoded, without the query string.
    pub path: String,
    /// The query string, without its `?`.
    pub query: Option<String>,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// The address of the connection the request came on, which the host
    /// gives; the address it counts as is chosen from it and the headers
    /// by [`address::ClientAddress`] (area 10, "Client addresses").
    pub address: Option<String>,
    /// The host stopped reading the body once it passed [`BODY_LIMIT`]:
    /// `body` holds what it read, and the request is refused with 413.
    pub too_large: bool,
}

impl Request {
    /// The first header called `name`, compared without case.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header, _)| header.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// A response, as a host sends it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// The failure code it answers with, for the host's log line; never
    /// sent beyond the body.
    pub code: Option<String>,
    /// The route it answered by: `POST /todos`, shared with the trie,
    /// which made it once (chunk 8.1k).
    pub route: Option<std::sync::Arc<str>>,
    /// The fault behind a 500, for the host's log and a route test's
    /// report; never sent.
    pub fault: Option<Box<crate::Fault>>,
    /// Set while the component answers when the request has been counted
    /// against a rate limit; never sent.
    pub counted: bool,
    /// The address the request counted as (area 10, "Client addresses"),
    /// for the host's log line; never sent.
    pub client: Option<String>,
    /// The work what it ran left for after it is sent: what its action
    /// emitted once it committed (chunk 6.1a). The host hands it to the
    /// runner ([`crate::runner::after_response`]); never sent.
    pub deferred: Vec<crate::runner::Deferred>,
    /// The calls what it ran made outside the process, as the capability
    /// seam logged them, without their values (chunk 7.0c): what `jadpo
    /// calls` lists by request (chunk 7.2); never sent.
    pub calls: Vec<crate::capability::Logged>,
    /// The steps its action reached, when it keeps a record of several
    /// changing calls: what `jadpo dev` shows under its line (chunk
    /// 7.5f); never sent.
    pub steps: Vec<crate::steps::RunSteps>,
    /// Why a sign-in was refused, for the host's log line (area 8,
    /// "Events"; chunk 7.3b4); never sent.
    pub refused: Option<crate::ctx::SignInRefused>,
    /// A model's answer to send after the head, event by event, in place
    /// of `body` (area 7, "Streaming a model's answer"; chunk 5.5v6).
    pub stream: Option<crate::stream::EventStream>,
}

impl Response {
    /// The first header called `name`, compared without case.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header, _)| header.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// What the component needs from a prepared program besides its executor:
/// the route table, the codecs with their validators and shapes compiled,
/// and the failure plans a missing record's failure is raised from.
#[derive(Clone, Copy, Debug)]
pub struct App<'a> {
    pub table: &'a RouteTable,
    /// The trie of the table's routes, health's paths and the program's
    /// webhooks, built once as the plan is loaded (chunk 8.1k).
    pub routes: &'a Routes,
    pub codecs: Codecs<'a>,
    pub failures: &'a [FailurePlan],
    /// The codecs a JSON-held column a route's query reads is read with
    /// (chunk 5.5w2b): the plan's JSON table's.
    pub json: crate::JsonCodecs<'a>,
    /// Each route's typed entry, by its number, where compiled code gives
    /// one (chunk 8.1p); the interpreter gives none, and reads and writes
    /// every route by the codec walk.
    pub entries: Option<Entries>,
}

impl App<'_> {
    /// Route `number`'s typed entry, where the program gives one.
    pub fn entry(&self, number: u32) -> Option<&'static Entry> {
        self.entries.and_then(|entries| entries(number))
    }
}

/// A route's typed entry (chunk 8.1p; docs/compiler-runtime.md §2.5, "A
/// typed per-route entry"): compiled code's readers of its placeholders
/// and its input and its output's writer, each generated from its type's
/// plan, which the component reads and writes the route with in place of
/// the codec walk. Everything around them is the component's own.
#[derive(Clone, Copy, Debug)]
pub struct Entry {
    /// Each placeholder's reader, in the path's order.
    pub params: &'static [TextReader],
    /// The input's reader; `None` when it is read by the codec walk.
    pub input: Option<InputReader>,
    /// The output's writer; `None` when it is written by the codec walk.
    pub output: Option<Writer>,
}

/// A program's routes' entries, by route number.
pub type Entries = fn(u32) -> Option<&'static Entry>;

/// The failures the runtime answers with itself (area 7, "How failures
/// reach the client").
const INVALID_REQUEST: (u16, &str, &str) = (400, "invalid_request", "Request could not be read.");
const INVALID_INPUT: (u16, &str, &str) = (422, "invalid_input", "Request input is invalid.");
const HEADER_UNREADABLE: (u16, &str, &str) = (
    400,
    "invalid_request",
    "A header's value isn't printable ASCII.",
);
const BODY_NOT_JSON: (u16, &str, &str) = (
    400,
    "invalid_request",
    "A request body is JSON, sent with `Content-Type: application/json`.",
);
const ROUTE_NOT_FOUND: (u16, &str, &str) =
    (404, "route_not_found", "No route matches this request.");
pub(crate) const METHOD_NOT_ALLOWED: (u16, &str, &str) = (
    405,
    "method_not_allowed",
    "This path doesn't answer this method.",
);
const RATE_LIMITED: (u16, &str, &str) = (429, "rate_limited", "Too many requests.");
pub(crate) const REQUEST_TOO_LARGE: (u16, &str, &str) =
    (413, "request_too_large", "The request body is over 1 MB.");
const DEADLINE_EXCEEDED: (u16, &str, &str) = (
    504,
    "deadline_exceeded",
    "The request ran out of time, and nothing it did was kept.",
);
const KEY_REQUIRED: (u16, &str, &str) = (
    400,
    "invalid_request",
    "This request needs an Idempotency-Key header.",
);
const KEY_INVALID: (u16, &str, &str) = (
    400,
    "invalid_request",
    "An Idempotency-Key is 1 to 255 printable ASCII characters.",
);
const KEY_REUSED: (u16, &str, &str) = (
    422,
    "idempotency_key_reused",
    "This Idempotency-Key was already used with a different request.",
);
const KEY_IN_FLIGHT: (u16, &str, &str) = (
    409,
    "conflict",
    "A request with this Idempotency-Key is still running.",
);
pub(crate) const UNAVAILABLE: (u16, &str, &str) = (
    503,
    "unavailable",
    "A dependency is temporarily unavailable.",
);
pub(crate) const INTERNAL_FAULT: (u16, &str, &str) = (
    500,
    "internal_fault",
    "The server failed to answer the request.",
);

/// The message a failure without one of its own gets, by kind (area 4).
fn kind_message(kind: Option<&str>) -> &'static str {
    match kind {
        Some("invalid_value") | Some("rejected") => "The request was refused.",
        Some("not_found") | Some("not_visible") => "Not found.",
        Some("not_permitted") => "Not permitted.",
        Some("unauthenticated") => "Sign in first.",
        Some("conflict") => "The request conflicts with the current state.",
        Some("rate_limited") => "Too many requests.",
        Some("unavailable") => "A dependency is temporarily unavailable.",
        Some("timed_out") => "A dependency or deadline ran out of time.",
        _ => "The server failed to answer the request.",
    }
}

/// A new request id: `req_` and 16 hexadecimal digits, from the host's
/// randomness, or from the invocation's seed in a test, so a test's ids
/// reproduce.
pub fn request_id(ctx: &Ctx<'_>, sequence: u64) -> String {
    new_request_id(ctx.host(), ctx.env().seed, sequence)
}

/// [`request_id`], from `host`'s randomness or `seed`.
fn new_request_id(host: &dyn Host, seed: Option<u64>, sequence: u64) -> String {
    let mut bytes = [0u8; 8];
    let n = if host.random(&mut bytes) {
        u64::from_be_bytes(bytes)
    } else {
        let mut z = seed
            .unwrap_or(0)
            .wrapping_add(sequence.wrapping_mul(0x9E37_79B9_7F4A_7C15))
            .wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    };
    format!("req_{n:016x}")
}

/// What a host answers itself, before the HTTP component runs, while it
/// can't serve: 503 `unavailable` with a new request id, from `host`'s
/// randomness. A Workers isolate whose start failed, or that couldn't
/// check its stores' migrations yet, answers it (chunk 8.2d).
pub fn unavailable(host: &dyn Host) -> Response {
    unserved(host, UNAVAILABLE)
}

/// What a host answers itself for a request whose body it couldn't read:
/// 400 `invalid_request` with a new request id (chunk 8.2d).
pub fn unreadable(host: &dyn Host) -> Response {
    unserved(host, INVALID_REQUEST)
}

fn unserved(host: &dyn Host, failure: (u16, &str, &str)) -> Response {
    let id = new_request_id(host, None, 0);
    let mut response = runtime_failure(failure, &id, None);
    response.headers.push(("x-request-id".to_string(), id));
    response
}

/// What the HTTP component keeps between the requests one worker answers
/// (natively a worker thread, on Workers an isolate): how many it has
/// answered, which numbers request ids on a host with no randomness, and
/// its last readiness check, reused for a second.
#[derive(Debug, Default)]
pub struct Worker {
    answered: Cell<u64>,
    ready: health::Readiness,
}

/// Answers `request` as a deployable does: in a context of its own on
/// `host`, under the host's clock and no seed, with a new request id,
/// running what a route runs with `executor`. `jadpo dev` and every
/// deployable call this, so everything around a request but reading and
/// writing its bytes is the runtime's. The response carries its request id
/// in `x-request-id`, and the fault behind a 500 for the host's log.
///
/// A host without a clock is refused: every request, health's included, is
/// a 500 whose fault says so, and nothing runs, rather than answering as
/// though it were 1970 (chunk 5.8a). A deployable checks [`Host::now_ms`]
/// before it listens, so this is the last line, not the first.
pub async fn serve_request(
    app: &App<'_>,
    executor: &dyn Executor,
    host: &dyn Host,
    worker: &Worker,
    request: &Request,
) -> Response {
    let answered = worker.answered.get().wrapping_add(1);
    worker.answered.set(answered);
    let Some(now) = host.now_ms() else {
        let id = new_request_id(host, None, answered);
        let mut response = runtime_failure(INTERNAL_FAULT, &id, None);
        response.fault = Some(Box::new(crate::Fault {
            kind: crate::FaultKind::Defect("the host has no clock".into()),
            frames: Vec::new(),
        }));
        response.headers.push(("x-request-id".to_string(), id));
        if request.method == "HEAD" {
            response.body.clear();
        }
        return response;
    };
    let env = Env {
        seed: None,
        clock: Clock::fixed(now),
    };
    let mut ctx = Ctx::of(executor, host, env);
    let id = request_id(&ctx, answered);
    let mut response =
        answer_request(app, executor, &mut ctx, request, &id, Some(&worker.ready)).await;
    response.deferred = ctx.take_deferred();
    response.calls = ctx.take_calls();
    response.steps = ctx.take_steps(0);
    response
}

/// Answers `request` with `app`'s routes, running what a route runs with
/// `executor` against `ctx`, whose stack is empty: what a route test's
/// request goes through, in the test's own context, made from `executor`
/// ([`Ctx::of`]), which reaches what the program started with.
pub async fn handle(
    app: &App<'_>,
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    request: &Request,
    request_id: &str,
) -> Response {
    answer_request(app, executor, ctx, request, request_id, None).await
}

async fn answer_request(
    app: &App<'_>,
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    request: &Request,
    request_id: &str,
    ready: Option<&health::Readiness>,
) -> Response {
    // A request is never the system, whatever context it is answered in.
    ctx.background_mut().set_system(None);
    // Its client's address, chosen as the program started choosing it.
    let client = ctx.started().client().of(request);
    ctx.set_client(client);
    // The cookie binding a `same_browser:` magic link to this browser, for
    // `magic_links.redeem` in the application's own route (chunk 7.3b3).
    ctx.set_sign_in_cookie(
        app.table
            .identity
            .as_ref()
            .filter(|plan| plan.magic_links.as_ref().is_some_and(|m| m.same_browser))
            .and_then(magic_links::binding_cookie)
            .and_then(|cookie| magic_links::cookie_value(request, &cookie))
            .map(str::to_string),
    );
    // Its trace continues the caller's `traceparent`, or starts one, so
    // what it emits carries it to each subscriber (area 12, question 17).
    let trace = match request
        .header("traceparent")
        .and_then(crate::outbox::Trace::parse)
    {
        Some(parent) => ctx.new_span(parent),
        None => ctx.new_trace(),
    };
    ctx.set_trace(Some(trace));
    let mut response = respond(app, executor, ctx, request, request_id, ready).await;
    response.client = ctx.client().map(str::to_string);
    response.refused = ctx.take_sign_in_refused();
    response
        .headers
        .push(("x-request-id".to_string(), request_id.to_string()));
    if request.method == "HEAD" {
        response.body.clear();
    }
    response
}

async fn respond(
    app: &App<'_>,
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    request: &Request,
    request_id: &str,
    ready: Option<&health::Readiness>,
) -> Response {
    // What the path is, found once in the trie (chunk 8.1k).
    let found = app.routes.find(&request.method, &request.path);
    if let Found::Health { ready: readiness } = found {
        let mut response =
            health::answer(app.table, ctx, request, request_id, readiness, ready).await;
        response.route = Some(app.routes.health_label(readiness));
        return response;
    }
    // A webhook's route is the runtime's own (area 11, "Receiving from
    // outside"; chunk 7.7), counted against its address before anything
    // in it is read, whatever credential it carries.
    if let Found::Webhook(at) = found {
        let started = ctx.started();
        let webhooks = executor.plan().webhooks(&started);
        if let Some(webhooks) = &webhooks
            && let Some(plan) = webhooks.plan.get(at as usize)
        {
            if let Some(response) = by_address(ctx, request_id).await {
                return response;
            }
            if !headers_readable(request) {
                return runtime_failure(HEADER_UNREADABLE, request_id, None);
            }
            let mut response = crate::webhooks::answer(
                app,
                executor,
                webhooks,
                (at as usize, plan),
                ctx,
                request,
                request_id,
            )
            .await;
            response.route = app.routes.webhook_label(at);
            return response;
        }
    }
    // A request with a credential counts against its caller once signed
    // in, or against its address if it isn't; every other request against
    // its address, here.
    let deferred = identity::carries_credential(app.table, request);
    if !deferred && let Some(response) = by_address(ctx, request_id).await {
        return response;
    }
    let mut response = route_request(app, executor, ctx, request, request_id, found).await;
    if deferred
        && !response.counted
        && let Some(limited) = by_address(ctx, request_id).await
    {
        response = limited;
    }
    response.counted = false;
    response
}

/// 429 when `request`'s address has used its allowance, as the host
/// counts it: an IPv6 client by its /64 ([`address::counted_as`]), on
/// every route this guards, the magic-link and passkey routes among them.
async fn by_address(ctx: &Ctx<'_>, request_id: &str) -> Option<Response> {
    let key = format!(
        "address {}",
        ctx.client()
            .map_or_else(|| "unknown".to_string(), address::counted_as)
    );
    match ctx.host().rate_limit(&key, ADDRESS_LIMIT, ctx.now()).await {
        Allowed::Yes => None,
        Allowed::No { retry_after } => Some(rate_limited(request_id, retry_after)),
    }
}

fn rate_limited(request_id: &str, retry_after: u64) -> Response {
    let mut response = runtime_failure(RATE_LIMITED, request_id, None);
    response
        .headers
        .push(("retry-after".to_string(), retry_after.to_string()));
    response.counted = true;
    response
}

async fn route_request(
    app: &App<'_>,
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    request: &Request,
    request_id: &str,
    found: Found,
) -> Response {
    if found == Found::Unreadable {
        return runtime_failure(INVALID_REQUEST, request_id, None);
    }
    if !headers_readable(request) {
        return runtime_failure(HEADER_UNREADABLE, request_id, None);
    }
    if request.too_large || request.body.len() > BODY_LIMIT {
        return runtime_failure(REQUEST_TOO_LARGE, request_id, None);
    }
    // A fixed segment beats a placeholder: of the routes for this method,
    // the one with fixed text earliest (the trie's walk).
    let Found::Route { route, allow } = found else {
        return runtime_failure(ROUTE_NOT_FOUND, request_id, None);
    };
    if request.method == "OPTIONS" {
        return Response {
            status: 204,
            headers: vec![("allow".to_string(), allow.allow())],
            ..Response::default()
        };
    }
    let Some((number, route)) =
        route.and_then(|number| Some((number, app.table.routes.get(number as usize)?)))
    else {
        let mut response = runtime_failure(METHOD_NOT_ALLOWED, request_id, None);
        response.headers.push(("allow".to_string(), allow.allow()));
        return response;
    };
    // Work that never waits checks the deadline as it runs.
    ctx.set_deadline(Some(u64::from(route.deadline_ms)));
    let mut response = run_route(app, executor, ctx, request, request_id, (number, route)).await;
    ctx.set_deadline(None);
    response.route = app.routes.label(number);
    response
}

/// Whether every header's value is printable ASCII, spaces and tabs
/// included: a value a host couldn't read as such reaches the runtime as
/// it came, and is refused rather than dropped, so a credential or an
/// `Idempotency-Key` is never silently left out.
fn headers_readable(request: &Request) -> bool {
    request.headers.iter().all(|(_, value)| {
        value
            .bytes()
            .all(|byte| byte == b'\t' || (b' '..=b'~').contains(&byte))
    })
}

/// Whether the request says its body is JSON: `application/json`, or
/// another `application/…+json` type, with any parameters. Only such a
/// body is read, so a form a browser posts across sites never reaches an
/// action.
fn json_content(request: &Request) -> bool {
    request.header("content-type").is_some_and(|value| {
        let essence = value
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        essence == "application/json"
            || (essence.starts_with("application/") && essence.ends_with("+json"))
    })
}

/// A path's segments, percent-decoded; `None` when one isn't valid UTF-8.
#[cfg(test)]
fn split_path(path: &str) -> Option<Vec<String>> {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| percent_decode(segment, false))
        .collect()
}

/// Segment `raw` of a path, percent-decoded: borrowed when it has no
/// escapes. The trie has refused a path with one that isn't text, which
/// reads here as empty.
pub(crate) fn decoded(raw: &str) -> std::borrow::Cow<'_, str> {
    if raw.contains('%') {
        std::borrow::Cow::Owned(percent_decode(raw, false).unwrap_or_default())
    } else {
        std::borrow::Cow::Borrowed(raw)
    }
}

/// `text` with its `%XX` escapes decoded, and `+` read as a space in a
/// query string; `None` when an escape is malformed or the bytes aren't
/// UTF-8.
pub(crate) fn percent_decode(text: &str, plus: bool) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        match byte {
            b'%' => {
                let hex = text.get(at + 1..at + 3)?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                at += 3;
            }
            b'+' if plus => {
                out.push(b' ');
                at += 1;
            }
            other => {
                out.push(other);
                at += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// A query string's values, by key, in the order they came; `None` when
/// it can't be read.
pub(crate) fn query_values(query: &str) -> Option<BTreeMap<String, Vec<String>>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        out.entry(percent_decode(key, true)?)
            .or_default()
            .push(percent_decode(value, true)?);
    }
    Some(out)
}

/// What reading a route's request gave (chunk 8.1p).
#[derive(Debug)]
pub enum Read {
    /// Answered with this failure before its values are read: a query
    /// string or a body the runtime can't read, or a body not sent as JSON.
    Refused((u16, &'static str, &'static str)),
    /// The problems found reading its values, in order (`invalid_input`).
    Problems(Vec<Problem>),
    /// Each placeholder's value, in the path's order, and its input.
    Values {
        params: Vec<Option<Value>>,
        input: Option<Value>,
    },
}

/// Route `number`'s placeholders and input, read from `request`: by the
/// route's typed entry where the program gives one (chunk 8.1p), and by
/// the codec walk otherwise. A query string or a body the route doesn't
/// read is refused here too, as an unknown field is, so either way a
/// request is refused for the same reasons, in the same order.
pub fn read_request(app: &App<'_>, number: u32, request: &Request) -> Read {
    let Some(route) = app.table.routes.get(number as usize) else {
        return Read::Values {
            params: Vec::new(),
            input: None,
        };
    };
    let entry = app.entry(number);
    let mut reading = Reading::new(app.codecs, Vec::new());
    // Placeholders, each decoded once and checked.
    let mut params = Vec::new();
    for (segment, written) in route.segments.iter().zip(trie::segments(&request.path)) {
        if let Segment::Param { name, codec } = segment {
            let written = decoded(written);
            let value = match entry.and_then(|entry| entry.params.get(params.len())) {
                Some(read) => read(&mut reading, &written, &At::Member(&At::Top, name)),
                None => app.codecs.read_text(
                    *codec,
                    &written,
                    &Path::field(name),
                    reading.problems_mut(),
                ),
            };
            params.push(value);
        }
    }
    // A query string or a body the route doesn't read is refused, as an
    // unknown field is: the query's keys each, the body as a whole.
    let reads_query = route.input.as_ref().is_some_and(|plan| plan.query);
    let reads_body = route.input.as_ref().is_some_and(|plan| !plan.query);
    if !reads_query && let Some(query) = request.query.as_deref() {
        let Some(values) = query_values(query) else {
            return Read::Refused(INVALID_REQUEST);
        };
        for key in values.into_keys() {
            reading.problems_mut().push(Problem {
                field: key,
                rule: "unknown_field".to_string(),
            });
        }
    }
    if !reads_body && !request.body.iter().all(u8::is_ascii_whitespace) {
        reading.problems_mut().push(Problem {
            field: String::new(),
            rule: "unknown_field".to_string(),
        });
    }
    // The input.
    let input = match &route.input {
        None => None,
        Some(plan) => {
            let read = match entry.and_then(|entry| entry.input) {
                Some(read) => typed_input(plan, request, &mut reading, read),
                None => walked_input(app, plan, request, reading.problems_mut()),
            };
            match read {
                Ok(input) => input,
                Err(refused) => return Read::Refused(refused),
            }
        }
    };
    let problems = reading.into_problems();
    if !problems.is_empty() {
        return Read::Problems(problems);
    }
    Read::Values { params, input }
}

/// A route's input read by the codec walk: its query string's values or
/// its body as a `serde_json` object, its header fields put in, then read
/// by its codec.
fn walked_input(
    app: &App<'_>,
    plan: &jadpo_std::http::InputPlan,
    request: &Request,
    problems: &mut Vec<Problem>,
) -> Result<Option<Value>, (u16, &'static str, &'static str)> {
    let mut object = serde_json::Map::new();
    // The body's numbers as written, so a vector's items are read from
    // their own text (chunk 5.8c).
    let mut numbers = jadpo_value::json::Numbers::default();
    if plan.query {
        let Some(values) = query_values(request.query.as_deref().unwrap_or("")) else {
            return Err(INVALID_REQUEST);
        };
        let lists = list_fields(app, plan.codec);
        for (key, values) in values {
            let value = if lists.contains(&key) {
                serde_json::Value::Array(
                    values.into_iter().map(serde_json::Value::String).collect(),
                )
            } else if values.len() == 1 {
                values
                    .into_iter()
                    .next()
                    .map_or(serde_json::Value::Null, serde_json::Value::String)
            } else {
                // A single value given twice: reported as the wrong kind.
                serde_json::Value::Array(
                    values.into_iter().map(serde_json::Value::String).collect(),
                )
            };
            object.insert(key, value);
        }
    } else {
        if !json_content(request) {
            return Err(BODY_NOT_JSON);
        }
        if request.body.iter().all(u8::is_ascii_whitespace) {
            return Err(INVALID_REQUEST);
        }
        // A member named twice makes the body unreadable, as `json.read`
        // reads it (chunk 5.8c).
        match jadpo_value::json::parse(&request.body) {
            Some((serde_json::Value::Object(entries), written)) => {
                object = entries;
                numbers = written;
            }
            Some(_) => {
                problems.push(Problem {
                    field: String::new(),
                    rule: "type".to_string(),
                });
            }
            None => return Err(INVALID_REQUEST),
        }
    }
    // Fields read from headers never come in the body or the query.
    for (field, header) in &plan.headers {
        if object.contains_key(field) {
            problems.push(Problem {
                field: field.clone(),
                rule: "unknown_field".to_string(),
            });
            object.remove(field);
        }
        if let Some(value) = request.header(header) {
            object.insert(field.clone(), serde_json::Value::String(value.to_string()));
        }
    }
    let before = problems.len();
    let value = app.codecs.read(
        plan.codec,
        &serde_json::Value::Object(object),
        &Path::default().numbered(&numbers),
        problems,
    );
    Ok(if problems.len() > before { None } else { value })
}

/// A route's input read by its typed entry's reader (chunk 8.1p): its
/// query string's values, or its body read in place ([`Doc`]), as the
/// members of an object with its header fields in place of any member of
/// their names, refused and reported as [`walked_input`] refuses and
/// reports them.
fn typed_input(
    plan: &jadpo_std::http::InputPlan,
    request: &Request,
    reading: &mut Reading<'_>,
    read: InputReader,
) -> Result<Option<Value>, (u16, &'static str, &'static str)> {
    let values;
    let doc;
    let members = if plan.query {
        values = query_values(request.query.as_deref().unwrap_or("")).ok_or(INVALID_REQUEST)?;
        Members::query(&values)
    } else {
        if !json_content(request) {
            return Err(BODY_NOT_JSON);
        }
        if request.body.iter().all(u8::is_ascii_whitespace) {
            return Err(INVALID_REQUEST);
        }
        // A member named twice makes the body unreadable, as `json.read`
        // reads it (chunk 5.8c).
        doc = Doc::parse(&request.body).ok_or(INVALID_REQUEST)?;
        match doc.root().members() {
            Some(members) => members,
            None => {
                reading.problems_mut().push(Problem {
                    field: String::new(),
                    rule: "type".to_string(),
                });
                Members::empty()
            }
        }
    };
    // Fields read from headers never come in the body or the query.
    let mut headers = Vec::with_capacity(plan.headers.len());
    for (field, header) in &plan.headers {
        if members.contains(field) {
            reading.problems_mut().push(Problem {
                field: field.clone(),
                rule: "unknown_field".to_string(),
            });
        }
        headers.push((field.as_str(), request.header(header)));
    }
    let members = members.with_headers(&headers);
    let before = reading.count();
    let value = read(reading, &members);
    Ok(if reading.count() > before {
        None
    } else {
        value
    })
}

async fn run_route(
    app: &App<'_>,
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    request: &Request,
    request_id: &str,
    (number, route): (u32, &RoutePlan),
) -> Response {
    // The deadline counts from here; the timer is made at once.
    let host = ctx.host();
    let mut deadline = host.sleep(u64::from(route.deadline_ms));
    // A stream the route sends ends by it too (chunk 5.5v6).
    let ends_at = host
        .monotonic_ms()
        .map(|now| now.saturating_add(u64::from(route.deadline_ms)));
    // So does how long a route waits for its follow-on work (chunk 6.3).
    let within = route
        .settle
        .as_ref()
        .map(|settle| host.sleep(u64::from(settle.within_ms)));
    // Who is calling, before the input is read or anything runs (area 8).
    let mut reissued = None;
    let mut exchanged = None;
    let caller = if route.authentication.needs_caller() {
        let checked = identity::authenticate(app, ctx, request, route);
        match race(checked, &mut deadline).await {
            First::Left(Ok(admitted)) => {
                reissued = admitted.reissued;
                exchanged = admitted.exchanged;
                Some(admitted.caller)
            }
            First::Left(Err(refused)) => {
                if refused.credential
                    && let Some(limited) = by_address(ctx, request_id).await
                {
                    return limited;
                }
                let mut response = identity::refusal(app.table, &refused, request_id);
                response.counted = refused.credential;
                return response;
            }
            First::Right(()) => return runtime_failure(DEADLINE_EXCEEDED, request_id, None),
        }
    } else {
        // A route anyone may call runs as no one, so a request with a
        // credential counts against its address before it runs, as every
        // other does.
        if identity::carries_credential(app.table, request)
            && let Some(limited) = by_address(ctx, request_id).await
        {
            return limited;
        }
        None
    };
    let counted = caller.is_none() && identity::carries_credential(app.table, request);
    if let Some(caller) = &caller
        && let Allowed::No { retry_after } = ctx
            .host()
            .rate_limit(&caller.key, CALLER_LIMIT, ctx.now())
            .await
    {
        return rate_limited(request_id, retry_after);
    }
    let scope = caller.as_ref().map_or_else(
        || idempotency::ANONYMOUS.to_string(),
        |caller| caller.key.clone(),
    );
    let mut response = match (exchanged, &route.run) {
        // The exchange runs nothing: the key's read signed its token.
        (Some(exchanged), _) => exchange(request, request_id, exchanged),
        // The runtime answers the passkey routes itself (area 8,
        // "Passkeys"), with no idempotency key: a replayed answer is never
        // answered from a stored response.
        (None, Run::Passkey(step)) => {
            let answered = passkeys::answer(
                app,
                executor,
                ctx,
                request,
                request_id,
                *step,
                caller.as_ref(),
            );
            match race(answered, &mut deadline).await {
                First::Left(response) => response,
                First::Right(()) => runtime_failure(DEADLINE_EXCEEDED, request_id, None),
            }
        }
        // The runtime answers the magic-link routes itself (area 8, "Magic
        // links"), with no idempotency key: a link or a code is used once.
        (None, Run::MagicLink(step)) => {
            let answered = magic_links::answer(app, executor, ctx, request, request_id, *step);
            match race(answered, &mut deadline).await {
                First::Left(response) => response,
                First::Right(()) => runtime_failure(DEADLINE_EXCEEDED, request_id, None),
            }
        }
        // Each API's own OpenAPI document (area 7, "OpenAPI"), as it is.
        (None, Run::OpenApi(document)) => openapi(request, request_id, document),
        (None, _) => {
            run_with_caller(
                app,
                executor,
                ctx,
                request,
                request_id,
                (number, route),
                (deadline, ends_at),
                within,
                caller,
                scope,
                reissued,
            )
            .await
        }
    };
    response.counted = response.counted || counted || ctx.caller().is_some();
    ctx.set_caller(None);
    response
}

/// `POST /auth/exchange`'s answer (area 8, "Ways to sign in"): the token
/// the key signed in with is traded for, and when it expires. Like any
/// route that takes no input, it refuses a query string or a body.
fn exchange(request: &Request, request_id: &str, (token, expires): (String, i64)) -> Response {
    if request
        .query
        .as_deref()
        .is_some_and(|query| !query.is_empty())
        || !request.body.iter().all(u8::is_ascii_whitespace)
    {
        return invalid_input(
            request_id,
            &[Problem {
                field: String::new(),
                rule: "unknown_field".to_string(),
            }],
        );
    }
    let mut body = String::from("{\"token\":");
    write_string(&token, &mut body);
    body.push_str(",\"expires_at\":");
    // An instant, as the contract sends one (chunk 5.5w3).
    let expires = jadpo_value::Value::Instant(expires);
    match jadpo_value::standard::entry_of(&expires) {
        Some(entry) => jadpo_value::standard::json::write(entry, &expires, &mut body),
        None => body.push_str("null"),
    }
    body.push('}');
    Response {
        status: 200,
        headers: vec![
            ("content-type".to_string(), "application/json".to_string()),
            ("cache-control".to_string(), "no-store".to_string()),
        ],
        body: body.into_bytes(),
        ..Response::default()
    }
}

/// `GET <prefix>/openapi.json`'s answer (area 7, "OpenAPI"): the API's
/// document. Like any route that takes no input, it refuses a query string
/// or a body.
fn openapi(request: &Request, request_id: &str, document: &str) -> Response {
    if request
        .query
        .as_deref()
        .is_some_and(|query| !query.is_empty())
        || !request.body.iter().all(u8::is_ascii_whitespace)
    {
        return invalid_input(
            request_id,
            &[Problem {
                field: String::new(),
                rule: "unknown_field".to_string(),
            }],
        );
    }
    Response {
        status: 200,
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        body: document.as_bytes().to_vec(),
        ..Response::default()
    }
}

/// What a route answers before or instead of running what it runs.
enum Early {
    Respond(Box<Response>),
    /// A repeat of a route that waits for its follow-on work: its answer
    /// query's arguments, as JSON, and the status kept for it (chunk 6.3).
    Answer {
        status: u16,
        args: String,
    },
}

/// The rest of a route's run, once its caller is known: `deadline` is the
/// route's timer, already running, and `within` its `settle:`'s.
#[allow(clippy::too_many_arguments)]
async fn run_with_caller(
    app: &App<'_>,
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    request: &Request,
    request_id: &str,
    (number, route): (u32, &RoutePlan),
    (deadline, ends_at): (crate::LocalFuture<'_, ()>, Option<u64>),
    mut within: Option<crate::LocalFuture<'_, ()>>,
    caller: Option<crate::Caller>,
    scope: String,
    reissued: Option<identity::Reissued>,
) -> Response {
    let caller_value = caller.as_ref().map(|caller| caller.value.clone());
    ctx.set_caller(caller);
    let codecs = &app.codecs;
    let (params, input) = match read_request(app, number, request) {
        Read::Refused(refusal) => return runtime_failure(refusal, request_id, None),
        Read::Problems(problems) => return invalid_input(request_id, &problems),
        Read::Values { params, input } => (params, input),
    };
    // The call's arguments.
    let mut args = Vec::with_capacity(route.args.len());
    for arg in &route.args {
        let mut value = match &arg.root {
            ArgRoot::Param(at) => params.get(*at as usize).cloned().flatten(),
            ArgRoot::Input => input.clone(),
            ArgRoot::Caller => caller_value.clone(),
        }
        .unwrap_or(Value::None);
        for name in &arg.path {
            value = match &value {
                Value::Record(record) => record.get(name).cloned().unwrap_or(Value::None),
                _ => Value::None,
            };
        }
        args.push(value);
    }
    // The key, on every route but a `GET`.
    let mut guard = None;
    let mut client_key = None;
    if route.method != HttpMethod::Get {
        let key = match idempotency::key(request) {
            Ok(key) => key,
            Err(()) => return runtime_failure(KEY_INVALID, request_id, None),
        };
        let required = app
            .table
            .apis
            .get(route.api as usize)
            .is_some_and(|api| api.idempotency == Idempotency::Required);
        if key.is_none() && required {
            return runtime_failure(KEY_REQUIRED, request_id, None);
        }
        match (key, &route.keys) {
            (Some(key), Some(store)) => {
                guard = Some(Guard {
                    store: store.clone(),
                    scope,
                    key,
                    digest: idempotency::digest(codecs, route, &params, input.as_ref()),
                    at: ctx.now(),
                });
            }
            (Some(key), None) => client_key = Some((scope, key)),
            (None, _) => {}
        }
    }
    // What a provider call's key is derived from (area 12, "Keys"; chunks
    // 7.2c and 7.9a): the client's key in its caller's scope, kept or not,
    // so a retry under it sends the keys the first request sent; else the
    // request's id.
    match (&guard, client_key) {
        (Some(guard), _) => ctx.set_call_key(guard.scope.clone(), guard.key.clone()),
        (None, Some((scope, key))) => ctx.set_call_key(scope, key),
        (None, None) => ctx.set_call_key("request", request_id),
    }
    ctx.set_request_id(request_id);
    let claimed = Cell::new(false);
    // What was left for after the response before this route ran: only
    // its own action's work is waited for (chunk 6.3).
    let earlier = ctx.deferred().len();
    let mut deadline = deadline;
    let mut attempt = 1;
    // Its client sends it again under its key after a crash `jadpo test`
    // made at a step (chunk 7.5d): claimed again.
    let mut again = false;
    // Sent again, its keyed calls take the steps they took, so each goes
    // under the key it went under.
    let steps = ctx.steps();
    // Where its calls start in the invocation's log, and whether an
    // unkeyed change among them went out before a concurrent change was
    // found at the commit: then it isn't run again, but answered
    // `outcome_unknown`, kept for its key (chunk 7.9a).
    let mark = ctx.capabilities().calls().len();
    let mut parked = false;
    loop {
        // Each run, after a concurrent change at the commit too, takes the
        // steps the first took.
        ctx.set_steps(steps);
        let first = attempt == 1 || std::mem::take(&mut again);
        let work = async {
            if parked {
                return Ok(Outcome::Failed(Box::new(crate::Failure {
                    frames: ctx.frames(),
                    ..crate::outcome_unknown()
                })));
            }
            if first && let Some(guard) = &guard {
                match idempotency::claim(ctx, guard, route.deadline_ms).await {
                    Claim::Claimed => claimed.set(true),
                    Claim::Replay(response) => return Err(Early::Respond(response)),
                    Claim::Answer { status, args } => return Err(Early::Answer { status, args }),
                    Claim::Reused => {
                        return Err(Early::Respond(Box::new(runtime_failure(
                            KEY_REUSED, request_id, None,
                        ))));
                    }
                    Claim::InFlight => {
                        return Err(Early::Respond(Box::new(runtime_failure(
                            KEY_IN_FLIGHT,
                            request_id,
                            None,
                        ))));
                    }
                    Claim::Unavailable => {
                        return Err(Early::Respond(Box::new(runtime_failure(
                            UNAVAILABLE,
                            request_id,
                            None,
                        ))));
                    }
                }
            }
            Ok(match &route.run {
                Run::Action(callable) => {
                    // Its commit waits until the deadline can't interrupt it.
                    ctx.hold();
                    // Under a key it claimed, its key survives a retry: an
                    // action with several changing calls keeps a record of
                    // its steps, held by this claim (area 12, "One runner
                    // per record"; chunk 7.5b).
                    ctx.set_resumable(
                        guard
                            .as_ref()
                            .filter(|_| claimed.get())
                            .map(|guard| guard.at),
                    );
                    let ran = executor.run(ctx, *callable, args.clone()).await;
                    ctx.set_resumable(None);
                    ran
                }
                Run::Listing { keys } => {
                    let holder = args.first().cloned().unwrap_or(Value::None);
                    match identity::list(ctx, app.table.identity.as_ref(), *keys, &holder).await {
                        Stored::Value(value) => Outcome::Value(value),
                        Stored::Failed(failure) => Outcome::Failed(failure),
                        Stored::Conflict { cause, .. } => Outcome::Failed(cause),
                        Stored::Fault(fault) => Outcome::Fault(fault),
                    }
                }
                // Answered once signed in, without running anything.
                Run::Exchange | Run::Passkey(_) | Run::MagicLink(_) | Run::OpenApi(_) => {
                    Outcome::Fault(ctx.fault(
                        crate::FaultKind::Defect("an exchange with no key to trade".into()),
                        Frame::ENTRY,
                    ))
                }
                Run::Query(plan) => {
                    match crate::store(
                        ctx,
                        StoreOp::Read(plan),
                        app.failures,
                        app.json,
                        &args,
                        Frame::ENTRY,
                    )
                    .await
                    {
                        Stored::Value(value) => Outcome::Value(value),
                        Stored::Failed(failure) => Outcome::Failed(failure),
                        Stored::Conflict { cause, .. } => Outcome::Failed(cause),
                        Stored::Fault(fault) => Outcome::Fault(fault),
                    }
                }
            })
        };
        let outcome = match race(work, &mut deadline).await {
            // A body that ran past the deadline without waiting, stopped
            // by its executor or not, is answered as one that waited past it.
            First::Left(Ok(outcome)) if !ctx.out_of_time() => outcome,
            First::Left(Err(Early::Respond(response))) => return *response,
            // A repeat runs the answer query again, rather than replaying a
            // stored body, so it shows how far the work has got (area 7).
            First::Left(Err(Early::Answer { status, args })) => {
                let Some(settle) = &route.settle else {
                    return runtime_failure(INTERNAL_FAULT, request_id, None);
                };
                let Some(values) = kept_args(app, settle, &args) else {
                    return runtime_failure(INTERNAL_FAULT, request_id, None);
                };
                let answered = race(answer_query(app, ctx, settle, &values), &mut deadline).await;
                let First::Left(outcome) = answered else {
                    return runtime_failure(DEADLINE_EXCEEDED, request_id, None);
                };
                let mut response = answer(app, (number, route), request_id, outcome);
                if response.status == route.status.code() {
                    response.status = status;
                }
                return response;
            }
            First::Left(Ok(_)) | First::Right(()) => {
                // Whatever was running is dropped, its transaction with it.
                ctx.abandon();
                if claimed.get()
                    && let Some(guard) = &guard
                {
                    idempotency::release(ctx, guard).await;
                }
                return runtime_failure(DEADLINE_EXCEEDED, request_id, None);
            }
        };
        // A crash `jadpo test` made at a step of the action (chunk 7.5d):
        // nothing it wrote was kept, its claim lapses as a dead process's
        // would, and its client sends the request again under its key.
        if let Outcome::Fault(fault) = &outcome
            && fault.kind == crate::FaultKind::Crashed
            && claimed.get()
            && let Some(guard) = &guard
        {
            let _ = ctx.take_held();
            idempotency::release(ctx, guard).await;
            claimed.set(false);
            // Sent again, as a client's retry is, to a runner that kept
            // none of the models' answers this one had (chunk 7.9a).
            ctx.forget_reused();
            again = true;
            continue;
        }
        let held = ctx.take_held();
        // A session the action started or ended sets its cookies once it
        // commits; such a response isn't kept for a repeat, so a retry signs
        // in again rather than getting an answer without its cookies.
        let session = held.as_ref().and_then(|held| held.session.clone());
        // Nor is one that carries an API token or a service key, which is
        // shown once and kept only as its verifier.
        let issued = held.as_ref().is_some_and(|held| held.issued);
        // Nor one that sets or clears a magic link's binding cookie, which
        // a repeat without the cookie would leave unbound (chunk 7.3b3).
        let binding = held.as_ref().and_then(|held| held.binding.clone());
        // Successes and the failures the action returns are kept for a
        // repeat; `unavailable` and faults are not, so a retry runs again.
        // Nor a model's answer streamed, which a repeat can't be given
        // again (chunk 5.5v6).
        let streamed = match &outcome {
            Outcome::Value(value) if route.stream => crate::stream::number_of(value),
            _ => None,
        };
        let storable = session.is_none()
            && !issued
            && binding.is_none()
            && !route.stream
            && match &outcome {
                Outcome::Value(_) => true,
                Outcome::Failed(failure) => failure.kind.as_deref() != Some("unavailable"),
                Outcome::Fault(_) | Outcome::Assertion(_) => false,
            };
        // A route that waits for its follow-on work answers with its
        // query once the action has committed, and keeps the query's
        // arguments for a repeat rather than a body (chunk 6.3).
        let settling = match (&route.settle, &outcome) {
            (Some(settle), Outcome::Value(result)) => Some((settle, answer_args(settle, result))),
            _ => None,
        };
        let mut response = match &settling {
            Some(_) => Response::default(),
            None => answer(app, (number, route), request_id, outcome),
        };
        let guard = guard.as_ref().filter(|_| claimed.get());
        let keep = guard.filter(|_| storable);
        let params = keep.map(|guard| match &settling {
            Some((settle, args)) => {
                idempotency::answer_params(guard, ACCEPTED, written_args(app, settle, args))
            }
            None => idempotency::store_params(guard, &response),
        });
        let then = match (keep, &params) {
            (Some(guard), Some(params)) => {
                Some((guard.store.as_str(), idempotency::STORE, params.as_slice()))
            }
            _ => None,
        };
        let committed = match held {
            Some(held) => crate::commit_held(ctx, held, then).await,
            None if then.is_some() => crate::commit_held(ctx, crate::Held::default(), then).await,
            None => Ok(()),
        };
        match committed {
            Ok(()) => {}
            // Another transaction changed what the action read, found as
            // it commits, after an unkeyed change went out: it isn't run
            // again, but answered `outcome_unknown` (chunk 7.9a).
            Err(crate::StoreError::ConcurrentChange)
                if matches!(route.run, Run::Action(_))
                    && ctx.capabilities().may_have_acted(mark) =>
            {
                parked = true;
                continue;
            }
            // Otherwise the whole action runs again, as the boundary
            // reruns one (area 6), within the same budget and deadline.
            Err(crate::StoreError::ConcurrentChange)
                if attempt < crate::RERUN_ATTEMPTS && matches!(route.run, Run::Action(_)) =>
            {
                attempt += 1;
                continue;
            }
            Err(_) => {
                if let Some(guard) = guard {
                    idempotency::release(ctx, guard).await;
                }
                return runtime_failure(UNAVAILABLE, request_id, None);
            }
        }
        // The action has committed: its stream is sent as it comes, ended
        // by the call's `timeout:` or the route's deadline (area 7,
        // "Streaming a model's answer"; chunk 5.5v6).
        if let Some(number) = streamed {
            match ctx.take_stream(number) {
                Some(mut events) => {
                    events.end_by(ends_at);
                    response.headers = vec![
                        (
                            "content-type".to_string(),
                            jadpo_std::ai::events::CONTENT_TYPE.to_string(),
                        ),
                        ("cache-control".to_string(), "no-store".to_string()),
                    ];
                    response.stream = Some(crate::stream::EventStream::new(events, request_id));
                }
                None => {
                    response = runtime_failure(INTERNAL_FAULT, request_id, None);
                    response.fault = Some(ctx.fault(
                        crate::FaultKind::Defect("a stream its action didn't open".into()),
                        Frame::ENTRY,
                    ));
                }
            }
        }
        if let Some((settle, args)) = &settling {
            // The work the action set off, delivered now, while the
            // request waits, until it settles or `within:` passes; what
            // is left carries on after the response.
            let work = ctx.background_mut().take_deferred_from(earlier);
            let settled = match within.as_mut() {
                Some(within) => crate::runner::settle_within(executor, ctx, work, within).await,
                None => {
                    ctx.background_mut().defer_all(work);
                    crate::runner::Settled::Unsettled
                }
            };
            response = answer(
                app,
                (number, route),
                request_id,
                answer_query(app, ctx, settle, args).await,
            );
            if response.status == route.status.code() {
                match settled {
                    crate::runner::Settled::Settled => {
                        if let Some(guard) = keep {
                            idempotency::settled(ctx, guard, route.status.code()).await;
                        }
                    }
                    crate::runner::Settled::Unsettled => response.status = ACCEPTED,
                }
            }
        }
        if let (Some(effect), Some(plan)) = (&session, &app.table.identity) {
            response.headers.extend(identity::set_cookies(plan, effect));
        }
        if let (Some(binding), Some(plan)) = (&binding, &app.table.identity) {
            response
                .headers
                .extend(magic_links::binding_header(plan, binding));
        }
        // A credential signing in reissued goes back with the response,
        // unless what ran started or ended the session it came with.
        if let (None, Some(reissued), Some(plan)) = (&session, &reissued, &app.table.identity) {
            response.headers.extend(identity::renewal(plan, reissued));
        }
        if keep.is_none()
            && let Some(guard) = guard
        {
            idempotency::release(ctx, guard).await;
        }
        return response;
    }
}

/// What a route that waits for its follow-on work answers with when the
/// work hadn't settled within `within:` (area 7).
const ACCEPTED: u16 = 202;

/// The arguments `settle`'s answer query takes from the action's
/// `result`: each a path read through it.
fn answer_args(settle: &jadpo_std::http::SettlePlan, result: &Value) -> Vec<Value> {
    settle
        .args
        .iter()
        .map(|path| {
            let mut at = result.clone();
            for name in path {
                at = match &at {
                    Value::Record(record) => record.get(name).cloned().unwrap_or(Value::None),
                    _ => Value::None,
                };
            }
            at
        })
        .collect()
}

/// `args`, the answer query's arguments, as the JSON kept with an
/// `Idempotency-Key`: an array, each written by its parameter's codec.
fn written_args(app: &App<'_>, settle: &jadpo_std::http::SettlePlan, args: &[Value]) -> String {
    let mut out = String::from("[");
    for (at, (value, codec)) in args.iter().zip(&settle.codecs).enumerate() {
        if at > 0 {
            out.push(',');
        }
        app.codecs.write(*codec, value, &mut out);
    }
    out.push(']');
    out
}

/// The answer query's arguments kept as `kept` ([`written_args`]), read
/// back by their codecs; `None` when they no longer read.
fn kept_args(
    app: &App<'_>,
    settle: &jadpo_std::http::SettlePlan,
    kept: &str,
) -> Option<Vec<Value>> {
    let (json, numbers) = jadpo_value::json::parse(kept.as_bytes())?;
    let serde_json::Value::Array(items) = json else {
        return None;
    };
    if items.len() != settle.codecs.len() {
        return None;
    }
    let mut problems = Vec::new();
    let values = items
        .iter()
        .zip(&settle.codecs)
        .map(|(item, codec)| {
            app.codecs.read(
                *codec,
                item,
                &Path::default().numbered(&numbers),
                &mut problems,
            )
        })
        .collect::<Option<Vec<_>>>()?;
    problems.is_empty().then_some(values)
}

/// Runs `settle`'s answer query with `args`, as the request's caller and
/// under the query's policy, as a `GET` running it would (chunk 6.3).
async fn answer_query(
    app: &App<'_>,
    ctx: &mut Ctx<'_>,
    settle: &jadpo_std::http::SettlePlan,
    args: &[Value],
) -> Outcome {
    match crate::store(
        ctx,
        StoreOp::Read(&settle.answer),
        app.failures,
        app.json,
        args,
        Frame::ENTRY,
    )
    .await
    {
        Stored::Value(value) => Outcome::Value(value),
        Stored::Failed(failure) => Outcome::Failed(failure),
        Stored::Conflict { cause, .. } => Outcome::Failed(cause),
        Stored::Fault(fault) => Outcome::Fault(fault),
    }
}

/// The response `outcome` gives: the output with the route's status, or
/// the failure envelope.
fn answer(
    app: &App<'_>,
    (number, route): (u32, &RoutePlan),
    request_id: &str,
    outcome: Outcome,
) -> Response {
    match outcome {
        Outcome::Value(value) => match route.output {
            Some(codec) => {
                let mut body = String::new();
                match app.entry(number).and_then(|entry| entry.output) {
                    Some(write) => write(&app.codecs, &value, &mut body),
                    None => app.codecs.write(codec, &value, &mut body),
                }
                Response {
                    status: route.status.code(),
                    headers: vec![("content-type".to_string(), "application/json".to_string())],
                    body: body.into_bytes(),
                    ..Response::default()
                }
            }
            None => Response {
                status: route.status.code(),
                ..Response::default()
            },
        },
        // A page's cursor that isn't one of this query's is bad input in
        // the field it came in (area 6).
        Outcome::Failed(failure) if failure.key == crate::read::INVALID_CURSOR => invalid_input(
            request_id,
            &[Problem {
                field: cursor_field(route),
                rule: "format".to_string(),
            }],
        ),
        Outcome::Failed(failure) => declared_failure(app, &failure, request_id),
        Outcome::Fault(fault) => {
            let mut response = runtime_failure(INTERNAL_FAULT, request_id, None);
            response.fault = Some(fault);
            response
        }
        Outcome::Assertion(_) => runtime_failure(INTERNAL_FAULT, request_id, None),
    }
}

/// The field a route's page cursor comes in, by its path in the input
/// (`after`, `filter.after`) or the placeholder's name; empty when it
/// doesn't come straight from the request.
fn cursor_field(route: &RoutePlan) -> String {
    let Run::Query(plan) = &route.run else {
        return String::new();
    };
    let Some(jadpo_std::read::Source::Arg { index, path }) =
        plan.page.as_ref().map(|page| &page.after)
    else {
        return String::new();
    };
    let Some(arg) = route.args.get(*index as usize) else {
        return String::new();
    };
    let mut parts: Vec<String> = match &arg.root {
        ArgRoot::Input => Vec::new(),
        ArgRoot::Param(at) => route
            .segments
            .iter()
            .filter_map(|segment| match segment {
                Segment::Param { name, .. } => Some(name.clone()),
                Segment::Fixed(_) => None,
            })
            .nth(*at as usize)
            .into_iter()
            .collect(),
        ArgRoot::Caller => return String::new(),
    };
    parts.extend(arg.path.iter().cloned());
    parts.extend(path.iter().cloned());
    parts.join(".")
}

/// The names of an input's fields whose type is a list or a set, which
/// the query string gives as a repeated key.
fn list_fields(app: &App<'_>, codec: CodecId) -> Vec<String> {
    let Some(Codec::Object { shape, fields, .. }) = app.codecs.codecs.get(codec.0 as usize) else {
        return Vec::new();
    };
    let Some(shape) = app.codecs.shapes.get(shape.0 as usize) else {
        return Vec::new();
    };
    fields
        .iter()
        .enumerate()
        .filter(|(_, codec)| {
            let mut id = **codec;
            if let Some(Codec::Nullable(inner)) = app.codecs.codecs.get(id.0 as usize) {
                id = *inner;
            }
            matches!(
                app.codecs.codecs.get(id.0 as usize),
                Some(Codec::List { .. } | Codec::Set { .. })
            )
        })
        .filter_map(|(slot, _)| shape.field(slot).map(|(name, _)| name.to_string()))
        .collect()
}

/// The envelope (area 4): `code`, `message`, `request_id` and, when given,
/// `details`, already written as JSON.
pub(crate) fn envelope(
    code: &str,
    message: &str,
    request_id: &str,
    details: Option<&str>,
) -> Vec<u8> {
    let mut out = String::from("{\"error\":{\"code\":");
    write_string(code, &mut out);
    out.push_str(",\"message\":");
    write_string(message, &mut out);
    out.push_str(",\"request_id\":");
    write_string(request_id, &mut out);
    if let Some(details) = details {
        out.push_str(",\"details\":");
        out.push_str(details);
    }
    out.push_str("}}");
    out.into_bytes()
}

fn json_response(status: u16, code: &str, body: Vec<u8>) -> Response {
    Response {
        status,
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        body,
        code: Some(code.to_string()),
        route: None,
        fault: None,
        counted: false,
        client: None,
        deferred: Vec::new(),
        calls: Vec::new(),
        steps: Vec::new(),
        refused: None,
        stream: None,
    }
}

pub(crate) fn runtime_failure(
    (status, code, message): (u16, &str, &str),
    request_id: &str,
    details: Option<&str>,
) -> Response {
    json_response(status, code, envelope(code, message, request_id, details))
}

/// 422 `invalid_input`, each problem as `{ field, rule }`.
pub(crate) fn invalid_input(request_id: &str, problems: &[Problem]) -> Response {
    let mut details = String::from("{\"problems\":[");
    for (at, problem) in problems.iter().enumerate() {
        if at > 0 {
            details.push(',');
        }
        details.push_str("{\"field\":");
        write_string(&problem.field, &mut details);
        details.push_str(",\"rule\":");
        write_string(&problem.rule, &mut details);
        details.push('}');
    }
    details.push_str("]}");
    runtime_failure(INVALID_INPUT, request_id, Some(&details))
}

/// A failure the program raised, in the envelope with its client fields
/// and the status its kind sets.
pub(crate) fn declared_failure(app: &App<'_>, failure: &Failure, request_id: &str) -> Response {
    let status = failure.status.unwrap_or(500);
    let message = failure
        .message
        .clone()
        .unwrap_or_else(|| kind_message(failure.kind.as_deref()).to_string());
    let details = app.table.failure(&failure.key).and_then(|codec| {
        if codec.client.is_empty() {
            return None;
        }
        let mut out = String::from("{");
        let mut first = true;
        for (name, id) in &codec.client {
            let Some((_, value)) = failure.client.iter().find(|(field, _)| field == name) else {
                continue;
            };
            if !first {
                out.push(',');
            }
            first = false;
            write_string(name, &mut out);
            out.push(':');
            app.codecs.write(*id, value, &mut out);
        }
        out.push('}');
        Some(out)
    });
    let mut response = json_response(
        status,
        &failure.code,
        envelope(&failure.code, &message, request_id, details.as_deref()),
    );
    // A built-in failure that says when to try again: `rate_limited` or
    // `sign_in_codes_disabled` from magic links' operations (chunk 7.3b3).
    if let Some(seconds) = failure.retry_after {
        response
            .headers
            .push(("retry-after".to_string(), seconds.max(1).to_string()));
    }
    response
}

// --- identity operations -------------------------------------------------------

/// `sessions.start(user_id)`, `sessions.end()`, an API token's or a service
/// key's operation, magic links' or a test's passkeys', called with `args`
/// at site `at` (area 8): the one entry point both executors call (chunk
/// 8.1h), which keeps, lists or deletes what it names in the running
/// action's transaction. A store's refusal is the failure it raised, never
/// a `conflict:` arm, which none of them has.
pub async fn session(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    op: jadpo_std::http::SessionOp,
    args: &[Value],
    at: u32,
) -> Stored {
    use jadpo_std::http::SessionOp;
    let plan = executor
        .plan()
        .routes
        .and_then(|routes| routes.identity.as_ref());
    let stored = match (op, args) {
        // Magic links' operations, for the application's own routes (area
        // 8, "Writing the routes yourself"; chunk 7.3b3), through the code
        // the generated routes use.
        (SessionOp::MagicLinkRequest, _) => {
            magic_links::request(executor, ctx, plan, args, at).await
        }
        (SessionOp::MagicLinkRedeem, _) => magic_links::redeem(executor, ctx, plan, args, at).await,
        (SessionOp::MagicLinkRedeemCode, _) => {
            magic_links::redeem_code(executor, ctx, plan, args, at).await
        }
        // `test_passkeys`, which only a route test calls (chunk 5.5z2),
        // through the generated passkey routes.
        (SessionOp::TestPasskeyRegister, _) => {
            test_passkeys::register(&executor.app(), executor, ctx, args, at).await
        }
        (SessionOp::TestPasskeySignIn, _) => {
            test_passkeys::sign_in(&executor.app(), executor, ctx, args, at).await
        }
        (SessionOp::Start, [user]) => identity::start(ctx, plan, user, at).await,
        (SessionOp::End, []) => identity::end(ctx, plan, at).await,
        (SessionOp::IssueToken, [user]) => identity::issue_token(ctx, plan, user, at).await,
        (SessionOp::IssueKey, [service, expires_at]) => {
            identity::issue_key(ctx, plan, service, expires_at, at).await
        }
        (SessionOp::Tokens | SessionOp::Keys, [holder]) => {
            identity::issued(ctx, plan, op.on_service(), holder, at).await
        }
        (SessionOp::RevokeToken | SessionOp::RevokeKey, [id]) => {
            identity::revoke(ctx, plan, op.on_service(), id, at).await
        }
        _ => {
            return Stored::Fault(ctx.fault(
                crate::FaultKind::Defect(format!(
                    "`{}` called with the wrong arguments",
                    op.name()
                )),
                at,
            ));
        }
    };
    match stored {
        Stored::Conflict { cause, .. } => Stored::Failed(cause),
        stored => stored,
    }
}

/// A caller made from its record, `record`, in a test, fixture or seed
/// (`Principal.user(user)`, area 8, question 10), as the program's sign-in
/// plan makes one of `kind`: both executors make it here. `None` when the
/// value isn't such a record, which the checker rules out.
pub fn caller_from(
    executor: &dyn Executor,
    kind: jadpo_std::http::PrincipalKind,
    record: &Value,
) -> Option<Value> {
    let plan = executor
        .plan()
        .routes
        .and_then(|routes| routes.identity.as_ref())?
        .principal(kind)?;
    identity::caller_from_record(plan, kind, record)
}

// --- route tests ---------------------------------------------------------------

/// A route test's request (area 13: "through the real generated boundary
/// without a network"), answered by the HTTP component in the test's own
/// context and read back as a `Response` value: route `route`'s request
/// built from `args` ([`test_request`]), with `key` as its
/// `Idempotency-Key` and signed in as `caller` when given. The fault
/// behind a 500 ends the test with that fault, as one anywhere else in it
/// would: a 500 is never what a test means to check.
#[allow(clippy::too_many_arguments)]
pub async fn route_test(
    app: &App<'_>,
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    route: u32,
    args: &[Value],
    key: Option<&Value>,
    caller: Option<&Value>,
) -> Result<Value, Box<Fault>> {
    let Some(request) = test_request(ctx, app, route, args, key, caller).await? else {
        return Ok(Value::None);
    };
    let id = request_id(ctx, u64::from(ctx.actions()));
    let mut response = handle(app, executor, ctx, &request, &id).await;
    if let Some(fault) = response.fault.take() {
        return Err(fault);
    }
    // A streaming route's events, read to the last as a client reads
    // them (area 7, "Streaming a model's answer"; chunk 5.5v6).
    let mut events = Vec::new();
    if let Some(stream) = response.stream.take() {
        while let Some(event) = stream.next_on(Some(ctx.host())).await {
            events.push(event);
        }
    }
    Ok(response_value(app, route, &response, &events))
}

/// The request a route test sends: route `route`'s method and path, its
/// placeholders written from `args` (in the route's order), then its input,
/// the last of `args`, as the query string or the JSON body, its header
/// fields as headers. Every request but a `GET` sends an `Idempotency-Key`,
/// as a real client would: `key`, the one the test gives, or a fresh one
/// from the test's seed (chunk 4.7). With `caller` (`as:`), it signs in
/// through the application's real authentication
/// ([`identity::test_credentials`]). `None` when `route` isn't in the
/// table.
pub async fn test_request(
    ctx: &mut Ctx<'_>,
    app: &App<'_>,
    route: u32,
    args: &[Value],
    key: Option<&Value>,
    caller: Option<&Value>,
) -> Result<Option<Request>, Box<Fault>> {
    let Some(mut request) = written_request(app, route, args) else {
        return Ok(None);
    };
    let writes = app
        .table
        .routes
        .get(route as usize)
        .is_some_and(|plan| plan.method != HttpMethod::Get);
    if writes && request.header(idempotency::HEADER).is_none() {
        let key = match key {
            Some(Value::Text(text)) => Some(text.to_string()),
            Some(_) => None,
            None => ctx.new_id().map(|id| crate::uuid_text(&id)),
        };
        if let Some(key) = key {
            request.headers.push(("Idempotency-Key".to_string(), key));
        }
    }
    if let Some(caller) = caller {
        let started = ctx.started();
        let headers =
            identity::test_credentials(ctx, app.table, started.auth(), route, caller).await?;
        request.headers.extend(headers);
    }
    Ok(Some(request))
}

/// Route `route`'s request as its values write it, before its key and
/// credentials.
fn written_request(app: &App<'_>, route: u32, args: &[Value]) -> Option<Request> {
    let plan = app.table.routes.get(route as usize)?;
    let mut values = args.iter();
    let mut path = String::new();
    for segment in &plan.segments {
        path.push('/');
        match segment {
            Segment::Fixed(text) => path.push_str(text),
            Segment::Param { codec, .. } => {
                let value = values.next().cloned().unwrap_or(Value::None);
                let mut json = String::new();
                app.codecs.write(*codec, &value, &mut json);
                path.push_str(&percent_encode(&json_text(&json)));
            }
        }
    }
    if path.is_empty() {
        path.push('/');
    }
    let mut request = Request {
        method: plan.method.word().to_string(),
        path,
        address: Some("route test".to_string()),
        ..Request::default()
    };
    if let Some(input) = &plan.input {
        let value = values.next().cloned().unwrap_or(Value::None);
        let mut json = String::new();
        app.codecs.write(input.codec, &value, &mut json);
        let mut object = match serde_json::from_str::<serde_json::Value>(&json) {
            Ok(serde_json::Value::Object(object)) => object,
            _ => serde_json::Map::new(),
        };
        for (field, header) in &input.headers {
            if let Some(value) = object.remove(field)
                && !value.is_null()
            {
                request
                    .headers
                    .push((header.clone(), json_text(&value.to_string())));
            }
        }
        if input.query {
            let mut pairs = Vec::new();
            for (key, value) in &object {
                let items = match value {
                    serde_json::Value::Array(items) => items.clone(),
                    serde_json::Value::Null => Vec::new(),
                    other => vec![other.clone()],
                };
                for item in items {
                    pairs.push(format!(
                        "{}={}",
                        percent_encode(key),
                        percent_encode(&json_text(&item.to_string()))
                    ));
                }
            }
            request.query = (!pairs.is_empty()).then(|| pairs.join("&"));
        } else {
            request.body = serde_json::Value::Object(object).to_string().into_bytes();
            request
                .headers
                .push(("content-type".to_string(), "application/json".to_string()));
        }
    }
    Some(request)
}

/// A JSON value as text: a string's contents, anything else as written.
fn json_text(json: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(serde_json::Value::String(text)) => text,
        _ => json.to_string(),
    }
}

/// `text` with everything but unreserved characters percent-encoded.
fn percent_encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// What a route test gets back: a `Response` with its `status`, the
/// failure's `code` (`none` on success) and route `route`'s output read
/// back from the body (`none` unless it succeeded). A streaming route's
/// has `pieces`, the text of each `piece` event of `events`, in place of
/// its output, and `code` the `error` event's that ended it (chunk 5.5v6).
pub fn response_value(
    app: &App<'_>,
    route: u32,
    response: &Response,
    events: &[crate::stream::ServerEvent],
) -> Value {
    let plan = app.table.routes.get(route as usize);
    if plan.is_some_and(|plan| plan.stream) {
        let pieces = events
            .iter()
            .filter_map(|event| match event {
                crate::stream::ServerEvent::Piece(text) => Some(Value::text(text)),
                _ => None,
            })
            .collect();
        let code = events
            .iter()
            .find_map(|event| match event {
                crate::stream::ServerEvent::Failed { code, .. } => Some(Value::text(code)),
                _ => None,
            })
            .or_else(|| response.code.as_deref().map(Value::text))
            .unwrap_or(Value::None);
        return Value::record(
            "Response",
            vec![
                ("status".into(), Value::Int(i64::from(response.status))),
                ("code".into(), code),
                ("pieces".into(), Value::list(pieces)),
            ],
        );
    }
    let output = match plan.and_then(|plan| plan.output) {
        Some(codec) if (200..300).contains(&response.status) => {
            jadpo_value::json::parse(&response.body)
                .and_then(|(json, numbers)| {
                    app.codecs.read(
                        codec,
                        &json,
                        &Path::default().numbered(&numbers),
                        &mut Vec::new(),
                    )
                })
                .unwrap_or(Value::None)
        }
        _ => Value::None,
    };
    let code = match &response.code {
        Some(code) => Value::text(code),
        None => Value::None,
    };
    Value::record(
        "Response",
        vec![
            ("status".into(), Value::Int(i64::from(response.status))),
            ("code".into(), code),
            ("output".into(), output),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_and_query_strings_decode_their_escapes() {
        assert_eq!(
            split_path("/todos/a%20b/"),
            Some(vec!["todos".to_string(), "a b".to_string()])
        );
        assert_eq!(split_path("/bad/%zz"), None);
        let values = query_values("status=open&status=done&q=milk+and%20eggs").unwrap();
        assert_eq!(values["status"], ["open", "done"]);
        assert_eq!(values["q"], ["milk and eggs"]);
    }

    #[test]
    fn a_host_that_cant_serve_answers_in_the_envelope_with_a_request_id() {
        let host = crate::MemoryHost::new();
        for (response, status, code) in [
            (unavailable(&host), 503, "unavailable"),
            (unreadable(&host), 400, "invalid_request"),
        ] {
            assert_eq!(response.status, status);
            assert_eq!(response.code.as_deref(), Some(code));
            let id = response.header("x-request-id").expect("a request id");
            assert!(id.starts_with("req_"), "{id}");
            let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
            assert_eq!(body["error"]["code"], code);
            assert_eq!(body["error"]["request_id"], id);
        }
    }

    #[test]
    fn the_envelope_carries_the_request_id() {
        let body = envelope("route_not_found", "No route.", "req_1", None);
        assert_eq!(
            String::from_utf8(body).unwrap(),
            "{\"error\":{\"code\":\"route_not_found\",\"message\":\"No route.\",\"request_id\":\"req_1\"}}"
        );
    }
}
