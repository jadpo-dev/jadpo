//! A person's export against live PostgreSQL (area 15, "Export" and "Who
//! may erase and export"): a member's own record, every record that
//! belongs to them and those erasure deletes with them, hidden or not and
//! in every store, as one JSON document with the API's field names and
//! nothing that is a credential or a secret. A caller without read on the person's
//! record, and someone erased, gets an export of nothing. The tests run
//! through `jadpo test` here, and compiled with the interpreter's records
//! in the gate's compiled step (`crates/jadpo-cli/tests/compiled.rs`).

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_term::Environment;

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

/// A library's readers, what they leave behind, and the tests its export
/// must pass.
pub(crate) fn files() -> Vec<(String, String)> {
    let action = |name: &str, uses: &str, signature: &str, body: &str| {
        (
            format!("domains/lending/actions/{name}.jadpo"),
            format!("{uses}\n\n/// {name}.\naction {name}{signature} {{\n{body}}}\n"),
        )
    };
    let entity =
        |name: &str, source: String| (format!("domains/lending/entities/{name}.jadpo"), source);
    vec![
        (
            "app.jadpo".to_string(),
            "use lending/entities { Reader }

application {
    connections: {
        main: {
            driver: Driver.postgres
        }
        archive: {
            driver: Driver.postgres
        }
        erasures: {
            driver: Driver.postgres
        }
    }
    schedules: {
        store: main
    }
    privacy: {
        erasure_log: {
            store: erasures
        }
    }
    field_encryption_key: config.field_encryption_key
}

principal {
    user: {
        subject: Reader.handle
    }
}

authentication {
    api_token: {}
}
"
            .to_string(),
        ),
        (
            "config.jadpo".to_string(),
            "config {\n    field_encryption_key: EncryptionKey\n    fine_retention: Duration {\n        default: 2192d\n    }\n}\n".to_string(),
        ),
        (
            "domains/lending/failures/handle_taken.jadpo".to_string(),
            "/// Someone has that handle already.\ntype HandleTaken = Failure {\n    kind: FailureKind.conflict\n}\n".to_string(),
        ),
        (
            "domains/lending/types/name.jadpo".to_string(),
            "/// A name, a title or a line of text.\ntype Name = Text {\n    max_length: 80\n}\n"
                .to_string(),
        ),
        entity(
            "reader",
            "use lending/types { Name }

/// Someone with a library card, who may be read only by themselves.
entity Reader {
    id: Uuid
    handle: Name {
        unique: true
        personal: PersonalData.identifying
    }
    invited_by_id: Reader.id? {
        on_delete: OnDelete.cleared
    }
    passphrase: PasswordHash?
    authenticator: TotpSecret?
    private_note: Name? {
        encrypted: true
    }
    left_at: Instant?

    lifecycle: {
        visible: left_at == none
        transitions: {
            delete: {
                when: left_at == none
                set: {
                    left_at: clock.now
                }
            }
        }
    }

    policy: {
        create: [Access.public]
        read: [id]
        update: [id]
        delete: [id]
    }

    person: true
}
"
            .to_string(),
        ),
        entity(
            "loan",
            format!(
                "use lending/entities {{ Reader }}
use lending/types {{ Name }}

/// A book a reader borrowed, hidden once it is returned.
entity Loan {{
    id: Uuid
    reader_id: Reader.id {{
        on_delete: OnDelete.restricted
        erase: Erasure.deleted
    }}
    title: Name
    returned_at: Instant?

    lifecycle: {{
        visible: returned_at == none
        transitions: {{
            delete: {{
                when: returned_at == none
                set: {{
                    returned_at: clock.now
                }}
            }}
        }}
    }}

{OPEN}}}
"
            ),
        ),
        entity(
            "loan_note",
            format!(
                "use lending/entities {{ Loan }}
use lending/types {{ Name }}

/// A librarian's note on a loan.
entity LoanNote {{
    id: Uuid
    loan_id: Loan.id {{
        on_delete: OnDelete.restricted
        erase: Erasure.deleted
    }}
    body: Name

{OPEN}}}
"
            ),
        ),
        entity(
            "review",
            format!(
                "use lending/entities {{ Reader }}
use lending/types {{ Name }}

/// A review a reader wrote, kept without them.
entity Review {{
    id: Uuid
    reader_id: Reader.id? {{
        on_delete: OnDelete.cleared
    }}
    body: Name

{OPEN}}}
"
            ),
        ),
        entity(
            "fine",
            format!(
                "use lending/entities {{ Reader }}
use lending/types {{ Name }}

/// A fine, kept for the accounts as it is.
entity Fine {{
    id: Uuid
    reader_id: Reader.id? {{
        on_delete: OnDelete.restricted
        erase: Erasure.retained
    }}
    amount: Name
    issued_at: Instant {{
        generated: Generated.on_create
    }}

    retention: {{
        after: config.fine_retention
        since: issued_at
    }}

{OPEN}}}
"
            ),
        ),
        entity(
            "keepsake",
            format!(
                "use lending/entities {{ Reader }}
use lending/types {{ Name }}

/// Something kept for a reader in the archive, deleted with them.
entity Keepsake {{
    id: Uuid
    reader_id: Reader.id {{
        erase: Erasure.deleted
    }}
    label: Name

{OPEN}
    store: archive
}}
"
            ),
        ),
        action(
            "join",
            "use lending/entities { Reader }\nuse lending/failures { HandleTaken }",
            "(handle: Reader.handle, passphrase: Password) fails HandleTaken -> Reader",
            "    return attempt Reader.create {\n        set: {\n            handle: handle\n            passphrase: crypto.hash_password(passphrase)\n            authenticator: none\n            private_note: \"only mine\"\n            invited_by_id: none\n        }\n        conflict: {\n            Reader.handle: HandleTaken\n        }\n    }\n",
        ),
        action(
            "invite",
            "use lending/entities { Reader, ReaderNotFound }\nuse lending/failures { HandleTaken }",
            "(inviter: Reader.id, handle: Reader.handle) fails ReaderNotFound, HandleTaken -> Reader",
            "    return attempt Reader.create {\n        set: {\n            handle: handle\n            passphrase: none\n            authenticator: none\n            private_note: none\n            invited_by_id: inviter\n        }\n        conflict: {\n            Reader.handle: HandleTaken\n        }\n    }\n",
        ),
        action(
            "leave",
            "use lending/entities { Reader, ReaderNotFound }",
            "(reader_id: Reader.id) fails ReaderNotFound -> Unit",
            "    var reader: Reader = attempt Reader.get(reader_id)\n    Reader.delete(reader)\n",
        ),
        action(
            "borrow",
            "use lending/entities { Loan, Reader, ReaderNotFound }",
            "(reader_id: Reader.id, title: Loan.title) fails ReaderNotFound -> Loan",
            "    return attempt Loan.create {\n        set: {\n            reader_id: reader_id\n            title: title\n        }\n    }\n",
        ),
        action(
            "give_back",
            "use lending/entities { Loan, LoanNotFound }",
            "(loan_id: Loan.id) fails LoanNotFound -> Unit",
            "    var loan: Loan = attempt Loan.get(loan_id)\n    Loan.delete(loan)\n",
        ),
        action(
            "note_on_loan",
            "use lending/entities { Loan, LoanNote, LoanNotFound }",
            "(loan_id: Loan.id, body: LoanNote.body) fails LoanNotFound -> LoanNote",
            "    return attempt LoanNote.create {\n        set: {\n            loan_id: loan_id\n            body: body\n        }\n    }\n",
        ),
        action(
            "write_review",
            "use lending/entities { Reader, ReaderNotFound, Review }",
            "(reader_id: Reader.id, body: Review.body) fails ReaderNotFound -> Review",
            "    return attempt Review.create {\n        set: {\n            reader_id: reader_id\n            body: body\n        }\n    }\n",
        ),
        action(
            "fine_reader",
            "use lending/entities { Fine, Reader, ReaderNotFound }",
            "(reader_id: Reader.id, amount: Fine.amount) fails ReaderNotFound -> Fine",
            "    return attempt Fine.create {\n        set: {\n            reader_id: reader_id\n            amount: amount\n        }\n    }\n",
        ),
        action(
            "keep_in_archive",
            "use lending/entities { Keepsake, Reader }",
            "(reader_id: Reader.id, label: Keepsake.label) -> Keepsake",
            "    return Keepsake.create {\n        set: {\n            reader_id: reader_id\n            label: label\n        }\n    }\n",
        ),
        action(
            "export_reader",
            "use lending/entities { Reader }",
            "(reader_id: Reader.id) -> PersonalDataExport",
            "    return Reader.export(reader_id)\n",
        ),
        action(
            "export_as",
            "use lending/entities { Reader }",
            "(reader_id: Reader.id, me: Principal.user) -> PersonalDataExport",
            "    return Reader.export(reader_id)\n",
        ),
        action(
            "forget",
            "use lending/entities { Reader, ReaderNotFound }",
            "(reader_id: Reader.id) fails ReaderNotFound -> Unit",
            "    attempt Reader.erase(reader_id)\n",
        ),
        (
            "domains/lending/actions/export_reader.test.jadpo".to_string(),
            r#"use lending/actions { borrow, export_as, fine_reader, forget, give_back, invite, join, keep_in_archive, leave, note_on_loan, write_review }
use lending/entities { Keepsake, Fine, Loan, LoanNote, Reader, Review }

test "an export holds everything of theirs and nothing else" {
    var ada: Reader = attempt join("ada", "correct horse battery staple")
    var bob: Reader = attempt join("bob", "another fine passphrase")
    var loan: Loan = attempt borrow(ada.id, "Middlemarch")
    var note: LoanNote = attempt note_on_loan(loan.id, "Spine loose")
    var written: Review = attempt write_review(ada.id, "A long one")
    var charged: Fine = attempt fine_reader(ada.id, "Three pounds")
    var kept: Keepsake = keep_in_archive(ada.id, "Pressed flower")
    var theirs: Loan = attempt borrow(bob.id, "Bob's own book")
    assert note.body == "Spine loose" and written.body == "A long one"
    assert charged.amount == "Three pounds" and kept.label == "Pressed flower"
    assert theirs.title == "Bob's own book"

    var export: PersonalDataExport = export_reader(ada.id)

    assert export.records == 6
    assert text.contains(export.json, "\"handle\":\"ada\"")
    assert text.contains(export.json, "\"title\":\"Middlemarch\"")
    assert text.contains(export.json, "\"body\":\"Spine loose\"")
    assert text.contains(export.json, "\"body\":\"A long one\"")
    assert text.contains(export.json, "\"amount\":\"Three pounds\"")
    assert text.contains(export.json, "\"label\":\"Pressed flower\"")
    assert text.contains(export.json, "\"private_note\":\"only mine\"")
    assert text.contains(export.json, "\"Loan\":[")
    assert not text.contains(export.json, "Bob")
}

test "a credential is never in an export" {
    var ada: Reader = attempt join("ada", "correct horse battery staple")
    var export: PersonalDataExport = export_reader(ada.id)
    assert export.records == 1
    assert not text.contains(export.json, "passphrase")
    assert not text.contains(export.json, "authenticator")
    assert not text.contains(export.json, "correct horse")
}

test "each kind of data says how long it is kept" {
    var ada: Reader = attempt join("ada", "correct horse battery staple")
    var charged: Fine = attempt fine_reader(ada.id, "Three pounds")
    assert charged.amount == "Three pounds"
    var export: PersonalDataExport = export_reader(ada.id)
    assert text.contains(export.json, "\"entity\":\"Fine\",\"retention\":[{\"field\":null,\"after\":\"PT52608H\",\"since\":\"issued_at\"}],\"recipients\":[]")
    assert text.contains(export.json, "\"entity\":\"Reader\",\"retention\":[],\"recipients\":[]")
}

test "a hidden record and a reader who has left are exported" {
    var ada: Reader = attempt join("ada", "correct horse battery staple")
    var loan: Loan = attempt borrow(ada.id, "Returned already")
    attempt give_back(loan.id)
    attempt leave(ada.id)

    var export: PersonalDataExport = export_reader(ada.id)

    assert export.records == 2
    assert text.contains(export.json, "\"title\":\"Returned already\"")
}

test "another person's own record that refers to them isn't in their export" {
    var ada: Reader = attempt join("ada", "correct horse battery staple")
    var cat: Reader = attempt invite(ada.id, "cat")
    var export: PersonalDataExport = export_reader(ada.id)
    assert export.records == 1
    assert not text.contains(export.json, "\"handle\":\"cat\"")
    assert cat.handle == "cat"
}

test "someone without read on the person's record gets nothing" {
    var ada: Reader = attempt join("ada", "correct horse battery staple")
    var bob: Reader = attempt join("bob", "another fine passphrase")
    var loan: Loan = attempt borrow(ada.id, "Middlemarch")
    assert loan.title == "Middlemarch"
    var theirs: PersonalDataExport = export_as(ada.id, Principal.user(bob))
    assert theirs.records == 0
    assert theirs.json == "{\"entities\":{},\"kinds\":[]}"
    var own: PersonalDataExport = export_as(ada.id, Principal.user(ada))
    assert own.records == 2
}

test "after erase, an export holds nothing" {
    var ada: Reader = attempt join("ada", "correct horse battery staple")
    var loan: Loan = attempt borrow(ada.id, "Middlemarch")
    var note: LoanNote = attempt note_on_loan(loan.id, "Spine loose")
    var written: Review = attempt write_review(ada.id, "A long one")
    var charged: Fine = attempt fine_reader(ada.id, "Three pounds")
    var kept: Keepsake = keep_in_archive(ada.id, "Pressed flower")
    assert note.body == "Spine loose" and written.body == "A long one"
    assert charged.amount == "Three pounds" and kept.label == "Pressed flower"
    attempt forget(ada.id)
    var export: PersonalDataExport = export_reader(ada.id)
    assert export.records == 0
    assert export.json == "{\"entities\":{},\"kinds\":[]}"
}
"#
            .to_string(),
        ),
    ]
}

/// A project of `files` in a folder of its own.
fn project(name: &str, files: &[(String, String)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("jadpo-export-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        fs::write(path, text).expect("write");
    }
    root
}

/// An export that would leave out a `File` field is refused where it is
/// written, until the links downloads give are built.
#[test]
fn an_export_reaching_a_file_is_not_built_yet() {
    let mut files = files();
    for (path, text) in &mut files {
        if path == "app.jadpo" {
            *text = text.replace(
                "    connections: {\n",
                "    connections: {\n        scans: {\n            driver: Driver.disk\n            path: \"/var/lib/library/scans\"\n        }\n",
            );
        }
        if path == "domains/lending/actions/join.jadpo"
            || path == "domains/lending/actions/invite.jadpo"
        {
            *text = text.replace(
                "            private_note:",
                "            photo: none\n            private_note:",
            );
        }
        if path == "domains/lending/entities/reader.jadpo" {
            *text = text.replace(
                "    passphrase: PasswordHash?\n",
                "    passphrase: PasswordHash?\n    photo: File? {\n        store: scans\n        max_size: 1mb\n        types: [FileType.png]\n    }\n",
            );
        }
    }
    let root = project("file", &files);
    let records = jadpo(&root, &["records", "update"]);
    assert_eq!(records.code, 0, "{}{}", records.stdout, records.stderr);
    let checked = jadpo(&root, &["check"]);
    assert_ne!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let output = format!("{}{}", checked.stdout, checked.stderr);
    assert!(output.contains("toolchain.not_built_yet"), "{output}");
    assert!(
        output.contains("Exporting a `Reader` here isn't built yet"),
        "{output}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn an_export_passes_its_tests() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("tests", &files());
    for args in [&["migrate", "new", "initial"][..], &["records", "update"]] {
        let done = jadpo(&root, args);
        assert_eq!(done.code, 0, "{args:?}: {}{}", done.stdout, done.stderr);
    }
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("7 passed"), "{}", tested.stdout);
    let _ = fs::remove_dir_all(&root);
}
