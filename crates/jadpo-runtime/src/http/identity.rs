//! Signing in (area 8; docs/compiler-runtime.md §2.6, "Identity"): who is
//! calling, decided before a route's input is read or anything it runs
//! starts, the same for every host and executor.
//!
//! - A request carries exactly one credential: the browser session's
//!   cookie, when the route's API accepts browser sessions, or an
//!   `Authorization` header. None is 401 `authentication_required`; two
//!   (a cookie and a bearer, or either twice) are 401
//!   `ambiguous_credentials`, never picked from.
//! - A bearer credential is an API token (`jadpo_token_…`) or a service
//!   key (`jadpo_key_…`), accepted where the route's API accepts its
//!   method; anything else is 401 `invalid_credentials`.
//! - A session or token is found by its verifier with its user's record,
//!   and a key with its service's, in one statement (the plan's
//!   [`PrincipalPlan::lookup`]): none, or one past its end, is 401
//!   `invalid_credentials`, which also clears a session's cookies; a
//!   record whose `active:` doesn't hold is 403 `user_disabled` or
//!   `service_disabled`; a store that can't answer is 503
//!   `authentication_unavailable`.
//! - A write that comes with the cookie needs an `Origin` naming this
//!   server and the session's CSRF token in `X-Jadpo-CSRF`, or it is 403
//!   `csrf_rejected`. A bearer credential needs neither: a browser never
//!   sends one by itself.
//! - `sessions.start(user_id)` keeps a new session in the action's own
//!   transaction, and the route's response sets its cookies once the
//!   action commits; `sessions.end()` deletes the request's session or
//!   API token and the response clears the cookies.
//! - `sessions.issue_token`, `.tokens` and `.revoke_token`, and the service
//!   entity's `issue_key`, `keys` and `revoke_key`, keep, list and delete
//!   tokens and keys in the action's transaction; a response carrying a
//!   new one is never kept for an idempotent repeat.
//!
//! Only verifiers are kept: a session's token and its CSRF token, an API
//! token and a service key are 32 random bytes each, sent once, and stored
//! as their BLAKE3 digests, so a copy of the database signs no one in.

use jadpo_std::AuthenticationMethod;
use jadpo_std::http::{
    API_TOKEN_PREFIX, CSRF_HEADER, ISSUED_KEY, ISSUED_TOKEN, IdentityPlan, PrincipalKind,
    PrincipalPlan, RoutePlan, RouteTable, SERVICE_KEY_PREFIX, SESSION_LIFETIME_MS,
};
use jadpo_std::read::SqlType;
use jadpo_value::Value;

use super::{Request, Response};
use crate::ctx::{Caller, SessionEffect};
use crate::read::Interrupt;
use crate::{
    ActionBody, Ctx, Fault, FaultKind, Isolation, LocalFuture, Outcome, SqlValue, StoreError,
    Stored, action,
};

/// The runtime's own failures for signing in (area 8, "Failures").
pub(crate) const AUTHENTICATION_REQUIRED: (u16, &str, &str) =
    (401, "authentication_required", "Sign in first.");
pub(crate) const INVALID_CREDENTIALS: (u16, &str, &str) = (
    401,
    "invalid_credentials",
    "The credentials are invalid or have expired.",
);
pub(crate) const AMBIGUOUS_CREDENTIALS: (u16, &str, &str) = (
    401,
    "ambiguous_credentials",
    "The request carries more than one credential.",
);
pub(crate) const CSRF_REJECTED: (u16, &str, &str) = (
    403,
    "csrf_rejected",
    "The request's origin or CSRF token is missing or wrong.",
);
pub(crate) const USER_DISABLED: (u16, &str, &str) =
    (403, "user_disabled", "This account is disabled.");
pub(crate) const SERVICE_DISABLED: (u16, &str, &str) =
    (403, "service_disabled", "This service is disabled.");
pub(crate) const NOT_PERMITTED: (u16, &str, &str) = (403, "not_permitted", "Not permitted.");
pub(crate) const AUTHENTICATION_UNAVAILABLE: (u16, &str, &str) = (
    503,
    "authentication_unavailable",
    "Signing in is temporarily unavailable.",
);

/// Keeps a new session, or with a `NULL` CSRF verifier an API token.
pub(crate) const START: &str = "INSERT INTO \"jadpo_sessions\" (\"id\", \"principal_id\", \"verifier\", \"csrf_verifier\", \"started_at\", \"ends_at\") VALUES ($1, $2, $3, $4, $5, $6)";

/// Ends one.
pub(crate) const END: &str = "DELETE FROM \"jadpo_sessions\" WHERE \"id\" = $1";

/// A user's API tokens that haven't ended, oldest first.
const TOKENS: &str = "SELECT \"id\", \"started_at\", \"ends_at\" FROM \"jadpo_sessions\" WHERE \"principal_id\" = $1 AND \"csrf_verifier\" IS NULL AND \"ends_at\" > $2 ORDER BY \"started_at\", \"id\"";

/// Ends one API token, never a browser session.
const REVOKE_TOKEN: &str =
    "DELETE FROM \"jadpo_sessions\" WHERE \"id\" = $1 AND \"csrf_verifier\" IS NULL";

/// Keeps a new service key.
const ISSUE_KEY: &str = "INSERT INTO \"jadpo_service_keys\" (\"id\", \"principal_id\", \"verifier\", \"started_at\", \"ends_at\") VALUES ($1, $2, $3, $4, $5)";

/// A service's keys that haven't expired, oldest first.
const KEYS: &str = "SELECT \"id\", \"started_at\", \"ends_at\" FROM \"jadpo_service_keys\" WHERE \"principal_id\" = $1 AND \"ends_at\" > $2 ORDER BY \"started_at\", \"id\"";

/// Deletes one service key.
const REVOKE_KEY: &str = "DELETE FROM \"jadpo_service_keys\" WHERE \"id\" = $1";

/// How long a route test's service key lasts: as long as a session.
const TEST_KEY_LIFETIME_MS: i64 = SESSION_LIFETIME_MS;

/// Why a request isn't signed in, as the response that says so.
pub(crate) struct Refused {
    pub failure: (u16, &'static str, &'static str),
    /// Whether the response clears the session's cookies: the one it came
    /// with is no good.
    pub clear: bool,
    /// Whether the request carried a credential, so the refusal counts
    /// against its address's rate limit.
    pub credential: bool,
}

impl Refused {
    fn new(failure: (u16, &'static str, &'static str)) -> Refused {
        Refused {
            failure,
            clear: false,
            credential: true,
        }
    }
}

/// The credentials a request carries, as the route's API accepts them.
struct Credentials<'r> {
    /// The session cookie's values, one per time it is sent.
    cookies: Vec<&'r str>,
    /// Each `Authorization` header.
    authorizations: usize,
}

fn credentials<'r>(table: &RouteTable, route: &RoutePlan, request: &'r Request) -> Credentials<'r> {
    let accepts_cookie = table
        .apis
        .get(route.api as usize)
        .is_some_and(|api| api.methods.contains(&AuthenticationMethod::BrowserSession));
    let name = table
        .identity
        .as_ref()
        .and_then(|identity| identity.cookie.as_deref())
        .filter(|_| accepts_cookie);
    let cookies = match name {
        Some(name) => cookie_values(request, name),
        None => Vec::new(),
    };
    let authorizations = request
        .headers
        .iter()
        .filter(|(header, _)| header.eq_ignore_ascii_case("authorization"))
        .count();
    Credentials {
        cookies,
        authorizations,
    }
}

/// Whether a request carries anything that could be a credential, before
/// it is routed: the session's cookie or an `Authorization` header. Such a
/// request counts against its caller's rate limit once signed in, rather
/// than its address's.
pub(crate) fn carries_credential(table: &RouteTable, request: &Request) -> bool {
    let cookie = table
        .identity
        .as_ref()
        .and_then(|identity| identity.cookie.as_deref())
        .is_some_and(|name| !cookie_values(request, name).is_empty());
    cookie || request.header("authorization").is_some()
}

/// Every value cookie `name` has in the request's `Cookie` headers.
fn cookie_values<'r>(request: &'r Request, name: &str) -> Vec<&'r str> {
    request
        .headers
        .iter()
        .filter(|(header, _)| header.eq_ignore_ascii_case("cookie"))
        .flat_map(|(_, value)| value.split(';'))
        .filter_map(|pair| {
            let (key, value) = pair.trim().split_once('=')?;
            (key.trim() == name).then(|| value.trim())
        })
        .collect()
}

/// Who is calling `route`: the signed-in caller, or why the request is
/// refused.
pub(crate) async fn authenticate(
    table: &RouteTable,
    ctx: &mut Ctx<'_>,
    request: &Request,
    route: &RoutePlan,
) -> Result<Caller, Refused> {
    let found = credentials(table, route, request);
    let count = found.cookies.len() + found.authorizations;
    if count == 0 {
        return Err(Refused {
            credential: false,
            ..Refused::new(AUTHENTICATION_REQUIRED)
        });
    }
    if count > 1 {
        return Err(Refused::new(AMBIGUOUS_CREDENTIALS));
    }
    let Some(identity) = &table.identity else {
        return Err(Refused::new(INVALID_CREDENTIALS));
    };
    if let Some(token) = found.cookies.first().copied() {
        return cookie(identity, ctx, request, route, token).await;
    }
    let accepts = |method: AuthenticationMethod| {
        table
            .apis
            .get(route.api as usize)
            .is_some_and(|api| api.methods.contains(&method))
    };
    let credential = request.header("authorization").and_then(bearer);
    let Some(credential) = credential else {
        return Err(Refused::new(INVALID_CREDENTIALS));
    };
    if credential.starts_with(API_TOKEN_PREFIX)
        && identity.api_tokens
        && accepts(AuthenticationMethod::ApiToken)
        && is_token(&credential[API_TOKEN_PREFIX.len()..])
    {
        let user = identity.user.as_ref().filter(|user| user.lookup.is_some());
        let Some(user) = user else {
            return Err(Refused::new(INVALID_CREDENTIALS));
        };
        let row = look_up(ctx, user, credential).await?;
        let Some(
            [
                SqlValue::Uuid(session),
                SqlValue::Null(_),
                id,
                subject,
                SqlValue::Bool(active),
            ],
        ) = row.as_deref()
        else {
            return Err(Refused::new(INVALID_CREDENTIALS));
        };
        if !*active {
            return Err(Refused::new(USER_DISABLED));
        }
        return caller_of(
            route,
            user,
            PrincipalKind::User,
            id,
            subject,
            Some(*session),
        );
    }
    if credential.starts_with(SERVICE_KEY_PREFIX)
        && accepts(AuthenticationMethod::ServiceKey)
        && is_token(&credential[SERVICE_KEY_PREFIX.len()..])
    {
        let service = identity
            .service
            .as_ref()
            .filter(|service| service.lookup.is_some());
        let Some(service) = service else {
            return Err(Refused::new(INVALID_CREDENTIALS));
        };
        let row = look_up(ctx, service, credential).await?;
        let Some([SqlValue::Uuid(_), id, subject, SqlValue::Bool(active)]) = row.as_deref() else {
            return Err(Refused::new(INVALID_CREDENTIALS));
        };
        if !*active {
            return Err(Refused::new(SERVICE_DISABLED));
        }
        return caller_of(route, service, PrincipalKind::Service, id, subject, None);
    }
    Err(Refused::new(INVALID_CREDENTIALS))
}

/// A request that came with the browser session's cookie: its session,
/// and for a write the CSRF check.
async fn cookie(
    identity: &IdentityPlan,
    ctx: &mut Ctx<'_>,
    request: &Request,
    route: &RoutePlan,
    token: &str,
) -> Result<Caller, Refused> {
    let user = identity.user.as_ref().filter(|user| user.lookup.is_some());
    let Some(user) = user.filter(|_| identity.cookie.is_some()) else {
        return Err(Refused::new(INVALID_CREDENTIALS));
    };
    let invalid = Refused {
        clear: true,
        ..Refused::new(INVALID_CREDENTIALS)
    };
    if !is_token(token) {
        return Err(invalid);
    }
    let row = look_up(ctx, user, token).await?;
    // An API token has no CSRF token, and never comes as the cookie.
    let Some(
        [
            SqlValue::Uuid(session),
            csrf @ SqlValue::Text(_),
            id,
            subject,
            SqlValue::Bool(active),
        ],
    ) = row.as_deref()
    else {
        return Err(invalid);
    };
    if !*active {
        return Err(Refused::new(USER_DISABLED));
    }
    if route.method != jadpo_std::HttpMethod::Get && !csrf_holds(request, csrf) {
        return Err(Refused::new(CSRF_REJECTED));
    }
    caller_of(
        route,
        user,
        PrincipalKind::User,
        id,
        subject,
        Some(*session),
    )
}

/// The row a credential's verifier finds, with its caller's record: none
/// when nothing live has it.
async fn look_up(
    ctx: &mut Ctx<'_>,
    plan: &PrincipalPlan,
    credential: &str,
) -> Result<Option<Vec<SqlValue>>, Refused> {
    let Some(lookup) = plan.lookup.as_deref() else {
        return Ok(None);
    };
    let Some(database) = ctx.host().database(&plan.store) else {
        return Err(Refused::new(AUTHENTICATION_UNAVAILABLE));
    };
    let Ok(mut transaction) = database.begin_at(Isolation::ReadCommitted).await else {
        return Err(Refused::new(AUTHENTICATION_UNAVAILABLE));
    };
    let rows = transaction
        .query(
            lookup,
            &[
                SqlValue::Text(verifier(credential)),
                SqlValue::Timestamp(ctx.now().saturating_mul(1000)),
            ],
        )
        .await;
    let _ = transaction.commit().await;
    let rows = rows.map_err(|_| Refused::new(AUTHENTICATION_UNAVAILABLE))?;
    Ok(rows.into_iter().next())
}

/// The caller a credential signed in, once the route accepts its kind.
fn caller_of(
    route: &RoutePlan,
    plan: &PrincipalPlan,
    kind: PrincipalKind,
    id: &SqlValue,
    subject: &SqlValue,
    session: Option<[u8; 16]>,
) -> Result<Caller, Refused> {
    if route.caller.is_some_and(|wanted| wanted != kind) {
        return Err(Refused::new(NOT_PERMITTED));
    }
    let id = crate::from_sql(id, &jadpo_std::read::Column::Uuid)
        .map_err(|_| Refused::new(AUTHENTICATION_UNAVAILABLE))?;
    let subject = crate::from_sql(subject, &plan.column)
        .map_err(|_| Refused::new(AUTHENTICATION_UNAVAILABLE))?;
    Ok(Caller {
        key: format!("{} {id}", kind.name()),
        value: principal(kind, id, subject),
        session,
    })
}

/// The credential an `Authorization` header carries: `Bearer <value>`,
/// the scheme in any case.
fn bearer(header: &str) -> Option<&str> {
    let (scheme, value) = header.trim().split_once(' ')?;
    let value = value.trim();
    (scheme.eq_ignore_ascii_case("bearer") && !value.is_empty()).then_some(value)
}

/// The caller value code receives (area 8, question 2).
pub fn principal(kind: PrincipalKind, id: Value, subject: Value) -> Value {
    Value::record(
        kind.type_name(),
        vec![
            ("id".into(), id),
            ("subject".into(), subject),
            (
                "strength".into(),
                Value::variant(
                    jadpo_std::AuthenticationStrength::NAME,
                    0,
                    jadpo_std::AuthenticationStrength::Primary.name(),
                    Vec::new(),
                ),
            ),
        ],
    )
}

/// A caller made from its record in a test, fixture or seed
/// (`Principal.user(user)`, area 8, question 10).
pub fn caller_from_record(
    plan: &PrincipalPlan,
    kind: PrincipalKind,
    record: &Value,
) -> Option<Value> {
    let Value::Record(fields) = record else {
        return None;
    };
    let id = fields.get("id")?.clone();
    let subject = fields.get(&plan.subject)?.clone();
    Some(principal(kind, id, subject))
}

/// Whether a write that came with the session's cookie may go ahead: its
/// `Origin` names the server it was sent to, and its `X-Jadpo-CSRF` is the
/// session's token. Until configuration names the application's origin
/// (area 10), the server's own is the one its `Host` header gives.
fn csrf_holds(request: &Request, csrf: &SqlValue) -> bool {
    let (Some(origin), Some(host)) = (request.header("origin"), request.header("host")) else {
        return false;
    };
    let Some((_, authority)) = origin.split_once("://") else {
        return false;
    };
    if !authority.eq_ignore_ascii_case(host.trim()) {
        return false;
    }
    let (Some(sent), SqlValue::Text(kept)) = (request.header(CSRF_HEADER), csrf) else {
        return false;
    };
    is_token(sent) && constant_eq(verifier(sent).as_bytes(), kept.as_bytes())
}

/// Compares in time that doesn't depend on where they differ.
fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Whether `text` could be a token the runtime made: 64 hexadecimal digits.
fn is_token(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| b.is_ascii_hexdigit())
}

/// What is kept of a token: its BLAKE3 digest, in hexadecimal.
fn verifier(token: &str) -> String {
    blake3::hash(token.as_bytes()).to_hex().to_string()
}

/// 32 random bytes as 64 hexadecimal digits.
fn new_token(ctx: &mut Ctx<'_>) -> Option<String> {
    let mut bytes = [0u8; 32];
    if !ctx.random_bytes(&mut bytes) {
        return None;
    }
    Some(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn defect(ctx: &mut Ctx<'_>, what: &str, site: u32) -> Box<Fault> {
    ctx.fault(FaultKind::Defect(what.to_string()), site)
}

/// `sessions.start(user_id)` at site `site`: keeps a new session for the
/// user in the running action's transaction, for its route's response to
/// set the cookies once the action commits.
pub async fn start(
    ctx: &mut Ctx<'_>,
    plan: Option<&IdentityPlan>,
    user: &Value,
    site: u32,
) -> Stored {
    let Some(plan) = plan else {
        return Stored::Fault(defect(
            ctx,
            "a session started with nothing to sign in",
            site,
        ));
    };
    let (Some(token), Some(csrf), Some(id)) = (new_token(ctx), new_token(ctx), ctx.new_id()) else {
        return Stored::Fault(defect(ctx, "a session started with no randomness", site));
    };
    let Ok(user) = crate::to_sql(user, SqlType::Uuid) else {
        return Stored::Fault(defect(
            ctx,
            "a session started for a value that isn't an id",
            site,
        ));
    };
    let now = ctx.now();
    let params = [
        SqlValue::Uuid(id),
        user,
        SqlValue::Text(verifier(&token)),
        SqlValue::Text(verifier(&csrf)),
        SqlValue::Timestamp(now.saturating_mul(1000)),
        SqlValue::Timestamp(now.saturating_add(SESSION_LIFETIME_MS).saturating_mul(1000)),
    ];
    let store = plan.store.clone();
    let written = match ctx.transaction(&store).await {
        Ok(transaction) => transaction.execute(START, &params).await,
        Err(error) => Err(error),
    };
    if let Err(error) = written {
        return stopped(ctx, error, site);
    }
    ctx.session(SessionEffect::Start { token, csrf });
    Stored::Value(Value::Unit)
}

/// `sessions.end()` at site `site`: deletes the session the request came
/// with, in the running action's transaction, for its route's response to
/// clear the cookies. A request with no session ends none.
pub async fn end(ctx: &mut Ctx<'_>, plan: Option<&IdentityPlan>, site: u32) -> Stored {
    let Some(plan) = plan else {
        return Stored::Fault(defect(ctx, "a session ended with nothing to sign in", site));
    };
    if let Some(session) = ctx.caller().and_then(|caller| caller.session) {
        let store = plan.store.clone();
        let deleted = match ctx.transaction(&store).await {
            Ok(transaction) => transaction.execute(END, &[SqlValue::Uuid(session)]).await,
            Err(error) => Err(error),
        };
        if let Err(error) = deleted {
            return stopped(ctx, error, site);
        }
    }
    ctx.session(SessionEffect::End);
    Stored::Value(Value::Unit)
}

/// `sessions.issue_token(user_id)` at site `site`: keeps a new API token
/// for the user in the running action's transaction and gives it, the one
/// time anyone sees it.
pub async fn issue_token(
    ctx: &mut Ctx<'_>,
    plan: Option<&IdentityPlan>,
    user: &Value,
    site: u32,
) -> Stored {
    let Some(plan) = plan else {
        return Stored::Fault(defect(ctx, "a token issued with nothing to sign in", site));
    };
    let (Some(token), Some(id)) = (new_token(ctx), ctx.new_id()) else {
        return Stored::Fault(defect(ctx, "a token issued with no randomness", site));
    };
    let Ok(user) = crate::to_sql(user, SqlType::Uuid) else {
        return Stored::Fault(defect(
            ctx,
            "a token issued for a value that isn't an id",
            site,
        ));
    };
    let token = format!("{API_TOKEN_PREFIX}{token}");
    let now = ctx.now();
    let params = [
        SqlValue::Uuid(id),
        user,
        SqlValue::Text(verifier(&token)),
        SqlValue::Null(SqlType::Text),
        SqlValue::Timestamp(now.saturating_mul(1000)),
        SqlValue::Timestamp(now.saturating_add(SESSION_LIFETIME_MS).saturating_mul(1000)),
    ];
    let store = plan.store.clone();
    if let Err(error) = execute(ctx, &store, START, &params).await {
        return stopped(ctx, error, site);
    }
    ctx.issued();
    Stored::Value(Value::text(&token))
}

/// `Service.issue_key(service_id, expires_at)` at site `site`: keeps a new
/// key for the service in the running action's transaction and gives it,
/// the one time anyone sees it.
pub async fn issue_key(
    ctx: &mut Ctx<'_>,
    plan: Option<&IdentityPlan>,
    service: &Value,
    expires_at: &Value,
    site: u32,
) -> Stored {
    let Some(store) = plan.and_then(IdentityPlan::keys_store).map(str::to_string) else {
        return Stored::Fault(defect(ctx, "a key issued with no service keys", site));
    };
    let (Some(key), Some(id)) = (new_token(ctx), ctx.new_id()) else {
        return Stored::Fault(defect(ctx, "a key issued with no randomness", site));
    };
    let (Ok(service), Value::Instant(expires_at)) =
        (crate::to_sql(service, SqlType::Uuid), expires_at)
    else {
        return Stored::Fault(defect(
            ctx,
            "a key issued for values that aren't an id and a time",
            site,
        ));
    };
    let key = format!("{SERVICE_KEY_PREFIX}{key}");
    let params = vec![
        SqlValue::Uuid(id),
        service,
        SqlValue::Text(verifier(&key)),
        SqlValue::Timestamp(ctx.now().saturating_mul(1000)),
        SqlValue::Timestamp(expires_at.saturating_mul(1000)),
    ];
    let gated = plan.and_then(|plan| plan.gated.issue_key.as_ref());
    let Some((statement, params)) = policed(ctx, ISSUE_KEY, gated, params) else {
        return Stored::Fault(unpoliced(ctx, site));
    };
    match execute(ctx, &store, &statement, &params).await {
        Ok(0) if gated.is_some() && !ctx.unpoliced() => {
            return Stored::Failed(ctx.fail(crate::read::not_permitted(), site));
        }
        Ok(_) => {}
        Err(error) => return stopped(ctx, error, site),
    }
    ctx.issued();
    Stored::Value(Value::text(&key))
}

/// `sessions.tokens(user_id)` or `Service.keys(service_id)` at site
/// `site`: the caller's tokens or keys that haven't ended, oldest first,
/// as `IssuedToken`s or `IssuedKey`s.
pub async fn issued(
    ctx: &mut Ctx<'_>,
    plan: Option<&IdentityPlan>,
    keys: bool,
    holder: &Value,
    site: u32,
) -> Stored {
    let store = match (plan, keys) {
        (Some(plan), false) => Some(plan.store.clone()),
        (Some(plan), true) => plan.keys_store().map(str::to_string),
        (None, _) => None,
    };
    let Some(store) = store else {
        return Stored::Fault(defect(
            ctx,
            "tokens or keys listed with nothing to sign in",
            site,
        ));
    };
    let Ok(holder) = crate::to_sql(holder, SqlType::Uuid) else {
        return Stored::Fault(defect(
            ctx,
            "tokens or keys listed for a value that isn't an id",
            site,
        ));
    };
    let params = vec![holder, SqlValue::Timestamp(ctx.now().saturating_mul(1000))];
    let (statement, gated) = if keys {
        (KEYS, plan.and_then(|plan| plan.gated.keys.as_ref()))
    } else {
        (TOKENS, plan.and_then(|plan| plan.gated.tokens.as_ref()))
    };
    let Some((statement, params)) = policed(ctx, statement, gated, params) else {
        return Stored::Fault(unpoliced(ctx, site));
    };
    let rows = match ctx.transaction(&store).await {
        Ok(transaction) => transaction.query(&statement, &params).await,
        Err(error) => Err(error),
    };
    let rows = match rows {
        Ok(rows) => rows,
        Err(error) => return stopped(ctx, error, site),
    };
    let ty = if keys { ISSUED_KEY } else { ISSUED_TOKEN };
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let mut fields = Vec::with_capacity(jadpo_std::http::ISSUED_FIELDS.len());
        for ((name, builtin), column) in jadpo_std::http::ISSUED_FIELDS.iter().zip(&row) {
            let read = if *builtin == jadpo_std::Builtin::Uuid {
                jadpo_std::read::Column::Uuid
            } else {
                jadpo_std::read::Column::Instant
            };
            match crate::from_sql(column, &read) {
                Ok(value) => fields.push(((*name).into(), value)),
                Err(error) => return Stored::Fault(defect(ctx, &error, site)),
            }
        }
        items.push(Value::record(ty, fields));
    }
    Stored::Value(Value::list(items))
}

/// A `GET` route that lists tokens or keys itself (area 8, "Listing tokens
/// and keys from a `GET`"): `holder`'s, as `sessions.tokens` and
/// `Service.keys` list them in an action, in a boundary of its own as a
/// one-statement read at repeatable read, limited by the holder's record's
/// `read` grants like any query.
pub async fn list(
    ctx: &mut Ctx<'_>,
    plan: Option<&IdentityPlan>,
    keys: bool,
    holder: &Value,
) -> Stored {
    let mut alone = Listing { plan, keys, holder };
    match action(ctx, &mut alone).await {
        Outcome::Value(value) => Stored::Value(value),
        Outcome::Failed(failure) => Stored::Failed(failure),
        Outcome::Fault(fault) => Stored::Fault(fault),
        Outcome::Assertion(_) => Stored::Fault(ctx.fault(
            FaultKind::Defect("an assertion from a listing".into()),
            crate::Frame::ENTRY,
        )),
    }
}

/// The listing as its boundary's body.
struct Listing<'o> {
    plan: Option<&'o IdentityPlan>,
    keys: bool,
    holder: &'o Value,
}

impl<'p> ActionBody<'p> for Listing<'_> {
    fn isolation(&self) -> Isolation {
        Isolation::RepeatableRead
    }

    fn run<'c>(&'c mut self, ctx: &'c mut Ctx<'p>) -> LocalFuture<'c, Outcome> {
        Box::pin(async move {
            match issued(ctx, self.plan, self.keys, self.holder, crate::Frame::ENTRY).await {
                Stored::Value(value) => Outcome::Value(value),
                Stored::Failed(failure) => Outcome::Failed(failure),
                Stored::Conflict { cause, .. } => Outcome::Failed(cause),
                Stored::Fault(fault) => Outcome::Fault(fault),
            }
        })
    }
}

/// `sessions.revoke_token(token_id)` or `Service.revoke_key(key_id)` at
/// site `site`: deletes it in the running action's transaction; an `id`
/// nothing has deletes nothing.
pub async fn revoke(
    ctx: &mut Ctx<'_>,
    plan: Option<&IdentityPlan>,
    keys: bool,
    id: &Value,
    site: u32,
) -> Stored {
    let store = match (plan, keys) {
        (Some(plan), false) => Some(plan.store.clone()),
        (Some(plan), true) => plan.keys_store().map(str::to_string),
        (None, _) => None,
    };
    let Some(store) = store else {
        return Stored::Fault(defect(
            ctx,
            "a token or key revoked with nothing to sign in",
            site,
        ));
    };
    let Ok(id) = crate::to_sql(id, SqlType::Uuid) else {
        return Stored::Fault(defect(
            ctx,
            "a token or key revoked by a value that isn't an id",
            site,
        ));
    };
    let (statement, gated) = if keys {
        (
            REVOKE_KEY,
            plan.and_then(|plan| plan.gated.revoke_key.as_ref()),
        )
    } else {
        (
            REVOKE_TOKEN,
            plan.and_then(|plan| plan.gated.revoke_token.as_ref()),
        )
    };
    let Some((statement, params)) = policed(ctx, statement, gated, vec![id]) else {
        return Stored::Fault(unpoliced(ctx, site));
    };
    if let Err(error) = execute(ctx, &store, &statement, &params).await {
        return stopped(ctx, error, site);
    }
    Stored::Value(Value::Unit)
}

/// A credential statement with the policy its holder's grants add (area
/// 9, question 8), and its parameters with the caller's after `own`; the
/// statement without policy only for a call a test makes without a
/// caller. A policed call whose plan has no policed statement runs
/// nothing: `None`, never the statement without policy.
fn policed(
    ctx: &Ctx<'_>,
    plain: &str,
    gated: Option<&jadpo_std::policy::Gated>,
    mut own: Vec<SqlValue>,
) -> Option<(String, Vec<SqlValue>)> {
    if ctx.unpoliced() {
        return Some((plain.to_string(), own));
    }
    let gated = gated?;
    for param in &gated.extra {
        own.push(match &param.source {
            jadpo_std::read::Source::Caller(part) => ctx.caller_param(*part),
            _ => SqlValue::Timestamp(ctx.now().saturating_mul(1000)),
        });
    }
    Some((gated.sql.clone(), own))
}

/// The fault a policed credential statement the plan has no policy for
/// is: a mistake in the plan, which refuses rather than runs unpoliced.
fn unpoliced(ctx: &mut Ctx<'_>, site: u32) -> Box<Fault> {
    defect(
        ctx,
        "a credential statement with no policy for a caller",
        site,
    )
}

/// Runs `statement` in the running action's transaction in `store`.
async fn execute(
    ctx: &mut Ctx<'_>,
    store: &str,
    statement: &str,
    params: &[SqlValue],
) -> Result<u64, StoreError> {
    ctx.transaction(store)
        .await?
        .execute(statement, params)
        .await
}

/// A store's refusal of a session's statement, as what the runtime raises.
fn stopped(ctx: &mut Ctx<'_>, error: StoreError, site: u32) -> Stored {
    match crate::read::refused(ctx, error, site) {
        Interrupt::Failed(failure) => Stored::Failed(failure),
        Interrupt::Fault(fault) => Stored::Fault(fault),
    }
}

/// The `Set-Cookie` headers a session effect's response carries: the
/// session's token, which scripts can't read, and its CSRF token, which
/// the application's own pages read to send back in `X-Jadpo-CSRF`.
pub(crate) fn set_cookies(plan: &IdentityPlan, effect: &SessionEffect) -> Vec<(String, String)> {
    let (Some(cookie), Some(csrf_cookie)) = (plan.cookie.as_deref(), plan.csrf_cookie()) else {
        return Vec::new();
    };
    let lifetime = SESSION_LIFETIME_MS / 1000;
    let (token, csrf, age) = match effect {
        SessionEffect::Start { token, csrf } => (token.as_str(), csrf.as_str(), lifetime),
        SessionEffect::End => ("", "", 0),
    };
    vec![
        (
            "set-cookie".to_string(),
            format!("{cookie}={token}; Path=/; Max-Age={age}; HttpOnly; Secure; SameSite=Strict"),
        ),
        (
            "set-cookie".to_string(),
            format!("{csrf_cookie}={csrf}; Path=/; Max-Age={age}; Secure; SameSite=Strict"),
        ),
    ]
}

/// The response a refusal gives.
pub(crate) fn refusal(table: &RouteTable, refused: &Refused, request_id: &str) -> Response {
    let mut response = super::runtime_failure(refused.failure, request_id, None);
    if refused.clear
        && let Some(plan) = &table.identity
    {
        response
            .headers
            .extend(set_cookies(plan, &SessionEffect::End));
    }
    response
}

/// The headers a route test's request to route number `route` signs in
/// with as `caller` (area 13, `as:`): a real credential, kept in its own
/// transaction and sent as a client sends it. A user signs in with a
/// browser session, its CSRF token and an `Origin` naming the server, when
/// the route's API accepts browser sessions, and otherwise with an API
/// token; a service with a key, as `Service.issue_key` issues one.
pub async fn test_credentials(
    ctx: &mut Ctx<'_>,
    table: &RouteTable,
    route: u32,
    caller: &Value,
) -> Result<Vec<(String, String)>, Box<Fault>> {
    let site = 0;
    let Some(plan) = &table.identity else {
        return Err(defect(
            ctx,
            "a request signed in with nothing to sign in",
            site,
        ));
    };
    let (kind, id) = match caller {
        Value::Record(record) => (
            [PrincipalKind::User, PrincipalKind::Service]
                .into_iter()
                .find(|kind| *record.ty == *kind.type_name()),
            record.get("id").cloned(),
        ),
        _ => (None, None),
    };
    let (Some(kind), Some(id)) = (kind, id) else {
        return Err(defect(
            ctx,
            "a request signed in as a value that isn't a caller",
            site,
        ));
    };
    let Ok(id) = crate::to_sql(&id, SqlType::Uuid) else {
        return Err(defect(ctx, "a caller whose id isn't one", site));
    };
    let (Some(token), Some(csrf), Some(row)) = (new_token(ctx), new_token(ctx), ctx.new_id())
    else {
        return Err(defect(ctx, "a request signed in with no randomness", site));
    };
    let accepts = |method: AuthenticationMethod| {
        table
            .routes
            .get(route as usize)
            .and_then(|route| table.apis.get(route.api as usize))
            .is_some_and(|api| api.methods.contains(&method))
    };
    let now = ctx.now();
    let ends =
        |lifetime: i64| SqlValue::Timestamp(now.saturating_add(lifetime).saturating_mul(1000));
    let started = SqlValue::Timestamp(now.saturating_mul(1000));
    let session = plan.cookie.clone().zip(plan.csrf_cookie()).filter(|_| {
        kind == PrincipalKind::User
            && (accepts(AuthenticationMethod::BrowserSession) || !plan.api_tokens)
    });
    let (store, statement, params, headers) = match (kind, session) {
        (PrincipalKind::User, Some((cookie, csrf_cookie))) => (
            plan.store.clone(),
            START,
            vec![
                SqlValue::Uuid(row),
                id,
                SqlValue::Text(verifier(&token)),
                SqlValue::Text(verifier(&csrf)),
                started,
                ends(SESSION_LIFETIME_MS),
            ],
            vec![
                (
                    "cookie".to_string(),
                    format!("{cookie}={token}; {csrf_cookie}={csrf}"),
                ),
                (CSRF_HEADER.to_string(), csrf),
                ("host".to_string(), TEST_HOST.to_string()),
                ("origin".to_string(), format!("http://{TEST_HOST}")),
            ],
        ),
        (PrincipalKind::User, None) if plan.api_tokens => {
            let token = format!("{API_TOKEN_PREFIX}{token}");
            (
                plan.store.clone(),
                START,
                vec![
                    SqlValue::Uuid(row),
                    id,
                    SqlValue::Text(verifier(&token)),
                    SqlValue::Null(SqlType::Text),
                    started,
                    ends(SESSION_LIFETIME_MS),
                ],
                vec![("authorization".to_string(), format!("Bearer {token}"))],
            )
        }
        (PrincipalKind::Service, _) if plan.keys_store().is_some() => {
            let key = format!("{SERVICE_KEY_PREFIX}{token}");
            (
                plan.keys_store().unwrap_or_default().to_string(),
                ISSUE_KEY,
                vec![
                    SqlValue::Uuid(row),
                    id,
                    SqlValue::Text(verifier(&key)),
                    started,
                    ends(TEST_KEY_LIFETIME_MS),
                ],
                vec![("authorization".to_string(), format!("Bearer {key}"))],
            )
        }
        _ => {
            return Err(defect(
                ctx,
                "a request signed in with nothing that signs this caller in",
                site,
            ));
        }
    };
    let unavailable = |ctx: &mut Ctx<'_>| {
        ctx.fault(
            FaultKind::Store("the store credentials are kept in couldn't be reached".into()),
            0,
        )
    };
    let Some(database) = ctx.host().database(&store) else {
        return Err(unavailable(ctx));
    };
    let Ok(mut transaction) = database.begin_at(Isolation::ReadCommitted).await else {
        return Err(unavailable(ctx));
    };
    if transaction.execute(statement, &params).await.is_err() {
        let _ = transaction.rollback().await;
        return Err(unavailable(ctx));
    }
    if transaction.commit().await.is_err() {
        return Err(unavailable(ctx));
    }
    Ok(headers)
}

/// The server a route test's requests are sent to.
const TEST_HOST: &str = "route-test";

#[cfg(test)]
mod tests {
    use super::*;

    fn request(headers: &[(&str, &str)]) -> Request {
        Request {
            method: "POST".into(),
            path: "/".into(),
            headers: headers
                .iter()
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .collect(),
            ..Request::default()
        }
    }

    #[test]
    fn a_bearer_credential_is_read_from_its_scheme_in_any_case() {
        assert_eq!(bearer("Bearer abc"), Some("abc"));
        assert_eq!(bearer("bearer  abc "), Some("abc"));
        assert_eq!(bearer("Basic abc"), None);
        assert_eq!(bearer("Bearer"), None);
        assert_eq!(bearer("Bearer "), None);
    }

    #[test]
    fn a_cookie_is_read_by_name_each_time_it_is_sent() {
        let sent = request(&[
            ("Cookie", "theme=dark; todo_session=abc"),
            ("cookie", "todo_session=def"),
        ]);
        assert_eq!(cookie_values(&sent, "todo_session"), ["abc", "def"]);
        assert!(cookie_values(&sent, "todo").is_empty());
    }

    #[test]
    fn a_write_needs_the_servers_origin_and_the_sessions_csrf_token() {
        let token = "a".repeat(64);
        let kept = SqlValue::Text(verifier(&token));
        let good = request(&[
            ("host", "app.test"),
            ("origin", "https://app.test"),
            ("x-jadpo-csrf", &token),
        ]);
        assert!(csrf_holds(&good, &kept));
        let elsewhere = request(&[
            ("host", "app.test"),
            ("origin", "https://evil.test"),
            ("x-jadpo-csrf", &token),
        ]);
        assert!(!csrf_holds(&elsewhere, &kept));
        let no_token = request(&[("host", "app.test"), ("origin", "https://app.test")]);
        assert!(!csrf_holds(&no_token, &kept));
        let wrong = request(&[
            ("host", "app.test"),
            ("origin", "https://app.test"),
            ("x-jadpo-csrf", &"b".repeat(64)),
        ]);
        assert!(!csrf_holds(&wrong, &kept));
    }

    #[test]
    fn a_session_sets_both_cookies_and_ending_it_clears_them() {
        let plan = IdentityPlan {
            store: "main".into(),
            signs_in: true,
            cookie: Some("todo_session".into()),
            api_tokens: false,
            user: None,
            service: None,
            gated: Default::default(),
        };
        let set = set_cookies(
            &plan,
            &SessionEffect::Start {
                token: "t".into(),
                csrf: "c".into(),
            },
        );
        assert_eq!(
            set[0].1,
            "todo_session=t; Path=/; Max-Age=2592000; HttpOnly; Secure; SameSite=Strict"
        );
        assert_eq!(
            set[1].1,
            "todo_session_csrf=c; Path=/; Max-Age=2592000; Secure; SameSite=Strict"
        );
        let cleared = set_cookies(&plan, &SessionEffect::End);
        assert!(cleared.iter().all(|(_, value)| value.contains("Max-Age=0")));
    }
}
