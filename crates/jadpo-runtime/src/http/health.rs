//! Health (area 7, "Health"): the runtime answers `GET /health/live` and
//! `GET /health/ready` itself, outside every API, its OpenAPI document and
//! its record, and outside the rate limits, so a load balancer's probes are
//! never refused.
//!
//! - `/health/live` answers 200 while the process runs.
//! - `/health/ready` asks each store the application's entities live in to
//!   run `SELECT 1`, all within two seconds, and answers 200 when every one
//!   did, 503 otherwise; integrations are reported as unchecked, by name,
//!   when area 11 builds them. A worker reuses its last answer for a
//!   second ([`Readiness`]), so probes, which no rate limit counts, cost
//!   each store at most one statement a second on each worker.
//!
//! Both answer `HEAD` as `GET` and `OPTIONS` with `Allow`; another method
//! is 405.

use std::cell::RefCell;

use jadpo_std::http::{READY_WITHIN_MS, RouteTable};

use jadpo_value::json::write_string;

use super::{METHOD_NOT_ALLOWED, Request, Response, runtime_failure};
use crate::race::race;
use crate::{Ctx, Host};

/// What a store said within the time it had.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Store {
    Ready,
    Unavailable,
}

/// How long a worker reuses its last readiness check.
pub(crate) const READY_REUSED_MS: u64 = 1_000;

/// One worker's last readiness check: when it was made, on the host's
/// monotonic clock, and its answer.
#[derive(Debug, Default)]
pub(crate) struct Readiness(RefCell<Option<(u64, Response)>>);

/// The health check at `/health/ready` when `ready`, else at
/// `/health/live`, answered; `/health/ready` from `ready_cache` when it
/// checked within the last second.
pub async fn answer(
    table: &RouteTable,
    ctx: &Ctx<'_>,
    request: &Request,
    request_id: &str,
    ready: bool,
    ready_cache: Option<&Readiness>,
) -> Response {
    match request.method.as_str() {
        "GET" | "HEAD" if ready => reused(table, ctx.host(), ready_cache).await,
        "GET" | "HEAD" => json(200, "{\"status\":\"live\"}".to_string()),
        "OPTIONS" => Response {
            status: 204,
            headers: vec![("allow".to_string(), ALLOW.to_string())],
            ..Response::default()
        },
        _ => {
            let mut response = runtime_failure(METHOD_NOT_ALLOWED, request_id, None);
            response
                .headers
                .push(("allow".to_string(), ALLOW.to_string()));
            response
        }
    }
}

const ALLOW: &str = "GET, HEAD, OPTIONS";

fn json(status: u16, body: String) -> Response {
    Response {
        status,
        headers: vec![
            ("content-type".to_string(), "application/json".to_string()),
            ("cache-control".to_string(), "no-store".to_string()),
        ],
        body: body.into_bytes(),
        ..Response::default()
    }
}

/// The readiness answer, from `cache` when it is under a second old on the
/// host's monotonic clock, otherwise checked again and kept there. A host
/// with no such clock, as a test's, checks every time.
async fn reused(table: &RouteTable, host: &dyn Host, cache: Option<&Readiness>) -> Response {
    let now = host.monotonic_ms();
    if let (Some(cache), Some(now)) = (cache, now)
        && let Some((at, answer)) = &*cache.0.borrow()
        && now.saturating_sub(*at) < READY_REUSED_MS
    {
        return answer.clone();
    }
    let answer = readiness(table, host).await;
    if let (Some(cache), Some(now)) = (cache, now) {
        *cache.0.borrow_mut() = Some((now, answer.clone()));
    }
    answer
}

/// `{"status":"ready","stores":{"main":"ready"},"integrations":{}}`, 200
/// when every store answered within two seconds, 503 with `not_ready`
/// otherwise.
async fn readiness(table: &RouteTable, host: &dyn Host) -> Response {
    let found: RefCell<Vec<Store>> = RefCell::new(Vec::new());
    let checks = async {
        for store in &table.stores {
            let said = check(host, store).await;
            found.borrow_mut().push(said);
        }
    };
    let _ = race(checks, host.sleep(u64::from(READY_WITHIN_MS))).await;
    let found = found.into_inner();
    let mut ready = true;
    let mut stores = String::from("{");
    for (at, store) in table.stores.iter().enumerate() {
        let said = found.get(at).copied().unwrap_or(Store::Unavailable);
        ready &= said == Store::Ready;
        if at > 0 {
            stores.push(',');
        }
        write_string(store, &mut stores);
        stores.push_str(match said {
            Store::Ready => ":\"ready\"",
            Store::Unavailable => ":\"unavailable\"",
        });
    }
    stores.push('}');
    let mut integrations = String::from("{");
    for (at, integration) in table.integrations.iter().enumerate() {
        if at > 0 {
            integrations.push(',');
        }
        write_string(integration, &mut integrations);
        integrations.push_str(":\"unchecked\"");
    }
    integrations.push('}');
    let status = if ready { "ready" } else { "not_ready" };
    json(
        if ready { 200 } else { 503 },
        format!("{{\"status\":\"{status}\",\"stores\":{stores},\"integrations\":{integrations}}}"),
    )
}

/// Whether store `name` runs `SELECT 1`, one statement of its own.
async fn check(host: &dyn Host, name: &str) -> Store {
    let Some(database) = host.database(name) else {
        return Store::Unavailable;
    };
    match database.query_once("SELECT 1", &[]).await {
        Ok(_) => Store::Ready,
        Err(_) => Store::Unavailable,
    }
}
