//! Wire encodings, decoders and retries of provider calls (area 11,
//! "Decoding" and "Calling"; chunk 7.2c2): a provider package's fields read
//! and written in the built-in encodings its `wire:` names, or by its own
//! `decode:` and `encode:` functions, which the executor runs at the call's
//! site; an operation's whole answer read as its `wire_output:` and turned
//! into its `output:` by its `decode:` function; only the fields code reads
//! decoded; `timed_out:` mapped on an operation safe to send again; and
//! `retry:` sending such a call again when it surely did nothing, or when
//! sending it again is safe.
//!
//! Each test compiles one project whose `couriers` connection reaches a
//! provider started in the test on this machine. Nothing reaches a
//! network.

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

const PACKAGE: &str = "shared/providers/acme_couriers/v1";

fn files(sandbox: &str) -> Vec<(String, String)> {
    let package = |path: &str, text: &str| (format!("{PACKAGE}/{path}"), text.to_string());
    let app = |path: &str, text: &str| (path.to_string(), text.to_string());
    vec![
        app(
            "app.jadpo",
            r#"application {
    connections: {
        couriers: {
            package: AcmeCouriers.v1
            domains: [shipping]
            credential: config.couriers_api_key
            environment: config.couriers_environment
            timeout: 300ms
            retry: {
                attempts: 3
                within: 10s
            }
        }
        quick: {
            package: AcmeCouriers.v1
            domains: [shipping]
            credential: config.couriers_api_key
            environment: config.couriers_environment
            timeout: 300ms
            retry: {
                attempts: 1
            }
        }
    }
}
"#,
        ),
        app(
            "config.jadpo",
            r#"config {
    couriers_api_key: Text {
        secret: true
    }
    couriers_environment: ProviderEnvironment
}
"#,
        ),
        package(
            "package.jadpo",
            r#"/// Acme Couriers' booking and tracking API.
package AcmeCouriers.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
"#,
        ),
        package(
            "provider.jadpo",
            &format!(
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
    }}
    error_code: "error.code"
}}
"#
            ),
        ),
        package(
            "resources/parcels.jadpo",
            r#"public resource parcels {
    get: {
        method: HttpMethod.get
        path: "/v1/parcels/{id}"
        input: ParcelLookup
        output: Parcel
        changes: false
        source: PackageSource.written
    }
    send: {
        method: HttpMethod.post
        path: "/v1/parcels"
        input: NewParcel
        output: Receipt
        changes: true
        idempotent: true
        source: PackageSource.written
    }
    note: {
        method: HttpMethod.post
        path: "/v1/notes"
        input: ParcelLookup
        output: Receipt
        changes: true
        idempotent: false
        source: PackageSource.written
    }
}
"#,
        ),
        package(
            "resources/tracking.jadpo",
            r#"public resource tracking {
    get: {
        method: HttpMethod.get
        path: "/v1/parcels/{id}/tracking"
        input: ParcelLookup
        wire_output: TrackingTable
        output: List<TrackingEvent>
        decode: tables.to_events
        output_at: "data"
        changes: false
        source: PackageSource.written
    }
}
"#,
        ),
        package(
            "types/parcel_lookup.jadpo",
            "/// Which parcel.\npublic type ParcelLookup = Object {\n    id: ParcelRef\n}\n",
        ),
        package(
            "types/receipt.jadpo",
            "/// What the courier gives back.\npublic type Receipt = Object {\n    id: Text\n}\n",
        ),
        package(
            "types/parcel_ref.jadpo",
            "/// The courier's reference for a parcel.\npublic type ParcelRef = Text {\n    max_length: 40\n}\n",
        ),
        package(
            "types/status.jadpo",
            "/// Where a parcel is, in the courier's words.\npublic type Status = Text {\n    max_length: 40\n}\n",
        ),
        package(
            "types/collection_time.jadpo",
            "/// When a parcel is collected.\npublic type CollectionTime = Instant {}\n",
        ),
        package(
            "types/due_date.jadpo",
            "/// The day a parcel is due.\npublic type DueDate = CalendarDate {}\n",
        ),
        package(
            "types/window_code.jadpo",
            "/// A collection window as the courier writes it.\npublic type WindowCode = Text {\n    max_length: 2\n}\n",
        ),
        package(
            "types/pieces.jadpo",
            "/// How many pieces a parcel is in.\npublic type Pieces = Int {\n    min: 0\n    max: 1000\n}\n",
        ),
        package(
            "types/weight.jadpo",
            "/// A parcel's weight in kilograms.\npublic type Weight = Decimal {\n    scale: 2\n    min: 0.0\n    max: 1000.0\n}\n",
        ),
        package(
            "types/parcel_price.jadpo",
            "/// A price in pounds.\npublic type ParcelPrice = Money {\n    currency: Currency.gbp\n}\n",
        ),
        package(
            "types/depot.jadpo",
            "/// Where the courier keeps a parcel.\npublic type Depot = Object {\n    name: Text\n}\n",
        ),
        package(
            "types/slot_window.jadpo",
            "/// When in the day a collection comes.\npublic type SlotWindow = Enum {\n    morning\n    afternoon\n}\n",
        ),
        package(
            "types/tag.jadpo",
            "/// A label on a parcel.\npublic type Tag = Enum {\n    fragile\n    heavy\n    signed\n}\n",
        ),
        package(
            "types/parcel.jadpo",
            r#"/// A parcel and where it is.
public type Parcel = Object {
    id: Text
    status: Status
    collect_at: CollectionTime {
        wire: Wire.unix_seconds
        wire_name: "collectAt"
    }
    scanned_at: Instant {
        wire: Wire.unix_milliseconds
        wire_name: "scannedAt"
    }
    delivered_at: Instant {
        wire: Wire.iso_8601
        wire_name: "deliveredAt"
    }
    due_on: DueDate {
        wire: Wire.day_month_year
        wire_name: "dueOn"
    }
    pieces: Pieces {
        wire: Wire.text
    }
    weight: Weight {
        wire: Wire.number
    }
    tags: List<Tag> {
        wire: Wire.comma_separated
    }
    note: Text {
        wire: Wire.base64
    }
    depot: Depot {
        wire: Wire.json_text
    }
    price: ParcelPrice {
        wire: Wire.minor_units
    }
    slot: SlotWindow {
        decode: slots.parse_window
        encode: slots.format_window
    }
}
"#,
        ),
        package(
            "types/new_parcel.jadpo",
            r#"/// A parcel to send.
public type NewParcel = Object {
    collect_at: CollectionTime {
        wire: Wire.unix_seconds
        wire_name: "collectAt"
    }
    due_on: DueDate {
        wire: Wire.day_month_year
        wire_name: "dueOn"
    }
    weight: Weight {
        wire: Wire.number
    }
    tags: List<Tag> {
        wire: Wire.comma_separated
    }
    note: Text {
        wire: Wire.base64
    }
    depot: Depot {
        wire: Wire.json_text
    }
    price: ParcelPrice {
        wire: Wire.minor_units
    }
    slot: SlotWindow {
        decode: slots.parse_window
        encode: slots.format_window
    }
}
"#,
        ),
        package(
            "types/tracking_table.jadpo",
            r#"/// The courier's tracking history as it sends it.
public type TrackingTable = Object {
    scans: List<Scan>
}
"#,
        ),
        package(
            "types/scan.jadpo",
            r#"/// One scan as the courier sends it.
public type Scan = Object {
    place: Text {
        wire_name: "where"
    }
}
"#,
        ),
        package(
            "types/tracking_event.jadpo",
            "/// Where a parcel was seen.\npublic type TrackingEvent = Object {\n    place: Text\n}\n",
        ),
        package(
            "failures/window_unknown.jadpo",
            r#"/// A collection window the courier names that this package doesn't know.
type WindowUnknown = Failure {
    kind: FailureKind.invalid_value
    message: "The courier named a collection window this package doesn't know."
}
"#,
        ),
        package(
            "libraries/slots.jadpo",
            r#"/// The courier's collection windows, which it writes as "AM" and "PM".
library slots {
    /// The window the courier's code names.
    function parse_window(code: WindowCode) fails WindowUnknown -> SlotWindow {
        if (code == "AM") {
            return SlotWindow.morning
        }
        if (code == "PM") {
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
        ),
        package(
            "libraries/tables.jadpo",
            r#"/// The courier's tracking history, as a list of where a parcel was seen.
library tables {
    /// Each scan as an event.
    function to_events(table: TrackingTable) -> List<TrackingEvent> {
        return list.map(table.scans, scan -> TrackingEvent {
            place: scan.place
        })
    }
}
"#,
        ),
        app(
            "domains/shipping/failures/courier_unavailable.jadpo",
            "type CourierUnavailable = Failure {\n    kind: FailureKind.unavailable\n    message: \"The courier isn't answering.\"\n}\n",
        ),
        app(
            "domains/shipping/actions/track.jadpo",
            r#"use app { couriers }
use shared/providers/acme_couriers

/// Everything the courier knows about a parcel.
action track(id: AcmeCouriers.ParcelRef) -> AcmeCouriers.Parcel {
    return couriers.parcels.get(AcmeCouriers.ParcelLookup {
        id: id
    })
}
"#,
        ),
        app(
            "domains/shipping/actions/status_of.jadpo",
            r#"use app { couriers }
use shared/providers/acme_couriers

/// Only a parcel's status: the other fields are never decoded.
action status_of(id: AcmeCouriers.ParcelRef) -> AcmeCouriers.Status {
    var parcel: AcmeCouriers.Parcel = couriers.parcels.get(AcmeCouriers.ParcelLookup {
        id: id
    })
    return parcel.status
}
"#,
        ),
        app(
            "domains/shipping/actions/collected_at.jadpo",
            r#"use app { couriers }
use shared/providers/acme_couriers

/// When a parcel is collected.
action collected_at(id: AcmeCouriers.ParcelRef) -> AcmeCouriers.CollectionTime {
    var parcel: AcmeCouriers.Parcel = couriers.parcels.get(AcmeCouriers.ParcelLookup {
        id: id
    })
    return parcel.collect_at
}
"#,
        ),
        app(
            "domains/shipping/actions/window_of.jadpo",
            r#"use app { couriers }
use shared/providers/acme_couriers

/// When in the day a parcel is collected.
action window_of(id: AcmeCouriers.ParcelRef) -> AcmeCouriers.SlotWindow {
    var parcel: AcmeCouriers.Parcel = couriers.parcels.get(AcmeCouriers.ParcelLookup {
        id: id
    })
    return parcel.slot
}
"#,
        ),
        app(
            "domains/shipping/actions/send_parcel.jadpo",
            r#"use app { couriers }
use shared/providers/acme_couriers

/// Sends a parcel.
action send_parcel(
    collect_at: AcmeCouriers.CollectionTime,
    due_on: AcmeCouriers.DueDate,
    weight: AcmeCouriers.Weight,
    price: AcmeCouriers.ParcelPrice,
    window: AcmeCouriers.SlotWindow,
    tags: List<AcmeCouriers.Tag>
) -> AcmeCouriers.Receipt {
    return couriers.parcels.send(AcmeCouriers.NewParcel {
        collect_at: collect_at
        due_on: due_on
        weight: weight
        tags: tags
        note: "Leave with the porter"
        depot: AcmeCouriers.Depot {
            name: "North"
        }
        price: price
        slot: window
    })
}
"#,
        ),
        app(
            "domains/shipping/actions/history.jadpo",
            r#"use app { couriers }
use shared/providers/acme_couriers

/// Where a parcel has been.
action history(id: AcmeCouriers.ParcelRef) -> List<AcmeCouriers.TrackingEvent> {
    return couriers.tracking.get(AcmeCouriers.ParcelLookup {
        id: id
    })
}
"#,
        ),
        app(
            "domains/shipping/actions/note_parcel.jadpo",
            r#"use app { couriers }
use shared/providers/acme_couriers

/// Leaves a note on a parcel: a change with no key.
action note_parcel(id: AcmeCouriers.ParcelRef) -> AcmeCouriers.Receipt {
    return couriers.parcels.note(AcmeCouriers.ParcelLookup {
        id: id
    })
}
"#,
        ),
        app(
            "domains/shipping/actions/status_or_wait.jadpo",
            r#"use app { quick }
use shipping/failures { CourierUnavailable }
use shared/providers/acme_couriers

/// A parcel's status, mapping a courier that doesn't answer in time to
/// the shop's own failure.
action status_or_wait(id: AcmeCouriers.ParcelRef) fails CourierUnavailable -> AcmeCouriers.Status {
    var parcel: AcmeCouriers.Parcel = attempt quick.parcels.get(AcmeCouriers.ParcelLookup {
        id: id
    }) {
        timed_out: CourierUnavailable
    }
    return parcel.status
}
"#,
        ),
        app(
            "domains/shipping/actions/quick_status.jadpo",
            r#"use app { quick }
use shared/providers/acme_couriers

/// A parcel's status, tried once.
action quick_status(id: AcmeCouriers.ParcelRef) -> AcmeCouriers.Status {
    var parcel: AcmeCouriers.Parcel = quick.parcels.get(AcmeCouriers.ParcelLookup {
        id: id
    })
    return parcel.status
}
"#,
        ),
    ]
}

/// The project, checked, with the couriers' sandbox at `sandbox`.
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
            "the project checks: {:#?}",
            problems
                .iter()
                .map(|p| format!(
                    "{} at {:?}",
                    p.message,
                    p.primary.as_ref().map(|label| &label.span)
                ))
                .collect::<Vec<_>>()
        )
    });
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
    Program::new(prepared).with_config(&values)
}

fn root(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("jadpo-7.2c2-{name}-{}", std::process::id()))
}

/// The package driver over the native host's HTTP. The host has no
/// timer, so a retry is sent again at once.
fn host() -> Faked<MemoryHost> {
    let driver: Rc<dyn Capability> = Rc::new(JsonHttp::new(NativeFetch));
    Faked::new(MemoryHost::new())
        .with("couriers", Rc::clone(&driver))
        .with("quick", driver)
}

fn env() -> Env {
    Env {
        seed: Some(7),
        clock: Clock::TEST,
    }
}

/// Runs action `name` with `args`.
fn run(runtime: &Runtime, program: &Program, name: &str, args: Vec<Value>) -> Outcome {
    let host = host();
    let mut ctx = Ctx::of(program, &host, env());
    ctx.set_call_key("user:ada", "client-key-1");
    let id = program.plan().find(name).expect("the action");
    runtime.block_on(program.run(&mut ctx, id, args))
}

/// The tags a parcel is sent with.
fn tags() -> Value {
    Value::list(vec![
        Value::variant("Tag", 0, "fragile", Vec::new()),
        Value::variant("Tag", 2, "signed", Vec::new()),
    ])
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

fn fault(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Fault(fault) => match &fault.kind {
            FaultKind::Provider(what) => what.clone(),
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    }
}

/// An answer in every built-in encoding.
const PARCEL: &str = r#"{
    "id": "p-1",
    "status": "out_for_delivery",
    "collectAt": 1792400400,
    "scannedAt": 1792400400250,
    "deliveredAt": "2026-10-20T10:00:00+01:00",
    "dueOn": "21/10/2026",
    "pieces": "3",
    "weight": 2.5,
    "tags": "fragile,heavy",
    "note": "TGVhdmUgd2l0aCB0aGUgcG9ydGVy",
    "depot": "{\"name\": \"North\"}",
    "price": 1250,
    "slot": "PM",
    "courier": "Sam"
}"#;

/// Each built-in encoding is read from the provider's answer into the
/// package's types, and a field's own decoder is run by the executor at
/// the call's site.
#[test]
fn an_answer_is_read_in_its_fields_encodings() {
    let provider = FakeProvider::start();
    let program = program(&root("read"), &provider.url);
    let runtime = Runtime::new().expect("a runtime");
    provider.answer(200, PARCEL);
    let outcome = run(&runtime, &program, "track", vec![text("p-1")]);
    let Outcome::Value(parcel) = outcome else {
        panic!("{outcome:?}");
    };
    let shown = |name: &str| field(&parcel, name).to_string();
    assert_eq!(shown("collect_at"), "\"2026-10-19T09:00:00Z\"");
    assert_eq!(shown("scanned_at"), "\"2026-10-19T09:00:00.250Z\"");
    assert_eq!(shown("delivered_at"), "\"2026-10-20T09:00:00Z\"");
    assert_eq!(shown("due_on"), "\"2026-10-21\"");
    assert_eq!(field(&parcel, "pieces"), &Value::Int(3));
    assert_eq!(shown("weight"), "2.50");
    assert_eq!(shown("tags"), "[Tag.fragile, Tag.heavy]");
    assert_eq!(field(&parcel, "note"), &text("Leave with the porter"));
    assert_eq!(field(field(&parcel, "depot"), "name"), &text("North"));
    assert_eq!(shown("price"), "money.from_minor(1250, Currency.gbp)");
    match field(&parcel, "slot") {
        Value::Variant(variant) => assert_eq!(&*variant.name, "afternoon"),
        other => panic!("{other:?}"),
    }
}

/// The input is written in its fields' encodings, a field's own encoder
/// run by the executor.
#[test]
fn a_request_is_written_in_its_fields_encodings() {
    let provider = FakeProvider::start();
    let program = program(&root("write"), &provider.url);
    let runtime = Runtime::new().expect("a runtime");
    provider.answer(201, r#"{"id": "r-1"}"#);
    let collect_at = Value::Instant(1_792_400_400_000);
    let due_on =
        match jadpo_value::standard::builtin_text(jadpo_std::Builtin::CalendarDate, "2026-10-21") {
            Some(day) => day,
            None => panic!("a date"),
        };
    let weight = Value::decimal("2.50").expect("a decimal");
    let price =
        jadpo_value::standard::builtin_text(jadpo_std::Builtin::Money, "GBP 12.50").expect("money");
    let window = Value::variant("SlotWindow", 0, "morning", Vec::new());
    let outcome = run(
        &runtime,
        &program,
        "send_parcel",
        vec![collect_at, due_on, weight, price, window, tags()],
    );
    assert!(matches!(outcome, Outcome::Value(_)), "{outcome:?}");
    let sent = provider.requests();
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0].json(),
        serde_json::json!({
            "collectAt": 1792400400,
            "dueOn": "21/10/2026",
            "weight": 2.50,
            "tags": "fragile,signed",
            "note": "TGVhdmUgd2l0aCB0aGUgcG9ydGVy",
            "depot": "{\"name\":\"North\"}",
            "price": 1250,
            "slot": "AM"
        })
    );
    assert!(sent[0].body.contains("\"weight\":2.50"), "{}", sent[0].body);
}

/// Only the fields code reads are decoded: an action reading a parcel's
/// status succeeds though the answer's other fields would never decode,
/// and its field decoder never runs; one reading a field that doesn't
/// decode is a fault naming the operation, the field and the rule, never
/// the value.
#[test]
fn only_the_fields_code_reads_are_decoded() {
    let provider = FakeProvider::start();
    let program = program(&root("lazy"), &provider.url);
    let runtime = Runtime::new().expect("a runtime");
    let broken = r#"{"id": "p-1", "status": "held", "collectAt": "soon", "slot": "NOON"}"#;
    provider.answer(200, broken);
    provider.answer(200, broken);
    provider.answer(200, broken);
    let status = run(&runtime, &program, "status_of", vec![text("p-1")]);
    assert_eq!(status, Outcome::Value(text("held")), "{status:?}");
    let collected = fault(&run(&runtime, &program, "collected_at", vec![text("p-1")]));
    assert!(
        collected.contains("`couriers.parcels.get`'s answer doesn't decode")
            && collected.contains("collectAt"),
        "{collected}"
    );
    assert!(!collected.contains("soon"), "never the value: {collected}");
    let window = fault(&run(&runtime, &program, "window_of", vec![text("p-1")]));
    assert!(
        window.contains("`couriers.parcels.get`'s answer doesn't decode")
            && window.contains("slot"),
        "{window}"
    );
    assert!(!window.contains("NOON"), "never the value: {window}");
}

/// An operation's whole answer is read as its `wire_output:`, from inside
/// its envelope, and turned into its `output:` by its decoder.
#[test]
fn a_whole_answer_is_decoded_by_the_packages_function() {
    let provider = FakeProvider::start();
    let program = program(&root("whole"), &provider.url);
    let runtime = Runtime::new().expect("a runtime");
    provider.answer(
        200,
        r#"{"data": {"scans": [{"where": "Depot"}, {"where": "Van", "at": 3}]}}"#,
    );
    let outcome = run(&runtime, &program, "history", vec![text("p-1")]);
    let Outcome::Value(events) = outcome else {
        panic!("{outcome:?}");
    };
    let places: Vec<Value> = events
        .items()
        .expect("a list")
        .iter()
        .map(|event| field(event, "place").clone())
        .collect();
    assert_eq!(places, vec![text("Depot"), text("Van")]);
    assert_eq!(provider.requests()[0].path, "/v1/parcels/p-1/tracking");
}

/// A call that may be sent again is retried, under the same key, while
/// its `retry:` lasts: a server's error on a safe or keyed call, and a
/// refusal that did nothing on any call; a server's error on a change
/// with no key is never sent again.
#[test]
fn calls_safe_to_repeat_are_retried() {
    let provider = FakeProvider::start();
    let program = program(&root("retry"), &provider.url);
    let runtime = Runtime::new().expect("a runtime");
    let parcel = r#"{"id": "p-1", "status": "held"}"#;
    // A safe read: a server's error, then the answer.
    provider.answer(503, "");
    provider.answer(200, parcel);
    let read = run(&runtime, &program, "status_of", vec![text("p-1")]);
    assert_eq!(read, Outcome::Value(text("held")));
    assert_eq!(provider.requests().len(), 2);
    // A keyed change: sent again under the same key.
    provider.answer(502, "");
    provider.answer(201, r#"{"id": "r-1"}"#);
    let sent = run(
        &runtime,
        &program,
        "send_parcel",
        vec![
            Value::Instant(1_792_400_400_000),
            jadpo_value::standard::builtin_text(jadpo_std::Builtin::CalendarDate, "2026-10-21")
                .expect("a date"),
            Value::decimal("1").expect("a decimal"),
            jadpo_value::standard::builtin_text(jadpo_std::Builtin::Money, "GBP 1.00")
                .expect("money"),
            Value::variant("SlotWindow", 1, "afternoon", Vec::new()),
            tags(),
        ],
    );
    assert!(matches!(sent, Outcome::Value(_)), "{sent:?}");
    let requests = provider.requests();
    assert_eq!(requests.len(), 4);
    assert_eq!(
        requests[2].header("idempotency-key"),
        requests[3].header("idempotency-key")
    );
    assert!(requests[2].header("idempotency-key").is_some());
    // A change with no key: a server's error may have acted, so it isn't
    // sent again.
    provider.answer(500, "");
    let unkeyed = run(&runtime, &program, "note_parcel", vec![text("p-1")]);
    assert!(
        matches!(&unkeyed, Outcome::Failed(failure) if failure.code == "outcome_unknown"),
        "{unkeyed:?}"
    );
    assert_eq!(provider.requests().len(), 5);
    // The provider said it did nothing: sent again even with no key.
    provider.answer(429, "");
    provider.answer(201, r#"{"id": "n-1"}"#);
    let noted = run(&runtime, &program, "note_parcel", vec![text("p-1")]);
    assert!(matches!(noted, Outcome::Value(_)), "{noted:?}");
    assert_eq!(provider.requests().len(), 7);
    // Every attempt spent: `unavailable`.
    for _ in 0..3 {
        provider.answer(503, "");
    }
    let spent = run(&runtime, &program, "status_of", vec![text("p-1")]);
    assert!(
        matches!(&spent, Outcome::Failed(failure) if failure.code == "unavailable"),
        "{spent:?}"
    );
    assert_eq!(provider.requests().len(), 10);
    // A connection tried once isn't sent again.
    provider.answer(503, "");
    let once = run(&runtime, &program, "quick_status", vec![text("p-1")]);
    assert!(
        matches!(&once, Outcome::Failed(failure) if failure.code == "unavailable"),
        "{once:?}"
    );
    assert_eq!(provider.requests().len(), 11);
}

/// A call safe to send again that gets no answer in time is mapped by
/// `timed_out:` at the call to the app's own `unavailable` failure, with
/// the runtime's `timed_out` as its cause; unmapped, it is `unavailable`.
#[test]
fn a_timeout_is_mapped_at_the_call() {
    let provider = FakeProvider::start();
    let program = program(&root("timeout"), &provider.url);
    let runtime = Runtime::new().expect("a runtime");
    provider.answer_after(1500, 200, r#"{"id": "p-1", "status": "held"}"#);
    let mapped = run(&runtime, &program, "status_or_wait", vec![text("p-1")]);
    match &mapped {
        Outcome::Failed(failure) => {
            assert_eq!(failure.code, "courier_unavailable");
            assert_eq!(
                failure.cause.as_ref().map(|cause| cause.code.as_str()),
                Some("timed_out")
            );
        }
        other => panic!("{other:?}"),
    }
    provider.answer_after(1500, 200, r#"{"id": "p-1", "status": "held"}"#);
    let unmapped = run(&runtime, &program, "quick_status", vec![text("p-1")]);
    assert!(
        matches!(&unmapped, Outcome::Failed(failure) if failure.code == "unavailable"),
        "{unmapped:?}"
    );
}

// --- what the checker refuses ---------------------------------------------------

/// The project with `changed` files written over it, checked: each
/// problem's rule and headline.
fn problems(name: &str, changed: &[(&str, &str)]) -> Vec<(String, String)> {
    let root = root(name);
    let _ = std::fs::remove_dir_all(&root);
    let mut all = files("https://sandbox.acme-couriers.example");
    for (path, text) in changed {
        let path = (*path).to_string();
        all.retain(|(held, _)| *held != path);
        all.push((path, (*text).to_string()));
    }
    for (path, text) in all {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        std::fs::write(path, text).expect("write");
    }
    let loaded = jadpo_db::load(&root);
    match Prepared::load(&loaded.db, loaded.project) {
        Ok(_) => Vec::new(),
        Err(problems) => problems
            .iter()
            .map(|problem| (problem.rule.id.to_string(), problem.message.clone()))
            .collect(),
    }
}

/// One problem with `rule`, whose headline holds `says`, and none besides
/// a function left unused by the mistake.
fn refused(name: &str, changed: &[(&str, &str)], rule: &str, says: &str) {
    let found = problems(name, changed);
    let matching = found
        .iter()
        .filter(|(id, message)| id == rule && message.contains(says))
        .count();
    let others = found
        .iter()
        .filter(|(id, _)| id != rule && id != "layout.unused_declaration")
        .count();
    assert!(
        matching == 1 && others == 0,
        "{name}: expected one `{rule}` saying {says:?}, got {found:#?}"
    );
}

/// A package type nothing builds, where each case writes its field.
const DEPOT: &str = "shared/providers/acme_couriers/v1/types/shelf.jadpo";

/// `wire:`, `decode:` and `encode:` say how a provider package's field is
/// sent, and only that: one way, in an encoding that holds the field's
/// type, or with the package's own functions of the right shape.
#[test]
fn a_fields_wire_is_held_to_what_it_can_say() {
    refused(
        "wire-unknown",
        &[(
            DEPOT,
            "/// Where the courier shelves a parcel.\npublic type Shelf = Object {\n    name: Text\n    opened: Instant {\n        wire: Wire.unix_secs\n    }\n}\n",
        )],
        "package.wire_invalid",
        "`Wire.unix_secs` isn't one of the built-in encodings",
    );
    refused(
        "wire-misfit",
        &[(
            DEPOT,
            "/// Where the courier shelves a parcel.\npublic type Shelf = Object {\n    name: Text {\n        wire: Wire.day_month_year\n    }\n}\n",
        )],
        "package.wire_invalid",
        "`Wire.day_month_year` sends a `CalendarDate`, and `name` is a `Text`",
    );
    refused(
        "wire-currency",
        &[(
            DEPOT,
            "/// Where the courier shelves a parcel.\npublic type Shelf = Object {\n    name: Text\n    fee: Money {\n        wire: Wire.minor_units\n    }\n}\n",
        )],
        "package.wire_invalid",
        "`Wire.minor_units` needs `fee`'s one currency",
    );
    refused(
        "wire-both",
        &[(
            DEPOT,
            "/// Where the courier shelves a parcel.\npublic type Shelf = Object {\n    name: Text\n    slot: SlotWindow {\n        wire: Wire.text\n        decode: slots.parse_window\n        encode: slots.format_window\n    }\n}\n",
        )],
        "package.wire_invalid",
        "`slot` gives both `wire:` and `decode:`",
    );
    refused(
        "wire-half",
        &[(
            DEPOT,
            "/// Where the courier shelves a parcel.\npublic type Shelf = Object {\n    name: Text\n    slot: SlotWindow {\n        decode: slots.parse_window\n    }\n}\n",
        )],
        "package.wire_invalid",
        "`slot` gives `decode:` without `encode:`",
    );
    refused(
        "wire-no-function",
        &[(
            DEPOT,
            "/// Where the courier shelves a parcel.\npublic type Shelf = Object {\n    name: Text\n    slot: SlotWindow {\n        decode: slots.read_window\n        encode: slots.format_window\n    }\n}\n",
        )],
        "package.wire_invalid",
        "`slots.read_window` isn't one of the package's functions",
    );
    refused(
        "wire-wrong-shape",
        &[(
            DEPOT,
            "/// Where the courier shelves a parcel.\npublic type Shelf = Object {\n    name: Text\n    slot: Pieces {\n        decode: slots.parse_window\n        encode: slots.format_window\n    }\n}\n",
        )],
        "package.wire_invalid",
        "`slots.parse_window` can't decode `slot`",
    );
    refused(
        "wire-app",
        &[(
            "domains/shipping/types/shelf.jadpo",
            "/// Where the shop keeps a parcel.\ntype Shelf = Object {\n    at: Instant {\n        wire: Wire.unix_seconds\n    }\n}\n",
        )],
        "package.wire_invalid",
        "`wire` is for a provider package's fields",
    );
}

/// An operation decodes its whole answer with `wire_output:` and
/// `decode:` together, a function from the one to its `output:`.
#[test]
fn a_whole_answer_decoder_is_held_to_its_types() {
    let tracking = |body: &str| {
        format!(
            "public resource tracking {{\n    get: {{\n        method: HttpMethod.get\n        path: \"/v1/parcels/{{id}}/tracking\"\n        input: ParcelLookup\n{body}        changes: false\n        source: PackageSource.written\n    }}\n}}\n"
        )
    };
    let path = "shared/providers/acme_couriers/v1/resources/tracking.jadpo";
    let half =
        tracking("        wire_output: TrackingTable\n        output: List<TrackingEvent>\n");
    refused(
        "whole-half",
        &[(path, &half)],
        "package.operation_invalid",
        "`tracking.get` gives `wire_output:` without `decode:`",
    );
    let wrong = tracking(
        "        wire_output: TrackingTable\n        output: List<TrackingEvent>\n        decode: slots.parse_window\n",
    );
    refused(
        "whole-wrong",
        &[(path, &wrong)],
        "package.operation_invalid",
        "`slots.parse_window` can't decode `tracking.get`'s answer",
    );
}

/// `timed_out:` is mapped only on an operation safe to send again, to an
/// `unavailable` failure; `retry:` holds 1 to 10 attempts within up to
/// five minutes.
#[test]
fn timed_out_and_retry_are_held_to_what_is_safe() {
    refused(
        "timed-out-unkeyed",
        &[(
            "domains/shipping/actions/note_parcel.jadpo",
            r#"use app { couriers }
use shipping/failures { CourierUnavailable }
use shared/providers/acme_couriers

/// Leaves a note on a parcel.
action note_parcel(id: AcmeCouriers.ParcelRef) fails CourierUnavailable -> AcmeCouriers.Receipt {
    return attempt couriers.parcels.note(AcmeCouriers.ParcelLookup {
        id: id
    }) {
        timed_out: CourierUnavailable
    }
}
"#,
        )],
        "failure.unknown_mapping",
        "`couriers.parcels.note(...)` can't map `timed_out:`",
    );
    refused(
        "timed-out-kind",
        &[(
            "domains/shipping/failures/courier_unavailable.jadpo",
            "type CourierUnavailable = Failure {\n    kind: FailureKind.conflict\n    message: \"The courier isn't answering.\"\n}\n",
        )],
        "failure.mapping_kind",
        "`timed_out:` needs an `unavailable` failure",
    );
    refused(
        "retry-attempts",
        &[(
            "app.jadpo",
            r#"application {
    connections: {
        couriers: {
            package: AcmeCouriers.v1
            domains: [shipping]
            credential: config.couriers_api_key
            environment: config.couriers_environment
            retry: {
                attempts: 50
            }
        }
        quick: {
            package: AcmeCouriers.v1
            domains: [shipping]
            credential: config.couriers_api_key
            environment: config.couriers_environment
        }
    }
}
"#,
        )],
        "connection.value_invalid",
        "`couriers`'s `retry: { attempts: }` must be from 1 to 10",
    );
}
