//! Handshakes (area 11, "Handshakes"; chunk 7.7b): how a provider checks
//! an address before it sends to it, answered by the runtime with the
//! rule the package names in `webhooks: { handshake: }`.
//!
//! - **Microsoft Graph** sends a `POST` with `validationToken` in its
//!   query, unsigned, and waits for the token back as plain text: answered
//!   before any signature is looked for, the token at most 1,024
//!   printable characters, sent as `text/plain` that no browser sniffs.
//! - **Slack** sends `{"type": "url_verification", "challenge": ...}`,
//!   signed as its events are: answered once the signature is checked,
//!   with the challenge back.
//! - **SNS** sends a `SubscriptionConfirmation`, signed as its messages
//!   are and from the connection's topic: confirmed by visiting its
//!   `SubscribeURL` on AWS's SNS host ([`super::sns::confirm`]).

use jadpo_std::provider::{WebhookHandshake, WebhookPlan};

use crate::Ctx;
use crate::http::{Request, Response, runtime_failure as failure};

use super::sns::Message;

/// The longest validation token answered.
const TOKEN_MOST: usize = 1024;

/// Microsoft Graph's validation, when `request` is one: its token, as
/// plain text.
pub(super) fn graph(request: &Request, request_id: &str) -> Option<Response> {
    let values = crate::http::query_values(request.query.as_deref()?)?;
    let token = values.get("validationToken")?;
    let [token] = token.as_slice() else {
        return Some(failure(super::NO_TOKEN, request_id, None));
    };
    if token.is_empty() || token.len() > TOKEN_MOST || !token.chars().all(|c| !c.is_control()) {
        return Some(failure(super::NO_TOKEN, request_id, None));
    }
    Some(Response {
        status: 200,
        headers: vec![
            (
                "content-type".to_string(),
                "text/plain; charset=utf-8".to_string(),
            ),
            ("x-content-type-options".to_string(), "nosniff".to_string()),
        ],
        body: token.clone().into_bytes(),
        ..Response::default()
    })
}

/// Slack's `url_verification`, when the signed body `json` is one: its
/// challenge, back as JSON.
pub(super) fn slack(json: &serde_json::Value) -> Option<Response> {
    if json.get("type")?.as_str()? != "url_verification" {
        return None;
    }
    let challenge = json.get("challenge")?.as_str()?;
    let mut body = String::from("{\"challenge\":");
    jadpo_value::json::write_string(challenge, &mut body);
    body.push('}');
    Some(Response {
        status: 200,
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        body: body.into_bytes(),
        ..Response::default()
    })
}

/// An SNS message that carries no event, checked: a confirmation the
/// package's handshake confirms, and anything else acknowledged.
pub(super) async fn sns(
    ctx: &Ctx<'_>,
    plan: &WebhookPlan,
    message: &Message,
    request_id: &str,
) -> Response {
    let confirming =
        plan.handshake == Some(WebhookHandshake::Sns) && message.kind == "SubscriptionConfirmation";
    if !confirming {
        return super::accepted();
    }
    let Some(url) = &message.confirm_at else {
        return super::accepted();
    };
    if super::sns::confirm(ctx.host().outbound(), url).await {
        super::accepted()
    } else {
        let mut unavailable = failure(crate::http::UNAVAILABLE, request_id, None);
        unavailable.code = Some("unavailable (confirmation)".into());
        unavailable
    }
}
