//! Background work within its host's budget, against live PostgreSQL
//! (area 12, "Limits and runtime faults"; chunk 6.7, from the Phase 6
//! architecture review): each invocation of background work is given a
//! budget by its host, and the runner stops taking work and gives back
//! what it holds before the budget ends, so no claim outlives the
//! invocation that took it; a batch run carries on, by time, in a fresh
//! invocation; and a native server that is asked to stop drains, giving
//! back the delivery it was running.
//!
//! A notice board: posting a note emits `NotePosted`, which `echo_note`
//! records an echo of; and a shelf of items, which `touch_all` touches
//! two at a time in a batch run.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::cell::{Cell, RefCell};
use std::fs;
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use jadpo_cli::{Output, run};
use jadpo_host_native::{Postgres, Runtime, postgres};
use jadpo_interp::Program;
use jadpo_runtime::budget::Invocation;
use jadpo_runtime::runner::{Deferred, HandOn, after_response, carry_on, run_deferred};
use jadpo_runtime::{
    Clock, Ctx, Database, Env, Executor, Host, Isolation, LocalFuture, Outcome, Row, SqlValue,
    StoreError, Transaction,
};
use jadpo_term::Environment;
use jadpo_value::Value;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    run(&args, &env.with_cwd(cwd))
}

const FILES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"application {
    events: {
        store: main
    }
}
"#,
    ),
    (
        "domains/board/types/words.jadpo",
        r#"type Words = Text {
    min_length: 1
    max_length: 200
}
"#,
    ),
    (
        "domains/board/types/note_request.jadpo",
        r#"use board/types { Words }

type NoteRequest = Input {
    words: Words
}
"#,
    ),
    (
        "domains/board/types/note_view.jadpo",
        r#"use board/entities { Note }
use board/types { Words }

type NoteView = Output {
    id: Note.id
    words: Words
}
"#,
    ),
    (
        "domains/board/entities/note.jadpo",
        r#"use board/types { Words }

entity Note {
    id: Uuid
    words: Words

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
    }
}
"#,
    ),
    (
        "domains/board/entities/echo.jadpo",
        r#"use board/entities { Note }

entity Echo {
    id: Uuid
    note_id: Note.id {
        on_delete: OnDelete.cascade
    }

    policy: {
        create: [Access.system]
        read: [Access.public, Access.system]
    }
}
"#,
    ),
    (
        "domains/board/events/note_posted.jadpo",
        r#"use board/entities { Note }

/// A note was posted.
type NotePosted = Event {
    note_id: Note.id
}
"#,
    ),
    (
        "domains/board/actions/post_note.jadpo",
        r#"use board/entities { Note }
use board/events { NotePosted }
use board/types { NoteView, Words }

action post_note(words: Words) -> NoteView {
    var note: Note = Note.create {
        set: {
            words: words
        }
    }
    emit NotePosted {
        note_id: note.id
    }
    return NoteView {
        id: note.id
        words: note.words
    }
}
"#,
    ),
    (
        "domains/board/actions/record_echo.jadpo",
        r#"use board/entities { Echo, Note, NoteNotFound }

action record_echo(note_id: Note.id) fails NoteNotFound -> Unit {
    var echo: Echo = attempt Echo.create {
        set: {
            note_id: note_id
        }
    }
    assert echo.note_id == note_id
}
"#,
    ),
    (
        "domains/board/subscribers/echo_note.jadpo",
        r#"use board/actions { record_echo }
use board/events { NotePosted }

/// Records an echo of each note posted.
subscriber echo_note {
    on: NotePosted
    run: record_echo(event.note_id)
}
"#,
    ),
    (
        "domains/board/routes/notes.jadpo",
        r#"use board/actions { post_note }
use board/types { NoteRequest, NoteView }

routes Notes {
    prefix: /notes

    /// Posts a note.
    POST / {
        authentication: RouteAuthentication.none
        input: NoteRequest
        output: NoteView
        status: SuccessStatus.created
        run: post_note(input.words)
    }
}
"#,
    ),
    (
        "domains/shelf/types/code.jadpo",
        r#"type Code = Text {
    min_length: 1
    max_length: 40
}
"#,
    ),
    (
        "domains/shelf/types/item_page.jadpo",
        r#"type ItemPage = Input {
    page_size: PageSize {
        default: 2
    }
    after: PageCursor {
        optional: true
    }
}
"#,
    ),
    (
        "domains/shelf/entities/item.jadpo",
        r#"use shelf/types { Code }

entity Item {
    id: Uuid
    code: Code {
        immutable: true
    }
    touches: Int

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.system]
    }
}
"#,
    ),
    (
        "domains/shelf/queries/items_page.jadpo",
        r#"use shelf/entities { Item }
use shelf/types { ItemPage }

query items_page(page: ItemPage) -> Page<Item> {
    from: Item
    order: [Item.code asc]
    page: {
        size: page.page_size
        after: page.after
    }
}
"#,
    ),
    (
        "domains/shelf/actions/add_item.jadpo",
        r#"use shelf/entities { Item }
use shelf/types { Code }

action add_item(code: Code) -> Item {
    return Item.create {
        set: {
            code: code
            touches: 0
        }
    }
}
"#,
    ),
    (
        "domains/shelf/actions/touch.jadpo",
        r#"use shelf/entities { Item }

action touch(item: Item) -> Unit {
    var touched: Item = Item.update(item) {
        set: {
            touches: item.touches + 1
        }
    }
    assert touched.touches == item.touches + 1
}
"#,
    ),
    (
        "domains/shelf/actions/touch_all.jadpo",
        r#"use shelf/actions { touch }
use shelf/queries { items_page }
use shelf/types { ItemPage }

action touch_all() -> BatchRun {
    return batches.each(
        from: items_page(ItemPage {}),
        each: item -> touch(item)
    )
}
"#,
    ),
];

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("background-budget-6.7-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let recorded = jadpo(&root, &["records", "update"]);
    assert_eq!(recorded.code, 0, "{}{}", recorded.stdout, recorded.stderr);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    root
}

/// When the test starts: 2026-10-19T09:00:00Z.
const T0: i64 = 1_792_400_400_000;

/// A store, the host's clocks and how long its invocations may run: the
/// wall clock fixed at [`T0`]; the monotonic clock either the process's
/// own, with tokio's timer, or one the test moves, `step` on each commit,
/// with timers that never end. Statements naming `"echo"` never end once
/// `stall` is set. Each claim of a delivery is noted with the end of its
/// lease, and the work the host is given to run after a response or as a
/// fresh invocation is kept for the test to run.
struct Budgeted {
    db: Postgres,
    budget_ms: u64,
    real_time: bool,
    began: Instant,
    monotonic: Rc<Cell<u64>>,
    step: u64,
    stall: Rc<Cell<bool>>,
    seeds: Cell<u64>,
    drawn: Cell<u64>,
    claims: RefCell<Vec<(i64, i64)>>,
    deferred: RefCell<Vec<LocalFuture<'static, ()>>>,
    handed: RefCell<Vec<HandOn>>,
    stop_after_claims: Cell<Option<usize>>,
    stopping: Cell<bool>,
    stopping_waker: RefCell<Option<std::task::Waker>>,
    expire_after_claims: Cell<Option<usize>>,
    timer_waker: RefCell<Option<std::task::Waker>>,
}

impl Budgeted {
    fn new(url: &str, budget_ms: u64, real_time: bool, step: u64) -> Budgeted {
        Budgeted {
            db: postgres(url).expect("store"),
            budget_ms,
            real_time,
            began: Instant::now(),
            monotonic: Rc::new(Cell::new(0)),
            step,
            stall: Rc::new(Cell::new(false)),
            seeds: Cell::new(0),
            drawn: Cell::new(0),
            claims: RefCell::new(Vec::new()),
            deferred: RefCell::new(Vec::new()),
            handed: RefCell::new(Vec::new()),
            stop_after_claims: Cell::new(None),
            stopping: Cell::new(false),
            stopping_waker: RefCell::new(None),
            expire_after_claims: Cell::new(None),
            timer_waker: RefCell::new(None),
        }
    }

    fn ctx<'p>(&'p self, program: &'p Program) -> Ctx<'p> {
        self.seeds.set(self.seeds.get() + 1);
        let env = Env {
            seed: Some(self.seeds.get()),
            clock: Clock::fixed(T0),
        };
        Ctx::of(program, self, env)
    }

    async fn rows(&self, sql: &str) -> Vec<Row> {
        self.db
            .query_once(sql, &[])
            .await
            .expect("the store answers")
    }

    async fn count(&self, sql: &str) -> i64 {
        match self.rows(sql).await.first().and_then(|row| row.first()) {
            Some(SqlValue::Int8(n)) => *n,
            Some(SqlValue::Int4(n)) => i64::from(*n),
            other => panic!("not a count: {other:?}"),
        }
    }

    /// Runs `action` with `args` as no one signed in, giving the work it
    /// left for after its response.
    async fn call(&self, program: &Program, action: &str, args: Vec<Value>) -> Vec<Deferred> {
        let id = program.plan().find(action).expect("the action");
        let mut ctx = self.ctx(program);
        match program.run(&mut ctx, id, args).await {
            Outcome::Value(_) => {}
            other => panic!("`{action}` runs: {other:?}"),
        }
        ctx.take_deferred()
    }
}

impl Host for Budgeted {
    fn database(&self, _store: &str) -> Option<&dyn Database> {
        Some(self)
    }

    fn random(&self, bytes: &mut [u8]) -> bool {
        for chunk in bytes.chunks_mut(8) {
            self.drawn.set(self.drawn.get() + 1);
            let mut z = self.drawn.get().wrapping_mul(0x9E37_79B9_7F4A_7C15);
            z = (z ^ (z >> 31)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            for (to, from) in chunk.iter_mut().zip(z.to_be_bytes()) {
                *to = from;
            }
        }
        true
    }

    fn now_ms(&self) -> Option<i64> {
        Some(T0)
    }

    fn monotonic_ms(&self) -> Option<u64> {
        if self.real_time {
            u64::try_from(self.began.elapsed().as_millis()).ok()
        } else {
            Some(self.monotonic.get())
        }
    }

    fn sleep(&self, millis: u64) -> LocalFuture<'_, ()> {
        if self.real_time {
            Box::pin(tokio::time::sleep(Duration::from_millis(millis)))
        } else if self.expire_after_claims.get().is_some() {
            let until = self.monotonic.get().saturating_add(millis);
            Box::pin(std::future::poll_fn(move |cx| {
                if self.monotonic.get() >= until {
                    std::task::Poll::Ready(())
                } else {
                    *self.timer_waker.borrow_mut() = Some(cx.waker().clone());
                    std::task::Poll::Pending
                }
            }))
        } else {
            Box::pin(std::future::pending())
        }
    }

    fn stopping(&self) -> LocalFuture<'_, ()> {
        Box::pin(std::future::poll_fn(|cx| {
            if self.stopping.get() {
                std::task::Poll::Ready(())
            } else {
                *self.stopping_waker.borrow_mut() = Some(cx.waker().clone());
                std::task::Poll::Pending
            }
        }))
    }

    fn budget_ms(&self, _invocation: Invocation) -> u64 {
        self.budget_ms
    }

    fn defer(&self, work: LocalFuture<'static, ()>) {
        self.deferred.borrow_mut().push(work);
    }

    fn hand_on(&self, work: HandOn) {
        self.handed.borrow_mut().push(work);
    }
}

impl Database for Budgeted {
    fn begin_at(
        &self,
        isolation: Isolation,
    ) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>> {
        Box::pin(async move {
            let inner = self.db.begin_at(isolation).await?;
            Ok(Box::new(Paced {
                inner,
                stall: Rc::clone(&self.stall),
                monotonic: Rc::clone(&self.monotonic),
                step: self.step,
            }) as Box<dyn Transaction>)
        })
    }

    fn query_once<'a>(
        &'a self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
        if sql == jadpo_runtime::outbox::CLAIM
            && let (Some(SqlValue::Timestamp(at)), Some(SqlValue::Timestamp(until))) =
                (params.get(1), params.get(2))
        {
            self.claims
                .borrow_mut()
                .push((at.div_euclid(1000), until.div_euclid(1000)));
        }
        if self
            .stop_after_claims
            .get()
            .is_some_and(|limit| self.claims.borrow().len() >= limit)
        {
            self.stopping.set(true);
            if let Some(waker) = self.stopping_waker.borrow_mut().take() {
                waker.wake();
            }
        }
        if self
            .expire_after_claims
            .get()
            .is_some_and(|limit| self.claims.borrow().len() >= limit)
        {
            self.monotonic.set(self.budget_ms);
            if let Some(waker) = self.timer_waker.borrow_mut().take() {
                waker.wake();
            }
        }
        self.db.query_once(sql, params)
    }
}

/// A transaction whose statements naming `"echo"` never end while `stall`
/// is set, and whose commit moves the test's monotonic clock on `step`.
struct Paced {
    inner: Box<dyn Transaction>,
    stall: Rc<Cell<bool>>,
    monotonic: Rc<Cell<u64>>,
    step: u64,
}

impl Paced {
    fn stalls(&self, sql: &str) -> bool {
        self.stall.get() && sql.contains("\"echo\"")
    }
}

impl Transaction for Paced {
    fn query<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
        if self.stalls(sql) {
            return Box::pin(std::future::pending());
        }
        self.inner.query(sql, params)
    }

    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<u64, StoreError>> {
        if self.stalls(sql) {
            return Box::pin(std::future::pending());
        }
        self.inner.execute(sql, params)
    }

    fn batch<'a>(&'a mut self, sql: &'a str) -> LocalFuture<'a, Result<(), StoreError>> {
        self.inner.batch(sql)
    }

    fn commit(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
        self.monotonic.set(self.monotonic.get() + self.step);
        self.inner.commit()
    }

    fn rollback(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
        self.inner.rollback()
    }
}

struct Loaded {
    _root: PathBuf,
    program: Program,
    _databases: jadpo_test::database::TestDatabases,
    url: String,
}

fn load(name: &str) -> Option<Loaded> {
    let root = project(name);
    if !jadpo_test::live() {
        return None;
    }
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    Some(Loaded {
        _root: root,
        program,
        _databases: databases,
        url,
    })
}

/// The delivery of the note saying `words`: its state, attempts and last
/// failure's code.
async fn delivery(host: &dyn Database, words: &str) -> (String, i32, Option<String>) {
    let sql = format!(
        "SELECT \"d\".\"status\", \"d\".\"attempts\", \"d\".\"last_failure\" ->> 'code' FROM \"jadpo_deliveries\" AS \"d\" JOIN \"jadpo_outbox\" AS \"o\" ON \"o\".\"id\" = \"d\".\"outbox_id\" JOIN \"note\" AS \"n\" ON \"n\".\"id\"::text = \"o\".\"payload\" ->> 'note_id' WHERE \"n\".\"words\" = '{words}'"
    );
    let rows = host.query_once(&sql, &[]).await.expect("the store answers");
    match rows.as_slice() {
        [row] => match row.as_slice() {
            [SqlValue::Text(status), SqlValue::Int4(attempts), code] => (
                status.clone(),
                *attempts,
                match code {
                    SqlValue::Text(code) => Some(code.clone()),
                    _ => None,
                },
            ),
            other => panic!("not a delivery: {other:?}"),
        },
        other => panic!("one delivery of `{words}`: {other:?}"),
    }
}

/// A host whose invocations may run 2 seconds runs a delivery whose
/// action never ends for less than that, then settles it as an attempt
/// that ran out of time, never holding it for the 16 minutes a lease used
/// to last; and, its budget spent, it takes no more work, leaving the next
/// delivery for the sweep.
#[test]
fn a_two_second_budget_ends_a_stalled_delivery_and_takes_nothing_more() {
    let Some(loaded) = load("two-seconds") else {
        return;
    };
    let program = &loaded.program;
    let host = Budgeted::new(&loaded.url, 2_000, true, 0);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let mut work = host
            .call(program, "post_note", vec![Value::text("slow")])
            .await;
        work.extend(
            host.call(program, "post_note", vec![Value::text("quick")])
                .await,
        );
        host.stall.set(true);
        let began = Instant::now();
        let ran = tokio::time::timeout(Duration::from_secs(30), run_deferred(program, &host, work))
            .await
            .expect("the budget ends the invocation");
        let took = began.elapsed();
        assert!(took < Duration::from_secs(10), "it ran {took:?}");
        assert_eq!(ran.len(), 1, "one delivery ran: {ran:?}");
        assert_eq!(
            delivery(&host.db, "slow").await,
            (
                "pending".to_string(),
                1,
                Some("deadline_exceeded".to_string())
            ),
            "an attempt that ran out of time, to be tried again"
        );
        let claims = host.claims.borrow().clone();
        assert_eq!(claims.len(), 1, "{claims:?}");
        let (claimed, until) = claims[0];
        assert!(
            until - claimed <= 2_000 + 60_000,
            "its lease is the budget and a margin: {}",
            until - claimed
        );
        assert_eq!(
            delivery(&host.db, "quick").await,
            ("pending".to_string(), 0, None),
            "never claimed"
        );
        assert_eq!(host.count("SELECT count(*) FROM \"echo\"").await, 0);
    });
}

/// A batch run whose invocation's budget is spent hands itself on: what
/// it committed stands, its lease is given back, and a fresh invocation,
/// with a budget of its own, carries it on from there to its end.
#[test]
fn a_batch_run_carries_on_in_a_second_invocation() {
    let Some(loaded) = load("carries-on") else {
        return;
    };
    let program: Rc<Program> = Rc::new(loaded.program);
    // Ten seconds, and each page's commit takes four.
    let host = Rc::new(Budgeted::new(&loaded.url, 10_000, false, 4_000));
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        for code in ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"] {
            host.call(&program, "add_item", vec![Value::text(code)])
                .await;
        }
        let work = host.call(&program, "touch_all", vec![]).await;
        after_response(
            Rc::clone(&program) as Rc<dyn Executor>,
            Rc::clone(&host) as Rc<dyn Host>,
            work,
        );
        let first = host.deferred.borrow_mut().pop().expect("work for after");
        first.await;
        let touched = "SELECT count(*) FROM \"item\" WHERE \"touches\" = 1";
        let after_first = host.count(touched).await;
        assert!(
            (1..10).contains(&after_first),
            "the first invocation touched {after_first} of 10"
        );
        assert_eq!(
            host.count("SELECT count(*) FROM \"jadpo_batch_runs\" WHERE \"state\" = 'running' AND \"lease_until\" IS NULL")
                .await,
            1,
            "running, its lease given back"
        );
        let mut invocations = 1;
        loop {
            let next = host.handed.borrow_mut().pop();
            let Some(next) = next else {
                break;
            };
            next.run().await;
            invocations += 1;
            assert!(invocations < 10, "it keeps handing itself on");
        }
        assert!(invocations >= 2, "{invocations}");
        assert_eq!(host.count(touched).await, 10, "each item touched once");
        assert_eq!(
            host.count("SELECT count(*) FROM \"jadpo_batch_runs\" WHERE \"state\" = 'finished' AND \"items_done\" = 10")
                .await,
            1
        );
    });
}

/// Work handed on is carried on from its records alone, as a Worker's
/// fresh invocation carries it on (chunk 8.2h): the message names each
/// delivery and batch run by its id and store and holds nothing an
/// application keeps; read back, a delivery still to run is run, one run
/// meanwhile is left out, and the batch run goes on to its end, handing
/// itself on by message in turn. A message that isn't one carries nothing
/// on.
#[test]
fn work_handed_on_is_carried_on_from_its_records() {
    let Some(loaded) = load("carried-by-message") else {
        return;
    };
    let program: Rc<Program> = Rc::new(loaded.program);
    // Ten seconds, and each commit takes four: an invocation runs three
    // pieces.
    let host = Rc::new(Budgeted::new(&loaded.url, 10_000, false, 4_000));
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let mut work = Vec::new();
        for words in ["first", "second", "third", "fourth", "fifth"] {
            work.extend(
                host.call(&program, "post_note", vec![Value::text(words)])
                    .await,
            );
        }
        for code in ["a", "b", "c", "d", "e", "f"] {
            host.call(&program, "add_item", vec![Value::text(code)])
                .await;
        }
        work.extend(host.call(&program, "touch_all", vec![]).await);
        after_response(
            Rc::clone(&program) as Rc<dyn Executor>,
            Rc::clone(&host) as Rc<dyn Host>,
            work,
        );
        let first = host.deferred.borrow_mut().pop().expect("work for after");
        first.await;
        assert_eq!(host.count("SELECT count(*) FROM \"echo\"").await, 3);
        let handed = host.handed.borrow_mut().pop().expect("work handed on");
        assert_eq!(handed.work().len(), 3, "{handed:?}");
        let message = handed.message();
        drop(handed);
        let document: serde_json::Value = serde_json::from_str(&message).expect("JSON");
        let pieces = document["handed_on"].as_array().expect("pieces");
        assert_eq!(pieces.len(), 3, "{message}");
        for piece in &pieces[..2] {
            assert_eq!(piece["store"], "main", "{message}");
            assert_eq!(piece["delivery"].as_str().map(str::len), Some(32), "{message}");
        }
        assert_eq!(pieces[2]["store"], "main", "{message}");
        assert_eq!(pieces[2]["batch"].as_str().map(str::len), Some(32), "{message}");
        assert!(
            !message.contains("fourth") && !message.contains("note_id"),
            "{message}"
        );

        // The fifth note's delivery is run meanwhile, as the tick's sweep
        // might run it.
        host.rows(
            "UPDATE \"jadpo_deliveries\" SET \"status\" = 'succeeded' WHERE \"id\" IN (SELECT \"d\".\"id\" FROM \"jadpo_deliveries\" AS \"d\" JOIN \"jadpo_outbox\" AS \"o\" ON \"o\".\"id\" = \"d\".\"outbox_id\" JOIN \"note\" AS \"n\" ON \"n\".\"id\"::text = \"o\".\"payload\" ->> 'note_id' WHERE \"n\".\"words\" = 'fifth') RETURNING 1",
        )
        .await;
        assert!(
            carry_on(
                Rc::clone(&program) as Rc<dyn Executor>,
                Rc::clone(&host) as Rc<dyn Host>,
                "{\"handed_on\":[{\"store\":\"main\",\"delivery\":\"not-an-id\"}]}",
            )
            .is_none(),
            "a message that isn't one"
        );
        let mut message = Some(message);
        let mut invocations = 1;
        while let Some(next) = message.take() {
            carry_on(
                Rc::clone(&program) as Rc<dyn Executor>,
                Rc::clone(&host) as Rc<dyn Host>,
                &next,
            )
            .expect("a message it wrote")
            .await;
            invocations += 1;
            assert!(invocations < 10, "it keeps handing itself on");
            message = host.handed.borrow_mut().pop().map(|handed| handed.message());
        }
        assert!(invocations >= 3, "{invocations}");
        assert_eq!(
            delivery(&host.db, "fourth").await,
            ("succeeded".to_string(), 1, None),
            "read back and run"
        );
        assert_eq!(
            delivery(&host.db, "fifth").await,
            ("succeeded".to_string(), 0, None),
            "left out, run already"
        );
        assert_eq!(host.count("SELECT count(*) FROM \"echo\"").await, 4);
        assert_eq!(
            host.count("SELECT count(*) FROM \"item\" WHERE \"touches\" = 1")
                .await,
            6,
            "each item touched once"
        );
        assert_eq!(
            host.count("SELECT count(*) FROM \"jadpo_batch_runs\" WHERE \"state\" = 'finished' AND \"items_done\" = 6")
                .await,
            1
        );
    });
}

/// Sends one HTTP/1.1 request with a JSON body and reads the status.
fn post(port: u16, path: &str, body: &str) -> u16 {
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).expect("connects");
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("a timeout");
    let request = format!(
        "POST {path} HTTP/1.1\r\nhost: localhost\r\nconnection: close\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).expect("sends");
    let mut answer = String::new();
    stream.read_to_string(&mut answer).expect("an answer");
    answer
        .split(' ')
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or_else(|| panic!("not a response: {answer}"))
}

/// What `jadpo dev --json` has written.
#[derive(Clone, Default)]
struct Written(Arc<std::sync::Mutex<Vec<u8>>>);

impl Write for Written {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("written").extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Written {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().expect("written")).into_owned()
    }
}

/// A native server asked to stop drains: the delivery its background work
/// is running, whose action waits on a table another session holds, is
/// given back as it was, `pending` with its attempt not counted, rather
/// than left `running` for its lease.
#[test]
fn stopping_a_native_server_gives_back_the_delivery_it_was_running() {
    let root = project("native-stop");
    if !jadpo_test::live() {
        return;
    }
    let server = jadpo_test::database::server().expect("a PostgreSQL server");
    let url = server.create_database("budget_6_7").expect("a database");
    let out = Written::default();
    let stop = Arc::new(AtomicBool::new(false));
    let args = jadpo_cli::dev::DevArgs {
        cwd: root.clone(),
        port: 0,
        url: Some(url.clone()),
        no_run: false,
        json: true,
        choice: jadpo_term::ColorChoice::Never,
        fake: Vec::new(),
        sandbox: Vec::new(),
    };
    let session = {
        let mut out = out.clone();
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut err = Written::default();
            jadpo_cli::dev::serve_to(&args, &Environment::empty(), &mut out, &mut err, &|| {
                stop.load(Ordering::SeqCst)
            })
        })
    };
    let started = Instant::now();
    let port: u16 = loop {
        let text = out.text();
        if let Some(line) = text.lines().find(|line| line.contains("\"listening\"")) {
            let document: serde_json::Value = serde_json::from_str(line).expect("json");
            let url = document["data"]["url"].as_str().expect("a url").to_string();
            break url
                .rsplit(':')
                .next()
                .and_then(|port| port.parse().ok())
                .expect("a port");
        }
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "jadpo dev didn't start:\n{text}"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    let runtime = Runtime::new().expect("runtime");
    let store = postgres(&url).expect("store");
    // Another session holds the echoes' table, so the subscriber's action
    // waits on it.
    let mut held = runtime
        .block_on(store.begin_at(Isolation::ReadCommitted))
        .expect("a transaction");
    runtime
        .block_on(held.batch("LOCK TABLE \"echo\" IN ACCESS EXCLUSIVE MODE"))
        .expect("the lock");
    assert_eq!(post(port, "/notes", r#"{"words": "held"}"#), 201);
    let waited = Instant::now();
    loop {
        let (status, ..) = runtime.block_on(delivery(&store, "held"));
        if status == "running" {
            break;
        }
        assert!(
            waited.elapsed() < Duration::from_secs(20),
            "the delivery wasn't claimed: {status}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    stop.store(true, Ordering::SeqCst);
    let stopping = Instant::now();
    while !session.is_finished() {
        assert!(
            stopping.elapsed() < Duration::from_secs(30),
            "jadpo dev didn't stop"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(session.join().expect("the session"), 0);
    assert_eq!(
        runtime.block_on(delivery(&store, "held")),
        ("pending".to_string(), 0, None),
        "given back as it was"
    );
    runtime.block_on(held.rollback()).expect("the lock let go");
}

/// `jadpo dev` stops cleanly on SIGTERM, as on ctrl-c: it drains what it
/// runs and exits 0, rather than being killed by the signal.
#[cfg(unix)]
#[test]
fn jadpo_dev_stops_cleanly_on_sigterm() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("background-budget-6.7-sigterm");
    let _ = fs::remove_dir_all(&root);
    write(&root, "app.jadpo", "application {}\n");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_jadpo"))
        .args(["dev", "--no-run"])
        .current_dir(&root)
        .env_clear()
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("jadpo dev starts");
    // Its first line is written once it watches for the signal.
    let stdout = child.stdout.take().expect("its output");
    let (said, heard) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(stdout).lines() {
            if said.send(line.unwrap_or_default()).is_err() {
                break;
            }
        }
    });
    heard
        .recv_timeout(Duration::from_secs(30))
        .expect("jadpo dev writes its header");
    let killed = std::process::Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("kill runs");
    assert!(killed.success());
    let waited = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("its status") {
            break status;
        }
        if waited.elapsed() > Duration::from_secs(20) {
            let _ = child.kill();
            panic!("jadpo dev didn't stop on SIGTERM");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(status.code(), Some(0), "{status:?}");
}

#[test]
fn a_sweep_stops_admission_and_settles_all_four_inflight_claims_on_shutdown() {
    let Some(loaded) = load("sweep-shutdown") else {
        return;
    };
    let program = &loaded.program;
    let host = Budgeted::new(&loaded.url, 2_000, true, 0);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        for index in 0..8 {
            drop(
                host.call(
                    program,
                    "post_note",
                    vec![Value::text(&format!("note-{index}"))],
                )
                .await,
            );
        }
        host.stall.set(true);
        host.stop_after_claims.set(Some(4));
        tokio::time::timeout(
            Duration::from_secs(10),
            jadpo_runtime::runner::sweep(program, &host, "main", T0),
        )
        .await
        .expect("shutdown settles")
        .expect("sweep");
        assert_eq!(host.claims.borrow().len(), 4, "no admission after shutdown");
        assert_eq!(
            host.count(
                "SELECT count(*) FROM jadpo_deliveries WHERE status='pending' AND attempts=0"
            )
            .await,
            8
        );
        assert_eq!(host.count("SELECT count(*) FROM echo").await, 0);
        host.stop_after_claims.set(None);
        host.stopping.set(false);
        host.stall.set(false);
        jadpo_runtime::runner::sweep(program, &host, "main", T0)
            .await
            .expect("fresh tick recovers");
        assert_eq!(host.count("SELECT count(*) FROM echo").await, 8);
        assert_eq!(
            host.count("SELECT count(*) FROM jadpo_handled_deliveries")
                .await,
            8
        );
    });
}

#[test]
fn a_sweep_budget_settles_four_timed_out_attempts_and_leaves_the_other_four_unclaimed() {
    let Some(loaded) = load("sweep-budget") else {
        return;
    };
    let program = &loaded.program;
    let host = Budgeted::new(&loaded.url, 2_000, false, 0);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        for index in 0..8 {
            drop(host.call(program, "post_note", vec![Value::text(&format!("note-{index}"))]).await);
        }
        host.stall.set(true);
        host.expire_after_claims.set(Some(4));
        tokio::time::timeout(Duration::from_secs(10), jadpo_runtime::runner::sweep(program, &host, "main", T0)).await.expect("budget settles").expect("sweep");
        assert_eq!(host.claims.borrow().len(), 4, "no new admission once budget spent");
        assert_eq!(host.count("SELECT count(*) FROM jadpo_deliveries WHERE status='pending' AND attempts=1 AND last_failure->>'code'='deadline_exceeded'").await, 4);
        assert_eq!(host.count("SELECT count(*) FROM jadpo_deliveries WHERE status='pending' AND attempts=0").await, 4);
        assert_eq!(host.count("SELECT count(*) FROM echo").await, 0);
    });
}
