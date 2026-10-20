//! Area 21, "How a provider fills it" (chunk 5.5v3b), in the checker: a
//! package's `provides/ai.jadpo` maps `generate` onto one of its own
//! operations as data, and `ai: { connection: }` names a connection to
//! a package that does; each mistake in the mapping is
//! `package.provider_invalid` where it is written; `stream:` and
//! `embed:` (chunk 5.5v7) are read and checked as `generate:` is.

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
    problems_with_extra(app, mapping, &[])
}

/// [`problems_with`], with `extra` files besides, or in place of the
/// files of the same paths.
fn problems_with_extra(app: &str, mapping: Option<&str>, extra: &[(&str, &str)]) -> Vec<String> {
    let mut files: Vec<(String, String)> = vec![
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
    // An extra file in a base file's place stands in for it.
    files.retain(|(path, _)| !extra.iter().any(|(given, _)| given == path));
    files.extend(
        extra
            .iter()
            .map(|(path, text)| (path.to_string(), text.to_string())),
    );
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

/// The `stream:` mapping of the package as the tests write it (chunk
/// 5.5v7): the chat operation's answer read as a stream of events.
const STREAM: &str = "    stream: {\n        call: chat.complete\n        input: {\n            model: request.model\n            instructions: request.instructions\n            messages: {\n                each: request.messages\n                as: {\n                    role: item.role\n                    content: item.content\n                }\n            }\n            max_tokens: request.max_output\n        }\n        pieces: event.choices.first.message.content\n        refused_when: {\n            holds: event.choices.first.message.refusal\n            path: \"choices.0.finish_reason\"\n            equals: \"content_filter\"\n        }\n        cut_off_when: {\n            path: \"choices.0.finish_reason\"\n            equals: \"length\"\n        }\n        done_when: {\n            path: \"choices.0.finish_reason\"\n            equals: \"stop\"\n        }\n    }\n";

/// The `embed:` mapping of the package as the tests write it (chunk
/// 5.5v7), onto [`EMBEDDINGS`]' operation.
const EMBED: &str = "    embed: {\n        call: embeddings.create\n        input: {\n            model: request.model\n            input: request.texts\n            dimensions: request.dimensions\n        }\n        vectors: response.data\n        vector: item.embedding\n        index: item.index\n        batch: 2048\n        usage: {\n            input: response.usage.prompt_tokens\n            output: 0\n        }\n    }\n";

/// The package's embeddings operation and its types.
const EMBEDDINGS: &[(&str, &str)] = &[
    (
        "shared/providers/acme_models/v1/resources/embeddings.jadpo",
        "public resource embeddings {\n    create: {\n        method: HttpMethod.post\n        path: \"/v1/embeddings\"\n        input: EmbedRequest\n        output: EmbedResponse\n        changes: false\n        source: PackageSource.written\n        docs: \"https://docs.acme-models.example/embeddings\"\n    }\n}\n",
    ),
    (
        "shared/providers/acme_models/v1/types/embed_request.jadpo",
        "/// What embedding texts takes.\npublic type EmbedRequest = Object {\n    model: Text {\n        max_length: 100\n    }\n    input: List<Text>\n    dimensions: Int? {\n        min: 1\n        max: 16000\n    }\n}\n",
    ),
    (
        "shared/providers/acme_models/v1/types/embed_item.jadpo",
        "/// One text's embedding.\npublic type EmbedItem = Object {\n    index: Int {\n        min: 0\n        max: 1000\n    }\n    embedding: Vector\n}\n",
    ),
    (
        "shared/providers/acme_models/v1/types/embed_response.jadpo",
        "/// The texts' embeddings.\npublic type EmbedResponse = Object {\n    data: List<EmbedItem>\n    usage: ChatUsage\n}\n",
    ),
];

/// A package maps `stream:` and `embed:` beside `generate:` (chunk
/// 5.5v7).
#[test]
fn a_package_maps_streaming_and_embedding() {
    let found = problems_with_extra(
        APP,
        Some(&provides(&format!("{GENERATE}{STREAM}{EMBED}{MODELS}"))),
        EMBEDDINGS,
    );
    assert!(found.is_empty(), "{found:#?}");
}

/// Each mistake in `stream:` and `embed:` is reported where it is written.
#[test]
fn each_mistake_in_streaming_and_embedding_is_reported() {
    let cases: [(&str, &str, &str); 8] = [
        (
            "pieces: event.choices.first.message.content",
            "pieces: event.choices.first.message.text",
            "`event.choices.first.message.text` has no field `text`",
        ),
        (
            "pieces: event.choices.first.message.content",
            "pieces: response.choices.first.message.content",
            "`response.choices.first.message.content` isn't a place in the event",
        ),
        (
            "            path: \"choices.0.finish_reason\"\n            equals: \"content_filter\"\n",
            "            path: \"choices.0.finish_reason\"\n",
            "`refused_when:` needs a `path:` with the value it `equals:`, or a place it `holds:`",
        ),
        (
            "            path: \"choices.0.finish_reason\"\n            equals: \"stop\"\n",
            "            path: \"choices.0.done\"\n            equals: \"stop\"\n",
            "The answer has nothing at `choices.0.done`",
        ),
        (
            "            input: request.texts\n",
            "            input: request.messages\n",
            "`request.messages` isn't a value a mapping can give",
        ),
        (
            "vector: item.embedding",
            "vector: item.index",
            "`item.index` isn't a vector",
        ),
        (
            "vectors: response.data",
            "vectors: response.usage",
            "`response.usage` isn't a list of objects",
        ),
        (
            "        index: item.index\n",
            "        index: item.index\n        size: 3\n",
            "`embed:` has no `size:`",
        ),
    ];
    let both = format!("{STREAM}{EMBED}");
    for (from, to, message) in cases {
        let changed = both.replacen(from, to, 1);
        assert_ne!(changed, both, "{from}");
        let found = problems_with_extra(
            APP,
            Some(&provides(&format!("{GENERATE}{changed}{MODELS}"))),
            EMBEDDINGS,
        );
        assert_eq!(
            found,
            [format!("{PROVIDES} package.provider_invalid: {message}")],
            "{from} -> {to}"
        );
    }
    // A provider that takes fewer texts at once than `ai.embed_all` sends.
    let found = problems_with_extra(
        APP,
        Some(&provides(&format!(
            "{GENERATE}{}{MODELS}",
            EMBED.replace("batch: 2048", "batch: 16")
        ))),
        EMBEDDINGS,
    );
    assert_eq!(
        found,
        [format!(
            "{PROVIDES} toolchain.not_built_yet: Embedding fewer than 100 texts a request isn't built yet"
        )],
        "{found:#?}"
    );
}

/// `schema: { envelope: }` names the provider's own shape; Anthropic's has
/// no JSON-only mode, and no mode has none's.
#[test]
fn the_envelope_goes_with_its_mode() {
    let anthropic = GENERATE.replace(
        "            at: \"format\"\n",
        "            at: \"output_config.format\"\n            envelope: AiEnvelope.anthropic\n",
    );
    let found = problems(&provides(&format!("{anthropic}{MODELS}")));
    assert!(found.is_empty(), "{found:#?}");
    let json_only = anthropic.replace("AiSchema.strict_subset", "AiSchema.json_only");
    let found = problems(&provides(&format!("{json_only}{MODELS}")));
    assert_eq!(
        found,
        [format!(
            "{PROVIDES} package.provider_invalid: This envelope doesn't go with the schema's mode"
        )],
        "{found:#?}"
    );
    let unknown = anthropic.replace("AiEnvelope.anthropic", "AiEnvelope.gemini");
    let found = problems(&provides(&format!("{unknown}{MODELS}")));
    assert_eq!(
        found,
        [format!(
            "{PROVIDES} package.provider_invalid: `AiEnvelope.gemini` isn't an envelope"
        )],
        "{found:#?}"
    );
}

/// `CredentialScheme.header` sends the credential in the header
/// `credential_header:` names, and `headers:` lists the fixed headers
/// every request carries, never one the driver writes (chunk 5.5v7).
#[test]
fn a_header_credential_names_its_header() {
    let provider = "shared/providers/acme_models/v1/provider.jadpo";
    let with = |body: &str| {
        problems_with_extra(
            APP,
            Some(&provides(&format!("{GENERATE}{MODELS}"))),
            &[(
                provider,
                &format!(
                    "/// Acme Models' API.\nprovider {{\n    protocol: Protocol.json_http\n    hosts: {{\n        live: \"https://api.acme-models.example\"\n        sandbox: \"https://sandbox.acme-models.example\"\n    }}\n{body}    error_code: \"error.code\"\n}}\n"
                ),
            )],
        )
    };
    let found = with(
        "    credential: CredentialScheme.header\n    credential_header: \"x-api-key\"\n    headers: [\"acme-version: 2026-01-01\"]\n",
    );
    assert!(found.is_empty(), "{found:#?}");
    let cases: [(&str, &str); 4] = [
        (
            "    credential: CredentialScheme.header\n",
            "`provider` doesn't give `credential_header:`",
        ),
        (
            "    credential: CredentialScheme.bearer\n    credential_header: \"x-api-key\"\n",
            "`credential_header:` goes with `CredentialScheme.header` alone",
        ),
        (
            "    credential: CredentialScheme.header\n    credential_header: \"Authorization\"\n",
            "`credential_header:` isn't a header the credential can go in",
        ),
        (
            "    credential: CredentialScheme.bearer\n    headers: [\"acme-version 2026\"]\n",
            "`headers:` has `acme-version 2026`, which isn't `Name: value`",
        ),
    ];
    for (body, message) in cases {
        let found = with(body);
        assert_eq!(
            found,
            [format!("{provider} package.provider_invalid: {message}")],
            "{body}"
        );
    }
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

/// An embedding asked through a package's connection whose mapping has
/// no `embed:` is told so where it asks (chunk 5.5v7).
#[test]
fn an_embedding_through_a_package_needs_its_mapping() {
    let found = problems_with_extra(
        APP,
        Some(&provides(&format!("{GENERATE}{MODELS}"))),
        &[
            (
                "domains/support/types/question_meaning.jadpo",
                "/// A question's meaning.\ntype QuestionMeaning = Vector {\n    dimensions: 3\n}\n",
            ),
            (
                "domains/support/actions/meaning_of.jadpo",
                "use support/types { CustomerQuestion, QuestionMeaning }\nuse support/failures { AssistantUnavailable }\n\n/// A question's meaning.\naction meaning_of(question: CustomerQuestion) fails AssistantUnavailable -> QuestionMeaning {\n    return attempt ai.embed(question, as: QuestionMeaning, model: default) {\n        timed_out: AssistantUnavailable\n        over_budget: AssistantUnavailable\n        too_long: AssistantUnavailable\n    }\n}\n",
            ),
        ],
    );
    assert_eq!(
        found,
        ["domains/support/actions/meaning_of.jadpo ai.settings_invalid: `acme` doesn't embed"],
        "{found:#?}"
    );
}
