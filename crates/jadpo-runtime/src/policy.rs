//! Who may call an action that reaches no entity (area 9, "Actions"): its
//! line under root `policy { actions: }`, as the plan's [`Gate`], checked
//! as the action starts, wherever it is called from. Grants on records are
//! compiled into each statement by the plan, so only this check runs on
//! its own.

use jadpo_std::policy::Gate;
use jadpo_std::read::Source;
use jadpo_value::Value;

use crate::read::{Interrupt, not_permitted, refused};
use crate::{Ctx, SqlValue, Stored};

/// Whether the caller may call the action `gate` guards, from site `site`:
/// `Stored::Value(Unit)` when they may, `not_permitted` when they may not.
/// A call a test makes without a caller isn't checked (area 13).
pub async fn admit(ctx: &mut Ctx<'_>, gate: &Gate, site: u32) -> Stored {
    if ctx.unpoliced() {
        return Stored::Value(Value::Unit);
    }
    let admitted = match gate {
        Gate::Signed => ctx.caller().is_some(),
        Gate::Nobody => false,
        Gate::Roles { store, sql, params } => {
            let values: Vec<SqlValue> = params
                .iter()
                .map(|param| match &param.source {
                    Source::Caller(part) => ctx.caller_param(*part),
                    _ => SqlValue::Timestamp(ctx.now().saturating_mul(1000)),
                })
                .collect();
            let found = match ctx.transaction(store).await {
                Ok(transaction) => transaction.query(sql, &values).await,
                Err(error) => Err(error),
            };
            match found {
                Ok(rows) => !rows.is_empty(),
                Err(error) => {
                    return match refused(ctx, error, site) {
                        Interrupt::Failed(failure) => Stored::Failed(failure),
                        Interrupt::Fault(fault) => Stored::Fault(fault),
                    };
                }
            }
        }
    };
    if admitted {
        Stored::Value(Value::Unit)
    } else {
        Stored::Failed(ctx.fail(not_permitted(), site))
    }
}
