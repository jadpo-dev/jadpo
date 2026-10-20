//! A model's answer in an action whose steps are recorded (area 21, "The
//! `ai` capability": "its answers are reused from the first run, kept as
//! area 12's step records keep providers' answers"; chunk 5.5v3), live
//! against PostgreSQL: a subscriber opens a customer at a payment
//! provider, asks a model on this machine for a welcome line, then starts
//! the subscription and keeps the account.
//!
//! Asked after the first changing call, the model's answer is a step of
//! the record, sealed like a provider's: when the subscription's call is
//! left in doubt and the delivery retried, the answer is replayed with
//! the customer's, and the model is never asked again, so the retry
//! neither spends twice nor answers differently.
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
use jadpo_runtime::capability::{Capability, Faked};
use jadpo_runtime::runner::run_deferred;
use jadpo_runtime::schedules::tick;
use jadpo_runtime::{Executor, Outcome, SqlValue};
use jadpo_term::Environment;
use jadpo_test::host::Harness;
use jadpo_value::Value;

use crate::support::fake_provider::FakeProvider;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

/// The project, its provider at `sandbox`.
fn files(sandbox: &str) -> Vec<(&'static str, String)> {
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
        local_model: {
            driver: Driver.openai_compatible
            url: config.model_url
        }
    }
    ai: {
        connection: local_model
        models: {
            default: config.drafting_model
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
    model_url: Url
    drafting_model: ModelId {
        default: "local/llama3.2"
    }
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
    }}
    error_code: "error.code"
}}
"#
            ),
        ),
        (
            "shared/providers/acme_pay/v1/resources/customers.jadpo",
            "public resource customers {\n    create: {\n        method: HttpMethod.post\n        path: \"/v1/customers\"\n        input: CustomerRequest\n        output: Customer\n        changes: true\n        idempotent: true\n        source: PackageSource.written\n    }\n}\n"
                .to_string(),
        ),
        (
            "shared/providers/acme_pay/v1/resources/subscriptions.jadpo",
            "public resource subscriptions {\n    create: {\n        method: HttpMethod.post\n        path: \"/v1/subscriptions\"\n        input: SubscriptionRequest\n        output: Subscription\n        changes: true\n        idempotent: true\n        source: PackageSource.written\n    }\n}\n"
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
            "/// A customer's subscription.\npublic type SubscriptionRequest = Object {\n    customer: Reference\n}\n"
                .to_string(),
        ),
        (
            "shared/providers/acme_pay/v1/types/subscription.jadpo",
            "/// A subscription Acme Pay keeps.\npublic type Subscription = Object {\n    id: Reference\n}\n"
                .to_string(),
        ),
        (
            "domains/billing/types/welcome_line.jadpo",
            "/// A line welcoming a customer.\ntype WelcomeLine = Text {\n    max_length: 200\n}\n"
                .to_string(),
        ),
        (
            "domains/billing/failures/assistant_unavailable.jadpo",
            "/// The assistant can't write now.\ntype AssistantUnavailable = Failure {\n    kind: FailureKind.unavailable\n}\n"
                .to_string(),
        ),
        (
            "domains/billing/prompts/welcome_customer.jadpo",
            "use shared/providers/acme_pay\n\n/// Welcomes a new customer.\ntype WelcomeCustomer = Prompt {\n    customer: AcmePay.Reference\n}\n"
                .to_string(),
        ),
        (
            "domains/billing/prompts/welcome_customer.prompt",
            "# Instructions\n\nWrite one line welcoming a new customer of a bakery.\n\n# Request\n\n{customer}\n"
                .to_string(),
        ),
        (
            "domains/billing/entities/account.jadpo",
            r#"use billing/types { WelcomeLine }
use shared/providers/acme_pay

/// A customer's account, its subscription and its welcome.
entity Account {
    id: Uuid
    customer: AcmePay.Reference
    subscription: AcmePay.Reference
    welcome: WelcomeLine

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
            "use shared/providers/acme_pay\n\n/// A customer chose a plan.\ntype PlanChosen = Event {\n    name: AcmePay.Reference\n}\n"
                .to_string(),
        ),
        (
            "domains/billing/actions/choose.jadpo",
            "use billing/events { PlanChosen }\nuse shared/providers/acme_pay\n\n/// A customer chooses a plan.\naction choose(name: AcmePay.Reference) -> Unit {\n    emit PlanChosen {\n        name: name\n    }\n}\n"
                .to_string(),
        ),
        (
            "domains/billing/actions/start.jadpo",
            r#"use app { pay }
use billing/entities { Account }
use billing/events { PlanChosen }
use billing/failures { AssistantUnavailable }
use billing/prompts { WelcomeCustomer }
use billing/types { WelcomeLine }
use shared/providers/acme_pay

/// Opens the customer, asks for a welcome, starts the subscription and
/// keeps the account: two changing calls with a model's answer between.
action start(event: PlanChosen) fails AssistantUnavailable -> Unit {
    var opened: AcmePay.Customer = pay.customers.create(AcmePay.CustomerRequest {
        reference: event.name
    })
    var welcome: WelcomeLine = attempt ai.generate(WelcomeCustomer { customer: event.name }, as: WelcomeLine) {
        refused: AssistantUnavailable
        malformed: AssistantUnavailable
        timed_out: AssistantUnavailable
        over_budget: AssistantUnavailable
        too_long: AssistantUnavailable
    }
    var subscription: AcmePay.Subscription = pay.subscriptions.create(AcmePay.SubscriptionRequest {
        customer: opened.id
    })
    var account: Account = Account.create {
        set: {
            customer: opened.id
            subscription: subscription.id
            welcome: welcome
        }
    }
    assert account.welcome == welcome
}
"#
            .to_string(),
        ),
        (
            "domains/billing/subscribers/start_subscription.jadpo",
            "use billing/actions { start }\nuse billing/events { PlanChosen }\n\n/// Starts the subscription a customer chose.\nsubscriber start_subscription {\n    on: PlanChosen\n    run: start(event)\n    retry: {\n        attempts: 10\n        within: 3d\n    }\n}\n"
                .to_string(),
        ),
    ]
}

/// When the test starts: 2026-10-20T09:00:00Z.
const T0: i64 = 1_792_486_800_000;
const HOUR: i64 = 60 * 60 * 1_000;
/// How many times a call that may have acted is sent in one delivery
/// attempt: the connection's default `retry:`, 3 attempts (area 11).
const TRIES: usize = 3;
const STEPS_KEY: &str = "a-signing-key-for-the-record-of-steps-long-enough";

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
    _root: PathBuf,
}

/// The project, checked and migrated, with a test database for it, its
/// model at `model`; `None` when live tests are skipped.
fn load(sandbox: &str, model: &str) -> Option<Loaded> {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("ai-steps-5.5v3");
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files(sandbox) {
        write(&root, path, &text);
    }
    for args in [
        &["migrate", "new", "initial"][..],
        &["records", "update"],
        &["check"],
    ] {
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
    let model_url = format!("{model}/v1");
    let variables = |name: &str| -> Option<String> {
        match name {
            "PAY_API_KEY" => Some("pk_test_123".into()),
            "PAY_ENVIRONMENT" => Some("sandbox".into()),
            "STEP_ENCRYPTION_KEY" => Some(STEPS_KEY.into()),
            "MODEL_URL" => Some(model_url.clone()),
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
        _root: root,
    })
}

/// A chat completion answering with `content`.
fn completion(content: &str) -> String {
    serde_json::json!({
        "choices": [{"index": 0, "message": {"role": "assistant", "content": content}, "finish_reason": "stop"}]
    })
    .to_string()
}

/// The model's answer, asked after the first changing call, is a step of
/// the record: sealed with the provider's, replayed when the delivery is
/// retried after the subscription's call was left in doubt, and never
/// asked of the model again.
#[test]
fn a_models_answer_is_a_step_a_retry_replays() {
    let provider = FakeProvider::start();
    let model = FakeProvider::start();
    let Some(loaded) = load(&provider.url, &model.url) else {
        return;
    };
    let harness = Harness::postgres(&loaded.url).at(T0).random_from(0);
    let driver: Rc<dyn Capability> = Rc::new(JsonHttp::new(NativeFetch));
    let host = Faked::new(&harness)
        .with("pay", Rc::clone(&driver))
        .with("local_model", driver);
    let program = &loaded.program;
    provider.answer(200, r#"{"id": "cus_4821"}"#);
    for _ in 0..TRIES {
        provider.answer(503, "");
    }
    provider.answer(200, r#"{"id": "sub_9930"}"#);
    model.answer(200, &completion("Welcome to the bakery, Ada!"));
    Runtime::new().expect("runtime").block_on(async {
        let id = program.plan().find("choose").expect("the action");
        let mut ctx = harness.ctx_on(program.places(), &host);
        let outcome = program.run(&mut ctx, id, vec![Value::text("Ada")]).await;
        assert!(matches!(outcome, Outcome::Value(_)), "{outcome:?}");
        let work = ctx.take_deferred();
        drop(ctx);
        run_deferred(program, &host, work).await;
        let kept = harness
            .rows("SELECT \"position\", \"operation\", \"outcome\", \"answer\" FROM \"jadpo_steps\" ORDER BY \"position\"")
            .await;
        let operations: Vec<(i32, String, Option<String>)> = kept
            .iter()
            .map(|row| match row.as_slice() {
                [SqlValue::Int4(position), SqlValue::Text(operation), outcome, _] => (
                    *position,
                    operation.clone(),
                    match outcome {
                        SqlValue::Text(text) => Some(text.clone()),
                        _ => None,
                    },
                ),
                other => panic!("not a step: {other:?}"),
            })
            .collect();
        assert_eq!(
            operations,
            [
                (1, "pay.customers.create".to_string(), Some("answered".to_string())),
                (
                    2,
                    "local_model.welcome_customer".to_string(),
                    Some("answered".to_string())
                ),
                (3, "pay.subscriptions.create".to_string(), None),
            ],
            "the model's answer kept as a step, the subscription in doubt"
        );
        let SqlValue::Bytes(sealed) = &kept[1][3] else {
            panic!("the model's answer is kept: {:?}", kept[1][3]);
        };
        assert!(
            !String::from_utf8_lossy(sealed).contains("Welcome"),
            "the answer is sealed"
        );
        assert_eq!(model.requests().len(), 1);

        // An hour later the retry: the customer and the welcome replayed,
        // the subscription sent again under its key.
        harness.set_now(T0 + HOUR);
        tick(program, &host, T0 + HOUR).await;
        assert_eq!(model.requests().len(), 1, "the model was never asked again");
        let accounts = harness
            .rows("SELECT \"customer\", \"subscription\", \"welcome\" FROM \"account\"")
            .await;
        assert_eq!(
            accounts,
            [vec![
                SqlValue::Text("cus_4821".into()),
                SqlValue::Text("sub_9930".into()),
                SqlValue::Text("Welcome to the bakery, Ada!".into()),
            ]]
        );
        assert!(
            harness
                .rows("SELECT 1 FROM \"jadpo_step_records\"")
                .await
                .is_empty(),
            "the record is deleted in the commit"
        );
    });
}
