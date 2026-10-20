//! Batches (area 12, "Batches: work over many items", decided 2026-10-16;
//! chunk 6.4): work over more items than one action can hold, started by
//! an action with `batches.each(from:, each:, finished:, key:)` and run in
//! the background, a page of `from:` at a time, each page one transaction
//! that commits its items' writes and the run's position together.
//!
//! What is decided once here, for the compiler and the runtime alike: the
//! built-in types a program reads (area 2, "The batch types' fields"),
//! the operations of the `batches` namespace and the file helpers, the
//! system tables a run is kept in (area 5, "System tables"), and
//! [`BatchPlan`], what the plan says of each call starting a run, as
//! plain data the runtime runs without a compiler crate.

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

/// What `finished:` receives after `batches.each` or a file helper's
/// read.
pub const IMPORT_SUMMARY: &str = "ImportSummary";

/// What `finished:` receives after a file helper's write (chunk 6.4b2):
/// the counts and problems an `ImportSummary` holds, and the stored
/// `File`.
pub const EXPORT_SUMMARY: &str = "ExportSummary";

/// A batch type's field: a value built on a built-in type, one or `none`,
/// a standard enum's variant, a list of another batch type, or another
/// object only the runtime builds (an `ExportSummary`'s `File`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Of(Builtin),
    Optional(Builtin),
    Enum(&'static str),
    List(&'static str),
    Kept(&'static str),
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

/// `ExportSummary`'s fields (area 2): an `ImportSummary`'s, then the file
/// written.
pub const EXPORT_SUMMARY_FIELDS: [(&str, Part); 4] = [
    ("items", Part::Of(Builtin::Int)),
    ("items_with_problems", Part::Of(Builtin::Int)),
    ("problems", Part::List(ROW_PROBLEM)),
    ("file", Part::Kept(crate::files::FILE)),
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
        EXPORT_SUMMARY => Some(&EXPORT_SUMMARY_FIELDS),
        _ => None,
    }
}

/// The batch types, in the order area 2 lists them.
pub const OBJECTS: [&str; 5] = [
    BATCH_RUN,
    BATCH_PROGRESS,
    ROW_PROBLEM,
    IMPORT_SUMMARY,
    EXPORT_SUMMARY,
];

/// The objects only the runtime builds and an entity field or a run keeps
/// as their JSON: the batch types and a stored `File` (chunk 6.4b), each
/// with its fields.
pub fn kept_fields(name: &str) -> Option<&'static [(&'static str, Part)]> {
    object_fields(name).or_else(|| crate::files::object_fields(name))
}

/// The format a file helper reads or writes rows in (area 19, "Data
/// formats"), named by its namespace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RowFormat {
    /// `csv`: comma-separated cells under a header row (RFC 4180).
    Csv,
    /// `tsv`: CSV's tab-separated form.
    Tsv,
    /// `json_lines`: one JSON value a line.
    JsonLines,
    /// `json`'s file form: one JSON list, an item a row, written (chunk
    /// 6.4b2) and read where `items_at:` says it sits (chunk 6.4b3).
    Json,
}

impl RowFormat {
    pub const ALL: [RowFormat; 4] = [
        RowFormat::Csv,
        RowFormat::Tsv,
        RowFormat::JsonLines,
        RowFormat::Json,
    ];

    /// Its namespace: `csv`, `tsv`, `json_lines`, `json`.
    pub fn namespace(self) -> &'static str {
        match self {
            RowFormat::Csv => "csv",
            RowFormat::Tsv => "tsv",
            RowFormat::JsonLines => "json_lines",
            RowFormat::Json => "json",
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
            RowFormat::JsonLines | RowFormat::Json => None,
        }
    }

    /// The `FileType` variant a file written in it has (area 16).
    pub fn file_type(self) -> &'static str {
        match self {
            RowFormat::Csv => "csv",
            RowFormat::Tsv => "tsv",
            RowFormat::JsonLines => "json_lines",
            RowFormat::Json => "json",
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
    /// `csv.write(store, name:, format:, from:, each:, finished:, key:)`,
    /// and `tsv`'s, `json_lines`'s and `json`'s file form (area 19, "Data
    /// formats"; chunk 6.4b2): starts a run writing a query's pages into a
    /// stored file once the calling action commits.
    Write(RowFormat),
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
            BatchOp::Write(_) => "write",
        }
    }

    /// Whether it starts a run: `batches.each`, or a file helper's.
    pub fn starts(self) -> bool {
        matches!(self, BatchOp::Each | BatchOp::Read(_) | BatchOp::Write(_))
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
            BatchOp::Write(_) => &WRITE_PARAMS,
        }
    }
}

/// `batches.each`'s parameters, in the order its call is lowered.
pub const EACH_PARAMS: [&str; 4] = ["from", "each", "finished", "key"];

/// A file helper's parameters when it reads (area 19, "Data formats"),
/// in the order its call is lowered: the stored `File`, `as:` (a type,
/// which lowers to nothing; `each:`'s item is of it), then as
/// `batches.each`'s; then, from chunk 6.4b3, `items_at:` (`json.read`'s
/// only: where the list sits in the document, a text literal) and
/// `max_unpacked_size:` (a size literal, which lowers to its bytes), each
/// the plan's rather than a value the run saves.
pub const READ_PARAMS: [&str; 7] = [
    "file",
    "as",
    "each",
    "finished",
    "key",
    "items_at",
    "max_unpacked_size",
];

/// Where `items_at:` sits among [`READ_PARAMS`].
pub const ITEMS_AT: usize = 5;

/// Where `max_unpacked_size:` sits among [`READ_PARAMS`].
pub const MAX_UNPACKED_SIZE: usize = 6;

/// A file helper's parameters when it writes (area 19, "Data formats";
/// chunk 6.4b2), in the order its call is lowered: the store of files it
/// writes to (a connection, which lowers to its name), the file's `name:`,
/// `format:` (a `CsvFormat`, CSV's and TSV's only), then `from:`, `each:`
/// (an inline function giving the item's row), `finished:` and `key:`.
pub const WRITE_PARAMS: [&str; 7] = ["store", "name", "format", "from", "each", "finished", "key"];

/// The runs, one row each, in the store the run's items write (area 12,
/// "How a run goes").
pub const RUNS: &str = crate::system::BATCH_RUNS.table;

/// The problems a run's items had, one row per item, never its value.
pub const PROBLEMS: &str = crate::system::BATCH_PROBLEMS.table;

/// The runs `batches.cancel` asked to stop, each stopped after the batch
/// in hand.
pub const CANCELS: &str = crate::system::BATCH_CANCELS.table;

/// What the plan says of one call starting a run: what the runtime needs
/// to start it, walk `from:` a page at a time, run `each:` per item, write
/// what a writer writes, and run `finished:` once.
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
    /// first, then a writer's `name:` and `format:`, then what `each:` and
    /// `finished:` capture.
    pub captured: Vec<u32>,
    /// How many of `captured` are `from:`'s arguments, in its query's
    /// parameter order, or the file: one.
    pub from_args: usize,
    pub each: BatchEach,
    pub finished: Option<BatchCall>,
    /// What a file helper's write writes each item's row into (chunk
    /// 6.4b2): `None` for a run that writes no file.
    pub to: Option<FileWrite>,
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
    Query(Box<ReadPlan>),
    /// The rows of the stored `File` saved first with the run (chunk
    /// 6.4b).
    File(FileRows),
}

impl BatchFrom {
    /// The query it walks, if it walks one.
    pub fn query(&self) -> Option<&ReadPlan> {
        match self {
            BatchFrom::Query(read) => Some(read.as_ref()),
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
    /// For `json.read`, the members `items_at:` names, outermost first,
    /// leading from the document's top object to its list of items; empty
    /// when the document is the list (chunk 6.4b3).
    pub items_at: Vec<String>,
    /// `max_unpacked_size:`, in bytes: a gzip file is unpacked up to it,
    /// and without it one stops the run (chunk 6.4b3).
    pub max_unpacked: Option<u64>,
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

/// What `each:` is: an action called per item, or a writer's row.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum BatchEach {
    /// `batches.each`'s and a read's: an action, in the item's batch's
    /// transaction.
    Action(BatchCall),
    /// A writer's (chunk 6.4b2): a pure expression giving the item's row.
    Row(BatchRow),
}

impl BatchEach {
    /// The action it calls, when it calls one.
    pub fn action(&self) -> Option<&BatchCall> {
        match self {
            BatchEach::Action(call) => Some(call),
            BatchEach::Row(_) => None,
        }
    }
}

/// A writer's `each:`, `item -> <expression>` (chunk 6.4b2): the inline
/// function's body, in the body of the action that started the run, run
/// once per item with the item given to its parameter and each local of
/// the action it reads given the value the run saved for it. An inline
/// function is pure and can't fail (`attempt` only starts a statement), so
/// what stops it is a fault.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BatchRow {
    /// The callable whose body holds it: the action starting the run.
    pub callable: u32,
    /// The site of the inline function's body in that callable's body.
    pub body: u32,
    /// The local its parameter is.
    pub item: u32,
    /// Each local of the action it reads, with the place in
    /// [`BatchPlan::captured`] of the value saved for it.
    pub locals: Vec<(u32, u32)>,
}

/// Where a writer writes (chunk 6.4b2): each page's rows put as a piece
/// of the file once the page's transaction has closed, the pieces joined
/// into one file once every page is written.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FileWrite {
    pub format: RowFormat,
    /// The `files` connection it writes to.
    pub store: String,
    /// The JSON table's number of the row type, an `Output`, whose codec
    /// writes each row.
    pub row: u32,
    /// For CSV and TSV, each of the row type's fields' column, in its
    /// fields' order: its `column:`, else its name, the header's cells.
    pub columns: Vec<String>,
    /// The place in [`BatchPlan::captured`] of `name:`'s value.
    pub name: u32,
    /// For CSV and TSV, the place in [`BatchPlan::captured`] of
    /// `format:`'s `CsvFormat`.
    pub csv_format: Option<u32>,
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
            assert_eq!(BatchOp::Write(format).params(), WRITE_PARAMS);
            assert!(BatchOp::Read(format).starts() && BatchOp::Write(format).starts());
            assert!(
                crate::FILE_TYPES
                    .iter()
                    .any(|(name, _)| *name == format.file_type())
            );
        }
        assert_eq!(
            object_fields(EXPORT_SUMMARY).map(|fields| fields.len()),
            Some(4)
        );
        assert!(kept_fields(crate::files::FILE).is_some());
        assert!(kept_fields(BATCH_RUN).is_some());
        assert_eq!(BatchOp::Each.params(), EACH_PARAMS);
        assert_eq!(
            crate::system::BATCH_TABLES.map(|table| table.table),
            [RUNS, PROBLEMS, CANCELS]
        );
    }
}
