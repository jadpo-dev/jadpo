//! The calls `ai: { keep_content: }` keeps (area 21, "Prompts and answers
//! in logs"; chunk K6): an `ai.generate` call of a prompt it lists that a
//! model answered is kept, once, in `jadpo_ai_content` in the store it
//! names, outside any transaction as every outside call's answer is.
//!
//! Each row holds the prompt's name, the model, when, and the signed-in
//! caller it was made for, `NULL` for one made for no one; the messages
//! the last attempt sent, as a `List<ChatMessage>`'s JSON, and the
//! answer's text, as a `Text?`'s, are each sealed with the field ring and
//! bound to their column and the row's `id`, as an encrypted field is, so
//! a person's export opens them as it opens any sealed field. Erasure and
//! a delete of the caller's record delete their rows with their sessions,
//! and the tick deletes each once `keep:` has passed.

use jadpo_std::ai::{AiKeep, KEPT_ANSWER_BOUND, KEPT_MESSAGES_BOUND};
use jadpo_std::read::sealed_context;

use crate::read::uuid_text;
use crate::{Ctx, SqlType, SqlValue};

/// Keeps one call: `$1` to `$7` its identity, when it was made, its
/// prompt, its model, its caller, its sealed messages and its sealed
/// answer.
pub const KEEP: &str = "INSERT INTO \"jadpo_ai_content\" (\"id\", \"made_at\", \"prompt\", \"model\", \"principal_id\", \"messages\", \"answer\") VALUES ($1, $2, $3, $4, $5, $6, $7)";

/// Deletes up to `$2` calls made before `$1`, once `keep:` has passed.
pub const OLD_CONTENT: &str = "DELETE FROM \"jadpo_ai_content\" WHERE \"id\" IN (SELECT \"id\" FROM \"jadpo_ai_content\" WHERE \"made_at\" <= $1 LIMIT $2) RETURNING 1";

/// One call to keep.
pub(crate) struct Call<'c> {
    pub prompt: &'c str,
    /// The model's name as its setting holds it, `openai/gpt-6-luna`.
    pub model: &'c str,
    /// Each message the last attempt sent, its role and text.
    pub messages: &'c [(String, String)],
    /// The text the model answered with, if any.
    pub answer: Option<&'c str>,
}

/// Why a call couldn't be kept.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Unkept {
    /// The store couldn't be written: the call is `unavailable`.
    Unavailable,
    /// The run has no field key or randomness to seal it with, which a
    /// checked program always has: a defect naming what, never a value.
    Defect(&'static str),
}

/// Keeps `call` where `keep` says. Nothing is kept for a prompt `keep`
/// doesn't list.
pub(crate) async fn keep(ctx: &mut Ctx<'_>, keep: &AiKeep, call: Call<'_>) -> Result<(), Unkept> {
    if !keep.keeps(call.prompt) {
        return Ok(());
    }
    let Some(id) = ctx.new_id() else {
        return Err(Unkept::Defect(
            "a kept call with no randomness for its identity",
        ));
    };
    let started = ctx.started();
    let Some(ring) = started.field_ring() else {
        return Err(Unkept::Defect(
            "a call is kept sealed, and `field_encryption_key:` has no value in this run",
        ));
    };
    let messages: Vec<serde_json::Value> = call
        .messages
        .iter()
        .map(|(role, content)| serde_json::json!({ "role": role, "content": content }))
        .collect();
    let messages = serde_json::Value::Array(messages).to_string();
    let identity = uuid_text(&id);
    let mut seal = |text: &str, bound: &str| {
        jadpo_value::crypto::encrypt(
            text.as_bytes(),
            ring,
            &sealed_context(bound, &identity),
            &mut |out: &mut [u8]| ctx.secret_random_bytes(out),
        )
        .map(SqlValue::Bytes)
        .map_err(|_| Unkept::Defect("a kept call couldn't be sealed: the host gave no randomness"))
    };
    let messages = seal(&messages, KEPT_MESSAGES_BOUND)?;
    let answer = match call.answer {
        Some(text) => seal(
            &serde_json::Value::String(text.to_string()).to_string(),
            KEPT_ANSWER_BOUND,
        )?,
        None => SqlValue::Null(SqlType::Bytes),
    };
    let caller = principal_of(ctx).map_or(SqlValue::Null(SqlType::Uuid), SqlValue::Uuid);
    let Some(database) = ctx.host().database(&keep.store) else {
        return Err(Unkept::Unavailable);
    };
    let params = [
        SqlValue::Uuid(id),
        crate::outbox::micros(ctx.now()),
        SqlValue::Text(call.prompt.to_string()),
        SqlValue::Text(call.model.to_string()),
        caller,
        messages,
        answer,
    ];
    database
        .query_once(KEEP, &params)
        .await
        .map(|_| ())
        .map_err(|_| Unkept::Unavailable)
}

/// The record of the signed-in caller a call is made for: a user's or a
/// service's, by the `id` its principal holds.
fn principal_of(ctx: &Ctx<'_>) -> Option<[u8; 16]> {
    let jadpo_value::Value::Record(principal) = &ctx.caller()?.value else {
        return None;
    };
    match crate::read::to_sql(principal.get("id")?, SqlType::Uuid) {
        Ok(SqlValue::Uuid(id)) => Some(id),
        _ => None,
    }
}
