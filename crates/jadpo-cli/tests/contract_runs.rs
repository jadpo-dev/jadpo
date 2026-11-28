//! `jadpo test --kind contract` (area 11, "Tests and fakes"; area 13,
//! "Kinds of test"; area 14, "`jadpo test`"; chunk K4c): the app's tests
//! that reach a provider operation, each call their `fakes:` doesn't
//! script sent through the connection to the provider's sandbox, the
//! connection's settings read as `jadpo dev` reads them and a live one
//! refused; an unconfirmed operation written from the provider's docs that
//! no case sends failing the run; and the app's own integrations' suites
//! taken through `--live`. The sandbox is a fake provider on this
//! machine: nothing reaches a network.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

use crate::support::fake_provider::FakeProvider;

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("contract-k4c-{name}"));
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

/// The output without a run's duration, which a slow machine adds.
fn timeless(text: &str) -> String {
    text.lines()
        .map(|line| match line.rfind(" in ") {
            Some(at) if line.ends_with(" s") => &line[..at],
            _ => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn json(out: &Output) -> serde_json::Value {
    serde_json::from_str(&out.stdout).unwrap_or_else(|_| panic!("JSON: {}", said(out)))
}

/// A courier desk booking collections through its own integration,
/// written from the courier's docs, whose sandbox is `sandbox`. With
/// `cancels`, it also cancels bookings, through an operation one test
/// scripts and no test sends.
fn desk(name: &str, sandbox: &str, cancels: bool) -> PathBuf {
    let root = folder(name);
    let home = "shared/providers/acme_couriers/v1";
    write(
        &root,
        "app.jadpo",
        "application {\n    connections: {\n        couriers: {\n            package: AcmeCouriers.v1\n            domains: [shipping]\n            credential: config.couriers_api_key\n            environment: config.couriers_environment\n        }\n    }\n}\n",
    );
    write(
        &root,
        "config.jadpo",
        "config {\n    couriers_api_key: Text {\n        secret: true\n    }\n    couriers_environment: ProviderEnvironment {\n        default: ProviderEnvironment.sandbox\n    }\n}\n",
    );
    write(
        &root,
        &format!("{home}/package.jadpo"),
        "/// Acme Couriers' booking API.\npackage AcmeCouriers.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n",
    );
    write(
        &root,
        &format!("{home}/provider.jadpo"),
        &format!(
            "/// Acme Couriers' API.\nprovider {{\n    protocol: Protocol.json_http\n    hosts: {{\n        live: \"https://api.acme-couriers.example\"\n        sandbox: \"{sandbox}\"\n    }}\n    credential: CredentialScheme.bearer\n    error_code: \"error.code\"\n}}\n"
        ),
    );
    let cancel = if cancels {
        "    cancel: {\n        method: HttpMethod.post\n        path: \"/v1/cancellations\"\n        input: CancelBooking\n        output: Booking\n        changes: true\n        source: PackageSource.written\n        docs: \"https://docs.acme-couriers.example/cancellations\"\n    }\n"
    } else {
        ""
    };
    write(
        &root,
        &format!("{home}/resources/bookings.jadpo"),
        &format!(
            "public resource bookings {{\n{cancel}    create: {{\n        method: HttpMethod.post\n        path: \"/v1/bookings\"\n        input: CreateBooking\n        output: Booking\n        changes: true\n        failures: [SlotTaken]\n        source: PackageSource.written\n        docs: \"https://docs.acme-couriers.example/bookings\"\n    }}\n}}\n"
        ),
    );
    write(
        &root,
        &format!("{home}/types/postcode.jadpo"),
        "/// Where a collection is.\npublic type Postcode = Text {\n    max_length: 8\n}\n",
    );
    write(
        &root,
        &format!("{home}/types/booking_id.jadpo"),
        "/// The courier's reference for a booking.\npublic type BookingId = Text {\n    max_length: 40\n}\n",
    );
    write(
        &root,
        &format!("{home}/types/create_booking.jadpo"),
        "/// What booking a collection takes.\npublic type CreateBooking = Object {\n    postcode: Postcode\n}\n",
    );
    write(
        &root,
        &format!("{home}/types/booking.jadpo"),
        "/// A booked collection.\npublic type Booking = Object {\n    id: BookingId\n}\n",
    );
    write(
        &root,
        &format!("{home}/failures/slot_taken.jadpo"),
        "/// The slot went before the booking arrived.\npublic type SlotTaken = Failure {\n    kind: FailureKind.conflict\n    message: \"That slot is taken.\"\n}\n",
    );
    write(
        &root,
        "domains/shipping/failures/slot_gone.jadpo",
        "/// The slot the customer chose has gone.\ntype SlotGone = Failure {\n    kind: FailureKind.conflict\n    message: \"That slot has gone; choose another.\"\n}\n",
    );
    write(
        &root,
        "domains/shipping/actions/book.jadpo",
        "use app { couriers }\nuse shipping/failures { SlotGone }\nuse shared/providers/acme_couriers\n\n/// Books a collection with the courier.\naction book(postcode: AcmeCouriers.Postcode) fails SlotGone -> AcmeCouriers.Booking {\n    return attempt couriers.bookings.create(\n        AcmeCouriers.CreateBooking {\n            postcode: postcode\n        }\n    ) {\n        slot_taken: SlotGone\n    }\n}\n",
    );
    write(
        &root,
        "domains/shipping/actions/book.test.jadpo",
        "use app { couriers }\nuse shipping/failures { SlotGone }\nuse shared/providers/acme_couriers\n\ntest \"a collection is booked\" {\n    var booking: AcmeCouriers.Booking = attempt book(\"N1 9GU\")\n    assert booking.id == \"bk_sandbox_1\"\n}\n\ntest \"a taken slot is the desk's own failure\" {\n    fakes: {\n        couriers.bookings.create: [slot_taken]\n    }\n    assert book(\"N1 9GU\") fails SlotGone\n}\n",
    );
    if cancels {
        write(
            &root,
            &format!("{home}/types/cancel_booking.jadpo"),
            "/// Which booking to cancel.\npublic type CancelBooking = Object {\n    id: BookingId\n}\n",
        );
        write(
            &root,
            "domains/shipping/actions/cancel.jadpo",
            "use app { couriers }\nuse shared/providers/acme_couriers\n\n/// Cancels a booked collection.\naction cancel(id: AcmeCouriers.BookingId) -> AcmeCouriers.Booking {\n    return couriers.bookings.cancel(\n        AcmeCouriers.CancelBooking {\n            id: id\n        }\n    )\n}\n",
        );
        write(
            &root,
            "domains/shipping/actions/cancel.test.jadpo",
            "use app { couriers }\nuse shared/providers/acme_couriers\n\ntest \"a cancelled booking comes back\" {\n    fakes: {\n        couriers.bookings.cancel: [\n            AcmeCouriers.Booking {\n                id: \"bk_1\"\n            }\n        ]\n    }\n    var booking: AcmeCouriers.Booking = cancel(\"bk_1\")\n    assert booking.id == \"bk_1\"\n}\n",
        );
    }
    let updated = jadpo(&root, &["records", "update"]);
    assert_eq!(updated.code, 0, "{}", said(&updated));
    root
}

/// A case the sandbox fails keeps its trace with `--keep-failed`, as any
/// failed test does, and `jadpo trace` shows the call it sent there
/// (area 17; chunk E0).
#[test]
fn a_failed_contract_case_keeps_its_trace() {
    let api = FakeProvider::start();
    api.answer(200, "{\"id\":\"bk_other\"}");
    let root = desk("sandbox-kept", &api.url, false);
    write(&root, ".env.local", "COURIERS_API_KEY=sandbox_key_k4c\n");
    let out = jadpo(
        &root,
        &["test", "--kind", "contract", "--keep-failed", "--seed", "1"],
    );
    assert_ne!(out.code, 0, "{}", said(&out));
    let report = said(&out);
    let id = report
        .split("jadpo trace ")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap_or_else(|| panic!("no trace named: {report}"))
        .to_string();
    assert!(id.starts_with("t-"), "{report}");
    let shown = jadpo(&root, &["trace", &id]);
    assert_eq!(shown.code, 0, "{}", said(&shown));
    assert!(
        shown.stdout.starts_with("test \"a collection is booked\""),
        "{}",
        shown.stdout
    );
    assert!(
        shown.stdout.contains("call      couriers.bookings.create")
            || shown.stdout.contains("call    couriers.bookings.create"),
        "{}",
        shown.stdout
    );
}

/// The unscripted call goes to the sandbox with the key `.env.local`
/// gives; the scripted one stays scripted.
#[test]
fn a_contract_run_sends_unscripted_calls_to_the_sandbox_and_keeps_scripted_ones() {
    let api = FakeProvider::start();
    api.answer(200, "{\"id\":\"bk_sandbox_1\"}");
    let root = desk("sandbox", &api.url, false);
    write(&root, ".env.local", "COURIERS_API_KEY=sandbox_key_k4c\n");
    let out = jadpo(
        &root,
        &["test", "--kind", "contract", "--json", "--seed", "1"],
    );
    assert_eq!(out.code, 0, "{}", said(&out));
    let document = json(&out);
    assert_eq!(document["data"]["contract"]["cases"], 2);
    assert_eq!(document["data"]["contract"]["whole"], true);
    assert_eq!(
        document["data"]["contract"]["uncovered"],
        serde_json::json!([])
    );
    let requests = api.requests();
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, "/v1/bookings");
    assert_eq!(
        requests[0].header("authorization"),
        Some("Bearer sandbox_key_k4c")
    );
    assert_eq!(requests[0].json()["postcode"], "N1 9GU");

    // The screen: one line, as a passing `jadpo test` is.
    api.answer(200, "{\"id\":\"bk_sandbox_1\"}");
    let out = jadpo(&root, &["test", "--kind", "contract", "--seed", "1"]);
    assert_eq!(out.code, 0, "{}", said(&out));
    assert_eq!(timeless(&out.stdout), "tested: 2 passed, 2 contract cases");

    // Without `--kind contract`, nothing leaves the machine.
    let plain = jadpo(&root, &["test", "--name", "taken slot", "--seed", "1"]);
    assert_eq!(plain.code, 0, "{}", said(&plain));
    assert_eq!(api.requests().len(), 2);
}

/// A connection whose environment chooses `live` is refused before
/// anything is sent, and so is one whose credential nothing sets.
#[test]
fn a_live_or_unset_connection_is_refused_before_anything_is_sent() {
    let api = FakeProvider::start();
    let root = desk("live", &api.url, false);
    write(
        &root,
        ".env.local",
        "COURIERS_API_KEY=sandbox_key_k4c\nCOURIERS_ENVIRONMENT=live\n",
    );
    let out = jadpo(&root, &["test", "--kind", "contract", "--seed", "1"]);
    assert_eq!(out.code, 1, "{}", said(&out));
    assert_eq!(
        timeless(&out.stdout),
        "error: `couriers`'s environment is live\n  \
         reason: A contract run sends its cases' calls to the provider's sandbox, never its live API, and `COURIERS_ENVIRONMENT` chooses `live`.\n  \
         next: Set `COURIERS_ENVIRONMENT` to `sandbox` for the run, then run it again (human)\n  \
         rule: test.contract_refused\n  \
         help: https://jadpo.dev/docs/errors/test.contract_refused\n\n\
         tests not run: 1 error, 13 files"
    );
    assert!(api.requests().is_empty());

    let unset = desk("unset", &api.url, false);
    let out = jadpo(&unset, &["test", "--kind", "contract", "--json"]);
    assert_eq!(out.code, 1, "{}", said(&out));
    let document = json(&out);
    assert_eq!(
        document["diagnostics"][0]["rule"], "test.contract_refused",
        "{}",
        out.stdout
    );
    assert_eq!(
        document["diagnostics"][0]["message"],
        "`couriers` has nothing set for the provider's sandbox"
    );
    assert!(
        out.stdout.contains("nothing sets `COURIERS_API_KEY`"),
        "{}",
        out.stdout
    );
    assert!(api.requests().is_empty());
}

/// An unconfirmed operation written from the provider's docs that no
/// case sends fails the run, though a test scripts it; a recording
/// confirms it, and the run passes.
#[test]
fn an_unconfirmed_written_operation_no_case_sends_fails_the_run() {
    let api = FakeProvider::start();
    api.answer(200, "{\"id\":\"bk_sandbox_1\"}");
    let root = desk("uncovered", &api.url, true);
    write(&root, ".env.local", "COURIERS_API_KEY=sandbox_key_k4c\n");
    let out = jadpo(&root, &["test", "--kind", "contract", "--seed", "1"]);
    assert_eq!(out.code, 1, "{}", said(&out));
    assert_eq!(
        timeless(&out.stdout),
        "error: `couriers.bookings.cancel` has no contract case\n \
         --> shared/providers/acme_couriers/v1/resources/bookings.jadpo:2:5\n  \
         |\n\
         2 |     cancel: {\n  \
         |     ^^^^^^ no case sent it to the sandbox\n  \
         reason: It was written from the provider's documentation and no recording confirms it, so a contract run is what checks it against the provider, and none of its cases sent it to the sandbox: no case reaches it, or each that does scripts its answer.\n  \
         next: Write a test that reaches `couriers.bookings.cancel` with no `fakes:` answer for it, or confirm it with `jadpo package record couriers bookings.cancel` (agent)\n  \
         rule: test.contract_uncovered\n  \
         help: https://jadpo.dev/docs/errors/test.contract_uncovered\n\n\
         test failed: contract run failed, 3 contract cases, 1 operation with no case"
    );
    // Only the unscripted booking went to the sandbox.
    assert_eq!(api.requests().len(), 1);

    write(
        &root,
        "shared/providers/acme_couriers/v1/recordings/bookings.cancel.json",
        "{\n  \"id\": \"{Text}\"\n}\n",
    );
    api.answer(200, "{\"id\":\"bk_sandbox_1\"}");
    let out = jadpo(&root, &["test", "--kind", "contract", "--seed", "1"]);
    assert_eq!(out.code, 0, "{}", said(&out));
    assert_eq!(timeless(&out.stdout), "tested: 3 passed, 3 contract cases");

    // A run that leaves cases out judges no operation.
    let _ = fs::remove_file(
        root.join("shared/providers/acme_couriers/v1/recordings/bookings.cancel.json"),
    );
    let out = jadpo(
        &root,
        &[
            "test",
            "--kind",
            "contract",
            "--name",
            "taken slot",
            "--json",
        ],
    );
    assert_eq!(out.code, 0, "{}", said(&out));
    let document = json(&out);
    assert_eq!(document["data"]["contract"]["whole"], false);
    assert_eq!(document["data"]["contract"]["cases"], 1);
}

/// The app's own mail package, with fixed `hosts:`, its sandbox at
/// `sandbox`, sending the app's email.
fn mail_desk(name: &str, sandbox: &str) -> PathBuf {
    let root = folder(name);
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
        &format!(
            "/// Acme's mail API at its fixed hosts.\nprovider {{\n    protocol: Protocol.json_http\n    hosts: {{\n        live: \"https://api.acme-mail.example\"\n        sandbox: \"{sandbox}\"\n    }}\n    credential: CredentialScheme.bearer\n    error_code: \"error\"\n    settings: {{\n        stream: Text\n    }}\n}}\n"
        ),
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
    write(
        &root,
        &format!("{home}/recordings/messages.send.json"),
        "{\n  \"message_id\": \"{Text}\"\n}\n",
    );
    let updated = jadpo(&root, &["records", "update"]);
    assert_eq!(updated.code, 0, "{}", said(&updated));
    root
}

/// The run takes the email suite of the app's own mail package through
/// `--live`, through the connection `email:` names, to the app's own
/// sender: here the sandbox on this machine.
#[test]
fn the_run_takes_the_apps_own_integrations_suites_live() {
    let api = FakeProvider::start();
    for _ in 0..3 {
        api.answer(200, "{\"message_id\":\"m_sandbox\"}");
    }
    let root = mail_desk("suite", &api.url);
    write(&root, ".env.local", "ACME_KEY=sandbox_key_k4c\n");
    let out = jadpo(&root, &["test", "--kind", "contract", "--json"]);
    assert_eq!(out.code, 0, "{}", said(&out));
    let document = json(&out);
    let suites = &document["data"]["contract"]["suites"];
    assert_eq!(suites[0]["package"], "AcmeMail.v1", "{suites}");
    assert_eq!(suites[0]["connection"], "mail");
    assert_eq!(suites[0]["capability"], "email");
    assert_eq!(suites[0]["conforms"], true);
    assert_eq!(suites[0]["against"], "live");
    let requests = api.requests();
    assert_eq!(requests.len(), 3, "{requests:?}");
    for request in &requests {
        assert_eq!(request.json()["recipient"], "desk@acme.test");
        assert_eq!(
            request.header("authorization"),
            Some("Bearer sandbox_key_k4c")
        );
    }

    for _ in 0..3 {
        api.answer(200, "{\"message_id\":\"m_sandbox\"}");
    }
    let out = jadpo(&root, &["test", "--kind", "contract"]);
    assert_eq!(out.code, 0, "{}", said(&out));
    assert_eq!(
        timeless(&out.stdout),
        "tested: contract run passed, 0 contract cases, email conforms live"
    );

    // Its connection is held to the sandbox too.
    write(
        &root,
        ".env.local",
        "ACME_KEY=sandbox_key_k4c\nACME_ENVIRONMENT=live\n",
    );
    let out = jadpo(&root, &["test", "--kind", "contract"]);
    assert_eq!(out.code, 1, "{}", said(&out));
    assert!(
        out.stdout.contains("`mail`'s environment is live"),
        "{}",
        out.stdout
    );
    assert_eq!(api.requests().len(), 6);
}

/// A contract run runs once, and `--kind` names it among the kinds.
#[test]
fn a_contract_run_doesnt_watch_and_is_named_among_the_kinds() {
    let root = desk("arguments", "https://sandbox.acme-couriers.example", false);
    let out = jadpo(&root, &["test", "--kind", "contract", "--watch"]);
    assert_eq!(out.code, 2, "{}", said(&out));
    assert!(
        said(&out).contains("`--kind contract` doesn't watch"),
        "{}",
        said(&out)
    );
    let out = jadpo(&root, &["test", "--kind", "contrat"]);
    assert_eq!(out.code, 2, "{}", said(&out));
    assert!(
        said(&out).contains("Did you mean `contract`?"),
        "{}",
        said(&out)
    );
}
