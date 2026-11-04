//! A route's and an API's own `rate_limit:` over real HTTP (area 16, "Rate
//! limits"; chunk G4). One project answers the same requests the same way
//! on every host: `jadpo dev` here, its compiled native deployable in
//! `deployable.rs` and its Worker under workerd in `workers/limits.rs`. An
//! anonymous `POST` that starts expensive work takes three a minute and an
//! anonymous `GET` that runs an action, as one calling a model does, two in
//! 10 seconds, each counted alone; two routes of an API share its limit; and
//! a route that writes none keeps the default.

use std::path::Path;

use jadpo_cli::run;
use jadpo_term::Environment;
use serde_json::Value;

use super::http::{Dev, tempdir};

/// `routes` holding one anonymous route that keeps its action, limited as
/// `limit` says.
fn route(method: &str, name: &str, group: &str, api: &str, paths: &[&str], limit: &str) -> String {
    let api = if api.is_empty() {
        String::new()
    } else {
        format!("    api: {api}\n")
    };
    let entries: Vec<String> = paths
        .iter()
        .map(|path| {
            format!(
                "    {method} {path} {{\n        authentication: RouteAuthentication.none\n{limit}        output: Kept\n        run: keep()\n    }}\n"
            )
        })
        .collect();
    format!(
        "use desk/types {{ Kept }}\nuse desk/actions {{ keep }}\n\nroutes {group} {{\n{api}    prefix: /{name}\n\n{}}}\n",
        entries.join("\n")
    )
}

/// `application`'s body, inside `apis:`.
const APIS: &str = "    apis: {\n        main: {\n            prefix: /\n        }\n        partners: {\n            prefix: /partners\n            rate_limit: {\n                requests: 2\n                within: 1m\n            }\n        }\n    }\n";

/// The application as `jadpo dev` and a native deployable run it.
pub(crate) fn native_application() -> String {
    format!(
        "application {{\n{APIS}    deployables: {{\n        desk: {{\n            serves: [main, partners]\n            target: Target.native\n        }}\n    }}\n}}\n"
    )
}

/// The application as a Worker runs it, counted in Cloudflare's binding.
pub(crate) fn worker_application() -> String {
    format!(
        "application {{\n{APIS}    connections: {{\n        limits: {{\n            driver: Driver.cloudflare_rate_limits\n            namespace: 1001\n        }}\n    }}\n    rate_limits: {{\n        store: limits\n    }}\n    deployables: {{\n        desk: {{\n            serves: [main, partners]\n            target: Target.wasm\n        }}\n    }}\n}}\n"
    )
}

/// Writes the project at `root`.
pub(crate) fn project_at(root: &Path, application: &str) {
    let limited = |requests: u32, within: &str, by: &str| {
        format!(
            "        rate_limit: {{\n            requests: {requests}\n            within: {within}\n            by: RateLimitKey.{by}\n        }}\n"
        )
    };
    let files = [
        ("app.jadpo", application.to_string()),
        (
            "policy.jadpo",
            "use desk\n\npolicy {\n    actions: {\n        desk.keep: [Access.public]\n    }\n}\n"
                .to_string(),
        ),
        (
            "domains/desk/types/kept.jadpo",
            "/// What was kept.\ntype Kept = Output {\n    words: Text\n}\n".to_string(),
        ),
        (
            "domains/desk/actions/keep.jadpo",
            "use desk/types { Kept }\n\n/// Keeps nothing, and says so.\naction keep() -> Kept {\n    return Kept {\n        words: \"kept\"\n    }\n}\n".to_string(),
        ),
        (
            "domains/desk/routes/releases.jadpo",
            route("POST", "releases", "Releases", "", &["/"], &limited(3, "1m", "ip")),
        ),
        (
            "domains/desk/routes/estimates.jadpo",
            route("GET", "estimates", "Estimates", "", &["/"], &limited(2, "10s", "caller")),
        ),
        (
            "domains/desk/routes/plain.jadpo",
            route("POST", "plain", "Plain", "", &["/"], ""),
        ),
        (
            "domains/desk/routes/partners_shared.jadpo",
            route("POST", "partners/shared", "PartnersShared", "partners", &["/a", "/b"], ""),
        ),
    ];
    for (path, text) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("a folder")).expect("folders");
        if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
            std::fs::write(&path, text).expect("written");
        }
    }
}

/// Each request in order (a `GET` on `/estimates`, a `POST` on the rest),
/// what it should answer, and the policy a refusal
/// says it was over: what every host must give.
pub(crate) const ANSWERS: [(&str, u16, Option<&str>); 12] = [
    // Three a minute, each address alone.
    ("/releases", 200, None),
    ("/releases", 200, None),
    ("/releases", 200, None),
    ("/releases", 429, Some("\"default\";q=3;w=60")),
    // Refused there, a route with a limit of its own still answers, since
    // it counts alone.
    ("/estimates", 200, None),
    ("/estimates", 200, None),
    ("/estimates", 429, Some("\"default\";q=2;w=10")),
    // A route writing none keeps the default allowance.
    ("/plain", 200, None),
    // An API's limit is one count across its routes.
    ("/partners/shared/a", 200, None),
    ("/partners/shared/b", 200, None),
    ("/partners/shared/a", 429, Some("\"default\";q=2;w=60")),
    ("/partners/shared/b", 429, Some("\"default\";q=2;w=60")),
];

/// One request's answer: its status and its head's lines in lower case.
pub(crate) type Answered = (u16, String);

/// Sends [`ANSWERS`]' requests through `send` and checks every answer.
pub(crate) fn answers_as_every_host_does(send: impl Fn(&str, &str) -> Answered) {
    for (path, status, policy) in ANSWERS {
        let method = if path == "/estimates" { "GET" } else { "POST" };
        let (got, head) = send(method, path);
        let value = |name: &str| {
            head.lines().find_map(|line| {
                let (header, value) = line.split_once(':')?;
                (header.trim().eq_ignore_ascii_case(name)).then(|| value.trim().to_string())
            })
        };
        assert_eq!(got, status, "{path}: {head}");
        assert_eq!(
            value("ratelimit-policy").as_deref(),
            policy,
            "{path}: {head}"
        );
        if status == 429 {
            let window: u64 = if policy.is_some_and(|policy| policy.ends_with("w=10")) {
                10
            } else {
                60
            };
            let retry: u64 = value("retry-after")
                .expect("Retry-After")
                .parse()
                .expect("seconds");
            assert!((1..=window).contains(&retry), "{path}: {retry}");
            assert_eq!(
                value("ratelimit").as_deref(),
                Some(format!("\"default\";r=0;t={retry}").as_str()),
                "{path}: {head}"
            );
        }
    }
}

#[test]
fn a_routes_limit_is_counted_on_jadpo_dev() {
    let dir = tempdir::Dir::new("route-limits");
    project_at(dir.path(), &native_application());
    let env = Environment::empty().with_cwd(dir.path());
    let records = run(&["records".to_string(), "update".to_string()], &env);
    assert_eq!(records.code, 0, "{}{}", records.stdout, records.stderr);
    let dev = Dev::start_without_database(dir.path());
    answers_as_every_host_does(|method, path| {
        let reply = dev.send(method, path, &[], "");
        let head = ["ratelimit-policy", "ratelimit", "retry-after"]
            .iter()
            .filter_map(|name| reply.header(name).map(|value| format!("{name}: {value}\n")))
            .collect();
        (reply.status, head)
    });
}

/// `jadpo routes` and OpenAPI say what each route is limited to (area 16,
/// "What a client sees").
#[test]
fn a_routes_limit_is_shown_in_routes_and_openapi() {
    let dir = tempdir::Dir::new("route-limits-shown");
    project_at(dir.path(), &native_application());
    let env = Environment::empty().with_cwd(dir.path());
    let records = run(&["records".to_string(), "update".to_string()], &env);
    assert_eq!(records.code, 0, "{}{}", records.stdout, records.stderr);
    let routes = run(&["routes".to_string()], &env);
    assert_eq!(routes.code, 0, "{}{}", routes.stdout, routes.stderr);
    for shown in ["3/1m ip", "2/10s caller", "120/1m ip", "2/1m caller"] {
        assert!(routes.stdout.contains(shown), "{shown}: {}", routes.stdout);
    }
    let listed = run(&["routes".to_string(), "--json".to_string()], &env);
    let listed: Value = serde_json::from_str(&listed.stdout).expect("JSON");
    let every = listed["data"]["routes"].as_array().expect("routes");
    let releases = every
        .iter()
        .find(|route| route["path"] == "/releases")
        .expect("the route");
    assert_eq!(
        releases["rate_limit"],
        serde_json::json!({"requests": 3, "within": "1m", "by": "ip"}),
        "{releases}"
    );
    assert!(
        every
            .iter()
            .filter(|route| route["generated"] == true)
            .all(|route| route["rate_limit"].is_null()),
        "{listed}"
    );

    let checked = run(&["check".to_string()], &env);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let document: Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join("build/openapi/main.json")).expect("the document"),
    )
    .expect("JSON");
    assert_eq!(
        document["paths"]["/releases"]["post"]["x-jadpo-rate-limit"],
        serde_json::json!({"requests": 3, "within": "1m", "by": "ip"}),
    );
    assert_eq!(
        document["paths"]["/plain"]["post"]["x-jadpo-rate-limit"],
        serde_json::json!({"requests": 120, "within": "1m", "by": "ip"}),
    );
}
