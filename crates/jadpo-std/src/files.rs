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

/// What `store.read` gives (area 16).
pub const FILE_CONTENTS: &str = "FileContents";

/// A `File`'s parts as the runtime keeps them, in the order its JSON writes
/// them: the connection holding its bytes, the key the runtime made for
/// them, the name it was given (none for one the application put), its
/// size in bytes and its type. A program reads none of them yet.
pub const FILE_FIELDS: [(&str, Part); 5] = [
    ("store", Part::Of(Builtin::Text)),
    ("key", Part::Of(Builtin::Text)),
    ("name", Part::Optional(Builtin::Text)),
    ("size", Part::Of(Builtin::Int)),
    ("type", Part::Enum(crate::standard::FILE_TYPE)),
];

/// The fields of the file type called `name`, when it is `File`.
pub fn object_fields(name: &str) -> Option<&'static [(&'static str, Part)]> {
    (name == FILE).then_some(&FILE_FIELDS[..])
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
/// `key` from 0, and it answers `Unit` once the file is whole under `key`
/// and its pieces are gone, or `storage_full`; one with a piece missing,
/// and no file under `key` already, is `not_found`. Joining a file that is
/// already whole does nothing, so a repeat acts once.
pub const JOIN: &str = "join";

/// The capability's operation that drops a cancelled export's file
/// (chunk 6.4b2): its input is `{ key, pieces }`, and it drops the first
/// `pieces` pieces of the file `key` names and, if they were joined, the
/// file, answering `Unit`, whatever was already gone.
pub const DISCARD: &str = "discard";

/// The failure a put answers when its store has no room (area 16, "The
/// disk driver").
pub const STORAGE_FULL: &str = "storage_full";

/// The failure a read answers when its store holds no bytes under the key.
pub const NOT_FOUND: &str = "not_found";

/// A files connection, as the plan gives the hosts: its name, its driver,
/// and, for the disk driver, the folder it keeps files under in
/// production (`jadpo dev` keeps them under `.jadpo/files/<name>`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct FilesConnection {
    pub name: String,
    pub driver: Driver,
    pub path: Option<String>,
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
}
