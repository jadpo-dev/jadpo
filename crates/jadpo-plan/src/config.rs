//! The settings plan (area 10; docs/compiler-runtime.md §2.6): each
//! setting `config { }` declares as `jadpo-std`'s plain data, with the
//! codec its value is read and checked by, so either executor, on any
//! platform, starts the application from the same list of variables.

use jadpo_check::config::{Reads, config, connection_urls};
use jadpo_db::{Db, Project};
use jadpo_graph::{Key, Kind};
use jadpo_std::config::{ConfigConnection, ConfigField, ConfigPlan, Spelling};

use crate::routes::Codecs;
use crate::types::Types;

/// The settings of `project` that `keeps` keeps, every one but in a
/// deployable's plan (chunk 8.1o3), and the connections' addresses.
pub(crate) fn plan(
    db: &dyn Db,
    project: Project,
    types: &mut Types,
    keeps: &dyn Fn(&str) -> bool,
) -> ConfigPlan {
    let model = config(db, project);
    let lookup = jadpo_check::lookups::Lookup::new(db, project);
    let mut codecs =
        Codecs::new(types).supporting(jadpo_check::locales::locales(db, project).plan.clone());
    let fields = model
        .settings
        .iter()
        .filter(|setting| keeps(&setting.name))
        .map(|setting| ConfigField {
            name: setting.name.clone(),
            variable: setting.variable.clone(),
            secret: setting.secret,
            codec: codecs.codec(&jadpo_check::graph::type_ref(&lookup, &setting.ty)),
            spelling: match setting.reads {
                Reads::Zone => Spelling::of(jadpo_std::ZONE),
                Reads::Locale => Spelling::of(jadpo_std::LOCALE),
                Reads::Duration => Spelling::Duration,
                _ => Spelling::Plain,
            },
            default: setting.default.clone(),
            test: setting.test.clone(),
            generated: setting.generated,
            rotates: setting.rotates,
        })
        .collect::<Vec<_>>();
    let connections = connection_urls(db, project)
        .urls
        .iter()
        .filter_map(|url| {
            let field = fields.iter().position(|field| field.name == url.setting)?;
            Some(ConfigConnection {
                connection: url.connection.clone(),
                driver: match url.kind {
                    jadpo_check::data::ConnectionKind::Redis => jadpo_std::Driver::Redis,
                    _ => jadpo_std::Driver::Postgres,
                },
                field: u32::try_from(field).ok()?,
            })
        })
        .collect();
    ConfigPlan {
        fields,
        codecs: codecs.into_codecs(),
        connections,
    }
}

/// The key a setting called `name` has in the graph.
pub(crate) fn key(registry: &jadpo_graph::Registry, name: &str) -> Key {
    registry.key(Kind::ConfigField, name, None)
}
