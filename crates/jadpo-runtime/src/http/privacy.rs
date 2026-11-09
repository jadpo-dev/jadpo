//! Data requests (area 15, "Data requests"): the routes `privacy:`
//! generates, answered by the runtime for every host and executor, and
//! the address check Jadpo's own email carries.
//!
//! - **Asking by address** (`POST <path>/requests`) keeps one row per
//!   address (`jadpo_privacy_address_checks`, by its digest), counts a
//!   send against the address whether or not an account has it, and
//!   answers `202` either way. It leaves an email to check the address
//!   only when an account has it, so it never reveals who has one, and
//!   nobody else is mailed.
//! - **The link is made as the email is sent** ([`mint`]): a random token,
//!   kept only as a verifier, with the address sealed under it, so neither
//!   the table nor the outbox holds the link.
//! - **A request** is one row in `jadpo_privacy_requests`, with its
//!   one-month deadline, who made it and how they knew the person. It
//!   holds the person's `id`, never an address. Asking again while one of
//!   its kind is open gives the same request.
//! - **Each step is in the access records** (`privacy.request_asked`,
//!   `privacy.request_received`, `privacy.request_cancelled`) and has its
//!   event, for what subscribes.

use std::rc::Rc;

use jadpo_std::http::{
    IdentityPlan, PRIVACY_CHECK_EXPIRES_AFTER_MS, PRIVACY_CHECK_RESEND_AFTER_MS,
    PRIVACY_CHECKS_PER_HOUR, PRIVACY_COOLING_OFF_MS, PRIVACY_REQUEST_ACCEPTED, PrivacyPlan,
    PrivacyStep,
};
use jadpo_std::read::SqlType;
use jadpo_std::{PrivacyRequestKind, PrivacyRequestSource};
use jadpo_value::Value;

use super::identity::{AUTHENTICATION_MISCONFIGURED, AUTHENTICATION_UNAVAILABLE, CSRF_REJECTED};
use super::magic_links::{
    Refused, Stop, address_digest, email_address, format_uuid, json_body, limited, locale_named,
    micros, millis, new_token, ok_json, parse_uuid, problem, raised, seal, text, token_bytes,
    unseal, verifier,
};
use super::sign_in_events::{self, SignInEvent};
use super::{App, Request, Response, runtime_failure};
use crate::ctx::Caller;
use crate::outbox::{Delivery, Sealed};
use crate::runner::Deferred;
use crate::{Ctx, Executor, FaultKind, SqlValue, Stored};

pub(crate) const LINK_EXPIRED: (u16, &str, &str) = (
    401,
    "privacy_link_expired",
    "This link has expired or has been used. Ask again.",
);
pub(crate) const PERSON_NOT_FOUND: (u16, &str, &str) = (
    404,
    "privacy_person_not_found",
    "We can't find that person.",
);

/// Forgets the checks nothing counts any more.
pub(crate) const SWEEP: &str =
    "DELETE FROM \"jadpo_privacy_address_checks\" WHERE \"kept_until\" <= $1";

/// Makes an address's row if it has none.
pub(crate) const ENSURE: &str = "INSERT INTO \"jadpo_privacy_address_checks\" (\"id\", \"address\", \"sends\", \"kept_until\") VALUES ($1, $2, 0, $3) ON CONFLICT (\"address\") DO NOTHING";

/// An address's row, held until the request commits.
pub(crate) const LOCK: &str = "SELECT \"id\", \"requested_at\", \"sends_since\", \"sends\" FROM \"jadpo_privacy_address_checks\" WHERE \"address\" = $1 FOR UPDATE";

/// A new check: the earlier link is gone.
pub(crate) const ASKED: &str = "UPDATE \"jadpo_privacy_address_checks\" SET \"request\" = $2, \"token\" = NULL, \"sealed\" = NULL, \"requested_at\" = $3, \"expires_at\" = $4, \"sends_since\" = $5, \"sends\" = $6, \"kept_until\" = GREATEST(\"kept_until\", $7) WHERE \"id\" = $1";

/// The link of check `$1`, as its email is sent, while it is the latest
/// and hasn't expired.
pub(crate) const MINT: &str = "UPDATE \"jadpo_privacy_address_checks\" SET \"token\" = $2, \"sealed\" = $3 WHERE \"request\" = $1 AND \"expires_at\" > $4 RETURNING \"expires_at\"";

/// The check a link's token names, held, while it hasn't expired.
pub(crate) const BY_TOKEN: &str = "SELECT \"id\", \"sealed\" FROM \"jadpo_privacy_address_checks\" WHERE \"token\" = $1 AND \"expires_at\" > $2 FOR UPDATE";

/// A link's token, used: gone, with what opens it.
pub(crate) const USED: &str = "UPDATE \"jadpo_privacy_address_checks\" SET \"request\" = NULL, \"token\" = NULL, \"sealed\" = NULL WHERE \"id\" = $1";

/// A new request, unless one of its kind is open for this person.
pub(crate) const CREATE: &str = "INSERT INTO \"jadpo_privacy_requests\" (\"id\", \"kind\", \"source\", \"person\", \"created_by\", \"identity_check\", \"state\", \"requested_at\", \"deadline\", \"erases_at\", \"cancel_key\", \"open\") VALUES ($1, $2, $3, $4, $5, $6, 'received', $7, $8, $9, $10, $11) ON CONFLICT (\"open\") DO NOTHING RETURNING \"id\"";

/// The open request of this kind for this person.
pub(crate) const EXISTING: &str =
    "SELECT \"id\", \"deadline\" FROM \"jadpo_privacy_requests\" WHERE \"open\" = $1";

/// An erasure's cancel link, used: the erasure is off and the link gone.
pub(crate) const CANCEL: &str = "UPDATE \"jadpo_privacy_requests\" SET \"state\" = 'cancelled', \"cancelled_at\" = $2, \"cancel_key\" = NULL, \"open\" = NULL WHERE \"cancel_key\" = $1 AND \"state\" = 'received' AND \"erases_at\" > $2 RETURNING \"id\", \"person\"";

const HOUR_MS: i64 = 60 * 60 * 1000;

/// One calendar month after `at`, in UTC: a day the next month doesn't
/// have is its last.
pub(crate) fn deadline_after(at: i64) -> i64 {
    const DAY_MS: i64 = 86_400_000;
    let (day, ms) = (at.div_euclid(DAY_MS), at.rem_euclid(DAY_MS));
    let Ok(day) = i32::try_from(day) else {
        return at;
    };
    let (year, month, day_of_month) = jadpo_std::civil_date(day);
    let (year, month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let last = jadpo_std::days_in_month(year, month);
    jadpo_std::epoch_day(year, month, day_of_month.min(last))
        .map_or(at, |day| i64::from(day) * DAY_MS + ms)
}

/// What every step reads.
struct Site<'s> {
    plan: &'s IdentityPlan,
    privacy: &'s PrivacyPlan,
    executor: &'s dyn Executor,
    request_id: &'s str,
}

/// Answers `step`.
pub(crate) async fn answer(
    app: &App<'_>,
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    request: &Request,
    request_id: &str,
    step: PrivacyStep,
    caller: Option<&Caller>,
) -> Response {
    let run = Box::pin(run(app, executor, ctx, request, request_id, step, caller));
    let mut response = match run.await {
        Ok(response) => response,
        Err(stopped) => *stopped,
    };
    response
        .headers
        .push(("cache-control".to_string(), "no-store".to_string()));
    response
}

fn fail(failure: (u16, &str, &str), request_id: &str) -> Stop {
    Box::new(runtime_failure(failure, request_id, None))
}

async fn run(
    app: &App<'_>,
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    request: &Request,
    request_id: &str,
    step: PrivacyStep,
    caller: Option<&Caller>,
) -> Result<Response, Stop> {
    let Some((plan, privacy)) = app
        .table
        .identity
        .as_ref()
        .and_then(|plan| Some((plan, plan.privacy.as_ref()?)))
    else {
        return Err(fail(AUTHENTICATION_MISCONFIGURED, request_id));
    };
    if request
        .query
        .as_deref()
        .is_some_and(|query| !query.is_empty())
    {
        return Err(Box::new(problem(request_id, "", "unknown_field")));
    }
    let site = Site {
        plan,
        privacy,
        executor,
        request_id,
    };
    match step {
        PrivacyStep::Request => site.request(ctx, request).await,
        PrivacyStep::Confirm => site.confirm(ctx, request).await,
        PrivacyStep::Cancel => site.cancel(ctx, request).await,
        PrivacyStep::Export => {
            site.own(ctx, request, caller, PrivacyRequestKind::Export)
                .await
        }
        PrivacyStep::Erasure => {
            site.own(ctx, request, caller, PrivacyRequestKind::Erasure)
                .await
        }
    }
}

/// A browser session's request, or anyone's in a test, must come from the
/// application's own site, as a sign-in's does.
fn from_own_site(ctx: &Ctx<'_>, request: &Request) -> bool {
    let started = ctx.started();
    let Some(origin) = started.auth().origin() else {
        return true;
    };
    request
        .header("origin")
        .and_then(super::signed::origin_of)
        .as_deref()
        == Some(origin)
}

/// What an address check keeps.
struct Asking<'a> {
    now: i64,
    row: [u8; 16],
    request: [u8; 16],
    address: &'a str,
}

/// Asks for a check in `transaction`, the generated route's own or the
/// running action's: the send limits counted alike for every address.
async fn ask(
    transaction: &mut dyn crate::Transaction,
    privacy: &PrivacyPlan,
    asking: &Asking<'_>,
) -> Result<Result<bool, i64>, crate::StoreError> {
    let now = asking.now;
    let digest = address_digest(asking.address);
    let expires = now.saturating_add(PRIVACY_CHECK_EXPIRES_AFTER_MS);
    transaction
        .execute(
            ENSURE,
            &[
                SqlValue::Uuid(asking.row),
                SqlValue::Text(digest.clone()),
                micros(now.saturating_add(HOUR_MS)),
            ],
        )
        .await?;
    let rows = transaction.query(LOCK, &[SqlValue::Text(digest)]).await?;
    let Some(
        [
            SqlValue::Uuid(id),
            requested_at,
            sends_since,
            SqlValue::Int4(sends),
        ],
    ) = rows.first().map(Vec::as_slice)
    else {
        return Err(crate::StoreError::Unavailable);
    };
    if let Some(at) = millis(requested_at) {
        let next = at.saturating_add(PRIVACY_CHECK_RESEND_AFTER_MS);
        if next > now {
            return Ok(Err(next - now));
        }
    }
    let (since, sends) = match millis(sends_since) {
        Some(since) if since.saturating_add(HOUR_MS) > now => {
            if u32::try_from(*sends).unwrap_or(u32::MAX) >= PRIVACY_CHECKS_PER_HOUR {
                return Ok(Err(since.saturating_add(HOUR_MS) - now));
            }
            (since, sends.saturating_add(1))
        }
        _ => (now, 1),
    };
    let found = match &privacy.lookup {
        Some(lookup) => !transaction
            .query(
                lookup,
                &[SqlValue::Text(asking.address.to_string()), micros(now)],
            )
            .await?
            .is_empty(),
        None => false,
    };
    transaction
        .execute(
            ASKED,
            &[
                SqlValue::Uuid(*id),
                SqlValue::Uuid(asking.request),
                micros(now),
                micros(expires),
                micros(since),
                SqlValue::Int4(sends),
                micros(expires.max(since.saturating_add(HOUR_MS))),
            ],
        )
        .await?;
    Ok(Ok(found))
}

impl Site<'_> {
    fn stop(&self, why: Refused) -> Stop {
        match why {
            Refused::Failure(failure, wait) => {
                let mut response = runtime_failure(failure, self.request_id, None);
                if let Some(wait) = wait {
                    response
                        .headers
                        .push(("retry-after".to_string(), wait.to_string()));
                }
                Box::new(response)
            }
            Refused::Store(_) => fail(AUTHENTICATION_UNAVAILABLE, self.request_id),
            Refused::Fault(fault) => {
                let mut response = runtime_failure(super::INTERNAL_FAULT, self.request_id, None);
                response.fault = Some(fault);
                Box::new(response)
            }
        }
    }

    fn database<'c>(&self, ctx: &Ctx<'c>) -> Result<&'c dyn crate::Database, Refused> {
        ctx.host()
            .database(&self.plan.store)
            .ok_or(Refused::Failure(AUTHENTICATION_MISCONFIGURED, None))
    }

    /// The email's language: `asked` when the application supports it,
    /// else the best match for `accept`, else its default.
    fn locale(&self, asked: Option<u32>, accept: Option<&str>) -> Option<u32> {
        let email = self.executor.plan().events().and_then(|events| {
            events
                .plan
                .emails
                .email(jadpo_std::email::PRIVACY_ADDRESS_CHECK_EMAIL)
                .cloned()
        })?;
        let Some(jadpo_std::email::BuiltinEmail::PrivacyAddressCheck(mail)) =
            email.builtin.as_ref()
        else {
            return None;
        };
        Some(
            asked
                .filter(|ordinal| mail.locales.supports(*ordinal))
                .unwrap_or_else(|| {
                    jadpo_value::locale::from_accept_language(accept, &mail.locales)
                }),
        )
    }

    /// `POST <path>/requests`.
    async fn request(&self, ctx: &mut Ctx<'_>, request: &Request) -> Result<Response, Stop> {
        let body = json_body(request, self.request_id, &["address", "locale"])?;
        let address = email_address(&body, self.request_id)?;
        let locale = self.locale(
            text(&body, "locale").and_then(|asked| locale_named(&asked)),
            request.header("accept-language"),
        );
        let now = ctx.now();
        let (Some(row), Some(asked)) = (ctx.new_id(), ctx.new_id()) else {
            return Err(fail(AUTHENTICATION_MISCONFIGURED, self.request_id));
        };
        let database = self.database(ctx).map_err(|why| self.stop(why))?;
        let mut transaction = database
            .begin()
            .await
            .map_err(|_| fail(AUTHENTICATION_UNAVAILABLE, self.request_id))?;
        let asking = Asking {
            now,
            row,
            request: asked,
            address: &address,
        };
        let steps: Result<Option<i64>, crate::StoreError> = async {
            transaction.execute(SWEEP, &[micros(now)]).await?;
            let sends_email = match ask(transaction.as_mut(), self.privacy, &asking).await? {
                Err(wait) => return Ok(Some(wait)),
                Ok(sends_email) => sends_email,
            };
            if sends_email {
                let Some(locale) = locale else {
                    return Err(crate::StoreError::Unavailable);
                };
                let (Some(outbox), Some(delivery)) = (ctx.new_id(), ctx.new_id()) else {
                    return Err(crate::StoreError::Unavailable);
                };
                let value = email_value(&address, locale, asked);
                let sealed = Sealed {
                    outbox: vec![(
                        outbox,
                        jadpo_std::email::PRIVACY_ADDRESS_CHECK_EMAIL.to_string(),
                        payload(&address, locale, asked),
                    )],
                    deliveries: vec![Delivery {
                        store: self.plan.store.clone(),
                        id: delivery,
                        outbox,
                        subscriber: subscriber_key(),
                        event: jadpo_std::email::PRIVACY_ADDRESS_CHECK_EMAIL.to_string(),
                        value,
                        emitted_at: now,
                        trace: ctx.trace(),
                        depth: 0,
                    }],
                };
                crate::outbox::write(transaction.as_mut(), &sealed, now, ctx.trace(), 0).await?;
                ctx.defer(Deferred::Deliveries(sealed.deliveries));
            }
            Ok(None)
        }
        .await;
        match steps {
            Ok(Some(wait)) => {
                let _ = transaction.rollback().await;
                return Err(Box::new(limited(self.request_id, wait)));
            }
            Ok(None) => {}
            Err(_) => {
                let _ = transaction.rollback().await;
                let _ = ctx.take_deferred();
                return Err(fail(AUTHENTICATION_UNAVAILABLE, self.request_id));
            }
        }
        if transaction.commit().await.is_err() {
            let _ = ctx.take_deferred();
            return Err(fail(AUTHENTICATION_UNAVAILABLE, self.request_id));
        }
        crate::logging::privacy_asked(ctx);
        Ok(Response {
            status: 202,
            ..Response::default()
        })
    }

    /// `POST <path>/requests/confirm`.
    async fn confirm(&self, ctx: &mut Ctx<'_>, request: &Request) -> Result<Response, Stop> {
        if !from_own_site(ctx, request) {
            return Err(fail(CSRF_REJECTED, self.request_id));
        }
        let body = json_body(request, self.request_id, &["token", "kind"])?;
        let Some(token) = text(&body, "token") else {
            return Err(Box::new(problem(self.request_id, "token", "required")));
        };
        let Some(kind) = text(&body, "kind") else {
            return Err(Box::new(problem(self.request_id, "kind", "required")));
        };
        let Some(kind) = PrivacyRequestKind::ALL
            .iter()
            .copied()
            .find(|known| known.name() == kind)
        else {
            return Err(Box::new(problem(self.request_id, "kind", "format")));
        };
        let database = self.database(ctx).map_err(|why| self.stop(why))?;
        let mut transaction = database
            .begin()
            .await
            .map_err(|_| fail(AUTHENTICATION_UNAVAILABLE, self.request_id))?;
        let Some(prepared) = prepare(ctx, kind) else {
            let _ = transaction.rollback().await;
            return Err(fail(AUTHENTICATION_MISCONFIGURED, self.request_id));
        };
        let confirmed =
            confirm_in(self.privacy, prepared, transaction.as_mut(), &token, kind).await;
        match confirmed {
            Ok(made) => {
                let events = made.events();
                sign_in_events::commit_with(
                    ctx,
                    self.executor,
                    &self.plan.store,
                    transaction,
                    &events,
                )
                .await
                .map_err(|_| fail(AUTHENTICATION_UNAVAILABLE, self.request_id))?;
                crate::logging::privacy_received(ctx, &made.log());
                Ok(accepted(&made))
            }
            Err(why) => {
                let _ = transaction.rollback().await;
                Err(self.stop(why))
            }
        }
    }

    /// `POST <path>/requests/cancel`.
    async fn cancel(&self, ctx: &mut Ctx<'_>, request: &Request) -> Result<Response, Stop> {
        if !from_own_site(ctx, request) {
            return Err(fail(CSRF_REJECTED, self.request_id));
        }
        let body = json_body(request, self.request_id, &["token"])?;
        let Some(token) = text(&body, "token") else {
            return Err(Box::new(problem(self.request_id, "token", "required")));
        };
        let database = self.database(ctx).map_err(|why| self.stop(why))?;
        let cancelled = cancel_through(ctx, database, &token).await;
        match cancelled {
            Ok(Some((request, person))) => {
                crate::logging::privacy_cancelled(ctx, &request, &person);
                Ok(Response {
                    status: 204,
                    ..Response::default()
                })
            }
            Ok(None) => Err(fail(LINK_EXPIRED, self.request_id)),
            Err(_) => Err(fail(AUTHENTICATION_UNAVAILABLE, self.request_id)),
        }
    }

    /// `POST <path>/export` and `<path>/erasure`: the caller's own, with
    /// no person id to pass.
    async fn own(
        &self,
        ctx: &mut Ctx<'_>,
        request: &Request,
        caller: Option<&Caller>,
        kind: PrivacyRequestKind,
    ) -> Result<Response, Stop> {
        let _ = request;
        let Some(caller) = caller else {
            return Err(fail(AUTHENTICATION_MISCONFIGURED, self.request_id));
        };
        let Value::Record(record) = &caller.value else {
            return Err(fail(AUTHENTICATION_MISCONFIGURED, self.request_id));
        };
        let Some(Value::Uuid(id)) = record.get("id") else {
            return Err(fail(AUTHENTICATION_MISCONFIGURED, self.request_id));
        };
        let Some(person) = parse_uuid(id) else {
            return Err(fail(AUTHENTICATION_MISCONFIGURED, self.request_id));
        };
        let database = self.database(ctx).map_err(|why| self.stop(why))?;
        let mut transaction = database
            .begin()
            .await
            .map_err(|_| fail(AUTHENTICATION_UNAVAILABLE, self.request_id))?;
        let making = Making {
            kind,
            source: PrivacyRequestSource::SignedIn,
            person,
            created_by: Some(caller.key.clone()),
            identity_check: None,
        };
        let Some(prepared) = prepare(ctx, kind) else {
            let _ = transaction.rollback().await;
            return Err(fail(AUTHENTICATION_MISCONFIGURED, self.request_id));
        };
        let made = make(self.privacy, prepared, transaction.as_mut(), making).await;
        let made = match made {
            Ok(made) => made,
            Err(_) => {
                let _ = transaction.rollback().await;
                return Err(fail(AUTHENTICATION_UNAVAILABLE, self.request_id));
            }
        };
        let events = made.events();
        sign_in_events::commit_with(ctx, self.executor, &self.plan.store, transaction, &events)
            .await
            .map_err(|_| fail(AUTHENTICATION_UNAVAILABLE, self.request_id))?;
        crate::logging::privacy_received(ctx, &made.log());
        Ok(accepted(&made))
    }
}

/// A person's request being made.
struct Making {
    kind: PrivacyRequestKind,
    source: PrivacyRequestSource,
    person: [u8; 16],
    created_by: Option<String>,
    identity_check: Option<String>,
}

/// A request, new or found.
struct Made {
    id: [u8; 16],
    person: [u8; 16],
    kind: PrivacyRequestKind,
    source: PrivacyRequestSource,
    deadline: i64,
    /// Set when it is new: its erasure's date and cancel link, its events.
    new: bool,
    erases_at: Option<i64>,
    cancel_link: Option<String>,
}

impl Made {
    fn events(&self) -> Vec<SignInEvent> {
        if !self.new {
            return Vec::new();
        }
        let mut out = vec![SignInEvent::PrivacyRequestReceived {
            request: self.id,
            user: self.person,
            kind: self.kind.name(),
            source: self.source.name(),
            deadline: self.deadline,
        }];
        if let (Some(erases_at), Some(cancel_link)) = (self.erases_at, &self.cancel_link) {
            out.push(SignInEvent::PrivacyErasureScheduled {
                request: self.id,
                user: self.person,
                erases_at,
                cancel_link: cancel_link.clone(),
            });
        }
        out
    }

    fn log(&self) -> crate::logging::PrivacyLogged {
        crate::logging::PrivacyLogged {
            request: format_uuid(&self.id),
            person: format_uuid(&self.person),
            kind: self.kind.name(),
            source: self.source.name(),
            new: self.new,
        }
    }
}

/// What a new request takes from the run, read before its transaction
/// is borrowed, so nothing in the transaction waits on the context.
struct Prepared {
    id: [u8; 16],
    now: i64,
    /// An erasure's cancel link's token, and the browser origin it opens.
    cancel: Option<(String, String)>,
}

fn prepare(ctx: &mut Ctx<'_>, kind: PrivacyRequestKind) -> Option<Prepared> {
    let id = ctx.new_id()?;
    let now = ctx.now();
    let cancel = if kind == PrivacyRequestKind::Erasure {
        let token = new_token(ctx)?;
        let started = ctx.started();
        let origin = started
            .auth()
            .origin()
            .and_then(super::signed::origin_of)
            .unwrap_or_default();
        Some((token, origin))
    } else {
        None
    };
    Some(Prepared { id, now, cancel })
}

/// Makes the request, or finds the one of its kind already open for this
/// person: then nothing happens twice and cancelling it cancels the one
/// there is.
async fn make(
    privacy: &PrivacyPlan,
    prepared: Prepared,
    transaction: &mut dyn crate::Transaction,
    making: Making,
) -> Result<Made, crate::StoreError> {
    let (id, now) = (prepared.id, prepared.now);
    let deadline = deadline_after(now);
    let open = format!("{} {}", making.kind.name(), format_uuid(&making.person));
    let (erases_at, cancel_key, cancel_link) = if let Some((token, origin)) = prepared.cancel {
        let link = format!("{origin}{}?token={token}", privacy.cancel_link);
        (
            Some(now.saturating_add(PRIVACY_COOLING_OFF_MS)),
            Some(verifier(token.as_bytes())),
            Some(link),
        )
    } else {
        (None, None, None)
    };
    let text = |value: Option<String>| value.map_or(SqlValue::Null(SqlType::Text), SqlValue::Text);
    let rows = transaction
        .query(
            CREATE,
            &[
                SqlValue::Uuid(id),
                SqlValue::Text(making.kind.name().to_string()),
                SqlValue::Text(making.source.name().to_string()),
                SqlValue::Uuid(making.person),
                text(making.created_by.clone()),
                text(making.identity_check.clone()),
                micros(now),
                micros(deadline),
                erases_at.map_or(SqlValue::Null(SqlType::Timestamp), micros),
                text(cancel_key.clone()),
                SqlValue::Text(open.clone()),
            ],
        )
        .await?;
    if rows.is_empty() {
        let rows = transaction.query(EXISTING, &[SqlValue::Text(open)]).await?;
        let Some([SqlValue::Uuid(existing), deadline]) = rows.first().map(Vec::as_slice) else {
            return Err(crate::StoreError::Unavailable);
        };
        return Ok(Made {
            id: *existing,
            person: making.person,
            kind: making.kind,
            source: making.source,
            deadline: millis(deadline).unwrap_or(now),
            new: false,
            erases_at: None,
            cancel_link: None,
        });
    }
    Ok(Made {
        id,
        person: making.person,
        kind: making.kind,
        source: making.source,
        deadline,
        new: true,
        erases_at,
        cancel_link,
    })
}

/// A checked link's token used and the request made, in `transaction`.
async fn confirm_in(
    privacy: &PrivacyPlan,
    prepared: Prepared,
    transaction: &mut dyn crate::Transaction,
    token: &str,
    kind: PrivacyRequestKind,
) -> Result<Made, Refused> {
    let Some(bytes) = token_bytes(token) else {
        return Err(Refused::Failure(LINK_EXPIRED, None));
    };
    let now = prepared.now;
    let rows = transaction
        .query(
            BY_TOKEN,
            &[SqlValue::Text(verifier(token.as_bytes())), micros(now)],
        )
        .await
        .map_err(Refused::Store)?;
    let Some([SqlValue::Uuid(row), SqlValue::Bytes(sealed)]) = rows.first().map(Vec::as_slice)
    else {
        return Err(Refused::Failure(LINK_EXPIRED, None));
    };
    let (row, sealed) = (*row, sealed.clone());
    transaction
        .execute(USED, &[SqlValue::Uuid(row)])
        .await
        .map_err(Refused::Store)?;
    let Some(address) = unseal(&bytes, &sealed) else {
        return Err(Refused::Failure(LINK_EXPIRED, None));
    };
    let Some(lookup) = privacy.lookup.as_deref() else {
        return Err(Refused::Failure(AUTHENTICATION_MISCONFIGURED, None));
    };
    let rows = transaction
        .query(lookup, &[SqlValue::Text(address), micros(now)])
        .await
        .map_err(Refused::Store)?;
    // A person erased since the email went has no one to act for.
    let Some([SqlValue::Uuid(person), ..]) = rows.first().map(Vec::as_slice) else {
        return Err(Refused::Failure(LINK_EXPIRED, None));
    };
    make(
        privacy,
        prepared,
        transaction,
        Making {
            kind,
            source: PrivacyRequestSource::SelfService,
            person: *person,
            created_by: None,
            identity_check: None,
        },
    )
    .await
    .map_err(Refused::Store)
}

/// Cancels the erasure a cancel link names, in a transaction of its own.
async fn cancel_through(
    ctx: &mut Ctx<'_>,
    database: &dyn crate::Database,
    token: &str,
) -> Result<Option<(String, String)>, crate::StoreError> {
    let now = ctx.now();
    let rows = database
        .query_once(
            CANCEL,
            &[SqlValue::Text(verifier(token.as_bytes())), micros(now)],
        )
        .await?;
    Ok(match rows.first().map(Vec::as_slice) {
        Some([SqlValue::Uuid(request), SqlValue::Uuid(person)]) => {
            Some((format_uuid(request), format_uuid(person)))
        }
        _ => None,
    })
}

/// The `202` naming the request.
fn accepted(made: &Made) -> Response {
    let mut body = String::from("{\"request_id\":");
    jadpo_value::json::write_string(&format_uuid(&made.id), &mut body);
    body.push_str(",\"kind\":");
    jadpo_value::json::write_string(made.kind.name(), &mut body);
    body.push_str(",\"deadline\":");
    let instant = match jadpo_value::standard::from(&Value::Instant(made.deadline)) {
        Ok(Value::Text(text)) => text.to_string(),
        _ => String::new(),
    };
    jadpo_value::json::write_string(&instant, &mut body);
    body.push('}');
    let mut out = ok_json(body);
    out.status = 202;
    out
}

/// `PrivacyRequestAccepted` from a generated route's body, as a route test
/// reads it back.
pub(crate) fn response_output(body: &[u8]) -> Value {
    let Ok(serde_json::Value::Object(object)) = serde_json::from_slice::<serde_json::Value>(body)
    else {
        return Value::None;
    };
    let (Some(request_id), Some(kind), Some(deadline)) = (
        object.get("request_id").and_then(serde_json::Value::as_str),
        object.get("kind").and_then(serde_json::Value::as_str),
        object.get("deadline").and_then(serde_json::Value::as_str),
    ) else {
        return Value::None;
    };
    let Ok(deadline) = jadpo_value::standard::parse(deadline, "Instant") else {
        return Value::None;
    };
    let ordinal = jadpo_std::variant_ordinal(PrivacyRequestKind::NAME, kind).unwrap_or_default();
    Value::record(
        PRIVACY_REQUEST_ACCEPTED,
        vec![
            (Rc::from("request_id"), Value::Uuid(Rc::from(request_id))),
            (
                Rc::from("kind"),
                Value::variant(PrivacyRequestKind::NAME, ordinal, kind, Vec::new()),
            ),
            (Rc::from("deadline"), deadline),
        ],
    )
}

// --- the operations ------------------------------------------------------------

fn defect(ctx: &mut Ctx<'_>, what: &str, site: u32) -> Stored {
    Stored::Fault(ctx.fault(FaultKind::Defect(what.to_string()), site))
}

fn plan_of<'s>(executor: &'s dyn Executor, plan: Option<&'s IdentityPlan>) -> Option<Site<'s>> {
    let plan = plan?;
    Some(Site {
        plan,
        privacy: plan.privacy.as_ref()?,
        executor,
        request_id: "",
    })
}

/// `privacy.request(address, locale)` at site `site`.
pub async fn request_op(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    plan: Option<&IdentityPlan>,
    args: &[Value],
    at: u32,
) -> Stored {
    let Some(site) = plan_of(executor, plan) else {
        return defect(ctx, "`privacy.request` with no `privacy:`", at);
    };
    let [Value::Text(address), Value::Variant(locale)] = args else {
        return defect(ctx, "`privacy.request` called with the wrong arguments", at);
    };
    let locale = site.locale(Some(locale.ordinal), None);
    let now = ctx.now();
    let (Some(row), Some(asked)) = (ctx.new_id(), ctx.new_id()) else {
        return raised(
            ctx,
            Refused::Failure(AUTHENTICATION_MISCONFIGURED, None),
            at,
        );
    };
    if let Ok(database) = site.database(ctx) {
        let _ = database.query_once(SWEEP, &[micros(now)]).await;
    }
    let asking = Asking {
        now,
        row,
        request: asked,
        address,
    };
    let asked_now = match ctx.transaction(&site.plan.store).await {
        Ok(transaction) => ask(transaction, site.privacy, &asking).await,
        Err(error) => Err(error),
    };
    let sends = match asked_now {
        Ok(Err(wait)) => {
            return raised(
                ctx,
                Refused::Failure(super::RATE_LIMITED, Some(wait / 1000 + 1)),
                at,
            );
        }
        Ok(Ok(sends)) => sends,
        Err(error) => return raised(ctx, Refused::Store(error), at),
    };
    if sends {
        let Some(locale) = locale else {
            return raised(ctx, Refused::Failure(AUTHENTICATION_UNAVAILABLE, None), at);
        };
        let event = crate::Event {
            key: jadpo_std::email::PRIVACY_ADDRESS_CHECK_EMAIL.to_string(),
            value: email_value(address, locale, asked),
            payload: payload(address, locale, asked),
            subscribers: vec![subscriber_key()],
        };
        if ctx.emit(event).is_err() {
            return raised(ctx, Refused::Failure(AUTHENTICATION_UNAVAILABLE, None), at);
        }
    }
    crate::logging::privacy_asked(ctx);
    Stored::Value(Value::Unit)
}

/// `privacy.confirm(token, kind)`.
pub async fn confirm_op(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    plan: Option<&IdentityPlan>,
    args: &[Value],
    at: u32,
) -> Stored {
    let Some(site) = plan_of(executor, plan) else {
        return defect(ctx, "`privacy.confirm` with no `privacy:`", at);
    };
    let [Value::Text(token), Value::Variant(kind)] = args else {
        return defect(ctx, "`privacy.confirm` called with the wrong arguments", at);
    };
    let Some(kind) = PrivacyRequestKind::ALL
        .iter()
        .copied()
        .find(|known| known.name() == &*kind.name)
    else {
        return defect(ctx, "`privacy.confirm` with an unknown kind", at);
    };
    let Some(prepared) = prepare(ctx, kind) else {
        return raised(
            ctx,
            Refused::Failure(AUTHENTICATION_MISCONFIGURED, None),
            at,
        );
    };
    let privacy = site.privacy;
    let made = match ctx.transaction(&site.plan.store).await {
        Ok(transaction) => confirm_in(privacy, prepared, transaction, token, kind).await,
        Err(error) => Err(Refused::Store(error)),
    };
    finish_op(ctx, &site, made, at).await
}

/// `privacy.cancel(token)`.
pub async fn cancel_op(
    ctx: &mut Ctx<'_>,
    plan: Option<&IdentityPlan>,
    args: &[Value],
    at: u32,
) -> Stored {
    let Some(privacy) = plan.and_then(|plan| plan.privacy.as_ref()) else {
        return defect(ctx, "`privacy.cancel` with no `privacy:`", at);
    };
    let [Value::Text(token)] = args else {
        return defect(ctx, "`privacy.cancel` called with the wrong arguments", at);
    };
    let Some(database) = ctx.host().database(&privacy.store) else {
        return raised(
            ctx,
            Refused::Failure(AUTHENTICATION_MISCONFIGURED, None),
            at,
        );
    };
    match cancel_through(ctx, database, token).await {
        Ok(Some((request, person))) => {
            crate::logging::privacy_cancelled(ctx, &request, &person);
            Stored::Value(Value::Unit)
        }
        Ok(None) => raised(ctx, Refused::Failure(LINK_EXPIRED, None), at),
        Err(error) => raised(ctx, Refused::Store(error), at),
    }
}

/// `privacy.create_request(user_id, kind, identity_check)`, granted by
/// `actions:`: its record says who made it and how they knew the person.
pub async fn create_request_op(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    plan: Option<&IdentityPlan>,
    args: &[Value],
    at: u32,
) -> Stored {
    let Some(site) = plan_of(executor, plan) else {
        return defect(ctx, "`privacy.create_request` with no `privacy:`", at);
    };
    if let Some(gate) = &site.privacy.create_request {
        let admitted = crate::policy::admit(ctx, gate, at).await;
        if !matches!(admitted, Stored::Value(_)) {
            return admitted;
        }
    } else {
        return Stored::Failed(Box::new(crate::read::not_permitted()));
    }
    let [
        Value::Uuid(person),
        Value::Variant(kind),
        Value::Text(identity_check),
    ] = args
    else {
        return defect(
            ctx,
            "`privacy.create_request` called with the wrong arguments",
            at,
        );
    };
    let Some(kind) = PrivacyRequestKind::ALL
        .iter()
        .copied()
        .find(|known| known.name() == &*kind.name)
    else {
        return defect(ctx, "`privacy.create_request` with an unknown kind", at);
    };
    let Some(person_id) = parse_uuid(person) else {
        return defect(
            ctx,
            "`privacy.create_request` with an id that isn't one",
            at,
        );
    };
    let Some(lookup) = site.privacy.lookup_id.clone() else {
        return raised(
            ctx,
            Refused::Failure(AUTHENTICATION_MISCONFIGURED, None),
            at,
        );
    };
    let making = Making {
        kind,
        source: PrivacyRequestSource::Assisted,
        person: person_id,
        created_by: ctx.caller().map(|caller| caller.key.clone()),
        identity_check: Some(identity_check.to_string()),
    };
    let Some(prepared) = prepare(ctx, kind) else {
        return raised(
            ctx,
            Refused::Failure(AUTHENTICATION_MISCONFIGURED, None),
            at,
        );
    };
    let privacy = site.privacy;
    let made = match ctx.transaction(&site.plan.store).await {
        Ok(transaction) => create_in_action(privacy, prepared, transaction, &lookup, making).await,
        Err(error) => Err(Refused::Store(error)),
    };
    finish_op(ctx, &site, made, at).await
}

/// `privacy.create_request`'s body against the action's transaction.
async fn create_in_action(
    privacy: &PrivacyPlan,
    prepared: Prepared,
    transaction: &mut dyn crate::Transaction,
    lookup: &str,
    making: Making,
) -> Result<Made, Refused> {
    let rows = transaction
        .query(
            lookup,
            &[SqlValue::Uuid(making.person), micros(prepared.now)],
        )
        .await
        .map_err(Refused::Store)?;
    if rows.is_empty() {
        return Err(Refused::Failure(PERSON_NOT_FOUND, None));
    }
    make(privacy, prepared, transaction, making)
        .await
        .map_err(Refused::Store)
}

/// What an operation that made a request gives: its events kept in the
/// running action, its access record, and the `PrivacyRequestAccepted`.
async fn finish_op(
    ctx: &mut Ctx<'_>,
    site: &Site<'_>,
    made: Result<Made, Refused>,
    at: u32,
) -> Stored {
    let made = match made {
        Ok(made) => made,
        Err(why) => return raised(ctx, why, at),
    };
    for event in made.events() {
        if let Err(error) = sign_in_events::emit(ctx, site.executor, &site.plan.store, &event).await
        {
            return raised(ctx, Refused::Store(error), at);
        }
    }
    crate::logging::privacy_received(ctx, &made.log());
    Stored::Value(Value::record(
        PRIVACY_REQUEST_ACCEPTED,
        vec![
            (
                Rc::from("request_id"),
                Value::Uuid(Rc::from(format_uuid(&made.id))),
            ),
            (
                Rc::from("kind"),
                Value::variant(
                    PrivacyRequestKind::NAME,
                    jadpo_std::variant_ordinal(PrivacyRequestKind::NAME, made.kind.name())
                        .unwrap_or_default(),
                    made.kind.name(),
                    Vec::new(),
                ),
            ),
            (Rc::from("deadline"), Value::Instant(made.deadline)),
        ],
    ))
}

// --- Jadpo's address-check email ----------------------------------------------

/// The subscriber key the address-check email's delivery names.
pub(crate) fn subscriber_key() -> String {
    format!("email {}", jadpo_std::email::PRIVACY_ADDRESS_CHECK_EMAIL)
}

/// The email's value as its delivery reads it: the address, the locale and
/// the check, nothing secret.
fn email_value(address: &str, locale: u32, request: [u8; 16]) -> Value {
    Value::record(
        jadpo_std::email::PRIVACY_ADDRESS_CHECK_EMAIL,
        vec![
            (Rc::from("to"), Value::text(address)),
            (Rc::from("locale"), jadpo_value::locale::value(locale)),
            (Rc::from("request"), Value::text(&format_uuid(&request))),
        ],
    )
}

/// The outbox payload the email is kept as.
fn payload(address: &str, locale: u32, request: [u8; 16]) -> String {
    let tag = jadpo_std::LOCALES
        .get(locale as usize)
        .map_or("", |(name, _)| *name);
    let mut out = String::from("{\"to\":");
    jadpo_value::json::write_string(address, &mut out);
    out.push_str(",\"locale\":");
    jadpo_value::json::write_string(tag, &mut out);
    out.push_str(",\"request\":");
    jadpo_value::json::write_string(&format_uuid(&request), &mut out);
    out.push('}');
    out
}

/// The email's value from its outbox payload.
pub(crate) fn read_payload(payload: &str) -> Result<Value, String> {
    let not = || "not the address-check email's payload".to_string();
    let Ok(serde_json::Value::Object(object)) = serde_json::from_str::<serde_json::Value>(payload)
    else {
        return Err(not());
    };
    let field = |key: &str| object.get(key).and_then(serde_json::Value::as_str);
    let (Some(to), Some(locale), Some(request)) = (
        field("to"),
        field("locale").and_then(jadpo_std::locale_ordinal),
        field("request").and_then(parse_uuid),
    ) else {
        return Err(not());
    };
    Ok(email_value(to, locale, request))
}

/// What the email carries, made as it is sent. `None` when its check was
/// used, replaced or has expired, and nothing is sent.
pub(crate) struct Minted {
    pub address: String,
    pub link: String,
    pub token: String,
}

/// Makes the link of the check `value` names, keeping only its verifier
/// and the address sealed under the token.
pub(crate) async fn mint(
    ctx: &mut Ctx<'_>,
    plan: &jadpo_std::email::PrivacyAddressCheckEmailPlan,
    value: &Value,
) -> Result<Option<Minted>, &'static str> {
    let Value::Record(record) = value else {
        return Err("an address-check email that isn't a record");
    };
    let (Some(Value::Text(address)), Some(Value::Text(request))) =
        (record.get("to"), record.get("request"))
    else {
        return Err("an address-check email without its address or check");
    };
    let Some(request) = parse_uuid(request) else {
        return Err("an address-check email whose check isn't an id");
    };
    let origin = match ctx.setting(plan.origin) {
        Some(Value::Text(origin)) => super::signed::origin_of(origin),
        _ => None,
    };
    let Some(origin) = origin else {
        return Err("an address-check email whose origin setting has no value");
    };
    let Some(token) = new_token(ctx) else {
        return Err("an address-check email with no randomness");
    };
    let Some(bytes) = token_bytes(&token) else {
        return Err("an address-check token that isn't one");
    };
    let Some(database) = ctx.host().database(&plan.store) else {
        return Err("an address-check email with no store for its checks");
    };
    let now = ctx.now();
    let rows = database
        .query_once(
            MINT,
            &[
                SqlValue::Uuid(request),
                SqlValue::Text(verifier(token.as_bytes())),
                SqlValue::Bytes(seal(&bytes, address)),
                micros(now),
            ],
        )
        .await
        .map_err(|_| "unavailable")?;
    if rows.is_empty() {
        return Ok(None);
    }
    Ok(Some(Minted {
        address: address.to_string(),
        link: format!("{origin}{}?token={token}", plan.link),
        token,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_month_after_is_a_calendar_month_in_utc() {
        // 2026-01-31T12:00:00Z → 2026-02-28T12:00:00Z.
        let january = 1_769_860_800_000;
        let february = 1_772_280_000_000;
        assert_eq!(deadline_after(january), february);
    }

    #[test]
    fn the_payload_holds_nothing_secret_and_reads_back() {
        let locale = jadpo_std::locale_ordinal("en_gb").unwrap();
        let written = payload("ada@example.com", locale, [3; 16]);
        assert!(!written.contains("token"), "{written}");
        assert_eq!(
            read_payload(&written).expect("reads"),
            email_value("ada@example.com", locale, [3; 16])
        );
    }
}
