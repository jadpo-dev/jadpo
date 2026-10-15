//! The request guarantees (area 7, chunk 4.2), against a store that keeps
//! `jadpo_idempotency_keys` in memory and writes down every statement, so
//! each test reads what reached the store and in which transaction: the
//! body limit, health, deadlines and idempotent writes.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use jadpo_std::http::{ApiPlan, RoutePlan, RouteTable, Run, Segment};
use jadpo_std::{HttpMethod, Idempotency, SuccessStatus};
use jadpo_value::Value;
use jadpo_value::json::Codecs;

use super::idempotency::{CLAIM, LOOKUP, RELEASE, STORE};
use super::{App, Request, Response, handle};
use crate::{
    ActionBody, Ctx, Database, Env, Event, Executor, Failure, Host, Isolation, LocalFuture,
    Outcome, Places, Row, SqlValue, StoreError, Transaction, block_on,
};

/// One kept key: its digest, status, response, `stored_at`, `kept_until`.
type Kept = (String, Option<i16>, Option<String>, i64, i64);

/// The store's state: the keys table and every line that reached it.
#[derive(Default)]
struct State {
    keys: BTreeMap<(String, String), Kept>,
    lines: Vec<String>,
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
                return Ok(vec![vec![SqlValue::Text(scope)]]);
            }
            return Ok(Vec::new());
        }
        if sql == LOOKUP {
            state.lines.push("lookup".into());
            let now = time(params.get(2));
            return Ok(state
                .keys
                .get(&id)
                .filter(|kept| kept.4 > now)
                .map(|kept| {
                    vec![vec![
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
            }) as Box<dyn Transaction>)
        })
    }
}

/// A host with store `main`, unless `no_store`, whose timer runs out at
/// once when `out_of_time`, and never otherwise.
#[derive(Default)]
struct Fake {
    store: Store,
    no_store: bool,
    out_of_time: bool,
}

impl Host for Fake {
    fn database(&self, store: &str) -> Option<&dyn Database> {
        (store == "main" && !self.no_store).then_some(&self.store as &dyn Database)
    }

    fn deliver(&self, _events: Vec<Event>) -> LocalFuture<'_, ()> {
        Box::pin(async {})
    }

    fn random(&self, _bytes: &mut [u8]) -> bool {
        false
    }

    fn sleep(&self, _millis: u64) -> LocalFuture<'_, ()> {
        if self.out_of_time {
            Box::pin(async {})
        } else {
            Box::pin(std::future::pending())
        }
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

    fn callable(&self, _key: &str) -> Option<u32> {
        Some(0)
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

/// `POST /notes`, answering 204, keeping its keys in `keys`; its API
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
            key: "route POST /notes".into(),
            method: HttpMethod::Post,
            segments: vec![Segment::Fixed("notes".into())],
            api: 0,
            input: None,
            output: None,
            status: SuccessStatus::NoContent,
            deadline_ms: 10_000,
            run: Run::Action(0),
            keys: keys.map(str::to_string),
            args: Vec::new(),
            authentication: jadpo_std::http::Authentication::Public,
            caller: None,
            file: "domains/notes/routes/notes.jadpo".into(),
            line: 1,
        }],
        stores: vec!["main".into()],
        ..RouteTable::default()
    }
}

fn send(
    table: &RouteTable,
    program: &Program,
    host: &Fake,
    request: &Request,
    clock: i64,
) -> Response {
    let app = App {
        table,
        codecs: Codecs {
            codecs: &[],
            validators: &[],
            shapes: &[],
        },
        failures: &[],
    };
    let env = Env {
        seed: Some(1),
        clock: crate::Clock::fixed(clock),
    };
    let mut ctx = Ctx::new(&program.places, host, env);
    block_on(handle(&app, program, &mut ctx, request, "req_1", None)).expect("nothing waits")
}

fn post(key: Option<&str>, body: &str) -> Request {
    let mut request = Request {
        method: "POST".into(),
        path: "/notes".into(),
        body: body.as_bytes().to_vec(),
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
    let mut request = post(None, "");
    request.body = vec![b' '; super::BODY_LIMIT + 1];
    let refused = send(&table, &program, &host, &request, NOW);
    assert_eq!(
        (refused.status, code(&refused)),
        (413, Some("request_too_large"))
    );
    let mut cut_short = post(None, "{}");
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
        ["begin", "SELECT 1", "rollback"],
        "each store runs `SELECT 1`"
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
fn a_passed_deadline_drops_the_work_commits_nothing_and_answers_504() {
    let table = table(Some("main"), false);
    let program = Program::new(Ends::Hangs);
    let host = Fake {
        out_of_time: true,
        ..Fake::default()
    };
    let timed_out = send(&table, &program, &host, &post(Some("k1"), "{}"), NOW);
    assert_eq!(
        (timed_out.status, code(&timed_out)),
        (504, Some("deadline_exceeded"))
    );
    let lines = host.store.lines();
    assert_eq!(
        lines,
        [
            "begin", "claim", "commit", // the claim
            "begin", "write", "dropped", // the action, abandoned
            "begin", "release", "commit", // the claim given up
        ]
    );
    assert_eq!(host.store.kept("k1"), None, "a retry runs again");
}

#[test]
fn a_write_with_a_key_answers_its_repeat_without_running_again() {
    let table = table(Some("main"), false);
    let program = Program::new(Ends::Writes);
    let host = Fake::default();
    let first = send(&table, &program, &host, &post(Some("k1"), "{}"), NOW);
    assert_eq!(first.status, 204);
    assert_eq!(
        host.store.lines(),
        [
            "begin", "claim", "commit", // the claim
            "begin", "write", "store", "commit", // the response, with the writes
        ]
    );
    let kept = host.store.kept("k1").expect("kept");
    assert_eq!((kept.1, kept.4), (Some(204), (NOW + DAY) * 1000));
    let again = send(&table, &program, &host, &post(Some("k1"), "{}"), NOW + 1000);
    assert_eq!(again.status, 204);
    assert_eq!(program.runs.get(), 1, "the repeat didn't run the action");
    let other = send(
        &table,
        &program,
        &host,
        &post(Some("k1"), "{\"x\":1}"),
        NOW + 2000,
    );
    assert_eq!(
        (other.status, code(&other)),
        (422, Some("idempotency_key_reused"))
    );
    assert_eq!(program.runs.get(), 1);
    // A day later the key is ignored, and the write runs again.
    let later = send(
        &table,
        &program,
        &host,
        &post(Some("k1"), "{\"x\":1}"),
        NOW + DAY,
    );
    assert_eq!(later.status, 204);
    assert_eq!(program.runs.get(), 2);
    // Without a key, every request runs.
    send(&table, &program, &host, &post(None, "{}"), NOW);
    assert_eq!(program.runs.get(), 3);
}

#[test]
fn a_repeat_while_the_first_runs_is_a_conflict() {
    let table = table(Some("main"), false);
    let program = Program::new(Ends::Writes);
    let host = Fake::default();
    let digest = super::idempotency::digest(&post(Some("k1"), "{}"), &table.routes[0]);
    host.store.keep(
        "k1",
        (digest, None, None, NOW * 1000, (NOW + 15_000) * 1000),
    );
    let busy = send(&table, &program, &host, &post(Some("k1"), "{}"), NOW + 1000);
    assert_eq!((busy.status, code(&busy)), (409, Some("conflict")));
    assert_eq!(program.runs.get(), 0);
    // A claim past its deadline was left by a request that never finished.
    let after = send(
        &table,
        &program,
        &host,
        &post(Some("k1"), "{}"),
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
    let failed = send(&table, &program, &host, &post(Some("k1"), "{}"), NOW);
    assert_eq!((failed.status, code(&failed)), (404, Some("gone")));
    let kept = host.store.kept("k1").expect("kept");
    assert_eq!(kept.1, Some(404));
    program.ends.set(Ends::Writes);
    let again = send(&table, &program, &host, &post(Some("k1"), "{}"), NOW + 1000);
    assert_eq!(
        (again.status, code(&again), again.body.clone()),
        (404, Some("gone"), failed.body.clone()),
        "the stored response, as it was sent"
    );
    assert_eq!(program.runs.get(), 1);

    program.ends.set(Ends::Unavailable);
    let unavailable = send(&table, &program, &host, &post(Some("k2"), "{}"), NOW);
    assert_eq!(unavailable.status, 503);
    assert_eq!(host.store.kept("k2"), None, "dropped, so a retry runs");
    program.ends.set(Ends::Writes);
    let retried = send(&table, &program, &host, &post(Some("k2"), "{}"), NOW + 1000);
    assert_eq!(retried.status, 204);
}

#[test]
fn an_api_can_require_the_key_and_a_bad_key_is_refused() {
    let table = table(Some("main"), true);
    let program = Program::new(Ends::Writes);
    let host = Fake::default();
    let missing = send(&table, &program, &host, &post(None, "{}"), NOW);
    assert_eq!(
        (missing.status, code(&missing)),
        (400, Some("invalid_request"))
    );
    let long = "k".repeat(256);
    for bad in ["", long.as_str(), "caf\u{e9}"] {
        let refused = send(&table, &program, &host, &post(Some(bad), "{}"), NOW);
        assert_eq!(refused.status, 400, "{bad:?}");
    }
    assert_eq!(program.runs.get(), 0);
    let sent = send(&table, &program, &host, &post(Some("k1"), "{}"), NOW);
    assert_eq!(sent.status, 204);
}

#[test]
fn an_action_that_writes_no_store_keeps_no_key() {
    let table = table(None, false);
    let program = Program::new(Ends::Writes);
    let host = Fake::default();
    send(&table, &program, &host, &post(Some("k1"), "{}"), NOW);
    send(&table, &program, &host, &post(Some("k1"), "{}"), NOW);
    assert_eq!(program.runs.get(), 2, "running it again changes nothing");
    assert!(!host.store.lines().iter().any(|line| line == "claim"));
}
