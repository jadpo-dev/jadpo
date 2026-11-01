//! The conformance test (area 11, "The registry protocol"): a served
//! OpenAPI document checked against the protocol's table. It runs here
//! against the local registry, built from the same table, and against
//! every way a registry application's document could drift from it. With
//! `JADPO_REGISTRY_OPENAPI` set to a file or an `http://` address it
//! checks that document too: the registry application's CI runs it so.

use jadpo_registry::conformance::{Mismatch, check, load};
use jadpo_registry::http::fetch;
use jadpo_registry::local::LocalRegistry;
use jadpo_registry::protocol::Method;
use serde_json::{Value, json};

fn served() -> Value {
    let registry = LocalRegistry::new();
    let served = registry.serve().expect("serves");
    let answer =
        fetch(Method::Get, &format!("{}openapi.json", served.url()), None).expect("answers");
    assert_eq!(answer.status, 200);
    serde_json::from_slice(&answer.body).expect("JSON")
}

fn problems(document: &Value) -> Vec<String> {
    check(document).iter().map(Mismatch::to_string).collect()
}

/// `document` changed by `edit`, and the one mismatch that must name.
fn drifts(edit: impl FnOnce(&mut Value), expected: &str) {
    let mut document = served();
    edit(&mut document);
    let found = problems(&document);
    assert!(
        found.iter().any(|p| p.contains(expected)),
        "expected a mismatch containing {expected:?}, found {found:#?}"
    );
}

fn op<'a>(document: &'a mut Value, path: &str, method: &str) -> &'a mut Value {
    &mut document["paths"][format!("/registry/v1/{path}")][method]
}

fn component<'a>(document: &'a mut Value, name: &str) -> &'a mut Value {
    &mut document["components"]["schemas"][name]
}

#[test]
fn the_local_registrys_served_document_conforms() {
    let document = served();
    assert_eq!(document["openapi"], "3.1.0");
    assert_eq!(problems(&document), Vec::<String>::new());
}

#[test]
fn a_document_set_from_outside_conforms() {
    let Ok(source) = std::env::var("JADPO_REGISTRY_OPENAPI") else {
        return;
    };
    let document = load(&source).unwrap_or_else(|e| panic!("{source}: {e}"));
    let found = problems(&document);
    assert!(
        found.is_empty(),
        "{source} drifts from the protocol:\n{}",
        found.join("\n")
    );
}

#[test]
fn a_document_is_loaded_from_a_file_or_a_served_address() {
    let registry = LocalRegistry::new();
    let served_registry = registry.serve().unwrap();
    let from_url = load(&format!("{}openapi.json", served_registry.url())).expect("fetched");
    let file = std::env::temp_dir().join(format!(
        "jadpo-registry-openapi-{}.json",
        std::process::id()
    ));
    std::fs::write(&file, serde_json::to_vec(&from_url).unwrap()).unwrap();
    let from_file = load(file.to_str().unwrap()).expect("read");
    std::fs::remove_file(&file).unwrap();
    assert_eq!(from_file, from_url);
    assert!(
        load("https://jadpo.dev/registry/v1/openapi.json").is_err(),
        "only plain HTTP on this machine is fetched"
    );
    assert!(load("/no/such/file.json").is_err());
}

#[test]
fn a_missing_or_extra_operation_is_named() {
    drifts(
        |d| {
            d["paths"]
                .as_object_mut()
                .unwrap()
                .remove("/registry/v1/log/head");
        },
        "GET /registry/v1/log/head: missing",
    );
    drifts(
        |d| {
            let get = d["paths"]["/registry/v1/log/head"]["get"].take();
            d["paths"]["/registry/v1/log/head"] = json!({ "post": get });
        },
        "POST /registry/v1/log/head: not in the protocol",
    );
    drifts(
        |d| {
            d["paths"]["/registry/v1/publish"] = json!({ "post": { "responses": {} } });
        },
        "POST /registry/v1/publish: not in the protocol",
    );
    drifts(
        |d| {
            let paths = d["paths"].as_object_mut().unwrap();
            let op = paths
                .remove("/registry/v1/packages/{publisher}/{name}")
                .unwrap();
            paths.insert("/registry/v1/packages/{scope}/{name}".into(), op);
        },
        "GET /registry/v1/packages/{publisher}/{name}: missing",
    );
}

#[test]
fn an_answer_that_differs_is_named_by_its_field() {
    drifts(
        |d| component(d, "LogHead")["properties"]["size"] = json!({ "type": "string" }),
        "answer.size: string where the protocol has an integer",
    );
    drifts(
        |d| {
            component(d, "LogHead")["properties"]
                .as_object_mut()
                .unwrap()
                .remove("root");
        },
        "answer.root: missing",
    );
    drifts(
        |d| {
            component(d, "LogHead")["properties"]["signature"] = json!({ "type": "string" });
            component(d, "LogHead")["required"]
                .as_array_mut()
                .unwrap()
                .push(json!("signature"));
        },
        "answer.signature: not in the protocol",
    );
    drifts(
        |d| {
            let required = component(d, "LogHead")["required"].as_array_mut().unwrap();
            required.retain(|r| r != "root");
        },
        "answer.root: may be left out",
    );
    drifts(
        |d| {
            component(d, "ReleaseSummary")["properties"]["yanked"] =
                json!({ "type": ["boolean", "null"] })
        },
        "answer.releases[].yanked: may be null",
    );
    drifts(
        |d| component(d, "Package")["properties"]["successor"] = json!({ "type": "string" }),
        "answer.successor: never null where the protocol allows null",
    );
    drifts(
        |d| {
            component(d, "ReleaseSummary")["properties"]["released_at"] =
                json!({ "type": "string" })
        },
        "answer.releases[].released_at: not a date-time",
    );
    drifts(
        |d| {
            component(d, "ReleaseRequest")["properties"]["state"] =
                json!({ "type": "string", "enum": ["verifying", "failed", "live"] })
        },
        "answer.state: values",
    );
    drifts(
        |d| component(d, "Package")["properties"]["releases"] = json!({ "type": "object" }),
        "answer.releases: object where the protocol has a list",
    );
}

#[test]
fn inputs_that_differ_are_named() {
    drifts(
        |d| {
            let params = op(d, "search", "get")["parameters"].as_array_mut().unwrap();
            params.retain(|p| p["name"] != "kind");
        },
        "GET /registry/v1/search: query.kind: missing",
    );
    drifts(
        |d| {
            let params = op(d, "search", "get")["parameters"].as_array_mut().unwrap();
            for p in params.iter_mut() {
                if p["name"] == "from" {
                    p["required"] = json!(true);
                }
            }
        },
        "query.from: required where the protocol leaves it out",
    );
    drifts(
        |d| {
            op(d, "log/head", "get")["parameters"] = json!([{ "name": "since", "in": "query", "required": true, "schema": { "type": "integer" } }]);
        },
        "GET /registry/v1/log/head: query.since: not in the protocol",
    );
    drifts(
        |d| {
            let schema = component(d, "StartRelease");
            schema["properties"]
                .as_object_mut()
                .unwrap()
                .remove("dry_run");
        },
        "POST /registry/v1/releases: body.dry_run: missing",
    );
    drifts(
        |d| {
            op(d, "releases", "post")
                .as_object_mut()
                .unwrap()
                .remove("requestBody");
        },
        "POST /registry/v1/releases: body: missing",
    );
    drifts(
        |d| {
            let params = op(d, "archives/{digest}", "get")["parameters"]
                .as_array_mut()
                .unwrap();
            params[0]["schema"] = json!({ "type": "integer" });
        },
        "path.digest: integer where the protocol has text",
    );
}

#[test]
fn statuses_failures_and_sign_in_are_checked() {
    drifts(
        |d| {
            let responses = op(d, "releases", "post")["responses"]
                .as_object_mut()
                .unwrap();
            let accepted = responses.remove("202").unwrap();
            responses.insert("201".into(), accepted);
        },
        "POST /registry/v1/releases: answers 202",
    );
    drifts(
        |d| {
            op(d, "packages/{publisher}/{name}", "get")["responses"]
                .as_object_mut()
                .unwrap()
                .remove("404");
        },
        "fails with `package_not_found` (404)",
    );
    drifts(
        |d| {
            let schema = &mut op(d, "log/consistency", "get")["responses"]["422"]["content"]["application/json"]
                ["schema"];
            *schema = json!({ "type": "object" });
        },
        "fails with `log_size_ahead` (422)",
    );
    drifts(
        |d| op(d, "log/head", "get")["security"] = json!([{ "session": [] }]),
        "GET /registry/v1/log/head: needs a sign-in",
    );
    drifts(
        |d| {
            op(d, "log/head", "get")
                .as_object_mut()
                .unwrap()
                .remove("security");
            d["security"] = json!([{ "session": [] }]);
        },
        "GET /registry/v1/log/head: needs a sign-in",
    );
    drifts(
        |d| {
            let ok = &mut op(d, "archives/{digest}", "get")["responses"]["200"]["content"];
            *ok = json!({ "application/json": { "schema": { "type": "string" } } });
        },
        "GET /registry/v1/archives/{digest}: answers application/x-tar",
    );
}

#[test]
fn the_forms_a_jadpo_application_writes_are_read() {
    let mut document = served();
    // Null as a type list rather than `anyOf`, every schema behind one
    // more `$ref`, the compiler's own keywords, the runtime's shared
    // failures, and two failures at one status as `oneOf`.
    document["components"]["schemas"]["Package"]["properties"]["successor"] =
        json!({ "type": ["string", "null"], "x-jadpo-constraints": "ScopedName?" });
    document["components"]["schemas"]["LogHeadAlias"] =
        json!({ "$ref": "#/components/schemas/LogHead" });
    document["paths"]["/registry/v1/log/head"]["get"]["responses"]["200"]["content"]["application/json"]
        ["schema"] =
        json!({ "$ref": "#/components/schemas/LogHeadAlias", "x-jadpo-constraints": "LogHead" });
    document["paths"]["/registry/v1/log/head"]["get"]["responses"]["429"] =
        json!({ "$ref": "#/components/responses/RateLimited" });
    document["paths"]["/registry/v1/log/head"]["get"]["x-jadpo-changes"] = json!(false);
    document["paths"]["/registry/v1/releases"]["post"]["parameters"] = json!([
        { "name": "Idempotency-Key", "in": "header", "required": true, "schema": { "type": "string" } }
    ]);
    document["paths"]["/registry/v1/openapi.json"] = json!({ "get": { "responses": {} } });
    assert_eq!(problems(&document), Vec::<String>::new());
}

#[test]
fn a_document_that_is_not_openapi_is_refused() {
    assert!(!problems(&json!({})).is_empty());
    assert!(!problems(&json!({ "openapi": "3.0.3", "paths": {} })).is_empty());
    let mut looping = served();
    looping["components"]["schemas"]["LogHead"] = json!({ "$ref": "#/components/schemas/LogHead" });
    assert!(
        problems(&looping).iter().any(|p| p.contains("$ref")),
        "{:?}",
        problems(&looping)
    );
}
