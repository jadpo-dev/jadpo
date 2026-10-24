//! Shared write path, ordinary compiler-generated plans, release Rust host.
//! Interpreter dispatch, not generated application or HTTP throughput.
use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::time::Instant;

use jadpo_host_native::{Postgres, Runtime, postgres};
use jadpo_interp::{Prepared, Program};
use jadpo_runtime::capability::{Answer, Call, Capability, Faked};
use jadpo_runtime::{
    Clock, Ctx, Database, Env, Executor, Host, Isolation, LocalFuture, Outcome, Row, SqlValue,
    StoreError, Transaction,
};
use jadpo_test::postgres::Server;
use jadpo_value::Value;
use serde_json::{Value as Json, json};

mod version_fixture;

#[derive(Default)]
struct Meter {
    census: Cell<bool>,
    calls: Cell<usize>,
    sql: RefCell<Vec<Json>>,
}
impl Meter {
    fn record(&self, sql: &str, params: &[SqlValue]) {
        self.calls.set(self.calls.get() + 1);
        if self.census.get() {
            self.sql
                .borrow_mut()
                .push(json!({"sql":sql,"params":format!("{params:?}")}));
        }
    }
}
struct Metered {
    store: Postgres,
    meter: Rc<Meter>,
}
struct Tx {
    inner: Box<dyn Transaction>,
    meter: Rc<Meter>,
}
impl Database for Metered {
    fn begin_at(
        &self,
        isolation: Isolation,
    ) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>> {
        Box::pin(async move {
            self.meter
                .record(&format!("BEGIN ISOLATION LEVEL {}", isolation.word()), &[]);
            Ok(Box::new(Tx {
                inner: self.store.begin_at(isolation).await?,
                meter: Rc::clone(&self.meter),
            }) as Box<dyn Transaction>)
        })
    }
    fn query_once<'a>(
        &'a self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
        self.meter.record(sql, params);
        self.store.query_once(sql, params)
    }
}
impl Host for Metered {
    fn database(&self, _: &str) -> Option<&dyn Database> {
        Some(self)
    }
    fn random(&self, bytes: &mut [u8]) -> bool {
        bytes.fill(7);
        true
    }
}
impl Transaction for Tx {
    fn query<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
        self.meter.record(sql, params);
        self.inner.query(sql, params)
    }
    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<u64, StoreError>> {
        self.meter.record(sql, params);
        self.inner.execute(sql, params)
    }
    fn batch<'a>(&'a mut self, sql: &'a str) -> LocalFuture<'a, Result<(), StoreError>> {
        self.meter.record(sql, &[]);
        self.inner.batch(sql)
    }
    fn commit(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
        self.meter.record("COMMIT", &[]);
        self.inner.commit()
    }
    fn rollback(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
        self.meter.record("ROLLBACK", &[]);
        self.inner.rollback()
    }
}
struct Courier {
    store: Postgres,
    calls: Cell<usize>,
    corrupt: Cell<bool>,
    change: Cell<bool>,
}
impl Capability for Courier {
    fn call<'a>(&'a self, _: &'a Call) -> LocalFuture<'a, Answer> {
        Box::pin(async move {
            self.calls.set(self.calls.get() + 1);
            let open=self.store.query_once("SELECT count(*)::int4 FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid() AND backend_type='client backend' AND xact_start IS NOT NULL",&[]).await.unwrap();
            assert_eq!(
                open,
                vec![vec![SqlValue::Int4(0)]],
                "provider held no transaction"
            );
            if self.change.replace(false) {
                self.store
                    .query_once("UPDATE shipment SET note='Fresh'", &[])
                    .await
                    .unwrap();
            }
            if self.corrupt.get() {
                Answer::Wire(Rc::from("{\"data\":{\"id\":42}}"))
            } else {
                Answer::Wire(Rc::from("{\"data\":{\"id\":\"b\"}}"))
            }
        })
    }
}
fn rows(rt: &Runtime, store: &Postgres, sql: &str) -> Vec<Row> {
    rt.block_on(store.query_once(sql, &[])).unwrap()
}
fn field<'a>(value: &'a Value, name: &str) -> &'a Value {
    let Value::Record(r) = value else {
        panic!("record")
    };
    r.get(name).unwrap()
}
fn invoke(
    rt: &Runtime,
    program: &Program,
    host: &dyn Host,
    name: &str,
    id: Option<Value>,
    serial: u64,
) -> Outcome {
    let mut ctx = Ctx::of(
        program,
        host,
        Env {
            seed: Some(serial + 100),
            clock: Clock::TEST,
        },
    );
    ctx.set_call_key("request", format!("version-{serial}"));
    let args = match id {
        Some(id) => vec![id, Value::text("N1"), Value::text("am")],
        None => vec![Value::text("Plain")],
    };
    rt.block_on(program.run(&mut ctx, program.plan().find(name).unwrap(), args))
}
fn logging(rt: &Runtime, store: &Postgres, expected: &str) {
    let deadline = Instant::now() + std::time::Duration::from_secs(5);
    while rows(rt, store, "SHOW log_statement") != vec![vec![SqlValue::Text(expected.into())]] {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}
fn main() {
    assert!(
        std::env::var_os("JADPO_TEST_DATABASE_URL").is_none(),
        "owned PostgreSQL only"
    );
    let output = std::env::args().nth(1).expect("output");
    let count: usize = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "500".into())
        .parse()
        .unwrap();
    assert!(count > 0);
    let directory = Path::new(&output).with_extension("project");
    assert!(!directory.exists(), "fresh fixture directory required");
    std::fs::create_dir_all(&directory).unwrap();
    for (path, text) in version_fixture::files("https://courier.invalid") {
        let path = directory.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let compiler = std::env::var("JADPO_BENCH_COMPILER").expect("matched compiler path");
    let migration = std::process::Command::new(compiler)
        .current_dir(&directory)
        .args(["migrate", "new", "initial"])
        .output()
        .unwrap();
    assert!(
        migration.status.success(),
        "{}{}",
        String::from_utf8_lossy(&migration.stdout),
        String::from_utf8_lossy(&migration.stderr)
    );
    let loaded = jadpo_db::load(&directory);
    let prepared =
        Prepared::load(&loaded.db, loaded.project).unwrap_or_else(|e| panic!("fixture: {e:?}"));
    let config = prepared
        .config(
            &|name: &str| match name {
                "COURIERS_API_KEY" => Some("local-unused".into()),
                "COURIERS_ENVIRONMENT" => Some("sandbox".into()),
                _ => None,
            },
            jadpo_runtime::config::Mode::Environment,
            &|_: &mut [u8]| false,
        )
        .unwrap();
    let schema = jadpo_check::data::schema(&loaded.db, loaded.project);
    let sql = jadpo_graph::migration::migration_sql(&Default::default(), schema, &|_| None)
        .unwrap()
        .to_document("20261023T0000_versions");
    let program = Program::new(prepared).with_config(&config);
    let server = Server::start().unwrap();
    let url = server.create_database("versions_bench").unwrap();
    let rt = Runtime::new().unwrap();
    let control = postgres(&url).unwrap();
    for section in jadpo_graph::migration::sections(&sql) {
        rt.block_on(control.batch_outside_transaction(&section.sql))
            .unwrap();
    }
    let data = rows(&rt, &control, "SHOW data_directory");
    let SqlValue::Text(data) = &data[0][0] else {
        panic!("data")
    };
    let log_path = Path::new(data).parent().unwrap().join("server.log");
    let meter = Rc::new(Meter::default());
    let courier = Rc::new(Courier {
        store: postgres(&url).unwrap(),
        calls: Cell::new(0),
        corrupt: Cell::new(false),
        change: Cell::new(false),
    });
    let host = Faked::new(Metered {
        store: postgres(&url).unwrap(),
        meter: Rc::clone(&meter),
    })
    .with("couriers", courier.clone());
    let Outcome::Value(shipment) = invoke(&rt, &program, &host, "add_shipment", None, 0) else {
        panic!("added")
    };
    let id = field(&shipment, "id").clone();
    let mut serial = 1;
    let mut runs = Vec::new();
    for round in 1..=3 {
        rows(&rt, &control, "UPDATE shipment SET note='Plain',label=NULL");
        // Refused provider answer: no write survives.
        let before = rows(&rt, &control, "SELECT note,label FROM shipment");
        courier.corrupt.set(true);
        let bad = invoke(
            &rt,
            &program,
            &host,
            "book_shipment",
            Some(id.clone()),
            serial,
        );
        serial += 1;
        assert!(!matches!(bad, Outcome::Value(_)));
        assert_eq!(
            rows(&rt, &control, "SELECT note,label FROM shipment"),
            before
        );
        courier.corrupt.set(false);
        // Real concurrent change at the capability seam: rerun reads Fresh.
        courier.change.set(true);
        let calls = courier.calls.get();
        let Outcome::Value(fresh) = invoke(
            &rt,
            &program,
            &host,
            "book_shipment",
            Some(id.clone()),
            serial,
        ) else {
            panic!("stale control")
        };
        serial += 1;
        assert_eq!(courier.calls.get() - calls, 2);
        assert_eq!(field(&fresh, "label"), &Value::text("Fresh b"));
        assert_eq!(
            rows(&rt, &control, "SELECT note,label FROM shipment"),
            vec![vec![
                SqlValue::Text("Fresh".into()),
                SqlValue::Text("Fresh b".into())
            ]]
        );
        for _ in 0..50 {
            assert!(matches!(
                invoke(
                    &rt,
                    &program,
                    &host,
                    "book_shipment",
                    Some(id.clone()),
                    serial
                ),
                Outcome::Value(_)
            ));
            serial += 1;
        }
        server
            .admin("ALTER SYSTEM SET log_statement='all'")
            .unwrap();
        rows(&rt, &control, "SELECT pg_reload_conf()");
        logging(&rt, &control, "all");
        logging(&rt, &host.host().store, "all");
        logging(&rt, &courier.store, "all");
        let log_start = std::fs::metadata(&log_path).unwrap().len() as usize;
        meter.calls.set(0);
        meter.sql.borrow_mut().clear();
        meter.census.set(true);
        assert!(matches!(
            invoke(
                &rt,
                &program,
                &host,
                "book_shipment",
                Some(id.clone()),
                serial
            ),
            Outcome::Value(_)
        ));
        serial += 1;
        let census = meter.sql.borrow().clone();
        let statements = meter.calls.get();
        meter.census.set(false);
        // The provider's independent no-transaction assertion is an extra
        // query on its own control connection, outside the runtime census.
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        let (log, pg_statements) = loop {
            let log = std::fs::read_to_string(&log_path).unwrap();
            let log = log[log_start..].to_string();
            let actual = log
                .lines()
                .filter(|line| {
                    line.contains("LOG:  execute ") || line.contains("LOG:  statement: ")
                })
                .count();
            if actual > statements {
                break (log, actual);
            }
            assert!(
                Instant::now() < deadline,
                "incomplete PostgreSQL census: {log}"
            );
            std::thread::yield_now();
        };
        assert_eq!(pg_statements, statements + 1, "{log}");
        server
            .admin("ALTER SYSTEM SET log_statement='none'")
            .unwrap();
        rows(&rt, &control, "SELECT pg_reload_conf()");
        logging(&rt, &control, "none");
        logging(&rt, &host.host().store, "none");
        logging(&rt, &courier.store, "none");
        meter.calls.set(0);
        let mut latency = Vec::new();
        let provider_before = courier.calls.get();
        let started = Instant::now();
        for _ in 0..count {
            let now = Instant::now();
            let result = invoke(
                &rt,
                &program,
                &host,
                "book_shipment",
                Some(id.clone()),
                serial,
            );
            serial += 1;
            latency.push(now.elapsed().as_secs_f64() * 1000.);
            let Outcome::Value(value) = result else {
                panic!("{result:?}")
            };
            assert_eq!(field(&value, "label"), &Value::text("Fresh b"));
        }
        let elapsed = started.elapsed().as_secs_f64();
        assert_eq!(meter.calls.get(), statements * count);
        assert_eq!(courier.calls.get() - provider_before, count);
        let state = rows(&rt, &control, "SELECT note,label FROM shipment");
        assert_eq!(
            state,
            vec![vec![
                SqlValue::Text("Fresh".into()),
                SqlValue::Text("Fresh b".into())
            ]]
        );
        let mut ordered = latency.clone();
        ordered.sort_by(f64::total_cmp);
        runs.push(json!({"round":round,"count":count,"seconds":elapsed,"calls_per_second":count as f64/elapsed,"p95_ms":ordered[(count*95).div_ceil(100)-1],"latency_ms":latency,"statements_per_call":statements,"census_sql":census,"postgres_statements":pg_statements,"provider_control_queries":1,"provider_calls":courier.calls.get()-provider_before,"data_statements_per_call":census.iter().filter(|row| !matches!(row["sql"].as_str().unwrap(), "COMMIT"|"ROLLBACK") && !row["sql"].as_str().unwrap().starts_with("BEGIN ")).count(),"postgres_log":log,"persisted_note":"Fresh","persisted_label":"Fresh b","stale_control":true,"refusal_control":true,"fault_control":true}));
    }
    std::fs::write(output,serde_json::to_vec_pretty(&json!({"protocol":1,"complete":true,"scope":"release Rust runtime/compiler-generated-plan; interpreter dispatch; no HTTP","warmup":50,"count":count,"postgres":format!("{:?}",rows(&rt,&control,"SELECT version()")),"runs":runs})).unwrap()).unwrap();
}
