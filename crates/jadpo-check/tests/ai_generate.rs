//! Area 21, "The `ai` capability" and "Typed output" (chunk 5.5v3), in the
//! checker: `ai:` in `application { }` names a connection with `driver:
//! Driver.openai_compatible` and its models, each a `ModelId` setting;
//! `ai.generate(prompt, as: T)` is an outside call only an action makes,
//! its options written out, its `as:` a type a model's answer can be, and
//! its five failures each mapped to an `unavailable` failure; a `ModelId`
//! is never written in code; and a test scripts a model's answers with
//! `fakes: { ai.<prompt>: }` and reads what was asked with `.requests`.

use jadpo_db::{Database, Layout};

use crate::support;

const CONFIG: (&str, &str) = (
    "config.jadpo",
    "config {\n    model_url: Url {\n        default: \"http://localhost:11434/v1\"\n    }\n    drafting_model: ModelId {\n        default: \"local/llama3.2\"\n    }\n    reply_limit: Int {\n        default: 3\n    }\n    ai_daily_spend: Money {\n        default: \"GBP 20.00\"\n    }\n}\n",
);

const APP: &str = "application {\n    connections: {\n        local_model: {\n            driver: Driver.openai_compatible\n            url: config.model_url\n        }\n    }\n    ai: {\n        connection: local_model\n        models: {\n            default: config.drafting_model\n        }\n        limits: {\n            per_day: {\n                tokens: 2000000\n            }\n        }\n    }\n}\n";

/// The daily limit every `ai:` gives (chunk 5.5v5).
const LIMITS: &str = "        limits: {\n            per_day: {\n                tokens: 2000000\n            }\n        }\n";

const TYPES: [(&str, &str); 5] = [
    (
        "domains/support/types/customer_question.jadpo",
        "/// What a customer asked.\ntype CustomerQuestion = Text {\n    max_length: 500\n}\n",
    ),
    (
        "domains/support/types/support_answer.jadpo",
        "/// What the assistant says back.\ntype SupportAnswer = Output {\n    /// The reply to show the customer.\n    reply: Text {\n        max_length: 2000\n    }\n}\n",
    ),
    (
        "domains/support/failures/assistant_unavailable.jadpo",
        "/// The assistant can't answer now.\ntype AssistantUnavailable = Failure {\n    kind: FailureKind.unavailable\n}\n",
    ),
    (
        "domains/support/failures/question_invalid.jadpo",
        "/// The question breaks a rule.\ntype QuestionInvalid = Failure {\n    kind: FailureKind.invalid_value\n}\n",
    ),
    (
        "domains/support/prompts/answer_question.jadpo",
        "use support/types { CustomerQuestion }\n\n/// Answers a customer's question.\ntype AnswerQuestion = Prompt {\n    question: CustomerQuestion\n}\n",
    ),
];

const PROMPT: (&str, &str) = (
    "domains/support/prompts/answer_question.prompt",
    "# Instructions\n\nYou answer questions from customers of an online bakery.\n\n# Request\n\n{question}\n",
);

const ACTION_PATH: &str = "domains/support/actions/answer_customer.jadpo";

/// The action, asking with `options` after `as:` and mapping with `arms`.
fn action(options: &str, arms: &str) -> String {
    format!(
        "use support/failures {{ AssistantUnavailable }}\nuse support/prompts {{ AnswerQuestion }}\nuse support/types {{ CustomerQuestion, SupportAnswer }}\n\n/// Answers a customer's question.\naction answer_customer(question: CustomerQuestion) fails AssistantUnavailable -> SupportAnswer {{\n    return attempt ai.generate(AnswerQuestion {{ question: question }}, as: SupportAnswer{options}) {{\n{arms}    }}\n}}\n"
    )
}

const ALL_ARMS: &str = "        refused: AssistantUnavailable\n        malformed: AssistantUnavailable\n        timed_out: AssistantUnavailable\n        over_budget: AssistantUnavailable\n        too_long: AssistantUnavailable\n";

/// Each problem as `path rule: message`, for the desk with `app.jadpo`
/// and the files given.
fn problems_with(app: &str, files: &[(&str, &str)]) -> Vec<String> {
    let db = Database::default();
    let mut all: Vec<(&str, &str)> = vec![("app.jadpo", app), CONFIG];
    all.extend_from_slice(&TYPES);
    all.extend_from_slice(files);
    let project = support::project_with_migrated_records(&db, Layout::Project, &all, &[PROMPT]);
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

fn problems(files: &[(&str, &str)]) -> Vec<String> {
    problems_with(APP, files)
}

const NONE: &[&str] = &[];

/// The problems of rule `rule` alone, past what a fragment leaves unused.
fn of(rule: &str, found: Vec<String>) -> Vec<String> {
    found
        .into_iter()
        .filter(|problem| problem.contains(&format!(" {rule}: ")))
        .collect()
}

/// The desk checks clean: a model asked through the local driver, every
/// failure mapped, and a test scripting each kind of answer.
#[test]
fn a_model_asked_with_every_failure_mapped_checks_clean() {
    let tests = "use support/failures { AssistantUnavailable }\nuse support/prompts { AnswerQuestion }\nuse support/types { CustomerQuestion, SupportAnswer }\n\ntest \"answers\" {\n    fakes: {\n        ai.answer_question: [SupportAnswer { reply: \"No.\" }, raw(\"{}\"), refused, malformed]\n    }\n    var answered: SupportAnswer = attempt answer_customer(CustomerQuestion(\"Sesame?\"))\n    assert answered.reply == \"No.\"\n    var asked: List<AnswerQuestion> = ai.answer_question.requests\n    assert list.count(asked) == 1\n    assert answer_customer(CustomerQuestion(\"Rye?\")) fails AssistantUnavailable\n}\n";
    assert_eq!(
        problems(&[
            (
                ACTION_PATH,
                &action(
                    ", model: default, max_output: 600, timeout: 30s, attempts: 3",
                    ALL_ARMS
                )
            ),
            ("domains/support/actions/answer_customer.test.jadpo", tests),
        ]),
        NONE
    );
}

/// `ai:` names a model connection and its models, each a `ModelId`
/// setting with `default` among them; anything else is
/// `ai.settings_invalid`, and what later chunks build is
/// `toolchain.not_built_yet`.
#[test]
fn ai_s_settings_are_checked() {
    let answer = action("", ALL_ARMS);
    let files = [(ACTION_PATH, answer.as_str())];
    let limits = LIMITS;
    let with = |ai: &str| {
        problems_with(
            &format!(
                "application {{\n    connections: {{\n        local_model: {{\n            driver: Driver.openai_compatible\n            url: config.model_url\n        }}\n        mail: {{\n            driver: Driver.smtp\n            host: \"localhost\"\n        }}\n    }}\n    ai: {{\n{ai}{limits}    }}\n}}\n"
            ),
            &files,
        )
    };
    assert_eq!(
        with(
            "        connection: mail\n        models: {\n            default: config.drafting_model\n        }\n"
        ),
        ["app.jadpo ai.settings_invalid: `connection: mail` answers no model"]
    );
    assert_eq!(
        with(
            "        connection: local_model\n        models: {\n            default: config.reply_limit\n        }\n"
        ),
        ["app.jadpo ai.settings_invalid: Model `default` must be a `ModelId` setting"]
    );
    assert_eq!(
        with(
            "        connection: local_model\n        models: {\n            default: \"local/llama3.2\"\n        }\n"
        ),
        ["app.jadpo ai.settings_invalid: Model `default` must be a setting"]
    );
    assert_eq!(
        with(
            "        connection: local_model\n        models: {\n            drafting: config.drafting_model\n        }\n"
        ),
        ["app.jadpo ai.settings_invalid: `models:` has no `default:`"]
    );
    assert_eq!(
        with(
            "        conection: local_model\n        models: {\n            default: config.drafting_model\n        }\n"
        ),
        [
            "app.jadpo ai.settings_invalid: `ai:` has no `connection:`",
            "app.jadpo ai.settings_invalid: `conection` isn't one of `ai:`'s settings",
        ]
    );
    assert_eq!(
        with(
            "        connection: local_model\n        models: {\n            default: config.drafting_model\n        }\n        keep_content: {\n            keep: 30d\n        }\n"
        ),
        ["app.jadpo toolchain.not_built_yet: `ai: { keep_content: }` isn't built yet"]
    );
}

/// `ai:` caps what its calls use with `limits:`: `per_day: { tokens: }`
/// is required, so nothing is unlimited (`semantic.ai_budget_missing`),
/// `cost:` is a `Money` setting, the others whole numbers of tokens, and
/// the counts are kept in a database (chunk 5.5v5).
#[test]
fn the_daily_limit_is_required_and_checked() {
    let answer = action("", ALL_ARMS);
    let files = [(ACTION_PATH, answer.as_str())];
    let with = |limits: &str| {
        problems_with(
            &format!(
                "application {{\n    connections: {{\n        local_model: {{\n            driver: Driver.openai_compatible\n            url: config.model_url\n        }}\n        mail: {{\n            driver: Driver.smtp\n            host: \"localhost\"\n        }}\n    }}\n    ai: {{\n        connection: local_model\n        models: {{\n            default: config.drafting_model\n        }}\n{limits}    }}\n}}\n"
            ),
            &files,
        )
    };
    let limits = |inner: &str| format!("        limits: {{\n{inner}        }}\n");
    assert_eq!(
        with(&limits(
            "            per_request: 20000\n            per_day: {\n                tokens: 2000000\n                cost: config.ai_daily_spend\n            }\n            per_caller_per_day: 60000\n            store: main\n"
        )),
        NONE
    );
    assert_eq!(
        with(""),
        ["app.jadpo semantic.ai_budget_missing: `ai:` sets no daily limit"]
    );
    assert_eq!(
        with(&limits("            per_request: 20000\n")),
        ["app.jadpo semantic.ai_budget_missing: `limits:` sets no daily limit"]
    );
    assert_eq!(
        with(&limits(
            "            per_day: {\n                cost: config.ai_daily_spend\n            }\n"
        )),
        ["app.jadpo semantic.ai_budget_missing: `per_day:` sets no daily tokens"]
    );
    let day = "            per_day: {\n                tokens: 2000000\n            }\n";
    assert_eq!(
        with(&limits(&format!("{day}            per_request: 0\n"))),
        ["app.jadpo ai.settings_invalid: `per_request` must be a number of tokens"]
    );
    assert_eq!(
        with(&limits(&format!(
            "{day}            per_caller_per_day: 2.5\n"
        ))),
        ["app.jadpo ai.settings_invalid: `per_caller_per_day` must be a number of tokens"]
    );
    assert_eq!(
        with(&limits(&format!("{day}            per_hour: 100\n"))),
        ["app.jadpo ai.settings_invalid: `per_hour` isn't one of `limits:`'s settings"]
    );
    assert_eq!(
        with(&limits(&format!("{day}            store: mail\n"))),
        ["app.jadpo ai.settings_invalid: `store: mail` isn't a database"]
    );
    assert_eq!(
        with(&limits(
            "            per_day: {\n                tokens: 2000000\n                cost: \"20.00 GBP\"\n            }\n"
        )),
        ["app.jadpo ai.settings_invalid: `cost:` must be a setting"]
    );
    assert_eq!(
        with(&limits(
            "            per_day: {\n                tokens: 2000000\n                cost: config.reply_limit\n            }\n"
        )),
        ["app.jadpo ai.settings_invalid: `cost:` must be an amount of money"]
    );
}

/// A model connection gives `url:`, an address a call may be sent to:
/// HTTPS, or plain HTTP only to this machine.
#[test]
fn a_model_connection_gives_an_address_it_may_reach() {
    let answer = action("", ALL_ARMS);
    let files = [(ACTION_PATH, answer.as_str())];
    let with = |connection: &str| {
        problems_with(
            &format!(
                "application {{\n    connections: {{\n        local_model: {{\n            driver: Driver.openai_compatible\n{connection}        }}\n    }}\n    ai: {{\n        connection: local_model\n        models: {{\n            default: config.drafting_model\n        }}\n{LIMITS}    }}\n}}\n"
            ),
            &files,
        )
    };
    assert_eq!(
        with("            url: \"https://models.example/v1\"\n"),
        NONE
    );
    assert_eq!(
        with("            url: \"http://127.0.0.1:8080/v1\"\n"),
        NONE
    );
    assert_eq!(
        with("            url: \"http://models.example/v1\"\n"),
        [
            "app.jadpo connection.value_invalid: `local_model`'s `url:` is plain HTTP to another machine"
        ]
    );
    assert_eq!(
        with(""),
        ["app.jadpo connection.key_missing: `local_model` doesn't say which server it reaches"]
    );
    assert_eq!(
        with("            url: config.model_url\n            key: config.model_url\n"),
        [
            "app.jadpo connection.key_invalid: A connection with `driver: Driver.openai_compatible` takes no `key:`"
        ]
    );
}

/// An action that asks a model with no `ai:` to ask through is
/// `ai.connection_required`.
#[test]
fn asking_a_model_needs_ai() {
    let answer = action("", ALL_ARMS);
    assert_eq!(
        problems_with("application {}\n", &[(ACTION_PATH, answer.as_str())]),
        [format!(
            "{ACTION_PATH} ai.connection_required: `ai.generate` asks a model, and the application names none"
        )]
    );
}

/// A `ModelId` is a setting, never written in code; a setting's own
/// values write one.
#[test]
fn a_model_written_in_code_is_refused() {
    let found = problems(&[(
        "domains/support/libraries/fallbacks.jadpo",
        "library fallbacks {\n    /// The model to fall back on.\n    function fallback_model() -> ModelId {\n        return \"openai/gpt-4.1-mini\"\n    }\n}\n",
    )]);
    assert_eq!(
        of("semantic.ai_model_literal", found),
        [
            "domains/support/libraries/fallbacks.jadpo semantic.ai_model_literal: `\"openai/gpt-4.1-mini\"` writes a model in code"
        ]
    );
}

/// `as:` names a type a model's answer can be: never a reference, a
/// file, bytes, text made for display or a secret.
#[test]
fn a_model_answers_only_as_a_type_json_from_it_can_be() {
    let with = |declared: &str, answer: &str| {
        let action = format!(
            "use support/failures {{ AssistantUnavailable }}\nuse support/prompts {{ AnswerQuestion }}\nuse support/types {{ CustomerQuestion, Picked }}\n\n/// Answers.\naction pick(question: CustomerQuestion) fails AssistantUnavailable -> {answer} {{\n    var picked: Picked = attempt ai.generate(AnswerQuestion {{ question: question }}, as: Picked) {{\n{ALL_ARMS}    }}\n    return picked.chosen\n}}\n"
        );
        problems(&[
            (
                "domains/support/types/picked.jadpo",
                &format!(
                    "/// What was picked.\ntype Picked = Object {{\n    chosen: {declared}\n}}\n"
                ),
            ),
            (
                "domains/support/entities/help_article.jadpo",
                "/// An article.\nentity HelpArticle {\n    id: Uuid\n}\n",
            ),
            ("domains/support/actions/pick.jadpo", &action),
        ])
    };
    let refused = |found: Vec<String>| {
        found
            .iter()
            .filter(|problem| problem.contains("semantic.ai_output_type"))
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(
        refused(with("HelpArticle.id", "HelpArticle.id")),
        [
            "domains/support/actions/pick.jadpo semantic.ai_output_type: A model can't answer as `Picked`"
        ]
    );
    assert_eq!(
        refused(with("Bytes", "Bytes")),
        [
            "domains/support/actions/pick.jadpo semantic.ai_output_type: A model can't answer as `Picked`"
        ]
    );
    let secret = with("Password", "Password");
    assert!(
        secret.iter().any(|problem| problem.contains(
            "configuration.secret_flow: `ai.generate` can't answer as `Picked`, which holds a `Password`"
        )),
        "{secret:#?}"
    );
}

/// The options are written out, within their bounds, and `model:` names a
/// model `ai:` lists.
#[test]
fn the_options_are_checked() {
    let with = |options: &str| problems(&[(ACTION_PATH, &action(options, ALL_ARMS))]);
    assert_eq!(
        with(", attempts: 4"),
        [format!(
            "{ACTION_PATH} type.invalid_literal: `4` isn't a valid `attempts:` for `ai.generate`"
        )]
    );
    assert_eq!(
        with(", max_output: 0"),
        [format!(
            "{ACTION_PATH} type.invalid_literal: `0` isn't a valid `max_output:` for `ai.generate`"
        )]
    );
    assert_eq!(
        with(", timeout: 20m"),
        [format!(
            "{ACTION_PATH} type.invalid_literal: `20m` isn't a valid `timeout:` for `ai.generate`"
        )]
    );
    assert_eq!(
        with(", model: drafting"),
        [format!(
            "{ACTION_PATH} semantic.unknown_name: `drafting` isn't a model `ai:` names"
        )]
    );
    assert_eq!(
        with(", temperature: 0"),
        [format!(
            "{ACTION_PATH} type.named_argument_unknown: `ai.generate` has no argument `temperature`"
        )]
    );
}

/// Each of the five failures is mapped to an `unavailable` failure, and
/// `invalid:` covers none of them.
#[test]
fn every_failure_is_mapped_to_an_unavailable_failure() {
    assert_eq!(
        problems(&[(
            ACTION_PATH,
            &action(
                "",
                "        refused: AssistantUnavailable\n        malformed: AssistantUnavailable\n"
            )
        )]),
        [format!(
            "{ACTION_PATH} failure.rule_unmapped: `ai.generate(...)` can fail on `timed_out`, `over_budget` and `too_long`, and nothing maps them"
        )]
    );
    assert_eq!(
        problems(&[(
            ACTION_PATH,
            &action(
                "",
                "        refused: AssistantUnavailable\n        invalid: AssistantUnavailable\n"
            )
        )]),
        [format!(
            "{ACTION_PATH} failure.rule_unmapped: `ai.generate(...)` can fail on `malformed`, `timed_out`, `over_budget` and `too_long`, and nothing maps them"
        )]
    );
    let found = problems(&[(
        ACTION_PATH,
        &action(
            "",
            &ALL_ARMS.replace(
                "too_long: AssistantUnavailable",
                "too_long: QuestionInvalid",
            ),
        )
        .replace(
            "use support/failures { AssistantUnavailable }",
            "use support/failures { AssistantUnavailable, QuestionInvalid }",
        )
        .replace(
            "fails AssistantUnavailable ->",
            "fails AssistantUnavailable, QuestionInvalid ->",
        ),
    )]);
    assert_eq!(
        found,
        [format!(
            "{ACTION_PATH} failure.mapping_kind: `too_long:` needs an `unavailable` failure, and `QuestionInvalid` is `invalid_value`"
        )]
    );
}

/// A test reaching a call reads its model's setting and its address,
/// so each needs a value in tests.
#[test]
fn a_test_reaching_a_call_needs_its_settings() {
    let db = Database::default();
    let answer = action("", ALL_ARMS);
    let test = "use support/failures { AssistantUnavailable }\nuse support/types { CustomerQuestion }\n\ntest \"answers\" {\n    fakes: {\n        ai.answer_question: [refused]\n    }\n    assert answer_customer(CustomerQuestion(\"Rye?\")) fails AssistantUnavailable\n}\n";
    let mut all: Vec<(&str, &str)> = vec![
        ("app.jadpo", APP),
        (
            "config.jadpo",
            "config {\n    model_url: Url\n    drafting_model: ModelId {\n        default: \"local/llama3.2\"\n    }\n    reply_limit: Int {\n        default: 3\n    }\n}\n",
        ),
    ];
    all.extend_from_slice(&TYPES);
    all.push((ACTION_PATH, answer.as_str()));
    all.push(("domains/support/actions/answer_customer.test.jadpo", test));
    let project = support::project_with_migrated_records(&db, Layout::Project, &all, &[PROMPT]);
    let found: Vec<String> = jadpo_check::check(&db, project)
        .iter()
        .map(|d| format!("{}: {}", d.rule.id, d.message))
        .collect();
    assert_eq!(
        found,
        ["configuration.test_value_missing: `config.model_url` has no value in tests"]
    );
}

/// A call nothing handles says which conditions to map.
#[test]
fn a_model_asked_with_no_attempt_is_reported() {
    let found = problems(&[(
        "domains/support/actions/draft.jadpo",
        "use support/prompts { AnswerQuestion }\nuse support/types { CustomerQuestion, SupportAnswer }\n\n/// Drafts.\naction draft(question: CustomerQuestion) -> SupportAnswer {\n    return ai.generate(AnswerQuestion { question: question }, as: SupportAnswer)\n}\n",
    )]);
    assert_eq!(
        of("failure.attempt_required", found),
        [
            "domains/support/actions/draft.jadpo failure.attempt_required: `ai.generate(...)` can fail at runtime, and nothing handles it"
        ]
    );
}

/// Only an action asks a model, never in a loop, and before its first
/// write.
#[test]
fn only_an_action_asks_a_model_before_it_writes() {
    let function = problems(&[(
        "domains/support/libraries/drafting.jadpo",
        "use support/prompts { AnswerQuestion }\nuse support/types { CustomerQuestion, SupportAnswer }\n\nlibrary drafting {\n    /// Drafts.\n    function drafted(question: CustomerQuestion) -> SupportAnswer {\n        return ai.generate(AnswerQuestion { question: question }, as: SupportAnswer)\n    }\n}\n",
    )]);
    assert_eq!(
        of("effect.function_persistence", function),
        [
            "domains/support/libraries/drafting.jadpo effect.function_persistence: A function can't call `ai.generate`"
        ]
    );
    let looped = problems(&[(
        "domains/support/actions/answer_all.jadpo",
        "use support/failures { AssistantUnavailable }\nuse support/prompts { AnswerQuestion }\nuse support/types { CustomerQuestion, SupportAnswer }\n\n/// Answers each.\naction answer_all(questions: List<CustomerQuestion>) fails AssistantUnavailable -> List<SupportAnswer> {\n    var answers: List<SupportAnswer> = []\n    for question in questions {\n        var answered: SupportAnswer = attempt ai.generate(AnswerQuestion { question: question }, as: SupportAnswer) {\n            refused: AssistantUnavailable\n            malformed: AssistantUnavailable\n            timed_out: AssistantUnavailable\n            over_budget: AssistantUnavailable\n            too_long: AssistantUnavailable\n        }\n        answers = list.append(answers, answered)\n    }\n    return answers\n}\n",
    )]);
    assert!(
        looped
            .iter()
            .any(|problem| problem.contains("effect.call_in_loop")),
        "{looped:#?}"
    );
    let written = problems(&[
        (
            "domains/support/entities/note.jadpo",
            "/// A note the desk keeps.\nentity Note {\n    id: Uuid\n    body: Text\n}\n",
        ),
        (
            "domains/support/actions/note_then_ask.jadpo",
            &format!(
                "use support/entities {{ Note }}\nuse support/failures {{ AssistantUnavailable }}\nuse support/prompts {{ AnswerQuestion }}\nuse support/types {{ CustomerQuestion, SupportAnswer }}\n\n/// Notes the question, then asks.\naction note_then_ask(question: CustomerQuestion) fails AssistantUnavailable -> SupportAnswer {{\n    var kept: Note = Note.create {{\n        set: {{\n            body: \"asked\"\n        }}\n    }}\n    assert kept.body == \"asked\"\n    return attempt ai.generate(AnswerQuestion {{ question: question }}, as: SupportAnswer) {{\n{ALL_ARMS}    }}\n}}\n"
            ),
        ),
    ]);
    assert_eq!(
        of("data.write_before_call", written).len(),
        1,
        "a write before the model is asked"
    );
}

/// A test scripts a model's answers by its prompt's name: a value of the
/// type its calls ask for, `raw(...)` text, one of the five failures, or
/// `recorded(...)`, naming one of the prompt's recordings (chunk 5.5v4).
#[test]
fn a_test_s_scripted_answers_are_checked() {
    let answer = action("", ALL_ARMS);
    let with = |fakes: &str| {
        let test = format!(
            "use support/failures {{ AssistantUnavailable }}\nuse support/types {{ CustomerQuestion, SupportAnswer }}\n\ntest \"answers\" {{\n    fakes: {{\n        {fakes}\n    }}\n    assert answer_customer(CustomerQuestion(\"Rye?\")) fails AssistantUnavailable\n}}\n"
        );
        problems(&[
            (ACTION_PATH, answer.as_str()),
            ("domains/support/actions/answer_customer.test.jadpo", &test),
        ])
    };
    let test_path = "domains/support/actions/answer_customer.test.jadpo";
    assert_eq!(
        of(
            "semantic.fake_invalid",
            with("ai.answer_questoin: [refused]")
        ),
        [format!(
            "{test_path} semantic.fake_invalid: `answer_questoin` isn't a prompt"
        )]
    );
    assert_eq!(
        of(
            "semantic.fake_invalid",
            with("ai.answer_question: [refusd]")
        ),
        [format!(
            "{test_path} semantic.fake_invalid: `refusd` isn't an answer `ai.answer_question` gives"
        )]
    );
    assert_eq!(
        of(
            "semantic.fake_invalid",
            with("ai.answer_question: [recorded(\"sesame\")]")
        ),
        [format!(
            "{test_path} semantic.fake_invalid: `answer_question` has no recording `sesame`"
        )]
    );
    assert_eq!(
        of(
            "type.mismatch",
            with("ai.answer_question: [CustomerQuestion(\"x\")]")
        ),
        [format!(
            "{test_path} type.mismatch: `CustomerQuestion` can't be used where `SupportAnswer` is required"
        )]
    );
}

/// `.requests` is what a test's fake kept: read anywhere else, it is
/// `semantic.not_callable`.
#[test]
fn requests_are_read_only_in_a_test() {
    let found = problems(&[(
        "domains/support/actions/asked.jadpo",
        "use support/prompts { AnswerQuestion }\n\n/// What was asked.\naction asked() -> List<AnswerQuestion> {\n    return ai.answer_question.requests\n}\n",
    )]);
    assert_eq!(
        of("semantic.not_callable", found),
        [
            "domains/support/actions/asked.jadpo semantic.not_callable: Only a test reads `ai.answer_question.requests`"
        ]
    );
}

/// The schema a model is asked to answer in is OpenAPI's for the type,
/// whole, its fields' descriptions included, its references in `$defs`.
#[test]
fn the_schema_is_the_types_own_with_its_descriptions() {
    let db = Database::default();
    let answer = action("", ALL_ARMS);
    let mut all: Vec<(&str, &str)> = vec![("app.jadpo", APP), CONFIG];
    all.extend_from_slice(&TYPES);
    all.push((ACTION_PATH, answer.as_str()));
    let project = support::project_with_migrated_records(&db, Layout::Project, &all, &[PROMPT]);
    let graph = jadpo_check::graph::graph(&db, project);
    let key = graph
        .nodes()
        .iter()
        .find(|node| node.name == "SupportAnswer" && node.kind == jadpo_graph::Kind::Type)
        .map(|node| node.key.clone())
        .expect("SupportAnswer");
    let (schema, object) =
        jadpo_check::ai::schema(graph, None, &jadpo_graph::TypeRef::Node(key)).expect("a schema");
    assert!(object);
    let schema = jadpo_graph::json::parse(&schema).expect("JSON");
    let at = |path: &[&str]| {
        path.iter()
            .try_fold(&schema, |value, key| value.get(key))
            .cloned()
    };
    let text = |path: &[&str]| at(path).and_then(|value| value.as_str().map(str::to_string));
    assert_eq!(text(&["$ref"]).as_deref(), Some("#/$defs/SupportAnswer"));
    assert_eq!(
        text(&["$defs", "SupportAnswer", "description"]).as_deref(),
        Some("What the assistant says back.")
    );
    assert_eq!(
        text(&[
            "$defs",
            "SupportAnswer",
            "properties",
            "reply",
            "description"
        ])
        .as_deref(),
        Some("The reply to show the customer.")
    );
    assert_eq!(
        at(&[
            "$defs",
            "SupportAnswer",
            "properties",
            "reply",
            "x-jadpo-constraints"
        ]),
        None
    );
    // The field's inline rule is the schema's, as OpenAPI's is (chunk 7.7d).
    assert_eq!(
        at(&["$defs", "SupportAnswer", "properties", "reply", "maxLength"]),
        Some(jadpo_graph::json::Value::from(2000u32))
    );
    // A type built on `Text` asks for plain text.
    let text = graph
        .nodes()
        .iter()
        .find(|node| node.name == "CustomerQuestion")
        .map(|node| node.key.clone())
        .expect("CustomerQuestion");
    assert_eq!(
        jadpo_check::ai::schema(graph, None, &jadpo_graph::TypeRef::Node(text)),
        None
    );
}

/// A route reaching `ai.generate` leaves room for it: the call's
/// `timeout:` times its `attempts:` (25s and 2 by default, which a route
/// gives `deadline:` room for, Richard 2026-10-20), summed with
/// the route's other outside calls, fits within the route's deadline
/// (area 21, "Operations"; area 11's `route.calls_past_deadline`, chunk
/// 7.5e).
#[test]
fn a_route_leaves_room_for_its_ai_calls() {
    let route = "use support/actions { answer_customer }\nuse support/types { CustomerAsk, SupportAnswer }\n\n/// Where customers ask the assistant.\nroutes Questions {\n    prefix: /questions\n\n    /// Answers a customer's question.\n    POST / {\n        authentication: RouteAuthentication.none\n        input: CustomerAsk\n        output: SupportAnswer\n        status: SuccessStatus.ok\n        run: answer_customer(input.question)\n    }\n}\n";
    let ask = "use support/types { CustomerQuestion }\n\n/// A customer's question, as the page sends it.\ntype CustomerAsk = Input {\n    question: CustomerQuestion\n}\n";
    let policy = "use support\n\npolicy {\n    actions: {\n        support.answer_customer: [Access.public]\n    }\n}\n";
    let found_at = |options: &str, route: &str| {
        of(
            "route.calls_past_deadline",
            problems(&[
                (ACTION_PATH, &action(options, ALL_ARMS)),
                ("domains/support/routes/questions.jadpo", route),
                ("domains/support/types/customer_ask.jadpo", ask),
                ("policy.jadpo", policy),
            ]),
        )
    };
    let found = |options: &str| found_at(options, route);
    assert_eq!(
        found(""),
        [
            "domains/support/routes/questions.jadpo route.calls_past_deadline: `POST /questions` runs `answer_customer`, whose outside calls can take 50s, longer than its 10s deadline"
        ]
    );
    assert_eq!(
        found(", timeout: 10s, attempts: 2"),
        [
            "domains/support/routes/questions.jadpo route.calls_past_deadline: `POST /questions` runs `answer_customer`, whose outside calls can take 20s, longer than its 10s deadline"
        ]
    );
    assert_eq!(found(", timeout: 8s, attempts: 1"), NONE);
    // The defaults fit a route that waits the most a route may.
    let patient = route.replace("        run:", "        deadline: 60s\n        run:");
    assert_eq!(found_at("", &patient), NONE);
}

/// A store holding only AI counters still owns hourly retention. Its lease
/// is part of the same generated schema/snapshot as any other system table.
#[test]
fn ai_only_store_migrates_the_housekeeping_lease() {
    let db = Database::default();
    let project = support::project_with_migrated_records(
        &db,
        Layout::Project,
        &[("app.jadpo", APP), CONFIG],
        &[],
    );
    let schema = jadpo_check::data::schema(&db, project);
    let store = schema.stores.get("main").expect("AI-only main store");
    assert!(store.tables.contains_key("jadpo_ai_usage"));
    assert!(store.tables.contains_key("jadpo_housekeeping"));
    assert!(!store.tables.contains_key("jadpo_outbox"));
    let mut previous = (**schema).clone();
    previous
        .stores
        .get_mut("main")
        .unwrap()
        .tables
        .remove("jadpo_housekeeping");
    let migration = jadpo_graph::migration::migration_sql(&previous, schema, &|_| None)
        .expect("lease migrates")
        .to_document("hourly_cleanup");
    assert!(
        migration.contains("CREATE TABLE \"jadpo_housekeeping\""),
        "{migration}"
    );
    for column in ["lease_id", "lease_until", "last_cleaned_at"] {
        assert!(migration.contains(&format!("\"{column}\"")), "{migration}");
    }
}
