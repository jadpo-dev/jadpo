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
    /// `clock.advance(duration)` in a test (area 13).
    AdvanceClock,
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
    /// The failure each arm of an `attempt` on a construction names, by
    /// the rule's key; `None` for `invalid:`, which covers the rest.
    pub mapped: BTreeMap<NodeKey, Vec<(Option<String>, String)>>,
    /// The failure each `reject` raises.
    pub rejects: BTreeMap<NodeKey, String>,
    /// The failure each `propagate` passes on.
    pub propagates: BTreeMap<NodeKey, String>,
    /// What each case pattern matches.
    pub patterns: BTreeMap<NodeKey, PatternFact>,
    /// The `match`es on a call's outcome.
    pub outcomes: BTreeSet<NodeKey>,
    /// The effects with no typed form of their own yet (`clock.now`), which
    /// can pause.
    pub effects: BTreeSet<NodeKey>,
    /// Each entity write (area 6): keyed by the record for a `create`, the
    /// call for an `update` or `delete`.
    pub writes: BTreeMap<NodeKey, super::writes::WriteFact>,
    /// Each stored field an update by `id` reads (`Post.view_count`).
    pub stored: BTreeSet<NodeKey>,
    /// Each read of a setting (`config.mail_sender`), with the setting's
    /// name (area 10).
    pub settings: BTreeMap<NodeKey, String>,
    /// Each `format:` argument, as the format's text (area 10).
    pub formats: BTreeMap<NodeKey, String>,
    /// Each `request { ... }` in a route test (area 13).
    pub requests: BTreeMap<NodeKey, super::request::RequestFact>,
}
