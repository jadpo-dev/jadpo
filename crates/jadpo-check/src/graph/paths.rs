//! What a piece of code refers to, for the graph's `uses` and `calls`
//! edges: dotted names resolved against the symbol and type tables, the
//! way the checker resolves them.

use std::collections::BTreeSet;

use jadpo_db::{ItemKind, Symbols};
use jadpo_graph::Key;
use jadpo_syntax::SyntaxKind::*;
use jadpo_syntax::SyntaxNode;
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::{declaration_key, field_key, library_function_key, variant_key};
use crate::case::starts_upper;
use crate::tree::{self, dotted};
use crate::types::{Callables, TypeTable};

pub(super) struct Resolver<'a> {
    pub symbols: &'a Symbols,
    pub table: &'a TypeTable,
    pub callables: &'a Callables,
}

impl Resolver<'_> {
    /// The node a dotted name written as a type or a value means: the most
    /// specific of a type or entity, its field, its variant, or a variant's
    /// field. Names of the standard library and of nothing are `None`.
    pub fn path(&self, names: &[String]) -> Option<Key> {
        let (first, rest) = names.split_first()?;
        if !starts_upper(first) {
            return None;
        }
        let symbol = self
            .symbols
            .declarations
            .get(first)?
            .iter()
            .find(|symbol| symbol.library.is_none())?;
        if !matches!(symbol.kind, ItemKind::Type(_) | ItemKind::Entity) {
            return None;
        }
        let host = declaration_key(self.symbols, first)?;
        for n in (1..=rest.len()).rev() {
            if self.table.field(first, &rest[..n]).is_some() {
                return Some(field_key(first, &rest[..n]));
            }
        }
        if let Some(variant) = rest.first()
            && self
                .table
                .variants(first)
                .is_some_and(|variants| variants.iter().any(|v| &v.name == variant))
        {
            return Some(variant_key(first, variant));
        }
        Some(host)
    }

    /// Every type, entity, field and variant `node` names, leaving out the
    /// subtrees `skip` picks.
    pub fn uses(&self, node: &SyntaxNode, skip: &dyn Fn(&SyntaxNode) -> bool) -> BTreeSet<Key> {
        let mut out = BTreeSet::new();
        self.visit(node, skip, &mut out);
        out
    }

    fn visit(
        &self,
        node: &SyntaxNode,
        skip: &dyn Fn(&SyntaxNode) -> bool,
        out: &mut BTreeSet<Key>,
    ) {
        let names: Option<Vec<String>> = match node.kind() {
            PATH => ast::Path::cast(node.clone()).map(|path| {
                path.name_refs()
                    .filter_map(|name| tree::name_ref_text(&name))
                    .collect()
            }),
            NAME_REF | FIELD_EXPR if outermost(node) => Expr::cast(node.clone()).and_then(|expr| {
                let (first, rest) = dotted(&expr)?;
                std::iter::once(&first)
                    .chain(rest.iter())
                    .map(tree::name_ref_text)
                    .collect()
            }),
            _ => None,
        };
        if let Some(key) = names.and_then(|names| self.path(&names)) {
            out.insert(key);
        }
        for child in node.children() {
            if !skip(&child) {
                self.visit(&child, skip, out);
            }
        }
    }

    /// Every function, action, query, fixture and library function `node`
    /// calls; `library` is the library whose functions call each other
    /// bare.
    pub fn calls(&self, node: &SyntaxNode, library: Option<&str>) -> BTreeSet<Key> {
        node.descendants()
            .filter_map(ast::CallExpr::cast)
            .filter_map(|call| self.callee(&call.expr()?, library))
            .collect()
    }

    fn callee(&self, callee: &Expr, library: Option<&str>) -> Option<Key> {
        let (first, rest) = dotted(callee)?;
        let first = tree::name_ref_text(&first)?;
        if starts_upper(&first) {
            return None;
        }
        let rest: Vec<String> = rest.iter().filter_map(tree::name_ref_text).collect();
        match rest.as_slice() {
            [] => {
                if let Some(library) = library
                    && self
                        .callables
                        .library
                        .contains_key(&(library.to_string(), first.clone()))
                {
                    return Some(library_function_key(library, &first));
                }
                self.callable(
                    &first,
                    &[
                        ItemKind::Function,
                        ItemKind::Action,
                        ItemKind::Query,
                        ItemKind::Fixture,
                    ],
                )
            }
            [member] => {
                if self
                    .callables
                    .library
                    .contains_key(&(first.clone(), member.clone()))
                {
                    return Some(library_function_key(&first, member));
                }
                if self.symbols.domains.contains(&first) {
                    return self.callable(member, &[ItemKind::Action, ItemKind::Query]);
                }
                None
            }
            _ => None,
        }
    }

    fn callable(&self, name: &str, kinds: &[ItemKind]) -> Option<Key> {
        let symbol = self
            .symbols
            .declarations
            .get(name)?
            .iter()
            .find(|symbol| symbol.library.is_none())?;
        if !kinds.contains(&symbol.kind) {
            return None;
        }
        declaration_key(self.symbols, name)
    }
}

/// Whether a name or field read is the whole of its dotted chain, rather
/// than the inside of a longer one.
fn outermost(node: &SyntaxNode) -> bool {
    match node.parent() {
        Some(parent) if parent.kind() == FIELD_EXPR => ast::FieldExpr::cast(parent)
            .and_then(|field| field.expr())
            .is_none_or(|expr| expr.syntax() != node),
        _ => true,
    }
}
