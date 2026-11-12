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

/// Whether a value of `ty` holds a `File`, however deep: what a route's
/// answer sends as a signed link (area 16, "Downloading").
pub fn holds_file(lookup: &crate::lookups::Lookup<'_>, ty: &ty::Ty) -> bool {
    decl::held_file(lookup, ty, &mut Vec::new()).is_some()
}

/// The members of object type `ty`, by their spellings on the wire, that a
/// body of parts sends as files (area 11, "How provider calls are made";
/// chunk K2): each field holding `Bytes`, `FileContents` or a `File`, or
/// one of them that may be `none`.
pub fn file_members(lookup: &crate::lookups::Lookup<'_>, ty: &ty::Ty) -> Vec<String> {
    use table::TypeView;
    let ty::Ty::Named(name) = lookup.structure(ty) else {
        return Vec::new();
    };
    let Some(fields) = lookup.fields(&name) else {
        return Vec::new();
    };
    fields
        .iter()
        .filter(|field| {
            lookup.base_prim(&field.declared) == Some(ty::Prim::Bytes)
                || matches!(
                    lookup.structure(&field.declared),
                    ty::Ty::Opaque(held, args) if args.is_empty()
                        && (held == jadpo_std::files::FILE || held == jadpo_std::files::FILE_CONTENTS)
                )
        })
        .map(|field| field.wire_name.clone().unwrap_or_else(|| field.name.clone()))
        .collect()
}

pub(crate) mod across;
mod body;
mod connections;
pub(crate) mod decl;
mod facts;
mod formats;
pub(crate) mod json_form;
pub(crate) mod literal;
mod loading;
mod outside;
mod read_only;
mod recursion;
pub(crate) mod rules;
pub(crate) mod secrets;
mod shape;
mod signatures;
mod steps;
mod stores;
pub(crate) mod table;
mod ty;

pub(crate) use body::transition_key;
pub(crate) use body::{BodyView, LocalKind, ViewLocal, members_of};
pub use body::{
    Callable, Checked, FailureSet, FailureSets, Fallible, backfill_callables, body_problems,
    call_problems, callables_of, failure_set, failure_sets, hole_problems, rule_callables,
    template_body, template_reads, typed_body,
};
pub(crate) use body::{batch_fields, sign_in_object_fields};
pub use connections::called_connections;
pub use decl::type_problems;

/// Whether `name` is `Feature` or `FeatureCollection` (area 20), whose
/// properties type argument is followed wherever what a type holds is.
pub(crate) fn is_feature_type(name: &str) -> bool {
    name == jadpo_std::standard::geo::FEATURE
        || name == jadpo_std::standard::geo::FEATURE_COLLECTION
}
pub use facts::{
    BodyFacts, Fact, Write, body_facts, fact_of, facts_at, named, resolve, run_places,
};
pub use loading::{Decided, Loading, Loads, file_loading, has_isolation, isolation};
pub use outside::{Outside, call_order_problems, outside_calls};
pub use read_only::{Change, NotReadOnly, not_read_only, read_only_problems};
pub use recursion::{Cycles, action_calls, action_cycles, recursion_problems};
pub use rules::{CONSTRAINTS, Const, FIELD_SETTINGS, Num, Rules, show as show_number};
pub use shape::{
    COPY_LINES, DECODE, ENCODE, FieldsBase, Line, REPLY, Shape, ShapeKind, WIRE, WIRE_NAME,
    is_wire_name, looks_like_type, type_shapes, variant_block,
};
pub use signatures::{Callables, Sig, callables};
pub use steps::{
    MOST_STEPS, Steps, StepsRead, action_steps, first_several, record_plans, record_store,
    resumable_runs, step_problems, steps_settings,
};
pub use stores::{
    Reach as StoreReach, batch_store_problems, batch_stores, cancelling, file_run_problems,
    file_run_stores, store_problems, store_reaches, unkept_responses, used_stores, written_stores,
};
pub use table::{
    Def, EMAIL_FIELDS, FieldDef, PARTIAL_FIELDS, TypeTable, TypeView, VariantDef, WrittenCoding,
    standard_fields, type_table,
};
pub use ty::{BASES, Prim, TIME_FORMAT, Ty, TypeRef, container_arity, shown};
