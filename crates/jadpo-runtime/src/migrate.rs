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
//!
//! A migration that fills a backfill in batches (area 5, "Computed
//! backfills"; chunk 5.6b) runs as steps instead: the statements before
//! the backfill in one transaction, each batch in a transaction of its
//! own, and the statements after it in a last one, which records the
//! migration. Every step takes the table's lock first and records how far
//! the migration got in `jadpo_migration_steps`, in the transaction that
//! does the step, so a run stopped anywhere leaves every column
//! consistent and the next run carries on from the step after the last
//! one committed. Its steps run at read committed, so each batch fills
//! the rows as they are when it runs and the last step, which locks the
//! table against writes before it fills what servers still on the old
//! build wrote meanwhile, sees every row they committed.
//!
//! A backfill with an operation that has no SQL form (chunk 5.6b2) runs in
//! the same steps, but each batch reads its rows, locked as an `UPDATE`
//! would lock them, has [`Backfills`] work their values out in Jadpo, and
//! writes them back in one statement; its last step does the same for the
//! rows written meanwhile, with the table locked against writes.
//!
//! A structured field whose type may no longer read the values stored as
//! its JSON (chunk 5.9c) has them checked first, in batches, each read in
//! a transaction of its own and changing nothing: [`Backfills`] reads each
//! with the field's type now, and while any doesn't read the migration is
//! refused, naming how many and the first rule broken, never a value, with
//! nothing of it applied. A run stopped during a check starts it again,
//! since a check records nothing. With a backfill, those values are
//! rewritten instead, in batches recorded as a backfill's are.

use std::ops::ControlFlow;

use jadpo_std::migration::Part;
use jadpo_std::{Isolation, SqlType, Step, Work};

use crate::{Database, SqlValue, StoreError, Transaction};

/// The table that records what a store has applied. It is the runtime's
/// own bookkeeping, created here before the first migration runs, so it is
/// in no migration itself.
pub const MIGRATIONS_TABLE: &str = "jadpo_migrations";

const CREATE: &str = "CREATE TABLE IF NOT EXISTS jadpo_migrations (\n    name text PRIMARY KEY,\n    digest text NOT NULL,\n    applied_at timestamptz NOT NULL DEFAULT now()\n)";

/// The table that records how far a migration filled in batches got: its
/// next step and the key the last batch ended at (chunk 5.6b). Created
/// the first time such a migration runs; a migration's row goes when it
/// is recorded as applied.
pub const STEPS_TABLE: &str = "jadpo_migration_steps";

const CREATE_STEPS: &str = "CREATE TABLE IF NOT EXISTS jadpo_migration_steps (\n    name text PRIMARY KEY,\n    digest text NOT NULL,\n    step integer NOT NULL,\n    after uuid\n)";

/// Works out in Jadpo the values of a backfill with an operation that has
/// no SQL form (area 5, "Computed backfills"; chunk 5.6b2): what `jadpo
/// migrate apply` hands the runtime, from the project's interpreter.
pub trait Backfills {
    /// The value of `field`'s backfill in `migration` for each of `rows`,
    /// in order, bound for its column through the storage codec: each row
    /// the columns the backfill reads, as its SQL selects them.
    fn work_out(
        &self,
        migration: &str,
        field: &str,
        rows: &[&[SqlValue]],
    ) -> Result<Vec<SqlValue>, Unworked>;

    /// For each of `values`, a value of the structured field `field` as
    /// its JSON column holds it, `None` when the field's type as of
    /// `migration` reads it, and otherwise the first rule it breaks and
    /// where, never the value (chunk 5.9c).
    fn unreadable(
        &self,
        _migration: &str,
        _field: &str,
        _values: &[&SqlValue],
    ) -> Result<Vec<Option<String>>, Unworked> {
        Err(Unworked::Unavailable)
    }
}

/// Why [`Backfills`] gave no values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unworked {
    /// Nothing here can work it out: no program, or one that doesn't hold
    /// this backfill, since only the latest migration's are checked
    /// against the project's code.
    Unavailable,
    /// Working out row `at` stopped: why, never the row's values.
    Stopped { at: usize, reason: String },
}

/// No program: a backfill worked out in Jadpo stops at its first row.
struct NoProgram;

impl Backfills for NoProgram {
    fn work_out(
        &self,
        _migration: &str,
        _field: &str,
        _rows: &[&[SqlValue]],
    ) -> Result<Vec<SqlValue>, Unworked> {
        Err(Unworked::Unavailable)
    }
}

/// What applying migrations reports as it goes (chunk 5.6b).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Progress<'a> {
    /// A backfill's batches begin, or carry on from an earlier run; or a
    /// structured field's check or rewrite of its stored values (chunk
    /// 5.9c).
    Filling {
        migration: &'a str,
        field: &'a str,
        /// What each batch does with its rows.
        work: Work,
        /// How each batch is worked out: `by UPDATE`, or `in Jadpo`.
        how: &'a str,
        /// The rows left to fill.
        left: u64,
        /// Whether an earlier run filled some of it already.
        resumed: bool,
    },
    /// A batch committed.
    Filled {
        migration: &'a str,
        field: &'a str,
        /// The rows this batch filled, checked or rewrote.
        rows: u64,
    },
    /// A migration was applied and recorded.
    Applied { migration: &'a str },
}

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
    /// A migration creates `extension`, which the database server offers
    /// but the role applying migrations can't create: PostGIS isn't a
    /// trusted extension, so a superuser or the provider's own admin role
    /// creates it (area 20, "A store with no PostGIS"; chunk 5.5u4).
    ExtensionNotCreatable {
        extension: String,
    },
    /// Applying stopped between two steps of `name`, as the run was asked
    /// to: what committed stays, and the next run carries on (chunk 5.6b).
    Stopped {
        name: String,
    },
    /// `name`'s backfill of `field` is worked out in Jadpo and rows need it,
    /// but nothing given can work it out (chunk 5.6b2).
    Unworked {
        name: String,
        field: String,
    },
    /// `name` reads the stored values of the structured field `field` with
    /// its type as the project's code declares it, rows need it, and
    /// nothing given can read them (chunk 5.9c).
    Unchecked {
        name: String,
        field: String,
    },
    /// Working out `name`'s backfill of `field` in Jadpo stopped at the
    /// row whose key is `record`, for `reason` (chunk 5.6b2): its batch
    /// rolled back.
    BackfillStopped {
        name: String,
        field: String,
        record: String,
        reason: String,
    },
    /// `name` changes the type of the structured field `field`, which reads
    /// less than it did, and `rows` of its stored values don't read with
    /// it, the first breaking `first` (`` `max_length` on `line` ``), never
    /// a value (chunk 5.9c): nothing of `name` was applied.
    Unreadable {
        name: String,
        field: String,
        rows: u64,
        first: String,
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
    migrate_reporting(database, migrations, &mut |_| ControlFlow::Continue(())).await
}

/// [`migrate`], telling `report` what it does as it goes: each backfill's
/// batches as they commit. When `report` breaks, applying stops before the
/// next step with [`MigrateError::Stopped`]; what committed stays.
pub async fn migrate_reporting(
    database: &dyn Database,
    migrations: &[Migration],
    report: &mut dyn FnMut(&Progress<'_>) -> ControlFlow<()>,
) -> Result<Vec<String>, MigrateError> {
    migrate_with(database, migrations, &NoProgram, report).await
}

/// [`migrate_reporting`], with `backfills` working out in Jadpo the
/// backfills that have no SQL form (chunk 5.6b2). Without it, such a
/// backfill stops at the first row it has to fill, with
/// [`MigrateError::Unworked`]; on an empty table it has none.
pub async fn migrate_with(
    database: &dyn Database,
    migrations: &[Migration],
    backfills: &dyn Backfills,
    report: &mut dyn FnMut(&Progress<'_>) -> ControlFlow<()>,
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
    // The backfills whose rows left have been counted in this run.
    let mut counted: Vec<(String, usize)> = Vec::new();
    // How far each check of stored values got in this run (chunk 5.9c).
    let mut checking: Vec<Checking> = Vec::new();
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
        if migration.has_batches() {
            transaction.rollback().await?;
            match in_steps(
                database,
                migration,
                migrations,
                backfills,
                &mut Tally {
                    counted: &mut counted,
                    checking: &mut checking,
                },
                report,
            )
            .await?
            {
                Stepped::Applied => {
                    ran.push(migration.name.clone());
                    if report(&Progress::Applied {
                        migration: &migration.name,
                    })
                    .is_break()
                    {
                        return Err(MigrateError::Stopped {
                            name: migration.name.clone(),
                        });
                    }
                }
                Stepped::Again => {}
            }
            continue;
        }
        let applied = async {
            transaction.batch(&migration.sql).await?;
            record(transaction.as_mut(), migration).await
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
        if report(&Progress::Applied {
            migration: &migration.name,
        })
        .is_break()
        {
            return Err(MigrateError::Stopped {
                name: migration.name.clone(),
            });
        }
    }
}

/// Records `migration` as applied.
async fn record(
    transaction: &mut dyn Transaction,
    migration: &Migration,
) -> Result<(), StoreError> {
    transaction
        .execute(
            "INSERT INTO jadpo_migrations (name, digest) VALUES ($1, $2)",
            &[
                SqlValue::Text(migration.name.clone()),
                SqlValue::Text(migration.digest.clone()),
            ],
        )
        .await?;
    Ok(())
}

/// How far a check of a structured field's stored values got in this
/// run (chunk 5.9c): it records nothing in the store, so a run stopped
/// during it checks every value again.
struct Checking {
    migration: String,
    step: usize,
    /// The key the last batch ended at.
    after: Option<[u8; 16]>,
    /// The values that didn't read, and the first one's problem.
    unread: u64,
    first: Option<String>,
}

/// What a run keeps between a migration's steps.
struct Tally<'t> {
    /// The steps in batches whose rows left have been counted.
    counted: &'t mut Vec<(String, usize)>,
    checking: &'t mut Vec<Checking>,
}

/// What one step of a migration filled in batches left to do.
enum Stepped {
    /// The migration is applied and recorded.
    Applied,
    /// It has steps left, or another deploy moved it on: look again.
    Again,
}

/// How many times a batch is tried again after a concurrent change (a
/// deadlock with a request writing the same rows) before applying stops.
const BATCH_ATTEMPTS: u32 = 3;

/// Runs the next step of `migration`, which fills a backfill in batches,
/// in a transaction of its own at read committed: the table's lock
/// first, then the step and the record of how far it got.
async fn in_steps(
    database: &dyn Database,
    migration: &Migration,
    migrations: &[Migration],
    backfills: &dyn Backfills,
    tally: &mut Tally<'_>,
    report: &mut dyn FnMut(&Progress<'_>) -> ControlFlow<()>,
) -> Result<Stepped, MigrateError> {
    let steps = migration.steps();
    let mut attempt = 0;
    loop {
        attempt += 1;
        let mut transaction = database.begin_at(Isolation::ReadCommitted).await?;
        let stepped = one_step(
            transaction.as_mut(),
            migration,
            migrations,
            &steps,
            backfills,
            tally,
            report,
        )
        .await;
        match stepped {
            Ok(Some(stepped)) => {
                transaction.commit().await?;
                if let Some(rows) = stepped.filled
                    && let Some(field) = stepped.field
                    && report(&Progress::Filled {
                        migration: &migration.name,
                        field,
                        rows,
                    })
                    .is_break()
                {
                    return Err(MigrateError::Stopped {
                        name: migration.name.clone(),
                    });
                }
                return Ok(if stepped.applied {
                    Stepped::Applied
                } else {
                    Stepped::Again
                });
            }
            Ok(None) => {
                transaction.rollback().await?;
                return Ok(Stepped::Again);
            }
            Err(MigrateError::Store(StoreError::ConcurrentChange)) if attempt < BATCH_ATTEMPTS => {
                let _ = transaction.rollback().await;
            }
            // Refused for a stored value its type no longer reads (chunk
            // 5.9c): every step before is a check, which changed nothing,
            // so the migration starts again from its first, and the next
            // run checks every value again.
            Err(error @ MigrateError::Unreadable { .. }) => {
                let _ = transaction.rollback().await;
                let mut transaction = database.begin_at(Isolation::ReadCommitted).await?;
                transaction
                    .batch("LOCK TABLE jadpo_migrations IN EXCLUSIVE MODE")
                    .await?;
                transaction
                    .execute(
                        "DELETE FROM jadpo_migration_steps WHERE name = $1",
                        &[SqlValue::Text(migration.name.clone())],
                    )
                    .await?;
                transaction.commit().await?;
                return Err(error);
            }
            Err(error) => {
                let _ = transaction.rollback().await;
                return Err(error);
            }
        }
    }
}

/// What one step did.
struct OneStep<'s> {
    /// The migration is now applied.
    applied: bool,
    /// The field a batch filled, and how many rows it filled.
    field: Option<&'s str>,
    filled: Option<u64>,
}

/// One step of `migration`, under the table's lock: `None` when it is no
/// longer the next migration to apply.
async fn one_step<'s>(
    transaction: &mut dyn Transaction,
    migration: &Migration,
    migrations: &[Migration],
    steps: &'s [Step],
    backfills: &dyn Backfills,
    tally: &mut Tally<'_>,
    report: &mut dyn FnMut(&Progress<'_>) -> ControlFlow<()>,
) -> Result<Option<OneStep<'s>>, MigrateError> {
    // The lock is the transaction's first statement, so nothing it reads
    // was decided before another deploy's step committed.
    transaction
        .batch("LOCK TABLE jadpo_migrations IN EXCLUSIVE MODE")
        .await?;
    let still = next(transaction, migrations).await?;
    if still.map(|m| &m.name) != Some(&migration.name) {
        return Ok(None);
    }
    transaction.batch(CREATE_STEPS).await?;
    let rows = transaction
        .query(
            "SELECT digest, step, after FROM jadpo_migration_steps WHERE name = $1",
            &[SqlValue::Text(migration.name.clone())],
        )
        .await?;
    let (at, after) = match rows.first().map(Vec::as_slice) {
        None => (0usize, None),
        Some([SqlValue::Text(digest), SqlValue::Int4(step), after]) => {
            if digest != &migration.digest {
                return Err(MigrateError::Changed {
                    name: migration.name.clone(),
                });
            }
            let after = match after {
                SqlValue::Uuid(bytes) => Some(*bytes),
                _ => None,
            };
            (usize::try_from(*step).unwrap_or(0), after)
        }
        Some(_) => {
            return Err(MigrateError::Store(StoreError::Fault(
                "jadpo_migration_steps holds a row that isn't a digest, a step and a key".into(),
            )));
        }
    };
    let Some(step) = steps.get(at) else {
        return Err(MigrateError::Store(StoreError::Fault(format!(
            "jadpo_migration_steps says `{}` is at step {at}, past its last",
            migration.name
        ))));
    };
    let last = at + 1 == steps.len();
    match step {
        Step::Once(_) | Step::Mixed(_) => {
            let parts = match step {
                Step::Mixed(parts) => parts.clone(),
                Step::Once(sql) => vec![Part::Sql(sql.clone())],
                Step::Batches(_) => Vec::new(),
            };
            for part in &parts {
                match part {
                    Part::Sql(sql) => transaction.batch(sql).await?,
                    // The rows written meanwhile, worked out in Jadpo
                    // with the table locked against writes (chunk 5.6b2).
                    Part::Fill(fill) => {
                        let rows = transaction.query(&fill.read, &[]).await?;
                        in_jadpo(
                            transaction,
                            backfills,
                            &migration.name,
                            &fill.field,
                            &rows,
                            &fill.write,
                        )
                        .await?;
                    }
                }
            }
            if last {
                finish(transaction, migration).await?;
            } else {
                progress(transaction, migration, at + 1, None).await?;
            }
            Ok(Some(OneStep {
                applied: last,
                field: None,
                filled: None,
            }))
        }
        Step::Batches(batches) if batches.work == Work::Check => {
            check_batch(
                transaction,
                migration,
                at,
                last,
                batches,
                backfills,
                tally,
                report,
            )
            .await
        }
        Step::Batches(batches) => {
            let counted = &mut *tally.counted;
            let key = (migration.name.clone(), at);
            if !counted.contains(&key) {
                let rows = transaction.query(&batches.count, &[]).await?;
                let left = match rows.first().and_then(|row| row.first()) {
                    Some(SqlValue::Int8(n)) => u64::try_from(*n).unwrap_or(0),
                    _ => 0,
                };
                counted.push(key);
                if report(&Progress::Filling {
                    migration: &migration.name,
                    field: &batches.field,
                    work: batches.work,
                    how: &batches.how,
                    left,
                    resumed: after.is_some(),
                })
                .is_break()
                {
                    return Err(MigrateError::Stopped {
                        name: migration.name.clone(),
                    });
                }
            }
            let rows = transaction
                .query(&batches.batch, &[SqlValue::Uuid(after.unwrap_or([0; 16]))])
                .await?;
            // In Jadpo (chunk 5.6b2): the batch's rows read, their values
            // worked out and written back.
            let rows = match &batches.write {
                None => rows,
                Some(write) => {
                    let ended = match rows.last().and_then(|row| row.first()) {
                        Some(SqlValue::Uuid(key)) => SqlValue::Uuid(*key),
                        _ => SqlValue::Null(SqlType::Uuid),
                    };
                    let filled = match batches.work {
                        Work::Rewrite => {
                            rewrite(
                                transaction,
                                backfills,
                                &migration.name,
                                &batches.field,
                                &rows,
                                write,
                            )
                            .await?
                        }
                        Work::Fill | Work::Check => {
                            in_jadpo(
                                transaction,
                                backfills,
                                &migration.name,
                                &batches.field,
                                &rows,
                                write,
                            )
                            .await?
                        }
                    };
                    vec![vec![ended, SqlValue::Int8(filled as i64)]]
                }
            };
            let (ended, filled) = match rows.first().map(Vec::as_slice) {
                Some([ended, SqlValue::Int8(filled)]) => (
                    match ended {
                        SqlValue::Uuid(bytes) => Some(*bytes),
                        _ => None,
                    },
                    u64::try_from(*filled).unwrap_or(0),
                ),
                _ => {
                    return Err(MigrateError::Store(StoreError::Fault(format!(
                        "a batch of `{}` gave {rows:?}, not the key it ended at and the rows it filled",
                        batches.field
                    ))));
                }
            };
            match ended {
                // No row left past the last batch: the next step.
                None => {
                    if last {
                        finish(transaction, migration).await?;
                    } else {
                        progress(transaction, migration, at + 1, None).await?;
                    }
                    Ok(Some(OneStep {
                        applied: last,
                        field: None,
                        filled: None,
                    }))
                }
                Some(ended) => {
                    progress(transaction, migration, at, Some(ended)).await?;
                    Ok(Some(OneStep {
                        applied: false,
                        field: Some(&batches.field),
                        filled: Some(filled),
                    }))
                }
            }
        }
    }
}

/// One batch of the check of a structured field's stored values (chunk
/// 5.9c): the next rows past where this run's last batch ended, each its
/// key and its value, read with the field's type now. Once no row is left
/// the step is done, or, when a value didn't read, the migration is
/// refused, with nothing of it applied, since its check is its first step.
#[allow(clippy::too_many_arguments)]
async fn check_batch<'s>(
    transaction: &mut dyn Transaction,
    migration: &Migration,
    at: usize,
    last: bool,
    batches: &'s jadpo_std::Batches,
    backfills: &dyn Backfills,
    tally: &mut Tally<'_>,
    report: &mut dyn FnMut(&Progress<'_>) -> ControlFlow<()>,
) -> Result<Option<OneStep<'s>>, MigrateError> {
    let found = tally
        .checking
        .iter()
        .position(|c| c.migration == migration.name && c.step == at);
    let index = match found {
        Some(index) => index,
        None => {
            let rows = transaction.query(&batches.count, &[]).await?;
            let left = match rows.first().and_then(|row| row.first()) {
                Some(SqlValue::Int8(n)) => u64::try_from(*n).unwrap_or(0),
                _ => 0,
            };
            if report(&Progress::Filling {
                migration: &migration.name,
                field: &batches.field,
                work: Work::Check,
                how: &batches.how,
                left,
                resumed: false,
            })
            .is_break()
            {
                return Err(MigrateError::Stopped {
                    name: migration.name.clone(),
                });
            }
            tally.checking.push(Checking {
                migration: migration.name.clone(),
                step: at,
                after: None,
                unread: 0,
                first: None,
            });
            tally.checking.len() - 1
        }
    };
    let after = tally.checking[index].after;
    let rows = transaction
        .query(&batches.batch, &[SqlValue::Uuid(after.unwrap_or([0; 16]))])
        .await?;
    let Some(ended) = rows.last().and_then(|row| match row.first() {
        Some(SqlValue::Uuid(key)) => Some(*key),
        _ => None,
    }) else {
        // Every value is read: refused while one didn't read.
        let done = tally.checking.remove(index);
        if done.unread > 0 {
            return Err(MigrateError::Unreadable {
                name: migration.name.clone(),
                field: batches.field.clone(),
                rows: done.unread,
                first: done.first.unwrap_or_default(),
            });
        }
        if last {
            finish(transaction, migration).await?;
        } else {
            progress(transaction, migration, at + 1, None).await?;
        }
        return Ok(Some(OneStep {
            applied: last,
            field: None,
            filled: None,
        }));
    };
    let mut values: Vec<&SqlValue> = Vec::with_capacity(rows.len());
    for row in &rows {
        match row.get(1) {
            Some(value) => values.push(value),
            None => {
                return Err(MigrateError::Store(StoreError::Fault(format!(
                    "a check of `{}` gave a row without its value",
                    batches.field
                ))));
            }
        }
    }
    let unread = unread(backfills, &migration.name, &batches.field, &values)?;
    let checking = &mut tally.checking[index];
    for problem in unread.into_iter().flatten() {
        checking.unread += 1;
        checking.first.get_or_insert(problem);
    }
    checking.after = Some(ended);
    Ok(Some(OneStep {
        applied: false,
        field: Some(&batches.field),
        filled: Some(rows.len() as u64),
    }))
}

/// Which of `values`, a structured field's stored JSON, its type as of
/// `migration` doesn't read, each with its problem (chunk 5.9c).
fn unread(
    backfills: &dyn Backfills,
    migration: &str,
    field: &str,
    values: &[&SqlValue],
) -> Result<Vec<Option<String>>, MigrateError> {
    let unread =
        backfills
            .unreadable(migration, field, values)
            .map_err(|unworked| match unworked {
                Unworked::Unavailable => MigrateError::Unchecked {
                    name: migration.to_string(),
                    field: field.to_string(),
                },
                Unworked::Stopped { reason, .. } => MigrateError::Store(StoreError::Fault(reason)),
            })?;
    if unread.len() != values.len() {
        return Err(MigrateError::Store(StoreError::Fault(format!(
            "`{field}`'s check gave {} answers for {} values",
            unread.len(),
            values.len()
        ))));
    }
    Ok(unread)
}

/// Rewrites, with `migration`'s backfill of `field` worked out in Jadpo,
/// the values among `rows` that the field's type doesn't read (chunk
/// 5.9c): each row its key, its value and then the columns the backfill
/// reads. How many it rewrote.
async fn rewrite(
    transaction: &mut dyn Transaction,
    backfills: &dyn Backfills,
    migration: &str,
    field: &str,
    rows: &[crate::Row],
    write: &str,
) -> Result<u64, MigrateError> {
    let mut values: Vec<&SqlValue> = Vec::with_capacity(rows.len());
    for row in rows {
        match row.get(1) {
            Some(value) => values.push(value),
            None => {
                return Err(MigrateError::Store(StoreError::Fault(format!(
                    "a rewrite of `{field}` gave a row without its value"
                ))));
            }
        }
    }
    let unread = unread(backfills, migration, field, &values)?;
    // The rows to rewrite, each its key and then the columns it reads.
    let chosen: Vec<crate::Row> = rows
        .iter()
        .zip(&unread)
        .filter(|(_, problem)| problem.is_some())
        .map(|(row, _)| {
            let mut kept = row.clone();
            kept.remove(1);
            kept
        })
        .collect();
    in_jadpo(transaction, backfills, migration, field, &chosen, write).await
}

/// Works out in Jadpo the values of `migration`'s backfill of `field` for
/// `rows`, each its key and then the columns the backfill reads, and
/// writes them back with `write`: how many rows it filled.
async fn in_jadpo(
    transaction: &mut dyn Transaction,
    backfills: &dyn Backfills,
    migration: &str,
    field: &str,
    rows: &[crate::Row],
    write: &str,
) -> Result<u64, MigrateError> {
    if rows.is_empty() {
        return Ok(0);
    }
    let fault = |what: String| MigrateError::Store(StoreError::Fault(what));
    let mut keys = Vec::with_capacity(rows.len());
    let mut reads: Vec<&[SqlValue]> = Vec::with_capacity(rows.len());
    for row in rows {
        match row.split_first() {
            Some((SqlValue::Uuid(key), rest)) => {
                keys.push(SqlValue::Uuid(*key));
                reads.push(rest);
            }
            _ => {
                return Err(fault(format!(
                    "a batch of `{field}` gave a row that doesn't start with its key"
                )));
            }
        }
    }
    let values =
        backfills
            .work_out(migration, field, &reads)
            .map_err(|unworked| match unworked {
                Unworked::Unavailable => MigrateError::Unworked {
                    name: migration.to_string(),
                    field: field.to_string(),
                },
                Unworked::Stopped { at, reason } => MigrateError::BackfillStopped {
                    name: migration.to_string(),
                    field: field.to_string(),
                    record: match keys.get(at) {
                        Some(SqlValue::Uuid(key)) => crate::read::uuid_text(key),
                        _ => String::new(),
                    },
                    reason,
                },
            })?;
    if values.len() != keys.len() {
        return Err(fault(format!(
            "`{field}`'s backfill gave {} values for {} rows",
            values.len(),
            keys.len()
        )));
    }
    let ty = match values.first() {
        Some(SqlValue::Null(_)) | None => {
            return Err(fault(format!("`{field}`'s backfill gave no value")));
        }
        Some(value) => value.ty(),
    };
    let filled = transaction
        .execute(
            write,
            &[
                SqlValue::Array(SqlType::Uuid, keys),
                SqlValue::Array(ty, values),
            ],
        )
        .await?;
    Ok(filled)
}

/// Records `migration`, filled in batches, as applied, and forgets how
/// far it got.
async fn finish(
    transaction: &mut dyn Transaction,
    migration: &Migration,
) -> Result<(), StoreError> {
    record(transaction, migration).await?;
    transaction
        .execute(
            "DELETE FROM jadpo_migration_steps WHERE name = $1",
            &[SqlValue::Text(migration.name.clone())],
        )
        .await?;
    Ok(())
}

/// Records that `migration` is at step `step`, its batches past `after`.
async fn progress(
    transaction: &mut dyn Transaction,
    migration: &Migration,
    step: usize,
    after: Option<[u8; 16]>,
) -> Result<(), StoreError> {
    transaction
        .execute(
            "INSERT INTO jadpo_migration_steps (name, digest, step, after) VALUES ($1, $2, $3, $4) ON CONFLICT (name) DO UPDATE SET step = EXCLUDED.step, after = EXCLUDED.after",
            &[
                SqlValue::Text(migration.name.clone()),
                SqlValue::Text(migration.digest.clone()),
                SqlValue::Int4(i32::try_from(step).unwrap_or(i32::MAX)),
                after.map_or(SqlValue::Null(jadpo_std::SqlType::Uuid), SqlValue::Uuid),
            ],
        )
        .await?;
    Ok(())
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
/// (`pg_available_extensions`), which the role applying migrations may
/// create ([`may_create`]).
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
            may_create(transaction, &extension).await?;
        }
    }
    Ok(standing)
}

/// Whether the role may create `extension`, where the database hasn't it
/// yet: created in a savepoint and undone, since whether a role may
/// create an extension that isn't trusted (PostGIS) depends on the server
/// and the provider's roles, so trying it is the one sure test. A role
/// that can't is refused before anything runs (area 20, "A store with no
/// PostGIS"; chunk 5.5u4).
async fn may_create(
    transaction: &mut dyn Transaction,
    extension: &str,
) -> Result<(), MigrateError> {
    let installed = transaction
        .query(
            "SELECT 1 FROM pg_extension WHERE extname = $1",
            &[SqlValue::Text(extension.to_string())],
        )
        .await?;
    if !installed.is_empty() {
        return Ok(());
    }
    transaction.savepoint().await?;
    let created = transaction
        .batch(&format!(
            "CREATE EXTENSION IF NOT EXISTS \"{}\"",
            extension.replace('"', "\"\"")
        ))
        .await;
    transaction.rollback_to_savepoint().await?;
    match created {
        Ok(()) => Ok(()),
        Err(StoreError::Unavailable) => Err(MigrateError::Store(StoreError::Unavailable)),
        Err(_) => Err(MigrateError::ExtensionNotCreatable {
            extension: extension.to_string(),
        }),
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
