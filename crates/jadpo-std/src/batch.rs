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

/// The objects only the runtime builds and an entity field or a run keeps
/// as their JSON: the batch types and a stored `File` (chunk 6.4b), each
/// with its fields.
pub fn kept_fields(name: &str) -> Option<&'static [(&'static str, Part)]> {
    object_fields(name).or_else(|| crate::files::object_fields(name))
}

/// The format a file helper reads rows in (area 19, "Data formats"),
/// named by its namespace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RowFormat {
    /// `csv`: comma-separated cells under a header row (RFC 4180).
    Csv,
    /// `tsv`: CSV's tab-separated form.
    Tsv,
    /// `json_lines`: one JSON value a line.
    JsonLines,
}

impl RowFormat {
    pub const ALL: [RowFormat; 3] = [RowFormat::Csv, RowFormat::Tsv, RowFormat::JsonLines];

    /// Its namespace: `csv`, `tsv`, `json_lines`.
    pub fn namespace(self) -> &'static str {
        match self {
            RowFormat::Csv => "csv",
            RowFormat::Tsv => "tsv",
            RowFormat::JsonLines => "json_lines",
        }
    }

    pub fn from_namespace(name: &str) -> Option<RowFormat> {
        RowFormat::ALL
            .into_iter()
            .find(|format| format.namespace() == name)
    }

    /// The byte between a row's cells, for the formats with a header.
    pub fn delimiter(self) -> Option<u8> {
        match self {
            RowFormat::Csv => Some(b','),
            RowFormat::Tsv => Some(b'\t'),
            RowFormat::JsonLines => None,
        }
    }
}

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
    /// `csv.read(file, as:, each:, finished:, key:)`, and `tsv`'s and
    /// `json_lines`'s (area 19, "Data formats"; chunk 6.4b): starts a run
    /// over a stored file's rows once the calling action commits.
    Read(RowFormat),
}

impl BatchOp {
    /// The operations of the `batches` namespace.
    pub const ALL: [BatchOp; 3] = [BatchOp::Each, BatchOp::Status, BatchOp::Cancel];

    /// As a call writes it after `batches.`, or after a format's
    /// namespace.
    pub fn name(self) -> &'static str {
        match self {
            BatchOp::Each => "each",
            BatchOp::Status => "status",
            BatchOp::Cancel => "cancel",
            BatchOp::Read(_) => "read",
        }
    }

    /// Whether it starts a run: `batches.each`, or a file helper's.
    pub fn starts(self) -> bool {
        matches!(self, BatchOp::Each | BatchOp::Read(_))
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
            BatchOp::Read(_) => &READ_PARAMS,
        }
    }
}

/// `batches.each`'s parameters, in the order its call is lowered.
pub const EACH_PARAMS: [&str; 4] = ["from", "each", "finished", "key"];

/// A file helper's parameters when it reads (area 19, "Data formats"),
/// in the order its call is lowered: the stored `File`, `as:` (a type,
/// which lowers to nothing; `each:`'s item is of it), then as
/// `batches.each`'s.
pub const READ_PARAMS: [&str; 5] = ["file", "as", "each", "finished", "key"];

/// The runs, one row each, in the store the run's items write (area 12,
/// "How a run goes").
pub const RUNS: &str = crate::system::BATCH_RUNS.table;

/// The problems a run's items had, one row per item, never its value.
pub const PROBLEMS: &str = crate::system::BATCH_PROBLEMS.table;

/// The runs `batches.cancel` asked to stop, each stopped after the batch
/// in hand.
pub const CANCELS: &str = crate::system::BATCH_CANCELS.table;

/// What the plan says of one `batches.each` call: what the runtime needs
/// to start its run, walk `from:` a page at a time and run `each:` per
/// item and `finished:` once.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BatchPlan {
    /// Which call it is, kept with each run, by identity keys that
    /// survive a rename (chunk 6.8): the starting callable's, the query
    /// `from:` walks and the action `each:` runs, `action reindex from
    /// query stale_items each action reindex_item`; a second call of the
    /// same three in one callable adds its place among them, ` #2`.
    pub name: String,
    /// What the run's saved values and the calls it makes are, written
    /// out in canonical type text (chunk 6.8): a run started under one
    /// shape resumes only under the same.
    pub shape: String,
    /// The store the run is kept in, beside the writes of its items.
    pub store: String,
    /// The level each batch's transaction begins at: repeatable read when
    /// `each:`'s or `finished:`'s action loads a record.
    pub isolation: Isolation,
    /// What the run walks: a named query's pages, or a stored file's
    /// rows.
    pub from: BatchFrom,
    /// The JSON table's number of each value saved with the run, in
    /// order: `from:`'s arguments (or the file a file helper reads)
    /// first, then what `each:` and `finished:` capture.
    pub captured: Vec<u32>,
    /// How many of `captured` are `from:`'s arguments, in its query's
    /// parameter order, or the file: one.
    pub from_args: usize,
    pub each: BatchCall,
    pub finished: Option<BatchCall>,
    /// The JSON table's number of `key:`'s type, when the call has one.
    pub key: Option<u32>,
    /// The tenant the run is limited to (area 12, "A batch's tenant";
    /// chunk 6.4c).
    pub tenant: BatchTenant,
}

/// What a run walks.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum BatchFrom {
    /// The named query `from:` calls, which gives a `Page`, walked from
    /// the cursor each page gives.
    Query(ReadPlan),
    /// The rows of the stored `File` saved first with the run (chunk
    /// 6.4b).
    File(FileRows),
}

impl BatchFrom {
    /// The query it walks, if it walks one.
    pub fn query(&self) -> Option<&ReadPlan> {
        match self {
            BatchFrom::Query(read) => Some(read),
            BatchFrom::File(_) => None,
        }
    }
}

/// How a file helper reads a file's rows (area 19, "Data formats";
/// chunk 6.4b).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FileRows {
    pub format: RowFormat,
    /// The JSON table's number of `as:`'s type, an `Input`, each row read
    /// as one.
    pub row: u32,
    /// For CSV and TSV, the column each of the row type's fields is read
    /// from, in its fields' order: its `column:`, else its name.
    pub columns: Vec<String>,
}

/// Where a run's tenant comes from: its saved values holding a tenant's
/// id, as an event's tenant comes from its field holding one
/// ([`crate::subscriber::TenantFrom`]).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum BatchTenant {
    /// The application has no tenant: nothing limits the system.
    Untenanted,
    /// The saved values whose type is the tenant's `id`: their places in
    /// [`BatchPlan::captured`], and whether `key:` is one. The run is
    /// limited to the one tenant they all name, and to no tenant when
    /// there is none, one is `none`, or they name different tenants.
    Saved { captured: Vec<u32>, key: bool },
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
        // A file helper's `read` is its format's, never `batches.read`.
        assert_eq!(BatchOp::from_name("read"), None);
        for format in RowFormat::ALL {
            assert_eq!(RowFormat::from_namespace(format.namespace()), Some(format));
            assert_eq!(BatchOp::Read(format).params(), READ_PARAMS);
            assert!(BatchOp::Read(format).starts());
        }
        assert!(kept_fields(crate::files::FILE).is_some());
        assert!(kept_fields(BATCH_RUN).is_some());
        assert_eq!(BatchOp::Each.params(), EACH_PARAMS);
        assert_eq!(
            crate::system::BATCH_TABLES.map(|table| table.table),
            [RUNS, PROBLEMS, CANCELS]
        );
    }
}
