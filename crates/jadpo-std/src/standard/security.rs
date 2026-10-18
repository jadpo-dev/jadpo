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
//!
//! Chunk 5.5r added the token types `crypto.verify_jwt` takes and gives:
//! `Jwt`, a signed token in JWS compact form (RFC 7515, RFC 7519); `Jwk`,
//! a public key it is checked against (RFC 7517, RFC 8037), never one
//! holding a secret; `JwtClaims`, the registered claims of a token whose
//! signature, times, issuer and audience held; and two values a token or
//! an identity provider carries, `OAuthScope` (RFC 6749) and `OidcClaims`
//! (OpenID Connect Core 1.0).

use super::{
    Examples, FIRST_RELEASE, Format, FormatKey, Holds, Json, NO_OPTIONS, Part, StandardType,
    Storage, TextForm, Validator,
};
use crate::Builtin;
use crate::read::SqlType;

/// `PasswordHash` (area 19).
pub const PASSWORD_HASH: &str = "PasswordHash";
/// `PublicKey` (area 19).
pub const PUBLIC_KEY: &str = "PublicKey";
/// `PasswordCheck` (area 19).
pub const PASSWORD_CHECK: &str = "PasswordCheck";
/// `Jwt` (area 19, chunk 5.5r).
pub const JWT: &str = "Jwt";
/// `JwtClaims` (area 19, chunk 5.5r).
pub const JWT_CLAIMS: &str = "JwtClaims";
/// `Jwk` (area 19, chunk 5.5r).
pub const JWK: &str = "Jwk";
/// `OAuthScope` (area 19, chunk 5.5r).
pub const OAUTH_SCOPE: &str = "OAuthScope";
/// `OidcClaims` (area 19, chunk 5.5r).
pub const OIDC_CLAIMS: &str = "OidcClaims";

/// The longest a `Jwt` is, in characters, so reading and checking one
/// never works through an unbounded token: 8 KiB, twice the 4 KiB a
/// browser keeps of a cookie, which holds any token an identity provider
/// issues (chosen in chunk 5.5r).
pub const JWT_MAX_LENGTH: usize = 8192;

/// How far, in seconds, `crypto.verify_jwt` lets the issuer's clock and
/// the server's disagree when it checks `exp`, `nbf` and `iat` (RFC 7519
/// §4.1.4 allows "some small leeway"): 30 seconds (chosen in chunk 5.5r).
pub const JWT_LEEWAY_SECONDS: i64 = 30;

/// The one algorithm a `Jwk` holds a key for, and so the one a token is
/// checked with: Ed25519 (RFC 8037's `EdDSA`), the signature `crypto`
/// already makes and checks (chosen in chunk 5.5r). A token's own `alg`
/// header never chooses it; one naming anything else, `none` included,
/// fails `invalid_signature`.
pub const JWT_ALGORITHM: &str = "EdDSA";

/// The bytes `text` holds as RFC 4648's URL alphabet with no padding, in
/// its one spelling (the bits past the last whole byte zero), as JOSE
/// writes every part (RFC 7515 §2); `None` for anything else, an empty
/// text included.
pub fn decode_base64url(text: &str) -> Option<Vec<u8>> {
    fn sextet(byte: u8) -> Option<u32> {
        Some(u32::from(match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        }))
    }
    if text.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    for chunk in text.as_bytes().chunks(4) {
        let mut bits: u32 = 0;
        for &byte in chunk {
            bits = (bits << 6) | sextet(byte)?;
        }
        let (count, spare) = match chunk.len() {
            4 => (3, 0),
            3 => (2, 2),
            2 => (1, 4),
            _ => return None,
        };
        if bits & ((1 << spare) - 1) != 0 {
            return None;
        }
        bits >>= spare;
        for shift in (0..count).rev() {
            out.push((bits >> (8 * shift)) as u8);
        }
    }
    Some(out)
}

/// Whether `text` is a `Jwt`: a JWS in compact serialisation (RFC 7515
/// §7.1), three parts joined by `.`, each unpadded Base64url in its one
/// spelling and none empty, so an unsecured token (RFC 7519 §6, `alg:
/// none`, whose signature is empty) is never one; at most
/// [`JWT_MAX_LENGTH`] characters. What the parts say is
/// `crypto.verify_jwt`'s to check.
pub fn is_jwt(text: &str) -> bool {
    if text.len() > JWT_MAX_LENGTH {
        return false;
    }
    let parts: Vec<&str> = text.split('.').collect();
    parts.len() == 3 && parts.iter().all(|part| decode_base64url(part).is_some())
}

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

/// A JWT (area 19, chunk 5.5r), as its compact form: what
/// `crypto.verify_jwt` checks. A token signed with another algorithm, as
/// RFC 7519 §3.1's example is, is still a `Jwt`, which `verify_jwt`
/// refuses.
pub const JWT_ENTRY: StandardType = StandardType {
    name: JWT,
    holds: Holds::Text {
        pattern: r"[A-Za-z0-9_\-]+\.[A-Za-z0-9_\-]+\.[A-Za-z0-9_\-]+",
        personal: None,
    },
    text: Some(TextForm::Checked),
    json: Json::Text,
    storage: Storage::Column(SqlType::Text),
    // A token is a credential: shown as its last four only, as a card
    // number is.
    format: Format {
        key: FormatKey::LastFour {
            mask: "•••• "
        },
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: Some(Validator::Jwt),
    shape: None,
    examples: Examples {
        valid: &[
            // RFC 7519 §3.1's example (HMAC SHA-256), and RFC 8037 §A.4's
            // Ed25519 signature.
            (
                "eyJ0eXAiOiJKV1QiLA0KICJhbGciOiJIUzI1NiJ9.eyJpc3MiOiJqb2UiLA0KICJleHAiOjEzMDA4MTkzODAsDQogImh0dHA6Ly9leGFtcGxlLmNvbS9pc19yb290Ijp0cnVlfQ.dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk",
                "•••• EjXk",
            ),
            (
                "eyJhbGciOiJFZERTQSJ9.RXhhbXBsZSBvZiBFZDI1NTE5IHNpZ25pbmc.hgyY0il_MGCjP0JzlnLWG1PPOt7-09PGcvMg3AIbQR6dWbhijcNR4ki4iylGjg5BhVsPt9g7sVvpAr_MuM0KAg",
                "•••• 0KAg",
            ),
        ],
        format: None,
        malformed: &[
            // RFC 7519 §6.1's unsecured token, whose signature is empty;
            // two parts; padding; the standard alphabet; a JWE's five
            // parts; bits set past a part's last byte; nothing at all.
            "eyJhbGciOiJub25lIn0.eyJpc3MiOiJqb2UiLA0KICJleHAiOjEzMDA4MTkzODAsDQogImh0dHA6Ly9leGFtcGxlLmNvbS9pc19yb290Ijp0cnVlfQ.",
            "eyJhbGciOiJFZERTQSJ9.RXhhbXBsZSBvZiBFZDI1NTE5IHNpZ25pbmc",
            "eyJhbGciOiJFZERTQSJ9.RXhhbXBsZSBvZiBFZDI1NTE5IHNpZ25pbmc=.hgyY0il_MGCjP0JzlnLWG1PPOt7-09PGcvMg3AIbQR6dWbhijcNR4ki4iylGjg5BhVsPt9g7sVvpAr_MuM0KAg",
            "eyJhbGciOiJFZERTQSJ9.RXhhbXBsZSBvZiBFZDI1NTE5IHNpZ25pbmc.hgyY0il/MGCjP0JzlnLWG1PPOt7+09PGcvMg3AIbQR6dWbhijcNR4ki4iylGjg5BhVsPt9g7sVvpAr/MuM0KAg",
            "eyJhbGciOiJkaXIiLCJlbmMiOiJBMTI4R0NNIn0..48V1_ALb6US04U3b.5eym8TW_c8SuK0ltJ3rpYIzOeDQz7TALvtu6UG9oMo4vpzs9tX_EFShS8iB7j6jiSdiwkIr3ajwQzaBtQD_A.XFBoMYUZodetZdvTiFvSkQ",
            "eyJhbGciOiJFZERTQSJ9.RXhhbXBsZSBvZiBFZDI1NTE5IHNpZ25pbmd.hgyY0il_MGCjP0JzlnLWG1PPOt7-09PGcvMg3AIbQR6dWbhijcNR4ki4iylGjg5BhVsPt9g7sVvpAr_MuM0KAg",
            "",
        ],
        of: None,
        settings: "",
    },
    release: FIRST_RELEASE,
};

/// What a `JwtClaims`' text claims (`iss`, `sub`, `jti` and each of
/// `aud`'s) hold: RFC 7519's StringOrURI, as one line of 1 to 1,024
/// characters with no control character (chosen in chunk 5.5r).
pub const CLAIM_PATTERN: &str = r"[^\p{Cc}]{1,1024}";

/// `JwtClaims`' parts, in order: RFC 7519 §4.1's registered claims, each
/// by its own name. `aud` is always a list, as one audience is read; `exp`
/// is always there, since `crypto.verify_jwt` refuses a token that never
/// expires.
pub const JWT_CLAIMS_FIELDS: [(&str, Part); 7] = [
    ("iss", Part::Optional(&Part::Matching(CLAIM_PATTERN))),
    ("sub", Part::Optional(&Part::Matching(CLAIM_PATTERN))),
    ("aud", Part::List(&Part::Matching(CLAIM_PATTERN))),
    ("exp", Part::Of(Builtin::Instant)),
    ("nbf", Part::Optional(&Part::Of(Builtin::Instant))),
    ("iat", Part::Optional(&Part::Of(Builtin::Instant))),
    ("jti", Part::Optional(&Part::Matching(CLAIM_PATTERN))),
];

/// A token's registered claims, once `crypto.verify_jwt` has checked it:
/// sent with RFC 7519's names, its times as Jadpo writes an instant, and
/// stored as that JSON.
pub const JWT_CLAIMS_ENTRY: StandardType = StandardType {
    name: JWT_CLAIMS,
    holds: Holds::Object(&JWT_CLAIMS_FIELDS),
    text: None,
    json: Json::Members(&["iss", "sub", "aud", "exp", "nbf", "iat", "jti"]),
    storage: Storage::Column(SqlType::Json),
    format: Format {
        key: FormatKey::FirstPart(&["sub", "iss", "jti", "exp"]),
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: Examples {
        valid: &[
            (
                r#"{"iss":"https://id.example.com","sub":"member-42","aud":["clinic"],"exp":"2026-10-18T10:00:00Z","nbf":"2026-10-18T09:00:00Z","iat":"2026-10-18T09:00:00Z","jti":"a1b2c3"}"#,
                "member-42",
            ),
            (
                r#"{"iss":"https://id.example.com","aud":[],"exp":"2026-10-18T10:00:00Z"}"#,
                "https://id.example.com",
            ),
            (
                r#"{"aud":[],"exp":"2026-10-18T10:00:00Z"}"#,
                "2026-10-18T10:00:00Z",
            ),
        ],
        format: None,
        malformed: &[
            // No expiry; RFC 7519's numeric date rather than an instant; an
            // audience that isn't a list; an empty subject; a claim the
            // type doesn't hold; not an object.
            r#"{"aud":[]}"#,
            r#"{"aud":[],"exp":1300819380}"#,
            r#"{"aud":"clinic","exp":"2026-10-18T10:00:00Z"}"#,
            r#"{"sub":"","aud":[],"exp":"2026-10-18T10:00:00Z"}"#,
            r#"{"aud":[],"exp":"2026-10-18T10:00:00Z","admin":true}"#,
            r#""member-42""#,
        ],
        of: None,
        settings: "",
    },
    release: FIRST_RELEASE,
};

/// What a `Jwk`'s `x` holds: an Ed25519 public key's 32 bytes as 43
/// characters of unpadded Base64url, the last carrying no bits past the
/// 256th (RFC 8037 §2).
pub const ED25519_X_PATTERN: &str = r"[A-Za-z0-9_\-]{42}[AEIMQUYcgkosw048]";

/// What a `Jwk`'s `kid` holds: 1 to 256 printable ASCII characters
/// (chosen in chunk 5.5r).
pub const KEY_ID_PATTERN: &str = r"[\x21-\x7E]{1,256}";

/// `Jwk`'s parts, in order: RFC 7517's members for an Ed25519 public key
/// (RFC 8037 §2), `use_` sent as `use`, which is a reserved word. No part
/// holds a private key (`d`) or a shared secret (an `oct` key's `k`):
/// JSON naming either isn't a `Jwk`, so a key set read from outside can't
/// put a secret in a program (chosen in chunk 5.5r).
pub const JWK_FIELDS: [(&str, Part); 6] = [
    ("kty", Part::Matching("OKP")),
    ("crv", Part::Matching("Ed25519")),
    ("x", Part::Matching(ED25519_X_PATTERN)),
    ("kid", Part::Optional(&Part::Matching(KEY_ID_PATTERN))),
    ("alg", Part::Optional(&Part::Matching(JWT_ALGORITHM))),
    ("use_", Part::Optional(&Part::Matching("sig"))),
];

/// A public key a token is checked against, as RFC 7517 sends one in a
/// key set: Ed25519 only, the one algorithm `verify_jwt` checks with.
pub const JWK_ENTRY: StandardType = StandardType {
    name: JWK,
    holds: Holds::Object(&JWK_FIELDS),
    text: None,
    json: Json::Members(&["kty", "crv", "x", "kid", "alg", "use"]),
    storage: Storage::Column(SqlType::Json),
    format: Format {
        key: FormatKey::FirstPart(&["kid", "x"]),
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: Examples {
        // RFC 8037 §A.2's public key, with a key id and the optional
        // members, and RFC 8032's second test key alone.
        valid: &[
            (
                r#"{"kty":"OKP","crv":"Ed25519","x":"11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo","kid":"2026-10","alg":"EdDSA","use":"sig"}"#,
                "2026-10",
            ),
            (
                r#"{"kty":"OKP","crv":"Ed25519","x":"PUAXw-hDiVqStwqnTRt-vJyYLM8uxJaMwM1V8Sr0Zgw"}"#,
                "PUAXw-hDiVqStwqnTRt-vJyYLM8uxJaMwM1V8Sr0Zgw",
            ),
        ],
        format: None,
        malformed: &[
            // RFC 8037 §A.1's private key (`d`), RFC 7517 §A.3's shared
            // secret, RFC 7517 §A.1's P-256 and RSA public keys, another
            // algorithm, an encryption key, a key one character short,
            // and not an object.
            r#"{"kty":"OKP","crv":"Ed25519","d":"nWGxne_9WmC6hEr0kuwsxERJxWl7MmkZcDusAxyuf2A","x":"11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo"}"#,
            r#"{"kty":"oct","alg":"A128KW","k":"GawgguFyGrWKav7AX4VKUg"}"#,
            r#"{"kty":"EC","crv":"P-256","x":"MKBCTNIcKUSDii11ySs3526iDZ8AiTo7Tu6KPAqv7D4","y":"4Etl6SRW2YiLUrN5vfvVHuhp7x8PxltmWWlbbM4IFyM","use":"enc","kid":"1"}"#,
            r#"{"kty":"RSA","n":"0vx7agoebGcQSuuPiLJXZptN9nndrQmbXEps2aiAFbWhM78LhWx4cbbfAAtVT86zwu1RK7aPFFxuhDR1L6tSoc_BJECPebWKRXjBZCiFV4n3oknjhMstn64tZ_2W-5JsGY4Hc5n9yBXArwl93lqt7_RN5w6Cf0h4QyQ5v-65YGjQR0_FDW2QvzqY368QQMicAtaSqzs8KJZgnYb9c7d0zgdAZHzu6qMQvRL5hajrn1n91CbOpbISD08qNLyrdkt-bFTWhAI4vMQFh6WeZu0fM4lFd2NcRwr3XPksINHaQ-G_xBniIqbw0Ls1jF44-csFCur-kEgU8awapJzKnqDKgw","e":"AQAB","alg":"RS256","kid":"2011-04-29"}"#,
            r#"{"kty":"OKP","crv":"Ed25519","x":"11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo","alg":"HS256"}"#,
            r#"{"kty":"OKP","crv":"Ed25519","x":"11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo","use":"enc"}"#,
            r#"{"kty":"OKP","crv":"Ed25519","x":"11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHUR"}"#,
            r#""11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo""#,
        ],
        of: None,
        settings: "",
    },
    release: FIRST_RELEASE,
};

/// An OAuth scope (area 19, RFC 6749 §3.3): one scope-token, printable
/// ASCII but the space, `"` and `\`. A token's scopes are several, each
/// one of these.
pub const OAUTH_SCOPE_ENTRY: StandardType = StandardType {
    name: OAUTH_SCOPE,
    holds: Holds::Text {
        pattern: r#"[\x21\x23-\x5B\x5D-\x7E]{1,256}"#,
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
    validator: None,
    shape: None,
    examples: Examples {
        valid: &[
            ("openid", "openid"),
            ("profile:read", "profile:read"),
            (
                "https://www.googleapis.com/auth/userinfo.email",
                "https://www.googleapis.com/auth/userinfo.email",
            ),
        ],
        format: None,
        malformed: &["openid email", "say\"hi\"", "back\\slash", "tab\tbed", ""],
        of: None,
        settings: "",
    },
    release: FIRST_RELEASE,
};

/// What an `OidcClaims`' `sub` holds: OpenID Connect Core §2's subject
/// identifier, 1 to 255 ASCII characters, here printable ones.
pub const OIDC_SUBJECT_PATTERN: &str = r"[\x21-\x7E]{1,255}";

/// `OidcClaims`' parts, in order: OpenID Connect Core 1.0 §5.1's standard
/// claims a sign-in reads, each by its own name; a person's name in parts
/// as the standard types hold them. `gender` stays out, as area 19's rule
/// keeps it out of every standard type, with `birthdate` and `address`,
/// which a sign-in doesn't need (chosen in chunk 5.5r).
pub const OIDC_CLAIMS_FIELDS: [(&str, Part); 10] = [
    ("sub", Part::Matching(OIDC_SUBJECT_PATTERN)),
    ("name", Part::Optional(&Part::Line)),
    ("given_name", Part::Optional(&Part::Standard("GivenName"))),
    ("family_name", Part::Optional(&Part::Standard("FamilyName"))),
    ("preferred_username", Part::Optional(&Part::Line)),
    ("email", Part::Optional(&Part::Standard("Email"))),
    ("email_verified", Part::Optional(&Part::Of(Builtin::Bool))),
    (
        "phone_number",
        Part::Optional(&Part::Standard("PhoneNumber")),
    ),
    (
        "phone_number_verified",
        Part::Optional(&Part::Of(Builtin::Bool)),
    ),
    ("picture", Part::Optional(&Part::Standard("Url"))),
];

/// Who an identity provider says signed in (OpenID Connect Core 1.0):
/// sent with the standard's names, the members it doesn't have left out,
/// stored as that JSON. Personal data, by its parts.
pub const OIDC_CLAIMS_ENTRY: StandardType = StandardType {
    name: OIDC_CLAIMS,
    holds: Holds::Object(&OIDC_CLAIMS_FIELDS),
    text: None,
    json: Json::Members(&[
        "sub",
        "name",
        "given_name",
        "family_name",
        "preferred_username",
        "email",
        "email_verified",
        "phone_number",
        "phone_number_verified",
        "picture",
    ]),
    storage: Storage::Column(SqlType::Json),
    format: Format {
        key: FormatKey::FirstPart(&["name", "email", "sub"]),
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: Examples {
        valid: &[
            (
                r#"{"sub":"248289761001","name":"Jane Doe","given_name":"Jane","family_name":"Doe","preferred_username":"j.doe","email":"janedoe@example.com","email_verified":true,"picture":"https://example.com/janedoe/me.jpg"}"#,
                "Jane Doe",
            ),
            (
                r#"{"sub":"248289761002","email":"j.doe@example.org","email_verified":false}"#,
                "j.doe@example.org",
            ),
            (r#"{"sub":"248289761003"}"#, "248289761003"),
        ],
        format: None,
        malformed: &[
            // No subject, a subject with a space, an address that isn't
            // one, a flag that isn't one, a claim the type leaves out, and
            // not an object.
            r#"{"name":"Jane Doe"}"#,
            r#"{"sub":"jane doe"}"#,
            r#"{"sub":"248289761001","email":"janedoe"}"#,
            r#"{"sub":"248289761001","email_verified":"yes"}"#,
            r#"{"sub":"248289761001","gender":"female"}"#,
            r#""248289761001""#,
        ],
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

    #[test]
    fn base64url_has_one_spelling() {
        assert_eq!(
            decode_base64url("eyJhbGciOiJFZERTQSJ9").as_deref(),
            Some(&b"{\"alg\":\"EdDSA\"}"[..])
        );
        assert_eq!(decode_base64url("AQAB").as_deref(), Some(&[1, 0, 1][..]));
        // `R` carries bits past the last byte; padding; another alphabet;
        // a length no bytes give.
        assert_eq!(decode_base64url("AQ"), Some(vec![1]));
        assert_eq!(decode_base64url("AR"), None);
        assert_eq!(decode_base64url("AQ=="), None);
        assert_eq!(decode_base64url("A+/B"), None);
        assert_eq!(decode_base64url("A"), None);
        assert_eq!(decode_base64url(""), None);
    }

    #[test]
    fn a_jwt_is_three_parts_and_never_unsecured() {
        for (valid, _) in JWT_ENTRY.examples.valid {
            assert!(is_jwt(valid), "{valid}");
        }
        for malformed in JWT_ENTRY.examples.malformed {
            assert!(!is_jwt(malformed), "{malformed}");
        }
        let long = format!("{}.e30.AAAA", "A".repeat(JWT_MAX_LENGTH));
        assert!(!is_jwt(&long));
    }
}
