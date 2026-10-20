//! Area 21, "The `ai` capability", "Embeddings and search" and "Tests"
//! (chunk 5.5v4), in the checker: `ai.embed(text, as: V)` and
//! `ai.embed_all(texts, as: V)` are outside calls only an action makes,
//! asking the `embedding` model unless `model:` names another, their `as:`
//! a vector type as long as `ai:`'s `embedding_dimensions:` says
//! (`semantic.ai_dimensions_mismatch`), and their three failures each
//! mapped to an `unavailable` failure; a test scripts an embedding's
//! failures with `fakes: { ai.embed: }`, and answers a prompt with one of
//! its recordings with `recorded("<name>")`.

use jadpo_db::{Database, Layout};

use crate::support;

const CONFIG: (&str, &str) = (
    "config.jadpo",
    "config {\n    model_url: Url {\n        default: \"http://localhost:11434/v1\"\n    }\n    drafting_model: ModelId {\n        default: \"local/llama3.2\"\n    }\n    search_model: ModelId {\n        default: \"local/nomic-embed-text\"\n    }\n}\n",
);

/// `ai:` with `models:` and `rest` after them.
fn app(models: &str, rest: &str) -> String {
    format!(
        "application {{\n    connections: {{\n        local_model: {{\n            driver: Driver.openai_compatible\n            url: config.model_url\n        }}\n    }}\n    ai: {{\n        connection: local_model\n        models: {{\n{models}        }}\n{rest}    }}\n}}\n"
    )
}

const MODELS: &str =
    "            default: config.drafting_model\n            embedding: config.search_model\n";

const DIMENSIONS: &str = "        embedding_dimensions: 3\n";

const TYPES: [(&str, &str); 6] = [
    (
        "domains/support/types/question_meaning.jadpo",
        "/// A question's meaning.\ntype QuestionMeaning = Vector {\n    dimensions: 3\n}\n",
    ),
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
        "domains/support/prompts/answer_question.jadpo",
        "use support/types { CustomerQuestion }\n\n/// Answers a customer's question.\ntype AnswerQuestion = Prompt {\n    question: CustomerQuestion\n}\n",
    ),
    (
        "domains/support/actions/answer_customer.jadpo",
        "use support/failures { AssistantUnavailable }\nuse support/prompts { AnswerQuestion }\nuse support/types { CustomerQuestion, SupportAnswer }\n\n/// Answers a customer's question.\naction answer_customer(question: CustomerQuestion) fails AssistantUnavailable -> SupportAnswer {\n    return attempt ai.generate(AnswerQuestion { question: question }, as: SupportAnswer) {\n        refused: AssistantUnavailable\n        malformed: AssistantUnavailable\n        timed_out: AssistantUnavailable\n        over_budget: AssistantUnavailable\n        too_long: AssistantUnavailable\n    }\n}\n",
    ),
];

const PROMPT: (&str, &str) = (
    "domains/support/prompts/answer_question.prompt",
    "# Instructions\n\nYou answer questions from customers of an online bakery.\n\n# Request\n\n{question}\n",
);

const RECORDING: (&str, &str) = (
    "domains/support/prompts/recordings/answer_question/not_covered.json",
    "{\n  \"answer\": {\n    \"reply\": \"{Text}\"\n  },\n  \"version\": \"answer_question@00000000\"\n}\n",
);

const ACTION_PATH: &str = "domains/support/actions/meaning_of.jadpo";

const ARMS: &str = "        timed_out: AssistantUnavailable\n        over_budget: AssistantUnavailable\n        too_long: AssistantUnavailable\n";

/// An action embedding `text` with `call` (`embed` or `embed_all`) and
/// `options` after `as:`, mapping with `arms`, giving `gives`.
fn action(call: &str, text: &str, options: &str, arms: &str, gives: &str) -> String {
    format!(
        "use support/failures {{ AssistantUnavailable }}\nuse support/types {{ CustomerQuestion, QuestionMeaning }}\n\n/// A meaning.\naction meaning_of(question: CustomerQuestion, questions: List<CustomerQuestion>) fails AssistantUnavailable -> {gives} {{\n    return attempt ai.{call}({text}, as: QuestionMeaning{options}) {{\n{arms}    }}\n}}\n"
    )
}

fn embedding(options: &str) -> String {
    action("embed", "question", options, ARMS, "QuestionMeaning")
}

/// Each problem as `path rule: message`.
fn problems_with(app: &str, files: &[(&str, &str)]) -> Vec<String> {
    let db = Database::default();
    let mut all: Vec<(&str, &str)> = vec![("app.jadpo", app), CONFIG];
    all.extend_from_slice(&TYPES);
    all.extend_from_slice(files);
    let project = support::project_with_records(&db, Layout::Project, &all, &[PROMPT, RECORDING]);
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
    problems_with(&app(MODELS, DIMENSIONS), files)
}

const NONE: &[&str] = &[];

/// An embedding asked through the local driver, each failure mapped, a
/// list of them, and a test scripting an embedding's failures and a
/// recorded answer, checks clean.
#[test]
fn embeddings_and_a_recorded_answer_check_clean() {
    let tests = "use support/actions { answer_customer }\nuse support/types { CustomerQuestion, QuestionMeaning, SupportAnswer }\n\ntest \"meanings\" {\n    fakes: {\n        ai.embed: [timed_out, over_budget, too_long]\n        ai.answer_question: [recorded(\"not_covered\")]\n    }\n    var answered: SupportAnswer = attempt answer_customer(CustomerQuestion(\"Jersey?\"))\n    assert answered == answered\n}\n";
    assert_eq!(
        problems(&[
            (ACTION_PATH, &embedding(", model: embedding, timeout: 30s")),
            (
                "domains/support/actions/meanings_of.jadpo",
                &action("embed_all", "questions", "", ARMS, "List<QuestionMeaning>")
                    .replace("meaning_of(", "meanings_of("),
            ),
            ("domains/support/actions/meaning_of.test.jadpo", tests),
        ]),
        NONE
    );
}

/// `embedding_dimensions:` is a whole number of 1 to 16,000, and every
/// vector type `ai.embed` answers as has that many dimensions.
#[test]
fn an_embedding_is_as_long_as_the_model_s() {
    let answer = embedding("");
    let files = [(ACTION_PATH, answer.as_str())];
    assert_eq!(
        problems_with(&app(MODELS, "        embedding_dimensions: 1536\n"), &files),
        [
            "domains/support/actions/meaning_of.jadpo semantic.ai_dimensions_mismatch: `QuestionMeaning` has 3 dimensions, and the model makes 1,536"
        ]
    );
    assert_eq!(
        problems_with(&app(MODELS, ""), &files),
        [
            "domains/support/actions/meaning_of.jadpo semantic.ai_dimensions_mismatch: `QuestionMeaning` has 3 dimensions, and `ai:` doesn't say how long the model's embeddings are"
        ]
    );
    assert_eq!(
        problems_with(&app(MODELS, "        embedding_dimensions: 0\n"), &files),
        [
            "app.jadpo ai.settings_invalid: `embedding_dimensions:` must be a whole number from 1 to 16,000",
            "domains/support/actions/meaning_of.jadpo semantic.ai_dimensions_mismatch: `QuestionMeaning` has 3 dimensions, and `ai:` doesn't say how long the model's embeddings are",
        ]
    );
}

/// An embedding asks the `embedding` model unless `model:` names another
/// `ai: { models: }` gives.
#[test]
fn an_embedding_asks_a_model_ai_names() {
    let answer = embedding("");
    assert_eq!(
        problems_with(
            &app("            default: config.drafting_model\n", DIMENSIONS),
            &[(ACTION_PATH, answer.as_str())]
        ),
        [
            "domains/support/actions/meaning_of.jadpo ai.settings_invalid: `ai.embed` asks the `embedding` model, and `ai:` names none"
        ]
    );
    let named = embedding(", model: embeding");
    assert_eq!(
        problems(&[(ACTION_PATH, named.as_str())]),
        [
            "domains/support/actions/meaning_of.jadpo semantic.unknown_name: `embeding` isn't a model `ai:` names"
        ]
    );
    let drafting = embedding(", model: default");
    assert_eq!(problems(&[(ACTION_PATH, drafting.as_str())]), NONE);
}

/// `as:` is a vector type with `dimensions:`; the text is text; the
/// options are `model:` and `timeout:`, written out.
#[test]
fn an_embedding_s_arguments_are_checked() {
    let check = |call: &str, text: &str, options: &str, gives: &str| {
        problems(&[(ACTION_PATH, &action(call, text, options, ARMS, gives))])
    };
    assert_eq!(
        check("embed", "questions", "", "QuestionMeaning"),
        [
            "domains/support/actions/meaning_of.jadpo type.mismatch: `List<CustomerQuestion>` can't be used where a text is required"
        ]
    );
    assert_eq!(
        check("embed_all", "question", "", "List<QuestionMeaning>"),
        [
            "domains/support/actions/meaning_of.jadpo type.mismatch: `CustomerQuestion` can't be used where a list of texts is required"
        ]
    );
    assert_eq!(
        check("embed", "question", ", timeout: 0s", "QuestionMeaning"),
        [
            "domains/support/actions/meaning_of.jadpo type.invalid_literal: `0s` isn't a valid `timeout:` for `ai.embed`"
        ]
    );
    assert_eq!(
        check("embed", "question", ", attempts: 2", "QuestionMeaning"),
        [
            "domains/support/actions/meaning_of.jadpo type.named_argument_unknown: `ai.embed` has no argument `attempts`"
        ]
    );
    let plain = problems(&[(
        ACTION_PATH,
        &action("embed", "question", "", ARMS, "QuestionMeaning")
            .replace("as: QuestionMeaning", "as: Vector"),
    )]);
    assert!(
        plain.contains(
            &"domains/support/actions/meaning_of.jadpo type.mismatch: `ai.embed` can't answer as `Vector`, which has no length"
                .to_string()
        ),
        "{plain:?}"
    );
    let text = problems(&[(
        ACTION_PATH,
        &action("embed", "question", "", ARMS, "QuestionMeaning")
            .replace("as: QuestionMeaning", "as: CustomerQuestion"),
    )]);
    assert!(
        text.contains(
            &"domains/support/actions/meaning_of.jadpo type.mismatch: `ai.embed` can't answer as `CustomerQuestion`"
                .to_string()
        ),
        "{text:?}"
    );
}

/// Each of the three failures is mapped to an `unavailable` failure, and
/// `invalid:` covers none; a call nothing handles is reported.
#[test]
fn every_failure_of_an_embedding_is_mapped() {
    let missing = problems(&[(
        ACTION_PATH,
        &action(
            "embed",
            "question",
            "",
            "        timed_out: AssistantUnavailable\n",
            "QuestionMeaning",
        ),
    )]);
    assert_eq!(
        missing,
        [
            "domains/support/actions/meaning_of.jadpo failure.rule_unmapped: `ai.embed(...)` can fail on `over_budget` and `too_long`, and nothing maps them"
        ]
    );
    let bare = "use support/types { CustomerQuestion, QuestionMeaning }\n\n/// A meaning.\naction meaning_of(question: CustomerQuestion) -> QuestionMeaning {\n    return ai.embed(question, as: QuestionMeaning)\n}\n";
    assert_eq!(
        problems(&[(ACTION_PATH, bare)]),
        [
            "domains/support/actions/meaning_of.jadpo failure.attempt_required: `ai.embed(...)` can fail at runtime, and nothing handles it"
        ]
    );
}

/// Only an action embeds: a function is pure, and a test reaches a model
/// through an action.
#[test]
fn only_an_action_asks_for_an_embedding() {
    let function = "use support/types { CustomerQuestion, QuestionMeaning }\n\n/// Meanings.\nlibrary meanings {\n    /// A meaning.\n    function meaning_of(question: CustomerQuestion) -> QuestionMeaning {\n        return ai.embed(question, as: QuestionMeaning)\n    }\n}\n";
    assert_eq!(
        problems(&[("domains/support/libraries/meanings.jadpo", function)])
            .into_iter()
            .filter(|problem| !problem.contains("layout.unused_declaration"))
            .collect::<Vec<_>>(),
        [
            "domains/support/libraries/meanings.jadpo effect.function_persistence: A function can't call `ai.embed`"
        ]
    );
}

/// `fakes: { ai.embed: }` scripts an embedding's failures; a vector or
/// another word isn't one, and `ai.embed_all` is scripted as `ai.embed`.
#[test]
fn an_embedding_s_scripted_answers_are_its_failures() {
    let answer = embedding("");
    let test = |fakes: &str| {
        format!(
            "use support/types {{ CustomerQuestion }}\n\ntest \"meanings\" {{\n    fakes: {{\n{fakes}\n    }}\n    assert meaning_of(CustomerQuestion(\"Rye?\"), []) fails AssistantUnavailable\n}}\n"
        )
        .replace(
            "use support/types",
            "use support/failures { AssistantUnavailable }\nuse support/types",
        )
    };
    let found = |fakes: &str| {
        problems(&[
            (ACTION_PATH, answer.as_str()),
            (
                "domains/support/actions/meaning_of.test.jadpo",
                &test(fakes),
            ),
        ])
    };
    assert_eq!(found("        ai.embed: [timed_out]"), NONE);
    assert_eq!(
        found("        ai.embed: [timed_ot]"),
        [
            "domains/support/actions/meaning_of.test.jadpo semantic.fake_invalid: `timed_ot` isn't an answer `ai.embed` gives"
        ]
    );
    assert_eq!(
        found("        ai.embed_all: [timed_out]"),
        [
            "domains/support/actions/meaning_of.test.jadpo semantic.fake_invalid: `ai.embed_all` isn't scripted on its own"
        ]
    );
}

/// `recorded("<name>")` names one of the prompt's recordings, beside it
/// in `recordings/<prompt>/`.
#[test]
fn a_recorded_answer_names_one_of_the_prompt_s_recordings() {
    let test = |answer: &str| {
        format!(
            "use support/types {{ CustomerQuestion, SupportAnswer }}\n\ntest \"recorded\" {{\n    fakes: {{\n        ai.answer_question: [{answer}]\n    }}\n    var answered: SupportAnswer = attempt answer_customer(CustomerQuestion(\"Jersey?\"))\n    assert answered == answered\n}}\n"
        )
    };
    let found = |answer: &str| -> Vec<String> {
        problems(&[(
            "domains/support/actions/answer_customer.test.jadpo",
            &test(answer),
        )])
        .into_iter()
        .filter(|problem| problem.contains("semantic.fake_invalid"))
        .collect()
    };
    assert_eq!(found("recorded(\"not_covered\")"), NONE);
    assert_eq!(
        found("recorded(\"not_coverd\")"),
        [
            "domains/support/actions/answer_customer.test.jadpo semantic.fake_invalid: `answer_question` has no recording `not_coverd`"
        ]
    );
    assert_eq!(
        found("recorded(name)"),
        [
            "domains/support/actions/answer_customer.test.jadpo semantic.fake_invalid: `recorded` takes a recording's name, written out"
        ]
    );
}

/// The network report lists the model connection for the domains that
/// embed, as for those that generate.
#[test]
fn a_domain_that_embeds_asks_the_model() {
    let db = Database::default();
    let answer = embedding("");
    let app = app(MODELS, DIMENSIONS);
    let mut all: Vec<(&str, &str)> = vec![("app.jadpo", app.as_str()), CONFIG];
    all.extend_from_slice(&TYPES[..4]);
    all.push((ACTION_PATH, answer.as_str()));
    let project = support::project_with_records(&db, Layout::Project, &all, &[]);
    assert_eq!(*jadpo_check::ai::asking_domains(&db, project), ["support"]);
}

/// An embedding's `timeout:` counts once in its route's deadline sum, as
/// every outside call's does (area 11's `route.calls_past_deadline`;
/// chunk 7.7e): 5s by default, so a search route embedding its query fits
/// the route's default 10s.
#[test]
fn an_embedding_counts_against_its_routes_deadline() {
    let search = |call: &str, options: &str| {
        format!(
            "use support/failures {{ AssistantUnavailable }}\nuse support/types {{ CustomerQuestion, QuestionMeaning }}\n\n/// Finds questions like this one.\naction find_similar(question: CustomerQuestion) fails AssistantUnavailable -> Unit {{\n    var meanings: {gives} = attempt ai.{call}({text}, as: QuestionMeaning{options}) {{\n{ARMS}    }}\n    assert meanings == meanings\n}}\n",
            gives = if call == "embed" {
                "QuestionMeaning"
            } else {
                "List<QuestionMeaning>"
            },
            text = if call == "embed" {
                "question"
            } else {
                "[question]"
            },
        )
    };
    let route = "use support/actions { find_similar }\nuse support/types { CustomerAsk }\n\n/// Where customers search past questions.\nroutes Similar {\n    prefix: /similar\n\n    /// Finds questions like a customer's.\n    POST / {\n        authentication: RouteAuthentication.none\n        input: CustomerAsk\n        status: SuccessStatus.no_content\n        run: find_similar(input.question)\n    }\n}\n";
    let ask = "use support/types { CustomerQuestion }\n\n/// A customer's question, as the page sends it.\ntype CustomerAsk = Input {\n    question: CustomerQuestion\n}\n";
    let policy = "use support\n\npolicy {\n    actions: {\n        support.find_similar: [Access.public]\n    }\n}\n";
    let found = |call: &str, options: &str| -> Vec<String> {
        problems(&[
            (
                "domains/support/actions/find_similar.jadpo",
                &search(call, options),
            ),
            ("domains/support/routes/similar.jadpo", route),
            ("domains/support/types/customer_ask.jadpo", ask),
            ("policy.jadpo", policy),
        ])
        .into_iter()
        .filter(|problem| problem.contains(" route.calls_past_deadline: "))
        .collect()
    };
    assert_eq!(found("embed", ""), NONE);
    assert_eq!(found("embed_all", ""), NONE);
    assert_eq!(
        found("embed", ", timeout: 15s"),
        [
            "domains/support/routes/similar.jadpo route.calls_past_deadline: `POST /similar` runs `find_similar`, whose outside calls can take 15s, longer than its 10s deadline"
        ]
    );
    assert_eq!(
        found("embed_all", ", timeout: 12s"),
        [
            "domains/support/routes/similar.jadpo route.calls_past_deadline: `POST /similar` runs `find_similar`, whose outside calls can take 12s, longer than its 10s deadline"
        ]
    );
}
