//! Webhooks as events, against live PostgreSQL (area 11, "Receiving from
//! outside"; area 12, "Events from outside"; chunk 7.7): a connection given
//! a webhook secret has `POST /webhooks/<connection>`, which the runtime
//! answers itself. A signed event is kept once, in the outbox with the
//! provider's id, and delivered to the app's subscriber after the
//! response, carrying the provider's `occurred_at`; a repeat is answered
//! as the first was and keeps nothing; a request whose signature doesn't
//! match, or was made more than five minutes ago, keeps nothing; a type
//! the package doesn't declare is acknowledged and dropped; a body its
//! event doesn't read is refused with its problems; and the tick deletes
//! the ids received once no provider would send them again.
//!
//! A shop takes payments through Acme Pay, which signs as Stripe does and
//! names each event's time `created`, and sends mail through Acme Mail,
//! which signs as Standard Webhooks do and gives no time in its body, so
//! its events' `occurred_at` is the signature's. Nothing reaches a
//! network: each request is handed to the runtime as a host would.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::path::Path;

use jadpo_cli::{Output, run};
use jadpo_host_native::Runtime;
use jadpo_interp::{Prepared, Program};
use jadpo_runtime::http::{Request, Response, handle};
use jadpo_runtime::runner::{Deferred, run_deferred};
use jadpo_runtime::schedules::tick;
use jadpo_runtime::{Executor, Outcome, SqlValue};
use jadpo_term::Environment;
use jadpo_test::host::Harness;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

const FILES: &[(&str, &str)] = &[
    (
        "app.jadpo",
        r#"application {
    connections: {
        payments: {
            package: AcmePay.v1
            domains: [billing]
            credential: config.payments_key
            environment: config.payments_environment
            webhooks: {
                secret: config.payments_webhook_secret
            }
        }
        mail: {
            package: AcmeMail.v1
            domains: [billing]
            credential: config.mail_key
            host: config.mail_address
            webhooks: {
                secret: config.mail_webhook_secret
            }
        }
    }
}
"#,
    ),
    (
        "config.jadpo",
        r#"config {
    payments_key: Text {
        secret: true
    }
    payments_environment: ProviderEnvironment
    payments_webhook_secret: Text {
        secret: true
    }
    mail_key: Text {
        secret: true
    }
    mail_address: Url
    mail_webhook_secret: Text {
        secret: true
    }
}
"#,
    ),
    (
        "shared/providers/acme_pay/v1/package.jadpo",
        r#"/// Acme Pay's payments API.
package AcmePay.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
"#,
    ),
    (
        "shared/providers/acme_pay/v1/provider.jadpo",
        r#"/// Acme Pay's API, which signs its webhooks as Stripe does.
provider {
    protocol: Protocol.json_http
    hosts: {
        live: "https://api.acme-pay.example"
        sandbox: "https://sandbox.acme-pay.example"
    }
    credential: CredentialScheme.bearer
    webhooks: {
        check: SignatureScheme.stripe
        event_id: "id"
        event_type: "type"
        occurred_at: "created"
    }
}
"#,
    ),
    (
        "shared/providers/acme_pay/v1/types/payment_id.jadpo",
        "/// Acme Pay's reference for a payment.\npublic type PaymentId = Text {\n    max_length: 40\n}\n",
    ),
    (
        "shared/providers/acme_pay/v1/events/payment_succeeded.jadpo",
        r#"/// A payment was taken.
public type PaymentSucceeded = Event {
    wire_name: "payment.succeeded"
    occurred_at: Instant
    payment_id: PaymentId {
        wire_name: "paymentId"
    }
    pence: Int {
        min: 0
    }
}
"#,
    ),
    (
        "shared/providers/acme_mail/v1/package.jadpo",
        r#"/// Acme Mail's sending API.
package AcmeMail.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
"#,
    ),
    (
        "shared/providers/acme_mail/v1/provider.jadpo",
        r#"/// Acme Mail's API, which signs its webhooks as Standard Webhooks do.
provider {
    protocol: Protocol.json_http
    credential: CredentialScheme.bearer
    webhooks: {
        check: SignatureScheme.standard_webhooks
        event_id: "data.message_id"
        event_type: "type"
    }
}
"#,
    ),
    (
        "shared/providers/acme_mail/v1/types/bounce.jadpo",
        "/// A message that came back.\npublic type Bounce = Object {\n    message_id: Text\n    address: Text\n}\n",
    ),
    (
        "shared/providers/acme_mail/v1/events/message_bounced.jadpo",
        r#"/// A message came back undelivered.
public type MessageBounced = Event {
    wire_name: "message.bounced"
    occurred_at: Instant
    data: Bounce
}
"#,
    ),
    (
        "domains/billing/entities/payment.jadpo",
        r#"use shared/providers/acme_pay

entity Payment {
    id: Uuid
    reference: AcmePay.PaymentId
    pence: Int {
        min: 0
    }
    taken_at: Instant

    policy: {
        create: [Access.system]
        read: [Access.system]
    }
}
"#,
    ),
    (
        "domains/billing/actions/record_payment.jadpo",
        r#"use billing/entities { Payment }
use shared/providers/acme_pay

/// Records a payment Acme Pay took, when it took it.
action record_payment(event: AcmePay.PaymentSucceeded) -> Payment {
    return Payment.create {
        set: {
            reference: event.payment_id
            pence: event.pence
            taken_at: event.occurred_at
        }
    }
}
"#,
    ),
    (
        "domains/billing/subscribers/payment_taken.jadpo",
        r#"use billing/actions { record_payment }
use shared/providers/acme_pay

/// Records each payment Acme Pay takes.
subscriber payment_taken {
    on: AcmePay.PaymentSucceeded
    run: record_payment(event)
}
"#,
    ),
];

/// When the test starts: 2026-10-19T09:00:00Z.
const T0: i64 = 1_792_400_400_000;
const PAY_SECRET: &str = "whsec_pay_test_secret";
/// A Standard Webhooks secret: `whsec_` and the Base64 of the key.
const MAIL_SECRET: &str = "whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw";

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
fn load(name: &str) -> Option<Loaded> {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("webhooks-7.7-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, text);
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
            "PAYMENTS_KEY" | "MAIL_KEY" => Some("key_test".into()),
            "PAYMENTS_ENVIRONMENT" => Some("sandbox".into()),
            "MAIL_ADDRESS" => Some("https://mail.example".into()),
            "PAYMENTS_WEBHOOK_SECRET" => Some(PAY_SECRET.into()),
            "MAIL_WEBHOOK_SECRET" => Some(MAIL_SECRET.into()),
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

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A request to `path` with `body`, signed with `secret` at `at` as Acme
/// Pay signs: `Stripe-Signature: t=<seconds>,v1=<hex>`.
fn pay(body: &str, secret: &str, at: i64) -> Request {
    let t = (at / 1000).to_string();
    let mac = hmac(secret.as_bytes(), format!("{t}.{body}").as_bytes());
    Request {
        method: "POST".into(),
        path: "/webhooks/payments".into(),
        headers: vec![
            ("content-type".into(), "application/json".into()),
            ("stripe-signature".into(), format!("t={t},v1={}", hex(&mac))),
        ],
        body: body.as_bytes().to_vec(),
        address: Some("192.0.2.10".into()),
        ..Request::default()
    }
}

/// A request signed as Acme Mail signs, Standard Webhooks' way.
fn mail(id: &str, body: &str, at: i64) -> Request {
    let t = (at / 1000).to_string();
    let key = jadpo_value::encoding::from_base64(MAIL_SECRET.trim_start_matches("whsec_"))
        .expect("the key");
    let mac = hmac(&key, format!("{id}.{t}.{body}").as_bytes());
    Request {
        method: "POST".into(),
        path: "/webhooks/mail".into(),
        headers: vec![
            ("webhook-id".into(), id.into()),
            ("webhook-timestamp".into(), t),
            (
                "webhook-signature".into(),
                format!("v1,{}", jadpo_value::encoding::to_base64(&mac)),
            ),
        ],
        body: body.as_bytes().to_vec(),
        address: Some("192.0.2.11".into()),
        ..Request::default()
    }
}

/// A payment's body: event `id` for payment `payment`, made at `created`
/// seconds since 1970.
fn payment(id: &str, payment: &str, created: i64) -> String {
    format!(
        r#"{{"id":"{id}","type":"payment.succeeded","created":{created},"paymentId":"{payment}","pence":1250,"livemode":false}}"#
    )
}

/// Answers `request` on `harness` as a host would, giving the response and
/// the work it left.
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

async fn count(harness: &Harness<jadpo_host_native::Postgres>, table: &str) -> i64 {
    harness
        .count(&format!("SELECT count(*) FROM \"{table}\""))
        .await
}

fn answer(response: &Response) -> serde_json::Value {
    serde_json::from_slice(&response.body).expect("a JSON body")
}

/// A signed payment is kept in the outbox once, with Acme Pay's id, and
/// delivered after the response to `payment_taken`, which records it at
/// the time Acme Pay says it was taken; sent again, it is answered the
/// same and nothing more is kept or delivered.
#[test]
fn a_signed_webhook_is_kept_once_and_delivered_to_its_subscriber() {
    let Some(loaded) = load("kept") else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let program = &loaded.program;
    let runtime = Runtime::new().expect("runtime");
    let created = T0 / 1000 - 90;
    runtime.block_on(async {
        let first = pay(&payment("evt_1", "pi_1", created), PAY_SECRET, T0);
        let (response, deferred) = send(&harness, program, &first).await;
        assert_eq!(
            response.status,
            204,
            "{:?}",
            String::from_utf8_lossy(&response.body)
        );
        assert!(response.body.is_empty());
        assert_eq!(response.route.as_deref(), Some("POST /webhooks/payments"));
        assert_eq!(count(&harness, "jadpo_outbox").await, 1);
        assert_eq!(count(&harness, "jadpo_deliveries").await, 1);
        let received = harness
            .rows("SELECT \"source\", \"event_id\" FROM \"jadpo_received_events\"")
            .await;
        assert_eq!(
            received,
            vec![vec![
                SqlValue::Text("webhook payments".into()),
                SqlValue::Text("evt_1".into())
            ]]
        );
        // The payload is the event as the contract writes it, its time the
        // provider's.
        let payload = harness
            .rows("SELECT \"payload\"::text FROM \"jadpo_outbox\"")
            .await;
        let [SqlValue::Text(payload)] = payload[0].as_slice() else {
            panic!("a payload: {payload:?}");
        };
        let payload: serde_json::Value = serde_json::from_str(payload).expect("JSON");
        assert_eq!(payload["paymentId"], "pi_1");
        // An `Int` is the contract's text, as every payload writes one.
        assert_eq!(payload["pence"], "1250");
        assert_eq!(payload["occurred_at"], "2026-10-19T08:58:30Z");
        assert!(payload.get("livemode").is_none(), "{payload}");
        // Delivered after the response: the payment is recorded.
        let outcomes = run_deferred(program, &harness, deferred).await;
        assert!(
            matches!(outcomes.as_slice(), [Outcome::Value(_)]),
            "{outcomes:?}"
        );
        let payments = harness
            .rows("SELECT \"reference\", \"pence\", \"taken_at\" FROM \"payment\"")
            .await;
        assert_eq!(
            payments,
            vec![vec![
                SqlValue::Text("pi_1".into()),
                SqlValue::Int8(1250),
                SqlValue::Timestamp((T0 - 90_000) * 1000),
            ]]
        );
        // Acme Pay sends it again, signed afresh a minute later: answered
        // the same, nothing kept, nothing delivered.
        harness.advance(60_000);
        let again = pay(&payment("evt_1", "pi_1", created), PAY_SECRET, T0 + 60_000);
        let (response, deferred) = send(&harness, program, &again).await;
        assert_eq!(response.status, 204);
        assert!(deferred.is_empty());
        assert_eq!(count(&harness, "jadpo_outbox").await, 1);
        assert_eq!(count(&harness, "jadpo_received_events").await, 1);
        assert_eq!(count(&harness, "payment").await, 1);
    });
}

/// A signature made with another secret, one made six minutes ago, none
/// at all, or a body changed after it was signed: `401 unauthenticated`,
/// and nothing kept.
#[test]
fn a_webhook_whose_signature_doesnt_match_keeps_nothing() {
    let Some(loaded) = load("refused") else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let program = &loaded.program;
    let runtime = Runtime::new().expect("runtime");
    let body = payment("evt_2", "pi_2", T0 / 1000);
    runtime.block_on(async {
        let mut changed = pay(&body, PAY_SECRET, T0);
        changed.body = payment("evt_2", "pi_999", T0 / 1000).into_bytes();
        let mut unsigned = pay(&body, PAY_SECRET, T0);
        unsigned
            .headers
            .retain(|(name, _)| name != "stripe-signature");
        for (why, request) in [
            ("another secret", pay(&body, "whsec_someone_else", T0)),
            (
                "signed six minutes ago",
                pay(&body, PAY_SECRET, T0 - 360_000),
            ),
            ("changed after signing", changed),
            ("unsigned", unsigned),
        ] {
            let (response, deferred) = send(&harness, program, &request).await;
            assert_eq!(response.status, 401, "{why}");
            assert_eq!(
                answer(&response)["error"]["code"],
                "unauthenticated",
                "{why}"
            );
            assert_eq!(
                answer(&response)["error"]["message"],
                "The webhook's signature doesn't match.",
                "{why}"
            );
            assert!(deferred.is_empty(), "{why}");
        }
        assert_eq!(count(&harness, "jadpo_outbox").await, 0);
        assert_eq!(count(&harness, "jadpo_received_events").await, 0);
    });
}

/// A type Acme Pay sends that its package doesn't declare is acknowledged
/// and dropped; a body that isn't JSON, or has no event id where the
/// package says, is `400`; one its event doesn't read is `422` with each
/// problem by the provider's field; another method is `405`.
#[test]
fn what_a_webhook_cant_be_kept_as_is_answered_without_keeping_it() {
    let Some(loaded) = load("unread") else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let program = &loaded.program;
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let refunded = r#"{"id":"evt_3","type":"payment.refunded","created":1792400000}"#;
        let (response, _) = send(&harness, program, &pay(refunded, PAY_SECRET, T0)).await;
        assert_eq!(response.status, 204);
        let (response, _) = send(&harness, program, &pay("not json", PAY_SECRET, T0)).await;
        assert_eq!(response.status, 400);
        assert_eq!(answer(&response)["error"]["code"], "invalid_request");
        let no_id =
            r#"{"type":"payment.succeeded","created":1792400000,"paymentId":"pi_4","pence":1}"#;
        let (response, _) = send(&harness, program, &pay(no_id, PAY_SECRET, T0)).await;
        assert_eq!(response.status, 400);
        let unread = r#"{"id":"evt_5","type":"payment.succeeded","created":1792400000,"pence":-3}"#;
        let (response, _) = send(&harness, program, &pay(unread, PAY_SECRET, T0)).await;
        assert_eq!(response.status, 422);
        let problems = &answer(&response)["error"]["details"]["problems"];
        assert_eq!(
            problems,
            &serde_json::json!([
                {"field": "paymentId", "rule": "required"},
                {"field": "pence", "rule": "min"}
            ]),
            "{problems}"
        );
        let mut get = pay(&payment("evt_6", "pi_6", 1), PAY_SECRET, T0);
        get.method = "GET".into();
        let (response, _) = send(&harness, program, &get).await;
        assert_eq!(response.status, 405);
        assert_eq!(response.header("allow"), Some("OPTIONS, POST"));
        assert_eq!(count(&harness, "jadpo_outbox").await, 0);
        assert_eq!(count(&harness, "jadpo_received_events").await, 0);
    });
}

/// Acme Mail gives no time in its body, so a bounce's `occurred_at` is when
/// it was signed; its id is read from inside the body. Thirty days on, the
/// tick deletes the ids received, so Acme Pay's and Acme Mail's are gone.
#[test]
fn an_event_with_no_time_in_its_body_carries_its_signatures_and_old_ids_go() {
    let Some(loaded) = load("signed-time") else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let program = &loaded.program;
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let signed_at = T0 - 30_000;
        let bounced =
            r#"{"type":"message.bounced","data":{"message_id":"m_1","address":"sam@example.com"}}"#;
        let (response, deferred) =
            send(&harness, program, &mail("msg_1", bounced, signed_at)).await;
        assert_eq!(
            response.status,
            204,
            "{:?}",
            String::from_utf8_lossy(&response.body)
        );
        // Nothing subscribes to a bounce: kept, with nothing to deliver.
        assert!(deferred.is_empty());
        let payload = harness
            .rows("SELECT \"payload\"::text FROM \"jadpo_outbox\"")
            .await;
        let [SqlValue::Text(payload)] = payload[0].as_slice() else {
            panic!("a payload: {payload:?}");
        };
        let payload: serde_json::Value = serde_json::from_str(payload).expect("JSON");
        assert_eq!(payload["occurred_at"], "2026-10-19T08:59:30Z");
        assert_eq!(payload["data"]["address"], "sam@example.com");
        // An event over 64 KB, every event's limit, isn't kept.
        let long = format!(
            r#"{{"type":"message.bounced","data":{{"message_id":"m_2","address":"{}"}}}}"#,
            "a".repeat(70_000)
        );
        let (response, _) = send(&harness, program, &mail("msg_2", &long, signed_at)).await;
        assert_eq!(
            response.status,
            413,
            "{:?}",
            String::from_utf8_lossy(&response.body)
        );
        assert_eq!(
            answer(&response)["error"]["message"],
            "The webhook's event is over 64 KB."
        );
        let received = harness
            .rows("SELECT \"source\", \"event_id\" FROM \"jadpo_received_events\"")
            .await;
        assert_eq!(
            received,
            vec![vec![
                SqlValue::Text("webhook mail".into()),
                SqlValue::Text("m_1".into())
            ]]
        );
        let (response, _) = send(
            &harness,
            program,
            &pay(&payment("evt_7", "pi_7", T0 / 1000), PAY_SECRET, T0),
        )
        .await;
        assert_eq!(response.status, 204);
        assert_eq!(count(&harness, "jadpo_received_events").await, 2);
        // A tick a day later keeps them; one thirty days and a minute after
        // they arrived deletes them.
        let ticked = tick(program, &harness, T0 + 24 * 60 * 60 * 1000).await;
        assert_eq!(ticked.deleted.received, 0);
        let later = T0 + 30 * 24 * 60 * 60 * 1000 + 60_000;
        harness.set_now(later);
        let ticked = tick(program as &dyn Executor, &harness, later).await;
        assert_eq!(ticked.deleted.received, 2, "{:?}", ticked.problems);
        assert_eq!(count(&harness, "jadpo_received_events").await, 0);
    });
}

/// `jadpo routes` lists each connection's webhook in the `webhooks` API,
/// run by the runtime and authenticated by its signature.
#[test]
fn jadpo_routes_lists_each_connections_webhook() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("webhooks-7.7-routes");
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, text);
    }
    let out = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    let out = jadpo(&root, &["routes"]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout,
        "webhooks  /webhooks\n  POST  /webhooks/mail      generated webhook  signature  generated\n  POST  /webhooks/payments  generated webhook  signature  generated\n\n2 routes: 1 API\n"
    );
}

/// `jadpo check` on the project with `changes`, each a file's path and
/// its text in place of the project's (`None` to remove it): every
/// problem's headline and rule, as `rule: headline` lines, but for the
/// migrations and records the change puts out of date.
fn check_with(name: &str, changes: &[(&str, Option<&str>)]) -> Vec<String> {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("webhooks-7.7-check-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        if changes.iter().all(|(changed, _)| changed != path) {
            write(&root, path, text);
        }
    }
    for (path, text) in changes {
        if let Some(text) = text {
            write(&root, path, text);
        }
    }
    let out = jadpo(&root, &["check", "--json"]);
    let report: serde_json::Value = serde_json::from_str(&out.stdout)
        .unwrap_or_else(|_| panic!("JSON: {}{}", out.stdout, out.stderr));
    report["diagnostics"]
        .as_array()
        .map(|all| {
            all.iter()
                .map(|d| {
                    format!(
                        "{}: {}",
                        d["rule"].as_str().unwrap_or_default(),
                        d["message"].as_str().unwrap_or_default()
                    )
                })
                .filter(|line| !line.starts_with("migration.") && !line.starts_with("record."))
                .collect()
        })
        .unwrap_or_default()
}

/// A case of a connection's checks: its name, `app.jadpo`, the other
/// files it changes, and the problems expected.
type ConnectionCase = (
    &'static str,
    String,
    Vec<(&'static str, Option<String>)>,
    Vec<&'static str>,
);

/// A case of an event's checks: its name, the files it writes, and the
/// problems expected.
type EventCase = (
    &'static str,
    &'static [(&'static str, &'static str)],
    &'static [&'static str],
);

const PAY_PROVIDER: &str = "shared/providers/acme_pay/v1/provider.jadpo";

/// `provider.jadpo` with `webhooks` as its `webhooks:` block's members.
fn pay_provider(webhooks: &str) -> String {
    format!(
        "/// Acme Pay's API.\nprovider {{\n    protocol: Protocol.json_http\n    hosts: {{\n        live: \"https://api.acme-pay.example\"\n        sandbox: \"https://sandbox.acme-pay.example\"\n    }}\n    credential: CredentialScheme.bearer\n    webhooks: {{\n{webhooks}\n    }}\n}}\n"
    )
}

/// A provider's `webhooks:` names a built-in scheme, `header:` only for
/// the one that reads one, and the places of each event's id and type;
/// a handshake is signed as its provider signs (chunk 7.7b).
#[test]
fn a_providers_webhooks_are_checked() {
    let cases: &[(&str, &str, &[&str])] = &[
        (
            "no-check",
            "        event_id: \"id\"\n        event_type: \"type\"",
            &["package.provider_invalid: `webhooks:` doesn't give `check:`"],
        ),
        (
            "unknown-scheme",
            "        check: SignatureScheme.paypal\n        event_id: \"id\"\n        event_type: \"type\"",
            &["package.provider_invalid: `SignatureScheme.paypal` isn't a signature scheme"],
        ),
        (
            "twilio",
            "        check: SignatureScheme.twilio\n        event_id: \"id\"\n        event_type: \"type\"",
            &[],
        ),
        (
            "header-unread",
            "        check: SignatureScheme.stripe\n        header: \"X-Signature\"\n        event_id: \"id\"\n        event_type: \"type\"",
            &["package.provider_invalid: `SignatureScheme.stripe` takes no `header:`"],
        ),
        (
            "header-missing",
            "        check: SignatureScheme.hmac_sha256\n        event_id: \"id\"\n        event_type: \"type\"",
            &["package.provider_invalid: `webhooks:` doesn't give `header:`"],
        ),
        (
            "places",
            "        check: SignatureScheme.stripe\n        event_id: 5\n        occurred_at: \"data..created\"",
            &[
                "package.provider_invalid: `webhooks:` doesn't give `event_type:`",
                "package.provider_invalid: `event_id:` isn't a place in the body",
                "package.provider_invalid: `occurred_at:` isn't a place in the body",
            ],
        ),
        (
            "handshake",
            "        check: SignatureScheme.stripe\n        event_id: \"id\"\n        event_type: \"type\"\n        handshake: WebhookHandshake.sns",
            &["package.provider_invalid: `WebhookHandshake.sns` needs `SignatureScheme.sns`"],
        ),
        (
            "unknown-key",
            "        check: SignatureScheme.stripe\n        event_id: \"id\"\n        event_type: \"type\"\n        signature: \"x\"",
            &["package.provider_invalid: `webhooks:` has no key `signature:`"],
        ),
    ];
    for (name, webhooks, expected) in cases {
        let provider = pay_provider(webhooks);
        let found = check_with(name, &[(PAY_PROVIDER, Some(&provider))]);
        assert_eq!(&found, expected, "{name}");
    }
}

/// A connection gives `webhooks: { secret: }` only to a package whose
/// provider sends them, from a secret setting; `replies:` names only
/// events that need a reply (chunk 7.7b); and its events need a store to
/// be kept in.
#[test]
fn a_connections_webhooks_are_checked() {
    let app = |payments_webhooks: &str| {
        FILES[0]
            .1
            .replace(
                "            webhooks: {\n                secret: config.payments_webhook_secret\n            }\n        }\n        mail",
                &format!("{payments_webhooks}        }}\n        mail"),
            )
    };
    let cases: Vec<ConnectionCase> = vec![
        (
            "no-secret",
            app("            webhooks: {}\n"),
            vec![],
            vec!["connection.key_missing: `payments`'s `webhooks:` doesn't give `secret:`"],
        ),
        (
            "not-secret",
            app("            webhooks: {\n                secret: config.payments_environment\n            }\n"),
            vec![],
            vec!["connection.value_invalid: `payments`'s webhook secret must be a secret setting"],
        ),
        (
            "replies",
            format!(
                "use billing\n\n{}",
                app("            webhooks: {\n                secret: config.payments_webhook_secret\n                replies: {\n                    payment_succeeded: billing.record_payment\n                }\n            }\n")
            ),
            vec![],
            vec!["connection.key_invalid: `payments`'s `replies:` has no event `payment_succeeded`"],
        ),
        (
            "unsent",
            app("            webhooks: {\n                secret: config.payments_webhook_secret\n            }\n"),
            vec![(
                PAY_PROVIDER,
                Some("/// Acme Pay's API.\nprovider {\n    protocol: Protocol.json_http\n    hosts: {\n        live: \"https://api.acme-pay.example\"\n        sandbox: \"https://sandbox.acme-pay.example\"\n    }\n    credential: CredentialScheme.bearer\n}\n".to_string()),
            )],
            vec![
                "connection.key_invalid: `payments` gives `webhooks:`, which `AcmePay.v1` doesn't take",
            ],
        ),
    ];
    for (name, app, mut more, expected) in cases {
        more.push(("app.jadpo", Some(app)));
        let changes: Vec<(&str, Option<&str>)> = more
            .iter()
            .map(|(path, text)| (*path, text.as_deref()))
            .collect();
        let found = check_with(name, &changes);
        assert_eq!(found, expected, "{name}");
    }
    // With no store of records, the events have nowhere to be kept.
    let found = check_with(
        "no-store",
        &[
            ("domains/billing/entities/payment.jadpo", None),
            (
                "domains/billing/actions/record_payment.jadpo",
                Some(
                    "use shared/providers/acme_pay\n\n/// Notes a payment Acme Pay took.\naction record_payment(event: AcmePay.PaymentSucceeded) -> AcmePay.PaymentId {\n    return event.payment_id\n}\n",
                ),
            ),
        ],
    );
    assert_eq!(
        found,
        vec![
            "event.store_required: `payments` receives webhooks but has no store to keep them in",
            "event.store_required: `mail` receives webhooks but has no store to keep them in",
        ]
    );
}

/// Each event of a package with webhooks gives the provider's name for it,
/// its own, and declares `occurred_at: Instant`, which the runtime fills;
/// an app's own event names no `wire_name:` until queues are received.
#[test]
fn a_webhooks_events_are_checked() {
    const EVENT: &str = "shared/providers/acme_pay/v1/events/payment_succeeded.jadpo";
    let cases: &[EventCase] = &[
        (
            "no-wire-name",
            &[(
                EVENT,
                "/// A payment was taken.\npublic type PaymentSucceeded = Event {\n    occurred_at: Instant\n    payment_id: PaymentId\n    pence: Int {\n        min: 0\n    }\n}\n",
            )],
            &[
                "package.wire_name_invalid: Webhook event `PaymentSucceeded` doesn't give `wire_name:`",
            ],
        ),
        (
            "same-wire-name",
            &[(
                "shared/providers/acme_pay/v1/events/payment_failed.jadpo",
                "/// A payment was refused.\npublic type PaymentFailed = Event {\n    wire_name: \"payment.succeeded\"\n    occurred_at: Instant\n}\n",
            )],
            &[
                "package.wire_name_invalid: `PaymentSucceeded` and `PaymentFailed` are both `payment.succeeded`",
            ],
        ),
        (
            "no-occurred-at",
            &[(
                EVENT,
                "/// A payment was taken.\npublic type PaymentSucceeded = Event {\n    wire_name: \"payment.succeeded\"\n    payment_id: PaymentId\n    pence: Int {\n        min: 0\n    }\n}\n",
            )],
            &[
                "type.unknown_field: `AcmePay.PaymentSucceeded` has no field `occurred_at`",
                "package.wire_invalid: Webhook event `PaymentSucceeded` doesn't declare `occurred_at: Instant`",
            ],
        ),
        (
            "occurred-at-sent",
            &[(
                EVENT,
                "/// A payment was taken.\npublic type PaymentSucceeded = Event {\n    wire_name: \"payment.succeeded\"\n    occurred_at: Instant {\n        wire: Wire.unix_seconds\n    }\n    payment_id: PaymentId\n    pence: Int {\n        min: 0\n    }\n}\n",
            )],
            &[
                "package.wire_invalid: Webhook event `PaymentSucceeded`'s `occurred_at` says how it is sent",
            ],
        ),
        (
            "app-event",
            &[(
                "domains/billing/events/refund_asked.jadpo",
                "/// A customer asked for a refund.\ntype RefundAsked = Event {\n    wire_name: \"refund.asked\"\n    reference: Text\n}\n",
            )],
            &[
                "toolchain.not_built_yet: An app's event names its `wire_name:` for a queue, which isn't built yet",
            ],
        ),
    ];
    for (name, changes, expected) in cases {
        let changes: Vec<(&str, Option<&str>)> = changes
            .iter()
            .map(|(path, text)| (*path, Some(*text)))
            .collect();
        let found = check_with(name, &changes);
        assert_eq!(&found, expected, "{name}");
    }
}

/// Declared, the `webhooks` API takes its own prefix, and its record and
/// OpenAPI document list each connection's webhook; an app route at a
/// webhook's path would never run.
#[test]
fn a_declared_webhooks_api_lists_each_webhook_and_keeps_its_paths() {
    let app = FILES[0].1.replace(
        "application {\n",
        "application {\n    apis: {\n        main: {\n            prefix: /\n        }\n        webhooks: {\n            prefix: /hooks\n        }\n    }\n",
    );
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("webhooks-7.7-declared");
    let _ = fs::remove_dir_all(&root);
    for (path, text) in FILES {
        write(&root, path, if *path == "app.jadpo" { &app } else { text });
    }
    for args in [
        &["migrate", "new", "initial"][..],
        &["records", "update"],
        &["check"],
    ] {
        let out = jadpo(&root, args);
        assert_eq!(out.code, 0, "{args:?}: {}{}", out.stdout, out.stderr);
    }
    let record = fs::read_to_string(root.join("api/webhooks.jadpo_record")).expect("the record");
    assert!(
        record.contains(
            "POST /hooks/payments\n    generated webhook\n    connection payments\n    authentication signature\n    status 204\n"
        ),
        "{record}"
    );
    assert!(record.contains("POST /hooks/mail\n"), "{record}");
    let document: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("build/openapi/webhooks.json")).expect("the document"),
    )
    .expect("JSON");
    let operation = &document["paths"]["/hooks/payments"]["post"];
    assert_eq!(operation["operationId"], "post_hooks_payments");
    assert_eq!(operation["x-jadpo-generated"], true);
    assert_eq!(operation["security"], serde_json::json!([]));
    assert!(operation["responses"]["204"].is_object(), "{operation}");
    assert!(operation["responses"]["401"].is_object(), "{operation}");
    let out = jadpo(&root, &["routes"]);
    assert!(
        out.stdout
            .contains("  POST  /hooks/payments      generated webhook  signature  generated\n"),
        "{}",
        out.stdout
    );
    // A hand-written route where a webhook is.
    write(
        &root,
        "domains/billing/actions/ping.jadpo",
        "/// Answers.\naction ping() -> Unit {\n    return\n}\n",
    );
    write(
        &root,
        "domains/billing/routes/hooks.jadpo",
        "use billing/actions { ping }\n\n/// A hand-written hook.\nroutes Hooks {\n    prefix: /hooks\n    api: webhooks\n\n    /// Pings.\n    POST /payments {\n        authentication: RouteAuthentication.none\n        status: SuccessStatus.no_content\n        run: ping()\n    }\n}\n",
    );
    let out = jadpo(&root, &["check"]);
    assert_eq!(out.code, 1, "{}", out.stdout);
    assert!(
        out.stdout
            .contains("error: `POST /hooks/payments` is where `payments` receives its webhooks"),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("rule: route.path_overlap"),
        "{}",
        out.stdout
    );
}
