//! The conformance test (area 11, "The registry protocol"): checks a
//! served OpenAPI document against the protocol's table, so the `jadpo`
//! tool and the registry application can't drift apart.
//!
//! Every row must be there, with the method, path, placeholders, query
//! members, body, success status, answer and failures the table gives,
//! open to a caller who hasn't signed in; nothing else may be served
//! below the protocol's path but the document itself. Shapes are compared
//! structurally, through any `$ref`, so a component's name is the
//! application's own; `null` may be written as a type list or as `anyOf`.
//! What a Jadpo application adds that the protocol doesn't constrain (the
//! runtime's shared failures, `x-jadpo-` keywords, patterns, the
//! `Idempotency-Key` header) is left alone.

use std::fmt;

use serde_json::Value;

use crate::protocol::{Answer, BASE, ENDPOINTS, Endpoint, Method};
use crate::shape::{Shape, Text, named};

/// One way a document differs from the protocol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mismatch {
    /// The request it is in, `GET /registry/v1/log/head`, or `document`.
    pub operation: String,
    pub problem: String,
}

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.operation, self.problem)
    }
}

/// Reads a document from a file or an `http://` address on this machine.
pub fn load(source: &str) -> Result<Value, String> {
    let bytes = if source.starts_with("http://") {
        let answer = crate::http::fetch(Method::Get, source, None)?;
        if answer.status != 200 {
            return Err(format!("answered {}", answer.status));
        }
        answer.body
    } else if source.contains("://") {
        return Err("only a file or an http:// address on this machine is read".into());
    } else {
        std::fs::read(source).map_err(|e| e.to_string())?
    };
    serde_json::from_slice(&bytes).map_err(|e| format!("isn't JSON: {e}"))
}

/// Every way `document` differs from the protocol; none when it conforms.
pub fn check(document: &Value) -> Vec<Mismatch> {
    let mut out = Vec::new();
    let whole = |problem: &str| Mismatch {
        operation: "document".into(),
        problem: problem.into(),
    };
    if !document["openapi"]
        .as_str()
        .is_some_and(|v| v.starts_with("3.1."))
    {
        out.push(whole("isn't an OpenAPI 3.1 document"));
    }
    let Some(paths) = document["paths"].as_object() else {
        out.push(whole("has no paths"));
        return out;
    };
    let reader = Reader { document };
    for endpoint in ENDPOINTS {
        let name = format!("{} {}", endpoint.method, endpoint.full_path());
        let operation =
            &document["paths"][endpoint.full_path()][endpoint.method.as_str().to_ascii_lowercase()];
        if !operation.is_object() {
            out.push(Mismatch {
                operation: name,
                problem: "missing".into(),
            });
            continue;
        }
        let mut problems = Vec::new();
        reader.operation(
            endpoint,
            &document["paths"][endpoint.full_path()],
            operation,
            &mut problems,
        );
        out.extend(problems.into_iter().map(|problem| Mismatch {
            operation: name.clone(),
            problem,
        }));
    }
    let own = format!("{BASE}openapi.json");
    for (path, item) in paths {
        if !path.starts_with(BASE) || *path == own {
            continue;
        }
        let Some(item) = item.as_object() else {
            continue;
        };
        for method in item.keys() {
            let Some(upper) = http_method(method) else {
                continue;
            };
            let known = ENDPOINTS
                .iter()
                .any(|e| e.full_path() == *path && e.method.as_str() == upper);
            if !known {
                out.push(Mismatch {
                    operation: format!("{upper} {path}"),
                    problem: "not in the protocol".into(),
                });
            }
        }
    }
    out
}

fn http_method(key: &str) -> Option<&'static str> {
    Some(match key {
        "get" => "GET",
        "put" => "PUT",
        "post" => "POST",
        "delete" => "DELETE",
        "patch" => "PATCH",
        "options" => "OPTIONS",
        "head" => "HEAD",
        "trace" => "TRACE",
        _ => return None,
    })
}

/// Whether a value is read from an answer, where every field is always
/// sent, or from a body or query, where an optional field may be left out.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Answer,
    Input,
}

struct Reader<'a> {
    document: &'a Value,
}

/// How deep `$ref`s and named shapes may go before a loop is assumed.
const DEPTH: usize = 64;

impl<'a> Reader<'a> {
    /// Follows `$ref`s to `#/components/...` until a schema without one.
    fn resolve<'b>(&self, mut value: &'b Value) -> Result<&'b Value, String>
    where
        'a: 'b,
    {
        for _ in 0..DEPTH {
            let Some(target) = value.get("$ref").and_then(Value::as_str) else {
                return Ok(value);
            };
            let pointer = target
                .strip_prefix('#')
                .ok_or_else(|| format!("`$ref` {target} isn't in this document"))?;
            value = self
                .document
                .pointer(pointer)
                .ok_or_else(|| format!("`$ref` {target} names nothing"))?;
        }
        Err("`$ref`s loop".into())
    }

    fn operation(&self, endpoint: &Endpoint, item: &Value, op: &Value, out: &mut Vec<String>) {
        self.security(op, out);
        self.parameters(endpoint, item, op, out);
        self.body(endpoint, op, out);
        self.answers(endpoint, op, out);
    }

    /// Reads need no sign-in, and the `jadpo` tool never signs in, so every
    /// request must be open to a caller with no credential.
    fn security(&self, op: &Value, out: &mut Vec<String>) {
        let open = |requirements: &Value| match requirements.as_array() {
            Some(list) => {
                list.is_empty()
                    || list
                        .iter()
                        .any(|r| r.as_object().is_some_and(|o| o.is_empty()))
            }
            None => false,
        };
        let needs = match op.get("security") {
            Some(requirements) => !open(requirements),
            None => self.document.get("security").is_some_and(|r| !open(r)),
        };
        if needs {
            out.push("needs a sign-in, and the jadpo tool never signs in".into());
        }
    }

    fn parameters(&self, endpoint: &Endpoint, item: &Value, op: &Value, out: &mut Vec<String>) {
        let mut served: Vec<&Value> = Vec::new();
        for list in [item.get("parameters"), op.get("parameters")]
            .into_iter()
            .flatten()
        {
            for parameter in list.as_array().into_iter().flatten() {
                match self.resolve(parameter) {
                    Ok(parameter) => served.push(parameter),
                    Err(e) => out.push(format!("a parameter: {e}")),
                }
            }
        }
        let find = |place: &str, name: &str| {
            served
                .iter()
                .find(|p| p["in"] == place && p["name"] == name)
                .copied()
        };
        for param in endpoint.path_params {
            let at = format!("path.{}", param.name);
            match find("path", param.name) {
                Some(p) => self.compare(&p["schema"], &(param.shape)(), &at, Role::Input, 0, out),
                None => out.push(format!("{at}: missing")),
            }
        }
        for param in endpoint.query {
            let at = format!("query.{}", param.name);
            match find("query", param.name) {
                Some(p) => {
                    let required = p["required"].as_bool().unwrap_or(false);
                    if required && !param.required {
                        out.push(format!("{at}: required where the protocol leaves it out"));
                    }
                    if !required && param.required {
                        out.push(format!("{at}: may be left out where the protocol needs it"));
                    }
                    self.compare(&p["schema"], &(param.shape)(), &at, Role::Input, 0, out);
                }
                None => out.push(format!("{at}: missing")),
            }
        }
        for p in &served {
            let place = p["in"].as_str().unwrap_or("");
            let name = p["name"].as_str().unwrap_or("");
            let known = match place {
                "path" => endpoint.path_params.iter().any(|q| q.name == name),
                "query" => endpoint.query.iter().any(|q| q.name == name),
                // The jadpo tool sends an `Idempotency-Key` with every
                // `POST`; any other header or cookie it doesn't send, so
                // only one that may be left out is allowed.
                "header" if name.eq_ignore_ascii_case("idempotency-key") => true,
                _ => !p["required"].as_bool().unwrap_or(false),
            };
            if !known {
                out.push(format!("{place}.{name}: not in the protocol"));
            }
        }
    }

    fn body(&self, endpoint: &Endpoint, op: &Value, out: &mut Vec<String>) {
        let served = match op.get("requestBody").map(|b| self.resolve(b)) {
            None => None,
            Some(Ok(body)) => Some(body),
            Some(Err(e)) => {
                out.push(format!("body: {e}"));
                return;
            }
        };
        match (endpoint.body, served) {
            (Some(name), Some(body)) => {
                let schema = &body["content"]["application/json"]["schema"];
                if schema.is_null() {
                    out.push("body: isn't JSON".into());
                } else {
                    self.compare(schema, &Shape::Named(name), "body", Role::Input, 0, out);
                }
            }
            (Some(_), None) => out.push("body: missing".into()),
            (None, Some(body)) if body["required"].as_bool().unwrap_or(false) => {
                out.push("body: not in the protocol".into());
            }
            (None, _) => {}
        }
    }

    fn answers(&self, endpoint: &Endpoint, op: &Value, out: &mut Vec<String>) {
        let responses = &op["responses"];
        let status = endpoint.status.to_string();
        match responses.get(&status).map(|r| self.resolve(r)) {
            None => out.push(format!("answers {status}, which the document doesn't list")),
            Some(Err(e)) => out.push(format!("answer: {e}")),
            Some(Ok(response)) => match endpoint.answer {
                Answer::Json(name) => {
                    let schema = &response["content"]["application/json"]["schema"];
                    if schema.is_null() {
                        out.push(format!(
                            "answers JSON with {status}, which the document doesn't say"
                        ));
                    } else {
                        self.compare(schema, &Shape::Named(name), "answer", Role::Answer, 0, out);
                    }
                }
                Answer::Tar => {
                    if response["content"].get("application/x-tar").is_none() {
                        out.push(format!(
                            "answers application/x-tar with {status}, which the document doesn't say"
                        ));
                    }
                }
            },
        }
        for failure in endpoint.failures {
            let status = failure.status.to_string();
            let mut codes = Vec::new();
            if let Some(Ok(response)) = responses.get(&status).map(|r| self.resolve(r)) {
                self.codes(
                    &response["content"]["application/json"]["schema"],
                    0,
                    &mut codes,
                );
            }
            if !codes.iter().any(|c| c == failure.code) {
                out.push(format!(
                    "doesn't say it fails with `{}` ({status}) in the failure envelope",
                    failure.code
                ));
            }
        }
    }

    /// The failure codes a response's schema can carry: each envelope's
    /// `error.code`, through `$ref`s and `oneOf` or `anyOf`.
    fn codes(&self, schema: &Value, depth: usize, out: &mut Vec<String>) {
        if depth > DEPTH {
            return;
        }
        let Ok(schema) = self.resolve(schema) else {
            return;
        };
        for key in ["oneOf", "anyOf"] {
            for option in schema[key].as_array().into_iter().flatten() {
                self.codes(option, depth + 1, out);
            }
        }
        let Ok(error) = self.resolve(&schema["properties"]["error"]) else {
            return;
        };
        let enveloped = ["code", "message", "request_id"]
            .iter()
            .all(|member| error["properties"].get(member).is_some());
        if !enveloped {
            return;
        }
        let Ok(code) = self.resolve(&error["properties"]["code"]) else {
            return;
        };
        if let Some(constant) = code["const"].as_str() {
            out.push(constant.to_string());
        }
        for value in code["enum"].as_array().into_iter().flatten() {
            if let Some(value) = value.as_str() {
                out.push(value.to_string());
            }
        }
    }

    /// The schema's non-null part if it may be `null`: a type list with
    /// `"null"`, or `anyOf` or `oneOf` with a `null` member.
    fn nullable(&self, schema: &Value) -> Option<Value> {
        if let Some(types) = schema["type"].as_array()
            && types.iter().any(|t| t == "null")
        {
            let rest: Vec<&Value> = types.iter().filter(|t| *t != "null").collect();
            let mut copy = schema.clone();
            copy["type"] = match rest.as_slice() {
                [one] => (*one).clone(),
                _ => Value::Array(rest.into_iter().cloned().collect()),
            };
            return Some(copy);
        }
        for key in ["anyOf", "oneOf"] {
            let Some(options) = schema[key].as_array() else {
                continue;
            };
            let is_null = |o: &Value| self.resolve(o).is_ok_and(|o| o["type"] == "null");
            if options.iter().any(is_null) {
                let rest: Vec<&Value> = options.iter().filter(|o| !is_null(o)).collect();
                if let [one] = rest.as_slice() {
                    return Some((*one).clone());
                }
                let mut copy = schema.clone();
                copy[key] = Value::Array(rest.into_iter().cloned().collect());
                return Some(copy);
            }
        }
        None
    }

    fn compare(
        &self,
        served: &Value,
        expected: &Shape,
        at: &str,
        role: Role,
        depth: usize,
        out: &mut Vec<String>,
    ) {
        if depth > DEPTH {
            out.push(format!("{at}: nested past {DEPTH} levels"));
            return;
        }
        let served = match self.resolve(served) {
            Ok(served) => served,
            Err(e) => {
                out.push(format!("{at}: {e}"));
                return;
            }
        };
        if let Shape::Named(name) = expected {
            match named(name) {
                Some(shape) => self.compare(served, &shape, at, role, depth + 1, out),
                None => out.push(format!("{at}: the protocol has no shape `{name}`")),
            }
            return;
        }
        let split = self.nullable(served);
        if let Shape::Nullable(inner) = expected {
            match split {
                Some(rest) => self.compare(&rest, inner, at, role, depth + 1, out),
                None => out.push(format!("{at}: never null where the protocol allows null")),
            }
            return;
        }
        if let Some(rest) = split {
            out.push(format!("{at}: may be null"));
            self.compare(&rest, expected, at, role, depth + 1, out);
            return;
        }
        let kind = served_kind(served);
        let wrong = |out: &mut Vec<String>, what: &str| {
            out.push(format!("{at}: {kind} where the protocol has {what}"))
        };
        match expected {
            Shape::Text(text) => {
                if kind != "string" {
                    wrong(out, "text");
                } else if *text == Text::DateTime && served["format"] != "date-time" {
                    out.push(format!("{at}: not a date-time"));
                }
            }
            Shape::Integer => {
                if kind != "integer" {
                    wrong(out, "an integer");
                }
            }
            Shape::Bool => {
                if kind != "boolean" {
                    wrong(out, "a boolean");
                }
            }
            Shape::Enum(values) => {
                let mut served_values: Vec<&str> = served["enum"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect();
                served_values.sort_unstable();
                let mut expected_values = values.to_vec();
                expected_values.sort_unstable();
                if kind != "string" || served_values != expected_values {
                    let listed = if served_values.is_empty() {
                        "any text".to_string()
                    } else {
                        served_values.join(", ")
                    };
                    out.push(format!(
                        "{at}: values {listed} where the protocol has {}",
                        values.join(", ")
                    ));
                }
            }
            Shape::List(inner) => {
                if kind != "array" {
                    wrong(out, "a list");
                } else {
                    self.compare(
                        &served["items"],
                        inner,
                        &format!("{at}[]"),
                        role,
                        depth + 1,
                        out,
                    );
                }
            }
            Shape::Object(fields) => {
                if kind != "object" {
                    wrong(out, "an object");
                    return;
                }
                let properties = served["properties"].as_object();
                let required: Vec<&str> = served["required"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect();
                for field in fields {
                    let here = format!("{at}.{}", field.name);
                    let Some(property) = properties.and_then(|p| p.get(field.name)) else {
                        out.push(format!("{here}: missing"));
                        continue;
                    };
                    let is_required = required.contains(&field.name);
                    let may_be_left_out = role == Role::Input && field.optional;
                    if !is_required && !may_be_left_out {
                        out.push(format!("{here}: may be left out"));
                    }
                    if is_required && may_be_left_out {
                        out.push(format!("{here}: required where the protocol leaves it out"));
                    }
                    self.compare(property, &field.shape, &here, role, depth + 1, out);
                }
                for name in properties.into_iter().flat_map(|p| p.keys()) {
                    if !fields.iter().any(|f| f.name == name) {
                        out.push(format!("{at}.{name}: not in the protocol"));
                    }
                }
            }
            Shape::Named(_) | Shape::Nullable(_) => unreachable!("handled above"),
        }
    }
}

/// What a resolved schema says it is: its `type`, or `a choice` for
/// `oneOf` or `anyOf`, or `untyped`.
fn served_kind(schema: &Value) -> String {
    match &schema["type"] {
        Value::String(kind) => kind.clone(),
        Value::Array(kinds) => kinds
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" or "),
        _ if schema.get("oneOf").is_some() || schema.get("anyOf").is_some() => "a choice".into(),
        _ if schema.get("properties").is_some() => "object".into(),
        _ => "untyped".into(),
    }
}
