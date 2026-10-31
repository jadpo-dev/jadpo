//! Where rate limits are counted (area 16, "Rate limits"; chunk 8.2m2):
//! `application { rate_limits: { store: <connection> } }` names the
//! connection every route's rate limits are counted in, so the servers or
//! the Worker's instances share one count:
//!
//! ```text
//! connections: {
//!     limits: {
//!         driver: Driver.cloudflare_rate_limits
//!         namespace: 1001
//!     }
//! }
//! rate_limits: {
//!     store: limits
//! }
//! ```
//!
//! The store is a Redis connection (`Driver.redis`), or on Workers
//! Cloudflare's rate-limiting binding (`Driver.cloudflare_rate_limits`),
//! which counts one limit over 10 seconds or a minute a binding: each limit
//! the app counts there has a binding of its own, numbered from the
//! connection's `namespace:` ([`jadpo_std::http::rate_limit_bindings`]),
//! and every limit counted today is a minute's; a route's own
//! `rate_limit:` brings the error the page decides for any other window,
//! naming Redis. A deployable built for Workers that serves an
//! API needs a store, since a Worker keeps no memory its instances share
//! (`deployables::target_problems`).

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_std::http::{COUNTED_LIMITS, RateLimitStore, rate_limit_bindings};
use jadpo_syntax::ast::AstNode;

use crate::config::singletons;
use crate::data::ConnectionKind;
use crate::tree::{self, name_text, node_span};

/// What a connection with `driver: Driver.cloudflare_rate_limits` holds.
pub const BINDING_KEYS: [&str; 2] = ["driver", "namespace"];

/// What `rate_limits:` holds.
const OPTIONS: [&str; 1] = ["store"];

const BINDING_REASON: &str = "A connection with `driver: Driver.cloudflare_rate_limits` counts rate limits in Cloudflare's rate-limiting bindings, one binding for each limit the app counts, since a binding counts one limit over one window. `namespace:` is the first of the namespace ids in your Cloudflare account the bindings are numbered from, a whole number of at least 1; they are written into the Wrangler file when the app is built.";

const STORE_REASON: &str = "Rate limits are counted in one store the servers or a Worker's instances share: a Redis connection (`Driver.redis`), or on Workers Cloudflare's rate-limiting binding (`Driver.cloudflare_rate_limits`). `rate_limits: { store: <connection> }` names it.";

/// A connection to Cloudflare's rate-limiting binding.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BindingConnection {
    pub name: String,
    /// `namespace:`, the first namespace id its bindings take.
    pub namespace: u32,
}

/// Each connection to Cloudflare's rate-limiting binding, and the problems
/// with them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BindingConnections {
    pub list: Vec<BindingConnection>,
    pub problems: Vec<Diagnostic>,
}

/// Each connection with `driver: Driver.cloudflare_rate_limits`, read.
#[salsa::tracked(returns(ref))]
pub fn binding_connections(db: &dyn Db, project: Project) -> BindingConnections {
    let mut out = BindingConnections::default();
    for found in singletons(db, project, "application") {
        let path = found.path.clone();
        for (key, member) in tree::members(found.list.clone()) {
            if name_text(&key).as_deref() != Some("connections") {
                continue;
            }
            for (name, entry) in tree::members(member.block()) {
                let Some(connection) = name_text(&name) else {
                    continue;
                };
                if crate::data::connection_kind(entry.block())
                    != ConnectionKind::CloudflareRateLimits
                {
                    continue;
                }
                let mut namespace = None;
                let mut written = false;
                for (setting, option) in tree::members(entry.block()) {
                    let word = name_text(&setting).unwrap_or_default();
                    match word.as_str() {
                        "driver" => {}
                        "namespace" => {
                            written = true;
                            let read = crate::connections::whole(&option).filter(|first| {
                                rate_limit_bindings(&connection, *first, &COUNTED_LIMITS).is_some()
                            });
                            if read.is_none() {
                                let span = option.value().map_or_else(
                                    || node_span(&path, option.syntax()),
                                    |value| node_span(&path, value.syntax()),
                                );
                                out.problems.push(
                                    Diagnostic::new(
                                        &rule::CONNECTION_VALUE_INVALID,
                                        format!(
                                            "`{connection}`'s `namespace:` must be a whole number of at least 1"
                                        ),
                                    )
                                    .at(span, "not a namespace id")
                                    .reason(BINDING_REASON)
                                    .next("Write a namespace id no other rate limit in your Cloudflare account uses, such as `namespace: 1001`"),
                                );
                            }
                            namespace = read;
                        }
                        other => {
                            let near = nearest(other, BINDING_KEYS.iter().copied());
                            let problem = Diagnostic::new(
                                &rule::CONNECTION_KEY_INVALID,
                                format!(
                                    "A connection with `driver: Driver.cloudflare_rate_limits` takes no `{other}:`"
                                ),
                            )
                            .at(node_span(&path, setting.syntax()), "not a rate-limit setting")
                            .reason(BINDING_REASON);
                            out.problems.push(match near {
                                Some(near) => {
                                    problem.next(format!("Use `{near}:` instead of `{other}:`"))
                                }
                                None => problem.next(format!("Remove `{other}:`")),
                            });
                        }
                    }
                }
                if !written {
                    out.problems.push(
                        Diagnostic::new(
                            &rule::CONNECTION_KEY_MISSING,
                            format!("`{connection}` doesn't give `namespace:`"),
                        )
                        .at(node_span(&path, name.syntax()), "no `namespace:`")
                        .reason(BINDING_REASON)
                        .next("Add `namespace: 1001`, a namespace id no other rate limit in your Cloudflare account uses"),
                    );
                }
                if let Some(namespace) = namespace {
                    out.list.push(BindingConnection {
                        name: connection,
                        namespace,
                    });
                }
            }
        }
    }
    out
}

/// The project's `rate_limits:`, read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RateLimitsRead {
    /// The connection `store:` names, and where, once it is one that can
    /// count them.
    pub store: Option<(String, Span)>,
    /// Whether `rate_limits:` is written at all: one written wrongly is
    /// reported for what is wrong with it, not as missing.
    pub written: bool,
    pub problems: Vec<Diagnostic>,
}

/// `application { rate_limits: { store: } }`.
#[salsa::tracked(returns(ref))]
pub fn rate_limits(db: &dyn Db, project: Project) -> RateLimitsRead {
    let mut out = RateLimitsRead::default();
    let connections = crate::data::connections(db, project);
    let mut first: Option<Span> = None;
    for found in singletons(db, project, "application") {
        let path = &found.path;
        for (key, member) in tree::members(found.list.clone()) {
            if name_text(&key).as_deref() != Some("rate_limits") {
                continue;
            }
            let at = node_span(path, key.syntax());
            out.written = true;
            if let Some(first) = &first {
                out.problems.push(
                    invalid("`rate_limits:` is declared twice", at, "declared again")
                        .also(first.clone(), "first declared here")
                        .next("Keep one `rate_limits:` and remove the other"),
                );
                continue;
            }
            first = Some(at.clone());
            let Some(block) = member.block() else {
                out.problems.push(
                    invalid("`rate_limits:` holds no block", at, "not a block")
                        .next("Write `rate_limits: { store: <connection> }`"),
                );
                continue;
            };
            let mut given = false;
            let mut shaped = true;
            for (setting, entry) in tree::members(Some(block)) {
                let word = name_text(&setting).unwrap_or_default();
                let setting_span = node_span(path, setting.syntax());
                if word != "store" {
                    shaped = false;
                    let near = nearest(&word, OPTIONS);
                    let problem = invalid(
                        format!("`{word}` isn't a rate-limits setting"),
                        setting_span.clone(),
                        "not a setting",
                    );
                    out.problems.push(match near {
                        Some(near) => problem
                            .next(format!("Use `{near}` instead of `{word}`"))
                            .edit(setting_span, near),
                        None => problem.next(format!("Remove `{word}:`")),
                    });
                    continue;
                }
                given = true;
                let written = entry
                    .value()
                    .map(|value| tree::written(value.syntax()))
                    .unwrap_or_default();
                let span = entry.value().map_or_else(
                    || node_span(path, entry.syntax()),
                    |value| node_span(path, value.syntax()),
                );
                match connections.kind(&written) {
                    Some(ConnectionKind::Redis | ConnectionKind::CloudflareRateLimits) => {
                        out.store = Some((written, span));
                    }
                    Some(kind) => out.problems.push(
                        invalid(
                            format!("`store: {written}` can't count rate limits"),
                            span,
                            "not Redis or a rate-limiting binding",
                        )
                        .reason(format!(
                            "{STORE_REASON} `{written}` is {}.",
                            described(kind)
                        ))
                        .next("Name a connection with `driver: Driver.redis` or `driver: Driver.cloudflare_rate_limits`"),
                    ),
                    None => {
                        let able: Vec<&str> = connections
                            .declared
                            .iter()
                            .filter(|(_, kind)| {
                                matches!(
                                    kind,
                                    ConnectionKind::Redis | ConnectionKind::CloudflareRateLimits
                                )
                            })
                            .map(|(name, _)| name.as_str())
                            .collect();
                        let problem = invalid(
                            format!("`store: {written}` isn't a connection"),
                            span.clone(),
                            "no such connection",
                        );
                        out.problems.push(match nearest(&written, able.iter().copied()) {
                            Some(near) => problem
                                .next(format!("Use `{near}` instead of `{written}`"))
                                .edit(span, near),
                            None => problem.next(format!(
                                "Declare `{written}` in `connections:` with `driver: Driver.cloudflare_rate_limits` and a `namespace:`, or with `driver: Driver.redis`"
                            )),
                        });
                    }
                }
            }
            if !given && shaped {
                out.problems.push(
                    invalid("`rate_limits:` names no `store:`", at, "no `store:`")
                        .next("Add `store: <connection>`, naming a Redis connection or Cloudflare's rate-limiting binding"),
                );
            }
        }
    }
    out
}

/// The problems in the project's `rate_limits:`.
pub fn rate_limits_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let mut out = binding_connections(db, project).problems.clone();
    out.extend(rate_limits(db, project).problems.iter().cloned());
    out
}

/// The store the plan counts in: its connection, and on Cloudflare's
/// binding the binding each limit the app counts takes.
pub fn store_plan(db: &dyn Db, project: Project) -> Option<RateLimitStore> {
    let (store, _) = rate_limits(db, project).store.as_ref()?;
    let bindings = match crate::data::connections(db, project).kind(store)? {
        ConnectionKind::CloudflareRateLimits => {
            let connection = binding_connections(db, project)
                .list
                .iter()
                .find(|connection| &connection.name == store)?;
            rate_limit_bindings(store, connection.namespace, &COUNTED_LIMITS)?
        }
        _ => Vec::new(),
    };
    Some(RateLimitStore {
        connection: store.clone(),
        bindings,
    })
}

/// What a connection that can't count rate limits is, for a reason.
fn described(kind: ConnectionKind) -> &'static str {
    match kind {
        ConnectionKind::Postgres => "a PostgreSQL database",
        ConnectionKind::Files => "a store of files",
        ConnectionKind::Smtp => "an SMTP server",
        ConnectionKind::Model => "a model server",
        ConnectionKind::CloudflareQueue => "a Cloudflare queue",
        ConnectionKind::SignIn => "a sign-in provider",
        ConnectionKind::Package => "a package's connection",
        ConnectionKind::Redis | ConnectionKind::CloudflareRateLimits | ConnectionKind::Other => {
            "another kind of connection"
        }
    }
}

fn invalid(message: impl Into<String>, at: Span, label: &str) -> Diagnostic {
    Diagnostic::new(&rule::RATE_LIMITS_STORE_INVALID, message)
        .at(at, label)
        .reason(STORE_REASON)
}
