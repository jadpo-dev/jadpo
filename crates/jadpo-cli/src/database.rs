//! A database the commands that reach one open (`jadpo migrate`, `jadpo
//! schedules`, `jadpo runs`, `jadpo deliveries`, `jadpo ai usage`): its
//! address and how its connection in `app.jadpo` says it is secured (area
//! 5, "PostgreSQL over TLS"; chunk 8.1n), so a command reaches a database
//! as the application does.

use std::ops::Deref;

use jadpo_check::databases::DatabaseTls;
use jadpo_db::Loaded;
use jadpo_host_native::{Postgres, Security};
use jadpo_runtime::StoreError;

use crate::run::{Call, Reply};

/// A database's address and how it is secured.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Address {
    pub(crate) url: String,
    pub(crate) security: Security,
}

impl Deref for Address {
    type Target = str;

    fn deref(&self) -> &str {
        &self.url
    }
}

impl Address {
    /// `url`, secured by default: TLS against Mozilla's roots, but to this
    /// machine. What a database `jadpo dev` started is reached as.
    pub(crate) fn by_default(url: String) -> Address {
        Address {
            url,
            security: Security::Roots,
        }
    }

    /// `url` as the database of connection `store`, secured as its
    /// connection says: its `authority:` setting read as the address's is
    /// (the environment over `.env.local`), or `tls: false`.
    pub(crate) fn of(
        call: &Call,
        loaded: &Loaded,
        store: &str,
        url: String,
    ) -> Result<Address, Reply> {
        let tls =
            jadpo_check::databases::database_connections(&loaded.db, loaded.project).tls_of(store);
        let security = match tls {
            DatabaseTls::Roots => Security::Roots,
            DatabaseTls::Off => Security::Plain,
            DatabaseTls::Authority(field) => {
                Security::Authorities(authority(call, loaded, &field)?)
            }
        };
        Ok(Address { url, security })
    }

    /// The store at this address, over the native transport.
    pub(crate) fn open(&self) -> Result<Postgres, StoreError> {
        jadpo_host_native::postgres_secured(&self.url, self.security.clone())
    }
}

/// The certificates setting `field`, a `Certificate`, holds.
fn authority(call: &Call, loaded: &Loaded, field: &str) -> Result<Vec<Vec<u8>>, Reply> {
    use jadpo_runtime::config::{ProblemKind, Variables};
    let model = jadpo_check::config::config(&loaded.db, loaded.project);
    let Some(setting) = model.setting(field) else {
        return Ok(Vec::new());
    };
    let layered = crate::config::Layered::new(&loaded.root, &call.environment)
        .map_err(|problem| Reply::failed(*problem))?;
    let problem = |kind: ProblemKind| {
        Reply::failed(crate::config::value_diagnostic(
            &setting.name,
            &setting.variable,
            false,
            "Certificate",
            &kind,
        ))
    };
    let Some(text) = layered.variable(&setting.variable) else {
        return Err(problem(ProblemKind::Missing));
    };
    jadpo_std::database_tls::certificates(&text).map_err(|_| {
        problem(ProblemKind::Invalid {
            rule: "format".to_string(),
        })
    })
}

/// How each database connection in `connections` is secured, its
/// authority read from the settings the program started with: what `jadpo
/// dev` reaches each store with. A store on a disposable server is on a
/// Unix socket, which no security changes.
pub(crate) fn securities(
    connections: &[jadpo_std::connections::Connection],
    values: &jadpo_runtime::config::ConfigValues,
) -> std::collections::BTreeMap<String, Security> {
    use jadpo_std::connections::{ConnectionDriver, DatabaseTls as Planned};
    connections
        .iter()
        .filter_map(|connection| {
            let ConnectionDriver::Postgres { tls, .. } = &connection.driver else {
                return None;
            };
            let security = match tls {
                Planned::Roots => Security::Roots,
                Planned::Off => Security::Plain,
                // The start-up check held the setting to `Certificate`'s
                // rules; with no value, nothing is trusted and the store
                // can't be reached over TLS.
                Planned::Authority(at) => Security::Authorities(
                    values
                        .text_at(*at)
                        .and_then(|text| jadpo_std::database_tls::certificates(text).ok())
                        .unwrap_or_default(),
                ),
            };
            Some((connection.name.clone(), security))
        })
        .collect()
}
