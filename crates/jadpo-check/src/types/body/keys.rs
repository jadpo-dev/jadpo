//! Keys held as data (area 19, "A key is never a value"; chunk P9):
//! `crypto.new_key(as: <key type>)`, and what a value of a key type may do.
//! It goes into a key-type field, which keeps it sealed, or a `crypto`
//! call's `key:`, and may wait in a `var` on the way to one. Anywhere else
//! is `type.key_as_value`: code never reads, prints, compares or passes on
//! a key, and a setting's key stays `configuration.secret_flow`'s, read
//! nowhere but a `key:`.

use jadpo_diag::{Diagnostic, rule};
use jadpo_syntax::SyntaxKind;
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::facts::{Reach, node_key};
use super::{Body, LocalKind};
use crate::resolve::Want;
use crate::tree;
use crate::types::{FieldsBase, Prim, Ty, TypeRef};

impl Body<'_> {
    /// `crypto.new_key`'s `as:`: a key type written out.
    pub(super) fn key_type_argument(&mut self, value: &Expr, written: &str) -> Option<Ty> {
        let shown = tree::written(value.syntax());
        let target = match TypeRef::from_expr(value) {
            TypeRef::Path(names, args)
                if names
                    .first()
                    .is_some_and(|name| name.starts_with(|c: char| c.is_ascii_uppercase())) =>
            {
                if !self.resolved_by(|names, out| names.type_expr(value, Want::Type, out)) {
                    return None;
                }
                Some(self.table.resolve(&TypeRef::Path(names, args)))
            }
            _ => None,
        };
        if let Some(target) = &target
            && self
                .table
                .base_prim(target)
                .is_some_and(|prim| prim.is_key())
            && !self.table.is_nullable(target)
        {
            return Some(target.clone());
        }
        if target.is_none() && !matches!(value, Expr::NullableExpr(_)) {
            self.infer(value);
        }
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_MISMATCH,
                format!("`{written}`'s `as:` is a key type, not `{shown}`"),
            )
            .at(self.span(value), "not a key type")
            .reason(format!(
                "`{written}` makes 32 random bytes from the host's generator as a key, an `EncryptionKey`, a `PrivateKey` or a `SigningKey`, or a type built on one; no other value is made from them."
            ))
            .next("Write `as: EncryptionKey`, or use `random.token` for a value code may read"),
        );
        None
    }

    /// What `crypto.new_key` gives: the type `as:` names, however the call
    /// is reached.
    pub(super) fn new_key_result(&mut self, call: &ast::CallExpr, target: Option<Ty>) -> Ty {
        let Some(target) = target else {
            return Ty::Error;
        };
        self.facts
            .types
            .insert(node_key(call.syntax()), target.clone());
        target
    }

    /// Whether `value`, given to a `crypto` call's `key:`, holds a key as
    /// data: a `var` of a key type, or a key-type field of an entity's
    /// record. A parameter or a value of any other kind holds none, since
    /// only those two ever hold one.
    pub(super) fn holds_key_as_data(&self, value: &Expr) -> bool {
        match value {
            Expr::ParenExpr(paren) => paren
                .expr()
                .is_some_and(|inner| self.holds_key_as_data(&inner)),
            Expr::NameRef(name) => self
                .find(&tree::name_ref_text(name).unwrap_or_default())
                .is_some_and(|local| matches!(local.kind, LocalKind::Var | LocalKind::VarMut)),
            Expr::FieldExpr(field) => self.receiver_type(field).is_some_and(|receiver| {
                let structure = self.table.structure(receiver.strip_nullable());
                matches!(&structure, Ty::Named(name) if self.table.base(name) == Some(FieldsBase::Entity))
            }),
            _ => false,
        }
    }

    /// The type of what `field` is read from: `device` of `device.key`,
    /// which a dotted read doesn't work out on its own.
    pub(super) fn receiver_type(&self, field: &ast::FieldExpr) -> Option<Ty> {
        let receiver = field.expr()?;
        if let Expr::NameRef(name) = &receiver {
            return self
                .find(&tree::name_ref_text(name)?)
                .map(|local| local.ty.clone());
        }
        self.facts.types.get(&node_key(receiver.syntax())).cloned()
    }

    /// `value`, given to a `crypto` call's `key:` that takes `key`, as data:
    /// a key of that type, and nothing that may be `none`.
    pub(super) fn held_key_argument(&mut self, value: &Expr, given: &Ty, written: &str, key: Prim) {
        if self.table.base_prim(given) == Some(key) && !self.table.is_nullable(given) {
            return;
        }
        if given.has_error() {
            return;
        }
        let shown = tree::written(value.syntax());
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_MISMATCH,
                format!("`{written}`'s `key:` takes a `{}`, not `{shown}`", key.name()),
            )
            .at(self.span(value), format!("this is `{given}`"))
            .reason(format!(
                "Each `crypto` operation runs with one kind of key, and `{written}` runs with a `{}`, which must be there when it runs.",
                key.name()
            ))
            .next(format!(
                "Give a `{}`, reading a `none` one only once it has been checked",
                key.name()
            )),
        );
    }

    /// Every value of a key type in the callable, once it has been read:
    /// `type.key_as_value` for each that isn't going where a key may go.
    pub(super) fn keys_held(&mut self, syntax: &jadpo_syntax::SyntaxNode) {
        for node in syntax.descendants() {
            let Some(expr) = Expr::cast(node.clone()) else {
                continue;
            };
            let at = node_key(&node);
            let Some(ty) = self.facts.types.get(&at) else {
                continue;
            };
            if !self.table.base_prim(ty).is_some_and(|prim| prim.is_key()) {
                continue;
            }
            // A setting's read is `configuration.secret_flow`'s, a key built
            // from a value `type.constructor_input`'s, and a bracket holds
            // what is inside it.
            if self.facts.key_sinks.contains(&at)
                || is_setting_read(&expr)
                || matches!(self.facts.reaches.get(&at), Some(Reach::Construct(_)))
                || matches!(
                    node.parent().map(|parent| parent.kind()),
                    Some(SyntaxKind::PAREN_EXPR)
                )
                || waits_in_a_var(&node)
                || compared_with_none(&node)
            {
                continue;
            }
            let shown = tree::written(&node);
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_KEY_AS_VALUE,
                    format!("`{shown}` is a key, read as a value"),
                )
                .at(self.span(&expr), "a key, read here")
                .reason("A key held as data is opaque: code never reads, prints, compares, returns or passes on a key, so it goes only into a key-type field, which keeps it sealed, or a `crypto` call's `key:`.")
                .next(format!(
                    "Give `{shown}` only as a `crypto` call's `key:`, or set a key-type field to it"
                )),
            );
        }
    }
}

/// Whether `node` is the value a `var` starts with.
fn waits_in_a_var(node: &jadpo_syntax::SyntaxNode) -> bool {
    node.parent()
        .is_some_and(|parent| parent.kind() == SyntaxKind::VAR_STMT)
}

/// Whether `node` is compared with `none`, which says only whether a field
/// holds a key.
fn compared_with_none(node: &jadpo_syntax::SyntaxNode) -> bool {
    let Some(binary) = node.parent().and_then(ast::BinExpr::cast) else {
        return false;
    };
    let is_none = |side: Option<Expr>| {
        side.is_some_and(|side| {
            side.syntax() != node
                && matches!(
                    crate::types::Const::from_expr(&side),
                    crate::types::Const::None
                )
        })
    };
    binary
        .op()
        .is_some_and(|op| matches!(op.text(), "==" | "!="))
        && (is_none(binary.lhs()) || is_none(binary.rhs()))
}

/// `config.<name>`.
fn is_setting_read(value: &Expr) -> bool {
    tree::dotted(value).is_some_and(|(first, rest)| {
        tree::name_ref_text(&first).as_deref() == Some("config") && rest.len() == 1
    })
}
