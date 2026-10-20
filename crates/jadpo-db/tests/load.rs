//! Finding and reading a project: walking up to `app.jadpo`, the files it
//! holds in path order, and loose files outside any project.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_db::{Layout, item_tree, load, symbols};

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
