//! Health (area 7, "Health"): the runtime answers `GET /health/live` and
//! `GET /health/ready` itself, outside every API, its OpenAPI document and
//! its record, and outside the rate limits, so a load balancer's probes are
//! never refused.
//!
//! - `/health/live` answers 200 while the process runs.
//! - `/health/ready` asks each store the application's entities live in to
//!   run `SELECT 1`, all within two seconds, and answers 200 when every one
//!   did, 503 otherwise; integrations are reported as unchecked, by name,
//!   when area 11 builds them.
//!
//! Both answer `HEAD` as `GET` and `OPTIONS` with `Allow`; another method
//! is 405.

use std::cell::RefCell;

use jadpo_std::http::{HEALTH_LIVE, HEALTH_READY, READY_WITHIN_MS, RouteTable};

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

/// The health check `segments` names, answered, or `None` when it names
/// none.
pub async fn answer(
    table: &RouteTable,
    ctx: &Ctx<'_>,
    request: &Request,
    request_id: &str,
    segments: &[String],
) -> Option<Response> {
    let ready = if segments == HEALTH_LIVE {
        false
    } else if segments == HEALTH_READY {
        true
    } else {
        return None;
    };
    let label = if ready {
        "GET /health/ready"
    } else {
        "GET /health/live"
    };
    let mut response = match request.method.as_str() {
        "GET" | "HEAD" if ready => readiness(table, ctx.host()).await,
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
    };
    response.route = Some(label.to_string());
    Some(response)
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
    let status = if ready { "ready" } else { "not_ready" };
    json(
        if ready { 200 } else { 503 },
        format!("{{\"status\":\"{status}\",\"stores\":{stores},\"integrations\":{{}}}}"),
    )
}

/// Whether store `name` runs `SELECT 1`.
async fn check(host: &dyn Host, name: &str) -> Store {
    let Some(database) = host.database(name) else {
        return Store::Unavailable;
    };
    let Ok(mut transaction) = database.begin().await else {
        return Store::Unavailable;
    };
    let answered = transaction.query("SELECT 1", &[]).await.is_ok();
    let ended = transaction.rollback().await.is_ok();
    if answered && ended {
        Store::Ready
    } else {
        Store::Unavailable
    }
}
