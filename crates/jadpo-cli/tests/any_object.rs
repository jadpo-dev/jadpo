//! `AnyObject` (area 2, "Other built-in types"; area 11, "Types"): a
//! provider package's input field that takes any of the app's object
//! types and sends it as its JSON, as an email provider's template
//! variables and a payment provider's metadata need. A shop sends its
//! welcome email's variables, a receipt's and a price sheet's through one
//! operation of its mail provider's package, each its own object type.
//!
//! The project's tests run through `jadpo test` here, and compiled with
//! the interpreter's records in the gate's compiled step (`compiled.rs`,
//! "any of the app's objects sent to a provider"); the body each call
//! sends is read here from a provider started on this machine. Nothing
//! reaches a network.

use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use jadpo_cli::{Output, run};
use jadpo_drivers::package::JsonHttp;
use jadpo_host_native::Runtime;
use jadpo_host_native::fetch::NativeFetch;
use jadpo_interp::{Executor, Outcome, Prepared, Program};
use jadpo_runtime::capability::{Capability, Faked};
use jadpo_runtime::files::MemoryFiles;
use jadpo_runtime::{Clock, Ctx, Env, MemoryHost};
use jadpo_term::Environment;
use jadpo_value::Value;

use crate::support::fake_provider::FakeProvider;

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    run(&args, &env.with_cwd(cwd))
}

const MAILER: &str = "shared/providers/mailer/v1";

const APP: &str = r#"application {
    display_name: "Shop"
    connections: {
        sheets: {
            driver: Driver.disk
            path: "/var/lib/shop/sheets"
        }
        mailer: {
            package: Mailer.v1
            domains: [shop]
            credential: config.mailer_key
            environment: config.mailer_environment
        }
    }
}
"#;

const CONFIG: &str = r#"config {
    mailer_key: Text {
        secret: true
    }
    mailer_environment: ProviderEnvironment {
        default: ProviderEnvironment.sandbox
    }
}
"#;

const PACKAGE: &str = r#"/// A mail provider's templated email API.
package Mailer.v1 {
    licence: Licence.proprietary
    jadpo: "1.0"
}
"#;

const TEMPLATED_EMAIL: &str = r#"/// An email made from one of the account's templates.
public type TemplatedEmail = Object {
    to: Text
    template: Text
    /// What the template's placeholders read, by name.
    variables: AnyObject
    /// Kept with the email and sent back on its events.
    metadata: AnyObject?
}
"#;

const SENT: &str = r#"/// The email the provider accepted.
public type Sent = Object {
    id: Text
}
"#;

const EMAILS: &str = r#"public resource emails {
    send: {
        method: HttpMethod.post
        path: "/email/template"
        input: TemplatedEmail
        output: Sent
        changes: false
        source: PackageSource.written
    }
}
"#;

const SEND_WELCOME: &str = r#"use app { mailer }
use shared/providers/mailer
use shop/types { CustomerName, OrderCount, OrderNumber, OrderTag, WelcomeVariables }

/// Welcomes a customer with the welcome template.
action send_welcome(name: CustomerName, orders: OrderCount, order: OrderNumber) -> Mailer.Sent {
    return mailer.emails.send(Mailer.TemplatedEmail {
        to: "ada@example.test"
        template: "welcome"
        variables: WelcomeVariables {
            name: name
            orders: orders
            town: none
        }
        metadata: OrderTag {
            order: order
        }
    })
}
"#;

const SEND_RECEIPT: &str = r#"use app { mailer }
use shared/providers/mailer
use shop/types { CustomerName, ReceiptVariables }

/// Sends a receipt with the receipt template.
action send_receipt(name: CustomerName) -> Mailer.Sent {
    var variables: ReceiptVariables = ReceiptVariables {
        name: name
        lines: ["flour", "yeast"]
        paid: true
    }
    var email: Mailer.TemplatedEmail = Mailer.TemplatedEmail {
        to: "ada@example.test"
        template: "receipt"
        variables: variables
        metadata: none
    }
    return mailer.emails.send(email)
}
"#;

const SEND_SHEET: &str = r#"use app { mailer }
use shared/providers/mailer
use shop/types { SheetVariables }

/// Sends a price sheet with the sheet template.
action send_sheet(sheet: File) -> Mailer.Sent {
    return mailer.emails.send(Mailer.TemplatedEmail {
        to: "ada@example.test"
        template: "sheet"
        variables: SheetVariables {
            sheet: sheet
        }
        metadata: none
    })
}
"#;

const SEND_WELCOME_TEST: &str = r#"use app { mailer }
use shared/providers/mailer
use shop/actions { send_receipt }
use shop/types { ReceiptVariables }

test "a welcome's variables go to the provider" {
    fakes: {
        mailer.emails.send: [Mailer.Sent { id: "m_1" }]
    }
    var sent: Mailer.Sent = send_welcome("Ada", 3, "A-1")
    assert sent.id == "m_1"
    assert list.count(mailer.emails.send.requests) == 1
}

test "a receipt's go through the same operation" {
    fakes: {
        mailer.emails.send: [Mailer.Sent { id: "m_2" }]
    }
    var sent: Mailer.Sent = send_receipt("Ada")
    assert sent.id == "m_2"
    var requests: List<Mailer.TemplatedEmail> = mailer.emails.send.requests
    assert list.all(requests, item -> item.variables == ReceiptVariables {
        name: "Ada"
        lines: ["flour", "yeast"]
        paid: true
    })
}
"#;

/// The shop: its own object types, the mail provider's package with
/// `sandbox` as its sandbox's address, and the tests every executor runs.
fn files_at(sandbox: &str) -> Vec<(String, String)> {
    let owned = |path: &str, text: &str| (path.to_string(), text.to_string());
    let package = |path: &str, text: &str| (format!("{MAILER}/{path}"), text.to_string());
    vec![
        owned("app.jadpo", APP),
        owned("config.jadpo", CONFIG),
        package("package.jadpo", PACKAGE),
        (
            format!("{MAILER}/provider.jadpo"),
            format!(
                "/// How the mail provider is reached.\nprovider {{\n    protocol: Protocol.json_http\n    hosts: {{\n        live: \"https://api.mailer.example\"\n        sandbox: \"{sandbox}\"\n    }}\n    credential: CredentialScheme.bearer\n}}\n"
            ),
        ),
        package("types/templated_email.jadpo", TEMPLATED_EMAIL),
        package("types/sent.jadpo", SENT),
        package("resources/emails.jadpo", EMAILS),
        owned(
            "domains/shop/types/customer_name.jadpo",
            "/// A customer's name as they gave it.\ntype CustomerName = Text {\n    min_length: 1\n    max_length: 100\n}\n",
        ),
        owned(
            "domains/shop/types/town_name.jadpo",
            "/// The town an order goes to.\ntype TownName = Text {\n    min_length: 1\n    max_length: 100\n}\n",
        ),
        owned(
            "domains/shop/types/order_count.jadpo",
            "/// How many orders a customer has placed.\ntype OrderCount = Int {\n    min: 0\n    max: 1000\n}\n",
        ),
        owned(
            "domains/shop/types/order_number.jadpo",
            "/// An order's number as a customer reads it.\ntype OrderNumber = Text {\n    min_length: 1\n    max_length: 20\n}\n",
        ),
        owned(
            "domains/shop/types/line_name.jadpo",
            "/// What a receipt's line is for.\ntype LineName = Text {\n    min_length: 1\n    max_length: 100\n}\n",
        ),
        owned(
            "domains/shop/types/welcome_variables.jadpo",
            "use shop/types { CustomerName, OrderCount, TownName }\n\n/// What the welcome template reads.\ntype WelcomeVariables = Object {\n    name: CustomerName\n    orders: OrderCount\n    town: TownName?\n}\n",
        ),
        owned(
            "domains/shop/types/receipt_variables.jadpo",
            "use shop/types { CustomerName, LineName }\n\n/// What the receipt template reads.\ntype ReceiptVariables = Object {\n    name: CustomerName\n    lines: List<LineName>\n    paid: Bool\n}\n",
        ),
        owned(
            "domains/shop/types/sheet_variables.jadpo",
            "/// What the sheet template reads: the sheet itself.\ntype SheetVariables = Object {\n    sheet: File\n}\n",
        ),
        owned(
            "domains/shop/types/order_tag.jadpo",
            "use shop/types { OrderNumber }\n\n/// Which order an email is about.\ntype OrderTag = Object {\n    order: OrderNumber\n}\n",
        ),
        owned("domains/shop/actions/send_welcome.jadpo", SEND_WELCOME),
        owned("domains/shop/actions/send_receipt.jadpo", SEND_RECEIPT),
        owned("domains/shop/actions/send_sheet.jadpo", SEND_SHEET),
        owned(
            "domains/shop/actions/send_welcome.test.jadpo",
            SEND_WELCOME_TEST,
        ),
    ]
}

/// The project every executor runs, for `compiled.rs`.
pub(crate) fn files() -> Vec<(String, String)> {
    files_at("https://sandbox.mailer.example")
}

fn project(name: &str, files: Vec<(String, String)>) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("any-object-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files {
        write(&root, &path, &text);
    }
    // A project with errors writes no records; its check says so.
    let _ = jadpo(&root, &["records", "update"]);
    root
}

#[test]
fn any_of_the_apps_objects_passes_its_tests() {
    let root = project("tests", files());
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("2 passed"), "{}", tested.stdout);
}

/// The project checked, with the mail provider's sandbox at `sandbox`.
fn program(name: &str, sandbox: &str) -> Program {
    let root = project(name, files_at(sandbox));
    let loaded = jadpo_db::load(&root);
    let prepared = Prepared::load(&loaded.db, loaded.project).unwrap_or_else(|problems| {
        panic!(
            "the project checks: {:#?}",
            problems.iter().map(|p| &p.message).collect::<Vec<_>>()
        )
    });
    let variables = |name: &str| -> Option<String> {
        match name {
            "MAILER_KEY" => Some("mailer-test-key".into()),
            "MAILER_ENVIRONMENT" => Some("sandbox".into()),
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

/// Each of the app's object types goes out as its own JSON, as the
/// app's contract writes it, with an optional field that is `none` left
/// out as a provider's own are; a `File` one holds goes as its bytes, never
/// its store and key.
#[test]
fn each_object_goes_to_the_provider_as_its_json() {
    let provider = FakeProvider::start();
    let program = program("provider", &provider.url);
    let runtime = Runtime::new().expect("a runtime");
    let files = Rc::new(MemoryFiles::new());
    let sheet = files.keep("sheets", b"flour,1.20\n", "csv");
    let driver: Rc<dyn Capability> = Rc::new(JsonHttp::new(NativeFetch::new()));
    let host = Faked::new(MemoryHost::new())
        .with("mailer", driver)
        .with("sheets", Rc::clone(&files) as Rc<dyn Capability>);
    let call = |action: &str, args: Vec<Value>| {
        let mut ctx = Ctx::of(
            &program,
            &host,
            Env {
                seed: Some(7),
                clock: Clock::TEST,
            },
        );
        let id = program.plan().find(action).expect("the action");
        runtime.block_on(program.run(&mut ctx, id, args))
    };

    provider.answer(200, r#"{"id": "m_1"}"#);
    let outcome = call(
        "send_welcome",
        vec![Value::text("Ada"), Value::Int(3), Value::text("A-1")],
    );
    assert!(matches!(outcome, Outcome::Value(_)), "{outcome:?}");
    provider.answer(200, r#"{"id": "m_2"}"#);
    let outcome = call("send_receipt", vec![Value::text("Ada")]);
    assert!(matches!(outcome, Outcome::Value(_)), "{outcome:?}");
    provider.answer(200, r#"{"id": "m_3"}"#);
    let outcome = call("send_sheet", vec![sheet.clone()]);
    assert!(matches!(outcome, Outcome::Value(_)), "{outcome:?}");

    let sent = provider.requests();
    assert_eq!(sent.len(), 3);
    assert_eq!(
        sent[0].json(),
        serde_json::json!({
            "to": "ada@example.test",
            "template": "welcome",
            "variables": {"name": "Ada", "orders": 3},
            "metadata": {"order": "A-1"},
        })
    );
    assert_eq!(
        sent[1].json(),
        serde_json::json!({
            "to": "ada@example.test",
            "template": "receipt",
            "variables": {"name": "Ada", "lines": ["flour", "yeast"], "paid": true},
        })
    );
    assert_eq!(
        sent[2].json(),
        serde_json::json!({
            "to": "ada@example.test",
            "template": "sheet",
            "variables": {"sheet": jadpo_value::encoding::to_base64(b"flour,1.20\n")},
        })
    );
    let key = jadpo_runtime::files::stored(&sheet).expect("a file").key;
    assert!(!sent[2].body.contains(&key), "{}", sent[2].body);
}

/// What `jadpo check` says of the project once `path` reads `text`.
fn checked_with(name: &str, path: &str, text: &str) -> Output {
    let mut files = files();
    files.retain(|(at, _)| at != path);
    files.push((path.to_string(), text.to_string()));
    let root = project(name, files);
    jadpo(&root, &["check"])
}

/// Only an object type takes an `AnyObject`'s place: text is
/// `type.mismatch`, saying what goes there.
#[test]
fn text_is_not_an_object() {
    let done = checked_with(
        "text",
        "domains/shop/actions/send_receipt.jadpo",
        &SEND_RECEIPT.replace("variables: variables", "variables: name"),
    );
    assert_eq!(done.code, 1, "{}{}", done.stdout, done.stderr);
    assert!(
        done.stdout.contains("rule: type.mismatch"),
        "{}",
        done.stdout
    );
    assert!(
        done.stdout
            .contains("`CustomerName` can't be used where `AnyObject` is required"),
        "{}",
        done.stdout
    );
    assert!(
        done.stdout
            .contains("one of the application's own object types"),
        "{}",
        done.stdout
    );
}

/// A `Password` in the object goes out as one would in the package's own
/// input: `type.password_sent_to_provider`, at the object it is in.
#[test]
fn a_password_in_the_object_is_refused() {
    let mut files = files();
    files.push((
        "domains/shop/types/reset_variables.jadpo".into(),
        "use shop/types { CustomerName }\n\n/// What the reset template reads.\ntype ResetVariables = Object {\n    name: CustomerName\n    password: Password\n}\n".into(),
    ));
    files.push((
        "domains/shop/actions/send_reset.jadpo".into(),
        "use app { mailer }\nuse shared/providers/mailer\nuse shop/types { ResetVariables }\n\n/// Sends a new password.\naction send_reset(variables: ResetVariables) -> Mailer.Sent {\n    return mailer.emails.send(Mailer.TemplatedEmail {\n        to: \"ada@example.test\"\n        template: \"reset\"\n        variables: variables\n        metadata: none\n    })\n}\n".into(),
    ));
    let root = project("password", files);
    let done = jadpo(&root, &["check"]);
    assert_eq!(done.code, 1, "{}{}", done.stdout, done.stderr);
    assert!(
        done.stdout.contains("rule: type.password_sent_to_provider"),
        "{}",
        done.stdout
    );
    assert!(
        done.stdout.contains("`ResetVariables` holds a `Password`"),
        "{}",
        done.stdout
    );
}

/// An `AnyObject` is only a provider package's input field: the app's own
/// type, and a package's answer, are `type.any_object_outside_input`.
#[test]
fn any_object_outside_an_input_is_refused() {
    let done = checked_with(
        "own",
        "domains/shop/types/order_tag.jadpo",
        "/// Which order an email is about.\ntype OrderTag = Object {\n    order: AnyObject\n}\n",
    );
    assert_eq!(done.code, 1, "{}{}", done.stdout, done.stderr);
    assert!(
        done.stdout.contains("rule: type.any_object_outside_input"),
        "{}",
        done.stdout
    );

    let done = checked_with(
        "answer",
        &format!("{MAILER}/types/sent.jadpo"),
        "/// The email the provider accepted.\npublic type Sent = Object {\n    id: Text\n    metadata: AnyObject?\n}\n",
    );
    assert_eq!(done.code, 1, "{}{}", done.stdout, done.stderr);
    assert!(
        done.stdout.contains("rule: type.any_object_outside_input"),
        "{}",
        done.stdout
    );
    assert!(done.stdout.contains("`output:`"), "{}", done.stdout);

    let done = checked_with(
        "list",
        &format!("{MAILER}/types/templated_email.jadpo"),
        &TEMPLATED_EMAIL.replace("metadata: AnyObject?", "metadata: List<AnyObject>"),
    );
    assert_eq!(done.code, 1, "{}{}", done.stdout, done.stderr);
    assert!(
        done.stdout.contains("rule: type.any_object_outside_input"),
        "{}",
        done.stdout
    );
}
