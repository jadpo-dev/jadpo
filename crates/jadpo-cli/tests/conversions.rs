//! Changing a column's type (area 5, "Changing a column's type"; chunk
//! Q3b): a column whose type changes other than by widening is never
//! altered in place, since servers still on the old build read and write
//! it. The migration adds a new column of the new type, `<field>__2`, and
//! fills it from the old one through the backfill, which reads the field
//! as the built-in type its old column holds; the new build reads and
//! writes the new column, and the old one stays, retired, until a later
//! migration drops it. A backfill worked out in Jadpo also fills a value
//! held in several columns, an amount of money's two, and reads one held
//! so or as JSON. The live tests need PostgreSQL, which
//! `JADPO_SKIP_POSTGRES=1` skips.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_runtime::{Database as _, SqlValue};
use jadpo_term::Environment;

use crate::support;

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("conversion-{name}"));
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
/// second is written once its entity says `then`: the second migration's
/// folder name and what `jadpo migrate new` said.
fn two_migrations(root: &Path, first: &str, then: &str) -> (String, Output) {
    write(root, "app.jadpo", "application {}\n");
    write(root, ITEM, first);
    ok(&jadpo(root, &["migrate", "new", "initial"]));
    write(root, ITEM, then);
    let second = jadpo(root, &["migrate", "new", "second"]);
    assert!(
        second.stdout.contains("Wrote database/migrations/"),
        "{}{}",
        second.stdout,
        second.stderr
    );
    (latest(root), second)
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

fn decide(root: &Path, name: &str, decisions: &str) {
    write(
        root,
        &format!("database/migrations/{name}/changes.jadpo"),
        &format!("migration {{\n{decisions}}}\n"),
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
            let sql = file(root, &name, "postgres.sql");
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
                        SqlValue::Bool(b) => b.to_string(),
                        SqlValue::Null(_) => "NULL".to_string(),
                        other => format!("{other:?}"),
                    })
                    .collect()
            })
            .collect()
    })
}

/// Whether each of `columns` of `item` may be `NULL`, in order.
fn nullable(url: &str, columns: &[&str]) -> Vec<String> {
    columns
        .iter()
        .map(|column| {
            rows(
                url,
                &format!(
                    "SELECT is_nullable::text FROM information_schema.columns WHERE table_name = 'item' AND column_name = '{column}'"
                ),
            )[0][0]
                .clone()
        })
        .collect()
}

const CODE_INT: &str = "entity Item {\n    id: Uuid\n    code: Int\n    note: Int?\n}\n";
const CODE_TEXT: &str = "entity Item {\n    id: Uuid\n    code: Text\n    note: Text?\n}\n";

#[test]
fn a_column_whose_type_changes_moves_to_a_new_column_filled_from_the_old_one() {
    let root = folder("by-update");
    let (name, second) = two_migrations(&root, CODE_INT, CODE_TEXT);
    // The old columns retire with nothing to decide; each field takes a
    // new one, and needs its values worked out from the old.
    for line in [
        "- code     bigint  retired: kept for servers on the old build",
        "+ code__2  text    new type: filled from the old column",
        "decision: `Item.code` changes type: existing `Item` records need its new value",
        "decision: `Item.note` changes type: existing `Item` records need its new value",
    ] {
        assert!(second.stdout.contains(line), "{line}\n{}", second.stdout);
    }
    assert!(
        !second.stdout.contains("retire its column"),
        "{}",
        second.stdout
    );
    let changes = file(&root, &name, "changes.jadpo");
    assert!(
        changes.contains("// `Item.code` changes type: its values move to a new column, and\n"),
        "{changes}"
    );
    // Even a field that may be `none` gets a decision, since its stored
    // values would otherwise be lost.
    decide(
        &root,
        &name,
        "    Item.code: {\n        backfill: text.from(Item.code)\n    }\n    Item.note: {\n        backfill: none\n    }\n",
    );
    ok(&jadpo(&root, &["migrate", "new"]));
    let sql = file(&root, &name, "postgres.sql");
    for expected in [
        "ALTER TABLE \"item\" ALTER COLUMN \"code\" DROP NOT NULL;\n",
        "ALTER TABLE \"item\" ADD COLUMN \"code__2\" text;\n",
        "ALTER TABLE \"item\" ADD COLUMN \"note__2\" text;\n",
        "-- backfill Item.code: by UPDATE, 1000 rows a batch, each batch its own transaction\n",
        "UPDATE \"item\" SET \"code__2\" = (\"item\".\"code\")::text FROM \"batch\"",
        "ALTER TABLE \"item\" ALTER COLUMN \"code__2\" SET NOT NULL;\n",
    ] {
        assert!(sql.contains(expected), "{expected}\n{sql}");
    }
    // The old column is never altered in place, and a field that may be
    // `none` isn't made required.
    assert!(!sql.contains("TYPE text"), "{sql}");
    assert!(!sql.contains("\"note__2\" SET NOT NULL"), "{sql}");
    jadpo(&root, &["records", "update"]);
    ok(&jadpo(&root, &["check"]));
    // Nothing more to migrate: the source asks for the new columns.
    let again = jadpo(&root, &["migrate", "new", "again"]);
    assert!(!again.stdout.contains("Wrote"), "{}", again.stdout);
    let snapshot = file(&root, &name, "snapshot.json");
    assert!(snapshot.contains("\"code__2\""), "{snapshot}");
    if !jadpo_test::live() {
        return;
    }
    let url = first_applied(&root, "conversion_by_update");
    statement(
        &url,
        "INSERT INTO \"item\" (\"id\", \"code\", \"note\") SELECT gen_random_uuid(), n, NULL FROM generate_series(1, 1200) AS n",
    );
    ok(&jadpo(&root, &["migrate", "apply", "--url", &url]));
    assert_eq!(
        rows(
            &url,
            "SELECT count(*) FROM \"item\" WHERE \"code__2\" = \"code\"::text"
        ),
        [["1200"]]
    );
    // The old column stays, with its values, for servers on the old build.
    assert_eq!(
        rows(
            &url,
            "SELECT count(*) FROM \"item\" WHERE \"code\" IS NOT NULL"
        ),
        [["1200"]]
    );
    assert_eq!(
        nullable(&url, &["code", "code__2", "note__2"]),
        ["YES", "NO", "YES"]
    );
    // The old column goes in a later migration, once no server reads it.
    let dropped = jadpo(
        &root,
        &["migrate", "new", "drop_code", "--drop", "Item.code"],
    );
    ok(&dropped);
    let drop = file(&root, &latest(&root), "postgres.sql");
    assert!(
        drop.contains("ALTER TABLE \"item\" DROP COLUMN \"code\";\n"),
        "{drop}"
    );
    assert!(!drop.contains("code__2"), "{drop}");
}

#[test]
fn a_conversion_in_jadpo_reads_the_old_column_and_fills_the_new_one() {
    let root = folder("in-jadpo");
    let (name, _) = two_migrations(&root, CODE_INT, CODE_TEXT);
    decide(
        &root,
        &name,
        "    Item.code: {\n        backfill: text.upper(text.from(Item.code))\n    }\n    Item.note: {\n        backfill: none\n    }\n",
    );
    ok(&jadpo(&root, &["migrate", "new"]));
    let sql = file(&root, &name, "postgres.sql");
    assert!(
        sql.contains("-- backfill Item.code: in Jadpo, 1000 rows a batch"),
        "{sql}"
    );
    assert!(
        sql.contains(
            "SELECT \"id\", \"code\" FROM \"item\" WHERE \"id\" > $1 AND \"code__2\" IS NULL"
        ),
        "{sql}"
    );
    jadpo(&root, &["records", "update"]);
    ok(&jadpo(&root, &["check"]));
    if !jadpo_test::live() {
        return;
    }
    let url = first_applied(&root, "conversion_in_jadpo");
    statement(
        &url,
        "INSERT INTO \"item\" (\"id\", \"code\") SELECT gen_random_uuid(), n FROM generate_series(1, 1500) AS n",
    );
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    ok(&applied);
    assert!(
        applied
            .stdout
            .contains("1500 rows filled in 2 batches in Jadpo"),
        "{}",
        applied.stdout
    );
    assert_eq!(
        rows(
            &url,
            "SELECT count(*) FROM \"item\" WHERE \"code__2\" = \"code\"::text"
        ),
        [["1500"]]
    );
}

#[test]
fn an_amount_of_money_is_filled_in_both_its_columns_and_read_from_them() {
    // `price` becomes an amount of money, two columns in place of one,
    // and a new required `cost` is worked out from it as stored; `total`
    // reads `cost`'s two columns and the tags kept as JSON.
    let root = folder("money");
    let first = "entity Item {\n    id: Uuid\n    price: Int\n    tags: List<Text>\n}\n";
    let then =
        "entity Item {\n    id: Uuid\n    price: Money\n    tags: List<Text>\n    cost: Money\n}\n";
    let (name, second) = two_migrations(&root, first, then);
    assert!(
        second
            .stdout
            .contains("- price           bigint   retired: kept for servers on the old build"),
        "{}",
        second.stdout
    );
    assert!(
        !second.stdout.contains("retire its column"),
        "{}",
        second.stdout
    );
    decide(
        &root,
        &name,
        "    Item.price: {\n        backfill: money.from_minor(Item.price, Currency.gbp)\n    }\n    Item.cost: {\n        backfill: money.from_minor(Item.price * 2, Currency.gbp)\n    }\n",
    );
    ok(&jadpo(&root, &["migrate", "new"]));
    let sql = file(&root, &name, "postgres.sql");
    for expected in [
        "-- backfill Item.price: in Jadpo, 1000 rows a batch",
        "UPDATE \"item\" SET \"price_amount\" = \"jadpo_value\".\"value_1\", \"price_currency\" = \"jadpo_value\".\"value_2\" FROM unnest($1::uuid[], $2::bigint[], $3::char(3)[]) AS \"jadpo_value\"(\"key\", \"value_1\", \"value_2\") WHERE \"item\".\"id\" = \"jadpo_value\".\"key\" AND \"item\".\"price_amount\" IS NULL",
        "ALTER TABLE \"item\" ALTER COLUMN \"cost_currency\" SET NOT NULL;\n",
        "ALTER TABLE \"item\" ALTER COLUMN \"price_amount\" SET NOT NULL;\n",
    ] {
        assert!(sql.contains(expected), "{expected}\n{sql}");
    }
    jadpo(&root, &["records", "update"]);
    ok(&jadpo(&root, &["check"]));
    // Applied with the build that wrote it, as an earlier migration's
    // backfill in Jadpo must be.
    let live = jadpo_test::live();
    let url = live.then(|| {
        let url = first_applied(&root, "conversion_money");
        statement(
            &url,
            "INSERT INTO \"item\" (\"id\", \"price\", \"tags\") VALUES (gen_random_uuid(), 250, '[]'), (gen_random_uuid(), 1000, '[\"a\", \"b\"]')",
        );
        ok(&jadpo(&root, &["migrate", "apply", "--url", &url]));
        url
    });
    // A third migration reads both: a value held in several columns, and
    // one held as JSON.
    write(
        &root,
        ITEM,
        "entity Item {\n    id: Uuid\n    price: Money\n    tags: List<Text>\n    cost: Money\n    total: Int\n}\n",
    );
    ok(&jadpo(&root, &["migrate", "new", "add_total"]));
    let total = latest(&root);
    decide(
        &root,
        &total,
        "    Item.total: {\n        backfill: money.minor(Item.cost) + list.count(Item.tags)\n    }\n",
    );
    ok(&jadpo(&root, &["migrate", "new"]));
    let sql = file(&root, &total, "postgres.sql");
    assert!(
        sql.contains("SELECT \"id\", \"cost_amount\", \"cost_currency\", \"tags\" FROM \"item\""),
        "{sql}"
    );
    jadpo(&root, &["records", "update"]);
    let checked = jadpo(&root, &["check"]);
    ok(&checked);
    assert!(
        !checked.stdout.contains("not_built_yet"),
        "{}",
        checked.stdout
    );
    let Some(url) = url else {
        return;
    };
    ok(&jadpo(&root, &["migrate", "apply", "--url", &url]));
    assert_eq!(
        rows(
            &url,
            "SELECT \"price\", \"price_amount\", \"price_currency\"::text, \"cost_amount\", \"cost_currency\"::text, \"total\" FROM \"item\" ORDER BY \"price\""
        ),
        [
            ["250", "250", "GBP", "500", "GBP", "500"],
            ["1000", "1000", "GBP", "2000", "GBP", "2002"],
        ]
    );
    assert_eq!(
        nullable(
            &url,
            &[
                "price",
                "price_amount",
                "price_currency",
                "cost_amount",
                "cost_currency"
            ]
        ),
        ["YES", "NO", "NO", "NO", "NO"]
    );
}

#[test]
fn a_backfill_reads_a_converted_enum_only_as_its_built_in_type() {
    let root = folder("enum");
    write(
        &root,
        "domains/shop/types/size.jadpo",
        "type Size = Enum {\n    small\n    large\n}\n",
    );
    let (name, _) = two_migrations(
        &root,
        "use shop/types { Size }\n\nentity Item {\n    id: Uuid\n    size: Size\n}\n",
        "use shop/types { Size }\n\nentity Item {\n    id: Uuid\n    size: Int\n    kind: Size?\n}\n",
    );
    decide(
        &root,
        &name,
        "    Item.size: {\n        backfill: integer.max(Item.size, 0)\n    }\n",
    );
    jadpo(&root, &["records", "update"]);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 1, "{}", checked.stdout);
    assert!(
        checked
            .stdout
            .contains("error: The backfill for `Item.size` can't read `Item.size`"),
        "{}",
        checked.stdout
    );
    assert!(
        checked.stdout.contains("a field this migration converts"),
        "{}",
        checked.stdout
    );
}
