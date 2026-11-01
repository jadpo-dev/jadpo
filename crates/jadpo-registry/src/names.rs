//! A registry's names, each with exactly one spelling: the scoped name
//! `@publisher/name`, a publisher's handle `@publisher`, a release's
//! version and its digest as the lock writes it.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Why a name or digest was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameError {
    pub text: String,
    pub expected: &'static str,
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` isn't {}", self.text, self.expected)
    }
}

impl std::error::Error for NameError {}

/// The longest a publisher's or a package's part of a name may be.
pub const LONGEST_PART: usize = 39;

/// One part of a scoped name: lower-case ASCII letters and digits,
/// starting with a letter, with single `-`s between them, at most
/// [`LONGEST_PART`] characters, so a name is one spelling and reads the
/// same in a URL, a path and a lock line.
pub fn is_part(text: &str) -> bool {
    let bytes = text.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= LONGEST_PART
        && bytes[0].is_ascii_lowercase()
        && bytes[bytes.len() - 1] != b'-'
        && !text.contains("--")
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

/// A package's registry name, `@acme/postcodes` (area 11, "Names and
/// publishers").
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ScopedName {
    publisher: String,
    name: String,
}

impl ScopedName {
    pub fn parse(text: &str) -> Result<Self, NameError> {
        let refused = || NameError {
            text: text.to_string(),
            expected: "a scoped name such as `@acme/postcodes`",
        };
        let rest = text.strip_prefix('@').ok_or_else(refused)?;
        let (publisher, name) = rest.split_once('/').ok_or_else(refused)?;
        if !is_part(publisher) || !is_part(name) {
            return Err(refused());
        }
        Ok(Self {
            publisher: publisher.to_string(),
            name: name.to_string(),
        })
    }

    /// The publisher, without its `@`: `acme`.
    pub fn publisher(&self) -> &str {
        &self.publisher
    }

    /// The package's own part: `postcodes`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The publisher as the URL's segment writes it, with its `@`.
    pub fn publisher_segment(&self) -> String {
        format!("@{}", self.publisher)
    }

    /// The publisher's handle, `@acme`.
    pub fn handle(&self) -> Handle {
        Handle(self.publisher_segment())
    }
}

impl fmt::Display for ScopedName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@{}/{}", self.publisher, self.name)
    }
}

impl TryFrom<String> for ScopedName {
    type Error = NameError;
    fn try_from(text: String) -> Result<Self, NameError> {
        Self::parse(&text)
    }
}

impl From<ScopedName> for String {
    fn from(name: ScopedName) -> String {
        name.to_string()
    }
}

/// A publisher's handle, `@acme`: the scope every one of its packages'
/// names starts with.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Handle(String);

impl Handle {
    pub fn parse(text: &str) -> Result<Self, NameError> {
        match text.strip_prefix('@') {
            Some(part) if is_part(part) => Ok(Self(text.to_string())),
            _ => Err(NameError {
                text: text.to_string(),
                expected: "a publisher such as `@acme`",
            }),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for Handle {
    type Error = NameError;
    fn try_from(text: String) -> Result<Self, NameError> {
        Self::parse(&text)
    }
}

impl From<Handle> for String {
    fn from(handle: Handle) -> String {
        handle.0
    }
}

/// A release's version, `2.1.0` or `2.0.0-beta.1`, as the lock reads it:
/// three numbers without leading zeros and an optional pre-release of
/// letters, digits, `.` and `-`; no build metadata.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Version(String);

impl Version {
    pub fn parse(text: &str) -> Result<Self, NameError> {
        let (core, pre) = match text.split_once('-') {
            Some((core, pre)) => (core, Some(pre)),
            None => (text, None),
        };
        let numbers: Vec<&str> = core.split('.').collect();
        let good = numbers.len() == 3
            && numbers.iter().all(|n| {
                !n.is_empty()
                    && n.len() <= 9
                    && n.bytes().all(|b| b.is_ascii_digit())
                    && (*n == "0" || !n.starts_with('0'))
            })
            && pre.is_none_or(|pre| {
                !pre.is_empty()
                    && pre
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
            });
        if good {
            Ok(Self(text.to_string()))
        } else {
            Err(NameError {
                text: text.to_string(),
                expected: "a version such as `2.1.0`",
            })
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Its three numbers, then whether it is a full release (which sorts
    /// after its pre-releases), then the pre-release's text.
    fn order(&self) -> (u64, u64, u64, bool, &str) {
        let (core, pre) = match self.0.split_once('-') {
            Some((core, pre)) => (core, pre),
            None => (self.0.as_str(), ""),
        };
        let mut numbers = core.split('.').map(|n| n.parse::<u64>().unwrap_or(0));
        let mut next = || numbers.next().unwrap_or(0);
        (next(), next(), next(), pre.is_empty(), pre)
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.order().cmp(&other.order())
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for Version {
    type Error = NameError;
    fn try_from(text: String) -> Result<Self, NameError> {
        Self::parse(&text)
    }
}

impl From<Version> for String {
    fn from(version: Version) -> String {
        version.0
    }
}

/// A release's digest, `sha256:` and 64 lower-case hex digits, as
/// `jadpo.lock` records it (area 18, "The lock").
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Digest(String);

impl Digest {
    pub fn parse(text: &str) -> Result<Self, NameError> {
        if text
            .strip_prefix("sha256:")
            .is_some_and(|hex| hex.len() == 64 && is_lower_hex(hex))
        {
            Ok(Self(text.to_string()))
        } else {
            Err(NameError {
                text: text.to_string(),
                expected: "a digest such as `sha256:e3b0…55`",
            })
        }
    }

    /// The digest of a package's files, as the lock computes it.
    pub fn of_files(files: &[(&str, &[u8])]) -> Self {
        Self(jadpo_db::package_digest(files))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for Digest {
    type Error = NameError;
    fn try_from(text: String) -> Result<Self, NameError> {
        Self::parse(&text)
    }
}

impl From<Digest> for String {
    fn from(digest: Digest) -> String {
        digest.0
    }
}

pub(crate) fn is_lower_hex(text: &str) -> bool {
    text.bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
