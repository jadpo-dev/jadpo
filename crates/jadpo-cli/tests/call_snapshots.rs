//! Outside calls hold no transaction (area 6, "Outside calls hold no
//! transaction"; area 11, "Calling"; chunk 7.4), live against PostgreSQL:
//! an action reads a shipment, books its collection through a provider
//! started in the test, then labels the shipment with what it read and
//! what the courier answered. What it read before the call is a snapshot,
//! closed before the call goes out; its write runs in a transaction opened
//! after the call, which holds the shipment to the row version it was
//! loaded at; and a change made while the call is out runs the whole
//! action again, the call repeated under the same key.
//!
//! The provider holds each request while the test looks at the database
//! from another connection: no transaction of the action's is open then.
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use jadpo_drivers::package::JsonHttp;
use jadpo_host_native::Runtime;
use jadpo_host_native::fetch::NativeFetch;
use jadpo_interp::{Executor, Outcome, Prepared, Program};
use jadpo_runtime::capability::{Capability, Faked};
use jadpo_runtime::{Clock, Ctx, Database, Env, Host, SqlValue};
use jadpo_value::Value;

use crate::support::fake_provider::FakeProvider;

/// The courier's package, at `sandbox`, and a shipping domain that keeps
/// shipments.
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
            "shared/providers/acme_couriers/v1/package.jadpo",
            r#"/// Acme Couriers' booking API.
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
            "/// A booked collection.\npublic type Booking = Object {\n    id: Text\n}\n"
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
            "domains/shipping/types/note.jadpo",
            "/// What the warehouse wrote about a shipment.\ntype Note = Text {\n    max_length: 40\n}\n"
                .to_string(),
        ),
        (
            "domains/shipping/types/label.jadpo",
            "/// What is printed on a shipment.\ntype Label = Text {\n    max_length: 80\n}\n"
                .to_string(),
        ),
        (
            "domains/shipping/failures/collection_slot_gone.jadpo",
            "type CollectionSlotGone = Failure {\n    kind: FailureKind.conflict\n    message: \"That collection slot has gone.\"\n}\n"
                .to_string(),
        ),
        (
            "domains/shipping/failures/label_invalid.jadpo",
            "type LabelInvalid = Failure {\n    kind: FailureKind.invalid_value\n    message: \"The label is too long to print.\"\n}\n"
                .to_string(),
        ),
        (
            "domains/shipping/entities/shipment.jadpo",
            r#"use shipping/types { Label, Note }

/// A parcel waiting for collection.
entity Shipment {
    id: Uuid
    note: Note
    label: Label?

    policy: {
        create: [Access.public, Access.system]
        read: [Access.public, Access.system]
        update: [Access.public, Access.system]
    }
}
"#
            .to_string(),
        ),
        (
            "domains/shipping/entities/pickup.jadpo",
            r#"use shipping/entities { Shipment }

/// A collection booked for a shipment.
entity Pickup {
    id: Uuid
    shipment_id: Shipment.id {
        on_delete: OnDelete.restricted
    }
    booking: Text

    policy: {
        create: [Access.public, Access.system]
        read: [Access.public, Access.system]
    }
}
"#
            .to_string(),
        ),
        (
            "domains/shipping/actions/book_pickup.jadpo",
            r#"use app { couriers }
use shipping/entities { Pickup, Shipment, ShipmentNotFound }
use shipping/failures { CollectionSlotGone }
use shared/providers/acme_couriers

/// Reads a shipment, books its collection, then keeps a pickup that
/// refers to the shipment it read.
action book_pickup(shipment_id: Shipment.id, postcode: AcmeCouriers.Postcode, slot: AcmeCouriers.SlotId) fails ShipmentNotFound, CollectionSlotGone -> Pickup {
    var shipment: Shipment = attempt Shipment.get(shipment_id)
    var booking: AcmeCouriers.Booking = attempt couriers.bookings.create(AcmeCouriers.CreateBooking {
        postcode: postcode
        slot_id: slot
    }) {
        slot_taken: CollectionSlotGone
    }
    return Pickup.create {
        set: {
            shipment_id: shipment.id
            booking: booking.id
        }
    }
}
"#
            .to_string(),
        ),
        (
            "domains/shipping/actions/add_shipment.jadpo",
            r#"use shipping/entities { Shipment }

/// Adds a shipment.
action add_shipment(note: Shipment.note) -> Shipment {
    return Shipment.create {
        set: {
            note: note
            label: none
        }
    }
}
"#
            .to_string(),
        ),
        (
            "domains/shipping/actions/book_shipment.jadpo",
            r#"use app { couriers }
use shipping/entities { Shipment, ShipmentNotFound }
use shipping/failures { CollectionSlotGone, LabelInvalid }
use shipping/types { Label }
use shared/providers/acme_couriers

/// Reads a shipment, books its collection, then labels it with its note
/// and the booking: the read comes before the call, the write after it.
action book_shipment(shipment_id: Shipment.id, postcode: AcmeCouriers.Postcode, slot: AcmeCouriers.SlotId) fails ShipmentNotFound, CollectionSlotGone, LabelInvalid -> Shipment {
    var shipment: Shipment = attempt Shipment.get(shipment_id)
    var booking: AcmeCouriers.Booking = attempt couriers.bookings.create(AcmeCouriers.CreateBooking {
        postcode: postcode
        slot_id: slot
    }) {
        slot_taken: CollectionSlotGone
    }
    var label: Label = attempt Label(shipment.note + " " + booking.id) {
        invalid: LabelInvalid
    }
    return Shipment.update(shipment) {
        set: {
            label: label
        }
    }
}
"#
            .to_string(),
        ),
    ]
}

fn root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("jadpo-7.4-{name}-{}", std::process::id()))
}

/// Writes the project at `root` and its first migration.
fn project(root: &Path, sandbox: &str) {
    let _ = std::fs::remove_dir_all(root);
    for (path, text) in files(sandbox) {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        std::fs::write(path, text).expect("write");
    }
    let terminal = jadpo_term::Environment::empty()
        .with_var("SOURCE_DATE_EPOCH", "1792504800")
        .with_cwd(root);
    let migrated = jadpo_cli::run(
        &["migrate".to_string(), "new".into(), "initial".into()],
        &terminal,
    );
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
}

/// The project's program, with the couriers' sandbox chosen.
fn program(root: &Path) -> Program {
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

/// Runs action `name` with `args` as the request `request` would, whose
/// id its provider calls' keys are derived from (area 11, "Calling").
fn run(
    runtime: &Runtime,
    program: &Program,
    host: &dyn Host,
    request: &str,
    name: &str,
    args: Vec<Value>,
) -> Outcome {
    let id = program.plan().find(name).expect("the action");
    // Each request its own seed, so the records it makes have their own
    // identities.
    let seed = request
        .bytes()
        .fold(7u64, |seed, byte| seed * 31 + u64::from(byte));
    let env = Env {
        seed: Some(seed),
        clock: Clock::TEST,
    };
    let mut ctx = Ctx::new(program.places(), host, env);
    ctx.set_call_key("request", request);
    runtime.block_on(program.run(&mut ctx, id, args))
}

fn field<'v>(value: &'v Value, name: &str) -> &'v Value {
    match value {
        Value::Record(record) => record.get(name).expect("the field"),
        other => panic!("not a record: {other:?}"),
    }
}

/// One statement on its own, from another connection.
fn once(url: &str, sql: &str) -> Vec<Vec<SqlValue>> {
    let runtime = Runtime::new().expect("a runtime");
    let store = jadpo_host_native::postgres(url).expect("a store");
    runtime
        .block_on(store.query_once(sql, &[]))
        .unwrap_or_else(|error| panic!("{sql}: {error:?}"))
}

/// How many transactions other than its own are open on the database
/// `url` names by a client: the runtime's. The server's own workers are
/// left out, since an autovacuum worker visiting the database holds a
/// transaction of its own while it looks.
const OPEN: &str = "SELECT count(*)::int4 FROM pg_stat_activity WHERE datname = current_database() AND pid <> pg_backend_pid() AND backend_type = 'client backend' AND xact_start IS NOT NULL";

/// No transaction is open while a provider call is out, and a change made
/// to what the action read while the call was out runs the action again,
/// the call repeated under its key (roadmap 7.4's "Done when").
#[test]
fn a_call_holds_no_transaction_and_a_change_meanwhile_reruns_the_action() {
    let provider = FakeProvider::start();
    let root = root("rerun");
    project(&root, &provider.url);
    if !jadpo_test::live() {
        return;
    }
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = program(&root);
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let runtime = Runtime::new().expect("a runtime");
    let store = jadpo_host_native::postgres(&url).expect("a store");
    let driver: Rc<dyn Capability> = Rc::new(JsonHttp::new(NativeFetch));
    let host = Faked::new(jadpo_host_native::NativeHost::new().with_store("main", store))
        .with("couriers", driver);

    let added = run(
        &runtime,
        &program,
        &host,
        "r1",
        "add_shipment",
        vec![Value::text("Plain")],
    );
    let Outcome::Value(plain) = added else {
        panic!("{added:?}");
    };
    let added = run(
        &runtime,
        &program,
        &host,
        "r2",
        "add_shipment",
        vec![Value::text("Boxed")],
    );
    let Outcome::Value(boxed) = added else {
        panic!("{added:?}");
    };
    let Value::Uuid(boxed_id) = field(&boxed, "id").clone() else {
        panic!("an id");
    };

    // While each request is held: how many transactions are open, and,
    // the first time it is asked to, another request's change to the
    // shipment the action read.
    let open_while_out: Arc<Mutex<Vec<i32>>> = Arc::default();
    let change: Arc<Mutex<Option<String>>> = Arc::default();
    {
        let open_while_out = Arc::clone(&open_while_out);
        let change = Arc::clone(&change);
        let url = url.clone();
        provider.hold(move |_| {
            let open = once(&url, OPEN);
            if let Some(SqlValue::Int4(n)) = open.first().and_then(|row| row.first()) {
                open_while_out.lock().expect("open").push(*n);
            }
            if let Some(sql) = change.lock().expect("change").take() {
                once(&url, &sql);
            }
        });
    }

    // Nothing changes while the call is out: one call, and the write
    // after it keeps what the action read.
    provider.answer(201, r#"{"data": {"id": "b-7"}}"#);
    let booked = run(
        &runtime,
        &program,
        &host,
        "r3",
        "book_shipment",
        vec![
            field(&plain, "id").clone(),
            Value::text("N1"),
            Value::text("am"),
        ],
    );
    let Outcome::Value(booked) = booked else {
        panic!("{booked:?}");
    };
    assert_eq!(field(&booked, "label"), &Value::text("Plain b-7"));
    assert_eq!(provider.requests().len(), 1);
    assert_eq!(
        *open_while_out.lock().expect("open"),
        [0],
        "the read's snapshot closed before the call went out"
    );

    // Another request changes the shipment while the call is out: the
    // write after the call finds its row changed since it was loaded, and
    // the whole action runs again, reading the change and sending the
    // call again under the same key.
    *change.lock().expect("change") = Some(format!(
        "UPDATE \"shipment\" SET \"note\" = 'Fragile' WHERE \"id\" = '{boxed_id}'"
    ));
    provider.answer(201, r#"{"data": {"id": "b-8"}}"#);
    provider.answer(201, r#"{"data": {"id": "b-8"}}"#);
    let booked = run(
        &runtime,
        &program,
        &host,
        "r4",
        "book_shipment",
        vec![Value::uuid(&boxed_id), Value::text("N1"), Value::text("pm")],
    );
    let Outcome::Value(booked) = booked else {
        panic!("{booked:?}");
    };
    assert_eq!(
        field(&booked, "label"),
        &Value::text("Fragile b-8"),
        "the rerun read the change made while the call was out"
    );
    let requests = provider.requests();
    assert_eq!(requests.len(), 3, "the call went out again on the rerun");
    let key = |at: usize| requests[at].header("idempotency-key").map(str::to_string);
    assert!(key(1).is_some());
    assert_eq!(key(1), key(2), "repeated under its key");
    assert_ne!(key(0), key(1), "a different call's key differs");
    assert_eq!(
        *open_while_out.lock().expect("open"),
        [0, 0, 0],
        "no transaction open while any call was out"
    );
    let rows = once(
        &url,
        &format!("SELECT \"note\", \"label\" FROM \"shipment\" WHERE \"id\" = '{boxed_id}'"),
    );
    assert_eq!(
        rows,
        [vec![
            SqlValue::Text("Fragile".into()),
            SqlValue::Text("Fragile b-8".into())
        ]],
        "the change kept, and the label written from it"
    );
}

/// A record deleted while the call is out is a change like any other: the
/// action runs again, and its fresh read finds the record gone, whether
/// the action writes the record or a record referring to it.
#[test]
fn a_record_deleted_while_the_call_is_out_reruns_the_action() {
    let provider = FakeProvider::start();
    let root = root("deleted");
    project(&root, &provider.url);
    if !jadpo_test::live() {
        return;
    }
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = program(&root);
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let runtime = Runtime::new().expect("a runtime");
    let store = jadpo_host_native::postgres(&url).expect("a store");
    let driver: Rc<dyn Capability> = Rc::new(JsonHttp::new(NativeFetch));
    let host = Faked::new(jadpo_host_native::NativeHost::new().with_store("main", store))
        .with("couriers", driver);
    let added = run(
        &runtime,
        &program,
        &host,
        "r1",
        "add_shipment",
        vec![Value::text("Gone")],
    );
    let Outcome::Value(gone) = added else {
        panic!("{added:?}");
    };
    let Value::Uuid(gone_id) = field(&gone, "id").clone() else {
        panic!("an id");
    };
    let delete: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(Some(format!(
        "DELETE FROM \"shipment\" WHERE \"id\" = '{gone_id}'"
    ))));
    {
        let url = url.clone();
        let delete = Arc::clone(&delete);
        provider.hold(move |_| {
            if let Some(sql) = delete.lock().expect("delete").take() {
                once(&url, &sql);
            }
        });
    }
    provider.answer(201, r#"{"data": {"id": "b-9"}}"#);
    let booked = run(
        &runtime,
        &program,
        &host,
        "r2",
        "book_shipment",
        vec![Value::uuid(&gone_id), Value::text("N1"), Value::text("am")],
    );
    assert!(
        matches!(&booked, Outcome::Failed(failure) if failure.code == "shipment_not_found"),
        "{booked:?}"
    );
    assert_eq!(
        provider.requests().len(),
        1,
        "the rerun stopped at its read"
    );

    // The action refers to the shipment it read rather than writing it.
    let added = run(
        &runtime,
        &program,
        &host,
        "r3",
        "add_shipment",
        vec![Value::text("Kept")],
    );
    let Outcome::Value(kept) = added else {
        panic!("{added:?}");
    };
    let Value::Uuid(kept_id) = field(&kept, "id").clone() else {
        panic!("an id");
    };
    *delete.lock().expect("delete") = Some(format!(
        "DELETE FROM \"shipment\" WHERE \"id\" = '{kept_id}'"
    ));
    provider.answer(201, r#"{"data": {"id": "b-10"}}"#);
    let booked = run(
        &runtime,
        &program,
        &host,
        "r4",
        "book_pickup",
        vec![Value::uuid(&kept_id), Value::text("N1"), Value::text("am")],
    );
    assert!(
        matches!(&booked, Outcome::Failed(failure) if failure.code == "shipment_not_found"),
        "{booked:?}"
    );
    assert_eq!(provider.requests().len(), 2);
    let pickups = once(&url, "SELECT count(*)::int4 FROM \"pickup\"");
    assert_eq!(pickups, [vec![SqlValue::Int4(0)]], "nothing kept");
}
