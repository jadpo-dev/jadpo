//! Encrypted fields and keys held as data against live PostgreSQL (area 15,
//! "Encrypted fields" and "Rotating the key"; area 19, "Encrypted fields",
//! "A key is never a value" and "How a secret is kept"; chunk P9): what goes
//! into a sealed field comes back out of it, its column holds only sealed
//! bytes, a value moved to another row or altered opens for no one, a
//! rotated key still reads what the old one sealed until the built-in
//! re-seal has moved every row, a key made in an action works and is stored
//! sealed, and the history of an audited record never holds a sealed value.
//! The rotation runs through the interpreter and the compiled executor,
//! whose plan is the canonical bytes generated code holds; `deployable.rs`
//! and `workers/sealed.rs` run this fixture's routes in real deployables.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started from
//! the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_host_native::Runtime;
use jadpo_interp::Program;
use jadpo_runtime::config::Mode;
use jadpo_runtime::schedules::tick;
use jadpo_runtime::{Executor, Outcome, SqlValue};
use jadpo_term::Environment;
use jadpo_test::host::Harness;
use jadpo_value::Value;

/// The draft-irtf-cfrg-xchacha key, 80 81 ... 9f.
pub(crate) const FIRST_KEY: &str = "gIGCg4SFhoeIiYqLjI2Oj5CRkpOUlZaXmJmam5ydnp8";
/// RFC 8032's first test key's seed.
pub(crate) const SECOND_KEY: &str = "nWGxne_9WmC6hEr0kuwsxERJxWl7MmkZcDusAxyuf2A";

const POLICY: &str = "    policy: {
        create: [Access.public]
        read: [Access.public]
        update: [Access.public]
        delete: [Access.public]
    }
";

/// A clinic: a consultation's notes are sealed, as a device's own key, its
/// authenticator's secret and its recovery text are. `deployables`, for a
/// fixture a deployable serves, is what `application { }` says of them.
pub(crate) fn files(application: &str) -> Vec<(String, String)> {
    let policy = POLICY;
    let files: Vec<(&str, String)> = vec![
        (
            "app.jadpo",
            format!(
                "application {{\n    field_encryption_key: config.field_encryption_key\n{application}}}\n"
            ),
        ),
        (
            "config.jadpo",
            "config {\n    database_url: PostgresUrl\n    field_encryption_key: EncryptionKey\n}\n"
                .to_string(),
        ),
        (
            "domains/clinic/types/clinical_note.jadpo",
            "/// What a clinician wrote.\ntype ClinicalNote = Text {\n    max_length: 2000\n}\n"
                .to_string(),
        ),
        (
            "domains/clinic/types/sealed_for_device.jadpo",
            "/// Bytes sealed for one device.\ntype SealedForDevice = Bytes {}\n".to_string(),
        ),
        (
            "domains/clinic/types/opened_for_device.jadpo",
            "/// Bytes opened for one device.\ntype OpenedForDevice = Bytes {}\n".to_string(),
        ),
        (
            "domains/clinic/types/booking_notes.jadpo",
            "use clinic/types { ClinicalNote }\n\n/// A consultation as a request books it.\ntype BookingNotes = Input {\n    notes: ClinicalNote\n}\n".to_string(),
        ),
        (
            "domains/clinic/types/notes_shown.jadpo",
            "use clinic/types { ClinicalNote }\n\n/// The notes of a consultation, and no more of it.\ntype NotesShown = Output {\n    notes: ClinicalNote\n}\n".to_string(),
        ),
        (
            "domains/clinic/types/amended_notes.jadpo",
            "use clinic/types { ClinicalNote }\n\n/// What a patch may change.\ntype AmendedNotes = Input {\n    notes: ClinicalNote {\n        optional: true\n    }\n    summary: ClinicalNote {\n        optional: true\n    }\n}\n".to_string(),
        ),
        (
            "domains/clinic/failures/not_for_this_device.jadpo",
            "/// The bytes aren't sealed for this device.\ntype NotForThisDevice = Failure {\n    kind: FailureKind.invalid_value\n}\n".to_string(),
        ),
        (
            "domains/clinic/entities/consultation.jadpo",
            format!(
                "use clinic/types {{ ClinicalNote }}

/// A consultation, whose notes are kept sealed.
entity Consultation {{
    id: Uuid
    notes: ClinicalNote {{
        encrypted: true
    }}
    summary: ClinicalNote? {{
        encrypted: true
    }}
    fee: Money? {{
        encrypted: true
    }}
    tags: List<ClinicalNote> {{
        encrypted: true
    }}
    seen_at: Instant {{
        generated: Generated.on_create
    }}
    changed_at: Instant {{
        generated: Generated.on_change
    }}

{policy}}}
"
            ),
        ),
        (
            "domains/clinic/entities/device.jadpo",
            format!(
                "/// A clinician's device, with a key of its own.
entity Device {{
    id: Uuid
    device_key: EncryptionKey
    backup_key: EncryptionKey?
    signing_key: PrivateKey
    authenticator: TotpSecret?
    recovery: SecretText?

{policy}}}
"
            ),
        ),
        (
            "domains/clinic/entities/referral.jadpo",
            format!(
                "use clinic/types {{ ClinicalNote }}

/// A referral, whose history says when its letter changed and never what it said.
entity Referral {{
    id: Uuid
    letter: ClinicalNote {{
        encrypted: true
    }}
    subject: ClinicalNote

    audit: {{}}

{policy}}}
"
            ),
        ),
        (
            "domains/clinic/entities/visit.jadpo",
            format!(
                "use clinic/entities {{ Consultation }}
use clinic/types {{ ClinicalNote }}

/// A visit during a consultation, whose remark is kept sealed too.
entity Visit {{
    id: Uuid
    consultation_id: Consultation.id {{
        on_delete: OnDelete.cascade
    }}
    remark: ClinicalNote {{
        encrypted: true
    }}

{policy}}}
"
            ),
        ),
        (
            "domains/clinic/types/visit_shown.jadpo",
            "use clinic/types { ClinicalNote }\n\n/// A visit's remark, and no more of it.\ntype VisitShown = Output {\n    remark: ClinicalNote\n}\n".to_string(),
        ),
        (
            "domains/clinic/types/visits_shown.jadpo",
            "use clinic/types { ClinicalNote, VisitShown }\n\n/// A consultation's notes with what its visits say.\ntype VisitsShown = Output {\n    notes: ClinicalNote\n    visits: List<VisitShown>\n}\n".to_string(),
        ),
        (
            "domains/clinic/queries/visits_of.jadpo",
            "use clinic/entities { Consultation, ConsultationNotFound, Visit }\nuse clinic/types { VisitsShown }\n\n/// A consultation with its visits, both sealed.\nquery visits_of(consultation_id: Consultation.id) fails ConsultationNotFound -> VisitsShown {\n    from: Consultation\n    where: Consultation.id == consultation_id\n    missing: ConsultationNotFound\n    include: {\n        visits: {\n            via: Visit.consultation_id\n            order: [Visit.id asc]\n            limit: 10\n        }\n    }\n}\n".to_string(),
        ),
        (
            "domains/clinic/actions/record_visit.jadpo",
            "use clinic/entities { Consultation, Visit }\n\n/// Records a visit.\naction record_visit(consultation: Consultation, remark: Visit.remark) -> Visit {\n    return Visit.create {\n        set: {\n            consultation_id: consultation.id\n            remark: remark\n        }\n    }\n}\n".to_string(),
        ),
        (
            "domains/clinic/actions/show_visits.jadpo",
            "use clinic/entities { Consultation, ConsultationNotFound }\nuse clinic/queries { visits_of }\nuse clinic/types { VisitsShown }\n\n/// A consultation with its visits.\naction show_visits(consultation_id: Consultation.id) fails ConsultationNotFound -> VisitsShown {\n    return attempt visits_of(consultation_id)\n}\n".to_string(),
        ),
        (
            "domains/clinic/queries/notes_of.jadpo",
            "use clinic/entities { Consultation, ConsultationNotFound }\nuse clinic/types { NotesShown }\n\n/// One consultation's notes, reading only them.\nquery notes_of(consultation_id: Consultation.id) fails ConsultationNotFound -> NotesShown {\n    from: Consultation\n    where: Consultation.id == consultation_id\n    missing: ConsultationNotFound\n}\n".to_string(),
        ),
        (
            "domains/clinic/actions/consult.jadpo",
            "use clinic/entities { Consultation }\n\n/// Records a consultation.\naction consult(notes: Consultation.notes, summary: Consultation.summary, fee: Consultation.fee, tags: Consultation.tags) -> Consultation {\n    return Consultation.create {\n        set: {\n            notes: notes\n            summary: summary\n            fee: fee\n            tags: tags\n        }\n    }\n}\n".to_string(),
        ),
        (
            "domains/clinic/actions/book.jadpo",
            "use clinic/entities { Consultation }\nuse clinic/types { BookingNotes, NotesShown }\n\n/// Books a consultation with what a request gives.\naction book(input: BookingNotes) -> NotesShown {\n    var made: Consultation = Consultation.create {\n        set: {\n            notes: input.notes\n            summary: none\n            fee: none\n            tags: []\n        }\n    }\n    return NotesShown {\n        notes: made.notes\n    }\n}\n".to_string(),
        ),
        (
            "domains/clinic/actions/amend.jadpo",
            "use clinic/entities { Consultation }\n\n/// Changes a loaded consultation's notes.\naction amend(consultation: Consultation, notes: Consultation.notes) -> Consultation {\n    return Consultation.update(consultation) {\n        set: {\n            notes: notes\n        }\n    }\n}\n".to_string(),
        ),
        (
            "domains/clinic/actions/amend_by_id.jadpo",
            "use clinic/entities { Consultation, ConsultationNotFound }\n\n/// Changes a consultation's notes by its `id`.\naction amend_by_id(consultation_id: Consultation.id, notes: Consultation.notes) fails ConsultationNotFound -> Consultation {\n    return attempt Consultation.update(consultation_id) {\n        set: {\n            notes: notes\n        }\n    }\n}\n".to_string(),
        ),
        (
            "domains/clinic/actions/patch.jadpo",
            "use clinic/entities { Consultation, ConsultationNotFound }\nuse clinic/types { AmendedNotes }\n\n/// Changes what a request supplies.\naction patch(consultation_id: Consultation.id, input: AmendedNotes) fails ConsultationNotFound -> Consultation {\n    return attempt Consultation.update(consultation_id) {\n        patch: input\n    }\n}\n".to_string(),
        ),
        (
            "domains/clinic/actions/shown.jadpo",
            "use clinic/entities { Consultation, ConsultationNotFound }\nuse clinic/queries { notes_of }\nuse clinic/types { NotesShown }\n\n/// One consultation's notes, or its not-found failure.\naction shown(consultation_id: Consultation.id) fails ConsultationNotFound -> NotesShown {\n    return attempt notes_of(consultation_id)\n}\n".to_string(),
        ),
        (
            "domains/clinic/actions/enrol.jadpo",
            "use clinic/entities { Device }\n\n/// Enrols a device, making its keys, which are stored sealed.\naction enrol(authenticator: Device.authenticator, recovery: Device.recovery) -> Device {\n    var device_key: EncryptionKey = crypto.new_key(as: EncryptionKey)\n    return Device.create {\n        set: {\n            device_key: device_key\n            backup_key: none\n            signing_key: crypto.new_key(as: PrivateKey)\n            authenticator: authenticator\n            recovery: recovery\n        }\n    }\n}\n".to_string(),
        ),
        (
            "domains/clinic/actions/replace_key.jadpo",
            "use clinic/entities { Device }\n\n/// Gives a device a new key, keeping the old one to read what it sealed.\naction replace_key(device: Device) -> Device {\n    return Device.update(device) {\n        set: {\n            backup_key: device.device_key\n            device_key: crypto.new_key(as: EncryptionKey)\n        }\n    }\n}\n".to_string(),
        ),
        (
            "domains/clinic/actions/seal_for.jadpo",
            "use clinic/entities { Device }\nuse clinic/types { ClinicalNote, SealedForDevice }\n\n/// A note sealed with a device's own key.\naction seal_for(device: Device, note: ClinicalNote) -> SealedForDevice {\n    return SealedForDevice(crypto.encrypt(note, key: device.device_key, context: \"note\"))\n}\n".to_string(),
        ),
        (
            "domains/clinic/actions/seal_once.jadpo",
            "use clinic/types { ClinicalNote, SealedForDevice }\n\n/// A note sealed with a key that goes with this call.\naction seal_once(note: ClinicalNote) -> SealedForDevice {\n    var made: EncryptionKey = crypto.new_key(as: EncryptionKey)\n    return SealedForDevice(crypto.encrypt(note, key: made, context: \"note\"))\n}\n".to_string(),
        ),
        (
            "domains/clinic/actions/open_for.jadpo",
            "use clinic/entities { Device }\nuse clinic/failures { NotForThisDevice }\nuse clinic/types { OpenedForDevice, SealedForDevice }\n\n/// What a device's own key opens.\naction open_for(device: Device, sealed: SealedForDevice) fails NotForThisDevice -> OpenedForDevice {\n    var opened: Bytes = attempt crypto.decrypt(sealed, key: device.device_key, context: \"note\") {\n        unreadable: NotForThisDevice\n    }\n    return OpenedForDevice(opened)\n}\n".to_string(),
        ),
        (
            "domains/clinic/actions/signed_by.jadpo",
            "use clinic/entities { Device }\nuse clinic/types { ClinicalNote, SealedForDevice }\n\n/// A note signed with a device's own key.\naction signed_by(device: Device, note: ClinicalNote) -> SealedForDevice {\n    return SealedForDevice(crypto.sign(note, key: device.signing_key))\n}\n".to_string(),
        ),
        (
            "domains/clinic/actions/refer.jadpo",
            "use clinic/entities { Referral }\n\n/// Makes a referral.\naction refer(letter: Referral.letter, subject: Referral.subject) -> Referral {\n    return Referral.create {\n        set: {\n            letter: letter\n            subject: subject\n        }\n    }\n}\n".to_string(),
        ),
        (
            "domains/clinic/actions/rewrite.jadpo",
            "use clinic/entities { Referral }\n\n/// Rewrites a referral's letter.\naction rewrite(referral: Referral, letter: Referral.letter) -> Referral {\n    return Referral.update(referral) {\n        set: {\n            letter: letter\n        }\n    }\n}\n".to_string(),
        ),
        (
            "domains/clinic/routes/consultations.jadpo",
            "use clinic/actions { book }\nuse clinic/entities { Consultation }\nuse clinic/queries { notes_of }\nuse clinic/types { BookingNotes, NotesShown }\n\n/// What the clinic's front desk books and reads.\nroutes Consultations {\n    prefix: /consultations\n\n    POST / {\n        input: BookingNotes\n        output: NotesShown\n        status: SuccessStatus.created\n        authentication: RouteAuthentication.none\n        run: book(input)\n    }\n\n    GET /{consultation_id: Consultation.id} {\n        output: NotesShown\n        authentication: RouteAuthentication.none\n        run: notes_of(consultation_id)\n    }\n}\n".to_string(),
        ),
    ];
    files
        .into_iter()
        .map(|(path, text)| (path.to_string(), text))
        .collect()
}

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

/// The clinic in `root`, which a deployable's test keeps between runs for
/// its build: its sources and migration are written again, so none is left
/// from an earlier form of it.
pub(crate) fn project_at(root: &Path, application: &str) {
    for folder in ["database", "domains"] {
        let _ = fs::remove_dir_all(root.join(folder));
    }
    for (path, text) in files(application) {
        write(root, &path, &text);
    }
    for args in [
        &["migrate", "new", "initial"][..],
        &["records", "update"][..],
    ] {
        let output = jadpo(root, args);
        assert_eq!(output.code, 0, "{}{}", output.stdout, output.stderr);
    }
}

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("sealed-{name}"));
    let _ = fs::remove_dir_all(&root);
    project_at(&root, "");
    root
}

/// `program` as a deployment starts it with these keys.
fn started(program: Program, current: &str, previous: Option<&str>) -> Program {
    let mut variables = BTreeMap::new();
    variables.insert(
        "DATABASE_URL".to_string(),
        "postgres://localhost/clinic".to_string(),
    );
    variables.insert("FIELD_ENCRYPTION_KEY".to_string(), current.to_string());
    if let Some(previous) = previous {
        variables.insert(
            "FIELD_ENCRYPTION_KEY_PREVIOUS".to_string(),
            previous.to_string(),
        );
    }
    let values = program
        .prepared()
        .config(&variables, Mode::Environment, &|_: &mut [u8]| false)
        .unwrap_or_else(|problems| panic!("doesn't start: {problems:?}"));
    program.with_config(&values)
}

/// The project, loaded and started with `current` and `previous`.
fn program(root: &Path, current: &str, previous: Option<&str>) -> Program {
    let loaded = jadpo_db::load(root);
    started(
        Program::load(&loaded.db, loaded.project).expect("the project checks"),
        current,
        previous,
    )
}

/// A test database of `root`'s migrations.
fn database(root: &Path) -> (jadpo_test::database::TestDatabases, String) {
    let loaded = jadpo_db::load(root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    std::mem::forget(copy);
    (databases, url)
}

/// 2026-10-19T09:00:00Z.
const T0: i64 = 1_792_400_400_000;
const HOUR: i64 = 3_600_000;

async fn outcome(program: &Program, host: &Harness, action: &str, args: Vec<Value>) -> Outcome {
    let id = program.plan().find(action).expect("the action");
    let mut ctx = host.ctx(program);
    program.run(&mut ctx, id, args).await
}

async fn call(program: &Program, host: &Harness, action: &str, args: Vec<Value>) -> Value {
    match outcome(program, host, action, args).await {
        Outcome::Value(value) => value,
        other => panic!("{action}: expected a value, got {other:?}"),
    }
}

fn field(record: &Value, name: &str) -> Value {
    match record {
        Value::Record(record) => record.get(name).cloned().expect("the field"),
        other => panic!("not a record: {other:?}"),
    }
}

fn text(value: &str) -> Value {
    Value::text(value)
}

/// The bytes in each row's first column, in the row's `id` order.
async fn bytes_of(host: &Harness, table: &str, column: &str) -> Vec<Option<Vec<u8>>> {
    host.rows(&format!(
        "SELECT \"{column}\" FROM \"{table}\" ORDER BY \"id\""
    ))
    .await
    .into_iter()
    .map(|row| match &row[0] {
        SqlValue::Bytes(bytes) => Some(bytes.clone()),
        SqlValue::Null(_) => None,
        other => panic!("`{table}.{column}` isn't bytes: {other:?}"),
    })
    .collect()
}

/// Whether `bytes` are the sealed format, version 1 of XChaCha20-Poly1305,
/// and hold none of `plain`.
fn only_sealed(bytes: &[u8], plain: &[u8]) {
    assert!(bytes.len() >= 46, "too short for the sealed format");
    assert_eq!(
        &bytes[..2],
        &[1, 1],
        "the sealed format's version and algorithm"
    );
    assert!(
        !bytes.windows(plain.len()).any(|window| window == plain),
        "the column holds the value"
    );
}

fn money(text: &str) -> Value {
    jadpo_value::standard::builtin_text(jadpo_std::Builtin::Money, text).expect("an amount")
}

fn consultation_args(notes: &str, summary: Option<&str>) -> Vec<Value> {
    vec![
        text(notes),
        summary.map_or(Value::None, text),
        money("GBP 12.50"),
        Value::list(vec![text("urgent"), text("cardiology")]),
    ]
}

#[test]
fn a_sealed_field_round_trips_and_its_column_holds_only_sealed_bytes() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("round-trip");
    let program = program(&root, FIRST_KEY, None);
    let (_databases, url) = database(&root);
    let host = Harness::postgres(&url).at(T0).random_from(0);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let made = call(
            &program,
            &host,
            "consult",
            consultation_args("Allergic to penicillin.", Some("Allergy")),
        )
        .await;
        assert_eq!(field(&made, "notes"), text("Allergic to penicillin."));
        assert_eq!(field(&made, "summary"), text("Allergy"));
        assert_eq!(
            field(&made, "tags"),
            Value::list(vec![text("urgent"), text("cardiology")])
        );
        let id = field(&made, "id");

        // Its columns hold only sealed bytes: one `bytea` a field, a compound
        // type's included, whose text says nothing.
        let notes = bytes_of(&host, "consultation", "notes").await;
        let sealed = notes[0].clone().expect("a value");
        only_sealed(&sealed, b"penicillin");
        let summary = bytes_of(&host, "consultation", "summary").await;
        only_sealed(summary[0].as_deref().expect("a value"), b"Allergy");
        let fee = bytes_of(&host, "consultation", "fee").await;
        only_sealed(fee[0].as_deref().expect("a value"), b"1250");
        let tags = bytes_of(&host, "consultation", "tags").await;
        only_sealed(tags[0].as_deref().expect("a value"), b"cardiology");
        let types = host
            .rows("SELECT \"data_type\" FROM \"information_schema\".\"columns\" WHERE \"table_name\" = 'consultation' AND \"column_name\" LIKE 'fee%'")
            .await;
        assert_eq!(types.len(), 1, "one column, not `Money`'s two: {types:?}");

        // One value sealed twice gives different bytes.
        call(
            &program,
            &host,
            "consult",
            consultation_args("Allergic to penicillin.", None),
        )
        .await;
        let notes = bytes_of(&host, "consultation", "notes").await;
        let (first, second) = (notes[0].as_ref().unwrap(), notes[1].as_ref().unwrap());
        assert_ne!(first, second);
        assert_eq!(
            bytes_of(&host, "consultation", "summary").await[1],
            None,
            "`none` is the column's `NULL`"
        );

        // Read back through a query that reads only the field: its record's
        // `id` comes with it, to open it for.
        let shown = call(&program, &host, "shown", vec![id.clone()]).await;
        assert_eq!(field(&shown, "notes"), text("Allergic to penicillin."));

        // And through an include, whose records' `id`s come with them.
        call(
            &program,
            &host,
            "record_visit",
            vec![made.clone(), text("Came back on Tuesday.")],
        )
        .await;
        let with_visits = call(&program, &host, "show_visits", vec![id.clone()]).await;
        assert_eq!(field(&with_visits, "notes"), text("Allergic to penicillin."));
        let Value::List(visits) = field(&with_visits, "visits") else {
            panic!("a list of visits");
        };
        assert_eq!(field(&visits[0], "remark"), text("Came back on Tuesday."));
        only_sealed(
            bytes_of(&host, "visit", "remark").await[0].as_deref().unwrap(),
            b"Tuesday",
        );

        // Changed on a loaded record, by `id` and by a patch.
        let changed = call(
            &program,
            &host,
            "amend",
            vec![made.clone(), text("Allergic to aspirin.")],
        )
        .await;
        assert_eq!(field(&changed, "notes"), text("Allergic to aspirin."));
        let changed = call(
            &program,
            &host,
            "amend_by_id",
            vec![id.clone(), text("Allergic to ibuprofen.")],
        )
        .await;
        assert_eq!(field(&changed, "notes"), text("Allergic to ibuprofen."));
        assert_eq!(field(&changed, "summary"), text("Allergy"));
        let input = Value::record(
            "AmendedNotes",
            vec![(std::rc::Rc::from("summary"), text("No allergy"))],
        );
        let changed = call(&program, &host, "patch", vec![id.clone(), input]).await;
        assert_eq!(field(&changed, "notes"), text("Allergic to ibuprofen."), "left out");
        assert_eq!(field(&changed, "summary"), text("No allergy"));
        assert_ne!(
            bytes_of(&host, "consultation", "notes").await[0].as_ref(),
            Some(&sealed)
        );
    });
}

#[test]
fn a_value_moved_to_another_row_or_altered_opens_for_no_one() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("moved");
    let program = program(&root, FIRST_KEY, None);
    let (_databases, url) = database(&root);
    let host = Harness::postgres(&url).at(T0).random_from(0);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let first = call(&program, &host, "consult", consultation_args("Ada's notes", None)).await;
        let second = call(&program, &host, "consult", consultation_args("Bob's notes", None)).await;
        let unopened = |outcome: Outcome| {
            let Outcome::Fault(fault) = outcome else {
                panic!("expected a fault, got {outcome:?}");
            };
            let shown = format!("{fault:?}");
            assert!(shown.contains("clinic") || shown.contains("consultation.notes"), "{shown}");
            assert!(shown.contains("no key"), "{shown}");
            assert!(!shown.contains("notes\" "), "{shown}");
        };
        // Another record's value, moved here with the database's own rights.
        host.rows(&format!(
            "UPDATE \"consultation\" SET \"notes\" = (SELECT \"notes\" FROM \"consultation\" WHERE \"id\" = '{}') WHERE \"id\" = '{}' RETURNING \"id\"",
            uuid(&field(&first, "id")),
            uuid(&field(&second, "id")),
        ))
        .await;
        unopened(outcome(&program, &host, "shown", vec![field(&second, "id")]).await);
        // Another field's value of the same record.
        host.rows(&format!(
            "UPDATE \"consultation\" SET \"notes\" = \"tags\" WHERE \"id\" = '{}' RETURNING \"id\"",
            uuid(&field(&first, "id")),
        ))
        .await;
        unopened(outcome(&program, &host, "shown", vec![field(&first, "id")]).await);
        // One byte altered.
        host.rows(&format!(
            "UPDATE \"consultation\" SET \"notes\" = set_byte(\"tags\", 40, get_byte(\"tags\", 40) # 1) WHERE \"id\" = '{}' RETURNING \"id\"",
            uuid(&field(&first, "id")),
        ))
        .await;
        unopened(outcome(&program, &host, "shown", vec![field(&first, "id")]).await);
    });
}

fn uuid(value: &Value) -> String {
    match value {
        Value::Uuid(text) => text.to_string(),
        other => panic!("not an id: {other:?}"),
    }
}

#[test]
fn a_secret_and_a_key_are_stored_sealed_and_a_key_is_usable_only_as_a_key() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("keys");
    let program = program(&root, FIRST_KEY, None);
    let (_databases, url) = database(&root);
    let host = Harness::postgres(&url).at(T0).random_from(0);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let authenticator = Value::secret(
            jadpo_std::standard::security::TOTP_SECRET,
            "GEZDGNBVGY3TQOJQ",
        );
        let recovery = Value::secret(
            jadpo_std::standard::security::SECRET_TEXT,
            "the blue bicycle",
        );
        let device = call(
            &program,
            &host,
            "enrol",
            vec![authenticator.clone(), recovery.clone()],
        )
        .await;
        assert_eq!(field(&device, "authenticator"), authenticator);
        assert_eq!(field(&device, "recovery"), recovery);
        assert_eq!(format!("{}", field(&device, "device_key")), "<key>");

        // Each column holds only sealed bytes, a key's 32 among them.
        only_sealed(
            bytes_of(&host, "device", "authenticator").await[0]
                .as_deref()
                .unwrap(),
            b"GEZDGNBVGY3TQOJQ",
        );
        only_sealed(
            bytes_of(&host, "device", "recovery").await[0]
                .as_deref()
                .unwrap(),
            b"the blue bicycle",
        );
        let kept = bytes_of(&host, "device", "device_key").await[0]
            .clone()
            .unwrap();
        assert_eq!(kept.len(), 46 + 32, "the sealed format of 32 bytes");
        assert_eq!(&kept[..2], &[1, 1]);

        // Read back from the store, the key opens what it sealed, and
        // another device's doesn't.
        let sealed = call(
            &program,
            &host,
            "seal_for",
            vec![device.clone(), text("for this device")],
        )
        .await;
        let opened = call(
            &program,
            &host,
            "open_for",
            vec![device.clone(), sealed.clone()],
        )
        .await;
        assert_eq!(
            opened,
            Value::Bytes(std::rc::Rc::from(&b"for this device"[..]))
        );
        let other = call(&program, &host, "enrol", vec![Value::None, Value::None]).await;
        assert!(matches!(
            outcome(&program, &host, "open_for", vec![other, sealed]).await,
            Outcome::Failed(_)
        ));
        // A key made for one call does its job and goes with the call.
        call(&program, &host, "seal_once", vec![text("once")]).await;
        // A private key held as data signs: pure Ed25519's 64 bytes.
        let signed = call(
            &program,
            &host,
            "signed_by",
            vec![device.clone(), text("note")],
        )
        .await;
        assert!(matches!(&signed, Value::Bytes(bytes) if bytes.len() == 64));

        // A key is replaced by storing another, the old one kept sealed.
        let replaced = call(&program, &host, "replace_key", vec![device.clone()]).await;
        let kept_now = bytes_of(&host, "device", "device_key").await;
        assert!(!kept_now.contains(&Some(kept.clone())));
        assert_eq!(format!("{}", field(&replaced, "backup_key")), "<key>");
        let again = call(&program, &host, "seal_for", vec![replaced, text("x")]).await;
        assert!(matches!(again, Value::Bytes(_)));
    });
}

#[test]
fn a_recording_keeps_no_key() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("recording");
    let program = program(&root, FIRST_KEY, None);
    let (_databases, url) = database(&root);
    let host = Harness::postgres(&url).at(T0).random_from(0);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let id = program.plan().find("seal_once").expect("the action");
        let mut ctx = host.ctx(&program);
        ctx.record_run();
        let ran = program.run(&mut ctx, id, vec![text("once")]).await;
        assert!(matches!(ran, Outcome::Value(_)), "{ran:?}");
        let tape = ctx.recorded_run().expect("recording");
        assert!(
            tape.random.is_empty(),
            "a key's bytes and a nonce are secret randomness: {:?} bytes kept",
            tape.random.len()
        );
    });
}

/// The rotation, ticked by the interpreter or, when `compiled_too`, by the
/// compiled executor from the plan's canonical bytes.
fn rotation(name: &str, compiled_too: bool) {
    let root = project(name);
    let (_databases, url) = database(&root);
    let host = Harness::postgres(&url).at(T0).random_from(0);
    let runtime = Runtime::new().expect("runtime");
    let before = program(&root, FIRST_KEY, None);
    let during = program(&root, SECOND_KEY, Some(FIRST_KEY));
    let after = program(&root, SECOND_KEY, None);
    let made = compiled_too.then(|| compiled(&during));
    let ticking: &dyn Executor = match &made {
        Some(made) => made,
        None => &during,
    };
    let host = &host;
    let standing = |program: &Program| {
        let started = program.started().clone();
        let ring = started.field_ring().expect("the key").clone();
        let plans = program.plan().runtime().reseals.clone();
        async move {
            jadpo_runtime::sealed::rotation(host, &plans, &ring)
                .await
                .expect("counted")
                .into_iter()
                .find(|field| field.field == "Consultation.notes")
                .expect("Consultation.notes")
        }
    };
    runtime.block_on(async {
        let first = call(&before, host, "consult", consultation_args("Written first", None)).await;
        let device = call(&before, host, "enrol", vec![Value::None, Value::None]).await;
        let sealed = call(&before, host, "seal_for", vec![device.clone(), text("x")]).await;
        let original = bytes_of(host, "consultation", "notes").await[0].clone().unwrap();

        // With the new key set and the old one moved to `_PREVIOUS`, what it
        // sealed still reads, and what is written now goes under the new.
        let shown = call(&during, host, "shown", vec![field(&first, "id")]).await;
        assert_eq!(field(&shown, "notes"), text("Written first"));
        call(&during, host, "consult", consultation_args("Written second", None)).await;
        let count = standing(&during).await;
        assert_eq!(
            (count.rows, count.current, count.previous, count.unreadable),
            (2, 1, 1, 0),
            "{count:?}"
        );

        // The next hourly run moves every row, changing nothing else: a
        // field generated on every change stands as it was.
        let changed_at = host
            .rows("SELECT \"changed_at\"::text FROM \"consultation\" ORDER BY \"id\"")
            .await;
        tick(ticking, host, T0).await;
        let ticked = tick(ticking, host, T0 + 2 * HOUR).await;
        let ran = ticked
            .due
            .iter()
            .find(|due| due.schedule == "schedule field_encryption.reseal")
            .expect("the re-seal ran");
        assert!(ran.items.is_some_and(|items| items >= 1), "{ran:?}");
        let count = standing(&during).await;
        assert_eq!(
            (count.rows, count.current, count.previous, count.unreadable),
            (2, 2, 0, 0),
            "{count:?}"
        );
        let moved = bytes_of(host, "consultation", "notes").await;
        assert_ne!(moved[0].as_ref(), Some(&original));
        only_sealed(moved[0].as_deref().unwrap(), b"Written first");
        assert_eq!(
            host.rows("SELECT \"changed_at\"::text FROM \"consultation\" ORDER BY \"id\"").await,
            changed_at
        );
        // A run with nothing left moves nothing.
        let ticked = tick(ticking, host, T0 + 4 * HOUR).await;
        let ran = ticked
            .due
            .iter()
            .find(|due| due.schedule == "schedule field_encryption.reseal")
            .expect("the re-seal ran");
        assert_eq!(ran.items, Some(0), "{ran:?}");

        // So `_PREVIOUS` can go: everything reads under the new key alone,
        // the device's own key among it.
        let shown = call(&after, host, "shown", vec![field(&first, "id")]).await;
        assert_eq!(field(&shown, "notes"), text("Written first"));
        let opened = call(&after, host, "open_for", vec![device, sealed]).await;
        assert_eq!(opened, Value::Bytes(std::rc::Rc::from(&b"x"[..])));

        // A row still under the old key, with `_PREVIOUS` gone, is what the
        // pages warn of: it opens for no one, and is counted.
        let (table, column, id) = ("consultation", "notes", uuid(&field(&first, "id")));
        host.rows(&format!(
            "UPDATE \"{table}\" SET \"{column}\" = set_byte(\"{column}\", 2, get_byte(\"{column}\", 2) # 255) WHERE \"id\" = '{id}' RETURNING \"id\""
        ))
        .await;
        assert!(matches!(
            outcome(&after, host, "shown", vec![field(&first, "id")]).await,
            Outcome::Fault(_)
        ));
        let count = standing(&after).await;
        assert_eq!((count.rows, count.unreadable), (2, 1), "{count:?}");
    });
}

/// The compiled executor on `program`'s plan, as generated code holds it,
/// started as `program` is. The re-seal calls no callable, so none is given.
fn compiled(program: &Program) -> jadpo_runtime::compiled::Program {
    use jadpo_runtime::compiled::{Code, Shared};
    let bytes = program
        .plan()
        .runtime()
        .encode_deployable()
        .expect("the plan's bytes");
    let code: &'static Code = Box::leak(Box::new(Code {
        plan: Box::leak(bytes.into_boxed_slice()),
        places: &[],
        callable: |_| None,
        row: |_, _| None,
        routes: |_| None,
        shared: Box::leak(Box::new(Shared::new())),
    }));
    let mut variables = BTreeMap::new();
    variables.insert(
        "DATABASE_URL".to_string(),
        "postgres://localhost/clinic".to_string(),
    );
    variables.insert("FIELD_ENCRYPTION_KEY".to_string(), SECOND_KEY.to_string());
    variables.insert(
        "FIELD_ENCRYPTION_KEY_PREVIOUS".to_string(),
        FIRST_KEY.to_string(),
    );
    let values = program
        .prepared()
        .config(&variables, Mode::Environment, &|_: &mut [u8]| false)
        .expect("it starts");
    jadpo_runtime::compiled::Program::new(code)
        .expect("the plan decodes")
        .with_config(&values)
}

#[test]
fn a_rotation_on_the_interpreter() {
    if !jadpo_test::live() {
        return;
    }
    rotation("rotation-interpreted", false);
}

#[test]
fn a_rotation_compiled() {
    if !jadpo_test::live() {
        return;
    }
    rotation("rotation-compiled", true);
}

#[test]
fn a_history_says_a_sealed_field_changed_and_never_what_it_held() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("audit");
    let program = program(&root, FIRST_KEY, None);
    let (_databases, url) = database(&root);
    let host = Harness::postgres(&url).at(T0).random_from(0);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let made = call(
            &program,
            &host,
            "refer",
            vec![text("Dear colleague"), text("Referral")],
        )
        .await;
        call(&program, &host, "rewrite", vec![made, text("Dear doctor")]).await;
        let kept = host
            .rows("SELECT \"changes\"::text FROM \"jadpo_audit\" ORDER BY \"seq\"")
            .await;
        assert_eq!(kept.len(), 2);
        let text_of = |row: &Vec<SqlValue>| match &row[0] {
            SqlValue::Text(text) => text.clone(),
            other => panic!("not text: {other:?}"),
        };
        let created = text_of(&kept[0]);
        assert!(created.contains("\"letter\""), "{created}");
        assert!(created.contains("Referral"), "the plain field: {created}");
        assert!(!created.contains("colleague"), "{created}");
        let updated = text_of(&kept[1]);
        assert!(updated.contains("\"letter\""), "{updated}");
        assert!(
            !updated.contains("colleague") && !updated.contains("doctor"),
            "{updated}"
        );
    });
}

#[test]
fn a_migration_never_seals_or_opens_a_value() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("sealed-migrations");
    let _ = fs::remove_dir_all(&root);
    let entity = |fields: &str| format!("entity Note {{\n    id: Uuid\n{fields}{POLICY}}}\n");
    write(
        &root,
        "app.jadpo",
        "application {\n    field_encryption_key: config.field_encryption_key\n}\n",
    );
    write(
        &root,
        "config.jadpo",
        "config {\n    field_encryption_key: EncryptionKey\n}\n",
    );
    write(
        &root,
        "domains/notes/types/body.jadpo",
        "/// A note's text.\ntype Body = Text {\n    max_length: 200\n}\n",
    );
    write(
        &root,
        "domains/notes/types/blob.jadpo",
        "/// A note's bytes.\ntype Blob = Bytes {}\n",
    );
    let path = "domains/notes/entities/note.jadpo";
    let head = "use notes/types { Blob, Body }\n\n";
    let fields = |rest: &str| format!("{head}{}", entity(rest));
    write(
        &root,
        path,
        &fields(
            "    plain: Body\n    bytes: Blob\n    sealed: Body {\n        encrypted: true\n    }\n",
        ),
    );
    let first = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(first.code, 0, "{}{}", first.stdout, first.stderr);
    let refused = |edited: String, says: &str| {
        write(&root, path, &edited);
        let output = jadpo(&root, &["migrate", "new", "change"]);
        assert_ne!(output.code, 0, "{}", output.stdout);
        let shown = format!("{}{}", output.stdout, output.stderr);
        assert!(shown.contains(says), "{shown}");
    };
    // Turned on, a type's column changing or not, and off.
    refused(
        fields(
            "    plain: Body {\n        encrypted: true\n    }\n    bytes: Blob\n    sealed: Body {\n        encrypted: true\n    }\n",
        ),
        "turns `encrypted:` on",
    );
    refused(
        fields(
            "    plain: Body\n    bytes: Blob {\n        encrypted: true\n    }\n    sealed: Body {\n        encrypted: true\n    }\n",
        ),
        "turns `encrypted:` on",
    );
    refused(
        fields("    plain: Body\n    bytes: Blob\n    sealed: Body\n"),
        "turns `encrypted:` off",
    );
    // A required sealed field on rows already there.
    refused(
        fields(
            "    plain: Body\n    bytes: Blob\n    sealed: Body {\n        encrypted: true\n    }\n    added: Body {\n        encrypted: true\n    }\n",
        ),
        "required sealed column",
    );
    // A narrower type that no SQL can check against what is stored.
    write(
        &root,
        "domains/notes/types/body.jadpo",
        "/// A note's text.\ntype Body = Text {\n    max_length: 100\n}\n",
    );
    let shown = jadpo(&root, &["migrate", "new", "change"]);
    assert_ne!(shown.code, 0);
    write(
        &root,
        "domains/notes/types/body.jadpo",
        "/// A note's text.\ntype Body = Text {\n    max_length: 200\n}\n",
    );
    refused(
        format!(
            "{head}{}",
            entity(
                "    plain: Body\n    bytes: Blob\n    sealed: Body {\n        encrypted: true\n        max_length: 100\n    }\n"
            )
        ),
        "allows less",
    );
    // An optional one is added, and then made required, leaving the migration
    // to refuse a record without a value.
    write(
        &root,
        path,
        &fields(
            "    plain: Body\n    bytes: Blob\n    sealed: Body {\n        encrypted: true\n    }\n    added: Body? {\n        encrypted: true\n    }\n",
        ),
    );
    let added = jadpo(&root, &["migrate", "new", "add_sealed"]);
    assert_eq!(added.code, 0, "{}{}", added.stdout, added.stderr);
    write(
        &root,
        path,
        &fields(
            "    plain: Body\n    bytes: Blob\n    sealed: Body {\n        encrypted: true\n    }\n    added: Body {\n        encrypted: true\n    }\n",
        ),
    );
    let required = jadpo(&root, &["migrate", "new", "require_sealed"]);
    assert_eq!(required.code, 0, "{}{}", required.stdout, required.stderr);
    let sql = fs::read_dir(root.join("database/migrations"))
        .expect("migrations")
        .filter_map(Result::ok)
        .find(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .ends_with("require_sealed")
        })
        .map(|entry| fs::read_to_string(entry.path().join("postgres.sql")).expect("its SQL"))
        .expect("the migration");
    assert!(sql.contains("SET NOT NULL"), "{sql}");
    // A value worked out in the database can't read one.
    write(
        &root,
        path,
        &fields(
            "    plain: Body\n    bytes: Blob\n    sealed: Body {\n        encrypted: true\n    }\n    added: Body {\n        encrypted: true\n    }\n    copied: Body\n",
        ),
    );
    let need = jadpo(&root, &["migrate", "new", "copy"]);
    assert_eq!(need.code, 0, "{}{}", need.stdout, need.stderr);
    let folder = fs::read_dir(root.join("database/migrations"))
        .expect("migrations")
        .filter_map(Result::ok)
        .find(|entry| entry.file_name().to_string_lossy().ends_with("_copy"))
        .expect("the migration");
    fs::write(
        folder.path().join("changes.jadpo"),
        "/// Copies what can't be copied.\nmigration {\n    Note.copied: {\n        backfill: Note.sealed\n    }\n}\n",
    )
    .expect("its decisions");
    let checked = jadpo(&root, &["check"]);
    let shown = format!("{}{}", checked.stdout, checked.stderr);
    assert!(shown.contains("data.encrypted_field_used"), "{shown}");
}

// --- the commands --------------------------------------------------------------

/// `jadpo` with the environment `pairs` and these terminals.
fn jadpo_at(cwd: &Path, args: &[&str], pairs: &[(&str, &str)], stdout_terminal: bool) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    for (name, value) in pairs {
        env = env.with_var(name, value);
    }
    run(
        &args,
        &env.with_terminals(false, stdout_terminal, false)
            .with_cwd(cwd),
    )
}

/// A small project keeping one note sealed, with its migration.
fn small(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("sealed-cli-{name}"));
    let _ = fs::remove_dir_all(&root);
    write(
        &root,
        "app.jadpo",
        "application {\n    field_encryption_key: config.field_encryption_key\n}\n",
    );
    write(
        &root,
        "config.jadpo",
        "use notes/types { GreetingText }\n\nconfig {\n    database_url: PostgresUrl\n    field_encryption_key: EncryptionKey\n    greeting: GreetingText {\n        default: \"hello\"\n    }\n}\n",
    );
    write(
        &root,
        "domains/notes/types/greeting_text.jadpo",
        "/// A greeting.\ntype GreetingText = Text {\n    max_length: 20\n}\n",
    );
    write(
        &root,
        "domains/notes/types/body.jadpo",
        "/// A note's text.\ntype Body = Text {\n    max_length: 200\n}\n",
    );
    write(
        &root,
        "domains/notes/entities/note.jadpo",
        &format!(
            "use notes/types {{ Body }}\n\n/// A note.\nentity Note {{\n    id: Uuid\n    body: Body {{\n        encrypted: true\n    }}\n\n{POLICY}}}\n"
        ),
    );
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    root
}

#[test]
fn set_generate_writes_a_key_into_the_local_file_and_prints_nothing() {
    let root = small("set-generate");
    let output = jadpo_at(
        &root,
        &["config", "set", "field_encryption_key", "--generate"],
        &[],
        false,
    );
    assert_eq!(output.code, 0, "{}{}", output.stdout, output.stderr);
    assert_eq!(output.stdout, "", "prints nothing");
    assert_eq!(output.stderr, "", "prints nothing");
    let local = fs::read_to_string(root.join(".env.local")).expect("written");
    let key = local
        .lines()
        .find_map(|line| line.strip_prefix("FIELD_ENCRYPTION_KEY="))
        .expect("the variable")
        .trim_matches('"');
    assert_eq!(key.len(), 43, "32 bytes as unpadded Base64url");
    assert!(jadpo_std::decode_key(key).is_some());
    // Nothing else holds it, `jadpo config check` says it is set, and it
    // stays out of git.
    let ignored = fs::read_to_string(root.join(".gitignore")).expect("kept out of git");
    assert!(ignored.contains(".env.local"));
    let checked = jadpo_at(&root, &["config", "check"], &[], false);
    assert!(!checked.stdout.contains(key), "{}", checked.stdout);
    assert!(
        checked.stdout.contains("field_encryption_key  set"),
        "{}",
        checked.stdout
    );

    // A second run would strand all the first one's key sealed.
    let again = jadpo_at(
        &root,
        &["config", "set", "field_encryption_key", "--generate"],
        &[],
        false,
    );
    assert_ne!(again.code, 0);
    assert!(
        again.stderr.contains("cli.config_generate_refused"),
        "{}{}",
        again.stdout,
        again.stderr
    );
    assert!(
        fs::read_to_string(root.join(".env.local"))
            .expect("kept")
            .contains(key),
        "the key stands"
    );
    // A value from the environment counts too.
    let elsewhere = small("set-generate-environment");
    let refused = jadpo_at(
        &elsewhere,
        &["config", "set", "field_encryption_key", "--generate"],
        &[("FIELD_ENCRYPTION_KEY", FIRST_KEY)],
        false,
    );
    assert!(
        refused.stderr.contains("cli.config_generate_refused"),
        "{}{}",
        refused.stdout,
        refused.stderr
    );
    assert!(!elsewhere.join(".env.local").exists());
}

#[test]
fn only_a_key_is_generated() {
    let root = small("only-keys");
    for args in [
        &["config", "set", "greeting", "--generate"][..],
        &["config", "generate", "greeting"][..],
    ] {
        let output = jadpo_at(&root, args, &[], false);
        assert_ne!(output.code, 0, "{args:?}");
        assert!(
            output.stderr.contains("cli.config_arguments"),
            "{args:?}: {}{}",
            output.stdout,
            output.stderr
        );
        assert!(!root.join(".env.local").exists());
    }
}

#[test]
fn generate_writes_a_key_into_a_pipe_and_refuses_a_terminal() {
    let root = small("generate");
    let piped = jadpo_at(
        &root,
        &["config", "generate", "field_encryption_key"],
        &[],
        false,
    );
    assert_eq!(piped.code, 0, "{}{}", piped.stdout, piped.stderr);
    assert_eq!(piped.stderr, "");
    let key = piped.stdout.strip_suffix('\n').expect("one line");
    assert_eq!(key.len(), 43);
    assert!(jadpo_std::decode_key(key).is_some(), "a key's own spelling");
    assert!(!root.join(".env.local").exists(), "nothing is kept");
    let second = jadpo_at(
        &root,
        &["config", "generate", "field_encryption_key"],
        &[],
        false,
    );
    assert_ne!(second.stdout, piped.stdout, "a new key each time");

    let terminal = jadpo_at(
        &root,
        &["config", "generate", "field_encryption_key"],
        &[],
        true,
    );
    assert_ne!(terminal.code, 0);
    assert!(
        terminal.stdout.contains("cli.config_generate_refused")
            || terminal.stderr.contains("cli.config_generate_refused"),
        "{}{}",
        terminal.stdout,
        terminal.stderr
    );
    assert_eq!(
        terminal
            .stdout
            .lines()
            .filter(|line| jadpo_std::decode_key(line.trim()).is_some())
            .count(),
        0,
        "no key on a terminal"
    );

    // Nor wrapped in a document.
    let json = jadpo_at(
        &root,
        &["config", "generate", "field_encryption_key", "--json"],
        &[],
        false,
    );
    assert_ne!(json.code, 0, "{}", json.stdout);
}

#[test]
fn check_says_how_to_make_a_key_and_dev_refuses_to_start_without_one() {
    let root = small("check-missing");
    let checked = jadpo_at(&root, &["config", "check"], &[], false);
    assert_ne!(checked.code, 0);
    assert!(
        checked
            .stdout
            .contains("jadpo config set field_encryption_key --generate"),
        "{}",
        checked.stdout
    );
    let loaded = jadpo_db::load(&root);
    let prepared = jadpo_interp::Prepared::load(&loaded.db, loaded.project).expect("it checks");
    let problems = prepared
        .config(
            &BTreeMap::from([(
                "DATABASE_URL".to_string(),
                "postgres://localhost/notes".to_string(),
            )]),
            Mode::Environment,
            &|_: &mut [u8]| false,
        )
        .expect_err("a required key stops it starting");
    assert!(
        problems
            .iter()
            .any(|problem| problem.variable == "FIELD_ENCRYPTION_KEY"),
        "{problems:?}"
    );
}

#[test]
fn rotation_says_when_the_previous_key_can_go() {
    if !jadpo_test::live() {
        return;
    }
    let root = small("rotation");
    write(
        &root,
        "domains/notes/actions/write_note.jadpo",
        "use notes/entities { Note }\n\n/// Writes a note.\naction write_note(body: Note.body) -> Note {\n    return Note.create {\n        set: {\n            body: body\n        }\n    }\n}\n",
    );
    let (_databases, url) = database(&root);
    let program_with = |current: &str, previous: Option<&str>| {
        let loaded = jadpo_db::load(&root);
        let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
        started(program, current, previous)
    };
    let host = Harness::postgres(&url).at(T0).random_from(0);
    let runtime = Runtime::new().expect("runtime");
    let first = program_with(FIRST_KEY, None);
    runtime.block_on(call(&first, &host, "write_note", vec![text("One")]));
    let rotation = |variables: &[(&str, &str)]| {
        jadpo_at(
            &root,
            &["config", "rotation", "--url", url.as_str(), "--json"],
            variables,
            false,
        )
    };
    let plain = |variables: &[(&str, &str)]| {
        jadpo_at(
            &root,
            &["config", "rotation", "--url", url.as_str()],
            variables,
            false,
        )
    };

    // No rotation: every row current, nothing to remove.
    let output = rotation(&[("FIELD_ENCRYPTION_KEY", FIRST_KEY)]);
    assert_eq!(output.code, 0, "{}{}", output.stdout, output.stderr);
    let found: serde_json::Value = serde_json::from_str(&output.stdout).expect("JSON");
    assert_eq!(found["data"]["fields"][0]["field"], "Note.body", "{found}");
    assert_eq!(found["data"]["fields"][0]["current"], 1, "{found}");
    assert_eq!(found["data"]["can_remove_previous"], false, "{found}");
    assert!(
        plain(&[("FIELD_ENCRYPTION_KEY", FIRST_KEY)])
            .stdout
            .contains("No rotation under way")
    );

    // A rotation begun: the row is under the previous key, so it can't go.
    let during = [
        ("FIELD_ENCRYPTION_KEY", SECOND_KEY),
        ("FIELD_ENCRYPTION_KEY_PREVIOUS", FIRST_KEY),
    ];
    let found: serde_json::Value = serde_json::from_str(&rotation(&during).stdout).expect("JSON");
    assert_eq!(found["data"]["fields"][0]["previous"], 1, "{found}");
    assert_eq!(found["data"]["can_remove_previous"], false, "{found}");
    let shown = plain(&during).stdout;
    assert!(shown.contains("Rotation under way"), "{shown}");
    assert!(
        shown.contains("1 row still under FIELD_ENCRYPTION_KEY_PREVIOUS"),
        "{shown}"
    );
    assert!(
        !shown.contains(SECOND_KEY) && !shown.contains(FIRST_KEY),
        "{shown}"
    );

    // The re-seal done, it can.
    let second = program_with(SECOND_KEY, Some(FIRST_KEY));
    let ring = second.started().field_ring().expect("the key").clone();
    let plans = second.plan().runtime().reseals.clone();
    let moved = runtime
        .block_on(jadpo_runtime::sealed::reseal_all(&host, &plans, &ring, 1))
        .expect("moved");
    assert_eq!((moved.rows, moved.finished), (1, true));
    let found: serde_json::Value = serde_json::from_str(&rotation(&during).stdout).expect("JSON");
    assert_eq!(found["data"]["can_remove_previous"], true, "{found}");
    assert!(
        plain(&during)
            .stdout
            .contains("FIELD_ENCRYPTION_KEY_PREVIOUS can go")
    );

    // A key that opens nothing: rows no key opens, and nothing can go.
    let third = "TM0Imyj_ltqdtsNG7BFOD1uKMZ81q6Yk2oz27U-4pvs";
    let wrong = [
        ("FIELD_ENCRYPTION_KEY", third),
        ("FIELD_ENCRYPTION_KEY_PREVIOUS", FIRST_KEY),
    ];
    let shown = plain(&wrong).stdout;
    assert!(shown.contains("Rows no key opens"), "{shown}");
    let found: serde_json::Value = serde_json::from_str(&rotation(&wrong).stdout).expect("JSON");
    assert_eq!(found["data"]["fields"][0]["neither"], 1, "{found}");
    assert_eq!(found["data"]["can_remove_previous"], false, "{found}");
}

#[test]
fn rotation_names_a_database_it_cant_read() {
    let root = small("rotation-unavailable");
    let output = jadpo_at(
        &root,
        &["config", "rotation"],
        &[
            ("DATABASE_URL", "postgres://nobody@127.0.0.1:1/notes"),
            ("FIELD_ENCRYPTION_KEY", FIRST_KEY),
        ],
        false,
    );
    assert_ne!(output.code, 0);
    assert!(
        output.stderr.contains("cli.config_rotation_unavailable"),
        "{}{}",
        output.stdout,
        output.stderr
    );
    // And a key it hasn't got.
    let output = jadpo_at(&root, &["config", "rotation"], &[], false);
    assert!(
        output.stderr.contains("configuration.value_missing"),
        "{}{}",
        output.stdout,
        output.stderr
    );
}

#[test]
fn a_key_field_that_may_be_none_is_read_once_checked() {
    let root = small("nullable-key");
    write(
        &root,
        "domains/notes/types/sealed_text.jadpo",
        "/// Bytes sealed for a device.\ntype SealedText = Bytes {}\n",
    );
    write(
        &root,
        "domains/notes/entities/device.jadpo",
        &format!(
            "/// A device.\nentity Device {{\n    id: Uuid\n    device_key: EncryptionKey?\n\n{POLICY}}}\n"
        ),
    );
    write(
        &root,
        "domains/notes/actions/seal_if_keyed.jadpo",
        "use notes/entities { Device }\nuse notes/types { Body, SealedText }\n\n/// A note sealed for a device with a key, or nothing.\naction seal_if_keyed(device: Device, note: Body) -> SealedText? {\n    return match device.device_key {\n        some(key) -> SealedText(crypto.encrypt(note, key: key, context: \"note\"))\n        none -> none\n    }\n}\n",
    );
    let output = jadpo(&root, &["check"]);
    let shown = format!("{}{}", output.stdout, output.stderr);
    assert!(!shown.contains("type.key_as_value"), "{shown}");
    assert!(!shown.contains("type.mismatch"), "{shown}");
}
