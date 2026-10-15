//! `jadpo migrate` (area 5, "Migrations"; area 14): `new` writes the next
//! migration from the entities, waits for the decisions a change needs and
//! finishes the migration once they are written; `jadpo check` agrees at
//! each step; `apply` and `status` run against a live database, which
//! `JADPO_SKIP_POSTGRES=1` skips.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

const APP: &str = "application {}\n";
const TODO: &str = "domains/planning/entities/todo.jadpo";

mod support;

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("migrate-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temporary folder");
    dir
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut output = run(&args, &Environment::empty().with_cwd(cwd));
    output.stdout = support::without_run_time(&output.stdout);
    output
}

/// A project with one entity and no migration yet.
fn project(name: &str) -> PathBuf {
    let root = folder(name);
    write(&root, "app.jadpo", APP);
    write(
        &root,
        TODO,
        "entity Todo {\n    id: Uuid\n    title: Text\n}\n",
    );
    root
}

/// The migration folders, in order.
fn migrations(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(root.join("database/migrations"))
        .map(|entries| {
            entries
                .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn the_first_migration_creates_the_tables_and_check_agrees() {
    let root = project("first");
    let before = jadpo(&root, &["check"]);
    assert_eq!(before.code, 1);
    assert!(
        before.stdout.contains("rule: migration.out_of_date")
            && before.stdout.contains("Run `jadpo migrate new initial`"),
        "{}",
        before.stdout
    );
    let missing = jadpo(&root, &["migrate", "new"]);
    assert_eq!(missing.code, 2);
    assert!(
        missing.stderr.contains("rule: cli.migrate_arguments"),
        "{}",
        missing.stderr
    );
    let made = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(made.code, 0, "{}{}", made.stdout, made.stderr);
    let names = migrations(&root);
    assert_eq!(names.len(), 1);
    assert!(
        names[0].ends_with("_initial") && names[0].len() == 21,
        "{names:?}"
    );
    assert!(
        made.stdout
            .contains("  compared the entities with an empty database\n"),
        "{}",
        made.stdout
    );
    assert!(made.stdout.contains("    + table  todo"), "{}", made.stdout);
    assert!(
        made.stdout
            .contains(&format!("Wrote database/migrations/{}/\n", names[0]))
    );
    assert!(
        made.stdout
            .contains("next: jadpo check  confirms the migration matches the entities")
    );
    let sql =
        fs::read_to_string(root.join(format!("database/migrations/{}/postgres.sql", names[0])))
            .expect("postgres.sql");
    assert!(sql.contains("-- store: main\nCREATE TABLE "), "{sql}");
    assert!(
        sql.contains(
            "CREATE TABLE \"todo\" (\n    \"id\" uuid NOT NULL,\n    \"title\" text NOT NULL,"
        ),
        "{sql}"
    );
    assert_eq!(jadpo(&root, &["check"]).stdout, "checked: 2 files\n");
    let again = jadpo(&root, &["migrate", "new", "nothing"]);
    assert_eq!(again.code, 0);
    assert!(
        again
            .stdout
            .starts_with("Nothing to migrate: the entities match "),
        "{}",
        again.stdout
    );
    let status = jadpo(&root, &["migrate", "status"]);
    assert!(
        status.stdout.starts_with("1 migration\n"),
        "{}",
        status.stdout
    );
}

#[test]
fn a_change_that_needs_decisions_waits_for_them() {
    let root = project("decisions");
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    write(
        &root,
        "domains/planning/types/rank.jadpo",
        "type Rank = Int {\n    min: 1\n    max: 5\n}\n",
    );
    write(
        &root,
        TODO,
        "use planning/types { Rank }\n\nentity Todo {\n    id: Uuid\n    rank: Rank\n}\n",
    );
    let started = jadpo(&root, &["migrate", "new", "add_rank"]);
    assert_eq!(started.code, 0, "{}", started.stdout);
    assert!(
        started
            .stdout
            .contains("    - title  text      retired: the column and its data stay"),
        "{}",
        started.stdout
    );
    assert!(
        started.stdout.contains("    + rank   smallint  new field"),
        "{}",
        started.stdout
    );
    assert!(
        started
            .stdout
            .contains("changes.jadpo  2 decisions to make"),
        "{}",
        started.stdout
    );
    assert!(
        started
            .stdout
            .contains("decision: existing `Todo` records need a `rank`"),
        "{}",
        started.stdout
    );
    assert!(
        started
            .stdout
            .contains("Set `backfill:` for `Todo.rank`, such as `1` (human)"),
        "{}",
        started.stdout
    );
    let latest = migrations(&root).pop().expect("a migration");
    let changes_path = format!("database/migrations/{latest}/changes.jadpo");
    let check = jadpo(&root, &["check"]);
    assert_eq!(
        check
            .stdout
            .matches("rule: migration.decision_required")
            .count(),
        2,
        "{}",
        check.stdout
    );
    // A value its rules refuse doesn't fit.
    let changes = fs::read_to_string(root.join(&changes_path)).expect("changes");
    let written = changes
        .replace("    Todo.title: {}", "    Todo.title: FieldChange.retired")
        .replace(
            "    Todo.rank: {}",
            "    Todo.rank: {\n        backfill: 9\n    }",
        );
    write(&root, &changes_path, &written);
    let check = jadpo(&root, &["check"]);
    assert!(
        check.stdout.contains("rule: migration.decision_invalid"),
        "{}",
        check.stdout
    );
    assert!(
        check.stdout.contains("it is above the field's `max`"),
        "{}",
        check.stdout
    );
    write(
        &root,
        &changes_path,
        &written.replace("backfill: 9", "backfill: 3"),
    );
    let check = jadpo(&root, &["check"]);
    assert!(
        check.stdout.contains("rule: migration.unfinished"),
        "{}",
        check.stdout
    );
    let finished = jadpo(&root, &["migrate", "new"]);
    assert_eq!(finished.code, 0, "{}", finished.stdout);
    assert!(
        finished
            .stdout
            .contains("changes.jadpo  every decision made"),
        "{}",
        finished.stdout
    );
    assert_eq!(migrations(&root).len(), 2, "finished in place");
    let sql = fs::read_to_string(root.join(format!("database/migrations/{latest}/postgres.sql")))
        .expect("sql");
    assert!(
        sql.contains("UPDATE \"todo\" SET \"rank\" = 3 WHERE \"rank\" IS NULL;"),
        "{sql}"
    );
    assert!(
        sql.contains("ALTER TABLE \"todo\" ALTER COLUMN \"title\" DROP NOT NULL;"),
        "{sql}"
    );
    assert_eq!(jadpo(&root, &["check"]).code, 0);
    assert_eq!(
        jadpo(&root, &["fmt", "--check"]).code,
        0,
        "changes.jadpo is formatted"
    );
}

#[test]
fn removing_an_entity_isnt_built_yet() {
    let root = project("remove");
    write(
        &root,
        "domains/planning/entities/tag.jadpo",
        "entity Tag {\n    id: Uuid\n}\n",
    );
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    fs::remove_file(root.join(TODO)).expect("remove");
    let refused = jadpo(&root, &["migrate", "new", "remove_todo"]);
    assert_eq!(refused.code, 1);
    assert!(
        refused.stderr.contains("rule: toolchain.not_built_yet"),
        "{}",
        refused.stderr
    );
    assert!(
        refused.stderr.contains("entity `Todo` removed"),
        "{}",
        refused.stderr
    );
}

#[test]
fn apply_and_status_need_a_database() {
    let root = project("needs-url");
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    let apply = jadpo(&root, &["migrate", "apply"]);
    assert_eq!(apply.code, 2);
    assert!(
        apply.stderr.contains("needs the database's URL"),
        "{}",
        apply.stderr
    );
    let wrong = jadpo(&root, &["migrate", "undo"]);
    assert_eq!(wrong.code, 2);
    assert!(
        wrong
            .stderr
            .contains("`undo` isn't something `jadpo migrate` does"),
        "{}",
        wrong.stderr
    );
}

// --- against a live database --------------------------------------------------

fn skipped() -> bool {
    std::env::var("JADPO_SKIP_POSTGRES").is_ok_and(|value| value == "1")
}

#[test]
fn apply_runs_pending_migrations_once_and_status_shows_them() {
    if skipped() {
        println!("skipped: JADPO_SKIP_POSTGRES is set");
        return;
    }
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server.create_database("migrate_apply").expect("database");
    let root = project("apply");
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    let first = migrations(&root).remove(0);
    let pending = jadpo(&root, &["migrate", "status", "--url", &url]);
    assert_eq!(pending.code, 0, "{}{}", pending.stdout, pending.stderr);
    assert!(
        pending.stdout.contains("`main`: 0 applied, 1 pending"),
        "{}",
        pending.stdout
    );
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    assert!(
        applied
            .stdout
            .starts_with("Applied 1 migration to `main`\n"),
        "{}",
        applied.stdout
    );
    assert!(applied.stdout.contains(&first), "{}", applied.stdout);
    let again = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert!(
        again.stdout.starts_with("`main` is up to date"),
        "{}",
        again.stdout
    );
    let status = jadpo(&root, &["migrate", "status", "--url", &url]);
    assert!(
        status.stdout.contains(&format!("    {first}  applied")),
        "{}",
        status.stdout
    );
    // A migration changed after it ran is refused.
    let sql = root.join(format!("database/migrations/{first}/postgres.sql"));
    let text = fs::read_to_string(&sql).expect("sql");
    fs::write(
        &sql,
        text.replace("\"title\" text NOT NULL", "\"title\" text"),
    )
    .expect("write");
    let changed = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(changed.code, 1);
    assert!(
        changed.stderr.contains("rule: migration.store_diverged"),
        "{}",
        changed.stderr
    );
}
