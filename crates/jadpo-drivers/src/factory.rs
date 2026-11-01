//! The driver factory (chunk 8.1d): one place that builds each
//! connection's driver, from the plan's connections table
//! ([`jadpo_std::connections`]) and what the program started with
//! ([`Started`]: the settings each driver is built from, secrets
//! included), over what the host supplies ([`Supplies`]): the transport
//! sockets and TLS come through, HTTP, and a disk.
//!
//! - `Driver.postgres`: [`crate::postgres::Postgres`] at its `url:`'s
//!   address, over the transport; nothing connects until a transaction
//!   begins.
//! - A package and `Driver.openai_compatible`: one package driver
//!   ([`crate::package::JsonHttp`]) over the host's HTTP, shared by every
//!   such connection, since the runtime makes each request from the plan.
//! - `Driver.smtp`: [`crate::smtp::Smtp`] for its server, over the
//!   transport.
//! - `Driver.disk`: what the host's disk gives for its `path:`.
//! - `Driver.cloudflare_queues`: nothing to build, since Cloudflare pushes
//!   its batches to the Workers host's consumer, which calls no driver.
//! - `Driver.cloudflare_rate_limits`: nothing to build, since the Workers
//!   host counts rate limits in the Worker's bindings itself (chunk 8.2m2).
//!
//! A connection whose driver doesn't run on the host's target, or that the
//! host or the program's settings can't build, is kept as [`Unbuilt`] with
//! why, for whoever starts the program to report; asking for its driver
//! then finds none, which the runtime answers `misconfigured`. `jadpo dev`
//! and `jadpo test` put their fakes in front of what it builds
//! (`capability::Faked`), so nothing they reach leaves the machine.

use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;

use jadpo_runtime::Started;
use jadpo_runtime::capability::Capability;
use jadpo_std::connections::{Connection, ConnectionDriver, Target};

use crate::Transport;

/// A host's disk, which keeps a store of files (area 16, "The disk
/// driver"): natively the server's own, under each connection's `path:`
/// or `jadpo dev`'s folder.
pub trait Disk {
    /// The driver of store of files `connection`, kept under `path` with
    /// `floor` millionths of the disk kept free; why not, never a secret,
    /// when this disk can't keep it there.
    fn store(
        &self,
        connection: &str,
        path: Option<&str>,
        floor: u32,
    ) -> Result<Rc<dyn Capability>, String>;
}

/// What a host supplies the drivers it builds: the target it runs on, the
/// transport sockets and TLS come through, HTTP for the package driver,
/// and a disk. A part it leaves out leaves every driver over it unbuilt.
#[derive(Clone)]
pub struct Supplies {
    pub target: Target,
    pub transport: Option<Rc<dyn Transport>>,
    #[cfg(feature = "package")]
    pub fetch: Option<Rc<dyn crate::package::Fetch>>,
    pub disk: Option<Rc<dyn Disk>>,
}

impl fmt::Debug for Supplies {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = f.debug_struct("Supplies");
        out.field("target", &self.target)
            .field("transport", &self.transport.is_some());
        #[cfg(feature = "package")]
        out.field("fetch", &self.fetch.is_some());
        out.field("disk", &self.disk.is_some()).finish()
    }
}

impl Supplies {
    /// Nothing on `target`: every driver unbuilt until a part is given.
    pub fn on(target: Target) -> Supplies {
        Supplies {
            target,
            transport: None,
            #[cfg(feature = "package")]
            fetch: None,
            disk: None,
        }
    }

    pub fn with_transport(mut self, transport: Rc<dyn Transport>) -> Supplies {
        self.transport = Some(transport);
        self
    }

    #[cfg(feature = "package")]
    pub fn with_fetch(mut self, fetch: Rc<dyn crate::package::Fetch>) -> Supplies {
        self.fetch = Some(fetch);
        self
    }

    pub fn with_disk(mut self, disk: Rc<dyn Disk>) -> Supplies {
        self.disk = Some(disk);
        self
    }
}

/// Why a connection's driver wasn't built.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Why {
    /// Its driver doesn't run on the host's target (area 11, "Targets").
    NotOnTarget(Target),
    /// The host supplies nothing it is built over: a transport, HTTP or a
    /// disk.
    NotSupplied(&'static str),
    /// This build of the runtime carries no such driver.
    NotInThisBuild,
    /// A setting it is built from has no value: the setting's number, or
    /// `None` for a connection that names none.
    Unset(Option<u32>),
    /// Its settings don't make a driver: why not, never their values.
    Invalid(String),
}

/// A connection whose driver wasn't built, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unbuilt {
    pub connection: String,
    pub driver: &'static str,
    pub why: Why,
}

impl fmt::Display for Unbuilt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Unbuilt {
            connection, driver, ..
        } = self;
        match &self.why {
            Why::NotOnTarget(target) => write!(
                f,
                "`{connection}`'s {driver} doesn't run on {}",
                target.name()
            ),
            Why::NotSupplied(what) => write!(
                f,
                "`{connection}`'s {driver} needs {what}, which this host doesn't give"
            ),
            Why::NotInThisBuild => {
                write!(f, "`{connection}`'s {driver} isn't in this build of Jadpo")
            }
            Why::Unset(_) => write!(
                f,
                "`{connection}`'s {driver} reads a setting that has no value"
            ),
            Why::Invalid(why) => write!(f, "`{connection}`'s {driver} can't be built: {why}"),
        }
    }
}

/// As a deployable reports it, on either host (chunk 8.4c).
impl jadpo_runtime::deployable::NotBuilt for Unbuilt {
    fn connection(&self) -> &str {
        &self.connection
    }

    fn driver(&self) -> &str {
        self.driver
    }

    fn unaddressed(&self) -> bool {
        matches!(self.why, Why::Unset(None))
    }
}

/// Every connection's driver a host built ([`Drivers::build`]), by the
/// connection's name, and those it couldn't, with why.
#[derive(Default)]
pub struct Drivers {
    #[cfg(feature = "postgres")]
    databases: BTreeMap<String, crate::postgres::Postgres>,
    capabilities: BTreeMap<String, Rc<dyn Capability>>,
    unbuilt: Vec<Unbuilt>,
}

impl fmt::Debug for Drivers {
    /// The connections' names, never a setting.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = f.debug_struct("Drivers");
        #[cfg(feature = "postgres")]
        out.field("databases", &self.databases.keys().collect::<Vec<_>>());
        out.field(
            "capabilities",
            &self.capabilities.keys().collect::<Vec<_>>(),
        )
        .field("unbuilt", &self.unbuilt)
        .finish()
    }
}

/// What building one connection's driver gave.
enum Built {
    #[cfg(feature = "postgres")]
    Database(crate::postgres::Postgres),
    Capability(Rc<dyn Capability>),
    /// A driver nothing calls: a queue Cloudflare pushes.
    Nothing,
}

impl Drivers {
    /// Each driver of `connections`, the plan's connections table, from
    /// the settings `started` holds, over what `supplies` gives.
    pub fn build(connections: &[Connection], started: &Started, supplies: &Supplies) -> Drivers {
        let mut out = Drivers::default();
        // One package driver for every connection the runtime makes HTTP
        // requests through.
        #[cfg(feature = "package")]
        let http: Option<Rc<dyn Capability>> = supplies.fetch.as_ref().map(|fetch| {
            Rc::new(crate::package::JsonHttp::new(Rc::clone(fetch))) as Rc<dyn Capability>
        });
        #[cfg(not(feature = "package"))]
        let http: Option<Rc<dyn Capability>> = None;
        for connection in connections {
            let built = if connection.runs_on(supplies.target) {
                build_one(connection, started, supplies, http.as_ref())
            } else {
                Err(Why::NotOnTarget(supplies.target))
            };
            match built {
                #[cfg(feature = "postgres")]
                Ok(Built::Database(database)) => {
                    out.databases.insert(connection.name.clone(), database);
                }
                Ok(Built::Capability(driver)) => {
                    out.capabilities.insert(connection.name.clone(), driver);
                }
                Ok(Built::Nothing) => {}
                Err(why) => out.unbuilt.push(Unbuilt {
                    connection: connection.name.clone(),
                    driver: connection.driver.name(),
                    why,
                }),
            }
        }
        out
    }

    /// The driver of capability connection `connection`.
    pub fn capability(&self, connection: &str) -> Option<&dyn Capability> {
        self.capabilities.get(connection).map(Rc::as_ref)
    }

    /// The store `connection`, a PostgreSQL database.
    #[cfg(feature = "postgres")]
    pub fn database(&self, connection: &str) -> Option<&crate::postgres::Postgres> {
        self.databases.get(connection)
    }

    /// The connections whose drivers weren't built, and why.
    pub fn unbuilt(&self) -> &[Unbuilt] {
        &self.unbuilt
    }

    /// Only the drivers of the connections `keep` names: what a tool that
    /// calls through one connection gives its host.
    pub fn keeping(mut self, keep: impl Fn(&str) -> bool) -> Drivers {
        #[cfg(feature = "postgres")]
        self.databases.retain(|name, _| keep(name));
        self.capabilities.retain(|name, _| keep(name));
        self.unbuilt.retain(|unbuilt| keep(&unbuilt.connection));
        self
    }
}

/// `connection`'s driver, `http` the package driver every HTTP connection
/// shares.
fn build_one(
    connection: &Connection,
    started: &Started,
    supplies: &Supplies,
    http: Option<&Rc<dyn Capability>>,
) -> Result<Built, Why> {
    let text = |at: u32| started.driver_setting(at).ok_or(Why::Unset(Some(at)));
    match &connection.driver {
        ConnectionDriver::Postgres { url, tls } => {
            let url = text(url.ok_or(Why::Unset(None))?)?;
            let authority = match tls {
                jadpo_std::connections::DatabaseTls::Authority(at) => Some(text(*at)?),
                _ => None,
            };
            postgres(url, *tls, authority, supplies)
        }
        ConnectionDriver::Redis { .. } => Err(Why::NotInThisBuild),
        ConnectionDriver::Disk { path, floor } => {
            let disk = supplies.disk.as_ref().ok_or(Why::NotSupplied("a disk"))?;
            disk.store(&connection.name, path.as_deref(), *floor)
                .map(Built::Capability)
                .map_err(Why::Invalid)
        }
        ConnectionDriver::Smtp(server) => {
            let host = match &server.host {
                jadpo_std::email::SmtpHost::Setting(at) => text(*at)?.to_string(),
                jadpo_std::email::SmtpHost::Written(host) => host.clone(),
            };
            let username = server.username.map(text).transpose()?;
            let password = server.password.map(text).transpose()?;
            smtp(host, server.port, username.zip(password), supplies)
        }
        ConnectionDriver::OpenAiCompatible { .. } | ConnectionDriver::Package(_) => http
            .map(|http| Built::Capability(Rc::clone(http)))
            .ok_or(if cfg!(feature = "package") {
                Why::NotSupplied("HTTP")
            } else {
                Why::NotInThisBuild
            }),
        ConnectionDriver::CloudflareQueues(_) | ConnectionDriver::CloudflareRateLimits { .. } => {
            Ok(Built::Nothing)
        }
    }
}

/// A PostgreSQL store at `url`, secured as its connection says (area 5,
/// "PostgreSQL over TLS"; chunk 8.1n): `authority` is the text of the
/// `Certificate` setting `tls` names.
#[cfg(feature = "postgres")]
fn postgres(
    url: &str,
    tls: jadpo_std::connections::DatabaseTls,
    authority: Option<&str>,
    supplies: &Supplies,
) -> Result<Built, Why> {
    use crate::postgres::Security;
    use jadpo_std::connections::DatabaseTls;
    let transport = supplies
        .transport
        .as_ref()
        .ok_or(Why::NotSupplied("a transport"))?;
    let security = match (tls, authority) {
        (DatabaseTls::Roots, _) => Security::Roots,
        (DatabaseTls::Off, _) => Security::Plain,
        (DatabaseTls::Authority(_), authority) => Security::Authorities(
            jadpo_std::database_tls::certificates(authority.unwrap_or_default())
                .map_err(|why| Why::Invalid(format!("its `authority:` {why}")))?,
        ),
    };
    crate::postgres::Postgres::secured(url, Rc::clone(transport), security)
        .map(Built::Database)
        // The driver's message names what is wrong, never the address.
        .map_err(|error| {
            Why::Invalid(match error {
                jadpo_runtime::StoreError::Fault(why) => why,
                _ => "its `url:` isn't a PostgreSQL address".into(),
            })
        })
}

#[cfg(not(feature = "postgres"))]
fn postgres(
    _: &str,
    _: jadpo_std::connections::DatabaseTls,
    _: Option<&str>,
    _: &Supplies,
) -> Result<Built, Why> {
    Err(Why::NotInThisBuild)
}

#[cfg(feature = "smtp")]
fn smtp(
    host: String,
    port: u16,
    signing_in: Option<(&str, &str)>,
    supplies: &Supplies,
) -> Result<Built, Why> {
    let transport = supplies
        .transport
        .as_ref()
        .ok_or(Why::NotSupplied("a transport"))?;
    if host.trim().is_empty() {
        return Err(Why::Invalid("its `host:` is empty".into()));
    }
    let server = crate::smtp::Server::new(host, port);
    let server = match signing_in {
        Some((username, password)) => server.signing_in(username, password),
        None => server,
    };
    Ok(Built::Capability(Rc::new(crate::smtp::Smtp::new(
        Rc::clone(transport),
        server,
    ))))
}

#[cfg(not(feature = "smtp"))]
fn smtp(_: String, _: u16, _: Option<(&str, &str)>, _: &Supplies) -> Result<Built, Why> {
    Err(Why::NotInThisBuild)
}

#[cfg(all(test, feature = "postgres", feature = "package", feature = "smtp"))]
mod tests {
    use std::cell::RefCell;
    use std::future::Future;

    use jadpo_runtime::LocalFuture;
    use jadpo_runtime::capability::{Answer, Call};
    use jadpo_std::connections::{DatabaseTls, SmtpServer};
    use jadpo_std::email::SmtpHost;
    use jadpo_std::receiving::CloudflareQueue;
    use jadpo_value::Value;

    use super::*;
    use crate::{Endpoint, Stream};

    /// A host where nothing listens, keeping where each driver tried to
    /// connect: nothing leaves the test.
    #[derive(Default)]
    struct Nowhere(RefCell<Vec<Endpoint>>);

    impl Transport for Nowhere {
        fn connect(
            &self,
            endpoint: &Endpoint,
        ) -> LocalFuture<'_, std::io::Result<Box<dyn Stream>>> {
            self.0.borrow_mut().push(endpoint.clone());
            Box::pin(async { Err(std::io::Error::from(std::io::ErrorKind::ConnectionRefused)) })
        }

        fn spawn(&self, _: LocalFuture<'static, ()>) {}

        fn sleep(&self, _: u64) -> LocalFuture<'_, ()> {
            Box::pin(std::future::pending())
        }
    }

    /// HTTP that never sends.
    struct NoHttp;

    impl crate::package::Fetch for NoHttp {
        fn fetch<'a>(
            &'a self,
            _: &'a jadpo_runtime::provider::Request,
        ) -> LocalFuture<'a, Result<jadpo_runtime::provider::Response, crate::package::FetchError>>
        {
            Box::pin(async { Err(crate::package::FetchError::NotSent("no HTTP".into())) })
        }
    }

    /// Hands every store of files to one driver.
    struct OneDisk;

    impl Disk for OneDisk {
        fn store(&self, _: &str, _: Option<&str>, _: u32) -> Result<Rc<dyn Capability>, String> {
            Ok(Rc::new(crate::package::JsonHttp::new(NoHttp)))
        }
    }

    fn table() -> Vec<Connection> {
        vec![
            Connection::new(
                "assets",
                ConnectionDriver::Disk {
                    path: None,
                    floor: jadpo_std::files::DEFAULT_FREE_SPACE_FLOOR,
                },
            ),
            Connection::new(
                "local_model",
                ConnectionDriver::OpenAiCompatible {
                    url: jadpo_std::ai::AiUrl::Written("http://localhost:11434/v1".into()),
                },
            ),
            Connection::new(
                "mail",
                ConnectionDriver::Smtp(SmtpServer {
                    host: SmtpHost::Written("mail.example.test".into()),
                    port: 2525,
                    username: None,
                    password: None,
                }),
            ),
            Connection::new(
                "main",
                ConnectionDriver::Postgres {
                    url: None,
                    tls: DatabaseTls::Roots,
                },
            ),
            Connection::new(
                "reports",
                ConnectionDriver::Postgres {
                    url: Some(3),
                    tls: DatabaseTls::Authority(5),
                },
            ),
            Connection::new(
                "warehouse",
                ConnectionDriver::CloudflareQueues(CloudflareQueue {
                    queue: "warehouse-events".into(),
                    dead_letter_queue: "warehouse-events-dead".into(),
                }),
            ),
        ]
    }

    fn now<F: Future>(future: F) -> F::Output {
        let mut future = std::pin::pin!(future);
        match future
            .as_mut()
            .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
        {
            std::task::Poll::Ready(answer) => answer,
            std::task::Poll::Pending => panic!("answered at once"),
        }
    }

    fn message() -> Value {
        Value::record(
            "EmailMessage",
            vec![
                (Rc::from("from"), Value::text("orders@grocer.test")),
                (Rc::from("to"), Value::text("ana@example.test")),
                (Rc::from("subject"), Value::text("Your tide table for May")),
                (Rc::from("html"), Value::None),
                (Rc::from("text"), Value::text("Hi")),
                (Rc::from("attachments"), Value::List(Vec::new().into())),
            ],
        )
    }

    /// The SMTP driver is built for the server its connection names and
    /// reaches it over the host's transport; a store with no address and
    /// one whose setting has no value are kept unbuilt, as is a queue on a
    /// target it doesn't run on, while nothing is built for a disk the
    /// host doesn't give.
    #[test]
    fn each_connection_gets_its_driver_or_why_not() {
        let nowhere = Rc::new(Nowhere::default());
        let supplies = Supplies::on(Target::Native)
            .with_transport(Rc::clone(&nowhere) as Rc<dyn Transport>)
            .with_fetch(Rc::new(NoHttp));
        let drivers = Drivers::build(&table(), &Started::default(), &supplies);
        let mail = drivers.capability("mail").expect("the SMTP driver");
        let call = Call::new(
            "mail",
            jadpo_std::email::SEND,
            message(),
            jadpo_std::subscriber::Repeat::Unkeyed,
        );
        assert_eq!(now(mail.call(&call)), Answer::Unavailable);
        assert_eq!(
            nowhere.0.borrow().as_slice(),
            [Endpoint::Tcp {
                host: "mail.example.test".into(),
                port: 2525
            }]
        );
        assert!(drivers.capability("local_model").is_some());
        assert!(drivers.capability("warehouse").is_none());
        let why: Vec<(&str, &Why)> = drivers
            .unbuilt()
            .iter()
            .map(|unbuilt| (unbuilt.connection.as_str(), &unbuilt.why))
            .collect();
        assert_eq!(
            why,
            [
                ("assets", &Why::NotSupplied("a disk")),
                ("main", &Why::Unset(None)),
                ("reports", &Why::Unset(Some(3))),
                ("warehouse", &Why::NotOnTarget(Target::Native)),
            ]
        );
        assert_eq!(
            drivers.unbuilt()[3].to_string(),
            "`warehouse`'s Driver.cloudflare_queues doesn't run on native"
        );
    }

    /// On Workers the disk doesn't run and the queue needs no driver; a
    /// host that gives no transport or HTTP builds nothing over them.
    #[test]
    fn what_a_host_doesnt_give_leaves_its_drivers_unbuilt() {
        let drivers = Drivers::build(
            &table(),
            &Started::default(),
            &Supplies::on(Target::Workers).with_disk(Rc::new(OneDisk)),
        );
        let why: Vec<(&str, &Why)> = drivers
            .unbuilt()
            .iter()
            .map(|unbuilt| (unbuilt.connection.as_str(), &unbuilt.why))
            .collect();
        assert_eq!(
            why,
            [
                ("assets", &Why::NotOnTarget(Target::Workers)),
                ("local_model", &Why::NotSupplied("HTTP")),
                ("mail", &Why::NotSupplied("a transport")),
                ("main", &Why::Unset(None)),
                ("reports", &Why::Unset(Some(3))),
            ]
        );
        let native = Drivers::build(
            &table(),
            &Started::default(),
            &Supplies::on(Target::Native).with_disk(Rc::new(OneDisk)),
        );
        assert!(native.capability("assets").is_some());
        let kept = native.keeping(|name| name == "assets");
        assert!(kept.unbuilt().is_empty() && kept.capability("assets").is_some());
    }
}
