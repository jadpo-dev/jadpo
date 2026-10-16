//! One OpenAPI 3.1 document per API (area 7, "OpenAPI"), written by `jadpo
//! check` into `build/openapi/<api>.json`: every route with its
//! placeholders, input, output and success status, the failures it can
//! reach under their statuses, the 400, 413, 429, 500 and 504 every route
//! can answer, the 422 of a route that takes input, the 503 of one that
//! uses a store, and on every route but a `GET` the `Idempotency-Key`
//! header (required when the API says so), described by what the route
//! keeps, with the 409 and 422 a key's repeat can answer where a key is
//! claimed, all with the `///` descriptions. Each route's responses come
//! from what it runs (chunk 4.9): what it can be refused, whether it uses
//! a store, and whether its response is kept for a repeat. Its
//! title is the application's name and its version the API record's
//! version. Health (`/health/live`, `/health/ready`) is the runtime's own
//! and in no document. The sign-in methods the API accepts are its
//! security schemes (area 8; area 7, question 12): a route anyone may call
//! says `security: []`, and every other answers 401, 403 and 503 for the
//! ways signing in fails. With a revocation delay and service keys, an API
//! that accepts them lists the runtime's `POST /auth/exchange`, marked
//! `x-jadpo-generated` (area 7, question 12).

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, Project};
use jadpo_graph::json::Value;
use jadpo_graph::{Base, Detail, Key, Section};
use jadpo_std::http::PrincipalKind;

use super::wire::{Wire, reference};
use super::{GroupModel, RouteModel, Segment, groups, routes_clean};
use crate::graph::{declaration_key, type_ref};
use crate::lookups::Lookup;

/// The responses every route shares, by status, with the code and message
/// each sends (area 7, "How failures reach the client").
const SHARED: [(&str, &str, &str, &str); 9] = [
    (
        "400",
        "InvalidRequest",
        "invalid_request",
        "The request can't be read: its path or query string isn't valid UTF-8, a header's value isn't printable ASCII, the body isn't JSON or isn't sent as `application/json`, or the `Idempotency-Key` header is missing where the API requires it or isn't 1 to 255 printable ASCII characters.",
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
        "503",
        "Unavailable",
        "unavailable",
        "A store the route needs can't be reached, or kept changing under it; nothing was kept, so a retry runs again.",
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

/// When a refusal from signing in can answer a route.
#[derive(Clone, Copy, PartialEq, Eq)]
enum When {
    /// Every route that needs a caller.
    Always,
    /// A write, where the API accepts browser sessions.
    CookieWrite,
    /// Where the API accepts a method that signs users in.
    Users,
    /// Where the API accepts service keys.
    Services,
    /// A route that selects one kind of caller, where the API accepts a
    /// method that signs in the other.
    OtherKind,
}

/// What signing in can answer (area 8, "Failures"): each status, schema
/// name and code, its description, and when it answers.
const SIGN_IN: [(&str, &str, &str, &str, When); 8] = [
    (
        "401",
        "AuthenticationRequired",
        "authentication_required",
        "The request carries no credential.",
        When::Always,
    ),
    (
        "401",
        "InvalidCredentials",
        "invalid_credentials",
        "The credential is malformed, expired or unknown; a browser session's cookies are cleared.",
        When::Always,
    ),
    (
        "401",
        "AmbiguousCredentials",
        "ambiguous_credentials",
        "The request carries more than one credential, such as a cookie and a bearer token.",
        When::Always,
    ),
    (
        "403",
        "AccountDisabled",
        "user_disabled",
        "The signed-in user's record isn't active.",
        When::Users,
    ),
    (
        "403",
        "ServiceDisabled",
        "service_disabled",
        "The signed-in service's record isn't active.",
        When::Services,
    ),
    (
        "403",
        "CsrfRejected",
        "csrf_rejected",
        "A write that came with the session's cookie has no `Origin` naming this server, or no `X-Jadpo-CSRF` matching the session.",
        When::CookieWrite,
    ),
    (
        "403",
        "WrongKindOfCaller",
        "not_permitted",
        "A service called a route for users, or a user one for services.",
        When::OtherKind,
    ),
    (
        "503",
        "AuthenticationUnavailable",
        "authentication_unavailable",
        "The store sessions are kept in can't be reached.",
        When::Always,
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
    let wire = Wire {
        graph,
        locales: crate::locales::locales(db, project).plan.as_ref(),
    };
    let declared = super::api_table(db, project).get(api);
    let required = declared.is_some_and(|api| api.idempotency == jadpo_std::Idempotency::Required);
    let identity = crate::identity::identity(db, project);
    // The methods it accepts: its own list, or every declared one.
    let methods = super::apis::accepted_methods(db, project, api);
    let mut refs: BTreeSet<Key> = BTreeSet::new();
    let mut failure_schemas: BTreeMap<String, Value> = BTreeMap::new();
    let mut paths: BTreeMap<String, Value> = BTreeMap::new();
    for (_, route) in &routes {
        let facts = facts(db, project, &lookup, route);
        let mut operation = operation(
            &wire,
            &lookup,
            route,
            required,
            &facts,
            &methods,
            &mut refs,
            &mut failure_schemas,
        );
        if !route.authentication.needs_caller() {
            operation.set("security", Vec::<Value>::new());
        }
        let entry = paths.entry(route.path()).or_insert_with(Value::object);
        entry.set(&route.method.word().to_ascii_lowercase(), operation);
    }
    let exchange =
        identity.exchanges() && methods.contains(&jadpo_std::AuthenticationMethod::ServiceKey);
    if exchange {
        let entry = paths
            .entry(format!("/{}", jadpo_std::http::EXCHANGE_PATH.join("/")))
            .or_insert_with(Value::object);
        entry.set("post", exchange_operation(&methods, &mut failure_schemas));
    }

    // Every object and enum the routes name, until none is new.
    let mut schemas: BTreeMap<String, Value> = BTreeMap::new();
    let mut done: BTreeSet<Key> = BTreeSet::new();
    while let Some(key) = refs.iter().find(|key| !done.contains(*key)).cloned() {
        done.insert(key.clone());
        let name = wire.name(&key);
        let schema = match super::wire::issued_of(&key) {
            Some(issued) => super::wire::issued_schema(issued),
            None => type_schema(&wire, &key, &mut refs),
        };
        schemas.insert(name, schema);
    }
    if exchange {
        schemas.insert(
            jadpo_std::http::EXCHANGED_TOKEN.to_string(),
            exchanged_schema(),
        );
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
    let mut security = Vec::new();
    let mut schemes = Value::object();
    for method in &methods {
        let bearer = match method {
            jadpo_std::AuthenticationMethod::ApiToken => Some(
                "An API token, `jadpo_token_` and 64 hexadecimal digits, which a route whose action calls `sessions.issue_token` gives once.",
            ),
            jadpo_std::AuthenticationMethod::ServiceKey => Some(
                "A service key, `jadpo_key_` and 64 hexadecimal digits, which a route whose action calls `issue_key` gives once.",
            ),
            _ => None,
        };
        if let Some(description) = bearer.filter(|_| identity.built(*method)) {
            let mut scheme = Value::object();
            scheme.set("type", "http");
            scheme.set("scheme", "bearer");
            scheme.set("description", description);
            schemes.set(method.name(), scheme);
            let mut requirement = Value::object();
            requirement.set(method.name(), Vec::<Value>::new());
            security.push(requirement);
        }
        if *method == jadpo_std::AuthenticationMethod::BrowserSession
            && let Some(cookie) = identity.cookie()
        {
            let configured = identity
                .method(jadpo_std::AuthenticationMethod::BrowserSession)
                .is_some_and(|method| method.origin.is_some());
            let mut scheme = Value::object();
            scheme.set("type", "apiKey");
            scheme.set("in", "cookie");
            scheme.set("name", cookie);
            scheme.set(
                "description",
                if configured {
                    "A browser session, which a route whose action calls `sessions.start` begins. A write also sends `X-Jadpo-CSRF` with the token from the `_csrf` cookie, and an `Origin` that is the application's configured origin."
                } else {
                    "A browser session, which a route whose action calls `sessions.start` begins. A write also sends `X-Jadpo-CSRF` with the token from the `_csrf` cookie, and an `Origin` naming this server."
                },
            );
            schemes.set(method.name(), scheme);
            let mut requirement = Value::object();
            requirement.set(method.name(), Vec::<Value>::new());
            security.push(requirement);
        }
    }
    if !security.is_empty() {
        components.set("securitySchemes", schemes);
    }
    let mut out = Value::object();
    out.set("openapi", "3.1.0");
    out.set("info", info);
    out.set_some("security", security);
    out.set("paths", Value::Object(paths));
    out.set("components", components);
    Some(out)
}

/// The runtime's `POST /auth/exchange` (area 8, "Ways to sign in"): a
/// service key in, a short-lived token out, with what signing in with the
/// key can answer.
fn exchange_operation(
    methods: &[jadpo_std::AuthenticationMethod],
    failure_schemas: &mut BTreeMap<String, Value>,
) -> Value {
    use jadpo_std::AuthenticationMethod as M;
    let mut out = Value::object();
    out.set("operationId", "post_auth_exchange");
    out.set(
        "description",
        "Trades the service key the request signs in with for a token that signs the service in without a read until it expires, within the application's revocation delay; once it has, exchange the key again.",
    );
    out.set("x-jadpo-generated", true);
    let mut requirement = Value::object();
    requirement.set(M::ServiceKey.name(), Vec::<Value>::new());
    out.set("security", vec![requirement]);
    let mut responses = Value::object();
    let mut json = Value::object();
    json.set("schema", reference(jadpo_std::http::EXCHANGED_TOKEN));
    let mut content = Value::object();
    content.set("application/json", json);
    let mut success = Value::object();
    success.set("description", "The token, and when it expires.");
    success.set("content", content);
    responses.set("200", success);
    let users = methods
        .iter()
        .any(|method| matches!(method, M::BrowserSession | M::ApiToken));
    let mut by_status: BTreeMap<&str, Vec<(&str, &str)>> = BTreeMap::new();
    for (status, name, code, description, when) in SIGN_IN {
        let answers = match when {
            When::Always | When::Services => true,
            When::OtherKind => users,
            When::CookieWrite => methods.contains(&M::BrowserSession),
            When::Users => false,
        };
        if !answers {
            continue;
        }
        failure_schemas.insert(name.to_string(), envelope(code, description, None));
        by_status.entry(status).or_default().push((code, name));
    }
    for (status, mut failures) in by_status {
        failures.sort();
        let mut schemas: Vec<Value> = failures.iter().map(|(_, name)| reference(name)).collect();
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
            status,
            response(
                &format!("Fails with {}.", crate::and_list(&codes)),
                Some(schema),
            ),
        );
    }
    responses.set("400", shared_response("InvalidRequest"));
    responses.set("413", shared_response("RequestTooLarge"));
    responses.set("422", shared_response("InvalidInput"));
    responses.set("429", shared_response("RateLimited"));
    responses.set("500", shared_response("InternalFault"));
    responses.set("504", shared_response("DeadlineExceeded"));
    out.set("responses", responses);
    out
}

/// The JSON Schema of what the exchange answers with.
fn exchanged_schema() -> Value {
    let mut out = Value::object();
    out.set(
        "description",
        "A short-lived token for a service, sent as `Authorization: Bearer`, and when it expires.",
    );
    out.set("type", "object");
    let mut properties = Value::object();
    let mut token = Value::object();
    token.set("type", "string");
    token.set(
        "description",
        format!(
            "`{}` and a signed part.",
            jadpo_std::http::SERVICE_TOKEN_PREFIX
        ),
    );
    properties.set("token", token);
    let mut expires = Value::object();
    expires.set("type", "string");
    expires.set("format", "date-time");
    properties.set("expires_at", expires);
    out.set("properties", properties);
    out.set(
        "required",
        vec![Value::from("token"), Value::from("expires_at")],
    );
    out.set("additionalProperties", false);
    out
}

/// What a route's responses follow from in what it runs.
struct Facts {
    /// Policy may refuse its caller with 403 `not_permitted`.
    refuses: bool,
    /// It uses a store, so it can answer 503 `unavailable`.
    store: bool,
    /// What a write keeps for an `Idempotency-Key`.
    keys: Keys,
}

/// What a write keeps for an `Idempotency-Key` (area 7, "Idempotency
/// keys").
#[derive(Clone, Copy, PartialEq, Eq)]
enum Keys {
    /// The key and the response, so a repeat gets it.
    Kept,
    /// The key while the request runs, but not its response, which starts
    /// or ends a session or carries a new credential.
    Claimed,
    /// Nothing: the action writes no store, so running it again changes
    /// nothing.
    Nothing,
}

fn facts(db: &dyn Db, project: Project, lookup: &Lookup<'_>, route: &RouteModel) -> Facts {
    if route.run.query || route.run.generated.is_some() {
        return Facts {
            refuses: false,
            store: true,
            keys: Keys::Nothing,
        };
    }
    let Some(key) = crate::graph::declaration_key(lookup, &route.run.name) else {
        return Facts {
            refuses: false,
            store: false,
            keys: Keys::Nothing,
        };
    };
    let name = key.name();
    let writes = crate::types::written_stores(db, project)
        .get(name)
        .is_some_and(|stores| !stores.is_empty());
    let keys = if !writes {
        Keys::Nothing
    } else if crate::types::unkept_responses(db, project).contains(name) {
        Keys::Claimed
    } else {
        Keys::Kept
    };
    Facts {
        refuses: crate::policy::reach::may_refuse(db, project, name),
        store: crate::types::used_stores(db, project)
            .get(name)
            .is_some_and(|stores| !stores.is_empty()),
        keys,
    }
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

#[allow(clippy::too_many_arguments)]
fn operation(
    wire: &Wire<'_>,
    lookup: &Lookup<'_>,
    route: &RouteModel,
    key_required: bool,
    facts: &Facts,
    methods: &[jadpo_std::AuthenticationMethod],
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
            match facts.keys {
                Keys::Kept => "Makes a retry safe: a repeat with the same key and request gets the first response for 24 hours instead of running again.",
                Keys::Claimed => "Keeps a retry from running alongside the first request; the response isn't kept, since it starts or ends a session or carries a new credential, so a retry once the first has finished runs again.",
                Keys::Nothing => "Accepted, but nothing is kept: the route writes no store, so running it again changes nothing.",
            },
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
    // Signing in (area 8).
    if route.authentication.needs_caller() {
        use jadpo_std::AuthenticationMethod as M;
        let users = methods
            .iter()
            .any(|method| matches!(method, M::BrowserSession | M::ApiToken));
        let services = methods.contains(&M::ServiceKey);
        for (status, name, code, description, when) in SIGN_IN {
            let answers = match when {
                When::Always => true,
                When::CookieWrite => writes && methods.contains(&M::BrowserSession),
                When::Users => users,
                When::Services => services,
                When::OtherKind => match route.caller {
                    Some(PrincipalKind::User) => services,
                    Some(PrincipalKind::Service) => users,
                    None => false,
                },
            };
            if !answers {
                continue;
            }
            failure_schemas.insert(name.to_string(), envelope(code, description, None));
            by_status
                .entry(status.to_string())
                .or_default()
                .push((code.to_string(), name.to_string()));
        }
    }
    // Policy (area 9): a create, a change to a record the action loaded,
    // or an action listed under `actions:` can be refused.
    if facts.refuses {
        failure_schemas.insert(
            "PolicyRefused".to_string(),
            envelope(
                "not_permitted",
                "Policy doesn't let the caller do this.",
                None,
            ),
        );
        by_status
            .entry("403".to_string())
            .or_default()
            .push(("not_permitted".to_string(), "PolicyRefused".to_string()));
    }
    // A store that can't answer, or a change found as the action commits
    // more times than the budget allows.
    if facts.store {
        by_status
            .entry("503".to_string())
            .or_default()
            .push(("unavailable".to_string(), "Unavailable".to_string()));
    }
    if writes && facts.keys != Keys::Nothing {
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
        if status == "422" {
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
    // Any request can be unreadable or too large, and a query string or
    // body a route doesn't read is unknown input.
    responses.set("400", shared_response("InvalidRequest"));
    responses.set("413", shared_response("RequestTooLarge"));
    if responses.get("422").is_none() {
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
                // A variant without fields is sent as its name, one with
                // fields as an object tagged with it.
                let options: Vec<Value> = variants
                    .into_iter()
                    .map(|(name, fields)| {
                        if fields.is_empty() {
                            let mut plain = Value::object();
                            plain.set("type", "string");
                            plain.set("const", name.as_str());
                            return plain;
                        }
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
