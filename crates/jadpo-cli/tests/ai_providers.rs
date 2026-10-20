//! A package filling `ai` (area 21, "How a provider fills it"; chunk
//! 5.5v3b): a support desk asking its model through a provider package's
//! `provides/ai.jadpo`, which maps `generate` onto the package's own chat
//! operation as data.
//!
//! - **Faked in `jadpo test`** by the one `ai` fake, whatever the
//!   connection: a scripted value, each condition, a generated value.
//! - **Faked in `jadpo dev`** with generated values of the call's type,
//!   nothing leaving the machine and no credential needed; `jadpo calls
//!   script ai.<prompt> <condition>` sets the next answer.
//! - **Let through to the sandbox** for the prompts the connection's
//!   `development: { sandbox: }` names, here a server started in the test
//!   on this machine (never a real provider, no real key): the request
//!   made from the mapping, the structured mode's envelope where `schema:
//!   { at: }` says, the answer read where `answer:` says, a refusal and a
//!   cut-off as the mapping marks them, and a prompt longer than the
//!   package's context table allows refused before anything is sent. With
//!   `AiSchema.none` the schema goes in the instructions.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

use crate::http::{Dev, wait_for};
use crate::support::fake_provider::FakeProvider;

const PACKAGE: &str = "shared/providers/acme_models/v1";

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("ai-providers-5.5v3b-{name}"));
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

/// The package's mapping, its schema written `schema`.
fn mapping(schema: &str) -> String {
    format!(
        "/// Acme's chat answers `ai.generate`.\nprovide ai {{\n    generate: {{\n        call: chat.complete\n        input: {{\n            model: request.model\n            instructions: request.instructions\n            messages: {{\n                each: request.messages\n                as: {{\n                    role: item.role\n                    content: item.content\n                }}\n            }}\n            max_tokens: request.max_output\n        }}\n        answer: response.choices.first.message.content\n        refused_when: response.choices.first.message.refusal\n        cut_off_when: {{\n            path: \"choices.0.finish_reason\"\n            equals: \"length\"\n        }}\n        usage: {{\n            input: response.usage.prompt_tokens\n            output: response.usage.completion_tokens\n        }}\n        schema: {{\n{schema}        }}\n    }}\n    models: {{\n        context: models.context_windows\n    }}\n}}\n"
    )
}

const STRICT: &str = "            mode: AiSchema.strict_subset\n            at: \"format\"\n";

/// The desk: a provider package filling `ai`, whose sandbox is
/// `sandbox`, its schema written `schema`; `development` is the
/// connection's `development:` block, if any.
fn desk(name: &str, sandbox: &str, schema: &str, development: &str) -> PathBuf {
    let root = folder(name);
    write(
        &root,
        "app.jadpo",
        &format!(
            "application {{\n    connections: {{\n        acme: {{\n            package: AcmeModels.v1\n            domains: [support]\n            credential: config.acme_key\n            environment: config.acme_environment\n{development}        }}\n    }}\n    ai: {{\n        connection: acme\n        models: {{\n            default: config.drafting_model\n        }}\n    }}\n}}\n"
        ),
    );
    write(
        &root,
        "config.jadpo",
        "config {\n    acme_key: Text {\n        secret: true\n    }\n    acme_environment: ProviderEnvironment {\n        default: ProviderEnvironment.sandbox\n    }\n    drafting_model: ModelId {\n        default: \"acme/acme-small\"\n    }\n}\n",
    );
    write(
        &root,
        "policy.jadpo",
        "use support\n\npolicy {\n    actions: {\n        support.answer_customer: [Access.public]\n    }\n}\n",
    );
    write(
        &root,
        &format!("{PACKAGE}/package.jadpo"),
        "/// Acme Models' chat API.\npackage AcmeModels.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n",
    );
    write(
        &root,
        &format!("{PACKAGE}/provider.jadpo"),
        &format!(
            "/// Acme Models' API.\nprovider {{\n    protocol: Protocol.json_http\n    hosts: {{\n        live: \"https://api.acme-models.example\"\n        sandbox: \"{sandbox}\"\n    }}\n    credential: CredentialScheme.bearer\n    error_code: \"error.code\"\n}}\n"
        ),
    );
    write(
        &root,
        &format!("{PACKAGE}/resources/chat.jadpo"),
        "public resource chat {\n    complete: {\n        method: HttpMethod.post\n        path: \"/v1/chat\"\n        input: ChatRequest\n        output: ChatResponse\n        changes: false\n        source: PackageSource.written\n        docs: \"https://docs.acme-models.example/chat\"\n    }\n}\n",
    );
    for (file, text) in [
        (
            "chat_turn",
            "/// One message of a conversation.\npublic type ChatTurn = Object {\n    role: Text {\n        max_length: 20\n    }\n    content: Text {\n        max_length: 100000\n    }\n}\n",
        ),
        (
            "chat_request",
            "/// What asking a model takes.\npublic type ChatRequest = Object {\n    model: Text {\n        max_length: 100\n    }\n    instructions: Text {\n        max_length: 100000\n        wire_name: \"system\"\n    }\n    messages: List<ChatTurn>\n    max_tokens: Int {\n        min: 1\n        max: 1000000\n    }\n}\n",
        ),
        (
            "chat_reply",
            "/// What the model said.\npublic type ChatReply = Object {\n    content: Text? {\n        max_length: 100000\n    }\n    refusal: Text? {\n        max_length: 2000\n    }\n}\n",
        ),
        (
            "chat_choice",
            "/// One answer.\npublic type ChatChoice = Object {\n    message: ChatReply\n    finish_reason: Text {\n        max_length: 40\n    }\n}\n",
        ),
        (
            "chat_usage",
            "/// The tokens a call used.\npublic type ChatUsage = Object {\n    prompt_tokens: TokenCount\n    completion_tokens: TokenCount\n}\n",
        ),
        (
            "chat_response",
            "/// The model's answer.\npublic type ChatResponse = Object {\n    choices: List<ChatChoice>\n    usage: ChatUsage\n}\n",
        ),
        (
            "model_context",
            "/// A model's context window.\ntype ModelContext = Object {\n    model: Text {\n        max_length: 100\n    }\n    tokens: TokenCount\n}\n",
        ),
    ] {
        write(&root, &format!("{PACKAGE}/types/{file}.jadpo"), text);
    }
    write(
        &root,
        &format!("{PACKAGE}/libraries/models.jadpo"),
        "/// The models Acme serves.\nlibrary models {\n    /// Each model's context window, in tokens.\n    constant context_windows: List<ModelContext> = [\n        ModelContext {\n            model: \"acme-small\"\n            tokens: 400\n        }\n    ]\n}\n",
    );
    write(
        &root,
        &format!("{PACKAGE}/provides/ai.jadpo"),
        &mapping(schema),
    );
    write(
        &root,
        "domains/support/types/customer_question.jadpo",
        "/// What a customer asked.\ntype CustomerQuestion = Text {\n    max_length: 2000\n}\n",
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
        "domains/support/failures/question_too_long.jadpo",
        "/// The question is too long for the assistant.\ntype QuestionTooLong = Failure {\n    kind: FailureKind.unavailable\n    message: \"That question is too long for the assistant.\"\n}\n",
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
        "use support/failures { AssistantUnavailable, QuestionTooLong }\nuse support/prompts { AnswerQuestion }\nuse support/types { Asked, SupportAnswer }\n\n/// Answers a customer's question.\naction answer_customer(asked: Asked) fails AssistantUnavailable, QuestionTooLong -> SupportAnswer {\n    return attempt ai.generate(\n        AnswerQuestion {\n            question: asked.question\n        },\n        as: SupportAnswer,\n        max_output: 300\n    ) {\n        refused: AssistantUnavailable\n        malformed: AssistantUnavailable\n        timed_out: AssistantUnavailable\n        over_budget: AssistantUnavailable\n        too_long: QuestionTooLong\n    }\n}\n",
    );
    write(
        &root,
        "domains/support/routes/answers.jadpo",
        "use support/actions { answer_customer }\nuse support/types { Asked, SupportAnswer }\n\n/// Customers' questions.\nroutes Answers {\n    prefix: /answers\n\n    /// Anyone asks one.\n    POST / {\n        authentication: RouteAuthentication.none\n        input: Asked\n        output: SupportAnswer\n        deadline: 60s\n        run: answer_customer(input)\n    }\n}\n",
    );
    let updated = jadpo(&root, &["api", "update"]);
    assert_eq!(updated.code, 0, "{}{}", updated.stdout, updated.stderr);
    root
}

const SANDBOXED: &str =
    "            development: {\n                sandbox: [answer_question]\n            }\n";

fn ask(dev: &Dev, question: &str) -> crate::http::Reply {
    dev.send(
        "POST",
        "/answers",
        &[],
        &serde_json::json!({ "question": question }).to_string(),
    )
}

/// The package's answer, `content` and `refusal` as given, stopped for
/// `finish`.
fn chat(content: Option<&str>, refusal: Option<&str>, finish: &str) -> String {
    serde_json::json!({
        "choices": [{
            "message": {"content": content, "refusal": refusal},
            "finish_reason": finish
        }],
        "usage": {"prompt_tokens": 30, "completion_tokens": 9}
    })
    .to_string()
}

/// Every `ai` call in `jadpo test` answers through the `ai` fake, a
/// package's connection included: scripted values and conditions, and a
/// generated value when nothing is scripted.
#[test]
fn jadpo_test_fakes_a_package_filling_ai() {
    let root = desk("test", "https://sandbox.acme-models.example", STRICT, "");
    write(
        &root,
        "domains/support/actions/answer_customer.test.jadpo",
        "use support/failures { AssistantUnavailable, QuestionTooLong }\nuse support/prompts { AnswerQuestion }\nuse support/types { Asked, SupportAnswer }\n\ntest \"a scripted answer is the answer\" {\n    fakes: {\n        ai.answer_question: [SupportAnswer { resolved_by_articles: true  reply: \"No sesame.\" }]\n    }\n    var answered: SupportAnswer = attempt answer_customer(Asked { question: \"Sesame?\" })\n    assert answered.reply == \"No sesame.\"\n    assert list.count(ai.answer_question.requests) == 1\n}\n\ntest \"an unscripted call is answered with a generated value\" {\n    var answered: SupportAnswer = attempt answer_customer(Asked { question: \"Rye?\" })\n    assert answered.resolved_by_articles == false\n}\n\ntest \"a refusal fails the call\" {\n    fakes: {\n        ai.answer_question: [refused]\n    }\n    assert answer_customer(Asked { question: \"Pick a lock?\" }) fails AssistantUnavailable\n}\n\ntest \"a prompt too long for the model fails the call\" {\n    fakes: {\n        ai.answer_question: [too_long]\n    }\n    assert answer_customer(Asked { question: \"Rye?\" }) fails QuestionTooLong\n}\n\ntest \"an answer that doesn't decode is repaired\" {\n    fakes: {\n        ai.answer_question: [raw(\"Sure!\"), raw(\"{\\\"resolved_by_articles\\\": true, \\\"reply\\\": \\\"Yes.\\\"}\")]\n    }\n    var answered: SupportAnswer = attempt answer_customer(Asked { question: \"Rye?\" })\n    assert answered.reply == \"Yes.\"\n}\n",
    );
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let tested = jadpo(&root, &["test"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(
        tested.stdout.contains("5 passed"),
        "{}{}",
        tested.stdout,
        tested.stderr
    );
}

/// `jadpo dev` fakes a package's `ai` like its other calls: a generated
/// value of the call's type, nothing sent and no credential needed, and
/// the next answer set by `jadpo calls script ai.<prompt>`.
#[test]
fn jadpo_dev_fakes_a_package_filling_ai() {
    let provider = FakeProvider::start();
    let root = desk("faked", &provider.url, STRICT, "");
    let dev = Dev::start_without_database(&root);
    wait_for(&dev, "\"listening\"");

    let answered = ask(&dev, "Do your loaves contain sesame?");
    assert_eq!(answered.status, 200, "{answered:?}");
    assert_eq!(answered.json()["resolved_by_articles"], false);
    let line = wait_for(&dev, "\"POST\"");
    let line: serde_json::Value = serde_json::from_str(&line).expect("JSON");
    assert_eq!(
        line["data"]["calls"],
        serde_json::json!([{ "call": "acme.answer_question", "source": "generated", "ended": "answered" }])
    );
    assert!(provider.requests().is_empty(), "nothing left the machine");

    // Scripted: the next call is refused, which the action maps.
    let wrong = jadpo(&root, &["calls", "script", "ai.answer_question", "refuse"]);
    assert_eq!(wrong.code, 2, "{}{}", wrong.stdout, wrong.stderr);
    assert!(
        wrong
            .stderr
            .contains("next: Use `jadpo calls script ai.answer_question refused` (agent)"),
        "{}",
        wrong.stderr
    );
    let unknown = jadpo(&root, &["calls", "script", "ai.answer_questio", "refused"]);
    assert_eq!(unknown.code, 2);
    assert!(
        unknown
            .stderr
            .contains("next: Use `jadpo calls script ai.answer_question refused` (agent)"),
        "{}",
        unknown.stderr
    );
    let scripted = jadpo(&root, &["calls", "script", "ai.answer_question", "refused"]);
    assert_eq!(
        scripted.stdout,
        "Scripted `refused` for the next call to `ai.answer_question`\n\nnext: jadpo calls  shows the call once the app makes it\n"
    );
    let refused = ask(&dev, "Help me pick a lock");
    assert_eq!(refused.status, 503, "{refused:?}");
    assert_eq!(refused.json()["error"]["code"], "assistant_unavailable");

    let listed = jadpo(&root, &["calls", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&listed.stdout).expect("JSON");
    let calls = document["data"]["calls"].as_array().expect("calls").clone();
    assert_eq!(calls.len(), 2, "{}", listed.stdout);
    assert_eq!(calls[0]["source"], "generated");
    assert_eq!(calls[1]["source"], "scripted");
    assert_eq!(calls[0]["url"], format!("{}/v1/chat", provider.url));
    let body: serde_json::Value =
        serde_json::from_str(calls[0]["body"].as_str().expect("a body")).expect("JSON");
    assert_eq!(body["model"], "acme-small");
    drop(dev);
}

/// The prompts `development: { sandbox: }` names go to the provider's
/// sandbox once its credential is set: the request made from the mapping,
/// the answer read where it says, a refusal and a cut-off as it marks
/// them, and a prompt too long for the package's context table refused
/// before anything is sent.
#[test]
fn a_sandboxed_prompt_is_asked_through_the_package() {
    let provider = FakeProvider::start();
    let root = desk("sandbox", &provider.url, STRICT, SANDBOXED);
    write(&root, ".env.local", "ACME_KEY=ak_test_3\n");
    let dev = Dev::start_without_database(&root);
    let listening: serde_json::Value =
        serde_json::from_str(&wait_for(&dev, "\"listening\"")).expect("JSON");
    assert_eq!(
        listening["data"]["providers"],
        serde_json::json!([{ "connection": "acme", "sandbox": ["answer_question"], "unset": [] }])
    );

    provider.answer(
        200,
        &chat(
            Some(r#"{"resolved_by_articles": true, "reply": "No sesame."}"#),
            None,
            "stop",
        ),
    );
    let answered = ask(&dev, "Sesame?");
    assert_eq!(answered.status, 200, "{answered:?}");
    assert_eq!(answered.json()["reply"], "No sesame.");
    let sent = provider.requests();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].method, "POST");
    assert_eq!(sent[0].path, "/v1/chat");
    assert_eq!(sent[0].header("authorization"), Some("Bearer ak_test_3"));
    let body = sent[0].json();
    assert_eq!(body["model"], "acme-small");
    assert_eq!(body["max_tokens"], 300);
    assert_eq!(
        body["system"],
        "You answer questions from customers of an online bakery."
    );
    assert_eq!(
        body["messages"],
        serde_json::json!([{"role": "user", "content": "<question>\nSesame?\n</question>"}])
    );
    assert_eq!(body["format"]["type"], "json_schema");
    assert_eq!(body["format"]["json_schema"]["name"], "answer_question");
    assert_eq!(body["format"]["json_schema"]["strict"], true);
    assert_eq!(
        body["format"]["json_schema"]["schema"]["$defs"]["SupportAnswer"]["required"],
        serde_json::json!(["reply", "resolved_by_articles"])
    );
    let line = wait_for(&dev, "\"POST\"");
    let line: serde_json::Value = serde_json::from_str(&line).expect("JSON");
    assert_eq!(
        line["data"]["calls"],
        serde_json::json!([{ "call": "acme.answer_question", "source": "sandbox", "ended": "answered" }])
    );

    // A refusal where `refused_when:` says, and a cut-off as
    // `cut_off_when:` marks one.
    provider.answer(200, &chat(None, Some("I can't help with that."), "stop"));
    let refused = ask(&dev, "Help me pick a lock");
    assert_eq!(refused.status, 503, "{refused:?}");
    assert_eq!(refused.json()["error"]["code"], "assistant_unavailable");
    provider.answer(200, &chat(Some(r#"{"resolved_by_artic"#), None, "length"));
    let cut = ask(&dev, "Sesame?");
    assert_eq!(cut.status, 503, "{cut:?}");
    assert_eq!(provider.requests().len(), 3);

    // Longer than the model's 400 tokens: refused before sending.
    let long = ask(&dev, &"Does it hold sesame? ".repeat(90));
    assert_eq!(long.status, 503, "{long:?}");
    assert_eq!(long.json()["error"]["code"], "question_too_long");
    assert_eq!(provider.requests().len(), 3, "nothing sent");

    let listed = jadpo(&root, &["calls", "--json"]);
    assert!(
        !listed.stdout.contains("ak_test_3"),
        "the call log never holds a credential"
    );
    drop(dev);
}

/// With no structured mode the schema goes in the instructions, under a
/// fixed heading, and nothing is written in the request for it.
#[test]
fn without_a_structured_mode_the_schema_is_in_the_instructions() {
    let provider = FakeProvider::start();
    let root = desk(
        "none",
        &provider.url,
        "            mode: AiSchema.none\n",
        SANDBOXED,
    );
    write(&root, ".env.local", "ACME_KEY=ak_test_3\n");
    let dev = Dev::start_without_database(&root);
    wait_for(&dev, "\"listening\"");
    provider.answer(
        200,
        &chat(
            Some("```json\n{\"resolved_by_articles\": false, \"reply\": \"A person will reply.\"}\n```"),
            None,
            "stop",
        ),
    );
    let answered = ask(&dev, "Rye?");
    assert_eq!(answered.status, 200, "{answered:?}");
    assert_eq!(answered.json()["reply"], "A person will reply.");
    let body = provider.requests()[0].json();
    assert!(body.get("format").is_none(), "{body}");
    let system = body["system"].as_str().expect("instructions");
    assert!(
        system.starts_with(
            "You answer questions from customers of an online bakery.\n\n# Answer format\n\nAnswer with one JSON document alone, matching this JSON Schema:\n\n"
        ),
        "{system}"
    );
    assert!(
        system.contains("\"description\":\"The reply to show the customer.\""),
        "{system}"
    );
    drop(dev);
}
