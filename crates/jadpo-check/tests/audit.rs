//! Area 16, "Audit trail" (chunk B11): `audit: { retention: { after: } }`
//! on an entity keeps a history of every change to its records, in a
//! `jadpo_audit` table in its own store, read back with the generated
//! `Entity.history(id)`. Its retention is a `Duration` setting counted
//! from each entry's own time, applied by the entity's purge schedule.
//! What the history can't yet keep or remove soundly is
//! `toolchain.not_built_yet` where it is written: a personal value, a
//! `File`, a field with a retention of its own, a person's record that
//! something other than erasure removes, a change a cascade makes, and an
//! erasure that keeps an audited record.

use jadpo_db::{Database, Layout};
use jadpo_graph::{Detail, Key, Kind, Status};

use crate::support;

/// The problems of `source` alone, after the settings, as `rule: message`.
fn problems(source: &str) -> Vec<String> {
    support::problems(Layout::Loose, &[("main.jadpo", &with_settings(source))])
        .into_iter()
        .map(|problem| problem.trim_start().to_string())
        .collect()
}

/// Whether the one problem of `source` is `rule`, its message holding
/// `what`.
fn refused_once(source: &str, rule: &str, what: &str) {
    let found = problems(source);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].starts_with(&format!("main.jadpo {rule}:")) && found[0].contains(what),
        "{found:?}"
    );
}

fn with_settings(source: &str) -> String {
    format!("{SETTINGS}\n{source}")
}

const SETTINGS: &str = "config {
    audit_retention: Duration {
        default: 2557d
    }
    label: Text {
        default: \"x\"
    }
}

type Note = Text {
    max_length: 80
}
";

/// An invoice whose every change is kept seven years.
const INVOICE: &str = "entity Invoice {
    id: Uuid
    note: Note?

    policy: {
        create: [Access.public]
        read: [Access.public]
        update: [Access.public]
        delete: [Access.public]
    }

    audit: {
        retention: {
            after: config.audit_retention
        }
    }
}
";

#[test]
fn an_audited_entity_checks_and_keeps_its_history_beside_it() {
    let db = Database::default();
    let project = support::project(
        &db,
        Layout::Loose,
        &[("main.jadpo", &with_settings(INVOICE))],
    );
    let found = jadpo_check::check(&db, project);
    assert!(found.is_empty(), "{found:?}");
    let schema = jadpo_check::data::schema(&db, project);
    let main = schema.stores.get("main").expect("main");
    assert!(
        main.tables.contains_key("jadpo_audit"),
        "{:?}",
        main.tables.keys()
    );
    // Its retention gives it the purge schedule.
    let names: Vec<String> = jadpo_check::schedules::schedules(&db, project)
        .iter()
        .map(|schedule| schedule.name.clone())
        .collect();
    assert_eq!(names, ["Invoice.purge"]);
}

#[test]
fn an_audit_without_retention_keeps_entries_until_erasure_and_needs_no_schedule() {
    let db = Database::default();
    let source = INVOICE.replace(
        "audit: {\n        retention: {\n            after: config.audit_retention\n        }\n    }",
        "audit: {}",
    );
    let project = support::project(
        &db,
        Layout::Loose,
        &[("main.jadpo", &with_settings(&source))],
    );
    let found = jadpo_check::check(&db, project);
    assert!(found.is_empty(), "{found:?}");
    assert!(jadpo_check::schedules::schedules(&db, project).is_empty());
}

#[test]
fn an_unaudited_store_has_no_history_table() {
    let db = Database::default();
    let source = INVOICE.replace(
        "    audit: {\n        retention: {\n            after: config.audit_retention\n        }\n    }\n",
        "",
    );
    let project = support::project(
        &db,
        Layout::Loose,
        &[("main.jadpo", &with_settings(&source))],
    );
    let schema = jadpo_check::data::schema(&db, project);
    let main = schema.stores.get("main").expect("main");
    assert!(!main.tables.contains_key("jadpo_audit"));
}

#[test]
fn the_audit_kind_is_built_and_its_node_holds_how_long_entries_are_kept() {
    assert_eq!(Kind::Audit.status(), Status::Built);
    let db = Database::default();
    let project = support::project(
        &db,
        Layout::Loose,
        &[("main.jadpo", &with_settings(INVOICE))],
    );
    let graph = jadpo_check::graph::graph(&db, project);
    let node = graph
        .nodes()
        .iter()
        .find(|node| node.key == Key::new(Kind::Audit, "Invoice"))
        .expect("the audit node");
    assert_eq!(node.parent, Some(Key::new(Kind::Entity, "Invoice")));
    assert_eq!(
        node.detail,
        Detail::Audit {
            kept: "kept after config.audit_retention".to_string()
        }
    );
}

#[test]
fn audit_takes_a_block() {
    refused_once(
        &INVOICE.replace(
            "audit: {\n        retention: {\n            after: config.audit_retention\n        }\n    }",
            "audit: true",
        ),
        "type.setting_value",
        "`audit:` takes",
    );
}

#[test]
fn audit_takes_only_a_retention() {
    refused_once(
        &INVOICE.replace(
            "        retention: {\n            after",
            "        retain: {\n            after",
        ),
        "type.unknown_setting",
        "`retain` isn't part of `audit:`",
    );
}

#[test]
fn an_audits_retention_counts_from_each_entry() {
    refused_once(
        &INVOICE.replace(
            "            after: config.audit_retention\n",
            "            after: config.audit_retention\n            since: note\n",
        ),
        "type.unknown_setting",
        "counts from each entry's own time",
    );
}

#[test]
fn an_audits_period_is_a_duration_setting() {
    refused_once(
        &INVOICE.replace("after: config.audit_retention", "after: 30d"),
        "data.retention_period",
        "is written in place",
    );
    refused_once(
        &INVOICE.replace("after: config.audit_retention", "after: config.label"),
        "data.retention_period",
        "isn't a duration",
    );
}

#[test]
fn a_personal_value_is_not_kept_in_a_history_yet() {
    refused_once(
        &INVOICE.replace(
            "    note: Note?\n",
            "    note: Note?\n    payer: Note {\n        personal: PersonalData.identifying\n    }\n",
        )
        .replace("    audit: {", "    person: true\n\n    audit: {"),
        "toolchain.not_built_yet",
        "Auditing `Invoice` isn't built yet",
    );
    // A standard type that is personal data whatever holds it, as logs
    // treat it.
    refused_once(
        &INVOICE.replace(
            "    note: Note?\n",
            "    note: Note?\n    payer_phone: PhoneNumber\n",
        ),
        "toolchain.not_built_yet",
        "Auditing `Invoice` isn't built yet",
    );
}

#[test]
fn a_file_or_a_field_with_its_own_retention_is_not_kept_in_a_history_yet() {
    let found = problems(&INVOICE.replace(
        "    note: Note?\n",
        "    note: Note?\n    sent_at: Instant\n    reference: Note? {\n        retention: {\n            after: config.audit_retention\n            since: sent_at\n        }\n    }\n",
    ));
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].starts_with("main.jadpo toolchain.not_built_yet: Auditing `Invoice`"),
        "{found:?}"
    );
}

/// A person, and a note of theirs that is audited.
const PERSON: &str = "entity Owner {
    id: Uuid

    policy: {
        create: [Access.public]
        read: [Access.public]
        delete: [Access.public]
    }

    person: true
}

entity OwnerNote {
    id: Uuid
    owner_id: Owner.id {
        on_delete: OnDelete.cascade
    }
    written_at: Instant

    policy: {
        create: [Access.public]
        read: [Access.public]
        delete: [Access.public]
    }

    audit: {}
}
";

#[test]
fn a_persons_audited_record_erasure_deletes_checks() {
    let found = problems(&format!(
        "{PERSON}
action forget(owner_id: Owner.id) fails OwnerNotFound -> Unit {{
    attempt Owner.erase(owner_id)
}}
"
    ));
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_persons_audited_record_something_else_removes_is_not_built_yet() {
    // Its own retention.
    refused_once(
        &PERSON.replace(
            "    audit: {}\n",
            "    retention: {\n        after: config.audit_retention\n        since: written_at\n    }\n\n    audit: {}\n",
        ),
        "toolchain.not_built_yet",
        "Auditing `OwnerNote` isn't built yet",
    );
    // A delete in code.
    refused_once(
        &format!(
            "{PERSON}
action drop_note(note_id: OwnerNote.id) fails OwnerNoteNotFound -> Unit {{
    attempt OwnerNote.delete(note_id)
}}
"
        ),
        "toolchain.not_built_yet",
        "`OwnerNote.delete` isn't built yet here",
    );
}

#[test]
fn a_delete_whose_cascade_changes_an_audited_record_is_not_built_yet() {
    refused_once(
        &format!(
            "{PERSON}
action drop_owner(owner_id: Owner.id) fails OwnerNotFound -> Unit {{
    attempt Owner.delete(owner_id)
}}
"
        ),
        "toolchain.not_built_yet",
        "`Owner.delete` isn't built yet here",
    );
}

#[test]
fn an_erasure_that_keeps_an_audited_record_is_not_built_yet() {
    let source = PERSON.replace(
        "    owner_id: Owner.id {\n        on_delete: OnDelete.cascade\n    }",
        "    owner_id: Owner.id? {\n        on_delete: OnDelete.cleared\n    }",
    );
    refused_once(
        &format!(
            "{source}
action forget(owner_id: Owner.id) fails OwnerNotFound -> Unit {{
    attempt Owner.erase(owner_id)
}}
"
        ),
        "toolchain.not_built_yet",
        "Erasing a `Owner` here isn't built yet",
    );
}

#[test]
fn history_is_an_audited_entitys_alone() {
    refused_once(
        &format!(
            "{}
action changes(invoice_id: Invoice.id) -> Page<AuditEntry> {{
    return Invoice.history(invoice_id)
}}
",
            INVOICE.replace(
                "    audit: {\n        retention: {\n            after: config.audit_retention\n        }\n    }\n",
                ""
            )
        ),
        "semantic.unknown_callee",
        "`Invoice` keeps no history",
    );
}

#[test]
fn history_takes_an_id_and_after() {
    let found = problems(&format!(
        "{INVOICE}
action changes(invoice_id: Invoice.id, after: PageCursor?) -> Page<AuditEntry> {{
    return Invoice.history(invoice_id, after: after)
}}

action entries(invoice_id: Invoice.id) -> List<AuditEntry> {{
    var page: Page<AuditEntry> = Invoice.history(invoice_id)
    return page.items
}}
"
    ));
    assert!(found.is_empty(), "{found:?}");
    refused_once(
        &format!(
            "{INVOICE}
action changes(invoice_id: Invoice.id) -> Page<AuditEntry> {{
    return Invoice.history(invoice_id, invoice_id)
}}
"
        ),
        "type.argument_count",
        "takes a record's `id` and, for a later page, `after:`",
    );
}

#[test]
fn a_subscriber_hears_audit_entries_only_when_something_is_audited() {
    let subscriber = "action forward(event: AuditEntryRecorded) -> Unit {
    assert list.count(event.fields) >= 0
}

subscriber forward_entries {
    on: AuditEntryRecorded
    run: forward(event)
}
";
    let found = problems(&format!("{INVOICE}\n{subscriber}"));
    assert!(found.is_empty(), "{found:?}");
    let without = INVOICE.replace(
        "    audit: {\n        retention: {\n            after: config.audit_retention\n        }\n    }\n",
        "",
    );
    refused_once(
        &format!("{without}\n{subscriber}"),
        "subscriber.event_invalid",
        "Nothing publishes `AuditEntryRecorded`",
    );
}

#[test]
fn the_permissions_report_says_which_entities_are_audited() {
    let db = Database::default();
    let project = support::project(
        &db,
        Layout::Loose,
        &[("main.jadpo", &with_settings(INVOICE))],
    );
    let report = jadpo_check::policy::report::report(&db, project);
    assert!(
        report.markdown.contains(
            "Audited: each change is kept in its history for `config.audit_retention`, which `Invoice.history` shows to those who may read the record, without the fields they may not."
        ),
        "{}",
        report.markdown
    );
}
