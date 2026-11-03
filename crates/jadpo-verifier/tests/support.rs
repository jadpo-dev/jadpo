//! What the tests share: a temporary folder, a git repository, and the
//! sandbox this machine has.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use jadpo_verifier::sandbox::Sandbox;

/// A folder removed when dropped.
#[derive(Debug)]
pub struct Dir(PathBuf);

impl Dir {
    pub fn new(name: &str) -> Dir {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "jadpo-verifier-test-{name}-{}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("a folder");
        Dir(path.canonicalize().expect("a real path"))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// This machine's sandbox, or `None`, when a test has nothing to run in.
pub fn sandbox() -> Option<Sandbox> {
    match Sandbox::detect() {
        Ok(sandbox) => Some(sandbox),
        Err(why) => {
            assert!(
                std::env::var_os("JADPO_REQUIRE_SANDBOX").is_none(),
                "no sandbox on a machine that must have one: {why}"
            );
            None
        }
    }
}

pub fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args(["-c", "user.name=T", "-c", "user.email=t@example.com"])
        .args(args)
        .output()
        .expect("git");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

pub fn write(root: &Path, path: &str, text: impl AsRef<[u8]>) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("a folder");
    std::fs::write(path, text).expect("a file");
}

/// A repository holding `files`, committed, that a shallow fetch of a commit
/// can read; its head's commit.
pub fn repository(root: &Path, files: &[(&str, &str)]) -> String {
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["config", "uploadpack.allowAnySHA1InWant", "true"]);
    for (path, text) in files {
        write(root, path, text);
    }
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "a commit"]);
    git(root, &["rev-parse", "HEAD"])
}

/// Writes a script, runnable, that stands in for `jadpo`.
pub fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("a script");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("runnable");
    path
}
