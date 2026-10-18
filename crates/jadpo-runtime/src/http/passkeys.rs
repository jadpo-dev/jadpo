//! Passkeys (area 8, "Passkeys"; chunk 5.5z): the routes `sign_in: {
//! passkey: }` generates, answered by the runtime for every host and
//! executor, with the ceremonies themselves on the host's [`Passkeys`]
//! capability (`webauthn-rs` natively).
//!
//! - **Challenges** are made by the ceremony's options route and kept in
//!   `jadpo_passkey_challenges` with the state the answer is checked
//!   against: server-side, found by the challenge's verifier, used once
//!   (taken with `DELETE … RETURNING` before the answer is checked, so a
//!   replay finds nothing), expiring after `expires_after:`, and bound to
//!   the browser that asked (a `<cookie>_passkey` cookie's verifier) or,
//!   when adding a passkey, the session. A new options request replaces
//!   the binding's earlier challenge; expired ones are swept as new ones
//!   are made.
//! - **The relying party** is the browser session's configured origin,
//!   never the request's; a public write needs an `Origin` naming it.
//! - **Passkeys** are kept in `jadpo_passkeys` as `WebAuthnCredential`
//!   JSON with their user and a random user handle; signing in updates
//!   the counter, backup state and last use only if the counter is still
//!   what was read, then starts a browser session in the same
//!   transaction.
//!
//! Every failure is one of area 8's: `passkey_challenge_expired`,
//! `passkey_rejected`, `passkey_already_registered`,
//! `passkey_limit_reached`, `passkey_not_found`, or the usual
//! `invalid_input`, `csrf_rejected`, `user_disabled`,
//! `authentication_unavailable` and `authentication_misconfigured`.

use jadpo_std::http::{IdentityPlan, PASSKEY_LIMIT, PasskeyPlan, PasskeyStep, SESSION_LIFETIME_MS};
use jadpo_std::read::SqlType;
use jadpo_value::encoding::{from_base64, to_base64};

use super::identity::{
    AUTHENTICATION_MISCONFIGURED, AUTHENTICATION_UNAVAILABLE, CSRF_REJECTED, START, USER_DISABLED,
    set_cookies,
};
use super::{App, Problem, Request, Response, invalid_input, runtime_failure};
use crate::ctx::{Caller, SessionEffect};
use crate::{Ctx, PasskeyParts, Passkeys, SqlValue, StoreError};

pub(crate) const CHALLENGE_EXPIRED: (u16, &str, &str) = (
    401,
    "passkey_challenge_expired",
    "The passkey request expired or was already used.",
);
pub(crate) const REJECTED: (u16, &str, &str) =
    (401, "passkey_rejected", "The passkey couldn't be checked.");
pub(crate) const ALREADY_REGISTERED: (u16, &str, &str) = (
    409,
    "passkey_already_registered",
    "This passkey is registered already.",
);
pub(crate) const LIMIT_REACHED: (u16, &str, &str) = (
    409,
    "passkey_limit_reached",
    "This account has as many passkeys as it can hold.",
);
pub(crate) const NOT_FOUND: (u16, &str, &str) =
    (404, "passkey_not_found", "No passkey of yours has this id.");

/// Keeps a binding's new challenge, replacing its earlier one.
const PUT_CHALLENGE: &str = "INSERT INTO \"jadpo_passkey_challenges\" (\"id\", \"challenge\", \"purpose\", \"binding\", \"principal_id\", \"user_handle\", \"state\", \"expires_at\") VALUES ($1, $2, $3, $4, $5, $6, $7, $8) ON CONFLICT (\"purpose\", \"binding\") DO UPDATE SET \"id\" = EXCLUDED.\"id\", \"challenge\" = EXCLUDED.\"challenge\", \"principal_id\" = EXCLUDED.\"principal_id\", \"user_handle\" = EXCLUDED.\"user_handle\", \"state\" = EXCLUDED.\"state\", \"expires_at\" = EXCLUDED.\"expires_at\"";

/// Forgets the challenges that have expired.
const SWEEP: &str = "DELETE FROM \"jadpo_passkey_challenges\" WHERE \"expires_at\" <= $1";

/// Takes a challenge, once: whoever deletes it has it.
const TAKE: &str = "DELETE FROM \"jadpo_passkey_challenges\" WHERE \"challenge\" = $1 AND \"purpose\" = $2 RETURNING \"binding\", \"principal_id\", \"user_handle\", \"state\", \"expires_at\"";

/// A person's passkeys, oldest first.
const LIST: &str = "SELECT \"id\", \"credential\" FROM \"jadpo_passkeys\" WHERE \"principal_id\" = $1 ORDER BY \"registered_at\", \"id\"";

/// Keeps a new passkey while its person has fewer than the limit.
const ADD: &str = "INSERT INTO \"jadpo_passkeys\" (\"id\", \"principal_id\", \"credential_id\", \"user_handle\", \"credential\", \"registered_at\") SELECT $1, $2, $3, $4, $5, $6 WHERE (SELECT count(*) FROM \"jadpo_passkeys\" WHERE \"principal_id\" = $2) < $7";

/// Writes a passkey after it signed in, if its counter is still the one
/// read: a second answer racing it changes nothing.
const USED: &str = "UPDATE \"jadpo_passkeys\" SET \"credential\" = $3 WHERE \"id\" = $1 AND \"credential\" ->> 'sign_count' = $2";

/// Renames one of a person's passkeys.
const RENAME: &str = "UPDATE \"jadpo_passkeys\" SET \"credential\" = jsonb_set(\"credential\", '{name}', to_jsonb($3::text)) WHERE \"id\" = $1 AND \"principal_id\" = $2 RETURNING \"id\", \"credential\"";

/// Removes one of a person's passkeys.
const REMOVE: &str = "DELETE FROM \"jadpo_passkeys\" WHERE \"id\" = $1 AND \"principal_id\" = $2";

/// What a challenge is kept for.
const SIGN_IN: &str = "sign_in";
const REGISTER: &str = "register";

/// Why a step stopped: the response that says so.
type Stop = Box<Response>;

fn stop(failure: (u16, &str, &str), request_id: &str) -> Stop {
    Box::new(runtime_failure(failure, request_id, None))
}

/// The cookie binding a sign-in challenge to its browser: `<cookie>_passkey`.
fn binding_cookie(plan: &IdentityPlan) -> Option<String> {
    plan.cookie
        .as_ref()
        .map(|cookie| format!("{cookie}_passkey"))
}

/// Answers `step` for `caller` (none on the sign-in routes).
pub(crate) async fn answer(
    app: &App<'_>,
    ctx: &mut Ctx<'_>,
    request: &Request,
    request_id: &str,
    step: PasskeyStep,
    segments: &[String],
    caller: Option<&Caller>,
) -> Response {
    let mut response = match run(app, ctx, request, request_id, step, segments, caller).await {
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
    ctx: &mut Ctx<'_>,
    request: &Request,
    request_id: &str,
    step: PasskeyStep,
    segments: &[String],
    caller: Option<&Caller>,
) -> Result<Response, Stop> {
    let (Some(plan), Some(origin)) = (app.table.identity.as_ref(), app.auth.origin()) else {
        return Err(stop(AUTHENTICATION_MISCONFIGURED, request_id));
    };
    let Some(passkeys) = plan.passkeys.as_ref() else {
        return Err(stop(AUTHENTICATION_MISCONFIGURED, request_id));
    };
    let Some(ceremonies) = ctx.host().passkeys() else {
        return Err(stop(AUTHENTICATION_MISCONFIGURED, request_id));
    };
    if request
        .query
        .as_deref()
        .is_some_and(|query| !query.is_empty())
    {
        return Err(Box::new(unknown_field(request_id, "")));
    }
    let site = Site {
        plan,
        passkeys,
        origin,
        ceremonies,
        request_id,
    };
    match step {
        PasskeyStep::SignInOptions => site.sign_in_options(ctx, request).await,
        PasskeyStep::SignIn => site.sign_in(ctx, request).await,
        PasskeyStep::RegisterOptions => site.register_options(ctx, request, caller).await,
        PasskeyStep::Register => site.register(ctx, request, caller).await,
        PasskeyStep::List => site.list(ctx, request, caller).await,
        PasskeyStep::Rename => site.rename(ctx, request, segments, caller).await,
        PasskeyStep::Remove => site.remove(ctx, request, segments, caller).await,
    }
}

/// What every step reads: the plan, the configured origin, the host's
/// ceremonies and the request's id.
struct Site<'s> {
    plan: &'s IdentityPlan,
    passkeys: &'s PasskeyPlan,
    origin: &'s str,
    ceremonies: &'s dyn Passkeys,
    request_id: &'s str,
}

impl Site<'_> {
    fn timeout_ms(&self) -> u32 {
        u32::try_from(self.passkeys.expires_after_ms).unwrap_or(u32::MAX)
    }

    fn fail(&self, failure: (u16, &str, &str)) -> Stop {
        stop(failure, self.request_id)
    }

    /// One statement on its own, against the store the user lives in.
    async fn once(
        &self,
        ctx: &Ctx<'_>,
        sql: &str,
        params: &[SqlValue],
    ) -> Result<Vec<crate::Row>, Stop> {
        let Some(database) = ctx.host().database(&self.plan.store) else {
            return Err(self.fail(AUTHENTICATION_MISCONFIGURED));
        };
        database
            .query_once(sql, params)
            .await
            .map_err(|_| self.fail(AUTHENTICATION_UNAVAILABLE))
    }

    /// A public write must come from the application's own site.
    fn same_site(&self, request: &Request) -> Result<(), Stop> {
        let from = request.header("origin").and_then(super::signed::origin_of);
        if from.as_deref() == Some(self.origin) {
            Ok(())
        } else {
            Err(self.fail(CSRF_REJECTED))
        }
    }

    /// A body that must be empty.
    fn no_body(&self, request: &Request) -> Result<(), Stop> {
        if request.body.iter().all(u8::is_ascii_whitespace) {
            Ok(())
        } else {
            Err(Box::new(unknown_field(self.request_id, "")))
        }
    }

    /// A JSON object body with exactly the members `wanted` allows, each
    /// given; `credential` is kept as the JSON text it was sent as.
    fn body(
        &self,
        request: &Request,
        wanted: &[&str],
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
            return Err(Box::new(invalid_input(
                self.request_id,
                &[Problem {
                    field: String::new(),
                    rule: "type".to_string(),
                }],
            )));
        };
        let mut problems = Vec::new();
        for key in object.keys() {
            if !wanted.contains(&key.as_str()) {
                problems.push(Problem {
                    field: key.clone(),
                    rule: "unknown_field".to_string(),
                });
            }
        }
        for key in wanted {
            match object.get(*key) {
                None | Some(serde_json::Value::Null) => problems.push(Problem {
                    field: key.to_string(),
                    rule: "required".to_string(),
                }),
                Some(serde_json::Value::Object(_)) if *key == "credential" => {}
                Some(serde_json::Value::String(text)) if *key == "name" => {
                    if jadpo_std::standard::Part::Line
                        .rules()
                        .check_text(text)
                        .is_err()
                    {
                        problems.push(Problem {
                            field: key.to_string(),
                            rule: "pattern".to_string(),
                        });
                    }
                }
                Some(_) => problems.push(Problem {
                    field: key.to_string(),
                    rule: "type".to_string(),
                }),
            }
        }
        if problems.is_empty() {
            Ok(object)
        } else {
            Err(Box::new(invalid_input(self.request_id, &problems)))
        }
    }

    /// Keeps a new challenge for `binding`, read from the options
    /// `start` gave, and sweeps the expired ones.
    #[allow(clippy::too_many_arguments)]
    async fn keep(
        &self,
        ctx: &mut Ctx<'_>,
        options: &str,
        state: String,
        purpose: &str,
        binding: String,
        user: Option<SqlValue>,
        handle: Option<[u8; 16]>,
    ) -> Result<(), Stop> {
        let challenge = serde_json::from_str::<serde_json::Value>(options)
            .ok()
            .and_then(|options| {
                options
                    .get("publicKey")?
                    .get("challenge")?
                    .as_str()
                    .and_then(jadpo_value::encoding::from_base64url)
            })
            .ok_or_else(|| self.fail(AUTHENTICATION_MISCONFIGURED))?;
        let Some(id) = ctx.new_id() else {
            return Err(self.fail(AUTHENTICATION_MISCONFIGURED));
        };
        let now = ctx.now();
        self.once(ctx, SWEEP, &[SqlValue::Timestamp(now.saturating_mul(1000))])
            .await?;
        let params = [
            SqlValue::Uuid(id),
            SqlValue::Text(verifier(&challenge)),
            SqlValue::Text(purpose.to_string()),
            SqlValue::Text(binding),
            user.unwrap_or(SqlValue::Null(SqlType::Uuid)),
            handle.map_or(SqlValue::Null(SqlType::Bytes), |handle| {
                SqlValue::Bytes(handle.to_vec())
            }),
            SqlValue::Text(state),
            SqlValue::Timestamp(
                now.saturating_add(self.passkeys.expires_after_ms)
                    .saturating_mul(1000),
            ),
        ];
        self.once(ctx, PUT_CHALLENGE, &params).await?;
        Ok(())
    }

    /// Takes the challenge `credential` answers, if it is still kept for
    /// `purpose`, bound to `binding` and unexpired; any other answer is
    /// `passkey_challenge_expired`. The challenge is gone either way.
    async fn take(
        &self,
        ctx: &Ctx<'_>,
        credential: &str,
        purpose: &str,
        binding: Option<&str>,
    ) -> Result<Taken, Stop> {
        let Some(answer) = self.ceremonies.read_answer(credential) else {
            return Err(self.fail(REJECTED));
        };
        let rows = self
            .once(
                ctx,
                TAKE,
                &[
                    SqlValue::Text(verifier(&answer.challenge)),
                    SqlValue::Text(purpose.to_string()),
                ],
            )
            .await?;
        let Some(
            [
                SqlValue::Text(bound),
                user,
                handle,
                SqlValue::Text(state),
                SqlValue::Timestamp(expires),
            ],
        ) = rows.first().map(Vec::as_slice)
        else {
            return Err(self.fail(CHALLENGE_EXPIRED));
        };
        let bound_here =
            binding.is_some_and(|binding| constant_eq(bound.as_bytes(), binding.as_bytes()));
        if !bound_here || *expires <= ctx.now().saturating_mul(1000) {
            return Err(self.fail(CHALLENGE_EXPIRED));
        }
        Ok(Taken {
            state: state.clone(),
            user: match user {
                SqlValue::Uuid(user) => Some(*user),
                _ => None,
            },
            handle: match handle {
                SqlValue::Bytes(handle) => <[u8; 16]>::try_from(handle.as_slice()).ok(),
                _ => None,
            },
            answer,
        })
    }

    async fn sign_in_options(
        &self,
        ctx: &mut Ctx<'_>,
        request: &Request,
    ) -> Result<Response, Stop> {
        self.same_site(request)?;
        self.no_body(request)?;
        let Some(cookie) = binding_cookie(self.plan) else {
            return Err(self.fail(AUTHENTICATION_MISCONFIGURED));
        };
        let (options, state) = self
            .ceremonies
            .start_sign_in(self.origin, self.timeout_ms())
            .map_err(|_| self.fail(AUTHENTICATION_MISCONFIGURED))?;
        let Some(token) = new_token(ctx) else {
            return Err(self.fail(AUTHENTICATION_MISCONFIGURED));
        };
        self.keep(
            ctx,
            &options,
            state,
            SIGN_IN,
            text_verifier(&token),
            None,
            None,
        )
        .await?;
        let age = self.passkeys.expires_after_ms / 1000;
        let mut response = ok_json(200, options);
        response.headers.push((
            "set-cookie".to_string(),
            format!("{cookie}={token}; Path=/; Max-Age={age}; HttpOnly; Secure; SameSite=Strict"),
        ));
        Ok(response)
    }

    async fn sign_in(&self, ctx: &mut Ctx<'_>, request: &Request) -> Result<Response, Stop> {
        self.same_site(request)?;
        let body = self.body(request, &["credential"])?;
        let credential = body
            .get("credential")
            .map(serde_json::Value::to_string)
            .unwrap_or_default();
        let Some(cookie) = binding_cookie(self.plan) else {
            return Err(self.fail(AUTHENTICATION_MISCONFIGURED));
        };
        let binding = cookie_value(request, &cookie)
            .filter(|token| is_token(token))
            .map(text_verifier);
        let taken = self
            .take(ctx, &credential, SIGN_IN, binding.as_deref())
            .await?;
        // The passkey with its user's record.
        let now = ctx.now();
        let rows = self
            .once(
                ctx,
                &self.passkeys.lookup,
                &[
                    SqlValue::Bytes(taken.answer.credential_id.clone()),
                    SqlValue::Timestamp(now.saturating_mul(1000)),
                ],
            )
            .await?;
        let Some(
            [
                SqlValue::Uuid(row),
                SqlValue::Json(stored),
                SqlValue::Bytes(handle),
                SqlValue::Uuid(user),
                SqlValue::Bool(active),
            ],
        ) = rows.first().map(Vec::as_slice)
        else {
            return Err(self.fail(REJECTED));
        };
        let (Some(parts), Ok(handle)) = (
            read_credential(stored),
            <[u8; 16]>::try_from(handle.as_slice()),
        ) else {
            return Err(self.fail(REJECTED));
        };
        // The answer must name the user handle the passkey was made with.
        if taken
            .answer
            .user_handle
            .as_deref()
            .is_none_or(|given| !constant_eq(given, &handle))
        {
            return Err(self.fail(REJECTED));
        }
        let updated = self
            .ceremonies
            .finish_sign_in(
                self.origin,
                self.timeout_ms(),
                &taken.state,
                &credential,
                &parts.ceremony,
                handle,
            )
            .map_err(|_| self.fail(REJECTED))?;
        if !*active {
            return Err(self.fail(USER_DISABLED));
        }
        // The passkey's new counter and last use, and the session, together.
        let written = Credential {
            ceremony: updated,
            name: parts.name.clone(),
            registered_at: parts.registered_at.clone(),
            last_used_at: Some(jadpo_std::format_instant(now)),
        };
        let (Some(token), Some(csrf), Some(session)) =
            (new_token(ctx), new_token(ctx), ctx.new_id())
        else {
            return Err(self.fail(AUTHENTICATION_MISCONFIGURED));
        };
        let Some(database) = ctx.host().database(&self.plan.store) else {
            return Err(self.fail(AUTHENTICATION_MISCONFIGURED));
        };
        let unavailable = || self.fail(AUTHENTICATION_UNAVAILABLE);
        let mut transaction = database.begin().await.map_err(|_| unavailable())?;
        let changed = transaction
            .execute(
                USED,
                &[
                    SqlValue::Uuid(*row),
                    SqlValue::Text(parts.ceremony.sign_count.to_string()),
                    SqlValue::Json(written.to_json()),
                ],
            )
            .await;
        if changed != Ok(1) {
            let _ = transaction.rollback().await;
            return Err(match changed {
                Ok(_) | Err(StoreError::ConcurrentChange) => self.fail(REJECTED),
                Err(_) => unavailable(),
            });
        }
        let started = transaction
            .execute(
                START,
                &[
                    SqlValue::Uuid(session),
                    SqlValue::Uuid(*user),
                    SqlValue::Text(text_verifier(&token)),
                    SqlValue::Text(text_verifier(&csrf)),
                    SqlValue::Timestamp(now.saturating_mul(1000)),
                    SqlValue::Timestamp(
                        now.saturating_add(SESSION_LIFETIME_MS).saturating_mul(1000),
                    ),
                ],
            )
            .await;
        if started.is_err() {
            let _ = transaction.rollback().await;
            return Err(unavailable());
        }
        transaction.commit().await.map_err(|_| unavailable())?;
        let mut response = Response {
            status: 204,
            ..Response::default()
        };
        response.headers.extend(set_cookies(
            self.plan,
            &SessionEffect::Start { token, csrf },
        ));
        response.headers.push((
            "set-cookie".to_string(),
            format!("{cookie}=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Strict"),
        ));
        Ok(response)
    }

    async fn register_options(
        &self,
        ctx: &mut Ctx<'_>,
        request: &Request,
        caller: Option<&Caller>,
    ) -> Result<Response, Stop> {
        self.no_body(request)?;
        let (user, session, name) = self.holder(caller)?;
        // A passkey is added from a session, whose challenge is bound to it.
        let Some(session) = session else {
            return Err(self.fail(CHALLENGE_EXPIRED));
        };
        let existing = self.passkeys_of(ctx, user).await?;
        let exclude: Vec<Vec<u8>> = existing
            .iter()
            .map(|(_, credential)| credential.ceremony.credential_id.clone())
            .collect();
        let mut handle = [0u8; 16];
        if !ctx.secret_random_bytes(&mut handle) {
            return Err(self.fail(AUTHENTICATION_MISCONFIGURED));
        }
        let (options, state) = self
            .ceremonies
            .start_registration(self.origin, self.timeout_ms(), handle, &name, &exclude)
            .map_err(|_| self.fail(AUTHENTICATION_MISCONFIGURED))?;
        self.keep(
            ctx,
            &options,
            state,
            REGISTER,
            session_binding(session),
            Some(SqlValue::Uuid(user)),
            Some(handle),
        )
        .await?;
        Ok(ok_json(200, options))
    }

    async fn register(
        &self,
        ctx: &mut Ctx<'_>,
        request: &Request,
        caller: Option<&Caller>,
    ) -> Result<Response, Stop> {
        let body = self.body(request, &["name", "credential"])?;
        let (user, session, _) = self.holder(caller)?;
        let Some(session) = session else {
            return Err(self.fail(CHALLENGE_EXPIRED));
        };
        let name = body
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let credential = body
            .get("credential")
            .map(serde_json::Value::to_string)
            .unwrap_or_default();
        let binding = session_binding(session);
        let taken = self
            .take(ctx, &credential, REGISTER, Some(&binding))
            .await?;
        let (Some(holder), Some(handle)) = (taken.user, taken.handle) else {
            return Err(self.fail(CHALLENGE_EXPIRED));
        };
        if holder != user {
            return Err(self.fail(CHALLENGE_EXPIRED));
        }
        let parts = self
            .ceremonies
            .finish_registration(self.origin, self.timeout_ms(), &taken.state, &credential)
            .map_err(|_| self.fail(REJECTED))?;
        let now = ctx.now();
        let kept = Credential {
            ceremony: parts,
            name,
            registered_at: jadpo_std::format_instant(now),
            last_used_at: None,
        };
        let Some(id) = ctx.new_id() else {
            return Err(self.fail(AUTHENTICATION_MISCONFIGURED));
        };
        let params = [
            SqlValue::Uuid(id),
            SqlValue::Uuid(user),
            SqlValue::Bytes(kept.ceremony.credential_id.clone()),
            SqlValue::Bytes(handle.to_vec()),
            SqlValue::Json(kept.to_json()),
            SqlValue::Timestamp(now.saturating_mul(1000)),
            SqlValue::Int8(PASSKEY_LIMIT),
        ];
        let Some(database) = ctx.host().database(&self.plan.store) else {
            return Err(self.fail(AUTHENTICATION_MISCONFIGURED));
        };
        let unavailable = || self.fail(AUTHENTICATION_UNAVAILABLE);
        let mut transaction = database.begin().await.map_err(|_| unavailable())?;
        let added = transaction.execute(ADD, &params).await;
        match added {
            Ok(1) => transaction.commit().await.map_err(|_| unavailable())?,
            Ok(_) => {
                let _ = transaction.rollback().await;
                return Err(self.fail(LIMIT_REACHED));
            }
            Err(StoreError::Conflict { .. }) => {
                let _ = transaction.rollback().await;
                return Err(self.fail(ALREADY_REGISTERED));
            }
            Err(_) => {
                let _ = transaction.rollback().await;
                return Err(unavailable());
            }
        }
        Ok(ok_json(201, view(&id, &kept)))
    }

    async fn list(
        &self,
        ctx: &mut Ctx<'_>,
        request: &Request,
        caller: Option<&Caller>,
    ) -> Result<Response, Stop> {
        self.no_body(request)?;
        let (user, _, _) = self.holder(caller)?;
        let views: Vec<String> = self
            .passkeys_of(ctx, user)
            .await?
            .iter()
            .map(|(id, credential)| view(id, credential))
            .collect();
        Ok(ok_json(200, format!("[{}]", views.join(","))))
    }

    async fn rename(
        &self,
        ctx: &mut Ctx<'_>,
        request: &Request,
        segments: &[String],
        caller: Option<&Caller>,
    ) -> Result<Response, Stop> {
        let id = self.passkey_id(segments)?;
        let body = self.body(request, &["name"])?;
        let (user, _, _) = self.holder(caller)?;
        let name = body
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let rows = self
            .once(
                ctx,
                RENAME,
                &[
                    SqlValue::Uuid(id),
                    SqlValue::Uuid(user),
                    SqlValue::Text(name),
                ],
            )
            .await?;
        match rows.first().map(Vec::as_slice) {
            Some([SqlValue::Uuid(id), SqlValue::Json(stored)]) => match read_credential(stored) {
                Some(credential) => Ok(ok_json(200, view(id, &credential))),
                None => Err(self.fail(AUTHENTICATION_MISCONFIGURED)),
            },
            _ => Err(self.fail(NOT_FOUND)),
        }
    }

    async fn remove(
        &self,
        ctx: &mut Ctx<'_>,
        request: &Request,
        segments: &[String],
        caller: Option<&Caller>,
    ) -> Result<Response, Stop> {
        let id = self.passkey_id(segments)?;
        self.no_body(request)?;
        let (user, _, _) = self.holder(caller)?;
        let Some(database) = ctx.host().database(&self.plan.store) else {
            return Err(self.fail(AUTHENTICATION_MISCONFIGURED));
        };
        let unavailable = || self.fail(AUTHENTICATION_UNAVAILABLE);
        let mut transaction = database.begin().await.map_err(|_| unavailable())?;
        let removed = transaction
            .execute(REMOVE, &[SqlValue::Uuid(id), SqlValue::Uuid(user)])
            .await;
        match removed {
            Ok(1) => transaction.commit().await.map_err(|_| unavailable())?,
            Ok(_) => {
                let _ = transaction.rollback().await;
                return Err(self.fail(NOT_FOUND));
            }
            Err(_) => {
                let _ = transaction.rollback().await;
                return Err(unavailable());
            }
        }
        Ok(Response {
            status: 204,
            ..Response::default()
        })
    }

    /// The signed-in person: their `id`, the session they came with, if
    /// any, and what their device lists the passkey under.
    fn holder(&self, caller: Option<&Caller>) -> Result<Holder, Stop> {
        let Some(caller) = caller else {
            return Err(self.fail(AUTHENTICATION_MISCONFIGURED));
        };
        let jadpo_value::Value::Record(record) = &caller.value else {
            return Err(self.fail(AUTHENTICATION_MISCONFIGURED));
        };
        let user = record
            .get("id")
            .and_then(|id| crate::to_sql(id, SqlType::Uuid).ok());
        let Some(SqlValue::Uuid(user)) = user else {
            return Err(self.fail(AUTHENTICATION_MISCONFIGURED));
        };
        let session = caller.session;
        let name = match record.get("subject") {
            Some(jadpo_value::Value::Text(text)) => text.to_string(),
            Some(jadpo_value::Value::Uuid(text)) => text.to_string(),
            _ => caller.key.clone(),
        };
        Ok((user, session, name))
    }

    /// The `{passkey_id}` the path names, or 404 when it can't be one.
    fn passkey_id(&self, segments: &[String]) -> Result<[u8; 16], Stop> {
        segments
            .last()
            .and_then(|text| parse_uuid(text))
            .ok_or_else(|| self.fail(NOT_FOUND))
    }

    async fn passkeys_of(
        &self,
        ctx: &Ctx<'_>,
        user: [u8; 16],
    ) -> Result<Vec<([u8; 16], Credential)>, Stop> {
        let rows = self.once(ctx, LIST, &[SqlValue::Uuid(user)]).await?;
        Ok(rows
            .iter()
            .filter_map(|row| match row.as_slice() {
                [SqlValue::Uuid(id), SqlValue::Json(stored)] => {
                    Some((*id, read_credential(stored)?))
                }
                _ => None,
            })
            .collect())
    }
}

/// The signed-in person: their `id`, the session they came with, if any,
/// and what their device lists the passkey under.
type Holder = ([u8; 16], Option<[u8; 16]>, String);

/// A challenge taken for an answer.
struct Taken {
    state: String,
    user: Option<[u8; 16]>,
    handle: Option<[u8; 16]>,
    answer: crate::PasskeyAnswer,
}

/// A passkey as `jadpo_passkeys` keeps it: a `WebAuthnCredential` (area
/// 19), its ceremony's parts and the person's.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Credential {
    ceremony: PasskeyParts,
    name: String,
    registered_at: String,
    last_used_at: Option<String>,
}

impl Credential {
    /// Its JSON, as `WebAuthnCredential` is sent and stored: bytes in
    /// Base64, the count a JSON number, as the contract sends a whole number
    /// whose range JavaScript holds (chunk 5.5w3), `last_used_at` left out
    /// until it is used.
    fn to_json(&self) -> String {
        let mut object = serde_json::Map::new();
        let part = &self.ceremony;
        object.insert(
            "credential_id".into(),
            to_base64(&part.credential_id).into(),
        );
        object.insert("public_key".into(), to_base64(&part.public_key).into());
        object.insert("sign_count".into(), part.sign_count.into());
        object.insert(
            "transports".into(),
            serde_json::Value::Array(
                part.transports
                    .iter()
                    .map(|transport| transport.clone().into())
                    .collect(),
            ),
        );
        object.insert("backup_eligible".into(), part.backup_eligible.into());
        object.insert("backed_up".into(), part.backed_up.into());
        object.insert("name".into(), self.name.clone().into());
        object.insert("registered_at".into(), self.registered_at.clone().into());
        if let Some(used) = &self.last_used_at {
            object.insert("last_used_at".into(), used.clone().into());
        }
        serde_json::Value::Object(object).to_string()
    }
}

/// A kept passkey read back; `None` when it isn't one.
fn read_credential(stored: &str) -> Option<Credential> {
    let json: serde_json::Value = serde_json::from_str(stored).ok()?;
    let text = |key: &str| json.get(key).and_then(serde_json::Value::as_str);
    let flag = |key: &str| json.get(key).and_then(serde_json::Value::as_bool);
    Some(Credential {
        ceremony: PasskeyParts {
            credential_id: from_base64(text("credential_id")?)?,
            public_key: from_base64(text("public_key")?)?,
            sign_count: json
                .get("sign_count")?
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())?,
            transports: json
                .get("transports")?
                .as_array()?
                .iter()
                .map(|transport| transport.as_str().map(str::to_string))
                .collect::<Option<Vec<_>>>()?,
            backup_eligible: flag("backup_eligible")?,
            backed_up: flag("backed_up")?,
        },
        name: text("name")?.to_string(),
        registered_at: text("registered_at")?.to_string(),
        last_used_at: text("last_used_at").map(str::to_string),
    })
}

/// A passkey as its holder sees it: never its key or credential ID.
fn view(id: &[u8; 16], credential: &Credential) -> String {
    let mut object = serde_json::Map::new();
    object.insert("id".into(), format_uuid(id).into());
    object.insert("name".into(), credential.name.clone().into());
    object.insert(
        "registered_at".into(),
        credential.registered_at.clone().into(),
    );
    object.insert(
        "last_used_at".into(),
        credential
            .last_used_at
            .clone()
            .map_or(serde_json::Value::Null, serde_json::Value::String),
    );
    serde_json::Value::Object(object).to_string()
}

fn ok_json(status: u16, body: String) -> Response {
    Response {
        status,
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        body: body.into_bytes(),
        ..Response::default()
    }
}

fn unknown_field(request_id: &str, field: &str) -> Response {
    invalid_input(
        request_id,
        &[Problem {
            field: field.to_string(),
            rule: "unknown_field".to_string(),
        }],
    )
}

/// What a challenge is found by: the BLAKE3 digest of its bytes.
fn verifier(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// What is kept of a cookie's or session's token: its BLAKE3 digest.
fn text_verifier(token: &str) -> String {
    verifier(token.as_bytes())
}

/// What a registration's challenge is bound to: the session.
fn session_binding(session: [u8; 16]) -> String {
    format!("session {}", format_uuid(&session))
}

fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn is_token(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| b.is_ascii_hexdigit())
}

/// 32 random bytes as 64 hexadecimal digits, never on a recording's tape.
fn new_token(ctx: &mut Ctx<'_>) -> Option<String> {
    let mut bytes = [0u8; 32];
    if !ctx.secret_random_bytes(&mut bytes) {
        return None;
    }
    Some(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_credential_is_kept_as_its_standard_json() {
        let credential = Credential {
            ceremony: PasskeyParts {
                credential_id: vec![1, 2, 3],
                public_key: vec![4, 5],
                sign_count: 7,
                transports: vec!["internal".to_string()],
                backup_eligible: true,
                backed_up: false,
            },
            name: "Work phone".to_string(),
            registered_at: "2026-10-18T09:00:00Z".to_string(),
            last_used_at: None,
        };
        let json = credential.to_json();
        assert_eq!(
            json,
            r#"{"backed_up":false,"backup_eligible":true,"credential_id":"AQID","name":"Work phone","public_key":"BAU=","registered_at":"2026-10-18T09:00:00Z","sign_count":7,"transports":["internal"]}"#
        );
        assert_eq!(read_credential(&json), Some(credential));
    }

    #[test]
    fn a_passkey_id_is_a_uuid() {
        let id = [
            0x01, 0x92, 0xd3, 0xc4, 0, 0, 0x70, 0, 0x80, 0, 0, 0, 0, 0, 0, 0x2a,
        ];
        let text = format_uuid(&id);
        assert_eq!(text, "0192d3c4-0000-7000-8000-00000000002a");
        assert_eq!(parse_uuid(&text), Some(id));
        assert_eq!(parse_uuid("not-a-uuid"), None);
    }
}
