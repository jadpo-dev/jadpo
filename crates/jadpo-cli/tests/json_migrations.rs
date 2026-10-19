//! Migrating structured values stored as JSON (area 5, "Storage types";
//! chunk 5.9c) against live PostgreSQL. The snapshot records what a JSON
//! column's type reads, so:
//!
//! - a change that only widens what it reads is a migration with nothing
//!   to run, and the stored values read as they did;
//! - one that may read less checks every stored value in batches and
//!   refuses the migration while any doesn't read, naming how many and the
//!   rule, never a value, and applies once they do;
//! - with a `backfill:`, the values that don't read are rewritten instead;
//! - a required structured field added to a table with rows takes a
//!   `backfill:`, worked out in Jadpo and written through the codec.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips the
//! live parts.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_runtime::{Database as _, SqlValue};
use jadpo_term::Environment;

use crate::support;

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("json-migration-{name}"));
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

const TAG: &str = "domains/shipping/types/tag.jadpo";
const LABEL: &str = "domains/shipping/types/label.jadpo";
const DELIVERY: &str = "domains/shipping/types/delivery.jadpo";
const PARCEL: &str = "domains/shipping/entities/parcel.jadpo";

fn tag(max_length: u32) -> String {
    format!("type Tag = Text {{\n    max_length: {max_length}\n}}\n")
}

const LABEL_TYPE: &str = "use shipping/types { Tag }\n\n/// What is printed on a parcel.\ntype Label = Object {\n    line: Tag\n    note: Tag?\n}\n";

fn delivery(variants: &str) -> String {
    format!(
        "use shipping/types {{ Tag }}\n\n/// How a parcel leaves.\ntype Delivery = Enum {{\n{variants}}}\n"
    )
}

const PICKUP_OR_COURIER: &str = "    pickup\n    courier: {\n        company: Tag\n    }\n";

const PARCEL_ENTITY: &str = "use shipping/types { Delivery, Label }\n\nentity Parcel {\n    id: Uuid\n    label: Label\n    delivery: Delivery\n}\n";

/// A project whose first migration made `Parcel` with a label and a
/// delivery stored as JSON, a `Tag` at most `max_length` characters.
fn shipping(name: &str, max_length: u32) -> PathBuf {
    let root = folder(name);
    write(&root, "app.jadpo", "application {}\n");
    write(&root, TAG, &tag(max_length));
    write(&root, LABEL, LABEL_TYPE);
    write(&root, DELIVERY, &delivery(PICKUP_OR_COURIER));
    write(&root, PARCEL, PARCEL_ENTITY);
    ok(&jadpo(&root, &["migrate", "new", "initial"]));
    root
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

fn file(root: &Path, name: &str, file: &str) -> String {
    fs::read_to_string(root.join(format!("database/migrations/{name}/{file}"))).expect(file)
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

/// Runs a query, giving each row's columns as text.
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
                        SqlValue::Text(text) | SqlValue::Json(text) => text,
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

/// `good` parcels whose label reads under a `Tag` of 5 characters, and
/// `bad` whose line is longer, each going by courier.
fn parcels(url: &str, good: u32, bad: u32) {
    statement(
        url,
        &format!(
            "INSERT INTO \"parcel\" (\"id\", \"label\", \"delivery\") SELECT gen_random_uuid(), '{{\"line\": \"short\", \"note\": null}}'::jsonb, '{{\"tag\": \"courier\", \"company\": \"Swift\"}}'::jsonb FROM generate_series(1, {good}) AS n"
        ),
    );
    statement(
        url,
        &format!(
            "INSERT INTO \"parcel\" (\"id\", \"label\", \"delivery\") SELECT gen_random_uuid(), '{{\"line\": \"{SECRET_LINE}\", \"note\": null}}'::jsonb, '\"pickup\"'::jsonb FROM generate_series(1, {bad}) AS n"
        ),
    );
}

/// A line too long for a `Tag` of 5 characters, which no output may show.
const SECRET_LINE: &str = "hush-1234567";

/// Whether `out` has a row for `name` saying `what`.
fn said(out: &str, name: &str, what: &str) -> bool {
    out.lines().any(|line| {
        line.starts_with("      ")
            && line.trim_start().starts_with(name)
            && line.ends_with(&format!("  {what}"))
    })
}

fn applied(url: &str) -> Vec<String> {
    rows(url, "SELECT name FROM jadpo_migrations ORDER BY name")
        .into_iter()
        .map(|row| row[0].clone())
        .collect()
}

#[test]
fn the_snapshot_records_what_a_json_columns_type_reads() {
    let root = shipping("snapshot", 20);
    let name = latest(&root);
    let snapshot =
        jadpo_graph::schema::Schema::read(&file(&root, &name, "snapshot.json")).expect("snapshot");
    let label = &snapshot.stores["main"].tables["parcel"].columns["label"];
    let reads = label
        .allows
        .as_ref()
        .and_then(|allows| allows.json.as_ref())
        .expect("what the label's type reads");
    assert_eq!(
        reads.shape,
        jadpo_graph::stored_json::Shape::Named("Label".into())
    );
    let Some(jadpo_graph::stored_json::Named::Object { fields, .. }) = reads.types.get("Label")
    else {
        panic!("{reads:?}");
    };
    assert_eq!(
        fields["line"],
        jadpo_graph::stored_json::Shape::Value(jadpo_graph::stored_json::Scalar {
            ty: "Text".into(),
            max_length: Some(20),
            ..Default::default()
        })
    );
    assert!(matches!(
        fields["note"],
        jadpo_graph::stored_json::Shape::Optional(_)
    ));
    // Changing the rules inside the value is drift `jadpo check` reports.
    write(&root, TAG, &tag(30));
    let check = jadpo(&root, &["check"]);
    assert_ne!(check.code, 0);
    assert!(
        check.stdout.contains("migration.out_of_date"),
        "{}",
        check.stdout
    );
}

#[test]
fn a_change_that_only_widens_what_is_read_runs_nothing() {
    let root = shipping("widen", 20);
    write(&root, TAG, &tag(40));
    write(
        &root,
        DELIVERY,
        &delivery(&format!("{PICKUP_OR_COURIER}    post\n")),
    );
    let made = jadpo(&root, &["migrate", "new", "widen_labels"]);
    ok(&made);
    assert!(
        made.stdout
            .contains("no statements: nothing in the database changes"),
        "{}",
        made.stdout
    );
    let name = latest(&root);
    assert!(
        !root
            .join(format!("database/migrations/{name}/changes.jadpo"))
            .exists()
    );
    ok(&jadpo(&root, &["check"]));
    if !jadpo_test::live() {
        return;
    }
    let url = first_applied(&root, "json_migration_widen");
    parcels(&url, 3, 0);
    // A migration with nothing to run has nothing for the store to
    // record: it is up to date.
    let apply = jadpo(&root, &["migrate", "apply", "--url", &url]);
    ok(&apply);
    assert!(apply.stdout.contains("is up to date"), "{}", apply.stdout);
}

#[test]
fn a_change_that_may_read_less_checks_every_value_and_refuses_while_one_doesnt_read() {
    let root = shipping("narrow", 20);
    write(&root, TAG, &tag(5));
    let made = jadpo(&root, &["migrate", "new", "shorten_tags"]);
    ok(&made);
    assert!(
        made.stdout
            .contains("check stored values in batches in Jadpo"),
        "{}",
        made.stdout
    );
    let name = latest(&root);
    let sql = file(&root, &name, "postgres.sql");
    // The check is the section's first step, so a refusal leaves nothing
    // of the migration applied; both JSON columns read less.
    assert!(
        sql.contains("\n-- store: main\n-- check Parcel.delivery: in Jadpo, 1000 rows a batch"),
        "{sql}"
    );
    assert!(
        sql.contains("-- check Parcel.label: in Jadpo, 1000 rows a batch"),
        "{sql}"
    );
    assert!(
        sql.contains("SELECT \"id\", \"label\" FROM \"parcel\" WHERE \"id\" > $1 AND \"label\" IS NOT NULL ORDER BY \"id\" LIMIT 1000;\n"),
        "{sql}"
    );
    // No decision is needed; `changes.jadpo` says how to give one.
    let changes = file(&root, &name, "changes.jadpo");
    assert!(
        changes.contains("`Parcel.label: { backfill: <value> }`"),
        "{changes}"
    );
    ok(&jadpo(&root, &["check"]));
    if !jadpo_test::live() {
        return;
    }
    let url = first_applied(&root, "json_migration_narrow");
    parcels(&url, 1200, 3);
    let refused = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_ne!(refused.code, 0, "{}", refused.stdout);
    let all = format!("{}{}", refused.stdout, refused.stderr);
    assert!(all.contains("migration.apply_failed"), "{all}");
    assert!(
        all.contains("`Parcel.label` holds 3 stored values its type no longer reads"),
        "{all}"
    );
    assert!(all.contains("`max_length` on `line`"), "{all}");
    assert!(!all.contains(SECRET_LINE), "never a value: {all}");
    assert_eq!(applied(&url).len(), 1, "nothing of it was applied");
    assert!(
        rows(&url, "SELECT name FROM jadpo_migration_steps").is_empty(),
        "the next run checks every value again"
    );
    // Once every stored value reads, the migration applies.
    statement(
        &url,
        &format!("DELETE FROM \"parcel\" WHERE \"label\"->>'line' = '{SECRET_LINE}'"),
    );
    let apply = jadpo(&root, &["migrate", "apply", "--url", &url]);
    ok(&apply);
    assert!(
        said(
            &apply.stdout,
            "Parcel.label",
            "1200 rows checked in 2 batches in Jadpo"
        ),
        "{}",
        apply.stdout
    );
    assert!(
        said(
            &apply.stdout,
            "Parcel.delivery",
            "1200 rows checked in 2 batches in Jadpo"
        ),
        "{}",
        apply.stdout
    );
    assert_eq!(applied(&url).len(), 2);
}

#[test]
fn a_backfill_rewrites_the_values_a_narrower_type_doesnt_read() {
    let root = shipping("rewrite", 20);
    write(&root, TAG, &tag(5));
    ok(&jadpo(&root, &["migrate", "new", "shorten_tags"]));
    let name = latest(&root);
    write(
        &root,
        &format!("database/migrations/{name}/changes.jadpo"),
        "/// Shortens tags, relabelling what no longer fits.\nmigration {\n    Parcel.label: {\n        backfill: Label {\n            line: \"?\"\n            note: none\n        }\n    }\n}\n",
    );
    let unfinished = jadpo(&root, &["check"]);
    assert!(
        unfinished.stdout.contains("migration.unfinished"),
        "{}",
        unfinished.stdout
    );
    let finished = jadpo(&root, &["migrate", "new"]);
    ok(&finished);
    let sql = file(&root, &name, "postgres.sql");
    assert!(
        sql.contains("-- rewrite Parcel.label: in Jadpo, 1000 rows a batch"),
        "{sql}"
    );
    assert!(
        !sql.contains("-- check Parcel.label"),
        "the backfill replaces the check: {sql}"
    );
    assert!(
        sql.contains("-- check Parcel.delivery"),
        "a field without one is still checked: {sql}"
    );
    ok(&jadpo(&root, &["check"]));
    if !jadpo_test::live() {
        return;
    }
    let url = first_applied(&root, "json_migration_rewrite");
    parcels(&url, 1200, 3);
    let apply = jadpo(&root, &["migrate", "apply", "--url", &url]);
    ok(&apply);
    assert!(
        said(
            &apply.stdout,
            "Parcel.label",
            "3 rows rewritten in 2 batches in Jadpo"
        ),
        "{}",
        apply.stdout
    );
    assert!(!apply.stdout.contains(SECRET_LINE));
    let lines = rows(
        &url,
        "SELECT \"label\"->>'line', count(*) FROM \"parcel\" GROUP BY 1 ORDER BY 1",
    );
    assert_eq!(
        lines,
        vec![
            vec!["?".to_string(), "3".to_string()],
            vec!["short".to_string(), "1200".to_string()],
        ]
    );
    // Rewritten through the codec: the label's JSON, `none` as `null`.
    let rewritten = rows(
        &url,
        "SELECT \"label\"::text FROM \"parcel\" WHERE \"label\"->>'line' = '?' LIMIT 1",
    );
    assert_eq!(rewritten[0][0], "{\"line\": \"?\", \"note\": null}");
}

#[test]
fn a_required_structured_field_added_to_a_table_with_rows_takes_a_backfill() {
    let root = folder("add");
    write(&root, "app.jadpo", "application {}\n");
    write(
        &root,
        "domains/shipping/types/rush.jadpo",
        "/// Whether a parcel is rushed.\ntype Rush = Bool {}\n",
    );
    write(
        &root,
        PARCEL,
        "use shipping/types { Rush }\n\nentity Parcel {\n    id: Uuid\n    rush: Rush\n}\n",
    );
    ok(&jadpo(&root, &["migrate", "new", "initial"]));
    write(&root, TAG, &tag(20));
    write(&root, LABEL, LABEL_TYPE);
    write(&root, DELIVERY, &delivery(PICKUP_OR_COURIER));
    write(
        &root,
        PARCEL,
        "use shipping/types { Delivery, Label, Rush }\n\nentity Parcel {\n    id: Uuid\n    rush: Rush\n    label: Label\n    delivery: Delivery\n}\n",
    );
    let made = jadpo(&root, &["migrate", "new", "add_label"]);
    ok(&made);
    let name = latest(&root);
    let pending = jadpo(&root, &["check"]);
    assert!(
        pending.stdout.contains("migration.decision_required"),
        "{}",
        pending.stdout
    );
    write(
        &root,
        "domains/shipping/libraries/deliveries.jadpo",
        "use shipping/types { Delivery, Rush }\n\nlibrary deliveries {\n    function for_rush(rush: Rush) -> Delivery {\n        if (rush) {\n            return Delivery.courier {\n                company: \"Swift\"\n            }\n        }\n        return Delivery.pickup\n    }\n}\n",
    );
    write(
        &root,
        &format!("database/migrations/{name}/changes.jadpo"),
        "/// Labels every parcel, and sends rushed ones by courier.\nmigration {\n    Parcel.label: {\n        backfill: Label {\n            line: \"Unlabelled\"\n            note: none\n        }\n    }\n    Parcel.delivery: {\n        backfill: deliveries.for_rush(Parcel.rush)\n    }\n}\n",
    );
    let finished = jadpo(&root, &["migrate", "new"]);
    ok(&finished);
    let sql = file(&root, &name, "postgres.sql");
    assert!(
        sql.contains("-- backfill Parcel.label: in Jadpo, 1000 rows a batch"),
        "{sql}"
    );
    assert!(
        sql.contains("-- backfill Parcel.delivery: in Jadpo, 1000 rows a batch"),
        "{sql}"
    );
    assert!(sql.contains("$2::jsonb[]"), "{sql}");
    ok(&jadpo(&root, &["check"]));
    if !jadpo_test::live() {
        return;
    }
    let url = first_applied(&root, "json_migration_add");
    statement(
        &url,
        "INSERT INTO \"parcel\" (\"id\", \"rush\") SELECT gen_random_uuid(), n % 4 = 0 FROM generate_series(1, 1205) AS n",
    );
    let apply = jadpo(&root, &["migrate", "apply", "--url", &url]);
    ok(&apply);
    assert!(
        said(
            &apply.stdout,
            "Parcel.label",
            "1205 rows filled in 2 batches in Jadpo (slower than SQL)"
        ),
        "{}",
        apply.stdout
    );
    assert_eq!(
        rows(
            &url,
            "SELECT \"label\"::text, \"delivery\"::text, \"rush\"::text, count(*) FROM \"parcel\" GROUP BY 1, 2, 3 ORDER BY 3"
        ),
        vec![
            vec![
                "{\"line\": \"Unlabelled\", \"note\": null}".to_string(),
                "\"pickup\"".to_string(),
                "false".to_string(),
                "904".to_string(),
            ],
            vec![
                "{\"line\": \"Unlabelled\", \"note\": null}".to_string(),
                "{\"tag\": \"courier\", \"company\": \"Swift\"}".to_string(),
                "true".to_string(),
                "301".to_string(),
            ],
        ]
    );
    assert_eq!(
        rows(
            &url,
            "SELECT is_nullable::text FROM information_schema.columns WHERE table_name = 'parcel' AND column_name IN ('label', 'delivery') ORDER BY column_name",
        ),
        vec![vec!["NO".to_string()], vec!["NO".to_string()]],
        "both are required once filled"
    );
}
