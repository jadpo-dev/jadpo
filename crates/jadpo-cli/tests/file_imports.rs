//! Files and reading their rows into batches, against live PostgreSQL
//! (area 16, "Files"; area 19, "Data formats"; area 12, "Batches"; chunk
//! 6.4b): a store's `put` keeping bytes through the capability seam and
//! giving a `File`, which an entity field keeps; `csv.read`, `tsv.read` and
//! `json_lines.read` starting a run over a stored file once the action
//! commits; each page's rows read from the file before its transaction
//! opens, since no outside call goes out inside one (chunk 7.0c), and
//! decoded as the row type's `Input`; a row that can't be decoded, or whose
//! action fails, kept as a problem with the row as a spreadsheet numbers
//! it and its column, never its value; a run resumed from the position its
//! last page committed, no row done twice and none skipped; the faults
//! that stop a run at the file's level; and `store.read`'s whole read.
//!
//! The store is the `files` capability's in-memory fake, on the shared
//! test host, reading a few bytes at a time so pages straddle its reads.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use jadpo_cli::{Output, run};
use jadpo_host_native::{Postgres, Runtime};
use jadpo_interp::Program;
use jadpo_runtime::batches::{BatchStart, Ran};
use jadpo_runtime::capability::{Capability, Faked};
use jadpo_runtime::files::MemoryFiles;
use jadpo_runtime::runner::{Deferred, run_deferred};
use jadpo_runtime::{
    Database, Executor, Isolation, LocalFuture, Outcome, Row, SqlValue, StoreError, Transaction,
};
use jadpo_term::Environment;
use jadpo_test::host::Harness;
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
    connections: {
        imports: {
            driver: Driver.disk
            path: "/var/lib/pantry/imports"
        }
    }
}
"#,
    ),
    (
        "domains/pantry/types/shelf_code.jadpo",
        r#"type ShelfCode = Text {
    min_length: 1
    max_length: 20
}
"#,
    ),
    (
        "domains/pantry/types/item_name.jadpo",
        r#"type ItemName = Text {
    min_length: 1
    max_length: 60
}
"#,
    ),
    (
        "domains/pantry/types/stock_count.jadpo",
        r#"type StockCount = Int {
    min: 0
    max: 1000000
}
"#,
    ),
    (
        "domains/pantry/types/sheet_text.jadpo",
        r#"type SheetText = Text {
    max_length: 1000000
}
"#,
    ),
    (
        "domains/pantry/types/item_row.jadpo",
        r#"use pantry/types { ItemName, ShelfCode, StockCount }

type ItemRow = Input {
    code: ShelfCode
    name: ItemName {
        column: "Item name"
    }
    count: StockCount {
        default: 0
    }
}
"#,
    ),
    (
        "domains/pantry/entities/item.jadpo",
        r#"use pantry/types { ItemName, ShelfCode, StockCount }

entity Item {
    id: Uuid
    code: ShelfCode {
        unique: true
    }
    name: ItemName
    count: StockCount

    policy: {
        create: [Access.public, Access.system]
        read: [Access.public, Access.system]
    }
}
"#,
    ),
    (
        "domains/pantry/entities/item_import.jadpo",
        r#"entity ItemImport {
    id: Uuid
    sheet: File {
        store: imports
        max_size: 5mb
        types: [FileType.csv]
    }
    run: BatchRun?
    items: Int?
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
        "domains/pantry/failures/code_taken.jadpo",
        r#"type CodeTaken = Failure {
    kind: FailureKind.conflict
    message: "Another item has that code."
}
"#,
    ),
    (
        "domains/pantry/failures/banned_code.jadpo",
        r#"type BannedCode = Failure {
    kind: FailureKind.rejected
    message: "That code may not be stocked."
}
"#,
    ),
    (
        "domains/pantry/failures/sheet_not_kept.jadpo",
        r#"type SheetNotKept = Failure {
    kind: FailureKind.unavailable
    message: "There was no room to keep the sheet."
}
"#,
    ),
    (
        "domains/pantry/actions/keep_sheet.jadpo",
        r#"use app { imports }
use pantry/entities { ItemImport }
use pantry/failures { SheetNotKept }
use pantry/types { SheetText }

action keep_sheet(sheet_text: SheetText) fails SheetNotKept -> ItemImport {
    var sheet: File = attempt imports.put(encoding.to_utf8(sheet_text), FileType.csv) {
        storage_full: SheetNotKept
    }
    return ItemImport.create {
        set: {
            sheet: sheet
            run: none
            items: none
            problems: none
            listed: none
        }
    }
}
"#,
    ),
    (
        "domains/pantry/actions/save_item.jadpo",
        r#"use pantry/entities { Item }
use pantry/failures { BannedCode, CodeTaken }
use pantry/types { ItemRow }

action save_item(row: ItemRow) fails BannedCode, CodeTaken -> Unit {
    var item: Item = attempt Item.create {
        set: {
            code: row.code
            name: row.name
            count: row.count
        }
        conflict: {
            Item.code: CodeTaken
        }
    }
    if (item.code == "BANNED") {
        reject BannedCode {}
    }
}
"#,
    ),
    (
        "domains/pantry/actions/items_read.jadpo",
        r#"use pantry/entities { ItemImport, ItemImportNotFound }

action items_read(summary: ImportSummary, import_id: ItemImport.id) fails ItemImportNotFound -> Unit {
    attempt ItemImport.update(import_id) {
        set: {
            items: summary.items
            problems: summary.items_with_problems
            listed: list.count(summary.problems)
        }
    }
}
"#,
    ),
    (
        "domains/pantry/actions/read_items.jadpo",
        r#"use pantry/actions { items_read, save_item }
use pantry/entities { ItemImport, ItemImportNotFound }
use pantry/types { ItemRow }

action read_items(import_id: ItemImport.id) fails ItemImportNotFound -> ItemImport {
    var import: ItemImport = attempt ItemImport.get(import_id)
    var run: BatchRun = csv.read(
        import.sheet,
        as: ItemRow,
        each: row -> save_item(row),
        finished: summary -> items_read(summary, import.id),
        key: import.id
    )
    return ItemImport.update(import) {
        set: {
            run: run
        }
    }
}
"#,
    ),
    (
        "domains/pantry/actions/read_sheet.jadpo",
        r#"use pantry/actions { save_item }
use pantry/types { ItemRow }

action read_sheet(sheet: File) -> BatchRun {
    return csv.read(sheet, as: ItemRow, each: row -> save_item(row))
}
"#,
    ),
    (
        "domains/pantry/actions/read_tab_sheet.jadpo",
        r#"use pantry/actions { save_item }
use pantry/types { ItemRow }

action read_tab_sheet(sheet: File) -> BatchRun {
    return tsv.read(sheet, as: ItemRow, each: row -> save_item(row))
}
"#,
    ),
    (
        "domains/pantry/actions/read_lines.jadpo",
        r#"use pantry/actions { save_item }
use pantry/types { ItemRow }

action read_lines(sheet: File) -> BatchRun {
    return json_lines.read(sheet, as: ItemRow, each: row -> save_item(row))
}
"#,
    ),
    (
        "domains/pantry/actions/sheet_bytes.jadpo",
        r#"use app { imports }

action sheet_bytes(sheet: File) -> FileContents {
    return imports.read(sheet)
}
"#,
    ),
];

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("file-imports-6.4b-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    root
}

/// When the test starts: 2026-10-19T09:00:00Z.
const T0: i64 = 1_792_400_400_000;

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

/// The store, through which the test can make the commit of the `nth`
/// page from now that writes a run's position die, as a process that
/// stops mid-batch does, keeping nothing the page did.
struct Crashable {
    db: Postgres,
    crash_in: Rc<Cell<Option<u32>>>,
}

impl Database for Crashable {
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

    fn query_once<'a>(
        &'a self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
        self.db.query_once(sql, params)
    }
}

struct Crashing {
    inner: Box<dyn Transaction>,
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

    // The driver's own, which keep what a refused statement left undone
    // by going back to a savepoint.
    fn savepoint(&mut self) -> LocalFuture<'_, Result<(), StoreError>> {
        self.inner.savepoint()
    }

    fn release_savepoint(&mut self) -> LocalFuture<'_, Result<(), StoreError>> {
        self.inner.release_savepoint()
    }

    fn rollback_to_savepoint(&mut self) -> LocalFuture<'_, Result<(), StoreError>> {
        self.inner.rollback_to_savepoint()
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

/// The stand a test runs on: the harness over the store, and the `imports`
/// store's in-memory fake reading `read_at_most` bytes at a time.
struct Stand {
    harness: Harness<Crashable>,
    files: Rc<MemoryFiles>,
    crash_in: Rc<Cell<Option<u32>>>,
}

impl Stand {
    fn new(url: &str, read_at_most: usize, rows_per_batch: u32) -> Stand {
        let crash_in = Rc::new(Cell::new(None));
        let db = jadpo_host_native::postgres(url).expect("the store");
        let harness = Harness::new(Crashable {
            db,
            crash_in: Rc::clone(&crash_in),
        })
        .at(T0)
        .random_from(0)
        .rows_per_batch(rows_per_batch);
        Stand {
            harness,
            files: Rc::new(MemoryFiles::new().reading_at_most(read_at_most)),
            crash_in,
        }
    }

    fn host(&self) -> Faked<&Harness<Crashable>> {
        Faked::new(&self.harness).with("imports", Rc::clone(&self.files) as Rc<dyn Capability>)
    }

    /// Runs `action` with `args`, giving its outcome and the work it left
    /// for after its response.
    async fn call(
        &self,
        program: &Program,
        action: &str,
        args: Vec<Value>,
    ) -> (Outcome, Vec<Deferred>) {
        let id = program.plan().find(action).expect("the action");
        let host = self.host();
        let mut ctx = self.harness.ctx_on(program.places(), &host);
        let outcome = program.run(&mut ctx, id, args).await;
        let left = ctx.take_deferred();
        (outcome, left)
    }

    /// `bytes` kept in the store as a CSV file, as a `File` an action is
    /// given.
    fn keep(&self, bytes: &[u8]) -> Value {
        self.files.keep("imports", bytes, "csv")
    }

    async fn rows(&self, sql: &str) -> Vec<Row> {
        self.harness.rows(sql).await
    }
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
        other => panic!("not a record: {other:?}"),
    }
}

fn starts(work: &[Deferred]) -> Vec<BatchStart> {
    work.iter()
        .filter_map(|piece| match piece {
            Deferred::Batch(start) => Some(start.clone()),
            _ => None,
        })
        .collect()
}

/// Each item's code, name and count, in code order.
async fn items(stand: &Stand) -> Vec<(String, String, i64)> {
    stand
        .rows("SELECT \"code\", \"name\", \"count\" FROM \"item\" ORDER BY \"code\"")
        .await
        .into_iter()
        .map(|row| match row.as_slice() {
            [SqlValue::Text(code), SqlValue::Text(name), count] => (
                code.clone(),
                name.clone(),
                match count {
                    SqlValue::Int8(n) => *n,
                    SqlValue::Int4(n) => i64::from(*n),
                    other => panic!("a count: {other:?}"),
                },
            ),
            other => panic!("an item's row: {other:?}"),
        })
        .collect()
}

/// Each problem kept: its row, its column and its rule, in row order.
async fn problems(stand: &Stand) -> Vec<(i64, String, String)> {
    stand
        .rows("SELECT \"row\", \"column\", \"rule\" FROM \"jadpo_batch_problems\" ORDER BY \"row\"")
        .await
        .into_iter()
        .map(|row| match row.as_slice() {
            [
                SqlValue::Int8(at),
                SqlValue::Text(column),
                SqlValue::Text(rule),
            ] => (*at, column.clone(), rule.clone()),
            other => panic!("a problem's row: {other:?}"),
        })
        .collect()
}

fn problem(row: i64, column: &str, rule: &str) -> (i64, String, String) {
    (row, column.to_string(), rule.to_string())
}

/// The run's state, why it ended if it failed, items done and problems.
async fn run_row(stand: &Stand) -> (String, Option<String>, i64, i64) {
    let rows = stand
        .rows(
            "SELECT \"state\", \"failure\", \"items_done\", \"problems\" FROM \"jadpo_batch_runs\"",
        )
        .await;
    match rows.as_slice() {
        [row] => match row.as_slice() {
            [
                SqlValue::Text(state),
                failure,
                SqlValue::Int8(done),
                SqlValue::Int8(problems),
            ] => (
                state.clone(),
                match failure {
                    SqlValue::Text(why) => Some(why.clone()),
                    _ => None,
                },
                *done,
                *problems,
            ),
            other => panic!("the run's row: {other:?}"),
        },
        other => panic!("one run: {other:?}"),
    }
}

/// How a run ended, as its row keeps it: its state and why.
type Ended = (String, Option<String>);

/// A sheet with every kind of row: quoted cells holding a comma, a line
/// break and quotes, a cell that isn't its type's, one past its rule, a
/// blank line, an item whose action fails after it wrote, a code taken
/// already, a required cell left empty, and no line break at the end.
const SHEET: &str = "code,Item name,count\r\nA1,Apples,3\r\nA2,\"Pears, conference\",4\r\nA3,\"Plums\r\nfrom the orchard\",5\r\nA4,Quinces,x\r\nA5,Raisins,-1\r\n\r\nBANNED,Bad,1\r\nA1,Again,2\r\nA6,,7\r\nA7,\"Dates \"\"Medjool\"\"\",8";

/// A sheet put through the store and read with `csv.read`: each row read
/// as an `ItemRow` by its column, in pages read from the store before each
/// page's transaction opens, a few bytes at a time; every row that can't be
/// read or whose action fails kept as a problem, by its row as a
/// spreadsheet numbers it and its column, never its value; and
/// `finished:` given the summary once.
#[test]
fn a_csv_sheet_is_read_into_items_row_by_row() {
    let Some(loaded) = load("csv") else {
        return;
    };
    let program = &loaded.program;
    let stand = Stand::new(&loaded.url, 7, 3);
    Runtime::new().expect("runtime").block_on(async {
        let (kept, _) = stand
            .call(program, "keep_sheet", vec![Value::text(SHEET)])
            .await;
        let import = value(kept);
        let sheet = field(&import, "sheet");
        assert_eq!(field(&sheet, "store"), Value::text("imports"));
        assert_eq!(field(&sheet, "size"), Value::Int(SHEET.len() as i64));
        let key = match field(&sheet, "key") {
            Value::Text(key) => key.to_string(),
            other => panic!("a key: {other:?}"),
        };
        assert!(jadpo_std::files::is_key(&key), "{key}");
        assert_eq!(stand.files.bytes(&key).as_deref(), Some(SHEET.as_bytes()));

        let (read, work) = stand
            .call(program, "read_items", vec![field(&import, "id")])
            .await;
        let read = value(read);
        assert!(matches!(field(&read, "run"), Value::Record(_)));
        assert_eq!(stand.files.reads(), 0, "nothing is read in the request");
        let ran = run_deferred(program, &stand.host(), work).await;
        assert!(
            ran.iter()
                .all(|outcome| matches!(outcome, Outcome::Value(_))),
            "{ran:?}"
        );
        assert!(stand.files.reads() > 1, "read in pieces");
        assert_eq!(
            items(&stand).await,
            [
                ("A1".into(), "Apples".into(), 3),
                ("A2".into(), "Pears, conference".into(), 4),
                ("A3".into(), "Plums\r\nfrom the orchard".into(), 5),
                ("A7".into(), "Dates \"Medjool\"".into(), 8),
            ]
        );
        assert_eq!(
            problems(&stand).await,
            [
                problem(5, "count", "type"),
                problem(6, "count", "min"),
                problem(8, "", "banned_code"),
                problem(9, "", "code_taken"),
                problem(10, "Item name", "required"),
            ]
        );
        assert_eq!(run_row(&stand).await, ("finished".into(), None, 9, 5));
        let kept = stand
            .rows("SELECT \"items\", \"problems\", \"listed\" FROM \"item_import\"")
            .await;
        assert_eq!(
            kept,
            [vec![
                SqlValue::Int8(9),
                SqlValue::Int8(5),
                SqlValue::Int8(5)
            ]]
        );
        // Never a value in a problem, nor in what the run kept.
        let held = stand
            .rows("SELECT \"failure\" FROM \"jadpo_batch_runs\" WHERE \"failure\" IS NOT NULL")
            .await;
        assert!(held.is_empty(), "{held:?}");
    });
}

/// A page whose commit dies keeps nothing it did, and the run resumes
/// from the position the last page committed: no row done twice, none
/// skipped, and the problems' rows numbered as before.
#[test]
fn a_run_resumes_from_its_last_committed_page_with_no_row_done_twice() {
    let Some(loaded) = load("resume") else {
        return;
    };
    let program = &loaded.program;
    let stand = Stand::new(&loaded.url, 64, 4);
    let mut sheet = String::from("code,Item name\n");
    for n in 0..30 {
        let name = if n == 17 { "" } else { "Tin" };
        sheet.push_str(&format!("B{n:02},{name}\n"));
    }
    Runtime::new().expect("runtime").block_on(async {
        let file = stand.keep(sheet.as_bytes());
        let (started, work) = stand.call(program, "read_sheet", vec![file]).await;
        let _run = value(started);
        // The third page's items run, and its commit dies.
        stand.crash_in.set(Some(3));
        let start = starts(&work).remove(0);
        let (ran, _) = jadpo_runtime::batches::run(program, &stand.host(), &start).await;
        assert!(matches!(ran, Ran::Paused(_)), "{ran:?}");
        assert_eq!(items(&stand).await.len(), 8, "two pages of four");
        stand
            .harness
            .set_now(T0 + jadpo_runtime::lease::LONGEST_MS + 1_000);
        let (resumed, _) = jadpo_runtime::batches::resume(
            program,
            &stand.host(),
            T0 + jadpo_runtime::lease::LONGEST_MS + 1_000,
        )
        .await;
        assert_eq!(resumed.len(), 1, "{resumed:?}");
        assert_eq!(resumed[0].1, Ran::Finished);
        let codes: Vec<String> = items(&stand)
            .await
            .into_iter()
            .map(|(code, _, count)| {
                assert_eq!(count, 0, "the default");
                code
            })
            .collect();
        let expected: Vec<String> = (0..30)
            .filter(|n| *n != 17)
            .map(|n| format!("B{n:02}"))
            .collect();
        assert_eq!(codes, expected);
        assert_eq!(
            problems(&stand).await,
            [problem(19, "Item name", "required")]
        );
        assert_eq!(run_row(&stand).await, ("finished".into(), None, 30, 1));
    });
}

/// What stops a run at the file's level, before or at the place it
/// occurs, its rows before kept: a required column missing, a column no
/// field reads, an unterminated quote (named by the line it opened on),
/// text that isn't text, and a gzip file read without
/// `max_unpacked_size:`. A byte-order mark is removed, and UTF-16 with
/// one is read.
#[test]
fn file_level_faults_stop_the_run_and_a_marked_encoding_is_read() {
    let Some(loaded) = load("faults") else {
        return;
    };
    let program = &loaded.program;
    let stand = Stand::new(&loaded.url, 5, 2);
    Runtime::new().expect("runtime").block_on(async {
        let failed = |why: &str| ("failed".to_string(), Some(why.to_string()));
        let cases: Vec<(&str, Vec<u8>, Ended)> = vec![
            (
                "missing",
                b"Item name,count\nApples,3\n".to_vec(),
                failed("its file has no column `code`, which `ItemRow.code` is read from"),
            ),
            (
                "unknown",
                b"code,Item name,colour\nC1,Apples,red\n".to_vec(),
                failed("its file's column `colour` is no field of `ItemRow`"),
            ),
            (
                "quote",
                b"code,Item name\nD1,Apples\nD2,\"Pears\nD3,Plums\n".to_vec(),
                failed("its file has a quote opened on line 3 that never closes"),
            ),
            (
                "text",
                b"code,Item name\nE1,App\xffles\n".to_vec(),
                failed("line 2 of its file isn't UTF-8 text"),
            ),
            (
                "gzip",
                vec![0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00],
                failed(
                    "its file is gzip, which `csv.read` reads only when given `max_unpacked_size:`",
                ),
            ),
        ];
        for (case, bytes, expected) in cases {
            stand.rows("DELETE FROM \"jadpo_batch_problems\"").await;
            stand.rows("DELETE FROM \"jadpo_batch_runs\"").await;
            let file = stand.keep(&bytes);
            let (_, work) = stand.call(program, "read_sheet", vec![file]).await;
            run_deferred(program, &stand.host(), work).await;
            let (state, why, _, _) = run_row(&stand).await;
            assert_eq!((state, why), expected, "{case}");
        }
        // The quote's run kept the row before it.
        assert!(items(&stand).await.iter().any(|(code, _, _)| code == "D1"));

        // A byte-order mark, then UTF-16 with one, little- and big-endian.
        stand.rows("DELETE FROM \"jadpo_batch_runs\"").await;
        let mut marked = vec![0xef, 0xbb, 0xbf];
        marked.extend_from_slice("code,Item name\nF1,Éclairs\n".as_bytes());
        let text = "code,Item name\r\nF2,Figs\r\nF3,Fennel ☕\r\n";
        let mut little = vec![0xff, 0xfe];
        let mut big = vec![0xfe, 0xff];
        for unit in text.encode_utf16() {
            little.extend_from_slice(&unit.to_le_bytes());
        }
        for unit in "code,Item name\nF4,Feijoa\n".encode_utf16() {
            big.extend_from_slice(&unit.to_be_bytes());
        }
        for bytes in [marked, little, big] {
            stand.rows("DELETE FROM \"jadpo_batch_runs\"").await;
            let file = stand.keep(&bytes);
            let (_, work) = stand.call(program, "read_sheet", vec![file]).await;
            run_deferred(program, &stand.host(), work).await;
            assert_eq!(run_row(&stand).await.0, "finished");
        }
        let names: Vec<String> = items(&stand)
            .await
            .into_iter()
            .filter(|(code, _, _)| code.starts_with('F'))
            .map(|(_, name, _)| name)
            .collect();
        assert_eq!(names, ["Éclairs", "Figs", "Fennel ☕", "Feijoa"]);
    });
}

/// TSV is CSV with a tab between cells; JSON Lines is a JSON value a
/// line, in the row type's JSON (`column:` is a CSV column's), a
/// problem's column the field's path, and its row the line.
#[test]
fn tsv_and_json_lines_rows_are_read_the_same_way() {
    let Some(loaded) = load("formats") else {
        return;
    };
    let program = &loaded.program;
    let stand = Stand::new(&loaded.url, 9, 2);
    Runtime::new().expect("runtime").block_on(async {
        let file = stand.keep(b"code\tItem name\tcount\nT1\tTea, green\t2\nT2\tToffee\tmany\n");
        let (_, work) = stand.call(program, "read_tab_sheet", vec![file]).await;
        run_deferred(program, &stand.host(), work).await;
        assert_eq!(run_row(&stand).await, ("finished".into(), None, 2, 1));
        assert_eq!(problems(&stand).await, [problem(3, "count", "type")]);

        stand.rows("DELETE FROM \"jadpo_batch_problems\"").await;
        stand.rows("DELETE FROM \"jadpo_batch_runs\"").await;
        let lines = concat!(
            "{\"code\": \"J1\", \"name\": \"Jam\", \"count\": 4}\n",
            "{\"code\": \"J2\", \"name\": \"Jelly\", \"count\": -4}\n",
            "\n",
            "not json\n",
            "{\"code\": \"J3\", \"name\": \"Juice\", \"colour\": \"red\"}\n",
            "{\"code\": \"J4\", \"name\": \"Jerky\"}",
        );
        let file = stand.keep(lines.as_bytes());
        let (_, work) = stand.call(program, "read_lines", vec![file]).await;
        run_deferred(program, &stand.host(), work).await;
        assert_eq!(run_row(&stand).await, ("finished".into(), None, 5, 3));
        assert_eq!(
            problems(&stand).await,
            [
                problem(2, "count", "min"),
                problem(4, "", "malformed"),
                problem(5, "colour", "unknown_field"),
            ]
        );
        let codes: Vec<String> = items(&stand)
            .await
            .into_iter()
            .map(|(code, _, _)| code)
            .filter(|code| code.starts_with('J'))
            .collect();
        assert_eq!(codes, ["J1", "J4"]);
    });
}

/// `store.read` gives a file's bytes whole, as `FileContents`, and the
/// runtime refuses to read more than 10 MB into memory; a put that finds
/// no room fails as its `attempt` maps `storage_full`.
#[test]
fn a_whole_read_gives_the_bytes_and_a_full_store_fails_as_mapped() {
    let Some(loaded) = load("whole") else {
        return;
    };
    let program = &loaded.program;
    let stand = Stand::new(&loaded.url, 3, 2);
    Runtime::new().expect("runtime").block_on(async {
        let file = stand.keep(b"one,two\n1,2\n");
        let (read, _) = stand.call(program, "sheet_bytes", vec![file]).await;
        assert_eq!(value(read), Value::Bytes(Rc::from(&b"one,two\n1,2\n"[..])));

        let large = jadpo_runtime::files::file_value(
            "imports",
            &jadpo_std::files::key_for(&[7; 16]),
            None,
            jadpo_std::files::WHOLE_READ_LIMIT as i64 + 1,
            "csv",
        );
        let (refused, _) = stand.call(program, "sheet_bytes", vec![large]).await;
        assert!(
            matches!(&refused, Outcome::Fault(fault) if format!("{:?}", fault.kind).contains("10 MB")),
            "{refused:?}"
        );

        stand.files.fill();
        let (full, _) = stand
            .call(program, "keep_sheet", vec![Value::text("a,b\n")])
            .await;
        assert!(
            matches!(&full, Outcome::Failed(failure) if failure.code == "sheet_not_kept"),
            "{full:?}"
        );
        let imports = stand
            .rows("SELECT count(*) FROM \"item_import\"")
            .await;
        assert_eq!(imports, [vec![SqlValue::Int8(0)]]);
    });
}
