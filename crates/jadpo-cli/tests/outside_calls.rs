//! One outside-call path (chunk 7.9d, from the Phase 7 architecture
//! review): every kind of call that leaves the process goes out through
//! one exchange and one retrying send, so each holds to what a call
//! through its connection holds to.
//!
//! - **`max_concurrent:` for every call kind.** An `ai.generate` through a
//!   package's connection takes one of the connection's slots, as a
//!   provider operation's call does, and waits while they are all out
//!   (area 11, "Calling").
//! - **Never waiting past the time left.** A provider's `Retry-After`
//!   longer than what is left of the route's deadline ends the sending at
//!   once with its last answer, `unavailable`, rather than waiting until
//!   the deadline passes; for a provider operation and for `ai.generate`
//!   alike (area 11, "What is sent again"; area 21, "Reading an answer").
//!
//! Each test compiles one project, a support desk whose `ai` a provider
//! package fills, and answers its routes on a harness whose timers never
//! finish by themselves, through fakes in the package's place. Nothing
//! reaches a network.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll, Wake, Waker};

use jadpo_interp::{Prepared, Program};
use jadpo_runtime::ai::AiFake;
use jadpo_runtime::capability::{Answer, Capability, Fake, Faked, Scripted};
use jadpo_runtime::http::{Request, Response, Worker, serve_request};
use jadpo_runtime::{
    Database, Executor, Host, Isolation, LocalFuture, Outcome, Row, SqlValue, StoreError,
    Transaction,
};
use jadpo_test::host::Harness;

const PACKAGE: &str = "shared/providers/acme_models/v1";

/// The desk's files: a package filling `ai`, its connection given
/// `max_concurrent: 1`; a route asking the model, and one calling the
/// package's chat operation itself, each with the default deadline.
fn files() -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = vec![
        (
            "app.jadpo".into(),
            "application {\n    connections: {\n        acme: {\n            package: AcmeModels.v1\n            domains: [support]\n            credential: config.acme_key\n            environment: config.acme_environment\n            max_concurrent: 1\n        }\n    }\n    ai: {\n        connection: acme\n        models: {\n            default: config.drafting_model\n        }\n        limits: {\n            per_day: {\n                tokens: 2000000\n            }\n        }\n    }\n}\n".into(),
        ),
        (
            "config.jadpo".into(),
            "config {\n    acme_key: Text {\n        secret: true\n    }\n    acme_environment: ProviderEnvironment {\n        default: ProviderEnvironment.sandbox\n    }\n    drafting_model: ModelId {\n        default: \"acme/acme-small\"\n    }\n}\n".into(),
        ),
        (
            "policy.jadpo".into(),
            "use support\n\npolicy {\n    actions: {\n        support.answer_customer: [Access.public]\n        support.chat_directly: [Access.public]\n        support.answer: [Access.public]\n    }\n}\n".into(),
        ),
        (
            "domains/support/types/customer_question.jadpo".into(),
            "/// What a customer asked.\ntype CustomerQuestion = Text {\n    max_length: 2000\n}\n".into(),
        ),
        (
            "domains/support/types/asked.jadpo".into(),
            "use support/types { CustomerQuestion }\n\n/// A customer's question, as the desk sends it.\ntype Asked = Input {\n    question: CustomerQuestion\n}\n".into(),
        ),
        (
            "domains/support/types/support_answer.jadpo".into(),
            "/// What the assistant says back.\ntype SupportAnswer = Output {\n    /// False when the help articles don't answer the question.\n    resolved_by_articles: Bool\n    /// The reply to show the customer.\n    reply: Text {\n        max_length: 40\n    }\n}\n".into(),
        ),
        (
            "domains/support/failures/assistant_unavailable.jadpo".into(),
            "/// The assistant can't answer now.\ntype AssistantUnavailable = Failure {\n    kind: FailureKind.unavailable\n    message: \"The assistant can't answer now.\"\n}\n".into(),
        ),
        (
            "domains/support/prompts/answer_question.jadpo".into(),
            "use support/types { CustomerQuestion }\n\n/// Answers a customer's question.\ntype AnswerQuestion = Prompt {\n    question: CustomerQuestion\n}\n".into(),
        ),
        (
            "domains/support/prompts/answer_question.prompt".into(),
            "# Instructions\n\nYou answer questions from customers of an online bakery.\n\n# Request\n\n{question}\n".into(),
        ),
        (
            "domains/support/actions/answer_customer.jadpo".into(),
            "use support/failures { AssistantUnavailable }\nuse support/prompts { AnswerQuestion }\nuse support/types { Asked, SupportAnswer }\n\n/// Answers a customer's question.\naction answer_customer(asked: Asked) fails AssistantUnavailable -> SupportAnswer {\n    return attempt ai.generate(\n        AnswerQuestion {\n            question: asked.question\n        },\n        as: SupportAnswer,\n        max_output: 300\n    ) {\n        refused: AssistantUnavailable\n        malformed: AssistantUnavailable\n        timed_out: AssistantUnavailable\n        over_budget: AssistantUnavailable\n        too_long: AssistantUnavailable\n    }\n}\n".into(),
        ),
        (
            "domains/support/actions/answer.jadpo".into(),
            "use support/failures { AssistantUnavailable }\nuse support/prompts { AnswerQuestion }\nuse support/types { CustomerQuestion, SupportAnswer }\n\n/// Answers a question, as background work does.\naction answer(question: CustomerQuestion) fails AssistantUnavailable -> SupportAnswer {\n    return attempt ai.generate(\n        AnswerQuestion {\n            question: question\n        },\n        as: SupportAnswer,\n        max_output: 300\n    ) {\n        refused: AssistantUnavailable\n        malformed: AssistantUnavailable\n        timed_out: AssistantUnavailable\n        over_budget: AssistantUnavailable\n        too_long: AssistantUnavailable\n    }\n}\n".into(),
        ),
        (
            "domains/support/actions/chat_directly.jadpo".into(),
            "use app { acme }\nuse shared/providers/acme_models\nuse support/types { Asked, SupportAnswer }\n\n/// Asks the package's chat operation itself.\naction chat_directly(asked: Asked) -> SupportAnswer {\n    var answered: AcmeModels.ChatResponse = acme.chat.complete(AcmeModels.ChatRequest {\n        model: \"acme-small\"\n        instructions: \"Answer the customer.\"\n        messages: [\n            AcmeModels.ChatTurn {\n                role: \"user\"\n                content: \"Is the rye sourdough?\"\n            }\n        ]\n        max_tokens: 50\n    })\n    return SupportAnswer {\n        resolved_by_articles: list.count(answered.choices) > 0\n        reply: \"Asked.\"\n    }\n}\n".into(),
        ),
        (
            "domains/support/routes/answers.jadpo".into(),
            "use support/actions { answer_customer, chat_directly }\nuse support/types { Asked, SupportAnswer }\n\n/// Customers' questions.\nroutes Answers {\n    prefix: /answers\n\n    /// Anyone asks one.\n    POST / {\n        authentication: RouteAuthentication.none\n        input: Asked\n        output: SupportAnswer\n        deadline: 60s\n        run: answer_customer(input)\n    }\n\n    /// Anyone asks the chat itself.\n    POST /direct {\n        authentication: RouteAuthentication.none\n        input: Asked\n        output: SupportAnswer\n        run: chat_directly(input)\n    }\n}\n".into(),
        ),
        (
            "package.jadpo".into(),
            "/// Acme Models' chat API.\npackage AcmeModels.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n".into(),
        ),
        (
            "provider.jadpo".into(),
            "/// Acme Models' API.\nprovider {\n    protocol: Protocol.json_http\n    hosts: {\n        live: \"https://api.acme-models.example\"\n        sandbox: \"https://sandbox.acme-models.example\"\n    }\n    credential: CredentialScheme.bearer\n    error_code: \"error.code\"\n}\n".into(),
        ),
        (
            "resources/chat.jadpo".into(),
            "public resource chat {\n    complete: {\n        method: HttpMethod.post\n        path: \"/v1/chat\"\n        input: ChatRequest\n        output: ChatResponse\n        changes: false\n        source: PackageSource.written\n        docs: \"https://docs.acme-models.example/chat\"\n    }\n}\n".into(),
        ),
        (
            "libraries/models.jadpo".into(),
            crate::ai_providers::library("0.40", "0.10", "1.60"),
        ),
        (
            "provides/ai.jadpo".into(),
            "/// Acme's chat answers `ai.generate`.\nprovide ai {\n    generate: {\n        call: chat.complete\n        input: {\n            model: request.model\n            instructions: request.instructions\n            messages: {\n                each: request.messages\n                as: {\n                    role: item.role\n                    content: item.content\n                }\n            }\n            max_tokens: request.max_output\n        }\n        answer: response.choices.first.message.content\n        refused_when: response.choices.first.message.refusal\n        cut_off_when: {\n            path: \"choices.0.finish_reason\"\n            equals: \"length\"\n        }\n        usage: {\n            input: response.usage.prompt_tokens\n            output: response.usage.completion_tokens\n        }\n        schema: {\n            mode: AiSchema.strict_subset\n            at: \"format\"\n        }\n    }\n    models: {\n        context: models.context_windows\n        prices: models.prices\n    }\n}\n".into(),
        ),
    ];
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
            "model_price",
            "/// What a model costs per million tokens, from a day.\ntype ModelPrice = Object {\n    model: Text {\n        max_length: 100\n    }\n    currency: Currency\n    input: Decimal\n    cached: Decimal\n    output: Decimal\n    from: CalendarDate\n}\n",
        ),
        (
            "model_context",
            "/// A model's context window.\ntype ModelContext = Object {\n    model: Text {\n        max_length: 100\n    }\n    tokens: TokenCount\n}\n",
        ),
    ] {
        files.push((format!("types/{file}.jadpo"), text.to_string()));
    }
    files
        .into_iter()
        .map(|(path, text)| {
            let path = if path.starts_with("domains/") || is_app(&path) {
                path
            } else {
                format!("{PACKAGE}/{path}")
            };
            (path, text)
        })
        .collect()
}

/// Whether `file`, at the top of the project, is the app's own rather
/// than the package's.
fn is_app(file: &str) -> bool {
    matches!(file, "app.jadpo" | "config.jadpo" | "policy.jadpo")
}

fn root(name: &str) -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("outside-calls-7.9d-{name}"))
}

/// The desk, checked, its settings read from the environment's names.
fn program(name: &str) -> Program {
    let root = root(name);
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in files() {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        std::fs::write(path, text).expect("write");
    }
    // The counts of what its `ai` calls use are kept in `main`.
    for args in [&["migrate", "new", "initial"][..], &["api", "update"]] {
        let done = crate::ai_providers::jadpo(&root, args);
        assert_eq!(done.code, 0, "{args:?}: {}{}", done.stdout, done.stderr);
    }
    let loaded = jadpo_db::load(&root);
    let prepared = Prepared::load(&loaded.db, loaded.project).unwrap_or_else(|problems| {
        panic!(
            "the project checks: {:#?}",
            problems
                .iter()
                .map(|p| format!(
                    "{} at {:?}",
                    p.message,
                    p.primary.as_ref().map(|label| &label.span)
                ))
                .collect::<Vec<_>>()
        )
    });
    let variables = |name: &str| -> Option<String> {
        match name {
            "ACME_KEY" => Some("ak_test_123".into()),
            _ => None,
        }
    };
    let values = prepared
        .config(
            &variables,
            jadpo_runtime::config::Mode::Environment,
            &|_: &mut [u8]| false,
        )
        .unwrap_or_else(|problems| panic!("the settings read: {problems:?}"));
    Program::new(prepared).with_config(&values)
}

/// A store that grants every reservation an `ai` call's limits ask of it
/// and begins no transaction: the desk's `main`, which keeps only the
/// counts.
struct Counting;

impl Database for Counting {
    fn begin_at(
        &self,
        _isolation: Isolation,
    ) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>> {
        Box::pin(async { Err(StoreError::Unavailable) })
    }

    fn query_once<'a>(
        &'a self,
        _sql: &'a str,
        _params: &'a [SqlValue],
    ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
        Box::pin(async { Ok(vec![vec![SqlValue::Int4(1)]]) })
    }
}

/// A harness with a clock, randomness and a monotonic clock that stands
/// still, whose timers never finish: a wait the runtime starts is never
/// over, and the route's deadline never passes by itself.
fn harness() -> Harness<Counting> {
    Harness::new(Counting)
        .at(1_792_400_400_000)
        .random_from(1)
        .monotonic(0, 0)
}

/// A customer's question, posted to `path`.
fn question(path: &str) -> Request {
    Request {
        method: "POST".to_string(),
        path: path.to_string(),
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        body: serde_json::json!({ "question": "Is the rye sourdough?" })
            .to_string()
            .into_bytes(),
        address: Some("outside calls".to_string()),
        ..Request::default()
    }
}

/// Wakes nothing; notes that it was woken.
struct Noted(AtomicBool);

impl Wake for Noted {
    fn wake(self: Arc<Self>) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// Polls `future` while it asks to be polled again: its output once it
/// has one, `None` once it waits on something that hasn't come.
fn until_stalled<F: Future>(mut future: Pin<&mut F>) -> Option<F::Output> {
    let noted = Arc::new(Noted(AtomicBool::new(false)));
    let waker = Waker::from(Arc::clone(&noted));
    let mut cx = Context::from_waker(&waker);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut cx) {
            return Some(output);
        }
        if !noted.0.swap(false, Ordering::SeqCst) {
            return None;
        }
    }
}

fn code(response: &Response) -> String {
    let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap_or_default();
    body["error"]["code"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

/// An `ai.generate` through the package's connection takes its one slot
/// under `max_concurrent: 1`: while the slot is out the model isn't asked,
/// and once it is given back the call goes and the route answers.
#[test]
fn an_ai_call_through_a_package_waits_for_its_connections_slot() {
    let program = program("slot");
    let fake = Rc::new(AiFake::new());
    let host = Faked::new(harness()).with("acme", Rc::clone(&fake) as Rc<dyn Capability>);
    let app = program.app();
    let worker = Worker::default();
    let request = question("/answers");
    let out = host
        .host()
        .slots()
        .try_take("acme", 1)
        .expect("the connection's one slot");
    let mut serving = std::pin::pin!(serve_request(&app, &program, &host, &worker, &request));
    assert!(
        until_stalled(serving.as_mut()).is_none(),
        "the call waits for the slot"
    );
    assert!(
        fake.requests("acme", "answer_question").is_empty(),
        "the model isn't asked while the slot is out"
    );
    drop(out);
    let response = until_stalled(serving.as_mut()).expect("the call goes once the slot is back");
    assert_eq!(
        response.status,
        200,
        "{}",
        String::from_utf8_lossy(&response.body)
    );
    assert_eq!(fake.requests("acme", "answer_question").len(), 1);
    assert_eq!(host.host().slots().busy("acme"), 0, "the slot given back");
}

/// A provider operation's call refused with a `Retry-After` of 20 seconds,
/// inside its `retry:`'s 30 but past what is left of the route's
/// 10-second deadline, isn't waited for: the sending ends at once with
/// its last answer, `unavailable`, and the provider is asked once.
#[test]
fn a_retry_after_past_the_routes_deadline_ends_a_provider_call() {
    let program = program("provider-retry-after");
    let fake = Rc::new(Fake::answering(Answer::Wire(Rc::from(
        r#"{"choices": [], "usage": {"prompt_tokens": 1, "completion_tokens": 0}}"#,
    ))));
    fake.script(
        "chat.complete",
        Scripted::Answer(Answer::RetryAfter(20_000)),
    );
    let host = Faked::new(harness()).with("acme", Rc::clone(&fake) as Rc<dyn Capability>);
    let app = program.app();
    let worker = Worker::default();
    let request = question("/answers/direct");
    let serving = std::pin::pin!(serve_request(&app, &program, &host, &worker, &request));
    let response = until_stalled(serving).expect("no wait past the deadline");
    assert_eq!(response.status, 503, "{}", code(&response));
    assert_eq!(code(&response), "unavailable");
    assert_eq!(fake.sent().len(), 1, "asked once");
}

/// The same for `ai.generate` run as background work with 10 seconds of
/// its budget left (what a runner gives a delivery's action as its
/// deadline): a `Retry-After` of 20 seconds, within the call's 25-second
/// `timeout:` but past the time left, ends the call as `unavailable` at
/// once.
#[test]
fn a_retry_after_past_the_time_left_ends_an_ai_call() {
    let program = program("ai-retry-after");
    let fake = Rc::new(AiFake::new());
    fake.script(
        "acme",
        "answer_question",
        Answer::RetryAfter(20_000),
        jadpo_runtime::Frame {
            callable: 0,
            site: jadpo_runtime::Frame::ENTRY,
        },
    );
    let host = Faked::new(harness()).with("acme", Rc::clone(&fake) as Rc<dyn Capability>);
    let mut ctx = host.host().ctx_on(&program, &host);
    ctx.set_deadline(Some(10_000));
    let id = program.plan().find("answer").expect("the action");
    let running = std::pin::pin!(program.run(
        &mut ctx,
        id,
        vec![jadpo_value::Value::text("Is the rye sourdough?")]
    ));
    let outcome = until_stalled(running).expect("no wait past the time left");
    assert!(
        matches!(&outcome, Outcome::Failed(failure) if failure.code == "unavailable"),
        "{outcome:?}"
    );
    assert_eq!(fake.usage()[0].called, 1, "asked once");
}
