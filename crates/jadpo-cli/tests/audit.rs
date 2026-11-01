//! The audit trail against live PostgreSQL (area 16, "Audit trail"; chunk
//! B11): an entity with `audit:` keeps a history of every change to its
//! records, each entry a `jadpo_audit` row written in the transaction
//! that makes the change, read back newest first through
//! `Entity.history(id)`.
//!
//! The project's own tests, run through `jadpo test`, change audited
//! records every way a body can (a create, an update, a counter by `id`, a
//! delete, a lifecycle's `delete` transition, a subscriber's update) and
//! read each change back: who made it, through which action, and each
//! changed field before and after as text, a field no request may touch
//! left out; a reader the record's grants let in sees it without the
//! field its own `policy:` keeps from them, and anyone else an empty page;
//! a long history comes 25 entries a page; a subscriber of
//! `AuditEntryRecorded` hears each entry; and a person's erasure takes the
//! history of each record it deletes. They run interpreted here and
//! compiled with the interpreter's records in the gate's compiled step
//! (`crates/jadpo-cli/tests/compiled.rs`, "an audit trail").
//!
//! The rows are read here after runs through the interpreter: each entry
//! numbered in its record's order and emitted to the outbox with it, the
//! entries of an erased person's records gone, and the purge schedule's
//! audit retention deleting only the entity's entries older than its
//! period.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools; `JADPO_SKIP_POSTGRES=1` skips them.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_host_native::Runtime;
use jadpo_interp::Program;
use jadpo_runtime::{Executor, Outcome, SqlValue};
use jadpo_term::Environment;
use jadpo_test::host::Harness;
use jadpo_value::Value;

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

/// A bookkeeper's invoices and an owner's notes, both audited, with the
/// actions that change them and the tests the audit trail must pass.
pub(crate) fn files() -> Vec<(String, String)> {
    FILES
        .iter()
        .map(|(path, text)| (path.to_string(), text.to_string()))
        .collect()
}

pub(crate) const FILES: [(&str, &str); 28] = [
    (
        "app.jadpo",
        r#"use billing/entities { Clerk }

application {}

principal {
    user: {
        subject: Clerk.handle
    }
}

authentication {
    api_token: {}
}
"#,
    ),
    (
        "config.jadpo",
        r#"config {
    invoice_audit_retention: Duration {
        default: 2557d
    }
}
"#,
    ),
    (
        "domains/billing/actions/add_owner.jadpo",
        r#"use billing/entities { Owner }
use billing/types { Name }

/// Adds an owner.
action add_owner(alias: Name) -> Owner {
    return Owner.create {
        set: {
            alias: alias
        }
    }
}
"#,
    ),
    (
        "domains/billing/actions/bill.jadpo",
        r#"use billing/entities { Clerk, ClerkNotFound, Invoice }
use billing/types { InvoiceStatus, Name }

/// Issues an invoice, reviewed by `reviewer`.
action bill(me: Principal.user, reviewer: Clerk.id, number: Name) fails ClerkNotFound -> Invoice {
    return attempt Invoice.create {
        set: {
            clerk_id: me.id
            reviewer_id: reviewer
            number: number
            total: money.from_minor(12000, Currency.gbp)
            note: none
            status: InvoiceStatus.open
            reminders: 0
            discount: 5
            import_reference: none
        }
    }
}
"#,
    ),
    (
        "domains/billing/actions/bill.test.jadpo",
        r#"use billing/entities { Clerk, Invoice }
use billing/actions { cancel, invoice_history, join, remind, settle }

test "every change to an invoice is kept, newest first" {
    clock: "2026-10-20T09:00:00Z"
    var ada: Clerk = attempt join("ada")
    var bea: Clerk = attempt join("bea")
    var invoice: Invoice = attempt bill(Principal.user(ada), bea.id, "INV-1")
    attempt settle(Principal.user(ada), invoice.id, "paid by card")
    attempt remind(Principal.user(ada), invoice.id)
    var page: Page<AuditEntry> = Invoice.history(invoice.id)
    assert list.map(page.items, entry -> entry.change) == [AuditChange.updated, AuditChange.updated, AuditChange.created]
    assert list.map(page.items, entry -> entry.by) == [AuditBy.user, AuditBy.user, AuditBy.user]
    assert list.map(page.items, entry -> entry.user_id) == [ada.id, ada.id, ada.id]
    assert list.map(page.items, entry -> entry.action) == ["remind", "settle", "bill"]
    assert list.map(page.items, entry -> list.map(entry.fields, change -> change.field)) == [["reminders"], ["note", "status", "discount"], ["clerk_id", "reviewer_id", "number", "total", "status", "reminders", "discount"]]
    assert list.map(page.items, entry -> list.map(entry.fields, change -> change.after)) == [["1"], ["paid by card", "paid", "2"], [text.from(ada.id), text.from(bea.id), "INV-1", "GBP 120.00", "open", "0", "5"]]
    assert list.map(page.items, entry -> list.map(entry.fields, change -> change.before)) == [["0"], [none, "open", "5"], [none, none, none, none, none, none, none]]
    assert page.next == none
}

test "a reader sees the changes their grants let them read" {
    var ada: Clerk = attempt join("ada")
    var bea: Clerk = attempt join("bea")
    var cy: Clerk = attempt join("cy")
    var invoice: Invoice = attempt bill(Principal.user(ada), bea.id, "INV-2")
    attempt settle(Principal.user(ada), invoice.id, "paid")
    var own: Page<AuditEntry> = invoice_history(Principal.user(ada), invoice.id, none)
    assert list.map(own.items, entry -> list.map(entry.fields, change -> change.field)) == [["note", "status", "discount"], ["clerk_id", "reviewer_id", "number", "total", "status", "reminders", "discount"]]
    var reviewed: Page<AuditEntry> = invoice_history(Principal.user(bea), invoice.id, none)
    assert list.map(reviewed.items, entry -> list.map(entry.fields, change -> change.field)) == [["note", "status"], ["clerk_id", "reviewer_id", "number", "total", "status", "reminders"]]
    var outside: Page<AuditEntry> = invoice_history(Principal.user(cy), invoice.id, none)
    assert outside.items == []
}

test "a deleted invoice's history is kept" {
    var ada: Clerk = attempt join("ada")
    var bea: Clerk = attempt join("bea")
    var invoice: Invoice = attempt bill(Principal.user(ada), bea.id, "INV-3")
    attempt cancel(Principal.user(ada), invoice.id)
    var page: Page<AuditEntry> = Invoice.history(invoice.id)
    assert list.map(page.items, entry -> entry.change) == [AuditChange.deleted, AuditChange.created]
    assert list.map(page.items, entry -> list.map(entry.fields, change -> change.after)) == [[none, none, none, none, none, none, none], [text.from(ada.id), text.from(bea.id), "INV-3", "GBP 120.00", "open", "0", "5"]]
    var gone: Page<AuditEntry> = invoice_history(Principal.user(ada), invoice.id, none)
    assert gone.items == []
}

test "a history comes a page at a time" {
    var ada: Clerk = attempt join("ada")
    var bea: Clerk = attempt join("bea")
    var invoice: Invoice = attempt bill(Principal.user(ada), bea.id, "INV-4")
    for n in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25] {
        assert n > 0
        attempt remind(Principal.user(ada), invoice.id)
    }
    var first: Page<AuditEntry> = Invoice.history(invoice.id)
    assert list.count(first.items) == 25
    assert list.map(list.keep_first(first.items, 1), entry -> list.map(entry.fields, change -> change.after)) == [["25"]]
    assert first.next != none
    var second: Page<AuditEntry> = Invoice.history(invoice.id, after: first.next)
    assert list.map(second.items, entry -> entry.change) == [AuditChange.created]
    assert second.next == none
}
"#,
    ),
    (
        "domains/billing/actions/cancel.jadpo",
        r#"use billing/entities { Invoice, InvoiceNotFound }

/// Deletes an invoice.
action cancel(me: Principal.user, invoice_id: Invoice.id) fails InvoiceNotFound -> Unit {
    attempt Invoice.delete(invoice_id)
}
"#,
    ),
    (
        "domains/billing/actions/close_note.jadpo",
        r#"use billing/entities { Note, NoteNotFound }

/// Closes a note, which is kept hidden.
action close_note(note_id: Note.id) fails NoteNotFound -> Unit {
    attempt Note.delete(note_id)
}
"#,
    ),
    (
        "domains/billing/actions/forget.jadpo",
        r#"use billing/entities { Owner, OwnerNotFound }

/// Erases an owner.
action forget(owner_id: Owner.id) fails OwnerNotFound -> Unit {
    attempt Owner.erase(owner_id)
}
"#,
    ),
    (
        "domains/billing/actions/invoice_history.jadpo",
        r#"use billing/entities { Invoice }

/// The invoice's changes, newest first, as the caller may see them.
action invoice_history(me: Principal.user, invoice_id: Invoice.id, after: PageCursor?) -> Page<AuditEntry> {
    return Invoice.history(invoice_id, after: after)
}
"#,
    ),
    (
        "domains/billing/actions/join.jadpo",
        r#"use billing/entities { Clerk }
use billing/failures { HandleTaken }
use billing/types { Name }

/// Adds a clerk.
action join(handle: Name) fails HandleTaken -> Clerk {
    return attempt Clerk.create {
        set: {
            handle: handle
        }
        conflict: {
            Clerk.handle: HandleTaken
        }
    }
}
"#,
    ),
    (
        "domains/billing/actions/keep_copy.jadpo",
        r#"use billing/entities { Copy }

/// Keeps a copy of an audit entry.
action keep_copy(event: AuditEntryRecorded) -> Copy {
    return Copy.create {
        set: {
            record_id: event.record_id
            change: event.change
            by: event.by
            fields: list.count(event.fields)
        }
    }
}
"#,
    ),
    (
        "domains/billing/actions/remind.jadpo",
        r#"use billing/entities { Invoice, InvoiceNotFound }

/// Counts one more reminder sent.
action remind(me: Principal.user, invoice_id: Invoice.id) fails InvoiceNotFound -> Invoice {
    return attempt Invoice.update(invoice_id) {
        set: {
            reminders: Invoice.reminders + 1
        }
    }
}
"#,
    ),
    (
        "domains/billing/actions/review_note.jadpo",
        r#"use billing/entities { Note, NoteNotFound }

/// Marks a note reviewed.
action review_note(note_id: Note.id) fails NoteNotFound -> Unit {
    attempt Note.update(note_id) {
        set: {
            reviewed: true
        }
    }
}
"#,
    ),
    (
        "domains/billing/actions/settle.jadpo",
        r#"use billing/entities { Invoice, InvoiceNotFound }
use billing/types { InvoiceStatus, Name }

/// Marks an invoice paid, with a note and a smaller discount.
action settle(me: Principal.user, invoice_id: Invoice.id, note: Name) fails InvoiceNotFound -> Invoice {
    var invoice: Invoice = attempt Invoice.get(invoice_id)
    return Invoice.update(invoice) {
        set: {
            status: InvoiceStatus.paid
            note: note
            discount: 2
        }
    }
}
"#,
    ),
    (
        "domains/billing/actions/write_note.jadpo",
        r#"use billing/entities { Note, Owner, OwnerNotFound }
use billing/events { NoteWritten }
use billing/types { Name }

/// Writes a note for an owner; a subscriber reviews it.
action write_note(owner_id: Owner.id, body: Name) fails OwnerNotFound -> Note {
    var note: Note = attempt Note.create {
        set: {
            owner_id: owner_id
            body: body
            reviewed: false
        }
    }
    emit NoteWritten {
        note_id: note.id
    }
    return note
}
"#,
    ),
    (
        "domains/billing/actions/write_note.test.jadpo",
        r#"use billing/entities { Copy, Note, NoteNotFound, Owner }
use billing/actions { add_owner, close_note, forget }
use billing/queries { copies_of }

test "a subscriber's change is the application's, and a closed note's change is kept" {
    var ida: Owner = add_owner("ida")
    var note: Note = attempt write_note(ida.id, "call back")
    attempt close_note(note.id)
    assert Note.get(note.id) fails NoteNotFound
    var page: Page<AuditEntry> = Note.history(note.id)
    assert list.map(page.items, entry -> entry.change) == [AuditChange.updated, AuditChange.updated, AuditChange.created]
    assert list.map(page.items, entry -> entry.by) == [AuditBy.no_caller, AuditBy.application, AuditBy.no_caller]
    assert list.map(page.items, entry -> entry.action) == ["close_note", "review_note", "write_note"]
    assert list.map(page.items, entry -> list.map(entry.fields, change -> change.field)) == [["closed_at"], ["reviewed"], ["owner_id", "body", "reviewed"]]
}

test "a subscriber hears each entry" {
    var ida: Owner = add_owner("ida")
    var note: Note = attempt write_note(ida.id, "call back")
    var copies: List<Copy> = copies_of(Copy.record_id(note.id))
    assert list.count(copies) == 2
    assert list.sum(list.map(copies, copy -> copy.fields)) == 4
}

test "a note's history goes with its owner's erasure" {
    var ida: Owner = add_owner("ida")
    var note: Note = attempt write_note(ida.id, "call back")
    attempt close_note(note.id)
    attempt forget(ida.id)
    var page: Page<AuditEntry> = Note.history(note.id)
    assert page.items == []
}
"#,
    ),
    (
        "domains/billing/entities/clerk.jadpo",
        r#"use billing/types { Name }

/// Someone who keeps the books.
entity Clerk {
    id: Uuid
    handle: Name {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [Access.public]
        update: [id]
    }
}
"#,
    ),
    (
        "domains/billing/entities/copy.jadpo",
        r#"/// An audit entry passed on elsewhere, as a SIEM would keep it.
entity Copy {
    id: Uuid
    record_id: Uuid
    change: AuditChange
    by: AuditBy
    fields: Int

    policy: {
        create: [Access.system]
        read: [Access.public]
    }
}
"#,
    ),
    (
        "domains/billing/entities/invoice.jadpo",
        r#"use billing/entities { Clerk }
use billing/types { InvoiceStatus, Name }

/// A bill, whose every change is kept.
entity Invoice {
    id: Uuid
    clerk_id: Clerk.id {
        on_delete: OnDelete.restricted
    }
    reviewer_id: Clerk.id? {
        on_delete: OnDelete.cleared
    }
    number: Name
    total: Money
    note: Name?
    status: InvoiceStatus
    reminders: Int
    discount: Int {
        policy: {
            read: [clerk_id]
            update: [clerk_id]
        }
    }
    import_reference: Name? {
        policy: FieldAccess.server_only
    }

    policy: {
        create: [clerk_id]
        read: [clerk_id, reviewer_id]
        update: [clerk_id]
        delete: [clerk_id]
    }

    audit: {
        retention: {
            after: config.invoice_audit_retention
        }
    }
}
"#,
    ),
    (
        "domains/billing/entities/note.jadpo",
        r#"use billing/entities { Owner }
use billing/types { Name }

/// A note of an owner's, whose every change is kept, and which goes with
/// them.
entity Note {
    id: Uuid
    owner_id: Owner.id {
        on_delete: OnDelete.cascade
    }
    body: Name
    closed_at: Instant?
    reviewed: Bool

    lifecycle: {
        visible: closed_at == none
        transitions: {
            delete: {
                set: {
                    closed_at: clock.now
                }
            }
        }
    }

    policy: {
        create: [Access.public]
        read: [Access.public, Access.system]
        update: [Access.public, Access.system]
        delete: [Access.public]
    }

    audit: {}
}
"#,
    ),
    (
        "domains/billing/entities/owner.jadpo",
        r#"use billing/types { Name }

/// Someone who keeps notes, and may be erased.
entity Owner {
    id: Uuid
    alias: Name

    policy: {
        create: [Access.public]
        read: [Access.public]
        delete: [Access.public]
    }

    person: true
}
"#,
    ),
    (
        "domains/billing/events/note_written.jadpo",
        r#"use billing/entities { Note }

/// Someone wrote a note.
type NoteWritten = Event {
    note_id: Note.id
}
"#,
    ),
    (
        "domains/billing/failures/handle_taken.jadpo",
        r#"/// Someone has that handle already.
type HandleTaken = Failure {
    kind: FailureKind.conflict
}
"#,
    ),
    (
        "domains/billing/queries/copies_of.jadpo",
        r#"use billing/entities { Copy }

/// The copies kept of a record's entries.
query copies_of(record_id: Copy.record_id) -> List<Copy> {
    from: Copy
    where: Copy.record_id == record_id
    order: [Copy.id asc]
    limit: 50
}
"#,
    ),
    (
        "domains/billing/subscribers/copy_entries.jadpo",
        r#"use billing/actions { keep_copy }

/// Passes each audit entry on.
subscriber copy_entries {
    on: AuditEntryRecorded
    run: keep_copy(event)
}
"#,
    ),
    (
        "domains/billing/subscribers/review_written.jadpo",
        r#"use billing/actions { review_note }
use billing/events { NoteWritten }

/// Reviews each note written.
subscriber review_written {
    on: NoteWritten
    run: review_note(event.note_id)
}
"#,
    ),
    (
        "domains/billing/types/invoice_status.jadpo",
        r#"/// Where an invoice has got to.
type InvoiceStatus = Enum {
    open
    paid
}
"#,
    ),
    (
        "domains/billing/types/name.jadpo",
        r#"/// A handle, a number or a line of text.
type Name = Text {
    max_length: 80
}
"#,
    ),
];

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("audit-b11-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files() {
        write(&root, &path, &text);
    }
    for args in [&["records", "update"][..], &["migrate", "new", "initial"]] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{args:?}: {}{}", done.stdout, done.stderr);
    }
    root
}

#[test]
fn the_audit_trail_passes_its_tests() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("tests");
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("7 passed"), "{}", tested.stdout);
}

/// 2026-10-19T09:00:00Z.
const T0: i64 = 1_792_400_400_000;
const DAY: i64 = 24 * 3_600_000;

/// Runs `action` with `args` at the harness's clock, with no caller.
async fn call(program: &Program, host: &Harness, action: &str, args: Vec<Value>) -> Value {
    let id = program.plan().find(action).expect("the action");
    let mut ctx = host.ctx(program);
    match program.run(&mut ctx, id, args).await {
        Outcome::Value(value) => value,
        other => panic!("{action}: expected a value, got {other:?}"),
    }
}

fn id_of(value: &Value) -> Value {
    match value {
        Value::Record(record) => record.get("id").cloned().expect("an id"),
        other => panic!("not a record: {other:?}"),
    }
}

/// The text in each row's first column.
async fn texts(host: &Harness, sql: &str) -> Vec<String> {
    host.rows(sql)
        .await
        .into_iter()
        .map(|row| match &row[0] {
            SqlValue::Text(text) => text.clone(),
            SqlValue::Null(_) => "none".to_string(),
            other => panic!("not text: {other:?}"),
        })
        .collect()
}

#[test]
fn entries_are_rows_beside_their_records_until_erasure_or_retention() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("rows");
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let host = Harness::postgres(&url).at(T0).random_from(0);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let ida = call(&program, &host, "add_owner", vec![Value::text("ida")]).await;
        let jo = call(&program, &host, "add_owner", vec![Value::text("jo")]).await;
        let note = call(
            &program,
            &host,
            "write_note",
            vec![id_of(&ida), Value::text("call back")],
        )
        .await;
        call(&program, &host, "write_note", vec![id_of(&jo), Value::text("pay")]).await;
        call(&program, &host, "close_note", vec![id_of(&note)]).await;
        // Each entry is its record's next, by the action that made it.
        let entries = "SELECT \"seq\" || ' ' || \"change\" || ' ' || \"by\" || ' ' || \"action\" || ' ' || (SELECT string_agg(\"c\"->>'field', ',' ORDER BY \"n\") FROM jsonb_array_elements(\"changes\") WITH ORDINALITY AS \"e\"(\"c\", \"n\")) FROM \"jadpo_audit\" WHERE \"entity\" = 'Note' ORDER BY \"record_id\" = (SELECT \"id\" FROM \"note\" WHERE \"body\" = 'pay'), \"seq\"";
        assert_eq!(
            texts(&host, entries).await,
            [
                "1 created no_caller write_note owner_id,body,reviewed",
                "2 updated no_caller close_note closed_at",
                "1 created no_caller write_note owner_id,body,reviewed",
            ]
        );
        // The event its subscriber hears was written with each, in the
        // same transaction.
        assert_eq!(
            texts(
                &host,
                "SELECT count(*)::text FROM \"jadpo_outbox\" WHERE \"event\" = 'jadpo.AuditEntryRecorded'"
            )
            .await,
            ["3"]
        );
        // Erasing Ida takes her note's history with it; Jo's stays.
        call(&program, &host, "forget", vec![id_of(&ida)]).await;
        assert_eq!(
            texts(&host, entries).await,
            ["1 created no_caller write_note owner_id,body,reviewed"]
        );
        // Invoices' entries are kept seven years from when each was
        // written; the purge schedule deletes older ones, and no other
        // entity's.
        host.rows(
            "INSERT INTO \"jadpo_audit\" (\"entity\", \"record_id\", \"seq\", \"at\", \"change\", \"by\", \"changes\", \"encoding\") \
             SELECT 'Invoice', gen_random_uuid(), 1, TIMESTAMPTZ '2019-01-01 00:00:00+00', 'created', 'no_caller', '[]'::jsonb, 1 FROM generate_series(1, 600) \
             UNION ALL \
             SELECT 'Invoice', gen_random_uuid(), 1, TIMESTAMPTZ '2026-01-01 00:00:00+00', 'created', 'no_caller', '[]'::jsonb, 1 FROM generate_series(1, 3)",
        )
        .await;
        let started = program.started();
        let plans = program.runtime_plan().purges_of("Invoice", started);
        let kinds: Vec<String> = plans.iter().map(|(plan, _)| plan.what()).collect();
        assert_eq!(kinds, ["the retention of `Invoice`'s history"]);
        assert_eq!(plans[0].1, Some(2557 * DAY));
        let first = jadpo_runtime::purge_all(&host, &plans, T0, 1)
            .await
            .expect("purge");
        assert_eq!(first.batches, vec![500]);
        assert!(!first.finished, "stopped at its budget");
        let rest = jadpo_runtime::purge_all(&host, &plans, T0, jadpo_runtime::PURGE_BATCHES)
            .await
            .expect("purge");
        assert_eq!(rest.batches, vec![100]);
        assert!(rest.finished);
        assert_eq!(
            texts(
                &host,
                "SELECT \"entity\" || ' ' || count(*)::text FROM \"jadpo_audit\" GROUP BY \"entity\" ORDER BY \"entity\""
            )
            .await,
            ["Invoice 3", "Note 1"]
        );
    });
    drop(host);
    databases.release(copy, false);
}
