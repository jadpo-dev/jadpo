//! Sealed fields (area 15, "Encrypted fields"; area 19, "Encrypted fields",
//! "A key is never a value" and "How a secret is kept"; chunk P9): what both
//! executors run for a field kept sealed with the application's
//! `field_encryption_key:`, from the plans `jadpo-plan` decides.
//!
//! A sealed field's `bytea` holds the sealed format `crypto.encrypt` writes
//! (area 19, "The sealed format"), under a key derived once from the setting
//! for this one use, so an application that also calls `crypto.encrypt` with
//! the setting never meets a field's values. Each value is bound to its
//! column and its record's `id`, so it opens for no other field and no other
//! row: moved anywhere, it is `unreadable`. A nonce comes from the host's
//! secret randomness, never kept on a recording's tape nor taken from a
//! replayed one, and a key held as data goes in as these same sealed bytes,
//! so neither a recording nor a replay ever holds one unsealed.
//!
//! [`reseal_all`] is the field key's rotation: the built-in schedule moves
//! each row sealed with `FIELD_ENCRYPTION_KEY_PREVIOUS` to the current key,
//! batch after batch, comparing each row's bytes as it writes so a request
//! that changed one since keeps its change, and [`rotation`] counts how far
//! it has got.

use jadpo_std::read::{SealedForm, sealed_context};
use jadpo_std::write::{RESEAL_BATCH, ResealPlan};
use jadpo_value::Value;
use jadpo_value::crypto::KeyRing;
use jadpo_value::secret::Secret;

use crate::read::{JsonCodecs, uuid_text};
use crate::{Database, Host, SqlType, SqlValue, StoreError};

/// The label the field key is derived under: one setting's key never serves
/// two uses as itself.
const FIELD_KEY: &str = "jadpo field encryption key v1";

/// The ring sealed fields use, derived from `setting`'s and its `_PREVIOUS`.
pub fn field_ring(setting: &KeyRing) -> Result<KeyRing, jadpo_value::Fault> {
    setting.derived(FIELD_KEY)
}

/// `value`, of the form `form`, as its sealed bytes for the record whose
/// `id` is `id`: `NULL` for no value. A fault names what happened, never the
/// value.
pub(crate) fn seal(
    value: &Value,
    form: &SealedForm,
    context: (&str, &Value),
    ring: Option<&KeyRing>,
    json: JsonCodecs<'_>,
    source: &mut jadpo_value::random::Source<'_>,
) -> Result<SqlValue, String> {
    if matches!(value, Value::None) {
        return Ok(SqlValue::Null(SqlType::Bytes));
    }
    let (bound, id) = context;
    let ring = ring.ok_or_else(|| {
        format!("`{bound}` is kept sealed, and `field_encryption_key:` has no value in this run")
    })?;
    let Value::Uuid(id) = id else {
        return Err(format!("`{bound}` sealed for a record with no `id`"));
    };
    let context = sealed_context(bound, id);
    let sealed = match (form, value) {
        (SealedForm::Key, Value::Key(key)) => {
            jadpo_value::crypto::seal_key(key, ring, &context, source)
        }
        (SealedForm::Secret(_), Value::Secret(secret)) => {
            jadpo_value::crypto::encrypt(secret.expose().as_bytes(), ring, &context, source)
        }
        (SealedForm::Json(number), value) => {
            let SqlValue::Json(text) = json.write(*number, value)? else {
                return Err(format!("`{bound}` has no value to seal"));
            };
            jadpo_value::crypto::encrypt(text.as_bytes(), ring, &context, source)
        }
        _ => return Err(format!("`{bound}` given a value that isn't its type")),
    };
    sealed
        .map(SqlValue::Bytes)
        .map_err(|_| format!("`{bound}` couldn't be sealed: the host gave no randomness"))
}

/// What the bytes of a sealed column hold, opened with `ring` for the record
/// whose `id` the row holds: `none` for its `NULL`. Bytes no key in `ring`
/// opens for this column and record, whether altered, moved from another row
/// or sealed with a key this run doesn't have, are a defect naming the
/// column, never the bytes.
pub(crate) fn open(
    stored: &SqlValue,
    form: &SealedForm,
    context: (&str, Option<&SqlValue>),
    ring: Option<&KeyRing>,
    json: JsonCodecs<'_>,
) -> Result<Value, String> {
    let (bound, id) = context;
    let bytes = match stored {
        SqlValue::Null(_) => return Ok(Value::None),
        SqlValue::Bytes(bytes) => bytes,
        other => return Err(format!("a `{:?}` column read as `{bound}`", other.ty())),
    };
    let ring = ring.ok_or_else(|| {
        format!("`{bound}` is kept sealed, and `field_encryption_key:` has no value in this run")
    })?;
    let Some(SqlValue::Uuid(id)) = id else {
        return Err(format!("`{bound}` read from a row with no `id`"));
    };
    let context = sealed_context(bound, &uuid_text(id));
    let unreadable = || {
        format!(
            "a sealed `{bound}` no key in this run opens: altered, moved from another record, or sealed with a key the application no longer has"
        )
    };
    if matches!(form, SealedForm::Key) {
        return jadpo_value::crypto::open_key(bytes, ring, &context)
            .map(Value::Key)
            .map_err(|_| unreadable());
    }
    let opened = jadpo_value::crypto::decrypt(bytes, ring, &context).map_err(|_| unreadable())?;
    let text = std::str::from_utf8(&opened).map_err(|_| unreadable())?;
    match form {
        SealedForm::Secret(kind) => {
            let kind = jadpo_std::standard::security::secret_value(kind)
                .ok_or_else(|| format!("`{bound}` holds a secret of no known type"))?;
            Ok(Value::Secret(Secret::new(kind, text)))
        }
        SealedForm::Json(number) => json.read(*number, text),
        SealedForm::Key => Err(unreadable()),
    }
}

/// What one re-seal run did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Resealed {
    /// How many rows it moved to the current key.
    pub rows: u64,
    /// Whether every row sealed with the previous key was moved: `false`
    /// when it stopped at its budget, leaving the rest to the next run.
    pub finished: bool,
}

/// Moves up to `batches` batches of each of `plans`' rows sealed with the
/// previous key to the current one, sharing the budget among them. With no
/// previous key there is nothing to do. A batch that fails ends the run, the
/// batches before it staying done, and the next run carries on.
pub async fn reseal_all(
    host: &dyn Host,
    plans: &[ResealPlan],
    ring: &KeyRing,
    batches: u32,
) -> Result<Resealed, StoreError> {
    let mut out = Resealed {
        rows: 0,
        finished: true,
    };
    let (_, Some(previous)) = ring
        .identities()
        .map_err(|_| StoreError::Fault("the field key isn't 32 bytes".into()))?
    else {
        return Ok(out);
    };
    let mut left = batches;
    for plan in plans {
        loop {
            if left == 0 {
                out.finished = false;
                return Ok(out);
            }
            left -= 1;
            let (moved, read) = reseal_batch(host, plan, ring, &previous).await?;
            out.rows += moved;
            if read < u64::from(RESEAL_BATCH) {
                break;
            }
        }
    }
    Ok(out)
}

/// One batch of `plan`: how many rows it moved and how many it read.
async fn reseal_batch(
    host: &dyn Host,
    plan: &ResealPlan,
    ring: &KeyRing,
    previous: &[u8],
) -> Result<(u64, u64), StoreError> {
    let database = host
        .database(&plan.store)
        .ok_or_else(|| StoreError::Fault(format!("no connection to the store `{}`", plan.store)))?;
    let rows = database
        .query_once(&plan.select, &[SqlValue::Bytes(previous.to_vec())])
        .await?;
    let read = rows.len() as u64;
    if rows.is_empty() {
        return Ok((0, 0));
    }
    let (mut ids, mut was, mut now) = (Vec::new(), Vec::new(), Vec::new());
    for row in &rows {
        let (Some(SqlValue::Uuid(id)), Some(SqlValue::Bytes(bytes))) = (row.first(), row.get(1))
        else {
            return Err(StoreError::Fault(format!(
                "the re-seal of `{}` read a row it can't move",
                plan.field
            )));
        };
        let context = sealed_context(&plan.bound, &uuid_text(id));
        // What this run's previous key can't open stays as it is: it reads
        // only rows that key's identity is on, and one of those that isn't
        // really its own is a defect a read names.
        let Ok(opened) = jadpo_value::crypto::decrypt(bytes, ring, &context) else {
            continue;
        };
        let sealed =
            jadpo_value::crypto::encrypt(&opened, ring, &context, &mut |out: &mut [u8]| {
                host.random(out)
            })
            .map_err(|_| StoreError::Fault("the host gave no randomness".into()))?;
        ids.push(SqlValue::Uuid(*id));
        was.push(SqlValue::Bytes(bytes.clone()));
        now.push(SqlValue::Bytes(sealed));
    }
    if ids.is_empty() {
        return Ok((0, read));
    }
    let moved = database
        .query_once(
            &plan.update,
            &[
                SqlValue::Array(SqlType::Uuid, ids),
                SqlValue::Array(SqlType::Bytes, was),
                SqlValue::Array(SqlType::Bytes, now),
            ],
        )
        .await?;
    Ok((moved.len() as u64, read))
}

/// How one sealed field's rows stand against the keys a run has (area 15,
/// "Rotating the key"): what tells an operator when `_PREVIOUS` can go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rotation {
    /// `Consultation.notes`.
    pub field: String,
    /// How many rows hold a value.
    pub rows: u64,
    /// How many of them the current key sealed.
    pub current: u64,
    /// How many the previous key sealed, which the re-seal moves.
    pub previous: u64,
    /// How many neither did: no key this run has opens them.
    pub unreadable: u64,
}

/// How `plan`'s rows stand against `ring`, in `database`.
pub async fn standing(
    database: &dyn Database,
    plan: &ResealPlan,
    ring: &KeyRing,
) -> Result<Rotation, StoreError> {
    let (current, previous) = ring
        .identities()
        .map_err(|_| StoreError::Fault("the field key isn't 32 bytes".into()))?;
    let rows = database
        .query_once(
            &plan.count,
            &[
                SqlValue::Bytes(current.to_vec()),
                previous.map_or(SqlValue::Null(SqlType::Bytes), |identity| {
                    SqlValue::Bytes(identity.to_vec())
                }),
            ],
        )
        .await?;
    let count = |at: usize| match rows.first().and_then(|row| row.get(at)) {
        Some(SqlValue::Int8(count)) => u64::try_from(*count).unwrap_or(0),
        _ => 0,
    };
    let (all, on_current, on_previous) = (count(0), count(1), count(2));
    Ok(Rotation {
        field: plan.field.clone(),
        rows: all,
        current: on_current,
        previous: on_previous,
        unreadable: all.saturating_sub(on_current).saturating_sub(on_previous),
    })
}

/// How each of `plans`' rows stand against `ring`.
pub async fn rotation(
    host: &dyn Host,
    plans: &[ResealPlan],
    ring: &KeyRing,
) -> Result<Vec<Rotation>, StoreError> {
    let mut out = Vec::with_capacity(plans.len());
    for plan in plans {
        let database = host.database(&plan.store).ok_or_else(|| {
            StoreError::Fault(format!("no connection to the store `{}`", plan.store))
        })?;
        out.push(standing(database, plan, ring).await?);
    }
    Ok(out)
}
