//! Area 21's `ai.stream` and area 7's streaming routes (chunk 5.5v6), in
//! the checker: `ai.stream(prompt)` is an outside call only an action
//! makes, giving an `AiStream`, with `model:`, `max_output:` and
//! `timeout:` and none of `ai.generate`'s `as:` and `attempts:`, its four
//! failures each mapped to an `unavailable` failure; a route whose action
//! returns one sends it with `output: AiStream`, answering 200 and
//! waiting for nothing; its route test reads `response.pieces`; and a
//! test scripts a streamed prompt's text and `partial(...)`.

use jadpo_db::{Database, Layout};

use crate::support;

const CONFIG: (&str, &str) = (
    "config.jadpo",
    "config {\n    model_url: Url {\n        default: \"http://localhost:11434/v1\"\n    }\n    drafting_model: ModelId {\n        default: \"local/llama3.2\"\n    }\n}\n",
);

const APP: (&str, &str) = (
    "app.jadpo",
    "application {\n    connections: {\n        local_model: {\n            driver: Driver.openai_compatible\n            url: config.model_url\n        }\n    }\n    ai: {\n        connection: local_model\n        models: {\n            default: config.drafting_model\n        }\n        limits: {\n            per_day: {\n                tokens: 2000000\n            }\n        }\n    }\n}\n",
);

const POLICY: (&str, &str) = (
    "policy.jadpo",
    "use support\n\npolicy {\n    actions: {\n        support.stream_answer: [Access.public]\n    }\n}\n",
);

const TYPES: [(&str, &str); 4] = [
    (
        "domains/support/types/customer_question.jadpo",
        "/// What a customer asked.\ntype CustomerQuestion = Text {\n    max_length: 500\n}\n",
    ),
    (
        "domains/support/types/asked.jadpo",
        "use support/types { CustomerQuestion }\n\n/// A customer's question.\ntype Asked = Input {\n    question: CustomerQuestion\n}\n",
    ),
    (
        "domains/support/failures/assistant_unavailable.jadpo",
        "/// The assistant can't answer now.\ntype AssistantUnavailable = Failure {\n    kind: FailureKind.unavailable\n    message: \"The assistant can't answer now.\"\n}\n",
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

const ACTION_PATH: &str = "domains/support/actions/stream_answer.jadpo";
const ROUTE_PATH: &str = "domains/support/routes/answers.jadpo";
const TEST_PATH: &str = "domains/support/routes/answers.test.jadpo";

const ALL_ARMS: &str = "        refused: AssistantUnavailable\n        timed_out: AssistantUnavailable\n        over_budget: AssistantUnavailable\n        too_long: AssistantUnavailable\n";

/// The action, streaming with `options` and mapping with `arms`.
fn action(options: &str, arms: &str) -> String {
    format!(
        "use support/failures {{ AssistantUnavailable }}\nuse support/prompts {{ AnswerQuestion }}\nuse support/types {{ Asked }}\n\n/// Streams an answer to a customer's question.\naction stream_answer(asked: Asked) fails AssistantUnavailable -> AiStream {{\n    return attempt ai.stream(AnswerQuestion {{ question: asked.question }}{options}) {{\n{arms}    }}\n}}\n"
    )
}

/// The route, with `items` besides its input, output and run.
fn route(items: &str) -> String {
    format!(
        "use support/actions {{ stream_answer }}\nuse support/types {{ Asked }}\n\n/// Customers' questions.\nroutes Answers {{\n    prefix: /answers\n\n    /// Streams the assistant's reply as it is written.\n    POST / {{\n        authentication: RouteAuthentication.none\n        input: Asked\n        output: AiStream\n{items}        run: stream_answer(input)\n    }}\n}}\n"
    )
}

fn problems(files: &[(&str, &str)]) -> Vec<String> {
    let db = Database::default();
    let mut all: Vec<(&str, &str)> = vec![APP, CONFIG, POLICY];
    all.extend_from_slice(&TYPES);
    all.extend_from_slice(files);
    let project = support::project_with_migrated_records(&db, Layout::Project, &all, &[PROMPT]);
    jadpo_check::check(&db, project)
        .iter()
        // The API's record is `jadpo api update`'s, read in the CLI's tests.
        .filter(|d| d.rule.id != "record.out_of_date")
        .map(|d| {
            let path = d
                .primary
                .as_ref()
                .map_or("", |label| label.span.path.as_str());
            format!("{path} {}: {}", d.rule.id, d.message)
        })
        .collect()
}

/// The problems of rule `rule` alone.
fn of(rule: &str, found: Vec<String>) -> Vec<String> {
    found
        .into_iter()
        .filter(|problem| problem.contains(&format!(" {rule}: ")))
        .collect()
}

const NONE: &[&str] = &[];

const TESTS: &str = "use support/types { Asked }\n\ntest \"a reply comes piece by piece\" {\n    fakes: {\n        ai.answer_question: [\"Our sourdough is baked beside sesame.\", partial(\"Our sourdough\", timed_out), refused]\n    }\n    var response: Response = request {\n        method: HttpMethod.post\n        path: \"/answers\"\n        input: Asked {\n            question: \"Sesame?\"\n        }\n    }\n    assert response.status == 200\n    assert response.pieces == [\"Our sourdough is \", \"baked beside sesame.\"]\n    assert response.code == none\n}\n";

/// A streamed answer, its route and a route test reading its pieces check
/// clean.
#[test]
fn a_streaming_route_and_its_test_check_clean() {
    assert_eq!(
        problems(&[
            (
                ACTION_PATH,
                &action(", model: default, max_output: 600, timeout: 8s", ALL_ARMS)
            ),
            (ROUTE_PATH, &route("")),
            (TEST_PATH, TESTS),
        ]),
        NONE
    );
}

/// `ai.stream` takes no `as:` and no `attempts:`, and is never
/// `malformed`; each of its four conditions needs an arm.
#[test]
fn a_streams_options_and_conditions_are_its_own() {
    let found = problems(&[
        (ACTION_PATH, &action(", attempts: 2", ALL_ARMS)),
        (ROUTE_PATH, &route("")),
    ]);
    assert_eq!(
        of("type.named_argument_unknown", found),
        [format!(
            "{ACTION_PATH} type.named_argument_unknown: `ai.stream` has no argument `attempts`"
        )]
    );
    let found = problems(&[
        (
            ACTION_PATH,
            &action(
                "",
                "        refused: AssistantUnavailable\n        timed_out: AssistantUnavailable\n        over_budget: AssistantUnavailable\n",
            ),
        ),
        (ROUTE_PATH, &route("")),
    ]);
    assert_eq!(of("failure.rule_unmapped", found).len(), 1);
    let found = problems(&[
        (
            ACTION_PATH,
            &action(
                "",
                &format!("{ALL_ARMS}        malformed: AssistantUnavailable\n"),
            ),
        ),
        (ROUTE_PATH, &route("")),
    ]);
    assert!(
        found.iter().any(|problem| problem.contains("malformed")),
        "{found:?}"
    );
}

/// Only an action streams: a function can't, and neither can a test.
#[test]
fn only_an_action_streams() {
    let function = "use support/prompts { AnswerQuestion }\n\nfunction asks() -> AiStream {\n    return ai.stream(AnswerQuestion { question: \"Rye?\" })\n}\n";
    let found = problems(&[
        (ACTION_PATH, &action("", ALL_ARMS)),
        (ROUTE_PATH, &route("")),
        ("domains/support/functions/asks.jadpo", function),
    ]);
    assert_eq!(
        of("effect.function_persistence", found),
        [
            "domains/support/functions/asks.jadpo effect.function_persistence: A function can't call `ai.stream`"
        ]
    );
}

/// A streaming route answers 200 and waits for nothing.
#[test]
fn a_streaming_route_answers_ok_and_waits_for_nothing() {
    let found = problems(&[
        (ACTION_PATH, &action("", ALL_ARMS)),
        (
            ROUTE_PATH,
            &route("        status: SuccessStatus.created\n"),
        ),
    ]);
    assert_eq!(
        of("route.stream_invalid", found),
        [format!(
            "{ROUTE_PATH} route.stream_invalid: `POST /answers` streams a model's answer with `SuccessStatus.created`"
        )]
    );
}

/// A streamed prompt's script is text, a stream's conditions or
/// `partial(...)`, which stops only as a stream can once started; a
/// prompt no action streams can't stop part-way.
#[test]
fn a_streamed_prompts_script_is_checked() {
    let test = |answers: &str| {
        format!(
            "use support/types {{ Asked }}\n\ntest \"streams\" {{\n    fakes: {{\n        ai.answer_question: [{answers}]\n    }}\n    var response: Response = request {{\n        method: HttpMethod.post\n        path: \"/answers\"\n        input: Asked {{\n            question: \"Sesame?\"\n        }}\n    }}\n    assert response.status == 200\n}}\n"
        )
    };
    let with = |answers: &str| {
        problems(&[
            (ACTION_PATH, &action(", timeout: 8s", ALL_ARMS)),
            (ROUTE_PATH, &route("")),
            (TEST_PATH, &test(answers)),
        ])
    };
    assert_eq!(
        of("semantic.fake_invalid", with("partial(\"Our\", too_long)")),
        [format!(
            "{TEST_PATH} semantic.fake_invalid: `too_long` isn't how a stream stops part-way"
        )]
    );
    assert_eq!(
        of("semantic.fake_invalid", with("malformed")),
        [format!(
            "{TEST_PATH} semantic.fake_invalid: `malformed` isn't an answer `ai.answer_question` gives"
        )]
    );
    assert_eq!(
        of("semantic.fake_invalid", with("partial(\"Our\")")),
        [format!(
            "{TEST_PATH} semantic.fake_invalid: `partial` takes the text sent and how the stream stops"
        )]
    );
    assert_eq!(with("\"Rye.\", over_budget, too_long, raw(\"Rye.\")"), NONE);
}

/// A streaming route's response has `pieces` and no `output`.
#[test]
fn a_streaming_routes_response_has_pieces() {
    let test = "use support/types { Asked }\n\ntest \"reads\" {\n    var response: Response = request {\n        method: HttpMethod.post\n        path: \"/answers\"\n        input: Asked {\n            question: \"Sesame?\"\n        }\n    }\n    assert response.output == none\n}\n";
    let found = problems(&[
        (ACTION_PATH, &action(", timeout: 8s", ALL_ARMS)),
        (ROUTE_PATH, &route("")),
        (TEST_PATH, test),
    ]);
    assert_eq!(
        found,
        [format!(
            "{TEST_PATH} type.unknown_field: `Response` has no field `output`"
        )]
    );
}

/// A stream's `timeout:` counts once in its route's deadline sum, since it
/// asks once (area 7, "Time"; area 11's `route.calls_past_deadline`;
/// chunk 7.7e): 25s by default, past the route's default 10s, so a route
/// that streams by default gives itself `deadline:`.
#[test]
fn a_streams_timeout_counts_once_against_its_routes_deadline() {
    let found = |options: &str, items: &str| {
        of(
            "route.calls_past_deadline",
            problems(&[
                (ACTION_PATH, &action(options, ALL_ARMS)),
                (ROUTE_PATH, &route(items)),
            ]),
        )
    };
    assert_eq!(
        found("", ""),
        [format!(
            "{ROUTE_PATH} route.calls_past_deadline: `POST /answers` runs `stream_answer`, whose outside calls can take 25s, longer than its 10s deadline"
        )]
    );
    assert_eq!(found("", "        deadline: 25s\n"), NONE);
    assert_eq!(found(", timeout: 10s", ""), NONE);
}
