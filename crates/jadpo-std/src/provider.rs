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
