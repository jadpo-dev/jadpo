//! Area 15, "Retention" (chunk P2): `retention: { after: since: }` on a
//! nullable field clears its value when its time is up, and on an entity
//! deletes the record with what cascades from it. The period is a
//! `Duration` setting, `since:` one of the record's `Instant` fields, and
//! the entity's built-in purge schedule applies it. What retention can't
//! do is refused where it is written: a required field, a field the
//! lifecycle owns or reads, a record a `restricted` reference holds, a
//! `File`'s bytes and a cascade to someone who signs in. An event's
//! `retention:` stays `toolchain.not_built_yet`.

use jadpo_db::{Database, Layout};
use jadpo_graph::{Detail, Key, Kind, Status};

use crate::support;

/// The rules of each problem of `source` alone, after the settings, in
/// order.
fn rules(source: &str) -> Vec<String> {
    support::rules(Layout::Loose, &[("main.jadpo", &with_settings(source))])
}

/// The problems of `source` alone, after the settings, as `rule: message`.
fn problems(source: &str) -> Vec<String> {
    support::problems(Layout::Loose, &[("main.jadpo", &with_settings(source))])
        .into_iter()
        .map(|problem| problem.trim_start().to_string())
        .collect()
}

/// `source` after the settings the retentions read.
fn with_settings(source: &str) -> String {
    format!("{SETTINGS}\n{source}")
}

const SETTINGS: &str = "config {
    signup_ip_retention: Duration {
        default: 30d
    }
    invoice_retention: Duration {
        default: 60d
    }
    label: Text {
        default: \"x\"
    }
    token: Text {
        secret: true
    }
}
";

const CLEAN: [&str; 0] = [];

/// A visitor whose sign-up address is cleared 30 days after they joined.
const VISITOR: &str = "entity Visitor {
    id: Uuid
    signup_ip: IpAddress? {
        retention: {
            after: config.signup_ip_retention
            since: created_at
        }
    }
    created_at: Instant {
        generated: Generated.on_create
    }
}
";

/// An invoice deleted 60 days after it was issued, with its lines.
const INVOICE: &str = "type Note = Text {
    max_length: 80
}

entity Invoice {
    id: Uuid
    issued_at: Instant

    retention: {
        after: config.invoice_retention
        since: issued_at
    }
}

entity InvoiceLine {
    id: Uuid
    invoice_id: Invoice.id {
        on_delete: OnDelete.cascade
    }
    note: Note
}
";

#[test]
fn a_fields_retention_and_an_entitys_are_accepted() {
    assert_eq!(rules(VISITOR), CLEAN);
    assert_eq!(rules(INVOICE), CLEAN);
}

#[test]
fn a_field_may_still_be_named_retention() {
    assert_eq!(
        rules("type Days = Int {}\n\nentity Policy {\n    id: Uuid\n    retention: Days?\n}\n"),
        CLEAN
    );
}

#[test]
fn the_period_is_a_duration_setting() {
    let with = |after: &str| rules(&VISITOR.replace("config.signup_ip_retention", after));
    assert_eq!(with("30d"), ["data.retention_period"]);
    assert_eq!(with("config.label"), ["data.retention_period"]);
    assert_eq!(with("signup_ip"), ["data.retention_period"]);
    assert_eq!(with("config.token"), ["configuration.secret_flow"]);
    assert_eq!(
        with("config.signup_ip_retentoin"),
        ["configuration.field_unknown"]
    );
    let found = problems(&VISITOR.replace("config.signup_ip_retention", "30d"));
    assert_eq!(
        found,
        ["main.jadpo data.retention_period: `after: 30d` is written in place"]
    );
}

#[test]
fn since_is_an_instant_field_of_the_record() {
    let with = |since: &str| rules(&VISITOR.replace("since: created_at", since));
    assert_eq!(with("since: signup_ip"), ["data.retention_since"]);
    assert_eq!(with("since: created"), ["data.retention_since"]);
    assert_eq!(with("since: Visitor.created_at"), ["data.retention_since"]);
    assert_eq!(with(""), ["data.retention_since"]);
    // A nullable time counts from when it is set.
    let nullable = VISITOR.replace(
        "    created_at: Instant {\n        generated: Generated.on_create\n    }\n",
        "    created_at: Instant?\n",
    );
    assert_eq!(rules(&nullable), CLEAN);
}

#[test]
fn a_retention_has_after_and_since_only() {
    assert_eq!(
        rules(&VISITOR.replace("since: created_at", "")),
        ["data.retention_since"]
    );
    assert_eq!(
        rules(&VISITOR.replace("after: config.signup_ip_retention", "")),
        ["data.retention_period"]
    );
    assert_eq!(
        rules(&VISITOR.replace(
            "since: created_at",
            "since: created_at\n            until: created_at"
        )),
        ["type.unknown_setting"]
    );
    assert_eq!(
        rules(&VISITOR.replace(
            "since: created_at",
            "since: created_at\n            since: created_at"
        )),
        ["type.constraint_repeated"]
    );
    assert_eq!(
        rules(&INVOICE.replace(
            "    retention: {\n        after: config.invoice_retention\n        since: issued_at\n    }\n",
            "    retention: config.invoice_retention\n",
        )),
        ["type.setting_value"]
    );
}

#[test]
fn a_fields_retention_needs_a_field_that_can_be_none() {
    let found = problems(&VISITOR.replace("IpAddress?", "IpAddress"));
    assert_eq!(
        found,
        ["main.jadpo data.retention_field_required: `Visitor.signup_ip` can't be cleared"]
    );
}

/// A todo whose lifecycle hides it once deleted and archives it.
const TODO: &str = "entity Todo {
    id: Uuid
    deleted_at: Instant?
    archived_at: Instant?
    created_at: Instant {
        generated: Generated.on_create
    }

    lifecycle: {
        visible: deleted_at == none
        transitions: {
            delete: {
                when: deleted_at == none
                set: {
                    deleted_at: clock.now
                }
            }
            archive: {
                when: archived_at == none
                set: {
                    archived_at: clock.now
                }
            }
        }
    }
}
";

#[test]
fn a_fields_retention_leaves_the_lifecycles_fields_alone() {
    let todo = |field: &str| {
        TODO
        .replace(
            &format!("    {field}: Instant?\n"),
            &format!(
                "    {field}: Instant? {{\n        retention: {{\n            after: config.signup_ip_retention\n            since: created_at\n        }}\n    }}\n"
            ),
        )
    };
    // `visible:` reads it, and a transition sets it.
    assert_eq!(
        rules(&todo("deleted_at")),
        ["data.retention_lifecycle_field"]
    );
    // A transition sets it, and its `when:` reads it.
    assert_eq!(
        rules(&todo("archived_at")),
        ["data.retention_lifecycle_field"]
    );
    let found = problems(&todo("deleted_at"));
    assert_eq!(
        found,
        [
            "main.jadpo data.retention_lifecycle_field: Clearing `Todo.deleted_at` would change its lifecycle"
        ]
    );
}

#[test]
fn an_entitys_retention_deletes_nothing_a_restricted_reference_holds() {
    let held = INVOICE.replace(
        "        on_delete: OnDelete.cascade\n",
        "        on_delete: OnDelete.restricted\n",
    );
    let found = problems(&held);
    assert_eq!(
        found,
        [
            "main.jadpo data.retention_held: `Invoice`'s retention can't delete an invoice an `InvoiceLine` holds"
        ]
    );
    // Through a cascade: deleting an invoice deletes its lines, which a
    // refund holds.
    let through = format!(
        "{INVOICE}\nentity Refund {{\n    id: Uuid\n    line_id: InvoiceLine.id {{\n        on_delete: OnDelete.restricted\n    }}\n}}\n"
    );
    assert_eq!(rules(&through), ["data.retention_held"]);
    // A cleared reference keeps the refund without its line.
    let cleared = through
        .replace("line_id: InvoiceLine.id {", "line_id: InvoiceLine.id? {")
        .replace(
            "        on_delete: OnDelete.restricted\n    }\n}\n",
            "        on_delete: OnDelete.cleared\n    }\n}\n",
        );
    assert_eq!(rules(&cleared), CLEAN);
    // An invoice that replaces another holds it.
    let replaces = INVOICE.replace(
        "    issued_at: Instant\n",
        "    issued_at: Instant\n    replaces: Invoice.id? {\n        on_delete: OnDelete.restricted\n    }\n",
    );
    assert_eq!(rules(&replaces), ["data.retention_held"]);
}

#[test]
fn retention_that_would_leave_a_files_bytes_is_not_built_yet() {
    let app = "application {\n    connections: {\n        uploads: {\n            driver: Driver.disk\n            path: \"/var/lib/claims\"\n        }\n    }\n}\n";
    let claim = "entity Claim {
    id: Uuid
    created_at: Instant {
        generated: Generated.on_create
    }
    scan: File? {
        store: uploads
        max_size: 5mb
        types: [FileType.pdf]
        retention: {
            after: config.signup_ip_retention
            since: created_at
        }
    }
}
";
    let found = support::problems(
        Layout::Loose,
        &[("app.jadpo", app), ("main.jadpo", &with_settings(claim))],
    );
    assert!(
        found.len() == 1 && found[0].contains("main.jadpo toolchain.not_built_yet:"),
        "{found:?}"
    );
    let whole = claim
        .replace(
            "        retention: {\n            after: config.signup_ip_retention\n            since: created_at\n        }\n",
            "",
        )
        .replace(
            "    }\n}\n",
            "    }\n\n    retention: {\n        after: config.invoice_retention\n        since: created_at\n    }\n}\n",
        );
    let found = support::problems(
        Layout::Loose,
        &[("app.jadpo", app), ("main.jadpo", &with_settings(&whole))],
    );
    assert!(
        found.len() == 1 && found[0].contains("main.jadpo toolchain.not_built_yet:"),
        "{found:?}"
    );
}

/// A club whose members sign in, and which closes after a while.
const CLUB: &str = "type Handle = Text {
    max_length: 40
}

principal {
    user: {
        subject: Member.handle
    }
}

authentication {
    api_token: {}
}

entity Club {
    id: Uuid
    opened_at: Instant
}

entity Member {
    id: Uuid
    handle: Handle {
        unique: true
    }
    club_id: Club.id {
        on_delete: OnDelete.cascade
    }
    joined_at: Instant
}
";

#[test]
fn a_person_who_signs_in_is_deleted_with_their_sign_in_state_but_not_through_a_cascade() {
    // Members who sign in go when their time is up, their sessions,
    // passkeys and sign-in links with them.
    let own = CLUB.replace(
        "    joined_at: Instant\n",
        "    joined_at: Instant\n\n    retention: {\n        after: config.invoice_retention\n        since: joined_at\n    }\n",
    );
    assert_eq!(rules(&own), CLEAN);
    // Deleting a club would delete its members through the cascade, and
    // their sign-in state with nothing.
    let club = CLUB.replace(
        "    opened_at: Instant\n",
        "    opened_at: Instant\n\n    retention: {\n        after: config.invoice_retention\n        since: opened_at\n    }\n",
    );
    let found = problems(&club);
    assert!(
        found.len() == 1 && found[0].starts_with("main.jadpo toolchain.not_built_yet:"),
        "{found:?}"
    );
}

#[test]
fn an_events_retention_is_still_not_built_yet() {
    let found = problems(
        "type VisitorArrived = Event {\n    ip: IpAddress? {\n        retention: {\n            after: config.signup_ip_retention\n            since: at\n        }\n    }\n}\n",
    );
    assert!(
        found.len() == 1 && found[0].starts_with("main.jadpo toolchain.not_built_yet:"),
        "{found:?}"
    );
}

#[test]
fn a_retention_gives_the_entity_its_purge_schedule() {
    let db = Database::default();
    let project = support::project(
        &db,
        Layout::Loose,
        &[(
            "main.jadpo",
            &with_settings(&format!("{VISITOR}\n{INVOICE}")),
        )],
    );
    let found = jadpo_check::check(&db, project);
    assert!(found.is_empty(), "{found:?}");
    let names: Vec<String> = jadpo_check::schedules::schedules(&db, project)
        .iter()
        .map(|schedule| schedule.name.clone())
        .collect();
    assert_eq!(names, ["Invoice.purge", "Visitor.purge"]);
}

#[test]
fn the_retention_kind_is_built() {
    assert_eq!(Kind::Retention.status(), Status::Built);
}

#[test]
fn each_retention_is_a_node_under_what_it_keeps() {
    let db = Database::default();
    let project = support::project(
        &db,
        Layout::Loose,
        &[(
            "main.jadpo",
            &with_settings(&format!("{VISITOR}\n{INVOICE}")),
        )],
    );
    let found = jadpo_check::check(&db, project);
    assert!(found.is_empty(), "{found:?}");
    let graph = jadpo_check::graph::graph(&db, project);
    let node = |key: &str| {
        let id = graph
            .find(&Key::from_text(key.to_string()))
            .unwrap_or_else(|| panic!("{key} is missing"));
        graph.node(id).clone()
    };
    let field = node("retention Visitor.signup_ip");
    assert_eq!(
        field.detail,
        Detail::Retention {
            retention: "after config.signup_ip_retention since created_at".to_string(),
            clears: true,
        }
    );
    assert_eq!(
        field.parent.as_ref().map(Key::as_str),
        Some("field Visitor.signup_ip")
    );
    let entity = node("retention Invoice");
    assert_eq!(
        entity.detail,
        Detail::Retention {
            retention: "after config.invoice_retention since issued_at".to_string(),
            clears: false,
        }
    );
    assert_eq!(
        entity.parent.as_ref().map(Key::as_str),
        Some("entity Invoice")
    );
    assert!(entity.place.is_some());
}
