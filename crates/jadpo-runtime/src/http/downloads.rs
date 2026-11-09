//! Downloads (area 16, "Downloading"; chunk B4): a `File` leaves the
//! server as a signed link, and the runtime serves the link from the
//! app's own origin, for a store whose driver has no links of its own
//! (the disk driver). The same on both executors, since all of it is here.
//!
//! - **The link.** `GET /files/<store>/<link>`, where `<link>` is
//!   `<payload>.<tag>`, both base64url without padding: the payload is
//!   JSON of the file's key, name, size, type and hash and when the link
//!   expires, and the tag its keyed BLAKE3 hash under a key derived from
//!   the key the store keeps ([`crate::capability::Capability::link_key`])
//!   and the store's name. Everything the download needs is in what was
//!   signed, so serving one reads no record: the read that made the link
//!   has already passed policy. The tag is compared in constant time, and
//!   nothing in a link is read before it matches.
//! - **Serving it.** Single ranges and `If-Range`, an ETag from the file's
//!   hash, in windows of [`WINDOW`], so a large file uses the memory a small
//!   one does. A file's bytes are hostile, and this is the app's own origin:
//!   every answer says `nosniff` and `sandbox`, and all but an allow-list of
//!   image types are `attachment`, the file's name only ever in `filename*`.
//! - **A `Range` header.** Anything but one well-formed range of bytes is
//!   read as no `Range` at all (RFC 9110 lets a server ignore it): a list of
//!   ranges, overlapping or not, is the whole file once, never a piece for
//!   each. A well-formed range past the end is 416.

use std::rc::Rc;
use std::sync::Arc;

use jadpo_std::files::{DOWNLOADS, FILE, FILE_LINK, LINK_VALID_MS, READ, is_key, shown_inline};
use jadpo_value::Value;

use super::signed::{base64url, unbase64url};
use super::{
    HEADER_UNREADABLE, METHOD_NOT_ALLOWED, REQUEST_TOO_LARGE, Request, Response, headers_readable,
    runtime_failure,
};
use crate::capability::{Answer, Call, Repeat};
use crate::stream::{EventStream, Events, ServerEvent};
use crate::{Ctx, Host, LocalFuture};

/// How much of a file one read asks a store for.
const WINDOW: u64 = 1 << 16;

/// What a link's key is derived for: BLAKE3's key derivation keeps it apart
/// from any other use of what a store keeps.
const CONTEXT: &str = "jadpo 2026-11-08 file download link v1";

const LINK_INVALID: (u16, &str, &str) =
    (403, "link_invalid", "This download link isn't a valid one.");
const LINK_EXPIRED: (u16, &str, &str) = (403, "link_expired", "This download link has expired.");
const FILE_GONE: (u16, &str, &str) = (404, "file_not_found", "This file no longer exists.");
const FILE_UNAVAILABLE: (u16, &str, &str) = (
    503,
    "unavailable",
    "A dependency is temporarily unavailable.",
);
const RANGE_NOT_SATISFIABLE: (u16, &str, &str) = (
    416,
    "range_not_satisfiable",
    "The requested range is past the end of the file.",
);

const ALLOW: &str = "GET, HEAD, OPTIONS";

/// A key signing `store`'s links or slots, as `context` keeps them apart,
/// derived from what `store` keeps; `None` when no connection of that name
/// keeps files.
pub(super) fn derived_key(
    host: &dyn Host,
    store: &str,
    make: bool,
    context: &str,
) -> Option<[u8; 32]> {
    let kept = host.capability(store)?.link_key(store, make)?;
    let mut key = blake3::Hasher::new_derive_key(context);
    key.update(&kept);
    key.update(store.as_bytes());
    Some(*key.finalize().as_bytes())
}

/// The key signing `store`'s links; `None` when no connection of that name
/// keeps files.
fn signing_key(host: &dyn Host, store: &str, make: bool) -> Option<[u8; 32]> {
    derived_key(host, store, make, CONTEXT)
}

fn tag(key: &[u8; 32], body: &str) -> blake3::Hash {
    blake3::keyed_hash(key, body.as_bytes())
}

/// What a link says, once its tag has matched.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Said {
    key: String,
    name: Option<String>,
    size: u64,
    media: &'static str,
    hash: Option<String>,
    expires: i64,
}

/// `<payload>.<tag>` for `said`, signed with `key`.
fn sign(key: &[u8; 32], said: &Said) -> String {
    let payload = serde_json::json!({
        "k": said.key,
        "n": said.name,
        "s": said.size,
        "t": said.media,
        "h": said.hash,
        "e": said.expires,
    })
    .to_string();
    let body = base64url(payload.as_bytes());
    let tag = tag(key, &body);
    format!("{body}.{}", base64url(tag.as_bytes()))
}

/// What `link` says, when `key` signed it and what it says is one a store
/// can answer for; `None` for anything else, so what isn't signed is never
/// read.
fn open(key: &[u8; 32], link: &str) -> Option<Said> {
    let (body, sent) = link.split_once('.')?;
    let sent: [u8; 32] = unbase64url(sent)?.try_into().ok()?;
    if tag(key, body) != blake3::Hash::from_bytes(sent) {
        return None;
    }
    let payload = unbase64url(body)?;
    let serde_json::Value::Object(said) = serde_json::from_slice(&payload).ok()? else {
        return None;
    };
    let text = |name: &str| said.get(name).and_then(serde_json::Value::as_str);
    let key = text("k").filter(|key| is_key(key))?.to_string();
    let media = jadpo_std::FILE_TYPES
        .iter()
        .map(|(_, media)| *media)
        .find(|media| Some(*media) == text("t"))?;
    let hash = match said.get("h") {
        Some(serde_json::Value::String(hash)) if is_hash(hash) => Some(hash.clone()),
        Some(serde_json::Value::Null) | None => None,
        _ => return None,
    };
    Some(Said {
        key,
        name: text("n").map(str::to_string),
        size: said.get("s")?.as_u64()?,
        media,
        hash,
        expires: said.get("e")?.as_i64()?,
    })
}

/// Whether `hash` is safe to quote in a header: what [`crate::files::hash_of`]
/// writes.
fn is_hash(hash: &str) -> bool {
    !hash.is_empty() && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

// --- files becoming links -------------------------------------------------------

/// What a `File` becomes where it leaves the server, on one request's
/// clock and host.
pub(crate) struct Linker<'c> {
    host: &'c dyn Host,
    now: i64,
    /// How long a link it makes lasts.
    valid_ms: i64,
}

impl<'c> Linker<'c> {
    pub(crate) fn new(ctx: &'c Ctx<'_>) -> Linker<'c> {
        Self::valid_for(ctx, LINK_VALID_MS)
    }

    /// One whose links last `valid_ms`: an export's, as long as the export
    /// that holds them (area 15, "Exports").
    pub(crate) fn valid_for(ctx: &'c Ctx<'_>, valid_ms: i64) -> Linker<'c> {
        Linker {
            host: ctx.host(),
            now: ctx.env().clock.now(),
            valid_ms,
        }
    }

    /// `file`, a record of type `File`, as a [`FILE_LINK`]; `None` when its
    /// store signs no links.
    fn link(&self, file: &jadpo_value::Record) -> Option<Value> {
        let text = |name: &str| match file.get(name) {
            Some(Value::Text(text)) => Some(text.to_string()),
            _ => None,
        };
        let store = text("store")?;
        let key = text("key")?;
        let file_type = file.get("type")?;
        let media = crate::files::media_type(file_type)?;
        let size = match file.get("size") {
            Some(Value::Int(size)) => *size,
            _ => return None,
        };
        let name = text("name");
        let said = Said {
            key,
            name: name.clone(),
            size: u64::try_from(size).ok()?,
            media,
            hash: text("hash"),
            expires: self.now.saturating_add(self.valid_ms),
        };
        let link = sign(&signing_key(self.host, &store, true)?, &said);
        Some(Value::record(
            FILE_LINK,
            vec![
                (
                    Rc::from("url"),
                    Value::text(&format!("/{DOWNLOADS}/{store}/{link}")),
                ),
                (
                    Rc::from("name"),
                    name.as_deref().map_or(Value::None, Value::text),
                ),
                (Rc::from("size"), Value::Int(size)),
                (Rc::from("type"), file_type.clone()),
            ],
        ))
    }

    /// `value` with each `File` in it, however deep, as its link; `None`
    /// when a file's store signs no links, so a file is never sent as the
    /// store and key it is kept under.
    pub(crate) fn linked(&self, value: &Value) -> Option<Value> {
        Some(match value {
            Value::Record(record) if &*record.ty == FILE => self.link(record)?,
            Value::Record(record) => {
                let fields = record
                    .fields
                    .iter()
                    .map(|(name, field)| Some((name.clone(), self.linked(field)?)))
                    .collect::<Option<Vec<_>>>()?;
                let mut linked = (**record).clone();
                linked.fields = fields;
                Value::Record(Rc::new(linked))
            }
            Value::List(items) => Value::List(
                items
                    .iter()
                    .map(|item| self.linked(item))
                    .collect::<Option<Rc<[_]>>>()?,
            ),
            Value::Set(items) => Value::Set(
                items
                    .iter()
                    .map(|item| self.linked(item))
                    .collect::<Option<Rc<[_]>>>()?,
            ),
            Value::Map(map) => Value::Map(Rc::new(
                map.iter()
                    .map(|(key, value)| Some((key.clone(), self.linked(value)?)))
                    .collect::<Option<_>>()?,
            )),
            Value::Variant(variant) => {
                let fields = variant
                    .fields
                    .iter()
                    .map(|(name, field)| Some((name.clone(), self.linked(field)?)))
                    .collect::<Option<Vec<_>>>()?;
                let mut linked = (**variant).clone();
                linked.fields = fields;
                Value::Variant(Rc::new(linked))
            }
            other => other.clone(),
        })
    }
}

// --- serving a link ---------------------------------------------------------------

/// The store and link of a download's path, as sent: two segments after
/// [`DOWNLOADS`], neither decoded, since neither a store's name nor a link
/// holds anything to decode.
pub(super) fn path(request: &Request) -> Option<(&str, &str)> {
    let rest = request.path.strip_prefix('/')?;
    let rest = rest.strip_prefix(DOWNLOADS)?.strip_prefix('/')?;
    let (store, link) = rest.split_once('/')?;
    (!store.is_empty() && !link.is_empty() && !link.contains('/')).then_some((store, link))
}

/// Which bytes a request asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Asked {
    Whole,
    Part { from: u64, length: u64 },
    Beyond,
}

/// What `range`, a `Range` header's value, asks of a file of `size` bytes.
fn asked(range: &str, size: u64) -> Asked {
    let Some(spec) = range.strip_prefix("bytes=") else {
        return Asked::Whole;
    };
    let Some((first, last)) = spec.split_once('-') else {
        return Asked::Whole;
    };
    // Digits alone: no sign, space or list, and no value past a `u64`.
    let number = |digits: &str| {
        (!digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| digits.parse::<u64>().ok())
            .flatten()
    };
    if first.is_empty() {
        let Some(suffix) = number(last) else {
            return Asked::Whole;
        };
        return if suffix == 0 || size == 0 {
            Asked::Beyond
        } else {
            let length = suffix.min(size);
            Asked::Part {
                from: size - length,
                length,
            }
        };
    }
    let Some(from) = number(first) else {
        return Asked::Whole;
    };
    let end = if last.is_empty() {
        None
    } else {
        let Some(end) = number(last) else {
            return Asked::Whole;
        };
        if end < from {
            return Asked::Whole;
        }
        Some(end)
    };
    if from >= size {
        return Asked::Beyond;
    }
    let end = end.map_or(size - 1, |end| end.min(size - 1));
    Asked::Part {
        from,
        length: end - from + 1,
    }
}

/// What `request` asks of a file of `size` bytes with hash `hash`: a
/// `GET`'s one `Range`, unless an `If-Range` says the file has changed.
fn asked_of(request: &Request, size: u64, hash: Option<&str>) -> Asked {
    if request.method != "GET" {
        return Asked::Whole;
    }
    let mut ranges = request
        .headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("range"))
        .map(|(_, value)| value.as_str());
    let (Some(range), None) = (ranges.next(), ranges.next()) else {
        return Asked::Whole;
    };
    if let Some(validator) = request.header("if-range") {
        let same = hash.is_some_and(|hash| validator == format!("\"{hash}\""));
        if !same {
            return Asked::Whole;
        }
    }
    asked(range, size)
}

/// `name` as `filename*`'s value (RFC 8187): everything but its
/// attr-chars percent-encoded, so nothing in it can end the header.
fn filename(name: &str) -> String {
    let mut out = String::from("UTF-8''");
    for byte in name.bytes() {
        if byte.is_ascii_alphanumeric() || b"!#$&+-.^_`|~".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// `name` percent-encoded as [`filename`] writes it, without its charset
/// prefix.
pub(super) fn filename_value(name: &str) -> String {
    filename(name).trim_start_matches("UTF-8''").to_string()
}

/// What a file with no name of its own is saved as.
fn saved_as(media: &str) -> String {
    let extension = jadpo_std::FILE_TYPES
        .iter()
        .find(|(_, known)| *known == media)
        .and_then(|(variant, _)| jadpo_std::files::extension(variant))
        .unwrap_or("bin");
    format!("download.{extension}")
}

fn disposition(said: &Said) -> String {
    let kind = if shown_inline(said.media) {
        "inline"
    } else {
        "attachment"
    };
    let name = said.name.clone().unwrap_or_else(|| saved_as(said.media));
    format!("{kind}; filename*={}", filename(&name))
}

fn file_headers(said: &Said, length: u64) -> Vec<(String, String)> {
    let mut headers = vec![
        ("content-type".to_string(), said.media.to_string()),
        ("content-length".to_string(), length.to_string()),
        ("content-disposition".to_string(), disposition(said)),
        ("x-content-type-options".to_string(), "nosniff".to_string()),
        ("content-security-policy".to_string(), "sandbox".to_string()),
        ("accept-ranges".to_string(), "bytes".to_string()),
        ("cache-control".to_string(), "private, no-store".to_string()),
    ];
    if let Some(hash) = &said.hash {
        headers.push(("etag".to_string(), format!("\"{hash}\"")));
    }
    headers
}

/// The call reading `length` bytes of `key` from `from`.
fn window(store: &str, key: &str, from: u64, length: u64) -> Call {
    let input = Value::record(
        "FileRead",
        vec![
            (Rc::from("key"), Value::text(key)),
            (
                Rc::from("from"),
                Value::Int(i64::try_from(from).unwrap_or(i64::MAX)),
            ),
            (
                Rc::from("length"),
                Value::Int(i64::try_from(length).unwrap_or(0)),
            ),
        ],
    );
    Call::new(store, READ, input, Repeat::Safe)
}

/// The rest of a file's bytes, read a window at a time as the host sends
/// them: dropped when the client goes away. A read that fails part-way
/// ends it early, and the `content-length` already sent says so.
struct Windows {
    store: String,
    key: String,
    at: u64,
    left: u64,
    first: Option<Vec<u8>>,
}

impl Events for Windows {
    fn next<'a>(&'a mut self, host: Option<&'a dyn Host>) -> LocalFuture<'a, Option<ServerEvent>> {
        Box::pin(async move {
            if let Some(first) = self.first.take() {
                return Some(ServerEvent::Bytes(first));
            }
            if self.left == 0 {
                return None;
            }
            let call = window(&self.store, &self.key, self.at, self.left.min(WINDOW));
            let answer = host?.capability(&self.store)?.call(&call).await;
            let Answer::Value(Value::Bytes(bytes)) = answer else {
                return None;
            };
            let read = bytes.len() as u64;
            if read == 0 {
                return None;
            }
            self.at += read;
            self.left = self.left.saturating_sub(read);
            Some(ServerEvent::Bytes(bytes.to_vec()))
        })
    }

    fn end_by(&mut self, _at: Option<u64>) {}
}

/// The answer to a request for `link` in `store`, when `store` keeps files;
/// `None` when no store has that name, so the routes are looked through
/// as for any other path.
pub(super) async fn answer(
    ctx: &Ctx<'_>,
    request: &Request,
    request_id: &str,
    store: &str,
    link: &str,
) -> Option<Response> {
    let key = signing_key(ctx.host(), store, false)?;
    let mut response = answered(ctx, request, request_id, store, link, &key).await;
    response.route = Some(Arc::from(format!("GET /{DOWNLOADS}/{{store}}/{{link}}")));
    Some(response)
}

async fn answered(
    ctx: &Ctx<'_>,
    request: &Request,
    request_id: &str,
    store: &str,
    link: &str,
    key: &[u8; 32],
) -> Response {
    // Counted against its address before anything in it is read, as any
    // request is.
    if let Some(response) = super::by_address(ctx, request_id).await {
        return response;
    }
    if !headers_readable(request) {
        return runtime_failure(HEADER_UNREADABLE, request_id, None);
    }
    if request.too_large || request.body.len() > super::BODY_LIMIT {
        return runtime_failure(REQUEST_TOO_LARGE, request_id, None);
    }
    match request.method.as_str() {
        "GET" | "HEAD" => {}
        "OPTIONS" => {
            return Response {
                status: 204,
                headers: vec![("allow".to_string(), ALLOW.to_string())],
                ..Response::default()
            };
        }
        _ => {
            let mut response = runtime_failure(METHOD_NOT_ALLOWED, request_id, None);
            response
                .headers
                .push(("allow".to_string(), ALLOW.to_string()));
            return response;
        }
    }
    let Some(said) = open(key, link) else {
        return runtime_failure(LINK_INVALID, request_id, None);
    };
    if ctx.env().clock.now() > said.expires {
        return runtime_failure(LINK_EXPIRED, request_id, None);
    }
    let (status, from, length) = match asked_of(request, said.size, said.hash.as_deref()) {
        Asked::Whole => (200, 0, said.size),
        Asked::Part { from, length } => (206, from, length),
        Asked::Beyond => {
            let mut response = runtime_failure(RANGE_NOT_SATISFIABLE, request_id, None);
            response.headers.push((
                "content-range".to_string(),
                format!("bytes */{}", said.size),
            ));
            response
                .headers
                .push(("accept-ranges".to_string(), "bytes".to_string()));
            return response;
        }
    };
    // The first window is read before the answer begins, so a file that
    // has gone is a 404 and not an empty 200.
    let first = if length == 0 {
        Vec::new()
    } else {
        let Some(driver) = ctx.host().capability(store) else {
            return runtime_failure(FILE_UNAVAILABLE, request_id, None);
        };
        let call = window(store, &said.key, from, length.min(WINDOW));
        match driver.call(&call).await {
            Answer::Value(Value::Bytes(bytes)) => bytes.to_vec(),
            Answer::Failed(_) => return runtime_failure(FILE_GONE, request_id, None),
            _ => return runtime_failure(FILE_UNAVAILABLE, request_id, None),
        }
    };
    let mut headers = file_headers(&said, length);
    if status == 206 {
        headers.push((
            "content-range".to_string(),
            format!("bytes {from}-{}/{}", from + length - 1, said.size),
        ));
    }
    let read = first.len() as u64;
    let body = Windows {
        store: store.to_string(),
        key: said.key.clone(),
        at: from + read,
        left: length.saturating_sub(read),
        first: Some(first),
    };
    Response {
        status,
        headers,
        stream: (length > 0).then(|| EventStream::new(Box::new(body), request_id)),
        ..Response::default()
    }
}

/// A route test's `request { link: ... }` (area 13, "Route tests"): the
/// link's own request, through the HTTP component, its body read to the
/// end as a client reads it. `link` is the `File` a response's output
/// held, or the path of one as text; `range` a `Range` header's value.
pub async fn link_test(
    app: &super::App<'_>,
    executor: &dyn crate::Executor,
    ctx: &mut Ctx<'_>,
    link: &Value,
    range: Option<&Value>,
) -> Result<Value, Box<crate::Fault>> {
    // A `File` from a record, as well as a link from a response's output.
    let made;
    let link = match link {
        Value::Record(record) if &*record.ty == FILE => {
            made = Linker::new(ctx).link(record).unwrap_or(Value::None);
            &made
        }
        other => other,
    };
    let path = match link {
        Value::Text(path) => Some(path.to_string()),
        Value::Record(record) => match record.get("url") {
            Some(Value::Text(url)) => Some(url.to_string()),
            _ => None,
        },
        _ => None,
    };
    let Some(path) = path else {
        return Err(ctx.fault(
            crate::FaultKind::Defect("a link that isn't a file's".to_string()),
            0,
        ));
    };
    let mut request = Request {
        method: "GET".to_string(),
        path,
        ..Request::default()
    };
    if let Some(Value::Text(range)) = range {
        request
            .headers
            .push(("range".to_string(), range.to_string()));
    }
    let id = super::request_id(ctx, u64::from(ctx.actions()));
    let mut response = super::handle(app, executor, ctx, &request, &id).await;
    if let Some(fault) = response.fault.take() {
        return Err(fault);
    }
    let mut body = Vec::new();
    if let Some(stream) = response.stream.take() {
        while let Some(event) = stream.next_on(Some(ctx.host())).await {
            if let ServerEvent::Bytes(bytes) = event {
                body.extend(bytes);
            }
        }
    }
    let ok = (200..300).contains(&response.status);
    Ok(Value::record(
        "Response",
        vec![
            (Rc::from("status"), Value::Int(i64::from(response.status))),
            (
                Rc::from("code"),
                response.code.as_deref().map_or(Value::None, Value::text),
            ),
            (
                Rc::from("body"),
                if ok {
                    Value::Bytes(Rc::from(body))
                } else {
                    Value::None
                },
            ),
        ],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said() -> Said {
        Said {
            key: jadpo_std::files::key_for(&[7; 16]),
            name: Some("r\u{e9}sum\u{e9}.pdf".to_string()),
            size: 100,
            media: "application/pdf",
            hash: Some("abc123".to_string()),
            expires: 1_000,
        }
    }

    #[test]
    fn a_link_opens_only_as_it_was_signed() {
        let key = [1u8; 32];
        let link = sign(&key, &said());
        assert_eq!(open(&key, &link), Some(said()));
        // Another key's, or nothing: the same answer, nothing read.
        assert_eq!(open(&[2u8; 32], &link), None);
        assert_eq!(open(&key, "nothing"), None);
        assert_eq!(open(&key, "a.b"), None);
        assert_eq!(open(&key, ""), None);
        // Each part of it counts: another link's tag, or body, is refused.
        let mut other = said();
        other.key = jadpo_std::files::key_for(&[8; 16]);
        let second = sign(&key, &other);
        let (body_a, tag_a) = link.split_once('.').unwrap();
        let (body_b, tag_b) = second.split_once('.').unwrap();
        assert_eq!(open(&key, &format!("{body_a}.{tag_b}")), None);
        assert_eq!(open(&key, &format!("{body_b}.{tag_a}")), None);
    }

    #[test]
    fn a_store_signs_with_its_own_key() {
        // What makes a link good for one store and no other.
        let (a, b) = {
            let mut one = blake3::Hasher::new_derive_key(CONTEXT);
            one.update(&[3u8; 32]);
            one.update(b"sheets");
            let mut two = blake3::Hasher::new_derive_key(CONTEXT);
            two.update(&[3u8; 32]);
            two.update(b"full");
            (*one.finalize().as_bytes(), *two.finalize().as_bytes())
        };
        assert_ne!(a, b);
        let link = sign(&a, &said());
        assert_eq!(open(&b, &link), None);
    }

    #[test]
    fn nothing_but_what_a_store_can_answer_for_opens_even_when_signed() {
        let key = [1u8; 32];
        // A signed payload naming a path outside the store is no key.
        for mut hostile in [said(), said(), said()] {
            hostile.key = "../../etc/passwd".to_string();
            assert_eq!(open(&key, &sign(&key, &hostile)), None);
        }
        let mut hostile = said();
        hostile.key = "/etc/passwd".to_string();
        assert_eq!(open(&key, &sign(&key, &hostile)), None);
        // A hash that isn't hex never reaches a header.
        let mut hostile = said();
        hostile.hash = Some("a\"\r\nx: y".to_string());
        assert_eq!(open(&key, &sign(&key, &hostile)), None);
    }

    #[test]
    fn a_range_is_one_well_formed_range_or_the_whole_file() {
        let at = |from, length| Asked::Part { from, length };
        for (written, wanted) in [
            ("bytes=0-4", at(0, 5)),
            ("bytes=95-", at(95, 5)),
            ("bytes=95-5000", at(95, 5)),
            ("bytes=-5", at(95, 5)),
            ("bytes=-5000", at(0, 100)),
            ("bytes=100-", Asked::Beyond),
            ("bytes=100-200", Asked::Beyond),
            ("bytes=-0", Asked::Beyond),
            // Malformed or more than one: read as no range at all.
            ("bytes=5-1", Asked::Whole),
            ("bytes=", Asked::Whole),
            ("bytes=-", Asked::Whole),
            ("bytes=a-b", Asked::Whole),
            ("bytes=0-1,2-3", Asked::Whole),
            ("bytes=0-1,0-1", Asked::Whole),
            ("bytes= 0-1", Asked::Whole),
            ("bytes=0 -1", Asked::Whole),
            ("bytes=+0-1", Asked::Whole),
            ("bytes=-+1", Asked::Whole),
            ("bytes=0-0x1", Asked::Whole),
            ("bytes=99999999999999999999-", Asked::Whole),
            ("bytes=-99999999999999999999", Asked::Whole),
            ("Bytes=0-1", Asked::Whole),
            ("items=0-1", Asked::Whole),
            ("", Asked::Whole),
        ] {
            assert_eq!(asked(written, 100), wanted, "{written}");
        }
        assert_eq!(asked("bytes=0-", 0), Asked::Beyond);
        assert_eq!(asked("bytes=-1", 0), Asked::Beyond);
        assert_eq!(asked("bytes=0-0", 1), at(0, 1));
    }

    #[test]
    fn if_range_matches_only_the_etag_exactly() {
        let request = |headers: &[(&str, &str)]| Request {
            method: "GET".to_string(),
            headers: headers
                .iter()
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .collect(),
            ..Request::default()
        };
        let hash = Some("abc");
        let range = ("Range", "bytes=0-1");
        let of = |request: &Request| asked_of(request, 100, hash);
        let part = Asked::Part { from: 0, length: 2 };
        assert_eq!(of(&request(&[range])), part);
        assert_eq!(of(&request(&[range, ("If-Range", "\"abc\"")])), part);
        for stale in [
            "\"abd\"",
            "W/\"abc\"",
            "abc",
            "Sun, 06 Nov 1994 08:49:37 GMT",
        ] {
            assert_eq!(
                of(&request(&[range, ("If-Range", stale)])),
                Asked::Whole,
                "{stale}"
            );
        }
        // Two `Range` headers are as much a list as one with a comma.
        assert_eq!(of(&request(&[range, range])), Asked::Whole);
        // A file with no hash has no ETag to match.
        let r = request(&[range, ("If-Range", "\"abc\"")]);
        assert_eq!(asked_of(&r, 100, None), Asked::Whole);
        // A `HEAD` reads no range.
        let mut head = request(&[range]);
        head.method = "HEAD".to_string();
        assert_eq!(asked_of(&head, 100, hash), Asked::Whole);
    }

    #[test]
    fn a_file_name_only_goes_in_filename_star_encoded() {
        assert_eq!(filename("a.csv"), "UTF-8''a.csv");
        // Nothing in it can end the header or split the value.
        for hostile in [
            "a\r\nX-Evil: 1.csv",
            "a\"; filename=\"b",
            "../../etc/passwd",
            "a b",
        ] {
            let got = filename(hostile);
            assert!(
                got.bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"!#$&+-.^_`|~%'".contains(&byte)),
                "{got}"
            );
            assert!(!got.contains('"') && !got.contains('/') && !got.contains('\r'));
        }
        assert_eq!(filename("r\u{e9}.pdf"), "UTF-8''r%C3%A9.pdf");
    }

    #[test]
    fn only_listed_image_types_are_shown_in_place() {
        let mut image = said();
        for media in ["image/png", "image/jpeg", "image/webp"] {
            image.media = media;
            assert!(
                disposition(&image).starts_with("inline; filename*="),
                "{media}"
            );
        }
        let mut other = said();
        for media in ["application/pdf", "text/csv", "application/json"] {
            other.media = media;
            assert!(
                disposition(&other).starts_with("attachment; filename*="),
                "{media}"
            );
        }
        other.name = None;
        other.media = "application/pdf";
        assert_eq!(
            disposition(&other),
            "attachment; filename*=UTF-8''download.pdf"
        );
    }
}
