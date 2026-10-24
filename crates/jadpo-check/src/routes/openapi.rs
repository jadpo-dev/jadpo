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
//! title is the application's name and its version the first 12 hex
//! digits of the SHA-256 of the API's record (area 18), so it changes
//! exactly when the record does and is never stored. Health (`/health/live`, `/health/ready`) is the runtime's own
//! and in no document. The sign-in methods the API accepts are its
//! security schemes (area 8; area 7, question 12): a route anyone may call
//! says `security: []`, and every other answers 401, 403 and 503 for the
//! ways signing in fails. With a revocation delay and service keys, an API
//! that accepts them lists the runtime's `POST /auth/exchange`, marked
//! `x-jadpo-generated` (area 7, question 12), and each API that serves its
//! document lists `GET <prefix>/openapi.json` the same way
//! ([`super::served`], chunk 7.2).
//!
//! The `x-jadpo` fields carry what plain OpenAPI lacks (area 7, chunk
//! 7.2), so area 11's importer makes an exact package from the document:
//! `x-jadpo-changes` on every operation (a route that runs an action
//! changes something), `x-jadpo-idempotency` on every operation that
//! reads an `Idempotency-Key` (the header, whether the API requires it,
//! what a key keeps, for how long, and the codes a key's repeat answers
//! with), `x-jadpo-failure` on each failure the application declares (its
//! `code` and `kind`) and `x-jadpo-constraints` on every field,
//! placeholder and output: its type and rules as the API record writes
//! them. Each
//! route's operation is tagged with its group, and the group's `///`
//! description is the tag's.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, Project};
use jadpo_graph::json::Value;
use jadpo_graph::{Base, Detail, Key, Section};
use jadpo_std::http::PrincipalKind;

use super::wire::{Wire, field_type, reference};
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
    External,
}

/// What signing in can answer (area 8, "Failures"): each status, schema
/// name and code, its description, and when it answers.
const SIGN_IN: [(&str, &str, &str, &str, When); 10] = [
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
    (
        "403",
        "ExternalSignInRefused",
        "sign_in_refused",
        "No provisioned active application user has this verified provider subject.",
        When::External,
    ),
    (
        "500",
        "AuthenticationMisconfigured",
        "authentication_misconfigured",
        "The configured issuer, discovery metadata or signing keys are invalid.",
        When::External,
    ),
];

/// The OpenAPI document of `api`, titled `title` at `version`; `None`
/// while a route has a problem or the API has no routes.
pub fn document(
    db: &dyn Db,
    project: Project,
    api: &str,
    title: &str,
    version: &str,
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
    let identity = crate::identity::identity(db, project);
    // The passkey and magic-link routes belong to the first API that
    // accepts browser sessions (area 8).
    let here = super::served::sign_in_api(db, project).as_deref() == Some(api);
    let passkeys = identity.passkeys().filter(|_| here);
    let magic_links = identity.magic_link_routes().filter(|_| here);
    let webhooks = crate::webhooks::declared_in(db, project, api);
    if routes.is_empty() && passkeys.is_none() && magic_links.is_none() && webhooks.is_empty() {
        return None;
    }
    let wire = Wire {
        graph,
        locales: crate::locales::locales(db, project).plan.as_ref(),
    };
    let declared = super::api_table(db, project).get(api);
    let required = declared.is_some_and(|api| api.idempotency == jadpo_std::Idempotency::Required);
    // The methods it accepts: its own list, or every declared one.
    let methods = super::apis::accepted_methods(db, project, api);
    let mut refs: BTreeSet<Key> = BTreeSet::new();
    let mut failure_schemas: BTreeMap<String, Value> = BTreeMap::new();
    let mut paths: BTreeMap<String, Value> = BTreeMap::new();
    let mut tags: BTreeMap<String, Option<String>> = BTreeMap::new();
    for (group, route) in &routes {
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
        operation.set("tags", vec![Value::from(group.name.as_str())]);
        tags.entry(group.name.clone())
            .or_insert_with(|| group.doc.clone());
        let entry = paths.entry(route.path()).or_insert_with(Value::object);
        entry.set(&route.method.word().to_ascii_lowercase(), operation);
    }
    // Its own document (area 7, "OpenAPI").
    if let Some(served) = super::served::served_by(db, project, api) {
        let entry = paths.entry(served.path()).or_insert_with(Value::object);
        entry.set(
            "get",
            served_operation(&served, &methods, &mut failure_schemas),
        );
    }
    let exchange =
        identity.exchanges() && methods.contains(&jadpo_std::AuthenticationMethod::ServiceKey);
    if exchange {
        let entry = paths
            .entry(format!("/{}", jadpo_std::http::EXCHANGE_PATH.join("/")))
            .or_insert_with(Value::object);
        entry.set("post", exchange_operation(&methods, &mut failure_schemas));
    }
    for webhook in &webhooks {
        let entry = paths.entry(webhook.path()).or_insert_with(Value::object);
        entry.set("post", webhook_operation(webhook, &mut failure_schemas));
    }
    if let Some(passkey) = passkeys {
        for step in jadpo_std::http::PasskeyStep::ALL {
            let path: String = step
                .segments(&passkey.path)
                .iter()
                .map(|text| format!("/{text}"))
                .collect();
            let entry = paths.entry(path).or_insert_with(Value::object);
            entry.set(
                &step.method().word().to_ascii_lowercase(),
                passkey_operation(step, &methods, &mut failure_schemas),
            );
        }
    }
    if let Some(magic) = magic_links {
        let api_tokens = identity.built(jadpo_std::AuthenticationMethod::ApiToken);
        for step in jadpo_std::http::MagicLinkStep::ALL {
            if step == jadpo_std::http::MagicLinkStep::Redeem && !api_tokens {
                continue;
            }
            let path: String = step
                .segments(&magic.path)
                .iter()
                .map(|text| format!("/{text}"))
                .collect();
            let entry = paths.entry(path).or_insert_with(Value::object);
            entry.set(
                &step.method().word().to_ascii_lowercase(),
                magic_link_operation(step, &mut failure_schemas),
            );
        }
    }

    // Every object and enum the routes name, until none is new.
    let mut schemas: BTreeMap<String, Value> = BTreeMap::new();
    let mut done: BTreeSet<Key> = BTreeSet::new();
    while let Some(key) = refs.iter().find(|key| !done.contains(*key)).cloned() {
        done.insert(key.clone());
        let name = wire.name(&key);
        let schema = component_schema(&wire, &key, &mut refs);
        schemas.insert(name, schema);
    }
    if exchange {
        schemas.insert(
            jadpo_std::http::EXCHANGED_TOKEN.to_string(),
            exchanged_schema(),
        );
    }
    if passkeys.is_some() {
        schemas.insert(
            jadpo_std::http::PASSKEY_VIEW.to_string(),
            passkey_view_schema(),
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
    info.set("version", version);
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
            jadpo_std::AuthenticationMethod::ExternalToken => Some(
                "A signed JWT from the configured issuer, checked against its published keys and the configured audience. Its subject resolves an active application user on every request; provider profile claims grant no authority.",
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
            if *method == jadpo_std::AuthenticationMethod::ExternalToken {
                scheme.set("bearerFormat", "JWT");
            }
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
    let tags: Vec<Value> = tags
        .into_iter()
        .map(|(name, doc)| {
            let mut tag = Value::object();
            tag.set("name", name.as_str());
            tag.set_some("description", doc.as_deref());
            tag
        })
        .collect();
    out.set_some("tags", tags);
    Some(out)
}

/// The runtime's `GET <prefix>/openapi.json` (area 7, "OpenAPI"): this
/// document, under the API's own default authentication.
fn served_operation(
    served: &super::served::Served,
    methods: &[jadpo_std::AuthenticationMethod],
    failure_schemas: &mut BTreeMap<String, Value>,
) -> Value {
    use jadpo_std::AuthenticationMethod as M;
    let mut out = Value::object();
    out.set(
        "operationId",
        format!(
            "get_{}",
            served
                .segments
                .iter()
                .map(|segment| segment.replace(['-', '.'], "_"))
                .collect::<Vec<_>>()
                .join("_")
        ),
    );
    out.set(
        "description",
        "This document: the API's routes, what each takes and answers with, and how clients sign in.",
    );
    out.set("x-jadpo-generated", true);
    out.set("x-jadpo-changes", false);
    let mut responses = Value::object();
    let mut document = Value::object();
    document.set("type", "object");
    document.set("description", "An OpenAPI 3.1 document.");
    responses.set("200", response("This document.", Some(document)));
    if served.authentication.needs_caller() {
        let users = methods
            .iter()
            .any(|method| matches!(method, M::BrowserSession | M::ApiToken | M::ExternalToken));
        let services = methods.contains(&M::ServiceKey);
        let mut by_status: BTreeMap<&str, Vec<(&str, &str)>> = BTreeMap::new();
        for (status, name, code, description, when) in SIGN_IN {
            let answers = match when {
                When::Always => true,
                When::External => methods.contains(&M::ExternalToken),
                When::Users => users,
                When::Services => services,
                When::CookieWrite | When::OtherKind => false,
            };
            if !answers {
                continue;
            }
            failure_schemas.insert(name.to_string(), envelope(code, description, None));
            by_status.entry(status).or_default().push((code, name));
        }
        for (status, mut failures) in by_status {
            failures.sort();
            let mut schemas: Vec<Value> =
                failures.iter().map(|(_, name)| reference(name)).collect();
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
    } else {
        out.set("security", Vec::<Value>::new());
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

/// A connection's generated webhook (area 11, "Receiving from outside";
/// chunk 7.7): the provider's JSON body, checked by its signature and kept
/// for the app's subscribers, answered with no content.
fn webhook_operation(
    webhook: &crate::webhooks::WebhookModel,
    failure_schemas: &mut BTreeMap<String, Value>,
) -> Value {
    let mut out = Value::object();
    out.set(
        "operationId",
        format!(
            "post_{}",
            webhook
                .segments
                .iter()
                .map(|segment| segment.replace(['-', '.'], "_"))
                .collect::<Vec<_>>()
                .join("_")
        ),
    );
    out.set(
        "description",
        format!(
            "Receives `{}`'s webhooks from its provider: each checked by its signature, a repeat of an event already received answered alike, and each event its package declares kept for the app's subscribers.",
            webhook.package
        ),
    );
    out.set("x-jadpo-generated", true);
    out.set("x-jadpo-changes", true);
    out.set("security", Vec::<Value>::new());
    let mut object = Value::object();
    object.set("type", "object");
    let mut json = Value::object();
    json.set("schema", object);
    let mut content = Value::object();
    content.set("application/json", json);
    let mut body = Value::object();
    body.set("required", true);
    body.set("content", content);
    out.set("requestBody", body);
    let mut responses = Value::object();
    let mut received = Value::object();
    received.set("description", "Received.");
    responses.set("204", received);
    failure_schemas.insert(
        "SignatureMismatch".to_string(),
        envelope(
            "unauthenticated",
            "The webhook's signature doesn't match, or it was signed more than five minutes ago.",
            None,
        ),
    );
    responses.set("400", shared_response("InvalidRequest"));
    responses.set(
        "401",
        response(
            "Fails with `unauthenticated`.",
            Some(reference("SignatureMismatch")),
        ),
    );
    responses.set("413", shared_response("RequestTooLarge"));
    responses.set("422", shared_response("InvalidInput"));
    responses.set("429", shared_response("RateLimited"));
    responses.set("500", shared_response("InternalFault"));
    responses.set("503", shared_response("Unavailable"));
    out.set("responses", responses);
    out
}

/// `x-jadpo-idempotency` (area 7, chunk 7.2): how an operation that reads
/// an `Idempotency-Key` treats one, so a client, and area 11's importer,
/// knows whether repeating it under the same key is safe.
fn idempotency_extension(required: bool, keys: Keys) -> Value {
    let mut out = Value::object();
    out.set("header", "Idempotency-Key");
    out.set("required", required);
    out.set(
        "keeps",
        match keys {
            Keys::Kept => "response",
            Keys::Claimed => "request",
            Keys::Nothing | Keys::Derived => "nothing",
        },
    );
    out.set(
        "window",
        jadpo_std::Dur {
            millis: i128::from(jadpo_std::http::IDEMPOTENCY_KEEP_MS),
        }
        .to_string(),
    );
    out.set("max_length", jadpo_std::http::IDEMPOTENCY_KEY_MAX);
    out.set("in_use", "conflict");
    out.set("reused", "idempotency_key_reused");
    out
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
    // It keeps nothing: the key's read signed its token.
    out.set("x-jadpo-changes", false);
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
        .any(|method| matches!(method, M::BrowserSession | M::ApiToken | M::ExternalToken));
    let mut by_status: BTreeMap<&str, Vec<(&str, &str)>> = BTreeMap::new();
    for (status, name, code, description, when) in SIGN_IN {
        let answers = match when {
            When::Always | When::Services => true,
            When::OtherKind => users,
            When::CookieWrite => methods.contains(&M::BrowserSession),
            When::Users | When::External => false,
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

/// One of the runtime's passkey routes (area 8, "Passkeys"): WebAuthn's
/// own JSON in and out of the ceremonies, a passkey as its holder sees it
/// from the rest, with every failure it can answer.
fn passkey_operation(
    step: jadpo_std::http::PasskeyStep,
    methods: &[jadpo_std::AuthenticationMethod],
    failure_schemas: &mut BTreeMap<String, Value>,
) -> Value {
    use jadpo_std::http::PasskeyStep as S;
    let object = |description: &str| {
        let mut out = Value::object();
        out.set("type", "object");
        out.set("description", description);
        out
    };
    let mut out = Value::object();
    out.set(
        "operationId",
        format!(
            "{}_{}",
            step.method().word().to_ascii_lowercase(),
            step.label().replace([' ', '-'], "_")
        ),
    );
    out.set(
        "description",
        match step {
            S::SignInOptions => "Gives the options for `navigator.credentials.get()` and a cookie binding them to this browser; the challenge is used once and expires.",
            S::SignIn => "Checks the device's answer, as WebAuthn's `toJSON()` writes it, against the challenge this browser was given, and starts a browser session.",
            S::RegisterOptions => "Gives the options for `navigator.credentials.create()`, bound to this session; the challenge is used once and expires.",
            S::Register => "Checks the device's new passkey against the challenge this session was given, and keeps it with the person's name for it.",
            S::List => "The signed-in person's passkeys, oldest first.",
            S::Rename => "Renames one of the signed-in person's passkeys.",
            S::Remove => "Removes one of the signed-in person's passkeys.",
        },
    );
    out.set("x-jadpo-generated", true);
    out.set(
        "x-jadpo-changes",
        step.method() != jadpo_std::HttpMethod::Get,
    );
    if !step.authentication().needs_caller() {
        out.set("security", Vec::<Value>::new());
    }
    if matches!(step, S::Rename | S::Remove) {
        let mut parameter = Value::object();
        parameter.set("name", "passkey_id");
        parameter.set("in", "path");
        parameter.set("required", true);
        let mut schema = Value::object();
        schema.set("type", "string");
        schema.set("format", "uuid");
        parameter.set("schema", schema);
        out.set("parameters", vec![parameter]);
    }
    let body: Option<(Vec<&str>, Value)> = match step {
        S::SignIn => Some((vec!["credential"], Value::object())),
        S::Register => Some((vec!["name", "credential"], Value::object())),
        S::Rename => Some((vec!["name"], Value::object())),
        _ => None,
    };
    if let Some((fields, _)) = body {
        let mut properties = Value::object();
        for field in &fields {
            properties.set(
                field,
                if *field == "credential" {
                    object("The device's answer, as WebAuthn's `PublicKeyCredential.toJSON()` writes it.")
                } else {
                    let mut name = Value::object();
                    name.set("type", "string");
                    name.set("description", "The person's name for the passkey: one line, 1 to 200 characters.");
                    name
                },
            );
        }
        let mut schema = Value::object();
        schema.set("type", "object");
        schema.set("properties", properties);
        schema.set(
            "required",
            fields
                .iter()
                .map(|field| Value::from(*field))
                .collect::<Vec<_>>(),
        );
        schema.set("additionalProperties", false);
        let mut json = Value::object();
        json.set("schema", schema);
        let mut content = Value::object();
        content.set("application/json", json);
        let mut request = Value::object();
        request.set("required", true);
        request.set("content", content);
        out.set("requestBody", request);
    }
    let mut responses = Value::object();
    let status = step.status().code().to_string();
    let success = match step {
        S::SignInOptions => response(
            "WebAuthn's request options under `publicKey`, with `mediation`.",
            Some(object(
                "`{ publicKey, mediation }`, as `PublicKeyCredential.parseRequestOptionsFromJSON()` reads `publicKey`.",
            )),
        ),
        S::RegisterOptions => response(
            "WebAuthn's creation options under `publicKey`.",
            Some(object(
                "`{ publicKey }`, as `PublicKeyCredential.parseCreationOptionsFromJSON()` reads `publicKey`.",
            )),
        ),
        S::Register | S::Rename => response(
            "The passkey, as its holder sees it.",
            Some(reference(jadpo_std::http::PASSKEY_VIEW)),
        ),
        S::List => {
            let mut list = Value::object();
            list.set("type", "array");
            list.set("items", reference(jadpo_std::http::PASSKEY_VIEW));
            response("The person's passkeys, oldest first.", Some(list))
        }
        S::SignIn => response("Signed in: the response sets the session's cookies.", None),
        S::Remove => response("Removed.", None),
    };
    responses.set(&status, success);
    let mut failures: Vec<(&str, &str, &str, &str)> = Vec::new();
    match step {
        S::SignInOptions => {}
        S::SignIn => {
            failures.push((
                "401",
                "PasskeyChallengeExpired",
                "passkey_challenge_expired",
                "The challenge expired, was used, was replaced, or was given to another browser.",
            ));
            failures.push((
                "401",
                "PasskeyRejected",
                "passkey_rejected",
                "The answer doesn't check, or its passkey isn't registered.",
            ));
            failures.push((
                "403",
                "AccountDisabled",
                "user_disabled",
                "The signed-in user's record isn't active.",
            ));
        }
        S::RegisterOptions => {}
        S::Register => {
            failures.push((
                "401",
                "PasskeyChallengeExpired",
                "passkey_challenge_expired",
                "The challenge expired, was used, was replaced, or was given to another session.",
            ));
            failures.push((
                "401",
                "PasskeyRejected",
                "passkey_rejected",
                "The answer doesn't check, or its passkey isn't registered.",
            ));
            failures.push((
                "409",
                "PasskeyAlreadyRegistered",
                "passkey_already_registered",
                "The passkey is registered already.",
            ));
            failures.push((
                "409",
                "PasskeyLimitReached",
                "passkey_limit_reached",
                "The person has 20 passkeys already.",
            ));
        }
        S::List => {}
        S::Rename | S::Remove => {
            failures.push((
                "404",
                "PasskeyNotFound",
                "passkey_not_found",
                "No passkey of the signed-in person has this `id`.",
            ));
        }
    }
    if !step.authentication().needs_caller() {
        failures.push(("403", "CsrfRejected", "csrf_rejected", "A write that came with the session's cookie has no `Origin` naming this server, or no `X-Jadpo-CSRF` matching the session."));
    } else {
        use jadpo_std::AuthenticationMethod as M;
        let users = methods
            .iter()
            .any(|method| matches!(method, M::BrowserSession | M::ApiToken | M::ExternalToken));
        for (status, name, code, description, when) in SIGN_IN {
            let answers = match when {
                When::Always | When::Users => true,
                When::CookieWrite => step.method() != jadpo_std::HttpMethod::Get,
                When::OtherKind => methods.contains(&M::ServiceKey) && users,
                When::Services | When::External => false,
            };
            if answers {
                failures.push((status, name, code, description));
            }
        }
    }
    let mut by_status: BTreeMap<&str, Vec<(&str, &str)>> = BTreeMap::new();
    for (status, name, code, description) in failures {
        failure_schemas.insert(name.to_string(), envelope(code, description, None));
        by_status.entry(status).or_default().push((code, name));
    }
    for (status, mut failures) in by_status {
        failures.sort();
        failures.dedup();
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

/// One of the runtime's magic-link routes (area 8, "Magic links"): its
/// fixed body and answer, with every failure it can answer.
fn magic_link_operation(
    step: jadpo_std::http::MagicLinkStep,
    failure_schemas: &mut BTreeMap<String, Value>,
) -> Value {
    use jadpo_std::http::MagicLinkStep as S;
    let text = |description: &str| {
        let mut out = Value::object();
        out.set("type", "string");
        out.set("description", description);
        out
    };
    let mut out = Value::object();
    out.set(
        "operationId",
        format!(
            "{}_{}",
            step.method().word().to_ascii_lowercase(),
            step.label().replace([' ', '-'], "_")
        ),
    );
    out.set(
        "description",
        match step {
            S::Request => "Sends a link and a six-digit code to the address once the request commits, when an account has it or a first sign-in may make one. The answer is the same for every address.",
            S::Confirm => "Takes a link's token, or an address and its code, once, and starts a browser session; answers with the path the person asked to go to.",
            S::Redeem => "Takes an address and its code, once, and issues an API token for the application's own client.",
        },
    );
    out.set("x-jadpo-generated", true);
    out.set("x-jadpo-changes", true);
    out.set("security", Vec::<Value>::new());
    let mut properties = Value::object();
    let mut required: Vec<Value> = Vec::new();
    match step {
        S::Request => {
            let mut address = text("The address to send the link and the code to.");
            address.set("format", "email");
            properties.set("address", address);
            properties.set(
                "return_to",
                text("Where to go once signed in: a path on this site, starting with one `/`. Without it, `/`."),
            );
            properties.set(
                "locale",
                text("The email's language, when the application supports it; otherwise the request's `Accept-Language` decides."),
            );
            required.push(Value::from("address"));
        }
        S::Confirm => {
            properties.set("token", text("The link's token, 64 hexadecimal digits."));
            let mut address = text("The address the code was sent to, with `code`.");
            address.set("format", "email");
            properties.set("address", address);
            properties.set("code", text("The six-digit code, with `address`."));
        }
        S::Redeem => {
            let mut address = text("The address the code was sent to.");
            address.set("format", "email");
            properties.set("address", address);
            properties.set("code", text("The six-digit code."));
            required.push(Value::from("address"));
            required.push(Value::from("code"));
        }
    }
    let mut schema = Value::object();
    schema.set("type", "object");
    schema.set("properties", properties);
    if !required.is_empty() {
        schema.set("required", required);
    }
    schema.set("additionalProperties", false);
    let mut json = Value::object();
    json.set("schema", schema);
    let mut content = Value::object();
    content.set("application/json", json);
    let mut request = Value::object();
    request.set("required", true);
    request.set("content", content);
    out.set("requestBody", request);
    let mut responses = Value::object();
    let status = step.status().code().to_string();
    let answer = |field: &str, description: &str| {
        let mut properties = Value::object();
        properties.set(field, text(description));
        let mut out = Value::object();
        out.set("type", "object");
        out.set("properties", properties);
        out.set("required", vec![Value::from(field)]);
        out.set("additionalProperties", false);
        out
    };
    let success = match step {
        S::Request => response(
            "Accepted: an email is sent if an account has the address.",
            None,
        ),
        S::Confirm => response(
            "Signed in: the response sets the session's cookies.",
            Some(answer(
                "return_to",
                "The path the person asked to go to, or `/`.",
            )),
        ),
        S::Redeem => response(
            "The API token, shown once.",
            Some(answer(
                "token",
                "The API token: `jadpo_token_` and 64 hexadecimal digits.",
            )),
        ),
    };
    responses.set(&status, success);
    let mut failures: Vec<(&str, &str, &str, &str)> = Vec::new();
    match step {
        S::Request => {}
        S::Confirm | S::Redeem => {
            if step == S::Confirm {
                failures.push((
                    "401",
                    "SignInLinkExpired",
                    "sign_in_link_expired",
                    "The link expired, was used or replaced, or is unknown.",
                ));
                failures.push((
                    "401",
                    "SignInLinkOtherBrowser",
                    "sign_in_link_other_browser",
                    "The link was opened in a browser other than the one that asked for it.",
                ));
                failures.push((
                    "403",
                    "CsrfRejected",
                    "csrf_rejected",
                    "The request has no `Origin` naming this site.",
                ));
            }
            failures.push((
                "401",
                "SignInCodeInvalid",
                "sign_in_code_invalid",
                "The code is wrong, used, ended, expired or replaced, or the address has none.",
            ));
            failures.push((
                "429",
                "SignInCodesDisabled",
                "sign_in_codes_disabled",
                "The address sent too many wrong codes today; its links still work.",
            ));
            failures.push(("429", "RateLimited", "rate_limited", "Too many requests."));
            failures.push((
                "403",
                "SignInRefused",
                "sign_in_refused",
                "The application refused a first sign-in.",
            ));
            failures.push((
                "403",
                "AccountDisabled",
                "user_disabled",
                "The user's record isn't active.",
            ));
            failures.push((
                "503",
                "AuthenticationUnavailable",
                "authentication_unavailable",
                "The store sessions are kept in can't be reached.",
            ));
        }
    }
    let mut by_status: BTreeMap<&str, Vec<(&str, &str)>> = BTreeMap::new();
    for (status, name, code, description) in failures {
        failure_schemas.insert(name.to_string(), envelope(code, description, None));
        by_status.entry(status).or_default().push((code, name));
    }
    for (status, mut failures) in by_status {
        failures.sort();
        failures.dedup();
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
    if step == S::Request {
        responses.set("429", shared_response("RateLimited"));
    }
    responses.set("500", shared_response("InternalFault"));
    responses.set("504", shared_response("DeadlineExceeded"));
    out.set("responses", responses);
    out
}

/// The JSON Schema of a passkey as its holder sees it.
fn passkey_view_schema() -> Value {
    let mut out = Value::object();
    out.set(
        "description",
        "A passkey as its holder sees it: never its key or credential ID.",
    );
    out.set("type", "object");
    let mut properties = Value::object();
    let field = |ty: Value, format: Option<&str>| {
        let mut out = Value::object();
        out.set("type", ty);
        if let Some(format) = format {
            out.set("format", format);
        }
        out
    };
    properties.set("id", field(Value::from("string"), Some("uuid")));
    properties.set("name", field(Value::from("string"), None));
    properties.set(
        "registered_at",
        field(Value::from("string"), Some("date-time")),
    );
    properties.set(
        "last_used_at",
        field(
            Value::from(vec![Value::from("string"), Value::from("null")]),
            Some("date-time"),
        ),
    );
    out.set("properties", properties);
    out.set(
        "required",
        ["id", "name", "registered_at", "last_used_at"]
            .iter()
            .map(|name| Value::from(*name))
            .collect::<Vec<_>>(),
    );
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
    /// Nothing, for want of a store to keep it in, though the action
    /// makes changing calls: their keys are derived from it (chunk 7.9a).
    Derived,
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
    // Where the runtime keeps the key (chunk 7.9a): the store it writes,
    // or beside its events or its changing calls' record.
    let keys = if super::keys_store(db, project, &key).is_none() {
        let changes = crate::types::action_steps(db, project)
            .get(&key)
            .is_some_and(|steps| steps.changing > 0);
        if changes {
            Keys::Derived
        } else {
            Keys::Nothing
        }
    } else if route.output.as_ref().is_some_and(super::streams)
        || crate::types::unkept_responses(db, project).contains(name)
    {
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

/// A streaming route's `200` (area 7, "Streaming a model's answer"):
/// `text/event-stream`, described with its three events, and each event's
/// data's schema in `x-jadpo-stream`, which plain OpenAPI 3.1 has no field
/// for.
fn stream_response() -> Value {
    use jadpo_std::ai::events;
    let mut text = Value::object();
    text.set("type", "string");
    let mut piece = Value::object();
    piece.set("type", "object");
    let mut properties = Value::object();
    let mut member = Value::object();
    member.set("type", "string");
    properties.set("text", member);
    piece.set("properties", properties);
    piece.set("required", vec![Value::from("text")]);
    piece.set("additionalProperties", false);
    let mut done = Value::object();
    done.set("type", "object");
    done.set("additionalProperties", false);
    let mut error = Value::object();
    error.set(
        "anyOf",
        vec![reference("Unavailable"), reference("InternalFault")],
    );
    let mut each = Value::object();
    each.set(events::PIECE, piece);
    each.set(events::DONE, done);
    each.set(events::ERROR, error);
    let mut stream = Value::object();
    stream.set("events", each);
    let mut media = Value::object();
    media.set("schema", text);
    media.set("x-jadpo-stream", stream);
    let mut content = Value::object();
    content.set(events::CONTENT_TYPE, media);
    let mut out = Value::object();
    out.set(
        "description",
        "The model's answer as server-sent events, sent as it is written: `piece` events, each `{\"text\": ...}` with the next part of the answer, then `done` (`{}`) once it is whole, or `error`, the failure envelope, if it stopped before. Events carry no `id:`, and a stream can't be resumed.",
    );
    out.set("content", content);
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
        parameter.set("schema", constrained(wire, &type_ref(lookup, ty), refs));
        parameters.push(parameter);
    }
    let input_key = route
        .input
        .as_ref()
        .and_then(|input| declaration_key(lookup, input));
    if let Some(key) = &input_key {
        for field in wire.fields(key) {
            let Detail::Field {
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
            parameter.set("schema", constrained(wire, &field_type(field), refs));
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
                // A repeat runs `answer:` again rather than replaying a
                // body (area 7, "Waiting for follow-on work").
                Keys::Kept if route.settle.is_some() => "Makes a retry safe: for 24 hours a repeat with the same key and request doesn't run the action again, but answers with `answer:` run again, showing how far the work has got, and the first response's status.",
                Keys::Kept => "Makes a retry safe: a repeat with the same key and request gets the first response for 24 hours instead of running again.",
                // A stream is never kept (area 7, "Streaming a model's
                // answer").
                Keys::Claimed if route.output.as_ref().is_some_and(super::streams) => "Keeps a retry from running alongside the first request until its action has committed; a stream isn't kept, so a retry once it has started runs again.",
                Keys::Claimed => "Keeps a retry from running alongside the first request; the response isn't kept, since it starts or ends a session or carries a new credential, so a retry once the first has finished runs again.",
                Keys::Nothing => "Accepted, but nothing is kept: the route writes no store, so running it again changes nothing.",
                Keys::Derived => "Accepted, but nothing is kept, since the application names no store to keep it in (`events: { store: }`): the route's calls out go under keys derived from it, so a provider that keeps keys acts once for a repeat with the same key.",
            },
        );
        parameter.set_some("required", key_required);
        let mut schema = Value::object();
        schema.set("type", "string");
        schema.set("minLength", 1u32);
        schema.set("maxLength", jadpo_std::http::IDEMPOTENCY_KEY_MAX);
        parameter.set("schema", schema);
        parameters.push(parameter);
        out.set(
            "x-jadpo-idempotency",
            idempotency_extension(key_required, facts.keys),
        );
    }
    out.set("x-jadpo-changes", writes);
    out.set_some("parameters", parameters);

    let mut responses = Value::object();
    let success = route.status.code().to_string();
    match (&route.output, &route.settle) {
        // A route that waits for its work answers `answer:`'s result
        // either way, and 202 when the work hadn't settled in time (area
        // 7, "Waiting for follow-on work").
        (Some(output), Some(settle)) => {
            let schema = constrained(wire, &type_ref(lookup, output), refs);
            responses.set(
                &success,
                response(
                    "Success, once the work it set off has settled.",
                    Some(schema.clone()),
                ),
            );
            responses.set(
                "202",
                response(
                    &format!(
                        "Accepted: the work it set off hadn't settled within {}; the body is what it has done so far, and the work carries on.",
                        jadpo_std::Dur {
                            millis: i128::from(settle.within_ms)
                        }
                    ),
                    Some(schema),
                ),
            );
        }
        // A model's answer, as server-sent events (area 7, "Streaming a
        // model's answer"; chunk 5.5v6).
        (Some(output), None) if super::streams(output) => {
            responses.set(&success, stream_response());
        }
        (Some(output), None) => responses.set(
            &success,
            response(
                "Success.",
                Some(constrained(wire, &type_ref(lookup, output), refs)),
            ),
        ),
        (None, _) => responses.set(&success, response("Success, with no content.", None)),
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
                section: Some(Section::Client),
                ..
            } = &field.detail
            {
                let field_name = field.name.rsplit('.').next().unwrap_or(&field.name);
                properties.set(field_name, wire.schema(&field_type(field), refs));
                required.push(Value::from(field_name));
            }
        }
        let details = (!required.is_empty()).then(|| {
            details.set("properties", properties);
            details.set("required", required);
            details
        });
        let mut schema = envelope(
            &detail.code,
            detail.message.as_deref().unwrap_or(&detail.code),
            details,
        );
        let mut marked = Value::object();
        marked.set("code", detail.code.as_str());
        marked.set_some("kind", detail.kind.as_deref());
        schema.set("x-jadpo-failure", marked);
        failure_schemas.insert(name.clone(), schema);
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
            .any(|method| matches!(method, M::BrowserSession | M::ApiToken | M::ExternalToken));
        let services = methods.contains(&M::ServiceKey);
        for (status, name, code, description, when) in SIGN_IN {
            let answers = match when {
                When::Always => true,
                When::CookieWrite => writes && methods.contains(&M::BrowserSession),
                When::External => methods.contains(&M::ExternalToken),
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

/// A field's or placeholder's schema with `x-jadpo-constraints`: its type
/// and rules as the API record writes them (`Text { min_length: 1,
/// max_length: 200 }`, `List<TodoView>`, `Money?`), which say exactly
/// what JSON Schema can only approximate (area 7, chunk 7.2).
fn constrained(wire: &Wire<'_>, ty: &jadpo_graph::TypeRef, refs: &mut BTreeSet<Key>) -> Value {
    let mut schema = wire.schema(ty, refs);
    schema.set("x-jadpo-constraints", wire.render(ty, refs));
    schema
}

/// The schema of the object or enum `key` names, as its component in a
/// document: what `ai.generate` asks a model to answer in too (area 21,
/// "Typed output"; chunk 5.5v3).
pub fn component_schema(wire: &Wire<'_>, key: &Key, refs: &mut BTreeSet<Key>) -> Value {
    match super::wire::issued_of(key) {
        Some(issued) => super::wire::issued_schema(issued),
        None => type_schema(wire, key, refs),
    }
}

/// An object's or enum's schema.
fn type_schema(wire: &Wire<'_>, key: &Key, refs: &mut BTreeSet<Key>) -> Value {
    let Some(node) = wire.node(key) else {
        return Value::object();
    };
    let mut out = Value::object();
    // OpenAPI can't say most rules across an object's fields, so the
    // description lists them as written (area 2, "Rules across fields").
    let mut rules: Vec<(u32, String)> = wire
        .graph
        .find(key)
        .into_iter()
        .flat_map(|id| wire.graph.children(id))
        .filter_map(|child| {
            let child = wire.graph.node(child);
            match &child.detail {
                Detail::Rule { order, condition } => Some((
                    *order,
                    format!(
                        "`{}`: `{condition}`",
                        child.name.rsplit('.').next().unwrap_or(&child.name)
                    ),
                )),
                _ => None,
            }
        })
        .collect();
    rules.sort();
    let rules: Vec<String> = rules.into_iter().map(|(_, rule)| rule).collect();
    let description = match (node.doc.as_deref(), rules.is_empty()) {
        (doc, true) => doc.map(str::to_string),
        (None, false) => Some(format!("Rules across its fields: {}.", rules.join("; "))),
        (Some(doc), false) => Some(format!(
            "{doc}\n\nRules across its fields: {}.",
            rules.join("; ")
        )),
    };
    out.set_some("description", description.as_deref());
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
                            if let Detail::Field { .. } = &field.detail {
                                let field_name =
                                    field.name.rsplit('.').next().unwrap_or(&field.name);
                                properties
                                    .set(field_name, constrained(wire, &field_type(field), refs));
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
                let mut schema = constrained(wire, &field_type(field), refs);
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

/// What titles the documents: the application's name, its folder's until
/// `app.jadpo` gives one (area 7, "OpenAPI").
pub fn title(db: &dyn Db, project: Project) -> String {
    let name = project.name(db);
    if name.is_empty() {
        "Jadpo application".to_string()
    } else {
        name.clone()
    }
}

/// Every API's document, by API name, each at the version its record
/// gives, titled `title`.
pub fn documents(db: &dyn Db, project: Project, title: &str) -> Vec<(String, Value)> {
    super::record::record_texts(db, project)
        .iter()
        .filter_map(|(api, text)| {
            let version = super::record::openapi_version(text);
            Some((api.clone(), document(db, project, api, title, &version)?))
        })
        .collect()
}
