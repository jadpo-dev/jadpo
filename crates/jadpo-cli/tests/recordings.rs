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
use crate::support::fake_provider::FakeProvider;

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
    integration_at(root, at, "https://sandbox.acme-couriers.example");
}

/// [`integration`], its sandbox at `sandbox`.
fn integration_at(root: &Path, at: &str, sandbox: &str) {
    let file = |path: &str, text: &str| write(root, &format!("{at}/{path}"), text);
    file(
        "package.jadpo",
        "/// Acme Couriers' booking API.\npackage AcmeCouriers.v1 {\n    licence: Licence.mit\n    jadpo: \"1.0\"\n}\n",
    );
    file(
        "provider.jadpo",
        &format!(
            "/// Acme Couriers' API.\nprovider {{\n    protocol: Protocol.json_http\n    hosts: {{\n        live: \"https://api.acme-couriers.example\"\n        sandbox: \"{sandbox}\"\n    }}\n    credential: CredentialScheme.bearer\n    idempotency: {{\n        header: \"Idempotency-Key\"\n        window: 24h\n    }}\n    error_code: \"error.code\"\n}}\n"
        ),
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
    desk_at(name, "https://sandbox.acme-couriers.example", recordings)
}

/// [`desk`], its courier's sandbox at `sandbox`.
fn desk_at(name: &str, sandbox: &str, recordings: &[(&str, String)]) -> PathBuf {
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
    integration_at(&root, PACKAGE, sandbox);
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
    let updated = jadpo(&root, &["records", "update"]);
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

// --- jadpo package record ------------------------------------------------------

const ANSWER: &str = r#"{"data": {"id": "bk_481", "collectAt": "2026-10-21T09:00:00Z", "status": "booked", "window": "PM", "driver": "Ada Lovelace"}, "meta": {"trace": "t_99"}}"#;

fn record(root: &Path, extra: &[&str]) -> Output {
    let mut args = vec![
        "package",
        "record",
        "couriers",
        "bookings.create",
        "--input",
        r#"{"postcode": "N1 9GU"}"#,
    ];
    args.extend_from_slice(extra);
    jadpo(root, &args)
}

/// `jadpo package record` calls the sandbox once through the connection,
/// with its credential, checks the answer against the operation's types
/// and keeps it as the operation's recording: only what the types
/// declare, along `output_at:`, each plain value a placeholder and the
/// values the package's own decoders read as sent. `jadpo test` then
/// decodes it, and recording the same answer again changes nothing.
#[test]
fn jadpo_package_record_keeps_the_sandboxs_answer() {
    let sandbox = FakeProvider::start();
    let root = desk_at("record", &sandbox.url, &[]);
    write(&root, ".env.local", "COURIERS_API_KEY=sk_test_481\n");
    sandbox.answer(200, ANSWER);
    let recorded = record(&root, &[]);
    assert_eq!(recorded.code, 0, "{}{}", recorded.stdout, recorded.stderr);
    let file = "shared/providers/acme_couriers/v1/recordings/bookings.create.json";
    assert_eq!(
        fs::read_to_string(root.join(file)).expect("the recording"),
        recording("booked", "PM")
    );
    assert_eq!(
        recorded.stdout,
        format!(
            "Recorded `couriers.bookings.create` from the sandbox\n    sent    POST {}/v1/bookings\n    answer  decodes as Booking\n    values  2 placeholders, 2 kept as sent\n    file    {file}\n\noperation recorded: AcmeCouriers.v1 bookings.create, confirmed\n\nnext: jadpo test  decodes the recording with every decoder\n",
            sandbox.url
        )
    );
    let requests = sandbox.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, "/v1/bookings");
    assert_eq!(
        requests[0].header("authorization"),
        Some("Bearer sk_test_481")
    );
    assert_eq!(requests[0].json()["postcode"], "N1 9GU");
    assert!(
        requests[0]
            .header("idempotency-key")
            .is_some_and(|key| key.starts_with("jd1_")),
        "{:?}",
        requests[0].headers
    );

    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.stdout, "tested: 1 passed, 1 recording test\n");
    let shown = jadpo(
        &root,
        &[
            "package",
            "show",
            "acme_couriers",
            "bookings.create",
            "--json",
        ],
    );
    assert!(
        shown.stdout.contains("recordings/bookings.create.json"),
        "{}",
        shown.stdout
    );

    sandbox.answer(200, ANSWER);
    let again = record(&root, &["--json"]);
    let document: serde_json::Value = serde_json::from_str(&again.stdout).expect("JSON");
    assert_eq!(document["data"]["written"], false);
    assert_eq!(document["data"]["placeholders"], 2);
    assert_eq!(document["data"]["file"], file);
}

/// `record` never calls a live API, nor a sandbox with nothing set for
/// its credential: it refuses before anything is sent.
#[test]
fn jadpo_package_record_refuses_live_and_missing_credentials() {
    let sandbox = FakeProvider::start();
    let root = desk_at("record-refused", &sandbox.url, &[]);
    let unset = record(&root, &[]);
    assert_eq!(unset.code, 1, "{}{}", unset.stdout, unset.stderr);
    assert!(
        unset.stderr.contains("rule: recording.refused"),
        "{}",
        unset.stderr
    );
    assert!(
        unset
            .stderr
            .contains("next: Set `COURIERS_API_KEY` in `.env.local` to the sandbox's, then run it again (human)"),
        "{}",
        unset.stderr
    );
    write(
        &root,
        ".env.local",
        "COURIERS_API_KEY=sk_live_1\nCOURIERS_ENVIRONMENT=live\n",
    );
    let live = record(&root, &[]);
    assert_eq!(live.code, 1, "{}{}", live.stdout, live.stderr);
    assert!(
        live.stderr
            .contains("error: `couriers` calls its provider's live API"),
        "{}",
        live.stderr
    );
    assert!(sandbox.requests().is_empty());
    assert!(!root.join(PACKAGE).join("recordings").exists());
}

/// An answer the operation's types refuse, or a refusal, is recorded as
/// nothing, and says why.
#[test]
fn jadpo_package_record_keeps_only_an_answer_that_decodes() {
    let sandbox = FakeProvider::start();
    let root = desk_at("record-mismatch", &sandbox.url, &[]);
    write(&root, ".env.local", "COURIERS_API_KEY=sk_test_481\n");
    sandbox.answer(200, &ANSWER.replace("booked", "lost"));
    let lost = record(&root, &[]);
    assert_eq!(lost.code, 1, "{}{}", lost.stdout, lost.stderr);
    assert!(
        lost.stderr.contains(
            "error: The sandbox's answer to `couriers.bookings.create` doesn't decode as `Booking`"
        ),
        "{}",
        lost.stderr
    );
    assert!(
        lost.stderr.contains("`variant` on `status`"),
        "{}",
        lost.stderr
    );
    assert!(
        lost.stderr.contains("rule: recording.failed"),
        "{}",
        lost.stderr
    );
    sandbox.answer(409, r#"{"error": {"code": "slot_taken"}}"#);
    let taken = record(&root, &[]);
    assert!(
        taken
            .stderr
            .contains("error: The sandbox refused `couriers.bookings.create` with `slot_taken`"),
        "{}",
        taken.stderr
    );
    assert!(!root.join(PACKAGE).join("recordings").exists());
}

/// What to record is checked against the app first, naming the nearest.
#[test]
fn jadpo_package_record_checks_what_it_is_asked_for() {
    let root = desk("record-arguments", &[]);
    let connection = jadpo(&root, &["package", "record", "courier", "bookings.create"]);
    assert_eq!(
        connection.code, 2,
        "{}{}",
        connection.stdout, connection.stderr
    );
    assert!(
        connection
            .stderr
            .contains("next: Use `jadpo package record couriers bookings.create`"),
        "{}",
        connection.stderr
    );
    let operation = jadpo(&root, &["package", "record", "couriers", "bookings.creat"]);
    assert!(
        operation
            .stderr
            .contains("next: Use `jadpo package record couriers bookings.create`"),
        "{}",
        operation.stderr
    );
    let input = jadpo(&root, &["package", "record", "couriers", "bookings.create"]);
    assert!(
        input
            .stderr
            .contains("error: `couriers.bookings.create` needs its input"),
        "{}",
        input.stderr
    );
    let wrong = jadpo(
        &root,
        &[
            "package",
            "record",
            "couriers",
            "bookings.create",
            "--input",
            r#"{"postcode": "far too long a postcode"}"#,
        ],
    );
    assert!(
        wrong.stderr.contains("`max_length` on `postcode`"),
        "{}",
        wrong.stderr
    );
}
