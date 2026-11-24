//! `limit: every` on live PostgreSQL (area 6, "Every record that
//! matches"; chunk 10.4a): a query reads every record that matches, up to
//! 10,000, in one statement asking for 10,001; with more than that it
//! gives none back and the action stops with the fault `too_many_records`,
//! and a bulk change made of that query and a loop of deletes changes
//! nothing.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips the
//! live part.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_interp::Program;
use jadpo_runtime::{Clock, Ctx, Database, Env, Executor, FaultKind, Outcome};
use jadpo_term::Environment;
use jadpo_test::host::Harness;
use jadpo_value::Value;

use crate::support;

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
    let mut output = run(&args, &env.with_cwd(cwd));
    output.stdout = support::without_run_time(&output.stdout);
    output
}

/// Todos, a query of every done one, an action giving them and one
/// deleting them all.
fn project() -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("every-record");
    let _ = fs::remove_dir_all(&root);
    write(&root, "app.jadpo", "application {}\n");
    for (file, text) in [
        (
            "types/todo_status.jadpo",
            "/// Whether a todo is still to do.\ntype TodoStatus = Enum {\n    open\n    done\n}\n",
        ),
        (
            "entities/todo.jadpo",
            "use planning/types { TodoStatus }\n\ntype TodoTitle = Text {\n    max_length: 200\n}\n\n/// Something to do.\nentity Todo {\n    id: Uuid\n    title: TodoTitle\n    status: TodoStatus\n\n    policy: {\n        read: [Access.public]\n        delete: [Access.public]\n    }\n}\n",
        ),
        (
            "queries/done_todos.jadpo",
            "use planning/entities { Todo }\nuse planning/types { TodoStatus }\n\n/// Every done todo.\nquery done_todos() -> List<Todo> {\n    from: Todo\n    where: Todo.status == TodoStatus.done\n    order: [Todo.title asc]\n    limit: every\n}\n",
        ),
        (
            "actions/export_done.jadpo",
            "use planning/entities { Todo }\nuse planning/queries { done_todos }\n\n/// Every done todo, for an export.\naction export_done() -> List<Todo> {\n    return done_todos()\n}\n",
        ),
        (
            "actions/clear_done.jadpo",
            "use planning/entities { Todo }\nuse planning/queries { done_todos }\n\n/// How many todos a clear deleted.\ntype ClearedCount = Int {}\n\n/// Deletes every done todo, and gives how many.\naction clear_done() -> ClearedCount {\n    var done: List<Todo> = done_todos()\n    for todo in done {\n        Todo.delete(todo)\n    }\n    return ClearedCount(list.count(done))\n}\n",
        ),
    ] {
        write(&root, &format!("domains/planning/{file}"), text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    jadpo(&root, &["records", "update"]);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    root
}

/// Runs `sql` on the database at `url` in a transaction of its own.
fn execute(url: &str, sql: &str) {
    let store = jadpo_host_native::postgres(url).expect("store");
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    runtime.block_on(async {
        let mut transaction = store.begin().await.expect("begin");
        transaction.execute(sql, &[]).await.expect("statement");
        transaction.commit().await.expect("commit");
    });
}

/// What the action called `name` gives, run by the interpreter.
fn call(program: &Program, url: &str, name: &str) -> Outcome {
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    let host = Harness::postgres(url);
    let id = program.plan().find(name).expect("the action");
    runtime.block_on(async {
        let env = Env {
            seed: Some(1),
            clock: Clock::TEST,
        };
        let mut ctx = Ctx::of(program, &host, env);
        program.run(&mut ctx, id, Vec::new()).await
    })
}

fn listed(outcome: &Outcome) -> usize {
    match outcome {
        Outcome::Value(Value::List(items)) => items.len(),
        other => panic!("not a list: {other:?}"),
    }
}

fn too_many(outcome: &Outcome) {
    let Outcome::Fault(fault) = outcome else {
        panic!("not the fault: {outcome:?}");
    };
    assert_eq!(
        fault.kind,
        FaultKind::TooManyRecords { most: 10_000 },
        "{fault:?}"
    );
    assert_eq!(fault.kind.classification(), "too_many_records");
}

#[test]
fn every_reads_ten_thousand_and_more_is_a_fault_that_changes_nothing() {
    let root = project();
    let loaded = jadpo_db::load(&root);
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    // One statement asking for one more than the most, so the runtime
    // knows when more match.
    let plan = program.plan().query_named("done_todos").expect("the plan");
    assert_eq!(plan.read.every, Some(10_000));
    assert!(
        plan.read.select.sql.ends_with("LIMIT 10001"),
        "{}",
        plan.read.select.sql
    );
    if !jadpo_test::live() {
        return;
    }
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server.create_database("every_record").expect("database");
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    let seed = |from: u32, to: u32, status: &str| {
        execute(
            &url,
            &format!(
                "INSERT INTO \"todo\" (\"id\", \"title\", \"status\") SELECT gen_random_uuid(), 'todo ' || lpad(n::text, 5, '0'), '{status}' FROM generate_series({from}, {to}) AS n"
            ),
        );
    };
    // Exactly the most, and open ones beside them: every done one.
    seed(1, 10_000, "done");
    seed(1, 50, "open");
    assert_eq!(listed(&call(&program, &url, "export_done")), 10_000);

    // One more: none of them, and the action stops.
    seed(10_001, 10_001, "done");
    too_many(&call(&program, &url, "export_done"));
    // The bulk change reads them first, so it deletes nothing.
    too_many(&call(&program, &url, "clear_done"));
    execute(&url, "DELETE FROM \"todo\" WHERE \"title\" = 'todo 10001'");
    assert_eq!(
        listed(&call(&program, &url, "export_done")),
        10_000,
        "nothing was deleted"
    );

    // Within the most, the bulk change deletes every one.
    match call(&program, &url, "clear_done") {
        Outcome::Value(Value::Int(n)) => assert_eq!(n, 10_000),
        other => panic!("not a count: {other:?}"),
    }
    assert_eq!(listed(&call(&program, &url, "export_done")), 0);
}

/// Baskets kept in Redis, a query of every open one, and tests reading
/// three and then 10,001.
fn redis_project() -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("every-record-redis");
    let _ = fs::remove_dir_all(&root);
    write(
        &root,
        "app.jadpo",
        "application {\n    connections: {\n        baskets: {\n            driver: Driver.redis\n            url: config.redis_url\n        }\n    }\n}\n",
    );
    write(
        &root,
        "config.jadpo",
        "config {\n    redis_url: RedisUrl\n}\n",
    );
    for (file, text) in [
        (
            "types/basket_state.jadpo",
            "/// Whether a basket is still being filled.\ntype BasketState = Enum {\n    open\n    paid\n}\n",
        ),
        (
            "types/item_count.jadpo",
            "/// How many items a basket holds.\ntype ItemCount = Int {\n    min: 0\n}\n",
        ),
        (
            "entities/basket.jadpo",
            "use shop/types { BasketState, ItemCount }\n\n/// A shopper's basket.\nentity Basket {\n    id: Uuid\n    state: BasketState\n    items: ItemCount\n\n    store: baskets\n    expires_after: 1d\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n    }\n    indexes: {\n        by_state: [state]\n    }\n}\n",
        ),
        (
            "queries/all_open.jadpo",
            "use shop/entities { Basket }\nuse shop/types { BasketState }\n\n/// Every open basket.\nquery all_open() -> List<Basket> {\n    from: Basket\n    where: Basket.state == BasketState.open\n    limit: every\n}\n",
        ),
        (
            "actions/open_baskets_for.jadpo",
            "use shop/entities { Basket }\nuse shop/types { BasketState }\n\n/// Opens one basket for each of `items`, holding that many items.\naction open_baskets_for(items: List<Basket.items>) -> List<Basket.id> {\n    var mut opened: List<Basket.id> = []\n    for count in items {\n        var basket: Basket = Basket.create {\n            set: {\n                state: BasketState.open\n                items: count\n            }\n        }\n        opened = list.append(opened, basket.id)\n    }\n    return opened\n}\n",
        ),
        (
            "actions/open_baskets_for.test.jadpo",
            "use shop/entities { Basket }\nuse shop/queries { all_open }\n\ntest \"every open basket comes back\" {\n    var opened: List<Basket.id> = open_baskets_for([1, 2, 3])\n    assert list.count(all_open()) == list.count(opened)\n}\n\ntest \"more than ten thousand is too many\" {\n    var ten: List<Basket.items> = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]\n    var hundred: List<Basket.items> = list.flatten(list.map(ten, a -> ten))\n    var many: List<Basket.items> = list.flatten(list.map(hundred, a -> hundred))\n    var opened: List<Basket.id> = open_baskets_for(list.append(many, 1))\n    assert list.count(all_open()) == list.count(opened)\n}\n",
        ),
    ] {
        write(&root, &format!("domains/shop/{file}"), text);
    }
    jadpo(&root, &["records", "update"]);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    root
}

#[test]
fn every_reads_a_redis_list_and_more_is_the_same_fault() {
    let root = redis_project();
    let loaded = jadpo_db::load(&root);
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let plan = program.plan().query_named("all_open").expect("the plan");
    assert_eq!(plan.read.every, Some(10_000));
    if !jadpo_test::live() {
        return;
    }
    let tested = jadpo(&root, &["test"]);
    let said = format!("{}{}", tested.stdout, tested.stderr);
    assert!(
        said.contains("test failed: 1 failed, 1 passed"),
        "the three come back, and 10,001 are too many: {said}"
    );
    assert!(
        said.contains("fault: too_many_records in test \"more than ten thousand is too many\""),
        "{said}"
    );
    assert!(said.contains("more than 10,000 match"), "{said}");
}
