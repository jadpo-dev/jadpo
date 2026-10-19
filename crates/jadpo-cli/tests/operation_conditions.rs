//! Standard operations in `where:` (area 6, "Operations in conditions";
//! chunk 6.3b), against live PostgreSQL: each query whose condition is an
//! operation of a field, run by the interpreter through an action, gives
//! exactly the records the same operation keeps in code over the same
//! values, edge cases among them (text by code points and Unicode's white
//! space, years before 1 AD, halves rounded each way, `none`), through the
//! index on its expression its migration builds (`EXPLAIN`). And every
//! form the checker lets a condition use is one PostgreSQL can index.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips the
//! live part.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_interp::Program;
use jadpo_runtime::{
    Clock, Ctx, Database, Env, Event, Executor, Host, LocalFuture, Outcome, SqlValue,
};
use jadpo_std::Op;
use jadpo_std::read::Source;
use jadpo_term::Environment;
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

/// Each query: its name, parameters, `where:` line, and the index its
/// expression is searched through. An action `find_<name>` returns it.
const QUERIES: &[(&str, &str, &str, &str)] = &[
    (
        "spine",
        "longest: Int",
        "where: text.length(Book.title) <= longest",
        "book_title_length_idx",
    ),
    (
        "of_year",
        "year: Int",
        "where: temporal.year(Book.published_on) == year",
        "book_published_on_year_idx",
    ),
    (
        "prefixed",
        "prefix: Book.title",
        "where: text.keep_first(Book.title, 3) == prefix",
        "book_title_keep_first_idx",
    ),
    (
        "the_titles",
        "",
        "where: text.starts_with(Book.title, \"The \")",
        "book_title_starts_with_idx",
    ),
    (
        "unshouted",
        "",
        "where: not text.ends_with(Book.title, \"!\")",
        "book_title_ends_with_idx",
    ),
    (
        "short_subtitles",
        "n: Int",
        "where: text.length(Book.subtitle) < n",
        "book_subtitle_length_idx",
    ),
    (
        "long_subtitles",
        "n: Int",
        "where: not (text.length(Book.subtitle) < n)",
        "book_subtitle_length_idx",
    ),
    (
        "hundreds",
        "n: Int",
        "where: integer.divide(Book.pages, 100, rounding: Rounding.half_even) == n",
        "book_pages_divide_idx",
    ),
    (
        "rounded",
        "most: Decimal",
        "where: decimal.round(Book.price, 0, rounding: Rounding.half_up) <= most",
        "book_price_round_idx",
    ),
    (
        "tagged",
        "n: Int",
        "where: list.count(Book.tags) >= n",
        "book_tags_count_idx",
    ),
    (
        "trimmed",
        "title: Book.title",
        "where: text.trim(Book.title) == title",
        "book_title_trim_idx",
    ),
];

/// A catalogue of books, each query above and an action that calls it.
fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("operation-conditions-{name}"));
    let _ = fs::remove_dir_all(&root);
    write(&root, "app.jadpo", "application {}\n");
    let domain = |path: &str, text: &str| write(&root, &format!("domains/library/{path}"), text);
    for (file, text) in [
        (
            "book_title",
            "/// A book's title.\ntype BookTitle = Text {\n    max_length: 200\n}\n",
        ),
        (
            "page_count",
            "/// How many pages a book has.\ntype PageCount = Int {\n    min: 1\n    max: 5000\n}\n",
        ),
        (
            "book_price",
            "/// What a book costs.\ntype BookPrice = Decimal {\n    scale: 2\n    min: -1000.00\n    max: 1000.00\n}\n",
        ),
        (
            "shelf_tag",
            "/// A shelf a book is on.\ntype ShelfTag = Text {\n    max_length: 20\n}\n",
        ),
        (
            "book_card",
            "use library/entities { Book }\n\n/// A book in a list.\ntype BookCard = Output {\n    pick: [Book.id, Book.title]\n}\n",
        ),
    ] {
        domain(&format!("types/{file}.jadpo"), text);
    }
    domain(
        "entities/book.jadpo",
        "use library/types { BookPrice, BookTitle, PageCount, ShelfTag }\n\n/// A book in the catalogue.\nentity Book {\n    id: Uuid\n    title: BookTitle\n    subtitle: BookTitle?\n    pages: PageCount\n    price: BookPrice\n    published_on: CalendarDate\n    tags: List<ShelfTag>\n\n    policy: {\n        read: [Access.public]\n    }\n}\n",
    );
    for (name, params, condition, _) in QUERIES {
        let args: Vec<&str> = params
            .split(", ")
            .filter(|param| !param.is_empty())
            .map(|param| param.split(':').next().unwrap_or(param))
            .collect();
        domain(
            &format!("queries/{name}.jadpo"),
            &format!(
                "use library/entities {{ Book }}\nuse library/types {{ BookCard }}\n\n/// Books found by an operation of a field.\nquery {name}({params}) -> List<BookCard> {{\n    from: Book\n    {condition}\n    order: [Book.id asc]\n    limit: 5000\n}}\n"
            ),
        );
        domain(
            &format!("actions/find_{name}.jadpo"),
            &format!(
                "use library/entities {{ Book }}\nuse library/queries {{ {name} }}\nuse library/types {{ BookCard }}\n\n/// What the query finds.\naction find_{name}({params}) -> List<BookCard> {{\n    return {name}({})\n}}\n",
                args.join(", ")
            ),
        );
    }
    let formatted = jadpo(&root, &["fmt"]);
    assert_eq!(
        formatted.code, 0,
        "{}{}",
        formatted.stdout, formatted.stderr
    );
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    root
}

#[test]
fn each_operation_derives_the_index_on_its_expression() {
    let root = project("indexes");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let folder = fs::read_dir(root.join("database/migrations"))
        .expect("migrations")
        .map(|entry| entry.expect("entry").path())
        .next()
        .expect("a migration");
    let sql = fs::read_to_string(folder.join("postgres.sql")).expect("postgres.sql");
    for index in [
        "CREATE INDEX \"book_title_length_idx\" ON \"book\" ((char_length(\"title\")::bigint));",
        "CREATE INDEX \"book_published_on_year_idx\" ON \"book\" (((CASE WHEN extract(year FROM \"published_on\") < 0 THEN extract(year FROM \"published_on\") + 1 ELSE extract(year FROM \"published_on\") END)::bigint));",
        "CREATE INDEX \"book_title_keep_first_idx\" ON \"book\" ((left(\"title\", LEAST(3, 2147483647)::integer)));",
        "CREATE INDEX \"book_tags_count_idx\" ON \"book\" ((jsonb_array_length(\"tags\")::bigint));",
    ] {
        assert!(sql.contains(index), "{index}\n{sql}");
    }
}

/// A host whose one store is a PostgreSQL database.
struct Store(jadpo_host_native::Postgres);

impl Host for Store {
    fn database(&self, _store: &str) -> Option<&dyn Database> {
        Some(&self.0)
    }

    fn deliver(&self, _events: Vec<Event>) -> LocalFuture<'_, ()> {
        Box::pin(async {})
    }

    fn random(&self, _bytes: &mut [u8]) -> bool {
        false
    }
}

/// A migrated database for the project, its program, and a runtime to
/// call it with.
struct Live {
    url: String,
    program: Program,
    host: Store,
    runtime: jadpo_host_native::Runtime,
}

impl Live {
    fn new(root: &Path, stem: &str) -> Live {
        let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
        let url = server.create_database(stem).expect("database");
        let applied = jadpo(root, &["migrate", "apply", "--url", &url]);
        assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
        let loaded = jadpo_db::load(root);
        let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
        let host = Store(jadpo_host_native::postgres(&url).expect("store"));
        Live {
            url,
            program,
            host,
            runtime: jadpo_host_native::Runtime::new().expect("runtime"),
        }
    }

    /// Runs `statements` then `sql` with `params` in one transaction,
    /// committed.
    fn rows(&self, statements: &[&str], sql: &str, params: &[SqlValue]) -> Vec<Vec<SqlValue>> {
        let store = jadpo_host_native::postgres(&self.url).expect("store");
        self.runtime.block_on(async {
            let mut transaction = store.begin().await.expect("begin");
            for statement in statements {
                transaction
                    .batch(statement)
                    .await
                    .unwrap_or_else(|e| panic!("{statement}: {e:?}"));
            }
            let rows = transaction
                .query(sql, params)
                .await
                .unwrap_or_else(|e| panic!("{sql}: {e:?}"));
            transaction.commit().await.expect("commit");
            rows
        })
    }

    /// Inserts rows, each its values, in one transaction.
    fn insert(&self, sql: &str, rows: &[Vec<SqlValue>]) {
        let store = jadpo_host_native::postgres(&self.url).expect("store");
        self.runtime.block_on(async {
            let mut transaction = store.begin().await.expect("begin");
            for row in rows {
                transaction
                    .execute(sql, row)
                    .await
                    .unwrap_or_else(|e| panic!("{sql}: {e:?}"));
            }
            transaction.commit().await.expect("commit");
        });
    }

    /// The numbers of the books `find_<query>` returns, in order.
    fn call(&self, query: &str, args: Vec<Value>) -> Vec<u64> {
        let action = format!("find_{query}");
        let id = self.program.plan().find(&action).expect("the action");
        let found = self.runtime.block_on(async {
            let env = Env {
                seed: Some(1),
                clock: Clock::TEST,
            };
            let mut ctx = Ctx::new(self.program.places(), &self.host, env);
            self.program.run(&mut ctx, id, args).await
        });
        let Outcome::Value(Value::List(found)) = found else {
            panic!("{action}: expected a list, got {found:?}");
        };
        found
            .iter()
            .map(|card| {
                let Value::Record(card) = card else {
                    panic!("not a record: {card:?}");
                };
                match card.get("id") {
                    Some(Value::Uuid(id)) => number(id),
                    other => panic!("no id: {other:?}"),
                }
            })
            .collect()
    }

    /// The plan PostgreSQL makes for the query's statement with `args`, with
    /// sequential scans off, so it shows whether an index serves it at all
    /// whatever a small table's costs say.
    fn explain(&self, query: &str, args: &[Value]) -> String {
        let plan = self
            .program
            .plan()
            .query_named(query)
            .expect("the query's plan");
        let params: Vec<SqlValue> = plan
            .read
            .select
            .params
            .iter()
            .map(|param| match &param.source {
                Source::Arg { index, path } if path.is_empty() => {
                    jadpo_runtime::to_sql(&args[*index as usize], param.ty).expect("a parameter")
                }
                Source::Stored(value) => value.clone(),
                other => panic!("a parameter from {other:?}"),
            })
            .collect();
        let rows = self.rows(
            &["SET LOCAL enable_seqscan = off"],
            &format!("EXPLAIN {}", plan.read.select.sql),
            &params,
        );
        rows.iter()
            .filter_map(|row| match row.first() {
                Some(SqlValue::Text(line)) => Some(line.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Book `n`'s identity.
fn uuid(n: u64) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    bytes[8..].copy_from_slice(&n.to_be_bytes());
    bytes[6] = 0x70;
    bytes[8] |= 0x80;
    bytes
}

/// The book number an identity holds.
fn number(id: &str) -> u64 {
    let hex: String = id.chars().filter(|c| *c != '-').collect();
    let bytes: Vec<u8> = (0..16)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).expect("hex"))
        .collect();
    let mut tail = [0u8; 8];
    tail.copy_from_slice(&bytes[8..]);
    tail[0] &= 0x7f;
    u64::from_be_bytes(tail)
}

/// One book as code holds its fields.
struct Book {
    n: u64,
    title: String,
    subtitle: Option<String>,
    pages: i64,
    price: &'static str,
    day: i32,
    tags: usize,
}

/// What operation `op` gives on `args` in code.
fn ran(op: Op, args: &[Value]) -> Value {
    jadpo_value::run(op, args, None, &mut |_| None).unwrap_or_else(|e| panic!("{op:?}: {e:?}"))
}

fn int(value: Value) -> i64 {
    match value {
        Value::Int(n) => n,
        other => panic!("not a whole number: {other:?}"),
    }
}

fn holds(value: Value) -> bool {
    match value {
        Value::Bool(holds) => holds,
        other => panic!("not a `Bool`: {other:?}"),
    }
}

fn decimal(text: &str) -> Value {
    Value::decimal(text).expect("a decimal")
}

fn rounding(name: &str, ordinal: u32) -> Value {
    Value::variant(jadpo_std::Rounding::NAME, ordinal, name, Vec::new())
}

/// The catalogue: titles around 24 code points and their bytes, white
/// space Unicode names at either end, titles that start with "The " or
/// end with "!", days either side of 1 AD and of New Year, page counts
/// and prices on their halves, and subtitles that may be `none`.
fn catalogue() -> Vec<Book> {
    let titles = [
        "Dune",
        "Café Society à la Plage!",
        "Café Society à la Plages",
        "The Left Hand of Darkness",
        "The Dispossessed",
        "the lowercase article",
        "Thermodynamics",
        "\u{3000}Ideographic space",
        "Ideographic space\u{3000}",
        "\u{a0}No-break both\u{a0}",
        "\u{85}Next line",
        "Tabbed\t",
        "Combining e\u{301}\u{301}\u{301}",
        "雪国",
        "🦀 Crab book",
        "A!",
        "!",
        "Plain",
        "Plain ",
        " Plain",
    ];
    let subtitles = [
        None,
        Some("A short one"),
        Some(""),
        Some("A subtitle that runs on for a while"),
        Some("雪"),
    ];
    let pages = [1, 49, 50, 51, 149, 150, 151, 250, 350, 4999, 5000];
    let prices = [
        "0.00", "0.49", "0.50", "0.51", "1.50", "2.50", "-0.50", "-1.50", "-2.49", "999.99",
    ];
    // Days since 1970-01-01: 1 AD, 1 BC and 2 BC either side of New Year,
    // and a spread of others.
    let mut days: Vec<i32> = vec![
        -719_162, -719_163, -719_528, -719_529, -719_894, 0, -1, 364, 365, 20_000, 20_454,
    ];
    days.extend((0..40).map(|i| -900_000 + i * 47_351));
    let mut books = Vec::new();
    let mut n = 1u64;
    for (t, title) in titles.iter().enumerate() {
        for (s, subtitle) in subtitles.iter().enumerate() {
            for k in 0..6 {
                let at = t * 31 + s * 7 + k;
                books.push(Book {
                    n,
                    title: title.to_string(),
                    subtitle: subtitle.map(str::to_string),
                    pages: pages[at % pages.len()],
                    price: prices[at % prices.len()],
                    day: days[at % days.len()],
                    tags: at % 5,
                });
                n += 1;
            }
        }
    }
    books
}

#[test]
fn each_operation_gives_the_code_s_answer_through_its_index() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("live");
    let live = Live::new(&root, "operation_conditions_live");
    let books = catalogue();
    live.insert(
        "INSERT INTO book (id, title, subtitle, pages, price, published_on, tags) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        &books
            .iter()
            .map(|book| {
                let tags: Vec<String> = (0..book.tags).map(|i| format!("\"shelf {i}\"")).collect();
                vec![
                    SqlValue::Uuid(uuid(book.n)),
                    SqlValue::Text(book.title.clone()),
                    match &book.subtitle {
                        Some(text) => SqlValue::Text(text.clone()),
                        None => SqlValue::Null(jadpo_std::read::SqlType::Text),
                    },
                    SqlValue::Int8(book.pages),
                    SqlValue::Numeric(book.price.to_string()),
                    SqlValue::Date(book.day),
                    SqlValue::Json(format!("[{}]", tags.join(","))),
                ]
            })
            .collect::<Vec<_>>(),
    );
    let text = |text: &str| Value::text(text);
    let date = |book: &Book| Value::Date(book.day);
    // Each query, the arguments it is called with, and which books code
    // keeps for them.
    type Keeps = Box<dyn Fn(&Book) -> bool>;
    let cases: Vec<(&str, Vec<Value>, Keeps)> = vec![
        (
            "spine",
            vec![Value::Int(24)],
            Box::new(move |book: &Book| int(ran(Op::TextLength, &[text(&book.title)])) <= 24),
        ),
        (
            "of_year",
            vec![Value::Int(0)],
            Box::new(move |book: &Book| int(ran(Op::TemporalYear, &[date(book)])) == 0),
        ),
        (
            "of_year",
            vec![Value::Int(1)],
            Box::new(move |book: &Book| int(ran(Op::TemporalYear, &[date(book)])) == 1),
        ),
        (
            "of_year",
            vec![Value::Int(-1)],
            Box::new(move |book: &Book| int(ran(Op::TemporalYear, &[date(book)])) == -1),
        ),
        (
            "of_year",
            vec![Value::Int(1970)],
            Box::new(move |book: &Book| int(ran(Op::TemporalYear, &[date(book)])) == 1970),
        ),
        (
            "prefixed",
            vec![text("Caf")],
            Box::new(move |book: &Book| {
                ran(Op::TextKeepFirst, &[text(&book.title), Value::Int(3)]) == text("Caf")
            }),
        ),
        (
            "prefixed",
            vec![text("\u{3000}Id")],
            Box::new(move |book: &Book| {
                ran(Op::TextKeepFirst, &[text(&book.title), Value::Int(3)]) == text("\u{3000}Id")
            }),
        ),
        (
            "the_titles",
            vec![],
            Box::new(move |book: &Book| {
                holds(ran(Op::TextStartsWith, &[text(&book.title), text("The ")]))
            }),
        ),
        (
            "unshouted",
            vec![],
            Box::new(move |book: &Book| {
                !holds(ran(Op::TextEndsWith, &[text(&book.title), text("!")]))
            }),
        ),
        (
            "short_subtitles",
            vec![Value::Int(5)],
            Box::new(move |book: &Book| match &book.subtitle {
                Some(subtitle) => int(ran(Op::TextLength, &[text(subtitle)])) < 5,
                None => false,
            }),
        ),
        (
            "long_subtitles",
            vec![Value::Int(5)],
            Box::new(move |book: &Book| match &book.subtitle {
                Some(subtitle) => int(ran(Op::TextLength, &[text(subtitle)])) >= 5,
                // A record whose field is `none` meets no condition on an
                // operation of it, `not` included.
                None => false,
            }),
        ),
        (
            "hundreds",
            vec![Value::Int(2)],
            Box::new(move |book: &Book| {
                int(ran(
                    Op::IntegerDivide,
                    &[Value::Int(book.pages), Value::Int(100), rounding("half_even", 0)],
                )) == 2
            }),
        ),
        (
            "hundreds",
            vec![Value::Int(0)],
            Box::new(move |book: &Book| {
                int(ran(
                    Op::IntegerDivide,
                    &[Value::Int(book.pages), Value::Int(100), rounding("half_even", 0)],
                )) == 0
            }),
        ),
        (
            "rounded",
            vec![decimal("1")],
            Box::new(move |book: &Book| {
                let rounded = ran(
                    Op::DecimalRound,
                    &[decimal(book.price), Value::Int(0), rounding("half_up", 1)],
                );
                matches!(
                    jadpo_value::ops::compare(&rounded, &decimal("1")),
                    Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
                )
            }),
        ),
        (
            "tagged",
            vec![Value::Int(3)],
            Box::new(move |book: &Book| book.tags >= 3),
        ),
        (
            "trimmed",
            vec![text("Ideographic space")],
            Box::new(move |book: &Book| {
                ran(Op::TextTrim, &[text(&book.title)]) == text("Ideographic space")
            }),
        ),
        (
            "trimmed",
            vec![text("No-break both")],
            Box::new(move |book: &Book| {
                ran(Op::TextTrim, &[text(&book.title)]) == text("No-break both")
            }),
        ),
        (
            "trimmed",
            vec![text("Plain")],
            Box::new(move |book: &Book| ran(Op::TextTrim, &[text(&book.title)]) == text("Plain")),
        ),
    ];
    for (query, args, keeps) in &cases {
        let expected: Vec<u64> = books
            .iter()
            .filter(|book| keeps(book))
            .map(|book| book.n)
            .collect();
        let found = live.call(query, args.clone());
        assert_eq!(found, expected, "{query} {args:?}: the code's answer");
    }
    // Each through the index on its expression.
    for (query, _, _, index) in QUERIES {
        let args: Vec<Value> = cases
            .iter()
            .find(|(name, ..)| name == query)
            .map(|(_, args, _)| args.clone())
            .expect("a case for each query");
        let plan = live.explain(query, &args);
        assert!(plan.contains(index), "{query} reads `{index}`:\n{plan}");
    }
}

/// Every form an operation in a condition is lowered to, applied to a
/// column of the type its first argument is held in, builds an index on
/// PostgreSQL: an immutable expression with no subquery. One the
/// checker refuses for its form fails to.
#[test]
fn every_indexable_form_can_be_indexed() {
    if !jadpo_test::live() {
        return;
    }
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server
        .create_database("operation_conditions_forms")
        .expect("database");
    let store = jadpo_host_native::postgres(&url).expect("store");
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    let tried = jadpo_sql::form::indexable_examples();
    assert!(tried.len() > 30, "every form tried: {}", tried.len());
    runtime.block_on(async {
        let mut transaction = store.begin().await.expect("begin");
        transaction
            .batch("CREATE TABLE forms (t text, i bigint, d numeric, b bytea, day date, j jsonb, u uuid, f boolean, c text)")
            .await
            .expect("a table");
        transaction.commit().await.expect("commit");
        for (at, (op, expression, indexable)) in tried.iter().enumerate() {
            let mut transaction = store.begin().await.expect("begin");
            let made = transaction
                .batch(&format!(
                    "CREATE INDEX \"form_{at}_idx\" ON forms (({expression}))"
                ))
                .await;
            assert_eq!(
                made.is_ok(),
                *indexable,
                "{op:?}: {expression} ({made:?})"
            );
            if made.is_ok() {
                transaction.commit().await.expect("commit");
            }
        }
    });
}
