//! The OpenAI and Anthropic packages filling `ai` (area 21, "How a
//! provider fills it"; chunk 5.5v7): `packages/openai` and
//! `packages/anthropic`, installed as an app installs them with `jadpo
//! package add`, each asked through `jadpo dev` against a server started
//! in the test on this machine, never a real provider, the network or a
//! real key.
//!
//! - **OpenAI** answers `ai.generate` as a Chat Completion in its strict
//!   structured-output mode, `ai.stream` as a streamed one ended by `data:
//!   [DONE]`, and `ai.embed` through its embeddings endpoint at the
//!   dimensions `ai:` asks for; a bearer credential.
//! - **Anthropic** answers `ai.generate` as a message with its structured
//!   outputs in `output_config.format`, the answer its text blocks'
//!   (thinking blocks have none), and `ai.stream` as its named events,
//!   ended by `message_stop`; its key in `x-api-key` and its version in
//!   `anthropic-version`. It maps no `embed:`.
//! - **Recordings.** Each package's recordings are the providers'
//!   documented answers kept as `jadpo package record` keeps one, and they
//!   decode, as the app's own integration's are decoded by `jadpo test`.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

use crate::http::{Dev, wait_for};
use crate::support::fake_provider::FakeProvider;

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("ai-packages-5.5v7-{name}"));
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

/// A first-party package as this repository holds it.
fn package(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages")
        .join(name)
}

/// One of the two packages, as an app connects it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Provider {
    OpenAi,
    Anthropic,
}

impl Provider {
    fn folder(self) -> &'static str {
        match self {
            Provider::OpenAi => "openai",
            Provider::Anthropic => "anthropic",
        }
    }
}

/// The support desk asking its model through `provider`'s package,
/// installed with `jadpo package add`, its address `.env.local`'s.
fn desk(name: &str, provider: Provider) -> PathBuf {
    let root = folder(name);
    let (connection, package, model, embedding) = match provider {
        Provider::OpenAi => (
            "openai",
            "OpenAi.v1",
            "openai/gpt-6-luna",
            Some("openai/text-embedding-3-small"),
        ),
        Provider::Anthropic => (
            "anthropic",
            "Anthropic.v1",
            "anthropic/claude-haiku-5-5",
            None,
        ),
    };
    let sandbox = if embedding.is_some() {
        "[answer_question, embed]"
    } else {
        "[answer_question]"
    };
    let models = match embedding {
        Some(_) => {
            "            default: config.drafting_model\n            embedding: config.search_model\n        }\n        embedding_dimensions: 3\n"
        }
        None => "            default: config.drafting_model\n        }\n",
    };
    write(
        &root,
        "app.jadpo",
        &format!(
            "application {{\n    connections: {{\n        {connection}: {{\n            package: {package}\n            domains: [support]\n            credential: config.provider_key\n            host: config.provider_host\n            development: {{\n                sandbox: {sandbox}\n            }}\n        }}\n    }}\n    ai: {{\n        connection: {connection}\n        models: {{\n{models}    }}\n}}\n"
        ),
    );
    let search = embedding.map_or_else(String::new, |model| {
        format!("    search_model: ModelId {{\n        default: \"{model}\"\n    }}\n")
    });
    write(
        &root,
        "config.jadpo",
        &format!(
            "config {{\n    drafting_model: ModelId {{\n        default: \"{model}\"\n    }}\n    provider_host: Url {{\n        default: \"https://provider.invalid\"\n    }}\n    provider_key: Text {{\n        secret: true\n    }}\n{search}}}\n"
        ),
    );
    let embeds = embedding.is_some();
    write(
        &root,
        "policy.jadpo",
        &format!(
            "use support\n\npolicy {{\n    actions: {{\n        support.answer_customer: [Access.public]\n        support.stream_answer: [Access.public]\n{}    }}\n}}\n",
            if embeds {
                "        support.meaning_of: [Access.public]\n"
            } else {
                ""
            }
        ),
    );
    for (path, text) in SUPPORT {
        write(&root, &format!("domains/support/{path}"), text);
    }
    if embeds {
        for (path, text) in EMBEDS {
            write(&root, &format!("domains/support/{path}"), text);
        }
    }
    let added = jadpo(
        &root,
        &[
            "package",
            "add",
            package_path(provider).to_str().expect("a path"),
        ],
    );
    assert_eq!(added.code, 0, "{}{}", added.stdout, added.stderr);
    let updated = jadpo(&root, &["api", "update"]);
    assert_eq!(updated.code, 0, "{}{}", updated.stdout, updated.stderr);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    root
}

fn package_path(provider: Provider) -> PathBuf {
    package(provider.folder())
}

/// The desk's domain: a question, a typed answer, a prompt, and the
/// actions and routes that generate and stream it.
const SUPPORT: &[(&str, &str)] = &[
    (
        "types/customer_question.jadpo",
        "/// What a customer asked.\ntype CustomerQuestion = Text {\n    max_length: 40000\n}\n",
    ),
    (
        "types/asked.jadpo",
        "use support/types { CustomerQuestion }\n\n/// A customer's question, as the desk sends it.\ntype Asked = Input {\n    question: CustomerQuestion\n}\n",
    ),
    (
        "types/support_answer.jadpo",
        "/// What the assistant says back.\ntype SupportAnswer = Output {\n    /// False when the help articles don't answer the question.\n    resolved_by_articles: Bool\n    /// The reply to show the customer.\n    reply: Text {\n        max_length: 400\n    }\n}\n",
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
        "actions/answer_customer.jadpo",
        "use support/failures { AssistantUnavailable }\nuse support/prompts { AnswerQuestion }\nuse support/types { Asked, SupportAnswer }\n\n/// Answers a customer's question.\naction answer_customer(asked: Asked) fails AssistantUnavailable -> SupportAnswer {\n    return attempt ai.generate(\n        AnswerQuestion {\n            question: asked.question\n        },\n        as: SupportAnswer,\n        max_output: 300,\n        timeout: 15s\n    ) {\n        refused: AssistantUnavailable\n        malformed: AssistantUnavailable\n        timed_out: AssistantUnavailable\n        over_budget: AssistantUnavailable\n        too_long: AssistantUnavailable\n    }\n}\n",
    ),
    (
        "actions/stream_answer.jadpo",
        "use support/failures { AssistantUnavailable }\nuse support/prompts { AnswerQuestion }\nuse support/types { Asked }\n\n/// Streams an answer to a customer's question as it is written.\naction stream_answer(asked: Asked) fails AssistantUnavailable -> AiStream {\n    return attempt ai.stream(\n        AnswerQuestion {\n            question: asked.question\n        },\n        max_output: 300,\n        timeout: 8s\n    ) {\n        refused: AssistantUnavailable\n        timed_out: AssistantUnavailable\n        over_budget: AssistantUnavailable\n        too_long: AssistantUnavailable\n    }\n}\n",
    ),
    (
        "routes/answers.jadpo",
        "use support/actions { answer_customer, stream_answer }\nuse support/types { Asked, SupportAnswer }\n\n/// Customers' questions.\nroutes Answers {\n    prefix: /answers\n\n    /// Anyone asks one.\n    POST / {\n        authentication: RouteAuthentication.none\n        input: Asked\n        output: SupportAnswer\n        deadline: 30s\n        run: answer_customer(input)\n    }\n\n    /// The reply, as it is written.\n    POST /streamed {\n        authentication: RouteAuthentication.none\n        input: Asked\n        output: AiStream\n        run: stream_answer(input)\n    }\n}\n",
    ),
];

/// The desk's embeddings, for a package that maps `embed:`.
const EMBEDS: &[(&str, &str)] = &[
    (
        "types/question_meaning.jadpo",
        "/// A question's meaning, as the embedding model makes it.\ntype QuestionMeaning = Vector {\n    dimensions: 3\n}\n",
    ),
    (
        "types/meaning.jadpo",
        "use support/types { QuestionMeaning }\n\n/// A question's meaning, sent back.\ntype Meaning = Output {\n    meaning: QuestionMeaning\n}\n",
    ),
    (
        "actions/meaning_of.jadpo",
        "use support/failures { AssistantUnavailable }\nuse support/types { Asked, Meaning, QuestionMeaning }\n\n/// A question's meaning.\naction meaning_of(asked: Asked) fails AssistantUnavailable -> Meaning {\n    var meaning: QuestionMeaning = attempt ai.embed(asked.question, as: QuestionMeaning) {\n        timed_out: AssistantUnavailable\n        over_budget: AssistantUnavailable\n        too_long: AssistantUnavailable\n    }\n    return Meaning {\n        meaning: meaning\n    }\n}\n",
    ),
    (
        "routes/meanings.jadpo",
        "use support/actions { meaning_of }\nuse support/types { Asked, Meaning }\n\n/// Questions' meanings.\nroutes Meanings {\n    prefix: /meanings\n\n    /// Anyone asks one.\n    POST / {\n        authentication: RouteAuthentication.none\n        input: Asked\n        output: Meaning\n        run: meaning_of(input)\n    }\n}\n",
    ),
];

fn ask(dev: &Dev, path: &str, question: &str) -> crate::http::Reply {
    dev.send(
        "POST",
        path,
        &[],
        &serde_json::json!({ "question": question }).to_string(),
    )
}

/// `jadpo dev` with the provider's sandbox at `provider`, its key a test
/// value.
fn dev(root: &Path, provider: &FakeProvider) -> Dev {
    write(
        root,
        ".env.local",
        &format!(
            "PROVIDER_KEY=test_key_5_5v7\nPROVIDER_HOST={}\n",
            provider.url
        ),
    );
    let dev = Dev::start_without_database(root);
    wait_for(&dev, "\"listening\"");
    dev
}

/// The pieces a streamed reply's `piece` events carry, and the event it
/// ends with.
fn pieces(body: &str) -> (Vec<String>, String) {
    let mut pieces = Vec::new();
    let mut last = String::new();
    for event in body.split("\n\n") {
        let Some(name) = event
            .lines()
            .find_map(|line| line.strip_prefix("event: "))
            .map(str::to_string)
        else {
            continue;
        };
        if name == "piece" {
            let data = event
                .lines()
                .find_map(|line| line.strip_prefix("data: "))
                .unwrap_or_default();
            let data: serde_json::Value = serde_json::from_str(data).expect("JSON");
            pieces.push(data["text"].as_str().unwrap_or_default().to_string());
        }
        last = name;
    }
    (pieces, last)
}

/// A Chat Completion as OpenAI documents one, its text `content` or its
/// `refusal`, stopped for `finish`.
fn chat_completion(content: Option<&str>, refusal: Option<&str>, finish: &str) -> String {
    serde_json::json!({
        "id": "chatcmpl-B9MBs8CjcvOU2jLn4n570S5qMJKcT",
        "object": "chat.completion",
        "created": 1741569952,
        "model": "gpt-6-luna",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": content, "refusal": refusal, "annotations": []},
            "logprobs": null,
            "finish_reason": finish
        }],
        "usage": {
            "prompt_tokens": 19,
            "completion_tokens": 10,
            "total_tokens": 29,
            "prompt_tokens_details": {"cached_tokens": 0, "audio_tokens": 0}
        },
        "service_tier": "default"
    })
    .to_string()
}

/// A streamed Chat Completion as OpenAI documents one: a chunk for each
/// piece, the last with its finish, then `data: [DONE]`.
fn chat_stream(pieces: &[&str], finish: &str) -> String {
    let chunk = |delta: serde_json::Value, finish: Option<&str>| {
        let chunk = serde_json::json!({
            "id": "chatcmpl-123",
            "object": "chat.completion.chunk",
            "created": 1694268190,
            "model": "gpt-6-luna",
            "system_fingerprint": "fp_44709d6fcb",
            "choices": [{"index": 0, "delta": delta, "logprobs": null, "finish_reason": finish}]
        });
        format!("data: {chunk}\n\n")
    };
    let mut body = chunk(
        serde_json::json!({"role": "assistant", "content": ""}),
        None,
    );
    for piece in pieces {
        body.push_str(&chunk(serde_json::json!({"content": piece}), None));
    }
    body.push_str(&chunk(serde_json::json!({}), Some(finish)));
    body.push_str("data: [DONE]\n\n");
    body
}

/// A Message as Anthropic documents one: a thinking block with its text
/// omitted, then the answer's text block, stopped for `stop`.
fn message(text: &str, stop: &str) -> String {
    serde_json::json!({
        "id": "msg_013Zva2CMHLNnXjNJJKqJ2EF",
        "type": "message",
        "role": "assistant",
        "model": "claude-haiku-5-5",
        "content": [
            {"type": "thinking", "thinking": "", "signature": "EqQBCgIYAhIM1gbcDa9GJwZA2b3hGgxBdjrkzLoky3dl1pkiMOYds"},
            {"type": "text", "text": text}
        ],
        "stop_reason": stop,
        "stop_sequence": null,
        "stop_details": null,
        "usage": {
            "input_tokens": 2095,
            "output_tokens": 503,
            "cache_creation_input_tokens": 0,
            "cache_read_input_tokens": 0
        }
    })
    .to_string()
}

/// A streamed Message as Anthropic documents one: named events, the
/// thinking block's with no text, a `text_delta` for each piece, then
/// `message_delta` with its stop reason and `message_stop`.
fn message_stream(pieces: &[&str], stop: &str) -> String {
    let event = |name: &str, data: serde_json::Value| format!("event: {name}\ndata: {data}\n\n");
    let mut body = event(
        "message_start",
        serde_json::json!({"type": "message_start", "message": {"id": "msg_1", "type": "message", "role": "assistant", "content": [], "model": "claude-haiku-5-5", "stop_reason": null, "stop_sequence": null, "usage": {"input_tokens": 25, "output_tokens": 1}}}),
    );
    body.push_str(&event(
        "content_block_start",
        serde_json::json!({"type": "content_block_start", "index": 0, "content_block": {"type": "thinking", "thinking": ""}}),
    ));
    body.push_str(&event(
        "content_block_delta",
        serde_json::json!({"type": "content_block_delta", "index": 0, "delta": {"type": "thinking_delta", "thinking": ""}}),
    ));
    body.push_str(&event(
        "content_block_stop",
        serde_json::json!({"type": "content_block_stop", "index": 0}),
    ));
    body.push_str(&event("ping", serde_json::json!({"type": "ping"})));
    body.push_str(&event(
        "content_block_start",
        serde_json::json!({"type": "content_block_start", "index": 1, "content_block": {"type": "text", "text": ""}}),
    ));
    for piece in pieces {
        body.push_str(&event(
            "content_block_delta",
            serde_json::json!({"type": "content_block_delta", "index": 1, "delta": {"type": "text_delta", "text": piece}}),
        ));
    }
    body.push_str(&event(
        "content_block_stop",
        serde_json::json!({"type": "content_block_stop", "index": 1}),
    ));
    body.push_str(&event(
        "message_delta",
        serde_json::json!({"type": "message_delta", "delta": {"stop_reason": stop, "stop_sequence": null}, "usage": {"output_tokens": 15}}),
    ));
    if stop == "end_turn" {
        body.push_str(&event(
            "message_stop",
            serde_json::json!({"type": "message_stop"}),
        ));
    }
    body
}

/// OpenAI through its package: the Chat Completions request made from its
/// mapping, with a bearer credential and the strict structured mode; the
/// answer, a refusal and a cut-off read as its mapping says; a streamed
/// answer piece by piece; and an embedding at the dimensions `ai:` asks.
#[test]
fn openai_generates_streams_and_embeds_through_its_package() {
    let provider = FakeProvider::start();
    let root = desk("openai", Provider::OpenAi);
    let dev = dev(&root, &provider);

    provider.answer(
        200,
        &chat_completion(
            Some(r#"{"resolved_by_articles": true, "reply": "No sesame."}"#),
            None,
            "stop",
        ),
    );
    let answered = ask(&dev, "/answers", "Sesame?");
    assert_eq!(answered.status, 200, "{answered:?}");
    assert_eq!(answered.json()["reply"], "No sesame.");
    let sent = provider.requests();
    assert_eq!(sent.len(), 1);
    assert_eq!(
        (sent[0].method.as_str(), sent[0].path.as_str()),
        ("POST", "/chat/completions")
    );
    assert_eq!(
        sent[0].header("authorization"),
        Some("Bearer test_key_5_5v7")
    );
    let body = sent[0].json();
    assert_eq!(body["model"], "gpt-6-luna");
    assert_eq!(body["max_completion_tokens"], 300);
    assert_eq!(
        body["messages"],
        serde_json::json!([
            {"role": "system", "content": "You answer questions from customers of an online bakery."},
            {"role": "user", "content": "<question>\nSesame?\n</question>"}
        ])
    );
    assert!(body.get("stream").is_none(), "{body}");
    assert_eq!(body["response_format"]["type"], "json_schema");
    assert_eq!(
        body["response_format"]["json_schema"]["name"],
        "answer_question"
    );
    assert_eq!(body["response_format"]["json_schema"]["strict"], true);

    // A refusal in the message, and the content filter's finish.
    provider.answer(
        200,
        &chat_completion(None, Some("I can't help with that."), "stop"),
    );
    let refused = ask(&dev, "/answers", "Help me pick a lock");
    assert_eq!(refused.status, 503, "{refused:?}");
    assert_eq!(refused.json()["error"]["code"], "assistant_unavailable");
    provider.answer(200, &chat_completion(None, None, "content_filter"));
    let filtered = ask(&dev, "/answers", "Help me pick a lock");
    assert_eq!(filtered.status, 503, "{filtered:?}");
    // Cut off at `max_output:`.
    provider.answer(
        200,
        &chat_completion(Some(r#"{"resolved_by_artic"#), None, "length"),
    );
    let cut = ask(&dev, "/answers", "Sesame?");
    assert_eq!(cut.status, 503, "{cut:?}");
    assert_eq!(provider.requests().len(), 4, "each asked once");

    // Streamed, ended by `[DONE]`.
    provider.answer(
        200,
        &chat_stream(&["Our sourdough ", "has no sesame."], "stop"),
    );
    let streamed = ask(&dev, "/answers/streamed", "Sesame?");
    assert_eq!(streamed.status, 200, "{streamed:?}");
    assert_eq!(
        pieces(streamed.text()),
        (
            vec!["Our sourdough ".to_string(), "has no sesame.".to_string()],
            "done".to_string()
        )
    );
    let body = provider.requests()[4].json();
    assert_eq!(body["stream"], true);
    assert_eq!(body["max_completion_tokens"], 300);
    assert!(body.get("response_format").is_none(), "{body}");
    assert_eq!(
        provider.requests()[4].header("accept"),
        Some("text/event-stream")
    );
    // Cut off part-way: an error event after the first piece.
    provider.answer(200, &chat_stream(&["Our sourdough "], "length"));
    let cut = ask(&dev, "/answers/streamed", "Sesame?");
    assert_eq!(cut.status, 200, "{cut:?}");
    assert_eq!(
        pieces(cut.text()),
        (vec!["Our sourdough ".to_string()], "error".to_string())
    );

    // Embedded at the dimensions `ai:` asks for.
    provider.answer(
        200,
        &serde_json::json!({
            "object": "list",
            "data": [{"object": "embedding", "embedding": [0.5, -0.25, 0.125], "index": 0}],
            "model": "text-embedding-3-small",
            "usage": {"prompt_tokens": 2, "total_tokens": 2}
        })
        .to_string(),
    );
    let meaning = ask(&dev, "/meanings", "Sesame?");
    assert_eq!(meaning.status, 200, "{meaning:?}");
    assert_eq!(
        meaning.json()["meaning"],
        serde_json::json!([0.5, -0.25, 0.125])
    );
    let sent = provider.requests();
    assert_eq!(sent[6].path, "/embeddings");
    assert_eq!(
        sent[6].json(),
        serde_json::json!({"model": "text-embedding-3-small", "input": ["Sesame?"], "dimensions": 3})
    );
    // A vector of another length isn't the model `ai:` names.
    provider.answer(
        200,
        &serde_json::json!({
            "object": "list",
            "data": [{"object": "embedding", "embedding": [0.6, 0.8], "index": 0}],
            "model": "text-embedding-3-small",
            "usage": {"prompt_tokens": 2, "total_tokens": 2}
        })
        .to_string(),
    );
    let wrong = ask(&dev, "/meanings", "Rye?");
    assert_eq!(wrong.status, 500, "{wrong:?}");
    assert_eq!(wrong.json()["error"]["code"], "misconfigured");

    // Longer than the embedding model takes: refused before sending.
    let long = ask(&dev, "/meanings", &"Sesame? ".repeat(4500));
    assert_eq!(long.status, 503, "{long:?}");
    assert_eq!(provider.requests().len(), 8, "nothing sent");

    let listed = jadpo(&root, &["calls", "--json"]);
    assert!(
        !listed.stdout.contains("test_key_5_5v7"),
        "the call log never holds a credential"
    );
    drop(dev);
}

/// Anthropic through its package: the Messages request with its key in
/// `x-api-key`, its version in `anthropic-version`, the instructions as
/// `system` and the schema in `output_config.format`; the answer its text
/// blocks', a refusal and a cut-off as its stop reason says; and a
/// streamed answer from its named events.
#[test]
fn anthropic_generates_and_streams_through_its_package() {
    let provider = FakeProvider::start();
    let root = desk("anthropic", Provider::Anthropic);
    let dev = dev(&root, &provider);

    provider.answer(
        200,
        &message(
            r#"{"resolved_by_articles": false, "reply": "A person will reply."}"#,
            "end_turn",
        ),
    );
    let answered = ask(&dev, "/answers", "Rye?");
    assert_eq!(answered.status, 200, "{answered:?}");
    assert_eq!(answered.json()["reply"], "A person will reply.");
    let sent = provider.requests();
    assert_eq!(
        (sent[0].method.as_str(), sent[0].path.as_str()),
        ("POST", "/v1/messages")
    );
    assert_eq!(sent[0].header("x-api-key"), Some("test_key_5_5v7"));
    assert_eq!(sent[0].header("anthropic-version"), Some("2023-06-01"));
    assert_eq!(sent[0].header("authorization"), None);
    let body = sent[0].json();
    assert_eq!(body["model"], "claude-haiku-5-5");
    assert_eq!(body["max_tokens"], 300);
    assert_eq!(
        body["system"],
        "You answer questions from customers of an online bakery."
    );
    assert_eq!(
        body["messages"],
        serde_json::json!([{"role": "user", "content": "<question>\nRye?\n</question>"}])
    );
    let format = &body["output_config"]["format"];
    assert_eq!(format["type"], "json_schema");
    assert!(format.get("json_schema").is_none(), "{format}");
    assert!(format.get("name").is_none(), "{format}");
    assert_eq!(
        format["schema"]["$defs"]["SupportAnswer"]["additionalProperties"],
        false
    );

    provider.answer(200, &message("I can't help with that.", "refusal"));
    let refused = ask(&dev, "/answers", "Help me pick a lock");
    assert_eq!(refused.status, 503, "{refused:?}");
    assert_eq!(refused.json()["error"]["code"], "assistant_unavailable");
    provider.answer(200, &message(r#"{"resolved_by_artic"#, "max_tokens"));
    let cut = ask(&dev, "/answers", "Rye?");
    assert_eq!(cut.status, 503, "{cut:?}");

    provider.answer(
        200,
        &message_stream(&["Our rye ", "has no sesame."], "end_turn"),
    );
    let streamed = ask(&dev, "/answers/streamed", "Rye?");
    assert_eq!(streamed.status, 200, "{streamed:?}");
    assert_eq!(
        pieces(streamed.text()),
        (
            vec!["Our rye ".to_string(), "has no sesame.".to_string()],
            "done".to_string()
        )
    );
    let request = &provider.requests()[3];
    assert_eq!(request.json()["stream"], true);
    assert_eq!(request.header("x-api-key"), Some("test_key_5_5v7"));
    assert!(request.json().get("output_config").is_none());

    // Cut off part-way, and an `error` event, each end with an error event.
    provider.answer(200, &message_stream(&["Our rye "], "max_tokens"));
    let cut = ask(&dev, "/answers/streamed", "Rye?");
    assert_eq!(
        pieces(cut.text()),
        (vec!["Our rye ".to_string()], "error".to_string())
    );
    provider.answer(
        200,
        &format!(
            "{}event: error\ndata: {}\n\n",
            message_stream(&["Our rye "], "end_turn")
                .rsplitn(2, "event: content_block_stop")
                .last()
                .unwrap_or_default(),
            serde_json::json!({"type": "error", "error": {"type": "overloaded_error", "message": "Overloaded"}})
        ),
    );
    let broken = ask(&dev, "/answers/streamed", "Rye?");
    assert_eq!(
        pieces(broken.text()),
        (vec!["Our rye ".to_string()], "error".to_string())
    );
    drop(dev);
}

/// Anthropic has no embeddings endpoint: an app embedding through its
/// connection is told so where it asks.
#[test]
fn anthropic_doesnt_embed() {
    let root = desk("anthropic-embed", Provider::Anthropic);
    write(
        &root,
        "domains/support/types/question_meaning.jadpo",
        "/// A question's meaning.\ntype QuestionMeaning = Vector {\n    dimensions: 3\n}\n",
    );
    write(
        &root,
        "domains/support/actions/meaning_of.jadpo",
        "use support/failures { AssistantUnavailable }\nuse support/types { CustomerQuestion, QuestionMeaning }\n\n/// A question's meaning.\naction meaning_of(question: CustomerQuestion) fails AssistantUnavailable -> QuestionMeaning {\n    return attempt ai.embed(question, as: QuestionMeaning) {\n        timed_out: AssistantUnavailable\n        over_budget: AssistantUnavailable\n        too_long: AssistantUnavailable\n    }\n}\n",
    );
    let checked = jadpo(&root, &["check"]);
    assert_eq!(
        checked.code, 1,
        "{}: {}{}",
        checked.code, checked.stdout, checked.stderr
    );
    let said = format!("{}{}", checked.stdout, checked.stderr);
    assert!(said.contains("error: `anthropic` doesn't embed"), "{said}");
}

/// Each package's recordings are its provider's documented answers, kept
/// as `jadpo package record` keeps one: recorded again here from the same
/// documented answers, through a connection to the package held as the
/// app's own integration, against a server started in the test, they are
/// the files the package holds, and `jadpo test` decodes them.
#[test]
fn each_packages_recordings_are_its_documented_answers() {
    let chat = serde_json::json!({
        "model": "gpt-6-luna",
        "messages": [{"role": "user", "content": "Hello!"}],
        "max_completion_tokens": 300
    });
    let streamed_chat = serde_json::json!({
        "model": "gpt-6-luna",
        "messages": [{"role": "user", "content": "Hello!"}],
        "max_completion_tokens": 300,
        "stream": true
    });
    let embeddings = serde_json::json!({
        "model": "text-embedding-3-small",
        "input": ["Hello world"],
        "dimensions": 3
    });
    let messages = serde_json::json!({
        "model": "claude-haiku-5-5",
        "max_tokens": 1024,
        "system": "You answer questions.",
        "messages": [{"role": "user", "content": "Hello, Claude"}]
    });
    let streamed_messages = serde_json::json!({
        "model": "claude-haiku-5-5",
        "max_tokens": 1024,
        "system": "You answer questions.",
        "messages": [{"role": "user", "content": "Hello, Claude"}],
        "stream": true
    });
    let chunk = serde_json::json!({
        "id": "chatcmpl-123",
        "object": "chat.completion.chunk",
        "created": 1694268190,
        "model": "gpt-6-luna",
        "system_fingerprint": "fp_44709d6fcb",
        "choices": [{"index": 0, "delta": {"content": "Hello"}, "logprobs": null, "finish_reason": null}]
    });
    let embedded = serde_json::json!({
        "object": "list",
        "data": [{"object": "embedding", "embedding": [0.26726124, 0.53452248, 0.80178373], "index": 0}],
        "model": "text-embedding-3-small",
        "usage": {"prompt_tokens": 2, "total_tokens": 2}
    });
    let delta = serde_json::json!({
        "type": "content_block_delta",
        "index": 0,
        "delta": {"type": "text_delta", "text": "ello frien"}
    });
    let cases: [(Provider, &str, &str, Vec<Recorded>); 2] = [
        (
            Provider::OpenAi,
            "OpenAi.v1",
            "open_ai",
            vec![
                (
                    "chat_completions.create",
                    chat,
                    chat_completion(Some("Hello! How can I assist you today?"), None, "stop"),
                ),
                ("chat_completions.stream", streamed_chat, chunk.to_string()),
                ("embeddings.create", embeddings, embedded.to_string()),
            ],
        ),
        (
            Provider::Anthropic,
            "Anthropic.v1",
            "anthropic",
            vec![
                (
                    "messages.create",
                    messages,
                    message("Hi! My name is Claude.", "end_turn"),
                ),
                ("messages.stream", streamed_messages, delta.to_string()),
            ],
        ),
    ];
    let mut differ: Vec<String> = Vec::new();
    for (provider, package, folder, recordings) in cases {
        let root = self::folder(&format!("recordings-{}", provider.folder()));
        write(
            &root,
            "app.jadpo",
            &format!(
                "application {{\n    connections: {{\n        provider: {{\n            package: {package}\n            domains: [support]\n            credential: config.provider_key\n            host: config.provider_host\n        }}\n    }}\n}}\n"
            ),
        );
        write(
            &root,
            "config.jadpo",
            "config {\n    provider_host: Url {\n        default: \"https://provider.invalid\"\n    }\n    provider_key: Text {\n        secret: true\n    }\n}\n",
        );
        write(
            &root,
            "domains/support/types/note.jadpo",
            "/// A note.\ntype Note = Text {\n    max_length: 10\n}\n",
        );
        write(
            &root,
            "domains/support/actions/greet.jadpo",
            "use support/types { Note }\n\n/// A note.\naction greet() -> Note {\n    return \"Hi\"\n}\n",
        );
        write(
            &root,
            "policy.jadpo",
            "use support\n\npolicy {\n    actions: {\n        support.greet: [Access.public]\n    }\n}\n",
        );
        let source = package_path(provider);
        let own = root.join(format!("shared/providers/{folder}/v1"));
        copy(&source, &own);
        let _ = fs::remove_dir_all(own.join("recordings"));
        let server = FakeProvider::start();
        write(
            &root,
            ".env.local",
            &format!(
                "PROVIDER_KEY=test_key_5_5v7\nPROVIDER_HOST={}\n",
                server.url
            ),
        );
        for (operation, input, answer) in &recordings {
            server.answer(200, answer);
            let recorded = jadpo(
                &root,
                &[
                    "package",
                    "record",
                    "provider",
                    operation,
                    "--input",
                    &input.to_string(),
                ],
            );
            assert_eq!(
                recorded.code, 0,
                "{operation}: {}{}",
                recorded.stdout, recorded.stderr
            );
            let file = format!("recordings/{operation}.json");
            let kept = fs::read_to_string(own.join(&file)).expect("recorded");
            let committed = fs::read_to_string(source.join(&file)).unwrap_or_default();
            if kept != committed {
                differ.push(format!("{}/{file}:\n{kept}", provider.folder()));
            }
        }
        if !differ.is_empty() {
            continue;
        }
        let tested = jadpo(&root, &["test"]);
        assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
        let line = format!("{} recording test", recordings.len());
        assert!(
            tested.stdout.contains(&line),
            "{line}: {}{}",
            tested.stdout,
            tested.stderr
        );
    }
    assert!(
        differ.is_empty(),
        "each recording is the documented answer kept:\n{}",
        differ.join("\n")
    );
}

/// An operation recorded, the input it is sent and the documented answer.
type Recorded = (&'static str, serde_json::Value, String);

/// Copies the folder `from` to `to`, its files and folders.
fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("folder");
    for entry in fs::read_dir(from).expect("a folder") {
        let entry = entry.expect("an entry");
        let path = entry.path();
        let target = to.join(entry.file_name());
        if path.is_dir() {
            copy(&path, &target);
        } else {
            fs::copy(&path, &target).expect("copied");
        }
    }
}
