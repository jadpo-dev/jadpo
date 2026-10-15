//! Signing in (area 8; docs/compiler-runtime.md §2.6, "Identity"): who is
//! calling, decided before a route's input is read or anything it runs
//! starts, the same for every host and executor.
//!
//! - A request carries exactly one credential: the browser session's
//!   cookie, when the route's API accepts browser sessions, or an
//!   `Authorization` header. None is 401 `authentication_required`; two
//!   (a cookie and a bearer, or either twice) are 401
//!   `ambiguous_credentials`, never picked from.
//! - A session is found by its token's verifier, with its user's record,
//!   in one statement (the plan's [`PrincipalPlan::session`]): none, or
//!   one past its 30 days, is 401 `invalid_credentials`, which also clears
//!   the cookies; a record whose `active:` doesn't hold is 403
//!   `user_disabled`; a store that can't answer is 503
//!   `authentication_unavailable`.
//! - A write that comes with the cookie needs an `Origin` naming this
//!   server and the session's CSRF token in `X-Jadpo-CSRF`, or it is 403
//!   `csrf_rejected`.
//! - `sessions.start(user_id)` keeps a new session in the action's own
//!   transaction, and the route's response sets its cookies once the
//!   action commits; `sessions.end()` deletes the request's session and
//!   the response clears them.
//!
//! Only verifiers are kept: a session's token and its CSRF token are 32
//! random bytes each, sent once in cookies, and stored as their BLAKE3
//! digests, so a copy of the database signs no one in. API tokens and
//! service keys come with chunk 4.3b.

use jadpo_std::AuthenticationMethod;
use jadpo_std::http::{
    CSRF_HEADER, IdentityPlan, PrincipalKind, PrincipalPlan, RoutePlan, RouteTable,
    SESSION_LIFETIME_MS,
};
use jadpo_std::read::SqlType;
use jadpo_value::Value;

use super::{Request, Response};
use crate::ctx::{Caller, SessionEffect};
use crate::read::Interrupt;
use crate::{Ctx, Fault, FaultKind, Isolation, SqlValue, StoreError, Stored};

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
pub(crate) const NOT_PERMITTED: (u16, &str, &str) = (403, "not_permitted", "Not permitted.");
pub(crate) const AUTHENTICATION_UNAVAILABLE: (u16, &str, &str) = (
    503,
    "authentication_unavailable",
    "Signing in is temporarily unavailable.",
);

/// Keeps a new session.
pub(crate) const START: &str = "INSERT INTO \"jadpo_sessions\" (\"id\", \"principal_id\", \"verifier\", \"csrf_verifier\", \"started_at\", \"ends_at\") VALUES ($1, $2, $3, $4, $5, $6)";

/// Ends one.
pub(crate) const END: &str = "DELETE FROM \"jadpo_sessions\" WHERE \"id\" = $1";

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
    // A bearer token: API tokens and service keys come with chunk 4.3b, so
    // no API accepts one yet.
    let Some(token) = found.cookies.first().copied() else {
        return Err(Refused::new(INVALID_CREDENTIALS));
    };
    let Some(identity) = &table.identity else {
        return Err(Refused::new(INVALID_CREDENTIALS));
    };
    let Some(user) = &identity.user else {
        return Err(Refused::new(INVALID_CREDENTIALS));
    };
    let invalid = Refused {
        clear: true,
        ..Refused::new(INVALID_CREDENTIALS)
    };
    if !is_token(token) {
        return Err(invalid);
    }
    let Some(database) = ctx.host().database(&identity.store) else {
        return Err(Refused::new(AUTHENTICATION_UNAVAILABLE));
    };
    let Ok(mut transaction) = database.begin_at(Isolation::ReadCommitted).await else {
        return Err(Refused::new(AUTHENTICATION_UNAVAILABLE));
    };
    let rows = transaction
        .query(
            &user.session,
            &[
                SqlValue::Text(verifier(token)),
                SqlValue::Timestamp(ctx.now().saturating_mul(1000)),
            ],
        )
        .await;
    let _ = transaction.commit().await;
    let rows = rows.map_err(|_| Refused::new(AUTHENTICATION_UNAVAILABLE))?;
    let Some(
        [
            SqlValue::Uuid(session),
            csrf,
            id,
            subject,
            SqlValue::Bool(active),
        ],
    ) = rows.first().map(Vec::as_slice)
    else {
        return Err(invalid);
    };
    if !*active {
        return Err(Refused::new(USER_DISABLED));
    }
    if route.method != jadpo_std::HttpMethod::Get && !csrf_holds(request, csrf) {
        return Err(Refused::new(CSRF_REJECTED));
    }
    if route.caller == Some(PrincipalKind::Service) {
        return Err(Refused::new(NOT_PERMITTED));
    }
    let id = crate::from_sql(id, &jadpo_std::read::Column::Uuid)
        .map_err(|_| Refused::new(AUTHENTICATION_UNAVAILABLE))?;
    let subject = crate::from_sql(subject, &user.column)
        .map_err(|_| Refused::new(AUTHENTICATION_UNAVAILABLE))?;
    let key = match &id {
        Value::Uuid(_) | Value::Text(_) => format!("user {id}"),
        other => format!("user {other}"),
    };
    Ok(Caller {
        value: principal(PrincipalKind::User, id, subject),
        key,
        session: Some(*session),
    })
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

/// The headers a route test's request signs in with as `caller` (area 13,
/// `as:`): a real session, kept in its own transaction, sent as a browser
/// sends it, with the CSRF token and an `Origin` naming the server.
pub async fn test_credentials(
    ctx: &mut Ctx<'_>,
    table: &RouteTable,
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
    let (Some(cookie), Some(csrf_cookie)) = (plan.cookie.clone(), plan.csrf_cookie()) else {
        return Err(defect(
            ctx,
            "a request signed in with no browser session",
            site,
        ));
    };
    let user = match caller {
        Value::Record(record) => record.get("id").cloned(),
        _ => None,
    };
    let Some(user) = user else {
        return Err(defect(
            ctx,
            "a request signed in as a value that isn't a caller",
            site,
        ));
    };
    let (Some(token), Some(csrf), Some(id)) = (new_token(ctx), new_token(ctx), ctx.new_id()) else {
        return Err(defect(ctx, "a request signed in with no randomness", site));
    };
    let Ok(user) = crate::to_sql(&user, SqlType::Uuid) else {
        return Err(defect(ctx, "a caller whose id isn't one", site));
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
    let unavailable = |ctx: &mut Ctx<'_>| {
        ctx.fault(
            FaultKind::Store("the store sessions are kept in couldn't be reached".into()),
            0,
        )
    };
    let Some(database) = ctx.host().database(&plan.store) else {
        return Err(unavailable(ctx));
    };
    let Ok(mut transaction) = database.begin_at(Isolation::ReadCommitted).await else {
        return Err(unavailable(ctx));
    };
    if transaction.execute(START, &params).await.is_err() {
        let _ = transaction.rollback().await;
        return Err(unavailable(ctx));
    }
    if transaction.commit().await.is_err() {
        return Err(unavailable(ctx));
    }
    Ok(vec![
        (
            "cookie".to_string(),
            format!("{cookie}={token}; {csrf_cookie}={csrf}"),
        ),
        (CSRF_HEADER.to_string(), csrf),
        ("host".to_string(), TEST_HOST.to_string()),
        ("origin".to_string(), format!("http://{TEST_HOST}")),
    ])
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
            cookie: Some("todo_session".into()),
            user: None,
            service: None,
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
