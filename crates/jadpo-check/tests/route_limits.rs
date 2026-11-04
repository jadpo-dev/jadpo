//! A route's and an API's own `rate_limit:` (area 16, "Rate limits"; chunk
//! G4): what `requests:`, `within:` and `by:` may hold, and a window a
//! store can't count refused where it is written.

use crate::support;

use jadpo_db::Layout;

const MEMOS: &[(&str, &str)] = &[
    (
        "policy.jadpo",
        "use memos\n\npolicy {\n    actions: {\n        memos.keep_memo: [Access.public]\n    }\n}\n",
    ),
    (
        "domains/memos/types/memo_view.jadpo",
        "type MemoView = Output {\n    text: Text\n}\n",
    ),
    (
        "domains/memos/actions/keep_memo.jadpo",
        "use memos/types { MemoView }\n\naction keep_memo() -> MemoView {\n    return MemoView {\n        text: \"kept\"\n    }\n}\n",
    ),
];

/// The memos route with `rate_limit` written as `limit` (nothing when
/// empty).
fn route(limit: &str) -> String {
    format!(
        "use memos/types {{ MemoView }}\nuse memos/actions {{ keep_memo }}\n\nroutes Memos {{\n    prefix: /memos\n\n    POST / {{\n        authentication: RouteAuthentication.none\n{limit}        output: MemoView\n        run: keep_memo()\n    }}\n}}\n"
    )
}

fn problems(application: &str, limit: &str) -> Vec<String> {
    let routes = route(limit);
    let mut files: Vec<(&str, &str)> = vec![
        ("app.jadpo", application),
        ("domains/memos/routes/memos.jadpo", &routes),
    ];
    files.extend_from_slice(MEMOS);
    let mut found: Vec<String> = support::problems(Layout::Project, &files)
        .into_iter()
        .filter(|problem| !problem.contains(" migration.") && !problem.contains(" record."))
        .collect();
    found.sort();
    found
}

fn on_a_route(limit: &str) -> Vec<String> {
    problems("application {}\n", limit)
}

fn block(body: &str) -> String {
    format!("        rate_limit: {{\n{body}        }}\n")
}

const CLEAN: &[&str] = &[];

/// Cloudflare's binding counts the app's limits, on a Worker.
const ON_THE_BINDING: &str = "application {\n    connections: {\n        limits: {\n            driver: Driver.cloudflare_rate_limits\n            namespace: 1001\n        }\n    }\n    rate_limits: {\n        store: limits\n    }\n    deployables: {\n        edge: {\n            serves: [main]\n            target: Target.wasm\n        }\n    }\n}\n";

/// The API's own limit, `limit`, counted natively.
fn api(limit: &str) -> String {
    format!(
        "application {{\n    apis: {{\n        main: {{\n            prefix: /\n{limit}        }}\n    }}\n}}\n"
    )
}

#[test]
fn a_route_writes_its_own_limit() {
    // The page's example: five an hour from each address.
    assert_eq!(
        on_a_route(&block(
            "            requests: 5\n            within: 1h\n            by: RateLimitKey.ip\n"
        )),
        CLEAN
    );
    // `by:` is the caller unless it says otherwise.
    assert_eq!(
        on_a_route(&block("            requests: 5\n            within: 1m\n")),
        CLEAN
    );
    assert_eq!(
        on_a_route(&block(
            "            requests: 5\n            within: 1m\n            by: RateLimitKey.caller\n"
        )),
        CLEAN
    );
}

#[test]
fn a_limit_says_how_many_and_over_how_long() {
    let rule = "domains/memos/routes/memos.jadpo route.rate_limit_invalid";
    assert_eq!(
        on_a_route("        rate_limit: 5\n"),
        [format!("{rule}: `rate_limit:` holds no block")]
    );
    assert_eq!(
        on_a_route(&block("            within: 1m\n")),
        [format!("{rule}: `rate_limit:` says no `requests:`")]
    );
    assert_eq!(
        on_a_route(&block("            requests: 5\n")),
        [format!("{rule}: `rate_limit:` says no `within:`")]
    );
    assert_eq!(
        on_a_route(&block("            requests: 0\n            within: 1m\n")),
        [format!("{rule}: `0` can't be a limit's `requests:`")]
    );
    assert_eq!(
        on_a_route(&block(
            "            requests: 5\n            requests: 6\n            within: 1m\n"
        )),
        [format!("{rule}: `requests:` is written twice")]
    );
    assert_eq!(
        on_a_route(&block(
            "            requests: 5\n            per: 1m\n            within: 1m\n"
        )),
        [format!("{rule}: `per` isn't part of a rate limit")]
    );
    // A window is whole seconds, from one second to a day: a `Retry-After`
    // and a `w=` are in seconds, and a longer allowance is a plan's limit,
    // application data rather than something a server's memory can hold.
    for within in ["500ms", "1500ms", "2d", "soon", "0s"] {
        assert_eq!(
            on_a_route(&block(&format!(
                "            requests: 5\n            within: {within}\n"
            ))),
            [format!("{rule}: `{within}` can't be a limit's `within:`")]
        );
    }
    assert_eq!(
        on_a_route(&block(
            "            requests: 5\n            within: 1m\n            by: RateLimitKey.everyone\n"
        )),
        [format!(
            "{rule}: `RateLimitKey.everyone` isn't a `RateLimitKey`"
        )]
    );
}

/// The binding counts windows of 10 or 60 seconds (area 16, "Counting on
/// Workers"); a limit it can't count is refused where it is written, never
/// quietly counted over another window.
#[test]
fn a_window_the_binding_cant_count_is_refused() {
    let limit = |within: &str| {
        block(&format!(
            "            requests: 5\n            within: {within}\n            by: RateLimitKey.ip\n"
        ))
    };
    assert_eq!(problems(ON_THE_BINDING, &limit("10s")), CLEAN);
    assert_eq!(problems(ON_THE_BINDING, &limit("1m")), CLEAN);
    assert_eq!(problems(ON_THE_BINDING, &limit("60s")), CLEAN);
    for within in ["30s", "1h"] {
        assert_eq!(
            problems(ON_THE_BINDING, &limit(within)),
            [format!(
                "domains/memos/routes/memos.jadpo rate_limits.window_unsupported: `within: {within}` can't be counted in Cloudflare's rate-limiting binding"
            )]
        );
    }
    // An API's own limit the same, where it is written.
    let written = ON_THE_BINDING.replace(
        "    connections: {",
        "    apis: {\n        main: {\n            prefix: /\n            rate_limit: {\n                requests: 5\n                within: 1h\n            }\n        }\n    }\n    connections: {",
    );
    assert_eq!(
        problems(&written, ""),
        [
            "app.jadpo rate_limits.window_unsupported: `within: 1h` can't be counted in Cloudflare's rate-limiting binding"
        ]
    );
    // Counted natively, in memory, any window from a second to a day is
    // fine, an hour's among them.
    assert_eq!(on_a_route(&limit("1h")), CLEAN);
}

#[test]
fn an_api_writes_the_limit_its_routes_take() {
    assert_eq!(
        problems(
            &api(
                "            rate_limit: {\n                requests: 300\n                within: 1m\n                by: RateLimitKey.ip\n            }\n"
            ),
            ""
        ),
        CLEAN
    );
    assert_eq!(
        problems(
            &api("            rate_limit: {\n                within: 1m\n            }\n"),
            ""
        ),
        ["app.jadpo route.rate_limit_invalid: `rate_limit:` says no `requests:`"]
    );
}

#[test]
fn a_limit_is_in_the_routes_plan_with_what_counts_it() {
    use jadpo_db::Database;
    use jadpo_std::RateLimitKey;
    use jadpo_std::http::RateLimit;

    let routes = route(&block(
        "            requests: 5\n            within: 1m\n            by: RateLimitKey.ip\n",
    ));
    let application = api(
        "            rate_limit: {\n                requests: 300\n                within: 10s\n            }\n",
    );
    let more = "use memos/types { MemoView }\nuse memos/actions { keep_memo }\n\nroutes Others {\n    prefix: /others\n\n    POST / {\n        authentication: RouteAuthentication.none\n        output: MemoView\n        run: keep_memo()\n    }\n}\n";
    let mut files: Vec<(&str, &str)> = vec![
        ("app.jadpo", application.as_str()),
        ("domains/memos/routes/memos.jadpo", routes.as_str()),
        ("domains/memos/routes/others.jadpo", more),
    ];
    files.extend_from_slice(MEMOS);
    let db = Database::default();
    let project = support::project(&db, Layout::Project, &files);
    let apis = jadpo_check::routes::api_table(&db, project);
    let groups = jadpo_check::routes::groups(&db, project);
    let limits: Vec<_> = groups
        .routes()
        .map(|(group, route)| {
            let limit = jadpo_check::routes::limit_of(apis, group, route).expect("a limit");
            (route.label(), limit.limit, limit.by, limit.counted)
        })
        .collect();
    assert_eq!(
        limits,
        [
            (
                "POST /memos".to_string(),
                RateLimit {
                    requests: 5,
                    window_ms: 60_000,
                },
                RateLimitKey::Ip,
                "route POST /memos".to_string(),
            ),
            (
                "POST /others".to_string(),
                RateLimit {
                    requests: 300,
                    window_ms: 10_000,
                },
                RateLimitKey::Caller,
                "api main".to_string(),
            ),
        ]
    );
    // In the graph, each is a limit its route uses.
    let graph = jadpo_check::graph::graph(&db, project);
    for (route, limit) in [
        ("route POST /memos", "rate_limit POST /memos"),
        ("route POST /others", "rate_limit api main"),
    ] {
        let limit = jadpo_graph::Key::from_text(limit);
        assert!(graph.get(&limit).is_some(), "{limit:?}");
        let route = graph
            .find(&jadpo_graph::Key::from_text(route))
            .expect("the route");
        assert!(
            graph
                .outgoing(route, jadpo_graph::EdgeKind::Uses)
                .any(|used| graph.node(used).key == limit),
            "{route:?}"
        );
    }
}
