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
    let plain = jadpo(&root, &["check", "."]);
    assert!(
        plain
            .stdout
            .contains("next: Use `slot_taken` instead of `slot_takn` (agent)"),
        "{}",
        plain.stdout
    );
}
