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
/// `timeout:` (area 11, question 8): 3 seconds, so its 3 attempts fit a
/// route's 10-second default deadline (Richard, 2026-10-20).
pub const DEFAULT_TIMEOUT_MS: u64 = 3_000;

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

/// The setting a provider package's field reads its standard type's parts
/// from its object's own members with (area 11, "Mapping imported types
/// to standard types"; chunk 7.2e): `amount: Money { wire_parts: { amount:
/// "amount" currency: "currency" } }`, so Stripe's `"amount": 1050,
/// "currency": "gbp"` is one `Money`.
pub const WIRE_PARTS: &str = "wire_parts";

/// The parts standard type `standard` is read from with `wire_parts:`, in
/// its own order: a type sent as an object of named members (`Money`'s
/// `amount` and `currency`, `InstantRange`'s `start` and `end`), and a
/// `Point` by its `latitude` and `longitude`, which GeoJSON sends as a
/// pair. `None` for a type made of no parts.
pub fn wire_parts(standard: &str) -> Option<Vec<&'static str>> {
    use crate::standard::Json;
    if standard == crate::standard::geo::POINT {
        return Some(vec!["latitude", "longitude"]);
    }
    match crate::standard_type(standard)?.json {
        Json::Parts(parts) => Some(parts.iter().map(|(name, _)| *name).collect()),
        Json::Fields(fields) => Some(fields.to_vec()),
        _ => None,
    }
}

/// The common names a part of a standard type goes by, from the shape
/// `type.standard_exists` matches (area 19, question 5): a `Money`'s
/// `amount`, `value`, `cents`; a `Point`'s `lat` and `lng`.
fn shape_names(standard: &str, meaning: &str) -> &'static [&'static str] {
    crate::standard_type(standard)
        .and_then(|entry| entry.shape)
        .and_then(|shape| shape.iter().find(|field| field.meaning == meaning))
        .map_or(&[], |field| field.names)
}

/// Whether a field named `name` is an amount by `Money`'s shape:
/// `amount`, `value`, `cents`, `pence`.
pub fn amount_named(name: &str) -> bool {
    shape_names("Money", "an amount").contains(&name)
}

/// Whether a field named `name` reads as an amount of money though the
/// shape doesn't match it: `amount_refunded`, `total_amount`.
pub fn amount_like(name: &str) -> bool {
    amount_named(name) || name.starts_with("amount_") || name.ends_with("_amount")
}

/// Whether a field named `name` is a currency by `Money`'s shape.
pub fn currency_named(name: &str) -> bool {
    shape_names("Money", "a currency").contains(&name)
}

/// Whether a field named `name` is a latitude by `Point`'s shape.
pub fn latitude_named(name: &str) -> bool {
    shape_names(crate::standard::geo::POINT, "a latitude").contains(&name)
}

/// Whether a field named `name` is a longitude by `Point`'s shape.
pub fn longitude_named(name: &str) -> bool {
    shape_names(crate::standard::geo::POINT, "a longitude").contains(&name)
}

/// Whether a whole number named `name` reads as a time (area 11,
/// "Mapping imported types to standard types"): `created`, `expires`,
/// `paid_at`, `start_time`, a candidate `Instant` sent as seconds.
pub fn time_named(name: &str) -> bool {
    matches!(
        name,
        "created" | "updated" | "deleted" | "expires" | "timestamp" | "time"
    ) || ["_at", "_time", "_timestamp"]
        .iter()
        .any(|end| name.len() > end.len() && name.ends_with(end))
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
    /// Every recording of the app's own integrations (area 11,
    /// "Provenance and confirmation"; chunk 7.2d2), which `jadpo test` and
    /// the publishing bar decode with its operation's decoders.
    pub recorded: Vec<RecordedPlan>,
    /// Each queue the app receives through `receiving:` (area 12, "Events
    /// from outside"; chunk 7.7c), fetched by the tick through its
    /// connection's package.
    pub queues: Vec<crate::receiving::QueuePlan>,
    /// Each connection's polls whose event the app subscribes to (area
    /// 11, "Polls"; chunk 7.6c), called by the tick through it.
    pub polls: Vec<PollPlan>,
}

/// One poll the tick runs (area 11, "Polls"; chunk 7.6c): a connection's
/// call of its package's operation, at its interval, whose answer is made
/// into the package's event and kept only when it changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PollPlan {
    /// The connection it calls through, `fuel`.
    pub connection: String,
    /// Its name in the package's `polls:`, `latest`.
    pub name: String,
    /// The operation it calls, `prices.latest`, which takes nothing.
    pub operation: String,
    /// How often it is called: the connection's interval, or its
    /// package's `every:`.
    pub every_ms: u64,
    /// The event's identity key, which its outbox row records.
    pub event: String,
    /// The event's number in the plan's JSON table: its payload is
    /// written with its codec.
    pub number: u32,
    /// The event's fields, each taken from the answer's field of its name.
    pub fields: Vec<String>,
    /// The store its events, and what it last saw, are kept in.
    pub store: String,
}

impl PollPlan {
    /// How its state is named in `jadpo_polls`: `poll fuel.latest`.
    pub fn key(&self) -> String {
        format!("poll {}.{}", self.connection, self.name)
    }
}

/// One recording, `recordings/<resource>.<operation>.json`, of a package
/// the app's own integration holds (chunk 7.2d2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordedPlan {
    /// Its path in the project.
    pub file: String,
    /// The package's name and version, `AcmeCouriers.v1`.
    pub package: String,
    /// `pickups.create`, from the file's name.
    pub operation: String,
    /// The operation it records, planned as a call through a connection
    /// would be (its `connection` is `u32::MAX` when no connection calls
    /// it); `None` when the package has no such operation.
    pub plan: Option<OperationPlan>,
    /// The recording as committed.
    pub text: String,
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
    /// AWS Signature Version 4 (chunk 7.7c): each request signed with the
    /// secret access key, which is the connection's secret setting, under
    /// the access key id the setting numbered here holds.
    AwsSigV4 { key_id: u32 },
    /// The secret alone, in the header the connection's
    /// [`ConnectionPlan::credential_header`] names (chunk 5.5v7), as
    /// Anthropic's `x-api-key`.
    Header,
}

/// How a provider's bodies are sent (area 11, `protocol:`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Protocol {
    /// JSON over HTTP, each operation at its method and path.
    #[default]
    JsonHttp,
    /// AWS's JSON protocol (chunk 7.7c): every operation a `POST` to `/`
    /// with `Content-Type: application/x-amz-json-1.0`, named by
    /// `X-Amz-Target: <target>.<Operation>`.
    AwsJson,
}

/// A provider's `aws: { service: target: }` (chunk 7.7c): the service
/// name its requests are signed for, `sqs`, and the prefix its operations
/// are named under in `X-Amz-Target`, `AmazonSQS`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AwsPlan {
    pub service: String,
    pub target: Option<String>,
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
    /// How long the provider keeps a key, in milliseconds (`window:`): a
    /// step in doubt is sent again under its key only within it (area 11,
    /// "Keys and windows"; chunk 7.5b).
    pub window_ms: Option<u64>,
    /// The provider's code for "the first attempt failed and the key can't
    /// be used again" (`new_key_on:`): sent again under the next key.
    pub new_key_on: Option<String>,
    /// Whether the provider replays a failed answer under the same key
    /// (`replays_failures: true`), so a `5xx` it answered parks the step.
    pub replays_failures: bool,
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
    /// The header [`CredentialScheme::Header`] sends the credential in.
    pub credential_header: Option<String>,
    /// The fixed headers every request carries, each its name and value
    /// (chunk 5.5v7).
    pub headers: Vec<(String, String)>,
    /// How its bodies are sent, and, for an AWS API, what its requests are
    /// signed for and named under (chunk 7.7c).
    pub protocol: Protocol,
    pub aws: Option<AwsPlan>,
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
    /// How many of its calls go out at once per app instance (area 11,
    /// "Calling"; chunk 7.6c): its package's `max_concurrent:`, or the
    /// connection's lower one; `None` for no cap.
    pub max_concurrent: Option<u32>,
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
    /// Whether it changes something (`changes: true`): a step of an action
    /// with several changing calls from the first such call on (chunk
    /// 7.5b).
    pub changes: bool,
    /// The identity keys of the package failures it answers with, whose
    /// codes the provider's error code is matched against.
    pub failures: Vec<String>,
    /// Its recording's text, `recordings/<resource>.<operation>.json` in
    /// its package, which `jadpo dev`'s fake answers from with its
    /// placeholders filled (chunk 7.2d2).
    pub recording: Option<String>,
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
    /// Twilio's `X-Twilio-Signature`: the Base64 of the HMAC-SHA1, under
    /// the account's auth token, of the URL it requested followed by each
    /// form member's name and value, sorted by name (chunk 7.7b). The body
    /// is a form; no time is signed.
    Twilio,
    /// Slack's `X-Slack-Signature: v0=<hex>`: HMAC-SHA-256 of
    /// `v0:<X-Slack-Request-Timestamp>:<body>` under the signing secret
    /// (chunk 7.7b).
    Slack,
    /// AWS SNS: the message's own `Signature`, RSA over its listed members
    /// with SHA-1 (`SignatureVersion` 1) or SHA-256 (2), by the
    /// certificate at its `SigningCertURL` on one of AWS's SNS hosts,
    /// fetched by the runtime; the secret is the topic's ARN, which the
    /// message must come from (chunk 7.7b).
    Sns,
}

impl SignatureScheme {
    /// Every scheme, by its name after `SignatureScheme.`.
    pub const BUILT: [&'static str; 7] = [
        "stripe",
        "standard_webhooks",
        "hmac_sha256",
        "basic",
        "twilio",
        "slack",
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
            "twilio" => SignatureScheme::Twilio,
            "slack" => SignatureScheme::Slack,
            "sns" => SignatureScheme::Sns,
            _ => return None,
        })
    }

    /// Whether its webhooks' bodies are forms rather than JSON: Twilio's.
    pub fn form_body(&self) -> bool {
        matches!(self, SignatureScheme::Twilio)
    }
}

/// How a provider checks an address before it sends to it (area 11,
/// "Handshakes"; chunk 7.7b): built-in rules the runtime answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WebhookHandshake {
    /// AWS SNS's `SubscriptionConfirmation`: signed as every SNS message
    /// is, from the connection's topic, and confirmed by visiting its
    /// `SubscribeURL` on AWS's SNS host.
    Sns,
    /// Slack's `url_verification`: signed, and answered with its
    /// `challenge`.
    Slack,
    /// Microsoft Graph's validation: a `POST` with `validationToken` in
    /// its query, unsigned, answered with the token as plain text.
    MicrosoftGraph,
}

impl WebhookHandshake {
    /// Every handshake, by its name after `WebhookHandshake.`.
    pub const NAMED: [&'static str; 3] = ["sns", "slack", "microsoft_graph"];

    pub fn from_name(name: &str) -> Option<WebhookHandshake> {
        Some(match name {
            "sns" => WebhookHandshake::Sns,
            "slack" => WebhookHandshake::Slack,
            "microsoft_graph" => WebhookHandshake::MicrosoftGraph,
            _ => return None,
        })
    }

    /// The scheme its provider signs with, when it needs one: SNS's
    /// confirmation is signed as its messages are, Slack's challenge as
    /// its events are.
    pub fn scheme(self) -> Option<&'static str> {
        match self {
            WebhookHandshake::Sns => Some("sns"),
            WebhookHandshake::Slack => Some("slack"),
            WebhookHandshake::MicrosoftGraph => None,
        }
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
    /// The check its provider makes of the address first, if any (chunk
    /// 7.7b).
    pub handshake: Option<WebhookHandshake>,
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
    /// The answer it waits for, when its package names a `reply:` (chunk
    /// 7.7b): the event isn't kept in the outbox, its answering action
    /// runs inside the request instead.
    pub reply: Option<WebhookReply>,
}

/// How a webhook that needs a reply is answered (area 11, "Webhooks that
/// need a reply"; chunk 7.7b).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebhookReply {
    /// The identity key of the app's action the connection's `replies:`
    /// names, which takes the event and returns the reply.
    pub action: String,
    /// The reply type's number in the plan's JSON table, which the package
    /// encodes the reply with.
    pub number: u32,
}

/// `address` when a call may be sent to it (area 11, question 9): HTTPS,
/// or plain HTTP to this machine, which is how a test reaches its server;
/// a host and an optional port and path, with no credentials, query or
/// fragment of its own. The runtime holds every provider call to it, and
/// `ai:`'s local driver too (area 21; chunk 5.5v3).
pub fn sendable(address: &str) -> Option<String> {
    let address = address.trim();
    let (secure, rest) = if let Some(rest) = address.strip_prefix("https://") {
        (true, rest)
    } else {
        (false, address.strip_prefix("http://")?)
    };
    let authority = rest.split('/').next().unwrap_or_default();
    if authority.is_empty() || authority.contains('@') || rest.contains(['?', '#']) {
        return None;
    }
    let host = if let Some(inner) = authority.strip_prefix('[') {
        format!("[{}]", inner.split(']').next().unwrap_or_default())
    } else {
        authority
            .rsplit_once(':')
            .map_or(authority, |(host, _)| host)
            .to_string()
    };
    if host.is_empty() {
        return None;
    }
    let host = host.to_ascii_lowercase();
    let loopback = host == "localhost"
        || host == "[::1]"
        || host.starts_with("127.") && host.split('.').count() == 4;
    (secure || loopback).then(|| address.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The names the importer and `package.standard_candidate` read a
    /// field's meaning from, and the parts `wire_parts:` takes.
    #[test]
    fn names_and_parts_say_what_a_field_may_be() {
        assert!(time_named("created") && time_named("paid_at") && time_named("start_time"));
        assert!(!time_named("opened") && !time_named("_at") && !time_named("attempts"));
        assert!(amount_named("amount") && amount_named("cents") && !amount_named("amount_due"));
        assert!(amount_like("amount_due") && amount_like("total_amount") && !amount_like("count"));
        assert!(currency_named("currency_code") && !currency_named("country"));
        assert!(latitude_named("lat") && longitude_named("lng") && !latitude_named("lng"));
        assert_eq!(wire_parts("Money"), Some(vec!["amount", "currency"]));
        assert_eq!(wire_parts("Point"), Some(vec!["latitude", "longitude"]));
        assert_eq!(wire_parts("InstantRange"), Some(vec!["start", "end"]));
        assert_eq!(wire_parts("Email"), None);
    }
}
