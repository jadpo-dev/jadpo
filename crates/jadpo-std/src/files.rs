//! Files (area 16, "Files"; chunk 6.4b): what the compiler and the runtime
//! both decide of a stored file.
//!
//! - [`FILE`]: a stored file, as a `File` field keeps it and `store.put`
//!   gives it: the connection holding its bytes, the key the runtime made
//!   for them, its name, its size and its type ([`FILE_FIELDS`]). Only
//!   the runtime builds one; it is kept as its JSON.
//! - [`FILE_CONTENTS`]: what `store.read` gives, a file's bytes for
//!   passing on, which the format helpers don't take.
//! - [`size`]: a size literal's bytes (`5mb`), and [`WHOLE_READ_LIMIT`],
//!   the most `store.read` reads into memory.
//! - [`FilesOp`]: the operations a program calls on a store, and the
//!   capability's own operations the runtime sends through the seam,
//!   [`PUT`] and [`READ`].
//! - [`key_for`] and [`is_key`]: the keys the runtime makes for a file's
//!   bytes, which a driver refuses any other than (area 16, "The disk
//!   driver").

use crate::batch::Part;
use crate::{Builtin, Driver};

/// A stored file (area 2, "Files").
pub const FILE: &str = "File";

/// The token of a finished upload, in an `Input` (area 16, "Uploading"):
/// text on the wire, which an ordinary create or update attaches.
pub const FILE_UPLOAD: &str = "FileUpload";

/// What `Entity.field_upload()` gives (area 16, "Uploading"): where the
/// client sends the file, and the token that attaches it.
pub const UPLOAD_SLOT: &str = "UploadSlot";

/// An [`UPLOAD_SLOT`]'s parts, in the order its JSON writes them: the
/// signed address the bytes go to (none on a slot in parts), the token an
/// `Input` attaches them by, the most bytes it takes, and on a slot in
/// parts (area 16, "Large uploads go in parts") the size of each part and
/// where they go.
pub const UPLOAD_SLOT_FIELDS: [(&str, Part); 5] = [
    ("url", Part::Optional(Builtin::Text)),
    ("token", Part::Of(Builtin::Text)),
    ("max_size", Part::Of(Builtin::Int)),
    ("part_size", Part::Optional(Builtin::Int)),
    ("parts_url", Part::Optional(Builtin::Text)),
];

/// The [`UPLOAD_SLOT`] parts a slot leaves out of its JSON when they don't
/// apply to it: `url` on a slot in parts, the other two on a plain one.
pub const UPLOAD_SLOT_LEFT_OUT: [&str; 3] = ["url", "part_size", "parts_url"];

/// What `store.read` gives (area 16).
pub const FILE_CONTENTS: &str = "FileContents";

/// A `File`'s parts as the runtime keeps them, in the order its JSON writes
/// them: the connection holding its bytes, the key the runtime made for
/// them, the name it was given (none for one the application put), its
/// size in bytes, its type, and the hash of its bytes (none for a file
/// kept before files were hashed), which its ETag is made from. A program
/// reads none of them yet.
pub const FILE_FIELDS: [(&str, Part); 6] = [
    ("store", Part::Of(Builtin::Text)),
    ("key", Part::Of(Builtin::Text)),
    ("name", Part::Optional(Builtin::Text)),
    ("size", Part::Of(Builtin::Int)),
    ("type", Part::Enum(crate::standard::FILE_TYPE)),
    ("hash", Part::Optional(Builtin::Text)),
];

/// What a `File` becomes where it leaves the server (area 16, "Downloading"):
/// a signed link valid for five minutes, with the file's name, size and type.
pub const FILE_LINK: &str = "FileLink";

/// A [`FILE_LINK`]'s parts, in the order its JSON writes them. The store and
/// key a `File` holds aren't among them: they are in `url`'s signature.
pub const FILE_LINK_FIELDS: [(&str, Part); 4] = [
    ("url", Part::Of(Builtin::Text)),
    ("name", Part::Optional(Builtin::Text)),
    ("size", Part::Of(Builtin::Int)),
    ("type", Part::Enum(crate::standard::FILE_TYPE)),
];

/// How the plan's JSON table writes each `File` a type holds, however
/// deep, named as a wrapper around that type which no source can write
/// (chunk B14): as its signed link, what a person's export holds
/// ([`FILE_LINK`]), or as its bytes in Base64, what an operation's `input:`
/// and a webhook's `reply:` send, as `FileContents` is sent. Anywhere else
/// a `File` is written as the record the runtime keeps.
pub const FILES_AS_LINKS: &str = "files as links";

/// See [`FILES_AS_LINKS`].
pub const FILES_AS_BYTES: &str = "files as bytes";

/// The fields of the file type called `name`, when it is `File` or
/// [`FILE_LINK`].
pub fn object_fields(name: &str) -> Option<&'static [(&'static str, Part)]> {
    match name {
        FILE => Some(&FILE_FIELDS[..]),
        FILE_LINK => Some(&FILE_LINK_FIELDS[..]),
        UPLOAD_SLOT => Some(&UPLOAD_SLOT_FIELDS[..]),
        _ => None,
    }
}

/// How long a file's signed link is good for (area 16, "Downloading"):
/// five minutes.
pub const LINK_VALID_MS: i64 = 5 * 60 * 1000;

/// A download's path begins here: `GET /files/<store>/<link>` (area 16,
/// "Downloading").
pub const DOWNLOADS: &str = "files";

/// Whether a file of `FileType`'s media type `media` may be shown in place
/// rather than saved (area 16, "Downloading"): an allow-list of image types
/// a browser can't run anything from. No SVG, which can hold scripts.
pub fn shown_inline(media: &str) -> bool {
    matches!(media, "image/png" | "image/jpeg" | "image/webp")
}

/// What a file of `FileType`'s variant `file_type` ends in, as an
/// attachment with no name of its own is named (area 16, "Attachments").
pub fn extension(file_type: &str) -> Option<&'static str> {
    crate::standard::FILE_TYPES
        .iter()
        .find(|(name, _)| *name == file_type)
        .map(|(name, _)| match *name {
            "json_lines" => "jsonl",
            other => other,
        })
}

/// The most bytes `store.read` reads into memory (area 16, "Storage and
/// formats are separate layers"): 10 MB, a field whose `max_size:` is
/// over it being a compile error that points at the format helpers.
pub const WHOLE_READ_LIMIT: u64 = 10_000_000;

/// The units a size literal is written in, each with its bytes: SI
/// multiples, so `5mb` is five million bytes, as area 16 writes "100 MB"
/// apart from "16 MiB" (chosen in chunk 6.4b).
pub const SIZE_UNITS: [(&str, u64); 4] = [
    ("b", 1),
    ("kb", 1_000),
    ("mb", 1_000_000),
    ("gb", 1_000_000_000),
];

/// The bytes a size literal says (`5mb`, `512kb`): a whole number then a
/// unit; `None` for anything else, a size past what a `u64` holds among
/// them.
pub fn size(text: &str) -> Option<u64> {
    let digits = text.find(|c: char| !c.is_ascii_digit())?;
    let (number, unit) = text.split_at(digits);
    if number.is_empty() {
        return None;
    }
    let (_, bytes) = SIZE_UNITS.iter().find(|(word, _)| *word == unit)?;
    number.parse::<u64>().ok()?.checked_mul(*bytes)
}

/// A size as a person reads it, in the largest unit it is a whole number
/// of: `5 MB`, `512 kB`.
pub fn shown_size(bytes: u64) -> String {
    for (word, unit) in SIZE_UNITS.iter().rev() {
        if *unit > 1 && bytes.is_multiple_of(*unit) {
            let name = match *word {
                "kb" => "kB",
                "mb" => "MB",
                _ => "GB",
            };
            return format!("{} {name}", bytes / unit);
        }
    }
    format!("{bytes} bytes")
}

/// An operation a program calls on a store, through its connection
/// (`imports.put(bytes, FileType.csv)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FilesOp {
    /// `store.put(bytes, type)`: keeps the bytes under a key the runtime
    /// makes, before the action's first write, and gives the `File`.
    Put,
    /// `store.read(file)`: the file's bytes, whole, as `FileContents`.
    Read,
}

impl FilesOp {
    pub const ALL: [FilesOp; 2] = [FilesOp::Put, FilesOp::Read];

    /// As a call writes it after the connection's name.
    pub fn name(self) -> &'static str {
        match self {
            FilesOp::Put => "put",
            FilesOp::Read => "read",
        }
    }

    pub fn from_name(name: &str) -> Option<FilesOp> {
        FilesOp::ALL.into_iter().find(|op| op.name() == name)
    }
}

/// The capability's operation that keeps bytes under a key: its input
/// is `{ key, type, bytes }`, and it answers `Unit`, or `storage_full`.
pub const PUT: &str = "put";

/// The capability's operation that reads a range of a file's bytes: its
/// input is `{ key, from, length }`, and it answers with at most `length`
/// bytes from `from`, fewer only at the file's end or when the driver
/// reads less at once; a file it doesn't hold is `not_found`.
pub const READ: &str = "read";

/// The capability's operation that joins a written file's pieces into the
/// file (area 12, "How an export's run is kept"; chunk 6.4b2): its input
/// is `{ key, pieces, type }`, the pieces kept under [`piece_key`] of
/// `key` from 0, and it answers the hash of the whole file, as text, once
/// the file is whole under `key` and its pieces are gone, or `storage_full`;
/// one with a piece missing,
/// and no file under `key` already, is `not_found`. Joining a file that is
/// already whole does nothing, so a repeat acts once.
pub const JOIN: &str = "join";

/// The capability's operation that drops a cancelled export's file
/// (chunk 6.4b2): its input is `{ key, pieces }`, and it drops the first
/// `pieces` pieces of the file `key` names and, if they were joined, the
/// file, answering `Unit`, whatever was already gone.
pub const DISCARD: &str = "discard";

/// The capability's operation that deletes a file's bytes (area 16,
/// "Deleting and erasing"; chunk B3): its input is `{ key }`, and it
/// answers `Unit` once nothing is kept under `key`, whether or not
/// anything was, so a repeat acts once. Only the runtime sends it, once
/// the record of which files are held says no record holds the file
/// ([`FilesPlan`]).
pub const DELETE: &str = "delete";

/// The capability's operation that lists what the erasure log keeps here
/// (area 15, "What erasure doesn't rewrite"; chunk P4c): its input is empty,
/// and it answers every key [`crate::erasure::log_key`] made that it holds,
/// in order, as a list of text. `put` and `read` take such a key too; only
/// the runtime sends any of them, and no file's key can name one.
pub const LIST: &str = "list";

/// The capability's operation that lists what a store keeps, for `jadpo
/// files` (area 14; area 16, "The disk driver"): its input is
/// `{ after }`, a key or empty text, and it answers [`FILE_KEYS`], at most
/// [`KEYS_PAGE`] of the keys after it, in order, each with its size: every
/// file's key [`key_for`] made and every key of the erasure log
/// ([`is_listed`]), never an upload still waiting or a store's own files.
/// `next` is where the following page starts, `none` after the last.
pub const KEYS: &str = "keys";

/// What [`KEYS`] answers: `{ keys: List<{ key, size }>, next: Text? }`.
pub const FILE_KEYS: &str = "FileKeys";

/// One key [`KEYS`] lists, with its size in bytes.
pub const FILE_KEY: &str = "FileKey";

/// The most keys one [`KEYS`] answers.
pub const KEYS_PAGE: usize = 1000;

/// Whether `key` is one [`KEYS`] lists and `jadpo files copy` copies: a
/// file's, or the erasure log's (area 15, "What erasure doesn't rewrite").
pub fn is_listed(key: &str) -> bool {
    is_key(key) || crate::erasure::log_key_parts(key).is_some()
}

/// The capability's operation that says what an upload left under a key
/// (area 16, "Uploading"): its input is `{ key }`, and it answers with a
/// [`UPLOADED`] of the file's size, hash, name, the type the client claimed
/// and its first [`HEAD_BYTES`] bytes, for the attach that checks them;
/// a key nothing was uploaded to is `not_found`.
pub const STAT: &str = "stat";

/// [`STAT`]'s failure: the upload waiting under the key says it is
/// compressed, so it is never made the file (area 16, "Compressed uploads").
pub const UPLOAD_COMPRESSED: &str = "upload_compressed";

/// Joins an upload's parts into the file its slot names (area 16, "Large
/// uploads go in parts"), in order, once every part is there: what an
/// attach asks before it [`STAT`]s. Input: `key`, `part_size`, `max_size`.
/// Done already, it does nothing.
pub const COMPLETE: &str = "complete";

/// [`COMPLETE`]'s failure: a part is missing.
pub const PARTS_MISSING: &str = "parts_missing";

/// [`COMPLETE`]'s failure: the parts aren't equal.
pub const PARTS_UNEQUAL: &str = "parts_unequal";

/// What [`STAT`] answers.
pub const UPLOADED: &str = "FileUploaded";

/// How many of a file's first bytes its type is told from: enough for
/// every magic number, and for text to be seen to be text.
pub const HEAD_BYTES: usize = 4096;

/// How long an upload slot's address is good for (area 16, "Uploading"):
/// fifteen minutes. The bytes it took stay a day for a record to hold.
pub const SLOT_VALID_MS: i64 = 15 * 60 * 1000;

/// The largest field a single slot serves (area 16, "Large uploads go in
/// parts"): 100 MB, past which a field gets a slot in parts.
pub const SINGLE_SLOT_LIMIT: u64 = 100_000_000;

/// The part size of a slot in parts (area 16, "Large uploads go in
/// parts"): 16 MiB, larger only for a field whose parts would otherwise
/// number over [`MAX_PARTS`].
pub const PART_SIZE: u64 = 16 * 1024 * 1024;

/// The most parts a slot has: what S3 and R2 allow in one upload, so a
/// slot means the same on every store.
pub const MAX_PARTS: u64 = 10_000;

/// How long a slot in parts is good for: six hours, so that a large file
/// over a slow connection can finish, well inside the day its bytes stay
/// for a record to hold.
pub const PARTS_SLOT_VALID_MS: i64 = 6 * 60 * 60 * 1000;

/// An upload's path, after `/files/<store>/` (area 16, "Uploading"):
/// `PUT /files/<store>/uploads/<token>`.
pub const UPLOADS: &str = "uploads";

/// What follows a slot's token on the path of one of its parts:
/// `PUT /files/<store>/uploads/<token>/parts/<n>`.
pub const PARTS: &str = "parts";

/// The size of each part of a slot for a field of `max_size`: [`PART_SIZE`],
/// or the whole number of MiB that keeps its parts within [`MAX_PARTS`].
pub fn part_size_for(max_size: u64) -> u64 {
    const MIB: u64 = 1024 * 1024;
    let needed = max_size
        .div_ceil(MAX_PARTS)
        .div_ceil(MIB)
        .saturating_mul(MIB);
    needed.max(PART_SIZE)
}

/// The largest part number of a slot of `max_size` with parts of
/// `part_size`.
pub fn part_count(max_size: u64, part_size: u64) -> u64 {
    max_size.div_ceil(part_size.max(1)).max(1)
}

/// The most bytes part `number` of a slot of `max_size` with parts of
/// `part_size` may hold: `None` when no such part can exist, a number
/// past the last or part 0.
pub fn part_limit(max_size: u64, part_size: u64, number: u64) -> Option<u64> {
    if number == 0 || number > part_count(max_size, part_size) {
        return None;
    }
    let before = (number - 1).saturating_mul(part_size);
    Some(part_size.min(max_size.saturating_sub(before)))
}

/// Why a slot's parts don't make a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartsRefused {
    /// None were sent, or a number between them is missing.
    Missing,
    /// A part but the last isn't exactly the part size, or the last is
    /// empty or over it.
    Unequal,
}

/// Whether parts of `sizes`, in order from part 1, are the equal parts a
/// slot of `part_size` takes (area 16, "Large uploads go in parts"): every
/// one but the last exactly that size, and the last between one byte and
/// that size. `gaps` is whether a part past the first missing number was
/// sent.
pub fn parts_fit(sizes: &[u64], gaps: bool, part_size: u64) -> Result<(), PartsRefused> {
    let Some((last, before)) = sizes.split_last() else {
        return Err(PartsRefused::Missing);
    };
    if gaps {
        return Err(PartsRefused::Missing);
    }
    let equal = before.iter().all(|size| *size == part_size);
    if !equal || *last == 0 || *last > part_size {
        return Err(PartsRefused::Unequal);
    }
    Ok(())
}

/// The name of a field's generated slot operation, `sheet` for
/// `sheet_upload`, when `operation` is written that way.
pub fn upload_field(operation: &str) -> Option<&str> {
    operation
        .strip_suffix("_upload")
        .filter(|field| !field.is_empty())
}

/// One `Entity.field_upload()` (area 16, "Uploading"): the field's
/// settings the slot is signed for, and who may ask for one.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct UploadPlan {
    /// The field, `PriceSheet.sheet`, which the slot's token is good for
    /// and no other.
    pub field: String,
    /// The connection keeping the field's files.
    pub store: String,
    /// The most bytes the field holds.
    pub max_size: u64,
    /// The `FileType` variants it may be; empty for any.
    pub types: Vec<String>,
    /// Who may ask: the entity's `create` and `update` grants as they can
    /// be told with no record to look at; `None` for anyone at all.
    pub gate: Option<crate::policy::Gate>,
}

/// What `first`, a file's first bytes, is, as `FileType`'s variant: its
/// own magic number when it has one, whatever the client claimed; text
/// otherwise, which `claimed`, or `listed` when it leaves one possibility,
/// says which text it is. `listed` holds what the field takes, empty for
/// any. `None` when it is none of them, no bytes at all included.
pub fn type_of(first: &[u8], claimed: Option<&str>, listed: &[String]) -> Option<&'static str> {
    if first.is_empty() {
        return None;
    }
    let takes = |variant: &str| listed.is_empty() || listed.iter().any(|name| name == variant);
    if let Some(variant) = magic(first) {
        return takes(variant).then_some(variant);
    }
    if !looks_like_text(first) {
        return None;
    }
    let json = first
        .iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace())
        .is_some_and(|byte| {
            matches!(
                byte,
                b'{' | b'[' | b'"' | b'-' | b'0'..=b'9' | b't' | b'f' | b'n'
            )
        });
    let possible: Vec<&'static str> = crate::standard::FILE_TYPES
        .iter()
        .map(|(variant, _)| *variant)
        .filter(|variant| {
            matches!(*variant, "csv" | "tsv") || (json && matches!(*variant, "json" | "json_lines"))
        })
        .filter(|variant| takes(variant))
        .collect();
    if let Some(claimed) =
        claimed.and_then(|claimed| possible.iter().copied().find(|variant| *variant == claimed))
    {
        return Some(claimed);
    }
    match possible.as_slice() {
        [only] => Some(only),
        _ => None,
    }
}

/// The variant a file's magic number says it is.
fn magic(first: &[u8]) -> Option<&'static str> {
    if first.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("png")
    } else if first.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpeg")
    } else if first.get(..4) == Some(&b"RIFF"[..]) && first.get(8..12) == Some(&b"WEBP"[..]) {
        Some("webp")
    } else if first.starts_with(b"%PDF-") {
        Some("pdf")
    } else {
        None
    }
}

/// Whether `first`, however it was cut short, is UTF-8 with no control
/// bytes but white space: a text file's first bytes.
fn looks_like_text(first: &[u8]) -> bool {
    let bytes = first.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(first);
    let readable = match std::str::from_utf8(bytes) {
        Ok(text) => text,
        // Only a character cut off by the end of the window.
        Err(error) if error.error_len().is_none() => bytes
            .get(..error.valid_up_to())
            .and_then(|valid| std::str::from_utf8(valid).ok())
            .unwrap_or(""),
        Err(_) => return false,
    };
    !readable.is_empty()
        && !readable
            .chars()
            .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t' | '\u{0C}'))
}

/// The event a change that leaves a file held by no record keeps in the
/// outbox (chunk B3), whose generated subscriber deletes the file's bytes
/// ([`crate::subscriber::Target::Files`]): its payload is `{"store", "key"}`,
/// the runtime's own, never a program's.
pub const FILE_RELEASED: &str = "jadpo.FileReleased";

/// How long bytes a put kept are left for a record to hold before they
/// are deleted (area 16, "Writing a file your app makes: `put`"): a day.
pub const UNHELD_KEPT_MS: i64 = 24 * 60 * 60 * 1000;

/// Where each store of files keeps its record of which files its records
/// hold (area 16, "Deleting and erasing"; chunk B3): each database store
/// whose entities' `File` fields name it, which keeps `jadpo_files`
/// ([`crate::system::FILES`]) and the triggers that count each file's
/// holders among its own rows as they change. A store of files no
/// entity's field names has none, and nothing it keeps is deleted.
///
/// When fields in entities of several databases name one store of files,
/// each database counts its own records' holders, and a file's bytes go
/// only once none of them holds it: the first database, the one a put
/// records its file in, is where deleting one file is decided, one
/// deletion at a time.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct FilesPlan {
    /// By the files connection's name, the database stores counting its
    /// files' holders: the first entity's store, in entity name order,
    /// then the rest in name order.
    pub counters: std::collections::BTreeMap<String, Vec<String>>,
}

impl FilesPlan {
    /// The database store a put of `connection`'s files records them in,
    /// and where deleting one is decided.
    pub fn counter(&self, connection: &str) -> Option<&str> {
        self.counters
            .get(connection)
            .and_then(|stores| stores.first())
            .map(String::as_str)
    }

    /// Every database store counting the holders of `connection`'s files.
    pub fn counting(&self, connection: &str) -> &[String] {
        self.counters.get(connection).map_or(&[], Vec::as_slice)
    }

    /// Whether database store `store` keeps a record of files.
    pub fn keeps(&self, store: &str) -> bool {
        self.counters
            .values()
            .any(|stores| stores.iter().any(|counter| counter == store))
    }

    /// Every database store keeping a record of files, once each, in name
    /// order.
    pub fn stores(&self) -> Vec<&str> {
        let mut out: Vec<&str> = self
            .counters
            .values()
            .flatten()
            .map(String::as_str)
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// The generated subscriber that deletes `connection`'s unheld bytes:
    /// `files sheets`.
    pub fn subscriber(connection: &str) -> String {
        format!("files {connection}")
    }
}

/// The failure a put answers when its store has no room (area 16, "The
/// disk driver").
pub const STORAGE_FULL: &str = "storage_full";

/// `fixture_file("customers.csv", store: imports)` in a test (area 13,
/// "Files"): a real file from the test's domain's [`FIXTURE_FOLDER`],
/// kept in the test's in-memory store as a stored `File`.
pub const FIXTURE_FILE: &str = "fixture_file";

/// The folder of a domain a test's fixture files are read from:
/// `domains/<domain>/fixtures/files/`.
pub const FIXTURE_FOLDER: &str = "fixtures/files";

/// The `FileType` variant of a fixture file named `name`, from what its
/// name ends in (`customers.csv` is `csv`, `photo.jpg` is `jpeg`), or
/// `None` for an ending no `FileType` has.
pub fn fixture_type(name: &str) -> Option<&'static str> {
    let (_, ending) = name.rsplit_once('.')?;
    let ending = ending.to_ascii_lowercase();
    let variant = match ending.as_str() {
        "jpg" => "jpeg",
        "jsonl" => "json_lines",
        other => other,
    };
    crate::standard::FILE_TYPES
        .iter()
        .find(|(name, _)| *name == variant)
        .map(|(name, _)| *name)
}

/// Whether `name` names a file below a fixture folder: one or more parts
/// split by `/`, none empty, none `.` or `..` and none starting with a dot,
/// so it never names a file outside the folder or one a project doesn't
/// hold.
pub fn is_fixture_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('\\')
        && name
            .split('/')
            .all(|part| !part.is_empty() && !part.starts_with('.'))
}

/// What a test's `fakes:` tells a store of files to answer, written
/// `fakes: { imports: unavailable }` (area 13, "Files"): every call the
/// store is sent answers `unavailable`, or every write `storage_full`,
/// for the rest of the test. The operation it is scripted as is
/// [`WHOLE_STORE`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum StoreFails {
    /// The store can't be reached: every read and every write answers
    /// `unavailable`.
    Unavailable,
    /// The store has no room: every write answers `storage_full`, and
    /// reads go on.
    StorageFull,
}

impl StoreFails {
    pub const ALL: [StoreFails; 2] = [StoreFails::Unavailable, StoreFails::StorageFull];

    /// As a test writes it.
    pub fn word(self) -> &'static str {
        match self {
            StoreFails::Unavailable => "unavailable",
            StoreFails::StorageFull => STORAGE_FULL,
        }
    }

    pub fn named(word: &str) -> Option<StoreFails> {
        StoreFails::ALL
            .into_iter()
            .find(|fails| fails.word() == word)
    }
}

/// The operation a store's `fakes:` entry is scripted as: the store as a
/// whole, not one of its operations.
pub const WHOLE_STORE: &str = "";

/// The failure a read answers when its store holds no bytes under the key.
pub const NOT_FOUND: &str = "not_found";

/// A disk store's free-space floor when `free_space_floor:` gives none
/// (area 16, "The disk driver"): 5%, in millionths of the disk.
pub const DEFAULT_FREE_SPACE_FLOOR: u32 = 50_000;

/// A free-space floor's whole disk, in millionths: what 100% is.
pub const WHOLE_DISK: u32 = 1_000_000;

/// A files connection, as the plan gives the hosts: its name, its driver,
/// and, for the disk driver, the folder it keeps files under in
/// production (`jadpo dev` keeps them under `.jadpo/files/<name>`) and
/// its free-space floor, in millionths of the disk ([`WHOLE_DISK`]).
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct FilesConnection {
    pub name: String,
    pub driver: Driver,
    pub path: Option<String>,
    pub floor: u32,
}

/// The key the bytes of a file whose new id is `id` are kept under:
/// sharded into two directory levels by its first two bytes, then the id,
/// `3f/a2/3fa2…-…`.
pub fn key_for(id: &[u8; 16]) -> String {
    let hex: String = id.iter().map(|byte| format!("{byte:02x}")).collect();
    let part = |from: usize, to: usize| hex.get(from..to).unwrap_or_default();
    format!(
        "{}/{}/{}-{}-{}-{}-{}",
        part(0, 2),
        part(2, 4),
        part(0, 8),
        part(8, 12),
        part(12, 16),
        part(16, 20),
        part(20, 32)
    )
}

/// The key of piece `piece` of the file whose new id is `id`, as an
/// export writes them (chunk 6.4b2): a key [`key_for`] makes, from the
/// file's id with its last eight bytes turned by the piece's number, so
/// a driver joining a file finds its pieces from its key alone.
pub fn piece_key(id: &[u8; 16], piece: u64) -> String {
    let mut held = *id;
    let turn = (piece.wrapping_add(1)).to_be_bytes();
    for (byte, by) in held.iter_mut().skip(8).zip(turn) {
        *byte ^= by;
    }
    // A piece never shares its file's key, nor another piece's.
    held[6] ^= 0x80;
    key_for(&held)
}

/// The id a key [`key_for`] made holds; `None` for any other.
pub fn key_id(key: &str) -> Option<[u8; 16]> {
    if !is_key(key) {
        return None;
    }
    let hex: String = key.rsplit('/').next()?.split('-').collect();
    let mut id = [0u8; 16];
    for (at, byte) in id.iter_mut().enumerate() {
        *byte = u8::from_str_radix(hex.get(at * 2..at * 2 + 2)?, 16).ok()?;
    }
    Some(id)
}

/// Whether `key` is one [`key_for`] makes: a driver refuses any other, so
/// no key names a path outside its own.
pub fn is_key(key: &str) -> bool {
    let parts: Vec<&str> = key.split('/').collect();
    let [first, second, id] = parts.as_slice() else {
        return false;
    };
    let hex = |text: &str| {
        text.bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    };
    let groups: Vec<&str> = id.split('-').collect();
    let shaped = groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(group, length)| group.len() == length && hex(group));
    first.len() == 2
        && second.len() == 2
        && hex(first)
        && hex(second)
        && shaped
        && id.get(0..2) == Some(*first)
        && id.get(2..4) == Some(*second)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_size_literal_is_a_whole_number_of_an_si_unit() {
        assert_eq!(size("5mb"), Some(5_000_000));
        assert_eq!(size("512kb"), Some(512_000));
        assert_eq!(size("10mb"), Some(WHOLE_READ_LIMIT));
        assert_eq!(size("2gb"), Some(2_000_000_000));
        assert_eq!(size("700b"), Some(700));
        for wrong in ["mb", "5", "5MB", "5.5mb", "5 mb", "5tb", "-5mb"] {
            assert_eq!(size(wrong), None, "{wrong}");
        }
        assert_eq!(size("99999999999999gb"), None);
        assert_eq!(shown_size(5_000_000), "5 MB");
        assert_eq!(shown_size(512_000), "512 kB");
        assert_eq!(shown_size(1_500_000), "1500 kB");
        assert_eq!(shown_size(7), "7 bytes");
    }

    #[test]
    fn a_key_is_sharded_by_its_ids_first_bytes_and_nothing_else_is_one() {
        let id = [
            0x3f, 0xa2, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb,
            0xcc, 0xdd,
        ];
        let key = key_for(&id);
        assert_eq!(key, "3f/a2/3fa20011-2233-4455-6677-8899aabbccdd");
        assert!(is_key(&key));
        for wrong in [
            "../a2/3fa20011-2233-4455-6677-8899aabbccdd",
            "3f/a2/../../etc/passwd",
            "3f/a3/3fa20011-2233-4455-6677-8899aabbccdd",
            "3F/A2/3FA20011-2233-4455-6677-8899AABBCCDD",
            "3f/a2/3fa20011-2233-4455-6677-8899aabbccdd/x",
            "3fa20011-2233-4455-6677-8899aabbccdd",
            "",
        ] {
            assert!(!is_key(wrong), "{wrong}");
        }
        assert_eq!(key_id(&key), Some(id));
        assert_eq!(key_id("3f/a2/../x"), None);
        let pieces: Vec<String> = (0..3).map(|n| piece_key(&id, n)).collect();
        assert!(pieces.iter().all(|piece| is_key(piece) && *piece != key));
        assert!(pieces[0] != pieces[1] && pieces[1] != pieces[2]);
        assert_eq!(FilesOp::from_name("put"), Some(FilesOp::Put));
        assert_eq!(FilesOp::from_name("delete"), None);
        assert_eq!(
            object_fields(FILE).map(<[(&str, Part)]>::len),
            Some(FILE_FIELDS.len())
        );
        assert_eq!(object_fields("FileContents"), None);
    }

    #[test]
    fn a_files_type_comes_from_its_first_bytes_and_not_from_what_was_claimed() {
        let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0];
        let listed = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<Vec<_>>();
        assert_eq!(type_of(&png, None, &[]), Some("png"));
        // The bytes win over the claim, and the field's list holds.
        assert_eq!(type_of(&png, Some("pdf"), &[]), Some("png"));
        assert_eq!(type_of(&png, None, &listed(&["csv"])), None);
        assert_eq!(type_of(b"%PDF-1.7", None, &listed(&["pdf"])), Some("pdf"));
        assert_eq!(type_of(&[0xFF, 0xD8, 0xFF, 0xE0], None, &[]), Some("jpeg"));
        assert_eq!(type_of(b"RIFF1234WEBPVP8 ", None, &[]), Some("webp"));
        // Text: a field that takes only one kind needs no claim.
        assert_eq!(type_of(b"a,b\n1,2\n", None, &listed(&["csv"])), Some("csv"));
        // One that takes any needs one, and only one the bytes could be.
        assert_eq!(type_of(b"a,b\n", None, &[]), None);
        assert_eq!(type_of(b"a,b\n", Some("csv"), &[]), Some("csv"));
        assert_eq!(type_of(b"a,b\n", Some("json"), &[]), None);
        assert_eq!(type_of(b"{\"a\":1}", Some("json"), &[]), Some("json"));
        assert_eq!(type_of(b"", None, &listed(&["csv"])), None);
        // A control byte, or bytes that aren't UTF-8, aren't text.
        assert_eq!(type_of(b"a\0b", Some("csv"), &listed(&["csv"])), None);
        assert_eq!(type_of(&[0xC3, 0x28], Some("csv"), &listed(&["csv"])), None);
        // A character cut off by the window's end is still text.
        assert_eq!(
            type_of(
                "caf\u{e9}".as_bytes().split_last().unwrap().1,
                None,
                &listed(&["csv"])
            ),
            Some("csv")
        );
    }

    #[test]
    fn a_fields_slot_is_named_after_it() {
        assert_eq!(upload_field("sheet_upload"), Some("sheet"));
        assert_eq!(upload_field("big_sheet_upload"), Some("big_sheet"));
        assert_eq!(upload_field("_upload"), None);
        assert_eq!(upload_field("upload"), None);
        assert_eq!(upload_field("get"), None);
    }

    #[test]
    fn a_file_types_extension_is_how_its_files_are_named() {
        for (name, _) in crate::standard::FILE_TYPES {
            let extension = extension(name).expect("an extension");
            assert!(
                extension.chars().all(|c| c.is_ascii_alphanumeric()),
                "{name}"
            );
        }
        assert_eq!(extension("pdf"), Some("pdf"));
        assert_eq!(extension("json_lines"), Some("jsonl"));
        assert_eq!(extension("nope"), None);
    }
}

/// An address the runtime answers for a store of files, ahead of every
/// route (area 16, "Uploading" and "Downloading").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileAddress {
    /// `PUT /files/<store>/uploads/<token>`: a slot's file.
    Upload,
    /// `PUT /files/<store>/uploads/<token>/parts/<n>`: one part of a large
    /// file.
    UploadPart,
    /// `GET /files/<store>/<link>`: a file's signed link.
    Download,
}

impl FileAddress {
    pub const ALL: [FileAddress; 3] = [
        FileAddress::Upload,
        FileAddress::UploadPart,
        FileAddress::Download,
    ];

    pub fn method(self) -> crate::HttpMethod {
        match self {
            FileAddress::Download => crate::HttpMethod::Get,
            _ => crate::HttpMethod::Put,
        }
    }

    /// Its path for `store`, each placeholder in braces.
    pub fn path(self, store: &str) -> String {
        match self {
            FileAddress::Upload => format!("/{DOWNLOADS}/{store}/{UPLOADS}/{{token}}"),
            FileAddress::UploadPart => {
                format!("/{DOWNLOADS}/{store}/{UPLOADS}/{{token}}/{PARTS}/{{part}}")
            }
            FileAddress::Download => format!("/{DOWNLOADS}/{store}/{{link}}"),
        }
    }

    /// What `jadpo routes` says it runs.
    pub fn label(self) -> &'static str {
        match self {
            FileAddress::Upload => "file upload",
            FileAddress::UploadPart => "file upload part",
            FileAddress::Download => "file download",
        }
    }
}
