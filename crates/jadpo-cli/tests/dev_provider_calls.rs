//! Provider calls in `jadpo dev` and the call log (area 11, "Provider
//! calls in development"; area 17, "Provider calls in development: `jadpo
//! calls`"; chunk 7.2d): every call faked by default with no credential
//! needed, its answer scripted by `jadpo calls script`, the operations
//! `development: { sandbox: }` names let through to a sandbox started in
//! the test on this machine, never live, and every call listed by `jadpo
//! calls` with its values in full.
//!
//! The project has no tables, so `jadpo dev` serves it without a
//! database, and nothing reaches a network.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

use crate::http::{Dev, wait_for};
use crate::support::fake_provider::FakeProvider;

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("dev-calls-7.2d-{name}"));
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
    run(&args, &Environment::empty().with_cwd(cwd))
}

/// A courier desk with a route that books a collection through its own
/// integration, whose sandbox is `sandbox`; `development` is the
/// connection's `development:` block, if any.
fn desk(name: &str, sandbox: &str, development: &str) -> PathBuf {
    let root = folder(name);
    write(
        &root,
        "app.jadpo",
        &format!(
            "application {{\n    connections: {{\n        couriers: {{\n            package: AcmeCouriers.v1\n            domains: [shipping]\n            credential: config.couriers_api_key\n            environment: config.couriers_environment\n{development}        }}\n    }}\n}}\n"
        ),
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
    write(
        &root,
        "shared/providers/acme_couriers/v1/package.jadpo",
        "/// Acme Couriers' booking API.\npackage AcmeCouriers.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n",
    );
    write(
        &root,
        "shared/providers/acme_couriers/v1/provider.jadpo",
        &format!(
            "/// Acme Couriers' API.\nprovider {{\n    protocol: Protocol.json_http\n    hosts: {{\n        live: \"https://api.acme-couriers.example\"\n        sandbox: \"{sandbox}\"\n    }}\n    credential: CredentialScheme.bearer\n    idempotency: {{\n        header: \"Idempotency-Key\"\n        window: 24h\n    }}\n    error_code: \"error.code\"\n}}\n"
        ),
    );
    write(
        &root,
        "shared/providers/acme_couriers/v1/resources/bookings.jadpo",
        "public resource bookings {\n    create: {\n        method: HttpMethod.post\n        path: \"/v1/bookings\"\n        input: CreateBooking\n        output: Booking\n        output_at: \"data\"\n        changes: true\n        idempotent: true\n        failures: [SlotTaken]\n        source: PackageSource.written\n        docs: \"https://docs.acme-couriers.example/bookings\"\n    }\n}\n",
    );
    write(
        &root,
        "shared/providers/acme_couriers/v1/types/postcode.jadpo",
        "/// Where a collection is.\npublic type Postcode = Text {\n    max_length: 8\n}\n",
    );
    write(
        &root,
        "shared/providers/acme_couriers/v1/types/booking_id.jadpo",
        "/// The courier's reference.\npublic type BookingId = Text {\n    max_length: 40\n}\n",
    );
    write(
        &root,
        "shared/providers/acme_couriers/v1/types/create_booking.jadpo",
        "/// What booking a collection takes.\npublic type CreateBooking = Object {\n    postcode: Postcode\n}\n",
    );
    write(
        &root,
        "shared/providers/acme_couriers/v1/types/booking.jadpo",
        "/// A booked collection.\npublic type Booking = Object {\n    id: BookingId\n    collect_at: Instant {\n        wire_name: \"collectAt\"\n    }\n}\n",
    );
    write(
        &root,
        "shared/providers/acme_couriers/v1/failures/slot_taken.jadpo",
        "/// The slot went before the booking arrived.\npublic type SlotTaken = Failure {\n    kind: FailureKind.conflict\n    message: \"That slot is taken.\"\n}\n",
    );
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
        "use shared/providers/acme_couriers\n\n/// A booked collection, as the desk shows it.\ntype Booked = Output {\n    id: AcmeCouriers.BookingId\n}\n",
    );
    write(
        &root,
        "domains/shipping/actions/book.jadpo",
        "use app { couriers }\nuse shipping/failures { SlotGone }\nuse shipping/types { Booked, BookingForm }\nuse shared/providers/acme_couriers\n\n/// Books a collection.\naction book(form: BookingForm) fails SlotGone -> Booked {\n    var booking: AcmeCouriers.Booking = attempt couriers.bookings.create(AcmeCouriers.CreateBooking {\n        postcode: form.postcode\n    }) {\n        slot_taken: SlotGone\n    }\n    return Booked {\n        id: booking.id\n    }\n}\n",
    );
    write(
        &root,
        "domains/shipping/routes/bookings.jadpo",
        "use shipping/actions { book }\nuse shipping/types { Booked, BookingForm }\n\n/// Booking collections.\nroutes Bookings {\n    prefix: /bookings\n\n    /// Anyone at the desk books one.\n    POST / {\n        authentication: RouteAuthentication.none\n        input: BookingForm\n        output: Booked\n        status: SuccessStatus.created\n        deadline: 30s\n        run: book(input)\n    }\n}\n",
    );
    let updated = jadpo(&root, &["api", "update"]);
    assert_eq!(updated.code, 0, "{}{}", updated.stdout, updated.stderr);
    root
}

fn book(dev: &Dev) -> crate::http::Reply {
    dev.send("POST", "/bookings", &[], r#"{"postcode": "N1 9GU"}"#)
}

/// Every provider call is faked by default, with no credential set: its
/// generated default, read back through the operation's decoding. Each
/// call shows under its request's line and in `jadpo calls`, and `jadpo
/// calls script` sets the next answer.
#[test]
fn jadpo_dev_fakes_provider_calls_and_logs_each_one() {
    let root = desk("faked", "https://sandbox.acme-couriers.example", "");
    let dev = Dev::start_without_database(&root);
    let listening = wait_for(&dev, "\"listening\"");
    let listening: serde_json::Value = serde_json::from_str(&listening).expect("JSON");
    assert_eq!(
        listening["data"]["providers"],
        serde_json::json!([{ "connection": "couriers", "sandbox": [], "unset": [] }])
    );
    let booked = book(&dev);
    assert_eq!(booked.status, 201, "{booked:?}");
    assert_eq!(booked.json()["id"], "example");
    let line = wait_for(&dev, "\"POST\"");
    let line: serde_json::Value = serde_json::from_str(&line).expect("JSON");
    assert_eq!(
        line["data"]["calls"],
        serde_json::json!([{ "call": "couriers.bookings.create", "source": "generated", "ended": "answered" }])
    );

    // Scripted: the next call answers `slot_taken`, which the action maps.
    let refused = jadpo(
        &root,
        &["calls", "script", "couriers.bookings.create", "slot_takn"],
    );
    assert_eq!(refused.code, 2);
    assert!(
        refused
            .stderr
            .contains("next: Use `jadpo calls script couriers.bookings.create slot_taken` (agent)"),
        "{}",
        refused.stderr
    );
    let scripted = jadpo(
        &root,
        &["calls", "script", "couriers.bookings.create", "slot_taken"],
    );
    assert_eq!(
        scripted.stdout,
        "Scripted `slot_taken` for the next call to `couriers.bookings.create`\n\nnext: jadpo calls  shows the call once the app makes it\n"
    );
    let gone = book(&dev);
    assert_eq!(gone.status, 409, "{gone:?}");
    assert_eq!(gone.json()["error"]["code"], "slot_gone");

    let listed = jadpo(&root, &["calls", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&listed.stdout).expect("JSON");
    let calls = document["data"]["calls"].as_array().expect("calls").clone();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0]["id"], "call_1");
    assert_eq!(calls[0]["source"], "generated");
    assert_eq!(calls[0]["ended"], "answered");
    assert_eq!(calls[0]["body"], r#"{"postcode":"N1 9GU"}"#);
    assert_eq!(calls[0]["method"], "POST");
    assert_eq!(
        calls[0]["url"],
        "https://sandbox.acme-couriers.example/v1/bookings"
    );
    let answer: serde_json::Value =
        serde_json::from_str(calls[0]["answer"].as_str().expect("an answer")).expect("JSON");
    assert_eq!(answer["data"]["id"], "example");
    assert_eq!(calls[1]["source"], "scripted");
    assert_eq!(calls[1]["ended"], "failed");
    assert_eq!(calls[1]["code"], "slot_taken");
    let request = calls[1]["requestId"].as_str().expect("its request");
    assert_eq!(Some(request), gone.header("x-request-id"));

    // The screen, with its times and request ids as they came.
    let plain = jadpo(&root, &["calls"]);
    let first = calls[0]["requestId"].as_str().expect("a request");
    let time = |at: usize| format!("{} ms", calls[at]["ms"].as_u64().unwrap_or(0));
    assert_eq!(
        plain.stdout,
        format!(
            "  call    operation                 source     answer        time  request\n  call_1  couriers.bookings.create  generated  + answered    {}  {}\n  call_2  couriers.bookings.create  scripted   x slot_taken  {}  {}\n\ncalls: 2 calls, 1 answered, 1 failed\n\nnext: jadpo calls show call_2  shows what was sent and what came back\n",
            time(0),
            &first[..8],
            time(1),
            &request[..8],
        )
    );
    let filtered = jadpo(&root, &["calls", "--request", &request[..12], "--json"]);
    let filtered: serde_json::Value = serde_json::from_str(&filtered.stdout).expect("JSON");
    assert_eq!(filtered["data"]["calls"].as_array().map(Vec::len), Some(1));
    let none = jadpo(&root, &["calls", "--connection", "ledger"]);
    assert_eq!(none.stdout, "No calls match: --connection ledger\n");

    let shown = jadpo(&root, &["calls", "show", "call_1"]);
    assert_eq!(
        shown.stdout,
        format!(
            "`couriers.bookings.create` answered\n    call     call_1\n    source   generated\n    request  {first}\n    at       {}\n    time     {}\n\nsent\n  POST https://sandbox.acme-couriers.example/v1/bookings\n  {{\n    \"postcode\": \"N1 9GU\"\n  }}\n\nanswer\n  {{\n    \"data\": {{\n      \"collectAt\": \"{}\",\n      \"id\": \"example\"\n    }}\n  }}\n",
            calls[0]["at"].as_str().unwrap_or_default(),
            time(0),
            answer["data"]["collectAt"].as_str().unwrap_or_default(),
        )
    );
    let missing = jadpo(&root, &["calls", "show", "call_9"]);
    assert_eq!(missing.code, 2);
    assert!(
        missing.stderr.contains("error: There is no call `call_9`"),
        "{}",
        missing.stderr
    );
    drop(dev);
}

/// Before `jadpo dev` has made a call, `jadpo calls` says so and what
/// makes one.
#[test]
fn jadpo_calls_says_when_there_are_none() {
    let root = desk("none", "https://sandbox.acme-couriers.example", "");
    let output = jadpo(&root, &["calls"]);
    assert_eq!(
        output.stdout,
        "No calls yet\n\nnext: jadpo dev  runs the app, logging each provider call\n"
    );
    assert_eq!(output.code, 0);
}

/// The operations `development: { sandbox: }` names go to the sandbox
/// once its credential is set, here a provider started on this machine;
/// the rest stay faked. Without the credential the connection stays
/// faked and says which variable to set; with its environment live, `jadpo
/// dev` refuses to start.
#[test]
fn named_operations_go_to_the_sandbox_never_live() {
    let provider = FakeProvider::start();
    let development =
        "            development: {\n                sandbox: [bookings.create]\n            }\n";

    // No credential: faked, and the variable to set named.
    let root = desk("unset", &provider.url, development);
    let dev = Dev::start_without_database(&root);
    let listening: serde_json::Value =
        serde_json::from_str(&wait_for(&dev, "\"listening\"")).expect("JSON");
    assert_eq!(
        listening["data"]["providers"],
        serde_json::json!([{ "connection": "couriers", "sandbox": [], "unset": ["COURIERS_API_KEY"] }])
    );
    assert_eq!(book(&dev).status, 201);
    assert!(provider.requests().is_empty(), "nothing left the machine");
    drop(dev);

    // With the credential: through to the sandbox, with it.
    let root = desk("sandbox", &provider.url, development);
    write(&root, ".env.local", "COURIERS_API_KEY=ck_test_7\n");
    let dev = Dev::start_without_database(&root);
    let listening: serde_json::Value =
        serde_json::from_str(&wait_for(&dev, "\"listening\"")).expect("JSON");
    assert_eq!(
        listening["data"]["providers"],
        serde_json::json!([{ "connection": "couriers", "sandbox": ["bookings.create"], "unset": [] }])
    );
    provider.answer(
        201,
        r#"{"data": {"id": "b-7", "collectAt": "2026-10-20T09:00:00Z"}}"#,
    );
    let booked = book(&dev);
    assert_eq!(booked.status, 201, "{booked:?}");
    assert_eq!(booked.json()["id"], "b-7");
    let sent = provider.requests();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].header("authorization"), Some("Bearer ck_test_7"));
    let listed = jadpo(&root, &["calls", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&listed.stdout).expect("JSON");
    assert_eq!(document["data"]["calls"][0]["source"], "sandbox");
    assert!(
        !listed.stdout.contains("ck_test_7"),
        "the call log never holds a credential"
    );
    drop(dev);

    // `--fake couriers` keeps it faked for the session.
    let dev = Dev::start_with_providers(&root, vec!["couriers".into()], Vec::new());
    let listening: serde_json::Value =
        serde_json::from_str(&wait_for(&dev, "\"listening\"")).expect("JSON");
    assert_eq!(
        listening["data"]["providers"][0]["sandbox"],
        serde_json::json!([])
    );
    assert_eq!(book(&dev).status, 201);
    assert_eq!(provider.requests().len(), 1, "faked for the session");
    drop(dev);

    // Live: refused before it serves anything.
    write(
        &root,
        ".env.local",
        "COURIERS_API_KEY=ck_test_7\nCOURIERS_ENVIRONMENT=live\n",
    );
    let mut out = Vec::new();
    let mut err = Vec::new();
    let args = jadpo_cli::dev::DevArgs {
        cwd: root.clone(),
        port: 0,
        url: None,
        no_run: false,
        json: false,
        choice: jadpo_term::ColorChoice::Never,
        fake: Vec::new(),
        sandbox: Vec::new(),
    };
    let code = jadpo_cli::dev::serve_to(&args, &Environment::empty(), &mut out, &mut err, &|| true);
    assert_eq!(code, 1);
    let shown = String::from_utf8_lossy(&err);
    assert!(
        shown.contains(
            "error: `couriers` lets calls through to its provider, and its environment is live"
        ),
        "{shown}"
    );
    assert!(
        shown.contains(
            "next: Set `COURIERS_ENVIRONMENT` to `sandbox` in `.env.local`, or run `jadpo dev --fake couriers` (human)"
        ),
        "{shown}"
    );
    assert_eq!(provider.requests().len(), 1);
}

/// `development: { sandbox: }` names the package's operations or
/// resources, checked by `jadpo check`.
#[test]
fn a_sandbox_entry_is_checked_against_the_package() {
    let root = desk(
        "unknown-sandbox",
        "https://sandbox.acme-couriers.example",
        "            development: {\n                sandbox: [bookings.creat]\n            }\n",
    );
    let output = jadpo(&root, &["check", "."]);
    assert_eq!(output.code, 1);
    assert!(
        output.stdout.contains(
            "error: `couriers`'s `sandbox:` lists `bookings.creat`, which `AcmeCouriers.v1` doesn't have"
        ),
        "{}",
        output.stdout
    );
    assert!(
        output
            .stdout
            .contains("next: Use `bookings.create` instead of `bookings.creat` (agent)"),
        "{}",
        output.stdout
    );
}
