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
    };
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = || args.next().unwrap_or_else(|| panic!("{flag} needs a value"));
        match flag.as_str() {
            "--addr" => o.addr = value(),
            "--method" => o.method = value(),
            "--path" => o.path = value(),
            "--body" => o.body = value(),
            "--header" => o.headers.push(value()),
            "--connections" => o.connections = value().parse().expect("a count"),
            "--warmup" => o.warmup = value().parse().expect("seconds"),
            "--duration" => o.duration = value().parse().expect("seconds"),
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
    o
}

#[derive(Default)]
struct Tally {
    latencies: Vec<u32>,
    statuses: BTreeMap<u16, u64>,
    errors: u64,
}

/// One response read off `stream`: its status, and whether the server
/// closes the connection after it.
async fn response(stream: &mut TcpStream, buf: &mut Vec<u8>) -> std::io::Result<(u16, bool)> {
    let eof = || std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "closed");
    let head_end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        if stream.read_buf(buf).await? == 0 {
            return Err(eof());
        }
    };
    let head = std::str::from_utf8(&buf[..head_end]).unwrap_or("").to_ascii_lowercase();
    let status: u16 = head.get(9..12).and_then(|s| s.parse().ok()).unwrap_or(0);
    let close = head.contains("\r\nconnection: close");
    let length = head
        .split("\r\n")
        .find_map(|line| line.strip_prefix("content-length:"))
        .and_then(|v| v.trim().parse::<usize>().ok());
    let chunked = head.contains("\r\ntransfer-encoding: chunked");
    buf.drain(..head_end);
    if let Some(length) = length {
        while buf.len() < length {
            if stream.read_buf(buf).await? == 0 {
                return Err(eof());
            }
        }
        buf.drain(..length);
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
                std::str::from_utf8(&buf[..line_end]).unwrap_or("0").split(';').next().unwrap_or("0").trim(),
                16,
            )
            .unwrap_or(0);
            let need = line_end + 2 + size + 2;
            while buf.len() < need {
                if stream.read_buf(buf).await? == 0 {
                    return Err(eof());
                }
            }
            buf.drain(..need);
            if size == 0 {
                break;
            }
        }
    }
    Ok((status, close))
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
    let seed = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_micros() as u64);
    let counter = Arc::new(AtomicU64::new((seed % 4096) << 12));
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
                        match TcpStream::connect(&o.addr).await {
                            Ok(s) => {
                                let _ = s.set_nodelay(true);
                                buf.clear();
                                stream = Some(s);
                            }
                            Err(_) => {
                                tally.errors += 1;
                                tokio::time::sleep(Duration::from_millis(5)).await;
                                continue;
                            }
                        }
                    }
                    let n = counter.fetch_add(1, Ordering::Relaxed);
                    let ip = format!("10.{}.{}.{}", (n >> 16) & 255, (n >> 8) & 255, n & 255);
                    let mut request = format!("{} {} HTTP/1.1\r\nhost: bench\r\n", o.method, o.path);
                    for header in &o.headers {
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
                    let answered = match s.write_all(request.as_bytes()).await {
                        Ok(()) => response(s, &mut buf).await,
                        Err(e) => Err(e),
                    };
                    let done = Instant::now();
                    match answered {
                        Ok((status, close)) => {
                            if sent >= measure_from && done <= until {
                                tally.latencies.push(done.duration_since(sent).as_micros() as u32);
                                *tally.statuses.entry(status).or_default() += 1;
                            }
                            if close {
                                stream = None;
                            }
                        }
                        Err(_) => {
                            if sent >= measure_from {
                                tally.errors += 1;
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
    let mut errors = 0;
    for tally in tallies {
        latencies.extend(tally.latencies);
        for (status, count) in tally.statuses {
            *statuses.entry(status).or_default() += count;
        }
        errors += tally.errors;
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
    let statuses_json: Vec<String> = statuses.iter().map(|(s, c)| format!("\"{s}\":{c}")).collect();
    println!(
        "{{\"connections\":{},\"requests\":{},\"rps\":{:.1},\"p50_us\":{},\"p90_us\":{},\"p99_us\":{},\"max_us\":{},\"statuses\":{{{}}},\"unexpected\":{},\"errors\":{}}}",
        o.connections,
        latencies.len(),
        latencies.len() as f64 / o.duration,
        pct(50.0),
        pct(90.0),
        pct(99.0),
        latencies.last().copied().unwrap_or(0),
        statuses_json.join(","),
        unexpected,
        errors
    );
}
