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

// Shared ordinary Jadpo source with the focused performance fixture.
#[path = "../../../tools/bench/version_fixture.rs"]
mod fixture;
use fixture::files;

fn root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("jadpo-7.4-{name}-{}", std::process::id()))
}

/// Writes the project at `root` and its first migration.
fn project(root: &Path, sandbox: &str) {
    project_changed(root, sandbox, |_, text| text);
}

fn project_changed(root: &Path, sandbox: &str, change: impl Fn(&str, String) -> String) {
    let _ = std::fs::remove_dir_all(root);
    for (path, text) in files(sandbox) {
        let text = change(path, text);
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
    let mut ctx = Ctx::of(program, host, env);
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
    let driver: Rc<dyn Capability> = Rc::new(JsonHttp::new(NativeFetch::new()));
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
    let driver: Rc<dyn Capability> = Rc::new(JsonHttp::new(NativeFetch::new()));
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

/// First-loaded xmin survives rereads, a second closed snapshot, id-based
/// writes, and our own first successful update in the write transaction.
#[test]
fn first_loaded_versions_survive_rereads_and_repeated_writes() {
    for mode in [
        "reread",
        "second-snapshot",
        "by-id",
        "repeat",
        "delete-record",
        "delete-id",
        "delete-twice",
        "delete-after-update",
        "row-policy",
        "field-policy",
        "guard",
        "unkeyed",
    ] {
        let provider = FakeProvider::start();
        let root = root(mode);
        project_changed(&root, &provider.url, |path, mut text| {
            if mode == "second-snapshot" && path.ends_with("resources/bookings.jadpo") {
                return text
                    .replace("changes: true", "changes: false")
                    .replace("        idempotent: true\n", "");
            }
            if mode == "unkeyed" && path.ends_with("resources/bookings.jadpo") {
                return text.replace("        idempotent: true\n", "");
            }
            if mode == "guard" && path.ends_with("actions/add_shipment.jadpo") {
                return text.replace("            label: none\n", "");
            }
            if path.ends_with("entities/shipment.jadpo") {
                if mode == "row-policy" {
                    return text.replace(
                        "update: [Access.public, Access.system]",
                        "update: [Access.system]",
                    );
                }
                if mode == "field-policy" {
                    return text.replace(
                        "label: Label?",
                        "label: Label? { policy: { update: [Access.system] } }",
                    );
                }
                if mode == "guard" {
                    return text.replace("    policy: {", "    lifecycle: { visible: note != \"\" transitions: { mark: { when: note == \"Plain\" set: { label: \"Marked\" } } } }\n    policy: {");
                }
            }
            if mode.starts_with("delete-") && path.ends_with("entities/shipment.jadpo") {
                return text.replace("        update: [Access.public, Access.system]", "        update: [Access.public, Access.system]\n        delete: [Access.public, Access.system]");
            }
            if path != "domains/shipping/actions/book_shipment.jadpo" {
                return text;
            }
            if mode == "second-snapshot" {
                text = text.replace("decline step.late_failure: \"A shipment's note and booking always make a label\"\n", "");
            }
            if mode == "reread" || mode == "second-snapshot" {
                text = text.replace("    var label: Label", "    var reread: Shipment = attempt Shipment.get(shipment_id)\n    var label: Label");
                text = text.replace("Shipment.update(shipment)", "Shipment.update(reread)");
            }
            if mode == "second-snapshot" {
                text = text.replace("    var label: Label", r#"    var second: AcmeCouriers.Booking = attempt couriers.bookings.create(AcmeCouriers.CreateBooking {
        postcode: postcode
        slot_id: slot
    }) {
        slot_taken: CollectionSlotGone
    }
    if (second.id != booking.id) { reject CollectionSlotGone }
    var label: Label"#);
            }
            if mode == "guard" {
                let start = text.find("    return Shipment.update(shipment)").unwrap();
                text.truncate(start);
                text.push_str("    if (label == \"\") { reject LabelInvalid }\n    return attempt Shipment.mark(shipment) { conflict: { Shipment.mark: CollectionSlotGone } }\n}\n");
            }
            if mode == "by-id" {
                text = text.replace(
                    "return Shipment.update(shipment)",
                    "return attempt Shipment.update(shipment_id)",
                );
                text = text.replace("label: label", "label: \"By id\"");
                text = text.replace("    return attempt Shipment.update", "    if (label == \"\") { reject LabelInvalid }\n    return attempt Shipment.update");
            }
            if mode.starts_with("delete-") {
                text = text.replace("-> Shipment {", "-> Unit {").replace(
                    "{ Shipment, ShipmentNotFound }",
                    "{ Pickup, Shipment, ShipmentNotFound }",
                );
                let start = text.find("    return Shipment.update(shipment)").unwrap();
                text.truncate(start);
                text.push_str("    if (label == \"\") { reject LabelInvalid }\n");
                if mode == "delete-twice" {
                    text.push_str("    attempt Shipment.delete(shipment) { conflict: { Pickup.shipment_id: CollectionSlotGone } }\n");
                }
                if mode == "delete-after-update" {
                    text.push_str("    var changed: Shipment = Shipment.update(shipment) { set: { label: label } }\n");
                }
                text.push_str(if mode == "delete-id" || mode == "delete-twice" {
                    "    return attempt Shipment.delete(shipment_id) { conflict: { Pickup.shipment_id: CollectionSlotGone } }\n}\n"
                } else if mode == "delete-after-update" {
                    "    return attempt Shipment.delete(changed) { conflict: { Pickup.shipment_id: CollectionSlotGone } }\n}\n"
                } else {
                    "    return attempt Shipment.delete(shipment) { conflict: { Pickup.shipment_id: CollectionSlotGone } }\n}\n"
                });
            }
            if mode == "repeat" {
                text = text.replace(
                    "    return Shipment.update(shipment)",
                    "    var changed: Shipment = Shipment.update(shipment)",
                );
                text = text.replace(
                    "\n}\n",
                    "\n    return Shipment.update(changed) { set: { label: label } }\n}\n",
                );
            }
            text
        });
        if !jadpo_test::live() {
            continue;
        }
        let loaded = jadpo_db::load(&root);
        let steps = jadpo_check::data::migrations::store_steps(
            jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
        );
        let program = program(&root);
        let databases = jadpo_test::database::TestDatabases::new(steps, false);
        let copy = databases.acquire().expect("database");
        let (_, url) = copy.stores.get("main").unwrap().clone();
        let runtime = Runtime::new().unwrap();
        let store = jadpo_host_native::postgres(&url).unwrap();
        let driver: Rc<dyn Capability> = Rc::new(JsonHttp::new(NativeFetch::new()));
        let host = Faked::new(jadpo_host_native::NativeHost::new().with_store("main", store))
            .with("couriers", driver);
        let Outcome::Value(shipment) = run(
            &runtime,
            &program,
            &host,
            "add",
            "add_shipment",
            vec![Value::text("Plain")],
        ) else {
            panic!("added");
        };
        let Value::Uuid(id) = field(&shipment, "id").clone() else {
            panic!("id");
        };
        let id = id.to_string();
        let changed = Arc::new(Mutex::new(false));
        let changed_in_call = Arc::clone(&changed);
        let url_in_call = url.clone();
        provider.hold(move |_| {
            let mut changed = changed_in_call.lock().unwrap();
            if !*changed {
                once(
                    &url_in_call,
                    &format!("UPDATE shipment SET note = 'Fresh' WHERE id = '{id}'"),
                );
                *changed = true;
            }
        });
        for _ in 0..4 {
            provider.answer(201, r#"{"data":{"id":"b"}}"#);
        }
        let result = run(
            &runtime,
            &program,
            &host,
            "book",
            "book_shipment",
            vec![
                field(&shipment, "id").clone(),
                Value::text("N1"),
                Value::text("am"),
            ],
        );
        if ["row-policy", "field-policy", "guard", "unkeyed"].contains(&mode) {
            let code = match mode {
                "guard" => "collection_slot_gone",
                "unkeyed" => "outcome_unknown",
                _ => "not_permitted",
            };
            assert!(
                matches!(&result, Outcome::Failed(failure) if failure.code == code),
                "{mode}: {result:?}"
            );
            assert_eq!(
                provider.requests().len(),
                if mode == "unkeyed" { 1 } else { 2 },
                "{mode}: version refusal precedes ordinary refusal"
            );
            assert_eq!(
                once(&url, "SELECT note,label FROM shipment"),
                vec![vec![
                    SqlValue::Text("Fresh".into()),
                    SqlValue::Null(jadpo_runtime::SqlType::Text)
                ]]
            );
            continue;
        }
        if mode == "delete-twice" {
            assert!(
                matches!(&result, Outcome::Failed(failure) if failure.code == "shipment_not_found"),
                "{result:?}"
            );
            assert_eq!(provider.requests().len(), 2);
            assert_eq!(
                once(&url, "SELECT note,label FROM shipment"),
                vec![vec![
                    SqlValue::Text("Fresh".into()),
                    SqlValue::Null(jadpo_runtime::SqlType::Text)
                ]]
            );
            continue;
        }
        if mode.starts_with("delete-") {
            assert_eq!(result, Outcome::Value(Value::Unit), "{mode}");
            assert_eq!(
                provider.requests().len(),
                2,
                "{mode}: changed delete retried"
            );
            assert!(once(&url, "SELECT note,label FROM shipment").is_empty());
            continue;
        }
        let Outcome::Value(booked) = result else {
            panic!("{mode}: {result:?}");
        };
        assert_eq!(
            field(&booked, "label"),
            &Value::text(if mode == "by-id" { "By id" } else { "Fresh b" }),
            "{mode}: retry read fresh"
        );
        assert_eq!(
            provider.requests().len(),
            if mode == "second-snapshot" { 4 } else { 2 },
            "{mode}: changed attempt retried"
        );
        assert_eq!(
            once(&url, "SELECT note,label FROM shipment"),
            vec![vec![
                SqlValue::Text("Fresh".into()),
                SqlValue::Text(if mode == "by-id" {
                    "By id".into()
                } else {
                    "Fresh b".into()
                })
            ]]
        );
    }
}

/// Observe a real blocked UPDATE before releasing its competing writer.
/// The waiter must rerun after that writer commits; elapsed sleeps are
/// never used as evidence that the interleaving happened.
#[test]
fn a_waiting_write_rechecks_the_version_after_the_holder_commits() {
    let provider = FakeProvider::start();
    let root = root("blocked-update");
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
    let copy = databases.acquire().expect("database");
    let (_, url) = copy.stores.get("main").unwrap().clone();
    let runtime = Runtime::new().unwrap();
    let store = jadpo_host_native::postgres(&url).unwrap();
    let driver: Rc<dyn Capability> = Rc::new(JsonHttp::new(NativeFetch::new()));
    let host = Faked::new(jadpo_host_native::NativeHost::new().with_store("main", store))
        .with("couriers", driver);
    let Outcome::Value(shipment) = run(
        &runtime,
        &program,
        &host,
        "add",
        "add_shipment",
        vec![Value::text("Plain")],
    ) else {
        panic!("added");
    };
    let id = match field(&shipment, "id") {
        Value::Uuid(id) => id.to_string(),
        _ => panic!("id"),
    };
    let holder = Arc::new(Mutex::new(None));
    let holder_in_call = Arc::clone(&holder);
    let observed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let observed_in_call = Arc::clone(&observed);
    let once_only = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let url_in_call = url.clone();
    provider.hold(move |_| {
        use std::sync::atomic::Ordering;
        if once_only.swap(true, Ordering::SeqCst) { return; }
        let (ready, waiting) = std::sync::mpsc::channel();
        let url = url_in_call.clone();
        let id = id.clone();
        let observed = Arc::clone(&observed_in_call);
        *holder_in_call.lock().unwrap() = Some(std::thread::spawn(move || {
            let rt = Runtime::new().unwrap();
            let store = jadpo_host_native::postgres(&url).unwrap();
            rt.block_on(async {
                let mut transaction = store.begin_at(jadpo_runtime::Isolation::ReadCommitted).await.unwrap();
                transaction.execute(&format!("UPDATE shipment SET note = 'Held' WHERE id = '{id}'"), &[]).await.unwrap();
                ready.send(()).unwrap();
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
                loop {
                    let observer = jadpo_host_native::postgres(&url).unwrap();
                    let blocked = observer.query_once("SELECT count(*)::int4 FROM pg_stat_activity WHERE datname = current_database() AND wait_event_type = 'Lock' AND query LIKE '%shipment%' AND query LIKE '%UPDATE%'", &[]).await.unwrap();
                    if blocked == vec![vec![SqlValue::Int4(1)]] { break; }
                    assert!(std::time::Instant::now() < deadline, "no observed blocked write: {blocked:?}");
                    std::thread::yield_now();
                }
                observed.store(true, Ordering::SeqCst);
                transaction.commit().await.unwrap();
            });
        }));
        waiting.recv_timeout(std::time::Duration::from_secs(15)).unwrap();
    });
    provider.answer(201, r#"{"data":{"id":"b"}}"#);
    provider.answer(201, r#"{"data":{"id":"b"}}"#);
    let result = run(
        &runtime,
        &program,
        &host,
        "book",
        "book_shipment",
        vec![
            field(&shipment, "id").clone(),
            Value::text("N1"),
            Value::text("am"),
        ],
    );
    holder.lock().unwrap().take().unwrap().join().unwrap();
    assert!(observed.load(std::sync::atomic::Ordering::SeqCst));
    let Outcome::Value(booked) = result else {
        panic!("{result:?}");
    };
    assert_eq!(field(&booked, "label"), &Value::text("Held b"));
    assert_eq!(provider.requests().len(), 2);
}

/// The same ordinary source with one route, so workerd exercises a real
/// non-NULL expected version decoded from its generated deployable plan.
pub(crate) fn compiled_route_files() -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = files("http://127.0.0.1:1")
        .into_iter()
        .map(|(p, t)| (p.into(), t))
        .collect();
    for (path, text) in &mut files {
        if path == "config.jadpo" {
            *text = text.replace("couriers_environment: ProviderEnvironment\n", "couriers_environment: ProviderEnvironment { default: ProviderEnvironment.sandbox }\n");
        }
    }
    for (path, text) in [
        (
            "policy.jadpo",
            r#"use shipping
policy { actions: { shipping.book_for_http: [Access.public] } }
"#,
        ),
        (
            "domains/shipping/types/asked.jadpo",
            r#"use shared/providers/acme_couriers
type Asked = Input { postcode: AcmeCouriers.Postcode slot: AcmeCouriers.SlotId }
"#,
        ),
        (
            "domains/shipping/types/booked.jadpo",
            r#"use shipping/entities { Shipment }
type Booked = Output { pick: [Shipment.id, Shipment.label] }
"#,
        ),
        (
            "domains/shipping/actions/book_for_http.jadpo",
            r#"use shipping/entities { Shipment, ShipmentNotFound }
use shipping/actions { book_shipment }
use shipping/failures { CollectionSlotGone, LabelInvalid }
use shipping/types { Asked, Booked }
action book_for_http(shipment_id: Shipment.id, input: Asked) fails ShipmentNotFound, CollectionSlotGone, LabelInvalid -> Booked {
    var booked: Shipment = attempt book_shipment(shipment_id, input.postcode, input.slot)
    return Booked { id: booked.id label: booked.label }
}
"#,
        ),
        (
            "domains/shipping/routes/shipments.jadpo",
            r#"use shipping/entities { Shipment }
use shipping/types { Asked, Booked }
use shipping/actions { book_for_http }
routes Shipments {
    prefix: /shipments
    POST /{shipment_id: Shipment.id} {
        authentication: RouteAuthentication.none
        input: Asked
        output: Booked
        run: book_for_http(shipment_id, input)
    }
}
"#,
        ),
        (
            "domains/shipping/routes/shipments.test.jadpo",
            r#"use app { couriers }
use shipping/entities { Shipment }
use shipping/types { Asked }
use shipping/actions { add_shipment }
use shared/providers/acme_couriers

test "a compiled write checks its pre-call version" {
    fakes: { couriers.bookings.create: [AcmeCouriers.Booking { id: "b" }] }
    var shipment: Shipment = add_shipment("Plain")
    var response: Response = request {
        method: HttpMethod.post
        path: "/shipments/{shipment_id}"
        params: { shipment_id: shipment.id }
        input: Asked { postcode: "N1" slot: "am" }
    }
    assert response.status == 200
    var labelled: Bool = match response.output {
        some(booked) -> booked.id == shipment.id and booked.label == "Plain b"
        none -> false
    }
    assert labelled
}
"#,
        ),
    ] {
        files.push((path.into(), text.into()));
    }
    files
}

#[test]
fn the_compiled_version_route_fixture_checks_and_runs() {
    let root = root("compiled-fixture");
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in compiled_route_files() {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let terminal = jadpo_term::Environment::empty()
        .with_var("SOURCE_DATE_EPOCH", "1792504800")
        .with_cwd(&root);
    for args in [vec!["migrate", "new", "initial"], vec!["api", "update"]] {
        let output = jadpo_cli::run(
            &args.into_iter().map(str::to_string).collect::<Vec<_>>(),
            &terminal,
        );
        assert_eq!(output.code, 0, "{}{}", output.stdout, output.stderr);
    }
    if !jadpo_test::live() {
        return;
    }
    let output = jadpo_cli::run(&["test".into(), "--seed".into(), "1".into()], &terminal);
    assert_eq!(output.code, 0, "{}{}", output.stdout, output.stderr);
    assert!(output.stdout.contains("1 passed"), "{}", output.stdout);
}
