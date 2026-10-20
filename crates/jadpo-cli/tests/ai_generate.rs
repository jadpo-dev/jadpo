//! The `ai` capability's `generate` (area 21, "The `ai` capability" and
//! "Typed output"; chunk 5.5v3): a support desk that answers a customer's
//! question as a typed `SupportAnswer`, through a model on this machine.
//!
//! - **The fake.** `jadpo test` answers every `ai` call through the `ai`
//!   fake: each of the five failures scripted, a repaired answer, a value,
//!   `raw(...)` text run through the real decoding, `.requests`, and an
//!   unscripted call's generated value; a script run past and one with
//!   answers left fail the test.
//! - **The local driver.** `jadpo dev` asks a server that speaks OpenAI's
//!   Chat Completions API, here one started in the test on this machine
//!   (never a real provider, never the network, no key): the schema sent in
//!   its structured mode, an answer that doesn't decode repaired with the
//!   problem, a refusal mapped, and each attempt in `jadpo calls`.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

use crate::http::{Dev, wait_for};
use crate::support::fake_provider::FakeProvider;

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("ai-generate-5.5v3-{name}"));
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
    run(&args, &Environment::empty().with_cwd(cwd))
}

/// The desk: a prompt, the type its answer is decoded into, an action
/// that asks the model and maps every failure, and a route that runs it.
fn desk(name: &str) -> PathBuf {
    let root = folder(name);
    write(
        &root,
        "app.jadpo",
        "application {\n    connections: {\n        local_model: {\n            driver: Driver.openai_compatible\n            url: config.model_url\n        }\n    }\n    ai: {\n        connection: local_model\n        models: {\n            default: config.drafting_model\n        }\n        limits: {\n            per_day: {\n                tokens: 2000000\n            }\n        }\n    }\n}\n",
    );
    write(
        &root,
        "config.jadpo",
        "config {\n    model_url: Url {\n        default: \"http://localhost:11434/v1\"\n    }\n    drafting_model: ModelId {\n        default: \"local/llama3.2\"\n    }\n}\n",
    );
    write(
        &root,
        "policy.jadpo",
        "use support\n\npolicy {\n    actions: {\n        support.answer_customer: [Access.public]\n    }\n}\n",
    );
    write(
        &root,
        "domains/support/types/customer_question.jadpo",
        "/// What a customer asked.\ntype CustomerQuestion = Text {\n    max_length: 500\n}\n",
    );
    write(
        &root,
        "domains/support/types/asked.jadpo",
        "use support/types { CustomerQuestion }\n\n/// A customer's question, as the desk sends it.\ntype Asked = Input {\n    question: CustomerQuestion\n}\n",
    );
    write(
        &root,
        "domains/support/types/support_answer.jadpo",
        "/// What the assistant says back.\ntype SupportAnswer = Output {\n    /// False when the help articles don't answer the question.\n    resolved_by_articles: Bool\n    /// The reply to show the customer.\n    reply: Text {\n        max_length: 40\n    }\n}\n",
    );
    write(
        &root,
        "domains/support/failures/assistant_unavailable.jadpo",
        "/// The assistant can't answer now.\ntype AssistantUnavailable = Failure {\n    kind: FailureKind.unavailable\n    message: \"The assistant can't answer now.\"\n}\n",
    );
    write(
        &root,
        "domains/support/failures/answer_unreadable.jadpo",
        "/// The assistant's answer couldn't be read.\ntype AnswerUnreadable = Failure {\n    kind: FailureKind.unavailable\n    message: \"The assistant's answer couldn't be read.\"\n    internal: {\n        problem: DecodeProblem\n    }\n}\n",
    );
    write(
        &root,
        "domains/support/prompts/answer_question.jadpo",
        "use support/types { CustomerQuestion }\n\n/// Answers a customer's question.\ntype AnswerQuestion = Prompt {\n    question: CustomerQuestion\n}\n",
    );
    write(
        &root,
        "domains/support/prompts/answer_question.prompt",
        "# Instructions\n\nYou answer questions from customers of an online bakery.\n\n# Request\n\n{question}\n",
    );
    write(
        &root,
        "domains/support/actions/answer_customer.jadpo",
        "use support/failures { AnswerUnreadable, AssistantUnavailable }\nuse support/prompts { AnswerQuestion }\nuse support/types { Asked, SupportAnswer }\n\n/// Answers a customer's question.\naction answer_customer(asked: Asked) fails AnswerUnreadable, AssistantUnavailable -> SupportAnswer {\n    return attempt ai.generate(\n        AnswerQuestion {\n            question: asked.question\n        },\n        as: SupportAnswer,\n        max_output: 600\n    ) {\n        refused: AssistantUnavailable\n        malformed: AnswerUnreadable\n        timed_out: AssistantUnavailable\n        over_budget: AssistantUnavailable\n        too_long: AssistantUnavailable\n    }\n}\n",
    );
    write(
        &root,
        "domains/support/routes/answers.jadpo",
        "use support/actions { answer_customer }\nuse support/types { Asked, SupportAnswer }\n\n/// Customers' questions.\nroutes Answers {\n    prefix: /answers\n\n    /// Anyone asks one.\n    POST / {\n        authentication: RouteAuthentication.none\n        input: Asked\n        output: SupportAnswer\n        deadline: 60s\n        run: answer_customer(input)\n    }\n}\n",
    );
    // The counts of what its calls use are kept in `main` (chunk 5.5v5).
    for args in [&["migrate", "new", "initial"][..], &["api", "update"]] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{args:?}: {}{}", done.stdout, done.stderr);
    }
    root
}

/// The tests a desk's developer writes: every failure, a repair, a value,
/// what was asked, and a generated answer.
const TESTS: &str = r#"use support/failures { AnswerUnreadable, AssistantUnavailable }
use support/prompts { AnswerQuestion }
use support/types { Asked, SupportAnswer }

test "a scripted answer is the answer" {
    fakes: {
        ai.answer_question: [SupportAnswer { resolved_by_articles: true  reply: "No sesame." }]
    }
    var answered: SupportAnswer = attempt answer_customer(Asked { question: "Sesame?" })
    assert answered.reply == "No sesame."
    var asked: List<AnswerQuestion> = ai.answer_question.requests
    assert list.count(asked) == 1
    assert list.first(asked) == AnswerQuestion { question: "Sesame?" }
}

test "an unscripted call is answered with a generated value" {
    var answered: SupportAnswer = attempt answer_customer(Asked { question: "Rye?" })
    assert answered.resolved_by_articles == false
}

test "an answer that doesn't decode is repaired" {
    fakes: {
        ai.answer_question: [raw("Sure! Here you go."), raw("{\"resolved_by_articles\": true, \"reply\": \"Yes.\"}")]
    }
    var answered: SupportAnswer = attempt answer_customer(Asked { question: "Rye?" })
    assert answered.reply == "Yes."
    assert list.count(ai.answer_question.requests) == 1
}

test "a refusal fails the call" {
    fakes: {
        ai.answer_question: [refused]
    }
    assert answer_customer(Asked { question: "Pick a lock?" }) fails AssistantUnavailable
}

test "an answer still malformed after its repair fails the call" {
    fakes: {
        ai.answer_question: [malformed, raw("{\"resolved_by_articles\": true, \"reply\": \"Far too long a reply for the desk to show.\"}")]
    }
    assert answer_customer(Asked { question: "Rye?" }) fails AnswerUnreadable
}

test "no whole answer in time fails the call" {
    fakes: {
        ai.answer_question: [timed_out]
    }
    assert answer_customer(Asked { question: "Rye?" }) fails AssistantUnavailable
}

test "an answer cut off fails the call" {
    fakes: {
        ai.answer_question: [over_budget]
    }
    assert answer_customer(Asked { question: "Rye?" }) fails AssistantUnavailable
}

test "a prompt too long for the model fails the call" {
    fakes: {
        ai.answer_question: [too_long]
    }
    assert answer_customer(Asked { question: "Rye?" }) fails AssistantUnavailable
}
"#;

/// Every failure and a repaired answer, driven through the fake: each
/// test passes, nothing reaches a model, and a script run past or left
/// with answers fails its test.
#[test]
fn jadpo_test_drives_every_failure_and_a_repair_through_the_fake() {
    let root = desk("fake");
    write(
        &root,
        "domains/support/actions/answer_customer.test.jadpo",
        TESTS,
    );
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let tested = jadpo(&root, &["test"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(
        tested.stdout.contains("8 passed"),
        "{}{}",
        tested.stdout,
        tested.stderr
    );

    // A repair the script didn't give an answer for runs past it; an
    // answer no call takes is left over.
    write(
        &root,
        "domains/support/actions/answer_customer.test.jadpo",
        "use support/failures { AssistantUnavailable }\nuse support/types { Asked, SupportAnswer }\n\ntest \"one malformed answer is repaired with nothing scripted\" {\n    fakes: {\n        ai.answer_question: [malformed]\n    }\n    var answered: SupportAnswer = attempt answer_customer(Asked { question: \"Rye?\" })\n    assert answered.resolved_by_articles == false\n}\n\ntest \"an answer is left over\" {\n    fakes: {\n        ai.answer_question: [refused, refused]\n    }\n    assert answer_customer(Asked { question: \"Rye?\" }) fails AssistantUnavailable\n}\n",
    );
    let tested = jadpo(&root, &["test"]);
    assert_eq!(tested.code, 1, "{}{}", tested.stdout, tested.stderr);
    assert!(
        tested.stdout.contains("rule: test.fake_exhausted"),
        "{}",
        tested.stdout
    );
    assert!(
        tested.stdout.contains("rule: test.fake_unused"),
        "{}",
        tested.stdout
    );
    assert!(
        tested.stdout.contains("`ai.answer_question`"),
        "{}",
        tested.stdout
    );
}

/// A chat completion answering with `content`.
fn completion(content: &str) -> String {
    serde_json::json!({
        "id": "chatcmpl-1",
        "object": "chat.completion",
        "model": "local/llama3.2",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": content},
            "finish_reason": "stop"
        }],
        "usage": {"prompt_tokens": 40, "completion_tokens": 12, "total_tokens": 52}
    })
    .to_string()
}

fn ask(dev: &Dev, question: &str) -> crate::http::Reply {
    dev.send(
        "POST",
        "/answers",
        &[],
        &serde_json::json!({ "question": question }).to_string(),
    )
}

/// A local model answers through `Driver.openai_compatible` in `jadpo
/// dev`: the prompt rendered as messages, the answer's schema in the
/// server's structured mode, the answer decoded into its type; one that
/// doesn't decode asked again with its problem; a refusal mapped to the
/// app's failure; and each attempt in `jadpo calls` with what was sent.
#[test]
fn a_local_model_answers_through_the_driver_in_jadpo_dev() {
    let model = FakeProvider::start();
    let root = desk("dev");
    write(
        &root,
        ".env.local",
        &format!("MODEL_URL={}/v1\n", model.url),
    );
    let dev = Dev::start_without_database(&root);
    wait_for(&dev, "\"listening\"");

    model.answer(
        200,
        &completion(r#"{"resolved_by_articles": true, "reply": "No sesame."}"#),
    );
    let answered = ask(&dev, "Do your loaves contain sesame?");
    assert_eq!(answered.status, 200, "{answered:?}");
    assert_eq!(answered.json()["reply"], "No sesame.");
    assert_eq!(answered.json()["resolved_by_articles"], true);
    let sent = model.requests();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].method, "POST");
    assert_eq!(sent[0].path, "/v1/chat/completions");
    assert_eq!(sent[0].header("authorization"), None, "no key at all");
    let body = sent[0].json();
    assert_eq!(body["model"], "local/llama3.2");
    assert_eq!(body["max_tokens"], 600);
    assert_eq!(
        body["messages"],
        serde_json::json!([
            {"role": "system", "content": "You answer questions from customers of an online bakery."},
            {"role": "user", "content": "<question>\nDo your loaves contain sesame?\n</question>"}
        ])
    );
    let format = &body["response_format"];
    assert_eq!(format["type"], "json_schema");
    assert_eq!(format["json_schema"]["name"], "answer_question");
    assert_eq!(format["json_schema"]["strict"], true);
    let answer = &format["json_schema"]["schema"]["$defs"]["SupportAnswer"];
    assert_eq!(answer["additionalProperties"], false);
    assert_eq!(
        answer["required"],
        serde_json::json!(["reply", "resolved_by_articles"])
    );
    assert_eq!(
        answer["properties"]["reply"],
        serde_json::json!({"type": "string", "description": "The reply to show the customer."}),
        "the limit it doesn't enforce is left for decoding"
    );
    let line = wait_for(&dev, "\"POST\"");
    let line: serde_json::Value = serde_json::from_str(&line).expect("JSON");
    assert_eq!(
        line["data"]["calls"],
        serde_json::json!([{ "call": "local_model.answer_question", "source": "live", "ended": "answered" }])
    );

    // An answer too long for its type is asked again, with the problem.
    model.answer(
        200,
        &completion(
            r#"{"resolved_by_articles": true, "reply": "Our sourdough is baked on lines that handle sesame."}"#,
        ),
    );
    model.answer(
        200,
        &completion(r#"{"resolved_by_articles": true, "reply": "It may hold sesame."}"#),
    );
    let repaired = ask(&dev, "Sesame?");
    assert_eq!(repaired.status, 200, "{repaired:?}");
    assert_eq!(repaired.json()["reply"], "It may hold sesame.");
    let sent = model.requests();
    assert_eq!(sent.len(), 3);
    let messages = sent[2].json()["messages"].clone();
    assert_eq!(messages.as_array().map(Vec::len), Some(4));
    assert_eq!(messages[2]["role"], "assistant");
    assert_eq!(
        messages[3],
        serde_json::json!({
            "role": "user",
            "content": "In your answer, `reply` breaks the rule `max_length`. Answer again with one JSON document alone that matches the schema."
        })
    );

    // A refusal is the app's own failure.
    model.answer(
        200,
        &serde_json::json!({
            "choices": [{"index": 0, "message": {"role": "assistant", "content": null, "refusal": "I can't help with that."}, "finish_reason": "stop"}]
        })
        .to_string(),
    );
    let refused = ask(&dev, "Help me pick a lock");
    assert_eq!(refused.status, 503, "{refused:?}");
    assert_eq!(refused.json()["error"]["code"], "assistant_unavailable");

    // Every attempt in the call log, sent and answered in full.
    let listed = jadpo(&root, &["calls", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&listed.stdout).expect("JSON");
    let calls = document["data"]["calls"].as_array().expect("calls").clone();
    assert_eq!(calls.len(), 4, "{}", listed.stdout);
    assert_eq!(calls[0]["connection"], "local_model");
    assert_eq!(calls[0]["operation"], "answer_question");
    assert_eq!(calls[0]["source"], "live");
    assert_eq!(calls[0]["method"], "POST");
    assert_eq!(
        calls[0]["url"],
        format!("{}/v1/chat/completions", model.url)
    );
    assert!(
        calls[0]["body"]
            .as_str()
            .is_some_and(|body| body.contains("Do your loaves contain sesame?")),
        "{}",
        listed.stdout
    );
    drop(dev);
}

/// With no server where `url:` points, the call never left: `unavailable`,
/// once its sends are spent, never a made-up answer.
#[test]
fn a_model_that_cant_be_reached_is_unavailable() {
    let root = desk("unreached");
    let closed = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    let port = closed.local_addr().expect("address").port();
    drop(closed);
    write(
        &root,
        ".env.local",
        &format!("MODEL_URL=http://127.0.0.1:{port}/v1\n"),
    );
    let dev = Dev::start_without_database(&root);
    wait_for(&dev, "\"listening\"");
    let reply = ask(&dev, "Sesame?");
    assert_eq!(reply.status, 503, "{reply:?}");
    assert_eq!(reply.json()["error"]["code"], "unavailable");
    drop(dev);
}
