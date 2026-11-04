//! A route that sends raw bytes on Workers (area 7, "Sending raw bytes" and
//! "Answers that never change"; chunk G3): built for `Target.wasm` and
//! served by workerd, the field goes out to the byte with its media type and
//! length, `HEAD` keeps the length and sends no body, a failure stays JSON
//! and a repeat under its key gets the first answer.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::deployable::{apply, migrations};
use crate::sends::{DIGEST, RAW, files, posted};

use super::{Database, Workerd, jadpo};

const APP: &str = "application {
    connections: {
        limits: {
            driver: Driver.cloudflare_rate_limits
            namespace: 1001
        }
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
    }
    rate_limits: {
        store: limits
    }
    deployables: {
        registry: {
            serves: [main]
            target: Target.wasm
        }
    }
}
";

const CONFIG: &str = "config {
    database_url: PostgresUrl
}
";

fn project() -> PathBuf {
    let target = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let root = target
        .parent()
        .unwrap_or(target)
        .join("jadpo-compiled/workers/sends");
    // The build stays between runs, the project is written afresh.
    let _ = fs::remove_dir_all(root.join("domains"));
    for (path, text) in files() {
        let text = if path == "app.jadpo" {
            APP.to_string()
        } else {
            text
        };
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        fs::write(path, text).expect("write");
    }
    fs::write(root.join("config.jadpo"), CONFIG).expect("write");
    if !root.join("migrations").exists() {
        jadpo(&root, &["migrate", "new", "initial"]);
    }
    jadpo(&root, &["records", "update"]);
    root
}

#[test]
fn workerd_sends_the_field_to_the_byte() {
    if !jadpo_test::live() {
        return;
    }
    let root = project();
    jadpo(&root, &["build", "--json"]);
    let out: &Path = &root.join("build/target/registry");
    let wrangler: Value = serde_json::from_str(
        &fs::read_to_string(out.join("wrangler.json")).expect("the Wrangler file"),
    )
    .expect("JSON");
    let database = Database::on_tcp();
    apply(&database.native, &migrations(&root));
    let worker = Workerd::start(
        out,
        &wrangler,
        &[("DATABASE_URL", database.worker.as_str())],
    );
    let log = || worker.log();
    let path = format!("/registry/v1/archives/{DIGEST}");

    let keyed = [("Idempotency-Key", "first")];
    let posted = posted(DIGEST, &RAW);
    let (status, head, body) = worker.exchange("POST", "/registry/v1/archives", &keyed, &posted);
    assert_eq!(status, 201, "{head}\n{}", log());
    assert_eq!(body, RAW, "{head}");
    assert!(
        head.contains("content-type: application/x-tar\r\n"),
        "{head}"
    );
    assert!(head.contains("content-length: 8\r\n"), "{head}");
    assert!(!head.contains("cache-control"), "{head}");
    let (status, head, body) = worker.exchange("POST", "/registry/v1/archives", &keyed, &posted);
    assert_eq!(status, 201, "{head}\n{}", log());
    assert_eq!(body, RAW, "{head}");
    assert!(
        head.contains("content-type: application/x-tar\r\n"),
        "{head}"
    );

    let (status, head, body) = worker.exchange("GET", &path, &[], "");
    assert_eq!(status, 200, "{head}\n{}", log());
    assert_eq!(body, RAW, "{head}");
    assert!(
        head.contains("content-type: application/x-tar\r\n"),
        "{head}"
    );
    assert!(head.contains("content-length: 8\r\n"), "{head}");
    assert!(
        head.contains("cache-control: public, max-age=31536000, immutable\r\n"),
        "{head}"
    );

    let (status, head, body) = worker.exchange("HEAD", &path, &[], "");
    assert_eq!(status, 200, "{head}\n{}", log());
    assert!(body.is_empty(), "{head}");
    assert!(
        head.contains("content-type: application/x-tar\r\n"),
        "{head}"
    );
    assert!(head.contains("content-length: 8\r\n"), "{head}");
    assert!(
        head.contains("cache-control: public, max-age=31536000, immutable\r\n"),
        "{head}"
    );

    let (status, head, body) =
        worker.exchange("GET", &format!("/registry/v1/worked/{DIGEST}"), &[], "");
    assert_eq!(status, 200, "{head}\n{}", log());
    assert_eq!(body, RAW, "{head}");
    assert!(
        head.contains("content-type: application/gzip\r\n"),
        "{head}"
    );
    assert!(!head.contains("cache-control"), "{head}");

    let other = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
    let (status, head, body) =
        worker.exchange("GET", &format!("/registry/v1/archives/{other}"), &[], "");
    assert_eq!(status, 404, "{head}\n{}", log());
    assert!(
        head.contains("content-type: application/json\r\n"),
        "{head}"
    );
    assert!(!head.contains("cache-control"), "{head}");
    let failure: Value = serde_json::from_slice(&body).expect("JSON");
    assert_eq!(failure["error"]["code"], "archive_not_found", "{failure}");
}
