//! What `ai` calls may spend (area 21, "Budgets"; chunk 5.5v5): the
//! support desk of [`crate::ai_providers`], its model priced by its
//! package, under `ai: { limits: }`.
//!
//! - **Each limit refuses a call over it** `over_budget`, with nothing
//!   sent, in `jadpo test`: the request's (two calls in one action), the
//!   day's tokens and the day's estimated cost (calls of separate
//!   requests), each counted in the store's `jadpo_ai_usage` and charged
//!   what each call used; the call's own `max_output:` is its cut-off. A
//!   test's calls answered by the fake are charged the estimate of what
//!   was sent and answered.
//! - **In `jadpo dev`** a caller's day is counted for its address, and a
//!   sandboxed prompt is charged what the provider says it used, under the
//!   same daily limit; `jadpo ai usage` shows the day and the month by
//!   prompt and model, with the estimated cost, from the counts alone.
//!
//! Never a real provider or the network: the sandbox is a server started
//! in the test on this machine.

use std::fs;
use std::path::Path;

use crate::ai_providers::{SANDBOXED, STRICT, ask, chat, desk, jadpo, write};
use crate::http::{Dev, wait_for};
use crate::support::fake_provider::FakeProvider;

/// The daily limit `desk` gives, which each case replaces.
const GIVEN: &str = "        limits: {\n            per_day: {\n                tokens: 2000000\n            }\n        }\n";

/// `app.jadpo` with `limits` in place of the desk's own.
fn limited(root: &Path, limits: &str) {
    let path = root.join("app.jadpo");
    let app = fs::read_to_string(&path).expect("app.jadpo");
    assert!(app.contains(GIVEN), "{app}");
    fs::write(&path, app.replace(GIVEN, limits)).expect("write");
}

/// Two actions besides the desk's: one asking once with one attempt, and
/// one asking twice in one request.
fn actions(root: &Path) {
    let call = |question: &str| {
        format!(
            "attempt ai.generate(\n        AnswerQuestion {{\n            question: {question}\n        }},\n        as: SupportAnswer,\n        max_output: 300,\n        attempts: 1\n    ) {{\n        refused: AssistantUnavailable\n        malformed: AssistantUnavailable\n        timed_out: AssistantUnavailable\n        over_budget: AssistantUnavailable\n        too_long: QuestionTooLong\n    }}"
        )
    };
    let header = "use support/failures { AssistantUnavailable, QuestionTooLong }\nuse support/prompts { AnswerQuestion }\nuse support/types { Asked, SupportAnswer }\n\n";
    write(
        root,
        "domains/support/actions/answer_once.jadpo",
        &format!(
            "{header}/// Answers a question with one attempt.\naction answer_once(asked: Asked) fails AssistantUnavailable, QuestionTooLong -> SupportAnswer {{\n    return {}\n}}\n",
            call("asked.question")
        ),
    );
    write(
        root,
        "domains/support/actions/answer_twice.jadpo",
        &format!(
            "{header}/// Asks the model twice in one request.\naction answer_twice(asked: Asked) fails AssistantUnavailable, QuestionTooLong -> SupportAnswer {{\n    var first: SupportAnswer = {}\n    var second: SupportAnswer = {}\n    return SupportAnswer {{\n        resolved_by_articles: second.resolved_by_articles\n        reply: first.reply\n    }}\n}}\n",
            call("asked.question"),
            call("asked.question")
        ),
    );
    write(
        root,
        "policy.jadpo",
        "use support\n\npolicy {\n    actions: {\n        support.answer_customer: [Access.public]\n        support.answer_once: [Access.public]\n        support.answer_twice: [Access.public]\n    }\n}\n",
    );
}

/// A scripted answer about 230 tokens long, as the four-bytes-a-token
/// estimate counts it: the JSON pads its object with spaces.
fn long_answer() -> String {
    format!(
        "raw(\"{{\\\"resolved_by_articles\\\": true, \\\"reply\\\": \\\"No sesame.\\\"{}}}\")",
        " ".repeat(800)
    )
}

const TEST_HEADER: &str = "use support/actions { answer_twice }\nuse support/failures { AssistantUnavailable }\nuse support/types { Asked, SupportAnswer }\n\n";

/// Runs `tests` under `limits` with `jadpo test`: each passes.
fn passes(root: &Path, limits: &str, tests: &str, count: usize) {
    limited(root, limits);
    write(
        root,
        "domains/support/actions/answer_once.test.jadpo",
        &format!("{TEST_HEADER}{tests}"),
    );
    jadpo(root, &["records", "update"]);
    let checked = jadpo(root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let tested = jadpo(root, &["test"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(
        tested.stdout.contains(&format!("{count} passed")),
        "{}{}",
        tested.stdout,
        tested.stderr
    );
    // Put the desk's own limit back for the next case.
    limited_back(root, limits);
}

fn limited_back(root: &Path, limits: &str) {
    let path = root.join("app.jadpo");
    let app = fs::read_to_string(&path).expect("app.jadpo");
    fs::write(&path, app.replace(limits, GIVEN)).expect("write");
}

/// Each limit refuses `over_budget` the call that would pass it, before
/// anything is sent: the request's, the day's tokens and the day's
/// estimated cost; and the call's own `max_output:`, where the answer is
/// cut off.
#[test]
fn each_limit_refuses_the_call_that_would_pass_it() {
    let root = desk(
        "budget-limits",
        "https://sandbox.acme-models.example",
        STRICT,
        "",
    );
    actions(&root);
    let long = long_answer();

    // The request's: a call takes its prompt's tokens and `max_output:`
    // (about 330), and is charged what it used (about 250 for the long
    // answer), so a second call in the same request passes 500; the next
    // request starts again.
    passes(
        &root,
        "        limits: {\n            per_request: 500\n            per_day: {\n                tokens: 2000000\n            }\n        }\n",
        &format!(
            "test \"a second call in one request passes its limit\" {{\n    fakes: {{\n        ai.answer_question: [{long}]\n    }}\n    assert answer_twice(Asked {{ question: \"Sesame?\" }}) fails AssistantUnavailable\n}}\n\ntest \"each request has its own\" {{\n    fakes: {{\n        ai.answer_question: [{long}, {long}]\n    }}\n    var first: SupportAnswer = attempt answer_once(Asked {{ question: \"Sesame?\" }})\n    var second: SupportAnswer = attempt answer_once(Asked {{ question: \"Rye?\" }})\n    assert second.reply == first.reply\n}}\n"
        ),
        2,
    );

    // The day's tokens: the third call of the day would pass 700.
    passes(
        &root,
        "        limits: {\n            per_day: {\n                tokens: 700\n            }\n        }\n",
        &format!(
            "test \"the day's tokens run out\" {{\n    fakes: {{\n        ai.answer_question: [{long}, {long}]\n    }}\n    var first: SupportAnswer = attempt answer_once(Asked {{ question: \"Sesame?\" }})\n    var second: SupportAnswer = attempt answer_once(Asked {{ question: \"Rye?\" }})\n    assert second.reply == first.reply\n    assert answer_once(Asked {{ question: \"Spelt?\" }}) fails AssistantUnavailable\n}}\n"
        ),
        1,
    );

    // The day's estimated cost: at 100 and 400 US dollars a million input
    // and output tokens, a call may cost about 0.12 and is charged about
    // 0.08, so the third would pass 0.25.
    write(
        &root,
        crate::ai_providers::LIBRARY,
        &crate::ai_providers::library("100.00", "10.00", "400.00"),
    );
    let config = fs::read_to_string(root.join("config.jadpo")).expect("config");
    write(
        &root,
        "config.jadpo",
        &config.replacen(
            "config {\n",
            "config {\n    ai_daily_spend: Money {\n        default: \"USD 0.25\"\n    }\n",
            1,
        ),
    );
    passes(
        &root,
        "        limits: {\n            per_day: {\n                tokens: 2000000\n                cost: config.ai_daily_spend\n            }\n        }\n",
        &format!(
            "test \"the day's spend runs out\" {{\n    fakes: {{\n        ai.answer_question: [{long}, {long}]\n    }}\n    var first: SupportAnswer = attempt answer_once(Asked {{ question: \"Sesame?\" }})\n    var second: SupportAnswer = attempt answer_once(Asked {{ question: \"Rye?\" }})\n    assert second.reply == first.reply\n    assert answer_once(Asked {{ question: \"Spelt?\" }}) fails AssistantUnavailable\n}}\n\ntest \"an answer cut off at max_output is over budget\" {{\n    fakes: {{\n        ai.answer_question: [over_budget]\n    }}\n    assert answer_once(Asked {{ question: \"Rye?\" }}) fails AssistantUnavailable\n}}\n"
        ),
        2,
    );
}

/// In `jadpo dev` a caller's day is counted for the address it comes
/// from, and a sandboxed prompt is charged what the provider says it
/// used, under the same daily limit; `jadpo ai usage` shows the day and
/// the month by prompt and model, with the estimated cost.
#[test]
fn development_counts_each_caller_and_the_sandbox_and_shows_the_usage() {
    let provider = FakeProvider::start();
    let root = desk("budget-dev", &provider.url, STRICT, SANDBOXED);
    write(&root, ".env.local", "ACME_KEY=ak_test_3\n");
    // A call of the desk's takes about 1008 tokens before it is sent (its
    // prompt and `max_output:` for each of two attempts, and the repair's
    // answer and correction resent: chunk 7.9c); the provider says the
    // first used 39, so a second from the same address would pass 1020
    // for the caller, while the day's 2000000 is far off.
    limited(
        &root,
        "        limits: {\n            per_day: {\n                tokens: 2000000\n            }\n            per_caller_per_day: 1020\n        }\n",
    );
    jadpo(&root, &["records", "update"]);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let dev = Dev::start_without_database(&root);
    wait_for(&dev, "\"listening\"");

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
    let refused = ask(&dev, "Rye?");
    assert_eq!(refused.status, 503, "{refused:?}");
    assert_eq!(refused.json()["error"]["code"], "assistant_unavailable");
    assert_eq!(provider.requests().len(), 1, "nothing sent over the limit");

    let usage = jadpo(&root, &["ai", "usage", "--json"]);
    assert_eq!(usage.code, 0, "{}{}", usage.stdout, usage.stderr);
    let data: serde_json::Value = serde_json::from_str(&usage.stdout).expect("JSON");
    // The provider's own count: 30 in, 9 out, at 0.40 and 1.60 a million.
    assert_eq!(data["data"]["today"]["tokens"], 39, "{data:#}");
    assert_eq!(data["data"]["today"]["calls"], 1);
    assert_eq!(data["data"]["today"]["limit"], 2_000_000);
    assert_eq!(data["data"]["today"]["cost"], "USD 0.01");
    assert_eq!(
        data["data"]["prompts"],
        serde_json::json!([{
            "prompt": "answer_question",
            "model": "acme/acme-small",
            "calls": 1,
            "tokensToday": 39,
            "tokensMonth": 39,
            "inputTokens": 30,
            "outputTokens": 9,
            "cachedTokens": 0,
            "cost": "USD 0.01"
        }])
    );
    let shown = jadpo(&root, &["ai", "usage"]);
    assert_eq!(shown.code, 0, "{}{}", shown.stdout, shown.stderr);
    for expected in [
        "answer_question",
        "acme/acme-small",
        "39 of 2000000 tokens",
        "USD 0.01",
        "39 tokens today",
    ] {
        assert!(
            shown.stdout.contains(expected),
            "{expected}: {}",
            shown.stdout
        );
    }
    drop(dev);
}

/// A price the application sets for its model overrides its package's
/// (area 21, question 7; chunk 5.5v7): the package's table prices the
/// model at nothing, and the app's `Money` settings at 100, 10 and 400 US
/// dollars a million input, cached and output tokens, so the third call
/// of the day would pass 0.25.
#[test]
fn a_price_the_app_sets_overrides_its_packages() {
    let root = desk(
        "budget-prices",
        "https://sandbox.acme-models.example",
        STRICT,
        "",
    );
    actions(&root);
    let long = long_answer();
    write(
        &root,
        crate::ai_providers::LIBRARY,
        &crate::ai_providers::library("0.00", "0.00", "0.00"),
    );
    let config = fs::read_to_string(root.join("config.jadpo")).expect("config");
    write(
        &root,
        "config.jadpo",
        &config.replacen(
            "config {\n",
            "config {\n    ai_daily_spend: Money {\n        default: \"USD 0.25\"\n    }\n    input_price: Money {\n        default: \"USD 100.00\"\n    }\n    cached_price: Money {\n        default: \"USD 10.00\"\n    }\n    output_price: Money {\n        default: \"USD 400.00\"\n    }\n",
            1,
        ),
    );
    passes(
        &root,
        "        limits: {\n            per_day: {\n                tokens: 2000000\n                cost: config.ai_daily_spend\n            }\n        }\n        prices: {\n            default: {\n                input: config.input_price\n                cached: config.cached_price\n                output: config.output_price\n            }\n        }\n",
        &format!(
            "test \"the day's spend runs out at the app's own prices\" {{\n    fakes: {{\n        ai.answer_question: [{long}, {long}]\n    }}\n    var first: SupportAnswer = attempt answer_once(Asked {{ question: \"Sesame?\" }})\n    var second: SupportAnswer = attempt answer_once(Asked {{ question: \"Rye?\" }})\n    assert second.reply == first.reply\n    assert answer_once(Asked {{ question: \"Spelt?\" }}) fails AssistantUnavailable\n}}\n"
        ),
        1,
    );
    // A price for a model `models:` doesn't name, and one that isn't
    // money, are each the app's mistake.
    limited(
        &root,
        "        limits: {\n            per_day: {\n                tokens: 2000000\n            }\n        }\n        prices: {\n            drafting: {\n                input: config.input_price\n                output: config.ai_daily_spend\n            }\n            default: {\n                input: config.drafting_model\n                output: config.output_price\n            }\n        }\n",
    );
    jadpo(&root, &["records", "update"]);
    let checked = jadpo(&root, &["check"]);
    let said = format!("{}{}", checked.stdout, checked.stderr);
    assert_eq!(checked.code, 1, "{said}");
    assert!(
        said.contains("error: `drafting` isn't a model `models:` names"),
        "{said}"
    );
    assert!(
        said.contains("error: `input:` must be an amount of money"),
        "{said}"
    );
}
