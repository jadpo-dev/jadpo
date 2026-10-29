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
    if !jadpo_test::live() {
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
    if !jadpo_test::live() {
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

#[test]
fn a_backfill_copies_a_field_only_into_one_its_readers_may_read() {
    // A note's secret is its owner's; copied into a field every signed-in
    // reader may read, the migration would show it to them (area 9,
    // "Copies"). Copied into a field held as tightly, it is clean.
    let root = folder("field-rules");
    write(
        &root,
        "app.jadpo",
        "use notes/entities { Person }\n\nprincipal {\n    user: {\n        subject: Person.handle\n    }\n}\n\nauthentication {\n    api_token: {}\n}\n",
    );
    write(
        &root,
        "domains/notes/types/label.jadpo",
        "type Label = Text {\n    max_length: 80\n}\n",
    );
    write(
        &root,
        "domains/notes/entities/person.jadpo",
        "use notes/types { Label }\n\nentity Person {\n    id: Uuid\n    handle: Label {\n        unique: true\n    }\n\n    policy: {\n        read: [id]\n    }\n}\n",
    );
    let note = |extra: &str| {
        format!(
            "use notes/entities {{ Person }}\nuse notes/types {{ Label }}\n\nentity Note {{\n    id: Uuid\n    owner_id: Person.id {{\n        on_delete: OnDelete.cascade\n    }}\n    secret: Label {{\n        policy: {{\n            read: [owner_id]\n        }}\n    }}\n{extra}\n    policy: {{\n        create: [owner_id]\n        read: [owner_id, Access.authenticated]\n    }}\n}}\n"
        )
    };
    const NOTE: &str = "domains/notes/entities/note.jadpo";
    write(&root, NOTE, &note(""));
    let made = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(made.code, 0, "{}{}", made.stdout, made.stderr);
    write(&root, NOTE, &note("    shown: Label\n"));
    let made = jadpo(&root, &["migrate", "new", "show_secret"]);
    assert_eq!(made.code, 0, "{}{}", made.stdout, made.stderr);
    let name = latest(&root);
    write(
        &root,
        &format!("database/migrations/{name}/changes.jadpo"),
        "migration {\n    Note.shown: {\n        backfill: Note.secret\n    }\n}\n",
    );
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 1, "{}", checked.stdout);
    assert!(
        checked.stdout.contains(
            "error: The backfill for `Note.shown` copies `Note.secret`, which not everyone who may read `Note.shown` may read\n"
        ) && checked.stdout.contains("rule: policy.output_field_unproved"),
        "{}",
        checked.stdout
    );
    write(
        &root,
        NOTE,
        &note(
            "    shown: Label {\n        policy: {\n            read: [owner_id]\n        }\n    }\n",
        ),
    );
    let finished = jadpo(&root, &["migrate", "new"]);
    assert_eq!(finished.code, 0, "{}{}", finished.stdout, finished.stderr);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
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
    if !jadpo_test::live() {
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
    if !jadpo_test::live() {
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
    if !jadpo_test::live() {
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

const ITEM: &str = "domains/shop/entities/item.jadpo";

/// A shop whose items have a cost to the penny, then a second migration
/// adding `total`, an `Amount` to four places, worked out by `backfill`
/// through the `totals` library `library` holds, with the types `types`
/// beside it: the migration, finished.
fn shop(name: &str, types: &[(&str, &str)], library: &str, backfill: &str) -> (PathBuf, String) {
    let root = folder(name);
    write(&root, "app.jadpo", "application {}\n");
    write(
        &root,
        "domains/shop/types/cost.jadpo",
        "type Cost = Decimal {\n    scale: 2\n}\n",
    );
    write(
        &root,
        ITEM,
        "use shop/types { Cost }\n\nentity Item {\n    id: Uuid\n    cost: Cost\n}\n",
    );
    let made = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(made.code, 0, "{}{}", made.stdout, made.stderr);
    write(
        &root,
        "domains/shop/types/amount.jadpo",
        "type Amount = Decimal {\n    scale: 4\n}\n",
    );
    write(
        &root,
        ITEM,
        "use shop/types { Amount, Cost }\n\nentity Item {\n    id: Uuid\n    cost: Cost\n    total: Amount\n}\n",
    );
    let made = jadpo(&root, &["migrate", "new", "add_total"]);
    assert_eq!(made.code, 0, "{}{}", made.stdout, made.stderr);
    for (file, text) in types {
        write(&root, &format!("domains/shop/types/{file}.jadpo"), text);
    }
    write(&root, "domains/shop/libraries/totals.jadpo", library);
    let name = latest(&root);
    write(
        &root,
        &format!("database/migrations/{name}/changes.jadpo"),
        &format!("migration {{\n    Item.total: {{\n        backfill: {backfill}\n    }}\n}}\n"),
    );
    let finished = jadpo(&root, &["migrate", "new"]);
    assert_eq!(finished.code, 0, "{}{}", finished.stdout, finished.stderr);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    (root, name)
}

/// Applies the shop's first migration, stores items costing `costs`, then
/// applies the rest with `jadpo migrate apply`: each item's cost and total
/// as text, by cost.
fn shop_totals(root: &Path, database: &str, costs: &[&str]) -> Vec<Vec<String>> {
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server.create_database(database).expect("database");
    let migrations = steps(root);
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    let store = jadpo_host_native::postgres(&url).expect("store");
    runtime
        .block_on(jadpo_runtime::migrate(&store, &migrations[..1]))
        .expect("the first migration");
    let values: Vec<String> = costs
        .iter()
        .map(|cost| format!("(gen_random_uuid(), {cost})"))
        .collect();
    statement(
        &url,
        &format!(
            "INSERT INTO \"item\" (\"id\", \"cost\") VALUES {}",
            values.join(", ")
        ),
    );
    let applied = jadpo(root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    rows(
        &url,
        "SELECT \"cost\"::text, \"total\"::text FROM \"item\" ORDER BY \"item\".\"cost\"",
    )
}

/// Costs either side of each half, both signs.
const HALVES: [&str; 9] = [
    "-3.50", "-2.50", "0.00", "0.50", "1.50", "2.49", "2.50", "2.51", "3.50",
];

#[test]
fn a_scaled_construction_by_update_rounds_half_to_even_as_the_runtime_does() {
    // The review's `Amount(Whole(cost))`: `Whole` keeps no places, so the
    // runtime keeps 2.50 as 2 and 3.50 as 4.
    let (root, name) = shop(
        "scaled",
        &[("whole", "type Whole = Decimal {\n    scale: 0\n}\n")],
        "use shop/types { Amount, Cost, Whole }\n\nlibrary totals {\n    function whole_total(cost: Cost) -> Amount {\n        var whole: Whole = Whole(cost)\n        return Amount(whole)\n    }\n}\n",
        "totals.whole_total(Item.cost)",
    );
    let sql = fs::read_to_string(root.join(format!("database/migrations/{name}/postgres.sql")))
        .expect("postgres.sql");
    assert!(sql.contains("-- backfill Item.total: by UPDATE, "), "{sql}");
    if !jadpo_test::live() {
        return;
    }
    let totals = shop_totals(&root, "backfill_scaled", &HALVES);
    let runtime = |cost: &str| -> String {
        let cost = jadpo_value::Decimal::parse(cost).expect("a cost");
        let whole = cost
            .rescale(0, jadpo_value::Rounding::HalfEven)
            .expect("a whole number");
        whole
            .rescale(4, jadpo_value::Rounding::HalfEven)
            .expect("an amount")
            .to_string()
    };
    let expected: Vec<Vec<String>> = HALVES
        .iter()
        .map(|cost| vec![cost.to_string(), runtime(cost)])
        .collect();
    assert_eq!(totals, expected);
    assert_eq!(totals[1], vec!["-2.50".to_string(), "-2.0000".to_string()]);
    assert_eq!(totals[8], vec!["3.50".to_string(), "4.0000".to_string()]);
}

#[test]
fn a_constant_built_with_a_scale_rounds_as_area_2_says() {
    // The review's `Rate(0.10 * 1.25)`: 0.125 kept at two places is 0.12.
    let (root, _) = shop(
        "constant-scale",
        &[("rate", "type Rate = Decimal {\n    scale: 2\n}\n")],
        "use shop/types { Amount, Cost, Rate }\n\nlibrary totals {\n    constant rate: Rate = Rate(0.10 * 1.25)\n\n    function rated(cost: Cost) -> Amount {\n        return Amount(rate)\n    }\n}\n",
        "totals.rated(Item.cost)",
    );
    if !jadpo_test::live() {
        return;
    }
    let totals = shop_totals(&root, "backfill_constant_scale", &["1.00", "2.00"]);
    assert_eq!(
        totals,
        vec![
            vec!["1.00".to_string(), "0.1200".to_string()],
            vec!["2.00".to_string(), "0.1200".to_string()],
        ]
    );
}

#[test]
fn a_backfill_in_jadpos_digest_covers_the_types_it_reaches() {
    // The review's `var mut` total: worked out in Jadpo, through `Whole`.
    let whole = |scale: u32| format!("type Whole = Decimal {{\n    scale: {scale}\n}}\n");
    let (root, name) = shop(
        "digest-types",
        &[("whole", &whole(1))],
        "use shop/types { Amount, Cost, Whole }\n\nlibrary totals {\n    function whole_total(cost: Cost) -> Amount {\n        var mut whole: Whole = Whole(cost)\n        whole = Whole(cost)\n        return Amount(whole)\n    }\n}\n",
        "totals.whole_total(Item.cost)",
    );
    let sql = fs::read_to_string(root.join(format!("database/migrations/{name}/postgres.sql")))
        .expect("postgres.sql");
    assert!(sql.contains("-- backfill Item.total: in Jadpo, "), "{sql}");
    // `Whole` keeps fewer places now: rows filled before would hold
    // values the rows filled after don't, so the migration must change.
    write(&root, "domains/shop/types/whole.jadpo", &whole(0));
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

/// `Item` with two whole numbers, then a migration adding two more, each
/// worked out from one of them by `UPDATE`.
fn two_backfills(name: &str) -> (PathBuf, String) {
    let root = folder(name);
    write(&root, "app.jadpo", "application {}\n");
    write(
        &root,
        ITEM,
        "entity Item {\n    id: Uuid\n    a: Int\n    b: Int\n}\n",
    );
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    write(
        &root,
        ITEM,
        "entity Item {\n    id: Uuid\n    a: Int\n    b: Int\n    q: Int\n    r: Int\n}\n",
    );
    assert_eq!(jadpo(&root, &["migrate", "new", "add_qr"]).code, 0);
    let name = latest(&root);
    write(
        &root,
        &format!("database/migrations/{name}/changes.jadpo"),
        "migration {\n    Item.q: {\n        backfill: Item.a + 1\n    }\n    Item.r: {\n        backfill: Item.b * 2\n    }\n}\n",
    );
    let finished = jadpo(&root, &["migrate", "new"]);
    assert_eq!(finished.code, 0, "{}{}", finished.stdout, finished.stderr);
    assert_eq!(jadpo(&root, &["check"]).code, 0);
    (root, name)
}

#[test]
fn every_backfill_on_a_table_fills_before_any_of_its_columns_is_required() {
    let (root, name) = two_backfills("two");
    let sql = fs::read_to_string(root.join(format!("database/migrations/{name}/postgres.sql")))
        .expect("postgres.sql");
    // Both backfills' batches come before the one step that locks the
    // table and requires a value.
    let lock = sql.find("LOCK TABLE").expect("a lock");
    assert!(sql.matches("LOCK TABLE").count() == 1, "{sql}");
    assert!(sql.find("-- backfill Item.q").expect("q") < lock, "{sql}");
    assert!(sql.find("-- backfill Item.r").expect("r") < lock, "{sql}");
    assert!(sql.find("SET NOT NULL").expect("required") > lock, "{sql}");
    if !jadpo_test::live() {
        return;
    }
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server.create_database("backfill_two").expect("database");
    let migrations = steps(&root);
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    let store = jadpo_host_native::postgres(&url).expect("store");
    runtime
        .block_on(jadpo_runtime::migrate(&store, &migrations[..1]))
        .expect("the first migration");
    statement(
        &url,
        "INSERT INTO \"item\" (\"id\", \"a\", \"b\") SELECT gen_random_uuid(), n, n + 1 FROM generate_series(1, 2500) AS n",
    );
    // Stopped as the second backfill starts: the first has filled every
    // row it found.
    let mut started = 0;
    let stopped = runtime.block_on(jadpo_runtime::migrate_reporting(
        &store,
        &migrations,
        &mut |progress| {
            if matches!(progress, jadpo_runtime::Progress::Filling { .. }) {
                started += 1;
                if started == 2 {
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
    let nullable = |column: &str| {
        rows(
            &url,
            &format!("SELECT is_nullable::text FROM information_schema.columns WHERE table_name = 'item' AND column_name = '{column}'"),
        )[0][0]
            .clone()
    };
    assert_eq!(nullable("q"), "YES", "no column is required yet");
    assert_eq!(nullable("r"), "YES");
    // A server still on the old build writes an item meanwhile, knowing
    // neither new field; nothing refuses it.
    statement(
        &url,
        "INSERT INTO \"item\" (\"id\", \"a\", \"b\") VALUES ('00000000-0000-0000-0000-00000000000a', 7, 8)",
    );
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    assert_eq!(
        rows(
            &url,
            "SELECT count(*) FROM \"item\" WHERE \"q\" IS DISTINCT FROM \"a\" + 1 OR \"r\" IS DISTINCT FROM \"b\" * 2"
        ),
        vec![vec!["0".to_string()]]
    );
    assert_eq!(
        rows(&url, "SELECT count(*) FROM \"item\""),
        vec![vec!["2501".to_string()]]
    );
    assert_eq!(nullable("q"), "NO");
    assert_eq!(nullable("r"), "NO");
}

/// What `seq_scan` says this transaction has read of `todo` so far.
async fn scans(transaction: &mut dyn jadpo_runtime::Transaction) -> i64 {
    let found = transaction
        .query(
            "SELECT seq_scan FROM pg_stat_xact_user_tables WHERE relname = 'todo'",
            &[],
        )
        .await
        .expect("scans");
    match found.first().map(Vec::as_slice) {
        Some([SqlValue::Int8(n)]) => *n,
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_last_step_locks_writes_and_requires_a_value_without_reading_the_table() {
    let root = project("last-step");
    let name = priority_from_flag(&root, "priorities.from_flag(LegacyFlag(Todo.legacy_flag))");
    assert_eq!(jadpo(&root, &["migrate", "new"]).code, 0);
    let migrations = steps(&root);
    let second = migrations
        .iter()
        .find(|migration| migration.name == name)
        .expect("the second migration");
    // The rows written meanwhile are filled with writes locked out and a
    // check that every row has a value added unchecked; the check is then
    // proved in a step of its own, which locks out neither reads nor
    // writes; then the column is required, which the proved check lets
    // PostgreSQL do without reading a row, and the check goes.
    let parts = second.steps();
    let once = |at: usize| match parts.get(at) {
        Some(jadpo_std::Step::Once(sql)) => sql.clone(),
        other => panic!("step {at}: {other:?}"),
    };
    assert_eq!(parts.len(), 5, "{parts:?}");
    let fill = once(2);
    assert!(
        fill.starts_with(
            "LOCK TABLE \"todo\" IN SHARE ROW EXCLUSIVE MODE;\nUPDATE \"todo\" SET \"priority\" = "
        ),
        "{fill}"
    );
    assert!(
        fill.ends_with("ALTER TABLE \"todo\" ADD CONSTRAINT \"todo_priority_filled\" CHECK (\"priority\" IS NOT NULL) NOT VALID;"),
        "{fill}"
    );
    assert_eq!(
        once(3),
        "ALTER TABLE \"todo\" VALIDATE CONSTRAINT \"todo_priority_filled\";"
    );
    assert_eq!(
        once(4),
        "ALTER TABLE \"todo\" ALTER COLUMN \"priority\" SET NOT NULL;\nALTER TABLE \"todo\" DROP CONSTRAINT \"todo_priority_filled\";"
    );
    if !jadpo_test::live() {
        return;
    }
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server
        .create_database("backfill_last_step")
        .expect("database");
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    let store = jadpo_host_native::postgres(&url).expect("store");
    runtime
        .block_on(jadpo_runtime::migrate(&store, &migrations[..1]))
        .expect("the first migration");
    todos(&url, 1500);
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    let (counts, nullable) = priorities(&url);
    assert_eq!(counts.len(), 2, "{counts:?}");
    assert_eq!(nullable, "NO");
    assert_eq!(
        rows(
            &url,
            "SELECT count(*) FROM pg_constraint WHERE conname = 'todo_priority_filled'"
        ),
        vec![vec!["0".to_string()]],
        "the check goes once the column is required"
    );
    // The last three steps again, each in its transaction as `migrate
    // apply` runs it, watched from inside.
    statement(
        &url,
        "ALTER TABLE \"todo\" ALTER COLUMN \"priority\" DROP NOT NULL",
    );
    runtime.block_on(async {
        let mut transaction = store.begin().await.expect("begin");
        transaction.batch(&fill).await.expect("fill");
        transaction.commit().await.expect("commit");
        // Proving the check reads every row, holding only the lock that
        // lets reads and writes go on.
        let mut transaction = store.begin().await.expect("begin");
        transaction.batch(&once(3)).await.expect("validate");
        let locks = transaction
            .query(
                "SELECT mode FROM pg_locks WHERE relation = 'todo'::regclass AND pid = pg_backend_pid() ORDER BY mode",
                &[],
            )
            .await
            .expect("locks");
        assert_eq!(
            locks,
            vec![vec![SqlValue::Text("ShareUpdateExclusiveLock".into())]]
        );
        transaction.commit().await.expect("commit");
        // Requiring a value reads no row.
        let mut transaction = store.begin().await.expect("begin");
        let before = scans(transaction.as_mut()).await;
        transaction.batch(&once(4)).await.expect("require");
        assert_eq!(scans(transaction.as_mut()).await, before);
        transaction.commit().await.expect("commit");
        // Without the check, it reads the table.
        let mut transaction = store.begin().await.expect("begin");
        transaction
            .batch("ALTER TABLE \"todo\" ALTER COLUMN \"priority\" DROP NOT NULL")
            .await
            .expect("optional");
        let before = scans(transaction.as_mut()).await;
        transaction
            .batch("ALTER TABLE \"todo\" ALTER COLUMN \"priority\" SET NOT NULL")
            .await
            .expect("required");
        assert_eq!(scans(transaction.as_mut()).await, before + 1);
        transaction.rollback().await.expect("rollback");
    });
}

#[test]
fn a_backfill_reaching_a_setting_a_key_or_a_field_its_migration_adds_is_refused() {
    let root = folder("outside-row");
    write(&root, "app.jadpo", "application {}\n");
    write(
        &root,
        "domains/planning/types/label.jadpo",
        "type Label = Text {}\n",
    );
    write(
        &root,
        "domains/planning/types/title.jadpo",
        "type Title = Text {}\n",
    );
    write(
        &root,
        "config.jadpo",
        "use planning/types { Label }\n\nconfig {\n    hook_key: SigningKey\n    label_prefix: Label {\n        default: \"todo\"\n    }\n}\n",
    );
    write(
        &root,
        TODO,
        "use planning/types { Title }\n\nentity Todo {\n    id: Uuid\n    title: Title\n}\n",
    );
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    write(
        &root,
        "domains/planning/types/mac.jadpo",
        "type Mac = Bytes {}\n",
    );
    write(
        &root,
        TODO,
        "use planning/types { Label, Mac, Title }\n\nentity Todo {\n    id: Uuid\n    title: Title\n    label: Label\n    heading: Label\n    mac: Mac\n}\n",
    );
    assert_eq!(jadpo(&root, &["migrate", "new", "add_label"]).code, 0);
    write(
        &root,
        "domains/planning/libraries/labels.jadpo",
        "use planning/types { Label, Mac, Title }\n\nlibrary labels {\n    function prefix(title: Title) -> Label {\n        return config.label_prefix\n    }\n\n    function signed(title: Title) -> Mac {\n        return Mac(crypto.hmac(title, key: config.hook_key))\n    }\n}\n",
    );
    let name = latest(&root);
    write(
        &root,
        &format!("database/migrations/{name}/changes.jadpo"),
        "migration {\n    Todo.label: {\n        backfill: labels.prefix(Todo.title)\n    }\n    Todo.heading: {\n        backfill: Todo.label\n    }\n    Todo.mac: {\n        backfill: labels.signed(Todo.title)\n    }\n}\n",
    );
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 1, "{}", checked.stdout);
    for (field, read, label) in [
        (
            "Todo.label",
            "config.label_prefix",
            "reads a setting, through `labels.prefix`",
        ),
        (
            "Todo.mac",
            "config.hook_key",
            "reads a setting, through `labels.signed`",
        ),
        ("Todo.heading", "Todo.label", "a field this migration adds"),
    ] {
        assert!(
            checked.stdout.contains(&format!(
                "error: The backfill for `{field}` can't read `{read}`\n"
            )) && checked.stdout.contains(label),
            "{field}: {}",
            checked.stdout
        );
    }
    assert_eq!(
        checked
            .stdout
            .matches("rule: migration.backfill_outside_row")
            .count(),
        3,
        "{}",
        checked.stdout
    );
    assert!(
        !checked.stdout.contains("not_built_yet"),
        "{}",
        checked.stdout
    );
}

#[test]
fn a_backfill_in_jadpo_reads_and_fills_a_percentage_as_a_query_reads_one() {
    // A backfill in Jadpo reads and writes a column through the one table
    // a query's read uses (chunk 5.8b), so a percentage, held as the
    // fraction it is in one column, is no longer refused as not built.
    let root = folder("percentage");
    write(&root, "app.jadpo", "application {}\n");
    write(
        &root,
        "domains/shop/types/share.jadpo",
        "type Share = Percentage {}\n",
    );
    write(
        &root,
        ITEM,
        "use shop/types { Share }\n\nentity Item {\n    id: Uuid\n    share: Share\n}\n",
    );
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    write(
        &root,
        ITEM,
        "use shop/types { Share }\n\nentity Item {\n    id: Uuid\n    share: Share\n    discount: Share\n}\n",
    );
    assert_eq!(jadpo(&root, &["migrate", "new", "add_discount"]).code, 0);
    let name = latest(&root);
    write(
        &root,
        &format!("database/migrations/{name}/changes.jadpo"),
        "migration {\n    Item.discount: {\n        backfill: Item.share\n    }\n}\n",
    );
    let finished = jadpo(&root, &["migrate", "new"]);
    assert_eq!(finished.code, 0, "{}{}", finished.stdout, finished.stderr);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    let sql = fs::read_to_string(root.join(format!("database/migrations/{name}/postgres.sql")))
        .expect("postgres.sql");
    assert!(
        sql.contains("-- backfill Item.discount: in Jadpo, "),
        "{sql}"
    );
    if !jadpo_test::live() {
        return;
    }
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server
        .create_database("backfill_percentage")
        .expect("database");
    let migrations = steps(&root);
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    let store = jadpo_host_native::postgres(&url).expect("store");
    runtime
        .block_on(jadpo_runtime::migrate(&store, &migrations[..1]))
        .expect("the first migration");
    statement(
        &url,
        "INSERT INTO \"item\" (\"id\", \"share\") VALUES (gen_random_uuid(), 0.25), (gen_random_uuid(), 0.125), (gen_random_uuid(), 1.5)",
    );
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    assert_eq!(
        rows(
            &url,
            "SELECT \"share\"::text, \"discount\"::text FROM \"item\" ORDER BY \"item\".\"share\""
        ),
        vec![
            vec!["0.125".to_string(), "0.125".to_string()],
            vec!["0.25".to_string(), "0.25".to_string()],
            vec!["1.5".to_string(), "1.5".to_string()],
        ]
    );
}
