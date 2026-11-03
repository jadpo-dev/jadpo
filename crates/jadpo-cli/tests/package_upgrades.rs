//! Risky package upgrades (area 18, "Package upgrades"; chunk V4), end to
//! end through git: the review compares what the app used of a package on
//! the before side with the package now, version ignored. A used operation
//! removed, a used field removed or retyped, a new failure on a used
//! operation, and a host or credential scheme changed each need a
//! `PackageChange` acceptance; what the app never uses, and what only adds,
//! need none. `jadpo package update` prints the review, and the approval
//! report says each in plain words.

use std::fs;
use std::path::{Path, PathBuf};

use super::acceptances::git;
use super::http::{jadpo, tempdir, write};

const PACKAGE: &[(&str, &str)] = &[
    (
        "package.jadpo",
        "/// Acme Pay's payments API.\npackage AcmePay.v1 {\n    licence: Licence.mit\n    jadpo: \"1.0\"\n}\n",
    ),
    (
        "provider.jadpo",
        "/// Acme Pay's payments API.\nprovider {\n    protocol: Protocol.json_http\n    hosts: {\n        live: \"https://api.acme-pay.example\"\n        sandbox: \"https://sandbox.acme-pay.example\"\n    }\n    credential: CredentialScheme.bearer\n}\n",
    ),
    (
        "types/words.jadpo",
        "/// Words Acme Pay keeps.\npublic type Words = Text {}\n",
    ),
    (
        "types/customer_request.jadpo",
        "/// A customer to create.\npublic type CustomerRequest = Object {\n    /// Their name.\n    name: Words\n}\n",
    ),
    (
        "types/customer_reply.jadpo",
        "/// The customer Acme Pay made.\npublic type CustomerReply = Object {\n    /// Acme Pay's id for them.\n    id: Words\n    /// Their plan.\n    plan: Words\n    /// Their balance.\n    balance: Int\n}\n",
    ),
    (
        "failures/card_refused.jadpo",
        "/// Acme Pay refused the card.\npublic type CardRefused = Failure {\n    kind: FailureKind.invalid_value\n    message: \"Acme Pay refused the card.\"\n}\n",
    ),
    (
        "resources/customers.jadpo",
        "/// Customers.\npublic resource customers {\n    /// Creates a customer.\n    create: {\n        method: HttpMethod.post\n        path: \"/v1/customers\"\n        input: CustomerRequest\n        output: CustomerReply\n        changes: true\n        source: PackageSource.written\n    }\n    /// Looks a customer up.\n    find: {\n        method: HttpMethod.get\n        path: \"/v1/customers/find\"\n        input: CustomerRequest\n        output: CustomerReply\n        source: PackageSource.written\n    }\n}\n",
    ),
    (
        "resources/refunds.jadpo",
        "/// Refunds, which the app never asks for.\npublic resource refunds {\n    /// Starts a refund.\n    start: {\n        method: HttpMethod.post\n        path: \"/v1/refunds\"\n        input: CustomerRequest\n        output: CustomerRequest\n        changes: true\n        source: PackageSource.written\n    }\n}\n",
    ),
];

const APP: &str = "application {\n    connections: {\n        pay: {\n            package: AcmePay.v1\n            domains: [billing]\n            credential: config.pay_api_key\n            environment: config.pay_environment\n        }\n    }\n}\n";

const CONFIG: &str = "config {\n    pay_api_key: Text {\n        secret: true\n    }\n    pay_environment: ProviderEnvironment\n}\n";

const SEND: &str = "use app { pay }\nuse shared/providers/acme_pay\nuse packages/acme_pay\n\n/// Sends a customer to Acme Pay.\naction send(name: AcmePay.Words) -> AcmePay.CustomerReply {\n    return pay.customers.create(AcmePay.CustomerRequest {\n        name: name\n    })\n}\n";

/// A package's folder and an app with it installed from there, committed
/// on `main` and on a new branch.
fn installed(name: &str) -> (tempdir::Dir, PathBuf, PathBuf) {
    let dir = tempdir::Dir::new(name);
    let package = dir.path().join("acme_pay");
    for (path, text) in PACKAGE {
        write(&package, path, text);
    }
    jadpo(&package, &["package", "check"]);
    let app = dir.path().join("shop");
    write(&app, "app.jadpo", APP);
    write(&app, "config.jadpo", CONFIG);
    write(
        &app,
        "domains/billing/actions/send.jadpo",
        &SEND.replace("use shared/providers/acme_pay\n", ""),
    );
    let added = jadpo(&app, &["package", "add", "../acme_pay"]);
    assert_eq!(added.code, 0, "{}{}", added.stdout, added.stderr);
    let updated = jadpo(&app, &["records", "update"]);
    assert_eq!(updated.code, 0, "{}{}", updated.stdout, updated.stderr);
    let checked = jadpo(&app, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    git(&app, &["init", "-q", "-b", "main"]);
    git(&app, &["add", "-A"]);
    git(&app, &["commit", "-q", "-m", "The shop"]);
    git(&app, &["checkout", "-q", "-b", "upgrade"]);
    (dir, package, app)
}

/// Replaces `from` in the package's file, and writes its surface again.
fn change(package: &Path, path: &str, from: &str, to: &str) {
    let text = fs::read_to_string(package.join(path)).expect("read");
    assert_eq!(text.matches(from).count(), 1, "{from:?} in {path}");
    fs::write(package.join(path), text.replacen(from, to, 1)).expect("written");
    jadpo(package, &["package", "check"]);
}

/// Each change `jadpo check` says needs a person, as an acceptance lists
/// it, from the draft `jadpo acceptance new` writes.
fn drafted(app: &Path) -> Vec<String> {
    let done = jadpo(app, &["acceptance", "new", "upgrade_acme_pay"]);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    let mut out = Vec::new();
    for entry in fs::read_dir(app.join("acceptances")).expect("the folder") {
        let text = fs::read_to_string(entry.expect("entry").path()).expect("read");
        out.extend(
            text.lines()
                .filter(|line| line.contains("Change."))
                .map(|line| line.trim().to_string()),
        );
    }
    out.sort();
    out
}

/// The update's screen, each line's indent and column padding squeezed.
fn update(app: &Path) -> String {
    let done = jadpo(app, &["package", "update", "acme_pay"]);
    assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    done.stdout
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" ") + "\n")
        .collect()
}

/// The app's fix for a new failure: mapping it, as the compile error asks.
fn handle_refusal(app: &Path) {
    write(
        app,
        "domains/billing/failures/payment_refused.jadpo",
        "/// The card was refused.\ntype PaymentRefused = Failure {\n    kind: FailureKind.invalid_value\n}\n",
    );
    write(
        app,
        "domains/billing/actions/send.jadpo",
        "use app { pay }\nuse billing/failures { PaymentRefused }\nuse packages/acme_pay\n\n/// Sends a customer to Acme Pay.\naction send(name: AcmePay.Words) fails PaymentRefused -> AcmePay.CustomerReply {\n    return attempt pay.customers.create(AcmePay.CustomerRequest {\n        name: name\n    }) {\n        card_refused: PaymentRefused\n    }\n}\n",
    );
}

#[test]
fn a_used_operation_removed_or_a_new_failure_on_one_needs_a_person() {
    let (_dir, package, app) = installed("upgrade-operations");
    change(
        &package,
        "resources/customers.jadpo",
        "        changes: true\n",
        "        changes: true\n        failures: [CardRefused]\n",
    );
    let screen = update(&app);
    assert!(
        screen.contains("Needs a person\n+ customers.create new failure card_refused\n"),
        "{screen}"
    );
    assert!(screen.contains("1 needs a person"), "{screen}");
    handle_refusal(&app);
    assert_eq!(
        drafted(&app),
        ["AcmePay.customers.create.card_refused: PackageChange.failure_added"]
    );
}

#[test]
fn a_used_operation_gone_stays_a_decision_once_the_app_stops_calling_it() {
    let (_dir, package, app) = installed("upgrade-removed");
    let text = fs::read_to_string(package.join("resources/customers.jadpo")).expect("read");
    let start = text.find("    /// Creates a customer.").expect("create");
    let end = text.find("    /// Looks a customer up.").expect("find");
    fs::write(
        package.join("resources/customers.jadpo"),
        format!("{}{}", &text[..start], &text[end..]),
    )
    .expect("written");
    jadpo(&package, &["package", "check"]);
    let screen = update(&app);
    assert!(screen.contains("- customers.create removed\n"), "{screen}");
    // The compile error is fixed by moving to `find`, and the removal
    // still needs a person: it was used on the before side.
    write(
        &app,
        "domains/billing/actions/send.jadpo",
        &SEND
            .replace("use shared/providers/acme_pay\n", "")
            .replace("pay.customers.create", "pay.customers.find"),
    );
    assert_eq!(
        drafted(&app),
        ["AcmePay.customers.create: PackageChange.operation_removed"]
    );
}

#[test]
fn a_used_field_removed_or_retyped_needs_a_person() {
    let (_dir, package, app) = installed("upgrade-fields");
    change(
        &package,
        "types/customer_reply.jadpo",
        "    /// Their plan.\n    plan: Words\n    /// Their balance.\n    balance: Int\n",
        "    /// Their balance.\n    balance: Words\n",
    );
    let screen = update(&app);
    assert!(
        screen.contains("- CustomerReply field removed plan\n"),
        "{screen}"
    );
    assert!(
        screen.contains("~ CustomerReply field retyped balance Int → Words\n"),
        "{screen}"
    );
    assert_eq!(
        drafted(&app),
        [
            "AcmePay.CustomerReply.balance: PackageChange.field_retyped",
            "AcmePay.CustomerReply.plan: PackageChange.field_removed",
        ]
    );
}

#[test]
fn a_host_or_credential_scheme_changed_needs_a_person() {
    let (_dir, package, app) = installed("upgrade-hosts");
    change(
        &package,
        "provider.jadpo",
        "credential: CredentialScheme.bearer",
        "credential: CredentialScheme.header\n    credential_header: \"x-api-key\"",
    );
    change(
        &package,
        "provider.jadpo",
        "live: \"https://api.acme-pay.example\"",
        "live: \"https://eu.acme-pay.example\"",
    );
    let screen = update(&app);
    assert!(
        screen.contains("~ credential changed bearer → header x-api-key\n"),
        "{screen}"
    );
    assert!(
        screen.contains(
            "~ host changed https://api.acme-pay.example → https://eu.acme-pay.example\n"
        ),
        "{screen}"
    );
    jadpo(&app, &["records", "update"]);
    assert_eq!(
        drafted(&app),
        [
            "AcmePay.credential: PackageChange.credential_changed",
            "AcmePay.host: PackageChange.host_changed",
            "pay: NetworkChange.host_added",
        ]
    );
}

#[test]
fn what_only_adds_and_what_the_app_never_uses_need_nobody() {
    let (_dir, package, app) = installed("upgrade-quiet");
    // A new optional field, a new operation, a description, and a change
    // to an operation the app never calls.
    change(
        &package,
        "types/customer_reply.jadpo",
        "    /// Their plan.\n",
        "    /// Their nickname.\n    nickname: Words?\n    /// The plan they are on.\n",
    );
    change(
        &package,
        "resources/refunds.jadpo",
        "        changes: true\n",
        "        changes: true\n        failures: [CardRefused]\n",
    );
    change(
        &package,
        "resources/refunds.jadpo",
        "    /// Starts a refund.\n",
        "    /// Cancels a refund.\n    cancel: {\n        method: HttpMethod.post\n        path: \"/v1/refunds/cancel\"\n        input: CustomerRequest\n        output: CustomerRequest\n        changes: true\n        source: PackageSource.written\n    }\n    /// Starts a refund.\n",
    );
    let screen = update(&app);
    assert!(!screen.contains("Needs a person"), "{screen}");
    assert!(!screen.contains("need a person"), "{screen}");
    assert!(
        screen.contains("~ type CustomerReply used by the app\n"),
        "{screen}"
    );
    assert!(screen.contains("~ operation refunds.start\n"), "{screen}");
    let checked = jadpo(&app, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
}

#[test]
fn check_passes_once_a_person_gives_the_reason_and_the_report_says_it_plainly() {
    let (_dir, package, app) = installed("upgrade-accepted");
    change(
        &package,
        "resources/customers.jadpo",
        "        changes: true\n",
        "        changes: true\n        failures: [CardRefused]\n",
    );
    update(&app);
    handle_refusal(&app);
    jadpo(&app, &["records", "update"]);
    let checked = jadpo(&app, &["check"]);
    assert_eq!(checked.code, 1, "{}", checked.stdout);
    assert!(
        checked
            .stdout
            .contains("a new failure on an operation the app uses needs a person"),
        "{}",
        checked.stdout
    );
    assert!(
        checked
            .stdout
            .contains("AcmePay.customers.create.card_refused"),
        "{}",
        checked.stdout
    );
    assert_eq!(
        drafted(&app),
        ["AcmePay.customers.create.card_refused: PackageChange.failure_added"]
    );
    let approval = jadpo(&app, &["approval"]);
    assert_eq!(approval.code, 0, "{}{}", approval.stdout, approval.stderr);
    let report: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(app.join("build/review/approval.json")).expect("the report"),
    )
    .expect("JSON");
    let decisions = report["decisions"].as_array().expect("decisions");
    assert!(
        decisions.iter().any(|decision| decision["said"]
            == "`AcmePay.customers.create` can now fail with `card_refused`"),
        "{decisions:?}"
    );
    let folder = app.join("acceptances");
    let file = fs::read_dir(&folder)
        .expect("the folder")
        .next()
        .expect("a draft")
        .expect("entry")
        .path();
    let text = fs::read_to_string(&file).expect("read");
    fs::write(
        &file,
        format!("/// A refused card is shown as any other failed payment.\n{text}"),
    )
    .expect("written");
    let checked = jadpo(&app, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
}
