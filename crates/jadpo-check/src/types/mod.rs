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

mod body;
pub(crate) mod decl;
mod literal;
mod rules;
mod shape;
mod signatures;
mod table;
mod ty;

pub use body::body_problems;
pub use decl::type_problems;
pub use rules::{Const, Num, Rules};
pub use shape::{FieldsBase, Shape, type_shapes};
pub use signatures::{Callables, Sig, callables};
pub use table::{Def, FieldDef, TypeTable, VariantDef, type_table};
pub use ty::{BASES, Prim, Ty, TypeRef, container_arity};
