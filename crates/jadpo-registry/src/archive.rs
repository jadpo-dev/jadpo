//! A release's archive, as `GET archives/{digest}` serves it: a POSIX
//! ustar archive of the release's regular files, whose files, unpacked,
//! have the release's digest as the lock computes it.
//!
//! Packing is deterministic (mode 0644, owner 0, time 0, paths in byte
//! order). Unpacking refuses anything but a regular file at a plain
//! relative path inside the package: no links, folders, devices, pax or
//! GNU extensions, absolute paths, `.` or `..`, hidden files or a path
//! given twice, so an archive can't write outside the folder it is
//! unpacked into, and refuses a damaged archive or one past a size limit.

use std::fmt;

const BLOCK: usize = 512;

/// The most an archive unpacks to by default: the publishing bar's limit
/// on a package's size is checked by the verifier; this bounds what one
/// download can make a client hold.
pub const LIMIT: usize = 64 * 1024 * 1024;

/// Why an archive couldn't be packed or unpacked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArchiveError {
    BadPath(String),
    Duplicate(String),
    NotAFile(String),
    Damaged(&'static str),
    TooLarge,
}

impl fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArchiveError::BadPath(path) => write!(f, "`{path}` isn't a path inside a package"),
            ArchiveError::Duplicate(path) => write!(f, "`{path}` is in the archive twice"),
            ArchiveError::NotAFile(path) => write!(f, "`{path}` isn't a regular file"),
            ArchiveError::Damaged(why) => write!(f, "the archive is damaged: {why}"),
            ArchiveError::TooLarge => f.write_str("the archive unpacks past the size limit"),
        }
    }
}

impl std::error::Error for ArchiveError {}

/// Whether `path` is a plain relative path inside a package: `/`-separated
/// parts, none empty, `.`, `..` or hidden, with no `\` or control
/// characters, at most 255 bytes.
pub fn is_package_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 255
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|part| !part.is_empty() && !part.starts_with('.'))
}

/// Packs `files` into an archive.
pub fn pack(files: &[(String, Vec<u8>)]) -> Result<Vec<u8>, ArchiveError> {
    let mut sorted: Vec<&(String, Vec<u8>)> = files.iter().collect();
    sorted.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    for pair in sorted.windows(2) {
        if pair[0].0 == pair[1].0 {
            return Err(ArchiveError::Duplicate(pair[0].0.clone()));
        }
    }
    let mut out = Vec::new();
    for (path, bytes) in sorted {
        if !is_package_path(path) {
            return Err(ArchiveError::BadPath(path.clone()));
        }
        let (prefix, name) = split_path(path).ok_or_else(|| ArchiveError::BadPath(path.clone()))?;
        let mut header = [0u8; BLOCK];
        header[..name.len()].copy_from_slice(name.as_bytes());
        octal(&mut header[100..108], 0o644);
        octal(&mut header[108..116], 0);
        octal(&mut header[116..124], 0);
        octal(&mut header[124..136], bytes.len() as u64);
        octal(&mut header[136..148], 0);
        header[156] = b'0';
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        header[345..345 + prefix.len()].copy_from_slice(prefix.as_bytes());
        header[148..156].copy_from_slice(b"        ");
        let sum: u64 = header.iter().map(|&b| u64::from(b)).sum();
        let field = format!("{sum:06o}\0 ");
        header[148..156].copy_from_slice(field.as_bytes());
        out.extend_from_slice(&header);
        out.extend_from_slice(bytes);
        out.resize(out.len().div_ceil(BLOCK) * BLOCK, 0);
    }
    out.extend_from_slice(&[0u8; 2 * BLOCK]);
    Ok(out)
}

/// A path as ustar's prefix (up to 155 bytes) and name (up to 100),
/// split at a `/`.
fn split_path(path: &str) -> Option<(&str, &str)> {
    if path.len() <= 100 {
        return Some(("", path));
    }
    path.match_indices('/')
        .map(|(i, _)| (&path[..i], &path[i + 1..]))
        .find(|(prefix, name)| prefix.len() <= 155 && name.len() <= 100 && !name.is_empty())
}

/// Writes `value` as zero-padded octal ended by a NUL, filling `field`.
fn octal(field: &mut [u8], value: u64) {
    let width = field.len() - 1;
    let text = format!("{value:0width$o}");
    field[..width].copy_from_slice(text.as_bytes());
    field[width] = 0;
}

/// Reads an octal field: digits, then NULs or spaces.
fn read_octal(field: &[u8]) -> Option<u64> {
    let start = field.iter().position(|&b| b != b' ')?;
    let digits: Vec<u8> = field[start..]
        .iter()
        .copied()
        .take_while(|b| (b'0'..=b'7').contains(b))
        .collect();
    let rest = &field[start + digits.len()..];
    if digits.is_empty() || !rest.iter().all(|&b| b == 0 || b == b' ') {
        return None;
    }
    u64::from_str_radix(std::str::from_utf8(&digits).ok()?, 8).ok()
}

/// A NUL-ended text field.
fn read_text(field: &[u8]) -> Option<&str> {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    std::str::from_utf8(&field[..end]).ok()
}

/// Unpacks an archive into its files, in the archive's order, within
/// [`LIMIT`].
pub fn unpack(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, ArchiveError> {
    unpack_within(bytes, LIMIT)
}

/// Unpacks an archive whose files come to at most `limit` bytes.
pub fn unpack_within(bytes: &[u8], limit: usize) -> Result<Vec<(String, Vec<u8>)>, ArchiveError> {
    if !bytes.len().is_multiple_of(BLOCK) {
        return Err(ArchiveError::Damaged("its length isn't whole blocks"));
    }
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut total = 0usize;
    let mut at = 0;
    loop {
        let header = bytes
            .get(at..at + BLOCK)
            .ok_or(ArchiveError::Damaged("it ends before its end marker"))?;
        if header.iter().all(|&b| b == 0) {
            let next = bytes
                .get(at + BLOCK..at + 2 * BLOCK)
                .ok_or(ArchiveError::Damaged("its end marker is one block"))?;
            if !next.iter().all(|&b| b == 0) {
                return Err(ArchiveError::Damaged("a block follows an empty header"));
            }
            if !bytes[at + 2 * BLOCK..].iter().all(|&b| b == 0) {
                return Err(ArchiveError::Damaged("data follows its end marker"));
            }
            return Ok(files);
        }
        let stored = read_octal(&header[148..156])
            .ok_or(ArchiveError::Damaged("a header's checksum can't be read"))?;
        let sum: u64 = header
            .iter()
            .enumerate()
            .map(|(i, &b)| {
                if (148..156).contains(&i) {
                    u64::from(b' ')
                } else {
                    u64::from(b)
                }
            })
            .sum();
        if sum != stored {
            return Err(ArchiveError::Damaged("a header's checksum is wrong"));
        }
        if &header[257..263] != b"ustar\0" {
            return Err(ArchiveError::Damaged("a header isn't ustar"));
        }
        let name = read_text(&header[..100]).ok_or(ArchiveError::Damaged("a name isn't UTF-8"))?;
        let prefix =
            read_text(&header[345..500]).ok_or(ArchiveError::Damaged("a name isn't UTF-8"))?;
        let path = if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{prefix}/{name}")
        };
        if !matches!(header[156], b'0' | 0) {
            return Err(ArchiveError::NotAFile(path));
        }
        if !is_package_path(&path) {
            return Err(ArchiveError::BadPath(path));
        }
        if !seen.insert(path.clone()) {
            return Err(ArchiveError::Duplicate(path));
        }
        let size =
            read_octal(&header[124..136]).ok_or(ArchiveError::Damaged("a size can't be read"))?;
        let size = usize::try_from(size).map_err(|_| ArchiveError::TooLarge)?;
        total = total.checked_add(size).ok_or(ArchiveError::TooLarge)?;
        if total > limit {
            return Err(ArchiveError::TooLarge);
        }
        let start = at + BLOCK;
        let body = bytes
            .get(start..start + size)
            .ok_or(ArchiveError::Damaged("a file runs past the end"))?;
        files.push((path, body.to_vec()));
        at = start + size.div_ceil(BLOCK) * BLOCK;
    }
}
