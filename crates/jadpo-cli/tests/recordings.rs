//! A provider package's recordings (area 11, "Provenance and
//! confirmation" and "Provider calls in development"; chunk 7.2d2): every
//! decoder run over every recording of the app's own integrations in
//! `jadpo test` and the publishing bar, and `jadpo dev`'s fake answering
//! from an operation's recording with its placeholders filled.
//!
//! The project has no tables, so nothing here needs PostgreSQL, and
//! nothing reaches a network.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

use crate::http::{Dev, wait_for};
use crate::support;

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("recordings-7.2d2-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temporary folder");
    dir
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut output = run(&args, &Environment::empty().with_cwd(cwd));
    output.stdout = support::without_run_time(&output.stdout);
    output
}

const PACKAGE: &str = "shared/providers/acme_couriers/v1";

/// The courier's own integration, in `root/<at>`: `bookings.create`
/// answers a `Booking` in a `data` envelope, its collection time spelt
/// `collectAt`, its window sent as the courier's `AM` or `PM` and read by
/// the package's own `decode:` and `encode:` (which also reads `EVE`, so
/// it doesn't give back what it read), and its status one of two
/// spellings.
fn integration(root: &Path, at: &str) {
    let file = |path: &str, text: &str| write(root, &format!("{at}/{path}"), text);
    file(
        "package.jadpo",
        "/// Acme Couriers' booking API.\npackage AcmeCouriers.v1 {\n    licence: Licence.mit\n    jadpo: \"1.0\"\n}\n",
    );
    file(
        "provider.jadpo",
        "/// Acme Couriers' API.\nprovider {\n    protocol: Protocol.json_http\n    hosts: {\n        live: \"https://api.acme-couriers.example\"\n        sandbox: \"https://sandbox.acme-couriers.example\"\n    }\n    credential: CredentialScheme.bearer\n    idempotency: {\n        header: \"Idempotency-Key\"\n        window: 24h\n    }\n    error_code: \"error.code\"\n}\n",
    );
    file(
        "resources/bookings.jadpo",
        "public resource bookings {\n    create: {\n        method: HttpMethod.post\n        path: \"/v1/bookings\"\n        input: CreateBooking\n        output: Booking\n        output_at: \"data\"\n        changes: true\n        idempotent: true\n        failures: [SlotTaken]\n        source: PackageSource.written\n        docs: \"https://docs.acme-couriers.example/bookings\"\n    }\n}\n",
    );
    file(
        "types/postcode.jadpo",
        "/// Where a collection is.\npublic type Postcode = Text {\n    max_length: 8\n}\n",
    );
    file(
        "types/create_booking.jadpo",
        "/// What booking a collection takes.\npublic type CreateBooking = Object {\n    postcode: Postcode\n}\n",
    );
    file(
        "types/status.jadpo",
        "/// Where a booking stands.\npublic type Status = Enum {\n    booked\n    collected\n}\n",
    );
    file(
        "types/window_code.jadpo",
        "/// A collection window as the courier writes it.\npublic type WindowCode = Text {\n    max_length: 3\n}\n",
    );
    file(
        "types/slot_window.jadpo",
        "/// When in the day a collection comes.\npublic type SlotWindow = Enum {\n    morning\n    afternoon\n}\n",
    );
    file(
        "types/booking.jadpo",
        "/// A booked collection.\npublic type Booking = Object {\n    id: Text {\n        max_length: 40\n    }\n    collect_at: Instant {\n        wire_name: \"collectAt\"\n    }\n    status: Status\n    window: SlotWindow {\n        decode: slots.parse_window\n        encode: slots.format_window\n    }\n}\n",
    );
    file(
        "failures/slot_taken.jadpo",
        "/// The slot went before the booking arrived.\npublic type SlotTaken = Failure {\n    kind: FailureKind.conflict\n    message: \"That slot is taken.\"\n}\n",
    );
    file(
        "failures/window_unknown.jadpo",
        "/// A collection window the courier names that this package doesn't know.\ntype WindowUnknown = Failure {\n    kind: FailureKind.invalid_value\n    message: \"The courier named a window this package doesn't know.\"\n}\n",
    );
    file(
        "libraries/slots.jadpo",
        r#"/// The courier's collection windows, which it writes as "AM" and "PM".
library slots {
    /// The window the courier's code names.
    function parse_window(code: WindowCode) fails WindowUnknown -> SlotWindow {
        if (code == "AM") {
            return SlotWindow.morning
        }
        if (code == "PM" or code == "EVE") {
            return SlotWindow.afternoon
        }
        reject WindowUnknown
    }

    /// The courier's code for a window.
    function format_window(window: SlotWindow) -> WindowCode {
        if (window == SlotWindow.morning) {
            return "AM"
        }
        return "PM"
    }
}
"#,
    );
}

/// A recording of `bookings.create` as `jadpo package record` keeps one:
/// its plain values placeholders, its status and window as sent.
fn recording(status: &str, window: &str) -> String {
    format!(
        "{{\n  \"data\": {{\n    \"collectAt\": \"{{Instant}}\",\n    \"id\": \"{{Text}}\",\n    \"status\": \"{status}\",\n    \"window\": \"{window}\"\n  }}\n}}\n"
    )
}

/// A courier desk whose route books a collection through its own
/// integration, with `recordings` (each a name and its text) in the
/// integration's `recordings/`.
fn desk(name: &str, recordings: &[(&str, String)]) -> PathBuf {
    let root = folder(name);
    write(
        &root,
        "app.jadpo",
        "application {\n    connections: {\n        couriers: {\n            package: AcmeCouriers.v1\n            domains: [shipping]\n            credential: config.couriers_api_key\n            environment: config.couriers_environment\n        }\n    }\n}\n",
    );
    write(
        &root,
        "config.jadpo",
        "config {\n    couriers_api_key: Text {\n        secret: true\n    }\n    couriers_environment: ProviderEnvironment {\n        default: ProviderEnvironment.sandbox\n    }\n}\n",
    );
    write(
        &root,
        "policy.jadpo",
        "use shipping\n\npolicy {\n    actions: {\n        shipping.book: [Access.public]\n    }\n}\n",
    );
    integration(&root, PACKAGE);
    for (file, text) in recordings {
        write(&root, &format!("{PACKAGE}/recordings/{file}"), text);
    }
    write(
        &root,
        "domains/shipping/failures/slot_gone.jadpo",
        "type SlotGone = Failure {\n    kind: FailureKind.conflict\n    message: \"That slot has gone; choose another.\"\n}\n",
    );
    write(
        &root,
        "domains/shipping/types/booking_form.jadpo",
        "use shared/providers/acme_couriers\n\n/// What the desk sends.\ntype BookingForm = Input {\n    postcode: AcmeCouriers.Postcode\n}\n",
    );
    write(
        &root,
        "domains/shipping/types/booked.jadpo",
        "use shared/providers/acme_couriers\n\n/// A booked collection, as the desk shows it.\ntype Booked = Output {\n    id: Text\n    status: AcmeCouriers.Status\n}\n",
    );
    write(
        &root,
        "domains/shipping/actions/book.jadpo",
        "use app { couriers }\nuse shipping/failures { SlotGone }\nuse shipping/types { Booked, BookingForm }\nuse shared/providers/acme_couriers\n\n/// Books a collection.\naction book(form: BookingForm) fails SlotGone -> Booked {\n    var booking: AcmeCouriers.Booking = attempt couriers.bookings.create(AcmeCouriers.CreateBooking {\n        postcode: form.postcode\n    }) {\n        slot_taken: SlotGone\n    }\n    return Booked {\n        id: booking.id\n        status: booking.status\n    }\n}\n",
    );
    write(
        &root,
        "domains/shipping/routes/bookings.jadpo",
        "use shipping/actions { book }\nuse shipping/types { Booked, BookingForm }\n\n/// Booking collections.\nroutes Bookings {\n    prefix: /bookings\n\n    /// Anyone at the desk books one.\n    POST / {\n        authentication: RouteAuthentication.none\n        input: BookingForm\n        output: Booked\n        status: SuccessStatus.created\n        run: book(input)\n    }\n}\n",
    );
    let updated = jadpo(&root, &["api", "update"]);
    assert_eq!(updated.code, 0, "{}{}", updated.stdout, updated.stderr);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    root
}

fn tests_of(output: &Output) -> Vec<serde_json::Value> {
    let document: serde_json::Value = serde_json::from_str(&output.stdout).expect("JSON");
    document["data"]["tests"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// `jadpo test` reads every recording of the app's own integrations as
/// its operation's output, its placeholders filled, through the
/// package's own decoders: one counted as a recording beside the tests.
#[test]
fn jadpo_test_decodes_every_recording() {
    let root = desk(
        "decoded",
        &[("bookings.create.json", recording("booked", "PM"))],
    );
    let ran = jadpo(&root, &["test", "--json", "--seed", "1"]);
    assert_eq!(ran.code, 0, "{}{}", ran.stdout, ran.stderr);
    let tests = tests_of(&ran);
    assert_eq!(tests.len(), 1, "{tests:?}");
    assert_eq!(tests[0]["kind"], "recording");
    assert_eq!(tests[0]["name"], "bookings.create's recording decodes");
    assert_eq!(
        tests[0]["file"],
        "shared/providers/acme_couriers/v1/recordings/bookings.create.json"
    );
    assert_eq!(tests[0]["outcome"], "passed");
    let plain = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(plain.stdout, "tested: 1 passed, 1 recording test\n");
    // One recording runs alone by its file.
    let alone = jadpo(
        &root,
        &[
            "test",
            "shared/providers/acme_couriers/v1/recordings/bookings.create.json",
            "--seed",
            "1",
        ],
    );
    assert_eq!(alone.code, 0, "{}{}", alone.stdout, alone.stderr);
}

/// A recording a decoder refuses fails `jadpo test` as
/// `test.recording_undecodable`, naming the field and the rule; one whose
/// field's `encode:` doesn't give back what its `decode:` read as
/// `test.round_trip_failed`; one named after no operation of its package
/// as `test.recording_undecodable`.
#[test]
fn a_recording_its_decoders_refuse_fails_the_run() {
    let root = desk(
        "refused",
        &[("bookings.create.json", recording("lost", "PM"))],
    );
    let ran = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(ran.code, 1, "{}{}", ran.stdout, ran.stderr);
    assert_eq!(
        ran.stdout,
        "FAIL  bookings.create's recording decodes\n --> shared/providers/acme_couriers/v1/recordings/bookings.create.json:1\n\n  decode: `bookings.create`'s recording doesn't decode: `variant` on `status`\n  reason: Every decoder decodes every recorded answer: the operation's output type, or a decoder of its own, refuses what the provider sent here, so a real answer like it would fault the call.\n  next: Correct the type or its decoder against the recording and the provider's documentation (agent)\n\n  rule: test.recording_undecodable\n  rerun: jadpo test shared/providers/acme_couriers/v1/recordings/bookings.create.json:1 --seed 1\n\ntest failed: 1 failed, 1 recording test\n"
    );
    let json = jadpo(&root, &["test", "--json", "--seed", "1"]);
    let tests = tests_of(&json);
    assert_eq!(tests[0]["failure"]["code"], "test.recording_undecodable");
    assert_eq!(tests[0]["failure"]["recording"]["field"], "status");
    assert_eq!(
        tests[0]["failure"]["recording"]["package"],
        "AcmeCouriers.v1"
    );

    let root = desk(
        "round-trip",
        &[("bookings.create.json", recording("booked", "EVE"))],
    );
    let ran = jadpo(&root, &["test", "--json", "--seed", "1"]);
    assert_eq!(ran.code, 1, "{}{}", ran.stdout, ran.stderr);
    let tests = tests_of(&ran);
    assert_eq!(tests[0]["failure"]["code"], "test.round_trip_failed");
    assert_eq!(tests[0]["failure"]["recording"]["recorded"], "\"EVE\"");
    assert_eq!(tests[0]["failure"]["recording"]["written"], "\"PM\"");

    let root = desk(
        "no-operation",
        &[("bookings.cancel.json", recording("booked", "PM"))],
    );
    let ran = jadpo(&root, &["test", "--json", "--seed", "1"]);
    let tests = tests_of(&ran);
    assert_eq!(tests[0]["failure"]["code"], "test.recording_undecodable");
    assert_eq!(
        tests[0]["failure"]["recording"]["message"],
        "`AcmeCouriers.v1` has no operation `bookings.cancel` to read it"
    );
}

/// The publishing bar runs every decoder over every recording, as its
/// `decoders` row.
#[test]
fn the_publishing_bar_decodes_every_recording() {
    let checkout = folder("bar");
    integration(&checkout, ".");
    write(
        &checkout,
        "recordings/bookings.create.json",
        &recording("booked", "AM"),
    );
    let checked = jadpo(&checkout, &["package", "check", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&checked.stdout).expect("JSON");
    let bar = document["data"]["bar"].as_array().expect("the bar").clone();
    let decoders = bar
        .iter()
        .find(|item| item["item"] == "decoders")
        .expect("a decoders row");
    assert_eq!(decoders["state"], "passed", "{bar:?}");
    assert_eq!(decoders["detail"], "1 recording decoded");
    let tests = bar
        .iter()
        .find(|item| item["item"] == "tests")
        .expect("a tests row");
    assert_eq!(tests["detail"], "no tests");

    write(
        &checkout,
        "recordings/bookings.create.json",
        &recording("booked", "EVE"),
    );
    let checked = jadpo(&checkout, &["package", "check", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&checked.stdout).expect("JSON");
    let bar = document["data"]["bar"].as_array().expect("the bar").clone();
    let decoders = bar
        .iter()
        .find(|item| item["item"] == "decoders")
        .expect("a decoders row");
    assert_eq!(decoders["state"], "failed", "{bar:?}");
    assert_eq!(decoders["detail"], "1 of 1 recording");
    let _ = fs::remove_dir_all(checkout);
}

/// `jadpo dev`'s fake answers an operation with a recording from it, its
/// placeholders filled with generated values of their types, logged as
/// `recording`.
#[test]
fn jadpo_dev_answers_from_a_recording() {
    let root = desk(
        "dev",
        &[("bookings.create.json", recording("collected", "AM"))],
    );
    let dev = Dev::start_without_database(&root);
    wait_for(&dev, "\"listening\"");
    let booked = dev.send("POST", "/bookings", &[], r#"{"postcode": "N1 9GU"}"#);
    assert_eq!(booked.status, 201, "{booked:?}");
    assert_eq!(booked.json()["status"], "collected");
    assert_eq!(booked.json()["id"], "example");
    let line = wait_for(&dev, "\"POST\"");
    let line: serde_json::Value = serde_json::from_str(&line).expect("JSON");
    assert_eq!(
        line["data"]["calls"],
        serde_json::json!([{ "call": "couriers.bookings.create", "source": "recording", "ended": "answered" }])
    );
    let listed = jadpo(&root, &["calls", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&listed.stdout).expect("JSON");
    let calls = document["data"]["calls"].as_array().expect("calls").clone();
    assert_eq!(calls[0]["source"], "recording");
    let answer: serde_json::Value =
        serde_json::from_str(calls[0]["answer"].as_str().expect("an answer")).expect("JSON");
    assert_eq!(answer["data"]["window"], "AM");
    assert_eq!(answer["data"]["id"], "example");
    assert_ne!(answer["data"]["collectAt"], "{Instant}");
}
