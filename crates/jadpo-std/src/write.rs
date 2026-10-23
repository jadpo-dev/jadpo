//! What a write runs (area 6, "The operations every entity gets";
//! docs/compiler-runtime.md §2.5): one statement per `create`, `update` or
//! `delete`, where each parameter comes from, what the row it returns
//! becomes, the constraints whose refusal the write maps and what each
//! raises, and the failure when the record it names isn't there, decided
//! once by `jadpo-plan` and run by `jadpo-runtime` for both executors.
//!
//! The executor works out the write's inputs first (a loaded record or an
//! `id`, the values `set:` gives, a patch's input), in the order they are
//! written; each parameter names an input, a field inside one, whether a
//! patch supplied a field, a new identity or the clock. The text is fixed
//! when the project is compiled: a patch's field is written
//! `CASE WHEN <supplied> THEN <value> ELSE <column> END`, so the same write
//! always sends the same statement.

use crate::read::{Column, MissingPlan, Record, SqlType};

/// Which operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum WriteOp {
    Create,
    Update,
    Delete,
}

impl WriteOp {
    /// As Jadpo writes it: `create`.
    pub fn word(self) -> &'static str {
        match self {
            WriteOp::Create => "create",
            WriteOp::Update => "update",
            WriteOp::Delete => "delete",
        }
    }
}

/// Where a write's parameter comes from when it runs.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum WriteSource {
    /// The executor's input at this position.
    Input(u32),
    /// A field of the input at this position: a loaded record's `id`, or a
    /// patch's field, `NULL` when the patch left it out.
    Field { input: u32, field: String },
    /// Whether the patch at this position supplied `field`.
    Supplied { input: u32, field: String },
    /// Whether the patch at this position supplied any of `fields`.
    AnySupplied { input: u32, fields: Vec<String> },
    /// A new identity: a version 7 UUID (area 5).
    NewId,
    /// The operation's clock, for a generated field (area 5).
    Now,
    /// Who is calling, for the policy compiled into the statement (area
    /// 9).
    Caller(crate::policy::CallerPart),
    /// The column at `index` of those a compound standard type from `of`
    /// is stored in (`Money`'s amount, then its currency, area 19); `NULL`
    /// for no value.
    Part { of: Box<WriteSource>, index: u32 },
    /// The value from `of` written as its contract's JSON, by the codec
    /// `codec` names in the plan's JSON table, for a JSON-held standard
    /// value's `jsonb` column (chunk 5.5w2b); `NULL` for no value.
    Json { of: Box<WriteSource>, codec: u32 },
    /// A literal or an enum value the statement sets or compares with, as
    /// the storage codec binds it (area 19, chunk 5.5w3b).
    Stored(crate::SqlValue),
}

/// One parameter: `$1` is the first.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct WriteParam {
    pub source: WriteSource,
    pub ty: SqlType,
}

/// What the row a `create` or an `update` returns becomes.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Returned {
    /// What each returned column becomes, in order.
    pub columns: Vec<Column>,
    /// The rules each column's field is held to when read back (chunk
    /// 5.9d), as a read's [`crate::read::Select::held`].
    pub held: Vec<Option<crate::read::FieldRules>>,
    pub record: Record,
}

/// One write: its statement, its parameters, what it returns and the
/// constraints whose refusal it maps.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct WritePlan {
    /// The store it writes (a connection's name).
    pub store: String,
    pub op: WriteOp,
    pub sql: String,
    /// The statement without the policy compiled into `sql`, when policy
    /// limits it: what a test's call without a caller runs (area 9). It
    /// takes the same parameters.
    pub unpoliced: Option<String>,
    /// When policy limits it: what writing no row means (area 9).
    pub policy: Option<PolicyRefusal>,
    pub params: Vec<WriteParam>,
    /// The record a `create` or `update` gives; `None` for a `delete`.
    pub returns: Option<Returned>,
    /// Whether it names its record by `id` rather than a loaded record:
    /// then writing no row means the record isn't there, which is the
    /// entity's not-found failure, rather than a record loaded earlier and
    /// gone since.
    pub by_id: bool,
    /// The constraints a refusal can name that the write maps, in order,
    /// each with what it raises. A lifecycle transition's `when:` is one
    /// too, written `when: Loan.check_in`.
    pub refusals: Vec<Refusal>,
    /// For a transition whose `when:` can refuse a visible record: what
    /// to ask when the write changed no row.
    pub guard: Option<Guard>,
    /// A write by `id` that finds no record: the entity's not-found
    /// failure, given the `id`.
    pub missing: Option<MissingPlan>,
    /// Whether its statement, with policy or without, also writes no row
    /// when a reference it sets points into another tenant (area 9,
    /// "References stay inside the tenant"): then writing no row without
    /// policy, a create or a record that is there, is `not_permitted` too.
    pub kept_in_tenant: bool,
    /// For an update or a delete, a lifecycle's transitions among them:
    /// how its row is checked to be the version loaded before an outside
    /// call (area 6, "Outside calls hold no transaction"; chunk 7.4).
    pub row: Option<RowCheck>,
}

/// What a write holds in its WHERE when its action loaded its row in a snapshot
/// an outside call closed (area 6, "Outside calls hold no transaction";
/// chunk 7.4): that the row is still the version loaded, by PostgreSQL's
/// row version. The write itself locks the row. A row changed or gone
/// since is a concurrent change, which runs the action again. The extra
/// probe runs only when the mutation changes no row, preserving refusal
/// ordering before policy, field, lifecycle and not-found probes.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RowCheck {
    /// The entity's table, which names the row among those loaded, as
    /// [`crate::read::RowVersions::table`] does.
    pub table: String,
    /// Where the row's `id` comes from: the `id` the write is given, or
    /// its loaded record's.
    pub id: WriteSource,
    /// The zero-based parameter the runtime replaces with the first-loaded
    /// version, or a typed NULL when this transaction already checked it.
    pub version: u32,
    /// Failure-only probe: `SELECT "xmin"::text FROM <table> WHERE "id" = $1
    /// FOR NO KEY UPDATE`, before classifying a refused write.
    pub sql: String,
}

/// A constraint whose refusal a write maps (area 6, "When a write is
/// refused"), and what the refusal raises.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Refusal {
    /// The constraint as the store names it, or the transition whose
    /// `when:` refused, `when: Loan.check_in`.
    pub constraint: String,
    pub raises: Raises,
}

/// What a refused write raises.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Raises {
    /// The failure the site's `conflict:` arm at this position names,
    /// whose fields the executor works out only then.
    Arm(u32),
    /// A reference naming a record that isn't there: that entity's
    /// not-found failure.
    NotFound(MissingPlan),
    /// A reference read from a record loaded earlier, whose record was
    /// deleted since: a fault saying so.
    Gone(String),
}

/// What a guarded transition asks when it changed no row (area 5,
/// "Lifecycle"): whether the record is there and visible, in which case
/// its `when:` refused it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Guard {
    /// `SELECT 1` of the record by `id`, under the lifecycle's `visible:`
    /// and policy.
    pub sql: String,
    /// The same without policy.
    pub unpoliced: Option<String>,
    /// Its parameters, from the write's inputs.
    pub params: Vec<WriteParam>,
    /// The position in `refusals` its refusal takes.
    pub refusal: u32,
}

/// What a write that policy limits means when it changed no row (area 9,
/// "When policy says no").
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum PolicyRefusal {
    /// A create policy refused, the record's grants or a field's own:
    /// `not_permitted`.
    Create,
    /// An update or delete: `SELECT 1` of the record by `id` under its
    /// lifecycle but not policy. Found, policy refused it: a write by `id`
    /// reports the record's not-found failure, logged as concealed by
    /// policy, and one of a record loaded earlier `not_permitted`.
    Probe {
        sql: String,
        params: Vec<WriteParam>,
        /// For an update changing a field whose own `policy:` narrows the
        /// record's grants (area 9, "Field restrictions"): `SELECT 1` of
        /// the record under its lifecycle and the record's grants, but not
        /// the fields'. Found, the caller may change the record but not a
        /// field the request changes, so the whole write is refused with
        /// `not_permitted`, by `id` or not.
        fields: Option<FieldProbe>,
    },
}

/// The statement that tells a field's policy refusing a write from the
/// record's (see [`PolicyRefusal::Probe`]).
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct FieldProbe {
    pub sql: String,
    pub params: Vec<WriteParam>,
}

/// How many records one purge batch deletes at most (area 5: "in small
/// audited batches"), each batch its own transaction.
pub const PURGE_BATCH: u32 = 500;

/// Where a purge statement's parameter comes from.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum PurgeSource {
    /// The run's time less the lifecycle's `after:`: a record hidden since
    /// before it goes.
    Cutoff,
    /// The run's time, for a `visible:` that reads `clock.now`.
    Now,
    /// A literal or an enum value `visible:` compares with, as the storage
    /// codec binds it (area 19, chunk 5.5w3b).
    Stored(crate::SqlValue),
}

/// One purge parameter: `$1` is the first.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct PurgeParam {
    pub source: PurgeSource,
    pub ty: SqlType,
}

/// The permanent deletion of an entity's hidden records once they have
/// been hidden for its lifecycle's `after:` (area 5, "Lifecycle"): the
/// compiler's own operation, outside every action and policy (area 9),
/// which the built-in hourly purge schedule runs (area 12).
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct PurgePlan {
    /// The entity's name.
    pub entity: String,
    pub store: String,
    /// One batch: `DELETE ... RETURNING "id"` of at most [`PURGE_BATCH`]
    /// records that are hidden, hidden since before the cutoff, and that
    /// no `restricted` reference holds, oldest first.
    pub sql: String,
    pub params: Vec<PurgeParam>,
    /// The lifecycle's `after:`, in milliseconds; 0 when a setting gives it
    /// (`after_setting`).
    pub after: i64,
    /// The setting `after:` reads, by its number in the plan's settings
    /// (area 10, "Retention periods"): the period is what the application
    /// started with, so a run is given it (`purge_after`).
    pub after_setting: Option<u32>,
}
