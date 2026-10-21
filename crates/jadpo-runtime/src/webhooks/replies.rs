//! Webhooks that need a reply (area 11, "Webhooks that need a reply";
//! chunk 7.7b): the package's event names its `reply:` type, and the
//! connection's `webhooks: { replies: }` the app's action that answers it,
//! run inside the provider's request once its signature is checked.
//!
//! - The event's id is claimed first, in `jadpo_received_events` in a
//!   statement of its own, with no reply yet; a repeat that finds the
//!   reply kept is answered with it, one that finds none while the claim
//!   is younger than [`CLAIM_MS`] is told the first is still being
//!   answered (`409`), and a claim older than that, left by a process
//!   that stopped, is taken over.
//! - The action runs as the system, reaching no tenant's records, in its
//!   own transaction, as a first sign-in's does; the event isn't kept in
//!   the outbox, so no subscriber receives it. What its reply may carry
//!   the checker proves, as an output to `Access.public` (chunk 7.9b).
//! - Its reply, encoded with the package's type, is kept over the claim
//!   once the action has committed, and sent as JSON. A failure gives the
//!   claim back, so the provider's retry runs the action again: the
//!   failure the action declares answered as a route answers it,
//!   `unavailable` with `503`, a fault with `500`.
//!
//! The reply is kept in a statement after the action's commit, not in its
//! transaction, since the action may write another store: if that
//! statement fails the claim stays, lapses after [`CLAIM_MS`], and a retry
//! after that runs the action again.

use jadpo_std::provider::{WebhookPlan, WebhookReply};
use jadpo_value::Value;

use crate::http::{App, Response};
use crate::{Ctx, Executor, Outcome, SqlValue, System};

use super::{Webhooks, source};

/// How long a claim on an event id holds before another request may take
/// it over: longer than any provider waits for an answer.
pub const CLAIM_MS: i64 = 60_000;

/// Claims an event's id, with no reply yet: `RETURNING` a row only when
/// it wasn't held.
pub const CLAIM: &str = "INSERT INTO \"jadpo_received_events\" (\"source\", \"event_id\", \"received_at\", \"reply\") VALUES ($1, $2, $3, NULL) ON CONFLICT DO NOTHING RETURNING 1";

/// What an id already held is kept with.
pub const FIND: &str = "SELECT \"reply\", \"received_at\" FROM \"jadpo_received_events\" WHERE \"source\" = $1 AND \"event_id\" = $2";

/// Takes over a claim made before `$4` with no reply kept.
pub const RETAKE: &str = "UPDATE \"jadpo_received_events\" SET \"received_at\" = $3 WHERE \"source\" = $1 AND \"event_id\" = $2 AND \"reply\" IS NULL AND \"received_at\" <= $4 RETURNING 1";

/// Keeps the reply over this request's claim.
pub const KEEP: &str = "UPDATE \"jadpo_received_events\" SET \"reply\" = $4 WHERE \"source\" = $1 AND \"event_id\" = $2 AND \"received_at\" = $3 AND \"reply\" IS NULL RETURNING 1";

/// Gives this request's claim back.
pub const RELEASE: &str = "DELETE FROM \"jadpo_received_events\" WHERE \"source\" = $1 AND \"event_id\" = $2 AND \"received_at\" = $3 AND \"reply\" IS NULL RETURNING 1";

const STILL_ANSWERING: (u16, &str, &str) =
    (409, "conflict", "The webhook is still being answered.");

/// How a claim on an event's id went.
enum Claimed {
    /// This request holds it, made at the time given.
    Held,
    /// The reply kept for it.
    Kept(String),
    /// Another request holds it.
    Busy,
}

/// Answers a webhook's event, its id `id` and its value `value`, with
/// the reply the action `reply` names gives.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn answer(
    app: &App<'_>,
    executor: &dyn Executor,
    webhooks: &Webhooks<'_>,
    plan: &WebhookPlan,
    reply: &WebhookReply,
    ctx: &mut Ctx<'_>,
    request_id: &str,
    (id, value): (&str, Value),
) -> Response {
    use crate::http::{INTERNAL_FAULT, UNAVAILABLE, runtime_failure as failure};
    let Some(callable) = executor.callable(&reply.action) else {
        return super::internal(request_id, "a webhook's reply with no action to run");
    };
    let Some(database) = ctx.capabilities().database(&plan.store) else {
        return super::internal(request_id, "a webhook's store the host doesn't give");
    };
    let now = ctx.now();
    let at = crate::outbox::micros(now);
    let key = [SqlValue::Text(source(plan)), SqlValue::Text(id.to_string())];
    let claimed = match claim(database, &key, &at, now).await {
        Ok(claimed) => claimed,
        Err(_) => return failure(UNAVAILABLE, request_id, None),
    };
    match claimed {
        Claimed::Held => {}
        Claimed::Kept(reply) => return with_reply(reply),
        Claimed::Busy => return failure(STILL_ANSWERING, request_id, None),
    }
    ctx.become_system(System::NO_TENANT);
    let outcome = executor.run(ctx, callable, vec![value]).await;
    ctx.set_caller(None);
    let answered = match outcome {
        Outcome::Value(answered) => answered,
        Outcome::Failed(declared) => {
            release(database, &key, &at).await;
            if declared.kind.as_deref() == Some("unavailable") {
                return failure(UNAVAILABLE, request_id, None);
            }
            return crate::http::declared_failure(app, &declared, request_id);
        }
        Outcome::Fault(fault) => {
            release(database, &key, &at).await;
            let mut response = failure(INTERNAL_FAULT, request_id, None);
            response.fault = Some(fault);
            return response;
        }
        Outcome::Assertion(_) => {
            release(database, &key, &at).await;
            return failure(INTERNAL_FAULT, request_id, None);
        }
    };
    let Some(codec) = webhooks.json.codec_of(reply.number) else {
        release(database, &key, &at).await;
        return super::internal(request_id, "a webhook's reply the plan gave no codec");
    };
    let wire = super::Wire;
    let codecs = jadpo_value::json::Codecs {
        coder: Some(&wire),
        ..webhooks.json.codecs
    };
    let written = codecs.write_value(codec, &answered);
    let params = [
        key[0].clone(),
        key[1].clone(),
        at.clone(),
        SqlValue::Text(written.clone()),
    ];
    // Kept for a repeat; if it can't be, the claim lapses and a retry
    // after that runs the action again, as the module says.
    let _ = database.query_once(KEEP, &params).await;
    with_reply(written)
}

/// Claims `key` at `at` (milliseconds `now`), or finds what holds it.
async fn claim(
    database: &dyn crate::Database,
    key: &[SqlValue; 2],
    at: &SqlValue,
    now: i64,
) -> Result<Claimed, crate::StoreError> {
    let params = [key[0].clone(), key[1].clone(), at.clone()];
    if !database.query_once(CLAIM, &params).await?.is_empty() {
        return Ok(Claimed::Held);
    }
    let rows = database.query_once(FIND, key).await?;
    match rows.first().map(Vec::as_slice) {
        Some([SqlValue::Text(reply), _]) => return Ok(Claimed::Kept(reply.clone())),
        // Received as an event kept for subscribers, or answered and then
        // its package changed: nothing to answer with but the claim.
        Some([SqlValue::Null(_), _]) => {}
        // Deleted since: claimed afresh by the next attempt.
        _ => return Ok(Claimed::Busy),
    }
    let lapsed = crate::outbox::micros(now - CLAIM_MS);
    let params = [key[0].clone(), key[1].clone(), at.clone(), lapsed];
    if database.query_once(RETAKE, &params).await?.is_empty() {
        Ok(Claimed::Busy)
    } else {
        Ok(Claimed::Held)
    }
}

/// Gives the claim on `key` made at `at` back, so a retry runs again.
async fn release(database: &dyn crate::Database, key: &[SqlValue; 2], at: &SqlValue) {
    let params = [key[0].clone(), key[1].clone(), at.clone()];
    let _ = database.query_once(RELEASE, &params).await;
}

/// `200` with the reply, its package's JSON.
fn with_reply(reply: String) -> Response {
    Response {
        status: 200,
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        body: reply.into_bytes(),
        ..Response::default()
    }
}
