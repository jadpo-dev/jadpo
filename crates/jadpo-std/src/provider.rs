//! Provider calls as plain data (area 11, "Calling" and "Decoding"; chunk
//! 7.2c): each connection to a provider package, how its address and
//! credential are read and how long a call waits, and each operation an
//! application calls through it, how it is sent and how its answer is
//! read. Both executors send a call from this through the capability seam
//! (`jadpo_runtime::provider`).

use crate::HttpMethod;
use crate::subscriber::Repeat;

/// What a call waits for its answer when its connection gives no
/// `timeout:` (area 11, question 8).
pub const DEFAULT_TIMEOUT_MS: u64 = 10_000;

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
