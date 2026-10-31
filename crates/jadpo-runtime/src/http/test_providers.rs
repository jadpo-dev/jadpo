//! `test_providers` (area 8, "Providers"; chunk A7b): signs a route test
//! in through a sign-in provider's two routes for real, with the test
//! run's local issuer as the provider. Each request goes through the HTTP
//! component as a browser's would: the start from the application's own
//! page, the callback as the provider's redirect brings the browser back,
//! with the binding cookie the start set.
//!
//! - **`sign_in(method, account)`** starts the sign-in, has the local
//!   issuer sign `account` in for the address the start answered with,
//!   and gives the callback's `Response`, whose `code` is the `error` its
//!   redirect names.
//!
//! A refusal of the start, which the test meant to get past, ends the
//! test as a fault naming the route and its failure's code.

use jadpo_std::http::{ProviderStep, Run, TEST_ACCOUNT};
use jadpo_value::Value;

use super::local_issuer::Account;
use super::{App, Request, Response, handle, request_id, response_value};
use crate::{Ctx, Executor, FaultKind, Stored};

/// `test_providers.sign_in(method, account)`, called at site `at`.
pub async fn sign_in(
    app: &App<'_>,
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    args: &[Value],
    at: u32,
) -> Stored {
    let (Some(name), Some(account)) = (
        args.first().and_then(variant_name),
        args.get(1).and_then(account),
    ) else {
        return defect(
            ctx,
            "`test_providers.sign_in` given the wrong arguments",
            at,
        );
    };
    let Some(plan) = app.table.identity.as_ref() else {
        return defect(ctx, "`test_providers.sign_in` with nothing to sign in", at);
    };
    let provider = plan
        .providers
        .iter()
        .flat_map(|providers| providers.providers.iter().enumerate())
        .find(|(_, provider)| provider.name == name)
        .map(|(index, _)| index as u32);
    let Some(provider) = provider else {
        return defect(
            ctx,
            &format!("`test_providers.sign_in` for `{name}`, which isn't a sign-in provider"),
            at,
        );
    };
    let (Some(origin), Some(start), Some(callback), Some(cookie)) = (
        ctx.started().auth().origin().map(str::to_string),
        route(app, provider, ProviderStep::Start),
        route(app, provider, ProviderStep::Callback),
        super::provider_sign_in::binding_cookie(plan),
    ) else {
        return defect(ctx, "`test_providers.sign_in` with no provider routes", at);
    };
    if ctx.host().local_issuer().is_none() {
        return defect(
            ctx,
            "`test_providers.sign_in` outside a test: only a test has a local issuer",
            at,
        );
    }
    // The application's own page starts the sign-in.
    let page = vec![("origin".to_string(), origin)];
    let started = match send(app, executor, ctx, "POST", &start.path, None, page).await {
        Ok(response) => response,
        Err(stored) => return stored,
    };
    if started.status != 200 {
        let code = started.code.as_deref().unwrap_or("no code");
        return defect(
            ctx,
            &format!(
                "`{}` answered {} {code} on the way to the provider",
                start.label, started.status
            ),
            at,
        );
    }
    let address = serde_json::from_slice::<serde_json::Value>(&started.body)
        .ok()
        .and_then(|body| body.get("address")?.as_str().map(str::to_string));
    let query = address
        .as_deref()
        .and_then(|address| address.split_once('?'))
        .and_then(|(_, query)| super::query_values(query));
    let asked = |name: &str| {
        query
            .as_ref()
            .and_then(|query| query.get(name))
            .and_then(|values| values.first())
            .cloned()
    };
    let (Some(state), Some(nonce), Some(challenge), Some(redirect), Some(client)) = (
        asked("state"),
        asked("nonce"),
        asked("code_challenge"),
        asked("redirect_uri"),
        asked("client_id"),
    ) else {
        return defect(
            ctx,
            "the provider's address names no sign-in to answer",
            at,
        );
    };
    // The local issuer signs the account in, as the provider would.
    let now = ctx.now();
    let code = match ctx.host().local_issuer() {
        Some(issuer) => issuer.authorize(account, &client, &redirect, &nonce, &challenge, now),
        None => return defect(ctx, "the local issuer is gone", at),
    };
    // The provider sends the browser back, with the cookie the start set.
    let mut back = Vec::new();
    if let Some(binding) = set_cookie(&started, &cookie) {
        back.push(("cookie".to_string(), format!("{cookie}={binding}")));
    }
    let query = format!(
        "code={}&state={}",
        super::percent_encode(&code),
        super::percent_encode(&state)
    );
    match send(app, executor, ctx, "GET", &callback.path, Some(query), back).await {
        Ok(response) => Stored::Value(response_value(app, callback.index, &response, &[])),
        Err(stored) => stored,
    }
}

/// The variant's name a `SignInMethod` holds.
fn variant_name(value: &Value) -> Option<String> {
    match value {
        Value::Variant(variant) => Some(variant.name.to_string()),
        _ => None,
    }
}

/// What a `TestAccount` says about the person.
fn account(value: &Value) -> Option<Account> {
    let Value::Record(record) = value else {
        return None;
    };
    if *record.ty != *TEST_ACCOUNT {
        return None;
    }
    let text = |name: &str| match record.get(name) {
        Some(Value::Text(text)) => Some(text.to_string()),
        _ => None,
    };
    Some(Account {
        subject: text("subject")?,
        email: text("email"),
        email_verified: matches!(record.get("email_verified"), Some(Value::Bool(true))),
        name: text("name"),
    })
}

/// One of a provider's routes: its number in the table and its path.
struct Route {
    index: u32,
    path: String,
    label: String,
}

fn route(app: &App<'_>, provider: u32, step: ProviderStep) -> Option<Route> {
    let (index, plan) = app.table.routes.iter().enumerate().find(|(_, plan)| {
        matches!(plan.run, Run::Provider(at, found) if at == provider && found == step)
    })?;
    Some(Route {
        index: u32::try_from(index).ok()?,
        path: plan.path(),
        label: plan.label(),
    })
}

/// Sends `method` to `path` with `query` and `headers`, through the HTTP
/// component as a browser's request would go. The fault behind a 500
/// ends the test.
async fn send(
    app: &App<'_>,
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    method: &str,
    path: &str,
    query: Option<String>,
    headers: Vec<(String, String)>,
) -> Result<Response, Stored> {
    let request = Request {
        method: method.to_string(),
        path: path.to_string(),
        query,
        headers,
        address: Some("route test".to_string()),
        ..Request::default()
    };
    let id = request_id(ctx, u64::from(ctx.actions()));
    let mut response = handle(app, executor, ctx, &request, &id).await;
    match response.fault.take() {
        Some(fault) => Err(Stored::Fault(fault)),
        None => Ok(response),
    }
}

/// The value `response` sets cookie `name` to, if it sets it.
fn set_cookie(response: &Response, name: &str) -> Option<String> {
    response
        .headers
        .iter()
        .filter(|(header, _)| header.eq_ignore_ascii_case("set-cookie"))
        .filter_map(|(_, value)| {
            let pair = value.split(';').next()?;
            let (key, value) = pair.split_once('=')?;
            (key.trim() == name && !value.is_empty()).then(|| value.to_string())
        })
        .next()
}

fn defect(ctx: &mut Ctx<'_>, what: &str, at: u32) -> Stored {
    Stored::Fault(ctx.fault(FaultKind::Defect(what.to_string()), at))
}
