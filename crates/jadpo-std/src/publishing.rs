//! Events published outward, as plain data (area 12, "Where events
//! travel"; area 16, "Outgoing webhooks"): what `application {
//! publishing: }` chooses, each chosen event's delivery to its connection,
//! and the form it leaves the app in, which both executors and every host
//! share.
//!
//! A published event leaves through a connection with `driver:
//! Driver.webhook`, one address the operator gives: an HTTPS `POST` of
//! [`body`], signed as Standard Webhooks signs (`webhook-id`,
//! `webhook-timestamp`, `webhook-signature: v1,<Base64>`), the HMAC-SHA-256
//! of `<id>.<timestamp>.<body>` under the connection's secret, with the
//! trace context of the work that delivers it in `traceparent`. Its id is
//! the event's own, the same on every attempt, so a receiver keeps it once.

use serde::{Deserialize, Serialize};

/// The capability's one operation: hand one event outward.
pub const PUBLISH: &str = "publish";

/// What a delivery hands the connection's driver, a record of
/// [`PublishedEvent`]'s fields.
pub const PUBLISHED_EVENT: &str = "PublishedEvent";

/// The longest a receiver is waited for, each attempt: an answer that
/// hasn't come by then is tried again under the same id.
pub const TIMEOUT_MS: u64 = 15_000;

/// The identity key of the subscriber that publishes the event whose key
/// is `event` through `connection`: `publish partner_hooks type
/// planning.TodoCompleted`.
pub fn subscriber(connection: &str, event: &str) -> String {
    format!("publish {connection} {event}")
}

/// The type a published event is named by when its declaration gives no
/// `wire_name:`: its identity key without the kind, `planning.TodoCompleted`.
pub fn default_type(event_key: &str) -> String {
    event_key
        .split_once(' ')
        .map_or(event_key, |(_, name)| name)
        .to_string()
}

/// The body a published event is sent as: its id, its type and its
/// payload, the event's contract JSON, in that order.
pub fn body(id: &str, type_name: &str, data: &str) -> String {
    let id = serde_json::Value::String(id.to_string()).to_string();
    let type_name = serde_json::Value::String(type_name.to_string()).to_string();
    format!("{{\"id\":{id},\"type\":{type_name},\"data\":{data}}}")
}

/// The address a connection with `driver: Driver.webhook` posts to, and
/// the secret its deliveries are signed with (area 16, "Outgoing
/// webhooks"): `url:`, written or a setting's number, and `secret:`, a
/// secret setting's number, never read by code.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookTarget {
    pub url: crate::connections::Said,
    pub secret: u32,
}

/// One event `publishing:` chooses for one connection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishedPlan {
    /// The connection it leaves through.
    pub connection: String,
    /// The event type's identity key.
    pub event: String,
    /// The type it is named by outside: its `wire_name:`, or
    /// [`default_type`].
    pub type_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_published_event_is_named_by_its_key_without_the_kind() {
        assert_eq!(
            default_type("type planning.TodoCompleted"),
            "planning.TodoCompleted"
        );
        assert_eq!(
            default_type("jadpo.AuditEntryRecorded"),
            "jadpo.AuditEntryRecorded"
        );
    }

    #[test]
    fn its_body_holds_its_id_type_and_payload_in_order() {
        assert_eq!(
            body("4f0c", "planning.TodoCompleted", "{\"todo_id\":\"a\"}"),
            "{\"id\":\"4f0c\",\"type\":\"planning.TodoCompleted\",\"data\":{\"todo_id\":\"a\"}}"
        );
        // Text that needs escaping is escaped.
        assert_eq!(
            body("a\"b", "t", "null"),
            "{\"id\":\"a\\\"b\",\"type\":\"t\",\"data\":null}"
        );
    }
}
