//! Queues through `receiving`, against live PostgreSQL and a local SQS
//! (area 12, "Events from outside"; area 11, "Queues"; chunk 7.7c): the
//! app's tick fetches a queue through the SQS package
//! (`packages/amazon_sqs`, installed as an app installs it), each request
//! signed with AWS Signature Version 4; each message is kept once as its
//! event, with its id in `jadpo_received_events`, then deleted from the
//! queue, and delivered to the app's subscriber; a repeat is deleted and
//! keeps nothing; a message of a type the queue doesn't list, or one its
//! event can't read, is parked whole for an operator and deleted, listed by
//! `jadpo deliveries`, read again on the next tick once retried, or set
//! aside when dismissed; and a request the queue refuses keeps nothing and
//! leaves every message where it is, the credential in no problem.
//!
//! A warehouse sends `order.shipped` and `stock.low` messages to the shop's
//! queue. Nothing reaches a network: the queue is a server on this machine
//! (`support::fake_sqs`). The database is the one
//! `JADPO_TEST_DATABASE_URL` names or one started from the machine's
//! PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use jadpo_cli::{Output, run};
use jadpo_drivers::package::JsonHttp;
use jadpo_host_native::Runtime;
use jadpo_host_native::fetch::NativeFetch;
use jadpo_interp::{Prepared, Program};
use jadpo_runtime::capability::{Capability, Faked};
use jadpo_runtime::schedules::tick;
use jadpo_runtime::{Outcome, SqlValue};
use jadpo_term::Environment;
use jadpo_test::host::Harness;

use crate::support::fake_sqs::FakeSqs;

const KEY_ID: &str = "AKIDEXAMPLE";
const SECRET: &str = "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY";

const FILES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"use stock/events { OrderShipped, StockLow }

application {
    connections: {
        warehouse_queue: {
            package: AmazonSqs.v2012_11_05
            domains: [stock]
            credential: {
                key_id: config.aws_key_id
                secret: config.aws_secret_key
            }
            host: config.sqs_endpoint
            queue_url: config.warehouse_queue_url
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
        "config.jadpo",
        r#"config {
    aws_key_id: Text
    aws_secret_key: Text {
        secret: true
    }
    sqs_endpoint: Url
    warehouse_queue_url: Text
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

/// The SQS package as this repository holds it.
fn sqs_package() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/amazon_sqs")
}

struct Loaded {
    root: PathBuf,
    program: Program,
    _databases: jadpo_test::database::TestDatabases,
    url: String,
}

/// The project with the SQS package installed, checked, with its
/// migration, set to read `sqs`'s queue with `secret`, and a test database
/// of its own; `None` when live tests are skipped.
fn load(name: &str, sqs: &FakeSqs, secret: &str) -> Option<Loaded> {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("receiving-7.7c-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, text);
    }
    let package = sqs_package();
    let package = package.to_str().expect("a path");
    for args in [
        &["package", "add", package][..],
        &["migrate", "new", "initial"],
        &["records", "update"],
        &["check"],
    ] {
        let out = jadpo(&root, args);
        assert_eq!(out.code, 0, "{args:?}: {}{}", out.stdout, out.stderr);
    }
    if !jadpo_test::live() {
        return None;
    }
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let prepared = Prepared::load(&loaded.db, loaded.project).unwrap_or_else(|problems| {
        panic!(
            "the project checks: {:?}",
            problems.iter().map(|p| &p.message).collect::<Vec<_>>()
        )
    });
    let endpoint = sqs.url.clone();
    let queue = sqs.queue_url("warehouse");
    let secret = secret.to_string();
    let variables = move |name: &str| -> Option<String> {
        match name {
            "AWS_KEY_ID" => Some(KEY_ID.into()),
            "AWS_SECRET_KEY" => Some(secret.clone()),
            "SQS_ENDPOINT" => Some(endpoint.clone()),
            "WAREHOUSE_QUEUE_URL" => Some(queue.clone()),
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
    Some(Loaded {
        root,
        program,
        _databases: databases,
        url,
    })
}

/// The host a tick runs on: the test's database and clock, with the
/// package driver, over this machine's HTTP, for the queue's connection.
fn host(url: &str) -> Faked<Harness<jadpo_host_native::Postgres>> {
    let driver: Rc<dyn Capability> = Rc::new(JsonHttp::new(NativeFetch));
    Faked::new(Harness::postgres(url).at(T0).random_from(0)).with("warehouse_queue", driver)
}

async fn count(host: &Faked<Harness<jadpo_host_native::Postgres>>, table: &str) -> i64 {
    host.host()
        .count(&format!("SELECT count(*) FROM \"{table}\""))
        .await
}

fn shipped(id: &str, order: &str) -> String {
    format!(r#"{{"type":"order.shipped","message_id":"{id}","order":"{order}"}}"#)
}

/// Each message becomes its event, kept once with its id and then deleted
/// from the queue; the subscriber records the shipment; a repeat of an id
/// is deleted and keeps nothing; an event no subscriber takes is kept all
/// the same. Every request is signed, and names its operation in
/// `X-Amz-Target`.
#[test]
fn a_queues_messages_are_kept_once_as_events_and_then_deleted() {
    let sqs = FakeSqs::start("warehouse", KEY_ID, SECRET);
    let Some(loaded) = load("kept", &sqs, SECRET) else {
        return;
    };
    sqs.send("warehouse", &shipped("w-1", "A-100"));
    sqs.send(
        "warehouse",
        r#"{"type":"stock.low","message_id":"w-2","sku":"PEG-7","left":3}"#,
    );
    sqs.send("warehouse", &shipped("w-1", "A-100"));
    let host = host(&loaded.url);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let ticked = tick(&loaded.program, &host, T0).await;
        assert!(ticked.problems.is_empty(), "{:?}", ticked.problems);
        let received = ticked.received.expect("messages received");
        assert_eq!(
            (
                received.kept,
                received.repeats,
                received.parked,
                received.left
            ),
            (2, 1, 0, 0)
        );
        assert!(
            ticked
                .deliveries
                .iter()
                .all(|outcome| matches!(outcome, Outcome::Value(_))),
            "{:?}",
            ticked.deliveries
        );
        assert!(
            sqs.left("warehouse").is_empty(),
            "{:?}",
            sqs.left("warehouse")
        );
        assert_eq!(count(&host, "jadpo_outbox").await, 2);
        let ids = host
            .host()
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
        let shipments = host.host().rows("SELECT \"order\" FROM \"shipment\"").await;
        assert_eq!(shipments, vec![vec![SqlValue::Text("A-100".into())]]);
        // Nothing more to receive: the next tick keeps nothing.
        host.host().set_now(T0 + 10_000);
        let again = tick(&loaded.program, &host, T0 + 10_000).await;
        assert!(again.problems.is_empty(), "{:?}", again.problems);
        assert_eq!(again.received, None);
    });
    let seen = sqs.seen();
    assert!(seen.iter().all(|request| request.signed), "{seen:?}");
    assert!(
        seen.iter()
            .all(|request| request.content_type == "application/x-amz-json-1.0")
    );
    assert!(seen.iter().all(
        |request| request.authorization.contains("Credential=AKIDEXAMPLE/")
            && !request.authorization.contains(SECRET)
    ));
    let targets: Vec<&str> = seen.iter().map(|request| request.target.as_str()).collect();
    assert_eq!(
        targets,
        [
            "AmazonSQS.ReceiveMessage",
            "AmazonSQS.DeleteMessage",
            "AmazonSQS.DeleteMessage",
            "AmazonSQS.DeleteMessage",
            "AmazonSQS.ReceiveMessage",
        ]
    );
    assert_eq!(seen[0].body["QueueUrl"], sqs.queue_url("warehouse"));
    assert_eq!(seen[0].body["MaxNumberOfMessages"], 10);
}

/// A message of a type the queue doesn't list, one that isn't JSON, and
/// one its event refuses are parked whole and deleted from the queue;
/// `jadpo deliveries` lists them; one retried is read again on the next
/// tick, kept once it reads, and parked again while it doesn't; one
/// dismissed is set aside.
#[test]
fn messages_that_cant_be_kept_are_parked_for_an_operator() {
    let sqs = FakeSqs::start("warehouse", KEY_ID, SECRET);
    let Some(loaded) = load("parked", &sqs, SECRET) else {
        return;
    };
    sqs.send(
        "warehouse",
        r#"{"type":"order.cancelled","message_id":"w-3","order":"A-101"}"#,
    );
    sqs.send("warehouse", "not json");
    sqs.send("warehouse", &shipped("w-4", "AN-ORDER-NUMBER-TOO-LONG"));
    let host = host(&loaded.url);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let ticked = tick(&loaded.program, &host, T0).await;
        assert!(ticked.problems.is_empty(), "{:?}", ticked.problems);
        let received = ticked.received.expect("messages received");
        assert_eq!((received.kept, received.parked), (0, 3));
        assert!(sqs.left("warehouse").is_empty());
        assert_eq!(count(&host, "jadpo_outbox").await, 0);
        let parked = host
            .host()
            .rows("SELECT \"reason\", \"status\", \"message_id\", \"problems\"::text FROM \"jadpo_parked_messages\" ORDER BY \"message_id\" NULLS LAST")
            .await;
        assert_eq!(
            parked,
            vec![
                vec![
                    SqlValue::Text("unlisted_type".into()),
                    SqlValue::Text("failed".into()),
                    SqlValue::Text("w-3".into()),
                    SqlValue::Null(jadpo_runtime::SqlType::Text),
                ],
                vec![
                    SqlValue::Text("unreadable".into()),
                    SqlValue::Text("failed".into()),
                    SqlValue::Text("w-4".into()),
                    SqlValue::Text(r#"[{"rule": "max_length", "field": "order"}]"#.into()),
                ],
                vec![
                    SqlValue::Text("unreadable".into()),
                    SqlValue::Text("failed".into()),
                    SqlValue::Null(jadpo_runtime::SqlType::Text),
                    SqlValue::Null(jadpo_runtime::SqlType::Text),
                ],
            ]
        );
    });
    let url = loaded.url.as_str();
    let listed = jadpo(&loaded.root, &["deliveries", "--url", url]);
    assert_eq!(listed.code, 0, "{}{}", listed.stdout, listed.stderr);
    for row in [
        "queue warehouse_queue · OrderShipped",
        "unreadable",
        "unlisted_type",
    ] {
        assert!(listed.stdout.contains(row), "{row}:\n{}", listed.stdout);
    }
    assert!(
        listed.stdout.contains("jadpo deliveries retry"),
        "{}",
        listed.stdout
    );
    let id_of = |which: &str| -> String {
        runtime
            .block_on(host.host().rows(&format!(
                "SELECT replace(\"id\"::text, '-', '') FROM \"jadpo_parked_messages\" WHERE {which}"
            )))
            .first()
            .and_then(|row| match row.first() {
                Some(SqlValue::Text(id)) => Some(id[24..].to_string()),
                _ => None,
            })
            .expect("a parked message")
    };
    // The order number was too long for the app's event: the operator
    // retries it once the event takes it, here by the body being put
    // right, and dismisses the type nothing reads.
    let unlisted = id_of("\"reason\" = 'unlisted_type'");
    let dismissed = jadpo(
        &loaded.root,
        &["deliveries", "dismiss", &unlisted, "--url", url],
    );
    assert_eq!(
        dismissed.code, 0,
        "{}{}",
        dismissed.stdout, dismissed.stderr
    );
    assert!(
        dismissed
            .stdout
            .contains("the message from `warehouse_queue` won't be kept"),
        "{}",
        dismissed.stdout
    );
    let wrong = jadpo(
        &loaded.root,
        &["deliveries", "cancel", &unlisted, "--url", url],
    );
    assert_eq!(wrong.code, 1);
    assert!(
        wrong
            .stderr
            .contains("is a parked message, which is only retried or dismissed"),
        "{}",
        wrong.stderr
    );
    runtime.block_on(async {
        host.host()
            .rows(&format!(
                "UPDATE \"jadpo_parked_messages\" SET \"body\" = '{}' WHERE \"message_id\" = 'w-4' RETURNING 1",
                shipped("w-4", "A-104")
            ))
            .await;
    });
    let long = id_of("\"message_id\" = 'w-4'");
    let retried = jadpo(&loaded.root, &["deliveries", "retry", &long, "--url", url]);
    assert_eq!(retried.code, 0, "{}{}", retried.stdout, retried.stderr);
    assert!(
        retried
            .stdout
            .contains("the app's next tick reads the message from `warehouse_queue` again"),
        "{}",
        retried.stdout
    );
    runtime.block_on(async {
        host.host().set_now(T0 + 20_000);
        let ticked = tick(&loaded.program, &host, T0 + 20_000).await;
        assert!(ticked.problems.is_empty(), "{:?}", ticked.problems);
        let received = ticked.received.expect("a retried message read");
        assert_eq!(received.kept, 1);
        let shipments = host
            .host()
            .rows("SELECT \"order\" FROM \"shipment\"")
            .await;
        assert_eq!(shipments, vec![vec![SqlValue::Text("A-104".into())]]);
        let left = host
            .host()
            .rows("SELECT \"reason\", \"status\" FROM \"jadpo_parked_messages\" ORDER BY \"reason\"")
            .await;
        assert_eq!(
            left,
            vec![
                vec![
                    SqlValue::Text("unlisted_type".into()),
                    SqlValue::Text("dismissed".into())
                ],
                vec![
                    SqlValue::Text("unreadable".into()),
                    SqlValue::Text("failed".into())
                ],
            ]
        );
        // The message that isn't JSON, retried, is parked again.
        let actions = host
            .host()
            .rows("SELECT \"action\", \"kind\", \"target\", \"operator\" FROM \"jadpo_operator_actions\" ORDER BY \"acted_at\"")
            .await;
        assert_eq!(
            actions,
            vec![
                vec![
                    SqlValue::Text("dismiss".into()),
                    SqlValue::Text("message".into()),
                    SqlValue::Text("warehouse_queue".into()),
                    SqlValue::Text("sam".into()),
                ],
                vec![
                    SqlValue::Text("retry".into()),
                    SqlValue::Text("message".into()),
                    SqlValue::Text("warehouse_queue".into()),
                    SqlValue::Text("sam".into()),
                ],
            ]
        );
    });
    let not_json = id_of("\"reason\" = 'unreadable'");
    let retried = jadpo(
        &loaded.root,
        &["deliveries", "retry", &not_json, "--url", url],
    );
    assert_eq!(retried.code, 0, "{}{}", retried.stdout, retried.stderr);
    runtime.block_on(async {
        host.host().set_now(T0 + 30_000);
        let ticked = tick(&loaded.program, &host, T0 + 30_000).await;
        assert_eq!(ticked.received.map(|tally| tally.parked), Some(1));
        let status = host
            .host()
            .rows(
                "SELECT \"status\" FROM \"jadpo_parked_messages\" WHERE \"reason\" = 'unreadable'",
            )
            .await;
        assert_eq!(status, vec![vec![SqlValue::Text("failed".into())]]);
    });
}

/// A queue that refuses the request, here signed with another secret,
/// keeps nothing and leaves every message on the queue; the problem names
/// the queue and never the credential.
#[test]
fn a_queue_that_refuses_the_request_keeps_nothing() {
    let sqs = FakeSqs::start("warehouse", KEY_ID, SECRET);
    let Some(loaded) = load("refused", &sqs, "not-the-secret") else {
        return;
    };
    sqs.send("warehouse", &shipped("w-5", "A-105"));
    let host = host(&loaded.url);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let ticked = tick(&loaded.program, &host, T0).await;
        assert_eq!(ticked.received, None);
        assert_eq!(ticked.problems.len(), 1, "{:?}", ticked.problems);
        let (source, error) = &ticked.problems[0];
        assert_eq!(source, "queue warehouse_queue");
        let shown = format!("{error:?}");
        assert!(shown.contains("misconfigured"), "{shown}");
        assert!(
            !shown.contains("not-the-secret") && !shown.contains(KEY_ID),
            "{shown}"
        );
        assert_eq!(count(&host, "jadpo_outbox").await, 0);
    });
    assert_eq!(sqs.left("warehouse").len(), 1);
    assert!(sqs.seen().iter().all(|request| !request.signed));
}

/// A message whose event the store refuses whatever the moment, here a
/// text holding `\u0000`, which PostgreSQL's `jsonb` can't hold, is parked
/// `unreadable` and deleted from the queue, not left for the queue to send
/// again until it expires; the next tick has nothing to receive (Phase 7
/// review, chunk 7.9c).
#[test]
fn a_message_the_store_always_refuses_is_parked() {
    let sqs = FakeSqs::start("warehouse", KEY_ID, SECRET);
    let Some(loaded) = load("refused-by-store", &sqs, SECRET) else {
        return;
    };
    sqs.send(
        "warehouse",
        r#"{"type":"order.shipped","message_id":"w-6","order":"A\u0000"}"#,
    );
    let host = host(&loaded.url);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let ticked = tick(&loaded.program, &host, T0).await;
        assert!(ticked.problems.is_empty(), "{:?}", ticked.problems);
        let received = ticked.received.expect("messages received");
        assert_eq!(
            (received.kept, received.parked, received.left),
            (0, 1, 0)
        );
        assert!(sqs.left("warehouse").is_empty());
        assert_eq!(count(&host, "jadpo_outbox").await, 0);
        let parked = host
            .host()
            .rows("SELECT \"reason\", \"message_id\", \"event\" IS NOT NULL FROM \"jadpo_parked_messages\"")
            .await;
        assert_eq!(
            parked,
            vec![vec![
                SqlValue::Text("unreadable".into()),
                SqlValue::Text("w-6".into()),
                SqlValue::Bool(true),
            ]]
        );
        host.host().set_now(T0 + 10_000);
        let again = tick(&loaded.program, &host, T0 + 10_000).await;
        assert_eq!(again.received, None);
    });
}
