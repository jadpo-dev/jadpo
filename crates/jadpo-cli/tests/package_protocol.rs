//! `jadpo package protocol <document>` (area 14; area 11, "The registry
//! protocol"): R4's conformance test run by the `jadpo` tool, so a registry
//! application's CI needs no Rust. Its document is a file or an address.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use jadpo_cli::{Output, run};
use jadpo_registry::http::{Fetched, Server};
use jadpo_registry::local::LocalRegistry;
use jadpo_term::Environment;

fn folder(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "jadpo-package-protocol-{}-{name}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let mut all = vec!["package".to_string(), "protocol".to_string()];
    all.extend(args.iter().map(|arg| arg.to_string()));
    run(&all, &Environment::empty().with_cwd(cwd))
}

/// The local registry's document, built from the protocol's own table.
fn conforming() -> serde_json::Value {
    let registry = LocalRegistry::new();
    let served = registry.serve().expect("serves");
    jadpo_registry::conformance::load(&format!("{}openapi.json", served.url())).expect("read")
}

fn answering(status: u16, body: &'static str) -> Server {
    Server::start(Arc::new(move |_| Fetched {
        status,
        content_type: Some("application/json".to_string()),
        body: body.as_bytes().to_vec(),
    }))
    .expect("serves")
}

/// A document that conforms is one line, from a file named as it was
/// typed, with no project anywhere near it.
#[test]
fn a_file_that_conforms_is_one_line() {
    let root = folder("conforms");
    fs::create_dir_all(root.join("build/openapi")).unwrap();
    fs::write(
        root.join("build/openapi/main.json"),
        serde_json::to_vec(&conforming()).unwrap(),
    )
    .unwrap();
    let out = jadpo(&root, &["build/openapi/main.json"]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout,
        "Conforms to the registry protocol: build/openapi/main.json, 13 requests\n"
    );
    let json = jadpo(&root, &["build/openapi/main.json", "--json"]);
    assert_eq!(json.code, 0, "{}", json.stdout);
    let data: serde_json::Value = serde_json::from_str(&json.stdout).expect("JSON");
    assert_eq!(data["data"]["conforms"], true, "{data:#}");
    assert_eq!(data["data"]["requests"], 13, "{data:#}");
    assert_eq!(
        data["data"]["mismatches"],
        serde_json::json!([]),
        "{data:#}"
    );
}

#[test]
fn a_served_document_is_fetched_from_its_address() {
    let registry = LocalRegistry::new();
    let served = registry.serve().expect("serves");
    let root = folder("served");
    let address = format!("{}openapi.json", served.url());
    let out = jadpo(&root, &[&address]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    assert!(
        out.stdout.starts_with("Conforms to the registry protocol"),
        "{}",
        out.stdout
    );
}

/// One that differs fails the run, each difference under its request, so
/// the CI stops and the list says what to change.
#[test]
fn a_document_that_differs_says_how_under_each_request() {
    let mut document = conforming();
    let paths = document["paths"].as_object_mut().unwrap();
    paths.remove("/registry/v1/log/head");
    paths.insert(
        "/registry/v1/publish".to_string(),
        serde_json::json!({ "post": { "responses": {} } }),
    );
    let root = folder("differs");
    fs::write(
        root.join("main.json"),
        serde_json::to_vec(&document).unwrap(),
    )
    .unwrap();
    let out = jadpo(&root, &["main.json"]);
    assert_eq!(out.code, 1, "{}{}", out.stdout, out.stderr);
    let said = format!("{}{}", out.stdout, out.stderr);
    assert!(
        said.contains("error: `main.json` differs from the registry protocol"),
        "{said}"
    );
    assert!(said.contains("rule: protocol.mismatch"), "{said}");
    assert!(
        said.contains("GET /registry/v1/log/head\n  missing"),
        "{said}"
    );
    assert!(
        said.contains("POST /registry/v1/publish\n  not in the protocol"),
        "{said}"
    );
    assert!(
        said.contains("Differs from the registry protocol: 2 differences, 2 requests, main.json"),
        "{said}"
    );
    let json = jadpo(&root, &["main.json", "--json"]);
    assert_eq!(json.code, 1, "{}", json.stdout);
    let data: serde_json::Value = serde_json::from_str(&json.stdout).expect("JSON");
    assert_eq!(data["data"]["conforms"], false, "{data:#}");
    assert_eq!(
        data["data"]["mismatches"][0],
        serde_json::json!({
            "operation": "GET /registry/v1/log/head",
            "problem": "missing",
        }),
        "{data:#}"
    );
}

/// What can't be read isn't a difference, and an answer that needs a
/// sign-in says how a Jadpo application opens its document.
#[test]
fn a_document_that_cant_be_read_says_why() {
    let root = folder("unreadable");
    let said = |out: &Output| format!("{}{}", out.stdout, out.stderr);

    let missing = jadpo(&root, &["nowhere.json"]);
    assert_eq!(missing.code, 1, "{}", said(&missing));
    assert!(
        said(&missing).contains("rule: protocol.unreadable"),
        "{}",
        said(&missing)
    );

    fs::write(root.join("page.json"), "<html>").unwrap();
    let not_json = jadpo(&root, &["page.json"]);
    assert_eq!(not_json.code, 1, "{}", said(&not_json));
    assert!(
        said(&not_json).contains("It isn't JSON"),
        "{}",
        said(&not_json)
    );

    let shut = answering(401, r#"{"error":{"code":"unauthenticated"}}"#);
    let address = format!("http://{}/registry/v1/openapi.json", shut.address());
    let refused = jadpo(&root, &[&address]);
    assert_eq!(refused.code, 1, "{}", said(&refused));
    assert!(said(&refused).contains("401"), "{}", said(&refused));
    assert!(
        said(&refused).contains("`authentication: []`"),
        "{}",
        said(&refused)
    );

    let elsewhere = jadpo(&root, &["http://registry.example/openapi.json"]);
    assert_eq!(elsewhere.code, 1, "{}", said(&elsewhere));
    assert!(
        said(&elsewhere).contains("only on this machine"),
        "{}",
        said(&elsewhere)
    );
}

#[test]
fn it_takes_one_document() {
    let root = folder("arguments");
    let none = jadpo(&root, &[]);
    assert_eq!(none.code, 2, "{}{}", none.stdout, none.stderr);
    let two = jadpo(&root, &["a.json", "b.json"]);
    assert_eq!(two.code, 2, "{}{}", two.stdout, two.stderr);
}
