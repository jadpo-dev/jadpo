//! Signing in through a provider (area 8, "Providers"; chunk A7b): the
//! two routes each `Driver.oidc` connection `sign_in: { providers: }`
//! names generates, which the runtime answers itself.
//!
//! - **`POST /auth/<name>/start`** needs an `Origin` naming the browser
//!   session's origin, takes `{ "return_to": "/path" }` or nothing, and
//!   answers with the provider's sign-in address. It keeps the sign-in in
//!   `jadpo_provider_sign_ins` by its `state`'s digest, bound to a cookie
//!   of 32 random bytes kept as their digest (`<cookie>_provider`,
//!   `SameSite=Lax` so it comes back on the provider's redirect, ten
//!   minutes). The PKCE verifier and the nonce are keyed BLAKE3 of that
//!   cookie and the `state`, so the table holds neither.
//! - **`GET /auth/<name>/callback`** takes the sign-in once by its `state`,
//!   checks the cookie and the time, exchanges the code at the provider's
//!   token endpoint with the verifier and the client secret, and checks
//!   the ID token: the provider's keys (fetched and cached as an external
//!   token's are), its exact issuer, the client ID as its audience (and
//!   `azp` when it names several), its times and the nonce. Only `sub`,
//!   `email`, `email_verified` and `name` are read, into `FirstSignIn`;
//!   none becomes an identity or a grant. The account finds its user only
//!   through its link, else `on_first_sign_in:` decides; the user must be
//!   active, and a browser session starts, answering `303` to `return_to`.
//! - **Every refusal of the callback** answers `303` to `return_to`, or
//!   `/` when the sign-in is unknown, with `?error=<code>` after it.
//!
//! In tests and `jadpo dev` the local issuer stands in for the provider:
//! its authorization and token endpoints are its own, answered in
//! process, and nothing is fetched.

use jadpo_std::http::{IdentityPlan, ProviderPlan, ProviderStep, ProvidersPlan};
use jadpo_value::Value;

use super::external_token::Endpoints;
use super::identity::{
    AUTHENTICATION_MISCONFIGURED, AUTHENTICATION_UNAVAILABLE, CSRF_REJECTED, Refused, START,
    USER_DISABLED, constant_eq, cookie_values, new_token, set_cookies, verifier,
};
use super::magic_links::{RETURN_INVALID, site_path};
use super::sign_in_events::{self, SignInEvent, Way};
use super::sign_in_links::{self, Arrival};
use super::signed::ProviderSettings;
use super::{App, Request, Response, runtime_failure};
use crate::ctx::SessionEffect;
use crate::{Ctx, Executor, SqlValue};

/// A sign-in through a provider that expired, was used, came back to
/// another browser, or that the provider refused or answered with an ID
/// token that doesn't check.
pub(crate) const PROVIDER_FAILED: (u16, &str, &str) = (
    401,
    jadpo_std::http::PROVIDER_FAILED,
    "The sign-in through the provider didn't work. Start again.",
);

/// How long a sign-in under way lasts: ten minutes, in milliseconds.
const SIGN_IN_MS: i64 = 600_000;

/// What the provider is asked for: the person's subject, address and
/// name, and nothing that acts for them.
const SCOPE: &str = "openid email profile";

/// The most a token endpoint's answer may hold, and how long it may take.
const ANSWER_BYTES: usize = 65_536;
const EXCHANGE_MS: u64 = 3_000;

/// Keeps a sign-in under way: `$1` its `id`, `$2` the provider, `$3` the
/// `state`'s digest, `$4` the binding cookie's, `$5` where to go next and
/// `$6` when it expires.
pub(crate) const PUT: &str = "INSERT INTO \"jadpo_provider_sign_ins\" (\"id\", \"provider\", \"state\", \"binding\", \"return_to\", \"expires_at\") VALUES ($1, $2, $3, $4, $5, $6)";

/// Drops the sign-ins that expired before `$1`.
pub(crate) const SWEEP: &str = "DELETE FROM \"jadpo_provider_sign_ins\" WHERE \"expires_at\" <= $1";

/// Takes a sign-in once, by `$1` its `state`'s digest and `$2` its
/// provider: its binding cookie's digest, where to go next and when it
/// expires.
pub(crate) const TAKE: &str = "DELETE FROM \"jadpo_provider_sign_ins\" WHERE \"state\" = $1 AND \"provider\" = $2 RETURNING \"binding\", \"return_to\", \"expires_at\"";

/// The cookie binding a sign-in to the browser that started it.
pub(crate) fn binding_cookie(plan: &IdentityPlan) -> Option<String> {
    plan.cookie
        .as_ref()
        .map(|cookie| format!("{cookie}_provider"))
}

/// The provider route `step` of the provider at `at`, answered.
pub(crate) async fn answer(
    app: &App<'_>,
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    request: &Request,
    request_id: &str,
    at: u32,
    step: ProviderStep,
) -> Response {
    let mut response = match Site::of(app, executor, ctx, at) {
        Some(site) => match step {
            ProviderStep::Start => site.start(ctx, request).await,
            ProviderStep::Callback => site.callback(ctx, request).await,
        },
        None => Err(Stop::Fail(AUTHENTICATION_MISCONFIGURED)),
    }
    .unwrap_or_else(|stop| match stop {
        Stop::Fail(failure) => runtime_failure(failure, request_id, None),
        Stop::Redirect(response) => *response,
    });
    response
        .headers
        .push(("cache-control".to_string(), "no-store".to_string()));
    response
}

/// Why a route stops: a JSON failure (the start's), or the callback's
/// redirect saying why.
enum Stop {
    Fail((u16, &'static str, &'static str)),
    Redirect(Box<Response>),
}

/// What answering a provider's routes reads, as the program started.
struct Site<'a> {
    plan: &'a IdentityPlan,
    providers: &'a ProvidersPlan,
    provider: &'a ProviderPlan,
    settings: ProviderSettings,
    origin: String,
    cookie: String,
    executor: &'a dyn Executor,
}

impl<'a> Site<'a> {
    fn of(
        app: &'a App<'_>,
        executor: &'a dyn Executor,
        ctx: &mut Ctx<'_>,
        at: u32,
    ) -> Option<Site<'a>> {
        let plan = app.table.identity.as_ref()?;
        let providers = plan.providers.as_ref()?;
        let provider = providers.providers.get(at as usize)?;
        let started = ctx.started();
        Some(Site {
            plan,
            providers,
            provider,
            settings: started.auth().provider(at)?.clone(),
            origin: started.auth().origin()?.to_string(),
            cookie: binding_cookie(plan)?,
            executor,
        })
    }

    /// The address the provider sends the browser back to.
    fn redirect_uri(&self) -> String {
        format!("{}/auth/{}/callback", self.origin, self.provider.name)
    }

    fn way(&self) -> Way {
        Way {
            name: self.provider.name.clone(),
            ordinal: self.provider.ordinal,
        }
    }

    async fn start(&self, ctx: &mut Ctx<'_>, request: &Request) -> Result<Response, Stop> {
        if request.header("origin") != Some(self.origin.as_str()) {
            return Err(Stop::Fail(CSRF_REJECTED));
        }
        let return_to = return_to(request)?;
        let now = ctx.now();
        let endpoints = self
            .settings
            .keys
            .endpoints(ctx.host(), now)
            .await
            .map_err(Stop::Fail)?;
        let Endpoints {
            authorization: Some(authorization),
            ..
        } = endpoints
        else {
            return Err(Stop::Fail(AUTHENTICATION_MISCONFIGURED));
        };
        let (Some(binding), Some(state), Some(id)) = (new_token(ctx), new_token(ctx), ctx.new_id())
        else {
            return Err(Stop::Fail(AUTHENTICATION_MISCONFIGURED));
        };
        let (pkce, nonce) = derived(&binding, &state);
        let challenge =
            jadpo_value::encoding::to_base64url(&jadpo_value::hash::sha256(pkce.as_bytes()));
        let unavailable = || Stop::Fail(AUTHENTICATION_UNAVAILABLE);
        let database = ctx
            .host()
            .database(&self.plan.store)
            .ok_or_else(unavailable)?;
        let at = SqlValue::Timestamp(now.saturating_mul(1000));
        let mut transaction = database.begin().await.map_err(|_| unavailable())?;
        let kept = match transaction.execute(SWEEP, &[at]).await {
            Ok(_) => {
                transaction
                    .execute(
                        PUT,
                        &[
                            SqlValue::Uuid(id),
                            SqlValue::Text(self.provider.name.clone()),
                            SqlValue::Text(verifier(&state)),
                            SqlValue::Text(verifier(&binding)),
                            SqlValue::Text(return_to),
                            SqlValue::Timestamp(
                                now.saturating_add(SIGN_IN_MS).saturating_mul(1000),
                            ),
                        ],
                    )
                    .await
            }
            Err(error) => Err(error),
        };
        if kept.is_err() {
            let _ = transaction.rollback().await;
            return Err(unavailable());
        }
        transaction.commit().await.map_err(|_| unavailable())?;
        let query = [
            ("response_type", "code"),
            ("client_id", self.settings.keys.audience()),
            ("redirect_uri", &self.redirect_uri()),
            ("scope", SCOPE),
            ("state", &state),
            ("nonce", &nonce),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
        ];
        let separator = if authorization.contains('?') {
            '&'
        } else {
            '?'
        };
        let address = format!("{authorization}{separator}{}", form(&query));
        let mut body = String::from("{\"address\":");
        jadpo_value::json::write_string(&address, &mut body);
        body.push('}');
        Ok(Response {
            status: 200,
            headers: vec![
                ("content-type".to_string(), "application/json".to_string()),
                (
                    "set-cookie".to_string(),
                    format!(
                        "{}={binding}; Path=/; Max-Age={}; HttpOnly; Secure; SameSite=Lax",
                        self.cookie,
                        SIGN_IN_MS / 1000
                    ),
                ),
            ],
            body: body.into_bytes(),
            ..Response::default()
        })
    }

    async fn callback(&self, ctx: &mut Ctx<'_>, request: &Request) -> Result<Response, Stop> {
        let query = request
            .query
            .as_deref()
            .and_then(super::query_values)
            .unwrap_or_default();
        let one = |name: &str| match query.get(name).map(Vec::as_slice) {
            Some([value]) => Some(value.clone()),
            _ => None,
        };
        let Some(state) = one("state") else {
            return Err(self.redirect("/", PROVIDER_FAILED));
        };
        let now = ctx.now();
        let unavailable = |site: &Site<'_>, to: &str| site.redirect(to, AUTHENTICATION_UNAVAILABLE);
        let database = ctx
            .host()
            .database(&self.plan.store)
            .ok_or_else(|| unavailable(self, "/"))?;
        // Taken once, whatever comes of it.
        let rows = database
            .query_once(
                TAKE,
                &[
                    SqlValue::Text(verifier(&state)),
                    SqlValue::Text(self.provider.name.clone()),
                ],
            )
            .await
            .map_err(|_| unavailable(self, "/"))?;
        let Some(
            [
                SqlValue::Text(bound),
                SqlValue::Text(return_to),
                SqlValue::Timestamp(ends),
            ],
        ) = rows.first().map(Vec::as_slice)
        else {
            return Err(self.redirect("/", PROVIDER_FAILED));
        };
        let return_to = return_to.clone();
        let cookies = cookie_values(request, &self.cookie);
        let binding = match cookies.as_slice() {
            [binding] => *binding,
            _ => return Err(self.redirect(&return_to, PROVIDER_FAILED)),
        };
        if !constant_eq(verifier(binding).as_bytes(), bound.as_bytes())
            || *ends <= now.saturating_mul(1000)
            || one("error").is_some()
        {
            return Err(self.redirect(&return_to, PROVIDER_FAILED));
        }
        let Some(code) = one("code") else {
            return Err(self.redirect(&return_to, PROVIDER_FAILED));
        };
        let (pkce, nonce) = derived(binding, &state);
        let token = self
            .exchange(ctx, &code, &pkce, now)
            .await
            .map_err(|failure| self.redirect(&return_to, failure))?;
        let issuer = self.settings.keys.issuer().to_string();
        let client_id = self.settings.keys.audience().to_string();
        let claims = self
            .settings
            .keys
            .verified(ctx.host(), now, &|keys: &[Value]| {
                id_token(&token, keys, &issuer, &client_id, &nonce, now)
            })
            .await
            .map_err(|failure| {
                let failure = if failure.1 == super::identity::INVALID_CREDENTIALS.1 {
                    PROVIDER_FAILED
                } else {
                    failure
                };
                self.redirect(&return_to, failure)
            })?;
        let user = self
            .user(ctx, &claims)
            .await
            .map_err(|refused| self.redirect(&return_to, refused.failure))?;
        self.session(ctx, user, &return_to).await
    }

    /// The ID token the provider gives for `code`: from the local
    /// issuer in process, else from its token endpoint, sent once.
    async fn exchange(
        &self,
        ctx: &mut Ctx<'_>,
        code: &str,
        pkce: &str,
        now: i64,
    ) -> Result<String, (u16, &'static str, &'static str)> {
        let redirect = self.redirect_uri();
        #[cfg(feature = "testing")]
        if let Some(local) = ctx.host().local_issuer() {
            return local
                .redeem(
                    code,
                    pkce,
                    self.settings.keys.audience(),
                    &redirect,
                    self.settings.keys.issuer(),
                    now,
                )
                .ok_or(PROVIDER_FAILED);
        }
        let endpoints = self.settings.keys.endpoints(ctx.host(), now).await?;
        let token_endpoint = endpoints.token.ok_or(AUTHENTICATION_MISCONFIGURED)?;
        let outbound = ctx.host().outbound().ok_or(AUTHENTICATION_UNAVAILABLE)?;
        let body = form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &redirect),
            ("client_id", self.settings.keys.audience()),
            ("client_secret", &self.settings.secret),
            ("code_verifier", pkce),
        ]);
        let (status, bytes) = outbound
            .post_form(&token_endpoint, &body, ANSWER_BYTES, EXCHANGE_MS)
            .await
            .map_err(|_| AUTHENTICATION_UNAVAILABLE)?;
        if status != 200 {
            return Err(PROVIDER_FAILED);
        }
        let document = jadpo_value::jwt::strict_document(&bytes).ok_or(PROVIDER_FAILED)?;
        document
            .get("id_token")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .ok_or(PROVIDER_FAILED)
    }

    /// The user the account signs in: through its link, else the one
    /// `on_first_sign_in:` gives; refused unless active.
    async fn user(
        &self,
        ctx: &mut Ctx<'_>,
        claims: &IdClaims,
    ) -> Result<([u8; 16], bool), Refused> {
        let first = &self.providers.first_sign_in;
        let params = [
            SqlValue::Null(self.providers.subject_sql),
            SqlValue::Timestamp(ctx.now().saturating_mul(1000)),
            SqlValue::Text(self.provider.name.clone()),
            SqlValue::Text(claims.subject.clone()),
        ];
        let database = ctx
            .host()
            .database(&self.plan.store)
            .ok_or_else(|| Refused::new(AUTHENTICATION_UNAVAILABLE))?;
        let rows = database
            .query_once(&first.lookup, &params)
            .await
            .map_err(|_| Refused::new(AUTHENTICATION_UNAVAILABLE))?;
        let (row, first_time) = match rows.into_iter().next() {
            Some(row) => (row, false),
            None => {
                let arrival = Arrival {
                    way: self.way(),
                    subject: &claims.subject,
                    address: claims.address.as_deref(),
                    address_verified: claims.address_verified,
                    name: claims.name.as_deref(),
                };
                sign_in_links::first(
                    ctx,
                    self.executor,
                    &self.plan.store,
                    first,
                    &params,
                    &arrival,
                    false,
                )
                .await?
            }
        };
        let [SqlValue::Uuid(user), SqlValue::Bool(active), ..] = row.as_slice() else {
            return Err(Refused::new(AUTHENTICATION_UNAVAILABLE));
        };
        if !active {
            return Err(Refused::new(USER_DISABLED));
        }
        Ok((*user, first_time))
    }

    /// Starts `user`'s browser session, with `SignedIn`, and sends the
    /// browser on to `return_to`.
    async fn session(
        &self,
        ctx: &mut Ctx<'_>,
        (user, first_time): ([u8; 16], bool),
        return_to: &str,
    ) -> Result<Response, Stop> {
        let now = ctx.now();
        let (Some(token), Some(csrf), Some(session)) =
            (new_token(ctx), new_token(ctx), ctx.new_id())
        else {
            return Err(self.redirect(return_to, AUTHENTICATION_MISCONFIGURED));
        };
        let unavailable = || self.redirect(return_to, AUTHENTICATION_UNAVAILABLE);
        let database = ctx
            .host()
            .database(&self.plan.store)
            .ok_or_else(unavailable)?;
        let mut transaction = database.begin().await.map_err(|_| unavailable())?;
        let started = transaction
            .execute(
                START,
                &[
                    SqlValue::Uuid(session),
                    SqlValue::Uuid(user),
                    SqlValue::Text(verifier(&token)),
                    SqlValue::Text(verifier(&csrf)),
                    SqlValue::Timestamp(now.saturating_mul(1000)),
                    SqlValue::Timestamp(
                        now.saturating_add(jadpo_std::http::SESSION_LIFETIME_MS)
                            .saturating_mul(1000),
                    ),
                ],
            )
            .await;
        if started.is_err() {
            let _ = transaction.rollback().await;
            return Err(unavailable());
        }
        let event = SignInEvent::SignedIn {
            user,
            method: self.way(),
            first_time,
        };
        sign_in_events::commit_with(ctx, self.executor, &self.plan.store, transaction, &[event])
            .await
            .map_err(|_| unavailable())?;
        let mut response = self.going(return_to, None);
        response.headers.extend(set_cookies(
            self.plan,
            &SessionEffect::Start { token, csrf },
        ));
        Ok(response)
    }

    /// A `303` to `to`, with the binding cookie cleared, and `failure`'s
    /// code after it as `?error=`.
    fn going(&self, to: &str, failure: Option<&str>) -> Response {
        let location = match failure {
            Some(code) => {
                let separator = if to.contains('?') { '&' } else { '?' };
                format!("{to}{separator}error={code}")
            }
            None => to.to_string(),
        };
        Response {
            status: 303,
            headers: vec![
                ("location".to_string(), location),
                (
                    "set-cookie".to_string(),
                    format!(
                        "{}=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Lax",
                        self.cookie
                    ),
                ),
            ],
            code: failure.map(str::to_string),
            ..Response::default()
        }
    }

    fn redirect(&self, to: &str, failure: (u16, &'static str, &'static str)) -> Stop {
        Stop::Redirect(Box::new(self.going(to, Some(failure.1))))
    }
}

/// Where the person goes once signed in: the start's `return_to`, a path
/// on the site, or `/`. An empty body asks for `/`.
fn return_to(request: &Request) -> Result<String, Stop> {
    if request.body.iter().all(u8::is_ascii_whitespace) {
        return Ok("/".to_string());
    }
    let json = request
        .header("content-type")
        .map(|kind| {
            kind.split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase()
        })
        .is_some_and(|kind| kind == "application/json" || kind.ends_with("+json"));
    if !json {
        return Err(Stop::Fail(super::BODY_NOT_JSON));
    }
    let Some(serde_json::Value::Object(document)) =
        jadpo_value::jwt::strict_document(&request.body)
    else {
        return Err(Stop::Fail(super::INVALID_REQUEST));
    };
    if document.keys().any(|key| key != "return_to") {
        return Err(Stop::Fail(super::INVALID_REQUEST));
    }
    match document.get("return_to") {
        None | Some(serde_json::Value::Null) => Ok("/".to_string()),
        Some(serde_json::Value::String(path)) if site_path(path) => Ok(path.clone()),
        Some(_) => Err(Stop::Fail(RETURN_INVALID)),
    }
}

/// The PKCE verifier and the nonce of the sign-in bound to `binding` with
/// `state`: keyed BLAKE3 of both, as unpadded Base64url, so a table that
/// holds only their digests can exchange nothing.
fn derived(binding: &str, state: &str) -> (String, String) {
    let key = blake3::derive_key("jadpo provider sign-in", binding.as_bytes());
    let part = |purpose: &str| {
        let mut hasher = blake3::Hasher::new_keyed(&key);
        hasher.update(purpose.as_bytes());
        hasher.update(&[0]);
        hasher.update(state.as_bytes());
        jadpo_value::encoding::to_base64url(hasher.finalize().as_bytes())
    };
    (part("pkce"), part("nonce"))
}

/// `pairs` as `application/x-www-form-urlencoded`.
fn form(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(name, value)| {
            format!(
                "{}={}",
                super::percent_encode(name),
                super::percent_encode(value)
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// What an ID token says about the person, once it checks.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct IdClaims {
    pub subject: String,
    /// Its `email`, when it is an address.
    pub address: Option<String>,
    /// Whether the provider says it verified that address.
    pub address_verified: bool,
    pub name: Option<String>,
}

/// The claims of `token` once it checks against `keys`: signed by one,
/// from exactly `issuer`, for `client_id` (and `azp` naming it when it
/// names more than one audience, or names one at all), within its times,
/// and carrying `nonce`.
pub(crate) fn id_token(
    token: &str,
    keys: &[Value],
    issuer: &str,
    client_id: &str,
    nonce: &str,
    now: i64,
) -> Result<IdClaims, jadpo_value::Fault> {
    let refused = || jadpo_value::Fault::Broke(jadpo_std::RuleKey::Malformed);
    if token.len() > jadpo_std::standard::security::JWT_MAX_LENGTH {
        return Err(refused());
    }
    let (_, members) =
        jadpo_value::jwt::verify_jwt_members(token, keys, Some(issuer), Some(client_id), now)?;
    let text = |name: &str| members.get(name).and_then(serde_json::Value::as_str);
    let audiences = match members.get("aud") {
        Some(serde_json::Value::Array(items)) => items.len(),
        _ => 1,
    };
    match text("azp") {
        Some(azp) if azp != client_id => return Err(refused()),
        None if audiences > 1 => return Err(refused()),
        _ => {}
    }
    match text("nonce") {
        Some(given) if constant_eq(given.as_bytes(), nonce.as_bytes()) => {}
        _ => return Err(refused()),
    }
    let subject = text("sub")
        .filter(|subject| !subject.is_empty() && subject.len() <= 255)
        .ok_or_else(refused)?
        .to_string();
    let address = text("email")
        .filter(|address| jadpo_std::is_email(address))
        .map(str::to_string);
    let address_verified =
        address.is_some() && members.get("email_verified") == Some(&serde_json::Value::Bool(true));
    let name = text("name")
        .filter(|name| !name.trim().is_empty())
        .map(str::to_string);
    Ok(IdClaims {
        subject,
        address,
        address_verified,
        name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use jadpo_value::crypto::{KeyRing, public_key, sign};
    use jadpo_value::encoding::to_base64url;

    const NOW: i64 = 1_800_000_000_000;
    const ISSUER: &str = "https://accounts.google.com";
    const CLIENT: &str = "desk.apps.googleusercontent.com";

    /// The nonce a sign-in derives, as `start` and `callback` do.
    fn nonce() -> String {
        derived("a browser's binding", "a sign-in's state").1
    }

    fn signer() -> KeyRing {
        KeyRing::from_signing_text("0123456789abcdef0123456789abcdef", None)
    }

    fn keys() -> Vec<Value> {
        vec![
            jadpo_value::jwt::read_jwk(&serde_json::json!({
                "kty":"OKP", "crv":"Ed25519", "kid":"current",
                "x":public_key(&signer()).expect("public key")
            }))
            .expect("a public JWK"),
        ]
    }

    fn token(claims: serde_json::Value) -> String {
        let header = r#"{"alg":"EdDSA","kid":"current"}"#;
        let body = format!(
            "{}.{}",
            to_base64url(header.as_bytes()),
            to_base64url(claims.to_string().as_bytes())
        );
        let signature = sign(body.as_bytes(), &signer()).expect("signed");
        format!("{body}.{}", to_base64url(&signature))
    }

    fn claims() -> serde_json::Value {
        serde_json::json!({
            "iss": ISSUER, "aud": CLIENT, "sub": "1098", "nonce": nonce(),
            "exp": NOW / 1000 + 300, "iat": NOW / 1000,
            "email": "ann@desk.example", "email_verified": true, "name": "Ann",
            "role": "administrator", "groups": ["staff-admins"],
        })
    }

    fn checked(claims: serde_json::Value) -> Option<IdClaims> {
        id_token(&token(claims), &keys(), ISSUER, CLIENT, &nonce(), NOW).ok()
    }

    #[test]
    fn only_the_subject_address_and_name_cross_into_sign_in() {
        assert_eq!(
            checked(claims()),
            Some(IdClaims {
                subject: "1098".into(),
                address: Some("ann@desk.example".into()),
                address_verified: true,
                name: Some("Ann".into()),
            })
        );
        // An address the provider doesn't say it verified is unverified,
        // as is one it says so of as text.
        let mut unverified = claims();
        unverified["email_verified"] = "true".into();
        assert!(checked(unverified).is_some_and(|claims| !claims.address_verified));
        let mut absent = claims();
        absent
            .as_object_mut()
            .expect("claims")
            .remove("email_verified");
        assert!(checked(absent).is_some_and(|claims| !claims.address_verified));
        // Something that isn't an address is no address at all.
        let mut odd = claims();
        odd["email"] = "not an address".into();
        assert!(checked(odd).is_some_and(|claims| claims.address.is_none()));
    }

    #[test]
    fn a_token_from_elsewhere_for_another_client_or_another_sign_in_is_refused() {
        let mut cases = Vec::new();
        let mut other_issuer = claims();
        other_issuer["iss"] = "https://accounts.google.com/".into();
        cases.push(other_issuer);
        let mut other_client = claims();
        other_client["aud"] = "someone-else".into();
        cases.push(other_client);
        let mut shared = claims();
        shared["aud"] = serde_json::json!([CLIENT, "someone-else"]);
        cases.push(shared);
        let mut wrong_party = claims();
        wrong_party["azp"] = "someone-else".into();
        cases.push(wrong_party);
        let mut replayed = claims();
        replayed["nonce"] = "another-sign-in".into();
        cases.push(replayed);
        let mut no_nonce = claims();
        no_nonce.as_object_mut().expect("claims").remove("nonce");
        cases.push(no_nonce);
        let mut expired = claims();
        expired["exp"] = (NOW / 1000 - 60).into();
        cases.push(expired);
        let mut no_subject = claims();
        no_subject["sub"] = "".into();
        cases.push(no_subject);
        for case in cases {
            assert!(checked(case.clone()).is_none(), "{case}");
        }
        // Several audiences hold when `azp` names this client.
        let mut named = claims();
        named["aud"] = serde_json::json!([CLIENT, "someone-else"]);
        named["azp"] = CLIENT.into();
        assert!(checked(named).is_some());
        // A token no key signed is refused.
        assert!(id_token(&token(claims()), &[], ISSUER, CLIENT, &nonce(), NOW).is_err());
    }

    #[test]
    fn the_verifier_and_the_nonce_come_from_the_cookie_and_the_state() {
        let (pkce, nonce) = derived("cookie", "state");
        assert_eq!((pkce.clone(), nonce.clone()), derived("cookie", "state"));
        assert_ne!(pkce, nonce);
        assert_ne!(derived("another cookie", "state").0, pkce);
        assert_ne!(derived("cookie", "another state").1, nonce);
        // PKCE's verifier is 43 to 128 unreserved characters.
        assert_eq!(pkce.len(), 43);
        assert!(
            pkce.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        );
    }
}
