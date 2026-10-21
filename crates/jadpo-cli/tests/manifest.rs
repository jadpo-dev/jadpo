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

#[test]
fn the_same_source_writes_the_same_bytes_in_any_order() {
    let first = project("order-a", FILES);
    let mut reversed: Vec<(&str, &str)> = FILES.to_vec();
    reversed.reverse();
    let second = project("order-b", &reversed);
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
