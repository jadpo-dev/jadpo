//! Area 15, "Erasure" (chunk P3): `User.erase(user_id)` erases a person,
//! deleting their record and deciding what happens to each record of
//! theirs by the reference that links it to a record erasure deletes: its
//! `erase:`, or what its `on_delete:` implies. A `restricted` reference
//! into records erasure deletes must say, a reference or marked field it
//! clears can be `none`, a record it keeps as it is has a `retention:`,
//! and an `erase:` it never reaches is refused. A record that belongs to
//! several people keeps its marked fields until the last of them is
//! erased, and another person's own record loses only the reference
//! (chunk P4). The person's own record kept field by field isn't built
//! yet, refused where its field's `erase:` is written. As policy sees it,
//! an erasure is a `delete` of the person's record, and an action erasing
//! one runs at repeatable read.

use jadpo_db::{Database, Layout};

use crate::support;

/// The problems of `source`, after the settings, as `path rule: message`.
fn problems(source: &str) -> Vec<String> {
    support::problems(Layout::Loose, &[("main.jadpo", &with_settings(source))])
        .into_iter()
        .map(|problem| problem.trim_start().to_string())
        .collect()
}

/// The rules of each problem of `source`, after the settings, in order.
fn rules(source: &str) -> Vec<String> {
    support::rules(Layout::Loose, &[("main.jadpo", &with_settings(source))])
}

fn with_settings(source: &str) -> String {
    format!("{SETTINGS}\n{source}")
}

const SETTINGS: &str = "config {
    invoice_retention: Duration {
        default: 2192d
    }
}
";

const CLEAN: [&str; 0] = [];

/// A person, and a type for their records' text.
const USER: &str = "type Caption = Text {
    max_length: 200
}

entity User {
    id: Uuid
    email: Email {
        personal: PersonalData.contact
    }

    person: true
}
";

/// A user's todos, deleted with them, and each todo's attachments, which
/// cascade from it; comments kept without their author; and invoices kept
/// as they are until their own retention ends.
const RECORDS: &str = "entity Todo {
    id: Uuid
    owner_id: User.id {
        on_delete: OnDelete.restricted
        erase: Erasure.deleted
    }
    title: Caption
}

entity Attachment {
    id: Uuid
    todo_id: Todo.id {
        on_delete: OnDelete.cascade
    }
    caption: Caption {
        personal: PersonalData.identifying
    }
}

entity Comment {
    id: Uuid
    author_id: User.id? {
        on_delete: OnDelete.cleared
    }
    body: Caption
    signature: Caption? {
        personal: PersonalData.identifying
    }
}

entity Invoice {
    id: Uuid
    customer_id: User.id? {
        on_delete: OnDelete.restricted
        erase: Erasure.retained
    }
    billing_name: Caption {
        personal: PersonalData.identifying
    }
    issued_at: Instant {
        generated: Generated.on_create
    }

    retention: {
        after: config.invoice_retention
        since: issued_at
    }
}
";

/// An action erasing a user.
const FORGET: &str = "action forget(user_id: User.id) fails UserNotFound -> Unit {
    attempt User.erase(user_id)
}
";

fn project(parts: &[&str]) -> String {
    parts.join("\n")
}

#[test]
fn a_person_with_records_of_every_outcome_is_erased_cleanly() {
    assert_eq!(rules(&project(&[USER, RECORDS, FORGET])), CLEAN);
    assert_eq!(rules(&project(&[USER, FORGET])), CLEAN);
}

#[test]
fn a_restricted_reference_into_a_persons_records_says_what_erasure_does() {
    let source = project(&[
        USER,
        &RECORDS.replacen("        erase: Erasure.deleted\n", "", 1),
    ]);
    assert_eq!(
        problems(&source),
        [
            "main.jadpo data.erase_required: `Todo.owner_id` doesn't say what erasing a `User` does to it"
        ]
    );
}

#[test]
fn erasure_reaches_through_what_it_deletes_but_not_past_what_it_keeps() {
    let notes = "entity TodoNote {
    id: Uuid
    todo_id: Todo.id {
        on_delete: OnDelete.restricted
    }
}

entity Reaction {
    id: Uuid
    comment_id: Comment.id {
        on_delete: OnDelete.restricted
    }
}
";
    assert_eq!(
        problems(&project(&[USER, RECORDS, notes])),
        [
            "main.jadpo data.erase_required: `TodoNote.todo_id` doesn't say what erasing a `User` does to it"
        ],
        "a todo is deleted, so its notes need an outcome; a comment is kept, so its reactions stay with it"
    );
    assert_eq!(
        rules(&project(&[
            USER,
            RECORDS,
            &notes.replacen(
                "on_delete: OnDelete.restricted\n",
                "on_delete: OnDelete.restricted\n        erase: Erasure.deleted\n",
                1
            )
        ])),
        CLEAN
    );
}

#[test]
fn a_reference_erasure_clears_can_be_none() {
    let required = RECORDS.replace("customer_id: User.id? {", "customer_id: User.id {");
    assert_eq!(
        problems(&project(&[USER, &required])),
        ["main.jadpo data.erase_field_required: `Invoice.customer_id` can't be cleared"]
    );
    let anonymised = RECORDS
        .replace("customer_id: User.id? {", "customer_id: User.id {")
        .replace("erase: Erasure.retained", "erase: Erasure.anonymised");
    assert_eq!(
        rules(&project(&[USER, &anonymised])),
        ["data.erase_field_required", "data.erase_field_required"],
        "the reference and the marked billing name"
    );
}

#[test]
fn a_marked_field_of_a_record_kept_without_the_person_can_be_none() {
    let required = RECORDS.replace("signature: Caption? {", "signature: Caption {");
    assert_eq!(
        problems(&project(&[USER, &required])),
        ["main.jadpo data.erase_field_required: Erasing a `User` can't clear `Comment.signature`"]
    );
    // Unmarked fields stay as they are: the body isn't personal data.
    assert_eq!(rules(&project(&[USER, RECORDS])), CLEAN);
}

#[test]
fn a_record_kept_as_it_is_has_a_retention_of_its_own() {
    let forever = RECORDS.replace(
        "\n    retention: {\n        after: config.invoice_retention\n        since: issued_at\n    }\n",
        "",
    );
    assert_eq!(
        problems(&project(&[USER, &forever])),
        [
            "main.jadpo data.erase_retention_required: `Invoice` has no retention, so an invoice kept after erasure would never go"
        ]
    );
}

#[test]
fn an_erase_that_erasure_never_reaches_is_refused() {
    let category = "entity Category {
    id: Uuid
}

entity Tag {
    id: Uuid
    category_id: Category.id {
        on_delete: OnDelete.cascade
        erase: Erasure.deleted
    }
    label: Caption {
        erase: Erasure.deleted
    }
}
";
    assert_eq!(
        problems(&project(&[USER, category])),
        [
            "main.jadpo data.erase_unreached: Erasing a person never reaches `Tag.category_id`",
            "main.jadpo data.erase_unreached: `Tag.label` isn't a reference, so `erase:` does nothing",
        ]
    );
    let event = "type TodoAdded = Event {
    owner_id: User.id {
        erase: Erasure.deleted
    }
}
";
    assert_eq!(
        rules(&project(&[USER, event])),
        ["data.erase_unreached"],
        "an event is sent as it was written"
    );
}

#[test]
fn erase_takes_an_erasure() {
    for value in ["true", "Erasure.forgotten", "OnDelete.cascade"] {
        let source = RECORDS.replacen("erase: Erasure.deleted", &format!("erase: {value}"), 1);
        assert_eq!(
            rules(&project(&[USER, &source])),
            ["data.erase_required", "type.setting_value"],
            "{value}"
        );
    }
}

#[test]
fn keeping_the_persons_own_field_is_not_built_yet() {
    let own = USER.replace(
        "    person: true",
        "    nickname: Caption? {\n        erase: Erasure.retained\n    }\n\n    person: true",
    );
    let found = problems(&own);
    assert_eq!(
        found,
        [
            "main.jadpo toolchain.not_built_yet: Keeping `User.nickname` when a `User` is erased isn't built yet"
        ]
    );
}

#[test]
fn erase_is_a_persons_operation() {
    let source = project(&[
        USER,
        RECORDS,
        "action forget_todo(todo_id: Todo.id) -> Unit {\n    Todo.erase(todo_id)\n}\n",
    ]);
    assert_eq!(
        problems(&source),
        ["main.jadpo semantic.unknown_callee: `Todo` has no operation `erase`"]
    );
    // A transition of that name on an entity that is no person is the
    // transition.
    let archived = "entity Note {
    id: Uuid
    gone_at: Instant?

    lifecycle: {
        visible: gone_at == none
        transitions: {
            erase: {
                set: {
                    gone_at: clock.now
                }
            }
        }
    }
}

action drop_note(note_id: Note.id) fails NoteNotFound -> Note {
    return attempt Note.erase(note_id)
}
";
    assert_eq!(rules(&project(&[USER, archived])), CLEAN);
    // On a person, the name is erasure's.
    let person = USER.replace(
        "    person: true",
        "    gone_at: Instant?\n\n    lifecycle: {\n        visible: gone_at == none\n        transitions: {\n            erase: {\n                set: {\n                    gone_at: clock.now\n                }\n            }\n        }\n    }\n\n    person: true",
    );
    assert_eq!(rules(&person), ["data.lifecycle_transition_name"]);
}

#[test]
fn erase_takes_the_persons_id_and_no_block() {
    let with = |body: &str| {
        rules(&project(&[
            USER,
            &format!(
                "action forget(user: User, user_id: User.id) fails UserNotFound -> Unit {{\n{body}}}\n"
            ),
        ]))
    };
    assert_eq!(with("    attempt User.erase(user_id)\n"), CLEAN);
    assert_eq!(with("    attempt User.erase(user)\n"), ["type.mismatch"]);
    assert_eq!(
        with("    attempt User.erase(user_id, user_id)\n"),
        ["type.argument_count"]
    );
    assert_eq!(
        with(
            "    attempt User.erase(user_id) {\n        set: {\n            email: \"ada@example.com\"\n        }\n    }\n"
        ),
        ["data.write_section_unexpected"]
    );
    let found = problems(&project(&[
        USER,
        "action forget(user: User) fails UserNotFound -> Unit {\n    attempt User.erase(user)\n}\n",
    ]));
    assert_eq!(
        found,
        ["main.jadpo type.mismatch: `User.erase` takes a `User.id`, and this is `User`"]
    );
}

#[test]
fn erase_fails_with_the_persons_not_found_failure() {
    let unhandled = project(&[
        USER,
        "action forget(user_id: User.id) -> Unit {\n    User.erase(user_id)\n}\n",
    ]);
    assert!(
        !rules(&unhandled).is_empty(),
        "a failure nothing handles is reported"
    );
}

#[test]
fn a_test_erases_through_an_action() {
    let found = support::problems(
        Layout::Loose,
        &[
            ("main.jadpo", &with_settings(&project(&[USER, FORGET]))),
            (
                "main.test.jadpo",
                "test \"a user is erased\" {\n    attempt User.erase(\"0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e\")\n}\n",
            ),
        ],
    );
    assert!(
        found.iter().any(
            |problem| problem.contains("semantic.not_callable: A test can't call `User.erase`")
        ),
        "{found:?}"
    );
}

#[test]
fn erasing_a_file_or_another_persons_own_record_is_built() {
    let app = "application {\n    connections: {\n        uploads: {\n            driver: Driver.disk\n            path: \"/var/lib/todos\"\n        }\n    }\n}\n";
    let scans = "entity Scan {
    id: Uuid
    todo_id: Todo.id {
        on_delete: OnDelete.cascade
    }
    file: File {
        store: uploads
        max_size: 5mb
        types: [FileType.pdf]
    }
}

entity Photo {
    id: Uuid
    taken_by_id: User.id? {
        on_delete: OnDelete.cleared
    }
    picture: File? {
        store: uploads
        max_size: 5mb
        types: [FileType.png]
        personal: PersonalData.identifying
    }
}
";
    let found = support::rules(
        Layout::Loose,
        &[
            ("app.jadpo", app),
            (
                "main.jadpo",
                &with_settings(&project(&[USER, RECORDS, scans, FORGET])),
            ),
        ],
    );
    assert_eq!(found, CLEAN, "a deleted record's file and a cleared one");
    let invited = USER.replace(
        "    person: true",
        "    invited_by_id: User.id? {\n        on_delete: OnDelete.cleared\n    }\n\n    person: true",
    );
    assert_eq!(rules(&project(&[&invited, FORGET])), CLEAN);
    let kept = invited.replace(
        "on_delete: OnDelete.cleared",
        "on_delete: OnDelete.restricted\n        erase: Erasure.deleted",
    );
    assert_eq!(rules(&project(&[&kept, FORGET])), CLEAN);
}

/// Records a user shares with others: a letter between two users, and a
/// reply on someone's todo.
const SHARED: &str = "entity Letter {
    id: Uuid
    from_id: User.id? {
        on_delete: OnDelete.cleared
    }
    to_id: User.id? {
        on_delete: OnDelete.cleared
    }
    signed_as: Caption? {
        personal: PersonalData.identifying
    }
}

entity Reply {
    id: Uuid
    todo_id: Todo.id? {
        on_delete: OnDelete.cleared
    }
    author_id: User.id? {
        on_delete: OnDelete.cleared
    }
    signed_as: Caption? {
        personal: PersonalData.identifying
    }
}
";

/// A record that belongs to several people keeps its marked fields while
/// any other of them remains (area 15's notes, "Records that belong to
/// more than one person"): erasure looks for another person through each
/// of the record's references that lead to one, climbing through the
/// records they refer to. Another person's own record loses only the
/// reference.
#[test]
fn a_record_belonging_to_several_people_is_cleared_only_by_the_last() {
    let invited = USER.replace(
        "    person: true",
        "    invited_by_id: User.id? {\n        on_delete: OnDelete.cleared\n    }\n\n    person: true",
    );
    let source = with_settings(&project(&[&invited, RECORDS, SHARED, FORGET]));
    assert_eq!(
        problems(&project(&[&invited, RECORDS, SHARED, FORGET])),
        CLEAN
    );
    let db = Database::default();
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", &source)]);
    let erasures = jadpo_check::erasure::erasures(&db, project);
    let user = &erasures.people["User"];
    let kept = |name: &str| {
        user.kept
            .iter()
            .find(|kept| kept.entity == name)
            .unwrap_or_else(|| panic!("{name} kept"))
    };
    // Another person's own record loses only the reference.
    assert!(kept("User").clears.is_empty(), "{:?}", kept("User"));
    assert!(kept("User").through.is_empty());
    // A comment belongs to its author alone, so it is cleared with them.
    assert_eq!(kept("Comment").clears, ["signature"]);
    assert!(kept("Comment").through.is_empty());
    // A letter belongs to both its users.
    let message = kept("Letter");
    assert_eq!(message.clears, ["signed_as"]);
    let through: Vec<(&str, &str)> = message
        .through
        .iter()
        .map(|(field, at)| (field.as_str(), message.chain[*at].entity.as_str()))
        .collect();
    assert_eq!(through, [("from_id", "User"), ("to_id", "User")]);
    // A reply belongs to its author and to the todo's owner, found by
    // climbing from the todo to its owner.
    let reply = kept("Reply");
    assert_eq!(reply.clears, ["signed_as"]);
    let owner = |at: usize| &reply.chain[at];
    let through: Vec<(&str, &str)> = reply
        .through
        .iter()
        .map(|(field, at)| (field.as_str(), owner(*at).entity.as_str()))
        .collect();
    assert_eq!(through, [("todo_id", "Todo"), ("author_id", "User")]);
    let todo = reply.through[0].1;
    assert!(!owner(todo).person);
    let climbs: Vec<(&str, &str)> = owner(todo)
        .climbs
        .iter()
        .map(|(field, at)| (field.as_str(), owner(*at).entity.as_str()))
        .collect();
    assert_eq!(climbs, [("owner_id", "User")]);
    let user_at = reply.through[1].1;
    assert!(owner(user_at).person);
    assert!(owner(user_at).climbs.is_empty(), "a person ends the search");
}

#[test]
fn an_action_erasing_a_person_runs_at_repeatable_read() {
    let db = Database::default();
    let source = with_settings(&project(&[USER, RECORDS, FORGET]));
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", &source)]);
    let decided = jadpo_check::types::isolation(&db, project);
    let forget = decided
        .iter()
        .find(|(key, _)| key.name() == "forget")
        .map(|(_, decided)| decided)
        .expect("forget's isolation");
    assert_eq!(forget.isolation, jadpo_std::Isolation::RepeatableRead);
    assert_eq!(
        forget.because,
        Some(jadpo_check::types::Loads::Erases("User".to_string()))
    );
}
