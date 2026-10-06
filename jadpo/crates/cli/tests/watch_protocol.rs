#![cfg(unix)]

use serde_json::Value;
use std::fs;
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct WatchSession {
    child: Child,
    events: Receiver<Value>,
}

impl WatchSession {
    fn start(project: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_jadpo"))
            .arg("watch")
            .arg(project)
            .arg("--diagnostic-format=json")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("watch process should start");
        let stdout = child.stdout.take().expect("watch stdout should be piped");
        let (sender, events) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let line = line.expect("watch output should be readable");
                let event = serde_json::from_str(&line).expect("watch output should be JSON");
                if sender.send(event).is_err() {
                    break;
                }
            }
        });
        Self { child, events }
    }

    fn start_dev(project: &Path, port: u16, fake_bin: &Path, server: &Path) -> Self {
        let inherited_path = std::env::var_os("PATH").unwrap_or_default();
        let mut paths = vec![fake_bin.to_owned()];
        paths.extend(std::env::split_paths(&inherited_path));
        let path = std::env::join_paths(paths).expect("test PATH should be joinable");
        let mut child = Command::new(env!("CARGO_BIN_EXE_jadpo"))
            .arg("dev")
            .arg(project)
            .arg("--diagnostic-format=json")
            .env("PATH", path)
            .env("PORT", port.to_string())
            .env("TEST_RUNTIME_SERVER", server)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("dev process should start");
        let stdout = child.stdout.take().expect("dev stdout should be piped");
        let (sender, events) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let line = line.expect("dev output should be readable");
                let event = serde_json::from_str(&line).expect("dev output should be JSON");
                if sender.send(event).is_err() {
                    break;
                }
            }
        });
        Self { child, events }
    }

    fn start_dev_without_bun(project: &Path) -> Self {
        let empty_path = project.join("empty-path");
        fs::create_dir_all(&empty_path).expect("empty test PATH should be created");
        let mut child = Command::new(env!("CARGO_BIN_EXE_jadpo"))
            .arg("dev")
            .arg(project)
            .arg("--diagnostic-format=json")
            .env("PATH", empty_path)
            .env("PORT", "43123")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("dev process should start without Bun");
        let stdout = child.stdout.take().expect("dev stdout should be piped");
        let (sender, events) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let line = line.expect("dev output should be readable");
                let event = serde_json::from_str(&line).expect("dev output should be JSON");
                if sender.send(event).is_err() {
                    break;
                }
            }
        });
        Self { child, events }
    }

    fn wait_for(&self, event: &str, revision: u64) -> Value {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut observed = Vec::new();
        loop {
            let value = self
                .events
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap_or_else(|error| {
                    panic!(
                        "waiting for {event} revision {revision}: {error}; observed={observed:?}"
                    )
                });
            observed.push((value["event"].clone(), value["revision"].clone()));
            if value["event"] == event && value["revision"] == revision {
                return value;
            }
        }
    }

    fn interrupt_and_wait(mut self, revision: u64) -> Value {
        let status = Command::new("kill")
            .arg("-INT")
            .arg(self.child.id().to_string())
            .status()
            .expect("SIGINT command should run");
        assert!(status.success(), "SIGINT should be delivered");
        let shutdown = self.wait_for("shutdown", revision);
        let status = self.child.wait().expect("watch process should be waitable");
        assert!(status.success(), "watch should stop cleanly: {status}");
        shutdown
    }
}

impl Drop for WatchSession {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            // Let dev stop its runtime on assertion failure as well as normal exit.
            let _ = Command::new("kill")
                .arg("-INT")
                .arg(self.child.id().to_string())
                .status();
            let deadline = Instant::now() + Duration::from_secs(2);
            while Instant::now() < deadline {
                if self.child.try_wait().ok().flatten().is_some() {
                    return;
                }
                thread::sleep(Duration::from_millis(20));
            }
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn fixture_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should follow the Unix epoch")
        .as_nanos();
    let sequence = FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "jadpo-watch-protocol-{}-{nonce}-{sequence}",
        std::process::id()
    ))
}

fn unused_port() -> Option<u16> {
    TcpListener::bind(("127.0.0.1", 0))
        .ok()?
        .local_addr()
        .ok()
        .map(|address| address.port())
}

fn write_fake_bun(root: &Path) -> (PathBuf, PathBuf) {
    let bin = root.join("fake-bin");
    fs::create_dir_all(&bin).expect("fake binary directory should be created");
    let bun = bin.join("bun");
    fs::write(
        &bun,
        r#"#!/bin/sh
[ "$#" -eq 3 ] && [ "$1" = "--no-install" ] && [ "$2" = "--env-file=/dev/null" ] || exit 43
if grep -q BrokenCandidate "$3"; then exit 42; fi
exec python3 "$TEST_RUNTIME_SERVER"
"#,
    )
    .expect("fake Bun launcher should be written");
    let mut permissions = fs::metadata(&bun)
        .expect("fake Bun metadata should be readable")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&bun, permissions).expect("fake Bun should be executable");

    let server = root.join("fake_server.py");
    fs::write(
        &server,
        r#"import os
import socket
import selectors
import json

selector = selectors.DefaultSelector()
for plane, binding in [("application", "PORT"), ("deployment", "JADPO_DEPLOYMENT_PORT")]:
    server = socket.socket()
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.bind(("127.0.0.1", int(os.environ[binding])))
    server.listen()
    selector.register(server, selectors.EVENT_READ, plane)
while True:
    for key, _ in selector.select():
        connection, _ = key.fileobj.accept()
        request = connection.recv(4096)
        path = request.split(b" ")[1].decode()
        with open(os.path.join(os.path.dirname(__file__), "deployment-probes.jsonl"), "a") as output:
            output.write(json.dumps({"plane": key.data, "path": path, "port": key.fileobj.getsockname()[1]}) + "\n")
        ready = key.data == "deployment" and path == "/health/ready"
        body = b'{"status":"ready"}' if ready else b'{"error":{"code":"not_found"}}'
        status = b"200 OK" if ready else b"404 Not Found"
        connection.sendall(b"HTTP/1.1 " + status + b"\r\nContent-Type: application/json\r\nContent-Length: " + str(len(body)).encode() + b"\r\nConnection: close\r\n\r\n" + body)
        connection.close()
"#,
    )
    .expect("fake readiness server should be written");
    (bin, server)
}

#[test]
fn watch_protocol_covers_coalescing_failure_recovery_file_changes_and_shutdown() {
    let root = fixture_root();
    fs::create_dir_all(&root).expect("watch fixture should be created");
    let source = root.join("app.jadpo");
    fs::write(&source, "type Name = Text {}\n").expect("initial source should be written");
    let session = WatchSession::start(&root);

    let initial = session.wait_for("build_succeeded", 1);
    assert_eq!(initial["stale"], false);
    let initial_target = fs::read(root.join("build/target/app.ts"))
        .expect("initial generated target should be readable");

    fs::write(&source, "this is temporarily invalid\n").expect("rapid invalid edit should write");
    thread::sleep(Duration::from_millis(10));
    fs::write(&source, "type Name = Text { min_length: 1 }\n")
        .expect("rapid valid edit should write");
    let coalesced = session.wait_for("build_succeeded", 2);
    assert_eq!(coalesced["stale"], false);
    let coalesced_target = fs::read(root.join("build/target/app.ts"))
        .expect("coalesced generated target should be readable");

    fs::write(&source, "type Name = Text { min_length: }\n")
        .expect("invalid source should be written");
    let failed = session.wait_for("build_failed", 3);
    assert_eq!(failed["stale"], true);
    assert_eq!(
        fs::read(root.join("build/target/app.ts"))
            .expect("last-known-good target should remain readable"),
        coalesced_target
    );

    fs::write(&source, "type Name = Text { min_length: 2 }\n")
        .expect("recovered source should be written");
    let recovered = session.wait_for("build_succeeded", 4);
    assert_eq!(recovered["stale"], false);
    assert_ne!(
        fs::read(root.join("build/target/app.ts")).expect("recovered target should be readable"),
        initial_target
    );

    let extra = root.join("extra.jadpo");
    fs::write(&extra, "type Count = Int {}\n").expect("new source should be written");
    session.wait_for("build_succeeded", 5);
    fs::remove_file(extra).expect("new source should be removable");
    session.wait_for("build_succeeded", 6);

    let shutdown = session.interrupt_and_wait(6);
    assert_eq!(shutdown["status"], "stopped");
    assert_eq!(shutdown["stale"], false);
    fs::remove_dir_all(root).expect("watch fixture should be removable");
}

#[test]
fn dev_restores_the_ready_runtime_when_a_candidate_fails_startup() {
    assert!(Command::new("python3")
        .arg("--version")
        .status()
        .expect("dev rollback test requires python3")
        .success());
    let root = fixture_root();
    fs::create_dir_all(&root).expect("dev fixture should be created");
    let source = root.join("app.jadpo");
    fs::write(&source, "type Healthy = Text {}\n").expect("initial source should be written");
    let (fake_bin, server) = write_fake_bun(&root);
    let port =
        unused_port().expect("dev rollback test requires permission to bind a localhost socket");
    let session = WatchSession::start_dev(&root, port, &fake_bin, &server);

    let ready = session.wait_for("runtime_ready", 1);
    assert_eq!(ready["stale"], false);
    for line in fs::read_to_string(root.join("deployment-probes.jsonl")).expect("deployment probe trace should exist").lines() {
        let probe: Value = serde_json::from_str(line).expect("probe should be structured");
        assert_eq!(probe["plane"], "deployment");
        assert_eq!(probe["path"], "/health/ready");
        assert_ne!(probe["port"], port);
    }
    fs::write(&source, "type BrokenCandidate = Text {}\n")
        .expect("failing candidate should be written");
    session.wait_for("runtime_failed", 2);
    let restored = session.wait_for("runtime_rollback_succeeded", 2);
    assert_eq!(restored["status"], "ready");
    assert_eq!(restored["stale"], true);
    assert!(root.join("build/target/app.ts").is_file());
    assert!(!fs::read_to_string(root.join("build/target/app.ts"))
        .expect("restored target should be readable")
        .contains("BrokenCandidate"));

    let shutdown = session.interrupt_and_wait(2);
    assert_eq!(shutdown["status"], "stopped");
    assert!(std::net::TcpStream::connect(("127.0.0.1", port)).is_err());
    fs::remove_dir_all(root).expect("dev fixture should be removable");
}

#[test]
fn dev_reports_a_missing_bun_runtime_and_still_shuts_down_cleanly() {
    let root = fixture_root();
    fs::create_dir_all(&root).expect("missing-Bun fixture should be created");
    fs::write(root.join("app.jadpo"), "type Healthy = Text {}\n")
        .expect("source should be written");
    let session = WatchSession::start_dev_without_bun(&root);

    let failed = session.wait_for("runtime_failed", 1);
    assert_eq!(
        failed["diagnostics"][0]["legacyAliases"][0],
        "CLI_DEV_BUN_START_FAILED"
    );
    let shutdown = session.interrupt_and_wait(1);
    assert_eq!(shutdown["status"], "stopped");
    fs::remove_dir_all(root).expect("missing-Bun fixture should be removable");
}
