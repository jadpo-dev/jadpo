//! The trace `--keep-failed` keeps (area 13, "Keeping a failed test's
//! database: `--keep-failed`"; area 17, "The request timeline in `jadpo
//! dev`"; chunk E0): a failed test's timeline, the same document `jadpo
//! dev` keeps for a request, in `build/test/traces/<id>.json`, which its
//! report names and `jadpo trace <id>` reads. Its id is `t-` and eight hex
//! digits. The newest [`KEPT`] stay.

use std::path::{Path, PathBuf};

use crate::outcome::TestFailure;
use crate::{Rerun, Test};

/// Where failed tests' traces are kept, relative to the project.
pub const FOLDER: &str = "build/test/traces";

/// How many are kept.
pub const KEPT: usize = 500;

/// `failure`, with its trace kept and named, when the run keeps failed
/// tests' traces and the test kept a timeline.
pub(crate) fn keep(
    rerun: &Rerun,
    test: &Test,
    failure: Option<Box<TestFailure>>,
    timeline: Option<serde_json::Value>,
) -> Option<Box<TestFailure>> {
    let mut failure = failure?;
    if rerun.keep_trace
        && let Some(document) = timeline
    {
        let location = format!("{}:{}", failure.location.path, failure.location.line);
        failure.trace = kept(&rerun.project, &test.name, &location, document);
    }
    Some(failure)
}

/// Writes `document` as test `name`'s trace, failed at `location`: its id.
fn kept(project: &Path, name: &str, location: &str, mut document: serde_json::Value) -> Option<String> {
    let id = format!("t-{}", hex(&random()));
    if let Some(object) = document.as_object_mut() {
        object.insert(
            "test".to_string(),
            serde_json::json!({ "name": name, "location": location }),
        );
        object.insert("traceId".to_string(), serde_json::json!(id));
    }
    let folder = project.join(FOLDER);
    std::fs::create_dir_all(&folder).ok()?;
    std::fs::write(folder.join(format!("{id}.json")), format!("{document}\n")).ok()?;
    if let Ok(entries) = std::fs::read_dir(&folder) {
        let mut kept: Vec<(std::time::SystemTime, PathBuf)> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| Some((entry.metadata().ok()?.modified().ok()?, entry.path())))
            .collect();
        if kept.len() > KEPT {
            kept.sort();
            for (_, path) in &kept[..kept.len() - KEPT] {
                let _ = std::fs::remove_file(path);
            }
        }
    }
    Some(id)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Four bytes no other kept trace is likely to have: the clock's
/// nanoseconds, the process and a count, mixed.
fn random() -> [u8; 4] {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos() as u64);
    let mut state = nanos
        ^ u64::from(std::process::id()).rotate_left(32)
        ^ COUNT.fetch_add(1, Ordering::Relaxed).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    state = (state ^ (state >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    state = (state ^ (state >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    state ^= state >> 31;
    let bytes = state.to_le_bytes();
    [bytes[0], bytes[1], bytes[2], bytes[3]]
}
