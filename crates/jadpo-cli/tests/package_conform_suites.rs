//! `jadpo package conform <package> email` and `receiving` (area 11,
//! "Proving a package works"; area 14, "`jadpo package conform`"; chunk
//! K4): the suites of the capabilities a package fills besides `ai`, run
//! against the Resend and Amazon SQS packages as an app installs them, and
//! against an app's own mail package with fixed `hosts:`, each against its
//! recordings and the local fake Jadpo starts on this machine's loopback
//! address. The live run goes only to a fake provider on this machine.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

use crate::support::fake_provider::FakeProvider;

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("package-conform-k4-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temporary folder");
    dir
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    run(&args, &Environment::empty().with_cwd(cwd))
}

fn said(out: &Output) -> String {
    format!("{}{}", out.stdout, out.stderr)
}

fn package(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/packages")
        .join(name)
}

/// Each case's state in `out`'s JSON, by its key.
fn cases(out: &Output) -> Vec<(String, String)> {
    let json: serde_json::Value = serde_json::from_str(&out.stdout).expect("JSON");
    json["data"]["cases"]
        .as_array()
        .unwrap_or_else(|| panic!("no cases: {}", out.stdout))
        .iter()
        .map(|case| {
            (
                case["case"].as_str().unwrap_or_default().to_string(),
                case["state"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect()
}

fn states(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(case, state)| (case.to_string(), state.to_string()))
        .collect()
}

fn checked(root: &Path) {
    jadpo(root, &["records", "update"]);
    let checked = jadpo(root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
}

/// An app sending its email through Resend, installed as an app installs
/// it; `email:` goes through it when `email` is true.
fn mail_app(test: &str, email: bool) -> PathBuf {
    let root = folder(test);
    let sends = if email {
        "    email: {\n        connection: mail\n        from: config.mail_sender\n    }\n"
    } else {
        ""
    };
    write(
        &root,
        "app.jadpo",
        &format!(
            "application {{\n    connections: {{\n        mail: {{\n            package: Resend.v1\n            credential: config.resend_key\n            host: config.resend_host\n{}        }}\n    }}\n{sends}}}\n",
            if email {
                ""
            } else {
                "            domains: [support]\n"
            }
        ),
    );
    write(
        &root,
        "config.jadpo",
        "config {\n    mail_sender: Email {\n        default: \"desk@acme.test\"\n    }\n    resend_host: Url {\n        default: \"https://provider.invalid\"\n    }\n    resend_key: Text {\n        secret: true\n    }\n}\n",
    );
    write(
        &root,
        "domains/support/types/asked.jadpo",
        "/// What a customer asked.\ntype Asked = Input {\n    question: Text {\n        max_length: 4000\n    }\n}\n",
    );
    write(
        &root,
        "domains/support/actions/echo.jadpo",
        "use support/types { Asked }\n\n/// Takes a question.\naction echo(asked: Asked) {\n}\n",
    );
    write(
        &root,
        "policy.jadpo",
        "use support\n\npolicy {\n    actions: {\n        support.echo: [Access.public]\n    }\n}\n",
    );
    let resend = package("resend");
    let added = jadpo(&root, &["package", "add", resend.to_str().expect("a path")]);
    assert_eq!(added.code, 0, "{}{}", added.stdout, added.stderr);
    checked(&root);
    root
}

#[test]
fn resend_conforms_to_email_against_its_recordings_and_a_local_fake() {
    let root = mail_app("resend", true);
    let out = jadpo(&root, &["package", "conform", "resend", "email"]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout,
        "Conformance suite\n  \
         message       passed                        sent, its receipt read\n  \
         text only     passed                        sent with no HTML\n  \
         attachment    passed                        1 file, its bytes sent\n  \
         rejection     passed                        5 failures, each read as mapped\n\
         \n\
         email conforms: Resend.v1, 4 passed, its recordings and a local fake\n\
         \n\
         next: jadpo package conform resend email --live  sends one email to the app's own sender through the provider's API\n",
    );
    let json = jadpo(&root, &["package", "conform", "resend", "email", "--json"]);
    assert_eq!(json.code, 0, "{}", json.stdout);
    assert_eq!(
        cases(&json),
        states(&[
            ("message", "passed"),
            ("text_only", "passed"),
            ("attachment", "passed"),
            ("rejection", "passed"),
        ])
    );
    let document: serde_json::Value = serde_json::from_str(&json.stdout).expect("JSON");
    assert_eq!(document["data"]["capability"], "email");
    assert_eq!(document["data"]["against"], "recordings");
}

/// Without `--live`, nothing goes to the connection's address, even with
/// its key set.
#[test]
fn without_the_live_flag_no_email_goes_to_the_connections_address() {
    let api = FakeProvider::start();
    let root = mail_app("email-no-flag", true);
    write(
        &root,
        ".env.local",
        &format!("RESEND_KEY=test_key_k4\nRESEND_HOST={}\n", api.url),
    );
    let out = jadpo(&root, &["package", "conform", "resend", "email"]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    assert!(api.requests().is_empty(), "{:?}", api.requests());
}

/// The live run sends through the app's own connection, to its own
/// sender: here a provider on this machine. The rejection is left to the
/// run against the recordings.
#[test]
fn the_live_email_run_sends_to_the_apps_own_sender_through_its_connection() {
    let api = FakeProvider::start();
    for _ in 0..3 {
        api.answer(200, "{\"id\":\"live-receipt\"}");
    }
    let root = mail_app("email-live", true);
    write(
        &root,
        ".env.local",
        &format!("RESEND_KEY=test_key_k4\nRESEND_HOST={}\n", api.url),
    );
    let out = jadpo(
        &root,
        &["package", "conform", "resend", "email", "--live", "--json"],
    );
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    assert_eq!(
        cases(&out),
        states(&[
            ("message", "passed"),
            ("text_only", "passed"),
            ("attachment", "passed"),
            ("rejection", "skipped"),
        ])
    );
    let requests = api.requests();
    assert_eq!(requests.len(), 3, "{requests:?}");
    for request in &requests {
        assert_eq!(request.json()["to"], "desk@acme.test");
        assert_eq!(request.header("authorization"), Some("Bearer test_key_k4"));
    }
}

#[test]
fn the_live_email_run_is_refused_unless_email_goes_through_the_connection() {
    let root = mail_app("email-live-elsewhere", false);
    write(
        &root,
        ".env.local",
        "RESEND_KEY=test_key_k4\nRESEND_HOST=https://provider.invalid\n",
    );
    let out = jadpo(&root, &["package", "conform", "resend", "email", "--live"]);
    assert_eq!(out.code, 1, "{}{}", out.stdout, out.stderr);
    assert!(said(&out).contains("conform.refused"), "{}", said(&out));
    assert!(
        said(&out).contains("`email:` doesn't send through `mail`"),
        "{}",
        said(&out)
    );
    let unset = mail_app("email-live-unset", true);
    let out = jadpo(&unset, &["package", "conform", "resend", "email", "--live"]);
    assert_eq!(out.code, 1, "{}{}", out.stdout, out.stderr);
    assert!(
        said(&out).contains("`mail` has nothing set for the provider's API"),
        "{}",
        said(&out)
    );
}

/// An app receiving a queue through Amazon SQS.
fn queue_app(test: &str) -> PathBuf {
    let root = folder(test);
    write(
        &root,
        "app.jadpo",
        "use depot/events { ParcelScanned }\n\napplication {\n    connections: {\n        main: {\n            driver: Driver.postgres\n            url: config.database_url\n        }\n        scans: {\n            package: AmazonSqs.v2012_11_05\n            domains: [depot]\n            credential: {\n                key_id: config.aws_key_id\n                secret: config.aws_secret_key\n            }\n            host: config.sqs_endpoint\n            queue_url: config.queue_url\n        }\n    }\n    events: {\n        store: main\n    }\n    receiving: {\n        scans: {\n            id_at: \"scan_id\"\n            events: [ParcelScanned]\n        }\n    }\n}\n",
    );
    write(
        &root,
        "config.jadpo",
        "config {\n    aws_key_id: Text\n    aws_secret_key: Text {\n        secret: true\n    }\n    database_url: PostgresUrl\n    queue_url: Text\n    sqs_endpoint: Url {\n        default: \"https://sqs.eu-west-2.amazonaws.com\"\n    }\n}\n",
    );
    write(
        &root,
        "domains/depot/events/parcel_scanned.jadpo",
        "/// A parcel went past a scanner.\ntype ParcelScanned = Event {\n    bay: Int {\n        min: 1\n        max: 40\n    }\n}\n",
    );
    write(
        &root,
        "domains/depot/entities/parcel.jadpo",
        "/// A parcel at the depot.\nentity Parcel {\n    id: Uuid\n    bay: Int {\n        min: 1\n        max: 40\n    }\n}\n",
    );
    let sqs = package("amazon_sqs");
    let added = jadpo(&root, &["package", "add", sqs.to_str().expect("a path")]);
    assert_eq!(added.code, 0, "{}{}", added.stdout, added.stderr);
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    checked(&root);
    root
}

#[test]
fn amazon_sqs_conforms_to_receiving_against_its_recordings_and_a_local_fake() {
    let root = queue_app("sqs");
    let out = jadpo(&root, &["package", "conform", "amazon_sqs", "receiving"]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout,
        "Conformance suite\n  \
         messages      passed                        2 read, each body and handle\n  \
         empty queue   passed                        read as no messages\n  \
         acknowledge   passed                        by its handle\n\
         \n\
         receiving conforms: AmazonSqs.v2012_11_05, 3 passed, its recordings and a local fake\n",
    );
}

#[test]
fn receiving_has_no_live_run() {
    let root = queue_app("sqs-live");
    let out = jadpo(
        &root,
        &["package", "conform", "amazon_sqs", "receiving", "--live"],
    );
    assert_eq!(out.code, 1, "{}{}", out.stdout, out.stderr);
    assert!(said(&out).contains("conform.refused"), "{}", said(&out));
    assert!(
        said(&out).contains("`receiving` has no live run"),
        "{}",
        said(&out)
    );
}

/// The app's own mail package, with fixed `hosts:`, written by hand.
fn own_mail_app(test: &str, recorded: bool) -> PathBuf {
    let root = folder(test);
    let home = "shared/providers/acme_mail/v1";
    write(
        &root,
        "app.jadpo",
        "application {\n    connections: {\n        mail: {\n            package: AcmeMail.v1\n            credential: config.acme_key\n            environment: config.acme_environment\n            stream: \"outbound\"\n        }\n    }\n    email: {\n        connection: mail\n        from: config.mail_sender\n    }\n}\n",
    );
    write(
        &root,
        "config.jadpo",
        "config {\n    acme_environment: ProviderEnvironment {\n        default: ProviderEnvironment.sandbox\n    }\n    acme_key: Text {\n        secret: true\n    }\n    mail_sender: Email {\n        default: \"desk@acme.test\"\n    }\n}\n",
    );
    write(
        &root,
        &format!("{home}/package.jadpo"),
        "/// Acme's mail API.\npackage AcmeMail.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n",
    );
    write(
        &root,
        &format!("{home}/provider.jadpo"),
        "/// Acme's mail API at its fixed hosts.\nprovider {\n    protocol: Protocol.json_http\n    hosts: {\n        live: \"https://api.acme-mail.example\"\n        sandbox: \"https://sandbox.acme-mail.example\"\n    }\n    credential: CredentialScheme.bearer\n    error_code: \"error\"\n    settings: {\n        stream: Text\n    }\n}\n",
    );
    write(
        &root,
        &format!("{home}/resources/messages.jadpo"),
        "public resource messages {\n    send: {\n        method: HttpMethod.post\n        path: \"/v1/messages\"\n        input: OutgoingMessage\n        output: SentMessage\n        changes: true\n        failures: [BadRecipient]\n        source: PackageSource.written\n        docs: \"https://docs.acme-mail.example/send\"\n    }\n}\n",
    );
    write(
        &root,
        &format!("{home}/types/outgoing_message.jadpo"),
        "/// One email to send.\npublic type OutgoingMessage = Object {\n    sender: Text {\n        personal: PersonalData.contact\n    }\n    recipient: Text {\n        personal: PersonalData.contact\n    }\n    subject: Text {\n        personal: PersonalData.identifying\n    }\n    html: Text? {\n        personal: PersonalData.identifying\n    }\n    text: Text? {\n        personal: PersonalData.identifying\n    }\n    stream: Text\n    files: List<OutgoingFile>\n}\n",
    );
    write(
        &root,
        &format!("{home}/types/outgoing_file.jadpo"),
        "/// One file sent with an email.\npublic type OutgoingFile = Object {\n    name: Text\n    data: Bytes\n}\n",
    );
    write(
        &root,
        &format!("{home}/types/sent_message.jadpo"),
        "/// The message Acme took.\npublic type SentMessage = Object {\n    message_id: Text\n}\n",
    );
    write(
        &root,
        &format!("{home}/failures/bad_recipient.jadpo"),
        "/// Acme refused the recipient.\npublic type BadRecipient = Failure {\n    kind: FailureKind.invalid_value\n    message: \"Acme refused the recipient.\"\n}\n",
    );
    write(
        &root,
        &format!("{home}/provides/email.jadpo"),
        "/// Sends each email through `messages.send`.\nprovide email {\n    send: {\n        call: messages.send\n        input: {\n            sender: message.from\n            recipient: message.to\n            subject: message.subject\n            html: message.html\n            text: message.text\n            stream: settings.stream\n            files: {\n                each: message.attachments\n                as: {\n                    name: item.name\n                    data: item.content\n                }\n            }\n        }\n        receipt: response.message_id\n        failures: {\n            bad_recipient: EmailFailure.recipient_rejected\n        }\n    }\n    limits: {\n        attachments: 10mb\n    }\n}\n",
    );
    if recorded {
        write(
            &root,
            &format!("{home}/recordings/messages.send.json"),
            "{\n  \"message_id\": \"{Text}\"\n}\n",
        );
    }
    checked(&root);
    root
}

/// A package with fixed `hosts:` runs against the local fake too: every
/// request goes to it, whatever host the package names.
#[test]
fn a_package_with_fixed_hosts_conforms_against_the_local_fake() {
    let root = own_mail_app("fixed-hosts", true);
    let out = jadpo(
        &root,
        &["package", "conform", "acme_mail", "email", "--json"],
    );
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    assert_eq!(
        cases(&out),
        states(&[
            ("message", "passed"),
            ("text_only", "passed"),
            ("attachment", "passed"),
            ("rejection", "passed"),
        ])
    );
}

/// A package without the recording the local fake answers from fails
/// each case that needs it, naming `jadpo package record`.
#[test]
fn a_missing_recording_fails_its_cases() {
    let root = own_mail_app("unrecorded", false);
    let out = jadpo(&root, &["package", "conform", "acme_mail", "email"]);
    assert_eq!(out.code, 1, "{}{}", out.stdout, out.stderr);
    assert!(said(&out).contains("conform.failed"), "{}", said(&out));
    assert!(
        said(&out).contains("`AcmeMail.v1`'s message doesn't conform to `email`"),
        "{}",
        said(&out)
    );
    assert!(
        said(&out).contains("jadpo package record"),
        "{}",
        said(&out)
    );
    assert!(
        out.stdout
            .contains("email doesn't conform: 3 failed, 1 passed"),
        "{}",
        out.stdout
    );
}

/// A capability whose mapping isn't read has no suite, and a package
/// that doesn't fill a capability is named.
#[test]
fn a_capability_the_package_doesnt_fill_is_named() {
    let root = mail_app("doesnt-fill", true);
    let out = jadpo(&root, &["package", "conform", "resend", "receiving"]);
    assert_eq!(out.code, 2, "{}", said(&out));
    assert!(
        said(&out).contains("`Resend.v1` doesn't fill `receiving`"),
        "{}",
        said(&out)
    );
    assert!(
        said(&out).contains("Run `jadpo package conform resend email`"),
        "{}",
        said(&out)
    );
}

/// A mapping of a capability whose mapping isn't read yet is
/// `toolchain.not_built_yet`, and one that computes a value or names no
/// capability a package fills is `package.provider_invalid` (area 11,
/// "How a package provides a capability").
#[test]
fn a_mapping_is_data_and_names_a_capability_a_package_fills() {
    let root = folder("mappings");
    let home = "shared/providers/acme_search/v1";
    write(&root, "app.jadpo", "application {}\n");
    write(
        &root,
        &format!("{home}/package.jadpo"),
        "/// Acme's search engine.\npackage AcmeSearch.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n",
    );
    write(
        &root,
        &format!("{home}/provider.jadpo"),
        "/// Acme's search API. The connection gives its address in `host:`.\nprovider {\n    protocol: Protocol.json_http\n}\n",
    );
    write(
        &root,
        &format!("{home}/provides/search.jadpo"),
        "/// Fills `search`.\nprovide search {\n    upsert: {\n        call: documents.put\n        input: {\n            id: record.id\n            words: lower(record.words)\n            boost: 2 + 1\n            tags: [\"a\", -1, Kind.plain]\n        }\n    }\n}\n",
    );
    write(
        &root,
        &format!("{home}/provides/sms.jadpo"),
        "/// Would fill `sms`.\nprovide sms {\n}\n",
    );
    let out = jadpo(&root, &["check", "--json"]);
    assert_eq!(out.code, 1, "{}", said(&out));
    let json: serde_json::Value = serde_json::from_str(&out.stdout).expect("JSON");
    let found: Vec<(String, String)> = json["diagnostics"]
        .as_array()
        .unwrap_or_else(|| panic!("no diagnostics: {}", out.stdout))
        .iter()
        .map(|diagnostic| {
            (
                diagnostic["rule"].as_str().unwrap_or_default().to_string(),
                diagnostic["message"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            )
        })
        .collect();
    let has = |rule: &str, message: &str| {
        found
            .iter()
            .filter(|(r, m)| r == rule && m == message)
            .count()
    };
    assert_eq!(
        has(
            "toolchain.not_built_yet",
            "A package filling `search` isn't built yet"
        ),
        1,
        "{found:?}"
    );
    assert_eq!(
        has(
            "package.provider_invalid",
            "`provide search` computes a value"
        ),
        2,
        "{found:?}"
    );
    assert_eq!(
        has(
            "package.provider_invalid",
            "`provide sms` names no capability"
        ),
        1,
        "{found:?}"
    );
}
