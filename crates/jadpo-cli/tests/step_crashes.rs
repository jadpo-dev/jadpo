//! Crash points in `jadpo test` (area 13, "Provider calls and capabilities
//! in tests"; area 11, "Tests and tools"; chunk 7.5d), live against
//! PostgreSQL: the billing project of chunk 7.5b, whose subscriber opens a
//! customer at a payment provider, keeps a receipt and starts their
//! subscription, three changing calls, then keeps the account.
//!
//! A test that reaches it runs again with a crash at each boundary of each
//! step it reached (before the step goes out, once it is answered and
//! before its answer is kept, once its answer is kept), and must pass
//! every time: the delivery is given back and runs again from its record,
//! the fake answers a step sent again under its key with its first answer,
//! and the provider acts once. A route's request is sent again under its
//! key. A step whose request is made afresh on each run stops as
//! `diverged` under a crash, and the report names the crash.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::Output;

use crate::step_records::{T0, files, jadpo};

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

/// The tests: a plan chosen through an action, and one through the
/// partners' route, which requires `Idempotency-Key`.
const TESTS: &[(&str, &str)] = &[
    (
        "domains/billing/actions/choose.test.jadpo",
        r#"use billing/actions { add_offer }
use billing/entities { Offer }

test "a chosen plan starts one subscription" {
    var offer: Offer = add_offer("gold")
    choose("ada", offer.id)
    var counted: Offer = attempt Offer.get(offer.id)
    assert counted.subscribers == 1
}
"#,
    ),
    (
        "domains/billing/routes/partners_subscriptions.test.jadpo",
        r#"use billing/actions { add_offer }
use billing/entities { Offer }
use billing/types { SubscriptionChoice }

test "a partner starts a subscription straight away" {
    var offer: Offer = add_offer("gold")
    var started: Response = request {
        method: HttpMethod.post
        path: "/partners/subscriptions"
        input: SubscriptionChoice {
            customer: "ada"
            offer_id: offer.id
        }
    }
    assert started.status == 204
    var counted: Offer = attempt Offer.get(offer.id)
    assert counted.subscribers == 1
}
"#,
    ),
];

/// The billing project with its tests, migrated and its records written;
/// `fresh` makes the customer's reference afresh on every run.
fn project(name: &str, fresh: bool) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("step-crashes-7.5d-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files("https://sandbox.acme-pay.example", false) {
        // The tests read the sandbox, as a project's tests do.
        let text = text.replace(
            "pay_environment: ProviderEnvironment\n",
            "pay_environment: ProviderEnvironment {\n        default: ProviderEnvironment.sandbox\n    }\n",
        );
        let text = if fresh {
            text.replace(
                "    reference: customer\n",
                "    reference: random.token()\n",
            )
            .replace(
                "public type CustomerRequest = Object {\n    reference: Reference\n",
                "public type CustomerRequest = Object {\n    reference: Text\n",
            )
        } else {
            text
        };
        write(&root, path, &text);
    }
    for (path, text) in TESTS {
        write(&root, path, text);
    }
    for args in [&["migrate", "new", "initial"][..], &["records", "update"]] {
        let out = jadpo(&root, T0, args);
        assert_eq!(out.code, 0, "{args:?}: {}{}", out.stdout, out.stderr);
    }
    root
}

fn test_named(root: &Path, name: &str, json: bool) -> Output {
    let mut args = vec!["test", "--name", name, "--seed", "1"];
    if json {
        args.push("--json");
    }
    jadpo(root, T0, &args)
}

#[test]
fn a_test_passes_with_a_crash_at_every_step() {
    let root = project("passes", false);
    if !jadpo_test::live() {
        return;
    }
    for name in ["a chosen plan", "a partner starts"] {
        let tested = test_named(&root, name, false);
        assert_eq!(tested.code, 0, "{name}: {}{}", tested.stdout, tested.stderr);
        assert!(
            tested.stdout.contains("1 passed"),
            "{name}: {}",
            tested.stdout
        );
    }
}

#[test]
fn a_step_made_afresh_on_each_run_fails_under_a_crash() {
    let root = project("fresh", true);
    if !jadpo_test::live() {
        return;
    }
    let tested = test_named(&root, "a chosen plan", false);
    assert_ne!(tested.code, 0, "{}", tested.stdout);
    for line in [
        "FAIL  a chosen plan starts one subscription",
        "crash: before step 1 of `subscribe`, `pay.customers.create`, went out; then it ran again",
        "delivery: `PlanChosen` to subscriber `start_subscription` stopped as diverged on attempt 1 of 10",
        "`reference` differs from the record",
        "rule: test.subscriber_failed",
        "rerun: jadpo test domains/billing/actions/choose.test.jadpo:",
    ] {
        assert!(tested.stdout.contains(line), "{line}\n{}", tested.stdout);
    }
    assert!(
        !tested.stdout.contains("ada"),
        "no request value is shown: {}",
        tested.stdout
    );
    let json = test_named(&root, "a chosen plan", true);
    let document: serde_json::Value = serde_json::from_str(&json.stdout).expect("json");
    let failure = &document["data"]["tests"][0]["failure"];
    assert_eq!(failure["code"], "test.subscriber_failed");
    assert_eq!(
        failure["crash"],
        serde_json::json!({
            "action": "subscribe",
            "step": 1,
            "operation": "pay.customers.create",
            "boundary": "before_send"
        })
    );
    assert_eq!(failure["delivery"]["diverged"], true);
    assert_eq!(failure["fault"]["kind"], "diverged");
}
