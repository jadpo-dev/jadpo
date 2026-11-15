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
        // A call with a caller runs as one signed in by magic link with
        // one factor, as `as:` does unless it says otherwise, so a role
        // that needs another way in doesn't count (area 9, "Roles that
        // need a way in"; chunk A10).
        Some(Caller {
            value: value.clone(),
            key: format!("{} {id}", kind.name()),
            session: None,
            sign_in: (kind == PrincipalKind::User)
                .then(|| jadpo_std::SignInMethod::MagicLink.name().to_string()),
            multi_factor: false,
            scopes: None,
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
    // A narrowed token needs one of the scopes the gate asks for, or with
    // none listed is refused, before its callers are asked (chunk N2).
    if let (Some(held), Some(wanted)) = (ctx.narrowed(), &gate.scopes)
        && !wanted.iter().any(|scope| held.contains(scope))
    {
        return Stored::Failed(ctx.fail(not_permitted(), site));
    }
    let admitted = match &gate.callers {
        _ if ctx.system().is_some() => gate.system,
        Callers::Anyone => true,
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

/// Whether the caller may do what `grants` decide to `record`, one record
/// of an entity read whole (area 6, "Reading and writing entities in
/// Redis"; chunk Q10): what a statement's compiled policy asks of a row,
/// asked in Jadpo's own code once the record is read. Any grant lets the
/// caller in; a narrowed token needs the effect's scope-token too. A call
/// a test makes without a caller isn't checked (area 13). An entity in
/// Redis is held to it on every read and write, and a record cache on
/// every read (area 9, "Caches and personal data").
pub fn permits(ctx: &Ctx<'_>, grants: &jadpo_std::policy::RecordGrants, record: &Value) -> bool {
    use jadpo_std::policy::{Narrowed, RecordGrant};
    if ctx.unpoliced() {
        return true;
    }
    if let Some(held) = ctx.narrowed() {
        match &grants.narrowed {
            Narrowed::Untouched => {}
            Narrowed::Needs(token) if held.iter().any(|scope| scope == token) => {}
            Narrowed::Needs(_) | Narrowed::Refused => return false,
        }
    }
    grants.grants.iter().any(|grant| match grant {
        RecordGrant::Anyone => true,
        RecordGrant::Signed => ctx.caller().is_some(),
        RecordGrant::System => ctx.system().is_some(),
        RecordGrant::Caller { field, part } => {
            let held = match record {
                Value::Record(record) => record.get(field),
                _ => None,
            };
            let caller = ctx.caller_param(*part);
            match (held, caller) {
                (Some(Value::Uuid(held)), SqlValue::Uuid(bytes)) => {
                    crate::read::uuid_text(&bytes) == held.as_ref()
                }
                _ => false,
            }
        }
    })
}

/// Whether `grants` decide the same for every record: none tests a
/// record's field, so a count needn't read each one.
pub fn same_for_every_record(grants: &jadpo_std::policy::RecordGrants) -> bool {
    !grants
        .grants
        .iter()
        .any(|grant| matches!(grant, jadpo_std::policy::RecordGrant::Caller { .. }))
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
