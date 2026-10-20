//! Area 21, "How a provider fills it" (chunk 5.5v3b), in the checker: a
//! package's `provides/ai.jadpo` maps `generate` onto one of its own
//! operations as data, and `ai: { connection: }` names a connection to
//! a package that does; each mistake in the mapping is
//! `package.provider_invalid` where it is written, and `stream:` and
//! `embed:` wait for their chunks.

use jadpo_db::{Database, Layout};

use crate::support;

const PACKAGE: &str = "shared/providers/acme_models/v1";

const CONFIG: (&str, &str) = (
    "config.jadpo",
    "config {\n    acme_key: Text {\n        secret: true\n    }\n    acme_environment: ProviderEnvironment {\n        default: ProviderEnvironment.sandbox\n    }\n    drafting_model: ModelId {\n        default: \"acme/acme-small\"\n    }\n}\n",
);

const APP: &str = "application {\n    connections: {\n        acme: {\n            package: AcmeModels.v1\n            domains: [support]\n            credential: config.acme_key\n            environment: config.acme_environment\n            development: {\n                sandbox: [answer_question]\n            }\n        }\n    }\n    ai: {\n        connection: acme\n        models: {\n            default: config.drafting_model\n        }\n    }\n}\n";

/// The `generate:` mapping of the package as the tests write it.
const GENERATE: &str = "    generate: {\n        call: chat.complete\n        input: {\n            model: request.model\n            instructions: request.instructions\n            messages: {\n                each: request.messages\n                as: {\n                    role: item.role\n                    content: item.content\n                }\n            }\n            max_tokens: request.max_output\n        }\n        answer: response.choices.first.message.content\n        refused_when: response.choices.first.message.refusal\n        cut_off_when: {\n            path: \"choices.0.finish_reason\"\n            equals: \"length\"\n        }\n        usage: {\n            input: response.usage.prompt_tokens\n            output: response.usage.completion_tokens\n        }\n        schema: {\n            mode: AiSchema.strict_subset\n            at: \"format\"\n        }\n    }\n";

const MODELS: &str = "    models: {\n        context: models.context_windows\n    }\n";

fn provides(body: &str) -> String {
    format!("/// Acme's chat answers `ai.generate`.\nprovide ai {{\n{body}}}\n")
}

/// The package and the app's desk, `provides/ai.jadpo` as given (none
/// for `None`), with `app.jadpo` as given.
fn problems_with(app: &str, mapping: Option<&str>) -> Vec<String> {
    let files: Vec<(String, String)> = vec![
        ("app.jadpo".into(), app.into()),
        (CONFIG.0.into(), CONFIG.1.into()),
        (
            format!("{PACKAGE}/package.jadpo"),
            "/// Acme Models' chat API.\npackage AcmeModels.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n".into(),
        ),
        (
            format!("{PACKAGE}/provider.jadpo"),
            "/// Acme Models' API.\nprovider {\n    protocol: Protocol.json_http\n    hosts: {\n        live: \"https://api.acme-models.example\"\n        sandbox: \"https://sandbox.acme-models.example\"\n    }\n    credential: CredentialScheme.bearer\n    error_code: \"error.code\"\n}\n".into(),
        ),
        (
            format!("{PACKAGE}/resources/chat.jadpo"),
            "public resource chat {\n    complete: {\n        method: HttpMethod.post\n        path: \"/v1/chat\"\n        input: ChatRequest\n        output: ChatResponse\n        changes: false\n        source: PackageSource.written\n        docs: \"https://docs.acme-models.example/chat\"\n    }\n    erase: {\n        method: HttpMethod.post\n        path: \"/v1/erase\"\n        input: ChatRequest\n        output: ChatResponse\n        changes: true\n        source: PackageSource.written\n        docs: \"https://docs.acme-models.example/erase\"\n    }\n}\n".into(),
        ),
        (
            format!("{PACKAGE}/types/chat_turn.jadpo"),
            "/// One message of a conversation.\npublic type ChatTurn = Object {\n    role: Text {\n        max_length: 20\n    }\n    content: Text {\n        max_length: 100000\n    }\n}\n".into(),
        ),
        (
            format!("{PACKAGE}/types/chat_request.jadpo"),
            "/// What asking a model takes.\npublic type ChatRequest = Object {\n    model: Text {\n        max_length: 100\n    }\n    instructions: Text {\n        max_length: 100000\n        wire_name: \"system\"\n    }\n    messages: List<ChatTurn>\n    max_tokens: Int {\n        min: 1\n        max: 1000000\n    }\n}\n".into(),
        ),
        (
            format!("{PACKAGE}/types/chat_reply.jadpo"),
            "/// What the model said.\npublic type ChatReply = Object {\n    content: Text? {\n        max_length: 100000\n    }\n    refusal: Text? {\n        max_length: 2000\n    }\n}\n".into(),
        ),
        (
            format!("{PACKAGE}/types/chat_choice.jadpo"),
            "/// One answer.\npublic type ChatChoice = Object {\n    message: ChatReply\n    finish_reason: Text {\n        max_length: 40\n    }\n}\n".into(),
        ),
        (
            format!("{PACKAGE}/types/chat_usage.jadpo"),
            "/// The tokens a call used.\npublic type ChatUsage = Object {\n    prompt_tokens: TokenCount\n    completion_tokens: TokenCount\n}\n".into(),
        ),
        (
            format!("{PACKAGE}/types/chat_response.jadpo"),
            "/// The model's answer.\npublic type ChatResponse = Object {\n    choices: List<ChatChoice>\n    usage: ChatUsage\n}\n".into(),
        ),
        (
            format!("{PACKAGE}/types/model_context.jadpo"),
            "/// A model's context window.\ntype ModelContext = Object {\n    model: Text {\n        max_length: 100\n    }\n    tokens: TokenCount\n}\n".into(),
        ),
        (
            format!("{PACKAGE}/libraries/models.jadpo"),
            "/// The models Acme serves.\nlibrary models {\n    /// Each model's context window, in tokens.\n    constant context_windows: List<ModelContext> = [\n        ModelContext {\n            model: \"acme-small\"\n            tokens: 64\n        }\n    ]\n}\n".into(),
        ),
        (
            "domains/support/types/customer_question.jadpo".into(),
            "/// What a customer asked.\ntype CustomerQuestion = Text {\n    max_length: 500\n}\n".into(),
        ),
        (
            "domains/support/types/support_answer.jadpo".into(),
            "/// What the assistant says back.\ntype SupportAnswer = Output {\n    /// The reply to show the customer.\n    reply: Text {\n        max_length: 2000\n    }\n}\n".into(),
        ),
        (
            "domains/support/failures/assistant_unavailable.jadpo".into(),
            "/// The assistant can't answer now.\ntype AssistantUnavailable = Failure {\n    kind: FailureKind.unavailable\n}\n".into(),
        ),
        (
            "domains/support/prompts/answer_question.jadpo".into(),
            "use support/types { CustomerQuestion }\n\n/// Answers a customer's question.\ntype AnswerQuestion = Prompt {\n    question: CustomerQuestion\n}\n".into(),
        ),
        (
            "domains/support/actions/answer_customer.jadpo".into(),
            "use support/failures { AssistantUnavailable }\nuse support/prompts { AnswerQuestion }\nuse support/types { CustomerQuestion, SupportAnswer }\n\n/// Answers a customer's question.\naction answer_customer(question: CustomerQuestion) fails AssistantUnavailable -> SupportAnswer {\n    return attempt ai.generate(AnswerQuestion { question: question }, as: SupportAnswer) {\n        refused: AssistantUnavailable\n        malformed: AssistantUnavailable\n        timed_out: AssistantUnavailable\n        over_budget: AssistantUnavailable\n        too_long: AssistantUnavailable\n    }\n}\n".into(),
        ),
    ];
    let mut files = files;
    if let Some(mapping) = mapping {
        files.push((format!("{PACKAGE}/provides/ai.jadpo"), mapping.to_string()));
    }
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let db = Database::default();
    let project = support::project_with_records(
        &db,
        Layout::Project,
        &borrowed,
        &[(
            "domains/support/prompts/answer_question.prompt",
            "# Instructions\n\nYou answer questions from customers of an online bakery.\n\n# Request\n\n{question}\n",
        )],
    );
    jadpo_check::check(&db, project)
        .iter()
        .map(|d| {
            let path = d
                .primary
                .as_ref()
                .map_or("", |label| label.span.path.as_str());
            format!("{path} {}: {}", d.rule.id, d.message)
        })
        .collect()
}

fn problems(mapping: &str) -> Vec<String> {
    problems_with(APP, Some(mapping))
}

const PROVIDES: &str = "shared/providers/acme_models/v1/provides/ai.jadpo";

/// A package that maps `generate` and names its context table fills `ai`,
/// and its connection's `sandbox:` may name a prompt.
#[test]
fn a_package_fills_ai_with_its_mapping() {
    let found = problems(&provides(&format!("{GENERATE}{MODELS}")));
    assert!(found.is_empty(), "{found:#?}");
}

/// A package without `provides/ai.jadpo` doesn't fill `ai`.
#[test]
fn a_package_without_the_mapping_does_not_fill_ai() {
    let found = problems_with(APP, None);
    assert_eq!(
        found,
        [
            "app.jadpo ai.settings_invalid: `AcmeModels.v1` doesn't fill `ai`",
            "shared/providers/acme_models/v1/libraries/models.jadpo layout.unused_declaration: `context_windows` isn't used",
        ],
        "{found:#?}"
    );
}

/// `stream:` and `embed:` are decided and built by later chunks.
#[test]
fn streaming_and_embedding_wait_for_their_chunks() {
    let found = problems(&provides(&format!(
        "{GENERATE}    stream: {{\n        call: chat.complete\n    }}\n    embed: {{\n        call: chat.complete\n    }}\n{MODELS}"
    )));
    assert_eq!(
        found,
        [
            format!(
                "{PROVIDES} toolchain.not_built_yet: `provide ai {{ stream: }}` isn't built yet"
            ),
            format!(
                "{PROVIDES} toolchain.not_built_yet: `provide ai {{ embed: }}` isn't built yet"
            ),
        ],
        "{found:#?}"
    );
}

/// Each mistake in `generate:` is reported where it is written.
#[test]
fn each_mistake_in_the_mapping_is_reported() {
    let cases: [(&str, &str, &str); 9] = [
        (
            "call: chat.complete",
            "call: chat.compete",
            "`chat.compete` isn't one of the package's operations",
        ),
        (
            "call: chat.complete",
            "call: chat.erase",
            "`chat.erase` changes something at the provider",
        ),
        (
            "            max_tokens: request.max_output\n",
            "",
            "`ChatRequest`'s field `max_tokens` isn't mapped",
        ),
        (
            "max_tokens: request.max_output",
            "max_tokens: request.model",
            "`request.model` is text, and the field isn't",
        ),
        (
            "model: request.model",
            "model: request.messages",
            "`request.messages` is mapped one message at a time",
        ),
        (
            "answer: response.choices.first.message.content",
            "answer: response.choices.first.message",
            "`response.choices.first.message` isn't text",
        ),
        (
            "answer: response.choices.first.message.content",
            "answer: response.choices.first.message.text",
            "`response.choices.first.message.text` has no field `text`",
        ),
        (
            "path: \"choices.0.finish_reason\"",
            "path: \"choices.0.reason\"",
            "The answer has nothing at `choices.0.reason`",
        ),
        (
            "            mode: AiSchema.strict_subset\n            at: \"format\"\n",
            "            mode: AiSchema.none\n            at: \"format\"\n",
            "`AiSchema.none` sends no schema in the request",
        ),
    ];
    for (from, to, message) in cases {
        let changed = GENERATE.replace(from, to);
        assert_ne!(changed, GENERATE, "{from}");
        let found = problems(&provides(&format!("{changed}{MODELS}")));
        assert_eq!(
            found,
            [format!("{PROVIDES} package.provider_invalid: {message}")],
            "{from} -> {to}"
        );
    }
}

/// `models: { context: }` names a constant of the package's libraries
/// listing each model's `model:` and `tokens:`.
#[test]
fn the_context_table_is_a_constant_of_the_package() {
    let found = problems(&provides(&format!(
        "{GENERATE}    models: {{\n        context: models.windows\n    }}\n"
    )));
    assert_eq!(
        found,
        [
            "shared/providers/acme_models/v1/libraries/models.jadpo layout.unused_declaration: `context_windows` isn't used".to_string(),
            format!(
                "{PROVIDES} package.provider_invalid: `models.windows` isn't a constant of the package's libraries"
            )
        ],
        "{found:#?}"
    );
    let found = problems(&provides(GENERATE));
    assert_eq!(
        found,
        [
            "shared/providers/acme_models/v1/libraries/models.jadpo layout.unused_declaration: `context_windows` isn't used".to_string(),
            format!("{PROVIDES} package.provider_invalid: `provide ai` doesn't map `models:`")
        ],
        "{found:#?}"
    );
}

/// A connection's `sandbox:` names a prompt only when it fills `ai`.
#[test]
fn a_sandbox_names_a_prompt_only_of_the_connection_that_fills_ai() {
    let found = problems_with(
        &APP.replace("sandbox: [answer_question]", "sandbox: [answer_questions]"),
        Some(&provides(&format!("{GENERATE}{MODELS}"))),
    );
    assert_eq!(
        found,
        [
            "app.jadpo connection.value_invalid: `acme`'s `sandbox:` lists `answer_questions`, which `AcmeModels.v1` doesn't have"
        ],
        "{found:#?}"
    );
}
