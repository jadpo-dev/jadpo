//! What `json.read` and `json.write` add to an ordinary standard-library
//! call (area 19, "Data formats", question 14): `json.read`'s `as:` is a
//! type written out, any type an `Input` may hold, and `json.write`'s
//! value is any value an `Output` may hold, each read and written in the
//! wire's forms (areas 2, 7 and 10). There is no JSON value of no type.

use std::collections::BTreeSet;

use jadpo_diag::{Diagnostic, rule};
use jadpo_std::standard::standard_type;
use jadpo_syntax::ast::{self, AstNode, Expr};

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
        // A password's hash is stored and checked, never sent or read
        // (area 19, decided by the lead session for chunk 5.5q2).
        if let Some(hash) = self.password_hash(ty) {
            return Some((
                hash,
                "a password's hash, which is stored and checked but never leaves the server"
                    .to_string(),
            ));
        }
        // A secret value is never written out, though it is read, as a
        // request's `Input` reads one (area 19, chunk 5.5r2).
        if !reading && let Some(secret) = super::super::secrets::secret_of(self.table, ty) {
            return Some((
                ty.strip_nullable().to_string(),
                format!("a `{secret}`, a secret value, which is never written out"),
            ));
        }
        let found = self.table.structure(ty);
        match &found {
            Ty::Error | Ty::Unknown(_) | Ty::Var(_) => None,
            Ty::Prim(prim) => match prim {
                Prim::Unit => Some((found.to_string(), "no value at all".to_string())),
                Prim::ApiToken
                | Prim::ServiceKey
                | Prim::PostgresUrl
                | Prim::RedisUrl
                | Prim::SigningKey
                | Prim::EncryptionKey
                | Prim::PrivateKey => Some((
                    found.to_string(),
                    "a secret, which only a route's `Output` or the setting it is read from ever holds".to_string(),
                )),
                Prim::PluralCount => Some((
                    found.to_string(),
                    "a message's plural count, used only as a `Message` field".to_string(),
                )),
                _ => None,
            },
            Ty::List(item) | Ty::Set(item) | Ty::Nullable(item) => self.unwritable(item, seen, reading),
            Ty::Map(key, value) => self
                .keyless(key)
                .or_else(|| self.unwritable(key, seen, reading))
                .or_else(|| self.unwritable(value, seen, reading)),
            Ty::Named(name) | Ty::Opaque(name, _) => {
                if standard_type(name).is_some() {
                    return match &found {
                        Ty::Opaque(_, args) => {
                            args.iter().find_map(|arg| self.unwritable(arg, seen, reading))
                        }
                        _ => None,
                    };
                }
                if !seen.insert(name.clone()) {
                    return None;
                }
                match self.table.def(name) {
                    Some(Def::Fields { base, fields, .. }) => match base {
                        FieldsBase::Object | FieldsBase::Input | FieldsBase::Output => fields
                            .iter()
                            .find_map(|field| self.unwritable(&field.declared, seen, reading)),
                        other => Some((found.to_string(), fields_base(*other).to_string())),
                    },
                    Some(Def::Enum { variants }) => variants
                        .iter()
                        .flat_map(|variant| variant.fields.iter().flatten())
                        .find_map(|field| self.unwritable(&field.declared, seen, reading)),
                    // A refinement is followed to what it is built on.
                    Some(Def::Scalar { .. }) => None,
                    Some(Def::Format { .. }) => Some((
                        found.to_string(),
                        "a format for showing times, which holds no values".to_string(),
                    )),
                    // A standard-library enum (`Rounding`, `Zone`) is its
                    // variant's name.
                    None if jadpo_std::enums()
                        .iter()
                        .any(|(enum_name, _)| *enum_name == name.as_str()) =>
                    {
                        None
                    }
                    None => Some((
                        found.to_string(),
                        "a built-in type that crosses no boundary as JSON".to_string(),
                    )),
                }
            }
            other => Some((other.to_string(), "not a value".to_string())),
        }
    }
}

impl Body<'_> {
    /// A map's key type when it has no single text form, with what it is:
    /// a JSON object's keys are text, so a key is written as its canonical
    /// text (`text.from`) and read back by `text.parse`'s reader (area 19,
    /// chunk 5.5w2), and an object, a choice, a collection, an enum with
    /// data or a value that may be `none` has none.
    fn keyless(&self, key: &Ty) -> Option<(String, String)> {
        const WHY: &str = "and a map's key is sent as its canonical text, the text `text.from` writes, which it has none of";
        if self.table.representation(key).is_some() {
            return None;
        }
        let found = self.table.structure(key);
        let what = match &found {
            Ty::Named(name) | Ty::Opaque(name, _) => match standard_type(name) {
                Some(entry) if entry.text.is_some() => return None,
                Some(_) => "a standard type with no single text form",
                None => match self.table.def(name) {
                    Some(Def::Enum { variants }) if variants.iter().all(|v| v.fields.is_none()) => {
                        return None;
                    }
                    Some(Def::Enum { .. }) => "an enum whose variants carry data",
                    Some(Def::Fields { .. }) => "an object",
                    _ => return None,
                },
            },
            Ty::List(_) | Ty::Set(_) | Ty::Map(..) => "a collection",
            Ty::Nullable(_) => "a value that may be `none`",
            _ => return None,
        };
        Some((found.to_string(), format!("{what}, {WHY}")))
    }
}

/// What a type with fields that crosses no boundary as JSON is.
fn fields_base(base: FieldsBase) -> &'static str {
    match base {
        FieldsBase::Failure => "a failure, which reaches a client only as area 4's envelope",
        FieldsBase::Event => "an event, which is emitted, not written",
        FieldsBase::EmailMessage => "an email, which is sent, not written",
        FieldsBase::Message => "a message, translated text that is shown, not written",
        FieldsBase::Prompt => "a prompt, which is rendered for a model with `prompts.render`, not written",
        FieldsBase::Entity => {
            "an entity, whose records reach a client only through an `Output` that picks what it sends"
        }
        FieldsBase::Object | FieldsBase::Input | FieldsBase::Output => "an object",
    }
}

impl Body<'_> {
    /// The type as written when it is a `PasswordHash` or built on one.
    fn password_hash(&self, ty: &Ty) -> Option<String> {
        let mut current = ty.strip_nullable().clone();
        for _ in 0..16 {
            let Ty::Named(name) = &current else {
                return None;
            };
            if name == jadpo_std::standard::security::PASSWORD_HASH {
                return Some(ty.strip_nullable().to_string());
            }
            match self.table.def(name) {
                Some(super::super::table::Def::Scalar { parent, .. }) => {
                    current = parent.strip_nullable().clone();
                }
                _ => return None,
            }
        }
        None
    }
}
