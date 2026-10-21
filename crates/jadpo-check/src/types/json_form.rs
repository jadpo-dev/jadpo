//! Which types have a JSON form where a value crosses as JSON (area 19,
//! "Data formats"; area 5, "Storage types"): `json.read` reads any type an
//! `Input` may hold, `json.write` writes any value an `Output` may hold, and
//! a structured field is stored whole as `jsonb` holding its type's JSON
//! (chunk 5.9b), each through the route codec. One walk decides all three,
//! so a type the codec can't carry is refused the same way wherever it
//! would cross.

use std::collections::BTreeSet;

use jadpo_std::standard::standard_type;

use super::TypeView;
use super::shape::FieldsBase;
use super::table::Def;
use super::ty::{Prim, Ty};

/// Where a value crosses as JSON.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JsonUse {
    /// `json.read`, which reads a secret value but never writes one.
    Read,
    /// `json.write`.
    Write,
    /// A structured field's `jsonb` column (chunk 5.9b): written and read
    /// back, kept on the server, so it holds a `PasswordHash` as a field of
    /// its own may, and is made of `Object`s rather than a boundary's
    /// `Input` or `Output`.
    Store,
}

/// The first type inside `ty` that has no JSON form for `used`, with what
/// it is, after "is": a failure, an event, an entity, a secret, `Unit`.
pub(crate) fn no_json_form(
    table: &dyn TypeView,
    ty: &Ty,
    seen: &mut BTreeSet<String>,
    used: JsonUse,
) -> Option<(String, String)> {
    // A password's hash is stored and checked, never sent or read (area
    // 19, decided by the lead session for chunk 5.5q2); kept in a column of
    // its own or inside a stored value alike.
    if used != JsonUse::Store
        && let Some(hash) = password_hash(table, ty)
    {
        return Some((
            hash,
            "a password's hash, which is stored and checked but never leaves the server"
                .to_string(),
        ));
    }
    // A secret value is never written out, though it is read, as a
    // request's `Input` reads one (area 19, chunk 5.5r2).
    if used != JsonUse::Read
        && let Some(secret) = super::secrets::secret_of(table, ty)
    {
        return Some((
            ty.strip_nullable().to_string(),
            format!("a `{secret}`, a secret value, which is never written out"),
        ));
    }
    let found = table.structure(ty);
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
                "a secret, which only a route's `Output` or the setting it is read from ever holds"
                    .to_string(),
            )),
            Prim::Certificate => Some((
                found.to_string(),
                "a database server's authority, which only the setting a connection's `authority:` reads ever holds"
                    .to_string(),
            )),
            Prim::SignInCode => Some((
                found.to_string(),
                "a sign-in code, which only the email or the sending that carries it ever holds"
                    .to_string(),
            )),
            Prim::SignInToken => Some((
                found.to_string(),
                "a sign-in link's token, which only the request the link's page posts ever holds"
                    .to_string(),
            )),
            Prim::PluralCount => Some((
                found.to_string(),
                "a message's plural count, used only as a `Message` field".to_string(),
            )),
            // Text for people is shown, never stored (area 10).
            Prim::PresentationText if used == JsonUse::Store => Some((
                found.to_string(),
                "text formatted for one reader, which is shown, never stored".to_string(),
            )),
            _ => None,
        },
        Ty::List(item) | Ty::Set(item) | Ty::Nullable(item) => {
            no_json_form(table, item, seen, used)
        }
        Ty::Map(key, value) => keyless(table, key)
            .or_else(|| no_json_form(table, key, seen, used))
            .or_else(|| no_json_form(table, value, seen, used)),
        Ty::Named(name) | Ty::Opaque(name, _) => {
            if standard_type(name).is_some() {
                return match &found {
                    Ty::Opaque(_, args) => args
                        .iter()
                        .find_map(|arg| no_json_form(table, arg, seen, used)),
                    _ => None,
                };
            }
            if !seen.insert(name.clone()) {
                return None;
            }
            match table.def(name) {
                Some(Def::Fields { base, fields, .. }) => match base {
                    FieldsBase::Object => fields
                        .iter()
                        .find_map(|field| no_json_form(table, &field.declared, seen, used)),
                    FieldsBase::Input | FieldsBase::Output if used != JsonUse::Store => fields
                        .iter()
                        .find_map(|field| no_json_form(table, &field.declared, seen, used)),
                    other => Some((found.to_string(), fields_base(*other, used).to_string())),
                },
                Some(Def::Enum { variants }) => variants
                    .iter()
                    .flat_map(|variant| variant.fields.iter().flatten())
                    .find_map(|field| no_json_form(table, &field.declared, seen, used)),
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
                // A batch run, kept in an entity field or saved with
                // another run, is its type's JSON on the server (area 12);
                // nothing of a batch crosses a boundary.
                None if jadpo_std::batch::object_fields(name).is_some() => (used != JsonUse::Store)
                    .then(|| {
                        (
                            found.to_string(),
                            "a batch's, which the runtime keeps and no request or document carries"
                                .to_string(),
                        )
                    }),
                // A stored file, kept in an entity field or saved with a
                // run, is its type's JSON on the server (area 16, chunk
                // 6.4b): a request sends a `FileUpload`, and an output a
                // signed link, which come with uploads.
                None if name == jadpo_std::files::FILE => (used != JsonUse::Store).then(|| {
                    (
                        found.to_string(),
                        "a stored file's, which the runtime keeps; a request sends a `FileUpload` and an output a signed link, which come with uploads"
                            .to_string(),
                    )
                }),
                None => Some((
                    found.to_string(),
                    "a built-in type that crosses no boundary as JSON".to_string(),
                )),
            }
        }
        other => Some((other.to_string(), "not a value".to_string())),
    }
}

/// A map's key type when it has no single text form, with what it is: a
/// JSON object's keys are text, so a key is written as its canonical text
/// (`text.from`) and read back by `text.parse`'s reader (area 19, chunk
/// 5.5w2), and an object, a choice, a collection, an enum with data or a
/// value that may be `none` has none.
fn keyless(table: &dyn TypeView, key: &Ty) -> Option<(String, String)> {
    const WHY: &str = "and a map's key is sent as its canonical text, the text `text.from` writes, which it has none of";
    if table.representation(key).is_some() {
        return None;
    }
    let found = table.structure(key);
    let what = match &found {
        Ty::Named(name) | Ty::Opaque(name, _) => match standard_type(name) {
            Some(entry) if entry.text.is_some() => return None,
            Some(_) => "a standard type with no single text form",
            None => match table.def(name) {
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

/// What a type with fields that has no JSON form for `used` is.
fn fields_base(base: FieldsBase, used: JsonUse) -> &'static str {
    match base {
        FieldsBase::Failure => "a failure, which reaches a client only as area 4's envelope",
        FieldsBase::Event => "an event, which is emitted, not written",
        FieldsBase::EmailMessage => "an email, which is sent, not written",
        FieldsBase::EmailPartial => {
            "an email partial, which an email's templates include, not written"
        }
        FieldsBase::Message => "a message, translated text that is shown, not written",
        FieldsBase::Prompt => {
            "a prompt, which is rendered for a model with `prompts.render`, not written"
        }
        FieldsBase::Entity => {
            "an entity, whose records reach a client only through an `Output` that picks what it sends"
        }
        FieldsBase::Input if used == JsonUse::Store => {
            "an `Input`, the shape a request arrives in, where a record keeps an `Object`"
        }
        FieldsBase::Output if used == JsonUse::Store => {
            "an `Output`, the shape a response is sent in, where a record keeps an `Object`"
        }
        FieldsBase::Object | FieldsBase::Input | FieldsBase::Output => "an object",
    }
}

/// The type as written when it is a `PasswordHash` or built on one.
fn password_hash(table: &dyn TypeView, ty: &Ty) -> Option<String> {
    let mut current = ty.strip_nullable().clone();
    for _ in 0..16 {
        let Ty::Named(name) = &current else {
            return None;
        };
        if name == jadpo_std::standard::security::PASSWORD_HASH {
            return Some(ty.strip_nullable().to_string());
        }
        match table.def(name) {
            Some(Def::Scalar { parent, .. }) => {
                current = parent.strip_nullable().clone();
            }
            _ => return None,
        }
    }
    None
}
