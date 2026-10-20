//! Who may call an action that reaches no entity (area 9, "Actions"): its
//! line under root `policy { actions: }`, as the plan's [`Gate`], checked
//! by the action boundary as the action starts, wherever it is called from
//! ([`crate::ActionBody::gate`]). Grants on records are compiled into each
//! statement by the plan, so only this check runs on its own. And who a
//! test's, fixture's or seed's call runs as ([`call_as`]).

use jadpo_std::policy::{Callers, Gate};
use jadpo_std::read::Source;
use jadpo_value::Value;

use crate::read::{Interrupt, not_permitted, refused};
use crate::{Caller, Ctx, SqlValue, Stored};

/// Runs what a test, fixture or seed calls next as the caller it passes,
/// so policy applies as to that caller (area 9): `caller` is the argument
/// the callee's plan names as its caller, the parameter whose type is a
/// `Principal`. With none, or `none` passed for it, the call runs without
/// policy, so it proves behaviour and data, as a route test or a call with
/// a caller proves policy (areas 9 and 13). Nothing a request runs calls
/// this: a request's caller is whoever signed in.
pub fn call_as(ctx: &mut Ctx<'_>, caller: Option<&Value>) {
    use jadpo_std::http::PrincipalKind;
    let caller = caller.and_then(|value| {
        let Value::Record(record) = value else {
            return None;
        };
        let kind = [PrincipalKind::User, PrincipalKind::Service]
            .into_iter()
            .find(|kind| *record.ty == *kind.type_name())?;
        let id = match record.get("id") {
            Some(Value::Uuid(id)) => id.to_string(),
            _ => String::new(),
        };
        Some(Caller {
            value: value.clone(),
            key: format!("{} {id}", kind.name()),
            session: None,
        })
    });
    match caller {
        Some(caller) => ctx.set_caller(Some(caller)),
        None => ctx.set_unpoliced(),
    }
}

/// Whether the caller may call the action `gate` guards, from site `site`:
/// `Stored::Value(Unit)` when they may, `not_permitted` when they may not.
/// The system may when the action's line names `Access.system`, and only
/// then (chunk 6.1a). A call a test makes without a caller isn't checked
/// (area 13).
pub async fn admit(ctx: &mut Ctx<'_>, gate: &Gate, site: u32) -> Stored {
    if ctx.unpoliced() {
        return Stored::Value(Value::Unit);
    }
    let admitted = match &gate.callers {
        _ if ctx.system().is_some() => gate.system,
        Callers::Signed => ctx.caller().is_some(),
        Callers::Nobody => false,
        Callers::Roles { store, sql, params } => {
            let values: Vec<SqlValue> = params
                .iter()
                .map(|param| match &param.source {
                    Source::Caller(part) => ctx.caller_param(*part),
                    // A role, as the storage codec bound it (chunk 5.5w3b).
                    Source::Stored(value) => value.clone(),
                    _ => SqlValue::Timestamp(ctx.now().saturating_mul(1000)),
                })
                .collect();
            let found = match ctx.read_transaction(store).await {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Env, MemoryHost, Places};

    #[test]
    fn a_call_runs_as_the_caller_its_plan_names_or_without_policy() {
        let places = Places::new();
        let host = MemoryHost::new();
        let mut ctx = Ctx::new(&places, &host, Env::default());
        let id = "0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e";
        let bob = crate::http::identity::principal(
            jadpo_std::http::PrincipalKind::User,
            Value::uuid(id),
            Value::text("bob"),
        );
        call_as(&mut ctx, Some(&bob));
        assert_eq!(
            ctx.caller().map(|caller| caller.key.as_str()),
            Some(format!("user {id}").as_str())
        );
        assert!(!ctx.unpoliced());
        for passed in [None, Some(&Value::None), Some(&Value::text("bob"))] {
            call_as(&mut ctx, passed);
            assert!(ctx.caller().is_none() && ctx.unpoliced(), "{passed:?}");
        }
    }
}
