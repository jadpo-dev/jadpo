//! The connections table (area 11, "Connecting in `app.jadpo`"; chunk
//! 8.1d): every connection `application { connections: }` declares, once,
//! in name order, each with its driver, the settings that driver is built
//! from and the targets it runs on. A connection's number is its place
//! here: a provider operation's plan names its connection by it, and the
//! credentials a program starts with are kept by it.
//!
//! The parts of the plan that use a connection keep what is their own:
//! `ai:` how it fills `ai` (an [`crate::ai::AiPlan`]), `receiving:` how a
//! queue's messages become events (a [`crate::receiving::QueuePlan`]),
//! `email:` which connection sends the mail; each names its connection,
//! and the table is where its driver is. `jadpo-drivers`' factory builds
//! each connection's driver from it, and `jadpo check` will refuse a
//! deployable whose target a connection's driver doesn't run on (area 11,
//! question 24; roadmap 8.1o).

use serde::{Deserialize, Serialize};

use crate::ai::AiUrl;
use crate::email::SmtpHost;
use crate::provider::{Address, ConnectionPlan};
use crate::receiving::CloudflareQueue;

/// What a deployable runs on (area 14): it decides which drivers it can
/// build, the source of a request's client address an application that
/// declares none takes, and which sources are accepted at all (area 10,
/// "Client addresses").
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Target {
    /// A native process, or `jadpo dev`: the connection's own address
    /// unless the application says otherwise.
    Native,
    /// Cloudflare Workers: there is no connection address, and only
    /// `CF-Connecting-IP` is accepted.
    Workers,
}

impl Target {
    /// How a report names it: `native`, `workers`.
    pub fn name(self) -> &'static str {
        match self {
            Target::Native => "native",
            Target::Workers => "workers",
        }
    }
}

/// Every target, in order.
pub const EVERY_TARGET: [Target; 2] = [Target::Native, Target::Workers];

/// One connection in `connections:`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Connection {
    /// Its name in `connections:`, `couriers`.
    pub name: String,
    /// Its driver and the settings it is built from.
    pub driver: ConnectionDriver,
    /// The targets its driver runs on (area 11, "Targets"), in order.
    pub targets: Vec<Target>,
}

impl Connection {
    /// `name` with `driver`, running on the targets that driver runs on.
    pub fn new(name: impl Into<String>, driver: ConnectionDriver) -> Connection {
        let targets = driver.targets();
        Connection {
            name: name.into(),
            driver,
            targets,
        }
    }

    /// Whether its driver runs on `target`.
    pub fn runs_on(&self, target: Target) -> bool {
        self.targets.contains(&target)
    }
}

/// A connection's driver (area 11, "Capabilities"): a built-in one,
/// `driver: Driver.x`, or the package driver, `package: X.vN`, each with
/// the settings it is built from, settings by their numbers in the plan's
/// settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionDriver {
    /// `Driver.postgres`, a store (area 5): its address, the `url:`
    /// setting numbered here; `None` for one that names none, as the
    /// `main` a project with no database connection is given, which only
    /// development and tests give a database. `tls` is how it is secured
    /// (area 5, "PostgreSQL over TLS"; chunk 8.1n).
    Postgres { url: Option<u32>, tls: DatabaseTls },
    /// `Driver.redis`, a cache (area 16): its `url:` setting. Its driver
    /// comes with caches.
    Redis { url: Option<u32> },
    /// `Driver.disk`, a store of files on the server's own disk (area 16,
    /// "The disk driver"): the folder `path:` names.
    Disk { path: Option<String> },
    /// `Driver.smtp`, the `email` capability's built-in driver (area 16,
    /// "Email"; chunk 7.3).
    Smtp(SmtpServer),
    /// `Driver.openai_compatible { url: }`, which fills `ai` for a server
    /// that speaks OpenAI's API (area 21; chunk 5.5v3): the address its
    /// API is under.
    OpenAiCompatible { url: AiUrl },
    /// `Driver.cloudflare_queues`, a queue whose batches Cloudflare pushes
    /// to the app's consumer on Workers (area 11, "Queues"; chunk 7.7c2).
    CloudflareQueues(CloudflareQueue),
    /// A provider package (area 11, "Calling"): the package driver, JSON
    /// over HTTP, and how each call through it is made.
    Package(Box<ConnectionPlan>),
}

impl ConnectionDriver {
    /// The targets it runs on (area 11, "Targets"): a package that calls
    /// an HTTP API, and every network driver, everywhere; the disk and a
    /// service Jadpo runs beside the deployable (`runs:`) only natively;
    /// Cloudflare's queues only on Workers.
    pub fn targets(&self) -> Vec<Target> {
        match self {
            ConnectionDriver::Disk { .. } => vec![Target::Native],
            ConnectionDriver::CloudflareQueues(_) => vec![Target::Workers],
            ConnectionDriver::Package(plan) if plan.address == Address::Unknown => {
                vec![Target::Native]
            }
            ConnectionDriver::Postgres { .. }
            | ConnectionDriver::Redis { .. }
            | ConnectionDriver::Smtp(_)
            | ConnectionDriver::OpenAiCompatible { .. }
            | ConnectionDriver::Package(_) => EVERY_TARGET.to_vec(),
        }
    }

    /// How `app.jadpo` names it: `Driver.smtp`, or `package` for the
    /// package driver.
    pub fn name(&self) -> &'static str {
        match self {
            ConnectionDriver::Postgres { .. } => "Driver.postgres",
            ConnectionDriver::Redis { .. } => "Driver.redis",
            ConnectionDriver::Disk { .. } => "Driver.disk",
            ConnectionDriver::Smtp(_) => "Driver.smtp",
            ConnectionDriver::OpenAiCompatible { .. } => "Driver.openai_compatible",
            ConnectionDriver::CloudflareQueues(_) => "Driver.cloudflare_queues",
            ConnectionDriver::Package(_) => "package",
        }
    }

    /// The settings it is built from, by number: what a host reads before
    /// it builds the driver.
    pub fn settings(&self) -> Vec<u32> {
        match self {
            ConnectionDriver::Postgres { url, tls } => {
                let authority = match tls {
                    DatabaseTls::Authority(at) => Some(*at),
                    DatabaseTls::Roots | DatabaseTls::Off => None,
                };
                [*url, authority].into_iter().flatten().collect()
            }
            ConnectionDriver::Redis { url } => url.iter().copied().collect(),
            ConnectionDriver::Smtp(server) => {
                let host = match server.host {
                    SmtpHost::Setting(at) => Some(at),
                    SmtpHost::Written(_) => None,
                };
                [host, server.username, server.password]
                    .into_iter()
                    .flatten()
                    .collect()
            }
            ConnectionDriver::OpenAiCompatible { url } => match url {
                AiUrl::Setting(at) => vec![*at],
                AiUrl::Written(_) => Vec::new(),
            },
            ConnectionDriver::Disk { .. } | ConnectionDriver::CloudflareQueues(_) => Vec::new(),
            ConnectionDriver::Package(plan) => {
                let address = match plan.address {
                    Address::Hosts { environment, .. } => Some(environment),
                    Address::Setting(at) => Some(at),
                    Address::Unknown => None,
                };
                let credential = plan.credential.map(|(_, at)| at);
                let key_id = match plan.credential {
                    Some((crate::provider::CredentialScheme::AwsSigV4 { key_id }, _)) => {
                        Some(key_id)
                    }
                    _ => None,
                };
                [address, credential, key_id]
                    .into_iter()
                    .flatten()
                    .collect()
            }
        }
    }
}

/// How a connection with `driver: Driver.postgres` is secured (area 5,
/// "PostgreSQL over TLS"; chunk 8.1n).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DatabaseTls {
    /// TLS, the server's certificate checked against the host's roots
    /// (Mozilla's, compiled in), but to this machine, which is reached
    /// without it: what a connection gets unless it says otherwise.
    #[default]
    Roots,
    /// TLS everywhere but a Unix socket, the server's certificate checked
    /// against only the authority in the `Certificate` setting (area 19)
    /// numbered here: `authority: config.<name>`.
    Authority(u32),
    /// No TLS anywhere: `tls: false`, which the network report lists.
    Off,
}

/// The server a connection with `driver: Driver.smtp` reaches (area 11's
/// settings, chosen in chunk 7.3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmtpServer {
    /// The server's name: `host:`.
    pub host: SmtpHost,
    /// `port:`, [`crate::email::SMTP_PORT`] when it gives none.
    pub port: u16,
    /// `credential: { username: password: }`, by their settings' numbers.
    pub username: Option<u32>,
    pub password: Option<u32>,
}

/// The connection called `name` in `table`, with its number.
pub fn named<'a>(table: &'a [Connection], name: &str) -> Option<(u32, &'a Connection)> {
    let at = table
        .iter()
        .position(|connection| connection.name == name)?;
    Some((u32::try_from(at).ok()?, table.get(at)?))
}

/// How a call through the provider connection numbered `at` is made:
/// `None` when it isn't a package's.
pub fn package(table: &[Connection], at: u32) -> Option<&ConnectionPlan> {
    match &table.get(at as usize)?.driver {
        ConnectionDriver::Package(plan) => Some(plan),
        _ => None,
    }
}

/// The provider package connections in `table`, each with its number.
pub fn packages(table: &[Connection]) -> impl Iterator<Item = (u32, &str, &ConnectionPlan)> {
    table
        .iter()
        .enumerate()
        .filter_map(|(at, connection)| match &connection.driver {
            ConnectionDriver::Package(plan) => Some((
                u32::try_from(at).ok()?,
                connection.name.as_str(),
                plan.as_ref(),
            )),
            _ => None,
        })
}

/// The stores of files in `table` (area 16): every `Driver.disk`
/// connection, with the folder its `path:` names.
pub fn files(table: &[Connection]) -> impl Iterator<Item = (&str, Option<&str>)> {
    table
        .iter()
        .filter_map(|connection| match &connection.driver {
            ConnectionDriver::Disk { path } => Some((connection.name.as_str(), path.as_deref())),
            _ => None,
        })
}

/// The `Driver.smtp` connections in `table`, with their servers.
pub fn smtp(table: &[Connection]) -> impl Iterator<Item = (&str, &SmtpServer)> {
    table
        .iter()
        .filter_map(|connection| match &connection.driver {
            ConnectionDriver::Smtp(server) => Some((connection.name.as_str(), server)),
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(address: Address) -> ConnectionDriver {
        ConnectionDriver::Package(Box::new(ConnectionPlan {
            package: "AcmeCouriers.v1".into(),
            address,
            credential: None,
            credential_header: None,
            headers: Vec::new(),
            protocol: crate::provider::Protocol::JsonHttp,
            aws: None,
            timeout_ms: 10_000,
            retry: crate::provider::RetryPlan::default(),
            idempotency: None,
            error_code: None,
            sandbox: Vec::new(),
            max_concurrent: None,
        }))
    }

    /// Each driver says where it runs (area 11, "Targets"): the disk and
    /// a service Jadpo runs only natively, Cloudflare's queues only on
    /// Workers, everything over the network everywhere.
    #[test]
    fn each_driver_lists_the_targets_it_runs_on() {
        let disk = Connection::new("uploads", ConnectionDriver::Disk { path: None });
        assert_eq!(disk.targets, [Target::Native]);
        let queue = Connection::new(
            "warehouse",
            ConnectionDriver::CloudflareQueues(CloudflareQueue {
                queue: "warehouse-events".into(),
                dead_letter_queue: "warehouse-events-dead".into(),
            }),
        );
        assert!(queue.runs_on(Target::Workers) && !queue.runs_on(Target::Native));
        let api = Connection::new("couriers", package(Address::Setting(0)));
        assert_eq!(api.targets, EVERY_TARGET);
        let service = Connection::new("pdfs", package(Address::Unknown));
        assert_eq!(service.targets, [Target::Native]);
        let mail = Connection::new(
            "mail",
            ConnectionDriver::Smtp(SmtpServer {
                host: SmtpHost::Setting(2),
                port: 587,
                username: Some(3),
                password: Some(4),
            }),
        );
        assert_eq!(mail.targets, EVERY_TARGET);
        assert_eq!(mail.driver.settings(), [2, 3, 4]);
    }

    #[test]
    fn a_connection_is_found_by_its_name_and_its_number() {
        let table = vec![
            Connection::new("couriers", package(Address::Setting(0))),
            Connection::new(
                "main",
                ConnectionDriver::Postgres {
                    url: Some(1),
                    tls: DatabaseTls::Authority(4),
                },
            ),
            Connection::new("uploads", ConnectionDriver::Disk { path: None }),
        ];
        assert_eq!(named(&table, "main").map(|(at, _)| at), Some(1));
        // A database is built from its address and the authority it
        // checks its server against (chunk 8.1n).
        assert_eq!(table[1].driver.settings(), [1, 4]);
        assert!(super::package(&table, 0).is_some() && super::package(&table, 1).is_none());
        assert_eq!(packages(&table).map(|(at, ..)| at).collect::<Vec<_>>(), [0]);
        assert_eq!(files(&table).collect::<Vec<_>>(), [("uploads", None)]);
    }
}
