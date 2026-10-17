//! The signatures of every function, action, query, fixture and library
//! function, with their types resolved, for checking calls (area 2's
//! compatibility rules applied to arguments and results).

use std::collections::BTreeMap;

use jadpo_db::{Db, ItemKind, Project, Signature, item_tree};

use super::table::{TypeView, type_table};
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
    /// Constants beside their use, by name, with their types (area 3).
    pub constants: BTreeMap<String, Ty>,
    /// Library constants, by library and name.
    pub library_constants: BTreeMap<(String, String), Ty>,
}

/// Every callable's signature. It reads item trees and the type table, so
/// an edit inside a body never recomputes it.
#[salsa::tracked(returns(ref))]
pub fn callables(db: &dyn Db, project: Project) -> Callables {
    let table = type_table(db, project);
    // A domain's contested private names are known by their domain too
    // (area 1, "Files and names").
    let contested = jadpo_db::contested(db, project);
    let sig = |signature: &Signature, canonical: &dyn Fn(&str) -> String| Sig {
        params: signature
            .params
            .iter()
            .map(|param| {
                (
                    param.name.clone(),
                    table.resolve(&TypeRef::parse(&param.ty).scoped(canonical)),
                )
            })
            .collect(),
        result: signature
            .result
            .as_ref()
            .map_or(Ty::Prim(Prim::Unit), |result| {
                table.resolve(&TypeRef::parse(result).scoped(canonical))
            }),
        fails: signature.fails.clone(),
    };
    let mut out = Callables::default();
    for &file in project.files(db) {
        let location = jadpo_db::location(db, project, file);
        let domain = location.ownership.domain();
        let canonical = |written: &str| contested.canonical(written, domain);
        for item in &item_tree(db, file).items {
            match item.kind {
                ItemKind::Function | ItemKind::Action | ItemKind::Query | ItemKind::Fixture => {
                    out.plain
                        .entry(canonical(&item.name))
                        .or_insert_with(|| sig(&item.signature, &canonical));
                }
                ItemKind::Constant => {
                    out.constants
                        .entry(canonical(&item.name))
                        .or_insert_with(|| sig(&item.signature, &canonical).result);
                }
                ItemKind::Library => {
                    for member in &item.functions {
                        let key = (item.name.clone(), member.name.clone());
                        if member.kind == ItemKind::Constant {
                            out.library_constants
                                .entry(key)
                                .or_insert_with(|| sig(&member.signature, &canonical).result);
                        } else {
                            out.library
                                .entry(key)
                                .or_insert_with(|| sig(&member.signature, &canonical));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    out
}
