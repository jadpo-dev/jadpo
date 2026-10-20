//! `jadpo ai record <prompt> --as <name>` (area 21, "Tests"; chunk
//! 5.5v4b): the configured model asked once, with the values of the
//! prompt's last `jadpo dev` call or a fixture's, and its answer kept as
//! the prompt's recording, its plain values placeholders, which a test
//! answers with through `recorded(...)`. The model is a server that speaks
//! OpenAI's API started in the test on this machine: never a real
//! provider, the network or a key.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

use crate::http::{Dev, wait_for};
use crate::support;
use crate::support::fake_provider::FakeProvider;

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("ai-record-5.5v4b-{name}"));
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
    let env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    let mut output = run(&args, &env.with_cwd(cwd));
    output.stdout = support::without_run_time(&output.stdout);
    output
}

const RECORDING: &str = "domains/support/prompts/recordings/answer_question/rye.json";

/// The desk: a prompt asked by an action a route runs, through a model on
/// this machine, and a fixture that makes the prompt's value.
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
    for (path, text) in [
        (
            "types/customer_question.jadpo",
            "/// What a customer asked.\ntype CustomerQuestion = Text {\n    max_length: 200\n}\n",
        ),
        (
            "types/asked.jadpo",
            "use support/types { CustomerQuestion }\n\n/// A question, as the desk sends it.\ntype Asked = Input {\n    question: CustomerQuestion\n}\n",
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
            "fixtures/rye_question.jadpo",
            "use support/prompts { AnswerQuestion }\n\n/// A question about rye bread.\nfixture rye_question() -> AnswerQuestion {\n    return AnswerQuestion {\n        question: \"Do you bake rye on Sundays?\"\n    }\n}\n",
        ),
        (
            "actions/answer_customer.jadpo",
            "use support/failures { AssistantUnavailable }\nuse support/prompts { AnswerQuestion }\nuse support/types { Asked, SupportAnswer }\n\n/// Answers a customer's question.\naction answer_customer(asked: Asked) fails AssistantUnavailable -> SupportAnswer {\n    return attempt ai.generate(AnswerQuestion { question: asked.question }, as: SupportAnswer, timeout: 10s) {\n        refused: AssistantUnavailable\n        malformed: AssistantUnavailable\n        timed_out: AssistantUnavailable\n        over_budget: AssistantUnavailable\n        too_long: AssistantUnavailable\n    }\n}\n",
        ),
        (
            "routes/answers.jadpo",
            "use support/actions { answer_customer }\nuse support/types { Asked, SupportAnswer }\n\n/// Customers' questions.\nroutes Answers {\n    prefix: /answers\n\n    /// Anyone asks one.\n    POST / {\n        authentication: RouteAuthentication.none\n        input: Asked\n        output: SupportAnswer\n        deadline: 30s\n        run: answer_customer(input)\n    }\n}\n",
        ),
    ] {
        write(&root, &format!("domains/support/{path}"), text);
    }
    // The counts of what its calls use are kept in `main` (chunk 5.5v5).
    for args in [&["migrate", "new", "initial"][..], &["api", "update"]] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{args:?}: {}{}", done.stdout, done.stderr);
    }
    root
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

/// What the last request to the model asked, as its user message says.
fn asked(model: &FakeProvider) -> String {
    let sent = model.requests();
    let last = sent.last().expect("a request");
    assert_eq!(last.path, "/v1/chat/completions");
    last.json()["messages"][1]["content"]
        .as_str()
        .expect("a user message")
        .to_string()
}

const TESTS: &str = r#"use support/types { Asked, SupportAnswer }

test "a question about rye is answered as the model did" {
    fakes: {
        ai.answer_question: [recorded("rye")]
    }
    var reply: SupportAnswer = attempt answer_customer(Asked { question: "Rye?" })
    assert reply.resolved_by_articles
}
"#;

/// The model asked with the last `jadpo dev` call's values, its answer
/// kept with its plain values as placeholders and the prompt's version,
/// and a test answered from it through `recorded(...)`; recorded again
/// with the same answer it is left as it was, and a fixture's values ask
/// it too.
#[test]
fn a_recording_made_by_jadpo_ai_record_answers_a_test() {
    let model = FakeProvider::start();
    let root = desk("dev");
    write(
        &root,
        ".env.local",
        &format!("MODEL_URL={}/v1\n", model.url),
    );

    // Nothing to ask with before `jadpo dev` has asked it.
    let early = jadpo(&root, &["ai", "record", "answer_question", "--as", "rye"]);
    assert_eq!(early.code, 1, "{}{}", early.stdout, early.stderr);
    assert!(
        early
            .stderr
            .contains("error: Nothing to ask `answer_question` with")
            && early.stderr.contains("rule: recording.refused"),
        "{}",
        early.stderr
    );
    assert!(model.requests().is_empty(), "nothing sent");

    let dev = Dev::start_without_database(&root);
    wait_for(&dev, "\"listening\"");
    model.answer(
        200,
        &completion(r#"{"resolved_by_articles": true, "reply": "Yes, every Sunday."}"#),
    );
    let answered = dev.send(
        "POST",
        "/answers",
        &[],
        &serde_json::json!({ "question": "Is your rye bread baked on Sundays, Ada?" }).to_string(),
    );
    assert_eq!(answered.status, 200, "{answered:?}");
    drop(dev);

    model.answer(
        200,
        &completion(r#"{"resolved_by_articles": true, "reply": "Yes, Ada, every Sunday."}"#),
    );
    let recorded = jadpo(&root, &["ai", "record", "answer_question", "--as", "rye"]);
    assert_eq!(recorded.code, 0, "{}{}", recorded.stdout, recorded.stderr);
    assert_eq!(
        asked(&model),
        "<question>\nIs your rye bread baked on Sundays, Ada?\n</question>",
        "asked with the dev call's values"
    );
    let version = {
        let shown = jadpo(&root, &["ai", "show", "answer_question", "--json"]);
        let document: serde_json::Value = serde_json::from_str(&shown.stdout).expect("JSON");
        document["data"]["version"]
            .as_str()
            .expect("version")
            .to_string()
    };
    let text = fs::read_to_string(root.join(RECORDING)).expect("the recording");
    assert_eq!(
        text,
        format!(
            "{{\n  \"answer\": {{\n    \"reply\": \"{{Text}}\",\n    \"resolved_by_articles\": true\n  }},\n  \"version\": \"{version}\"\n}}\n"
        ),
        "no personal data kept"
    );
    assert!(
        recorded.stdout.contains(&format!(
            "Recorded `answer_question`'s answer as `rye`\n    asked   local/llama3.2 through local_model\n    from    the last `jadpo dev` call, call_1\n    answer  decodes as SupportAnswer\n    values  1 placeholder\n    file    {RECORDING}\n"
        )) && recorded.stdout.contains(&format!("answer recorded: {version}, rye"))
            && recorded
                .stdout
                .contains("next: jadpo test  decodes the recording and answers tests with it"),
        "{}",
        recorded.stdout
    );

    // A test answered from it, and the recording decoded as its own test.
    write(
        &root,
        "domains/support/actions/answer_customer.test.jadpo",
        TESTS,
    );
    let tested = jadpo(&root, &["test"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(
        tested
            .stdout
            .contains("2 passed, 1 action test, 1 recording test")
            && !tested.stdout.contains("alert"),
        "{}",
        tested.stdout
    );

    // The same answer again leaves the file as it was.
    model.answer(
        200,
        &completion(r#"{"resolved_by_articles": true, "reply": "Yes."}"#),
    );
    let again = jadpo(
        &root,
        &["ai", "record", "answer_question", "--as", "rye", "--json"],
    );
    assert_eq!(again.code, 0, "{}{}", again.stdout, again.stderr);
    let document: serde_json::Value = serde_json::from_str(&again.stdout).expect("JSON");
    assert_eq!(document["data"]["written"], false);
    assert_eq!(document["data"]["from"]["kind"], "dev_call");
    assert_eq!(document["data"]["placeholders"], 1);
    assert_eq!(
        document["data"]["sent"]["url"],
        format!("{}/v1/chat/completions", model.url)
    );

    // A fixture's values.
    model.answer(
        200,
        &completion(r#"{"resolved_by_articles": false, "reply": "A person will reply."}"#),
    );
    let fixture = jadpo(
        &root,
        &[
            "ai",
            "record",
            "answer_question",
            "--as",
            "sundays",
            "--fixture",
            "rye_question",
        ],
    );
    assert_eq!(fixture.code, 0, "{}{}", fixture.stdout, fixture.stderr);
    assert_eq!(
        asked(&model),
        "<question>\nDo you bake rye on Sundays?\n</question>"
    );
    assert!(
        fixture.stdout.contains("from    fixture `rye_question`"),
        "{}",
        fixture.stdout
    );
    let kept = fs::read_to_string(
        root.join("domains/support/prompts/recordings/answer_question/sundays.json"),
    )
    .expect("the fixture's recording");
    assert!(kept.contains("\"resolved_by_articles\": false"), "{kept}");

    // A changed prompt's recording names this command as the next step.
    write(
        &root,
        "domains/support/prompts/answer_question.prompt",
        "# Instructions\n\nYou answer questions from customers of a bakery.\n\n# Request\n\n{question}\n",
    );
    let stale = jadpo(&root, &["test"]);
    assert_eq!(stale.code, 0, "{}{}", stale.stdout, stale.stderr);
    assert!(
        stale.stdout.contains(
            "next: jadpo ai record answer_question --as rye  records it again from the prompt now"
        ),
        "{}",
        stale.stdout
    );
}

/// What it refuses before asking anything, and an answer it can't keep.
#[test]
fn it_refuses_what_it_cant_record_and_keeps_nothing_on_a_bad_answer() {
    let model = FakeProvider::start();
    let root = desk("refusals");
    write(
        &root,
        ".env.local",
        &format!("MODEL_URL={}/v1\n", model.url),
    );
    let refused = |args: &[&str], code: u8, wanted: &[&str]| {
        let done = jadpo(&root, args);
        assert_eq!(done.code, code, "{args:?}: {}{}", done.stdout, done.stderr);
        for line in wanted {
            assert!(done.stderr.contains(line), "{args:?}: {}", done.stderr);
        }
    };
    refused(
        &["ai", "record", "answer_question"],
        2,
        &[
            "error: `jadpo ai record` needs a name for the recording",
            "rule: cli.ai_arguments",
        ],
    );
    refused(
        &["ai", "record", "answer_question", "--as", "Rye Bread"],
        2,
        &["error: `Rye Bread` can't name a recording"],
    );
    refused(
        &["ai", "record", "answer_questoin", "--as", "rye"],
        2,
        &[
            "the closest is `answer_question`",
            "Run `jadpo ai record answer_question --as rye`",
        ],
    );
    refused(
        &[
            "ai",
            "record",
            "answer_question",
            "--as",
            "rye",
            "--fixture",
            "rye_questoin",
        ],
        2,
        &[
            "error: `rye_questoin` isn't a fixture in this project",
            "Use `--fixture rye_question`",
        ],
    );

    // A call logged with values the prompt no longer reads.
    write(
        &root,
        "build/dev/calls.jsonl",
        &format!(
            "{}\n",
            serde_json::json!({
                "id": "call_7",
                "connection": "local_model",
                "operation": "answer_question",
                "values": "{\"question\":7}"
            })
        ),
    );
    refused(
        &["ai", "record", "answer_question", "--as", "rye"],
        1,
        &[
            "error: The last `jadpo dev` call's values aren't `AnswerQuestion`'s now",
            "rule: recording.refused",
        ],
    );

    // An answer that never decodes keeps nothing.
    for _ in 0..2 {
        model.answer(200, &completion("Yes, on Sundays."));
    }
    refused(
        &[
            "ai",
            "record",
            "answer_question",
            "--as",
            "rye",
            "--fixture",
            "rye_question",
        ],
        1,
        &[
            "error: The model's answer to `answer_question` doesn't decode as the call's type",
            "Nothing was recorded.",
            "rule: recording.failed",
        ],
    );
    assert!(!root.join(RECORDING).exists(), "nothing kept");
}

/// Through a package's connection it is never live, and never with a
/// stand-in credential: refused before anything is sent.
#[test]
fn a_package_connection_is_never_recorded_live() {
    let provider = FakeProvider::start();
    let root = crate::ai_providers::desk(
        "record-live",
        &provider.url,
        crate::ai_providers::STRICT,
        "",
    );
    let unset = jadpo(&root, &["ai", "record", "answer_question", "--as", "rye"]);
    assert_eq!(unset.code, 1, "{}{}", unset.stdout, unset.stderr);
    assert!(
        unset
            .stderr
            .contains("error: `acme` has nothing set for its sandbox")
            && unset
                .stderr
                .contains("`jadpo ai record` calls the provider's sandbox")
            && unset.stderr.contains("rule: recording.refused"),
        "{}",
        unset.stderr
    );
    write(
        &root,
        ".env.local",
        "ACME_KEY=ak_test_3\nACME_ENVIRONMENT=live\n",
    );
    let live = jadpo(&root, &["ai", "record", "answer_question", "--as", "rye"]);
    assert_eq!(live.code, 1, "{}{}", live.stdout, live.stderr);
    assert!(
        live.stderr
            .contains("error: `acme` calls its provider's live API")
            && live.stderr.contains("rule: recording.refused"),
        "{}",
        live.stderr
    );
    assert!(provider.requests().is_empty(), "nothing sent");
}
