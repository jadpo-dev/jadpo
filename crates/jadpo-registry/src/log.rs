//! The public log (area 11, "The registry protocol"): an append-only
//! Merkle tree of releases, hashed as RFC 6962 does with SHA-256, so the
//! registry can't give one app different code from another without it
//! showing.
//!
//! Each leaf is a release's [`LogEntry`]: its scoped name, version and
//! digest, so one log entry can't stand for another package's release.
//! Proofs are made by RFC 6962 §2.1's definitions and verified by RFC
//! 9162 §2.1.3.2 and §2.1.4.2's algorithms; a client checks them with
//! [`check_included`] and [`check_extends`].

use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::names::{Digest, ScopedName, Version, is_lower_hex};
pub use crate::protocol::{ConsistencyProof, InclusionProof, LogHead};

/// A SHA-256 hash in the log, written in JSON as 64 lower-case hex digits.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Hash([u8; 32]);

impl Hash {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn from_hex(text: &str) -> Option<Self> {
        if text.len() != 64 || !is_lower_hex(text) {
            return None;
        }
        let mut out = [0u8; 32];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16).ok()?;
        }
        Some(Self(out))
    }

    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hash({})", self.to_hex())
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl TryFrom<String> for Hash {
    type Error = String;
    fn try_from(text: String) -> Result<Self, String> {
        Self::from_hex(&text).ok_or_else(|| format!("`{text}` isn't 64 lower-case hex digits"))
    }
}

impl From<Hash> for String {
    fn from(hash: Hash) -> String {
        hash.to_hex()
    }
}

fn sha256(parts: &[&[u8]]) -> Hash {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&hasher.finalize());
    Hash(out)
}

/// A leaf's hash: SHA-256 of `0x00` and the leaf's bytes (RFC 6962 §2.1).
pub fn leaf_hash(data: &[u8]) -> Hash {
    sha256(&[&[0x00], data])
}

/// An interior node's hash: SHA-256 of `0x01` and its children's hashes.
pub fn node_hash(left: &Hash, right: &Hash) -> Hash {
    sha256(&[&[0x01], &left.0, &right.0])
}

/// One release in the log.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogEntry {
    pub package: ScopedName,
    pub version: Version,
    pub digest: Digest,
}

impl LogEntry {
    /// The leaf's bytes: `@acme/postcodes 2.1.0 sha256:…`. None of the
    /// three can hold a space, so the line reads back one way only.
    pub fn leaf(&self) -> Vec<u8> {
        format!("{} {} {}", self.package, self.version, self.digest).into_bytes()
    }
}

/// The largest power of two below `n`, for `n` of 2 or more: where RFC
/// 6962 splits a tree of `n` leaves.
fn split(n: u64) -> u64 {
    debug_assert!(n >= 2);
    1 << (63 - (n - 1).leading_zeros())
}

/// The log: its leaves' hashes, in order, and the entry each holds.
#[derive(Clone, Debug, Default)]
pub struct Log {
    leaves: Vec<Hash>,
    entries: Vec<Option<LogEntry>>,
}

impl Log {
    pub fn new() -> Self {
        Self::default()
    }

    /// How many leaves the log holds.
    pub fn size(&self) -> u64 {
        self.leaves.len() as u64
    }

    /// Adds a release, answering its index.
    pub fn append(&mut self, entry: &LogEntry) -> u64 {
        let index = self.append_leaf(&entry.leaf());
        self.entries[index as usize] = Some(entry.clone());
        index
    }

    /// Adds a leaf of any bytes, answering its index.
    pub fn append_leaf(&mut self, data: &[u8]) -> u64 {
        self.leaves.push(leaf_hash(data));
        self.entries.push(None);
        self.size() - 1
    }

    /// The entry at `index`.
    pub fn entry(&self, index: u64) -> Option<&LogEntry> {
        self.entries.get(usize::try_from(index).ok()?)?.as_ref()
    }

    /// The index of the release with `digest`.
    pub fn index_of(&self, digest: &Digest) -> Option<u64> {
        self.entries
            .iter()
            .position(|e| e.as_ref().is_some_and(|e| &e.digest == digest))
            .map(|i| i as u64)
    }

    /// The head: the log's size and root.
    pub fn head(&self) -> LogHead {
        let size = self.size();
        LogHead {
            size,
            root: self.subtree(0, size),
        }
    }

    /// The root of the log's first `size` leaves, if it has that many.
    pub fn root(&self, size: u64) -> Option<Hash> {
        (size <= self.size()).then(|| self.subtree(0, size))
    }

    /// The hash of the leaves from `lo` up to, not including, `hi`.
    fn subtree(&self, lo: u64, hi: u64) -> Hash {
        match hi - lo {
            0 => sha256(&[]),
            1 => self.leaves[lo as usize],
            n => {
                let k = split(n);
                node_hash(&self.subtree(lo, lo + k), &self.subtree(lo + k, hi))
            }
        }
    }

    /// The hashes that prove leaf `index` is in the tree of the first
    /// `size` leaves (RFC 6962 §2.1.1's `PATH`), if it is.
    pub fn inclusion_proof(&self, index: u64, size: u64) -> Option<Vec<Hash>> {
        if index >= size || size > self.size() {
            return None;
        }
        let mut out = Vec::new();
        self.path(index, 0, size, &mut out);
        Some(out)
    }

    fn path(&self, m: u64, lo: u64, hi: u64, out: &mut Vec<Hash>) {
        let n = hi - lo;
        if n <= 1 {
            return;
        }
        let k = split(n);
        if m < k {
            self.path(m, lo, lo + k, out);
            out.push(self.subtree(lo + k, hi));
        } else {
            self.path(m - k, lo + k, hi, out);
            out.push(self.subtree(lo, lo + k));
        }
    }

    /// The hashes that prove the tree of the first `to` leaves extends the
    /// tree of the first `from` (RFC 6962 §2.1.2's `PROOF`), if `from` is
    /// no more than `to` and the log holds `to`. Nothing proves a tree
    /// extends itself or the empty tree.
    pub fn consistency_proof(&self, from: u64, to: u64) -> Option<Vec<Hash>> {
        if from > to || to > self.size() {
            return None;
        }
        let mut out = Vec::new();
        if from > 0 && from < to {
            self.subproof(from, 0, to, true, &mut out);
        }
        Some(out)
    }

    fn subproof(&self, m: u64, lo: u64, hi: u64, whole: bool, out: &mut Vec<Hash>) {
        let n = hi - lo;
        if m == n {
            if !whole {
                out.push(self.subtree(lo, hi));
            }
            return;
        }
        let k = split(n);
        if m <= k {
            self.subproof(m, lo, lo + k, whole, out);
            out.push(self.subtree(lo + k, hi));
        } else {
            self.subproof(m - k, lo + k, hi, false, out);
            out.push(self.subtree(lo, lo + k));
        }
    }
}

/// Whether `proof` shows the leaf with hash `leaf` is at `index` in the
/// tree of `size` leaves whose root is `root` (RFC 9162 §2.1.3.2).
pub fn verify_inclusion(leaf: &Hash, index: u64, size: u64, proof: &[Hash], root: &Hash) -> bool {
    if index >= size {
        return false;
    }
    let mut f = index;
    let mut s = size - 1;
    let mut r = *leaf;
    for p in proof {
        if s == 0 {
            return false;
        }
        if f & 1 == 1 || f == s {
            r = node_hash(p, &r);
            while f & 1 == 0 && f != 0 {
                f >>= 1;
                s >>= 1;
            }
        } else {
            r = node_hash(&r, p);
        }
        f >>= 1;
        s >>= 1;
    }
    s == 0 && r == *root
}

/// Whether `proof` shows the tree of `to` leaves with root `to_root`
/// extends the tree of `from` leaves with root `from_root` (RFC 9162
/// §2.1.4.2). Equal sizes need equal roots and no hashes; every tree
/// extends the empty one, whose root is the hash of nothing.
pub fn verify_consistency(
    from: u64,
    to: u64,
    from_root: &Hash,
    to_root: &Hash,
    proof: &[Hash],
) -> bool {
    if from > to {
        return false;
    }
    if from == 0 {
        return proof.is_empty() && *from_root == sha256(&[]);
    }
    if from == to {
        return proof.is_empty() && from_root == to_root;
    }
    if proof.is_empty() {
        return false;
    }
    let mut path: Vec<Hash> = Vec::with_capacity(proof.len() + 1);
    if from.is_power_of_two() {
        path.push(*from_root);
    }
    path.extend_from_slice(proof);
    let mut f = from - 1;
    let mut s = to - 1;
    while f & 1 == 1 {
        f >>= 1;
        s >>= 1;
    }
    let mut fr = path[0];
    let mut sr = path[0];
    for c in &path[1..] {
        if s == 0 {
            return false;
        }
        if f & 1 == 1 || f == s {
            fr = node_hash(c, &fr);
            sr = node_hash(c, &sr);
            while f & 1 == 0 && f != 0 {
                f >>= 1;
                s >>= 1;
            }
        } else {
            sr = node_hash(&sr, c);
        }
        f >>= 1;
        s >>= 1;
    }
    fr == *from_root && sr == *to_root && s == 0
}

/// Why a client refused what the registry answered about its log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogError {
    /// The proof is of another release than the one asked about.
    NotTheRelease,
    /// The proof is at another head than the one the client holds.
    NotTheHead,
    /// The proof doesn't show the release is in the log.
    NotInLog,
    /// The new head is smaller than the last one seen.
    Shrank,
    /// The new head doesn't extend the last one seen: the log was
    /// rewritten, or this client is shown a different one.
    DoesNotExtend,
}

impl fmt::Display for LogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            LogError::NotTheRelease => "the registry proved another release than the one installed",
            LogError::NotTheHead => "the registry's proof is for another head of the log",
            LogError::NotInLog => {
                "the registry's proof doesn't show the release is in its public log"
            }
            LogError::Shrank => "the registry's log is smaller than when it was last seen",
            LogError::DoesNotExtend => {
                "the registry's log doesn't extend the one last seen: its history was changed"
            }
        })
    }
}

impl std::error::Error for LogError {}

/// Checks that `proof` shows `expected` is in the log at `head`, the head
/// the client has already checked extends the last one it saw.
pub fn check_included(
    head: &LogHead,
    expected: &LogEntry,
    proof: &InclusionProof,
) -> Result<(), LogError> {
    if proof.entry != *expected {
        return Err(LogError::NotTheRelease);
    }
    if proof.size != head.size {
        return Err(LogError::NotTheHead);
    }
    let leaf = leaf_hash(&expected.leaf());
    if verify_inclusion(&leaf, proof.index, proof.size, &proof.hashes, &head.root) {
        Ok(())
    } else {
        Err(LogError::NotInLog)
    }
}

/// Checks that `new` extends `seen`, the last head the client kept
/// (`.jadpo/registry_head`), by `proof`.
pub fn check_extends(
    seen: &LogHead,
    new: &LogHead,
    proof: &ConsistencyProof,
) -> Result<(), LogError> {
    if new.size < seen.size {
        return Err(LogError::Shrank);
    }
    if proof.from != seen.size || proof.to != new.size {
        return Err(LogError::NotTheHead);
    }
    if verify_consistency(seen.size, new.size, &seen.root, &new.root, &proof.hashes) {
        Ok(())
    } else {
        Err(LogError::DoesNotExtend)
    }
}
