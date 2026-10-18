//! Computed backfills (area 5, "Computed backfills"; chunk 5.6b): a
//! backfill worked out from each row's own fields, constants and pure
//! functions, filled in batches of one `UPDATE` each where every
//! operation in it has a SQL form, then made required; a run stopped
//! between batches leaves the column consistent and the next carries on;
//! an expression that can fail is refused. One with an operation that
//! has no SQL form is worked out in Jadpo by `jadpo migrate apply`, in
//! the same batches, transactions and resume marker (chunk 5.6b2). The
//! live tests need PostgreSQL, which `JADPO_SKIP_POSTGRES=1` skips.

use std::fs;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_runtime::{Database as _, SqlValue};
use jadpo_term::Environment;

use crate::support;

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("backfill-{name}"));
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

fn skipped() -> bool {
    std::env::var("JADPO_SKIP_POSTGRES").is_ok_and(|value| value == "1")
}

const TODO: &str = "domains/planning/entities/todo.jadpo";

/// A project whose first migration made `Todo` with a `legacy_flag`.
fn project(name: &str) -> PathBuf {
    let root = folder(name);
    write(&root, "app.jadpo", "application {}\n");
    write(
        &root,
        TODO,
        "entity Todo {\n    id: Uuid\n    legacy_flag: Bool\n}\n",
    );
    let made = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(made.code, 0, "{}{}", made.stdout, made.stderr);
    root
}

/// The second migration: `Todo.priority` worked out from the flag the
/// migration retires, through a library function, as `backfill`'s value.
fn priority_from_flag(root: &Path, backfill: &str) -> String {
    write(
        root,
        "domains/planning/types/priority.jadpo",
        "type Priority = Enum {\n    low\n    normal\n    high\n}\n",
    );
    write(
        root,
        TODO,
        "use planning/types { Priority }\n\nentity Todo {\n    id: Uuid\n    priority: Priority\n}\n",
    );
    let made = jadpo(root, &["migrate", "new", "priority_from_flag"]);
    assert_eq!(made.code, 0, "{}{}", made.stdout, made.stderr);
    write(
        root,
        "domains/planning/types/legacy_flag.jadpo",
        "type LegacyFlag = Bool {}\n",
    );
    write(
        root,
        "domains/planning/libraries/priorities.jadpo",
        "use planning/types { LegacyFlag, Priority }\n\nlibrary priorities {\n    function from_flag(legacy_flag: LegacyFlag) -> Priority {\n        if (legacy_flag) {\n            return Priority.high\n        }\n        return Priority.normal\n    }\n}\n",
    );
    let name = latest(root);
    write(
        root,
        &format!("database/migrations/{name}/changes.jadpo"),
        &format!(
            "/// Replaces the legacy flag with a priority worked out from it.\nmigration {{\n    Todo.priority: {{\n        backfill: {backfill}\n    }}\n    Todo.legacy_flag: FieldChange.retired\n}}\n"
        ),
    );
    name
}

/// The latest migration's folder name.
fn latest(root: &Path) -> String {
    let mut names: Vec<String> = fs::read_dir(root.join("database/migrations"))
        .expect("migrations")
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect();
    names.sort();
    names.pop().expect("a migration")
}

/// Runs statements, giving each row's columns as text.
fn rows(url: &str, sql: &str) -> Vec<Vec<String>> {
    let store = jadpo_host_native::postgres(url).expect("store");
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    runtime.block_on(async {
        let mut transaction = store.begin().await.expect("begin");
        let rows = transaction.query(sql, &[]).await.expect("query");
        transaction.commit().await.expect("commit");
        rows.into_iter()
            .map(|row| {
                row.into_iter()
                    .map(|value| match value {
                        SqlValue::Text(text) => text,
                        SqlValue::Int8(n) => n.to_string(),
                        SqlValue::Bool(b) => b.to_string(),
                        SqlValue::Null(_) => "NULL".to_string(),
                        other => format!("{other:?}"),
                    })
                    .collect()
            })
            .collect()
    })
}

/// Whether `out` has a row for `name` saying `what`, under the migration
/// that filled it.
fn said(out: &str, name: &str, what: &str) -> bool {
    out.lines().any(|line| {
        line.starts_with("      ")
            && line.trim_start().starts_with(name)
            && line.ends_with(&format!("  {what}"))
    })
}

fn statement(url: &str, sql: &str) {
    let store = jadpo_host_native::postgres(url).expect("store");
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    runtime.block_on(async {
        let mut transaction = store.begin().await.expect("begin");
        transaction.batch(sql).await.expect("statement");
        transaction.commit().await.expect("commit");
    });
}

/// `count` todos, every third flagged.
fn todos(url: &str, count: u32) {
    statement(
        url,
        &format!(
            "INSERT INTO \"todo\" (\"id\", \"legacy_flag\") SELECT gen_random_uuid(), n % 3 = 0 FROM generate_series(1, {count}) AS n"
        ),
    );
}

/// How many todos have each priority, and whether the column may be empty.
fn priorities(url: &str) -> (Vec<Vec<String>>, String) {
    let counts = rows(
        url,
        "SELECT coalesce(\"priority\", 'none'), \"legacy_flag\"::text, count(*) FROM \"todo\" GROUP BY 1, 2 ORDER BY 1, 2",
    );
    let nullable = rows(
        url,
        "SELECT is_nullable::text FROM information_schema.columns WHERE table_name = 'todo' AND column_name = 'priority'",
    );
    (counts, nullable[0][0].clone())
}

/// The store's migrations, as `migrate apply` hands them to the runtime.
fn steps(root: &Path) -> Vec<jadpo_runtime::Migration> {
    let mut names: Vec<String> = fs::read_dir(root.join("database/migrations"))
        .expect("migrations")
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect();
    names.sort();
    names
        .into_iter()
        .flat_map(|name| {
            let sql =
                fs::read_to_string(root.join(format!("database/migrations/{name}/postgres.sql")))
                    .expect("postgres.sql");
            jadpo_graph::migration::sections(&sql)
                .into_iter()
                .map(move |section| jadpo_runtime::Migration {
                    name: name.clone(),
                    digest: jadpo_graph::migration::section_digest(&section.sql),
                    sql: section.sql,
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn a_backfill_with_sql_forms_runs_in_batches_of_update_and_is_then_required() {
    let root = project("update");
    let name = priority_from_flag(&root, "priorities.from_flag(LegacyFlag(Todo.legacy_flag))");
    let finished = jadpo(&root, &["migrate", "new"]);
    assert_eq!(finished.code, 0, "{}{}", finished.stdout, finished.stderr);
    assert!(
        finished.stdout.contains("fill existing rows in batches"),
        "{}",
        finished.stdout
    );
    let sql = fs::read_to_string(root.join(format!("database/migrations/{name}/postgres.sql")))
        .expect("postgres.sql");
    // The generated SQL says how the backfill runs: by `UPDATE`, from
    // the forms of `if` and the variants it gives.
    assert!(
        sql.contains("-- backfill Todo.priority: by UPDATE, 1000 rows a batch, each batch its own transaction\n"),
        "{sql}"
    );
    assert!(
        sql.contains(
            "SET \"priority\" = (CASE WHEN \"todo\".\"legacy_flag\" THEN 'high' ELSE 'normal' END)"
        ),
        "{sql}"
    );
    assert!(
        sql.contains(
            "LOCK TABLE \"todo\" IN SHARE ROW EXCLUSIVE MODE;\nUPDATE \"todo\" SET \"priority\""
        ),
        "{sql}"
    );
    assert_eq!(jadpo(&root, &["check"]).code, 0);
    if skipped() {
        return;
    }
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server.create_database("backfill_update").expect("database");
    // The first migration, then rows for the second to fill.
    let mut first = steps(&root);
    first.truncate(1);
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    let store = jadpo_host_native::postgres(&url).expect("store");
    runtime
        .block_on(jadpo_runtime::migrate(&store, &first))
        .expect("the first migration");
    todos(&url, 2500);
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    assert!(
        applied.stdout.contains(&format!("{name}  applied\n")),
        "{}",
        applied.stdout
    );
    assert!(
        said(
            &applied.stdout,
            "Todo.priority",
            "2500 rows filled in 3 batches by UPDATE"
        ),
        "{}",
        applied.stdout
    );
    let (counts, nullable) = priorities(&url);
    assert_eq!(
        counts,
        vec![
            vec!["high".to_string(), "true".to_string(), "833".to_string()],
            vec![
                "normal".to_string(),
                "false".to_string(),
                "1667".to_string()
            ],
        ]
    );
    assert_eq!(nullable, "NO", "the column is required once filled");
    assert_eq!(
        rows(&url, "SELECT count(*) FROM jadpo_migration_steps"),
        vec![vec!["0".to_string()]],
        "a finished migration leaves no record of its steps"
    );
    let json = jadpo(&root, &["migrate", "apply", "--url", &url, "--json"]);
    assert!(json.stdout.contains("\"applied\": []"), "{}", json.stdout);
}

#[test]
fn a_run_stopped_between_batches_leaves_the_column_consistent_and_the_next_carries_on() {
    if skipped() {
        return;
    }
    let root = project("resume");
    priority_from_flag(&root, "priorities.from_flag(LegacyFlag(Todo.legacy_flag))");
    assert_eq!(jadpo(&root, &["migrate", "new"]).code, 0);
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server.create_database("backfill_resume").expect("database");
    let migrations = steps(&root);
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    let store = jadpo_host_native::postgres(&url).expect("store");
    runtime
        .block_on(jadpo_runtime::migrate(&store, &migrations[..1]))
        .expect("the first migration");
    todos(&url, 3500);
    // Killed after the second batch commits.
    let mut batches = 0;
    let stopped = runtime.block_on(jadpo_runtime::migrate_reporting(
        &store,
        &migrations,
        &mut |progress| {
            if matches!(progress, jadpo_runtime::Progress::Filled { .. }) {
                batches += 1;
                if batches == 2 {
                    return ControlFlow::Break(());
                }
            }
            ControlFlow::Continue(())
        },
    ));
    assert!(
        matches!(stopped, Err(jadpo_runtime::MigrateError::Stopped { .. })),
        "{stopped:?}"
    );
    // Consistent: two batches filled, every value right, the rest empty,
    // the column still optional, and the migration not recorded.
    let (counts, nullable) = priorities(&url);
    let filled: i64 = counts
        .iter()
        .filter(|row| row[0] != "none")
        .map(|row| row[2].parse::<i64>().expect("a count"))
        .sum();
    assert_eq!(filled, 2000, "{counts:?}");
    assert!(
        counts
            .iter()
            .all(|row| row[0] == "none" || (row[0] == "high") == (row[1] == "true")),
        "{counts:?}"
    );
    assert_eq!(nullable, "YES");
    assert_eq!(
        rows(&url, "SELECT count(*) FROM jadpo_migrations"),
        vec![vec!["1".to_string()]]
    );
    assert_eq!(
        rows(
            &url,
            "SELECT step::text, (after IS NOT NULL)::text FROM jadpo_migration_steps"
        ),
        vec![vec!["1".to_string(), "true".to_string()]]
    );
    // A server still on the old build writes a todo meanwhile, with a key
    // before where the batches had got to.
    statement(
        &url,
        "INSERT INTO \"todo\" (\"id\", \"legacy_flag\") VALUES ('00000000-0000-0000-0000-00000000000a', true)",
    );
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    assert!(
        said(
            &applied.stdout,
            "Todo.priority",
            "1500 rows filled in 2 batches by UPDATE, carrying on from an earlier run"
        ),
        "{}",
        applied.stdout
    );
    let (counts, nullable) = priorities(&url);
    assert_eq!(
        counts,
        vec![
            vec!["high".to_string(), "true".to_string(), "1167".to_string()],
            vec![
                "normal".to_string(),
                "false".to_string(),
                "2334".to_string()
            ],
        ]
    );
    assert_eq!(nullable, "NO");
    assert_eq!(
        rows(&url, "SELECT count(*) FROM jadpo_migration_steps"),
        vec![vec!["0".to_string()]]
    );
}

#[test]
fn a_backfill_that_can_fail_is_refused() {
    let root = folder("fails");
    write(&root, "app.jadpo", "application {}\n");
    write(
        &root,
        TODO,
        "entity Todo {\n    id: Uuid\n    title: Text\n}\n",
    );
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    write(
        &root,
        "domains/planning/types/rank.jadpo",
        "type Rank = Int {\n    min: 1\n    max: 5\n}\n",
    );
    write(
        &root,
        TODO,
        "use planning/types { Rank }\n\nentity Todo {\n    id: Uuid\n    title: Text\n    rank: Rank\n}\n",
    );
    assert_eq!(jadpo(&root, &["migrate", "new", "add_rank"]).code, 0);
    let name = latest(&root);
    // A title's length may be past the rank's `max`: area 5 doesn't say
    // what such a row gets.
    write(
        &root,
        &format!("database/migrations/{name}/changes.jadpo"),
        "migration {\n    Todo.rank: {\n        backfill: Rank(text.length(Todo.title))\n    }\n}\n",
    );
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 1);
    assert!(
        checked
            .stdout
            .contains("error: The backfill for `Todo.rank` can fail\n")
            && checked.stdout.contains("rule: migration.backfill_can_fail"),
        "{}",
        checked.stdout
    );
    let stopped = jadpo(&root, &["migrate", "new"]);
    assert_eq!(stopped.code, 1, "{}", stopped.stdout);
    // Kept in range, it can't fail, and runs by `UPDATE`.
    write(
        &root,
        &format!("database/migrations/{name}/changes.jadpo"),
        "migration {\n    Todo.rank: {\n        backfill: Rank(integer.max(1, integer.min(5, text.length(Todo.title))))\n    }\n}\n",
    );
    let finished = jadpo(&root, &["migrate", "new"]);
    assert_eq!(finished.code, 0, "{}{}", finished.stdout, finished.stderr);
    assert_eq!(jadpo(&root, &["check"]).code, 0);
}

/// `from_flag` with a `var mut`, which has no SQL form, so the backfill
/// calling it is worked out in Jadpo.
const FROM_FLAG_IN_JADPO: &str = "use planning/types { LegacyFlag, Priority }\n\nlibrary priorities {\n    function from_flag(legacy_flag: LegacyFlag) -> Priority {\n        var mut priority: Priority = Priority.normal\n        if (legacy_flag) {\n            priority = Priority.high\n        }\n        return priority\n    }\n}\n";

/// The priority migration, its backfill worked out in Jadpo, finished.
fn priority_in_jadpo(root: &Path) -> String {
    let name = priority_from_flag(root, "priorities.from_flag(LegacyFlag(Todo.legacy_flag))");
    write(
        root,
        "domains/planning/libraries/priorities.jadpo",
        FROM_FLAG_IN_JADPO,
    );
    let finished = jadpo(root, &["migrate", "new"]);
    assert_eq!(finished.code, 0, "{}{}", finished.stdout, finished.stderr);
    name
}

/// What the runtime's `from_flag` gives, for a test driving the runtime's
/// batches itself.
struct FromFlag;

impl jadpo_runtime::Backfills for FromFlag {
    fn work_out(
        &self,
        _migration: &str,
        field: &str,
        rows: &[&[SqlValue]],
    ) -> Result<Vec<SqlValue>, jadpo_runtime::Unworked> {
        assert_eq!(field, "Todo.priority");
        Ok(rows
            .iter()
            .map(|row| match row {
                [SqlValue::Bool(true)] => SqlValue::Text("high".into()),
                _ => SqlValue::Text("normal".into()),
            })
            .collect())
    }
}

#[test]
fn a_backfill_without_a_sql_form_is_worked_out_in_jadpo_with_the_runtimes_values() {
    let root = folder("in-jadpo");
    write(&root, "app.jadpo", "application {}\n");
    write(
        &root,
        TODO,
        "entity Todo {\n    id: Uuid\n    title: Text\n}\n",
    );
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    write(
        &root,
        TODO,
        "entity Todo {\n    id: Uuid\n    title: Text\n    shouted: Text\n}\n",
    );
    assert_eq!(jadpo(&root, &["migrate", "new", "add_shouted"]).code, 0);
    let name = latest(&root);
    // `text.upper` follows Unicode's case rules, which PostgreSQL's
    // `upper` doesn't, so it has no SQL form.
    write(
        &root,
        &format!("database/migrations/{name}/changes.jadpo"),
        "migration {\n    Todo.shouted: {\n        backfill: text.upper(Todo.title)\n    }\n}\n",
    );
    let finished = jadpo(&root, &["migrate", "new"]);
    assert_eq!(finished.code, 0, "{}{}", finished.stdout, finished.stderr);
    assert!(
        finished
            .stdout
            .contains("fill existing rows in batches in Jadpo"),
        "{}",
        finished.stdout
    );
    let sql = fs::read_to_string(root.join(format!("database/migrations/{name}/postgres.sql")))
        .expect("postgres.sql");
    // The generated SQL says the backfill runs in Jadpo, reads each batch
    // and writes the values back, and fills the rows written meanwhile in
    // Jadpo too.
    assert!(
        sql.contains("-- backfill Todo.shouted: in Jadpo, 1000 rows a batch, each batch its own transaction, slower than by UPDATE since something in it has no SQL form; its code's digest "),
        "{sql}"
    );
    assert!(
        sql.contains("SELECT \"id\", \"title\" FROM \"todo\" WHERE \"id\" > $1 AND \"shouted\" IS NULL ORDER BY \"id\" LIMIT 1000 FOR NO KEY UPDATE;\n"),
        "{sql}"
    );
    assert!(
        sql.contains("LOCK TABLE \"todo\" IN SHARE ROW EXCLUSIVE MODE;\n-- fill Todo.shouted in Jadpo: the rows written meanwhile\n"),
        "{sql}"
    );
    assert!(!sql.contains("SET \"shouted\" = upper"), "{sql}");
    assert_eq!(jadpo(&root, &["check"]).code, 0);
    if skipped() {
        return;
    }
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server
        .create_database("backfill_in_jadpo")
        .expect("database");
    let migrations = steps(&root);
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    let store = jadpo_host_native::postgres(&url).expect("store");
    runtime
        .block_on(jadpo_runtime::migrate(&store, &migrations[..1]))
        .expect("the first migration");
    let titles = ["straße", "istanbul", "Rye loaf", "ǆemal", "ﬁx the ﬂue"];
    for title in titles {
        statement(
            &url,
            &format!(
                "INSERT INTO \"todo\" (\"id\", \"title\") VALUES (gen_random_uuid(), '{title}')"
            ),
        );
    }
    statement(
        &url,
        "INSERT INTO \"todo\" (\"id\", \"title\") SELECT gen_random_uuid(), 'todo ' || n FROM generate_series(1, 1200) AS n",
    );
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    assert!(
        said(
            &applied.stdout,
            "Todo.shouted",
            "1205 rows filled in 2 batches in Jadpo (slower than SQL)"
        ),
        "{}",
        applied.stdout
    );
    // Every value is the runtime's own.
    for row in rows(&url, "SELECT \"title\", \"shouted\" FROM \"todo\"") {
        let expected = jadpo_value::text::upper(&row[0], None).expect("upper");
        assert_eq!(row[1], expected, "{}", row[0]);
    }
    assert_eq!(
        rows(
            &url,
            "SELECT \"shouted\" FROM \"todo\" WHERE \"title\" = 'straße'"
        ),
        vec![vec!["STRASSE".to_string()]]
    );
    assert_eq!(
        rows(
            &url,
            "SELECT is_nullable::text FROM information_schema.columns WHERE table_name = 'todo' AND column_name = 'shouted'",
        ),
        vec![vec!["NO".to_string()]],
        "the column is required once filled"
    );
    let json = jadpo(&root, &["migrate", "apply", "--url", &url, "--json"]);
    assert!(json.stdout.contains("\"applied\": []"), "{}", json.stdout);
}

#[test]
fn a_backfill_in_jadpo_resumes_after_a_stop_and_fills_rows_written_meanwhile() {
    let root = project("in-jadpo-resume");
    let name = priority_in_jadpo(&root);
    let sql = fs::read_to_string(root.join(format!("database/migrations/{name}/postgres.sql")))
        .expect("postgres.sql");
    assert!(
        sql.contains("-- backfill Todo.priority: in Jadpo, "),
        "{sql}"
    );
    if skipped() {
        return;
    }
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server
        .create_database("backfill_in_jadpo_resume")
        .expect("database");
    let migrations = steps(&root);
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    let store = jadpo_host_native::postgres(&url).expect("store");
    runtime
        .block_on(jadpo_runtime::migrate(&store, &migrations[..1]))
        .expect("the first migration");
    todos(&url, 3500);
    // Stopped after the second batch commits.
    let mut batches = 0;
    let stopped = runtime.block_on(jadpo_runtime::migrate_with(
        &store,
        &migrations,
        &FromFlag,
        &mut |progress| {
            if matches!(progress, jadpo_runtime::Progress::Filled { .. }) {
                batches += 1;
                if batches == 2 {
                    return ControlFlow::Break(());
                }
            }
            ControlFlow::Continue(())
        },
    ));
    assert!(
        matches!(stopped, Err(jadpo_runtime::MigrateError::Stopped { .. })),
        "{stopped:?}"
    );
    let (counts, nullable) = priorities(&url);
    let filled: i64 = counts
        .iter()
        .filter(|row| row[0] != "none")
        .map(|row| row[2].parse::<i64>().expect("a count"))
        .sum();
    assert_eq!(filled, 2000, "{counts:?}");
    assert_eq!(nullable, "YES");
    assert_eq!(
        rows(
            &url,
            "SELECT step::text, (after IS NOT NULL)::text FROM jadpo_migration_steps"
        ),
        vec![vec!["1".to_string(), "true".to_string()]]
    );
    // Servers still on the old build write two todos meanwhile: one with a
    // key before where the batches had got to, which only the last step
    // reaches, and one after it.
    statement(
        &url,
        "INSERT INTO \"todo\" (\"id\", \"legacy_flag\") VALUES ('00000000-0000-0000-0000-00000000000a', true), ('ffffffff-ffff-ffff-ffff-fffffffffff0', false)",
    );
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    assert!(
        said(
            &applied.stdout,
            "Todo.priority",
            "1501 rows filled in 2 batches in Jadpo (slower than SQL), carrying on from an earlier run"
        ),
        "{}",
        applied.stdout
    );
    let (counts, nullable) = priorities(&url);
    assert_eq!(
        counts,
        vec![
            vec!["high".to_string(), "true".to_string(), "1167".to_string()],
            vec![
                "normal".to_string(),
                "false".to_string(),
                "2335".to_string()
            ],
        ]
    );
    assert_eq!(nullable, "NO");
    assert_eq!(
        rows(&url, "SELECT count(*) FROM jadpo_migration_steps"),
        vec![vec!["0".to_string()]]
    );
}

#[test]
fn a_backfill_in_jadpo_follows_the_code_it_runs() {
    let root = project("in-jadpo-code");
    priority_in_jadpo(&root);
    assert_eq!(jadpo(&root, &["check"]).code, 0);
    // The function changes after the migration was written: its SQL no
    // longer holds what `migrate apply` would run.
    write(
        &root,
        "domains/planning/libraries/priorities.jadpo",
        &FROM_FLAG_IN_JADPO.replace("Priority.normal\n        if", "Priority.low\n        if"),
    );
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 1, "{}", checked.stdout);
    assert!(
        checked.stdout.contains("doesn't hold its decisions yet"),
        "{}",
        checked.stdout
    );
    assert_eq!(jadpo(&root, &["migrate", "new"]).code, 0);
    assert_eq!(jadpo(&root, &["check"]).code, 0);
}

#[test]
fn an_earlier_migrations_backfill_in_jadpo_waits_for_the_build_it_was_written_in() {
    if skipped() {
        return;
    }
    let root = project("in-jadpo-earlier");
    let name = priority_in_jadpo(&root);
    // A later migration: the priority's backfill is no longer the latest
    // migration's, so nothing checks it against today's code.
    write(
        &root,
        "domains/planning/entities/tag.jadpo",
        "entity Tag {\n    id: Uuid\n}\n",
    );
    let later = jadpo(&root, &["migrate", "new", "add_tag"]);
    assert_eq!(later.code, 0, "{}{}", later.stdout, later.stderr);
    // Nothing checks the earlier migration's decisions now, so nothing
    // uses `from_flag`, and it goes.
    fs::remove_file(root.join("domains/planning/libraries/priorities.jadpo")).expect("remove");
    fs::remove_file(root.join("domains/planning/types/legacy_flag.jadpo")).expect("remove");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server
        .create_database("backfill_in_jadpo_earlier")
        .expect("database");
    let migrations = steps(&root);
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    let store = jadpo_host_native::postgres(&url).expect("store");
    runtime
        .block_on(jadpo_runtime::migrate(&store, &migrations[..1]))
        .expect("the first migration");
    todos(&url, 10);
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 1, "{}{}", applied.stdout, applied.stderr);
    assert!(
        applied.stderr.contains(&format!(
            "error: Filling `Todo.priority` for `{name}` needs the build it was written in\n"
        )),
        "{}",
        applied.stderr
    );
    let (counts, nullable) = priorities(&url);
    assert_eq!(counts.len(), 2, "{counts:?}");
    assert!(counts.iter().all(|row| row[0] == "none"), "{counts:?}");
    assert_eq!(nullable, "YES");
    // On an empty table there is nothing to work out, so a new database
    // takes every migration at once.
    let fresh = server
        .create_database("backfill_in_jadpo_fresh")
        .expect("database");
    let applied = jadpo(&root, &["migrate", "apply", "--url", &fresh]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
}
