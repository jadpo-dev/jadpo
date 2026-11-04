//! `rate_limit: { requests: within: by: }` on a route or an API (area 16,
//! "Rate limits"; chunk G4), read: how many requests, over how long, and
//! what they are counted by. A route's or an API's own limit stands in for
//! the default limits, on its route or on the API's routes that write none.

use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_std::http::{
    ADDRESS_LIMIT, Authentication, CALLER_LIMIT, LONGEST_LIMIT_WINDOW_MS, RateLimit,
    RouteRateLimit, SHORTEST_LIMIT_WINDOW_MS,
};
use jadpo_std::{Dur, RateLimitKey};
use jadpo_syntax::ast::{self, AstNode};

use super::{Apis, GroupModel, RouteModel};
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

/// What `route` is limited to, its own limit or the default one, for
/// OpenAPI and `jadpo routes`.
pub fn shown(apis: &Apis, group: &GroupModel, route: &RouteModel) -> Limit {
    if let Some(own) = limit_of(apis, group, route) {
        return Limit {
            limit: own.limit,
            by: own.by,
        };
    }
    match route.authentication {
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

/// `requests` over `within`, by what: `5/1h ip`, as `jadpo routes` shows a
/// limit.
pub fn words(limit: &Limit) -> String {
    format!(
        "{}/{} {}",
        limit.limit.requests,
        within(limit.limit),
        limit.by.name()
    )
}

/// A limit's window as it is written: `1h`, `90s`.
pub fn within(limit: RateLimit) -> String {
    Dur {
        millis: i128::from(limit.window_ms),
    }
    .to_string()
}
