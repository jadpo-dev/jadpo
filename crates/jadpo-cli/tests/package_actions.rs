//! A provider package's actions (area 11, "What package code may do";
//! chunk 7.6), run by the interpreter: the app calls `pay.start(...)`,
//! the package's action calls its own operations, one kept to itself, in
//! order through the connection the app called it through, each answer
//! passed to the next call, sent by the package driver over the native
//! host's HTTP to a provider started in the test on this machine. Its
//! keyed calls are steps of the calling action, so their keys carry on
//! the caller's numbering; its package failure is mapped at the app's
//! call by its code.

use std::path::Path;
use std::rc::Rc;

use jadpo_drivers::package::JsonHttp;
use jadpo_host_native::Runtime;
use jadpo_host_native::fetch::NativeFetch;
use jadpo_interp::{Executor, Outcome, Prepared, Program};
use jadpo_runtime::capability::{Capability, Faked};
use jadpo_runtime::{Clock, Ctx, Env, MemoryHost};
use jadpo_value::Value;

use crate::support::fake_provider::FakeProvider;

const HOME: &str = "shared/providers/acme_pay/v1";

fn files(sandbox: &str) -> Vec<(String, String)> {
    let package = |path: &str, text: &str| (format!("{HOME}/{path}"), text.to_string());
    vec![
        (
            "app.jadpo".to_string(),
            "application {\n    connections: {\n        pay: {\n            package: AcmePay.v1\n            domains: [billing]\n            credential: config.pay_api_key\n            environment: config.pay_environment\n        }\n    }\n    steps: {\n        encryption_key: config.step_encryption_key\n    }\n}\n".to_string(),
        ),
        (
            "config.jadpo".to_string(),
            "config {\n    pay_api_key: Text {\n        secret: true\n    }\n    pay_environment: ProviderEnvironment\n    step_encryption_key: SigningKey\n}\n".to_string(),
        ),
        package(
            "package.jadpo",
            "/// Acme Pay's payments API.\npackage AcmePay.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n",
        ),
        (
            format!("{HOME}/provider.jadpo"),
            format!(
                "/// Acme Pay's payments API.\nprovider {{\n    protocol: Protocol.json_http\n    hosts: {{\n        live: \"https://api.acme-pay.example\"\n        sandbox: \"{sandbox}\"\n    }}\n    credential: CredentialScheme.bearer\n    idempotency: {{\n        header: \"Idempotency-Key\"\n        window: 24h\n    }}\n    error_code: \"error.code\"\n}}\n"
            ),
        ),
        package(
            "types/pay_request.jadpo",
            "/// What each call takes.\npublic type PayRequest = Object {\n    reference: Reference\n}\n",
        ),
        package(
            "types/pay_reply.jadpo",
            "/// What each call gives.\npublic type PayReply = Object {\n    id: Reference\n}\n",
        ),
        package(
            "types/reference.jadpo",
            "/// Acme Pay's reference for a record.\npublic type Reference = Text {\n    max_length: 40\n}\n",
        ),
        package(
            "failures/card_declined.jadpo",
            "/// The card was declined.\npublic type CardDeclined = Failure {\n    kind: FailureKind.rejected\n    message: \"The card was declined.\"\n}\n",
        ),
        package(
            "resources/customers.jadpo",
            "resource customers {\n    create: {\n        method: HttpMethod.post\n        path: \"/v1/customers\"\n        input: PayRequest\n        output: PayReply\n        changes: true\n        idempotent: true\n        source: PackageSource.written\n    }\n}\n",
        ),
        package(
            "resources/cards.jadpo",
            "public resource cards {\n    attach: {\n        method: HttpMethod.post\n        path: \"/v1/cards\"\n        input: PayRequest\n        output: PayReply\n        changes: true\n        idempotent: true\n        failures: [CardDeclined]\n        source: PackageSource.written\n    }\n}\n",
        ),
        package(
            "actions/start.jadpo",
            "/// Creates the customer, then attaches the card to it.\npublic action start(input: PayRequest) fails CardDeclined -> PayReply {\n    var customer: PayReply = customers.create(input)\n    return attempt cards.attach(PayRequest {\n        reference: customer.id\n    })\n}\n",
        ),
        (
            "domains/billing/failures/payment_refused.jadpo".to_string(),
            "/// The payment provider refused the card.\ntype PaymentRefused = Failure {\n    kind: FailureKind.rejected\n    message: \"The card was refused.\"\n}\n".to_string(),
        ),
        (
            "domains/billing/actions/subscribe.jadpo".to_string(),
            "use app { pay }\nuse billing/failures { PaymentRefused }\nuse shared/providers/acme_pay\n\n/// Starts a subscription.\ndecline step.late_failure: \"An empty customer left by a declined card costs nothing\"\naction subscribe(plan: AcmePay.Reference) fails PaymentRefused -> AcmePay.PayReply {\n    return attempt pay.start(AcmePay.PayRequest {\n        reference: plan\n    }) {\n        card_declined: PaymentRefused\n    }\n}\n".to_string(),
        ),
        (
            "domains/billing/actions/subscribe_twice.jadpo".to_string(),
            "use app { pay }\nuse billing/failures { PaymentRefused }\nuse shared/providers/acme_pay\n\n/// Starts a subscription, then attaches a second card itself.\ndecline step.late_failure: \"An empty customer left by a declined card costs nothing\"\naction subscribe_twice(plan: AcmePay.Reference) fails PaymentRefused -> AcmePay.PayReply {\n    var started: AcmePay.PayReply = attempt pay.start(AcmePay.PayRequest {\n        reference: plan\n    }) {\n        card_declined: PaymentRefused\n    }\n    return attempt pay.cards.attach(AcmePay.PayRequest {\n        reference: started.id\n    }) {\n        card_declined: PaymentRefused\n    }\n}\n".to_string(),
        ),
    ]
}

/// The project, checked, with its settings: Acme Pay's sandbox at
/// `sandbox`.
fn program(root: &Path, sandbox: &str) -> Program {
    let _ = std::fs::remove_dir_all(root);
    for (path, text) in files(sandbox) {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        std::fs::write(path, text).expect("write");
    }
    let loaded = jadpo_db::load(root);
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
            "STEP_ENCRYPTION_KEY" => {
                Some("a-signing-key-for-the-record-of-steps-long-enough".into())
            }
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
    Program::new(prepared).with_config(&values)
}

fn root(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("jadpo-7.6-{name}-{}", std::process::id()))
}

fn host() -> Faked<MemoryHost> {
    let driver: Rc<dyn Capability> = Rc::new(JsonHttp::new(NativeFetch::new()));
    Faked::new(MemoryHost::new()).with("pay", driver)
}

fn env() -> Env {
    Env {
        seed: Some(7),
        clock: Clock::TEST,
    }
}

fn run(runtime: &Runtime, program: &Program, name: &str) -> Outcome {
    let host = host();
    let mut ctx = Ctx::of(program, &host, env());
    ctx.set_call_key("delivery", "d-1");
    let id = program.plan().find(name).expect("the action");
    runtime.block_on(program.run(&mut ctx, id, vec![Value::text("gold")]))
}

fn field<'v>(value: &'v Value, name: &str) -> &'v Value {
    match value {
        Value::Record(record) => record.get(name).expect("the field"),
        other => panic!("not a record: {other:?}"),
    }
}

/// The package's action calls its operation kept to itself, then its
/// public one with the first's answer, both to the connection's sandbox
/// with its credential and each under a key of its own.
#[test]
fn a_package_action_calls_its_own_operations_in_order() {
    let provider = FakeProvider::start();
    let program = program(&root("order"), &provider.url);
    let runtime = Runtime::new().expect("a runtime");
    provider.answer(201, r#"{"id": "cus_1"}"#);
    provider.answer(201, r#"{"id": "card_1"}"#);
    let outcome = run(&runtime, &program, "subscribe");
    let Outcome::Value(reply) = outcome else {
        panic!("{:?}", program.report(&outcome).map(|r| r.headline()));
    };
    assert_eq!(field(&reply, "id"), &Value::text("card_1"));
    let sent = provider.requests();
    let paths: Vec<&str> = sent.iter().map(|request| request.path.as_str()).collect();
    assert_eq!(paths, ["/v1/customers", "/v1/cards"]);
    assert!(sent[1].body.contains("cus_1"), "{}", sent[1].body);
    for request in &sent {
        assert_eq!(request.header("authorization"), Some("Bearer pk_test_123"));
    }
    let keys: Vec<&str> = sent
        .iter()
        .map(|request| request.header("idempotency-key").expect("a key"))
        .collect();
    assert_ne!(keys[0], keys[1], "each step its own key");
}

/// A package action's calls are steps of the calling action: the app's
/// own call after it is its third step, with a key of its own.
#[test]
fn the_calling_actions_steps_carry_on_after_the_package_actions() {
    let provider = FakeProvider::start();
    let program = program(&root("steps"), &provider.url);
    let runtime = Runtime::new().expect("a runtime");
    provider.answer(201, r#"{"id": "cus_1"}"#);
    provider.answer(201, r#"{"id": "card_1"}"#);
    provider.answer(201, r#"{"id": "card_2"}"#);
    let outcome = run(&runtime, &program, "subscribe_twice");
    assert!(matches!(outcome, Outcome::Value(_)), "{outcome:?}");
    let first = provider.requests();
    // The same action run again sends the same three keys.
    provider.answer(201, r#"{"id": "cus_1"}"#);
    provider.answer(201, r#"{"id": "card_1"}"#);
    provider.answer(201, r#"{"id": "card_2"}"#);
    let outcome = run(&runtime, &program, "subscribe_twice");
    assert!(matches!(outcome, Outcome::Value(_)), "{outcome:?}");
    let keys: Vec<String> = provider
        .requests()
        .iter()
        .map(|request| {
            request
                .header("idempotency-key")
                .expect("a key")
                .to_string()
        })
        .collect();
    assert_eq!(first.len(), 3);
    assert_eq!(keys.len(), 6);
    assert_ne!(keys[0], keys[1]);
    assert_ne!(keys[1], keys[2], "the app's own call is the third step");
    assert_ne!(keys[0], keys[2]);
    assert_eq!(keys[..3], keys[3..], "a rerun sends the same keys");
}

/// The package's failure, passed on by its action, is mapped at the app's
/// call by its code, the package's kept as the cause.
#[test]
fn a_package_actions_failure_is_mapped_at_the_apps_call() {
    let provider = FakeProvider::start();
    let program = program(&root("declined"), &provider.url);
    let runtime = Runtime::new().expect("a runtime");
    provider.answer(201, r#"{"id": "cus_1"}"#);
    provider.answer(402, r#"{"error": {"code": "card_declined"}}"#);
    let outcome = run(&runtime, &program, "subscribe");
    let Outcome::Failed(failure) = outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!(failure.code, "payment_refused");
    assert_eq!(
        failure.cause.as_ref().map(|cause| cause.code.as_str()),
        Some("card_declined")
    );
}
