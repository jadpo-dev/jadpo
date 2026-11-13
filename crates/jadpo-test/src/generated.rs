//! The tests the compiler generates from what it already knows of each
//! route (area 13, "Tests generated for each route"; chunk W2): that it refuses bad input
//! with `{ field, rule }` details, that each failure it declares reaches
//! the client in its envelope with its status, and that its output sends
//! no field its type doesn't declare.
//!
//! What a route must do comes from its API's OpenAPI document, which the
//! check writes from the route's declarations: the input's required
//! fields and their kinds, each declared failure's status, code and
//! client fields, and the output's schema. A generated test sends the
//! route requests made from its codecs, through the HTTP component in the
//! test's own context as a route test's are, and holds each answer to
//! the document. Its request is answered as a caller of the kind the
//! route takes, given in place of signing one in, since what it checks
//! comes after sign-in; and for a failure or an output, with that failure
//! or a value of the output in place of what the route runs, so the
//! envelope and the fields are seen as a client sees them, whichever
//! executor wrote them.
//!
//! [`discover`] finds them, each with the route's place, so the report
//! shows it under the route it tests; [`exchanges`] makes a run's
//! requests on its own thread, and [`judge`] holds their answers to what
//! the document says.

use std::collections::BTreeMap;
use std::sync::Arc;

use jadpo_graph::{Graph, Kind};
use jadpo_interp::Prepared;
use jadpo_runtime::fakes::{Making, generated};
use jadpo_runtime::http::App;
use jadpo_runtime::test_run::{Answer, Exchange};
use jadpo_runtime::{Caller, Executor, Failure, Forced};
use jadpo_std::HttpMethod;
use jadpo_std::http::{PrincipalKind, RoutePlan, Run, Segment};
use jadpo_value::json::Path;
use serde_json::{Map, Value as J};

use crate::{Test, TestKind};

/// The start of every generated test's key, which no `test` declaration's
/// has: `generated bad_input route POST /todos`.
pub const GENERATED_KEY: &str = "generated ";

/// The member a request gives that no input declares.
const UNKNOWN: &str = "not_a_field";

/// The text a request sends where a value of another kind belongs: an
/// answer that shows it has sent a value back.
const SENT: &str = "jadpo-generated-value";

/// What a generated test checks of its route.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Check {
    /// It refuses bad input with `{ field, rule }` details.
    BadInput,
    /// Each failure it declares reaches the client in its envelope.
    Failures,
    /// Its output has no field its type doesn't declare.
    Output,
}

impl Check {
    fn word(self) -> &'static str {
        match self {
            Check::BadInput => "bad_input",
            Check::Failures => "failures",
            Check::Output => "output",
        }
    }

    /// The test's sentence, after the route it tests.
    fn sentence(self, label: &str) -> String {
        match self {
            Check::BadInput => format!("{label} refuses bad input with {{ field, rule }}"),
            Check::Failures => format!("{label} sends each failure it declares in its envelope"),
            Check::Output => format!("{label} sends only the fields its output declares"),
        }
    }
}

/// A request a generated test sends, as data every thread can read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sent {
    pub method: String,
    pub path: String,
    pub query: Option<String>,
    pub headers: Vec<(String, String)>,
    pub body: String,
    /// What it does, as the report says it: `leaves out \`title\``.
    pub does: String,
}

impl Sent {
    /// The request's first line: `POST /todos?not_a_field=…`.
    pub fn line(&self) -> String {
        match &self.query {
            Some(query) => format!("{} {}?{query}", self.method, self.path),
            None => format!("{} {}", self.method, self.path),
        }
    }
}

/// What a request is answered with in place of what its route runs.
#[derive(Clone, Debug, PartialEq)]
pub enum Forcing {
    Nothing,
    /// The declared failure with this code, its client fields' values as
    /// JSON.
    Failure {
        code: String,
        client: Vec<(String, J)>,
    },
    /// A value of the route's output, as JSON.
    Output(J),
}

/// What an answer must be.
#[derive(Clone, Debug, PartialEq)]
pub enum Expect {
    /// 422 `invalid_input`, naming this problem among its `problems`.
    Problem { field: String, rule: String },
    /// The failure's envelope: its status, its code and the client fields
    /// its `details` holds.
    Failure {
        status: u16,
        code: String,
        details: Vec<String>,
    },
    /// The success's status, and the schema no member it sends may go
    /// beyond.
    Output { status: u16, schema: J },
}

/// One request of a generated test, what it forces and what its answer
/// must be.
#[derive(Clone, Debug, PartialEq)]
pub struct Planned {
    pub sent: Sent,
    pub forcing: Forcing,
    pub expect: Expect,
}

/// A generated test: its route and what it checks, with the requests it
/// sends.
#[derive(Clone, Debug, PartialEq)]
pub struct Generated {
    pub check: Check,
    /// Its route's number in the table.
    pub route: u32,
    /// `POST /todos`.
    pub label: String,
    pub planned: Vec<Planned>,
    /// The route's API's document, which the schemas name parts of.
    pub document: Arc<J>,
}

/// Every generated test of a run, by key.
pub type Table = BTreeMap<String, Generated>;

/// Every generated test of `prepared`'s routes, each as a test of its
/// route's place in `graph`, by the documents `documents` gives (each
/// API's, by name), asked for only when the application declares a
/// route: the tests, in no order, and what each sends.
pub fn discover(
    prepared: &Arc<Prepared>,
    graph: &Graph,
    documents: impl FnOnce() -> Vec<(String, jadpo_graph::json::Value)>,
) -> (Vec<Test>, Table) {
    let program = jadpo_interp::Program::new(Arc::clone(prepared));
    let executor: &dyn Executor = &program;
    let app = executor.app();
    let mut tests = Vec::new();
    let mut table = Table::new();
    if !app.table.routes.iter().any(|route| {
        matches!(
            route.run,
            Run::Action(_) | Run::Query(_) | Run::Listing { .. }
        )
    }) {
        return (tests, table);
    }
    let documents = documents();
    let documents: Vec<(&str, Arc<J>)> = documents
        .iter()
        .map(|(api, document)| (api.as_str(), Arc::new(converted(document))))
        .collect();
    for (number, route) in app.table.routes.iter().enumerate() {
        // Only the routes the application writes: the runtime's own are
        // proven by its own tests.
        if !matches!(
            route.run,
            Run::Action(_) | Run::Query(_) | Run::Listing { .. }
        ) {
            continue;
        }
        let Some(place) = graph
            .ids()
            .map(|id| graph.node(id))
            .find(|node| node.kind == Kind::Route && node.key.as_str() == route.key)
            .and_then(|node| node.place.clone())
        else {
            continue;
        };
        let Some(api) = app.table.apis.get(route.api as usize) else {
            continue;
        };
        let Some(document) = documents
            .iter()
            .find(|(name, _)| *name == api.name)
            .map(|(_, document)| Arc::clone(document))
        else {
            continue;
        };
        let Some(operation) = document
            .get("paths")
            .and_then(|paths| paths.get(route.path()))
            .and_then(|path| path.get(route.method.word().to_ascii_lowercase()))
            .cloned()
        else {
            continue;
        };
        let number = u32::try_from(number).unwrap_or(u32::MAX);
        let making = Maker::new(number);
        let route_doc = RouteDoc {
            app: &app,
            route,
            document: &document,
            operation: &operation,
        };
        let mut checks = vec![(Check::BadInput, route_doc.bad_input(&making))];
        if let Some(valid) = route_doc.valid(&making) {
            let failures = route_doc.failures(&valid, &making);
            if !failures.is_empty() {
                checks.push((Check::Failures, failures));
            }
            if let Some(output) = route_doc.output(&valid, &making) {
                checks.push((Check::Output, vec![output]));
            }
        }
        for (check, planned) in checks {
            if planned.is_empty() {
                continue;
            }
            let key = format!("{GENERATED_KEY}{} {}", check.word(), route.key);
            tests.push(Test {
                key: key.clone(),
                name: check.sentence(&route.label()),
                file: place.file.clone(),
                line: place.lines.0,
                end_line: place.lines.1,
                kind: TestKind::Route,
            });
            table.insert(
                key,
                Generated {
                    check,
                    route: number,
                    label: route.label(),
                    planned,
                    document: Arc::clone(&document),
                },
            );
        }
    }
    (tests, table)
}

/// The key of the route a generated test's key names, `route POST
/// /todos`; `None` for any other test's.
pub fn route_key(key: &str) -> Option<&str> {
    let rest = key.strip_prefix(GENERATED_KEY)?;
    let (_, route) = rest.split_once(' ')?;
    Some(route)
}

/// What makes values for a route's requests: from a clock fixed at the
/// test clock's start and identities numbered from the route's, so every
/// run sends the same.
struct Maker {
    route: u32,
    next: std::cell::Cell<u32>,
}

impl Maker {
    fn new(route: u32) -> Maker {
        Maker {
            route,
            next: std::cell::Cell::new(0),
        }
    }

    /// The JSON a value of codec `codec` is made as.
    fn made(&self, app: &App<'_>, codec: jadpo_std::http::CodecId) -> J {
        let mut uuid = || {
            let n = self.next.get();
            self.next.set(n + 1);
            format!("00000000-0000-4000-8000-{:06x}{:06x}", self.route, n)
        };
        let mut making = Making {
            now: jadpo_runtime::Clock::TEST.now(),
            uuid: &mut uuid,
        };
        generated(&app.codecs, codec, &mut making)
    }
}

/// A route with its API's document and its operation there.
struct RouteDoc<'r> {
    app: &'r App<'r>,
    route: &'r RoutePlan,
    document: &'r J,
    operation: &'r J,
}

/// A valid request to a route, before it is sent: its path, its query
/// string's pairs or its body's members, and its header fields.
#[derive(Clone)]
struct Valid {
    path: String,
    query: Vec<(String, String)>,
    body: Option<Map<String, J>>,
    headers: Vec<(String, String)>,
    /// Whether every value in it reads back without a problem.
    clean: bool,
}

impl RouteDoc<'_> {
    /// `schema` with each `$ref` into the document followed.
    fn resolved<'s>(&'s self, schema: &'s J) -> &'s J {
        let mut at = schema;
        for _ in 0..16 {
            let Some(reference) = at.get("$ref").and_then(J::as_str) else {
                return at;
            };
            let Some(found) = reference
                .strip_prefix('#')
                .and_then(|pointer| self.document.pointer(pointer))
            else {
                return at;
            };
            at = found;
        }
        at
    }

    /// The request every check starts from, with values made for its
    /// placeholders and its input.
    fn base(&self, making: &Maker) -> Valid {
        let mut clean = true;
        let mut path = String::new();
        for segment in &self.route.segments {
            path.push('/');
            match segment {
                Segment::Fixed(text) => path.push_str(text),
                Segment::Param { codec, .. } => {
                    let json = making.made(self.app, *codec);
                    clean &= reads(self.app, *codec, &json);
                    path.push_str(&encoded(&text_of(&json)));
                }
            }
        }
        if path.is_empty() {
            path.push('/');
        }
        let mut valid = Valid {
            path,
            query: Vec::new(),
            body: None,
            headers: Vec::new(),
            clean,
        };
        let Some(input) = &self.route.input else {
            return valid;
        };
        let json = making.made(self.app, input.codec);
        valid.clean &= reads(self.app, input.codec, &json);
        let mut object = match json {
            J::Object(object) => object,
            _ => Map::new(),
        };
        for (field, header) in &input.headers {
            if let Some(value) = object.remove(field)
                && !value.is_null()
            {
                valid.headers.push((header.clone(), text_of(&value)));
            }
        }
        if input.query {
            for (key, value) in &object {
                let items = match value {
                    J::Array(items) => items.clone(),
                    J::Null => Vec::new(),
                    other => vec![other.clone()],
                };
                for item in items {
                    valid.query.push((key.clone(), text_of(&item)));
                }
            }
        } else {
            valid.body = Some(object);
        }
        valid
    }

    /// The request `valid` is, doing `does`.
    fn sent(&self, valid: &Valid, does: &str) -> Sent {
        let mut headers = valid.headers.clone();
        if valid.body.is_some() {
            headers.push(("content-type".to_string(), "application/json".to_string()));
        }
        // Every request but a `GET` sends a key of its own, as a client
        // would.
        if self.route.method != HttpMethod::Get {
            let key = jadpo_runtime::test_run::seed(&format!("{} {does}", self.route.key));
            headers.push((
                "idempotency-key".to_string(),
                format!("generated-{key:016x}"),
            ));
        }
        let query = (!valid.query.is_empty()).then(|| {
            valid
                .query
                .iter()
                .map(|(key, value)| format!("{}={}", encoded(key), encoded(value)))
                .collect::<Vec<_>>()
                .join("&")
        });
        Sent {
            method: self.route.method.word().to_string(),
            path: valid.path.clone(),
            query,
            headers,
            body: valid
                .body
                .as_ref()
                .map(|body| J::Object(body.clone()).to_string())
                .unwrap_or_default(),
            does: does.to_string(),
        }
    }

    /// The valid request, when every value made for it reads back.
    fn valid(&self, making: &Maker) -> Option<Valid> {
        let valid = self.base(making);
        valid.clean.then_some(valid)
    }

    /// What the bad-input test sends: a member no input declares; each
    /// required member left out; each member given a value of another
    /// kind; and each placeholder given text its type can't read.
    fn bad_input(&self, making: &Maker) -> Vec<Planned> {
        let base = self.base(making);
        let mut out = Vec::new();
        let problem = |field: &str, rule: &str| Expect::Problem {
            field: field.to_string(),
            rule: rule.to_string(),
        };
        // A query string's member it doesn't declare, whatever it reads.
        let mut valid = base.clone();
        valid.query.push((UNKNOWN.to_string(), SENT.to_string()));
        out.push(Planned {
            sent: self.sent(&valid, &format!("sends `{UNKNOWN}` in its query string")),
            forcing: Forcing::Nothing,
            expect: problem(UNKNOWN, "unknown_field"),
        });
        // Each placeholder given text that isn't a value of its type.
        for parameter in self.parameters("path") {
            let Some(rule) = self.wrong_text_rule(&parameter.1) else {
                continue;
            };
            let mut valid = base.clone();
            valid.path = replaced_segment(&valid.path, &self.route.segments, &parameter.0);
            out.push(Planned {
                sent: self.sent(
                    &valid,
                    &format!("gives `{}` text it can't read", parameter.0),
                ),
                forcing: Forcing::Nothing,
                expect: problem(&parameter.0, rule),
            });
        }
        if let Some(body) = &base.body {
            let schema = self.body_schema();
            let mut valid = base.clone();
            if let Some(object) = valid.body.as_mut() {
                object.insert(UNKNOWN.to_string(), J::String(SENT.to_string()));
            }
            out.push(Planned {
                sent: self.sent(&valid, &format!("sends `{UNKNOWN}` in its body")),
                forcing: Forcing::Nothing,
                expect: problem(UNKNOWN, "unknown_field"),
            });
            for name in required(schema) {
                if !body.contains_key(&name) {
                    continue;
                }
                let mut valid = base.clone();
                if let Some(object) = valid.body.as_mut() {
                    object.remove(&name);
                }
                out.push(Planned {
                    sent: self.sent(&valid, &format!("leaves out `{name}`")),
                    forcing: Forcing::Nothing,
                    expect: problem(&name, "required"),
                });
            }
            for (name, property) in properties(schema) {
                if !body.contains_key(&name) {
                    continue;
                }
                let Some(wrong) = self.wrong_kind(property) else {
                    continue;
                };
                let mut valid = base.clone();
                if let Some(object) = valid.body.as_mut() {
                    object.insert(name.clone(), wrong);
                }
                out.push(Planned {
                    sent: self.sent(&valid, &format!("gives `{name}` a value of another kind")),
                    forcing: Forcing::Nothing,
                    expect: problem(&name, "type"),
                });
                for (breaking, rule) in self.breaking(property) {
                    let mut valid = base.clone();
                    if let Some(object) = valid.body.as_mut() {
                        object.insert(name.clone(), breaking);
                    }
                    out.push(Planned {
                        sent: self
                            .sent(&valid, &format!("gives `{name}` a value past its `{rule}`")),
                        forcing: Forcing::Nothing,
                        expect: problem(&name, rule),
                    });
                }
            }
        }
        if self.route.input.as_ref().is_some_and(|input| input.query) {
            for (name, parameter, needed) in self.query_parameters() {
                if needed && base.query.iter().any(|(key, _)| *key == name) {
                    let mut valid = base.clone();
                    valid.query.retain(|(key, _)| *key != name);
                    out.push(Planned {
                        sent: self.sent(&valid, &format!("leaves out `{name}`")),
                        forcing: Forcing::Nothing,
                        expect: problem(&name, "required"),
                    });
                }
                let Some(rule) = self.wrong_text_rule(&parameter) else {
                    continue;
                };
                let mut valid = base.clone();
                valid.query.retain(|(key, _)| *key != name);
                valid.query.push((name.clone(), SENT.to_string()));
                out.push(Planned {
                    sent: self.sent(&valid, &format!("gives `{name}` text it can't read")),
                    forcing: Forcing::Nothing,
                    expect: problem(&name, rule),
                });
            }
        }
        out
    }

    /// The operation's parameters in `place` (`path`, `query`), each by
    /// name with its schema, in order.
    fn parameters(&self, place: &str) -> Vec<(String, J)> {
        self.operation
            .get("parameters")
            .and_then(J::as_array)
            .into_iter()
            .flatten()
            .map(|parameter| self.resolved(parameter))
            .filter(|parameter| parameter.get("in").and_then(J::as_str) == Some(place))
            .filter_map(|parameter| {
                Some((
                    parameter.get("name")?.as_str()?.to_string(),
                    parameter.get("schema").cloned().unwrap_or(J::Null),
                ))
            })
            .collect()
    }

    /// The query parameters, each with its schema and whether it is
    /// required.
    fn query_parameters(&self) -> Vec<(String, J, bool)> {
        self.operation
            .get("parameters")
            .and_then(J::as_array)
            .into_iter()
            .flatten()
            .map(|parameter| self.resolved(parameter))
            .filter(|parameter| parameter.get("in").and_then(J::as_str) == Some("query"))
            .filter_map(|parameter| {
                Some((
                    parameter.get("name")?.as_str()?.to_string(),
                    parameter.get("schema").cloned().unwrap_or(J::Null),
                    parameter.get("required").and_then(J::as_bool) == Some(true),
                ))
            })
            .collect()
    }

    /// The schema of the body the operation reads.
    fn body_schema(&self) -> &J {
        self.operation
            .pointer("/requestBody/content/application~1json/schema")
            .map_or(&J::Null, |schema| self.resolved(schema))
    }

    /// The rule a value of `schema` given text it can't read breaks: a
    /// number's or a yes-or-no's `type`, a UUID's `format`; `None` for
    /// text, which reads any.
    fn wrong_text_rule(&self, schema: &J) -> Option<&'static str> {
        let schema = self.resolved(schema);
        if schema.get("enum").is_some() {
            return None;
        }
        match kinds(schema).as_slice() {
            ["integer"] | ["number"] | ["boolean"] => Some("type"),
            ["string"] if schema.get("format").and_then(J::as_str) == Some("uuid") => {
                Some("format")
            }
            _ => None,
        }
    }

    /// A value of another kind than `schema` declares: a yes-or-no, or for
    /// a yes-or-no, text; `None` where the schema doesn't say one kind.
    fn wrong_kind(&self, schema: &J) -> Option<J> {
        let schema = self.resolved(schema);
        match kinds(schema).as_slice() {
            ["boolean"] => Some(J::String(SENT.to_string())),
            [_] => Some(J::Bool(true)),
            _ => None,
        }
    }

    /// A value of the kind `schema` declares past each rule it gives, with
    /// the rule: a whole number past its `minimum` or `maximum`, and text
    /// of no other form past its `minLength` or `maxLength`.
    fn breaking(&self, schema: &J) -> Vec<(J, &'static str)> {
        let schema = self.resolved(schema);
        let number = |name: &str| schema.get(name).and_then(J::as_i64);
        let mut out = Vec::new();
        match kinds(schema).as_slice() {
            ["integer"] => {
                if let Some(least) = number("minimum").and_then(|n| n.checked_sub(1)) {
                    out.push((J::from(least), "min"));
                }
                if let Some(most) = number("maximum").and_then(|n| n.checked_add(1)) {
                    out.push((J::from(most), "max"));
                }
            }
            ["string"]
                if ["format", "pattern", "enum"]
                    .iter()
                    .all(|word| schema.get(*word).is_none()) =>
            {
                if let Some(least) = number("minLength").filter(|n| (1..=1000).contains(n)) {
                    out.push((J::String("x".repeat((least - 1) as usize)), "min_length"));
                }
                if let Some(most) = number("maxLength").filter(|n| (0..1000).contains(n)) {
                    out.push((J::String("x".repeat((most + 1) as usize)), "max_length"));
                }
            }
            _ => {}
        }
        out
    }

    /// The failures the operation declares, each as the route's valid
    /// request answered with it, with a value made for each of its client
    /// fields.
    fn failures(&self, valid: &Valid, making: &Maker) -> Vec<Planned> {
        let mut out: Vec<Planned> = Vec::new();
        let Some(responses) = self.operation.get("responses").and_then(J::as_object) else {
            return out;
        };
        for (status, response) in responses {
            let Ok(status) = status.parse::<u16>() else {
                continue;
            };
            let response = self.resolved(response);
            let Some(schema) = response.pointer("/content/application~1json/schema") else {
                continue;
            };
            let schema = self.resolved(schema);
            let alternatives: Vec<&J> = match schema.get("oneOf").and_then(J::as_array) {
                Some(alternatives) => alternatives.iter().map(|one| self.resolved(one)).collect(),
                None => vec![schema],
            };
            for failure in alternatives {
                let Some(code) = failure.pointer("/x-jadpo-failure/code").and_then(J::as_str)
                else {
                    continue;
                };
                if out.iter().any(|planned| {
                    matches!(&planned.expect, Expect::Failure { code: seen, .. } if seen == code)
                }) {
                    continue;
                }
                let details: Vec<String> = failure
                    .pointer("/properties/error/properties/details")
                    .map(|details| properties(self.resolved(details)))
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(name, _)| name)
                    .collect();
                let client = self
                    .app
                    .failures
                    .iter()
                    .find(|plan| plan.code == code)
                    .and_then(|plan| self.app.table.failure(&plan.key))
                    .map(|codec| {
                        codec
                            .client
                            .iter()
                            .map(|(name, id)| (name.clone(), making.made(self.app, *id)))
                            .collect()
                    })
                    .unwrap_or_default();
                out.push(Planned {
                    sent: self.sent(valid, &format!("is answered with `{code}`")),
                    forcing: Forcing::Failure {
                        code: code.to_string(),
                        client,
                    },
                    expect: Expect::Failure {
                        status,
                        code: code.to_string(),
                        details,
                    },
                });
            }
        }
        out
    }

    /// The route's valid request answered with a value made for its
    /// output, when it sends one as JSON and a value can be made for it.
    fn output(&self, valid: &Valid, making: &Maker) -> Option<Planned> {
        let route = self.route;
        if route.stream || route.sends.is_some() || route.settle.is_some() {
            return None;
        }
        let codec = route.output?;
        let json = making.made(self.app, codec);
        if !reads(self.app, codec, &json) {
            return None;
        }
        let responses = self.operation.get("responses")?.as_object()?;
        let (status, schema) = responses.iter().find_map(|(status, response)| {
            let status = status.parse::<u16>().ok()?;
            if !(200..300).contains(&status) {
                return None;
            }
            let schema = self
                .resolved(response)
                .pointer("/content/application~1json/schema")?;
            Some((status, schema.clone()))
        })?;
        Some(Planned {
            sent: self.sent(valid, "is answered with a value of its output"),
            forcing: Forcing::Output(json),
            expect: Expect::Output { status, schema },
        })
    }
}

/// A document the check wrote, as JSON.
fn converted(value: &jadpo_graph::json::Value) -> J {
    use jadpo_graph::json::Value as G;
    match value {
        G::Null => J::Null,
        G::Bool(yes) => J::Bool(*yes),
        G::Int(n) => J::from(*n),
        G::Str(text) => J::String(text.clone()),
        G::Array(items) => J::Array(items.iter().map(converted).collect()),
        G::Object(members) => J::Object(
            members
                .iter()
                .map(|(name, member)| (name.clone(), converted(member)))
                .collect(),
        ),
    }
}

/// Whether `json` reads as a value of codec `codec` without a problem.
fn reads(app: &App<'_>, codec: jadpo_std::http::CodecId, json: &J) -> bool {
    let mut problems = Vec::new();
    app.codecs
        .read(codec, json, &Path::default(), &mut problems)
        .is_some()
        && problems.is_empty()
}

/// The JSON kinds `schema` allows, `null` aside.
fn kinds(schema: &J) -> Vec<&str> {
    match schema.get("type") {
        Some(J::String(kind)) => vec![kind.as_str()],
        Some(J::Array(kinds)) => kinds
            .iter()
            .filter_map(J::as_str)
            .filter(|kind| *kind != "null")
            .collect(),
        _ => Vec::new(),
    }
}

/// The names `schema` lists as required.
fn required(schema: &J) -> Vec<String> {
    schema
        .get("required")
        .and_then(J::as_array)
        .into_iter()
        .flatten()
        .filter_map(J::as_str)
        .map(str::to_string)
        .collect()
}

/// The properties `schema` declares, by name.
fn properties(schema: &J) -> Vec<(String, &J)> {
    schema
        .get("properties")
        .and_then(J::as_object)
        .into_iter()
        .flatten()
        .map(|(name, property)| (name.clone(), property))
        .collect()
}

/// A value's text as a path segment, a query value or a header holds it:
/// a string's own text, any other value as JSON.
fn text_of(json: &J) -> String {
    match json {
        J::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// `text` percent-encoded for a path segment or a query string.
fn encoded(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// `path` with placeholder `name`'s segment replaced by text no number,
/// yes-or-no or UUID is.
fn replaced_segment(path: &str, segments: &[Segment], name: &str) -> String {
    let parts: Vec<String> = path
        .split('/')
        .skip(1)
        .zip(segments)
        .map(|(part, segment)| match segment {
            Segment::Param { name: own, .. } if own == name => SENT.to_string(),
            _ => part.to_string(),
        })
        .collect();
    format!("/{}", parts.join("/"))
}

/// The requests generated test `generated` sends, made for `executor`'s
/// program on this thread: each answered as a caller of the kind its
/// route takes, with a new identity from the test's key, and with what it
/// forces read as the program's values. `Err` names a failure the
/// document declares that the program has none of.
pub fn exchanges(
    executor: &dyn Executor,
    key: &str,
    generated: &Generated,
) -> Result<Vec<Exchange>, String> {
    let app = executor.app();
    let route = app
        .table
        .routes
        .get(generated.route as usize)
        .ok_or_else(|| format!("{} isn't in the program's routes", generated.label))?;
    let caller = route
        .authentication
        .needs_caller()
        .then(|| caller(&app, route, key))
        .flatten();
    let mut out = Vec::new();
    for planned in &generated.planned {
        let forced = match &planned.forcing {
            Forcing::Nothing => None,
            Forcing::Failure { code, client } => Some(Forced::Failure(Box::new(
                failure(&app, code, client)
                    .ok_or_else(|| format!("the program declares no failure `{code}`"))?,
            ))),
            Forcing::Output(json) => {
                let codec = route.output.ok_or("the route sends no output")?;
                let mut problems = Vec::new();
                let value = app
                    .codecs
                    .read(codec, json, &Path::default(), &mut problems)
                    .ok_or("its output's value doesn't read")?;
                Some(Forced::Value(value))
            }
        };
        let sent = &planned.sent;
        out.push(Exchange {
            request: jadpo_runtime::http::Request {
                method: sent.method.clone(),
                path: sent.path.clone(),
                query: sent.query.clone(),
                headers: sent.headers.clone(),
                body: sent.body.clone().into_bytes(),
                address: Some("generated test".to_string()),
                ..jadpo_runtime::http::Request::default()
            },
            generated: jadpo_runtime::Generated {
                caller: caller.clone(),
                forced,
            },
        });
    }
    Ok(out)
}

/// A caller of the kind `route` takes, with an identity made from `key`.
fn caller(app: &App<'_>, route: &RoutePlan, key: &str) -> Option<Caller> {
    let identity = app.table.identity.as_ref()?;
    let kind = route.caller.or_else(|| {
        if identity.user.is_some() {
            Some(PrincipalKind::User)
        } else if identity.service.is_some() {
            Some(PrincipalKind::Service)
        } else {
            None
        }
    })?;
    // A new identity from the test's key, read as a stored id is.
    let seed = jadpo_runtime::test_run::seed(key);
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&seed.to_be_bytes());
    bytes[8..].copy_from_slice(&jadpo_runtime::test_run::splitmix(seed).to_be_bytes());
    let id = jadpo_runtime::from_sql(
        &jadpo_runtime::SqlValue::Uuid(bytes),
        &jadpo_std::read::Column::Uuid,
    )
    .ok()?;
    Some(Caller {
        key: format!("{} {id}", kind.name()),
        value: jadpo_runtime::http::identity::principal_of(
            kind,
            id,
            jadpo_value::Value::None,
            true,
        ),
        session: None,
        sign_in: None,
        multi_factor: true,
    })
}

/// The declared failure with code `code`, made as the program raises it,
/// its client fields holding `client`'s values.
fn failure(app: &App<'_>, code: &str, client: &[(String, J)]) -> Option<Failure> {
    let plan = app.failures.iter().find(|plan| plan.code == code)?;
    let codecs = app.table.failure(&plan.key);
    let values = plan
        .fields
        .iter()
        .map(|field| {
            let json = client.iter().find(|(name, _)| *name == field.name);
            let codec = codecs.and_then(|codecs| {
                codecs
                    .client
                    .iter()
                    .find(|(name, _)| *name == field.name)
                    .map(|(_, id)| *id)
            });
            match (json, codec) {
                (Some((_, json)), Some(codec)) => {
                    let mut problems = Vec::new();
                    app.codecs
                        .read(codec, json, &Path::default(), &mut problems)
                        .unwrap_or(jadpo_value::Value::None)
                }
                _ => jadpo_value::Value::None,
            }
        })
        .collect();
    let slots: Vec<Option<u32>> = (0..plan.fields.len())
        .map(|at| u32::try_from(at).ok())
        .collect();
    Failure::of(plan, &slots, values)
}

/// Why a generated test failed: the request that broke what the document
/// says, what it should have been answered with and what it was.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Broken {
    /// The request's first line.
    pub request: String,
    /// What it does: `leaves out \`title\``.
    pub does: String,
    pub expected: String,
    pub got: String,
}

/// The first answer in `answers` that isn't what its request's `planned`
/// says, or `None` when each is.
pub fn judge(generated: &Generated, answers: &[Answer]) -> Option<Broken> {
    for (planned, answer) in generated.planned.iter().zip(answers) {
        if let Err((expected, got)) = judged(generated, planned, answer) {
            return Some(Broken {
                request: planned.sent.line(),
                does: planned.sent.does.clone(),
                expected,
                got,
            });
        }
    }
    if answers.len() < generated.planned.len() {
        return Some(Broken {
            request: generated.label.clone(),
            does: "sends its requests".to_string(),
            expected: format!("{} answers", generated.planned.len()),
            got: format!("{} answers", answers.len()),
        });
    }
    None
}

/// What `answer` should have been, and what it was, when it isn't what
/// `planned` says.
fn judged(
    generated: &Generated,
    planned: &Planned,
    answer: &Answer,
) -> Result<(), (String, String)> {
    let shown = |answer: &Answer| format!("{} {}", answer.status, answer.body);
    let body: J = serde_json::from_str(&answer.body)
        .map_err(|_| ("a JSON body".to_string(), shown(answer)))?;
    match &planned.expect {
        Expect::Problem { field, rule } => {
            let wanted =
                format!("422 invalid_input with {{ field: \"{field}\", rule: \"{rule}\" }}");
            let error = envelope(&body, answer, 422, "invalid_input")
                .map_err(|why| (wanted.clone(), format!("{} ({why})", shown(answer))))?;
            let problems = error
                .pointer("/details/problems")
                .and_then(J::as_array)
                .ok_or_else(|| {
                    (
                        wanted.clone(),
                        format!("{} (no `details.problems`)", shown(answer)),
                    )
                })?;
            for problem in problems {
                let fields = problem.as_object().map(|object| {
                    let mut names: Vec<&str> = object.keys().map(String::as_str).collect();
                    names.sort_unstable();
                    names
                });
                if fields.as_deref() != Some(&["field", "rule"][..])
                    || !problem.get("field").is_some_and(J::is_string)
                    || !problem.get("rule").is_some_and(J::is_string)
                {
                    return Err((
                        wanted,
                        format!(
                            "{} (a problem that isn't `{{ field, rule }}`)",
                            shown(answer)
                        ),
                    ));
                }
            }
            if answer.body.contains(SENT) {
                return Err((
                    wanted,
                    format!("{} (the value sent, sent back)", shown(answer)),
                ));
            }
            let named = problems.iter().any(|problem| {
                problem.get("field").and_then(J::as_str) == Some(field)
                    && problem.get("rule").and_then(J::as_str) == Some(rule)
            });
            if !named {
                return Err((wanted, shown(answer)));
            }
            Ok(())
        }
        Expect::Failure {
            status,
            code,
            details,
        } => {
            let mut fields = details.clone();
            fields.sort();
            let wanted = if fields.is_empty() {
                format!("{status} {code}")
            } else {
                format!("{status} {code} with details {{ {} }}", fields.join(", "))
            };
            let error = envelope(&body, answer, *status, code)
                .map_err(|why| (wanted.clone(), format!("{} ({why})", shown(answer))))?;
            let mut sent: Vec<String> = error
                .get("details")
                .and_then(J::as_object)
                .map(|details| details.keys().cloned().collect())
                .unwrap_or_default();
            sent.sort();
            if sent != fields {
                return Err((wanted, shown(answer)));
            }
            Ok(())
        }
        Expect::Output { status, schema } => {
            let wanted = format!("{status} with only the fields its output declares");
            if answer.status != *status {
                return Err((wanted, shown(answer)));
            }
            let mut at = Vec::new();
            if let Some(extra) = undeclared(generated, schema, &body, &mut at) {
                return Err((
                    wanted,
                    format!("{} (`{extra}` isn't declared)", shown(answer)),
                ));
            }
            Ok(())
        }
    }
}

/// The envelope `body` is, as `answer` should give it: the status, its
/// `error` with `code`, `message`, `request_id` and `details` alone, the
/// code given, and the id the `x-request-id` header repeats.
fn envelope<'b>(body: &'b J, answer: &Answer, status: u16, code: &str) -> Result<&'b J, String> {
    if answer.status != status {
        return Err(format!("not {status}"));
    }
    let error = body
        .as_object()
        .filter(|object| object.len() == 1)
        .and_then(|object| object.get("error"))
        .ok_or("not one `error`")?;
    let members = error.as_object().ok_or("an `error` that isn't an object")?;
    if let Some(extra) = members
        .keys()
        .find(|name| !matches!(name.as_str(), "code" | "message" | "request_id" | "details"))
    {
        return Err(format!("`error.{extra}` isn't part of the envelope"));
    }
    if error.get("code").and_then(J::as_str) != Some(code) {
        return Err(format!("its code isn't `{code}`"));
    }
    if !error.get("message").is_some_and(J::is_string) {
        return Err("no `message`".to_string());
    }
    let id = error.get("request_id").and_then(J::as_str);
    if id.is_none() || id != answer.header("x-request-id") {
        return Err("its `request_id` isn't the `x-request-id` header's".to_string());
    }
    Ok(error)
}

/// The first member of `json` its `schema` doesn't declare, by its path
/// from the body, `items.0.colour`.
fn undeclared(generated: &Generated, schema: &J, json: &J, at: &mut Vec<String>) -> Option<String> {
    let doc = RouteSchemas {
        document: &generated.document,
    };
    doc.undeclared(schema, json, at, 0)
}

/// The document's schemas, followed through their `$ref`s.
struct RouteSchemas<'d> {
    document: &'d J,
}

impl RouteSchemas<'_> {
    fn resolved<'s>(&'s self, schema: &'s J) -> &'s J {
        let mut at = schema;
        for _ in 0..16 {
            let Some(reference) = at.get("$ref").and_then(J::as_str) else {
                return at;
            };
            match reference
                .strip_prefix('#')
                .and_then(|pointer| self.document.pointer(pointer))
            {
                Some(found) => at = found,
                None => return at,
            }
        }
        at
    }

    /// The alternatives `schema` allows, each followed.
    fn alternatives<'s>(&'s self, schema: &'s J) -> Vec<&'s J> {
        let schema = self.resolved(schema);
        let listed = ["oneOf", "anyOf", "allOf"]
            .iter()
            .find_map(|word| schema.get(*word).and_then(J::as_array));
        match listed {
            Some(listed) => listed
                .iter()
                .flat_map(|one| self.alternatives(one))
                .collect(),
            None => vec![schema],
        }
    }

    fn undeclared(
        &self,
        schema: &J,
        json: &J,
        at: &mut Vec<String>,
        depth: usize,
    ) -> Option<String> {
        if depth > 32 {
            return None;
        }
        let alternatives = self.alternatives(schema);
        match json {
            J::Object(members) => {
                // An object's members are declared by any alternative's
                // properties; one that takes any key (a map) checks only
                // what each holds.
                let open: Vec<&J> = alternatives
                    .iter()
                    .filter_map(|one| one.get("additionalProperties"))
                    .filter(|extra| extra.is_object())
                    .collect();
                let declares = alternatives
                    .iter()
                    .any(|one| one.get("properties").is_some());
                for (name, value) in members {
                    let property = alternatives
                        .iter()
                        .find_map(|one| one.get("properties").and_then(|props| props.get(name)));
                    at.push(name.clone());
                    let found = match (property, open.first()) {
                        (Some(property), _) => self.undeclared(property, value, at, depth + 1),
                        (None, Some(values)) => self.undeclared(values, value, at, depth + 1),
                        (None, None) if declares => Some(at.join(".")),
                        (None, None) => None,
                    };
                    at.pop();
                    if found.is_some() {
                        return found;
                    }
                }
                None
            }
            J::Array(items) => {
                let item = alternatives.iter().find_map(|one| one.get("items"))?;
                for (place, value) in items.iter().enumerate() {
                    at.push(place.to_string());
                    let found = self.undeclared(item, value, at, depth + 1);
                    at.pop();
                    if found.is_some() {
                        return found;
                    }
                }
                None
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(status: u16, body: &str) -> Answer {
        Answer {
            status,
            headers: vec![("x-request-id".to_string(), "req_1".to_string())],
            body: body.to_string(),
        }
    }

    fn sent() -> Sent {
        Sent {
            method: "POST".to_string(),
            path: "/todos".to_string(),
            query: None,
            headers: Vec::new(),
            body: "{}".to_string(),
            does: "leaves out `title`".to_string(),
        }
    }

    fn generated(expect: Expect) -> Generated {
        Generated {
            check: Check::BadInput,
            route: 0,
            label: "POST /todos".to_string(),
            planned: vec![Planned {
                sent: sent(),
                forcing: Forcing::Nothing,
                expect,
            }],
            document: Arc::new(serde_json::json!({
                "components": { "schemas": {
                    "TodoView": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": { "id": { "type": "string" }, "tags": { "type": "array", "items": { "$ref": "#/components/schemas/Tag" } } }
                    },
                    "Tag": { "type": "object", "properties": { "name": { "type": "string" } } }
                } }
            })),
        }
    }

    const REFUSED: &str = r#"{"error":{"code":"invalid_input","message":"Request input is invalid.","request_id":"req_1","details":{"problems":[{"field":"title","rule":"required"}]}}}"#;

    #[test]
    fn a_refusal_naming_the_field_and_rule_passes() {
        let test = generated(Expect::Problem {
            field: "title".to_string(),
            rule: "required".to_string(),
        });
        assert_eq!(judge(&test, &[answer(422, REFUSED)]), None);
    }

    #[test]
    fn a_refusal_that_names_another_field_sends_back_a_value_or_more_than_field_and_rule_fails() {
        let test = generated(Expect::Problem {
            field: "title".to_string(),
            rule: "required".to_string(),
        });
        let other = REFUSED.replace("\"title\"", "\"due\"");
        assert!(judge(&test, &[answer(422, &other)]).is_some());
        let echoed = REFUSED.replace(
            "\"required\"}",
            &format!("\"required\",\"value\":\"{SENT}\"}}"),
        );
        let broken = judge(&test, &[answer(422, &echoed)]).expect("fails");
        assert!(broken.got.contains("isn't `{ field, rule }`"), "{broken:?}");
        let wrong_id = answer(422, &REFUSED.replace("req_1", "req_2"));
        assert!(judge(&test, &[wrong_id]).is_some());
        let created = judge(&test, &[answer(201, "{}")]).expect("fails");
        assert_eq!(
            broken.expected,
            "422 invalid_input with { field: \"title\", rule: \"required\" }"
        );
        assert!(created.got.starts_with("201 {}"), "{created:?}");
    }

    #[test]
    fn a_failure_must_carry_its_status_code_and_client_fields() {
        let test = generated(Expect::Failure {
            status: 409,
            code: "quantity_unavailable".to_string(),
            details: vec!["available".to_string()],
        });
        let body = r#"{"error":{"code":"quantity_unavailable","message":"m","request_id":"req_1","details":{"available":2}}}"#;
        assert_eq!(judge(&test, &[answer(409, body)]), None);
        assert!(judge(&test, &[answer(400, body)]).is_some());
        let more = body.replace("\"available\":2", "\"available\":2,\"reason\":\"x\"");
        assert!(judge(&test, &[answer(409, &more)]).is_some());
        let none = body.replace(",\"details\":{\"available\":2}", "");
        assert!(judge(&test, &[answer(409, &none)]).is_some());
    }

    #[test]
    fn an_output_member_its_schema_doesnt_declare_fails_by_its_path() {
        let test = generated(Expect::Output {
            status: 201,
            schema: serde_json::json!({ "$ref": "#/components/schemas/TodoView" }),
        });
        assert_eq!(
            judge(&test, &[answer(201, r#"{"id":"a","tags":[{"name":"x"}]}"#)]),
            None
        );
        let broken = judge(
            &test,
            &[answer(
                201,
                r#"{"id":"a","tags":[{"name":"x","colour":"red"}]}"#,
            )],
        )
        .expect("fails");
        assert!(
            broken.got.contains("`tags.0.colour` isn't declared"),
            "{broken:?}"
        );
        assert!(judge(&test, &[answer(200, r#"{"id":"a"}"#)]).is_some());
    }

    #[test]
    fn a_generated_key_names_its_route() {
        assert_eq!(
            route_key("generated bad_input route POST /todos"),
            Some("route POST /todos")
        );
        assert_eq!(route_key("test todos adds"), None);
    }
}
