//! The record of an action with several changing calls (area 12,
//! "Several changing calls in one action"; area 11, the same; chunk
//! 7.5b), live against PostgreSQL: a subscriber opens a customer at a
//! payment provider started in the test, starts their subscription to the
//! offer they chose, then keeps the account, two keyed changing calls and
//! then the write.
//!
//! Each step is written before it is sent and its answer after, sealed;
//! a retry replays the answered steps without sending them and with the
//! first run's `clock.now`, sends a step in doubt again under its key
//! while the provider's `window:` lasts, and parks it after; a rerun whose
//! request changed, or that the provider says reused a key, stops as
//! `diverged` before sending anything; a runner whose lease another took
//! writes nothing more; and the record is deleted in the action's commit.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use jadpo_cli::{Output, run};
use jadpo_drivers::package::JsonHttp;
use jadpo_host_native::Runtime;
use jadpo_host_native::fetch::NativeFetch;
use jadpo_interp::{Prepared, Program};
use jadpo_runtime::capability::{Capability, Fake, Faked};
use jadpo_runtime::runner::{Deferred, run_deferred};
use jadpo_runtime::schedules::tick;
use jadpo_runtime::{Database, Executor, Host, Outcome, SqlValue};
use jadpo_term::Environment;
use jadpo_test::host::Harness;
use jadpo_value::Value;

use crate::support::fake_provider::FakeProvider;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

/// The billing project, its provider at `sandbox`, which replays a failed
/// answer under its key when `replays`.
pub(crate) fn files(sandbox: &str, replays: bool) -> Vec<(&'static str, String)> {
    let replays = if replays {
        "        replays_failures: true\n"
    } else {
        ""
    };
    vec![
        (
            "app.jadpo",
            r#"application {
    events: {
        store: main
    }
    steps: {
        encryption_key: config.step_encryption_key
    }
    connections: {
        pay: {
            package: AcmePay.v1
            domains: [billing]
            credential: config.pay_api_key
            environment: config.pay_environment
        }
        sheets: {
            driver: Driver.disk
            path: "/var/lib/billing/sheets"
        }
    }
    apis: {
        main: {
            prefix: /
        }
        partners: {
            prefix: /partners
            idempotency: Idempotency.required
        }
    }
}
"#
            .to_string(),
        ),
        (
            "config.jadpo",
            r#"config {
    pay_api_key: Text {
        secret: true
    }
    pay_environment: ProviderEnvironment
    step_encryption_key: SigningKey
}
"#
            .to_string(),
        ),
        (
            "shared/providers/acme_pay/v1/package.jadpo",
            "/// Acme Pay's payments API.\npackage AcmePay.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n"
                .to_string(),
        ),
        (
            "shared/providers/acme_pay/v1/provider.jadpo",
            format!(
                r#"/// Acme Pay's API.
provider {{
    protocol: Protocol.json_http
    hosts: {{
        live: "https://api.acme-pay.example"
        sandbox: "{sandbox}"
    }}
    credential: CredentialScheme.bearer
    idempotency: {{
        header: "Idempotency-Key"
        window: 24h
        max_length: 255
        in_use: "key_in_use"
        reused: "key_reused"
        new_key_on: "key_expired"
{replays}    }}
    error_code: "error.code"
}}
"#
            ),
        ),
        (
            "shared/providers/acme_pay/v1/resources/customers.jadpo",
            r#"public resource customers {
    create: {
        method: HttpMethod.post
        path: "/v1/customers"
        input: CustomerRequest
        output: Customer
        changes: true
        idempotent: true
        failures: [CardDeclined]
        source: PackageSource.written
    }
}
"#
            .to_string(),
        ),
        (
            "shared/providers/acme_pay/v1/resources/subscriptions.jadpo",
            r#"public resource subscriptions {
    create: {
        method: HttpMethod.post
        path: "/v1/subscriptions"
        input: SubscriptionRequest
        output: Subscription
        changes: true
        idempotent: true
        failures: [CardDeclined]
        source: PackageSource.written
    }
}
"#
            .to_string(),
        ),
        (
            "shared/providers/acme_pay/v1/types/reference.jadpo",
            "/// Acme Pay's reference for a record.\npublic type Reference = Text {\n    max_length: 40\n}\n"
                .to_string(),
        ),
        (
            "shared/providers/acme_pay/v1/types/customer_request.jadpo",
            "/// Who a customer is.\npublic type CustomerRequest = Object {\n    reference: Reference\n}\n"
                .to_string(),
        ),
        (
            "shared/providers/acme_pay/v1/types/customer.jadpo",
            "/// A customer Acme Pay keeps.\npublic type Customer = Object {\n    id: Reference\n}\n"
                .to_string(),
        ),
        (
            "shared/providers/acme_pay/v1/types/subscription_request.jadpo",
            "/// A customer's subscription to a plan.\npublic type SubscriptionRequest = Object {\n    customer: Reference\n    plan: Reference\n}\n"
                .to_string(),
        ),
        (
            "shared/providers/acme_pay/v1/types/subscription.jadpo",
            "/// A subscription Acme Pay keeps.\npublic type Subscription = Object {\n    id: Reference\n}\n"
                .to_string(),
        ),
        (
            "shared/providers/acme_pay/v1/failures/card_declined.jadpo",
            "/// The card was declined.\npublic type CardDeclined = Failure {\n    kind: FailureKind.rejected\n    message: \"The card was declined.\"\n}\n"
                .to_string(),
        ),
        (
            "domains/billing/failures/payment_refused.jadpo",
            "/// The payment provider refused the card.\ntype PaymentRefused = Failure {\n    kind: FailureKind.rejected\n    message: \"The card was refused.\"\n}\n"
                .to_string(),
        ),
        (
            "domains/billing/failures/receipt_not_kept.jadpo",
            "/// The sheets store had no room for a receipt.\ntype ReceiptNotKept = Failure {\n    kind: FailureKind.unavailable\n    message: \"There was no room to keep the receipt.\"\n}\n"
                .to_string(),
        ),
        (
            "domains/billing/types/subscription_choice.jadpo",
            r#"use billing/entities { Offer }
use shared/providers/acme_pay

/// What a partner's system sends to start a subscription.
type SubscriptionChoice = Input {
    customer: AcmePay.Reference
    offer_id: Offer.id
}
"#
            .to_string(),
        ),
        (
            "domains/billing/entities/offer.jadpo",
            r#"use shared/providers/acme_pay

/// A plan the shop offers, by the provider's code for it, and how many
/// customers subscribe to it.
entity Offer {
    id: Uuid
    code: AcmePay.Reference
    subscribers: Int

    policy: {
        create: [Access.public, Access.system]
        read: [Access.public, Access.system]
        update: [Access.public, Access.system]
    }
}
"#
            .to_string(),
        ),
        (
            "domains/billing/entities/account.jadpo",
            r#"use shared/providers/acme_pay

/// A customer's account, the subscription it pays for and the receipt
/// kept for it.
entity Account {
    id: Uuid
    customer: AcmePay.Reference
    subscription: AcmePay.Reference
    receipt: File {
        store: sheets
        max_size: 1mb
        types: [FileType.csv]
    }
    started_at: Instant

    policy: {
        create: [Access.public, Access.system]
        read: [Access.public, Access.system]
    }
}
"#
            .to_string(),
        ),
        (
            "domains/billing/events/plan_chosen.jadpo",
            r#"use billing/entities { Offer }
use shared/providers/acme_pay

/// A customer chose an offer.
type PlanChosen = Event {
    customer: AcmePay.Reference
    offer_id: Offer.id
}
"#
            .to_string(),
        ),
        (
            "domains/billing/actions/add_offer.jadpo",
            r#"use billing/entities { Offer }
use shared/providers/acme_pay

/// Adds an offer.
action add_offer(code: AcmePay.Reference) -> Offer {
    return Offer.create {
        set: {
            code: code
            subscribers: 0
        }
    }
}
"#
            .to_string(),
        ),
        (
            "domains/billing/actions/choose.jadpo",
            r#"use billing/entities { Offer }
use billing/events { PlanChosen }
use shared/providers/acme_pay

/// A customer chooses an offer.
action choose(customer: AcmePay.Reference, offer_id: Offer.id) -> Unit {
    emit PlanChosen {
        customer: customer
        offer_id: offer_id
    }
}
"#
            .to_string(),
        ),
        (
            "domains/billing/actions/start.jadpo",
            r#"use app { pay, sheets }
use billing/entities { Account, Offer, OfferNotFound }
use billing/failures { PaymentRefused, ReceiptNotKept }
use shared/providers/acme_pay

/// Opens the customer at Acme Pay, keeps a receipt, starts their
/// subscription to the offer, then keeps the account and counts the
/// subscriber: three changing calls, then the writes.
decline step.late_failure: "The card was checked when the customer was opened"
action start(customer: AcmePay.Reference, offer_id: Offer.id) fails OfferNotFound, PaymentRefused, ReceiptNotKept -> Account {
    var offer: Offer = attempt Offer.get(offer_id)
    var opened: AcmePay.Customer = attempt pay.customers.create(AcmePay.CustomerRequest {
        reference: customer
    }) {
        card_declined: PaymentRefused
    }
    var receipt: File = attempt sheets.put(encoding.to_utf8(opened.id), FileType.csv) {
        storage_full: ReceiptNotKept
    }
    var subscription: AcmePay.Subscription = attempt pay.subscriptions.create(AcmePay.SubscriptionRequest {
        customer: opened.id
        plan: offer.code
    }) {
        card_declined: PaymentRefused
    }
    var counted: Offer = Offer.update(offer) {
        set: {
            subscribers: offer.subscribers + 1
        }
    }
    assert counted.subscribers > 0
    return Account.create {
        set: {
            customer: opened.id
            subscription: subscription.id
            receipt: receipt
            started_at: clock.now
        }
    }
}
"#
            .to_string(),
        ),
        (
            "domains/billing/actions/subscribe.jadpo",
            r#"use billing/actions { start }
use billing/entities { Account, OfferNotFound }
use billing/events { PlanChosen }
use billing/failures { PaymentRefused, ReceiptNotKept }

/// Starts the subscription a customer chose.
action subscribe(event: PlanChosen) fails OfferNotFound, PaymentRefused, ReceiptNotKept -> Unit {
    var account: Account = attempt start(event.customer, event.offer_id)
    assert account.customer != ""
}
"#
            .to_string(),
        ),
        (
            "domains/billing/actions/subscribe_now.jadpo",
            r#"use billing/actions { start }
use billing/entities { Account, Offer, OfferNotFound }
use billing/failures { PaymentRefused, ReceiptNotKept }
use shared/providers/acme_pay

/// Starts a subscription a partner's system asks for.
action subscribe_now(customer: AcmePay.Reference, offer_id: Offer.id) fails OfferNotFound, PaymentRefused, ReceiptNotKept -> Unit {
    var account: Account = attempt start(customer, offer_id)
    assert account.customer != ""
}
"#
            .to_string(),
        ),
        (
            "domains/billing/routes/partners_subscriptions.jadpo",
            r#"use billing/actions { subscribe_now }
use billing/types { SubscriptionChoice }

/// What a partner's systems call.
routes PartnersSubscriptions {
    api: partners
    prefix: /partners/subscriptions

    /// Starts a subscription straight away.
    POST / {
        authentication: RouteAuthentication.none
        input: SubscriptionChoice
        status: SuccessStatus.no_content
        deadline: 60s
        run: subscribe_now(input.customer, input.offer_id)
    }
}
"#
            .to_string(),
        ),
        (
            "domains/billing/subscribers/start_subscription.jadpo",
            r#"use billing/actions { subscribe }
use billing/events { PlanChosen }

/// Starts the subscription a customer chose.
subscriber start_subscription {
    on: PlanChosen
    run: subscribe(event)
    retry: {
        attempts: 10
        within: 3d
    }
}
"#
            .to_string(),
        ),
    ]
}

/// When the test starts: 2026-10-20T09:00:00Z.
pub(crate) const T0: i64 = 1_792_486_800_000;
pub(crate) const SECOND: i64 = 1_000;
pub(crate) const HOUR: i64 = 60 * 60 * SECOND;
/// How many times a call that may have acted is sent in one delivery
/// attempt: the connection's default `retry:`, 3 attempts (area 11).
pub(crate) const TRIES: usize = 3;
/// The steps key, a `SigningKey` of more than 32 characters.
pub(crate) const STEPS_KEY: &str = "a-signing-key-for-the-record-of-steps-long-enough";

pub(crate) fn jadpo(cwd: &Path, at: i64, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty()
        .with_var("SOURCE_DATE_EPOCH", &(at / 1000).to_string())
        .with_var("NO_COLOR", "1");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    run(&args, &env.with_cwd(cwd))
}

pub(crate) struct Loaded {
    pub(crate) program: Program,
    _databases: jadpo_test::database::TestDatabases,
    pub(crate) url: String,
    pub(crate) root: PathBuf,
}

/// The project, checked and migrated, with a test database for it; `None`
/// when live tests are skipped.
pub(crate) fn load(name: &str, sandbox: &str, replays: bool) -> Option<Loaded> {
    load_calls(name, sandbox, replays, false)
}

fn load_calls(name: &str, sandbox: &str, replays: bool, single: bool) -> Option<Loaded> {
    load_mode(name, sandbox, replays, single, false)
}

fn load_mode(
    name: &str,
    sandbox: &str,
    replays: bool,
    single: bool,
    empty: bool,
) -> Option<Loaded> {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("step-records-7.5b-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, mut text) in files(sandbox, replays) {
        if single {
            match path {
                "app.jadpo" => text = text.replace("    steps: {\n        encryption_key: config.step_encryption_key\n    }\n", ""),
                "domains/billing/actions/add_offer.jadpo" => text = text.replace("subscribers: 0", "subscribers: 0\n            counted_at: none"),
                "domains/billing/entities/offer.jadpo" => text = text.replace("    subscribers: Int", "    subscribers: Int\n    counted_at: Instant?"),
                "config.jadpo" => text = text.replace("    step_encryption_key: SigningKey\n", ""),
                "domains/billing/actions/start.jadpo" => text = r#"use app { pay }
use billing/entities { Offer, OfferNotFound }
use billing/failures { PaymentRefused }
use shared/providers/acme_pay

action start(customer: AcmePay.Reference, offer_id: Offer.id) fails OfferNotFound, PaymentRefused -> AcmePay.Customer {
    var offer: Offer = attempt Offer.get(offer_id)
    var opened: AcmePay.Customer = attempt pay.customers.create(AcmePay.CustomerRequest {
        reference: customer
    }) {
        card_declined: PaymentRefused
    }
    var counted: Offer = Offer.update(offer) {
        set: { subscribers: offer.subscribers + 1
 counted_at: clock.now }
    }
    assert counted.subscribers > 0
    return opened
}
"#.into(),
                "domains/billing/actions/subscribe.jadpo" | "domains/billing/actions/subscribe_now.jadpo" => {
                    text = text.replace("var account: Account", "var account: AcmePay.Customer")
                        .replace("account.customer", "account.id")
                        .replace("Account, ", "")
                        .replace(", ReceiptNotKept", "");
                    if !text.contains("use shared/providers/acme_pay") {
                        text = format!("use shared/providers/acme_pay\n{text}");
                    }
                }
                "domains/billing/subscribers/start_subscription.jadpo" => text = text.replace("within: 3d", "within: 24h"),
                _ => {}
            }
        }
        if empty {
            if path == "shared/providers/acme_pay/v1/resources/customers.jadpo" {
                text = text.replace("        input: CustomerRequest\n", "");
            }
            text = text.replace("attempt pay.customers.create(AcmePay.CustomerRequest {\n        reference: customer\n    })", "attempt pay.customers.create()");
            if path == "domains/billing/actions/start.jadpo" {
                text = text.replace(
                    "    var offer: Offer",
                    "    assert customer != \"\"\n    var offer: Offer",
                );
            }
        }
        write(&root, path, &text);
    }
    for args in [
        &["migrate", "new", "initial"][..],
        &["records", "update"],
        &["check"],
    ] {
        let out = jadpo(&root, T0, args);
        assert_eq!(out.code, 0, "{args:?}: {}{}", out.stdout, out.stderr);
    }
    if !jadpo_test::live() {
        return None;
    }
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let prepared = Prepared::load(&loaded.db, loaded.project).unwrap_or_else(|problems| {
        panic!(
            "the project checks: {:?}",
            problems.iter().map(|p| &p.message).collect::<Vec<_>>()
        )
    });
    let variables = |name: &str| -> Option<String> {
        match name {
            "PAY_API_KEY" => Some("pk_test_123".into()),
            "PAY_ENVIRONMENT" => Some("sandbox".into()),
            "STEP_ENCRYPTION_KEY" => Some(STEPS_KEY.into()),
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
    let program = Program::new(prepared).with_config(&values);
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    Some(Loaded {
        program,
        _databases: databases,
        url,
        root,
    })
}

/// The package driver, over the native host's HTTP, for `pay`, and a
/// store of files that keeps nothing for `sheets`, which says what it
/// was sent.
pub(crate) fn faked(
    harness: &Harness<jadpo_host_native::Postgres>,
) -> (Faked<&Harness<jadpo_host_native::Postgres>>, Rc<Fake>) {
    let driver: Rc<dyn Capability> = Rc::new(JsonHttp::new(NativeFetch::new()));
    let sheets = Rc::new(Fake::new());
    let files: Rc<dyn Capability> = Rc::clone(&sheets) as Rc<dyn Capability>;
    (
        Faked::new(harness)
            .with("pay", driver)
            .with("sheets", files),
        sheets,
    )
}

/// Runs action `name` with `args` on `host`, giving its value and the
/// work it left for after its response.
pub(crate) async fn call(
    harness: &Harness<jadpo_host_native::Postgres>,
    host: &dyn Host,
    program: &Program,
    name: &str,
    args: Vec<Value>,
) -> (Value, Vec<Deferred>) {
    let id = program.plan().find(name).expect("the action");
    let mut ctx = harness.ctx_on(program, host);
    match program.run(&mut ctx, id, args).await {
        Outcome::Value(value) => (value, ctx.take_deferred()),
        other => panic!("{name}: {other:?}"),
    }
}

pub(crate) fn field<'v>(value: &'v Value, name: &str) -> &'v Value {
    match value {
        Value::Record(record) => record.get(name).expect("the field"),
        other => panic!("not a record: {other:?}"),
    }
}

/// An offer of plan `code`, and a customer choosing it: the delivery to
/// `start_subscription` it leaves.
pub(crate) async fn chosen(
    harness: &Harness<jadpo_host_native::Postgres>,
    host: &dyn Host,
    program: &Program,
    code: &str,
) -> (Value, Vec<Deferred>) {
    let (offer, _) = call(harness, host, program, "add_offer", vec![Value::text(code)]).await;
    let (_, work) = call(
        harness,
        host,
        program,
        "choose",
        vec![Value::text("ada"), field(&offer, "id").clone()],
    )
    .await;
    (offer, work)
}

/// The delivery's state and attempts, and its last failure as JSON text.
pub(crate) async fn delivery(
    harness: &Harness<jadpo_host_native::Postgres>,
) -> (String, i32, String) {
    let rows = harness
        .rows("SELECT \"status\", \"attempts\", COALESCE(\"last_failure\"::text, '') FROM \"jadpo_deliveries\"")
        .await;
    match rows.as_slice() {
        [row] => match row.as_slice() {
            [
                SqlValue::Text(status),
                SqlValue::Int4(attempts),
                SqlValue::Text(failure),
            ] => (status.clone(), *attempts, failure.clone()),
            other => panic!("not a delivery: {other:?}"),
        },
        other => panic!("one delivery: {other:?}"),
    }
}

/// Each recorded step: its position, operation, fingerprint as JSON
/// text, outcome, and its answer's bytes.
pub(crate) async fn steps(
    harness: &Harness<jadpo_host_native::Postgres>,
) -> Vec<(i32, String, String, Option<String>, Option<Vec<u8>>)> {
    harness
        .rows("SELECT \"position\", \"operation\", \"fingerprint\"::text, \"outcome\", \"answer\" FROM \"jadpo_steps\" ORDER BY \"position\"")
        .await
        .into_iter()
        .map(|row| match row.as_slice() {
            [SqlValue::Int4(position), SqlValue::Text(operation), SqlValue::Text(fingerprint), outcome, answer] => (
                *position,
                operation.clone(),
                fingerprint.clone(),
                match outcome {
                    SqlValue::Text(text) => Some(text.clone()),
                    _ => None,
                },
                match answer {
                    SqlValue::Bytes(bytes) => Some(bytes.clone()),
                    _ => None,
                },
            ),
            other => panic!("not a step: {other:?}"),
        })
        .collect()
}

/// The record's state and problem, when one is kept.
pub(crate) async fn record(
    harness: &Harness<jadpo_host_native::Postgres>,
) -> Option<(String, String)> {
    harness
        .rows("SELECT \"state\", COALESCE(\"problem\"::text, '') FROM \"jadpo_step_records\"")
        .await
        .into_iter()
        .next()
        .map(|row| match row.as_slice() {
            [SqlValue::Text(state), SqlValue::Text(problem)] => (state.clone(), problem.clone()),
            other => panic!("not a record: {other:?}"),
        })
}

/// The requests the provider was sent to `path`, with their keys.
pub(crate) fn sent(provider: &FakeProvider, path: &str) -> Vec<Option<String>> {
    provider
        .requests()
        .iter()
        .filter(|request| request.path == path)
        .map(|request| request.header("idempotency-key").map(str::to_string))
        .collect()
}

pub(crate) const CUSTOMERS: &str = "/v1/customers";
pub(crate) const SUBSCRIPTIONS: &str = "/v1/subscriptions";
pub(crate) const CUSTOMER: &str = r#"{"id": "cus_4821"}"#;
pub(crate) const SUBSCRIPTION: &str = r#"{"id": "sub_9930"}"#;

/// Steps 1 and 2 answered (the customer, the receipt kept), step 3
/// unanswered after every try: the delivery is retried later, the answers
/// kept sealed and step 3 in doubt. Its retry replays steps 1 and 2
/// without sending them, the receipt under the key it was first kept
/// under, sends step 3 again under the same key, writes the account with
/// the first run's `clock.now`, and deletes the record in the same
/// commit.
#[test]
fn a_retry_replays_what_was_answered_with_the_first_runs_time() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("replay", &provider.url, false) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let (host, sheets) = faked(&harness);
    let program = &loaded.program;
    provider.answer(200, CUSTOMER);
    for _ in 0..TRIES {
        provider.answer(503, "");
    }
    provider.answer(200, SUBSCRIPTION);
    Runtime::new().expect("runtime").block_on(async {
        let (_, work) = chosen(&harness, &host, program, "gold").await;
        run_deferred(program, &host, work).await;
        let (status, attempts, _) = delivery(&harness).await;
        assert_eq!((status.as_str(), attempts), ("pending", 1));
        let kept = steps(&harness).await;
        assert_eq!(kept.len(), 3, "each step recorded before it went out");
        let (position, operation, fingerprint, outcome, answer) = &kept[0];
        assert_eq!((*position, operation.as_str()), (1, "pay.customers.create"));
        assert_eq!(outcome.as_deref(), Some("answered"));
        let answer = answer.as_ref().expect("its answer");
        assert!(
            !String::from_utf8_lossy(answer).contains("cus_4821"),
            "the answer is sealed"
        );
        let fingerprint: serde_json::Value =
            serde_json::from_str(fingerprint).expect("a fingerprint");
        let fields: Vec<&String> = fingerprint.as_object().expect("by field").keys().collect();
        assert_eq!(fields, ["reference"]);
        assert!(
            !fingerprint.to_string().contains("ada"),
            "a request is kept only as its fingerprint"
        );
        assert_eq!(
            (kept[1].1.as_str(), kept[1].3.as_deref()),
            ("sheets.put", Some("answered"))
        );
        assert_eq!(kept[2].1, "pay.subscriptions.create");
        assert_eq!(kept[2].3, None, "step 3 is in doubt");
        assert_eq!(record(&harness).await.map(|(state, _)| state).as_deref(), Some("running"));

        // An hour later the retry: step 1 replayed, step 2 sent again.
        harness.set_now(T0 + HOUR);
        tick(program, &host, T0 + HOUR).await;
        let (status, attempts, _) = delivery(&harness).await;
        assert_eq!((status.as_str(), attempts), ("succeeded", 2));
        let customers = sent(&provider, CUSTOMERS);
        assert_eq!(customers.len(), 1, "step 1 was never sent again");
        let subscriptions = sent(&provider, SUBSCRIPTIONS);
        assert_eq!(subscriptions.len(), TRIES + 1);
        assert!(subscriptions[0].is_some());
        assert!(
            subscriptions.iter().all(|key| key == &subscriptions[0]),
            "step 2 sent again under its key"
        );
        assert_ne!(customers[0], subscriptions[0]);
        let accounts = harness
            .rows("SELECT \"customer\", \"subscription\", \"started_at\", \"receipt\" ->> 'key' FROM \"account\"")
            .await;
        let put = sheets.sent();
        assert_eq!(put.len(), 1, "the receipt was kept once");
        let Value::Record(input) = &put[0].input else {
            panic!("a put's input");
        };
        let Some(Value::Text(key)) = input.get("key") else {
            panic!("its key");
        };
        assert_eq!(
            accounts,
            [vec![
                SqlValue::Text("cus_4821".into()),
                SqlValue::Text("sub_9930".into()),
                SqlValue::Timestamp(T0 * 1000),
                SqlValue::Text(key.to_string()),
            ]],
            "written from the replayed answers, at the first run's time, the receipt under the key it was kept under"
        );
        assert_eq!(record(&harness).await, None, "deleted in the commit");
        assert!(steps(&harness).await.is_empty());
    });
}

/// A retry whose request differs from what step 3 sent (a fresh read of
/// the offer changed its plan) stops as `diverged` before sending
/// anything, held for an operator, naming the step and the field, never
/// the value; with the offer put back, as a fix deployed would, and the
/// delivery retried, it finishes.
#[test]
fn a_retry_whose_request_changed_diverges_before_sending() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("diverged", &provider.url, false) else {
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
    Runtime::new().expect("runtime").block_on(async {
        let (offer, work) = chosen(&harness, &host, program, "gold").await;
        run_deferred(program, &host, work).await;
        let Value::Uuid(offer_id) = field(&offer, "id").clone() else {
            panic!("an id");
        };
        let change = |code: &str| {
            format!("UPDATE \"offer\" SET \"code\" = '{code}' WHERE \"id\" = '{offer_id}'")
        };
        harness.rows(&change("platinum")).await;
        harness.set_now(T0 + HOUR);
        tick(program, &host, T0 + HOUR).await;
        let (status, _, failure) = delivery(&harness).await;
        assert_eq!(status, "diverged");
        let failure: serde_json::Value = serde_json::from_str(&failure).expect("json");
        assert_eq!(failure["code"], "diverged");
        assert_eq!(
            failure["problem"],
            "step 3 of `subscribe`, `pay.subscriptions.create`: `plan` differs from the record"
        );
        assert!(!failure.to_string().contains("gold") && !failure.to_string().contains("platinum"));
        assert_eq!(sent(&provider, SUBSCRIPTIONS).len(), TRIES, "nothing sent");
        let (state, problem) = record(&harness).await.expect("the record");
        assert_eq!(state, "diverged");
        let problem: serde_json::Value = serde_json::from_str(&problem).expect("json");
        assert_eq!(problem["step"], 3);
        assert_eq!(problem["fields"], serde_json::json!(["plan"]));

        // The offer put back and the delivery retried: it finishes.
        harness.rows(&change("gold")).await;
        harness
            .rows("UPDATE \"jadpo_deliveries\" SET \"status\" = 'pending', \"next_attempt_at\" = \"last_tried_at\"")
            .await;
        harness.set_now(T0 + 2 * HOUR);
        tick(program, &host, T0 + 2 * HOUR).await;
        assert_eq!(delivery(&harness).await.0, "succeeded");
        assert_eq!(sent(&provider, CUSTOMERS).len(), 1);
        assert_eq!(sent(&provider, SUBSCRIPTIONS).len(), TRIES + 1);
        assert_eq!(record(&harness).await, None);
    });
}

/// A step in doubt is sent again only while its provider keeps the key,
/// less the margin (an hour of a day's window): a retry after that parks
/// the delivery as `outcome_unknown` without sending it.
#[test]
fn a_step_in_doubt_past_its_window_parks() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("window", &provider.url, false) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let (host, _sheets) = faked(&harness);
    let program = &loaded.program;
    provider.answer(200, CUSTOMER);
    for _ in 0..TRIES {
        provider.answer(503, "");
    }
    Runtime::new().expect("runtime").block_on(async {
        let (_, work) = chosen(&harness, &host, program, "gold").await;
        run_deferred(program, &host, work).await;
        let late = T0 + 23 * HOUR + SECOND;
        harness.set_now(late);
        tick(program, &host, late).await;
        let (status, _, failure) = delivery(&harness).await;
        assert_eq!(status, "outcome_unknown", "{failure}");
        assert_eq!(
            sent(&provider, SUBSCRIPTIONS).len(),
            TRIES,
            "never sent again"
        );
        assert!(
            record(&harness).await.is_some(),
            "the record stays for an operator"
        );
    });
}

/// A provider that replays a failed answer under its key: its server's
/// error at a step parks the delivery at once, since sending it again
/// would only repeat it.
#[test]
fn a_replayed_server_error_parks_at_once() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("replays", &provider.url, true) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let (host, _sheets) = faked(&harness);
    let program = &loaded.program;
    provider.answer(200, CUSTOMER);
    for _ in 0..TRIES {
        provider.answer(500, r#"{"error": {"code": "api_error"}}"#);
    }
    Runtime::new().expect("runtime").block_on(async {
        let (_, work) = chosen(&harness, &host, program, "gold").await;
        run_deferred(program, &host, work).await;
        let (status, attempts, _) = delivery(&harness).await;
        assert_eq!((status.as_str(), attempts), ("outcome_unknown", 1));
        assert!(record(&harness).await.is_some());
    });
}

/// The provider's "key in use" is waited for: the delivery is retried
/// and the step sent again under its key. Its "key reused" answer, a
/// different request under that key, is `diverged`, never a failure.
#[test]
fn a_key_in_use_waits_and_a_key_reused_diverges() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("in-use", &provider.url, false) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let (host, _sheets) = faked(&harness);
    let program = &loaded.program;
    provider.answer(200, CUSTOMER);
    for _ in 0..TRIES {
        provider.answer(409, r#"{"error": {"code": "key_in_use"}}"#);
    }
    provider.answer(409, r#"{"error": {"code": "key_reused"}}"#);
    Runtime::new().expect("runtime").block_on(async {
        let (_, work) = chosen(&harness, &host, program, "gold").await;
        run_deferred(program, &host, work).await;
        assert_eq!(delivery(&harness).await.0, "pending", "waited for");
        harness.set_now(T0 + HOUR);
        tick(program, &host, T0 + HOUR).await;
        let (status, _, failure) = delivery(&harness).await;
        assert_eq!(status, "diverged");
        assert!(
            failure.contains("says its key came with a different request"),
            "{failure}"
        );
        let keys = sent(&provider, SUBSCRIPTIONS);
        assert_eq!(keys.len(), TRIES + 1);
        assert!(keys.iter().all(|key| key == &keys[0]));
    });
}

/// A record held by a newer runner (its generation past this attempt's)
/// takes no write from an older one: the step isn't sent, and the
/// delivery is retried.
#[test]
fn a_runner_holding_an_older_generation_writes_and_sends_nothing() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("generation", &provider.url, false) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let (host, _sheets) = faked(&harness);
    let program = &loaded.program;
    provider.answer(200, CUSTOMER);
    for _ in 0..TRIES {
        provider.answer(503, "");
    }
    Runtime::new().expect("runtime").block_on(async {
        let (_, work) = chosen(&harness, &host, program, "gold").await;
        run_deferred(program, &host, work).await;
        // Another runner took the record over meanwhile.
        harness
            .rows("UPDATE \"jadpo_step_records\" SET \"generation\" = 99")
            .await;
        harness.set_now(T0 + HOUR);
        tick(program, &host, T0 + HOUR).await;
        let (status, attempts, _) = delivery(&harness).await;
        assert_eq!((status.as_str(), attempts), ("pending", 2));
        assert_eq!(sent(&provider, CUSTOMERS).len(), 1, "nothing sent");
        assert_eq!(sent(&provider, SUBSCRIPTIONS).len(), TRIES);
        assert_eq!(steps(&harness).await.len(), 3, "nothing written");
    });
}

/// An answer past the 1 MB a step keeps is a fault naming the step, and
/// nothing of it is kept.
#[test]
fn an_answer_past_a_megabyte_is_a_fault_naming_the_step() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("large", &provider.url, false) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let (host, _sheets) = faked(&harness);
    let program = &loaded.program;
    let large = format!(
        r#"{{"id": "cus_4821", "notes": "{}"}}"#,
        "x".repeat(1 << 20)
    );
    provider.answer(200, &large);
    Runtime::new().expect("runtime").block_on(async {
        let (_, work) = chosen(&harness, &host, program, "gold").await;
        let outcomes = run_deferred(program, &host, work).await;
        let [Outcome::Fault(fault)] = outcomes.as_slice() else {
            panic!("{outcomes:?}");
        };
        assert_eq!(
            fault.kind,
            jadpo_runtime::FaultKind::Provider(
                "step 1 of `subscribe`, `pay.customers.create`, answered more than the 1 MB a step keeps"
                    .into()
            )
        );
        assert_eq!(steps(&harness).await[0].3, None, "nothing kept of it");
        assert_eq!(delivery(&harness).await.0, "failed");
    });
}

/// One statement on its own, from another connection.
pub(crate) fn once(url: &str, sql: &str) {
    let runtime = Runtime::new().expect("a runtime");
    let store = jadpo_host_native::postgres(url).expect("a store");
    runtime
        .block_on(store.query_once(sql, &[]))
        .unwrap_or_else(|error| panic!("{sql}: {error:?}"));
}

/// A change to the offer the action read, made while its last step is
/// out, runs the action again (area 6, "A change reruns the action"): the
/// rerun replays every step from its record rather than sending any
/// again, and its write keeps the change.
#[test]
fn a_rerun_after_a_concurrent_change_replays_its_steps() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("rerun", &provider.url, false) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let (host, sheets) = faked(&harness);
    let program = &loaded.program;
    provider.answer(200, CUSTOMER);
    provider.answer(200, SUBSCRIPTION);
    let change = std::sync::Arc::new(std::sync::Mutex::new(Some(
        "UPDATE \"offer\" SET \"subscribers\" = 5".to_string(),
    )));
    {
        let url = loaded.url.clone();
        let change = std::sync::Arc::clone(&change);
        provider.hold(move |request| {
            if request.path == SUBSCRIPTIONS
                && let Some(sql) = change.lock().expect("change").take()
            {
                once(&url, &sql);
            }
        });
    }
    Runtime::new().expect("runtime").block_on(async {
        let (_, work) = chosen(&harness, &host, program, "gold").await;
        run_deferred(program, &host, work).await;
        let (status, attempts, _) = delivery(&harness).await;
        assert_eq!((status.as_str(), attempts), ("succeeded", 1));
        assert_eq!(sent(&provider, CUSTOMERS).len(), 1);
        assert_eq!(
            sent(&provider, SUBSCRIPTIONS).len(),
            1,
            "replayed, not sent again"
        );
        assert_eq!(sheets.sent().len(), 1);
        assert!(
            change.lock().expect("change").is_none(),
            "the change was made"
        );
        assert_eq!(
            harness.rows("SELECT \"subscribers\" FROM \"offer\"").await,
            [vec![SqlValue::Int8(6)]],
            "the rerun read the change and counted on it"
        );
        assert_eq!(record(&harness).await, None);
    });
}

/// A route whose API requires `Idempotency-Key` keeps its record under
/// the client's key: a request that ends `unavailable` at step 3, retried
/// by the client under the same key a minute later, replays steps 1 and
/// 2 and finishes, its account written at the first request's time.
#[test]
fn a_clients_retry_under_its_key_resumes_a_routes_steps() {
    use jadpo_runtime::http::{Request, Worker, serve_request};
    let provider = FakeProvider::start();
    let Some(loaded) = load("route", &provider.url, false) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let (host, sheets) = faked(&harness);
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
        assert_eq!(
            first.status,
            503,
            "{}",
            String::from_utf8_lossy(&first.body)
        );
        assert!(record(&harness).await.is_some());
        harness.set_now(T0 + 60 * SECOND);
        let again = serve_request(&app, program, &host, &worker, &request).await;
        assert_eq!(
            again.status,
            204,
            "{}",
            String::from_utf8_lossy(&again.body)
        );
        assert_eq!(sent(&provider, CUSTOMERS).len(), 1);
        assert_eq!(sheets.sent().len(), 1);
        let keys = sent(&provider, SUBSCRIPTIONS);
        assert_eq!(keys.len(), TRIES + 1);
        assert!(keys.iter().all(|key| key == &keys[0]));
        assert_eq!(
            harness.rows("SELECT \"started_at\" FROM \"account\"").await,
            [vec![SqlValue::Timestamp(T0 * 1000)]]
        );
        assert_eq!(record(&harness).await, None);
    });
}

/// Every recorded step must be replayed: a rerun that ends without
/// reaching one (the record holds a fourth step this build's action
/// doesn't make) stops as `diverged` instead of committing.
#[test]
fn a_rerun_that_ends_before_a_recorded_step_diverges() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("missed", &provider.url, false) else {
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
    Runtime::new().expect("runtime").block_on(async {
        let (_, work) = chosen(&harness, &host, program, "gold").await;
        run_deferred(program, &host, work).await;
        harness
            .rows("INSERT INTO \"jadpo_steps\" (\"record_id\", \"position\", \"operation\", \"fingerprint\", \"key_generation\", \"dispatched_at\") SELECT \"id\", 4, 'pay.refunds.create', '{}', 0, \"started_at\" FROM \"jadpo_step_records\"")
            .await;
        harness.set_now(T0 + HOUR);
        tick(program, &host, T0 + HOUR).await;
        let (status, _, failure) = delivery(&harness).await;
        assert_eq!(status, "diverged");
        assert!(
            failure.contains("step 4 of `subscribe`, `pay.refunds.create`, wasn't reached"),
            "{failure}"
        );
        assert_eq!(harness.count("SELECT count(*) FROM \"account\"").await, 0, "nothing committed");
        assert!(record(&harness).await.is_some());
    });
}

/// One changing call needs no steps encryption setting, but its durable
/// first dispatch still bounds every later attempt under the provider key.
#[test]
fn a_single_call_in_doubt_obeys_the_safe_key_window_without_a_steps_key() {
    for (label, elapsed, expected) in [
        ("inside", 23 * HOUR - 1, "succeeded"),
        ("boundary", 23 * HOUR, "outcome_unknown"),
        ("after", 23 * HOUR + 1, "outcome_unknown"),
    ] {
        let provider = FakeProvider::start();
        let Some(loaded) = load_calls(&format!("single-{label}"), &provider.url, false, true)
        else {
            return;
        };
        let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
        let lost = Rc::new(Fake::answering(jadpo_runtime::capability::Answer::Wire(
            Rc::from(CUSTOMER),
        )));
        for _ in 0..TRIES {
            lost.script(
                "customers.create",
                jadpo_runtime::capability::Scripted::Answer(
                    jadpo_runtime::capability::Answer::Unknown,
                ),
            );
        }
        let host = Faked::new(&harness).with("pay", lost.clone() as Rc<dyn Capability>);
        Runtime::new().expect("runtime").block_on(async {
            let (_, work) = chosen(&harness, &host, &loaded.program, "gold").await;
            run_deferred(&loaded.program, &host, work).await;
            assert_eq!(delivery(&harness).await.0, "pending");
            let metadata = steps(&harness).await;
            assert_eq!(metadata.len(), 1);
            assert_eq!(metadata[0].2, "{}", "no request fingerprint is kept");
            assert_eq!(
                metadata[0].4, None,
                "no answer is kept without an encryption key"
            );
            let sent_before = lost.sent();
            assert!(!sent_before.is_empty());
            let key = sent_before[0]
                .key
                .as_ref()
                .expect("a provider key")
                .key
                .clone();
            assert!(
                sent_before
                    .iter()
                    .all(|call| call.key.as_ref().unwrap().key == key)
            );
            harness.set_now(T0 + elapsed);
            tick(&loaded.program, &host, T0 + elapsed).await;
            assert_eq!(delivery(&harness).await.0, expected, "{label}");
            let sent_after = lost.sent();
            if expected == "outcome_unknown" {
                assert_eq!(
                    sent_after.len(),
                    sent_before.len(),
                    "never sent past its safe window"
                );
            } else {
                assert_eq!(sent_after.len(), sent_before.len() + 1);
                assert_eq!(
                    harness.rows("SELECT \"counted_at\" FROM \"offer\"").await,
                    [vec![SqlValue::Timestamp((T0 + elapsed) * 1000)]],
                    "single-call code keeps a fresh clock"
                );
                assert!(
                    steps(&harness).await.is_empty(),
                    "dispatch metadata is removed in the commit"
                );
                assert_eq!(sent_after.last().unwrap().key.as_ref().unwrap().key, key);
            }
        });
    }
}

#[test]
fn a_single_call_rechecks_its_generation_before_a_physical_retry() {
    struct TakenOver {
        database: jadpo_host_native::Postgres,
        sent: std::cell::Cell<u32>,
    }
    impl Capability for TakenOver {
        fn call<'a>(
            &'a self,
            _: &'a jadpo_runtime::capability::Call,
        ) -> jadpo_runtime::LocalFuture<'a, jadpo_runtime::capability::Answer> {
            Box::pin(async move {
                self.sent.set(self.sent.get() + 1);
                self.database
                    .query_once("UPDATE \"jadpo_step_records\" SET \"generation\" = 99", &[])
                    .await
                    .expect("another runner took over");
                jadpo_runtime::capability::Answer::Unknown
            })
        }
    }
    let provider = FakeProvider::start();
    let Some(loaded) = load_calls("single-fenced", &provider.url, false, true) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let driver = Rc::new(TakenOver {
        database: jadpo_host_native::postgres(&loaded.url).expect("database"),
        sent: std::cell::Cell::new(0),
    });
    let host = Faked::new(&harness).with("pay", driver.clone() as Rc<dyn Capability>);
    Runtime::new().expect("runtime").block_on(async {
        let (_, work) = chosen(&harness, &host, &loaded.program, "gold").await;
        run_deferred(&loaded.program, &host, work).await;
        assert_eq!(
            driver.sent.get(),
            1,
            "the older runner never sends a second request"
        );
        assert_eq!(delivery(&harness).await.0, "pending");
    });
}

#[test]
fn a_single_call_checks_the_window_after_awaiting_its_dispatch_fence() {
    use jadpo_runtime::{Isolation, LocalFuture, Row, StoreError, Transaction};
    use std::cell::Cell;
    struct Delayed<'a> {
        inner: &'a dyn Database,
        clock: Rc<Cell<i64>>,
        jump: Cell<bool>,
    }
    impl Database for Delayed<'_> {
        fn begin_at(
            &self,
            isolation: Isolation,
        ) -> LocalFuture<'_, Result<Box<dyn Transaction>, StoreError>> {
            self.inner.begin_at(isolation)
        }
        fn query_once<'a>(
            &'a self,
            sql: &'a str,
            params: &'a [SqlValue],
        ) -> LocalFuture<'a, Result<Vec<Row>, StoreError>> {
            Box::pin(async move {
                let rows = self.inner.query_once(sql, params).await?;
                if sql == jadpo_runtime::steps::SEND && self.jump.replace(false) {
                    self.clock.set(T0 + 23 * HOUR + 1);
                }
                Ok(rows)
            })
        }
    }
    struct Wall<'a> {
        harness: &'a Harness,
        database: &'a Delayed<'a>,
    }
    impl Host for Wall<'_> {
        fn database(&self, _: &str) -> Option<&dyn Database> {
            Some(self.database)
        }
        fn random(&self, bytes: &mut [u8]) -> bool {
            self.harness.random(bytes)
        }
        fn now_ms(&self) -> Option<i64> {
            Some(self.database.clock.get())
        }
    }
    let provider = FakeProvider::start();
    let Some(loaded) = load_calls("single-fence-delay", &provider.url, false, true) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let database = Delayed {
        inner: harness.store(),
        clock: Rc::new(Cell::new(T0)),
        jump: Cell::new(false),
    };
    let wall = Wall {
        harness: &harness,
        database: &database,
    };
    let driver = Rc::new(Fake::answering(jadpo_runtime::capability::Answer::Wire(
        Rc::from(CUSTOMER),
    )));
    for _ in 0..TRIES {
        driver.script(
            "customers.create",
            jadpo_runtime::capability::Scripted::Answer(jadpo_runtime::capability::Answer::Unknown),
        );
    }
    let host = Faked::new(&wall).with("pay", driver.clone() as Rc<dyn Capability>);
    Runtime::new().expect("runtime").block_on(async {
        let (_, work) = chosen(&harness, &host, &loaded.program, "gold").await;
        run_deferred(&loaded.program, &host, work).await;
        assert_eq!(delivery(&harness).await.0, "pending");
        let before = driver.sent().len();
        database.clock.set(T0 + 23 * HOUR - 1);
        database.jump.set(true);
        tick(&loaded.program, &host, database.clock.get()).await;
        assert_eq!(delivery(&harness).await.0, "outcome_unknown");
        assert_eq!(
            driver.sent().len(),
            before,
            "a slow fence must not permit a send after its window"
        );
    });
}

async fn only_delivery_id(harness: &Harness) -> String {
    let rows = harness
        .rows("SELECT replace(\"id\"::text, '-', '') FROM \"jadpo_deliveries\"")
        .await;
    match rows.as_slice() {
        [row] => match row.as_slice() {
            [SqlValue::Text(id)] => id.clone(),
            other => panic!("delivery id: {other:?}"),
        },
        other => panic!("one delivery: {other:?}"),
    }
}

fn changing_record_mode(name: &str, old_single: bool, answered: bool, empty: bool) {
    use jadpo_runtime::capability::{Answer, Scripted};
    let provider = FakeProvider::start();
    let Some(old) = load_mode(
        &format!("mode-{name}-old"),
        &provider.url,
        false,
        old_single,
        empty,
    ) else {
        return;
    };
    let new = load_mode(
        &format!("mode-{name}-new"),
        &provider.url,
        false,
        !old_single,
        empty,
    )
    .expect("live");
    let harness = Harness::postgres(&old.url).at(T0).random_from(0);
    let driver = Rc::new(Fake::answering(Answer::Wire(Rc::from(CUSTOMER))));
    for _ in 0..TRIES {
        driver.script(
            if answered {
                "subscriptions.create"
            } else {
                "customers.create"
            },
            Scripted::Answer(Answer::Unknown),
        );
    }
    let host = Faked::new(&harness)
        .with("pay", driver.clone() as Rc<dyn Capability>)
        .with("sheets", Rc::new(Fake::new()));
    let id = Runtime::new().expect("runtime").block_on(async {
        let (_, work) = chosen(&harness, &host, &old.program, "gold").await;
        run_deferred(&old.program, &host, work).await;
        assert_eq!(delivery(&harness).await.0, "pending");
        let rows = steps(&harness).await;
        if answered {
            assert!(rows[0].4.is_some(), "legacy replay has a sealed answer");
        } else {
            assert_eq!(rows.len(), 1);
            assert!(rows[0].4.is_none());
            assert_eq!(
                rows[0].2, "{}",
                "empty requests cannot identify record mode"
            );
        }
        let before = driver.sent().len();
        harness.set_now(T0 + HOUR);
        tick(&new.program, &host, T0 + HOUR).await;
        assert_eq!(delivery(&harness).await.0, "diverged");
        assert_eq!(
            driver.sent().len(),
            before,
            "mode changes stop before any provider send"
        );
        let (state, problem) = record(&harness).await.expect("kept for the operator");
        assert_eq!(state, "diverged");
        let problem: serde_json::Value = serde_json::from_str(&problem).expect("problem");
        assert_eq!(problem["replay"], old_single);
        only_delivery_id(&harness).await
    });
    let shown = jadpo(
        &new.root,
        T0 + HOUR,
        &["deliveries", "show", &id, "--url", &old.url, "--json"],
    );
    assert_eq!(shown.code, 0, "{}{}", shown.stdout, shown.stderr);
    assert!(
        shown.stdout.contains("encrypted replay"),
        "{}",
        shown.stdout
    );
    assert!(
        shown.stdout.contains("dispatch metadata"),
        "{}",
        shown.stdout
    );
}

#[test]
fn changing_record_mode_with_an_encrypted_answer_diverges() {
    changing_record_mode("encrypted", false, true, false);
}

#[test]
fn changing_record_mode_from_legacy_empty_unanswered_replay_diverges() {
    changing_record_mode("legacy-empty", false, false, true);
}

#[test]
fn changing_record_mode_from_empty_dispatch_metadata_diverges() {
    changing_record_mode("dispatch-empty", true, false, true);
}

#[test]
fn dispatch_metadata_keeps_its_mode_through_operator_inspection_retry_and_abandon() {
    use jadpo_runtime::capability::{Answer, Scripted};
    for action in ["retry", "abandon"] {
        let provider = FakeProvider::start();
        let Some(loaded) = load_calls(
            &format!("dispatch-operator-{action}"),
            &provider.url,
            false,
            true,
        ) else {
            return;
        };
        let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
        let driver = Rc::new(Fake::answering(Answer::Wire(Rc::from(CUSTOMER))));
        for _ in 0..TRIES {
            driver.script("customers.create", Scripted::Answer(Answer::Unknown));
        }
        let host = Faked::new(&harness).with("pay", driver.clone() as Rc<dyn Capability>);
        let rt = Runtime::new().expect("runtime");
        let (before, shape, late, id) = rt.block_on(async {
            let (_, work) = chosen(&harness, &host, &loaded.program, "gold").await;
            run_deferred(&loaded.program, &host, work).await;
            let late = T0 + 23 * HOUR + 1;
            harness.set_now(late);
            tick(&loaded.program, &host, late).await;
            assert_eq!(delivery(&harness).await.0, "outcome_unknown");
            let shape = harness
                .rows("SELECT \"shape\" FROM \"jadpo_step_records\"")
                .await[0][0]
                .clone();
            assert!(matches!(&shape, SqlValue::Text(shape) if shape.starts_with("dispatch-v1:")));
            harness
                .rows("UPDATE \"jadpo_step_records\" SET \"build\" = 'older'")
                .await;
            (driver.sent(), shape, late, only_delivery_id(&harness).await)
        });
        let build = loaded.program.plan().build_id();
        let inspection = jadpo(
            &loaded.root,
            late,
            &[
                "runs",
                "unfinished",
                "--build",
                build,
                "--url",
                &loaded.url,
                "--json",
            ],
        );
        assert_eq!(
            inspection.code, 0,
            "{}{}",
            inspection.stdout, inspection.stderr
        );
        let json: serde_json::Value = serde_json::from_str(&inspection.stdout).expect("JSON");
        assert_eq!(
            json["data"]["changed"],
            serde_json::json!([]),
            "same metadata mode/shape resumes across a build"
        );
        let mut args = vec!["deliveries", action, &id, "--url", &loaded.url];
        if action == "retry" {
            args.push("--not-done");
        }
        let operated = jadpo(&loaded.root, late, &args);
        assert_eq!(operated.code, 0, "{}{}", operated.stdout, operated.stderr);
        rt.block_on(async {
            assert_eq!(
                harness
                    .rows("SELECT \"shape\" FROM \"jadpo_step_records\"")
                    .await[0][0],
                shape
            );
            harness.set_now(late + 60 * SECOND);
            tick(&loaded.program, &host, late + 60 * SECOND).await;
            if action == "retry" {
                assert_eq!(delivery(&harness).await.0, "succeeded");
                let sent = driver.sent();
                assert_ne!(
                    before[0].key.as_ref().unwrap().key,
                    sent.last().unwrap().key.as_ref().unwrap().key
                );
                assert!(steps(&harness).await.is_empty());
            } else {
                assert_eq!(driver.sent().len(), before.len());
                assert_eq!(
                    record(&harness).await.expect("abandoned record").0,
                    "abandoned"
                );
            }
        });
    }
}
