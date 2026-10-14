//! Column types (area 5, "Storage types"): each field's type maps to its
//! store engine's native column type, and the source never names one; and
//! the system tables' columns.

use std::collections::BTreeMap;

use jadpo_graph::schema::{Column, Key, Owner, Table, object_name};
use jadpo_std::{Builtin, Num};

use crate::types::{Def, Ty, TypeView};

/// The most digits a Jadpo decimal holds (area 2).
const DECIMAL_DIGITS: u32 = 38;

/// The PostgreSQL column type for a field of type `ty` (its `?` left
/// aside), or the type's name when it has none in this compiler.
pub fn column_type(table: &dyn TypeView, ty: &Ty) -> Result<String, String> {
    let structure = table.structure(ty);
    Ok(match &structure {
        Ty::Prim(prim) => match prim.representation() {
            Builtin::Uuid => "uuid".into(),
            Builtin::Bool => "boolean".into(),
            Builtin::Text => "text".into(),
            Builtin::Bytes => "bytea".into(),
            Builtin::Instant => "timestamptz(3)".into(),
            Builtin::CalendarDate => "date".into(),
            Builtin::Time => "time(3)".into(),
            // Milliseconds, as the runtime holds a duration.
            Builtin::Duration => "bigint".into(),
            Builtin::Int => integer(table, ty),
            Builtin::Decimal => decimal(table, ty),
            Builtin::Unit | Builtin::Email | Builtin::Url | Builtin::IpAddress => {
                return Err(prim.name().to_string());
            }
        },
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
    }
}

/// A system table's columns and keys (area 5, "System tables"): the event
/// outbox, the delivery records of each event to each subscriber, and the
/// stored responses of idempotent requests. Areas 7 and 12 give them their
/// behaviour.
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
            add("updated_at", "timestamptz(3)", false);
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
            add("created_at", "timestamptz(3)", false);
            add("expires_at", "timestamptz(3)", false);
            primary = vec!["scope".into(), "key".into()];
            indexes.insert(
                object_name(&[&table, "expires_at"], "idx"),
                jadpo_graph::schema::Index {
                    columns: vec!["expires_at".into()],
                    reason: "system idempotency keys".into(),
                },
            );
        }
    }
    Table {
        owner: Owner::System(name.to_string()),
        columns,
        primary_key: Key {
            name: object_name(&[&table], "pkey"),
            columns: primary,
        },
        unique,
        foreign_keys,
        indexes,
    }
}
