//! One OpenAPI 3.1 document per API (area 7, "OpenAPI"), written by `jadpo
//! check` into `build/openapi/<api>.json`: every route with its
//! placeholders, input, output and success status, the failures it can
//! reach under their statuses, the 400 and 422 input responses, rate
//! limits' 429, the 413 a body past the limit gets, the 500 and the 504
//! every route can answer, and on every route but a `GET` the
//! `Idempotency-Key` header (required when the API says so) with the 409
//! and 422 a key's repeat can answer, all with the `///` descriptions. Its
//! title is the application's name and its version the API record's
//! version. Health (`/health/live`, `/health/ready`) is the runtime's own
//! and in no document.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, Project};
use jadpo_graph::json::Value;
use jadpo_graph::{Base, Detail, Key, Section};

use super::wire::{Wire, reference};
use super::{GroupModel, RouteModel, Segment, groups, routes_clean};
use crate::graph::{declaration_key, type_ref};
use crate::lookups::Lookup;

/// The responses every route shares, by status, with the code and message
/// each sends (area 7, "How failures reach the client").
const SHARED: [(&str, &str, &str, &str); 8] = [
    (
        "400",
        "InvalidRequest",
        "invalid_request",
        "The body is not JSON, the query string can't be read, or the `Idempotency-Key` header is missing where the API requires it or isn't 1 to 255 printable ASCII characters.",
    ),
    (
        "409",
        "IdempotencyKeyInFlight",
        "conflict",
        "A request with this `Idempotency-Key` is still running; retry once it has finished.",
    ),
    (
        "413",
        "RequestTooLarge",
        "request_too_large",
        "The body is over 1 MB.",
    ),
    (
        "422",
        "InvalidInput",
        "invalid_input",
        "A field is missing, unknown or breaks a rule; each problem names the field and the rule, never the value.",
    ),
    (
        "422",
        "IdempotencyKeyReused",
        "idempotency_key_reused",
        "This `Idempotency-Key` was already used with a different request.",
    ),
    (
        "429",
        "RateLimited",
        "rate_limited",
        "Too many requests from this caller; wait as `Retry-After` says.",
    ),
    (
        "500",
        "InternalFault",
        "internal_fault",
        "The server failed to answer; the `request_id` traces it.",
    ),
    (
        "504",
        "DeadlineExceeded",
        "deadline_exceeded",
        "The route's deadline passed; nothing the request did was kept.",
    ),
];

/// The OpenAPI document of `api`, titled `title` at `version`; `None`
/// while a route has a problem or the API has no routes.
pub fn document(
    db: &dyn Db,
    project: Project,
    api: &str,
    title: &str,
    version: u64,
) -> Option<Value> {
    if !routes_clean(db, project) {
        return None;
    }
    let graph = crate::graph::graph(db, project);
    let lookup = Lookup::new(db, project);
    let groups = groups(db, project);
    let routes: Vec<(&GroupModel, &RouteModel)> = groups
        .routes()
        .filter(|(group, _)| group.api == api)
        .collect();
    if routes.is_empty() {
        return None;
    }
    let wire = Wire { graph };
    let required = super::api_table(db, project)
        .get(api)
        .is_some_and(|api| api.idempotency == jadpo_std::Idempotency::Required);
    let mut refs: BTreeSet<Key> = BTreeSet::new();
    let mut failure_schemas: BTreeMap<String, Value> = BTreeMap::new();
    let mut paths: BTreeMap<String, Value> = BTreeMap::new();
    for (_, route) in &routes {
        let operation = operation(
            &wire,
            &lookup,
            route,
            required,
            &mut refs,
            &mut failure_schemas,
        );
        let entry = paths.entry(route.path()).or_insert_with(Value::object);
        entry.set(&route.method.word().to_ascii_lowercase(), operation);
    }

    // Every object and enum the routes name, until none is new.
    let mut schemas: BTreeMap<String, Value> = BTreeMap::new();
    let mut done: BTreeSet<Key> = BTreeSet::new();
    while let Some(key) = refs.iter().find(|key| !done.contains(*key)).cloned() {
        done.insert(key.clone());
        let name = wire.name(&key);
        let schema = type_schema(&wire, &key, &mut refs);
        schemas.insert(name, schema);
    }
    for (_, name, code, description) in SHARED {
        schemas.insert(
            name.to_string(),
            envelope(code, description, problems_details(code)),
        );
    }
    schemas.extend(failure_schemas);

    let mut responses = Value::object();
    for (_, name, _, description) in SHARED {
        responses.set(name, response(description, Some(reference(name))));
    }

    let mut info = Value::object();
    info.set("title", title);
    info.set("version", version.to_string());
    let mut components = Value::object();
    components.set("schemas", Value::Object(schemas));
    components.set("responses", responses);
    let mut out = Value::object();
    out.set("openapi", "3.1.0");
    out.set("info", info);
    out.set("paths", Value::Object(paths));
    out.set("components", components);
    Some(out)
}

fn response(description: &str, schema: Option<Value>) -> Value {
    let mut out = Value::object();
    out.set("description", description);
    if let Some(schema) = schema {
        let mut json = Value::object();
        json.set("schema", schema);
        let mut content = Value::object();
        content.set("application/json", json);
        out.set("content", content);
    }
    out
}

fn shared_response(name: &str) -> Value {
    let mut out = Value::object();
    out.set("$ref", format!("#/components/responses/{name}"));
    out
}

/// The `details` an input problem carries: each `{ field, rule }`.
fn problems_details(code: &str) -> Option<Value> {
    if code != "invalid_input" {
        return None;
    }
    let mut problem = Value::object();
    problem.set("type", "object");
    let mut properties = Value::object();
    for name in ["field", "rule"] {
        let mut text = Value::object();
        text.set("type", "string");
        properties.set(name, text);
    }
    problem.set("properties", properties);
    problem.set("required", vec![Value::from("field"), Value::from("rule")]);
    let mut list = Value::object();
    list.set("type", "array");
    list.set("items", problem);
    let mut details = Value::object();
    details.set("type", "object");
    let mut props = Value::object();
    props.set("problems", list);
    details.set("properties", props);
    details.set("required", vec![Value::from("problems")]);
    Some(details)
}

/// The failure envelope (area 4) for one code, with its `details`.
fn envelope(code: &str, description: &str, details: Option<Value>) -> Value {
    let text = |constant: Option<&str>| {
        let mut out = Value::object();
        out.set("type", "string");
        if let Some(constant) = constant {
            out.set("const", constant);
        }
        out
    };
    let mut error = Value::object();
    error.set("type", "object");
    let mut properties = Value::object();
    properties.set("code", text(Some(code)));
    properties.set("message", text(None));
    properties.set("request_id", text(None));
    let mut required = vec![
        Value::from("code"),
        Value::from("message"),
        Value::from("request_id"),
    ];
    if let Some(details) = details {
        properties.set("details", details);
        required.push(Value::from("details"));
    }
    error.set("properties", properties);
    error.set("required", required);
    let mut outer = Value::object();
    outer.set("type", "object");
    outer.set("description", description);
    let mut props = Value::object();
    props.set("error", error);
    outer.set("properties", props);
    outer.set("required", vec![Value::from("error")]);
    outer
}

fn operation(
    wire: &Wire<'_>,
    lookup: &Lookup<'_>,
    route: &RouteModel,
    key_required: bool,
    refs: &mut BTreeSet<Key>,
    failure_schemas: &mut BTreeMap<String, Value>,
) -> Value {
    let mut out = Value::object();
    let id: Vec<String> = std::iter::once(route.method.word().to_ascii_lowercase())
        .chain(route.segments.iter().map(|segment| match segment {
            Segment::Fixed(text) => text.replace('-', "_"),
            Segment::Param { name, .. } => format!("by_{name}"),
        }))
        .collect();
    out.set("operationId", id.join("_"));
    out.set_some("description", route.doc.as_deref());

    let mut parameters = Vec::new();
    for (name, ty) in route.params() {
        let mut parameter = Value::object();
        parameter.set("name", name);
        parameter.set("in", "path");
        parameter.set("required", true);
        parameter.set("schema", wire.schema(&type_ref(lookup, ty), refs));
        parameters.push(parameter);
    }
    let input_key = route
        .input
        .as_ref()
        .and_then(|input| declaration_key(lookup, input));
    if let Some(key) = &input_key {
        for field in wire.fields(key) {
            let Detail::Field {
                ty,
                optional,
                default,
                header,
                ..
            } = &field.detail
            else {
                continue;
            };
            let name = field.name.rsplit('.').next().unwrap_or(&field.name);
            let place = match (header, route.method.reads_query()) {
                (Some(_), _) => "header",
                (None, true) => "query",
                (None, false) => continue,
            };
            let mut parameter = Value::object();
            parameter.set("name", header.clone().unwrap_or_else(|| name.to_string()));
            parameter.set("in", place);
            parameter.set_some("required", !optional && default.is_none());
            parameter.set("schema", wire.schema(ty, refs));
            parameters.push(parameter);
        }
        if !route.method.reads_query() {
            let mut body = Value::object();
            body.set("required", true);
            let mut json = Value::object();
            json.set("schema", reference(&wire.name(key)));
            let mut content = Value::object();
            content.set("application/json", json);
            body.set("content", content);
            out.set("requestBody", body);
            refs.insert(key.clone());
        }
    }
    // Every write honours an `Idempotency-Key` (area 7, question 17).
    let writes = route.method != jadpo_std::HttpMethod::Get;
    if writes {
        let mut parameter = Value::object();
        parameter.set("name", "Idempotency-Key");
        parameter.set("in", "header");
        parameter.set(
            "description",
            "Makes a retry safe: a repeat with the same key and request gets the first response for 24 hours instead of running again.",
        );
        parameter.set_some("required", key_required);
        let mut schema = Value::object();
        schema.set("type", "string");
        schema.set("minLength", 1u32);
        schema.set("maxLength", jadpo_std::http::IDEMPOTENCY_KEY_MAX);
        parameter.set("schema", schema);
        parameters.push(parameter);
    }
    out.set_some("parameters", parameters);

    let mut responses = Value::object();
    let success = route.status.code().to_string();
    match &route.output {
        Some(output) => responses.set(
            &success,
            response(
                "Success.",
                Some(wire.schema(&type_ref(lookup, output), refs)),
            ),
        ),
        None => responses.set(&success, response("Success, with no content.", None)),
    }
    // Declared failures, under their statuses.
    let mut by_status: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for failure in &route.fails {
        let Some(key) = declaration_key(lookup, failure) else {
            continue;
        };
        let Some(Detail::Type {
            failure: Some(detail),
            ..
        }) = wire.node(&key).map(|node| &node.detail)
        else {
            continue;
        };
        let Some(status) = detail.status else {
            continue;
        };
        let name = wire.name(&key);
        let mut details = Value::object();
        details.set("type", "object");
        let mut properties = Value::object();
        let mut required = Vec::new();
        for field in wire.fields(&key) {
            if let Detail::Field {
                ty,
                section: Some(Section::Client),
                ..
            } = &field.detail
            {
                let field_name = field.name.rsplit('.').next().unwrap_or(&field.name);
                properties.set(field_name, wire.schema(ty, refs));
                required.push(Value::from(field_name));
            }
        }
        let details = (!required.is_empty()).then(|| {
            details.set("properties", properties);
            details.set("required", required);
            details
        });
        failure_schemas.insert(
            name.clone(),
            envelope(
                &detail.code,
                detail.message.as_deref().unwrap_or(&detail.code),
                details,
            ),
        );
        by_status
            .entry(status.to_string())
            .or_default()
            .push((detail.code.clone(), name));
    }
    let has_input = route.input.is_some() || route.params().next().is_some();
    if writes {
        by_status
            .entry("409".to_string())
            .or_default()
            .push(("conflict".to_string(), "IdempotencyKeyInFlight".to_string()));
        by_status.entry("422".to_string()).or_default().push((
            "idempotency_key_reused".to_string(),
            "IdempotencyKeyReused".to_string(),
        ));
    }
    for (status, mut failures) in by_status {
        failures.sort();
        let mut schemas: Vec<Value> = failures.iter().map(|(_, name)| reference(name)).collect();
        if status == "422" && has_input {
            schemas.insert(0, reference("InvalidInput"));
        }
        let codes: Vec<String> = failures
            .iter()
            .map(|(code, _)| format!("`{code}`"))
            .collect();
        let schema = if schemas.len() == 1 {
            schemas.remove(0)
        } else {
            let mut any = Value::object();
            any.set("oneOf", schemas);
            any
        };
        responses.set(
            &status,
            response(
                &format!("Fails with {}.", crate::and_list(&codes)),
                Some(schema),
            ),
        );
    }
    if route.input.is_some() || writes {
        responses.set("400", shared_response("InvalidRequest"));
    }
    if route.input.is_some() && !route.method.reads_query() {
        responses.set("413", shared_response("RequestTooLarge"));
    }
    if has_input && responses.get("422").is_none() {
        responses.set("422", shared_response("InvalidInput"));
    }
    responses.set("429", shared_response("RateLimited"));
    responses.set("500", shared_response("InternalFault"));
    responses.set("504", shared_response("DeadlineExceeded"));
    out.set("responses", responses);
    out
}

/// An object's or enum's schema.
fn type_schema(wire: &Wire<'_>, key: &Key, refs: &mut BTreeSet<Key>) -> Value {
    let Some(node) = wire.node(key) else {
        return Value::object();
    };
    let mut out = Value::object();
    out.set_some("description", node.doc.as_deref());
    match &node.detail {
        Detail::Type {
            base: Base::Enum, ..
        } => {
            let variants = wire.variants(key);
            if variants.iter().all(|(_, fields)| fields.is_empty()) {
                out.set("type", "string");
                out.set(
                    "enum",
                    variants
                        .into_iter()
                        .map(|(name, _)| Value::from(name))
                        .collect::<Vec<_>>(),
                );
            } else {
                let options: Vec<Value> = variants
                    .into_iter()
                    .map(|(name, fields)| {
                        let mut tag = Value::object();
                        tag.set("const", name.as_str());
                        let mut properties = Value::object();
                        properties.set("tag", tag);
                        let mut required = vec![Value::from("tag")];
                        for field in fields {
                            if let Detail::Field { ty, .. } = &field.detail {
                                let field_name =
                                    field.name.rsplit('.').next().unwrap_or(&field.name);
                                properties.set(field_name, wire.schema(ty, refs));
                                required.push(Value::from(field_name));
                            }
                        }
                        let mut option = Value::object();
                        option.set("type", "object");
                        option.set("properties", properties);
                        option.set("required", required);
                        option.set("additionalProperties", false);
                        option
                    })
                    .collect();
                out.set("oneOf", options);
            }
        }
        detail => {
            let input = matches!(
                detail,
                Detail::Type {
                    base: Base::Input,
                    ..
                }
            );
            out.set("type", "object");
            let mut properties = Value::object();
            let mut required = Vec::new();
            for field in wire.fields(key) {
                let Detail::Field {
                    ty,
                    optional,
                    default,
                    header,
                    ..
                } = &field.detail
                else {
                    continue;
                };
                if header.is_some() {
                    continue;
                }
                let field_name = field.name.rsplit('.').next().unwrap_or(&field.name);
                let mut schema = wire.schema(ty, refs);
                if let Some(default) = default {
                    schema.set(
                        "description",
                        format!("Defaults to {}.", default.render(&|key| wire.name(key))),
                    );
                }
                properties.set(field_name, schema);
                if !(input && (*optional || default.is_some())) {
                    required.push(Value::from(field_name));
                }
            }
            out.set("properties", properties);
            out.set_some("required", required);
            out.set("additionalProperties", false);
        }
    }
    out
}

/// Every API's document, by API name, each at the version its committed
/// record says (0 before it has one), titled `title`.
pub fn documents(db: &dyn Db, project: Project, title: &str) -> Vec<(String, Value)> {
    let versions: BTreeMap<String, u64> = project
        .records(db)
        .iter()
        .filter_map(|record| {
            let api = record.path.strip_prefix("api/")?.strip_suffix(".api")?;
            let version = super::record::version(record.text.as_ref().ok()?)?;
            Some((api.to_string(), version))
        })
        .collect();
    super::record::record_bodies(db, project)
        .keys()
        .filter_map(|api| {
            let version = versions.get(api).copied().unwrap_or(0);
            Some((api.clone(), document(db, project, api, title, version)?))
        })
        .collect()
}
