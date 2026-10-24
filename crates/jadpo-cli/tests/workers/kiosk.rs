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
//! has its own allowance of 120 requests a minute; and over 500 bookings
//! more its isolate's linear memory holds steady (chunk 8.4e), read
//! through an entrypoint the test's own main module adds beside the
//! shim's. Every event it logs is one JSON line carrying the build stamp.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use jadpo_term::Environment;
use serde_json::Value;

use crate::deployable::{apply, copy_folder, migrations, rows};

use super::{Database, Workerd, custom_sections, jadpo, jadpo_in, linked, names, stamped};

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

/// A booking for a lamp.
const LAMP: &str = r#"{"item":"lamp","symptom":"flickers when the shade is touched"}"#;

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

    // Its isolate's memory holds steady over many bookings (chunk 8.4e):
    // the connection each booking opens says goodbye to the database
    // within the invocation that opened it, so nothing of it is kept once
    // that ends. Before, each was left waiting for its socket to close,
    // which workerd never settles once its invocation has ended: some 20 KB
    // a booking, never given back. Each booking comes from an address of
    // its own, within its allowance.
    let book = |at: usize| {
        let address = format!("198.51.100.{}", at % 250 + 1);
        let (status, body) =
            worker.send("POST", "/repairs", &[("CF-Connecting-IP", &address)], LAMP);
        // Read the exact durable state only on failure: a 202 is allowed by
        // this route, but this test still requires its expected inline result.
        // Keep its cause visible before the owned database is removed.
        let state = || {
            let response: Value = serde_json::from_str(&body).expect("a booking response");
            let id = response["id"]
                .as_str()
                .unwrap_or_default()
                .replace('\'', "''");
            rows(
                &database.native,
                &format!(
                    "SELECT json_build_object('repair', (SELECT row_to_json(r) FROM repair r WHERE id::text='{id}'), \
                     'deliveries', (SELECT json_agg(json_build_object('delivery',row_to_json(d),'event',o.event,'payload',o.payload)) \
                     FROM jadpo_deliveries d JOIN jadpo_outbox o ON o.id=d.outbox_id \
                     WHERE o.payload->>'repair_id'='{id}'))::text"
                ),
            )
        };
        assert_eq!(
            status,
            201,
            "{body}\nstate: {:?}\n{}",
            state(),
            worker.log()
        );
    };
    (0..WARM_BOOKINGS).for_each(book);
    let warm = worker.linear_memory();
    (WARM_BOOKINGS..WARM_BOOKINGS + BOOKINGS).for_each(book);
    let after = worker.linear_memory();
    println!(
        "measured: the kiosk's linear memory {warm} bytes after {WARM_BOOKINGS} bookings, {after} after {BOOKINGS} more"
    );
    assert!(
        after.saturating_sub(warm) <= STEADY_BYTES,
        "the kiosk's linear memory grew from {warm} to {after} bytes over {BOOKINGS} bookings"
    );

    // Every line it logged is one JSON object carrying the stamp.
    for event in worker.events("") {
        stamped(&event, built, kiosk);
    }
}

/// The bookings that bring the kiosk's memory to its working size, and
/// those its memory then holds steady over: by at most [`STEADY_BYTES`],
/// a few of WebAssembly's 64 KiB pages, where a booking's leaked
/// connection was some 20 KB, 8 MB over these (chunk 8.4e).
const WARM_BOOKINGS: usize = 100;
const BOOKINGS: usize = 400;
const STEADY_BYTES: u64 = 512 * 1024;
