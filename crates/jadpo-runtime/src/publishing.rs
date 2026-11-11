//! Events published outward (area 12, "Where events travel"; area 16,
//! "Outgoing webhooks"): what a delivery of a chosen event hands the
//! `publishing` capability, and how it is signed.
//!
//! The runner makes each delivery's [`Published`] from its event: the
//! event's id, the type it is named by outside, its payload as the
//! contract's JSON inside [`jadpo_std::publishing::body`], and the trace
//! context of the work delivering it. The capability's call carries it as
//! a [`jadpo_std::publishing::PUBLISHED_EVENT`] record, which a test's
//! fake and `jadpo dev`'s log read and a `Driver.webhook` connection's
//! driver posts, signed as Standard Webhooks signs ([`signature`]), so a
//! Jadpo app receiving it with `SignatureScheme.standard_webhooks` accepts
//! it. It is sent under its id, the same on every attempt, so a receiver
//! that keeps ids keeps it once: a retry is safe.

use std::rc::Rc;

use jadpo_value::Value;

use crate::capability::{Call, Repeat};
use crate::outbox::{Delivery, Events};

/// One published event, as its delivery hands it over.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Published {
    /// The event's id, its outbox row's, as a UUID's text.
    pub id: String,
    /// The type it is named by outside.
    pub type_name: String,
    /// Its whole body, [`jadpo_std::publishing::body`].
    pub body: String,
    /// The `traceparent` of the work delivering it.
    pub traceparent: Option<String>,
}

impl Published {
    /// What a capability's call is given.
    pub fn value(&self) -> Value {
        Value::record(
            jadpo_std::publishing::PUBLISHED_EVENT,
            vec![
                (Rc::from("id"), Value::text(&self.id)),
                (Rc::from("type"), Value::text(&self.type_name)),
                (Rc::from("body"), Value::text(&self.body)),
                (
                    Rc::from("traceparent"),
                    self.traceparent.as_deref().map_or(Value::None, Value::text),
                ),
            ],
        )
    }

    /// What `value`, a call's input, holds: `None` for anything that isn't
    /// a published event.
    pub fn from_value(value: &Value) -> Option<Published> {
        let Value::Record(record) = value else {
            return None;
        };
        if &*record.ty != jadpo_std::publishing::PUBLISHED_EVENT {
            return None;
        }
        let text = |name: &str| match record.get(name) {
            Some(Value::Text(text)) => Some(text.to_string()),
            _ => None,
        };
        Some(Published {
            id: text("id")?,
            type_name: text("type")?,
            body: text("body")?,
            traceparent: text("traceparent"),
        })
    }

    /// The call that publishes it through `connection`: sent again under
    /// its id while it may be, so it is keyed.
    pub fn call(&self, connection: &str) -> Call {
        Call::new(
            connection,
            jadpo_std::publishing::PUBLISH,
            self.value(),
            Repeat::Keyed,
        )
    }
}

/// The event `delivery` carries, published as `type_name`, delivered in
/// the trace `traceparent`; why not, never its values, when its payload
/// can't be written as its type is now.
pub fn published(
    events: &Events<'_>,
    delivery: &Delivery,
    type_name: &str,
    traceparent: Option<String>,
) -> Result<Published, String> {
    let data = if delivery.event == jadpo_std::audit::AUDIT_EVENT_KEY {
        crate::audit::published_data(&delivery.value)
            .ok_or_else(|| "an audit entry that isn't `AuditEntryRecorded`".to_string())?
    } else {
        let number = events
            .numbers
            .iter()
            .find(|known| known.key == delivery.event)
            .map(|known| known.number)
            .ok_or_else(|| "an event type this build doesn't declare".to_string())?;
        let codec = events
            .json
            .codec_of(number)
            .ok_or_else(|| "an event type the plan gave no codec".to_string())?;
        events.json.codecs.write_value(codec, &delivery.value)
    };
    let id = crate::read::uuid_text(&delivery.outbox);
    let body = jadpo_std::publishing::body(&id, type_name, &data);
    Ok(Published {
        id,
        type_name: type_name.to_string(),
        body,
        traceparent,
    })
}

/// The key a connection's `secret:` signs with: the Base64 after
/// Standard Webhooks' `whsec_`, or the secret's own bytes, as the webhooks
/// a Jadpo app receives are checked (area 11, "Receiving from outside").
pub fn signing_key(secret: &str) -> Vec<u8> {
    secret
        .strip_prefix("whsec_")
        .and_then(jadpo_value::encoding::from_base64)
        .filter(|bytes| !bytes.is_empty())
        .unwrap_or_else(|| secret.as_bytes().to_vec())
}

/// `webhook-signature`'s value for `body` sent with `id` at `seconds`
/// since 1970 under `key`: `v1,` and the Base64 of the HMAC-SHA-256 of
/// `<id>.<seconds>.<body>`.
pub fn signature(key: &[u8], id: &str, seconds: i64, body: &str) -> String {
    let seconds = seconds.to_string();
    let mac = jadpo_value::crypto::mac(
        key,
        &[
            id.as_bytes(),
            b".",
            seconds.as_bytes(),
            b".",
            body.as_bytes(),
        ],
    );
    format!("v1,{}", jadpo_value::encoding::to_base64(&mac))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bytes made for the test, never a key written in source.
    fn made(len: u8, from: u8) -> Vec<u8> {
        (0..len)
            .map(|at| at.wrapping_mul(37).wrapping_add(from))
            .collect()
    }

    #[test]
    fn a_whsec_secret_signs_with_its_decoded_bytes_and_any_other_with_its_own() {
        let raw = made(24, 3);
        let written = format!("whsec_{}", jadpo_value::encoding::to_base64(&raw));
        assert_eq!(signing_key(&written), raw);
        assert_eq!(signing_key("plain words"), b"plain words".to_vec());
        // `whsec_` with nothing after it is the secret's own bytes.
        assert_eq!(signing_key("whsec_"), b"whsec_".to_vec());
    }

    #[test]
    fn the_signature_is_the_hmac_of_id_time_and_body() {
        let key = made(32, 11);
        let signed = signature(&key, "msg", 1_700_000_000, "{}");
        let expected = jadpo_value::crypto::mac(&key, &[b"msg.1700000000.{}"]);
        assert_eq!(
            signed,
            format!("v1,{}", jadpo_value::encoding::to_base64(&expected))
        );
    }

    #[test]
    fn a_published_event_reads_back_from_its_value() {
        let published = Published {
            id: "0b9c".into(),
            type_name: "planning.TodoCompleted".into(),
            body: "{}".into(),
            traceparent: None,
        };
        assert_eq!(Published::from_value(&published.value()), Some(published));
        assert_eq!(Published::from_value(&Value::Unit), None);
    }
}
