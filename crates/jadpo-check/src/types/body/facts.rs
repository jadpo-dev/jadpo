//! What checking a body decides, recorded as it goes so the typed body is
//! built from the same decisions the diagnostics come from: each
//! expression's type, which local each name reads, what each call reaches,
//! which field or variant each dotted name is, and what each pattern
//! matches. Facts are keyed by syntax node: its range and kind.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::ItemKind;
use jadpo_diag::Span;
use jadpo_std::Op;
use jadpo_syntax::SyntaxNode;

use super::super::ty::Ty;
use super::known::Origin;
use super::{Fallible, LocalKind};

/// A syntax node, as a key: its range and its kind.
pub(crate) type NodeKey = (u32, u32, u16);

pub(crate) fn node_key(node: &SyntaxNode) -> NodeKey {
    let range = node.text_range();
    (range.start().into(), range.end().into(), node.kind() as u16)
}

/// A local: a parameter, variable, loop item, binding or inline parameter.
#[derive(Clone, Debug)]
pub(crate) struct LocalFact {
    pub name: String,
    pub ty: Ty,
    pub kind: LocalKind,
    pub span: Span,
}

/// What a call reaches, as the checker resolved it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Reach {
    /// A function, action, query or fixture, with its `fails` list.
    Callable {
        kind: ItemKind,
        name: String,
        fails: Vec<String>,
    },
    Library {
        library: String,
        function: String,
        fails: Vec<String>,
    },
    Std(Op),
    /// `Type(value)` or `Owner.field(value)`: the type being built.
    Construct(Ty),
    /// An entity's operation (area 6).
    Entity {
        entity: String,
        operation: String,
    },
    /// `Principal.user(record)` (area 8).
    Caller(jadpo_std::http::PrincipalKind),
    /// `sessions.start(user_id)` or `sessions.end()` (area 8).
    Session(jadpo_std::http::SessionOp),
    /// `batches.each(...)`, `batches.status(run)` or `batches.cancel(run)`
    /// (area 12).
    Batch(jadpo_std::batch::BatchOp),
    /// `store.put(bytes, type)` or `store.read(file)` through the `files`
    /// connection it names (area 16; chunk 6.4b).
    Files(String, jadpo_std::files::FilesOp),
    /// `connection.resource.operation(input)` (area 11, "Calling"; chunk
    /// 7.2c): the connection, the operation (`bookings.create`) and the
    /// package failures it answers with.
    Provider {
        connection: String,
        operation: String,
        fails: Vec<String>,
        /// Whether the operation was found and checked: one reported
        /// where it is declared can fail with anything.
        checked: bool,
    },
    /// `clock.advance(duration)` in a test (area 13).
    AdvanceClock,
    /// `schedule.run()` in a test (area 13, "Kinds of test"; chunk 6.2c):
    /// the schedule, the action it runs, unless it reads with problems,
    /// and that action's `fails` list.
    Schedule {
        schedule: jadpo_graph::Key,
        action: Option<jadpo_graph::Key>,
        fails: Vec<String>,
    },
    /// `translations.<key>(locale:, ...)` (area 10), with the number of
    /// arguments it takes: the locale and each placeholder.
    Translation {
        key: String,
        arity: usize,
    },
    /// `prompts.render(prompt)` (area 21): the prompt's name.
    Prompt(String),
    Unknown,
}

/// What a pattern matches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PatternFact {
    /// `Enum.variant`.
    Variant(String, String),
    Some,
    Success,
    /// A failure arm of an outcome `match`.
    Failure(String),
    Wildcard,
    Unknown,
}

/// What a block built with `{ ... }` is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BuildFact {
    Object(String),
    Variant(String, String),
}

#[derive(Debug, Default)]
pub(crate) struct Facts {
    /// Each expression's type: a literal's is the type it goes into.
    pub types: BTreeMap<NodeKey, Ty>,
    /// The type each checked position required, for the editor's
    /// completion (area 14).
    pub expected: BTreeMap<NodeKey, Ty>,
    pub locals: Vec<LocalFact>,
    /// The local each declared name brings in.
    pub declared: BTreeMap<NodeKey, usize>,
    /// The local each name reads or assigns.
    pub reads: BTreeMap<NodeKey, usize>,
    /// What each call reaches.
    pub reaches: BTreeMap<NodeKey, Reach>,
    /// Each standard-library call's arguments: the parameter each fills.
    pub slots: BTreeMap<NodeKey, Vec<Option<usize>>>,
    /// Each field read, as its host and path: `Note` and `body`.
    pub fields: BTreeMap<NodeKey, (String, Vec<String>)>,
    /// Each plain variant written as a value: `Shade.plain`.
    pub variants: BTreeMap<NodeKey, (String, String)>,
    /// Each `Enum.all` (area 19, question 22): the enum and the variants it
    /// lists, in order, a closed list's retired ones left out.
    pub all_variants: BTreeMap<NodeKey, (String, Vec<String>)>,
    /// Each object or variant built with its fields.
    pub builds: BTreeMap<NodeKey, BuildFact>,
    /// Whether each construction can fail.
    pub constructions: BTreeMap<NodeKey, Fallible>,
    /// The rules of a construction only a condition around it proves
    /// (area 2, "Conditions as proof"), each with the comparisons that
    /// prove it.
    pub proofs: BTreeMap<NodeKey, Vec<(String, Vec<Origin>)>>,
    /// What each call can fail with.
    pub fallible: BTreeMap<NodeKey, Fallible>,
    /// Each operator on two amounts of money whose currencies are known
    /// only while running, so it can fail with `currency_mismatch` (area
    /// 19).
    pub fallible_ops: BTreeSet<NodeKey>,
    /// The failure each arm of an `attempt` on a construction names, by
    /// the rule's key; `None` for `invalid:`, which covers the rest.
    pub mapped: BTreeMap<NodeKey, Vec<(Option<String>, String)>>,
    /// The arms of an `attempt` on `json.read` whose failure receives the
    /// first problem in its `problem: DecodeProblem` field (area 19, "Data
    /// formats"), by the `attempt` and the arm's key as written.
    pub receives: BTreeSet<(NodeKey, String)>,
    /// The failure each `reject` raises.
    pub rejects: BTreeMap<NodeKey, String>,
    /// The failure each `propagate` passes on.
    pub propagates: BTreeMap<NodeKey, String>,
    /// What each case pattern matches.
    pub patterns: BTreeMap<NodeKey, PatternFact>,
    /// The `match`es on a call's outcome.
    pub outcomes: BTreeSet<NodeKey>,
    /// Each read of `clock.now`, an effect that can pause, lowered to
    /// [`jadpo_graph::body::Expr::Now`].
    pub effects: BTreeSet<NodeKey>,
    /// Each entity write (area 6): keyed by the record for a `create`, the
    /// call for an `update` or `delete`.
    pub writes: BTreeMap<NodeKey, super::writes::WriteFact>,
    /// Each stored field an update by `id` reads (`Post.view_count`).
    pub stored: BTreeSet<NodeKey>,
    /// Each read of a setting (`config.mail_sender`), with the setting's
    /// name (area 10).
    pub settings: BTreeMap<NodeKey, String>,
    /// Each `format:` argument, as the format's text (area 10), and each
    /// `as:` of `text.parse`, as the name of the standard type or
    /// representation it reads (area 19): an argument written out, given
    /// to its operation as text.
    pub formats: BTreeMap<NodeKey, String>,
    /// Each `text.parse` call, with the type it gives (area 19).
    pub parsed: BTreeMap<NodeKey, Ty>,
    /// Each `text.format` call whose value is a standard type held as
    /// text, with that type's name, which the call is given (area 19,
    /// chunk 5.5i).
    pub shown: BTreeMap<NodeKey, &'static str>,
    /// Each `ai.distance` call, with how its vectors' type compares them,
    /// as `VectorDistance`'s variant name, which the call is given (area
    /// 21, chunk 5.5v1).
    pub metric: BTreeMap<NodeKey, &'static str>,
    /// Each read of a constant (area 3): its library, if it is a
    /// library's, and its name as written.
    pub constants: BTreeMap<NodeKey, (Option<String>, String)>,
    /// Each `request { ... }` in a route test (area 13).
    pub requests: BTreeMap<NodeKey, super::request::RequestFact>,
}
