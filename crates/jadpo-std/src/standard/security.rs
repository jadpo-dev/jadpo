//! The security types `crypto` gives and takes (area 19, "Hashing and
//! cryptography", chunk 5.5q2): `PasswordHash`, the PHC string Argon2id
//! writes; `PublicKey`, an Ed25519 public key, which is not secret; and
//! `PasswordCheck`, what checking a password against its hash says. The
//! keys themselves (`EncryptionKey`, `PrivateKey`, `SigningKey`) are
//! built-in secret types ([`crate::Builtin::is_key`]), held only by
//! settings and never by a program, so they have no entry here.
//!
//! Each is checked by its format alone; the constructions are
//! `jadpo-value`'s `crypto` module's.

use super::{
    Examples, FIRST_RELEASE, Format, FormatKey, Holds, Json, NO_OPTIONS, StandardType, Storage,
    TextForm, Validator,
};
use crate::read::SqlType;

/// `PasswordHash` (area 19).
pub const PASSWORD_HASH: &str = "PasswordHash";
/// `PublicKey` (area 19).
pub const PUBLIC_KEY: &str = "PublicKey";
/// `PasswordCheck` (area 19).
pub const PASSWORD_CHECK: &str = "PasswordCheck";

/// The Argon2id parameters a release hashes passwords with (area 19,
/// "Hashing and cryptography"): OWASP's Password Storage Cheat Sheet's
/// first Argon2id configuration, 19 MiB of memory, 2 passes and 1 lane,
/// with a 16-byte salt and a 32-byte hash, version 0x13 (19). A hash made
/// with any other is still checked, and found `correct_outdated`.
pub const ARGON2_MEMORY_KIB: u32 = 19 * 1024;
pub const ARGON2_PASSES: u32 = 2;
pub const ARGON2_LANES: u32 = 1;
pub const ARGON2_SALT_BYTES: usize = 16;
pub const ARGON2_HASH_BYTES: usize = 32;
pub const ARGON2_VERSION: u32 = 19;

/// The most a `PasswordHash` may ask checking it to spend, so a hash read
/// from outside can't make a sign-in allocate without bound: RFC 9106's
/// costliest recommended Argon2id (2 GiB), and at most 16 passes and 16
/// lanes. The fewest are the algorithm's own: 8 KiB per lane, 1 pass,
/// 1 lane.
pub const ARGON2_MEMORY_MAX_KIB: u64 = 2 * 1024 * 1024;
pub const ARGON2_PASSES_MAX: u64 = 16;
pub const ARGON2_LANES_MAX: u64 = 16;

/// A PHC string's salt and hash lengths, in bytes, that a `PasswordHash`
/// holds: a salt of at least the 8 bytes Argon2 needs, and a hash of at
/// least 16; at most 64 of either.
const SALT_BYTES: (usize, usize) = (8, 64);
const HASH_BYTES: (usize, usize) = (16, 64);

/// An Argon2id PHC string's parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Phc<'a> {
    pub memory_kib: u64,
    pub passes: u64,
    pub lanes: u64,
    /// The salt and the hash as the PHC string writes them: standard
    /// Base64 without padding.
    pub salt: &'a str,
    pub hash: &'a str,
}

impl Phc<'_> {
    /// Whether it was made with the release's parameters, so checking a
    /// password against it gives `correct` rather than `correct_outdated`.
    pub fn is_current(&self) -> bool {
        self.memory_kib == u64::from(ARGON2_MEMORY_KIB)
            && self.passes == u64::from(ARGON2_PASSES)
            && self.lanes == u64::from(ARGON2_LANES)
            && b64_bytes(self.hash) == Some(ARGON2_HASH_BYTES)
    }
}

/// A PHC string's whole number: digits with no leading zero.
fn decimal(text: &str) -> Option<u64> {
    let plain = !text.is_empty()
        && text.bytes().all(|b| b.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'));
    plain.then(|| text.parse().ok()).flatten()
}

/// How many bytes standard Base64 without padding holds, in its one
/// spelling (the bits past the last whole byte zero); `None` when it isn't
/// that.
fn b64_bytes(text: &str) -> Option<usize> {
    let value = |b: u8| -> Option<u8> {
        Some(match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        })
    };
    let last = *text.as_bytes().last()?;
    for b in text.bytes() {
        value(b)?;
    }
    let spare = match text.len() % 4 {
        0 => 0,
        2 => 4,
        3 => 2,
        _ => return None,
    };
    if value(last)? & ((1 << spare) - 1) != 0 {
        return None;
    }
    Some(text.len() * 6 / 8)
}

/// An Argon2id PHC string (the PHC string format, as the reference
/// implementation and RustCrypto's `argon2` write it):
/// `$argon2id$v=19$m=<KiB>,t=<passes>,p=<lanes>$<salt>$<hash>`, the salt
/// and the hash in standard Base64 without padding, each in its one
/// spelling, the parameters in that order with no others, and every cost
/// within the bounds above. Only Argon2id at version 19 is read: it is the
/// one construction (area 19, "No algorithm choices").
pub fn read_phc(text: &str) -> Option<Phc<'_>> {
    let rest = text.strip_prefix("$argon2id$v=19$m=")?;
    let mut parts = rest.split('$');
    let (params, salt, hash) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let (memory, rest) = params.split_once(",t=")?;
    let (passes, lanes) = rest.split_once(",p=")?;
    let (memory_kib, passes, lanes) = (decimal(memory)?, decimal(passes)?, decimal(lanes)?);
    let costs = (1..=ARGON2_LANES_MAX).contains(&lanes)
        && (1..=ARGON2_PASSES_MAX).contains(&passes)
        && (8 * lanes..=ARGON2_MEMORY_MAX_KIB).contains(&memory_kib);
    let salt_ok = b64_bytes(salt).is_some_and(|n| (SALT_BYTES.0..=SALT_BYTES.1).contains(&n));
    let hash_ok = b64_bytes(hash).is_some_and(|n| (HASH_BYTES.0..=HASH_BYTES.1).contains(&n));
    (costs && salt_ok && hash_ok).then_some(Phc {
        memory_kib,
        passes,
        lanes,
        salt,
        hash,
    })
}

/// Whether `text` is a `PasswordHash`.
pub fn is_password_hash(text: &str) -> bool {
    read_phc(text).is_some()
}

/// Whether `text` is a `PublicKey`: an Ed25519 public key's 32 bytes as 43
/// characters of unpadded Base64url, as a `PrivateKey` setting's seed is
/// written. Whether the bytes are a point on the curve is checked where a
/// signature is, which refuses every key that isn't.
pub fn is_public_key(text: &str) -> bool {
    crate::is_key_text(text)
}

/// The PHC string, read whole (area 19, "Hashing and cryptography").
pub const PASSWORD_HASH_ENTRY: StandardType = StandardType {
    name: PASSWORD_HASH,
    holds: Holds::Text {
        pattern: r"\$argon2id\$v=19\$m=[0-9]+,t=[0-9]+,p=[0-9]+\$[A-Za-z0-9+/]+\$[A-Za-z0-9+/]+",
        personal: None,
    },
    text: Some(TextForm::Checked),
    json: Json::Text,
    storage: Storage::Column(SqlType::Text),
    format: Format {
        key: FormatKey::Canonical,
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: Some(Validator::PasswordHash),
    shape: None,
    examples: Examples {
        // "correct horse battery staple" with the salt 00 01 ... 0f, at
        // the release's parameters and at older ones, as OpenSSL 3.6's
        // Argon2id gives them.
        valid: &[
            (
                "$argon2id$v=19$m=19456,t=2,p=1$AAECAwQFBgcICQoLDA0ODw$gYJZtjEAJqjg26xdLmknq8/bB7MiWPrE9hsYuA+SkIU",
                "$argon2id$v=19$m=19456,t=2,p=1$AAECAwQFBgcICQoLDA0ODw$gYJZtjEAJqjg26xdLmknq8/bB7MiWPrE9hsYuA+SkIU",
            ),
            (
                "$argon2id$v=19$m=4096,t=3,p=1$AAECAwQFBgcICQoLDA0ODw$VTsSHiMxkq48zEo8LUSymjr7JjaRSgsxGX2Z8HQE9Cw",
                "$argon2id$v=19$m=4096,t=3,p=1$AAECAwQFBgcICQoLDA0ODw$VTsSHiMxkq48zEo8LUSymjr7JjaRSgsxGX2Z8HQE9Cw",
            ),
        ],
        format: None,
        malformed: &[
            // Another algorithm, another version, no version, the
            // parameters out of order, a leading zero, too much memory, a
            // padded salt, a salt too short, no hash, and plain text.
            "$argon2i$v=19$m=19456,t=2,p=1$AAECAwQFBgcICQoLDA0ODw$gYJZtjEAJqjg26xdLmknq8/bB7MiWPrE9hsYuA+SkIU",
            "$argon2id$v=16$m=19456,t=2,p=1$AAECAwQFBgcICQoLDA0ODw$gYJZtjEAJqjg26xdLmknq8/bB7MiWPrE9hsYuA+SkIU",
            "$argon2id$m=19456,t=2,p=1$AAECAwQFBgcICQoLDA0ODw$gYJZtjEAJqjg26xdLmknq8/bB7MiWPrE9hsYuA+SkIU",
            "$argon2id$v=19$t=2,m=19456,p=1$AAECAwQFBgcICQoLDA0ODw$gYJZtjEAJqjg26xdLmknq8/bB7MiWPrE9hsYuA+SkIU",
            "$argon2id$v=19$m=019456,t=2,p=1$AAECAwQFBgcICQoLDA0ODw$gYJZtjEAJqjg26xdLmknq8/bB7MiWPrE9hsYuA+SkIU",
            "$argon2id$v=19$m=4194304,t=2,p=1$AAECAwQFBgcICQoLDA0ODw$gYJZtjEAJqjg26xdLmknq8/bB7MiWPrE9hsYuA+SkIU",
            "$argon2id$v=19$m=19456,t=2,p=1$AAECAwQFBgcICQoLDA0ODw==$gYJZtjEAJqjg26xdLmknq8/bB7MiWPrE9hsYuA+SkIU",
            "$argon2id$v=19$m=19456,t=2,p=1$AAEC$gYJZtjEAJqjg26xdLmknq8/bB7MiWPrE9hsYuA+SkIU",
            "$argon2id$v=19$m=19456,t=2,p=1$AAECAwQFBgcICQoLDA0ODw",
            "correct horse battery staple",
        ],
        of: None,
        settings: "",
    },
    release: FIRST_RELEASE,
};

/// An Ed25519 public key, as unpadded Base64url (area 19).
pub const PUBLIC_KEY_ENTRY: StandardType = StandardType {
    name: PUBLIC_KEY,
    holds: Holds::Text {
        pattern: r"[A-Za-z0-9_\-]{43}",
        personal: None,
    },
    text: Some(TextForm::Checked),
    json: Json::Text,
    storage: Storage::Column(SqlType::Text),
    format: Format {
        key: FormatKey::Canonical,
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: Some(Validator::PublicKey),
    shape: None,
    examples: Examples {
        // RFC 8032's first and second test keys (section 7.1).
        valid: &[
            (
                "11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo",
                "11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo",
            ),
            (
                "PUAXw-hDiVqStwqnTRt-vJyYLM8uxJaMwM1V8Sr0Zgw",
                "PUAXw-hDiVqStwqnTRt-vJyYLM8uxJaMwM1V8Sr0Zgw",
            ),
        ],
        format: None,
        malformed: &[
            // Padded, the standard alphabet, hexadecimal, one character
            // short, and bits set past the 256th.
            "11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo=",
            "PUAXw+hDiVqStwqnTRt+vJyYLM8uxJaMwM1V8Sr0Zgw",
            "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a",
            "11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHUR",
            "11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURp",
            "",
        ],
        of: None,
        settings: "",
    },
    release: FIRST_RELEASE,
};

/// What checking a password says (area 19): `wrong`; `correct`; or
/// `correct_outdated`, correct against a hash made with older parameters,
/// so the application stores a fresh one.
pub const PASSWORD_CHECKS: &[(&str, &str)] = &[
    ("wrong", "wrong"),
    ("correct", "correct"),
    ("correct_outdated", "correct_outdated"),
];

/// `PasswordCheck`, a built-in enum (area 19, "Consistency with the rest
/// of the language").
pub const PASSWORD_CHECK_ENTRY: StandardType = StandardType {
    name: PASSWORD_CHECK,
    holds: Holds::Enum(PASSWORD_CHECKS),
    text: Some(TextForm::Code),
    json: Json::Text,
    storage: Storage::Column(SqlType::Text),
    format: Format {
        key: FormatKey::Canonical,
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: Examples {
        valid: &[
            ("wrong", "wrong"),
            ("correct", "correct"),
            ("correct_outdated", "correct_outdated"),
        ],
        format: None,
        malformed: &["right", "correctly", "correct ", ""],
        of: None,
        settings: "",
    },
    release: FIRST_RELEASE,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_phc_string_is_read_with_its_costs_and_bounded() {
        let current = read_phc(PASSWORD_HASH_ENTRY.examples.valid[0].0).expect("a hash");
        assert_eq!(
            (current.memory_kib, current.passes, current.lanes),
            (19456, 2, 1)
        );
        assert_eq!(current.salt, "AAECAwQFBgcICQoLDA0ODw");
        assert!(current.is_current());
        let older = read_phc(PASSWORD_HASH_ENTRY.examples.valid[1].0).expect("a hash");
        assert!(!older.is_current());
        for malformed in PASSWORD_HASH_ENTRY.examples.malformed {
            assert!(!is_password_hash(malformed), "{malformed}");
        }
        // Memory below 8 KiB a lane, and too many lanes and passes.
        let hash = "AAECAwQFBgcICQoLDA0ODw$gYJZtjEAJqjg26xdLmknq8/bB7MiWPrE9hsYuA+SkIU";
        assert!(!is_password_hash(&format!(
            "$argon2id$v=19$m=31,t=2,p=4${hash}"
        )));
        assert!(is_password_hash(&format!(
            "$argon2id$v=19$m=32,t=3,p=4${hash}"
        )));
        assert!(!is_password_hash(&format!(
            "$argon2id$v=19$m=19456,t=17,p=1${hash}"
        )));
        assert!(!is_password_hash(&format!(
            "$argon2id$v=19$m=19456,t=2,p=17${hash}"
        )));
    }

    #[test]
    fn standard_base64_has_one_spelling() {
        assert_eq!(b64_bytes("AAECAwQFBgcICQoLDA0ODw"), Some(16));
        // `x` carries bits past the sixteenth byte.
        assert_eq!(b64_bytes("AAECAwQFBgcICQoLDA0ODx"), None);
        assert_eq!(b64_bytes("AAECAwQFBgcICQoLDA0OD"), None);
        assert_eq!(b64_bytes("AAEC-wQF"), None);
        assert_eq!(b64_bytes(""), None);
    }

    #[test]
    fn a_public_key_is_32_bytes_of_unpadded_base64url() {
        for (valid, _) in PUBLIC_KEY_ENTRY.examples.valid {
            assert!(is_public_key(valid));
        }
        for malformed in PUBLIC_KEY_ENTRY.examples.malformed {
            assert!(!is_public_key(malformed), "{malformed}");
        }
    }
}
