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
    let lib = text(files, "deployable/src/lib.rs");
    syn::parse_file(&lib).expect("Rust");
    for held in [
        "wasm_bindgen = jadpo_host_workers::wasm_bindgen",
        "pub fn fetch(request: JsValue, env: JsValue, ctx: JsValue) -> JsValue",
        "jadpo_host_workers::deployable::fetch(&DEPLOYABLE, request, env, ctx)",
        "pub fn hand_on(work: String, env: JsValue, ctx: JsValue) -> JsValue",
        "jadpo_host_workers::deployable::hand_on(&DEPLOYABLE, work, env, ctx)",
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
        "import { initSync, fetch as handleFetch, hand_on as handleHandOn } from \"./back_office.js\";",
        "initSync({ module });",
        "export default {",
        "return handleFetch(request, env, ctx);",
        // Work handed on reaches a named entrypoint, never the default
        // export a request from outside reaches.
        "import { WorkerEntrypoint } from \"cloudflare:workers\";",
        "export class JadpoHandOn extends WorkerEntrypoint {",
        "handOn(work) {",
        "return handleHandOn(work, this.env, this.ctx);",
    ] {
        assert!(shim.contains(held), "{held} isn't in:\n{shim}");
    }
    let wrangler: serde_json::Value = serde_json::from_str(&workers::wrangler(
        "back_office",
        &["DATABASE_URL".to_string(), "MAIL_SENDER".to_string()],
        true,
    ))
    .expect("JSON");
    assert_eq!(
        wrangler,
        serde_json::json!({
            "name": "back-office",
            "main": "worker.mjs",
            "compatibility_date": workers::COMPATIBILITY_DATE,
            "no_bundle": true,
            "find_additional_modules": true,
            "rules": [
                { "type": "ESModule", "globs": ["**/*.mjs", "**/*.js"] },
                { "type": "CompiledWasm", "globs": ["**/*.wasm"] },
            ],
            "secrets": { "required": ["DATABASE_URL", "MAIL_SENDER"] },
            // Bound to itself, to hand work on to a fresh invocation.
            "services": [
                { "binding": "JADPO_HAND_ON", "service": "back-office", "entrypoint": "JadpoHandOn" },
            ],
        })
    );
    // With no setting to give, there is no secret to require; running no
    // background work, it hands nothing on.
    let bare: serde_json::Value =
        serde_json::from_str(&workers::wrangler("web", &[], false)).expect("JSON");
    assert_eq!(bare["name"], "web");
    assert!(bare.get("secrets").is_none(), "{bare}");
    assert!(bare.get("services").is_none(), "{bare}");
}
