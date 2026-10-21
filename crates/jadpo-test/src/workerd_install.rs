//! workerd, Cloudflare's own Workers runtime, for the tests that run a
//! Worker (docs/compiler-runtime.md §2.6, "The Workers host"; chunks 8.2a
//! and 8.2e): the release pinned in `tools/workers/package-lock.json`,
//! installed there by `npm ci --ignore-scripts` (the platform's package
//! carries the binary), never globally, and found by every test that needs
//! it through [`workerd`]. The Workers host's own tests include this file
//! by its path, so there is one installer.
//!
//! Tests in several processes and threads ask at once (the gate runs the
//! `compiled` and `wasm32` groups side by side), and `npm ci` empties
//! `node_modules` before it fills it, so the check and the install are
//! made under one lock across processes, `tools/workers/.install.lock`:
//! the first to find the pinned release missing installs it, and every
//! other waits and then finds it. An install that leaves no binary is
//! tried again, since npm skips a platform package it couldn't fetch, an
//! optional dependency, without a word; then it says what npm said.

use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::Command;

/// How many times `npm ci` is run before the install is given up.
const ATTEMPTS: usize = 3;

/// `tools/workers/`, from the crate this file is compiled into.
fn tools() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/workers")
}

/// workerd's binary: the one `JADPO_WORKERD` names, else the pinned
/// release in `tools/workers/`, installed there first when it isn't, by
/// one process at a time.
pub fn workerd() -> Result<PathBuf, String> {
    if let Ok(path) = std::env::var("JADPO_WORKERD") {
        return Ok(PathBuf::from(path));
    }
    let tools = tools();
    let pinned = pinned(&tools)?;
    let lock = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(tools.join(".install.lock"))
        .map_err(|error| format!("tools/workers/.install.lock can't be opened: {error}"))?;
    // Held until this returns: released when `lock` is dropped.
    lock.lock()
        .map_err(|error| format!("tools/workers/.install.lock can't be locked: {error}"))?;
    if let Some(found) = installed(&tools, &pinned) {
        return Ok(found);
    }
    let mut said = String::new();
    for _ in 0..ATTEMPTS {
        let output = Command::new("npm")
            .args(["ci", "--ignore-scripts", "--no-audit", "--no-fund"])
            .current_dir(&tools)
            .output()
            .map_err(|error| {
                format!(
                    "npm didn't run: workerd is installed with Node's npm into tools/workers/: {error}"
                )
            })?;
        if let Some(found) = installed(&tools, &pinned) {
            return Ok(found);
        }
        said = format!(
            "{}\n{}{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Err(format!(
        "`npm ci` in tools/workers/ gave no workerd {pinned} for this platform in {ATTEMPTS} tries; the last ended {said}"
    ))
}

/// The release `tools/workers/package.json` pins.
fn pinned(tools: &Path) -> Result<String, String> {
    let manifest = fs::read_to_string(tools.join("package.json"))
        .map_err(|error| format!("tools/workers/package.json can't be read: {error}"))?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest)
        .map_err(|error| format!("tools/workers/package.json isn't JSON: {error}"))?;
    manifest
        .get("dependencies")
        .and_then(|dependencies| dependencies.get("workerd"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "tools/workers/package.json pins no workerd".to_string())
}

/// The binary of the platform's own workerd package at release `pinned`,
/// rather than npm's wrapper, which would run it as a child of its own
/// that stopping the wrapper doesn't stop.
fn installed(tools: &Path, pinned: &str) -> Option<PathBuf> {
    fs::read_dir(tools.join("node_modules/@cloudflare"))
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("workerd-"))
        .filter(|entry| {
            fs::read_to_string(entry.path().join("package.json"))
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
                .is_some_and(|package| {
                    package.get("version").and_then(serde_json::Value::as_str) == Some(pinned)
                })
        })
        .map(|entry| entry.path().join("bin/workerd"))
        .find(|binary| binary.is_file())
}
