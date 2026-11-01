//! Confirming one request with a passkey (area 8, "Confirm one request
//! with a passkey"; chunk R1): a route with `confirm:
//! Confirmation.passkey` runs only once the signed-in person presents one
//! of their own passkeys for that very request.
//!
//! - **The options** come from the route's generated `<path>/confirmation`,
//!   asked with the route's method (a `GET`'s with `POST`), which reads the
//!   request as the route itself would (its placeholders, query or body,
//!   and header fields) and keeps a
//!   challenge in `jadpo_passkey_challenges` for the purpose `confirm`,
//!   bound to the person (their `id`, kept beside it) and to a digest of
//!   the route and its input, the one an `Idempotency-Key` is checked
//!   against. The options list the person's passkeys in
//!   `allowCredentials`; a person with none is refused, never given
//!   options any passkey could answer.
//! - **The request** carries the device's answer in `X-Jadpo-Confirmation`,
//!   WebAuthn's `toJSON()` in base64. Before anything runs, its challenge
//!   is taken with `DELETE … RETURNING`, so it works once whatever the
//!   answer, and must be bound to this person and this request's digest
//!   and under two minutes old; the passkey must be one of this person's,
//!   the answer must check as `webauthn-rs`'s passkey authentication
//!   checks one, the person verified, and the passkey's counter, backup
//!   state and last use are written only if its counter is still the one
//!   read. Anything else is 403 `confirmation_required`, and nothing runs.

use jadpo_std::http::{CONFIRMATION_EXPIRES_MS, CONFIRMATION_HEADER, RoutePlan};
use jadpo_std::read::SqlType;
use jadpo_value::Value;

use super::identity::{AUTHENTICATION_MISCONFIGURED, AUTHENTICATION_UNAVAILABLE};
use super::passkeys::{
    self, Credential, Site, Stop, constant_eq, encoding_value, format_uuid, ok_json,
};
use super::{App, Read, Request, Response, invalid_input, read_request, runtime_failure};
use crate::{Caller, Ctx, Executor, SqlValue};

/// A request the person didn't confirm: 403, and nothing runs.
pub(crate) const REQUIRED: (u16, &str, &str) = (
    403,
    jadpo_std::http::CONFIRMATION_REQUIRED,
    "This request needs your passkey.",
);

/// What a confirmation's challenge is kept for.
const CONFIRM: &str = "confirm";

/// One of the person's passkeys, by its credential ID.
pub(crate) const OWN: &str = "SELECT \"id\", \"credential\", \"encoding\", \"user_handle\" FROM \"jadpo_passkeys\" WHERE \"credential_id\" = $1 AND \"principal_id\" = $2";

/// Writes a passkey after it confirmed a request, in this release's
/// encoding, if its counter is still the one read: a second answer racing
/// it changes nothing.
pub(crate) const USED: &str = "UPDATE \"jadpo_passkeys\" SET \"credential\" = $3, \"encoding\" = $4 WHERE \"id\" = $1 AND \"principal_id\" = $5 AND \"credential\" ->> 'sign_count' = $2 RETURNING \"id\"";

/// What a confirmation's challenge is bound to: the person, and the digest
/// of the route and the request's input.
fn binding(user: &[u8; 16], digest: &str) -> String {
    format!("{CONFIRM} {} {digest}", format_uuid(user))
}

/// The caller's user `id`, when the caller is a person: a service holds no
/// passkey.
pub(crate) fn person(caller: Option<&Caller>) -> Option<[u8; 16]> {
    let caller = caller?;
    if !caller.key.starts_with("user ") {
        return None;
    }
    let Value::Record(record) = &caller.value else {
        return None;
    };
    match record
        .get("id")
        .and_then(|id| crate::to_sql(id, SqlType::Uuid).ok())
    {
        Some(SqlValue::Uuid(user)) => Some(user),
        _ => None,
    }
}

/// The passkeys' site, or why there is none: the identity plan, the
/// configured origin, `passkey:` and the host's ceremonies.
macro_rules! site {
    ($app:expr, $executor:expr, $ctx:expr, $started:expr, $request_id:expr) => {{
        let (Some(plan), Some(origin)) = ($app.table.identity.as_ref(), $started.auth().origin())
        else {
            return Err(passkeys::stop(AUTHENTICATION_MISCONFIGURED, $request_id));
        };
        let (Some(passkeys), Some(ceremonies)) = (plan.passkeys.as_ref(), $ctx.host().passkeys())
        else {
            return Err(passkeys::stop(AUTHENTICATION_MISCONFIGURED, $request_id));
        };
        Site {
            plan,
            passkeys,
            json: &$app.json,
            origin,
            ceremonies,
            executor: $executor,
            request_id: $request_id,
        }
    }};
}

/// `<path>/confirmation` for route `confirmed`, asked by `user`: the
/// options for `navigator.credentials.get()`.
pub(crate) async fn options(
    app: &App<'_>,
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    request: &Request,
    request_id: &str,
    confirmed: u32,
    user: Option<[u8; 16]>,
) -> Response {
    let mut response = match asked(app, executor, ctx, request, request_id, confirmed, user).await {
        Ok(response) => response,
        Err(stopped) => *stopped,
    };
    response
        .headers
        .push(("cache-control".to_string(), "no-store".to_string()));
    response
}

async fn asked(
    app: &App<'_>,
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    request: &Request,
    request_id: &str,
    confirmed: u32,
    user: Option<[u8; 16]>,
) -> Result<Response, Stop> {
    let started = ctx.started();
    let site = site!(app, executor, ctx, started, request_id);
    let Some(route) = app.table.routes.get(confirmed as usize) else {
        return Err(site.fail(AUTHENTICATION_MISCONFIGURED));
    };
    // The request, read as the route reads it.
    let (params, input) = match read_request(app, confirmed, request) {
        Read::Refused(refusal) => return Err(Box::new(runtime_failure(refusal, request_id, None))),
        Read::Problems(problems) => return Err(Box::new(invalid_input(request_id, &problems))),
        Read::Values { params, input } => (params, input),
    };
    let Some(user) = user else {
        return Err(site.fail(REQUIRED));
    };
    // The person's own passkeys, which alone may answer.
    let own: Vec<_> = site
        .passkeys_of(ctx, user)
        .await?
        .into_iter()
        .map(|(_, credential)| credential.ceremony)
        .collect();
    if own.is_empty() {
        return Err(site.fail(REQUIRED));
    }
    let lasts = u32::try_from(CONFIRMATION_EXPIRES_MS).unwrap_or(u32::MAX);
    let (options, state) = site
        .ceremonies
        .start_confirmation(site.origin, lasts, &own)
        .map_err(|_| site.fail(AUTHENTICATION_MISCONFIGURED))?;
    let digest = super::idempotency::digest(&app.codecs, route, &params, input.as_ref());
    site.keep(
        ctx,
        &options,
        state,
        CONFIRM,
        binding(&user, &digest),
        Some(SqlValue::Uuid(user)),
        None,
        CONFIRMATION_EXPIRES_MS,
    )
    .await?;
    Ok(ok_json(200, options))
}

/// Whether `request` to `route`, read as `params` and `input`, carries
/// the confirmation of it by `user`, the person calling: `Err` is the
/// response that refuses it.
/// A confirmation that checks writes its passkey's counter and last use.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn check(
    app: &App<'_>,
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    request: &Request,
    request_id: &str,
    route: &RoutePlan,
    user: Option<[u8; 16]>,
    (params, input): (&[Option<Value>], Option<&Value>),
) -> Result<(), Box<Response>> {
    let required = || Box::new(runtime_failure(REQUIRED, request_id, None));
    // The device's answer, which a request without is refused for at once.
    let mut sent = request
        .headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case(CONFIRMATION_HEADER));
    let (Some((_, answer)), None) = (sent.next(), sent.next()) else {
        return Err(required());
    };
    let Some(credential) = answer_text(answer) else {
        return Err(required());
    };
    let Some(user) = user else {
        return Err(required());
    };
    let digest = super::idempotency::digest(&app.codecs, route, params, input);
    let started = ctx.started();
    let confirming = async {
        let site = site!(app, executor, ctx, started, request_id);
        confirmed(&site, ctx, user, &credential, &digest).await
    };
    match confirming.await {
        Ok(()) => Ok(()),
        Err(stopped) => {
            // The store's or the configuration's failure stays itself;
            // every refusal of the answer is the one refusal.
            let kept = stopped.code.as_deref().is_some_and(|code| {
                code == AUTHENTICATION_UNAVAILABLE.1 || code == AUTHENTICATION_MISCONFIGURED.1
            });
            Err(if kept { stopped } else { required() })
        }
    }
}

/// The checks of [`check`], each refusal any failure but the store's.
async fn confirmed(
    site: &Site<'_>,
    ctx: &mut Ctx<'_>,
    user: [u8; 16],
    credential: &str,
    digest: &str,
) -> Result<(), Stop> {
    let refused = || site.fail(REQUIRED);
    // The challenge, taken once whatever follows: it must be this
    // person's, for this request, and unexpired.
    let taken = site
        .take(ctx, credential, CONFIRM, Some(&binding(&user, digest)))
        .await?;
    if taken.user.is_none_or(|holder| !constant_eq(&holder, &user)) {
        return Err(refused());
    }
    // The passkey, which must be one of this person's.
    let rows = site
        .once(
            ctx,
            OWN,
            &[
                SqlValue::Bytes(taken.answer.credential_id.clone()),
                SqlValue::Uuid(user),
            ],
        )
        .await?;
    let Some(
        [
            SqlValue::Uuid(row),
            SqlValue::Json(stored),
            encoding,
            handle,
        ],
    ) = rows.first().map(Vec::as_slice)
    else {
        return Err(refused());
    };
    // A user handle the answer gives is the one the passkey was made with
    // (WebAuthn Level 3, §7.2).
    let SqlValue::Bytes(handle) = handle else {
        return Err(refused());
    };
    if taken
        .answer
        .user_handle
        .as_deref()
        .is_some_and(|given| !constant_eq(given, handle))
    {
        return Err(refused());
    }
    let Some(kept) = site.read_kept(stored, encoding) else {
        return Err(refused());
    };
    let lasts = u32::try_from(CONFIRMATION_EXPIRES_MS).unwrap_or(u32::MAX);
    let updated = site
        .ceremonies
        .finish_confirmation(site.origin, lasts, &taken.state, credential, &kept.ceremony)
        .map_err(|_| refused())?;
    // Its new counter and last use, only if its counter is still the one
    // read: a second answer racing this one changes nothing.
    let written = Credential {
        ceremony: updated,
        last_used_at: Some(ctx.now()),
        ..kept.clone()
    };
    let written = site.write_kept(&written)?;
    let changed = site
        .once(
            ctx,
            USED,
            &[
                SqlValue::Uuid(*row),
                SqlValue::Text(kept.ceremony.sign_count.to_string()),
                written,
                encoding_value(),
                SqlValue::Uuid(user),
            ],
        )
        .await?;
    if changed.len() == 1 {
        Ok(())
    } else {
        Err(refused())
    }
}

/// The JSON the header carries, in any of base64's four forms.
fn answer_text(header: &str) -> Option<String> {
    let header = header.trim();
    let unpadded = header.trim_end_matches('=');
    if header.len() - unpadded.len() > 2
        || unpadded.contains(['+', '/']) && unpadded.contains(['-', '_'])
    {
        return None;
    }
    let bytes =
        jadpo_value::encoding::from_base64url(&unpadded.replace('+', "-").replace('/', "_"))?;
    let text = String::from_utf8(bytes).ok()?;
    serde_json::from_str::<serde_json::Value>(&text)
        .ok()?
        .is_object()
        .then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_answer_is_json_in_any_base64() {
        let json = r#"{"id":"AQ","type":"public-key"}"#;
        let url = jadpo_value::encoding::to_base64url(json.as_bytes());
        assert_eq!(answer_text(&url).as_deref(), Some(json));
        let standard = url.replace('-', "+").replace('_', "/");
        let padded = format!("{standard}{}", "=".repeat((4 - standard.len() % 4) % 4));
        assert_eq!(answer_text(&padded).as_deref(), Some(json));
        assert_eq!(answer_text(json), None);
        assert_eq!(answer_text(""), None);
        assert_eq!(
            answer_text(&jadpo_value::encoding::to_base64url(b"[1]")),
            None
        );
    }

    /// Only a person can confirm: a service's `id` is never taken for one.
    #[test]
    fn only_a_person_confirms() {
        let caller = |key: &str| Caller {
            value: Value::record(
                "Principal.user",
                vec![(
                    "id".into(),
                    Value::Uuid(std::rc::Rc::from("01010101-0101-0101-0101-010101010101")),
                )],
            ),
            key: key.to_string(),
            session: None,
            sign_in: None,
            multi_factor: false,
        };
        assert_eq!(person(Some(&caller("user 1"))), Some([1; 16]));
        assert_eq!(person(Some(&caller("service 1"))), None);
        assert_eq!(person(None), None);
    }

    #[test]
    fn a_challenge_is_bound_to_the_person_and_the_request() {
        let ada = [1; 16];
        let bob = [2; 16];
        assert_ne!(binding(&ada, "d1"), binding(&bob, "d1"));
        assert_ne!(binding(&ada, "d1"), binding(&ada, "d2"));
        assert_eq!(
            binding(&ada, "d1"),
            "confirm 01010101-0101-0101-0101-010101010101 d1"
        );
    }
}
