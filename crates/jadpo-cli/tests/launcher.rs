//! The built `jadpo` choosing which release runs a command (area 14,
//! "Which jadpo runs"; roadmap chunk D14): a project on this release's
//! line runs here, one on another line runs the release kept for it with
//! the same arguments and folder, `JADPO_TOOLCHAIN=local` runs everything
//! here, and `jadpo toolchain` lists, pins and removes. Fetching and
//! checking a release is tested against a stand-in for GitHub in the
//! crate's own tests (`src/launcher.rs`, `src/releases.rs`), so nothing
//! here reaches the network.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const JADPO: &str = env!("CARGO_BIN_EXE_jadpo");

/// A project, `shop/`, on `line`, and a cache folder beside it.
struct Fixture {
    dir: PathBuf,
}

impl Fixture {
    fn new(name: &str, line: &str) -> Fixture {
        let dir =
            std::env::temp_dir().join(format!("jadpo-launcher-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("shop/domains")).expect("a project");
        std::fs::write(
            dir.join("shop/app.jadpo"),
            format!("application {{\n    jadpo: \"{line}\"\n}}\n"),
        )
        .expect("app.jadpo");
        Fixture { dir }
    }

    fn project(&self) -> PathBuf {
        self.dir.join("shop")
    }

    /// Keeps release `version` in the cache: a script that says what it
    /// was started with.
    fn keep(&self, version: &str) {
        use std::os::unix::fs::PermissionsExt;
        let binary = self
            .dir
            .join(format!("cache/jadpo/releases/{version}/jadpo"));
        std::fs::create_dir_all(binary.parent().expect("a folder")).expect("kept");
        std::fs::write(
            &binary,
            format!(
                "#!/bin/sh\necho \"jadpo {version} ran: $* in $(pwd -P) launched=$JADPO_TOOLCHAIN_LAUNCHED\"\nexit 3\n"
            ),
        )
        .expect("kept");
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).expect("runs");
    }

    fn jadpo(&self, cwd: &Path, args: &[&str], vars: &[(&str, &str)]) -> Output {
        let mut command = Command::new(JADPO);
        command
            .args(args)
            .current_dir(cwd)
            .env_clear()
            .env("XDG_CACHE_HOME", self.dir.join("cache"));
        for (key, value) in vars {
            command.env(key, value);
        }
        command.output().expect("jadpo runs")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("UTF-8")
}

/// This jadpo's line.
fn own_line() -> String {
    let mut parts = env!("CARGO_PKG_VERSION").split('.');
    format!("{}.{}", parts.next().unwrap(), parts.next().unwrap())
}

#[test]
fn a_project_on_this_line_runs_here() {
    let fixture = Fixture::new("own", &own_line());
    fixture.keep("0.0.4");
    let output = fixture.jadpo(&fixture.project(), &["--version"], &[]);
    assert!(output.status.success());
    assert!(
        text(&output.stdout).contains(env!("CARGO_PKG_VERSION")),
        "{}",
        text(&output.stdout)
    );
}

#[test]
fn a_project_on_another_line_runs_its_release_with_the_same_arguments_and_folder() {
    let fixture = Fixture::new("other", "0.0");
    fixture.keep("0.0.2");
    fixture.keep("0.0.4");
    let folder = fixture.project().join("domains");
    let output = fixture.jadpo(&folder, &["check", "--json", "domains/x"], &[]);
    let folder = std::fs::canonicalize(&folder).expect("the folder");
    assert_eq!(
        text(&output.stdout),
        format!(
            "jadpo 0.0.4 ran: check --json domains/x in {} launched=0.0.4\n",
            folder.display()
        )
    );
    // Its exit code is the command's.
    assert_eq!(output.status.code(), Some(3));
}

#[test]
fn jadpo_toolchain_local_runs_it_here_and_the_checker_refuses_the_line() {
    let fixture = Fixture::new("local", "0.0");
    fixture.keep("0.0.4");
    let output = fixture.jadpo(
        &fixture.project(),
        &["check"],
        &[("JADPO_TOOLCHAIN", "local")],
    );
    assert_eq!(output.status.code(), Some(1));
    let stdout = text(&output.stdout);
    assert!(
        stdout.starts_with("error: This project needs Jadpo 0.0\n"),
        "{stdout}"
    );
    assert!(stdout.contains("rule: toolchain.version_older"), "{stdout}");
}

#[test]
fn toolchain_lists_and_removes_the_kept_releases_with_this_jadpo() {
    let fixture = Fixture::new("list", "0.0");
    fixture.keep("0.0.2");
    fixture.keep("0.0.4");
    let output = fixture.jadpo(&fixture.project(), &["toolchain", "--json"], &[]);
    assert!(output.status.success(), "{}", text(&output.stderr));
    let document: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON");
    assert_eq!(
        document["data"]["releases"],
        serde_json::json!(["0.0.4", "0.0.2"])
    );
    assert_eq!(document["data"]["running"], env!("CARGO_PKG_VERSION"));
    let output = fixture.jadpo(&fixture.project(), &["toolchain", "list"], &[]);
    let stdout = text(&output.stdout);
    assert!(stdout.contains("  0.0.4\n  0.0.2\n"), "{stdout}");
    assert!(stdout.contains("2 releases kept"), "{stdout}");
    let output = fixture.jadpo(&fixture.project(), &["toolchain", "remove", "0.0.4"], &[]);
    assert!(output.status.success(), "{}", text(&output.stderr));
    assert!(text(&output.stdout).contains("Removed Jadpo 0.0.4"));
    assert!(!fixture.dir.join("cache/jadpo/releases/0.0.4").exists());
    let output = fixture.jadpo(&fixture.project(), &["toolchain", "remove", "0.0.4"], &[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(text(&output.stderr).contains("Jadpo 0.0.4 isn't kept"));
}

#[test]
fn toolchain_pin_writes_the_running_release_into_the_lock() {
    let fixture = Fixture::new("pin", &own_line());
    let output = fixture.jadpo(&fixture.project(), &["toolchain", "pin"], &[]);
    assert!(output.status.success(), "{}", text(&output.stderr));
    let lock = std::fs::read_to_string(fixture.project().join("jadpo.lock")).expect("a lock");
    assert!(
        lock.contains(&format!("\njadpo version {}\n", env!("CARGO_PKG_VERSION"))),
        "{lock}"
    );
    // Pinned to this very release, it still runs here.
    let output = fixture.jadpo(&fixture.project(), &["--version"], &[]);
    assert!(output.status.success());
    let output = fixture.jadpo(&fixture.project(), &["toolchain", "unpin"], &[]);
    assert!(output.status.success(), "{}", text(&output.stderr));
    assert!(!fixture.project().join("jadpo.lock").exists());
}
