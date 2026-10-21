//! Keys and replays (area 12, "Several changing calls in one action";
//! area 7, "Idempotency keys"; area 6, "All-or-nothing actions and
//! concurrent changes"; chunk 7.9a), live against PostgreSQL: a desk that
//! opens customers and subscriptions at a payment provider started in the
//! test, from routes, subscribers and a route that waits for its work.
//!
//! A route whose action calls out keeps its client's `Idempotency-Key`
//! even when it neither writes nor emits, so a retry replays rather than
//! acting twice; a delivery run while a route waits counts its keys from
//! none, as its retry does; a step failing `unavailable` with its own code
//! stays in doubt and goes again under its key; the runtime's own
//! timestamps are the host's time, never the record's fixed `clock.now`;
//! and an unkeyed change that went out is never sent again on area 6's
//! rerun.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use jadpo_drivers::package::JsonHttp;
use jadpo_host_native::Runtime;
use jadpo_host_native::fetch::NativeFetch;
use jadpo_interp::{Prepared, Program};
use jadpo_runtime::capability::{Capability, Faked};
use jadpo_runtime::http::{Request, Worker, serve_request};
use jadpo_runtime::runner::run_deferred;
use jadpo_runtime::schedules::tick;
use jadpo_runtime::{Executor, Host, Outcome, SqlValue};
use jadpo_test::host::Harness;
use jadpo_value::Value;

use crate::step_records::{HOUR, SECOND, STEPS_KEY, T0, TRIES, jadpo, once};
use crate::support::fake_provider::FakeProvider;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

/// The desk's package, Acme Pay at `sandbox`: two keyed changing calls,
/// one with no key, and a failure that says it did nothing.
fn package(sandbox: &str) -> Vec<(&'static str, String)> {
    vec![
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
    }}
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
        failures: [PayBusy]
        source: PackageSource.written
    }
}
"#
            .to_string(),
        ),
        (
            "shared/providers/acme_pay/v1/resources/notices.jadpo",
            r#"public resource notices {
    send: {
        method: HttpMethod.post
        path: "/v1/notices"
        input: NoticeRequest
        output: Notice
        changes: true
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
            "shared/providers/acme_pay/v1/types/notice_request.jadpo",
            "/// A notice to a customer.\npublic type NoticeRequest = Object {\n    to: Reference\n}\n"
                .to_string(),
        ),
        (
            "shared/providers/acme_pay/v1/types/notice.jadpo",
            "/// A notice Acme Pay sent.\npublic type Notice = Object {\n    id: Reference\n}\n"
                .to_string(),
        ),
        (
            "shared/providers/acme_pay/v1/failures/pay_busy.jadpo",
            "/// Acme Pay was too busy to act, and did nothing.\npublic type PayBusy = Failure {\n    kind: FailureKind.unavailable\n    message: \"Acme Pay is busy.\"\n}\n"
                .to_string(),
        ),
    ]
}

/// The desk, its provider at `sandbox`.
fn files(sandbox: &str) -> Vec<(&'static str, String)> {
    let mut files = vec![
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
            "policy.jadpo",
            "use billing\n\npolicy {\n    actions: {\n        billing.open_customer: [Access.public, Access.system]\n        billing.subscribe_now: [Access.public, Access.system]\n        billing.subscribe: [Access.public, Access.system]\n    }\n}\n"
                .to_string(),
        ),
        (
            "domains/billing/failures/busy.jadpo",
            "/// Acme Pay is too busy for now.\ntype Busy = Failure {\n    kind: FailureKind.unavailable\n    message: \"Acme Pay is busy for now.\"\n}\n"
                .to_string(),
        ),
        (
            "domains/billing/types/customer_form.jadpo",
            "use shared/providers/acme_pay\n\n/// Who to open.\ntype CustomerForm = Input {\n    customer: AcmePay.Reference\n}\n"
                .to_string(),
        ),
        (
            "domains/billing/types/booking_view.jadpo",
            "use billing/entities { Booking }\n\n/// A booking, as its caller sees it.\ntype BookingView = Output {\n    pick: [Booking.id, Booking.customer, Booking.subscription]\n}\n"
                .to_string(),
        ),
        (
            "domains/billing/entities/offer.jadpo",
            r#"use shared/providers/acme_pay

/// A plan the desk offers, and how many notices went about it.
entity Offer {
    id: Uuid
    code: AcmePay.Reference
    notices: Int

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
            "domains/billing/entities/booking.jadpo",
            r#"use shared/providers/acme_pay

/// A customer booked, and the subscription confirmed for them.
entity Booking {
    id: Uuid
    customer: AcmePay.Reference
    subscription: AcmePay.Reference?

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
            "domains/billing/events/plan_chosen.jadpo",
            "use shared/providers/acme_pay\n\n/// A customer chose a plan.\ntype PlanChosen = Event {\n    customer: AcmePay.Reference\n    plan: AcmePay.Reference\n}\n"
                .to_string(),
        ),
        (
            "domains/billing/events/subscribed.jadpo",
            "use shared/providers/acme_pay\n\n/// A customer's subscription started.\ntype Subscribed = Event {\n    subscription: AcmePay.Reference\n}\n"
                .to_string(),
        ),
        (
            "domains/billing/events/booking_made.jadpo",
            "use billing/entities { Booking }\n\n/// A booking was made.\ntype BookingMade = Event {\n    booking_id: Booking.id\n}\n"
                .to_string(),
        ),
        (
            "domains/billing/queries/booking_status.jadpo",
            r#"use billing/entities { Booking, BookingNotFound }
use billing/types { BookingView }

/// One booking.
query booking_status(booking_id: Booking.id) fails BookingNotFound -> BookingView {
    from: Booking
    where: Booking.id == booking_id
    missing: BookingNotFound
}
"#
            .to_string(),
        ),
        (
            "domains/billing/actions/add_offer.jadpo",
            "use billing/entities { Offer }\nuse shared/providers/acme_pay\n\n/// Adds an offer.\naction add_offer(code: AcmePay.Reference) -> Offer {\n    return Offer.create {\n        set: {\n            code: code\n            notices: 0\n        }\n    }\n}\n"
                .to_string(),
        ),
        (
            "domains/billing/actions/choose.jadpo",
            "use billing/events { PlanChosen }\nuse shared/providers/acme_pay\n\n/// A customer chooses a plan.\naction choose(customer: AcmePay.Reference, plan: AcmePay.Reference) -> Unit {\n    emit PlanChosen {\n        customer: customer\n        plan: plan\n    }\n}\n"
                .to_string(),
        ),
        (
            "domains/billing/actions/subscribe.jadpo",
            r#"use app { pay }
use billing/events { PlanChosen, Subscribed }
use billing/failures { Busy }
use shared/providers/acme_pay

/// Opens the customer and starts their subscription, two changing calls,
/// then says so: it writes nothing, so its record is kept beside its
/// events.
action subscribe(event: PlanChosen) fails Busy -> Unit {
    var opened: AcmePay.Customer = pay.customers.create(AcmePay.CustomerRequest {
        reference: event.customer
    })
    var subscription: AcmePay.Subscription = attempt pay.subscriptions.create(AcmePay.SubscriptionRequest {
        customer: opened.id
        plan: event.plan
    }) {
        pay_busy: Busy
    }
    emit Subscribed {
        subscription: subscription.id
    }
}
"#
            .to_string(),
        ),
        (
            "domains/billing/actions/subscribe_now.jadpo",
            r#"use app { pay }
use billing/failures { Busy }
use billing/types { CustomerForm }
use shared/providers/acme_pay

/// Opens a customer and starts their subscription straight away: two
/// changing calls, and nothing written or emitted.
action subscribe_now(form: CustomerForm) fails Busy -> Unit {
    var opened: AcmePay.Customer = pay.customers.create(AcmePay.CustomerRequest {
        reference: form.customer
    })
    var subscription: AcmePay.Subscription = attempt pay.subscriptions.create(AcmePay.SubscriptionRequest {
        customer: opened.id
        plan: "basic"
    }) {
        pay_busy: Busy
    }
    assert subscription.id != ""
}
"#
            .to_string(),
        ),
        (
            "domains/billing/actions/open_customer.jadpo",
            r#"use app { pay }
use billing/types { CustomerForm }
use shared/providers/acme_pay

/// Opens a customer: one changing call, and nothing written or emitted.
action open_customer(form: CustomerForm) -> Unit {
    var opened: AcmePay.Customer = pay.customers.create(AcmePay.CustomerRequest {
        reference: form.customer
    })
    assert opened.id != ""
}
"#
            .to_string(),
        ),
        (
            "domains/billing/actions/book.jadpo",
            r#"use app { pay }
use billing/entities { Booking }
use billing/events { BookingMade }
use billing/types { CustomerForm }
use shared/providers/acme_pay

/// Opens a customer and books them, for a subscriber to confirm.
action book(form: CustomerForm) -> Booking {
    var opened: AcmePay.Customer = pay.customers.create(AcmePay.CustomerRequest {
        reference: form.customer
    })
    var booking: Booking = Booking.create {
        set: {
            customer: opened.id
            subscription: none
        }
    }
    emit BookingMade {
        booking_id: booking.id
    }
    return booking
}
"#
            .to_string(),
        ),
        (
            "domains/billing/actions/confirm.jadpo",
            r#"use app { pay }
use billing/entities { Booking, BookingNotFound }
use billing/events { BookingMade }
use billing/failures { Busy }
use shared/providers/acme_pay

/// Starts a booked customer's subscription and keeps it.
action confirm(event: BookingMade) fails BookingNotFound, Busy -> Unit {
    var booking: Booking = attempt Booking.get(event.booking_id)
    var subscription: AcmePay.Subscription = attempt pay.subscriptions.create(AcmePay.SubscriptionRequest {
        customer: booking.customer
        plan: "basic"
    }) {
        pay_busy: Busy
    }
    var confirmed: Booking = Booking.update(booking) {
        set: {
            subscription: subscription.id
        }
    }
    assert confirmed.subscription != none
}
"#
            .to_string(),
        ),
        (
            "domains/billing/actions/send_notice.jadpo",
            r#"use app { pay }
use billing/entities { Offer, OfferNotFound }
use shared/providers/acme_pay

/// Reads an offer, sends a notice about it, which takes no key, then
/// counts the notice.
action send_notice(offer_id: Offer.id) fails OfferNotFound -> Offer {
    var offer: Offer = attempt Offer.get(offer_id)
    var notice: AcmePay.Notice = pay.notices.send(AcmePay.NoticeRequest {
        to: offer.code
    })
    assert notice.id != ""
    return Offer.update(offer) {
        set: {
            notices: offer.notices + 1
        }
    }
}
"#
            .to_string(),
        ),
        (
            "domains/billing/routes/customers.jadpo",
            r#"use billing/actions { book, open_customer }
use billing/queries { booking_status }
use billing/types { BookingView, CustomerForm }

/// Opening customers.
routes Customers {
    prefix: /customers

    /// Opens a customer.
    POST / {
        authentication: RouteAuthentication.none
        input: CustomerForm
        status: SuccessStatus.no_content
        run: open_customer(input)
    }

    /// Books a customer and answers once the booking is confirmed.
    POST /bookings {
        authentication: RouteAuthentication.none
        input: CustomerForm
        output: BookingView
        status: SuccessStatus.created
        deadline: 30s
        run: book(input)
        settle: {
            answer: booking_status(result.id)
            within: 20s
        }
    }
}
"#
            .to_string(),
        ),
        (
            "domains/billing/routes/partners_subscriptions.jadpo",
            r#"use billing/actions { subscribe_now }
use billing/types { CustomerForm }

/// What a partner's systems call.
routes PartnersSubscriptions {
    api: partners
    prefix: /partners/subscriptions

    /// Starts a subscription straight away.
    POST / {
        authentication: RouteAuthentication.none
        input: CustomerForm
        status: SuccessStatus.no_content
        deadline: 60s
        run: subscribe_now(input)
    }
}
"#
            .to_string(),
        ),
        (
            "domains/billing/subscribers/start_subscription.jadpo",
            "use billing/actions { subscribe }\nuse billing/events { PlanChosen }\n\n/// Starts the subscription a customer chose.\nsubscriber start_subscription {\n    on: PlanChosen\n    run: subscribe(event)\n    retry: {\n        attempts: 10\n        within: 3d\n    }\n}\n"
                .to_string(),
        ),
        (
            "domains/billing/subscribers/confirm_booking.jadpo",
            "use billing/actions { confirm }\nuse billing/events { BookingMade }\n\n/// Confirms each booking.\nsubscriber confirm_booking {\n    on: BookingMade\n    run: confirm(event)\n    retry: {\n        attempts: 10\n        within: 3d\n    }\n}\n"
                .to_string(),
        ),
    ];
    files.extend(package(sandbox));
    files
}

struct Loaded {
    program: Program,
    _databases: jadpo_test::database::TestDatabases,
    url: String,
}

/// The settings every run reads.
fn variables(name: &str) -> Option<String> {
    match name {
        "PAY_API_KEY" => Some("pk_test_123".into()),
        "PAY_ENVIRONMENT" => Some("sandbox".into()),
        "STEP_ENCRYPTION_KEY" => Some(STEPS_KEY.into()),
        _ => None,
    }
}

/// `files`, written at a folder of its own named `name`, migrated and
/// checked.
fn written(name: &str, files: Vec<(&'static str, String)>, migrate: bool) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("step-replays-7.9a-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files {
        write(&root, path, &text);
    }
    let mut commands: Vec<&[&str]> = Vec::new();
    if migrate {
        commands.push(&["migrate", "new", "initial"]);
    }
    commands.push(&["records", "update"]);
    commands.push(&["check"]);
    for args in commands {
        let out = jadpo(&root, T0, args);
        assert_eq!(out.code, 0, "{args:?}: {}{}", out.stdout, out.stderr);
    }
    root
}

/// The project at `root`, with its settings.
fn program(root: &Path) -> Program {
    let loaded = jadpo_db::load(root);
    let prepared = Prepared::load(&loaded.db, loaded.project).unwrap_or_else(|problems| {
        panic!(
            "the project checks: {:?}",
            problems.iter().map(|p| &p.message).collect::<Vec<_>>()
        )
    });
    let values = prepared
        .config(
            &variables,
            jadpo_runtime::config::Mode::Environment,
            &|_: &mut [u8]| false,
        )
        .unwrap_or_else(|problems| panic!("the settings read: {problems:?}"));
    Program::new(prepared).with_config(&values)
}

/// The desk, checked and migrated, with a test database for it; `None`
/// when live tests are skipped.
fn load(name: &str, sandbox: &str) -> Option<Loaded> {
    let root = written(name, files(sandbox), true);
    if !jadpo_test::live() {
        return None;
    }
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = program(&root);
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    Some(Loaded {
        program,
        _databases: databases,
        url,
    })
}

/// The package driver, over the native host's HTTP, for `pay`.
fn faked<H: Host>(host: H) -> Faked<H> {
    let driver: Rc<dyn Capability> = Rc::new(JsonHttp::new(NativeFetch));
    Faked::new(host).with("pay", driver)
}

/// The keys of the requests the provider was sent to `path`, in order.
fn sent(provider: &FakeProvider, path: &str) -> Vec<Option<String>> {
    provider
        .requests()
        .iter()
        .filter(|request| request.path == path)
        .map(|request| request.header("idempotency-key").map(str::to_string))
        .collect()
}

const CUSTOMERS: &str = "/v1/customers";
const SUBSCRIPTIONS: &str = "/v1/subscriptions";
const NOTICES: &str = "/v1/notices";
const CUSTOMER: &str = r#"{"id": "cus_4821"}"#;
const SUBSCRIPTION: &str = r#"{"id": "sub_9930"}"#;
const BUSY: &str = r#"{"error": {"code": "pay_busy"}}"#;

/// A `POST` of `body` to `path` under `key`.
fn post(path: &str, key: &str, body: serde_json::Value) -> Request {
    Request {
        method: "POST".to_string(),
        path: path.to_string(),
        headers: vec![
            ("content-type".to_string(), "application/json".to_string()),
            ("idempotency-key".to_string(), key.to_string()),
        ],
        body: body.to_string().into_bytes(),
        address: Some("replay test".to_string()),
        ..Request::default()
    }
}

/// Runs action `name` with `args` on `host`, giving its outcome and the
/// work it left for after its response.
async fn call(
    harness: &Harness<jadpo_host_native::Postgres>,
    host: &dyn Host,
    program: &Program,
    name: &str,
    args: Vec<Value>,
) -> (Outcome, Vec<jadpo_runtime::runner::Deferred>) {
    let id = program.plan().find(name).expect("the action");
    let mut ctx = harness.ctx_on(program.places(), host);
    let outcome = program.run(&mut ctx, id, args).await;
    (outcome, ctx.take_deferred())
}

/// The deliveries to `subscriber`: each one's state and attempts, and
/// its last failure as JSON text.
async fn deliveries(
    harness: &Harness<jadpo_host_native::Postgres>,
    subscriber: &str,
) -> Vec<(String, i32, String)> {
    harness
        .rows(&format!(
            "SELECT \"status\", \"attempts\", COALESCE(\"last_failure\"::text, '') FROM \"jadpo_deliveries\" WHERE \"subscriber\" LIKE '%{subscriber}' ORDER BY \"attempts\""
        ))
        .await
        .into_iter()
        .map(|row| match row.as_slice() {
            [
                SqlValue::Text(status),
                SqlValue::Int4(attempts),
                SqlValue::Text(failure),
            ] => (status.clone(), *attempts, failure.clone()),
            other => panic!("not a delivery: {other:?}"),
        })
        .collect()
}

/// Each recorded step's position and outcome, in order.
async fn steps(harness: &Harness<jadpo_host_native::Postgres>) -> Vec<(i32, Option<String>)> {
    harness
        .rows("SELECT \"position\", \"outcome\" FROM \"jadpo_steps\" ORDER BY \"position\"")
        .await
        .into_iter()
        .map(|row| match row.as_slice() {
            [SqlValue::Int4(position), outcome] => (
                *position,
                match outcome {
                    SqlValue::Text(text) => Some(text.clone()),
                    _ => None,
                },
            ),
            other => panic!("not a step: {other:?}"),
        })
        .collect()
}

// --- a route that calls out keeps its client's key -------------------------------

/// A route whose action makes two changing calls and neither writes nor
/// emits still keeps its client's key, beside its events: a request that
/// ends `unavailable` at step 2, sent again under the same key, replays
/// step 1 and sends step 2 again under its own key, so the customer is
/// opened once.
#[test]
fn a_route_that_only_calls_out_resumes_under_its_clients_key() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("route-steps", &provider.url) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let host = faked(&harness);
    let program = &loaded.program;
    provider.answer(200, CUSTOMER);
    for _ in 0..TRIES {
        provider.answer(503, "");
    }
    provider.answer(200, SUBSCRIPTION);
    let app = program.app();
    let worker = Worker::default();
    Runtime::new().expect("runtime").block_on(async {
        let request = post(
            "/partners/subscriptions",
            "k-ada",
            serde_json::json!({ "customer": "ada" }),
        );
        let first = serve_request(&app, program, &host, &worker, &request).await;
        assert_eq!(
            first.status,
            503,
            "{}",
            String::from_utf8_lossy(&first.body)
        );
        harness.set_now(T0 + 60 * SECOND);
        let again = serve_request(&app, program, &host, &worker, &request).await;
        assert_eq!(
            again.status,
            204,
            "{}",
            String::from_utf8_lossy(&again.body)
        );
        assert_eq!(sent(&provider, CUSTOMERS).len(), 1, "opened once");
        let keys = sent(&provider, SUBSCRIPTIONS);
        assert_eq!(keys.len(), TRIES + 1);
        assert!(keys.iter().all(|key| key == &keys[0]), "{keys:?}");
        assert_eq!(
            harness
                .count("SELECT count(*) FROM \"jadpo_step_records\"")
                .await,
            0,
            "the record went with the action's commit"
        );
        // A third time, the response kept for the key answers.
        let third = serve_request(&app, program, &host, &worker, &request).await;
        assert_eq!(third.status, 204);
        assert_eq!(sent(&provider, CUSTOMERS).len(), 1);
        assert_eq!(sent(&provider, SUBSCRIPTIONS).len(), TRIES + 1);
    });
}

/// A route whose action makes one changing call and neither writes nor
/// emits keeps its client's key too: the same request sent again under it
/// is answered with the first response, the provider not called again.
#[test]
fn a_route_with_one_call_answers_a_repeat_from_its_kept_response() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("route-one", &provider.url) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let host = faked(&harness);
    let program = &loaded.program;
    provider.answer(200, CUSTOMER);
    let app = program.app();
    let worker = Worker::default();
    Runtime::new().expect("runtime").block_on(async {
        let request = post(
            "/customers",
            "k-ada",
            serde_json::json!({ "customer": "ada" }),
        );
        for _ in 0..2 {
            let answered = serve_request(&app, program, &host, &worker, &request).await;
            assert_eq!(
                answered.status,
                204,
                "{}",
                String::from_utf8_lossy(&answered.body)
            );
        }
        assert_eq!(sent(&provider, CUSTOMERS).len(), 1, "called once");
    });
}

/// The desk with no store at all: `POST /customers` alone.
fn storeless(sandbox: &str) -> Vec<(&'static str, String)> {
    let mut files = vec![
        (
            "app.jadpo",
            r#"application {
    connections: {
        pay: {
            package: AcmePay.v1
            domains: [billing]
            credential: config.pay_api_key
            environment: config.pay_environment
        }
    }
}
"#
            .to_string(),
        ),
        (
            "config.jadpo",
            "config {\n    pay_api_key: Text {\n        secret: true\n    }\n    pay_environment: ProviderEnvironment\n}\n"
                .to_string(),
        ),
        (
            "policy.jadpo",
            "use billing\n\npolicy {\n    actions: {\n        billing.open_customer: [Access.public]\n    }\n}\n"
                .to_string(),
        ),
        (
            "domains/billing/types/customer_form.jadpo",
            "use shared/providers/acme_pay\n\n/// Who to open.\ntype CustomerForm = Input {\n    customer: AcmePay.Reference\n}\n"
                .to_string(),
        ),
        (
            "domains/billing/actions/open_customer.jadpo",
            r#"use app { pay }
use billing/types { CustomerForm }
use shared/providers/acme_pay

/// Opens a customer: one changing call, and no store to keep a key in.
action open_customer(form: CustomerForm) -> Unit {
    var opened: AcmePay.Customer = pay.customers.create(AcmePay.CustomerRequest {
        reference: form.customer
    })
    assert opened.id != ""
}
"#
            .to_string(),
        ),
        (
            "domains/billing/routes/customers.jadpo",
            "use billing/actions { open_customer }\nuse billing/types { CustomerForm }\n\n/// Opening customers.\nroutes Customers {\n    prefix: /customers\n\n    /// Opens a customer.\n    POST / {\n        authentication: RouteAuthentication.none\n        input: CustomerForm\n        status: SuccessStatus.no_content\n        run: open_customer(input)\n    }\n}\n"
                .to_string(),
        ),
    ];
    files.extend(package(sandbox));
    files
}

/// With no store to keep its client's key in, a route still derives its
/// provider calls' keys from it: the same `Idempotency-Key` sends the
/// same key, so the provider acts once, and another sends another.
#[test]
fn a_route_with_no_store_derives_its_calls_keys_from_its_clients_key() {
    let provider = FakeProvider::start();
    let root = written("storeless", storeless(&provider.url), false);
    let program = program(&root);
    for _ in 0..3 {
        provider.answer(200, CUSTOMER);
    }
    let host = faked(jadpo_host_native::NativeHost::new());
    let app = program.app();
    let worker = Worker::default();
    Runtime::new().expect("runtime").block_on(async {
        for key in ["k-ada", "k-ada", "k-bob"] {
            let request = post("/customers", key, serde_json::json!({ "customer": "ada" }));
            let answered = serve_request(&app, &program, &host, &worker, &request).await;
            assert_eq!(
                answered.status,
                204,
                "{}",
                String::from_utf8_lossy(&answered.body)
            );
        }
    });
    let keys = sent(&provider, CUSTOMERS);
    assert_eq!(keys.len(), 3);
    assert!(keys[0].is_some());
    assert_eq!(keys[0], keys[1], "the client's key sent again");
    assert_ne!(keys[0], keys[2], "another client's key");
}

// --- a delivery while a route waits ---------------------------------------------

/// A delivery a route waits for (`settle:`) counts its keyed calls from
/// none, as its retry in a context of its own does, whatever calls the
/// route's action made first: its subscription, unanswered while the
/// route waited, is sent again under the key it first went under.
#[test]
fn a_delivery_run_while_a_route_waits_keys_its_calls_as_its_retry_does() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("settle", &provider.url) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let host = faked(&harness);
    let program = &loaded.program;
    provider.answer(200, CUSTOMER);
    for _ in 0..TRIES {
        provider.answer(503, "");
    }
    provider.answer(200, SUBSCRIPTION);
    let app = program.app();
    let worker = Worker::default();
    Runtime::new().expect("runtime").block_on(async {
        let request = post(
            "/customers/bookings",
            "k-ada",
            serde_json::json!({ "customer": "ada" }),
        );
        let booked = serve_request(&app, program, &host, &worker, &request).await;
        assert_eq!(
            booked.status,
            202,
            "{}",
            String::from_utf8_lossy(&booked.body)
        );
        assert_eq!(
            deliveries(&harness, "confirm_booking").await[0].0,
            "pending"
        );
        harness.set_now(T0 + HOUR);
        tick(program, &host, T0 + HOUR).await;
        assert_eq!(
            deliveries(&harness, "confirm_booking").await[0].0,
            "succeeded"
        );
        let keys = sent(&provider, SUBSCRIPTIONS);
        assert_eq!(keys.len(), TRIES + 1);
        assert!(keys[0].is_some());
        assert!(
            keys.iter().all(|key| key == &keys[0]),
            "sent again under the key it first went under: {keys:?}"
        );
    });
}

// --- a failure that says it did nothing --------------------------------------------

/// A step answered with a failure of its own whose kind is `unavailable`
/// did nothing: it is left in doubt, not kept, and the retry sends it
/// again under its key, never replaying the failure.
#[test]
fn a_steps_unavailable_failure_is_left_in_doubt_and_sent_again() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("busy", &provider.url) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let host = faked(&harness);
    let program = &loaded.program;
    provider.answer(200, CUSTOMER);
    provider.answer(400, BUSY);
    provider.answer(200, SUBSCRIPTION);
    Runtime::new().expect("runtime").block_on(async {
        let (chosen, work) = call(
            &harness,
            &host,
            program,
            "choose",
            vec![Value::text("ada"), Value::text("gold")],
        )
        .await;
        assert!(matches!(chosen, Outcome::Value(_)), "{chosen:?}");
        run_deferred(program, &host, work).await;
        let delivered = deliveries(&harness, "start_subscription").await;
        assert_eq!(delivered[0].0, "pending", "{delivered:?}");
        assert_eq!(
            steps(&harness).await,
            [(1, Some("answered".to_string())), (2, None)],
            "step 2 in doubt, its failure not kept"
        );
        harness.set_now(T0 + HOUR);
        tick(program, &host, T0 + HOUR).await;
        let delivered = deliveries(&harness, "start_subscription").await;
        assert_eq!(delivered[0].0, "succeeded", "{delivered:?}");
        assert_eq!(sent(&provider, CUSTOMERS).len(), 1);
        let keys = sent(&provider, SUBSCRIPTIONS);
        assert_eq!(keys.len(), 2, "sent again, not replayed");
        assert_eq!(keys[0], keys[1], "under its key");
    });
}

// --- the runtime's own time ----------------------------------------------------------

/// Once a record fixes `clock.now` at its first run's start, the events
/// its rerun emits are stamped with the host's time, so a subscriber's
/// `within:` counts from when they were emitted.
#[test]
fn an_event_a_resumed_action_emits_is_stamped_with_the_hosts_time() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("stamped", &provider.url) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let host = faked(&harness);
    let program = &loaded.program;
    provider.answer(200, CUSTOMER);
    for _ in 0..TRIES {
        provider.answer(503, "");
    }
    provider.answer(200, SUBSCRIPTION);
    Runtime::new().expect("runtime").block_on(async {
        let (_, work) = call(
            &harness,
            &host,
            program,
            "choose",
            vec![Value::text("ada"), Value::text("gold")],
        )
        .await;
        run_deferred(program, &host, work).await;
        assert_eq!(
            deliveries(&harness, "start_subscription").await[0].0,
            "pending"
        );
        let later = T0 + 5 * HOUR;
        harness.set_now(later);
        tick(program, &host, later).await;
        assert_eq!(
            deliveries(&harness, "start_subscription").await[0].0,
            "succeeded"
        );
        assert_eq!(
            harness
                .rows(
                    "SELECT \"emitted_at\" FROM \"jadpo_outbox\" WHERE \"event\" LIKE '%Subscribed'"
                )
                .await,
            [vec![SqlValue::Timestamp(later * 1000)]],
            "stamped when it was emitted, not at the record's first run"
        );
    });
}

// --- a change that went out with no key -----------------------------------------------

/// A change to the offer the action read, made while its unkeyed notice
/// is out, would rerun the action (area 6): the notice can't be sent again
/// safely, so the action stops `outcome_unknown` instead, the notice sent
/// once and nothing written.
#[test]
fn an_unkeyed_change_that_went_out_is_never_sent_again_on_a_rerun() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("unkeyed", &provider.url) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let host = faked(&harness);
    let program = &loaded.program;
    provider.answer(200, r#"{"id": "ntc_1"}"#);
    provider.answer(200, r#"{"id": "ntc_2"}"#);
    let change = Arc::new(Mutex::new(Some(
        "UPDATE \"offer\" SET \"notices\" = 5".to_string(),
    )));
    {
        let url = loaded.url.clone();
        let change = Arc::clone(&change);
        provider.hold(move |request| {
            if request.path == NOTICES
                && let Some(sql) = change.lock().expect("change").take()
            {
                once(&url, &sql);
            }
        });
    }
    Runtime::new().expect("runtime").block_on(async {
        let (offer, _) = call(
            &harness,
            &host,
            program,
            "add_offer",
            vec![Value::text("gold")],
        )
        .await;
        let Outcome::Value(Value::Record(offer)) = offer else {
            panic!("{offer:?}");
        };
        let id = offer.get("id").expect("its id").clone();
        let (sent_notice, _) = call(&harness, &host, program, "send_notice", vec![id]).await;
        let Outcome::Failed(failure) = &sent_notice else {
            panic!("{sent_notice:?}");
        };
        assert_eq!(failure.code, "outcome_unknown", "{failure:?}");
        assert!(
            change.lock().expect("change").is_none(),
            "the change was made"
        );
        assert_eq!(sent(&provider, NOTICES).len(), 1, "never sent again");
        assert_eq!(
            harness.rows("SELECT \"notices\" FROM \"offer\"").await,
            [vec![SqlValue::Int8(5)]],
            "nothing written over the change"
        );
    });
}
