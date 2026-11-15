//! OTLP's JSON encoding (OpenTelemetry protocol 1.x, "JSON Protobuf
//! Encoding"): one invocation's trace, metrics and logs as the payloads
//! `/v1/traces`, `/v1/metrics` and `/v1/logs` take. Trace and span ids are
//! lowercase hex, 64-bit integers and times decimal strings, and enums
//! their numbers, as the encoding writes them. Metrics are sent with delta
//! temporality, one invocation's worth at a time, which a collector adds
//! up: so a Workers isolate, which keeps nothing between invocations for
//! sure, sends what a server does.

use std::collections::BTreeMap;

use jadpo_std::telemetry::{Level, MetricKind};
use serde_json::{Value as Json, json};

use super::{Attr, Invocation, LogRecord, Number, Resource, What};
use crate::Plan;
use crate::spans::{Ended, Kind, Span};

/// OpenTelemetry's recommended bounds for an HTTP server's durations, in
/// seconds: every duration histogram's, and its threshold's besides.
pub const BOUNDS_S: [f64; 14] = [
    0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.25, 0.5, 0.75, 1.0, 2.5, 5.0, 7.5, 10.0,
];

/// The bounds a `Measure` of the app's own is kept in.
pub const MEASURE_BOUNDS: [f64; 15] = [
    0.0, 5.0, 10.0, 25.0, 50.0, 75.0, 100.0, 250.0, 500.0, 750.0, 1000.0, 2500.0, 5000.0, 7500.0,
    10000.0,
];

/// `SpanKind`s.
const INTERNAL: u8 = 1;
const SERVER: u8 = 2;
const CLIENT: u8 = 3;

/// `StatusCode.ERROR`.
const ERROR: u8 = 2;

/// `AggregationTemporality.DELTA`.
const DELTA: u8 = 1;

/// The codes the runtime raises when the fault is the server's, not the
/// caller's: what `jadpo.outcome` calls a server failure.
const SERVER_FAILURES: [&str; 5] = [
    "unavailable",
    "outcome_unknown",
    "misconfigured",
    "internal_fault",
    "deadline_exceeded",
];

/// Sets `key` of the object `value` to `to`.
fn set(value: &mut Json, key: &str, to: Json) {
    if let Some(object) = value.as_object_mut() {
        object.insert(key.to_string(), to);
    }
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn nanos(ms: i64) -> String {
    (i128::from(ms) * 1_000_000).max(0).to_string()
}

/// An attribute: `{ "key": ..., "value": { "stringValue": ... } }`.
pub fn attribute(key: &str, value: &Attr) -> Json {
    json!({ "key": key, "value": any(value) })
}

fn any(value: &Attr) -> Json {
    match value {
        Attr::Text(text) => json!({ "stringValue": text }),
        Attr::Int(int) => json!({ "intValue": int.to_string() }),
        Attr::Double(double) => json!({ "doubleValue": double }),
        Attr::Bool(bool) => json!({ "boolValue": bool }),
    }
}

/// A JSON value as an `AnyValue`: what a log event's fields become.
fn any_json(value: &Json) -> Json {
    match value {
        Json::Null => json!({}),
        Json::Bool(bool) => json!({ "boolValue": bool }),
        Json::Number(number) => match number.as_i64() {
            Some(int) => json!({ "intValue": int.to_string() }),
            None => json!({ "doubleValue": number.as_f64().unwrap_or_default() }),
        },
        Json::String(text) => json!({ "stringValue": text }),
        Json::Array(items) => {
            json!({ "arrayValue": { "values": items.iter().map(any_json).collect::<Vec<_>>() } })
        }
        Json::Object(map) => json!({ "kvlistValue": { "values": map
            .iter()
            .map(|(key, value)| json!({ "key": key, "value": any_json(value) }))
            .collect::<Vec<_>>() } }),
    }
}

fn text(key: &str, value: &str) -> Json {
    attribute(key, &Attr::Text(value.to_string()))
}

fn int(key: &str, value: i64) -> Json {
    attribute(key, &Attr::Int(value))
}

/// The resource every payload names, and the scope.
fn resource(resource: &Resource) -> Json {
    json!({ "attributes": [
        text("service.name", &resource.service),
        text("service.version", &resource.compiler),
        text("service.instance.id", &resource.instance),
        text("telemetry.sdk.name", "jadpo"),
        text("telemetry.sdk.language", "rust"),
        text("telemetry.sdk.version", &resource.compiler),
        text("jadpo.build", &resource.build),
        text("jadpo.target", &resource.target),
    ] })
}

fn scope(resource: &Resource) -> Json {
    json!({ "name": "jadpo", "version": resource.compiler })
}

/// A route's label split into its method and its path as written:
/// `POST /todos/{id}`.
fn route_parts(route: &str) -> (&str, &str) {
    route.split_once(' ').unwrap_or((route, ""))
}

/// What a callable's label names, without its kind: `create_todo`.
fn bare(label: &str) -> &str {
    label.split_once(' ').map_or(label, |(_, name)| name)
}

/// A callable by the name its metrics carry: its domain's and its own,
/// `hello.greet`, from where it is declared.
fn qualified(file: &str, label: &str) -> String {
    let name = bare(label).trim_matches('"');
    match file
        .strip_prefix("domains/")
        .and_then(|rest| rest.split('/').next())
    {
        Some(domain) => format!("{domain}.{name}"),
        None => name.to_string(),
    }
}

/// The span's kind's word and the name it is sent under, with its kind.
fn named(span: &Span, invocation: &Invocation<'_>) -> (&'static str, String, u8) {
    let places = invocation.program.places();
    let label = span.callable.map(|callable| places.label(callable));
    match &span.kind {
        Kind::Action => (
            "action",
            label.map_or_else(|| "action".to_string(), |label| bare(label).to_string()),
            INTERNAL,
        ),
        Kind::Read => match label {
            Some(label) if label.starts_with("query ") => ("read", bare(label).to_string(), CLIENT),
            _ => ("read", "read".to_string(), CLIENT),
        },
        Kind::Write => ("write", "write".to_string(), CLIENT),
        Kind::Call {
            connection,
            operation,
        } => ("call", format!("{connection}.{operation}"), CLIENT),
    }
}

/// `invocation`'s trace: its root span, then a span for each of its
/// spans, nested as they ran, each with an id from `id`.
pub fn traces(
    resource_of: &Resource,
    invocation: &Invocation<'_>,
    trace: crate::outbox::Trace,
    id: &mut dyn FnMut() -> [u8; 8],
) -> Json {
    let trace_id = hex(&trace.trace_id);
    let root_id = hex(&trace.span_id);
    let start = invocation.start_ms;
    let end = start.saturating_add(invocation.ms as i64);
    let places = invocation.program.places();
    let mut out = Vec::with_capacity(invocation.spans.len() + 1);
    let mut root = json!({
        "traceId": trace_id,
        "spanId": root_id,
        "name": invocation.what.name(),
        "kind": SERVER,
        "startTimeUnixNano": nanos(start),
        "endTimeUnixNano": nanos(end),
        "attributes": root_attributes(invocation),
        "status": {},
    });
    if let Some(parent) = invocation.parent {
        set(&mut root, "parentSpanId", Json::from(hex(&parent)));
    }
    let failed_on_server = match invocation.what {
        What::Request { status, .. } => status >= 500,
    };
    if failed_on_server || invocation.fault.is_some() {
        set(
            &mut root,
            "status",
            json!({ "code": ERROR, "message": invocation.fault.or(invocation.failure).unwrap_or("server failure") }),
        );
    }
    out.push(root);
    let at = |ms: Option<u64>| -> i64 {
        match (ms, invocation.began) {
            (Some(ms), Some(began)) => start.saturating_add(ms.saturating_sub(began) as i64),
            _ => start,
        }
    };
    let ids: Vec<String> = invocation.spans.iter().map(|_| hex(&id())).collect();
    for (place, span) in invocation.spans.iter().enumerate() {
        let (kind, name, span_kind) = named(span, invocation);
        let parent = span
            .parent
            .and_then(|parent| ids.get(parent as usize))
            .unwrap_or(&root_id);
        let mut attributes = vec![text("jadpo.kind", kind)];
        if let Some(callable) = span.callable {
            let label = places.label(callable);
            if let Some(callable_place) = places.callable(callable)
                && (kind == "action" || label.starts_with("query "))
            {
                attributes.push(text("jadpo.name", &qualified(&callable_place.file, label)));
                // Its stable identity, as a threshold names what it watches.
                attributes.push(text("jadpo.identity", label));
            }
            // Where it is in the source, which the build's manifest
            // decodes: `domains/greeting/actions/greet.jadpo:4`.
            if let Some(frame) = span.frame()
                && let Some(found) = places.place(frame)
            {
                attributes.push(text(
                    "jadpo.source",
                    &format!("{}:{}", found.file, found.line),
                ));
            }
        }
        if let Kind::Call {
            connection,
            operation,
        } = &span.kind
        {
            attributes.push(text("jadpo.connection", connection));
            attributes.push(text("jadpo.operation", operation));
        }
        if let Some(rows) = span.rows {
            attributes.push(int("jadpo.rows", rows as i64));
        }
        let mut status = json!({});
        match &span.ended {
            Ended::Failed(code) => {
                attributes.push(text("jadpo.failure", code));
                if SERVER_FAILURES.contains(&code.as_str()) {
                    status = json!({ "code": ERROR, "message": code });
                }
            }
            Ended::Fault(classification) => {
                attributes.push(text("jadpo.fault", classification));
                status = json!({ "code": ERROR, "message": classification });
            }
            Ended::Open => attributes.push(attribute("jadpo.abandoned", &Attr::Bool(true))),
            Ended::Value => {}
        }
        out.push(json!({
            "traceId": trace_id,
            "spanId": ids.get(place).cloned().unwrap_or_default(),
            "parentSpanId": parent,
            "name": name,
            "kind": span_kind,
            "startTimeUnixNano": nanos(at(span.start_ms)),
            "endTimeUnixNano": nanos(at(span.end_ms.or(span.start_ms)).max(at(span.start_ms))),
            "attributes": attributes,
            "status": status,
        }));
    }
    json!({ "resourceSpans": [{
        "resource": resource(resource_of),
        "scopeSpans": [{ "scope": scope(resource_of), "spans": out }],
    }] })
}

/// The root span's attributes: the request's method, route and status,
/// its request id and failure, and the app's notes.
fn root_attributes(invocation: &Invocation<'_>) -> Vec<Json> {
    let mut out = Vec::new();
    match invocation.what {
        What::Request {
            method,
            route,
            status,
            request_id,
        } => {
            out.push(text("http.request.method", method));
            if let Some(route) = route {
                out.push(text("http.route", route_parts(route).1));
            }
            out.push(int("http.response.status_code", i64::from(status)));
            if let Some(id) = request_id {
                out.push(text("jadpo.request_id", id));
            }
        }
    }
    if let Some(code) = invocation.fault.or(invocation.failure) {
        out.push(text("error.type", code));
    }
    if let Some(code) = invocation.failure {
        out.push(text("jadpo.failure", code));
    }
    if invocation.spans.dropped() > 0 {
        out.push(int(
            "jadpo.spans_dropped",
            i64::from(invocation.spans.dropped()),
        ));
    }
    for (name, value) in &invocation.recorded.notes {
        out.push(attribute(name, value));
    }
    out
}

/// One metric's data points, by their attributes, as they are added up
/// within an invocation.
#[derive(Default)]
struct Series {
    unit: Option<String>,
    description: Option<String>,
    kind: Option<Shape>,
    points: BTreeMap<String, (Vec<Json>, Point)>,
}

#[derive(Clone, Copy, PartialEq)]
enum Shape {
    Histogram,
    Sum,
    Gauge,
}

#[derive(Clone)]
enum Point {
    Histogram {
        bounds: Vec<f64>,
        counts: Vec<u64>,
        sum: f64,
        count: u64,
        min: f64,
        max: f64,
    },
    Sum(Number),
    Gauge(Number),
}

/// Every metric an invocation sends, by name.
struct Metrics(BTreeMap<String, Series>);

impl Metrics {
    fn series(&mut self, name: &str, shape: Shape, unit: Option<&str>) -> &mut Series {
        let series = self.0.entry(name.to_string()).or_default();
        series.kind.get_or_insert(shape);
        if series.unit.is_none() {
            series.unit = unit.map(str::to_string);
        }
        series
    }

    fn histogram(
        &mut self,
        name: &str,
        unit: &str,
        attributes: Vec<Json>,
        value: f64,
        bounds: &[f64],
    ) {
        let series = self.series(name, Shape::Histogram, Some(unit));
        let key = Json::Array(attributes.clone()).to_string();
        let entry = series.points.entry(key).or_insert_with(|| {
            (
                attributes,
                Point::Histogram {
                    bounds: bounds.to_vec(),
                    counts: vec![0; bounds.len() + 1],
                    sum: 0.0,
                    count: 0,
                    min: f64::INFINITY,
                    max: f64::NEG_INFINITY,
                },
            )
        });
        if let Point::Histogram {
            bounds,
            counts,
            sum,
            count,
            min,
            max,
        } = &mut entry.1
        {
            let bucket = bounds
                .iter()
                .position(|bound| value <= *bound)
                .unwrap_or(bounds.len());
            if let Some(counted) = counts.get_mut(bucket) {
                *counted += 1;
            }
            *sum += value;
            *count += 1;
            *min = min.min(value);
            *max = max.max(value);
        }
    }

    fn sum(&mut self, name: &str, unit: Option<&str>, attributes: Vec<Json>, value: Number) {
        let series = self.series(name, Shape::Sum, unit);
        let key = Json::Array(attributes.clone()).to_string();
        let entry = series
            .points
            .entry(key)
            .or_insert_with(|| (attributes, Point::Sum(Number::Int(0))));
        if let Point::Sum(total) = &mut entry.1 {
            *total = match (*total, value) {
                (Number::Int(a), Number::Int(b)) => Number::Int(a.saturating_add(b)),
                (a, b) => Number::Double(a.as_f64() + b.as_f64()),
            };
        }
    }

    fn gauge(&mut self, name: &str, unit: Option<&str>, attributes: Vec<Json>, value: Number) {
        let series = self.series(name, Shape::Gauge, unit);
        let key = Json::Array(attributes.clone()).to_string();
        series.points.insert(key, (attributes, Point::Gauge(value)));
    }

    fn json(self, start: i64, end: i64) -> Vec<Json> {
        self.0
            .into_iter()
            .filter_map(|(name, series)| {
                let shape = series.kind?;
                let points: Vec<Json> = series
                    .points
                    .into_values()
                    .map(|(attributes, point)| {
                        let mut out = json!({
                            "attributes": attributes,
                            "startTimeUnixNano": nanos(start),
                            "timeUnixNano": nanos(end),
                        });
                        match point {
                            Point::Histogram {
                                bounds,
                                counts,
                                sum,
                                count,
                                min,
                                max,
                            } => {
                                set(&mut out, "count", Json::from(count.to_string()));
                                set(&mut out, "sum", Json::from(sum));
                                set(&mut out, "min", Json::from(min));
                                set(&mut out, "max", Json::from(max));
                                set(
                                    &mut out,
                                    "bucketCounts",
                                    Json::from(
                                        counts.iter().map(u64::to_string).collect::<Vec<_>>(),
                                    ),
                                );
                                set(&mut out, "explicitBounds", Json::from(bounds));
                            }
                            Point::Sum(value) | Point::Gauge(value) => match value {
                                Number::Int(int) => {
                                    set(&mut out, "asInt", Json::from(int.to_string()))
                                }
                                Number::Double(double) => {
                                    set(&mut out, "asDouble", Json::from(double))
                                }
                            },
                        }
                        out
                    })
                    .collect();
                let mut metric = json!({ "name": name });
                if let Some(unit) = series.unit {
                    set(&mut metric, "unit", Json::from(unit));
                }
                if let Some(description) = series.description {
                    set(&mut metric, "description", Json::from(description));
                }
                match shape {
                    Shape::Histogram => {
                        let data = json!({
                            "dataPoints": points,
                            "aggregationTemporality": DELTA,
                        });
                        set(&mut metric, "histogram", data);
                    }
                    Shape::Sum => {
                        let data = json!({
                            "dataPoints": points,
                            "aggregationTemporality": DELTA,
                            "isMonotonic": true,
                        });
                        set(&mut metric, "sum", data);
                    }
                    Shape::Gauge => set(&mut metric, "gauge", json!({ "dataPoints": points })),
                }
                Some(metric)
            })
            .collect()
    }
}

/// The bounds of a duration histogram of what `watched` names: the
/// standard ones, and each duration a threshold on it names.
fn bounds(plan: Plan<'_>, watched: &str) -> Vec<f64> {
    let mut out: Vec<f64> = BOUNDS_S.to_vec();
    if let Some(buckets) = plan.buckets.and_then(|buckets| buckets.get(watched)) {
        out.extend(buckets.iter().map(|ms| *ms as f64 / 1000.0));
    }
    out.sort_by(f64::total_cmp);
    out.dedup();
    out
}

/// Whether a span's end is the server's failure, a domain failure or a
/// value: `jadpo.outcome`.
fn outcome(ended: &Ended) -> &'static str {
    match ended {
        Ended::Value => "value",
        Ended::Failed(code) if SERVER_FAILURES.contains(&code.as_str()) => "server_failure",
        Ended::Failed(_) => "failure",
        Ended::Fault(_) => "server_failure",
        Ended::Open => "abandoned",
    }
}

/// `invocation`'s metrics: its request's duration and failure, each
/// action's duration and outcome, each read's and write's, each
/// provider call, a rate limit's refusal, and what the app's code
/// recorded.
pub fn metrics(resource_of: &Resource, invocation: &Invocation<'_>, plan: Plan<'_>) -> Json {
    let mut metrics = Metrics(BTreeMap::new());
    let places = invocation.program.places();
    match invocation.what {
        What::Request {
            method,
            route,
            status,
            ..
        } => {
            let mut attributes = vec![text("http.request.method", method)];
            if let Some(route) = route {
                attributes.push(text("http.route", route_parts(route).1));
            }
            attributes.push(int("http.response.status_code", i64::from(status)));
            if let Some(code) = invocation.fault.or(invocation.failure) {
                attributes.push(text("error.type", code));
            }
            let watched = route
                .map(|route| format!("route {route}"))
                .unwrap_or_default();
            metrics.histogram(
                "http.server.request.duration",
                "s",
                attributes,
                invocation.ms as f64 / 1000.0,
                &bounds(plan, &watched),
            );
            if let Some(code) = invocation.failure {
                let mut attributes = vec![text("jadpo.failure", code)];
                if let Some(route) = route {
                    attributes.push(text("http.route", route_parts(route).1));
                }
                metrics.sum("jadpo.failures", None, attributes.clone(), Number::Int(1));
                if code == "rate_limited" {
                    metrics.sum(
                        "jadpo.rate_limit.refusals",
                        None,
                        attributes,
                        Number::Int(1),
                    );
                }
            }
        }
    }
    for span in invocation.spans {
        let seconds = span.ms().unwrap_or_default() as f64 / 1000.0;
        let label = span.callable.map(|callable| places.label(callable));
        let file = span
            .callable
            .and_then(|callable| places.callable(callable))
            .map(|place| place.file.as_str())
            .unwrap_or_default();
        match &span.kind {
            Kind::Action => {
                let Some(label) = label else { continue };
                let attributes = vec![
                    text("jadpo.kind", "action"),
                    text("jadpo.name", &qualified(file, label)),
                    text("jadpo.outcome", outcome(&span.ended)),
                ];
                metrics.histogram(
                    "jadpo.run.duration",
                    "s",
                    attributes,
                    seconds,
                    &bounds(plan, label),
                );
            }
            Kind::Read | Kind::Write => {
                let name = match label {
                    Some(label) if label.starts_with("query ") => qualified(file, label),
                    _ => String::new(),
                };
                let mut attributes = vec![text(
                    "jadpo.kind",
                    if span.kind == Kind::Read {
                        "read"
                    } else {
                        "write"
                    },
                )];
                if !name.is_empty() {
                    attributes.push(text("jadpo.name", &name));
                }
                attributes.push(text("jadpo.outcome", outcome(&span.ended)));
                metrics.histogram("jadpo.query.duration", "s", attributes, seconds, &BOUNDS_S);
            }
            Kind::Call {
                connection,
                operation,
            } => {
                let attributes = vec![
                    text("jadpo.connection", connection),
                    text("jadpo.operation", operation),
                    text("jadpo.outcome", outcome(&span.ended)),
                ];
                metrics.sum(
                    "jadpo.provider.calls",
                    None,
                    attributes.clone(),
                    Number::Int(1),
                );
                metrics.histogram(
                    "jadpo.provider.call.duration",
                    "s",
                    attributes,
                    seconds,
                    &BOUNDS_S,
                );
            }
        }
    }
    for point in &invocation.recorded.points {
        let declared = plan
            .metrics
            .iter()
            .find(|metric| metric.name == point.metric);
        let unit = declared.and_then(|metric| metric.unit.as_deref());
        let attributes: Vec<Json> = point
            .labels
            .iter()
            .map(|(name, value)| attribute(name, value))
            .collect();
        match point.kind {
            MetricKind::Counter => metrics.sum(&point.metric, unit, attributes, point.value),
            MetricKind::Measure => metrics.histogram(
                &point.metric,
                unit.unwrap_or("1"),
                attributes,
                point.value.as_f64(),
                &MEASURE_BOUNDS,
            ),
            MetricKind::Gauge => metrics.gauge(&point.metric, unit, attributes, point.value),
        }
        if let Some(series) = metrics.0.get_mut(&point.metric)
            && series.description.is_none()
        {
            series.description = declared.and_then(|metric| metric.description.clone());
        }
    }
    let start = invocation.start_ms;
    let end = start.saturating_add(invocation.ms as i64);
    json!({ "resourceMetrics": [{
        "resource": resource(resource_of),
        "scopeMetrics": [{ "scope": scope(resource_of), "metrics": metrics.json(start, end) }],
    }] })
}

/// `records` as one logs payload, each tied to the trace of the request
/// `trace` names when the record names that request.
pub fn logs(
    resource_of: &Resource,
    records: &[LogRecord],
    trace: Option<(crate::outbox::Trace, &str)>,
) -> Json {
    let records: Vec<Json> = records
        .iter()
        .map(|record| {
            let mut attributes = vec![text("event.name", &record.event)];
            attributes.extend(
                record
                    .fields
                    .iter()
                    .map(|(key, value)| json!({ "key": key, "value": any_json(value) })),
            );
            let mut out = json!({
                "timeUnixNano": nanos(record.time_ms),
                "observedTimeUnixNano": nanos(record.time_ms),
                "severityNumber": record.level.severity_number(),
                "severityText": record.level.severity_text(),
                "body": { "stringValue": record.event },
                "attributes": attributes,
            });
            // Tied to the request's trace only when the event names that
            // request: another request on the thread may have logged it.
            if let Some((trace, id)) = trace
                && record.fields.get("request_id").and_then(Json::as_str) == Some(id)
            {
                set(&mut out, "traceId", Json::from(hex(&trace.trace_id)));
                set(&mut out, "spanId", Json::from(hex(&trace.span_id)));
            }
            out
        })
        .collect();
    json!({ "resourceLogs": [{
        "resource": resource(resource_of),
        "scopeLogs": [{ "scope": scope(resource_of), "logRecords": records }],
    }] })
}

/// Of the logs payload `payload`, the records at `floor_level` or above, or
/// `None` when none is.
pub fn logs_from(payload: &Json, floor_level: Level) -> Option<Json> {
    let mut payload = payload.clone();
    let mut any = false;
    for resource in payload.get_mut("resourceLogs")?.as_array_mut()? {
        for scope in resource.get_mut("scopeLogs")?.as_array_mut()? {
            let records = scope.get_mut("logRecords")?.as_array_mut()?;
            records.retain(|record| {
                record
                    .get("severityNumber")
                    .and_then(Json::as_u64)
                    .unwrap_or_default()
                    >= u64::from(floor_level.severity_number())
            });
            any |= !records.is_empty();
        }
    }
    any.then_some(payload)
}
