//! Traces, metrics and logs (area 16, "Telemetry"; chunk O1), recorded
//! the same way whichever executor runs a program, and sent over
//! OpenTelemetry's protocol, OTLP, as JSON over HTTP, with no
//! vendor-specific code: a vendor is an endpoint and a header.
//!
//! - **What code records** ([`Recorded`]): each `telemetry.<metric>.add`,
//!   `record` and `set`, and each `telemetry.note`, kept on the
//!   invocation's [`crate::Ctx`] by [`record`], the one entry point both
//!   executors call, and handed on with its response.
//! - **What is sent** ([`Telemetry`]): after each request, the
//!   invocation's spans as one trace when `detail:` says its whole trace
//!   goes ([`Telemetry::answered`]), its metrics always, and the log events
//!   the runtime wrote since the last send ([`Telemetry::log`]), each as
//!   an OTLP payload ([`otlp`]). Nothing in them is a field value: spans
//!   carry names, identifiers and failure codes, a log event's fields are
//!   already written by the log's rules (area 4, "Logging"), and the only
//!   values an app adds are a metric's labels and a note's attributes,
//!   which the compiler holds to enums, `Bool`, text and numbers, never a
//!   personal field.
//! - **Where it goes**: through the host's [`Transport`] to the endpoint
//!   the section names, after the response, in the host's deferred work
//!   ([`crate::Host::defer`]: `waitUntil` on Workers); and under `jadpo
//!   dev`, to the files it keeps on the machine ([`Keep`]), whether or not
//!   anything is sent.

pub mod otlp;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use jadpo_std::telemetry::{Full, Level, MetricKind, Recording, TelemetryPlan};
use jadpo_value::Value;
use serde_json::{Map, Value as Json};

use crate::calls::Called;
use crate::config::ConfigValues;
use crate::spans::Spans;
use crate::{Ctx, Executor, Host, LocalFuture};

/// The most notes one invocation keeps; past it each is counted, not kept.
pub const MOST_NOTES: usize = 64;

/// The most metric points one invocation keeps.
pub const MOST_POINTS: usize = 1_000;

/// The most log records kept between sends.
pub const MOST_LOGS: usize = 1_000;

/// How long one send waits for the collector.
pub const SEND_TIMEOUT_MS: u64 = 10_000;

/// A value an attribute or a label carries.
#[derive(Clone, Debug, PartialEq)]
pub enum Attr {
    Text(String),
    Int(i64),
    Double(f64),
    Bool(bool),
}

impl Attr {
    /// A value of the kinds the compiler lets a label or a note give:
    /// `Bool`, an enum's variant by name, text and numbers. `None` for
    /// anything else, which compiled code never hands over.
    pub fn of(value: &Value) -> Option<Attr> {
        match value {
            Value::Bool(bool) => Some(Attr::Bool(*bool)),
            Value::Int(int) => Some(Attr::Int(*int)),
            Value::Decimal(decimal) => Some(Attr::Double(decimal_f64(*decimal))),
            Value::Text(text) => Some(Attr::Text(text.to_string())),
            Value::Variant(variant) if variant.fields.is_empty() => {
                Some(Attr::Text(variant.name.to_string()))
            }
            _ => None,
        }
    }

    /// As JSON, for `jadpo dev`'s files and a test's comparison.
    pub fn json(&self) -> Json {
        match self {
            Attr::Text(text) => Json::from(text.as_str()),
            Attr::Int(int) => Json::from(*int),
            Attr::Double(double) => Json::from(*double),
            Attr::Bool(bool) => Json::from(*bool),
        }
    }
}

fn decimal_f64(decimal: jadpo_value::Decimal) -> f64 {
    decimal.mantissa as f64 / 10f64.powi(decimal.scale as i32)
}

/// A metric's value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Number {
    Int(i64),
    Double(f64),
}

impl Number {
    pub fn as_f64(self) -> f64 {
        match self {
            Number::Int(int) => int as f64,
            Number::Double(double) => double,
        }
    }
}

/// One value recorded against a metric.
#[derive(Clone, Debug, PartialEq)]
pub struct Point {
    /// The metric by the name it is sent under.
    pub metric: String,
    pub kind: MetricKind,
    pub value: Number,
    /// Each label's name and value, in the metric's `by:` order.
    pub labels: Vec<(String, Attr)>,
}

/// What one invocation's code recorded.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Recorded {
    /// Each `telemetry.note` attribute, the last for a name winning.
    pub notes: Vec<(String, Attr)>,
    pub points: Vec<Point>,
    /// How many notes and points it didn't keep, past [`MOST_NOTES`] and
    /// [`MOST_POINTS`].
    pub dropped: u32,
}

/// No value recorded is ever NaN: each comes from an `Int`, a `Duration`
/// or a `Decimal`.
impl Eq for Recorded {}

impl Recorded {
    pub fn is_empty(&self) -> bool {
        self.notes.is_empty() && self.points.is_empty()
    }

    fn note(&mut self, name: &str, value: Attr) {
        if let Some(kept) = self.notes.iter_mut().find(|(kept, _)| kept == name) {
            kept.1 = value;
        } else if self.notes.len() < MOST_NOTES {
            self.notes.push((name.to_string(), value));
        } else {
            self.dropped = self.dropped.saturating_add(1);
        }
    }

    fn point(&mut self, point: Point) {
        if self.points.len() < MOST_POINTS {
            self.points.push(point);
        } else {
            self.dropped = self.dropped.saturating_add(1);
        }
    }
}

/// Records what `recording` says, given `args`, at site `at` of the frame
/// on top of `ctx`: the one entry point both executors call for a
/// `telemetry.` call. It gives nothing. A counter given less than zero
/// records nothing, as OpenTelemetry's API ignores it, and logs
/// `telemetry.value_refused`.
pub fn record(ctx: &mut Ctx<'_>, recording: &Recording, args: &[Value], at: u32) -> Called {
    match recording {
        Recording::Note { names } => {
            if names.len() != args.len() {
                return defect(ctx, "a note with a value missing", at);
            }
            for (name, value) in names.iter().zip(args) {
                let Some(attr) = Attr::of(value) else {
                    return defect(ctx, "a note of a value it can't carry", at);
                };
                ctx.recorded_mut().note(name, attr);
            }
            Called::Value(Value::Unit)
        }
        Recording::Metric {
            metric,
            kind,
            labels,
        } => {
            let Some((value, given)) = args.split_first() else {
                return defect(ctx, "a metric recorded with no value", at);
            };
            if given.len() != labels.len() {
                return defect(ctx, "a metric recorded with a label missing", at);
            }
            let number = match value {
                Value::Int(int) => Number::Int(*int),
                Value::Duration(ms) => Number::Int(*ms),
                Value::Decimal(decimal) => Number::Double(decimal_f64(*decimal)),
                _ => {
                    return defect(
                        ctx,
                        "a metric recorded with a value that isn't a number",
                        at,
                    );
                }
            };
            if *kind == MetricKind::Counter && number.as_f64() < 0.0 {
                let mut fields = Map::new();
                fields.insert("metric".into(), Json::from(metric.as_str()));
                fields.insert("reason".into(), Json::from("a counter only rises"));
                ctx.host().log("telemetry.value_refused", fields);
                return Called::Value(Value::Unit);
            }
            let mut out = Vec::with_capacity(labels.len());
            for (name, value) in labels.iter().zip(given) {
                let Some(attr) = Attr::of(value) else {
                    return defect(ctx, "a label of a value it can't carry", at);
                };
                out.push((name.clone(), attr));
            }
            ctx.recorded_mut().point(Point {
                metric: metric.clone(),
                kind: *kind,
                value: number,
                labels: out,
            });
            Called::Value(Value::Unit)
        }
    }
}

fn defect(ctx: &mut Ctx<'_>, what: &str, at: u32) -> Called {
    Called::Fault(ctx.fault(crate::FaultKind::Defect(what.to_string()), at))
}

/// One of the three signals, and where OTLP takes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Signal {
    Traces,
    Metrics,
    Logs,
}

impl Signal {
    pub const ALL: [Signal; 3] = [Signal::Traces, Signal::Metrics, Signal::Logs];

    /// The path OTLP/HTTP takes it at, after the endpoint.
    pub fn path(self) -> &'static str {
        match self {
            Signal::Traces => "/v1/traces",
            Signal::Metrics => "/v1/metrics",
            Signal::Logs => "/v1/logs",
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            Signal::Traces => "traces",
            Signal::Metrics => "metrics",
            Signal::Logs => "logs",
        }
    }
}

/// How a host sends a payload to the collector: an HTTP `POST` of JSON to
/// `url`, with `headers`, which the runtime chose, waiting at most
/// `timeout_ms`, to any address: the endpoint is the operator's, often a
/// collector on the same machine. Its answer's status, or why it wasn't
/// sent, for the host's log, never naming a header's value.
pub trait Transport {
    fn post<'a>(
        &'a self,
        url: &'a str,
        headers: &'a [(String, String)],
        body: Vec<u8>,
        timeout_ms: u64,
    ) -> LocalFuture<'a, Result<u16, String>>;
}

/// Where `jadpo dev` keeps what a program would send, on the machine: one
/// OTLP payload a signal at a time, as it would be sent.
pub trait Keep {
    fn kept(&self, signal: Signal, payload: &Json);
}

/// What every payload says of who sent it: OpenTelemetry's resource.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Resource {
    /// `service.name`: the deployable's name, or the app's under `jadpo
    /// dev`.
    pub service: String,
    /// `jadpo.build`, the build id, which a monitoring product decodes the
    /// `jadpo.` attributes against.
    pub build: String,
    /// `service.version`: the Jadpo version that built it.
    pub compiler: String,
    /// `native`, `wasm` or `dev`.
    pub target: String,
    /// `service.instance.id`: the process or isolate.
    pub instance: String,
}

/// Where and how a program sends: the endpoint and headers read as it
/// started, the section and the host's way out.
pub struct Sending {
    pub endpoint: String,
    pub headers: Vec<(String, String)>,
    pub plan: TelemetryPlan,
    pub transport: Rc<dyn Transport>,
}

impl std::fmt::Debug for Sending {
    /// The endpoint and the headers' names; never a header's value.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sending")
            .field("endpoint", &self.endpoint)
            .field(
                "headers",
                &self
                    .headers
                    .iter()
                    .map(|(name, _)| name)
                    .collect::<Vec<_>>(),
            )
            .field("plan", &self.plan)
            .finish_non_exhaustive()
    }
}

/// Why a program can't send what its section asks for, as it starts:
/// the variable that holds no endpoint, or a headers variable that
/// doesn't read. Never a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unsendable {
    /// No endpoint: `endpoint:`'s setting has no value, or, left out,
    /// [`jadpo_std::telemetry::ENDPOINT_VARIABLE`] isn't set.
    NoEndpoint { variable: String },
    /// The endpoint isn't an `http` or `https` address.
    EndpointInvalid { variable: String },
    /// [`jadpo_std::telemetry::HEADERS_VARIABLE`] isn't `name=value`
    /// pairs.
    HeadersInvalid,
}

impl Unsendable {
    /// The problem's code, for a start's log.
    pub fn code(&self) -> &'static str {
        match self {
            Unsendable::NoEndpoint { .. } => "telemetry_endpoint_missing",
            Unsendable::EndpointInvalid { .. } => "telemetry_endpoint_invalid",
            Unsendable::HeadersInvalid => "telemetry_headers_invalid",
        }
    }

    /// One sentence naming the variable, never its value.
    pub fn message(&self) -> String {
        match self {
            Unsendable::NoEndpoint { variable } => format!(
                "`telemetry:` has nowhere to send: set `{variable}` to the collector's address"
            ),
            Unsendable::EndpointInvalid { variable } => {
                format!("`{variable}` isn't an http or https address")
            }
            Unsendable::HeadersInvalid => format!(
                "`{}` isn't comma-separated `name=value` pairs",
                jadpo_std::telemetry::HEADERS_VARIABLE
            ),
        }
    }
}

/// The endpoint and headers `plan` sends to, read from the settings'
/// `values` (the settings' variables, by `config`'s field names) or,
/// where the section leaves them out, from the standard OpenTelemetry
/// variables through `variable`.
pub fn destination(
    plan: &TelemetryPlan,
    config: Option<&jadpo_std::config::ConfigPlan>,
    values: Option<&ConfigValues>,
    variable: &dyn Fn(&str) -> Option<String>,
) -> Result<(String, Vec<(String, String)>), Unsendable> {
    let named = |at: u32| -> String {
        config
            .and_then(|config| config.fields.get(at as usize))
            .map(|field| field.variable.clone())
            .unwrap_or_default()
    };
    let (endpoint, from) = match plan.endpoint {
        Some(at) => (
            values
                .and_then(|values| values.text_at(at))
                .map(str::to_string),
            named(at),
        ),
        None => (
            variable(jadpo_std::telemetry::ENDPOINT_VARIABLE),
            jadpo_std::telemetry::ENDPOINT_VARIABLE.to_string(),
        ),
    };
    let Some(endpoint) = endpoint.filter(|endpoint| !endpoint.trim().is_empty()) else {
        return Err(Unsendable::NoEndpoint { variable: from });
    };
    let endpoint = endpoint.trim().trim_end_matches('/').to_string();
    if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
        return Err(Unsendable::EndpointInvalid { variable: from });
    }
    let headers = match &plan.credential {
        Some(credential) => values
            .and_then(|values| values.text_at(credential.value))
            .map(|value| vec![(credential.header.clone(), value.to_string())])
            .unwrap_or_default(),
        None => match variable(jadpo_std::telemetry::HEADERS_VARIABLE) {
            Some(text) => headers_of(&text).ok_or(Unsendable::HeadersInvalid)?,
            None => Vec::new(),
        },
    };
    Ok((endpoint, headers))
}

/// `OTEL_EXPORTER_OTLP_HEADERS`' pairs: `name=value`, comma-separated,
/// each value percent-decoded, as the specification writes them.
pub fn headers_of(text: &str) -> Option<Vec<(String, String)>> {
    let mut out = Vec::new();
    for pair in text.split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        let (name, value) = pair.split_once('=')?;
        let name = name.trim();
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
        {
            return None;
        }
        out.push((name.to_string(), percent_decoded(value.trim())?));
    }
    Some(out)
}

fn percent_decoded(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        if byte == b'%' {
            let hex = text.get(at + 1..at + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            at += 3;
        } else {
            out.push(byte);
            at += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// An endpoint as a log names it: its scheme, host and port, never a
/// name and password written into it, nor its path.
pub fn origin(endpoint: &str) -> String {
    let (scheme, rest) = endpoint.split_once("://").unwrap_or(("", endpoint));
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    format!("{scheme}://{host}")
}

/// One invocation, as [`Telemetry::answered`] sends it.
pub struct Invocation<'a> {
    /// A request's route, or the work's own name.
    pub what: What<'a>,
    /// The trace it was part of, and the span it continued, when the
    /// caller's `traceparent` named one.
    pub trace: Option<crate::outbox::Trace>,
    pub parent: Option<[u8; 8]>,
    /// When it started, in milliseconds since 1970 on the host's clock,
    /// and on its monotonic clock, which its spans are timed on.
    pub start_ms: i64,
    pub began: Option<u64>,
    /// How long it took, in milliseconds.
    pub ms: u64,
    pub spans: &'a Spans,
    pub recorded: &'a Recorded,
    /// Its failure's code, and its fault's classification.
    pub failure: Option<&'a str>,
    pub fault: Option<&'static str>,
    pub program: &'a dyn Executor,
}

impl std::fmt::Debug for Invocation<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Invocation")
            .field("what", &self.what)
            .field("ms", &self.ms)
            .field("spans", &self.spans.len())
            .finish_non_exhaustive()
    }
}

/// What an invocation was.
#[derive(Clone, Copy, Debug)]
pub enum What<'a> {
    /// A request, by its route's label (`POST /todos`), or by its method
    /// and path when no route took it; its status, and its request id.
    Request {
        method: &'a str,
        route: Option<&'a str>,
        status: u16,
        request_id: Option<&'a str>,
    },
}

impl What<'_> {
    /// Its root span's name, and the key a path is sampled by until
    /// requests carry path IDs: the route's label, or the method alone.
    pub fn name(&self) -> String {
        match self {
            What::Request { method, route, .. } => {
                route.map_or_else(|| method.to_string(), str::to_string)
            }
        }
    }

    /// The identity key thresholds name what they watch by.
    fn watched(&self) -> Option<String> {
        match self {
            What::Request { route, .. } => route.map(|route| format!("route {route}")),
        }
    }
}

/// A log event kept for the next send.
#[derive(Clone, Debug, PartialEq)]
pub struct LogRecord {
    pub time_ms: i64,
    pub level: Level,
    pub event: String,
    pub fields: Map<String, Json>,
}

/// What the runtime keeps between sends, on the thread it serves on.
#[derive(Default)]
struct State {
    logs: Vec<LogRecord>,
    dropped_logs: u32,
    /// Each path a trace has gone for in this process.
    seen: BTreeSet<String>,
    /// Each path's sampling window: when it started, and how many
    /// ordinary traces went in it.
    windows: BTreeMap<String, (i64, u32)>,
}

/// A program's telemetry on one thread: see the module.
pub struct Telemetry {
    sending: Option<Sending>,
    keep: Option<Rc<dyn Keep>>,
    resource: Resource,
    state: RefCell<State>,
    /// How many sends failed since the host's log last said so.
    failed: Rc<std::cell::Cell<u32>>,
}

impl std::fmt::Debug for Telemetry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Telemetry")
            .field("sending", &self.sending)
            .field("keeps", &self.keep.is_some())
            .field("resource", &self.resource)
            .finish_non_exhaustive()
    }
}

impl Telemetry {
    /// Telemetry that sends nowhere and keeps nothing until told.
    pub fn new(resource: Resource) -> Telemetry {
        Telemetry {
            sending: None,
            keep: None,
            resource,
            state: RefCell::default(),
            failed: Rc::default(),
        }
    }

    /// Sends as `sending` says.
    pub fn sending(mut self, sending: Sending) -> Telemetry {
        self.sending = Some(sending);
        self
    }

    /// Keeps everything in `keep` as well: `jadpo dev`'s files.
    pub fn keeping(mut self, keep: Rc<dyn Keep>) -> Telemetry {
        self.keep = Some(keep);
        self
    }

    pub fn resource(&self) -> &Resource {
        &self.resource
    }

    /// Whether it sends anywhere.
    pub fn sends(&self) -> bool {
        self.sending.is_some()
    }

    /// Keeps log event `event` with `fields` for the next send, at the
    /// level the runtime gives it ([`level_of`]), when that level is sent
    /// or kept. The runtime's own `telemetry.` events stay in the host's
    /// log alone, so a collector that can't be reached isn't told so
    /// forever.
    pub fn log(&self, time_ms: Option<i64>, event: &str, fields: &Map<String, Json>) {
        if event.starts_with("telemetry.") {
            return;
        }
        let level = level_of(event);
        let sent = self
            .sending
            .as_ref()
            .is_some_and(|sending| level >= sending.plan.send.logs);
        if !sent && self.keep.is_none() {
            return;
        }
        let mut state = self.state.borrow_mut();
        if state.logs.len() >= MOST_LOGS {
            state.dropped_logs = state.dropped_logs.saturating_add(1);
            return;
        }
        state.logs.push(LogRecord {
            time_ms: time_ms.unwrap_or_default(),
            level,
            event: event.to_string(),
            fields: fields.clone(),
        });
    }

    /// Sends what invocation `invocation` did, on `host`: its trace when
    /// `detail:` says it goes, its metrics, and the logs kept since the
    /// last send.
    pub fn answered(&self, host: &dyn Host, invocation: &Invocation<'_>) {
        self.said_failures(host);
        let plan = invocation.program.plan();
        let metrics = otlp::metrics(&self.resource, invocation, plan);
        let whole = self.sampled(invocation);
        let trace = invocation
            .trace
            .map(|trace| otlp::traces(&self.resource, invocation, trace, &mut ids(host)));
        let logs = self.take_logs();
        let request = match invocation.what {
            What::Request { request_id, .. } => request_id,
        };
        let tied = invocation.trace.zip(request);
        let logs = (!logs.is_empty()).then(|| otlp::logs(&self.resource, &logs, tied));
        if let Some(keep) = &self.keep {
            if let Some(trace) = &trace {
                keep.kept(Signal::Traces, trace);
            }
            keep.kept(Signal::Metrics, &metrics);
            if let Some(logs) = &logs {
                keep.kept(Signal::Logs, logs);
            }
        }
        let Some(sending) = &self.sending else {
            return;
        };
        let mut payloads = Vec::new();
        if sending.plan.send.traces
            && whole
            && let Some(trace) = trace
        {
            payloads.push((Signal::Traces, trace));
        }
        if sending.plan.send.metrics {
            payloads.push((Signal::Metrics, metrics));
        }
        if let Some(logs) = self.sent_logs(logs) {
            payloads.push((Signal::Logs, logs));
        }
        self.send(host, payloads);
    }

    /// Sends the logs kept since the last send, when there are any: what
    /// a tick does, so a quiet server's logs still go.
    pub fn flush(&self, host: &dyn Host) {
        self.said_failures(host);
        let logs = self.take_logs();
        if logs.is_empty() {
            return;
        }
        let payload = otlp::logs(&self.resource, &logs, None);
        if let Some(keep) = &self.keep {
            keep.kept(Signal::Logs, &payload);
        }
        if let Some(payload) = self.sent_logs(Some(payload)) {
            self.send(host, vec![(Signal::Logs, payload)]);
        }
    }

    /// Tells the host's log how many sends failed since it last did, as
    /// `telemetry.send_failed`: the count and the endpoint, never a
    /// header.
    fn said_failures(&self, host: &dyn Host) {
        let failed = self.failed.replace(0);
        if failed == 0 {
            return;
        }
        let mut fields = Map::new();
        fields.insert("failed".into(), Json::from(failed));
        if let Some(sending) = &self.sending {
            fields.insert("endpoint".into(), Json::from(origin(&sending.endpoint)));
        }
        host.log("telemetry.send_failed", fields);
    }

    fn take_logs(&self) -> Vec<LogRecord> {
        let mut state = self.state.borrow_mut();
        state.dropped_logs = 0;
        std::mem::take(&mut state.logs)
    }

    /// Of `logs`, kept at every level for `jadpo dev`, those the section
    /// sends.
    fn sent_logs(&self, logs: Option<Json>) -> Option<Json> {
        let sending = self.sending.as_ref()?;
        let logs = logs?;
        if self.keep.is_none() {
            return Some(logs);
        }
        otlp::logs_from(&logs, sending.plan.send.logs)
    }

    /// Hands each payload to the host's way out, after the response.
    fn send(&self, host: &dyn Host, payloads: Vec<(Signal, Json)>) {
        let Some(sending) = &self.sending else {
            return;
        };
        for (signal, payload) in payloads {
            let url = format!("{}{}", sending.endpoint, signal.path());
            let mut headers = vec![("content-type".to_string(), "application/json".to_string())];
            headers.extend(sending.headers.iter().cloned());
            let body = payload.to_string().into_bytes();
            let transport = Rc::clone(&sending.transport);
            let failed = Rc::clone(&self.failed);
            host.defer(Box::pin(async move {
                let sent = transport.post(&url, &headers, body, SEND_TIMEOUT_MS).await;
                if !matches!(sent, Ok(status) if (200..300).contains(&status)) {
                    failed.set(failed.get().saturating_add(1));
                }
            }));
        }
    }

    /// Whether `invocation`'s whole trace goes: it failed, faulted,
    /// crossed a threshold or took a path new to this process, as
    /// `full_traces:` lists, or it is one of the ordinary traces its path
    /// sends in `samples:`' window.
    fn sampled(&self, invocation: &Invocation<'_>) -> bool {
        let Some(sending) = &self.sending else {
            return true;
        };
        let detail = &sending.plan.detail;
        let path = invocation.what.name();
        let mut state = self.state.borrow_mut();
        let new = state.seen.insert(path.clone());
        if detail.full(Full::Failures) && invocation.failure.is_some() {
            return true;
        }
        if detail.full(Full::Faults) && invocation.fault.is_some() {
            return true;
        }
        if detail.full(Full::Slow) && slow(invocation) {
            return true;
        }
        if detail.full(Full::NewPaths) && new {
            return true;
        }
        let now = invocation.start_ms;
        let window = state.windows.entry(path).or_insert((now, 0));
        if now.saturating_sub(window.0) >= detail.every_ms as i64 {
            *window = (now, 0);
        }
        if window.1 < detail.per_path {
            window.1 += 1;
            return true;
        }
        false
    }
}

/// Whether `invocation` went past a threshold's per-request limit on what
/// it watches: its route's, or an action's it ran.
fn slow(invocation: &Invocation<'_>) -> bool {
    let thresholds = invocation.program.plan().thresholds;
    if thresholds.is_empty() {
        return false;
    }
    let queries = invocation
        .spans
        .iter()
        .filter(|span| {
            matches!(
                span.kind,
                crate::spans::Kind::Read | crate::spans::Kind::Write
            )
        })
        .count() as u64;
    if let Some(watched) = invocation.what.watched()
        && jadpo_std::thresholds::limits_exceeded(thresholds, &watched, invocation.ms, queries)
            .next()
            .is_some()
    {
        return true;
    }
    let places = invocation.program.places();
    invocation.spans.iter().any(|span| {
        span.kind == crate::spans::Kind::Action
            && span.callable.is_some_and(|callable| {
                let key = places.label(callable).to_string();
                jadpo_std::thresholds::limits_exceeded(
                    thresholds,
                    &key,
                    span.ms().unwrap_or_default(),
                    0,
                )
                .next()
                .is_some()
            })
    })
}

/// A source of span ids: the host's randomness, or a count when it has
/// none.
fn ids(host: &dyn Host) -> impl FnMut() -> [u8; 8] + '_ {
    let mut count = 0u64;
    move || {
        let mut bytes = [0u8; 8];
        if !host.random(&mut bytes) || bytes == [0; 8] {
            count += 1;
            bytes = count.to_be_bytes();
        }
        bytes
    }
}

/// The level each of the runtime's log events is sent at: a fault behind
/// a 500 an error, what a person or an operator should look at a warning,
/// the access records and what the runtime did information.
pub fn level_of(event: &str) -> Level {
    match event {
        "operation.failed" | "runtime.start_failed" => Level::Error,
        "failure.rejected" => Level::Info,
        _ if event.ends_with(".alert") || event.ends_with("_failed") => Level::Warn,
        "rate_limits.fallback" | "telemetry.value_refused" => Level::Warn,
        _ => Level::Info,
    }
}

#[cfg(test)]
mod tests;
