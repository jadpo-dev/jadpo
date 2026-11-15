//! Entities in Redis (area 5, "Entities in Redis"; area 6, "Reading and
//! writing entities in Redis"; chunk Q10): what the plan says of each read
//! and write of an entity whose `store:` is a Redis connection, and the
//! names its records and the keys Jadpo keeps for them are kept under.
//!
//! - **A record is one JSON value**, under
//!   `jadpo:entity:<table>:record:<id>`: the record as the API's JSON,
//!   written and read by its codec with the names its fields and variants
//!   were born with, wrapped with the revision Jadpo keeps on it and the
//!   millisecond it expires at, `{"revision":3,"expires":...,"record":{...}}`.
//!   It expires `expires_after:` after its last write.
//! - **A key per unique field's value**,
//!   `jadpo:entity:<table>:unique:<field>:<digest>`, holding the `id` of
//!   the record that has it, expiring with it.
//! - **Two sorted sets per value of each field in `indexes:`**:
//!   `…:index:<field>:<digest>`, the records holding it by `id`, which a
//!   page reads in order; and `…:expiry:<field>:<digest>`, each one's
//!   expiry, which a count reads.
//!
//! A value is never kept as written: each `<digest>` is 32 hex digits of
//! BLAKE3 of the field's JSON. Policy on a Redis record is applied in
//! Jadpo's own code once the record is read ([`crate::policy::RecordGrants`]).

use crate::Constant;
use crate::policy::RecordGrants;

/// What every key an entity in Redis keeps starts with, so a database
/// shared with a cache or rate limits keeps them apart.
pub const PREFIX: &str = "jadpo:entity:";

/// The shortest `expires_after:` an entity in Redis takes: a second, the
/// finest expiry every Redis-compatible service keeps.
pub const LEAST_EXPIRY_MS: i64 = 1_000;

/// What one entity in Redis is, as each of its reads and writes needs it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RedisEntity {
    /// Its name, as a record's type: `SignUpDraft`.
    pub entity: String,
    /// The connection its records are kept in.
    pub store: String,
    /// What its keys are named from: its table's name, from the name it
    /// was born with, so a rename keeps its records.
    pub keyspace: String,
    /// Its record's codec, by its number in the plan's JSON table.
    pub codec: u32,
    /// `expires_after:`, in milliseconds.
    pub expires_ms: i64,
    /// Each field with `unique: true`, in declared order.
    pub unique: Vec<KeyedField>,
    /// Each field `indexes:` lists, in declared order.
    pub indexed: Vec<KeyedField>,
    /// Who may read, create, update and delete its records.
    pub policy: RecordPolicy,
}

/// A field Jadpo keeps a key or a set for.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct KeyedField {
    /// Its name, as the record holds it.
    pub field: String,
    /// The name it was born with, which its keys are named from.
    pub born: String,
    /// Its type's codec, by its number in the plan's JSON table: what a
    /// value of it is written as before it is hashed.
    pub codec: u32,
}

/// An entity's grants, applied to each record once it is read (area 6:
/// "checked after reading").
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RecordPolicy {
    pub read: RecordGrants,
    pub create: RecordGrants,
    pub update: RecordGrants,
    pub delete: RecordGrants,
}

/// Where a value a read looks a record up by comes from.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Given {
    /// The read's argument at `index`, or a field inside it.
    Arg { index: u32, path: Vec<String> },
    /// A literal or an enum value, fixed while compiling.
    Constant(Constant),
    /// `clock.now`.
    Now,
}

/// How a read finds its records.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Lookup {
    /// By `id`: `get`, or a query whose `where:` is `Entity.id == value`.
    Id(Given),
    /// By the key Jadpo keeps for the unique field at `field` of
    /// [`RedisEntity::unique`].
    Unique { field: u32, value: Given },
    /// A page or a list of the records holding `value` in the field at
    /// `field` of [`RedisEntity::indexed`], in creation order.
    Index { field: u32, value: Given },
    /// How many records hold `value` in that field: an aggregate query
    /// whose one total is `count()`, given as a record of type `ty` whose
    /// one field is `total`.
    Count {
        field: u32,
        value: Given,
        ty: String,
        total: String,
    },
}

/// What each record read becomes.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Element {
    /// The entity's whole record.
    Record,
    /// A record of type `ty` holding, for each of its fields, the
    /// record's field of the second name.
    Picked {
        ty: String,
        fields: Vec<(String, String)>,
    },
}

/// A read of an entity in Redis: how it finds its records and what each
/// becomes. The [`crate::read::ReadPlan`] holding it says how many come
/// back, a page's size and cursor, and what a missing record raises.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RedisRead {
    pub entity: RedisEntity,
    pub lookup: Lookup,
    pub element: Element,
    /// A list's `limit:`.
    pub limit: Option<crate::read::Size>,
}

/// Where a value a write keeps comes from (area 6, "Counters and other
/// one-step updates").
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Kept {
    /// The executor's input at this position.
    Input(u32),
    /// A patch's field, the input at `input`: kept as stored when the
    /// patch leaves it out.
    Patched {
        input: u32,
        field: String,
    },
    /// `value` when the patch at `input` supplies any of `fields`, kept as
    /// stored otherwise (`when_supplied:`).
    WhenSupplied {
        input: u32,
        fields: Vec<String>,
        value: Box<Kept>,
    },
    /// A new identity: a version 7 UUID.
    NewId,
    /// The operation's clock, for a generated field.
    Now,
    /// The value stored in `field` as the write is made: the record as
    /// Redis holds it when the action commits.
    Stored(String),
    /// A whole number added to or taken from another.
    Add(Box<Kept>, Box<Kept>),
    Subtract(Box<Kept>, Box<Kept>),
    Negate(Box<Kept>),
}

impl Kept {
    /// Whether it reads the value stored as the write is made.
    pub fn reads_stored(&self) -> bool {
        match self {
            Kept::Stored(_) => true,
            Kept::Add(lhs, rhs) | Kept::Subtract(lhs, rhs) => {
                lhs.reads_stored() || rhs.reads_stored()
            }
            Kept::Negate(inner) => inner.reads_stored(),
            Kept::WhenSupplied { value, .. } => value.reads_stored(),
            Kept::Input(_) | Kept::Patched { .. } | Kept::NewId | Kept::Now => false,
        }
    }
}

/// A write of an entity in Redis. The [`crate::write::WritePlan`] holding
/// it says which operation, whether its record is named by `id`, what a
/// missing record raises and the refusals its site maps.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RedisWrite {
    pub entity: RedisEntity,
    /// For an update or a delete, the input holding the record loaded
    /// earlier, or its `id` when the plan's `by_id` says so.
    pub target: Option<u32>,
    /// What each field it sets keeps, in the record's field order: every
    /// field for a create, those `set:` and `patch:` give for an update,
    /// each `Generated.on_change` field among them.
    pub set: Vec<(String, Kept)>,
    /// For each unique field (by its position in
    /// [`RedisEntity::unique`]) whose refusal the site's `conflict:` maps,
    /// the position in the plan's `refusals` of what it raises.
    pub conflicts: Vec<(u32, u32)>,
}

/// The key record `id` of the entity kept under `keyspace` is kept under.
pub fn record_key(keyspace: &str, id: &str) -> String {
    format!("{PREFIX}{keyspace}:record:{id}")
}

/// What every record key of `keyspace` starts with, a record's `id` after.
pub fn record_prefix(keyspace: &str) -> String {
    format!("{PREFIX}{keyspace}:record:")
}

/// The key holding the `id` of the record whose unique field `born` holds
/// the value whose digest is `digest`.
pub fn unique_key(keyspace: &str, born: &str, digest: &str) -> String {
    format!("{PREFIX}{keyspace}:unique:{born}:{digest}")
}

/// The sorted set of the records whose field `born` holds the value whose
/// digest is `digest`, by `id`.
pub fn index_key(keyspace: &str, born: &str, digest: &str) -> String {
    format!("{PREFIX}{keyspace}:index:{born}:{digest}")
}

/// The sorted set of when each of those records expires.
pub fn expiry_key(keyspace: &str, born: &str, digest: &str) -> String {
    format!("{PREFIX}{keyspace}:expiry:{born}:{digest}")
}

/// `expires_after:` written as Jadpo writes a duration: `2h`, `30m`.
pub fn expiry_text(expires_ms: i64) -> String {
    crate::caches::keep_text(u64::try_from(expires_ms).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_name_the_entity_and_never_a_value() {
        assert_eq!(
            record_key("sign_up_draft", "0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e"),
            "jadpo:entity:sign_up_draft:record:0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e"
        );
        assert_eq!(
            unique_key("sign_up_draft", "email", "ab"),
            "jadpo:entity:sign_up_draft:unique:email:ab"
        );
        assert_eq!(
            index_key("sign_up_draft", "step", "cd"),
            "jadpo:entity:sign_up_draft:index:step:cd"
        );
        assert_eq!(
            expiry_key("sign_up_draft", "step", "cd"),
            "jadpo:entity:sign_up_draft:expiry:step:cd"
        );
        assert!(record_key("a", "b").starts_with(&record_prefix("a")));
    }

    #[test]
    fn a_counter_reads_what_is_stored_and_a_value_given_doesnt() {
        let counter = Kept::Add(
            Box::new(Kept::Stored("attempts".into())),
            Box::new(Kept::Input(1)),
        );
        assert!(counter.reads_stored());
        assert!(!Kept::Input(1).reads_stored());
        assert!(
            !Kept::Patched {
                input: 0,
                field: "step".into()
            }
            .reads_stored()
        );
    }
}
