//! Routes that wait for follow-on work (area 7, "Waiting for follow-on
//! work"; chunk 6.3): `settle:` delivers what the action emitted, and what
//! its subscribers emit in turn, while the request waits, then answers
//! with `answer:`'s query and the route's status once all of it has
//! settled (each delivery succeeded, failed for good or parked), and with
//! 202 past `within:` or while a delivery waits to retry, the rest
//! carrying on after the response. `output:` is the query's result, both
//! answers are in OpenAPI and the record, a repeat with the same
//! `Idempotency-Key` runs the query again, and a route test sees it all.
//!
//! A shop: placing an order emits `OrderPlaced`, which `confirm_order`
//! confirms (refusing a fragile item for good, and a later one until the
//! warehouse opens), emitting `OrderConfirmed`, which `stamp_order` stamps.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_host_native::{Postgres, Runtime, postgres};
use jadpo_interp::Program;
use jadpo_runtime::http::{Request, Response, Worker, serve_request};
use jadpo_runtime::runner::run_deferred;
use jadpo_runtime::{Clock, Ctx, Database, Env, Executor, Host, LocalFuture, Row, SqlValue};
use jadpo_std::http::PrincipalKind;
use jadpo_term::Environment;
use jadpo_value::Value;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    run(&args, &env.with_cwd(cwd))
}

/// The shop, migrated and its record written, or with `routes` in place
/// of its routes file.
fn project(name: &str, routes: Option<&str>) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("settle-6.3-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, text);
    }
    if let Some(routes) = routes {
        write(&root, ROUTES, routes);
    }
    // Routes with a problem stop the migration, which only `check`'s
    // answer about them matters for.
    if routes.is_none() {
        let migrated = jadpo(&root, &["migrate", "new", "initial"]);
        assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
        // The API record and the event record,
        // `domains/shop/events/events.jadpo_record` (chunk 6.1c).
        let updated = jadpo(&root, &["records", "update"]);
        assert_eq!(updated.code, 0, "{}{}", updated.stdout, updated.stderr);
    }
    root
}

const ROUTES: &str = "domains/shop/routes/orders.jadpo";

/// The shop's route with `from` written as `to`.
fn route_with(from: &str, to: &str) -> String {
    let (_, text) = FILES
        .iter()
        .find(|(path, _)| *path == ROUTES)
        .expect("the routes file");
    assert!(text.contains(from), "{from}");
    text.replace(from, to)
}

/// `settle:` checks, and both its answers are in OpenAPI and the API
/// record: the route's status once the work settles, 202 before, each
/// sending what `answer:` gives.
#[test]
fn a_settling_route_checks_and_describes_both_answers() {
    let root = project("described", None);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let record = fs::read_to_string(root.join("api/main.jadpo_record")).expect("the record");
    assert!(
        record.contains(
            "POST /orders\n    caller user\n    input OrderRequest (body)\n    output OrderView\n    status 201\n    settle within 2s\n    fails order_not_found\n"
        ),
        "{record}"
    );
    let openapi: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("build/openapi/main.json")).expect("the document"),
    )
    .expect("JSON");
    let responses = &openapi["paths"]["/orders"]["post"]["responses"];
    let view = serde_json::json!({ "$ref": "#/components/schemas/OrderView" });
    for status in ["201", "202"] {
        assert_eq!(
            responses[status]["content"]["application/json"]["schema"], view,
            "{status}: {responses}"
        );
    }
    assert_eq!(
        responses["202"]["description"],
        "Accepted: the work it set off hadn't settled within 2s; the body is what it has done so far, and the work carries on."
    );
    // The answer query's failure can be the route's answer too.
    assert!(
        responses["404"]["description"]
            .as_str()
            .is_some_and(|text| text.contains("order_not_found")),
        "{responses}"
    );
    let key = openapi["paths"]["/orders"]["post"]["parameters"]
        .as_array()
        .and_then(|parameters| {
            parameters
                .iter()
                .find(|parameter| parameter["name"] == "Idempotency-Key")
        })
        .expect("the key")
        .clone();
    assert!(
        key["description"]
            .as_str()
            .is_some_and(|text| text.contains("`answer:` run again")),
        "{key}"
    );
}

/// What `settle:` refuses, each by its rule.
#[test]
fn settle_refuses_what_it_cannot_wait_for() {
    let cases: &[(&str, String, &str, &str)] = &[
        (
            "past-deadline",
            route_with("within: 2s", "within: 10s"),
            "route.settle_invalid",
            "`within: 10s` doesn't fit inside the route's 10s deadline",
        ),
        (
            "not-result",
            route_with("order_status(result.id)", "order_status(input.item)"),
            "route.settle_invalid",
            "`answer:` passes `input.item`, not `result` or its fields",
        ),
        (
            "an-action",
            route_with(
                "use shop/queries { order_status }\n",
                "",
            )
            .replace("answer: order_status(result.id)", "answer: place_order(input, current_principal.user)"),
            "route.settle_invalid",
            "`answer: place_order` is an action",
        ),
        (
            "accepted",
            route_with("status: SuccessStatus.created", "status: SuccessStatus.accepted"),
            "route.settle_invalid",
            "`POST /orders` answers `SuccessStatus.accepted` and waits with `settle:`",
        ),
        (
            "no-within",
            route_with("            within: 2s\n", ""),
            "route.settle_invalid",
            "`settle:` has no `within:`",
        ),
        (
            "unknown-setting",
            route_with("within: 2s", "within: 2s\n            wait: 1s"),
            "route.settle_invalid",
            "`wait` isn't part of `settle:`",
        ),
        (
            "wrong-field",
            route_with("order_status(result.id)", "order_status(result.code)"),
            "type.unknown_field",
            "`Order` has no field `code`",
        ),
        // Without `settle:`, the route sends what the action returns.
        (
            "no-settle",
            route_with(
                "use shop/queries { order_status }\n",
                "",
            )
            .replace(
                "        settle: {\n            answer: order_status(result.id)\n            within: 2s\n        }\n",
                "",
            ),
            "route.output_mismatch",
            "`POST /orders` sends `OrderView`, but `place_order` returns `Order`",
        ),
    ];
    for (name, routes, rule, message) in cases {
        let root = project(name, Some(routes));
        let checked = jadpo(&root, &["check"]);
        assert_eq!(
            checked.code, 1,
            "{name}: {}{}",
            checked.stdout, checked.stderr
        );
        let said = format!("{}{}", checked.stdout, checked.stderr);
        assert!(said.contains(&format!("rule: {rule}")), "{name}: {said}");
        assert!(said.contains(message), "{name}: {said}");
    }
    // A `GET` runs a query, which sets off nothing to wait for.
    let get = route_with(
        "    POST / {\n        input: OrderRequest\n        output: OrderView\n        status: SuccessStatus.created\n        run: place_order(input, current_principal.user)\n",
        "    POST / {\n        input: OrderRequest\n        output: OrderView\n        status: SuccessStatus.created\n        run: place_order(input, current_principal.user)\n    }\n\n    GET /{order_id: Order.id} {\n        output: OrderView\n        run: order_status(order_id)\n",
    )
    .replace(
        "use shop/types { OrderRequest, OrderView }\n",
        "use shop/entities { Order }\nuse shop/types { OrderRequest, OrderView }\n",
    );
    let root = project("on-a-get", Some(&get));
    let checked = jadpo(&root, &["check"]);
    let said = format!("{}{}", checked.stdout, checked.stderr);
    assert!(
        said.contains("`GET /orders/{order_id}` runs a query, which sets off no work to wait for"),
        "{said}"
    );
}

/// A store, a clock, random bytes from a counter, and a `within:` that
/// passes as soon as it is waited on when the test says so; every other
/// timer never ends.
struct Store {
    db: Postgres,
    seeds: Cell<u64>,
    clock: Cell<i64>,
    drawn: Cell<u64>,
    within_passes: Cell<bool>,
}

impl Host for Store {
    fn database(&self, _store: &str) -> Option<&dyn Database> {
        Some(&self.db)
    }

    fn random(&self, bytes: &mut [u8]) -> bool {
        for chunk in bytes.chunks_mut(8) {
            self.drawn.set(self.drawn.get() + 1);
            let mut z = self.drawn.get().wrapping_mul(0x9E37_79B9_7F4A_7C15);
            z = (z ^ (z >> 31)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            for (to, from) in chunk.iter_mut().zip(z.to_be_bytes()) {
                *to = from;
            }
        }
        true
    }

    fn now_ms(&self) -> Option<i64> {
        Some(self.clock.get())
    }

    fn sleep(&self, millis: u64) -> LocalFuture<'_, ()> {
        if millis == WITHIN_MS && self.within_passes.get() {
            Box::pin(Passes(false))
        } else {
            Box::pin(std::future::pending())
        }
    }
}

/// A timer that has not finished the first time it is looked at, and has
/// every time after: `within:` passing while the first follow-on runs.
struct Passes(bool);

impl std::future::Future for Passes {
    type Output = ();

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<()> {
        if self.0 {
            return std::task::Poll::Ready(());
        }
        self.0 = true;
        cx.waker().wake_by_ref();
        std::task::Poll::Pending
    }
}

/// The route's `within:`.
const WITHIN_MS: u64 = 2_000;

impl Store {
    fn ctx<'p>(&'p self, program: &'p Program) -> Ctx<'p> {
        self.seeds.set(self.seeds.get() + 1);
        let env = Env {
            seed: Some(self.seeds.get()),
            clock: Clock::fixed(self.clock.get()),
        };
        Ctx::new(program.places(), self, env)
    }

    async fn rows(&self, sql: &str) -> Vec<Row> {
        self.db
            .query_once(sql, &[])
            .await
            .expect("the store answers")
    }

    /// Each delivery to `subscriber` of the order for `item`: its state,
    /// attempts and last failure's code.
    async fn deliveries(&self, subscriber: &str, item: &str) -> Vec<(String, i32, Option<String>)> {
        let sql = format!(
            "SELECT \"d\".\"status\", \"d\".\"attempts\", \"d\".\"last_failure\" ->> 'code' FROM \"jadpo_deliveries\" AS \"d\" JOIN \"jadpo_outbox\" AS \"o\" ON \"o\".\"id\" = \"d\".\"outbox_id\" JOIN \"order\" AS \"r\" ON \"r\".\"id\"::text = \"o\".\"payload\" ->> 'order_id' WHERE \"d\".\"subscriber\" LIKE '%{subscriber}' AND \"r\".\"item\" = '{item}' ORDER BY \"o\".\"emitted_at\", \"o\".\"id\""
        );
        self.rows(&sql)
            .await
            .into_iter()
            .map(|row| match row.as_slice() {
                [SqlValue::Text(status), SqlValue::Int4(attempts), code] => (
                    status.clone(),
                    *attempts,
                    match code {
                        SqlValue::Text(code) => Some(code.clone()),
                        _ => None,
                    },
                ),
                other => panic!("not a delivery: {other:?}"),
            })
            .collect()
    }
}

fn field(record: &Value, name: &str) -> Value {
    match record {
        Value::Record(record) => record.get(name).cloned().expect("the field"),
        other => panic!("not a record: {other:?}"),
    }
}

fn body(response: &Response) -> serde_json::Value {
    serde_json::from_slice(&response.body).unwrap_or(serde_json::Value::Null)
}

/// `(confirmed, stamped)` of the order a response sends.
fn progress(response: &Response) -> (bool, bool) {
    let order = body(response);
    match (order["confirmed"].as_bool(), order["stamped"].as_bool()) {
        (Some(confirmed), Some(stamped)) => (confirmed, stamped),
        _ => panic!(
            "not an order: {} {}",
            response.status,
            String::from_utf8_lossy(&response.body)
        ),
    }
}

/// When the test starts: 2026-10-19T09:00:00Z.
const T0: i64 = 1_792_400_400_000;

#[test]
fn a_route_waits_for_its_follow_on_work_then_answers_with_its_query() {
    let root = project("live", None);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    if !jadpo_test::live() {
        return;
    }
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let host = Store {
        db: postgres(&url).expect("store"),
        seeds: Cell::new(0),
        clock: Cell::new(T0),
        drawn: Cell::new(0),
        within_passes: Cell::new(false),
    };
    let app = program.app();
    let route = app
        .table
        .routes
        .iter()
        .position(|route| route.label() == "POST /orders")
        .expect("the route") as u32;
    let worker = Worker::default();
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let join = program.plan().find("join").expect("join");
        let mut ctx = host.ctx(&program);
        let ann = match program.run(&mut ctx, join, vec![Value::text("ann")]).await {
            jadpo_runtime::Outcome::Value(person) => person,
            other => panic!("ann joins: {other:?}"),
        };
        let caller = jadpo_runtime::http::identity::principal(
            PrincipalKind::User,
            field(&ann, "id"),
            field(&ann, "handle"),
        );
        let mut ctx = host.ctx(&program);
        let signed_in = jadpo_runtime::http::identity::test_credentials(
            &mut ctx, app.table, app.auth, route, &caller,
        )
        .await
        .expect("signed in");
        let order = |item: &str, key: &str| {
            let mut headers = signed_in.clone();
            headers.push(("content-type".to_string(), "application/json".to_string()));
            headers.push(("idempotency-key".to_string(), key.to_string()));
            Request {
                method: "POST".to_string(),
                path: "/orders".to_string(),
                headers,
                body: serde_json::json!({ "item": item }).to_string().into_bytes(),
                address: Some("settle test".to_string()),
                ..Request::default()
            }
        };
        let send = |request: Request| {
            let (app, program, host, worker) = (&app, &program, &host, &worker);
            async move { serve_request(app, program, host, worker, &request).await }
        };

        // The order is confirmed and then stamped, two deliveries deep,
        // before the route answers: 201 with the query's view of it, and
        // nothing left for after the response.
        let teapot = send(order("teapot", "k-teapot")).await;
        assert_eq!(
            teapot.status,
            201,
            "{}",
            String::from_utf8_lossy(&teapot.body)
        );
        assert_eq!(progress(&teapot), (true, true));
        assert!(teapot.deferred.is_empty(), "{:?}", teapot.deferred);
        assert_eq!(
            host.deliveries("confirm_order", "teapot").await,
            [("succeeded".to_string(), 1, None)]
        );
        assert_eq!(
            host.deliveries("stamp_order", "teapot").await,
            [("succeeded".to_string(), 1, None)]
        );

        // A follow-on that fails for good has settled: 201, the order as
        // the failure left it.
        let fragile = send(order("fragile", "k-fragile")).await;
        assert_eq!(fragile.status, 201);
        assert_eq!(progress(&fragile), (false, false));
        assert_eq!(
            host.deliveries("confirm_order", "fragile").await,
            [("failed".to_string(), 1, Some("out_of_stock".to_string()))]
        );
        assert!(fragile.deferred.is_empty());

        // One waiting to retry after a backoff hasn't: 202 at once, the
        // order as it is, and the retry left for the sweep.
        let later = send(order("later", "k-later")).await;
        assert_eq!(later.status, 202);
        assert_eq!(progress(&later), (false, false));
        assert_eq!(
            host.deliveries("confirm_order", "later").await,
            [(
                "pending".to_string(),
                1,
                Some("warehouse_closed".to_string())
            )]
        );
        assert!(later.deferred.is_empty(), "the retry is the sweep's");

        // Past `within:`, the subscriber's action running is dropped and
        // its delivery given back as it was: 202 with the order as it is,
        // and the work carries on after the response, exactly as it would
        // without `settle:`.
        host.within_passes.set(true);
        let kettle = send(order("kettle", "k-kettle")).await;
        host.within_passes.set(false);
        assert_eq!(
            kettle.status,
            202,
            "{}",
            String::from_utf8_lossy(&kettle.body)
        );
        assert_eq!(progress(&kettle), (false, false));
        assert_eq!(
            host.deliveries("confirm_order", "kettle").await,
            [("pending".to_string(), 0, None)],
            "given back as it was, its cut-short attempt not counted"
        );
        assert!(!kettle.deferred.is_empty());
        host.clock.set(T0 + 1_000);
        run_deferred(&program, &host, kettle.deferred.clone()).await;
        assert_eq!(
            host.deliveries("confirm_order", "kettle").await,
            [("succeeded".to_string(), 1, None)]
        );
        assert_eq!(
            host.deliveries("stamp_order", "kettle").await,
            [("succeeded".to_string(), 1, None)]
        );

        // A repeat with the same key runs the query again rather than the
        // action: it shows how far the work has got, with the status the
        // first answer had.
        let again = send(order("kettle", "k-kettle")).await;
        assert_eq!(again.status, 202);
        assert_eq!(progress(&again), (true, true));
        assert_eq!(body(&again)["id"], body(&kettle)["id"], "the same order");
        let again = send(order("teapot", "k-teapot")).await;
        assert_eq!(again.status, 201);
        assert_eq!(body(&again), body(&teapot));
        assert_eq!(
            host.rows("SELECT count(*)::int4 FROM \"order\"").await,
            vec![vec![SqlValue::Int4(4)]],
            "no order placed twice"
        );
    });
    // The project's own route tests, on a disposable database.
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("3 passed"), "{}", tested.stdout);
}

const FILES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"use shop/entities { Person }

application {}

principal {
    user: {
        subject: Person.handle
    }
}
"#,
    ),
    (
        "authentication.jadpo",
        r#"authentication {
    browser_session: {
        cookie: "shop_session"
    }
}
"#,
    ),
    (
        "domains/shop/actions/confirm.jadpo",
        r#"use shop/entities { Order, OrderNotFound }
use shop/types { Item }
use shop/failures { OutOfStock, WarehouseClosed }
use shop/events { OrderConfirmed }

action confirm(
    order_id: Order.id,
    item: Item
) fails OrderNotFound, OutOfStock, WarehouseClosed -> Unit {
    if (item == "fragile") {
        reject OutOfStock {}
    }
    if (item == "later") {
        reject WarehouseClosed {}
    }
    attempt Order.update(order_id) {
        set: {
            confirmed: true
        }
    }
    emit OrderConfirmed {
        order_id: order_id
    }
}
"#,
    ),
    (
        "domains/shop/actions/join.jadpo",
        r#"use shop/entities { Person }
use shop/types { Handle }
use shop/failures { HandleTaken }

action join(handle: Handle) fails HandleTaken -> Person {
    return attempt Person.create {
        set: {
            handle: handle
        }
        conflict: {
            Person.handle: HandleTaken
        }
    }
}
"#,
    ),
    (
        "domains/shop/actions/place_order.jadpo",
        r#"use shop/entities { Order }
use shop/types { OrderRequest }
use shop/events { OrderPlaced }

/// Places an order; confirming and stamping it follow in subscribers.
action place_order(input: OrderRequest, by: Principal.user) -> Order {
    var order: Order = Order.create {
        set: {
            owner_id: by.id
            item: input.item
            confirmed: false
            stamped: false
        }
    }
    emit OrderPlaced {
        order_id: order.id
        item: order.item
    }
    return order
}
"#,
    ),
    (
        "domains/shop/actions/stamp.jadpo",
        r#"use shop/entities { Order, OrderNotFound }

action stamp(order_id: Order.id) fails OrderNotFound -> Unit {
    attempt Order.update(order_id) {
        set: {
            stamped: true
        }
    }
}
"#,
    ),
    (
        "domains/shop/entities/order.jadpo",
        r#"use shop/entities { Person }
use shop/types { Item }

entity Order {
    id: Uuid
    owner_id: Person.id {
        on_delete: OnDelete.cascade
    }
    item: Item
    confirmed: Bool
    stamped: Bool

    policy: {
        create: [owner_id]
        read: [owner_id, Access.system]
        update: [Access.system]
    }
}
"#,
    ),
    (
        "domains/shop/entities/person.jadpo",
        r#"use shop/types { Handle }

entity Person {
    id: Uuid
    handle: Handle {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [id]
    }
}
"#,
    ),
    (
        "domains/shop/events/order_confirmed.jadpo",
        r#"use shop/entities { Order }

/// An order was confirmed.
type OrderConfirmed = Event {
    order_id: Order.id
}
"#,
    ),
    (
        "domains/shop/events/order_placed.jadpo",
        r#"use shop/entities { Order }
use shop/types { Item }

/// An order was placed.
type OrderPlaced = Event {
    order_id: Order.id
    item: Item
}
"#,
    ),
    (
        "domains/shop/failures/handle_taken.jadpo",
        r#"type HandleTaken = Failure {
    kind: FailureKind.conflict
    message: "That handle is taken."
}
"#,
    ),
    (
        "domains/shop/failures/out_of_stock.jadpo",
        r#"type OutOfStock = Failure {
    kind: FailureKind.rejected
    message: "That item is out of stock."
}
"#,
    ),
    (
        "domains/shop/failures/warehouse_closed.jadpo",
        r#"type WarehouseClosed = Failure {
    kind: FailureKind.unavailable
    message: "The warehouse is closed for now."
}
"#,
    ),
    (
        "domains/shop/queries/order_status.jadpo",
        r#"use shop/entities { Order, OrderNotFound }
use shop/types { OrderView }

/// One order, as its owner sees it.
query order_status(order_id: Order.id) fails OrderNotFound -> OrderView {
    from: Order
    where: Order.id == order_id
    missing: OrderNotFound
}
"#,
    ),
    (
        "domains/shop/routes/orders.jadpo",
        r#"use shop/types { OrderRequest, OrderView }
use shop/actions { place_order }
use shop/queries { order_status }

routes Orders {
    prefix: /orders

    /// Places an order and answers once it is confirmed and stamped.
    POST / {
        input: OrderRequest
        output: OrderView
        status: SuccessStatus.created
        run: place_order(input, current_principal.user)
        settle: {
            answer: order_status(result.id)
            within: 2s
        }
    }
}
"#,
    ),
    (
        "domains/shop/routes/orders.test.jadpo",
        r#"use shop/actions { join }
use shop/entities { Person }
use shop/types { OrderRequest }

test "an order answers once it is confirmed and stamped" {
    var ann: Person = attempt join("ann")
    var response: Response = request {
        method: HttpMethod.post
        path: "/orders"
        as: Principal.user(ann)
        input: OrderRequest {
            item: "teapot"
        }
    }
    assert response.status == 201
    var done: Bool = match response.output {
        some(order) -> order.confirmed and order.stamped
        none -> false
    }
    assert done
}

test "a follow-on left to retry answers 202 with the order as it is" {
    var bob: Person = attempt join("bob")
    var response: Response = request {
        method: HttpMethod.post
        path: "/orders"
        as: Principal.user(bob)
        input: OrderRequest {
            item: "later"
        }
    }
    assert response.status == 202
    var waiting: Bool = match response.output {
        some(order) -> not order.confirmed
        none -> false
    }
    assert waiting
}

test "a follow-on that fails for good has settled" {
    var cat: Person = attempt join("cat")
    var response: Response = request {
        method: HttpMethod.post
        path: "/orders"
        as: Principal.user(cat)
        input: OrderRequest {
            item: "fragile"
        }
    }
    assert response.status == 201
    var refused: Bool = match response.output {
        some(order) -> not order.confirmed and not order.stamped
        none -> false
    }
    assert refused
}
"#,
    ),
    (
        "domains/shop/subscribers/confirm_order.jadpo",
        r#"use shop/actions { confirm }
use shop/events { OrderPlaced }

/// Confirms each order placed.
subscriber confirm_order {
    on: OrderPlaced
    run: confirm(event.order_id, event.item)
    retry: {
        attempts: 3
        within: 1h
    }
}
"#,
    ),
    (
        "domains/shop/subscribers/stamp_order.jadpo",
        r#"use shop/actions { stamp }
use shop/events { OrderConfirmed }

/// Stamps each order confirmed.
subscriber stamp_order {
    on: OrderConfirmed
    run: stamp(event.order_id)
}
"#,
    ),
    (
        "domains/shop/types/handle.jadpo",
        r#"type Handle = Text {
    min_length: 3
    max_length: 40
}
"#,
    ),
    (
        "domains/shop/types/item.jadpo",
        r#"type Item = Text {
    min_length: 1
    max_length: 80
}
"#,
    ),
    (
        "domains/shop/types/order_request.jadpo",
        r#"use shop/types { Item }

type OrderRequest = Input {
    item: Item
}
"#,
    ),
    (
        "domains/shop/types/order_view.jadpo",
        r#"use shop/entities { Order }

type OrderView = Output {
    pick: [Order.id, Order.item, Order.confirmed, Order.stamped]
}
"#,
    ),
];
