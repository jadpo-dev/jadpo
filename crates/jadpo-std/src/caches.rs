//! Caches (area 5, "Caches"; area 6, "Where a read comes from"; area 7,
//! "Write revisions"; chunks Q9 and Q9b): what the plan says of a query's
//! `cache:`, of an entity's record cache and of what each committed change
//! clears, and the names an entry, a generation and a list of entries are
//! kept under in a cache's store.
//!
//! A query's results are kept per caller and per arguments under one key
//! each, beside a generation for each entity its statements read and a
//! list, per entity, of the entries that read it. A change to an entity
//! adds one to its generation and deletes every entry its list names, so
//! an entry whose read began before the change is never served after it,
//! and no copy is left behind (area 15, "What erasure reaches"). A record
//! cache keeps each record whole under a key of its own, one copy for
//! every caller, listed under its entity alone, and holds each copy to
//! policy as it is read ([`RecordRead`]).

/// What a route's display read of a query with `cache:` keeps (area 5,
/// "Caches"): where, for how long, and what clears it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RouteCache {
    /// The connection the query's `store:` names: a Redis connection, or
    /// `Driver.memory`.
    pub store: String,
    /// `keep:`, the oldest an entry can be, in milliseconds.
    pub keep_ms: u64,
    /// The entities its statements read, its own, an include's and what
    /// its policy reads, in name order: a change to any of them clears it.
    pub tags: Vec<String>,
    /// The query's identity key and a digest of its read plan, which every
    /// entry's key is worked out from: a build that reads differently
    /// reads none of an earlier build's entries. For a record cache, the
    /// entity's name and a digest of what it keeps of each record.
    pub key: String,
    /// For a route served from its entity's record cache (area 5, "A record
    /// cache"; chunk Q9b): how the one record is found, kept and shown.
    /// Its `tags` are then the entity alone, and an entry is one record's,
    /// every caller's.
    #[serde(default)]
    pub records: Option<Box<RecordRead>>,
}

/// A route's display read of one record by `id` through its entity's
/// record cache (area 5, "A record cache"; chunk Q9b). The record is kept
/// whole, but for the fields the cache leaves out, under a key worked out
/// from its `id`, and each time it is read it is held to the entity's
/// `read:` grants in Jadpo's own code, then becomes what the query gives.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RecordRead {
    /// The entity, as a record's type.
    pub entity: String,
    /// Where the `id` the query looks for comes from.
    pub id: crate::redis::Given,
    /// Each field it keeps, in declared order: what a copy holds.
    pub fields: Vec<KeptField>,
    /// Who may read a record, as a record read whole is tested.
    pub grants: crate::policy::RecordGrants,
    /// What the record becomes: the query's own type.
    pub element: crate::redis::Element,
    /// The record by `id`, its one argument, as the cache keeps it: every
    /// field it keeps, as `visible:` lets reads see, and no policy, which
    /// each read applies to the copy.
    pub fill: Box<crate::read::ReadPlan>,
}

/// A field a record cache keeps of each record.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct KeptField {
    /// Its name, as the record holds it.
    pub field: String,
    /// The name it was born with, which a copy keeps it under, as a record
    /// in Redis does.
    pub born: String,
    /// Its type's codec, by its number in the plan's JSON table, with the
    /// names it was born with.
    pub codec: u32,
}

/// What one committed change clears (area 5, "The running app keeps
/// caches current"): in the cache `store`, every entry that reads any of
/// `tags`, by entity name.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Clear {
    pub store: String,
    pub tags: Vec<String>,
}

/// What every key a cache keeps starts with, so a database shared with
/// rate limits keeps the two apart.
pub const PREFIX: &str = "jadpo:cache:";

/// The shortest `keep:` a cache takes: a second, the finest a store's
/// expiry is counted in that every service keeps.
pub const LEAST_KEEP_MS: u64 = 1_000;

/// The longest `keep:` a cache takes: a day (area 5's design notes).
pub const MOST_KEEP_MS: u64 = 86_400_000;

/// The header every write's response carries its revision in, and a
/// request sends it back in (area 7, "Write revisions").
pub const REVISION_HEADER: &str = "jadpo-revision";

/// The cookie a browser is given the same revision in.
pub const REVISION_COOKIE: &str = "jadpo_revision";

/// The key entity `tag`'s generation is kept under.
pub fn generation_key(tag: &str) -> String {
    format!("{PREFIX}generation:{tag}")
}

/// The key of the list of entries that read entity `tag`.
pub fn listing_key(tag: &str) -> String {
    format!("{PREFIX}entries:{tag}")
}

/// The key one entry is kept under, from the digest of what it was read
/// with, as 32 hex digits: never an argument or a caller as written.
pub fn entry_key(digest: &str) -> String {
    format!("{PREFIX}entry:{digest}")
}

/// How long a list of entries is kept after its latest entry was added: a
/// day and a minute, longer than any entry it names, so a list outlives
/// what it lists and a clear always finds each one.
pub const LISTING_KEEP_MS: u64 = MOST_KEEP_MS + 60_000;

/// `keep:` written as Jadpo writes a duration: `10m`, `1h`, `90s`.
pub fn keep_text(keep_ms: u64) -> String {
    const UNITS: [(u64, &str); 4] = [
        (86_400_000, "d"),
        (3_600_000, "h"),
        (60_000, "m"),
        (1_000, "s"),
    ];
    for (size, unit) in UNITS {
        if keep_ms >= size && keep_ms.is_multiple_of(size) {
            return format!("{}{unit}", keep_ms / size);
        }
    }
    format!("{keep_ms}ms")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_name_what_they_hold() {
        assert_eq!(generation_key("Product"), "jadpo:cache:generation:Product");
        assert_eq!(listing_key("Product"), "jadpo:cache:entries:Product");
        assert_eq!(entry_key("ab"), "jadpo:cache:entry:ab");
    }

    #[test]
    fn keep_is_written_in_its_largest_whole_unit() {
        assert_eq!(keep_text(600_000), "10m");
        assert_eq!(keep_text(3_600_000), "1h");
        assert_eq!(keep_text(86_400_000), "1d");
        assert_eq!(keep_text(90_000), "90s");
    }
}
