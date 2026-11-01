//! The routes the runtime answers itself in each API (areas 7, 8 and 11):
//! the services' token exchange, the passkey and magic-link routes, each
//! confirmed route's `…/confirmation`, the API's own OpenAPI document and
//! each connection's webhook. The API
//! record lists them (`generated ...`, [`super::record`]) and the graph
//! has a `generated_route` node for each (chunk 10.3g), both from this one
//! list, so they never disagree. Health is outside every API, so it is
//! the graph's alone.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, Project};
use jadpo_std::AuthenticationMethod;
use jadpo_std::http::{MagicLinkStep, PasskeyStep, ProviderStep};

use super::served::Served;
use super::{RouteModel, Segment};
use crate::webhooks::WebhookModel;

/// What generates a route the runtime answers in an API.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Generating {
    /// `POST /auth/exchange`, where a service trades its key for a token
    /// (area 8, "Ways to sign in").
    Exchange,
    /// The passkey routes under `path:` (area 8, "Passkeys").
    Passkey(Vec<String>),
    /// The magic-link routes under `path:` (area 8, "Magic links"), the
    /// redemption with them when API tokens are declared.
    MagicLink { path: Vec<String>, api_tokens: bool },
    /// A sign-in provider's two routes under `/auth/<name>` (area 8,
    /// "Providers"; chunk A7b), by its connection's name.
    Provider(String),
    /// `GET <prefix>/openapi.json` (area 7, "OpenAPI").
    OpenApi(Served),
    /// A connection's webhook (area 11, "Receiving from outside").
    Webhook(Box<WebhookModel>),
    /// `<path>/confirmation` for a route with `confirm:
    /// Confirmation.passkey`, asked with its method (area 8, "Confirm one
    /// request with a passkey"), by the route it confirms.
    Confirmation(Box<RouteModel>),
}

/// One route the runtime answers, as the graph keys it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedRoute {
    /// `POST`.
    pub method: jadpo_std::HttpMethod,
    /// Its path, a placeholder in braces: `/auth/passkey/{passkey_id}`.
    pub path: String,
    /// What generates it: `exchange`, `passkey`, `magic_link`,
    /// `provider`, `openapi`, `webhook` or `confirmation`.
    pub generated: &'static str,
    /// Who may call it: `none`, `caller`, `service`, `user` or `signature`.
    pub authentication: &'static str,
}

impl GeneratedRoute {
    /// `POST /auth/exchange`.
    pub fn label(&self) -> String {
        format!("{} {}", self.method.word(), self.path)
    }
}

fn joined(segments: &[String]) -> String {
    segments
        .iter()
        .map(|segment| format!("/{segment}"))
        .collect()
}

impl Generating {
    /// The routes it generates.
    pub fn routes(&self) -> Vec<GeneratedRoute> {
        use jadpo_std::HttpMethod;
        match self {
            Generating::Exchange => vec![GeneratedRoute {
                method: HttpMethod::Post,
                path: format!("/{}", jadpo_std::http::EXCHANGE_PATH.join("/")),
                generated: "exchange",
                authentication: "service",
            }],
            Generating::Passkey(path) => PasskeyStep::ALL
                .into_iter()
                .map(|step| GeneratedRoute {
                    method: step.method(),
                    path: joined(&step.segments(path)),
                    generated: "passkey",
                    authentication: if step.authentication().needs_caller() {
                        "user"
                    } else {
                        "none"
                    },
                })
                .collect(),
            Generating::MagicLink { path, api_tokens } => MagicLinkStep::ALL
                .into_iter()
                .filter(|step| *api_tokens || *step != MagicLinkStep::Redeem)
                .map(|step| GeneratedRoute {
                    method: step.method(),
                    path: joined(&step.segments(path)),
                    generated: "magic_link",
                    authentication: "none",
                })
                .collect(),
            Generating::Provider(name) => ProviderStep::ALL
                .into_iter()
                .map(|step| GeneratedRoute {
                    method: step.method(),
                    path: joined(&step.segments(name)),
                    generated: "provider",
                    authentication: "none",
                })
                .collect(),
            Generating::OpenApi(served) => vec![GeneratedRoute {
                method: HttpMethod::Get,
                path: served.path(),
                generated: "openapi",
                authentication: if served.authentication.needs_caller() {
                    "caller"
                } else {
                    "none"
                },
            }],
            Generating::Webhook(webhook) => vec![GeneratedRoute {
                method: HttpMethod::Post,
                path: webhook.path(),
                generated: "webhook",
                authentication: "signature",
            }],
            Generating::Confirmation(route) => vec![GeneratedRoute {
                method: jadpo_std::http::confirmation_method(route.method),
                path: confirmation_path(route),
                generated: "confirmation",
                authentication: "user",
            }],
        }
    }
}

/// A confirmed route's `…/confirmation`, its placeholders in braces:
/// `/releases/{release_id}/confirm/confirmation`.
pub fn confirmation_path(route: &RouteModel) -> String {
    let segments = route
        .confirmation_segments()
        .unwrap_or_else(|| route.segments.clone());
    segments
        .iter()
        .map(|segment| match segment {
            Segment::Fixed(text) => format!("/{text}"),
            Segment::Param { name, .. } => format!("/{{{name}}}"),
        })
        .collect()
}

/// What generates routes in each API that has a record, by API name: an
/// API with routes of its own, the first that accepts browser sessions
/// while passkeys or magic links are declared, and the `webhooks` API
/// while a connection receives webhooks.
pub fn generating(db: &dyn Db, project: Project) -> BTreeMap<String, Vec<Generating>> {
    let groups = super::groups(db, project);
    let identity = crate::identity::identity(db, project);
    let mut apis: BTreeSet<String> = groups
        .routes()
        .map(|(group, _)| group.api.clone())
        .collect();
    // The passkey and magic-link routes belong to the first API that
    // accepts browser sessions (area 8), routes of its own or not.
    let sign_in = super::served::sign_in_api(db, project);
    apis.extend(sign_in.iter().cloned());
    // Each connection's webhook, in the `webhooks` API the app declares
    // (chunk 7.7).
    if !crate::webhooks::declared_in(db, project, crate::webhooks::DEFAULT_PREFIX).is_empty() {
        apis.insert(crate::webhooks::DEFAULT_PREFIX.to_string());
    }
    apis.into_iter()
        .map(|api| {
            let methods = super::apis::accepted_methods(db, project, &api);
            let mut out = Vec::new();
            // The exchange the runtime answers where services sign in.
            if identity.exchanges() && methods.contains(&AuthenticationMethod::ServiceKey) {
                out.push(Generating::Exchange);
            }
            let signs_in_here = sign_in.as_deref() == Some(api.as_str());
            if let Some(passkey) = identity.passkeys()
                && signs_in_here
            {
                out.push(Generating::Passkey(passkey.path.clone()));
            }
            if let Some(magic) = identity.magic_link_routes()
                && signs_in_here
            {
                out.push(Generating::MagicLink {
                    path: magic.path.clone(),
                    api_tokens: identity.built(AuthenticationMethod::ApiToken),
                });
            }
            if signs_in_here {
                for provider in &identity.providers {
                    out.push(Generating::Provider(provider.name.clone()));
                }
            }
            if let Some(served) = super::served::served_by(db, project, &api) {
                out.push(Generating::OpenApi(served));
            }
            for webhook in crate::webhooks::declared_in(db, project, &api) {
                out.push(Generating::Webhook(Box::new(webhook)));
            }
            // Each confirmed route's options (area 8).
            for (_, route) in groups.routes().filter(|(group, _)| group.api == api) {
                if route.confirm {
                    out.push(Generating::Confirmation(Box::new(route.clone())));
                }
            }
            (api, out)
        })
        .collect()
}
