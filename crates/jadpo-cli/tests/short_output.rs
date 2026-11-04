//! What a coding agent reads (chunk A13, from the 0.2.0 run): a test
//! database that can't start says so once, marked `(human)`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const JADPO: &str = env!("CARGO_BIN_EXE_jadpo");

fn folder(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("short-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temporary folder");
    dir
}

fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("folder");
    for entry in fs::read_dir(from).expect("read") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("copy");
        }
    }
}

/// `initdb` as a sandbox with no shared memory leaves it, as the 0.2.0 run
/// saw it, and the other tools a disposable server needs.
fn sandboxed_postgres(name: &str) -> PathBuf {
    let bin = folder(name);
    let initdb = "#!/bin/sh\ncat <<'END'\nThe files belonging to this database system will be owned by user \"someone\".\nThis user must also own the server process.\n\nselecting dynamic shared memory implementation ... sysv\nselecting default max_connections ... 20\nrunning bootstrap script ... 2026-11-01 21:54:11.133 GMT [20904] FATAL:  could not create shared memory segment: Operation not permitted\n2026-11-01 21:54:11.133 GMT [20904] DETAIL:  Failed system call was shmget(key=621383025, size=56, 03600).\nchild process exited with exit code 1\nEND\nexit 1\n";
    for (tool, script) in [
        ("initdb", initdb),
        ("pg_ctl", "#!/bin/sh\nexit 1\n"),
        ("postgres", "#!/bin/sh\nexit 1\n"),
    ] {
        let path = bin.join(tool);
        fs::write(&path, script).expect("tool");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("executable");
        }
    }
    bin
}

/// `jadpo test` in its own process, whose server is started from `bin`.
fn tests_with(bin: &Path, root: &Path, args: &[&str]) -> (i32, String) {
    let path = std::env::var("PATH").unwrap_or_default();
    let output = Command::new(JADPO)
        .arg("test")
        .args(args)
        .current_dir(root)
        .env("PATH", format!("{}:{path}", bin.display()))
        .env_remove("JADPO_TEST_DATABASE_URL")
        .env_remove("CI")
        .output()
        .expect("jadpo");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

/// A project with tables and action tests, copied from the examples.
fn stored_project(name: &str) -> PathBuf {
    let root = folder(name);
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/22-plant-helpline");
    copy(&example, &root);
    root
}

#[test]
fn a_test_database_that_cant_start_is_one_diagnostic_for_a_person() {
    let bin = sandboxed_postgres("sandbox-tools");
    let root = stored_project("sandbox-project");

    let (code, stdout) = tests_with(&bin, &root, &["--json"]);
    assert_eq!(code, 1, "{stdout}");
    let document: serde_json::Value = serde_json::from_str(&stdout).expect("document");
    assert_eq!(document["ok"], false);
    let diagnostics = document["diagnostics"].as_array().expect("diagnostics");
    assert_eq!(diagnostics.len(), 1, "{stdout}");
    let diagnostic = &diagnostics[0];
    assert_eq!(diagnostic["rule"], "test.database_unavailable");
    assert_eq!(diagnostic["next"]["decides"], "human");
    let next = diagnostic["next"]["text"].as_str().expect("next");
    assert!(next.contains("outside the sandbox"), "{next}");
    assert!(next.contains("JADPO_TEST_DATABASE_URL"), "{next}");
    let reason = diagnostic["reason"].as_str().expect("reason");
    assert!(
        reason.contains("could not create shared memory segment: Operation not permitted"),
        "{reason}"
    );
    assert!(!reason.contains("selecting default"), "{reason}");
    assert!(reason.contains("--full"), "{reason}");
    assert!(!stdout.contains("internal_fault"), "{stdout}");
    assert!(stdout.len() < 1500, "{} bytes: {stdout}", stdout.len());

    // All PostgreSQL said is there on request.
    let (_, full) = tests_with(&bin, &root, &["--json", "--full"]);
    let document: serde_json::Value = serde_json::from_str(&full).expect("document");
    let reason = document["diagnostics"][0]["reason"]
        .as_str()
        .expect("reason");
    assert!(
        reason.contains("selecting default max_connections"),
        "{reason}"
    );

    // One report on the screen too, and no test is said to have failed.
    let (code, screen) = tests_with(&bin, &root, &[]);
    assert_eq!(code, 1);
    assert_eq!(screen.matches("error:").count(), 1, "{screen}");
    assert!(!screen.contains("FAIL"), "{screen}");
    assert!(screen.contains("(human)"), "{screen}");
    assert!(
        screen.contains("rule: test.database_unavailable"),
        "{screen}"
    );
}
