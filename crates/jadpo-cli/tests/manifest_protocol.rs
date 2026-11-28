//! The manifest protocol (area 16, "The manifest protocol"; area 14,
//! "`jadpo manifest`"; chunk O3). `jadpo manifest send` gives a monitoring
//! product each deployable's manifest, the part of the build manifest that
//! decodes what its telemetry carries, at the endpoint and with the
//! credential the deployed app sends its telemetry with; and `jadpo
//! manifest protocol` is the conformance test a product's CI runs against
//! its own endpoint. Both run against a product double on this machine.

use std::path::Path;

use jadpo_cli::{Output, run};
use jadpo_term::Environment;
use serde_json::Value;

use crate::http::tempdir;
use crate::support::fake_product::{FakeProduct, Faults};
use crate::telemetry::{APPLICATION, project_at};

fn jadpo(root: &Path, args: &[&str], vars: &[(&str, &str)]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty().with_cwd(root);
    for (name, value) in vars {
        env = env.with_var(name, value);
    }
    run(&args, &env)
}

fn json(root: &Path, args: &[&str], vars: &[(&str, &str)]) -> Value {
    let mut args = args.to_vec();
    args.push("--json");
    let output = jadpo(root, &args, vars);
    let said: Value = serde_json::from_str(&output.stdout)
        .unwrap_or_else(|_| panic!("JSON: {}{}", output.stdout, output.stderr));
    said["data"].clone()
}

/// The project checked, so its records are written, as a deploy's are
/// committed.
fn checked(root: &Path) {
    let checked = jadpo(root, &["check"], &[]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
}

/// The build id `jadpo check` gives the app's one deployable.
fn build_of(root: &Path) -> String {
    checked(root);
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("build/manifest.json")).expect("the manifest"),
    )
    .expect("JSON");
    manifest["deployables"]["app"]["build"]
        .as_str()
        .expect("a build id")
        .to_string()
}

fn rule_of(output: &Output) -> String {
    let said: Value = serde_json::from_str(&output.stdout).unwrap_or(Value::Null);
    said["diagnostics"][0]["rule"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

#[test]
fn a_deploy_sends_each_builds_manifest_to_the_telemetry_endpoint() {
    let dir = tempdir::Dir::new("manifest-send");
    let root = dir.path();
    project_at(root, APPLICATION);
    let build = build_of(root);
    let product = FakeProduct::start(None);

    let sent = json(root, &["manifest", "send"], &[("COLLECTOR", &product.url)]);
    assert_eq!(sent["builds"][0]["deployable"], "app", "{sent}");
    assert_eq!(sent["builds"][0]["build"], build.as_str());
    assert_eq!(sent["builds"][0]["held"], "new");
    // The endpoint is named by its origin alone.
    assert_eq!(sent["endpoint"], product.url.as_str());

    let requests = product.requests();
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert_eq!(requests[0].method, "PUT");
    assert_eq!(requests[0].path, format!("/jadpo/v1/builds/{build}"));
    assert_eq!(requests[0].header("content-type"), Some("application/json"));
    let body = requests[0].json();
    assert_eq!(body["protocol"], 1);
    assert_eq!(body["build"], build.as_str());
    assert_eq!(body["deployable"], "app");
    assert_eq!(body["target"], "native");
    assert!(body["compiler"].is_string(), "{body}");
    let nodes = body["nodes"].as_array().expect("nodes");
    let greet = nodes
        .iter()
        .find(|node| node["id"] == "action greet")
        .unwrap_or_else(|| panic!("the action, which `jadpo.identity` names: {body}"));
    assert_eq!(greet["kind"], "action");
    assert_eq!(greet["name"], "greet");
    assert_eq!(greet["domain"], "greeting");
    assert_eq!(greet["file"], "domains/greeting/actions/greet.jadpo");
    assert_eq!(greet["lines"][0], 5);
    // What decodes telemetry, and nothing else: no description, digest
    // or edge.
    for left_out in ["doc", "digest", "calls", "raises"] {
        assert!(greet.get(left_out).is_none(), "{left_out}: {greet}");
    }
    // Only what the deployable reaches, each a place in its source, in
    // key order, so the same build always sends the same bytes.
    assert!(nodes.iter().all(|node| node["file"].is_string()), "{body}");
    let ids: Vec<&str> = nodes.iter().filter_map(|n| n["id"].as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted);
    assert!(!ids.iter().any(|id| id.starts_with("type ")), "{ids:?}");

    // Sent again, the product already holds it.
    let again = json(root, &["manifest", "send"], &[("COLLECTOR", &product.url)]);
    assert_eq!(again["builds"][0]["held"], "already", "{again}");
    let requests = product.requests();
    assert_eq!(requests[1].body, requests[0].body);

    let screen = jadpo(root, &["manifest", "send"], &[("COLLECTOR", &product.url)]);
    assert_eq!(screen.code, 0, "{}", screen.stderr);
    assert!(
        screen.stdout.contains("Sent 1 build manifest") && screen.stdout.contains(&build),
        "{}",
        screen.stdout
    );
}

#[test]
fn the_credential_goes_with_it_and_a_refusal_stops_the_deploy() {
    let dir = tempdir::Dir::new("manifest-credential");
    let root = dir.path();
    project_at(
        root,
        "application {\n    telemetry: {\n        endpoint: config.collector\n        credential: {\n            header: \"x-api-key\"\n            value: config.collector_key\n        }\n    }\n}\n",
    );
    std::fs::write(
        root.join("config.jadpo"),
        "config {\n    collector: Url\n    collector_key: SecretText {\n        secret: true\n    }\n}\n",
    )
    .expect("written");
    checked(root);
    let product = FakeProduct::start(Some(("x-api-key", "k-1")));

    let sent = jadpo(
        root,
        &["manifest", "send"],
        &[("COLLECTOR", &product.url), ("COLLECTOR_KEY", "k-1")],
    );
    assert_eq!(sent.code, 0, "{}{}", sent.stdout, sent.stderr);
    assert_eq!(product.requests()[0].header("x-api-key"), Some("k-1"));

    let refused = jadpo(
        root,
        &["manifest", "send", "--json"],
        &[("COLLECTOR", &product.url), ("COLLECTOR_KEY", "k-wrong-9")],
    );
    assert_eq!(refused.code, 1, "{}", refused.stdout);
    assert_eq!(rule_of(&refused), "manifest.send_failed");
    assert!(refused.stdout.contains("401"), "{}", refused.stdout);
    // The key itself is never said.
    assert!(!refused.stdout.contains("k-wrong-9"), "{}", refused.stdout);
}

#[test]
fn the_standard_variables_serve_an_app_whose_section_leaves_them_out() {
    let dir = tempdir::Dir::new("manifest-standard");
    let root = dir.path();
    project_at(root, "application {\n    telemetry: {}\n}\n");
    checked(root);
    let product = FakeProduct::start(Some(("x-honeycomb-team", "a key")));
    let sent = jadpo(
        root,
        &["manifest", "send"],
        &[
            ("OTEL_EXPORTER_OTLP_ENDPOINT", &format!("{}/", product.url)),
            ("OTEL_EXPORTER_OTLP_HEADERS", "x-honeycomb-team=a%20key"),
        ],
    );
    assert_eq!(sent.code, 0, "{}{}", sent.stdout, sent.stderr);
    assert!(
        product.requests()[0]
            .path
            .starts_with("/jadpo/v1/builds/b_")
    );
}

#[test]
fn an_app_that_sends_no_telemetry_has_nowhere_to_send_it() {
    let dir = tempdir::Dir::new("manifest-nowhere");
    let root = dir.path();
    project_at(root, "application {}\n");
    checked(root);
    let said = jadpo(root, &["manifest", "send", "--json"], &[]);
    assert_eq!(said.code, 1, "{}", said.stdout);
    assert_eq!(rule_of(&said), "manifest.send_failed");
    assert!(said.stdout.contains("telemetry:"), "{}", said.stdout);

    // With the section, an endpoint setting no variable gives is named.
    project_at(root, APPLICATION);
    checked(root);
    let unset = jadpo(root, &["manifest", "send", "--json"], &[]);
    assert_eq!(unset.code, 1, "{}", unset.stdout);
    assert_eq!(rule_of(&unset), "manifest.send_failed");
    assert!(unset.stdout.contains("COLLECTOR"), "{}", unset.stdout);
}

#[test]
fn an_unreachable_product_and_an_unknown_deployable_stop_it() {
    let dir = tempdir::Dir::new("manifest-unreachable");
    let root = dir.path();
    project_at(root, APPLICATION);
    checked(root);
    let said = jadpo(
        root,
        &["manifest", "send", "--json"],
        &[("COLLECTOR", "http://127.0.0.1:1")],
    );
    assert_eq!(said.code, 1, "{}", said.stdout);
    assert_eq!(rule_of(&said), "manifest.send_failed");
    assert!(
        said.stdout.contains("nothing is listening there"),
        "{}",
        said.stdout
    );

    let unknown = jadpo(root, &["manifest", "send", "worker", "--json"], &[]);
    assert_ne!(unknown.code, 0, "{}", unknown.stdout);
    assert_eq!(rule_of(&unknown), "cli.manifest_arguments");
}

#[test]
fn a_product_that_follows_the_protocol_conforms() {
    let product = FakeProduct::start(None);
    let dir = tempdir::Dir::new("manifest-conforms");
    let said = json(dir.path(), &["manifest", "protocol", &product.url], &[]);
    assert_eq!(said["conforms"], true, "{said}");
    assert_eq!(said["checks"], 7);
    let screen = jadpo(dir.path(), &["manifest", "protocol", &product.url], &[]);
    assert_eq!(screen.code, 0, "{}{}", screen.stdout, screen.stderr);
    assert!(
        screen.stdout.contains("Conforms to the manifest protocol"),
        "{}",
        screen.stdout
    );
    // The protocol's sample build, kept as it was first sent.
    let held = product
        .held(jadpo_cli::manifest_protocol::SAMPLE_BUILD)
        .expect("the sample is held");
    assert_eq!(held, jadpo_cli::manifest_protocol::sample());

    // With a credential, it is sent, and a request without it must be
    // refused.
    let guarded = FakeProduct::start(Some(("x-api-key", "k-2")));
    let said = json(
        dir.path(),
        &["manifest", "protocol", &guarded.url],
        &[("OTEL_EXPORTER_OTLP_HEADERS", "x-api-key=k-2")],
    );
    assert_eq!(said["conforms"], true, "{said}");
    assert_eq!(said["checks"], 8);
}

#[test]
fn a_product_that_differs_is_told_how() {
    let dir = tempdir::Dir::new("manifest-differs");
    let replaces = FakeProduct::with_faults(
        None,
        Faults {
            replaces: true,
            lenient: true,
            ..Faults::default()
        },
    );
    let said = jadpo(
        dir.path(),
        &["manifest", "protocol", &replaces.url, "--json"],
        &[],
    );
    assert_eq!(said.code, 1, "{}", said.stdout);
    assert_eq!(rule_of(&said), "manifest.protocol_mismatch");
    let body: Value = serde_json::from_str(&said.stdout).expect("JSON");
    let problems: Vec<String> = body["data"]["mismatches"]
        .as_array()
        .expect("mismatches")
        .iter()
        .map(|m| format!("{} {}", m["request"], m["problem"]))
        .collect();
    assert_eq!(problems.len(), 2, "{problems:?}");
    assert!(
        problems[0].contains("kept"),
        "a build sent again replaced the first: {problems:?}"
    );
    assert!(problems[1].contains("422"), "{problems:?}");

    let open = FakeProduct::with_faults(
        Some(("x-api-key", "k-3")),
        Faults {
            open: true,
            ..Faults::default()
        },
    );
    let said = jadpo(
        dir.path(),
        &["manifest", "protocol", &open.url, "--json"],
        &[("OTEL_EXPORTER_OTLP_HEADERS", "x-api-key=k-3")],
    );
    assert_eq!(said.code, 1, "{}", said.stdout);
    assert!(said.stdout.contains("401"), "{}", said.stdout);

    let screen = jadpo(dir.path(), &["manifest", "protocol", &replaces.url], &[]);
    assert!(
        screen.stdout.contains("Differs from the manifest protocol"),
        "{}",
        screen.stdout
    );

    let nothing = jadpo(
        dir.path(),
        &["manifest", "protocol", "http://127.0.0.1:1", "--json"],
        &[],
    );
    assert_eq!(nothing.code, 1, "{}", nothing.stdout);
    assert_eq!(rule_of(&nothing), "manifest.protocol_mismatch");
}
