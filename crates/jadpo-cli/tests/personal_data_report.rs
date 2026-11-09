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

What the app holds about people, who may read it, how long it is kept, what erasing someone does to it and where it goes. Written by `jadpo check`.

This is most of a record of processing activities (Article 30). Your team adds what the compiler can't know: the purpose of each use, the lawful basis and the organisation's details.

## Person `User`

| Field | Category | Store | Who may read it | Kept for |
|---|---|---|---|---|
| `User.display_name` | `identifying` | `main` | `Access.public` | kept until erased |

### Erasing a `User`

| Record | Reached through | Store | What happens |
|---|---|---|---|
| `User` | the person | `main` | Deleted. |

## Stores

Each store is one database in one place. An app that must keep data in a region runs one deployment per region, each with its own stores.

| Store | Holds | Where it is set |
|---|---|---|
| `main` | database | not set: no setting says where it is |

## Where personal data goes

Each connection a field marked `personal:`, or a value of a standard type that is personal data, is sent through, and the hosts that connection reaches. A value counts as sent when it may be in what goes out, may have decided it, or was sent to a provider whose answer it may be in. Each connection says whether it erases its copy when a person is erased.

### `pay`

- `https://api.acme-pay.example` when `config.pay_environment` is `live`
- `https://sandbox.acme-pay.example` when `config.pay_environment` is `sandbox`

**Erasure:** kept after a person is erased, since `AcmePay.v1` declares no `erasure:`, and no acceptance says why.

| Personal data | Category | Sent by | How |
|---|---|---|---|
| `AcmePay.CustomerRequest.notes` | `health`, sensitive | operation `customers.create` | sent |
| `User.display_name` | `identifying` | operation `customers.create` | sent |

## Step records

An action with several changing calls records each call's answer, encrypted, until the action commits. An answer over 1 MB is a fault.

No action makes several changing calls.

## OAuth grants

No connection holds an OAuth grant: grants aren't built yet.

## What this report covers

- **Covered by the language:** minimisation and storage limitation, access, rectification, by ordinary updates, erasure, portability, privacy by design, records of processing, most of Article 30, security of processing.
- **Supported, but the app's choice:** restriction and objection, consent.
- **Not code at all:** the lawful basis, the privacy notice, data protection impact assessments (DPIAs), processor contracts, the data protection officer (DPO).
- **Still to add:** breach notification, which lists the people whose marked data a breached store or provider held, international transfers, where provider packages declare where data is processed and this report shows it.

This is not legal advice. A privacy professional reviews this report.
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

/// What only decided what was sent says so, and a store on the server's own
/// disk is told apart from a connection whose hosts aren't known.
#[test]
fn the_report_says_how_each_value_goes_and_where() {
    let root = project("personal-report-how", "user.plan");
    write(
        &root,
        "app.jadpo",
        "application {\n    connections: {\n        main: {\n            driver: Driver.postgres\n        }\n        papers: {\n            driver: Driver.disk\n            path: \"/var/lib/papers\"\n        }\n        pay: {\n            package: AcmePay.v1\n            domains: [billing]\n            credential: config.pay_api_key\n            environment: config.pay_environment\n        }\n    }\n}\n",
    );
    write(
        &root,
        SEND,
        "use app { pay }\nuse billing/entities { User, UserNotFound }\nuse shared/providers/acme_pay\n\n/// Sends a user to Acme Pay.\naction send(user_id: User.id) fails UserNotFound -> AcmePay.CustomerReply {\n    var user: User = attempt User.get(user_id)\n    if (user.display_name == \"x\") {\n        return pay.customers.create(AcmePay.CustomerRequest {\n            name: \"a\"\n            notes: \"b\"\n        })\n    }\n    return pay.customers.create(AcmePay.CustomerRequest {\n        name: \"c\"\n        notes: \"d\"\n    })\n}\n",
    );
    write(
        &root,
        "domains/billing/failures/no_room.jadpo",
        "/// The store is full.\ntype NoRoom = Failure {\n    kind: FailureKind.unavailable\n}\n",
    );
    write(
        &root,
        "domains/billing/actions/keep.jadpo",
        "use app { papers }\nuse billing/entities { User, UserNotFound }\nuse billing/failures { NoRoom }\n\n/// Keeps a user's name as a file.\naction keep(user_id: User.id) fails UserNotFound, NoRoom -> File {\n    var user: User = attempt User.get(user_id)\n    return attempt papers.put(encoding.to_utf8(user.display_name), FileType.csv) {\n        storage_full: NoRoom\n    }\n}\n",
    );
    jadpo(&root, &["records", "update"]);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let report = fs::read_to_string(root.join("build/report/personal-data.md")).expect("report");
    for line in [
        "### `papers`\n\nKeeps its files on the server's own disk, so it reaches no host.\n",
        "| `User.display_name` | `identifying` | a file in the store | sent |",
        "| `User.display_name` | `identifying` | operation `customers.create` | decided what was sent |",
    ] {
        assert!(report.contains(line), "{line}\n\n{report}");
    }
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
    for line in [
        "No entity says `person: true`, so no record is held about a person.\n",
        "No personal data leaves through a connection.\n",
    ] {
        assert!(report.contains(line), "{line}\n\n{report}");
    }
    let json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("build/report/personal-data.json")).expect("twin"),
    )
    .expect("JSON");
    assert_eq!(json["connections"], serde_json::json!([]));
}

const HELD_APP: &[(&str, &str)] = &[
    (
        "app.jadpo",
        "application {\n    privacy: {\n        erasure_log: {\n            store: papers\n        }\n    }\n    connections: {\n        main: {\n            driver: Driver.postgres\n            url: config.database_url\n        }\n        papers: {\n            driver: Driver.disk\n            path: \"/var/lib/papers\"\n        }\n    }\n}\n",
    ),
    (
        "config.jadpo",
        "config {\n    database_url: PostgresUrl {\n        secret: true\n    }\n    invoice_retention: Duration {\n        default: 2190d\n    }\n}\n",
    ),
    (
        "domains/shop/types/note.jadpo",
        "/// Words someone wrote.\ntype Note = Text {\n    max_length: 200\n}\n",
    ),
    (
        "domains/shop/entities/user.jadpo",
        "use shop/types { Note }\n\n/// Someone with an account.\nentity User {\n    id: Uuid\n    email: Email\n    medical_note: Note? {\n        personal: PersonalData.health\n    }\n    person: true\n\n    policy: {\n        read: [Access.authenticated]\n    }\n}\n",
    ),
    (
        "domains/shop/entities/order.jadpo",
        "use shop/entities { User }\nuse shop/types { Note }\n\n/// Something they bought.\nentity Order {\n    id: Uuid\n    buyer_id: User.id {\n        on_delete: OnDelete.cascade\n    }\n    delivery_note: Note {\n        personal: PersonalData.location\n    }\n\n    policy: {\n        read: [Access.public]\n    }\n}\n",
    ),
    (
        "domains/shop/entities/invoice.jadpo",
        "use shop/entities { User }\n\n/// What a person was charged, kept for the tax office.\nentity Invoice {\n    id: Uuid\n    buyer_id: User.id? {\n        on_delete: OnDelete.restricted\n        erase: Erasure.retained\n    }\n    issued_at: Instant\n\n    retention: {\n        after: config.invoice_retention\n        since: issued_at\n    }\n\n    policy: {\n        read: [Access.authenticated]\n    }\n}\n",
    ),
    (
        "domains/shop/queries/recent_orders.jadpo",
        "use shop/entities { Order, User }\nuse shop/types { Note }\n\n/// An order as a person sees it.\ntype OrderLine = Output {\n    delivery_note: Note\n}\n\n/// A person's recent orders.\nquery recent_orders(buyer: User.id) -> List<OrderLine> {\n    from: Order\n    where: Order.buyer_id == buyer\n    order: [Order.id desc]\n    limit: 20\n}\n",
    ),
    (
        "domains/shop/actions/contact.jadpo",
        "use shop/entities { User, UserNotFound }\n\n/// What a person gave us.\naction contact(user: User.id) fails UserNotFound -> Email {\n    var person: User = attempt User.get(user)\n    if (person.medical_note != none) {\n        return person.email\n    }\n    return person.email\n}\n",
    ),
];

/// For each person the report lists every marked field with its category,
/// store, who may read it and its retention, and what erasure does to
/// each record; then each store with what says where it is.
#[test]
fn the_report_lists_what_each_person_entity_holds() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("personal-report-held");
    let _ = fs::remove_dir_all(&root);
    for (path, text) in HELD_APP {
        write(&root, path, text);
    }
    jadpo(&root, &["migrate", "new", "initial"]);
    jadpo(&root, &["records", "update"]);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let report = fs::read_to_string(root.join("build/report/personal-data.md")).expect("report");
    for line in [
        "## Person `User`\n",
        "| `User.email` | `contact` (an `Email`) | `main` | `Access.authenticated` | kept until erased |",
        "| `User.medical_note` | `health`, sensitive | `main` | `Access.authenticated` | kept until erased |",
        "| `Order.delivery_note` | `location`, sensitive | `main` | `Access.public` | kept until erased |",
        "### Erasing a `User`\n",
        "| `User` | the person | `main` | Deleted. |",
        "| `Order` | `buyer_id` | `main` | Deleted. |",
        "| `Invoice` | `buyer_id` | `main` | Kept until its retention ends. Clears the reference. |",
        "| `main` | database | `config.database_url` |",
        "| `papers` | files | the `path:` its connection gives in `app.jadpo` |",
    ] {
        assert!(report.contains(line), "{line}\n\n{report}");
    }
    let json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("build/report/personal-data.json")).expect("twin"),
    )
    .expect("JSON");
    let person = &json["people"][0];
    assert_eq!(person["entity"], "User");
    let email = &person["fields"][0];
    assert_eq!(email["entity"], "User");
    assert_eq!(email["field"], "email");
    assert_eq!(email["categories"][0], "contact");
    assert_eq!(email["sensitive"], false);
    assert_eq!(email["store"], "main");
    assert_eq!(email["read_by"], "`Access.authenticated`");
    assert_eq!(email["kept_until_erased"], true);
    assert_eq!(email["caches"], serde_json::json!([]));
    let medical = &person["fields"][1];
    assert_eq!(medical["sensitive"], true);
    assert_eq!(person["erasure"][0]["entity"], "User");
    assert_eq!(person["erasure"][0]["outcome"], "deleted");
    let invoice = person["erasure"]
        .as_array()
        .expect("erasure")
        .iter()
        .find(|row| row["entity"] == "Invoice")
        .expect("the invoice");
    assert_eq!(invoice["outcome"], "retained");
    assert_eq!(invoice["through"][0], "buyer_id");
    assert_eq!(json["stores"][0]["store"], "main");
    assert_eq!(json["stores"][0]["setting"], "database_url");
    assert_eq!(json["stores"][1]["store"], "papers");
    assert_eq!(json["stores"][1]["kind"], "files");
    assert_eq!(json["step_records"], serde_json::json!([]));
    assert_eq!(json["oauth_grants"], serde_json::json!([]));
    assert_eq!(json["coverage"]["not_code"][0], "the lawful basis");
    assert_eq!(json["coverage"]["app_choice"][1], "consent");
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
