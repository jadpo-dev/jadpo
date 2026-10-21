//! Provider calls (area 11, "Calling" and "Decoding"; chunk 7.2c): a
//! provider package's operation called through its connection from an
//! action, run by the interpreter on the capability seam, sent by the
//! package driver over the native host's HTTP to a provider started in the
//! test on this machine, and its answer read back into the operation's
//! output.
//!
//! Each test compiles a project with two connections: `couriers`, to a
//! package with fixed live and sandbox hosts (the sandbox the fake
//! provider), a bearer credential, an idempotency key and an error code;
//! and `ledger`, to a package at the address a setting gives, with basic
//! credentials. Nothing reaches a network: an address that isn't HTTPS is
//! sent to only on this machine, and the live host is never chosen.

use std::path::Path;
use std::rc::Rc;

use jadpo_drivers::package::JsonHttp;
use jadpo_host_native::Runtime;
use jadpo_host_native::fetch::NativeFetch;
use jadpo_interp::{Executor, Outcome, Prepared, Program};
use jadpo_runtime::capability::{Capability, Ended, Faked};
use jadpo_runtime::{Clock, Ctx, Env, FaultKind, MemoryHost};
use jadpo_value::Value;

use crate::support::fake_provider::FakeProvider;

fn files(sandbox: &str) -> Vec<(&'static str, String)> {
    vec![
        (
            "app.jadpo",
            r#"application {
    connections: {
        couriers: {
            package: AcmeCouriers.v1
            domains: [shipping]
            credential: config.couriers_api_key
            environment: config.couriers_environment
            timeout: 5s
        }
        ledger: {
            package: Ledger.v1
            domains: [billing]
            credential: config.ledger_token
            host: config.ledger_address
        }
    }
    steps: {
        encryption_key: config.step_encryption_key
    }
}
"#
            .to_string(),
        ),
        (
            "config.jadpo",
            r#"config {
    couriers_api_key: Text {
        secret: true
    }
    couriers_environment: ProviderEnvironment
    ledger_address: Url
    ledger_token: Text {
        secret: true
    }
    step_encryption_key: SigningKey
}
"#
            .to_string(),
        ),
        (
            "shared/providers/acme_couriers/v1/package.jadpo",
            r#"/// Acme Couriers' booking and tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
"#
            .to_string(),
        ),
        (
            "shared/providers/acme_couriers/v1/provider.jadpo",
            format!(
                r#"/// Acme Couriers' API.
provider {{
    protocol: Protocol.json_http
    hosts: {{
        live: "https://api.acme-couriers.example"
        sandbox: "{sandbox}"
    }}
    credential: CredentialScheme.bearer
    idempotency: {{
        header: "Idempotency-Key"
        window: 24h
        max_length: 255
        in_use: "key_in_use"
    }}
    error_code: "error.code"
}}
"#
            ),
        ),
        (
            "shared/providers/acme_couriers/v1/resources/bookings.jadpo",
            r#"public resource bookings {
    create: {
        method: HttpMethod.post
        path: "/v1/bookings"
        input: CreateBooking
        output: Booking
        output_at: "data"
        failed_when: {
            path: "ok"
            equals: false
        }
        changes: true
        idempotent: true
        failures: [SlotTaken]
        source: PackageSource.written
    }
}
"#
            .to_string(),
        ),
        (
            "shared/providers/acme_couriers/v1/resources/parcels.jadpo",
            r#"public resource parcels {
    get: {
        method: HttpMethod.get
        path: "/v1/parcels/{parcelId}"
        input: ParcelLookup
        output: Parcel
        changes: false
        failures: [ParcelUnknown]
        source: PackageSource.written
    }
}
"#
            .to_string(),
        ),
        (
            "shared/providers/acme_couriers/v1/types/create_booking.jadpo",
            r#"/// What booking a collection takes.
public type CreateBooking = Object {
    postcode: Postcode
    slot_id: SlotId {
        wire_name: "slotId"
    }
}
"#
            .to_string(),
        ),
        (
            "shared/providers/acme_couriers/v1/types/booking.jadpo",
            r#"/// A booked collection.
public type Booking = Object {
    id: Text
    collect_at: Instant {
        wire_name: "collectAt"
    }
    window: SlotWindow
}
"#
            .to_string(),
        ),
        (
            "shared/providers/acme_couriers/v1/types/slot_window.jadpo",
            r#"/// When in the day a collection comes.
public type SlotWindow = Enum {
    morning
    after_noon: {
        wire_name: "after-noon"
    }
}
"#
            .to_string(),
        ),
        (
            "shared/providers/acme_couriers/v1/types/parcel_lookup.jadpo",
            r#"/// Which parcel to look up.
public type ParcelLookup = Object {
    parcel_id: ParcelId {
        wire_name: "parcelId"
    }
    with_history: Bool {
        wire_name: "withHistory"
    }
}
"#
            .to_string(),
        ),
        (
            "shared/providers/acme_couriers/v1/types/postcode.jadpo",
            "/// A postcode as the courier takes it.\npublic type Postcode = Text {\n    max_length: 10\n}\n"
                .to_string(),
        ),
        (
            "shared/providers/acme_couriers/v1/types/slot_id.jadpo",
            "/// A collection slot's code.\npublic type SlotId = Text {\n    max_length: 10\n}\n"
                .to_string(),
        ),
        (
            "shared/providers/acme_couriers/v1/types/parcel_id.jadpo",
            "/// The courier's parcel reference.\npublic type ParcelId = Text {\n    max_length: 40\n}\n"
                .to_string(),
        ),
        (
            "shared/providers/acme_couriers/v1/types/parcel.jadpo",
            r#"/// A parcel and where it is.
public type Parcel = Object {
    id: Text
    status: Text
}
"#
            .to_string(),
        ),
        (
            "shared/providers/acme_couriers/v1/failures/slot_taken.jadpo",
            r#"/// The collection slot was taken first.
public type SlotTaken = Failure {
    kind: FailureKind.conflict
    message: "The collection slot is no longer available."
}
"#
            .to_string(),
        ),
        (
            "shared/providers/acme_couriers/v1/failures/parcel_unknown.jadpo",
            r#"/// No parcel has that reference.
public type ParcelUnknown = Failure {
    kind: FailureKind.not_found
    message: "No parcel has that reference."
}
"#
            .to_string(),
        ),
        (
            "shared/providers/ledger/v1/package.jadpo",
            r#"/// The shop's ledger service.
package Ledger.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
"#
            .to_string(),
        ),
        (
            "shared/providers/ledger/v1/provider.jadpo",
            r#"/// The ledger's API, at the address each deployment gives.
provider {
    protocol: Protocol.json_http
    credential: CredentialScheme.basic
}
"#
            .to_string(),
        ),
        (
            "shared/providers/ledger/v1/resources/postings.jadpo",
            r#"public resource postings {
    create: {
        method: HttpMethod.post
        path: "/v1/postings"
        input: NewPosting
        output: Posting
        changes: true
        idempotent: false
        source: PackageSource.written
    }
}
"#
            .to_string(),
        ),
        (
            "shared/providers/ledger/v1/types/new_posting.jadpo",
            r#"/// An amount to record.
public type NewPosting = Object {
    reference: Reference
    pence: Pence
}
"#
            .to_string(),
        ),
        (
            "shared/providers/ledger/v1/types/reference.jadpo",
            "/// What a posting is filed under.\npublic type Reference = Text {\n    max_length: 20\n}\n"
                .to_string(),
        ),
        (
            "shared/providers/ledger/v1/types/pence.jadpo",
            "/// An amount in pence.\npublic type Pence = Int {\n    min: 0\n    max: 100000000\n}\n"
                .to_string(),
        ),
        (
            "shared/providers/ledger/v1/types/posting.jadpo",
            r#"/// A recorded amount.
public type Posting = Object {
    id: Text
}
"#
            .to_string(),
        ),
        (
            "domains/shipping/failures/collection_slot_gone.jadpo",
            "type CollectionSlotGone = Failure {\n    kind: FailureKind.conflict\n    message: \"That collection slot has gone.\"\n}\n"
                .to_string(),
        ),
        (
            "domains/shipping/failures/parcel_missing.jadpo",
            "type ParcelMissing = Failure {\n    kind: FailureKind.not_found\n    message: \"We can't find that parcel.\"\n}\n"
                .to_string(),
        ),
        (
            "domains/shipping/actions/book_collection.jadpo",
            r#"use app { couriers }
use shipping/failures { CollectionSlotGone }
use shared/providers/acme_couriers

/// Books a collection.
action book_collection(postcode: AcmeCouriers.Postcode, slot: AcmeCouriers.SlotId) fails CollectionSlotGone -> AcmeCouriers.Booking {
    var request: AcmeCouriers.CreateBooking = AcmeCouriers.CreateBooking {
        postcode: postcode
        slot_id: slot
    }
    return attempt couriers.bookings.create(request) {
        slot_taken: CollectionSlotGone
    }
}
"#
            .to_string(),
        ),
        (
            "domains/shipping/actions/book_twice.jadpo",
            r#"use app { couriers }
use shipping/failures { CollectionSlotGone }
use shared/providers/acme_couriers

/// Books two collections from one postcode.
decline step.late_failure: "A first collection booked without the second is still wanted"
action book_twice(postcode: AcmeCouriers.Postcode, first: AcmeCouriers.SlotId, second: AcmeCouriers.SlotId) fails CollectionSlotGone -> AcmeCouriers.Booking {
    var one: AcmeCouriers.Booking = attempt couriers.bookings.create(AcmeCouriers.CreateBooking {
        postcode: postcode
        slot_id: first
    }) {
        slot_taken: CollectionSlotGone
    }
    var two: AcmeCouriers.Booking = attempt couriers.bookings.create(AcmeCouriers.CreateBooking {
        postcode: postcode
        slot_id: second
    }) {
        slot_taken: CollectionSlotGone
    }
    if (one.id == two.id) {
        return one
    }
    return two
}
"#
            .to_string(),
        ),
        (
            "domains/shipping/actions/book_unmapped.jadpo",
            r#"use app { couriers }
use shared/providers/acme_couriers

/// Books a collection, leaving a taken slot unmapped.
action book_unmapped(postcode: AcmeCouriers.Postcode, slot: AcmeCouriers.SlotId) -> AcmeCouriers.Booking {
    return attempt couriers.bookings.create(AcmeCouriers.CreateBooking {
        postcode: postcode
        slot_id: slot
    })
}
"#
            .to_string(),
        ),
        (
            "domains/shipping/actions/track_parcel.jadpo",
            r#"use app { couriers }
use shipping/failures { ParcelMissing }
use shared/providers/acme_couriers

/// Where a parcel is.
action track_parcel(parcel: AcmeCouriers.ParcelId) fails ParcelMissing -> AcmeCouriers.Parcel {
    return attempt couriers.parcels.get(AcmeCouriers.ParcelLookup {
        parcel_id: parcel
        with_history: true
    }) {
        parcel_unknown: ParcelMissing
    }
}
"#
            .to_string(),
        ),
        (
            "domains/billing/actions/record_invoice.jadpo",
            r#"use app { ledger }
use shared/providers/ledger

/// Records an invoice's amount in the ledger.
action record_invoice(code: Ledger.Reference, pence: Ledger.Pence) -> Ledger.Posting {
    return ledger.postings.create(Ledger.NewPosting {
        reference: code
        pence: pence
    })
}
"#
            .to_string(),
        ),
    ]
}

/// The project, checked, with its settings: the couriers' sandbox, and
/// the ledger at `ledger`.
fn program(root: &Path, sandbox: &str, ledger: &str) -> Program {
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
        match name {
            "COURIERS_API_KEY" => Some("ck_test_123".into()),
            "COURIERS_ENVIRONMENT" => Some("sandbox".into()),
            "LEDGER_ADDRESS" => Some(ledger.to_string()),
            "LEDGER_TOKEN" => Some("shop:s3cret".into()),
            "STEP_ENCRYPTION_KEY" => {
                Some("a-signing-key-for-the-record-of-steps-long-enough".into())
            }
            _ => None,
        }
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
    std::env::temp_dir().join(format!("jadpo-7.2c-{name}-{}", std::process::id()))
}

/// The package driver, over the native host's HTTP, for both connections.
fn host() -> Faked<MemoryHost> {
    let driver: Rc<dyn Capability> = Rc::new(JsonHttp::new(NativeFetch::new()));
    Faked::new(MemoryHost::new())
        .with("couriers", Rc::clone(&driver))
        .with("ledger", driver)
}

fn env() -> Env {
    Env {
        seed: Some(7),
        clock: Clock::TEST,
    }
}

/// Runs action `name` with `args` in `ctx`.
fn run(
    runtime: &Runtime,
    program: &Program,
    ctx: &mut Ctx<'_>,
    name: &str,
    args: Vec<Value>,
) -> Outcome {
    let id = program.plan().find(name).expect("the action");
    runtime.block_on(program.run(ctx, id, args))
}

fn text(value: &str) -> Value {
    Value::text(value)
}

fn field<'v>(value: &'v Value, name: &str) -> &'v Value {
    match value {
        Value::Record(record) => record.get(name).expect("the field"),
        other => panic!("not a record: {other:?}"),
    }
}

/// The operation is sent as the package declares it: to the sandbox the
/// environment chooses, `POST` with its input as JSON in the provider's
/// spellings, the bearer credential, a derived idempotency key; and the
/// answer is read from inside its envelope, members it doesn't declare
/// left out, into the package's types, spellings on the wire mapped back.
#[test]
fn a_call_goes_to_the_provider_and_its_answer_is_decoded() {
    let provider = FakeProvider::start();
    let program = program(&root("decoded"), &provider.url, "https://ledger.example");
    let runtime = Runtime::new().expect("a runtime");
    provider.answer(
        201,
        r#"{"ok": true, "data": {"id": "b-1", "collectAt": "2026-10-20T09:00:00Z", "window": "after-noon", "courier": "Sam"}}"#,
    );
    let host = host();
    let mut ctx = Ctx::new(program.places(), &host, env());
    ctx.set_call_key("user:ada", "client-key-1");
    let outcome = run(
        &runtime,
        &program,
        &mut ctx,
        "book_collection",
        vec![text("N1 9GU"), text("am")],
    );
    let Outcome::Value(booking) = outcome else {
        panic!("{:?}", program.report(&outcome).map(|r| r.headline()));
    };
    assert_eq!(field(&booking, "id"), &text("b-1"));
    assert!(matches!(field(&booking, "collect_at"), Value::Instant(_)));
    match field(&booking, "window") {
        Value::Variant(variant) => assert_eq!(&*variant.name, "after_noon"),
        other => panic!("{other:?}"),
    }
    let sent = provider.requests();
    assert_eq!(sent.len(), 1);
    let request = &sent[0];
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/v1/bookings");
    assert_eq!(request.header("authorization"), Some("Bearer ck_test_123"));
    assert_eq!(request.header("content-type"), Some("application/json"));
    assert_eq!(
        request.json(),
        serde_json::json!({"postcode": "N1 9GU", "slotId": "am"})
    );
    let key = request
        .header("idempotency-key")
        .expect("a key")
        .to_string();
    assert_eq!(key.len(), 36, "{key}");
    assert!(key.starts_with("jd1_"), "{key}");
    // Logged without its values: keyed, answered.
    let logged = ctx.capabilities().calls();
    assert_eq!(logged.len(), 1);
    assert_eq!(logged[0].connection, "couriers");
    assert_eq!(logged[0].operation, "bookings.create");
    assert_eq!(logged[0].repeat, jadpo_runtime::capability::Repeat::Keyed);
    assert_eq!(logged[0].ended, Ended::Answered);
}

/// The key is derived from the caller's scope, the action's key and the
/// step: the same call by the same caller sends the same key, another
/// caller with the same `Idempotency-Key` another, and the second call of
/// an action another again.
#[test]
fn keys_differ_by_caller_and_step_and_repeat_for_the_same_call() {
    let provider = FakeProvider::start();
    let program = program(&root("keys"), &provider.url, "https://ledger.example");
    let runtime = Runtime::new().expect("a runtime");
    let booked = r#"{"ok": true, "data": {"id": "b-1", "collectAt": "2026-10-20T09:00:00Z", "window": "morning"}}"#;
    for _ in 0..5 {
        provider.answer(201, booked);
    }
    let host = host();
    let mut keys = Vec::new();
    for scope in ["user:ada", "user:ada", "user:bob"] {
        let mut ctx = Ctx::new(program.places(), &host, env());
        ctx.set_call_key(scope, "client-key-1");
        let outcome = run(
            &runtime,
            &program,
            &mut ctx,
            "book_collection",
            vec![text("N1"), text("am")],
        );
        assert!(matches!(outcome, Outcome::Value(_)), "{outcome:?}");
    }
    let mut ctx = Ctx::new(program.places(), &host, env());
    ctx.set_call_key("user:ada", "client-key-1");
    let outcome = run(
        &runtime,
        &program,
        &mut ctx,
        "book_twice",
        vec![text("N1"), text("am"), text("pm")],
    );
    assert!(matches!(outcome, Outcome::Value(_)), "{outcome:?}");
    for request in provider.requests() {
        keys.push(
            request
                .header("idempotency-key")
                .expect("a key")
                .to_string(),
        );
    }
    assert_eq!(keys.len(), 5);
    assert_eq!(keys[0], keys[1], "the same caller's same call");
    assert_ne!(keys[0], keys[2], "another caller with the same key");
    assert_eq!(keys[3], keys[0], "the first step of another action");
    assert_ne!(keys[4], keys[3], "the second step");
}

/// A failure is mapped at the call by its code, whether a `200` that
/// `failed_when:` marks or a refusal, with the provider's kept as the
/// cause; a code the call maps to nothing, or the operation doesn't
/// declare, is a fault; a rejected credential is `misconfigured`; and a
/// server's error on a keyed call, every attempt, is `unavailable`.
#[test]
fn failures_are_mapped_by_their_codes_at_the_call() {
    let provider = FakeProvider::start();
    let program = program(&root("failures"), &provider.url, "https://ledger.example");
    let runtime = Runtime::new().expect("a runtime");
    provider.answer(200, r#"{"ok": false, "error": {"code": "slot_taken"}}"#);
    provider.answer(409, r#"{"error": {"code": "slot_taken"}}"#);
    provider.answer(409, r#"{"error": {"code": "slot_taken"}}"#);
    provider.answer(422, r#"{"error": {"code": "postcode_unserved"}}"#);
    provider.answer(401, r#"{"error": {"code": "unauthorised"}}"#);
    // A keyed call is sent again on a server's error while `retry:` lasts
    // (chunk 7.2c2): three attempts by default.
    for _ in 0..3 {
        provider.answer(503, "");
    }
    provider.answer(
        200,
        r#"{"ok": true, "data": {"id": "b-1", "collectAt": "soon", "window": "morning"}}"#,
    );
    let host = host();
    let mut outcomes = Vec::new();
    for action in [
        "book_collection",
        "book_collection",
        "book_unmapped",
        "book_collection",
        "book_collection",
        "book_collection",
        "book_collection",
    ] {
        let mut ctx = Ctx::new(program.places(), &host, env());
        outcomes.push(run(
            &runtime,
            &program,
            &mut ctx,
            action,
            vec![text("N1"), text("am")],
        ));
    }
    let failed = |outcome: &Outcome| match outcome {
        Outcome::Failed(failure) => (
            failure.code.clone(),
            failure.cause.as_ref().map(|cause| cause.code.clone()),
        ),
        other => panic!("{other:?}"),
    };
    let fault = |outcome: &Outcome| match outcome {
        Outcome::Fault(fault) => match &fault.kind {
            FaultKind::Provider(what) => what.clone(),
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    };
    let mapped = (
        "collection_slot_gone".to_string(),
        Some("slot_taken".to_string()),
    );
    assert_eq!(failed(&outcomes[0]), mapped);
    assert_eq!(failed(&outcomes[1]), mapped);
    assert!(
        fault(&outcomes[2]).contains("maps to nothing"),
        "{}",
        fault(&outcomes[2])
    );
    assert!(
        fault(&outcomes[3]).contains("`postcode_unserved`, which it doesn't declare"),
        "{}",
        fault(&outcomes[3])
    );
    assert_eq!(failed(&outcomes[4]), ("misconfigured".to_string(), None));
    assert_eq!(failed(&outcomes[5]), ("unavailable".to_string(), None));
    let undecoded = fault(&outcomes[6]);
    assert!(
        undecoded.contains("`couriers.bookings.create`'s answer doesn't decode"),
        "{undecoded}"
    );
    assert!(
        undecoded.contains("collect_at") || undecoded.contains("collectAt"),
        "{undecoded}"
    );
    assert!(!undecoded.contains("soon"), "never the value: {undecoded}");
}

/// A `GET` fills its path's placeholder from the input's member of that
/// spelling and sends the rest as its query, with no body and no key.
#[test]
fn a_get_sends_its_input_in_its_path_and_query() {
    let provider = FakeProvider::start();
    let program = program(&root("get"), &provider.url, "https://ledger.example");
    let runtime = Runtime::new().expect("a runtime");
    provider.answer(200, r#"{"id": "p/1", "status": "out_for_delivery"}"#);
    provider.answer(404, r#"{"error": {"code": "parcel_unknown"}}"#);
    let host = host();
    let mut ctx = Ctx::new(program.places(), &host, env());
    let outcome = run(
        &runtime,
        &program,
        &mut ctx,
        "track_parcel",
        vec![text("p/1")],
    );
    let Outcome::Value(parcel) = outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!(field(&parcel, "status"), &text("out_for_delivery"));
    let mut ctx = Ctx::new(program.places(), &host, env());
    let missing = run(
        &runtime,
        &program,
        &mut ctx,
        "track_parcel",
        vec![text("p/2")],
    );
    assert!(
        matches!(&missing, Outcome::Failed(failure) if failure.code == "parcel_missing"),
        "{missing:?}"
    );
    let sent = provider.requests();
    assert_eq!(sent[0].method, "GET");
    assert_eq!(sent[0].path, "/v1/parcels/p%2F1?withHistory=true");
    assert_eq!(sent[0].body, "");
    assert_eq!(sent[0].header("idempotency-key"), None);
    assert_eq!(
        ctx.capabilities().calls()[0].repeat,
        jadpo_runtime::capability::Repeat::Safe
    );
}

/// A connection with no fixed hosts is called at the address its setting
/// gives, with basic credentials, and an unkeyed change sends no key.
#[test]
fn a_connection_at_a_settings_address_sends_basic_credentials() {
    let provider = FakeProvider::start();
    let program = program(
        &root("ledger"),
        "https://sandbox.acme-couriers.example",
        &provider.url,
    );
    let runtime = Runtime::new().expect("a runtime");
    provider.answer(201, r#"{"id": "post-9"}"#);
    let host = host();
    let mut ctx = Ctx::new(program.places(), &host, env());
    let outcome = run(
        &runtime,
        &program,
        &mut ctx,
        "record_invoice",
        vec![text("INV-1"), Value::Int(1250)],
    );
    let Outcome::Value(posting) = outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!(field(&posting, "id"), &text("post-9"));
    let sent = provider.requests();
    assert_eq!(sent[0].path, "/v1/postings");
    assert_eq!(
        sent[0].header("authorization"),
        Some(format!("Basic {}", jadpo_value::encoding::to_base64(b"shop:s3cret")).as_str())
    );
    assert_eq!(sent[0].header("idempotency-key"), None);
    assert_eq!(
        sent[0].json(),
        serde_json::json!({"reference": "INV-1", "pence": 1250})
    );
    assert_eq!(
        ctx.capabilities().calls()[0].repeat,
        jadpo_runtime::capability::Repeat::Unkeyed
    );
}

/// An address that isn't HTTPS, off this machine, is refused before
/// anything is sent: the call is `misconfigured`, and logged as refused.
#[test]
fn an_address_that_isnt_https_is_refused_unsent() {
    let provider = FakeProvider::start();
    let program = program(&root("refused"), &provider.url, "http://ledger.example");
    let runtime = Runtime::new().expect("a runtime");
    let host = host();
    let mut ctx = Ctx::new(program.places(), &host, env());
    let outcome = run(
        &runtime,
        &program,
        &mut ctx,
        "record_invoice",
        vec![text("INV-1"), Value::Int(1)],
    );
    assert!(
        matches!(&outcome, Outcome::Failed(failure) if failure.code == "misconfigured"),
        "{outcome:?}"
    );
    assert_eq!(ctx.capabilities().calls()[0].ended, Ended::Refused);
    assert!(provider.requests().is_empty());
}

// --- with a store: no transaction is open while a call is out -----------------

/// The couriers' package and a domain that keeps what it books, in
/// PostgreSQL.
fn kept_files(sandbox: &str) -> Vec<(&'static str, String)> {
    let mut out: Vec<(&'static str, String)> = files(sandbox)
        .into_iter()
        .filter(|(path, _)| {
            path.starts_with("shared/providers/acme_couriers")
                || *path == "domains/shipping/failures/collection_slot_gone.jadpo"
        })
        .collect();
    out.extend([
        (
            "app.jadpo",
            r#"application {
    connections: {
        couriers: {
            package: AcmeCouriers.v1
            domains: [shipping]
            credential: config.couriers_api_key
            environment: config.couriers_environment
        }
    }
}
"#
            .to_string(),
        ),
        (
            "config.jadpo",
            r#"config {
    couriers_api_key: Text {
        secret: true
    }
    couriers_environment: ProviderEnvironment
}
"#
            .to_string(),
        ),
        (
            "domains/shipping/types/courier_ref.jadpo",
            "/// A courier's booking reference, as the shop keeps it.\ntype CourierRef = Text {\n    max_length: 40\n}\n"
                .to_string(),
        ),
        (
            "domains/shipping/failures/reference_invalid.jadpo",
            "type ReferenceInvalid = Failure {\n    kind: FailureKind.invalid_value\n    message: \"The courier's reference is too long to keep.\"\n}\n"
                .to_string(),
        ),
        (
            "domains/shipping/entities/collection.jadpo",
            r#"use shipping/types { CourierRef }

entity Collection {
    id: Uuid
    reference: CourierRef

    policy: {
        create: [Access.public, Access.system]
        read: [Access.public, Access.system]
    }
}
"#
            .to_string(),
        ),
        (
            "domains/shipping/actions/book_and_keep.jadpo",
            r#"use app { couriers }
use shipping/entities { Collection }
use shipping/failures { CollectionSlotGone, ReferenceInvalid }
use shipping/types { CourierRef }
use shared/providers/acme_couriers

/// Books a collection, then keeps its reference: the call comes before
/// the action's first write.
decline step.late_failure: "The courier's references always fit a CourierRef"
action book_and_keep(postcode: AcmeCouriers.Postcode, slot: AcmeCouriers.SlotId) fails CollectionSlotGone, ReferenceInvalid -> Collection {
    var booking: AcmeCouriers.Booking = attempt couriers.bookings.create(AcmeCouriers.CreateBooking {
        postcode: postcode
        slot_id: slot
    }) {
        slot_taken: CollectionSlotGone
    }
    var reference: CourierRef = attempt CourierRef(booking.id) {
        invalid: ReferenceInvalid
    }
    return Collection.create {
        set: {
            reference: reference
        }
    }
}
"#
            .to_string(),
        ),
    ]);
    out
}

/// Keeps a reference, then books: the call comes after the action's first
/// write, which `jadpo check` refuses (chunk 7.4b).
const KEEP_THEN_BOOK: &str = r#"use app { couriers }
use shipping/entities { Collection }
use shipping/failures { CollectionSlotGone }
use shipping/types { CourierRef }
use shared/providers/acme_couriers

/// Keeps a reference, then books: the call comes after the action's first
/// write, while its transaction is open.
action keep_then_book(reference: CourierRef, postcode: AcmeCouriers.Postcode, slot: AcmeCouriers.SlotId) fails CollectionSlotGone -> Collection {
    var kept: Collection = Collection.create {
        set: {
            reference: reference
        }
    }
    var booking: AcmeCouriers.Booking = attempt couriers.bookings.create(AcmeCouriers.CreateBooking {
        postcode: postcode
        slot_id: slot
    }) {
        slot_taken: CollectionSlotGone
    }
    if (booking.id == "") {
        return kept
    }
    return kept
}
"#;

/// No transaction is open while a call is out (area 6, "Outside calls
/// hold no transaction"; 7.0c's guard, which chunk 7.4 restructures): a
/// call before the action's first write goes out, and what the action
/// writes after it commits; a call after a write is refused by `jadpo
/// check` (chunk 7.4b), so checked code never reaches the runtime's
/// refusal, which `capability.rs`'s tests hold to refusing it before it
/// goes out.
#[test]
fn a_call_goes_out_before_the_first_write_and_never_inside_the_transaction() {
    let provider = FakeProvider::start();
    let root = root("kept");
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in kept_files(&provider.url) {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        std::fs::write(path, text).expect("write");
    }
    let terminal = jadpo_term::Environment::empty()
        .with_var("SOURCE_DATE_EPOCH", "1792504800")
        .with_cwd(&root);
    let migrated = jadpo_cli::run(
        &["migrate".to_string(), "new".into(), "initial".into()],
        &terminal,
    );
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    // A call after a write is refused before anything runs.
    let late = root.join("domains/shipping/actions/keep_then_book.jadpo");
    std::fs::write(&late, KEEP_THEN_BOOK).expect("write");
    let checked = jadpo_cli::run(&["check".to_string()], &terminal);
    let printed = format!("{}{}", checked.stdout, checked.stderr);
    assert_eq!(checked.code, 1, "{printed}");
    assert!(
        printed
            .contains("`Collection.create` writes before the call to `couriers.bookings.create`"),
        "{printed}"
    );
    std::fs::remove_file(&late).expect("remove");
    if !jadpo_test::live() {
        return;
    }
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let prepared = Prepared::load(&loaded.db, loaded.project).expect("the project checks");
    let variables = |name: &str| -> Option<String> {
        match name {
            "COURIERS_API_KEY" => Some("ck_test_123".into()),
            "COURIERS_ENVIRONMENT" => Some("sandbox".into()),
            _ => None,
        }
    };
    let values = prepared
        .config(
            &variables,
            jadpo_runtime::config::Mode::Environment,
            &|_: &mut [u8]| false,
        )
        .unwrap_or_else(|problems| panic!("the settings read: {problems:?}"));
    let program = Program::new(prepared).with_config(&values);
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let runtime = Runtime::new().expect("a runtime");
    let store = jadpo_host_native::postgres(&url).expect("a store");
    let driver: Rc<dyn Capability> = Rc::new(JsonHttp::new(NativeFetch::new()));
    let host = Faked::new(jadpo_host_native::NativeHost::new().with_store("main", store))
        .with("couriers", driver);
    provider.answer(
        201,
        r#"{"ok": true, "data": {"id": "b-7", "collectAt": "2026-10-20T09:00:00Z", "window": "morning"}}"#,
    );
    let mut ctx = Ctx::new(program.places(), &host, env());
    let kept = run(
        &runtime,
        &program,
        &mut ctx,
        "book_and_keep",
        vec![text("N1"), text("am")],
    );
    let Outcome::Value(collection) = kept else {
        panic!("{kept:?}");
    };
    assert_eq!(field(&collection, "reference"), &text("b-7"));
    assert_eq!(provider.requests().len(), 1);

    // The action's record was kept.
    let rows = runtime
        .block_on(jadpo_runtime::Database::query_once(
            jadpo_runtime::Host::database(host.host(), "main").expect("main"),
            "SELECT count(*)::int4 FROM \"collection\"",
            &[],
        ))
        .expect("a count");
    assert_eq!(
        rows.first().and_then(|row| row.first()),
        Some(&jadpo_runtime::SqlValue::Int4(1))
    );
}
