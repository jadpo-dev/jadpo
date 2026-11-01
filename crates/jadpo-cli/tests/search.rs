//! Search on PostgreSQL (area 16, "Search"; chunk B12), against live
//! PostgreSQL: a `search:` added to an entity that already has records is
//! one migration, `jadpo migrate new`, that adds the generated `tsvector`
//! and its GIN index, so the records already stored are found at once; the
//! interpreter's query finds, ranks and pages them by words through that
//! index; and changing the search's fields makes the column again from
//! them, its index with it.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips the
//! live part.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_interp::Program;
use jadpo_runtime::{Clock, Ctx, Database, Env, Executor, Outcome, SqlValue};
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

const ENTITY: &str = "use planning/types { TodoNotes, TodoTitle }

/// Something to do.
entity Todo {
    id: Uuid
    title: TodoTitle
    notes: TodoNotes?

    policy: {
        read: [Access.public]
    }
}
";

/// `ENTITY` searched by `fields`.
fn searched(fields: &str) -> String {
    ENTITY.replace(
        "    notes: TodoNotes?\n",
        &format!(
            "    notes: TodoNotes?\n\n    search: {{\n        fields: [{fields}]\n        language: SearchLanguage.english\n    }}\n"
        ),
    )
}

/// A project of todos, not searched yet, with its first migration.
fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("search-{name}"));
    let _ = fs::remove_dir_all(&root);
    write(&root, "app.jadpo", "application {}\n");
    for (file, text) in [
        (
            "types/todo_title.jadpo",
            "/// A todo's title.\ntype TodoTitle = Text {\n    max_length: 200\n}\n",
        ),
        (
            "types/todo_notes.jadpo",
            "/// What a todo says beyond its title.\ntype TodoNotes = Text {\n    max_length: 2000\n}\n",
        ),
        ("entities/todo.jadpo", ENTITY),
    ] {
        write(&root, &format!("domains/planning/{file}"), text);
    }
    for args in [&["migrate", "new", "initial"][..], &["records", "update"]] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    }
    root
}

/// The query and the action that calls it, written once the entity is
/// searched.
fn add_search(root: &Path, fields: &str, purpose: &str) -> String {
    write(
        root,
        "domains/planning/entities/todo.jadpo",
        &searched(fields),
    );
    write(
        root,
        "domains/planning/types/todo_view.jadpo",
        "use planning/entities { Todo }\n\n/// A todo in a list.\ntype TodoView = Output {\n    pick: [Todo.id, Todo.title]\n}\n",
    );
    write(
        root,
        "domains/planning/queries/search_todos.jadpo",
        "use planning/entities { Todo }\nuse planning/types { TodoView }\n\n/// The todos that match a search, best first, two at a time.\nquery search_todos(phrase: SearchText, after: PageCursor?) -> Page<TodoView> {\n    from: Todo\n    where: Todo matches phrase\n    page: {\n        size: 2\n        after: after\n    }\n}\n",
    );
    write(
        root,
        "domains/planning/actions/find_todos.jadpo",
        "use planning/types { TodoView }\nuse planning/queries { search_todos }\n\n/// The todos that match a search.\naction find_todos(phrase: SearchText, after: PageCursor?) -> Page<TodoView> {\n    return search_todos(phrase, after)\n}\n",
    );
    let made = jadpo(root, &["migrate", "new", purpose]);
    assert_eq!(made.code, 0, "{}{}", made.stdout, made.stderr);
    let checked = jadpo(root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let folder = fs::read_dir(root.join("database/migrations"))
        .expect("migrations")
        .map(|entry| entry.expect("entry").path())
        .find(|path| path.to_string_lossy().ends_with(purpose))
        .expect("the migration");
    fs::read_to_string(folder.join("postgres.sql")).expect("postgres.sql")
}

fn rows(url: &str, sql: &str, params: &[SqlValue]) -> Vec<Vec<SqlValue>> {
    let store = jadpo_host_native::postgres(url).expect("store");
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    runtime.block_on(async {
        let mut transaction = store.begin().await.expect("begin");
        let rows = transaction
            .query(sql, params)
            .await
            .unwrap_or_else(|e| panic!("{sql}: {e:?}"));
        transaction.commit().await.expect("commit");
        rows
    })
}

/// One page of `search_todos` for `phrase`, run by the interpreter: the
/// titles and the cursor for the next page.
fn search(root: &Path, url: &str, phrase: &str, after: Value) -> (Vec<String>, Value) {
    let loaded = jadpo_db::load(root);
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let id = program.plan().find("find_todos").expect("the action");
    let host = Harness::postgres(url);
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    let found = runtime.block_on(async {
        let env = Env {
            seed: Some(1),
            clock: Clock::TEST,
        };
        let mut ctx = Ctx::of(&program, &host, env);
        program
            .run(&mut ctx, id, vec![Value::text(phrase), after])
            .await
    });
    let Outcome::Value(Value::Record(page)) = found else {
        panic!("expected a page, got {found:?}");
    };
    let Some(Value::List(items)) = page.get("items") else {
        panic!("no items: {page:?}");
    };
    let titles = items
        .iter()
        .map(|item| match item {
            Value::Record(item) => item
                .get("title")
                .map(|title| title.to_string().trim_matches('"').to_string())
                .unwrap_or_default(),
            other => panic!("not a record: {other:?}"),
        })
        .collect();
    (titles, page.get("next").cloned().unwrap_or(Value::None))
}

#[test]
fn records_stored_before_a_search_is_added_are_found_ranked_and_paged() {
    let root = project("added");
    // The first migration applied and records written, when there is a
    // server, before the search is added.
    let url = jadpo_test::live().then(|| {
        let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
        let url = server.create_database("search_added").expect("database");
        let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
        assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
        for (i, (title, notes)) in [
            ("Call the bank", Some("ask about packing boxes")),
            ("Pack the van", None),
            ("Pack books", Some("packing tape too")),
            ("Water plants", None),
        ]
        .into_iter()
        .enumerate()
        {
            let mut id = [0u8; 16];
            id[15] = u8::try_from(i + 1).expect("a byte");
            id[6] = 0x40;
            id[8] = 0x80;
            rows(
                &url,
                "INSERT INTO todo (id, title, notes) VALUES ($1, $2, $3)",
                &[
                    SqlValue::Uuid(id),
                    SqlValue::Text(title.to_string()),
                    notes.map_or(SqlValue::Null(jadpo_runtime::SqlType::Text), |n| {
                        SqlValue::Text(n.to_string())
                    }),
                ],
            );
        }
        url
    });
    let sql = add_search(&root, "title, notes", "search_todo");
    assert!(
        sql.contains("ALTER TABLE \"todo\" ADD COLUMN \"jadpo_search\" tsvector GENERATED ALWAYS AS (setweight(to_tsvector('english'::regconfig, coalesce(\"title\", '')), 'A') || setweight(to_tsvector('english'::regconfig, coalesce(\"notes\", '')), 'B')) STORED;\nCREATE INDEX \"todo_jadpo_search_idx\" ON \"todo\" USING gin (\"jadpo_search\");"),
        "{sql}"
    );
    assert!(!sql.contains("backfill"), "nothing to decide: {sql}");
    let Some(url) = url else {
        return;
    };
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    // Found at once, the best matches first, a page at a time: both of a
    // record's fields count, its title more than its notes.
    let (first, next) = search(&root, &url, "packing", Value::None);
    assert_eq!(first, ["Pack books", "Pack the van"]);
    assert!(matches!(next, Value::Text(_)), "a next page: {next:?}");
    let (second, last) = search(&root, &url, "packing", next);
    assert_eq!(second, ["Call the bank"]);
    assert_eq!(last, Value::None);
    // A quoted phrase, a word left out, `or` as a word, and stop words.
    assert_eq!(
        search(&root, &url, "\"pack the van\"", Value::None).0,
        ["Pack the van"]
    );
    // A word left out finds the others, in the order PostgreSQL ranks them.
    let mut without = search(&root, &url, "pack -boxes", Value::None).0;
    without.sort();
    assert_eq!(without, ["Pack books", "Pack the van"]);
    assert_eq!(
        search(&root, &url, "books or tape", Value::None).0,
        ["Pack books"]
    );
    assert!(search(&root, &url, "the", Value::None).0.is_empty());
    // The statement is one the GIN index serves.
    rows(&url, "ANALYZE todo", &[]);
    let loaded = jadpo_db::load(&root);
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let plan = program
        .plan()
        .query_named("search_todos")
        .expect("the query's plan");
    let params: Vec<SqlValue> = plan
        .read
        .select
        .params
        .iter()
        .map(|param| match &param.source {
            jadpo_std::read::Source::SearchPhrase { .. } => SqlValue::Text("packing".into()),
            jadpo_std::read::Source::Cursor(_) => SqlValue::Null(param.ty),
            other => panic!("a parameter from {other:?}"),
        })
        .collect();
    let store = jadpo_host_native::postgres(&url).expect("store");
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    let explained = runtime.block_on(async {
        let mut transaction = store.begin().await.expect("begin");
        transaction
            .batch("SET LOCAL enable_seqscan = off")
            .await
            .expect("a setting");
        let rows = transaction
            .query(&format!("EXPLAIN {}", plan.read.select.sql), &params)
            .await
            .expect("explain");
        let _ = transaction.rollback().await;
        rows
    });
    let lines: Vec<String> = explained
        .iter()
        .filter_map(|row| match row.first() {
            Some(SqlValue::Text(line)) => Some(line.clone()),
            _ => None,
        })
        .collect();
    assert!(
        lines
            .iter()
            .any(|line| line.contains("todo_jadpo_search_idx")),
        "{}",
        lines.join("\n")
    );
    // Searched by its notes first: the column made again from them.
    let sql = add_search(&root, "notes, title", "change_todo_search");
    assert!(
        sql.contains("ALTER TABLE \"todo\" DROP COLUMN \"jadpo_search\";\nALTER TABLE \"todo\" ADD COLUMN \"jadpo_search\" tsvector GENERATED ALWAYS AS (setweight(to_tsvector('english'::regconfig, coalesce(\"notes\", '')), 'A')"),
        "{sql}"
    );
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    assert_eq!(
        search(&root, &url, "packing", Value::None).0,
        ["Pack books", "Call the bank"]
    );
    let index = rows(
        &url,
        "SELECT indexname::text FROM pg_indexes WHERE tablename = 'todo' AND indexname = 'todo_jadpo_search_idx'",
        &[],
    );
    assert_eq!(index.len(), 1, "the index made again");
}
