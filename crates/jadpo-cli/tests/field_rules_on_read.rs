//! A field's rules on read (area 5, "A field's rules on read"; chunk 5.9d),
//! against live PostgreSQL: a row edited outside Jadpo so that a stored
//! value breaks a rule its field's type has (a whole number's range, a
//! text's lengths and pattern, a decimal's range, a duration's, money's
//! range, a built-in type's own check, the field's own rule) is a
//! defect when any read reaches it, naming the field and the rule and
//! never the value: a query, `get`, a parent's and a child's include, an
//! aggregate's group and `max`, a map layer, a nearest query, the record an
//! update gives back and a backfill worked out in Jadpo. Every read runs
//! through the runtime's one read path, which both executors call; the
//! interpreter drives it here.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools, with PostGIS and pgvector (CI's
//! image has both, `tools/postgres/Dockerfile`); `JADPO_SKIP_POSTGRES=1`
//! skips the live part.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_interp::Program;
use jadpo_runtime::{Clock, Ctx, Database, Env, Executor, Host, Outcome, SqlValue};
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

fn folder(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("field-rules-5.9d-{name}"));
    let _ = fs::remove_dir_all(&root);
    root
}

/// A workshop's sheds and tools, each tool's fields under a rule of every
/// kind, and a read of each kind of them.
const WORKSHOP: &[(&str, &str)] = &[
    (
        "types/stars.jadpo",
        "/// How well a tool works.\ntype Stars = Int {\n    min: 1\n    max: 5\n}\n",
    ),
    (
        "types/tool_name.jadpo",
        "/// What a tool or a shed is called.\ntype ToolName = Text {\n    min_length: 1\n    max_length: 20\n}\n",
    ),
    (
        "types/stock_code.jadpo",
        "/// A tool's stock code.\ntype StockCode = Text {\n    pattern: \"[A-Z]{3}\"\n}\n",
    ),
    (
        "types/hire_price.jadpo",
        "/// What a tool costs to hire.\ntype HirePrice = Decimal {\n    scale: 2\n    min: 0.00\n    max: 100.00\n}\n",
    ),
    (
        "types/wait.jadpo",
        "/// How long a tool takes to fetch.\ntype Wait = Duration {\n    min: 1m\n    max: 4h\n}\n",
    ),
    (
        "types/fee.jadpo",
        "/// A late fee.\ntype Fee = Money {\n    currency: Currency.gbp\n    min: 0\n}\n",
    ),
    (
        "entities/shed.jadpo",
        "use workshop/types { ToolName }\n\n/// Where tools are kept.\nentity Shed {\n    id: Uuid\n    name: ToolName\n\n    policy: {\n        read: [Access.public]\n    }\n}\n",
    ),
    (
        "entities/tool.jadpo",
        "use workshop/entities { Shed }\nuse workshop/types { Fee, HirePrice, Stars, StockCode, ToolName, Wait }\n\n/// A tool for hire.\nentity Tool {\n    id: Uuid\n    shed_id: Shed.id {\n        on_delete: OnDelete.cascade\n    }\n    name: ToolName\n    stars: Stars\n    sku: StockCode\n    price: HirePrice\n    wait: Wait\n    fee: Fee\n    contact: Email\n    note: Text {\n        max_length: 10\n    }\n    spot: Point\n\n    policy: {\n        read: [Access.public]\n        update: [Access.public]\n    }\n}\n",
    ),
    (
        "types/tool_view.jadpo",
        "use workshop/entities { Tool }\n\n/// A tool in a list.\ntype ToolView = Output {\n    pick: [Tool.id, Tool.name, Tool.stars, Tool.sku, Tool.price, Tool.wait, Tool.fee, Tool.contact, Tool.note]\n}\n",
    ),
    (
        "types/shed_view.jadpo",
        "use workshop/entities { Shed }\n\n/// A shed, briefly.\ntype ShedView = Output {\n    pick: [Shed.id, Shed.name]\n}\n",
    ),
    (
        "types/shed_page.jadpo",
        "use workshop/entities { Shed }\nuse workshop/types { ToolView }\n\n/// A shed and its tools.\ntype ShedPage = Output {\n    pick: [Shed.name]\n    tools: List<ToolView>\n}\n",
    ),
    (
        "types/tool_page.jadpo",
        "use workshop/entities { Tool }\nuse workshop/types { ShedView }\n\n/// A tool and its shed.\ntype ToolPage = Output {\n    pick: [Tool.name]\n    shed: ShedView\n}\n",
    ),
    (
        "types/sku_stars.jadpo",
        "use workshop/entities { Tool }\n\n/// The best tool of each stock code.\ntype SkuStars = Output {\n    sku: Tool.sku\n    best: Tool.stars\n    tools: Int\n}\n",
    ),
    (
        "types/pin.jadpo",
        "use workshop/entities { Tool }\nuse workshop/types { ToolName }\n\n/// What a tool's pin on the map says.\ntype Pin = Object {\n    id: Tool.id\n    name: ToolName\n}\n",
    ),
    (
        "failures/shed_not_found.jadpo",
        "/// No shed has that id.\ntype ShedNotFound = Failure {\n    kind: FailureKind.not_found\n}\n",
    ),
    (
        "failures/tool_not_found.jadpo",
        "/// No tool has that id.\ntype ToolNotFound = Failure {\n    kind: FailureKind.not_found\n}\n",
    ),
    (
        "queries/tools_by_name.jadpo",
        "use workshop/entities { Tool }\nuse workshop/types { ToolView }\n\n/// Every tool, by name.\nquery tools_by_name() -> List<ToolView> {\n    from: Tool\n    order: [Tool.name asc]\n    limit: 10\n}\n",
    ),
    (
        "queries/shed_with_tools.jadpo",
        "use workshop/entities { Shed, Tool }\nuse workshop/types { ShedPage }\nuse workshop/failures { ShedNotFound }\n\n/// A shed with its tools.\nquery shed_with_tools(shed_id: Shed.id) fails ShedNotFound -> ShedPage {\n    from: Shed\n    where: Shed.id == shed_id\n    missing: ShedNotFound\n    include: {\n        tools: {\n            via: Tool.shed_id\n            order: [Tool.name asc]\n            limit: 10\n        }\n    }\n}\n",
    ),
    (
        "queries/tool_with_shed.jadpo",
        "use workshop/entities { Tool }\nuse workshop/types { ToolPage }\nuse workshop/failures { ToolNotFound }\n\n/// A tool with its shed.\nquery tool_with_shed(tool_id: Tool.id) fails ToolNotFound -> ToolPage {\n    from: Tool\n    where: Tool.id == tool_id\n    missing: ToolNotFound\n    include: {\n        shed: Tool.shed_id\n    }\n}\n",
    ),
    (
        "queries/stars_by_sku.jadpo",
        "use workshop/entities { Tool }\nuse workshop/types { SkuStars }\n\n/// The best tool of each stock code.\nquery stars_by_sku() -> List<SkuStars> {\n    from: Tool\n    group: {\n        sku: Tool.sku\n    }\n    aggregate: {\n        best: max(Tool.stars)\n        tools: count()\n    }\n    order: [sku asc]\n    limit: 10\n}\n",
    ),
    (
        "queries/tool_layer.jadpo",
        "use workshop/entities { Tool }\nuse workshop/types { Pin }\n\n/// Every tool, as a map's layer.\nquery tool_layer() -> FeatureCollection<Point, Pin> {\n    from: Tool\n    geometry: Tool.spot\n    order: [Tool.name asc]\n    limit: 500\n}\n",
    ),
    (
        "actions/list_tools.jadpo",
        "use workshop/queries { tools_by_name }\nuse workshop/types { ToolView }\n\n/// Every tool.\naction list_tools() -> List<ToolView> {\n    return tools_by_name()\n}\n",
    ),
    (
        "actions/find_tool.jadpo",
        "use workshop/entities { Tool }\nuse workshop/failures { ToolNotFound }\n\n/// A tool by id.\naction find_tool(tool_id: Tool.id) fails ToolNotFound -> Tool {\n    return attempt Tool.get(tool_id)\n}\n",
    ),
    (
        "actions/open_shed.jadpo",
        "use workshop/entities { Shed }\nuse workshop/queries { shed_with_tools }\nuse workshop/types { ShedPage }\nuse workshop/failures { ShedNotFound }\n\n/// A shed and its tools.\naction open_shed(shed_id: Shed.id) fails ShedNotFound -> ShedPage {\n    return attempt shed_with_tools(shed_id)\n}\n",
    ),
    (
        "actions/tool_and_shed.jadpo",
        "use workshop/entities { Tool }\nuse workshop/queries { tool_with_shed }\nuse workshop/types { ToolPage }\nuse workshop/failures { ToolNotFound }\n\n/// A tool and its shed.\naction tool_and_shed(tool_id: Tool.id) fails ToolNotFound -> ToolPage {\n    return attempt tool_with_shed(tool_id)\n}\n",
    ),
    (
        "actions/best_by_sku.jadpo",
        "use workshop/queries { stars_by_sku }\nuse workshop/types { SkuStars }\n\n/// The best tool of each stock code.\naction best_by_sku() -> List<SkuStars> {\n    return stars_by_sku()\n}\n",
    ),
    (
        "actions/tool_pins.jadpo",
        "use workshop/queries { tool_layer }\nuse workshop/types { Pin }\n\n/// Every tool on the map.\naction tool_pins() -> FeatureCollection<Point, Pin> {\n    return tool_layer()\n}\n",
    ),
    (
        "actions/rename_tool.jadpo",
        "use workshop/entities { Tool }\nuse workshop/failures { ToolNotFound }\n\n/// Renames a tool, by id.\naction rename_tool(tool_id: Tool.id, name: Tool.name) fails ToolNotFound -> Tool {\n    return attempt Tool.update(tool_id) {\n        set: {\n            name: name\n        }\n    }\n}\n",
    ),
];

/// The workshop project, its first migration written.
fn workshop(name: &str) -> PathBuf {
    let root = folder(name);
    write(&root, "app.jadpo", "application {}\n");
    for (file, text) in WORKSHOP {
        write(&root, &format!("domains/workshop/{file}"), text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    root
}

/// A host whose one store is a PostgreSQL database.
struct Store(jadpo_host_native::Postgres);

impl Host for Store {
    fn database(&self, _store: &str) -> Option<&dyn Database> {
        Some(&self.0)
    }

    fn random(&self, _bytes: &mut [u8]) -> bool {
        false
    }
}

/// A UUID from a number, as version 4 lays one out.
fn uuid(n: u8) -> [u8; 16] {
    let mut id = [n; 16];
    id[6] = (id[6] & 0x0f) | 0x40;
    id[8] = (id[8] & 0x3f) | 0x80;
    id
}

/// Runs `sql` with `params` on the database at `url`, committed.
fn statement(url: &str, sql: &str, params: &[SqlValue]) {
    let store = jadpo_host_native::postgres(url).expect("store");
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    runtime.block_on(async {
        let mut transaction = store.begin().await.expect("begin");
        transaction
            .execute(sql, params)
            .await
            .unwrap_or_else(|e| panic!("{sql}: {e:?}"));
        transaction.commit().await.expect("commit");
    });
}

/// What the action `name` gives with `args`.
fn call(program: &Program, host: &Store, name: &str, args: Vec<Value>) -> Outcome {
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    let id = program.plan().find(name).expect("the action");
    runtime.block_on(async {
        let env = Env {
            seed: Some(1),
            clock: Clock::TEST,
        };
        let mut ctx = Ctx::new(program.places(), host, env);
        program.run(&mut ctx, id, args).await
    })
}

/// That `outcome` is a defect naming `names`, and never showing `hidden`.
fn broken(outcome: Outcome, read: &str, names: &str, hidden: &str) {
    let Outcome::Fault(fault) = outcome else {
        panic!("{read} gave {outcome:?}, not a defect naming {names}");
    };
    let why = format!("{:?}", fault.kind);
    assert!(why.contains("Defect"), "{read}: {why}");
    assert!(why.contains(names), "{read} names {names}: {why}");
    assert!(
        hidden.is_empty() || !why.contains(hidden),
        "{read} never shows {hidden}: {why}"
    );
}

#[test]
fn a_row_edited_outside_jadpo_to_break_a_fields_rule_is_a_defect_on_every_read() {
    if !jadpo_test::live() {
        return;
    }
    let root = workshop("reads");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server
        .create_database("field_rules_5_9d_reads")
        .expect("database");
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    let (shed, tool) = (uuid(1), uuid(2));
    statement(
        &url,
        "INSERT INTO shed (id, name) VALUES ($1, 'North shed')",
        &[SqlValue::Uuid(shed)],
    );
    statement(
        &url,
        "INSERT INTO tool (id, shed_id, name, stars, sku, price, wait, fee_amount, fee_currency, contact, note, spot) VALUES ($1, $2, 'Spade', 4, 'SPD', 12.50, 600000, 250, 'GBP', 'hire@example.com', 'Sharp', ST_GeogFromText('SRID=4326;POINT(-3.1347 54.6013)'))",
        &[SqlValue::Uuid(tool), SqlValue::Uuid(shed)],
    );
    let loaded = jadpo_db::load(&root);
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let host = Store(jadpo_host_native::postgres(&url).expect("store"));
    let tool_id = || Value::uuid(&jadpo_runtime::uuid_text(&tool));
    let shed_id = || Value::uuid(&jadpo_runtime::uuid_text(&shed));
    // Every read gives the row back while it keeps its rules.
    for (read, args) in [
        ("list_tools", vec![]),
        ("find_tool", vec![tool_id()]),
        ("open_shed", vec![shed_id()]),
        ("tool_and_shed", vec![tool_id()]),
        ("best_by_sku", vec![]),
        ("tool_pins", vec![]),
    ] {
        let outcome = call(&program, &host, read, args);
        assert!(matches!(outcome, Outcome::Value(_)), "{read}: {outcome:?}");
    }
    // Each edit, the column it puts back, what the defect names, and a
    // part of the edit it never shows.
    let cases: &[(&str, &str, &str, &str)] = &[
        (
            "stars = 77",
            "stars = 4",
            "`Tool.stars` that breaks `max: 5`",
            "77",
        ),
        ("stars = -3", "stars = 4", "`min: 1`", "-3"),
        (
            "name = 'Wholemeal sourdough spade'",
            "name = 'Spade'",
            "`Tool.name` that breaks `max_length: 20`",
            "sourdough",
        ),
        ("name = ''", "name = 'Spade'", "`min_length: 1`", ""),
        (
            "sku = 'zz9'",
            "sku = 'SPD'",
            "`Tool.sku` that breaks `pattern: ",
            "zz9",
        ),
        ("price = 123.45", "price = 12.50", "`max: 100.00`", "123.45"),
        ("wait = 18000000", "wait = 600000", "`max: 4h`", "18000000"),
        (
            "fee_amount = -250",
            "fee_amount = 250",
            "`Tool.fee` that breaks `min",
            "-250",
        ),
        (
            "contact = 'nobody at all'",
            "contact = 'hire@example.com'",
            "`Tool.contact` that breaks `Email`",
            "nobody at all",
        ),
        (
            "note = 'Far too long a note'",
            "note = 'Sharp'",
            "`Tool.note` that breaks `max_length: 10`",
            "Far too long",
        ),
    ];
    for (edit, back, names, hidden) in cases {
        statement(&url, &format!("UPDATE tool SET {edit}"), &[]);
        // A query, `get`, and a child's include.
        broken(
            call(&program, &host, "list_tools", vec![]),
            edit,
            names,
            hidden,
        );
        broken(
            call(&program, &host, "find_tool", vec![tool_id()]),
            edit,
            names,
            hidden,
        );
        broken(
            call(&program, &host, "open_shed", vec![shed_id()]),
            edit,
            names,
            hidden,
        );
        // The record an update gives back holds every stored field.
        if !edit.starts_with("name") {
            broken(
                call(
                    &program,
                    &host,
                    "rename_tool",
                    vec![tool_id(), Value::text("Shovel")],
                ),
                edit,
                names,
                hidden,
            );
        }
        // An aggregate's group, and its `max`.
        if edit.starts_with("sku") || edit.starts_with("stars") {
            broken(
                call(&program, &host, "best_by_sku", vec![]),
                edit,
                names,
                hidden,
            );
        }
        // A map layer's properties.
        if edit.starts_with("name") {
            broken(
                call(&program, &host, "tool_pins", vec![]),
                edit,
                names,
                hidden,
            );
        }
        statement(&url, &format!("UPDATE tool SET {back}"), &[]);
        let outcome = call(&program, &host, "find_tool", vec![tool_id()]);
        assert!(matches!(outcome, Outcome::Value(_)), "{back}: {outcome:?}");
    }
    // A parent's include.
    statement(
        &url,
        "UPDATE shed SET name = 'A shed with a long secret name'",
        &[],
    );
    broken(
        call(&program, &host, "tool_and_shed", vec![tool_id()]),
        "the shed's name",
        "`Shed.name` that breaks `max_length: 20`",
        "secret",
    );
    // The update ran nothing it shouldn't: the name is still the first.
    let outcome = call(&program, &host, "list_tools", vec![]);
    let Outcome::Value(Value::List(tools)) = outcome else {
        panic!("{outcome:?}");
    };
    let Value::Record(first) = &tools[0] else {
        panic!("{tools:?}");
    };
    assert_eq!(first.get("name"), Some(&Value::text("Spade")));
}

/// A help desk's articles, each with a meaning a nearest query searches.
fn help_desk(name: &str) -> PathBuf {
    let root = folder(name);
    write(&root, "app.jadpo", "application {}\n");
    for (file, text) in [
        (
            "types/article_meaning.jadpo",
            "/// A help article's meaning.\ntype ArticleMeaning = Vector {\n    dimensions: 3\n}\n",
        ),
        (
            "types/article_title.jadpo",
            "/// A help article's title.\ntype ArticleTitle = Text {\n    max_length: 12\n}\n",
        ),
        (
            "entities/help_article.jadpo",
            "use support/types { ArticleMeaning, ArticleTitle }\n\n/// An article on the help site.\nentity HelpArticle {\n    id: Uuid\n    title: ArticleTitle\n    body_meaning: ArticleMeaning\n\n    policy: {\n        read: [Access.public]\n    }\n}\n",
        ),
        (
            "types/article_excerpt.jadpo",
            "use support/entities { HelpArticle }\n\n/// An article in a list of answers.\ntype ArticleExcerpt = Output {\n    pick: [HelpArticle.id, HelpArticle.title]\n}\n",
        ),
        (
            "queries/closest_articles.jadpo",
            "use support/entities { HelpArticle }\nuse support/types { ArticleExcerpt, ArticleMeaning }\n\n/// The articles nearest a question.\nquery closest_articles(question_meaning: ArticleMeaning) -> List<Nearest<ArticleExcerpt>> {\n    from: HelpArticle\n    order: [HelpArticle.body_meaning nearest question_meaning]\n    limit: 5\n}\n",
        ),
        (
            "actions/answer_from.jadpo",
            "use support/queries { closest_articles }\nuse support/types { ArticleExcerpt, ArticleMeaning }\n\n/// The articles to answer a question from.\naction answer_from(question_meaning: ArticleMeaning) -> List<Nearest<ArticleExcerpt>> {\n    return closest_articles(question_meaning)\n}\n",
        ),
    ] {
        write(&root, &format!("domains/support/{file}"), text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    root
}

#[test]
fn a_nearest_querys_record_is_held_to_its_fields_rules() {
    if !jadpo_test::live() || crate::vectors::pgvector_too_old() {
        return;
    }
    let root = help_desk("nearest");
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server
        .create_database("field_rules_5_9d_nearest")
        .expect("database");
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    statement(
        &url,
        "INSERT INTO help_article (id, title, body_meaning) VALUES ($1, 'Refunds', '[1,0,0]')",
        &[SqlValue::Uuid(uuid(1))],
    );
    let loaded = jadpo_db::load(&root);
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let host = Store(jadpo_host_native::postgres(&url).expect("store"));
    let question = || jadpo_value::vector::from_floats(&[1.0, 0.5, 0.0]).expect("a vector");
    let outcome = call(&program, &host, "answer_from", vec![question()]);
    assert!(matches!(outcome, Outcome::Value(_)), "{outcome:?}");
    statement(
        &url,
        "UPDATE help_article SET title = 'Refunds and secret returns'",
        &[],
    );
    broken(
        call(&program, &host, "answer_from", vec![question()]),
        "a nearest query",
        "`HelpArticle.title` that breaks `max_length: 12`",
        "secret",
    );
}

#[test]
fn a_backfill_in_jadpo_reading_a_value_that_breaks_its_rule_stops() {
    if !jadpo_test::live() {
        return;
    }
    let root = folder("backfill");
    const TODO: &str = "domains/planning/entities/todo.jadpo";
    write(&root, "app.jadpo", "application {}\n");
    write(
        &root,
        "domains/planning/types/title.jadpo",
        "/// A todo's title.\ntype Title = Text {\n    max_length: 20\n}\n",
    );
    write(
        &root,
        TODO,
        "use planning/types { Title }\n\nentity Todo {\n    id: Uuid\n    title: Title\n}\n",
    );
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server
        .create_database("field_rules_5_9d_backfill")
        .expect("database");
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    statement(
        &url,
        "INSERT INTO todo (id, title) VALUES ($1, 'Rye loaf'), ($2, 'A title kept far too secretly long')",
        &[SqlValue::Uuid(uuid(1)), SqlValue::Uuid(uuid(2))],
    );
    write(
        &root,
        TODO,
        "use planning/types { Title }\n\nentity Todo {\n    id: Uuid\n    title: Title\n    shouted: Text\n}\n",
    );
    assert_eq!(jadpo(&root, &["migrate", "new", "add_shouted"]).code, 0);
    let mut names: Vec<String> = fs::read_dir(root.join("database/migrations"))
        .expect("migrations")
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect();
    names.sort();
    let name = names.pop().expect("a migration");
    // `text.upper` has no SQL form, so the backfill is worked out in Jadpo
    // from each row's stored title.
    write(
        &root,
        &format!("database/migrations/{name}/changes.jadpo"),
        "migration {\n    Todo.shouted: {\n        backfill: text.upper(Todo.title)\n    }\n}\n",
    );
    let finished = jadpo(&root, &["migrate", "new"]);
    assert_eq!(finished.code, 0, "{}{}", finished.stdout, finished.stderr);
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_ne!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    let said = format!("{}{}", applied.stdout, applied.stderr);
    assert!(said.contains("`max_length: 20`"), "{said}");
    assert!(said.contains("Todo.title"), "{said}");
    assert!(!said.contains("secretly"), "{said}");
}
