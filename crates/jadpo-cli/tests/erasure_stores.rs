//! Erasure past the person's own transaction, against live PostgreSQL (area
//! 15, "What erasure reaches" and "What erasure doesn't rewrite"; chunk
//! P4c). A club keeps its members in `main` and what they leave behind in
//! `archive`, and a person there may be a person of their own:
//!
//! - erasing a member commits in `main` with a `PersonErased` that holds
//!   their entity's name, `id` and the time and nothing personal; then the
//!   generated subscriber applies each reference's `erase:` in `archive`:
//!   their old todos deleted with their notes and the bytes of their scans,
//!   a review anonymised, an invoice kept without them, a letter's signature
//!   kept until its last member is erased;
//! - a patient in `archive` deleted that way is erased in their own right,
//!   and their prescriptions, back in `main`, go with their own event;
//! - the log keeps each of them after the commit, in `erasures` or in the
//!   store of files `scans`, and an app's own subscriber hears them too;
//! - a store that can't be reached leaves the delivery `pending` and shown,
//!   tried again until it can, and a delivery that runs out of time `failed`
//!   for an operator, whose retry finishes it;
//! - a restore, of `main`, of `archive` or of a kept person's record, doesn't
//!   bring anyone back: the replay every start runs erases them again, and a
//!   second one finds nothing and changes nothing.
//!
//! Its tests run interpreted through `jadpo test` here, and compiled with
//! the interpreter's records in the gate's compiled step (`compiled.rs`,
//! "an erasure across stores"). The rows are read here after runs through
//! the interpreter; a real start's replay is `jadpo dev`'s here, a compiled
//! deployable's in `deployable.rs` and a Worker's in `workers/erased.rs`.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_host_native::{Postgres, Runtime};
use jadpo_interp::Program;
use jadpo_runtime::capability::Capability;
use jadpo_runtime::erasure::{Replayed, replay};
use jadpo_runtime::files::MemoryFiles;
use jadpo_runtime::runner::sweep;
use jadpo_runtime::{Database, Executor, Host, Outcome, SqlValue};
use jadpo_term::Environment;
use jadpo_test::host::Harness;
use jadpo_value::Value;

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    run(&args, &env.with_cwd(cwd))
}

const OPEN: &str = "    policy: {
        create: [Access.public]
        read: [Access.public]
        update: [Access.public]
        delete: [Access.public]
    }
";

/// A small desk that answers requests, which a real start replays for:
/// `application` is what its `application { }` says of its connections,
/// its log and its deployables, and its `Ticket`s are kept in `archive`.
pub(crate) fn served_files(application: &str, config: &str) -> Vec<(String, String)> {
    let policy = "    policy: {
        create: [Access.public]
        read: [Access.public]
        delete: [Access.public]
    }
";
    let file = |path: &str, text: String| (path.to_string(), text);
    vec![
        file("app.jadpo", format!("application {{\n{application}}}\n")),
        file("config.jadpo", format!("config {{\n{config}}}\n")),
        file(
            "domains/desk/types/handle.jadpo",
            "/// What someone goes by.\ntype Handle = Text {\n    max_length: 80\n}\n".to_string(),
        ),
        file(
            "domains/desk/entities/member.jadpo",
            format!(
                "use desk/types {{ Handle }}\n\n/// Someone at the desk.\nentity Member {{\n    id: Uuid\n    decline data.personal_unread: \"Only erasure is under test\"\n    handle: Handle {{\n        personal: PersonalData.identifying\n    }}\n\n{policy}\n    person: true\n}}\n"
            ),
        ),
        file(
            "domains/desk/entities/ticket.jadpo",
            format!(
                "use desk/entities {{ Member }}\nuse desk/types {{ Handle }}\n\n/// What someone raised, deleted with them.\nentity Ticket {{\n    id: Uuid\n    owner_id: Member.id {{\n        erase: Erasure.deleted\n    }}\n    title: Handle\n\n{policy}\n    store: archive\n}}\n"
            ),
        ),
        file(
            "domains/desk/types/new_member.jadpo",
            "use desk/types { Handle }\n\n/// Who joins.\ntype NewMember = Input {\n    handle: Handle\n}\n".to_string(),
        ),
        file(
            "domains/desk/types/new_ticket.jadpo",
            "use desk/entities { Member }\nuse desk/types { Handle }\n\n/// What is raised.\ntype NewTicket = Input {\n    owner_id: Member.id\n    title: Handle\n}\n".to_string(),
        ),
        file(
            "domains/desk/types/joined.jadpo",
            "use desk/entities { Member }\n\n/// Who joined.\ntype Joined = Output {\n    id: Member.id\n}\n".to_string(),
        ),
        file(
            "domains/desk/types/closed.jadpo",
            "/// That it is done.\ntype Closed = Output {\n    okay: Bool\n}\n".to_string(),
        ),
        file(
            "domains/desk/actions/join.jadpo",
            "use desk/entities { Member }\nuse desk/types { Joined, NewMember }\n\n/// Adds someone.\naction join(input: NewMember) -> Joined {\n    var member: Member = Member.create {\n        set: {\n            handle: input.handle\n        }\n    }\n    return Joined {\n        id: member.id\n    }\n}\n".to_string(),
        ),
        file(
            "domains/desk/actions/raise.jadpo",
            "use desk/entities { Ticket }\nuse desk/types { Closed, NewTicket }\n\n/// Raises a ticket.\naction raise(input: NewTicket) -> Closed {\n    var ticket: Ticket = Ticket.create {\n        set: {\n            owner_id: input.owner_id\n            title: input.title\n        }\n    }\n    return Closed {\n        okay: ticket.id == ticket.id\n    }\n}\n".to_string(),
        ),
        file(
            "domains/desk/actions/close.jadpo",
            "use desk/entities { Member, MemberNotFound }\nuse desk/types { Closed }\n\n/// Erases someone.\naction close(member_id: Member.id) fails MemberNotFound -> Closed {\n    attempt Member.erase(member_id)\n    return Closed {\n        okay: true\n    }\n}\n".to_string(),
        ),
        file(
            "domains/desk/routes/members.jadpo",
            "use desk/actions { close, join, raise }\nuse desk/entities { Member }\nuse desk/types { Closed, Joined, NewMember, NewTicket }\n\n/// The desk.\nroutes Members {\n    prefix: /members\n\n    POST / {\n        input: NewMember\n        output: Joined\n        status: SuccessStatus.created\n        authentication: RouteAuthentication.none\n        run: join(input)\n    }\n\n    POST /tickets {\n        input: NewTicket\n        output: Closed\n        status: SuccessStatus.created\n        authentication: RouteAuthentication.none\n        run: raise(input)\n    }\n\n    POST /{member_id: Member.id}/close {\n        output: Closed\n        authentication: RouteAuthentication.none\n        run: close(member_id)\n    }\n}\n".to_string(),
        ),
    ]
}

/// The club, its log kept in the connection `log`: `erasures`, a database
/// that holds no person's records, or `scans`, a store of files.
pub(crate) fn files_with_log(log: &str) -> Vec<(String, String)> {
    let action = |name: &str, uses: &str, signature: &str, body: &str| {
        (
            format!("domains/club/actions/{name}.jadpo"),
            format!("{uses}\n\n/// {name}.\naction {name}{signature} {{\n{body}}}\n"),
        )
    };
    let entity =
        |name: &str, source: String| (format!("domains/club/entities/{name}.jadpo"), source);
    vec![
        (
            "app.jadpo".to_string(),
            format!(
                "use club/entities {{ Member }}

application {{
    connections: {{
        main: {{
            driver: Driver.postgres
        }}
        archive: {{
            driver: Driver.postgres
        }}
        erasures: {{
            driver: Driver.postgres
        }}
        scans: {{
            driver: Driver.disk
            path: \"/var/lib/club/scans\"
        }}
    }}
    schedules: {{
        store: main
    }}
    privacy: {{
        erasure_log: {{
            store: {log}
        }}
    }}
}}

principal {{
    user: {{
        subject: Member.handle
    }}
}}

authentication {{
    api_token: {{}}
}}
"
            ),
        ),
        (
            "config.jadpo".to_string(),
            "config {\n    kept_for: Duration {\n        default: 2192d\n    }\n}\n".to_string(),
        ),
        (
            "domains/club/types/name.jadpo".to_string(),
            "/// A name, a title or a line of text.\ntype Name = Text {\n    max_length: 80\n}\n"
                .to_string(),
        ),
        (
            "domains/club/failures/handle_taken.jadpo".to_string(),
            "/// Someone has that handle already.\ntype HandleTaken = Failure {\n    kind: FailureKind.conflict\n}\n".to_string(),
        ),
        (
            "domains/club/failures/scan_not_kept.jadpo".to_string(),
            "/// The store of scans had no room.\ntype ScanNotKept = Failure {\n    kind: FailureKind.unavailable\n    message: \"There was no room to keep the scan.\"\n}\n".to_string(),
        ),
        entity(
            "member",
            format!(
                "use club/types {{ Name }}

/// Someone in the club, kept in `main`.
entity Member {{
    id: Uuid
    handle: Name {{
        unique: true
        personal: PersonalData.identifying
    }}

{OPEN}
    person: true
}}
"
            ),
        ),
        entity(
            "todo",
            format!(
                "use club/entities {{ Member }}
use club/types {{ Name }}

/// Something a member means to do, in their own store.
entity Todo {{
    id: Uuid
    owner_id: Member.id {{
        on_delete: OnDelete.restricted
        erase: Erasure.deleted
    }}
    title: Name

{OPEN}}}
"
            ),
        ),
        entity(
            "old_todo",
            format!(
                "use club/entities {{ Member }}
use club/types {{ Name }}

/// What a member once meant to do, in the archive, deleted with them.
entity OldTodo {{
    id: Uuid
    owner_id: Member.id {{
        erase: Erasure.deleted
    }}
    title: Name
    scan: File? {{
        store: scans
        max_size: 1mb
        types: [FileType.csv]
    }}

{OPEN}
    store: archive
}}
"
            ),
        ),
        entity(
            "old_note",
            format!(
                "use club/entities {{ OldTodo }}
use club/types {{ Name }}

/// A note on an old todo, which goes with it.
entity OldNote {{
    id: Uuid
    todo_id: OldTodo.id {{
        on_delete: OnDelete.cascade
    }}
    words: Name

{OPEN}
    store: archive
}}
"
            ),
        ),
        entity(
            "old_review",
            format!(
                "use club/entities {{ Member }}
use club/types {{ Name }}

/// What a member wrote, kept in the archive without them.
entity OldReview {{
    id: Uuid
    author_id: Member.id? {{
        erase: Erasure.anonymised
    }}
    body: Name
    signed_as: Name? {{
        personal: PersonalData.identifying
    }}
    changed_at: Instant {{
        generated: Generated.on_change
    }}

{OPEN}
    store: archive
}}
"
            ),
        ),
        entity(
            "old_invoice",
            format!(
                "use club/entities {{ Member }}
use club/types {{ Name }}

/// A bill, kept for the tax years, in the archive, as it is.
entity OldInvoice {{
    id: Uuid
    customer_id: Member.id? {{
        erase: Erasure.retained
    }}
    billing_name: Name {{
        personal: PersonalData.identifying
    }}
    issued_at: Instant {{
        generated: Generated.on_create
    }}

{OPEN}
    retention: {{
        after: config.kept_for
        since: issued_at
    }}

    store: archive
}}
"
            ),
        ),
        entity(
            "old_letter",
            format!(
                "use club/entities {{ Member }}
use club/types {{ Name }}

/// A letter between two members, which belongs to both.
entity OldLetter {{
    id: Uuid
    sender_id: Member.id? {{
        erase: Erasure.anonymised
    }}
    recipient_id: Member.id? {{
        erase: Erasure.anonymised
    }}
    signed_as: Name? {{
        personal: PersonalData.identifying
    }}

{OPEN}
    store: archive
}}
"
            ),
        ),
        entity(
            "patient",
            format!(
                "use club/entities {{ Member }}
use club/types {{ Name }}

/// Someone a member looks after, kept in the archive, who goes with them.
entity Patient {{
    id: Uuid
    guardian_id: Member.id {{
        erase: Erasure.deleted
    }}
    known_as: Name {{
        personal: PersonalData.identifying
    }}

{OPEN}
    person: true

    store: archive
}}
"
            ),
        ),
        entity(
            "prescription",
            format!(
                "use club/entities {{ Patient }}
use club/types {{ Name }}

/// What a patient takes, back in `main`.
entity Prescription {{
    id: Uuid
    patient_id: Patient.id {{
        erase: Erasure.deleted
    }}
    drug: Name

{OPEN}}}
"
            ),
        ),
        entity(
            "customer",
            format!(
                "use club/types {{ Name }}

/// Someone with a policy, part of whose record the law makes the club keep.
entity Customer {{
    id: Uuid
    login: Name {{
        unique: true
    }}
    full_name: Name? {{
        personal: PersonalData.identifying
    }}
    policy_number: Name {{
        personal: PersonalData.identifying
        erase: Erasure.retained
    }}
    policy_ended_at: Instant?

{OPEN}
    person: true

    retention: {{
        after: config.kept_for
        since: policy_ended_at
    }}
}}
"
            ),
        ),
        entity(
            "departure",
            "/// Someone erased, as the app's own subscriber heard it.
entity Departure {
    id: Uuid
    who: Uuid
    person_entity: Text
    at: Instant

    policy: {
        create: [Access.system]
        read: [Access.public]
    }
}
"
            .to_string(),
        ),
        action(
            "join",
            "use club/entities { Member }\nuse club/failures { HandleTaken }",
            "(handle: Member.handle) fails HandleTaken -> Member",
            "    return attempt Member.create {\n        set: {\n            handle: handle\n        }\n        conflict: {\n            Member.handle: HandleTaken\n        }\n    }\n",
        ),
        action(
            "plan",
            "use club/entities { Member, Todo }",
            "(owner: Member, title: Todo.title) -> Todo",
            "    return Todo.create {\n        set: {\n            owner_id: owner.id\n            title: title\n        }\n    }\n",
        ),
        action(
            "file_todo",
            "use app { scans }\nuse club/entities { Member, OldTodo }\nuse club/failures { ScanNotKept }",
            "(owner: Member, title: OldTodo.title) fails ScanNotKept -> OldTodo",
            "    var scan: File = attempt scans.put(encoding.to_utf8(title), FileType.csv) {\n        storage_full: ScanNotKept\n    }\n    return OldTodo.create {\n        set: {\n            owner_id: owner.id\n            title: title\n            scan: scan\n        }\n    }\n",
        ),
        action(
            "note",
            "use club/entities { OldNote, OldTodo }",
            "(todo: OldTodo, words: OldNote.words) -> OldNote",
            "    return OldNote.create {\n        set: {\n            todo_id: todo.id\n            words: words\n        }\n    }\n",
        ),
        action(
            "review",
            "use club/entities { Member, OldReview }",
            "(author: Member, body: OldReview.body, signed_as: OldReview.signed_as) -> OldReview",
            "    return OldReview.create {\n        set: {\n            author_id: author.id\n            body: body\n            signed_as: signed_as\n        }\n    }\n",
        ),
        action(
            "bill",
            "use club/entities { Member, OldInvoice }",
            "(customer: Member, billing_name: OldInvoice.billing_name) -> OldInvoice",
            "    return OldInvoice.create {\n        set: {\n            customer_id: customer.id\n            billing_name: billing_name\n        }\n    }\n",
        ),
        action(
            "write_letter",
            "use club/entities { Member, OldLetter }",
            "(sender: Member, recipient: Member, signed_as: OldLetter.signed_as) -> OldLetter",
            "    return OldLetter.create {\n        set: {\n            sender_id: sender.id\n            recipient_id: recipient.id\n            signed_as: signed_as\n        }\n    }\n",
        ),
        action(
            "admit",
            "use club/entities { Member, Patient }",
            "(guardian: Member, known_as: Patient.known_as) -> Patient",
            "    return Patient.create {\n        set: {\n            guardian_id: guardian.id\n            known_as: known_as\n        }\n    }\n",
        ),
        action(
            "prescribe",
            "use club/entities { Patient, Prescription }",
            "(patient: Patient, drug: Prescription.drug) -> Prescription",
            "    return Prescription.create {\n        set: {\n            patient_id: patient.id\n            drug: drug\n        }\n    }\n",
        ),
        action(
            "take_out",
            "use club/entities { Customer }\nuse club/failures { HandleTaken }",
            "(login: Customer.login, full_name: Customer.full_name, number: Customer.policy_number) fails HandleTaken -> Customer",
            "    return attempt Customer.create {\n        set: {\n            login: login\n            full_name: full_name\n            policy_number: number\n            policy_ended_at: none\n        }\n        conflict: {\n            Customer.login: HandleTaken\n        }\n    }\n",
        ),
        action(
            "close",
            "use club/entities { Member, MemberNotFound }",
            "(member: Member.id) fails MemberNotFound -> Unit",
            "    attempt Member.erase(member)\n",
        ),
        action(
            "forget",
            "use club/entities { Customer, CustomerNotFound }",
            "(customer: Customer.id) fails CustomerNotFound -> Unit",
            "    attempt Customer.erase(customer)\n",
        ),
        action(
            "note_departure",
            "use club/entities { Departure }",
            "(event: PersonErased) -> Departure",
            "    return Departure.create {\n        set: {\n            who: event.id\n            person_entity: event.entity\n            at: event.erased_at\n        }\n    }\n",
        ),
        (
            "domains/club/subscribers/note_departures.jadpo".to_string(),
            "use club/actions { note_departure }\n\n/// Notes who has gone.\nsubscriber note_departures {\n    on: PersonErased\n    run: note_departure(event)\n}\n".to_string(),
        ),
        (
            "domains/club/queries/departures.jadpo".to_string(),
            "use club/entities { Departure }\n\n/// Whoever has gone.\nquery departures() -> List<Departure> {\n    from: Departure\n    order: [Departure.at asc, Departure.person_entity asc]\n    limit: 20\n}\n".to_string(),
        ),
        (
            "domains/club/actions/close.test.jadpo".to_string(),
            TESTS.to_string(),
        ),
    ]
}

/// The club, with its log in a database of its own.
pub(crate) fn files() -> Vec<(String, String)> {
    files_with_log("erasures")
}

const TESTS: &str = r#"use club/entities {
    Departure,
    Member,
    MemberNotFound,
    OldInvoice,
    OldLetter,
    OldNote,
    OldNoteNotFound,
    OldReview,
    OldTodo,
    OldTodoNotFound,
    Patient,
    PatientNotFound,
    Prescription,
    PrescriptionNotFound,
    Todo,
    TodoNotFound
}
use club/actions {
    admit,
    bill,
    file_todo,
    join,
    note,
    plan,
    prescribe,
    review,
    write_letter
}
use club/queries { departures }

test "erasing a member applies what each reference says in the archive" {
    var ada: Member = attempt join("ada")
    var bob: Member = attempt join("bob")
    var todo: Todo = plan(ada, "Pay the fees")
    var old: OldTodo = attempt file_todo(ada, "Old fees")
    var kept: OldTodo = attempt file_todo(bob, "Bob's fees")
    var scrawl: OldNote = note(old, "Overdue")
    var written: OldReview = review(ada, "Lovely", "Ada")
    var invoice: OldInvoice = bill(ada, "Ada Lovelace")
    attempt close(ada.id)
    assert Member.get(ada.id) fails MemberNotFound
    assert Todo.get(todo.id) fails TodoNotFound
    assert OldTodo.get(old.id) fails OldTodoNotFound
    assert OldNote.get(scrawl.id) fails OldNoteNotFound
    var still: OldTodo = attempt OldTodo.get(kept.id)
    assert still.owner_id == bob.id
    var anonymous: OldReview = attempt OldReview.get(written.id)
    assert anonymous.author_id == none
    assert anonymous.signed_as == none
    assert anonymous.body == "Lovely"
    var billed: OldInvoice = attempt OldInvoice.get(invoice.id)
    assert billed.customer_id == none
    assert billed.billing_name == "Ada Lovelace"
}

test "a letter keeps its signature until both its members are erased" {
    var ada: Member = attempt join("ada")
    var bob: Member = attempt join("bob")
    var letter: OldLetter = write_letter(ada, bob, "Ada")
    attempt close(ada.id)
    var after_ada: OldLetter = attempt OldLetter.get(letter.id)
    assert after_ada.sender_id == none
    assert after_ada.recipient_id == bob.id
    assert after_ada.signed_as == "Ada"
    attempt close(bob.id)
    var after_both: OldLetter = attempt OldLetter.get(letter.id)
    assert after_both.recipient_id == none
    assert after_both.signed_as == none
}

test "a person deleted in another store is erased in their own right" {
    var ada: Member = attempt join("ada")
    var patient: Patient = admit(ada, "Grace")
    var script: Prescription = prescribe(patient, "Tea")
    attempt close(ada.id)
    assert Patient.get(patient.id) fails PatientNotFound
    assert Prescription.get(script.id) fails PrescriptionNotFound
}

test "the app's own subscriber hears who was erased, with nothing personal" {
    var ada: Member = attempt join("ada")
    var patient: Patient = admit(ada, "Grace")
    attempt close(ada.id)
    var gone: List<Departure> = departures()
    var who: List<Uuid> = list.map(gone, one -> one.who)
    assert list.count(who) == 2
    assert list.contains(who, ada.id)
    assert list.contains(who, patient.id)
}
"#;

/// The club written out, migrated and its records updated, its log in
/// `log`.
fn project(name: &str, log: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("erasure-stores-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files_with_log(log) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        fs::write(path, text).expect("write");
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let recorded = jadpo(&root, &["records", "update"]);
    assert_eq!(recorded.code, 0, "{}{}", recorded.stdout, recorded.stderr);
    root
}

#[test]
fn an_erasure_across_stores_passes_its_tests() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("tests", "erasures");
    let tested = jadpo(&root, &["test"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
}

#[test]
fn with_the_log_in_a_store_of_files_it_passes_its_tests() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("tests-in-files", "scans");
    let tested = jadpo(&root, &["test"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
}

const T0: i64 = 1_792_400_400_000;
const HOUR: i64 = 3_600_000;
const DAY: i64 = 24 * HOUR;

/// A host over each of the club's databases, any of which can be down, and
/// its store of files in memory; its clock and randomness a harness's.
struct Club {
    clock: Harness,
    stores: BTreeMap<String, Postgres>,
    scans: MemoryFiles,
    down: RefCell<BTreeSet<String>>,
}

impl Club {
    /// Over a fresh copy of each of `root`'s databases: what the run's
    /// program writes, and the urls to read them through.
    fn new(copy: &jadpo_test::database::Copy) -> Club {
        let mut stores = BTreeMap::new();
        for (store, (_, url)) in &copy.stores {
            stores.insert(
                store.clone(),
                jadpo_host_native::postgres(url).expect("a store"),
            );
        }
        let (_, main) = copy.stores.get("main").expect("main");
        Club {
            clock: Harness::postgres(main).at(T0).random_from(0),
            stores,
            scans: MemoryFiles::new(),
            down: RefCell::default(),
        }
    }

    fn store(&self, store: &str) -> &Postgres {
        self.stores.get(store).expect("a store")
    }

    /// `sql`'s only column in `store`, each row as text, `none` for no value.
    async fn texts(&self, store: &str, sql: &str) -> Vec<String> {
        self.store(store)
            .query_once(sql, &[])
            .await
            .unwrap_or_else(|error| panic!("{sql}: {error:?}"))
            .iter()
            .map(|row| match row.first() {
                Some(SqlValue::Text(text)) => text.clone(),
                Some(SqlValue::Null(_)) => "none".to_string(),
                other => format!("{other:?}"),
            })
            .collect()
    }

    /// Runs `sql` in `store`.
    async fn run(&self, store: &str, sql: &str) {
        self.store(store)
            .query_once(sql, &[])
            .await
            .unwrap_or_else(|error| panic!("{sql}: {error:?}"));
    }

    /// `action` with `args`, dropping what it leaves to run after its
    /// response, as a crash does: the sweep finds it.
    async fn call(&self, program: &Program, action: &str, args: Vec<Value>) -> Value {
        let id = program.plan().find(action).expect("the action");
        let mut ctx = self.clock.ctx_on(program, self);
        match program.run(&mut ctx, id, args).await {
            Outcome::Value(value) => value,
            other => panic!("{action}: expected a value, got {other:?}"),
        }
    }

    /// Sweeps every store until nothing more is due at the host's time, a
    /// delivery's own events included.
    async fn sweep(&self, program: &Program) {
        for _ in 0..4 {
            for store in ["main", "archive"] {
                sweep(program, self, store, self.clock.now())
                    .await
                    .expect("a sweep");
            }
        }
    }
}

impl Host for Club {
    fn database(&self, store: &str) -> Option<&dyn Database> {
        if self.down.borrow().contains(store) {
            return None;
        }
        self.stores.get(store).map(|store| store as &dyn Database)
    }

    fn capability(&self, connection: &str) -> Option<&dyn Capability> {
        (connection == "scans").then_some(&self.scans as &dyn Capability)
    }

    fn random(&self, bytes: &mut [u8]) -> bool {
        self.clock.random(bytes)
    }

    fn now_ms(&self) -> Option<i64> {
        self.clock.now_ms()
    }
}

fn id_of(value: &Value) -> Value {
    match value {
        Value::Record(record) => record.get("id").cloned().expect("an id"),
        other => panic!("not a record: {other:?}"),
    }
}

fn id_text(value: &Value) -> String {
    match id_of(value) {
        Value::Uuid(text) => text.to_string(),
        other => panic!("not an id: {other:?}"),
    }
}

/// The club's program and a copy of its databases, for `run`.
fn in_club(name: &str, log: &str, run: impl AsyncFnOnce(&Program, &Club)) {
    in_project(project(name, log), run);
}

/// The program at `root` and a copy of its databases, for `run`.
fn in_project(root: PathBuf, run: impl AsyncFnOnce(&Program, &Club)) {
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let club = Club::new(&copy);
    Runtime::new()
        .expect("runtime")
        .block_on(run(&program, &club));
}

/// A member with something of every outcome, and one who stays: Ada and Bob.
async fn two_members(program: &Program, club: &Club) -> (Value, Value) {
    let join = |handle: &str| vec![Value::text(handle)];
    let ada = club.call(program, "join", join("ada")).await;
    let bob = club.call(program, "join", join("bob")).await;
    for (member, handle) in [(&ada, "ada"), (&bob, "bob")] {
        let pair = |action: &str| (action.to_string(), format!("{handle}'s {action}"));
        for (action, text) in [pair("plan"), pair("file_todo"), pair("bill")] {
            club.call(program, &action, vec![member.clone(), Value::text(&text)])
                .await;
        }
        club.call(
            program,
            "review",
            vec![
                member.clone(),
                Value::text(&format!("{handle}'s review")),
                Value::text(&format!("{handle} signed")),
            ],
        )
        .await;
        let patient = club
            .call(
                program,
                "admit",
                vec![member.clone(), Value::text(&format!("{handle}'s patient"))],
            )
            .await;
        club.call(
            program,
            "prescribe",
            vec![patient, Value::text(&format!("{handle}'s drug"))],
        )
        .await;
    }
    (ada, bob)
}

const ARCHIVED: [(&str, &str); 5] = [
    ("old_todos", "SELECT \"title\" FROM \"old_todo\" ORDER BY 1"),
    (
        "reviews",
        "SELECT \"body\" || ' ' || coalesce(\"signed_as\", 'none') || ' ' || (\"author_id\" IS NULL)::text FROM \"old_review\" ORDER BY 1",
    ),
    (
        "invoices",
        "SELECT \"billing_name\" || ' ' || (\"customer_id\" IS NULL)::text FROM \"old_invoice\" ORDER BY 1",
    ),
    (
        "patients",
        "SELECT \"known_as\" FROM \"patient\" ORDER BY 1",
    ),
    (
        "deliveries",
        "SELECT \"subscriber\" FROM \"jadpo_deliveries\" WHERE \"status\" <> 'succeeded' AND \"subscriber\" LIKE 'erasure%'",
    ),
];

/// The main commit first and everything else after it: until a sweep runs,
/// the archive still holds what it did, and the event holds nothing
/// personal. Then each reference's `erase:` holds there, and the log keeps
/// each person erased, the patient too, whose own event reaches their
/// prescriptions back in `main`.
#[test]
fn the_main_store_commits_first_then_each_store_and_the_log() {
    if !jadpo_test::live() {
        return;
    }
    in_club("rows", "erasures", async |program, club| {
        let (ada, _bob) = two_members(program, club).await;
        let ada_id = id_text(&ada);
        club.call(program, "close", vec![id_of(&ada)]).await;

        // Gone from `main` at once, and still in the archive.
        assert_eq!(
            club.texts("main", "SELECT \"handle\" FROM \"member\"")
                .await,
            ["bob"]
        );
        assert_eq!(
            club.texts("main", "SELECT \"title\" FROM \"todo\"").await,
            ["bob's plan"]
        );
        assert_eq!(
            club.texts("archive", ARCHIVED[0].1).await,
            ["ada's file_todo", "bob's file_todo"]
        );
        // What the event holds, and that its deliveries committed with it.
        let payload = club
            .texts(
                "main",
                "SELECT \"payload\"::text FROM \"jadpo_outbox\" WHERE \"event\" = 'jadpo.PersonErased'",
            )
            .await;
        let [payload] = payload.as_slice() else {
            panic!("one event: {payload:?}");
        };
        let read: serde_json::Value = serde_json::from_str(payload).expect("JSON");
        let object = read.as_object().expect("an object");
        let mut keys: Vec<&String> = object.keys().collect();
        keys.sort();
        assert_eq!(keys, ["entity", "erased_at", "id"], "{payload}");
        assert_eq!(object["entity"], "Member");
        assert_eq!(object["id"], ada_id.as_str());
        assert_eq!(object["erased_at"], T0);
        assert_eq!(
            club.texts(
                "main",
                "SELECT \"subscriber\" FROM \"jadpo_deliveries\" WHERE \"status\" = 'pending' ORDER BY 1",
            )
            .await,
            ["erasure in archive", "erasure log", "subscriber note_departures"]
        );
        assert_eq!(
            club.texts("erasures", "SELECT \"entity\" FROM \"jadpo_erasure_log\"")
                .await,
            Vec::<String>::new(),
            "the log is written after the commit"
        );

        club.sweep(program).await;

        // Each outcome, in the archive.
        assert_eq!(
            club.texts("archive", ARCHIVED[0].1).await,
            ["bob's file_todo"]
        );
        assert_eq!(
            club.texts("archive", ARCHIVED[1].1).await,
            ["ada's review none true", "bob's review bob signed false"]
        );
        assert_eq!(
            club.texts("archive", ARCHIVED[2].1).await,
            ["ada's bill true", "bob's bill false"]
        );
        assert_eq!(
            club.texts("archive", ARCHIVED[3].1).await,
            ["bob's patient"]
        );
        // The patient's own event went from the archive back to `main`.
        assert_eq!(
            club.texts("main", "SELECT \"drug\" FROM \"prescription\"")
                .await,
            ["bob's drug"]
        );
        assert_eq!(
            club.texts(
                "archive",
                "SELECT count(*)::text FROM \"jadpo_outbox\" WHERE \"event\" = 'jadpo.PersonErased'"
            )
            .await,
            ["1"]
        );
        // The bytes of her scan went too, from its delivery in the archive.
        assert_eq!(
            club.scans
                .keys()
                .iter()
                .filter(|key| jadpo_std::files::is_key(key))
                .count(),
            1,
            "only bob's scan: {:?}",
            club.scans.keys()
        );
        // Both are logged, by their born names, and nothing personal.
        assert_eq!(
            club.texts(
                "erasures",
                "SELECT \"entity\" || ' ' || (\"erased_at\" = TIMESTAMPTZ '2026-10-19 09:00:00+00')::text FROM \"jadpo_erasure_log\" ORDER BY 1"
            )
            .await
            .iter()
            .map(|row| row.split(' ').next().unwrap_or_default().to_string())
            .collect::<Vec<_>>(),
            ["Member", "Patient"]
        );
        // The app's own subscriber heard them both.
        assert_eq!(
            club.texts(
                "main",
                "SELECT \"person_entity\" FROM \"departure\" ORDER BY 1"
            )
            .await,
            ["Member", "Patient"]
        );
        for store in ["main", "archive"] {
            assert_eq!(
                club.texts(store, ARCHIVED[4].1).await,
                Vec::<String>::new(),
                "every delivery of an erasure succeeded in {store}"
            );
        }
    });
}

/// A delivery repeated, as a crash after its commit makes it, finds nothing
/// and changes nothing, and a record the app wrote since is reached.
#[test]
fn applying_an_erasure_again_finds_only_what_came_since() {
    if !jadpo_test::live() {
        return;
    }
    in_club("repeat", "erasures", async |program, club| {
        let (ada, _bob) = two_members(program, club).await;
        club.call(program, "close", vec![id_of(&ada)]).await;
        club.sweep(program).await;
        let before = club.texts("archive", ARCHIVED[1].1).await;
        // Every delivery due again, as a lost worker leaves it.
        club.run(
            "main",
            "UPDATE \"jadpo_deliveries\" SET \"status\" = 'pending', \"next_attempt_at\" = TIMESTAMPTZ '2026-01-01 00:00:00+00' WHERE \"subscriber\" LIKE 'erasure%'",
        )
        .await;
        let ada_id = id_text(&ada);
        club.run(
            "archive",
            &format!(
                "INSERT INTO \"old_todo\" (\"id\", \"owner_id\", \"title\") VALUES (gen_random_uuid(), '{ada_id}', 'written late')"
            ),
        )
        .await;
        club.sweep(program).await;
        assert_eq!(club.texts("archive", ARCHIVED[1].1).await, before);
        assert_eq!(
            club.texts("archive", ARCHIVED[0].1).await,
            ["bob's file_todo"],
            "what was written since goes too"
        );
        assert_eq!(
            club.texts(
                "erasures",
                "SELECT count(*)::text FROM \"jadpo_erasure_log\""
            )
            .await,
            ["2"],
            "one entry for each"
        );
    });
}

/// A store that can't be reached leaves its delivery where the erasure's
/// commit put it, shown to an operator and tried again, and the log and the
/// other subscribers don't wait for it. Once it can be reached it is
/// applied. One that runs out of time is `failed`, and stays for an operator,
/// whose retry finishes it.
#[test]
fn a_store_that_cant_apply_it_is_tried_again_and_shown() {
    if !jadpo_test::live() {
        return;
    }
    in_club("retries", "erasures", async |program, club| {
        let (ada, bob) = two_members(program, club).await;
        club.down.borrow_mut().insert("archive".to_string());
        club.call(program, "close", vec![id_of(&ada)]).await;
        for store in ["main"] {
            sweep(program, club, store, club.clock.now())
                .await
                .expect("a sweep");
        }
        let delivery = |status: &str| {
            format!(
                "SELECT \"status\" || ' ' || \"attempts\"::text || ' ' || (\"last_failure\" ->> 'code') FROM \"jadpo_deliveries\" WHERE \"subscriber\" = 'erasure in archive' AND \"status\" = '{status}'"
            )
        };
        assert_eq!(
            club.texts("main", &delivery("pending")).await,
            ["pending 1 unavailable"]
        );
        // The log doesn't wait for it.
        assert_eq!(
            club.texts("erasures", "SELECT \"entity\" FROM \"jadpo_erasure_log\"")
                .await,
            ["Member"]
        );
        // `jadpo deliveries` lists it.
        let holds = jadpo_runtime::deliveries::Holds {
            deliveries: true,
            runs: false,
            steps: false,
            messages: false,
            batches: false,
        };
        let shown = jadpo_runtime::deliveries::items(
            club.store("main"),
            holds,
            &["pending"],
            10,
            club.clock.now(),
        )
        .await
        .expect("the items");
        assert!(
            shown.iter().any(|item| item.target == "erasure in archive"
                && item.code.as_deref() == Some("unavailable")),
            "{shown:?}"
        );
        // Still down at its next attempt, it counts another.
        club.clock.advance(DAY);
        sweep(program, club, "main", club.clock.now())
            .await
            .expect("a sweep");
        assert_eq!(
            club.texts("main", &delivery("pending")).await,
            ["pending 2 unavailable"]
        );
        assert_eq!(
            club.texts("archive", ARCHIVED[0].1).await,
            ["ada's file_todo", "bob's file_todo"]
        );

        // Back, it is applied.
        club.down.borrow_mut().clear();
        club.clock.advance(DAY);
        club.sweep(program).await;
        assert_eq!(
            club.texts("archive", ARCHIVED[0].1).await,
            ["bob's file_todo"]
        );
        assert_eq!(
            club.texts(
                "main",
                "SELECT \"status\" FROM \"jadpo_deliveries\" WHERE \"subscriber\" = 'erasure in archive'"
            )
            .await,
            ["succeeded"]
        );

        // Down past the month it has, it is `failed`, and stays shown.
        club.down.borrow_mut().insert("archive".to_string());
        club.call(program, "close", vec![id_of(&bob)]).await;
        club.clock.advance(31 * DAY);
        sweep(program, club, "main", club.clock.now())
            .await
            .expect("a sweep");
        assert_eq!(
            club.texts("main", &delivery("failed")).await.len(),
            1,
            "{:?}",
            club.texts(
                "main",
                "SELECT \"subscriber\" || ' ' || \"status\" FROM \"jadpo_deliveries\""
            )
            .await
        );
        let shown = jadpo_runtime::deliveries::items(
            club.store("main"),
            holds,
            &["failed"],
            10,
            club.clock.now(),
        )
        .await
        .expect("the items");
        let failed = shown
            .iter()
            .find(|item| item.target == "erasure in archive")
            .unwrap_or_else(|| panic!("not shown: {shown:?}"));
        assert_eq!(
            club.texts("archive", ARCHIVED[0].1).await,
            ["bob's file_todo"],
            "nothing reached it"
        );
        // An operator's retry, once it is back, finishes it.
        club.down.borrow_mut().clear();
        let now = club.clock.now();
        jadpo_runtime::deliveries::act_on_delivery(
            club.store("main"),
            club,
            failed,
            jadpo_runtime::deliveries::Action::Retry,
            "an operator",
            now,
        )
        .await
        .expect("the retry");
        club.sweep(program).await;
        assert_eq!(
            club.texts("archive", ARCHIVED[0].1).await,
            Vec::<String>::new()
        );
    });
}

/// `replay` as a start runs it.
async fn replayed(program: &Program, club: &Club) -> Replayed {
    let events = program.plan();
    let events = events.events();
    replay(club, &events.erasure, &events.files)
        .await
        .expect("the replay")
}

/// Ada's rows put back as a backup holds them: `main`'s own and what her
/// records in `archive` held, as they were before she was erased.
async fn restore(club: &Club, ada: &Value, main: bool, archive: bool) {
    let id = id_text(ada);
    if main {
        club.run(
            "main",
            &format!("INSERT INTO \"member\" (\"id\", \"handle\") VALUES ('{id}', 'ada')"),
        )
        .await;
        club.run(
            "main",
            &format!(
                "INSERT INTO \"todo\" (\"id\", \"owner_id\", \"title\") VALUES (gen_random_uuid(), '{id}', 'restored plan')"
            ),
        )
        .await;
        // A restore takes the outbox back too, to before the erasure.
        club.run("main", "DELETE FROM \"jadpo_outbox\"").await;
    }
    if archive {
        club.run(
            "archive",
            &format!(
                "INSERT INTO \"old_todo\" (\"id\", \"owner_id\", \"title\") VALUES (gen_random_uuid(), '{id}', 'restored todo')"
            ),
        )
        .await;
        club.run(
            "archive",
            &format!(
                "UPDATE \"old_review\" SET \"author_id\" = '{id}', \"signed_as\" = 'ada signed' WHERE \"body\" = 'ada''s review'"
            ),
        )
        .await;
    }
}

/// The whole of a restore's story: each store brought back on its own and
/// together, the replay erasing Ada again wherever she is, and a second
/// replay finding nothing, writing nothing and emitting nothing. It runs
/// with the log in a database and in a store of files.
fn restoring_brings_no_one_back(name: &str, log: &str) {
    in_club(name, log, async |program, club| {
        let (ada, _bob) = two_members(program, club).await;
        club.call(program, "close", vec![id_of(&ada)]).await;
        club.sweep(program).await;

        // Nothing restored: nothing to do, however many are logged.
        let quiet = replayed(program, club).await;
        assert_eq!(quiet.logged, 2);
        assert_eq!(quiet.erased, 0);

        // A log it can't read stops the start: nothing is served on a guess.
        if log == "erasures" {
            club.down.borrow_mut().insert("erasures".to_string());
            let plan = program.plan();
            let events = plan.events();
            let stopped = replay(club, &events.erasure, &events.files)
                .await
                .expect_err("no log to read");
            assert_eq!(stopped.store, "erasures");
            club.down.borrow_mut().clear();
            // An entry of an entity no longer declared has nothing to erase.
            club.run(
                "erasures",
                "INSERT INTO \"jadpo_erasure_log\" (\"entity\", \"person_id\", \"erased_at\") VALUES ('Retired', gen_random_uuid(), now())",
            )
            .await;
            assert_eq!(
                replayed(program, club).await,
                Replayed {
                    logged: 3,
                    erased: 0
                }
            );
            club.run(
                "erasures",
                "DELETE FROM \"jadpo_erasure_log\" WHERE \"entity\" = 'Retired'",
            )
            .await;
        }

        // `main` restored: she is erased before anything is served, and her
        // erasure's commit sets the rest off again.
        restore(club, &ada, true, false).await;
        assert_eq!(
            club.texts("main", "SELECT \"handle\" FROM \"member\" ORDER BY 1")
                .await,
            ["ada", "bob"]
        );
        let found = replayed(program, club).await;
        assert_eq!(found.erased, 1, "{found:?}");
        assert_eq!(
            club.texts("main", "SELECT \"handle\" FROM \"member\"")
                .await,
            ["bob"]
        );
        assert_eq!(
            club.texts("main", "SELECT \"title\" FROM \"todo\"").await,
            ["bob's plan"]
        );
        // An event for its subscribers, with the time she was first erased.
        let payload = club
            .texts(
                "main",
                "SELECT (\"payload\" ->> 'erased_at') FROM \"jadpo_outbox\" WHERE \"event\" = 'jadpo.PersonErased'",
            )
            .await;
        assert_eq!(payload, [T0.to_string()]);

        // Replayed again: nothing, and nothing new in any outbox.
        let count = async |store: &str| {
            club.texts(store, "SELECT count(*)::text FROM \"jadpo_outbox\"")
                .await
        };
        let outboxes = (count("main").await, count("archive").await);
        let idle = replayed(program, club).await;
        assert_eq!(idle.erased, 0, "{idle:?}");
        assert_eq!((count("main").await, count("archive").await), outboxes);
        club.sweep(program).await;

        // `archive` restored alone: her records there go, with no event from
        // `main`, where she isn't.
        restore(club, &ada, false, true).await;
        assert_eq!(
            club.texts("archive", ARCHIVED[0].1).await,
            ["bob's file_todo", "restored todo"]
        );
        let found = replayed(program, club).await;
        assert_eq!(found.erased, 1, "{found:?}");
        assert_eq!(
            club.texts("archive", ARCHIVED[0].1).await,
            ["bob's file_todo"]
        );
        assert_eq!(
            club.texts("archive", ARCHIVED[1].1).await,
            ["ada's review none true", "bob's review bob signed false"]
        );
        assert_eq!(count("main").await, outboxes.0);

        // The patient, deleted in the archive, comes back with it, and goes
        // again by their own entry.
        let patient = logged_patient(club, log).await;
        club.run(
            "archive",
            &format!(
                "INSERT INTO \"patient\" (\"id\", \"guardian_id\", \"known_as\") VALUES ('{patient}', gen_random_uuid(), 'restored patient')"
            ),
        )
        .await;
        let found = replayed(program, club).await;
        assert_eq!(found.erased, 1, "{found:?}");
        assert_eq!(
            club.texts("archive", ARCHIVED[3].1).await,
            ["bob's patient"]
        );
    });
}

/// The `id` of the patient the log holds, kept in `log`.
async fn logged_patient(club: &Club, log: &str) -> String {
    if log == "erasures" {
        return club
            .texts(
                "erasures",
                "SELECT \"person_id\"::text FROM \"jadpo_erasure_log\" WHERE \"entity\" = 'Patient'",
            )
            .await
            .into_iter()
            .next()
            .expect("the patient's entry");
    }
    club.scans
        .keys()
        .iter()
        .find_map(|key| {
            let (born, id) = jadpo_std::erasure::log_key_parts(key)?;
            (born == "Patient").then(|| id.to_string())
        })
        .expect("the patient's entry")
}

#[test]
fn a_restore_brings_no_one_back_with_the_log_in_a_database() {
    if !jadpo_test::live() {
        return;
    }
    restoring_brings_no_one_back("restore-database", "erasures");
}

#[test]
fn a_restore_brings_no_one_back_with_the_log_in_a_store_of_files() {
    if !jadpo_test::live() {
        return;
    }
    restoring_brings_no_one_back("restore-files", "scans");
}

/// A kept person a restore brings back, not yet erased as far as their
/// record knows, is cleared again; one whose record says so already isn't
/// touched, so a replay never moves when they were erased, and neither
/// counts it nor emits.
#[test]
fn a_replay_clears_a_restored_kept_record_and_leaves_an_erased_one() {
    if !jadpo_test::live() {
        return;
    }
    in_club("restore-kept", "erasures", async |program, club| {
        let ada = club
            .call(
                program,
                "take_out",
                vec![
                    Value::text("ada"),
                    Value::text("Ada Lovelace"),
                    Value::text("P-100"),
                ],
            )
            .await;
        club.call(program, "forget", vec![id_of(&ada)]).await;
        club.sweep(program).await;
        let row = "SELECT coalesce(\"full_name\", 'none') || ' ' || \"policy_number\" || ' ' || (\"jadpo_erased_at\" IS NOT NULL)::text FROM \"customer\"";
        assert_eq!(club.texts("main", row).await, ["none P-100 true"]);
        club.clock.advance(HOUR);

        let idle = replayed(program, club).await;
        assert_eq!(
            idle,
            Replayed {
                logged: 1,
                erased: 0
            }
        );

        // As the backup holds it, from before she was erased.
        club.run(
            "main",
            "UPDATE \"customer\" SET \"full_name\" = 'Ada Lovelace', \"jadpo_erased_at\" = NULL",
        )
        .await;
        let found = replayed(program, club).await;
        assert_eq!(found.erased, 1, "{found:?}");
        assert_eq!(club.texts("main", row).await, ["none P-100 true"]);
        // The time she was first erased, whenever the replay ran.
        let logged = "SELECT \"erased_at\"::text FROM \"jadpo_erasure_log\"";
        let noted = "SELECT \"jadpo_erased_at\"::text FROM \"customer\"";
        assert_eq!(
            club.texts("main", noted).await,
            club.texts("erasures", logged).await
        );
    });
}

/// A plan with no log has nothing to replay, and asks nothing of any store.
/// The desk with flags, kept in `store`, and its log in `erasures`:
/// `archive` holds what members leave behind, and `flagged` holds nothing
/// but the flags.
fn flagged_desk(name: &str, store: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("erasure-flags-{name}"));
    let _ = fs::remove_dir_all(&root);
    let application = format!(
        "    connections: {{
        main: {{
            driver: Driver.postgres
            url: config.main_url
        }}
        archive: {{
            driver: Driver.postgres
            url: config.archive_url
        }}
        flagged: {{
            driver: Driver.postgres
            url: config.flagged_url
        }}
        erasures: {{
            driver: Driver.postgres
            url: config.erasures_url
        }}
    }}
    flags: {{
        store: {store}
    }}
    privacy: {{
        erasure_log: {{
            store: erasures
        }}
    }}
"
    );
    let config = "    main_url: PostgresUrl\n    archive_url: PostgresUrl\n    flagged_url: PostgresUrl\n    erasures_url: PostgresUrl\n";
    let mut files = served_files(&application, config);
    files.push(("flags.jadpo".to_string(), FLAGS_FILE.to_string()));
    for (path, text) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        fs::write(path, text).expect("write");
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let recorded = jadpo(&root, &["records", "update"]);
    assert_eq!(recorded.code, 0, "{}{}", recorded.stdout, recorded.stderr);
    root
}

/// A flag the desk declares; its state is what these tests read.
pub(crate) const FLAGS_FILE: &str = "flags {
    /// The new desk page.
    new_desk: Bool {
        default: false
        owner: \"desk\"
        expires: \"2099-01-31\"
    }
}
";

/// Someone who joined the desk, by `handle`.
async fn joined(program: &Program, club: &Club, handle: &str) -> String {
    let input = Value::record(
        "NewMember",
        vec![(std::rc::Rc::from("handle"), Value::text(handle))],
    );
    id_text(&club.call(program, "join", vec![input]).await)
}

/// Statements that put Ada and Bob in the flags' two tables: a flag naming
/// them both, one naming only Ada, and a log of each change.
pub(crate) fn flag_rows(ada: &str, bob: &str) -> [String; 4] {
    let both = format!(r#"{{"{ada}":"true","{bob}":"false"}}"#);
    let only = format!(r#"{{"{ada}":"true"}}"#);
    let state = |users: &str| format!(r#"{{"value":null,"users":{users},"shares":[]}}"#);
    [
        format!(
            "INSERT INTO jadpo_flags (flag, value, users, shares, set_at) VALUES ('new_desk', NULL, '{both}', NULL, '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO jadpo_flags (flag, value, users, shares, set_at) VALUES ('old_page', 'true', '{only}', NULL, '2026-01-01T00:00:00Z')"
        ),
        format!(
            "INSERT INTO jadpo_flag_changes (id, made_at, flag, action, was, now, operator, role) VALUES (gen_random_uuid(), '2026-01-02T00:00:00Z', 'new_desk', 'set', '{}', '{}', 'sam', 'app_owner')",
            state(&only),
            state(&both)
        ),
        format!(
            "INSERT INTO jadpo_flag_changes (id, made_at, flag, action, was, now, operator, role) VALUES (gen_random_uuid(), '2026-01-03T00:00:00Z', 'new_desk', 'reset', '{}', NULL, 'sam', 'app_owner')",
            state(&both)
        ),
    ]
}

/// Everything the flags' two tables hold, as text.
pub(crate) const FLAG_STATE: &str = "SELECT flag || ' ' || coalesce(users::text, 'none') || ' ' || set_at::text FROM jadpo_flags ORDER BY flag";
pub(crate) const FLAG_LOG: &str = "SELECT flag || ' ' || action || ' ' || coalesce(was::text, 'none') || ' ' || coalesce(now::text, 'none') || ' ' || operator FROM jadpo_flag_changes ORDER BY made_at";

async fn assert_only_bob_in_the_flags(club: &Club, store: &str, ada: &str, bob: &str) {
    let state = club.texts(store, FLAG_STATE).await;
    let log = club.texts(store, FLAG_LOG).await;
    for row in state.iter().chain(&log) {
        assert!(!row.contains(ada), "{row}");
    }
    assert_eq!(state.len(), 2, "{state:?}");
    assert!(state[0].starts_with("new_desk"), "{state:?}");
    assert!(state[0].contains(bob), "{state:?}");
    assert!(state[1].starts_with("old_page none"), "{state:?}");
    assert_eq!(log.len(), 2, "kept for good: {log:?}");
    assert!(log[0].contains(bob), "{log:?}");
    assert!(log[1].contains(bob), "{log:?}");
}

/// A member erased in the store the flags are in leaves no row of theirs in
/// either table, by the commit: everyone else's named users stay, a flag that
/// named only them has no named users left, and the log keeps each change
/// without them.
#[test]
fn erasing_a_person_takes_them_out_of_the_flags_with_the_commit() {
    if !jadpo_test::live() {
        return;
    }
    in_project(flagged_desk("own", "main"), async |program, club| {
        let ada = joined(program, club, "ada").await;
        let bob = joined(program, club, "bob").await;
        for row in flag_rows(&ada, &bob) {
            club.run("main", &row).await;
        }
        let id = Value::uuid(&ada);
        club.call(program, "close", vec![id]).await;
        assert_only_bob_in_the_flags(club, "main", &ada, &bob).await;
        // A change an operator read before the erasure is then told the
        // flag changed, rather than writing Ada back.
        let moved = club
                .texts(
                    "main",
                    "SELECT (extract(epoch FROM set_at) * 1000)::bigint::text FROM jadpo_flags WHERE flag = 'new_desk'",
                )
                .await;
        assert_eq!(moved, [T0.to_string()]);
        // A person who was never named changes nothing.
        let carol = joined(program, club, "carol").await;
        club.call(program, "close", vec![Value::uuid(&carol)]).await;
        assert_only_bob_in_the_flags(club, "main", &ada, &bob).await;
    });
}

/// A member whose flags are in a store that also holds records of theirs:
/// the generated subscriber's statement takes them out with those records.
#[test]
fn an_erasure_in_another_store_takes_them_out_of_the_flags_there() {
    if !jadpo_test::live() {
        return;
    }
    in_project(flagged_desk("archive", "archive"), async |program, club| {
        let ada = joined(program, club, "ada").await;
        let bob = joined(program, club, "bob").await;
        for row in flag_rows(&ada, &bob) {
            club.run("archive", &row).await;
        }
        club.call(program, "close", vec![Value::uuid(&ada)]).await;
        assert!(
            club.texts("archive", FLAG_STATE)
                .await
                .iter()
                .any(|row| row.contains(&ada)),
            "the other store's share runs after the commit"
        );
        club.sweep(program).await;
        assert_only_bob_in_the_flags(club, "archive", &ada, &bob).await;
    });
}

/// Flags in a store that holds nothing else of theirs get a delivery of
/// their own, retried and shown as any other, and the replay reaches them
/// after a restore of that store alone.
#[test]
fn flags_in_a_store_of_their_own_are_erased_after_the_commit_and_replayed() {
    if !jadpo_test::live() {
        return;
    }
    in_project(flagged_desk("flagged", "flagged"), async |program, club| {
        let ada = joined(program, club, "ada").await;
        let bob = joined(program, club, "bob").await;
        for row in flag_rows(&ada, &bob) {
            club.run("flagged", &row).await;
        }
        club.call(program, "close", vec![Value::uuid(&ada)]).await;
        assert_eq!(
                club.texts(
                    "main",
                    "SELECT \"subscriber\" FROM \"jadpo_deliveries\" WHERE \"status\" = 'pending' ORDER BY 1",
                )
                .await,
                ["erasure in archive", "erasure in flagged", "erasure log"]
            );
        club.sweep(program).await;
        assert_only_bob_in_the_flags(club, "flagged", &ada, &bob).await;

        // A restore of the flags' store alone: her id is back, and
        // nothing else of hers.
        club.run("flagged", "DELETE FROM jadpo_flags").await;
        club.run("flagged", "DELETE FROM jadpo_flag_changes").await;
        for row in flag_rows(&ada, &bob) {
            club.run("flagged", &row).await;
        }
        let first = replayed(program, club).await;
        assert_eq!((first.logged, first.erased), (1, 1), "{first:?}");
        assert_only_bob_in_the_flags(club, "flagged", &ada, &bob).await;
        let second = replayed(program, club).await;
        assert_eq!((second.logged, second.erased), (1, 0), "{second:?}");
    });
}

/// With the last flag gone the tables stay, and an erasure still reaches
/// them, until a migration of their own drops them; after that an erasure
/// asks nothing of them (area 16, "Removing").
#[test]
fn an_erasure_reaches_the_flag_tables_for_as_long_as_they_are_there() {
    if !jadpo_test::live() {
        return;
    }
    let root = flagged_desk("no-flags", "main");
    fs::write(root.join("flags.jadpo"), "flags {}\n").expect("write");
    let recorded = jadpo(&root, &["records", "update"]);
    assert_eq!(recorded.code, 0, "{}{}", recorded.stdout, recorded.stderr);
    in_project(root.clone(), async |program, club| {
        let ada = joined(program, club, "ada").await;
        let bob = joined(program, club, "bob").await;
        for row in flag_rows(&ada, &bob) {
            club.run("main", &row).await;
        }
        club.call(program, "close", vec![Value::uuid(&ada)]).await;
        assert_only_bob_in_the_flags(club, "main", &ada, &bob).await;
    });
    let dropped = jadpo(
        &root,
        &["migrate", "new", "drop_flags", "--drop", "jadpo_flags"],
    );
    assert_eq!(dropped.code, 0, "{}{}", dropped.stdout, dropped.stderr);
    let recorded = jadpo(&root, &["records", "update"]);
    assert_eq!(recorded.code, 0, "{}{}", recorded.stdout, recorded.stderr);
    in_project(root, async |program, club| {
        let ada = joined(program, club, "ada").await;
        club.call(program, "close", vec![Value::uuid(&ada)]).await;
        assert_eq!(
            club.texts("main", "SELECT \"handle\" FROM \"member\"")
                .await,
            Vec::<String>::new()
        );
    });
}

#[test]
fn a_replay_with_no_log_does_nothing() {
    let plan = jadpo_std::erasure::ErasurePlan::default();
    let files = jadpo_std::files::FilesPlan::default();
    let host = jadpo_runtime::MemoryHost::new();
    let replayed = jadpo_runtime::block_on(replay(&host, &plan, &files)).expect("no wait");
    assert_eq!(replayed, Ok(Replayed::default()));
}

/// `jadpo dev` replays the log as it starts, kept in `.jadpo/files/`, on a
/// database of this machine, so a restored one is erased before it answers
/// anything.
#[test]
fn jadpo_dev_replays_the_log_as_it_starts() {
    if !jadpo_test::live() {
        return;
    }
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("erasure-stores-dev");
    let _ = fs::remove_dir_all(&root);
    let application = "    connections: {
        main: {
            driver: Driver.postgres
            url: config.main_url
        }
        archive: {
            driver: Driver.postgres
            url: config.archive_url
        }
        erasures: {
            driver: Driver.disk
            path: \"/var/lib/desk/erasures\"
            free_space_floor: 0%
        }
    }
    privacy: {
        erasure_log: {
            store: erasures
        }
    }
";
    let config = "    main_url: PostgresUrl\n    archive_url: PostgresUrl\n";
    for (path, text) in served_files(application, config) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        fs::write(path, text).expect("write");
    }
    for args in [&["migrate", "new", "initial"][..], &["records", "update"]] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{}{}", done.stdout, done.stderr);
    }
    let server = jadpo_test::database::server().expect("a PostgreSQL server");
    let url = server.create_database("erasure_dev").expect("a database");
    let archive = server
        .create_database("erasure_dev_archive")
        .expect("a database");
    fs::write(
        root.join(".env.local"),
        format!(
            "MAIN_URL={}\nARCHIVE_URL={}\n",
            super::deployable::as_url(&url),
            super::deployable::as_url(&archive)
        ),
    )
    .expect("the settings");
    let dev = super::http::Dev::start_without_database(&root);
    let joined = dev.send("POST", "/members", &[], r#"{"handle":"ada"}"#);
    assert_eq!(joined.status, 201, "{joined:?}");
    let id = joined.json()["id"].as_str().expect("an id").to_string();
    let ticket = format!(r#"{{"owner_id":"{id}","title":"The heating"}}"#);
    let raised = dev.send("POST", "/members/tickets", &[], &ticket);
    assert_eq!(raised.status, 201, "{raised:?}");
    let closed = dev.send("POST", &format!("/members/{id}/close"), &[], "");
    assert_eq!(closed.status, 200, "{closed:?}");
    // Its entry, once its delivery has run after the response.
    let entry = root.join(format!(".jadpo/files/erasures/erasure-log/Member/{id}"));
    let waited = std::time::Instant::now();
    while !entry.exists() {
        assert!(
            waited.elapsed() < std::time::Duration::from_secs(30),
            "no entry: {}",
            dev.log()
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    drop(dev);
    // A restore brings them back, and leaves the log.
    let database = jadpo_host_native::postgres(&url).expect("the database");
    let runtime = Runtime::new().expect("a runtime");
    let run = |sql: String| {
        runtime
            .block_on(database.query_once(&sql, &[]))
            .unwrap_or_else(|error| panic!("{sql}: {error:?}"))
    };
    run(format!(
        "INSERT INTO \"member\" (\"id\", \"handle\") VALUES ('{id}', 'ada')"
    ));
    let count = |sql: &str| run(sql.to_string()).len();
    assert_eq!(count("SELECT 1 FROM \"member\""), 1);
    let dev = super::http::Dev::start_without_database(&root);
    assert_eq!(
        count("SELECT 1 FROM \"member\""),
        0,
        "erased before it served: {}",
        dev.log()
    );
}
