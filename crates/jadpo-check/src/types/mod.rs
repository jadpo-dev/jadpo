//! Values and types (area 2), as queries: each file's type declarations
//! lowered to shapes, the project's type table and callable signatures
//! built from them, then each file's declarations and bodies checked
//! against the table.
//!
//! Types are nominal and checked bidirectionally: the type a position
//! expects flows into literals, widening along declared steps is the only
//! implicit conversion, and the standard library's signatures are the only
//! generic ones. A type already reported as wrong is `Error`, which absorbs
//! every later check, so one mistake gives one error.

pub(crate) mod across;
mod body;
pub(crate) mod decl;
mod facts;
mod formats;
pub(crate) mod literal;
mod loading;
mod recursion;
pub(crate) mod rules;
pub(crate) mod secrets;
mod shape;
mod signatures;
mod stores;
mod table;
mod ty;

pub(crate) use body::transition_key;
pub(crate) use body::{BodyView, LocalKind, ViewLocal, members_of};
pub use body::{
    Callable, Checked, FailureSet, FailureSets, Fallible, body_problems, call_problems,
    backfill_callables, callables_of, failure_set, failure_sets, rule_callables, typed_body,
};
pub use decl::type_problems;
pub use facts::{BodyFacts, Fact, Write, body_facts, fact_of, facts_at, named, resolve};
pub use loading::{Decided, Loading, Loads, file_loading, has_isolation, isolation};
pub use recursion::{Cycles, action_calls, action_cycles, recursion_problems};
pub use rules::{CONSTRAINTS, Const, FIELD_SETTINGS, Num, Rules};
pub use shape::{COPY_LINES, FieldsBase, Shape, looks_like_type, type_shapes};
pub use signatures::{Callables, Sig, callables};
pub use stores::{
    Reach as StoreReach, store_problems, store_reaches, unkept_responses, used_stores,
    written_stores,
};
pub use table::{Def, FieldDef, TypeTable, TypeView, VariantDef, standard_fields, type_table};
pub use ty::{BASES, Prim, TIME_FORMAT, Ty, TypeRef, container_arity, shown};
