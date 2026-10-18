//! A literal in SQL is the value the runtime binds (area 19, "Every
//! boundary through the contract"; chunk 5.5w3b), made from each standard
//! type's data entry, so a type added later is tested without a line here.
//!
//! A project holds an entity with a field of every standard type stored in
//! one column, a query per example comparing that field with the example
//! written as code, and a migration backfilling a new required field of
//! each type with the same literal; a plain text field takes text holding
//! quotes, backslashes and Unicode both ways. The plan binds each query's
//! literal as a parameter exactly as the runtime binds the example's value
//! (`to_sql`), and the backfill writes into `postgres.sql` exactly that
//! value as SQL text (`jadpo_sql::literal`). Against live PostgreSQL, the
//! backfill stores what a parameter of the runtime's would compare equal
//! to, each query runs, and text that tries to end its literal stays one
//! value whatever `standard_conforming_strings` says.
//!
//! A quantity whose kind's base unit has no suffix has no literal
//! (`units.of(…)`), and bytes none at all, so neither is written here.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips the
//! live part.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_runtime::{Database as _, SqlType, SqlValue};
use jadpo_std::read::Source;
use jadpo_std::standard::{STANDARD_TYPES, StandardType, Storage};
use jadpo_term::Environment;
use jadpo_value::Value;

fn skipped() -> bool {
    let skip = std::env::var("JADPO_SKIP_POSTGRES").is_ok_and(|value| value == "1");
    if skip {
        println!("skipped: JADPO_SKIP_POSTGRES is set");
    }
    skip
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    run(&args, &env.with_cwd(cwd))
}

/// Text a program writes with quotes, backslashes and Unicode in it, and
/// how Jadpo source writes it.
const NASTY: &str = "it's \\'; DROP TABLE \"holder\"; --\n-- store: other\r\n\\ café ☕ 𝄞";
const NASTY_SOURCE: &str =
    "\"it's \\\\'; DROP TABLE \\\"holder\\\"; --\\n-- store: other\\r\\n\\\\ café ☕ 𝄞\"";

/// One standard type's example written as a literal: its entry, its
/// number, the example's number, the value the runtime holds, and the
/// literal as source writes it.
struct Case {
    entry: &'static StandardType,
    at: usize,
    n: usize,
    value: Value,
    written: String,
    /// The type a parameter compared with its column is sent as.
    ty: SqlType,
}

impl Case {
    fn field(&self) -> String {
        format!("f{}", self.at)
    }

    fn backfilled(&self) -> String {
        format!("b{}_{}", self.at, self.n)
    }

    fn query(&self) -> String {
        format!("q{}_{}", self.at, self.n)
    }

    /// The value the runtime binds for the example.
    fn bound(&self) -> SqlValue {
        jadpo_runtime::to_sql(&self.value, self.ty)
            .unwrap_or_else(|why| panic!("{}: {why}", self.entry.name))
    }
}

/// Every standard type stored in one column of a scalar type, with each
/// valid example that code can write as a literal.
fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    let mut at = 0;
    for entry in STANDARD_TYPES {
        let Storage::Column(column) = entry.storage else {
            continue;
        };
        if matches!(
            column,
            SqlType::Json | SqlType::Bytes | SqlType::Array(_) | SqlType::Range(_)
        ) || entry.examples.of.is_some()
        {
            continue;
        }
        let mut any = false;
        for (n, (text, _)) in entry.examples.valid.iter().enumerate() {
            let read = |text: &str| {
                jadpo_value::standard::read(entry, text)
                    .unwrap_or_else(|fault| panic!("{}: {text} ({fault:?})", entry.name))
            };
            let mut value = read(text);
            // A decimal literal has a point: `-90.0`, not `-90`, which is a
            // whole number.
            if let Value::Decimal(decimal) = &value
                && decimal.scale == 0
            {
                value = read(&format!("{value}.0"));
            }
            let written = value.to_string();
            if written.starts_with("units.of(") || written.starts_with('<') {
                continue;
            }
            any = true;
            out.push(Case {
                entry,
                at,
                n,
                value,
                written,
                ty: column.param(),
            });
        }
        if any {
            at += 1;
        }
    }
    out
}

/// The entity, with a field of each type and, once `backfilled`, the
/// required fields a migration backfills.
fn holder(backfilled: bool) -> String {
    let mut fields = String::from("    id: Uuid\n    title: Text?\n");
    let mut seen = Vec::new();
    for case in cases() {
        if !seen.contains(&case.at) {
            seen.push(case.at);
            fields.push_str(&format!("    {}: {}?\n", case.field(), case.entry.name));
        }
        if backfilled {
            fields.push_str(&format!("    {}: {}\n", case.backfilled(), case.entry.name));
        }
    }
    if backfilled {
        fields.push_str("    note: Text\n");
    }
    format!(
        "/// Holds a value of every standard type stored in one column.\nentity Holder {{\n{fields}\n    policy: {{\n        read: [Access.public]\n    }}\n}}\n"
    )
}

/// The project, migrated, with each backfill decided as its literal.
fn project() -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("sql-literals");
    let _ = fs::remove_dir_all(&root);
    write(&root, "app.jadpo", "application {}\n");
    write(&root, "domains/store/entities/holder.jadpo", &holder(false));
    write(
        &root,
        "domains/store/types/holder_view.jadpo",
        "use store/entities { Holder }\n\n/// A holder, by id.\ntype HolderView = Output {\n    pick: [Holder.id]\n}\n",
    );
    let mut queries = vec![(
        "titled".to_string(),
        "title".to_string(),
        NASTY_SOURCE.to_string(),
    )];
    for case in cases() {
        queries.push((case.query(), case.field(), case.written.clone()));
    }
    for (name, field, written) in queries {
        write(
            &root,
            &format!("domains/store/queries/{name}.jadpo"),
            &format!(
                "use store/entities {{ Holder }}\nuse store/types {{ HolderView }}\n\n/// Holders whose `{field}` is one value.\nquery {name}() -> List<HolderView> {{\n    from: Holder\n    where: Holder.{field} == {written}\n    order: [Holder.id asc]\n    limit: 1\n}}\n"
            ),
        );
    }
    let initial = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(initial.code, 0, "{}{}", initial.stdout, initial.stderr);
    write(&root, "domains/store/entities/holder.jadpo", &holder(true));
    let started = jadpo(&root, &["migrate", "new", "backfills"]);
    assert_eq!(started.code, 0, "{}{}", started.stdout, started.stderr);
    let latest = latest(&root);
    let changes_path = root.join(format!("database/migrations/{latest}/changes.jadpo"));
    let mut changes = fs::read_to_string(&changes_path).expect("changes.jadpo");
    for case in cases() {
        let pending = format!("    Holder.{}: {{}}", case.backfilled());
        assert!(changes.contains(&pending), "{pending}\n{changes}");
        changes = changes.replace(
            &pending,
            &format!(
                "    Holder.{}: {{\n        backfill: {}\n    }}",
                case.backfilled(),
                case.written
            ),
        );
    }
    changes = changes.replace(
        "    Holder.note: {}",
        &format!("    Holder.note: {{\n        backfill: {NASTY_SOURCE}\n    }}"),
    );
    fs::write(&changes_path, changes).expect("decisions");
    let finished = jadpo(&root, &["migrate", "new"]);
    assert_eq!(finished.code, 0, "{}{}", finished.stdout, finished.stderr);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    root
}

/// The latest migration's folder.
fn latest(root: &Path) -> String {
    let mut names: Vec<String> = fs::read_dir(root.join("database/migrations"))
        .expect("migrations")
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect();
    names.sort();
    names.pop().expect("a migration")
}

#[test]
fn every_standard_literal_is_bound_and_written_as_the_runtime_binds_it() {
    let root = project();
    let all = cases();
    assert!(all.len() > 30, "{} cases", all.len());

    // A query binds each literal as a parameter, the value the runtime
    // binds for the example.
    let loaded = jadpo_db::load(&root);
    let plan = jadpo_plan::Plan::build(&loaded.db, loaded.project);
    let stored = |name: &str| -> (String, Vec<SqlValue>) {
        let query = plan
            .queries()
            .iter()
            .find(|query| query.name == name)
            .unwrap_or_else(|| panic!("{name}"));
        let select = &query.read.select;
        let values = select
            .params
            .iter()
            .filter_map(|param| match &param.source {
                Source::Stored(value) => Some(value.clone()),
                _ => None,
            })
            .collect();
        (select.sql.clone(), values)
    };
    for case in &all {
        let (sql, values) = stored(&case.query());
        assert_eq!(values, vec![case.bound()], "{}: {sql}", case.written);
        assert!(
            sql.contains(&format!("\"{}\" = $1", case.field())),
            "{}: {sql}",
            case.written
        );
    }
    let (sql, values) = stored("titled");
    assert_eq!(values, vec![SqlValue::Text(NASTY.into())], "{sql}");
    assert!(!sql.contains("DROP"), "{sql}");

    // A backfill writes that value into the migration as SQL text.
    let latest = latest(&root);
    let migration =
        fs::read_to_string(root.join(format!("database/migrations/{latest}/postgres.sql")))
            .expect("postgres.sql");
    for case in &all {
        let written = jadpo_sql::literal::sql(&case.bound()).expect("SQL text");
        let column = case.backfilled();
        let fill =
            format!("UPDATE \"holder\" SET \"{column}\" = {written} WHERE \"{column}\" IS NULL;");
        assert!(migration.contains(&fill), "{fill}\n{migration}");
    }
    let note = jadpo_sql::literal::sql(&SqlValue::Text(NASTY.into())).expect("SQL text");
    assert!(
        migration.contains(&format!("UPDATE \"holder\" SET \"note\" = {note} WHERE")),
        "{note}\n{migration}"
    );
    assert!(
        note.starts_with("E'"),
        "a backslash makes an escape string: {note}"
    );

    if skipped() {
        return;
    }
    // Live: the backfill stores what the runtime's parameter compares
    // equal to, and each query runs with the parameters the plan binds.
    let server = jadpo_test::database::server().expect("a PostgreSQL server");
    let url = server.create_database("sql-literals").expect("a database");
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let steps = steps.get("main").expect("the main store").clone();
    let runtime = jadpo_host_native::Runtime::new().expect("a runtime");
    let store = jadpo_host_native::postgres(&url).expect("a store");
    runtime.block_on(async {
        jadpo_runtime::migrate(&store, &steps[..1])
            .await
            .expect("the first migration");
        let mut transaction = store.begin().await.expect("begins");
        transaction
            .batch(
                "INSERT INTO \"holder\" (\"id\") VALUES ('0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e')",
            )
            .await
            .expect("a holder");
        transaction.commit().await.expect("commits");
        jadpo_runtime::migrate(&store, &steps)
            .await
            .expect("the backfills");
        let mut transaction = store.begin().await.expect("begins");
        for case in &all {
            let rows = transaction
                .query(
                    &format!("SELECT \"{}\" = $1 FROM \"holder\"", case.backfilled()),
                    &[case.bound()],
                )
                .await
                .unwrap_or_else(|error| panic!("{}: {error:?}", case.written));
            assert_eq!(rows, vec![vec![SqlValue::Bool(true)]], "{}", case.written);
            let (sql, values) = stored(&case.query());
            transaction
                .query(&sql, &values)
                .await
                .unwrap_or_else(|error| panic!("{}: {sql} ({error:?})", case.written));
        }
        let rows = transaction
            .query(
                "SELECT \"note\" = $1 FROM \"holder\"",
                &[SqlValue::Text(NASTY.into())],
            )
            .await
            .expect("reads the note");
        assert_eq!(rows, vec![vec![SqlValue::Bool(true)]]);
        transaction.rollback().await.expect("rolls back");
    });
}

#[test]
fn text_written_into_sql_stays_one_value_in_any_session() {
    let texts = [
        "it's",
        "'); DROP TABLE \"t\"; --",
        "\\'); DROP TABLE \"t\"; --",
        "x\\",
        "\\\\",
        "\\x41",
        "$1 $$ dollars $$",
        "E'x'",
        "line\nbreak\ttab\r",
        "café ☕ 𝄞 \u{202e}desrever",
        "\u{feff}",
        "x\n-- store: other\nDROP TABLE \"t\"",
    ];
    for text in texts {
        let written = jadpo_sql::literal::sql(&SqlValue::Text(text.into())).expect("SQL text");
        assert!(written.ends_with('\''), "{written}");
    }
    // PostgreSQL text can't hold the NUL character: it is never written,
    // and the runtime's parameter holding one is refused too.
    assert_eq!(
        jadpo_sql::literal::sql(&SqlValue::Text("a\0b".into())),
        None
    );
    if skipped() {
        return;
    }
    let server = jadpo_test::database::server().expect("a PostgreSQL server");
    let url = server.create_database("sql-text").expect("a database");
    let runtime = jadpo_host_native::Runtime::new().expect("a runtime");
    let store = jadpo_host_native::postgres(&url).expect("a store");
    runtime.block_on(async {
        for conforming in ["on", "off"] {
            for text in texts {
                let written =
                    jadpo_sql::literal::sql(&SqlValue::Text(text.into())).expect("SQL text");
                let mut transaction = store.begin().await.expect("begins");
                // As a migration runs: several statements in one batch.
                transaction
                    .batch(&format!(
                        "SET LOCAL standard_conforming_strings = {conforming}; CREATE TABLE \"t\" (\"v\" text); INSERT INTO \"t\" (\"v\") VALUES ({written})"
                    ))
                    .await
                    .unwrap_or_else(|error| panic!("{text:?} ({conforming}): {error:?}"));
                let rows = transaction
                    .query(
                        "SELECT \"v\" = $1, count(*) OVER () FROM \"t\"",
                        &[SqlValue::Text(text.into())],
                    )
                    .await
                    .unwrap_or_else(|error| panic!("{text:?} ({conforming}): {error:?}"));
                assert_eq!(
                    rows,
                    vec![vec![SqlValue::Bool(true), SqlValue::Int8(1)]],
                    "{text:?} ({conforming}): {written}"
                );
                transaction.rollback().await.expect("rolls back");
            }
        }
        let mut transaction = store.begin().await.expect("begins");
        let refused = transaction
            .query("SELECT $1::text", &[SqlValue::Text("a\0b".into())])
            .await;
        assert!(refused.is_err(), "{refused:?}");
        let _ = transaction.rollback().await;
    });
}
