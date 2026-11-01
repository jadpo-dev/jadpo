//! Encrypted fields (area 15, "Encrypted fields"; area 19, "Encrypted
//! fields", "A key is never a value" and "How a secret is kept"; chunk P9).
//!
//! A field written `encrypted: true`, and any field of a key type, a
//! `TotpSecret` or a `SecretText` whatever it says, is kept sealed with the
//! application's one `field_encryption_key:`. Its column is a `bytea` whose
//! bytes differ every time one value is sealed and which no SQL reads or
//! writes, so:
//!
//! - **`data.encrypted_field_invalid`**: what the database itself has to
//!   read can't be sealed: the `id`, a reference, a generated field, a
//!   `File`; and a secret kept sealed can't say `encrypted: false`.
//! - **`data.encrypted_field_used`**: a sealed field in a unique rule, an
//!   index, a filter, an ordering, a search, a lifecycle, a retention's
//!   `since:`, a value the database works out, or a failure's field, which
//!   is written to the log.
//! - **`data.field_encryption_key_required`**: the first sealed field of an
//!   application that names no key.
//! - **`data.field_encryption_key_invalid`**: `field_encryption_key:` reads
//!   anything but an `EncryptionKey` setting.

use std::sync::Arc;

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_std::standard::security;
use jadpo_syntax::ast::{AstNode, Expr};

use super::{EntitySpans, FieldModel, Reader, data_model, entity_names, entity_read};
use crate::lookups::{Lookup, Name};
use crate::tree::{self, dotted, node_span};
use crate::types::{Const, Ty, TypeView};

/// What a sealed field's bytes hold once opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Holds {
    /// A value of any stored type, as its contract's JSON.
    Value,
    /// A secret value's text, by its standard type: `TotpSecret`,
    /// `SecretText`.
    Secret(&'static str),
    /// A key's 32 bytes.
    Key,
}

/// The sealed form of a field of type `ty` that is kept sealed whatever it
/// says: a key type, a `TotpSecret` or a `SecretText`, directly or through
/// a type built on it, never inside a structure.
pub(crate) fn kept_sealed(lookup: &dyn TypeView, ty: &Ty) -> Option<Holds> {
    if lookup.base_prim(ty).is_some_and(|prim| prim.is_key()) {
        return Some(Holds::Key);
    }
    crate::types::secrets::secret_of(lookup, ty)
        .filter(|secret| security::stored_encrypted_later(secret))
        .map(Holds::Secret)
}

/// A sealed field used where the database, or a log, would have to read it.
pub(crate) fn used(entity: &str, field: &str, what: &str, at: Span, next: String) -> Diagnostic {
    Diagnostic::new(
        &rule::DATA_ENCRYPTED_FIELD_USED,
        format!("{what} can't read `{entity}.{field}`, which is encrypted"),
    )
    .at(at, "kept sealed")
    .reason(format!(
        "`{entity}.{field}` is kept sealed: its column holds bytes only the application can open, which differ every time one value is sealed. So it is kept out of unique rules, indexes, filters, orderings, searches and logs, and nothing but an action's write can set it."
    ))
    .next(next)
}

/// A failure's field given a value read from a sealed field: what a failure
/// holds is written to the log (area 4, "Logging").
pub(crate) fn logged(entity: &str, field: &str, at: Span) -> Diagnostic {
    used(
        entity,
        field,
        "A failure, which is logged,",
        at,
        format!("Say which record it was instead, by its `id`, and leave `{field}` out"),
    )
}

impl Reader<'_> {
    /// What the block of a field of type `ty` says of sealing, with `kept`
    /// what the type keeps sealed whatever it says: how it is sealed, when
    /// it is.
    pub(super) fn sealing(
        &mut self,
        path: &str,
        entity: &str,
        name: &str,
        value: &Expr,
        ty: &Ty,
        kept: Option<Holds>,
    ) -> Option<Holds> {
        let written = match value {
            Expr::RecordExpr(record) => tree::members(record.member_list())
                .into_iter()
                .find(|(key, _)| tree::name_text(key).as_deref() == Some("encrypted")),
            _ => None,
        };
        let told = written
            .as_ref()
            .and_then(|(_, member)| member.value())
            .map(|value| Const::from_expr(&value));
        match (&told, &written) {
            (Some(Const::Bool(_)) | None, _) => {}
            (Some(_), Some((key, member))) => {
                let span = member.value().map_or_else(
                    || node_span(path, key.syntax()),
                    |v| node_span(path, v.syntax()),
                );
                self.out.push(
                    Diagnostic::new(
                        &rule::DATA_ENCRYPTED_FIELD_INVALID,
                        format!("`{entity}.{name}`'s `encrypted:` takes `true` or `false`"),
                    )
                    .at(span, "not `true` or `false`")
                    .reason("`encrypted: true` keeps a field sealed with the application's `field_encryption_key:`, which it opens when the record is loaded.")
                    .next("Write `encrypted: true`, or remove it"),
                );
                return kept;
            }
            _ => {}
        }
        if let Some(kept) = kept {
            if told == Some(Const::Bool(false))
                && let Some((key, _)) = &written
            {
                let shown = ty.strip_nullable();
                self.out.push(
                    Diagnostic::new(
                        &rule::DATA_ENCRYPTED_FIELD_INVALID,
                        format!("`{entity}.{name}` can't turn `encrypted:` off"),
                    )
                    .at(node_span(path, key.syntax()), "always sealed")
                    .reason(format!(
                        "A `{shown}` is a secret, so it is never stored as itself: a key type, a `TotpSecret` and a `SecretText` are always kept sealed with the application's `field_encryption_key:`."
                    ))
                    .next(format!("Remove `encrypted: false` from `{name}`")),
                );
            }
            return Some(kept);
        }
        if told != Some(Const::Bool(true)) {
            return None;
        }
        if let Some((offending, what)) = crate::types::json_form::no_json_form(
            self.lookup,
            ty,
            &mut std::collections::BTreeSet::new(),
            crate::types::json_form::JsonUse::Store,
        ) {
            self.out.push(
                Diagnostic::new(
                    &rule::DATA_ENCRYPTED_FIELD_INVALID,
                    format!("`{entity}.{name}` can't be kept sealed"),
                )
                .at(
                    node_span(path, value.syntax()),
                    format!("`{offending}` has no stored form"),
                )
                .reason(format!(
                    "A sealed field is kept as its type's JSON, sealed, so it holds what a structured field may hold; `{offending}`, {what}."
                ))
                .next(format!("Store what `{entity}.{name}` needs in fields of its own")),
            );
            return None;
        }
        Some(Holds::Value)
    }

    /// What the database itself has to read of a field can't be sealed: its
    /// `id`, a reference, a generated value, a `File`. `value` is the
    /// field's type and block, as written.
    pub(super) fn sealed_holds_up(
        &mut self,
        path: &str,
        entity: &str,
        value: &Expr,
        field: &FieldModel,
    ) {
        let why = if field.name == "id" {
            Some((
                "every record's `id`",
                "A sealed value is bound to its record's `id`, which the database also looks every record up by.",
            ))
        } else if field.reference.is_some() {
            Some((
                "a reference",
                "A reference is a foreign key, which the database itself compares.",
            ))
        } else if field.generated.is_some() {
            Some((
                "a generated field",
                "A generated field is set from the operation's clock in the statement itself, and read by the database.",
            ))
        } else if field.file.is_some()
            || matches!(
                self.lookup.structure(field.ty.strip_nullable()),
                Ty::Opaque(held, args) if held == jadpo_std::files::FILE && args.is_empty()
            )
        {
            Some((
                "a `File`",
                "A `File`'s column refers to bytes kept in a store of files, and the database counts who holds them.",
            ))
        } else {
            None
        };
        if let Some((what, reason)) = why {
            self.out.push(
                Diagnostic::new(
                    &rule::DATA_ENCRYPTED_FIELD_INVALID,
                    format!("`{entity}.{}` can't be kept sealed", field.name),
                )
                .at(
                    node_span(path, value.syntax()),
                    format!("{what}, read by the database"),
                )
                .reason(reason)
                .next(format!("Remove `encrypted:` from `{}`", field.name)),
            );
        }
    }
}

/// `application { field_encryption_key: config.<name> }`, read: the
/// `EncryptionKey` setting every sealed field is sealed with.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FieldKeyRead {
    /// The setting it reads.
    pub setting: Option<String>,
    /// Whether `application { }` writes one at all: one that doesn't read
    /// is reported as itself, not as a missing key.
    pub written: bool,
    /// Where it is written.
    pub span: Option<Span>,
    pub problems: Vec<Diagnostic>,
}

/// The project's `field_encryption_key:`, with
/// `data.field_encryption_key_invalid` when it doesn't read as one.
#[salsa::tracked(returns(ref))]
pub fn field_key(db: &dyn Db, project: Project) -> Arc<FieldKeyRead> {
    let mut out = FieldKeyRead::default();
    let declare = "Declare `field_encryption_key: EncryptionKey` in `config.jadpo`, and write `field_encryption_key: config.field_encryption_key`";
    for found in crate::config::singletons(db, project, "application") {
        for (key, member) in tree::members(found.list.clone()) {
            if tree::name_text(&key).as_deref() != Some("field_encryption_key") {
                continue;
            }
            out.written = true;
            let path = found.path.as_str();
            let value = member.value();
            let span = value.as_ref().map_or_else(
                || node_span(path, member.syntax()),
                |value| node_span(path, value.syntax()),
            );
            out.span = Some(node_span(path, key.syntax()));
            let read = value
                .as_ref()
                .and_then(dotted)
                .filter(|(first, rest)| {
                    tree::name_ref_text(first).as_deref() == Some("config") && rest.len() == 1
                })
                .and_then(|(_, rest)| rest.first().and_then(tree::name_ref_text));
            let Some(read) = read else {
                let literal = matches!(value, Some(Expr::Literal(_)));
                let (message, label) = if literal {
                    (
                        "An encryption key is written in the source",
                        "a secret in source",
                    )
                } else {
                    (
                        "`field_encryption_key:` doesn't read a setting",
                        "not `config.<name>`",
                    )
                };
                out.problems.push(
                    Diagnostic::new(&rule::DATA_FIELD_ENCRYPTION_KEY_INVALID, message)
                        .at(span, label)
                        .reason("The key encrypted fields are sealed with comes only from secret configuration (area 10): an `EncryptionKey` setting, read as `config.<name>`.")
                        .next(declare),
                );
                continue;
            };
            let config = crate::config::config(db, project);
            if let Some(setting) = config.setting(&read) {
                let lookup = Lookup::new(db, project);
                if lookup.base_prim(&setting.ty) != Some(jadpo_std::Builtin::EncryptionKey) {
                    out.problems.push(
                        Diagnostic::new(
                            &rule::DATA_FIELD_ENCRYPTION_KEY_INVALID,
                            format!("`field_encryption_key:` reads `{read}`, which isn't an `EncryptionKey`"),
                        )
                        .at(span, format!("`{read}` is `{}`", setting.ty))
                        .reason("Encrypted fields are sealed with an `EncryptionKey` setting: 32 random bytes, written as 43 Base64url characters, and replaced by moving the old value to the variable with `_PREVIOUS` after its name (area 10). A `SigningKey` is text of any length, so it isn't one.")
                        .next(format!("Declare `{read}: EncryptionKey` in `config.jadpo`")),
                    );
                    continue;
                }
            }
            out.setting = Some(read);
        }
    }
    Arc::new(out)
}

/// `data.field_encryption_key_required` at the first sealed field of an
/// application that names no key: the entity's name order, then its
/// fields'.
#[salsa::tracked(returns(ref))]
pub fn key_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let key = field_key(db, project);
    let mut out = key.problems.clone();
    if key.written {
        return out;
    }
    for name in entity_names(db, project).iter() {
        let Some(read) = entity_read(db, project, Name::new(db, name.clone())) else {
            continue;
        };
        let spans: &EntitySpans = &read.spans;
        for (field, span) in read.model.fields.iter().zip(&spans.fields) {
            if field.sealed.is_none() {
                continue;
            }
            out.push(
                Diagnostic::new(
                    &rule::DATA_FIELD_ENCRYPTION_KEY_REQUIRED,
                    format!(
                        "`{name}.{}` is kept sealed, but `application {{ }}` names no key to seal it with",
                        field.name
                    ),
                )
                .at(span.clone(), "kept sealed")
                .reason(format!(
                    "An encrypted field, a key-type field, a `TotpSecret` and a `SecretText` are kept sealed with the application's one key, which `application {{ field_encryption_key: }}` names, and `{name}.{}` is the first that needs it. Jadpo won't make up a secret for you: losing this key loses every value sealed with it for good, so it is a new value each environment has to be given and keep safe.",
                    field.name
                ))
                .next("Add `field_encryption_key: config.field_encryption_key` to `application { }` in `app.jadpo`, declare `field_encryption_key: EncryptionKey` in `config.jadpo`, and run `jadpo config set field_encryption_key --generate`"),
            );
            return out;
        }
    }
    out
}

/// Whether the project keeps anything sealed.
pub fn any_sealed(db: &dyn Db, project: Project) -> bool {
    data_model(db, project)
        .entities
        .iter()
        .any(|entity| entity.fields.iter().any(|field| field.sealed.is_some()))
}

/// `entity`'s sealed field called `field`, read as a record's field of
/// the entity whose type is `ty`: what a failure's field is checked for.
pub(crate) fn is_sealed(db: &dyn Db, project: Project, entity: &str, field: &str) -> bool {
    entity_read(db, project, Name::new(db, entity.to_string()))
        .as_ref()
        .is_some_and(|read| {
            read.model
                .fields
                .iter()
                .any(|found| found.name == field && found.sealed.is_some())
        })
}
