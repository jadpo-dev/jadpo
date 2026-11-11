//! The S3 driver (area 16, "The S3 driver"): the `files` capability on any
//! S3-compatible service, signed with AWS Signature Version 4 and sent
//! through the [`Fetch`] the host gives, so the same code runs natively
//! and on Workers. Path-style addresses, `host/bucket/key`, on every
//! service.
//!
//! - **Where files are.** A file is kept under its key. A whole upload
//!   waits under `uploads/<key>` until an attach checks it and copies it
//!   to its key, so nothing a client still holds a signed address for can
//!   ever change a file. An upload in parts is one multipart upload, its
//!   id kept in `uploads/<key>.upload` and what its first part said in
//!   `uploads/<key>.said`.
//! - **Compressed uploads.** A presigned `PUT` can't stop a client sending
//!   a `Content-Encoding`, which the service keeps with the bytes, so the
//!   attach reads it from a waiting upload's headers and refuses the upload
//!   before copying it.
//! - **Addresses.** Downloads and whole uploads are presigned `GET`s and
//!   `PUT`s; each part is a presigned `UploadPart` the app redirects to.
//! - **Joining an export's pieces.** Read in order and re-cut into equal
//!   parts, as R2 needs them, and one `PutObject` when it all fits in one.

use std::rc::Rc;

use jadpo_runtime::LocalFuture;
use jadpo_runtime::capability::{Answer, Call, Capability, DirectFiles, PartFile, Refused};
use jadpo_runtime::http::uploads::{claimed_type, file_name};
use jadpo_runtime::provider::{Reading, Request, Response};
use jadpo_runtime::sigv4;
use jadpo_runtime::stream::Chunk;
use jadpo_std::HttpMethod;
use jadpo_std::files::{
    COMPLETE, DELETE, DISCARD, HEAD_BYTES, JOIN, LIST, MAX_PARTS, NOT_FOUND, PART_SIZE,
    PARTS_MISSING, PARTS_UNEQUAL, PUT, PartsRefused, READ, STAT, UPLOAD_COMPRESSED, is_key, key_id,
    parts_fit, piece_key,
};
use jadpo_value::Value;

use crate::package::{Fetch, FetchError};

/// How long one of the driver's requests waits for its answer.
const TIMEOUT_MS: u64 = 30_000;

/// How long an upload of one part's bytes, or a part's join, may take.
const PART_TIMEOUT_MS: u64 = 120_000;

/// The part size an export's pieces are re-cut to when they are joined:
/// R2 needs every part but the last the same size.
const JOIN_PART: usize = PART_SIZE as usize;

/// How many of a listing's parts or keys one request asks for.
const PAGE: u32 = 1000;

/// What a request is signed as sending, for a body that isn't hashed.
const UNSIGNED: &str = "UNSIGNED-PAYLOAD";

/// The prefix of everything an upload leaves before its file is whole.
const UPLOADS: &str = "uploads";

/// What a store's clock reads: milliseconds since 1970.
pub type Clock = Rc<dyn Fn() -> i64>;

/// A connection's bucket and credential: `host` the service's address with
/// no trailing `/`, its secret never logged.
pub struct S3Store {
    fetch: Rc<dyn Fetch>,
    clock: Clock,
    host: String,
    region: String,
    bucket: String,
    key_id: String,
    secret: String,
    /// What a slot's token is signed with, derived from the credential so
    /// the store keeps nothing of its own.
    link_key: [u8; 32],
}

impl std::fmt::Debug for S3Store {
    /// Never the credential.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3Store")
            .field("host", &self.host)
            .field("region", &self.region)
            .field("bucket", &self.bucket)
            .finish_non_exhaustive()
    }
}

/// `host` when it is an address the driver may reach: `https://` and a
/// name, or `http://` and this machine; why not, never its value.
pub fn address_of(host: &str) -> Result<String, String> {
    let trimmed = host.trim_end_matches('/');
    let name = trimmed
        .strip_prefix("https://")
        .or_else(|| {
            trimmed.strip_prefix("http://").filter(|rest| {
                let name = rest.rsplit_once(':').map_or(*rest, |(name, _)| name);
                matches!(name, "localhost" | "127.0.0.1" | "[::1]")
            })
        })
        .ok_or_else(|| "its `host:` isn't `https://` and a name".to_string())?;
    if name.is_empty() || name.contains('/') {
        return Err("its `host:` isn't an address with nothing after its name".to_string());
    }
    Ok(trimmed.to_string())
}

impl S3Store {
    /// The store in `bucket` at `host`, signing with `key_id` and `secret`
    /// for `region`.
    pub fn new(
        fetch: Rc<dyn Fetch>,
        clock: Clock,
        (host, region, bucket): (&str, &str, &str),
        (key_id, secret): (&str, &str),
    ) -> Result<S3Store, String> {
        let host = address_of(host)?;
        if bucket.is_empty() || bucket.contains('/') {
            return Err("its `bucket:` isn't a bucket's name".to_string());
        }
        if region.is_empty() {
            return Err("its `region:` is empty".to_string());
        }
        let mut derive = blake3::Hasher::new_derive_key("jadpo 2026-11-09 s3 store slot key v1");
        derive.update(secret.as_bytes());
        derive.update(b"\0");
        derive.update(bucket.as_bytes());
        Ok(S3Store {
            fetch,
            clock,
            host,
            region: region.to_string(),
            bucket: bucket.to_string(),
            key_id: key_id.to_string(),
            secret: secret.to_string(),
            link_key: *derive.finalize().as_bytes(),
        })
    }

    fn signer(&self) -> sigv4::Signer<'_> {
        sigv4::Signer {
            key_id: &self.key_id,
            secret: &self.secret,
            region: &self.region,
            service: "s3",
        }
    }

    /// The address of object `key`, each segment percent-encoded once.
    fn url(&self, key: &str) -> String {
        let path = key
            .split('/')
            .map(encode_segment)
            .collect::<Vec<_>>()
            .join("/");
        format!("{}/{}/{path}", self.host, self.bucket)
    }

    /// A signed request of `method` to `url`, with `headers` beside what
    /// signing adds, `body` hashed unless `unsigned`.
    fn signed(
        &self,
        method: HttpMethod,
        url: &str,
        headers: Vec<(String, String)>,
        body: Option<Vec<u8>>,
        unsigned: bool,
    ) -> Option<Request> {
        let bytes = body.as_deref().unwrap_or_default();
        let payload = if unsigned {
            UNSIGNED.to_string()
        } else {
            hex(&jadpo_value::hash::sha256(bytes))
        };
        let mut sent = headers;
        sent.push(("x-amz-content-sha256".to_string(), payload.clone()));
        let at = (self.clock)();
        let unsigned_request = sigv4::Unsigned {
            method: method.word(),
            url,
            headers: &sent,
            body: bytes,
            payload: Some(&payload),
        };
        let [date, authorization] = sigv4::sign(&unsigned_request, &self.signer(), at)?;
        sent.push(date);
        sent.push(authorization);
        Some(Request {
            method,
            url: url.to_string(),
            headers: sent,
            body: None,
            raw_body: body,
            timeout_ms: if unsigned {
                PART_TIMEOUT_MS
            } else {
                TIMEOUT_MS
            },
            reading: Reading::default(),
            repeatable: true,
        })
    }

    /// Sends a signed request, the answer whole.
    async fn exchange(&self, request: Option<Request>) -> Result<Response, Answer> {
        let Some(request) = request else {
            return Err(Answer::Misconfigured);
        };
        self.fetch.fetch(&request).await.map_err(unanswered)
    }

    /// The first bytes of a whole upload waiting under `staged`, `None` when
    /// it isn't there, or `upload_compressed` when it says it is compressed:
    /// told from its headers before any of its body is read, since a
    /// Worker's `fetch()` can try to unpack it.
    async fn waiting(&self, staged: &str) -> Result<Option<Ranged>, Answer> {
        let range = format!("bytes=0-{}", HEAD_BYTES - 1);
        let url = self.url(staged);
        let request = self
            .signed(
                HttpMethod::Get,
                &url,
                vec![("range".to_string(), range)],
                None,
                false,
            )
            .ok_or(Answer::Misconfigured)?;
        let mut opened = self.fetch.open(&request).await.map_err(unanswered)?;
        match opened.status {
            200 | 206 | 416 => {}
            404 => return Ok(None),
            status => return Err(classified(status)),
        }
        if opened
            .headers
            .iter()
            .any(|(name, value)| name == "content-encoding" && says_compressed(value))
        {
            return Err(Answer::Failed(UPLOAD_COMPRESSED.into()));
        }
        let mut body = Vec::new();
        loop {
            match opened.body.next().await {
                Chunk::Bytes(bytes) => body.extend_from_slice(&bytes),
                Chunk::End => break,
                Chunk::TimedOut => return Err(Answer::TimedOut),
                Chunk::Broken => return Err(Answer::Unknown),
            }
        }
        let response = Response {
            status: opened.status,
            headers: opened.headers,
            body,
        };
        let size = total_of(&response).unwrap_or(response.body.len() as u64);
        Ok(Some(Ranged { size, response }))
    }

    /// A signed `GET` of `url` with `headers`, the answer whole.
    async fn get(&self, url: &str, headers: Vec<(String, String)>) -> Result<Response, Answer> {
        self.exchange(self.signed(HttpMethod::Get, url, headers, None, false))
            .await
    }

    async fn put_object(&self, key: &str, media: &str, bytes: &[u8]) -> Answer {
        let url = self.url(key);
        let headers = vec![("content-type".to_string(), media.to_string())];
        let request = self.signed(HttpMethod::Put, &url, headers, Some(bytes.to_vec()), false);
        match self.exchange(request).await {
            Ok(response) if ok(response.status) => Answer::Value(Value::Unit),
            Ok(response) => classified(response.status),
            Err(answer) => answer,
        }
    }

    /// A range of object `key`'s bytes, its headers with it: `None` when
    /// it isn't there.
    async fn ranged(&self, key: &str, from: u64, length: u64) -> Result<Option<Ranged>, Answer> {
        let url = self.url(key);
        let range = format!(
            "bytes={from}-{}",
            from.saturating_add(length).saturating_sub(1)
        );
        let response = self.get(&url, vec![("range".to_string(), range)]).await?;
        match response.status {
            200 | 206 => {
                let size = total_of(&response).unwrap_or(response.body.len() as u64);
                Ok(Some(Ranged { size, response }))
            }
            // A range past the end, an empty object among them.
            416 => {
                let size = total_of(&response).unwrap_or(0);
                Ok(Some(Ranged { size, response }))
            }
            404 => Ok(None),
            status => Err(classified(status)),
        }
    }

    async fn read(&self, key: &str, from: u64, length: u64) -> Answer {
        if length == 0 {
            return Answer::Value(Value::Bytes(Rc::from(Vec::new())));
        }
        match self.ranged(key, from, length).await {
            Ok(Some(ranged)) => {
                let bytes = if ranged.response.status == 416 {
                    Vec::new()
                } else {
                    ranged.response.body
                };
                Answer::Value(Value::Bytes(Rc::from(bytes)))
            }
            Ok(None) => Answer::Failed(NOT_FOUND.into()),
            Err(answer) => answer,
        }
    }

    /// `DELETE`s object `key`: what is already gone is gone.
    async fn remove(&self, key: &str) -> Result<(), Answer> {
        let url = self.url(key);
        let response = self
            .exchange(self.signed(HttpMethod::Delete, &url, Vec::new(), None, false))
            .await?;
        if ok(response.status) || response.status == 404 {
            Ok(())
        } else {
            Err(classified(response.status))
        }
    }

    /// The id of upload `key`'s multipart upload, when one is under way.
    async fn upload_id(&self, key: &str) -> Result<Option<String>, Answer> {
        let response = self.get(&self.url(&upload_key(key)), Vec::new()).await?;
        match response.status {
            200 => Ok(Some(String::from_utf8_lossy(&response.body).into_owned())),
            404 => Ok(None),
            status => Err(classified(status)),
        }
    }

    /// Deletes file `key` and everything an upload left of it: its
    /// staging object, what it said of itself, and any multipart upload.
    async fn delete(&self, key: &str) -> Answer {
        let aborted = match self.upload_id(key).await {
            Ok(Some(id)) => {
                let url = format!("{}?uploadId={}", self.url(key), encode_segment(&id));
                match self
                    .exchange(self.signed(HttpMethod::Delete, &url, Vec::new(), None, false))
                    .await
                {
                    Ok(_) => Ok(()),
                    Err(answer) => Err(answer),
                }
            }
            Ok(None) => Ok(()),
            Err(answer) => Err(answer),
        };
        if let Err(answer) = aborted {
            return answer;
        }
        for target in [
            key.to_string(),
            staged_key(key),
            said_key(key),
            upload_key(key),
        ] {
            if let Err(answer) = self.remove(&target).await {
                return answer;
            }
        }
        Answer::Value(Value::Unit)
    }

    /// What an upload left under `key`: the file itself, or a whole upload
    /// still waiting, copied to its key now its size is within `max_size`,
    /// so its bytes can't change (area 16, "The S3 driver").
    async fn stat(&self, key: &str, max_size: u64) -> Answer {
        let mut found = match self.ranged(key, 0, HEAD_BYTES as u64).await {
            Ok(found) => found,
            Err(answer) => return answer,
        };
        if found.is_none() {
            let staged = staged_key(key);
            let waiting = match self.waiting(&staged).await {
                Ok(Some(waiting)) => waiting,
                Ok(None) => return Answer::Failed(NOT_FOUND.into()),
                Err(answer) => return answer,
            };
            // Over the field's limit it is never copied; the attach refuses it.
            if waiting.size > max_size {
                return self.described(key, &waiting).await;
            }
            if let Err(answer) = self.copy(&staged, key, waiting.etag()).await {
                return answer;
            }
            let _ = self.remove(&staged).await;
            found = match self.ranged(key, 0, HEAD_BYTES as u64).await {
                Ok(found) => found,
                Err(answer) => return answer,
            };
        }
        match found {
            Some(ranged) => self.described(key, &ranged).await,
            None => Answer::Failed(NOT_FOUND.into()),
        }
    }

    /// The server-side copy of `from` to `to`, only of the bytes `etag` names.
    async fn copy(&self, from: &str, to: &str, etag: Option<String>) -> Result<(), Answer> {
        let url = self.url(to);
        let mut headers = vec![(
            "x-amz-copy-source".to_string(),
            format!("/{}/{}", self.bucket, from),
        )];
        if let Some(etag) = etag {
            headers.push(("x-amz-copy-source-if-match".to_string(), etag));
        }
        let response = self
            .exchange(self.signed(HttpMethod::Put, &url, headers, None, false))
            .await?;
        if ok(response.status) && !has_error(&response.body) {
            Ok(())
        } else {
            Err(classified(response.status))
        }
    }

    /// The [`jadpo_std::files::UPLOADED`] of `ranged`: what the client said
    /// of the file, from what its first part said when it came in parts,
    /// else its object's own headers.
    async fn described(&self, key: &str, ranged: &Ranged) -> Answer {
        let said = self.said_file(key).await;
        let (name, claimed) = match said {
            Ok(Some(said)) => said,
            Ok(None) => (
                file_name(ranged.response.header("content-disposition")),
                claimed_type(ranged.response.header("content-type")).map(str::to_string),
            ),
            Err(answer) => return answer,
        };
        let head = if ranged.response.status == 416 {
            Vec::new()
        } else {
            ranged
                .response
                .body
                .get(..HEAD_BYTES.min(ranged.response.body.len()))
                .unwrap_or_default()
                .to_vec()
        };
        Answer::Value(jadpo_runtime::files::uploaded(
            &head,
            ranged.size,
            None,
            name.as_deref(),
            claimed.as_deref(),
        ))
    }

    /// What upload `key`'s first part said of it: `None` when no part did.
    async fn said_file(
        &self,
        key: &str,
    ) -> Result<Option<(Option<String>, Option<String>)>, Answer> {
        let response = self.get(&self.url(&said_key(key)), Vec::new()).await?;
        match response.status {
            200 => {
                let Ok(serde_json::Value::Object(said)) =
                    serde_json::from_slice::<serde_json::Value>(&response.body)
                else {
                    return Err(Answer::Unavailable);
                };
                let text = |name: &str| {
                    said.get(name)
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string)
                };
                Ok(Some((text("n"), text("c"))))
            }
            404 => Ok(None),
            status => Err(classified(status)),
        }
    }

    /// Joins the parts of upload `key` once every part is there and they
    /// are equal: `CompleteMultipartUpload`. Done already, it does nothing.
    async fn complete(&self, key: &str, part_size: u64) -> Answer {
        match self.ranged(key, 0, 1).await {
            Ok(Some(_)) => return Answer::Value(Value::Unit),
            Ok(None) => {}
            Err(answer) => return answer,
        }
        let id = match self.upload_id(key).await {
            Ok(Some(id)) => id,
            Ok(None) => return Answer::Failed(PARTS_MISSING.into()),
            Err(answer) => return answer,
        };
        let parts = match self.list_parts(key, &id).await {
            Ok(parts) => parts,
            Err(answer) => return answer,
        };
        // Numbered from 1 with no gaps, and equal.
        let mut sizes = Vec::with_capacity(parts.len());
        let mut gaps = false;
        for (expected, part) in (1u64..).zip(parts.iter()) {
            if part.number != expected {
                gaps = true;
                break;
            }
            sizes.push(part.size);
        }
        match parts_fit(&sizes, gaps, part_size) {
            Ok(()) => {}
            Err(PartsRefused::Missing) => return Answer::Failed(PARTS_MISSING.into()),
            Err(PartsRefused::Unequal) => return Answer::Failed(PARTS_UNEQUAL.into()),
        }
        if let Err(answer) = self.complete_upload(key, &id, &parts).await {
            return answer;
        }
        let _ = self.remove(&upload_key(key)).await;
        Answer::Value(Value::Unit)
    }

    /// `CompleteMultipartUpload`, of `parts` in order.
    async fn complete_upload(&self, key: &str, id: &str, parts: &[Part]) -> Result<(), Answer> {
        let mut xml = String::from("<CompleteMultipartUpload>");
        for part in parts {
            xml.push_str(&format!(
                "<Part><PartNumber>{}</PartNumber><ETag>{}</ETag></Part>",
                part.number,
                xml_escape(&part.etag)
            ));
        }
        xml.push_str("</CompleteMultipartUpload>");
        let url = format!("{}?uploadId={}", self.url(key), encode_segment(id));
        let response = self
            .exchange(self.signed(
                HttpMethod::Post,
                &url,
                vec![("content-type".to_string(), "application/xml".to_string())],
                Some(xml.into_bytes()),
                false,
            ))
            .await?;
        if ok(response.status) && !has_error(&response.body) {
            Ok(())
        } else {
            Err(classified(response.status))
        }
    }

    /// Every part of multipart upload `id` of `key`, in order.
    async fn list_parts(&self, key: &str, id: &str) -> Result<Vec<Part>, Answer> {
        let mut parts = Vec::new();
        let mut marker: Option<String> = None;
        loop {
            let mut url = format!(
                "{}?max-parts={PAGE}&uploadId={}",
                self.url(key),
                encode_segment(id)
            );
            if let Some(marker) = &marker {
                url.push_str(&format!("&part-number-marker={marker}"));
            }
            let response = self.get(&url, Vec::new()).await?;
            if !ok(response.status) {
                return Err(classified(response.status));
            }
            let text = String::from_utf8_lossy(&response.body).into_owned();
            for block in xml_all(&text, "Part") {
                let number = xml_text(block, "PartNumber")
                    .and_then(|number| number.parse().ok())
                    .unwrap_or(0);
                let size = xml_text(block, "Size")
                    .and_then(|size| size.parse().ok())
                    .unwrap_or(0);
                let etag = xml_text(block, "ETag")
                    .map(xml_unescape)
                    .unwrap_or_default();
                parts.push(Part { number, size, etag });
            }
            if xml_text(&text, "IsTruncated") != Some("true") {
                break;
            }
            marker = xml_text(&text, "NextPartNumberMarker").map(str::to_string);
            if marker.is_none() || parts.len() as u64 >= MAX_PARTS {
                break;
            }
        }
        parts.sort_by_key(|part| part.number);
        Ok(parts)
    }

    /// Joins an export's `pieces` pieces of `key`, whose id is `id`, into
    /// its one file (area 12, "How an export's run is kept"): read in
    /// order and re-cut into equal parts; a file that fits in one is put
    /// whole. Answers the file's hash once it is whole and its pieces are
    /// gone. Joined already, it does nothing but say so.
    async fn join(&self, key: &str, id: &[u8; 16], pieces: u64, media: &str) -> Answer {
        match self.ranged(key, 0, 1).await {
            Ok(Some(_)) => return Answer::Value(Value::Unit),
            Ok(None) => {}
            Err(answer) => return answer,
        }
        let mut hasher = blake3::Hasher::new();
        let mut buffer: Vec<u8> = Vec::new();
        let mut upload: Option<(String, Vec<Part>)> = None;
        for piece in 0..pieces {
            let piece_name = piece_key(id, piece);
            let response = match self.get(&self.url(&piece_name), Vec::new()).await {
                Ok(response) if response.status == 404 => {
                    return Answer::Failed(NOT_FOUND.into());
                }
                Ok(response) if ok(response.status) => response,
                Ok(response) => return classified(response.status),
                Err(answer) => return answer,
            };
            hasher.update(&response.body);
            buffer.extend_from_slice(&response.body);
            while buffer.len() >= JOIN_PART {
                let rest = buffer.split_off(JOIN_PART);
                let part = std::mem::replace(&mut buffer, rest);
                if let Err(answer) = self.send_part(key, media, &mut upload, part).await {
                    return answer;
                }
            }
        }
        let finished = match &mut upload {
            None => self.put_object(key, media, &buffer).await,
            Some(_) => {
                if !buffer.is_empty() {
                    let last = std::mem::take(&mut buffer);
                    if let Err(answer) = self.send_part(key, media, &mut upload, last).await {
                        return answer;
                    }
                }
                let Some((upload_id, parts)) = &upload else {
                    return Answer::Unavailable;
                };
                match self.complete_upload(key, upload_id, parts).await {
                    Ok(()) => Answer::Value(Value::Unit),
                    Err(answer) => answer,
                }
            }
        };
        if !matches!(finished, Answer::Value(_)) {
            return finished;
        }
        for piece in 0..pieces {
            let _ = self.remove(&piece_key(id, piece)).await;
        }
        Answer::Value(Value::text(&hasher.finalize().to_hex()))
    }

    /// Sends one equal part of a join, starting the multipart upload at
    /// the first.
    async fn send_part(
        &self,
        key: &str,
        media: &str,
        upload: &mut Option<(String, Vec<Part>)>,
        bytes: Vec<u8>,
    ) -> Result<(), Answer> {
        if upload.is_none() {
            let id = self.create_upload(key, Some(media)).await?;
            *upload = Some((id, Vec::new()));
        }
        let Some((id, parts)) = upload else {
            return Err(Answer::Unavailable);
        };
        let number = parts.len() as u64 + 1;
        let size = bytes.len() as u64;
        let url = format!(
            "{}?partNumber={number}&uploadId={}",
            self.url(key),
            encode_segment(id)
        );
        let response = self
            .exchange(self.signed(HttpMethod::Put, &url, Vec::new(), Some(bytes), true))
            .await?;
        if !ok(response.status) {
            return Err(classified(response.status));
        }
        let etag = response.header("etag").unwrap_or_default().to_string();
        parts.push(Part { number, size, etag });
        Ok(())
    }

    /// `CreateMultipartUpload` of `key`, its id.
    async fn create_upload(&self, key: &str, media: Option<&str>) -> Result<String, Answer> {
        let url = format!("{}?uploads", self.url(key));
        let headers = media
            .map(|media| vec![("content-type".to_string(), media.to_string())])
            .unwrap_or_default();
        let response = self
            .exchange(self.signed(HttpMethod::Post, &url, headers, Some(Vec::new()), false))
            .await?;
        if !ok(response.status) {
            return Err(classified(response.status));
        }
        let text = String::from_utf8_lossy(&response.body).into_owned();
        xml_text(&text, "UploadId")
            .map(xml_unescape)
            .ok_or(Answer::Unknown)
    }

    /// Every key of the erasure log kept here (area 15), in order.
    async fn list(&self) -> Answer {
        let prefix = format!("{}/", jadpo_std::erasure::LOG_FOLDER);
        let mut keys: Vec<String> = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let mut url = format!(
                "{}/{}?list-type=2&max-keys={PAGE}&prefix={}",
                self.host,
                self.bucket,
                encode_segment(&prefix)
            );
            if let Some(token) = &token {
                url.push_str(&format!("&continuation-token={}", encode_segment(token)));
            }
            let response = match self.get(&url, Vec::new()).await {
                Ok(response) => response,
                Err(answer) => return answer,
            };
            if !ok(response.status) {
                return classified(response.status);
            }
            let text = String::from_utf8_lossy(&response.body).into_owned();
            for block in xml_all(&text, "Contents") {
                if let Some(key) = xml_text(block, "Key") {
                    let key = xml_unescape(key);
                    if jadpo_std::erasure::log_key_parts(&key).is_some() {
                        keys.push(key);
                    }
                }
            }
            match xml_text(&text, "NextContinuationToken") {
                Some(next) if xml_text(&text, "IsTruncated") == Some("true") => {
                    token = Some(xml_unescape(next));
                }
                _ => break,
            }
        }
        keys.sort();
        keys.dedup();
        Answer::Value(Value::list(
            keys.iter().map(|key| Value::text(key)).collect(),
        ))
    }

    /// A page of the keys the bucket holds after `after`, with their sizes
    /// (area 14, `jadpo files`): one `ListObjectsV2` from `after`, the
    /// files' and the erasure log's kept, an upload still waiting left
    /// out; `next` is the last key the service listed while there are more.
    async fn keys(&self, after: &str) -> Answer {
        let mut url = format!(
            "{}/{}?list-type=2&max-keys={}",
            self.host,
            self.bucket,
            jadpo_std::files::KEYS_PAGE
        );
        if !after.is_empty() {
            url.push_str(&format!("&start-after={}", encode_segment(after)));
        }
        let response = match self.get(&url, Vec::new()).await {
            Ok(response) => response,
            Err(answer) => return answer,
        };
        if !ok(response.status) {
            return classified(response.status);
        }
        let text = String::from_utf8_lossy(&response.body).into_owned();
        let mut keys = Vec::new();
        let mut last = None;
        for block in xml_all(&text, "Contents") {
            let Some(key) = xml_text(block, "Key").map(xml_unescape) else {
                continue;
            };
            let size = xml_text(block, "Size")
                .and_then(|size| size.parse::<u64>().ok())
                .unwrap_or(0);
            if jadpo_std::files::is_listed(&key) {
                keys.push((key.clone(), size));
            }
            last = Some(key);
        }
        let next = last.filter(|_| xml_text(&text, "IsTruncated") == Some("true"));
        Answer::Value(jadpo_runtime::files::keys_answer(&keys, next.as_deref()))
    }

    async fn answer(&self, call: &Call) -> Answer {
        let Value::Record(input) = &call.input else {
            return Answer::Misconfigured;
        };
        if call.operation == LIST {
            return self.list().await;
        }
        if call.operation == jadpo_std::files::KEYS {
            return self.keys(&jadpo_runtime::files::keys_after(input)).await;
        }
        let Some(Value::Text(key)) = input.get("key") else {
            return Answer::Misconfigured;
        };
        let logged = jadpo_std::erasure::log_key_parts(key).is_some()
            && matches!(call.operation.as_str(), PUT | READ);
        if !is_key(key) && !logged {
            return Answer::Failed(NOT_FOUND.into());
        }
        let number = |name: &str| match input.get(name) {
            Some(Value::Int(value)) => u64::try_from(*value).ok(),
            _ => None,
        };
        let media = || match input.get("type") {
            Some(Value::Text(media)) => media.to_string(),
            _ => "application/octet-stream".to_string(),
        };
        match call.operation.as_str() {
            PUT => match input.get("bytes") {
                Some(Value::Bytes(bytes)) => self.put_object(key, &media(), bytes).await,
                _ => Answer::Misconfigured,
            },
            READ => match (number("from"), number("length")) {
                (Some(from), Some(length)) => self.read(key, from, length).await,
                _ => Answer::Misconfigured,
            },
            DELETE => self.delete(key).await,
            STAT => self.stat(key, number("max_size").unwrap_or(u64::MAX)).await,
            COMPLETE => match number("part_size") {
                Some(part_size) => self.complete(key, part_size).await,
                None => Answer::Misconfigured,
            },
            JOIN | DISCARD => match (number("pieces"), key_id(key)) {
                (Some(pieces), Some(id)) => {
                    if call.operation == JOIN {
                        self.join(key, &id, pieces, &media()).await
                    } else {
                        self.discard(key, &id, pieces).await
                    }
                }
                _ => Answer::Misconfigured,
            },
            _ => Answer::Misconfigured,
        }
    }

    /// Drops a cancelled export's pieces and the file they joined, whatever
    /// was already gone.
    async fn discard(&self, key: &str, id: &[u8; 16], pieces: u64) -> Answer {
        if let Err(answer) = self.remove(key).await {
            return answer;
        }
        for piece in 0..pieces {
            if let Err(answer) = self.remove(&piece_key(id, piece)).await {
                return answer;
            }
        }
        Answer::Value(Value::Unit)
    }

    /// The multipart upload `key`'s parts go to: begun once, whoever asks
    /// first, the id written with `If-None-Match: *` so a client racing
    /// its own parts agrees on one.
    async fn begin_parts(&self, key: &str) -> Result<String, Refused> {
        let answered = |answer: Answer| match answer {
            Answer::Failed(code) if code == NOT_FOUND => Refused::Unavailable,
            _ => Refused::Unavailable,
        };
        if let Some(id) = self.upload_id(key).await.map_err(answered)? {
            return Ok(id);
        }
        match self.ranged(key, 0, 1).await {
            Ok(Some(_)) => return Err(Refused::Taken),
            Ok(None) => {}
            Err(answer) => return Err(answered(answer)),
        }
        let id = self.create_upload(key, None).await.map_err(answered)?;
        let url = self.url(&upload_key(key));
        let request = self.signed(
            HttpMethod::Put,
            &url,
            vec![("if-none-match".to_string(), "*".to_string())],
            Some(id.clone().into_bytes()),
            false,
        );
        match self.exchange(request).await {
            Ok(response) if ok(response.status) => Ok(id),
            Ok(response) if response.status == 412 => {
                // Another part got there first: its upload is the one.
                let abort = format!("{}?uploadId={}", self.url(key), encode_segment(&id));
                let _ = self
                    .exchange(self.signed(HttpMethod::Delete, &abort, Vec::new(), None, false))
                    .await;
                self.upload_id(key)
                    .await
                    .map_err(answered)?
                    .ok_or(Refused::Unavailable)
            }
            Ok(_) | Err(_) => Err(Refused::Unavailable),
        }
    }
}

/// A ranged read's answer, and the file's whole size.
struct Ranged {
    size: u64,
    response: Response,
}

impl Ranged {
    fn etag(&self) -> Option<String> {
        self.response.header("etag").map(str::to_string)
    }
}

/// One part of a multipart upload.
struct Part {
    number: u64,
    size: u64,
    etag: String,
}

/// What a request that got no answer is to the call.
fn unanswered(error: FetchError) -> Answer {
    match error {
        FetchError::NotSent(_) => Answer::Unavailable,
        FetchError::NoAnswer(_) => Answer::Unknown,
        FetchError::TimedOut => Answer::TimedOut,
        FetchError::Refused(_) => Answer::Misconfigured,
    }
}

/// Whether a `Content-Encoding` says its bytes are compressed: anything but
/// nothing at all or `identity`.
fn says_compressed(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty() && !value.eq_ignore_ascii_case("identity")
}

/// A status of a request that did what it asked.
fn ok(status: u16) -> bool {
    (200..300).contains(&status)
}

/// What a status the driver has no use for means to the call.
fn classified(status: u16) -> Answer {
    match status {
        401 | 403 => Answer::Misconfigured,
        _ => Answer::Unavailable,
    }
}

/// Whether a `200`'s body is really S3's error, as `CopyObject` and
/// `CompleteMultipartUpload` can send.
fn has_error(body: &[u8]) -> bool {
    body.windows(7).any(|window| window == b"<Error>")
}

/// The object an upload's whole file waits in until an attach checks it.
fn staged_key(key: &str) -> String {
    format!("{UPLOADS}/{key}")
}

/// The object that says which multipart upload an upload is.
fn upload_key(key: &str) -> String {
    format!("{UPLOADS}/{key}.upload")
}

/// The object that says what an upload's first part said of the file.
fn said_key(key: &str) -> String {
    format!("{UPLOADS}/{key}.said")
}

/// `value` percent-encoded as one path segment or query value: everything
/// but the unreserved characters.
fn encode_segment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// A `Content-Range` header's whole size.
fn total_of(response: &Response) -> Option<u64> {
    response
        .header("content-range")?
        .rsplit('/')
        .next()?
        .parse()
        .ok()
}

/// The text between the first `<tag>` and `</tag>` in `xml`.
fn xml_text<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let rest = xml.get(start..)?;
    let end = rest.find(&close)?;
    rest.get(..end)
}

/// Every `<tag>…</tag>`'s text in `xml`.
fn xml_all<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(&open) {
        let Some(after) = rest.get(start + open.len()..) else {
            break;
        };
        let Some(end) = after.find(&close) else {
            break;
        };
        let (Some(inner), Some(next)) = (after.get(..end), after.get(end + close.len()..)) else {
            break;
        };
        out.push(inner);
        rest = next;
    }
    out
}

fn xml_unescape(text: &str) -> String {
    text.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&#34;", "\"")
        .replace("&amp;", "&")
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

impl DirectFiles for S3Store {
    fn download(
        &self,
        key: &str,
        media: &str,
        disposition: &str,
        expires_ms: i64,
        now_ms: i64,
    ) -> Option<String> {
        let url = format!(
            "{}?response-content-disposition={}&response-content-type={}",
            self.url(key),
            encode_segment(disposition),
            encode_segment(media),
        );
        sigv4::presign(
            "GET",
            &url,
            &self.signer(),
            now_ms,
            seconds_between(now_ms, expires_ms),
        )
    }

    fn upload(&self, key: &str, expires_ms: i64, now_ms: i64) -> Option<String> {
        sigv4::presign(
            "PUT",
            &self.url(&staged_key(key)),
            &self.signer(),
            now_ms,
            seconds_between(now_ms, expires_ms),
        )
    }

    fn part<'a>(
        &'a self,
        key: &'a str,
        part: u64,
        file: PartFile<'a>,
        expires_ms: i64,
        now_ms: i64,
    ) -> LocalFuture<'a, Result<String, Refused>> {
        Box::pin(async move {
            let id = self.begin_parts(key).await?;
            // What the first part said of the file, kept beside it, since
            // `UploadPart` carries neither its name nor its claimed type.
            if part == 1 && (file.name.is_some() || file.claimed.is_some()) {
                let body = serde_json::json!({ "n": file.name, "c": file.claimed }).to_string();
                let request = self.signed(
                    HttpMethod::Put,
                    &self.url(&said_key(key)),
                    Vec::new(),
                    Some(body.into_bytes()),
                    false,
                );
                if !matches!(self.exchange(request).await, Ok(response) if ok(response.status)) {
                    return Err(Refused::Unavailable);
                }
            }
            let url = format!(
                "{}?partNumber={part}&uploadId={}",
                self.url(key),
                encode_segment(&id)
            );
            sigv4::presign(
                "PUT",
                &url,
                &self.signer(),
                now_ms,
                seconds_between(now_ms, expires_ms),
            )
            .ok_or(Refused::Unavailable)
        })
    }
}

/// Seconds from `now_ms` until `expires_ms`, at least one.
fn seconds_between(now_ms: i64, expires_ms: i64) -> u64 {
    u64::try_from(expires_ms.saturating_sub(now_ms) / 1000)
        .unwrap_or(1)
        .max(1)
}

impl Capability for S3Store {
    fn call<'a>(&'a self, call: &'a Call) -> LocalFuture<'a, Answer> {
        Box::pin(self.answer(call))
    }

    fn link_key(&self, _connection: &str, _make: bool) -> Option<[u8; 32]> {
        Some(self.link_key)
    }

    fn direct(&self, _connection: &str) -> Option<&dyn DirectFiles> {
        Some(self)
    }
}
