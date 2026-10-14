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

/// One migration of one store, as its folder holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Migration {
    /// The folder's name, `20261020T1400_add_priority`: names sort in the
    /// order migrations apply.
    pub name: String,
    /// The digest of what the migration does to this store.
    pub digest: String,
    /// The store's SQL, run as one batch.
    pub sql: String,
}

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
