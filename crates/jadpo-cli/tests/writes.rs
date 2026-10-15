//! Writes run against live PostgreSQL (area 6, "The operations every
//! entity gets", "When a write is refused", "All-or-nothing actions and
//! concurrent changes" and "Counters"): through `jadpo test`, a unique rule
//! and a restricting reference refused as the failures their `conflict:`
//! arms name, a reference to a record that isn't there as its not-found
//! failure, an update and a delete by `id`, a patch keeping what it leaves
//! out (and a required field it sends as `none`), a counter and text
//! joined onto a stored field, and an action that handles a refusal from
//! an action it calls and carries on, keeping none of what the failed action wrote
//! (a savepoint per called action whose failure is handled), and a test
//! that reads a record with `get` and nothing else. Then,
//! through the interpreter with a second connection changing a record
//! mid-action, the whole action rerun after a concurrent change, its
//! first attempt's rows gone and its second's there once, and concurrent counters losing no increment. Last, two
//! requests counting one row at once, each on its own connection (area 6,
//! "Counters and other one-step updates", chunk 3.7): a counter that is
//! its action's first statement runs at read committed, so both commit at
//! their first attempt; a counter after a read runs at repeatable read, so
//! the second runs again; either way the count goes up by two.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::cell::{Cell, RefCell};
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use jadpo_cli::{Output, run};
use jadpo_host_native::{Postgres, Runtime, postgres};
use jadpo_interp::Program;
use jadpo_runtime::{
    Clock, Ctx, Database, Env, Event, Executor, Host, Isolation, LocalFuture, Outcome, Row,
    SqlValue, StoreError, Transaction,
};
use jadpo_term::Environment;
use jadpo_value::Value;

fn skipped() -> bool {
    let skip = std::env::var("JADPO_SKIP_POSTGRES").is_ok_and(|value| value == "1");
    if skip {
        println!("skipped: JADPO_SKIP_POSTGRES is set");
    }
    skip
}

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
        "domains/shop/types/name.jadpo",
        "/// A name.\ntype Name = Text {\n    max_length: 80\n}\n",
    ),
    (
        "domains/shop/types/count.jadpo",
        "/// A count.\ntype Count = Int {\n    min: 0\n}\n",
    ),
    (
        "domains/shop/entities/shelf.jadpo",
        "use shop/types { Count, Name }\n\n/// A shelf.\nentity Shelf {\n    id: Uuid\n    name: Name {\n        unique: true\n    }\n    visits: Count\n}\n",
    ),
    (
        "domains/shop/entities/item.jadpo",
        "use shop/entities { Shelf }\nuse shop/types { Name }\n\n/// An item on a shelf.\nentity Item {\n    id: Uuid\n    shelf_id: Shelf.id {\n        on_delete: OnDelete.restricted\n    }\n    label: Name\n    note: Name?\n    changed_at: Instant {\n        generated: Generated.on_change\n    }\n}\n",
    ),
    (
        "domains/shop/types/item_changes.jadpo",
        "use shop/entities { Item }\n\n/// Changes to an item.\ntype ItemChanges = Input {\n    pick: [Item.label, Item.note]\n    optional: all\n}\n",
    ),
    (
        "domains/shop/failures/shelf_taken.jadpo",
        "/// A shelf has that name.\ntype ShelfTaken = Failure {\n    kind: FailureKind.conflict\n}\n",
    ),
    (
        "domains/shop/failures/shelf_in_use.jadpo",
        "/// A shelf still holds items.\ntype ShelfInUse = Failure {\n    kind: FailureKind.conflict\n}\n",
    ),
    (
        "domains/shop/actions/add_shelf.jadpo",
        "use shop/entities { Shelf }\nuse shop/failures { ShelfTaken }\n\n/// Adds a shelf.\naction add_shelf(name: Shelf.name) fails ShelfTaken -> Shelf {\n    return attempt Shelf.create {\n        set: {\n            name: name\n            visits: 0\n        }\n        conflict: {\n            Shelf.name: ShelfTaken\n        }\n    }\n}\n",
    ),
    (
        "domains/shop/actions/add_item.jadpo",
        "use shop/entities { Item, Shelf, ShelfNotFound }\n\n/// Adds an item to a shelf, by its id.\naction add_item(shelf_id: Shelf.id, label: Item.label) fails ShelfNotFound -> Item {\n    return attempt Item.create {\n        set: {\n            shelf_id: shelf_id\n            label: label\n            note: none\n        }\n    }\n}\n",
    ),
    (
        "domains/shop/actions/rename_shelf.jadpo",
        "use shop/entities { Shelf, ShelfNotFound }\nuse shop/failures { ShelfTaken }\n\n/// Renames a shelf.\naction rename_shelf(shelf_id: Shelf.id, name: Shelf.name) fails ShelfNotFound, ShelfTaken -> Shelf {\n    var shelf: Shelf = attempt Shelf.get(shelf_id)\n    return attempt Shelf.update(shelf) {\n        set: {\n            name: name\n        }\n        conflict: {\n            Shelf.name: ShelfTaken\n        }\n    }\n}\n",
    ),
    (
        "domains/shop/actions/visit.jadpo",
        "use shop/entities { Shelf, ShelfNotFound }\nuse shop/types { Count }\n\n/// Counts a visit.\naction visit(shelf_id: Shelf.id) fails ShelfNotFound -> Shelf {\n    return attempt Shelf.update(shelf_id) {\n        set: {\n            visits: Count(Shelf.visits + 1)\n        }\n    }\n}\n",
    ),
    (
        "domains/shop/actions/edit_item.jadpo",
        "use shop/entities { Item }\nuse shop/types { ItemChanges }\n\n/// Edits an item.\naction edit_item(item: Item, changes: ItemChanges) -> Item {\n    return Item.update(item) {\n        patch: changes\n    }\n}\n",
    ),
    (
        "domains/shop/actions/remove_shelf.jadpo",
        "use shop/entities { Item, Shelf, ShelfNotFound }\nuse shop/failures { ShelfInUse }\n\n/// Removes a shelf.\naction remove_shelf(shelf_id: Shelf.id) fails ShelfNotFound, ShelfInUse -> Unit {\n    attempt Shelf.delete(shelf_id) {\n        conflict: {\n            Item.shelf_id: ShelfInUse\n        }\n    }\n}\n",
    ),
    (
        "domains/shop/actions/shelf_named.jadpo",
        "use shop/entities { Shelf }\nuse shop/failures { ShelfTaken }\nuse shop/actions { add_shelf }\n\n/// A new shelf with a name, or one with the other name when the first is taken.\naction shelf_named(name: Shelf.name, other: Shelf.name) fails ShelfTaken -> Shelf {\n    var taken: Bool = match add_shelf(name) {\n        success -> false\n        ShelfTaken -> true\n    }\n    if (taken) {\n        var second: Shelf = attempt add_shelf(other)\n        return second\n    }\n    var first: Shelf = attempt Shelf.create {\n        set: {\n            name: other\n            visits: 0\n        }\n        conflict: {\n            Shelf.name: ShelfTaken\n        }\n    }\n    return first\n}\n",
    ),
    (
        "domains/shop/failures/too_many.jadpo",
        "/// Too many shelves.\ntype TooMany = Failure {\n    kind: FailureKind.conflict\n}\n",
    ),
    (
        "domains/shop/actions/add_then_refuse.jadpo",
        "use shop/entities { Shelf }\nuse shop/failures { ShelfTaken, TooMany }\n\n/// Adds a shelf, then refuses.\naction add_then_refuse(name: Shelf.name) fails ShelfTaken, TooMany -> Shelf {\n    var made: Shelf = attempt Shelf.create {\n        set: {\n            name: name\n            visits: 0\n        }\n        conflict: {\n            Shelf.name: ShelfTaken\n        }\n    }\n    if (made.visits == 0) {\n        reject TooMany\n    }\n    return made\n}\n",
    ),
    (
        "domains/shop/actions/keep_then_try.jadpo",
        "use shop/entities { Shelf }\nuse shop/failures { ShelfTaken, TooMany }\nuse shop/actions { add_shelf, add_then_refuse }\n\n/// Adds a shelf, then handles a called action that writes and fails, and one the store refuses.\naction keep_then_try(kept: Shelf.name, tried: Shelf.name) fails ShelfTaken -> Shelf {\n    var first: Shelf = attempt add_shelf(kept)\n    var refused: Bool = match add_then_refuse(tried) {\n        success -> false\n        TooMany -> true\n        ShelfTaken -> false\n    }\n    var taken: Bool = match add_shelf(kept) {\n        success -> false\n        ShelfTaken -> true\n    }\n    if (refused and taken) {\n        return first\n    }\n    reject ShelfTaken\n}\n",
    ),
    (
        "domains/shop/entities/log.jadpo",
        "/// A running log.\nentity Log {\n    id: Uuid\n    text: Text\n}\n",
    ),
    (
        "domains/shop/actions/start_log.jadpo",
        "use shop/entities { Log }\n\n/// Starts a log.\naction start_log(text: Log.text) -> Log {\n    return Log.create {\n        set: {\n            text: text\n        }\n    }\n}\n",
    ),
    (
        "domains/shop/actions/append.jadpo",
        "use shop/entities { Log, LogNotFound }\n\n/// Adds to a log in one step.\naction append(log_id: Log.id, more: Log.text) fails LogNotFound -> Log {\n    return attempt Log.update(log_id) {\n        set: {\n            text: Log.text + more\n        }\n    }\n}\n",
    ),
    (
        "domains/shop/actions/tally.jadpo",
        "use shop/entities { Shelf, ShelfNotFound }\nuse shop/types { Count }\n\n/// Counts a visit to a shelf it reads first.\naction tally(shelf_id: Shelf.id) fails ShelfNotFound -> Shelf {\n    var shelf: Shelf = attempt Shelf.get(shelf_id)\n    return attempt Shelf.update(shelf.id) {\n        set: {\n            visits: Count(Shelf.visits + 1)\n        }\n    }\n}\n",
    ),
    (
        "domains/shop/actions/restock.jadpo",
        "use shop/entities { Item, Shelf, ShelfNotFound }\nuse shop/types { Count }\n\n/// Adds an item named after its shelf, and counts the visit.\naction restock(shelf_id: Shelf.id) fails ShelfNotFound -> Item {\n    var shelf: Shelf = attempt Shelf.get(shelf_id)\n    var item: Item = Item.create {\n        set: {\n            shelf_id: shelf.id\n            label: shelf.name\n            note: none\n        }\n    }\n    var counted: Shelf = Shelf.update(shelf) {\n        set: {\n            visits: Count(shelf.visits + 1)\n        }\n    }\n    return Item.update(item) {\n        set: {\n            note: counted.name\n        }\n    }\n}\n",
    ),
    (
        "domains/shop/actions/recount.jadpo",
        "use shop/entities { Shelf, ShelfNotFound }\nuse shop/actions { tally }\n\n/// Counts a visit through an action that reads the shelf first.\naction recount(shelf_id: Shelf.id) fails ShelfNotFound -> Shelf {\n    return attempt tally(shelf_id)\n}\n",
    ),
    (
        "domains/shop/actions/revisit.jadpo",
        "use shop/entities { Shelf, ShelfNotFound }\nuse shop/actions { visit }\n\n/// Counts two visits through an action that reads nothing.\naction revisit(shelf_id: Shelf.id) fails ShelfNotFound -> Shelf {\n    attempt visit(shelf_id)\n    return attempt visit(shelf_id)\n}\n",
    ),
];

const TESTS: &str = "use shop/entities { Item, Log, Shelf, ShelfNotFound }
use shop/types { ItemChanges }
use shop/failures { ShelfInUse, ShelfTaken }
use shop/actions { add_item, append, edit_item, keep_then_try, remove_shelf, rename_shelf, shelf_named, start_log, visit }

test \"a unique rule is a conflict\" {
    var a: Shelf = attempt add_shelf(\"A\")
    assert add_shelf(\"A\") fails ShelfTaken
    var b: Shelf = attempt add_shelf(\"B\")
    assert rename_shelf(b.id, \"A\") fails ShelfTaken
    var renamed: Shelf = attempt rename_shelf(b.id, \"C\")
    assert renamed.name == \"C\"
    assert a.name == \"A\"
}

test \"a counter adds to what is stored\" {
    var a: Shelf = attempt add_shelf(\"A\")
    var once: Shelf = attempt visit(a.id)
    assert once.visits == 1
    var twice: Shelf = attempt visit(a.id)
    assert twice.visits == 2
    assert visit(\"0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e\") fails ShelfNotFound
    var log: Log = start_log(\"a\")
    var joined: Log = attempt append(log.id, \"b\")
    assert joined.text == \"ab\"
}

test \"a reference to a record that isn't there is not found\" {
    assert add_item(\"0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e\", \"Jam\") fails ShelfNotFound
}

test \"a patch keeps what it leaves out and clears what it sends as none\" {
    var a: Shelf = attempt add_shelf(\"A\")
    var item: Item = attempt add_item(a.id, \"Jam\")
    var noted: Item = edit_item(item, ItemChanges { note: \"sticky\" })
    assert noted.label == \"Jam\"
    assert noted.note == \"sticky\"
    var cleared: Item = edit_item(noted, ItemChanges { note: none })
    assert cleared.note == none
    var relabelled: Item = edit_item(cleared, ItemChanges { label: \"Marmalade\" })
    assert relabelled.note == none
    assert relabelled.label == \"Marmalade\"
    var kept: Item = edit_item(
        relabelled,
        ItemChanges {
            label: none
            note: \"kept\"
        }
    )
    assert kept.label == \"Marmalade\"
    assert kept.note == \"kept\"
}

test \"a restricted reference refuses a delete, and a delete by id finds nothing twice\" {
    var a: Shelf = attempt add_shelf(\"A\")
    var item: Item = attempt add_item(a.id, \"Jam\")
    assert item.shelf_id == a.id
    assert remove_shelf(a.id) fails ShelfInUse
    var b: Shelf = attempt add_shelf(\"B\")
    attempt remove_shelf(b.id)
    assert remove_shelf(b.id) fails ShelfNotFound
}

test \"an action handles a refusal from one it calls and carries on\" {
    var a: Shelf = attempt add_shelf(\"A\")
    var b: Shelf = attempt shelf_named(\"A\", \"A2\")
    assert b.name == \"A2\"
    assert a.name == \"A\"
}

test \"a test reads a record straight from its entity\" {
    assert Shelf.get(\"0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e\") fails ShelfNotFound
}

test \"a called action that fails and is handled keeps none of its writes\" {
    var kept: Shelf = attempt keep_then_try(\"K\", \"T\")
    assert kept.name == \"K\"
    var tried: Shelf = attempt add_shelf(\"T\")
    assert tried.name == \"T\"
    assert add_shelf(\"K\") fails ShelfTaken
}
";

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("writes-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    root
}

#[test]
fn writes_do_what_area_6_says() {
    if skipped() {
        return;
    }
    let root = project("semantics");
    write(&root, "domains/shop/actions/add_shelf.test.jadpo", TESTS);
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("8 passed"), "{}", tested.stdout);
}

// --- a concurrent change, forced -----------------------------------------------

/// What the store the action runs on saw, and the change made from another
/// connection after its first read.
struct State {
    store: Postgres,
    other: Postgres,
    /// The SQL another connection commits after the next transaction's
    /// first read, once.
    interfere: RefCell<Option<String>>,
    begun: Cell<u32>,
    /// The level each transaction began at.
    levels: RefCell<Vec<Isolation>>,
    /// The identity each `INSERT` sent, attempt by attempt.
    inserted: RefCell<Vec<SqlValue>>,
}

struct Interfering(Rc<State>);

impl Host for Interfering {
    fn database(&self, _store: &str) -> Option<&dyn Database> {
        Some(self)
    }

    fn deliver(&self, _events: Vec<Event>) -> LocalFuture<'_, ()> {
        Box::pin(async {})
    }

    fn random(&self, _bytes: &mut [u8]) -> bool {
        false
    }
}

impl Database for Interfering {
    fn begin_at(
        &self,
        isolation: Isolation,
    ) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>> {
        Box::pin(async move {
            self.0.begun.set(self.0.begun.get() + 1);
            self.0.levels.borrow_mut().push(isolation);
            let inner = self.0.store.begin_at(isolation).await?;
            Ok(Box::new(Watched {
                inner,
                state: Rc::clone(&self.0),
                read: false,
            }) as Box<dyn Transaction>)
        })
    }
}

struct Watched {
    inner: Box<dyn Transaction>,
    state: Rc<State>,
    read: bool,
}

impl Transaction for Watched {
    fn query<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
        Box::pin(async move {
            if sql.starts_with("INSERT") {
                self.state.inserted.borrow_mut().push(params[0].clone());
            }
            let rows = self.inner.query(sql, params).await;
            if !self.read {
                self.read = true;
                let change = self.state.interfere.borrow_mut().take();
                if let Some(change) = change {
                    // Another request commits a change to what this one
                    // just read.
                    let mut other = self.state.other.begin().await?;
                    other.batch(&change).await?;
                    other.commit().await?;
                }
            }
            rows
        })
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
        self.inner.commit()
    }

    fn rollback(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
        self.inner.rollback()
    }
}

/// Runs `action` with `args` on `host`, as one invocation.
async fn call(program: &Program, host: &dyn Host, action: &str, args: Vec<Value>) -> Outcome {
    let id = program.plan().find(action).expect("the action");
    let env = Env {
        seed: Some(7),
        clock: Clock::TEST,
    };
    let mut ctx = Ctx::new(program.places(), host, env);
    program.run(&mut ctx, id, args).await
}

fn value(outcome: Outcome) -> Value {
    match outcome {
        Outcome::Value(value) => value,
        other => panic!("expected a value, got {other:?}"),
    }
}

fn field(value: &Value, name: &str) -> Value {
    match value {
        Value::Record(record) => record.get(name).cloned().expect("the field"),
        other => panic!("not a record: {other:?}"),
    }
}

async fn rows(store: &Postgres, sql: &str) -> Vec<Row> {
    let mut transaction = store.begin().await.expect("begin");
    let rows = transaction.query(sql, &[]).await.expect("query");
    transaction.commit().await.expect("commit");
    rows
}

#[test]
fn a_concurrent_change_reruns_the_whole_action_and_its_writes_happen_once() {
    if skipped() {
        return;
    }
    let root = project("rerun");
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let state = Rc::new(State {
        store: postgres(&url).expect("store"),
        other: postgres(&url).expect("store"),
        interfere: RefCell::new(None),
        begun: Cell::new(0),
        levels: RefCell::new(Vec::new()),
        inserted: RefCell::new(Vec::new()),
    });
    let host = Interfering(Rc::clone(&state));
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let shelf = value(call(&program, &host, "add_shelf", vec![Value::text("Jams")]).await);
        let Value::Uuid(shelf_id) = field(&shelf, "id") else {
            panic!("a shelf id");
        };
        state.begun.set(0);
        state.inserted.borrow_mut().clear();
        // After the action reads the shelf, another request renames it and
        // counts ten visits: the action's update of the shelf it loaded
        // finds it changed, so the whole action runs again.
        *state.interfere.borrow_mut() = Some(format!(
            "UPDATE \"shelf\" SET \"name\" = 'Preserves', \"visits\" = \"visits\" + 10 WHERE \"id\" = '{shelf_id}'"
        ));
        let item = value(
            call(&program, &host, "restock", vec![Value::uuid(&shelf_id)]).await,
        );
        assert_eq!(state.begun.get(), 2, "the action ran twice");
        assert_eq!(
            field(&item, "label"),
            Value::text("Preserves"),
            "the rerun read the shelf afresh"
        );
        let inserted = state.inserted.borrow().clone();
        assert_eq!(inserted.len(), 2, "each attempt inserted its item: {inserted:?}");
        assert_ne!(inserted[0], inserted[1], "with a new identity each time");
        let items = rows(&state.store, "SELECT \"id\", \"label\" FROM \"item\"").await;
        assert_eq!(items.len(), 1, "the first attempt's item rolled back: {items:?}");
        assert_eq!(items[0][0], inserted[1], "the item kept is the second attempt's");
        let shelves = rows(&state.store, "SELECT \"visits\" FROM \"shelf\"").await;
        assert_eq!(shelves, vec![vec![SqlValue::Int8(11)]], "ten, then one more");

        // Two counters at once lose nothing: this one, whose row another
        // request counted after this one read it, runs again and adds to
        // that count.
        state.begun.set(0);
        *state.interfere.borrow_mut() = Some(format!(
            "UPDATE \"shelf\" SET \"visits\" = \"visits\" + 1 WHERE \"id\" = '{shelf_id}'"
        ));
        let counted = value(call(&program, &host, "tally", vec![Value::uuid(&shelf_id)]).await);
        assert_eq!(field(&counted, "visits"), Value::Int(13), "both visits counted");
        let shelves = rows(&state.store, "SELECT \"visits\" FROM \"shelf\"").await;
        assert_eq!(shelves, vec![vec![SqlValue::Int8(13)]]);
        assert_eq!(state.begun.get(), 2, "the counter ran again");
        assert_eq!(
            state.levels.borrow().last(),
            Some(&Isolation::RepeatableRead),
            "it reads the shelf first"
        );
    });
    drop(host);
    drop(state);
    databases.release(copy, false);
}

// --- two counters at once --------------------------------------------------------

/// Waits until it is polled again: the other invocation gets its turn.
async fn pause() {
    let mut paused = false;
    std::future::poll_fn(|cx| {
        if paused {
            std::task::Poll::Ready(())
        } else {
            paused = true;
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    })
    .await;
}

/// Runs `a` and `b` at once, on this thread, until both end.
async fn both<A: std::future::Future, B: std::future::Future>(
    a: A,
    b: B,
) -> (A::Output, B::Output) {
    let mut a = Box::pin(a);
    let mut b = Box::pin(b);
    let (mut from_a, mut from_b) = (None, None);
    std::future::poll_fn(|cx| {
        if from_a.is_none()
            && let std::task::Poll::Ready(value) = a.as_mut().poll(cx)
        {
            from_a = Some(value);
        }
        if from_b.is_none()
            && let std::task::Poll::Ready(value) = b.as_mut().poll(cx)
        {
            from_b = Some(value);
        }
        if from_a.is_some() && from_b.is_some() {
            std::task::Poll::Ready(())
        } else {
            std::task::Poll::Pending
        }
    })
    .await;
    (from_a.expect("a ended"), from_b.expect("b ended"))
}

/// What two racing requests share.
struct Race {
    /// A third connection, which watches for a request waiting on a lock.
    watcher: Postgres,
    /// The first request's update has run, so it holds the row's lock.
    updated: Cell<bool>,
    /// The first request has ended, whether or not it updated.
    ended: Cell<bool>,
    /// The first request has committed once; it holds nothing more.
    released: Cell<bool>,
    /// The first request saw the second waiting for the row before it
    /// committed.
    saw_waiting: Cell<bool>,
}

/// One of two requests racing on one row, on a connection of its own.
/// The first holds its transaction open after its update until the second
/// is seen waiting for the row's lock; the second sends nothing until the
/// first has updated. So the second's update always finds the row locked
/// by a transaction that then commits.
struct Racer {
    store: Postgres,
    race: Rc<Race>,
    first: bool,
    /// How many transactions it began: its attempts.
    begun: Cell<u32>,
    levels: RefCell<Vec<Isolation>>,
}

impl Host for Racer {
    fn database(&self, _store: &str) -> Option<&dyn Database> {
        Some(self)
    }

    fn deliver(&self, _events: Vec<Event>) -> LocalFuture<'_, ()> {
        Box::pin(async {})
    }

    fn random(&self, _bytes: &mut [u8]) -> bool {
        false
    }
}

impl Database for Racer {
    fn begin_at(
        &self,
        isolation: Isolation,
    ) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>> {
        Box::pin(async move {
            self.begun.set(self.begun.get() + 1);
            self.levels.borrow_mut().push(isolation);
            let inner = self.store.begin_at(isolation).await?;
            Ok(Box::new(Raced {
                inner,
                race: Rc::clone(&self.race),
                first: self.first,
            }) as Box<dyn Transaction>)
        })
    }
}

struct Raced {
    inner: Box<dyn Transaction>,
    race: Rc<Race>,
    first: bool,
}

impl Raced {
    /// The second request sends nothing before the first has updated.
    async fn turn(&self) {
        while !self.first && !self.race.updated.get() && !self.race.ended.get() {
            pause().await;
        }
    }
}

impl Transaction for Raced {
    fn query<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
        Box::pin(async move {
            self.turn().await;
            let rows = self.inner.query(sql, params).await;
            if self.first && sql.starts_with("UPDATE") {
                self.race.updated.set(true);
            }
            rows
        })
    }

    fn execute<'a>(
        &'a mut self,
        sql: &'a str,
        params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<u64, StoreError>> {
        Box::pin(async move {
            self.turn().await;
            self.inner.execute(sql, params).await
        })
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
        let Raced { inner, race, first } = *self;
        Box::pin(async move {
            if first && !race.released.get() {
                race.released.set(true);
                race.saw_waiting
                    .set(waiting_for_a_lock(&race.watcher).await);
            }
            inner.commit().await
        })
    }

    fn rollback(self: Box<Self>) -> LocalFuture<'static, Result<(), StoreError>> {
        self.inner.rollback()
    }
}

/// Whether, within ten seconds, another connection to this database is
/// waiting for a lock.
async fn waiting_for_a_lock(watcher: &Postgres) -> bool {
    let started = std::time::Instant::now();
    while started.elapsed() < std::time::Duration::from_secs(10) {
        let waiting = rows(
            watcher,
            "SELECT count(*) FROM pg_stat_activity WHERE datname = current_database() AND wait_event_type = 'Lock'",
        )
        .await;
        if waiting != vec![vec![SqlValue::Int8(0)]] {
            return true;
        }
        pause().await;
    }
    false
}

/// Runs `action` on `shelf_id` as two requests at once: their outcomes,
/// and each one's attempts and the levels its transactions began at.
async fn race(
    program: &Program,
    url: &str,
    action: &str,
    shelf_id: &str,
) -> ((Outcome, Outcome), Vec<(u32, Vec<Isolation>)>, bool) {
    let shared = Rc::new(Race {
        watcher: postgres(url).expect("store"),
        updated: Cell::new(false),
        ended: Cell::new(false),
        released: Cell::new(false),
        saw_waiting: Cell::new(false),
    });
    let racer = |first: bool| Racer {
        store: postgres(url).expect("store"),
        race: Rc::clone(&shared),
        first,
        begun: Cell::new(0),
        levels: RefCell::new(Vec::new()),
    };
    let (first, second) = (racer(true), racer(false));
    let outcomes = both(
        async {
            let outcome = call(program, &first, action, vec![Value::uuid(shelf_id)]).await;
            shared.ended.set(true);
            outcome
        },
        call(program, &second, action, vec![Value::uuid(shelf_id)]),
    )
    .await;
    let attempts = [&first, &second]
        .iter()
        .map(|racer| (racer.begun.get(), racer.levels.borrow().clone()))
        .collect();
    (outcomes, attempts, shared.saw_waiting.get())
}

#[test]
fn two_counters_at_once_rerun_only_when_their_action_reads_first() {
    if skipped() {
        return;
    }
    let root = project("race");
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let state = Rc::new(State {
        store: postgres(&url).expect("store"),
        other: postgres(&url).expect("store"),
        interfere: RefCell::new(None),
        begun: Cell::new(0),
        levels: RefCell::new(Vec::new()),
        inserted: RefCell::new(Vec::new()),
    });
    let host = Interfering(Rc::clone(&state));
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let shelf = value(call(&program, &host, "add_shelf", vec![Value::text("Jams")]).await);
        let Value::Uuid(shelf_id) = field(&shelf, "id") else {
            panic!("a shelf id");
        };
        let visits = || async { rows(&state.store, "SELECT \"visits\" FROM \"shelf\"").await };

        // `visit`'s counter is its first statement and it loads no record,
        // so it runs at read committed: the second request waits for the
        // first's lock, adds to its count and commits, and neither reruns.
        let ((a, b), attempts, waited) = race(&program, &url, "visit", &shelf_id).await;
        assert!(waited, "the second request waited for the first's lock");
        let counted = [field(&value(a), "visits"), field(&value(b), "visits")];
        assert_eq!(
            counted,
            [Value::Int(1), Value::Int(2)],
            "the second added to the first"
        );
        assert_eq!(
            attempts,
            vec![
                (1, vec![Isolation::ReadCommitted]),
                (1, vec![Isolation::ReadCommitted]),
            ],
            "each committed at its first attempt"
        );
        assert_eq!(visits().await, vec![vec![SqlValue::Int8(2)]], "up by two");

        // `tally` reads the shelf before it counts, so it runs at repeatable
        // read: the second request's update finds the row changed since its
        // snapshot, and the whole action runs again, reading afresh.
        let ((a, b), attempts, waited) = race(&program, &url, "tally", &shelf_id).await;
        assert!(waited, "the second request waited for the first's lock");
        let counted = [field(&value(a), "visits"), field(&value(b), "visits")];
        assert_eq!(counted, [Value::Int(3), Value::Int(4)]);
        assert_eq!(
            attempts,
            vec![
                (1, vec![Isolation::RepeatableRead]),
                (
                    2,
                    vec![Isolation::RepeatableRead, Isolation::RepeatableRead]
                ),
            ],
            "the second ran again"
        );
        assert_eq!(visits().await, vec![vec![SqlValue::Int8(4)]], "up by two");

        // An action that calls one that loads a record loads one; one that
        // calls only counters doesn't.
        state.levels.borrow_mut().clear();
        value(call(&program, &host, "recount", vec![Value::uuid(&shelf_id)]).await);
        value(call(&program, &host, "revisit", vec![Value::uuid(&shelf_id)]).await);
        assert_eq!(
            *state.levels.borrow(),
            [Isolation::RepeatableRead, Isolation::ReadCommitted]
        );
        assert_eq!(visits().await, vec![vec![SqlValue::Int8(7)]]);
    });
    drop(host);
    drop(state);
    databases.release(copy, false);
}
