//! A scoped native-runtime microfixture: the public ai.generate path,
//! AiFake, and real disposable PostgreSQL. No HTTP or real provider.
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use jadpo_host_native::{Postgres, Runtime, postgres};
use jadpo_runtime::ai::{AiFake, Calling, Done};
use jadpo_runtime::capability::{Answer, Capability, Faked};
use jadpo_runtime::{
    Clock, Ctx, Database, Env, Host, Isolation, JsonCodecs, LocalFuture, Places, Plan, Row,
    SqlValue, StoreError, Transaction,
};
use jadpo_std::ai::{AiDriver, AiLimits, AiPlan, AiUrl, GenerateSite, SchemaMode};
use jadpo_std::http::{Codec, CodecId};
use jadpo_std::prompt::{Piece, PromptEntry, PromptPlan};
use jadpo_std::types::ValidatorId;
use jadpo_test::postgres::Server;
use jadpo_value::Value;
use serde_json::{Value as Json, json};

struct Metered<'a> {
    store: &'a Postgres,
    census: Cell<bool>,
    calls: Cell<u64>,
    statements: RefCell<Vec<Json>>,
}
impl Database for Metered<'_> {
    fn begin_at(
        &self,
        isolation: Isolation,
    ) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>> {
        self.store.begin_at(isolation)
    }
    fn query_once<'a>(
        &'a self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
        self.calls.set(self.calls.get() + 1);
        if self.census.get() {
            self.statements
                .borrow_mut()
                .push(json!({"sql":sql,"params":format!("{params:?}")}));
        }
        self.store.query_once(sql, params)
    }
}
impl Host for Metered<'_> {
    fn database(&self, _: &str) -> Option<&dyn Database> {
        Some(self)
    }
    fn random(&self, bytes: &mut [u8]) -> bool {
        bytes.fill(7);
        true
    }
}

fn rows(rt: &Runtime, store: &Postgres, sql: &str) -> Vec<Row> {
    rt.block_on(store.query_once(sql, &[])).unwrap()
}
fn state(rt: &Runtime, store: &Postgres) -> Json {
    let rows = rows(
        rt,
        store,
        "SELECT scope, key, calls, input_tokens, output_tokens, cached_tokens, reserved_tokens, cost_micros, reserved_micros, currency FROM jadpo_ai_usage ORDER BY scope, key",
    );
    let names = [
        "scope",
        "key",
        "calls",
        "input_tokens",
        "output_tokens",
        "cached_tokens",
        "reserved_tokens",
        "cost_micros",
        "reserved_micros",
        "currency",
    ];
    json!(
        rows.into_iter()
            .map(|row| names
                .into_iter()
                .zip(row)
                .map(|(key, value)| (
                    key.to_string(),
                    match value {
                        SqlValue::Text(text) => json!(text),
                        SqlValue::Int4(n) => json!(n),
                        SqlValue::Int8(n) => json!(n),
                        SqlValue::Null(_) => Json::Null,
                        other => panic!("unexpected state {other:?}"),
                    }
                ))
                .collect::<serde_json::Map<_, _>>())
            .collect::<Vec<_>>()
    )
}

fn logging(rt: &Runtime, store: &Postgres, expected: &str) {
    let deadline = Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let observed = rows(rt, store, "SHOW log_statement");
        if observed == vec![vec![SqlValue::Text(expected.into())]] {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "measured connection logging did not become {expected}: {observed:?}"
        );
        std::thread::yield_now();
    }
}

fn main() {
    // An account-configured server is never used by a benchmark.
    assert!(
        std::env::var_os("JADPO_TEST_DATABASE_URL").is_none(),
        "benchmark requires its own disposable PostgreSQL"
    );
    let output = std::env::args().nth(1).expect("output JSON path");
    let count: usize = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "500".into())
        .parse()
        .unwrap();
    assert!(count > 0);
    let server = Server::start().expect("local PostgreSQL");
    let url = server.create_database("ai_accounting_bench").unwrap();
    let rt = Runtime::new().unwrap();
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
        rt.block_on(store.batch_outside_transaction(&section.sql))
            .unwrap();
    }
    let data = rows(&rt, &store, "SHOW data_directory");
    let SqlValue::Text(data) = &data[0][0] else {
        panic!("local data_directory");
    };
    let pg_log = std::path::Path::new(data)
        .parent()
        .unwrap()
        .join("server.log");
    assert!(pg_log.is_file(), "owned PostgreSQL log");
    let pg_version = rows(&rt, &store, "SELECT version()");
    let host = Metered {
        store: &store,
        census: Cell::new(false),
        calls: Cell::new(0),
        statements: RefCell::new(vec![]),
    };
    let prompts = PromptPlan {
        entries: vec![PromptEntry {
            key: "answer".into(),
            type_name: "Question".into(),
            digest: "00000000".into(),
            instructions: vec![Piece::Text("Answer in one word.".into())],
            request: vec![Piece::Text("Bread?".into())],
            fields: vec![],
            conversation: None,
        }],
    };
    let codecs = [Codec::Scalar {
        builtin: jadpo_std::Builtin::Text,
        validator: ValidatorId(0),
        int_as_number: false,
    }];
    let types = [CodecId(0)];
    let json_codecs = JsonCodecs {
        codecs: jadpo_value::json::Codecs {
            codecs: &codecs,
            validators: &[],
            shapes: &[],
            coder: None,
        },
        types: &types,
    };
    let site = GenerateSite {
        prompt: "answer".into(),
        entry: 0,
        fields: vec![],
        answer: 0,
        schema: None,
        object: false,
        model: "default".into(),
        max_output: 20,
        timeout_ms: 1000,
        attempts: 1,
    };
    let places = Places::new();
    let frame = jadpo_runtime::Frame {
        callable: 0,
        site: jadpo_runtime::Frame::ENTRY,
    };
    let mut result = json!({"protocol":2,"complete":false,"scope":"compiled native runtime ai::generate + AiFake + real local PostgreSQL; no HTTP, no Workers/Bun parity claim","count":count,"warmup":50,"runs":[]});
    let mut runs = vec![];
    for round in 0..3 {
        for scenario in if round % 2 == 0 {
            ["app", "caller", "retry-caller"]
        } else {
            ["retry-caller", "caller", "app"]
        } {
            rows(&rt, &store, "TRUNCATE jadpo_ai_usage");
            let caller = scenario != "app";
            let retry = scenario == "retry-caller";
            let ai = AiPlan {
                connection: "local".into(),
                driver: AiDriver::OpenAiCompatible(AiUrl::Written("http://localhost/v1".into())),
                models: vec![("default".into(), 0)],
                schema: SchemaMode::StrictSubset,
                limits: Some(AiLimits {
                    store: "main".into(),
                    per_request: Some(1000),
                    per_day: 1_000_000_000_000,
                    per_caller_per_day: caller.then_some(1_000_000_000_000),
                    cost: None,
                }),
                prices: vec![],
            };
            let plan = Plan {
                ai: Some(&ai),
                prompts: Some(&prompts),
                json: json_codecs,
                ..Plan::NONE
            };
            let invoke = |census: bool| {
                host.census.set(census);
                host.calls.set(0);
                host.statements.borrow_mut().clear();
                let fake = Rc::new(AiFake::new());
                if retry {
                    fake.script("local", "answer", Answer::Unavailable, frame);
                }
                // Fixed reported usage includes cached input; both sends count.
                fake.script("local", "answer", Answer::Wire(Rc::from(r#"{"choices":[{"message":{"content":"yes"},"finish_reason":"stop"}],"usage":{"prompt_tokens":6,"completion_tokens":1,"prompt_tokens_details":{"cached_tokens":2}}}"#)), frame);
                let faked = Faked::new(&host).with("local", Rc::clone(&fake) as Rc<dyn Capability>);
                let mut ctx = Ctx::new(
                    &places,
                    &faked,
                    Env {
                        seed: Some(1),
                        clock: Clock::TEST,
                    },
                );
                if caller {
                    ctx.set_client(Some("192.0.2.1".into()));
                }
                ctx.set_settings(vec![(
                    0,
                    jadpo_value::standard::parse("local/model", "ModelId").unwrap(),
                )]);
                let began = Instant::now();
                let done = rt.block_on(jadpo_runtime::ai::generate(
                    &mut ctx,
                    Calling { plan, site: &site },
                    Value::record("Question", vec![]),
                ));
                let elapsed = began.elapsed().as_secs_f64() * 1000.;
                assert!(
                    matches!(done, Done::Value(ref v) if *v == Value::text("yes")),
                    "{done:?}"
                );
                assert_eq!(
                    fake.requests("local", "answer").len(),
                    if retry { 2 } else { 1 }
                );
                let expected_input = 6 + if retry { 7 } else { 0 };
                (elapsed, host.calls.get(), expected_input)
            };
            // Known fault control: a missing accounting table must stop the
            // call before the fake is asked, even though the fake is ready.
            rows(
                &rt,
                &store,
                "ALTER TABLE jadpo_ai_usage RENAME TO hidden_usage",
            );
            let fake = Rc::new(AiFake::new());
            let faked = Faked::new(&host).with("local", Rc::clone(&fake) as Rc<dyn Capability>);
            let mut ctx = Ctx::new(
                &places,
                &faked,
                Env {
                    seed: Some(1),
                    clock: Clock::TEST,
                },
            );
            ctx.set_settings(vec![(
                0,
                jadpo_value::standard::parse("local/model", "ModelId").unwrap(),
            )]);
            let failed = rt.block_on(jadpo_runtime::ai::generate(
                &mut ctx,
                Calling { plan, site: &site },
                Value::record("Question", vec![]),
            ));
            assert!(
                matches!(failed, Done::Failed(_)),
                "missing store must fail: {failed:?}"
            );
            assert!(fake.requests("local", "answer").is_empty());
            rows(
                &rt,
                &store,
                "ALTER TABLE hidden_usage RENAME TO jadpo_ai_usage",
            );
            rows(
                &rt,
                &store,
                "INSERT INTO jadpo_ai_usage VALUES ('2000-01-01', 'app', '', 0, 1000000000000, 0, 0, 0, 0, 0, NULL, '2000-01-01')",
            );
            let held = rows(
                &rt,
                &store,
                "SELECT * FROM jadpo_ai_usage ORDER BY scope, key",
            );
            let fake = Rc::new(AiFake::new());
            let faked = Faked::new(&host).with("local", Rc::clone(&fake) as Rc<dyn Capability>);
            let mut ctx = Ctx::new(
                &places,
                &faked,
                Env {
                    seed: Some(1),
                    clock: Clock::TEST,
                },
            );
            if caller {
                ctx.set_client(Some("192.0.2.1".into()));
            }
            ctx.set_settings(vec![(
                0,
                jadpo_value::standard::parse("local/model", "ModelId").unwrap(),
            )]);
            let refused = rt.block_on(jadpo_runtime::ai::generate(
                &mut ctx,
                Calling { plan, site: &site },
                Value::record("Question", vec![]),
            ));
            assert!(
                matches!(
                    refused,
                    Done::Broke {
                        rule: jadpo_std::RuleKey::OverBudget,
                        ..
                    }
                ),
                "full budget must refuse: {refused:?}"
            );
            assert!(fake.requests("local", "answer").is_empty());
            assert_eq!(
                rows(
                    &rt,
                    &store,
                    "SELECT * FROM jadpo_ai_usage ORDER BY scope, key"
                ),
                held
            );
            rows(&rt, &store, "TRUNCATE jadpo_ai_usage");
            for _ in 0..50 {
                invoke(false);
            }
            let census_before = state(&rt, &store);
            server
                .admin("ALTER SYSTEM SET log_statement = 'all'")
                .unwrap();
            rows(&rt, &store, "SELECT pg_reload_conf()");
            logging(&rt, &store, "all");
            let start = std::fs::metadata(&pg_log).unwrap().len() as usize;
            let (_, statements, input) = invoke(true);
            let census_sql = host.statements.borrow().clone();
            std::thread::sleep(std::time::Duration::from_millis(100));
            let log = std::fs::read_to_string(&pg_log).unwrap();
            let log = &log[start..];
            let actual_sql = log
                .lines()
                .filter(|line| {
                    line.contains("LOG:  execute ") || line.contains("LOG:  statement: ")
                })
                .count();
            assert_eq!(
                actual_sql, statements as usize,
                "actual PostgreSQL census: {log}"
            );
            server
                .admin("ALTER SYSTEM SET log_statement = 'none'")
                .unwrap();
            rows(&rt, &store, "SELECT pg_reload_conf()");
            logging(&rt, &store, "none");
            let census_after = state(&rt, &store);
            host.census.set(false);
            let before = state(&rt, &store);
            let mut latency = Vec::with_capacity(count);
            let mut total_sql = 0;
            let began = Instant::now();
            for _ in 0..count {
                let (elapsed, queries, _) = invoke(false);
                latency.push(elapsed);
                total_sql += queries;
            }
            let seconds = began.elapsed().as_secs_f64();
            assert_eq!(total_sql, statements * count as u64);
            let persisted = rows(
                &rt,
                &store,
                "SELECT scope, calls, input_tokens, output_tokens, cached_tokens, reserved_tokens, reserved_micros FROM jadpo_ai_usage ORDER BY scope",
            );
            assert_eq!(persisted.len(), if caller { 3 } else { 2 });
            for row in &persisted {
                let n = (count + 51) as i64;
                assert_eq!(
                    &row[1..],
                    &[
                        SqlValue::Int4(n as i32),
                        SqlValue::Int8(n * input as i64),
                        SqlValue::Int8(n),
                        SqlValue::Int8(2 * n),
                        SqlValue::Int8(0),
                        SqlValue::Int8(0)
                    ]
                );
            }
            let mut sorted = latency.clone();
            sorted.sort_by(f64::total_cmp);
            runs.push(json!({"round":round+1,"scenario":scenario,"fault_control":true,"refusal_control":true,"postgres_statements":actual_sql,"postgres_log":log,"statements_per_call":statements,"census_sql":census_sql,"census_before":census_before,"census_after":census_after,"before":before,"after":state(&rt,&store),"latency_ms":latency,"p50_ms":sorted[(count-1)/2],"p95_ms":sorted[(count*95).div_ceil(100)-1],"seconds":seconds,"calls_per_second":count as f64/seconds}));
            result["runs"] = json!(runs);
            std::fs::write(
                &output,
                serde_json::to_string_pretty(&result).unwrap() + "\n",
            )
            .unwrap();
        }
    }
    result["complete"] = json!(true);
    result["postgres_version"] = json!(format!("{pg_version:?}"));
    std::fs::write(
        &output,
        serde_json::to_string_pretty(&result).unwrap() + "\n",
    )
    .unwrap();
    drop(store);
    drop(server);
}
