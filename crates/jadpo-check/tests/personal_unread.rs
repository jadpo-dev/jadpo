//! Area 15, "What the compiler checks": a field marked `personal:` that no
//! query, output or action reads is `data.personal_unread`, declinable with
//! a reason.

use jadpo_db::Layout;

use crate::support;

/// The problems of `source`, as `path rule: message`.
fn problems(source: &str) -> Vec<String> {
    support::every_problem(Layout::Loose, &[("main.jadpo", source)])
        .into_iter()
        .map(|problem| problem.trim_start().to_string())
        .collect()
}

const NONE: [&str; 0] = [];

const USER: &str = "type Note = Text {
    max_length: 200
}

entity User {
    id: Uuid
    medical_note: Note? {
        personal: PersonalData.health
    }
    person: true
}
";

#[test]
fn a_marked_field_nothing_reads_is_an_error() {
    assert_eq!(
        problems(USER),
        [
            "main.jadpo data.personal_unread: Nothing reads `User.medical_note`, which is personal data"
        ]
    );
}

#[test]
fn a_decline_with_a_reason_keeps_it() {
    let source = USER.replace(
        "    medical_note: Note?",
        "    decline data.personal_unread: \"Kept for the regulator, read only by an export\"\n    medical_note: Note?",
    );
    assert_eq!(problems(&source), NONE);
}

#[test]
fn an_action_reading_the_field_uses_it() {
    let source = format!(
        "{USER}
/// Whether someone has a note.
action noted(user: User.id) fails UserNotFound -> Note? {{
    var person: User = attempt User.get(user)
    return person.medical_note
}}
"
    );
    assert_eq!(problems(&source), NONE);
}

#[test]
fn a_query_reading_the_field_uses_it() {
    let source = format!(
        "{USER}
/// A person's note.
type NoteLine = Output {{
    medical_note: Note?
}}

/// Each note.
query notes() -> List<NoteLine> {{
    from: User
    order: [User.id asc]
    limit: 20
}}
"
    );
    assert_eq!(problems(&source), NONE);
}

#[test]
fn a_record_given_back_whole_uses_it() {
    let source = format!(
        "{USER}
/// A person's whole record.
action whole(user: User.id) fails UserNotFound -> User {{
    return attempt User.get(user)
}}
"
    );
    assert_eq!(problems(&source), NONE);
}

/// A test reading a field doesn't make the application use it.
#[test]
fn a_test_reading_it_doesnt_count() {
    let source = format!(
        "{USER}
fixture someone() -> User {{
    return User.create {{
        set: {{
            medical_note: none
        }}
    }}
}}

test \"holds a note\" {{
    var person: User = someone()
    assert person.medical_note == none
}}
"
    );
    assert_eq!(
        problems(&source),
        [
            "main.jadpo data.personal_unread: Nothing reads `User.medical_note`, which is personal data"
        ]
    );
}

/// A standard type that is personal data with no mark isn't held to this:
/// only `personal:` marks a field here.
#[test]
fn a_standard_type_with_no_mark_is_not_counted() {
    let source = "entity User {
    id: Uuid
    email: Email
    person: true
}
";
    assert_eq!(problems(source), NONE);
}
