//! A person's erasure on Workers (area 15, "What erasure doesn't rewrite";
//! chunk P4c): a desk built for `Target.wasm` erases a member through its
//! routes, their ticket in another store and the log written after the
//! commit in a database of its own; a restore of `main`, which leaves the
//! log, brings them back; and the next
//! isolate erases them again before it answers anything, saying so, while a
//! start with nothing to do says nothing.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::deployable::{apply, rows};

use super::{Database, Workerd, jadpo};

const APPLICATION: &str = "    connections: {
        limits: {
            driver: Driver.cloudflare_rate_limits
            namespace: 1001
        }
        main: {
            driver: Driver.postgres
            url: config.database_url
        }
        archive: {
            driver: Driver.postgres
            url: config.archive_url
        }
        erasures: {
            driver: Driver.postgres
            url: config.erasures_url
        }
    }
    rate_limits: {
        store: limits
    }
    privacy: {
        erasure_log: {
            store: erasures
        }
    }
    deployables: {
        desk: {
            serves: [main]
            runs: [Trigger.subscribers]
            target: Target.wasm
        }
    }
";

const CONFIG: &str =
    "    database_url: PostgresUrl\n    archive_url: PostgresUrl\n    erasures_url: PostgresUrl\n";

fn project() -> PathBuf {
    let target = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let root = target
        .parent()
        .unwrap_or(target)
        .join("jadpo-compiled/workers/erased-desk");
    let _ = std::fs::remove_dir_all(root.join("database"));
    for (path, text) in crate::erasure_stores::served_files(APPLICATION, CONFIG) {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        std::fs::write(path, text).expect("write");
    }
    for args in [&["migrate", "new", "initial"][..], &["records", "update"]] {
        jadpo(&root, args);
    }
    root
}

#[test]
fn an_isolate_replays_the_log_before_it_answers() {
    if !jadpo_test::live() {
        return;
    }
    let root = project();
    jadpo(&root, &["build", "--json"]);
    let out = root.join("build/target/desk");
    let wrangler: Value = serde_json::from_str(
        &std::fs::read_to_string(out.join("wrangler.json")).expect("the Wrangler file"),
    )
    .expect("JSON");

    let (main, archive, erasures) = (Database::on_tcp(), Database::on_tcp(), Database::on_tcp());
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    apply(&main.native, &steps["main"]);
    apply(&archive.native, &steps["archive"]);
    apply(&erasures.native, &steps["erasures"]);
    let bindings = [
        ("DATABASE_URL", main.worker.as_str()),
        ("ARCHIVE_URL", archive.worker.as_str()),
        ("ERASURES_URL", erasures.worker.as_str()),
    ];
    let worker = Workerd::start(&out, &wrangler, &bindings);
    let (status, body) = worker.send("POST", "/members", &[], r#"{"handle":"ada"}"#);
    assert_eq!(status, 201, "{body}\n{}", worker.log());
    let joined: Value = serde_json::from_str(&body).expect("JSON");
    let id = joined["id"].as_str().expect("an id").to_string();
    let ticket = format!(r#"{{"owner_id":"{id}","title":"The heating"}}"#);
    let (status, body) = worker.send("POST", "/members/tickets", &[], &ticket);
    assert_eq!(status, 201, "{body}\n{}", worker.log());
    let (status, body) = worker.send("POST", &format!("/members/{id}/close"), &[], "");
    assert_eq!(status, 200, "{body}\n{}", worker.log());
    assert!(rows(&main.native, "SELECT id::text FROM member").is_empty());
    // Its subscribers, once `waitUntil` has run them.
    let waited = std::time::Instant::now();
    while rows(&erasures.native, "SELECT entity FROM jadpo_erasure_log").is_empty()
        || !rows(&archive.native, "SELECT id::text FROM ticket").is_empty()
    {
        assert!(
            waited.elapsed() < std::time::Duration::from_secs(30),
            "no entry: {}",
            worker.log()
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    drop(worker);

    // A restore of `main`, which leaves the log.
    rows(
        &main.native,
        &format!("INSERT INTO member (id, handle) VALUES ('{id}', 'ada')"),
    );
    let restarted = Workerd::start(&out, &wrangler, &bindings);
    let (status, body) = restarted.send("POST", "/members", &[], r#"{"handle":"bob"}"#);
    assert_eq!(status, 201, "{body}\n{}", restarted.log());
    let left = rows(&main.native, "SELECT handle FROM member");
    assert_eq!(left, [["bob".to_string()]], "{}", restarted.log());
    assert!(
        restarted.log().contains("runtime.erasures_replayed"),
        "{}",
        restarted.log()
    );
    drop(restarted);

    // Nothing to do: nothing said.
    let quiet = Workerd::start(&out, &wrangler, &bindings);
    let (status, body) = quiet.send("POST", "/members", &[], r#"{"handle":"cy"}"#);
    assert_eq!(status, 201, "{body}\n{}", quiet.log());
    assert!(
        !quiet.log().contains("runtime.erasures_replayed"),
        "{}",
        quiet.log()
    );
}
