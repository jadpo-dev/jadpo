//! A `GET` that runs an action (area 7, "What a route runs"; chunk G2),
//! end to end: a route tests' project that works a value out, raises two
//! failures and asks a model from a `GET`, in `jadpo test` here and compiled
//! beside the interpreter in `compiled.rs`; then against a real server, a
//! `GET` reading on both sides of a model's call in a `READ ONLY` transaction
//! while the model's budget is counted beside it, its `HEAD`, its OpenAPI, and
//! PostgreSQL refusing a write in such a transaction.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_host_native::{Runtime, postgres};
use jadpo_runtime::{Database, Isolation, SqlValue};

use crate::http::{Dev, jadpo, wait_for};
use crate::support::fake_provider::FakeProvider;

const APP: &str = "application {\n    connections: {\n        local_model: {\n            driver: Driver.openai_compatible\n            url: config.model_url\n        }\n    }\n    ai: {\n        connection: local_model\n        models: {\n            default: config.drafting_model\n            embedding: config.search_model\n        }\n        limits: {\n            per_day: {\n                tokens: 2000000\n            }\n        }\n        embedding_dimensions: 3\n    }\n}\n";

const CONFIG: &str = "config {\n    model_url: Url {\n        default: \"http://localhost:11434/v1\"\n    }\n    drafting_model: ModelId {\n        default: \"local/llama3.2\"\n    }\n    search_model: ModelId {\n        default: \"local/nomic-embed-text\"\n    }\n}\n";

/// A shelf's project: each action a `GET` runs reads, works out, fails and
/// asks a model, and nothing changes.
pub fn files() -> Vec<(String, String)> {
    [
        ("app.jadpo", APP),
        ("config.jadpo", CONFIG),
        (
            "policy.jadpo",
            "use desk\n\npolicy {\n    actions: {\n        desk.name_shelf: [Access.public]\n        desk.stream_name: [Access.public]\n    }\n}\n",
        ),
        (
            "domains/desk/types/note.jadpo",
            "/// What a shelf says.\ntype Note = Text {\n    max_length: 40\n}\n",
        ),
        (
            "domains/desk/types/count.jadpo",
            "/// How many.\ntype Count = Int {\n    min: 0\n}\n",
        ),
        (
            "domains/desk/types/meaning.jadpo",
            "/// A note's meaning.\ntype Meaning = Vector {\n    dimensions: 3\n}\n",
        ),
        (
            "domains/desk/types/asked.jadpo",
            "use desk/types { Note }\n\n/// What a search asks.\ntype Asked = Input {\n    q: Note\n}\n",
        ),
        (
            "domains/desk/types/found.jadpo",
            "use desk/types { Count, Meaning }\n\n/// What a search finds.\ntype Found = Output {\n    before: Count\n    after: Count\n    meaning: Meaning\n}\n",
        ),
        (
            "domains/desk/types/named.jadpo",
            "use desk/types { Note }\n\n/// A shelf's name.\ntype Named = Output {\n    name: Note\n}\n",
        ),
        (
            "domains/desk/entities/shelf.jadpo",
            "use desk/types { Note }\n\n/// A shelf.\nentity Shelf {\n    id: Uuid\n    note: Note\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n    }\n}\n",
        ),
        (
            "domains/desk/failures/no_model.jadpo",
            "/// The model can't answer now.\ntype NoModel = Failure {\n    kind: FailureKind.unavailable\n    message: \"The model can't answer now.\"\n}\n",
        ),
        (
            "domains/desk/failures/too_far.jadpo",
            "/// A search past the end.\ntype TooFar = Failure {\n    kind: FailureKind.rejected\n    message: \"That is too far along.\"\n}\n",
        ),
        (
            "domains/desk/prompts/name_it.jadpo",
            "use desk/types { Note }\n\n/// Names a shelf.\ntype NameIt = Prompt {\n    note: Note\n}\n",
        ),
        (
            "domains/desk/prompts/name_it.prompt",
            "# Instructions\n\nYou name shelves.\n\n# Request\n\n{note}\n",
        ),
        (
            "domains/desk/prompts/stream_it.jadpo",
            "use desk/types { Note }\n\n/// Names a shelf, piece by piece.\ntype StreamIt = Prompt {\n    note: Note\n}\n",
        ),
        (
            "domains/desk/prompts/stream_it.prompt",
            "# Instructions\n\nYou name shelves.\n\n# Request\n\n{note}\n",
        ),
        (
            "domains/desk/queries/all_shelves.jadpo",
            "use desk/entities { Shelf }\n\n/// Every shelf.\nquery all_shelves() -> List<Shelf> {\n    from: Shelf\n    order: [Shelf.id asc]\n    limit: 100\n}\n",
        ),
        (
            "domains/desk/actions/count_shelves.jadpo",
            "use desk/entities { Shelf }\nuse desk/queries { all_shelves }\nuse desk/types { Count }\n\n/// How many shelves there are, through an action of its own.\naction count_shelves() -> Count {\n    var all: List<Shelf> = all_shelves()\n    return Count(list.count(all))\n}\n",
        ),
        (
            "domains/desk/actions/search.jadpo",
            "use desk/actions { count_shelves }\nuse desk/failures { NoModel, TooFar }\nuse desk/types { Asked, Count, Found, Meaning }\n\n/// Reads on both sides of asking for a meaning, and can fail two ways.\naction search(asked: Asked) fails NoModel, TooFar -> Found {\n    var before: Count = count_shelves()\n    if (asked.q == \"too far\") {\n        reject TooFar {}\n    }\n    var meaning: Meaning = attempt ai.embed(asked.q, as: Meaning) {\n        timed_out: NoModel\n        over_budget: NoModel\n        too_long: NoModel\n    }\n    var after: Count = count_shelves()\n    return Found {\n        before: before\n        after: after\n        meaning: meaning\n    }\n}\n",
        ),
        (
            "domains/desk/actions/name_shelf.jadpo",
            "use desk/failures { NoModel }\nuse desk/prompts { NameIt }\nuse desk/types { Asked, Named, Note }\n\n/// Asks a model for a shelf's name.\naction name_shelf(asked: Asked) fails NoModel -> Named {\n    var name: Note = attempt ai.generate(NameIt { note: asked.q }, as: Note) {\n        refused: NoModel\n        malformed: NoModel\n        timed_out: NoModel\n        over_budget: NoModel\n        too_long: NoModel\n    }\n    return Named {\n        name: name\n    }\n}\n",
        ),
        (
            "domains/desk/actions/stream_name.jadpo",
            "use desk/failures { NoModel }\nuse desk/prompts { StreamIt }\nuse desk/types { Asked }\n\n/// Streams a shelf's name as it comes.\naction stream_name(asked: Asked) fails NoModel -> AiStream {\n    return attempt ai.stream(StreamIt { note: asked.q }) {\n        refused: NoModel\n        timed_out: NoModel\n        over_budget: NoModel\n        too_long: NoModel\n    }\n}\n",
        ),
        (
            "domains/desk/routes/shelves.jadpo",
            "use desk/actions { name_shelf, search, stream_name }\nuse desk/types { Asked, Found, Named }\n\n/// Shelves, read and worked out.\nroutes Shelves {\n    prefix: /shelves\n\n    /// Finds shelves by what a note means.\n    GET /search {\n        authentication: RouteAuthentication.none\n        deadline: 30s\n        input: Asked\n        output: Found\n        run: search(input)\n    }\n\n    /// Asks a model for a name.\n    GET /name {\n        authentication: RouteAuthentication.none\n        deadline: 60s\n        input: Asked\n        output: Named\n        run: name_shelf(input)\n    }\n\n    /// Streams a model's name.\n    GET /stream {\n        authentication: RouteAuthentication.none\n        deadline: 30s\n        input: Asked\n        output: AiStream\n        run: stream_name(input)\n    }\n}\n",
        ),
        (
            "domains/desk/fixtures/a_shelf.jadpo",
            "use desk/entities { Shelf }\n\n/// A shelf to find.\nfixture a_shelf() -> Shelf {\n    return Shelf.create {\n        set: {\n            note: \"rye\"\n        }\n    }\n}\n",
        ),
        (
            "domains/desk/routes/shelves.test.jadpo",
            "use desk/fixtures { a_shelf }\nuse desk/types { Asked, Named }\n\ntest \"a GET works a value out on both sides of a model's call\" {\n    assert a_shelf().note == \"rye\"\n    var response: Response = request {\n        method: HttpMethod.get\n        path: \"/shelves/search\"\n        input: Asked {\n            q: \"bread\"\n        }\n    }\n    assert response.status == 200\n    assert response.code == none\n}\n\ntest \"a GET raises a rejected failure\" {\n    var response: Response = request {\n        method: HttpMethod.get\n        path: \"/shelves/search\"\n        input: Asked {\n            q: \"too far\"\n        }\n    }\n    assert response.status == 422\n    assert response.code == \"too_far\"\n}\n\ntest \"a GET raises a failure from a model's call\" {\n    fakes: {\n        ai.embed: [timed_out]\n    }\n    var response: Response = request {\n        method: HttpMethod.get\n        path: \"/shelves/search\"\n        input: Asked {\n            q: \"bread\"\n        }\n    }\n    assert response.status == 503\n    assert response.code == \"no_model\"\n}\n\ntest \"a GET asks a model for an answer\" {\n    fakes: {\n        ai.name_it: [\"The rye shelf\"]\n    }\n    var response: Response = request {\n        method: HttpMethod.get\n        path: \"/shelves/name\"\n        input: Asked {\n            q: \"rye\"\n        }\n    }\n    assert response.status == 200\n    assert response.output == Named {\n        name: \"The rye shelf\"\n    }\n}\n\ntest \"a GET streams a model's answer\" {\n    fakes: {\n        ai.stream_it: [\"The rye shelf\"]\n    }\n    var response: Response = request {\n        method: HttpMethod.get\n        path: \"/shelves/stream\"\n        input: Asked {\n            q: \"rye\"\n        }\n    }\n    assert response.status == 200\n    assert response.code == none\n}\n",
        ),
    ]
    .into_iter()
    .map(|(path, text)| (path.to_string(), text.to_string()))
    .collect()
}

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("read-only-get-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files() {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        fs::write(path, text).expect("write");
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let recorded = jadpo(&root, &["records", "update"]);
    assert_eq!(recorded.code, 0, "{}{}", recorded.stdout, recorded.stderr);
    root
}

#[test]
fn route_tests_pass_with_a_get_running_actions() {
    if std::env::var_os("JADPO_SKIP_POSTGRES").is_some() {
        return;
    }
    let root = project("tests");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let tested = jadpo(&root, &["test"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
}

/// An embedding of three numbers, as OpenAI's API answers one.
fn embedding(usage: u32) -> String {
    serde_json::json!({
        "object": "list",
        "data": [{"object": "embedding", "index": 0, "embedding": [0.5, 0.25, 0.125]}],
        "model": "local/nomic-embed-text",
        "usage": {"prompt_tokens": usage, "total_tokens": usage},
    })
    .to_string()
}

#[test]
fn a_get_reads_around_a_model_s_call_and_its_budget_is_counted_beside_it() {
    if std::env::var_os("JADPO_SKIP_POSTGRES").is_some() {
        return;
    }
    let model = FakeProvider::start();
    let root = project("dev");
    fs::write(
        root.join(".env.local"),
        format!("MODEL_URL={}/v1\n", model.url),
    )
    .expect("write");
    let dev = Dev::start(&root);
    wait_for(&dev, "\"listening\"");

    model.answer(200, &embedding(5));
    let answered = dev.send("GET", "/shelves/search?q=bread", &[], "");
    assert_eq!(answered.status, 200, "{answered:?}");
    assert_eq!(
        answered.json(),
        serde_json::json!({"before": "0", "after": "0", "meaning": [0.5, 0.25, 0.125]})
    );
    // Asked once, under no key: a `GET` keeps none.
    let sent = model.requests();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].header("idempotency-key"), None);

    // A `HEAD` is the `GET` with no body, so it asks the model too.
    model.answer(200, &embedding(5));
    let head = dev.send("HEAD", "/shelves/search?q=bread", &[], "");
    assert_eq!(head.status, 200, "{head:?}");
    assert_eq!(head.body(), "");
    assert_eq!(model.requests().len(), 2);

    // Failures: rejected, before anything is asked, and a model's.
    let rejected = dev.send("GET", "/shelves/search?q=too%20far", &[], "");
    assert_eq!(rejected.status, 422, "{rejected:?}");
    assert_eq!(rejected.json()["error"]["code"], "too_far");
    assert_eq!(model.requests().len(), 2);

    // What the model used is counted, in the runtime's own statement,
    // though the action's transaction can't write.
    let runtime = Runtime::new().expect("runtime");
    let store = postgres(dev.url()).expect("store");
    let rows = runtime
        .block_on(store.query_once(
            "SELECT \"calls\" FROM \"jadpo_ai_usage\" WHERE \"scope\" = 'app'",
            &[],
        ))
        .expect("the counts");
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0], vec![SqlValue::Int4(2)]);

    // A `HEAD` of a streaming route sends the head and nothing of the stream.
    model.answer(
        200,
        "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"The rye shelf\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
    );
    let head = dev.send("HEAD", "/shelves/stream?q=rye", &[], "");
    assert_eq!(head.status, 200, "{head:?}");
    assert_eq!(head.body(), "");

    // OpenAPI says it changes nothing, takes no key and can fail both ways.
    let document = dev.send("GET", "/openapi.json", &[], "");
    assert_eq!(document.status, 200, "{document:?}");
    let operation = &document.json()["paths"]["/shelves/search"]["get"];
    assert_eq!(operation["x-jadpo-changes"], false, "{operation}");
    assert!(operation["responses"]["422"].is_object(), "{operation}");
    assert!(operation["responses"]["503"].is_object(), "{operation}");
    let parameters = operation["parameters"].as_array().expect("its input");
    assert!(
        parameters
            .iter()
            .all(|one| one["name"] != "Idempotency-Key"),
        "{operation}"
    );
}

#[test]
fn postgresql_refuses_a_write_in_a_read_only_transaction() {
    if std::env::var_os("JADPO_SKIP_POSTGRES").is_some() {
        return;
    }
    let server = jadpo_test::database::server().expect("a PostgreSQL server");
    let url = server.create_database("read_only").expect("a database");
    let runtime = Runtime::new().expect("runtime");
    let store = postgres(&url).expect("store");
    for isolation in [Isolation::ReadCommitted, Isolation::RepeatableRead] {
        runtime.block_on(async {
            let mut transaction = store.begin_read_only(isolation).await.expect("begins");
            let read = transaction.query("SELECT 1", &[]).await.expect("reads");
            assert_eq!(read.len(), 1);
            let refused = transaction
                .batch("CREATE TABLE \"nope\" (\"id\" int)")
                .await;
            assert!(refused.is_err(), "{refused:?}");
            let _ = transaction.rollback().await;
        });
    }
}
