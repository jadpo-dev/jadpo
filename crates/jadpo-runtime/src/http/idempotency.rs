//! Idempotent writes (area 7, question 17): every route but a `GET`
//! honours an `Idempotency-Key` a client sends, entirely in the runtime, so
//! the action never sees it. Keys live in the `jadpo_idempotency_keys`
//! system table of the store the action writes (area 5, "System tables").
//!
//! - Before the action runs, the key is claimed for the caller in one
//!   statement of its own ([`CLAIM`], run with `Database::query_once`): a
//!   row with no response yet, kept only as long as the route's deadline
//!   (and a margin), so a repeat that arrives meanwhile is told the first
//!   is still running (409 `conflict`), and a claim a crashed process left
//!   behind lapses by itself. The same statement gives back what the key is
//!   already kept with when it can't claim it.
//! - A key already kept answers instead of the action: the stored response
//!   when the request's digest matches, 422 `idempotency_key_reused` when
//!   it doesn't.
//! - After the action, its response is written over the claim, kept for 24
//!   hours: a success in the action's own transaction, before it commits;
//!   a failure the action returned (whose writes were rolled back) in one
//!   statement of its own. `unavailable`, a fault or a passed deadline
//!   stores nothing and drops the claim, in one statement of its own, so a
//!   retry runs again.
//! - A lookup ignores a row past its `kept_until`; deleting such rows comes
//!   with the runtime's minute tick (area 12, roadmap chunk 6.2).

use jadpo_std::http::{IDEMPOTENCY_KEEP_MS, IDEMPOTENCY_KEY_MAX};

use jadpo_value::Value;

use super::{Request, Response};
use crate::{Ctx, SqlValue};

/// The header a client sends its key in.
pub const HEADER: &str = "idempotency-key";

/// How long past its route's deadline a claim is kept, for the commit that
/// follows the action.
const CLAIM_MARGIN_MS: i64 = 5_000;

/// The scope a request no one signed in to keeps its keys under: every
/// such caller is the same anonymous one (area 8 keeps a signed-in
/// caller's under `user <id>` or `service <id>`).
pub const ANONYMOUS: &str = "anonymous";

/// A request's key, as the runtime keeps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Guard {
    /// The store the action writes, which keeps the key.
    pub store: String,
    /// Whose key it is.
    pub scope: String,
    pub key: String,
    /// The request's digest, which a repeat must match.
    pub digest: String,
    /// When the request started, in milliseconds since the epoch: the
    /// claim's `stored_at`, which names this request's claim.
    pub at: i64,
}

/// The key a request sends: `Ok(None)` without one, `Err(())` when it
/// isn't 1 to 255 printable ASCII characters.
pub(crate) fn key(request: &Request) -> Result<Option<String>, ()> {
    let Some(key) = request.header(HEADER) else {
        return Ok(None);
    };
    let valid = !key.is_empty()
        && key.len() <= IDEMPOTENCY_KEY_MAX
        && key
            .bytes()
            .all(|byte| byte.is_ascii_graphic() || byte == b' ');
    if valid {
        Ok(Some(key.to_string()))
    } else {
        Err(())
    }
}

/// The request's digest, which a repeat must match: its route's key (the
/// method and the path as declared, one per route), then what
/// it says as the route reads it, each placeholder and its input (the
/// query string, the headers it reads, the body), decoded. Every secret
/// value is left out but for its kind, so whoever reads the keys table
/// can't test a guessed password or token against a digest (chunk 5.8a):
/// a repeat that differs only in a secret is the same request.
pub fn digest(route: &str, params: &[Option<Value>], input: Option<&Value>) -> String {
    let mut hasher = blake3::Hasher::new();
    part(&mut hasher, route.as_bytes());
    count(&mut hasher, params.len());
    for param in params {
        feed(&mut hasher, param.as_ref().unwrap_or(&Value::None));
    }
    feed(&mut hasher, input.unwrap_or(&Value::None));
    hasher.finalize().to_hex().to_string()
}

fn count(hasher: &mut blake3::Hasher, n: usize) {
    hasher.update(&(n as u64).to_le_bytes());
}

/// `bytes`, after their length, so parts are never run together.
fn part(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    count(hasher, bytes.len());
    hasher.update(bytes);
}

/// `value` into `hasher`: a tag for its kind, then what it holds, each
/// part told apart from the next. A secret value gives its kind alone.
fn feed(hasher: &mut blake3::Hasher, value: &Value) {
    let tag = |hasher: &mut blake3::Hasher, tag: u8| {
        hasher.update(&[tag]);
    };
    let fields = |hasher: &mut blake3::Hasher, fields: &[(std::rc::Rc<str>, Value)]| {
        count(hasher, fields.len());
        for (name, value) in fields {
            part(hasher, name.as_bytes());
            feed(hasher, value);
        }
    };
    match value {
        Value::Unit => tag(hasher, 0),
        Value::None => tag(hasher, 1),
        Value::Bool(flag) => {
            tag(hasher, 2);
            hasher.update(&[u8::from(*flag)]);
        }
        Value::Int(n) => {
            tag(hasher, 3);
            hasher.update(&n.to_le_bytes());
        }
        Value::Decimal(d) => {
            tag(hasher, 4);
            hasher.update(&d.mantissa.to_le_bytes());
            hasher.update(&d.scale.to_le_bytes());
        }
        Value::Text(text) => {
            tag(hasher, 5);
            part(hasher, text.as_bytes());
        }
        Value::Bytes(bytes) => {
            tag(hasher, 6);
            part(hasher, bytes);
        }
        Value::Uuid(text) => {
            tag(hasher, 7);
            part(hasher, text.as_bytes());
        }
        Value::Instant(ms) => {
            tag(hasher, 8);
            hasher.update(&ms.to_le_bytes());
        }
        Value::Date(day) => {
            tag(hasher, 9);
            hasher.update(&day.to_le_bytes());
        }
        Value::Zoned(zoned) => {
            tag(hasher, 10);
            hasher.update(&zoned.millis.to_le_bytes());
            hasher.update(&zoned.zone.to_le_bytes());
        }
        Value::Duration(ms) => {
            tag(hasher, 11);
            hasher.update(&ms.to_le_bytes());
        }
        Value::Money(money) => {
            tag(hasher, 12);
            hasher.update(&money.currency.to_le_bytes());
            hasher.update(&money.minor.to_le_bytes());
        }
        Value::Quantity(quantity) => {
            tag(hasher, 13);
            part(hasher, format!("{:?}", quantity.kind).as_bytes());
            hasher.update(&quantity.amount.to_le_bytes());
        }
        Value::Percentage(d) => {
            tag(hasher, 14);
            hasher.update(&d.mantissa.to_le_bytes());
            hasher.update(&d.scale.to_le_bytes());
        }
        Value::List(items) | Value::Set(items) => {
            tag(
                hasher,
                if matches!(value, Value::List(_)) {
                    15
                } else {
                    16
                },
            );
            count(hasher, items.len());
            for item in items.iter() {
                feed(hasher, item);
            }
        }
        Value::Map(entries) => {
            tag(hasher, 17);
            count(hasher, entries.len());
            for (key, value) in entries.iter() {
                feed(hasher, key);
                feed(hasher, value);
            }
        }
        Value::Record(record) => {
            tag(hasher, 18);
            part(hasher, record.ty.as_bytes());
            fields(hasher, &record.fields);
            // A field left out isn't one sent as `none` (a patch's).
            let omitted = record.omitted.as_deref().unwrap_or_default();
            count(hasher, omitted.len());
            for at in omitted {
                hasher.update(&at.to_le_bytes());
            }
        }
        Value::Variant(variant) => {
            tag(hasher, 19);
            part(hasher, variant.ty.as_bytes());
            part(hasher, variant.name.as_bytes());
            fields(hasher, &variant.fields);
        }
        Value::Secret(secret) => {
            tag(hasher, 20);
            part(hasher, secret.kind().as_bytes());
        }
    }
}

/// What claiming a key found.
#[derive(Debug)]
pub enum Claim {
    /// The key is this request's now: run the action.
    Claimed,
    /// The key was kept with this request's response: answer with it.
    Replay(Response),
    /// The key was kept with a different request.
    Reused,
    /// A request with the key is still running.
    InFlight,
    /// The store couldn't be reached.
    Unavailable,
}

/// Claims the key, or, when another request holds it or kept a response
/// with it, gives back what it is kept with: a first column `true` when
/// this request claimed it, otherwise `false` and the kept row's digest,
/// status and response. A row past its `kept_until` is claimed over. All
/// of it reads the one snapshot the statement starts with, so a claim
/// another request made after that is found as neither, which is a request
/// still running.
pub(crate) const CLAIM: &str = "WITH \"claimed\" AS (INSERT INTO \"jadpo_idempotency_keys\" (\"scope\", \"key\", \"request_digest\", \"status\", \"response\", \"stored_at\", \"kept_until\") VALUES ($1, $2, $3, NULL, NULL, $4, $5) ON CONFLICT (\"scope\", \"key\") DO UPDATE SET \"request_digest\" = EXCLUDED.\"request_digest\", \"status\" = NULL, \"response\" = NULL, \"stored_at\" = EXCLUDED.\"stored_at\", \"kept_until\" = EXCLUDED.\"kept_until\" WHERE \"jadpo_idempotency_keys\".\"kept_until\" <= EXCLUDED.\"stored_at\" RETURNING \"scope\") SELECT TRUE, NULL::text, NULL::smallint, NULL::jsonb FROM \"claimed\" UNION ALL SELECT FALSE, \"request_digest\", \"status\", \"response\" FROM \"jadpo_idempotency_keys\" WHERE \"scope\" = $1 AND \"key\" = $2 AND \"kept_until\" > $4 AND NOT EXISTS (SELECT 1 FROM \"claimed\")";

/// Writes the response over this request's claim, giving back the row it
/// wrote, so on its own it says whether the claim was still this
/// request's.
pub const STORE: &str = "UPDATE \"jadpo_idempotency_keys\" SET \"status\" = $4, \"response\" = $5, \"kept_until\" = $6 WHERE \"scope\" = $1 AND \"key\" = $2 AND \"stored_at\" = $3 AND \"status\" IS NULL RETURNING \"scope\"";

pub(crate) const RELEASE: &str = "DELETE FROM \"jadpo_idempotency_keys\" WHERE \"scope\" = $1 AND \"key\" = $2 AND \"stored_at\" = $3 AND \"status\" IS NULL";

fn micros(millis: i64) -> SqlValue {
    SqlValue::Timestamp(millis.saturating_mul(1000))
}

/// Claims `guard`'s key for a request whose route's deadline is
/// `deadline_ms`, or finds what it is already kept with, in one statement.
pub async fn claim(ctx: &mut Ctx<'_>, guard: &Guard, deadline_ms: u32) -> Claim {
    let Some(database) = ctx.host().database(&guard.store) else {
        return Claim::Unavailable;
    };
    let lease = guard
        .at
        .saturating_add(i64::from(deadline_ms))
        .saturating_add(CLAIM_MARGIN_MS);
    let found = database
        .query_once(
            CLAIM,
            &[
                SqlValue::Text(guard.scope.clone()),
                SqlValue::Text(guard.key.clone()),
                SqlValue::Text(guard.digest.clone()),
                micros(guard.at),
                micros(lease),
            ],
        )
        .await;
    let Ok(rows) = found else {
        return Claim::Unavailable;
    };
    match rows.first().map(Vec::as_slice) {
        Some([SqlValue::Bool(true), ..]) => Claim::Claimed,
        Some(
            [
                SqlValue::Bool(false),
                SqlValue::Text(digest),
                status,
                response,
            ],
        ) => {
            if *digest != guard.digest {
                Claim::Reused
            } else {
                replay(status, response).map_or(Claim::InFlight, Claim::Replay)
            }
        }
        // Claimed by a request the statement's snapshot doesn't see yet:
        // still running, as far as this request can tell.
        _ => Claim::InFlight,
    }
}

/// A kept response, or `None` while its request is still running.
fn replay(status: &SqlValue, response: &SqlValue) -> Option<Response> {
    let status = match status {
        SqlValue::Int2(status) => u16::try_from(*status).ok()?,
        _ => return None,
    };
    let SqlValue::Json(kept) = response else {
        return None;
    };
    let kept: serde_json::Value = serde_json::from_str(kept).ok()?;
    let body = kept.get("body").and_then(serde_json::Value::as_str)?;
    let code = kept
        .get("code")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let headers = if body.is_empty() {
        Vec::new()
    } else {
        vec![("content-type".to_string(), "application/json".to_string())]
    };
    Some(Response {
        status,
        headers,
        body: body.as_bytes().to_vec(),
        code,
        ..Response::default()
    })
}

/// The parameters that write `response` over `guard`'s claim, with
/// [`STORE`].
pub fn store_params(guard: &Guard, response: &Response) -> Vec<SqlValue> {
    let mut kept = serde_json::Map::new();
    kept.insert(
        "code".to_string(),
        response
            .code
            .clone()
            .map_or(serde_json::Value::Null, serde_json::Value::String),
    );
    kept.insert(
        "body".to_string(),
        serde_json::Value::String(String::from_utf8_lossy(&response.body).into_owned()),
    );
    vec![
        SqlValue::Text(guard.scope.clone()),
        SqlValue::Text(guard.key.clone()),
        micros(guard.at),
        SqlValue::Int2(i16::try_from(response.status).unwrap_or(i16::MAX)),
        SqlValue::Json(serde_json::Value::Object(kept).to_string()),
        micros(guard.at.saturating_add(IDEMPOTENCY_KEEP_MS)),
    ]
}

/// Drops `guard`'s claim, so a retry runs the action again, in one
/// statement of its own. Best effort: a claim that can't be dropped lapses
/// with its deadline.
pub async fn release(ctx: &mut Ctx<'_>, guard: &Guard) {
    let Some(database) = ctx.host().database(&guard.store) else {
        return;
    };
    let _ = database
        .query_once(
            RELEASE,
            &[
                SqlValue::Text(guard.scope.clone()),
                SqlValue::Text(guard.key.clone()),
                micros(guard.at),
            ],
        )
        .await;
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use jadpo_value::{Record, Value};

    use super::digest;

    const SIGN_UP: &str = "route POST /accounts/sign-up";

    fn form(handle: &str, password: &str) -> Value {
        Value::record(
            "SignUpForm",
            vec![
                ("handle".into(), Value::text(handle)),
                (
                    "chosen_password".into(),
                    Value::secret("Password", password),
                ),
            ],
        )
    }

    /// Whoever reads the keys table can't test a password guess against a
    /// sign-up's digest: whatever the password, it is the same (chunk
    /// 5.8a). So is a secret in a placeholder.
    #[test]
    fn a_digest_leaves_out_every_secret_value() {
        let of = |input: &Value| digest(SIGN_UP, &[], Some(input));
        assert_eq!(
            of(&form("ann", "hunter2hunter2")),
            of(&form("ann", "correct horse battery"))
        );
        let token = |text: &str| [Some(Value::secret("BearerToken", text))];
        assert_eq!(
            digest("route POST /resets/{token}", &token("abc"), None),
            digest("route POST /resets/{token}", &token("xyz"), None)
        );
        // The redacted text it would show as is still text, not a secret.
        assert_ne!(
            of(&form("ann", "x")),
            of(&Value::record(
                "SignUpForm",
                vec![
                    ("handle".into(), Value::text("ann")),
                    ("chosen_password".into(), Value::text("<redacted Password>")),
                ],
            ))
        );
    }

    /// Everything else a request says counts, so a repeat must send the
    /// same request.
    #[test]
    fn a_digest_covers_the_route_and_everything_else_the_request_says() {
        let of = |input: &Value| digest(SIGN_UP, &[], Some(input));
        assert_ne!(of(&form("ann", "x")), of(&form("bo", "x")));
        assert_ne!(
            digest(SIGN_UP, &[], None),
            digest("route POST /accounts/sign-in", &[], None)
        );
        let name = |text: &str| [Some(Value::text(text))];
        assert_ne!(
            digest("route POST /widgets/{name}", &name("a"), None),
            digest("route POST /widgets/{name}", &name("b"), None)
        );
        // Parts are told apart, not run together.
        let texts = |items: &[&str]| Value::list(items.iter().map(|t| Value::text(t)).collect());
        assert_ne!(of(&texts(&["ab", "c"])), of(&texts(&["a", "bc"])));
        assert_ne!(of(&texts(&["a"])), of(&Value::set(vec![Value::text("a")])));
        assert_ne!(of(&Value::Int(1)), of(&Value::Duration(1)));
        assert_ne!(
            of(&Value::Bytes(Rc::from(&b"ab"[..]))),
            of(&Value::Bytes(Rc::from(&b"ba"[..])))
        );
        // A field left out isn't one sent as `none`: a patch tells them
        // apart.
        let patch = |omitted: Option<Rc<[u32]>>| {
            Value::Record(Rc::new(Record {
                ty: Rc::from("NotePatch"),
                fields: vec![(Rc::from("text"), Value::None)],
                omitted,
            }))
        };
        assert_ne!(of(&patch(None)), of(&patch(Some(Rc::from(&[0u32][..])))));
    }
}
