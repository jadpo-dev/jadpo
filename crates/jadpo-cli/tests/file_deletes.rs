//! Deleting a file's bytes against live PostgreSQL (area 16, "Deleting and
//! erasing" and "Writing a file your app makes: `put`"; chunk B3): a
//! bakery keeps its suppliers' price sheets, each a `File` in the
//! `sheets` store, and the bytes go once no record holds them:
//!
//! - deleting a sheet, clearing its field or replacing its file deletes
//!   the bytes it held once the change commits, through the generated
//!   subscriber `files sheets`, and never before;
//! - a supplier's sheets, which cascade from it, go with it;
//! - bytes two sheets hold stay until the last of them lets go;
//! - bytes a put kept that no record ever holds are kept for a day, then
//!   deleted by the tick;
//! - a commit whose delivery was lost leaves its event in the outbox, and
//!   the tick's sweep deletes the bytes;
//! - a file whose bytes are gone can't be kept in a record again.
//!
//! Its tests run interpreted through `jadpo test` here, and compiled with
//! the interpreter's records, the bytes each store of files keeps
//! among them, in the gate's compiled step (`compiled.rs`, "a file's
//! bytes deleted"). The rows and bytes are read here after runs through
//! the interpreter.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use jadpo_cli::{Output, run};
use jadpo_host_native::Runtime;
use jadpo_interp::Program;
use jadpo_runtime::capability::{Capability, Faked};
use jadpo_runtime::files::MemoryFiles;
use jadpo_runtime::runner::{Deferred, run_deferred};
use jadpo_runtime::schedules::tick;
use jadpo_runtime::{Executor, Host, Outcome, SqlValue};
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

/// The bakery's suppliers and their price sheets, an action for each way
/// a sheet's bytes are let go of, and the tests every executor runs.
pub(crate) fn files() -> Vec<(String, String)> {
    let action = |name: &str, uses: &str, signature: &str, body: &str| {
        (
            format!("domains/bakery/actions/{name}.jadpo"),
            format!("{uses}\n\n/// {name}.\naction {name}{signature} {{\n{body}}}\n"),
        )
    };
    let put = "    var sheet: File = attempt sheets.put(encoding.to_utf8(sheet_text), FileType.csv) {\n        storage_full: SheetNotKept\n    }\n";
    let mut files: Vec<(String, String)> = vec![
        (
            "app.jadpo".to_string(),
            "application {\n    connections: {\n        sheets: {\n            driver: Driver.disk\n            path: \"/var/lib/bakery/sheets\"\n        }\n    }\n}\n".to_string(),
        ),
        (
            "domains/bakery/types/sheet_text.jadpo".to_string(),
            "/// A price sheet as a supplier sends it.\ntype SheetText = Text {\n    min_length: 1\n    max_length: 100000\n}\n".to_string(),
        ),
        (
            "domains/bakery/types/supplier_name.jadpo".to_string(),
            "/// Who sends price sheets.\ntype SupplierName = Text {\n    min_length: 1\n    max_length: 80\n}\n".to_string(),
        ),
        (
            "domains/bakery/failures/sheet_not_kept.jadpo".to_string(),
            "/// The sheets store had no room for a sheet.\ntype SheetNotKept = Failure {\n    kind: FailureKind.unavailable\n    message: \"There was no room to keep the sheet.\"\n}\n".to_string(),
        ),
        (
            "domains/bakery/entities/supplier.jadpo".to_string(),
            "use bakery/types { SupplierName }\n\n/// Someone who sends the bakery price sheets.\nentity Supplier {\n    id: Uuid\n    name: SupplierName\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n        delete: [Access.public]\n    }\n}\n".to_string(),
        ),
        (
            "domains/bakery/entities/price_sheet.jadpo".to_string(),
            "use bakery/entities { Supplier }\n\n/// A supplier's price sheet, kept as it was sent, which goes with\n/// its supplier.\nentity PriceSheet {\n    id: Uuid\n    supplier_id: Supplier.id {\n        on_delete: OnDelete.cascade\n    }\n    sheet: File? {\n        store: sheets\n        max_size: 1mb\n        types: [FileType.csv]\n    }\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n        update: [Access.public]\n        delete: [Access.public]\n    }\n}\n".to_string(),
        ),
    ];
    files.extend([
        action(
            "add_supplier",
            "use bakery/entities { Supplier }\nuse bakery/types { SupplierName }",
            "(name: SupplierName) -> Supplier",
            "    return Supplier.create {\n        set: {\n            name: name\n        }\n    }\n",
        ),
        (
            "domains/bakery/actions/keep_sheet.jadpo".to_string(),
            format!("use app {{ sheets }}\nuse bakery/entities {{ PriceSheet, Supplier, SupplierNotFound }}\nuse bakery/types {{ SheetText }}\nuse bakery/failures {{ SheetNotKept }}\n\n/// Keeps a supplier's sheet: one whose supplier has gone is put but\n/// held by no record.\ndecline step.late_failure: \"A sheet no record holds is deleted after a day\"\naction keep_sheet(supplier_id: Supplier.id, sheet_text: SheetText) fails SheetNotKept, SupplierNotFound -> PriceSheet {{\n{put}    return attempt PriceSheet.create {{\n        set: {{\n            supplier_id: supplier_id\n            sheet: sheet\n        }}\n    }}\n}}\n"),
        ),
        action(
            "forget_sheet",
            "use bakery/entities { PriceSheet, PriceSheetNotFound }",
            "(sheet_id: PriceSheet.id) fails PriceSheetNotFound -> Unit",
            "    attempt PriceSheet.delete(sheet_id)\n",
        ),
        action(
            "clear_sheet",
            "use bakery/entities { PriceSheet, PriceSheetNotFound }",
            "(sheet_id: PriceSheet.id) fails PriceSheetNotFound -> PriceSheet",
            "    return attempt PriceSheet.update(sheet_id) {\n        set: {\n            sheet: none\n        }\n    }\n",
        ),
        action(
            "replace_sheet",
            "use app { sheets }\nuse bakery/entities { PriceSheet, PriceSheetNotFound }\nuse bakery/types { SheetText }\nuse bakery/failures { SheetNotKept }",
            "(sheet_id: PriceSheet.id, sheet_text: SheetText) fails SheetNotKept, PriceSheetNotFound -> PriceSheet",
            &format!("    var kept: PriceSheet = attempt PriceSheet.get(sheet_id)\n{put}    return PriceSheet.update(kept) {{\n        set: {{\n            sheet: sheet\n        }}\n    }}\n"),
        ),
        action(
            "copy_sheet",
            "use bakery/entities { PriceSheet }",
            "(from: PriceSheet) -> PriceSheet",
            "    return PriceSheet.create {\n        set: {\n            supplier_id: from.supplier_id\n            sheet: from.sheet\n        }\n    }\n",
        ),
        action(
            "drop_supplier",
            "use bakery/entities { Supplier, SupplierNotFound }",
            "(supplier_id: Supplier.id) fails SupplierNotFound -> Unit",
            "    attempt Supplier.delete(supplier_id)\n",
        ),
    ]);
    files.push((
        "domains/bakery/actions/keep_sheet.test.jadpo".to_string(),
        TESTS.to_string(),
    ));
    files
}

/// What every executor runs: each way a sheet's bytes go, the records and
/// bytes left compared both ways.
const TESTS: &str = r#"use bakery/actions { add_supplier, clear_sheet, copy_sheet, drop_supplier, forget_sheet, replace_sheet }
use bakery/entities { PriceSheet, PriceSheetNotFound, Supplier, SupplierNotFound }

test "deleting a sheet deletes its bytes" {
    var millers: Supplier = add_supplier("Millers")
    var kept: PriceSheet = attempt keep_sheet(millers.id, "flour,1.20")
    attempt forget_sheet(kept.id)
    assert PriceSheet.get(kept.id) fails PriceSheetNotFound
}

test "clearing a sheet deletes its bytes" {
    var millers: Supplier = add_supplier("Millers")
    var kept: PriceSheet = attempt keep_sheet(millers.id, "flour,1.20")
    var cleared: PriceSheet = attempt clear_sheet(kept.id)
    assert cleared.sheet == none
}

test "replacing a sheet deletes the bytes it held" {
    var millers: Supplier = add_supplier("Millers")
    var kept: PriceSheet = attempt keep_sheet(millers.id, "flour,1.20")
    var replaced: PriceSheet = attempt replace_sheet(kept.id, "rye,2.40")
    assert replaced.id == kept.id
}

test "bytes two sheets hold stay until the last lets go" {
    var millers: Supplier = add_supplier("Millers")
    var kept: PriceSheet = attempt keep_sheet(millers.id, "flour,1.20")
    var copied: PriceSheet = copy_sheet(kept)
    attempt forget_sheet(kept.id)
    var still: PriceSheet = attempt PriceSheet.get(copied.id)
    assert still.id == copied.id
}

test "a supplier's sheets go with them" {
    var millers: Supplier = add_supplier("Millers")
    var first: PriceSheet = attempt keep_sheet(millers.id, "flour,1.20")
    var second: PriceSheet = attempt keep_sheet(millers.id, "rye,2.40")
    attempt drop_supplier(millers.id)
    assert PriceSheet.get(first.id) fails PriceSheetNotFound
    assert PriceSheet.get(second.id) fails PriceSheetNotFound
}

test "bytes no record holds are kept for a day" {
    var millers: Supplier = add_supplier("Millers")
    attempt drop_supplier(millers.id)
    assert keep_sheet(millers.id, "flour,1.20") fails SupplierNotFound
}
"#;

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("file-deletes-b3-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files() {
        write(&root, &path, &text);
    }
    for args in [&["migrate", "new", "initial"][..], &["records", "update"]] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{args:?}: {}{}", done.stdout, done.stderr);
    }
    root
}

#[test]
fn file_deletes_pass_their_tests() {
    let root = project("tests");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    if !jadpo_test::live() {
        return;
    }
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("6 passed"), "{}", tested.stdout);
}

/// The migration that starts a store's record of held files makes the
/// table, the function counting a file's holders and a trigger on each
/// table with a `File` column.
#[test]
fn the_migration_counts_each_files_holders() {
    let root = project("migration");
    let migrations = root.join("database/migrations");
    let folder = fs::read_dir(&migrations)
        .expect("migrations")
        .filter_map(Result::ok)
        .next()
        .expect("one migration")
        .path();
    let sql = fs::read_to_string(folder.join("postgres.sql")).expect("its sql");
    assert!(sql.contains("CREATE TABLE \"jadpo_files\""), "{sql}");
    assert!(
        sql.contains("CREATE OR REPLACE FUNCTION \"jadpo_files_held\"()"),
        "{sql}"
    );
    assert!(
        sql.contains("CREATE TRIGGER \"jadpo_files\" AFTER INSERT OR UPDATE OF \"sheet\" OR DELETE ON \"price_sheet\" FOR EACH ROW EXECUTE FUNCTION \"jadpo_files_held\"('sheet')"),
        "{sql}"
    );
    // Neither the supplier's table, which holds no file, nor a new
    // table's stored rows, of which it has none, are counted.
    assert!(!sql.contains("ON \"supplier\""), "{sql}");
    assert!(!sql.contains("count(*)"), "{sql}");
}

/// 2026-10-19T09:00:00Z.
const T0: i64 = 1_792_400_400_000;
const MINUTE: i64 = 60_000;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;

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

/// The harness, the store of files in memory and a host giving it.
struct Bakery {
    harness: Harness,
    files: Rc<MemoryFiles>,
}

impl Bakery {
    fn new(url: &str) -> Bakery {
        Bakery {
            harness: Harness::postgres(url).at(T0).random_from(0),
            files: Rc::new(MemoryFiles::new()),
        }
    }

    fn host(&self) -> Faked<&Harness> {
        Faked::new(&self.harness).with("sheets", Rc::clone(&self.files) as Rc<dyn Capability>)
    }

    /// The keys the store of files holds bytes under.
    fn kept(&self) -> Vec<String> {
        self.files.keys()
    }

    /// Each file's holders in the record of held files, by key.
    async fn holders(&self) -> Vec<(String, i32)> {
        self.harness
            .rows("SELECT \"key\", \"holders\" FROM \"jadpo_files\" ORDER BY \"key\"")
            .await
            .into_iter()
            .map(|row| match row.as_slice() {
                [SqlValue::Text(key), SqlValue::Int4(holders)] => (key.clone(), *holders),
                other => panic!("not a file: {other:?}"),
            })
            .collect()
    }
}

/// Runs `action` with `args` on `host`, giving what it gave and the work
/// it left for after its response.
async fn call(
    program: &Program,
    bakery: &Bakery,
    host: &dyn Host,
    action: &str,
    args: Vec<Value>,
) -> (Outcome, Vec<Deferred>) {
    let id = program.plan().find(action).expect("the action");
    let mut ctx = bakery.harness.ctx_on(program, host);
    let outcome = program.run(&mut ctx, id, args).await;
    (outcome, ctx.take_deferred())
}

/// What `action` gave, which must be a value, once the work it left has
/// run.
async fn done(
    program: &Program,
    bakery: &Bakery,
    host: &dyn Host,
    action: &str,
    args: Vec<Value>,
) -> Value {
    let (outcome, work) = call(program, bakery, host, action, args).await;
    run_deferred(program, host, work).await;
    match outcome {
        Outcome::Value(value) => value,
        other => panic!("{action}: expected a value, got {other:?}"),
    }
}

fn field(value: &Value, name: &str) -> Value {
    match value {
        Value::Record(record) => record.get(name).cloned().expect(name),
        other => panic!("not a record: {other:?}"),
    }
}

fn key_of(sheet: &Value) -> String {
    match field(&field(sheet, "sheet"), "key") {
        Value::Text(key) => key.to_string(),
        other => panic!("not a key: {other:?}"),
    }
}

/// Deleting, clearing and replacing each delete the bytes the sheet held
/// once the change commits, and a supplier's sheets go with it; bytes
/// another sheet holds stay until it lets go too. Each file's record says
/// its bytes are gone, and the bytes another holds, kept.
#[test]
fn bytes_go_once_no_record_holds_them() {
    let Some(loaded) = load("held") else {
        return;
    };
    let program = &loaded.program;
    let bakery = Bakery::new(&loaded.url);
    let host = bakery.host();
    Runtime::new().expect("runtime").block_on(async {
        let millers = done(program, &bakery, &host, "add_supplier", vec![Value::text("Millers")]).await;
        let millers_id = field(&millers, "id");
        let keep = |text: &str| {
            done(
                program,
                &bakery,
                &host,
                "keep_sheet",
                vec![millers_id.clone(), Value::text(text)],
            )
        };
        let deleted = keep("flour,1.20").await;
        let cleared = keep("rye,2.40").await;
        let replaced = keep("spelt,3.10").await;
        let shared = keep("oats,0.90").await;
        assert_eq!(bakery.kept().len(), 4);

        // Deleted: its delivery deletes the bytes, but only once the
        // delete has committed.
        let (outcome, work) = call(
            program,
            &bakery,
            &host,
            "forget_sheet",
            vec![field(&deleted, "id")],
        )
        .await;
        assert!(matches!(outcome, Outcome::Value(_)), "{outcome:?}");
        assert!(bakery.kept().contains(&key_of(&deleted)), "not before its delivery");
        assert!(!work.is_empty(), "the delete leaves its delivery");
        run_deferred(program, &host, work).await;
        assert!(!bakery.kept().contains(&key_of(&deleted)));

        done(program, &bakery, &host, "clear_sheet", vec![field(&cleared, "id")]).await;
        assert!(!bakery.kept().contains(&key_of(&cleared)));

        let renewed = done(
            program,
            &bakery,
            &host,
            "replace_sheet",
            vec![field(&replaced, "id"), Value::text("spelt,3.30")],
        )
        .await;
        assert!(!bakery.kept().contains(&key_of(&replaced)));
        assert!(bakery.kept().contains(&key_of(&renewed)));

        // Two sheets hold the same bytes: they stay until both let go.
        let copy = done(program, &bakery, &host, "copy_sheet", vec![shared.clone()]).await;
        assert_eq!(key_of(&copy), key_of(&shared));
        done(program, &bakery, &host, "forget_sheet", vec![field(&shared, "id")]).await;
        assert!(bakery.kept().contains(&key_of(&shared)), "the copy still holds them");
        done(program, &bakery, &host, "forget_sheet", vec![field(&copy, "id")]).await;
        assert!(!bakery.kept().contains(&key_of(&shared)));

        // The supplier's sheets cascade from it, and their bytes go too.
        assert_eq!(bakery.kept(), [key_of(&renewed)]);
        done(program, &bakery, &host, "drop_supplier", vec![millers_id.clone()]).await;
        assert!(bakery.kept().is_empty(), "{:?}", bakery.kept());

        let mut expected: Vec<(String, i32)> = [&deleted, &cleared, &replaced, &shared, &renewed]
            .into_iter()
            .map(|sheet| (key_of(sheet), -2))
            .collect();
        expected.sort();
        assert_eq!(bakery.holders().await, expected);
        // Each by its own event to the generated subscriber, delivered.
        let delivered = bakery
            .harness
            .count(
                "SELECT count(*) FROM \"jadpo_deliveries\" WHERE \"subscriber\" = 'files sheets' AND \"status\" = 'succeeded'",
            )
            .await;
        assert_eq!(delivered, 5);
    });
}

/// Bytes a put kept that no record holds are kept for a day, then the
/// tick deletes them; bytes a record holds the tick leaves.
#[test]
fn bytes_no_record_holds_go_after_a_day() {
    let Some(loaded) = load("unheld") else {
        return;
    };
    let program = &loaded.program;
    let bakery = Bakery::new(&loaded.url);
    let host = bakery.host();
    Runtime::new().expect("runtime").block_on(async {
        let millers = done(
            program,
            &bakery,
            &host,
            "add_supplier",
            vec![Value::text("Millers")],
        )
        .await;
        let held = done(
            program,
            &bakery,
            &host,
            "keep_sheet",
            vec![field(&millers, "id"), Value::text("flour,1.20")],
        )
        .await;
        // A put whose action then fails: no record holds its bytes.
        let gone = done(
            program,
            &bakery,
            &host,
            "add_supplier",
            vec![Value::text("Gone")],
        )
        .await;
        done(
            program,
            &bakery,
            &host,
            "drop_supplier",
            vec![field(&gone, "id")],
        )
        .await;
        let (failed, _) = call(
            program,
            &bakery,
            &host,
            "keep_sheet",
            vec![field(&gone, "id"), Value::text("rye,2.40")],
        )
        .await;
        assert!(matches!(failed, Outcome::Failed(_)), "{failed:?}");
        assert_eq!(bakery.kept().len(), 2);
        let unheld = bakery
            .kept()
            .into_iter()
            .find(|key| *key != key_of(&held))
            .expect("the unheld bytes");
        assert!(bakery.holders().await.contains(&(unheld.clone(), 0)));

        bakery.harness.set_now(T0 + 23 * HOUR);
        let young = tick(program, &host, T0 + 23 * HOUR).await;
        assert_eq!(young.files, 0, "{young:?}");
        assert_eq!(bakery.kept().len(), 2);

        bakery.harness.set_now(T0 + DAY + MINUTE);
        let day = tick(program, &host, T0 + DAY + MINUTE).await;
        assert!(day.problems.is_empty(), "{:?}", day.problems);
        assert_eq!(day.files, 1, "{day:?}");
        assert_eq!(bakery.kept(), [key_of(&held)]);
        assert!(bakery.holders().await.contains(&(unheld, -2)));
        assert!(bakery.holders().await.contains(&(key_of(&held), 1)));
    });
}

/// A commit whose delivery never ran, as a crash after it leaves it,
/// leaves its event in the outbox: the tick's sweep deletes the bytes.
/// A file whose bytes are gone can't be kept in a record again.
#[test]
fn a_lost_delivery_is_swept_and_gone_bytes_are_never_held_again() {
    let Some(loaded) = load("lost") else {
        return;
    };
    let program = &loaded.program;
    let bakery = Bakery::new(&loaded.url);
    let host = bakery.host();
    Runtime::new().expect("runtime").block_on(async {
        let millers = done(
            program,
            &bakery,
            &host,
            "add_supplier",
            vec![Value::text("Millers")],
        )
        .await;
        let kept = done(
            program,
            &bakery,
            &host,
            "keep_sheet",
            vec![field(&millers, "id"), Value::text("flour,1.20")],
        )
        .await;
        // The delete commits, and its delivery is dropped.
        let (outcome, work) = call(
            program,
            &bakery,
            &host,
            "forget_sheet",
            vec![field(&kept, "id")],
        )
        .await;
        assert!(matches!(outcome, Outcome::Value(_)), "{outcome:?}");
        drop(work);
        assert!(bakery.kept().contains(&key_of(&kept)));
        assert!(bakery.holders().await.contains(&(key_of(&kept), -1)));

        bakery.harness.set_now(T0 + MINUTE);
        let swept = tick(program, &host, T0 + MINUTE).await;
        assert!(swept.problems.is_empty(), "{:?}", swept.problems);
        assert!(bakery.kept().is_empty(), "{:?}", bakery.kept());
        assert!(bakery.holders().await.contains(&(key_of(&kept), -2)));

        // Its `File`, kept in a record again: refused as a fault, and
        // nothing written.
        let (copied, _) = call(program, &bakery, &host, "copy_sheet", vec![kept.clone()]).await;
        assert!(matches!(copied, Outcome::Fault(_)), "{copied:?}");
        let sheets = bakery
            .harness
            .count("SELECT count(*) FROM \"price_sheet\"")
            .await;
        assert_eq!(sheets, 0);
    });
}
