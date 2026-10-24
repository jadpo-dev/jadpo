//! The PostgreSQL driver against a live server (docs/compiler-runtime.md
//! §2.6 and §2.7): the one `JADPO_TEST_DATABASE_URL` names, or a disposable
//! one started from the machine's PostgreSQL tools, shared by the tests in
//! this binary, each with a fresh database. `JADPO_SKIP_POSTGRES=1` skips
//! them (the gate's quick profile); otherwise a machine with no server
//! fails them, saying how to get one.

use std::sync::OnceLock;

use jadpo_host_native::{Postgres, Runtime, postgres};
use jadpo_runtime::{
    Database, MigrateError, Migration, SqlType, SqlValue, Standing, StoreError, migrate, standing,
};
use jadpo_test::postgres::Server;

fn server() -> &'static Server {
    static SERVER: OnceLock<Server> = OnceLock::new();
    SERVER.get_or_init(|| Server::start().unwrap_or_else(|error| panic!("{error}")))
}

/// A runtime and a store on a new, empty database; `None` when skipped.
fn fresh(name: &str) -> Option<(Runtime, Postgres)> {
    if !jadpo_test::live() {
        return None;
    }
    let url = server().create_database(name).unwrap();
    Some((Runtime::new().unwrap(), postgres(&url).unwrap()))
}

async fn run(store: &Postgres, sql: &str) {
    let mut transaction = store.begin().await.unwrap();
    transaction.batch(sql).await.unwrap();
    transaction.commit().await.unwrap();
}

async fn rows(store: &Postgres, sql: &str, params: &[SqlValue]) -> Vec<Vec<SqlValue>> {
    let mut transaction = store.begin().await.unwrap();
    let rows = transaction.query(sql, params).await.unwrap();
    transaction.commit().await.unwrap();
    rows
}

async fn count(store: &Postgres, table: &str) -> i64 {
    match rows(store, &format!("SELECT count(*) FROM {table}"), &[])
        .await
        .as_slice()
    {
        [row] => match row.as_slice() {
            [SqlValue::Int8(n)] => *n,
            other => panic!("count gave {other:?}"),
        },
        other => panic!("count gave {other:?}"),
    }
}

#[test]
fn a_transaction_commits_or_rolls_back_as_a_whole() {
    let Some((runtime, store)) = fresh("commit") else {
        return;
    };
    runtime.block_on(async {
        run(&store, "CREATE TABLE notes (body text NOT NULL)").await;

        let mut transaction = store.begin().await.unwrap();
        for body in ["a", "b"] {
            let changed = transaction
                .execute(
                    "INSERT INTO notes (body) VALUES ($1)",
                    &[SqlValue::Text(body.into())],
                )
                .await
                .unwrap();
            assert_eq!(changed, 1);
        }
        transaction.rollback().await.unwrap();
        assert_eq!(count(&store, "notes").await, 0, "rolled-back rows are gone");

        let mut transaction = store.begin().await.unwrap();
        transaction
            .execute(
                "INSERT INTO notes (body) VALUES ($1), ($2), ($3)",
                &[
                    SqlValue::Text("a".into()),
                    SqlValue::Text("b".into()),
                    SqlValue::Text("c".into()),
                ],
            )
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        assert_eq!(count(&store, "notes").await, 3);

        let mut transaction = store.begin().await.unwrap();
        let changed = transaction
            .execute(
                "UPDATE notes SET body = body || '!' WHERE body <> $1",
                &[SqlValue::Text("a".into())],
            )
            .await
            .unwrap();
        assert_eq!(changed, 2, "execute counts the rows it changed");
        let deleted = transaction
            .execute("DELETE FROM notes WHERE body = 'nothing'", &[])
            .await
            .unwrap();
        assert_eq!(deleted, 0);
        transaction.commit().await.unwrap();

        // A transaction dropped without either ends with its connection.
        let mut transaction = store.begin().await.unwrap();
        transaction.execute("DELETE FROM notes", &[]).await.unwrap();
        drop(transaction);
        assert_eq!(count(&store, "notes").await, 3);
    });
}

fn every_value() -> Vec<SqlValue> {
    vec![
        SqlValue::Bool(true),
        SqlValue::Int2(i16::MIN),
        SqlValue::Int4(i32::MAX),
        SqlValue::Int8(i64::MIN + 1),
        SqlValue::Numeric("-12.50".into()),
        SqlValue::Text("naïve ☃ 'quoted'".into()),
        SqlValue::Bytes(vec![0, 1, 2, 254, 255]),
        SqlValue::Uuid([
            0x55, 0x0e, 0x84, 0x00, 0xe2, 0x9b, 0x41, 0xd4, 0xa7, 0x16, 0x44, 0x66, 0x55, 0x44,
            0x00, 0x00,
        ]),
        // 2026-10-14T12:34:56.789012Z
        SqlValue::Timestamp(1_791_981_296_789_012),
        // 2026-10-14
        SqlValue::Date(20_740),
        // In jsonb's own output form, so it reads back as written.
        SqlValue::Json(r#"{"a": [1, 2.5, "x"], "b": null}"#.into()),
        // 12:34:56.789
        SqlValue::Time(45_296_789_000),
    ]
}

const EVERY_COLUMN: &str = "CREATE TABLE every (
    b bool, i2 int2, i4 int4, i8 int8, n numeric, t text, by bytea,
    u uuid, ts timestamptz, d date, j jsonb, tm time
)";

const EVERY_INSERT: &str =
    "INSERT INTO every VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)";

#[test]
fn every_value_round_trips_through_a_table_and_as_parameters() {
    let Some((runtime, store)) = fresh("values") else {
        return;
    };
    runtime.block_on(async {
        run(&store, EVERY_COLUMN).await;
        let values = every_value();
        let nulls: Vec<SqlValue> = values
            .iter()
            .map(|value| SqlValue::Null(value.ty()))
            .collect();
        let mut transaction = store.begin().await.unwrap();
        transaction.execute(EVERY_INSERT, &values).await.unwrap();
        transaction.execute(EVERY_INSERT, &nulls).await.unwrap();
        transaction.commit().await.unwrap();

        let read = rows(&store, "SELECT * FROM every ORDER BY b NULLS LAST", &[]).await;
        assert_eq!(read, vec![values.clone(), nulls.clone()]);

        // As parameters: each comes back as the type it was sent as.
        let select = (1..=values.len())
            .map(|n| format!("${n}"))
            .collect::<Vec<_>>()
            .join(", ");
        let echoed = rows(&store, &format!("SELECT {select}"), &values).await;
        assert_eq!(echoed, vec![values.clone()]);
        let echoed = rows(&store, &format!("SELECT {select}"), &nulls).await;
        assert_eq!(echoed, vec![nulls]);

        // A parameter compared with a column finds its row.
        let found = rows(
            &store,
            "SELECT t FROM every WHERE u = $1 AND ts = $2 AND d = $3 AND n = $4",
            &[
                values[7].clone(),
                values[8].clone(),
                values[9].clone(),
                values[4].clone(),
            ],
        )
        .await;
        assert_eq!(found, vec![vec![values[5].clone()]]);
    });
}

#[test]
fn arrays_go_as_parameters_for_in_and_includes() {
    // Area 6: `in` a list and an include's keys are one array parameter,
    // `= ANY($1)`, and `unnest($1)` for a list per record.
    let Some((runtime, store)) = fresh("arrays") else {
        return;
    };
    runtime.block_on(async {
        run(
            &store,
            "CREATE TABLE items (id uuid PRIMARY KEY, n int2 NOT NULL, label text NOT NULL)",
        )
        .await;
        let ids: Vec<[u8; 16]> = (1..=3u8).map(|n| [n; 16]).collect();
        let mut transaction = store.begin().await.unwrap();
        for (i, id) in ids.iter().enumerate() {
            transaction
                .execute(
                    "INSERT INTO items VALUES ($1, $2, $3)",
                    &[
                        SqlValue::Uuid(*id),
                        SqlValue::Int8(i as i64),
                        SqlValue::Text(format!("item {i}")),
                    ],
                )
                .await
                .unwrap();
        }
        transaction.commit().await.unwrap();
        let some = SqlValue::Array(
            SqlType::Uuid,
            vec![SqlValue::Uuid(ids[0]), SqlValue::Uuid(ids[2])],
        );
        let found = rows(
            &store,
            "SELECT label FROM items WHERE id = ANY($1) ORDER BY label",
            std::slice::from_ref(&some),
        )
        .await;
        assert_eq!(
            found,
            vec![
                vec![SqlValue::Text("item 0".into())],
                vec![SqlValue::Text("item 2".into())]
            ]
        );
        // A smallint column against bigint values, as the plans send them.
        let found = rows(
            &store,
            "SELECT count(*) FROM items WHERE n = ANY($1)",
            &[SqlValue::Array(SqlType::Int8, vec![SqlValue::Int8(1), SqlValue::Int8(2)])],
        )
        .await;
        assert_eq!(found, vec![vec![SqlValue::Int8(2)]]);
        let empty = rows(
            &store,
            "SELECT count(*) FROM items WHERE id = ANY($1)",
            &[SqlValue::Array(SqlType::Uuid, Vec::new())],
        )
        .await;
        assert_eq!(empty, vec![vec![SqlValue::Int8(0)]]);
        let keyed = rows(
            &store,
            "SELECT \"key\".\"id\", \"row\".label FROM unnest($1) AS \"key\"(\"id\") CROSS JOIN LATERAL (SELECT label FROM items WHERE id = \"key\".\"id\") AS \"row\" ORDER BY 2",
            &[some],
        )
        .await;
        assert_eq!(keyed.len(), 2);
        assert_eq!(keyed[0][0], SqlValue::Uuid(ids[0]));
    });
}

#[test]
fn decimals_keep_their_digits_and_scale() {
    let Some((runtime, store)) = fresh("numeric") else {
        return;
    };
    runtime.block_on(async {
        run(&store, "CREATE TABLE amounts (at int4, amount numeric)").await;
        let cases = [
            "0",
            "0.00",
            "1",
            "-1",
            "-12.50",
            "9999",
            "10000",
            "0.0001",
            "0.00001",
            "123456789.987654321",
            "12345678901234567890123456789012345678",
            "-1234567890123456789012.3456789012345678",
            "0.00000000000000000000000000000000000001",
        ];
        let mut transaction = store.begin().await.unwrap();
        for (at, text) in cases.iter().enumerate() {
            transaction
                .execute(
                    "INSERT INTO amounts VALUES ($1, $2)",
                    &[
                        SqlValue::Int4(i32::try_from(at).unwrap()),
                        SqlValue::Numeric((*text).into()),
                    ],
                )
                .await
                .unwrap();
        }
        transaction.commit().await.unwrap();
        let read = rows(&store, "SELECT amount FROM amounts ORDER BY at", &[]).await;
        let expected: Vec<Vec<SqlValue>> = cases
            .iter()
            .map(|text| vec![SqlValue::Numeric((*text).into())])
            .collect();
        assert_eq!(read, expected);

        // What the server computes reads back in the same canonical form.
        let computed = rows(
            &store,
            "SELECT 1.10 + 2.205, -0.5 * 3, 10::numeric(10, 3), sum(amount) FROM amounts WHERE at < 5",
            &[],
        )
        .await;
        assert_eq!(
            computed,
            vec![vec![
                SqlValue::Numeric("3.305".into()),
                SqlValue::Numeric("-1.5".into()),
                SqlValue::Numeric("10.000".into()),
                SqlValue::Numeric("-12.50".into()),
            ]]
        );
    });
}

#[test]
fn related_column_types_read_as_the_runtimes_types() {
    let Some((runtime, store)) = fresh("types") else {
        return;
    };
    runtime.block_on(async {
        let read = rows(
            &store,
            "SELECT 'v'::varchar, 'c'::char(3), 'n'::name, '{\"a\":1}'::json,
                    '2000-01-01 00:00:01'::timestamp, NULL::varchar",
            &[],
        )
        .await;
        assert_eq!(
            read,
            vec![vec![
                SqlValue::Text("v".into()),
                SqlValue::Text("c  ".into()),
                SqlValue::Text("n".into()),
                SqlValue::Json("{\"a\":1}".into()),
                // A `timestamp` without a zone is a local one, a
                // `LocalDateTime`'s column (chunk 5.5w).
                SqlValue::LocalTimestamp(946_684_801_000_000),
                SqlValue::Null(SqlType::Text),
            ]]
        );

        let mut transaction = store.begin().await.unwrap();
        let unread = transaction.query("SELECT 1::oid AS thing", &[]).await;
        assert_eq!(
            unread,
            Err(StoreError::Fault(
                "column `thing` has type oid, which the runtime doesn't read".into()
            ))
        );
        transaction.rollback().await.unwrap();
    });
}

/// A connection keeps no session state between transactions (§2.6; chunk
/// 8.2i): Hyperdrive pools by transaction, so the next transaction on a
/// server connection may be another Worker's. The isolation level is the
/// transaction's own and every statement unnamed, so after a repeatable
/// read transaction that runs statements with parameters, its connection,
/// reused for the next statement, is at the server's own level and holds
/// no prepared statement.
#[test]
fn a_connection_keeps_no_session_state_between_transactions() {
    let Some((runtime, store)) = fresh("session") else {
        return;
    };
    runtime.block_on(async {
        let mut transaction = store
            .begin_at(jadpo_runtime::Isolation::RepeatableRead)
            .await
            .unwrap();
        let read = transaction
            .query(
                "SELECT current_setting('transaction_isolation'), $1::int8 + 1, pg_backend_pid()",
                &[SqlValue::Int8(1)],
            )
            .await
            .unwrap();
        let [level, sum, during] = read[0].as_slice() else {
            panic!("{read:?}");
        };
        assert_eq!(level, &SqlValue::Text("repeatable read".into()));
        assert_eq!(sum, &SqlValue::Int8(2));
        transaction
            .execute("SELECT $1::text", &[SqlValue::Text("again".into())])
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        let after = store
            .query_once(
                "SELECT current_setting('transaction_isolation'), (SELECT count(*) FROM pg_prepared_statements), pg_backend_pid()",
                &[],
            )
            .await
            .unwrap();
        assert_eq!(
            after,
            vec![vec![
                SqlValue::Text("read committed".into()),
                SqlValue::Int8(0),
                during.clone()
            ]],
            "the same connection, reused"
        );
    });
}

/// The terminal capability keeps explicit RR and leaves no session state.
#[test]
fn a_terminal_read_keeps_repeatable_read_and_reuses_an_unnamed_clean_session() {
    if !jadpo_test::live() {
        return;
    }
    // Own this focused test's disposable server so its Drop runs even when
    // this test is selected alone; a process-global OnceLock is not dropped.
    let server = Server::start().unwrap();
    let url = server.create_database("terminal_session").unwrap();
    let runtime = Runtime::new().unwrap();
    let store = postgres(&url).unwrap();
    runtime.block_on(async {
        let mut transaction = store.begin().await.unwrap();
        let read = transaction.query_last_read(
            "SELECT current_setting('transaction_isolation'), $1::int8 + 1, pg_backend_pid()",
            &[SqlValue::Int8(1)],
        ).await.unwrap();
        assert_eq!(read[0][0], SqlValue::Text("repeatable read".into()));
        assert_eq!(read[0][1], SqlValue::Int8(2));
        transaction.commit().await.unwrap();
        let after = store.query_once(
            "SELECT current_setting('transaction_isolation'), (SELECT count(*) FROM pg_prepared_statements), pg_backend_pid()",
            &[],
        ).await.unwrap();
        assert_eq!(after, vec![vec![SqlValue::Text("read committed".into()), SqlValue::Int8(0), read[0][2].clone()]]);
    });
}

#[test]
fn a_batch_runs_statements_one_after_another() {
    let Some((runtime, store)) = fresh("batch") else {
        return;
    };
    runtime.block_on(async {
        run(
            &store,
            "CREATE TABLE a (n int4);
             CREATE TABLE b (n int4);
             INSERT INTO a VALUES (1), (2);
             INSERT INTO b SELECT n * 10 FROM a;",
        )
        .await;
        assert_eq!(
            rows(&store, "SELECT n FROM b ORDER BY n", &[]).await,
            vec![vec![SqlValue::Int4(10)], vec![SqlValue::Int4(20)]]
        );

        let mut transaction = store.begin().await.unwrap();
        let broken = transaction
            .batch("INSERT INTO a VALUES (3); SELECT nope;")
            .await;
        assert!(
            matches!(&broken, Err(StoreError::Fault(message)) if message.contains("42703")),
            "{broken:?}"
        );
        transaction.rollback().await.unwrap();
        assert_eq!(count(&store, "a").await, 2);
    });
}

#[test]
fn constraint_violations_are_conflicts_named_by_their_constraint() {
    let Some((runtime, store)) = fresh("conflict") else {
        return;
    };
    runtime.block_on(async {
        run(
            &store,
            "CREATE TABLE parents (id int4 PRIMARY KEY, code text UNIQUE);
             CREATE TABLE children (parent int4 NOT NULL REFERENCES parents (id));
             INSERT INTO parents VALUES (1, 'one');",
        )
        .await;

        let mut transaction = store.begin().await.unwrap();
        let duplicate = transaction
            .execute(
                "INSERT INTO parents VALUES ($1, $2)",
                &[SqlValue::Int4(2), SqlValue::Text("one".into())],
            )
            .await;
        assert_eq!(
            duplicate,
            Err(StoreError::Conflict {
                constraint: "parents_code_key".into()
            })
        );
        // The transaction is aborted now: committing it says why.
        assert_eq!(
            transaction.commit().await,
            Err(StoreError::Conflict {
                constraint: "parents_code_key".into()
            })
        );

        let mut transaction = store.begin().await.unwrap();
        let orphan = transaction
            .execute("INSERT INTO children VALUES ($1)", &[SqlValue::Int4(9)])
            .await;
        assert_eq!(
            orphan,
            Err(StoreError::Conflict {
                constraint: "children_parent_fkey".into()
            })
        );
        transaction.rollback().await.unwrap();

        // The connection the conflicts ran on is still good.
        assert_eq!(count(&store, "parents").await, 1);
    });
}

#[test]
fn a_row_changed_since_the_snapshot_is_a_concurrent_change() {
    let Some((runtime, store)) = fresh("concurrent") else {
        return;
    };
    runtime.block_on(async {
        run(
            &store,
            "CREATE TABLE counters (id int4 PRIMARY KEY, n int4 NOT NULL);
             INSERT INTO counters VALUES (1, 0);",
        )
        .await;

        // The second transaction's snapshot is taken at its first read.
        let mut second = store.begin().await.unwrap();
        let seen = second
            .query("SELECT n FROM counters WHERE id = 1", &[])
            .await
            .unwrap();
        assert_eq!(seen, vec![vec![SqlValue::Int4(0)]]);

        let mut first = store.begin().await.unwrap();
        first
            .execute("UPDATE counters SET n = n + 1 WHERE id = 1", &[])
            .await
            .unwrap();
        first.commit().await.unwrap();

        let stale = second
            .execute("UPDATE counters SET n = n + 1 WHERE id = 1", &[])
            .await;
        assert_eq!(stale, Err(StoreError::ConcurrentChange));
        second.rollback().await.unwrap();

        assert_eq!(
            rows(&store, "SELECT n FROM counters", &[]).await,
            vec![vec![SqlValue::Int4(1)]]
        );
    });
}

#[test]
fn a_server_that_cant_be_reached_is_unavailable() {
    if !jadpo_test::live() {
        return;
    }
    let runtime = Runtime::new().unwrap();
    let dir = std::env::temp_dir().join(format!("jadpo-empty-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for url in [
        format!("host={} user=jadpo dbname=postgres", dir.display()),
        "postgresql://jadpo@127.0.0.1:1/postgres".to_string(),
    ] {
        let store = postgres(&url).unwrap();
        let begun = runtime.block_on(store.begin());
        assert_eq!(begun.err(), Some(StoreError::Unavailable), "{url}");
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

fn migration(name: &str, digest: &str, sql: &str) -> Migration {
    Migration {
        name: name.into(),
        digest: digest.into(),
        sql: sql.into(),
    }
}

#[test]
fn migrations_apply_once_in_order_and_refuse_a_changed_one() {
    let Some((runtime, store)) = fresh("migrate") else {
        return;
    };
    runtime.block_on(async {
        let migrations = [
            migration(
                "20261014T1000_create_lists",
                "d1",
                "CREATE TABLE lists (id int4 PRIMARY KEY, title text NOT NULL);",
            ),
            migration(
                "20261014T1100_add_archived",
                "d2",
                "ALTER TABLE lists ADD COLUMN archived bool NOT NULL DEFAULT false;
                 CREATE INDEX lists_archived ON lists (archived);",
            ),
        ];
        let names: Vec<String> = migrations.iter().map(|m| m.name.clone()).collect();

        assert_eq!(
            standing(&store, &migrations).await,
            Ok(Standing {
                applied: vec![],
                pending: names.clone(),
            })
        );
        assert_eq!(migrate(&store, &migrations).await, Ok(names.clone()));
        assert_eq!(
            standing(&store, &migrations).await,
            Ok(Standing {
                applied: names.clone(),
                pending: vec![],
            })
        );
        assert_eq!(migrate(&store, &migrations).await, Ok(vec![]));
        run(&store, "INSERT INTO lists (id, title) VALUES (1, 'x')").await;
        assert_eq!(
            rows(&store, "SELECT archived FROM lists", &[]).await,
            vec![vec![SqlValue::Bool(false)]]
        );

        let mut changed = migrations.clone();
        changed[0].digest = "edited".into();
        let refused = MigrateError::Changed {
            name: names[0].clone(),
        };
        assert_eq!(migrate(&store, &changed).await, Err(refused.clone()));
        assert_eq!(standing(&store, &changed).await, Err(refused));
    });
}

// --- the test database -------------------------------------------------------

use std::collections::BTreeMap;

use jadpo_runtime::Host;
use jadpo_test::database::{TestDatabases, TestHost};

/// One store's migration: a table of notes, with a unique rule.
fn note_steps(table: &str) -> BTreeMap<String, Vec<Migration>> {
    let sql = format!(
        "CREATE TABLE \"{table}\" (\"id\" uuid NOT NULL, \"body\" text NOT NULL, CONSTRAINT \"{table}_pkey\" PRIMARY KEY (\"id\"));\nALTER TABLE \"{table}\" ADD CONSTRAINT \"{table}_body_key\" UNIQUE (\"body\");"
    );
    BTreeMap::from([(
        "main".to_string(),
        vec![Migration {
            name: "20260101T0000_initial".into(),
            digest: jadpo_graph::migration::section_digest(&sql),
            sql,
        }],
    )])
}

async fn note_count(host: &TestHost<'_>, table: &str) -> i64 {
    let database = host.database("main").expect("main");
    let mut transaction = database.begin().await.unwrap();
    let found = transaction
        .query(&format!("SELECT count(*) FROM \"{table}\""), &[])
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    match found.first().and_then(|row| row.first()) {
        Some(SqlValue::Int8(n)) => *n,
        other => panic!("count gave {other:?}"),
    }
}

async fn add_note(host: &TestHost<'_>, table: &str, body: &str) -> Result<(), StoreError> {
    let database = host.database("main").expect("main");
    let mut transaction = database.begin().await?;
    transaction
        .execute(
            &format!("INSERT INTO \"{table}\" (\"id\", \"body\") VALUES (gen_random_uuid(), $1)"),
            &[SqlValue::Text(body.into())],
        )
        .await?;
    transaction.commit().await
}

#[test]
fn each_test_gets_its_own_copy_of_the_migrated_template() {
    if !jadpo_test::live() {
        return;
    }
    let started = std::time::Instant::now();
    let databases = TestDatabases::new(note_steps("note"), false);
    let runtime = Runtime::new().unwrap();
    let first = TestHost::new(&databases);
    let second = TestHost::new(&databases);
    runtime.block_on(async {
        add_note(&first, "note", "milk").await.unwrap();
        assert_eq!(note_count(&first, "note").await, 1);
        assert_eq!(note_count(&second, "note").await, 0, "a copy of its own");
        // The template's constraints came with the copy.
        assert_eq!(
            add_note(&first, "note", "milk").await,
            Err(StoreError::Conflict {
                constraint: "note_body_key".into()
            })
        );
    });
    println!("server, template and two copies: {:?}", started.elapsed());
    assert!(first.finish(false).is_empty());
    assert!(
        second.finish(true).is_empty(),
        "nothing kept without --keep-failed"
    );
    // Later copies come from the pool, prepared in the background.
    let copies = std::time::Instant::now();
    for _ in 0..8 {
        let host = TestHost::new(&databases);
        runtime.block_on(async { assert_eq!(note_count(&host, "note").await, 0) });
        host.finish(false);
    }
    println!("eight more copies, each used: {:?}", copies.elapsed());
}

#[test]
fn a_failed_tests_database_is_kept_when_asked() {
    if !jadpo_test::live() {
        return;
    }
    let databases = TestDatabases::new(note_steps("kept_note"), true);
    let runtime = Runtime::new().unwrap();
    let host = TestHost::new(&databases);
    runtime.block_on(async { add_note(&host, "kept_note", "kept").await.unwrap() });
    let kept = host.finish(true);
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert!(kept[0].starts_with("jadpo_test_"), "{kept:?}");
    drop(databases);
    // Still there, with the failed test's row, for someone to look at.
    let server = jadpo_test::database::server().unwrap();
    let store = postgres(&server.url(&kept[0])).unwrap();
    runtime.block_on(async {
        assert_eq!(count(&store, "kept_note").await, 1);
    });
}

#[test]
fn a_test_that_never_uses_a_store_starts_nothing() {
    let databases = TestDatabases::new(note_steps("unused"), false);
    let host = TestHost::new(&databases);
    assert!(host.finish(false).is_empty());
    drop(databases);
}

/// One store whose migration is `sql`.
fn steps_of(sql: &str) -> BTreeMap<String, Vec<Migration>> {
    BTreeMap::from([(
        "main".to_string(),
        vec![Migration {
            name: "20260101T0000_initial".into(),
            digest: jadpo_graph::migration::section_digest(sql),
            sql: sql.to_string(),
        }],
    )])
}

/// The rows a statement on the test's `main` store gives.
async fn host_rows(host: &TestHost<'_>, sql: &str) -> Vec<Vec<SqlValue>> {
    let database = host.database("main").expect("main");
    let mut transaction = database.begin().await.unwrap();
    let found = transaction.query(sql, &[]).await.unwrap();
    transaction.commit().await.unwrap();
    found
}

/// The name of the database the test's `main` store is.
async fn database_name(host: &TestHost<'_>) -> String {
    match host_rows(host, "SELECT current_database()::text")
        .await
        .as_slice()
    {
        [row] => match row.as_slice() {
            [SqlValue::Text(name)] => name.clone(),
            other => panic!("current_database gave {other:?}"),
        },
        other => panic!("current_database gave {other:?}"),
    }
}

#[test]
fn a_copy_given_back_is_emptied_and_used_again() {
    if !jadpo_test::live() {
        return;
    }
    let databases = TestDatabases::new(note_steps("reused_note"), false);
    let runtime = Runtime::new().unwrap();
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..12 {
        let host = TestHost::new(&databases);
        runtime.block_on(async {
            // The same unique body each time: nothing a test before it
            // wrote is left.
            add_note(&host, "reused_note", "same").await.unwrap();
            assert_eq!(note_count(&host, "reused_note").await, 1);
            seen.insert(database_name(&host).await);
        });
        host.finish(false);
    }
    assert!(
        seen.len() < 12,
        "copies are emptied and used again: {seen:?}"
    );
}

#[test]
fn a_reused_copy_starts_its_sequences_again() {
    if !jadpo_test::live() {
        return;
    }
    let databases = TestDatabases::new(
        steps_of(
            "CREATE SEQUENCE \"free_number\";\nCREATE TABLE \"ticket\" (\"n\" bigserial NOT NULL, \"body\" text NOT NULL);",
        ),
        false,
    );
    let runtime = Runtime::new().unwrap();
    for _ in 0..10 {
        let host = TestHost::new(&databases);
        runtime.block_on(async {
            assert_eq!(
                host_rows(&host, "SELECT nextval('free_number')").await,
                vec![vec![SqlValue::Int8(1)]]
            );
            assert_eq!(
                host_rows(&host, "INSERT INTO ticket (body) VALUES ('a') RETURNING n").await,
                vec![vec![SqlValue::Int8(1)]]
            );
        });
        host.finish(false);
    }
}

#[test]
fn copies_of_a_template_with_rows_start_with_those_rows() {
    if !jadpo_test::live() {
        return;
    }
    let databases = TestDatabases::new(
        steps_of(
            "CREATE TABLE \"setting\" (\"name\" text NOT NULL);\nINSERT INTO \"setting\" (\"name\") VALUES ('first');",
        ),
        false,
    );
    let runtime = Runtime::new().unwrap();
    for _ in 0..3 {
        let host = TestHost::new(&databases);
        runtime.block_on(async {
            assert_eq!(
                host_rows(&host, "SELECT count(*) FROM setting").await,
                vec![vec![SqlValue::Int8(1)]],
                "the migration's row, and nothing an earlier test did to it"
            );
            host_rows(&host, "DELETE FROM setting RETURNING name").await;
        });
        host.finish(false);
    }
}

#[test]
fn copies_keep_up_with_tests_on_many_threads() {
    if !jadpo_test::live() {
        return;
    }
    let databases = TestDatabases::new(note_steps("busy_note"), false);
    // The server and the template, before the clock starts.
    let warm = TestHost::new(&databases);
    let runtime = Runtime::new().unwrap();
    runtime.block_on(async { assert_eq!(note_count(&warm, "busy_note").await, 0) });
    warm.finish(false);
    let started = std::time::Instant::now();
    std::thread::scope(|scope| {
        for thread in 0..10 {
            let databases = &databases;
            scope.spawn(move || {
                let runtime = Runtime::new().unwrap();
                for test in 0..6 {
                    let host = TestHost::new(databases);
                    runtime.block_on(async {
                        add_note(&host, "busy_note", &format!("{thread}-{test}"))
                            .await
                            .unwrap();
                        assert_eq!(note_count(&host, "busy_note").await, 1);
                    });
                    host.finish(false);
                }
            });
        }
    });
    println!(
        "sixty tests on ten threads, each writing: {:?}",
        started.elapsed()
    );
}

/// The ids of the server's databases whose names start with `prefix`.
fn database_ids(runtime: &Runtime, prefix: &str) -> Vec<SqlValue> {
    let server = jadpo_test::database::server().unwrap();
    let store = postgres(&server.url("postgres")).unwrap();
    runtime.block_on(async {
        rows(
            &store,
            "SELECT oid::int8 FROM pg_database WHERE starts_with(datname, $1)",
            &[SqlValue::Text(prefix.into())],
        )
        .await
        .into_iter()
        .flatten()
        .collect()
    })
}

#[test]
fn copies_left_by_one_run_are_used_by_the_next() {
    if !jadpo_test::live() {
        return;
    }
    let runtime = Runtime::new().unwrap();
    let first = TestDatabases::new(note_steps("handed_note"), false);
    let host = TestHost::new(&first);
    runtime.block_on(async { add_note(&host, "handed_note", "left").await.unwrap() });
    host.finish(false);
    // The run is over: its copies are emptied and left on the server.
    drop(first);
    let free = database_ids(&runtime, "jadpo_free_");
    assert!(!free.is_empty(), "the run's copies are left for the next");
    let second = TestDatabases::new(note_steps("handed_note"), false);
    let host = TestHost::new(&second);
    runtime.block_on(async {
        assert_eq!(note_count(&host, "handed_note").await, 0, "emptied");
        let id = host_rows(
            &host,
            "SELECT oid::int8 FROM pg_database WHERE datname = current_database()",
        )
        .await;
        assert!(
            free.contains(&id[0][0]),
            "a copy the first run left: {id:?}"
        );
    });
    host.finish(false);
}

#[test]
fn a_run_makes_no_more_copies_than_its_tests_can_use() {
    if !jadpo_test::live() {
        return;
    }
    let runtime = Runtime::new().unwrap();
    let databases = TestDatabases::new(note_steps("lone_note"), false).for_tests(1);
    let host = TestHost::new(&databases);
    runtime.block_on(async { assert_eq!(note_count(&host, "lone_note").await, 0) });
    // The one test's copy, and none prepared beyond it.
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert_eq!(databases.copies(), 1);
    host.finish(false);
}
