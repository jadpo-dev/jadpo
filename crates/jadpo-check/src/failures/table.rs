//! Every failure the project declares, by name and by public code, lowered
//! without positions so an edit inside a body never rebuilds it.

use std::collections::BTreeMap;

use jadpo_db::{Db, File, Project};
use jadpo_syntax::ast::{self, AstNode, Decl};

use crate::case::to_snake;
use crate::tree;
use crate::types::Const;

/// One declared failure.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FailureInfo {
    pub name: String,
    /// The kind, when it is written `FailureKind.x`: `x`.
    pub kind: Option<String>,
    /// Its public code: `code:` when given, otherwise the name in
    /// lower_snake_case (question 1).
    pub code: String,
    pub explicit_code: bool,
    /// The file it is declared in, and its position among that file's
    /// declarations.
    pub path: String,
    pub index: u32,
}

/// Whether a type declaration is a failure: `type X = Failure { ... }`.
pub fn is_failure_decl(decl: &ast::TypeDecl) -> bool {
    decl.base()
        .is_some_and(|base| tree::written(base.syntax()) == "Failure")
}

/// The failures one file declares, in source order.
#[salsa::tracked(returns(ref))]
pub fn failure_decls(db: &dyn Db, file: File) -> Vec<FailureInfo> {
    let Some(source) = tree::source(db, file) else {
        return Vec::new();
    };
    let path = file.path(db);
    source
        .decls()
        .enumerate()
        .filter_map(|(index, decl)| {
            let Decl::TypeDecl(decl) = decl else {
                return None;
            };
            if !is_failure_decl(&decl) {
                return None;
            }
            let name = decl.name().as_ref().and_then(tree::name_text)?;
            let mut kind = None;
            let mut code = None;
            for (key, member) in tree::members(decl.member_list()) {
                let value = member
                    .value()
                    .map_or(Const::Other, |v| Const::from_expr(&v));
                match (tree::name_text(&key).as_deref(), value) {
                    (Some("kind"), Const::Path(path))
                        if kind.is_none() && path.len() == 2 && path[0] == "FailureKind" =>
                    {
                        kind = Some(path[1].clone());
                    }
                    (Some("code"), Const::Str(text)) if code.is_none() => code = Some(text),
                    _ => {}
                }
            }
            Some(FailureInfo {
                explicit_code: code.is_some(),
                code: code.unwrap_or_else(|| to_snake(&name)),
                name,
                kind,
                path: path.clone(),
                index: index as u32,
            })
        })
        .collect()
}

/// The project's failures.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct FailureTable {
    /// By name; the first declaration wins when a name is declared twice,
    /// which resolution reports (area 1).
    by_name: BTreeMap<String, FailureInfo>,
    /// By public code: every failure using it, in file and source order.
    by_code: BTreeMap<String, Vec<FailureInfo>>,
}

impl FailureTable {
    pub fn get(&self, name: &str) -> Option<&FailureInfo> {
        self.by_name.get(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.by_name.contains_key(name)
    }

    /// The failures using `code`, first declared first.
    pub fn with_code(&self, code: &str) -> &[FailureInfo] {
        self.by_code.get(code).map_or(&[], Vec::as_slice)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.by_name.keys().map(String::as_str)
    }
}

#[salsa::tracked(returns(ref))]
pub fn failure_table(db: &dyn Db, project: Project) -> FailureTable {
    let mut table = FailureTable::default();
    for &file in project.files(db) {
        for info in failure_decls(db, file) {
            table
                .by_name
                .entry(info.name.clone())
                .or_insert_with(|| info.clone());
            table
                .by_code
                .entry(info.code.clone())
                .or_default()
                .push(info.clone());
        }
    }
    table
}
