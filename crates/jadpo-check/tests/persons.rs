//! Area 15, "The person" and "What the compiler checks" (chunk P1): an
//! entity marked `person: true` is a person, a record that refers to one,
//! directly or through a chain of references, belongs to them, and a field
//! marked `personal:` on an entity that is no person and belongs to none is
//! `data.personal_unreachable`, since erasing a person could never find it.
//! The graph holds a `person` node for each person entity, listing the
//! references its records belong through.

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

/// A user's todos and their attachments: an attachment belongs to the
/// user through its todo.
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
        Detail::Person { through } => through.clone(),
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
fn records_belong_through_a_chain_of_references() {
    let source = format!("{USER}\n{TODOS}");
    assert_eq!(problems(&source), NONE);
    let (node, through) = person(&source, "User");
    assert_eq!(through, ["Attachment.todo_id", "Todo.owner_id"]);
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

#[test]
fn a_chain_that_comes_back_to_its_own_entity_ends() {
    // A folder inside a folder belongs to the user who owns the outer one,
    // and a user who invited another is one of their records too.
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
    assert_eq!(
        through,
        ["Folder.owner_id", "Folder.parent_id", "User.invited_by"]
    );
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
