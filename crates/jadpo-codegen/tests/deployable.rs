//! A native deployable's workspace (roadmap chunk 8.1o;
//! docs/compiler-runtime.md §2.6, "A native deployable", and §2.8): the
//! program's crate, a binary named after the deployable whose `main` hands
//! it, its build stamp and each store's migrations to the native host, the
//! toolchain pinned and the runtime crates' lock beside them, the CLDR
//! data it carries in `cldr/` (chunk 8.2c), the same files every time.

use std::collections::BTreeMap;

use jadpo_codegen::deployable::{Deployable, Stamp, workspace};
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

/// The CLDR data a deployable carries, as `jadpo_cldr::export` gives it.
fn cldr() -> &'static BTreeMap<String, Vec<u8>> {
    static CLDR: std::sync::OnceLock<BTreeMap<String, Vec<u8>>> = std::sync::OnceLock::new();
    CLDR.get_or_init(|| {
        BTreeMap::from([
            ("mod.rs".to_string(), b"include!(\"x.rs.data\");\n".to_vec()),
            ("x.rs.data".to_string(), b"// the data\n".to_vec()),
        ])
    })
}

fn deployable(stores: &BTreeMap<String, Vec<(String, String)>>) -> Deployable<'_> {
    Deployable {
        name: "kiosk",
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
        cldr: cldr(),
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
    // The CLDR data it carries, in `cldr/`, which its build names to
    // ICU4X's data crates.
    assert_eq!(text(files, "cldr/mod.rs"), "include!(\"x.rs.data\");\n");
    assert_eq!(text(files, "cldr/x.rs.data"), "// the data\n");
    // The program, as `generate` writes it, holding the plan's bytes
    // without the dev and test section.
    let program = text(files, "program/Cargo.toml");
    assert!(program.contains("name = \"jadpo-program\""), "{program}");
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
        binary.contains("jadpo-program = { path = \"../program\" }"),
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
