//! Vectors in PostgreSQL (area 21, "Embeddings and search"; chunk 5.5v1),
//! against live PostgreSQL with pgvector: an entity stores and reads back a
//! vector of each metric, a plain vector and an embedding unchanged, its
//! first migration creating the extension; `ai.distance` gives exactly
//! what pgvector's operator gives on the same vectors, for each metric;
//! `jadpo migrate apply` refuses a database that doesn't offer an
//! extension a migration creates before running anything, and the
//! start-up check refuses a store that lost one.
//!
//!
//! Nearest-neighbour queries (chunk 5.5v1b): a filtered nearest query of
//! five, run by the interpreter on PostgreSQL through its HNSW index,
//! returns the five records and distances `ai.distance` works out in code,
//! and a server whose pgvector is older than the 0.8.0 its iterative scans
//! need is refused before its index is built.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools, which must have pgvector (CI's
//! image does, `tools/postgres/Dockerfile`); `JADPO_SKIP_POSTGRES=1` skips
//! the live part. A nearest query needs pgvector 0.8.0: on a machine whose
//! server has an older one (Postgres.app ships 0.7.0) its live test is
//! skipped, saying so, except under CI, where it fails.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_interp::Program;
use jadpo_runtime::{
    Clock, Ctx, Database, Env, Executor, MigrateError, Migration, Outcome, SqlValue,
};
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

/// A project whose entity holds a vector of each metric, a plain vector
/// and an embedding, with a test that stores two and reads them back.
fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("vectors-{name}"));
    let _ = fs::remove_dir_all(&root);
    write(&root, "app.jadpo", "application {}\n");
    for (file, text) in [
        (
            "failures/bad_example.jadpo",
            "/// An example that isn't its type.\ntype BadExample = Failure {\n    kind: FailureKind.invalid_value\n}\n",
        ),
        (
            "types/article_meaning.jadpo",
            "/// An article's meaning, as the support site's model makes it.\ntype ArticleMeaning = Vector {\n    dimensions: 3\n}\n",
        ),
        (
            "types/taste_vector.jadpo",
            "/// A shop's place in a map of tastes, compared by straight-line distance.\ntype TasteVector = Vector {\n    dimensions: 3\n    distance: VectorDistance.euclidean\n}\n",
        ),
        (
            "types/score_vector.jadpo",
            "/// A normalised score, compared by inner product.\ntype ScoreVector = Vector {\n    dimensions: 3\n    distance: VectorDistance.inner_product\n}\n",
        ),
        (
            "entities/article.jadpo",
            "use search/types { ArticleMeaning, ScoreVector, TasteVector }\n\n/// An article with a vector of each kind.\nentity Article {\n    id: Uuid\n    meaning: ArticleMeaning\n    taste: TasteVector\n    score: ScoreVector\n    loose: Vector?\n    embedding: Embedding?\n}\n",
        ),
        (
            "actions/keep_article.jadpo",
            "use search/entities { Article }\nuse search/types { ArticleMeaning, ScoreVector, TasteVector }\n\n/// Keeps an article's vectors.\naction keep_article(meaning: ArticleMeaning, taste: TasteVector, score: ScoreVector, loose: Vector?, embedding: Embedding?) -> Article {\n    return Article.create {\n        set: {\n            meaning: meaning\n            taste: taste\n            score: score\n            loose: loose\n            embedding: embedding\n        }\n    }\n}\n",
        ),
        (
            "actions/keep_article.test.jadpo",
            "use search/entities { Article }\nuse search/failures { BadExample }\nuse search/types { ArticleMeaning, ScoreVector, TasteVector }\n\ntest \"vectors of every metric are stored and read back unchanged\" {\n    var meaning: ArticleMeaning = ArticleMeaning([0.5, -0.25, 0.123456789])\n    var taste: TasteVector = TasteVector([1250.5, 0.0000001, -3.0])\n    var score: ScoreVector = ScoreVector([0.1, 0.2, 0.3])\n    var loose: Vector = Vector([0.0123, -0.00456, 0.789, 16.0])\n    var embedding: Embedding = attempt json.read(\"{\\\"model\\\":\\\"openai/text-embedding-3-small\\\",\\\"vector\\\":[0.5,-0.25,1]}\", as: Embedding) {\n        invalid: BadExample\n    }\n    var kept: Article = keep_article(meaning, taste, score, loose, embedding)\n    var back: Article = attempt Article.get(kept.id)\n    assert back.meaning == meaning\n    assert back.taste == taste\n    assert back.score == score\n    assert back.loose == loose\n    assert back.embedding == embedding\n    assert ai.distance(back.taste, taste) == 0.0\n    var bare: Article = keep_article(meaning, taste, score, none, none)\n    var bare_back: Article = attempt Article.get(bare.id)\n    assert bare_back.loose == none\n    assert bare_back.embedding == none\n}\n",
        ),
    ] {
        write(&root, &format!("domains/search/{file}"), text);
    }
    root
}

/// The one migration's SQL.
fn migration_sql(root: &Path) -> (String, PathBuf) {
    let folder = fs::read_dir(root.join("database/migrations"))
        .expect("migrations")
        .map(|entry| entry.expect("entry").path())
        .next()
        .expect("a migration");
    let path = folder.join("postgres.sql");
    (fs::read_to_string(&path).expect("postgres.sql"), path)
}

#[test]
fn vectors_of_every_metric_are_stored_and_read_back_unchanged() {
    let root = project("stored");
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let (sql, _) = migration_sql(&root);
    // The extension first, then each vector in its column: a type's
    // dimensions fix its length, a plain vector has none, and an
    // embedding takes one for its model and one for its vector.
    assert!(
        sql.contains("-- store: main\nCREATE EXTENSION IF NOT EXISTS \"vector\";\nCREATE TABLE "),
        "{sql}"
    );
    for expected in [
        "\"meaning\" vector(3) NOT NULL",
        "\"taste\" vector(3) NOT NULL",
        "\"score\" vector(3) NOT NULL",
        "\"loose\" vector,",
        "\"embedding_model_provider\" text,",
        "\"embedding_model_name\" text,",
        "\"embedding_vector\" vector,",
    ] {
        assert!(sql.contains(expected), "`{expected}` in:\n{sql}");
    }
    if !jadpo_test::live() {
        return;
    }
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("1 passed"), "{}", tested.stdout);
}

/// A database on the run's server with pgvector created in it.
fn database(stem: &str) -> String {
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server.create_database(stem).expect("database");
    rows(&url, "CREATE EXTENSION IF NOT EXISTS vector", &[]);
    url
}

/// Runs `sql` with `params` on the database at `url`, committed.
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

/// A vector as the runtime holds it, from its items.
fn vector(items: &[f32]) -> Value {
    jadpo_value::vector::from_floats(items).expect("a vector")
}

fn bits(items: &[f32]) -> SqlValue {
    SqlValue::Vector(items.iter().map(|item| item.to_bits()).collect())
}

/// What pgvector gives for `metric` between `a` and `b`, and what
/// `ai.distance` gives, each as the double it is.
fn both(url: &str, a: &[f32], b: &[f32], metric: jadpo_std::VectorDistance) -> (f64, f64) {
    let sql = format!(
        "SELECT (($1::vector) {} ($2::vector))::text",
        metric.operator()
    );
    let found = rows(url, &sql, &[bits(a), bits(b)]);
    let Some(Some(SqlValue::Text(text))) = found.first().map(|row| row.first()) else {
        panic!("{sql} gave {found:?}");
    };
    let theirs: f64 = text.parse().expect("a double");
    let ours = jadpo_value::vector::distance(&vector(a), &vector(b), &Value::text(metric.name()))
        .expect("a distance");
    (theirs, ours.to_string().parse().expect("a decimal"))
}

#[test]
fn a_distance_in_code_is_pgvector_s_for_each_metric() {
    if !jadpo_test::live() {
        return;
    }
    let url = database("vector_distances");
    // A vector reads back from pgvector as the floats it was sent.
    let tricky = [0.1f32, -0.2, 0.123_456_79, 1e-7, 3.4e37, -0.0];
    let found = rows(&url, "SELECT $1::vector", &[bits(&tricky)]);
    assert_eq!(found, [[bits(&tricky)]]);
    // Items whose products and sums are exact floats, so the order and the
    // fusing pgvector's compiler chooses can't move a bit: the two agree
    // exactly.
    let long_a: Vec<f32> = (0..1536).map(|i| (i % 7) as f32 - 3.0).collect();
    let long_b: Vec<f32> = (0..1536).map(|i| (i % 5) as f32 * 0.5 - 1.0).collect();
    let exact: [(&[f32], &[f32]); 6] = [
        (&[1.0, 2.0, 3.0], &[4.0, -5.0, 6.0]),
        (&[0.5, -0.25, 1.0], &[0.25, 0.75, -1.0]),
        (&[3.0, 4.0], &[4.0, 3.0]),
        (&[1.0, 0.0, 0.0, 0.0], &[0.0, 1.0, 0.0, 0.0]),
        (&[2.0, 2.0], &[-2.0, -2.0]),
        (&long_a, &long_b),
    ];
    for metric in jadpo_std::VectorDistance::ALL {
        for (a, b) in exact {
            let (theirs, ours) = both(&url, a, b, *metric);
            // A decimal has no negative zero, where pgvector's inner
            // product of two vectors at right angles gives `-0`.
            assert!(
                ours.to_bits() == theirs.to_bits() || (ours == 0.0 && theirs == 0.0),
                "{} over {} items: pgvector {theirs}, code {ours}",
                metric.name(),
                a.len()
            );
        }
    }
    // Items like a model's, whose sums round: within a float's rounding of
    // the order pgvector's compiler adds them in.
    let model_a: Vec<f32> = (0..1536)
        .map(|i| ((i * 7919 % 2003) as f32 / 1001.0) - 1.0)
        .collect();
    let model_b: Vec<f32> = (0..1536)
        .map(|i| ((i * 104_729 % 1999) as f32 / 999.0) - 1.0)
        .collect();
    for metric in jadpo_std::VectorDistance::ALL {
        let (theirs, ours) = both(&url, &model_a, &model_b, *metric);
        assert!(
            (theirs - ours).abs() <= 1e-4 * theirs.abs().max(1.0),
            "{}: pgvector {theirs}, code {ours}",
            metric.name()
        );
    }
}

#[test]
fn a_database_without_an_extension_is_refused_before_anything_runs() {
    if !jadpo_test::live() {
        return;
    }
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server.create_database("vector_missing").expect("database");
    let root = project("missing");
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    // A server whose pgvector is missing, as a name no server offers.
    let (sql, path) = migration_sql(&root);
    fs::write(
        &path,
        sql.replace(
            "CREATE EXTENSION IF NOT EXISTS \"vector\"",
            "CREATE EXTENSION IF NOT EXISTS \"jadpo_missing\"",
        ),
    )
    .expect("write");
    let refused = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(refused.code, 1, "{}{}", refused.stdout, refused.stderr);
    assert_eq!(
        refused.stderr,
        "error: `main` has no `jadpo_missing` extension\n  reason: `jadpo_missing` is a PostgreSQL extension the project's columns need. The project's migrations create it, and its database server neither has it nor offers it, so nothing was applied.\n  next: Install `jadpo_missing` on the database server, or use a server that has it, then run the command again (human)\n  rule: migration.extension_missing\n  help: docs/language/14-tooling.html#command-errors\n"
    );
    let status = jadpo(&root, &["migrate", "status", "--url", &url]);
    assert!(
        status.stdout.contains("`main`: 0 applied, 1 pending"),
        "{}",
        status.stdout
    );
    // pgvector itself is offered, and its migration creates it.
    fs::write(&path, sql).expect("write");
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    let installed = rows(
        &url,
        "SELECT extname FROM pg_extension WHERE extname = 'vector'",
        &[],
    );
    assert_eq!(installed, [[SqlValue::Text("vector".into())]]);
}

/// `jadpo test` refuses a server without an extension the migrations
/// create before any test runs, as `migrate apply` does, rather than
/// each test faulting on the database it couldn't be given (chunk 5.7c).
#[test]
fn jadpo_test_refuses_a_server_without_an_extension_before_any_test_runs() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("test-missing");
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    let (sql, path) = migration_sql(&root);
    fs::write(
        &path,
        sql.replace(
            "CREATE EXTENSION IF NOT EXISTS \"vector\"",
            "CREATE EXTENSION IF NOT EXISTS \"jadpo_missing\"",
        ),
    )
    .expect("write");
    let refused = jadpo(&root, &["test", "--json", "--seed", "1"]);
    assert_eq!(refused.code, 1, "{}{}", refused.stdout, refused.stderr);
    let document: serde_json::Value = serde_json::from_str(&refused.stdout).expect("json");
    let rules: Vec<&str> = document["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .filter_map(|diagnostic| diagnostic["rule"].as_str())
        .collect();
    assert_eq!(rules, ["migration.extension_missing"], "{}", refused.stdout);
    assert_eq!(document["data"], serde_json::Value::Null, "no test ran");
    let shown = jadpo(&root, &["test"]);
    assert!(
        shown
            .stdout
            .contains("error: `main` has no `jadpo_missing` extension\n"),
        "{}{}",
        shown.stdout,
        shown.stderr
    );
}

#[test]
fn the_start_up_check_refuses_a_store_that_lost_its_extension() {
    if !jadpo_test::live() {
        return;
    }
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server.create_database("vector_start_up").expect("database");
    let migrations = [Migration {
        name: "20261020T1400_initial".into(),
        digest: "d".into(),
        sql: "CREATE EXTENSION IF NOT EXISTS \"vector\";\nCREATE TABLE \"article\" (\"id\" uuid NOT NULL, \"meaning\" vector(3))".into(),
    }];
    let store = jadpo_host_native::postgres(&url).expect("store");
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    runtime.block_on(async {
        jadpo_runtime::migrate(&store, &migrations)
            .await
            .expect("migrated");
        jadpo_runtime::has_extensions(&store, &migrations)
            .await
            .expect("has pgvector");
    });
    rows(&url, "DROP EXTENSION vector CASCADE", &[]);
    let checked = runtime.block_on(jadpo_runtime::has_extensions(&store, &migrations));
    assert_eq!(
        checked,
        Err(MigrateError::ExtensionMissing {
            extension: "vector".into()
        })
    );
}

// --- nearest-neighbour queries (chunk 5.5v1b) --------------------------------------

/// Items of `[-1, 1]` in sixteenths: their products and every running sum
/// of eight are exact floats, so pgvector's distance and the code's agree
/// to the bit (area 21, "Storing a vector").
fn sixteenths(seed: u64, count: usize) -> Vec<f32> {
    let mut state = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
    (0..count)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((state >> 33) % 33) as f32 / 16.0 - 1.0
        })
        .collect()
}

/// A project of help articles with an 8-item meaning compared by cosine,
/// and the page's query: public articles, none further than 0.6, the five
/// nearest.
fn nearest_project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("vectors-{name}"));
    let _ = fs::remove_dir_all(&root);
    write(&root, "app.jadpo", "application {}\n");
    for (file, text) in [
        (
            "types/article_meaning.jadpo",
            "/// A help article's meaning, as the configured embedding model makes it.\ntype ArticleMeaning = Vector {\n    dimensions: 8\n}\n",
        ),
        (
            "types/article_title.jadpo",
            "/// A help article's title.\ntype ArticleTitle = Text {\n    max_length: 120\n}\n",
        ),
        (
            "entities/help_article.jadpo",
            "use support/types { ArticleMeaning, ArticleTitle }\n\n/// An article on the help site, with its meaning once it is embedded.\nentity HelpArticle {\n    id: Uuid\n    title: ArticleTitle\n    shown_publicly: Bool\n    body_meaning: ArticleMeaning?\n\n    policy: {\n        read: [Access.public]\n    }\n}\n",
        ),
        (
            "types/article_excerpt.jadpo",
            "use support/entities { HelpArticle }\n\n/// What a reader sees of a help article in a list of answers.\ntype ArticleExcerpt = Output {\n    pick: [HelpArticle.id, HelpArticle.title]\n}\n",
        ),
        (
            "queries/closest_articles.jadpo",
            "use support/entities { HelpArticle }\nuse support/types { ArticleExcerpt, ArticleMeaning }\n\n/// The five public articles nearest a question, none further than 0.6.\nquery closest_articles(question_meaning: ArticleMeaning) -> List<Nearest<ArticleExcerpt>> {\n    from: HelpArticle\n    where: HelpArticle.shown_publicly == true\n    where: ai.distance(HelpArticle.body_meaning, question_meaning) <= 0.6\n    order: [HelpArticle.body_meaning nearest question_meaning]\n    limit: 5\n}\n",
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

/// The pgvector the run's server offers, as its three numbers.
fn offered_pgvector() -> [u32; 3] {
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server.create_database("vector_version").expect("database");
    let found = rows(
        &url,
        "SELECT default_version FROM pg_available_extensions WHERE name = 'vector'",
        &[],
    );
    match found.first().and_then(|row| row.first()) {
        Some(SqlValue::Text(text)) => {
            jadpo_std::migration::version(text).unwrap_or_else(|| panic!("version {text}"))
        }
        other => panic!("no pgvector: {other:?}"),
    }
}

/// Whether the run's server is too old for a nearest query's live test:
/// skipped locally, saying so; a failure under CI, whose image has 0.8.7.
pub(crate) fn pgvector_too_old() -> bool {
    let offered = offered_pgvector();
    if offered >= [0, 8, 0] {
        return false;
    }
    let shown = jadpo_std::migration::version_text(offered);
    jadpo_test::skip_for_version(format_args!(
        "the server offers pgvector {shown}, and nearest queries need 0.8.0"
    ));
    true
}

#[test]
fn a_nearest_query_s_migration_builds_its_hnsw_index() {
    let root = nearest_project("nearest-index");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let (sql, _) = migration_sql(&root);
    assert!(
        sql.contains("CREATE INDEX \"help_article_body_meaning_idx\" ON \"help_article\" USING hnsw (\"body_meaning\" vector_cosine_ops);"),
        "{sql}"
    );
}

#[test]
fn a_filtered_nearest_query_returns_the_five_code_works_out_through_its_index() {
    if !jadpo_test::live() || pgvector_too_old() {
        return;
    }
    let root = nearest_project("nearest-five");
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server.create_database("vector_nearest").expect("database");
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    // A thousand articles: a third private, one in fifty without a meaning.
    let question = sixteenths(7, 8);
    let mut articles: Vec<([u8; 16], bool, Option<Vec<f32>>)> = Vec::new();
    for i in 0..1000u64 {
        let mut id = [0u8; 16];
        id[..8].copy_from_slice(&(i + 1).to_be_bytes());
        id[8..].copy_from_slice(&(i.wrapping_mul(2_654_435_761)).to_be_bytes());
        id[6] = (id[6] & 0x0f) | 0x40;
        id[8] = (id[8] & 0x3f) | 0x80;
        let meaning = (i % 50 != 0).then(|| sixteenths(i + 100, 8));
        let meaning = meaning.filter(|items| items.iter().any(|item| *item != 0.0));
        articles.push((id, i % 3 != 0, meaning));
    }
    let store = jadpo_host_native::postgres(&url).expect("store");
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    runtime.block_on(async {
        let mut transaction = store.begin().await.expect("begin");
        for (i, (id, shown, meaning)) in articles.iter().enumerate() {
            transaction
                .execute(
                    "INSERT INTO help_article (id, title, shown_publicly, body_meaning) VALUES ($1, $2, $3, $4)",
                    &[
                        SqlValue::Uuid(*id),
                        SqlValue::Text(format!("Article {i}")),
                        SqlValue::Bool(*shown),
                        match meaning {
                            Some(items) => bits(items),
                            None => SqlValue::Null(jadpo_runtime::SqlType::Vector),
                        },
                    ],
                )
                .await
                .expect("insert");
        }
        transaction.commit().await.expect("commit");
    });
    rows(&url, "ANALYZE help_article", &[]);
    // What code works out: `ai.distance` from the question to each public
    // article with a meaning, those within 0.6, nearest first.
    let mut expected: Vec<(f64, String)> = articles
        .iter()
        .filter(|(_, shown, _)| *shown)
        .filter_map(|(id, _, meaning)| {
            let meaning = meaning.as_ref()?;
            let found = jadpo_value::vector::distance(
                &vector(meaning),
                &vector(&question),
                &Value::text("cosine"),
            )
            .expect("a distance");
            let found: f64 = found.to_string().parse().expect("a decimal");
            (found <= 0.6).then(|| (found, jadpo_runtime::uuid_text(id)))
        })
        .collect();
    expected.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    assert!(expected.len() > 5, "more than five match");
    assert!(expected[4].0 < expected[5].0, "no tie at the fifth");
    expected.truncate(5);
    // The query, called by an action the interpreter runs on PostgreSQL.
    let loaded = jadpo_db::load(&root);
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let id = program.plan().find("answer_from").expect("the action");
    let host = Harness::postgres(&url);
    let found = runtime.block_on(async {
        let env = Env {
            seed: Some(1),
            clock: Clock::TEST,
        };
        let mut ctx = Ctx::new(program.places(), &host, env);
        program.run(&mut ctx, id, vec![vector(&question)]).await
    });
    let Outcome::Value(Value::List(found)) = found else {
        panic!("expected a list, got {found:?}");
    };
    let found: Vec<(String, String)> = found
        .iter()
        .map(|nearest| {
            let Value::Record(nearest) = nearest else {
                panic!("not a record: {nearest:?}");
            };
            let Some(Value::Record(item)) = nearest.get("item") else {
                panic!("no item: {nearest:?}");
            };
            let id = item.get("id").expect("an id").to_string();
            let id = id.trim_matches('"').to_string();
            let distance = nearest.get("distance").expect("a distance").to_string();
            (distance, id)
        })
        .collect();
    let wanted: Vec<(String, String)> = expected
        .iter()
        .map(|(distance, id)| {
            (
                jadpo_std::standard::ai::distance_text(*distance),
                id.clone(),
            )
        })
        .collect();
    assert_eq!(found, wanted, "the same five, with the same distances");
    // Its plan reads the HNSW index.
    let plan = program
        .plan()
        .query_named("closest_articles")
        .expect("the query's plan");
    let params: Vec<SqlValue> = plan
        .read
        .select
        .params
        .iter()
        .map(|param| match &param.source {
            jadpo_std::read::Source::Arg { .. } => bits(&question),
            jadpo_std::read::Source::Stored(value) => value.clone(),
            other => panic!("a parameter from {other:?}"),
        })
        .collect();
    let explained = runtime.block_on(async {
        let mut transaction = store.begin().await.expect("begin");
        for setting in &plan.read.settings {
            transaction.batch(setting).await.expect("a setting");
        }
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
            .any(|line| line.contains("Index Scan using help_article_body_meaning_idx")),
        "{}",
        lines.join("\n")
    );
}

#[test]
fn an_older_pgvector_is_refused_before_a_nearest_query_s_index_is_built() {
    if !jadpo_test::live() {
        return;
    }
    let root = nearest_project("nearest-version");
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server
        .create_database("vector_version_check")
        .expect("database");
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    let offered = offered_pgvector();
    if offered >= [0, 8, 0] {
        // A server new enough applies it, index and all.
        assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
        let built = rows(
            &url,
            "SELECT indexdef FROM pg_indexes WHERE indexname = 'help_article_body_meaning_idx'",
            &[],
        );
        assert!(
            matches!(built.first().and_then(|row| row.first()), Some(SqlValue::Text(def)) if def.contains("USING hnsw")),
            "{built:?}"
        );
        return;
    }
    let shown = jadpo_std::migration::version_text(offered);
    assert_eq!(applied.code, 1, "{}{}", applied.stdout, applied.stderr);
    assert_eq!(
        applied.stderr,
        format!(
            "error: `main` has pgvector {shown}, and its nearest-neighbour queries need 0.8.0\n  reason: The project's migrations build an HNSW index for a `nearest` query, which runs with pgvector's iterative index scans so a filtered search still fills its limit; they came in pgvector 0.8.0, and the database server offers {shown}, so nothing was applied.\n  next: Upgrade pgvector on the database server to 0.8.0 or later (the major hosted services offer it), then run the command again (human)\n  rule: migration.extension_outdated\n  help: docs/language/14-tooling.html#command-errors\n"
        )
    );
    let status = jadpo(&root, &["migrate", "status", "--url", &url]);
    assert!(
        status.stdout.contains("`main`: 0 applied, 1 pending"),
        "{}",
        status.stdout
    );
}

// --- longer vectors and tuning (chunk 5.5v1c) --------------------------------

/// A project of help articles whose meaning has `dimensions`, measured by
/// cosine, with `indexes` on the entity and the five nearest articles a
/// question, `decline` written above the query's `order:`.
fn search_project(name: &str, dimensions: u32, indexes: &str, decline: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("vectors-{name}"));
    let _ = fs::remove_dir_all(&root);
    write(&root, "app.jadpo", "application {}\n");
    for (file, text) in [
        (
            "types/article_meaning.jadpo".to_string(),
            format!(
                "/// A help article's meaning, as the configured embedding model makes it.\ntype ArticleMeaning = Vector {{\n    dimensions: {dimensions}\n}}\n"
            ),
        ),
        (
            "types/article_title.jadpo".to_string(),
            "/// A help article's title.\ntype ArticleTitle = Text {\n    max_length: 120\n}\n"
                .to_string(),
        ),
        (
            "entities/help_article.jadpo".to_string(),
            format!(
                "use support/types {{ ArticleMeaning, ArticleTitle }}\n\n/// An article on the help site, with its meaning once it is embedded.\nentity HelpArticle {{\n    id: Uuid\n    title: ArticleTitle\n    body_meaning: ArticleMeaning?\n\n    policy: {{\n        read: [Access.public]\n    }}\n{indexes}}}\n"
            ),
        ),
        (
            "types/article_excerpt.jadpo".to_string(),
            "use support/entities { HelpArticle }\n\n/// What a reader sees of a help article in a list of answers.\ntype ArticleExcerpt = Output {\n    pick: [HelpArticle.id, HelpArticle.title]\n}\n"
                .to_string(),
        ),
        (
            "queries/closest_articles.jadpo".to_string(),
            format!(
                "use support/entities {{ HelpArticle }}\nuse support/types {{ ArticleExcerpt, ArticleMeaning }}\n\n/// The five articles nearest a question.\nquery closest_articles(question_meaning: ArticleMeaning) -> List<Nearest<ArticleExcerpt>> {{\n    from: HelpArticle\n{decline}    order: [HelpArticle.body_meaning nearest question_meaning]\n    limit: 5\n}}\n"
            ),
        ),
        (
            "actions/answer_from.jadpo".to_string(),
            "use support/queries { closest_articles }\nuse support/types { ArticleExcerpt, ArticleMeaning }\n\n/// The articles to answer a question from.\naction answer_from(question_meaning: ArticleMeaning) -> List<Nearest<ArticleExcerpt>> {\n    return closest_articles(question_meaning)\n}\n"
                .to_string(),
        ),
    ] {
        write(&root, &format!("domains/support/{file}"), &text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    root
}

/// An article's `id` and its meaning, if it has one.
type Article = ([u8; 16], Option<Vec<f32>>);

/// A question of `dimensions` and `count` articles: the first five near it,
/// the `k`th with `k` items a sixteenth away, so each is nearer than the
/// next; the rest elsewhere, one in ten without a meaning.
fn planted(dimensions: usize, count: u64) -> (Vec<f32>, Vec<Article>) {
    let question = sixteenths(7, dimensions);
    let mut articles = Vec::new();
    for i in 0..count {
        let mut id = [0u8; 16];
        id[..8].copy_from_slice(&(i + 1).to_be_bytes());
        id[8..].copy_from_slice(&(i.wrapping_mul(2_654_435_761)).to_be_bytes());
        id[6] = (id[6] & 0x0f) | 0x40;
        id[8] = (id[8] & 0x3f) | 0x80;
        let meaning = if i < 5 {
            let mut near = question.clone();
            for item in near.iter_mut().take(usize::try_from(i).expect("small") + 1) {
                *item = if *item > 0.5 {
                    *item - 0.0625
                } else {
                    *item + 0.0625
                };
            }
            Some(near)
        } else if i % 10 == 0 {
            None
        } else {
            Some(sixteenths(i + 100, dimensions))
        };
        articles.push((id, meaning));
    }
    (question, articles)
}

/// Applies the project's migration to a new database and stores
/// `articles` in it; the database's address.
fn stored(root: &Path, stem: &str, articles: &[Article]) -> String {
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server.create_database(stem).expect("database");
    let applied = jadpo(root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    let store = jadpo_host_native::postgres(&url).expect("store");
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    runtime.block_on(async {
        let mut transaction = store.begin().await.expect("begin");
        for (i, (id, meaning)) in articles.iter().enumerate() {
            transaction
                .execute(
                    "INSERT INTO help_article (id, title, body_meaning) VALUES ($1, $2, $3)",
                    &[
                        SqlValue::Uuid(*id),
                        SqlValue::Text(format!("Article {i}")),
                        match meaning {
                            Some(items) => bits(items),
                            None => SqlValue::Null(jadpo_runtime::SqlType::Vector),
                        },
                    ],
                )
                .await
                .expect("insert");
        }
        transaction.commit().await.expect("commit");
    });
    rows(&url, "ANALYZE help_article", &[]);
    url
}

/// The five `ai.distance` puts nearest `question`, ties by `id`, each as
/// its distance's text and its `id`.
fn nearest_in_code(question: &[f32], articles: &[Article]) -> Vec<(String, String)> {
    let mut expected: Vec<(f64, String)> = articles
        .iter()
        .filter_map(|(id, meaning)| {
            let found = jadpo_value::vector::distance(
                &vector(meaning.as_ref()?),
                &vector(question),
                &Value::text("cosine"),
            )
            .expect("a distance");
            Some((
                found.to_string().parse().expect("a decimal"),
                jadpo_runtime::uuid_text(id),
            ))
        })
        .collect();
    expected.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    assert!(expected[4].0 < expected[5].0, "no tie at the fifth");
    expected
        .into_iter()
        .take(5)
        .map(|(distance, id)| (jadpo_std::standard::ai::distance_text(distance), id))
        .collect()
}

/// What `answer_from` returns for `question`, run by the interpreter on
/// the database at `url`, each as its distance's text and its `id`.
fn answered(root: &Path, url: &str, question: &[f32]) -> Vec<(String, String)> {
    let loaded = jadpo_db::load(root);
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let id = program.plan().find("answer_from").expect("the action");
    let host = Harness::postgres(url);
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    let found = runtime.block_on(async {
        let env = Env {
            seed: Some(1),
            clock: Clock::TEST,
        };
        let mut ctx = Ctx::new(program.places(), &host, env);
        program.run(&mut ctx, id, vec![vector(question)]).await
    });
    let Outcome::Value(Value::List(found)) = found else {
        panic!("expected a list, got {found:?}");
    };
    found
        .iter()
        .map(|nearest| {
            let Value::Record(nearest) = nearest else {
                panic!("not a record: {nearest:?}");
            };
            let Some(Value::Record(item)) = nearest.get("item") else {
                panic!("no item: {nearest:?}");
            };
            let id = item.get("id").expect("an id").to_string();
            let distance = nearest.get("distance").expect("a distance").to_string();
            (distance, id.trim_matches('"').to_string())
        })
        .collect()
}

/// The plan PostgreSQL makes for `closest_articles` on `question`, after
/// its own settings and `more`.
fn explained(root: &Path, url: &str, question: &[f32], more: &[&str]) -> Vec<String> {
    let loaded = jadpo_db::load(root);
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let plan = program
        .plan()
        .query_named("closest_articles")
        .expect("the query's plan");
    let params: Vec<SqlValue> = plan
        .read
        .select
        .params
        .iter()
        .map(|param| match &param.source {
            jadpo_std::read::Source::Arg { .. } => bits(question),
            jadpo_std::read::Source::Stored(value) => value.clone(),
            other => panic!("a parameter from {other:?}"),
        })
        .collect();
    let store = jadpo_host_native::postgres(url).expect("store");
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    let rows = runtime.block_on(async {
        let mut transaction = store.begin().await.expect("begin");
        for setting in plan
            .read
            .settings
            .iter()
            .map(String::as_str)
            .chain(more.iter().copied())
        {
            transaction.batch(setting).await.expect("a setting");
        }
        let rows = transaction
            .query(&format!("EXPLAIN {}", plan.read.select.sql), &params)
            .await
            .expect("explain");
        let _ = transaction.rollback().await;
        rows
    });
    rows.iter()
        .filter_map(|row| match row.first() {
            Some(SqlValue::Text(line)) => Some(line.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_longer_vector_s_migration_indexes_it_at_half_precision() {
    let root = search_project("half-index", 3072, "", "");
    let (sql, _) = migration_sql(&root);
    assert!(
        sql.contains("CREATE INDEX \"help_article_body_meaning_idx\" ON \"help_article\" USING hnsw ((\"body_meaning\"::halfvec(3072)) halfvec_cosine_ops);"),
        "{sql}"
    );
    assert!(
        sql.contains("\"body_meaning\" vector(3072)"),
        "stored in full: {sql}"
    );
}

#[test]
fn a_3072_dimension_nearest_query_reads_its_half_precision_index() {
    if !jadpo_test::live() || pgvector_too_old() {
        return;
    }
    let root = search_project("half-nearest", 3072, "", "");
    let (question, articles) = planted(3072, 300);
    let url = stored(&root, "vector_half_nearest", &articles);
    // The five code works out, with the full-precision distances.
    assert_eq!(
        answered(&root, &url, &question),
        nearest_in_code(&question, &articles),
        "the same five, with the same distances"
    );
    // Its order is the expression the index holds. A table this small is
    // cheaper to read whole, so sequential scans are turned off to see
    // that the index serves the order at all.
    let lines = explained(&root, &url, &question, &["SET LOCAL enable_seqscan = off"]);
    assert!(
        lines
            .iter()
            .any(|line| line.contains("Index Scan using help_article_body_meaning_idx")),
        "{}",
        lines.join("\n")
    );
}

#[test]
fn a_vector_too_long_to_index_is_searched_exactly_once_declined() {
    let decline = "    decline query.nearest_exact: \"A few hundred articles, read in full\"\n";
    let root = search_project("exact", 5000, "", decline);
    let (sql, _) = migration_sql(&root);
    assert!(!sql.contains("USING hnsw"), "no index: {sql}");
    if !jadpo_test::live() {
        return;
    }
    // No index, so any pgvector serves it.
    let (question, articles) = planted(5000, 60);
    let url = stored(&root, "vector_exact", &articles);
    assert_eq!(
        answered(&root, &url, &question),
        nearest_in_code(&question, &articles),
        "the same five, with the same distances"
    );
    let lines = explained(&root, &url, &question, &[]);
    assert!(
        lines
            .iter()
            .any(|line| line.contains("Seq Scan on help_article")),
        "{}",
        lines.join("\n")
    );
}

#[test]
fn a_tuned_hnsw_index_s_migration_sets_m_and_ef_construction() {
    let indexes = "\n    indexes: {\n        by_meaning: {\n            fields: [body_meaning]\n            kind: IndexKind.hnsw\n            m: 24\n            ef_construction: 128\n        }\n    }\n";
    let root = search_project("tuned", 8, indexes, "");
    let (sql, _) = migration_sql(&root);
    assert!(
        sql.contains("CREATE INDEX \"help_article_by_meaning_idx\" ON \"help_article\" USING hnsw (\"body_meaning\" vector_cosine_ops) WITH (m = 24, ef_construction = 128);"),
        "{sql}"
    );
    assert_eq!(
        sql.matches("USING hnsw").count(),
        1,
        "the declared index serves the query: {sql}"
    );
    if !jadpo_test::live() || pgvector_too_old() {
        return;
    }
    let (question, articles) = planted(8, 200);
    let url = stored(&root, "vector_tuned", &articles);
    let built = rows(
        &url,
        "SELECT indexdef FROM pg_indexes WHERE indexname = 'help_article_by_meaning_idx'",
        &[],
    );
    assert!(
        matches!(built.first().and_then(|row| row.first()), Some(SqlValue::Text(def)) if def.contains("WITH (m='24', ef_construction='128')")),
        "{built:?}"
    );
    assert_eq!(
        answered(&root, &url, &question),
        nearest_in_code(&question, &articles)
    );
}

// --- distances that are no number (chunk 5.8c) ---------------------------------------

/// A project of notes with a cosine meaning and an inner-product score,
/// and queries on how far each is from a given one: past a literal, and
/// within a bound the caller gives.
fn far_project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("vectors-{name}"));
    let _ = fs::remove_dir_all(&root);
    write(&root, "app.jadpo", "application {}\n");
    for (file, text) in [
        (
            "types/meaning.jadpo",
            "/// A note's meaning.\ntype Meaning = Vector {\n    dimensions: 2\n    distance: VectorDistance.cosine\n}\n",
        ),
        (
            "types/score.jadpo",
            "/// A note's score, compared by inner product.\ntype Score = Vector {\n    dimensions: 2\n    distance: VectorDistance.inner_product\n}\n",
        ),
        (
            "types/reach.jadpo",
            "/// How far a search reaches.\ntype Reach = Decimal {\n    scale: 2\n}\n",
        ),
        (
            "entities/note.jadpo",
            "use notes/types { Meaning, Score }\n\n/// A note.\nentity Note {\n    id: Uuid\n    meaning: Meaning\n    score: Score\n\n    policy: {\n        read: [Access.public]\n    }\n}\n",
        ),
        (
            "types/note_card.jadpo",
            "use notes/entities { Note }\n\n/// A note in a list.\ntype NoteCard = Output {\n    pick: [Note.id]\n}\n",
        ),
        (
            "queries/far_notes.jadpo",
            "use notes/entities { Note }\nuse notes/types { Meaning, NoteCard }\n\n/// Notes far from a meaning.\nquery far_notes(from_meaning: Meaning) -> List<NoteCard> {\n    from: Note\n    where: ai.distance(Note.meaning, from_meaning) >= 0.5\n    order: [Note.id asc]\n    limit: 10\n}\n",
        ),
        (
            "queries/near_notes.jadpo",
            "use notes/entities { Note }\nuse notes/types { Meaning, NoteCard, Reach }\n\n/// Notes within a reach of a meaning.\nquery near_notes(from_meaning: Meaning, reach: Reach) -> List<NoteCard> {\n    from: Note\n    where: ai.distance(Note.meaning, from_meaning) <= reach\n    order: [Note.id asc]\n    limit: 10\n}\n",
        ),
        (
            "queries/scored_notes.jadpo",
            "use notes/entities { Note }\nuse notes/types { NoteCard, Score }\n\n/// Notes scoring at least as close as nothing to a score.\nquery scored_notes(from_score: Score) -> List<NoteCard> {\n    from: Note\n    where: ai.distance(Note.score, from_score) <= 0.0\n    order: [Note.id asc]\n    limit: 10\n}\n",
        ),
        (
            "actions/find_far.jadpo",
            "use notes/queries { far_notes }\nuse notes/types { Meaning, NoteCard }\n\n/// The far notes.\naction find_far(from_meaning: Meaning) -> List<NoteCard> {\n    return far_notes(from_meaning)\n}\n",
        ),
        (
            "actions/find_near.jadpo",
            "use notes/queries { near_notes }\nuse notes/types { Meaning, NoteCard, Reach }\n\n/// The near notes.\naction find_near(from_meaning: Meaning, reach: Reach) -> List<NoteCard> {\n    return near_notes(from_meaning, reach)\n}\n",
        ),
        (
            "actions/find_scored.jadpo",
            "use notes/queries { scored_notes }\nuse notes/types { NoteCard, Score }\n\n/// The scored notes.\naction find_scored(from_score: Score) -> List<NoteCard> {\n    return scored_notes(from_score)\n}\n",
        ),
    ] {
        write(&root, &format!("domains/notes/{file}"), text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    root
}

/// The ids of the cards an action returns.
fn card_ids(found: Outcome) -> Vec<String> {
    let Outcome::Value(Value::List(found)) = found else {
        panic!("expected a list, got {found:?}");
    };
    found
        .iter()
        .map(|card| {
            let Value::Record(card) = card else {
                panic!("not a record: {card:?}");
            };
            card.get("id")
                .expect("an id")
                .to_string()
                .trim_matches('"')
                .to_string()
        })
        .collect()
}

#[test]
fn a_distance_that_is_no_number_meets_no_condition() {
    if !jadpo_test::live() {
        return;
    }
    let root = far_project("far");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let url = database("vector_far");
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    // Notes written outside Jadpo: a meaning of zeros, whose cosine is
    // `NaN`, and a score whose inner product with itself is past a
    // float's range, minus infinity.
    let id = |n: u8| {
        let mut id = [0u8; 16];
        id[15] = n;
        id[6] = 0x40;
        id[8] = 0x80;
        id
    };
    let notes: [(u8, [f32; 2], [f32; 2]); 4] = [
        (1, [0.0, 0.0], [3e19, 0.0]),
        (2, [1.0, 0.0], [1.0, 0.0]),
        (3, [0.0, 1.0], [0.0, 1.0]),
        (4, [-1.0, 0.0], [-1.0, 0.0]),
    ];
    for (n, meaning, score) in &notes {
        rows(
            &url,
            "INSERT INTO note (id, meaning, score) VALUES ($1, $2, $3)",
            &[SqlValue::Uuid(id(*n)), bits(meaning), bits(score)],
        );
    }
    let shown = |n: u8| jadpo_runtime::uuid_text(&id(n));
    let loaded = jadpo_db::load(&root);
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let host = Harness::postgres(&url);
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    let call = |action: &str, args: Vec<Value>| {
        let id = program.plan().find(action).expect("the action");
        runtime.block_on(async {
            let env = Env {
                seed: Some(1),
                clock: Clock::TEST,
            };
            let mut ctx = Ctx::new(program.places(), &host, env);
            program.run(&mut ctx, id, args).await
        })
    };
    // At least 0.5 from (1, 0): the notes at right angles and opposite,
    // never the one of zeros, whose distance PostgreSQL sorts above every
    // number.
    assert_eq!(
        card_ids(call("find_far", vec![vector(&[1.0, 0.0])])),
        [shown(3), shown(4)]
    );
    // Within a reach the caller gives, sent as the double that agrees.
    let reach = Value::decimal("1.00").expect("a decimal");
    assert_eq!(
        card_ids(call("find_near", vec![vector(&[1.0, 0.0]), reach])),
        [shown(2), shown(3)]
    );
    // An inner product past a float's range is minus infinity, which
    // meets no bound either.
    assert_eq!(
        card_ids(call("find_scored", vec![vector(&[3e19, 0.0])])),
        [shown(2), shown(3)]
    );
}

/// A filtered nearest query visits as many records as filling its limit
/// takes (chunk 5.8c): 30,000 notes nearer than the five it wants, which
/// its condition turns away, are more than pgvector's default
/// `hnsw.max_scan_tuples` of 20,000, which would end the scan with none.
#[test]
fn a_filtered_nearest_query_fills_its_limit_past_twenty_thousand_records() {
    if !jadpo_test::live() || pgvector_too_old() {
        return;
    }
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("vectors-scan-tuples");
    let _ = fs::remove_dir_all(&root);
    write(&root, "app.jadpo", "application {}\n");
    for (file, text) in [
        (
            "types/meaning.jadpo",
            "/// A note's meaning.\ntype Meaning = Vector {\n    dimensions: 2\n}\n",
        ),
        (
            "entities/note.jadpo",
            "use notes/types { Meaning }\n\n/// A note.\nentity Note {\n    id: Uuid\n    meaning: Meaning\n\n    policy: {\n        read: [Access.public]\n    }\n}\n",
        ),
        (
            "types/note_card.jadpo",
            "use notes/entities { Note }\n\n/// A note in a list.\ntype NoteCard = Output {\n    pick: [Note.id]\n}\n",
        ),
        (
            "queries/nearest_far.jadpo",
            "use notes/entities { Note }\nuse notes/types { Meaning, NoteCard }\n\n/// The five notes nearest a meaning of those at least 0.5 from it.\nquery nearest_far(from_meaning: Meaning) -> List<Nearest<NoteCard>> {\n    from: Note\n    where: ai.distance(Note.meaning, from_meaning) >= 0.5\n    order: [Note.meaning nearest from_meaning]\n    limit: 5\n}\n",
        ),
        (
            "actions/find_nearest_far.jadpo",
            "use notes/queries { nearest_far }\nuse notes/types { Meaning, NoteCard }\n\n/// The five.\naction find_nearest_far(from_meaning: Meaning) -> List<Nearest<NoteCard>> {\n    return nearest_far(from_meaning)\n}\n",
        ),
    ] {
        write(&root, &format!("domains/notes/{file}"), text);
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let url = database("vector_scan_tuples");
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    rows(
        &url,
        "INSERT INTO note (id, meaning) SELECT gen_random_uuid(), ('[1,' || (i * 0.00001)::text || ']')::vector FROM generate_series(1, 30000) AS i",
        &[],
    );
    let far: [[f32; 2]; 5] = [
        [0.0, 1.0],
        [-1.0, 0.0],
        [0.0, -1.0],
        [-1.0, 1.0],
        [-1.0, -1.0],
    ];
    for (n, meaning) in far.iter().enumerate() {
        let mut id = [0u8; 16];
        id[15] = n as u8 + 1;
        id[6] = 0x40;
        id[8] = 0x80;
        rows(
            &url,
            "INSERT INTO note (id, meaning) VALUES ($1, $2)",
            &[SqlValue::Uuid(id), bits(meaning)],
        );
    }
    rows(&url, "ANALYZE note", &[]);
    // The index serves the order, whatever the table's size would
    // otherwise make the planner choose, and the scan has the memory it
    // needs, so the setting under test is what decides how far it goes.
    let name = match rows(&url, "SELECT current_database()", &[])
        .first()
        .and_then(|row| row.first())
    {
        Some(SqlValue::Text(name)) => name.clone(),
        other => panic!("no database name: {other:?}"),
    };
    rows(
        &url,
        &format!("ALTER DATABASE \"{name}\" SET enable_seqscan = off"),
        &[],
    );
    rows(
        &url,
        &format!("ALTER DATABASE \"{name}\" SET work_mem = '256MB'"),
        &[],
    );
    let loaded = jadpo_db::load(&root);
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let host = Harness::postgres(&url);
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    let id = program.plan().find("find_nearest_far").expect("the action");
    let found = runtime.block_on(async {
        let env = Env {
            seed: Some(1),
            clock: Clock::TEST,
        };
        let mut ctx = Ctx::new(program.places(), &host, env);
        program.run(&mut ctx, id, vec![vector(&[1.0, 0.0])]).await
    });
    let Outcome::Value(Value::List(found)) = found else {
        panic!("expected a list, got {found:?}");
    };
    assert_eq!(found.len(), 5, "the five far notes: {found:?}");
    // pgvector's default would have ended the scan first: the same
    // statement without the setting finds fewer.
    let plan = program
        .plan()
        .query_named("nearest_far")
        .expect("the query's plan");
    assert!(
        plan.read
            .settings
            .iter()
            .any(|setting| setting == "SET LOCAL hnsw.max_scan_tuples = 2147483647"),
        "{:?}",
        plan.read.settings
    );
    let params: Vec<SqlValue> = plan
        .read
        .select
        .params
        .iter()
        .map(|param| match &param.source {
            jadpo_std::read::Source::Arg { .. } => bits(&[1.0, 0.0]),
            jadpo_std::read::Source::Stored(value) => value.clone(),
            other => panic!("a parameter from {other:?}"),
        })
        .collect();
    let store = jadpo_host_native::postgres(&url).expect("store");
    let defaulted = runtime.block_on(async {
        let mut transaction = store.begin().await.expect("begin");
        for setting in &plan.read.settings {
            if !setting.contains("max_scan_tuples") {
                transaction.batch(setting).await.expect("a setting");
            }
        }
        let rows = transaction
            .query(&plan.read.select.sql, &params)
            .await
            .expect("the statement");
        let _ = transaction.rollback().await;
        rows
    });
    assert!(
        defaulted.len() < 5,
        "pgvector's default scan found {} of the five",
        defaulted.len()
    );
}
