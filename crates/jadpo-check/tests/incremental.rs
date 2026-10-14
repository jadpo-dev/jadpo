//! What an edit recomputes (docs/compiler-runtime.md §2.4): editing a body
//! leaves every item tree equal, so the symbol table and every other file's
//! resolution are reused; a body depends only on the declarations it reads,
//! through the per-name lookups; editing a signature recomputes what reads
//! it.
//!
//! Every test that says a query did not run also shows it running when what
//! it reads changes, so a renamed query or a dependency that was never
//! there can't make the test pass.

mod support;

use jadpo_db::{Database, File, Layout, Project};
use salsa::Setter;

const A: &str =
    "type Count = Int {\n    min: 0\n}\n\nfunction double(x: Count) -> Count {\n    return x\n}\n";
const B: &str = "function triple(x: Count) -> Count {\n    return double(x)\n}\n";
/// A file that reads nothing `A` or `B` declares.
const C: &str = "type Label = Text {\n    max_length: 20\n}\n\nfunction same(x: Label) -> Label {\n    return x\n}\n";

/// How many times each query ran since the last call.
struct Log(Vec<String>);

impl Log {
    fn take(db: &Database) -> Log {
        Log(db.take_executions())
    }

    fn count(&self, query: &str) -> usize {
        self.0
            .iter()
            .filter(|key| key.starts_with(&format!("{query}(")))
            .count()
    }
}

fn three(db: &Database) -> (Project, File, File, File) {
    let project = support::project(
        db,
        Layout::Loose,
        &[("a.jadpo", A), ("b.jadpo", B), ("c.jadpo", C)],
    );
    let files = project.files(db).clone();
    (project, files[0], files[1], files[2])
}

#[test]
fn editing_a_body_does_not_recompute_another_files_resolution() {
    let mut db = Database::recording();
    let (project, a, _, _) = three(&db);
    assert!(jadpo_check::check(&db, project).is_empty());
    let cold = Log::take(&db);
    assert_eq!(cold.count("item_tree"), 3, "{:?}", cold.0);
    assert_eq!(cold.count("symbols"), 1, "{:?}", cold.0);
    assert_eq!(cold.count("resolve_file"), 3, "{:?}", cold.0);

    // A longer body moves nothing that the item tree records.
    a.set_text(&mut db).to(A.replace(
        "    return x\n",
        "    var same: Count = x\n    return same\n",
    ));
    assert!(jadpo_check::check(&db, project).is_empty());
    let log = Log::take(&db);
    assert_eq!(
        log.count("item_tree"),
        1,
        "a's item tree is rebuilt: {:?}",
        log.0
    );
    assert_eq!(log.count("symbols"), 0, "but is equal: {:?}", log.0);
    assert_eq!(
        log.count("resolve_file"),
        1,
        "only a resolves again: {:?}",
        log.0
    );

    // Renaming the function changes the item tree, so the table is rebuilt.
    a.set_text(&mut db)
        .to(A.replace("function double", "function twice"));
    jadpo_check::check(&db, project);
    let log = Log::take(&db);
    assert_eq!(log.count("symbols"), 1, "{:?}", log.0);
}

#[test]
fn editing_a_signature_recomputes_the_files_that_read_it() {
    let mut db = Database::recording();
    let (project, a, _, _) = three(&db);
    jadpo_check::check(&db, project);
    Log::take(&db);
    a.set_text(&mut db)
        .to(A.replace("function double", "function twice"));
    let problems = jadpo_check::check(&db, project);
    assert_eq!(
        problems.iter().map(|d| d.rule.id).collect::<Vec<_>>(),
        ["semantic.unknown_callee"],
        "b now calls a function that is gone"
    );
    let log = Log::take(&db);
    assert_eq!(
        log.count("resolve_file"),
        1,
        "a's declarations; b's call is resolved in its body's walk: {:?}",
        log.0
    );
    assert_eq!(
        log.count("typed_body"),
        2,
        "`twice`, and `triple`, which called it: {:?}",
        log.0
    );
}

#[test]
fn a_rule_on_a_type_rechecks_only_the_bodies_that_read_it() {
    let mut db = Database::recording();
    let (project, a, _, c) = three(&db);
    assert!(jadpo_check::check(&db, project).is_empty());
    let cold = Log::take(&db);
    assert_eq!(cold.count("typed_body"), 3, "{:?}", cold.0);
    assert_eq!(cold.count("type_table"), 1, "{:?}", cold.0);

    // A new rule on `Count` changes its type, not any name: the two bodies
    // that read `Count` are checked again, `same` is not.
    a.set_text(&mut db)
        .to(A.replace("    min: 0\n", "    min: 0\n    max: 99\n"));
    assert!(jadpo_check::check(&db, project).is_empty());
    let log = Log::take(&db);
    assert_eq!(log.count("symbols"), 0, "names are unchanged: {:?}", log.0);
    assert_eq!(log.count("type_table"), 1, "the type changed: {:?}", log.0);
    assert_eq!(
        log.count("resolve_file"),
        1,
        "only a resolves again: {:?}",
        log.0
    );
    assert_eq!(
        log.count("typed_body"),
        2,
        "`double` and `triple` read `Count`; `same` doesn't: {:?}",
        log.0
    );
    assert_eq!(
        log.count("type_problems"),
        2,
        "a's and b's signatures: {:?}",
        log.0
    );

    // The control: a rule on `Label` rechecks `same` alone.
    c.set_text(&mut db)
        .to(C.replace("max_length: 20", "max_length: 30"));
    assert!(jadpo_check::check(&db, project).is_empty());
    let log = Log::take(&db);
    assert_eq!(log.count("typed_body"), 1, "{:?}", log.0);
}

#[test]
fn editing_a_function_body_does_not_rebuild_the_types() {
    let mut db = Database::recording();
    let (project, a, _, _) = three(&db);
    jadpo_check::check(&db, project);
    let cold = Log::take(&db);
    for query in ["type_table", "callables", "typed_body", "type_problems"] {
        assert!(cold.count(query) > 0, "{query} runs: {:?}", cold.0);
    }
    a.set_text(&mut db).to(A.replace(
        "    return x\n",
        "    var same: Count = x\n    return same\n",
    ));
    assert!(jadpo_check::check(&db, project).is_empty());
    let log = Log::take(&db);
    assert_eq!(
        log.count("type_shapes"),
        1,
        "a's shapes are lowered again: {:?}",
        log.0
    );
    assert_eq!(log.count("type_table"), 0, "but are equal: {:?}", log.0);
    assert_eq!(log.count("callables"), 0, "{:?}", log.0);
    assert_eq!(log.count("typed_body"), 1, "only `double`: {:?}", log.0);
    assert_eq!(
        log.count("type_problems"),
        1,
        "only a's declarations: {:?}",
        log.0
    );
}

#[test]
fn a_new_file_nothing_reads_checks_only_itself() {
    let mut db = Database::recording();
    let (project, _, _, _) = three(&db);
    jadpo_check::check(&db, project);
    Log::take(&db);
    let mut files = project.files(&db).clone();
    files.push(File::new(
        &db,
        "d.jadpo".to_string(),
        "type Score = Int {\n    min: 0\n}\n\nfunction keep(x: Score) -> Score {\n    return x\n}\n"
            .to_string(),
    ));
    project.set_files(&mut db).to(files);
    assert!(jadpo_check::check(&db, project).is_empty());
    let log = Log::take(&db);
    assert_eq!(log.count("symbols"), 1, "the table grew: {:?}", log.0);
    assert_eq!(log.count("type_table"), 1, "{:?}", log.0);
    for query in [
        "typed_body",
        "resolve_file",
        "naming_problems",
        "type_problems",
        "body_problems",
    ] {
        assert_eq!(
            log.count(query),
            1,
            "only the new file's {query}: {:?}",
            log.0
        );
    }
}
