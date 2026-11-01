//! A sealed field on Workers (area 15, "Encrypted fields"; chunk P9): a
//! clinic built for `Target.wasm` lists its key among the Wrangler file's
//! required secrets, seals what `POST /consultations` is given into a column
//! holding only sealed bytes, opens it for `GET`, and once its key is rotated
//! through `FIELD_ENCRYPTION_KEY_PREVIOUS` still reads it, the cron's re-seal
//! moving the row to the new key.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::deployable::{apply, migrations, rows};
use crate::encrypted_fields::{FIRST_KEY, SECOND_KEY, project_at};

use super::{Database, Workerd, jadpo, now_ms};

const DECLARED: &str = "    connections: {
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
        clinic: {
            serves: [main]
            runs: [Trigger.schedules]
            target: Target.wasm
        }
    }
";

fn project() -> PathBuf {
    let target = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let root = target
        .parent()
        .unwrap_or(target)
        .join("jadpo-compiled/workers/sealed-clinic");
    project_at(&root, DECLARED);
    root
}

#[test]
fn a_sealed_field_round_trips_on_workers_and_its_key_rotates() {
    if !jadpo_test::live() {
        return;
    }
    let root = project();
    jadpo(&root, &["build", "--json"]);
    let out = root.join("build/target/clinic");
    let wrangler: Value = serde_json::from_str(
        &std::fs::read_to_string(out.join("wrangler.json")).expect("the Wrangler file"),
    )
    .expect("JSON");
    let required = wrangler["secrets"]["required"].to_string();
    assert!(required.contains("FIELD_ENCRYPTION_KEY"), "{wrangler}");

    let database = Database::on_tcp();
    apply(&database.native, &migrations(&root));
    let worker = Workerd::start(
        &out,
        &wrangler,
        &[
            ("DATABASE_URL", database.worker.as_str()),
            ("FIELD_ENCRYPTION_KEY", FIRST_KEY),
        ],
    );
    let (status, body) = worker.send(
        "POST",
        "/consultations",
        &[],
        r#"{"notes":"Allergic to penicillin."}"#,
    );
    assert_eq!(status, 201, "{body}\n{}", worker.log());
    let answered: Value = serde_json::from_str(&body).expect("JSON");
    assert_eq!(answered["notes"], "Allergic to penicillin.");

    // The column holds only sealed bytes.
    let kept = rows(
        &database.native,
        "SELECT encode(\"notes\", 'hex'), \"id\"::text FROM \"consultation\"",
    );
    let (hex, id) = (&kept[0][0], kept[0][1].clone());
    assert!(hex.starts_with("0101"), "the sealed format: {hex}");
    assert!(
        !hex.contains(&hex_of("penicillin")),
        "the column holds the value"
    );
    let (status, body) = worker.send("GET", &format!("/consultations/{id}"), &[], "");
    assert_eq!(status, 200, "{body}\n{}", worker.log());
    assert_eq!(
        serde_json::from_str::<Value>(&body).expect("JSON")["notes"],
        "Allergic to penicillin."
    );
    drop(worker);

    // Rotated: what the old key sealed still reads, and the cron moves it.
    let rotated = Workerd::start(
        &out,
        &wrangler,
        &[
            ("DATABASE_URL", database.worker.as_str()),
            ("FIELD_ENCRYPTION_KEY", SECOND_KEY),
            ("FIELD_ENCRYPTION_KEY_PREVIOUS", FIRST_KEY),
        ],
    );
    let (status, body) = rotated.send("GET", &format!("/consultations/{id}"), &[], "");
    assert_eq!(status, 200, "{body}\n{}", rotated.log());
    let hour = 3_600_000;
    let at = now_ms();
    assert_eq!(rotated.scheduled(at), "ok", "{}", rotated.log());
    assert_eq!(rotated.scheduled(at + 2 * hour), "ok", "{}", rotated.log());
    let moved = rows(
        &database.native,
        "SELECT encode(\"notes\", 'hex') FROM \"consultation\"",
    );
    assert_ne!(&moved[0][0], hex, "{}", rotated.log());
    drop(rotated);

    // So the old key can go, as `jadpo config rotation` says.
    let after = Workerd::start(
        &out,
        &wrangler,
        &[
            ("DATABASE_URL", database.worker.as_str()),
            ("FIELD_ENCRYPTION_KEY", SECOND_KEY),
        ],
    );
    let (status, body) = after.send("GET", &format!("/consultations/{id}"), &[], "");
    assert_eq!(status, 200, "{body}\n{}", after.log());
    assert_eq!(
        serde_json::from_str::<Value>(&body).expect("JSON")["notes"],
        "Allergic to penicillin."
    );
}

fn hex_of(text: &str) -> String {
    text.bytes().map(|byte| format!("{byte:02x}")).collect()
}
