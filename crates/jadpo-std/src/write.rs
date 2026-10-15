//! What a write runs (area 6, "The operations every entity gets";
//! docs/compiler-runtime.md §2.5): one statement per `create`, `update` or
//! `delete`, where each parameter comes from, what the row it returns
//! becomes, and the constraints whose refusal the write maps, decided once
//! by `jadpo-plan` and run by `jadpo-runtime` for both executors.
//!
//! The executor works out the write's inputs first (a loaded record or an
//! `id`, the values `set:` gives, a patch's input), in the order they are
//! written; each parameter names an input, a field inside one, whether a
//! patch supplied a field, a new identity or the clock. The text is fixed
//! when the project is compiled: a patch's field is written
//! `CASE WHEN <supplied> THEN <value> ELSE <column> END`, so the same write
//! always sends the same statement.

use crate::read::{Column, Record, SqlType};

/// Which operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
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
}

/// One parameter: `$1` is the first.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WriteParam {
    pub source: WriteSource,
    pub ty: SqlType,
}

/// What the row a `create` or an `update` returns becomes.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Returned {
    /// What each returned column becomes, in order.
    pub columns: Vec<Column>,
    pub record: Record,
}

/// One write: its statement, its parameters, what it returns and the
/// constraints whose refusal it maps.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WritePlan {
    /// The store it writes (a connection's name).
    pub store: String,
    pub op: WriteOp,
    pub sql: String,
    pub params: Vec<WriteParam>,
    /// The record a `create` or `update` gives; `None` for a `delete`.
    pub returns: Option<Returned>,
    /// Whether it names its record by `id` rather than a loaded record:
    /// then writing no row means the record isn't there, which is the
    /// entity's not-found failure, rather than a record loaded earlier and
    /// gone since.
    pub by_id: bool,
    /// The constraints a refusal can name that the write maps, in order:
    /// the executor raises what its site maps each one to. A lifecycle
    /// transition's `when:` is one too, written `when: Loan.check_in`.
    pub refusals: Vec<String>,
    /// For a transition whose `when:` can refuse a visible record: what
    /// to ask when the write changed no row.
    pub guard: Option<Guard>,
}

/// What a guarded transition asks when it changed no row (area 5,
/// "Lifecycle"): whether the record is there and visible, in which case
/// its `when:` refused it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Guard {
    /// `SELECT 1` of the record by `id`, under the lifecycle's `visible:`.
    pub sql: String,
    /// Its parameters, from the write's inputs.
    pub params: Vec<WriteParam>,
    /// The position in `refusals` its refusal takes.
    pub refusal: u32,
}

/// How many records one purge batch deletes at most (area 5: "in small
/// audited batches"), each batch its own transaction.
pub const PURGE_BATCH: u32 = 500;

/// Where a purge statement's parameter comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PurgeSource {
    /// The run's time less the lifecycle's `after:`: a record hidden since
    /// before it goes.
    Cutoff,
    /// The run's time, for a `visible:` that reads `clock.now`.
    Now,
}

/// One purge parameter: `$1` is the first.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PurgeParam {
    pub source: PurgeSource,
    pub ty: SqlType,
}

/// The permanent deletion of an entity's hidden records once they have
/// been hidden for its lifecycle's `after:` (area 5, "Lifecycle"): the
/// compiler's own operation, outside every action and policy (area 9),
/// which the built-in hourly purge schedule runs (area 12).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PurgePlan {
    /// The entity's name.
    pub entity: String,
    pub store: String,
    /// One batch: `DELETE ... RETURNING "id"` of at most [`PURGE_BATCH`]
    /// records that are hidden, hidden since before the cutoff, and that
    /// no `restricted` reference holds, oldest first.
    pub sql: String,
    pub params: Vec<PurgeParam>,
    /// The lifecycle's `after:`, in milliseconds.
    pub after: i64,
}
