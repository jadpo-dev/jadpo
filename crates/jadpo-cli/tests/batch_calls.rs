//! Calls from a batch's items (area 12, "How a run is kept"; area 6,
//! "Outside calls hold no transaction"; chunk 7.4b), live against
//! PostgreSQL and a provider started in the test: a run whose `each:`
//! action books each shipment's collection runs each item as a batch of
//! its own, its call sent with no transaction open, under a key derived
//! from the run and the item's place in the walk, then its writes and the
//! run's position committed together. A crash after an item's call resends
//! it under its first key, and its booking is kept once. `finished:`
//! makes its call the same way, and so does each row of a file's run. And
//! `jadpo check` refuses a write placed before a call (area 6, "Several
//! outside calls").
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use jadpo_drivers::package::JsonHttp;
use jadpo_host_native::fetch::NativeFetch;
use jadpo_host_native::{Postgres, Runtime};
use jadpo_interp::{Executor, Outcome, Prepared, Program};
use jadpo_runtime::batches::{BatchStart, Ran};
use jadpo_runtime::capability::{Capability, Faked};
use jadpo_runtime::runner::Deferred;
use jadpo_runtime::{
    Database, Host, Isolation, LocalFuture, Row, SqlValue, StoreError, Transaction,
};
use jadpo_test::host::Harness;
use jadpo_value::Value;

use crate::support::fake_provider::FakeProvider;

/// The courier's package, at `sandbox`, and a shipping domain whose run
/// books each shipment's collection, then a van for the round once every
/// shipment is booked.
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
        imports: {
            driver: Driver.disk
            path: "/var/lib/shipping/imports"
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
            "domains/shipping/types/shipment_page.jadpo",
            r#"/// A page of shipments, in the order they were added.
type ShipmentPage = Input {
    page_size: PageSize {
        default: 25
    }
    after: PageCursor {
        optional: true
    }
}
"#
            .to_string(),
        ),
        (
            "domains/shipping/types/sheet_text.jadpo",
            "/// A sheet of parcels as the warehouse sends it.\ntype SheetText = Text {\n    min_length: 1\n    max_length: 100000\n}\n"
                .to_string(),
        ),
        (
            "domains/shipping/types/parcel_row.jadpo",
            "use shipping/types { Note }\n\n/// One parcel's row of a sheet.\ntype ParcelRow = Input {\n    note: Note {\n        column: \"Note\"\n    }\n}\n"
                .to_string(),
        ),
        (
            "domains/shipping/failures/sheet_not_kept.jadpo",
            "type SheetNotKept = Failure {\n    kind: FailureKind.unavailable\n    message: \"There was no room to keep the sheet.\"\n}\n"
                .to_string(),
        ),
        (
            "domains/shipping/entities/parcel.jadpo",
            r#"use shipping/types { Note }

/// A parcel a sheet listed, with its booked collection.
entity Parcel {
    id: Uuid
    note: Note
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
            "domains/shipping/actions/book_parcel.jadpo",
            r#"use app { couriers }
use shipping/entities { Parcel }
use shipping/failures { CollectionSlotGone }
use shipping/types { ParcelRow }
use shared/providers/acme_couriers

/// Books a sheet's parcel's collection, then keeps the parcel.
action book_parcel(row: ParcelRow) fails CollectionSlotGone -> Unit {
    var booking: AcmeCouriers.Booking = attempt couriers.bookings.create(AcmeCouriers.CreateBooking {
        postcode: "N1"
        slot_id: "pm"
    }) {
        slot_taken: CollectionSlotGone
    }
    var parcel: Parcel = Parcel.create {
        set: {
            note: row.note
            booking: booking.id
        }
    }
    assert parcel.note == row.note
}
"#
            .to_string(),
        ),
        (
            "domains/shipping/actions/import_parcels.jadpo",
            r#"use app { imports }
use shipping/actions { book_parcel }
use shipping/failures { SheetNotKept }
use shipping/types { ParcelRow, SheetText }

/// Keeps a sheet of parcels, then books each one's collection.
action import_parcels(sheet_text: SheetText) fails SheetNotKept -> BatchRun {
    var sheet: File = attempt imports.put(encoding.to_utf8(sheet_text), FileType.csv) {
        storage_full: SheetNotKept
    }
    return csv.read(sheet, as: ParcelRow, each: row -> book_parcel(row))
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
            "domains/shipping/entities/shipment.jadpo",
            r#"use shipping/types { Note }

/// A parcel waiting for collection.
entity Shipment {
    id: Uuid
    note: Note
    booking: Text?

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
            "domains/shipping/entities/round.jadpo",
            r#"/// A collection round: how many shipments its run booked, and the van.
entity Round {
    id: Uuid
    booked: Int
    van: Text

    policy: {
        create: [Access.public, Access.system]
        read: [Access.public, Access.system]
    }
}
"#
            .to_string(),
        ),
        (
            "domains/shipping/queries/to_book.jadpo",
            r#"use shipping/entities { Shipment }
use shipping/types { ShipmentPage }

/// The shipments, a page at a time, in an order no update moves.
query to_book(page: ShipmentPage) -> Page<Shipment> {
    from: Shipment
    order: [Shipment.id asc]
    page: {
        size: page.page_size
        after: page.after
    }
}
"#
            .to_string(),
        ),
        (
            "domains/shipping/actions/add_shipment.jadpo",
            r#"use shipping/entities { Shipment }
use shipping/types { Note }

/// Adds a shipment.
action add_shipment(note: Note) -> Shipment {
    return Shipment.create {
        set: {
            note: note
            booking: none
        }
    }
}
"#
            .to_string(),
        ),
        (
            "domains/shipping/actions/book_shipment.jadpo",
            r#"use app { couriers }
use shipping/entities { Shipment }
use shipping/failures { CollectionSlotGone }
use shared/providers/acme_couriers

/// Books the shipment's collection, then keeps the booking on it: the
/// call first, the write after it.
action book_shipment(shipment: Shipment) fails CollectionSlotGone -> Unit {
    var booking: AcmeCouriers.Booking = attempt couriers.bookings.create(AcmeCouriers.CreateBooking {
        postcode: "N1"
        slot_id: "am"
    }) {
        slot_taken: CollectionSlotGone
    }
    var booked: Shipment = Shipment.update(shipment) {
        set: {
            booking: booking.id
        }
    }
    assert booked.id == shipment.id
}
"#
            .to_string(),
        ),
        (
            "domains/shipping/actions/book_van.jadpo",
            r#"use app { couriers }
use shipping/entities { Round }
use shipping/failures { CollectionSlotGone }
use shared/providers/acme_couriers

/// Books a van for the round once its shipments are booked, then keeps
/// the round.
action book_van(summary: ImportSummary) fails CollectionSlotGone -> Unit {
    var van: AcmeCouriers.Booking = attempt couriers.bookings.create(AcmeCouriers.CreateBooking {
        postcode: "N1"
        slot_id: "van"
    }) {
        slot_taken: CollectionSlotGone
    }
    var round: Round = Round.create {
        set: {
            booked: summary.items
            van: van.id
        }
    }
    assert round.booked == summary.items
}
"#
            .to_string(),
        ),
        (
            "domains/shipping/actions/book_all.jadpo",
            r#"use shipping/actions { book_shipment, book_van }
use shipping/queries { to_book }
use shipping/types { ShipmentPage }

/// Books every shipment's collection, then a van for the round.
action book_all(page: ShipmentPage) -> BatchRun {
    return batches.each(
        from: to_book(page),
        each: shipment -> book_shipment(shipment),
        finished: summary -> book_van(summary)
    )
}
"#
            .to_string(),
        ),
    ]
}

/// A write placed before the call: what `jadpo check` refuses.
const LABEL_THEN_BOOK: &str = r#"use app { couriers }
use shipping/entities { Shipment }
use shipping/failures { CollectionSlotGone }
use shipping/types { Note }
use shared/providers/acme_couriers

/// Notes the shipment as booked, then books it: the write comes first.
action label_then_book(shipment: Shipment, note: Note) fails CollectionSlotGone -> Shipment {
    var noted: Shipment = Shipment.update(shipment) {
        set: {
            note: note
        }
    }
    var booking: AcmeCouriers.Booking = attempt couriers.bookings.create(AcmeCouriers.CreateBooking {
        postcode: "N1"
        slot_id: "am"
    }) {
        slot_taken: CollectionSlotGone
    }
    assert booking.id == booking.id
    return noted
}
"#;

fn root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("jadpo-7.4b-{name}-{}", std::process::id()))
}

fn jadpo(root: &Path, args: &[&str]) -> jadpo_cli::Output {
    let terminal = jadpo_term::Environment::empty()
        .with_var("SOURCE_DATE_EPOCH", "1792504800")
        .with_cwd(root);
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    jadpo_cli::run(&args, &terminal)
}

/// Writes the project at `root` and its first migration.
fn project(root: &Path, sandbox: &str) {
    let _ = std::fs::remove_dir_all(root);
    for (path, text) in files(sandbox) {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        std::fs::write(path, text).expect("write");
    }
    let migrated = jadpo(root, &["migrate", "new", "initial"]);
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

/// The store, whose `nth` transaction from now that writes a run's
/// position dies before its commit, as a process that stops there does,
/// keeping nothing it did.
struct Crashing {
    db: Postgres,
    crash_in: Rc<Cell<Option<u32>>>,
}

impl Database for Crashing {
    fn begin_at(
        &self,
        isolation: Isolation,
    ) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>> {
        let crash_in = Rc::clone(&self.crash_in);
        Box::pin(async move {
            let inner = self.db.begin_at(isolation).await?;
            Ok(Box::new(Dying {
                inner,
                advanced: false,
                crash_in,
            }) as Box<dyn Transaction>)
        })
    }

    fn query_once<'a>(
        &'a self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
        self.db.query_once(sql, params)
    }
}

/// A transaction that may die before its commit.
struct Dying {
    inner: Box<dyn Transaction>,
    /// It wrote a run's position.
    advanced: bool,
    crash_in: Rc<Cell<Option<u32>>>,
}

impl Transaction for Dying {
    fn query<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
        self.inner.query(sql, params)
    }

    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<u64, StoreError>> {
        if sql.starts_with("UPDATE \"jadpo_batch_runs\" SET \"position\"") {
            self.advanced = true;
        }
        self.inner.execute(sql, params)
    }

    fn batch<'a>(&'a mut self, sql: &'a str) -> LocalFuture<'a, Result<(), StoreError>> {
        self.inner.batch(sql)
    }

    fn commit(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
        let Dying {
            inner,
            advanced,
            crash_in,
        } = *self;
        Box::pin(async move {
            if advanced && let Some(left) = crash_in.get() {
                if left <= 1 {
                    crash_in.set(None);
                    let _ = inner.rollback().await;
                    return Err(StoreError::Unavailable);
                }
                crash_in.set(Some(left - 1));
            }
            inner.commit().await
        })
    }

    fn rollback(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
        self.inner.rollback()
    }
}

/// The time the test starts at, which a run's leases are counted from.
const START: i64 = 1_800_000_000_000;

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

/// How many items the run has committed.
const DONE: &str = "SELECT \"items_done\"::int4 FROM \"jadpo_batch_runs\"";

fn int(rows: &[Vec<SqlValue>]) -> i32 {
    match rows.first().and_then(|row| row.first()) {
        Some(SqlValue::Int4(n)) => *n,
        other => panic!("not a count: {other:?}"),
    }
}

/// A project checked and migrated, its program, and a database of its
/// own.
struct Loaded {
    _root: PathBuf,
    program: Program,
    _databases: jadpo_test::database::TestDatabases,
    url: String,
}

fn load(name: &str, provider: &FakeProvider) -> Option<Loaded> {
    let root = root(name);
    project(&root, &provider.url);
    if !jadpo_test::live() {
        return None;
    }
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = program(&root);
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    Some(Loaded {
        _root: root,
        program,
        _databases: databases,
        url,
    })
}

/// The harness over the store at `url`, with a clock the test moves and
/// randomness of its own, and the crash it can arm.
fn harness(url: &str) -> (Harness<Crashing>, Rc<Cell<Option<u32>>>) {
    let crash_in = Rc::new(Cell::new(None));
    let store = Crashing {
        db: jadpo_host_native::postgres(url).expect("a store"),
        crash_in: Rc::clone(&crash_in),
    };
    (Harness::new(store).at(START).random_from(1), crash_in)
}

/// Runs action `name` with `args` on `host`, giving its outcome and the
/// work it left for after its response.
fn call(
    runtime: &Runtime,
    program: &Program,
    harness: &Harness<Crashing>,
    host: &dyn Host,
    name: &str,
    args: Vec<Value>,
) -> (Outcome, Vec<Deferred>) {
    let id = program.plan().find(name).expect("the action");
    let mut ctx = harness.ctx_on(program, host);
    let outcome = runtime.block_on(program.run(&mut ctx, id, args));
    (outcome, ctx.take_deferred())
}

fn page(size: i64) -> Value {
    Value::record(
        "ShipmentPage",
        vec![
            (Rc::from("page_size"), Value::Int(size)),
            (Rc::from("after"), Value::None),
        ],
    )
}

/// Adds a shipment for each of `notes`.
fn add(
    runtime: &Runtime,
    program: &Program,
    harness: &Harness<Crashing>,
    host: &dyn Host,
    notes: &[&str],
) {
    for note in notes {
        let (added, _) = call(
            runtime,
            program,
            harness,
            host,
            "add_shipment",
            vec![Value::text(note)],
        );
        assert!(matches!(added, Outcome::Value(_)), "{added:?}");
    }
}

/// Starts the run over every shipment, in pages of `size`.
fn start(
    runtime: &Runtime,
    program: &Program,
    harness: &Harness<Crashing>,
    host: &dyn Host,
    size: i64,
) -> BatchStart {
    let (outcome, work) = call(
        runtime,
        program,
        harness,
        host,
        "book_all",
        vec![page(size)],
    );
    assert!(matches!(outcome, Outcome::Value(_)), "{outcome:?}");
    let mut starts: Vec<BatchStart> = work
        .into_iter()
        .filter_map(|work| match work {
            Deferred::Batch(start) => Some(start),
            _ => None,
        })
        .collect();
    assert_eq!(starts.len(), 1, "one run left for after the action");
    starts.remove(0)
}

/// Each shipment's booking, in the order the run walked them.
fn bookings(url: &str) -> Vec<SqlValue> {
    once(url, "SELECT \"booking\" FROM \"shipment\" ORDER BY \"id\"")
        .into_iter()
        .map(|row| row.into_iter().next().expect("a booking"))
        .collect()
}

/// The run's state, items done and problems, as its row keeps them.
fn kept(url: &str) -> Vec<SqlValue> {
    once(
        url,
        "SELECT \"state\", \"items_done\"::int4, \"problems\"::int4 FROM \"jadpo_batch_runs\"",
    )
    .into_iter()
    .next()
    .expect("the run's row")
}

/// Each item's call goes out with no transaction open, and its booking
/// commits with the run's position before the next item's call; then
/// `finished:` books the van the same way (roadmap 7.4b's "Done when").
#[test]
fn each_items_call_goes_out_with_no_transaction_open_and_commits_with_the_position() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("items", &provider) else {
        return;
    };
    let Loaded { program, url, .. } = &loaded;
    let runtime = Runtime::new().expect("a runtime");
    let (harness, _) = harness(url);
    let driver: Rc<dyn Capability> = Rc::new(JsonHttp::new(NativeFetch::new()));
    let host = Faked::new(&harness).with("couriers", driver);
    add(
        &runtime,
        program,
        &harness,
        &host,
        &["a", "b", "c", "d", "e"],
    );

    // While each call is held: how many transactions are open, and how
    // many items the run has committed.
    let seen: Arc<Mutex<Vec<(i32, i32)>>> = Arc::default();
    {
        let seen = Arc::clone(&seen);
        let url = url.clone();
        provider.hold(move |_| {
            let open = int(&once(&url, OPEN));
            let done = int(&once(&url, DONE));
            seen.lock().expect("seen").push((open, done));
        });
    }
    for n in 1..=5 {
        provider.answer(201, &format!(r#"{{"data": {{"id": "b-{n}"}}}}"#));
    }
    provider.answer(201, r#"{"data": {"id": "van-1"}}"#);

    // Pages of two hold five items, but each item is a batch of its own.
    let started = start(&runtime, program, &harness, &host, 2);
    let (ran, _) = runtime.block_on(jadpo_runtime::batches::run(program, &host, &started));
    assert_eq!(ran, Ran::Finished);

    assert_eq!(
        *seen.lock().expect("seen"),
        [(0, 0), (0, 1), (0, 2), (0, 3), (0, 4), (0, 5)],
        "no transaction open while a call was out, and each item committed before the next call"
    );
    let requests = provider.requests();
    assert_eq!(requests.len(), 6, "five items and the van");
    let keys: Vec<String> = requests
        .iter()
        .map(|request| {
            request
                .header("idempotency-key")
                .expect("a keyed call")
                .to_string()
        })
        .collect();
    let mut distinct = keys.clone();
    distinct.sort();
    distinct.dedup();
    assert_eq!(
        distinct.len(),
        6,
        "each item's call, and the van's, a key of its own"
    );
    assert_eq!(
        bookings(url),
        (1..=5)
            .map(|n| SqlValue::Text(format!("b-{n}")))
            .collect::<Vec<_>>(),
        "each item's booking kept on its shipment, in the order the run walked them"
    );
    assert_eq!(
        kept(url),
        [
            SqlValue::Text("finished".into()),
            SqlValue::Int4(5),
            SqlValue::Int4(0)
        ]
    );
    assert_eq!(
        once(url, "SELECT \"booked\"::int4, \"van\" FROM \"round\""),
        [vec![SqlValue::Int4(5), SqlValue::Text("van-1".into())]],
        "`finished:` booked the van, then kept the round"
    );
}

/// A run resumed after a crash between an item's call and its commit sends
/// that item's call again under its first key, and keeps its booking once
/// (roadmap 7.4b's "Done when").
#[test]
fn a_crash_after_an_items_call_resends_it_under_its_first_key() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("crash", &provider) else {
        return;
    };
    let Loaded { program, url, .. } = &loaded;
    let runtime = Runtime::new().expect("a runtime");
    let (harness, crash_in) = harness(url);
    let driver: Rc<dyn Capability> = Rc::new(JsonHttp::new(NativeFetch::new()));
    let host = Faked::new(&harness).with("couriers", driver);
    add(&runtime, program, &harness, &host, &["a", "b", "c"]);
    // The provider replays the second item's answer under its key.
    for id in ["b-1", "b-2", "b-2", "b-3", "van-1"] {
        provider.answer(201, &format!(r#"{{"data": {{"id": "{id}"}}}}"#));
    }

    let started = start(&runtime, program, &harness, &host, 25);
    // The second item's call goes out, and the process dies before its
    // booking and the run's position commit.
    crash_in.set(Some(2));
    let (ran, _) = runtime.block_on(jadpo_runtime::batches::run(program, &host, &started));
    assert!(matches!(ran, Ran::Paused(_)), "{ran:?}");
    assert_eq!(provider.requests().len(), 2);
    assert_eq!(
        bookings(url),
        [
            SqlValue::Text("b-1".into()),
            SqlValue::Null(jadpo_runtime::SqlType::Text),
            SqlValue::Null(jadpo_runtime::SqlType::Text)
        ],
        "the second item's booking died with its commit"
    );
    assert_eq!(
        kept(url),
        [
            SqlValue::Text("running".into()),
            SqlValue::Int4(1),
            SqlValue::Int4(0)
        ]
    );

    // Once its lease has lapsed, the tick picks the run up where it
    // committed.
    let later = START + jadpo_runtime::lease::LONGEST_MS + 1_000;
    harness.set_now(later);
    let (resumed, _) = runtime.block_on(jadpo_runtime::batches::resume(program, &host, later));
    assert_eq!(resumed.len(), 1, "{resumed:?}");
    assert_eq!(resumed[0].1, Ran::Finished);
    let requests = provider.requests();
    assert_eq!(
        requests.len(),
        5,
        "three items, the second twice, and the van"
    );
    let key = |at: usize| {
        requests[at]
            .header("idempotency-key")
            .expect("a keyed call")
            .to_string()
    };
    assert_eq!(
        key(1),
        key(2),
        "the repeated item's call under its first key"
    );
    assert_ne!(key(0), key(1));
    assert_ne!(key(2), key(3));
    assert_eq!(
        bookings(url),
        [
            SqlValue::Text("b-1".into()),
            SqlValue::Text("b-2".into()),
            SqlValue::Text("b-3".into())
        ],
        "each booking kept once"
    );
    assert_eq!(
        kept(url),
        [
            SqlValue::Text("finished".into()),
            SqlValue::Int4(3),
            SqlValue::Int4(0)
        ]
    );
}

/// An item whose call fails is a problem, undone alone, and the run
/// carries on.
#[test]
fn an_item_whose_call_fails_is_a_problem_and_the_run_carries_on() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("problem", &provider) else {
        return;
    };
    let Loaded { program, url, .. } = &loaded;
    let runtime = Runtime::new().expect("a runtime");
    let (harness, _) = harness(url);
    let driver: Rc<dyn Capability> = Rc::new(JsonHttp::new(NativeFetch::new()));
    let host = Faked::new(&harness).with("couriers", driver);
    add(&runtime, program, &harness, &host, &["a", "b", "c"]);
    provider.answer(201, r#"{"data": {"id": "b-1"}}"#);
    provider.answer(409, r#"{"error": {"code": "slot_taken"}}"#);
    provider.answer(201, r#"{"data": {"id": "b-3"}}"#);
    provider.answer(201, r#"{"data": {"id": "van-1"}}"#);

    let started = start(&runtime, program, &harness, &host, 25);
    let (ran, _) = runtime.block_on(jadpo_runtime::batches::run(program, &host, &started));
    assert_eq!(ran, Ran::Finished);
    assert_eq!(provider.requests().len(), 4);
    assert_eq!(
        bookings(url),
        [
            SqlValue::Text("b-1".into()),
            SqlValue::Null(jadpo_runtime::SqlType::Text),
            SqlValue::Text("b-3".into())
        ]
    );
    assert_eq!(
        kept(url),
        [
            SqlValue::Text("finished".into()),
            SqlValue::Int4(3),
            SqlValue::Int4(1)
        ]
    );
    assert_eq!(
        once(
            url,
            "SELECT \"row\"::int4, \"rule\" FROM \"jadpo_batch_problems\""
        ),
        [vec![
            SqlValue::Int4(2),
            SqlValue::Text("collection_slot_gone".into())
        ]]
    );
    assert_eq!(
        once(url, "SELECT \"booked\"::int4 FROM \"round\""),
        [vec![SqlValue::Int4(3)]]
    );
}

/// A file's rows whose action makes outside calls are each a batch of
/// their own too: each call out with no transaction open, keyed from the
/// run and where its row begins, so a row cut off between its call and its
/// commit is sent again under its first key, and its parcel kept once.
#[test]
fn a_files_rows_that_call_are_each_a_batch_of_their_own() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("rows", &provider) else {
        return;
    };
    let Loaded { program, url, .. } = &loaded;
    let runtime = Runtime::new().expect("a runtime");
    let (harness, crash_in) = harness(url);
    let driver: Rc<dyn Capability> = Rc::new(JsonHttp::new(NativeFetch::new()));
    let files: Rc<dyn Capability> = Rc::new(jadpo_runtime::files::MemoryFiles::new());
    let host = Faked::new(&harness)
        .with("couriers", driver)
        .with("imports", files);
    let open_while_out: Arc<Mutex<Vec<i32>>> = Arc::default();
    {
        let open_while_out = Arc::clone(&open_while_out);
        let url = url.clone();
        provider.hold(move |_| {
            open_while_out
                .lock()
                .expect("open")
                .push(int(&once(&url, OPEN)));
        });
    }
    for id in ["b-1", "b-2", "b-2", "b-3"] {
        provider.answer(201, &format!(r#"{{"data": {{"id": "{id}"}}}}"#));
    }
    let (outcome, work) = call(
        &runtime,
        program,
        &harness,
        &host,
        "import_parcels",
        vec![Value::text("Note\r\na\r\nb\r\nc\r\n")],
    );
    assert!(matches!(outcome, Outcome::Value(_)), "{outcome:?}");
    let started = match work.as_slice() {
        [Deferred::Batch(start)] => start.clone(),
        other => panic!("{other:?}"),
    };
    // The second row's call goes out, and the process dies before its
    // parcel and the run's position commit.
    crash_in.set(Some(2));
    let (ran, _) = runtime.block_on(jadpo_runtime::batches::run(program, &host, &started));
    assert!(matches!(ran, Ran::Paused(_)), "{ran:?}");
    let later = START + jadpo_runtime::lease::LONGEST_MS + 1_000;
    harness.set_now(later);
    let (resumed, _) = runtime.block_on(jadpo_runtime::batches::resume(program, &host, later));
    assert_eq!(resumed.len(), 1, "{resumed:?}");
    assert_eq!(resumed[0].1, Ran::Finished);

    assert_eq!(
        *open_while_out.lock().expect("open"),
        [0, 0, 0, 0],
        "no transaction open while any row's call was out"
    );
    let requests = provider.requests();
    assert_eq!(requests.len(), 4, "three rows, the second twice");
    let key = |at: usize| {
        requests[at]
            .header("idempotency-key")
            .expect("a keyed call")
            .to_string()
    };
    assert_eq!(
        key(1),
        key(2),
        "the repeated row's call under its first key"
    );
    assert_ne!(key(0), key(1));
    assert_ne!(key(2), key(3));
    assert_eq!(
        once(
            url,
            "SELECT \"note\", \"booking\" FROM \"parcel\" ORDER BY \"note\""
        ),
        [
            vec![SqlValue::Text("a".into()), SqlValue::Text("b-1".into())],
            vec![SqlValue::Text("b".into()), SqlValue::Text("b-2".into())],
            vec![SqlValue::Text("c".into()), SqlValue::Text("b-3".into())],
        ],
        "each row's parcel kept once"
    );
    assert_eq!(
        kept(url),
        [
            SqlValue::Text("finished".into()),
            SqlValue::Int4(3),
            SqlValue::Int4(0)
        ]
    );
}

/// A write placed before a later outside call is refused at compile time,
/// naming the call it must follow, so checked code never reaches the
/// runtime's `call_in_transaction` (area 6, "Several outside calls").
#[test]
fn jadpo_check_refuses_a_write_before_a_call() {
    let root = root("refused");
    project(&root, "http://127.0.0.1:9");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    std::fs::write(
        root.join("domains/shipping/actions/label_then_book.jadpo"),
        LABEL_THEN_BOOK,
    )
    .expect("write");
    let refused = jadpo(&root, &["check"]);
    let printed = format!("{}{}", refused.stdout, refused.stderr);
    assert_eq!(refused.code, 1, "{printed}");
    assert!(
        printed.contains(
            "error: `Shipment.update` writes before the call to `couriers.bookings.create`"
        ),
        "{printed}"
    );
    assert!(
        printed.contains("rule: data.write_before_call"),
        "{printed}"
    );
    let _ = std::fs::remove_dir_all(&root);
}
