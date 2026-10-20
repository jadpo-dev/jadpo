//! Unfinished records of several changing calls as an operator's items
//! (area 12, "What an operator sees"; area 11, "Ending"; chunk 7.5c),
//! live against PostgreSQL, on 7.5b's billing project (`step_records.rs`):
//! `jadpo deliveries show` lists each step, never a request or an answer;
//! `retry --not-done` sends the step in doubt, or the one that failed,
//! again under its next key generation; a delivery that diverged is listed
//! and retried once fixed; `abandon` ends one for good and keeps its step
//! list 30 days; a route's record whose client didn't retry within a day
//! is an item of its own; the provider's `new_key_on:` answer sends the
//! step again under its next key; and `jadpo runs unfinished --build`
//! lists what a new build changes.

use std::path::Path;

use jadpo_cli::{Output, run};
use jadpo_host_native::Runtime;
use jadpo_runtime::SqlValue;
use jadpo_runtime::runner::run_deferred;
use jadpo_runtime::schedules::tick;
use jadpo_term::Environment;
use jadpo_test::host::Harness;

use crate::step_records::{
    CUSTOMER, CUSTOMERS, HOUR, SECOND, SUBSCRIPTION, SUBSCRIPTIONS, T0, TRIES, chosen, delivery,
    faked, jadpo, record, sent, steps,
};
use crate::support::fake_provider::FakeProvider;

const DAY: i64 = 24 * HOUR;

/// The billing project, checked and migrated, with a test database;
/// `None` when live tests are skipped.
fn load(name: &str, sandbox: &str, replays: bool) -> Option<crate::step_records::Loaded> {
    let loaded = crate::step_records::load(name, sandbox, replays);
    if !jadpo_test::live() {
        return None;
    }
    loaded
}

/// `jadpo <args>` against `url` as the operator `sam` at `at`, as a
/// terminal with colour off shows it, bold taken out.
fn operating(root: &Path, url: &str, at: i64, args: &[&str]) -> Output {
    let mut args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    args.extend(["--url".to_string(), url.to_string()]);
    let env = Environment::empty()
        .with_var("SOURCE_DATE_EPOCH", &(at / 1000).to_string())
        .with_var("USER", "sam")
        .with_terminals(true, true, true)
        .with_var("TERM", "xterm-256color")
        .with_var("NO_COLOR", "1");
    let mut output = run(&args, &env.with_cwd(root));
    output.stdout = output.stdout.replace("\x1b[1m", "").replace("\x1b[0m", "");
    output.stderr = output.stderr.replace("\x1b[1m", "").replace("\x1b[0m", "");
    output
}

/// The one delivery's id as the list shows it: its last 8 hex digits.
async fn delivery_id(harness: &Harness<jadpo_host_native::Postgres>) -> String {
    let rows = harness
        .rows("SELECT replace(\"id\"::text, '-', '') FROM \"jadpo_deliveries\"")
        .await;
    match rows.as_slice() {
        [row] => match row.as_slice() {
            [SqlValue::Text(id)] => id[24..].to_string(),
            other => panic!("{other:?}"),
        },
        other => panic!("one delivery: {other:?}"),
    }
}

/// Each step's key generation, in order.
async fn generations(harness: &Harness<jadpo_host_native::Postgres>) -> Vec<i32> {
    harness
        .rows("SELECT \"key_generation\" FROM \"jadpo_steps\" ORDER BY \"position\"")
        .await
        .into_iter()
        .map(|row| match row.as_slice() {
            [SqlValue::Int4(n)] => *n,
            other => panic!("{other:?}"),
        })
        .collect()
}

/// What no operator's screen may show: an answer, a request's value or
/// a key.
fn nothing_secret(text: &str) {
    for secret in ["cus_4821", "sub_9930", "ada", "gold", "jd1_", "k-ada-gold"] {
        assert!(!text.contains(secret), "`{secret}` shown:\n{text}");
    }
}

/// Step 3 in doubt past its provider's window parks the delivery. `show`
/// lists its three steps and asks the operator to check step 3; `resolve
/// --done` is refused, since the action never committed, and `retry
/// --not-done` sends step 3 again under a new key: the delivery finishes,
/// steps 1 and 2 replayed.
#[test]
fn a_step_in_doubt_goes_again_under_a_new_key_once_the_operator_says_not_done() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("op-not-done", &provider.url, false) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let (host, _sheets) = faked(&harness);
    let program = &loaded.program;
    let (root, url) = (loaded.root.as_path(), loaded.url.as_str());
    provider.answer(200, CUSTOMER);
    for _ in 0..TRIES {
        provider.answer(503, "");
    }
    let rt = Runtime::new().expect("runtime");
    {
        let (_, work) = rt.block_on(chosen(&harness, &host, program, "gold"));
        rt.block_on(run_deferred(program, &host, work));
        let late = T0 + 23 * HOUR + SECOND;
        harness.set_now(late);
        rt.block_on(tick(program, &host, late));
        assert_eq!(rt.block_on(delivery(&harness)).0, "outcome_unknown");
        let id = rt.block_on(delivery_id(&harness));

        let listed = operating(root, url, late, &["deliveries"]);
        assert_eq!(listed.code, 0, "{}{}", listed.stdout, listed.stderr);
        assert!(listed.stdout.contains(&id), "{}", listed.stdout);
        assert!(
            listed.stdout.contains("◆ outcome unknown"),
            "{}",
            listed.stdout
        );
        assert!(
            listed
                .stdout
                .contains(&format!("→ jadpo deliveries show {id}")),
            "{}",
            listed.stdout
        );

        let shown = operating(root, url, late, &["deliveries", "show", &id]);
        assert_eq!(shown.code, 0, "{}{}", shown.stdout, shown.stderr);
        let text = &shown.stdout;
        for line in [
            "  item      PlanChosen → start_subscription\n",
            "  action    subscribe\n",
            "  state     outcome unknown · 2 attempts\n",
        ] {
            assert!(text.contains(line), "{line:?} in\n{text}");
        }
        assert!(text.contains("pay.customers.create"), "{text}");
        assert!(text.contains("sheets.put"), "{text}");
        assert!(text.contains("pay.subscriptions.create"), "{text}");
        assert_eq!(text.matches("✓ answered").count(), 2, "{text}");
        assert!(text.contains("◆ no answer"), "{text}");
        assert!(
            text.contains(
                "◆ decision  Check whether step 3, `pay.subscriptions.create`, took effect at the provider"
            ),
            "{text}"
        );
        assert!(
            text.contains(&format!("jadpo deliveries retry {id} --not-done")),
            "{text}"
        );
        nothing_secret(text);
        let json = operating(root, url, late, &["deliveries", "show", &id, "--json"]);
        let document: serde_json::Value = serde_json::from_str(&json.stdout).expect("JSON");
        let steps_json = document["data"]["record"]["steps"]
            .as_array()
            .or_else(|| document["record"]["steps"].as_array())
            .expect("the steps")
            .clone();
        assert_eq!(steps_json.len(), 3);
        assert_eq!(steps_json[2]["outcome"], serde_json::Value::Null);
        nothing_secret(&json.stdout);

        // Done would leave the action's writes undone: abandon it instead.
        let done = operating(root, url, late, &["deliveries", "resolve", &id, "--done"]);
        assert_eq!(done.code, 1, "{}{}", done.stdout, done.stderr);
        assert!(
            done.stderr.contains("rule      deliveries.wrong_state"),
            "{}",
            done.stderr
        );
        assert!(
            done.stderr
                .contains(&format!("`jadpo deliveries abandon {id}`")),
            "{}",
            done.stderr
        );

        let retried = operating(root, url, late, &["deliveries", "retry", &id, "--not-done"]);
        assert_eq!(retried.code, 0, "{}{}", retried.stdout, retried.stderr);
        assert!(
            retried.stdout.contains(&format!(
                "✓ Queued `{id}` for retry: step 3, `pay.subscriptions.create`, goes again under a new key on the app's next tick"
            )),
            "{}",
            retried.stdout
        );
        let (status, attempts, _) = rt.block_on(delivery(&harness));
        assert_eq!((status.as_str(), attempts), ("pending", 0));
        assert_eq!(rt.block_on(generations(&harness)), [0, 0, 1]);

        provider.answer(200, SUBSCRIPTION);
        let next = late + 60 * SECOND;
        harness.set_now(next);
        rt.block_on(tick(program, &host, next));
        assert_eq!(rt.block_on(delivery(&harness)).0, "succeeded");
        assert_eq!(sent(&provider, CUSTOMERS).len(), 1, "step 1 replayed");
        let keys = sent(&provider, SUBSCRIPTIONS);
        assert_eq!(keys.len(), TRIES + 1);
        assert_ne!(keys[TRIES], keys[0], "sent again under a new key");
        assert_eq!(rt.block_on(record(&harness)), None);
    }
}

/// A step the provider answered with one of its failures fails the
/// delivery; retrying it as it is would replay that answer, so `show`
/// offers `--not-done`, which sends it again under a new key.
#[test]
fn a_step_that_failed_goes_again_under_a_new_key_with_not_done() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("op-failed", &provider.url, false) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let (host, _sheets) = faked(&harness);
    let program = &loaded.program;
    let (root, url) = (loaded.root.as_path(), loaded.url.as_str());
    provider.answer(200, CUSTOMER);
    provider.answer(402, r#"{"error": {"code": "card_declined"}}"#);
    let rt = Runtime::new().expect("runtime");
    {
        let (_, work) = rt.block_on(chosen(&harness, &host, program, "gold"));
        rt.block_on(run_deferred(program, &host, work));
        assert_eq!(rt.block_on(delivery(&harness)).0, "failed");
        let kept = rt.block_on(steps(&harness));
        assert_eq!(kept[2].3.as_deref(), Some("failed"));
        let id = rt.block_on(delivery_id(&harness));
        let at = T0 + HOUR;

        let shown = operating(root, url, at, &["deliveries", "show", &id]);
        assert_eq!(shown.code, 0, "{}{}", shown.stdout, shown.stderr);
        assert!(shown.stdout.contains("✗ failed"), "{}", shown.stdout);
        assert!(
            shown
                .stdout
                .contains(&format!("→ jadpo deliveries retry {id} --not-done")),
            "{}",
            shown.stdout
        );
        nothing_secret(&shown.stdout);

        // Dismissing would leave what took effect at the provider unseen.
        let dismissed = operating(root, url, at, &["deliveries", "dismiss", &id]);
        assert_eq!(dismissed.code, 1, "{}", dismissed.stderr);
        assert!(
            dismissed
                .stderr
                .contains("so it is abandoned, not dismissed"),
            "{}",
            dismissed.stderr
        );

        let retried = operating(root, url, at, &["deliveries", "retry", &id, "--not-done"]);
        assert_eq!(retried.code, 0, "{}{}", retried.stdout, retried.stderr);
        assert_eq!(rt.block_on(generations(&harness)), [0, 0, 1]);
        assert_eq!(
            rt.block_on(steps(&harness))[2].3,
            None,
            "its answer cleared"
        );
        provider.answer(200, SUBSCRIPTION);
        harness.set_now(at + SECOND);
        rt.block_on(tick(program, &host, at + SECOND));
        assert_eq!(rt.block_on(delivery(&harness)).0, "succeeded");
        let keys = sent(&provider, SUBSCRIPTIONS);
        assert_eq!(keys.len(), 2);
        assert_ne!(keys[0], keys[1]);
        assert_eq!(sent(&provider, CUSTOMERS).len(), 1);
    }
}

/// A delivery that diverged is listed for an operator and named in the
/// footer; `show` says which step differs; once a fix is in, `retry`
/// finishes it, its record's generation counted again from none as its
/// attempts are.
#[test]
fn a_diverged_delivery_is_listed_and_retried_once_fixed() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("op-diverged", &provider.url, false) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let (host, _sheets) = faked(&harness);
    let program = &loaded.program;
    let (root, url) = (loaded.root.as_path(), loaded.url.as_str());
    provider.answer(200, CUSTOMER);
    for _ in 0..TRIES {
        provider.answer(503, "");
    }
    let rt = Runtime::new().expect("runtime");
    {
        let (_, work) = rt.block_on(chosen(&harness, &host, program, "gold"));
        rt.block_on(run_deferred(program, &host, work));
        let change = |code: &str| format!("UPDATE \"offer\" SET \"code\" = '{code}'");
        rt.block_on(harness.rows(&change("platinum")));
        harness.set_now(T0 + HOUR);
        rt.block_on(tick(program, &host, T0 + HOUR));
        assert_eq!(rt.block_on(delivery(&harness)).0, "diverged");
        let id = rt.block_on(delivery_id(&harness));
        let at = T0 + 2 * HOUR;

        let listed = operating(root, url, at, &["deliveries"]);
        assert_eq!(listed.code, 0, "{}{}", listed.stdout, listed.stderr);
        assert!(listed.stdout.contains("◆ diverged"), "{}", listed.stdout);
        assert!(listed.stdout.contains("✗ 1 diverged"), "{}", listed.stdout);
        assert!(
            listed
                .stdout
                .contains(&format!("→ jadpo deliveries show {id}")),
            "{}",
            listed.stdout
        );
        let only = operating(root, url, at, &["deliveries", "--state", "diverged"]);
        assert!(only.stdout.contains(&id), "{}", only.stdout);

        let shown = operating(root, url, at, &["deliveries", "show", &id]);
        assert!(
            shown.stdout.contains(
                "  differs   step 3 of `subscribe`, `pay.subscriptions.create`: `plan` differs\n"
            ),
            "{}",
            shown.stdout
        );
        assert!(
            shown
                .stdout
                .contains(&format!("→ jadpo deliveries retry {id}")),
            "{}",
            shown.stdout
        );
        nothing_secret(&shown.stdout);

        rt.block_on(harness.rows(&change("gold")));
        let retried = operating(root, url, at, &["deliveries", "retry", &id]);
        assert_eq!(retried.code, 0, "{}{}", retried.stdout, retried.stderr);
        provider.answer(200, SUBSCRIPTION);
        harness.set_now(at + SECOND);
        rt.block_on(tick(program, &host, at + SECOND));
        assert_eq!(rt.block_on(delivery(&harness)).0, "succeeded");
        assert_eq!(rt.block_on(record(&harness)), None);
    }
}

/// `abandon` ends a delivery with several changing calls for good: it is
/// listed as abandoned and its steps still shown, and 30 days on the tick
/// deletes the delivery and its record.
#[test]
fn an_abandoned_delivery_keeps_its_step_list_for_thirty_days() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("op-abandon", &provider.url, false) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let (host, _sheets) = faked(&harness);
    let program = &loaded.program;
    let (root, url) = (loaded.root.as_path(), loaded.url.as_str());
    provider.answer(200, CUSTOMER);
    provider.answer(402, r#"{"error": {"code": "card_declined"}}"#);
    let rt = Runtime::new().expect("runtime");
    {
        let (_, work) = rt.block_on(chosen(&harness, &host, program, "gold"));
        rt.block_on(run_deferred(program, &host, work));
        let id = rt.block_on(delivery_id(&harness));
        let at = T0 + HOUR;
        let abandoned = operating(root, url, at, &["deliveries", "abandon", &id]);
        assert_eq!(
            abandoned.code, 0,
            "{}{}",
            abandoned.stdout, abandoned.stderr
        );
        assert!(
            abandoned.stdout.contains(&format!(
                "✓ Abandoned `{id}`: `subscribe` won't finish, and its steps are listed until 2026-11-19"
            )),
            "{}",
            abandoned.stdout
        );
        assert!(
            abandoned.stdout.contains("logged  sam · role"),
            "{}",
            abandoned.stdout
        );
        assert_eq!(rt.block_on(delivery(&harness)).0, "abandoned");
        assert_eq!(
            rt.block_on(record(&harness))
                .map(|(state, _)| state)
                .as_deref(),
            Some("abandoned")
        );
        let again = operating(root, url, at, &["deliveries", "retry", &id]);
        assert_eq!(again.code, 1, "{}", again.stderr);
        let listed = operating(root, url, at, &["deliveries", "--state", "abandoned"]);
        assert!(listed.stdout.contains(&id), "{}", listed.stdout);
        let shown = operating(root, url, at, &["deliveries", "show", &id]);
        assert!(
            shown.stdout.contains("pay.subscriptions.create"),
            "{}",
            shown.stdout
        );
        assert!(shown.stdout.contains("✗ failed"), "{}", shown.stdout);
        assert!(
            shown.stdout.contains("listed until 2026-11-19"),
            "{}",
            shown.stdout
        );

        let kept = at + 29 * DAY;
        harness.set_now(kept);
        rt.block_on(tick(program, &host, kept));
        assert!(rt.block_on(record(&harness)).is_some(), "kept for 30 days");
        let gone = at + 31 * DAY;
        harness.set_now(gone);
        let ticked = rt.block_on(tick(program, &host, gone));
        assert!(ticked.problems.is_empty(), "{:?}", ticked.problems);
        assert_eq!(rt.block_on(record(&harness)), None);
        assert_eq!(
            rt.block_on(harness.count("SELECT count(*) FROM \"jadpo_deliveries\"")),
            0
        );
    }
}

/// The provider's `new_key_on:` answer says the key can't be used again:
/// the step goes again at once under its next key generation, which its
/// record keeps, so a rerun sends under it too.
#[test]
fn the_providers_new_key_answer_sends_the_step_again_under_the_next_key() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("op-new-key", &provider.url, false) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let (host, _sheets) = faked(&harness);
    let program = &loaded.program;
    provider.answer(409, r#"{"error": {"code": "key_expired"}}"#);
    provider.answer(200, CUSTOMER);
    for _ in 0..TRIES {
        provider.answer(503, "");
    }
    provider.answer(200, SUBSCRIPTION);
    let rt = Runtime::new().expect("runtime");
    {
        let (_, work) = rt.block_on(chosen(&harness, &host, program, "gold"));
        rt.block_on(run_deferred(program, &host, work));
        let keys = sent(&provider, CUSTOMERS);
        assert_eq!(keys.len(), 2);
        assert_ne!(keys[0], keys[1], "sent again under the next key");
        assert_eq!(rt.block_on(generations(&harness)), [1, 0, 0]);
        harness.set_now(T0 + HOUR);
        rt.block_on(tick(program, &host, T0 + HOUR));
        assert_eq!(rt.block_on(delivery(&harness)).0, "succeeded");
        assert_eq!(sent(&provider, CUSTOMERS).len(), 2, "replayed");
    }
}

/// A route's request whose steps ended unfinished is its client's to
/// retry for a day; then it is an item, `unfinished`. After 30 days its
/// answers are dropped and only its step list is left, so it can't be
/// resumed; an operator abandons it, and a client's retry under its key
/// is then refused without sending anything. Its record goes 30 days
/// after it was abandoned.
#[test]
fn a_routes_steps_its_client_left_are_an_item_kept_thirty_days() {
    use jadpo_runtime::http::{Request, Worker, serve_request};
    let provider = FakeProvider::start();
    let Some(loaded) = load("op-route", &provider.url, false) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let (host, _sheets) = faked(&harness);
    let program = &loaded.program;
    let (root, url) = (loaded.root.as_path(), loaded.url.as_str());
    provider.answer(200, CUSTOMER);
    for _ in 0..TRIES {
        provider.answer(503, "");
    }
    let app = program.app();
    let worker = Worker::default();
    let rt = Runtime::new().expect("runtime");
    {
        let (offer, _) = rt.block_on(crate::step_records::call(
            &harness,
            &host,
            program,
            "add_offer",
            vec![jadpo_value::Value::text("gold")],
        ));
        let jadpo_value::Value::Uuid(offer_id) = crate::step_records::field(&offer, "id").clone()
        else {
            panic!("an id");
        };
        let request = Request {
            method: "POST".to_string(),
            path: "/partners/subscriptions".to_string(),
            headers: vec![
                ("content-type".to_string(), "application/json".to_string()),
                ("idempotency-key".to_string(), "k-ada-gold".to_string()),
            ],
            body: serde_json::json!({ "customer": "ada", "offer_id": offer_id.as_ref() })
                .to_string()
                .into_bytes(),
            address: Some("step test".to_string()),
            ..Request::default()
        };
        let first = rt.block_on(serve_request(&app, program, &host, &worker, &request));
        assert_eq!(first.status, 503);

        let early = operating(root, url, T0 + HOUR, &["deliveries"]);
        assert_eq!(
            early.stdout, "✓ Nothing needs an operator\n",
            "the client's to retry"
        );

        let day = T0 + 25 * HOUR;
        let listed = operating(root, url, day, &["deliveries"]);
        assert_eq!(listed.code, 0, "{}{}", listed.stdout, listed.stderr);
        assert!(
            listed.stdout.contains("request → subscribe_now"),
            "{}",
            listed.stdout
        );
        assert!(listed.stdout.contains("◆ unfinished"), "{}", listed.stdout);
        nothing_secret(&listed.stdout);
        let rows = rt.block_on(
            harness.rows("SELECT replace(\"id\"::text, '-', '') FROM \"jadpo_step_records\""),
        );
        let [row] = rows.as_slice() else {
            panic!("one record: {rows:?}");
        };
        let [SqlValue::Text(record_id)] = row.as_slice() else {
            panic!("{row:?}");
        };
        let id = record_id[24..].to_string();
        let shown = operating(root, url, day, &["deliveries", "show", &id]);
        assert_eq!(shown.code, 0, "{}{}", shown.stdout, shown.stderr);
        assert!(
            shown
                .stdout
                .contains("Its client can still retry under the same key"),
            "{}",
            shown.stdout
        );
        nothing_secret(&shown.stdout);
        let retried = operating(root, url, day, &["deliveries", "retry", &id]);
        assert_eq!(retried.code, 1, "{}", retried.stderr);
        assert!(
            retried
                .stderr
                .contains("is a route's steps, which only its client can retry"),
            "{}",
            retried.stderr
        );

        // Thirty days on, only the step list is left.
        let month = T0 + 31 * DAY;
        harness.set_now(month);
        let ticked = rt.block_on(tick(program, &host, month));
        assert!(ticked.problems.is_empty(), "{:?}", ticked.problems);
        assert!(ticked.deleted.answers >= 2, "{:?}", ticked.deleted);
        let kept = rt.block_on(steps(&harness));
        assert_eq!(kept.len(), 3, "the step list stays");
        assert!(kept.iter().all(|step| step.4.is_none()), "answers dropped");
        assert_eq!(kept[0].3.as_deref(), Some("answered"));
        let shown = operating(root, url, month, &["deliveries", "show", &id]);
        assert!(shown.stdout.contains("answer dropped"), "{}", shown.stdout);

        let abandoned = operating(root, url, month, &["deliveries", "abandon", &id]);
        assert_eq!(
            abandoned.code, 0,
            "{}{}",
            abandoned.stdout, abandoned.stderr
        );
        assert!(
            abandoned
                .stdout
                .contains(&format!("✓ Abandoned `{id}`: `subscribe_now` won't finish")),
            "{}",
            abandoned.stdout
        );
        let before = provider.requests().len();
        let again = rt.block_on(serve_request(&app, program, &host, &worker, &request));
        assert_eq!(
            again.status,
            500,
            "{}",
            String::from_utf8_lossy(&again.body)
        );
        assert_eq!(provider.requests().len(), before, "nothing sent");

        let gone = month + 31 * DAY;
        harness.set_now(gone);
        rt.block_on(tick(program, &host, gone));
        assert_eq!(rt.block_on(record(&harness)), None);
    }
}

/// Before a deploy, `jadpo runs unfinished --build` lists the unfinished
/// records the new build changes: none while the record was started under
/// the same build; one started under another whose step shape differs
/// stops as diverged; one whose action no longer keeps steps would send
/// them again. A build the checkout doesn't make is refused.
#[test]
fn runs_unfinished_lists_what_a_new_build_changes() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("op-runs", &provider.url, false) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let (host, _sheets) = faked(&harness);
    let program = &loaded.program;
    let (root, url) = (loaded.root.as_path(), loaded.url.as_str());
    let build = program.plan().build_id().to_string();
    provider.answer(200, CUSTOMER);
    for _ in 0..TRIES {
        provider.answer(503, "");
    }
    let rt = Runtime::new().expect("runtime");
    {
        let (_, work) = rt.block_on(chosen(&harness, &host, program, "gold"));
        rt.block_on(run_deferred(program, &host, work));
        let at = T0 + HOUR;
        let id = rt.block_on(delivery_id(&harness));

        let wrong = operating(
            root,
            url,
            at,
            &["runs", "unfinished", "--build", "0123abcd"],
        );
        assert_eq!(wrong.code, 2, "{}{}", wrong.stdout, wrong.stderr);
        assert!(
            wrong.stderr.contains(&format!(
                "The checkout makes build `{build}`, not `0123abcd`"
            )),
            "{}",
            wrong.stderr
        );
        assert!(
            wrong.stderr.contains("rule      cli.runs_arguments"),
            "{}",
            wrong.stderr
        );

        let same = operating(root, url, at, &["runs", "unfinished", "--build", &build]);
        assert_eq!(same.code, 0, "{}{}", same.stdout, same.stderr);
        assert_eq!(
            same.stdout,
            "✓ The build changes no unfinished run  1 unfinished run resumes\n"
        );

        rt.block_on(
            harness
                .rows("UPDATE \"jadpo_step_records\" SET \"build\" = 'older', \"shape\" = 'older'"),
        );
        let changed = operating(root, url, at, &["runs", "unfinished", "--build", &build]);
        assert_eq!(changed.code, 0, "{}{}", changed.stdout, changed.stderr);
        assert!(changed.stdout.contains(&id), "{}", changed.stdout);
        assert!(
            changed.stdout.contains("steps of subscribe"),
            "{}",
            changed.stdout
        );
        assert!(
            changed.stdout.contains("◆ stops as diverged"),
            "{}",
            changed.stdout
        );
        assert!(
            changed.stdout.contains("✗ 1 run changes  1 unfinished"),
            "{}",
            changed.stdout
        );
        assert!(
            changed
                .stdout
                .contains(&format!("→ jadpo deliveries show {id}")),
            "{}",
            changed.stdout
        );
        let json = operating(
            root,
            url,
            at,
            &["runs", "unfinished", "--build", &build, "--json"],
        );
        assert!(
            json.stdout.contains("\"will\":\"diverge\"")
                || json.stdout.contains("\"will\": \"diverge\""),
            "{}",
            json.stdout
        );

        rt.block_on(
            harness.rows("UPDATE \"jadpo_step_records\" SET \"action\" = 'action billing.gone'"),
        );
        let gone = operating(root, url, at, &["runs", "unfinished", "--build", &build]);
        assert!(
            gone.stdout.contains("✗ sends its steps again"),
            "{}",
            gone.stdout
        );
    }
}

/// Mistakes on the command line are refused before any database is read.
#[test]
fn mistakes_on_the_command_line_are_refused() {
    let provider = FakeProvider::start();
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("step-records-7.5b-op-arguments");
    // The project is written whether or not live tests run.
    let _ = load("op-arguments", &provider.url, false);
    for (args, headline, rule) in [
        (
            &["runs"][..],
            "`jadpo runs` needs `unfinished`",
            "cli.runs_arguments",
        ),
        (
            &["runs", "unfinshed"],
            "`unfinshed` isn't something `jadpo runs` does",
            "cli.runs_arguments",
        ),
        (
            &["runs", "unfinished"],
            "`jadpo runs unfinished` needs `--build <build id>`",
            "cli.runs_arguments",
        ),
        (
            &["deliveries", "dismiss", "4fa2c09e", "--not-done"],
            "`--not-done` is for `jadpo deliveries retry` or `resolve`, not `dismiss`",
            "cli.deliveries_arguments",
        ),
        (
            &["deliveries", "show", "4fa2c09e", "--done"],
            "`--done` is for `jadpo deliveries resolve`, not `show`",
            "cli.deliveries_arguments",
        ),
        (
            &["deliveries", "--state", "abandond"],
            "`abandond` isn't a state",
            "cli.deliveries_arguments",
        ),
    ] {
        let out = jadpo(&root, T0, args);
        assert_eq!(out.code, 2, "{args:?}: {}{}", out.stdout, out.stderr);
        assert!(out.stderr.contains(headline), "{args:?}: {}", out.stderr);
        assert!(out.stderr.contains(rule), "{args:?}: {}", out.stderr);
    }
}
