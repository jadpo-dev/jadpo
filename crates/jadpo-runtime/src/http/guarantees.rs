//! The request guarantees (area 7, chunk 4.2), against a store that keeps
//! `jadpo_idempotency_keys` in memory and writes down every statement, so
//! each test reads what reached the store and in which transaction: the
//! body limit, health, deadlines and idempotent writes (chunk 4.2), and
//! the soundness fixes of chunk 4.9: a deadline passed by work that never
//! waits, a concurrent change found at commit, the address limit on a
//! route anyone may call, unreadable headers, and a query string or body
//! a route doesn't read.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use jadpo_std::http::{ApiPlan, RoutePlan, RouteTable, Run, Segment};
use jadpo_std::{HttpMethod, Idempotency, SuccessStatus};
use jadpo_value::Value;
use jadpo_value::json::Codecs;

use super::idempotency::{CLAIM, RELEASE, STORE};
use super::{App, Request, Response, handle};
use crate::{
    ActionBody, Ctx, Database, Env, Executor, Failure, Host, Isolation, LocalFuture, Outcome,
    Places, Row, SqlValue, StoreError, Transaction, block_on,
};

/// One kept key: its digest, status, response, `stored_at`, `kept_until`.
type Kept = (String, Option<i16>, Option<String>, i64, i64);

/// The store's state: the keys table and every line that reached it.
#[derive(Default)]
struct State {
    keys: BTreeMap<(String, String), Kept>,
    lines: Vec<String>,
    /// How many commits to come of the action's writes find a concurrent
    /// change.
    changes: u32,
}

#[derive(Clone, Default)]
struct Store(Rc<RefCell<State>>);

impl Store {
    fn lines(&self) -> Vec<String> {
        self.0.borrow().lines.clone()
    }

    fn line(&self, line: &str) {
        self.0.borrow_mut().lines.push(line.to_string());
    }

    fn kept(&self, key: &str) -> Option<Kept> {
        self.0
            .borrow()
            .keys
            .get(&("anonymous".to_string(), key.to_string()))
            .cloned()
    }

    fn keep(&self, key: &str, kept: Kept) {
        self.0
            .borrow_mut()
            .keys
            .insert(("anonymous".to_string(), key.to_string()), kept);
    }
}

fn text(value: Option<&SqlValue>) -> String {
    match value {
        Some(SqlValue::Text(text) | SqlValue::Json(text)) => text.clone(),
        _ => String::new(),
    }
}

fn time(value: Option<&SqlValue>) -> i64 {
    match value {
        Some(SqlValue::Timestamp(micros)) => *micros,
        _ => 0,
    }
}

/// A transaction: statements on the keys table act as the SQL says; a
/// `STORE` waits for the commit, as it would in PostgreSQL.
struct Tx {
    store: Store,
    pending: Vec<(String, String, Kept)>,
    ended: bool,
    /// It ran the action's write.
    wrote: bool,
}

impl Tx {
    fn run(&mut self, sql: &str, params: &[SqlValue]) -> Result<Vec<Row>, StoreError> {
        let scope = text(params.first());
        let key = text(params.get(1));
        let id = (scope.clone(), key.clone());
        let mut state = self.store.0.borrow_mut();
        if sql == CLAIM {
            state.lines.push("claim".into());
            let at = time(params.get(3));
            let free = state.keys.get(&id).is_none_or(|kept| kept.4 <= at);
            if free {
                let kept = (text(params.get(2)), None, None, at, time(params.get(4)));
                state.keys.insert(id, kept);
                return Ok(vec![vec![
                    SqlValue::Bool(true),
                    SqlValue::Null(crate::SqlType::Text),
                    SqlValue::Null(crate::SqlType::Int2),
                    SqlValue::Null(crate::SqlType::Json),
                ]]);
            }
            return Ok(state
                .keys
                .get(&id)
                .map(|kept| {
                    vec![vec![
                        SqlValue::Bool(false),
                        SqlValue::Text(kept.0.clone()),
                        kept.1
                            .map_or(SqlValue::Null(crate::SqlType::Int2), SqlValue::Int2),
                        kept.2
                            .clone()
                            .map_or(SqlValue::Null(crate::SqlType::Json), SqlValue::Json),
                    ]]
                })
                .unwrap_or_default());
        }
        if sql == STORE {
            state.lines.push("store".into());
            let at = time(params.get(2));
            let Some(kept) = state
                .keys
                .get(&id)
                .filter(|kept| kept.3 == at && kept.1.is_none())
            else {
                return Ok(Vec::new());
            };
            let status = match params.get(3) {
                Some(SqlValue::Int2(status)) => Some(*status),
                _ => None,
            };
            let kept = (
                kept.0.clone(),
                status,
                Some(text(params.get(4))),
                kept.3,
                time(params.get(5)),
            );
            self.pending.push((scope, key, kept));
            return Ok(vec![Vec::new()]);
        }
        if sql == RELEASE {
            state.lines.push("release".into());
            let at = time(params.get(2));
            if state
                .keys
                .get(&id)
                .is_some_and(|kept| kept.3 == at && kept.1.is_none())
            {
                state.keys.remove(&id);
            }
            return Ok(Vec::new());
        }
        self.wrote |= sql == "write";
        state.lines.push(sql.to_string());
        Ok(Vec::new())
    }
}

impl Drop for Tx {
    fn drop(&mut self) {
        if !self.ended {
            self.store.line("dropped");
        }
    }
}

impl Transaction for Tx {
    fn query<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
        let rows = self.run(sql, params);
        Box::pin(async move { rows })
    }

    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<u64, StoreError>> {
        let rows = self.run(sql, params).map(|rows| rows.len() as u64);
        Box::pin(async move { rows })
    }

    fn batch<'a>(&'a mut self, sql: &'a str) -> LocalFuture<'a, Result<(), StoreError>> {
        let done = self.run(sql, &[]).map(|_| ());
        Box::pin(async move { done })
    }

    fn commit(mut self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
        self.ended = true;
        let mut state = self.store.0.borrow_mut();
        if self.wrote && state.changes > 0 {
            state.changes -= 1;
            state.lines.push("changed".into());
            return Box::pin(async { Err(StoreError::ConcurrentChange) });
        }
        for (scope, key, kept) in self.pending.drain(..) {
            state.keys.insert((scope, key), kept);
        }
        state.lines.push("commit".into());
        Box::pin(async { Ok(()) })
    }

    fn rollback(mut self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
        self.ended = true;
        self.store.line("rollback");
        Box::pin(async { Ok(()) })
    }
}

impl Database for Store {
    fn begin_at(
        &self,
        _isolation: Isolation,
    ) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>> {
        self.line("begin");
        let store = self.clone();
        Box::pin(async move {
            Ok(Box::new(Tx {
                store,
                pending: Vec::new(),
                ended: false,
                wrote: false,
            }) as Box<dyn Transaction>)
        })
    }

    /// One statement on its own: it is kept as it runs, with no `begin`
    /// or `commit` line.
    fn query_once<'a>(
        &'a self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
        let mut alone = Tx {
            store: self.clone(),
            pending: Vec::new(),
            ended: true,
            wrote: false,
        };
        let rows = alone.run(sql, params);
        let mut state = self.0.borrow_mut();
        for (scope, key, kept) in alone.pending.drain(..) {
            state.keys.insert((scope, key), kept);
        }
        Box::pin(async move { rows })
    }
}

/// A host with store `main`, unless `no_store`, whose timer runs out at
/// once when `out_of_time`, and never otherwise.
#[derive(Default)]
struct Fake {
    store: Store,
    no_store: bool,
    out_of_time: bool,
    /// A monotonic clock that moves a second each time it is read, when
    /// given: work that never waits runs past a deadline on it.
    clock: Option<Cell<u64>>,
    /// Counts requests against one a minute, whatever the runtime's
    /// default, when given.
    limits: Option<crate::RateLimits>,
    /// A monotonic clock that moves only when the test moves it.
    still: Option<Cell<u64>>,
    /// No wall clock: `now_ms` gives nothing.
    no_wall_clock: bool,
    /// Gives random bytes, which an emitted event's identity takes.
    random: bool,
}

impl Host for Fake {
    fn database(&self, store: &str) -> Option<&dyn Database> {
        (store == "main" && !self.no_store).then_some(&self.store as &dyn Database)
    }

    fn random(&self, bytes: &mut [u8]) -> bool {
        if self.random {
            for (at, byte) in bytes.iter_mut().enumerate() {
                *byte = u8::try_from(at % 251).unwrap_or(0) + 1;
            }
        }
        self.random
    }

    fn sleep(&self, _millis: u64) -> LocalFuture<'_, ()> {
        if self.out_of_time {
            Box::pin(async {})
        } else {
            Box::pin(std::future::pending())
        }
    }

    fn now_ms(&self) -> Option<i64> {
        (!self.no_wall_clock).then_some(NOW)
    }

    fn monotonic_ms(&self) -> Option<u64> {
        if let Some(still) = &self.still {
            return Some(still.get());
        }
        let clock = self.clock.as_ref()?;
        let now = clock.get();
        clock.set(now + 1000);
        Some(now)
    }

    fn rate_limit<'a>(
        &'a self,
        key: &'a str,
        _limit: jadpo_std::http::RateLimit,
        now: i64,
    ) -> LocalFuture<'a, crate::Allowed> {
        let one = jadpo_std::http::RateLimit {
            requests: 1,
            window_ms: 60_000,
        };
        let allowed = self
            .limits
            .as_ref()
            .map_or(crate::Allowed::Yes, |limits| limits.allow(key, one, now));
        Box::pin(async move { allowed })
    }
}

/// How the action ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ends {
    Writes,
    NotFound,
    Unavailable,
    /// Waits for ever after it writes.
    Hangs,
    /// Runs without waiting until the deadline has passed, then succeeds,
    /// as a body whose executor never stopped it would.
    Spins,
    /// Writes, writes down whether the system runs it, as policy would
    /// bind it (`system true` or `system false`), and emits `type Done`.
    Emits,
}

/// Runs the one action: it writes, then ends as `ends` says.
struct Program {
    places: Places,
    ends: Cell<Ends>,
    runs: Cell<u32>,
}

impl Program {
    fn new(ends: Ends) -> Program {
        Program {
            places: Places::new(),
            ends: Cell::new(ends),
            runs: Cell::new(0),
        }
    }
}

struct Body(Ends);

impl<'p> ActionBody<'p> for Body {
    fn run<'c>(&'c mut self, ctx: &'c mut Ctx<'p>) -> LocalFuture<'c, Outcome> {
        let ends = self.0;
        Box::pin(async move {
            if let Ok(transaction) = ctx.transaction("main").await {
                let _ = transaction.execute("write", &[]).await;
            }
            match ends {
                Ends::Writes => Outcome::Value(Value::Unit),
                Ends::NotFound => {
                    let mut failure = Failure::new("type Gone", "Gone", "gone");
                    failure.kind = Some("not_found".into());
                    failure.status = Some(404);
                    failure.message = Some("Gone.".into());
                    Outcome::Failed(Box::new(failure))
                }
                Ends::Unavailable => Outcome::Failed(Box::new(crate::unavailable())),
                Ends::Hangs => std::future::pending().await,
                Ends::Spins => {
                    while !ctx.out_of_time() {}
                    Outcome::Value(Value::Unit)
                }
                Ends::Emits => {
                    let system = ctx.caller_param(jadpo_std::policy::CallerPart::System);
                    if let Ok(transaction) = ctx.transaction("main").await {
                        let line = format!("system {}", system == SqlValue::Bool(true));
                        let _ = transaction.execute(&line, &[]).await;
                    }
                    let _ = ctx.emit(crate::Event {
                        key: "type Done".into(),
                        value: Value::Unit,
                        payload: "null".into(),
                        subscribers: vec!["subscriber s".into()],
                    });
                    Outcome::Value(Value::Unit)
                }
            }
        })
    }

    fn isolation(&self) -> Isolation {
        Isolation::ReadCommitted
    }
}

impl Executor for Program {
    fn places(&self) -> &Places {
        &self.places
    }

    fn run<'a, 'p: 'a>(
        &'a self,
        ctx: &'a mut Ctx<'p>,
        _callable: u32,
        _args: Vec<Value>,
    ) -> LocalFuture<'a, Outcome> {
        self.runs.set(self.runs.get() + 1);
        let mut body = Body(self.ends.get());
        Box::pin(async move { crate::action(ctx, &mut body).await })
    }
}

/// `POST /widgets/{name}`, answering 204, keeping its keys in `keys`; its API
/// requires a key when `required`.
fn table(keys: Option<&str>, required: bool) -> RouteTable {
    RouteTable {
        apis: vec![ApiPlan {
            name: "main".into(),
            prefix: Vec::new(),
            idempotency: if required {
                Idempotency::Required
            } else {
                Idempotency::Optional
            },
            methods: Vec::new(),
        }],
        routes: vec![RoutePlan {
            key: "route POST /widgets".into(),
            method: HttpMethod::Post,
            segments: vec![
                Segment::Fixed("widgets".into()),
                Segment::Param {
                    name: "name".into(),
                    codec: jadpo_std::http::CodecId(0),
                },
            ],
            api: 0,
            input: None,
            output: None,
            status: SuccessStatus::NoContent,
            deadline_ms: 10_000,
            run: Run::Action(0),
            settle: None,
            stream: false,
            keys: keys.map(str::to_string),
            args: Vec::new(),
            authentication: jadpo_std::http::Authentication::Public,
            caller: None,
            file: "domains/widgets/routes/widgets.jadpo".into(),
            line: 1,
        }],
        stores: vec!["main".into()],
        ..RouteTable::default()
    }
}

/// The placeholder's codec: text, with no rules.
const CODECS: [jadpo_std::http::Codec; 1] = [jadpo_std::http::Codec::Scalar {
    builtin: jadpo_std::Builtin::Text,
    validator: jadpo_std::types::ValidatorId(0),
    int_as_number: false,
}];

fn send(
    table: &RouteTable,
    program: &Program,
    host: &Fake,
    request: &Request,
    clock: i64,
) -> Response {
    send_as(
        table,
        program,
        host,
        request,
        clock,
        &super::address::ClientAddress::CONNECTION,
    )
}

/// [`send`] with the client's address chosen as `client` says.
fn send_as(
    table: &RouteTable,
    program: &Program,
    host: &Fake,
    request: &Request,
    clock: i64,
    client: &super::address::ClientAddress,
) -> Response {
    let routes = super::Routes::new(table, &[]);
    let app = App {
        table,
        routes: &routes,
        codecs: Codecs {
            codecs: &CODECS,
            validators: &[],
            shapes: &[],
            coder: None,
        },
        failures: &[],
        json: crate::JsonCodecs::NONE,
        entries: None,
    };
    let env = Env {
        seed: Some(1),
        clock: crate::Clock::fixed(clock),
    };
    let mut ctx = Ctx::new(&program.places, host, env);
    ctx.start(Rc::new(crate::Started::default().with_client(*client)));
    block_on(handle(&app, program, &mut ctx, request, "req_1")).expect("nothing waits")
}

/// `POST /widgets/<name>`, with no body.
fn post(key: Option<&str>, name: &str) -> Request {
    let mut request = Request {
        method: "POST".into(),
        path: format!("/widgets/{name}"),
        ..Request::default()
    };
    if let Some(key) = key {
        request.headers.push(("Idempotency-Key".into(), key.into()));
    }
    request
}

fn code(response: &Response) -> Option<&str> {
    response.code.as_deref()
}

const NOW: i64 = 1_800_000_000_000;
const DAY: i64 = 24 * 60 * 60 * 1000;

#[test]
fn a_body_past_the_limit_is_refused_before_anything_runs() {
    let table = table(Some("main"), false);
    let program = Program::new(Ends::Writes);
    let host = Fake::default();
    let mut request = post(None, "a");
    request.body = vec![b' '; super::BODY_LIMIT + 1];
    let refused = send(&table, &program, &host, &request, NOW);
    assert_eq!(
        (refused.status, code(&refused)),
        (413, Some("request_too_large"))
    );
    let mut cut_short = post(None, "a");
    cut_short.too_large = true;
    let refused = send(&table, &program, &host, &cut_short, NOW);
    assert_eq!(refused.status, 413, "the host stopped reading");
    assert_eq!(program.runs.get(), 0);
    assert!(host.store.lines().is_empty());
}

#[test]
fn health_is_answered_by_the_runtime_from_its_stores() {
    let table = table(Some("main"), false);
    let program = Program::new(Ends::Writes);
    let get = |path: &str| Request {
        method: "GET".into(),
        path: path.into(),
        ..Request::default()
    };
    let host = Fake::default();
    let live = send(&table, &program, &host, &get("/health/live"), NOW);
    assert_eq!(live.status, 200);
    assert_eq!(String::from_utf8_lossy(&live.body), "{\"status\":\"live\"}");
    let ready = send(&table, &program, &host, &get("/health/ready"), NOW);
    assert_eq!(ready.status, 200);
    assert_eq!(
        String::from_utf8_lossy(&ready.body),
        "{\"status\":\"ready\",\"stores\":{\"main\":\"ready\"},\"integrations\":{}}"
    );
    assert_eq!(
        host.store.lines(),
        ["SELECT 1"],
        "each store runs `SELECT 1`, one statement of its own"
    );
    let gone = Fake {
        no_store: true,
        ..Fake::default()
    };
    let not_ready = send(&table, &program, &gone, &get("/health/ready"), NOW);
    assert_eq!(not_ready.status, 503);
    assert_eq!(
        String::from_utf8_lossy(&not_ready.body),
        "{\"status\":\"not_ready\",\"stores\":{\"main\":\"unavailable\"},\"integrations\":{}}"
    );
    let posted = send(
        &table,
        &program,
        &host,
        &Request {
            method: "POST".into(),
            path: "/health/live".into(),
            ..Request::default()
        },
        NOW,
    );
    assert_eq!(posted.status, 405);
    assert_eq!(posted.header("allow"), Some("GET, HEAD, OPTIONS"));
    assert_eq!(program.runs.get(), 0);
}

#[test]
fn readiness_lists_integrations_without_calling_them() {
    let mut encoded = serde_json::to_value(table(Some("main"), false)).expect("table");
    encoded["integrations"] = serde_json::json!(["provider_a", "provider_b"]);
    let table: RouteTable = serde_json::from_value(encoded).expect("table metadata");
    let program = Program::new(Ends::Writes);
    let host = Fake::default();
    let request = |method: &str| Request {
        method: method.into(),
        path: "/health/ready".into(),
        ..Request::default()
    };
    let ready = send(&table, &program, &host, &request("GET"), NOW);
    assert_eq!(ready.status, 200);
    let body: serde_json::Value = serde_json::from_slice(&ready.body).expect("readiness");
    assert_eq!(
        body["integrations"],
        serde_json::json!({"provider_a":"unchecked","provider_b":"unchecked"})
    );
    assert_eq!(ready.header("cache-control"), Some("no-store"));
    let head = send(&table, &program, &host, &request("HEAD"), NOW);
    assert_eq!(head.status, 200);
    assert!(head.body.is_empty());
    let options = send(&table, &program, &host, &request("OPTIONS"), NOW);
    assert_eq!(options.status, 204);
    assert_eq!(options.header("allow"), Some("GET, HEAD, OPTIONS"));
    assert_eq!(
        host.store.lines(),
        ["SELECT 1", "SELECT 1"],
        "only declared stores are probed"
    );
    assert_eq!(program.runs.get(), 0, "readiness executes no action");
    let missing = Fake {
        no_store: true,
        ..Fake::default()
    };
    let failed = send(&table, &program, &missing, &request("GET"), NOW);
    assert_eq!(failed.status, 503);
    let body: serde_json::Value = serde_json::from_slice(&failed.body).expect("readiness");
    assert_eq!(
        body["integrations"],
        serde_json::json!({"provider_a":"unchecked","provider_b":"unchecked"})
    );
}

#[test]
fn a_worker_reuses_its_readiness_check_for_a_second() {
    let table = table(Some("main"), false);
    let program = Program::new(Ends::Writes);
    let routes = super::Routes::new(&table, &[]);
    let app = App {
        table: &table,
        routes: &routes,
        codecs: Codecs {
            codecs: &CODECS,
            validators: &[],
            shapes: &[],
            coder: None,
        },
        failures: &[],
        json: crate::JsonCodecs::NONE,
        entries: None,
    };
    let host = Fake {
        still: Some(Cell::new(5_000)),
        ..Fake::default()
    };
    let worker = super::Worker::default();
    let ready = Request {
        method: "GET".into(),
        path: "/health/ready".into(),
        ..Request::default()
    };
    let probe = || {
        block_on(super::serve_request(&app, &program, &host, &worker, &ready))
            .expect("nothing waits")
    };
    let first = probe();
    assert_eq!(first.status, 200);
    let again = probe();
    assert_eq!((again.status, &again.body), (200, &first.body));
    assert_eq!(host.store.lines(), ["SELECT 1"], "checked once");
    if let Some(still) = &host.still {
        still.set(5_999);
    }
    probe();
    assert_eq!(host.store.lines(), ["SELECT 1"], "still within the second");
    if let Some(still) = &host.still {
        still.set(6_000);
    }
    probe();
    assert_eq!(host.store.lines(), ["SELECT 1", "SELECT 1"], "a second on");
    // Each answer has its own request id.
    let ids: Vec<String> = [probe(), probe()]
        .iter()
        .filter_map(|response| response.header("x-request-id").map(str::to_string))
        .collect();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
    // A route test's requests, which no worker answers, check every time.
    let host = Fake {
        still: Some(Cell::new(5_000)),
        ..Fake::default()
    };
    send(&table, &program, &host, &ready, NOW);
    send(&table, &program, &host, &ready, NOW);
    assert_eq!(host.store.lines(), ["SELECT 1", "SELECT 1"]);
}

/// A deployable answers under the host's clock, so a host without one
/// answers every request, health's included, with a 500 whose fault says
/// so, running nothing and reaching no store, rather than answering at
/// 1970 (chunk 5.8a).
#[test]
fn a_host_without_a_clock_is_refused_rather_than_run_at_1970() {
    let table = table(Some("main"), false);
    let program = Program::new(Ends::Writes);
    let routes = super::Routes::new(&table, &[]);
    let app = App {
        table: &table,
        routes: &routes,
        codecs: Codecs {
            codecs: &CODECS,
            validators: &[],
            shapes: &[],
            coder: None,
        },
        failures: &[],
        json: crate::JsonCodecs::NONE,
        entries: None,
    };
    let host = Fake {
        no_wall_clock: true,
        ..Fake::default()
    };
    let worker = super::Worker::default();
    let live = Request {
        method: "GET".into(),
        path: "/health/live".into(),
        ..Request::default()
    };
    for request in [post(Some("k1"), "a"), live] {
        let refused = block_on(super::serve_request(
            &app, &program, &host, &worker, &request,
        ))
        .expect("nothing waits");
        assert_eq!(
            (refused.status, code(&refused)),
            (500, Some("internal_fault")),
            "{}",
            request.path
        );
        assert!(refused.header("x-request-id").is_some());
        assert_eq!(
            refused.fault.map(|fault| fault.kind),
            Some(crate::FaultKind::Defect("the host has no clock".into()))
        );
    }
    assert_eq!(program.runs.get(), 0);
    assert!(host.store.lines().is_empty());
}

#[test]
fn a_passed_deadline_drops_the_work_commits_nothing_and_answers_504() {
    let table = table(Some("main"), false);
    let program = Program::new(Ends::Hangs);
    let host = Fake {
        out_of_time: true,
        ..Fake::default()
    };
    let timed_out = send(&table, &program, &host, &post(Some("k1"), "a"), NOW);
    assert_eq!(
        (timed_out.status, code(&timed_out)),
        (504, Some("deadline_exceeded"))
    );
    let lines = host.store.lines();
    assert_eq!(
        lines,
        [
            "claim",   // the claim, one statement
            "begin",   // the action,
            "write",   //
            "dropped", // abandoned
            "release", // the claim given up, one statement
        ]
    );
    assert_eq!(host.store.kept("k1"), None, "a retry runs again");
}

#[test]
fn a_write_with_a_key_answers_its_repeat_without_running_again() {
    let table = table(Some("main"), false);
    let program = Program::new(Ends::Writes);
    let host = Fake::default();
    let first = send(&table, &program, &host, &post(Some("k1"), "a"), NOW);
    assert_eq!(first.status, 204);
    assert_eq!(
        host.store.lines(),
        [
            "claim", // the claim, one statement
            "begin", "write", "store", "commit", // the response, with the writes
        ]
    );
    let kept = host.store.kept("k1").expect("kept");
    assert_eq!((kept.1, kept.4), (Some(204), (NOW + DAY) * 1000));
    let again = send(&table, &program, &host, &post(Some("k1"), "a"), NOW + 1000);
    assert_eq!(again.status, 204);
    assert_eq!(program.runs.get(), 1, "the repeat didn't run the action");
    let other = send(&table, &program, &host, &post(Some("k1"), "b"), NOW + 2000);
    assert_eq!(
        (other.status, code(&other)),
        (422, Some("idempotency_key_reused"))
    );
    assert_eq!(program.runs.get(), 1);
    // A day later the key is ignored, and the write runs again.
    let later = send(&table, &program, &host, &post(Some("k1"), "b"), NOW + DAY);
    assert_eq!(later.status, 204);
    assert_eq!(program.runs.get(), 2);
    // Without a key, every request runs.
    send(&table, &program, &host, &post(None, "a"), NOW);
    assert_eq!(program.runs.get(), 3);
}

#[test]
fn a_repeat_while_the_first_runs_is_a_conflict() {
    let table = table(Some("main"), false);
    let program = Program::new(Ends::Writes);
    let host = Fake::default();
    let codecs = Codecs {
        codecs: &CODECS,
        validators: &[],
        shapes: &[],
        coder: None,
    };
    let digest =
        super::idempotency::digest(&codecs, &table.routes[0], &[Some(Value::text("a"))], None);
    host.store.keep(
        "k1",
        (digest, None, None, NOW * 1000, (NOW + 15_000) * 1000),
    );
    let busy = send(&table, &program, &host, &post(Some("k1"), "a"), NOW + 1000);
    assert_eq!((busy.status, code(&busy)), (409, Some("conflict")));
    assert_eq!(program.runs.get(), 0);
    // A claim past its deadline was left by a request that never finished.
    let after = send(
        &table,
        &program,
        &host,
        &post(Some("k1"), "a"),
        NOW + 16_000,
    );
    assert_eq!(after.status, 204);
    assert_eq!(program.runs.get(), 1);
}

#[test]
fn declared_failures_are_kept_and_unavailable_is_not() {
    let table = table(Some("main"), false);
    let program = Program::new(Ends::NotFound);
    let host = Fake::default();
    let failed = send(&table, &program, &host, &post(Some("k1"), "a"), NOW);
    assert_eq!((failed.status, code(&failed)), (404, Some("gone")));
    let kept = host.store.kept("k1").expect("kept");
    assert_eq!(kept.1, Some(404));
    program.ends.set(Ends::Writes);
    let again = send(&table, &program, &host, &post(Some("k1"), "a"), NOW + 1000);
    assert_eq!(
        (again.status, code(&again), again.body.clone()),
        (404, Some("gone"), failed.body.clone()),
        "the stored response, as it was sent"
    );
    assert_eq!(program.runs.get(), 1);

    program.ends.set(Ends::Unavailable);
    let unavailable = send(&table, &program, &host, &post(Some("k2"), "a"), NOW);
    assert_eq!(unavailable.status, 503);
    assert_eq!(host.store.kept("k2"), None, "dropped, so a retry runs");
    program.ends.set(Ends::Writes);
    let retried = send(&table, &program, &host, &post(Some("k2"), "a"), NOW + 1000);
    assert_eq!(retried.status, 204);
}

#[test]
fn an_api_can_require_the_key_and_a_bad_key_is_refused() {
    let table = table(Some("main"), true);
    let program = Program::new(Ends::Writes);
    let host = Fake::default();
    let missing = send(&table, &program, &host, &post(None, "a"), NOW);
    assert_eq!(
        (missing.status, code(&missing)),
        (400, Some("invalid_request"))
    );
    let long = "k".repeat(256);
    for bad in ["", long.as_str(), "caf\u{e9}"] {
        let refused = send(&table, &program, &host, &post(Some(bad), "a"), NOW);
        assert_eq!(refused.status, 400, "{bad:?}");
    }
    assert_eq!(program.runs.get(), 0);
    let sent = send(&table, &program, &host, &post(Some("k1"), "a"), NOW);
    assert_eq!(sent.status, 204);
}

#[test]
fn an_action_that_writes_no_store_keeps_no_key() {
    let table = table(None, false);
    let program = Program::new(Ends::Writes);
    let host = Fake::default();
    send(&table, &program, &host, &post(Some("k1"), "a"), NOW);
    send(&table, &program, &host, &post(Some("k1"), "a"), NOW);
    assert_eq!(program.runs.get(), 2, "running it again changes nothing");
    assert!(!host.store.lines().iter().any(|line| line == "claim"));
}

#[test]
fn work_that_never_waits_past_its_deadline_commits_nothing_and_answers_504() {
    let table = table(Some("main"), false);
    let program = Program::new(Ends::Spins);
    let host = Fake {
        clock: Some(Cell::new(0)),
        ..Fake::default()
    };
    let timed_out = send(&table, &program, &host, &post(Some("k1"), "a"), NOW);
    assert_eq!(
        (timed_out.status, code(&timed_out)),
        (504, Some("deadline_exceeded"))
    );
    assert_eq!(
        host.store.lines(),
        [
            "claim",   // the claim, one statement
            "begin",   // the action,
            "write",   //
            "dropped", // never committed
            "release", // the claim given up, one statement
        ]
    );
    assert_eq!(host.store.kept("k1"), None, "a retry runs again");
}

#[test]
fn a_concurrent_change_found_at_commit_runs_the_action_again() {
    let table = table(Some("main"), false);
    let program = Program::new(Ends::Writes);
    let host = Fake::default();
    host.store.0.borrow_mut().changes = 1;
    let answered = send(&table, &program, &host, &post(Some("k1"), "a"), NOW);
    assert_eq!(answered.status, 204, "{:?}", host.store.lines());
    assert_eq!(program.runs.get(), 2, "run again, not answered 503");
    assert_eq!(
        host.store.lines(),
        [
            "claim", // the claim, one statement
            "begin", "write", "store", "changed", // the first run
            "begin", "write", "store", "commit", // the second
        ]
    );
    assert_eq!(host.store.kept("k1").and_then(|kept| kept.1), Some(204));
    // Past the budget it is unavailable, and the key is given up.
    let host = Fake::default();
    host.store.0.borrow_mut().changes = crate::RERUN_ATTEMPTS;
    let program = Program::new(Ends::Writes);
    let gave_up = send(&table, &program, &host, &post(Some("k1"), "a"), NOW);
    assert_eq!((gave_up.status, code(&gave_up)), (503, Some("unavailable")));
    assert_eq!(program.runs.get(), crate::RERUN_ATTEMPTS);
    assert_eq!(host.store.kept("k1"), None);
}

#[test]
fn a_credential_on_a_route_anyone_may_call_counts_its_address_before_it_runs() {
    let table = table(Some("main"), false);
    let program = Program::new(Ends::Writes);
    let host = Fake {
        limits: Some(crate::RateLimits::new(1 << 20)),
        ..Fake::default()
    };
    let mut junk = post(None, "a");
    junk.headers.push(("Authorization".into(), "junk".into()));
    junk.address = Some("10.0.0.1".into());
    let first = send(&table, &program, &host, &junk, NOW);
    assert_eq!(first.status, 204);
    let limited = send(&table, &program, &host, &junk, NOW);
    assert_eq!(
        (limited.status, code(&limited)),
        (429, Some("rate_limited"))
    );
    assert_eq!(program.runs.get(), 1, "the refused request never ran");
}

/// An IPv6 client counts by its /64, the least a network hands one
/// subscriber, so moving through its own block buys no allowance (chunk
/// 8.4d); the request line still shows the whole address.
#[test]
fn an_ipv6_client_is_counted_by_its_64() {
    let table = table(Some("main"), false);
    let program = Program::new(Ends::Writes);
    let host = Fake {
        limits: Some(crate::RateLimits::new(1 << 20)),
        ..Fake::default()
    };
    let from = |address: &str| {
        let mut request = post(None, "a");
        request.address = Some(address.into());
        request
    };
    let first = send(&table, &program, &host, &from("2001:db8:1:2::17"), NOW);
    assert_eq!(
        (first.status, first.client.as_deref()),
        (204, Some("2001:db8:1:2::17"))
    );
    let neighbour = send(
        &table,
        &program,
        &host,
        &from("2001:db8:1:2:ffff:ffff:ffff:ffff"),
        NOW,
    );
    assert_eq!(
        (neighbour.status, code(&neighbour)),
        (429, Some("rate_limited")),
        "an address in the same /64 shares its allowance"
    );
    let elsewhere = send(&table, &program, &host, &from("2001:db8:1:3::17"), NOW);
    assert_eq!(elsewhere.status, 204, "another /64 has its own");
    let four = send(&table, &program, &host, &from("203.0.113.9"), NOW);
    let four_next = send(&table, &program, &host, &from("203.0.113.10"), NOW);
    assert_eq!(
        (four.status, four_next.status),
        (204, 204),
        "IPv4 by address"
    );
}

#[test]
fn two_clients_behind_a_proxy_are_counted_apart() {
    use super::address::{ClientAddress, Target};
    use jadpo_std::ClientAddressSource;
    use jadpo_std::http::{ClientAddressPlan, Hops};

    let table = table(Some("main"), false);
    let program = Program::new(Ends::Writes);
    let host = Fake {
        limits: Some(crate::RateLimits::new(1 << 20)),
        ..Fake::default()
    };
    // From behind one proxy: every connection is the proxy's, and the proxy
    // adds the address it saw to the right of whatever the caller sent.
    let behind = |forwarded: &str| {
        let mut request = post(None, "a");
        request.address = Some("10.0.0.1".into());
        request
            .headers
            .push(("X-Forwarded-For".into(), forwarded.into()));
        request
    };
    let believing = ClientAddress::read(
        Some(&ClientAddressPlan {
            source: ClientAddressSource::ForwardedFor,
            hops: Some(Hops::Count(1)),
        }),
        &crate::config::ConfigValues::default(),
        Target::Native,
    )
    .expect("accepted");
    let first_client = send_as(
        &table,
        &program,
        &host,
        &behind("203.0.113.9"),
        NOW,
        &believing,
    );
    assert_eq!(
        (first_client.status, first_client.client.as_deref()),
        (204, Some("203.0.113.9"))
    );
    let second_client = send_as(
        &table,
        &program,
        &host,
        &behind("198.51.100.4"),
        NOW,
        &believing,
    );
    assert_eq!(
        (second_client.status, second_client.client.as_deref()),
        (204, Some("198.51.100.4")),
        "a second client has an allowance of its own"
    );
    let again = send_as(
        &table,
        &program,
        &host,
        &behind("203.0.113.9"),
        NOW,
        &believing,
    );
    assert_eq!((again.status, code(&again)), (429, Some("rate_limited")));
    // A caller writing another address to the left of the proxy's is still
    // the address the proxy saw.
    let claiming = send_as(
        &table,
        &program,
        &host,
        &behind("192.0.2.77, 203.0.113.9"),
        NOW,
        &believing,
    );
    assert_eq!(
        (claiming.status, claiming.client.as_deref()),
        (429, Some("203.0.113.9"))
    );
    // Without `client_address:` the header is not read, so the proxy's own
    // address is every client's, which is the problem the setting solves.
    let first = send(&table, &program, &host, &behind("203.0.113.50"), NOW);
    assert_eq!(
        (first.status, first.client.as_deref()),
        (204, Some("10.0.0.1"))
    );
    let second = send(&table, &program, &host, &behind("198.51.100.50"), NOW);
    assert_eq!((second.status, code(&second)), (429, Some("rate_limited")));
    assert_eq!(program.runs.get(), 3);
}

#[test]
fn a_header_that_isnt_printable_ascii_is_refused_never_dropped() {
    let table = table(Some("main"), false);
    let program = Program::new(Ends::Writes);
    let host = Fake::default();
    for (name, value) in [
        ("Authorization", "Bearer caf\u{e9}"),
        ("Idempotency-Key", "caf\u{e9}"),
        ("X-Anything", "\u{fffd}"),
    ] {
        let mut request = post(None, "a");
        request.headers.push((name.into(), value.into()));
        let refused = send(&table, &program, &host, &request, NOW);
        assert_eq!(
            (refused.status, code(&refused)),
            (400, Some("invalid_request")),
            "{name}"
        );
    }
    assert_eq!(program.runs.get(), 0);
    let mut tabbed = post(None, "a");
    tabbed.headers.push(("X-Anything".into(), "a\tb c".into()));
    assert_eq!(send(&table, &program, &host, &tabbed, NOW).status, 204);
}

#[test]
fn a_route_without_input_refuses_a_query_string_and_a_body() {
    let table = table(Some("main"), false);
    let program = Program::new(Ends::Writes);
    let host = Fake::default();
    let mut queried = post(None, "a");
    queried.query = Some("k1=x&k2=2".into());
    let refused = send(&table, &program, &host, &queried, NOW);
    assert_eq!(refused.status, 422);
    assert_eq!(
        String::from_utf8_lossy(&refused.body),
        "{\"error\":{\"code\":\"invalid_input\",\"message\":\"Request input is invalid.\",\"request_id\":\"req_1\",\"details\":{\"problems\":[{\"field\":\"k1\",\"rule\":\"unknown_field\"},{\"field\":\"k2\",\"rule\":\"unknown_field\"}]}}}"
    );
    let mut bodied = post(None, "a");
    bodied.body = b"{}".to_vec();
    let refused = send(&table, &program, &host, &bodied, NOW);
    assert_eq!(
        String::from_utf8_lossy(&refused.body),
        "{\"error\":{\"code\":\"invalid_input\",\"message\":\"Request input is invalid.\",\"request_id\":\"req_1\",\"details\":{\"problems\":[{\"field\":\"\",\"rule\":\"unknown_field\"}]}}}"
    );
    assert_eq!(program.runs.get(), 0);
    let mut blank = post(None, "a");
    blank.body = b" \r\n".to_vec();
    blank.query = Some(String::new());
    assert_eq!(send(&table, &program, &host, &blank, NOW).status, 204);
}

/// What an action emitted waits for the runner until its response is
/// built, and only once it committed: the response carries it for the
/// host to run after it is sent (chunk 6.1a), so delivery never holds a
/// request up.
#[test]
fn what_a_request_emits_is_left_for_after_its_response() {
    let table = table(Some("main"), false);
    let routes = super::Routes::new(&table, &[]);
    let app = App {
        table: &table,
        routes: &routes,
        codecs: Codecs {
            codecs: &CODECS,
            validators: &[],
            shapes: &[],
            coder: None,
        },
        failures: &[],
        json: crate::JsonCodecs::NONE,
        entries: None,
    };
    let worker = super::Worker::default();
    let program = Program::new(Ends::Emits);
    let host = Fake {
        random: true,
        ..Fake::default()
    };
    let response = block_on(super::serve_request(
        &app,
        &program,
        &host,
        &worker,
        &post(Some("k1"), "a"),
    ))
    .expect("nothing waits");
    assert_eq!(response.status, 204);
    let [crate::runner::Deferred::Deliveries(deliveries)] = response.deferred.as_slice() else {
        panic!("one event's deliveries: {:?}", response.deferred);
    };
    assert_eq!(
        deliveries
            .iter()
            .map(|d| (d.event.as_str(), d.subscriber.as_str(), d.store.as_str()))
            .collect::<Vec<_>>(),
        [("type Done", "subscriber s", "main")]
    );
    assert!(deliveries[0].trace.is_some(), "the request's trace carried");
    let lines = host.store.lines();
    assert_eq!(
        lines.last().map(String::as_str),
        Some("commit"),
        "{lines:?}"
    );
    // An action that fails leaves nothing.
    let program = Program::new(Ends::NotFound);
    let host = Fake::default();
    let response = block_on(super::serve_request(
        &app,
        &program,
        &host,
        &worker,
        &post(Some("k2"), "a"),
    ))
    .expect("nothing waits");
    assert_eq!(response.status, 404);
    assert!(response.deferred.is_empty());
}

/// No request ever acts as the system, the caller only the runner gives
/// a schedule's run or a subscriber's delivery (area 9, `Access.system`;
/// chunk 6.1a): not a deployable's, and not one answered in a context the
/// runner had made the system's.
#[test]
fn no_request_is_ever_the_system() {
    let table = table(Some("main"), false);
    let program = Program::new(Ends::Emits);
    let host = Fake {
        random: true,
        ..Fake::default()
    };
    let routes = super::Routes::new(&table, &[]);
    let app = App {
        table: &table,
        routes: &routes,
        codecs: Codecs {
            codecs: &CODECS,
            validators: &[],
            shapes: &[],
            coder: None,
        },
        failures: &[],
        json: crate::JsonCodecs::NONE,
        entries: None,
    };
    let worker = super::Worker::default();
    let served = block_on(super::serve_request(
        &app,
        &program,
        &host,
        &worker,
        &post(Some("k1"), "a"),
    ))
    .expect("nothing waits");
    assert_eq!(served.status, 204);
    let mut ctx = Ctx::new(&program.places, &host, Env::default());
    ctx.become_system(crate::System {
        tenant: Some([7; 16]),
    });
    let handled = block_on(handle(
        &app,
        &program,
        &mut ctx,
        &post(Some("k2"), "b"),
        "req_2",
    ))
    .expect("nothing waits");
    assert_eq!(handled.status, 204);
    let lines = host.store.lines();
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.starts_with("system"))
            .collect::<Vec<_>>(),
        ["system false", "system false"],
        "{lines:?}"
    );
    assert_eq!(ctx.system(), None);
    assert_eq!(
        ctx.caller_param(jadpo_std::policy::CallerPart::SystemTenant),
        SqlValue::Null(crate::SqlType::Uuid)
    );
}
