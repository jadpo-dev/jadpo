//! Erasure past the person's own transaction (area 15, "What erasure
//! reaches" and "What erasure doesn't rewrite"; chunk P4c): what the
//! compiler and the runtime both decide of it.
//!
//! - [`PERSON_ERASED`]: the event an erasure emits in its transaction, one
//!   for each person it erased, holding the person entity's name, their
//!   `id` and when, and nothing personal.
//! - [`ErasurePlan`]: for each person entity, its erasure as a statement
//!   for each store that holds records of theirs, each a generated
//!   subscriber's after the commit and each run again by the replay; and
//!   where the log is kept, written after the commit too.
//! - [`LogEntry`] and [`log_key`]: an entry of the log kept in a store of
//!   files, one file for each person, its key saying whose.

use crate::Builtin;
use crate::batch::Part;
use crate::write::WriteParam;

/// The built-in event an erasure emits (area 15, "Erasure at providers").
pub const PERSON_ERASED: &str = "PersonErased";
/// The identity key the outbox keeps `PersonErased` under, and its
/// subscribers listen for.
pub const PERSON_ERASED_KEY: &str = "jadpo.PersonErased";

/// `PersonErased`'s fields: the person entity's name, the person's `id`
/// and when they were erased.
pub const EVENT_FIELDS: [(&str, Part); 3] = [
    ("entity", Part::Of(Builtin::Text)),
    ("id", Part::Of(Builtin::Uuid)),
    ("erased_at", Part::Of(Builtin::Instant)),
];

/// The value `User.export(id)` gives (area 15, "Export"), which only the
/// runtime builds.
pub const PERSONAL_DATA_EXPORT: &str = "PersonalDataExport";

/// `PersonalDataExport`'s fields: the whole export as one JSON document,
/// and how many records it holds, none for someone with nothing held about
/// them or a caller who may not read their record.
pub const EXPORT_FIELDS: [(&str, Part); 2] = [
    ("json", Part::Of(Builtin::Text)),
    ("records", Part::Of(Builtin::Int)),
];

/// The fields of `PersonErased` and `PersonalDataExport`, which only the
/// runtime builds.
pub fn object_fields(name: &str) -> Option<&'static [(&'static str, Part)]> {
    match name {
        PERSON_ERASED => Some(&EVENT_FIELDS[..]),
        PERSONAL_DATA_EXPORT => Some(&EXPORT_FIELDS[..]),
        _ => None,
    }
}

/// A person entity's `export` (area 15, "Export"): whether the caller may
/// read the person's record, and then what is read of each kind of data.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ExportPlan {
    /// The person's own entity, as the export names it.
    pub entity: String,
    /// The person's `id` when the caller may read their record: nothing
    /// when policy refuses it, or the person isn't there or was erased.
    pub access: crate::read::ReadPlan,
    /// What it reads: one entity's records in one store each, the person's
    /// own first.
    pub parts: Vec<ExportPart>,
    /// What it says of each entity it reads, in the same order.
    pub kinds: Vec<ExportKind>,
}

/// One entity's records in an export.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ExportPart {
    /// The entity, as the export names its list.
    pub entity: String,
    /// Every record of it the person holds, each as the entity's fields
    /// that are exported.
    pub read: crate::read::ReadPlan,
    /// Each exported field by its name, with its number in the plan's JSON
    /// table: what its value is written with, in the API's form.
    pub fields: Vec<(String, u32)>,
}

/// How long one kind of data is kept and who receives it (area 15,
/// "Export").
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ExportKind {
    pub entity: String,
    /// Its `retention:` and its fields', empty when kept until erased.
    pub retention: Vec<ExportRetention>,
    /// The connections its personal data leaves through, by name.
    pub recipients: Vec<String>,
}

/// One `retention:` an export reports.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ExportRetention {
    /// The field it clears, or none when it deletes the record.
    pub field: Option<String>,
    /// The `Duration` setting its period is read from, by its number.
    pub setting: u32,
    /// The `Instant` field it counts from.
    pub since: String,
}

/// How many times a generated subscriber is tried: enough that
/// [`WITHIN_MS`] ends it, not a count. A store that is down for a while
/// waits, in the outbox, for as long as a request may take by law.
pub const ATTEMPTS: u32 = 10_000;
/// How long after the erasure a generated subscriber is given up, and left
/// `failed` for an operator (area 12, "What an operator sees"): the month
/// the law gives (area 15, "Deadlines and events").
pub const WITHIN_MS: u64 = 30 * 24 * 60 * 60 * 1000;

/// The most log entries the replay asks a store about at once.
pub const REPLAY_PAGE: u32 = 1_000;

/// The generated subscriber that keeps a person's entry in the log.
pub const LOG_SUBSCRIBER: &str = "erasure log";

/// The generated subscriber that applies an erasure in `store`, which
/// isn't the person's own: `erasure in archive`.
pub fn store_subscriber(store: &str) -> String {
    format!("erasure in {store}")
}

/// The generated subscriber that calls `connection`'s erasure operation:
/// `erasure at stripe`.
pub fn provider_subscriber(connection: &str) -> String {
    format!("erasure at {connection}")
}

/// Where a `PersonErased`'s payload holds each connection's key, by the
/// connection's name: the event's only personal part, read only by the
/// generated subscribers and taken away as each is delivered.
pub const PAYLOAD_KEYS: &str = "keys";

/// An erasure in every store it reaches, and where the log is kept.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ErasurePlan {
    /// Each person entity, by name.
    pub people: Vec<PersonPlan>,
    /// `privacy: { erasure_log: { store: } }`.
    pub log: Option<ErasureLog>,
    /// Each connection that can ask its provider to erase someone.
    pub providers: Vec<ProviderErasure>,
}

impl ErasurePlan {
    /// The plan of the person entity called `entity`.
    pub fn person(&self, entity: &str) -> Option<&PersonPlan> {
        self.people.iter().find(|person| person.entity == entity)
    }

    /// What `connection` calls to erase someone.
    pub fn provider(&self, connection: &str) -> Option<&ProviderErasure> {
        self.providers
            .iter()
            .find(|provider| provider.connection == connection)
    }
}

/// A connection's call that erases a person at its provider (area 15,
/// "Erasure at providers"): its package's `erasure: { call: by: }` and the
/// connection's `erasure: { key: }`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ProviderErasure {
    pub connection: String,
    /// `customers.delete`.
    pub operation: String,
    /// That operation's number in the plan's provider operations.
    pub called: u32,
    /// The field of its input the key is, by its spelling on the wire;
    /// `None` when the key is its whole input.
    pub field: Option<String>,
}

/// One person entity's erasure.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct PersonPlan {
    /// Its name, as `PersonErased` gives it.
    pub entity: String,
    /// Its born name, which the log keeps, so a rename finds its entries.
    pub born: String,
    /// The identity keys of the subscribers `PersonErased` is delivered to
    /// for one of these, in key order: the app's own, one for each of
    /// `elsewhere` and the log's.
    pub subscribers: Vec<String>,
    /// In the store it lives in, without policy: what the replay runs.
    pub own: StoreErasure,
    /// In each other store whose records refer to one of these.
    pub elsewhere: Vec<StoreErasure>,
}

impl PersonPlan {
    /// Its erasure in `store`, not its own, when it holds records of theirs.
    pub fn elsewhere(&self, store: &str) -> Option<&StoreErasure> {
        self.elsewhere.iter().find(|part| part.store == store)
    }
}

/// One person's erasure in one store, as one statement.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct StoreErasure {
    pub store: String,
    /// Finds the person by their `id` (`WriteSource::Input(0)`) and does
    /// all it does here, returning a row for each person it erased here:
    /// their place in `people` and their `id`. Run again it finds nothing
    /// and does nothing, however often.
    pub sql: String,
    pub params: Vec<WriteParam>,
    /// Which of `$1`'s `id`s it would find anything of here: a person not
    /// yet erased, or one a record here still refers to.
    pub present: String,
    /// By a row's first column, the person entity's name.
    pub people: Vec<(i32, String)>,
}

/// Where the log is kept (area 15, "What erasure doesn't rewrite").
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ErasureLog {
    /// `jadpo_erasure_log` in this database, which holds no person's
    /// records.
    Database { store: String },
    /// A file for each person in this store of files.
    Files { connection: String },
}

impl ErasureLog {
    /// The connection it is kept through.
    pub fn connection(&self) -> &str {
        match self {
            ErasureLog::Database { store } => store,
            ErasureLog::Files { connection } => connection,
        }
    }
}

/// The folder a store of files keeps the log's files in, which no file's
/// key ([`crate::files::key_for`]) can name.
pub const LOG_FOLDER: &str = "erasure-log";

/// The key of the log's file for the person of the entity born `born`
/// whose `id`, in its text form, is `id`: `erasure-log/User/0190…`.
pub fn log_key(born: &str, id: &str) -> String {
    format!("{LOG_FOLDER}/{born}/{id}")
}

/// The born name and `id` a key [`log_key`] made holds; `None` for any
/// other, so a driver refuses a key that could name a path outside its own.
pub fn log_key_parts(key: &str) -> Option<(&str, &str)> {
    let mut parts = key.split('/');
    let (Some(LOG_FOLDER), Some(born), Some(id), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    let named = !born.is_empty()
        && born
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
    let shaped = id.len() == 36
        && id.bytes().enumerate().all(|(at, byte)| match at {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte),
        });
    (named && shaped).then_some((born, id))
}

/// What the log's file for one person holds: nothing personal.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LogEntry {
    /// The person entity's born name.
    pub entity: String,
    /// Their `id`.
    pub id: String,
    /// When they were erased, in milliseconds since the epoch.
    pub erased_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_log_key_names_one_person_and_nothing_else() {
        let id = "0190a2b3-c4d5-7e6f-8a9b-0c1d2e3f4a5b";
        let key = log_key("User", id);
        assert_eq!(log_key_parts(&key), Some(("User", id)));
        for wrong in [
            "erasure-log/User",
            "erasure-log/../0190a2b3-c4d5-7e6f-8a9b-0c1d2e3f4a5b",
            "erasure-log/User/../../etc/passwd",
            "erasure-log/User/0190a2b3-c4d5-7e6f-8a9b-0c1d2e3f4a5b/more",
            "other/User/0190a2b3-c4d5-7e6f-8a9b-0c1d2e3f4a5b",
            "erasure-log/User/0190A2B3-c4d5-7e6f-8a9b-0c1d2e3f4a5b",
            "erasure-log/Us.er/0190a2b3-c4d5-7e6f-8a9b-0c1d2e3f4a5b",
        ] {
            assert_eq!(log_key_parts(wrong), None, "{wrong}");
        }
    }

    #[test]
    fn a_log_key_is_no_files_key() {
        assert!(!crate::files::is_key(&log_key(
            "User",
            "0190a2b3-c4d5-7e6f-8a9b-0c1d2e3f4a5b"
        )));
    }
}
