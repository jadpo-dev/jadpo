//! Batches against live PostgreSQL (area 12, "Batches: work over many
//! items"; chunk 6.4): `batches.each` started in an action, its run walking
//! a named query's pages after the action commits, each page one
//! transaction committing its items' writes and the run's position
//! together; an item whose action fails undone alone and kept as a
//! problem, never its value, while the rest carry on; a crash mid-batch
//! resumed from the last committed page with no item done twice and none
//! skipped; `key:` giving a second start the same run; `batches.cancel`
//! stopping it after the batch in hand; and `batches.status` reporting
//! progress, problems and, from chunk 6.4b3, the total its query's count
//! estimates. A product's code is `immutable: true`, so the walk in code
//! order meets each product once (chunk 6.4b3).
//!
//! A stock check: each product is counted once per check, and one with
//! none in stock fails `OutOfStock` after it was counted, so a problem's
//! count is undone with it. Pages of three over twenty-one products are
//! seven pages; over sixty, twenty, past the sixteen-deep event chain a
//! page per event would stop at.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use jadpo_cli::{Output, run};
use jadpo_host_native::{Postgres, Runtime, postgres};
use jadpo_interp::Program;
use jadpo_runtime::batches::{BatchStart, Ran};
use jadpo_runtime::runner::{Deferred, run_deferred};
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
    ("app.jadpo", "application {}\n"),
    (
        "domains/inventory/types/shelf_code.jadpo",
        r#"type ShelfCode = Text {
    min_length: 1
    max_length: 40
}
"#,
    ),
    (
        "domains/inventory/types/stock_count.jadpo",
        r#"type StockCount = Int {
    min: 0
    max: 1000000
}
"#,
    ),
    (
        "domains/inventory/types/check_page.jadpo",
        r#"type CheckPage = Input {
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
        "domains/inventory/entities/product.jadpo",
        r#"use inventory/types { ShelfCode, StockCount }

entity Product {
    id: Uuid
    code: ShelfCode {
        unique: true
        immutable: true
    }
    stock: StockCount
    checks: Int

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.system]
    }
}
"#,
    ),
    (
        "domains/inventory/entities/stock_check.jadpo",
        r#"entity StockCheck {
    id: Uuid
    run: BatchRun?
    checked: Int?
    problems: Int?
    listed: Int?

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.public, Access.system]
    }
}
"#,
    ),
    (
        "domains/inventory/failures/code_taken.jadpo",
        r#"type CodeTaken = Failure {
    kind: FailureKind.conflict
    message: "Another product has that code."
}
"#,
    ),
    (
        "domains/inventory/failures/out_of_stock.jadpo",
        r#"type OutOfStock = Failure {
    kind: FailureKind.rejected
    message: "That product has none in stock."
}
"#,
    ),
    (
        "domains/inventory/queries/products_to_check.jadpo",
        r#"use inventory/entities { Product }
use inventory/types { CheckPage }

query products_to_check(page: CheckPage) -> Page<Product> {
    from: Product
    order: [Product.code asc]
    page: {
        size: page.page_size
        after: page.after
    }
}
"#,
    ),
    (
        "domains/inventory/actions/add_product.jadpo",
        r#"use inventory/entities { Product }
use inventory/failures { CodeTaken }
use inventory/types { ShelfCode, StockCount }

action add_product(code: ShelfCode, count: StockCount) fails CodeTaken -> Product {
    return attempt Product.create {
        set: {
            code: code
            stock: count
            checks: 0
        }
        conflict: {
            Product.code: CodeTaken
        }
    }
}
"#,
    ),
    (
        "domains/inventory/actions/check_product.jadpo",
        r#"use inventory/entities { Product, ProductNotFound }
use inventory/failures { OutOfStock }

action check_product(product: Product) fails OutOfStock, ProductNotFound -> Unit {
    var counted: Product = attempt Product.update(product.id) {
        set: {
            checks: Product.checks + 1
        }
    }
    if (counted.stock == 0) {
        reject OutOfStock {}
    }
}
"#,
    ),
    (
        "domains/inventory/actions/check_done.jadpo",
        r#"use inventory/entities { StockCheck, StockCheckNotFound }

action check_done(summary: ImportSummary, check_id: StockCheck.id) fails StockCheckNotFound -> Unit {
    attempt StockCheck.update(check_id) {
        set: {
            checked: summary.items
            problems: summary.items_with_problems
            listed: list.count(summary.problems)
        }
    }
}
"#,
    ),
    (
        "domains/inventory/actions/new_check.jadpo",
        r#"use inventory/entities { StockCheck }

action new_check() -> StockCheck {
    return StockCheck.create {
        set: {
            run: none
            checked: none
            problems: none
            listed: none
        }
    }
}
"#,
    ),
    (
        "domains/inventory/actions/check_stock.jadpo",
        r#"use inventory/actions { check_done, check_product }
use inventory/entities { StockCheck, StockCheckNotFound }
use inventory/queries { products_to_check }
use inventory/types { CheckPage }

action check_stock(check_id: StockCheck.id, page: CheckPage) fails StockCheckNotFound -> StockCheck {
    var check: StockCheck = attempt StockCheck.get(check_id)
    var run: BatchRun = batches.each(
        from: products_to_check(page),
        each: product -> check_product(product),
        finished: summary -> check_done(summary, check_id),
        key: check_id
    )
    return StockCheck.update(check) {
        set: {
            run: run
        }
    }
}
"#,
    ),
    (
        "domains/inventory/actions/quick_check.jadpo",
        r#"use inventory/actions { check_product }
use inventory/queries { products_to_check }
use inventory/types { CheckPage }

action quick_check(page: CheckPage) -> BatchRun {
    return batches.each(
        from: products_to_check(page),
        each: product -> check_product(product)
    )
}
"#,
    ),
    (
        "domains/inventory/actions/check_progress.jadpo",
        r#"action check_progress(run: BatchRun) -> BatchProgress {
    return batches.status(run)
}
"#,
    ),
    (
        "domains/inventory/actions/stop_check.jadpo",
        r#"action stop_check(run: BatchRun) -> Unit {
    batches.cancel(run)
}
"#,
    ),
];

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("batches-6.4-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    root
}

/// The store, with a clock the test moves, randomness of its own, and a
/// crash it can arm: the `nth` batch from now that writes its position
/// dies before its commit, as a process that stops mid-batch does,
/// keeping nothing the batch did.
struct Store {
    db: Postgres,
    seeds: Cell<u64>,
    clock: Cell<i64>,
    random: Cell<u64>,
    crash_in: Rc<Cell<Option<u32>>>,
}

impl Host for Store {
    fn database(&self, _store: &str) -> Option<&dyn Database> {
        Some(self)
    }

    fn random(&self, bytes: &mut [u8]) -> bool {
        for byte in bytes.iter_mut() {
            let next = self
                .random
                .get()
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1);
            self.random.set(next);
            *byte = (next >> 33) as u8;
        }
        true
    }

    fn now_ms(&self) -> Option<i64> {
        Some(self.clock.get())
    }
}

impl Database for Store {
    fn begin_at(
        &self,
        isolation: Isolation,
    ) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>> {
        let crash_in = Rc::clone(&self.crash_in);
        Box::pin(async move {
            let inner = self.db.begin_at(isolation).await?;
            Ok(Box::new(Crashing {
                inner,
                advanced: false,
                crash_in,
            }) as Box<dyn Transaction>)
        })
    }
}

/// A transaction that may die before its commit.
struct Crashing {
    inner: Box<dyn Transaction>,
    /// It wrote a batch's position.
    advanced: bool,
    crash_in: Rc<Cell<Option<u32>>>,
}

impl Transaction for Crashing {
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
        if sql.starts_with("UPDATE \"jadpo_batch_runs\" SET \"position\"") {
            self.advanced = true;
        }
        self.inner.execute(sql, params)
    }

    fn batch<'a>(&'a mut self, sql: &'a str) -> LocalFuture<'a, Result<(), StoreError>> {
        self.inner.batch(sql)
    }

    fn commit(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
        let Crashing {
            inner,
            advanced,
            crash_in,
        } = *self;
        Box::pin(async move {
            if advanced && let Some(left) = crash_in.get() {
                if left <= 1 {
                    crash_in.set(None);
                    let _ = inner.rollback().await;
                    return Err(StoreError::Unavailable);
                }
                crash_in.set(Some(left - 1));
            }
            inner.commit().await
        })
    }

    fn rollback(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
        self.inner.rollback()
    }
}

/// The time the test starts at, which a run's leases are counted from.
const START: i64 = 1_800_000_000_000;

impl Store {
    fn ctx<'p>(&'p self, program: &'p Program) -> Ctx<'p> {
        self.seeds.set(self.seeds.get() + 1);
        let env = Env {
            seed: Some(self.seeds.get()),
            clock: Clock::TEST,
        };
        Ctx::of(program, self, env)
    }

    /// One query on its own, for what the runtime kept.
    async fn rows(&self, sql: &str, params: &[SqlValue]) -> Vec<Row> {
        self.db.query_once(sql, params).await.expect("the query")
    }
}

/// Runs `action` with `args` as no one signed in, giving its outcome and
/// the work it left for after its response.
async fn call(
    program: &Program,
    host: &Store,
    action: &str,
    args: Vec<Value>,
) -> (Outcome, Vec<Deferred>) {
    let id = program.plan().find(action).expect("the action");
    let mut ctx = host.ctx(program);
    let outcome = program.run(&mut ctx, id, args).await;
    (outcome, ctx.take_deferred())
}

fn value(outcome: Outcome) -> Value {
    match outcome {
        Outcome::Value(value) => value,
        other => panic!("expected a value, got {other:?}"),
    }
}

fn field(record: &Value, name: &str) -> Value {
    match record {
        Value::Record(record) => record.get(name).cloned().expect("the field"),
        Value::Variant(variant) if name == "name" => Value::text(&variant.name),
        other => panic!("not a record: {other:?}"),
    }
}

fn int(value: Value) -> i64 {
    match value {
        Value::Int(n) => n,
        other => panic!("not a whole number: {other:?}"),
    }
}

fn page(size: i64) -> Value {
    Value::record(
        "CheckPage",
        vec![
            (Rc::from("page_size"), Value::Int(size)),
            (Rc::from("after"), Value::None),
        ],
    )
}

/// Each product's code and how many times it was counted, in code order.
async fn counts(host: &Store) -> Vec<(String, i64)> {
    host.rows(
        "SELECT \"code\", \"checks\" FROM \"product\" ORDER BY \"code\"",
        &[],
    )
    .await
    .into_iter()
    .map(|row| match row.as_slice() {
        [SqlValue::Text(code), SqlValue::Int8(n)] => (code.clone(), *n),
        [SqlValue::Text(code), SqlValue::Int4(n)] => (code.clone(), i64::from(*n)),
        other => panic!("a product's row: {other:?}"),
    })
    .collect()
}

/// The run's state, items done and problems, as its row keeps them.
async fn kept(host: &Store, run: &Value) -> (String, i64, i64) {
    let id = match jadpo_runtime::to_sql(&field(run, "id"), jadpo_runtime::SqlType::Uuid) {
        Ok(SqlValue::Uuid(bytes)) => bytes,
        other => panic!("not an identity: {other:?}"),
    };
    let rows = host
        .rows(
            "SELECT \"state\", \"items_done\", \"problems\" FROM \"jadpo_batch_runs\" WHERE \"id\" = $1",
            &[SqlValue::Uuid(id)],
        )
        .await;
    match rows.first().map(Vec::as_slice) {
        Some(
            [
                SqlValue::Text(state),
                SqlValue::Int8(done),
                SqlValue::Int8(problems),
            ],
        ) => (state.clone(), *done, *problems),
        other => panic!("the run's row: {other:?}"),
    }
}

/// The progress `batches.status` gives, as (state, items done, problems).
async fn progress(program: &Program, host: &Store, run: &Value) -> (String, i64, i64) {
    let (outcome, _) = call(program, host, "check_progress", vec![run.clone()]).await;
    let progress = value(outcome);
    let state = match field(&progress, "state") {
        Value::Variant(variant) => variant.name.to_string(),
        other => panic!("not a state: {other:?}"),
    };
    (
        state,
        int(field(&progress, "items_done")),
        int(field(&progress, "problems")),
    )
}

/// The total `batches.status` estimates (chunk 6.4b3): `none` before the
/// first page is read, then the query's count as that page read it, and
/// the items done once the run has finished.
async fn estimate(program: &Program, host: &Store, run: &Value) -> Option<i64> {
    let (outcome, _) = call(program, host, "check_progress", vec![run.clone()]).await;
    match field(&value(outcome), "estimated_total") {
        Value::None => None,
        total => Some(int(total)),
    }
}

/// Adds `count` products, codes `p00` up, every seventh with none in
/// stock.
async fn stock(program: &Program, host: &Store, count: usize) {
    for n in 0..count {
        let stock = if n % 7 == 3 { 0 } else { 5 };
        let (outcome, _) = call(
            program,
            host,
            "add_product",
            vec![Value::text(&format!("p{n:02}")), Value::Int(stock)],
        )
        .await;
        value(outcome);
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
        clock: Cell::new(START),
        random: Cell::new(7),
        crash_in: Rc::new(Cell::new(None)),
    }
}

/// The batch runs one action left, as the runner is handed them.
fn starts(work: &[Deferred]) -> Vec<BatchStart> {
    work.iter()
        .filter_map(|work| match work {
            Deferred::Batch(start) => Some(start.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_run_commits_page_by_page_keeps_problems_and_finishes_once() {
    let Some(loaded) = load("pages") else {
        return;
    };
    let program = &loaded.program;
    let host = host(&loaded.url);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        stock(program, &host, 60).await;
        let check = value(call(program, &host, "new_check", vec![]).await.0);
        let (outcome, work) = call(
            program,
            &host,
            "check_stock",
            vec![field(&check, "id"), page(3)],
        )
        .await;
        let check = value(outcome);
        let run = field(&check, "run");
        assert_eq!(starts(&work).len(), 1, "the run is left for after the action");
        // Nothing runs before the runner does: the action only kept it.
        assert_eq!(kept(&host, &run).await, ("running".to_string(), 0, 0));
        assert_eq!(progress(program, &host, &run).await, ("running".into(), 0, 0));
        assert_eq!(estimate(program, &host, &run).await, None, "nothing counted yet");
        assert!(counts(&host).await.iter().all(|(_, n)| *n == 0));

        let outcomes = run_deferred(program, &host, work).await;
        assert!(matches!(&outcomes[..], [Outcome::Value(_)]), "{outcomes:?}");

        // Every product counted once: twenty pages of three, past the
        // sixteen a chain of events would stop at, each committed with its
        // position. One with none in stock failed after it was counted,
        // and its count was undone with it.
        let counted = counts(&host).await;
        assert_eq!(counted.len(), 60);
        for (n, (code, checks)) in counted.iter().enumerate() {
            let expected = if n % 7 == 3 { 0 } else { 1 };
            assert_eq!(*checks, expected, "{code}");
        }
        let failed = (0..60).filter(|n| n % 7 == 3).count() as i64;
        assert_eq!(kept(&host, &run).await, ("finished".to_string(), 60, failed));
        assert_eq!(
            progress(program, &host, &run).await,
            ("finished".into(), 60, failed)
        );
        assert_eq!(estimate(program, &host, &run).await, Some(60));
        // Each problem is its row, as the walk numbers items from 1, and
        // the failure's code; never a value.
        let problems = host
            .rows(
                "SELECT \"row\", \"column\", \"rule\" FROM \"jadpo_batch_problems\" ORDER BY \"row\"",
                &[],
            )
            .await;
        let expected: Vec<Row> = (0..60)
            .filter(|n| n % 7 == 3)
            .map(|n| {
                vec![
                    SqlValue::Int8(n + 1),
                    SqlValue::Text(String::new()),
                    SqlValue::Text("out_of_stock".into()),
                ]
            })
            .collect();
        assert_eq!(problems, expected);
        // `finished:` ran once, with the summary.
        let rows = host
            .rows(
                "SELECT \"checked\", \"problems\", \"listed\" FROM \"stock_check\"",
                &[],
            )
            .await;
        assert_eq!(
            rows,
            [vec![
                SqlValue::Int8(60),
                SqlValue::Int8(failed),
                SqlValue::Int8(failed)
            ]]
        );
        // A run that finished is never run again.
        let again = jadpo_runtime::batches::run(program, &host, &starts_of(&run)).await;
        assert_eq!(again.0, Ran::NotClaimed);
    });
}

/// The start a sweep would make of the run `run` is.
fn starts_of(run: &Value) -> BatchStart {
    let id = match jadpo_runtime::to_sql(&field(run, "id"), jadpo_runtime::SqlType::Uuid) {
        Ok(SqlValue::Uuid(bytes)) => bytes,
        other => panic!("not an identity: {other:?}"),
    };
    BatchStart {
        store: "main".into(),
        run: id,
    }
}

#[test]
fn a_crash_mid_batch_resumes_with_no_item_done_twice_and_none_skipped() {
    let Some(loaded) = load("crash") else {
        return;
    };
    let program = &loaded.program;
    let host = host(&loaded.url);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        stock(program, &host, 21).await;
        let check = value(call(program, &host, "new_check", vec![]).await.0);
        let (outcome, work) = call(
            program,
            &host,
            "check_stock",
            vec![field(&check, "id"), page(3)],
        )
        .await;
        let run = field(&value(outcome), "run");
        // The third page's items run, and the process dies before it
        // commits.
        host.crash_in.set(Some(3));
        let start = starts(&work).remove(0);
        let (ran, _) = jadpo_runtime::batches::run(program, &host, &start).await;
        assert!(matches!(ran, Ran::Paused(_)), "{ran:?}");
        let counted = counts(&host).await;
        let done: Vec<i64> = counted.iter().map(|(_, n)| *n).collect();
        // Two pages committed; the third's counts died with it.
        let expected: Vec<i64> = (0..21).map(|n| i64::from(n < 6 && n % 7 != 3)).collect();
        assert_eq!(done, expected);
        assert_eq!(
            progress(program, &host, &run).await,
            ("running".into(), 6, 1)
        );
        // The query's count, taken as its first page was read.
        assert_eq!(estimate(program, &host, &run).await, Some(21));

        // Its lease hasn't lapsed: a sweep leaves it to the runner that
        // holds it.
        let (resumed, _) = jadpo_runtime::batches::resume(program, &host, START + 1_000).await;
        assert!(resumed.is_empty(), "{resumed:?}");
        // Once it has, a sweep picks the run up where it committed.
        host.clock
            .set(START + jadpo_runtime::lease::LONGEST_MS + 1_000);
        let (resumed, _) = jadpo_runtime::batches::resume(
            program,
            &host,
            START + jadpo_runtime::lease::LONGEST_MS + 1_000,
        )
        .await;
        assert_eq!(resumed.len(), 1, "{resumed:?}");
        assert_eq!(resumed[0].1, Ran::Finished);
        for (n, (code, checks)) in counts(&host).await.iter().enumerate() {
            assert_eq!(
                *checks,
                i64::from(n % 7 != 3),
                "{code}: counted once, or undone"
            );
        }
        assert_eq!(
            progress(program, &host, &run).await,
            ("finished".into(), 21, 3)
        );
        let rows = host
            .rows("SELECT \"checked\", \"problems\" FROM \"stock_check\"", &[])
            .await;
        assert_eq!(rows, [vec![SqlValue::Int8(21), SqlValue::Int8(3)]]);
    });
}

#[test]
fn a_key_gives_a_second_start_the_same_run_and_cancel_stops_after_the_batch_in_hand() {
    let Some(loaded) = load("key") else {
        return;
    };
    let program = &loaded.program;
    let host = host(&loaded.url);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        stock(program, &host, 12).await;
        let check = value(call(program, &host, "new_check", vec![]).await.0);
        let (first, first_work) = call(
            program,
            &host,
            "check_stock",
            vec![field(&check, "id"), page(3)],
        )
        .await;
        let (second, second_work) = call(
            program,
            &host,
            "check_stock",
            vec![field(&check, "id"), page(3)],
        )
        .await;
        let run = field(&value(first), "run");
        assert_eq!(field(&value(second), "run"), run, "the same run");
        let runs = host
            .rows("SELECT count(*) FROM \"jadpo_batch_runs\"", &[])
            .await;
        assert_eq!(runs, [vec![SqlValue::Int8(1)]]);
        // One page, then the process stops; a cancel is asked for.
        host.crash_in.set(Some(2));
        let (ran, _) = jadpo_runtime::batches::run(program, &host, &starts(&first_work)[0]).await;
        assert!(matches!(ran, Ran::Paused(_)), "{ran:?}");
        let (stopped, _) = call(program, &host, "stop_check", vec![run.clone()]).await;
        assert_eq!(value(stopped), Value::Unit);
        assert_eq!(
            progress(program, &host, &run).await,
            ("running".into(), 3, 0)
        );
        assert_eq!(estimate(program, &host, &run).await, Some(12));
        // The second start's runner finds the run held, and the sweep,
        // once the lease lapses, finds the cancel at its next batch.
        let (ran, _) = jadpo_runtime::batches::run(program, &host, &starts(&second_work)[0]).await;
        assert_eq!(ran, Ran::NotClaimed);
        host.clock
            .set(START + jadpo_runtime::lease::LONGEST_MS + 1_000);
        let (resumed, _) = jadpo_runtime::batches::resume(
            program,
            &host,
            START + jadpo_runtime::lease::LONGEST_MS + 1_000,
        )
        .await;
        assert_eq!(resumed.len(), 1);
        assert_eq!(resumed[0].1, Ran::Cancelled);
        assert_eq!(
            progress(program, &host, &run).await,
            ("cancelled".into(), 3, 0)
        );
        // A run that didn't finish keeps the count it was given.
        assert_eq!(estimate(program, &host, &run).await, Some(12));
        let counted: i64 = counts(&host).await.iter().map(|(_, n)| n).sum();
        assert_eq!(counted, 3, "only the first page's counts");
        // `finished:` never ran for a cancelled run.
        let rows = host
            .rows("SELECT \"checked\" FROM \"stock_check\"", &[])
            .await;
        assert_eq!(rows, [vec![SqlValue::Null(jadpo_runtime::SqlType::Int8)]]);

        // A run with no key and no `finished:` starts afresh each time.
        let (one, work_one) = call(program, &host, "quick_check", vec![page(5)]).await;
        let (two, _) = call(program, &host, "quick_check", vec![page(5)]).await;
        let (one, two) = (value(one), value(two));
        assert_ne!(one, two);
        let outcomes = run_deferred(program, &host, work_one).await;
        assert!(matches!(&outcomes[..], [Outcome::Value(_)]));
        assert_eq!(
            progress(program, &host, &one).await,
            ("finished".into(), 12, 2)
        );
        assert_eq!(
            progress(program, &host, &two).await,
            ("running".into(), 0, 0)
        );
    });
}

#[test]
fn a_start_that_rolls_back_leaves_no_run() {
    let Some(loaded) = load("rollback") else {
        return;
    };
    let program = &loaded.program;
    let host = host(&loaded.url);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        stock(program, &host, 3).await;
        // No check with this id: the action fails after starting the run,
        // and rolls back.
        let missing = Value::uuid("0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e");
        let (outcome, work) = call(program, &host, "check_stock", vec![missing, page(3)]).await;
        assert!(matches!(outcome, Outcome::Failed(_)), "{outcome:?}");
        let runs = host
            .rows("SELECT count(*) FROM \"jadpo_batch_runs\"", &[])
            .await;
        assert_eq!(runs, [vec![SqlValue::Int8(0)]]);
        // What it left for after finds nothing to run.
        for start in starts(&work) {
            let (ran, _) = jadpo_runtime::batches::run(program, &host, &start).await;
            assert_eq!(ran, Ran::NotClaimed);
        }
        assert!(counts(&host).await.iter().all(|(_, n)| *n == 0));
    });
}

#[test]
fn the_runner_reaches_batches_only_through_the_executor() {
    // What compiled code will give the runner too (Phase 8): the plans
    // and codecs, behind `Executor::batches`, and nothing of the
    // interpreter's own.
    let root = project("executor");
    let loaded = jadpo_db::load(&root);
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let batches = program.runtime_plan().batches().expect("its batches");
    let names: Vec<&str> = batches
        .plans
        .iter()
        .map(|plan| plan.name.as_str())
        .collect();
    // Each call named by what it is, in identity keys (chunk 6.8): the
    // action starting it, the query `from:` walks and the action `each:`
    // runs, never its place among the action's calls.
    assert_eq!(
        names,
        [
            "action check_stock from query products_to_check each action check_product",
            "action quick_check from query products_to_check each action check_product"
        ]
    );
    let check = &batches.plans[0];
    // Its shape in canonical type text, never a compiler's `Debug` output.
    assert_eq!(
        check.shape,
        "from query products_to_check(type inventory.CheckPage,) each action check_product(item,) finished action check_done(item,field StockCheck.id,) key field StockCheck.id"
    );
    assert_eq!(check.store, "main");
    assert_eq!(check.from_args, 1);
    assert_eq!(check.captured.len(), 2, "the page, then the check's id");
    assert!(check.finished.is_some() && check.key.is_some());
    assert_eq!(check.isolation, Isolation::RepeatableRead);
    let quick = &batches.plans[1];
    assert!(quick.finished.is_none() && quick.key.is_none());
}

#[test]
fn a_run_a_deploy_changed_stops_rather_than_reading_what_it_saved_as_something_else() {
    let Some(loaded) = load("deploy") else {
        return;
    };
    let program = &loaded.program;
    let host = host(&loaded.url);
    let runtime = Runtime::new().expect("runtime");
    // The next build's `check_stock` no longer runs `finished:`: what the
    // run saved and calls is another shape.
    let changed = loaded
        ._root
        .join("domains/inventory/actions/check_stock.jadpo");
    let text = fs::read_to_string(&changed).expect("the action");
    fs::write(
        &changed,
        text.replace(
            "        finished: summary -> check_done(summary, check_id),\n",
            "",
        )
        .replace(
            "use inventory/actions { check_done, check_product }",
            "use inventory/actions { check_product }",
        ),
    )
    .expect("the change");
    let next = jadpo_db::load(&loaded._root);
    let deployed = Program::load(&next.db, next.project).expect("the next build checks");
    runtime.block_on(async {
        stock(program, &host, 6).await;
        let check = value(call(program, &host, "new_check", vec![]).await.0);
        let (outcome, work) = call(
            program,
            &host,
            "check_stock",
            vec![field(&check, "id"), page(3)],
        )
        .await;
        let run = field(&value(outcome), "run");
        let (ran, _) = jadpo_runtime::batches::run(&deployed, &host, &starts(&work)[0]).await;
        assert_eq!(
            ran,
            Ran::Failed("a deploy changed the `batches.each` call it was started by".into())
        );
        assert_eq!(kept(&host, &run).await, ("failed".to_string(), 0, 0));
        assert!(counts(&host).await.iter().all(|(_, n)| *n == 0));
    });
}

#[test]
fn a_run_is_read_by_its_encoding_and_one_it_cannot_read_ends_failed() {
    // Chunk 6.8: a run records the encoding its saved values and its
    // `key:` were written in. One kept before encodings were recorded
    // (`NULL`) reads as encoding 1's; one a later release wrote, or one an
    // earlier release kept under a call name this one doesn't give, ends
    // `failed` with why, never misread.
    let Some(loaded) = load("encodings-6-8") else {
        return;
    };
    let program = &loaded.program;
    let host = host(&loaded.url);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        stock(program, &host, 6).await;
        let start = |change: &'static str| {
            let host = &host;
            async move {
                let check = value(call(program, host, "new_check", vec![]).await.0);
                let (outcome, work) = call(
                    program,
                    host,
                    "check_stock",
                    vec![field(&check, "id"), page(3)],
                )
                .await;
                let run = field(&value(outcome), "run");
                let encoding = host
                    .rows(
                        "SELECT \"encoding\"::int4 FROM \"jadpo_batch_runs\" WHERE \"state\" = 'running'",
                        &[],
                    )
                    .await;
                assert_eq!(
                    encoding,
                    [vec![SqlValue::Int4(jadpo_std::standard::ENCODING as i32)]],
                    "this release's encoding"
                );
                host.rows(
                    &format!(
                        "UPDATE \"jadpo_batch_runs\" SET {change} WHERE \"state\" = 'running' RETURNING \"id\""
                    ),
                    &[],
                )
                .await;
                (run, starts(&work).remove(0))
            }
        };
        let (kept_before, work) = start("\"encoding\" = NULL").await;
        let (ran, _) = jadpo_runtime::batches::run(program, &host, &work).await;
        assert_eq!(ran, Ran::Finished);
        assert_eq!(kept(&host, &kept_before).await.0, "finished");

        // One 0.1.0 wrote in encoding 1, before `Jwk` held `key_ops`
        // (chunk A3), reads as it did.
        let (kept_in_1, work) = start("\"encoding\" = 1").await;
        let (ran, _) = jadpo_runtime::batches::run(program, &host, &work).await;
        assert_eq!(ran, Ran::Finished);
        assert_eq!(kept(&host, &kept_in_1).await.0, "finished");

        let next = jadpo_std::standard::ENCODING + 1;
        let (later, work) = start(Box::leak(format!("\"encoding\" = {next}").into_boxed_str())).await;
        let (ran, _) = jadpo_runtime::batches::run(program, &host, &work).await;
        let why = format!(
            "its saved values were written in encoding {next}, which this release doesn't read"
        );
        assert_eq!(ran, Ran::Failed(why));
        assert_eq!(kept(&host, &later).await.0, "failed");

        // As chunk 6.4 named calls, by their place among the action's.
        let (renamed, work) =
            start("\"encoding\" = NULL, \"batch\" = 'action check_stock#1'").await;
        let (ran, _) = jadpo_runtime::batches::run(program, &host, &work).await;
        assert_eq!(
            ran,
            Ran::Failed(
                "it was started by a release that named its `batches.each` call another way"
                    .into()
            )
        );
        assert_eq!(kept(&host, &renamed).await.0, "failed");
        let failures = host
            .rows(
                "SELECT \"failure\" FROM \"jadpo_batch_runs\" WHERE \"state\" = 'failed' ORDER BY \"started_at\", \"id\"",
                &[],
            )
            .await;
        assert_eq!(failures.len(), 2);
    });
}
