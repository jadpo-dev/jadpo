//! What a program started with (chunk 7.9e): the values its settings were
//! given, decoded once as it starts (area 10, "Checking values before the
//! app starts"), and what is read from them and held apart from the values
//! code reads: the key settings' keys (area 19), each connection's
//! credential and each webhook's secret (area 11), what signing in reads
//! (area 8), how a request's client address is chosen on the target it
//! runs on (area 10, "Client addresses"), and how long each purge waits
//! (area 5, "Lifecycle").
//!
//! [`Started::new`] reads it from the plan ([`crate::Plan`]) and
//! the values; the context reaches it ([`crate::Ctx::started`]), so a
//! setting is read one way wherever it is read: a test's own value (its
//! `config:`, area 13), else the one the program started with
//! ([`crate::Ctx::setting`]).

use std::fmt;
use std::rc::Rc;

use jadpo_value::Value;

use crate::Plan;
use crate::config::ConfigValues;
use crate::effects::Keys;
use crate::http::address::{ClientAddress, Target};
use crate::http::signed::AuthSettings;
use crate::provider::Credentials;

/// A program's start-up state: see the module.
#[derive(Default)]
pub struct Started {
    /// Each setting's value, decoded, by its number in the plan: `None`
    /// for one no value reached, and for a secret, which code never reads.
    settings: Vec<Option<Value>>,
    keys: Keys,
    credentials: Credentials,
    /// The settings each connection's driver is built from, as given
    /// (chunk 8.1d): what a host's driver factory reads.
    drivers: DriverSettings,
    auth: AuthSettings,
    client: ClientAddress,
    /// How long each of the plan's purges waits, in milliseconds, in their
    /// order: its `after:`, or the value of the setting it reads; `None`
    /// when that holds no duration.
    purge_after: Vec<Option<i64>>,
}

impl fmt::Debug for Started {
    /// How many settings have values; never a value, a key or a secret.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Started")
            .field(
                "settings",
                &self.settings.iter().filter(|value| value.is_some()).count(),
            )
            .field("credentials", &self.credentials)
            .field("auth", &self.auth)
            .field("client", &self.client)
            .finish_non_exhaustive()
    }
}

impl Started {
    /// What a program of `plan` starts with on `target`, its settings
    /// given `values`: what the application started with, or a test run's.
    /// With no values, no setting has one, and each purge waits only as
    /// long as its own `after:` says.
    pub fn new(plan: &Plan<'_>, values: Option<&ConfigValues>, target: Target) -> Started {
        let validators = plan.json.codecs.validators;
        let purge_after = plan
            .purges
            .iter()
            .map(|purge| match (purge.after_setting, plan.config, values) {
                (None, _, _) => Some(purge.after),
                (Some(at), Some(config), Some(values)) => {
                    crate::config::duration_ms(config, validators, values, at)
                }
                (Some(_), _, _) => None,
            })
            .collect();
        let Some(values) = values else {
            return Started {
                purge_after,
                ..Started::default()
            };
        };
        let settings = plan
            .config
            .map(|config| {
                config
                    .fields
                    .iter()
                    .enumerate()
                    .map(|(at, field)| {
                        // Code never reads a secret, so none is held here.
                        if field.secret {
                            return None;
                        }
                        let text = values.text_at(u32::try_from(at).ok()?)?;
                        crate::config::decode(config, validators, field, text).ok()
                    })
                    .collect()
            })
            .unwrap_or_default();
        let routes = plan.routes;
        Started {
            settings,
            keys: plan
                .config
                .map(|config| Keys::read(config, values))
                .unwrap_or_default(),
            credentials: Credentials::read(plan.connections, plan.providers, values),
            drivers: DriverSettings::read(plan.connections, values),
            auth: AuthSettings::read(routes.and_then(|routes| routes.identity.as_ref()), values),
            // A source the target doesn't accept is the deployable's
            // problem to report as it starts (chunk 8.2d); until then the
            // connection's own address, never a guess.
            client: ClientAddress::read(
                routes.and_then(|routes| routes.client_address.as_ref()),
                values,
                target,
            )
            .unwrap_or_default(),
            purge_after,
        }
    }

    /// The value the program started with for setting number `at`: its
    /// variable's, its `test:` or its `default:` (area 10). `None` for one
    /// no value reached, and for a secret.
    pub fn setting(&self, at: u32) -> Option<&Value> {
        self.settings.get(at as usize)?.as_ref()
    }

    /// The key settings' keys: what a `crypto` call's `key: config.<name>`
    /// gives it, and what a record of an action's steps is sealed with.
    pub fn keys(&self) -> &Keys {
        &self.keys
    }

    /// Each provider connection's credential, and each webhook's secret.
    pub fn credentials(&self) -> &Credentials {
        &self.credentials
    }

    /// The value setting number `at` was given as text, when a
    /// connection's driver is built from it (chunk 8.1d): a database's
    /// address, an SMTP server's name and sign-in. Secrets included, which
    /// only a driver reads; `None` for a setting no driver reads.
    pub fn driver_setting(&self, at: u32) -> Option<&str> {
        self.drivers.0.get(&at).map(String::as_str)
    }

    /// What signing in reads: the signing keys and the browser session's
    /// origin.
    pub fn auth(&self) -> &AuthSettings {
        &self.auth
    }

    /// How a request's client address is chosen.
    pub fn client(&self) -> &ClientAddress {
        &self.client
    }

    /// How long the purge at `at` in the plan's purges waits, in
    /// milliseconds; `None` when the setting it reads holds no duration.
    pub fn purge_after(&self, at: usize) -> Option<i64> {
        self.purge_after.get(at).copied().flatten()
    }

    /// The same, with `client` as how a request's client address is
    /// chosen: for a test of the HTTP component.
    #[cfg(test)]
    pub(crate) fn with_client(mut self, client: ClientAddress) -> Started {
        self.client = client;
        self
    }
}

/// The settings the connections' drivers are built from, as given, by
/// number: never printed, since a database's address and an SMTP
/// password are among them.
#[derive(Default)]
struct DriverSettings(std::collections::BTreeMap<u32, String>);

impl DriverSettings {
    /// What `values` give the settings each connection in `table` but a
    /// package's is built from: a package's are read as each call is made.
    fn read(table: &[jadpo_std::connections::Connection], values: &ConfigValues) -> DriverSettings {
        DriverSettings(
            table
                .iter()
                .filter(|connection| {
                    !matches!(
                        connection.driver,
                        jadpo_std::connections::ConnectionDriver::Package(_)
                    )
                })
                .flat_map(|connection| connection.driver.settings())
                .filter_map(|at| Some((at, values.text_at(at)?.to_string())))
                .collect(),
        )
    }
}

thread_local! {
    /// What a context made from a program's places alone reaches until the
    /// program runs in it: nothing started.
    static UNSTARTED: Rc<Started> = Rc::new(Started::default());
}

/// A program that started with nothing.
pub(crate) fn unstarted() -> Rc<Started> {
    UNSTARTED.with(Rc::clone)
}
