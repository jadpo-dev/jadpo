//! What the routes Jadpo answers itself are limited to (area 16, "What a
//! client sees"; area 7, "The `x-jadpo` fields"): `jadpo routes` and OpenAPI
//! say so for every generated route, the sign-in, data-request and file
//! routes among them, and a refused magic link or address check says which
//! limit it was over in the `RateLimit` headers (area 7, "Rate-limited
//! requests"). The refusals run the same way on `jadpo dev` and on a
//! compiled native deployable (`deployable.rs`).

use std::path::Path;

use jadpo_cli::{Output, run};
use jadpo_term::Environment;
use serde_json::{Value, json};

use super::http::{Dev, tempdir};

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    run(&args, &Environment::empty().with_cwd(cwd))
}

/// `files`, with `vault` kept at a path this machine may write to, so a
/// host that makes it as it starts can.
fn at_a_writable_path(files: Vec<(String, String)>, vault: &Path) -> Vec<(String, String)> {
    files
        .into_iter()
        .map(|(path, text)| {
            let text = text.replace("/var/lib/notes/vault", &vault.display().to_string());
            (path, text)
        })
        .collect()
}

/// The project that takes data requests by address, with its own settings
/// for how often a magic link is sent, and passkeys beside it.
pub(crate) fn generated_files(vault: &Path) -> Vec<(String, String)> {
    at_a_writable_path(crate::privacy_requests::files(), vault)
        .into_iter()
        .filter(|(path, _)| !path.ends_with(".test.jadpo"))
        .map(|(path, text)| {
            let text = if path == "authentication.jadpo" {
                text.replace(
                    "link: \"/sign-in/confirm\"",
                    "link: \"/sign-in/confirm\"\n            resend_after: 2m\n            codes_per_hour: 3\n            failed_codes_per_day: 7",
                )
                .replace("sign_in: {", "sign_in: {\n        passkey: {}")
            } else {
                text
            };
            (path, text)
        })
        .collect()
}

/// The project over its own routes: `magic_links.request` and
/// `privacy.request`, each an action of the app's.
pub(crate) fn own_files(vault: &Path) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> =
        at_a_writable_path(crate::privacy_requests::own_route_files(), vault)
            .into_iter()
            // Their tests go through the generated magic-link routes.
            .filter(|(path, _)| !path.ends_with(".test.jadpo"))
            .map(|(path, text)| {
                let text = match path.as_str() {
                    "authentication.jadpo" => text.replace(
                        "link: \"/sign-in/confirm\"",
                        "link: \"/sign-in/confirm\"\n            generate_routes: false",
                    ),
                    "policy.jadpo" => text.replace(
                        "notes.ask_own: [Access.public]",
                        "notes.ask_own: [Access.public]\n        notes.ask_link: [Access.public]",
                    ),
                    _ => text,
                };
                (path, text)
            })
            .collect();
    out.extend([
        (
            "domains/notes/actions/ask_link.jadpo".to_string(),
            "use notes/types { OwnAsk }\n\n/// Asks for a magic link.\naction ask_link(input: OwnAsk) -> Unit {\n    magic_links.request(input.address, input.locale, none)\n}\n".to_string(),
        ),
        (
            "domains/notes/routes/own_link.jadpo".to_string(),
            "use notes/actions { ask_link }\nuse notes/types { OwnAsk }\n\nroutes OwnLink {\n    prefix: /own-link\n\n    POST / {\n        authentication: RouteAuthentication.none\n        input: OwnAsk\n        status: SuccessStatus.no_content\n        run: ask_link(input)\n    }\n}\n".to_string(),
        ),
    ]);
    out
}

/// `files`, naming the database's address in a setting, as a deployable
/// needs.
pub(crate) fn with_a_database_setting(files: Vec<(String, String)>) -> Vec<(String, String)> {
    files
        .into_iter()
        .map(|(path, text)| {
            let text = match path.as_str() {
                "app.jadpo" => text.replace(
                    "connections: {",
                    "connections: {\n        main: {\n            driver: Driver.postgres\n            url: config.database_url\n        }",
                ),
                "config.jadpo" => text.replace(
                    "config {",
                    "config {\n    database_url: PostgresUrl",
                ),
                _ => text,
            };
            (path, text)
        })
        .collect()
}

/// Writes `files` at `root`, leaving alone what already says the same.
pub(crate) fn write_all(root: &Path, files: &[(String, String)]) {
    for (path, text) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("a folder")).expect("folders");
        if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
            std::fs::write(&path, text).expect("written");
        }
    }
}

/// One request's answer: its status and its head's lines.
pub(crate) type Answered = (u16, String);

fn header(head: &str, name: &str) -> Option<String> {
    head.lines().find_map(|line| {
        let (header, value) = line.split_once(':')?;
        header
            .trim()
            .eq_ignore_ascii_case(name)
            .then(|| value.trim().to_string())
    })
}

/// A refusal over `policy`: a 429 whose `RateLimit` says none are left
/// until `Retry-After` has passed.
fn over(answered: &Answered, policy: &str) {
    let (status, head) = answered;
    assert_eq!(*status, 429, "{head}");
    assert_eq!(
        header(head, "ratelimit-policy").as_deref(),
        Some(policy),
        "{head}"
    );
    let wait = header(head, "retry-after").expect("Retry-After");
    assert_eq!(
        header(head, "ratelimit"),
        Some(format!("\"default\";r=0;t={wait}")),
        "{head}"
    );
}

const LINK: &str = "{\"address\":\"cy@example.com\"}";
const CHECK: &str = "{\"address\":\"cy@example.com\",\"locale\":\"en-GB\"}";

/// Refusals on the generated routes, through `send` (method, path, body).
pub(crate) fn generated_routes_say_their_limit(send: impl Fn(&str, &str, &str) -> Answered) {
    assert_eq!(send("POST", "/auth/magic-link", LINK).0, 202);
    over(
        &send("POST", "/auth/magic-link", LINK),
        "\"default\";q=1;w=120",
    );
    assert_eq!(send("POST", "/privacy/requests", CHECK).0, 202);
    over(
        &send("POST", "/privacy/requests", CHECK),
        "\"default\";q=1;w=60",
    );
}

/// Refusals on the app's own routes over the same operations.
pub(crate) fn own_routes_say_their_limit(send: impl Fn(&str, &str, &str) -> Answered) {
    for path in ["/own-link", "/own/ask"] {
        let (status, head) = send("POST", path, CHECK);
        assert!(status < 300, "{path}: {status} {head}");
        over(&send("POST", path, CHECK), "\"default\";q=1;w=60");
    }
}

/// An upload's and a download's address is counted against the default
/// allowance of its address, and says so when it is over.
pub(crate) fn file_addresses_say_their_limit(send: impl Fn(&str, &str, &str) -> Answered) {
    for path in ["/files/vault/uploads/upl_x", "/files/vault/some-link"] {
        let method = if path.contains("uploads") {
            "PUT"
        } else {
            "GET"
        };
        // Other requests in the same run have used some of the allowance,
        // so this asks until it runs out.
        let refused = (0..=jadpo_std::http::ADDRESS_LIMIT.requests)
            .map(|_| send(method, path, ""))
            .find(|(status, _)| *status == 429)
            .expect("a refusal within the allowance");
        over(&refused, "\"default\";q=120;w=60");
    }
}

/// `send` as `jadpo dev` answers it, with the site's own `Origin`.
fn through(dev: &Dev) -> impl Fn(&str, &str, &str) -> Answered + '_ {
    move |method, path, body| {
        let headers = [
            ("origin", "https://notes.example"),
            ("content-type", "application/json"),
        ];
        let reply = dev.send(method, path, &headers, body);
        let mut head: String = ["ratelimit-policy", "ratelimit", "retry-after"]
            .iter()
            .filter_map(|name| reply.header(name).map(|value| format!("{name}: {value}\n")))
            .collect();
        head.push_str(&String::from_utf8_lossy(reply.bytes()));
        (reply.status, head)
    }
}

#[test]
fn a_refused_generated_route_says_its_limit_on_jadpo_dev() {
    if !jadpo_test::live() {
        return;
    }
    let dir = tempdir::Dir::new("generated-limits-dev");
    let vault = dir.path().join("vault");
    let root = dir.path().join("notes");
    write_all(&root, &generated_files(&vault));
    jadpo(&root, &["migrate", "new", "initial"]);
    jadpo(&root, &["records", "update"]);
    let dev = Dev::start(&root);
    let send = through(&dev);
    generated_routes_say_their_limit(&send);
    file_addresses_say_their_limit(&send);
}

#[test]
fn a_refused_own_route_says_its_limit_on_jadpo_dev() {
    if !jadpo_test::live() {
        return;
    }
    let dir = tempdir::Dir::new("generated-limits-own");
    let vault = dir.path().join("vault");
    let root = dir.path().join("notes");
    write_all(&root, &own_files(&vault));
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    jadpo(&root, &["records", "update"]);
    let dev = Dev::start(&root);
    own_routes_say_their_limit(&through(&dev));
}

/// What `route` is limited to in `jadpo routes --json`.
fn listed(every: &[Value], method: &str, path: &str) -> Value {
    every
        .iter()
        .find(|route| route["method"] == method && route["path"] == path)
        .unwrap_or_else(|| panic!("{method} {path} isn't listed: {every:?}"))["rate_limit"]
        .clone()
}

#[test]
fn every_generated_route_says_what_it_is_limited_to() {
    let dir = tempdir::Dir::new("generated-limits-shown");
    let root = dir.path().join("notes");
    let vault = dir.path().join("vault");
    write_all(&root, &generated_files(&vault));
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let records = jadpo(&root, &["records", "update"]);
    assert_eq!(records.code, 0, "{}{}", records.stdout, records.stderr);

    let by_address = json!({"requests": 120, "within": "1m", "by": "ip"});
    let by_caller = json!({"requests": 600, "within": "1m", "by": "caller"});
    let sends = json!({
        "requests": 120, "within": "1m", "by": "ip",
        "also": [
            {"requests": 1, "within": "2m", "by": "email"},
            {"requests": 3, "within": "1h", "by": "email"},
        ],
    });
    let checks = json!({
        "requests": 120, "within": "1m", "by": "ip",
        "also": [
            {"requests": 1, "within": "1m", "by": "email"},
            {"requests": 5, "within": "1h", "by": "email"},
        ],
    });
    let codes = json!({
        "requests": 120, "within": "1m", "by": "ip",
        "also": [{"requests": 7, "within": "1d", "by": "email", "counts": "wrong_codes"}],
    });
    let wanted = [
        ("POST", "/auth/magic-link", &sends),
        ("POST", "/auth/magic-link/confirm", &codes),
        ("POST", "/auth/passkey/sign-in", &by_address),
        ("POST", "/auth/passkey/register", &by_caller),
        ("POST", "/privacy/requests", &checks),
        ("POST", "/privacy/requests/confirm", &by_address),
        ("POST", "/privacy/requests/cancel", &by_address),
        ("POST", "/privacy/exports/download", &by_address),
        ("POST", "/privacy/export", &by_caller),
        ("POST", "/privacy/erasure", &by_caller),
    ];
    let address_files = [
        ("PUT", "/files/vault/uploads/{token}"),
        ("PUT", "/files/vault/uploads/{token}/parts/{part}"),
        ("GET", "/files/vault/{link}"),
    ];

    // `jadpo routes`, for a person and as JSON.
    let shown = jadpo(&root, &["routes"]);
    assert_eq!(shown.code, 0, "{}{}", shown.stdout, shown.stderr);
    for words in [
        "120/1m ip, 1/2m email, 3/1h email",
        "120/1m ip, 1/1m email, 5/1h email",
        "120/1m ip, 7/1d email wrong codes",
        "600/1m caller",
    ] {
        assert!(shown.stdout.contains(words), "{words}: {}", shown.stdout);
    }
    assert!(
        shown.stdout.contains("/files/vault/uploads/{token}"),
        "{}",
        shown.stdout
    );
    let json_out = jadpo(&root, &["routes", "--json"]);
    let parsed: Value = serde_json::from_str(&json_out.stdout).expect("JSON");
    let every = parsed["data"]["routes"].as_array().expect("routes");
    for (method, path, limit) in wanted {
        assert_eq!(&listed(every, method, path), limit, "{method} {path}");
    }
    for (method, path) in address_files {
        assert_eq!(listed(every, method, path), by_address, "{method} {path}");
    }
    assert!(
        every.iter().all(|route| !route["rate_limit"].is_null()),
        "{parsed}"
    );

    // OpenAPI.
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let document: Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("build/openapi/main.json")).expect("the document"),
    )
    .expect("JSON");
    let limit_at = |method: &str, path: &str| {
        document["paths"][path][method.to_ascii_lowercase()]["x-jadpo-rate-limit"].clone()
    };
    for (method, path, limit) in wanted {
        assert_eq!(&limit_at(method, path), limit, "{method} {path}");
    }
    for (method, path) in address_files {
        assert_eq!(limit_at(method, path), by_address, "{method} {path}");
    }
}

/// Every operation of every example's documents says what it is limited
/// to, however its route came to be.
#[test]
fn every_operation_of_every_document_says_what_it_is_limited_to() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    for example in std::fs::read_dir(&examples).expect("examples") {
        let root = example.expect("an entry").path();
        if !root.join("app.jadpo").is_file() {
            continue;
        }
        let loaded = jadpo_db::load(&root);
        for (api, document) in
            jadpo_check::routes::openapi::documents(&loaded.db, loaded.project, "Example")
        {
            let paths: Value =
                serde_json::from_str(&document.to_document()).expect("the document is JSON");
            for (path, operations) in paths["paths"].as_object().expect("paths") {
                for (method, operation) in operations.as_object().expect("operations") {
                    assert!(
                        operation.get("x-jadpo-rate-limit").is_some(),
                        "{}: {api}: {method} {path}",
                        root.display()
                    );
                }
            }
        }
    }
}
