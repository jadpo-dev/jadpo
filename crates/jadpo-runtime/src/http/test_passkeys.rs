//! `test_passkeys` (area 8, "In development and tests"; chunk 5.5z2): a
//! software authenticator driving the generated passkey routes for real,
//! which only a route test calls. Each request goes through the HTTP
//! component as a browser's would, from the application's own origin; the
//! device is the test host's ([`crate::TestDevices`]), so a `TestPasskey`
//! exists nowhere but a test.
//!
//! - **`register(user, name)`** signs in as `as:` signs in, asks for the
//!   registration options, has a new device answer them and posts its
//!   answer with `name`, giving the `TestPasskey` that holds the passkey.
//! - **`sign_in(passkey)`** asks for the sign-in options from a browser of
//!   its own, has the device answer them and posts the answer with the
//!   browser's binding cookie, giving the second route's `Response`.
//!
//! A refusal on the way to a passkey, which the test meant to make, ends
//! the test as a fault naming the route and its failure's code.

use std::rc::Rc;

use jadpo_std::http::{PasskeyStep, Run, TEST_PASSKEY};
use jadpo_value::Value;

use super::{App, Request, Response, handle, identity, request_id, response_value};
use crate::{Ctx, Executor, FaultKind, Stored};

/// `test_passkeys.register(user, name)`, called at site `at`.
pub async fn register(
    app: &App<'_>,
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    args: &[Value],
    at: u32,
) -> Stored {
    let [user, Value::Text(name)] = args else {
        return defect(
            ctx,
            "`test_passkeys.register` given the wrong arguments",
            at,
        );
    };
    let (Some(origin), Some(options), Some(finish)) = (
        app.auth.origin().map(str::to_string),
        route(app, PasskeyStep::RegisterOptions),
        route(app, PasskeyStep::Register),
    ) else {
        return defect(ctx, "`test_passkeys.register` with no passkey routes", at);
    };
    let Some(devices) = ctx.host().test_devices() else {
        return defect(
            ctx,
            "`test_passkeys.register` outside a test: only a test has a software authenticator",
            at,
        );
    };
    // Signed in as `as:` signs in: a browser session of the user's own.
    let headers =
        match identity::test_credentials(ctx, app.table, app.auth, options.index, user).await {
            Ok(headers) => headers,
            Err(fault) => return Stored::Fault(fault),
        };
    let asked = match send(app, executor, ctx, &options, headers.clone(), None).await {
        Ok(response) => response,
        Err(stored) => return stored,
    };
    if asked.status != 200 {
        return refused(ctx, &options, &asked, at);
    }
    let (device, credential) = match devices.create(&origin, &body_text(&asked)) {
        Ok(made) => made,
        Err(why) => {
            return defect(
                ctx,
                &format!("the test's device couldn't make a passkey: {why}"),
                at,
            );
        }
    };
    let mut body = String::from("{\"name\":");
    jadpo_value::json::write_string(name, &mut body);
    body.push_str(",\"credential\":");
    body.push_str(&credential);
    body.push('}');
    let kept = match send(app, executor, ctx, &finish, headers, Some(body)).await {
        Ok(response) => response,
        Err(stored) => return stored,
    };
    if kept.status != 201 {
        return refused(ctx, &finish, &kept, at);
    }
    Stored::Value(Value::record(
        TEST_PASSKEY,
        vec![(Rc::from("device"), Value::Int(i64::from(device)))],
    ))
}

/// `test_passkeys.sign_in(passkey)`, called at site `at`.
pub async fn sign_in(
    app: &App<'_>,
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    args: &[Value],
    at: u32,
) -> Stored {
    let device = match args {
        [Value::Record(record)] => match record.get("device") {
            Some(Value::Int(device)) => u32::try_from(*device).ok(),
            _ => None,
        },
        _ => None,
    };
    let Some(device) = device else {
        return defect(ctx, "`test_passkeys.sign_in` given no `TestPasskey`", at);
    };
    let (Some(origin), Some(options), Some(finish), Some(cookie)) = (
        app.auth.origin().map(str::to_string),
        route(app, PasskeyStep::SignInOptions),
        route(app, PasskeyStep::SignIn),
        app.table
            .identity
            .as_ref()
            .and_then(|plan| plan.cookie.as_ref())
            .map(|cookie| format!("{cookie}_passkey")),
    ) else {
        return defect(ctx, "`test_passkeys.sign_in` with no passkey routes", at);
    };
    let Some(devices) = ctx.host().test_devices() else {
        return defect(
            ctx,
            "`test_passkeys.sign_in` outside a test: only a test has a software authenticator",
            at,
        );
    };
    // A browser of its own, on the application's own page.
    let page = vec![("origin".to_string(), origin)];
    let asked = match send(app, executor, ctx, &options, page.clone(), None).await {
        Ok(response) => response,
        Err(stored) => return stored,
    };
    if asked.status != 200 {
        return refused(ctx, &options, &asked, at);
    }
    let answer = match devices.get(device, &body_text(&asked)) {
        Ok(answer) => answer,
        Err(why) => {
            return defect(
                ctx,
                &format!("the test's device couldn't answer: {why}"),
                at,
            );
        }
    };
    let mut headers = page;
    if let Some(binding) = set_cookie(&asked, &cookie) {
        headers.push(("cookie".to_string(), format!("{cookie}={binding}")));
    }
    let body = format!("{{\"credential\":{answer}}}");
    match send(app, executor, ctx, &finish, headers, Some(body)).await {
        Ok(response) => Stored::Value(response_value(app, finish.index, &response)),
        Err(stored) => stored,
    }
}

/// One generated passkey route: its number in the table and its path.
struct Route {
    index: u32,
    path: String,
    label: String,
}

/// The generated route answering `step`.
fn route(app: &App<'_>, step: PasskeyStep) -> Option<Route> {
    let (index, plan) = app
        .table
        .routes
        .iter()
        .enumerate()
        .find(|(_, plan)| matches!(plan.run, Run::Passkey(found) if found == step))?;
    Some(Route {
        index: u32::try_from(index).ok()?,
        path: plan.path(),
        label: plan.label(),
    })
}

/// `POST`s `body` to `route` with `headers`, through the HTTP component as
/// a browser's request would go. The fault behind a 500 ends the test.
async fn send(
    app: &App<'_>,
    executor: &dyn Executor,
    ctx: &mut Ctx<'_>,
    route: &Route,
    mut headers: Vec<(String, String)>,
    body: Option<String>,
) -> Result<Response, Stored> {
    if body.is_some() {
        headers.push(("content-type".to_string(), "application/json".to_string()));
    }
    let request = Request {
        method: "POST".to_string(),
        path: route.path.clone(),
        headers,
        body: body.unwrap_or_default().into_bytes(),
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

fn body_text(response: &Response) -> String {
    String::from_utf8_lossy(&response.body).into_owned()
}

/// A route that refused the way to a passkey: the test meant to make one,
/// so it ends there, naming the route and its failure's code.
fn refused(ctx: &mut Ctx<'_>, route: &Route, response: &Response, at: u32) -> Stored {
    let code = response.code.as_deref().unwrap_or("no code");
    defect(
        ctx,
        &format!(
            "`{}` answered {} {code} on the way to a test passkey",
            route.label, response.status
        ),
        at,
    )
}

fn defect(ctx: &mut Ctx<'_>, what: &str, at: u32) -> Stored {
    Stored::Fault(ctx.fault(FaultKind::Defect(what.to_string()), at))
}
