//! What an app's contract run sends the registry (area 11, "What a release
//! shows" and "The registry protocol"; chunk R10b): with `packages:`'s
//! `report_decode_failures: true`, an answer from the sandbox that a
//! package installed from the registry can no longer decode is sent to
//! `POST decode-failures` by its shape, and never a value of it; without
//! it, nothing is sent. The registry and the sandbox are both on this
//! machine: nothing reaches a network.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use jadpo_cli::{Output, run};
use jadpo_registry::http::{Incoming, Server};
use jadpo_registry::local::{LocalRegistry, ReleaseSeed};
use jadpo_registry::names::{ScopedName, Version};
use jadpo_registry::protocol::{ShapeKind, ShapeMember};
use jadpo_term::Environment;

use super::http::{tempdir, write};
use crate::support::fake_provider::FakeProvider;

/// What the sandbox answers: a booking whose `id` is now a number, with a
/// customer's details and a map keyed by a customer's id, none of which
/// may leave the machine.
const ANSWER: &str = r#"{"id":4242,"customer":{"email":"ada@example.com","name":"Ada Lovelace"},"by_user":{"usr_8812":"Ada"},"notes":["Leave with neighbour at 12 Mill Lane"]}"#;

/// Every value in [`ANSWER`], and the member names that are themselves
/// values.
const VALUES: &[&str] = &[
    "4242",
    "ada@example.com",
    "Ada Lovelace",
    "Ada",
    "usr_8812",
    "Leave with neighbour",
    "Mill Lane",
];

fn package_files(sandbox: &str) -> Vec<(&'static str, String)> {
    vec![
        (
            "package.jadpo",
            "/// Acme Couriers' booking API.\npackage AcmeCouriers.v1 {\n    publisher: \"@acme\"\n    licence: Licence.mit\n    jadpo: \"1.0\"\n}\n".into(),
        ),
        (
            "provider.jadpo",
            format!(
                "/// Acme Couriers' API.\nprovider {{\n    protocol: Protocol.json_http\n    hosts: {{\n        live: \"https://api.acme-couriers.example\"\n        sandbox: \"{sandbox}\"\n    }}\n    credential: CredentialScheme.bearer\n}}\n"
            ),
        ),
        (
            "types/booking_id.jadpo",
            "/// The courier's reference for a booking.\npublic type BookingId = Text {\n    max_length: 40\n}\n".into(),
        ),
        (
            "types/postcode.jadpo",
            "/// Where a collection is.\npublic type Postcode = Text {\n    max_length: 8\n}\n".into(),
        ),
        (
            "types/create_booking.jadpo",
            "/// What booking a collection takes.\npublic type CreateBooking = Object {\n    /// Where to collect from.\n    postcode: Postcode\n}\n".into(),
        ),
        (
            "types/booking.jadpo",
            "/// A booked collection.\npublic type Booking = Object {\n    /// The courier's reference.\n    id: BookingId\n}\n".into(),
        ),
        (
            "resources/bookings.jadpo",
            "/// Bookings.\npublic resource bookings {\n    /// Books a collection.\n    create: {\n        method: HttpMethod.post\n        path: \"/v1/bookings\"\n        input: CreateBooking\n        output: Booking\n        changes: true\n        source: PackageSource.written\n    }\n}\n".into(),
        ),
    ]
}

fn app_files(reports: Option<bool>) -> Vec<(&'static str, String)> {
    let packages = match reports {
        Some(on) => format!("    packages: {{\n        report_decode_failures: {on}\n    }}\n"),
        None => String::new(),
    };
    vec![
        (
            "app.jadpo",
            format!(
                "application {{\n{packages}    connections: {{\n        couriers: {{\n            package: AcmeCouriers.v1\n            domains: [shipping]\n            credential: config.couriers_api_key\n            environment: config.couriers_environment\n        }}\n    }}\n}}\n"
            ),
        ),
        (
            "config.jadpo",
            "config {\n    couriers_api_key: Text {\n        secret: true\n    }\n    couriers_environment: ProviderEnvironment {\n        default: ProviderEnvironment.sandbox\n    }\n}\n".into(),
        ),
        (
            "domains/shipping/actions/book.jadpo",
            "use app { couriers }\nuse packages/acme_couriers\n\n/// Books a collection with the courier.\naction book(postcode: AcmeCouriers.Postcode) -> AcmeCouriers.Booking {\n    return couriers.bookings.create(\n        AcmeCouriers.CreateBooking {\n            postcode: postcode\n        }\n    )\n}\n".into(),
        ),
        (
            "domains/shipping/actions/book.test.jadpo",
            "use packages/acme_couriers\n\ntest \"a collection is booked\" {\n    var booking: AcmeCouriers.Booking = book(\"N1 9GU\")\n    assert booking.id != \"\"\n}\n".into(),
        ),
        (".env.local", "COURIERS_API_KEY=sandbox_key_r10b\n".into()),
    ]
}

fn with(url: &str, cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    run(
        &args,
        &Environment::empty()
            .with_var("JADPO_REGISTRY", url)
            .with_cwd(cwd),
    )
}

fn said(out: &Output) -> String {
    format!("{}{}", out.stdout, out.stderr)
}

/// The package's files as a release holds them.
fn snapshot(folder: &Path) -> Vec<(String, Vec<u8>)> {
    jadpo_db::package_files(folder)
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(folder)
                .unwrap()
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            (relative, std::fs::read(&path).unwrap())
        })
        .collect()
}

struct World {
    _dir: tempdir::Dir,
    app: PathBuf,
    registry: LocalRegistry,
    _server: Server,
    url: String,
    /// Each request the registry got: its target and body.
    asked: Arc<Mutex<Vec<(String, Vec<u8>)>>>,
    sandbox: FakeProvider,
}

/// The courier's package 1.0.0 in a local registry, and an app that
/// installed it from there, its `packages:` as `reports` says.
fn world(name: &str, reports: Option<bool>) -> World {
    let sandbox = FakeProvider::start();
    let dir = tempdir::Dir::new(name);
    let package = dir.path().join("acme_couriers");
    for (path, text) in package_files(&sandbox.url) {
        write(&package, path, &text);
    }
    let checked = with(
        "http://127.0.0.1:9/registry/v1/",
        &package,
        &["package", "check"],
    );
    assert!(
        package.join("surface.jadpo_record").is_file(),
        "{}",
        said(&checked)
    );
    let registry = LocalRegistry::new();
    registry.publisher("@acme", true, false);
    let mut seed = ReleaseSeed::new(
        ScopedName::parse("@acme/acme-couriers").unwrap(),
        Version::parse("1.0.0").unwrap(),
        snapshot(&package),
    );
    seed.released_at = "2026-10-01T09:00:00Z".into();
    seed.surface = std::fs::read_to_string(package.join("surface.jadpo_record")).unwrap();
    registry.release(seed);
    let asked = Arc::new(Mutex::new(Vec::new()));
    let kept = asked.clone();
    let serving = registry.clone();
    let server = Server::start(Arc::new(move |incoming: Incoming| {
        kept.lock()
            .unwrap()
            .push((incoming.target.clone(), incoming.body.clone()));
        serving.handle(&incoming)
    }))
    .expect("serves");
    let url = format!("http://{}/registry/v1/", server.address());

    let app = dir.path().join("shop");
    for (path, text) in app_files(reports) {
        write(&app, path, &text);
    }
    let added = with(&url, &app, &["package", "add", "@acme/acme-couriers"]);
    assert_eq!(added.code, 0, "{}", said(&added));
    let updated = with(&url, &app, &["records", "update"]);
    assert_eq!(updated.code, 0, "{}", said(&updated));
    asked.lock().unwrap().clear();
    World {
        _dir: dir,
        app,
        registry,
        _server: server,
        url,
        asked,
        sandbox,
    }
}

fn reports_sent(world: &World) -> Vec<Vec<u8>> {
    world
        .asked
        .lock()
        .unwrap()
        .iter()
        .filter(|(target, _)| target.contains("decode-failures"))
        .map(|(_, body)| body.clone())
        .collect()
}

#[test]
fn an_answer_a_package_cant_decode_is_reported_by_its_shape_and_no_value() {
    let world = world("contract-reports-on", Some(true));
    world.sandbox.answer(200, ANSWER);
    let out = with(
        &world.url,
        &world.app,
        &["test", "--kind", "contract", "--json", "--seed", "1"],
    );
    // The case fails: the answer doesn't decode.
    assert_eq!(out.code, 1, "{}", said(&out));
    assert_eq!(world.sandbox.requests().len(), 1);

    // One report, holding the package, its version, the operation and the
    // answer's shape.
    let received = world.registry.decode_failures();
    assert_eq!(received.len(), 1, "{received:?}");
    let report = &received[0];
    assert_eq!(report.package.to_string(), "@acme/acme-couriers");
    assert_eq!(report.version.to_string(), "1.0.0");
    assert_eq!(report.operation, "bookings.create");
    let of = |path: &str, kind: ShapeKind| ShapeMember {
        path: path.into(),
        kind,
    };
    for member in [
        of("$", ShapeKind::Object),
        of("$.id", ShapeKind::Number),
        of("$.customer", ShapeKind::Object),
        of("$.customer.email", ShapeKind::Text),
        of("$.by_user.*", ShapeKind::Text),
        of("$.notes", ShapeKind::List),
        of("$.notes[]", ShapeKind::Text),
    ] {
        assert!(report.shape.contains(&member), "{member:?} in {report:?}");
    }

    // What went over the wire holds no value of the answer, and no other
    // field than the protocol's.
    let sent = reports_sent(&world);
    assert_eq!(sent.len(), 1);
    let body = String::from_utf8(sent[0].clone()).unwrap();
    for value in VALUES {
        assert!(!body.contains(value), "{value} left in {body}");
    }
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let mut keys: Vec<&str> = json
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["operation", "package", "shape", "version"]);
    // Nor did anything else of the run reach the registry.
    assert_eq!(world.asked.lock().unwrap().len(), 1);

    let document: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
    let reported = &document["data"]["contract"]["reported"];
    assert_eq!(reported[0]["package"], "@acme/acme-couriers");
    assert_eq!(reported[0]["version"], "1.0.0");
    assert_eq!(reported[0]["operation"], "bookings.create");
    assert!(reported[0]["id"].as_str().is_some_and(|id| !id.is_empty()));

    // The screen counts it in the footer.
    world.sandbox.answer(200, ANSWER);
    let out = with(
        &world.url,
        &world.app,
        &["test", "--kind", "contract", "--seed", "1"],
    );
    assert_eq!(out.code, 1, "{}", said(&out));
    let footer = out
        .stdout
        .lines()
        .find(|line| line.starts_with("test failed:"))
        .unwrap_or_else(|| panic!("{}", said(&out)));
    assert!(
        footer.starts_with("test failed: 1 failed, 1 contract case, 1 decode failure reported"),
        "{footer}"
    );
    for value in VALUES {
        assert!(
            !world
                .registry
                .decode_failures()
                .iter()
                .any(|report| serde_json::to_string(report).unwrap().contains(value)),
            "{value}"
        );
    }
}

/// Off unless the app turns it on: left out or `false`, the same failure
/// sends nothing at all.
#[test]
fn an_app_that_doesnt_opt_in_sends_nothing() {
    for (name, reports) in [
        ("contract-reports-unset", None),
        ("contract-reports-off", Some(false)),
    ] {
        let world = world(name, reports);
        world.sandbox.answer(200, ANSWER);
        let out = with(
            &world.url,
            &world.app,
            &["test", "--kind", "contract", "--json", "--seed", "1"],
        );
        assert_eq!(out.code, 1, "{}", said(&out));
        assert_eq!(world.sandbox.requests().len(), 1, "{name}");
        assert!(world.registry.decode_failures().is_empty(), "{name}");
        assert!(world.asked.lock().unwrap().is_empty(), "{name}");
        let document: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(
            document["data"]["contract"]["reported"],
            serde_json::json!([]),
            "{name}"
        );
    }
}

/// An answer that decodes is no failure, and sends nothing.
#[test]
fn an_answer_that_decodes_sends_nothing() {
    let world = world("contract-reports-decodes", Some(true));
    world.sandbox.answer(200, r#"{"id":"bk_sandbox_1"}"#);
    let out = with(
        &world.url,
        &world.app,
        &["test", "--kind", "contract", "--seed", "1"],
    );
    assert_eq!(out.code, 0, "{}", said(&out));
    assert!(world.asked.lock().unwrap().is_empty());
}

/// A registry that can't be told says so in one alert line, and the run's
/// own result stands.
#[test]
fn a_registry_out_of_reach_is_one_alert() {
    let world = world("contract-reports-unreached", Some(true));
    world.sandbox.answer(200, ANSWER);
    let out = with(
        "http://127.0.0.1:9/registry/v1/",
        &world.app,
        &["test", "--kind", "contract", "--seed", "1"],
    );
    assert_eq!(out.code, 1, "{}", said(&out));
    assert!(
        out.stdout.contains(
            "alert: The registry wasn't told of 1 answer a package couldn't decode: it couldn't be reached\n"
        ),
        "{}",
        said(&out)
    );
    assert!(
        !out.stdout.contains("decode failure reported"),
        "{}",
        out.stdout
    );
}
