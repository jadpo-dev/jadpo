//! A Workers deployable, built and served by workerd (roadmap chunks 8.2b,
//! 8.2d and 8.2i; docs/compiler-runtime.md §2.6, "A Workers deployable"
//! and "PostgreSQL through Hyperdrive"; area 14, "A Workers deployable"):
//! teaching example 23, the repair café, declared as one kiosk built for
//! `Target.wasm`. `jadpo build` writes it in `build/target/kiosk/` as a
//! WebAssembly module bound by wasm-bindgen's library, the JavaScript that
//! binds it, Jadpo's module shim and a Wrangler file naming them, its
//! database's address as a Hyperdrive binding, with the id the build is
//! given or, without one, an alert saying so, and the compatibility date;
//! its stamp's target is `wasm`, the screen shows the module's size raw
//! and gzipped, and the module links the PostgreSQL driver the café's
//! database needs, the runtime's HTTP component, and none of the testing
//! machinery.
//!
//! workerd, Cloudflare's own Workers runtime, pinned in `tools/workers/`,
//! then runs the files as the Wrangler file describes them, against a
//! PostgreSQL database on this machine reached through workerd's
//! Hyperdrive binding, which hands `connect()` to the database's server.
//! Without its database's address the Worker answers 503 and logs
//! `runtime.startup_failed` naming the setting's variable, once for its
//! isolate; against a database that hasn't applied its last migration it
//! answers 503 and logs the migration, and checks again at the next
//! request, so once the migration is applied the same isolate starts
//! serving: it books a repair through `POST /repairs`, the subscribers
//! that find it a bench and print its ticket delivered while the request
//! waits, and answers with the ticket read back from the database; a thank
//! you's note is pinned up after its answer, in `waitUntil`; its readiness
//! check reaches the database; and each address `CF-Connecting-IP` gives
//! has its own allowance of 120 requests a minute. Every event it logs is
//! one JSON line carrying the build stamp.
//!
//! The Workers cron (chunk 8.2f) is proved on a depot of its own that
//! serves nothing, receives a queue from a local SQS
//! (`support::fake_sqs`) through the SQS package and takes stock every
//! minute: its Wrangler file names the minute's cron, and workerd runs its
//! `scheduled` handler as the cron trigger does, through a service
//! binding's `scheduled()`, ticking once its database is current, again at
//! each 10-second mark of its minute while it receives a queue, and
//! running the schedule once due.
//!
//! After the response (chunk 8.2h) is proved on a news desk of its own,
//! whose model is a server this test starts speaking OpenAI's Chat
//! Completions API: a draft streamed from the model reaches the client
//! piece by piece, its first piece before the model has written the rest,
//! and a client that goes away closes the model's connection; stories
//! filed together are each summarised by a subscriber after the answer,
//! in `waitUntil`, and a walk through the archive's clippings is a batch
//! run, both taking longer than an invocation's 30 seconds, so each hands
//! what it didn't reach on to a fresh invocation of the Worker, a request
//! to itself through the loopback `ctx.exports` gives its `JadpoHandOn`
//! entrypoint, which carries it on to its end. Since chunk 8.4a the desk
//! answers its limits on the module's own stack and V8's: a thousand
//! frames of a plain recursion and of a heavy one, four hundred inline
//! functions running at once, and a level more of each the
//! `recursion_limit` fault, the isolate serving on.
//!
//! The build is kept beside the build's own folders
//! (`target/jadpo-compiled/workers/`), so a second run compiles only what
//! changed. The database is the run's when `JADPO_TEST_DATABASE_URL` names
//! one by a URL, as CI's does (`127.0.0.1`), and otherwise one this test
//! starts on `127.0.0.1` from the machine's PostgreSQL tools, since the
//! one a local run of the gate starts listens on a Unix socket, which a
//! Worker can't open; `JADPO_SKIP_POSTGRES=1` skips the serving part.
//! Nothing leaves this machine: workerd is installed into `tools/workers/`
//! by `npm ci --ignore-scripts` the first time (`JADPO_WORKERD` names
//! another), and the Worker reaches nothing but this machine.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use jadpo_cli::run;
use jadpo_term::Environment;
use jadpo_test::postgres::Server;
use serde_json::Value;

use crate::deployable::{apply, copy_folder, count, migrations, rows};
use crate::support::fake_sqs::FakeSqs;
use crate::support::probes;

/// Cloudflare Queues' consumer (chunk 8.2k), on the harness below.
mod queues;

/// The café declares one kiosk, built for Workers, serving its API and
/// running its subscribers and schedules, its rate limits counted in
/// Cloudflare's rate-limiting binding (chunk 8.2m2).
const DECLARED: &str = "application {\n    connections: {\n        limits: {\n            driver: Driver.cloudflare_rate_limits\n            namespace: 1001\n        }\n        main: {\n            driver: Driver.postgres\n            url: config.database_url\n        }\n    }\n    events: {\n        store: main\n    }\n    rate_limits: {\n        store: limits\n    }\n    deployables: {\n        kiosk: {\n            serves: [main]\n            runs: [Trigger.subscribers, Trigger.schedules]\n            target: Target.wasm\n        }\n    }\n}\n";

/// Teaching example 23 declared as a kiosk on Workers, copied where its
/// build is kept between runs.
fn repair_cafe() -> PathBuf {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let target = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let root = target
        .parent()
        .unwrap_or(target)
        .join("jadpo-compiled/workers/23-repair-cafe");
    copy_folder(&examples.join("23-repair-cafe"), &root);
    if fs::read_to_string(root.join("app.jadpo")).ok().as_deref() != Some(DECLARED) {
        fs::write(root.join("app.jadpo"), DECLARED).expect("written");
    }
    root
}

/// `jadpo` with `args` in `root`, which must succeed.
fn jadpo(root: &Path, args: &[&str]) -> jadpo_cli::Output {
    jadpo_in(root, args, Environment::empty())
}

/// `jadpo` with `args` in `root` and `environment`, which must succeed.
fn jadpo_in(root: &Path, args: &[&str], environment: Environment) -> jadpo_cli::Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let output = run(&args, &environment.with_cwd(root));
    assert_eq!(
        output.code, 0,
        "{args:?}: {}{}",
        output.stdout, output.stderr
    );
    output
}

/// The module cargo built for `deployable` of the project at `root`, in
/// the release profile it ships in, before it is bound: its names still
/// in it, which the module the Worker is deployed from leaves out (chunk
/// 8.4f).
fn linked(root: &Path, deployable: &str) -> Vec<u8> {
    let module = root.join(format!(
        "build/rust/target/wasm32-unknown-unknown/release/jadpo_worker_{deployable}.wasm"
    ));
    fs::read(&module).unwrap_or_else(|error| panic!("{}: {error}", module.display()))
}

/// The names of the custom sections of WebAssembly module `module`.
fn custom_sections(module: &[u8]) -> Vec<String> {
    fn leb(bytes: &[u8], at: &mut usize) -> usize {
        let (mut value, mut shift) = (0usize, 0);
        loop {
            let byte = bytes[*at];
            *at += 1;
            value |= usize::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return value;
            }
            shift += 7;
        }
    }
    let mut sections = Vec::new();
    let mut at = 8;
    while at < module.len() {
        let id = module[at];
        at += 1;
        let size = leb(module, &mut at);
        let end = at + size;
        if id == 0 {
            let length = leb(module, &mut at);
            sections.push(String::from_utf8_lossy(&module[at..at + length]).into_owned());
        }
        at = end;
    }
    sections
}

/// Whether the module's names hold a path in crate `krate` starting with
/// `path`, as rustc writes them: `jadpo_runtime[9f2c…]::fakes` or
/// `_ZN13jadpo_runtime5fakes…`.
fn names(module: &[u8], krate: &str, path: &str) -> bool {
    let mangled: String = std::iter::once(krate)
        .chain(path.trim_end_matches(':').split("::"))
        .map(|name| format!("{}{name}", name.len()))
        .collect();
    if module
        .windows(mangled.len())
        .any(|window| window == mangled.as_bytes())
    {
        return true;
    }
    let text = String::from_utf8_lossy(module);
    let pattern = regex::Regex::new(&format!(
        r"{}\[[0-9a-f]+\]::{}",
        regex::escape(krate),
        regex::escape(path)
    ))
    .expect("a pattern");
    pattern.is_match(&text)
}

#[test]
fn a_teaching_example_is_built_for_workers_and_served_by_workerd() {
    let root = repair_cafe();
    let output = jadpo(&root, &["build", "--json"]);
    let document: Value = serde_json::from_str(&output.stdout).expect("JSON");
    let built = &document["data"];
    let kiosk = &built["deployables"][0];
    assert_eq!(
        built["deployables"].as_array().map(Vec::len),
        Some(1),
        "{built}"
    );
    assert_eq!(kiosk["deployable"], "kiosk", "{built}");
    assert_eq!(kiosk["target"], "wasm", "{built}");
    assert_eq!(kiosk["binary"], Value::Null, "{built}");
    assert_eq!(kiosk["wrangler"], "build/target/kiosk/wrangler.json");
    assert_eq!(kiosk["module"]["path"], "build/target/kiosk/kiosk.wasm");
    assert_eq!(
        kiosk["openapi"],
        serde_json::json!(["build/target/kiosk/openapi/main.json"])
    );
    let id = kiosk["build"].as_str().expect("a build id");
    assert!(
        id.len() == 14 && id.starts_with("b_") && id[2..].chars().all(|c| c.is_ascii_hexdigit()),
        "{id}"
    );
    let manifest: Value =
        serde_json::from_str(&fs::read_to_string(root.join("build/manifest.json")).expect("read"))
            .expect("JSON");
    assert_eq!(manifest["deployables"]["kiosk"]["build"], id, "{manifest}");
    assert_eq!(manifest["deployables"]["kiosk"]["target"], "wasm");

    // The module, its size as the build gives it, raw and gzipped.
    let out = root.join("build/target/kiosk");
    let module = fs::read(out.join("kiosk.wasm")).expect("the module");
    assert_eq!(kiosk["module"]["bytes"], module.len(), "{built}");
    let gzipped = kiosk["module"]["gzip_bytes"].as_u64().expect("gzipped");
    assert!(
        gzipped > 0 && gzipped < module.len() as u64,
        "{gzipped} of {}",
        module.len()
    );
    assert!(module.starts_with(b"\0asm"), "a WebAssembly module");
    // It ships without its names or the tools that made it (chunk 8.4f),
    // which cargo's module still holds.
    let sections = custom_sections(&module);
    assert!(
        !sections
            .iter()
            .any(|section| section == "name" || section == "producers"),
        "{sections:?}"
    );
    let linked = linked(&root, "kiosk");
    assert!(
        custom_sections(&linked)
            .iter()
            .any(|section| section == "name"),
        "cargo's module has its names"
    );
    println!(
        "measured: the kiosk's module {} bytes, {gzipped} gzipped",
        module.len()
    );

    // The Wrangler file: the shim as the main module, uploaded with the
    // files beside it as they are, the compatibility date, and the
    // database's address as a Hyperdrive binding, by its setting's
    // variable, with no id, since the build was given none.
    let wrangler: Value = serde_json::from_str(
        &fs::read_to_string(out.join("wrangler.json")).expect("the Wrangler file"),
    )
    .expect("JSON");
    // Named after the app, its folder's, and the kiosk (chunk 8.4f).
    assert_eq!(wrangler["name"], "23-repair-cafe-kiosk", "{wrangler}");
    // Workers Logs keeps the lines it logs (chunk 8.4f).
    assert_eq!(
        wrangler["observability"],
        serde_json::json!({ "enabled": true }),
        "{wrangler}"
    );
    assert_eq!(wrangler["main"], "worker.mjs", "{wrangler}");
    assert_eq!(wrangler["no_bundle"], true, "{wrangler}");
    assert_eq!(
        wrangler["compatibility_date"],
        jadpo_codegen::workers::COMPATIBILITY_DATE
    );
    assert!(wrangler.get("secrets").is_none(), "{wrangler}");
    assert_eq!(
        wrangler["hyperdrive"],
        serde_json::json!([{ "binding": "DATABASE_URL" }]),
        "{wrangler}"
    );
    assert_eq!(
        kiosk["hyperdrive"],
        serde_json::json!([{ "binding": "DATABASE_URL", "id": null }]),
        "{built}"
    );
    // A rate-limiting binding for each limit the kiosk counts, numbered
    // from `limits`' namespace.
    assert_eq!(
        wrangler["ratelimits"],
        serde_json::json!([
            { "name": "LIMITS_120_PER_60S", "namespace_id": "1001", "simple": { "limit": 120, "period": 60 } },
            { "name": "LIMITS_600_PER_60S", "namespace_id": "1002", "simple": { "limit": 600, "period": 60 } },
        ]),
        "{wrangler}"
    );
    // Its tick has a database to sweep, so the minute's cron runs it.
    assert_eq!(
        wrangler["triggers"]["crons"],
        serde_json::json!(["* * * * *"]),
        "{wrangler}"
    );
    let shim = fs::read_to_string(out.join("worker.mjs")).expect("the shim");
    assert!(shim.contains("from \"./kiosk.js\""), "{shim}");
    let js = fs::read_to_string(out.join("kiosk.js")).expect("the module's JavaScript");
    // The module's `fetch`, renamed where it would hide the global one the
    // module itself calls.
    assert!(
        js.contains("export function fetch(")
            || regex::Regex::new(r"export \{ \w+ as fetch \}")
                .expect("a pattern")
                .is_match(&js),
        "{js}"
    );
    assert!(js.contains("initSync"), "{js}");

    // It links the PostgreSQL driver, which the café's database needs, and
    // no SMTP driver, since it sends no email; the runtime's HTTP component
    // and the runner that serve a request; none of the fakes, recordings,
    // crash points, records or a test's steps.
    let crate_manifest =
        fs::read_to_string(root.join("build/rust/kiosk/deployable/Cargo.toml")).expect("read");
    assert!(
        crate_manifest.contains("features = [\"postgres\"]"),
        "{crate_manifest}"
    );
    for (krate, path) in [
        ("jadpo_runtime", "fakes"),
        ("jadpo_runtime", "ai::fake"),
        ("jadpo_runtime", "capability::Faked"),
        ("jadpo_runtime", "recorded"),
        ("jadpo_runtime", "prompt_recordings"),
        ("jadpo_runtime", "crashes"),
        ("jadpo_runtime", "record::"),
        ("jadpo_runtime", "test_steps"),
    ] {
        assert!(!names(&linked, krate, path), "it holds {krate}::{path}");
    }
    for (krate, path) in [
        ("jadpo_runtime", "store"),
        ("jadpo_runtime", "http::trie::Routes"),
        ("jadpo_runtime", "serve::Serving"),
        ("jadpo_runtime", "runner"),
        ("jadpo_drivers", "postgres"),
        ("jadpo_program_kiosk", "cafe"),
        ("jadpo_host_workers", "deployable::fetch"),
    ] {
        assert!(names(&linked, krate, path), "it lacks {krate}::{path}");
    }

    // The screen names the Worker's files, the module's size and its
    // database's Hyperdrive binding, and says the binding has no id, which
    // `wrangler deploy` needs, nor its Hyperdrive's caching off.
    let screen = jadpo(&root, &["build", "kiosk"]).stdout;
    for shown in [
        "worker: build/target/kiosk/wrangler.json",
        "module: build/target/kiosk/kiosk.wasm",
        "MB gzipped",
        "database: DATABASE_URL through Hyperdrive, no id",
        "build: b_",
        "settings: DATABASE_URL",
        "`DATABASE_URL` has no Hyperdrive id",
        "caching off",
        "`JADPO_HYPERDRIVE_DATABASE_URL`",
        "built: kiosk, wasm in ",
    ] {
        assert!(screen.contains(shown), "{shown} isn't in:\n{screen}");
    }
    assert!(!screen.contains("503"), "{screen}");

    // Given its Hyperdrive's id, the build writes it into the Wrangler
    // file, and the screen names it.
    const ID: &str = "c020574a5623407bbe0ccd192bab9545";
    let given = jadpo_in(
        &root,
        &["build", "kiosk"],
        Environment::empty().with_var("JADPO_HYPERDRIVE_DATABASE_URL", ID),
    )
    .stdout;
    assert!(
        given.contains(&format!("database: DATABASE_URL through Hyperdrive {ID}")),
        "{given}"
    );
    assert!(!given.contains("has no Hyperdrive id"), "{given}");
    let wrangler: Value = serde_json::from_str(
        &fs::read_to_string(out.join("wrangler.json")).expect("the Wrangler file"),
    )
    .expect("JSON");
    assert_eq!(
        wrangler["hyperdrive"],
        serde_json::json!([{ "binding": "DATABASE_URL", "id": ID }]),
        "{wrangler}"
    );

    if !jadpo_test::live() {
        return;
    }
    let database = Database::on_tcp();
    let migrations = migrations(&root);
    let (last, earlier) = migrations.split_last().expect("migrations");
    apply(&database.native, earlier);

    // Without its database's address it serves nothing: every request is
    // 503, and its isolate logs why once, naming the setting's variable.
    let worker = Workerd::start(&out, &wrangler, &[]);
    for _ in 0..2 {
        let (status, body) = worker.send("POST", "/repairs", &[], LAMP);
        assert_eq!(status, 503, "{body}\n{}", worker.log());
        let answer: Value = serde_json::from_str(&body).expect("JSON");
        assert_eq!(answer["error"]["code"], "unavailable", "{answer}");
        assert!(
            answer["error"]["request_id"]
                .as_str()
                .is_some_and(|id| id.starts_with("req_")),
            "{answer}"
        );
    }
    let failed = worker.events("runtime.startup_failed");
    assert_eq!(failed.len(), 1, "{}", worker.log());
    stamped(&failed[0], built, kiosk);
    let problem = &failed[0]["problems"][0];
    assert_eq!(problem["problem"], "setting_missing", "{problem}");
    assert_eq!(problem["variable"], "DATABASE_URL", "{problem}");
    drop(worker);

    // Against a database that hasn't applied its last migration it serves
    // nothing either, naming the migration. The database is reached
    // through the Hyperdrive binding the Wrangler file names, which workerd
    // hands to the database's server, as Cloudflare hands it to Hyperdrive.
    let worker = Workerd::start(&out, &wrangler, &[("DATABASE_URL", &database.worker)]);
    let (status, body) = worker.send("POST", "/repairs", &[], LAMP);
    assert_eq!(status, 503, "{body}\n{}", worker.log());
    let failed = worker.events("runtime.startup_failed");
    assert_eq!(failed.len(), 1, "{}", worker.log());
    stamped(&failed[0], built, kiosk);
    let problem = &failed[0]["problems"][0];
    assert_eq!(problem["problem"], "migrations_pending", "{failed:?}");
    assert_eq!(problem["store"], "main", "{failed:?}");
    assert_eq!(problem["pending"], serde_json::json!([last.name]));
    assert!(
        !worker.log().contains(&database.worker) && !worker.log().contains("hyperdrive.local"),
        "the log holds the database's address:\n{}",
        worker.log()
    );

    // Up to date, the same isolate checks again and serves: the booking's
    // subscribers find it the staffed bench and print its ticket while the
    // request waits, and the answer is the ticket read back.
    apply(&database.native, &migrations);
    rows(
        &database.native,
        "INSERT INTO bench (id, item, fixer) VALUES ('6f0c4d5e-2b1a-4c3d-9e8f-7a6b5c4d3e2f', 'lamp', 'Ada')",
    );
    let (status, body) = worker.send("POST", "/repairs", &[], LAMP);
    assert_eq!(status, 201, "{body}\n{}", worker.log());
    let ticket: Value = serde_json::from_str(&body).expect("a ticket");
    assert_eq!(ticket["item"], "lamp", "{ticket}");
    assert_eq!(
        ticket["bench_id"], "6f0c4d5e-2b1a-4c3d-9e8f-7a6b5c4d3e2f",
        "{ticket}"
    );
    assert!(ticket["ticket_printed_at"].is_string(), "{ticket}");
    let repair = ticket["id"].as_str().expect("its id").to_string();
    assert_eq!(
        rows(
            &database.native,
            &format!("SELECT item, bench_id::text FROM repair WHERE id = '{repair}'"),
        ),
        vec![vec![
            "lamp".to_string(),
            "6f0c4d5e-2b1a-4c3d-9e8f-7a6b5c4d3e2f".to_string()
        ]]
    );
    let started = worker.events("runtime.started");
    assert_eq!(started.len(), 1, "{}", worker.log());
    stamped(&started[0], built, kiosk);
    assert_eq!(started[0]["unbuilt"], serde_json::json!([]), "{started:?}");

    // A thank-you is answered at once, its note pinned up after the
    // answer, in `waitUntil`.
    let (status, body) = worker.send(
        "POST",
        &format!("/repairs/{repair}/thanks"),
        &[],
        r#"{"note":"Good as new, thank you!"}"#,
    );
    assert_eq!(status, 202, "{body}\n{}", worker.log());
    let mut pinned = Vec::new();
    for _ in 0..100 {
        pinned = rows(
            &database.native,
            &format!("SELECT note FROM thank_you_note WHERE repair_id = '{repair}'"),
        );
        if !pinned.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(
        pinned,
        vec![vec!["Good as new, thank you!".to_string()]],
        "{}",
        worker.log()
    );

    // Its readiness check reaches the database.
    let (status, body) = worker.send("GET", "/health/ready", &[], "");
    assert_eq!(status, 200, "{body}\n{}", worker.log());

    // Each address `CF-Connecting-IP` gives is counted on its own, in the
    // rate-limiting binding the Wrangler file names: 120 requests a
    // minute, then 429, while another address still has its allowance.
    let one = [("CF-Connecting-IP", "203.0.113.9")];
    for _ in 0..120 {
        let (status, body) = worker.send("GET", "/nowhere", &one, "");
        assert_eq!(status, 404, "{body}");
    }
    let (status, body) = worker.send("GET", "/nowhere", &one, "");
    assert_eq!(status, 429, "{body}");
    assert_eq!(
        worker
            .log()
            .matches("rate-limiting binding: address 203.0.113.9 ")
            .count(),
        121,
        "{}",
        worker.log()
    );
    let (status, body) = worker.send(
        "GET",
        "/nowhere",
        &[("CF-Connecting-IP", "203.0.113.10")],
        "",
    );
    assert_eq!(status, 404, "{body}");

    // Every line it logged is one JSON object carrying the stamp.
    for event in worker.events("") {
        stamped(&event, built, kiosk);
    }
}

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

/// The time on this machine, in milliseconds since 1970.
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as i64)
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

/// The news desk, built for Workers: stories filed are summarised by a
/// subscriber, the archive's clippings by a batch run, and a draft is
/// streamed from the model as it is written.
const NEWSROOM: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"application {
    connections: {
        limits: {
            driver: Driver.cloudflare_rate_limits
            namespace: 1001
        }
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
        local_model: {
            driver: Driver.openai_compatible
            url: config.model_url
        }
    }
    events: {
        store: main
    }
    rate_limits: {
        store: limits
    }
    ai: {
        connection: local_model
        models: {
            default: config.drafting_model
        }
        limits: {
            per_day: {
                tokens: 2000000
            }
        }
    }
    deployables: {
        desk: {
            serves: [main]
            runs: [Trigger.subscribers, Trigger.schedules]
            target: Target.wasm
        }
    }
}
"#,
    ),
    (
        "config.jadpo",
        r#"config {
    database_url: PostgresUrl
    model_url: Url
    drafting_model: ModelId {
        default: "local/llama3.2"
    }
}
"#,
    ),
    (
        "domains/newsroom/actions/draft.jadpo",
        r#"use newsroom/failures { ModelUnavailable }
use newsroom/prompts { DraftStory }
use newsroom/types { DraftAsked }

/// Drafts a story as it is written.
action draft(asked: DraftAsked) fails ModelUnavailable -> AiStream {
    return attempt ai.stream(
        DraftStory {
            headline: asked.headline
        },
        max_output: 300,
        timeout: 20s
    ) {
        refused: ModelUnavailable
        timed_out: ModelUnavailable
        over_budget: ModelUnavailable
        too_long: ModelUnavailable
    }
}
"#,
    ),
    (
        "domains/newsroom/actions/file_stories.jadpo",
        r#"use newsroom/entities { Story }
use newsroom/events { StoryFiled }
use newsroom/types { Filed, FiledStories }

/// Files each story, each summarised after the answer.
action file_stories(filed: Filed) -> FiledStories {
    for headline in filed.headlines {
        var story: Story = Story.create {
            set: {
                headline: headline
                summary: none
            }
        }
        emit StoryFiled {
            story_id: story.id
        }
    }
    return FiledStories {
        filed: list.count(filed.headlines)
    }
}
"#,
    ),
    (
        "domains/newsroom/actions/resummarise_clippings.jadpo",
        r#"use newsroom/actions { summarise_clipping }
use newsroom/entities { ArchiveWalk }
use newsroom/queries { clippings_page }
use newsroom/types { ClippingPage, Resummarising }

/// Tells every clipping in one line again, one after another.
action resummarise_clippings() -> Resummarising {
    var run: BatchRun = batches.each(
        from: clippings_page(ClippingPage {}),
        each: clipping -> summarise_clipping(clipping)
    )
    var walk: ArchiveWalk = ArchiveWalk.create {
        set: {
            run: run
        }
    }
    return Resummarising {
        walk_id: walk.id
    }
}
"#,
    ),
    (
        "domains/newsroom/actions/summarise_clipping.jadpo",
        r#"use newsroom/entities { Clipping }
use newsroom/failures { ModelUnavailable }
use newsroom/prompts { Summarise }
use newsroom/types { Summary }

/// Tells a clipping in one line.
action summarise_clipping(clipping: Clipping) fails ModelUnavailable -> Unit {
    var summary: Summary = attempt ai.generate(
        Summarise {
            headline: clipping.headline
        },
        as: Summary,
        max_output: 100
    ) {
        refused: ModelUnavailable
        malformed: ModelUnavailable
        timed_out: ModelUnavailable
        over_budget: ModelUnavailable
        too_long: ModelUnavailable
    }
    var summarised: Clipping = Clipping.update(clipping) {
        set: {
            summary: summary.line
        }
    }
    assert summarised.summary == summary.line
}
"#,
    ),
    (
        "domains/newsroom/actions/summarise_story.jadpo",
        r#"use newsroom/entities { Story, StoryNotFound }
use newsroom/failures { ModelUnavailable }
use newsroom/prompts { Summarise }
use newsroom/types { Summary }

/// Tells a story filed in one line.
action summarise_story(story_id: Story.id) fails StoryNotFound, ModelUnavailable -> Unit {
    var story: Story = attempt Story.get(story_id)
    var summary: Summary = attempt ai.generate(
        Summarise {
            headline: story.headline
        },
        as: Summary,
        max_output: 100
    ) {
        refused: ModelUnavailable
        malformed: ModelUnavailable
        timed_out: ModelUnavailable
        over_budget: ModelUnavailable
        too_long: ModelUnavailable
    }
    var summarised: Story = Story.update(story) {
        set: {
            summary: summary.line
        }
    }
    assert summarised.summary == summary.line
}
"#,
    ),
    (
        "domains/newsroom/entities/archive_walk.jadpo",
        r#"/// A walk through the archive, telling each clipping again.
entity ArchiveWalk {
    id: Uuid
    run: BatchRun

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
    }
}
"#,
    ),
    (
        "domains/newsroom/entities/clipping.jadpo",
        r#"use newsroom/types { Headline, SummaryLine }

/// A story kept in the archive.
entity Clipping {
    id: Uuid
    headline: Headline
    summary: SummaryLine?

    policy: {
        read: [Access.system]
        update: [Access.system]
    }
}
"#,
    ),
    (
        "domains/newsroom/entities/story.jadpo",
        r#"use newsroom/types { Headline, SummaryLine }

/// A story filed.
entity Story {
    id: Uuid
    headline: Headline
    summary: SummaryLine?

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.system]
    }
}
"#,
    ),
    (
        "domains/newsroom/events/story_filed.jadpo",
        r#"use newsroom/entities { Story }

/// A story was filed.
type StoryFiled = Event {
    story_id: Story.id
}
"#,
    ),
    (
        "domains/newsroom/failures/model_unavailable.jadpo",
        r#"/// The model can't answer now.
type ModelUnavailable = Failure {
    kind: FailureKind.unavailable
    message: "The model can't answer now."
}
"#,
    ),
    (
        "domains/newsroom/prompts/draft_story.jadpo",
        r#"use newsroom/types { Headline }

/// Drafts a story under a headline.
type DraftStory = Prompt {
    headline: Headline
}
"#,
    ),
    (
        "domains/newsroom/prompts/draft_story.prompt",
        r#"# Instructions

You draft a newspaper's stories.

# Request

{headline}
"#,
    ),
    (
        "domains/newsroom/prompts/summarise.jadpo",
        r#"use newsroom/types { Headline }

/// Tells a story in one line.
type Summarise = Prompt {
    headline: Headline
}
"#,
    ),
    (
        "domains/newsroom/prompts/summarise.prompt",
        r#"# Instructions

You tell a newspaper's stories in one line.

# Request

{headline}
"#,
    ),
    (
        "domains/newsroom/queries/clippings_page.jadpo",
        r#"use newsroom/entities { Clipping }
use newsroom/types { ClippingPage }

/// The archive, a page at a time.
query clippings_page(page: ClippingPage) -> Page<Clipping> {
    from: Clipping
    order: [Clipping.id asc]
    page: {
        size: page.page_size
        after: page.after
    }
}
"#,
    ),
    (
        "domains/newsroom/routes/desk.jadpo",
        r#"use newsroom/actions { draft, file_stories, resummarise_clippings }
use newsroom/types { DraftAsked, Filed, FiledStories, Resummarising }

/// The news desk.
routes Desk {
    prefix: /desk

    /// Files stories, each summarised after the answer.
    POST /stories {
        authentication: RouteAuthentication.none
        input: Filed
        output: FiledStories
        status: SuccessStatus.created
        run: file_stories(input)
    }

    /// Tells every clipping in the archive in one line again.
    POST /clippings/summaries {
        authentication: RouteAuthentication.none
        output: Resummarising
        status: SuccessStatus.accepted
        run: resummarise_clippings()
    }

    /// Drafts a story, sent as it is written.
    POST /drafts {
        authentication: RouteAuthentication.none
        input: DraftAsked
        output: AiStream
        deadline: 30s
        run: draft(input)
    }
}
"#,
    ),
    (
        "domains/newsroom/subscribers/summarise_filed.jadpo",
        r#"use newsroom/actions { summarise_story }
use newsroom/events { StoryFiled }

/// Tells each story filed in one line.
subscriber summarise_filed {
    on: StoryFiled
    run: summarise_story(event.story_id)
}
"#,
    ),
    (
        "domains/newsroom/types/clipping_page.jadpo",
        r#"/// A page of clippings.
type ClippingPage = Input {
    page_size: PageSize {
        default: 20
    }
    after: PageCursor {
        optional: true
    }
}
"#,
    ),
    (
        "domains/newsroom/types/draft_asked.jadpo",
        r#"use newsroom/types { Headline }

/// A headline to draft a story for.
type DraftAsked = Input {
    headline: Headline
}
"#,
    ),
    (
        "domains/newsroom/types/filed.jadpo",
        r#"use newsroom/types { Headline }

/// Stories filed together.
type Filed = Input {
    headlines: List<Headline>
}
"#,
    ),
    (
        "domains/newsroom/types/filed_stories.jadpo",
        r#"/// How many stories were filed.
type FiledStories = Output {
    filed: Int
}
"#,
    ),
    (
        "domains/newsroom/types/headline.jadpo",
        r#"/// A story's headline.
type Headline = Text {
    min_length: 1
    max_length: 120
}
"#,
    ),
    (
        "domains/newsroom/types/resummarising.jadpo",
        r#"use newsroom/entities { ArchiveWalk }

/// The walk telling the archive's clippings again.
type Resummarising = Output {
    walk_id: ArchiveWalk.id
}
"#,
    ),
    (
        "domains/newsroom/types/summary.jadpo",
        r#"use newsroom/types { SummaryLine }

/// What the model says a story is.
type Summary = Output {
    line: SummaryLine
}
"#,
    ),
    (
        "domains/newsroom/types/summary_line.jadpo",
        r#"/// A story told in one line.
type SummaryLine = Text {
    max_length: 200
}
"#,
    ),
];

/// The news desk, written where its build is kept between runs, a file
/// left as it was keeping its time, with its migration and records.
fn newsroom() -> PathBuf {
    let target = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let root = target
        .parent()
        .unwrap_or(target)
        .join("jadpo-compiled/workers/newsroom");
    // Beside the desk's own, the routes that probe a deployable's limits
    // (chunk 8.4a), its policy opening both to anyone.
    let policy = format!(
        "use newsroom\nuse probe\n\npolicy {{\n    actions: {{\n        newsroom.draft: [Access.public]\n{}    }}\n}}\n",
        probes::POLICY
    );
    let files = NEWSROOM
        .iter()
        .chain(probes::FILES)
        .copied()
        .chain([("policy.jadpo", policy.as_str())]);
    for (path, text) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("a folder")).expect("folders");
        if fs::read_to_string(&path).ok().as_deref() != Some(text) {
            fs::write(&path, text).expect("written");
        }
    }
    jadpo(&root, &["api", "update"]);
    if !root.join("database/migrations").exists() {
        jadpo(&root, &["migrate", "new", "initial"]);
    }
    jadpo(&root, &["records", "update"]);
    root
}

/// How long the desk's model takes to summarise a story.
const SUMMARY_MS: u64 = 2_000;

/// How many stories are filed together, and how many clippings the
/// archive holds: each summarised in [`SUMMARY_MS`], more than one
/// invocation's 30 seconds of either.
const STORIES: usize = 18;

/// The desk's model: a server on this machine speaking OpenAI's Chat
/// Completions API. A call for a summary is answered after
/// [`SUMMARY_MS`] with the line `Told: <headline>`, and when it was asked
/// for which headline noted; a call for a stream is sent its head and its
/// first piece, `First `, and then handed to the test, which writes the
/// rest or watches it close.
struct Model {
    port: u16,
    asked: Arc<Mutex<Vec<(Instant, String)>>>,
    streams: mpsc::Receiver<TcpStream>,
}

impl Model {
    fn start() -> Model {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("an address").port();
        let asked = Arc::new(Mutex::new(Vec::new()));
        let (streamed, streams) = mpsc::channel();
        let noted = Arc::clone(&asked);
        std::thread::spawn(move || {
            for connection in listener.incoming() {
                let Ok(stream) = connection else {
                    return;
                };
                let noted = Arc::clone(&noted);
                let streamed = streamed.clone();
                std::thread::spawn(move || Model::answer(stream, &noted, &streamed));
            }
        });
        Model {
            port,
            asked,
            streams,
        }
    }

    fn answer(
        mut stream: TcpStream,
        asked: &Mutex<Vec<(Instant, String)>>,
        streamed: &mpsc::Sender<TcpStream>,
    ) {
        let Some(request) = model_request(&stream) else {
            return;
        };
        let body: Value = request
            .split_once("\r\n\r\n")
            .and_then(|(_, body)| serde_json::from_str(body).ok())
            .unwrap_or(Value::Null);
        if body["stream"] == true {
            let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n";
            if stream.write_all(head.as_bytes()).is_ok()
                && model_chunk(&mut stream, &completion_chunk("First ", None)).is_ok()
            {
                let _ = streamed.send(stream);
            }
            return;
        }
        let headline = regex::Regex::new(r"(Story|Clipping) \d\d")
            .expect("a pattern")
            .find(&request)
            .map(|found| found.as_str().to_string())
            .unwrap_or_default();
        asked
            .lock()
            .expect("noted")
            .push((Instant::now(), headline.clone()));
        std::thread::sleep(Duration::from_millis(SUMMARY_MS));
        let answer = serde_json::json!({
            "id": "chatcmpl-1",
            "object": "chat.completion",
            "model": "local/llama3.2",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": serde_json::json!({ "line": format!("Told: {headline}") }).to_string(),
                },
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 40, "completion_tokens": 12, "total_tokens": 52}
        })
        .to_string();
        // A call the Worker gave up on finds its connection gone.
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}",
            answer.len()
        );
    }

    /// When each headline starting with `kind` was asked for, from the
    /// first.
    fn asked(&self, kind: &str) -> Vec<(Duration, String)> {
        let asked = self.asked.lock().expect("noted");
        let mine: Vec<&(Instant, String)> = asked
            .iter()
            .filter(|(_, headline)| headline.starts_with(kind))
            .collect();
        let Some(first) = mine.iter().map(|(at, _)| *at).min() else {
            return Vec::new();
        };
        mine.iter()
            .map(|(at, headline)| (at.duration_since(first), headline.clone()))
            .collect()
    }
}

/// One request the model read: its head and body.
fn model_request(stream: &TcpStream) -> Option<String> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut seen = String::new();
    let mut length = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = value.trim().parse().ok()?;
        }
        seen.push_str(&line);
        if line == "\r\n" {
            break;
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    seen.push_str(&String::from_utf8_lossy(&body));
    Some(seen)
}

/// A chunk of OpenAI's streamed chat completion.
fn completion_chunk(content: &str, finish: Option<&str>) -> String {
    let event = serde_json::json!({
        "id": "chatcmpl-1",
        "object": "chat.completion.chunk",
        "model": "local/llama3.2",
        "choices": [{"index": 0, "delta": {"content": content}, "finish_reason": finish}]
    });
    format!("data: {event}\n\n")
}

/// Writes one part of a chunked body.
fn model_chunk(stream: &mut TcpStream, text: &str) -> std::io::Result<()> {
    stream.write_all(format!("{:x}\r\n{text}\r\n", text.len()).as_bytes())?;
    stream.flush()
}

/// A client of the Worker reading a streamed answer as it comes.
struct Reader {
    reader: BufReader<TcpStream>,
}

impl Reader {
    /// Asks the desk for a draft under `headline`.
    fn draft(port: u16, headline: &str) -> Reader {
        let body = serde_json::json!({ "headline": headline }).to_string();
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("workerd listens");
        stream
            .set_read_timeout(Some(Duration::from_secs(20)))
            .expect("a timeout");
        write!(
            stream,
            "POST /desk/drafts HTTP/1.1\r\nHost: desk\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .expect("sent");
        Reader {
            reader: BufReader::new(stream),
        }
    }

    /// The answer's head, lowercased.
    fn head(&mut self) -> String {
        let mut head = String::new();
        loop {
            let mut line = String::new();
            self.reader.read_line(&mut line).expect("a line");
            head.push_str(&line.to_ascii_lowercase());
            if line == "\r\n" || line.is_empty() {
                return head;
            }
        }
    }

    /// What the chunked body holds from here until `until` ends it, or
    /// its end.
    fn until(&mut self, until: &str) -> String {
        let mut text = String::new();
        while !text.ends_with(until) {
            let mut size = String::new();
            self.reader.read_line(&mut size).expect("a size");
            let size = usize::from_str_radix(size.trim(), 16).expect("a chunk's size");
            if size == 0 {
                break;
            }
            let mut data = vec![0; size + 2];
            self.reader.read_exact(&mut data).expect("a chunk");
            data.truncate(size);
            text.push_str(&String::from_utf8_lossy(&data));
        }
        text
    }
}

/// After the response (chunk 8.2h), on Workers: a draft streamed as it is
/// written, its first piece before the model writes the rest, and a
/// client going away closing the model's connection; stories filed
/// together summarised by a subscriber after the answer, and a batch run
/// through the archive, each taking longer than one invocation's 30
/// seconds, handing what is left on to a fresh invocation of the Worker,
/// which carries it to its end; and the minute's cron sweeping what the
/// budget's end left.
#[test]
fn a_worker_streams_answers_and_hands_on_what_one_invocation_leaves() {
    let root = newsroom();
    let output = jadpo(&root, &["build", "--json"]);
    let document: Value = serde_json::from_str(&output.stdout).expect("JSON");
    let built = &document["data"];
    let desk = &built["deployables"][0];
    assert_eq!(desk["deployable"], "desk", "{built}");
    let out = root.join("build/target/desk");
    let wrangler: Value = serde_json::from_str(
        &fs::read_to_string(out.join("wrangler.json")).expect("the Wrangler file"),
    )
    .expect("JSON");
    // It runs background work, handed on through the loopback binding
    // `ctx.exports` gives the entrypoint its shim exports, so no binding
    // names the Worker, whatever name `wrangler deploy` gives it (chunk
    // 8.4b).
    assert!(wrangler.get("services").is_none(), "{wrangler}");
    // Beside its database's Hyperdrive binding, the minute's cron and its
    // rate-limiting bindings.
    assert_eq!(
        wrangler["hyperdrive"][0]["binding"], "DATABASE_URL",
        "{wrangler}"
    );
    assert_eq!(
        wrangler["triggers"]["crons"],
        serde_json::json!(["* * * * *"]),
        "{wrangler}"
    );
    assert!(
        wrangler["ratelimits"]
            .as_array()
            .is_some_and(|limits| !limits.is_empty()),
        "{wrangler}"
    );
    let shim = fs::read_to_string(out.join("worker.mjs")).expect("the shim");
    assert!(
        shim.contains("export class JadpoHandOn extends WorkerEntrypoint {"),
        "{shim}"
    );
    assert!(
        names(
            &linked(&root, "desk"),
            "jadpo_host_workers",
            "deployable::hand_on"
        ),
        "it lacks the hand-on handler"
    );

    if !jadpo_test::live() {
        return;
    }
    let database = Database::on_tcp();
    apply(&database.native, &migrations(&root));
    for at in 1..=STORIES {
        rows(
            &database.native,
            &format!(
                "INSERT INTO clipping (id, headline) VALUES (gen_random_uuid(), 'Clipping {at:02}') RETURNING 1"
            ),
        );
    }
    let model = Model::start();
    let model_url = format!("http://127.0.0.1:{}/v1", model.port);
    let worker = Workerd::start(
        &out,
        &wrangler,
        &[
            ("DATABASE_URL", &database.worker),
            ("MODEL_URL", &model_url),
        ],
    );

    // A draft reaches the client as the model writes it: its first piece
    // before the model has written the rest.
    let mut reader = Reader::draft(worker.port, "Harbour wall repaired");
    let mut written = model
        .streams
        .recv_timeout(Duration::from_secs(30))
        .unwrap_or_else(|_| panic!("the model wasn't asked:\n{}", worker.log()));
    let head = reader.head();
    assert!(head.starts_with("http/1.1 200 ok\r\n"), "{head}");
    assert!(
        head.contains("content-type: text/event-stream\r\n"),
        "{head}"
    );
    assert!(head.contains("cache-control: no-store\r\n"), "{head}");
    assert!(!head.contains("content-length"), "{head}");
    assert_eq!(
        reader.until("\n\n"),
        "event: piece\ndata: {\"text\":\"First \"}\n\n",
        "{}",
        worker.log()
    );
    model_chunk(
        &mut written,
        &completion_chunk("then the rest.", Some("stop")),
    )
    .expect("a piece");
    model_chunk(&mut written, "data: [DONE]\n\n").expect("done");
    model_chunk(&mut written, "").expect("the end");
    drop(written);
    assert_eq!(
        reader.until("event: done\ndata: {}\n\n"),
        "event: piece\ndata: {\"text\":\"then the rest.\"}\n\nevent: done\ndata: {}\n\n"
    );
    assert_eq!(reader.until("\n\n"), "", "the stream ends after `done`");

    // A client that goes away ends the request, and with it the model's
    // call: workerd finds the client gone when it next sends it a piece,
    // so the model writes on until its connection closes.
    let mut reader = Reader::draft(worker.port, "Ferry timetable changes");
    let mut written = model
        .streams
        .recv_timeout(Duration::from_secs(30))
        .expect("the model is asked");
    reader.head();
    assert_eq!(
        reader.until("\n\n"),
        "event: piece\ndata: {\"text\":\"First \"}\n\n"
    );
    drop(reader);
    written
        .set_read_timeout(Some(Duration::from_millis(200)))
        .expect("a timeout");
    let until = Instant::now() + Duration::from_secs(10);
    let mut closed = false;
    while !closed && Instant::now() < until {
        if model_chunk(&mut written, &completion_chunk("more ", None)).is_err() {
            closed = true;
            break;
        }
        let mut rest = [0u8; 16];
        closed = match written.read(&mut rest) {
            Ok(0) => true,
            Err(error) => !matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ),
            Ok(_) => false,
        };
    }
    assert!(
        closed,
        "the model's connection stayed open:\n{}",
        worker.log()
    );

    // Stories filed together, and a walk through the archive, each more
    // work than one invocation's 30 seconds.
    let headlines: Vec<String> = (1..=STORIES).map(|at| format!("Story {at:02}")).collect();
    let began = Instant::now();
    let (status, body) = worker.send(
        "POST",
        "/desk/stories",
        &[],
        &serde_json::json!({ "headlines": headlines }).to_string(),
    );
    assert_eq!(status, 201, "{body}\n{}", worker.log());
    let (status, body) = worker.send("POST", "/desk/clippings/summaries", &[], "");
    assert_eq!(status, 202, "{body}\n{}", worker.log());
    assert!(
        began.elapsed() < Duration::from_secs(10),
        "answered after {:?}",
        began.elapsed()
    );
    let summarised = |table: &str| {
        count_where(
            &database.native,
            &format!("SELECT count(*) FROM {table} WHERE summary = 'Told: ' || headline"),
        )
    };
    let finished = || {
        count_where(
            &database.native,
            "SELECT count(*) FROM jadpo_batch_runs WHERE state = 'finished'",
        )
    };
    let until = Instant::now() + Duration::from_secs(120);
    while (summarised("story") < STORIES as i64 - 1
        || summarised("clipping") < STORIES as i64
        || finished() < 1)
        && Instant::now() < until
    {
        std::thread::sleep(Duration::from_millis(250));
    }
    // Every clipping is told by the run, which finished, its last items
    // asked for after the first invocation's budget was spent: in a fresh
    // invocation it was handed on to.
    assert_eq!(summarised("clipping"), STORIES as i64, "{}", worker.log());
    assert_eq!(
        rows(
            &database.native,
            "SELECT state, items_done::text FROM jadpo_batch_runs"
        ),
        vec![vec!["finished".to_string(), STORIES.to_string()]],
        "{}",
        worker.log()
    );
    let clippings = model.asked("Clipping");
    assert!(
        clippings
            .iter()
            .any(|(after, _)| *after > Duration::from_secs(28)),
        "every clipping was asked for within one invocation: {clippings:?}"
    );
    // Every story but at most one, whose summary the budget's end cut
    // short and which waits to be tried again, is told by the subscriber
    // after the answer, the last of them in a fresh invocation.
    assert!(
        summarised("story") >= STORIES as i64 - 1,
        "{} of {STORIES} stories told:\n{}",
        summarised("story"),
        worker.log()
    );
    let stories = model.asked("Story");
    assert!(
        stories
            .iter()
            .any(|(after, _)| *after > Duration::from_secs(28)),
        "every story was asked for within one invocation: {stories:?}"
    );
    let waiting = rows(
        &database.native,
        "SELECT status, attempts::text, last_failure ->> 'code' FROM jadpo_deliveries WHERE status <> 'succeeded'",
    );
    assert!(
        waiting.is_empty()
            || waiting
                == vec![vec![
                    "pending".to_string(),
                    "1".to_string(),
                    "deadline_exceeded".to_string()
                ]],
        "{waiting:?}"
    );
    // The minute's cron sweeps what an invocation left: due now, a summary
    // waiting to be tried again is told, and every delivery has succeeded.
    rows(
        &database.native,
        "UPDATE jadpo_deliveries SET next_attempt_at = now() WHERE status = 'pending' RETURNING 1",
    );
    assert_eq!(worker.scheduled(now_ms()), "ok", "{}", worker.log());
    assert_eq!(summarised("story"), STORIES as i64, "{}", worker.log());
    assert_eq!(
        rows(
            &database.native,
            "SELECT status FROM jadpo_deliveries WHERE status <> 'succeeded'"
        ),
        Vec::<Vec<String>>::new(),
        "{}",
        worker.log()
    );
    // Its limits, probed on the module's own stack and V8's (chunk 8.4a):
    // a thousand frames of any of the walks, four hundred inline
    // functions, and a level more of each the `recursion_limit` fault,
    // logged as such, the isolate serving on, where a trap would leave it
    // hung.
    for (path, depth, expected) in probes::WALKS {
        let (status, body) = worker.send("POST", path, &[], &probes::walk(*depth));
        match expected {
            Ok(()) => assert_eq!(
                (status, body.as_str()),
                (200, format!(r#"{{"depth":"{depth}"}}"#).as_str()),
                "{path} {depth}\n{}",
                worker.log()
            ),
            Err(code) => {
                assert_eq!(status, 500, "{path} {depth}: {body}\n{}", worker.log());
                assert!(body.contains(code), "{path} {depth}: {body}");
            }
        }
    }
    assert_eq!(
        worker
            .events("operation.failed")
            .iter()
            .filter(|event| event["classification"] == "recursion_limit")
            .count(),
        3,
        "{}",
        worker.log()
    );
    let started = worker.events("runtime.started");
    assert_eq!(started.len(), 1, "{}", worker.log());
    for event in worker.events("") {
        stamped(&event, built, desk);
    }
}

/// The one number `sql` counts.
fn count_where(connection: &str, sql: &str) -> i64 {
    rows(connection, sql)
        .first()
        .and_then(|row| row.first())
        .and_then(|count| count.parse().ok())
        .unwrap_or(0)
}

/// A booking for a lamp.
const LAMP: &str = r#"{"item":"lamp","symptom":"flickers when the shade is touched"}"#;

/// The stamp's part of an event, as the build said it of `deployable`.
fn stamped(event: &Value, built: &Value, deployable: &Value) {
    for key in ["commit", "modified", "compiler"] {
        assert_eq!(event[key], built[key], "{key} in {event}");
    }
    for key in ["build", "deployable", "target"] {
        assert_eq!(event[key], deployable[key], "{key} in {event}");
    }
}

/// A database a Worker reaches over TCP on this machine, and the server
/// it is on.
struct Database {
    /// Its address as the native client opens it.
    native: String,
    /// Its address as the Worker's `DATABASE_URL`: TCP to `127.0.0.1`.
    worker: String,
    _server: Server,
}

impl Database {
    /// A new database on the run's server when `JADPO_TEST_DATABASE_URL`
    /// names it by a URL, and otherwise on a server of the test's own
    /// listening on `127.0.0.1`.
    fn on_tcp() -> Database {
        let named = std::env::var(jadpo_test::postgres::DATABASE_URL_VARIABLE)
            .is_ok_and(|url| url.starts_with("postgres://") || url.starts_with("postgresql://"));
        let server = if named {
            Server::start()
        } else {
            Server::start_on_tcp(None)
        }
        .unwrap_or_else(|error| panic!("{error}"));
        let native = server.create_database("workers").expect("a database");
        let worker = match server.tcp_port() {
            // The pairs a disposable server gives, as a URL over TCP: its
            // database is the last `dbname=`, as libpq reads it.
            Some(port) => {
                let database = native
                    .split_whitespace()
                    .rev()
                    .find_map(|pair| pair.strip_prefix("dbname="))
                    .map(|name| name.trim_matches('\''))
                    .expect("a database's name");
                format!("postgresql://jadpo@127.0.0.1:{port}/{database}")
            }
            None => native.clone(),
        };
        Database {
            native,
            worker,
            _server: server,
        }
    }
}

/// workerd serving a built Worker on a port of this machine, stopped
/// when dropped, and beside it a Worker of the test's own on another port
/// that runs the built Worker's `scheduled` handler as Cloudflare's cron
/// trigger does, through a service binding's `scheduled()` (workerd's
/// `service_binding_extra_handlers`, which needs `--experimental`).
struct Workerd {
    child: Child,
    port: u16,
    cron: u16,
    log: PathBuf,
}

/// The test's cron: each request runs the bound Worker's `scheduled`
/// handler for the time its `?at=` gives, in milliseconds, and answers
/// with how it ended; a request to `/queue` hands the bound Worker's
/// `queue` handler the batch its body gives, as Cloudflare Queues does
/// (chunk 8.2k, `queues.rs`), and answers with how each message was.
const CRON: &str = "export default {\n  async fetch(request, env) {\n    if (new URL(request.url).pathname === \"/queue\") {\n      const { queue, messages } = await request.json();\n      const answered = await env.WORKER.queue(queue, messages.map((message) => ({ ...message, timestamp: new Date() })));\n      return Response.json(answered);\n    }\n    const at = Number(new URL(request.url).searchParams.get(\"at\"));\n    const ended = await env.WORKER.scheduled({ scheduledTime: new Date(at), cron: \"* * * * *\" });\n    return Response.json(ended);\n  },\n};\n";

impl Workerd {
    /// The Worker in `out` as `wrangler` describes it: its main module and
    /// the modules beside it by the file's rules, its compatibility date
    /// and its rate-limiting bindings, each a stand-in for Cloudflare's
    /// (tools/workers/ratelimit-binding.mjs), given `bindings`, its
    /// outbound network reaching this machine. A binding the Wrangler file
    /// names as Hyperdrive's is given as workerd's Hyperdrive binding for
    /// the database at that address, which hands `connect()` to its server,
    /// as `wrangler dev` does with a local connection string; any other is
    /// text.
    fn start(out: &Path, wrangler: &Value, bindings: &[(&str, &str)]) -> Workerd {
        // A folder of its own, config and log, since this module's tests
        // run side by side, each with its workerd.
        static STARTED: AtomicUsize = AtomicUsize::new(0);
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "workers-workerd/{}-{}",
            wrangler["name"].as_str().unwrap_or("worker"),
            STARTED.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("the folder");
        let main = wrangler["main"].as_str().expect("a main module");
        let mut modules = vec![format!("(name = \"{main}\", esModule = embed \"{main}\")")];
        for entry in fs::read_dir(out).expect("the Worker's folder") {
            let path = entry.expect("an entry").path();
            let name = path
                .file_name()
                .expect("a name")
                .to_string_lossy()
                .to_string();
            if !path.is_file() || name == main {
                continue;
            }
            let kind = match path.extension().and_then(|extension| extension.to_str()) {
                Some("js" | "mjs") => "esModule",
                Some("wasm") => "wasm",
                _ => continue,
            };
            modules.push(format!("(name = \"{name}\", {kind} = embed \"{name}\")"));
        }
        for module in fs::read_dir(out).expect("the Worker's folder") {
            let path = module.expect("an entry").path();
            if path.is_file() {
                fs::copy(&path, dir.join(path.file_name().expect("a name"))).expect("copied");
            }
        }
        let pooled: Vec<&str> = wrangler["hyperdrive"]
            .as_array()
            .map(|bindings| {
                bindings
                    .iter()
                    .filter_map(|binding| binding["binding"].as_str())
                    .collect()
            })
            .unwrap_or_default();
        let mut services = Vec::new();
        let mut bindings: Vec<String> = bindings
            .iter()
            .map(|(name, value)| {
                if !pooled.contains(name) {
                    return format!("(name = \"{name}\", text = \"{value}\")");
                }
                let (user, password, server, database) = parts(value);
                let service = format!("hyperdrive-{}", name.to_ascii_lowercase());
                services.push(format!(
                    "(name = \"{service}\", external = (address = \"{server}\", tcp = ())),"
                ));
                format!(
                    "(name = \"{name}\", hyperdrive = (designator = \"{service}\", database = \"{database}\", user = \"{user}\", password = \"{password}\", scheme = \"postgresql\"))"
                )
            })
            .collect();
        // Work is handed on through `ctx.exports`, which workerd gives
        // each entrypoint the Worker exports (chunk 8.4b): no binding.
        assert!(wrangler.get("services").is_none(), "{wrangler}");
        let tools = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/workers");
        fs::copy(
            tools.join("ratelimit-binding.mjs"),
            dir.join("ratelimit-binding.mjs"),
        )
        .expect("the stand-in rate-limiting binding");
        for binding in wrangler["ratelimits"].as_array().into_iter().flatten() {
            bindings.push(format!(
                "(name = \"{}\", wrapped = (moduleName = \"jadpo-test:ratelimit\", innerBindings = [(name = \"limit\", json = \"{}\"), (name = \"period\", json = \"{}\"), (name = \"log\", json = \"true\")]))",
                binding["name"].as_str().expect("a binding's name"),
                binding["simple"]["limit"],
                binding["simple"]["period"],
            ));
        }
        let free = || {
            TcpListener::bind("127.0.0.1:0")
                .and_then(|listener| listener.local_addr())
                .expect("a port")
                .port()
        };
        let (port, cron) = (free(), free());
        let date = wrangler["compatibility_date"].as_str().expect("a date");
        fs::write(dir.join("cron.mjs"), CRON).expect("the cron");
        fs::write(
            dir.join("config.capnp"),
            format!(
                "using Workerd = import \"/workerd/workerd.capnp\";\n\n\
                 const config :Workerd.Config = (\n\
                 \x20 services = [\n\
                 \x20   (name = \"kiosk\", worker = .kiosk),\n\
                 \x20   (name = \"cron\", worker = .cron),\n\
                 \x20   (name = \"machine\", network = (allow = [\"public\", \"private\", \"local\"])),\n\
                 \x20   {}\n\
                 \x20 ],\n\
                 \x20 sockets = [\n\
                 \x20   (name = \"http\", address = \"127.0.0.1:{port}\", http = (), service = \"kiosk\"),\n\
                 \x20   (name = \"cron\", address = \"127.0.0.1:{cron}\", http = (), service = \"cron\"),\n\
                 \x20 ],\n\
                 \x20 extensions = [(modules = [(name = \"jadpo-test:ratelimit\", internal = true, esModule = embed \"ratelimit-binding.mjs\")])],\n\
                 );\n\n\
                 const kiosk :Workerd.Worker = (\n\
                 \x20 modules = [{}],\n\
                 \x20 compatibilityDate = \"{date}\",\n\
                 \x20 bindings = [{}],\n\
                 \x20 globalOutbound = \"machine\",\n\
                 );\n\n\
                 const cron :Workerd.Worker = (\n\
                 \x20 modules = [(name = \"cron.mjs\", esModule = embed \"cron.mjs\")],\n\
                 \x20 compatibilityDate = \"{date}\",\n\
                 \x20 compatibilityFlags = [\"service_binding_extra_handlers\"],\n\
                 \x20 bindings = [(name = \"WORKER\", service = \"kiosk\")],\n\
                 );\n",
                services.join("\n    "),
                modules.join(", "),
                bindings.join(", "),
            ),
        )
        .expect("the config");
        let log = dir.join("workerd.log");
        let file = fs::File::create(&log).expect("the log");
        let child = Command::new(workerd())
            .args(["serve", "--experimental", "config.capnp"])
            .current_dir(&dir)
            .stdin(Stdio::null())
            .stdout(file.try_clone().expect("the log"))
            .stderr(file)
            .spawn()
            .expect("workerd starts");
        let mut worker = Workerd {
            child,
            port,
            cron,
            log,
        };
        let until = Instant::now() + Duration::from_secs(60);
        while TcpStream::connect(("127.0.0.1", port)).is_err()
            || TcpStream::connect(("127.0.0.1", cron)).is_err()
        {
            if let Ok(Some(status)) = worker.child.try_wait() {
                panic!("workerd ended ({status}):\n{}", worker.log());
            }
            assert!(
                Instant::now() < until,
                "workerd didn't listen within a minute:\n{}",
                worker.log()
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        worker
    }

    fn log(&self) -> String {
        fs::read_to_string(&self.log).unwrap_or_default()
    }

    /// Runs the Worker's `scheduled` handler as the cron trigger would for
    /// the minute at `at`, in milliseconds since 1970, waiting for the
    /// invocation to end: how it ended, `ok` or `exception`.
    fn scheduled(&self, at: i64) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", self.cron)).expect("the cron listens");
        stream
            .set_read_timeout(Some(Duration::from_secs(180)))
            .expect("a timeout");
        write!(
            stream,
            "GET /?at={at} HTTP/1.1\r\nHost: cron\r\nConnection: close\r\n\r\n"
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
        let ended: Value = serde_json::from_str(&body)
            .unwrap_or_else(|_| panic!("the cron's answer: {answer}\n{}", self.log()));
        ended["outcome"].as_str().unwrap_or_default().to_string()
    }

    /// The events the Worker has logged whose name starts with `event`,
    /// each a line of JSON; every line that names an event is one.
    fn events(&self, event: &str) -> Vec<Value> {
        self.log()
            .lines()
            .filter(|line| line.starts_with("{\"") && line.contains("\"event\""))
            .map(|line| {
                serde_json::from_str::<Value>(line)
                    .unwrap_or_else(|_| panic!("a log line is one JSON object: {line}"))
            })
            .filter(|logged| {
                logged["event"]
                    .as_str()
                    .is_some_and(|name| name.starts_with(event))
            })
            .collect()
    }

    /// One HTTP/1.1 exchange: the status and the body, read as far as the
    /// answer goes rather than until workerd closes the connection, which
    /// it may do only once the invocation's work after the answer has
    /// ended.
    fn send(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> (u16, String) {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).expect("workerd listens");
        stream
            .set_read_timeout(Some(Duration::from_secs(60)))
            .expect("a timeout");
        let mut head = format!(
            "{method} {path} HTTP/1.1\r\nHost: kiosk\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        );
        for (name, value) in headers {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
        write!(stream, "{head}\r\n{body}").expect("sent");
        let mut raw = Vec::new();
        let mut byte = [0u8; 1];
        while !raw.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap_or(0) == 1 {
            raw.push(byte[0]);
        }
        let head = String::from_utf8_lossy(&raw).to_ascii_lowercase();
        let status = head
            .split_whitespace()
            .nth(1)
            .and_then(|status| status.parse().ok())
            .unwrap_or_else(|| panic!("an HTTP answer: {head}\n{}", self.log()));
        let length = head.lines().find_map(|line| {
            line.strip_prefix("content-length:")
                .and_then(|length| length.trim().parse::<usize>().ok())
        });
        let mut body = Vec::new();
        match length {
            Some(length) => {
                body.resize(length, 0);
                stream.read_exact(&mut body).expect("the body");
            }
            None if head.contains("transfer-encoding: chunked") => {
                while !body.ends_with(b"0\r\n\r\n") && stream.read(&mut byte).unwrap_or(0) == 1 {
                    body.push(byte[0]);
                }
                return (status, dechunk(&String::from_utf8_lossy(&body)));
            }
            None => {
                let _ = stream.read_to_end(&mut body);
            }
        }
        (status, String::from_utf8_lossy(&body).into_owned())
    }
}

impl Drop for Workerd {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A PostgreSQL address's user, password, server (`host:port`) and
/// database, each percent-decoded, as libpq reads a URL: a `dbname=` in
/// its query names the database over its path, the last one winning, as
/// the run's server's addresses do (`…/postgres?dbname=jadpo_…`).
fn parts(url: &str) -> (String, String, String, String) {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let (rest, query) = rest.split_once('?').unwrap_or((rest, ""));
    let (user_info, rest) = rest.rsplit_once('@').unwrap_or(("", rest));
    let (user, password) = user_info.split_once(':').unwrap_or((user_info, ""));
    let (server, path) = rest.split_once('/').unwrap_or((rest, ""));
    let database = query
        .split('&')
        .rev()
        .find_map(|pair| pair.strip_prefix("dbname="))
        .unwrap_or(path);
    (
        decoded(user),
        decoded(password),
        server.to_string(),
        decoded(database),
    )
}

/// `text` with each `%XX` read as its byte.
fn decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let hex = text
            .get(at + 1..at + 3)
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match (bytes[at], hex) {
            (b'%', Some(byte)) => {
                out.push(byte);
                at += 3;
            }
            (byte, _) => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[test]
fn an_address_is_read_into_its_parts_as_libpq_reads_it() {
    assert_eq!(
        parts("postgresql://jadpo:jadpo@127.0.0.1:5432/postgres?dbname=jadpo_1_2_workers"),
        (
            "jadpo".to_string(),
            "jadpo".to_string(),
            "127.0.0.1:5432".to_string(),
            "jadpo_1_2_workers".to_string()
        )
    );
    assert_eq!(
        parts("postgres://app:p%40ss@db.test:6543/repair%20cafe?sslmode=x&dbname=a&dbname=b"),
        (
            "app".to_string(),
            "p@ss".to_string(),
            "db.test:6543".to_string(),
            "b".to_string()
        )
    );
    assert_eq!(
        parts("postgresql://jadpo@127.0.0.1:55432/jadpo_workers"),
        (
            "jadpo".to_string(),
            String::new(),
            "127.0.0.1:55432".to_string(),
            "jadpo_workers".to_string()
        )
    );
}

/// A chunked body's text.
fn dechunk(mut body: &str) -> String {
    let mut out = String::new();
    while let Some((size, rest)) = body.split_once("\r\n") {
        let size = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
        if size == 0 || rest.len() < size {
            break;
        }
        out.push_str(&rest[..size]);
        body = rest[size..].trim_start_matches("\r\n");
    }
    out
}

/// workerd: `JADPO_WORKERD`, else the one pinned in `tools/workers/`,
/// installed there by `npm ci` when it isn't yet, one process at a time
/// (`jadpo_test::workerd_install`).
fn workerd() -> PathBuf {
    jadpo_test::workerd_install::workerd().unwrap_or_else(|why| panic!("{why}"))
}
