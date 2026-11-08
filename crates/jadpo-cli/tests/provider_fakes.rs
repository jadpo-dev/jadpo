//! A package's fake in `jadpo test` (area 11, "Tests and fakes"; area 13,
//! "Fakes for integrations"; chunk 7.2d): every provider call a test
//! reaches is answered by its package's fake, with the operation's
//! generated default unless the test's `fakes:` scripts its answers,
//! each a value of its output type, one of its failures by its code, or a
//! condition; a script that runs out or has answers left fails the test;
//! and the compiler checks every scripted answer against the package.
//!
//! The project has no tables, so nothing here needs PostgreSQL.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

use crate::support;

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("fakes-7.2d-{name}"));
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
    let mut output = run(&args, &Environment::empty().with_cwd(cwd));
    output.stdout = support::without_run_time(&output.stdout);
    output
}

/// A courier desk calling its own integration: `bookings.create` answers
/// a `Booking` inside a `data` envelope, with its collection time spelt
/// `collectAt`, and fails with `slot_taken`.
pub(crate) fn desk(name: &str, test: &str) -> PathBuf {
    let root = folder(name);
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
        "shared/providers/acme_couriers/v1/package.jadpo",
        "/// Acme Couriers' booking API.\npackage AcmeCouriers.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n",
    );
    write(
        &root,
        "shared/providers/acme_couriers/v1/provider.jadpo",
        "/// Acme Couriers' API.\nprovider {\n    protocol: Protocol.json_http\n    hosts: {\n        live: \"https://api.acme-couriers.example\"\n        sandbox: \"https://sandbox.acme-couriers.example\"\n    }\n    credential: CredentialScheme.bearer\n    idempotency: {\n        header: \"Idempotency-Key\"\n        window: 24h\n    }\n    error_code: \"error.code\"\n}\n",
    );
    write(
        &root,
        "shared/providers/acme_couriers/v1/resources/bookings.jadpo",
        "public resource bookings {\n    create: {\n        method: HttpMethod.post\n        path: \"/v1/bookings\"\n        input: CreateBooking\n        output: Booking\n        output_at: \"data\"\n        changes: true\n        idempotent: true\n        failures: [SlotTaken]\n        source: PackageSource.written\n        docs: \"https://docs.acme-couriers.example/bookings\"\n    }\n}\n",
    );
    write(
        &root,
        "shared/providers/acme_couriers/v1/types/create_booking.jadpo",
        "/// What booking a collection takes.\npublic type CreateBooking = Object {\n    postcode: Postcode\n}\n",
    );
    write(
        &root,
        "shared/providers/acme_couriers/v1/types/booking.jadpo",
        "/// A booked collection.\npublic type Booking = Object {\n    id: Text {\n        min_length: 3\n    }\n    collect_at: Instant {\n        wire_name: \"collectAt\"\n    }\n    windows: List<Window>\n    note: Text?\n}\n",
    );
    write(
        &root,
        "shared/providers/acme_couriers/v1/types/window.jadpo",
        "/// When in the day a collection comes.\npublic type Window = Enum {\n    morning\n    afternoon\n}\n",
    );
    write(
        &root,
        "shared/providers/acme_couriers/v1/failures/slot_taken.jadpo",
        "/// The slot went before the booking arrived.\npublic type SlotTaken = Failure {\n    kind: FailureKind.conflict\n    message: \"That slot is taken.\"\n}\n",
    );
    write(
        &root,
        "domains/shipping/failures/slot_gone.jadpo",
        "type SlotGone = Failure {\n    kind: FailureKind.conflict\n    message: \"That slot has gone; choose another.\"\n}\n",
    );
    write(
        &root,
        "shared/providers/acme_couriers/v1/types/postcode.jadpo",
        "/// Where a collection is.\npublic type Postcode = Text {\n    max_length: 8\n}\n",
    );
    write(
        &root,
        "domains/shipping/actions/book.jadpo",
        "use app { couriers }\nuse shipping/failures { SlotGone }\nuse shared/providers/acme_couriers\n\n/// Books a collection.\naction book(postcode: AcmeCouriers.Postcode) fails SlotGone -> AcmeCouriers.Booking {\n    return attempt couriers.bookings.create(AcmeCouriers.CreateBooking {\n        postcode: postcode\n    }) {\n        slot_taken: SlotGone\n    }\n}\n",
    );
    write(&root, "domains/shipping/actions/book.test.jadpo", test);
    root
}

/// What every scripting test file brings in.
const HEAD: &str = "use app { couriers }\nuse shipping/failures { SlotGone }\n\n";

/// Unscripted, an operation answers with its generated default: a value
/// of its output type that keeps its rules, read back through the
/// operation's real decoding, its envelope and wire names included.
#[test]
fn an_unscripted_call_answers_with_its_generated_default() {
    let root = desk(
        "default",
        "use shared/providers/acme_couriers\n\ntest \"a booking is made\" {\n    var booking: AcmeCouriers.Booking = attempt book(\"N1 9GU\")\n    assert booking.id == \"example\"\n    assert list.count(booking.windows) == 0\n    assert booking.note == none\n    assert booking.collect_at == clock.now\n}\n",
    );
    let output = jadpo(&root, &["test", ".", "--seed", "1"]);
    assert_eq!(
        (output.stdout.as_str(), output.code),
        ("tested: 1 passed, 1 action test\n", 0),
        "{}",
        output.stderr
    );
}

/// Scripted, the calls take the answers in order: a value of the output
/// type, then one of its failures by its code, which the action maps.
#[test]
fn scripted_answers_are_taken_in_order() {
    let root = desk(
        "scripted",
        &format!(
            "{HEAD}use shared/providers/acme_couriers\n\ntest \"a second booking finds its slot taken\" {{\n    fakes: {{\n        couriers.bookings.create: [AcmeCouriers.Booking {{\n            id: \"b-17\"\n            collect_at: \"2026-10-20T09:00:00Z\"\n            windows: []\n            note: \"Ring twice\"\n        }}, slot_taken]\n    }}\n    var booking: AcmeCouriers.Booking = attempt book(\"N1 9GU\")\n    assert booking.id == \"b-17\"\n    assert booking.note == \"Ring twice\"\n    assert book(\"N1 9GU\") fails SlotGone\n}}\n"
        ),
    );
    let output = jadpo(&root, &["test", ".", "--seed", "1"]);
    assert_eq!(
        (output.stdout.as_str(), output.code),
        ("tested: 1 passed, 1 action test\n", 0),
        "{}",
        output.stderr
    );
}

/// A scripted value writes a package enum's values through the package's
/// name, as code does (chunk 7.1e), and the code reads them back.
#[test]
fn a_scripted_answer_writes_a_package_enum_through_its_name() {
    let root = desk(
        "enum",
        "use app { couriers }\nuse shared/providers/acme_couriers\n\ntest \"a booking keeps its windows\" {\n    fakes: {\n        couriers.bookings.create: [AcmeCouriers.Booking {\n            id: \"b-18\"\n            collect_at: \"2026-10-20T09:00:00Z\"\n            windows: [AcmeCouriers.Window.morning, AcmeCouriers.Window.afternoon]\n            note: none\n        }]\n    }\n    var booking: AcmeCouriers.Booking = attempt book(\"N1 9GU\")\n    assert booking.windows == [AcmeCouriers.Window.morning, AcmeCouriers.Window.afternoon]\n    assert booking.windows != [AcmeCouriers.Window.afternoon]\n}\n",
    );
    let output = jadpo(&root, &["test", ".", "--seed", "1"]);
    assert_eq!(
        (output.stdout.as_str(), output.code),
        ("tested: 1 passed, 1 action test\n", 0),
        "{}",
        output.stderr
    );
}

/// A call past the script fails the test, naming the operation, how many
/// answers were scripted and how many calls came.
#[test]
fn a_call_past_the_script_fails_the_test() {
    let root = desk(
        "exhausted",
        &format!(
            "{HEAD}test \"a taken slot is reported\" {{\n    fakes: {{\n        couriers.bookings.create: [slot_taken]\n    }}\n    assert book(\"N1 9GU\") fails SlotGone\n    assert book(\"N1 9GU\") fails SlotGone\n}}\n"
        ),
    );
    let output = jadpo(&root, &["test", ".", "--seed", "1"]);
    assert_eq!(output.code, 1);
    assert_eq!(
        output.stdout,
        "\
FAIL  a taken slot is reported
 --> domains/shipping/actions/book.test.jadpo:6

  fake: `couriers.bookings.create` was called twice, and its `fakes:` scripts 1 answer
  reason: A scripted operation answers each call with the next scripted answer, so a call past the last means the code calls it more often than the test says; that call was answered with the generated default.
  next: Script an answer for every call of `couriers.bookings.create`, or find why the code calls it again (agent)

  rule: test.fake_exhausted
  rerun: jadpo test domains/shipping/actions/book.test.jadpo:6 --seed 1

test failed: 1 failed, 1 action test
"
    );
    let json = jadpo(&root, &["test", ".", "--seed", "1", "--json"]);
    let document: serde_json::Value = serde_json::from_str(&json.stdout).expect("JSON");
    let failure = &document["data"]["tests"][0]["failure"];
    assert_eq!(failure["code"], "test.fake_exhausted");
    assert_eq!(
        failure["fake"],
        serde_json::json!({ "operation": "couriers.bookings.create", "scripted": 1, "called": 2 })
    );
}

/// Answers left over fail a test that otherwise passed.
#[test]
fn answers_left_over_fail_the_test() {
    let root = desk(
        "unused",
        &format!(
            "{HEAD}test \"a taken slot is reported\" {{\n    fakes: {{\n        couriers.bookings.create: [slot_taken, unavailable]\n    }}\n    assert book(\"N1 9GU\") fails SlotGone\n}}\n"
        ),
    );
    let output = jadpo(&root, &["test", ".", "--seed", "1"]);
    assert_eq!(output.code, 1);
    assert_eq!(
        output.stdout,
        "\
FAIL  a taken slot is reported
 --> domains/shipping/actions/book.test.jadpo:6

  fake: `couriers.bookings.create` was called once, and its `fakes:` scripts 2 answers
  reason: Each scripted answer is for a call the test expects, so one left over means the code calls the operation less often than the test says.
  next: Remove the answers no call takes, or find why the code doesn't call `couriers.bookings.create` (agent)

  rule: test.fake_unused
  rerun: jadpo test domains/shipping/actions/book.test.jadpo:6 --seed 1

test failed: 1 failed, 1 action test
"
    );
}

/// The compiler checks every scripted answer against the package: a
/// name that is none of the operation's failures' codes or a condition,
/// a value of another type, and an operation the package doesn't have.
#[test]
fn every_scripted_answer_is_checked_against_the_package() {
    let root = desk(
        "invalid",
        &format!(
            "{HEAD}test \"a taken slot is reported\" {{\n    fakes: {{\n        couriers.bookings.create: [slot_takn, 5]\n        couriers.bookings.cancel: [unavailable]\n    }}\n    assert book(\"N1 9GU\") fails SlotGone\n}}\n"
        ),
    );
    jadpo(&root, &["records", "update"]);
    let output = jadpo(&root, &["check", ".", "--json"]);
    assert_eq!(output.code, 1);
    let document: serde_json::Value = serde_json::from_str(&output.stdout).expect("JSON");
    let rules: Vec<String> = document["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .map(|diagnostic| diagnostic["rule"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(
        rules,
        [
            "semantic.fake_invalid",
            "type.mismatch",
            "semantic.fake_invalid"
        ]
    );
    jadpo(&root, &["records", "update"]);
    let plain = jadpo(&root, &["check", "."]);
    assert!(
        plain
            .stdout
            .contains("next: Use `slot_taken` instead of `slot_takn` (agent)"),
        "{}",
        plain.stdout
    );
}

/// `.requests` lists what the code sent, in order and typed as the
/// operation's input, so a test reads a sent field as any other value;
/// an operation it never called lists nothing.
#[test]
fn a_fakes_requests_are_typed_as_what_was_sent() {
    let root = desk(
        "requests",
        "use app { couriers }\nuse shared/providers/acme_couriers\n\ntest \"both bookings are kept in order\" {\n    fakes: {\n        couriers.bookings.create: [\n            AcmeCouriers.Booking {\n                id: \"b-1\"\n                collect_at: \"2026-10-20T09:00:00Z\"\n                windows: []\n                note: none\n            },\n            AcmeCouriers.Booking {\n                id: \"b-2\"\n                collect_at: \"2026-10-20T09:00:00Z\"\n                windows: []\n                note: none\n            }\n        ]\n    }\n    var first: AcmeCouriers.Booking = attempt book(\"N1 9GU\")\n    var second: AcmeCouriers.Booking = attempt book(\"E2 8AA\")\n    assert first.id != second.id\n    var sent: List<AcmeCouriers.CreateBooking> = couriers.bookings.create.requests\n    assert list.count(sent) == 2\n    assert list.first(sent) == AcmeCouriers.CreateBooking {\n        postcode: \"N1 9GU\"\n    }\n    assert list.map(sent, booking -> booking.postcode) == [\"N1 9GU\", \"E2 8AA\"]\n}\n\ntest \"nothing sent lists nothing\" {\n    assert list.count(couriers.bookings.create.requests) == 0\n}\n",
    );
    let output = jadpo(&root, &["test", ".", "--seed", "1"]);
    assert_eq!(
        (output.stdout.as_str(), output.code),
        ("tested: 2 passed, 1 pure test, 1 action test\n", 0),
        "{}\n{}",
        output.stdout,
        output.stderr
    );
}

/// A retry is another request: an answer that said it did nothing leaves
/// no key to repeat under, so the code sent it twice (area 13, "Scripting
/// answers").
#[test]
fn a_send_that_did_nothing_and_went_again_is_listed_twice() {
    let root = desk(
        "requests-retried",
        "use app { couriers }\nuse shared/providers/acme_couriers\n\ntest \"the retried booking was sent twice\" {\n    fakes: {\n        couriers.bookings.create: [\n            unavailable,\n            AcmeCouriers.Booking {\n                id: \"b-1\"\n                collect_at: \"2026-10-20T09:00:00Z\"\n                windows: []\n                note: none\n            }\n        ]\n    }\n    var booking: AcmeCouriers.Booking = attempt book(\"N1 9GU\")\n    assert booking.id == \"b-1\"\n    assert list.count(couriers.bookings.create.requests) == 2\n}\n",
    );
    let output = jadpo(&root, &["test", ".", "--seed", "1"]);
    assert_eq!(
        (output.stdout.as_str(), output.code),
        ("tested: 1 passed, 1 action test\n", 0),
        "{}\n{}",
        output.stdout,
        output.stderr
    );
}

/// A field the input marks secret reads as redacted, never its value: a
/// test can't confirm what was sent for it, and no report shows it.
#[test]
fn a_secret_field_of_a_request_reads_as_redacted() {
    let root = desk(
        "requests-secret",
        "use app { couriers }\nuse shared/providers/acme_couriers\nuse shipping/actions { sign }\n\ntest \"the signing key is never read back\" {\n    var booking: AcmeCouriers.Booking = sign(\"N1 9GU\", \"sk_live_abcdef\")\n    assert booking.id == \"example\"\n    var sent: List<AcmeCouriers.SignedBooking> = couriers.bookings.sign.requests\n    assert list.map(sent, signed -> signed.postcode) == [\"N1 9GU\"]\n    assert list.all(sent, signed -> signed.signing_key != \"sk_live_abcdef\")\n}\n\ntest \"a failing assertion shows it redacted\" {\n    var kept: AcmeCouriers.Booking = sign(\"N1 9GU\", \"sk_live_abcdef\")\n    assert kept.id == \"example\"\n    assert couriers.bookings.sign.requests == []\n}\n",
    );
    write(
        &root,
        "shared/providers/acme_couriers/v1/resources/bookings.jadpo",
        "public resource bookings {\n    create: {\n        method: HttpMethod.post\n        path: \"/v1/bookings\"\n        input: CreateBooking\n        output: Booking\n        output_at: \"data\"\n        changes: true\n        idempotent: true\n        failures: [SlotTaken]\n        source: PackageSource.written\n        docs: \"https://docs.acme-couriers.example/bookings\"\n    }\n    sign: {\n        method: HttpMethod.post\n        path: \"/v1/bookings/signed\"\n        input: SignedBooking\n        output: Booking\n        output_at: \"data\"\n        changes: true\n        idempotent: true\n        source: PackageSource.written\n        docs: \"https://docs.acme-couriers.example/bookings\"\n    }\n}\n",
    );
    write(
        &root,
        "shared/providers/acme_couriers/v1/types/signed_booking.jadpo",
        "/// A booking signed with the customer's key.\npublic type SignedBooking = Object {\n    postcode: Postcode\n    signing_key: ApiKey\n}\n",
    );
    write(
        &root,
        "domains/shipping/actions/sign.jadpo",
        "use app { couriers }\nuse shared/providers/acme_couriers\n\n/// Books a signed collection.\naction sign(postcode: AcmeCouriers.Postcode, signing_key: ApiKey) -> AcmeCouriers.Booking {\n    return couriers.bookings.sign(AcmeCouriers.SignedBooking {\n        postcode: postcode\n        signing_key: signing_key\n    })\n}\n",
    );
    let output = jadpo(&root, &["test", ".", "--seed", "1"]);
    assert_eq!(output.code, 1, "{}\n{}", output.stdout, output.stderr);
    assert!(
        output.stdout.contains("1 failed") && output.stdout.contains("1 passed"),
        "{}",
        output.stdout
    );
    assert!(
        !output.stdout.contains("sk_live") && output.stdout.contains("<redacted ApiKey>"),
        "{}",
        output.stdout
    );
    assert!(!output.stderr.contains("sk_live"), "{}", output.stderr);
}

/// Read anywhere but a test, `.requests` is refused, as `ai.<prompt>`'s
/// is, and what it names is checked against the package; its type is the
/// operation's input.
#[test]
fn a_fakes_requests_are_checked_against_the_package() {
    let root = desk(
        "requests-invalid",
        "use app { couriers }\nuse shared/providers/acme_couriers\n\ntest \"the wrong names\" {\n    assert list.count(couriers.bookings.cancel.requests) == 0\n    assert list.count(couriers.bookings.create.requests.requests) == 0\n    var sent: List<AcmeCouriers.Booking> = couriers.bookings.create.requests\n    assert list.count(sent) == 0\n}\n",
    );
    write(
        &root,
        "domains/shipping/actions/sent.jadpo",
        "use app { couriers }\nuse shared/providers/acme_couriers\n\n/// Not something an action can read.\naction sent() -> List<AcmeCouriers.CreateBooking> {\n    return couriers.bookings.create.requests\n}\n",
    );
    jadpo(&root, &["records", "update"]);
    let output = jadpo(&root, &["check", ".", "--json"]);
    assert_eq!(output.code, 1, "{}", output.stdout);
    let document: serde_json::Value = serde_json::from_str(&output.stdout).expect("JSON");
    let mut problems: Vec<String> = document["diagnostics"]
        .as_array()
        .expect("diagnostics")
        .iter()
        .map(|diagnostic| {
            format!(
                "{}: {}",
                diagnostic["rule"].as_str().unwrap_or_default(),
                diagnostic["message"].as_str().unwrap_or_default()
            )
        })
        .collect();
    problems.sort();
    assert_eq!(
        problems,
        [
            "semantic.not_callable: Only a test reads `couriers.bookings.create.requests`".to_string(),
            "semantic.unknown_name: `couriers.bookings.create.requests.requests` isn't a provider operation".to_string(),
            "semantic.unknown_name: `couriers` has no operation `bookings.cancel`".to_string(),
            "type.mismatch: `List<AcmeCouriers.CreateBooking>` can't be used where `List<AcmeCouriers.Booking>` is required".to_string(),
        ]
    );
}
