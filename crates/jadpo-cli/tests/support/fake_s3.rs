//! A local S3-compatible service for the S3 driver's tests: a localhost
//! HTTP server on its own thread holding one bucket in memory, refusing
//! any request whose Signature Version 4 doesn't verify against its one
//! credential, presigned URLs included, as S3 does. It implements what the
//! driver sends: objects (`PUT`, `GET` with ranges, `HEAD`, `DELETE`,
//! server-side copy and `If-None-Match`), multipart uploads with R2's rule
//! that every part but the last is the same size, and `ListObjectsV2`. It
//! never reaches a network.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use jadpo_runtime::sigv4;

/// One stored object.
#[derive(Clone, Debug)]
pub struct Object {
    pub bytes: Vec<u8>,
    pub content_type: Option<String>,
    pub content_disposition: Option<String>,
    pub content_encoding: Option<String>,
    pub etag: String,
}

#[derive(Default)]
struct State {
    bucket: String,
    key_id: String,
    secret: String,
    objects: BTreeMap<String, Object>,
    uploads: BTreeMap<String, Upload>,
    next_upload: u64,
    seen: Vec<String>, // each request as METHOD target
    unverified: u64,
}

struct Upload {
    key: String,
    content_type: Option<String>,
    parts: BTreeMap<u64, (String, Vec<u8>)>,
}

/// A running service, stopped when dropped.
pub struct FakeS3 {
    pub port: u16,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

/// What a client got back: status, headers (lowercase), body.
pub type Got = (u16, Vec<(String, String)>, Vec<u8>);

impl FakeS3 {
    /// A service holding `bucket`, signed with `key_id` and `secret`.
    pub fn start(bucket: &str, key_id: &str, secret: &str) -> FakeS3 {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        let port = listener.local_addr().expect("a port").port();
        listener.set_nonblocking(true).expect("nonblocking");
        let state = Arc::new(Mutex::new(State {
            bucket: bucket.to_string(),
            key_id: key_id.to_string(),
            secret: secret.to_string(),
            ..State::default()
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let (thread_state, thread_stop) = (Arc::clone(&state), Arc::clone(&stop));
        let thread = std::thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let state = Arc::clone(&thread_state);
                        std::thread::spawn(move || {
                            let _ = stream.set_nonblocking(false);
                            let _ = serve(stream, &state);
                        });
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(5)),
                }
            }
        });
        FakeS3 {
            port,
            state,
            stop,
            thread: Some(thread),
        }
    }

    /// `http://127.0.0.1:<port>`, what a connection's `host:` is given.
    pub fn address(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// The keys of every object, in order.
    pub fn keys(&self) -> Vec<String> {
        self.state
            .lock()
            .expect("state")
            .objects
            .keys()
            .cloned()
            .collect()
    }

    /// A stored object's contents.
    pub fn object(&self, key: &str) -> Option<Object> {
        self.state.lock().expect("state").objects.get(key).cloned()
    }

    /// How many multipart uploads are still under way.
    pub fn open_uploads(&self) -> usize {
        self.state.lock().expect("state").uploads.len()
    }

    /// How many requests failed their signature check.
    pub fn unverified(&self) -> u64 {
        self.state.lock().expect("state").unverified
    }
}

impl Drop for FakeS3 {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// One request read from a connection.
struct Incoming {
    method: String,
    /// `path?query` as sent.
    target: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Incoming {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header, _)| header == name)
            .map(|(_, value)| value.as_str())
    }

    fn path(&self) -> &str {
        self.target.split('?').next().unwrap_or("")
    }

    fn query(&self) -> Vec<(String, String)> {
        match self.target.split_once('?') {
            Some((_, query)) => parse_query(query),
            None => Vec::new(),
        }
    }

    fn query_value(&self, name: &str) -> Option<String> {
        self.query()
            .into_iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
    }
}

fn parse_query(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            (decode(name), decode(value))
        })
        .collect()
}

fn decode(text: &str) -> String {
    let mut out = Vec::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%'
            && at + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&text[at + 1..at + 3], 16)
        {
            out.push(byte);
            at += 3;
            continue;
        }
        out.push(bytes[at]);
        at += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn read_request(stream: &mut TcpStream) -> std::io::Result<Option<Incoming>> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut first = String::new();
    if reader.read_line(&mut first)? == 0 {
        return Ok(None);
    }
    let mut words = first.split_whitespace();
    let (Some(method), Some(target)) = (words.next(), words.next()) else {
        return Ok(None);
    };
    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    let length = headers
        .iter()
        .find(|(name, _)| name == "content-length")
        .and_then(|(_, value)| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    Ok(Some(Incoming {
        method: method.to_string(),
        target: target.to_string(),
        headers,
        body,
    }))
}

fn serve(mut stream: TcpStream, state: &Mutex<State>) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(60)))?;
    let Some(request) = read_request(&mut stream)? else {
        return Ok(());
    };
    let (status, headers, body) = handle(&request, state);
    let mut head = format!(
        "HTTP/1.1 {status} S\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (name, value) in &headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    if request.method != "HEAD" {
        stream.write_all(&body)?;
    }
    stream.flush()
}

fn error(status: u16, code: &str) -> Got {
    (
        status,
        vec![("content-type".into(), "application/xml".into())],
        format!("<Error><Code>{code}</Code></Error>").into_bytes(),
    )
}

/// Whether `request` was signed with the bucket's credential.
fn verified(request: &Incoming, state: &State) -> bool {
    let Some(host) = request.header("host") else {
        return false;
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as i64);
    if let Some(signature) = request.query_value("X-Amz-Signature") {
        // A presigned URL: its query without the X-Amz- parameters, signed
        // again and compared.
        let Some(credential) = request.query_value("X-Amz-Credential") else {
            return false;
        };
        let mut parts = credential.split('/');
        let (Some(key_id), Some(_date), Some(region), Some(service)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return false;
        };
        if key_id != state.key_id {
            return false;
        }
        let (Some(stamp), Some(expires)) = (
            request.query_value("X-Amz-Date"),
            request
                .query_value("X-Amz-Expires")
                .and_then(|text| text.parse::<u64>().ok()),
        ) else {
            return false;
        };
        let Some(at) = super::fake_sqs::date_ms(&stamp) else {
            return false;
        };
        if at + expires as i64 * 1000 < now {
            return false;
        }
        let base = request.target.split('?').next().unwrap_or("");
        let original: Vec<&str> = request
            .target
            .split_once('?')
            .map_or("", |(_, query)| query)
            .split('&')
            .filter(|pair| !pair.starts_with("X-Amz-"))
            .collect();
        let url = if original.is_empty() {
            format!("http://{host}{base}")
        } else {
            format!("http://{host}{base}?{}", original.join("&"))
        };
        let signer = sigv4::Signer {
            key_id,
            secret: &state.secret,
            region,
            service,
        };
        return sigv4::presign(&request.method, &url, &signer, at, expires)
            .is_some_and(|signed| signed.ends_with(&format!("X-Amz-Signature={signature}")));
    }
    let Some(authorization) = request.header("authorization") else {
        return false;
    };
    let Some(stamp) = request.header("x-amz-date") else {
        return false;
    };
    let Some(at) = super::fake_sqs::date_ms(stamp) else {
        return false;
    };
    let Some(content) = request.header("x-amz-content-sha256") else {
        return false;
    };
    // A body that is hashed must be the one signed.
    if content != "UNSIGNED-PAYLOAD" && hex(&jadpo_value::hash::sha256(&request.body)) != content {
        return false;
    }
    let Some(credential) = authorization
        .split("Credential=")
        .nth(1)
        .and_then(|rest| rest.split(',').next())
    else {
        return false;
    };
    let mut parts = credential.split('/');
    let (Some(key_id), Some(_date), Some(region), Some(service)) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    if key_id != state.key_id {
        return false;
    }
    let Some(signed) = authorization
        .split("SignedHeaders=")
        .nth(1)
        .and_then(|rest| rest.split(',').next())
    else {
        return false;
    };
    let given: Vec<(String, String)> = signed
        .split(';')
        .filter(|name| *name != "host" && *name != "x-amz-date")
        .filter_map(|name| Some((name.to_string(), request.header(name)?.to_string())))
        .collect();
    let url = format!("http://{host}{}", request.target);
    let unsigned = sigv4::Unsigned {
        method: &request.method,
        url: &url,
        headers: &given,
        body: &request.body,
        payload: Some(content),
    };
    let signer = sigv4::Signer {
        key_id,
        secret: &state.secret,
        region,
        service,
    };
    sigv4::sign(&unsigned, &signer, at).is_some_and(|[_, (_, expected)]| expected == authorization)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn etag_of(bytes: &[u8]) -> String {
    format!("\"{}\"", hex(&jadpo_value::hash::sha256(bytes)))
}

fn handle(request: &Incoming, shared: &Mutex<State>) -> Got {
    let mut state = shared.lock().expect("state");
    state
        .seen
        .push(format!("{} {}", request.method, request.target));
    if !verified(request, &state) {
        state.unverified += 1;
        return error(403, "SignatureDoesNotMatch");
    }
    let prefix = format!("/{}", state.bucket);
    let Some(rest) = request.path().strip_prefix(&prefix) else {
        return error(404, "NoSuchBucket");
    };
    let key = decode(rest.trim_start_matches('/'));
    let query = request.query();
    let has = |name: &str| query.iter().any(|(key, _)| key == name);
    let value = |name: &str| {
        query
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    };
    match request.method.as_str() {
        "GET" | "HEAD" => {
            if has("list-type") {
                let wanted = value("prefix").unwrap_or_default();
                let mut xml = String::from("<ListBucketResult><IsTruncated>false</IsTruncated>");
                for object in state.objects.keys().filter(|key| key.starts_with(&wanted)) {
                    xml.push_str(&format!("<Contents><Key>{object}</Key></Contents>"));
                }
                xml.push_str("</ListBucketResult>");
                return (200, Vec::new(), xml.into_bytes());
            }
            if let Some(id) = value("uploadId") {
                let Some(upload) = state.uploads.get(&id).filter(|upload| upload.key == key) else {
                    return error(404, "NoSuchUpload");
                };
                let mut xml = String::from("<ListPartsResult><IsTruncated>false</IsTruncated>");
                for (number, (etag, bytes)) in &upload.parts {
                    xml.push_str(&format!(
                        "<Part><PartNumber>{number}</PartNumber><ETag>{}</ETag><Size>{}</Size></Part>",
                        etag.replace('"', "&quot;"),
                        bytes.len()
                    ));
                }
                xml.push_str("</ListPartsResult>");
                return (200, Vec::new(), xml.into_bytes());
            }
            let Some(object) = state.objects.get(&key) else {
                return error(404, "NoSuchKey");
            };
            let mut headers = vec![
                ("etag".to_string(), object.etag.clone()),
                ("accept-ranges".to_string(), "bytes".to_string()),
            ];
            let content_type = value("response-content-type")
                .or_else(|| object.content_type.clone())
                .unwrap_or_else(|| "binary/octet-stream".to_string());
            headers.push(("content-type".to_string(), content_type));
            let disposition = value("response-content-disposition")
                .or_else(|| object.content_disposition.clone());
            if let Some(disposition) = disposition {
                headers.push(("content-disposition".to_string(), disposition));
            }
            if let Some(encoding) = &object.content_encoding {
                headers.push(("content-encoding".to_string(), encoding.clone()));
            }
            let total = object.bytes.len();
            let Some(range) = request
                .header("range")
                .and_then(|range| range.strip_prefix("bytes="))
            else {
                return (200, headers, object.bytes.clone());
            };
            let (first, last) = range.split_once('-').unwrap_or((range, ""));
            let from: usize = first.parse().unwrap_or(0);
            if total == 0 || from >= total {
                headers.push(("content-range".to_string(), format!("bytes */{total}")));
                return (416, headers, Vec::new());
            }
            let end = last
                .parse::<usize>()
                .map_or(total - 1, |end| end.min(total - 1));
            headers.push((
                "content-range".to_string(),
                format!("bytes {from}-{end}/{total}"),
            ));
            (206, headers, object.bytes[from..=end].to_vec())
        }
        "PUT" => {
            if let Some(source) = request.header("x-amz-copy-source") {
                let source_key = decode(source.trim_start_matches('/'));
                let Some(source_key) = source_key.strip_prefix(&format!("{}/", state.bucket))
                else {
                    return error(404, "NoSuchKey");
                };
                let Some(object) = state.objects.get(source_key).cloned() else {
                    return error(404, "NoSuchKey");
                };
                if request
                    .header("x-amz-copy-source-if-match")
                    .is_some_and(|etag| etag != object.etag)
                {
                    return error(412, "PreconditionFailed");
                }
                state.objects.insert(key, object.clone());
                let xml = format!(
                    "<CopyObjectResult><ETag>{}</ETag></CopyObjectResult>",
                    object.etag.replace('"', "&quot;")
                );
                return (200, Vec::new(), xml.into_bytes());
            }
            if let (Some(number), Some(id)) = (value("partNumber"), value("uploadId")) {
                let Ok(number) = number.parse::<u64>() else {
                    return error(400, "InvalidPart");
                };
                let etag = etag_of(&request.body);
                let Some(upload) = state
                    .uploads
                    .get_mut(&id)
                    .filter(|upload| upload.key == key)
                else {
                    return error(404, "NoSuchUpload");
                };
                upload
                    .parts
                    .insert(number, (etag.clone(), request.body.clone()));
                return (200, vec![("etag".to_string(), etag)], Vec::new());
            }
            if request.header("if-none-match") == Some("*") && state.objects.contains_key(&key) {
                return error(412, "PreconditionFailed");
            }
            let etag = etag_of(&request.body);
            state.objects.insert(
                key,
                Object {
                    bytes: request.body.clone(),
                    content_type: request.header("content-type").map(str::to_string),
                    content_disposition: request.header("content-disposition").map(str::to_string),
                    content_encoding: request.header("content-encoding").map(str::to_string),
                    etag: etag.clone(),
                },
            );
            (200, vec![("etag".to_string(), etag)], Vec::new())
        }
        "POST" => {
            if has("uploads") {
                state.next_upload += 1;
                let id = format!("upload-{}", state.next_upload);
                state.uploads.insert(
                    id.clone(),
                    Upload {
                        key,
                        content_type: request.header("content-type").map(str::to_string),
                        parts: BTreeMap::new(),
                    },
                );
                let xml = format!(
                    "<InitiateMultipartUploadResult><UploadId>{id}</UploadId></InitiateMultipartUploadResult>"
                );
                return (200, Vec::new(), xml.into_bytes());
            }
            if let Some(id) = value("uploadId") {
                let Some(upload) = state.uploads.remove(&id).filter(|upload| upload.key == key)
                else {
                    return error(404, "NoSuchUpload");
                };
                let text = String::from_utf8_lossy(&request.body).into_owned();
                let mut bytes = Vec::new();
                let mut sizes = Vec::new();
                for block in text.split("<Part>").skip(1) {
                    let number: u64 = block
                        .split("<PartNumber>")
                        .nth(1)
                        .and_then(|rest| rest.split("</PartNumber>").next())
                        .and_then(|number| number.parse().ok())
                        .unwrap_or(0);
                    let etag = block
                        .split("<ETag>")
                        .nth(1)
                        .and_then(|rest| rest.split("</ETag>").next())
                        .unwrap_or_default()
                        .replace("&quot;", "\"");
                    let Some((stored, part)) = upload.parts.get(&number) else {
                        return error(400, "InvalidPart");
                    };
                    if *stored != etag {
                        return error(400, "InvalidPart");
                    }
                    sizes.push(part.len());
                    bytes.extend_from_slice(part);
                }
                // R2's rule: every part but the last the same size.
                if let Some((_, before)) = sizes.split_last()
                    && before.windows(2).any(|pair| pair[0] != pair[1])
                {
                    return error(400, "InvalidPart");
                }
                let etag = etag_of(&bytes);
                state.objects.insert(
                    key,
                    Object {
                        bytes,
                        content_type: upload.content_type.clone(),
                        content_disposition: None,
                        content_encoding: None,
                        etag: etag.clone(),
                    },
                );
                let xml = format!(
                    "<CompleteMultipartUploadResult><ETag>{}</ETag></CompleteMultipartUploadResult>",
                    etag.replace('"', "&quot;")
                );
                return (200, Vec::new(), xml.into_bytes());
            }
            error(400, "InvalidRequest")
        }
        "DELETE" => {
            if let Some(id) = value("uploadId") {
                state.uploads.remove(&id);
                return (204, Vec::new(), Vec::new());
            }
            state.objects.remove(&key);
            (204, Vec::new(), Vec::new())
        }
        _ => error(405, "MethodNotAllowed"),
    }
}

/// A client's request to `url`, an `http://` address on this machine:
/// what a browser or a mobile client sends a signed address.
pub fn exchange(url: &str, method: &str, headers: &[(&str, &str)], body: &[u8]) -> Got {
    let rest = url.strip_prefix("http://").expect("an http:// address");
    let (authority, target) = rest.split_once('/').unwrap_or((rest, ""));
    let mut stream = TcpStream::connect(authority).expect("it accepts");
    let mut head = format!(
        "{method} /{target} HTTP/1.1\r\nHost: {authority}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).expect("sent");
    stream.write_all(body).expect("sent");
    let mut response = Vec::new();
    stream.read_to_end(&mut response).expect("answered");
    parse_response(&response)
}

/// A whole HTTP/1.1 response as status, headers (lowercase) and body.
fn parse_response(response: &[u8]) -> Got {
    let split = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("a head");
    let head = String::from_utf8_lossy(&response[..split]);
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse().ok())
        .expect("a status");
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_string()))
        .collect();
    (status, headers, response[split + 4..].to_vec())
}
