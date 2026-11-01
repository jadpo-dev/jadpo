//! Writes (area 6, "The operations every entity gets" and "When a write is
//! refused"; docs/compiler-runtime.md §2.5: `Ctx` `write` with a plan and
//! parameters), run the same way for both executors from the plans
//! `jadpo-plan` decides ([`jadpo_std::write`]).
//!
//! A write joins the action's transaction, begun at the first store
//! operation. A row another transaction changed since this one's snapshot
//! is a concurrent change, so the boundary runs the action again (area 6,
//! "All-or-nothing actions and concurrent changes"). A unique rule or a
//! reference the store refuses comes back as which of the plan's
//! constraints it was, which [`crate::store`] turns into what the plan
//! says it raises; a write by `id` that finds no row is the entity's
//! not-found failure. A caller that handles the failure carries on in a
//! transaction the refusal didn't end: the action it called ran inside a
//! savepoint ([`crate::action`]).

use jadpo_std::read::Field;
use jadpo_std::write::{FileCheck, PolicyRefusal, WriteParam, WritePlan, WriteSource};
use jadpo_value::Value;

use crate::read::{Interrupt, JsonCodecs, defect, refused, stored_value, to_sql, uuid_text};
use crate::{Ctx, Failure, FaultKind, SqlType, SqlValue, StoreError};

/// What a write did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Written {
    /// The record it created or changed; `Unit` for a delete.
    Value(Value),
    /// A write by `id` found no record: the entity's not-found failure,
    /// which its plan names. With `true`, the record is there and policy
    /// refused the caller, which the failure's log says (area 9).
    Missing(bool),
    /// Policy refused the caller a create, a change to a record it loaded
    /// earlier, or a change to a field whose own `policy:` refuses them:
    /// `not_permitted` (area 9, "When policy says no").
    NotPermitted,
    /// The store refused it with the plan's constraint at `at`: what the
    /// plan says that raises, keeping `cause`, the store's refusal, as its
    /// internal cause (area 4).
    Refused { at: u32, cause: Box<Failure> },
}

/// Runs `plan` with `inputs`, the values its site worked out, called from
/// site `site` of the frame on top of `ctx`.
pub(crate) async fn write(
    ctx: &mut Ctx<'_>,
    plan: &WritePlan,
    json: JsonCodecs<'_>,
    inputs: &[Value],
    site: u32,
) -> Result<Written, Interrupt> {
    let now = ctx.now();
    // A `File` that doesn't fit its field stops the write before anything
    // is sent (area 16, "What a write checks"; chunk B1).
    for check in &plan.files {
        let file = source_value(ctx, &check.source, inputs, site, now)?;
        // Only the runtime makes a `File`, so anything else is Jadpo's
        // mistake, refused rather than written unchecked.
        if !matches!(file, Value::None | Value::Record(_)) {
            return Err(defect(
                ctx,
                "a `File` field given a value that isn't a file",
                site,
            ));
        }
        if let Some((why, next)) = refused_file(check, &file) {
            return Err(Interrupt::Fault(
                ctx.fault(FaultKind::FileRefused { why, next }, site),
            ));
        }
    }
    let mut params = params_of(ctx, &plan.params, json, inputs, site, now)?;
    // Open the write transaction before consulting checked keys: a nested
    // rollback may have ended the transaction that established them.
    if let Err(error) = ctx.transaction(&plan.store).await {
        return Err(refused(ctx, error, site));
    }
    // An update or a delete may leave a stored file held by none, whose
    // bytes its commit sets out to delete (chunk B3).
    if plan.op != jadpo_std::write::WriteOp::Create
        && let Some(boundary) = ctx.boundary_mut()
    {
        boundary.may_release();
    }
    let versioned = if let Some(row) = plan.row.as_ref().filter(|_| ctx.loaded_before_call()) {
        let id = source_value(ctx, &row.id, inputs, site, now)?;
        let id = to_sql(&id, SqlType::Uuid).map_err(|what| defect(ctx, &what, site))?;
        let expected = ctx.expected_row(row, &id);
        let Some(param) = params.get_mut(row.version as usize) else {
            return Err(defect(
                ctx,
                "a write's row version parameter is missing",
                site,
            ));
        };
        *param = expected
            .as_ref()
            .map_or(SqlValue::Null(SqlType::Text), |v| SqlValue::Text(v.clone()));
        expected.map(|_| (row, id))
    } else {
        None
    };
    // An audited record as it was, read and locked before the write
    // changes it (area 16, "Audit trail"; chunk B11).
    let before = match plan
        .audit
        .as_ref()
        .and_then(|audit| audit.before.as_ref().map(|b| (audit, b)))
    {
        Some((audit, before)) => {
            let id = source_value(ctx, &before.id, inputs, site, now)?;
            crate::audit::before(ctx, audit, &plan.store, id, json, site).await?
        }
        None => None,
    };
    let unpoliced = ctx.unpoliced();
    let sql = match (&plan.unpoliced, unpoliced) {
        (Some(sql), true) => sql,
        _ => &plan.sql,
    };
    // A refusal ends a PostgreSQL transaction; a caller that handles the
    // failure it becomes returns to the savepoint taken before the action
    // it called (`crate::action`), which undoes the refusal with the rest.
    let result = match ctx.transaction(&plan.store).await {
        Ok(transaction) => match &plan.returns {
            Some(_) => transaction.query(sql, &params).await,
            None => transaction
                .execute(sql, &params)
                .await
                .map(|changed| (0..changed).map(|_| Vec::new()).collect()),
        },
        Err(error) => return Err(refused(ctx, error, site)),
    };
    let rows = match result {
        Ok(rows) => rows,
        Err(StoreError::Conflict { constraint }) => {
            return match plan
                .refusals
                .iter()
                .position(|refusal| refusal.constraint == constraint)
            {
                Some(at) => {
                    let mut cause =
                        Failure::new("runtime refused", "ConstraintRefused", "constraint_refused");
                    cause.kind = Some("conflict".to_string());
                    cause.status = Some(409);
                    cause
                        .internal
                        .push(("constraint".to_string(), Value::text(&constraint)));
                    let cause = ctx.fail(cause, site);
                    Ok(Written::Refused {
                        at: at as u32,
                        cause,
                    })
                }
                None => Err(refused(ctx, StoreError::Conflict { constraint }, site)),
            };
        }
        Err(error) => return Err(refused(ctx, error, site)),
    };
    let Some(row) = rows.first() else {
        // Preserve the precheck's refusal priority. Only a refused write
        // needs a separate probe; a successful mutation checked xmin and
        // acquired its row lock in the mutation itself.
        if let Some((row, id)) = &versioned
            && let Err(error) = ctx.check_row(&plan.store, row, id.clone()).await
        {
            return Err(refused(ctx, error, site));
        }
        // A transition whose `when:` can refuse a visible record asks
        // whether the record is there: then the condition refused it.
        if let Some(guard) = &plan.guard {
            let params = params_of(ctx, &guard.params, json, inputs, site, now)?;
            let probe = match (&guard.unpoliced, unpoliced) {
                (Some(sql), true) => sql,
                _ => &guard.sql,
            };
            let found = match ctx.transaction(&plan.store).await {
                Ok(transaction) => transaction.query(probe, &params).await,
                Err(error) => Err(error),
            };
            match found {
                Ok(rows) if !rows.is_empty() => {
                    let mut cause =
                        Failure::new("runtime refused", "TransitionRefused", "transition_refused");
                    cause.kind = Some("conflict".to_string());
                    cause.status = Some(409);
                    let at = guard.refusal;
                    if let Some(refusal) = plan.refusals.get(at as usize) {
                        cause
                            .internal
                            .push(("rule".to_string(), Value::text(&refusal.constraint)));
                    }
                    let cause = ctx.fail(cause, site);
                    return Ok(Written::Refused { at, cause });
                }
                Ok(_) => {}
                Err(error) => return Err(refused(ctx, error, site)),
            }
        }
        // Whether policy refused it: a create it refused writes no row,
        // and an update or delete asks whether the record is there.
        let policed = match (&plan.policy, unpoliced) {
            (Some(PolicyRefusal::Create), false) => return Ok(Written::NotPermitted),
            (
                Some(PolicyRefusal::Probe {
                    sql,
                    params: asked,
                    fields,
                }),
                false,
            ) => {
                let params = params_of(ctx, asked, json, inputs, site, now)?;
                let found = match ctx.transaction(&plan.store).await {
                    Ok(transaction) => transaction.query(sql, &params).await,
                    Err(error) => Err(error),
                };
                let found = match found {
                    Ok(rows) => !rows.is_empty(),
                    Err(error) => return Err(refused(ctx, error, site)),
                };
                // The caller may change the record but not a field the
                // request changes: the whole write is refused (area 9).
                if found && let Some(fields) = fields {
                    let params = params_of(ctx, &fields.params, json, inputs, site, now)?;
                    let granted = match ctx.transaction(&plan.store).await {
                        Ok(transaction) => transaction.query(&fields.sql, &params).await,
                        Err(error) => Err(error),
                    };
                    match granted {
                        Ok(rows) if !rows.is_empty() => return Ok(Written::NotPermitted),
                        Ok(_) => {}
                        Err(error) => return Err(refused(ctx, error, site)),
                    }
                }
                found
            }
            // Without policy, a reference into another tenant still
            // refuses it (area 9): a create, or a record that is there.
            (Some(PolicyRefusal::Create), true) if plan.kept_in_tenant => {
                return Ok(Written::NotPermitted);
            }
            (
                Some(PolicyRefusal::Probe {
                    sql, params: asked, ..
                }),
                true,
            ) if plan.kept_in_tenant => {
                let params = params_of(ctx, asked, json, inputs, site, now)?;
                let found = match ctx.transaction(&plan.store).await {
                    Ok(transaction) => transaction.query(sql, &params).await,
                    Err(error) => Err(error),
                };
                match found {
                    Ok(rows) if !rows.is_empty() => return Ok(Written::NotPermitted),
                    Ok(_) => false,
                    Err(error) => return Err(refused(ctx, error, site)),
                }
            }
            _ => false,
        };
        if plan.by_id {
            return Ok(Written::Missing(policed));
        }
        if policed {
            return Ok(Written::NotPermitted);
        }
        // A loaded record changed by another transaction is a concurrent
        // change; one this transaction can't see at all was deleted by
        // this action, or loaded by another before this one began.
        return Err(Interrupt::Fault(ctx.fault(
            FaultKind::Store(format!(
                "the record this {} names was deleted after it was loaded",
                plan.op.word()
            )),
            site,
        )));
    };
    if let Some((row, id)) = versioned {
        ctx.checked_row(row, id);
    }
    let Some(returns) = &plan.returns else {
        if let Some(audit) = &plan.audit {
            let Some(before) = &before else {
                return Err(defect(
                    ctx,
                    "an audited delete of a record it didn't read",
                    site,
                ));
            };
            // A lifecycle's `delete` keeps the record, as its transition
            // left it.
            let after = match &audit.after {
                Some(after) => {
                    let id = source_value(ctx, &after.id, inputs, site, now)?;
                    crate::audit::read_record(ctx, after, &plan.store, id, json, site).await?
                }
                None => None,
            };
            crate::audit::record(
                ctx,
                audit,
                &plan.store,
                Some(before),
                after.as_ref(),
                json,
                site,
            )
            .await?;
        }
        return Ok(Written::Value(Value::Unit));
    };
    let mut fields = Vec::with_capacity(returns.record.fields.len());
    let started = ctx.started();
    let ring = started.field_ring();
    for (name, field) in &returns.record.fields {
        let Field::Column(at) = field else {
            return Err(defect(ctx, "a written record with an include", site));
        };
        match stored_value(row, &returns.columns, &returns.held, *at, json, ring) {
            Ok(value) => fields.push((std::rc::Rc::from(name.as_str()), value)),
            Err(what) => return Err(defect(ctx, &what, site)),
        }
    }
    let written = Value::record(&returns.record.ty, fields);
    if let Some(audit) = &plan.audit {
        if plan.op != jadpo_std::write::WriteOp::Create && before.is_none() {
            return Err(defect(
                ctx,
                "an audited update of a record it didn't read",
                site,
            ));
        }
        crate::audit::record(
            ctx,
            audit,
            &plan.store,
            before.as_ref(),
            Some(&written),
            json,
            site,
        )
        .await?;
    }
    Ok(Written::Value(written))
}

/// Why `file`, the value a write sets `check`'s field to, isn't one the
/// field keeps, with what to do about it; `None` when it fits, or is
/// `none` (area 16, "What a write checks"; chunk B1). The store comes
/// first, then the type, then the size: a file in another store would be
/// read from the wrong one whatever its type and size.
pub fn refused_file(check: &FileCheck, file: &Value) -> Option<(String, String)> {
    let Value::Record(record) = file else {
        return None;
    };
    let field = &check.field;
    let store = match record.get("store") {
        Some(Value::Text(store)) => store.to_string(),
        _ => String::new(),
    };
    if store != check.store {
        return Some((
            format!(
                "`{field}` keeps its files in `{}`, and this file is in `{store}`",
                check.store
            ),
            format!(
                "Put the file with `{}.put`, the store `{field}` names, or name `{store}` in its `store:`",
                check.store
            ),
        ));
    }
    let file_type = match record.get("type") {
        Some(Value::Variant(variant)) => variant.name.to_string(),
        _ => String::new(),
    };
    if !check.types.is_empty() && !check.types.contains(&file_type) {
        let takes: Vec<String> = check
            .types
            .iter()
            .map(|t| format!("`FileType.{t}`"))
            .collect();
        let takes = match takes.split_last() {
            Some((last, [])) => last.clone(),
            Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
            None => String::new(),
        };
        return Some((
            format!("`{field}` takes {takes}, and this file is `FileType.{file_type}`"),
            format!(
                "Put the bytes as a type `{field}`'s `types:` lists, or add `FileType.{file_type}` to it"
            ),
        ));
    }
    let size = match record.get("size") {
        Some(Value::Int(size)) => u64::try_from(*size).unwrap_or(u64::MAX),
        _ => u64::MAX,
    };
    if size > check.max_size {
        return Some((
            format!(
                "`{field}` holds files of at most {}, and this one is {}",
                jadpo_std::files::shown_size(check.max_size),
                jadpo_std::files::shown_size(size)
            ),
            format!(
                "Bound the bytes before they are put, such as by the `max_length:` of the text they are made from, or raise `{field}`'s `max_size:`"
            ),
        ));
    }
    None
}

/// The values of a statement's parameters, from the write's inputs.
fn params_of(
    ctx: &mut Ctx<'_>,
    written: &[WriteParam],
    json: JsonCodecs<'_>,
    inputs: &[Value],
    site: u32,
    now: i64,
) -> Result<Vec<SqlValue>, Interrupt> {
    let mut params = Vec::with_capacity(written.len());
    // A new record's one identity: its `id`, and what its sealed fields are
    // bound to (chunk P9).
    let mut new_id: Option<Value> = None;
    for param in written {
        if param.source == WriteSource::NewId {
            let id = new_identity(ctx, &mut new_id, site)?;
            match to_sql(&id, param.ty) {
                Ok(sql) => params.push(sql),
                Err(what) => return Err(defect(ctx, &what, site)),
            }
            continue;
        }
        // A sealed field's value, sealed for its record (area 15, "Encrypted
        // fields"; chunk P9).
        if let WriteSource::Sealed {
            of,
            form,
            bound,
            id,
        } = &param.source
        {
            let value = source_value(ctx, of, inputs, site, now)?;
            let id = if **id == WriteSource::NewId {
                new_identity(ctx, &mut new_id, site)?
            } else {
                source_value(ctx, id, inputs, site, now)?
            };
            let started = ctx.started();
            let sealed = crate::sealed::seal(
                &value,
                form,
                (bound, &id),
                started.field_ring(),
                json,
                &mut |bytes: &mut [u8]| ctx.secret_random_bytes(bytes),
            );
            match sealed {
                Ok(sql) => params.push(sql),
                Err(what) => return Err(defect(ctx, &what, site)),
            }
            continue;
        }
        if let WriteSource::Caller(part) = &param.source {
            params.push(ctx.caller_param(*part));
            continue;
        }
        // A literal or an enum value, as the storage codec bound it when
        // the plan was made (chunk 5.5w3b).
        if let WriteSource::Stored(value) = &param.source {
            params.push(value.clone());
            continue;
        }
        // A JSON-held value, written as its contract's JSON (chunk
        // 5.5w2b).
        if let WriteSource::Json { of, codec } = &param.source {
            let value = source_value(ctx, of, inputs, site, now)?;
            match json.write(*codec, &value) {
                Ok(sql) => params.push(sql),
                Err(what) => return Err(defect(ctx, &what, site)),
            }
            continue;
        }
        let value = source_value(ctx, &param.source, inputs, site, now)?;
        match to_sql(&value, param.ty) {
            Ok(sql) => params.push(sql),
            Err(what) => return Err(defect(ctx, &what, site)),
        }
    }
    Ok(params)
}

/// The identity a new record is given, drawn once however often it is read.
fn new_identity(
    ctx: &mut Ctx<'_>,
    held: &mut Option<Value>,
    site: u32,
) -> Result<Value, Interrupt> {
    if let Some(id) = held {
        return Ok(id.clone());
    }
    let id = source_value(ctx, &WriteSource::NewId, &[], site, 0)?;
    *held = Some(id.clone());
    Ok(id)
}

/// The value a write's parameter is sent, from where its source says.
fn source_value(
    ctx: &mut Ctx<'_>,
    source: &WriteSource,
    inputs: &[Value],
    site: u32,
    now: i64,
) -> Result<Value, Interrupt> {
    Ok(match source {
        WriteSource::Input(at) => match inputs.get(*at as usize) {
            Some(value) => value.clone(),
            None => return Err(defect(ctx, "a write without one of its inputs", site)),
        },
        WriteSource::Field { input, field } => match inputs.get(*input as usize) {
            Some(Value::Record(record)) => record.get(field).cloned().unwrap_or(Value::None),
            _ => return Err(defect(ctx, "a write without the record it names", site)),
        },
        WriteSource::Supplied { input, field } => match inputs.get(*input as usize) {
            Some(Value::Record(record)) => Value::Bool(record.supplied(field)),
            _ => return Err(defect(ctx, "a patch that isn't a record", site)),
        },
        WriteSource::AnySupplied { input, fields } => match inputs.get(*input as usize) {
            Some(Value::Record(record)) => {
                Value::Bool(fields.iter().any(|field| record.supplied(field)))
            }
            _ => return Err(defect(ctx, "a patch that isn't a record", site)),
        },
        WriteSource::NewId => match ctx.new_id() {
            Some(id) => Value::uuid(&uuid_text(&id)),
            None => {
                return Err(defect(
                    ctx,
                    "a new identity on a host with no randomness and no seed",
                    site,
                ));
            }
        },
        WriteSource::Now => Value::Instant(now),
        // Answered above, as the caller's own parameter.
        WriteSource::Caller(_) => {
            return Err(defect(ctx, "a caller's part read as a value", site));
        }
        // Answered above, as its JSON.
        WriteSource::Json { .. } => {
            return Err(defect(ctx, "a JSON-held value read as a part", site));
        }
        // Answered above, bound already.
        WriteSource::Stored(_) => {
            return Err(defect(ctx, "a bound literal read as a part", site));
        }
        // Answered above, as its sealed bytes.
        WriteSource::Sealed { .. } => {
            return Err(defect(ctx, "a sealed value read as a part", site));
        }
        // One of the columns a compound standard type is stored in (area
        // 19).
        WriteSource::Part { of, index } => match source_value(ctx, of, inputs, site, now)? {
            Value::None => Value::None,
            value => match jadpo_value::standard::to_parts(&value)
                .and_then(|parts| parts.get(*index as usize).cloned())
            {
                Some(part) => part,
                None => return Err(defect(ctx, "a stored value without that part", site)),
            },
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(types: &[&str]) -> FileCheck {
        FileCheck {
            field: "PriceSheet.sheet".to_string(),
            source: WriteSource::Input(0),
            store: "sheets".to_string(),
            max_size: 1_000,
            types: types.iter().map(|t| t.to_string()).collect(),
        }
    }

    fn file(store: &str, size: i64, file_type: &str) -> Value {
        crate::files::file_value(
            store,
            &jadpo_std::files::key_for(&[7; 16]),
            None,
            size,
            file_type,
        )
    }

    #[test]
    fn a_file_is_held_to_its_fields_store_then_type_then_size() {
        let csv = check(&["csv"]);
        assert_eq!(refused_file(&csv, &file("sheets", 1_000, "csv")), None);
        assert_eq!(refused_file(&csv, &Value::None), None);
        let (why, _) = refused_file(&csv, &file("notes", 2_000, "pdf")).expect("refused");
        assert_eq!(
            why,
            "`PriceSheet.sheet` keeps its files in `sheets`, and this file is in `notes`"
        );
        let (why, _) = refused_file(&csv, &file("sheets", 2_000, "pdf")).expect("refused");
        assert_eq!(
            why,
            "`PriceSheet.sheet` takes `FileType.csv`, and this file is `FileType.pdf`"
        );
        let images = check(&["png", "jpeg", "webp"]);
        let (why, _) = refused_file(&images, &file("sheets", 10, "pdf")).expect("refused");
        assert_eq!(
            why,
            "`PriceSheet.sheet` takes `FileType.png`, `FileType.jpeg` or `FileType.webp`, and this file is `FileType.pdf`"
        );
        let (why, _) = refused_file(&csv, &file("sheets", 1_001, "csv")).expect("refused");
        assert_eq!(
            why,
            "`PriceSheet.sheet` holds files of at most 1 kB, and this one is 1001 bytes"
        );
        // A field that names no types takes any.
        assert_eq!(refused_file(&check(&[]), &file("sheets", 10, "pdf")), None);
    }
}
