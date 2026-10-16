//! Type names: a dotted name written as a type or a value resolved to the
//! type, entity, field or variant it means, against the same tables the
//! checker reads. Typed bodies record what their names mean through it,
//! and a query's declarative block, which has no typed body, is scanned
//! with it. Calls are resolved once, by the body check.

use std::collections::BTreeSet;

use jadpo_db::ItemKind;
use jadpo_graph::Key;
use jadpo_syntax::SyntaxKind::*;
use jadpo_syntax::SyntaxNode;
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::{declaration_key, field_key, variant_key};
use crate::case::starts_upper;
use crate::lookups::NameView;
use crate::tree::{self, dotted};
use crate::types::TypeView;

pub(crate) struct Resolver<'a> {
    pub symbols: &'a dyn NameView,
    pub table: &'a dyn TypeView,
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
            .declared(first)
            .iter()
            .find(|symbol| symbol.library.is_none())?;
        if !matches!(symbol.kind, ItemKind::Type(_) | ItemKind::Entity) {
            return None;
        }
        let host = declaration_key(self.symbols, first)?;
        for n in (1..=rest.len()).rev() {
            if self.table.field(first, &rest[..n]).is_some() {
                return Some(field_key(self.symbols, first, &rest[..n]));
            }
        }
        if let Some(variant) = rest.first()
            && self
                .table
                .variants(first)
                .is_some_and(|variants| variants.iter().any(|v| &v.name == variant))
        {
            return Some(variant_key(self.symbols, first, variant));
        }
        Some(host)
    }

    /// Every type, entity, field and variant `node` names, leaving out the
    /// subtrees `skip` picks.
    pub fn uses(&self, node: &SyntaxNode, skip: &dyn Fn(&SyntaxNode) -> bool) -> BTreeSet<Key> {
        let mut out = Vec::new();
        self.visit(node, skip, &mut out);
        out.into_iter().map(|(_, key)| key).collect()
    }

    /// Every type, entity, field and variant `node` names, each with the
    /// node that names it.
    pub fn named(&self, node: &SyntaxNode) -> Vec<(SyntaxNode, Key)> {
        let mut out = Vec::new();
        self.visit(node, &|_| false, &mut out);
        out
    }

    fn visit(
        &self,
        node: &SyntaxNode,
        skip: &dyn Fn(&SyntaxNode) -> bool,
        out: &mut Vec<(SyntaxNode, Key)>,
    ) {
        let names: Option<Vec<String>> = match node.kind() {
            PATH => ast::Path::cast(node.clone()).map(|path| {
                path.name_refs()
                    .filter_map(|name| tree::name_ref_text(&name))
                    .collect()
            }),
            // A name inside a type's path is part of that path, read above.
            NAME_REF | FIELD_EXPR
                if outermost(node) && node.parent().is_none_or(|parent| parent.kind() != PATH) =>
            {
                Expr::cast(node.clone()).and_then(|expr| {
                    let (first, rest) = dotted(&expr)?;
                    std::iter::once(&first)
                        .chain(rest.iter())
                        .map(tree::name_ref_text)
                        .collect()
                })
            }
            _ => None,
        };
        if let Some(key) = names.and_then(|names| self.path(&names)) {
            out.push((node.clone(), key));
        }
        for child in node.children() {
            if !skip(&child) {
                self.visit(&child, skip, out);
            }
        }
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
