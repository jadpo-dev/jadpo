//! What `json.read` and `json.write` add to an ordinary standard-library
//! call (area 19, "Data formats", question 14): `json.read`'s `as:` is a
//! type written out, any type an `Input` may hold, and `json.write`'s
//! value is any value an `Output` may hold, each read and written in the
//! wire's forms (areas 2, 7 and 10). There is no JSON value of no type.

use std::collections::BTreeSet;

use jadpo_diag::{Diagnostic, rule};
use jadpo_std::standard::standard_type;
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::super::json_form::{JsonUse, no_json_form};
use super::super::shape::FieldsBase;
use super::super::table::Def;
use super::super::ty::{Prim, Ty, TypeRef};
use super::{Body, Fallible, node_key};
use crate::resolve::Want;
use crate::tree;

/// What a document can be read as and a value written from, after "any
/// type an `Input` may hold": the reason a type that isn't one gives.
const HOLDS: &str = "values built on the built-in and standard types, enums, and objects, inputs and outputs made of them, in lists, sets and maps";

impl Body<'_> {
    /// `json.read`'s `as:`: the type written out, when it is one a
    /// document can be read as; what is wrong is reported.
    pub(super) fn json_type_argument(&mut self, value: &Expr, written: &str) -> Option<Ty> {
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
        let Some(target) = target else {
            if !super::is_literal(value) && !matches!(value, Expr::NullableExpr(_)) {
                self.infer(value);
            }
            let shown = tree::written(value.syntax());
            let (reason, next) = match value {
                Expr::NullableExpr(_) => (
                    format!(
                        "`{written}` reads a document as a value of the type `as:` names, never `none`; a document that may be absent is a `Text?` read only when it is there."
                    ),
                    "Write the type without `?`".to_string(),
                ),
                _ => (
                    format!(
                        "`{written}` reads a document as the type `as:` names, which is written out, such as `as: OrderImport`: a value can't say which type to read."
                    ),
                    "Write the type itself, such as `as: OrderImport`".to_string(),
                ),
            };
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_MISMATCH,
                    format!("`{written}`'s `as:` is a type written out, not `{shown}`"),
                )
                .at(self.span(value), "not a type")
                .reason(reason)
                .next(next),
            );
            return None;
        };
        if target.has_error() {
            return None;
        }
        if let Some((offending, what)) = self.unwritable(&target, &mut BTreeSet::new(), true) {
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_MISMATCH,
                    format!("`{written}` can't read a document as `{target}`"),
                )
                .at(
                    self.span(value),
                    format!("`{offending}` has no JSON form here"),
                )
                .reason(format!(
                    "`{written}` reads a document as any type an `Input` may hold: {HOLDS}; `{offending}` is {what}."
                ))
                .next("Read the document as a type made of those; keep a payload whose shape isn't known as `Text`"),
            );
            return None;
        }
        // The type is given to the operation as its name, as `text.parse`'s
        // is; the plan gives the call its codec.
        let key = node_key(value.syntax());
        self.facts.types.insert(key, Ty::Prim(Prim::Text));
        self.facts.formats.insert(key, target.to_string());
        Some(target)
    }

    /// `json.read`'s result: the type `as:` names, or `Error` once what is
    /// wrong with `as:` is reported, when what the call can fail with
    /// isn't known.
    pub(super) fn json_read_result(&mut self, call: &ast::CallExpr, target: Option<Ty>) -> Ty {
        let key = node_key(call.syntax());
        match target {
            Some(target) => {
                // The call's site is the type it gives, however the call is
                // reached (an `attempt` checks its call without inferring
                // it), which the plan reads its codec from.
                self.facts.types.insert(key, target.clone());
                target
            }
            None => {
                self.facts.constructions.insert(key, Fallible::Unknown);
                Ty::Error
            }
        }
    }

    /// `crypto.verify_jwt`'s `as:` (chunk 5.5r4): a type a document can be
    /// read as, which is an object type of the application's, each field
    /// it has for one of the seven registered claims built on that claim's
    /// type as `JwtClaims` holds it; what is wrong is reported.
    pub(super) fn claims_type_argument(&mut self, value: &Expr, written: &str) -> Option<Ty> {
        let target = self.json_type_argument(value, written)?;
        let fields = match &target {
            Ty::Named(name) if standard_type(name).is_none() => match self.table.def(name) {
                Some(Def::Fields {
                    base: FieldsBase::Object,
                    fields,
                    ..
                }) => Some(fields.clone()),
                _ => None,
            },
            _ => None,
        };
        let Some(fields) = fields else {
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_MISMATCH,
                    format!("`{written}`'s `as:` is an object type of the application's, not `{target}`"),
                )
                .at(self.span(value), "not an `Object` of the application's")
                .reason(format!(
                    "`{written}` with `as:` reads a token's claims into an `Object` the application declares, a field for each claim it reads; the claims it doesn't name are ignored."
                ))
                .next("Declare an `Object` with a field for each claim, such as `type SignIn = Object { email: Email }`"),
            );
            return None;
        };
        // A registered claim it names is given as `JwtClaims` holds it, so
        // its field is built on that claim's type (decided by the lead
        // session for chunk 5.5r4).
        let held = |claim: &str| match claim {
            "iss" | "sub" | "jti" => Some(("text", Ty::Prim(Prim::Text))),
            "exp" | "nbf" | "iat" => Some(("an `Instant`", Ty::Prim(Prim::Instant))),
            "aud" => Some(("a list of text", Ty::List(Box::new(Ty::Prim(Prim::Text))))),
            _ => None,
        };
        for field in &fields {
            let Some((what, expected)) = held(&field.name) else {
                continue;
            };
            let found = match self.table.structure(&field.declared) {
                Ty::List(item) => Ty::List(Box::new(self.table.structure(&item))),
                other => other,
            };
            if found == expected || found.has_error() {
                continue;
            }
            let claim = &field.name;
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_MISMATCH,
                    format!("`{target}.{claim}` is a registered claim, which is {what}, not `{}`", field.declared),
                )
                .at(self.span(value), format!("`{target}.{claim}` isn't {what}"))
                .reason(format!(
                    "`{written}` checks the seven claims RFC 7519 registers itself and gives a type `as:` names each it has a field for as `JwtClaims` holds it: `iss`, `sub` and `jti` as text, `aud` as a list of text, and `exp`, `nbf` and `iat` as an `Instant`."
                ))
                .next(format!(
                    "Declare `{claim}` as {what}, or a type built on it, with `?` where a token may leave it out"
                )),
            );
            return None;
        }
        Some(target)
    }

    /// `crypto.verify_jwt`'s result with `as:`: the type it names, or
    /// `Error` once what is wrong with `as:` is reported (chunk 5.5r4).
    pub(super) fn claims_result(&mut self, call: &ast::CallExpr, target: Option<Ty>) -> Ty {
        let Some(target) = target else {
            return Ty::Error;
        };
        // The call's site is the type it gives, however the call is
        // reached, which the plan reads its codec from.
        self.facts
            .types
            .insert(node_key(call.syntax()), target.clone());
        target
    }

    /// `json.write`'s value: one an `Output` may hold.
    pub(super) fn json_value(&mut self, value: &Expr, ty: &Ty, written: &str) {
        if ty.has_error() {
            return;
        }
        let Some((offending, what)) = self.unwritable(ty, &mut BTreeSet::new(), false) else {
            return;
        };
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_MISMATCH,
                format!("`{written}` can't write `{ty}` as JSON"),
            )
            .at(self.span(value), format!("`{offending}` has no JSON form here"))
            .reason(format!(
                "`{written}` writes any value an `Output` may hold: {HOLDS}; `{offending}` is {what}."
            ))
            .next("Write a value made of those, building an `Output` with the fields to send"),
        );
    }

    /// The first type inside `ty` that has no JSON form here, with what it
    /// is, after "is": a failure, an event, an entity, a secret, `Unit`;
    /// `reading` for `json.read`, which reads a secret value but never
    /// writes one.
    fn unwritable(
        &self,
        ty: &Ty,
        seen: &mut BTreeSet<String>,
        reading: bool,
    ) -> Option<(String, String)> {
        let used = if reading {
            JsonUse::Read
        } else {
            JsonUse::Write
        };
        no_json_form(self.table, ty, seen, used)
    }
}
