//! `rate_limit: { requests: within: by: }` on a route or an API (area 16,
//! "Rate limits"; chunk G4), read: how many requests, over how long, and
//! what they are counted by. A route's or an API's own limit stands in for
//! the default limits, on its route or on the API's routes that write none.

use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_std::http::{
    ADDRESS_LIMIT, Authentication, CALLER_LIMIT, LONGEST_LIMIT_WINDOW_MS, MagicLinkStep,
    PRIVACY_CHECK_RESEND_AFTER_MS, PRIVACY_CHECKS_PER_HOUR, PrivacyStep, RateLimit, RouteRateLimit,
    SHORTEST_LIMIT_WINDOW_MS,
};
use jadpo_std::{Dur, RateLimitKey};
use jadpo_syntax::ast::{self, AstNode};

use super::{Apis, GroupModel, RouteModel};
use crate::identity::MagicLink;
use crate::tree::{self, name_text, node_span};

/// What a `rate_limit:` holds.
const ITEMS: [&str; 3] = ["requests", "within", "by"];

const REASON: &str = "A rate limit says how many requests (`requests:`, a whole number of at least 1) over how long (`within:`, whole seconds from `1s` to `24h`), and what it counts them by (`by:`, `RateLimitKey.caller`, the default, or `RateLimitKey.ip`). A longer allowance is a plan's limit, which is the application's data.";

/// A limit as `rate_limit:` writes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Limit {
    pub limit: RateLimit,
    pub by: RateLimitKey,
}

/// The limit `member` writes and where its `within:` is, or `None` with
/// the problems it has in `out`.
pub fn read(path: &str, member: &ast::Member, out: &mut Vec<Diagnostic>) -> Option<(Limit, Span)> {
    let at = |syntax: &jadpo_syntax::SyntaxNode| node_span(path, syntax);
    let Some(block) = member.block() else {
        out.push(
            invalid(
                "`rate_limit:` holds no block",
                member
                    .value()
                    .map_or_else(|| at(member.syntax()), |value| at(value.syntax())),
                "not a block",
            )
            .next("Write `rate_limit: { requests: 100 within: 1m }`"),
        );
        return None;
    };
    let mut requests = None;
    let mut within = None;
    let mut by = RateLimitKey::Caller;
    let (mut seen, mut wrong): (Vec<String>, bool) = (Vec::new(), false);
    for (key, entry) in tree::members(Some(block)) {
        let word = name_text(&key).unwrap_or_default();
        let key_at = at(key.syntax());
        if !ITEMS.contains(&word.as_str()) {
            wrong = true;
            let problem = invalid(
                format!("`{word}` isn't part of a rate limit"),
                key_at.clone(),
                "not part of a limit",
            );
            out.push(match nearest(&word, ITEMS.iter().copied()) {
                Some(near) => problem
                    .next(format!("Use `{near}` instead of `{word}`"))
                    .edit(key_at, near),
                None => problem.next(format!("Remove `{word}:`")),
            });
            continue;
        }
        if seen.contains(&word) {
            wrong = true;
            out.push(
                invalid(
                    format!("`{word}:` is written twice"),
                    key_at,
                    "written again",
                )
                .next(format!("Keep one `{word}:` and remove the other")),
            );
            continue;
        }
        seen.push(word.clone());
        let value = entry.value();
        let written = value
            .as_ref()
            .map(|value| tree::written(value.syntax()))
            .unwrap_or_default();
        let value_at = value
            .as_ref()
            .map_or_else(|| at(entry.syntax()), |value| at(value.syntax()));
        match word.as_str() {
            "requests" => match written.parse::<u32>() {
                Ok(count) if count >= 1 => requests = Some(count),
                _ => {
                    wrong = true;
                    out.push(
                        invalid(
                            format!("`{written}` can't be a limit's `requests:`"),
                            value_at,
                            "not a whole number of at least 1",
                        )
                        .next("Write a whole number of requests, such as `requests: 100`"),
                    );
                }
            },
            "within" => match window(&written) {
                Some(window_ms) => within = Some((window_ms, value_at)),
                None => {
                    wrong = true;
                    out.push(
                        invalid(
                            format!("`{written}` can't be a limit's `within:`"),
                            value_at,
                            "not whole seconds from 1s to 24h",
                        )
                        .next("Write a duration such as `within: 1m`"),
                    );
                }
            },
            _ => match value.as_ref().and_then(key_written) {
                Some(key) => by = key,
                None => {
                    wrong = true;

                    out.push(
                        invalid(
                            format!("`{written}` isn't a `RateLimitKey`"),
                            value_at,
                            "not a `RateLimitKey`",
                        )
                        .next("Write `by: RateLimitKey.ip` or `by: RateLimitKey.caller`"),
                    );
                }
            },
        }
    }
    if wrong {
        return None;
    }
    let block_at = at(member.syntax());
    let Some(requests) = requests else {
        out.push(
            invalid(
                "`rate_limit:` says no `requests:`",
                block_at,
                "no `requests:`",
            )
            .next("Add `requests: 100`, how many requests it allows"),
        );
        return None;
    };
    let Some((window_ms, within_at)) = within else {
        out.push(
            invalid("`rate_limit:` says no `within:`", block_at, "no `within:`")
                .next("Add `within: 1m`, the window it counts them over"),
        );
        return None;
    };
    Some((
        Limit {
            limit: RateLimit {
                requests,
                window_ms,
            },
            by,
        },
        within_at,
    ))
}

/// The window `written` says, in whole seconds from a second to a day.
fn window(written: &str) -> Option<u32> {
    let millis = u32::try_from(Dur::parse(written)?.millis).ok()?;
    ((SHORTEST_LIMIT_WINDOW_MS..=LONGEST_LIMIT_WINDOW_MS).contains(&millis) && millis % 1000 == 0)
        .then_some(millis)
}

fn key_written(value: &ast::Expr) -> Option<RateLimitKey> {
    let (first, rest) = tree::dotted(value)?;
    let [variant] = rest.as_slice() else {
        return None;
    };
    (tree::name_ref_text(&first)? == RateLimitKey::NAME).then_some(())?;
    RateLimitKey::from_name(&tree::name_ref_text(variant)?)
}

fn invalid(message: impl Into<String>, at: Span, label: &str) -> Diagnostic {
    Diagnostic::new(&rule::ROUTE_RATE_LIMIT_INVALID, message)
        .at(at, label)
        .reason(REASON)
}

/// The limit `route` counts in place of the defaults: its own, counted
/// alone, or else the one its API gives its routes, which they share.
pub fn limit_of(apis: &Apis, group: &GroupModel, route: &RouteModel) -> Option<RouteRateLimit> {
    if let Some(own) = route.rate_limit {
        return Some(RouteRateLimit {
            limit: own.limit,
            by: own.by,
            counted: route.key.as_str().to_string(),
        });
    }
    let own = apis.get(&group.api)?.rate_limit?;
    Some(RouteRateLimit {
        limit: own.limit,
        by: own.by,
        counted: format!("api {}", group.api),
    })
}

/// A limit a route Jadpo answers keeps beside the requests it counts
/// (area 16, "What a limit you write changes"): how many sends to one
/// email address, or how many wrong codes for one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Also {
    pub limit: RateLimit,
    /// What it counts when that isn't requests: `wrong_codes`.
    pub counts: Option<&'static str>,
}

/// What a route is limited to, as OpenAPI and `jadpo routes` say it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Shown {
    pub limit: Limit,
    pub also: Vec<Also>,
}

impl From<Limit> for Shown {
    fn from(limit: Limit) -> Shown {
        Shown {
            limit,
            also: Vec::new(),
        }
    }
}

/// What `route` is limited to, its own limit or the default one, for
/// OpenAPI and `jadpo routes`.
pub fn shown(apis: &Apis, group: &GroupModel, route: &RouteModel) -> Shown {
    if let Some(own) = limit_of(apis, group, route) {
        return Limit {
            limit: own.limit,
            by: own.by,
        }
        .into();
    }
    defaults(route.authentication).into()
}

/// The default limit of a route that `authentication` lets in: its
/// caller's, or its address's while no one is signed in.
pub fn defaults(authentication: Authentication) -> Limit {
    match authentication {
        Authentication::Public => Limit {
            limit: ADDRESS_LIMIT,
            by: RateLimitKey::Ip,
        },
        Authentication::Required | Authentication::Fresh => Limit {
            limit: CALLER_LIMIT,
            by: RateLimitKey::Caller,
        },
    }
}

/// What a route the runtime answers is limited to: an API's limit never
/// reaches it, so the defaults for who may call it, and what it counts
/// beside them.
pub fn generated(authentication: Authentication, also: Vec<Also>) -> Shown {
    Shown {
        limit: defaults(authentication),
        also,
    }
}

/// One send to an email address per `resend_after_ms`, and `sends` an hour.
fn sends(resend_after_ms: i64, sends: u32) -> Vec<Also> {
    vec![
        Also {
            limit: RateLimit {
                requests: 1,
                window_ms: u32::try_from(resend_after_ms).unwrap_or(u32::MAX),
            },
            counts: None,
        },
        Also {
            limit: RateLimit {
                requests: sends,
                window_ms: 60 * 60 * 1000,
            },
            counts: None,
        },
    ]
}

/// A magic-link route's limits: `magic`'s sends, or its wrong codes.
pub fn magic_link(step: MagicLinkStep, magic: &MagicLink) -> Shown {
    let also = match step {
        MagicLinkStep::Request => sends(magic.resend_after_ms, magic.codes_per_hour),
        MagicLinkStep::Confirm | MagicLinkStep::Redeem => vec![Also {
            limit: RateLimit {
                requests: magic.failed_codes_per_day,
                window_ms: 24 * 60 * 60 * 1000,
            },
            counts: Some("wrong_codes"),
        }],
    };
    generated(step.authentication(), also)
}

/// A data-request route's limits: its address check's sends, which are
/// fixed (area 15, "Data requests").
pub fn privacy(step: PrivacyStep) -> Shown {
    let also = if step == PrivacyStep::Request {
        sends(PRIVACY_CHECK_RESEND_AFTER_MS, PRIVACY_CHECKS_PER_HOUR)
    } else {
        Vec::new()
    };
    generated(step.authentication(), also)
}

/// `requests` over `within`, by what: `5/1h ip`, as `jadpo routes` shows a
/// limit, and each of its other limits after it.
pub fn words(shown: &Shown) -> String {
    let mut out = format!(
        "{}/{} {}",
        shown.limit.limit.requests,
        within(shown.limit.limit),
        shown.limit.by.name()
    );
    for also in &shown.also {
        let counts = also
            .counts
            .map(|counts| format!(" {}", counts.replace('_', " ")))
            .unwrap_or_default();
        out.push_str(&format!(
            ", {}/{} email{counts}",
            also.limit.requests,
            within(also.limit)
        ));
    }
    out
}

/// A limit's window as it is written: `1h`, `90s`.
pub fn within(limit: RateLimit) -> String {
    Dur {
        millis: i128::from(limit.window_ms),
    }
    .to_string()
}
