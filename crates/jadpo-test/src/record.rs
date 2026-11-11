//! The last run's failures, kept in `build/test/last-run.json` so
//! `jadpo test --failed` can rerun them (area 13, "Running tests"), and
//! its passes, so the keyboard view shows each test's last result. Each
//! run updates the record for the tests it ran: a test that passed moves
//! to `passed`, one that failed to `failed`, and the tests it didn't run
//! keep their place, as long as they still exist. Only a project keeps a record;
//! loose files have no `build/`.

use std::collections::BTreeSet;
use std::path::Path;

use jadpo_diag::{Diagnostic, rule};
use jadpo_term::{Json, SCHEMA_VERSION};

use crate::{Test, TestResult};

/// Where the record lives, relative to the project root.
pub const RECORD: &str = "build/test/last-run.json";

/// The keys of the tests that failed last time.
pub(crate) fn read(root: &Path, project: bool) -> Result<BTreeSet<String>, Box<Diagnostic>> {
    let missing = |reason: String| {
        Box::new(
            Diagnostic::new(&rule::TEST_NO_TESTS, "There's no last run to rerun")
                .reason(reason)
                .next("Run `jadpo test` first, then `jadpo test --failed`"),
        )
    };
    if !project {
        return Err(missing(
            "Only a project, a folder with `app.jadpo`, keeps the failures of its last run."
                .to_string(),
        ));
    }
    let Ok(text) = std::fs::read_to_string(root.join(RECORD)) else {
        return Err(missing(format!(
            "`jadpo test` keeps the failures of each run in `{RECORD}`, and there isn't one yet."
        )));
    };
    let parsed: serde_json::Value = serde_json::from_str(&text).map_err(|_| {
        missing(format!(
            "`{RECORD}` isn't a record `jadpo test` wrote, so it was left alone."
        ))
    })?;
    Ok(parsed["failed"]
        .as_array()
        .map(|failed| {
            failed
                .iter()
                .filter_map(|test| test["key"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default())
}

/// The keys of the tests that failed and that passed the last time each
/// ran, by the record; empty when there is none.
pub(crate) fn results(root: &Path) -> (BTreeSet<String>, BTreeSet<String>) {
    let Ok(text) = std::fs::read_to_string(root.join(RECORD)) else {
        return Default::default();
    };
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Default::default();
    };
    let keys = |field: &str| -> BTreeSet<String> {
        parsed[field]
            .as_array()
            .map(|tests| {
                tests
                    .iter()
                    .filter_map(|test| test["key"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    (keys("failed"), keys("passed"))
}

/// Updates the record with `results`. A record that can't be written
/// only costs the next `--failed`, so failing to write it is silent.
pub(crate) fn write(root: &Path, tests: &[Test], results: &[TestResult]) {
    let (mut failed, mut passed) = self::results(root);
    for result in results {
        if result.failure.is_some() {
            failed.insert(result.test.key.clone());
            passed.remove(&result.test.key);
        } else {
            failed.remove(&result.test.key);
            passed.insert(result.test.key.clone());
        }
    }
    let entries = |keys: &BTreeSet<String>| -> Json {
        Json::Array(
            tests
                .iter()
                .filter(|test| keys.contains(&test.key))
                .map(|test| {
                    Json::object([
                        ("key", Json::from(&test.key)),
                        ("name", Json::from(&test.name)),
                        ("file", Json::from(&test.file)),
                        ("line", test.line.into()),
                    ])
                })
                .collect(),
        )
    };
    // The tests that passed are kept too, for the keyboard view's last
    // results (chunk T3).
    let document = Json::object([
        ("schemaVersion", Json::Int(SCHEMA_VERSION)),
        ("failed", entries(&failed)),
        ("passed", entries(&passed)),
    ]);
    let path = root.join(RECORD);
    if let Some(folder) = path.parent()
        && std::fs::create_dir_all(folder).is_ok()
    {
        let _ = std::fs::write(&path, document.to_document());
    }
}
