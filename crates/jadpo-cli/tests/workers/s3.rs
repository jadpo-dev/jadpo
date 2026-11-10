//! The S3 driver on Workers (area 16, "The S3 driver"): the disk driver's
//! tests as a Worker deployable under workerd, its outbound `fetch()`
//! reaching the stand-in S3 server on this machine.

use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::s3_files::{BUCKET, Got, KEY_ID, SECRET, project_files, runs_on_s3, written};
use crate::support::fake_s3::FakeS3;

use super::{Database, Workerd, jadpo};

#[test]
fn workerd_passes_the_disk_drivers_tests_on_s3() {
    if !jadpo_test::live() {
        return;
    }
    let s3 = FakeS3::start(BUCKET, KEY_ID, SECRET);
    let mut files = project_files();
    for (path, text) in &mut files {
        if path == "app.jadpo" {
            // Beside the connections, the rate-limit binding a Worker counts
            // in, and the Worker itself.
            let end = text.rfind("    }\n}\n").expect("the connections' end");
            text.insert_str(
                end,
                "        limits: {\n            driver: Driver.cloudflare_rate_limits\n            namespace: 1001\n        }\n    }\n    rate_limits: {\n        store: limits\n    }\n    deployables: {\n        worker: {\n            serves: [main]\n            runs: [Trigger.schedules, Trigger.subscribers]\n            target: Target.wasm\n        }\n",
            );
        }
    }
    let root = written("workerd", &files);
    jadpo(&root, &["build", "--json"]);
    let out: &Path = &root.join("build/target/worker");
    let wrangler: Value = serde_json::from_str(
        &fs::read_to_string(out.join("wrangler.json")).expect("the Wrangler file"),
    )
    .expect("JSON");
    let database = Database::on_tcp();
    crate::deployable::apply(&database.native, &crate::deployable::migrations(&root));
    let host = s3.address();
    let worker = Workerd::start(
        out,
        &wrangler,
        &[
            ("DATABASE_URL", database.worker.as_str()),
            ("S3_HOST", host.as_str()),
            ("S3_KEY_ID", KEY_ID),
            ("S3_SECRET", SECRET),
        ],
    );
    let send = |method: &str, path: &str, headers: &[(&str, &str)], body: &[u8]| -> Got {
        // `Workerd::exchange` writes `Content-Type: application/json` itself.
        let headers: Vec<(&str, &str)> = headers
            .iter()
            .copied()
            .filter(|(name, _)| !name.eq_ignore_ascii_case("content-type"))
            .collect();
        // A part's address redirects without reading its body, and workerd
        // closes the connection on a client still writing one, as any
        // server may; the bytes then go to the store, so none are sent here.
        let body = if path.contains("/parts/") {
            &[] as &[u8]
        } else {
            body
        };
        let (status, _, body) =
            worker.exchange(method, path, &headers, &String::from_utf8_lossy(body));
        // The head as it came: a signed address's case matters.
        let head = worker.last_head.lock().expect("the head").clone();
        let all = head
            .lines()
            .skip(1)
            .filter_map(|line| line.split_once(": "))
            .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_string()))
            .collect();
        (status, all, body)
    };
    runs_on_s3(&send, &s3, &[]);
    assert!(
        !worker.log().contains(SECRET),
        "the credential isn't logged"
    );
}
