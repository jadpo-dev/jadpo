//! Unique rules over a value with an empty part (area 5, "Unique rules"),
//! against live PostgreSQL. A value kept in several columns, such as an
//! `EmailWithName` with no name, leaves a column `NULL` while it holds a
//! value, and PostgreSQL's `UNIQUE` counts two `NULL`s as different. So
//! every key holding such a column treats `NULL`s as equal (`NULLS NOT
//! DISTINCT`), and a field that may be `none` keys only the records that
//! hold a value, so any number may hold none:
//!
//! - two records equal in every part, an empty part included, are refused
//!   by a field's `unique: true`, by a rule over several fields, and by a
//!   rule kept as a unique index because it holds a value as its JSON;
//! - a migration written before this rebuilds each such key, and refuses,
//!   naming the records, while stored records would break it.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips the
//! live parts.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_runtime::{Database as _, StoreError};
use jadpo_term::Environment;

use crate::support;

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("unique-parts-{name}"));
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

const CONTACT: &str = "domains/people/entities/contact.jadpo";
const NOTE: &str = "domains/people/types/note.jadpo";

const NOTE_TYPE: &str = "/// A short note kept whole.\ntype Note = Object {\n    line: Text {\n        max_length: 20\n    }\n}\n";

/// A contact unique by its address, by its team and address, by its
/// note and address (a unique index, the note kept as JSON), and by a
/// second address that may be `none`.
const CONTACT_ENTITY: &str = "use people/types { Note }\n\nentity Contact {\n    id: Uuid\n    team: Text\n    note: Note\n    email: EmailWithName {\n        unique: true\n    }\n    work: EmailWithName\n    backup: EmailWithName? {\n        unique: true\n    }\n\n    unique: {\n        work_per_team: [team, work]\n        work_per_note: [note, work]\n    }\n}\n";

/// The project's migrations, as `migrate apply` hands them to the runtime.
fn migrations(root: &Path) -> Vec<jadpo_runtime::Migration> {
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

/// Runs `sql` in a transaction of its own, giving the store's refusal.
fn tried(url: &str, sql: &str) -> Result<(), StoreError> {
    let store = jadpo_host_native::postgres(url).expect("store");
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    runtime.block_on(async {
        let mut transaction = store.begin().await?;
        transaction.batch(sql).await?;
        transaction.commit().await
    })
}

/// A contact, each address as `(name, address)` columns, `NULL` for
/// `None`.
fn contact(
    team: &str,
    note: &str,
    email: (Option<&str>, &str),
    work: (Option<&str>, &str),
    backup: Option<(Option<&str>, &str)>,
) -> String {
    let text = |value: Option<&str>| value.map_or("NULL".to_string(), |v| format!("'{v}'"));
    let (backup_name, backup_address) = match backup {
        Some((name, address)) => (text(name), text(Some(address))),
        None => ("NULL".to_string(), "NULL".to_string()),
    };
    format!(
        "INSERT INTO \"contact\" (\"id\", \"team\", \"note\", \"email_name\", \"email_address\", \"work_name\", \"work_address\", \"backup_name\", \"backup_address\") VALUES (gen_random_uuid(), '{team}', '{{\"line\": \"{note}\"}}'::jsonb, {}, '{}', {}, '{}', {backup_name}, {backup_address})",
        text(email.0),
        email.1,
        text(work.0),
        work.1,
    )
}

fn conflict(constraint: &str) -> Result<(), StoreError> {
    Err(StoreError::Conflict {
        constraint: constraint.into(),
    })
}

#[test]
fn two_values_equal_in_every_part_an_empty_one_included_are_one_value() {
    let root = folder("live");
    write(&root, "app.jadpo", "application {}\n");
    write(&root, NOTE, NOTE_TYPE);
    write(&root, CONTACT, CONTACT_ENTITY);
    ok(&jadpo(&root, &["migrate", "new", "initial"]));
    jadpo(&root, &["records", "update"]);
    ok(&jadpo(&root, &["check"]));
    if !jadpo_test::live() {
        return;
    }
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server
        .create_database("unique_parts_live")
        .expect("database");
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    let store = jadpo_host_native::postgres(&url).expect("store");
    runtime
        .block_on(jadpo_runtime::migrate(&store, &migrations(&root)))
        .expect("migrated");
    let ada = (None, "ada@example.com");
    let work = (None, "work@example.com");
    assert_eq!(
        tried(&url, &contact("a", "x", ada, work, None)),
        Ok(()),
        "the first contact"
    );
    // The same address with no name, on another team and note.
    assert_eq!(
        tried(
            &url,
            &contact("b", "y", ada, (None, "other@example.com"), None)
        ),
        conflict("contact_email_key"),
        "a field's `unique: true`"
    );
    // The same work address with no name, on the same team.
    assert_eq!(
        tried(
            &url,
            &contact("a", "y", (None, "grace@example.com"), work, None)
        ),
        conflict("contact_work_per_team_key"),
        "a rule over several fields"
    );
    // The same work address with no name, under the same note.
    assert_eq!(
        tried(
            &url,
            &contact("c", "x", (None, "grace@example.com"), work, None)
        ),
        conflict("contact_work_per_note_key"),
        "a rule kept as a unique index"
    );
    // A name makes it another value.
    assert_eq!(
        tried(
            &url,
            &contact(
                "a",
                "x",
                (Some("Ada"), "ada@example.com"),
                (Some("Work"), "work@example.com"),
                None
            )
        ),
        Ok(()),
        "a value with a name is another value"
    );
    // Any number of contacts hold no backup; two holding one backup with
    // no name are refused.
    let backup = Some((None, "backup@example.com"));
    for (n, held) in [None, None, backup].into_iter().enumerate() {
        assert_eq!(
            tried(
                &url,
                &contact(
                    "d",
                    &format!("n{n}"),
                    (None, &format!("{n}@example.com")),
                    (None, &format!("w{n}@example.com")),
                    held
                )
            ),
            Ok(()),
            "contact {n}"
        );
    }
    assert_eq!(
        tried(
            &url,
            &contact(
                "e",
                "z",
                (None, "z@example.com"),
                (None, "wz@example.com"),
                backup
            )
        ),
        conflict("contact_backup_key"),
        "an optional field's `unique: true`"
    );
}

/// Turns the initial migration back into what a compiler before this one
/// wrote: keys counting `NULL`s as different, the optional field's a
/// constraint over every row.
fn as_written_before(root: &Path) {
    let name = latest(root);
    let path = root.join(format!("database/migrations/{name}/snapshot.json"));
    let mut snapshot =
        jadpo_graph::schema::Schema::read(&fs::read_to_string(&path).expect("snapshot"))
            .expect("snapshot reads");
    for unique in snapshot
        .stores
        .get_mut("main")
        .and_then(|store| store.tables.get_mut("contact"))
        .expect("contact")
        .unique
        .values_mut()
    {
        unique.nulls_not_distinct = false;
        unique.predicate = None;
    }
    fs::write(&path, snapshot.to_document()).expect("snapshot written");
    let path = root.join(format!("database/migrations/{name}/postgres.sql"));
    let sql = fs::read_to_string(&path).expect("postgres.sql");
    let partial = "CREATE UNIQUE INDEX \"contact_backup_key\" ON \"contact\" (\"backup_name\", \"backup_address\") NULLS NOT DISTINCT WHERE \"backup_address\" IS NOT NULL";
    assert!(sql.contains(partial), "{sql}");
    let sql = sql
        .replace(
            partial,
            "ALTER TABLE \"contact\" ADD CONSTRAINT \"contact_backup_key\" UNIQUE (\"backup_name\", \"backup_address\")",
        )
        .replace(" NULLS NOT DISTINCT", "");
    fs::write(&path, sql).expect("postgres.sql written");
}

#[test]
fn a_key_written_before_is_rebuilt_and_refused_while_records_break_it() {
    let root = folder("rebuilt");
    write(&root, "app.jadpo", "application {}\n");
    write(&root, NOTE, NOTE_TYPE);
    write(&root, CONTACT, CONTACT_ENTITY);
    ok(&jadpo(&root, &["migrate", "new", "initial"]));
    as_written_before(&root);
    // The snapshot no longer says what the entity needs.
    let drift = jadpo(&root, &["check"]);
    assert_ne!(drift.code, 0, "{}", drift.stdout);
    ok(&jadpo(&root, &["migrate", "new", "rebuild_keys"]));
    jadpo(&root, &["records", "update"]);
    ok(&jadpo(&root, &["check"]));
    let name = latest(&root);
    let sql = file(&root, &name, "postgres.sql");
    for expected in [
        "ALTER TABLE \"contact\" DROP CONSTRAINT \"contact_email_key\";",
        "-- duplicates Contact.email: refused while stored records share a value\nSELECT array_to_string((array_agg(\"id\"::text ORDER BY \"id\"))[1:5], ' '), count(*) FROM \"contact\" GROUP BY \"email_name\", \"email_address\" HAVING count(*) > 1 ORDER BY min(\"id\"::text) LIMIT 10;",
        "-- duplicates Contact.backup: refused while stored records share a value\nSELECT array_to_string((array_agg(\"id\"::text ORDER BY \"id\"))[1:5], ' '), count(*) FROM \"contact\" WHERE \"backup_address\" IS NOT NULL GROUP BY \"backup_name\", \"backup_address\" HAVING count(*) > 1 ORDER BY min(\"id\"::text) LIMIT 10;",
        "ALTER TABLE \"contact\" ADD CONSTRAINT \"contact_email_key\" UNIQUE NULLS NOT DISTINCT (\"email_name\", \"email_address\");",
        "ALTER TABLE \"contact\" ADD CONSTRAINT \"contact_work_per_team_key\" UNIQUE NULLS NOT DISTINCT (\"team\", \"work_name\", \"work_address\");",
        "DROP INDEX \"contact_work_per_note_key\";",
        "CREATE UNIQUE INDEX \"contact_work_per_note_key\" ON \"contact\" ((md5(\"note\"::text)), \"work_name\", \"work_address\") NULLS NOT DISTINCT;",
        "ALTER TABLE \"contact\" DROP CONSTRAINT \"contact_backup_key\";",
        "CREATE UNIQUE INDEX \"contact_backup_key\" ON \"contact\" (\"backup_name\", \"backup_address\") NULLS NOT DISTINCT WHERE \"backup_address\" IS NOT NULL;",
    ] {
        assert!(sql.contains(expected), "{expected}\n{sql}");
    }
    // Every search comes before the first key is dropped, so a refused
    // migration has changed nothing.
    assert!(sql.rfind("-- duplicates ") < sql.find("DROP "), "{sql}");
    if !jadpo_test::live() {
        return;
    }
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server
        .create_database("unique_parts_rebuilt")
        .expect("database");
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    let store = jadpo_host_native::postgres(&url).expect("store");
    runtime
        .block_on(jadpo_runtime::migrate(&store, &migrations(&root)[..1]))
        .expect("the key as written before");
    // Two contacts the old key let in: the same address with no name.
    let ada = (None, "ada@example.com");
    for (team, work) in [("a", "one@example.com"), ("b", "two@example.com")] {
        assert_eq!(
            tried(&url, &contact(team, team, ada, (None, work), None)),
            Ok(())
        );
    }
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_ne!(applied.code, 0, "{}", applied.stdout);
    let ids: Vec<String> = {
        let store = jadpo_host_native::postgres(&url).expect("store");
        runtime.block_on(async {
            let mut transaction = store.begin().await.expect("begin");
            let rows = transaction
                .query("SELECT \"id\"::text FROM \"contact\" ORDER BY \"id\"", &[])
                .await
                .expect("ids");
            transaction.rollback().await.expect("rollback");
            rows.into_iter()
                .map(|row| match row.as_slice() {
                    [jadpo_runtime::SqlValue::Text(id)] => id.clone(),
                    other => panic!("{other:?}"),
                })
                .collect()
        })
    };
    let expected = format!(
        "error: Stored records share a value under `Contact.email`\n  reason: `{name}` makes the unique rule `Contact.email`, and 1 set of stored records hold the same value under it, two values equal in every part, an empty one included, being one value. The first set's records are `{}`, `{}`. The step that found them was rolled back, and nothing after it ran.\n  next: Change or delete all but one record of each set, then run `jadpo migrate apply` again (human)\n  rule: migration.apply_failed\n",
        ids[0], ids[1]
    );
    assert!(
        applied.stderr.contains(&expected),
        "{expected}\n{}{}",
        applied.stdout,
        applied.stderr
    );
    // Nothing of it ran: the old key still lets a third in.
    assert_eq!(
        tried(
            &url,
            &contact("c", "c", ada, (None, "three@example.com"), None)
        ),
        Ok(())
    );
    // Once only one holds the address, it applies, and the key holds.
    assert_eq!(
        tried(&url, "DELETE FROM \"contact\" WHERE \"team\" <> 'a'"),
        Ok(())
    );
    ok(&jadpo(&root, &["migrate", "apply", "--url", &url]));
    assert_eq!(
        tried(
            &url,
            &contact("d", "d", ada, (None, "four@example.com"), None)
        ),
        conflict("contact_email_key")
    );
}
