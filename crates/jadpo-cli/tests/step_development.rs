//! Several changing calls in development, and the metric and alert on
//! unfinished records (area 12, "Several changing calls in one action";
//! area 11, "Personal data"; area 17, "The request timeline"; chunk
//! 7.5f), live against PostgreSQL, on 7.5b's billing project
//! (`step_records.rs`): a route's response and a delivery's attempt carry
//! what became of each step they reached, never a request, an answer or a
//! key, which `jadpo dev` shows under their lines; the tick carries the
//! number of unfinished records and when the oldest started, and raises
//! the alert once one has been unfinished a day, once until none is; and
//! `jadpo deliveries` shows that alert beside the items.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use jadpo_cli::{Output, run};
use jadpo_host_native::Runtime;
use jadpo_runtime::call_log::{CallLog, DeliveryAttempt, Seen};
use jadpo_runtime::capability::Source;
use jadpo_runtime::runner::run_deferred;
use jadpo_runtime::schedules::tick;
use jadpo_runtime::schedules::watch::AlertKind;
use jadpo_runtime::steps::{RunSteps, Shown, StepState};
use jadpo_runtime::unfinished::Metric;
use jadpo_term::Environment;
use jadpo_test::host::Harness;
use jadpo_value::Value;

use crate::step_records::{
    CUSTOMER, HOUR, SECOND, SUBSCRIPTION, T0, TRIES, call, chosen, delivery, faked, field, record,
};
use crate::support::fake_provider::FakeProvider;

const DAY: i64 = 24 * HOUR;

/// The billing project, checked and migrated, with a test database;
/// `None` when live tests are skipped.
fn load(name: &str, sandbox: &str) -> Option<crate::step_records::Loaded> {
    let loaded = crate::step_records::load(name, sandbox, false);
    if !jadpo_test::live() {
        return None;
    }
    loaded
}

/// What the development call log heard of each delivery with steps.
#[derive(Default)]
struct Heard(RefCell<Vec<(String, String, Vec<RunSteps>)>>);

impl CallLog for Heard {
    fn seen(&self, _: &Seen<'_>) {}

    fn attempt_ended(&self, attempt: &DeliveryAttempt<'_>) {
        self.0.borrow_mut().push((
            attempt.subscriber.to_string(),
            attempt.state.to_string(),
            attempt.steps.to_vec(),
        ));
    }
}

fn step(position: u32, operation: &str, state: StepState, source: Option<Source>) -> Shown {
    Shown {
        position,
        operation: operation.to_string(),
        state,
        source,
    }
}

/// What no developer's or operator's screen may show: an answer, a
/// request's value or a key.
fn nothing_secret(text: &str) {
    for secret in ["cus_4821", "sub_9930", "ada", "gold", "k-ada-gold"] {
        assert!(!text.contains(secret), "`{secret}` shown:\n{text}");
    }
}

/// `jadpo <args>` against `url` at `at`, as a terminal with colour off
/// shows it, bold taken out.
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

/// A delivery whose step 3 is in doubt is heard with its three steps,
/// retrying; its retry is heard with steps 1 and 2 given back from the
/// record and step 3 answered.
#[test]
fn a_deliverys_attempts_are_heard_with_what_became_of_each_step() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("dev-delivery", &provider.url) else {
        return;
    };
    let heard = Rc::new(Heard::default());
    let harness = Harness::postgres(&loaded.url)
        .at(T0)
        .random_from(0)
        .with_call_log(Rc::clone(&heard) as Rc<dyn CallLog>);
    let (host, _sheets) = faked(&harness);
    let program = &loaded.program;
    provider.answer(200, CUSTOMER);
    for _ in 0..TRIES {
        provider.answer(503, "");
    }
    provider.answer(200, SUBSCRIPTION);
    let rt = Runtime::new().expect("runtime");
    let (_, work) = rt.block_on(chosen(&harness, &host, program, "gold"));
    rt.block_on(run_deferred(program, &host, work));
    assert_eq!(rt.block_on(delivery(&harness)).0, "pending");
    let later = T0 + HOUR;
    harness.set_now(later);
    rt.block_on(tick(program, &host, later));
    assert_eq!(rt.block_on(delivery(&harness)).0, "succeeded");

    let heard = heard.0.borrow();
    let customers = "pay.customers.create";
    let put = "sheets.put";
    let subscriptions = "pay.subscriptions.create";
    assert_eq!(heard.len(), 2, "{heard:?}");
    assert_eq!(heard[0].0, "start_subscription");
    assert_eq!(heard[0].1, "pending");
    assert_eq!(
        heard[0].2,
        [RunSteps {
            action: "subscribe".into(),
            steps: vec![
                step(1, customers, StepState::Answered, Some(Source::Live)),
                step(2, put, StepState::Answered, Some(Source::Fake)),
                step(3, subscriptions, StepState::InDoubt, Some(Source::Live)),
            ],
        }]
    );
    assert_eq!(heard[1].1, "succeeded");
    assert_eq!(
        heard[1].2,
        [RunSteps {
            action: "subscribe".into(),
            steps: vec![
                step(1, customers, StepState::Replayed, Some(Source::Replayed)),
                step(2, put, StepState::Replayed, Some(Source::Replayed)),
                step(3, subscriptions, StepState::Answered, Some(Source::Live)),
            ],
        }]
    );
    nothing_secret(&format!("{heard:?}"));
}

/// A route's response carries its steps, each call it made as a step
/// marked with its place: the first request ends with step 3 in doubt;
/// the client's retry under its key gives steps 1 and 2 back and answers
/// step 3.
#[test]
fn a_routes_response_carries_what_became_of_each_step() {
    use jadpo_runtime::http::{Request, Worker, serve_request};
    let provider = FakeProvider::start();
    let Some(loaded) = load("dev-route", &provider.url) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let (host, _sheets) = faked(&harness);
    let program = &loaded.program;
    provider.answer(200, CUSTOMER);
    for _ in 0..TRIES {
        provider.answer(503, "");
    }
    provider.answer(200, SUBSCRIPTION);
    let app = program.app();
    let worker = Worker::default();
    Runtime::new().expect("runtime").block_on(async {
        let (offer, _) = call(
            &harness,
            &host,
            program,
            "add_offer",
            vec![Value::text("gold")],
        )
        .await;
        let Value::Uuid(offer_id) = field(&offer, "id").clone() else {
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
        let first = serve_request(&app, program, &host, &worker, &request).await;
        assert_eq!(first.status, 503);
        let states = |runs: &[RunSteps]| -> Vec<(u32, StepState)> {
            runs.iter()
                .flat_map(|run| run.steps.iter())
                .map(|shown| (shown.position, shown.state.clone()))
                .collect()
        };
        assert_eq!(first.steps.len(), 1);
        assert_eq!(first.steps[0].action, "subscribe_now");
        assert_eq!(
            states(&first.steps),
            [
                (1, StepState::Answered),
                (2, StepState::Answered),
                (3, StepState::InDoubt),
            ]
        );
        // Every call of a step says which: step 3 went out three times.
        let places: Vec<Option<u32>> = first.calls.iter().map(|call| call.step).collect();
        assert_eq!(places, [Some(1), Some(2), Some(3), Some(3), Some(3)]);

        harness.set_now(T0 + 60 * SECOND);
        let again = serve_request(&app, program, &host, &worker, &request).await;
        assert_eq!(again.status, 204);
        assert_eq!(
            states(&again.steps),
            [
                (1, StepState::Replayed),
                (2, StepState::Replayed),
                (3, StepState::Answered),
            ]
        );
        let replayed: Vec<(Option<u32>, Source)> = again
            .calls
            .iter()
            .map(|call| (call.step, call.source))
            .collect();
        assert_eq!(
            replayed,
            [
                (Some(1), Source::Replayed),
                (Some(2), Source::Replayed),
                (Some(3), Source::Live),
            ]
        );
        nothing_secret(&format!("{:?} {:?}", first.steps, again.steps));
    });
}

/// The tick carries the metric on every store that keeps records; once
/// the record has been unfinished a day it raises the alert, once, and
/// `jadpo deliveries` shows it beside the item; abandoned, the record is
/// no longer counted.
#[test]
fn the_tick_raises_the_alert_on_unfinished_records_once() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("dev-alert", &provider.url) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let (host, _sheets) = faked(&harness);
    let program = &loaded.program;
    let (root, url) = (loaded.root.as_path(), loaded.url.as_str());
    provider.answer(200, CUSTOMER);
    for _ in 0..TRIES * 3 {
        provider.answer(503, "");
    }
    let rt = Runtime::new().expect("runtime");
    let (_, work) = rt.block_on(chosen(&harness, &host, program, "gold"));
    rt.block_on(run_deferred(program, &host, work));

    let unfinished_alerts = |ticked: &jadpo_runtime::schedules::Ticked| {
        ticked
            .raised
            .iter()
            .filter(|alert| matches!(alert.kind, AlertKind::UnfinishedSteps { .. }))
            .cloned()
            .collect::<Vec<_>>()
    };
    let soon = T0 + HOUR;
    harness.set_now(soon);
    let ticked = rt.block_on(tick(program, &host, soon));
    assert_eq!(
        ticked.unfinished,
        [(
            "main".to_string(),
            Metric {
                count: 1,
                oldest: Some(T0),
            }
        )]
    );
    assert!(unfinished_alerts(&ticked).is_empty(), "{:?}", ticked.raised);
    let quiet = operating(root, url, soon, &["deliveries"]);
    assert!(!quiet.stdout.contains("multi-step"), "{}", quiet.stdout);

    // A day on, its step in doubt past the provider's window, it parks
    // and the alert is raised, once.
    let late = T0 + DAY + SECOND;
    harness.set_now(late);
    let ticked = rt.block_on(tick(program, &host, late));
    let raised = unfinished_alerts(&ticked);
    assert_eq!(raised.len(), 1, "{:?}", ticked.raised);
    assert_eq!(
        raised[0].kind,
        AlertKind::UnfinishedSteps {
            store: "main".into(),
            count: 1,
            oldest: T0,
        }
    );
    assert_eq!(
        raised[0].event(),
        serde_json::json!({
            "event": "step.alert",
            "alert": "unfinished_steps",
            "store": "main",
            "unfinished": 1,
            "oldest": "2026-10-20T09:00:00Z",
        })
    );
    let next = late + 60 * SECOND;
    harness.set_now(next);
    let ticked = rt.block_on(tick(program, &host, next));
    assert!(unfinished_alerts(&ticked).is_empty(), "{:?}", ticked.raised);
    assert_eq!(ticked.unfinished[0].1.count, 1);

    let listed = operating(root, url, next, &["deliveries"]);
    assert_eq!(listed.code, 0, "{}{}", listed.stdout, listed.stderr);
    assert!(
        listed
            .stdout
            .contains("1 multi-step run has been unfinished since 2026-10-20 09:00"),
        "{}",
        listed.stdout
    );
    nothing_secret(&listed.stdout);
    let json = operating(root, url, next, &["deliveries", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&json.stdout).expect("JSON");
    let metric = &document["data"]["unfinished"][0];
    assert_eq!(metric["store"], "main");
    assert_eq!(metric["count"], 1);
    assert_eq!(metric["alert"], true);

    // Abandoned, it is no longer unfinished. Its receipt, put a day ago
    // and held by no record, is being deleted by a delivery of its own
    // (chunk B3), which isn't this one.
    let rows = rt.block_on(harness.rows(
        "SELECT replace(\"id\"::text, '-', '') FROM \"jadpo_deliveries\" WHERE \"subscriber\" <> 'files sheets'",
    ));
    let id = match rows.as_slice() {
        [row] => match row.as_slice() {
            [jadpo_runtime::SqlValue::Text(id)] => id[24..].to_string(),
            other => panic!("{other:?}"),
        },
        other => panic!("one delivery: {other:?}"),
    };
    let abandoned = operating(root, url, next, &["deliveries", "abandon", &id]);
    assert_eq!(
        abandoned.code, 0,
        "{}{}",
        abandoned.stdout, abandoned.stderr
    );
    assert!(rt.block_on(record(&harness)).is_some());
    let after = next + 60 * SECOND;
    harness.set_now(after);
    let ticked = rt.block_on(tick(program, &host, after));
    assert_eq!(ticked.unfinished[0].1, Metric::default());
    let cleared = operating(root, url, after, &["deliveries"]);
    assert!(!cleared.stdout.contains("multi-step"), "{}", cleared.stdout);
}
