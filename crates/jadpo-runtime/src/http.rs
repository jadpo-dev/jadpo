//! The runtime's HTTP component (area 7; docs/compiler-runtime.md §2.6):
//! a request in, a response out, the same for every host and executor. It
//! routes by the route table, decodes the placeholders and the one input
//! with the route's codecs, runs the action (through the executor, inside
//! the runtime's action boundary) or the query (through the store's entry
//! point), and answers with the output, or with the failure envelope and
//! the status its kind sets (area 4), always with a `request_id` in the
//! body and `x-request-id`.
//!
//! The runtime answers `HEAD` (as `GET`, without the body) and `OPTIONS`
//! (with `Allow`) itself, and every route has the default rate limits
//! ([`crate::RateLimits`]). Deadlines, body limits, idempotency and health
//! come with chunk 4.2, sign-in with 4.3 and policy with 4.4.

use std::collections::BTreeMap;

use jadpo_std::HttpMethod;
use jadpo_std::http::{ArgRoot, Codec, CodecId, RoutePlan, RouteTable, Run, Segment};
use jadpo_std::types::FailurePlan;
use jadpo_value::Value;
use jadpo_value::json::{Codecs, Path, Problem, write_string};

use crate::ratelimit::{Allowed, RateLimits};
use crate::{Ctx, Executor, Failure, Frame, Outcome, StoreOp, Stored};

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
    /// The client's address, which rate limits count by when no one is
    /// signed in.
    pub address: Option<String>,
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
    /// The route it answered by: `POST /todos`.
    pub route: Option<String>,
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
    pub codecs: Codecs<'a>,
    pub failures: &'a [FailurePlan],
}

/// The failures the runtime answers with itself (area 7, "How failures
/// reach the client").
const INVALID_REQUEST: (u16, &str, &str) = (400, "invalid_request", "Request could not be read.");
const INVALID_INPUT: (u16, &str, &str) = (422, "invalid_input", "Request input is invalid.");
const ROUTE_NOT_FOUND: (u16, &str, &str) =
    (404, "route_not_found", "No route matches this request.");
const METHOD_NOT_ALLOWED: (u16, &str, &str) = (
    405,
    "method_not_allowed",
    "This path doesn't answer this method.",
);
const RATE_LIMITED: (u16, &str, &str) = (429, "rate_limited", "Too many requests.");
const INTERNAL_FAULT: (u16, &str, &str) = (
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
    let mut bytes = [0u8; 8];
    let n = if ctx.host().random(&mut bytes) {
        u64::from_be_bytes(bytes)
    } else {
        let mut z = ctx
            .env()
            .seed
            .unwrap_or(0)
            .wrapping_add(sequence.wrapping_mul(0x9E37_79B9_7F4A_7C15))
            .wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    };
    format!("req_{n:016x}")
}

/// Answers `request` with `app`'s routes, running what a route runs with
/// `executor` against `ctx`, whose stack is empty. `limits` counts the
/// request against its caller's allowance when given.
pub async fn handle(
    app: &App<'_>,
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    request: &Request,
    request_id: &str,
    limits: Option<&RateLimits>,
) -> Response {
    let mut response = respond(app, executor, ctx, request, request_id, limits).await;
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
    limits: Option<&RateLimits>,
) -> Response {
    if let Some(limits) = limits {
        let key = format!(
            "address {}",
            request.address.as_deref().unwrap_or("unknown")
        );
        if let Allowed::No { retry_after } = limits.allow(&key, ctx.now()) {
            let mut response = runtime_failure(RATE_LIMITED, request_id, None);
            response
                .headers
                .push(("retry-after".to_string(), retry_after.to_string()));
            return response;
        }
    }
    let Some(segments) = split_path(&request.path) else {
        return runtime_failure(INVALID_REQUEST, request_id, None);
    };
    let matching: Vec<&RoutePlan> = app
        .table
        .routes
        .iter()
        .filter(|route| matches(route, &segments))
        .collect();
    if matching.is_empty() {
        return runtime_failure(ROUTE_NOT_FOUND, request_id, None);
    }
    let mut allowed: Vec<&str> = matching.iter().map(|route| route.method.word()).collect();
    if allowed.contains(&"GET") {
        allowed.push("HEAD");
    }
    allowed.push("OPTIONS");
    allowed.sort_unstable();
    allowed.dedup();
    let allow = allowed.join(", ");
    if request.method == "OPTIONS" {
        return Response {
            status: 204,
            headers: vec![("allow".to_string(), allow)],
            ..Response::default()
        };
    }
    let method = match request.method.as_str() {
        "HEAD" => Some(HttpMethod::Get),
        other => HttpMethod::from_word(other),
    };
    // A fixed segment beats a placeholder: of the routes for this method,
    // the one with fixed text where the others have a placeholder.
    let route = method.and_then(|method| {
        matching
            .iter()
            .filter(|route| route.method == method)
            .max_by_key(|route| fixed_mask(route))
            .copied()
    });
    let Some(route) = route else {
        let mut response = runtime_failure(METHOD_NOT_ALLOWED, request_id, None);
        response.headers.push(("allow".to_string(), allow));
        return response;
    };
    let mut response = run_route(app, executor, ctx, request, request_id, route, &segments).await;
    response.route = Some(route.label());
    response
}

/// Each `1` bit a fixed segment, the first segment highest, so the route
/// with fixed text earliest wins.
fn fixed_mask(route: &RoutePlan) -> u64 {
    route.segments.iter().take(64).fold(0u64, |mask, segment| {
        (mask << 1) | u64::from(matches!(segment, Segment::Fixed(_)))
    })
}

fn matches(route: &RoutePlan, segments: &[String]) -> bool {
    route.segments.len() == segments.len()
        && route
            .segments
            .iter()
            .zip(segments)
            .all(|(segment, written)| match segment {
                Segment::Fixed(text) => text == written,
                Segment::Param { .. } => !written.is_empty(),
            })
}

/// A path's segments, percent-decoded; `None` when one isn't valid UTF-8.
fn split_path(path: &str) -> Option<Vec<String>> {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| percent_decode(segment, false))
        .collect()
}

/// `text` with its `%XX` escapes decoded, and `+` read as a space in a
/// query string; `None` when an escape is malformed or the bytes aren't
/// UTF-8.
fn percent_decode(text: &str, plus: bool) -> Option<String> {
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
fn query_values(query: &str) -> Option<BTreeMap<String, Vec<String>>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        out.entry(percent_decode(key, true)?)
            .or_default()
            .push(percent_decode(value, true)?);
    }
    Some(out)
}

async fn run_route(
    app: &App<'_>,
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    request: &Request,
    request_id: &str,
    route: &RoutePlan,
    segments: &[String],
) -> Response {
    let codecs = &app.codecs;
    let mut problems: Vec<Problem> = Vec::new();
    // Placeholders, each decoded once and checked.
    let mut params = Vec::new();
    for (segment, written) in route.segments.iter().zip(segments) {
        if let Segment::Param { name, codec } = segment {
            params.push(codecs.read_text(*codec, written, &Path::field(name), &mut problems));
        }
    }
    // The input.
    let input = match &route.input {
        None => None,
        Some(plan) => {
            let mut object = serde_json::Map::new();
            if plan.query {
                let Some(values) = query_values(request.query.as_deref().unwrap_or("")) else {
                    return runtime_failure(INVALID_REQUEST, request_id, None);
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
                if request.body.iter().all(u8::is_ascii_whitespace) {
                    return runtime_failure(INVALID_REQUEST, request_id, None);
                }
                match serde_json::from_slice::<serde_json::Value>(&request.body) {
                    Ok(serde_json::Value::Object(entries)) => object = entries,
                    Ok(_) => {
                        problems.push(Problem {
                            field: String::new(),
                            rule: "type".to_string(),
                        });
                    }
                    Err(_) => return runtime_failure(INVALID_REQUEST, request_id, None),
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
            let value = codecs.read(
                plan.codec,
                &serde_json::Value::Object(object),
                &Path::default(),
                &mut problems,
            );
            if problems.len() > before { None } else { value }
        }
    };
    if !problems.is_empty() {
        return invalid_input(request_id, &problems);
    }
    // The call's arguments.
    let mut args = Vec::with_capacity(route.args.len());
    for arg in &route.args {
        let mut value = match &arg.root {
            ArgRoot::Param(at) => params.get(*at as usize).cloned().flatten(),
            ArgRoot::Input => input.clone(),
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
    let outcome = match &route.run {
        Run::Action(callable) => executor.run(ctx, *callable, args).await,
        Run::Query(plan) => {
            match crate::store(ctx, StoreOp::Read(plan), app.failures, &args, Frame::ENTRY).await {
                Stored::Value(value) => Outcome::Value(value),
                Stored::Failed(failure) => Outcome::Failed(failure),
                Stored::Conflict { cause, .. } => Outcome::Failed(cause),
                Stored::Fault(fault) => Outcome::Fault(fault),
            }
        }
    };
    match outcome {
        Outcome::Value(value) => match route.output {
            Some(codec) => {
                let mut body = String::new();
                codecs.write(codec, &value, &mut body);
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
        Outcome::Failed(failure) => declared_failure(app, &failure, request_id),
        Outcome::Fault(_) | Outcome::Assertion(_) => {
            runtime_failure(INTERNAL_FAULT, request_id, None)
        }
    }
}

/// The names of an input's fields whose type is a list or a set, which
/// the query string gives as a repeated key.
fn list_fields(app: &App<'_>, codec: CodecId) -> Vec<String> {
    let Some(Codec::Object { shape, fields }) = app.codecs.codecs.get(codec.0 as usize) else {
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
fn envelope(code: &str, message: &str, request_id: &str, details: Option<&str>) -> Vec<u8> {
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
    }
}

fn runtime_failure(
    (status, code, message): (u16, &str, &str),
    request_id: &str,
    details: Option<&str>,
) -> Response {
    json_response(status, code, envelope(code, message, request_id, details))
}

/// 422 `invalid_input`, each problem as `{ field, rule }`.
fn invalid_input(request_id: &str, problems: &[Problem]) -> Response {
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
fn declared_failure(app: &App<'_>, failure: &Failure, request_id: &str) -> Response {
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
    json_response(
        status,
        &failure.code,
        envelope(&failure.code, &message, request_id, details.as_deref()),
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
    fn the_envelope_carries_the_request_id() {
        let body = envelope("route_not_found", "No route.", "req_1", None);
        assert_eq!(
            String::from_utf8(body).unwrap(),
            "{\"error\":{\"code\":\"route_not_found\",\"message\":\"No route.\",\"request_id\":\"req_1\"}}"
        );
    }
}
