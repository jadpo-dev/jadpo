//! Exercise the load executable with bounded slow/corrupt local servers.
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn serve(body: &'static [u8]) -> (String, std::thread::JoinHandle<Result<(), String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let thread = std::thread::spawn(move || {
        let until = Instant::now() + Duration::from_secs(5);
        let mut socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < until =>
                {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => return Err(format!("no load connection before deadline: {error}")),
            }
        };
        // macOS can inherit the listener's O_NONBLOCK flag on accept.
        // Timeouts do not clear it: a scheduled-later client must be allowed
        // to send its request before this bounded blocking read returns.
        socket.set_nonblocking(false).map_err(|e| e.to_string())?;
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .map_err(|e| e.to_string())?;
        socket
            .set_write_timeout(Some(Duration::from_secs(2)))
            .map_err(|e| e.to_string())?;
        let mut request = Vec::new();
        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
            let mut part = [0; 4096];
            let read = socket.read(&mut part).map_err(|e| e.to_string())?;
            if read == 0 {
                return Err("load closed before sending a request".into());
            }
            request.extend_from_slice(&part[..read]);
        }
        // The response finishes past a generous one-second admission window.
        // A busy gate can delay startup without relying on a 30 ms schedule.
        std::thread::sleep(Duration::from_millis(1200));
        write!(
            socket,
            "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            body.len()
        )
        .map_err(|e| e.to_string())?;
        socket.write_all(body).map_err(|e| e.to_string())
    });
    (address, thread)
}

fn load(args: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_load"))
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if Instant::now() >= until {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            panic!(
                "load did not finish before its test deadline: {:?}",
                output.status
            );
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_request_started_in_the_window_keeps_its_slow_tail_in_p95() {
    let (address, server) = serve(b"{}");
    let result = load(&[
        "--addr",
        &address,
        "--duration",
        "1",
        "--warmup",
        "0",
        "--connections",
        "1",
    ]);
    server.join().unwrap().unwrap();
    assert!(result.status.success());
    let row: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(row["requests"], 1);
    assert_eq!(row["errors"], 0);
    assert_eq!(row["all_statuses"]["200"], 1);
    assert!(row["p95_us"].as_u64().unwrap() >= 1_100_000);
    assert!(row["drain_s"].as_f64().unwrap() >= 0.1);
    assert!(row["elapsed_s"].as_f64().unwrap() >= 1.1);
}

#[test]
fn a_corrupted_success_is_reported_by_the_real_load_process() {
    let (address, server) = serve(b"{\"id\":\"wrong\",\"title\":\"wrong\"}");
    let result = load(&[
        "--addr",
        &address,
        "--duration",
        "1",
        "--warmup",
        "0",
        "--connections",
        "1",
        "--expect-created-title",
        "task",
    ]);
    server.join().unwrap().unwrap();
    assert!(result.status.success());
    let row: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(row["invalid_bodies"], 1);
    assert_eq!(row["unexpected"], 0);
    assert_eq!(row["all_statuses"]["200"], 1);
}
