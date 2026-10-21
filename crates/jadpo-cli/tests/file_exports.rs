//! Exports: writing a query's records into a stored file a page at a
//! time, against live PostgreSQL (area 19, "Data formats"; area 12,
//! "How an export's run is kept"; area 16, "Files"; chunk 6.4b2):
//! `csv.write`, `tsv.write`, `json_lines.write` and `json.write`'s file
//! form starting a run over a named query's pages once the action commits;
//! each page read in a transaction of its own, each item turned into its
//! row by `each:`, a pure expression, and the page's bytes put in the
//! store only once that transaction has closed, since no outside call goes
//! out inside one (chunk 7.0c), before the position commits; the pieces
//! joined into the one file `finished:` receives in its `ExportSummary`; a
//! `CsvFormat` naming who reads a CSV; an item whose row faults kept as a
//! problem, by its place in the walk, never its value; a run resumed from
//! the position its last page committed, no row written twice; and a
//! cancelled run leaving no file and no pieces.
//!
//! The store is the `files` capability's in-memory fake, on the shared
//! test host.
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

/// Each export action's `each:`: the item's row, its `scaled` overflowing
/// for a count over 922,337, which makes that item a problem.
const LINE: &str = "item -> ItemLine {
            code: item.code
            name: item.name
            count: item.count
            scaled: Scaled(item.count * 10000000000000)
            note: note
        }";

fn files() -> Vec<(&'static str, String)> {
    let export = |name: &str, call: &str| {
        format!(
            "use app {{ exports }}
use pantry/actions {{ exported }}
use pantry/entities {{ ItemExport }}
use pantry/queries {{ items_by_code }}
use pantry/types {{ ItemLine, ItemNote, ItemPage, Scaled }}

action {name}(export_id: ItemExport.id, page: ItemPage, note: ItemNote?) -> BatchRun {{
    return {call}
}}
"
        )
    };
    vec![
        (
            "app.jadpo",
            r#"application {
    connections: {
        exports: {
            driver: Driver.disk
            path: "/var/lib/pantry/exports"
        }
    }
}
"#
            .to_string(),
        ),
        (
            "domains/pantry/types/shelf_code.jadpo",
            "type ShelfCode = Text {\n    min_length: 1\n    max_length: 20\n}\n".to_string(),
        ),
        (
            "domains/pantry/types/item_name.jadpo",
            "type ItemName = Text {\n    min_length: 1\n    max_length: 60\n}\n".to_string(),
        ),
        (
            "domains/pantry/types/stock_count.jadpo",
            "type StockCount = Int {\n    min: 0\n    max: 1000000\n}\n".to_string(),
        ),
        (
            "domains/pantry/types/scaled.jadpo",
            "type Scaled = Int {}\n".to_string(),
        ),
        (
            "domains/pantry/types/item_note.jadpo",
            "type ItemNote = Text {\n    max_length: 40\n}\n".to_string(),
        ),
        (
            "domains/pantry/types/item_page.jadpo",
            "type ItemPage = Input {\n    page_size: PageSize {\n        default: 25\n    }\n    after: PageCursor {\n        optional: true\n    }\n}\n"
                .to_string(),
        ),
        (
            "domains/pantry/types/item_line.jadpo",
            r#"use pantry/types { ItemName, ItemNote, Scaled, ShelfCode, StockCount }

type ItemLine = Output {
    code: ShelfCode {
        column: "Code"
    }
    name: ItemName {
        column: "Item name"
    }
    count: StockCount
    scaled: Scaled
    note: ItemNote?
}
"#
            .to_string(),
        ),
        (
            "domains/pantry/entities/item.jadpo",
            r#"use pantry/types { ItemName, ShelfCode, StockCount }

entity Item {
    id: Uuid
    code: ShelfCode {
        immutable: true
    }
    name: ItemName
    count: StockCount

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
    }
}
"#
            .to_string(),
        ),
        (
            "domains/pantry/entities/item_export.jadpo",
            r#"entity ItemExport {
    id: Uuid
    file: File? {
        store: exports
        max_size: 50mb
    }
    items: Int?
    problems: Int?

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.public, Access.system]
    }
}
"#
            .to_string(),
        ),
        (
            "domains/pantry/queries/items_by_code.jadpo",
            r#"use pantry/entities { Item }
use pantry/types { ItemPage }

query items_by_code(page: ItemPage) -> Page<Item> {
    from: Item
    order: [Item.code asc]
    page: {
        size: page.page_size
        after: page.after
    }
}
"#
            .to_string(),
        ),
        (
            "domains/pantry/actions/add_item.jadpo",
            r#"use pantry/entities { Item }
use pantry/types { ItemName, ShelfCode, StockCount }

action add_item(code: ShelfCode, name: ItemName, count: StockCount) -> Item {
    return Item.create {
        set: {
            code: code
            name: name
            count: count
        }
    }
}
"#
            .to_string(),
        ),
        (
            "domains/pantry/actions/new_export.jadpo",
            r#"use pantry/entities { ItemExport }

action new_export() -> ItemExport {
    return ItemExport.create {
        set: {
            file: none
            items: none
            problems: none
        }
    }
}
"#
            .to_string(),
        ),
        (
            "domains/pantry/actions/exported.jadpo",
            r#"use pantry/entities { ItemExport, ItemExportNotFound }

action exported(summary: ExportSummary, export_id: ItemExport.id) fails ItemExportNotFound -> Unit {
    attempt ItemExport.update(export_id) {
        set: {
            file: summary.file
            items: summary.items
            problems: summary.items_with_problems
        }
    }
}
"#
            .to_string(),
        ),
        (
            "domains/pantry/actions/export_sheet.jadpo",
            export(
                "export_sheet",
                &format!(
                    "csv.write(
        exports,
        name: \"items.csv\",
        format: CsvFormat.spreadsheet,
        from: items_by_code(page),
        each: {LINE},
        finished: summary -> exported(summary, export_id),
        key: export_id
    )"
                ),
            ),
        ),
        (
            "domains/pantry/actions/export_tab.jadpo",
            export(
                "export_tab",
                &format!(
                    "tsv.write(
        exports,
        name: \"items.tsv\",
        format: CsvFormat.standard,
        from: items_by_code(page),
        each: {LINE},
        finished: summary -> exported(summary, export_id)
    )"
                ),
            ),
        ),
        (
            "domains/pantry/actions/export_lines.jadpo",
            export(
                "export_lines",
                &format!(
                    "json_lines.write(
        exports,
        name: \"items.jsonl\",
        from: items_by_code(page),
        each: {LINE},
        finished: summary -> exported(summary, export_id)
    )"
                ),
            ),
        ),
        (
            "domains/pantry/actions/export_json.jadpo",
            export(
                "export_json",
                &format!(
                    "json.write(
        exports,
        name: \"items.json\",
        from: items_by_code(page),
        each: {LINE},
        finished: summary -> exported(summary, export_id)
    )"
                ),
            ),
        ),
        (
            "domains/pantry/actions/progress.jadpo",
            "action progress(run: BatchRun) -> BatchProgress {\n    return batches.status(run)\n}\n"
                .to_string(),
        ),
        (
            "domains/pantry/actions/stop.jadpo",
            "action stop(run: BatchRun) -> Unit {\n    batches.cancel(run)\n}\n".to_string(),
        ),
    ]
}

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("file-exports-6.4b2-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files() {
        write(&root, path, &text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    root
}

/// When the test starts: 2026-10-20T09:00:00Z.
const T0: i64 = 1_792_486_800_000;

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

/// The stand a test runs on: the harness over the store, and the `exports`
/// store's in-memory fake.
struct Stand {
    harness: Harness<Crashable>,
    files: Rc<MemoryFiles>,
    crash_in: Rc<Cell<Option<u32>>>,
}

impl Stand {
    fn new(url: &str) -> Stand {
        let crash_in = Rc::new(Cell::new(None));
        let db = jadpo_host_native::postgres(url).expect("the store");
        let harness = Harness::new(Crashable {
            db,
            crash_in: Rc::clone(&crash_in),
        })
        .at(T0)
        .random_from(0);
        Stand {
            harness,
            files: Rc::new(MemoryFiles::new()),
            crash_in,
        }
    }

    fn host(&self) -> Faked<&Harness<Crashable>> {
        Faked::new(&self.harness).with("exports", Rc::clone(&self.files) as Rc<dyn Capability>)
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
        let mut ctx = self.harness.ctx_on(program, &host);
        let outcome = program.run(&mut ctx, id, args).await;
        let left = ctx.take_deferred();
        (outcome, left)
    }

    async fn rows(&self, sql: &str) -> Vec<Row> {
        self.harness.rows(sql).await
    }

    /// Adds the items, each its code, name and count.
    async fn add(&self, program: &Program, items: &[(&str, &str, i64)]) {
        for (code, name, count) in items {
            let (added, _) = self
                .call(
                    program,
                    "add_item",
                    vec![Value::text(code), Value::text(name), Value::Int(*count)],
                )
                .await;
            value(added);
        }
    }

    /// Starts `action`'s export of every item, `size` a page, with `note`
    /// in each row: the export's record's id, the run and the work left
    /// for after the response.
    async fn export(
        &self,
        program: &Program,
        action: &str,
        size: i64,
        note: Option<&str>,
    ) -> (Value, Value, Vec<Deferred>) {
        let (made, _) = self.call(program, "new_export", Vec::new()).await;
        let export_id = field(&value(made), "id");
        let (started, work) = self
            .call(
                program,
                action,
                vec![
                    export_id.clone(),
                    page(size),
                    note.map_or(Value::None, Value::text),
                ],
            )
            .await;
        (export_id, value(started), work)
    }

    /// The one file the store holds once an export has joined its pieces,
    /// with what its export's record kept: its `File`'s parts, the items
    /// and the problems.
    async fn written(&self) -> Written {
        let keys = self.files.keys();
        let [key] = keys.as_slice() else {
            panic!("one file, its pieces gone: {keys:?}");
        };
        let bytes = self.files.bytes(key).expect("its bytes");
        let rows = self
            .rows("SELECT \"file\", \"items\", \"problems\" FROM \"item_export\" WHERE \"file\" IS NOT NULL")
            .await;
        let [row] = rows.as_slice() else {
            panic!("one export with its file: {rows:?}");
        };
        let (file, items, problems) = match row.as_slice() {
            [
                SqlValue::Json(file),
                SqlValue::Int8(items),
                SqlValue::Int8(problems),
            ] => (
                serde_json::from_str::<serde_json::Value>(file).expect("the file's JSON"),
                *items,
                *problems,
            ),
            other => panic!("an export's row: {other:?}"),
        };
        assert_eq!(file["key"], serde_json::Value::String(key.clone()));
        assert_eq!(file["store"], "exports");
        // An `Int` with no range is a string in its JSON (area 7).
        assert_eq!(file["size"], serde_json::json!(bytes.len().to_string()));
        Written {
            text: String::from_utf8(bytes).expect("UTF-8"),
            name: file["name"].as_str().expect("a name").to_string(),
            file_type: file["type"].as_str().expect("a type").to_string(),
            items,
            problems,
        }
    }
}

struct Written {
    text: String,
    name: String,
    file_type: String,
    items: i64,
    problems: i64,
}

fn page(size: i64) -> Value {
    Value::record(
        "ItemPage",
        vec![
            (Rc::from("page_size"), Value::Int(size)),
            (Rc::from("after"), Value::None),
        ],
    )
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

/// Clears every run, problem and export, and the store's files.
async fn clear(stand: &Stand) {
    for table in [
        "jadpo_batch_problems",
        "jadpo_batch_cancels",
        "jadpo_batch_runs",
        "item_export",
        "item",
    ] {
        stand.rows(&format!("DELETE FROM \"{table}\"")).await;
    }
    stand.files.clear();
}

/// The spreadsheet's rows: a byte-order mark, the header by each field's
/// column, CRLF after every row; a cell holding a comma, a quote or a line
/// break quoted, a quote doubled; text a spreadsheet would read as a
/// formula escaped with `'`, numbers never.
const SHEET: &str = "\u{feff}Code,Item name,count,scaled,note\r\n\
A1,Apples,3,30000000000000,Weekly\r\n\
A2,\"Pears, conference\",4,40000000000000,Weekly\r\n\
A3,\"'=SUM(1,2)\",5,50000000000000,Weekly\r\n\
A4,\"Plums \"\"Victoria\"\"\",6,60000000000000,Weekly\r\n\
A5,\"Line\nbreak\",7,70000000000000,Weekly\r\n\
A7,'+plus,2,20000000000000,Weekly\r\n\
A8,'@at,0,0,Weekly\r\n";

const ITEMS: &[(&str, &str, i64)] = &[
    ("A1", "Apples", 3),
    ("A2", "Pears, conference", 4),
    ("A3", "=SUM(1,2)", 5),
    ("A4", "Plums \"Victoria\"", 6),
    ("A5", "Line\nbreak", 7),
    ("A6", "-minus", 1_000_000),
    ("A7", "+plus", 2),
    ("A8", "@at", 0),
];

/// A query's records written as a CSV for a spreadsheet, three a page:
/// each page read in its own transaction, its rows put in the store once
/// that transaction has closed, the pieces joined into one file named as
/// `name:` says, which `finished:` keeps; an item whose row overflows a
/// problem by its place in the walk, its row not written.
#[test]
fn a_csv_export_writes_each_page_and_finished_keeps_the_file() {
    let Some(loaded) = load("csv") else {
        return;
    };
    let program = &loaded.program;
    let stand = Stand::new(&loaded.url);
    Runtime::new().expect("runtime").block_on(async {
        stand.add(program, ITEMS).await;
        let (_, run, work) = stand
            .export(program, "export_sheet", 3, Some("Weekly"))
            .await;
        assert!(
            stand.files.keys().is_empty(),
            "nothing is put in the request"
        );
        let ran = run_deferred(program, &stand.host(), work).await;
        assert!(
            ran.iter()
                .all(|outcome| matches!(outcome, Outcome::Value(_))),
            "{ran:?}"
        );
        let written = stand.written().await;
        assert_eq!(written.text, SHEET);
        assert_eq!(written.name, "items.csv");
        assert_eq!(written.file_type, "text/csv");
        assert_eq!((written.items, written.problems), (8, 1));
        assert_eq!(
            problems(&stand).await,
            [(6, String::new(), "int_overflow".to_string())]
        );
        assert_eq!(run_row(&stand).await, ("finished".into(), None, 8, 1));
        let (progress, _) = stand.call(program, "progress", vec![run]).await;
        let progress = value(progress);
        assert_eq!(field(&progress, "items_done"), Value::Int(8));
        assert_eq!(field(&progress, "problems"), Value::Int(1));
        // Never a value in what the run kept.
        let calls = stand
            .rows("SELECT \"failure\" FROM \"jadpo_batch_runs\" WHERE \"failure\" IS NOT NULL")
            .await;
        assert!(calls.is_empty(), "{calls:?}");
    });
}

/// TSV is CSV with a tab between cells, and `CsvFormat.standard` writes
/// no mark and escapes nothing; JSON Lines writes each row's JSON on a
/// line of its own, by the fields' names, as a response's body is (an
/// `Int` with no range a string), and `json.write` one JSON list.
#[test]
fn tsv_json_lines_and_json_exports_write_the_same_rows() {
    let Some(loaded) = load("formats") else {
        return;
    };
    let program = &loaded.program;
    let stand = Stand::new(&loaded.url);
    let items: &[(&str, &str, i64)] = &[("T1", "Tea\tgreen", 2), ("T2", "-Toffee", 1)];
    Runtime::new().expect("runtime").block_on(async {
        let cases: [(&str, &str, &str, &str); 3] = [
            (
                "export_tab",
                "Code\tItem name\tcount\tscaled\tnote\r\nT1\t\"Tea\tgreen\"\t2\t20000000000000\t\r\nT2\t-Toffee\t1\t10000000000000\t\r\n",
                "items.tsv",
                "text/tab-separated-values",
            ),
            (
                "export_lines",
                "{\"code\":\"T1\",\"name\":\"Tea\\tgreen\",\"count\":2,\"scaled\":\"20000000000000\",\"note\":null}\n{\"code\":\"T2\",\"name\":\"-Toffee\",\"count\":1,\"scaled\":\"10000000000000\",\"note\":null}\n",
                "items.jsonl",
                "application/jsonl",
            ),
            (
                "export_json",
                "[{\"code\":\"T1\",\"name\":\"Tea\\tgreen\",\"count\":2,\"scaled\":\"20000000000000\",\"note\":null},{\"code\":\"T2\",\"name\":\"-Toffee\",\"count\":1,\"scaled\":\"10000000000000\",\"note\":null}]",
                "items.json",
                "application/json",
            ),
        ];
        for (action, text, name, file_type) in cases {
            clear(&stand).await;
            stand.add(program, items).await;
            let (_, _, work) = stand.export(program, action, 1, None).await;
            run_deferred(program, &stand.host(), work).await;
            let written = stand.written().await;
            assert_eq!(written.text, text, "{action}");
            assert_eq!(
                (written.name.as_str(), written.file_type.as_str()),
                (name, file_type),
                "{action}"
            );
            assert_eq!((written.items, written.problems), (2, 0), "{action}");
        }
    });
}

/// A page whose commit dies keeps nothing it did, though its piece was
/// put: the run resumes from the position the last page committed and
/// puts that piece again, so the file holds every row once.
#[test]
fn an_export_resumes_from_its_last_committed_page_with_no_row_written_twice() {
    let Some(loaded) = load("resume") else {
        return;
    };
    let program = &loaded.program;
    let stand = Stand::new(&loaded.url);
    let items: Vec<(String, String)> = (0..10)
        .map(|n| (format!("B{n:02}"), format!("Tin {n}")))
        .collect();
    Runtime::new().expect("runtime").block_on(async {
        let listed: Vec<(&str, &str, i64)> = items
            .iter()
            .map(|(code, name)| (code.as_str(), name.as_str(), 1))
            .collect();
        stand.add(program, &listed).await;
        let (_, _, work) = stand.export(program, "export_tab", 3, None).await;
        // The second page's piece is put, and its commit dies.
        stand.crash_in.set(Some(2));
        let start = starts(&work).remove(0);
        let (ran, _) = jadpo_runtime::batches::run(program, &stand.host(), &start).await;
        assert!(matches!(ran, Ran::Paused(_)), "{ran:?}");
        // The query's count, taken as its first page was read (chunk
        // 6.4b3).
        let counted = stand
            .rows("SELECT \"estimated_total\" FROM \"jadpo_batch_runs\"")
            .await;
        assert_eq!(counted, [vec![SqlValue::Int8(10)]]);
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
        let mut expected = String::from("Code\tItem name\tcount\tscaled\tnote\r\n");
        for (code, name) in &items {
            expected.push_str(&format!("{code}\t{name}\t1\t10000000000000\t\r\n"));
        }
        assert_eq!(stand.written().await.text, expected);
        assert_eq!(run_row(&stand).await, ("finished".into(), None, 10, 0));
    });
}

/// An export of nothing writes its header, or an empty list, or nothing;
/// a cancelled export runs no `finished:`, keeps no file and leaves no
/// piece in the store.
#[test]
fn an_empty_export_writes_its_frame_and_a_cancelled_one_leaves_nothing() {
    let Some(loaded) = load("empty") else {
        return;
    };
    let program = &loaded.program;
    let stand = Stand::new(&loaded.url);
    Runtime::new().expect("runtime").block_on(async {
        for (action, text) in [
            (
                "export_sheet",
                "\u{feff}Code,Item name,count,scaled,note\r\n",
            ),
            ("export_lines", ""),
            ("export_json", "[]"),
        ] {
            clear(&stand).await;
            let (_, _, work) = stand.export(program, action, 5, None).await;
            run_deferred(program, &stand.host(), work).await;
            let written = stand.written().await;
            assert_eq!(written.text, text, "{action}");
            assert_eq!((written.items, written.problems), (0, 0), "{action}");
        }

        clear(&stand).await;
        let listed: Vec<(String, String)> = (0..6)
            .map(|n| (format!("C{n}"), format!("Can {n}")))
            .collect();
        let listed: Vec<(&str, &str, i64)> = listed
            .iter()
            .map(|(code, name)| (code.as_str(), name.as_str(), 1))
            .collect();
        stand.add(program, &listed).await;
        let (_, run, work) = stand.export(program, "export_sheet", 2, None).await;
        // One page, then the process stops; a cancel is asked for.
        stand.crash_in.set(Some(2));
        let start = starts(&work).remove(0);
        let (ran, _) = jadpo_runtime::batches::run(program, &stand.host(), &start).await;
        assert!(matches!(ran, Ran::Paused(_)), "{ran:?}");
        let (stopped, _) = stand.call(program, "stop", vec![run]).await;
        value(stopped);
        stand
            .harness
            .set_now(T0 + jadpo_runtime::lease::LONGEST_MS + 1_000);
        let (resumed, _) = jadpo_runtime::batches::resume(
            program,
            &stand.host(),
            T0 + jadpo_runtime::lease::LONGEST_MS + 1_000,
        )
        .await;
        assert_eq!(resumed[0].1, Ran::Cancelled);
        assert_eq!(run_row(&stand).await.0, "cancelled");
        assert!(stand.files.keys().is_empty(), "{:?}", stand.files.keys());
        let kept = stand
            .rows("SELECT count(*) FROM \"item_export\" WHERE \"file\" IS NOT NULL")
            .await;
        assert_eq!(kept, [vec![SqlValue::Int8(0)]]);
    });
}
