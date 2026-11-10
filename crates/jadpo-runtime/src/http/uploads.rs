//! Uploads (area 16, "Uploading"; chunk B5): a client's file goes to the
//! store's own address, not through the application's routes. An action
//! asks for a slot; the client sends the file to the slot's address; and
//! an ordinary create or update attaches it by the slot's token. The same
//! on both executors, since all of it is here.
//!
//! - **The token.** `upl_<payload>.<tag>`, both base64url without padding:
//!   the payload is JSON of the file's key, the field it is for, the most
//!   bytes it may hold, the types the field takes and when the address
//!   expires, and the tag its keyed BLAKE3 hash under a key derived from
//!   the key the store keeps and the store's name, apart from what signs
//!   its downloads. The tag is compared in constant time, and nothing in a
//!   token is read before it matches.
//! - **The address.** `PUT /files/<store>/uploads/<token>`, answered ahead
//!   of the routes, for a store whose driver has no addresses of its own.
//!   The body is taken a piece at a time: counted, with a `413` at the
//!   first byte over the slot's limit, hashed, its type told from its first
//!   bytes, and only then made the file, whole. A body that says it is
//!   compressed is refused before a byte is read. A slot is one file: once
//!   its file has landed, sending another is `409`.
//! - **Parts.** A field over 100 MB gets a slot in parts (area 16, "Large
//!   uploads go in parts"): `PUT /files/<store>/uploads/<token>/parts/<n>`,
//!   each part kept whole or not at all and sent again freely. The token
//!   says the part size, so the address needs no record of the slot; a
//!   store only ever keeps complete parts.
//! - **The attach.** A `FileUpload` set on a `File` field names the slot's
//!   token; [`crate::files::attach`] joins a slot's parts, then checks it
//!   before the write, the one check for a field, however the bytes reached
//!   the store.

use std::rc::Rc;
use std::sync::Arc;

use jadpo_std::files::{
    DOWNLOADS, HEAD_BYTES, PARTS, PARTS_SLOT_VALID_MS, SINGLE_SLOT_LIMIT, SLOT_VALID_MS,
    UPLOAD_SLOT, UPLOADS, is_key, part_limit, part_size_for, type_of,
};
use jadpo_value::Value;

use super::downloads::derived_key;
use super::signed::{base64url, unbase64url};
use super::{
    HEADER_UNREADABLE, METHOD_NOT_ALLOWED, Request, Response, headers_readable, runtime_failure,
};
use crate::capability::{Kept, Refused};
use crate::{Ctx, Host, LocalFuture};

/// What a slot's token is signed for: BLAKE3's key derivation keeps it
/// apart from a store's download links, signed with the same key it keeps.
const CONTEXT: &str = "jadpo 2026-11-09 file upload slot v1";

/// What every token starts with.
const PREFIX: &str = "upl_";

const LINK_INVALID: (u16, &str, &str) = (
    403,
    "link_invalid",
    "This upload address isn't a valid one.",
);
const LINK_EXPIRED: (u16, &str, &str) = (403, "link_expired", "This upload address has expired.");
const FILE_TOO_LARGE: (u16, &str, &str) = (
    413,
    "file_too_large",
    "This file is over the size this slot takes.",
);
const ENCODING_REFUSED: (u16, &str, &str) = (
    415,
    "content_encoding_refused",
    "An upload is sent as it is, without a Content-Encoding.",
);
const TYPE_REFUSED: (u16, &str, &str) = (
    415,
    "file_type_refused",
    "This file isn't a type this slot takes.",
);
const ALREADY_SENT: (u16, &str, &str) = (
    409,
    "upload_already_sent",
    "A file has already been sent to this slot.",
);
const STORAGE_FULL: (u16, &str, &str) =
    (503, "storage_full", "There is no room to keep this file.");
const STORE_UNAVAILABLE: (u16, &str, &str) = (
    503,
    "unavailable",
    "A dependency is temporarily unavailable.",
);
const BODY_LOST: (u16, &str, &str) = (
    400,
    "invalid_request",
    "The connection ended before the whole file was sent.",
);

const ALLOW: &str = "PUT, OPTIONS";

/// What a slot's token says, once its tag has matched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Slot {
    /// The key its file is kept under.
    pub(crate) key: String,
    /// The field it is for: `PriceSheet.sheet`.
    pub(crate) field: String,
    /// The most bytes it may hold.
    pub(crate) max_size: u64,
    /// The `FileType` variants the field takes; empty for any.
    pub(crate) types: Vec<String>,
    /// When its address stops being good.
    pub(crate) expires: i64,
    /// The size of each part, on a slot in parts.
    pub(crate) part_size: Option<u64>,
}

/// `upl_<payload>.<tag>` for `slot`, signed with `key`.
fn sign(key: &[u8; 32], slot: &Slot) -> String {
    let payload = serde_json::json!({
        "k": slot.key,
        "f": slot.field,
        "m": slot.max_size,
        "t": slot.types,
        "e": slot.expires,
        "p": slot.part_size,
    })
    .to_string();
    let body = base64url(payload.as_bytes());
    let tag = blake3::keyed_hash(key, body.as_bytes());
    format!("{PREFIX}{body}.{}", base64url(tag.as_bytes()))
}

/// What `token` says, when `key` signed it; `None` for anything else, so
/// what isn't signed is never read.
fn open(key: &[u8; 32], token: &str) -> Option<Slot> {
    let (body, sent) = token.strip_prefix(PREFIX)?.split_once('.')?;
    let sent: [u8; 32] = unbase64url(sent)?.try_into().ok()?;
    if blake3::keyed_hash(key, body.as_bytes()) != blake3::Hash::from_bytes(sent) {
        return None;
    }
    let payload = unbase64url(body)?;
    let serde_json::Value::Object(said) = serde_json::from_slice(&payload).ok()? else {
        return None;
    };
    let text = |name: &str| said.get(name).and_then(serde_json::Value::as_str);
    Some(Slot {
        key: text("k").filter(|key| is_key(key))?.to_string(),
        field: text("f")?.to_string(),
        max_size: said.get("m")?.as_u64()?,
        types: said
            .get("t")?
            .as_array()?
            .iter()
            .map(|item| item.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>()?,
        expires: said.get("e")?.as_i64()?,
        part_size: match said.get("p") {
            None | Some(serde_json::Value::Null) => None,
            Some(size) => Some(size.as_u64().filter(|size| *size > 0)?),
        },
    })
}

/// What `token` says of a slot of `store`, for an attach.
pub(crate) fn token_slot(host: &dyn Host, store: &str, token: &str) -> Option<Slot> {
    open(&derived_key(host, store, false, CONTEXT)?, token)
}

/// A new slot for `plan`'s field, its file to be kept under `key`, good
/// for fifteen minutes from `now`, or six hours when the field is over 100
/// MB and so sent in parts: the `UploadSlot` an action hands the client.
/// `None` when the store signs nothing.
pub(crate) fn slot_value(
    host: &dyn Host,
    plan: &jadpo_std::files::UploadPlan,
    key: String,
    now: i64,
) -> Option<Value> {
    let part_size = (plan.max_size > SINGLE_SLOT_LIMIT).then(|| part_size_for(plan.max_size));
    let valid = if part_size.is_some() {
        PARTS_SLOT_VALID_MS
    } else {
        SLOT_VALID_MS
    };
    let slot = Slot {
        key,
        field: plan.field.clone(),
        max_size: plan.max_size,
        types: plan.types.clone(),
        expires: now.saturating_add(valid),
        part_size,
    };
    let token = sign(&derived_key(host, &plan.store, true, CONTEXT)?, &slot);
    let address = format!("/{DOWNLOADS}/{}/{UPLOADS}/{token}", plan.store);
    let direct = host.capability(&plan.store)?.direct(&plan.store);
    let (url, part, parts) = match part_size {
        Some(size) => (
            Value::None,
            Value::Int(i64::try_from(size).unwrap_or(i64::MAX)),
            Value::text(&format!("{address}/{PARTS}")),
        ),
        // A store with addresses of its own takes the file at a signed
        // address of its own, not through this origin.
        None => match direct {
            Some(direct) => (
                Value::text(&direct.upload(&slot.key, slot.expires, now)?),
                Value::None,
                Value::None,
            ),
            None => (Value::text(&address), Value::None, Value::None),
        },
    };
    let fields = vec![
        (Rc::from("url"), url),
        (Rc::from("token"), Value::text(&token)),
        (
            Rc::from("max_size"),
            Value::Int(i64::try_from(plan.max_size).unwrap_or(i64::MAX)),
        ),
        (Rc::from("part_size"), part),
        (Rc::from("parts_url"), parts),
    ];
    // What doesn't apply to it is left out of its JSON rather than sent as
    // `null` (area 16, "Large uploads go in parts").
    let omitted: Vec<u32> = (0u32..)
        .zip(&fields)
        .filter(|(_, (_, value))| *value == Value::None)
        .map(|(place, _)| place)
        .collect();
    Some(Value::Record(Rc::new(jadpo_value::Record {
        ty: Rc::from(UPLOAD_SLOT),
        fields,
        omitted: (!omitted.is_empty()).then(|| Rc::from(omitted)),
    })))
}

/// An upload's address, whatever store it names: the store, the token and,
/// on a part's address, what the path says its number is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Address<'a> {
    pub(super) store: &'a str,
    pub(super) token: &'a str,
    pub(super) part: Option<&'a str>,
}

/// Whether `path` is an upload's address: what a host asks before it reads
/// a request's body.
fn split(path: &str) -> Option<Address<'_>> {
    let rest = path.strip_prefix('/')?.strip_prefix(DOWNLOADS)?;
    let rest = rest.strip_prefix('/')?;
    let (store, rest) = rest.split_once('/')?;
    let rest = rest.strip_prefix(UPLOADS)?.strip_prefix('/')?;
    let (token, part) = match rest.split_once('/') {
        Some((token, tail)) => {
            let number = tail.strip_prefix(PARTS)?.strip_prefix('/')?;
            if number.is_empty() || number.contains('/') {
                return None;
            }
            (token, Some(number))
        }
        None => (rest, None),
    };
    (!store.is_empty() && !token.is_empty()).then_some(Address { store, token, part })
}

/// The store of an upload's path, for a host to ask whether it keeps files
/// before it reads the body.
pub fn store_of(path: &str) -> Option<&str> {
    split(path).map(|address| address.store)
}

/// An upload's path, as sent.
pub(super) fn path(request: &Request) -> Option<Address<'_>> {
    split(&request.path)
}

/// A file's name as the client gave it, from its `Content-Disposition`
/// (`filename*`, else `filename`): the last part of a path, with no
/// control characters and no more than 255 bytes, or `None`.
pub fn file_name(header: Option<&str>) -> Option<String> {
    let header = header?;
    let param = |wanted: &str| {
        header.split(';').skip(1).find_map(|part| {
            let (name, value) = part.trim().split_once('=')?;
            name.trim()
                .eq_ignore_ascii_case(wanted)
                .then(|| value.trim())
        })
    };
    let raw = match param("filename*") {
        Some(value) => {
            // RFC 8187: `UTF-8''<percent-encoded>`; no other charset.
            let (charset, rest) = value.split_once('\'')?;
            if !charset.eq_ignore_ascii_case("utf-8") {
                return None;
            }
            let (_, encoded) = rest.split_once('\'')?;
            let bytes = percent_decode(encoded)?;
            String::from_utf8(bytes).ok()?
        }
        None => {
            let value = param("filename")?;
            value.trim_matches('"').to_string()
        }
    };
    let name = raw
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .chars()
        .filter(|ch| !ch.is_control())
        .collect::<String>();
    (!name.is_empty() && name.len() <= 255 && name != "." && name != "..").then_some(name)
}

fn percent_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len());
    let mut bytes = text.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let digits = [bytes.next()?, bytes.next()?];
            let digits = std::str::from_utf8(&digits).ok()?;
            out.push(u8::from_str_radix(digits, 16).ok()?);
        } else {
            out.push(byte);
        }
    }
    Some(out)
}

/// `FileType`'s variant a `Content-Type` names, if any.
pub fn claimed_type(header: Option<&str>) -> Option<&'static str> {
    let essence = header?.split(';').next()?.trim().to_ascii_lowercase();
    jadpo_std::FILE_TYPES
        .iter()
        .find(|(_, media)| *media == essence)
        .map(|(variant, _)| *variant)
}

/// The answer to a request at `address`, when its store keeps files;
/// `None` when no store has that name, so the routes are looked through as
/// for any other path.
pub(super) fn answer<'a>(
    ctx: &'a Ctx<'_>,
    request: &'a Request,
    request_id: &'a str,
    address: Address<'a>,
) -> LocalFuture<'a, Option<Response>> {
    Box::pin(async move {
        let key = derived_key(ctx.host(), address.store, false, CONTEXT)?;
        let mut response = answered(ctx, request, request_id, address, &key).await;
        response.route = Some(Arc::from(if address.part.is_some() {
            format!("PUT /{DOWNLOADS}/{{store}}/{UPLOADS}/{{slot}}/{PARTS}/{{part}}")
        } else {
            format!("PUT /{DOWNLOADS}/{{store}}/{UPLOADS}/{{slot}}")
        }));
        Some(response)
    })
}

/// A part's number as its address writes it: digits with no leading zero.
fn part_number(text: &str) -> Option<u64> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) || text.starts_with('0') {
        return None;
    }
    text.parse().ok()
}

async fn answered(
    ctx: &Ctx<'_>,
    request: &Request,
    request_id: &str,
    address: Address<'_>,
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
    match request.method.as_str() {
        "PUT" => {}
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
    let Some(slot) = open(key, address.token) else {
        return runtime_failure(LINK_INVALID, request_id, None);
    };
    if ctx.env().clock.now() > slot.expires {
        return runtime_failure(LINK_EXPIRED, request_id, None);
    }
    // A slot's own address, or one of its parts': never the other's.
    let (part, limit) = match (slot.part_size, address.part) {
        (None, None) => (None, slot.max_size),
        (Some(size), Some(text)) => {
            let Some(number) = part_number(text) else {
                return runtime_failure(LINK_INVALID, request_id, None);
            };
            let Some(limit) = part_limit(slot.max_size, size, number) else {
                return runtime_failure(FILE_TOO_LARGE, request_id, None);
            };
            (Some(number), limit)
        }
        _ => return runtime_failure(LINK_INVALID, request_id, None),
    };
    // Nothing is read of a body that says it is compressed, so a small
    // upload can't unpack to gigabytes (area 16, "Compressed uploads").
    if request.header("content-encoding").is_some() {
        return runtime_failure(ENCODING_REFUSED, request_id, None);
    }
    let declared = request
        .header("content-length")
        .and_then(|length| length.parse::<u64>().ok());
    if declared.is_some_and(|length| length > limit) {
        return runtime_failure(FILE_TOO_LARGE, request_id, None);
    }
    let name = file_name(request.header("content-disposition"));
    let claimed = claimed_type(request.header("content-type"));
    // A store with addresses of its own takes the bytes itself: a slot's
    // part is redirected there, and a plain slot was never given this
    // address (area 16, "The S3 driver").
    if let Some(direct) = ctx
        .host()
        .capability(address.store)
        .and_then(|driver| driver.direct(address.store))
    {
        let Some(number) = part else {
            return runtime_failure(LINK_INVALID, request_id, None);
        };
        let file = crate::capability::PartFile {
            name: name.as_deref(),
            claimed,
        };
        let now = ctx.env().clock.now();
        return match direct
            .part(&slot.key, number, file, slot.expires, now)
            .await
        {
            Ok(location) => Response {
                status: 307,
                headers: vec![
                    ("location".to_string(), location),
                    ("cache-control".to_string(), "no-store".to_string()),
                ],
                ..Response::default()
            },
            Err(refused) => runtime_failure(refused_answer(refused), request_id, None),
        };
    }
    let sent = Sent {
        part,
        limit,
        declared,
        name,
        claimed,
    };
    match received(ctx, request, address.store, &slot, sent).await {
        Ok(()) => Response {
            status: 204,
            ..Response::default()
        },
        Err(failure) => runtime_failure(failure, request_id, None),
    }
}

/// What a request says of the file, or the part, it sends.
struct Sent {
    /// The part's number, when it sends one.
    part: Option<u64>,
    /// The most bytes it may hold.
    limit: u64,
    /// What its `Content-Length` says.
    declared: Option<u64>,
    name: Option<String>,
    claimed: Option<&'static str>,
}

/// Keeps what `request` sends as `slot`'s file, or the part of it `sent`
/// names: counted, hashed, its type told, then made whole. Why it wasn't,
/// otherwise, with nothing kept. A file's type is told from its first
/// bytes, so only a part 1 is checked for it.
async fn received(
    ctx: &Ctx<'_>,
    request: &Request,
    store: &str,
    slot: &Slot,
    sent: Sent,
) -> Result<(), (u16, &'static str, &'static str)> {
    let Sent {
        part,
        limit,
        declared,
        name,
        claimed,
    } = sent;

    let Some(driver) = ctx.host().capability(store) else {
        return Err(STORE_UNAVAILABLE);
    };
    // Recorded as held by none before the bytes land, as a put's are, so
    // bytes no record ever holds are deleted after a day (area 16,
    // "Uploading"), a crash between the two included.
    if crate::files::keep_unheld(ctx, store, &slot.key)
        .await
        .is_err()
    {
        return Err(STORE_UNAVAILABLE);
    }
    let incoming = declared.unwrap_or(limit);
    let opened = match part {
        Some(number) => driver.receive_part(store, &slot.key, number, incoming),
        None => driver.receive(store, &slot.key, incoming),
    };
    let mut sink = match opened {
        Ok(sink) => sink,
        Err(Refused::Full) => return Err(STORAGE_FULL),
        Err(Refused::Taken) => return Err(ALREADY_SENT),
        Err(Refused::Unavailable) => return Err(STORE_UNAVAILABLE),
    };
    let mut hasher = blake3::Hasher::new();
    let mut size = 0u64;
    let mut head: Vec<u8> = Vec::new();
    // Only a file's first bytes tell its type, so a later part has none to
    // check.
    let mut checked = matches!(part, Some(number) if number > 1);
    let mut whole = Pieces::of(request);
    loop {
        let piece = match whole.next().await {
            Some(Ok(piece)) => piece,
            Some(Err(())) => return Err(BODY_LOST),
            None => break,
        };
        if piece.is_empty() {
            continue;
        }
        size = size.saturating_add(piece.len() as u64);
        if size > limit {
            return Err(FILE_TOO_LARGE);
        }
        hasher.update(&piece);
        if !checked {
            let room = HEAD_BYTES.saturating_sub(head.len());
            head.extend_from_slice(piece.get(..room.min(piece.len())).unwrap_or_default());
            if head.len() >= HEAD_BYTES {
                if type_of(&head, claimed, &slot.types).is_none() {
                    return Err(TYPE_REFUSED);
                }
                checked = true;
            }
        }
        if let Err(refused) = sink.write(&piece) {
            return Err(refused_answer(refused));
        }
    }
    if !checked && type_of(&head, claimed, &slot.types).is_none() {
        return Err(TYPE_REFUSED);
    }
    let hash = hasher.finalize().to_hex();
    sink.finish(&Kept {
        size,
        hash: hash.as_str(),
        name: name.as_deref(),
        claimed,
    })
    .map_err(refused_answer)
}

fn refused_answer(refused: Refused) -> (u16, &'static str, &'static str) {
    match refused {
        Refused::Full => STORAGE_FULL,
        Refused::Taken => ALREADY_SENT,
        Refused::Unavailable => STORE_UNAVAILABLE,
    }
}

/// A body read a piece at a time: what the host streams, or what it read
/// whole.
enum Pieces<'r> {
    Streamed(&'r super::BodyStream),
    Whole(Option<&'r [u8]>),
}

impl<'r> Pieces<'r> {
    fn of(request: &'r Request) -> Pieces<'r> {
        match &request.stream {
            Some(stream) => Pieces::Streamed(stream),
            None => Pieces::Whole(Some(&request.body)),
        }
    }

    fn next(&mut self) -> LocalFuture<'_, Option<Result<Vec<u8>, ()>>> {
        match self {
            Pieces::Streamed(stream) => stream.next(),
            Pieces::Whole(whole) => {
                let piece = whole.take().map(|bytes| Ok(bytes.to_vec()));
                Box::pin(async move { piece })
            }
        }
    }
}

/// A body that gives its bytes and then ends before its end, as a client
/// that goes away does: a route test's `dropped: true`.
struct Dropped(std::cell::RefCell<Option<Vec<u8>>>);

impl super::BodySource for Dropped {
    fn next(&self) -> LocalFuture<'_, Option<Result<Vec<u8>, ()>>> {
        let piece = self.0.borrow_mut().take();
        Box::pin(async move { Some(piece.ok_or(())) })
    }
}

/// A route test's `request { upload: ... }` (area 13, "Route tests"): the
/// file's request, through the HTTP component, as a client sends it. `args`
/// are the slot (an `UploadSlot` or its address as text), the body, its
/// `Content-Type`, `Content-Encoding` and file name, the part it is and
/// whether its connection is dropped before its end, each `none` when not
/// said.
pub fn upload_test<'a, 'b, 'p>(
    app: &'a super::App<'b>,
    executor: &'a dyn crate::Executor,
    ctx: &'a mut Ctx<'p>,
    args: &'a [Value],
) -> LocalFuture<'a, Result<Value, Box<crate::Fault>>> {
    Box::pin(sent_in_test(app, executor, ctx, args))
}

/// [`upload_test`]'s work, boxed so an executor's own frame stays small.
async fn sent_in_test(
    app: &super::App<'_>,
    executor: &dyn crate::Executor,
    ctx: &mut Ctx<'_>,
    args: &[Value],
) -> Result<Value, Box<crate::Fault>> {
    let part = match args.get(5) {
        Some(Value::Int(part)) => Some(*part),
        _ => None,
    };
    let path = match args.first() {
        Some(Value::Text(path)) => Some(path.to_string()),
        Some(Value::Record(record)) => {
            let address = |name: &str| match record.get(name) {
                Some(Value::Text(url)) => Some(url.to_string()),
                _ => None,
            };
            address("url").or_else(|| address("parts_url"))
        }
        _ => None,
    };
    let path = path.map(|path| match (part, args.first()) {
        (Some(part), Some(Value::Record(record)))
            if matches!(record.get("parts_url"), Some(Value::Text(_))) =>
        {
            format!("{path}/{part}")
        }
        (Some(part), _) => format!("{path}/{PARTS}/{part}"),
        (None, _) => path,
    });
    let (Some(path), Some(Value::Bytes(body))) = (path, args.get(1)) else {
        return Err(ctx.fault(
            crate::FaultKind::Defect("an upload given no slot or no body".to_string()),
            0,
        ));
    };
    let mut headers = Vec::new();
    let mut header = |name: &str, value: Option<&Value>| {
        if let Some(Value::Text(value)) = value {
            headers.push((name.to_string(), value.to_string()));
        }
    };
    header("content-type", args.get(2));
    header("content-encoding", args.get(3));
    if let Some(Value::Text(name)) = args.get(4) {
        header(
            "content-disposition",
            Some(&Value::text(&format!(
                "attachment; filename*=UTF-8''{}",
                super::downloads::filename_value(name)
            ))),
        );
    }
    headers.push(("content-length".to_string(), body.len().to_string()));
    let dropped = matches!(args.get(6), Some(Value::Bool(true)));
    let request = Request {
        method: "PUT".to_string(),
        path,
        headers,
        body: if dropped { Vec::new() } else { body.to_vec() },
        stream: dropped.then(|| {
            super::BodyStream::new(Rc::new(Dropped(std::cell::RefCell::new(Some(
                body.to_vec(),
            )))))
        }),
        ..Request::default()
    };
    let id = super::request_id(ctx, u64::from(ctx.actions()));
    let mut response = super::handle(app, executor, ctx, &request, &id).await;
    if let Some(fault) = response.fault.take() {
        return Err(fault);
    }
    Ok(Value::record(
        "Response",
        vec![
            (Rc::from("status"), Value::Int(i64::from(response.status))),
            (
                Rc::from("code"),
                response.code.as_deref().map_or(Value::None, Value::text),
            ),
        ],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot() -> Slot {
        Slot {
            key: jadpo_std::files::key_for(&[9; 16]),
            field: "PriceSheet.sheet".to_string(),
            max_size: 5_000,
            types: vec!["csv".to_string()],
            expires: 1_000,
            part_size: None,
        }
    }

    #[test]
    fn a_token_opens_only_as_it_was_signed() {
        let key = [1u8; 32];
        let token = sign(&key, &slot());
        assert!(token.starts_with("upl_"), "{token}");
        assert_eq!(open(&key, &token), Some(slot()));
        // Another store's, or nothing: the same answer, nothing read.
        assert_eq!(open(&[2u8; 32], &token), None);
        assert_eq!(open(&key, "upl_nothing"), None);
        assert_eq!(open(&key, "nothing"), None);
        assert_eq!(open(&key, ""), None);
        // Each part of it counts.
        let mut other = slot();
        other.field = "PriceSheet.other".to_string();
        let second = sign(&key, &other);
        let (body_a, tag_a) = token.split_once('.').unwrap();
        let (body_b, tag_b) = second.split_once('.').unwrap();
        assert_eq!(open(&key, &format!("{body_a}.{tag_b}")), None);
        assert_eq!(open(&key, &format!("{body_b}.{tag_a}")), None);
    }

    #[test]
    fn a_token_carries_a_slot_in_parts() {
        let key = [1u8; 32];
        let mut parts = slot();
        parts.part_size = Some(16 * 1024 * 1024);
        let token = sign(&key, &parts);
        assert_eq!(open(&key, &token), Some(parts));
        // A plain slot's token never reads as one in parts.
        assert_eq!(open(&key, &sign(&key, &slot())).unwrap().part_size, None);
    }

    #[test]
    fn a_part_number_is_digits_from_one_with_no_leading_zero() {
        assert_eq!(part_number("1"), Some(1));
        assert_eq!(part_number("120"), Some(120));
        for wrong in ["0", "01", "-1", "1a", "", "99999999999999999999999"] {
            assert_eq!(part_number(wrong), None, "{wrong}");
        }
    }

    #[test]
    fn a_signed_token_names_only_what_a_store_can_keep() {
        let key = [1u8; 32];
        let mut hostile = slot();
        hostile.key = "../../etc/passwd".to_string();
        assert_eq!(open(&key, &sign(&key, &hostile)), None);
    }

    #[test]
    fn an_upload_path_is_a_store_and_a_token() {
        assert_eq!(
            split("/files/sheets/uploads/upl_a.b"),
            Some(Address {
                store: "sheets",
                token: "upl_a.b",
                part: None
            })
        );
        assert_eq!(
            split("/files/sheets/uploads/upl_a.b/parts/12"),
            Some(Address {
                store: "sheets",
                token: "upl_a.b",
                part: Some("12")
            })
        );
        assert_eq!(store_of("/files/sheets/uploads/upl_a.b"), Some("sheets"));
        for path in [
            "/files/sheets/upl_a.b",
            "/files/sheets/uploads/",
            "/files/sheets/uploads/a/b",
            "/files/sheets/uploads/a/parts",
            "/files/sheets/uploads/a/parts/",
            "/files/sheets/uploads/a/parts/1/2",
            "/files//uploads/a",
            "/other/sheets/uploads/a",
        ] {
            assert_eq!(split(path), None, "{path}");
        }
    }

    #[test]
    fn a_files_name_is_the_clients_without_anything_to_trip_over() {
        let of = |header: &str| file_name(Some(header));
        assert_eq!(
            of("attachment; filename*=UTF-8''caf%C3%A9.csv").as_deref(),
            Some("caf\u{e9}.csv")
        );
        assert_eq!(
            of("attachment; filename=\"a.csv\"").as_deref(),
            Some("a.csv")
        );
        // A path's last part only.
        assert_eq!(
            of("attachment; filename=\"../../etc/passwd\"").as_deref(),
            Some("passwd")
        );
        assert_eq!(
            of("attachment; filename=\"C:\\\\up\\\\a.csv\"").as_deref(),
            Some("a.csv")
        );
        // No control characters, and no name made of none.
        assert_eq!(
            of("attachment; filename*=UTF-8''a%0D%0Ab.csv").as_deref(),
            Some("ab.csv")
        );
        assert_eq!(of("attachment; filename*=UTF-8''%2F"), None);
        assert_eq!(of("attachment"), None);
        assert_eq!(of("attachment; filename*=latin1''a.csv"), None);
        assert_eq!(file_name(None), None);
    }

    #[test]
    fn a_claimed_type_is_a_listed_media_type() {
        assert_eq!(claimed_type(Some("text/csv; charset=utf-8")), Some("csv"));
        assert_eq!(claimed_type(Some("IMAGE/PNG")), Some("png"));
        assert_eq!(claimed_type(Some("text/plain")), None);
        assert_eq!(claimed_type(None), None);
    }
}
