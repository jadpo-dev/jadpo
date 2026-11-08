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
        Dir::at(path)
    }

    /// The folder at `path`, made empty: one a stopped run left there, under
    /// a process id used again, holds a repository already committed.
    pub fn at(path: PathBuf) -> Dir {
        if path.exists() {
            std::fs::remove_dir_all(&path).expect("an earlier run's folder removed");
        }
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

/// Runs git in `root` as the tests need it, whoever runs them: no global or
/// system configuration (signing, hooks, a commit template), and none of a
/// calling git's own variables, such as a hook's `GIT_DIR`.
pub fn git(root: &Path, args: &[&str]) -> String {
    let mut command = Command::new("git");
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("GIT_") {
            command.env_remove(name);
        }
    }
    let output = command
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args([
            "-c",
            "user.name=T",
            "-c",
            "user.email=t@example.com",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "tag.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
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

/// A run that was stopped leaves its folders behind, and a later run whose
/// process has the same id is given the same names: each test's folder
/// starts empty all the same, so its repository's first commit has
/// something to commit.
#[test]
fn a_folder_left_by_an_earlier_run_starts_empty() {
    let left = Dir::new("left-behind");
    let path = left.path().to_path_buf();
    let committed = repository(&path, &[("package.jadpo", "package Postcodes.v2 {}\n")]);
    std::mem::forget(left);
    let again = Dir::at(path.clone());
    assert_eq!(
        std::fs::read_dir(again.path()).expect("a folder").count(),
        0
    );
    let commit = repository(
        again.path(),
        &[("package.jadpo", "package Postcodes.v2 {}\n")],
    );
    assert_eq!(commit.len(), committed.len());
}
