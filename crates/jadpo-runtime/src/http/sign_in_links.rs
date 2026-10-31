//! First sign-ins and their links (area 8, "A person the app has not
//! seen"; chunk A7): what decides an external token whose subject no user
//! has, once `on_first_sign_in:` is declared.
//!
//! - **One read finds the user**, by the subject field or through the
//!   link a first sign-in made ([`jadpo_std::http::FirstSignInPlan::lookup`]),
//!   so a linked user's request costs what a provisioned one's does.
//! - **The action runs once per subject.** A first sign-in takes a
//!   transaction-scoped advisory lock keyed by the way in and the subject,
//!   at read committed, and reads again under it, so requests that arrive
//!   together wait for the first and sign in through the link it made,
//!   rather than each running the action.
//! - **The action runs as the system**, reaching no tenant's records, in
//!   a transaction of its own, given a `FirstSignIn` holding the token's
//!   subject and nothing else of it: no claim becomes the user's identity
//!   or a grant. Any failure it declares is `sign_in_refused`, the real
//!   one kept for the log and `SignInRefused`; `unavailable` is
//!   `authentication_unavailable`.
//! - **The link is kept only for a user the action gave that the
//!   lifecycle shows**, active or not, committed with `SignedIn`, so a user
//!   the action made inactive is refused as disabled until they are made
//!   active, and the action isn't asked again.

use std::rc::Rc;

use jadpo_std::http::{FIRST_SIGN_IN, FirstSignInPlan};
use jadpo_std::{SignInMethod, SignInRefusal};
use jadpo_value::Value;

use super::identity::{AUTHENTICATION_UNAVAILABLE, Refused};
use super::magic_links::REFUSED;
use super::sign_in_events::{self, SignInEvent, Way};
use crate::ctx::{SignInRefused, System};
use crate::{Ctx, Executor, Isolation, Outcome, SqlValue};

/// Waits for any other first sign-in of the same way in and subject:
/// `$1` the lock's key, [`lock_key`]'s.
pub(crate) const LOCK: &str = "SELECT 1 FROM pg_advisory_xact_lock($1)";

/// Keeps a first sign-in's link: `$1` the way in, `$2` the subject, `$3`
/// the user and `$4` when.
pub(crate) const LINK: &str = "INSERT INTO \"jadpo_sign_in_links\" (\"method\", \"subject\", \"user_id\", \"linked_at\") VALUES ($1, $2, $3, $4) ON CONFLICT (\"method\", \"subject\") DO NOTHING";

/// The advisory lock a first sign-in of `method` and `subject` holds: the
/// first eight bytes of their BLAKE3 digest, so two subjects share a lock
/// only by a collision, which makes one wait, never sign in as the other.
fn lock_key(method: &str, subject: &str) -> i64 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(method.as_bytes());
    hasher.update(&[0]);
    hasher.update(subject.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest.as_bytes()[..8]);
    i64::from_be_bytes(bytes)
}

/// Who a way in vouches for, the first time: its subject, and for a
/// provider the address, whether it verified it and the name its profile
/// gives. Nothing else of a token or a provider reaches the action.
pub(crate) struct Arrival<'a> {
    pub way: Way,
    pub subject: &'a str,
    pub address: Option<&'a str>,
    pub address_verified: bool,
    pub name: Option<&'a str>,
}

impl Arrival<'_> {
    /// The `FirstSignIn` `on_first_sign_in:`'s action is given.
    fn value(&self) -> Value {
        let text = |value: Option<&str>| value.map_or(Value::None, Value::text);
        Value::record(
            FIRST_SIGN_IN,
            vec![
                (Rc::from("method"), self.way.value()),
                (Rc::from("address"), text(self.address)),
                (
                    Rc::from("address_verified"),
                    Value::Bool(self.address_verified),
                ),
                (Rc::from("subject"), Value::text(self.subject)),
                (Rc::from("name"), text(self.name)),
            ],
        )
    }
}

/// Refuses a first sign-in the application refused, keeping the failure
/// it declared for the log and publishing `SignInRefused`.
async fn refusing(
    ctx: &mut Ctx<'_>,
    executor: &dyn Executor,
    store: &str,
    way: &Way,
    failure: Option<String>,
) -> Refused {
    ctx.refuse_sign_in(SignInRefused {
        reason: SignInRefusal::RefusedByApp.name(),
        failure,
    });
    let event = SignInEvent::Refused {
        method: way.clone(),
        reason: SignInRefusal::RefusedByApp,
        user: None,
    };
    sign_in_events::publish(ctx, executor, store, &[event]).await;
    Refused::new(REFUSED)
}

/// The user a first sign-in of `arrival` gives, as `first.lookup` reads
/// one: `params`, that lookup's, read again under the lock, else
/// `on_first_sign_in:`'s action asked and its user linked. Its row is the
/// user's `id`, whether its `active:` holds and its subject field, and
/// whether the action gave it just now. With `announce`, the link is kept
/// with `SignedIn`, as an external token's first use is; a provider's
/// session start says so itself.
pub(crate) async fn first(
    ctx: &mut Ctx<'_>,
    executor: &dyn Executor,
    store: &str,
    first: &FirstSignInPlan,
    params: &[SqlValue],
    arrival: &Arrival<'_>,
    announce: bool,
) -> Result<(Vec<SqlValue>, bool), Refused> {
    let way = &arrival.way;
    let Some(action) = first.action else {
        return Err(refusing(ctx, executor, store, way, None).await);
    };
    let unavailable = || Refused::new(AUTHENTICATION_UNAVAILABLE);
    let now = SqlValue::Timestamp(ctx.now().saturating_mul(1000));
    let database = ctx.host().database(store).ok_or_else(unavailable)?;
    let mut transaction = database
        .begin_at(Isolation::ReadCommitted)
        .await
        .map_err(|_| unavailable())?;
    let locked = transaction
        .query(
            LOCK,
            &[SqlValue::Int8(lock_key(&way.name, arrival.subject))],
        )
        .await;
    // Another request may have linked them while this one waited.
    let found = match locked {
        Ok(_) => transaction.query(&first.lookup, params).await,
        Err(error) => Err(error),
    };
    match found {
        Ok(rows) if !rows.is_empty() => {
            let _ = transaction.rollback().await;
            return Ok((rows.into_iter().next().unwrap_or_default(), false));
        }
        Ok(_) => {}
        Err(_) => {
            let _ = transaction.rollback().await;
            return Err(unavailable());
        }
    }
    let given = arrival.value();
    // The application's own action, when one is running, waits with its
    // transaction while this one runs in its own, as the system reaching
    // no tenant's records.
    let waiting = ctx.take_boundary();
    let runner = ctx.runner();
    ctx.become_system(System::NO_TENANT);
    let outcome = executor.run(ctx, action, vec![given]).await;
    ctx.restore_runner(runner);
    ctx.give_boundary(waiting);
    let id = match outcome {
        Outcome::Value(id) => crate::to_sql(&id, jadpo_std::read::SqlType::Uuid).ok(),
        Outcome::Failed(failure) if failure.kind.as_deref() == Some("unavailable") => {
            let _ = transaction.rollback().await;
            return Err(unavailable());
        }
        Outcome::Failed(failure) => {
            let _ = transaction.rollback().await;
            return Err(refusing(ctx, executor, store, way, Some(failure.name.clone())).await);
        }
        Outcome::Fault(fault) => {
            let _ = transaction.rollback().await;
            return Err(Refused::fault(fault));
        }
        Outcome::Assertion(_) => {
            let _ = transaction.rollback().await;
            return Err(Refused::new(super::INTERNAL_FAULT));
        }
    };
    // The user the action gave, read as a sign-in reads them: one the
    // lifecycle doesn't show, or none, is refused by the application.
    let row = match id {
        Some(id) => match transaction.query(&first.by_id, &[id, now.clone()]).await {
            Ok(rows) => rows.into_iter().next(),
            Err(_) => {
                let _ = transaction.rollback().await;
                return Err(unavailable());
            }
        },
        None => None,
    };
    let Some(row) = row else {
        let _ = transaction.rollback().await;
        return Err(refusing(ctx, executor, store, way, None).await);
    };
    let Some(SqlValue::Uuid(user)) = row.first().cloned() else {
        let _ = transaction.rollback().await;
        return Err(unavailable());
    };
    let linked = transaction
        .execute(
            LINK,
            &[
                SqlValue::Text(way.name.clone()),
                SqlValue::Text(arrival.subject.to_string()),
                SqlValue::Uuid(user),
                now,
            ],
        )
        .await;
    if linked.is_err() {
        let _ = transaction.rollback().await;
        return Err(unavailable());
    }
    let events = if announce {
        vec![SignInEvent::SignedIn {
            user,
            method: way.clone(),
            first_time: true,
        }]
    } else {
        Vec::new()
    };
    sign_in_events::commit_with(ctx, executor, store, transaction, &events)
        .await
        .map_err(|_| unavailable())?;
    Ok((row, true))
}

/// [`first`] for an external token's subject (chunk A7).
pub(crate) async fn external(
    ctx: &mut Ctx<'_>,
    executor: &dyn Executor,
    store: &str,
    plan: &FirstSignInPlan,
    params: &[SqlValue],
    subject: &str,
) -> Result<Vec<SqlValue>, Refused> {
    let arrival = Arrival {
        way: SignInMethod::ExternalToken.into(),
        subject,
        address: None,
        address_verified: false,
        name: None,
    };
    first(ctx, executor, store, plan, params, &arrival, true)
        .await
        .map(|(row, _)| row)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lock_is_the_way_in_and_the_subject_together() {
        let ann = lock_key("external_token", "ann");
        assert_eq!(ann, lock_key("external_token", "ann"));
        assert_ne!(ann, lock_key("external_token", "bo"));
        assert_ne!(ann, lock_key("google", "ann"));
    }

    #[test]
    fn the_action_is_given_the_subject_and_nothing_a_token_claims() {
        let arrival = Arrival {
            way: SignInMethod::ExternalToken.into(),
            subject: "auth0|ann",
            address: None,
            address_verified: false,
            name: None,
        };
        let Value::Record(given) = arrival.value() else {
            panic!("a record");
        };
        assert_eq!(given.get("subject"), Some(&Value::text("auth0|ann")));
        assert_eq!(given.get("address"), Some(&Value::None));
        assert_eq!(given.get("address_verified"), Some(&Value::Bool(false)));
        assert_eq!(given.get("name"), Some(&Value::None));
    }
}
