//! A route's and an API's own `rate_limit:` on Workers (area 16, "Rate
//! limits"; chunk G4): the Wrangler file names a binding for each limit,
//! numbered by window and then requests, and the Worker counts each route's
//! requests in its binding, answering what `jadpo dev` and a native
//! deployable answer. The binding is the stand-in the harness gives workerd,
//! which has none of its own; what Cloudflare's does is checked in 8.2n.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::route_limits::{answers_as_every_host_does, project_at, worker_application};

use super::{Workerd, jadpo};

fn project() -> PathBuf {
    let target = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let root = target
        .parent()
        .unwrap_or(target)
        .join("jadpo-compiled/workers/route-limits");
    project_at(&root, &worker_application());
    jadpo(&root, &["records", "update"]);
    root
}

#[test]
fn a_routes_limit_is_counted_in_its_own_binding_on_workers() {
    let root = project();
    jadpo(&root, &["build", "--json"]);
    let out = root.join("build/target/desk");
    let wrangler: Value = serde_json::from_str(
        &std::fs::read_to_string(out.join("wrangler.json")).expect("the Wrangler file"),
    )
    .expect("JSON");
    let binding = |name: &str, namespace: &str, limit: u32, period: u32| {
        json!({
            "name": name,
            "namespace_id": namespace,
            "simple": { "limit": limit, "period": period },
        })
    };
    assert_eq!(
        wrangler["ratelimits"],
        json!([
            binding("LIMITS_2_PER_10S", "1001", 2, 10),
            binding("LIMITS_2_PER_60S", "1002", 2, 60),
            binding("LIMITS_3_PER_60S", "1003", 3, 60),
            binding("LIMITS_120_PER_60S", "1004", 120, 60),
            binding("LIMITS_600_PER_60S", "1005", 600, 60),
        ]),
    );

    // Wrangler accepts them, with no account and no network.
    let dry = super::queues::dry_run(&out);
    assert!(dry.contains("--dry-run: exiting now."), "{dry}");
    for refused in ["WARNING", "ERROR", "jadpo-offline:"] {
        assert!(!dry.contains(refused), "{refused} in:\n{dry}");
    }

    let worker = Workerd::start(&out, &wrangler, &[]);
    let from = [("CF-Connecting-IP", "203.0.113.9")];
    answers_as_every_host_does(|method, path| {
        let (status, head, _) = worker.exchange(method, path, &from, "");
        (status, head)
    });
    let log = worker.log();
    assert_eq!(
        log.matches("rate-limiting binding: route POST /releases address 203.0.113.9 within")
            .count(),
        3,
        "{log}"
    );
    assert_eq!(
        log.matches("rate-limiting binding: route POST /releases address 203.0.113.9 over")
            .count(),
        1,
        "{log}"
    );
    assert_eq!(
        log.matches("rate-limiting binding: api partners address 203.0.113.9 ")
            .count(),
        4,
        "the API's routes share one count: {log}"
    );
    // The binding answered, so nothing fell back to the isolate's memory.
    assert!(!log.contains("rate_limits.fallback"), "{log}");
    // Another address has its own allowance.
    let (status, _, _) = worker.exchange(
        "POST",
        "/releases",
        &[("CF-Connecting-IP", "198.51.100.4")],
        "",
    );
    assert_eq!(status, 200);
}
