//! What an edit recomputes (docs/compiler-runtime.md §2.4): editing a body
//! leaves every item tree equal, so the symbol table and every other file's
//! resolution are reused; editing a signature recomputes them.

mod support;

use jadpo_db::{Database, Layout};
use salsa::Setter;

const A: &str =
    "type Count = Int {\n    min: 0\n}\n\nfunction double(x: Count) -> Count {\n    return x\n}\n";
const B: &str = "function triple(x: Count) -> Count {\n    return double(x)\n}\n";

fn executed(db: &Database, query: &str) -> usize {
    db.take_executions()
        .iter()
        .filter(|key| key.starts_with(&format!("{query}(")))
        .count()
}

#[test]
fn editing_a_body_does_not_recompute_another_files_resolution() {
    let mut db = Database::recording();
    let project = support::project(&db, Layout::Loose, &[("a.jadpo", A), ("b.jadpo", B)]);
    let a = project.files(&db)[0];
    assert!(jadpo_check::check(&db, project).is_empty());
    let log = db.take_executions();
    let first = |query: &str| {
        log.iter()
            .filter(|key| key.starts_with(&format!("{query}(")))
            .count()
    };
    assert_eq!(first("item_tree"), 2, "{log:?}");
    assert_eq!(first("symbols"), 1, "{log:?}");
    assert_eq!(
        first("resolve_file"),
        2,
        "both files resolve the first time: {log:?}"
    );

    // A longer body moves nothing that the item tree records.
    a.set_text(&mut db).to(A.replace(
        "    return x\n",
        "    var same: Count = x\n    return same\n",
    ));
    assert!(jadpo_check::check(&db, project).is_empty());
    let log = db.take_executions();
    let count = |query: &str| {
        log.iter()
            .filter(|key| key.starts_with(&format!("{query}(")))
            .count()
    };
    assert_eq!(count("item_tree"), 1, "a's item tree is rebuilt: {log:?}");
    assert_eq!(
        count("symbols"),
        0,
        "but is equal, so the table is reused: {log:?}"
    );
    assert_eq!(count("resolve_file"), 1, "only a resolves again: {log:?}");
}

#[test]
fn editing_a_signature_recomputes_the_files_that_depend_on_it() {
    let mut db = Database::recording();
    let project = support::project(&db, Layout::Loose, &[("a.jadpo", A), ("b.jadpo", B)]);
    let a = project.files(&db)[0];
    jadpo_check::check(&db, project);
    db.take_executions();
    a.set_text(&mut db)
        .to(A.replace("function double", "function twice"));
    let problems = jadpo_check::check(&db, project);
    assert_eq!(
        problems.iter().map(|d| d.rule.id).collect::<Vec<_>>(),
        ["semantic.unknown_callee"],
        "b now calls a function that is gone"
    );
    assert_eq!(executed(&db, "resolve_file"), 2);
}

#[test]
fn editing_a_type_body_does_not_recompute_another_files_resolution() {
    let mut db = Database::recording();
    let project = support::project(&db, Layout::Loose, &[("a.jadpo", A), ("b.jadpo", B)]);
    let a = project.files(&db)[0];
    assert!(jadpo_check::check(&db, project).is_empty());
    db.take_executions();

    // A new rule on `Count` changes its type, not any name.
    a.set_text(&mut db)
        .to(A.replace("    min: 0\n", "    min: 0\n    max: 99\n"));
    assert!(jadpo_check::check(&db, project).is_empty());
    let log = db.take_executions();
    let count = |query: &str| {
        log.iter()
            .filter(|key| key.starts_with(&format!("{query}(")))
            .count()
    };
    assert_eq!(count("symbols"), 0, "names are unchanged: {log:?}");
    assert_eq!(count("resolve_file"), 1, "only a resolves again: {log:?}");
    assert_eq!(count("type_table"), 1, "the type changed: {log:?}");
    assert_eq!(
        count("body_problems"),
        2,
        "both bodies use `Count`, so both are checked again: {log:?}"
    );
}

#[test]
fn editing_a_function_body_does_not_rebuild_the_types() {
    let mut db = Database::recording();
    let project = support::project(&db, Layout::Loose, &[("a.jadpo", A), ("b.jadpo", B)]);
    let a = project.files(&db)[0];
    jadpo_check::check(&db, project);
    db.take_executions();
    a.set_text(&mut db).to(A.replace(
        "    return x\n",
        "    var same: Count = x\n    return same\n",
    ));
    assert!(jadpo_check::check(&db, project).is_empty());
    let log = db.take_executions();
    let count = |query: &str| {
        log.iter()
            .filter(|key| key.starts_with(&format!("{query}(")))
            .count()
    };
    assert_eq!(
        count("type_shapes"),
        1,
        "a's shapes are lowered again: {log:?}"
    );
    assert_eq!(count("type_table"), 0, "but are equal: {log:?}");
    assert_eq!(count("callables"), 0, "{log:?}");
    assert_eq!(count("body_problems"), 1, "only a's bodies: {log:?}");
    assert_eq!(count("type_problems"), 1, "only a's declarations: {log:?}");
}
