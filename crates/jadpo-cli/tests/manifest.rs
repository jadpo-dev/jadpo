//! What `jadpo check` writes to `build/` (areas 14 and 18): the manifest
//! and the catalogues, for a project only, errors or not, and the same
//! bytes however often and in whatever order the files were written.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

const FILES: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "domains/notes/types/body.jadpo",
        "/// The text of a note.\ntype Body = Text {\n    min_length: 1\n}\n",
    ),
    (
        "domains/notes/failures/note_refused.jadpo",
        "type NoteRefused = Failure {\n    kind: FailureKind.rejected\n    client: {\n        length: Int\n    }\n}\n",
    ),
    (
        "domains/notes/libraries/wording.jadpo",
        "use notes/types { Body }\n\nlibrary wording {\n    /// Keeps a body as it is.\n    function keep(body: Body) -> Body {\n        return body\n    }\n}\n",
    ),
    (
        "domains/notes/actions/save_note.jadpo",
        "use notes/types { Body }\nuse notes/failures { NoteRefused }\nuse notes/libraries { wording }\n\naction save_note(body: Body) fails NoteRefused -> Body {\n    if (body == Body(\"no\")) {\n        reject NoteRefused {\n            length: 2\n        }\n    }\n    return wording.keep(body)\n}\n",
    ),
];

const DERIVED: &[&str] = &[
    "build/manifest.json",
    "build/catalogue/libraries.md",
    "build/catalogue/libraries.json",
    "build/catalogue/failures.md",
    "build/catalogue/failures.json",
    "build/report/permissions.md",
    "build/report/permissions.json",
    "build/report/network.md",
    "build/report/network.json",
];

use crate::support;

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("manifest-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temporary folder");
    dir
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = folder(name);
    for (path, text) in files {
        write(&root, path, text);
    }
    root
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut output = run(&args, &Environment::empty().with_cwd(cwd));
    output.stdout = support::without_run_time(&output.stdout);
    output
}

fn read(root: &Path, path: &str) -> String {
    fs::read_to_string(root.join(path)).unwrap_or_else(|_| panic!("{path} was written"))
}

#[test]
fn check_writes_the_manifest_and_the_catalogues() {
    let root = project("writes", FILES);
    let output = jadpo(&root, &["check"]);
    assert_eq!(
        output.stdout, "checked: 5 files\n",
        "a passing run is one line"
    );
    for path in DERIVED {
        assert!(root.join(path).is_file(), "{path}");
    }
    let manifest: serde_json::Value =
        serde_json::from_str(&read(&root, "build/manifest.json")).expect("JSON");
    assert_eq!(manifest["format"], 2);
    assert_eq!(manifest["errors"], 0);
    let ids: Vec<&str> = manifest["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .map(|node| node["id"].as_str().expect("id"))
        .collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "nodes are sorted by id");
    let save = manifest["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["id"] == "action save_note")
        .expect("the action");
    assert_eq!(save["file"], "domains/notes/actions/save_note.jadpo");
    assert_eq!(save["lines"], serde_json::json!([5, 12]));
    assert_eq!(save["calls"], serde_json::json!(["function wording.keep"]));
    assert_eq!(
        save["raises"],
        serde_json::json!(["type notes.NoteRefused"])
    );
    let libraries = read(&root, "build/catalogue/libraries.md");
    assert!(libraries.contains("### wording.keep\n\n```jadpo\nfunction keep(body: Body) -> Body\n```\n\nKeeps a body as it is."), "{libraries}");
    assert!(
        libraries.contains("Called by `save_note` (domains/notes/actions/save_note.jadpo:5)."),
        "{libraries}"
    );
    let failures = read(&root, "build/catalogue/failures.md");
    assert!(failures.contains("## note_refused\n\n`NoteRefused` · rejected · 422 · `domains/notes/failures/note_refused.jadpo:1`"), "{failures}");
    assert!(
        failures.contains("- client fields: `length: Int`"),
        "{failures}"
    );
}

/// The project of `files` in a folder called `cafe` inside one called
/// `name`: the application's name is its folder's, which its build holds.
fn named_project(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = folder(name).join("cafe");
    for (path, text) in files {
        write(&root, path, text);
    }
    root
}

#[test]
fn the_same_source_writes_the_same_bytes_in_any_order() {
    let first = named_project("order-a", FILES);
    let mut reversed: Vec<(&str, &str)> = FILES.to_vec();
    reversed.reverse();
    let second = named_project("order-b", &reversed);
    jadpo(&first, &["check"]);
    jadpo(&second, &["check"]);
    let modified = fs::metadata(first.join("build/manifest.json"))
        .and_then(|m| m.modified())
        .expect("time");
    jadpo(&first, &["check"]);
    for path in DERIVED {
        assert_eq!(read(&first, path), read(&second, path), "{path}");
    }
    assert_eq!(
        fs::metadata(first.join("build/manifest.json"))
            .and_then(|m| m.modified())
            .expect("time"),
        modified,
        "unchanged bytes are not rewritten"
    );
}

#[test]
fn a_project_with_errors_still_writes_and_says_so() {
    let mut files: Vec<(&str, &str)> = FILES.to_vec();
    files.push((
        "domains/notes/actions/broken.jadpo",
        "use notes/types { Body }\n\naction broken(body: Body) -> Body {\n    return missing(body)\n}\n",
    ));
    let root = project("errors", &files);
    let output = jadpo(&root, &["check", "--json"]);
    assert_eq!(output.code, 1);
    let document: serde_json::Value = serde_json::from_str(&output.stdout).expect("JSON");
    assert_eq!(
        document["data"]["build"],
        serde_json::json!(DERIVED),
        "the files written are listed for tools"
    );
    let manifest: serde_json::Value =
        serde_json::from_str(&read(&root, "build/manifest.json")).expect("JSON");
    assert_eq!(manifest["errors"], 1);
}

#[test]
fn loose_files_write_nothing() {
    let root = folder("loose");
    write(
        &root,
        "notes/sketch.jadpo",
        "type Count = Int {\n    min: 0\n}\n",
    );
    let output = jadpo(&root, &["check", "notes", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&output.stdout).expect("JSON");
    assert_eq!(document["data"]["build"], serde_json::json!([]));
    assert!(!root.join("notes/build").exists() && !root.join("build").exists());
}

#[test]
fn help_json_documents_the_manifest_schema() {
    let output = jadpo(Path::new("."), &["help", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&output.stdout).expect("JSON");
    let schema = &document["data"]["manifest"];
    assert_eq!(schema["file"], "build/manifest.json");
    assert_eq!(schema["format"], 2);
    let kinds = schema["nodeKinds"].as_array().expect("kinds");
    assert!(
        kinds
            .iter()
            .any(|k| k["kind"] == "action" && k["status"] == "built")
    );
    assert!(
        kinds
            .iter()
            .any(|k| k["kind"] == "grant" && k["status"] == "built")
    );
    assert!(
        kinds
            .iter()
            .any(|k| k["kind"] == "oauth_scope" && k["status"] == "reserved")
    );
    assert!(schema["nodeFields"]["digest"].is_string());
}

/// A project git doesn't track: the manifest says so, with no commit; and
/// each deployable's target and build id (area 18; chunk 8.1o2).
#[test]
fn the_manifest_names_the_source_and_each_deployable_with_its_build_id() {
    let root = project("stamp", FILES);
    jadpo(&root, &["check"]);
    let manifest: serde_json::Value =
        serde_json::from_str(&read(&root, "build/manifest.json")).expect("JSON");
    // The project is in a folder git ignores, so no commit explains it.
    assert_eq!(
        manifest["source"],
        serde_json::json!({ "modified": true }),
        "{manifest}"
    );
    let app = &manifest["deployables"]["app"];
    assert_eq!(app["target"], "native", "{manifest}");
    let id = app["build"].as_str().expect("a build id");
    assert!(
        id.len() == 14 && id.starts_with("b_") && id[2..].chars().all(|c| c.is_ascii_hexdigit()),
        "{id}"
    );

    // Declared, each has an id of its own; the target is what `jadpo
    // build` builds it for.
    let declared = "application {\n    connections: {\n        limits: {\n            driver: Driver.cloudflare_rate_limits\n            namespace: 1001\n        }\n    }\n    rate_limits: {\n        store: limits\n    }\n    deployables: {\n        api: {\n            serves: [main]\n            target: Target.wasm\n        }\n        worker: {\n            serves: [main]\n        }\n    }\n}\n";
    write(&root, "app.jadpo", declared);
    let output = jadpo(&root, &["check"]);
    assert_eq!(output.code, 0, "{}{}", output.stdout, output.stderr);
    let manifest: serde_json::Value =
        serde_json::from_str(&read(&root, "build/manifest.json")).expect("JSON");
    let deployables = manifest["deployables"].as_object().expect("deployables");
    assert_eq!(
        deployables.keys().collect::<Vec<_>>(),
        ["api", "worker"],
        "{manifest}"
    );
    assert_eq!(deployables["api"]["target"], "wasm");
    assert_eq!(deployables["worker"]["target"], "native");
    assert_ne!(deployables["api"]["build"], deployables["worker"]["build"]);
    assert_ne!(deployables["worker"]["build"], id);
}

/// A kiosk serving `main` and an office serving `staff`, each greeting
/// with a setting of its own.
const DESK: &[(&str, &str)] = &[
    (
        "app.jadpo",
        "application {\n    apis: {\n        main: {\n            prefix: /\n        }\n        staff: {\n            prefix: /staff\n        }\n    }\n    deployables: {\n        kiosk: {\n            serves: [main]\n        }\n        office: {\n            serves: [staff]\n        }\n    }\n}\n",
    ),
    (
        "config.jadpo",
        "use desk/types { Line }\n\nconfig {\n    greeting: Line {\n        default: \"Welcome\"\n    }\n    heading: Line {\n        default: \"Staff\"\n    }\n}\n",
    ),
    (
        "policy.jadpo",
        "use desk\n\npolicy {\n    actions: {\n        desk.greet: [Access.public]\n        desk.head: [Access.public]\n    }\n}\n",
    ),
    (
        "domains/desk/types/line.jadpo",
        "/// A line of text.\ntype Line = Text {\n    max_length: 40\n}\n",
    ),
    (
        "domains/desk/types/said.jadpo",
        "use desk/types { Line }\n\n/// What is said.\ntype Said = Output {\n    line: Line\n}\n",
    ),
    (
        "domains/desk/actions/greet.jadpo",
        "use desk/types { Said }\n\n/// Greets a visitor.\naction greet() -> Said {\n    return Said {\n        line: config.greeting\n    }\n}\n",
    ),
    (
        "domains/desk/actions/head.jadpo",
        "use desk/types { Said }\n\n/// Heads the staff's page.\naction head() -> Said {\n    return Said {\n        line: config.heading\n    }\n}\n",
    ),
    (
        "domains/desk/routes/front.jadpo",
        "use desk/types { Said }\nuse desk/actions { greet }\n\n/// The front desk.\nroutes Front {\n    prefix: /front\n\n    /// A greeting.\n    POST / {\n        authentication: RouteAuthentication.none\n        output: Said\n        run: greet()\n    }\n}\n",
    ),
    (
        "domains/desk/routes/staff.jadpo",
        "use desk/types { Said }\nuse desk/actions { head }\n\n/// The staff's page.\nroutes Staff {\n    prefix: /staff\n    api: staff\n\n    /// Its heading.\n    POST / {\n        authentication: RouteAuthentication.none\n        output: Said\n        run: head()\n    }\n}\n",
    ),
];

/// Each deployable's build id, by name, as `jadpo check` writes it after
/// `jadpo api update`, which keeps the APIs' records up to date.
fn build_ids(root: &Path) -> std::collections::BTreeMap<String, Option<String>> {
    jadpo(root, &["api", "update"]);
    let output = jadpo(root, &["check"]);
    let manifest: serde_json::Value =
        serde_json::from_str(&read(root, "build/manifest.json")).expect("JSON");
    let deployables = manifest["deployables"].as_object().expect("deployables");
    assert!(
        output.code == 0
            || deployables
                .values()
                .all(|entry| entry.get("build").is_none()),
        "{}",
        output.stdout
    );
    deployables
        .iter()
        .map(|(name, entry)| (name.clone(), entry["build"].as_str().map(str::to_string)))
        .collect()
}

/// A deployable's build id digests what it reaches (area 18, "A build id
/// is a digest"; chunk 8.1o3): a change to what only another deployable
/// reaches, or to a test, leaves it alone; a change to what it reaches
/// doesn't. A project with errors builds nothing, so has none.
#[test]
fn a_change_a_deployable_doesnt_reach_leaves_its_build_id_alone() {
    let root = project("reached", DESK);
    let first = build_ids(&root);
    let (kiosk, office) = (first["kiosk"].clone(), first["office"].clone());
    assert!(kiosk.is_some() && office.is_some(), "{first:?}");
    assert_ne!(kiosk, office);

    // The office's action, its route, a setting only it reads and a test:
    // the kiosk's id stays.
    write(
        &root,
        "domains/desk/actions/head.jadpo",
        "use desk/types { Line, Said }\n\n/// Heads the staff's page, from its heading.\naction head() -> Said {\n    var heading: Line = config.heading\n    return Said {\n        line: heading\n    }\n}\n",
    );
    let changed = build_ids(&root);
    assert_eq!(changed["kiosk"], kiosk, "the office's action");
    assert_ne!(changed["office"], office, "the office's action");
    let office = changed["office"].clone();
    write(
        &root,
        "domains/desk/routes/staff.jadpo",
        "use desk/types { Said }\nuse desk/actions { head }\n\n/// The staff's page.\nroutes Staff {\n    prefix: /staff\n    api: staff\n\n    /// Its heading.\n    POST / {\n        authentication: RouteAuthentication.none\n        output: Said\n        run: head()\n    }\n\n    /// Its heading again.\n    POST /again {\n        authentication: RouteAuthentication.none\n        output: Said\n        run: head()\n    }\n}\n",
    );
    let changed = build_ids(&root);
    assert_eq!(changed["kiosk"], kiosk, "the office's route");
    assert_ne!(changed["office"], office, "the office's route");
    let office = changed["office"].clone();
    write(
        &root,
        "config.jadpo",
        "use desk/types { Line }\n\nconfig {\n    greeting: Line {\n        default: \"Welcome\"\n    }\n    heading: Line {\n        default: \"Staff only\"\n    }\n}\n",
    );
    let changed = build_ids(&root);
    assert_eq!(changed["kiosk"], kiosk, "the office's setting");
    assert_ne!(changed["office"], office, "the office's setting");
    let office = changed["office"].clone();
    write(
        &root,
        "domains/desk/actions/greet.test.jadpo",
        "use desk/types { Said }\n\ntest \"a visitor is welcomed\" {\n    var said: Said = greet()\n    assert said.line == \"Welcome\"\n}\n",
    );
    let changed = build_ids(&root);
    assert_eq!(changed["kiosk"], kiosk, "a test");
    assert_eq!(changed["office"], office, "a test");

    // The kiosk's own action: its id changes, the office's stays.
    write(
        &root,
        "domains/desk/actions/greet.jadpo",
        "use desk/types { Line, Said }\n\n/// Greets a visitor, from its greeting.\naction greet() -> Said {\n    var greeting: Line = config.greeting\n    return Said {\n        line: greeting\n    }\n}\n",
    );
    let changed = build_ids(&root);
    assert_ne!(changed["kiosk"], kiosk, "the kiosk's action");
    assert_eq!(changed["office"], office, "the kiosk's action");

    // With an error, nothing builds, so nothing has a build id.
    write(
        &root,
        "domains/desk/actions/greet.jadpo",
        "use desk/types { Said }\n\naction greet() -> Said {\n    return Said {\n        line: config.nothing\n    }\n}\n",
    );
    let broken = build_ids(&root);
    assert_eq!(broken["kiosk"], None, "{broken:?}");
    assert_eq!(broken["office"], None, "{broken:?}");
}
