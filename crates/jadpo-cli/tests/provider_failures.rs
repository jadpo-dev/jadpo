//! A provider's failures read by status and text (area 11, "A provider's
//! failures"; Richard, 2026-11-07), and a JSON body that leaves out an
//! optional field that is `none` (area 11, "How provider calls are made"),
//! both found building postcodes.io's package: its refusals are a status
//! and a sentence, `{"status": 404, "error": "Postcode not found"}`, and
//! it refuses a `null` radius or limit with `400`.
//!
//! Each test compiles an app with one connection to a postcodes package
//! whose sandbox is a provider started in the test on this machine, run by
//! the interpreter over the package driver and the native host's HTTP.

use std::path::Path;
use std::rc::Rc;

use jadpo_drivers::package::JsonHttp;
use jadpo_host_native::Runtime;
use jadpo_host_native::fetch::NativeFetch;
use jadpo_interp::{Executor, Outcome, Prepared, Program};
use jadpo_runtime::capability::{Capability, Faked};
use jadpo_runtime::{Clock, Ctx, Env, FaultKind, MemoryHost};
use jadpo_value::Value;

use crate::support::fake_provider::FakeProvider;

const PACKAGE: &str = "shared/providers/postcodes_io/v1";

fn files(sandbox: &str) -> Vec<(String, String)> {
    let package = |path: &str, text: &str| (format!("{PACKAGE}/{path}"), text.to_string());
    vec![
        (
            "app.jadpo".into(),
            "application {\n    connections: {\n        postcodes: {\n            package: PostcodesIo.v1\n            domains: [addresses]\n            environment: config.postcodes_environment\n        }\n    }\n}\n".into(),
        ),
        (
            "config.jadpo".into(),
            "config {\n    postcodes_environment: ProviderEnvironment\n}\n".into(),
        ),
        package(
            "package.jadpo",
            "/// postcodes.io's UK postcode lookups.\npackage PostcodesIo.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n",
        ),
        package(
            "provider.jadpo",
            &format!(
                "/// postcodes.io's public API.\nprovider {{\n    protocol: Protocol.json_http\n    hosts: {{\n        live: \"https://api.postcodes.io\"\n        sandbox: \"{sandbox}\"\n    }}\n    error_code: \"error\"\n}}\n"
            ),
        ),
        package(
            "types/postcode.jadpo",
            "/// A postcode as someone typed it.\npublic type Postcode = Text {\n    max_length: 20\n}\n",
        ),
        package(
            "types/postcode_lookup.jadpo",
            "/// Which postcode to look up.\npublic type PostcodeLookup = Object {\n    postcode: Postcode\n}\n",
        ),
        package(
            "types/postcode_answer.jadpo",
            "/// A postcode postcodes.io holds.\npublic type PostcodeAnswer = Object {\n    postcode: Postcode\n}\n",
        ),
        package(
            "types/geolocation.jadpo",
            "/// A grid square to find postcodes near, and how far to look.\npublic type Geolocation = Object {\n    square: Text\n    radius: Int? {\n        wire: Wire.number\n    }\n    limit: Int? {\n        wire: Wire.number\n    }\n}\n",
        ),
        package(
            "types/near_points.jadpo",
            "/// The points to find postcodes near.\npublic type NearPoints = Object {\n    geolocations: List<Geolocation>\n    wide: Bool?\n}\n",
        ),
        package(
            "types/near_answer.jadpo",
            "/// How many points were looked up.\npublic type NearAnswer = Object {\n    status: Int\n}\n",
        ),
        package(
            "failures/postcode_not_found.jadpo",
            "/// postcodes.io holds no such postcode.\npublic type PostcodeNotFound = Failure {\n    kind: FailureKind.not_found\n    wire_status: 404\n    wire_name: \"Postcode not found\"\n}\n",
        ),
        package(
            "failures/invalid_postcode.jadpo",
            "/// The text isn't a postcode.\npublic type InvalidPostcode = Failure {\n    kind: FailureKind.invalid_value\n    wire_name: \"Invalid postcode\"\n}\n",
        ),
        package(
            "failures/not_found.jadpo",
            "/// postcodes.io found nothing there.\npublic type NotFound = Failure {\n    kind: FailureKind.not_found\n    wire_status: 404\n}\n",
        ),
        package(
            "resources/postcodes.jadpo",
            "public resource postcodes {\n    get: {\n        method: HttpMethod.get\n        path: \"/postcodes/{postcode}\"\n        input: PostcodeLookup\n        output: PostcodeAnswer\n        changes: false\n        failures: [PostcodeNotFound, InvalidPostcode, NotFound]\n        source: PackageSource.written\n    }\n    near: {\n        method: HttpMethod.post\n        path: \"/postcodes\"\n        input: NearPoints\n        output: NearAnswer\n        changes: false\n        source: PackageSource.written\n    }\n}\n",
        ),
        (
            "domains/addresses/failures/address_unknown.jadpo".into(),
            "type AddressUnknown = Failure {\n    kind: FailureKind.not_found\n}\n".into(),
        ),
        (
            "domains/addresses/failures/address_unreadable.jadpo".into(),
            "type AddressUnreadable = Failure {\n    kind: FailureKind.invalid_value\n}\n".into(),
        ),
        (
            "domains/addresses/failures/area_unknown.jadpo".into(),
            "type AreaUnknown = Failure {\n    kind: FailureKind.not_found\n}\n".into(),
        ),
        (
            "domains/addresses/actions/look_up.jadpo".into(),
            "use app { postcodes }\nuse addresses/failures { AddressUnknown, AddressUnreadable, AreaUnknown }\nuse shared/providers/postcodes_io\n\n/// Where a postcode is.\naction look_up(postcode: PostcodesIo.Postcode) fails AddressUnknown, AddressUnreadable, AreaUnknown -> PostcodesIo.PostcodeAnswer {\n    return attempt postcodes.postcodes.get(PostcodesIo.PostcodeLookup {\n        postcode: postcode\n    }) {\n        postcode_not_found: AddressUnknown\n        invalid_postcode: AddressUnreadable\n        not_found: AreaUnknown\n    }\n}\n".into(),
        ),
        (
            "domains/addresses/actions/near.jadpo".into(),
            "use app { postcodes }\nuse shared/providers/postcodes_io\n\n/// Postcodes near one square, as far as postcodes.io looks by default.\naction near() -> PostcodesIo.NearAnswer {\n    return postcodes.postcodes.near(PostcodesIo.NearPoints {\n        geolocations: [\n            PostcodesIo.Geolocation {\n                square: \"TQ3080\"\n                radius: none\n                limit: 5\n            }\n        ]\n        wide: none\n    })\n}\n".into(),
        ),
    ]
}

fn program(root: &Path, sandbox: &str) -> Program {
    let _ = std::fs::remove_dir_all(root);
    for (path, text) in files(sandbox) {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        std::fs::write(path, text).expect("write");
    }
    let loaded = jadpo_db::load(root);
    let prepared = Prepared::load(&loaded.db, loaded.project).unwrap_or_else(|problems| {
        panic!(
            "the project checks: {:?}",
            problems.iter().map(|p| &p.message).collect::<Vec<_>>()
        )
    });
    let variables = |name: &str| -> Option<String> {
        (name == "POSTCODES_ENVIRONMENT").then(|| "sandbox".to_string())
    };
    let values = prepared
        .config(
            &variables,
            jadpo_runtime::config::Mode::Environment,
            &|_: &mut [u8]| false,
        )
        .unwrap_or_else(|problems| panic!("the settings read: {problems:?}"));
    Program::new(prepared).with_config(&values)
}

fn root(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("jadpo-provider-failures-{name}-{}", std::process::id()))
}

fn host() -> Faked<MemoryHost> {
    let driver: Rc<dyn Capability> = Rc::new(JsonHttp::new(NativeFetch::new()));
    Faked::new(MemoryHost::new()).with("postcodes", driver)
}

fn env() -> Env {
    Env {
        seed: Some(7),
        clock: Clock::TEST,
    }
}

fn run(runtime: &Runtime, program: &Program, name: &str, args: Vec<Value>) -> Outcome {
    let host = host();
    let mut ctx = Ctx::of(program, &host, env());
    let id = program.plan().find(name).expect("the action");
    runtime.block_on(program.run(&mut ctx, id, args))
}

/// Each refusal is the operation's most specific failure, mapped at the
/// call: a status and text, then a text at any status, then a status
/// alone; one nothing matches is a fault naming what the provider sent.
#[test]
fn a_refusal_is_read_by_its_status_and_its_text() {
    let provider = FakeProvider::start();
    let program = program(&root("refusals"), &provider.url);
    let runtime = Runtime::new().expect("a runtime");
    provider.answer(404, r#"{"status": 404, "error": "Postcode not found"}"#);
    provider.answer(404, r#"{"status": 404, "error": "Invalid postcode"}"#);
    provider.answer(400, r#"{"status": 400, "error": "Invalid postcode"}"#);
    provider.answer(404, r#"{"status": 404, "error": "Outcode not found"}"#);
    provider.answer(400, r#"{"status": 400, "error": "Postcode not found"}"#);
    let mut outcomes = Vec::new();
    for postcode in ["ZZ1 1ZZ", "SW1A", "SW1A", "ZZ1", "ZZ1 1ZZ"] {
        outcomes.push(run(&runtime, &program, "look_up", vec![Value::text(postcode)]));
    }
    let failed = |outcome: &Outcome| match outcome {
        Outcome::Failed(failure) => (
            failure.code.clone(),
            failure.cause.as_ref().map(|cause| cause.code.clone()),
        ),
        other => panic!("{other:?}"),
    };
    let pair = |app: &str, package: &str| (app.to_string(), Some(package.to_string()));
    assert_eq!(
        failed(&outcomes[0]),
        pair("address_unknown", "postcode_not_found")
    );
    assert_eq!(
        failed(&outcomes[1]),
        pair("address_unreadable", "invalid_postcode")
    );
    assert_eq!(
        failed(&outcomes[2]),
        pair("address_unreadable", "invalid_postcode")
    );
    assert_eq!(failed(&outcomes[3]), pair("area_unknown", "not_found"));
    match &outcomes[4] {
        Outcome::Fault(fault) => match &fault.kind {
            FaultKind::Provider(what) => assert!(
                what.contains("`Postcode not found`, which it doesn't declare"),
                "{what}"
            ),
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    }
}

/// An optional field that is `none` is left out of a JSON body, at any
/// depth, rather than sent as `null`.
#[test]
fn a_body_leaves_out_what_is_none() {
    let provider = FakeProvider::start();
    let program = program(&root("body"), &provider.url);
    let runtime = Runtime::new().expect("a runtime");
    provider.answer(200, r#"{"status": 200}"#);
    let outcome = run(
        &runtime,
        &program,
        "near",
        Vec::new(),
    );
    assert!(matches!(outcome, Outcome::Value(_)), "{outcome:?}");
    let sent = provider.requests();
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0].json(),
        serde_json::json!({
            "geolocations": [{"square": "TQ3080", "limit": 5}]
        })
    );
}
