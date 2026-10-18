//! Applying a store's migrations (area 5, "Migrations"): what `jadpo
//! migrate apply` runs as a deploy step and what `jadpo test` runs to build
//! its template database, so both apply the same SQL the same way.
//!
//! Each store records the migrations it has applied in `jadpo_migrations`,
//! by folder name and digest. Applying runs every pending migration in
//! order, each in its own transaction with its record, after taking the
//! table's lock, so two deploys at once apply each migration once (a row
//! lock, never an advisory lock, §2.6). A store that holds a migration the
//! project doesn't, or one whose digest differs from the project's, has
//! diverged, and nothing is applied: the application refuses to start
//! against a database that does not match (area 5).

use crate::{Database, SqlValue, StoreError, Transaction};

/// The table that records what a store has applied. It is the runtime's
/// own bookkeeping, created here before the first migration runs, so it is
/// in no migration itself.
pub const MIGRATIONS_TABLE: &str = "jadpo_migrations";

const CREATE: &str = "CREATE TABLE IF NOT EXISTS jadpo_migrations (\n    name text PRIMARY KEY,\n    digest text NOT NULL,\n    applied_at timestamptz NOT NULL DEFAULT now()\n)";

/// One migration of one store, as its folder holds it and `jadpo-check`
/// reads it.
pub use jadpo_std::Migration;

/// Where a store stands against the project's migrations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Standing {
    /// Applied, in order.
    pub applied: Vec<String>,
    /// Not applied yet, in order.
    pub pending: Vec<String>,
}

/// Why migrations could not be applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MigrateError {
    Store(StoreError),
    /// The store applied `name`, which the project doesn't have.
    Unknown {
        name: String,
    },
    /// The store applied `name` with another digest: the migration was
    /// changed after it ran.
    Changed {
        name: String,
    },
    /// The store applied `name` but not `missing`, which comes before it.
    OutOfOrder {
        name: String,
        missing: String,
    },
    /// A migration creates `extension` (pgvector's `vector`, area 21),
    /// which the database doesn't offer, or which the store doesn't have
    /// though a migration it applied created it.
    ExtensionMissing {
        extension: String,
    },
    /// A migration needs `extension` at version `least` or later, and the
    /// database server offers `found`: pgvector 0.8.0 for an HNSW index,
    /// whose `nearest` queries run with its iterative scans (area 21,
    /// chunk 5.5v1b).
    ExtensionOutdated {
        extension: String,
        found: String,
        least: String,
    },
}

impl From<StoreError> for MigrateError {
    fn from(error: StoreError) -> MigrateError {
        MigrateError::Store(error)
    }
}

/// Where `database` stands against `migrations`, which must be in order.
pub async fn standing(
    database: &dyn Database,
    migrations: &[Migration],
) -> Result<Standing, MigrateError> {
    let mut transaction = database.begin().await?;
    let found = recorded(transaction.as_mut(), false).await;
    let _ = transaction.rollback().await;
    compare(&found?, migrations)
}

/// Applies every pending migration in `migrations` (in order) to
/// `database`: the names it ran.
pub async fn migrate(
    database: &dyn Database,
    migrations: &[Migration],
) -> Result<Vec<String>, MigrateError> {
    // Every extension a pending migration creates is one the database
    // offers, checked before anything runs (area 21, "Without pgvector").
    let mut transaction = database.begin().await?;
    let offered = offers_every_extension(transaction.as_mut(), migrations).await;
    let offered = match offered {
        Ok(standing) => new_enough(transaction.as_mut(), migrations, &standing.pending).await,
        Err(error) => Err(error),
    };
    let _ = transaction.rollback().await;
    offered?;
    let mut transaction = database.begin().await?;
    transaction.batch(CREATE).await?;
    transaction.commit().await?;
    let mut ran = Vec::new();
    loop {
        let mut transaction = database.begin().await?;
        let step = next(transaction.as_mut(), migrations).await;
        let migration = match step {
            Ok(Some(migration)) => migration,
            Ok(None) => {
                transaction.rollback().await?;
                return Ok(ran);
            }
            Err(error) => {
                let _ = transaction.rollback().await;
                return Err(error);
            }
        };
        let applied = async {
            transaction.batch(&migration.sql).await?;
            transaction
                .execute(
                    "INSERT INTO jadpo_migrations (name, digest) VALUES ($1, $2)",
                    &[
                        SqlValue::Text(migration.name.clone()),
                        SqlValue::Text(migration.digest.clone()),
                    ],
                )
                .await?;
            Ok::<(), StoreError>(())
        }
        .await;
        match applied {
            Ok(()) => transaction.commit().await?,
            Err(error) => {
                let _ = transaction.rollback().await;
                return Err(error.into());
            }
        }
        ran.push(migration.name.clone());
    }
}

/// The start-up check for extensions (area 21, "Without pgvector"): the
/// store has every extension the migrations it applied created, so an
/// application whose vectors need pgvector refuses a database without it
/// rather than failing on its first search.
pub async fn has_extensions(
    database: &dyn Database,
    migrations: &[Migration],
) -> Result<(), MigrateError> {
    let mut transaction = database.begin().await?;
    let found = async {
        let standing = compare(&recorded(transaction.as_mut(), false).await?, migrations)?;
        for name in &standing.applied {
            let Some(migration) = migrations.iter().find(|m| &m.name == name) else {
                continue;
            };
            for extension in migration.extensions() {
                let rows = transaction
                    .query(
                        "SELECT 1 FROM pg_extension WHERE extname = $1",
                        &[SqlValue::Text(extension.clone())],
                    )
                    .await?;
                if rows.is_empty() {
                    return Err(MigrateError::ExtensionMissing { extension });
                }
            }
        }
        new_enough(transaction.as_mut(), migrations, &standing.applied).await
    }
    .await;
    let _ = transaction.rollback().await;
    found
}

/// Whether the database server offers each extension the migrations
/// `names` need at the version they need it (pgvector 0.8.0 once one
/// builds an HNSW index, chunk 5.5v1b): the version its server would load,
/// `pg_available_extensions.default_version`, since the settings a query
/// turns on come from the library the server has, whichever version the
/// database last installed.
async fn new_enough(
    transaction: &mut dyn Transaction,
    migrations: &[Migration],
    names: &[String],
) -> Result<(), MigrateError> {
    let mut needed: Vec<(String, [u32; 3])> = Vec::new();
    for name in names {
        let Some(migration) = migrations.iter().find(|m| &m.name == name) else {
            continue;
        };
        for (extension, least) in migration.least_versions() {
            match needed.iter_mut().find(|(e, _)| *e == extension) {
                Some((_, most)) => *most = (*most).max(least),
                None => needed.push((extension, least)),
            }
        }
    }
    for (extension, least) in needed {
        let rows = transaction
            .query(
                "SELECT default_version FROM pg_available_extensions WHERE name = $1",
                &[SqlValue::Text(extension.clone())],
            )
            .await?;
        let Some(Some(SqlValue::Text(found))) = rows.first().map(|row| row.first()) else {
            return Err(MigrateError::ExtensionMissing { extension });
        };
        if jadpo_std::migration::version(found).is_none_or(|version| version < least) {
            return Err(MigrateError::ExtensionOutdated {
                extension,
                found: found.clone(),
                least: jadpo_std::migration::version_text(least),
            });
        }
    }
    Ok(())
}

/// Whether the database offers every extension a migration not yet
/// applied creates: one installed or one its server could install
/// (`pg_available_extensions`). Whether the role may create it is found
/// when the migration runs.
async fn offers_every_extension(
    transaction: &mut dyn Transaction,
    migrations: &[Migration],
) -> Result<Standing, MigrateError> {
    let standing = compare(&recorded(transaction, false).await?, migrations)?;
    for name in &standing.pending {
        let Some(migration) = migrations.iter().find(|m| &m.name == name) else {
            continue;
        };
        for extension in migration.extensions() {
            let rows = transaction
                .query(
                    "SELECT 1 FROM pg_available_extensions WHERE name = $1",
                    &[SqlValue::Text(extension.clone())],
                )
                .await?;
            if rows.is_empty() {
                return Err(MigrateError::ExtensionMissing { extension });
            }
        }
    }
    Ok(standing)
}

/// The first migration still to apply, under the table's lock.
async fn next<'m>(
    transaction: &mut dyn Transaction,
    migrations: &'m [Migration],
) -> Result<Option<&'m Migration>, MigrateError> {
    let found = recorded(transaction, true).await?;
    let standing = compare(&found, migrations)?;
    Ok(standing
        .pending
        .first()
        .and_then(|name| migrations.iter().find(|m| &m.name == name)))
}

/// The migrations the store records, in order, locked against another
/// deploy when `lock`. A store that has never been migrated records none.
async fn recorded(
    transaction: &mut dyn Transaction,
    lock: bool,
) -> Result<Vec<(String, String)>, MigrateError> {
    let exists = transaction
        .query("SELECT to_regclass('jadpo_migrations') IS NOT NULL", &[])
        .await?;
    if !matches!(
        exists.first().and_then(|row| row.first()),
        Some(SqlValue::Bool(true))
    ) {
        return Ok(Vec::new());
    }
    if lock {
        transaction
            .batch("LOCK TABLE jadpo_migrations IN EXCLUSIVE MODE")
            .await?;
    }
    let rows = transaction
        .query(
            "SELECT name, digest FROM jadpo_migrations ORDER BY name",
            &[],
        )
        .await?;
    let mut found = Vec::new();
    for row in rows {
        match (row.first(), row.get(1)) {
            (Some(SqlValue::Text(name)), Some(SqlValue::Text(digest))) => {
                found.push((name.clone(), digest.clone()));
            }
            _ => {
                return Err(MigrateError::Store(StoreError::Fault(
                    "jadpo_migrations holds a row that isn't a name and a digest".into(),
                )));
            }
        }
    }
    Ok(found)
}

fn compare(found: &[(String, String)], migrations: &[Migration]) -> Result<Standing, MigrateError> {
    for (name, digest) in found {
        match migrations.iter().find(|m| &m.name == name) {
            None => return Err(MigrateError::Unknown { name: name.clone() }),
            Some(m) if &m.digest != digest => {
                return Err(MigrateError::Changed { name: name.clone() });
            }
            Some(_) => {}
        }
    }
    let mut applied = Vec::new();
    let mut pending = Vec::new();
    for migration in migrations {
        if found.iter().any(|(name, _)| name == &migration.name) {
            if let Some(missing) = pending.first() {
                return Err(MigrateError::OutOfOrder {
                    name: migration.name.clone(),
                    missing: String::clone(missing),
                });
            }
            applied.push(migration.name.clone());
        } else {
            pending.push(migration.name.clone());
        }
    }
    Ok(Standing { applied, pending })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(name: &str, digest: &str) -> Migration {
        Migration {
            name: name.into(),
            digest: digest.into(),
            sql: String::new(),
        }
    }

    fn found(rows: &[(&str, &str)]) -> Vec<(String, String)> {
        rows.iter()
            .map(|(n, d)| (n.to_string(), d.to_string()))
            .collect()
    }

    #[test]
    fn a_store_is_compared_with_the_projects_migrations() {
        let project = [m("a", "1"), m("b", "2"), m("c", "3")];
        assert_eq!(
            compare(&found(&[("a", "1")]), &project),
            Ok(Standing {
                applied: vec!["a".into()],
                pending: vec!["b".into(), "c".into()],
            })
        );
        assert_eq!(
            compare(&found(&[("a", "9")]), &project),
            Err(MigrateError::Changed { name: "a".into() })
        );
        assert_eq!(
            compare(&found(&[("z", "1")]), &project),
            Err(MigrateError::Unknown { name: "z".into() })
        );
        assert_eq!(
            compare(&found(&[("a", "1"), ("c", "3")]), &project),
            Err(MigrateError::OutOfOrder {
                name: "c".into(),
                missing: "b".into(),
            })
        );
    }
}
