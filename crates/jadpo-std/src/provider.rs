//! Provider calls as plain data (area 11, "Calling" and "Decoding"; chunk
//! 7.2c): each connection to a provider package, how its address and
//! credential are read and how long a call waits, and each operation an
//! application calls through it, how it is sent and how its answer is
//! read. Both executors send a call from this through the capability seam
//! (`jadpo_runtime::provider`). Chunk 7.2c2 adds the built-in encodings
//! ([`WireEncoding`]), a whole answer's decoder and `retry:`.

use crate::HttpMethod;
use crate::subscriber::Repeat;

/// What a call waits for its answer when its connection gives no
/// `timeout:` (area 11, question 8).
pub const DEFAULT_TIMEOUT_MS: u64 = 10_000;

/// How often a call is tried, and for how long, when its connection gives
/// no `retry:` (area 11, question 8).
pub const DEFAULT_RETRY: RetryPlan = RetryPlan {
    attempts: 3,
    within_ms: 30_000,
};

/// The most a connection's `retry:` may try a call, and the longest it may
/// keep trying: a call is part of an action a person or a delivery waits
/// on (chunk 7.2c2).
pub const MAX_ATTEMPTS: u32 = 10;
pub const MAX_WITHIN_MS: u64 = 5 * 60 * 1000;

/// The first wait before a call is sent again, doubled after each attempt;
/// each wait is drawn evenly from zero to its ceiling (full jitter), and
/// none goes past the end of `within:` (chunk 7.2c2).
pub const RETRY_BASE_MS: u64 = 200;

/// A connection's `retry:`: how many times a call that may be sent again is
/// tried, the first included, and within how long of the first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RetryPlan {
    pub attempts: u32,
    pub within_ms: u64,
}

impl Default for RetryPlan {
    fn default() -> RetryPlan {
        DEFAULT_RETRY
    }
}

/// The built-in encodings a provider package's field names with `wire:`
/// (area 11, "Decoding"; chunk 7.2c2): a fixed list, extended by Jadpo
/// releases. A field without one is sent as the contract writes its type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WireEncoding {
    /// An `Instant` as a whole number of seconds since 1970.
    UnixSeconds,
    /// An `Instant` as a whole number of milliseconds since 1970.
    UnixMilliseconds,
    /// An `Instant` as ISO 8601 text with any offset from UTC
    /// (`2026-10-20T10:00:00+01:00`), written in UTC.
    Iso8601,
    /// A `CalendarDate` as `20/10/2026`.
    DayMonthYear,
    /// An `Int` or a `Decimal` as text: `"42"`, `"2.50"`.
    Text,
    /// An `Int` or a `Decimal` as a JSON number: `2.50`.
    Number,
    /// A list of single values as one text, joined by commas: `"a,b,c"`.
    CommaSeparated,
    /// `Text` as the Base64 of its UTF-8.
    Base64,
    /// Any value as its JSON, inside a string.
    JsonText,
    /// An amount of `Money` as a whole number of its currency's minor
    /// units, its one currency fixed by its type's `currency:`.
    MinorUnits,
}

impl WireEncoding {
    pub const ALL: [WireEncoding; 10] = [
        WireEncoding::UnixSeconds,
        WireEncoding::UnixMilliseconds,
        WireEncoding::Iso8601,
        WireEncoding::DayMonthYear,
        WireEncoding::Text,
        WireEncoding::Number,
        WireEncoding::CommaSeparated,
        WireEncoding::Base64,
        WireEncoding::JsonText,
        WireEncoding::MinorUnits,
    ];

    /// Its name after `Wire.`: `unix_seconds`.
    pub fn name(self) -> &'static str {
        match self {
            WireEncoding::UnixSeconds => "unix_seconds",
            WireEncoding::UnixMilliseconds => "unix_milliseconds",
            WireEncoding::Iso8601 => "iso_8601",
            WireEncoding::DayMonthYear => "day_month_year",
            WireEncoding::Text => "text",
            WireEncoding::Number => "number",
            WireEncoding::CommaSeparated => "comma_separated",
            WireEncoding::Base64 => "base64",
            WireEncoding::JsonText => "json_text",
            WireEncoding::MinorUnits => "minor_units",
        }
    }

    /// The encoding called `name` after `Wire.`.
    pub fn from_name(name: &str) -> Option<WireEncoding> {
        WireEncoding::ALL
            .into_iter()
            .find(|encoding| encoding.name() == name)
    }

    /// The values it holds, as a problem names them.
    pub fn holds(self) -> &'static str {
        match self {
            WireEncoding::UnixSeconds | WireEncoding::UnixMilliseconds | WireEncoding::Iso8601 => {
                "an `Instant`"
            }
            WireEncoding::DayMonthYear => "a `CalendarDate`",
            WireEncoding::Text | WireEncoding::Number => "an `Int` or a `Decimal`",
            WireEncoding::CommaSeparated => {
                "a list of text, numbers or an enum's variants without data"
            }
            WireEncoding::Base64 => "`Text`",
            WireEncoding::JsonText => "any value",
            WireEncoding::MinorUnits => "`Money` in one currency, fixed by `currency:`",
        }
    }
}

/// Every connection to a provider package an application calls, and the
/// operations it calls through them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProviderPlan {
    /// The application's name, which every derived idempotency key is
    /// made from (area 12, "Keys").
    pub application: String,
    pub connections: Vec<ConnectionPlan>,
    pub operations: Vec<OperationPlan>,
    /// Each connection given a webhook secret, whose provider's events the
    /// runtime receives at its generated route (area 11, "Receiving from
    /// outside"; chunk 7.7).
    pub webhooks: Vec<WebhookPlan>,
}

impl ProviderPlan {
    /// Operation `operation` (`bookings.create`) of connection
    /// `connection`, with the connection.
    pub fn operation(
        &self,
        connection: &str,
        operation: &str,
    ) -> Option<(&ConnectionPlan, &OperationPlan)> {
        let found = self.operations.iter().find(|op| {
            op.name == operation
                && self
                    .connections
                    .get(op.connection as usize)
                    .is_some_and(|plan| plan.name == connection)
        })?;
        Some((self.connections.get(found.connection as usize)?, found))
    }
}

/// How a credential is sent (area 11, `credential:`): the scheme the
/// package names, with the secret setting the connection reads it from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialScheme {
    /// `Authorization: Bearer <secret>`.
    Bearer,
    /// `Authorization: Basic <base64 of the secret>`: the secret is
    /// `user:password`, as RFC 7617 joins them.
    Basic,
}

/// Where a connection's API is (area 11, "Where an API is").
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Address {
    /// The package's `hosts:`, chosen by the `ProviderEnvironment` setting
    /// numbered `environment`.
    Hosts {
        live: Option<String>,
        sandbox: Option<String>,
        environment: u32,
    },
    /// The connection's `host:`, the `Url` setting numbered here.
    Setting(u32),
    /// Neither: a service Jadpo runs (`runs:`), not built yet.
    Unknown,
}

/// A provider's `idempotency:` as a call needs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdempotencyPlan {
    /// The header the key goes in.
    pub header: String,
    /// The longest key the provider takes: the derived key is cut to it.
    pub max_length: Option<u32>,
    /// The provider's code for a key whose first request is still
    /// running: wait and send again (area 12, "One runner per record").
    pub in_use: Option<String>,
    /// The provider's code for a key sent with a different request.
    pub reused: Option<String>,
}

/// One connection to a provider package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionPlan {
    /// Its name in `connections:`, `couriers`.
    pub name: String,
    /// The package's name and version, `AcmeCouriers.v1`.
    pub package: String,
    pub address: Address,
    /// The scheme its credential is sent with, and the secret setting it
    /// is read from.
    pub credential: Option<(CredentialScheme, u32)>,
    /// How long a call waits for its answer, in milliseconds.
    pub timeout_ms: u64,
    /// How a call that may be sent again is retried (chunk 7.2c2).
    pub retry: RetryPlan,
    pub idempotency: Option<IdempotencyPlan>,
    /// Where a failed answer's code is, dotted from its root.
    pub error_code: Option<String>,
    /// The operations `jadpo dev` lets through to the provider's sandbox,
    /// `pickups.create` (area 11, "Provider calls in development"; chunk
    /// 7.2d); every other call there is faked.
    pub sandbox: Vec<String>,
}

/// One part of an operation's path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathPart {
    Fixed(String),
    /// A placeholder: the input's member of this spelling on the wire.
    Field(String),
}

/// An operation an application calls.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperationPlan {
    /// Its connection's place in [`ProviderPlan::connections`].
    pub connection: u32,
    /// `bookings.create`.
    pub name: String,
    pub method: HttpMethod,
    pub path: Vec<PathPart>,
    /// Its input type's number in the plan's JSON table.
    pub input: Option<u32>,
    /// Its output type's number in the plan's JSON table; `None` when it
    /// gives nothing.
    pub output: Option<u32>,
    /// The answer as sent, `wire_output:`'s number in the plan's JSON
    /// table, and the identity key of the package's function, `decode:`,
    /// that turns it into the output (chunk 7.2c2).
    pub decode: Option<(u32, String)>,
    /// Where the real object is in an envelope, dotted.
    pub output_at: Option<String>,
    /// A `200` answer that is really a failure: the place, dotted, and
    /// the JSON value that marks it.
    pub failed_when: Option<(String, String)>,
    /// Whether it may be sent again: safe when it changes nothing or is
    /// marked idempotent without a key, keyed when it takes its package's
    /// key, unkeyed otherwise.
    pub repeat: Repeat,
    /// The identity keys of the package failures it answers with, whose
    /// codes the provider's error code is matched against.
    pub failures: Vec<String>,
}

/// What the call at one site needs besides its operation (chunk 7.2c2):
/// the fields of its output its code reads, the only ones decoded, and
/// whether its `attempt` maps `timed_out:`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct CallSite {
    /// The output's fields read, by name; `None` when the whole value is
    /// used (returned, passed on, or anything but a field read).
    pub reads: Option<Vec<String>>,
    /// Whether a timeout is the call's `timed_out` failure, which its
    /// `attempt` maps, rather than the runtime's `unavailable`.
    pub timed_out: bool,
}

/// How a provider signs its webhooks (area 11, "Receiving from outside";
/// chunk 7.7): the built-in schemes, extended by Jadpo releases, each
/// checked over the request's raw body.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SignatureScheme {
    /// Stripe's `Stripe-Signature: t=<seconds>,v1=<hex>`: HMAC-SHA-256 of
    /// `<t>.<body>` under the secret as written.
    Stripe,
    /// Standard Webhooks (Svix and others): `webhook-id`,
    /// `webhook-timestamp` and `webhook-signature: v1,<base64>`, or the
    /// same headers under `svix-`; HMAC-SHA-256 of `<id>.<timestamp>.<body>`
    /// under the Base64 after the secret's `whsec_`.
    StandardWebhooks,
    /// HMAC-SHA-256 of the body under the secret, in the header named here,
    /// as hex or Base64, with or without a leading `sha256=`. It carries
    /// no time, so no age is checked.
    HmacSha256 { header: String },
    /// No signature: HTTP basic authentication with the secret as
    /// `user:password` (Postmark).
    Basic,
}

impl SignatureScheme {
    /// The schemes built so far, by their names after `SignatureScheme.`;
    /// `twilio` and `sns` wait for chunk 7.7b.
    pub const BUILT: [&'static str; 4] = ["stripe", "standard_webhooks", "hmac_sha256", "basic"];

    /// Every scheme the language names, built or not.
    pub const NAMED: [&'static str; 6] = [
        "stripe",
        "standard_webhooks",
        "hmac_sha256",
        "basic",
        "twilio",
        "sns",
    ];

    /// The scheme called `name`, with `header` for `hmac_sha256`.
    pub fn from_name(name: &str, header: Option<&str>) -> Option<SignatureScheme> {
        Some(match name {
            "stripe" => SignatureScheme::Stripe,
            "standard_webhooks" => SignatureScheme::StandardWebhooks,
            "hmac_sha256" => SignatureScheme::HmacSha256 {
                header: header?.to_string(),
            },
            "basic" => SignatureScheme::Basic,
            _ => return None,
        })
    }
}

/// One connection's webhooks: where the runtime receives them, how they
/// are signed, where each event's id, type and time are in its body, the
/// events its package declares, and the store they are kept in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebhookPlan {
    /// The connection's name in `connections:`, `payments`.
    pub connection: String,
    /// The generated route's path, by segment: the `webhooks` API's
    /// prefix, then the connection's name.
    pub segments: Vec<String>,
    pub scheme: SignatureScheme,
    /// The number of the secret setting the signature is checked with.
    pub secret: u32,
    /// Where the provider's event id, its event type and the time the
    /// event happened are in the body, dotted from its root; `occurred_at`
    /// is `None` when the provider gives none there.
    pub event_id: String,
    pub event_type: String,
    pub occurred_at: Option<String>,
    /// The store the events and the ids already received are kept in.
    pub store: String,
    /// Each event the package declares, by its `wire_name:`.
    pub events: Vec<WebhookEvent>,
}

/// An event a provider's webhook carries (chunk 7.7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebhookEvent {
    /// The type the provider names it with in `event_type:`'s place,
    /// `payment_intent.succeeded`.
    pub wire_name: String,
    /// The event type's identity key, which its outbox row records.
    pub key: String,
    /// Its number in the plan's JSON table: the body is read with its
    /// codec, and its payload kept with it.
    pub number: u32,
}
