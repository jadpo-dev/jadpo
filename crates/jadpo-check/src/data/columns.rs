//! Column types (area 5, "Storage types"): each field's type maps to its
//! store engine's native column type, and the source never names one; and
//! the system tables' columns.

use std::collections::BTreeMap;

use jadpo_graph::schema::{Allows, Column, Holds, Key, Table, object_name};
use jadpo_std::{Builtin, Num};

use crate::types::{Def, Ty, TypeView};

/// The most digits a Jadpo decimal holds (area 2).
const DECIMAL_DIGITS: u32 = 38;

/// The PostgreSQL column type for a field of type `ty` (its `?` left
/// aside), or the type's name when it has none in this compiler.
pub fn column_type(table: &dyn TypeView, ty: &Ty) -> Result<String, String> {
    let structure = table.structure(ty);
    Ok(match &structure {
        // A token or a key is kept only as its verifier, by the runtime
        // (area 8).
        // And a configuration value is a secret, which never leaves the
        // server (area 10).
        Ty::Prim(prim) if prim.is_secret() => {
            return Err(prim.name().to_string());
        }
        Ty::Prim(prim @ (Builtin::ApiToken | Builtin::ServiceKey)) => {
            return Err(prim.name().to_string());
        }
        // A standard type is stored as its data entry says (area 19).
        Ty::Prim(prim) if let Some(entry) = jadpo_std::standard::of_builtin(*prim) => {
            entry.column_type().into()
        }
        Ty::Prim(prim) => match prim.representation() {
            Builtin::Bool => "boolean".into(),
            Builtin::Text => "text".into(),
            Builtin::Bytes => "bytea".into(),
            Builtin::Int => integer(table, ty),
            Builtin::Decimal => decimal(table, ty),
            // The standard types are stored as their entries say, above;
            // the page types are made of text and numbers, which
            // `representation` has already given.
            Builtin::Uuid
            | Builtin::Instant
            | Builtin::CalendarDate
            | Builtin::ZonedTime
            | Builtin::Duration
            | Builtin::Money
            | Builtin::Quantity(_)
            | Builtin::Percentage
            | Builtin::Unit
            | Builtin::Email
            | Builtin::Url
            | Builtin::IpAddress
            | Builtin::PageCursor
            | Builtin::PageSize
            | Builtin::ApiToken
            | Builtin::ServiceKey
            | Builtin::PostgresUrl
            | Builtin::RedisUrl
            | Builtin::SigningKey
            | Builtin::EncryptionKey
            | Builtin::PrivateKey
            // Text for people is shown, never stored (area 10), and a
            // plural count only picks the form of a message's wording.
            | Builtin::PresentationText
            | Builtin::PluralCount => {
                return Err(prim.name().to_string());
            }
        },
        Ty::Named(name) | Ty::Opaque(name, _)
            if let Some(entry) = jadpo_std::standard_type(name) =>
        {
            entry.column_type().into()
        }
        Ty::Named(name) => match table.def(name) {
            Some(Def::Enum { variants }) => {
                if variants.iter().any(|v| v.fields.is_some()) {
                    "jsonb".into()
                } else {
                    "text".into()
                }
            }
            Some(Def::Fields { .. }) => "jsonb".into(),
            _ => return Err(name.clone()),
        },
        Ty::List(_) | Ty::Set(_) | Ty::Map(..) => "jsonb".into(),
        other => return Err(other.to_string()),
    })
}

/// What the field typed `field` (`Todo.priority`, so the field's own
/// settings count) allows beyond its column type: an enum's variants, and
/// the bounds, lengths and patterns its rules set, most specific first,
/// since a refinement only narrows.
pub fn allows(table: &dyn TypeView, field: &Ty) -> Allows {
    let mut out = Allows::default();
    if let Ty::Named(name) = table.structure(field)
        && let Some(Def::Enum { variants }) = table.def(&name)
        && variants.iter().all(|v| v.fields.is_none())
    {
        // As the column holds them: `Currency`'s by code (area 19).
        out.values = variants
            .iter()
            .map(|v| jadpo_std::standard::stored_variant(&name, &v.name).to_string())
            .collect();
        out.values.sort();
    }
    for (_, rules) in table.rule_chain(field) {
        if out.min.is_none() {
            out.min = rules.min.map(|n| n.to_string());
        }
        if out.max.is_none() {
            out.max = rules.max.map(|n| n.to_string());
        }
        out.min_length = out.min_length.or(rules.min_length);
        out.max_length = out.max_length.or(rules.max_length);
        if let Some(pattern) = &rules.pattern
            && !out.patterns.contains(pattern)
        {
            out.patterns.push(pattern.clone());
        }
        // A `Money` type's one currency, which its currency column holds
        // (area 19).
        if out.values.is_empty()
            && let Some(code) = rules.currency.and_then(jadpo_std::money::code)
        {
            out.values = vec![code.to_string()];
        }
    }
    out.patterns.sort();
    out
}

/// The declared bounds of a number type, most specific first: a
/// refinement only narrows, so the first `min` and `max` found hold.
fn bounds(table: &dyn TypeView, ty: &Ty) -> (Option<Num>, Option<Num>, Option<u32>) {
    let (mut min, mut max, mut scale) = (None, None, None);
    for (_, rules) in table.rule_chain(ty) {
        min = min.or(rules.min);
        max = max.or(rules.max);
        scale = scale.or(rules.scale);
    }
    (min, max, scale)
}

/// `smallint`, `integer` or `bigint`, from the declared range.
fn integer(table: &dyn TypeView, ty: &Ty) -> String {
    let (min, max, _) = bounds(table, ty);
    let within = |low: i128, high: i128| {
        min.is_some_and(|n| n.compare(Num::whole(low)).is_ge())
            && max.is_some_and(|n| n.compare(Num::whole(high)).is_le())
    };
    if within(i128::from(i16::MIN), i128::from(i16::MAX)) {
        "smallint".into()
    } else if within(i128::from(i32::MIN), i128::from(i32::MAX)) {
        "integer".into()
    } else {
        "bigint".into()
    }
}

/// `numeric(p, s)`: the scale from `scale:`, the precision from the
/// declared range when there is one, else the most a decimal holds.
fn decimal(table: &dyn TypeView, ty: &Ty) -> String {
    let (min, max, scale) = bounds(table, ty);
    let Some(scale) = scale else {
        return "numeric".into();
    };
    let digits = |n: &Option<Num>| n.map(integer_digits);
    let precision = match (digits(&min), digits(&max)) {
        (Some(a), Some(b)) => (a.max(b) + scale).clamp(scale.max(1), DECIMAL_DIGITS),
        _ => DECIMAL_DIGITS,
    };
    format!("numeric({precision}, {scale})")
}

/// How many digits a number's whole part has: `123.45` has three.
fn integer_digits(n: Num) -> u32 {
    let whole = n.mantissa.unsigned_abs() / 10u128.pow(n.scale.min(38));
    let mut digits = 1;
    let mut rest = whole / 10;
    while rest > 0 {
        digits += 1;
        rest /= 10;
    }
    digits
}

fn column(position: u32, sql_type: &str, nullable: bool) -> Column {
    Column {
        position,
        field: None,
        sql_type: sql_type.to_string(),
        nullable,
        retired: false,
        allows: None,
    }
}

/// A system table's columns and keys (area 5, "System tables"): the event
/// outbox, the delivery records of each event to each subscriber, and the
/// stored responses of idempotent requests, browser sessions and API
/// tokens, and service keys. Areas 7, 8 and 12 give them their behaviour.
pub fn system_table(name: &str) -> Table {
    let table = format!("jadpo_{name}");
    let mut columns = BTreeMap::new();
    let mut primary = vec!["id".to_string()];
    let mut unique = BTreeMap::new();
    let mut foreign_keys = BTreeMap::new();
    let mut indexes = BTreeMap::new();
    let mut add = |name: &str, sql_type: &str, nullable: bool| {
        let position = u32::try_from(columns.len()).unwrap_or(u32::MAX);
        columns.insert(name.to_string(), column(position, sql_type, nullable));
    };
    match name {
        "outbox" => {
            add("id", "uuid", false);
            // The event type's identity key, which survives a rename.
            add("event", "text", false);
            add("payload", "jsonb", false);
            add("emitted_at", "timestamptz(3)", false);
            // W3C trace context of the action that emitted it.
            add("traceparent", "text", true);
            indexes.insert(
                object_name(&[&table, "emitted_at"], "idx"),
                jadpo_graph::schema::Index {
                    columns: vec!["emitted_at".into()],
                    reason: "system outbox".into(),
                    predicate: None,
                    collate: Vec::new(),
                },
            );
        }
        "deliveries" => {
            add("id", "uuid", false);
            add("outbox_id", "uuid", false);
            add("subscriber", "text", false);
            add("status", "text", false);
            add("attempts", "integer", false);
            add("next_attempt_at", "timestamptz(3)", true);
            add("last_failure", "jsonb", true);
            add("last_tried_at", "timestamptz(3)", false);
            unique.insert(
                object_name(&[&table, "outbox_id", "subscriber"], "key"),
                jadpo_graph::schema::Unique {
                    columns: vec!["outbox_id".into(), "subscriber".into()],
                    rule: "system deliveries".into(),
                },
            );
            foreign_keys.insert(
                object_name(&[&table, "outbox_id"], "fkey"),
                jadpo_graph::schema::ForeignKey {
                    columns: vec!["outbox_id".into()],
                    table: "jadpo_outbox".into(),
                    on_delete: jadpo_graph::schema::OnDelete::Cascade,
                },
            );
            indexes.insert(
                object_name(&[&table, "status", "next_attempt_at"], "idx"),
                jadpo_graph::schema::Index {
                    columns: vec!["status".into(), "next_attempt_at".into()],
                    reason: "system deliveries".into(),
                    predicate: None,
                    collate: Vec::new(),
                },
            );
        }
        "sessions" => {
            // Browser sessions and API tokens (area 8): each found by its
            // token's verifier, never the token itself, with a browser
            // session's CSRF token's; an API token has none.
            add("id", "uuid", false);
            add("principal_id", "uuid", false);
            add("verifier", "text", false);
            add("csrf_verifier", "text", true);
            add("started_at", "timestamptz(3)", false);
            add("ends_at", "timestamptz(3)", false);
            unique.insert(
                object_name(&[&table, "verifier"], "key"),
                jadpo_graph::schema::Unique {
                    columns: vec!["verifier".into()],
                    rule: "system sessions".into(),
                },
            );
            indexes.insert(
                object_name(&[&table, "principal_id"], "idx"),
                jadpo_graph::schema::Index {
                    columns: vec!["principal_id".into()],
                    reason: "system sessions".into(),
                    predicate: None,
                    collate: Vec::new(),
                },
            );
            indexes.insert(
                object_name(&[&table, "ends_at"], "idx"),
                jadpo_graph::schema::Index {
                    columns: vec!["ends_at".into()],
                    reason: "system sessions".into(),
                    predicate: None,
                    collate: Vec::new(),
                },
            );
        }
        "service_keys" => {
            // Service keys (area 8): each found by its verifier, never the
            // key itself, with the service it signs in and when it expires.
            add("id", "uuid", false);
            add("principal_id", "uuid", false);
            add("verifier", "text", false);
            add("started_at", "timestamptz(3)", false);
            add("ends_at", "timestamptz(3)", false);
            unique.insert(
                object_name(&[&table, "verifier"], "key"),
                jadpo_graph::schema::Unique {
                    columns: vec!["verifier".into()],
                    rule: "system service keys".into(),
                },
            );
            indexes.insert(
                object_name(&[&table, "principal_id"], "idx"),
                jadpo_graph::schema::Index {
                    columns: vec!["principal_id".into()],
                    reason: "system service keys".into(),
                    predicate: None,
                    collate: Vec::new(),
                },
            );
        }
        "passkeys" => {
            // Passkeys (area 8, "Passkeys"): each found by its credential
            // ID, kept as the `WebAuthnCredential` it is, with its user
            // and the user handle its device keeps. Nothing in it signs
            // anyone in.
            add("id", "uuid", false);
            add("principal_id", "uuid", false);
            add("credential_id", "bytea", false);
            add("user_handle", "bytea", false);
            add("credential", "jsonb", false);
            add("registered_at", "timestamptz(3)", false);
            unique.insert(
                object_name(&[&table, "credential_id"], "key"),
                jadpo_graph::schema::Unique {
                    columns: vec!["credential_id".into()],
                    rule: "system passkeys".into(),
                },
            );
            indexes.insert(
                object_name(&[&table, "principal_id"], "idx"),
                jadpo_graph::schema::Index {
                    columns: vec!["principal_id".into()],
                    reason: "system passkeys".into(),
                    predicate: None,
                    collate: Vec::new(),
                },
            );
        }
        "passkey_challenges" => {
            // Passkey ceremonies under way (area 8, "Passkeys"): each
            // found by its challenge's verifier and used once, bound to
            // the browser or session that asked, with the state
            // `webauthn-rs` checks the answer against.
            add("id", "uuid", false);
            add("challenge", "text", false);
            add("purpose", "text", false);
            add("binding", "text", false);
            add("principal_id", "uuid", true);
            add("user_handle", "bytea", true);
            add("state", "text", false);
            add("expires_at", "timestamptz(3)", false);
            unique.insert(
                object_name(&[&table, "challenge"], "key"),
                jadpo_graph::schema::Unique {
                    columns: vec!["challenge".into()],
                    rule: "system passkey challenges".into(),
                },
            );
            unique.insert(
                object_name(&[&table, "purpose", "binding"], "key"),
                jadpo_graph::schema::Unique {
                    columns: vec!["purpose".into(), "binding".into()],
                    rule: "system passkey challenges".into(),
                },
            );
            indexes.insert(
                object_name(&[&table, "expires_at"], "idx"),
                jadpo_graph::schema::Index {
                    columns: vec!["expires_at".into()],
                    reason: "system passkey challenges".into(),
                    predicate: None,
                    collate: Vec::new(),
                },
            );
        }
        _ => {
            // Idempotency keys: one stored response per key and caller.
            add("scope", "text", false);
            add("key", "text", false);
            add("request_digest", "text", false);
            add("status", "smallint", true);
            add("response", "jsonb", true);
            add("stored_at", "timestamptz(3)", false);
            add("kept_until", "timestamptz(3)", false);
            primary = vec!["scope".into(), "key".into()];
            indexes.insert(
                object_name(&[&table, "kept_until"], "idx"),
                jadpo_graph::schema::Index {
                    columns: vec!["kept_until".into()],
                    reason: "system idempotency keys".into(),
                    predicate: None,
                    collate: Vec::new(),
                },
            );
        }
    }
    Table {
        holds: Holds::System(name.to_string()),
        columns,
        primary_key: Key {
            name: object_name(&[&table], "pkey"),
            columns: primary,
        },
        unique,
        foreign_keys,
        indexes,
        checks: BTreeMap::new(),
    }
}
