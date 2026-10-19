//! Batches (area 12, "Batches: work over many items", decided 2026-10-16;
//! chunk 6.4): work over more items than one action can hold, started by
//! an action with `batches.each(from:, each:, finished:, key:)` and run in
//! the background, a page of `from:` at a time, each page one transaction
//! that commits its items' writes and the run's position together.
//!
//! What is decided once here, for the compiler and the runtime alike: the
//! built-in types a program reads (area 2, "The batch types' fields"),
//! the operations of the `batches` namespace, the system tables a run is
//! kept in (area 5, "System tables"), and [`BatchPlan`], what the plan
//! says of each `batches.each` call, as plain data the runtime runs
//! without a compiler crate.

use crate::Builtin;
use crate::Isolation;
use crate::read::ReadPlan;

/// What `batches.each` gives, and an entity field can keep: the run, by
/// its `id`.
pub const BATCH_RUN: &str = "BatchRun";

/// What `batches.status` gives.
pub const BATCH_PROGRESS: &str = "BatchProgress";

/// One item that failed, without its value.
pub const ROW_PROBLEM: &str = "RowProblem";

/// What `finished:` receives after `batches.each`.
pub const IMPORT_SUMMARY: &str = "ImportSummary";

/// A batch type's field: a value built on a built-in type, one or `none`,
/// a standard enum's variant, or a list of another batch type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Of(Builtin),
    Optional(Builtin),
    Enum(&'static str),
    List(&'static str),
}

/// `BatchRun`'s fields (area 2).
pub const BATCH_RUN_FIELDS: [(&str, Part); 1] = [("id", Part::Of(Builtin::Uuid))];

/// `BatchProgress`'s fields (area 2): items done, the estimated total,
/// problems so far and the run's state.
pub const BATCH_PROGRESS_FIELDS: [(&str, Part); 4] = [
    ("items_done", Part::Of(Builtin::Int)),
    ("estimated_total", Part::Optional(Builtin::Int)),
    ("problems", Part::Of(Builtin::Int)),
    ("state", Part::Enum(crate::BatchState::NAME)),
];

/// `RowProblem`'s fields (area 2): the row, the column and the rule's
/// code, never the value.
pub const ROW_PROBLEM_FIELDS: [(&str, Part); 3] = [
    ("row", Part::Of(Builtin::Int)),
    ("column", Part::Of(Builtin::Text)),
    ("rule", Part::Of(Builtin::Text)),
];

/// `ImportSummary`'s fields (area 2).
pub const IMPORT_SUMMARY_FIELDS: [(&str, Part); 3] = [
    ("items", Part::Of(Builtin::Int)),
    ("items_with_problems", Part::Of(Builtin::Int)),
    ("problems", Part::List(ROW_PROBLEM)),
];

/// The fields of a batch type, in declared order, by its name. Only the
/// runtime builds one; a program reads its fields, and keeps a `BatchRun`
/// in an entity field.
pub fn object_fields(name: &str) -> Option<&'static [(&'static str, Part)]> {
    match name {
        BATCH_RUN => Some(&BATCH_RUN_FIELDS),
        BATCH_PROGRESS => Some(&BATCH_PROGRESS_FIELDS),
        ROW_PROBLEM => Some(&ROW_PROBLEM_FIELDS),
        IMPORT_SUMMARY => Some(&IMPORT_SUMMARY_FIELDS),
        _ => None,
    }
}

/// The batch types, in the order area 2 lists them.
pub const OBJECTS: [&str; 4] = [BATCH_RUN, BATCH_PROGRESS, ROW_PROBLEM, IMPORT_SUMMARY];

/// An operation of the `batches` namespace, which only an action calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BatchOp {
    /// `batches.each(from:, each:, finished:, key:)`: starts a run once
    /// the calling action commits, and gives its `BatchRun` at once.
    Each,
    /// `batches.status(run)`: the run's `BatchProgress`.
    Status,
    /// `batches.cancel(run)`: stops the run after the batch in hand.
    Cancel,
}

impl BatchOp {
    pub const ALL: [BatchOp; 3] = [BatchOp::Each, BatchOp::Status, BatchOp::Cancel];

    /// As a call writes it after `batches.`.
    pub fn name(self) -> &'static str {
        match self {
            BatchOp::Each => "each",
            BatchOp::Status => "status",
            BatchOp::Cancel => "cancel",
        }
    }

    pub fn from_name(name: &str) -> Option<BatchOp> {
        BatchOp::ALL.into_iter().find(|op| op.name() == name)
    }

    /// Its parameters, in order: `batches.each`'s are named, the others
    /// take the run.
    pub fn params(self) -> &'static [&'static str] {
        match self {
            BatchOp::Each => &EACH_PARAMS,
            BatchOp::Status | BatchOp::Cancel => &["run"],
        }
    }
}

/// `batches.each`'s parameters, in the order its call is lowered.
pub const EACH_PARAMS: [&str; 4] = ["from", "each", "finished", "key"];

/// The runs, one row each, in the store the run's items write (area 12,
/// "How a run goes").
pub const RUNS: &str = "jadpo_batch_runs";

/// The problems a run's items had, one row per item, never its value.
pub const PROBLEMS: &str = "jadpo_batch_problems";

/// The runs `batches.cancel` asked to stop, each stopped after the batch
/// in hand.
pub const CANCELS: &str = "jadpo_batch_cancels";

/// The system tables a store holding a run keeps, without their
/// `jadpo_` prefix.
pub const TABLES: [&str; 3] = ["batch_runs", "batch_problems", "batch_cancels"];

/// How long a runner holds a run before another may take it over, in
/// milliseconds: renewed with each batch's commit, so a run whose runner
/// died is picked up again once this has passed.
pub const LEASE_MS: i64 = 120_000;

/// What the plan says of one `batches.each` call: what the runtime needs
/// to start its run, walk `from:` a page at a time and run `each:` per
/// item and `finished:` once.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BatchPlan {
    /// Which call it is, kept with each run: the starting callable's
    /// identity key and the call's place among its batches,
    /// `action catalogue.reindex#1`.
    pub name: String,
    /// What the run's saved values and the calls it makes are, written
    /// out: a run started under one shape resumes only under the same.
    pub shape: String,
    /// The store the run is kept in, beside the writes of its items.
    pub store: String,
    /// The level each batch's transaction begins at: repeatable read when
    /// `each:`'s or `finished:`'s action loads a record.
    pub isolation: Isolation,
    /// The named query `from:` calls, which gives a `Page`, walked from
    /// the cursor each page gives.
    pub read: ReadPlan,
    /// The JSON table's number of each value saved with the run, in
    /// order: `from:`'s arguments first, then what `each:` and
    /// `finished:` capture.
    pub captured: Vec<u32>,
    /// How many of `captured` are `from:`'s arguments, in its query's
    /// parameter order.
    pub from_args: usize,
    pub each: BatchCall,
    pub finished: Option<BatchCall>,
    /// The JSON table's number of `key:`'s type, when the call has one.
    pub key: Option<u32>,
}

/// An action `each:` or `finished:` calls, by number, with its
/// arguments.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BatchCall {
    pub callable: u32,
    pub args: Vec<BatchArg>,
}

/// Where one argument of a batch's call comes from.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum BatchArg {
    /// The item, or for `finished:` the summary, or a field inside it:
    /// `row.sku` is `["sku"]`.
    Item(Vec<String>),
    /// A value saved with the run, by its place in
    /// [`BatchPlan::captured`].
    Captured(u32),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_type_lists_its_fields_and_each_operation_its_name() {
        for name in OBJECTS {
            assert!(object_fields(name).is_some_and(|fields| !fields.is_empty()));
        }
        assert_eq!(object_fields("Page"), None);
        for op in BatchOp::ALL {
            assert_eq!(BatchOp::from_name(op.name()), Some(op));
        }
        assert_eq!(BatchOp::Each.params(), EACH_PARAMS);
        assert_eq!(
            TABLES.map(|table| format!("jadpo_{table}")),
            [RUNS, PROBLEMS, CANCELS]
        );
    }
}
