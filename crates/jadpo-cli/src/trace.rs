//! The request timeline and `jadpo trace` (area 17, "The request timeline
//! in `jadpo dev`"; chunk E0).
//!
//! - `jadpo dev` keeps each request's timeline, built from its spans and
//!   what its host kept beside them ([`jadpo_runtime::timeline`]), one
//!   document a line in `build/dev/timelines.jsonl`, the last [`KEPT`];
//!   and each attempt at a delivery its requests set off, in
//!   `build/dev/deliveries.jsonl`, which a timeline lists by its trace.
//! - A request that ran several queries, or a query no index serves, is
//!   flagged on its line ([`Flags`]).
//! - `jadpo test --keep-failed` keeps a failed test's timeline in
//!   `build/test/traces/<id>.json`, which its report names
//!   ([`jadpo_test::kept_trace`]).
//! - `jadpo trace <id>` prints one: a request's by any start of its id or
//!   of its trace's, or a kept test's: the route, the caller, each action,
//!   query and write with its time, its rows and its SQL, each provider
//!   call with what was sent and what came back from the call log, each
//!   step's state, the failure and where it was raised, and the outcome:
//!   committed or rolled back, the events emitted and the subscribers they
//!   ran.

use std::collections::BTreeSet;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use jadpo_diag::{Diagnostic, rule};
use jadpo_term::{Bit, Block, Json, Screen, Symbol, Token};
use serde_json::{Value, json};

use crate::run::{Call, Reply};

/// Where `jadpo dev` keeps its requests' timelines, relative to the project.
pub(crate) const REQUESTS: &str = "build/dev/timelines.jsonl";
/// Where it keeps the attempts at the deliveries they set off.
pub(crate) const DELIVERIES: &str = "build/dev/deliveries.jsonl";
/// Where `jadpo test --keep-failed` keeps failed tests' timelines.
pub(crate) const TESTS: &str = jadpo_test::kept_trace::FOLDER;
/// How many it keeps of each.
pub(crate) const KEPT: usize = 500;

// --- keeping ------------------------------------------------------------------

/// `jadpo dev`'s keeping of its timelines, which every worker shares.
#[derive(Clone)]
pub(crate) struct DevTimelines {
    root: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl DevTimelines {
    pub(crate) fn new(root: &Path) -> DevTimelines {
        DevTimelines {
            root: root.to_path_buf(),
            lock: Arc::new(Mutex::new(())),
        }
    }

    pub(crate) fn request(&self, document: &Value) {
        self.keep(REQUESTS, document);
    }

    pub(crate) fn delivery(&self, document: &Value) {
        self.keep(DELIVERIES, document);
    }

    /// Appends `document` to `file`, dropping the oldest once it holds a
    /// fifth more than [`KEPT`].
    fn keep(&self, file: &str, document: &Value) {
        let Ok(_held) = self.lock.lock() else {
            return;
        };
        let path = self.root.join(file);
        if let Some(folder) = path.parent() {
            let _ = std::fs::create_dir_all(folder);
        }
        let appended = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .and_then(|mut out| out.write_all(format!("{document}\n").as_bytes()));
        if appended.is_err() {
            return;
        }
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

/// What a request's line is flagged with (area 17, "What gets flagged"):
/// the queries it sent when it sent several, and the queries it ran that
/// no index serves.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Flags {
    pub queries: usize,
    pub unindexed: Vec<String>,
}

impl Flags {
    /// From a timeline document: each statement a read ran is a query,
    /// an include's as well as its own.
    pub(crate) fn of(document: &Value, unindexed: &BTreeSet<String>) -> Flags {
        let mut flags = Flags::default();
        for span in spans(document) {
            if span["kind"] != "read" {
                continue;
            }
            flags.queries += span["statements"].as_array().map_or(0, Vec::len);
            if let Some(name) = span["name"].as_str()
                && unindexed.contains(name)
                && !flags.unindexed.iter().any(|seen| seen == name)
            {
                flags.unindexed.push(name.to_string());
            }
        }
        flags
    }

    /// Whether it ran several queries.
    pub(crate) fn several(&self) -> bool {
        self.queries > 1
    }

    pub(crate) fn any(&self) -> bool {
        self.several() || !self.unindexed.is_empty()
    }

    /// As the line says it: `3 queries · no index`.
    pub(crate) fn words(&self) -> String {
        let mut words = Vec::new();
        if self.several() {
            words.push(format!("{} queries", self.queries));
        }
        if !self.unindexed.is_empty() {
            words.push("no index".to_string());
        }
        words.join(" · ")
    }
}

/// A request's timeline document: what [`jadpo_runtime::timeline::document`]
/// makes of its spans, with the request itself, its route's place, its
/// calls, steps and fault, and its flags. `None` when its host kept none.
pub(crate) fn request_document(
    answered: &jadpo_runtime::serve::Answered<'_>,
    timeline: &jadpo_runtime::timeline::Timeline,
    known: &Known,
    at_ms: i64,
    steps: Json,
) -> Option<(Value, Flags)> {
    let response = answered.response;
    let places = answered.program.places();
    let mut document = jadpo_runtime::timeline::document(answered.spans, timeline, places);
    let route = response.route.as_deref();
    let route_place = route.and_then(|route| known.routes.get(route).cloned());
    let trace = response
        .trace
        .map(|trace| jadpo_runtime::telemetry::otlp::hex(&trace.trace_id));
    let calls: Vec<Value> = response
        .calls
        .iter()
        .map(|call| {
            json!({
                "connection": call.connection,
                "operation": call.operation,
                "source": call.source.word(),
                "ended": crate::dev::ended_word(&call.ended),
                "step": call.step,
            })
        })
        .collect();
    let fault = response.fault.as_ref().map(|fault| {
        json!({
            "classification": fault.kind.classification(),
            "frames": fault.stack(places),
        })
    });
    name_reads(
        &mut document,
        &|file| known.texts.get(file).cloned(),
        route
            .and_then(|route| known.runs.get(route))
            .map(String::as_str),
    );
    let flags = Flags::of(&document, &known.unindexed);
    if let Some(object) = document.as_object_mut() {
        let id = response.header("x-request-id").unwrap_or_default();
        for (key, value) in [
            ("requestId", json!(id)),
            ("traceId", json!(trace)),
            ("at", json!(jadpo_std::format_instant(at_ms))),
            ("method", json!(answered.request.method)),
            ("path", json!(answered.request.path)),
            ("route", json!(route)),
            ("routePlace", json!(route_place)),
            ("status", json!(response.status)),
            ("code", json!(response.code)),
            ("ms", json!(answered.ms)),
            ("address", json!(response.client)),
            ("calls", Value::Array(calls)),
            ("steps", from_term(&steps)),
            ("fault", json!(fault)),
            (
                "flags",
                json!({ "queries": flags.queries, "unindexed": flags.unindexed }),
            ),
        ] {
            object.insert(key.to_string(), value);
        }
    }
    Some((document, flags))
}

/// An attempt at a delivery, as `jadpo dev` keeps it for its request's
/// timeline.
pub(crate) fn delivery_document(attempt: &jadpo_runtime::call_log::DeliveryAttempt<'_>) -> Value {
    json!({
        "delivery": jadpo_runtime::uuid_text(&attempt.delivery),
        "traceId": attempt.trace.map(|trace| jadpo_runtime::telemetry::otlp::hex(&trace)),
        "event": attempt.event,
        "subscriber": attempt.subscriber,
        "state": attempt.state,
        "ms": attempt.ms,
    })
}

fn from_term(json: &Json) -> Value {
    match json {
        Json::Null => Value::Null,
        Json::Bool(bool) => Value::Bool(*bool),
        Json::Int(int) => Value::from(*int),
        Json::Str(text) => Value::from(text.as_str()),
        Json::Array(items) => Value::Array(items.iter().map(from_term).collect()),
        Json::Object(entries) => Value::Object(
            entries
                .iter()
                .map(|(key, value)| (key.clone(), from_term(value)))
                .collect(),
        ),
    }
}

/// What a request's timeline needs of the version being served: where
/// each route is declared, and the queries no index serves.
#[derive(Clone, Debug, Default)]
pub(crate) struct Known {
    /// `file:line` by route, `POST /todos`.
    pub routes: std::collections::BTreeMap<String, String>,
    pub unindexed: BTreeSet<String>,
    /// Each route's `run:` as written, by route.
    pub runs: std::collections::BTreeMap<String, String>,
    /// The project's sources, which name each read at its site.
    pub texts: std::collections::BTreeMap<String, String>,
}

/// Names each read span of `document` as the code calls it, from the
/// source at its site (`find_todo`, `Todo.get`), or for a read the runtime
/// makes on a route's behalf, from the route's `run:`. A read runs inside
/// the action that calls it, so its span's callable doesn't name it.
pub(crate) fn name_reads(
    document: &mut Value,
    texts: &dyn Fn(&str) -> Option<String>,
    run: Option<&str>,
) {
    let Some(spans) = document.get_mut("spans").and_then(Value::as_array_mut) else {
        return;
    };
    for span in spans {
        if span["kind"] != "read" {
            continue;
        }
        let named = match span["site"].as_object() {
            Some(site) => site["file"].as_str().and_then(|file| {
                let text = texts(file)?;
                let start = usize::try_from(site["start"].as_u64()?).ok()?;
                let end = usize::try_from(site["end"].as_u64()?).ok()?;
                called(text.get(start..end)?)
            }),
            None => run.and_then(called),
        };
        if let Some(name) = named {
            span["name"] = Value::from(name);
        }
    }
}

/// What a call written as `text` calls: `find_todo` from `attempt
/// find_todo(todo_id)`.
fn called(text: &str) -> Option<String> {
    let text = text.trim();
    let text = text.strip_prefix("attempt ").unwrap_or(text).trim_start();
    let name: String = text
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
        .collect();
    (!name.is_empty()).then_some(name)
}

impl Known {
    pub(crate) fn of(db: &dyn jadpo_db::Db, project: jadpo_db::Project) -> Known {
        // An include's statement counts for its query (area 14, "The
        // `build/` folder").
        let unindexed = jadpo_check::data::report::reads(db, project)
            .into_iter()
            .filter(|read| read.index.is_none())
            .map(|read| read.query)
            .collect();
        let texts = jadpo_db::source_texts(db, project);
        let mut routes = std::collections::BTreeMap::new();
        let mut runs = std::collections::BTreeMap::new();
        for name in jadpo_check::routes::group_names(db, project) {
            let Some(read) = jadpo_check::routes::group_read(
                db,
                project,
                jadpo_check::lookups::Name::new(db, name.clone()),
            ) else {
                continue;
            };
            for (label, span) in &read.spans {
                let line = texts.get(&span.path).map_or(1, |text| {
                    let end = (span.start as usize).min(text.len());
                    text.as_bytes()[..end]
                        .iter()
                        .filter(|b| **b == b'\n')
                        .count()
                        + 1
                });
                routes.insert(label.clone(), format!("{}:{line}", span.path));
            }
            for (label, span) in &read.run_spans {
                if let Some(run) = texts
                    .get(&span.path)
                    .and_then(|text| text.get(span.start as usize..span.end as usize))
                {
                    runs.insert(label.clone(), run.to_string());
                }
            }
        }
        Known {
            routes,
            unindexed,
            runs,
            texts,
        }
    }
}

// --- reading ------------------------------------------------------------------

fn lines(root: &Path, file: &str) -> Vec<Value> {
    let Ok(text) = std::fs::read_to_string(root.join(file)) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn spans(document: &Value) -> &[Value] {
    document["spans"].as_array().map_or(&[], Vec::as_slice)
}

fn arguments(headline: String, reason: String, next: String) -> Diagnostic {
    Diagnostic::new(&rule::CLI_TRACE_ARGUMENTS, headline)
        .reason(reason)
        .next(next)
}

/// `jadpo trace <id>`.
pub fn trace(call: &Call) -> Reply {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return crate::fmt::project_not_found("trace");
    };
    let [id] = call.arguments.as_slice() else {
        return Reply::failed(arguments(
            "`jadpo trace` takes one request's id".to_string(),
            "It shows one request `jadpo dev` answered, by its request id (`req_0b1e`), its trace's id, or a failed test's kept trace (`t-7f3a9c1e`).".to_string(),
            "Run `jadpo trace <id>` with an id `jadpo dev` printed".to_string(),
        ));
    };
    let full = call.flag("--full");
    let mut document = match find(&root, id) {
        Ok(document) => document,
        Err(diagnostic) => return Reply::failed(*diagnostic),
    };
    // A test's trace names its reads from the sources on disk.
    if document.get("test").is_some() {
        name_reads(
            &mut document,
            &|file| std::fs::read_to_string(root.join(file)).ok(),
            None,
        );
    }
    let request = document["requestId"].as_str().unwrap_or_default();
    let calls: Vec<crate::dev_calls::Entry> = if request.is_empty() {
        Vec::new()
    } else {
        crate::dev_calls::read(&root)
            .into_iter()
            .filter(|entry| entry.request_id.as_deref() == Some(request))
            .collect()
    };
    let trace = document["traceId"].as_str().unwrap_or_default();
    let deliveries: Vec<Value> = if trace.is_empty() || document.get("test").is_some() {
        Vec::new()
    } else {
        lines(&root, DELIVERIES)
            .into_iter()
            .filter(|delivery| delivery["traceId"].as_str() == Some(trace))
            .collect()
    };
    let shown = Shown {
        document: &document,
        calls: &calls,
        deliveries: &deliveries,
        full,
    };
    let (lines, cut) = shown.lines();
    let mut screen = Screen::new().with(Block::Lines(lines));
    if cut {
        screen.push(Block::hint(
            format!("jadpo trace {id} --full"),
            "shows each statement and value whole",
        ));
    }
    let mut data = document.clone();
    if let Some(object) = data.as_object_mut() {
        for (call, entry) in object
            .get_mut("calls")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
            .zip(&calls)
        {
            if let Some(call) = call.as_object_mut() {
                call.insert("id".to_string(), json!(entry.id));
                call.insert("body".to_string(), json!(entry.body));
                call.insert("answer".to_string(), json!(entry.answer));
                call.insert("ms".to_string(), json!(entry.ms));
            }
        }
        object.insert("deliveries".to_string(), Value::Array(deliveries));
    }
    Reply::screen(screen, crate::telemetry::to_json(&data))
}

/// The timeline `id` names: a kept test's, or a request's by any start of
/// its request id or its trace's id, the newest when a start fits more
/// than one request's trace.
fn find(root: &Path, id: &str) -> Result<Value, Box<Diagnostic>> {
    if id.starts_with("t-") {
        let path = root.join(TESTS).join(format!("{id}.json"));
        return std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .ok_or_else(|| {
                Box::new(arguments(
                    format!("No failed test's trace is kept as `{id}`"),
                    "`jadpo test --keep-failed` keeps each failed test's trace, named in its report; a trace is kept until the newest 500 replace it.".to_string(),
                    "Run `jadpo test --keep-failed` and use the trace its report names".to_string(),
                ))
            });
    }
    let all = lines(root, REQUESTS);
    let matching: Vec<&Value> = all
        .iter()
        .filter(|document| {
            document["requestId"]
                .as_str()
                .is_some_and(|request| request.starts_with(id))
                || document["traceId"]
                    .as_str()
                    .is_some_and(|trace| id.len() >= 4 && trace.starts_with(id))
        })
        .collect();
    let requests: BTreeSet<&str> = matching
        .iter()
        .filter_map(|document| document["requestId"].as_str())
        .collect();
    match matching.as_slice() {
        [] => Err(Box::new(arguments(
            format!("No request `{id}` is kept"),
            format!(
                "`jadpo dev` keeps the last {KEPT} requests' timelines{}.",
                match all.last().and_then(|last| last["requestId"].as_str()) {
                    Some(last) => format!(", the newest `{last}`"),
                    None => ", and holds none yet".to_string(),
                }
            ),
            "Run `jadpo traces` to see the requests kept".to_string(),
        ))),
        [.., last] if requests.len() == 1 || !id.starts_with("req_") => Ok((*last).clone()),
        _ => Err(Box::new(arguments(
            format!("`{id}` starts {} requests' ids", requests.len()),
            format!(
                "It could be {}.",
                crate::run::either(&crate::run::quoted(requests.iter()))
            ),
            "Give more of the id".to_string(),
        ))),
    }
}

// --- the screen ---------------------------------------------------------------

/// The column a line's value starts at (CLI design language, "Layout").
const VALUE: usize = 12;

struct Shown<'a> {
    document: &'a Value,
    calls: &'a [crate::dev_calls::Entry],
    deliveries: &'a [Value],
    full: bool,
}

type Line = Vec<Bit>;

fn text(value: impl Into<String>) -> Bit {
    Bit::Text(value.into())
}

fn muted(value: impl Into<String>) -> Bit {
    Bit::Painted(Token::Muted, value.into())
}

/// A label at `indent`, padded to the value column.
fn label(indent: usize, word: &str, token: Option<Token>) -> Vec<Bit> {
    let pad = VALUE.saturating_sub(indent + word.chars().count()).max(2);
    let word = match token {
        Some(token) => Bit::Painted(token, word.to_string()),
        None => muted(word),
    };
    vec![text(" ".repeat(indent)), word, text(" ".repeat(pad))]
}

/// An id as a line shows it: `0192…a7` for a UUID.
fn short(id: &str) -> String {
    if id.len() == 36 && id.chars().filter(|c| *c == '-').count() == 4 {
        format!("{}…{}", &id[..4], &id[34..])
    } else {
        id.to_string()
    }
}

/// A statement on one line, its runs of white space each one space.
fn one_line(sql: &str) -> String {
    sql.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A statement as a line shows it: on one line, its selected columns as
/// `…`.
fn statement(sql: &str) -> String {
    let sql = one_line(sql);
    if let Some(rest) = sql.strip_prefix("SELECT ")
        && let Some(at) = rest.find(" FROM ")
    {
        return format!("SELECT …{}", &rest[at..]);
    }
    sql
}

/// An event's name as a line shows it: `TodoAdded` for
/// `planning.TodoAdded`.
fn event_name(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

fn plural(count: u64, one: &str, many: &str) -> String {
    if count == 1 {
        format!("1 {one}")
    } else {
        format!("{count} {many}")
    }
}

impl Shown<'_> {
    /// The value at the end of a line: cut to fit, unless `--full`.
    fn fit(&self, token: Option<Token>, value: String, cut: &mut bool, room: usize) -> Bit {
        if self.full {
            return match token {
                Some(token) => Bit::Painted(token, value),
                None => text(value),
            };
        }
        if value.chars().count() > room {
            *cut = true;
        }
        Bit::Fit(token, value)
    }

    fn lines(&self) -> (Vec<Line>, bool) {
        let document = self.document;
        let mut out = Vec::new();
        let mut cut = false;
        out.push(self.headline());
        if let Some(test) = document.get("test") {
            let mut line = label(2, "test", None);
            line.push(text(test["location"].as_str().unwrap_or_default()));
            out.push(line);
        }
        if let Some(place) = document["routePlace"].as_str() {
            let mut line = label(2, "route", None);
            line.push(text(place));
            out.push(line);
        }
        let spans = spans(document);
        let mut calls = self.calls.iter();
        let steps = document["steps"].as_array().cloned().unwrap_or_default();
        let mut stepped = BTreeSet::new();
        for (at, span) in spans.iter().enumerate() {
            let depth = depth(spans, at);
            let indent = 2 + 2 * depth;
            match span["kind"].as_str().unwrap_or_default() {
                "action" => {
                    let name = span["name"].as_str().unwrap_or_default();
                    let mut line = label(indent, "action", None);
                    line.push(text(format!("{name:<22}")));
                    line.extend(timing(span));
                    if let Some(place) = span["place"].as_str() {
                        line.push(muted(format!("  {place}")));
                    }
                    out.push(line);
                    // The steps it reached under a record, once.
                    if stepped.insert(name.to_string()) {
                        for step in steps.iter().filter(|step| step["action"] == name) {
                            out.push(step_line(indent + 2, step));
                        }
                    }
                }
                "read" | "write" => {
                    let word = if span["kind"] == "read" {
                        "query"
                    } else {
                        "write"
                    };
                    let mut line = label(indent, word, None);
                    line.push(text(format!(
                        "{:<22}",
                        span["name"].as_str().unwrap_or_default()
                    )));
                    line.extend(timing(span));
                    if let Some(rows) = span["rows"].as_u64() {
                        line.push(muted(format!("  {}", plural(rows, "row", "rows"))));
                    }
                    if let Some(code) = span["code"].as_str() {
                        line.push(Bit::Painted(Token::Failure, format!("  {code}")));
                    }
                    let unindexed = document["flags"]["unindexed"]
                        .as_array()
                        .is_some_and(|names| names.iter().any(|n| *n == span["name"]));
                    if unindexed && span["kind"] == "read" {
                        line.push(Bit::Painted(Token::Warning, "  no index".to_string()));
                    }
                    out.push(line);
                    let at = VALUE.max(indent + 7);
                    for one in span["statements"].as_array().into_iter().flatten() {
                        let sql = one["sql"].as_str().unwrap_or_default();
                        let sql = if self.full {
                            one_line(sql)
                        } else {
                            statement(sql)
                        };
                        let room = 80usize.saturating_sub(at);
                        out.push(vec![
                            text(" ".repeat(at)),
                            self.fit(Some(Token::Muted), sql, &mut cut, room),
                        ]);
                        let params: Vec<String> = one["params"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .enumerate()
                            .map(|(n, param)| {
                                let param = param.as_str().unwrap_or_default();
                                let param = if self.full {
                                    param.to_string()
                                } else {
                                    shown_param(param)
                                };
                                format!("${} {param}", n + 1)
                            })
                            .collect();
                        if !params.is_empty() {
                            out.push(vec![
                                text(" ".repeat(at)),
                                self.fit(Some(Token::Muted), params.join("  "), &mut cut, room),
                            ]);
                        }
                    }
                }
                "call" => {
                    let name = span["name"].as_str().unwrap_or_default();
                    let entry = calls.next();
                    let mut line = label(indent, "call", None);
                    line.push(text(format!("{name:<22}")));
                    if let Some(entry) = entry {
                        line.push(muted(format!("  {}", entry.source)));
                    }
                    line.extend(timing(span));
                    let (symbol, token, word) = match entry {
                        Some(entry) => match entry.ended.as_str() {
                            "answered" => (Symbol::Success, Token::Success, "answered".to_string()),
                            "outcome_unknown" | "in_flight" => (
                                Symbol::Decision,
                                Token::Decision,
                                entry.ended.replace('_', " "),
                            ),
                            _ => (
                                Symbol::Failure,
                                Token::Failure,
                                entry.code.clone().unwrap_or_else(|| entry.ended.clone()),
                            ),
                        },
                        None => match span["ended"].as_str() {
                            Some("value") => {
                                (Symbol::Success, Token::Success, "answered".to_string())
                            }
                            _ => (
                                Symbol::Failure,
                                Token::Failure,
                                span["code"].as_str().unwrap_or("failed").to_string(),
                            ),
                        },
                    };
                    line.push(text("  "));
                    line.push(Bit::Symbol(symbol, token));
                    line.push(Bit::Painted(token, word));
                    out.push(line);
                    if let Some(entry) = entry {
                        let at = VALUE.max(indent + 7);
                        let mut sent = format!("{} {}", entry.method, entry.url);
                        if let Some(body) = &entry.body {
                            sent.push(' ');
                            sent.push_str(&compact(body));
                        }
                        let mut row = label(at, "sent", None);
                        let room = 80usize.saturating_sub(at + 8);
                        row.push(self.fit(None, sent, &mut cut, room));
                        out.push(row);
                        if let Some(answer) = &entry.answer {
                            let mut row = label(at, "answer", None);
                            row.push(self.fit(None, compact(answer), &mut cut, room));
                            out.push(row);
                        }
                    }
                }
                _ => {}
            }
        }
        // Steps of an action no span named.
        for step in &steps {
            if !stepped.contains(step["action"].as_str().unwrap_or_default()) {
                out.push(step_line(4, step));
            }
        }
        if let Some(failure) = document["failure"].as_object() {
            let mut line = label(4, "fail", Some(Token::Failure));
            line.push(Bit::Painted(
                Token::Failure,
                format!(
                    "{:<22}",
                    failure["name"].as_str().unwrap_or_default().to_string()
                ),
            ));
            if let Some(place) = failure["place"].as_str() {
                line.push(muted(format!("  {place}")));
            }
            out.push(line);
        }
        if let Some(fault) = document["fault"].as_object() {
            let mut line = label(2, "fault", Some(Token::Failure));
            line.push(Bit::Painted(
                Token::Failure,
                fault["classification"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            ));
            out.push(line);
            for frame in fault["frames"].as_array().into_iter().flatten() {
                out.push(vec![
                    text(" ".repeat(VALUE)),
                    muted(frame.as_str().unwrap_or_default()),
                ]);
            }
        }
        out.push(vec![text("  "), text(self.outcome())]);
        for delivery in self.deliveries {
            let mut line = label(2, "then", None);
            line.push(text(format!(
                "{:<22}",
                delivery["subscriber"].as_str().unwrap_or_default()
            )));
            let (symbol, token, word) =
                crate::dev::attempt_state(delivery["state"].as_str().unwrap_or_default());
            line.push(text("  "));
            line.push(Bit::Symbol(symbol, token));
            line.push(Bit::Painted(token, word.to_string()));
            if let Some(ms) = delivery["ms"].as_u64() {
                line.push(muted(format!("  {ms} ms")));
            }
            line.push(muted(format!(
                "  on {}",
                event_name(delivery["event"].as_str().unwrap_or_default())
            )));
            out.push(line);
        }
        (out, cut)
    }

    /// `POST /todos/0192…/complete  409 todo_mutation_conflict  41 ms
    /// as user 0192…a7`, or a test's name.
    fn headline(&self) -> Line {
        let document = self.document;
        if let Some(test) = document.get("test") {
            return vec![
                Bit::Bold(format!(
                    "test \"{}\"",
                    test["name"].as_str().unwrap_or_default()
                )),
                muted(format!(
                    "  trace {}",
                    document["traceId"].as_str().unwrap_or_default()
                )),
            ];
        }
        let path = crate::dev::shorten(document["path"].as_str().unwrap_or_default());
        let status = document["status"].as_u64().unwrap_or_default();
        let token = match status {
            200..=399 => Token::Success,
            400..=499 => Token::Warning,
            _ => Token::Failure,
        };
        let mut line = vec![
            Bit::Bold(format!(
                "{} {path}",
                document["method"].as_str().unwrap_or_default()
            )),
            text("  "),
            Bit::Painted(token, status.to_string()),
        ];
        if let Some(code) = document["code"].as_str() {
            line.push(text(format!(" {code}")));
        }
        if let Some(ms) = document["ms"].as_u64() {
            line.push(muted(format!("  {ms} ms")));
        }
        let caller = match document["caller"].as_str() {
            Some(caller) => match caller.split_once(' ') {
                Some((kind, id)) => format!("as {kind} {}", short(id)),
                None => format!("as {caller}"),
            },
            None => "not signed in".to_string(),
        };
        line.push(muted(format!("  {caller}")));
        line
    }

    /// `committed; emitted TodoAdded`, `rolled back; no events emitted`.
    fn outcome(&self) -> String {
        let transactions = self.document["transactions"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let wrote = |committed: bool| {
            transactions
                .iter()
                .any(|end| end["wrote"] == true && end["committed"] == committed)
        };
        let written = if wrote(true) {
            "committed"
        } else if wrote(false) {
            "rolled back"
        } else {
            "wrote nothing"
        };
        let emitted: Vec<&str> = self.document["emitted"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(event_name)
            .collect();
        if emitted.is_empty() {
            format!("{written}; no events emitted")
        } else {
            format!("{written}; emitted {}", emitted.join(", "))
        }
    }
}

/// A span's time, when its host had a clock: `  3 ms`.
fn timing(span: &Value) -> Vec<Bit> {
    match span["ms"].as_u64() {
        Some(ms) => vec![muted(format!("{ms:>5} ms"))],
        None => Vec::new(),
    }
}

/// How deep span `at` is nested, from its parents.
fn depth(spans: &[Value], at: usize) -> usize {
    let mut depth = 0;
    let mut parent = spans[at]["parent"].as_u64();
    while let Some(up) = parent {
        depth += 1;
        parent = spans
            .get(up as usize)
            .and_then(|span| span["parent"].as_u64());
        if depth > spans.len() {
            break;
        }
    }
    depth
}

/// A step under its action: `step    2 sheets.put  live  answered`.
fn step_line(indent: usize, step: &Value) -> Line {
    let mut line = label(indent, "step", None);
    line.push(text(format!(
        "{} {}",
        step["step"].as_u64().unwrap_or_default(),
        step["operation"].as_str().unwrap_or_default()
    )));
    if let Some(source) = step["source"].as_str()
        && step["state"] != "replayed"
    {
        line.push(muted(format!("  {source}")));
    }
    let (token, word) = match step["state"].as_str().unwrap_or_default() {
        "failed" => (
            Token::Failure,
            step["code"].as_str().unwrap_or("failed").to_string(),
        ),
        "in_doubt" => (Token::Warning, "in doubt".to_string()),
        "diverged" => (Token::Decision, "diverged".to_string()),
        "outcome_unknown" => (Token::Decision, "outcome unknown".to_string()),
        other => (Token::Muted, other.replace('_', " ")),
    };
    line.push(Bit::Painted(token, format!("  {word}")));
    line
}

/// A parameter as a line shows it: a UUID or a hash shortened.
fn shown_param(param: &str) -> String {
    if let Some(hash) = param.strip_prefix("hmac-sha256:") {
        return format!("#{}", &hash[..hash.len().min(8)]);
    }
    if param.starts_with('[') {
        let inner = param.trim_start_matches('[').trim_end_matches(']');
        let items: Vec<String> = inner.split(", ").map(shown_param).collect();
        return format!("[{}]", items.join(", "));
    }
    short(param)
}

/// JSON text on one line, or as it is when it isn't JSON.
fn compact(text: &str) -> String {
    serde_json::from_str::<Value>(text)
        .map_or_else(|_| text.replace('\n', " "), |value| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> Value {
        json!({
            "schemaVersion": 1,
            "requestId": "req_0b1e5f0a9c3d2e1f",
            "traceId": "0af7651916cd43dd8448eb211c80319c",
            "method": "POST",
            "path": "/todos/0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1da7/done",
            "route": "POST /todos/{todo_id}/done",
            "routePlace": "domains/planning/routes/todos.jadpo:31",
            "status": 409,
            "code": "todo_already_done",
            "ms": 41,
            "caller": "user 0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1da7",
            "spans": [
                { "kind": "action", "name": "complete_todo", "place": "domains/planning/actions/complete_todo.jadpo:4",
                  "parent": null, "ms": 38, "rows": null, "ended": "failed", "code": "todo_already_done", "statements": [] },
                { "kind": "read", "name": "Todo.get", "place": "domains/planning/actions/complete_todo.jadpo:5",
                  "parent": 0, "ms": 3, "rows": 1, "ended": "value", "code": null,
                  "statements": [{ "sql": "SELECT \"id\", \"title\" FROM \"todos\" WHERE \"id\" = $1", "params": ["0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1da7"], "rows": 1 }] }
            ],
            "transactions": [{ "committed": false, "wrote": false }],
            "emitted": [],
            "failure": { "name": "TodoAlreadyDone", "code": "todo_already_done", "place": "domains/planning/actions/complete_todo.jadpo:9" },
            "calls": [], "steps": [], "fault": null,
            "flags": { "queries": 1, "unindexed": [] }
        })
    }

    fn plain(lines: Vec<Line>) -> String {
        let screen = Screen::new().with(Block::Lines(lines));
        screen.render(&jadpo_term::Style::new(jadpo_term::Mode::Plain))
    }

    /// The page's example, as this request's timeline reads.
    #[test]
    fn a_timeline_reads_route_action_query_and_failure_in_order() {
        let document = document();
        let shown = Shown {
            document: &document,
            calls: &[],
            deliveries: &[],
            full: false,
        };
        let (lines, cut) = shown.lines();
        assert!(!cut);
        let text = plain(lines);
        let expected = "\
POST /todos/0192…/done  409 todo_already_done  41 ms  as user 0192…a7
  route     domains/planning/routes/todos.jadpo:31
  action    complete_todo            38 ms  domains/planning/actions/complete_todo.jadpo:4
    query   Todo.get                  3 ms  1 row
            SELECT … FROM \"todos\" WHERE \"id\" = $1
            $1 0192…a7
    fail    TodoAlreadyDone         domains/planning/actions/complete_todo.jadpo:9
  wrote nothing; no events emitted
";
        assert_eq!(text, expected);
    }

    /// A request that ran several queries, or one no index serves, is
    /// flagged; one query is not.
    #[test]
    fn several_queries_or_one_without_an_index_are_flagged() {
        let mut document = document();
        let one = Flags::of(&document, &BTreeSet::new());
        assert!(!one.any());
        document["spans"][1]["name"] = json!("list_todos");
        let unindexed: BTreeSet<String> = ["list_todos".to_string()].into();
        let flagged = Flags::of(&document, &unindexed);
        assert_eq!(flagged.words(), "no index");
        let statements = document["spans"][1]["statements"].clone();
        document["spans"][1]["statements"] = json!([statements[0], statements[0], statements[0]]);
        assert_eq!(
            Flags::of(&document, &unindexed).words(),
            "3 queries · no index"
        );
    }

    #[test]
    fn a_hash_and_an_id_are_shortened() {
        assert_eq!(shown_param("hmac-sha256:3f2a9c1e00aa"), "#3f2a9c1e");
        assert_eq!(
            shown_param("0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1da7"),
            "0192…a7"
        );
        assert_eq!(shown_param("7"), "7");
        assert_eq!(
            statement("SELECT \"a\", \"b\" FROM \"t\""),
            "SELECT … FROM \"t\""
        );
    }
}
