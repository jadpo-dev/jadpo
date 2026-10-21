//! A standard type read from the members of the object that holds it
//! (area 11, "Mapping imported types to standard types"; chunk 7.2e): a
//! provider package's field gives `wire_parts:`, the spelling of each of
//! its standard type's parts among its object's own members, so Stripe's
//! `"amount": 1050, "currency": "gbp"` is read as one `Money` and sent
//! back as the two, and a shop's `"lat"` and `"lng"` as one `Point`.
//! Fields share a part's member, as a charge's amounts share its
//! currency. Each test compiles one project whose `payments` connection
//! reaches a provider started in the test on this machine; nothing
//! reaches a network.

use std::path::Path;
use std::rc::Rc;

use jadpo_drivers::package::JsonHttp;
use jadpo_host_native::Runtime;
use jadpo_host_native::fetch::NativeFetch;
use jadpo_interp::{Executor, Outcome, Prepared, Program};
use jadpo_runtime::capability::{Capability, Faked};
use jadpo_runtime::{Clock, Ctx, Env, FaultKind, MemoryHost};
use jadpo_value::Value;

use crate::support::fake_provider::FakeProvider;

const PACKAGE: &str = "shared/providers/tidepay/v1";

const CHARGE: &str = r#"/// A charge to a card.
public type Charge = Object {
    id: ChargeId
    amount: Amount {
        wire_parts: {
            amount: "amount"
            currency: "currency"
        }
    }
    amount_refunded: Amount {
        wire_parts: {
            amount: "amount_refunded"
            currency: "currency"
        }
    }
    shop: Point? {
        wire_parts: {
            latitude: "lat"
            longitude: "lng"
        }
    }
}
"#;

fn files(sandbox: &str) -> Vec<(String, String)> {
    let package = |path: &str, text: &str| (format!("{PACKAGE}/{path}"), text.to_string());
    let app = |path: &str, text: &str| (path.to_string(), text.to_string());
    vec![
        app(
            "app.jadpo",
            r#"application {
    connections: {
        payments: {
            package: Tidepay.v1
            domains: [billing]
            credential: config.payments_api_key
            environment: config.payments_environment
            retry: {
                attempts: 1
            }
        }
    }
}
"#,
        ),
        app(
            "config.jadpo",
            "config {\n    payments_api_key: Text {\n        secret: true\n    }\n    payments_environment: ProviderEnvironment\n}\n",
        ),
        package(
            "package.jadpo",
            "/// Tidepay's card payments.\npackage Tidepay.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n",
        ),
        package(
            "provider.jadpo",
            &format!(
                "/// Tidepay's API.\nprovider {{\n    protocol: Protocol.json_http\n    hosts: {{\n        live: \"https://api.tidepay.example\"\n        sandbox: \"{sandbox}\"\n    }}\n    credential: CredentialScheme.bearer\n}}\n"
            ),
        ),
        package(
            "resources/charges.jadpo",
            r#"public resource charges {
    get: {
        method: HttpMethod.get
        path: "/v1/charges/{id}"
        input: ChargeLookup
        output: Charge
        changes: false
        source: PackageSource.written
    }
    create: {
        method: HttpMethod.post
        path: "/v1/charges"
        input: Charge
        output: ChargeLookup
        changes: true
        source: PackageSource.written
    }
}
"#,
        ),
        package(
            "types/charge_lookup.jadpo",
            "/// Which charge.\npublic type ChargeLookup = Object {\n    id: ChargeId\n}\n",
        ),
        package(
            "types/charge_id.jadpo",
            "/// The provider's reference for a charge.\npublic type ChargeId = Text {\n    max_length: 40\n}\n",
        ),
        package(
            "types/amount.jadpo",
            "/// An amount charged.\npublic type Amount = Money {}\n",
        ),
        package("types/charge.jadpo", CHARGE),
        app(
            "domains/billing/actions/charge_of.jadpo",
            r#"use app { payments }
use shared/providers/tidepay

/// A charge, as the provider has it.
action charge_of(id: Tidepay.ChargeId) -> Tidepay.Charge {
    return payments.charges.get(Tidepay.ChargeLookup {
        id: id
    })
}
"#,
        ),
        app(
            "domains/billing/actions/amount_of.jadpo",
            r#"use app { payments }
use shared/providers/tidepay

/// Only a charge's amount.
action amount_of(id: Tidepay.ChargeId) -> Tidepay.Amount {
    var charge: Tidepay.Charge = payments.charges.get(Tidepay.ChargeLookup {
        id: id
    })
    return charge.amount
}
"#,
        ),
        app(
            "domains/billing/actions/charge.jadpo",
            r#"use app { payments }
use shared/providers/tidepay

/// Charges a card.
action charge(amount: Tidepay.Amount, refunded: Tidepay.Amount) -> Tidepay.ChargeLookup {
    return payments.charges.create(Tidepay.Charge {
        id: "new"
        amount: amount
        amount_refunded: refunded
        shop: none
    })
}
"#,
        ),
    ]
}

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
            "the project checks: {:#?}",
            problems
                .iter()
                .map(|p| format!("{}: {}", p.rule.id, p.message))
                .collect::<Vec<_>>()
        )
    });
    let variables = |name: &str| -> Option<String> {
        match name {
            "PAYMENTS_API_KEY" => Some("tp_test_123".into()),
            "PAYMENTS_ENVIRONMENT" => Some("sandbox".into()),
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
    std::env::temp_dir().join(format!("jadpo-7.2e-{name}-{}", std::process::id()))
}

fn run(runtime: &Runtime, program: &Program, name: &str, args: Vec<Value>) -> Outcome {
    let driver: Rc<dyn Capability> = Rc::new(JsonHttp::new(NativeFetch::new()));
    let host = Faked::new(MemoryHost::new()).with("payments", driver);
    let mut ctx = Ctx::new(
        program.places(),
        &host,
        Env {
            seed: Some(7),
            clock: Clock::TEST,
        },
    );
    let id = program.plan().find(name).expect("the action");
    runtime.block_on(program.run(&mut ctx, id, args))
}

fn field<'v>(value: &'v Value, name: &str) -> &'v Value {
    match value {
        Value::Record(record) => record.get(name).expect("the field"),
        other => panic!("not a record: {other:?}"),
    }
}

fn money(text: &str) -> Value {
    jadpo_value::standard::builtin_text(jadpo_std::Builtin::Money, text).expect("money")
}

fn fault(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Fault(fault) => match &fault.kind {
            FaultKind::Provider(what) => what.clone(),
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    }
}

/// Each part is read from its own member: both amounts from theirs, each
/// with the currency they share, and the shop's place from its latitude
/// and longitude.
#[test]
fn a_standard_type_is_read_from_its_parts() {
    let provider = FakeProvider::start();
    let program = program(&root("read"), &provider.url);
    let runtime = Runtime::new().expect("a runtime");
    provider.answer(
        200,
        r#"{"id": "ch_1", "amount": 1050, "amount_refunded": 250, "currency": "gbp", "lat": 51.5, "lng": -0.125, "livemode": false}"#,
    );
    let outcome = run(&runtime, &program, "charge_of", vec![Value::text("ch_1")]);
    let Outcome::Value(charge) = outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!(field(&charge, "amount"), &money("GBP 10.50"));
    assert_eq!(field(&charge, "amount_refunded"), &money("GBP 2.50"));
    assert_eq!(
        field(&charge, "shop").to_string(),
        "Point { latitude: 51.5, longitude: -0.125, altitude: none }",
        "{charge:?}"
    );
    // A charge with no shop has neither member.
    provider.answer(
        200,
        r#"{"id": "ch_2", "amount": 1050, "amount_refunded": 0, "currency": "GBP"}"#,
    );
    let outcome = run(&runtime, &program, "charge_of", vec![Value::text("ch_2")]);
    let Outcome::Value(charge) = outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!(field(&charge, "shop"), &Value::None);
}

/// A part missing is a fault naming the operation and the member, never
/// a value; a field code doesn't read isn't decoded.
#[test]
fn a_missing_part_is_named() {
    let provider = FakeProvider::start();
    let program = program(&root("missing"), &provider.url);
    let runtime = Runtime::new().expect("a runtime");
    let body = r#"{"id": "ch_1", "amount": 1050, "amount_refunded": "lots", "currency": "gbp"}"#;
    provider.answer(200, body);
    provider.answer(
        200,
        r#"{"id": "ch_1", "amount": 1050, "amount_refunded": 0}"#,
    );
    let amount = run(&runtime, &program, "amount_of", vec![Value::text("ch_1")]);
    assert_eq!(amount, Outcome::Value(money("GBP 10.50")), "{amount:?}");
    let missing = fault(&run(
        &runtime,
        &program,
        "charge_of",
        vec![Value::text("ch_1")],
    ));
    assert!(
        missing.contains("`payments.charges.get`'s answer doesn't decode")
            && missing.contains("currency"),
        "{missing}"
    );
}

/// A value is sent as its parts, the shared currency once.
#[test]
fn a_standard_type_is_written_as_its_parts() {
    let provider = FakeProvider::start();
    let program = program(&root("write"), &provider.url);
    let runtime = Runtime::new().expect("a runtime");
    provider.answer(200, r#"{"id": "ch_9"}"#);
    let outcome = run(
        &runtime,
        &program,
        "charge",
        vec![money("GBP 10.50"), money("GBP 0.00")],
    );
    assert!(matches!(outcome, Outcome::Value(_)), "{outcome:?}");
    let sent = provider.requests();
    assert_eq!(
        sent[0].json(),
        serde_json::json!({
            "id": "new",
            "amount": 1050,
            "amount_refunded": 0,
            "currency": "GBP",
            "lat": null,
            "lng": null
        })
    );
    // Two amounts that share the provider's one currency member can't be
    // in two currencies: the call isn't sent.
    let outcome = run(
        &runtime,
        &program,
        "charge",
        vec![money("GBP 10.50"), money("EUR 1.00")],
    );
    let refused = fault(&outcome);
    assert!(refused.contains("currency"), "{refused}");
    assert_eq!(provider.requests().len(), 1);
}

// --- what the checker refuses ---------------------------------------------------

fn problems(name: &str, charge: &str) -> Vec<(String, String)> {
    let root = root(name);
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in files("https://sandbox.tidepay.example") {
        let text = if path.ends_with("types/charge.jadpo") {
            charge.to_string()
        } else {
            text
        };
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        std::fs::write(path, text).expect("write");
    }
    let loaded = jadpo_db::load(&root);
    match Prepared::load(&loaded.db, loaded.project) {
        Ok(_) => Vec::new(),
        Err(problems) => problems
            .iter()
            .map(|problem| (problem.rule.id.to_string(), problem.message.clone()))
            .collect(),
    }
}

fn refused(name: &str, from: &str, to: &str, rule: &str, says: &str) {
    assert!(CHARGE.contains(from), "{from}");
    let found = problems(name, &CHARGE.replace(from, to));
    assert!(
        found.len() == 1 && found[0].0 == rule && found[0].1.contains(says),
        "{name}: expected one `{rule}` saying {says:?}, got {found:#?}"
    );
}

/// `wire_parts:` names every part of a standard type made of parts, each
/// once, as text, and is the field's one way of being sent.
#[test]
fn wire_parts_are_held_to_the_types_parts() {
    assert_eq!(problems("parts-fine", CHARGE), Vec::new());
    refused(
        "parts-missing",
        "            currency: \"currency\"\n        }\n    }\n    amount_refunded",
        "        }\n    }\n    amount_refunded",
        "package.wire_invalid",
        "`amount`'s `wire_parts` leaves out `currency`",
    );
    refused(
        "parts-unknown",
        "            latitude: \"lat\"",
        "            latitude: \"lat\"\n            elevation: \"alt\"",
        "package.wire_invalid",
        "`Point` has no part `elevation`",
    );
    refused(
        "parts-not-text",
        "            longitude: \"lng\"",
        "            longitude: lng",
        "package.wire_invalid",
        "`wire_parts` takes each part's spelling as text",
    );
    refused(
        "parts-not-standard",
        "    id: ChargeId\n",
        "    id: ChargeId {\n        wire_parts: {\n            text: \"id\"\n        }\n    }\n",
        "package.wire_invalid",
        "`wire_parts` is for a standard type made of parts",
    );
    refused(
        "parts-and-wire",
        "            currency: \"currency\"\n        }\n    }\n    amount_refunded",
        "            currency: \"currency\"\n        }\n        wire: Wire.minor_units\n    }\n    amount_refunded",
        "package.wire_invalid",
        "`amount` gives both `wire_parts:` and `wire:`",
    );
    // A field of its own can't take a member a part is read from; each
    // amount reading it is told.
    let clash: Vec<(String, String)> = problems(
        "parts-clash",
        &CHARGE.replace(
            "    id: ChargeId\n",
            "    id: ChargeId\n    currency: ChargeId?\n",
        ),
    )
    .into_iter()
    // The action that builds a charge doesn't give the new field.
    .filter(|(rule, _)| rule != "type.missing_field")
    .collect();
    assert!(
        clash.len() == 2
            && clash
                .iter()
                .all(|(rule, message)| rule == "package.wire_name_invalid"
                    && message.ends_with("are both `currency` on the wire")),
        "{clash:#?}"
    );
}

/// `jadpo test` on the Tidepay app with `test` beside `charge_of`: what
/// it prints, and its exit code.
fn tested(name: &str, test: &str) -> (String, u8) {
    let root = root(name);
    let _ = std::fs::remove_dir_all(&root);
    let mut all = files("https://sandbox.tidepay.example");
    // A test needs no credential, and runs against the sandbox.
    for (path, text) in &mut all {
        if path == "config.jadpo" {
            *text = text.replace(
                "    payments_environment: ProviderEnvironment\n",
                "    payments_environment: ProviderEnvironment {\n        default: ProviderEnvironment.sandbox\n    }\n",
            );
        }
    }
    all.push((
        "domains/billing/actions/charge_of.test.jadpo".to_string(),
        test.to_string(),
    ));
    for (path, text) in all {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        std::fs::write(path, text).expect("write");
    }
    let args: Vec<String> = ["test", ".", "--seed", "1"]
        .iter()
        .map(|arg| arg.to_string())
        .collect();
    let env = jadpo_term::Environment::empty().with_var("JADPO_PLAIN", "1");
    let output = jadpo_cli::run(&args, &env.with_cwd(&root));
    let stdout = crate::support::without_run_time(&output.stdout);
    (format!("{stdout}{}", output.stderr), output.code)
}

/// Unscripted in a test, the call answers with its generated default,
/// written as the provider sends it, each part its own member, and read
/// back through the same decoding.
#[test]
fn a_fakes_default_answer_sends_the_parts() {
    let (out, code) = tested(
        "fake",
        "use shared/providers/tidepay\n\ntest \"a charge is read from its parts\" {\n    var found: Tidepay.Charge = charge_of(\"ch_1\")\n    assert found.shop == none\n    assert found.amount == found.amount_refunded\n}\n",
    );
    assert_eq!(
        (out.as_str(), code),
        ("tested: 1 passed, 1 action test\n", 0)
    );
}

/// A test's scripted answer is written as the provider sends it too, each
/// part its own member, so the call's decoding reads it back (found in
/// chunk 7.8b: it was written as the contract writes it, with the amount
/// whole and no `currency` member).
#[test]
fn a_scripted_answer_sends_the_parts() {
    let (out, code) = tested(
        "fake-scripted",
        "use app { payments }\nuse shared/providers/tidepay\n\ntest \"a scripted charge is read from its parts\" {\n    fakes: {\n        payments.charges.get: [\n            Tidepay.Charge {\n                id: \"ch_2\"\n                amount: Tidepay.Amount(money.from_minor(1050, Currency.gbp))\n                amount_refunded: Tidepay.Amount(money.from_minor(0, Currency.gbp))\n                shop: none\n            }\n        ]\n    }\n    var found: Tidepay.Charge = charge_of(\"ch_2\")\n    assert found.amount == Tidepay.Amount(money.from_minor(1050, Currency.gbp))\n    assert found.shop == none\n}\n",
    );
    assert_eq!(
        (out.as_str(), code),
        ("tested: 1 passed, 1 action test\n", 0)
    );
}
