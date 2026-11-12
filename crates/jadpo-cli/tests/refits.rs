//! Refits (area 5, "Changing what a field allows"; chunk Q3): a field
//! that allows less than it did gives the records already stored, whose
//! value it no longer allows, one it does. A new `pattern:` is one, which
//! only Jadpo finds the values of, since PostgreSQL's regular expressions
//! aren't Jadpo's: `jadpo migrate apply` reads every stored value in
//! batches, checks it with the field's rules now and writes the
//! backfill's value back for those they refuse. A backfill worked out from
//! each record refits too, by `UPDATE` in batches where it has a SQL form
//! and in Jadpo otherwise, reading the field it refits as the built-in
//! type its column holds. The live tests need PostgreSQL, which
//! `JADPO_SKIP_POSTGRES=1` skips.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_runtime::{Database as _, SqlValue};
use jadpo_term::Environment;

use crate::support;

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("refit-{name}"));
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

fn ok(out: &Output) {
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
}

const ITEM: &str = "domains/shop/entities/item.jadpo";

/// A project whose first migration made `Item` as `first` says, and whose
/// second, `second`, is written once its entity says `then`: the second
/// migration's folder name.
fn two_migrations(root: &Path, first: &str, then: &str) -> String {
    two_migrations_with(root, first, then, &[])
}

/// As [`two_migrations`], with `files` written for the second.
fn two_migrations_with(root: &Path, first: &str, then: &str, files: &[(&str, &str)]) -> String {
    write(root, "app.jadpo", "application {}\n");
    write(root, ITEM, first);
    ok(&jadpo(root, &["migrate", "new", "initial"]));
    for (path, text) in files {
        write(root, path, text);
    }
    write(root, ITEM, then);
    let second = jadpo(root, &["migrate", "new", "second"]);
    assert!(
        second.stdout.contains("Wrote database/migrations/"),
        "{}{}",
        second.stdout,
        second.stderr
    );
    latest(root)
}

fn latest(root: &Path) -> String {
    let mut names: Vec<String> = fs::read_dir(root.join("database/migrations"))
        .expect("migrations")
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect();
    names.sort();
    names.pop().expect("a migration")
}

fn file(root: &Path, name: &str, file: &str) -> String {
    fs::read_to_string(root.join(format!("database/migrations/{name}/{file}"))).expect(file)
}

fn decide(root: &Path, name: &str, decision: &str) {
    write(
        root,
        &format!("database/migrations/{name}/changes.jadpo"),
        &format!("migration {{\n    {decision}\n}}\n"),
    );
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
            let sql = file_at(root, &name);
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

fn file_at(root: &Path, name: &str) -> String {
    file(root, name, "postgres.sql")
}

/// Applies the first migration only, to a database of its own.
fn first_applied(root: &Path, database: &str) -> String {
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server.create_database(database).expect("database");
    let migrations = steps(root);
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    let store = jadpo_host_native::postgres(&url).expect("store");
    runtime
        .block_on(jadpo_runtime::migrate(&store, &migrations[..1]))
        .expect("the first migration");
    url
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
                        SqlValue::Null(_) => "NULL".to_string(),
                        other => format!("{other:?}"),
                    })
                    .collect()
            })
            .collect()
    })
}

/// Whether `out` has a row for `name` saying `what`, under the migration.
fn said(out: &str, name: &str, what: &str) -> bool {
    out.lines().any(|line| {
        line.starts_with("      ")
            && line.trim_start().starts_with(name)
            && line.ends_with(&format!("  {what}"))
    })
}

/// Items, each with its `code`: those named, then `more` that read `OK`.
fn items(url: &str, column: &str, values: &[&str], more: u32) {
    for value in values {
        statement(
            url,
            &format!(
                "INSERT INTO \"item\" (\"id\", \"{column}\") VALUES (gen_random_uuid(), '{value}')"
            ),
        );
    }
    statement(
        url,
        &format!(
            "INSERT INTO \"item\" (\"id\", \"{column}\") SELECT gen_random_uuid(), 'OK' FROM generate_series(1, {more})"
        ),
    );
}

fn values(url: &str, column: &str) -> Vec<String> {
    rows(
        url,
        &format!(
            "SELECT \"{column}\" FROM \"item\" WHERE \"{column}\" <> 'OK' ORDER BY \"{column}\""
        ),
    )
    .into_iter()
    .map(|row| row[0].clone())
    .collect()
}

const PLAIN_CODE: &str = "entity Item {\n    id: Uuid\n    code: Text\n}\n";
const CODE_TO_PATTERN: &str =
    "entity Item {\n    id: Uuid\n    code: Text {\n        pattern: \"[A-Z]+\"\n    }\n}\n";

#[test]
fn a_new_pattern_needs_a_backfill_and_jadpo_replaces_the_values_it_refuses() {
    let root = folder("pattern");
    let name = two_migrations(&root, PLAIN_CODE, CODE_TO_PATTERN);
    // It allows less, so it needs a decision, as a shorter `max_length`
    // does.
    let changes = file(&root, &name, "changes.jadpo");
    assert!(changes.contains("Item.code: {}"), "{changes}");
    let checked = jadpo(&root, &["check"]);
    assert_ne!(checked.code, 0);
    assert!(
        checked.stdout.contains("migration.decision_required"),
        "{}",
        checked.stdout
    );
    assert!(
        !checked.stdout.contains("not_built_yet"),
        "{}",
        checked.stdout
    );
    decide(
        &root,
        &name,
        "Item.code: {\n        backfill: \"UNKNOWN\"\n    }",
    );
    let finished = jadpo(&root, &["migrate", "new"]);
    ok(&finished);
    assert!(
        finished
            .stdout
            .contains("replace values no longer allowed in batches in Jadpo"),
        "{}",
        finished.stdout
    );
    let sql = file_at(&root, &name);
    assert!(
        sql.contains("-- rewrite Item.code: in Jadpo, 1000 rows a batch, each batch its own transaction, replacing the stored values the field's rules no longer allow; its code's digest "),
        "{sql}"
    );
    assert!(
        sql.contains("SELECT \"id\", \"code\" FROM \"item\" WHERE \"id\" > $1 AND \"code\" IS NOT NULL ORDER BY \"id\" LIMIT 1000 FOR NO KEY UPDATE;\n"),
        "{sql}"
    );
    // No PostgreSQL regular expression finds them.
    assert!(!sql.contains('~'), "{sql}");
    jadpo(&root, &["records", "update"]);
    ok(&jadpo(&root, &["check"]));
    if !jadpo_test::live() {
        return;
    }
    let url = first_applied(&root, "refit_pattern");
    // `ABC` keeps to the pattern; the rest don't, `ÄBC` among them, since
    // `[A-Z]` is ASCII in Jadpo.
    items(&url, "code", &["ABC", "abc", "A-1", "ÄBC"], 1200);
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    ok(&applied);
    assert!(
        said(
            &applied.stdout,
            "Item.code",
            "3 rows rewritten in 2 batches in Jadpo"
        ),
        "{}",
        applied.stdout
    );
    assert_eq!(
        values(&url, "code"),
        ["ABC", "UNKNOWN", "UNKNOWN", "UNKNOWN"]
    );
}

#[test]
fn a_backfill_that_breaks_the_new_pattern_is_refused() {
    let root = folder("pattern-literal");
    let name = two_migrations(&root, PLAIN_CODE, CODE_TO_PATTERN);
    decide(
        &root,
        &name,
        "Item.code: {\n        backfill: \"unknown\"\n    }",
    );
    let checked = jadpo(&root, &["check"]);
    assert_ne!(checked.code, 0, "{}", checked.stdout);
    assert!(checked.stdout.contains("pattern"), "{}", checked.stdout);
}

const SCORED: &str =
    "use shop/types { Score }\n\nentity Item {\n    id: Uuid\n    score: Score\n}\n";

#[test]
fn a_computed_backfill_replaces_by_update_the_values_a_field_no_longer_allows() {
    let root = folder("by-update");
    let score = "domains/shop/types/score.jadpo";
    write(
        &root,
        score,
        "type Score = Int {\n    min: -1000\n    max: 1000\n}\n",
    );
    let name = two_migrations_with(
        &root,
        SCORED,
        SCORED,
        &[(score, "type Score = Int {\n    min: 0\n    max: 10\n}\n")],
    );
    // The field it refits is read as its column holds it, `Int`, since a
    // stored value may be outside what `Score` allows.
    decide(
        &root,
        &name,
        "Item.score: {\n        backfill: Score(integer.max(integer.min(Item.score, 10), 0))\n    }",
    );
    let finished = jadpo(&root, &["migrate", "new"]);
    ok(&finished);
    assert!(
        finished
            .stdout
            .contains("replace values no longer allowed in batches"),
        "{}",
        finished.stdout
    );
    let sql = file_at(&root, &name);
    assert!(
        sql.contains("-- rewrite Item.score: by UPDATE, 1000 rows a batch, each batch its own transaction, replacing the stored values the field's rules no longer allow\nSELECT count(*) FROM \"item\" WHERE \"score\" < 0 OR \"score\" > 10;\n"),
        "{sql}"
    );
    assert!(
        sql.contains("AND (\"score\" < 0 OR \"score\" > 10) RETURNING 1)"),
        "{sql}"
    );
    jadpo(&root, &["records", "update"]);
    ok(&jadpo(&root, &["check"]));
    if !jadpo_test::live() {
        return;
    }
    let url = first_applied(&root, "refit_by_update");
    for score in [3, 42, -7, 10] {
        statement(
            &url,
            &format!(
                "INSERT INTO \"item\" (\"id\", \"score\") VALUES (gen_random_uuid(), {score})"
            ),
        );
    }
    statement(
        &url,
        "INSERT INTO \"item\" (\"id\", \"score\") SELECT gen_random_uuid(), 5 FROM generate_series(1, 1200)",
    );
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    ok(&applied);
    assert!(
        said(
            &applied.stdout,
            "Item.score",
            "2 rows rewritten in 2 batches by UPDATE"
        ),
        "{}",
        applied.stdout
    );
    let scores: Vec<String> = rows(
        &url,
        "SELECT \"score\"::text FROM \"item\" WHERE \"score\" <> 5 ORDER BY \"item\".\"score\"",
    )
    .into_iter()
    .map(|row| row[0].clone())
    .collect();
    assert_eq!(scores, ["0", "3", "10", "10"]);
}

#[test]
fn a_computed_backfill_for_a_new_pattern_is_worked_out_in_jadpo() {
    let root = folder("in-jadpo");
    write(
        &root,
        "domains/shop/types/big.jadpo",
        "type Big = Bool {}\n",
    );
    let name = two_migrations_with(
        &root,
        "use shop/types { Big }\n\nentity Item {\n    id: Uuid\n    code: Text\n    big: Big\n}\n",
        "use shop/types { Big, Code }\n\nentity Item {\n    id: Uuid\n    code: Code\n    big: Big\n}\n",
        &[(
            "domains/shop/types/code.jadpo",
            "type Code = Text {\n    pattern: \"[A-Z]+\"\n}\n",
        )],
    );
    write(
        &root,
        "domains/shop/libraries/codes.jadpo",
        "use shop/types { Big, Code }\n\nlibrary codes {\n    function by_size(big: Big) -> Code {\n        if (big) {\n            return Code(\"BIG\")\n        }\n        return Code(\"SMALL\")\n    }\n}\n",
    );
    // It has a SQL form, but only Jadpo finds the values a new pattern
    // refuses.
    decide(
        &root,
        &name,
        "Item.code: {\n        backfill: codes.by_size(Item.big)\n    }",
    );
    ok(&jadpo(&root, &["migrate", "new"]));
    let sql = file_at(&root, &name);
    assert!(
        sql.contains("-- rewrite Item.code: in Jadpo, 1000 rows a batch"),
        "{sql}"
    );
    // The batch reads the stored value, then the field the backfill reads.
    assert!(
        sql.contains("SELECT \"id\", \"code\", \"big\" FROM \"item\""),
        "{sql}"
    );
    jadpo(&root, &["records", "update"]);
    ok(&jadpo(&root, &["check"]));
    if !jadpo_test::live() {
        return;
    }
    let url = first_applied(&root, "refit_in_jadpo");
    for (code, big) in [("KEEP", true), ("lower", true), ("a-1", false)] {
        statement(
            &url,
            &format!(
                "INSERT INTO \"item\" (\"id\", \"code\", \"big\") VALUES (gen_random_uuid(), '{code}', {big})"
            ),
        );
    }
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    ok(&applied);
    assert!(
        said(
            &applied.stdout,
            "Item.code",
            "2 rows rewritten in 1 batch in Jadpo"
        ),
        "{}",
        applied.stdout
    );
    let codes: Vec<String> = rows(&url, "SELECT \"code\" FROM \"item\" ORDER BY \"code\"")
        .into_iter()
        .map(|row| row[0].clone())
        .collect();
    assert_eq!(codes, ["BIG", "KEEP", "SMALL"]);
}

#[test]
fn a_backfill_doesnt_read_an_enum_field_its_migration_refits() {
    let root = folder("enum");
    write(&root, "app.jadpo", "application {}\n");
    write(
        &root,
        "domains/shop/types/size.jadpo",
        "type Size = Enum {\n    small\n    medium\n    large\n}\n",
    );
    write(
        &root,
        ITEM,
        "use shop/types { Size }\n\nentity Item {\n    id: Uuid\n    size: Size\n    big: Bool\n}\n",
    );
    ok(&jadpo(&root, &["migrate", "new", "initial"]));
    write(
        &root,
        "domains/shop/types/size.jadpo",
        "type Size = Enum {\n    small\n    medium\n}\n",
    );
    jadpo(&root, &["migrate", "new", "drop_large"]);
    let name = latest(&root);
    decide(
        &root,
        &name,
        "Item.size: {\n        backfill: Item.size\n    }",
    );
    let checked = jadpo(&root, &["check"]);
    assert_ne!(checked.code, 0);
    assert!(
        checked.stdout.contains("migration.backfill_outside_row")
            && checked.stdout.contains("a field this migration refits"),
        "{}",
        checked.stdout
    );
}
