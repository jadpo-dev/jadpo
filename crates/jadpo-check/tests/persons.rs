//! Area 15, "The person" and "What the compiler checks" (chunks P1 and
//! P4b): an entity marked `person: true` is a person, a record that refers
//! to one directly belongs to them, and a field marked `personal:` on an
//! entity erasing a person never reaches is `data.personal_unreachable`.
//! The graph holds a `person` node for each person entity, listing the
//! references its records belong through, and what erasure keeps of it.

use jadpo_db::{Database, Layout};
use jadpo_graph::{Detail, Key, Kind, Status};

use crate::support;

/// The problems of `source` alone, as `path rule: message`.
fn problems(source: &str) -> Vec<String> {
    support::problems(Layout::Loose, &[("main.jadpo", source)])
        .into_iter()
        .map(|problem| problem.trim_start().to_string())
        .collect()
}

const NONE: [&str; 0] = [];

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

/// A user's todos and their attachments: a todo belongs to the user, and
/// an attachment, which refers to no person, is deleted with its todo.
const TODOS: &str = "entity Todo {
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
";

/// The `person` node of `entity` in the graph of `source`, with the
/// references its records belong through.
fn person(source: &str, entity: &str) -> (jadpo_graph::Node, Vec<String>) {
    let db = Database::default();
    let project = support::project(&db, Layout::Loose, &[("main.jadpo", source)]);
    let found = jadpo_check::check(&db, project);
    assert!(found.is_empty(), "{found:?}");
    let graph = jadpo_check::graph::graph(&db, project);
    let id = graph
        .find(&Key::from_text(format!("person {entity}")))
        .unwrap_or_else(|| panic!("person {entity} is missing"));
    let node = graph.node(id).clone();
    let through = match &node.detail {
        Detail::Person { through, .. } => through.clone(),
        other => panic!("{other:?}"),
    };
    (node, through)
}

#[test]
fn the_person_kind_is_built() {
    assert_eq!(Kind::Person.status(), Status::Built);
}

#[test]
fn a_person_entity_is_accepted() {
    assert_eq!(problems(USER), NONE);
}

#[test]
fn person_takes_only_true() {
    for written in ["false", "1", "\"yes\""] {
        let source = USER.replace("person: true", &format!("person: {written}"));
        assert_eq!(
            problems(&source),
            ["main.jadpo type.setting_value: `person:` takes only `true`"],
            "{written}"
        );
    }
}

#[test]
fn a_marked_field_on_no_person_is_refused() {
    assert_eq!(
        problems(
            "entity Visitor {
    id: Uuid
    email: Email {
        personal: PersonalData.contact
    }
}
"
        ),
        ["main.jadpo data.personal_unreachable: Erasing a person can't find `Visitor.email`"]
    );
}

#[test]
fn a_record_belongs_only_to_the_people_it_refers_to() {
    // An attachment's caption is found by erasure, which deletes it with
    // its todo, but it doesn't belong to the todo's owner.
    let source = format!("{USER}\n{TODOS}");
    assert_eq!(problems(&source), NONE);
    let (node, through) = person(&source, "User");
    assert_eq!(through, ["Todo.owner_id"]);
    assert_eq!(
        node.parent.as_ref().map(|key| key.to_string()).as_deref(),
        Some("entity User")
    );
    let place = node.place.expect("placed at `person: true`");
    assert_eq!(place.file, "main.jadpo");
    assert_eq!(&source[place.start as usize..place.end as usize], "person");
    assert!(node.digest.is_some());
}

#[test]
fn a_record_reaching_no_person_is_refused_whatever_it_refers_to() {
    // An order refers to its shop, which is no person.
    assert_eq!(
        problems(&format!(
            "{USER}
entity Shop {{
    id: Uuid
}}

entity Order {{
    id: Uuid
    shop_id: Shop.id {{
        on_delete: OnDelete.restricted
    }}
    buyer_name: Caption {{
        personal: PersonalData.identifying
    }}
}}
"
        )),
        ["main.jadpo data.personal_unreachable: Erasing a person can't find `Order.buyer_name`"]
    );
}

#[test]
fn a_record_that_refers_to_two_people_belongs_to_both() {
    let source = format!(
        "{USER}
entity Patient {{
    id: Uuid
    patient_name: Caption {{
        personal: PersonalData.identifying
    }}
    person: true
}}

entity DirectMessage {{
    id: Uuid
    from_id: User.id {{
        on_delete: OnDelete.cascade
    }}
    to_id: User.id {{
        on_delete: OnDelete.cascade
    }}
    body: Caption {{
        personal: PersonalData.identifying
    }}
}}

entity Appointment {{
    id: Uuid
    patient_id: Patient.id {{
        on_delete: OnDelete.cascade
    }}
    booked_by: User.id? {{
        on_delete: OnDelete.cleared
    }}
    reason: Caption? {{
        personal: PersonalData.health
    }}
}}
"
    );
    let (_, user) = person(&source, "User");
    assert_eq!(
        user,
        [
            "Appointment.booked_by",
            "DirectMessage.from_id",
            "DirectMessage.to_id"
        ]
    );
    let (_, patient) = person(&source, "Patient");
    assert_eq!(patient, ["Appointment.patient_id"]);
}

/// Erasure walks on only through what it deletes, so a marked field past
/// a record it keeps is out of reach, whatever the references say.
#[test]
fn a_marked_field_past_a_record_erasure_keeps_is_refused() {
    let source = format!(
        "{USER}
entity Comment {{
    id: Uuid
    author_id: User.id? {{
        on_delete: OnDelete.cleared
    }}
}}

entity Reaction {{
    id: Uuid
    comment_id: Comment.id? {{
        on_delete: OnDelete.cleared
    }}
    signed_as: Caption? {{
        personal: PersonalData.identifying
    }}
}}
"
    );
    assert_eq!(
        problems(&source),
        ["main.jadpo data.personal_unreachable: Erasing a person can't find `Reaction.signed_as`"]
    );
}

/// A marked field on a record kept when something it refers to is erased
/// is found: the record refers to no person, and loses its marked fields
/// when erasure deletes what it refers to.
#[test]
fn a_marked_field_beside_a_reference_erasure_clears_is_reached() {
    let source = format!(
        "{USER}\n{TODOS}
entity GuestNote {{
    id: Uuid
    todo_id: Todo.id? {{
        on_delete: OnDelete.cleared
    }}
    signed_as: Caption? {{
        personal: PersonalData.identifying
    }}
}}
"
    );
    assert_eq!(problems(&source), NONE);
}

#[test]
fn a_reference_to_another_of_their_records_is_no_way_of_belonging() {
    // A folder inside a folder belongs to its own owner, and a user someone
    // invited refers to them directly.
    let source = format!(
        "{}
entity Folder {{
    id: Uuid
    owner_id: User.id {{
        on_delete: OnDelete.cascade
    }}
    parent_id: Folder.id? {{
        on_delete: OnDelete.cleared
    }}
    label: Caption? {{
        personal: PersonalData.identifying
    }}
}}
",
        USER.replace(
            "    person: true",
            "    invited_by: User.id? {\n        on_delete: OnDelete.cleared\n    }\n    person: true"
        )
    );
    let (_, through) = person(&source, "User");
    assert_eq!(through, ["Folder.owner_id", "User.invited_by"]);
}

#[test]
fn one_problem_names_every_marked_field_of_an_entity() {
    let db = Database::default();
    let project = support::project(
        &db,
        Layout::Loose,
        &[(
            "main.jadpo",
            "type Caption = Text {
    max_length: 200
}

entity LossReport {
    id: Uuid
    passenger_name: Caption {
        personal: PersonalData.identifying
    }
    what_was_lost: Caption
    passenger_email: Email {
        personal: PersonalData.contact
    }
}
",
        )],
    );
    let found = jadpo_check::check(&db, project);
    assert_eq!(found.len(), 1, "{found:?}");
    let problem = &found[0];
    assert_eq!(problem.rule.id, "data.personal_unreachable");
    assert_eq!(
        problem.message,
        "Erasing a person can't find `LossReport.passenger_name`"
    );
    let text = |span: &jadpo_diag::Span| {
        let source = project.files(&db)[0].text(&db);
        source[span.start as usize..span.end as usize].to_string()
    };
    let primary = problem.primary.as_ref().expect("a primary label");
    assert_eq!(text(&primary.span), "passenger_name");
    let also: Vec<String> = problem
        .secondary
        .iter()
        .map(|label| text(&label.span))
        .collect();
    assert_eq!(also, ["passenger_email"]);
    assert!(
        problem
            .next
            .as_ref()
            .is_some_and(|next| next.text.contains("`person: true`")),
        "{problem:?}"
    );
}

#[test]
fn only_a_personal_setting_marks_a_field() {
    // A standard type that is personal data whatever holds it (area 19)
    // is hashed in logs and its reads recorded, but a shop's own phone
    // number and its manager's name don't make it a person.
    assert_eq!(
        problems(
            "entity Shop {
    id: Uuid
    phone: PhoneNumber
    manager: PersonName
}
"
        ),
        NONE
    );
}

#[test]
fn an_events_marked_field_needs_no_person() {
    assert_eq!(
        problems(
            "type VisitorSignedUp = Event {
    email: Email {
        personal: PersonalData.contact
    }
}
"
        ),
        NONE
    );
}

/// Erasure is built (chunk P3, `tests/erasure.rs`); export isn't yet.
#[test]
fn export_is_not_built_yet() {
    let source = format!(
        "{USER}
action forget(user_id: User.id) -> Unit {{
    User.export(user_id)
}}
"
    );
    assert_eq!(
        problems(&source),
        ["main.jadpo toolchain.not_built_yet: `User.export` isn't built yet"]
    );
}
