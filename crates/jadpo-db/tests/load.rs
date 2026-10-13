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
    assert_eq!(
        loaded.project.other_files(db),
        &["domains/planning/emails/welcome.html".to_string()]
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
