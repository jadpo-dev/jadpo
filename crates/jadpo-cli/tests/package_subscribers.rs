//! A package's subscribers on its own webhooks, against live PostgreSQL
//! (area 11, "What package code may do"; chunk 7.6b): Acme Pay's webhook
//! says only that an invoice was paid, an event its package keeps to
//! itself; the package's own subscriber runs its action, which looks the
//! subscription up through the connection the webhook came in on and
//! emits the package's public `SubscriptionRenewed` through the app's
//! outbox, for the app's subscriber to keep. A package connected twice
//! hears each connection's webhooks once, through that connection.
//!
//! Acme Pay's API is a provider started in the test on this machine; the
//! webhooks are handed to the runtime as a host would. The server is the
//! one `JADPO_TEST_DATABASE_URL` names or one started from the machine's
//! PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::path::Path;
use std::rc::Rc;

use jadpo_cli::{Output, run};
use jadpo_drivers::package::JsonHttp;
use jadpo_host_native::Runtime;
use jadpo_host_native::fetch::NativeFetch;
use jadpo_interp::{Prepared, Program};
use jadpo_runtime::capability::{Capability, Faked};
use jadpo_runtime::http::{Request, Response, handle};
use jadpo_runtime::runner::{Deferred, run_deferred};
use jadpo_runtime::{Outcome, SqlValue};
use jadpo_term::Environment;
use jadpo_test::host::Harness;

use crate::support::fake_provider::FakeProvider;

const HOME: &str = "shared/providers/acme_pay/v1";

/// The project, Acme Pay's sandbox at `sandbox`.
fn files(sandbox: &str) -> Vec<(String, String)> {
    let package = |path: &str, text: &str| (format!("{HOME}/{path}"), text.to_string());
    vec![
        (
            "app.jadpo".into(),
            "application {\n    connections: {\n        payments: {\n            package: AcmePay.v1\n            domains: [billing]\n            credential: config.payments_key\n            environment: config.payments_environment\n            webhooks: {\n                secret: config.payments_webhook_secret\n            }\n        }\n        payments_eu: {\n            package: AcmePay.v1\n            domains: [billing]\n            credential: config.payments_eu_key\n            environment: config.payments_environment\n            webhooks: {\n                secret: config.payments_eu_webhook_secret\n            }\n        }\n    }\n}\n".into(),
        ),
        (
            "config.jadpo".into(),
            "config {\n    payments_key: Text {\n        secret: true\n    }\n    payments_eu_key: Text {\n        secret: true\n    }\n    payments_environment: ProviderEnvironment\n    payments_webhook_secret: Text {\n        secret: true\n    }\n    payments_eu_webhook_secret: Text {\n        secret: true\n    }\n}\n".into(),
        ),
        package(
            "package.jadpo",
            "/// Acme Pay's payments API.\npackage AcmePay.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n",
        ),
        package(
            "provider.jadpo",
            &format!(
                "/// Acme Pay's API, which signs its webhooks as Stripe does.\nprovider {{\n    protocol: Protocol.json_http\n    hosts: {{\n        live: \"https://api.acme-pay.example\"\n        sandbox: \"{sandbox}\"\n    }}\n    credential: CredentialScheme.bearer\n    webhooks: {{\n        check: SignatureScheme.stripe\n        event_id: \"id\"\n        event_type: \"type\"\n        occurred_at: \"created\"\n    }}\n}}\n"
            ),
        ),
        package(
            "types/reference.jadpo",
            "/// Acme Pay's reference for a record.\npublic type Reference = Text {\n    max_length: 40\n}\n",
        ),
        package(
            "types/subscription_asked.jadpo",
            "/// Which subscription to look up.\ntype SubscriptionAsked = Object {\n    id: Reference\n}\n",
        ),
        package(
            "types/subscription.jadpo",
            "/// A subscription, as Acme Pay keeps it.\ntype Subscription = Object {\n    id: Reference\n    plan: Reference\n}\n",
        ),
        package(
            "resources/subscriptions.jadpo",
            "resource subscriptions {\n    retrieve: {\n        method: HttpMethod.post\n        path: \"/v1/subscriptions/lookup\"\n        input: SubscriptionAsked\n        output: Subscription\n        source: PackageSource.written\n    }\n}\n",
        ),
        package(
            "events/invoice_paid.jadpo",
            "/// An invoice was paid: all Acme Pay's webhook says.\ntype InvoicePaid = Event {\n    wire_name: \"invoice.paid\"\n    occurred_at: Instant\n    subscription: Reference\n}\n",
        ),
        package(
            "events/subscription_renewed.jadpo",
            "/// A subscription was renewed, with its plan.\npublic type SubscriptionRenewed = Event {\n    subscription: Reference\n    plan: Reference\n    paid_at: Instant\n}\n",
        ),
        package(
            "actions/announce_renewal.jadpo",
            "/// Looks up the subscription an invoice paid for, and announces it.\naction announce_renewal(event: InvoicePaid) -> Unit {\n    var found: Subscription = subscriptions.retrieve(SubscriptionAsked {\n        id: event.subscription\n    })\n    emit SubscriptionRenewed {\n        subscription: found.id\n        plan: found.plan\n        paid_at: event.occurred_at\n    }\n}\n",
        ),
        package(
            "subscribers/renewal.jadpo",
            "/// Turns a paid invoice into a renewal.\nsubscriber renewal {\n    on: InvoicePaid\n    run: announce_renewal(event)\n}\n",
        ),
        (
            "domains/billing/entities/renewal.jadpo".into(),
            "use shared/providers/acme_pay\n\nentity Renewal {\n    id: Uuid\n    subscription: AcmePay.Reference\n    plan: AcmePay.Reference\n    paid_at: Instant\n\n    policy: {\n        create: [Access.system]\n        read: [Access.system]\n    }\n}\n".into(),
        ),
        (
            "domains/billing/actions/record_renewal.jadpo".into(),
            "use billing/entities { Renewal }\nuse shared/providers/acme_pay\n\n/// Keeps a renewal Acme Pay announced.\naction record_renewal(event: AcmePay.SubscriptionRenewed) -> Renewal {\n    return Renewal.create {\n        set: {\n            subscription: event.subscription\n            plan: event.plan\n            paid_at: event.paid_at\n        }\n    }\n}\n".into(),
        ),
        (
            "domains/billing/subscribers/renewed.jadpo".into(),
            "use billing/actions { record_renewal }\nuse shared/providers/acme_pay\n\n/// Keeps each renewal.\nsubscriber renewed {\n    on: AcmePay.SubscriptionRenewed\n    run: record_renewal(event)\n}\n".into(),
        ),
    ]
}

/// When the test starts: 2026-10-19T09:00:00Z.
const T0: i64 = 1_792_400_400_000;
const SECRET: &str = "whsec_pay_test_secret";
const EU_SECRET: &str = "whsec_pay_eu_test_secret";

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty()
        .with_var("SOURCE_DATE_EPOCH", &(T0 / 1000).to_string())
        .with_var("NO_COLOR", "1");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    run(&args, &env.with_cwd(cwd))
}

struct Loaded {
    program: Program,
    _databases: jadpo_test::database::TestDatabases,
    url: String,
}

/// The project, checked, with its migration, and a test database of its
/// own; `None` when live tests are skipped.
fn load(name: &str, sandbox: &str) -> Option<Loaded> {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("package-subscribers-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files(sandbox) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        fs::write(path, text).expect("write");
    }
    for args in [&["migrate", "new", "initial"][..], &["check"]] {
        let out = jadpo(&root, args);
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
            "PAYMENTS_KEY" => Some("pk_test_uk".into()),
            "PAYMENTS_EU_KEY" => Some("pk_test_eu".into()),
            "PAYMENTS_ENVIRONMENT" => Some("sandbox".into()),
            "PAYMENTS_WEBHOOK_SECRET" => Some(SECRET.into()),
            "PAYMENTS_EU_WEBHOOK_SECRET" => Some(EU_SECRET.into()),
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
    })
}

/// HMAC-SHA-256, written out from RFC 2104 over SHA-256, so the test signs
/// as a provider does without the runtime's own code.
fn hmac(key: &[u8], message: &[u8]) -> Vec<u8> {
    let sha = |data: &[u8]| jadpo_value::hash::sha256(data).to_vec();
    let mut block = if key.len() > 64 {
        sha(key)
    } else {
        key.to_vec()
    };
    block.resize(64, 0);
    let inner: Vec<u8> = block.iter().map(|b| b ^ 0x36).collect();
    let outer: Vec<u8> = block.iter().map(|b| b ^ 0x5c).collect();
    let first = sha(&[inner, message.to_vec()].concat());
    sha(&[outer, first].concat())
}

/// Invoice `id` paid for subscription `subscription`, sent to
/// `connection`'s webhook signed with `secret`, as Acme Pay signs.
fn paid(connection: &str, secret: &str, id: &str, subscription: &str) -> Request {
    let created = T0 / 1000 - 30;
    let body = format!(
        r#"{{"id":"{id}","type":"invoice.paid","created":{created},"subscription":"{subscription}"}}"#
    );
    let t = (T0 / 1000).to_string();
    let mac = hmac(secret.as_bytes(), format!("{t}.{body}").as_bytes());
    let hex: String = mac.iter().map(|b| format!("{b:02x}")).collect();
    Request {
        method: "POST".into(),
        path: format!("/webhooks/{connection}"),
        headers: vec![
            ("content-type".into(), "application/json".into()),
            ("stripe-signature".into(), format!("t={t},v1={hex}")),
        ],
        body: body.into_bytes(),
        address: Some("192.0.2.10".into()),
        ..Request::default()
    }
}

/// Answers `request` as a host would, giving the response and the work it
/// left.
async fn send(
    harness: &Harness<jadpo_host_native::Postgres>,
    program: &Program,
    request: &Request,
) -> (Response, Vec<Deferred>) {
    let app = program.app();
    let mut ctx = harness.ctx(program.places());
    let response = handle(&app, program, &mut ctx, request, "req_test").await;
    (response, ctx.take_deferred())
}

/// The package driver over the native host's HTTP, for both connections.
fn faked(
    harness: &Harness<jadpo_host_native::Postgres>,
) -> Faked<&Harness<jadpo_host_native::Postgres>> {
    let driver: Rc<dyn Capability> = Rc::new(JsonHttp::new(NativeFetch));
    Faked::new(harness)
        .with("payments", Rc::clone(&driver))
        .with("payments_eu", driver)
}

/// Acme Pay says an invoice was paid; the package's subscriber looks the
/// subscription up through `payments`, with its credential, and emits the
/// renewal with the plan it found; the app keeps it, at the time Acme Pay
/// says the invoice was paid.
#[test]
fn a_webhook_is_enriched_by_the_packages_subscriber_and_reaches_the_app() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("enriched", &provider.url) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let program = &loaded.program;
    let runtime = Runtime::new().expect("runtime");
    provider.answer(200, r#"{"id": "sub_1", "plan": "gold"}"#);
    runtime.block_on(async {
        let (response, deferred) = send(
            &harness,
            program,
            &paid("payments", SECRET, "evt_1", "sub_1"),
        )
        .await;
        assert_eq!(
            response.status,
            204,
            "{:?}",
            String::from_utf8_lossy(&response.body)
        );
        // Kept for the package's subscriber, heard on `payments` only.
        let delivered = harness
            .rows("SELECT \"subscriber\" FROM \"jadpo_deliveries\"")
            .await;
        assert_eq!(
            delivered,
            vec![vec![SqlValue::Text(
                "subscriber AcmePay.renewal payments".into()
            )]]
        );
        let host = faked(&harness);
        let outcomes = run_deferred(program, &host, deferred).await;
        assert!(
            outcomes
                .iter()
                .all(|outcome| matches!(outcome, Outcome::Value(_))),
            "{outcomes:?}"
        );
        let sent = provider.requests();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].path, "/v1/subscriptions/lookup");
        assert_eq!(sent[0].json()["id"], "sub_1");
        assert_eq!(sent[0].header("authorization"), Some("Bearer pk_test_uk"));
        // The renewal it emitted reached the app's subscriber, through
        // the outbox.
        let kept = harness.rows("SELECT \"event\" FROM \"jadpo_outbox\"").await;
        assert_eq!(kept.len(), 2, "{kept:?}");
        let renewals = harness
            .rows("SELECT \"subscription\", \"plan\", \"paid_at\" FROM \"renewal\"")
            .await;
        assert_eq!(
            renewals,
            vec![vec![
                SqlValue::Text("sub_1".into()),
                SqlValue::Text("gold".into()),
                SqlValue::Timestamp((T0 - 30_000) * 1000),
            ]]
        );
    });
}

/// The same package connected twice: an invoice paid on `payments_eu` is
/// looked up through `payments_eu`, with its own credential.
#[test]
fn each_connection_hears_its_own_webhooks_through_itself() {
    let provider = FakeProvider::start();
    let Some(loaded) = load("each", &provider.url) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let program = &loaded.program;
    let runtime = Runtime::new().expect("runtime");
    provider.answer(200, r#"{"id": "sub_9", "plan": "silver"}"#);
    runtime.block_on(async {
        let (response, deferred) = send(
            &harness,
            program,
            &paid("payments_eu", EU_SECRET, "evt_9", "sub_9"),
        )
        .await;
        assert_eq!(response.status, 204);
        let delivered = harness
            .rows("SELECT \"subscriber\" FROM \"jadpo_deliveries\"")
            .await;
        assert_eq!(
            delivered,
            vec![vec![SqlValue::Text(
                "subscriber AcmePay.renewal payments_eu".into()
            )]]
        );
        let host = faked(&harness);
        run_deferred(program, &host, deferred).await;
        let sent = provider.requests();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].header("authorization"), Some("Bearer pk_test_eu"));
        let renewals = harness.rows("SELECT \"plan\" FROM \"renewal\"").await;
        assert_eq!(renewals, vec![vec![SqlValue::Text("silver".into())]]);
    });
}
