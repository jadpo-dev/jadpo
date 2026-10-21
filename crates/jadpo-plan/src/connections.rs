//! The connections table (area 11, "Connecting in `app.jadpo`"; chunk
//! 8.1d): every connection `connections:` declares, in name order, with
//! its driver, the settings that driver is built from and the targets it
//! runs on, read from what the checker found of each kind. A connection
//! whose settings don't read (a checked program has none) is left out.

use jadpo_check::data::ConnectionKind;
use jadpo_db::{Db, Project};
use jadpo_std::ai::AiUrl;
use jadpo_std::connections::{Connection, ConnectionDriver, DatabaseTls, SmtpServer};
use jadpo_std::email::SmtpHost;
use jadpo_std::receiving::CloudflareQueue;

/// The table of the connections `keeps` keeps, by name and kind: every
/// one but in a deployable's plan (chunk 8.1o3). Each setting is read
/// through `setting`.
pub(crate) fn table(
    db: &dyn Db,
    project: Project,
    setting: &dyn Fn(&str) -> Option<u32>,
    keeps: &dyn Fn(&str, &ConnectionKind) -> bool,
) -> Vec<Connection> {
    let declared = jadpo_check::data::connections(db, project);
    let mut kinds: Vec<(String, ConnectionKind)> = declared.declared.clone();
    // A project with no database connection keeps its entities in `main`
    // on PostgreSQL (area 5, "Stores").
    if declared.implied_main {
        kinds.push((
            jadpo_check::data::MAIN.to_string(),
            ConnectionKind::Postgres,
        ));
    }
    let urls = jadpo_check::config::connection_urls(db, project);
    let url = |name: &str| urls.url_of(name).and_then(|url| setting(&url.setting));
    let databases = jadpo_check::databases::database_connections(db, project);
    let files = jadpo_check::data::files_connections(db, project);
    let smtp = &jadpo_check::emails::smtp_connections(db, project).list;
    let models = &jadpo_check::ai::model_connections(db, project).list;
    let queues = &jadpo_check::receiving::cloudflare_queues(db, project).list;
    let limits = &jadpo_check::rate_limits::binding_connections(db, project).list;
    let packages = &jadpo_check::connections::package_connections(db, project).connections;
    let mut table: Vec<Connection> = kinds
        .iter()
        .filter(|(name, kind)| keeps(name, kind))
        .filter_map(|(name, kind)| {
            let driver = match kind {
                ConnectionKind::Postgres => ConnectionDriver::Postgres {
                    url: url(name),
                    tls: match databases.tls_of(name) {
                        jadpo_check::databases::DatabaseTls::Roots => DatabaseTls::Roots,
                        jadpo_check::databases::DatabaseTls::Authority(field) => {
                            DatabaseTls::Authority(setting(&field)?)
                        }
                        jadpo_check::databases::DatabaseTls::Off => DatabaseTls::Off,
                    },
                },
                ConnectionKind::Redis => ConnectionDriver::Redis { url: url(name) },
                ConnectionKind::Files => ConnectionDriver::Disk {
                    path: files
                        .iter()
                        .find(|store| store.name == *name)
                        .and_then(|store| store.path.clone()),
                },
                ConnectionKind::Smtp => {
                    let server = smtp.iter().find(|server| server.name == *name)?;
                    ConnectionDriver::Smtp(SmtpServer {
                        host: match server.host.as_ref()? {
                            jadpo_check::emails::SmtpHost::Setting(name) => {
                                SmtpHost::Setting(setting(name)?)
                            }
                            jadpo_check::emails::SmtpHost::Written(host) => {
                                SmtpHost::Written(host.clone())
                            }
                        },
                        port: server.port,
                        username: server.username.as_deref().and_then(setting),
                        password: server.password.as_deref().and_then(setting),
                    })
                }
                ConnectionKind::Model => {
                    let model = models.iter().find(|model| model.name == *name)?;
                    ConnectionDriver::OpenAiCompatible {
                        url: match model.url.as_ref()? {
                            jadpo_check::ai::ModelUrl::Setting(name) => {
                                AiUrl::Setting(setting(name)?)
                            }
                            jadpo_check::ai::ModelUrl::Written(text) => {
                                AiUrl::Written(text.clone())
                            }
                        },
                    }
                }
                ConnectionKind::CloudflareQueue => {
                    let queue = queues.iter().find(|queue| queue.name == *name)?;
                    ConnectionDriver::CloudflareQueues(CloudflareQueue {
                        queue: queue.queue.clone(),
                        dead_letter_queue: queue.dead_letter_queue.clone(),
                    })
                }
                ConnectionKind::CloudflareRateLimits => ConnectionDriver::CloudflareRateLimits {
                    namespace: limits
                        .iter()
                        .find(|connection| connection.name == *name)?
                        .namespace,
                },
                ConnectionKind::Package => {
                    let connection = packages.iter().find(|known| known.name == *name)?;
                    ConnectionDriver::Package(Box::new(crate::providers::connection_plan(
                        db, project, connection, setting,
                    )))
                }
                ConnectionKind::Other => return None,
            };
            Some(Connection::new(name.clone(), driver))
        })
        .collect();
    table.sort_by(|a, b| a.name.cmp(&b.name));
    table.dedup_by(|a, b| a.name == b.name);
    table
}
