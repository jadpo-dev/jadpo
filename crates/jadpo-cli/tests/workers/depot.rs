//! The Workers cron (chunk 8.2f) is proved on a depot of its own that
//! serves nothing, receives a queue from a local SQS
//! (`support::fake_sqs`) through the SQS package and takes stock every
//! minute: its Wrangler file names the minute's cron, and workerd runs its
//! `scheduled` handler as the cron trigger does, through a service
//! binding's `scheduled()`, ticking once its database is current, again at
//! each 10-second mark of its minute while it receives a queue, and
//! running the schedule once due.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use jadpo_cli::run;
use jadpo_term::Environment;
use serde_json::Value;

use crate::deployable::{apply, count, migrations, rows};
use crate::support::fake_sqs::FakeSqs;

use super::{Database, Workerd, jadpo, linked, names, now_ms, stamped};

/// A depot built for Workers that serves nothing: it reads the
/// warehouse's queue through the SQS package, recording each shipment
/// its messages tell it of, and takes stock every minute.
const DEPOT: &[(&str, &str)] = &[
    (
        "app.jadpo",
        "use stock/events { OrderShipped }\n\napplication {\n    connections: {\n        main: {\n            driver: Driver.postgres\n            url: config.database_url\n        }\n        warehouse_queue: {\n            package: AmazonSqs.v2012_11_05\n            domains: [stock]\n            credential: {\n                key_id: config.aws_key_id\n                secret: config.aws_secret_key\n            }\n            host: config.sqs_endpoint\n            queue_url: config.warehouse_queue_url\n        }\n    }\n    receiving: {\n        warehouse_queue: {\n            id_at: \"message_id\"\n            events: [OrderShipped]\n        }\n    }\n    deployables: {\n        depot: {\n            runs: [Trigger.schedules, Trigger.subscribers]\n            target: Target.wasm\n        }\n    }\n}\n",
    ),
    (
        "config.jadpo",
        "config {\n    database_url: PostgresUrl\n    aws_key_id: Text\n    aws_secret_key: Text {\n        secret: true\n    }\n    sqs_endpoint: Url\n    warehouse_queue_url: Text\n}\n",
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
    (
        "domains/stock/actions/take_stock.jadpo",
        "/// Takes stock, which changes nothing.\naction take_stock() -> Unit {\n    assert 1 == 1\n}\n",
    ),
    ("domains/stock/schedules/stocktake.jadpo", STOCKTAKE),
];

/// The depot's schedule, every minute.
const STOCKTAKE: &str = "use stock/actions { take_stock }\n\n/// Takes stock every minute.\nschedule stocktake {\n    every: 1m\n    run: take_stock()\n}\n";

/// The queue's credential.
const KEY_ID: &str = "AKIDEXAMPLE";
const SECRET: &str = "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY";

/// The depot, written where its build is kept between runs, a file left
/// as it was keeping its time, with the SQS package and its migration.
fn depot() -> PathBuf {
    let target = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let root = target
        .parent()
        .unwrap_or(target)
        .join("jadpo-compiled/workers/depot");
    for (path, text) in DEPOT {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("a folder")).expect("folders");
        if fs::read_to_string(&path).ok().as_deref() != Some(*text) {
            fs::write(&path, text).expect("written");
        }
    }
    if !root.join("jadpo.lock").exists() {
        let package = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/amazon_sqs");
        jadpo(
            &root,
            &["package", "add", package.to_str().expect("a path")],
        );
    }
    if !root.join("database/migrations").exists() {
        jadpo(&root, &["migrate", "new", "initial"]);
    }
    jadpo(&root, &["records", "update"]);
    root
}

/// The Workers cron (chunk 8.2f): a deployable that runs on the tick is
/// given the minute's cron trigger in its Wrangler file and a `scheduled`
/// handler that ticks. Under workerd, as Cloudflare's cron runs it: out of
/// date, the cron logs why and ticks nothing; up to date, it receives the
/// queue and delivers its event, and, since the depot receives a queue,
/// ticks again at each 10-second mark of its minute, so a message sent
/// between two marks is received in the same invocation; a schedule due
/// runs on it. Each tick is logged as `runtime.ticked`, as a native
/// server logs its ticks, a store it can't work in among it (chunk 8.4c).
/// A schedule more often than once a minute is refused for Workers by
/// `jadpo check`, naming it.
#[test]
fn a_workers_cron_ticks_runs_schedules_and_receives_a_queue_each_10_seconds() {
    let root = depot();

    // More often than once a minute, the depot's schedule can't run on
    // Workers' cron.
    fs::write(
        root.join("domains/stock/schedules/stocktake.jadpo"),
        STOCKTAKE.replace("every: 1m", "every: 30s"),
    )
    .expect("written");
    let args = ["check".to_string()];
    let refused = run(
        &args,
        &Environment::empty()
            .with_var("NO_COLOR", "1")
            .with_cwd(&root),
    );
    fs::write(
        root.join("domains/stock/schedules/stocktake.jadpo"),
        STOCKTAKE,
    )
    .expect("written");
    assert_eq!(refused.code, 1, "{}{}", refused.stdout, refused.stderr);
    let screen = format!("{}{}", refused.stdout, refused.stderr);
    for shown in [
        "`depot` can't run `stocktake` every 30 seconds on Workers, whose cron runs once a minute",
        "domains/stock/schedules/stocktake.jadpo:5:12",
        "rule: deployable.target_unsupported",
    ] {
        assert!(screen.contains(shown), "{shown} isn't in:\n{screen}");
    }

    let output = jadpo(&root, &["build", "--json"]);
    let document: Value = serde_json::from_str(&output.stdout).expect("JSON");
    let built = &document["data"];
    let depot = &built["deployables"][0];
    assert_eq!(depot["deployable"], "depot", "{built}");
    assert_eq!(
        depot["runs"],
        serde_json::json!(["schedules", "subscribers"])
    );
    let out = root.join("build/target/depot");
    let wrangler: Value = serde_json::from_str(
        &fs::read_to_string(out.join("wrangler.json")).expect("the Wrangler file"),
    )
    .expect("JSON");
    // The minute's cron, and the settings it needs.
    assert_eq!(
        wrangler["triggers"],
        serde_json::json!({ "crons": ["* * * * *"] }),
        "{wrangler}"
    );
    assert_eq!(
        wrangler["secrets"]["required"],
        serde_json::json!([
            "AWS_KEY_ID",
            "AWS_SECRET_KEY",
            "SQS_ENDPOINT",
            "WAREHOUSE_QUEUE_URL"
        ]),
        "{wrangler}"
    );
    // Its database's address is a Hyperdrive binding (chunk 8.2i), which
    // the harness gives as workerd's.
    assert_eq!(
        wrangler["hyperdrive"],
        serde_json::json!([{ "binding": "DATABASE_URL" }]),
        "{wrangler}"
    );
    let shim = fs::read_to_string(out.join("worker.mjs")).expect("the shim");
    assert!(shim.contains("scheduled(controller, env, ctx)"), "{shim}");
    assert!(
        names(
            &linked(&root, "depot"),
            "jadpo_host_workers",
            "deployable::scheduled"
        ),
        "it lacks the scheduled handler"
    );

    if !jadpo_test::live() {
        return;
    }
    let database = Database::on_tcp();
    let sqs = FakeSqs::start("warehouse", KEY_ID, SECRET);
    let queue = sqs.queue_url("warehouse");
    let worker = Workerd::start(
        &out,
        &wrangler,
        &[
            ("DATABASE_URL", &database.worker),
            ("AWS_KEY_ID", KEY_ID),
            ("AWS_SECRET_KEY", SECRET),
            ("SQS_ENDPOINT", &sqs.url),
            ("WAREHOUSE_QUEUE_URL", &queue),
        ],
    );
    sqs.send("warehouse", &shipped("w-1", "A-100"));

    // Its database hasn't applied its migration: the cron logs it and
    // ticks nothing, leaving the message on the queue.
    assert_eq!(worker.scheduled(now_ms()), "ok", "{}", worker.log());
    let failed = worker.events("runtime.startup_failed");
    assert_eq!(failed.len(), 1, "{}", worker.log());
    stamped(&failed[0], built, depot);
    assert_eq!(failed[0]["problems"][0]["problem"], "migrations_pending");
    assert_eq!(sqs.left("warehouse").len(), 1, "{}", worker.log());
    assert!(sqs.seen().is_empty(), "{:?}", sqs.seen());

    // Up to date, the same isolate ticks: a cron 40 seconds into its
    // minute ticks at once and again at its 50-second mark, as the depot
    // receives a queue, then ends with the minute. The first tick keeps
    // the message as its event and delivers it; one sent between the two
    // is received by the second.
    apply(&database.native, &migrations(&root));
    let minute = now_ms() - 40_000;
    let cron = std::thread::scope(|scope| {
        let cron = scope.spawn(|| worker.scheduled(minute));
        let until = Instant::now() + Duration::from_secs(60);
        while rows(&database.native, "SELECT \"order\" FROM shipment").is_empty() {
            assert!(Instant::now() < until, "no shipment:\n{}", worker.log());
            std::thread::sleep(Duration::from_millis(100));
        }
        sqs.send("warehouse", &shipped("w-2", "A-200"));
        cron.join().expect("the cron ran")
    });
    assert_eq!(cron, "ok", "{}", worker.log());
    let ended = now_ms();
    assert!(
        ended >= minute + 49_900 && ended < minute + 120_000,
        "the cron ended {} ms into its minute",
        ended - minute
    );
    assert_eq!(
        rows(
            &database.native,
            "SELECT \"order\" FROM shipment ORDER BY \"order\""
        ),
        vec![vec!["A-100".to_string()], vec!["A-200".to_string()]],
        "{}",
        worker.log()
    );
    assert!(
        sqs.left("warehouse").is_empty(),
        "{:?}",
        sqs.left("warehouse")
    );
    let receives = sqs
        .seen()
        .iter()
        .filter(|seen| seen.target == "AmazonSQS.ReceiveMessage")
        .count();
    assert!(receives >= 2, "{:?}", sqs.seen());
    // The first tick saw the schedule, its first run due a minute on, and
    // the last tick that reached it was the 50-second mark's.
    let state = rows(
        &database.native,
        "SELECT (extract(epoch from \"last_tick_at\") * 1000)::int8, (extract(epoch from \"next_run_at\") * 1000)::int8 FROM jadpo_schedules",
    );
    let [state] = state.as_slice() else {
        panic!("one schedule's state: {state:?}")
    };
    let last_tick: i64 = state[0].parse().expect("a time");
    let next_run: i64 = state[1].parse().expect("a time");
    // A timer may fire a millisecond or so early by the clock.
    assert!(
        last_tick >= minute + 49_900,
        "the last tick was {} ms into the minute",
        last_tick - minute
    );
    assert!(
        next_run > minute + 60_000 && next_run <= minute + 120_000,
        "the first run is due {} ms into the minute",
        next_run - minute
    );
    assert_eq!(count(&database.native, "jadpo_schedule_runs"), 0);

    // Once the schedule is due, the next cron runs it, and a cron at the
    // end of its minute ticks once.
    rows(
        &database.native,
        "UPDATE jadpo_schedules SET next_run_at = now() - interval '1 second' RETURNING 1",
    );
    let before = sqs.seen().len();
    assert_eq!(
        worker.scheduled(now_ms() - 59_000),
        "ok",
        "{}",
        worker.log()
    );
    assert_eq!(
        rows(&database.native, "SELECT outcome FROM jadpo_schedule_runs"),
        vec![vec!["succeeded".to_string()]],
        "{}",
        worker.log()
    );
    let receives = sqs.seen()[before..]
        .iter()
        .filter(|seen| seen.target == "AmazonSQS.ReceiveMessage")
        .count();
    assert_eq!(receives, 1, "{:?}", sqs.seen());

    // Each tick was logged: the 40-second cron's two and the last's,
    // which ran the schedule, by its name; none couldn't work in a store.
    let ticks = worker.events("runtime.ticked");
    assert_eq!(ticks.len(), 3, "{}", worker.log());
    for tick in &ticks {
        assert_eq!(tick["problems"], serde_json::json!([]), "{tick}");
        assert_eq!(tick["unbuilt"], serde_json::json!([]), "{tick}");
    }
    assert_eq!(ticks[2]["ran"][0]["schedule"], "stocktake", "{}", ticks[2]);
    assert_eq!(ticks[2]["ran"][0]["outcome"], "succeeded", "{}", ticks[2]);
    // A store the tick can't work in is logged, as a native server's tick
    // logs it: here the table of idempotency keys every tick sweeps, gone
    // for a moment.
    rows(
        &database.native,
        "UPDATE jadpo_housekeeping SET last_cleaned_at = NULL",
    );
    rows(
        &database.native,
        "ALTER TABLE jadpo_idempotency_keys RENAME TO jadpo_idempotency_keys_away",
    );
    let ended = worker.scheduled(now_ms() - 59_000);
    rows(
        &database.native,
        "ALTER TABLE jadpo_idempotency_keys_away RENAME TO jadpo_idempotency_keys",
    );
    assert_eq!(ended, "ok", "{}", worker.log());
    let ticks = worker.events("runtime.ticked");
    let [.., last] = ticks.as_slice() else {
        panic!("{}", worker.log())
    };
    assert_eq!(last["problems"][0]["problem"], "store_failed", "{last}");
    assert_eq!(last["problems"][0]["store"], "main", "{last}");

    let started = worker.events("runtime.started");
    assert_eq!(started.len(), 1, "{}", worker.log());
    for event in worker.events("") {
        stamped(&event, built, depot);
    }
    assert!(
        !worker.log().contains(SECRET) && !worker.log().contains(&database.worker),
        "the log holds a secret:\n{}",
        worker.log()
    );
}

/// A message telling of order `order` shipped, by id `id`.
fn shipped(id: &str, order: &str) -> String {
    format!(r#"{{"message_id":"{id}","order":"{order}"}}"#)
}
