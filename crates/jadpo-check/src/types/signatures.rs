//! The signatures of every function, action, query, fixture and library
//! function, with their types resolved, for checking calls (area 2's
//! compatibility rules applied to arguments and results).

use std::collections::BTreeMap;

use jadpo_db::{Db, ItemKind, Project, Signature, item_tree};

use super::table::type_table;
use super::ty::{Prim, Ty, TypeRef};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Sig {
    pub params: Vec<(String, Ty)>,
    pub result: Ty,
    /// The failures its `fails` list names, as written and in order
    /// (area 4). Callers handle these: the callable's own check makes sure
    /// the list is exactly what can leave it.
    pub fails: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Callables {
    /// Functions, actions, queries and fixtures, by name.
    pub plain: BTreeMap<String, Sig>,
    /// Library functions, by library and name.
    pub library: BTreeMap<(String, String), Sig>,
}

/// Every callable's signature. It reads item trees and the type table, so
/// an edit inside a body never recomputes it.
#[salsa::tracked(returns(ref))]
pub fn callables(db: &dyn Db, project: Project) -> Callables {
    let table = type_table(db, project);
    let sig = |signature: &Signature| Sig {
        params: signature
            .params
            .iter()
            .map(|param| {
                (
                    param.name.clone(),
                    table.resolve(&TypeRef::parse(&param.ty)),
                )
            })
            .collect(),
        result: signature
            .result
            .as_ref()
            .map_or(Ty::Prim(Prim::Unit), |result| {
                table.resolve(&TypeRef::parse(result))
            }),
        fails: signature.fails.clone(),
    };
    let mut out = Callables::default();
    for &file in project.files(db) {
        for item in &item_tree(db, file).items {
            match item.kind {
                ItemKind::Function | ItemKind::Action | ItemKind::Query | ItemKind::Fixture => {
                    out.plain
                        .entry(item.name.clone())
                        .or_insert_with(|| sig(&item.signature));
                }
                ItemKind::Library => {
                    for function in &item.functions {
                        out.library
                            .entry((item.name.clone(), function.name.clone()))
                            .or_insert_with(|| sig(&function.signature));
                    }
                }
                _ => {}
            }
        }
    }
    out
}
