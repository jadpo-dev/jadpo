//! The application graph (docs/compiler-runtime.md §2.3): what `jadpo-check`
//! builds from a project and everything else reads, from the manifest and
//! its reports to the language server, and later both executors.
//!
//! - [`Kind`] and [`EdgeKind`] name every node and edge the language's
//!   areas need, the ones later areas build included.
//! - [`Key`] is a node's stable identity: its kind and born name, through
//!   the identity [`Registry`]. A [`Digest`] of its canonical form says
//!   whether it changed.
//! - [`Graph`] holds nodes in key order and typed edges between them,
//!   built once by a [`Builder`].
//! - [`json`] writes canonical JSON; [`manifest`] is the graph in it;
//!   [`catalogue`] renders the views `check` writes; [`diff`] compares two
//!   graphs.

pub mod body;
pub mod catalogue;
pub mod diff;
mod graph;
pub mod json;
mod key;
mod kind;
pub mod manifest;
pub mod migration;
mod node;
pub mod schema;
pub mod stored_json;
mod types;

pub use graph::{Builder, Edge, Graph, NodeId};
pub use jadpo_std::Rules;
pub use key::{Key, Registry};
pub use kind::{EdgeKind, Group, Kind, Status};
pub use node::{
    Base, DIGEST_DIGITS, Detail, Digest, FailureDetail, FieldCoding, FieldDefault, Node, Place,
    Section, Signature,
};
pub use types::{TypeRef, rule_pairs};
