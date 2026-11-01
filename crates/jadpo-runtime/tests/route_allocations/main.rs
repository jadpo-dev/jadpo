//! Routing a request allocates nothing (chunk 8.1k): finding its path and
//! method in the route trie, its route's label, and a declared failure's
//! client fields. A binary of its own with no test harness, so the
//! counting allocator sees only this thread's work.

use std::alloc::System;

use jadpo_runtime::http::{Found, Routes};
use jadpo_std::HttpMethod;
use jadpo_std::http::{
    Authentication, CodecId, FailureCodec, MagicLinkStep, PasskeyStep, RoutePlan, RouteTable, Run,
    Segment,
};
use jadpo_std::provider::{SignatureScheme, WebhookPlan};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static COUNTED: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

fn route(method: HttpMethod, path: &str, run: Run) -> RoutePlan {
    let segments = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| match segment.strip_prefix('{') {
            Some(name) => Segment::Param {
                name: name.trim_end_matches('}').to_string(),
                codec: CodecId(0),
            },
            None => Segment::Fixed(segment.to_string()),
        })
        .collect();
    RoutePlan {
        key: format!("route {} {path}", method.word()),
        method,
        segments,
        api: 0,
        input: None,
        output: None,
        status: jadpo_std::SuccessStatus::Ok,
        deadline_ms: 10_000,
        run,
        settle: None,
        stream: false,
        keys: None,
        args: Vec::new(),
        authentication: Authentication::Public,
        caller: None,
        confirm: false,
        file: "routes.jadpo".into(),
        line: 1,
    }
}

fn table() -> RouteTable {
    use HttpMethod::*;
    RouteTable {
        routes: vec![
            route(Get, "/todos", Run::Action(0)),
            route(Post, "/todos", Run::Action(1)),
            route(Get, "/todos/{todo_id}", Run::Action(2)),
            route(Delete, "/todos/{todo_id}", Run::Action(3)),
            route(Get, "/todos/archived", Run::Action(4)),
            route(Get, "/v1/openapi.json", Run::OpenApi("{}".into())),
            route(
                Post,
                "/auth/passkeys/sign-in",
                Run::Passkey(PasskeyStep::SignIn),
            ),
            route(
                Patch,
                "/auth/passkeys/{passkey_id}",
                Run::Passkey(PasskeyStep::Rename),
            ),
            route(
                Post,
                "/auth/magic-link",
                Run::MagicLink(MagicLinkStep::Request),
            ),
            route(Get, "/", Run::Action(5)),
        ],
        failures: vec![
            FailureCodec {
                key: "type QuantityUnavailable".into(),
                client: Vec::new(),
            },
            FailureCodec {
                key: "type NotFound".into(),
                client: Vec::new(),
            },
        ],
        ..RouteTable::default()
    }
}

fn webhook() -> WebhookPlan {
    WebhookPlan {
        connection: "payments".into(),
        segments: vec!["webhooks".into(), "payments".into()],
        scheme: SignatureScheme::Stripe,
        handshake: None,
        secret: 0,
        event_id: "id".into(),
        event_type: "type".into(),
        occurred_at: None,
        store: "main".into(),
        events: Vec::new(),
    }
}

/// Each request, and the label of the route it should find, if one.
const REQUESTS: [(&str, &str, Option<&str>); 14] = [
    ("GET", "/todos", Some("GET /todos")),
    ("POST", "/todos/", Some("POST /todos")),
    ("HEAD", "/todos/7f0c", Some("GET /todos/{todo_id}")),
    ("GET", "/todos/archived", Some("GET /todos/archived")),
    ("GET", "/todos/arch%69ved", Some("GET /todos/archived")),
    (
        "DELETE",
        "/todos/a%20b%E2%9C%93",
        Some("DELETE /todos/{todo_id}"),
    ),
    ("GET", "/v1/openapi.json", Some("GET /v1/openapi.json")),
    (
        "PATCH",
        "/auth/passkeys/5d2e",
        Some("PATCH /auth/passkeys/{passkey_id}"),
    ),
    ("POST", "/auth/magic-link", Some("POST /auth/magic-link")),
    ("GET", "/", Some("GET /")),
    ("PUT", "/todos", None),
    ("OPTIONS", "/todos", None),
    ("GET", "/nowhere/at/all", None),
    ("GET", "/todos/%zz", None),
];

/// Routes every request, taking each label and looking up each failure,
/// `times` times over: the number of routes it found.
fn route_all(routes: &Routes, table: &RouteTable, times: usize) -> usize {
    let mut found = 0;
    for _ in 0..times {
        for (method, path, expected) in REQUESTS {
            let label = match routes.find(method, path) {
                Found::Route {
                    route: Some(route), ..
                } => routes.label(route),
                Found::Webhook(at) => routes.webhook_label(at),
                Found::Health { ready } => Some(routes.health_label(ready)),
                _ => None,
            };
            assert!(label.as_deref() == expected, "{method} {path}");
            found += usize::from(label.is_some());
        }
        for (method, path) in [("POST", "/webhooks/payments"), ("GET", "/health/ready")] {
            assert!(!matches!(
                routes.find(method, path),
                Found::Route { .. } | Found::NotFound
            ));
        }
        for key in ["type QuantityUnavailable", "type NotFound", "type Other"] {
            found += usize::from(table.failure(key).is_some());
        }
    }
    found
}

fn main() {
    let table = table();
    let routes = Routes::new(&table, &[webhook()]);
    // Once before counting, so nothing made the first time is counted.
    route_all(&routes, &table, 1);
    let region = Region::new(COUNTED);
    let found = route_all(&routes, &table, 100);
    let change = region.change();
    assert_eq!(found, 100 * (10 + 2));
    assert_eq!(
        (change.allocations, change.reallocations),
        (0, 0),
        "routing allocated: {change:?}"
    );
    println!("routing 1,600 requests allocated nothing");
}
