//! Personal data a connection can't erase (area 15, "Erasure at providers";
//! area 18, `ErasureNotPossible`; chunk V6), end to end through git: a
//! connection that receives personal data, with no `erasure:` to ask it to
//! erase, fails `jadpo check` until a person accepts it with the reason the
//! data stays; the report lists the connection and the reason; and the
//! approval report shows the report's difference.

use std::fs;
use std::path::{Path, PathBuf};

use crate::acceptances::{acceptance_files, edit, git, jadpo, write};

const HOME: &str = "shared/providers/acme_pay/v1";

/// The package's files, with or without its `erasure:`.
fn package(root: &Path, erasure: &str) {
    for (path, text) in [
        (
            "package.jadpo",
            "/// Acme Pay's payments API.\npackage AcmePay.v1 {\n    licence: Licence.mit\n    jadpo: \"1.0\"\n}\n".to_string(),
        ),
        (
            "provider.jadpo",
            format!("/// Acme Pay's payments API.\nprovider {{\n    protocol: Protocol.json_http\n    hosts: {{\n        live: \"https://api.acme-pay.example\"\n        sandbox: \"https://sandbox.acme-pay.example\"\n    }}\n    credential: CredentialScheme.bearer\n    idempotency: {{\n        header: \"Idempotency-Key\"\n        window: 24h\n    }}\n    error_code: \"error.code\"\n{erasure}}}\n"),
        ),
        (
            "types/words.jadpo",
            "/// Words Acme Pay keeps.\npublic type Words = Text {}\n".to_string(),
        ),
        (
            "types/customer_id.jadpo",
            "/// Acme Pay's id for a customer.\npublic type CustomerId = Text {}\n".to_string(),
        ),
        (
            "types/customer_request.jadpo",
            "/// A customer to create.\npublic type CustomerRequest = Object {\n    /// Their name.\n    name: Words\n}\n".to_string(),
        ),
        (
            "types/customer_key.jadpo",
            "/// A customer to delete.\npublic type CustomerKey = Object {\n    /// Acme Pay's id for them.\n    id: CustomerId\n}\n".to_string(),
        ),
        (
            "types/customer_reply.jadpo",
            "/// The customer Acme Pay made.\npublic type CustomerReply = Object {\n    /// Acme Pay's id for them.\n    id: CustomerId\n}\n".to_string(),
        ),
        (
            "resources/customers.jadpo",
            "/// Customers.\npublic resource customers {\n    /// Creates a customer.\n    create: {\n        method: HttpMethod.post\n        path: \"/v1/customers\"\n        input: CustomerRequest\n        output: CustomerReply\n        changes: true\n        idempotent: true\n        source: PackageSource.written\n    }\n\n    /// Deletes a customer.\n    delete: {\n        method: HttpMethod.delete\n        path: \"/v1/customers/{id}\"\n        input: CustomerKey\n        changes: true\n        idempotent: true\n        source: PackageSource.written\n    }\n\n    /// Reads a customer.\n    read: {\n        method: HttpMethod.get\n        path: \"/v1/customers/{id}\"\n        input: CustomerKey\n        output: CustomerReply\n        source: PackageSource.written\n    }\n}\n".to_string(),
        ),
    ] {
        write(root, &format!("{HOME}/{path}"), &text);
    }
}

const ERASURE: &str =
    "    erasure: {\n        call: customers.delete\n        by: CustomerId\n    }\n";

/// `application { }` with the connection's extra lines.
fn app(root: &Path, extra: &str) {
    let used = if extra.contains("key: User.") {
        "use billing/entities { User }\n\n"
    } else if extra.contains("key: Invoice.") {
        "use billing/entities { Invoice }\n\n"
    } else {
        ""
    };
    write(
        root,
        "app.jadpo",
        &format!(
            "{used}application {{\n    connections: {{\n        main: {{\n            driver: Driver.postgres\n        }}\n        pay: {{\n            package: AcmePay.v1\n            domains: [billing]\n            credential: config.pay_api_key\n            environment: config.pay_environment\n{extra}        }}\n    }}\n}}\n"
        ),
    );
}

const KEY: &str =
    "            erasure: {\n                key: User.pay_customer_id\n            }\n";

/// The action sending a user's `name` to Acme Pay.
const SEND: &str = "domains/billing/actions/send.jadpo";

fn send(name: &str) -> String {
    format!(
        "use app {{ pay }}\nuse billing/entities {{ User, UserNotFound }}\nuse shared/providers/acme_pay\n\n/// Sends a user to Acme Pay.\naction send(user_id: User.id) fails UserNotFound -> AcmePay.CustomerReply {{\n    var user: User = attempt User.get(user_id)\n    return pay.customers.create(AcmePay.CustomerRequest {{\n        name: {name}\n    }})\n}}\n"
    )
}

/// A project on `main` sending `name`, committed.
fn project(name: &str, send_name: &str, erasure: &str, extra: &str) -> PathBuf {
    built(name, send_name, erasure, extra, false)
}

/// [`project`], whose declarations `refused` says don't check.
fn built(name: &str, send_name: &str, erasure: &str, extra: &str, refused: bool) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("folder");
    package(&root, erasure);
    app(&root, extra);
    write(
        &root,
        "config.jadpo",
        "config {\n    pay_api_key: Text {\n        secret: true\n    }\n    pay_environment: ProviderEnvironment\n}\n",
    );
    write(
        &root,
        "domains/billing/entities/user.jadpo",
        "use shared/providers/acme_pay\n\n/// Someone who pays.\nentity User {\n    id: Uuid\n    display_name: AcmePay.Words {\n        personal: PersonalData.identifying\n    }\n    plan: AcmePay.Words\n    pay_customer_id: AcmePay.CustomerId?\n    person: true\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n    }\n}\n",
    );
    write(
        &root,
        "domains/billing/entities/invoice.jadpo",
        "use shared/providers/acme_pay\n\n/// A bill, with no person's data.\nentity Invoice {\n    id: Uuid\n    pay_customer_id: AcmePay.CustomerId?\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n    }\n}\n",
    );
    write(&root, SEND, &send(send_name));
    write(&root, ".gitignore", "build/\n");
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    if !refused {
        assert_eq!(migrated.code, 0, "{}", migrated.stdout);
        let recorded = jadpo(&root, &["records", "update"]);
        assert_eq!(recorded.code, 0, "{}", recorded.stdout);
    }
    git(&root, &["init", "-q", "-b", "main"]);
    commit(&root, "start");
    root
}

fn commit(root: &Path, message: &str) {
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", message]);
}

fn branch(root: &Path, name: &str) {
    git(root, &["checkout", "-q", "-b", name]);
}

fn report(root: &Path) -> String {
    fs::read_to_string(root.join("build/report/personal-data.md")).expect("report")
}

fn twin(root: &Path) -> serde_json::Value {
    serde_json::from_str(
        &fs::read_to_string(root.join("build/report/personal-data.json")).expect("twin"),
    )
    .expect("JSON")
}

const NEEDS: &str = "pay: ErasureNotPossible.data_stays";

/// A branch starts sending a marked field to a connection that can't erase
/// it: refused until a person accepts it with a reason, which then stands.
#[test]
fn a_connection_that_cannot_erase_needs_an_acceptance_with_a_reason() {
    let root = project("erasure-needs", "user.plan", "", "");
    branch(&root, "send-names");
    write(&root, SEND, &send("user.display_name"));
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 1, "{}", checked.stdout);
    for words in [
        "error: a connection that can't erase the personal data it receives needs a person: pay",
        "--> app.jadpo:6:9",
        "ErasureNotPossible.data_stays",
        "connection `pay` keeps `User.display_name` after a person is erased: `AcmePay.v1` declares no `erasure:`",
        "rule: acceptance.missing",
    ] {
        assert!(
            checked.stdout.contains(words),
            "{words}\n{}",
            checked.stdout
        );
    }

    let drafted = jadpo(&root, &["acceptance", "new", "names_at_pay"]);
    assert_eq!(drafted.code, 0, "{}", drafted.stdout);
    let files = acceptance_files(&root);
    assert_eq!(files.len(), 1);
    let path = root.join(&files[0]);
    let draft = fs::read_to_string(&path).expect("draft");
    assert!(draft.contains(NEEDS), "{draft}");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 1, "{}", checked.stdout);
    assert!(
        checked.stdout.contains("rule: acceptance.no_reason"),
        "{}",
        checked.stdout
    );

    fs::write(
        &path,
        format!("/// Acme Pay must keep customers' names for seven years for tax.\n{draft}"),
    )
    .expect("written");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    assert!(
        report(&root).contains("**Erasure:** kept after a person is erased, since `AcmePay.v1` declares no `erasure:`. Accepted in `acceptances/"),
        "{}",
        report(&root)
    );
    assert!(
        report(&root).contains("seven years for tax"),
        "{}",
        report(&root)
    );
    let json = twin(&root);
    let erasure = &json["connections"][0]["erasure"];
    assert_eq!(erasure["status"], "kept");
    assert_eq!(
        erasure["reason"],
        "Acme Pay must keep customers' names for seven years for tax."
    );

    // Merged, it goes on allowing what it allowed: this is how things are,
    // not a change each later branch makes.
    commit(&root, "send names");
    git(&root, &["checkout", "-q", "main"]);
    git(&root, &["merge", "-q", "--ff-only", "send-names"]);
    branch(&root, "later");
    write(&root, ".later", "something else\n");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);

    // Taking the acceptance away takes away what it allowed.
    fs::remove_file(&path).expect("removed");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 1, "{}", checked.stdout);
    assert!(
        checked.stdout.contains("rule: acceptance.missing"),
        "{}",
        checked.stdout
    );
}

/// It isn't only the change that starts sending: a project already sending
/// needs a person as soon as it can be judged, whatever a branch changes.
#[test]
fn what_a_project_already_sends_needs_a_person_too() {
    let root = project("erasure-already", "user.display_name", "", "");
    branch(&root, "anything");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 1, "{}", checked.stdout);
    assert!(checked.stdout.contains(": pay"), "{}", checked.stdout);
    assert!(
        checked.stdout.contains("rule: acceptance.missing"),
        "{}",
        checked.stdout
    );
}

/// A standard type that is personal data counts, as it does in the report:
/// nothing says whose it is, so a person says why it stays.
#[test]
fn a_standard_type_that_is_personal_data_needs_a_person_too() {
    let root = project("erasure-standard-type", "user.plan", "", "");
    branch(&root, "phones");
    write(
        &root,
        &format!("{HOME}/types/customer_request.jadpo"),
        "/// A customer to create.\npublic type CustomerRequest = Object {\n    /// Their name.\n    name: Words\n    /// Their phone, when they gave one.\n    phone: PhoneNumber?\n}\n",
    );
    edit(
        &root,
        SEND,
        "        name: user.plan\n",
        "        name: user.plan\n        phone: none\n",
    );
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 1, "{}", checked.stdout);
    assert!(
        checked
            .stdout
            .contains("connection `pay` keeps a `PhoneNumber` after a person is erased"),
        "{}",
        checked.stdout
    );
}

/// An entry listing a connection that can erase matches nothing.
#[test]
fn an_acceptance_for_a_connection_that_can_erase_matches_nothing() {
    let root = project("erasure-unmatched", "user.display_name", ERASURE, KEY);
    branch(&root, "needless");
    write(
        &root,
        "acceptances/20260101000000_needless.jadpo",
        "/// Not needed.\nacceptance {\n    changes: {\n        pay: ErasureNotPossible.data_stays\n    }\n}\n",
    );
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 1, "{}", checked.stdout);
    assert!(
        checked.stdout.contains("rule: acceptance.unmatched"),
        "{}",
        checked.stdout
    );
}

/// A package that declares `erasure:` and a connection naming the key need
/// no acceptance, and the report says who erases it.
#[test]
fn a_declared_erasure_with_a_key_needs_no_acceptance() {
    let root = project("erasure-declared", "user.display_name", ERASURE, KEY);
    branch(&root, "anything");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    assert!(
        report(&root).contains("**Erasure:** the provider's `customers.delete` erases its copy, with the key in `User.pay_customer_id`."),
        "{}",
        report(&root)
    );
    let json = twin(&root);
    let erasure = &json["connections"][0]["erasure"];
    assert_eq!(erasure["status"], "declared");
    assert_eq!(erasure["call"], "customers.delete");
    assert_eq!(erasure["key"], "User.pay_customer_id");
}

/// A package's `erasure:` alone doesn't do it: nothing says which person's
/// key to send.
#[test]
fn a_connection_that_names_no_key_needs_an_acceptance() {
    let root = project("erasure-no-key", "user.display_name", ERASURE, "");
    branch(&root, "anything");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 1, "{}", checked.stdout);
    assert!(
        checked.stdout.contains(
            "`AcmePay.v1` declares `erasure:`, and the connection names no `erasure: { key: }`"
        ),
        "{}",
        checked.stdout
    );
}

/// A key on one person entity can't erase another's data.
#[test]
fn a_key_on_one_person_does_not_erase_another() {
    let root = project("erasure-other-person", "user.plan", ERASURE, KEY);
    branch(&root, "staff");
    write(
        &root,
        "domains/billing/entities/employee.jadpo",
        "use shared/providers/acme_pay\n\n/// Someone who works here.\nentity Employee {\n    id: Uuid\n    full_name: AcmePay.Words {\n        personal: PersonalData.identifying\n    }\n    person: true\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n    }\n}\n",
    );
    write(
        &root,
        SEND,
        "use app { pay }\nuse billing/entities { Employee, EmployeeNotFound }\nuse shared/providers/acme_pay\n\n/// Sends an employee to Acme Pay.\naction send(employee_id: Employee.id) fails EmployeeNotFound -> AcmePay.CustomerReply {\n    var employee: Employee = attempt Employee.get(employee_id)\n    return pay.customers.create(AcmePay.CustomerRequest {\n        name: employee.full_name\n    })\n}\n",
    );
    assert_eq!(jadpo(&root, &["migrate", "new", "staff"]).code, 0);
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 1, "{}", checked.stdout);
    assert!(
        checked.stdout.contains("its `erasure:` key `User.pay_customer_id` can't erase `Employee.full_name`, which goes with `Employee`"),
        "{}",
        checked.stdout
    );
}

/// A record erasure reaches through another counts as that person's too,
/// though it belongs to no one directly.
#[test]
fn a_key_on_one_person_does_not_erase_what_goes_with_another() {
    let root = project("erasure-other-person-far", "user.plan", ERASURE, KEY);
    branch(&root, "scans");
    write(
        &root,
        "domains/billing/entities/employee.jadpo",
        "/// Someone who works here.\nentity Employee {\n    id: Uuid\n    person: true\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n    }\n}\n",
    );
    write(
        &root,
        "domains/billing/entities/badge.jadpo",
        "use billing/entities { Employee }\n\n/// An employee's badge.\nentity Badge {\n    id: Uuid\n    employee_id: Employee.id {\n        on_delete: OnDelete.cascade\n    }\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n    }\n}\n",
    );
    write(
        &root,
        "domains/billing/entities/scan.jadpo",
        "use billing/entities { Badge }\nuse shared/providers/acme_pay\n\n/// Where a badge was scanned.\nentity Scan {\n    id: Uuid\n    badge_id: Badge.id {\n        on_delete: OnDelete.cascade\n    }\n    place: AcmePay.Words {\n        personal: PersonalData.location\n    }\n\n    policy: {\n        create: [Access.public]\n        read: [Access.public]\n    }\n}\n",
    );
    write(
        &root,
        SEND,
        "use app { pay }\nuse billing/entities { Scan, ScanNotFound }\nuse shared/providers/acme_pay\n\n/// Sends where a badge was scanned to Acme Pay.\naction send(scan_id: Scan.id) fails ScanNotFound -> AcmePay.CustomerReply {\n    var scan: Scan = attempt Scan.get(scan_id)\n    return pay.customers.create(AcmePay.CustomerRequest {\n        name: scan.place\n    })\n}\n",
    );
    let migrated = jadpo(&root, &["migrate", "new", "scans"]);
    assert_eq!(migrated.code, 0, "{}", migrated.stdout);
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 1, "{}", checked.stdout);
    assert!(
        checked
            .stdout
            .contains("can't erase `Scan.place`, which goes with `Employee`"),
        "{}",
        checked.stdout
    );
}

/// A name sent to one provider and what came back sent to another: the row
/// says it is only what the answer may hold, since nothing says the answer
/// doesn't echo the name, so the second connection still needs a person.
#[test]
fn what_a_provider_answered_says_so_and_still_needs_a_person() {
    let root = project("erasure-answered", "user.display_name", ERASURE, KEY);
    write(
        &root,
        "app.jadpo",
        "use billing/entities { User }\n\napplication {\n    connections: {\n        main: {\n            driver: Driver.postgres\n        }\n        pay: {\n            package: AcmePay.v1\n            domains: [billing]\n            credential: config.pay_api_key\n            environment: config.pay_environment\n            erasure: {\n                key: User.pay_customer_id\n            }\n        }\n        ledger: {\n            package: AcmePay.v1\n            domains: [billing]\n            credential: config.pay_api_key\n            environment: config.pay_environment\n        }\n    }\n}\n",
    );
    write(
        &root,
        "domains/billing/actions/send.jadpo",
        "use app { ledger, pay }\nuse billing/entities { User, UserNotFound }\nuse shared/providers/acme_pay\n\n/// Sends a user to Acme Pay, and the id it made to the ledger.\naction send(user_id: User.id) fails UserNotFound -> AcmePay.CustomerReply {\n    var user: User = attempt User.get(user_id)\n    var made: AcmePay.CustomerReply = pay.customers.create(AcmePay.CustomerRequest {\n        name: user.display_name\n    })\n    return ledger.customers.read(AcmePay.CustomerKey {\n        id: made.id\n    })\n}\n",
    );
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    commit(&root, "two connections");
    branch(&root, "anything");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 1, "{}", checked.stdout);
    assert!(checked.stdout.contains(": ledger"), "{}", checked.stdout);

    // Accepted, the report tells the two rows apart.
    assert_eq!(jadpo(&root, &["acceptance", "new", "ledger_ids"]).code, 0);
    let files = acceptance_files(&root);
    let path = root.join(&files[0]);
    let draft = fs::read_to_string(&path).expect("draft");
    fs::write(
        &path,
        format!("/// The ledger gets only the id Acme Pay made.\n{draft}"),
    )
    .expect("written");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    let written = report(&root);
    for row in [
        "| `User.display_name` | `identifying` | operation `customers.create` | sent |",
        "| `User.display_name` | `identifying` | operation `customers.read` | may be in a provider's answer that was sent |",
    ] {
        assert!(written.contains(row), "{row}\n{written}");
    }
    let json = twin(&root);
    let ledger = &json["connections"][0];
    assert_eq!(ledger["connection"], "ledger");
    assert_eq!(ledger["sent"][0]["answer_only"], true);
}

/// A store on the server's own disk is no other company.
#[test]
fn the_apps_own_store_of_files_needs_no_acceptance() {
    let root = project("erasure-own-store", "user.plan", "", "");
    branch(&root, "papers");
    write(
        &root,
        "app.jadpo",
        "application {\n    connections: {\n        main: {\n            driver: Driver.postgres\n        }\n        papers: {\n            driver: Driver.disk\n            path: \"/var/lib/papers\"\n        }\n        pay: {\n            package: AcmePay.v1\n            domains: [billing]\n            credential: config.pay_api_key\n            environment: config.pay_environment\n        }\n    }\n}\n",
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
    assert_eq!(jadpo(&root, &["records", "update"]).code, 0);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}", checked.stdout);
    assert!(
        report(&root).contains(
            "**Erasure:** the app's own store: a file goes with the records that hold it."
        ),
        "{}",
        report(&root)
    );
}

/// The approval report shows the personal-data report's difference, and the
/// decision with the path that makes it true.
#[test]
fn the_approval_report_shows_the_reports_difference() {
    let root = project("erasure-approval", "user.plan", "", "");
    branch(&root, "send-names");
    write(&root, SEND, &send("user.display_name"));
    let output = jadpo(&root, &["approval", "--json"]);
    assert_eq!(output.code, 0, "{}{}", output.stdout, output.stderr);
    let document: serde_json::Value = serde_json::from_str(&output.stdout).expect("JSON");
    let data = &document["data"];
    assert_eq!(
        data["decisions"][0]["said"],
        "connection `pay` keeps `User.display_name` after a person is erased: `AcmePay.v1` declares no `erasure:`"
    );
    assert_eq!(
        data["decisions"][0]["change"],
        "ErasureNotPossible.data_stays"
    );
    assert_eq!(data["decisions"][0]["acceptance"], "missing");
    let markdown = fs::read_to_string(root.join("build/review/approval.md")).expect("report");
    for words in [
        "### Personal data",
        "`build/report/personal-data.md`: 1 entry added.",
        "+ connection pay",
        "+ `User.display_name` (identifying): operation `customers.create`, sent",
        "+ kept: `AcmePay.v1` declares no `erasure:`",
    ] {
        assert!(markdown.contains(words), "{words}\n{markdown}");
    }
    let terminal = jadpo(&root, &["approval"]);
    assert!(
        terminal.stdout.contains("personal: 1 connection added"),
        "{}",
        terminal.stdout
    );
    assert!(
        root.join("build/before")
            .read_dir()
            .expect("before")
            .flatten()
            .any(|entry| entry.path().join("personal-data.json").exists())
    );
}

/// Nothing sends anything new: the review says no change to the report.
#[test]
fn the_approval_report_says_when_the_report_is_unchanged() {
    let root = project("erasure-approval-same", "user.display_name", ERASURE, KEY);
    branch(&root, "elsewhere");
    edit(
        &root,
        "domains/billing/entities/invoice.jadpo",
        "/// A bill, with no person's data.",
        "/// A bill, holding no person's data.",
    );
    let output = jadpo(&root, &["approval"]);
    assert_eq!(output.code, 0, "{}{}", output.stdout, output.stderr);
    let markdown = fs::read_to_string(root.join("build/review/approval.md")).expect("report");
    assert!(
        markdown.contains("### Personal data\n\nNo change."),
        "{markdown}"
    );
}

/// Only declarations that can be carried out let a connection off.
#[test]
fn a_package_erasure_that_cannot_be_called_is_refused() {
    for (name, erasure, words) in [
        (
            "unknown-call",
            "    erasure: {\n        call: customers.destroy\n        by: CustomerId\n    }\n",
            "`erasure:` names `customers.destroy`, which `AcmePay` doesn't have",
        ),
        (
            "no-change",
            "    erasure: {\n        call: customers.read\n        by: CustomerId\n    }\n",
            "`erasure:` names `customers.read`, which changes nothing",
        ),
        (
            "unknown-by",
            "    erasure: {\n        call: customers.delete\n        by: NoSuchThing\n    }\n",
            "`erasure:`'s `by:` names `NoSuchThing`, which isn't a type",
        ),
        (
            "wrong-by",
            "    erasure: {\n        call: customers.delete\n        by: Words\n    }\n",
            "`erasure:` can't call `customers.delete` with only a `Words`",
        ),
        (
            "not-both",
            "    erasure: {\n        call: customers.delete\n    }\n",
            "`erasure:` needs `call:` and `by:`",
        ),
        (
            "not-a-block",
            "    erasure: customers.delete\n",
            "`erasure:` isn't a block",
        ),
    ] {
        let root = built(
            &format!("erasure-package-{name}"),
            "user.plan",
            erasure,
            "",
            true,
        );
        let checked = jadpo(&root, &["check"]);
        assert_eq!(checked.code, 1, "{name}: {}", checked.stdout);
        assert!(checked.stdout.contains(words), "{name}: {}", checked.stdout);
        assert!(
            checked.stdout.contains("rule: package.provider_invalid"),
            "{name}: {}",
            checked.stdout
        );
    }
}

#[test]
fn a_connections_key_that_cannot_be_used_is_refused() {
    for (name, extra, words) in [
        (
            "no-field",
            "            erasure: {\n                key: User.no_such_field\n            }\n",
            "`pay`'s `erasure:` key `User.no_such_field` isn't a field",
        ),
        (
            "not-a-person",
            "            erasure: {\n                key: Invoice.pay_customer_id\n            }\n",
            "`pay`'s `erasure:` key `Invoice.pay_customer_id` isn't on a person",
        ),
        (
            "wrong-type",
            "            erasure: {\n                key: User.plan\n            }\n",
            "`pay`'s `erasure:` key `User.plan` isn't a `CustomerId`",
        ),
        (
            "no-key",
            "            erasure: {\n            }\n",
            "`pay`'s `erasure:` needs `key:`",
        ),
    ] {
        let root = built(
            &format!("erasure-key-{name}"),
            "user.plan",
            ERASURE,
            extra,
            true,
        );
        let checked = jadpo(&root, &["check"]);
        assert_eq!(checked.code, 1, "{name}: {}", checked.stdout);
        assert!(checked.stdout.contains(words), "{name}: {}", checked.stdout);
        assert!(
            checked.stdout.contains("rule: connection.value_invalid"),
            "{name}: {}",
            checked.stdout
        );
    }
    let root = built("erasure-key-not-taken", "user.plan", "", KEY, true);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 1, "{}", checked.stdout);
    assert!(
        checked
            .stdout
            .contains("`pay` gives `erasure:`, which `AcmePay.v1` doesn't take"),
        "{}",
        checked.stdout
    );
}
