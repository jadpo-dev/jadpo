//! The audit trail (area 16, "Audit trail"; chunk B11): an entity with
//! an `audit:` section keeps a history of every change to its records,
//! one `jadpo_audit` row a change, written in the transaction that makes
//! it. What an entry holds, as a program reads it through
//! `Entity.history(id)` and a subscriber of `AuditEntryRecorded` is
//! given it, and the plans `jadpo-plan` decides for both executors: what
//! a write records ([`AuditPlan`]) and what `history` reads
//! ([`HistoryPlan`]).
//!
//! A changed field's value is kept as its canonical text, as `text.from`
//! gives it, or its JSON for a type with no single text form: the form
//! area 15 hashes a marked field from, so the same entry reads the same
//! on both executors.

use crate::Builtin;
use crate::batch::Part;
use crate::read::{Param, SqlType};
use crate::write::{Returned, WriteSource};

/// One entry of a record's history, as `history` gives it.
pub const AUDIT_ENTRY: &str = "AuditEntry";
/// One changed field of an entry.
pub const AUDIT_FIELD_CHANGE: &str = "AuditFieldChange";
/// The built-in event each entry is also emitted as when something
/// subscribes to it (area 16, "Sending audit entries elsewhere").
pub const AUDIT_ENTRY_RECORDED: &str = "AuditEntryRecorded";
/// The identity key the outbox keeps `AuditEntryRecorded` under, and its
/// subscribers listen for.
pub const AUDIT_EVENT_KEY: &str = "jadpo.AuditEntryRecorded";
/// The generated query, `Invoice.history(invoice_id)`.
pub const HISTORY: &str = "history";
/// How many entries one page of a history holds: `PageSize`'s default.
pub const HISTORY_PAGE: u32 = 25;

/// `AuditEntry`'s fields, in order: when, what kind of change, who made
/// it (the signed-in user's or service's `id`), the outermost action it
/// was made through, the request that made it and the trace it was made
/// in, and each changed field.
pub const ENTRY_FIELDS: [(&str, Part); 9] = [
    ("at", Part::Of(Builtin::Instant)),
    ("change", Part::Enum(crate::AuditChange::NAME)),
    ("by", Part::Enum(crate::AuditBy::NAME)),
    ("user_id", Part::Optional(Builtin::Uuid)),
    ("service_id", Part::Optional(Builtin::Uuid)),
    ("action", Part::Optional(Builtin::Text)),
    ("request", Part::Optional(Builtin::Text)),
    ("trace", Part::Optional(Builtin::Text)),
    ("fields", Part::List(AUDIT_FIELD_CHANGE)),
];

/// `AuditFieldChange`'s fields: the field's name and its value before and
/// after, each `none` when it had none.
pub const FIELD_CHANGE_FIELDS: [(&str, Part); 3] = [
    ("field", Part::Of(Builtin::Text)),
    ("before", Part::Optional(Builtin::Text)),
    ("after", Part::Optional(Builtin::Text)),
];

/// `AuditEntryRecorded`'s fields: the entity's name and the record's `id`,
/// then an entry's.
pub const EVENT_FIELDS: [(&str, Part); 11] = [
    ("entity", Part::Of(Builtin::Text)),
    ("record_id", Part::Of(Builtin::Uuid)),
    ENTRY_FIELDS[0],
    ENTRY_FIELDS[1],
    ENTRY_FIELDS[2],
    ENTRY_FIELDS[3],
    ENTRY_FIELDS[4],
    ENTRY_FIELDS[5],
    ENTRY_FIELDS[6],
    ENTRY_FIELDS[7],
    ENTRY_FIELDS[8],
];

/// The fields of one of the audit trail's objects, which only the runtime
/// builds, by its name.
pub fn object_fields(name: &str) -> Option<&'static [(&'static str, Part)]> {
    match name {
        AUDIT_ENTRY => Some(&ENTRY_FIELDS),
        AUDIT_FIELD_CHANGE => Some(&FIELD_CHANGE_FIELDS),
        AUDIT_ENTRY_RECORDED => Some(&EVENT_FIELDS),
        _ => None,
    }
}

/// What a write of an audited entity records, beside its statement, in
/// the same transaction.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct AuditPlan {
    /// The entity's name, as `AuditEntryRecorded` gives it.
    pub entity: String,
    /// Its born name, which its entries are kept under.
    pub born: String,
    /// For an update or a delete: the record as it was, read and locked
    /// before the write changes it.
    pub before: Option<AuditBefore>,
    /// For a lifecycle's `delete` transition, which keeps the record and
    /// returns nothing: the record as the transition left it, read by the
    /// same statement once it has run.
    pub after: Option<AuditBefore>,
    /// The fields an entry records, in the entity's order: every field
    /// but `id` and those no request may touch.
    pub fields: Vec<AuditedField>,
    /// The identity keys of the subscribers `AuditEntryRecorded` is
    /// delivered to; with none, it isn't emitted.
    pub subscribers: Vec<String>,
}

/// The record a change starts from.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct AuditBefore {
    /// `SELECT` of every column by `id` (`$1`), locking the row as the
    /// write will: `FOR NO KEY UPDATE` before an update, `FOR UPDATE`
    /// before a delete.
    pub sql: String,
    /// Where the record's `id` comes from: the `id` the write is given, or
    /// its loaded record's.
    pub id: WriteSource,
    /// What the row becomes, as the write's own returned row.
    pub returned: Returned,
}

/// A field an entry records, and how its value becomes text.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct AuditedField {
    pub name: String,
    /// For a JSON-held field, its codec in the plan's JSON table: its
    /// value is kept as that JSON. Otherwise its canonical text.
    pub json: Option<u32>,
}

/// What `Entity.history(id)` reads: a page of the record's entries, newest
/// first, for a caller its `read` grant lets see the record, without the
/// fields whose own `policy:` keeps them from the caller.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct HistoryPlan {
    /// The store the entity and its history live in.
    pub store: String,
    /// The entity's born name, which its entries are kept under.
    pub born: String,
    /// Whether the record may be read: `SELECT` of one row by `id` (`$1`,
    /// the call's first argument) under the entity's lifecycle and its
    /// `read` grant, with a `Bool` column for each field in `restricted`,
    /// whether the caller may read it. No row and the page is empty. A
    /// call without a caller asks nothing and reads every entry.
    pub access: String,
    pub params: Vec<Param>,
    /// The fields whose own `policy:` narrows who reads them, in the order
    /// `access` selects their grants.
    pub restricted: Vec<String>,
    /// The tag a cursor of this history carries, so a cursor from
    /// anything else is refused.
    pub tag: u64,
}

/// The type a page cursor's position in a history is sent as: an entry's
/// `seq`.
pub const CURSOR: SqlType = SqlType::Int8;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_events_fields_are_its_record_then_an_entrys() {
        let names: Vec<&str> = EVENT_FIELDS.iter().map(|(name, _)| *name).collect();
        assert_eq!(&names[..2], ["entity", "record_id"]);
        let entry: Vec<&str> = ENTRY_FIELDS.iter().map(|(name, _)| *name).collect();
        assert_eq!(&names[2..], entry.as_slice());
        assert_eq!(object_fields("AuditEntry").map(<[_]>::len), Some(9));
        assert_eq!(object_fields("FieldChange"), None);
    }
}
