//! The prompts and answers `ai: { keep_content: }` keeps, against live
//! PostgreSQL (area 21, "Prompts and answers in logs" and "Keeping prompts
//! and answers"; chunk K6). A bakery's support desk keeps its answers for
//! 30 days, each sealed with the field key as `health` data:
//!
//! - a call a signed-in customer made is in their export, its messages and
//!   answer opened from their seals, with how long it is kept and where it
//!   was sent;
//! - their erasure deletes it with their sessions, so their export holds
//!   nothing, as the check after every erasure in a test makes sure;
//! - a call made for no one, and a call of a prompt not listed, is in no
//!   one's export.
//!
//! The tests run through `jadpo test` here, with the `ai` fake, and
//! compiled with the interpreter's records in the gate's compiled step
//! (`crates/jadpo-cli/tests/compiled.rs`, "ai calls kept"). The server is
//! the one `JADPO_TEST_DATABASE_URL` names or one started from the
//! machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_host_native::Runtime;
use jadpo_interp::Program;
use jadpo_runtime::schedules::tick;
use jadpo_runtime::{Executor, Outcome, SqlValue};
use jadpo_term::Environment;
use jadpo_test::host::Harness;
use jadpo_value::Value;

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    run(&args, &env.with_cwd(cwd))
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

/// The desk: customers who sign in and ask, its prompts' calls kept.
pub(crate) fn files() -> Vec<(String, String)> {
    let files: [(&str, &str); 20] = [
        (
            "app.jadpo",
            "use desk/entities { Customer }\n\napplication {\n    connections: {\n        local_model: {\n            driver: Driver.openai_compatible\n            url: config.model_url\n        }\n        erasures: {\n            driver: Driver.disk\n            path: \"/var/lib/desk/erasures\"\n        }\n    }\n    field_encryption_key: config.field_encryption_key\n    privacy: {\n        erasure_log: {\n            store: erasures\n        }\n    }\n    ai: {\n        connection: local_model\n        models: {\n            default: config.drafting_model\n        }\n        limits: {\n            per_day: {\n                tokens: 2000000\n            }\n        }\n        keep_content: {\n            prompts: [answer_question]\n            keep: 30d\n        }\n    }\n}\n\nprincipal {\n    user: {\n        subject: Customer.login\n    }\n}\n",
        ),
        (
            "authentication.jadpo",
            "authentication {\n    api_token: {}\n}\n",
        ),
        (
            "config.jadpo",
            "config {\n    model_url: Url {\n        default: \"http://localhost:11434/v1\"\n    }\n    drafting_model: ModelId {\n        default: \"local/llama3.2\"\n    }\n    field_encryption_key: EncryptionKey\n}\n",
        ),
        (
            "policy.jadpo",
            "use desk\n\npolicy {\n    actions: {\n        desk.answer_customer: [Access.authenticated]\n        desk.title_question: [Access.authenticated]\n    }\n}\n",
        ),
        (
            "domains/desk/types/line.jadpo",
            "/// A short line of text.\ntype Line = Text {\n    max_length: 80\n}\n",
        ),
        (
            "domains/desk/types/customer_question.jadpo",
            "/// What a customer asked.\ntype CustomerQuestion = Text {\n    max_length: 500\n}\n",
        ),
        (
            "domains/desk/types/asked.jadpo",
            "use desk/types { CustomerQuestion }\n\n/// A customer's question, as the desk sends it.\ntype Asked = Input {\n    question: CustomerQuestion\n}\n",
        ),
        (
            "domains/desk/types/support_answer.jadpo",
            "/// What the assistant says back.\ntype SupportAnswer = Output {\n    /// The reply to show the customer.\n    reply: Text {\n        max_length: 400\n    }\n}\n",
        ),
        (
            "domains/desk/failures/assistant_unavailable.jadpo",
            "/// The assistant can't answer now.\ntype AssistantUnavailable = Failure {\n    kind: FailureKind.unavailable\n}\n",
        ),
        (
            "domains/desk/entities/customer.jadpo",
            "use desk/types { Line }\n\n/// Someone who signs in to ask the desk.\nentity Customer {\n    id: Uuid\n    login: Line {\n        unique: true\n    }\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n        delete: [Access.public]\n    }\n\n    person: true\n}\n",
        ),
        (
            "domains/desk/prompts/answer_question.jadpo",
            "use desk/types { CustomerQuestion }\n\n/// Answers a customer's question.\ntype AnswerQuestion = Prompt {\n    question: CustomerQuestion\n}\n",
        ),
        (
            "domains/desk/prompts/answer_question.prompt",
            "# Instructions\n\nYou answer questions from customers of an online bakery.\n\n# Request\n\n{question}\n",
        ),
        (
            "domains/desk/prompts/suggest_title.jadpo",
            "use desk/types { CustomerQuestion }\n\n/// Suggests a title for a customer's question.\ntype SuggestTitle = Prompt {\n    question: CustomerQuestion\n}\n",
        ),
        (
            "domains/desk/prompts/suggest_title.prompt",
            "# Instructions\n\nSuggest a title of a few words for the question.\n\n# Request\n\n{question}\n",
        ),
        (
            "domains/desk/actions/answer_customer.jadpo",
            "use desk/failures { AssistantUnavailable }\nuse desk/prompts { AnswerQuestion }\nuse desk/types { Asked, SupportAnswer }\n\n/// Answers a customer's question.\naction answer_customer(asked: Asked) fails AssistantUnavailable -> SupportAnswer {\n    return attempt ai.generate(AnswerQuestion { question: asked.question }, as: SupportAnswer) {\n        refused: AssistantUnavailable\n        malformed: AssistantUnavailable\n        timed_out: AssistantUnavailable\n        over_budget: AssistantUnavailable\n        too_long: AssistantUnavailable\n    }\n}\n",
        ),
        (
            "domains/desk/actions/title_question.jadpo",
            "use desk/failures { AssistantUnavailable }\nuse desk/prompts { SuggestTitle }\nuse desk/types { Asked, SupportAnswer }\n\n/// Suggests a title for a customer's question, which isn't kept.\naction title_question(asked: Asked) fails AssistantUnavailable -> SupportAnswer {\n    return attempt ai.generate(SuggestTitle { question: asked.question }, as: SupportAnswer) {\n        refused: AssistantUnavailable\n        malformed: AssistantUnavailable\n        timed_out: AssistantUnavailable\n        over_budget: AssistantUnavailable\n        too_long: AssistantUnavailable\n    }\n}\n",
        ),
        (
            "domains/desk/actions/join.jadpo",
            "use desk/entities { Customer }\nuse desk/failures { LoginTaken }\nuse desk/types { Line }\n\n/// Signs a customer up.\naction join(login: Line) fails LoginTaken -> Customer {\n    return attempt Customer.create {\n        set: {\n            login: login\n        }\n        conflict: {\n            Customer.login: LoginTaken\n        }\n    }\n}\n",
        ),
        (
            "domains/desk/actions/forget.jadpo",
            "use desk/entities { Customer, CustomerNotFound }\n\n/// Erases a customer.\naction forget(customer_id: Customer.id) fails CustomerNotFound -> Unit {\n    attempt Customer.erase(customer_id)\n}\n",
        ),
        (
            "domains/desk/failures/login_taken.jadpo",
            "/// Someone signs in with that already.\ntype LoginTaken = Failure {\n    kind: FailureKind.conflict\n}\n",
        ),
        (
            "domains/desk/routes/answers.jadpo",
            "use desk/actions { answer_customer, title_question }\nuse desk/types { Asked, SupportAnswer }\n\n/// Customers' questions.\nroutes Answers {\n    api: main\n    prefix: /answers\n\n    /// A signed-in customer asks one.\n    POST / {\n        input: Asked\n        output: SupportAnswer\n        deadline: 60s\n        run: answer_customer(input)\n    }\n\n    /// A title for one.\n    POST /title {\n        input: Asked\n        output: SupportAnswer\n        deadline: 60s\n        run: title_question(input)\n    }\n}\n",
        ),
    ];
    let mut out: Vec<(String, String)> = files
        .iter()
        .map(|(path, text)| (path.to_string(), text.to_string()))
        .collect();
    out.push((
        "domains/desk/actions/answer_customer.test.jadpo".to_string(),
        TESTS.to_string(),
    ));
    out
}

const TESTS: &str = r#"use desk/actions { forget, join }
use desk/entities { Customer }
use desk/types { Asked, SupportAnswer }

test "a customer's kept call is in their export" {
    fakes: {
        ai.answer_question: [SupportAnswer { reply: "Our rye has no sesame." }]
    }
    var ada: Customer = attempt join("ada")
    var asked: Response = request {
        method: HttpMethod.post
        path: "/answers"
        input: Asked {
            question: "Is your rye made beside sesame?"
        }
        as: Principal.user(ada)
    }
    assert asked.status == 200
    var export: PersonalDataExport = Customer.export(ada.id)
    assert export.records == 2
    assert text.contains(export.json, "\"AiContent\":[{")
    assert text.contains(export.json, "\"prompt\":\"answer_question\"")
    assert text.contains(export.json, "\"model\":\"local/llama3.2\"")
    assert text.contains(export.json, "{\"role\":\"system\",\"content\":\"You answer questions from customers of an online bakery.\"}")
    assert text.contains(export.json, "Is your rye made beside sesame?")
    assert text.contains(export.json, "Our rye has no sesame.")
    assert text.contains(export.json, "{\"entity\":\"AiContent\",\"retention\":[{\"field\":null,\"after\":\"PT720H\",\"since\":\"made_at\"}],\"recipients\":[\"local_model\"]}")
}

test "a customer's erasure takes what was kept for them" {
    fakes: {
        ai.answer_question: [SupportAnswer { reply: "Our rye has no sesame." }]
    }
    var ada: Customer = attempt join("ada")
    var asked: Response = request {
        method: HttpMethod.post
        path: "/answers"
        input: Asked {
            question: "Is your rye made beside sesame?"
        }
        as: Principal.user(ada)
    }
    assert asked.status == 200
    attempt forget(ada.id)
    var bob: Customer = attempt join("bob")
    var export: PersonalDataExport = Customer.export(bob.id)
    assert export.records == 1
}

test "a prompt keep_content doesn't list is kept for no one" {
    fakes: {
        ai.suggest_title: [SupportAnswer { reply: "Rye and sesame" }]
    }
    var ada: Customer = attempt join("ada")
    var titled: Response = request {
        method: HttpMethod.post
        path: "/answers/title"
        input: Asked {
            question: "Is your rye made beside sesame?"
        }
        as: Principal.user(ada)
    }
    assert titled.status == 200
    var export: PersonalDataExport = Customer.export(ada.id)
    assert export.records == 1
    assert not text.contains(export.json, "AiContent")
}
"#;

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("kept-ai-k6-{name}"));
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
fn kept_calls_pass_their_tests() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("tests");
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(
        tested
            .stdout
            .contains("9 passed, 3 route tests, 6 generated tests"),
        "{}",
        tested.stdout
    );
}

/// The migration makes the kept calls' table, with the messages and the
/// answer sealed in `bytea` columns, beside everyone who signs in, and the
/// personal-data report lists what is kept.
#[test]
fn the_kept_calls_table_is_migrated_beside_the_customers() {
    let root = project("migrated");
    let migration = fs::read_dir(root.join("database/migrations"))
        .expect("migrations")
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("postgres.sql"))
        .find(|path| path.exists())
        .expect("a migration");
    let sql = fs::read_to_string(migration).expect("the migration");
    assert!(sql.contains("CREATE TABLE \"jadpo_ai_content\""), "{sql}");
    assert!(sql.contains("\"messages\" bytea NOT NULL"), "{sql}");
    assert!(sql.contains("\"answer\" bytea"), "{sql}");
    assert!(sql.contains("\"principal_id\" uuid"), "{sql}");
    // The personal-data report says what is kept, where and for how long.
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let report =
        fs::read_to_string(root.join("build/report/personal-data.md")).expect("the report");
    assert!(report.contains("## Prompts and answers kept"), "{report}");
    assert!(
        report.contains("| `answer_question` | `main` | 30 days |"),
        "{report}"
    );
    assert!(!report.contains("`suggest_title` | `main`"), "{report}");
}

/// 2026-10-19T09:00:00Z.
const T0: i64 = 1_792_400_400_000;
const DAY: i64 = 86_400_000;

/// `root`'s program, started with a field key.
fn started(root: &Path) -> Program {
    let loaded = jadpo_db::load(root);
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let variables = std::collections::BTreeMap::from([(
        "FIELD_ENCRYPTION_KEY".to_string(),
        crate::encrypted_fields::FIRST_KEY.to_string(),
    )]);
    let values = program
        .prepared()
        .config(
            &variables,
            jadpo_runtime::config::Mode::Environment,
            &|_: &mut [u8]| false,
        )
        .unwrap_or_else(|problems| panic!("doesn't start: {problems:?}"));
    program.with_config(&values)
}

/// Runs `action` with `args` at the harness's clock, with no caller.
async fn call(program: &Program, host: &Harness, action: &str, args: Vec<Value>) -> Value {
    let id = program.plan().find(action).expect("the action");
    let mut ctx = host.ctx(program);
    match program.run(&mut ctx, id, args).await {
        Outcome::Value(value) => value,
        other => panic!("{action}: expected a value, got {other:?}"),
    }
}

fn id_of(value: &Value) -> Value {
    match value {
        Value::Record(record) => record.get("id").cloned().expect("an id"),
        other => panic!("not a record: {other:?}"),
    }
}

async fn texts(host: &Harness, sql: &str) -> Vec<String> {
    host.rows(sql)
        .await
        .into_iter()
        .map(|row| match &row[0] {
            SqlValue::Text(text) => text.clone(),
            SqlValue::Null(_) => "none".to_string(),
            other => panic!("not text: {other:?}"),
        })
        .collect()
}

/// A customer's erasure deletes the calls kept for them, in its one
/// statement, and leaves everyone else's and those made for no one; the
/// tick deletes each once `keep:` has passed, and no sooner.
#[test]
fn kept_calls_go_with_their_customer_and_their_time() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("rows");
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = started(&root);
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let host = Harness::postgres(&url).at(T0).random_from(0);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let ada = call(&program, &host, "join", vec![Value::text("ada")]).await;
        call(&program, &host, "join", vec![Value::text("bob")]).await;
        // A call kept for each customer, and one made for no one, on the
        // first day; another for Bob 20 days on.
        let kept = |principal: &str, made_at: &str| {
            format!(
                "INSERT INTO \"jadpo_ai_content\" (\"id\", \"made_at\", \"prompt\", \"model\", \"principal_id\", \"messages\") SELECT gen_random_uuid(), TIMESTAMPTZ '{made_at}', 'answer_question', 'local/llama3.2', {principal}, '\\x00'::bytea"
            )
        };
        for principal in [
            "(SELECT \"id\" FROM \"customer\" WHERE \"login\" = 'ada')",
            "(SELECT \"id\" FROM \"customer\" WHERE \"login\" = 'bob')",
            "NULL",
        ] {
            host.rows(&kept(principal, "2026-10-19 09:00:00+00")).await;
        }
        host.rows(&kept(
            "(SELECT \"id\" FROM \"customer\" WHERE \"login\" = 'bob')",
            "2026-11-08 09:00:00+00",
        ))
        .await;
        let held = || async {
            texts(
                &host,
                "SELECT coalesce(c.\"login\", 'no one') || ' ' || to_char(k.\"made_at\", 'MM-DD') FROM \"jadpo_ai_content\" AS k LEFT JOIN \"customer\" AS c ON c.\"id\" = k.\"principal_id\" ORDER BY 1",
            )
            .await
        };
        assert_eq!(
            held().await,
            ["ada 10-19", "bob 10-19", "bob 11-08", "no one 10-19"]
        );

        call(&program, &host, "forget", vec![id_of(&ada)]).await;
        assert_eq!(held().await, ["bob 10-19", "bob 11-08", "no one 10-19"]);

        // Thirty days on, nothing goes yet; a day later, the first day's.
        host.set_now(T0 + 29 * DAY);
        tick(&program, &host, T0 + 29 * DAY).await;
        assert_eq!(held().await, ["bob 10-19", "bob 11-08", "no one 10-19"]);
        host.set_now(T0 + 31 * DAY);
        tick(&program, &host, T0 + 31 * DAY).await;
        assert_eq!(held().await, ["bob 11-08"]);
    });
}
