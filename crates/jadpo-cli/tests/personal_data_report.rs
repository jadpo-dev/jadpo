//! The personal-data report (area 15, "The personal-data report"; chunk
//! V5): `jadpo check` writes `build/report/personal-data.md` and its
//! `.json` twin, every connection personal data leaves through and the
//! hosts it reaches, only for a project with no errors; and `jadpo package
//! check` says what a package's own operations send.

use std::fs;
use std::path::{Path, PathBuf};

use super::http::{jadpo, tempdir, write};
use super::packages::{git, plain};

const HOME: &str = "shared/providers/acme_pay/v1";

/// The package's files, relative to its folder.
const PACKAGE: &[(&str, &str)] = &[
    (
        "package.jadpo",
        "/// Acme Pay's payments API.\npackage AcmePay.v1 {\n    licence: Licence.mit\n    jadpo: \"1.0\"\n}\n",
    ),
    (
        "provider.jadpo",
        "/// Acme Pay's payments API.\nprovider {\n    protocol: Protocol.json_http\n    hosts: {\n        live: \"https://api.acme-pay.example\"\n        sandbox: \"https://sandbox.acme-pay.example\"\n    }\n    credential: CredentialScheme.bearer\n    idempotency: {\n        header: \"Idempotency-Key\"\n        window: 24h\n    }\n    error_code: \"error.code\"\n}\n",
    ),
    (
        "types/words.jadpo",
        "/// Words Acme Pay keeps.\npublic type Words = Text {}\n",
    ),
    (
        "types/customer_request.jadpo",
        "/// A customer to create.\npublic type CustomerRequest = Object {\n    /// Their name.\n    name: Words\n    /// Their account's own notes.\n    notes: Words {\n        personal: PersonalData.health\n    }\n}\n",
    ),
    (
        "types/customer_reply.jadpo",
        "/// The customer Acme Pay made.\npublic type CustomerReply = Object {\n    /// Acme Pay's id for them.\n    id: Words\n}\n",
    ),
    (
        "resources/customers.jadpo",
        "/// Customers.\npublic resource customers {\n    /// Creates a customer.\n    create: {\n        method: HttpMethod.post\n        path: \"/v1/customers\"\n        input: CustomerRequest\n        output: CustomerReply\n        changes: true\n        idempotent: true\n        source: PackageSource.written\n    }\n}\n",
    ),
];

const APP: &[(&str, &str)] = &[
    (
        "app.jadpo",
        "application {\n    connections: {\n        main: {\n            driver: Driver.postgres\n        }\n        pay: {\n            package: AcmePay.v1\n            domains: [billing]\n            credential: config.pay_api_key\n            environment: config.pay_environment\n        }\n    }\n}\n",
    ),
    (
        "config.jadpo",
        "config {\n    pay_api_key: Text {\n        secret: true\n    }\n    pay_environment: ProviderEnvironment\n}\n",
    ),
    (
        "domains/billing/entities/user.jadpo",
        "use shared/providers/acme_pay\n\n/// Someone who pays.\nentity User {\n    id: Uuid\n    display_name: AcmePay.Words {\n        personal: PersonalData.identifying\n    }\n    plan: AcmePay.Words\n    person: true\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n    }\n}\n",
    ),
];

/// `billing`'s action sending `name`.
fn send(name: &str) -> String {
    format!(
        "use app {{ pay }}\nuse billing/entities {{ User, UserNotFound }}\nuse shared/providers/acme_pay\n\n/// Sends a user to Acme Pay.\naction send(user_id: User.id) fails UserNotFound -> AcmePay.CustomerReply {{\n    var user: User = attempt User.get(user_id)\n    return pay.customers.create(AcmePay.CustomerRequest {{\n        name: {name}\n        notes: user.plan\n    }})\n}}\n"
    )
}

const SEND: &str = "domains/billing/actions/send.jadpo";

fn project(name: &str, send_name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&root);
    for (path, text) in APP {
        write(&root, path, text);
    }
    for (path, text) in PACKAGE {
        write(&root, &format!("{HOME}/{path}"), text);
    }
    write(&root, SEND, &send(send_name));
    jadpo(&root, &["migrate", "new", "initial"]);
    jadpo(&root, &["records", "update"]);
    root
}

#[test]
fn check_writes_where_personal_data_goes() {
    let root = project("personal-report", "user.display_name");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let report = fs::read_to_string(root.join("build/report/personal-data.md")).expect("report");
    assert_eq!(
        report,
        "# Personal data

Where the app's personal data goes: each connection a field marked `personal:`, or a value of a standard type that is personal data, is sent through, and the hosts that connection reaches. A value counts as sent when it may be in what goes out, may have decided it, or was sent to a provider whose answer it may be in. Written by `jadpo check`.

Not listed yet: what each store and cache keeps, who may read it and how long it is kept.

## `pay`

- `https://api.acme-pay.example` when `config.pay_environment` is `live`
- `https://sandbox.acme-pay.example` when `config.pay_environment` is `sandbox`

| Personal data | Category | Sent by | How |
|---|---|---|---|
| `AcmePay.CustomerRequest.notes` | `health`, sensitive | operation `customers.create` | sent |
| `User.display_name` | `identifying` | operation `customers.create` | sent |
"
    );
    let json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("build/report/personal-data.json")).expect("twin"),
    )
    .expect("JSON");
    let connection = &json["connections"][0];
    assert_eq!(connection["connection"], "pay");
    assert_eq!(
        connection["hosts"][0]["address"],
        "https://api.acme-pay.example"
    );
    assert_eq!(connection["hosts"][0]["when"]["is"], "live");
    let first = &connection["sent"][0];
    assert_eq!(first["host"], "AcmePay.CustomerRequest");
    assert_eq!(first["field"], "notes");
    assert_eq!(first["categories"][0], "health");
    assert_eq!(first["sensitive"], true);
    assert_eq!(first["through"], "operation");
    assert_eq!(first["name"], "customers.create");
    assert_eq!(first["decided_only"], false);
}

/// A project with errors may hold calls that can't be followed, so an
/// earlier report is taken away rather than left saying less than happens.
#[test]
fn a_project_with_errors_keeps_no_report() {
    let root = project("personal-report-errors", "user.display_name");
    assert_eq!(jadpo(&root, &["check"]).code, 0);
    assert!(root.join("build/report/personal-data.md").exists());
    write(&root, SEND, &send("user.nothing_of_the_kind"));
    assert_eq!(jadpo(&root, &["check"]).code, 1);
    assert!(!root.join("build/report/personal-data.md").exists());
    assert!(!root.join("build/report/personal-data.json").exists());
}

/// An app that sends nothing marked says so.
#[test]
fn an_app_that_sends_nothing_says_so() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("personal-report-empty");
    let _ = fs::remove_dir_all(&root);
    write(&root, "app.jadpo", "application {}\n");
    jadpo(&root, &["records", "update"]);
    assert_eq!(jadpo(&root, &["check"]).code, 0);
    let report = fs::read_to_string(root.join("build/report/personal-data.md")).expect("report");
    assert!(
        report.ends_with("No personal data leaves through a connection.\n"),
        "{report}"
    );
    let json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("build/report/personal-data.json")).expect("twin"),
    )
    .expect("JSON");
    assert_eq!(json["connections"], serde_json::json!([]));
}

/// A package's repository: `jadpo package check` shows what its operations
/// send, and `--json` the same.
#[test]
fn package_check_shows_what_a_package_sends() {
    let dir = tempdir::Dir::new("personal-package");
    let root = dir.path().join("acme_pay");
    for (path, text) in PACKAGE {
        write(&root, path, text);
    }
    git(&root, &["init", "-q", "-b", "main"]);
    let checked = plain(&root, &["package", "check"]);
    assert!(
        checked.stdout.contains(
            "Personal data sent
  to https://api.acme-pay.example
  to https://sandbox.acme-pay.example
  customers.create  CustomerRequest.notes  health, sensitive

Publishing bar
"
        ),
        "{}",
        checked.stdout
    );
    let json = plain(&root, &["package", "check", "--json"]);
    let json: serde_json::Value = serde_json::from_str(&json.stdout).expect("JSON");
    let facts = &json["data"]["personal_data"];
    assert_eq!(facts["hosts"][1], "https://sandbox.acme-pay.example");
    assert_eq!(facts["sends"][0]["operation"], "customers.create");
    assert_eq!(facts["sends"][0]["host"], "AcmePay.CustomerRequest");
    assert_eq!(facts["sends"][0]["field"], "notes");
    assert_eq!(facts["sends"][0]["categories"][0], "health");
    assert_eq!(facts["sends"][0]["decided_only"], false);
}
