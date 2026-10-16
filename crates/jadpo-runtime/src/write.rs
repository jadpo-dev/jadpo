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

use crate::read::{Interrupt, defect, from_sql, refused, to_sql, uuid_text};
use crate::{Ctx, Failure, FaultKind, SqlValue, StoreError};

/// What a write did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Written {
    /// The record it created or changed; `Unit` for a delete.
    Value(Value),
    /// A write by `id` found no record: the entity's not-found failure,
    /// which its plan names. With `true`, the record is there and policy
    /// refused the caller, which the failure's log says (area 9).
    Missing(bool),
    /// Policy refused the caller a create, or a change to a record it
    /// loaded earlier: `not_permitted` (area 9, "When policy says no").
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
    inputs: &[Value],
    site: u32,
) -> Result<Written, Interrupt> {
    let now = ctx.now();
    let params = params_of(ctx, &plan.params, inputs, site, now)?;
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
        // A transition whose `when:` can refuse a visible record asks
        // whether the record is there: then the condition refused it.
        if let Some(guard) = &plan.guard {
            let params = params_of(ctx, &guard.params, inputs, site, now)?;
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
            (Some(PolicyRefusal::Probe { sql, params: asked }), false) => {
                let params = params_of(ctx, asked, inputs, site, now)?;
                let found = match ctx.transaction(&plan.store).await {
                    Ok(transaction) => transaction.query(sql, &params).await,
                    Err(error) => Err(error),
                };
                match found {
                    Ok(rows) => !rows.is_empty(),
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
    let Some(returns) = &plan.returns else {
        return Ok(Written::Value(Value::Unit));
    };
    let mut fields = Vec::with_capacity(returns.record.fields.len());
    for (name, field) in &returns.record.fields {
        let Field::Column(at) = field else {
            return Err(defect(ctx, "a written record with an include", site));
        };
        let (Some(sql), Some(column)) = (row.get(*at as usize), returns.columns.get(*at as usize))
        else {
            return Err(defect(
                ctx,
                "a returned row without a column it needs",
                site,
            ));
        };
        match from_sql(sql, column) {
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
    inputs: &[Value],
    site: u32,
    now: i64,
) -> Result<Vec<SqlValue>, Interrupt> {
    let mut params = Vec::with_capacity(written.len());
    for param in written {
        let value = match &param.source {
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
            WriteSource::Caller(part) => {
                params.push(ctx.caller_param(*part));
                continue;
            }
        };
        match to_sql(&value, param.ty) {
            Ok(sql) => params.push(sql),
            Err(what) => return Err(defect(ctx, &what, site)),
        }
    }
    Ok(params)
}
