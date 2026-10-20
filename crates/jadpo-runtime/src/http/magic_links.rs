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
//! Nothing here logs or answers with a token, a code or an address.

use std::rc::Rc;

use jadpo_std::email::{MAGIC_LINK_EMAIL, MagicLinkEmailPlan};
use jadpo_std::http::{
    API_TOKEN_PREFIX, FIRST_SIGN_IN, IdentityPlan, MagicLinkPlan, MagicLinkStep, RETURN_TO_MAX,
    SESSION_LIFETIME_MS,
};
use jadpo_std::read::SqlType;
use jadpo_value::Value;

use super::identity::{
    AUTHENTICATION_MISCONFIGURED, AUTHENTICATION_UNAVAILABLE, CSRF_REJECTED, START, USER_DISABLED,
    set_cookies,
};
use super::{
    App, INTERNAL_FAULT, Problem, RATE_LIMITED, Request, Response, invalid_input, runtime_failure,
};
use crate::ctx::{SessionEffect, System};
use crate::outbox::{Delivery, Sealed};
use crate::runner::Deferred;
use crate::{Ctx, Executor, Outcome, SqlValue};

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
fn binding_cookie(plan: &IdentityPlan) -> Option<String> {
    plan.cookie
        .as_ref()
        .map(|cookie| format!("{cookie}_sign_in"))
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
    let (Some(plan), Some(origin)) = (app.table.identity.as_ref(), app.auth.origin()) else {
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
        origin,
        executor,
        request_id,
    };
    match step {
        MagicLinkStep::Request => site.request(ctx, request).await,
        MagicLinkStep::Confirm => site.confirm(ctx, request, false).await,
        MagicLinkStep::Redeem => site.confirm(ctx, request, true).await,
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
    origin: &'s str,
    executor: &'s dyn Executor,
    request_id: &'s str,
}

/// What a confirmed link or code gives: the address it was sent to and
/// where to go.
struct Confirmed {
    address: String,
    return_to: Option<String>,
}

impl Site<'_> {
    fn fail(&self, failure: (u16, &str, &str)) -> Stop {
        stop(failure, self.request_id)
    }

    fn unavailable(&self) -> Stop {
        self.fail(AUTHENTICATION_UNAVAILABLE)
    }

    fn database<'c>(&self, ctx: &Ctx<'c>) -> Result<&'c dyn crate::Database, Stop> {
        ctx.host()
            .database(&self.plan.store)
            .ok_or_else(|| self.fail(AUTHENTICATION_MISCONFIGURED))
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
        let email = self
            .executor
            .events()
            .and_then(|events| events.plan.emails.email(MAGIC_LINK_EMAIL).cloned());
        let locales = email
            .as_ref()
            .and_then(|email| email.builtin.as_ref())
            .map(|jadpo_std::email::BuiltinEmail::MagicLink(mail)| mail.locales.clone());
        let locale = locales.as_ref().map(|locales| {
            text(&body, "locale")
                .and_then(|asked| locale_named(&asked))
                .filter(|ordinal| locales.supports(*ordinal))
                .unwrap_or_else(|| {
                    jadpo_value::locale::from_accept_language(
                        request.header("accept-language"),
                        locales,
                    )
                })
        });
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
        let database = self.database(ctx)?;
        let mut transaction = database.begin().await.map_err(|_| self.unavailable())?;
        let digest = address_digest(&address);
        let expires = now.saturating_add(self.magic.expires_after_ms);
        let steps: Result<Option<Response>, crate::StoreError> = async {
            transaction.execute(SWEEP, &[micros(now)]).await?;
            transaction
                .execute(
                    ENSURE,
                    &[
                        SqlValue::Uuid(row),
                        SqlValue::Text(digest.clone()),
                        micros(now.saturating_add(HOUR_MS)),
                    ],
                )
                .await?;
            let rows = transaction
                .query(LOCK, &[SqlValue::Text(digest.clone())])
                .await?;
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
                let next = at.saturating_add(self.magic.resend_after_ms);
                if next > now {
                    return Ok(Some(limited(self.request_id, next - now)));
                }
            }
            let (since, sends) = match millis(sends_since) {
                Some(since) if since.saturating_add(HOUR_MS) > now => {
                    if u32::try_from(*sends).unwrap_or(u32::MAX) >= self.magic.codes_per_hour {
                        return Ok(Some(limited(
                            self.request_id,
                            since.saturating_add(HOUR_MS) - now,
                        )));
                    }
                    (since, sends.saturating_add(1))
                }
                _ => (now, 1),
            };
            // Whether an email goes: to an account, or to an address a
            // first sign-in may make one for.
            let found = transaction
                .query(
                    &self.magic.lookup,
                    &[SqlValue::Text(address.clone()), micros(now)],
                )
                .await?;
            let sends_email = !found.is_empty() || self.magic.first_sign_in.is_some();
            transaction
                .execute(
                    ASKED,
                    &[
                        SqlValue::Uuid(*id),
                        SqlValue::Uuid(asked),
                        SqlValue::Text(exact_digest(&address)),
                        return_to
                            .clone()
                            .map_or(SqlValue::Null(SqlType::Text), SqlValue::Text),
                        binding
                            .as_deref()
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
            if sends_email {
                let (Some(email), Some(locale)) = (&email, locale) else {
                    return Err(crate::StoreError::Unavailable);
                };
                let _ = email;
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
                return Err(self.unavailable());
            }
        }
        if transaction.commit().await.is_err() {
            let _ = ctx.take_deferred();
            return Err(self.unavailable());
        }
        let mut response = Response {
            status: 202,
            ..Response::default()
        };
        if let (Some(token), Some(cookie)) = (binding, binding_cookie(self.plan)) {
            let age = self.magic.expires_after_ms / 1000;
            response.headers.push((
                "set-cookie".to_string(),
                format!("{cookie}={token}; Path=/; Max-Age={age}; HttpOnly; Secure; SameSite=Lax"),
            ));
        }
        Ok(response)
    }

    // --- confirming -------------------------------------------------------------

    /// A link's token or a code, then the person: a browser session, or an
    /// API token when `redeem`.
    async fn confirm(
        &self,
        ctx: &mut Ctx<'_>,
        request: &Request,
        redeem: bool,
    ) -> Result<Response, Stop> {
        let mut clear_binding = false;
        let confirmed = if redeem {
            let body = self.body(request, &["address", "code"])?;
            let address = self.address(&body)?;
            let Some(code) = text(&body, "code") else {
                return Err(Box::new(problem(self.request_id, "code", "required")));
            };
            self.by_code(ctx, &address, &code).await?
        } else {
            // A browser session starts only from the application's own site.
            let from = request.header("origin").and_then(super::signed::origin_of);
            if from.as_deref() != Some(self.origin) {
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
                    self.by_token(ctx, request, &token).await?
                }
                (None, Some(_), Some(code)) => {
                    let address = self.address(&body)?;
                    self.by_code(ctx, &address, &code).await?
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
        let user = self.person(ctx, &confirmed.address).await?;
        let now = ctx.now();
        let (Some(token), Some(id)) = (new_token(ctx), ctx.new_id()) else {
            return Err(self.fail(AUTHENTICATION_MISCONFIGURED));
        };
        let database = self.database(ctx)?;
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
            database
                .query_once(START, &params)
                .await
                .map_err(|_| self.unavailable())?;
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
        database
            .query_once(START, &params)
            .await
            .map_err(|_| self.unavailable())?;
        let mut body = String::from("{\"return_to\":");
        jadpo_value::json::write_string(confirmed.return_to.as_deref().unwrap_or("/"), &mut body);
        body.push('}');
        let mut response = ok_json(body);
        response.headers.extend(set_cookies(
            self.plan,
            &SessionEffect::Start { token, csrf },
        ));
        if clear_binding && let Some(cookie) = binding_cookie(self.plan) {
            response.headers.push((
                "set-cookie".to_string(),
                format!("{cookie}=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Lax"),
            ));
        }
        Ok(response)
    }

    /// Uses a link's token, once.
    async fn by_token(
        &self,
        ctx: &mut Ctx<'_>,
        request: &Request,
        token: &str,
    ) -> Result<Confirmed, Stop> {
        let Some(bytes) = token_bytes(token) else {
            return Err(self.fail(LINK_EXPIRED));
        };
        let now = ctx.now();
        let database = self.database(ctx)?;
        let mut transaction = database.begin().await.map_err(|_| self.unavailable())?;
        let rows = match transaction
            .query(BY_TOKEN, &[SqlValue::Text(verifier(token.as_bytes()))])
            .await
        {
            Ok(rows) => rows,
            Err(_) => {
                let _ = transaction.rollback().await;
                return Err(self.unavailable());
            }
        };
        let Some([SqlValue::Uuid(id), sealed, return_to, binding, expires]) =
            rows.first().map(Vec::as_slice)
        else {
            let _ = transaction.rollback().await;
            return Err(self.fail(LINK_EXPIRED));
        };
        let unexpired = millis(expires).is_some_and(|at| at > now);
        let SqlValue::Bytes(sealed) = sealed else {
            let _ = transaction.rollback().await;
            return Err(self.fail(LINK_EXPIRED));
        };
        if !unexpired {
            let _ = transaction.rollback().await;
            return Err(self.fail(LINK_EXPIRED));
        }
        // A `same_browser:` link works only in the browser that asked; it
        // is left as it was, so that browser or the code still works.
        if self.magic.same_browser {
            let given = binding_cookie(self.plan)
                .and_then(|cookie| cookie_value(request, &cookie))
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
                return Err(self.fail(LINK_OTHER_BROWSER));
            }
        }
        let Some(address) = unseal(&bytes, sealed) else {
            let _ = transaction.rollback().await;
            return Err(self.fail(LINK_EXPIRED));
        };
        let return_to = match return_to {
            SqlValue::Text(path) => Some(path.clone()),
            _ => None,
        };
        let used = transaction.execute(USED, &[SqlValue::Uuid(*id)]).await;
        if used.is_err() || transaction.commit().await.is_err() {
            return Err(self.unavailable());
        }
        Ok(Confirmed { address, return_to })
    }

    /// Uses an address's code, once; a wrong one is counted.
    async fn by_code(
        &self,
        ctx: &mut Ctx<'_>,
        address: &str,
        code: &str,
    ) -> Result<Confirmed, Stop> {
        let now = ctx.now();
        let today = now - now.rem_euclid(DAY_MS);
        let database = self.database(ctx)?;
        let mut transaction = database.begin().await.map_err(|_| self.unavailable())?;
        let rows = match transaction
            .query(BY_ADDRESS, &[SqlValue::Text(address_digest(address))])
            .await
        {
            Ok(rows) => rows,
            Err(_) => {
                let _ = transaction.rollback().await;
                return Err(self.unavailable());
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
            return Err(self.fail(CODE_INVALID));
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
            let mut response = runtime_failure(CODES_DISABLED, self.request_id, None);
            let wait = (today + DAY_MS - now + 999) / 1000;
            response
                .headers
                .push(("retry-after".to_string(), wait.max(1).to_string()));
            return Err(Box::new(response));
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
                return Err(self.unavailable());
            }
            return Err(self.fail(CODE_INVALID));
        }
        let return_to = match return_to {
            SqlValue::Text(path) => Some(path.clone()),
            _ => None,
        };
        let used = transaction.execute(USED, &[SqlValue::Uuid(*id)]).await;
        if used.is_err() || transaction.commit().await.is_err() {
            return Err(self.unavailable());
        }
        Ok(Confirmed {
            address: address.to_string(),
            return_to,
        })
    }

    /// The user the address belongs to, made by `on_first_sign_in:`'s
    /// action when none has it: its `id`, once its `active:` holds.
    async fn person(&self, ctx: &mut Ctx<'_>, address: &str) -> Result<SqlValue, Stop> {
        let now = ctx.now();
        let database = self.database(ctx)?;
        let found = database
            .query_once(
                &self.magic.lookup,
                &[SqlValue::Text(address.to_string()), micros(now)],
            )
            .await
            .map_err(|_| self.unavailable())?;
        let found = match found.first().map(Vec::as_slice) {
            Some([id @ SqlValue::Uuid(_), SqlValue::Bool(active)]) => Some((id.clone(), *active)),
            _ => None,
        };
        let (user, active) = match found {
            Some(found) => found,
            None => {
                let id = self.first_sign_in(ctx, address).await?;
                let database = self.database(ctx)?;
                let rows = database
                    .query_once(&self.magic.lookup_id, &[id, micros(now)])
                    .await
                    .map_err(|_| self.unavailable())?;
                match rows.first().map(Vec::as_slice) {
                    Some([id @ SqlValue::Uuid(_), SqlValue::Bool(active)]) => (id.clone(), *active),
                    _ => return Err(self.fail(REFUSED)),
                }
            }
        };
        if !active {
            return Err(self.fail(USER_DISABLED));
        }
        Ok(user)
    }

    /// Runs `on_first_sign_in:`'s action for an address no user has, as
    /// the system, in its own transaction: the `id` it gives. Any failure
    /// it declares, or none declared, is `sign_in_refused`.
    async fn first_sign_in(&self, ctx: &mut Ctx<'_>, address: &str) -> Result<SqlValue, Stop> {
        let Some(callable) = self.magic.first_sign_in else {
            return Err(self.fail(REFUSED));
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
        ctx.become_system(System { tenant: None });
        let outcome = self.executor.run(ctx, callable, vec![first]).await;
        ctx.set_caller(None);
        match outcome {
            Outcome::Value(id) => crate::to_sql(&id, SqlType::Uuid).map_err(|_| self.fail(REFUSED)),
            Outcome::Failed(failure) if failure.kind.as_deref() == Some("unavailable") => {
                Err(self.unavailable())
            }
            Outcome::Failed(_) => Err(self.fail(REFUSED)),
            Outcome::Fault(fault) => {
                let mut response = runtime_failure(INTERNAL_FAULT, self.request_id, None);
                response.fault = Some(fault);
                Err(Box::new(response))
            }
            Outcome::Assertion(_) => Err(self.fail(INTERNAL_FAULT)),
        }
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
fn cookie_value<'r>(request: &'r Request, name: &str) -> Option<&'r str> {
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

fn format_uuid(bytes: &[u8; 16]) -> String {
    let mut out = String::with_capacity(36);
    for (at, byte) in bytes.iter().enumerate() {
        if matches!(at, 4 | 6 | 8 | 10) {
            out.push('-');
        }
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn parse_uuid(text: &str) -> Option<[u8; 16]> {
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
    executor: &dyn Executor,
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
    let origin = match ctx
        .setting(plan.origin)
        .cloned()
        .or_else(|| executor.setting(plan.origin))
    {
        Some(Value::Text(origin)) => super::signed::origin_of(&origin),
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
    executor: &dyn Executor,
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
    let Some(minted) = mint(executor, ctx, plan, value).await? else {
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
