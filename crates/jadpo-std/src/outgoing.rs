//! Outgoing webhooks to the addresses customers register, as plain data
//! (area 16, "Outgoing webhooks"; chunk C1b): what an entity's `webhooks:`
//! section chooses, the operations generated on that entity, and the one
//! runtime-owned network exception every delivery leaves through.
//!
//! An entity whose `webhooks:` section lists events is the customer's: each
//! of its records registers addresses with `<Entity>.register_webhook(id,
//! url)`, which gives the address's own Standard Webhooks secret once
//! ([`SECRET_PREFIX`] and the Base64 of [`SECRET_BYTES`] random bytes),
//! lists them with `<Entity>.webhooks(id)` as [`REGISTERED_WEBHOOK`]s and
//! removes one with `<Entity>.remove_webhook(webhook_id)`, each under the
//! record's grants. Each listed event names its record in its one field
//! typed `<Entity>.id`, and its delivery posts it, as
//! [`crate::publishing::body`], to each address that record has, signed
//! with that address's secret, through [`CONNECTION`]: HTTPS only, to
//! public addresses checked as each attempt is made, no redirects
//! followed.

use serde::{Deserialize, Serialize};

/// The section on the customer's entity that lists the events its
/// records' addresses receive.
pub const SECTION: &str = "webhooks";

/// The runtime-owned network exception every delivery to a customer's
/// address leaves through: what the seam names its calls by, the network
/// report its row by, and an acceptance for it. No connection in
/// `app.jadpo` may take the name.
pub const CONNECTION: &str = "outgoing_webhooks";

/// The exception's one operation: post one signed event to one address.
pub const POST: &str = "post";

/// What a delivery hands [`CONNECTION`]'s driver for one address, a
/// record of [`OUTGOING_FIELDS`]: already signed, so the address's secret
/// never reaches the seam, its log or a fake.
pub const OUTGOING_WEBHOOK: &str = "OutgoingWebhook";

/// The fields of an [`OUTGOING_WEBHOOK`], in order.
pub const OUTGOING_FIELDS: [&str; 7] = [
    "url",
    "id",
    "type",
    "body",
    "timestamp",
    "signature",
    "traceparent",
];

/// What `<Entity>.webhooks(id)` lists, never an address's secret.
pub const REGISTERED_WEBHOOK: &str = "RegisteredWebhook";

/// The fields of a [`REGISTERED_WEBHOOK`], in order.
pub const REGISTERED_FIELDS: [(&str, crate::Builtin); 3] = [
    ("id", crate::Builtin::Uuid),
    ("url", crate::Builtin::Url),
    ("registered_at", crate::Builtin::Instant),
];

/// The most addresses one record may have registered at once, so one
/// event's delivery posts to a bounded number of them.
pub const MOST_ADDRESSES: i64 = 10;

/// What an address's secret starts with, Standard Webhooks' prefix,
/// before the Base64 of its bytes.
pub const SECRET_PREFIX: &str = "whsec_";

/// How many random bytes an address's secret is.
pub const SECRET_BYTES: usize = 32;

/// What each sealed secret is bound to, besides its address's `id`: it
/// opens for no other column and no other address.
pub const SECRET_BOUND: &str = "jadpo_webhook_addresses.secret";

/// The longest a registered address may be, in bytes.
pub const MOST_URL_BYTES: usize = 2048;

/// The identity key of the subscriber that delivers the event whose key
/// is `event` to the addresses `entity`'s records register:
/// `webhooks Shop type orders.OrderPlaced`.
pub fn subscriber(entity: &str, event: &str) -> String {
    format!("webhooks {entity} {event}")
}

/// The entity whose records register addresses, and the statements its
/// generated operations and each delivery run (chunk C1b).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WebhooksPlan {
    /// `Shop`.
    pub entity: String,
    /// The store it lives in, which keeps its addresses.
    pub store: String,
    /// `register_webhook`'s hold on the record, `$1` its `id`: one row,
    /// how many addresses it has, when the caller may update it, holding
    /// it until the action commits so two registrations count in turn;
    /// none when it may not, or there is no such record.
    pub lock: Option<crate::policy::Gated>,
    /// The same without policy, for a call a test makes without a caller.
    pub lock_plain: String,
    /// `webhooks(id)`: `$1` the record, its addresses oldest first, when
    /// the caller may read it.
    pub list: Option<crate::policy::Gated>,
    /// `remove_webhook(webhook_id)`: `$1` the address, when the caller may
    /// update its record.
    pub remove: Option<crate::policy::Gated>,
    /// What a delivery reads, as the system: `$1` the record, each of its
    /// addresses' `id`, `url` and sealed secret, oldest first, none once
    /// the record is gone.
    pub addresses: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_subscriber_is_named_by_its_entity_and_event() {
        assert_eq!(
            subscriber("Shop", "type orders.OrderPlaced"),
            "webhooks Shop type orders.OrderPlaced"
        );
    }
}
