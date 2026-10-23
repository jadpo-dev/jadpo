//! Deliveries held to exactly-once and to their limits, against live
//! PostgreSQL (area 12, "Delivery: at least once, safe to repeat" and
//! "Retries and giving up"; chunk 6.6a, from the Phase 6 soundness
//! review): a subscriber whose action only emits keeps a handled record,
//! so a repeat sets nothing off twice; the sweep runs each delivery at
//! the host's time, leasing it from when it was claimed; a delivery
//! claimed again over its `retry:` budget or past `within:` fails without
//! running, and one stopped by the 15-minute limit of a run is settled
//! against its budget; and a route's `within:` races a subscriber's body,
//! never its commit.
//!
//! A notice board: posting a note emits `NotePosted`, which `relay_note`
//! relays as `NoteRelayed` without writing anything, and `echo_note`
//! records an echo of.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::cell::Cell;
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use jadpo_cli::{Output, run};
use jadpo_host_native::{Postgres, Runtime, postgres};
use jadpo_interp::Program;
use jadpo_runtime::runner::{Deferred, RUN_LIMIT_MS, Settled, run_deferred, settle_within, sweep};
use jadpo_runtime::{
    Ctx, Database, Executor, Isolation, LocalFuture, Outcome, Places, Plan, Row, SqlValue,
    StoreError, Transaction,
};
use jadpo_term::Environment;
use jadpo_test::host::{Harness, Timer};
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
        "domains/board/events/note_relayed.jadpo",
        r#"use board/entities { Note }

/// A posted note was passed on.
type NoteRelayed = Event {
    note_id: Note.id
}
"#,
    ),
    (
        "domains/board/actions/post_note.jadpo",
        r#"use board/entities { Note }
use board/events { NotePosted }
use board/types { Words }

action post_note(words: Words) -> Note {
    var note: Note = Note.create {
        set: {
            words: words
        }
    }
    emit NotePosted {
        note_id: note.id
    }
    return note
}
"#,
    ),
    (
        "domains/board/actions/relay.jadpo",
        r#"use board/entities { Note }
use board/events { NoteRelayed }

action relay(note_id: Note.id) -> Unit {
    emit NoteRelayed {
        note_id: note_id
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
        "domains/board/subscribers/relay_note.jadpo",
        r#"use board/actions { relay }
use board/events { NotePosted }

/// Passes each posted note on, writing nothing itself.
subscriber relay_note {
    on: NotePosted
    run: relay(event.note_id)
    retry: {
        attempts: 2
        within: 1h
    }
}
"#,
    ),
    (
        "domains/board/subscribers/echo_note.jadpo",
        r#"use board/actions { record_echo }
use board/events { NoteRelayed }

/// Records an echo of each note passed on.
subscriber echo_note {
    on: NoteRelayed
    run: record_echo(event.note_id)
}
"#,
    ),
];

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("delivery-limits-6.6a-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let recorded = jadpo(&root, &["records", "update"]);
    assert_eq!(recorded.code, 0, "{}{}", recorded.stdout, recorded.stderr);
    root
}

/// The store, its transactions noting when a commit begins: what the
/// test's harness runs on, with a clock the test sets, which moves on each
/// time the host reads it when the test says so, as a sweep's work takes
/// time; random bytes from a counter; and the 15-minute limit of a run,
/// which passes at once when the test says so.
struct Noting {
    db: Postgres,
    committing: Rc<Cell<bool>>,
}

impl Database for Noting {
    fn begin_at(
        &self,
        isolation: Isolation,
    ) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>> {
        let committing = Rc::clone(&self.committing);
        Box::pin(async move {
            let inner = self.db.begin_at(isolation).await?;
            Ok(Box::new(Noted { inner, committing }) as Box<dyn Transaction>)
        })
    }

    fn query_once<'a>(
        &'a self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
        self.db.query_once(sql, params)
    }
}

/// A transaction that notes when its commit begins.
struct Noted {
    inner: Box<dyn Transaction>,
    committing: Rc<Cell<bool>>,
}

impl Transaction for Noted {
    fn query<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
        self.inner.query(sql, params)
    }

    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<u64, StoreError>> {
        self.inner.execute(sql, params)
    }

    fn batch<'a>(&'a mut self, sql: &'a str) -> LocalFuture<'a, Result<(), StoreError>> {
        self.inner.batch(sql)
    }

    fn commit(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
        self.committing.set(true);
        self.inner.commit()
    }

    fn rollback(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
        self.inner.rollback()
    }
}

/// When the test starts: 2026-10-19T09:00:00Z.
const T0: i64 = 1_792_400_400_000;

/// How many events of type `event` the outbox keeps.
async fn events(host: &Harness<Noting>, event: &str) -> i64 {
    host.count(&format!(
        "SELECT count(*) FROM \"jadpo_outbox\" WHERE \"event\" LIKE '%{event}'"
    ))
    .await
}

/// Each delivery to `subscriber` of the note with `words`: its state,
/// attempts and last failure's code.
async fn deliveries(
    host: &Harness<Noting>,
    subscriber: &str,
    words: &str,
) -> Vec<(String, i32, Option<String>)> {
    let sql = format!(
        "SELECT \"d\".\"status\", \"d\".\"attempts\", \"d\".\"last_failure\" ->> 'code' FROM \"jadpo_deliveries\" AS \"d\" JOIN \"jadpo_outbox\" AS \"o\" ON \"o\".\"id\" = \"d\".\"outbox_id\" JOIN \"note\" AS \"n\" ON \"n\".\"id\"::text = \"o\".\"payload\" ->> 'note_id' WHERE \"d\".\"subscriber\" LIKE '%{subscriber}' AND \"n\".\"words\" = '{words}' ORDER BY \"o\".\"emitted_at\", \"o\".\"id\""
    );
    host.rows(&sql)
        .await
        .into_iter()
        .map(|row| match row.as_slice() {
            [SqlValue::Text(status), SqlValue::Int4(attempts), code] => (
                status.clone(),
                *attempts,
                match code {
                    SqlValue::Text(code) => Some(code.clone()),
                    _ => None,
                },
            ),
            other => panic!("not a delivery: {other:?}"),
        })
        .collect()
}

/// Sets the delivery to `subscriber` of the note with `words` as `set`
/// says, as a crash or a dropped run would have left it.
async fn leave(host: &Harness<Noting>, subscriber: &str, words: &str, set: &str) {
    host.rows(&format!(
        "UPDATE \"jadpo_deliveries\" AS \"d\" SET {set} FROM \"jadpo_outbox\" AS \"o\", \"note\" AS \"n\" WHERE \"o\".\"id\" = \"d\".\"outbox_id\" AND \"n\".\"id\"::text = \"o\".\"payload\" ->> 'note_id' AND \"d\".\"subscriber\" LIKE '%{subscriber}' AND \"n\".\"words\" = '{words}' RETURNING \"d\".\"id\""
    ))
    .await;
}

/// Posts a note saying `words` at the clock's time, giving the work it
/// left for after its response.
async fn post(host: &Harness<Noting>, program: &Program, words: &str) -> Vec<Deferred> {
    let id = program.plan().find("post_note").expect("the action");
    let mut ctx = host.ctx(program);
    match program.run(&mut ctx, id, vec![Value::text(words)]).await {
        Outcome::Value(_) => {}
        other => panic!("the note is posted: {other:?}"),
    }
    ctx.take_deferred()
}

/// A timestamp as its SQL literal, `ms` after [`T0`].
fn at(ms: i64) -> String {
    let seconds = (T0 + ms) / 1000;
    format!("to_timestamp({seconds})")
}

struct Loaded {
    _root: PathBuf,
    program: Program,
    _databases: jadpo_test::database::TestDatabases,
    url: String,
}

fn load(name: &str) -> Option<Loaded> {
    let root = project(name);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
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

fn host(url: &str) -> Harness<Noting> {
    Harness::new(Noting {
        db: postgres(url).expect("store"),
        committing: Rc::new(Cell::new(false)),
    })
    .at(T0)
    .random_from(0)
}

fn succeeded(attempts: i32) -> Vec<(String, i32, Option<String>)> {
    vec![("succeeded".to_string(), attempts, None)]
}

fn failed(attempts: i32, code: &str) -> Vec<(String, i32, Option<String>)> {
    vec![("failed".to_string(), attempts, Some(code.to_string()))]
}

/// A subscriber whose action emits and writes nothing keeps its handled
/// record in the store its events are kept in, after its outbox rows, so
/// a repeated delivery finds it and sets nothing off a second time.
#[test]
fn an_emit_only_subscriber_keeps_a_handled_record_so_a_repeat_sets_nothing_off_twice() {
    let Some(loaded) = load("emit-only") else {
        return;
    };
    let (program, host) = (&loaded.program, host(&loaded.url));
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let work = post(&host, program, "hello").await;
        run_deferred(program, &host, work).await;
        assert_eq!(deliveries(&host, "relay_note", "hello").await, succeeded(1));
        assert_eq!(deliveries(&host, "echo_note", "hello").await, succeeded(1));
        assert_eq!(events(&host, "NoteRelayed").await, 1);
        assert_eq!(
            host.count("SELECT count(*) FROM \"jadpo_handled_deliveries\"")
                .await,
            2,
            "the relay's handled record committed with its outbox rows"
        );

        // The relay's delivery repeated, as after a crash between its
        // commit and its record: its handled record is found, and the note
        // is relayed and echoed once.
        leave(
            &host,
            "relay_note",
            "hello",
            &format!("\"status\" = 'pending', \"next_attempt_at\" = {}", at(0)),
        )
        .await;
        host.set_now(T0 + 1_000);
        let repeated = sweep(program, &host, "main", T0 + 1_000)
            .await
            .expect("the sweep runs");
        assert_eq!(repeated.len(), 1, "{repeated:?}");
        assert_eq!(deliveries(&host, "relay_note", "hello").await, succeeded(2));
        assert_eq!(events(&host, "NoteRelayed").await, 1, "relayed once");
        assert_eq!(host.count("SELECT count(*) FROM \"echo\"").await, 1);
    });
}

/// The sweep runs each delivery, and what it sets off, at the host's time
/// when it starts, not the tick's, and leases it from when it was
/// claimed: a sweep whose work takes time never claims with a lease that
/// has already run out.
#[test]
fn the_sweep_runs_each_delivery_at_the_hosts_time_and_leases_it_from_its_claim() {
    let Some(loaded) = load("host-time") else {
        return;
    };
    let (program, host) = (&loaded.program, host(&loaded.url));
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        // Two notes posted, their work dropped with the process.
        drop(post(&host, program, "first").await);
        drop(post(&host, program, "second").await);
        // The tick a minute on, each reading of the clock five minutes
        // after the last, as long deliveries would take, all within the
        // relays' `within:` of an hour.
        let tick = T0 + 60_000;
        host.set_now(tick);
        host.step(300_000);
        let swept = sweep(program, &host, "main", tick)
            .await
            .expect("the sweep runs");
        assert_eq!(swept.len(), 4, "two relays and their echoes: {swept:?}");
        let claims = host
            .rows("SELECT (extract(epoch FROM \"last_tried_at\") * 1000)::int8 FROM \"jadpo_deliveries\" ORDER BY \"last_tried_at\"")
            .await;
        let claims: Vec<i64> = claims
            .iter()
            .map(|row| match row.as_slice() {
                [SqlValue::Int8(at)] => *at,
                other => panic!("not a claim's time: {other:?}"),
            })
            .collect();
        assert_eq!(claims.len(), 4);
        assert!(claims.iter().all(|claim| *claim >= tick), "{claims:?}");
        let mut distinct = claims.clone();
        distinct.dedup();
        assert_eq!(
            distinct, claims,
            "each delivery claimed at its own time, not the tick's"
        );
        assert!(claims[3] > tick + 300_000, "{claims:?}");
    });
}

/// A delivery a lost worker or a dropped run left, claimed again over its
/// `retry:` budget or past `within:` from its event, fails without
/// running: one whose last attempt was lost keeps `lease_expired`, one
/// never tried in time `deadline_exceeded`. One still within both runs.
#[test]
fn a_delivery_claimed_again_over_its_budget_or_past_within_fails_without_running() {
    let Some(loaded) = load("budget") else {
        return;
    };
    let (program, host) = (&loaded.program, host(&loaded.url));
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        for words in ["spent", "lost", "late", "kept"] {
            drop(post(&host, program, words).await);
        }
        // Its second attempt, the last of its budget, was lost with its
        // worker; and another's first was, with time left.
        let lapsed = format!(
            "\"status\" = 'running', \"attempts\" = 2, \"next_attempt_at\" = {}",
            at(30_000)
        );
        leave(&host, "relay_note", "spent", &lapsed).await;
        let lapsed = format!(
            "\"status\" = 'running', \"attempts\" = 1, \"next_attempt_at\" = {}",
            at(30_000)
        );
        leave(&host, "relay_note", "kept", &lapsed).await;
        // Two not due until after `within:` (an hour) has passed: one
        // whose first attempt was lost, one never tried.
        let lapsed = format!(
            "\"status\" = 'running', \"attempts\" = 1, \"next_attempt_at\" = {}",
            at(5_400_000)
        );
        leave(&host, "relay_note", "lost", &lapsed).await;
        let waiting = format!("\"next_attempt_at\" = {}", at(5_400_000));
        leave(&host, "relay_note", "late", &waiting).await;

        host.set_now(T0 + 60_000);
        sweep(program, &host, "main", T0 + 60_000)
            .await
            .expect("the sweep runs");
        assert_eq!(
            deliveries(&host, "relay_note", "spent").await,
            failed(2, "lease_expired"),
            "over its budget: failed, its lost attempt the last"
        );
        assert_eq!(
            deliveries(&host, "relay_note", "kept").await,
            succeeded(2),
            "within its budget, a lost attempt is tried again"
        );

        host.set_now(T0 + 7_200_000);
        sweep(program, &host, "main", T0 + 7_200_000)
            .await
            .expect("the sweep runs");
        assert_eq!(
            deliveries(&host, "relay_note", "lost").await,
            failed(1, "lease_expired")
        );
        assert_eq!(
            deliveries(&host, "relay_note", "late").await,
            failed(0, "deadline_exceeded")
        );
        assert_eq!(
            events(&host, "NoteRelayed").await,
            1,
            "only the delivery within both ran"
        );
    });
}

/// A delivery stopped by the 15-minute limit of a run is an attempt that
/// ran out of time: settled at once against its budget, nothing it did
/// kept, rather than held until its lease runs out.
#[test]
fn a_delivery_stopped_by_the_run_limit_is_settled_against_its_budget() {
    let Some(loaded) = load("run-limit") else {
        return;
    };
    let (program, host) = (&loaded.program, host(&loaded.url));
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let work = post(&host, program, "slow").await;
        host.set_timer(RUN_LIMIT_MS, Timer::Now);
        let ran = run_deferred(program, &host, work).await;
        assert_eq!(ran.len(), 1, "{ran:?}");
        assert_eq!(
            deliveries(&host, "relay_note", "slow").await,
            [(
                "pending".to_string(),
                1,
                Some("deadline_exceeded".to_string())
            )]
        );
        let wait = host
            .count(&format!(
                "SELECT (extract(epoch FROM \"next_attempt_at\") * 1000)::int8 - {T0} FROM \"jadpo_deliveries\""
            ))
            .await;
        assert!((1_000..=30_000).contains(&wait), "the first wait: {wait}");
        assert_eq!(events(&host, "NoteRelayed").await, 0, "nothing it did kept");

        // Its second attempt, the last of its budget, runs out of time too.
        host.set_now(T0 + 31_000);
        sweep(program, &host, "main", T0 + 31_000)
            .await
            .expect("the sweep runs");
        assert_eq!(
            deliveries(&host, "relay_note", "slow").await,
            failed(2, "deadline_exceeded")
        );
        assert_eq!(events(&host, "NoteRelayed").await, 0);
    });
}

/// A route's `within:` races a subscriber's body, never its commit: once
/// the body has its value, the commit runs outside the race, so a delivery
/// whose work committed is never given back as though it hadn't run.
#[test]
fn a_routes_within_races_a_subscribers_body_never_its_commit() {
    let Some(loaded) = load("within") else {
        return;
    };
    let (program, host) = (&loaded.program, host(&loaded.url));
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let work = post(&host, program, "quick").await;
        // `within:` passes the moment a commit begins.
        host.store().committing.set(false);
        let committing = Rc::clone(&host.store().committing);
        let mut within: LocalFuture<'_, ()> = Box::pin(std::future::poll_fn(move |_| {
            if committing.get() {
                std::task::Poll::Ready(())
            } else {
                std::task::Poll::Pending
            }
        }));
        let mut ctx = host.ctx(program);
        let settled = settle_within(program, &mut ctx, work, &mut within).await;
        assert_eq!(
            deliveries(&host, "relay_note", "quick").await,
            succeeded(1),
            "its work committed, so it is recorded as done"
        );
        assert_eq!(events(&host, "NoteRelayed").await, 1);
        // `within:` had passed by the next piece: the echo is left for
        // after the response.
        assert_eq!(settled, Settled::Unsettled);
        assert_eq!(
            deliveries(&host, "echo_note", "quick").await,
            [("pending".to_string(), 0, None)]
        );
        assert_eq!(ctx.take_deferred().len(), 1);
    });
}

/// Hold four real subscriber bodies until all are admitted. The timeout
/// only detects deadlock; counters, not wall time, establish the bound.
struct Overlapping<'a> {
    program: &'a Program,
    active: Cell<usize>,
    maximum: Cell<usize>,
    released: Cell<bool>,
}
impl Executor for Overlapping<'_> {
    fn places(&self) -> &Places {
        self.program.places()
    }
    fn plan(&self) -> Plan<'_> {
        Executor::plan(self.program)
    }
    fn run<'a, 'p: 'a>(
        &'a self,
        ctx: &'a mut Ctx<'p>,
        callable: u32,
        args: Vec<Value>,
    ) -> LocalFuture<'a, Outcome> {
        Box::pin(async move {
            struct Active<'a>(&'a Cell<usize>);
            impl Drop for Active<'_> {
                fn drop(&mut self) {
                    self.0.set(self.0.get() - 1);
                }
            }
            self.active.set(self.active.get() + 1);
            let _active = Active(&self.active);
            self.maximum.set(self.maximum.get().max(self.active.get()));
            if self.active.get() >= 4 {
                self.released.set(true);
            }
            std::future::poll_fn(|cx| {
                if self.released.get() {
                    std::task::Poll::Ready(())
                } else {
                    cx.waker().wake_by_ref();
                    std::task::Poll::Pending
                }
            })
            .await;
            tokio::task::yield_now().await;
            self.program.run(ctx, callable, args).await
        })
    }
}

#[test]
fn sweeps_overlap_four_bodies_keep_the_hundred_limit_and_commit_each_effect_once() {
    let Some(loaded) = load("bounded-sweeps") else {
        return;
    };
    let (program, host) = (&loaded.program, host(&loaded.url));
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        for index in 0..120 {
            drop(post(&host, program, &format!("note-{index}")).await);
        }
        let executor = Overlapping {
            program,
            active: Cell::new(0),
            maximum: Cell::new(0),
            released: Cell::new(false),
        };
        let first = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            sweep(&executor, &host, "main", T0),
        )
        .await
        .expect("four bodies must overlap")
        .expect("sweep");
        assert_eq!(
            executor.maximum.get(),
            4,
            "exactly four lanes, including descendants"
        );
        assert_eq!(first.len(), 200, "100 selected relays and their 100 echoes");
        assert!(
            first
                .iter()
                .all(|outcome| matches!(outcome, Outcome::Value(_)))
        );
        assert_eq!(host.count("SELECT count(*) FROM echo").await, 100);
        assert_eq!(
            host.count("SELECT count(*) FROM jadpo_deliveries WHERE status='pending'")
                .await,
            20
        );
        let mut left = Box::pin(sweep(&executor, &host, "main", T0));
        let mut right = Box::pin(sweep(&executor, &host, "main", T0));
        let (mut a, mut b) = (None, None);
        std::future::poll_fn(|cx| {
            if a.is_none()
                && let std::task::Poll::Ready(out) = left.as_mut().poll(cx)
            {
                a = Some(out);
            }
            if b.is_none()
                && let std::task::Poll::Ready(out) = right.as_mut().poll(cx)
            {
                b = Some(out);
            }
            if a.is_some() && b.is_some() {
                std::task::Poll::Ready(())
            } else {
                std::task::Poll::Pending
            }
        })
        .await;
        let (left, right) = (a.expect("first sweep"), b.expect("second sweep"));
        assert!(left.is_ok() && right.is_ok());
        assert!(executor.maximum.get() <= 8, "four lanes per invocation");
        assert_eq!(host.count("SELECT count(*) FROM echo").await, 120);
        assert_eq!(
            host.count("SELECT count(DISTINCT note_id) FROM echo").await,
            120
        );
        assert_eq!(
            host.count("SELECT count(*) FROM jadpo_handled_deliveries")
                .await,
            240
        );
        assert_eq!(
            host.count("SELECT count(*) FROM jadpo_deliveries WHERE status <> 'succeeded'")
                .await,
            0
        );
        assert!(
            sweep(&executor, &host, "main", T0)
                .await
                .expect("empty sweep")
                .is_empty()
        );
    });
}

#[test]
fn a_spent_sweep_leaves_unstarted_deliveries_durable_for_the_next_tick() {
    let Some(loaded) = load("spent-sweep") else {
        return;
    };
    let program = &loaded.program;
    let host = host(&loaded.url).budget(0);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        drop(post(&host, program, "durable").await);
        assert!(
            sweep(program, &host, "main", T0)
                .await
                .expect("spent sweep")
                .is_empty()
        );
        assert_eq!(
            host.count(
                "SELECT count(*) FROM jadpo_deliveries WHERE status='pending' AND attempts=0"
            )
            .await,
            1
        );
        let fresh = self::host(&loaded.url);
        assert_eq!(
            sweep(program, &fresh, "main", T0)
                .await
                .expect("fresh sweep")
                .len(),
            2
        );
        assert_eq!(fresh.count("SELECT count(*) FROM echo").await, 1);
    });
}
