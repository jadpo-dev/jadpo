//! Marked fields in logs and the access records against live PostgreSQL
//! (area 4, "Logging"; area 15, "Marked fields in logs", "Sensitive
//! fields" and "Access records"): a failure's log event holds a marked
//! field as a keyed hash whatever its type, an identifier and a number
//! as values; every export, erasure and read of a
//! sensitive field leaves a long-kept event without values; and an audited
//! entity's marked field is kept in its history as that hash. The tests
//! run interpreted here, and compiled with the interpreter's records,
//! each event among them, in the gate's compiled step
//! (`crates/jadpo-cli/tests/compiled.rs`, "marked fields in logs").

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;
use serde_json::Value;

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    run(&args, &env.with_cwd(cwd))
}

/// A clinic's patients: a `born_on` marked `personal:` and a `condition`
/// of a sensitive category, an audited consent with a phone number and a
/// failure holding what the marked fields gave it.
pub(crate) fn files() -> Vec<(String, String)> {
    FILES
        .iter()
        .map(|(path, text)| (path.to_string(), text.to_string()))
        .collect()
}

pub(crate) const FILES: [(&str, &str); 20] = [
    (
        "app.jadpo",
        r#"application {
    connections: {
        main: {
            driver: Driver.postgres
        }
        erasures: {
            driver: Driver.postgres
        }
    }
    privacy: {
        erasure_log: {
            store: erasures
        }
    }
}
"#,
    ),
    (
        "domains/care/types/name.jadpo",
        "/// A name or a line of text.\ntype Name = Text {\n    max_length: 80\n}\n",
    ),
    (
        "domains/care/entities/patient.jadpo",
        r#"use care/types { Name }

/// A day of birth.
type BornOn = CalendarDate {}

/// Someone the clinic looks after.
entity Patient {
    id: Uuid
    handle: Name {
        unique: true
        personal: PersonalData.identifying
    }
    born_on: BornOn {
        personal: PersonalData.demographic
    }
    condition: Name? {
        personal: PersonalData.health
    }

    search: {
        fields: [condition]
        language: SearchLanguage.english
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
        update: [Access.public]
        delete: [Access.public]
    }

    person: true
}
"#,
    ),
    (
        "domains/care/entities/consent.jadpo",
        r#"use care/entities { Patient }
use care/types { Name }

/// A patient's agreement, kept with a history of every change.
entity Consent {
    id: Uuid
    patient_id: Patient.id {
        on_delete: OnDelete.restricted
        erase: Erasure.deleted
    }
    phone: PhoneNumber
    memo: Name

    audit: {}

    policy: {
        create: [Access.public]
        read: [Access.public]
        update: [Access.public]
        delete: [Access.public]
    }
}
"#,
    ),
    (
        "domains/care/failures/handle_taken.jadpo",
        "/// Someone has that handle already.\ntype HandleTaken = Failure {\n    kind: FailureKind.conflict\n    message: \"That handle is taken.\"\n}\n",
    ),
    (
        "domains/care/failures/check_failed.jadpo",
        r#"use care/entities { Patient }
use care/types { Name }

/// The patient can't be booked.
type CheckFailed = Failure {
    kind: FailureKind.conflict
    message: "The patient can't be booked."
    internal: {
        patient_id: Patient.id
        born_on: Patient.born_on
        visits: Int
        note: Name
        urgent: Bool
        address: Email
    }
}
"#,
    ),
    (
        "domains/care/actions/join.jadpo",
        r#"use care/entities { Patient }
use care/failures { HandleTaken }
use care/types { Name }

/// Registers a patient.
action join(handle: Name, born_on: Patient.born_on, condition: Name?) fails HandleTaken -> Patient {
    return attempt Patient.create {
        set: {
            handle: handle
            born_on: born_on
            condition: condition
        }
        conflict: {
            Patient.handle: HandleTaken
        }
    }
}
"#,
    ),
    (
        "domains/care/actions/check.jadpo",
        r#"use care/entities { Patient, PatientNotFound }
use care/failures { CheckFailed }
use care/types { Name }

/// Refuses a booking, carrying what the patient's record says.
action check(patient_id: Patient.id, note: Name) fails CheckFailed, PatientNotFound -> Unit {
    var patient: Patient = attempt Patient.get(patient_id)
    reject CheckFailed {
        patient_id: patient.id
        born_on: patient.born_on
        visits: 3
        note: note
        urgent: true
        address: "ann@example.com"
    }
}
"#,
    ),
    (
        "domains/care/actions/give_consent.jadpo",
        r#"use care/entities { Consent, Patient, PatientNotFound }
use care/types { Name }

/// Records a patient's consent.
action give_consent(patient_id: Patient.id, phone: PhoneNumber, memo: Name) fails PatientNotFound -> Consent {
    return attempt Consent.create {
        set: {
            patient_id: patient_id
            phone: phone
            memo: memo
        }
    }
}
"#,
    ),
    (
        "domains/care/actions/change_phone.jadpo",
        r#"use care/entities { Consent, ConsentNotFound }

/// Moves a consent to a new number.
action change_phone(consent_id: Consent.id, phone: PhoneNumber) fails ConsentNotFound -> Consent {
    return attempt Consent.update(consent_id) {
        set: {
            phone: phone
        }
    }
}
"#,
    ),
    (
        "domains/care/actions/forget.jadpo",
        r#"use care/entities { Patient, PatientNotFound }

/// Erases a patient.
action forget(patient_id: Patient.id) fails PatientNotFound -> Unit {
    attempt Patient.erase(patient_id)
}
"#,
    ),
    (
        "domains/care/actions/forget_then_refuse.jadpo",
        r#"use care/entities { Patient, PatientNotFound }
use care/failures { CheckFailed }

/// Erases a patient and then fails, so nothing it did is kept.
action forget_then_refuse(patient_id: Patient.id) fails CheckFailed, PatientNotFound -> Unit {
    attempt Patient.erase(patient_id)
    reject CheckFailed {
        patient_id: patient_id
        born_on: Patient.born_on(CalendarDate("2000-01-01"))
        visits: 1
        note: "erased, then refused"
        urgent: false
        address: "nobody@example.com"
    }
}
"#,
    ),
    (
        "domains/care/actions/fetch_export.jadpo",
        r#"use care/entities { Patient }

/// A patient's export.
action fetch_export(patient_id: Patient.id) -> PersonalDataExport {
    return Patient.export(patient_id)
}
"#,
    ),
    (
        "domains/care/queries/patient_names.jadpo",
        r#"use care/entities { Patient }
use care/types { NameRow }

/// Each patient's name, with no sensitive field.
query patient_names() -> List<NameRow> {
    from: Patient
    order: [Patient.handle asc]
    limit: 20
}
"#,
    ),
    (
        "domains/care/queries/with_condition.jadpo",
        r#"use care/entities { Patient }
use care/types { NameRow }

/// Those whose condition matches a search.
query with_condition(phrase: SearchText) -> List<NameRow> {
    from: Patient
    where: Patient matches phrase
    limit: 20
}
"#,
    ),
    (
        "domains/care/types/name_row.jadpo",
        "use care/entities { Patient }\n\n/// A patient's name, and nothing sensitive.\ntype NameRow = Output {\n    pick: [Patient.id, Patient.handle]\n}\n",
    ),
    (
        "domains/care/routes/care.jadpo",
        r#"use care/types { CheckAsked }
use care/actions { check }

/// What the clinic answers.
routes Care {
    prefix: /care

    /// Refuses a booking.
    POST /check {
        authentication: RouteAuthentication.none
        input: CheckAsked
        status: SuccessStatus.no_content
        run: check(input.patient_id, input.note)
    }
}
"#,
    ),
    (
        "domains/care/types/check_asked.jadpo",
        r#"use care/entities { Patient }
use care/types { Name }

/// A booking to check.
type CheckAsked = Input {
    patient_id: Patient.id
    note: Name
}
"#,
    ),
    (
        "policy.jadpo",
        r#"use care

policy {
    actions: {
        care.check: [Access.public]
    }
}
"#,
    ),
    (
        "domains/care/actions/check.test.jadpo",
        r#"use care/entities { Consent, Patient }
use care/actions { change_phone, fetch_export, forget, forget_then_refuse, give_consent, join }
use care/failures { CheckFailed }
use care/types { CheckAsked, NameRow }
use care/queries { patient_names, with_condition }

test "a refused booking is logged with its marked field hashed" {
    var ann: Patient = attempt join("ann", Patient.born_on(CalendarDate("1990-03-01")), none)
    var response: Response = request {
        method: HttpMethod.post
        path: "/care/check"
        input: CheckAsked {
            patient_id: ann.id
            note: "ann's note"
        }
    }
    assert response.status == 409
}

test "exporting a person is recorded" {
    var ann: Patient = attempt join("ann", Patient.born_on(CalendarDate("1990-03-01")), none)
    var export: PersonalDataExport = fetch_export(ann.id)
    assert export.records == 1
}

test "erasing a person is recorded" {
    var ann: Patient = attempt join("ann", Patient.born_on(CalendarDate("1990-03-01")), none)
    attempt forget(ann.id)
}

test "an erasure that rolls back is not recorded" {
    var ann: Patient = attempt join("ann", Patient.born_on(CalendarDate("1990-03-01")), none)
    assert forget_then_refuse(ann.id) fails CheckFailed
}

test "reading a sensitive field is recorded" {
    var ann: Patient = attempt join("ann", Patient.born_on(CalendarDate("1990-03-01")), "asthma")
    var read: Patient = attempt Patient.get(ann.id)
    assert read.condition == "asthma"
}

test "a query that reads no sensitive field records nothing" {
    var ann: Patient = attempt join("ann", Patient.born_on(CalendarDate("1990-03-01")), "asthma")
    var rows: List<NameRow> = patient_names()
    assert ann.handle == "ann"
    assert list.map(rows, row -> row.handle) == ["ann"]
}

test "searching a sensitive field is recorded" {
    var ann: Patient = attempt join("ann", Patient.born_on(CalendarDate("1990-03-01")), "asthma")
    var rows: List<NameRow> = with_condition("asthma")
    assert ann.handle == "ann"
    assert list.map(rows, row -> row.handle) == ["ann"]
}

test "a consent's history keeps a phone number as a hash" {
    var ann: Patient = attempt join("ann", Patient.born_on(CalendarDate("1990-03-01")), none)
    var consent: Consent = attempt give_consent(ann.id, "+442079460958", "first")
    var moved: Consent = attempt change_phone(consent.id, "+442079460000")
    assert moved.phone == "+442079460000"
    var page: Page<AuditEntry> = Consent.history(consent.id)
    assert list.count(page.items) == 2
    assert list.map(page.items, entry -> list.map(entry.fields, change -> change.field)) == [["phone"], ["patient_id", "phone", "memo"]]
    assert list.map(page.items, entry -> list.map(entry.fields, change -> change.after == "+442079460000")) == [[false], [false, false, false]]
}
"#,
    ),
];

/// A project of the files in a folder of its own, migrated.
fn project(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("jadpo-marked-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files() {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        fs::write(path, text).expect("write");
    }
    for args in [&["migrate", "new", "initial"][..], &["records", "update"]] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{args:?}: {}{}", done.stdout, done.stderr);
    }
    root
}

/// The log events of the test named `name`.
fn events(run: &jadpo_test::Run, name: &str) -> Vec<Value> {
    let test = run
        .results
        .iter()
        .find(|result| result.test.name == name)
        .unwrap_or_else(|| panic!("no test called {name}"));
    let record = test.record.as_ref().expect("a record");
    record
        .lines()
        .iter()
        .filter_map(|line| line.strip_prefix("log "))
        .map(|json| serde_json::from_str(json).expect("an event"))
        .collect()
}

/// A hash of something this test can't see, by its shape.
fn is_hash(value: &Value) -> bool {
    value
        .as_str()
        .and_then(|text| text.strip_prefix("hmac-sha256:"))
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
}

#[test]
fn log_events_show_what_the_pages_say() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("events");
    let run = jadpo_test::run(jadpo_test::Options {
        cwd: root.clone(),
        target: Some(".".into()),
        seed: Some(1),
        record: true,
        ..jadpo_test::Options::default()
    });
    let jadpo_test::Ran::Done(run) = run else {
        panic!("the tests didn't run: {run:?}");
    };
    let failures: Vec<_> = run
        .results
        .iter()
        .filter_map(|result| {
            result
                .failure
                .as_ref()
                .map(|failure| (&result.test.name, failure))
        })
        .collect();
    assert!(failures.is_empty(), "{failures:#?}");

    // The failure's event: a marked date hashed, a number, a boolean and
    // an identifier as values, text hashed.
    let events_of = |name: &str| events(&run, name);
    let check = events_of("a refused booking is logged with its marked field hashed");
    let rejected = check
        .iter()
        .find(|event| event["event"] == "failure.rejected")
        .unwrap_or_else(|| panic!("no failure.rejected in {check:?}"));
    assert_eq!(rejected["code"], "check_failed", "{rejected}");
    assert_eq!(rejected["kind"], "conflict", "{rejected}");
    assert!(
        rejected["site"]
            .as_str()
            .is_some_and(|site| site.starts_with("domains/care/actions/check.jadpo:")),
        "{rejected}"
    );
    assert!(
        rejected["trace"].as_array().is_some_and(|trace| trace
            .iter()
            .any(|entry| entry == "action check")
            && trace
                .last()
                .is_some_and(|last| last == "route POST /care/check")),
        "{rejected}"
    );
    let internal = &rejected["internal"];
    assert!(is_hash(&internal["born_on"]), "{internal}");
    assert_eq!(internal["visits"], 3, "{internal}");
    assert_eq!(internal["urgent"], true, "{internal}");
    assert!(is_hash(&internal["note"]), "{internal}");
    assert!(is_hash(&internal["address"]), "{internal}");
    assert!(
        internal["patient_id"]
            .as_str()
            .is_some_and(|id| id.len() == 36 || id.starts_with("id#")),
        "{internal}"
    );
    let rejected = rejected.to_string();
    assert!(!rejected.contains("1990-03-01"), "{rejected}");
    assert!(!rejected.contains("ann@example.com"), "{rejected}");
    assert!(!rejected.contains("ann's note"), "{rejected}");

    // Exports and erasures: who asked, for whom, kept long, never a value.
    let exported = events_of("exporting a person is recorded");
    let record = exported
        .iter()
        .find(|event| event["event"] == "privacy.exported")
        .unwrap_or_else(|| panic!("no privacy.exported in {exported:?}"));
    assert_eq!(record["entity"], "Patient", "{record}");
    assert_eq!(record["kept"], "long", "{record}");
    assert_eq!(record["by"], "no_caller", "{record}");
    assert!(record["person_id"].is_string(), "{record}");
    assert!(record["at"].is_string(), "{record}");

    let erased = events_of("erasing a person is recorded");
    let record = erased
        .iter()
        .find(|event| event["event"] == "privacy.erased")
        .unwrap_or_else(|| panic!("no privacy.erased in {erased:?}"));
    assert_eq!(record["entity"], "Patient", "{record}");
    assert_eq!(record["kept"], "long", "{record}");

    // A read of a sensitive field names the field and the person; a read
    // of the rest, nothing.
    let rolled = events_of("an erasure that rolls back is not recorded");
    assert!(
        rolled
            .iter()
            .all(|event| event["event"] != "privacy.erased"),
        "{rolled:?}"
    );

    let read = events_of("reading a sensitive field is recorded");
    let record = read
        .iter()
        .find(|event| event["event"] == "privacy.read")
        .unwrap_or_else(|| panic!("no privacy.read in {read:?}"));
    assert_eq!(record["entity"], "Patient", "{record}");
    assert_eq!(
        record["fields"],
        serde_json::json!(["condition"]),
        "{record}"
    );
    assert!(record["person_id"].is_string(), "{record}");
    assert!(!record.to_string().contains("asthma"), "{record}");
    let quiet = events_of("a query that reads no sensitive field records nothing");
    assert!(
        quiet.iter().all(|event| event["event"] != "privacy.read"),
        "{quiet:?}"
    );

    // A history keeps a phone number only as its hash.
    let consent = run
        .results
        .iter()
        .find(|result| result.test.name == "a consent's history keeps a phone number as a hash")
        .and_then(|result| result.record.as_ref())
        .expect("its record");
    let rows: Vec<&String> = consent
        .lines()
        .iter()
        .filter(|line| line.starts_with("row ") && line.contains("jadpo_audit"))
        .collect();
    assert!(!rows.is_empty(), "{:?}", consent.lines());
    for row in &rows {
        assert!(!row.contains("+442079460000"), "{row}");
        assert!(!row.contains("+442079460958"), "{row}");
    }
    assert!(
        rows.iter().any(|row| row.contains("hmac-sha256:")),
        "{rows:?}"
    );

    // A search reads the words of what it searches.
    let searched = events_of("searching a sensitive field is recorded");
    let record = searched
        .iter()
        .find(|event| event["event"] == "privacy.read")
        .unwrap_or_else(|| panic!("no privacy.read in {searched:?}"));
    assert_eq!(
        record["fields"],
        serde_json::json!(["condition"]),
        "{record}"
    );
    let _ = fs::remove_dir_all(&root);
}
