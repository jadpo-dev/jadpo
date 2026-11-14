//! Traces, metrics and logs in development (area 17, "Traces, metrics and
//! logs in development"; area 16, "Telemetry"; chunk O1): what `jadpo
//! dev` keeps of its app's telemetry on this machine, and the commands
//! that read it.
//!
//! - [`DevTelemetry`] keeps each request's OTLP payloads, the same bytes a
//!   collector would be sent, one a line in `build/dev/telemetry/`:
//!   `traces.jsonl`, `metrics.jsonl` and `logs.jsonl`, the last
//!   [`KEPT`] of each. Every trace is kept, whatever `detail:` sends,
//!   and every log at every level.
//! - `jadpo traces` lists the requests, one line each; `jadpo metrics`
//!   adds up the metrics of the requests listed, a line a series; `jadpo
//!   logs` lists the log events, with their request. Each takes the same
//!   filters, `--route`, `--action`, `--domain`, `--failure`,
//!   `--slower-than` and `--request`, which choose requests: metrics and
//!   logs are those of the requests chosen. `--follow` keeps printing as
//!   new ones arrive, and `--json` gives them as data.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use jadpo_diag::{Diagnostic, rule};
use jadpo_runtime::telemetry::{Keep, Signal};
use jadpo_term::{
    Block, Cell, ColorChoice, Environment, Fact, Json, Outcome, Screen, Stream, Style, Symbol,
    Table, Token,
};
use serde_json::Value;

use crate::run::{Call, Reply};

/// Where `jadpo dev` keeps its telemetry, relative to the project.
pub(crate) const FOLDER: &str = "build/dev/telemetry";

/// How many payloads of each signal it keeps.
pub(crate) const KEPT: usize = 500;

/// The file a signal is kept in.
pub(crate) fn file(root: &Path, signal: Signal) -> PathBuf {
    root.join(FOLDER).join(format!("{}.jsonl", signal.word()))
}

// --- keeping ------------------------------------------------------------------

/// `jadpo dev`'s keeping of its telemetry, which every worker shares.
#[derive(Clone)]
pub(crate) struct DevTelemetry {
    root: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl DevTelemetry {
    pub(crate) fn new(root: &Path, lock: Arc<Mutex<()>>) -> DevTelemetry {
        DevTelemetry {
            root: root.to_path_buf(),
            lock,
        }
    }
}

impl Keep for DevTelemetry {
    fn kept(&self, signal: Signal, payload: &Value) {
        let Ok(_held) = self.lock.lock() else {
            return;
        };
        let path = file(&self.root, signal);
        if let Some(folder) = path.parent() {
            let _ = std::fs::create_dir_all(folder);
        }
        let appended = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut file| file.write_all(format!("{payload}\n").as_bytes()));
        if appended.is_err() {
            return;
        }
        // Past the limit by a fifth, the oldest go.
        if let Ok(text) = std::fs::read_to_string(&path) {
            let lines: Vec<&str> = text.lines().collect();
            if lines.len() > KEPT + KEPT / 5 {
                let mut out = lines[lines.len() - KEPT..].join("\n");
                out.push('\n');
                let partial = path.with_extension("jsonl.partial");
                if std::fs::write(&partial, out).is_ok() {
                    let _ = std::fs::rename(&partial, &path);
                }
            }
        }
    }
}

// --- reading ------------------------------------------------------------------

/// Each payload kept of `signal`, oldest first.
fn payloads(root: &Path, signal: Signal) -> Vec<Value> {
    let Ok(text) = std::fs::read_to_string(file(root, signal)) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// An OTLP attribute list as a map of its values, each as plain JSON.
fn attributes(list: &Value) -> BTreeMap<String, Value> {
    list.as_array()
        .into_iter()
        .flatten()
        .filter_map(|attribute| {
            let key = attribute.get("key")?.as_str()?.to_string();
            Some((key, plain(attribute.get("value")?)))
        })
        .collect()
}

/// An OTLP `AnyValue` as plain JSON.
fn plain(value: &Value) -> Value {
    if let Some(text) = value.get("stringValue") {
        return text.clone();
    }
    if let Some(bool) = value.get("boolValue") {
        return bool.clone();
    }
    if let Some(int) = value.get("intValue") {
        return int
            .as_str()
            .and_then(|text| text.parse::<i64>().ok())
            .map_or_else(|| int.clone(), Value::from);
    }
    if let Some(double) = value.get("doubleValue") {
        return double.clone();
    }
    if let Some(list) = value.get("arrayValue") {
        return Value::Array(
            list.get("values")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(plain)
                .collect(),
        );
    }
    if let Some(map) = value.get("kvlistValue") {
        return Value::Object(
            attributes(map.get("values").unwrap_or(&Value::Null))
                .into_iter()
                .collect(),
        );
    }
    Value::Null
}

fn nanos(value: &Value) -> u128 {
    value
        .as_str()
        .and_then(|text| text.parse::<u128>().ok())
        .or_else(|| value.as_u64().map(u128::from))
        .unwrap_or_default()
}

fn text(map: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    map.get(key).and_then(Value::as_str).map(str::to_string)
}

/// One span of a kept trace.
#[derive(Clone, Debug, PartialEq)]
struct Span {
    name: String,
    kind: String,
    /// `jadpo.name`: an action's or query's domain and name.
    qualified: Option<String>,
    ms: u64,
    failure: Option<String>,
    fault: Option<String>,
}

/// One request's trace, as kept.
#[derive(Clone, Debug, PartialEq)]
struct Trace {
    trace_id: String,
    request_id: Option<String>,
    name: String,
    route: Option<String>,
    status: Option<i64>,
    failure: Option<String>,
    fault: Option<String>,
    /// When it started, in milliseconds since 1970.
    at_ms: u128,
    ms: u64,
    spans: Vec<Span>,
    notes: BTreeMap<String, Value>,
}

/// The attributes the runtime gives a root span, which aren't notes.
const ROOT_ATTRIBUTES: [&str; 6] = [
    "http.request.method",
    "http.route",
    "http.response.status_code",
    "jadpo.request_id",
    "error.type",
    "jadpo.failure",
];

fn traces_of(payload: &Value) -> Vec<Trace> {
    let mut out = Vec::new();
    for resource in payload["resourceSpans"].as_array().into_iter().flatten() {
        for scope in resource["scopeSpans"].as_array().into_iter().flatten() {
            let spans: Vec<&Value> = scope["spans"].as_array().into_iter().flatten().collect();
            let Some(root) = spans.iter().find(|span| span["kind"] == 2) else {
                continue;
            };
            let attrs = attributes(&root["attributes"]);
            let start = nanos(&root["startTimeUnixNano"]);
            let end = nanos(&root["endTimeUnixNano"]);
            let fault = (root["status"]["code"] == 2)
                .then(|| root["status"]["message"].as_str().map(str::to_string))
                .flatten()
                .filter(|message| text(&attrs, "jadpo.failure").as_deref() != Some(message));
            out.push(Trace {
                trace_id: root["traceId"].as_str().unwrap_or_default().to_string(),
                request_id: text(&attrs, "jadpo.request_id"),
                name: root["name"].as_str().unwrap_or_default().to_string(),
                route: text(&attrs, "http.route"),
                status: attrs
                    .get("http.response.status_code")
                    .and_then(Value::as_i64),
                failure: text(&attrs, "jadpo.failure"),
                fault,
                at_ms: start / 1_000_000,
                ms: u64::try_from(end.saturating_sub(start) / 1_000_000).unwrap_or(u64::MAX),
                spans: spans
                    .iter()
                    .filter(|span| span["kind"] != 2)
                    .map(|span| {
                        let attrs = attributes(&span["attributes"]);
                        let start = nanos(&span["startTimeUnixNano"]);
                        let end = nanos(&span["endTimeUnixNano"]);
                        Span {
                            name: span["name"].as_str().unwrap_or_default().to_string(),
                            kind: text(&attrs, "jadpo.kind").unwrap_or_default(),
                            qualified: text(&attrs, "jadpo.name"),
                            ms: u64::try_from(end.saturating_sub(start) / 1_000_000)
                                .unwrap_or(u64::MAX),
                            failure: text(&attrs, "jadpo.failure"),
                            fault: text(&attrs, "jadpo.fault"),
                        }
                    })
                    .collect(),
                notes: attrs
                    .into_iter()
                    .filter(|(key, _)| !ROOT_ATTRIBUTES.contains(&key.as_str()))
                    .collect(),
            });
        }
    }
    out
}

/// What the commands' filters choose.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filters {
    route: Option<String>,
    action: Option<String>,
    domain: Option<String>,
    failure: Option<String>,
    slower_than: Option<(String, u64)>,
    request: Option<String>,
}

impl Filters {
    fn read(call: &Call) -> Result<Filters, Box<Diagnostic>> {
        let slower_than = match call.value("--slower-than") {
            None => None,
            Some(written) => match jadpo_std::Dur::parse(written) {
                Some(duration) if duration.millis >= 0 => {
                    Some((written.to_string(), duration.millis as u64))
                }
                _ => {
                    return Err(Box::new(arguments(
                        call,
                        format!("`{written}` isn't a duration"),
                        "`--slower-than` takes a duration, as Jadpo writes one: `200ms`, `2s`, `1m`.".to_string(),
                        "Give a duration such as `--slower-than 200ms`".to_string(),
                    )));
                }
            },
        };
        Ok(Filters {
            route: call.value("--route").map(str::to_string),
            action: call.value("--action").map(str::to_string),
            domain: call.value("--domain").map(str::to_string),
            failure: call.value("--failure").map(str::to_string),
            slower_than,
            request: call.value("--request").map(str::to_string),
        })
    }

    fn any(&self) -> bool {
        *self != Filters::default()
    }

    /// The filters given, as the empty screen repeats them.
    fn facts(&self) -> Vec<Fact> {
        [
            ("--route", self.route.clone()),
            ("--action", self.action.clone()),
            ("--domain", self.domain.clone()),
            ("--failure", self.failure.clone()),
            (
                "--slower-than",
                self.slower_than
                    .as_ref()
                    .map(|(written, _)| written.clone()),
            ),
            ("--request", self.request.clone()),
        ]
        .into_iter()
        .filter_map(|(flag, value)| Some(Fact::plain(format!("{flag} {}", value?))))
        .collect()
    }

    fn chooses(&self, trace: &Trace) -> bool {
        let named = |span: &Span, wanted: &str| {
            span.kind == "action"
                && (span.name == wanted
                    || span.qualified.as_deref().is_some_and(|name| {
                        name == wanted || name.ends_with(&format!(".{wanted}"))
                    }))
        };
        self.route
            .as_deref()
            .is_none_or(|wanted| trace.name == wanted || trace.route.as_deref() == Some(wanted))
            && self
                .action
                .as_deref()
                .is_none_or(|wanted| trace.spans.iter().any(|span| named(span, wanted)))
            && self.domain.as_deref().is_none_or(|wanted| {
                trace.spans.iter().any(|span| {
                    span.qualified
                        .as_deref()
                        .is_some_and(|name| name.starts_with(&format!("{wanted}.")))
                })
            })
            && self.failure.as_deref().is_none_or(|wanted| {
                trace.failure.as_deref() == Some(wanted) || trace.fault.as_deref() == Some(wanted)
            })
            && self
                .slower_than
                .as_ref()
                .is_none_or(|(_, ms)| trace.ms > *ms)
            && self.request.as_deref().is_none_or(|wanted| {
                trace
                    .request_id
                    .as_deref()
                    .is_some_and(|id| id.starts_with(wanted))
            })
    }
}

fn arguments(call: &Call, headline: String, reason: String, next: String) -> Diagnostic {
    let _ = call;
    Diagnostic::new(&rule::CLI_TELEMETRY_ARGUMENTS, headline)
        .reason(reason)
        .next(next)
}

/// Which of the three commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    Traces,
    Metrics,
    Logs,
}

impl Which {
    fn name(self) -> &'static str {
        match self {
            Which::Traces => "traces",
            Which::Metrics => "metrics",
            Which::Logs => "logs",
        }
    }
}

pub fn traces(call: &Call) -> Reply {
    command(call, Which::Traces)
}

pub fn metrics(call: &Call) -> Reply {
    command(call, Which::Metrics)
}

pub fn logs(call: &Call) -> Reply {
    command(call, Which::Logs)
}

/// What `--follow` was asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowArgs {
    pub which: Which,
    pub root: PathBuf,
    pub filters: Filters,
    pub level: Option<jadpo_std::telemetry::Level>,
    pub json: bool,
    pub choice: ColorChoice,
}

fn command(call: &Call, which: Which) -> Reply {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return crate::fmt::project_not_found(which.name());
    };
    if let Some(extra) = call.arguments.first() {
        return Reply::failed(arguments(
            call,
            format!("`jadpo {}` takes no `{extra}`", which.name()),
            format!(
                "`jadpo {}` lists what `jadpo dev` kept, chosen by its filters: `--route`, `--action`, `--domain`, `--failure`, `--slower-than` and `--request`.",
                which.name()
            ),
            format!(
                "Run `jadpo {}`, with a filter if you want fewer",
                which.name()
            ),
        ));
    }
    let filters = match Filters::read(call) {
        Ok(filters) => filters,
        Err(problem) => return Reply::Failed(problem),
    };
    let level = match call.value("--level") {
        None => None,
        Some(written) => match jadpo_std::telemetry::Level::from_word(written) {
            Some(level) if which == Which::Logs => Some(level),
            Some(_) => {
                return Reply::failed(arguments(
                    call,
                    format!("`jadpo {}` takes no `--level`", which.name()),
                    "`--level` chooses the level `jadpo logs` lists events from.".to_string(),
                    "Use `--level` with `jadpo logs`".to_string(),
                ));
            }
            None => {
                return Reply::failed(arguments(
                    call,
                    format!("`{written}` isn't a level"),
                    "A log's level is one of OpenTelemetry's six: `trace`, `debug`, `info`, `warn`, `error` or `fatal`.".to_string(),
                    "Give a level such as `--level warn`".to_string(),
                ));
            }
        },
    };
    if call.flag("--follow") {
        return Reply::Session(crate::run::Session::Follow(Box::new(FollowArgs {
            which,
            root,
            filters,
            level,
            json: false,
            choice: ColorChoice::Auto,
        })));
    }
    let view = View::read(&root, &filters, level);
    let (screen, data) = match which {
        Which::Traces => view.traces_screen(&filters),
        Which::Metrics => view.metrics_screen(&filters),
        Which::Logs => view.logs_screen(&filters),
    };
    Reply::screen(screen, data)
}

/// What is kept, read and chosen by the filters.
struct View {
    /// Every trace kept, and those chosen.
    all: usize,
    traces: Vec<Trace>,
    /// The metrics payloads of the requests chosen.
    metrics: Vec<Value>,
    any_metrics: bool,
    /// The log records chosen.
    logs: Vec<Log>,
    any_logs: bool,
}

/// One log record, as kept.
#[derive(Clone, Debug, PartialEq)]
struct Log {
    at_ms: u128,
    level: String,
    event: String,
    request_id: Option<String>,
    fields: BTreeMap<String, Value>,
}

impl View {
    fn read(root: &Path, filters: &Filters, level: Option<jadpo_std::telemetry::Level>) -> View {
        let every: Vec<Trace> = payloads(root, Signal::Traces)
            .iter()
            .flat_map(traces_of)
            .collect();
        let all = every.len();
        let traces: Vec<Trace> = every.into_iter().filter(|t| filters.chooses(t)).collect();
        let chosen: BTreeMap<&str, &Trace> =
            traces.iter().map(|t| (t.trace_id.as_str(), t)).collect();
        let metric_payloads = payloads(root, Signal::Metrics);
        let any_metrics = !metric_payloads.is_empty();
        let metrics = metric_payloads
            .into_iter()
            .filter(|payload| !filters.any() || metric_chosen(payload, filters))
            .collect();
        let log_payloads = payloads(root, Signal::Logs);
        let any_logs = !log_payloads.is_empty();
        let from_level = level.map_or(0, |level| u64::from(level.severity_number()));
        let mut logs = Vec::new();
        for payload in &log_payloads {
            for resource in payload["resourceLogs"].as_array().into_iter().flatten() {
                for scope in resource["scopeLogs"].as_array().into_iter().flatten() {
                    for record in scope["logRecords"].as_array().into_iter().flatten() {
                        if record["severityNumber"].as_u64().unwrap_or_default() < from_level {
                            continue;
                        }
                        let mut fields = attributes(&record["attributes"]);
                        fields.remove("event.name");
                        // Its request's trace, by the trace it is tied
                        // to or the request it names.
                        let trace = record["traceId"]
                            .as_str()
                            .and_then(|id| chosen.get(id).copied())
                            .or_else(|| {
                                let named = text(&fields, "request_id")?;
                                traces
                                    .iter()
                                    .find(|trace| trace.request_id.as_deref() == Some(&named))
                            });
                        if filters.any() && trace.is_none() {
                            continue;
                        }
                        let request_id = trace
                            .and_then(|trace| trace.request_id.clone())
                            .or_else(|| text(&fields, "request_id"));
                        logs.push(Log {
                            at_ms: nanos(&record["timeUnixNano"]) / 1_000_000,
                            level: record["severityText"]
                                .as_str()
                                .unwrap_or_default()
                                .to_lowercase(),
                            event: record["body"]["stringValue"]
                                .as_str()
                                .unwrap_or_default()
                                .to_string(),
                            request_id,
                            fields,
                        });
                    }
                }
            }
        }
        View {
            all,
            traces,
            metrics,
            any_metrics,
            logs,
            any_logs,
        }
    }

    fn empty(&self, nothing: bool, kind: &str, filters: &Filters) -> Screen {
        let mut screen = Screen::new();
        if nothing {
            screen.push(Block::summary(
                Outcome::Success,
                format!("No {kind} yet"),
                Vec::new(),
            ));
            screen.push(Block::hint(
                "jadpo dev",
                "runs the app, keeping its traces, metrics and logs",
            ));
        } else {
            screen.push(Block::summary(
                Outcome::Success,
                format!("No {kind} match"),
                filters.facts(),
            ));
        }
        screen
    }

    fn traces_screen(&self, filters: &Filters) -> (Screen, Json) {
        let data = Json::object([("traces", Json::array(self.traces.iter().map(trace_json)))]);
        if self.traces.is_empty() {
            return (self.empty(self.all == 0, "traces", filters), data);
        }
        let mut screen = Screen::new();
        screen.push(Block::Table(trace_table(&self.traces)));
        let failed = self
            .traces
            .iter()
            .filter(|trace| trace.failure.is_some() || trace.fault.is_some())
            .count();
        let mut facts = vec![Fact::plain(format!(
            "{} answered",
            self.traces.len() - failed
        ))];
        if failed > 0 {
            facts.push(Fact {
                text: format!("{failed} failed"),
                tone: Some(Token::Failure),
            });
        }
        screen.push(
            Block::summary(
                Outcome::Success,
                plural(self.traces.len(), "request", "requests"),
                facts,
            )
            .with_plain_headline("traces"),
        );
        if let Some(last) = self.traces.last()
            && let Some(id) = &last.request_id
        {
            screen.push(Block::hint(
                format!("jadpo logs --request {}", short_request(id)),
                "shows what that request logged",
            ));
        }
        (screen, data)
    }

    fn metrics_screen(&self, filters: &Filters) -> (Screen, Json) {
        let series = series_of(&self.metrics);
        let data = Json::object([("metrics", Json::array(series.iter().map(Series::json)))]);
        if series.is_empty() {
            return (self.empty(!self.any_metrics, "metrics", filters), data);
        }
        let mut table = Table::new(["metric", "by", "count", "value"])
            .right(2)
            .right(3)
            .cut(1);
        for one in &series {
            table = table.row(vec![
                Cell::text(one.name.clone()),
                Cell::muted(one.by()),
                Cell::text(one.count.to_string()),
                Cell::text(one.value()),
            ]);
        }
        let mut screen = Screen::new();
        screen.push(Block::Table(table));
        let names: std::collections::BTreeSet<&str> =
            series.iter().map(|one| one.name.as_str()).collect();
        screen.push(
            Block::summary(
                Outcome::Success,
                plural(names.len(), "metric", "metrics"),
                vec![
                    Fact::plain(plural(series.len(), "series", "series")),
                    Fact::plain(plural(self.metrics.len(), "request", "requests")),
                ],
            )
            .with_plain_headline("metrics"),
        );
        screen.push(Block::hint(
            "jadpo metrics --route <route>",
            "adds up one route's requests alone",
        ));
        (screen, data)
    }

    fn logs_screen(&self, filters: &Filters) -> (Screen, Json) {
        let data = Json::object([("logs", Json::array(self.logs.iter().map(log_json)))]);
        if self.logs.is_empty() {
            return (self.empty(!self.any_logs, "logs", filters), data);
        }
        let mut screen = Screen::new();
        screen.push(Block::Table(log_table(&self.logs)));
        let warned = self
            .logs
            .iter()
            .filter(|log| matches!(log.level.as_str(), "warn" | "error" | "fatal"))
            .count();
        let mut facts = Vec::new();
        if warned > 0 {
            facts.push(Fact {
                text: format!("{warned} warn or worse"),
                tone: Some(Token::Warning),
            });
        }
        screen.push(
            Block::summary(
                Outcome::Success,
                plural(self.logs.len(), "event", "events"),
                facts,
            )
            .with_plain_headline("logs"),
        );
        screen.push(Block::hint(
            "jadpo logs --level warn",
            "lists only what should be looked at",
        ));
        (screen, data)
    }
}

/// Whether a metrics payload is of a request the filters choose: its
/// request's duration point carries its route, status and failure, and
/// each action's its name.
fn metric_chosen(payload: &Value, filters: &Filters) -> bool {
    let mut route = None;
    let mut failure = None;
    let mut ms = 0u64;
    let mut names = Vec::new();
    for metric in metric_list(payload) {
        let points = points_of(metric);
        match metric["name"].as_str() {
            Some("http.server.request.duration") => {
                if let Some(point) = points.first() {
                    let attrs = attributes(&point["attributes"]);
                    route = text(&attrs, "http.route");
                    failure = text(&attrs, "error.type");
                    ms = (point["sum"].as_f64().unwrap_or_default() * 1000.0) as u64;
                }
            }
            Some("jadpo.run.duration") => {
                for point in points {
                    if let Some(name) = text(&attributes(&point["attributes"]), "jadpo.name") {
                        names.push(name);
                    }
                }
            }
            _ => {}
        }
    }
    let trace = Trace {
        trace_id: String::new(),
        request_id: None,
        name: String::new(),
        route,
        status: None,
        failure,
        fault: None,
        at_ms: 0,
        ms,
        spans: names
            .into_iter()
            .map(|name| Span {
                name: name.rsplit('.').next().unwrap_or(&name).to_string(),
                kind: "action".into(),
                qualified: Some(name),
                ms: 0,
                failure: None,
                fault: None,
            })
            .collect(),
        notes: BTreeMap::new(),
    };
    // A request's id isn't on its metrics: `--request` reads the traces.
    let route_ok = filters.route.as_deref().is_none_or(|wanted| {
        trace.route.as_deref() == Some(wanted)
            || wanted
                .split_once(' ')
                .is_some_and(|(_, path)| trace.route.as_deref() == Some(path))
    });
    let rest = Filters {
        route: None,
        request: None,
        ..filters.clone()
    };
    route_ok && filters.request.is_none() && rest.chooses(&trace)
}

fn metric_list(payload: &Value) -> Vec<&Value> {
    payload["resourceMetrics"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|resource| resource["scopeMetrics"].as_array().into_iter().flatten())
        .flat_map(|scope| scope["metrics"].as_array().into_iter().flatten())
        .collect()
}

fn points_of(metric: &Value) -> Vec<&Value> {
    ["histogram", "sum", "gauge"]
        .iter()
        .filter_map(|shape| metric.get(*shape))
        .flat_map(|data| data["dataPoints"].as_array().into_iter().flatten())
        .collect()
}

/// The runtime's own attributes, which a series is shown by the values
/// of, in this order.
const OWN_ATTRIBUTES: [&str; 10] = [
    "http.request.method",
    "http.route",
    "http.response.status_code",
    "jadpo.kind",
    "jadpo.name",
    "jadpo.connection",
    "jadpo.operation",
    "error.type",
    "jadpo.failure",
    "jadpo.outcome",
];

/// One metric's series, added up over the requests listed.
struct Series {
    name: String,
    unit: Option<String>,
    shape: &'static str,
    labels: BTreeMap<String, Value>,
    count: u64,
    total: f64,
    last: f64,
}

impl Series {
    /// What the series is of: the runtime's own attributes by their
    /// values alone, in the order a request reads (`POST /todos 201`,
    /// `action planning.create_todo value`), then the app's labels as
    /// `name=value`.
    fn by(&self) -> String {
        if self.labels.is_empty() {
            return "–".to_string();
        }
        let shown = |value: &Value| match value {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        };
        let mut out: Vec<String> = OWN_ATTRIBUTES
            .iter()
            .filter_map(|key| self.labels.get(*key).map(shown))
            .collect();
        if let (Some(failure), Some(error)) = (
            self.labels.get("jadpo.failure"),
            self.labels.get("error.type"),
        ) && failure == error
        {
            out.dedup();
        }
        out.extend(
            self.labels
                .iter()
                .filter(|(key, _)| !OWN_ATTRIBUTES.contains(&key.as_str()))
                .map(|(key, value)| format!("{key}={}", shown(value))),
        );
        out.join(" ")
    }

    /// What it comes to: a histogram's mean, a sum's total, a gauge's
    /// last value; durations in milliseconds.
    fn value(&self) -> String {
        let shown = |value: f64| -> String {
            match self.unit.as_deref() {
                Some("s") => format!("{} ms", (value * 1000.0).round() as i64),
                Some("ms") => format!("{} ms", value.round() as i64),
                _ if value.fract() == 0.0 => format!("{}", value as i64),
                _ => format!("{value:.2}"),
            }
        };
        match self.shape {
            "histogram" if self.count > 0 => {
                format!("avg {}", shown(self.total / self.count as f64))
            }
            "sum" => shown(self.total),
            _ => shown(self.last),
        }
    }

    fn json(&self) -> Json {
        Json::object([
            ("name", Json::from(self.name.as_str())),
            ("kind", Json::from(self.shape)),
            ("unit", self.unit.as_deref().map_or(Json::Null, Json::from)),
            (
                "by",
                Json::object(
                    self.labels
                        .iter()
                        .map(|(key, value)| (key.as_str(), to_json(value))),
                ),
            ),
            ("count", Json::Int(self.count as i64)),
            ("total", number_json(self.total)),
            ("last", number_json(self.last)),
            ("value", Json::from(self.value())),
        ])
    }
}

fn series_of(payloads: &[Value]) -> Vec<Series> {
    let mut out: BTreeMap<(String, String), Series> = BTreeMap::new();
    for payload in payloads {
        for metric in metric_list(payload) {
            let Some(name) = metric["name"].as_str() else {
                continue;
            };
            let shape = if metric.get("histogram").is_some() {
                "histogram"
            } else if metric.get("sum").is_some() {
                "sum"
            } else {
                "gauge"
            };
            for point in points_of(metric) {
                let labels = attributes(&point["attributes"]);
                let key = (
                    name.to_string(),
                    serde_json::to_string(&labels).unwrap_or_default(),
                );
                let value = point["asDouble"].as_f64().or_else(|| {
                    point["asInt"]
                        .as_str()
                        .and_then(|text| text.parse::<f64>().ok())
                });
                let entry = out.entry(key).or_insert_with(|| Series {
                    name: name.to_string(),
                    unit: metric["unit"].as_str().map(str::to_string),
                    shape,
                    labels,
                    count: 0,
                    total: 0.0,
                    last: 0.0,
                });
                match shape {
                    "histogram" => {
                        entry.count += point["count"]
                            .as_str()
                            .and_then(|text| text.parse::<u64>().ok())
                            .unwrap_or_default();
                        entry.total += point["sum"].as_f64().unwrap_or_default();
                    }
                    _ => {
                        let value = value.unwrap_or_default();
                        entry.count += 1;
                        entry.total += value;
                        entry.last = value;
                    }
                }
            }
        }
    }
    out.into_values().collect()
}

fn to_json(value: &Value) -> Json {
    match value {
        Value::Null => Json::Null,
        Value::Bool(bool) => Json::from(*bool),
        Value::Number(number) => match number.as_i64() {
            Some(int) => Json::Int(int),
            None => Json::Str(number.to_string()),
        },
        Value::String(text) => Json::from(text.as_str()),
        Value::Array(items) => Json::array(items.iter().map(to_json)),
        Value::Object(map) => Json::object(
            map.iter()
                .map(|(key, value)| (key.as_str(), to_json(value))),
        ),
    }
}

/// A number as data: whole, as a number; otherwise as its text, which
/// keeps every digit.
fn number_json(value: f64) -> Json {
    if value.fract() == 0.0 && value.abs() < 9.0e15 {
        Json::Int(value as i64)
    } else {
        Json::Str(value.to_string())
    }
}

fn plural(count: usize, one: &str, many: &str) -> String {
    if count == 1 {
        format!("1 {one}")
    } else {
        format!("{count} {many}")
    }
}

/// A request id as `jadpo dev`'s request lines show it: `req_739b`.
fn short_request(id: &str) -> String {
    id.chars().take(8).collect()
}

/// When, as the time of day it was on this machine's clock in UTC:
/// `06:34:34.202`.
fn clock(ms: u128) -> String {
    let day = (ms % 86_400_000) as u64;
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        day / 3_600_000,
        day / 60_000 % 60,
        day / 1000 % 60,
        day % 1000
    )
}

/// How a request ended: its status, or its failure or fault.
fn outcome_cell(trace: &Trace) -> Cell {
    match (&trace.fault, &trace.failure, trace.status) {
        (Some(fault), _, _) => Cell::state(Symbol::Failure, fault.clone()),
        (None, Some(code), _) => Cell::state(Symbol::Failure, code.clone()),
        (None, None, Some(status)) => Cell::state(Symbol::Success, status.to_string()),
        (None, None, None) => Cell::state(Symbol::Skipped, "–"),
    }
}

fn trace_table(traces: &[Trace]) -> Table {
    let mut table = Table::new(["request", "route", "answer", "time", "spans", "trace"])
        .right(3)
        .right(4)
        .cut(1);
    for trace in traces {
        table = table.row(vec![
            Cell::text(
                trace
                    .request_id
                    .as_deref()
                    .map_or_else(|| "–".to_string(), short_request),
            ),
            Cell::text(trace.name.clone()),
            outcome_cell(trace),
            Cell::muted(format!("{} ms", trace.ms)),
            Cell::muted(trace.spans.len().to_string()),
            Cell::muted(trace.trace_id.chars().take(8).collect::<String>()),
        ]);
    }
    table
}

fn level_cell(level: &str) -> Cell {
    match level {
        "error" | "fatal" => Cell::state(Symbol::Failure, level),
        "warn" => Cell::state(Symbol::Warning, level),
        _ => Cell::muted(level.to_string()),
    }
}

fn log_table(logs: &[Log]) -> Table {
    let mut table = Table::new(["time", "level", "event", "request", "fields"]).cut(4);
    for log in logs {
        table = table.row(vec![
            Cell::muted(clock(log.at_ms)),
            level_cell(&log.level),
            Cell::text(log.event.clone()),
            Cell::muted(
                log.request_id
                    .as_deref()
                    .map_or_else(|| "–".to_string(), short_request),
            ),
            Cell::muted(
                log.fields
                    .iter()
                    .filter(|(key, _)| key.as_str() != "request_id")
                    .map(|(key, value)| match value {
                        Value::String(text) => format!("{key}={text}"),
                        other => format!("{key}={other}"),
                    })
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
        ]);
    }
    table
}

fn trace_json(trace: &Trace) -> Json {
    let text = |value: &Option<String>| value.as_deref().map_or(Json::Null, Json::from);
    Json::object([
        ("traceId", Json::from(trace.trace_id.as_str())),
        ("requestId", text(&trace.request_id)),
        ("name", Json::from(trace.name.as_str())),
        ("route", text(&trace.route)),
        ("status", trace.status.map_or(Json::Null, Json::Int)),
        ("failure", text(&trace.failure)),
        ("fault", text(&trace.fault)),
        ("ms", Json::Int(i64::try_from(trace.ms).unwrap_or(i64::MAX))),
        (
            "notes",
            Json::object(
                trace
                    .notes
                    .iter()
                    .map(|(key, value)| (key.as_str(), to_json(value))),
            ),
        ),
        (
            "spans",
            Json::array(trace.spans.iter().map(|span| {
                Json::object([
                    ("name", Json::from(span.name.as_str())),
                    ("kind", Json::from(span.kind.as_str())),
                    (
                        "qualified",
                        span.qualified.as_deref().map_or(Json::Null, Json::from),
                    ),
                    ("ms", Json::Int(i64::try_from(span.ms).unwrap_or(i64::MAX))),
                    (
                        "failure",
                        span.failure.as_deref().map_or(Json::Null, Json::from),
                    ),
                    (
                        "fault",
                        span.fault.as_deref().map_or(Json::Null, Json::from),
                    ),
                ])
            })),
        ),
    ])
}

fn log_json(log: &Log) -> Json {
    Json::object([
        ("level", Json::from(log.level.as_str())),
        ("event", Json::from(log.event.as_str())),
        (
            "requestId",
            log.request_id.as_deref().map_or(Json::Null, Json::from),
        ),
        (
            "fields",
            Json::object(
                log.fields
                    .iter()
                    .map(|(key, value)| (key.as_str(), to_json(value))),
            ),
        ),
    ])
}

// --- following ------------------------------------------------------------------

/// `--follow`: what is kept now, then each new entry as it arrives, until
/// ctrl-c. With `--json`, each is one envelope on one line.
pub fn follow(args: &FollowArgs, env: &Environment) -> u8 {
    let style = Style::detect(env, args.json, args.choice, Stream::Stdout);
    let mut out = std::io::stdout();
    let mut shown = 0usize;
    let stop = jadpo_host_native::stop_signals()
        .unwrap_or_else(|_| Arc::new(std::sync::atomic::AtomicBool::new(false)));
    while !stop.load(std::sync::atomic::Ordering::SeqCst) {
        let view = View::read(&args.root, &args.filters, args.level);
        let entries: Vec<Json> = match args.which {
            Which::Traces => view.traces.iter().map(trace_json).collect(),
            Which::Metrics => series_of(&view.metrics).iter().map(Series::json).collect(),
            Which::Logs => view.logs.iter().map(log_json).collect(),
        };
        // A file trimmed under the limit starts the count again.
        if entries.len() < shown {
            shown = 0;
        }
        if entries.len() > shown {
            let text = if args.json {
                entries[shown..]
                    .iter()
                    .map(|entry| {
                        jadpo_term::short_envelope(
                            Some(args.which.name()),
                            Some(entry.clone()),
                            &[],
                            &(),
                        )
                        .to_line()
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
                    + "\n"
            } else {
                let table = match args.which {
                    Which::Traces => trace_table(&view.traces[shown..]),
                    Which::Logs => log_table(&view.logs[shown..]),
                    Which::Metrics => {
                        // Series are added up again each time: shown whole.
                        let (screen, _) = view.metrics_screen(&args.filters);
                        let _ = write!(out, "{}", screen.render(&style));
                        let _ = out.flush();
                        shown = entries.len();
                        std::thread::sleep(std::time::Duration::from_millis(500));
                        continue;
                    }
                };
                let rendered = Screen::new().with(Block::Table(table)).render(&style);
                // The header once, at the start.
                if shown == 0 {
                    rendered
                } else {
                    rendered.lines().skip(1).collect::<Vec<_>>().join("\n") + "\n"
                }
            };
            let _ = write!(out, "{text}");
            let _ = out.flush();
            shown = entries.len();
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trace_is_read_from_its_payload() {
        let payload = serde_json::json!({ "resourceSpans": [{ "scopeSpans": [{ "spans": [
            { "traceId": "4bf92f3577b34da6a3ce929d0e0e4736", "spanId": "00f067aa0ba902b7",
              "name": "POST /todos", "kind": 2,
              "startTimeUnixNano": "1000000000", "endTimeUnixNano": "1004000000",
              "attributes": [
                { "key": "http.route", "value": { "stringValue": "/todos" } },
                { "key": "http.response.status_code", "value": { "intValue": "201" } },
                { "key": "jadpo.request_id", "value": { "stringValue": "req_58d4e1" } },
                { "key": "plan", "value": { "stringValue": "paid" } }
              ], "status": {} },
            { "traceId": "4bf92f3577b34da6a3ce929d0e0e4736", "spanId": "1111111111111111",
              "parentSpanId": "00f067aa0ba902b7", "name": "create_todo", "kind": 1,
              "startTimeUnixNano": "1001000000", "endTimeUnixNano": "1003000000",
              "attributes": [
                { "key": "jadpo.kind", "value": { "stringValue": "action" } },
                { "key": "jadpo.name", "value": { "stringValue": "planning.create_todo" } }
              ], "status": {} }
        ] }] }] });
        let traces = traces_of(&payload);
        assert_eq!(traces.len(), 1);
        let trace = &traces[0];
        assert_eq!(trace.name, "POST /todos");
        assert_eq!(trace.status, Some(201));
        assert_eq!(trace.ms, 4);
        assert_eq!(trace.notes.get("plan"), Some(&Value::from("paid")));
        let filters = Filters {
            action: Some("create_todo".into()),
            domain: Some("planning".into()),
            slower_than: Some(("3ms".into(), 3)),
            ..Filters::default()
        };
        assert!(filters.chooses(trace));
        let slower = Filters {
            slower_than: Some(("4ms".into(), 4)),
            ..Filters::default()
        };
        assert!(!slower.chooses(trace));
    }

    #[test]
    fn the_clock_reads_the_time_of_day() {
        assert_eq!(clock(1_792_486_800_202), "09:00:00.202");
    }
}
