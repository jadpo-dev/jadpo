//! Batch runs held to the limits of background work, against live
//! PostgreSQL (area 12, "Batches: work over many items" and "Limits and
//! runtime faults"; chunk 6.6a, from the Phase 6 soundness review): a
//! run keeps the depth in a chain of events it was started at, so an
//! event loop through a batch stops at 16 as any loop does; and the
//! minute tick deletes ended runs, with their problems and cancels, 30
//! days after they ended.
//!
//! A shelf of items: touching one counts it and emits `Touched`, and
//! `touch_again` starts a run touching every item for each, a loop only
//! the chain's limit stops.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::cell::Cell;
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_host_native::{Postgres, Runtime, postgres};
use jadpo_interp::Program;
use jadpo_runtime::runner::{Deferred, deliver};
use jadpo_runtime::{Clock, Ctx, Database, Env, Executor, Host, Outcome, Row, SqlValue};
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
    ("app.jadpo", "application {}\n"),
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
        default: 25
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
    code: Code
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
        "domains/shelf/events/touched.jadpo",
        r#"use shelf/entities { Item }

/// An item was touched.
type Touched = Event {
    item_id: Item.id
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
use shelf/events { Touched }

action touch(item: Item) -> Unit {
    var touched: Item = Item.update(item) {
        set: {
            touches: item.touches + 1
        }
    }
    emit Touched {
        item_id: touched.id
    }
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
    (
        "domains/shelf/subscribers/touch_again.jadpo",
        r#"use shelf/actions { touch_all }
use shelf/events { Touched }

/// Touches every item again for each touch: a loop.
subscriber touch_again {
    on: Touched
    run: touch_all()
}
"#,
    ),
];

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("batch-limits-6.6a-{name}"));
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

/// The store, a clock the test moves, and random bytes from a counter.
struct Store {
    db: Postgres,
    seeds: Cell<u64>,
    clock: Cell<i64>,
    drawn: Cell<u64>,
}

impl Host for Store {
    fn database(&self, _store: &str) -> Option<&dyn Database> {
        Some(&self.db)
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
        Some(self.clock.get())
    }
}

/// When the test starts: 2026-10-19T09:00:00Z.
const T0: i64 = 1_792_400_400_000;

/// A day, in milliseconds.
const DAY: i64 = 24 * 60 * 60 * 1000;

impl Store {
    fn ctx<'p>(&'p self, program: &'p Program) -> Ctx<'p> {
        self.seeds.set(self.seeds.get() + 1);
        let env = Env {
            seed: Some(self.seeds.get()),
            clock: Clock::fixed(self.clock.get()),
        };
        Ctx::new(program.places(), self, env)
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

fn host(url: &str) -> Store {
    Store {
        db: postgres(url).expect("store"),
        seeds: Cell::new(0),
        clock: Cell::new(T0),
        drawn: Cell::new(0),
    }
}

/// Runs `work` and what it sets off, one piece at a time, as deferred work
/// runs it, at most `limit` pieces: how many it ran, and what was left
/// when it stopped.
async fn drive(
    program: &Program,
    host: &Store,
    work: Vec<Deferred>,
    limit: usize,
) -> (usize, usize) {
    let mut queue: VecDeque<Deferred> = work.into();
    let mut ran = 0;
    while ran < limit {
        let Some(next) = queue.pop_front() else {
            break;
        };
        ran += 1;
        match next {
            Deferred::Batch(start) => {
                let (_, more) = jadpo_runtime::batches::run(program, host, &start).await;
                queue.extend(more);
            }
            Deferred::Deliveries(deliveries) => {
                for delivery in deliveries {
                    let mut ctx = host.ctx(program);
                    deliver(program, &mut ctx, delivery).await;
                    queue.extend(ctx.take_deferred());
                }
            }
            Deferred::Run(job) => panic!("no job is left: {job:?}"),
        }
    }
    (ran, queue.len())
}

/// Starts the loop on a shelf of one item and runs it until it stops.
async fn loop_until_it_stops(program: &Program, host: &Store) {
    host.call(program, "add_item", vec![Value::text("a")]).await;
    let work = host.call(program, "touch_all", vec![]).await;
    let (ran, left) = drive(program, host, work, 200).await;
    assert_eq!(left, 0, "the loop stopped, after {ran} pieces");
}

/// An event loop through a batch stops at 16, as any chain of events
/// does: each run keeps the depth it was started at, so what its items
/// emit is one deeper than the event that started it.
#[test]
fn an_event_loop_through_a_batch_stops_at_the_chains_limit() {
    let Some(loaded) = load("chain") else {
        return;
    };
    let (program, host) = (&loaded.program, host(&loaded.url));
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        loop_until_it_stops(program, &host).await;
        let depths: Vec<i32> = host
            .rows("SELECT \"depth\" FROM \"jadpo_outbox\" ORDER BY \"depth\"")
            .await
            .iter()
            .map(|row| match row.as_slice() {
                [SqlValue::Int4(depth)] => *depth,
                other => panic!("not a depth: {other:?}"),
            })
            .collect();
        assert_eq!(depths, (0..=16).collect::<Vec<i32>>());
        assert_eq!(
            host.count("SELECT count(*) FROM \"jadpo_batch_runs\" WHERE \"state\" = 'finished'")
                .await,
            17
        );
        assert_eq!(
            host.count("SELECT max(\"touches\") FROM \"item\"").await,
            17
        );
        assert_eq!(
            host.count("SELECT count(*) FROM \"jadpo_deliveries\" WHERE \"status\" = 'failed' AND \"last_failure\" ->> 'code' = 'event_chain_limit'")
                .await,
            1,
            "the event 16 deep is the one refused"
        );
    });
}

/// The minute tick deletes a run 30 days after it ended, with its problems
/// and cancels, and a cancel left with no run once as old; a run still
/// going, or ended since, stays.
#[test]
fn the_tick_deletes_ended_runs_with_their_problems_and_cancels_after_30_days() {
    let Some(loaded) = load("clean") else {
        return;
    };
    let (program, host) = (&loaded.program, host(&loaded.url));
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        loop_until_it_stops(program, &host).await;
        // Each run a problem and a cancel; all but the newest ended 31
        // days ago, and one of those is still going, as a held lease
        // would have it.
        host.rows("INSERT INTO \"jadpo_batch_problems\" (\"run_id\", \"row\", \"column\", \"rule\") SELECT \"id\", 1, '', 'gone' FROM \"jadpo_batch_runs\" RETURNING 1")
            .await;
        host.rows("INSERT INTO \"jadpo_batch_cancels\" (\"run_id\", \"requested_at\") SELECT \"id\", now() FROM \"jadpo_batch_runs\" RETURNING 1")
            .await;
        let month_ago = (T0 - 31 * DAY) / 1000;
        host.rows(&format!(
            "UPDATE \"jadpo_batch_runs\" SET \"finished_at\" = to_timestamp({month_ago}) WHERE \"id\" <> (SELECT \"id\" FROM \"jadpo_batch_runs\" ORDER BY \"started_at\" DESC, \"id\" DESC LIMIT 1) RETURNING 1"
        ))
        .await;
        host.rows(&format!(
            "UPDATE \"jadpo_batch_runs\" SET \"state\" = 'running', \"lease_until\" = to_timestamp({}) WHERE \"id\" = (SELECT \"id\" FROM \"jadpo_batch_runs\" ORDER BY \"started_at\", \"id\" LIMIT 1) RETURNING 1",
            (T0 + DAY) / 1000
        ))
        .await;
        // Two cancels of runs no store keeps: one asked for 31 days ago,
        // one just now.
        host.rows(&format!(
            "INSERT INTO \"jadpo_batch_cancels\" (\"run_id\", \"requested_at\") VALUES (gen_random_uuid(), to_timestamp({month_ago})), (gen_random_uuid(), to_timestamp({})) RETURNING 1",
            T0 / 1000
        ))
        .await;

        jadpo_runtime::schedules::tick(program, &host, T0).await;
        assert_eq!(
            host.rows("SELECT \"state\" FROM \"jadpo_batch_runs\" ORDER BY \"state\"")
                .await,
            [
                vec![SqlValue::Text("finished".into())],
                vec![SqlValue::Text("running".into())],
            ],
            "the run ended since, and the one still going"
        );
        assert_eq!(
            host.count("SELECT count(*) FROM \"jadpo_batch_problems\"").await,
            2
        );
        assert_eq!(
            host.count("SELECT count(*) FROM \"jadpo_batch_cancels\"").await,
            3,
            "the kept runs' cancels, and the one asked for just now"
        );
    });
}
