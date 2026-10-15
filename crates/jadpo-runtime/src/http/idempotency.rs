//! Idempotent writes (area 7, question 17): every route but a `GET`
//! honours an `Idempotency-Key` a client sends, entirely in the runtime, so
//! the action never sees it. Keys live in the `jadpo_idempotency_keys`
//! system table of the store the action writes (area 5, "System tables").
//!
//! - Before the action runs, the key is claimed for the caller in a short
//!   transaction of its own: a row with no response yet, kept only as long
//!   as the route's deadline (and a margin), so a repeat that arrives
//!   meanwhile is told the first is still running (409 `conflict`), and a
//!   claim a crashed process left behind lapses by itself.
//! - A key already kept answers instead of the action: the stored response
//!   when the request's digest matches, 422 `idempotency_key_reused` when
//!   it doesn't.
//! - After the action, its response is written over the claim, kept for 24
//!   hours: a success in the action's own transaction, before it commits;
//!   a failure the action returned (whose writes were rolled back) in a
//!   transaction of its own. `unavailable`, a fault or a passed deadline
//!   stores nothing and drops the claim, so a retry runs again.
//! - A lookup ignores a row past its `kept_until`; deleting such rows comes
//!   with the runtime's minute tick (area 12, roadmap chunk 6.2).

use jadpo_std::http::{IDEMPOTENCY_KEEP_MS, IDEMPOTENCY_KEY_MAX, RoutePlan};

use super::{Request, Response};
use crate::{Ctx, Isolation, SqlValue};

/// The header a client sends its key in.
pub const HEADER: &str = "idempotency-key";

/// How long past its route's deadline a claim is kept, for the commit that
/// follows the action.
const CLAIM_MARGIN_MS: i64 = 5_000;

/// The scope keys are kept under until sign-in (area 8, roadmap chunk
/// 4.3): with no one signed in, every caller is the same anonymous one.
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

/// The request's digest: its method, path, query string, the headers its
/// input reads and its body, so a repeat must send the same request.
pub fn digest(request: &Request, route: &RoutePlan) -> String {
    let mut hasher = blake3::Hasher::new();
    let mut part = |bytes: &[u8]| {
        hasher.update(&(bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    };
    part(request.method.as_bytes());
    part(request.path.as_bytes());
    part(request.query.as_deref().unwrap_or("").as_bytes());
    if let Some(input) = &route.input {
        for (_, header) in &input.headers {
            part(request.header(header).unwrap_or("").as_bytes());
        }
    }
    part(&request.body);
    hasher.finalize().to_hex().to_string()
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

pub(crate) const CLAIM: &str = "INSERT INTO \"jadpo_idempotency_keys\" (\"scope\", \"key\", \"request_digest\", \"status\", \"response\", \"stored_at\", \"kept_until\") VALUES ($1, $2, $3, NULL, NULL, $4, $5) ON CONFLICT (\"scope\", \"key\") DO UPDATE SET \"request_digest\" = EXCLUDED.\"request_digest\", \"status\" = NULL, \"response\" = NULL, \"stored_at\" = EXCLUDED.\"stored_at\", \"kept_until\" = EXCLUDED.\"kept_until\" WHERE \"jadpo_idempotency_keys\".\"kept_until\" <= EXCLUDED.\"stored_at\" RETURNING \"scope\"";

pub(crate) const LOOKUP: &str = "SELECT \"request_digest\", \"status\", \"response\" FROM \"jadpo_idempotency_keys\" WHERE \"scope\" = $1 AND \"key\" = $2 AND \"kept_until\" > $3";

/// Writes the response over this request's claim.
pub const STORE: &str = "UPDATE \"jadpo_idempotency_keys\" SET \"status\" = $4, \"response\" = $5, \"kept_until\" = $6 WHERE \"scope\" = $1 AND \"key\" = $2 AND \"stored_at\" = $3 AND \"status\" IS NULL";

pub(crate) const RELEASE: &str = "DELETE FROM \"jadpo_idempotency_keys\" WHERE \"scope\" = $1 AND \"key\" = $2 AND \"stored_at\" = $3 AND \"status\" IS NULL";

fn micros(millis: i64) -> SqlValue {
    SqlValue::Timestamp(millis.saturating_mul(1000))
}

/// Claims `guard`'s key for a request whose route's deadline is
/// `deadline_ms`, or finds what it is already kept with.
pub async fn claim(ctx: &mut Ctx<'_>, guard: &Guard, deadline_ms: u32) -> Claim {
    let Some(database) = ctx.host().database(&guard.store) else {
        return Claim::Unavailable;
    };
    let Ok(mut transaction) = database.begin_at(Isolation::ReadCommitted).await else {
        return Claim::Unavailable;
    };
    let lease = guard
        .at
        .saturating_add(i64::from(deadline_ms))
        .saturating_add(CLAIM_MARGIN_MS);
    let claimed = transaction
        .query(
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
    let found = match claimed {
        Ok(rows) if !rows.is_empty() => Claim::Claimed,
        Ok(_) => {
            let kept = transaction
                .query(
                    LOOKUP,
                    &[
                        SqlValue::Text(guard.scope.clone()),
                        SqlValue::Text(guard.key.clone()),
                        micros(guard.at),
                    ],
                )
                .await;
            match kept {
                Ok(rows) => match rows.first().map(Vec::as_slice) {
                    Some([SqlValue::Text(digest), status, response]) => {
                        if *digest != guard.digest {
                            Claim::Reused
                        } else {
                            replay(status, response).map_or(Claim::InFlight, Claim::Replay)
                        }
                    }
                    // Released between the two statements: still running,
                    // as far as this request can tell.
                    _ => Claim::InFlight,
                },
                Err(_) => Claim::Unavailable,
            }
        }
        Err(_) => Claim::Unavailable,
    };
    match transaction.commit().await {
        Ok(()) => found,
        Err(_) => Claim::Unavailable,
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

/// Drops `guard`'s claim, so a retry runs the action again. Best effort:
/// a claim that can't be dropped lapses with its deadline.
pub async fn release(ctx: &mut Ctx<'_>, guard: &Guard) {
    let Some(database) = ctx.host().database(&guard.store) else {
        return;
    };
    let Ok(mut transaction) = database.begin_at(Isolation::ReadCommitted).await else {
        return;
    };
    let dropped = transaction
        .execute(
            RELEASE,
            &[
                SqlValue::Text(guard.scope.clone()),
                SqlValue::Text(guard.key.clone()),
                micros(guard.at),
            ],
        )
        .await;
    let _ = match dropped {
        Ok(_) => transaction.commit().await,
        Err(_) => transaction.rollback().await,
    };
}
