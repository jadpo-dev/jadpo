//! A deployable's workspace (roadmap chunks 8.1o and 8.2b;
//! docs/compiler-runtime.md §2.6, "A native deployable" and "A Workers
//! deployable", and §2.8): the program's crate, a binary named after the
//! deployable whose `main` hands it, its build stamp and each store's
//! migrations to the native host, or for Workers a `cdylib` exporting the
//! Worker's `fetch`, which hands the same three to the Workers host,
//! linking only the drivers it reaches; the toolchain pinned and the
//! runtime crates' lock beside them, the same files every time. Then what
//! a Worker is deployed from: Jadpo's module shim and the Wrangler file.

use std::collections::BTreeMap;

use jadpo_codegen::deployable::{Deployable, Drivers, Host, Stamp, workspace};
use jadpo_codegen::workers;
use jadpo_db::{Database, File, Layout, Project};
use jadpo_plan::Plan;
use jadpo_std::RuntimePlan;

fn plan() -> Plan {
    let db = Database::default();
    let files = [("app.jadpo", "application {}\n")];
    let inputs: Vec<File> = files
        .iter()
        .map(|(path, text)| File::new(&db, path.to_string(), text.to_string()))
        .collect();
    let project = Project::new(
        &db,
        inputs,
        Vec::new(),
        Layout::Project,
        Default::default(),
        Vec::new(),
    );
    let problems: Vec<String> = jadpo_check::check(&db, project)
        .iter()
        .filter(|diagnostic| jadpo_check::blocks_running(diagnostic))
        .map(|diagnostic| format!("{}: {}", diagnostic.rule.id, diagnostic.message))
        .collect();
    assert!(problems.is_empty(), "{problems:#?}");
    Plan::build(&db, project)
}

fn deployable(stores: &BTreeMap<String, Vec<(String, String)>>) -> Deployable<'_> {
    Deployable {
        name: "kiosk",
        host: Host::Native,
        stamp: Stamp {
            build: "b_3c41d07a9e12",
            commit: Some("8d113e6"),
            modified: false,
            compiler: "0.1.0",
            target: "native",
        },
        stores,
        runtime: "/opt/jadpo/crates",
        toolchain: "[toolchain]\nchannel = \"1.99.0\"\n",
        lock: b"# the runtime crates' lock\n",
    }
}

fn text(files: &BTreeMap<String, Vec<u8>>, path: &str) -> String {
    let bytes = files
        .get(path)
        .unwrap_or_else(|| panic!("no {path} in {:?}", files.keys()));
    String::from_utf8(bytes.clone()).expect("text")
}

#[test]
fn a_deployable_is_its_program_and_a_binary_named_after_it() {
    let mut stores = BTreeMap::new();
    stores.insert(
        "main".to_string(),
        vec![
            ("20261019T2016_initial".to_string(), "4b1d".to_string()),
            (
                "20261020T0626_hold_deliveries".to_string(),
                "9e0a".to_string(),
            ),
        ],
    );
    let plan = plan();
    let generated = workspace(&plan, &deployable(&stores)).expect("written");
    let files = &generated.files;
    // The workspace: its two packages, the toolchain pinned and the
    // runtime crates' lock.
    let manifest = text(files, "Cargo.toml");
    assert!(
        manifest.contains("members = [\"program\", \"deployable\"]"),
        "{manifest}"
    );
    // Built as it ships (chunk 8.4f): the whole program optimised as one
    // unit at link time, for speed natively, its symbols kept for a
    // backtrace and a profiler.
    assert!(
        manifest.contains("[profile.release]\nopt-level = 3\nlto = \"fat\"\ncodegen-units = 1\n"),
        "{manifest}"
    );
    assert!(!manifest.contains("strip"), "{manifest}");
    assert_eq!(
        text(files, "rust-toolchain.toml"),
        "[toolchain]\nchannel = \"1.99.0\"\n"
    );
    assert_eq!(text(files, "Cargo.lock"), "# the runtime crates' lock\n");
    // The program, as `generate` writes it, holding the plan's bytes
    // without the dev and test section.
    // Its package is named after the deployable, so the programs of
    // several deployables built into one cargo target never share a
    // unit (chunk 8.1o2).
    let program = text(files, "program/Cargo.toml");
    assert!(
        program.contains("name = \"jadpo-program-kiosk\""),
        "{program}"
    );
    assert!(
        program.contains("jadpo-runtime = { path = \"/opt/jadpo/crates/jadpo-runtime\" }"),
        "{program}"
    );
    let bytes = files.get("program/plan.bin").expect("the plan's bytes");
    assert_eq!(
        RuntimePlan::decode(bytes).expect("a plan"),
        RuntimePlan::decode(&plan.runtime().encode_deployable().expect("bytes")).expect("a plan")
    );
    // The binary, named after the deployable, on the native host without
    // the `testing` feature.
    let binary = text(files, "deployable/Cargo.toml");
    assert!(binary.contains("name = \"kiosk\""), "{binary}");
    assert!(
        binary.contains(
            "jadpo-program = { path = \"../program\", package = \"jadpo-program-kiosk\" }"
        ),
        "{binary}"
    );
    assert!(
        binary.contains("jadpo-host-native = { path = \"/opt/jadpo/crates/jadpo-host-native\" }"),
        "{binary}"
    );
    assert!(!binary.contains("testing"), "{binary}");
    let main = text(files, "deployable/src/main.rs");
    syn::parse_file(&main).expect("Rust");
    for held in [
        "jadpo_host_native::deployable::run(&DEPLOYABLE)",
        "program: jadpo_program::program",
        "build: \"b_3c41d07a9e12\"",
        "commit: Some(\"8d113e6\")",
        "modified: false",
        "compiler: \"0.1.0\"",
        "deployable: \"kiosk\"",
        "target: \"native\"",
        "name: \"main\"",
        "name: \"20261020T0626_hold_deliveries\"",
        "digest: \"9e0a\"",
    ] {
        assert!(main.contains(held), "{held} isn't in:\n{main}");
    }
    // The same files every time.
    assert_eq!(
        workspace(&plan, &deployable(&stores)).expect("written"),
        generated
    );
}

#[test]
fn a_build_with_no_commit_says_so() {
    let stores = BTreeMap::new();
    let mut unstamped = deployable(&stores);
    unstamped.stamp.commit = None;
    unstamped.stamp.modified = true;
    let generated = workspace(&plan(), &unstamped).expect("written");
    let main = text(&generated.files, "deployable/src/main.rs");
    assert!(main.contains("commit: None"), "{main}");
    assert!(main.contains("modified: true"), "{main}");
    assert!(main.contains("stores: &[]"), "{main}");
}

#[test]
fn a_worker_is_its_program_and_a_cdylib_exporting_fetch() {
    let mut stores = BTreeMap::new();
    stores.insert(
        "main".to_string(),
        vec![("20261019T2016_initial".to_string(), "4b1d".to_string())],
    );
    let plan = plan();
    let mut worker = deployable(&stores);
    worker.host = Host::Workers(Drivers {
        postgres: true,
        smtp: false,
    });
    worker.stamp.target = "wasm";
    let generated = workspace(&plan, &worker).expect("written");
    let files = &generated.files;
    assert!(
        text(files, "Cargo.toml").contains("members = [\"program\", \"deployable\"]"),
        "{:?}",
        files.keys()
    );
    // A Worker's module is optimised for size, under a plan's limit on
    // what it uploads, as one unit at link time (chunk 8.4f); its names go
    // when it is bound (`workers::bind`).
    let manifest = text(files, "Cargo.toml");
    assert!(
        manifest
            .contains("[profile.release]\nopt-level = \"s\"\nlto = \"fat\"\ncodegen-units = 1\n"),
        "{manifest}"
    );
    assert!(text(files, "program/Cargo.toml").contains("name = \"jadpo-program-kiosk\""));
    // A `cdylib` named after the deployable, on the Workers host with the
    // one driver it reaches beyond HTTP, without the `testing` feature.
    let manifest = text(files, "deployable/Cargo.toml");
    for held in [
        "name = \"jadpo-worker-kiosk\"",
        "crate-type = [\"cdylib\"]",
        "jadpo-program = { path = \"../program\", package = \"jadpo-program-kiosk\" }",
        "jadpo-host-workers = { path = \"/opt/jadpo/crates/jadpo-host-workers\", features = [\"postgres\"] }",
    ] {
        assert!(manifest.contains(held), "{held} isn't in:\n{manifest}");
    }
    assert!(!manifest.contains("testing"), "{manifest}");
    assert!(!manifest.contains("jadpo-host-native"), "{manifest}");
    assert!(!files.contains_key("deployable/src/main.rs"));
    // The module's own stack, given at its link alone (chunk 8.4a).
    let build = text(files, "deployable/build.rs");
    syn::parse_file(&build).expect("Rust");
    let stack = format!(
        "cargo::rustc-link-arg=-zstack-size={}",
        jadpo_std::compiled::WASM_STACK_BYTES
    );
    assert!(build.contains(&stack), "{build}");
    let lib = text(files, "deployable/src/lib.rs");
    syn::parse_file(&lib).expect("Rust");
    for held in [
        "wasm_bindgen = jadpo_host_workers::wasm_bindgen",
        "pub fn fetch(request: JsValue, env: JsValue, ctx: JsValue) -> JsValue",
        "jadpo_host_workers::deployable::fetch(&DEPLOYABLE, request, env, ctx)",
        "pub fn scheduled(controller: JsValue, env: JsValue, ctx: JsValue) -> JsValue",
        "jadpo_host_workers::deployable::scheduled(&DEPLOYABLE, controller, env, ctx)",
        "pub fn hand_on(work: String, env: JsValue, ctx: JsValue) -> JsValue",
        "jadpo_host_workers::deployable::hand_on(&DEPLOYABLE, work, env, ctx)",
        "pub fn queue(batch: JsValue, env: JsValue, ctx: JsValue) -> JsValue",
        "jadpo_host_workers::deployable::queue(&DEPLOYABLE, batch, env, ctx)",
        "program: jadpo_program::program",
        "build: \"b_3c41d07a9e12\"",
        "deployable: \"kiosk\"",
        "target: \"wasm\"",
        "name: \"20261019T2016_initial\"",
    ] {
        assert!(lib.contains(held), "{held} isn't in:\n{lib}");
    }
    assert_eq!(workspace(&plan, &worker).expect("written"), generated);

    // A Worker that reaches no database and no SMTP server links neither.
    worker.host = Host::Workers(Drivers::default());
    let bare = workspace(&plan, &worker).expect("written");
    let manifest = text(&bare.files, "deployable/Cargo.toml");
    assert!(
        manifest.contains("jadpo-host-workers = { path = \"/opt/jadpo/crates/jadpo-host-workers\", features = [] }"),
        "{manifest}"
    );
    worker.host = Host::Workers(Drivers {
        postgres: true,
        smtp: true,
    });
    let both = workspace(&plan, &worker).expect("written");
    assert!(
        text(&both.files, "deployable/Cargo.toml").contains("features = [\"postgres\", \"smtp\"]")
    );
}

#[test]
fn a_worker_is_deployed_from_its_shim_and_its_wrangler_file() {
    let shim = workers::shim("back_office");
    for held in [
        "import module from \"./back_office.wasm\";",
        "import { initSync, fetch as handleFetch, scheduled as handleScheduled, queue as handleQueue, hand_on as handleHandOn } from \"./back_office.js\";",
        "initSync({ module });",
        "export default {",
        "return handleFetch(request, env, ctx);",
        "scheduled(controller, env, ctx) {",
        "return handleScheduled(controller, env, ctx);",
        // Each batch Cloudflare Queues pushes to its consumer (chunk 8.2k).
        "queue(batch, env, ctx) {",
        "return handleQueue(batch, env, ctx);",
        // Work handed on reaches a named entrypoint, never the default
        // export a request from outside reaches.
        "import { WorkerEntrypoint } from \"cloudflare:workers\";",
        "export class JadpoHandOn extends WorkerEntrypoint {",
        "handOn(work) {",
        "return handleHandOn(work, this.env, this.ctx);",
    ] {
        assert!(shim.contains(held), "{held} isn't in:\n{shim}");
    }
    let file = workers::Wrangler {
        app: "Acme Ops",
        deployable: "back_office",
        required: &["MAIL_SENDER".to_string()],
        hyperdrive: &[
            workers::Hyperdrive {
                binding: "DATABASE_URL".to_string(),
                id: Some("c020574a5623407bbe0ccd192bab9545".to_string()),
            },
            workers::Hyperdrive {
                binding: "LEDGER_URL".to_string(),
                id: None,
            },
        ],
        ticks: true,
        public_identity: false,
        rate_limits: &[],
        queues: &[],
    };
    let written = workers::wrangler(&file);
    let wrangler: serde_json::Value = serde_json::from_str(&written).expect("JSON");
    assert_eq!(
        wrangler,
        serde_json::json!({
            // The app's name and the deployable's (chunk 8.4f), so two
            // apps' `back_office`s are two Workers in one account.
            "name": "acme-ops-back-office",
            "main": "worker.mjs",
            "compatibility_date": workers::COMPATIBILITY_DATE,
            "no_bundle": true,
            "find_additional_modules": true,
            "rules": [
                { "type": "ESModule", "globs": ["**/*.mjs", "**/*.js"] },
                // Wrangler warns of a rule that hides its own default
                // without saying so (chunk 8.2k).
                { "type": "CompiledWasm", "globs": ["**/*.wasm"], "fallthrough": false },
            ],
            // Workers Logs keeps what the Worker logs only when it is
            // enabled, which Wrangler leaves off unless the file says so
            // (chunk 8.4f).
            "observability": { "enabled": true },
            "secrets": { "required": ["MAIL_SENDER"] },
            // Each database's address is a Hyperdrive binding (chunk
            // 8.2i), with the id the build was given; without one, Wrangler
            // refuses to deploy until it has one.
            "hyperdrive": [
                { "binding": "DATABASE_URL", "id": "c020574a5623407bbe0ccd192bab9545" },
                { "binding": "LEDGER_URL" },
            ],
            "triggers": { "crons": ["* * * * *"] },
            // No binding to itself by name (chunk 8.4b): work is handed on
            // through the loopback `ctx.exports` gives `JadpoHandOn`, which
            // reaches the Worker that handed it on whatever name or
            // environment `wrangler deploy --name` or `--env` gave it,
            // where a service binding would still name `back-office`.
        })
    );
    // Written for a person to read: its name first, a short entry on one
    // line, the same file every time.
    assert!(
        written
            .starts_with("{\n  \"name\": \"acme-ops-back-office\",\n  \"main\": \"worker.mjs\",\n"),
        "{written}"
    );
    assert!(
        written.contains(
            "\n    { \"type\": \"ESModule\", \"globs\": [\"**/*.mjs\", \"**/*.js\"] },\n"
        ),
        "{written}"
    );
    assert!(written.ends_with("\n}\n"), "{written}");
    assert_eq!(workers::wrangler(&file), written);
    // With no setting to give, there is no secret to require and no
    // binding to name; with no store for its tick to keep or sweep, no
    // cron to run it.
    let bare: serde_json::Value = serde_json::from_str(&workers::wrangler(&workers::Wrangler {
        app: "web",
        deployable: "web",
        required: &[],
        hyperdrive: &[],
        ticks: false,
        public_identity: false,
        rate_limits: &[],
        queues: &[],
    }))
    .expect("JSON");
    assert_eq!(bare["name"], "web-web");
    assert_eq!(
        bare["observability"],
        serde_json::json!({ "enabled": true })
    );
    for absent in [
        "secrets",
        "hyperdrive",
        "triggers",
        "services",
        "ratelimits",
        "queues",
    ] {
        assert!(bare.get(absent).is_none(), "{absent} in {bare}");
    }
}

/// A Worker's name on Cloudflare is its app's and its deployable's, in the
/// letters, digits and dashes Cloudflare takes, at most 63 of them, the
/// deployable's kept whole (chunk 8.4f).
#[test]
fn a_worker_is_named_after_its_app_and_its_deployable() {
    for (app, deployable, name) in [
        ("23-repair-cafe", "kiosk", "23-repair-cafe-kiosk"),
        ("Repair Café", "back_office", "repair-caf-back-office"),
        ("--acme__ops..", "web", "acme-ops-web"),
        // A project with no name of its own, as a test's.
        ("", "kiosk", "kiosk"),
        ("Ω", "kiosk", "kiosk"),
        (
            "the-very-long-name-of-a-community-repair-cafe-in-a-market-town",
            "back_office",
            "the-very-long-name-of-a-community-repair-cafe-in-a-back-office",
        ),
    ] {
        let named = workers::worker_name(app, deployable);
        assert_eq!(named, name, "{app:?} and {deployable:?}");
        assert!(named.len() <= 63, "{named}");
        assert!(!named.starts_with('-') && !named.ends_with('-'), "{named}");
        assert!(
            named
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "{named}"
        );
    }
}

/// Each limit counted in Cloudflare's rate-limiting binding is a binding
/// of the Worker's own in its Wrangler file, its namespace id as text and
/// its window in seconds (chunk 8.2m2).
#[test]
fn a_worker_is_given_a_rate_limiting_binding_for_each_limit() {
    let bindings =
        jadpo_std::http::rate_limit_bindings("limits", 1001, &jadpo_std::http::COUNTED_LIMITS)
            .expect("numbered");
    let wrangler: serde_json::Value =
        serde_json::from_str(&workers::wrangler(&workers::Wrangler {
            app: "shop",
            deployable: "web",
            required: &["DATABASE_URL".to_string()],
            hyperdrive: &[],
            ticks: true,
            public_identity: false,
            rate_limits: &bindings,
            queues: &[],
        }))
        .expect("JSON");
    assert_eq!(
        wrangler["ratelimits"],
        serde_json::json!([
            {
                "name": "LIMITS_120_PER_60S",
                "namespace_id": "1001",
                "simple": { "limit": 120, "period": 60 },
            },
            {
                "name": "LIMITS_600_PER_60S",
                "namespace_id": "1002",
                "simple": { "limit": 600, "period": 60 },
            },
        ])
    );
    assert_eq!(
        wrangler["secrets"]["required"],
        serde_json::json!(["DATABASE_URL"])
    );
    assert_eq!(
        wrangler["triggers"]["crons"],
        serde_json::json!(["* * * * *"])
    );
}

/// Each queue on Cloudflare Queues the Worker receives is a consumer in its
/// Wrangler file, by Wrangler's own setting names (chunk 8.2k), and so is
/// its dead-letter queue (chunk 8.4b); a package's queue, fetched on the
/// tick, is none.
#[test]
fn a_worker_consumes_each_cloudflare_queue_it_receives() {
    use jadpo_std::receiving::{CloudflareQueue, QueuePlan, QueueSource};
    let queue = |connection: &str, name: &str| QueuePlan {
        connection: connection.to_string(),
        type_at: None,
        id_at: "id".to_string(),
        events: Vec::new(),
        store: "main".to_string(),
        source: QueueSource::Cloudflare(CloudflareQueue {
            queue: name.to_string(),
            dead_letter_queue: format!("{name}-dead"),
        }),
    };
    let bindings =
        jadpo_std::http::rate_limit_bindings("limits", 1001, &jadpo_std::http::COUNTED_LIMITS)
            .expect("numbered");
    let file = workers::Wrangler {
        app: "depot",
        deployable: "depot",
        required: &[],
        hyperdrive: &[],
        ticks: true,
        public_identity: false,
        rate_limits: &bindings,
        queues: &[queue("warehouse_queue", "warehouse-events")],
    };
    let written = workers::wrangler(&file);
    let wrangler: serde_json::Value = serde_json::from_str(&written).expect("JSON");
    assert_eq!(
        wrangler["queues"],
        serde_json::json!({
            "consumers": [
                {
                    "queue": "warehouse-events",
                    "max_batch_size": 10,
                    "max_batch_timeout": 5,
                    "max_retries": 30,
                    "dead_letter_queue": "warehouse-events-dead",
                },
                // Its dead-letter queue's consumer parks what reaches it
                // (chunk 8.4b).
                {
                    "queue": "warehouse-events-dead",
                    "max_batch_size": 10,
                    "max_batch_timeout": 5,
                    "max_retries": 100,
                },
            ]
        })
    );
    // Every binding beside it, none dropped for another (chunk 8.4f).
    assert_eq!(
        wrangler["triggers"]["crons"],
        serde_json::json!(["* * * * *"])
    );
    assert_eq!(wrangler["ratelimits"].as_array().map(Vec::len), Some(2));
    // Each consumer its queue first.
    assert!(
        written.contains("{\n        \"queue\": \"warehouse-events\",\n"),
        "{written}"
    );
    // With no queue on Cloudflare Queues, no consumers (a package's queue,
    // fetched on the tick, is none: `jadpo_std::receiving`'s own test).
    let none: serde_json::Value = serde_json::from_str(&workers::wrangler(&workers::Wrangler {
        queues: &[],
        ..file
    }))
    .expect("JSON");
    assert!(none.get("queues").is_none(), "{none}");
    assert_eq!(none["ratelimits"].as_array().map(Vec::len), Some(2));
}

#[test]
fn external_identity_fetches_are_strictly_public_on_workers() {
    let written = workers::wrangler(&workers::Wrangler {
        app: "identity",
        deployable: "web",
        required: &[],
        hyperdrive: &[],
        ticks: false,
        public_identity: true,
        rate_limits: &[],
        queues: &[],
    });
    let config: serde_json::Value = serde_json::from_str(&written).expect("JSON");
    assert_eq!(
        config["compatibility_flags"],
        serde_json::json!(["global_fetch_strictly_public"])
    );
}
