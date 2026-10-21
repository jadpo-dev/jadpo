//! The events every sign-in publishes (area 8, "Events"; chunk 7.3b4):
//! `SignedIn`, `SignInRefused` and `SignInCodesDisabled`, and a passkey's
//! `PasskeyAdded` and `PasskeyRemoved` (chunk 5.5z2), which only the
//! runtime emits, kept in the outbox of the store sessions are kept in,
//! each with a delivery for every subscriber that listens for it.
//!
//! - **Nothing is kept when nothing listens**: a sign-in event no
//!   subscriber listens for writes no row, so the outbox holds no record
//!   of who signed in that no one asked for.
//! - **`SignedIn`** commits with the session or API token it reports, in
//!   the same transaction; from the application's own routes it is the
//!   running action's event, kept only if that action commits.
//! - **`SignInRefused` and `SignInCodesDisabled`** are kept in a
//!   transaction of their own once the refusal is decided, and a store
//!   that can't keep them leaves the refusal as it was.
//! - **`PasskeyAdded` and `PasskeyRemoved`** commit with the passkey
//!   they report.
//! - **What they carry** is the user's `id`, the way in, the reason, a
//!   time and a passkey's `id`: never an address, a link, a code, a
//!   token, a challenge or a passkey's credential ID or key. Their payload is
//!   the runtime's own, read back by [`read_payload`].

use std::rc::Rc;

use jadpo_std::http::{
    PASSKEY_ADDED, PASSKEY_REMOVED, SIGN_IN_CODES_DISABLED, SIGN_IN_REFUSED, SIGNED_IN,
    sign_in_event_key,
};
use jadpo_std::{SignInMethod, SignInRefusal};
use jadpo_value::Value;

use crate::outbox::{Delivery, Sealed};
use crate::runner::Deferred;
use crate::{Ctx, Executor, StoreError};

/// One sign-in event, as the runtime publishes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SignInEvent {
    /// A session or API token was started.
    SignedIn {
        user: [u8; 16],
        method: SignInMethod,
        first_time: bool,
    },
    /// A link, code or first sign-in was refused.
    Refused {
        method: SignInMethod,
        reason: SignInRefusal,
        user: Option<[u8; 16]>,
    },
    /// An address reached `failed_codes_per_day:`; codes work again at
    /// `until`, in milliseconds.
    CodesDisabled { user: Option<[u8; 16]>, until: i64 },
    /// A person registered a passkey.
    PasskeyAdded { user: [u8; 16], passkey: [u8; 16] },
    /// A person removed one of their passkeys.
    PasskeyRemoved { user: [u8; 16], passkey: [u8; 16] },
}

impl SignInEvent {
    fn name(&self) -> &'static str {
        match self {
            SignInEvent::SignedIn { .. } => SIGNED_IN,
            SignInEvent::Refused { .. } => SIGN_IN_REFUSED,
            SignInEvent::CodesDisabled { .. } => SIGN_IN_CODES_DISABLED,
            SignInEvent::PasskeyAdded { .. } => PASSKEY_ADDED,
            SignInEvent::PasskeyRemoved { .. } => PASSKEY_REMOVED,
        }
    }

    /// Its identity key: `jadpo.SignedIn`.
    pub(crate) fn key(&self) -> &'static str {
        sign_in_event_key(self.name()).unwrap_or_default()
    }

    /// Its value as a subscriber's action is given it, its fields in the
    /// order a test builds it in.
    pub(crate) fn value(&self) -> Value {
        let user = |user: &Option<[u8; 16]>| {
            user.map_or(Value::None, |id| {
                Value::Uuid(Rc::from(super::magic_links::format_uuid(&id)))
            })
        };
        let fields = match self {
            SignInEvent::SignedIn {
                user: id,
                method,
                first_time,
            } => vec![
                ("user_id", user(&Some(*id))),
                ("method", method_value(*method)),
                ("first_time", Value::Bool(*first_time)),
            ],
            SignInEvent::Refused {
                method,
                reason,
                user: id,
            } => vec![
                ("method", method_value(*method)),
                ("reason", reason_value(*reason)),
                ("user_id", user(id)),
            ],
            SignInEvent::CodesDisabled { user: id, until } => {
                vec![("user_id", user(id)), ("until", Value::Instant(*until))]
            }
            SignInEvent::PasskeyAdded { user: id, passkey }
            | SignInEvent::PasskeyRemoved { user: id, passkey } => vec![
                ("user_id", user(&Some(*id))),
                ("passkey_id", user(&Some(*passkey))),
            ],
        };
        Value::record(
            self.name(),
            fields
                .into_iter()
                .map(|(name, value)| (Rc::from(name), value))
                .collect(),
        )
    }

    /// The payload the outbox keeps it as.
    pub(crate) fn payload(&self) -> String {
        let user = |user: &Option<[u8; 16]>| {
            user.map_or(serde_json::Value::Null, |id| {
                serde_json::Value::from(super::magic_links::format_uuid(&id))
            })
        };
        let object = match self {
            SignInEvent::SignedIn {
                user: id,
                method,
                first_time,
            } => serde_json::json!({
                "user_id": user(&Some(*id)),
                "method": method.name(),
                "first_time": first_time,
            }),
            SignInEvent::Refused {
                method,
                reason,
                user: id,
            } => serde_json::json!({
                "method": method.name(),
                "reason": reason.name(),
                "user_id": user(id),
            }),
            SignInEvent::CodesDisabled { user: id, until } => serde_json::json!({
                "user_id": user(id),
                "until": until,
            }),
            SignInEvent::PasskeyAdded { user: id, passkey }
            | SignInEvent::PasskeyRemoved { user: id, passkey } => serde_json::json!({
                "user_id": user(&Some(*id)),
                "passkey_id": user(&Some(*passkey)),
            }),
        };
        object.to_string()
    }
}

fn method_value(method: SignInMethod) -> Value {
    let ordinal = jadpo_std::variant_ordinal(SignInMethod::NAME, method.name()).unwrap_or_default();
    Value::variant(SignInMethod::NAME, ordinal, method.name(), Vec::new())
}

fn reason_value(reason: SignInRefusal) -> Value {
    let ordinal =
        jadpo_std::variant_ordinal(SignInRefusal::NAME, reason.name()).unwrap_or_default();
    Value::variant(SignInRefusal::NAME, ordinal, reason.name(), Vec::new())
}

/// The value of the sign-in event whose identity key is `key` from its
/// outbox payload, as the sweep reads a delivery back; `None` when `key`
/// isn't one.
pub(crate) fn read_payload(key: &str, payload: &str) -> Option<Result<Value, String>> {
    let name = jadpo_std::http::sign_in_event_named(key)?;
    let not = || format!("not the payload of `{name}`");
    let Ok(serde_json::Value::Object(object)) = serde_json::from_str::<serde_json::Value>(payload)
    else {
        return Some(Err(not()));
    };
    let user = match object.get("user_id") {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(text)) => super::magic_links::parse_uuid(text)
            .map(Some)
            .ok_or_else(not),
        Some(_) => Err(not()),
    };
    let text = |field: &str| object.get(field).and_then(serde_json::Value::as_str);
    let method = text("method").and_then(|written| {
        SignInMethod::ALL
            .iter()
            .copied()
            .find(|method| method.name() == written)
    });
    let event = match name {
        SIGNED_IN => match (
            user,
            method,
            object
                .get("first_time")
                .and_then(serde_json::Value::as_bool),
        ) {
            (Ok(Some(user)), Some(method), Some(first_time)) => SignInEvent::SignedIn {
                user,
                method,
                first_time,
            },
            _ => return Some(Err(not())),
        },
        SIGN_IN_REFUSED => {
            let reason = text("reason").and_then(|written| {
                SignInRefusal::ALL
                    .iter()
                    .copied()
                    .find(|reason| reason.name() == written)
            });
            match (user, method, reason) {
                (Ok(user), Some(method), Some(reason)) => SignInEvent::Refused {
                    method,
                    reason,
                    user,
                },
                _ => return Some(Err(not())),
            }
        }
        PASSKEY_ADDED | PASSKEY_REMOVED => {
            let passkey = text("passkey_id").and_then(super::magic_links::parse_uuid);
            match (user, passkey) {
                (Ok(Some(user)), Some(passkey)) if name == PASSKEY_ADDED => {
                    SignInEvent::PasskeyAdded { user, passkey }
                }
                (Ok(Some(user)), Some(passkey)) => SignInEvent::PasskeyRemoved { user, passkey },
                _ => return Some(Err(not())),
            }
        }
        _ => match (
            user,
            object.get("until").and_then(serde_json::Value::as_i64),
        ) {
            (Ok(user), Some(until)) => SignInEvent::CodesDisabled { user, until },
            _ => return Some(Err(not())),
        },
    };
    Some(Ok(event.value()))
}

/// The identity keys of the subscribers that listen for `event`.
fn listening(executor: &dyn Executor, event: &SignInEvent) -> Vec<String> {
    executor.plan().events().map_or_else(Vec::new, |events| {
        events
            .plan
            .subscribers_of(event.key())
            .map(|subscriber| subscriber.key.clone())
            .collect()
    })
}

/// Whether any subscriber listens for `event`.
pub(crate) fn listens(executor: &dyn Executor, event: &SignInEvent) -> bool {
    !listening(executor, event).is_empty()
}

/// Writes `events` in `transaction`, kept in `store`'s outbox, each with
/// a delivery for each subscriber, and leaves the deliveries for once it
/// commits; nothing for an event no subscriber listens for. What the
/// caller commits, or drops with [`Ctx::take_deferred`] if it can't.
pub(crate) async fn write(
    ctx: &mut Ctx<'_>,
    executor: &dyn Executor,
    store: &str,
    transaction: &mut dyn crate::Transaction,
    events: &[SignInEvent],
) -> Result<(), StoreError> {
    let now = ctx.now();
    let trace = ctx.trace();
    let mut sealed = Sealed {
        outbox: Vec::new(),
        deliveries: Vec::new(),
    };
    for event in events {
        let subscribers = listening(executor, event);
        if subscribers.is_empty() {
            continue;
        }
        let Some(outbox) = ctx.new_id() else {
            return Err(StoreError::Unavailable);
        };
        let value = event.value();
        for subscriber in subscribers {
            let Some(id) = ctx.new_id() else {
                return Err(StoreError::Unavailable);
            };
            sealed.deliveries.push(Delivery {
                store: store.to_string(),
                id,
                outbox,
                subscriber,
                event: event.key().to_string(),
                value: value.clone(),
                emitted_at: now,
                trace,
                depth: 0,
            });
        }
        sealed
            .outbox
            .push((outbox, event.key().to_string(), event.payload()));
    }
    if sealed.outbox.is_empty() {
        return Ok(());
    }
    crate::outbox::write(transaction, &sealed, now, trace, 0).await?;
    ctx.defer(Deferred::Deliveries(sealed.deliveries));
    Ok(())
}

/// Writes `events` in `transaction` and commits it, the deliveries left
/// for once it has: what a passkey's sign-in, registration and removal
/// keep beside the session or the passkey they report (chunk 5.5z2). A
/// transaction that can't keep them is rolled back, and nothing is
/// delivered.
pub(crate) async fn commit_with(
    ctx: &mut Ctx<'_>,
    executor: &dyn Executor,
    store: &str,
    mut transaction: Box<dyn crate::Transaction>,
    events: &[SignInEvent],
) -> Result<(), StoreError> {
    let earlier = ctx.take_deferred();
    let written = write(ctx, executor, store, transaction.as_mut(), events).await;
    let kept = match written {
        Ok(()) => transaction.commit().await,
        Err(error) => {
            let _ = transaction.rollback().await;
            Err(error)
        }
    };
    let mine = ctx.take_deferred();
    for work in earlier {
        ctx.defer(work);
    }
    kept?;
    for work in mine {
        ctx.defer(work);
    }
    Ok(())
}

/// Publishes `events` in a transaction of their own in `store`: what a
/// refusal keeps once it is decided. A store that can't keep them leaves
/// the refusal as it was.
pub(crate) async fn publish(
    ctx: &mut Ctx<'_>,
    executor: &dyn Executor,
    store: &str,
    events: &[SignInEvent],
) {
    if events
        .iter()
        .all(|event| listening(executor, event).is_empty())
    {
        return;
    }
    let Some(database) = ctx.host().database(store) else {
        return;
    };
    let Ok(mut transaction) = database.begin().await else {
        return;
    };
    let before = ctx.take_deferred();
    let written = write(ctx, executor, store, transaction.as_mut(), events).await;
    let kept = written.is_ok() && transaction.commit().await.is_ok();
    let mine = ctx.take_deferred();
    for work in before {
        ctx.defer(work);
    }
    if kept {
        for work in mine {
            ctx.defer(work);
        }
    }
}

/// Emits `event` as the running action's own (the application's own
/// routes, chunk 7.3b3): kept in its transaction, begun in `store` when it
/// has none, and delivered only once it commits.
pub(crate) async fn emit(
    ctx: &mut Ctx<'_>,
    executor: &dyn Executor,
    store: &str,
    event: &SignInEvent,
) -> Result<(), StoreError> {
    let subscribers = listening(executor, event);
    if subscribers.is_empty() {
        return Ok(());
    }
    ctx.transaction(store).await?;
    ctx.emit(crate::Event {
        key: event.key().to_string(),
        value: event.value(),
        payload: event.payload(),
        subscribers,
    })
    .map_err(|_| StoreError::Fault("a sign-in event the action couldn't keep".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_payload_holds_no_address_and_reads_back() {
        for event in [
            SignInEvent::SignedIn {
                user: [1; 16],
                method: SignInMethod::MagicLink,
                first_time: true,
            },
            SignInEvent::Refused {
                method: SignInMethod::MagicLink,
                reason: SignInRefusal::CodeInvalid,
                user: None,
            },
            SignInEvent::CodesDisabled {
                user: Some([2; 16]),
                until: 1_792_540_800_000,
            },
            SignInEvent::SignedIn {
                user: [3; 16],
                method: SignInMethod::Passkey,
                first_time: false,
            },
            SignInEvent::Refused {
                method: SignInMethod::Passkey,
                reason: SignInRefusal::PasskeyRejected,
                user: Some([4; 16]),
            },
            SignInEvent::PasskeyAdded {
                user: [5; 16],
                passkey: [6; 16],
            },
            SignInEvent::PasskeyRemoved {
                user: [5; 16],
                passkey: [6; 16],
            },
        ] {
            let payload = event.payload();
            assert!(!payload.contains('@'), "{payload}");
            assert_eq!(read_payload(event.key(), &payload), Some(Ok(event.value())));
        }
        assert_eq!(read_payload("type notes.Other", "{}"), None);
    }
}
