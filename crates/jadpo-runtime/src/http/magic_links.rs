//! Magic links (area 8, "Magic links"; chunk 7.3b): the routes `sign_in: {
//! magic_link: }` generates, answered by the runtime for every host and
//! executor, and the link and code Jadpo's own email carries.
//!
//! - **One row per address** in `jadpo_sign_in_requests`, found by the
//!   digest of the address in lower case, so the send limits and the
//!   wrong-code limits count every way of writing it as one; it keeps the
//!   digest of the address as written too, which a code must be confirmed
//!   with. Nothing in it is an address, a link or a code.
//! - **Asking** (`POST <path>`) counts a send against the address whether
//!   or not an account has it, refuses over `resend_after:` or
//!   `codes_per_hour:` with `rate_limited` for every address alike, makes
//!   the earlier link and code invalid, and, when an account has the
//!   address or `on_first_sign_in:` may make one, leaves Jadpo's email in
//!   the outbox in the same transaction. The answer is 202 with no body
//!   either way.
//! - **The link and the code are made as the email is sent**
//!   ([`mint`]): 32 random bytes and six random digits, kept only as
//!   verifiers (the code's keyed by its request), with the address sealed
//!   under the link's token, so neither the outbox nor the table ever
//!   holds them. A delivery whose request was used or replaced sends
//!   nothing.
//! - **Confirming** (`POST <path>/confirm`, `POST <path>/redeem`) takes a
//!   link's token or an address and its code, once, under a row lock:
//!   using either ends the other. Codes are compared in constant time; a
//!   wrong one counts against the code (`code_attempts:`) and the address's
//!   day (`failed_codes_per_day:`), and every wrong, used, ended, expired
//!   or replaced code answers `sign_in_code_invalid` alike. A link opened
//!   in another browser under `same_browser:` is refused and left as it
//!   was, so the right browser or the code still works.
//! - **The person** is found by the address, exactly as the user's field
//!   holds it; with no user, `on_first_sign_in:`'s action decides, run as
//!   the system in its own transaction; a user whose `active:` doesn't
//!   hold is `user_disabled`. The browser session or API token is kept as
//!   `sessions.start` and `sessions.issue_token` keep one.
//!
//! - **Every sign-in publishes an event** (chunk 7.3b4): `SignedIn`
//!   with the session or token, `SignInRefused` for a link, code or first
//!   sign-in refused, its reason kept for the request's log line too, and
//!   `SignInCodesDisabled` when an address reaches the day's wrong codes
//!   ([`super::sign_in_events`]).
//!
//! Nothing here logs or answers with a token, a code or an address.

use std::rc::Rc;

use jadpo_std::email::{MAGIC_LINK_EMAIL, MagicLinkEmailPlan};
use jadpo_std::http::{
    API_TOKEN_PREFIX, FIRST_SIGN_IN, IdentityPlan, MAGIC_LINK_REDEEMED, MagicLinkPlan,
    MagicLinkStep, RETURN_TO_MAX, SESSION_LIFETIME_MS,
};
use jadpo_std::read::SqlType;
use jadpo_std::{SignInMethod, SignInRefusal};
use jadpo_value::Value;

use super::identity::{
    AUTHENTICATION_MISCONFIGURED, AUTHENTICATION_UNAVAILABLE, CSRF_REJECTED, START, USER_DISABLED,
    set_cookies,
};
use super::sign_in_events::{self, SignInEvent};
use super::{
    App, INTERNAL_FAULT, Problem, RATE_LIMITED, Request, Response, invalid_input, runtime_failure,
};
use crate::ctx::{Binding, SessionEffect, SignInRefused, System};
use crate::outbox::{Delivery, Sealed};
use crate::runner::Deferred;
use crate::store::Stored;
use crate::{Ctx, Executor, FaultKind, Outcome, SqlValue};

pub(crate) const LINK_EXPIRED: (u16, &str, &str) = (
    401,
    "sign_in_link_expired",
    "The sign-in link expired or was already used.",
);
pub(crate) const LINK_OTHER_BROWSER: (u16, &str, &str) = (
    401,
    "sign_in_link_other_browser",
    "The sign-in link was opened in another browser.",
);
pub(crate) const CODE_INVALID: (u16, &str, &str) =
    (401, "sign_in_code_invalid", "The sign-in code isn't right.");
pub(crate) const CODES_DISABLED: (u16, &str, &str) = (
    429,
    "sign_in_codes_disabled",
    "Too many wrong sign-in codes today.",
);
pub(crate) const REFUSED: (u16, &str, &str) = (403, "sign_in_refused", "The sign-in was refused.");
/// `magic_links.request`'s refusal of a path to go back to that isn't on
/// the application's own site (chunk 7.3b3).
pub(crate) const RETURN_INVALID: (u16, &str, &str) = (
    422,
    "sign_in_return_invalid",
    "The path to go back to isn't on this site.",
);

/// Forgets the rows nothing counts any more.
pub(crate) const SWEEP: &str = "DELETE FROM \"jadpo_sign_in_requests\" WHERE \"kept_until\" <= $1";

/// Makes an address's row if it has none.
pub(crate) const ENSURE: &str = "INSERT INTO \"jadpo_sign_in_requests\" (\"id\", \"address\", \"wrong_codes\", \"sends\", \"failed\", \"kept_until\") VALUES ($1, $2, 0, 0, 0, $3) ON CONFLICT (\"address\") DO NOTHING";

/// An address's row, held until the request commits.
pub(crate) const LOCK: &str = "SELECT \"id\", \"requested_at\", \"sends_since\", \"sends\" FROM \"jadpo_sign_in_requests\" WHERE \"address\" = $1 FOR UPDATE";

/// A new request: the earlier link and code are gone.
pub(crate) const ASKED: &str = "UPDATE \"jadpo_sign_in_requests\" SET \"request\" = $2, \"exact\" = $3, \"token\" = NULL, \"code\" = NULL, \"sealed\" = NULL, \"return_to\" = $4, \"binding\" = $5, \"wrong_codes\" = 0, \"requested_at\" = $6, \"expires_at\" = $7, \"sends_since\" = $8, \"sends\" = $9, \"kept_until\" = GREATEST(\"kept_until\", $10) WHERE \"id\" = $1";

/// The link and the code of request `$1`, as its email is sent, while it
/// is still the latest and hasn't expired.
pub(crate) const MINT: &str = "UPDATE \"jadpo_sign_in_requests\" SET \"token\" = $2, \"code\" = $3, \"sealed\" = $4 WHERE \"request\" = $1 AND \"expires_at\" > $5 RETURNING \"expires_at\"";

/// The row a link's token names, held.
pub(crate) const BY_TOKEN: &str = "SELECT \"id\", \"sealed\", \"return_to\", \"binding\", \"expires_at\" FROM \"jadpo_sign_in_requests\" WHERE \"token\" = $1 FOR UPDATE";

/// The row an address names, held, for its code.
pub(crate) const BY_ADDRESS: &str = "SELECT \"id\", \"exact\", \"request\", \"code\", \"return_to\", \"expires_at\", \"wrong_codes\", \"failed_since\", \"failed\" FROM \"jadpo_sign_in_requests\" WHERE \"address\" = $1 FOR UPDATE";

/// A link or code used: both are gone.
pub(crate) const USED: &str = "UPDATE \"jadpo_sign_in_requests\" SET \"request\" = NULL, \"token\" = NULL, \"code\" = NULL, \"sealed\" = NULL, \"binding\" = NULL, \"wrong_codes\" = 0 WHERE \"id\" = $1";

/// A wrong code counted: against the code, which `$3` ends, and the day.
pub(crate) const WRONG: &str = "UPDATE \"jadpo_sign_in_requests\" SET \"wrong_codes\" = $2, \"code\" = CASE WHEN $3 THEN NULL ELSE \"code\" END, \"failed_since\" = $4, \"failed\" = $5, \"kept_until\" = GREATEST(\"kept_until\", $6) WHERE \"id\" = $1";

const HOUR_MS: i64 = 60 * 60 * 1000;
const DAY_MS: i64 = 24 * HOUR_MS;

/// Why a step stopped: the response that says so.
type Stop = Box<Response>;

fn stop(failure: (u16, &str, &str), request_id: &str) -> Stop {
    Box::new(runtime_failure(failure, request_id, None))
}

/// The cookie binding a `same_browser:` link to its browser:
/// `<cookie>_sign_in`.
pub(crate) fn binding_cookie(plan: &IdentityPlan) -> Option<String> {
    plan.cookie
        .as_ref()
        .map(|cookie| format!("{cookie}_sign_in"))
}

/// The `Set-Cookie` header setting or clearing the binding cookie, on the
/// response of a generated route or of the application's own route whose
/// action asked for or used a link (chunk 7.3b3).
pub(crate) fn binding_header(plan: &IdentityPlan, binding: &Binding) -> Vec<(String, String)> {
    let Some(cookie) = binding_cookie(plan) else {
        return Vec::new();
    };
    let (token, age) = match binding {
        Binding::Set { token, max_age } => (token.as_str(), *max_age),
        Binding::Clear => ("", 0),
    };
    vec![(
        "set-cookie".to_string(),
        format!("{cookie}={token}; Path=/; Max-Age={age}; HttpOnly; Secure; SameSite=Lax"),
    )]
}

/// Answers `step`.
pub(crate) async fn answer(
    app: &App<'_>,
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    request: &Request,
    request_id: &str,
    step: MagicLinkStep,
) -> Response {
    let mut response = match run(app, executor, ctx, request, request_id, step).await {
        Ok(response) => response,
        Err(stopped) => *stopped,
    };
    response
        .headers
        .push(("cache-control".to_string(), "no-store".to_string()));
    response
}

async fn run(
    app: &App<'_>,
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    request: &Request,
    request_id: &str,
    step: MagicLinkStep,
) -> Result<Response, Stop> {
    let started = ctx.started();
    let (Some(plan), Some(origin)) = (app.table.identity.as_ref(), started.auth().origin()) else {
        return Err(stop(AUTHENTICATION_MISCONFIGURED, request_id));
    };
    let Some(magic) = plan.magic_links.as_ref() else {
        return Err(stop(AUTHENTICATION_MISCONFIGURED, request_id));
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
        magic,
        executor,
        request_id,
    };
    match step {
        MagicLinkStep::Request => site.request(ctx, request).await,
        MagicLinkStep::Confirm => site.confirm(ctx, request, origin, false).await,
        MagicLinkStep::Redeem => site.confirm(ctx, request, origin, true).await,
    }
}

/// A 422 naming one field and its rule.
fn problem(request_id: &str, field: &str, rule: &str) -> Response {
    invalid_input(
        request_id,
        &[Problem {
            field: field.to_string(),
            rule: rule.to_string(),
        }],
    )
}

/// What every step reads.
struct Site<'s> {
    plan: &'s IdentityPlan,
    magic: &'s MagicLinkPlan,
    executor: &'s dyn Executor,
    /// The request's id, for a generated route's response; empty for an
    /// operation, which answers nothing itself.
    request_id: &'s str,
}

/// The person a confirmed link or code names: their user's `id`, and
/// whether `on_first_sign_in:`'s action made them just now.
struct Person {
    user: [u8; 16],
    first_time: bool,
}

/// What a confirmed link or code gives: the address it was sent to and
/// where to go.
struct Confirmed {
    address: String,
    return_to: Option<String>,
}

/// Why a step was refused, which a generated route answers and an
/// operation raises alike (chunk 7.3b3).
enum Refused {
    /// A runtime failure (its status, code and message), with the seconds
    /// to wait before trying again when it says.
    Failure((u16, &'static str, &'static str), Option<i64>),
    /// The running action's transaction couldn't go on.
    Store(crate::StoreError),
    /// `on_first_sign_in:`'s action faulted.
    Fault(Box<crate::Fault>),
}

fn refused(failure: (u16, &'static str, &'static str)) -> Refused {
    Refused::Failure(failure, None)
}

/// Whole seconds, at least one, for a wait of `millis`.
fn seconds(millis: i64) -> i64 {
    ((millis + 999) / 1000).max(1)
}

/// What one request for a link and a code keeps (area 8, Decisions,
/// "What is kept").
struct Asking<'a> {
    now: i64,
    /// The address's row, made if it has none.
    row: [u8; 16],
    /// The request's own id, which its email's delivery names.
    request: [u8; 16],
    address: &'a str,
    return_to: Option<&'a str>,
    /// A `same_browser:` link's binding token.
    binding: Option<&'a str>,
}

/// What asking did.
enum Asked {
    /// Over `resend_after:` or `codes_per_hour:`: wait this many
    /// milliseconds, whether or not an account has the address.
    Limited(i64),
    /// Kept as the address's only valid request, with an email to send
    /// when an account has the address or a first sign-in may make one.
    Kept { sends_email: bool },
}

/// Asks for a link and a code in `transaction`, the generated route's own
/// or the running action's: the address's row made and held, the send
/// limits counted alike for every address, and the earlier link and code
/// made invalid.
async fn ask(
    transaction: &mut dyn crate::Transaction,
    magic: &MagicLinkPlan,
    asking: &Asking<'_>,
) -> Result<Asked, crate::StoreError> {
    let now = asking.now;
    let digest = address_digest(asking.address);
    let expires = now.saturating_add(magic.expires_after_ms);
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
    // One send per `resend_after:`, and `codes_per_hour:` an hour,
    // whether or not an account has the address.
    if let Some(at) = millis(requested_at) {
        let next = at.saturating_add(magic.resend_after_ms);
        if next > now {
            return Ok(Asked::Limited(next - now));
        }
    }
    let (since, sends) = match millis(sends_since) {
        Some(since) if since.saturating_add(HOUR_MS) > now => {
            if u32::try_from(*sends).unwrap_or(u32::MAX) >= magic.codes_per_hour {
                return Ok(Asked::Limited(since.saturating_add(HOUR_MS) - now));
            }
            (since, sends.saturating_add(1))
        }
        _ => (now, 1),
    };
    // Whether an email goes: to an account, or to an address a first
    // sign-in may make one for.
    let found = transaction
        .query(
            &magic.lookup,
            &[SqlValue::Text(asking.address.to_string()), micros(now)],
        )
        .await?;
    let sends_email = !found.is_empty() || magic.first_sign_in.is_some();
    transaction
        .execute(
            ASKED,
            &[
                SqlValue::Uuid(*id),
                SqlValue::Uuid(asking.request),
                SqlValue::Text(exact_digest(asking.address)),
                asking
                    .return_to
                    .map_or(SqlValue::Null(SqlType::Text), |path| {
                        SqlValue::Text(path.to_string())
                    }),
                asking
                    .binding
                    .map_or(SqlValue::Null(SqlType::Text), |token| {
                        SqlValue::Text(verifier(token.as_bytes()))
                    }),
                micros(now),
                micros(expires),
                micros(since),
                SqlValue::Int4(sends),
                micros(expires.max(since.saturating_add(HOUR_MS))),
            ],
        )
        .await?;
    Ok(Asked::Kept { sends_email })
}

impl Site<'_> {
    /// A generated route's response to `refused`.
    fn stop(&self, refused: Refused) -> Stop {
        match refused {
            Refused::Failure(failure, wait) => {
                let mut response = runtime_failure(failure, self.request_id, None);
                if let Some(wait) = wait {
                    response
                        .headers
                        .push(("retry-after".to_string(), wait.to_string()));
                }
                Box::new(response)
            }
            Refused::Store(_) => stop(AUTHENTICATION_UNAVAILABLE, self.request_id),
            Refused::Fault(fault) => {
                let mut response = runtime_failure(INTERNAL_FAULT, self.request_id, None);
                response.fault = Some(fault);
                Box::new(response)
            }
        }
    }

    fn fail(&self, failure: (u16, &str, &str)) -> Stop {
        stop(failure, self.request_id)
    }

    fn database<'c>(&self, ctx: &Ctx<'c>) -> Result<&'c dyn crate::Database, Refused> {
        ctx.host()
            .database(&self.plan.store)
            .ok_or(refused(AUTHENTICATION_MISCONFIGURED))
    }

    /// The email's language: `asked` when the application supports it,
    /// else the best match for `accept` (a request's `Accept-Language`),
    /// else its default. `None` when no sign-in email is planned.
    fn locale(&self, asked: Option<u32>, accept: Option<&str>) -> Option<u32> {
        let email = self
            .executor
            .plan()
            .events()
            .and_then(|events| events.plan.emails.email(MAGIC_LINK_EMAIL).cloned())?;
        let jadpo_std::email::BuiltinEmail::MagicLink(mail) = email.builtin.as_ref()?;
        Some(
            asked
                .filter(|ordinal| mail.locales.supports(*ordinal))
                .unwrap_or_else(|| {
                    jadpo_value::locale::from_accept_language(accept, &mail.locales)
                }),
        )
    }

    /// The body: a JSON object of text members, each one of `allowed`,
    /// `null` counting as left out.
    fn body(
        &self,
        request: &Request,
        allowed: &[&str],
    ) -> Result<serde_json::Map<String, serde_json::Value>, Stop> {
        let json = request.header("content-type").is_some_and(|value| {
            let essence = value
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            essence == "application/json"
                || (essence.starts_with("application/") && essence.ends_with("+json"))
        });
        if !json {
            return Err(self.fail(super::BODY_NOT_JSON));
        }
        let Ok(serde_json::Value::Object(object)) =
            serde_json::from_slice::<serde_json::Value>(&request.body)
        else {
            return Err(Box::new(problem(self.request_id, "", "type")));
        };
        let mut problems = Vec::new();
        for (key, value) in &object {
            if !allowed.contains(&key.as_str()) {
                problems.push(Problem {
                    field: key.clone(),
                    rule: "unknown_field".to_string(),
                });
            } else if !matches!(
                value,
                serde_json::Value::String(_) | serde_json::Value::Null
            ) {
                problems.push(Problem {
                    field: key.clone(),
                    rule: "type".to_string(),
                });
            }
        }
        if problems.is_empty() {
            Ok(object)
        } else {
            Err(Box::new(invalid_input(self.request_id, &problems)))
        }
    }

    /// An address the body gives as `address`, required.
    fn address(&self, body: &serde_json::Map<String, serde_json::Value>) -> Result<String, Stop> {
        match text(body, "address") {
            None => Err(Box::new(problem(self.request_id, "address", "required"))),
            Some(address) if address.len() <= 254 && jadpo_std::is_email(&address) => Ok(address),
            Some(_) => Err(Box::new(problem(self.request_id, "address", "format"))),
        }
    }

    // --- asking ------------------------------------------------------------------

    /// `POST <path>`: asks in a transaction of its own, keeping Jadpo's
    /// email in the outbox beside the request.
    async fn request(&self, ctx: &mut Ctx<'_>, request: &Request) -> Result<Response, Stop> {
        let body = self.body(request, &["address", "return_to", "locale"])?;
        let address = self.address(&body)?;
        let return_to = text(&body, "return_to");
        if let Some(path) = &return_to
            && !site_path(path)
        {
            return Err(Box::new(problem(self.request_id, "return_to", "path")));
        }
        // The email's language: the one asked for, when the application
        // supports it, else the best match for `Accept-Language`.
        let locale = self.locale(
            text(&body, "locale").and_then(|asked| locale_named(&asked)),
            request.header("accept-language"),
        );
        let now = ctx.now();
        let (Some(row), Some(asked)) = (ctx.new_id(), ctx.new_id()) else {
            return Err(self.fail(AUTHENTICATION_MISCONFIGURED));
        };
        // A `same_browser:` link is bound to this browser.
        let binding = if self.magic.same_browser {
            let Some(token) = new_token(ctx) else {
                return Err(self.fail(AUTHENTICATION_MISCONFIGURED));
            };
            Some(token)
        } else {
            None
        };
        let database = self.database(ctx).map_err(|why| self.stop(why))?;
        let mut transaction = database
            .begin()
            .await
            .map_err(|_| self.fail(AUTHENTICATION_UNAVAILABLE))?;
        let asking = Asking {
            now,
            row,
            request: asked,
            address: &address,
            return_to: return_to.as_deref(),
            binding: binding.as_deref(),
        };
        let steps: Result<Option<Response>, crate::StoreError> = async {
            transaction.execute(SWEEP, &[micros(now)]).await?;
            let sends_email = match ask(transaction.as_mut(), self.magic, &asking).await? {
                Asked::Limited(wait) => {
                    return Ok(Some(limited(self.request_id, wait)));
                }
                Asked::Kept { sends_email } => sends_email,
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
                        MAGIC_LINK_EMAIL.to_string(),
                        payload(&address, locale, asked),
                    )],
                    deliveries: vec![Delivery {
                        store: self.plan.store.clone(),
                        id: delivery,
                        outbox,
                        subscriber: subscriber_key(),
                        event: MAGIC_LINK_EMAIL.to_string(),
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
            Ok(Some(limited)) => {
                let _ = transaction.rollback().await;
                return Err(Box::new(limited));
            }
            Ok(None) => {}
            Err(_) => {
                let _ = transaction.rollback().await;
                // What was left for after the response goes with it.
                let _ = ctx.take_deferred();
                return Err(self.fail(AUTHENTICATION_UNAVAILABLE));
            }
        }
        if transaction.commit().await.is_err() {
            let _ = ctx.take_deferred();
            return Err(self.fail(AUTHENTICATION_UNAVAILABLE));
        }
        let mut response = Response {
            status: 202,
            ..Response::default()
        };
        if let Some(token) = binding {
            response.headers.extend(binding_header(
                self.plan,
                &Binding::Set {
                    token,
                    max_age: self.magic.expires_after_ms / 1000,
                },
            ));
        }
        Ok(response)
    }

    /// `magic_links.request`: asks in the running action's transaction,
    /// leaving Jadpo's email as the action's own event, so it is sent once
    /// the action commits and never if it fails.
    async fn request_in_action(
        &self,
        ctx: &mut Ctx<'_>,
        address: &str,
        locale: u32,
        return_to: Option<&str>,
    ) -> Result<(), Refused> {
        if let Some(path) = return_to
            && !site_path(path)
        {
            return Err(refused(RETURN_INVALID));
        }
        let locale = self.locale(Some(locale), None);
        let now = ctx.now();
        let (Some(row), Some(asked)) = (ctx.new_id(), ctx.new_id()) else {
            return Err(refused(AUTHENTICATION_MISCONFIGURED));
        };
        let binding = if self.magic.same_browser {
            Some(new_token(ctx).ok_or(refused(AUTHENTICATION_MISCONFIGURED))?)
        } else {
            None
        };
        // Rows nothing counts any more are forgotten on their own, outside
        // the action's transaction, as the generated route forgets them.
        let database = self.database(ctx)?;
        let _ = database.query_once(SWEEP, &[micros(now)]).await;
        let asking = Asking {
            now,
            row,
            request: asked,
            address,
            return_to,
            binding: binding.as_deref(),
        };
        let asked_now = match ctx.transaction(&self.plan.store).await {
            Ok(transaction) => ask(transaction, self.magic, &asking).await,
            Err(error) => Err(error),
        };
        match asked_now.map_err(Refused::Store)? {
            Asked::Limited(wait) => {
                return Err(Refused::Failure(RATE_LIMITED, Some(seconds(wait))));
            }
            Asked::Kept { sends_email: false } => {}
            Asked::Kept { sends_email: true } => {
                let Some(locale) = locale else {
                    return Err(refused(AUTHENTICATION_UNAVAILABLE));
                };
                let event = crate::Event {
                    key: MAGIC_LINK_EMAIL.to_string(),
                    value: email_value(address, locale, asked),
                    payload: payload(address, locale, asked),
                    subscribers: vec![subscriber_key()],
                };
                if ctx.emit(event).is_err() {
                    return Err(refused(INTERNAL_FAULT));
                }
            }
        }
        // Using a link or a code in this action would wait on the row its
        // transaction now holds.
        ctx.ask_sign_in();
        if let Some(token) = binding {
            ctx.binding(Binding::Set {
                token,
                max_age: self.magic.expires_after_ms / 1000,
            });
        }
        Ok(())
    }

    // --- confirming -------------------------------------------------------------

    /// A link's token or a code, then the person: a browser session, or an
    /// API token when `redeem`.
    async fn confirm(
        &self,
        ctx: &mut Ctx<'_>,
        request: &Request,
        origin: &str,
        redeem: bool,
    ) -> Result<Response, Stop> {
        let mut clear_binding = false;
        let confirmed = if redeem {
            let body = self.body(request, &["address", "code"])?;
            let address = self.address(&body)?;
            let Some(code) = text(&body, "code") else {
                return Err(Box::new(problem(self.request_id, "code", "required")));
            };
            self.by_code(ctx, &address, &code)
                .await
                .map_err(|why| self.stop(why))?
        } else {
            // A browser session starts only from the application's own site.
            let from = request.header("origin").and_then(super::signed::origin_of);
            if from.as_deref() != Some(origin) {
                return Err(self.fail(CSRF_REJECTED));
            }
            let body = self.body(request, &["token", "address", "code"])?;
            match (
                text(&body, "token"),
                text(&body, "address"),
                text(&body, "code"),
            ) {
                (Some(token), None, None) => {
                    clear_binding = self.magic.same_browser;
                    let given =
                        binding_cookie(self.plan).and_then(|cookie| cookie_value(request, &cookie));
                    self.by_token(ctx, given, &token)
                        .await
                        .map_err(|why| self.stop(why))?
                }
                (None, Some(_), Some(code)) => {
                    let address = self.address(&body)?;
                    self.by_code(ctx, &address, &code)
                        .await
                        .map_err(|why| self.stop(why))?
                }
                (None, None, None) => {
                    return Err(Box::new(problem(self.request_id, "token", "required")));
                }
                (Some(_), _, _) => {
                    return Err(Box::new(problem(self.request_id, "token", "unknown_field")));
                }
                (None, None, Some(_)) => {
                    return Err(Box::new(problem(self.request_id, "address", "required")));
                }
                (None, Some(_), None) => {
                    return Err(Box::new(problem(self.request_id, "code", "required")));
                }
            }
        };
        let person = self
            .person(ctx, &confirmed.address)
            .await
            .map_err(|why| self.stop(why))?;
        let user = SqlValue::Uuid(person.user);
        let now = ctx.now();
        let (Some(token), Some(id)) = (new_token(ctx), ctx.new_id()) else {
            return Err(self.fail(AUTHENTICATION_MISCONFIGURED));
        };
        if redeem {
            let token = format!("{API_TOKEN_PREFIX}{token}");
            let params = [
                SqlValue::Uuid(id),
                user,
                SqlValue::Text(verifier(token.as_bytes())),
                SqlValue::Null(SqlType::Text),
                micros(now),
                micros(now.saturating_add(SESSION_LIFETIME_MS)),
            ];
            self.start(ctx, &params, &person)
                .await
                .map_err(|_| self.fail(AUTHENTICATION_UNAVAILABLE))?;
            let mut body = String::from("{\"token\":");
            jadpo_value::json::write_string(&token, &mut body);
            body.push('}');
            return Ok(ok_json(body));
        }
        let Some(csrf) = new_token(ctx) else {
            return Err(self.fail(AUTHENTICATION_MISCONFIGURED));
        };
        let params = [
            SqlValue::Uuid(id),
            user,
            SqlValue::Text(verifier(token.as_bytes())),
            SqlValue::Text(verifier(csrf.as_bytes())),
            micros(now),
            micros(now.saturating_add(SESSION_LIFETIME_MS)),
        ];
        self.start(ctx, &params, &person)
            .await
            .map_err(|_| self.fail(AUTHENTICATION_UNAVAILABLE))?;
        let mut body = String::from("{\"return_to\":");
        jadpo_value::json::write_string(confirmed.return_to.as_deref().unwrap_or("/"), &mut body);
        body.push('}');
        let mut response = ok_json(body);
        response.headers.extend(set_cookies(
            self.plan,
            &SessionEffect::Start { token, csrf },
        ));
        if clear_binding {
            response
                .headers
                .extend(binding_header(self.plan, &Binding::Clear));
        }
        Ok(response)
    }

    /// Keeps the session or API token `params` describe, with `SignedIn`
    /// for `person` beside it in the same transaction.
    async fn start(
        &self,
        ctx: &mut Ctx<'_>,
        params: &[SqlValue],
        person: &Person,
    ) -> Result<(), crate::StoreError> {
        let database = self
            .database(ctx)
            .map_err(|_| crate::StoreError::Unavailable)?;
        let mut transaction = database.begin().await?;
        if let Err(error) = transaction.execute(START, params).await {
            let _ = transaction.rollback().await;
            return Err(error);
        }
        let event = SignInEvent::SignedIn {
            user: person.user,
            method: SignInMethod::MagicLink,
            first_time: person.first_time,
        };
        sign_in_events::commit_with(ctx, self.executor, &self.plan.store, transaction, &[event])
            .await
    }

    /// Keeps why the sign-in was refused for the request's log line, and
    /// publishes `SignInRefused` with the user `address` names, when it
    /// names one; gives back `refused`, what the caller is answered.
    async fn refusing(
        &self,
        ctx: &mut Ctx<'_>,
        reason: SignInRefusal,
        address: Option<&str>,
        failure: Option<String>,
        refused: Refused,
    ) -> Refused {
        ctx.refuse_sign_in(SignInRefused {
            reason: reason.name(),
            failure,
        });
        let mut event = SignInEvent::Refused {
            method: SignInMethod::MagicLink,
            reason,
            user: None,
        };
        if sign_in_events::listens(self.executor, &event) {
            if let (SignInEvent::Refused { user, .. }, Some(address)) = (&mut event, address) {
                *user = self.user_of(ctx, address).await;
            }
            sign_in_events::publish(ctx, self.executor, &self.plan.store, &[event]).await;
        }
        refused
    }

    /// The `id` of the user `address` belongs to, for an event; `None`
    /// when none does or the store can't say.
    async fn user_of(&self, ctx: &mut Ctx<'_>, address: &str) -> Option<[u8; 16]> {
        let now = ctx.now();
        let database = self.database(ctx).ok()?;
        let rows = database
            .query_once(
                &self.magic.lookup,
                &[SqlValue::Text(address.to_string()), micros(now)],
            )
            .await
            .ok()?;
        match rows.first().map(Vec::as_slice) {
            Some([SqlValue::Uuid(id), ..]) => Some(*id),
            _ => None,
        }
    }

    /// `magic_links.redeem` or `.redeem_code`: the link or the code used,
    /// then the person, found or made, as the generated routes find them;
    /// the application's action then starts their session itself, and
    /// `SignedIn` is its event, delivered once it commits.
    async fn redeemed(&self, ctx: &mut Ctx<'_>, confirmed: Confirmed) -> Result<Value, Refused> {
        let person = self.person(ctx, &confirmed.address).await?;
        let id = person.user;
        let event = SignInEvent::SignedIn {
            user: person.user,
            method: SignInMethod::MagicLink,
            first_time: person.first_time,
        };
        sign_in_events::emit(ctx, self.executor, &self.plan.store, &event)
            .await
            .map_err(Refused::Store)?;
        Ok(Value::record(
            MAGIC_LINK_REDEEMED,
            vec![
                (Rc::from("user_id"), Value::Uuid(Rc::from(format_uuid(&id)))),
                (
                    Rc::from("return_to"),
                    Value::text(confirmed.return_to.as_deref().unwrap_or("/")),
                ),
            ],
        ))
    }

    /// Uses a link's token, once, in a transaction of its own; `given` is
    /// the binding cookie the request sent. A refusal is kept for the log
    /// and published as `SignInRefused`.
    async fn by_token(
        &self,
        ctx: &mut Ctx<'_>,
        given: Option<&str>,
        token: &str,
    ) -> Result<Confirmed, Refused> {
        match self.token_used(ctx, given, token).await {
            Ok(confirmed) => Ok(confirmed),
            Err((why, address)) => {
                let reason = match &why {
                    Refused::Failure(failure, _) if *failure == LINK_OTHER_BROWSER => {
                        SignInRefusal::LinkOtherBrowser
                    }
                    Refused::Failure(failure, _) if *failure == LINK_EXPIRED => {
                        SignInRefusal::LinkExpired
                    }
                    _ => return Err(why),
                };
                Err(self
                    .refusing(ctx, reason, address.as_deref(), None, why)
                    .await)
            }
        }
    }

    /// [`Site::by_token`]'s steps: a refusal comes with the address the
    /// link was sent to, when the link names one.
    async fn token_used(
        &self,
        ctx: &mut Ctx<'_>,
        given: Option<&str>,
        token: &str,
    ) -> Result<Confirmed, (Refused, Option<String>)> {
        let Some(bytes) = token_bytes(token) else {
            return Err((refused(LINK_EXPIRED), None));
        };
        let now = ctx.now();
        let database = self.database(ctx).map_err(|why| (why, None))?;
        let mut transaction = database
            .begin()
            .await
            .map_err(|_| (refused(AUTHENTICATION_UNAVAILABLE), None))?;
        let rows = match transaction
            .query(BY_TOKEN, &[SqlValue::Text(verifier(token.as_bytes()))])
            .await
        {
            Ok(rows) => rows,
            Err(_) => {
                let _ = transaction.rollback().await;
                return Err((refused(AUTHENTICATION_UNAVAILABLE), None));
            }
        };
        let Some([SqlValue::Uuid(id), sealed, return_to, binding, expires]) =
            rows.first().map(Vec::as_slice)
        else {
            let _ = transaction.rollback().await;
            return Err((refused(LINK_EXPIRED), None));
        };
        let unexpired = millis(expires).is_some_and(|at| at > now);
        let SqlValue::Bytes(sealed) = sealed else {
            let _ = transaction.rollback().await;
            return Err((refused(LINK_EXPIRED), None));
        };
        if !unexpired {
            let _ = transaction.rollback().await;
            return Err((refused(LINK_EXPIRED), None));
        }
        // A `same_browser:` link works only in the browser that asked; it
        // is left as it was, so that browser or the code still works.
        if self.magic.same_browser {
            let given = given
                .filter(|value| token_bytes(value).is_some())
                .map(|value| verifier(value.as_bytes()));
            let here = match (binding, given) {
                (SqlValue::Text(bound), Some(given)) => {
                    constant_eq(bound.as_bytes(), given.as_bytes())
                }
                _ => false,
            };
            if !here {
                let _ = transaction.rollback().await;
                return Err((refused(LINK_OTHER_BROWSER), unseal(&bytes, sealed)));
            }
        }
        let Some(address) = unseal(&bytes, sealed) else {
            let _ = transaction.rollback().await;
            return Err((refused(LINK_EXPIRED), None));
        };
        let return_to = match return_to {
            SqlValue::Text(path) => Some(path.clone()),
            _ => None,
        };
        let used = transaction.execute(USED, &[SqlValue::Uuid(*id)]).await;
        if used.is_err() || transaction.commit().await.is_err() {
            return Err((refused(AUTHENTICATION_UNAVAILABLE), None));
        }
        Ok(Confirmed { address, return_to })
    }

    /// Uses an address's code, once, in a transaction of its own; a wrong
    /// one is counted, and stays counted whatever happens after.
    async fn by_code(
        &self,
        ctx: &mut Ctx<'_>,
        address: &str,
        code: &str,
    ) -> Result<Confirmed, Refused> {
        let now = ctx.now();
        let today = now - now.rem_euclid(DAY_MS);
        let database = self.database(ctx)?;
        let mut transaction = database
            .begin()
            .await
            .map_err(|_| refused(AUTHENTICATION_UNAVAILABLE))?;
        let rows = match transaction
            .query(BY_ADDRESS, &[SqlValue::Text(address_digest(address))])
            .await
        {
            Ok(rows) => rows,
            Err(_) => {
                let _ = transaction.rollback().await;
                return Err(refused(AUTHENTICATION_UNAVAILABLE));
            }
        };
        let Some(
            [
                SqlValue::Uuid(id),
                exact,
                asked,
                kept,
                return_to,
                expires,
                SqlValue::Int4(wrong),
                failed_since,
                SqlValue::Int4(failed),
            ],
        ) = rows.first().map(Vec::as_slice)
        else {
            let _ = transaction.rollback().await;
            return Err(self
                .refusing(
                    ctx,
                    SignInRefusal::CodeInvalid,
                    Some(address),
                    None,
                    refused(CODE_INVALID),
                )
                .await);
        };
        // Past the day's wrong codes, codes stop until the next day (UTC);
        // links still work.
        let failed = if millis(failed_since) == Some(today) {
            *failed
        } else {
            0
        };
        if u32::try_from(failed).unwrap_or(u32::MAX) >= self.magic.failed_codes_per_day {
            let _ = transaction.rollback().await;
            return Err(self
                .refusing(
                    ctx,
                    SignInRefusal::CodesDisabled,
                    Some(address),
                    None,
                    Refused::Failure(CODES_DISABLED, Some(seconds(today + DAY_MS - now))),
                )
                .await);
        }
        let right = match (exact, asked, kept) {
            (SqlValue::Text(exact), SqlValue::Uuid(asked), SqlValue::Text(kept)) => {
                let same_address = constant_eq(exact.as_bytes(), exact_digest(address).as_bytes());
                let same_code = constant_eq(kept.as_bytes(), code_verifier(asked, code).as_bytes());
                let unexpired = millis(expires).is_some_and(|at| at > now);
                same_address & same_code & unexpired
            }
            _ => false,
        };
        if !right {
            // Counted against the code, which `code_attempts:` ends, and
            // the address's day; every wrong guess answers the same.
            let wrong = wrong.saturating_add(1);
            let ends = u32::try_from(wrong).unwrap_or(u32::MAX) >= self.magic.code_attempts;
            let counted = transaction
                .execute(
                    WRONG,
                    &[
                        SqlValue::Uuid(*id),
                        SqlValue::Int4(wrong),
                        SqlValue::Bool(ends),
                        micros(today),
                        SqlValue::Int4(failed.saturating_add(1)),
                        micros(today + DAY_MS),
                    ],
                )
                .await;
            if counted.is_err() || transaction.commit().await.is_err() {
                return Err(refused(AUTHENTICATION_UNAVAILABLE));
            }
            // This wrong code reached the day's: codes stop until the next
            // day (UTC), which `SignInCodesDisabled` says.
            let reached = u32::try_from(failed.saturating_add(1)).unwrap_or(u32::MAX)
                >= self.magic.failed_codes_per_day;
            let disabled = SignInEvent::CodesDisabled {
                user: None,
                until: today + DAY_MS,
            };
            if reached && sign_in_events::listens(self.executor, &disabled) {
                let disabled = SignInEvent::CodesDisabled {
                    user: self.user_of(ctx, address).await,
                    until: today + DAY_MS,
                };
                sign_in_events::publish(ctx, self.executor, &self.plan.store, &[disabled]).await;
            }
            return Err(self
                .refusing(
                    ctx,
                    SignInRefusal::CodeInvalid,
                    Some(address),
                    None,
                    refused(CODE_INVALID),
                )
                .await);
        }
        let return_to = match return_to {
            SqlValue::Text(path) => Some(path.clone()),
            _ => None,
        };
        let used = transaction.execute(USED, &[SqlValue::Uuid(*id)]).await;
        if used.is_err() || transaction.commit().await.is_err() {
            return Err(refused(AUTHENTICATION_UNAVAILABLE));
        }
        Ok(Confirmed {
            address: address.to_string(),
            return_to,
        })
    }

    /// The user the address belongs to, made by `on_first_sign_in:`'s
    /// action when none has it, once its `active:` holds. A refusal is
    /// kept for the log and published as `SignInRefused`.
    async fn person(&self, ctx: &mut Ctx<'_>, address: &str) -> Result<Person, Refused> {
        let now = ctx.now();
        let database = self.database(ctx)?;
        let found = database
            .query_once(
                &self.magic.lookup,
                &[SqlValue::Text(address.to_string()), micros(now)],
            )
            .await
            .map_err(|_| refused(AUTHENTICATION_UNAVAILABLE))?;
        let found = match found.first().map(Vec::as_slice) {
            Some([SqlValue::Uuid(id), SqlValue::Bool(active)]) => Some((*id, *active)),
            _ => None,
        };
        let (user, active, first_time) = match found {
            Some((user, active)) => (user, active, false),
            None => {
                let id = match self.first_sign_in(ctx, address).await {
                    Ok(id) => id,
                    Err((why, failure)) => {
                        return Err(match &why {
                            Refused::Failure(answer, _) if *answer == REFUSED => {
                                self.refusing(ctx, SignInRefusal::RefusedByApp, None, failure, why)
                                    .await
                            }
                            _ => why,
                        });
                    }
                };
                let database = self.database(ctx)?;
                let rows = database
                    .query_once(&self.magic.lookup_id, &[id, micros(now)])
                    .await
                    .map_err(|_| refused(AUTHENTICATION_UNAVAILABLE))?;
                match rows.first().map(Vec::as_slice) {
                    Some([SqlValue::Uuid(id), SqlValue::Bool(active)]) => (*id, *active, true),
                    _ => {
                        return Err(self
                            .refusing(
                                ctx,
                                SignInRefusal::RefusedByApp,
                                None,
                                None,
                                refused(REFUSED),
                            )
                            .await);
                    }
                }
            }
        };
        if !active {
            ctx.refuse_sign_in(SignInRefused {
                reason: SignInRefusal::UserDisabled.name(),
                failure: None,
            });
            let event = SignInEvent::Refused {
                method: SignInMethod::MagicLink,
                reason: SignInRefusal::UserDisabled,
                user: Some(user),
            };
            sign_in_events::publish(ctx, self.executor, &self.plan.store, &[event]).await;
            return Err(refused(USER_DISABLED));
        }
        Ok(Person { user, first_time })
    }

    /// Runs `on_first_sign_in:`'s action for an address no user has, as
    /// the system, in its own transaction, whether or not an action of
    /// the application's is running: the `id` it gives. Any failure it
    /// declares, or none declared, is `sign_in_refused`.
    /// A refusal comes with the name of the failure it declared, for the
    /// log.
    async fn first_sign_in(
        &self,
        ctx: &mut Ctx<'_>,
        address: &str,
    ) -> Result<SqlValue, (Refused, Option<String>)> {
        let Some(callable) = self.magic.first_sign_in else {
            return Err((refused(REFUSED), None));
        };
        let method = jadpo_std::SignInMethod::MagicLink;
        let ordinal = jadpo_std::variant_ordinal(jadpo_std::SignInMethod::NAME, method.name())
            .unwrap_or_default();
        let first = Value::record(
            FIRST_SIGN_IN,
            vec![
                (
                    Rc::from("method"),
                    Value::variant(
                        jadpo_std::SignInMethod::NAME,
                        ordinal,
                        method.name(),
                        Vec::new(),
                    ),
                ),
                (Rc::from("address"), Value::text(address)),
                (Rc::from("address_verified"), Value::Bool(true)),
                (Rc::from("subject"), Value::None),
                (Rc::from("name"), Value::None),
            ],
        );
        // The application's own action, when one is running, waits with
        // its transaction while this one runs in its own.
        let waiting = ctx.take_boundary();
        let runner = ctx.runner();
        // As the system, reaching no tenant's records (chunk 7.9b).
        ctx.become_system(System::NO_TENANT);
        let outcome = self.executor.run(ctx, callable, vec![first]).await;
        ctx.restore_runner(runner);
        ctx.give_boundary(waiting);
        match outcome {
            Outcome::Value(id) => {
                crate::to_sql(&id, SqlType::Uuid).map_err(|_| (refused(REFUSED), None))
            }
            Outcome::Failed(failure) if failure.kind.as_deref() == Some("unavailable") => {
                Err((refused(AUTHENTICATION_UNAVAILABLE), None))
            }
            Outcome::Failed(failure) => Err((refused(REFUSED), Some(failure.name.clone()))),
            Outcome::Fault(fault) => Err((Refused::Fault(fault), None)),
            Outcome::Assertion(_) => Err((refused(INTERNAL_FAULT), None)),
        }
    }
}

// --- the application's own routes (chunk 7.3b3) -------------------------------

/// The plan of `plan`'s magic links, or the fault an operation called
/// without them is.
fn site_of<'s>(executor: &'s dyn Executor, plan: Option<&'s IdentityPlan>) -> Option<Site<'s>> {
    let plan = plan?;
    Some(Site {
        plan,
        magic: plan.magic_links.as_ref()?,
        executor,
        request_id: "",
    })
}

fn defect(ctx: &mut Ctx<'_>, what: &str, site: u32) -> Stored {
    Stored::Fault(ctx.fault(FaultKind::Defect(what.to_string()), site))
}

/// What an operation's refusal raises at site `site`: the same failure,
/// code and status the generated routes answer with, never naming the
/// token, the code or the address.
fn raised(ctx: &mut Ctx<'_>, refused: Refused, site: u32) -> Stored {
    match refused {
        Refused::Failure((status, code, message), wait) => {
            let name: String = code
                .split('_')
                .map(|word| {
                    let mut chars = word.chars();
                    chars.next().map_or_else(String::new, |first| {
                        first.to_ascii_uppercase().to_string() + chars.as_str()
                    })
                })
                .collect();
            let mut failure = crate::Failure::new(format!("runtime {code}"), name, code);
            failure.kind = match status {
                401 => Some("unauthenticated"),
                403 => Some("not_permitted"),
                422 => Some("invalid_value"),
                429 => Some("rate_limited"),
                503 => Some("unavailable"),
                _ => None,
            }
            .map(str::to_string);
            failure.status = Some(status);
            failure.message = Some(message.to_string());
            failure.retry_after = wait;
            Stored::Failed(ctx.fail(failure, site))
        }
        Refused::Store(error) => match crate::read::refused(ctx, error, site) {
            crate::read::Interrupt::Failed(failure) => Stored::Failed(failure),
            crate::read::Interrupt::Fault(fault) => Stored::Fault(fault),
        },
        Refused::Fault(fault) => Stored::Fault(fault),
    }
}

/// `magic_links.request(address, locale, return_to)` at site `site` (area
/// 8, "Writing the routes yourself"): asks for a link and a code in the
/// running action's transaction, as `POST <path>` asks in its own, the
/// email sent once the action commits; `rate_limited` over the send
/// limits, alike for every address, and `sign_in_return_invalid` for a
/// path off the application's site. Under `same_browser:` the route's
/// response sets the binding cookie.
pub async fn request(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    plan: Option<&IdentityPlan>,
    args: &[Value],
    site: u32,
) -> Stored {
    let Some(at) = site_of(executor, plan) else {
        return defect(ctx, "a magic link asked for with no `magic_link:`", site);
    };
    let [Value::Text(address), Value::Variant(locale), return_to] = args else {
        return defect(
            ctx,
            "`magic_links.request` called with the wrong arguments",
            site,
        );
    };
    let return_to = match return_to {
        Value::Text(path) => Some(path.to_string()),
        _ => None,
    };
    match at
        .request_in_action(ctx, address, locale.ordinal, return_to.as_deref())
        .await
    {
        Ok(()) => Stored::Value(Value::Unit),
        Err(why) => raised(ctx, why, site),
    }
}

/// `magic_links.redeem(token)` at site `site`: uses a link's token once,
/// as `POST <path>/confirm` does, in a transaction of its own, so a used
/// link stays used whatever the action does next; then the person, made
/// by `on_first_sign_in:`'s action when new. Gives a `MagicLinkRedeemed`.
pub async fn redeem(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    plan: Option<&IdentityPlan>,
    args: &[Value],
    site: u32,
) -> Stored {
    let Some(at) = site_of(executor, plan) else {
        return defect(ctx, "a magic link redeemed with no `magic_link:`", site);
    };
    let [Value::Text(token)] = args else {
        return defect(
            ctx,
            "`magic_links.redeem` called with the wrong arguments",
            site,
        );
    };
    if ctx.asked_sign_in() {
        return defect(
            ctx,
            "`magic_links.redeem` in the action that called `magic_links.request`",
            site,
        );
    }
    let given = ctx.sign_in_cookie().map(str::to_string);
    let confirmed = match at.by_token(ctx, given.as_deref(), token).await {
        Ok(confirmed) => confirmed,
        Err(why) => return raised(ctx, why, site),
    };
    match at.redeemed(ctx, confirmed).await {
        Ok(value) => {
            // Signed in with the link: its binding cookie is cleared.
            if at.magic.same_browser {
                ctx.binding(Binding::Clear);
            }
            Stored::Value(value)
        }
        Err(why) => raised(ctx, why, site),
    }
}

/// `magic_links.redeem_code(address, code)` at site `site`: the same for
/// a code, counting a wrong one against it and the address's day even
/// though the action then fails.
pub async fn redeem_code(
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    plan: Option<&IdentityPlan>,
    args: &[Value],
    site: u32,
) -> Stored {
    let Some(at) = site_of(executor, plan) else {
        return defect(
            ctx,
            "a magic link's code redeemed with no `magic_link:`",
            site,
        );
    };
    let [Value::Text(address), Value::Text(code)] = args else {
        return defect(
            ctx,
            "`magic_links.redeem_code` called with the wrong arguments",
            site,
        );
    };
    if ctx.asked_sign_in() {
        return defect(
            ctx,
            "`magic_links.redeem_code` in the action that called `magic_links.request`",
            site,
        );
    }
    let confirmed = match at.by_code(ctx, address, code).await {
        Ok(confirmed) => confirmed,
        Err(why) => return raised(ctx, why, site),
    };
    match at.redeemed(ctx, confirmed).await {
        Ok(value) => Stored::Value(value),
        Err(why) => raised(ctx, why, site),
    }
}

/// 429 `rate_limited`, waiting `wait_ms`.
fn limited(request_id: &str, wait_ms: i64) -> Response {
    let mut response = runtime_failure(RATE_LIMITED, request_id, None);
    let seconds = (wait_ms + 999) / 1000;
    response
        .headers
        .push(("retry-after".to_string(), seconds.max(1).to_string()));
    response
}

fn ok_json(body: String) -> Response {
    Response {
        status: 200,
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        body: body.into_bytes(),
        ..Response::default()
    }
}

/// A text member, `null` and left out alike.
fn text(body: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<String> {
    match body.get(key) {
        Some(serde_json::Value::String(text)) => Some(text.clone()),
        _ => None,
    }
}

/// Whether `path` is a path on the application's own site, which a
/// browser can't read as another host: one `/`, then printable ASCII with
/// no `\`, at most [`RETURN_TO_MAX`] bytes.
pub(crate) fn site_path(path: &str) -> bool {
    path.len() <= RETURN_TO_MAX
        && path.starts_with('/')
        && !path.starts_with("//")
        && path.bytes().all(|b| b.is_ascii_graphic() && b != b'\\')
}

/// A locale by its variant name (`fr_fr`) or its BCP 47 tag (`fr-FR`).
fn locale_named(asked: &str) -> Option<u32> {
    jadpo_std::locale_ordinal(asked).or_else(|| {
        jadpo_std::LOCALES
            .iter()
            .position(|(_, tag)| tag.eq_ignore_ascii_case(asked))
            .and_then(|at| u32::try_from(at).ok())
    })
}

/// What an address is counted as: the digest of it in lower case.
fn address_digest(address: &str) -> String {
    verifier(format!("address {}", address.to_lowercase()).as_bytes())
}

/// What an address as written is kept as: its digest.
fn exact_digest(address: &str) -> String {
    verifier(format!("exact {address}").as_bytes())
}

/// What a code is kept as: its digest keyed by its request, so the same
/// code for two requests is kept as two values.
fn code_verifier(request: &[u8; 16], code: &str) -> String {
    let mut hasher = blake3::Hasher::new_derive_key("jadpo 2026-10-20 sign-in code");
    hasher.update(request);
    hasher.update(code.as_bytes());
    hasher.finalize().to_hex().to_string()
}

/// The BLAKE3 digest of `bytes`, in hexadecimal.
fn verifier(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// The 32 bytes a link's token is, as 64 lower-case hexadecimal digits.
fn token_bytes(token: &str) -> Option<[u8; 32]> {
    if token.len() != 64
        || !token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    let bytes = jadpo_value::encoding::from_hex(token)?;
    <[u8; 32]>::try_from(bytes.as_slice()).ok()
}

/// 32 random bytes as 64 hexadecimal digits, never on a recording's tape.
fn new_token(ctx: &mut Ctx<'_>) -> Option<String> {
    let mut bytes = [0u8; 32];
    if !ctx.secret_random_bytes(&mut bytes) {
        return None;
    }
    Some(jadpo_value::encoding::to_hex(&bytes))
}

/// Six random digits, each drawn evenly.
fn new_code(ctx: &mut Ctx<'_>) -> Option<String> {
    let mut code = String::with_capacity(6);
    while code.len() < 6 {
        let mut byte = [0u8; 1];
        if !ctx.secret_random_bytes(&mut byte) {
            return None;
        }
        // 250 is the largest multiple of 10 a byte holds.
        if byte[0] < 250 {
            code.push(char::from(b'0' + byte[0] % 10));
        }
    }
    Some(code)
}

/// The key stream an address is sealed with under a link's token.
fn stream(token: &[u8; 32], length: usize) -> Vec<u8> {
    let key = blake3::derive_key("jadpo 2026-10-20 sign-in address", token);
    let mut out = vec![0u8; length];
    blake3::Hasher::new_keyed(&key)
        .finalize_xof()
        .fill(&mut out);
    out
}

/// `address` sealed under `token`, which only the link opens.
fn seal(token: &[u8; 32], address: &str) -> Vec<u8> {
    let bytes = address.as_bytes();
    stream(token, bytes.len())
        .iter()
        .zip(bytes)
        .map(|(key, byte)| key ^ byte)
        .collect()
}

fn unseal(token: &[u8; 32], sealed: &[u8]) -> Option<String> {
    let bytes: Vec<u8> = stream(token, sealed.len())
        .iter()
        .zip(sealed)
        .map(|(key, byte)| key ^ byte)
        .collect();
    String::from_utf8(bytes)
        .ok()
        .filter(|address| jadpo_std::is_email(address))
}

/// The value of cookie `name`, when the request sends it exactly once.
pub(crate) fn cookie_value<'r>(request: &'r Request, name: &str) -> Option<&'r str> {
    let mut found = request
        .headers
        .iter()
        .filter(|(header, _)| header.eq_ignore_ascii_case("cookie"))
        .flat_map(|(_, value)| value.split(';'))
        .filter_map(|pair| {
            let (key, value) = pair.trim().split_once('=')?;
            (key.trim() == name).then(|| value.trim())
        });
    let first = found.next()?;
    found.next().is_none().then_some(first)
}

fn micros(millis: i64) -> SqlValue {
    SqlValue::Timestamp(millis.saturating_mul(1000))
}

/// A `timestamptz` column's value in milliseconds.
fn millis(value: &SqlValue) -> Option<i64> {
    match value {
        SqlValue::Timestamp(micros) => Some(micros.div_euclid(1000)),
        _ => None,
    }
}

pub(crate) fn format_uuid(bytes: &[u8; 16]) -> String {
    let mut out = String::with_capacity(36);
    for (at, byte) in bytes.iter().enumerate() {
        if matches!(at, 4 | 6 | 8 | 10) {
            out.push('-');
        }
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

pub(crate) fn parse_uuid(text: &str) -> Option<[u8; 16]> {
    let hex: String = text.chars().filter(|c| *c != '-').collect();
    if text.len() != 36 || hex.len() != 32 {
        return None;
    }
    let bytes = jadpo_value::encoding::from_hex(&hex)?;
    <[u8; 16]>::try_from(bytes.as_slice()).ok()
}

// --- Jadpo's own email ---------------------------------------------------------

/// The subscriber key the magic-link email's delivery names.
pub(crate) fn subscriber_key() -> String {
    format!("email {MAGIC_LINK_EMAIL}")
}

/// The email's value as its delivery reads it: the address, the locale and
/// the request, nothing secret.
fn email_value(address: &str, locale: u32, request: [u8; 16]) -> Value {
    Value::record(
        MAGIC_LINK_EMAIL,
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

/// The email's value from its outbox payload, as the sweep reads a
/// delivery back.
pub(crate) fn read_payload(payload: &str) -> Result<Value, String> {
    let Ok(serde_json::Value::Object(object)) = serde_json::from_str::<serde_json::Value>(payload)
    else {
        return Err("not the magic-link email's payload".to_string());
    };
    let field = |key: &str| object.get(key).and_then(serde_json::Value::as_str);
    let (Some(to), Some(locale), Some(request)) = (
        field("to"),
        field("locale").and_then(jadpo_std::locale_ordinal),
        field("request").and_then(parse_uuid),
    ) else {
        return Err("not the magic-link email's payload".to_string());
    };
    Ok(email_value(to, locale, request))
}

/// What the email carries, made as it is sent: the link, the code, and
/// when they stop working. `None` when its request was used, replaced or
/// has expired, and nothing is sent.
pub(crate) struct Minted {
    pub address: String,
    pub link: String,
    pub code: String,
    /// When the link and the code stop working, in milliseconds.
    pub expires_at: i64,
}

/// Makes the link and the code of the request `value` names, keeping only
/// their verifiers and the address sealed under the link's token. `Err`
/// with what the delivery is settled with when it can't.
pub(crate) async fn mint(
    ctx: &mut Ctx<'_>,
    plan: &MagicLinkEmailPlan,
    value: &Value,
) -> Result<Option<Minted>, &'static str> {
    let Value::Record(record) = value else {
        return Err("a magic-link email that isn't a record");
    };
    let (Some(Value::Text(address)), Some(Value::Text(request))) =
        (record.get("to"), record.get("request"))
    else {
        return Err("a magic-link email without its address or request");
    };
    let Some(request) = parse_uuid(request) else {
        return Err("a magic-link email whose request isn't an id");
    };
    let origin = match ctx.setting(plan.origin) {
        Some(Value::Text(origin)) => super::signed::origin_of(origin),
        _ => None,
    };
    let Some(origin) = origin else {
        return Err("a magic-link email whose origin setting has no value");
    };
    let (Some(token), Some(code)) = (new_token(ctx), new_code(ctx)) else {
        return Err("a magic-link email with no randomness");
    };
    let Some(bytes) = token_bytes(&token) else {
        return Err("a magic-link token that isn't one");
    };
    let Some(database) = ctx.host().database(&plan.store) else {
        return Err("a magic-link email with no store for its requests");
    };
    let now = ctx.now();
    let rows = database
        .query_once(
            MINT,
            &[
                SqlValue::Uuid(request),
                SqlValue::Text(verifier(token.as_bytes())),
                SqlValue::Text(code_verifier(&request, &code)),
                SqlValue::Bytes(seal(&bytes, address)),
                micros(now),
            ],
        )
        .await
        .map_err(|_| "unavailable")?;
    let Some(expires_at) = rows.first().and_then(|row| row.first()).and_then(millis) else {
        return Ok(None);
    };
    Ok(Some(Minted {
        address: address.to_string(),
        link: format!("{origin}{}?token={token}", plan.link),
        code,
        expires_at,
    }))
}

/// The `MagicLinkRequest` the action `magic_link: { send: }` names is
/// given (area 16, "Built-in emails", level 5), made as its delivery runs:
/// the link and the code made now, when they stop working, and the
/// locale the request chose. `Ok(None)` when the request was used,
/// replaced or has expired since, and nothing is sent.
pub(crate) async fn request_value(
    ctx: &mut Ctx<'_>,
    plan: &MagicLinkEmailPlan,
    value: &Value,
) -> Result<Option<Value>, &'static str> {
    let locale = match value {
        Value::Record(record) => record.get("locale").cloned(),
        _ => None,
    };
    let Some(locale @ Value::Variant(_)) = locale else {
        return Err("a magic-link email with no locale");
    };
    let Some(minted) = mint(ctx, plan, value).await? else {
        return Ok(None);
    };
    Ok(Some(Value::record(
        jadpo_std::email::MAGIC_LINK_REQUEST,
        vec![
            (Rc::from("address"), Value::text(&minted.address)),
            (Rc::from("sign_in_link"), Value::text(&minted.link)),
            (Rc::from("code"), Value::text(&minted.code)),
            (Rc::from("expires_at"), Value::Instant(minted.expires_at)),
            (Rc::from("locale"), locale),
        ],
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_sealed_under_a_token_opens_only_with_it() {
        let token = [7u8; 32];
        let sealed = seal(&token, "ada@example.com");
        assert_ne!(sealed, b"ada@example.com".to_vec());
        assert_eq!(unseal(&token, &sealed).as_deref(), Some("ada@example.com"));
        assert_eq!(unseal(&[8u8; 32], &sealed), None);
    }

    #[test]
    fn a_return_path_stays_on_the_site() {
        assert!(site_path("/notes"));
        assert!(site_path("/notes?tab=1#top"));
        for path in [
            "//evil.example",
            "https://evil.example",
            "/\\evil.example",
            "notes",
            "/a b",
            "",
        ] {
            assert!(!site_path(path), "{path}");
        }
    }

    #[test]
    fn a_code_is_kept_per_request_and_an_address_in_lower_case() {
        assert_ne!(
            code_verifier(&[1; 16], "042317"),
            code_verifier(&[2; 16], "042317")
        );
        assert_eq!(
            address_digest("Ada@Example.com"),
            address_digest("ada@example.com")
        );
        assert_ne!(
            exact_digest("Ada@Example.com"),
            exact_digest("ada@example.com")
        );
    }

    #[test]
    fn the_payload_holds_nothing_secret_and_reads_back() {
        let written = payload(
            "ada@example.com",
            jadpo_std::locale_ordinal("en_gb").unwrap(),
            [3; 16],
        );
        assert!(
            !written.contains("token") && !written.contains("code"),
            "{written}"
        );
        let read = read_payload(&written).expect("reads");
        assert_eq!(
            read,
            email_value(
                "ada@example.com",
                jadpo_std::locale_ordinal("en_gb").unwrap(),
                [3; 16]
            )
        );
    }

    #[test]
    fn a_token_is_64_lower_case_hexadecimal_digits() {
        assert!(token_bytes(&"a".repeat(64)).is_some());
        assert!(token_bytes(&"A".repeat(64)).is_none());
        assert!(token_bytes(&"a".repeat(63)).is_none());
    }
}
