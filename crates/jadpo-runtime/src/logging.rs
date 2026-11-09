//! What the runtime writes to the application's log about what it did
//! with a person's data (area 4, "Logging"; area 15, "Marked fields in
//! logs", "Sensitive fields" and "Access records").
//!
//! - **The keyed hash** ([`Hasher`]): a value logged as a hash covers the
//!   UTF-8 of its canonical text, or of its JSON where it has no single
//!   text form, under the key in [`LOG_HASH_KEY`], so one value hashes the
//!   same in every log and on both executors. A test's record keys it with
//!   the record's own key, and with no variable set a process draws a key
//!   of its own: its hashes match each other and nothing else, and
//!   `runtime.started` says so ([`key_source`]).
//! - **`failure.rejected`** ([`rejected`]): each failure that ends a route,
//!   a delivery or a schedule's run, its internal fields written by the
//!   rules a log follows: identifiers, numbers, enums, booleans and times
//!   as values, text and personal data as a hash, and a secret not at all.
//! - **Access records** ([`exported`], [`erased`], [`read`]): every export,
//!   erasure and read of a sensitive field, as a long-kept event with who
//!   asked, for which person's `id`, and when, never a value.

use std::sync::OnceLock;

use jadpo_value::Value;
use serde_json::{Map, Value as Json, json};

use crate::read::uuid_text;
use crate::{Ctx, Failure, Host, SqlValue};

/// The variable holding the key the log's hashes are made with.
pub const LOG_HASH_KEY: &str = "JADPO_LOG_HASH_KEY";

/// What marks an event as one to keep for a long time.
const KEPT: &str = "long";

/// A standard failure detail, which never holds a value (area 19).
const DECODE_PROBLEM: &str = "DecodeProblem";

/// A process's own key, drawn the first time one is needed.
static PROCESS_KEY: OnceLock<Option<[u8; 32]>> = OnceLock::new();

/// Makes the keyed hashes a log and a history keep.
pub(crate) struct Hasher {
    key: Option<Vec<u8>>,
}

impl Hasher {
    /// The hasher `host` logs with: a test's record's key, else the key in
    /// its environment, else the process's own.
    pub(crate) fn of(host: &dyn Host) -> Hasher {
        #[cfg(feature = "testing")]
        if let Some(record) = host.record() {
            return Hasher {
                key: Some(record.key().to_vec()),
            };
        }
        if let Some(key) = host.variable(LOG_HASH_KEY) {
            return Hasher {
                key: Some(key.into_bytes()),
            };
        }
        let key = PROCESS_KEY.get_or_init(|| {
            let mut key = [0; 32];
            host.random(&mut key).then_some(key)
        });
        Hasher {
            key: key.map(|key| key.to_vec()),
        }
    }

    /// `bytes`' keyed hash; `None` for a host with no key to give.
    pub(crate) fn hash(&self, bytes: &[u8]) -> Option<String> {
        let key = self.key.as_ref()?;
        let mac = jadpo_value::crypto::mac(key, &[bytes]);
        let hex: String = mac.iter().map(|byte| format!("{byte:02x}")).collect();
        Some(format!("hmac-sha256:{hex}"))
    }
}

/// Where the log's hashes get their key, for `runtime.started`.
pub fn key_source(host: &dyn Host) -> &'static str {
    if host.variable(LOG_HASH_KEY).is_some() {
        "variable"
    } else {
        "per_process"
    }
}

/// `value` as a log's hash of the whole of it: `null` for none, and for a
/// secret, which no hash covers.
fn hashed(value: &Value, hasher: &Hasher) -> Option<Json> {
    match value {
        Value::Unit | Value::None => Some(Json::Null),
        Value::Secret(_) | Value::Key(_) => None,
        Value::Bytes(bytes) => Some(hash_json(hasher, bytes)),
        _ => {
            let text = canonical(value)?;
            Some(hash_json(hasher, text.as_bytes()))
        }
    }
}

fn hash_json(hasher: &Hasher, bytes: &[u8]) -> Json {
    hasher.hash(bytes).map_or(Json::Null, Json::String)
}

/// A value's canonical text, as `text.from` gives it; its JSON where it
/// has no single text form; `None` when it holds a secret.
fn canonical(value: &Value) -> Option<String> {
    if let Ok(Value::Text(text)) = jadpo_value::standard::from(value) {
        return Some(text.to_string());
    }
    match value {
        Value::Variant(variant) if variant.fields.is_empty() => Some(variant.name.to_string()),
        _ => plain(value).map(|json| json.to_string()),
    }
}

/// `value` as JSON with its text as it is, for a whole value that has no
/// single text form and a problem that holds none; `None` for a secret
/// anywhere in it.
fn plain(value: &Value) -> Option<Json> {
    Some(match value {
        Value::Unit | Value::None => Json::Null,
        Value::Secret(_) | Value::Key(_) => return None,
        Value::Bool(flag) => Json::Bool(*flag),
        Value::Int(n) => json!(n),
        Value::Text(text) => Json::String(text.to_string()),
        Value::List(items) | Value::Set(items) => {
            Json::Array(items.iter().map(plain).collect::<Option<_>>()?)
        }
        Value::Record(record) => plain_fields(&record.fields)?,
        Value::Variant(variant) if variant.fields.is_empty() => {
            Json::String(variant.name.to_string())
        }
        Value::Variant(variant) => {
            let mut out = Map::new();
            out.insert("variant".to_string(), variant.name.to_string().into());
            for (name, part) in &variant.fields {
                out.insert(name.to_string(), plain(part)?);
            }
            Json::Object(out)
        }
        Value::Map(map) => {
            let mut out = Map::new();
            for (key, part) in map.iter() {
                out.insert(canonical(key)?, plain(part)?);
            }
            Json::Object(out)
        }
        other => match jadpo_value::standard::from(other) {
            Ok(Value::Text(text)) => Json::String(text.to_string()),
            _ => Json::Null,
        },
    })
}

fn plain_fields(fields: &[(std::rc::Rc<str>, Value)]) -> Option<Json> {
    let mut out = Map::new();
    for (name, part) in fields {
        out.insert(name.to_string(), plain(part)?);
    }
    Some(Json::Object(out))
}

/// `value` as a log writes it (area 4; area 17, "What the SQL shows"):
/// identifiers, numbers, enums, booleans and times as values, text as a
/// hash, and a secret left out (`None`).
fn logged(value: &Value, hasher: &Hasher) -> Option<Json> {
    Some(match value {
        Value::Unit | Value::None => Json::Null,
        Value::Secret(_) | Value::Key(_) => return None,
        Value::Bool(flag) => Json::Bool(*flag),
        Value::Int(n) => json!(n),
        Value::Text(text) => hash_json(hasher, text.as_bytes()),
        Value::Bytes(bytes) => hash_json(hasher, bytes),
        Value::List(items) | Value::Set(items) => Json::Array(
            items
                .iter()
                .filter_map(|item| logged(item, hasher))
                .collect(),
        ),
        Value::Map(map) => {
            let mut out = Map::new();
            for (key, part) in map.iter() {
                let (Some(key), Some(part)) = (logged(key, hasher), logged(part, hasher)) else {
                    continue;
                };
                let key = match key {
                    Json::String(text) => text,
                    other => other.to_string(),
                };
                out.insert(key, part);
            }
            Json::Object(out)
        }
        // A decoding problem says where and by which rule, and never holds
        // a value, so it is written as itself.
        Value::Record(record) if &*record.ty == DECODE_PROBLEM => plain_fields(&record.fields)?,
        Value::Record(record) => logged_fields(&record.fields, hasher),
        Value::Variant(variant) if variant.fields.is_empty() => {
            Json::String(variant.name.to_string())
        }
        Value::Variant(variant) => {
            let mut out = Map::new();
            out.insert("variant".to_string(), variant.name.to_string().into());
            if let Json::Object(fields) = logged_fields(&variant.fields, hasher) {
                out.extend(fields);
            }
            Json::Object(out)
        }
        other => match jadpo_value::standard::from(other) {
            Ok(Value::Text(text)) => Json::String(text.to_string()),
            _ => Json::Null,
        },
    })
}

fn logged_fields(fields: &[(std::rc::Rc<str>, Value)], hasher: &Hasher) -> Json {
    let mut out = Map::new();
    for (name, part) in fields {
        if let Some(part) = logged(part, hasher) {
            out.insert(name.to_string(), part);
        }
    }
    Json::Object(out)
}

/// A failure's fields as its event holds them.
fn failure_fields(failure: &Failure, hasher: &Hasher, ctx: &Ctx<'_>) -> Map<String, Json> {
    let mut out = Map::new();
    out.insert("code".to_string(), failure.code.clone().into());
    out.insert(
        "kind".to_string(),
        failure.kind.clone().map_or(Json::Null, Json::String),
    );
    let places = ctx.places();
    let site = failure.site().and_then(|frame| places.site(frame));
    out.insert(
        "site".to_string(),
        site.map_or(Json::Null, |(path, place)| {
            Json::String(format!("{path}:{}", place.line))
        }),
    );
    out.insert(
        "trace".to_string(),
        failure
            .trace(places)
            .into_iter()
            .map(Json::String)
            .collect::<Vec<_>>()
            .into(),
    );
    let mut internal = Map::new();
    for (name, value) in &failure.internal {
        let written = if failure.shown.iter().any(|shown| shown == name) {
            plain(value)
        } else if failure.hashed.iter().any(|hashed| hashed == name) {
            hashed(value, hasher)
        } else {
            logged(value, hasher)
        };
        if let Some(written) = written {
            internal.insert(name.clone(), written);
        }
    }
    out.insert("internal".to_string(), Json::Object(internal));
    if let Some(cause) = &failure.cause {
        out.insert(
            "cause".to_string(),
            Json::Object(failure_fields(cause, hasher, ctx)),
        );
    }
    out
}

/// Writes the `failure.rejected` event of `failure`, which ended what
/// `ctx` ran.
pub(crate) fn rejected(ctx: &Ctx<'_>, failure: &Failure, route: Option<&str>) {
    let host = ctx.host();
    let hasher = Hasher::of(host);
    let mut fields = failure_fields(failure, &hasher, ctx);
    if let (Some(route), Some(Json::Array(trace))) = (route, fields.get_mut("trace")) {
        trace.push(route.into());
    }
    fields.insert(
        "request_id".to_string(),
        ctx.request_id().map_or(Json::Null, Json::from),
    );
    host.log("failure.rejected", fields);
}

/// What every access record says: who asked and when, and that it is kept
/// for a long time.
fn access(ctx: &Ctx<'_>, entity: &str) -> Map<String, Json> {
    let (by, user, service) = crate::audit::who(ctx);
    let id = |id: Option<[u8; 16]>| id.map_or(Json::Null, |id| Json::String(uuid_text(&id)));
    let mut out = Map::new();
    out.insert("at".to_string(), instant(ctx.now()));
    out.insert("by".to_string(), by.name().into());
    out.insert("user_id".to_string(), id(user));
    out.insert("service_id".to_string(), id(service));
    out.insert("entity".to_string(), entity.into());
    out.insert("kept".to_string(), KEPT.into());
    out.insert(
        "request_id".to_string(),
        ctx.request_id().map_or(Json::Null, Json::from),
    );
    out
}

fn instant(at: i64) -> Json {
    match jadpo_value::standard::from(&Value::Instant(at)) {
        Ok(Value::Text(text)) => Json::String(text.to_string()),
        _ => Json::Null,
    }
}

/// An export of the person of `entity` with `id` was asked for.
pub(crate) fn exported(ctx: &Ctx<'_>, entity: &str, id: Option<&str>) {
    let mut fields = access(ctx, entity);
    fields.insert("person_id".to_string(), id.map_or(Json::Null, Json::from));
    ctx.host().log("privacy.exported", fields);
}

/// The event an erasure's access record is, for the person of `entity`
/// with `id`, to write once their erasure has committed.
pub(crate) fn erased(ctx: &Ctx<'_>, entity: &str, id: &str) -> Map<String, Json> {
    let mut fields = access(ctx, entity);
    fields.insert("person_id".to_string(), id.into());
    fields
}

/// What a read of a sensitive field read, for its access record.
pub(crate) struct Accessed {
    pub entity: String,
    pub fields: Vec<String>,
    pub record_id: Json,
    pub person_id: Json,
}

/// A column's `id` as an access record holds it.
pub(crate) fn id_of(row: &[SqlValue], at: u32) -> Json {
    match row.get(at as usize) {
        Some(SqlValue::Uuid(id)) => Json::String(uuid_text(id)),
        _ => Json::Null,
    }
}

/// Writes the access record of each sensitive read in `accessed`.
pub(crate) fn read(ctx: &Ctx<'_>, accessed: Vec<Accessed>) {
    for accessed in accessed {
        let mut fields = access(ctx, &accessed.entity);
        fields.insert("record_id".to_string(), accessed.record_id);
        fields.insert("person_id".to_string(), accessed.person_id);
        fields.insert("fields".to_string(), accessed.fields.into());
        ctx.host().log("privacy.read", fields);
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::*;

    fn key() -> Vec<u8> {
        std::process::id().to_be_bytes().to_vec()
    }

    fn hasher() -> Hasher {
        Hasher { key: Some(key()) }
    }

    fn hash_of(text: &str) -> Json {
        Json::String(format!(
            "hmac-sha256:{}",
            jadpo_value::crypto::mac(&key(), &[text.as_bytes()])
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        ))
    }

    /// A marked field is hashed from its canonical text, whatever its
    /// type, so a date never shows as a date (area 15, "Marked fields in
    /// logs").
    #[test]
    fn a_marked_field_is_hashed_from_its_canonical_text() {
        let date = Value::Date(20_386); // 2025-10-25
        let hashed_date = hashed(&date, &hasher()).expect("a hash");
        assert_eq!(
            hashed_date,
            hash_of(match jadpo_value::standard::from(&date) {
                Ok(Value::Text(text)) => Box::leak(text.to_string().into_boxed_str()),
                _ => panic!("a date has a text form"),
            })
        );
        assert_ne!(logged(&date, &hasher()), Some(hashed_date));
        assert_eq!(hashed(&Value::None, &hasher()), Some(Json::Null));
        assert_eq!(
            hashed(&Value::text("ann@example.com"), &hasher()),
            Some(hash_of("ann@example.com"))
        );
    }

    /// Text is hashed; identifiers, numbers, booleans and times are values
    /// (area 4; area 17, "What the SQL shows").
    #[test]
    fn a_failures_fields_follow_the_rule_for_logs() {
        let hash = hasher();
        assert_eq!(logged(&Value::Int(12), &hash), Some(json!(12)));
        assert_eq!(logged(&Value::Bool(true), &hash), Some(json!(true)));
        assert_eq!(logged(&Value::text("ann"), &hash), Some(hash_of("ann")));
        let uuid = "01234567-89ab-cdef-0123-456789abcdef";
        assert_eq!(
            logged(&Value::Uuid(Rc::from(uuid)), &hash),
            Some(Json::String(uuid.to_string()))
        );
        assert!(
            logged(&Value::Instant(0), &hash)
                .and_then(|json| json.as_str().map(str::to_string))
                .is_some_and(|time| time.starts_with("1970-01-01"))
        );
    }

    /// A secret reaches no log, as a value or as a hash, on its own or
    /// inside a record.
    #[test]
    fn a_secret_reaches_no_log() {
        let hash = hasher();
        let secret = jadpo_value::secret::Secret::new("Password", "correct horse");
        assert_eq!(logged(&Value::Secret(secret.clone()), &hash), None);
        assert_eq!(hashed(&Value::Secret(secret.clone()), &hash), None);
        let inner = Value::record("Held", vec![(Rc::from("tried"), Value::Secret(secret))]);
        assert_eq!(hashed(&inner, &hash), None);
        assert_eq!(logged(&inner, &hash), Some(json!({})));
    }

    /// A problem holds a place and a rule, never a value, so it is written
    /// as itself (area 19).
    #[test]
    fn a_decoding_problem_is_written_as_itself() {
        let problem = Value::record(
            DECODE_PROBLEM,
            vec![(Rc::from("rule"), Value::text("format"))],
        );
        assert_eq!(
            logged(&problem, &hasher()),
            Some(json!({ "rule": "format" }))
        );
    }
}
