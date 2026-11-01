//! Secret values (area 19, "Secret values", chunk 5.5r2): which types are
//! or hold a `Password`, an `ApiKey`, a `BearerToken`, a `TotpSecret`, a
//! `SecretText` or a `Jwt`, and the diagnostics `type.secret_exposed`
//! gives where one would leave the server, be kept or be read as text.
//! The declaration check refuses one in an `Output`, a failure, an event,
//! an email and a message; the data model refuses one in an entity; the
//! body check refuses `json.write` of one, an operator other than `==` and
//! `!=` on one, and passing one to any standard operation but the one made
//! to take it.

use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_std::Op;
use jadpo_std::standard::security;

use super::shape::FieldsBase;
use super::table::{Def, TypeView};
use super::ty::Ty;

/// The secret value `ty` is, or is built on: `Password` for a `Password`,
/// a type built on one (`StaffPassword`) or one that may be `none`.
pub(crate) fn secret_of(table: &dyn TypeView, ty: &Ty) -> Option<&'static str> {
    let mut current = ty.strip_nullable().clone();
    for _ in 0..32 {
        let name = match &current {
            Ty::Named(name) => name,
            // A type that names a field holds what the field does: an
            // entity's can hold a secret, kept sealed (chunk P9).
            Ty::Field(host, path) => {
                current = table.field(host, path)?.declared.strip_nullable().clone();
                continue;
            }
            _ => return None,
        };
        if let Some(secret) = security::secret_value(name) {
            return Some(secret);
        }
        match table.def(name) {
            Some(Def::Scalar { parent, .. }) => current = parent.strip_nullable().clone(),
            _ => return None,
        }
    }
    None
}

/// The secret value `ty` is, or a collection of `ty` holds as its items,
/// keys or values: what an operation given a value of `ty` would read.
pub(crate) fn secret_in(table: &dyn TypeView, ty: &Ty) -> Option<&'static str> {
    if let Some(secret) = secret_of(table, ty) {
        return Some(secret);
    }
    match table.structure(ty) {
        Ty::List(inner) | Ty::Set(inner) | Ty::Nullable(inner) => secret_in(table, &inner),
        Ty::Map(key, value) => secret_in(table, &key).or_else(|| secret_in(table, &value)),
        _ => None,
    }
}

/// The secret value a value of `ty` is or holds, however deep: in a list,
/// a set, a map or an option, in a field of an object or an `Input`, or in
/// a variant's data. An `Output`, a failure, an event, an email or a
/// message holding one is reported at its own declaration, so it isn't
/// followed again.
pub(crate) fn held_secret(
    table: &dyn TypeView,
    ty: &Ty,
    seen: &mut Vec<String>,
) -> Option<&'static str> {
    if let Some(secret) = secret_of(table, ty) {
        return Some(secret);
    }
    match ty {
        // A feature's properties (area 20, chunk 5.5u7).
        Ty::Opaque(name, args) if super::is_feature_type(name) => {
            args.iter().find_map(|arg| held_secret(table, arg, seen))
        }
        Ty::List(inner) | Ty::Set(inner) | Ty::Nullable(inner) => held_secret(table, inner, seen),
        Ty::Map(key, value) => {
            held_secret(table, key, seen).or_else(|| held_secret(table, value, seen))
        }
        Ty::Named(name) => {
            if seen.contains(name) {
                return None;
            }
            seen.push(name.clone());
            match table.def(name) {
                Some(Def::Scalar { parent, .. }) => held_secret(table, parent, seen),
                Some(Def::Fields {
                    base: FieldsBase::Object | FieldsBase::Input,
                    fields,
                    ..
                }) => fields
                    .iter()
                    .find_map(|field| held_secret(table, &field.declared, seen)),
                Some(Def::Enum { variants }) => variants
                    .iter()
                    .flat_map(|variant| variant.fields.iter().flatten())
                    .find_map(|field| held_secret(table, &field.declared, seen)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// The magic link's code or token a value of `ty` is or holds, however
/// deep (area 8, "Magic links"): in a list, a set, a map or an option, in
/// a field of an object or an `Input`, or in a variant's data.
pub(crate) fn held_sign_in(
    table: &dyn TypeView,
    ty: &Ty,
    seen: &mut Vec<String>,
) -> Option<jadpo_std::Builtin> {
    match table.structure(ty) {
        Ty::Prim(held @ (jadpo_std::Builtin::SignInCode | jadpo_std::Builtin::SignInToken)) => {
            Some(held)
        }
        Ty::List(inner) | Ty::Set(inner) | Ty::Nullable(inner) => held_sign_in(table, &inner, seen),
        Ty::Map(key, value) => {
            held_sign_in(table, &key, seen).or_else(|| held_sign_in(table, &value, seen))
        }
        Ty::Named(name) => {
            if seen.contains(&name) {
                return None;
            }
            seen.push(name.clone());
            match table.def(&name) {
                Some(Def::Fields {
                    base: FieldsBase::Object | FieldsBase::Input,
                    fields,
                    ..
                }) => fields
                    .iter()
                    .find_map(|field| held_sign_in(table, &field.declared, seen)),
                Some(Def::Enum { variants }) => variants
                    .iter()
                    .flat_map(|variant| variant.fields.iter().flatten())
                    .find_map(|field| held_sign_in(table, &field.declared, seen)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Whether a field of a declaration of `base` (a failure's when
/// `failure`) may hold `held`, a magic link's code or token: a code goes
/// into an email or the application's own sending, and a token only from
/// a request's `Input` to `magic_links.redeem`, so neither is answered,
/// put in a failure or an event, and a token is never emailed or worded.
pub(crate) fn sign_in_refused(base: FieldsBase, failure: bool, held: jadpo_std::Builtin) -> bool {
    failure
        || match base {
            FieldsBase::Output | FieldsBase::Event | FieldsBase::Failure => true,
            FieldsBase::EmailMessage
            | FieldsBase::EmailPartial
            | FieldsBase::Message
            | FieldsBase::Prompt => held == jadpo_std::Builtin::SignInToken,
            FieldsBase::Object | FieldsBase::Input | FieldsBase::Entity => false,
        }
}

/// A field holding a magic link's code or token where none may go: a
/// response, a failure or an event, or, for a token, an email, a message
/// or a prompt (area 8, "Magic links").
pub(crate) fn sign_in_code_field(
    base: FieldsBase,
    failure: bool,
    host: &str,
    name: &str,
    held: jadpo_std::Builtin,
    span: Span,
) -> Diagnostic {
    let (label, why) = if failure || base == FieldsBase::Failure {
        (
            "in a failure",
            "A failure is logged and its client fields reach the client.",
        )
    } else if base == FieldsBase::Event {
        (
            "in an event",
            "An event is kept in the outbox and handed to every subscriber.",
        )
    } else if matches!(base, FieldsBase::EmailMessage | FieldsBase::EmailPartial) {
        (
            "in an email",
            "An email is kept in the outbox until it is sent.",
        )
    } else if matches!(base, FieldsBase::Message | FieldsBase::Prompt) {
        (
            "written out",
            "A message or a prompt is text written for someone else to read.",
        )
    } else {
        (
            "sent to a client",
            "A response is written out, logged by proxies and kept by browsers.",
        )
    };
    let goes = if held == jadpo_std::Builtin::SignInToken {
        "A sign-in link's token comes only from the request the link's page posts, to `magic_links.redeem`, so nobody reading a response, a log or the outbox can sign in as the person it was sent to."
    } else {
        "A sign-in code goes only into the email that carries it, or to the application's own sending, so nobody reading a response, a log or the outbox can sign in as the person it was sent to."
    };
    Diagnostic::new(
        &rule::TYPE_SECRET_EXPOSED,
        format!("`{host}.{name}` holds a `{}`", held.name()),
    )
    .at(span, label)
    .reason(format!("{goes} {why}"))
    .next(format!("Remove `{name}`"))
}

/// Whether a declaration of `base` keeps a secret value out of its fields:
/// everything but an `Object` or an `Input`, which stay in memory or arrive
/// from outside (an entity's fields are the data model's to refuse).
pub(crate) fn refuses(base: FieldsBase, failure: bool) -> bool {
    failure
        || matches!(
            base,
            FieldsBase::Output
                | FieldsBase::Event
                | FieldsBase::EmailMessage
                | FieldsBase::EmailPartial
                | FieldsBase::Message
                | FieldsBase::Failure
        )
}

/// `a` or `an` before a secret value's name, as it is read aloud.
fn indefinite(secret: &str) -> &'static str {
    if secret.starts_with(['A', 'E', 'I', 'O', 'U']) {
        "an"
    } else {
        "a"
    }
}

/// `A` or `An`, starting a sentence.
fn capital(written: &str) -> &'static str {
    if written == "an" { "An" } else { "A" }
}

/// What a secret value is, for a reason line.
fn what(secret: &str) -> &'static str {
    match secret {
        security::PASSWORD => {
            "A password is hashed with `crypto.hash_password` and checked with `crypto.verify_password`"
        }
        security::API_KEY | security::BEARER_TOKEN => {
            "A key or a token is checked against its hash, `hash.sha256` of it, which is what is kept"
        }
        security::JWT => "A token is checked with `crypto.verify_jwt`",
        security::TOTP_SECRET => "An authenticator's secret checks the codes it makes",
        security::ONE_TIME_CODE => {
            "A one-time code is checked when it arrives and never kept, so it can't be used again"
        }
        _ => "A secret is passed only to what needs it",
    }
}

/// A field holding a secret value where none may go: an `Output`, a
/// failure, an event, an email or a message.
pub(crate) fn field(
    base: FieldsBase,
    failure: bool,
    host: &str,
    name: &str,
    secret: &str,
    span: Span,
) -> Diagnostic {
    let (label, why, next) = if failure || base == FieldsBase::Failure {
        (
            "in a failure",
            "A failure is logged and its client fields reach the client, so a secret in one would be written out.",
            format!("Remove `{name}`; say which record or field was refused instead"),
        )
    } else {
        match base {
            FieldsBase::Event => (
                "in an event",
                "An event is kept in the outbox and handed to every subscriber, so a secret in one would be stored and passed on.",
                format!("Remove `{name}`, and do what needs the secret in the action itself"),
            ),
            FieldsBase::EmailMessage | FieldsBase::EmailPartial => (
                "sent in an email",
                "An email is sent in clear to a mailbox the application doesn't control, and kept there.",
                format!(
                    "Remove `{name}`, and send a link to a page that signs the person in instead"
                ),
            ),
            FieldsBase::Message => (
                "shown in a message",
                "A message is text shown to a person, and a secret is never shown.",
                format!("Remove `{name}`"),
            ),
            _ => (
                "sent to a client",
                "A response is written out, logged by proxies and kept by browsers, and a client never needs a secret back.",
                format!(
                    "Remove `{name}`, and send what the client needs instead, such as whether one is set"
                ),
            ),
        }
    };
    Diagnostic::new(
        &rule::TYPE_SECRET_EXPOSED,
        format!("`{host}.{name}` holds {} `{secret}`", indefinite(secret)),
    )
    .at(span, label)
    .reason(format!(
        "{} `{secret}` is a secret value: it is never sent, logged, stored or shown. {why}",
        capital(indefinite(secret))
    ))
    .next(next)
}

/// An entity's field holding a secret value: refused for a password, a
/// key, a token and a JWT, which are kept only as a hash. A `TotpSecret` and
/// a `SecretText` are kept sealed in a field of their own (chunk P9), so one
/// is refused only inside a structure.
pub(crate) fn stored(entity: &str, name: &str, secret: &str, span: Span) -> Diagnostic {
    if security::stored_encrypted_later(secret) {
        return Diagnostic::new(
            &rule::TYPE_SECRET_EXPOSED,
            format!(
                "`{entity}.{name}` keeps {} `{secret}` inside another value",
                indefinite(secret)
            ),
        )
        .at(span, "a secret, inside a value")
        .reason(format!(
            "{} `{secret}` is never stored as itself, only sealed with the application's `field_encryption_key:`, in a field of its own, so its column holds only sealed bytes.",
            capital(indefinite(secret))
        ))
        .next(format!(
            "Give the `{secret}` a field of its own on `{entity}`, typed `{secret}`"
        ));
    }
    let next = match secret {
        security::PASSWORD => format!(
            "Store `crypto.hash_password(...)` of it in a `PasswordHash` field in `{name}`'s place"
        ),
        security::API_KEY | security::BEARER_TOKEN => format!(
            "Store `Hash(hash.sha256(...))` of it in a `Hash` field in `{name}`'s place, and look it up by that"
        ),
        security::ONE_TIME_CODE => format!(
            "Remove `{name}`, check the code when it arrives, and keep only when it was used"
        ),
        _ => {
            format!("Remove `{name}`, and check the token with `crypto.verify_jwt` when it arrives")
        }
    };
    Diagnostic::new(
        &rule::TYPE_SECRET_EXPOSED,
        format!("`{entity}.{name}` keeps {} `{secret}`", indefinite(secret)),
    )
    .at(span, "a secret, stored")
    .reason(format!(
        "{} `{secret}` is never stored as itself, so a copy of the database holds none. {}.",
        capital(indefinite(secret)),
        what(secret)
    ))
    .next(next)
}

/// Whether standard operation `op` takes secret value `secret` as its
/// parameter numbered `slot`: the operations made for it, and nothing
/// else, so no text, number or flag is ever made from a secret.
pub(crate) fn taken_by(op: Op, slot: usize, secret: &str) -> bool {
    match (op, slot) {
        (Op::CryptoHashPassword | Op::CryptoVerifyPassword, 0) => secret == security::PASSWORD,
        (Op::CryptoVerifyJwt, 0) => secret == security::JWT,
        // What is kept of an API key or a bearer token, to look one up by
        // (chosen in chunk 5.5r2); a password's hash is Argon2id's alone.
        (Op::HashSha256, 0) => matches!(secret, security::API_KEY | security::BEARER_TOKEN),
        _ => false,
    }
}

/// A secret value passed to a standard operation not made to take it.
pub(crate) fn argument(written: &str, secret: &str, span: Span) -> Diagnostic {
    let next = match secret {
        security::PASSWORD => {
            "Pass it only to `crypto.hash_password` or `crypto.verify_password`".to_string()
        }
        security::API_KEY | security::BEARER_TOKEN => {
            "Pass it only to `hash.sha256`, and keep or compare that".to_string()
        }
        security::JWT => {
            "Pass it only to `crypto.verify_jwt`, and read the claims it gives".to_string()
        }
        _ => "Compare it with `==` or pass it on as itself, never into another value".to_string(),
    };
    Diagnostic::new(
        &rule::TYPE_SECRET_EXPOSED,
        format!("`{written}` can't take {} `{secret}`", indefinite(secret)),
    )
    .at(span, "a secret, read here")
    .reason(format!(
        "{} `{secret}` is a secret value: only the operation made for it reads its text, so nothing derived from it (its length, a part, its text, how it is shown) can carry it out. {}.",
        capital(indefinite(secret)),
        what(secret)
    ))
    .next(next)
}

/// An operator other than `==` and `!=` on a secret value, or a type that
/// isn't built on it made from one.
pub(crate) fn made_from(written: &str, secret: &str, span: Span) -> Diagnostic {
    Diagnostic::new(
        &rule::TYPE_SECRET_EXPOSED,
        format!("{written} can't take {} `{secret}`", indefinite(secret)),
    )
    .at(span, "a secret, read here")
    .reason(format!(
        "{} `{secret}` is a secret value: it is compared with `==` and `!=`, in constant time, and passed on as itself, but nothing else reads it, so no other value can carry it out.",
        capital(indefinite(secret))
    ))
    .next("Compare it with `==`, or pass it on as itself")
}
