//! The AI accounting statements on real PostgreSQL, including races on
//! previously absent rows. Every store is local and disposable.
use jadpo_host_native::{Runtime, postgres};
use jadpo_runtime::{Database, SqlType, SqlValue, StoreError, ai_budget};
use jadpo_test::postgres::Server;
use std::sync::{Arc, Barrier};

fn setup(stem: &str) -> Option<(Server, String)> {
    if !jadpo_test::live() {
        return None;
    }
    let server = Server::start().expect("local PostgreSQL");
    let url = server.create_database(stem).unwrap();
    let runtime = Runtime::new().unwrap();
    let store = postgres(&url).unwrap();
    let table = jadpo_check::data::system_table(&jadpo_std::system::AI_USAGE);
    let mut schema = jadpo_graph::schema::Schema::default();
    schema.stores.insert(
        "main".into(),
        jadpo_graph::schema::Store {
            driver: "postgres".into(),
            tables: [("jadpo_ai_usage".into(), table)].into(),
        },
    );
    let sql = jadpo_graph::migration::migration_sql(&Default::default(), &schema, &|_| None)
        .unwrap()
        .to_document("20261023T0000_ai");
    for section in jadpo_graph::migration::sections(&sql) {
        runtime
            .block_on(store.batch_outside_transaction(&section.sql))
            .unwrap();
    }
    Some((server, url))
}

fn params(
    day: i32,
    caller: Option<&str>,
    tokens: i64,
    micros: i64,
    (app, caller_limit): (i64, i64),
    cost: Option<i64>,
    at: i64,
) -> Vec<SqlValue> {
    vec![
        SqlValue::Date(day),
        SqlValue::Text("app".into()),
        SqlValue::Text(String::new()),
        SqlValue::Int8(tokens),
        SqlValue::Int8(micros),
        SqlValue::Int8(app),
        cost.map_or(SqlValue::Null(SqlType::Int8), SqlValue::Int8),
        SqlValue::Timestamp(at),
        caller.map_or(SqlValue::Null(SqlType::Text), |v| SqlValue::Text(v.into())),
        SqlValue::Int8(caller_limit),
    ]
}

fn reserve(url: &str, params: &[SqlValue]) -> Result<Vec<jadpo_runtime::Row>, StoreError> {
    let runtime = Runtime::new().unwrap();
    let store = postgres(url).unwrap();
    runtime.block_on(store.query_once(ai_budget::TAKE, params))
}
fn sql(url: &str, sql: &str) -> Vec<jadpo_runtime::Row> {
    let runtime = Runtime::new().unwrap();
    let store = postgres(url).unwrap();
    runtime.block_on(store.query_once(sql, &[])).unwrap()
}
fn snapshot(url: &str) -> Vec<jadpo_runtime::Row> {
    sql(url, "SELECT * FROM jadpo_ai_usage ORDER BY day, scope, key")
}
fn refused(result: Result<Vec<jadpo_runtime::Row>, StoreError>) {
    assert_eq!(
        result,
        Err(StoreError::Conflict {
            constraint: "jadpo_ai_usage_pkey".into()
        })
    );
}

#[test]
fn refusal_rolls_back_every_window_and_its_timestamp_even_on_first_rows() {
    let Some((_server, url)) = setup("ai_atomic") else {
        return;
    };
    // Existing caller, no APP row on its day: the inserted APP must vanish.
    sql(
        &url,
        "INSERT INTO jadpo_ai_usage VALUES ('1970-01-02', 'caller', 'a', 0, 0, 0, 0, 90, 0, 0, NULL, '2000-01-01')",
    );
    let before = snapshot(&url);
    refused(reserve(
        &url,
        &params(1, Some("a"), 20, 0, (1000, 100), None, 2000),
    ));
    assert_eq!(snapshot(&url), before, "no new APP and caller unchanged");
    // Existing APP refuses, new caller must vanish too.
    reserve(&url, &params(2, None, 90, 0, (100, 100), None, 1000)).unwrap();
    let before = snapshot(&url);
    refused(reserve(
        &url,
        &params(2, Some("new"), 20, 0, (100, 100), None, 2000),
    ));
    assert_eq!(snapshot(&url), before, "no new caller and APP unchanged");
    // Existing APP succeeds before existing caller refuses: no touch or compensation.
    reserve(&url, &params(1, None, 10, 0, (1000, 100), None, 1000)).unwrap();
    let before = snapshot(&url);
    refused(reserve(
        &url,
        &params(1, Some("a"), 20, 0, (1000, 100), None, 2000),
    ));
    assert_eq!(snapshot(&url), before);
}

#[test]
fn first_rows_and_existing_rows_share_atomic_token_and_cost_limits() {
    let Some((_server, url)) = setup("ai_races") else {
        return;
    };
    // Same caller, different callers sharing APP, token and cost caps.
    for (round, same, cost) in [(1, true, false), (2, false, false), (3, false, true)] {
        for existing in [false, true] {
            let day = round * 10 + i32::from(existing);
            if existing {
                reserve(
                    &url,
                    &params(day, Some("same"), 0, 0, (100, 100), None, 1000),
                )
                .unwrap();
            }
            let barrier = Arc::new(Barrier::new(16));
            let joins: Vec<_> = (0..16)
                .map(|n| {
                    let barrier = Arc::clone(&barrier);
                    let url = url.clone();
                    std::thread::spawn(move || {
                        let caller = if same {
                            "same".to_string()
                        } else {
                            format!("{n}")
                        };
                        let p = params(
                            day,
                            Some(&caller),
                            if cost { 1 } else { 30 },
                            if cost { 30 } else { 0 },
                            (100, if same { 70 } else { 100 }),
                            cost.then_some(100),
                            2000,
                        );
                        barrier.wait();
                        reserve(&url, &p)
                    })
                })
                .collect();
            let mut passed = 0;
            for join in joins {
                match join.join().unwrap() {
                    Ok(rows) => {
                        assert_eq!(rows.len(), 2);
                        passed += 1;
                    }
                    other => refused(other),
                }
            }
            assert_eq!(passed, if same { 2 } else { 3 }, "day {day}");
            let rows = sql(
                &url,
                &format!(
                    "SELECT reserved_tokens, reserved_micros FROM jadpo_ai_usage WHERE day = DATE '1970-01-01' + {day} AND scope = 'app'"
                ),
            );
            assert_eq!(
                rows,
                vec![vec![
                    SqlValue::Int8(if cost { 3 } else { passed * 30 }),
                    SqlValue::Int8(if cost { 90 } else { 0 })
                ]]
            );
        }
    }
}

#[test]
fn statement_errors_keep_all_reservations_and_debit_counts_every_window() {
    let Some((_server, url)) = setup("ai_debit") else {
        return;
    };
    reserve(
        &url,
        &params(1, Some("a"), 60, 90, (100, 100), Some(150), 1000),
    )
    .unwrap();
    let before = snapshot(&url);
    sql(
        &url,
        "ALTER TABLE jadpo_ai_usage ADD CONSTRAINT injected CHECK (reserved_tokens <= 70)",
    );
    assert!(matches!(
        reserve(
            &url,
            &params(1, Some("a"), 20, 30, (100, 100), Some(150), 2000)
        ),
        Err(StoreError::Statement(_))
    ));
    assert_eq!(
        snapshot(&url),
        before,
        "statement failure holds every reserve"
    );
    let debit = vec![
        SqlValue::Date(1),
        SqlValue::Text("app".into()),
        SqlValue::Text(String::new()),
        SqlValue::Int8(21),
        SqlValue::Int8(7),
        SqlValue::Int8(5),
        SqlValue::Int8(60),
        SqlValue::Int8(31),
        SqlValue::Int8(90),
        SqlValue::Text("USD".into()),
        SqlValue::Timestamp(2000),
        SqlValue::Text("a".into()),
        SqlValue::Text("answer model".into()),
    ];
    let runtime = Runtime::new().unwrap();
    let store = postgres(&url).unwrap();
    sql(
        &url,
        "ALTER TABLE jadpo_ai_usage ADD CONSTRAINT injected_debit CHECK (scope <> 'prompt')",
    );
    assert!(
        runtime
            .block_on(store.query_once(ai_budget::DEBIT, &debit))
            .is_err()
    );
    assert_eq!(
        snapshot(&url),
        before,
        "failed prompt charge must leave APP and CALLER held too"
    );
    sql(
        &url,
        "ALTER TABLE jadpo_ai_usage DROP CONSTRAINT injected_debit",
    );
    runtime
        .block_on(store.query_once(ai_budget::DEBIT, &debit))
        .unwrap();
    let rows = sql(
        &url,
        "SELECT scope, calls, input_tokens, output_tokens, cached_tokens, reserved_tokens, cost_micros, reserved_micros, currency FROM jadpo_ai_usage ORDER BY scope",
    );
    assert_eq!(rows.len(), 3);
    for (row, scope) in rows.iter().zip(["app", "caller", "prompt"]) {
        assert_eq!(
            *row,
            vec![
                SqlValue::Text(scope.into()),
                SqlValue::Int4(1),
                SqlValue::Int8(21),
                SqlValue::Int8(7),
                SqlValue::Int8(5),
                SqlValue::Int8(0),
                SqlValue::Int8(31),
                SqlValue::Int8(0),
                SqlValue::Text("USD".into())
            ]
        );
    }
}

/// Hold APP changed in an uncommitted transaction, observe both actual
/// reservations waiting in PostgreSQL, THEN commit. Their statement snapshots
/// predate that commit; only predicates on locked current rows enforce 100.
#[test]
fn reservations_waiting_on_app_recheck_its_committed_current_count() {
    let Some((_server, url)) = setup("ai_blocked") else {
        return;
    };
    for first in [false, true] {
        let day = if first { 2 } else { 1 };
        if !first {
            reserve(&url, &params(day, None, 0, 0, (100, 100), None, 1000)).unwrap();
        }
        let runtime = Runtime::new().unwrap();
        let store = postgres(&url).unwrap();
        let mut holder = runtime.block_on(store.begin()).unwrap();
        if first {
            runtime.block_on(holder.execute("INSERT INTO jadpo_ai_usage VALUES ('1970-01-03', 'app', '', 0, 0, 0, 0, 40, 0, 0, NULL, '2000-01-01')", &[])).unwrap();
        } else {
            runtime.block_on(holder.execute("UPDATE jadpo_ai_usage SET reserved_tokens = 40 WHERE day = '1970-01-02' AND scope = 'app'", &[])).unwrap();
        }
        let barrier = Arc::new(Barrier::new(3));
        let joins: Vec<_> = (0..2)
            .map(|n| {
                let barrier = Arc::clone(&barrier);
                let url = url.clone();
                std::thread::spawn(move || {
                    let rt = Runtime::new().unwrap();
                    let store = postgres(&url).unwrap();
                    let p = params(
                        day,
                        Some(&format!("blocked-{n}")),
                        40,
                        0,
                        (100, 100),
                        None,
                        2000,
                    );
                    barrier.wait();
                    rt.block_on(store.query_once(ai_budget::TAKE, &p))
                })
            })
            .collect();
        barrier.wait();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let waiting = sql(
                &url,
                "SELECT count(*) FROM pg_stat_activity WHERE datname = current_database() AND pid <> pg_backend_pid() AND wait_event_type = 'Lock' AND cardinality(pg_blocking_pids(pid)) > 0 AND query LIKE '%jadpo_ai_usage%'",
            );
            if waiting == vec![vec![SqlValue::Int8(2)]] {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "both real reservations must wait on the held APP row: {waiting:?}"
            );
            std::thread::yield_now();
        }
        runtime.block_on(holder.commit()).unwrap();
        let mut passed = 0;
        for join in joins {
            match join.join().unwrap() {
                Ok(rows) => {
                    assert_eq!(rows.len(), 2);
                    passed += 1;
                }
                other => refused(other),
            }
        }
        assert_eq!(passed, 1, "only one caller fits after the committed 40");
        let app = sql(
            &url,
            &format!(
                "SELECT reserved_tokens FROM jadpo_ai_usage WHERE day = DATE '1970-01-01' + {day} AND scope = 'app'"
            ),
        );
        assert_eq!(app, vec![vec![SqlValue::Int8(80)]]);
    }
}
