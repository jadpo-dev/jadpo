//! Cloudflare Queues on Workers, against live PostgreSQL (area 12,
//! "Events from outside"; area 11, "Queues"; chunk 7.7c2): a connection
//! with `driver: Driver.cloudflare_queues` names a queue Cloudflare pushes
//! to the app's consumer a batch at a time. The batches are handed in
//! here, in-process, as the Workers host's consumer will hand them
//! (`receiving::consume`): each message is kept once as its event, as a
//! fetched one is, and acknowledged; a repeat and a message that can't be
//! kept, parked for an operator, are acknowledged too; one the store
//! can't take now is retried after a delay; a batch from a queue the app
//! doesn't receive is retried whole. A message whose retries are spent,
//! handed back from the queue's dead-letter queue, is parked for an
//! operator (chunk 8.4b). The tick reads again a parked message an
//! operator retried and fetches nothing. The consumers the Wrangler file
//! gets are written from the plan.
//!
//! Nothing reaches Cloudflare or a network. The database is the one
//! `JADPO_TEST_DATABASE_URL` names or one started from the machine's
//! PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips the live tests.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use jadpo_cli::{Output, run};
use jadpo_host_native::Runtime;
use jadpo_interp::{Prepared, Program};
use jadpo_runtime::receiving::{Answer, Pushed, consume};
use jadpo_runtime::runner::run_deferred;
use jadpo_runtime::schedules::tick;
use jadpo_runtime::{Outcome, SqlValue};
use jadpo_term::Environment;
use jadpo_test::host::Harness;

const FILES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"use stock/events { OrderShipped, StockLow }

application {
    connections: {
        warehouse_queue: {
            driver: Driver.cloudflare_queues
            queue: "warehouse-events"
            dead_letter_queue: "warehouse-events-dead"
        }
    }
    receiving: {
        warehouse_queue: {
            type_at: "type"
            id_at: "message_id"
            events: [OrderShipped, StockLow]
        }
    }
}
"#,
    ),
    (
        "domains/stock/events/order_shipped.jadpo",
        r#"/// An order left the warehouse.
type OrderShipped = Event {
    wire_name: "order.shipped"
    order: Text {
        max_length: 12
    }
}
"#,
    ),
    (
        "domains/stock/events/stock_low.jadpo",
        r#"/// A product is running out.
type StockLow = Event {
    wire_name: "stock.low"
    sku: Text {
        max_length: 40
    }
    left: Int {
        min: 0
    }
}
"#,
    ),
    (
        "domains/stock/entities/shipment.jadpo",
        r#"entity Shipment {
    id: Uuid
    order: Text {
        max_length: 12
    }

    policy: {
        create: [Access.system]
        read: [Access.system]
    }
}
"#,
    ),
    (
        "domains/stock/actions/record_shipment.jadpo",
        r#"use stock/entities { Shipment }
use stock/events { OrderShipped }

/// Records an order the warehouse shipped.
action record_shipment(event: OrderShipped) -> Shipment {
    return Shipment.create {
        set: {
            order: event.order
        }
    }
}
"#,
    ),
    (
        "domains/stock/subscribers/shipped.jadpo",
        r#"use stock/actions { record_shipment }
use stock/events { OrderShipped }

/// Records each order the warehouse ships.
subscriber shipped {
    on: OrderShipped
    run: record_shipment(event)
}
"#,
    ),
];

/// When the test starts: 2026-10-20T09:00:00Z.
const T0: i64 = 1_792_486_800_000;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty()
        .with_var("SOURCE_DATE_EPOCH", &(T0 / 1000).to_string())
        .with_var("NO_COLOR", "1")
        .with_var("USER", "sam");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    run(&args, &env.with_cwd(cwd))
}

/// The project, checked, with its migration.
fn project(name: &str) -> (PathBuf, Arc<Prepared>) {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("cloudflare-queues-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, text);
    }
    for args in [
        &["migrate", "new", "initial"][..],
        &["records", "update"],
        &["check"],
    ] {
        let out = jadpo(&root, args);
        assert_eq!(out.code, 0, "{args:?}: {}{}", out.stdout, out.stderr);
    }
    let loaded = jadpo_db::load(&root);
    let prepared = Prepared::load(&loaded.db, loaded.project).unwrap_or_else(|problems| {
        panic!(
            "the project checks: {:?}",
            problems.iter().map(|p| &p.message).collect::<Vec<_>>()
        )
    });
    (root, prepared)
}

struct Loaded {
    root: PathBuf,
    program: Program,
    _databases: jadpo_test::database::TestDatabases,
    url: String,
}

/// The project with a test database of its own; `None` when live tests
/// are skipped.
fn load(name: &str) -> Option<Loaded> {
    let (root, prepared) = project(name);
    if !jadpo_test::live() {
        return None;
    }
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = Program::new(prepared);
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    Some(Loaded {
        root,
        program,
        _databases: databases,
        url,
    })
}

/// The host a batch is consumed on: the test's database and clock, and
/// no capability, so nothing is fetched.
fn host(url: &str) -> Harness<jadpo_host_native::Postgres> {
    Harness::postgres(url).at(T0).random_from(0)
}

fn pushed(body: &str, attempts: u32) -> Pushed {
    Pushed {
        body: body.to_string(),
        attempts,
    }
}

fn shipped(id: &str, order: &str) -> String {
    format!(r#"{{"type":"order.shipped","message_id":"{id}","order":"{order}"}}"#)
}

/// The consumer the Wrangler file gets: the queue, ten messages a batch,
/// five seconds to fill one, 30 retries, then the dead-letter queue, whose
/// own consumer parks what reaches it (chunk 8.4b), retried as long as
/// Cloudflare allows.
#[test]
fn the_wrangler_file_gets_a_consumer_for_each_cloudflare_queue() {
    let (_, prepared) = project("wrangler");
    let program = Program::new(prepared);
    let queues = &program.plan().providers().queues;
    assert_eq!(
        jadpo_std::receiving::wrangler_queues(queues),
        Some(serde_json::json!({
            "consumers": [
                {
                    "queue": "warehouse-events",
                    "max_batch_size": 10,
                    "max_batch_timeout": 5,
                    "max_retries": 30,
                    "dead_letter_queue": "warehouse-events-dead",
                },
                {
                    "queue": "warehouse-events-dead",
                    "max_batch_size": 10,
                    "max_batch_timeout": 5,
                    "max_retries": 100,
                },
            ]
        }))
    );
}

/// A message whose retries are spent comes back through the queue's
/// dead-letter queue (chunk 8.4b; area 12, "Events from outside"): its
/// consumer parks it whole for an operator, with what could be read of
/// it, keeping no event, and acknowledges it; `jadpo deliveries` lists
/// it, and once an operator retries it the tick keeps it. One the store
/// can't park now is retried after a delay.
#[test]
fn a_dead_lettered_message_is_parked_for_an_operator() {
    let Some(loaded) = load("dead") else {
        return;
    };
    let host = host(&loaded.url);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let consumed = consume(
            &loaded.program,
            &host,
            "warehouse-events-dead",
            &[pushed(&shipped("w-7", "A-107"), 1), pushed("not json", 1)],
            T0,
        )
        .await;
        assert_eq!(consumed.answers, [Answer::Ack; 2]);
        let received = &consumed.received;
        assert!(received.problems.is_empty(), "{:?}", received.problems);
        assert_eq!(
            (received.kept, received.parked, received.left),
            (0, 2, 0)
        );
        assert!(consumed.received.deferred.is_empty());
        assert_eq!(host.count("SELECT count(*) FROM \"jadpo_outbox\"").await, 0);
        let parked = host
            .rows("SELECT \"queue\", \"reason\", \"status\", coalesce(\"message_id\", '-'), \"event\" IS NOT NULL FROM \"jadpo_parked_messages\" ORDER BY \"message_id\" NULLS LAST")
            .await;
        assert_eq!(
            parked,
            vec![
                vec![
                    SqlValue::Text("warehouse_queue".into()),
                    SqlValue::Text("dead_lettered".into()),
                    SqlValue::Text("failed".into()),
                    SqlValue::Text("w-7".into()),
                    SqlValue::Bool(true),
                ],
                vec![
                    SqlValue::Text("warehouse_queue".into()),
                    SqlValue::Text("dead_lettered".into()),
                    SqlValue::Text("failed".into()),
                    SqlValue::Text("-".into()),
                    SqlValue::Bool(false),
                ],
            ]
        );
    });
    let id = runtime
        .block_on(host.rows("SELECT replace(\"id\"::text, '-', '') FROM \"jadpo_parked_messages\" WHERE \"message_id\" = 'w-7'"))
        .first()
        .and_then(|row| match row.first() {
            Some(SqlValue::Text(id)) => Some(id[24..].to_string()),
            _ => None,
        })
        .expect("a parked message");
    let listed = jadpo(&loaded.root, &["deliveries", "--url", &loaded.url]);
    assert_eq!(listed.code, 0, "{}{}", listed.stdout, listed.stderr);
    assert!(
        listed.stdout.contains("queue warehouse_queue · OrderShipped")
            && listed.stdout.contains("dead_lettered"),
        "{}",
        listed.stdout
    );
    let retried = jadpo(
        &loaded.root,
        &["deliveries", "retry", &id, "--url", &loaded.url],
    );
    assert_eq!(retried.code, 0, "{}{}", retried.stdout, retried.stderr);
    runtime.block_on(async {
        host.set_now(T0 + 60_000);
        let ticked = tick(&loaded.program, &host, T0 + 60_000).await;
        assert!(ticked.problems.is_empty(), "{:?}", ticked.problems);
        assert_eq!(ticked.received.map(|tally| tally.kept), Some(1));
        // The store loses the table messages are parked in: a message
        // from the dead-letter queue is retried, its delay growing.
        host.rows("ALTER TABLE \"jadpo_parked_messages\" RENAME TO \"gone\"")
            .await;
        let left = consume(
            &loaded.program,
            &host,
            "warehouse-events-dead",
            &[pushed(&shipped("w-8", "A-108"), 2)],
            T0 + 60_000,
        )
        .await;
        assert_eq!(left.answers, [Answer::Retry { delay_seconds: 20 }]);
        assert_eq!(left.received.left, 1);
        host.rows("ALTER TABLE \"gone\" RENAME TO \"jadpo_parked_messages\"")
            .await;
    });
}

/// Each message of a batch becomes its event, kept once with its id; a
/// repeat keeps nothing; a type the queue doesn't list and a body that
/// isn't JSON are parked whole; every one is acknowledged, and the
/// subscriber records the shipment once the host runs the deliveries.
#[test]
fn a_batch_is_kept_once_and_every_message_acknowledged() {
    let Some(loaded) = load("kept") else {
        return;
    };
    let host = host(&loaded.url);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let batch = [
            pushed(&shipped("w-1", "A-100"), 1),
            pushed(
                r#"{"type":"stock.low","message_id":"w-2","sku":"PEG-7","left":3}"#,
                1,
            ),
            pushed(&shipped("w-1", "A-100"), 1),
            pushed(
                r#"{"type":"order.cancelled","message_id":"w-3","order":"A-101"}"#,
                1,
            ),
            pushed("not json", 2),
        ];
        let consumed = consume(&loaded.program, &host, "warehouse-events", &batch, T0).await;
        assert_eq!(consumed.answers, [Answer::Ack; 5]);
        let received = &consumed.received;
        assert!(received.problems.is_empty(), "{:?}", received.problems);
        assert_eq!(
            (
                received.kept,
                received.repeats,
                received.parked,
                received.left
            ),
            (2, 1, 2, 0)
        );
        let outcomes = run_deferred(&loaded.program, &host, consumed.received.deferred).await;
        assert!(
            !outcomes.is_empty()
                && outcomes
                    .iter()
                    .all(|outcome| matches!(outcome, Outcome::Value(_))),
            "{outcomes:?}"
        );
        let shipments = host.rows("SELECT \"order\" FROM \"shipment\"").await;
        assert_eq!(shipments, vec![vec![SqlValue::Text("A-100".into())]]);
        let ids = host
            .rows("SELECT \"source\", \"event_id\" FROM \"jadpo_received_events\" ORDER BY 2")
            .await;
        assert_eq!(
            ids,
            vec![
                vec![
                    SqlValue::Text("queue warehouse_queue".into()),
                    SqlValue::Text("w-1".into())
                ],
                vec![
                    SqlValue::Text("queue warehouse_queue".into()),
                    SqlValue::Text("w-2".into())
                ],
            ]
        );
        let parked = host
            .rows("SELECT \"queue\", \"reason\", \"status\" FROM \"jadpo_parked_messages\" ORDER BY \"reason\"")
            .await;
        assert_eq!(
            parked,
            vec![
                vec![
                    SqlValue::Text("warehouse_queue".into()),
                    SqlValue::Text("unlisted_type".into()),
                    SqlValue::Text("failed".into())
                ],
                vec![
                    SqlValue::Text("warehouse_queue".into()),
                    SqlValue::Text("unreadable".into()),
                    SqlValue::Text("failed".into())
                ],
            ]
        );
        // Cloudflare hands the same message again: a repeat, acknowledged.
        let again = consume(
            &loaded.program,
            &host,
            "warehouse-events",
            &[pushed(&shipped("w-1", "A-100"), 2)],
            T0 + 1_000,
        )
        .await;
        assert_eq!(again.answers, [Answer::Ack]);
        assert_eq!(again.received.repeats, 1);
        assert_eq!(host.count("SELECT count(*) FROM \"jadpo_outbox\"").await, 2);
    });
}

/// A parked message an operator retried is read again by the tick, which
/// fetches nothing from a queue Cloudflare pushes.
#[test]
fn the_tick_reads_again_a_parked_message_an_operator_retried() {
    let Some(loaded) = load("retried") else {
        return;
    };
    let host = host(&loaded.url);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let consumed = consume(
            &loaded.program,
            &host,
            "warehouse-events",
            &[pushed(&shipped("w-4", "AN-ORDER-NUMBER-TOO-LONG"), 1)],
            T0,
        )
        .await;
        assert_eq!(consumed.answers, [Answer::Ack]);
        assert_eq!(consumed.received.parked, 1);
        // Nothing parked waits for the tick, and it fetches nothing.
        let ticked = tick(&loaded.program, &host, T0).await;
        assert!(ticked.problems.is_empty(), "{:?}", ticked.problems);
        assert_eq!(ticked.received, None);
        host.rows(&format!(
            "UPDATE \"jadpo_parked_messages\" SET \"body\" = '{}' RETURNING 1",
            shipped("w-4", "A-104")
        ))
        .await;
    });
    let id = runtime
        .block_on(host.rows("SELECT replace(\"id\"::text, '-', '') FROM \"jadpo_parked_messages\""))
        .first()
        .and_then(|row| match row.first() {
            Some(SqlValue::Text(id)) => Some(id[24..].to_string()),
            _ => None,
        })
        .expect("a parked message");
    let listed = jadpo(&loaded.root, &["deliveries", "--url", &loaded.url]);
    assert_eq!(listed.code, 0, "{}{}", listed.stdout, listed.stderr);
    assert!(
        listed
            .stdout
            .contains("queue warehouse_queue · OrderShipped"),
        "{}",
        listed.stdout
    );
    let retried = jadpo(
        &loaded.root,
        &["deliveries", "retry", &id, "--url", &loaded.url],
    );
    assert_eq!(retried.code, 0, "{}{}", retried.stdout, retried.stderr);
    runtime.block_on(async {
        host.set_now(T0 + 60_000);
        let ticked = tick(&loaded.program, &host, T0 + 60_000).await;
        assert!(ticked.problems.is_empty(), "{:?}", ticked.problems);
        assert_eq!(ticked.received.map(|tally| tally.kept), Some(1));
        let shipments = host.rows("SELECT \"order\" FROM \"shipment\"").await;
        assert_eq!(shipments, vec![vec![SqlValue::Text("A-104".into())]]);
        assert_eq!(
            host.count("SELECT count(*) FROM \"jadpo_parked_messages\"")
                .await,
            0
        );
    });
}

/// A message the store can't take now is retried, each attempt waiting
/// longer; a batch from a queue the app doesn't receive is retried whole,
/// and keeps nothing.
#[test]
fn what_cant_be_kept_now_is_retried_after_a_delay() {
    let Some(loaded) = load("left") else {
        return;
    };
    let host = host(&loaded.url);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let unknown = consume(
            &loaded.program,
            &host,
            "billing-events",
            &[pushed(&shipped("w-5", "A-105"), 1), pushed("{}", 3)],
            T0,
        )
        .await;
        assert_eq!(
            unknown.answers,
            [
                Answer::Retry { delay_seconds: 10 },
                Answer::Retry { delay_seconds: 40 }
            ]
        );
        assert_eq!(unknown.received.left, 2);
        let (source, _) = &unknown.received.problems[0];
        assert_eq!(source, "cloudflare queue billing-events");
        // The store loses the table ids are kept in: nothing can be kept.
        host.rows("ALTER TABLE \"jadpo_received_events\" RENAME TO \"gone\" ")
            .await;
        let left = consume(
            &loaded.program,
            &host,
            "warehouse-events",
            &[pushed(&shipped("w-6", "A-106"), 2)],
            T0,
        )
        .await;
        assert_eq!(left.answers, [Answer::Retry { delay_seconds: 20 }]);
        assert_eq!(left.received.left, 1);
        assert_eq!(
            left.received.problems.len(),
            1,
            "{:?}",
            left.received.problems
        );
        assert_eq!(left.received.problems[0].0, "main");
        host.rows("ALTER TABLE \"gone\" RENAME TO \"jadpo_received_events\"")
            .await;
        assert_eq!(host.count("SELECT count(*) FROM \"jadpo_outbox\"").await, 0);
    });
}
