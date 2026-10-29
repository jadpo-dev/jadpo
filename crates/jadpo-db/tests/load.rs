//! Finding and reading a project: walking up to `app.jadpo`, the files it
//! holds in path order, and loose files outside any project.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_db::{FileSet, Layout, item_tree, load, load_files, symbols};

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("load-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temporary folder");
    dir
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

#[test]
fn a_project_is_found_from_any_file_inside_it() {
    let root = folder("project");
    write(
        &root,
        "app.jadpo",
        "application {\n    connections: {\n        mail: {\n            package: Postbox.v1\n        }\n    }\n}\n",
    );
    write(
        &root,
        "domains/planning/types/count.jadpo",
        "public type Count = Int {}\n",
    );
    write(
        &root,
        "domains/planning/emails/welcome.html",
        "<p>Hello</p>\n",
    );
    write(
        &root,
        "shared/libraries/maths.jadpo",
        "library maths {\n    function same(x: Count) -> Count {\n        return x\n    }\n}\n",
    );
    write(&root, "build/stale.jadpo", "type Stale = Int {}\n");
    write(&root, ".editor/scratch.jadpo", "type Scratch = Int {}\n");
    let loaded = load(&root.join("domains/planning/types/count.jadpo"));
    let db = &loaded.db;
    assert_eq!(loaded.root, root);
    assert_eq!(loaded.project.layout(db), Layout::Project);
    let paths: Vec<&String> = loaded
        .project
        .files(db)
        .iter()
        .map(|f| f.path(db))
        .collect();
    assert_eq!(
        paths,
        [
            "app.jadpo",
            "domains/planning/types/count.jadpo",
            "shared/libraries/maths.jadpo"
        ]
    );
    // An email's template is read whole, as a record (area 16).
    assert!(loaded.project.other_files(db).is_empty());
    assert!(
        loaded
            .project
            .records(db)
            .iter()
            .any(|record| record.path == "domains/planning/emails/welcome.html")
    );
    let table = symbols(db, loaded.project);
    assert!(table.domains.contains("planning"));
    assert!(table.connections.contains("mail") && table.packages.contains("Postbox"));
    assert_eq!(
        table.library_function("maths", "same").map(|s| s.index),
        Some(0)
    );
    let count = loaded.project.files(db)[1];
    assert!(item_tree(db, count).items[0].public);
}

#[test]
fn outside_a_project_files_are_loose() {
    let root = folder("loose");
    write(&root, "a.jadpo", "type A = Int {}\n");
    write(&root, "nested/b.jadpo", "type B = Int {}\n");
    let one = load(&root.join("nested/b.jadpo"));
    assert_eq!(one.project.layout(&one.db), Layout::Loose);
    assert_eq!(one.project.files(&one.db).len(), 1);
    let all = load(&root);
    let paths: Vec<&String> = all
        .project
        .files(&all.db)
        .iter()
        .map(|f| f.path(&all.db))
        .collect();
    assert_eq!(paths, ["a.jadpo", "nested/b.jadpo"]);
}

#[test]
fn a_loaded_project_takes_edits_in_place_and_keeps_path_order() {
    let root = folder("edits");
    write(&root, "app.jadpo", "application {}\n");
    write(
        &root,
        "domains/planning/types/count.jadpo",
        "type Count = Int {}\n",
    );
    let mut loaded = load(&root);
    let paths = |loaded: &jadpo_db::Loaded| -> Vec<String> {
        loaded
            .project
            .files(&loaded.db)
            .iter()
            .map(|file| file.path(&loaded.db).clone())
            .collect()
    };
    assert_eq!(
        loaded.relative(&root.join("domains/planning/types/count.jadpo")),
        Some("domains/planning/types/count.jadpo".to_string())
    );
    assert_eq!(loaded.relative(Path::new("/elsewhere/x.jadpo")), None);

    loaded.set_text(
        "domains/planning/types/count.jadpo",
        "type Total = Int {}\n".to_string(),
    );
    let count = loaded
        .file("domains/planning/types/count.jadpo")
        .expect("file");
    assert_eq!(count.text(&loaded.db), "type Total = Int {}\n");

    // A new file joins in path order; a non-source path is ignored.
    loaded.set_text(
        "domains/billing/types/fee.jadpo",
        "type Fee = Int {}\n".into(),
    );
    loaded.set_text("notes.md", "not source".into());
    loaded.set_text("build/stale.jadpo", "type Stale = Int {}\n".into());
    assert_eq!(
        paths(&loaded),
        [
            "app.jadpo",
            "domains/billing/types/fee.jadpo",
            "domains/planning/types/count.jadpo"
        ]
    );
    assert!(symbols(&loaded.db, loaded.project).get("Fee").is_some());

    // Reloading reads the disk: the edit is gone, a deleted file goes.
    loaded.reload("domains/planning/types/count.jadpo");
    assert_eq!(count.text(&loaded.db), "type Count = Int {}\n");
    loaded.reload("domains/billing/types/fee.jadpo");
    assert_eq!(
        paths(&loaded),
        ["app.jadpo", "domains/planning/types/count.jadpo"]
    );
    assert!(symbols(&loaded.db, loaded.project).get("Fee").is_none());
}

/// What a load puts in the database, and beside it, for comparing two.
fn contents(loaded: &jadpo_db::Loaded) -> String {
    let db = &loaded.db;
    let project = loaded.project;
    let texts = |files: &[jadpo_db::File]| -> Vec<(String, String)> {
        files
            .iter()
            .map(|file| (file.path(db).clone(), file.text(db).clone()))
            .collect()
    };
    format!(
        "{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}",
        project.layout(db),
        project.name(db),
        texts(project.files(db)),
        texts(project.lazy(db)),
        project.other_files(db),
        project.records(db),
        project.identities(db),
        loaded.unreadable,
        loaded.links,
    )
}

/// A project with every kind of file the rules sort: source, records, a
/// template, an asset, the identity registry under its old name, an
/// installed package with a resource the app doesn't call yet, and a
/// package linked to a checkout beside the project.
const PROJECT: &[(&str, &str)] = &[
    ("app.jadpo", "application {}\n"),
    (
        "domains/planning/types/count.jadpo",
        "type Total = Int {}\n",
    ),
    ("domains/planning/emails/welcome.html", "<p>Hello</p>\n"),
    ("domains/planning/images/logo.svg", "<svg/>\n"),
    ("database/identities.json", "{\"type Total\": \"Count\"}\n"),
    (
        "packages/postcodes/v1/package.jadpo",
        "package Postcodes {}\n",
    ),
    ("packages/postcodes/v1/README.md", "# Postcodes\n"),
    (
        "packages/postcodes/v1/resources/lookup.jadpo",
        "resource lookup {}\n",
    ),
    (
        "packages/couriers/v2/package.jadpo",
        "package Couriers {}\n",
    ),
    ("packages/couriers/v2/old.jadpo", "type Old = Int {}\n"),
];

const CHECKOUT: &[(&str, &str)] = &[
    ("package.jadpo", "package Couriers {}\n"),
    ("new.jadpo", "type New = Int {}\n"),
    ("notes.md", "Working copy\n"),
];

fn lock(checkout: &str) -> String {
    jadpo_db::write_lock(&[jadpo_db::Locked {
        package: "Couriers.v2".to_string(),
        source: jadpo_db::Source::Link {
            path: checkout.to_string(),
        },
    }])
}

#[test]
fn a_project_loads_from_files_in_memory_as_from_disk() {
    let dir = folder("memory");
    let root = dir.join("shop");
    for (path, text) in PROJECT {
        write(&root, path, text);
    }
    write(&root, "jadpo.lock", &lock("../couriers-dev"));
    for (path, text) in CHECKOUT {
        write(&dir, &format!("couriers-dev/{path}"), text);
    }
    let from_disk = load(&root);

    let mut files = FileSet::of(PROJECT.iter().copied());
    files.insert("jadpo.lock", Ok(lock("../couriers-dev")));
    for (path, text) in CHECKOUT {
        files.insert(format!("../couriers-dev/{path}"), Ok(text.to_string()));
    }
    let in_memory = load_files(root.clone(), &files);
    assert_eq!(contents(&in_memory), contents(&from_disk));

    // The rules sorted them: the linked checkout's files are read in the
    // package's folder, the installed copy's own left out, the resource
    // waits, and the registry's line is read.
    let db = &in_memory.db;
    let paths: Vec<&String> = in_memory
        .project
        .files(db)
        .iter()
        .map(|file| file.path(db))
        .collect();
    assert_eq!(
        paths,
        [
            "app.jadpo",
            "domains/planning/types/count.jadpo",
            "packages/couriers/v2/new.jadpo",
            "packages/couriers/v2/package.jadpo",
            "packages/postcodes/v1/package.jadpo",
        ]
    );
    assert_eq!(in_memory.project.lazy(db).len(), 1);
    assert_eq!(
        in_memory.project.other_files(db),
        &["domains/planning/images/logo.svg"]
    );
    assert_eq!(
        in_memory.project.identities(db).get("type Total"),
        Some(&"Count".to_string())
    );
    assert_eq!(
        in_memory.links,
        [(
            "packages/couriers/v2".to_string(),
            "../couriers-dev".to_string()
        )]
    );
}

#[test]
fn files_a_project_does_not_hold_are_left_out_of_a_set() {
    let mut files = FileSet::of(PROJECT.iter().copied());
    let plain = load_files(PathBuf::from("shop"), &files);
    for (path, text) in [
        ("README.md", "# Shop\n"),
        (".editor/scratch.jadpo", "type Scratch = Int {}\n"),
        ("domains/planning/.hidden.jadpo", "type Hidden = Int {}\n"),
        ("build/stale.jadpo", "type Stale = Int {}\n"),
        ("../elsewhere.jadpo", "type Elsewhere = Int {}\n"),
        ("/absolute.jadpo", "type Absolute = Int {}\n"),
    ] {
        files.insert(path, Ok(text.to_string()));
    }
    files.insert("build/", Err("permission denied".to_string()));
    assert_eq!(
        contents(&load_files(PathBuf::from("shop"), &files)),
        contents(&plain)
    );
}

#[test]
fn what_a_set_could_not_read_is_unreadable_as_on_disk() {
    let mut files = FileSet::of([("app.jadpo", "application {}\n")]);
    files.insert(
        "domains/planning/types/count.jadpo",
        Err("permission denied".to_string()),
    );
    files.insert("domains/billing/", Err("permission denied".to_string()));
    files.insert(
        "database/migrations/0001/snapshot.json",
        Err("permission denied".to_string()),
    );
    // A link to a checkout the set doesn't hold can't be read.
    files.insert("jadpo.lock", Ok(lock("../couriers-dev")));
    let loaded = load_files(PathBuf::from("shop"), &files);
    let unreadable: Vec<&str> = loaded
        .unreadable
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    assert_eq!(
        unreadable,
        [
            "../couriers-dev/",
            "domains/billing/",
            "domains/planning/types/count.jadpo"
        ]
    );
    // A record keeps why it couldn't be read, for the records' checks.
    let records = loaded.project.records(&loaded.db);
    assert!(records.iter().any(
        |record| record.path == "database/migrations/0001/snapshot.json" && record.text.is_err()
    ));
}

#[test]
fn a_set_without_app_jadpo_is_loose_files() {
    let files = FileSet::of([
        ("a.jadpo", "type A = Int {}\n"),
        ("nested/b.jadpo", "type B = Int {}\n"),
        ("nested/notes.md", "notes\n"),
        ("domains/x/emails/welcome.html", "<p/>\n"),
    ]);
    let loaded = load_files(PathBuf::from("loose"), &files);
    let db = &loaded.db;
    assert_eq!(loaded.project.layout(db), Layout::Loose);
    let paths: Vec<&String> = loaded
        .project
        .files(db)
        .iter()
        .map(|file| file.path(db))
        .collect();
    assert_eq!(paths, ["a.jadpo", "nested/b.jadpo"]);
    assert!(loaded.project.other_files(db).is_empty());
    assert!(loaded.project.records(db).is_empty());
}

/// A link to a file outside the project isn't read, so a package or an
/// example can't make a check print, or a build carry, a file from
/// elsewhere on the machine; a link to a file inside it is read as the
/// file (security audit S1).
#[cfg(unix)]
#[test]
fn a_link_out_of_the_project_is_not_read() {
    let dir = folder("links");
    let root = dir.join("shop");
    write(&root, "app.jadpo", "application {}\n");
    write(
        &dir,
        "outside/credentials",
        "aws_secret_access_key = canary\n",
    );
    write(
        &root,
        "shared/types/count.jadpo",
        "public type Count = Int {}\n",
    );
    let types = root.join("shared/types");
    std::os::unix::fs::symlink(dir.join("outside/credentials"), types.join("secret.jadpo"))
        .expect("a link out");
    std::os::unix::fs::symlink(types.join("count.jadpo"), types.join("again.jadpo"))
        .expect("a link in");
    let loaded = load(&root);
    let unreadable: Vec<(&str, &str)> = loaded
        .unreadable
        .iter()
        .map(|file| (file.path.as_str(), file.error.as_str()))
        .collect();
    assert_eq!(
        unreadable,
        [(
            "shared/types/secret.jadpo",
            "it links to a file outside the project, which Jadpo doesn't read"
        )]
    );
    let texts: Vec<String> = loaded
        .project
        .files(&loaded.db)
        .iter()
        .map(|file| file.text(&loaded.db).to_string())
        .collect();
    assert!(
        texts.iter().all(|text| !text.contains("canary")),
        "{texts:?}"
    );
    assert!(
        loaded
            .project
            .files(&loaded.db)
            .iter()
            .any(|file| file.path(&loaded.db) == "shared/types/again.jadpo"),
        "a link inside the project is read"
    );
}
