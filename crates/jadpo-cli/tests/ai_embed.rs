//! Embeddings and recordings (area 21, "The `ai` capability", "Embeddings
//! and search" and "Tests"; chunk 5.5v4).
//!
//! - **The fake.** `jadpo test` answers every embedding with a vector made
//!   from the text's SHA-256 and the test's seed, so the same text gives
//!   the same vector, `ai.embed_all` gives each in order, and a script
//!   gives an embedding's failures; a prompt is answered from one of its
//!   recordings with `recorded("<name>")`, its placeholders filled.
//! - **Recordings.** Every recording is decoded through each type its
//!   prompt is asked for as, a test of the `recording` kind; one that
//!   doesn't decode fails it, and one made from an older version of its
//!   prompt passes with an alert naming it.
//! - **The local driver.** `jadpo dev` asks a server that speaks OpenAI's
//!   embeddings API, here one started in the test on this machine (never a
//!   real provider, the network or a key), and refuses a vector of another
//!   length than the type's.
//! - **The support desk** of area 21, end to end against PostgreSQL with
//!   pgvector: articles embedded and stored, the nearest found for a
//!   question, and a typed answer from a recording. A nearest query needs
//!   pgvector 0.8.0, so on a server with an older one (Postgres.app ships
//!   0.7.0) that test is skipped, saying so, except under CI.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

use crate::http::{Dev, wait_for};
use crate::support;
use crate::support::fake_provider::FakeProvider;

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("ai-embed-5.5v4-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temporary folder");
    dir
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
    let mut output = run(&args, &env.with_cwd(cwd));
    output.stdout = support::without_run_time(&output.stdout);
    output
}

/// `app.jadpo` asking a model on this machine for text and embeddings of
/// `dimensions` numbers.
fn app(dimensions: u32) -> String {
    format!(
        "application {{\n    connections: {{\n        local_model: {{\n            driver: Driver.openai_compatible\n            url: config.model_url\n        }}\n    }}\n    ai: {{\n        connection: local_model\n        models: {{\n            default: config.drafting_model\n            embedding: config.search_model\n        }}\n        limits: {{\n            per_day: {{\n                tokens: 2000000\n            }}\n        }}\n        embedding_dimensions: {dimensions}\n    }}\n}}\n"
    )
}

const CONFIG: &str = "config {\n    model_url: Url {\n        default: \"http://localhost:11434/v1\"\n    }\n    drafting_model: ModelId {\n        default: \"local/llama3.2\"\n    }\n    search_model: ModelId {\n        default: \"local/nomic-embed-text\"\n    }\n}\n";

/// The desk without a store: a question's meaning, several at once, and
/// an answer from a model.
fn desk(name: &str) -> PathBuf {
    let root = folder(name);
    write(&root, "app.jadpo", &app(8));
    write(&root, "config.jadpo", CONFIG);
    write(
        &root,
        "policy.jadpo",
        "use support\n\npolicy {\n    actions: {\n        support.meaning_of: [Access.public]\n    }\n}\n",
    );
    for (path, text) in [
        (
            "types/question_meaning.jadpo",
            "/// A question's meaning, as the embedding model makes it.\ntype QuestionMeaning = Vector {\n    dimensions: 8\n}\n",
        ),
        (
            "types/customer_question.jadpo",
            "/// What a customer asked.\ntype CustomerQuestion = Text {\n    max_length: 500\n}\n",
        ),
        (
            "types/asked.jadpo",
            "use support/types { CustomerQuestion }\n\n/// A question, as the desk sends it.\ntype Asked = Input {\n    question: CustomerQuestion\n}\n",
        ),
        (
            "types/meaning.jadpo",
            "use support/types { QuestionMeaning }\n\n/// A question's meaning, sent back.\ntype Meaning = Output {\n    meaning: QuestionMeaning\n}\n",
        ),
        (
            "types/support_answer.jadpo",
            "/// What the assistant says back.\ntype SupportAnswer = Output {\n    /// False when the help articles don't answer the question.\n    resolved_by_articles: Bool\n    /// The reply to show the customer.\n    reply: Text {\n        max_length: 200\n    }\n}\n",
        ),
        (
            "failures/assistant_unavailable.jadpo",
            "/// The assistant can't answer now.\ntype AssistantUnavailable = Failure {\n    kind: FailureKind.unavailable\n    message: \"The assistant can't answer now.\"\n}\n",
        ),
        (
            "prompts/answer_question.jadpo",
            "use support/types { CustomerQuestion }\n\n/// Answers a customer's question.\ntype AnswerQuestion = Prompt {\n    question: CustomerQuestion\n}\n",
        ),
        (
            "prompts/answer_question.prompt",
            "# Instructions\n\nYou answer questions from customers of an online bakery.\n\n# Request\n\n{question}\n",
        ),
        (
            "actions/meaning_of.jadpo",
            "use support/failures { AssistantUnavailable }\nuse support/types { Asked, Meaning, QuestionMeaning }\n\n/// A question's meaning.\naction meaning_of(asked: Asked) fails AssistantUnavailable -> Meaning {\n    var meaning: QuestionMeaning = attempt ai.embed(asked.question, as: QuestionMeaning) {\n        timed_out: AssistantUnavailable\n        over_budget: AssistantUnavailable\n        too_long: AssistantUnavailable\n    }\n    return Meaning {\n        meaning: meaning\n    }\n}\n",
        ),
        (
            "actions/meanings_of.jadpo",
            "use support/failures { AssistantUnavailable }\nuse support/types { CustomerQuestion, QuestionMeaning }\n\n/// Several questions' meanings, in order.\naction meanings_of(questions: List<CustomerQuestion>) fails AssistantUnavailable -> List<QuestionMeaning> {\n    return attempt ai.embed_all(questions, as: QuestionMeaning) {\n        timed_out: AssistantUnavailable\n        over_budget: AssistantUnavailable\n        too_long: AssistantUnavailable\n    }\n}\n",
        ),
        (
            "actions/answer_customer.jadpo",
            "use support/failures { AssistantUnavailable }\nuse support/prompts { AnswerQuestion }\nuse support/types { CustomerQuestion, SupportAnswer }\n\n/// Answers a customer's question.\naction answer_customer(question: CustomerQuestion) fails AssistantUnavailable -> SupportAnswer {\n    return attempt ai.generate(AnswerQuestion { question: question }, as: SupportAnswer) {\n        refused: AssistantUnavailable\n        malformed: AssistantUnavailable\n        timed_out: AssistantUnavailable\n        over_budget: AssistantUnavailable\n        too_long: AssistantUnavailable\n    }\n}\n",
        ),
        (
            "routes/meanings.jadpo",
            "use support/actions { meaning_of }\nuse support/types { Asked, Meaning }\n\n/// Questions' meanings.\nroutes Meanings {\n    prefix: /meanings\n\n    /// Anyone asks one.\n    POST / {\n        authentication: RouteAuthentication.none\n        input: Asked\n        output: Meaning\n        run: meaning_of(input)\n    }\n}\n",
        ),
    ] {
        write(&root, &format!("domains/support/{path}"), text);
    }
    // The counts of what its calls use are kept in `main` (chunk 5.5v5).
    for args in [&["migrate", "new", "initial"][..], &["records", "update"]] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{args:?}: {}{}", done.stdout, done.stderr);
    }
    root
}

/// The version `jadpo ai show` gives the prompt `prompt`.
fn version(root: &Path, prompt: &str) -> String {
    let shown = jadpo(root, &["ai", "show", prompt, "--json"]);
    assert_eq!(shown.code, 0, "{}{}", shown.stdout, shown.stderr);
    let document: serde_json::Value = serde_json::from_str(&shown.stdout).expect("JSON");
    document["data"]["version"]
        .as_str()
        .expect("a version")
        .to_string()
}

/// A recording of `prompt` called `name` keeping `answer`, made from
/// `version`.
fn record(root: &Path, prompt: &str, name: &str, answer: &str, version: &str) {
    write(
        root,
        &format!("domains/support/prompts/recordings/{prompt}/{name}.json"),
        &format!("{{\n  \"answer\": {answer},\n  \"version\": \"{version}\"\n}}\n"),
    );
}

const TESTS: &str = r#"use support/actions { answer_customer, meanings_of }
use support/failures { AssistantUnavailable }
use support/types { Asked, CustomerQuestion, Meaning, QuestionMeaning, SupportAnswer }

test "the same text always gives the same meaning" {
    var first: Meaning = attempt meaning_of(Asked { question: "Sesame?" })
    var again: Meaning = attempt meaning_of(Asked { question: "Sesame?" })
    var other: Meaning = attempt meaning_of(Asked { question: "Rye?" })
    assert first == again
    assert ai.distance(first.meaning, again.meaning) < 0.000001
    assert ai.distance(first.meaning, other.meaning) > 0.01
    var both: List<QuestionMeaning> = attempt meanings_of(["Sesame?", "Rye?"])
    assert list.count(both) == 2
    assert list.first(both) == first.meaning
    assert list.last(both) == other.meaning
    var none_asked: List<QuestionMeaning> = attempt meanings_of([])
    assert list.count(none_asked) == 0
}

test "an embedding's failures are the app's own" {
    fakes: {
        ai.embed: [timed_out, over_budget, too_long]
    }
    assert meaning_of(Asked { question: "Sesame?" }) fails AssistantUnavailable
    assert meaning_of(Asked { question: "Sesame?" }) fails AssistantUnavailable
    assert meanings_of(["Sesame?", "Rye?"]) fails AssistantUnavailable
}

test "a recorded answer is the answer" {
    fakes: {
        ai.answer_question: [recorded("not_covered")]
    }
    var reply: SupportAnswer = attempt answer_customer("Do you deliver to Jersey?")
    assert reply.resolved_by_articles == false
}
"#;

/// Embeddings through the fake, an embedding's scripted failures, an
/// answer from a recording, and the recording decoded as a test of its
/// own; then a recording that doesn't decode fails, and one made from an
/// older prompt passes with an alert.
#[test]
fn jadpo_test_embeds_through_the_fake_and_answers_from_recordings() {
    let root = desk("fake");
    let now = version(&root, "answer_question");
    record(
        &root,
        "answer_question",
        "not_covered",
        "{\n    \"reply\": \"{Text}\",\n    \"resolved_by_articles\": false\n  }",
        &now,
    );
    write(
        &root,
        "domains/support/actions/meaning_of.test.jadpo",
        TESTS,
    );
    jadpo(&root, &["records", "update"]);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let tested = jadpo(&root, &["test"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(
        tested
            .stdout
            .contains("tested: 4 passed, 3 action tests, 1 recording test"),
        "{}",
        tested.stdout
    );
    assert!(!tested.stdout.contains("alert"), "{}", tested.stdout);

    // An answer its type doesn't read fails its recording's test.
    record(
        &root,
        "answer_question",
        "sesame",
        "{\n    \"reply\": \"{Text}\",\n    \"resolved_by_articles\": \"yes\"\n  }",
        &now,
    );
    let tested = jadpo(&root, &["test", "--kind", "recording"]);
    assert_eq!(tested.code, 1, "{}{}", tested.stdout, tested.stderr);
    assert!(
        tested
            .stdout
            .contains("answer_question's sesame recording decodes"),
        "{}",
        tested.stdout
    );
    assert!(
        tested.stdout.contains(
            "`answer_question`'s recording `sesame` doesn't decode: `type` on `resolved_by_articles`"
        ),
        "{}",
        tested.stdout
    );
    assert!(
        tested.stdout.contains("rule: test.recording_undecodable"),
        "{}",
        tested.stdout
    );
    fs::remove_file(root.join("domains/support/prompts/recordings/answer_question/sesame.json"))
        .expect("removed");

    // A prompt changed since its recording was made: it still decodes,
    // and the run says so.
    write(
        &root,
        "domains/support/prompts/answer_question.prompt",
        "# Instructions\n\nYou answer questions from customers of an online bakery, briefly.\n\n# Request\n\n{question}\n",
    );
    let changed = version(&root, "answer_question");
    assert_ne!(changed, now);
    let tested = jadpo(&root, &["test", "--json"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    let document: serde_json::Value = serde_json::from_str(&tested.stdout).expect("JSON");
    assert_eq!(
        document["data"]["alerts"],
        serde_json::json!([{
            "kind": "recording_stale",
            "file": "domains/support/prompts/recordings/answer_question/not_covered.json",
            "prompt": "answer_question",
            "name": "not_covered",
            "recorded": now,
            "now": changed,
            "next": "jadpo ai record answer_question --as not_covered",
        }])
    );
    let tested = jadpo(&root, &["test"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(
        tested.stdout.contains(&format!(
            "alert: `answer_question`'s recording `not_covered` was made from an older prompt ({}, now {}): its answer may no longer be what the model says",
            now.trim_start_matches("answer_question@"),
            changed.trim_start_matches("answer_question@")
        )),
        "{}",
        tested.stdout
    );
    assert!(
        tested
            .stdout
            .contains("next: jadpo ai record answer_question --as not_covered"),
        "{}",
        tested.stdout
    );
}

/// An embeddings answer, as an OpenAI-compatible server sends one.
fn embeddings(vectors: &[&[f32]]) -> String {
    let data: Vec<serde_json::Value> = vectors
        .iter()
        .enumerate()
        .map(|(index, vector)| {
            serde_json::json!({"object": "embedding", "index": index, "embedding": vector})
        })
        .collect();
    serde_json::json!({
        "object": "list",
        "data": data,
        "model": "local/nomic-embed-text",
        "usage": {"prompt_tokens": 4, "total_tokens": 4}
    })
    .to_string()
}

/// A local model embeds through `Driver.openai_compatible` in `jadpo dev`:
/// OpenAI's embeddings request, the vector read as the type; a vector of
/// another length is the model `ai:` doesn't name, `misconfigured`.
#[test]
fn a_local_model_embeds_through_the_driver_in_jadpo_dev() {
    let model = FakeProvider::start();
    let root = desk("dev");
    write(
        &root,
        ".env.local",
        &format!("MODEL_URL={}/v1\n", model.url),
    );
    let dev = Dev::start_without_database(&root);
    wait_for(&dev, "\"listening\"");

    let vector: [f32; 8] = [0.5, -0.25, 0.125, 0.0, 1.0, 0.75, -0.5, 0.25];
    model.answer(200, &embeddings(&[&vector]));
    let answered = dev.send(
        "POST",
        "/meanings",
        &[],
        &serde_json::json!({ "question": "Sesame?" }).to_string(),
    );
    assert_eq!(answered.status, 200, "{answered:?}");
    assert_eq!(
        answered.json()["meaning"],
        serde_json::json!([0.5, -0.25, 0.125, 0, 1, 0.75, -0.5, 0.25])
    );
    let sent = model.requests();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].method, "POST");
    assert_eq!(sent[0].path, "/v1/embeddings");
    assert_eq!(sent[0].header("authorization"), None, "no key at all");
    assert_eq!(
        sent[0].json(),
        serde_json::json!({"model": "local/nomic-embed-text", "input": ["Sesame?"]})
    );

    // A vector of another length isn't the type's.
    model.answer(200, &embeddings(&[&[0.5, 0.5]]));
    let refused = dev.send(
        "POST",
        "/meanings",
        &[],
        &serde_json::json!({ "question": "Rye?" }).to_string(),
    );
    assert_eq!(refused.status, 500, "{refused:?}");
    assert_eq!(refused.json()["error"]["code"], "misconfigured");

    // Too long a text for the model is the app's own failure.
    model.answer(
        400,
        &serde_json::json!({"error": {"message": "too long", "code": "context_length_exceeded"}})
            .to_string(),
    );
    let too_long = dev.send(
        "POST",
        "/meanings",
        &[],
        &serde_json::json!({ "question": "Rye?" }).to_string(),
    );
    assert_eq!(too_long.status, 503, "{too_long:?}");
    assert_eq!(too_long.json()["error"]["code"], "assistant_unavailable");

    let listed = jadpo(&root, &["calls", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&listed.stdout).expect("JSON");
    let calls = document["data"]["calls"].as_array().expect("calls").clone();
    assert_eq!(calls.len(), 3, "{}", listed.stdout);
    assert_eq!(calls[0]["connection"], "local_model");
    assert_eq!(calls[0]["operation"], "embed");
    assert_eq!(calls[0]["source"], "live");
    assert_eq!(calls[0]["url"], format!("{}/v1/embeddings", model.url));
    drop(dev);
}

/// The support desk of area 21 ("Typed output", "Embeddings and search"
/// and "Tests"), as files.
const SUPPORT: &[(&str, &str)] = &[
    (
        "types/article_slug.jadpo",
        "/// A help article's address, `reset-your-password`.\ntype ArticleSlug = Text {\n    max_length: 80\n}\n",
    ),
    (
        "types/article_title.jadpo",
        "/// A help article's title.\ntype ArticleTitle = Text {\n    max_length: 120\n}\n",
    ),
    (
        "types/article_meaning.jadpo",
        "/// A help article's meaning, as the configured embedding model makes it.\ntype ArticleMeaning = Vector {\n    dimensions: 1536\n    distance: VectorDistance.cosine\n}\n",
    ),
    (
        "types/customer_question.jadpo",
        "/// What a customer asked the help desk.\ntype CustomerQuestion = Text {\n    max_length: 2000\n}\n",
    ),
    (
        "types/article_excerpt.jadpo",
        "use support/entities { HelpArticle }\n\n/// What a reader sees of a help article in a list of answers.\ntype ArticleExcerpt = Output {\n    pick: [HelpArticle.id, HelpArticle.slug, HelpArticle.title]\n}\n",
    ),
    (
        "types/support_answer.jadpo",
        "use support/types { ArticleSlug }\n\n/// What the assistant says back, and which articles it drew on.\ntype SupportAnswer = Output {\n    /// False when the articles given don't answer the question.\n    resolved_by_articles: Bool\n    /// The reply to show the customer, at most a few short paragraphs.\n    reply: Text {\n        max_length: 2000\n    }\n    /// The slugs of the articles the reply used, from those given.\n    cited_slugs: List<ArticleSlug> {\n        max_length: 5\n    }\n}\n",
    ),
    (
        "failures/assistant_unavailable.jadpo",
        "/// The assistant can't answer now; a person will reply.\ntype AssistantUnavailable = Failure {\n    kind: FailureKind.unavailable\n    message: \"The assistant can't answer now; a person will reply.\"\n}\n",
    ),
    (
        "failures/embedding_delayed.jadpo",
        "/// An article's meaning couldn't be made now; it is made again later.\ntype EmbeddingDelayed = Failure {\n    kind: FailureKind.unavailable\n}\n",
    ),
    (
        "entities/help_article.jadpo",
        "use support/types { ArticleMeaning, ArticleSlug, ArticleTitle }\n\n/// An article on the help site, with its meaning once it is embedded.\nentity HelpArticle {\n    id: Uuid\n    slug: ArticleSlug {\n        unique: true\n    }\n    title: ArticleTitle\n    body: Markdown\n    shown_publicly: Bool\n    body_meaning: ArticleMeaning?\n}\n",
    ),
    (
        "queries/closest_articles.jadpo",
        "use support/entities { HelpArticle }\nuse support/types { ArticleExcerpt, ArticleMeaning }\n\n/// The five public articles nearest a question, none further than 0.6.\nquery closest_articles(question_meaning: ArticleMeaning) -> List<Nearest<ArticleExcerpt>> {\n    from: HelpArticle\n    where: HelpArticle.shown_publicly == true\n    where: ai.distance(HelpArticle.body_meaning, question_meaning) <= 0.6\n    order: [HelpArticle.body_meaning nearest question_meaning]\n    limit: 5\n}\n",
    ),
    (
        "prompts/answer_question.jadpo",
        "use support/types { ArticleExcerpt, CustomerQuestion }\n\n/// Answers a customer's question using only the help articles found for it.\ntype AnswerQuestion = Prompt {\n    question: CustomerQuestion\n    articles: List<ArticleExcerpt>\n}\n",
    ),
    (
        "prompts/answer_question.prompt",
        "# Instructions\n\nYou answer questions from customers of an online bakery, using only the\nhelp articles given with the question. If they don't answer it, set\nresolved_by_articles to false and say a person will reply. Never promise\nrefunds or delivery dates.\n\n# Request\n\n{question}\n\n{articles}\n",
    ),
    (
        "failures/slug_taken.jadpo",
        "/// Another article has this address.\ntype SlugTaken = Failure {\n    kind: FailureKind.conflict\n}\n",
    ),
    (
        "actions/add_article.jadpo",
        "use support/entities { HelpArticle }\nuse support/failures { SlugTaken }\nuse support/types { ArticleSlug, ArticleTitle }\n\n/// Adds an article to the help site, not yet embedded.\naction add_article(slug: ArticleSlug, title: ArticleTitle, body: Markdown) fails SlugTaken -> HelpArticle {\n    return attempt HelpArticle.create {\n        set: {\n            slug: slug\n            title: title\n            body: body\n            shown_publicly: true\n            body_meaning: none\n        }\n        conflict: {\n            HelpArticle.slug: SlugTaken\n        }\n    }\n}\n",
    ),
    (
        "actions/refresh_meaning.jadpo",
        "use support/entities { HelpArticle, HelpArticleNotFound }\nuse support/failures { EmbeddingDelayed }\nuse support/types { ArticleMeaning }\n\n/// Remakes a help article's embedding from its current text.\naction refresh_meaning(article_id: HelpArticle.id) fails EmbeddingDelayed, HelpArticleNotFound -> HelpArticle {\n    var article: HelpArticle = attempt HelpArticle.get(article_id)\n    var fresh_meaning: ArticleMeaning = attempt ai.embed(article.body, as: ArticleMeaning) {\n        timed_out: EmbeddingDelayed\n        over_budget: EmbeddingDelayed\n        too_long: EmbeddingDelayed\n    }\n    return HelpArticle.update(article) {\n        set: {\n            body_meaning: fresh_meaning\n        }\n    }\n}\n",
    ),
    (
        "actions/answer_customer.jadpo",
        "use support/failures { AssistantUnavailable }\nuse support/prompts { AnswerQuestion }\nuse support/queries { closest_articles }\nuse support/types { ArticleExcerpt, ArticleMeaning, CustomerQuestion, SupportAnswer }\n\n/// Answers a customer's question from the closest help articles.\naction answer_customer(asked: CustomerQuestion) fails AssistantUnavailable -> SupportAnswer {\n    var question_meaning: ArticleMeaning = attempt ai.embed(asked, as: ArticleMeaning) {\n        timed_out: AssistantUnavailable\n        over_budget: AssistantUnavailable\n        too_long: AssistantUnavailable\n    }\n    var nearest_excerpts: List<Nearest<ArticleExcerpt>> = closest_articles(question_meaning)\n    return attempt ai.generate(\n        AnswerQuestion {\n            question: asked\n            articles: list.map(nearest_excerpts, found -> found.item)\n        },\n        as: SupportAnswer,\n        max_output: 600\n    ) {\n        refused: AssistantUnavailable\n        malformed: AssistantUnavailable\n        timed_out: AssistantUnavailable\n        over_budget: AssistantUnavailable\n        too_long: AssistantUnavailable\n    }\n}\n",
    ),
    (
        "actions/answer_customer.test.jadpo",
        "use support/actions { add_article, refresh_meaning }\nuse support/entities { HelpArticle }\nuse support/prompts { AnswerQuestion }\nuse support/types { CustomerQuestion, SupportAnswer }\n\ntest \"a question is answered from the nearest article and a recording\" {\n    fakes: {\n        ai.answer_question: [recorded(\"sesame\")]\n    }\n    var allergens: HelpArticle = attempt add_article(\"allergens\", \"Allergens in our bread\", \"Our sourdough is baked on lines that also handle sesame.\")\n    var delivery: HelpArticle = attempt add_article(\"delivery\", \"Where we deliver\", \"We deliver across mainland Britain.\")\n    var embedded: HelpArticle = attempt refresh_meaning(allergens.id)\n    assert embedded.body_meaning != none\n    var also_embedded: HelpArticle = attempt refresh_meaning(delivery.id)\n    assert also_embedded.body_meaning != embedded.body_meaning\n    var reply: SupportAnswer = attempt answer_customer(\"Our sourdough is baked on lines that also handle sesame.\")\n    assert reply.resolved_by_articles\n    assert reply.cited_slugs == [\"allergens\"]\n    var asked: List<AnswerQuestion> = ai.answer_question.requests\n    assert list.count(asked) == 1\n    assert list.map(asked, request -> list.map(request.articles, article -> article.slug)) == [[\"allergens\"]]\n}\n\ntest \"a question no article is near gets none\" {\n    fakes: {\n        ai.answer_question: [recorded(\"not_covered\")]\n    }\n    var delivery: HelpArticle = attempt add_article(\"delivery\", \"Where we deliver\", \"We deliver across mainland Britain.\")\n    var embedded: HelpArticle = attempt refresh_meaning(delivery.id)\n    assert embedded.body_meaning != none\n    var reply: SupportAnswer = attempt answer_customer(\"Do you deliver to Jersey?\")\n    assert reply.resolved_by_articles == false\n    var asked: List<AnswerQuestion> = ai.answer_question.requests\n    assert list.map(asked, request -> list.count(request.articles)) == [0]\n}\n",
    ),
];

/// The support example of area 21 end to end, in `jadpo test` against
/// PostgreSQL with pgvector: articles embedded by the fake and stored, the
/// question embedded and its nearest article found through the HNSW
/// index, and the typed answer from a recording.
#[test]
fn the_support_desk_answers_from_the_nearest_article_and_a_recording() {
    if !jadpo_test::live() {
        return;
    }
    if crate::vectors::pgvector_too_old() {
        return;
    }
    let root = folder("support");
    write(&root, "app.jadpo", &app(1536));
    write(&root, "config.jadpo", CONFIG);
    for (path, text) in SUPPORT {
        write(&root, &format!("domains/support/{path}"), text);
    }
    let now = version(&root, "answer_question");
    record(
        &root,
        "answer_question",
        "sesame",
        "{\n    \"cited_slugs\": [\"allergens\"],\n    \"reply\": \"{Text}\",\n    \"resolved_by_articles\": true\n  }",
        &now,
    );
    record(
        &root,
        "answer_question",
        "not_covered",
        "{\n    \"cited_slugs\": [],\n    \"reply\": \"{Text}\",\n    \"resolved_by_articles\": false\n  }",
        &now,
    );
    let migrated = jadpo(&root, &["migrate", "new", "articles"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    jadpo(&root, &["records", "update"]);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let tested = jadpo(&root, &["test"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(
        tested
            .stdout
            .contains("tested: 4 passed, 2 action tests, 2 recording tests"),
        "{}",
        tested.stdout
    );
}
