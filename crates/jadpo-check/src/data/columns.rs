//! Column types (area 5, "Storage types"): each field's type maps to its
//! store engine's native column type, and the source never names one; and
//! the system tables' columns.

use std::collections::BTreeMap;

use jadpo_graph::schema::{Allows, Column, Holds, Key, Table, object_name};
use jadpo_graph::stored_json;
use jadpo_std::{Builtin, Num};

use crate::types::{Def, Ty, TypeView};

/// The most digits a Jadpo decimal holds (area 2).
const DECIMAL_DIGITS: u32 = 38;

/// The PostgreSQL column type for a field of type `ty` (its `?` left
/// aside), or the type's name when it has none in this compiler.
pub fn column_type(table: &dyn TypeView, ty: &Ty) -> Result<String, String> {
    // A vector, whose structure is a list of decimals, is stored in a
    // pgvector `vector` (area 21, chunk 5.5v1).
    if table
        .rule_chain(ty)
        .iter()
        .any(|(_, rules)| rules.standard == Some(jadpo_std::standard::ai::VECTOR))
    {
        return Ok("vector".into());
    }
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
        // A range in PostgreSQL's range type of its ends (area 19,
        // "Ranges", chunk 5.5w).
        Ty::Opaque(name, args) if name == jadpo_std::RANGE => {
            let item = args.first().and_then(|item| table.representation(item));
            match item {
                Some(Builtin::Int | Builtin::Duration) => "int8range".into(),
                Some(Builtin::Decimal) => "numrange".into(),
                Some(Builtin::Instant) => "tstzrange".into(),
                Some(Builtin::CalendarDate) => "daterange".into(),
                _ => return Err(structure.to_string()),
            }
        }
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

/// What a structured field typed `field` (`Parcel.label`, so its own
/// settings count) reads when stored whole as its type's JSON (area 5,
/// "Storage types"; chunk 5.9c): the value's shape and every object and
/// enum of the application's inside it, with the rules along each part's
/// chain, as the route codec reads it back. Its `?` is the column's
/// `NULL`, so the value itself is never `none`.
pub fn json_reads(table: &dyn TypeView, field: &Ty) -> stored_json::Reads {
    let mut types = BTreeMap::new();
    let shape = match json_shape(table, field, &mut types) {
        stored_json::Shape::Optional(inner) => *inner,
        shape => shape,
    };
    stored_json::Reads { shape, types }
}

fn json_shape(
    table: &dyn TypeView,
    ty: &Ty,
    types: &mut BTreeMap<String, stored_json::Named>,
) -> stored_json::Shape {
    use stored_json::Shape;
    let optional = match ty {
        Ty::Nullable(_) => true,
        Ty::Field(host, path) => table
            .field(host, path)
            .is_some_and(|f| matches!(f.declared, Ty::Nullable(_))),
        _ => false,
    };
    if optional {
        let inner = match ty {
            Ty::Nullable(inner) => (**inner).clone(),
            other => other.strip_nullable().clone(),
        };
        let shape = match &inner {
            // A field's own rules are kept with the type it is.
            Ty::Field(..) => json_shape_of(table, &inner, types),
            _ => json_shape(table, &inner, types),
        };
        return Shape::Optional(Box::new(shape));
    }
    json_shape_of(table, ty, types)
}

/// [`json_shape`] of a type that isn't `none`.
fn json_shape_of(
    table: &dyn TypeView,
    ty: &Ty,
    types: &mut BTreeMap<String, stored_json::Named>,
) -> stored_json::Shape {
    use stored_json::{Named, Shape};
    let rules: Vec<&jadpo_std::Rules> = table.rule_chain(ty).into_iter().map(|(_, r)| r).collect();
    let lengths = || {
        (
            rules.iter().find_map(|r| r.min_length),
            rules.iter().find_map(|r| r.max_length),
        )
    };
    let structure = table.structure(ty);
    match &structure {
        Ty::List(item) | Ty::Set(item) => {
            let (min_length, max_length) = lengths();
            Shape::List {
                set: matches!(structure, Ty::Set(_)),
                item: Box::new(json_shape(table, item, types)),
                min_length,
                max_length,
            }
        }
        Ty::Map(key, value) => {
            let (min_length, max_length) = lengths();
            Shape::Map {
                key: Box::new(json_shape(table, key, types)),
                value: Box::new(json_shape(table, value, types)),
                min_length,
                max_length,
            }
        }
        Ty::Named(name) if jadpo_std::standard_type(name).is_none() => match table.def(name) {
            Some(Def::Fields { fields, rules, .. }) => {
                if !types.contains_key(name) {
                    // Held first, so a type that holds itself stops here.
                    types.insert(
                        name.clone(),
                        Named::Object {
                            fields: BTreeMap::new(),
                            rules: Vec::new(),
                        },
                    );
                    let fields: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
                    let mut written: Vec<String> = rules
                        .iter()
                        .map(|rule| format!("{}: {}", rule.name, rule.written))
                        .collect();
                    written.sort();
                    let fields = fields
                        .into_iter()
                        .map(|field| {
                            let at = Ty::Field(name.clone(), vec![field.clone()]);
                            (field, json_shape(table, &at, types))
                        })
                        .collect();
                    types.insert(
                        name.clone(),
                        Named::Object {
                            fields,
                            rules: written,
                        },
                    );
                }
                Shape::Named(name.clone())
            }
            Some(Def::Enum { variants }) => {
                if !types.contains_key(name) {
                    types.insert(
                        name.clone(),
                        Named::Enum {
                            variants: BTreeMap::new(),
                        },
                    );
                    let variants: Vec<(String, Option<Vec<String>>)> = variants
                        .iter()
                        .map(|v| {
                            (
                                v.name.clone(),
                                v.fields
                                    .as_ref()
                                    .map(|fields| fields.iter().map(|f| f.name.clone()).collect()),
                            )
                        })
                        .collect();
                    let variants = variants
                        .into_iter()
                        .map(|(variant, fields)| {
                            let fields = fields.map(|fields| {
                                fields
                                    .into_iter()
                                    .map(|field| {
                                        let at = Ty::Field(
                                            name.clone(),
                                            vec![variant.clone(), field.clone()],
                                        );
                                        (field, json_shape(table, &at, types))
                                    })
                                    .collect()
                            });
                            (variant, fields)
                        })
                        .collect();
                    types.insert(name.clone(), Named::Enum { variants });
                }
                Shape::Named(name.clone())
            }
            _ => Shape::Value(json_scalar(structure.to_string(), &rules)),
        },
        _ => Shape::Value(json_scalar(structure.to_string(), &rules)),
    }
}

/// A value built on a built-in or standard type `ty`, with the rules along
/// its chain, most specific first: the first bound and length found hold,
/// since a refinement only narrows, and every pattern and other rule.
fn json_scalar(ty: String, chain: &[&jadpo_std::Rules]) -> stored_json::Scalar {
    let mut out = stored_json::Scalar {
        ty,
        ..stored_json::Scalar::default()
    };
    for rules in chain {
        if out.min.is_none() {
            out.min = rules.min.map(|n| n.to_string());
        }
        if out.max.is_none() {
            out.max = rules.max.map(|n| n.to_string());
        }
        out.min_length = out.min_length.or(rules.min_length);
        out.max_length = out.max_length.or(rules.max_length);
        out.scale = out.scale.or(rules.scale);
        if let Some(pattern) = &rules.pattern
            && !out.patterns.contains(pattern)
        {
            out.patterns.push(pattern.clone());
        }
        let other = jadpo_std::Rules {
            min: None,
            max: None,
            min_length: None,
            max_length: None,
            pattern: None,
            scale: None,
            ..(*rules).clone()
        };
        if other != jadpo_std::Rules::default() {
            let text = format!("{other:?}");
            if !out.other.contains(&text) {
                out.other.push(text);
            }
        }
    }
    out.patterns.sort();
    out.other.sort();
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
        comment: None,
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
            // The encoding its payload was written in (area 19, "Changing
            // a standard type's encoding"; chunk 5.9). The runtime always
            // writes it; a migration adds it to a table that may hold rows
            // already, so it may be `NULL`, as a passkey's may.
            add("encoding", "smallint", true);
            // How many events led to it from the request that set the
            // chain off, which stops a loop at 16 (area 12, "Limits";
            // chunk 6.1b). Added to a table that may hold rows, so it may
            // be `NULL`, read as 0.
            add("depth", "integer", true);
            indexes.insert(
                object_name(&[&table, "emitted_at"], "idx"),
                jadpo_graph::schema::Index {
                    expression: None,
                    columns: vec!["emitted_at".into()],
                    reason: "system outbox".into(),
                    predicate: None,
                    collate: Vec::new(),
                    method: None,
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
            // `NULL` until it is first tried (chunk 6.1b).
            add("last_tried_at", "timestamptz(3)", true);
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
                    expression: None,
                    columns: vec!["status".into(), "next_attempt_at".into()],
                    reason: "system deliveries".into(),
                    predicate: None,
                    collate: Vec::new(),
                    method: None,
                },
            );
        }
        "handled_deliveries" => {
            // A subscriber's handled record (area 12, "Outbox placement,
            // settled"; chunk 6.1b): in the store its action writes,
            // committed with its writes, so a repeated delivery finds it
            // and does nothing. Keyed by the delivery's `id`, which the
            // outbox's store holds: no reference across stores.
            add("id", "uuid", false);
            add("handled_at", "timestamptz(3)", false);
        }
        "schedules" => {
            // Each schedule's state (area 12, "Schedules"; chunk 6.2): the
            // interval its due times were worked out with, when it is next
            // due, whether an operator paused it, the lease that lets one
            // instance run it at a time and the run holding it, and the last
            // tick that reached it. In `schedules: { store: }`.
            add("schedule", "text", false);
            add("every_ms", "bigint", false);
            add("next_due_at", "timestamptz(3)", false);
            add("paused", "boolean", false);
            add("lease_until", "timestamptz(3)", true);
            add("run_id", "uuid", true);
            add("last_tick_at", "timestamptz(3)", true);
            primary = vec!["schedule".into()];
        }
        "schedule_runs" => {
            // Each run of a schedule, and each due tick it skipped and why
            // (area 12, "Watching schedules"; chunk 6.2): when it was due,
            // started and ended, how it ended with its failure's code and
            // kind, never a value, how many events it emitted, and its
            // trace. Kept 30 days.
            add("id", "uuid", false);
            add("schedule", "text", false);
            add("due_at", "timestamptz(3)", false);
            add("started_at", "timestamptz(3)", false);
            add("ended_at", "timestamptz(3)", true);
            add("outcome", "text", false);
            add("reason", "text", true);
            add("failure", "jsonb", true);
            add("events", "integer", true);
            add("traceparent", "text", true);
            for columns in [vec!["schedule", "started_at"], vec!["started_at"]] {
                indexes.insert(
                    object_name(
                        &std::iter::once(table.as_str())
                            .chain(columns.iter().copied())
                            .collect::<Vec<_>>(),
                        "idx",
                    ),
                    jadpo_graph::schema::Index {
                        expression: None,
                        columns: columns.iter().map(|c| c.to_string()).collect(),
                        reason: "system schedule runs".into(),
                        predicate: None,
                        collate: Vec::new(),
                        method: None,
                    },
                );
            }
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
                    expression: None,
                    columns: vec!["principal_id".into()],
                    reason: "system sessions".into(),
                    predicate: None,
                    collate: Vec::new(),
                    method: None,
                },
            );
            indexes.insert(
                object_name(&[&table, "ends_at"], "idx"),
                jadpo_graph::schema::Index {
                    expression: None,
                    columns: vec!["ends_at".into()],
                    reason: "system sessions".into(),
                    predicate: None,
                    collate: Vec::new(),
                    method: None,
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
                    expression: None,
                    columns: vec!["principal_id".into()],
                    reason: "system service keys".into(),
                    predicate: None,
                    collate: Vec::new(),
                    method: None,
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
            // The encoding `credential` was written in (chunk 5.9); `NULL`
            // for a passkey kept before encodings were recorded.
            add("encoding", "smallint", true);
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
                    expression: None,
                    columns: vec!["principal_id".into()],
                    reason: "system passkeys".into(),
                    predicate: None,
                    collate: Vec::new(),
                    method: None,
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
                    expression: None,
                    columns: vec!["expires_at".into()],
                    reason: "system passkey challenges".into(),
                    predicate: None,
                    collate: Vec::new(),
                    method: None,
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
                    expression: None,
                    columns: vec!["kept_until".into()],
                    reason: "system idempotency keys".into(),
                    predicate: None,
                    collate: Vec::new(),
                    method: None,
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
