//! Test the actual load executable against a deliberately slow/corrupt server.
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::time::Duration;

fn serve(body: &'static [u8], delay: Duration) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let thread = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = [0; 4096];
        let _ = socket.read(&mut request).unwrap();
        std::thread::sleep(delay);
        write!(
            socket,
            "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            body.len()
        )
        .unwrap();
        socket.write_all(body).unwrap();
    });
    (address, thread)
}

#[test]
fn a_request_started_in_the_window_keeps_its_slow_tail_in_p95() {
    let (address, server) = serve(b"{}", Duration::from_millis(90));
    let result = Command::new(env!("CARGO_BIN_EXE_load"))
        .args([
            "--addr",
            &address,
            "--duration",
            "0.03",
            "--warmup",
            "0",
            "--connections",
            "1",
        ])
        .output()
        .unwrap();
    server.join().unwrap();
    assert!(result.status.success());
    let row: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(row["requests"], 1);
    assert_eq!(row["errors"], 0);
    assert_eq!(row["all_statuses"]["200"], 1);
    assert!(row["p95_us"].as_u64().unwrap() >= 80_000);
    assert!(row["drain_s"].as_f64().unwrap() >= 0.05);
    assert!(row["elapsed_s"].as_f64().unwrap() >= 0.08);
}

#[test]
fn a_corrupted_success_is_reported_by_the_real_load_process() {
    let (address, server) = serve(
        b"{\"id\":\"wrong\",\"title\":\"wrong\"}",
        Duration::from_millis(90),
    );
    let result = Command::new(env!("CARGO_BIN_EXE_load"))
        .args([
            "--addr",
            &address,
            "--duration",
            "0.03",
            "--warmup",
            "0",
            "--connections",
            "1",
            "--expect-created-title",
            "task",
        ])
        .output()
        .unwrap();
    server.join().unwrap();
    assert!(result.status.success());
    let row: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(row["invalid_bodies"], 1);
    assert_eq!(row["unexpected"], 0);
    assert_eq!(row["all_statuses"]["200"], 1);
}
