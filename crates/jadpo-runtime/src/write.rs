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
use jadpo_std::write::{PolicyRefusal, WriteParam, WritePlan, WriteSource};
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
    let mut params = params_of(ctx, &plan.params, json, inputs, site, now)?;
    // Open the write transaction before consulting checked keys: a nested
    // rollback may have ended the transaction that established them.
    if let Err(error) = ctx.transaction(&plan.store).await {
        return Err(refused(ctx, error, site));
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
        return Ok(Written::Value(Value::Unit));
    };
    let mut fields = Vec::with_capacity(returns.record.fields.len());
    for (name, field) in &returns.record.fields {
        let Field::Column(at) = field else {
            return Err(defect(ctx, "a written record with an include", site));
        };
        match stored_value(row, &returns.columns, &returns.held, *at, json) {
            Ok(value) => fields.push((std::rc::Rc::from(name.as_str()), value)),
            Err(what) => return Err(defect(ctx, &what, site)),
        }
    }
    Ok(Written::Value(Value::record(&returns.record.ty, fields)))
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
    for param in written {
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
