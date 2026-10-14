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
//! constraints it was, for the executor to raise what its site maps it
//! to; a write by `id` that finds no row is the entity's not-found
//! failure. A write that can be refused, run by an action another action
//! called, runs inside a savepoint, so a caller that handles the failure
//! carries on in a transaction the refusal didn't end.

use jadpo_std::read::Field;
use jadpo_std::write::{WritePlan, WriteSource};
use jadpo_value::Value;

use crate::read::{Interrupt, defect, from_sql, refused, to_sql, uuid_text};
use crate::{Ctx, Failure, FaultKind, StoreError};

/// What a write did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Written {
    /// The record it created or changed; `Unit` for a delete.
    Value(Value),
    /// A write by `id` found no record: the executor raises the entity's
    /// not-found failure.
    Missing,
    /// The store refused it with the plan's constraint at `at`: the
    /// executor raises what its site maps that to, keeping `cause`, the
    /// store's refusal, as its internal cause (area 4).
    Refused { at: u32, cause: Box<Failure> },
}

/// Where a savepoint around a write that can be refused is kept.
const SAVEPOINT: &str = "\"jadpo_write\"";

/// Runs `plan` with `inputs`, the values its site worked out, called from
/// site `site` of the frame on top of `ctx`.
pub async fn write(
    ctx: &mut Ctx<'_>,
    plan: &WritePlan,
    inputs: &[Value],
    site: u32,
) -> Result<Written, Interrupt> {
    let now = ctx.now();
    let mut params = Vec::with_capacity(plan.params.len());
    for param in &plan.params {
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
            WriteSource::NewId => Value::uuid(&uuid_text(&ctx.new_id())),
            WriteSource::Now => Value::Instant(now),
        };
        match to_sql(&value, param.ty) {
            Ok(sql) => params.push(sql),
            Err(what) => return Err(defect(ctx, &what, site)),
        }
    }
    // A refusal ends a PostgreSQL transaction; a savepoint keeps it going
    // for a caller that handles the failure. Only an action called inside
    // another can have such a caller: the outermost one rolls back.
    let savepoint = !plan.refusals.is_empty() && ctx.nested();
    let result = {
        let transaction = match ctx.transaction(&plan.store).await {
            Ok(transaction) => transaction,
            Err(error) => return Err(refused(ctx, error, site)),
        };
        let mut result = Ok(Vec::new());
        if savepoint && let Err(error) = transaction.batch(&format!("SAVEPOINT {SAVEPOINT}")).await
        {
            result = Err(error);
        }
        if result.is_ok() {
            result = match &plan.returns {
                Some(_) => transaction.query(&plan.sql, &params).await,
                None => transaction
                    .execute(&plan.sql, &params)
                    .await
                    .map(|changed| (0..changed).map(|_| Vec::new()).collect()),
            };
        }
        let settle = match &result {
            Ok(_) => format!("RELEASE SAVEPOINT {SAVEPOINT}"),
            Err(_) => format!("ROLLBACK TO SAVEPOINT {SAVEPOINT}"),
        };
        let mapped = matches!(&result, Err(StoreError::Conflict { constraint })
            if plan.refusals.contains(constraint));
        if savepoint
            && (result.is_ok() || mapped)
            && let Err(error) = transaction.batch(&settle).await
        {
            result = Err(error);
        }
        result
    };
    let rows = match result {
        Ok(rows) => rows,
        Err(StoreError::Conflict { constraint }) => {
            return match plan.refusals.iter().position(|c| *c == constraint) {
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
        if plan.by_id {
            return Ok(Written::Missing);
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
            return Err(defect(ctx, "a returned row without a column it needs", site));
        };
        match from_sql(sql, column) {
            Ok(value) => fields.push((std::rc::Rc::from(name.as_str()), value)),
            Err(what) => return Err(defect(ctx, &what, site)),
        }
    }
    Ok(Written::Value(Value::record(&returns.record.ty, fields)))
}
