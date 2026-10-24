//! A closed-loop HTTP/1.1 load generator for the performance spike
//! (tools/bench/README.md). Each of `--connections` keeps one keep-alive
//! connection and sends its next request as soon as the last is answered,
//! for `--warmup` seconds unmeasured and then `--duration` seconds
//! measured. `{ip}` in a header is replaced by an address of its own for
//! every request, so an app's per-address rate limit never refuses one.
//! It prints one JSON line: requests a second, latency percentiles in
//! microseconds, and the statuses seen.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

struct Case {
    headers: Vec<String>,
    owner: Option<String>,
    path: Option<String>,
    expected_json: Option<serde_json::Value>,
}

struct Options {
    addr: String,
    method: String,
    path: String,
    body: String,
    headers: Vec<String>,
    connections: usize,
    warmup: f64,
    duration: f64,
    expect: Vec<u16>,
    threads: usize,
    expected_json: Option<serde_json::Value>,
    created_title: Option<String>,
    created_todo_title: Option<String>,
    cases: Vec<Case>,
    address_pool: u64,
}

fn options() -> Options {
    let mut o = Options {
        addr: String::new(),
        method: "GET".into(),
        path: "/".into(),
        body: String::new(),
        headers: Vec::new(),
        connections: 1,
        warmup: 2.0,
        duration: 10.0,
        expect: vec![200],
        threads: 2,
        expected_json: None,
        created_title: None,
        created_todo_title: None,
        cases: Vec::new(),
        address_pool: 0,
    };
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = || {
            args.next()
                .unwrap_or_else(|| panic!("{flag} needs a value"))
        };
        match flag.as_str() {
            "--addr" => o.addr = value(),
            "--method" => o.method = value(),
            "--path" => o.path = value(),
            "--body" => o.body = value(),
            "--header" => o.headers.push(value()),
            "--address-pool" => o.address_pool = value().parse().expect("an address count"),
            "--connections" => o.connections = value().parse().expect("a count"),
            "--warmup" => o.warmup = value().parse().expect("seconds"),
            "--duration" => o.duration = value().parse().expect("seconds"),
            "--expect-json-file" => {
                o.expected_json = Some(
                    serde_json::from_str(&std::fs::read_to_string(value()).expect("a JSON file"))
                        .expect("JSON"),
                )
            }
            "--cases-file" => {
                let cases: serde_json::Value =
                    serde_json::from_str(&std::fs::read_to_string(value()).expect("a cases file"))
                        .expect("JSON");
                o.cases = cases
                    .as_array()
                    .expect("an array of cases")
                    .iter()
                    .map(|case| Case {
                        headers: case["headers"]
                            .as_array()
                            .expect("case headers")
                            .iter()
                            .map(|h| h.as_str().expect("a header").to_string())
                            .collect(),
                        path: case.get("path").and_then(|v| v.as_str()).map(str::to_owned),
                        owner: case
                            .get("owner")
                            .and_then(|v| v.as_str())
                            .map(str::to_owned),
                        expected_json: case.get("expected_json").cloned(),
                    })
                    .collect();
                assert!(!o.cases.is_empty(), "at least one case");
            }
            "--expect-created-title" => o.created_title = Some(value()),
            "--expect-created-todo-title" => o.created_todo_title = Some(value()),
            "--threads" => o.threads = value().parse().expect("a count"),
            "--expect" => {
                o.expect = value()
                    .split(',')
                    .map(|s| s.parse().expect("a status"))
                    .collect()
            }
            other => panic!("unknown flag {other}"),
        }
    }
    assert!(!o.addr.is_empty(), "--addr is required");
    assert!(
        o.connections > 0 && o.threads > 0,
        "positive connection and thread counts"
    );
    assert!(
        o.duration.is_finite() && o.duration > 0.0 && o.warmup.is_finite() && o.warmup >= 0.0,
        "finite positive duration and nonnegative warmup"
    );
    o
}

#[derive(Default)]
struct Tally {
    latencies: Vec<u32>,
    statuses: BTreeMap<u16, u64>,
    errors: u64,
    warmup_errors: u64,
    invalid_bodies: u64,
    warmup_invalid_bodies: u64,
    all_statuses: BTreeMap<u16, u64>,
    last_done: Option<Instant>,
    created_rows: Vec<serde_json::Value>,
}

/// One response read off `stream`: its status, and whether the server
/// closes the connection after it.
async fn response(
    stream: &mut TcpStream,
    buf: &mut Vec<u8>,
) -> std::io::Result<(u16, bool, Vec<u8>)> {
    let eof = || std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "closed");
    let head_end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        if stream.read_buf(buf).await? == 0 {
            return Err(eof());
        }
    };
    let head = std::str::from_utf8(&buf[..head_end])
        .unwrap_or("")
        .to_ascii_lowercase();
    let status: u16 = head.get(9..12).and_then(|s| s.parse().ok()).unwrap_or(0);
    let close = head.contains("\r\nconnection: close");
    let length = head
        .split("\r\n")
        .find_map(|line| line.strip_prefix("content-length:"))
        .and_then(|v| v.trim().parse::<usize>().ok());
    let chunked = head.contains("\r\ntransfer-encoding: chunked");
    buf.drain(..head_end);
    let mut body = Vec::new();
    if let Some(length) = length {
        while buf.len() < length {
            if stream.read_buf(buf).await? == 0 {
                return Err(eof());
            }
        }
        body.extend(buf.drain(..length));
    } else if chunked {
        loop {
            let line_end = loop {
                if let Some(i) = buf.windows(2).position(|w| w == b"\r\n") {
                    break i;
                }
                if stream.read_buf(buf).await? == 0 {
                    return Err(eof());
                }
            };
            let size = usize::from_str_radix(
                std::str::from_utf8(&buf[..line_end])
                    .unwrap_or("0")
                    .split(';')
                    .next()
                    .unwrap_or("0")
                    .trim(),
                16,
            )
            .unwrap_or(0);
            let need = line_end + 2 + size + 2;
            while buf.len() < need {
                if stream.read_buf(buf).await? == 0 {
                    return Err(eof());
                }
            }
            body.extend_from_slice(&buf[line_end + 2..line_end + 2 + size]);
            buf.drain(..need);
            if size == 0 {
                break;
            }
        }
    }
    Ok((status, close, body))
}

fn case_at(cases: &[Case], sequence: u64) -> Option<&Case> {
    if cases.is_empty() {
        None
    } else {
        Some(&cases[sequence as usize % cases.len()])
    }
}

fn valid_body(expected: Option<&serde_json::Value>, title: Option<&str>, body: &[u8]) -> bool {
    if expected.is_none() && title.is_none() {
        return true;
    }
    let Ok(actual) = serde_json::from_slice::<serde_json::Value>(body) else {
        return false;
    };
    if let Some(expected) = expected {
        return actual == *expected;
    }
    let Some(object) = actual.as_object() else {
        return false;
    };
    object.len() == 2
        && object.get("title").and_then(|v| v.as_str()) == title
        && object.get("id").and_then(|v| v.as_str()).is_some_and(|id| {
            id.len() == 36
                && id.as_bytes()[14] == b'7'
                && matches!(
                    id.as_bytes()[19].to_ascii_lowercase(),
                    b'8' | b'9' | b'a' | b'b'
                )
                && id.chars().enumerate().all(|(i, c)| {
                    if [8, 13, 18, 23].contains(&i) {
                        c == '-'
                    } else {
                        c.is_ascii_hexdigit()
                    }
                })
        })
}

// The complete golden response has six public fields. Keep the fixture's old
// two-field validator independent. Receipts are audited against PostgreSQL after
// draining, including the owner of the exact credential used for each request.
fn created_todo(title: &str, body: &[u8], owner: Option<&str>) -> Option<serde_json::Value> {
    let mut actual: serde_json::Value = serde_json::from_slice(body).ok()?;
    let object = actual.as_object_mut()?;
    if object.len() != 6
        || object.get("title")?.as_str()? != title
        || object.get("status")?.as_str()? != "open"
        || !object.get("due_at")?.is_null()
        || object.get("created_at")?.as_str()? != object.get("updated_at")?.as_str()?
    {
        return None;
    }
    let small = serde_json::json!({"id": object.get("id")?, "title": title});
    if !valid_body(None, Some(title), &serde_json::to_vec(&small).ok()?) {
        return None;
    }
    // Real timestamps and exact returned values are checked against persisted
    // fields outside the timed load. Empty or malformed values fail there too.
    object.insert("owner_id".into(), serde_json::Value::String(owner?.into()));
    Some(actual)
}

fn cpu_seconds() -> f64 {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // The operating system fills the whole rusage on success.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } != 0 {
        return 0.0;
    }
    let usage = unsafe { usage.assume_init() };
    usage.ru_utime.tv_sec as f64
        + usage.ru_utime.tv_usec as f64 / 1e6
        + usage.ru_stime.tv_sec as f64
        + usage.ru_stime.tv_usec as f64 / 1e6
}

fn main() {
    let o = Arc::new(options());
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(o.threads)
        .enable_all()
        .build()
        .expect("a runtime");
    // Each run's addresses start somewhere new, so runs a few seconds
    // apart never share one.
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_micros() as u64);
    let counter = Arc::new(AtomicU64::new((seed % 4096) << 12));
    let cpu_started = cpu_seconds();
    let started = Instant::now();
    let measure_from = started + Duration::from_secs_f64(o.warmup);
    let until = measure_from + Duration::from_secs_f64(o.duration);
    let tallies = runtime.block_on(async {
        let mut tasks = Vec::new();
        for _ in 0..o.connections {
            let o = o.clone();
            let counter = counter.clone();
            tasks.push(tokio::spawn(async move {
                let mut tally = Tally::default();
                let mut buf = Vec::with_capacity(16 * 1024);
                let mut stream: Option<TcpStream> = None;
                while Instant::now() < until {
                    if stream.is_none() {
                        match tokio::time::timeout(
                            Duration::from_secs(10),
                            TcpStream::connect(&o.addr),
                        )
                        .await
                        {
                            Ok(Ok(s)) => {
                                let _ = s.set_nodelay(true);
                                buf.clear();
                                stream = Some(s);
                            }
                            _ => {
                                if Instant::now() >= measure_from {
                                    tally.errors += 1;
                                } else {
                                    tally.warmup_errors += 1;
                                }
                                tokio::time::sleep(Duration::from_millis(5)).await;
                                continue;
                            }
                        }
                    }
                    if Instant::now() >= until {
                        break;
                    }
                    let n = counter.fetch_add(1, Ordering::Relaxed);
                    let case = case_at(&o.cases, n);
                    let address = if o.address_pool == 0 {
                        n
                    } else {
                        n % o.address_pool
                    };
                    let ip = format!(
                        "10.{}.{}.{}",
                        (address >> 16) & 255,
                        (address >> 8) & 255,
                        address & 255
                    );
                    let mut request = format!(
                        "{} {} HTTP/1.1\r\nhost: bench\r\n",
                        o.method,
                        case.and_then(|c| c.path.as_deref()).unwrap_or(&o.path)
                    );
                    for header in o
                        .headers
                        .iter()
                        .chain(case.into_iter().flat_map(|c| c.headers.iter()))
                    {
                        request.push_str(&header.replace("{ip}", &ip));
                        request.push_str("\r\n");
                    }
                    if !o.body.is_empty() {
                        request.push_str("content-type: application/json\r\n");
                        request.push_str(&format!("content-length: {}\r\n", o.body.len()));
                    }
                    request.push_str("\r\n");
                    request.push_str(&o.body);
                    let s = stream.as_mut().expect("connected");
                    let sent = Instant::now();
                    let answered = tokio::time::timeout(Duration::from_secs(30), async {
                        s.write_all(request.as_bytes()).await?;
                        response(s, &mut buf).await
                    })
                    .await;
                    let done = Instant::now();
                    match answered {
                        Ok(Ok((status, close, body))) => {
                            *tally.all_statuses.entry(status).or_default() += 1;
                            let mut valid = valid_body(
                                case.and_then(|c| c.expected_json.as_ref())
                                    .or(o.expected_json.as_ref()),
                                o.created_title.as_deref(),
                                &body,
                            );
                            if let Some(title) = &o.created_todo_title {
                                if let Some(receipt) = created_todo(
                                    title,
                                    &body,
                                    case.and_then(|c| c.owner.as_deref()),
                                ) {
                                    tally.created_rows.push(receipt);
                                } else {
                                    valid = false;
                                }
                            }
                            if sent >= measure_from {
                                if !valid {
                                    tally.invalid_bodies += 1;
                                }
                                tally.last_done = Some(done);
                                tally
                                    .latencies
                                    .push(done.duration_since(sent).as_micros() as u32);
                                *tally.statuses.entry(status).or_default() += 1;
                            } else if !valid {
                                tally.warmup_invalid_bodies += 1;
                            }
                            if close {
                                stream = None;
                            }
                        }
                        _ => {
                            if sent >= measure_from {
                                tally.errors += 1;
                            } else {
                                tally.warmup_errors += 1;
                            }
                            stream = None;
                        }
                    }
                }
                tally
            }));
        }
        let mut all = Vec::new();
        for task in tasks {
            all.push(task.await.expect("a connection's task"));
        }
        all
    });
    let mut latencies: Vec<u32> = Vec::new();
    let mut statuses: BTreeMap<u16, u64> = BTreeMap::new();
    let mut all_statuses: BTreeMap<u16, u64> = BTreeMap::new();
    let (mut errors, mut warmup_errors, mut invalid_bodies, mut warmup_invalid_bodies) =
        (0, 0, 0, 0);
    let mut last_done = until;
    let mut created_rows = Vec::new();
    for tally in tallies {
        latencies.extend(tally.latencies);
        created_rows.extend(tally.created_rows);
        for (status, count) in tally.statuses {
            *statuses.entry(status).or_default() += count;
        }
        for (status, count) in tally.all_statuses {
            *all_statuses.entry(status).or_default() += count;
        }
        errors += tally.errors;
        warmup_errors += tally.warmup_errors;
        invalid_bodies += tally.invalid_bodies;
        warmup_invalid_bodies += tally.warmup_invalid_bodies;
        last_done = last_done.max(tally.last_done.unwrap_or(until));
    }
    latencies.sort_unstable();
    let pct = |p: f64| -> u32 {
        if latencies.is_empty() {
            return 0;
        }
        let i = ((p / 100.0) * latencies.len() as f64).ceil() as usize;
        latencies[i.saturating_sub(1).min(latencies.len() - 1)]
    };
    let unexpected: u64 = statuses
        .iter()
        .filter(|(s, _)| !o.expect.contains(s))
        .map(|(_, c)| c)
        .sum();
    let all_unexpected: u64 = all_statuses
        .iter()
        .filter(|(s, _)| !o.expect.contains(s))
        .map(|(_, c)| c)
        .sum();
    let elapsed = last_done.duration_since(measure_from).as_secs_f64();
    println!(
        "{}",
        serde_json::json!({
            "schemaVersion": 3, "created_rows": created_rows, "connections": o.connections,
            "load_cpu_s": cpu_seconds() - cpu_started, "load_wall_s": started.elapsed().as_secs_f64(), "load_threads": o.threads, "requests": latencies.len(),
            "rps": latencies.len() as f64 / elapsed, "p50_us": pct(50.0),
            "p90_us": pct(90.0), "p95_us": pct(95.0), "p99_us": pct(99.0),
            "max_us": latencies.last().copied().unwrap_or(0), "statuses": statuses,
            "all_statuses": all_statuses, "unexpected": unexpected, "errors": errors,
            "invalid_bodies": invalid_bodies, "warmup_errors": warmup_errors,
            "warmup_invalid_bodies": warmup_invalid_bodies, "all_unexpected": all_unexpected,
            "elapsed_s": elapsed, "drain_s": (elapsed - o.duration).max(0.0),
        })
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rotating_cases_keep_each_credential_with_its_own_response() {
        let cases = [
            Case {
                headers: vec!["authorization: Bearer alice".into()],
                owner: None,
                path: None,
                expected_json: Some(serde_json::json!([{"id": "alice-row"}])),
            },
            Case {
                headers: vec!["authorization: Bearer bob".into()],
                owner: None,
                path: None,
                expected_json: Some(serde_json::json!([{"id": "bob-row"}])),
            },
        ];
        assert!(case_at(&[], 0).is_none());
        for sequence in [0, 2, 100] {
            let alice = case_at(&cases, sequence).unwrap();
            assert_eq!(alice.headers[0], "authorization: Bearer alice");
            assert!(valid_body(
                alice.expected_json.as_ref(),
                None,
                br#"[{"id":"alice-row"}]"#
            ));
            assert!(!valid_body(
                alice.expected_json.as_ref(),
                None,
                br#"[{"id":"bob-row"}]"#
            ));
        }
        assert_eq!(
            case_at(&cases, 1).unwrap().headers[0],
            "authorization: Bearer bob"
        );
    }
    #[test]
    fn a_corrupt_200_body_does_not_count_as_equivalent_work() {
        let expected = serde_json::json!({"payload": "correct"});
        assert!(valid_body(
            Some(&expected),
            None,
            br#"{"payload":"correct"}"#
        ));
        assert!(!valid_body(
            Some(&expected),
            None,
            br#"{"payload":"wrong"}"#
        ));
        assert!(!valid_body(Some(&expected), None, b""));
    }
    #[test]
    fn a_write_must_answer_its_title_and_a_generated_identity() {
        assert!(valid_body(
            None,
            Some("task"),
            br#"{"id":"11111111-1111-7111-8111-111111111111","title":"task"}"#
        ));
        assert!(!valid_body(
            None,
            Some("task"),
            br#"{"id":"bad","title":"task"}"#
        ));
        assert!(!valid_body(
            None,
            Some("task"),
            br#"{"id":"11111111-1111-7111-8111-111111111111","title":"other"}"#
        ));
        assert!(!valid_body(
            None,
            Some("task"),
            br#"{"id":"11111111-1111-7111-1111-111111111111","title":"task"}"#
        ));
    }
    #[test]
    fn golden_writes_collect_complete_receipts_for_the_credential_owner() {
        let response = serde_json::json!({
            "id": "11111111-1111-7111-8111-111111111111", "title": "task",
            "status": "open", "due_at": null,
            "created_at": "2026-10-23T12:00:00.000Z", "updated_at": "2026-10-23T12:00:00.000Z"
        });
        let body = serde_json::to_vec(&response).unwrap();
        assert_eq!(
            created_todo("task", &body, Some("alice")).unwrap()["owner_id"],
            "alice"
        );
        assert!(created_todo("other", &body, Some("alice")).is_none());
        assert!(created_todo("task", &body, None).is_none());
        for (key, value) in [
            ("status", serde_json::json!("done")),
            ("due_at", serde_json::json!("wrong")),
            ("id", serde_json::json!("wrong")),
            ("updated_at", serde_json::json!("different")),
            ("owner_id", serde_json::json!("leaked")),
        ] {
            let mut corrupt = response.clone();
            corrupt[key] = value;
            assert!(
                created_todo(
                    "task",
                    &serde_json::to_vec(&corrupt).unwrap(),
                    Some("alice")
                )
                .is_none()
            );
        }
    }
    #[tokio::test]
    async fn responses_return_the_whole_content_length_body() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let send = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            socket
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 6\r\n\r\nabc")
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(20)).await;
            socket.write_all(b"def").await.unwrap();
        });
        let mut socket = TcpStream::connect(addr).await.unwrap();
        let (status, _, body) = response(&mut socket, &mut Vec::new()).await.unwrap();
        assert_eq!(status, 200);
        assert_eq!(body, b"abcdef");
        send.await.unwrap();
    }
}
