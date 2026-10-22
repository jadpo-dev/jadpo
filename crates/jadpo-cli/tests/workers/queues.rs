//! Cloudflare Queues' consumer on Workers (roadmap chunk 8.2k; area 11,
//! "Chosen in chunk 7.7c2"; area 14, "A Workers deployable"): a warehouse
//! built for Workers that receives a queue on Cloudflare Queues
//! (`Driver.cloudflare_queues`) and records each shipment its messages tell
//! it of. Its Wrangler file names the queue's consumer by Wrangler's own
//! setting names, and Wrangler's `deploy --dry-run`, pinned in
//! `tools/workers/`, accepts the file without a warning or a single network
//! call (`tools/workers/offline.mjs` refuses and reports any). Under
//! workerd, its `queue` handler is handed batches as Cloudflare Queues
//! hands them, through a service binding's `queue()`: out of date it
//! retries each message after the delay its attempts give; current, it
//! keeps each message once as its event and acknowledges it, a repeat and
//! one it parks too, and delivers the kept events after answering; a
//! batch from a queue it doesn't receive is retried whole; and a message
//! handed back from the queue's dead-letter queue, its retries spent, is
//! parked for an operator (chunk 8.4b), the dead-letter queue a consumer
//! of the Worker's too.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use jadpo_term::Environment;
use serde_json::{Value, json};

use super::{Database, Workerd, apply, dechunk, jadpo, jadpo_in, migrations, names, rows};

/// The warehouse, built for Workers: it receives the warehouse's queue on
/// Cloudflare Queues and records each shipment it tells of.
const WAREHOUSE: &[(&str, &str)] = &[
    (
        "app.jadpo",
        "use stock/events { OrderShipped }\n\napplication {\n    connections: {\n        main: {\n            driver: Driver.postgres\n            url: config.database_url\n        }\n        warehouse_queue: {\n            driver: Driver.cloudflare_queues\n            queue: \"warehouse-events\"\n            dead_letter_queue: \"warehouse-events-dead\"\n        }\n    }\n    receiving: {\n        warehouse_queue: {\n            type_at: \"type\"\n            id_at: \"message_id\"\n            events: [OrderShipped]\n        }\n    }\n    deployables: {\n        warehouse: {\n            runs: [Trigger.subscribers]\n            target: Target.wasm\n        }\n    }\n}\n",
    ),
    (
        "config.jadpo",
        "config {\n    database_url: PostgresUrl\n}\n",
    ),
    (
        "domains/stock/events/order_shipped.jadpo",
        "/// An order left the warehouse.\ntype OrderShipped = Event {\n    wire_name: \"order.shipped\"\n    order: Text {\n        max_length: 12\n    }\n}\n",
    ),
    (
        "domains/stock/entities/shipment.jadpo",
        "entity Shipment {\n    id: Uuid\n    order: Text {\n        max_length: 12\n    }\n\n    policy: {\n        create: [Access.system]\n        read: [Access.system]\n    }\n}\n",
    ),
    (
        "domains/stock/actions/record_shipment.jadpo",
        "use stock/entities { Shipment }\nuse stock/events { OrderShipped }\n\n/// Records an order the warehouse shipped.\naction record_shipment(event: OrderShipped) -> Shipment {\n    return Shipment.create {\n        set: {\n            order: event.order\n        }\n    }\n}\n",
    ),
    (
        "domains/stock/subscribers/shipped.jadpo",
        "use stock/actions { record_shipment }\nuse stock/events { OrderShipped }\n\n/// Records each order the warehouse ships.\nsubscriber shipped {\n    on: OrderShipped\n    run: record_shipment(event)\n}\n",
    ),
];

/// The Hyperdrive configuration's id the build is given, as Wrangler
/// requires one.
const HYPERDRIVE_ID: &str = "c020574a5623407bbe0ccd192bab9545";

/// The warehouse, written where its build is kept between runs, with its
/// migration.
fn warehouse() -> PathBuf {
    let target = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let root = target
        .parent()
        .unwrap_or(target)
        .join("jadpo-compiled/workers/warehouse");
    for (path, text) in WAREHOUSE {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("a folder")).expect("folders");
        if fs::read_to_string(&path).ok().as_deref() != Some(*text) {
            fs::write(&path, text).expect("written");
        }
    }
    if !root.join("database/migrations").exists() {
        jadpo(&root, &["migrate", "new", "initial"]);
    }
    jadpo(&root, &["records", "update"]);
    root
}

/// A shipment's message by id `id`.
fn shipped(id: &str, order: &str) -> String {
    format!(r#"{{"type":"order.shipped","message_id":"{id}","order":"{order}"}}"#)
}

#[test]
fn a_worker_consumes_a_cloudflare_queue_answering_each_message() {
    let root = warehouse();
    let output = jadpo_in(
        &root,
        &["build", "--json"],
        Environment::empty().with_var("JADPO_HYPERDRIVE_DATABASE_URL", HYPERDRIVE_ID),
    );
    let document: Value = serde_json::from_str(&output.stdout).expect("JSON");
    let warehouse = &document["data"]["deployables"][0];
    assert_eq!(warehouse["deployable"], "warehouse", "{document}");
    let out = root.join("build/target/warehouse");
    let wrangler: Value = serde_json::from_str(
        &fs::read_to_string(out.join("wrangler.json")).expect("the Wrangler file"),
    )
    .expect("JSON");
    // The queue's consumer, by Wrangler's setting names, retried for
    // about 22 hours, and its dead-letter queue's (chunk 8.4b).
    assert_eq!(
        wrangler["queues"],
        json!({
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
        }),
        "{wrangler}"
    );
    let shim = fs::read_to_string(out.join("worker.mjs")).expect("the shim");
    assert!(shim.contains("queue(batch, env, ctx)"), "{shim}");
    let module = fs::read(out.join("warehouse.wasm")).expect("the module");
    assert!(
        names(&module, "jadpo_host_workers", "deployable::queue"),
        "it lacks the queue handler"
    );

    // Wrangler accepts the file as it is, with no account and no network.
    let dry = dry_run(&out);
    assert!(dry.contains("--dry-run: exiting now."), "{dry}");
    for refused in ["WARNING", "ERROR", "jadpo-offline:"] {
        assert!(!dry.contains(refused), "{refused} in:\n{dry}");
    }

    if !jadpo_test::live() {
        return;
    }
    let database = Database::on_tcp();
    let worker = Workerd::start(&out, &wrangler, &[("DATABASE_URL", &database.worker)]);

    // Its database hasn't applied its migration: each message is retried
    // after the delay its attempts give, nothing kept.
    let answered = worker.queue(
        "warehouse-events",
        json!([{ "id": "m-0", "body": shipped("w-1", "A-100"), "attempts": 2 }]),
    );
    assert_eq!(answered["outcome"], "ok", "{answered}\n{}", worker.log());
    assert_eq!(answered["ackAll"], false, "{answered}");
    assert_eq!(answered["explicitAcks"], json!([]), "{answered}");
    assert_eq!(
        answered["retryMessages"],
        json!([{ "msgId": "m-0", "delaySeconds": 20 }]),
        "{answered}"
    );
    let failed = worker.events("runtime.startup_failed");
    assert_eq!(failed.len(), 1, "{}", worker.log());
    assert_eq!(failed[0]["problems"][0]["problem"], "migrations_pending");

    // Current, each message is kept once as its event and acknowledged on
    // its own: a body sent as text and one sent as JSON, a repeat, one of
    // a type the queue doesn't list and one that isn't JSON, both parked.
    apply(&database.native, &migrations(&root));
    let answered = worker.queue(
        "warehouse-events",
        json!([
            { "id": "m-1", "body": shipped("w-1", "A-100"), "attempts": 1 },
            {
                "id": "m-2",
                "body": { "type": "order.shipped", "message_id": "w-2", "order": "A-200" },
                "attempts": 1,
            },
            { "id": "m-3", "body": shipped("w-1", "A-100"), "attempts": 1 },
            {
                "id": "m-4",
                "body": { "type": "order.returned", "message_id": "w-3" },
                "attempts": 1,
            },
            { "id": "m-5", "body": "not json", "attempts": 1 },
        ]),
    );
    assert_eq!(answered["outcome"], "ok", "{answered}\n{}", worker.log());
    assert_eq!(answered["ackAll"], false, "{answered}");
    assert_eq!(
        answered["explicitAcks"],
        json!(["m-1", "m-2", "m-3", "m-4", "m-5"]),
        "{answered}\n{}",
        worker.log()
    );
    assert_eq!(answered["retryMessages"], json!([]), "{answered}");
    // The kept events' subscribers run after the answer.
    let until = Instant::now() + Duration::from_secs(60);
    let shipments = || {
        rows(
            &database.native,
            "SELECT \"order\" FROM shipment ORDER BY \"order\"",
        )
    };
    while shipments().len() < 2 {
        assert!(Instant::now() < until, "no shipments:\n{}", worker.log());
        std::thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(
        shipments(),
        vec![vec!["A-100".to_string()], vec!["A-200".to_string()]]
    );
    assert_eq!(
        rows(
            &database.native,
            "SELECT \"source\", \"event_id\" FROM \"jadpo_received_events\" ORDER BY 2",
        ),
        vec![
            vec!["queue warehouse_queue".to_string(), "w-1".to_string()],
            vec!["queue warehouse_queue".to_string(), "w-2".to_string()],
        ]
    );
    assert_eq!(
        rows(
            &database.native,
            "SELECT \"queue\", \"reason\" FROM \"jadpo_parked_messages\" ORDER BY \"reason\"",
        ),
        vec![
            vec!["warehouse_queue".to_string(), "unlisted_type".to_string()],
            vec!["warehouse_queue".to_string(), "unreadable".to_string()],
        ]
    );

    // A batch from a queue it doesn't receive is retried whole.
    let answered = worker.queue(
        "elsewhere",
        json!([{ "id": "m-6", "body": shipped("w-4", "A-400"), "attempts": 3 }]),
    );
    assert_eq!(
        answered["retryMessages"],
        json!([{ "msgId": "m-6", "delaySeconds": 40 }]),
        "{answered}"
    );
    assert_eq!(answered["explicitAcks"], json!([]), "{answered}");

    // A message whose retries are spent comes back through the
    // dead-letter queue: parked for an operator and acknowledged, no event
    // kept (chunk 8.4b).
    let answered = worker.queue(
        "warehouse-events-dead",
        json!([{ "id": "m-7", "body": shipped("w-5", "A-500"), "attempts": 1 }]),
    );
    assert_eq!(answered["outcome"], "ok", "{answered}\n{}", worker.log());
    assert_eq!(answered["explicitAcks"], json!(["m-7"]), "{answered}");
    assert_eq!(answered["retryMessages"], json!([]), "{answered}");
    assert_eq!(
        rows(
            &database.native,
            "SELECT \"queue\", \"message_id\" FROM \"jadpo_parked_messages\" WHERE \"reason\" = 'dead_lettered'",
        ),
        vec![vec!["warehouse_queue".to_string(), "w-5".to_string()]]
    );
    assert_eq!(
        rows(
            &database.native,
            "SELECT count(*) FROM \"jadpo_received_events\" WHERE \"event_id\" = 'w-5'",
        ),
        vec![vec!["0".to_string()]]
    );
    let retried: Vec<(String, String)> = worker
        .events("queue.retried")
        .iter()
        .map(|event| {
            (
                event["queue"].as_str().unwrap_or_default().to_string(),
                event["reason"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    assert_eq!(
        retried,
        vec![
            ("warehouse-events".to_string(), "not_started".to_string()),
            ("elsewhere".to_string(), "not_received".to_string()),
        ],
        "{}",
        worker.log()
    );
    assert!(
        !worker.log().contains(&database.worker),
        "the log holds the database's address:\n{}",
        worker.log()
    );
}

/// Wrangler's `deploy --dry-run` of the Worker in `out`, as pinned in
/// `tools/workers/`: what it wrote, standard output then standard error.
/// It runs with no account, its banner's update check, telemetry and
/// error reports off, a home of its own, and `offline.mjs` refusing and
/// reporting any connection it would make.
fn dry_run(out: &Path) -> String {
    let (wrangler, offline) =
        jadpo_test::workerd_install::wrangler().unwrap_or_else(|why| panic!("{why}"));
    let home = Path::new(env!("CARGO_TARGET_TMPDIR")).join("workers-wrangler");
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(&home).expect("a home");
    let mut command = Command::new("node");
    command
        .arg("--import")
        .arg(&offline)
        .arg(&wrangler)
        .args(["deploy", "--dry-run", "--config"])
        .arg(out.join("wrangler.json"))
        .arg("--outdir")
        .arg(home.join("upload"))
        .env_clear()
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("WRANGLER_LOG_PATH", home.join("logs"))
        .env("WRANGLER_HIDE_BANNER", "true")
        .env("WRANGLER_SEND_METRICS", "false")
        .env("WRANGLER_SEND_ERROR_REPORTS", "false")
        .env("NO_COLOR", "1")
        .env("CI", "1")
        .current_dir(&home);
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }
    let output = command.output().expect("node runs Wrangler");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.status.success(), "{}\n{said}", output.status);
    said
}

impl Workerd {
    /// Hands the Worker's `queue` handler `messages`, each its `id`,
    /// `body` and `attempts`, as Cloudflare Queues hands a batch of queue
    /// `queue` to its consumer: how the invocation ended and how each
    /// message was answered (`explicitAcks`, `retryMessages`).
    fn queue(&self, queue: &str, messages: Value) -> Value {
        let body = json!({ "queue": queue, "messages": messages }).to_string();
        let mut stream = TcpStream::connect(("127.0.0.1", self.cron)).expect("the cron listens");
        stream
            .set_read_timeout(Some(Duration::from_secs(120)))
            .expect("a timeout");
        write!(
            stream,
            "POST /queue HTTP/1.1\r\nHost: cron\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .expect("sent");
        let mut answer = String::new();
        let _ = stream.read_to_string(&mut answer);
        let body = answer
            .split_once("\r\n\r\n")
            .map(|(head, body)| {
                if head
                    .to_ascii_lowercase()
                    .contains("transfer-encoding: chunked")
                {
                    dechunk(body)
                } else {
                    body.to_string()
                }
            })
            .unwrap_or_else(|| panic!("an HTTP answer: {answer}\n{}", self.log()));
        serde_json::from_str(&body)
            .unwrap_or_else(|_| panic!("the queue's answer: {answer}\n{}", self.log()))
    }
}
