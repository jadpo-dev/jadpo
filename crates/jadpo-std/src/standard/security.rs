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
//!
//! Chunk 5.5r2 added the secret values, and made `Jwt` one: `Password`,
//! what a person types; `ApiKey`, a key for an API; `BearerToken`, an RFC
//! 6750 access token; `TotpSecret`, an authenticator's shared secret (RFC
//! 6238); and `SecretText`, any other secret. Each is read from text and
//! JSON, never written, shown or stored as itself ([`SECRET_VALUES`]):
//! its text form is [`TextForm::Secret`], its JSON [`Json::Secret`], its
//! format [`FormatKey::Redacted`] and its storage [`Storage::Never`], and
//! the runtime holds it as `jadpo-value`'s `Secret`, compared in constant
//! time and written only redacted.
//!
//! Chunk 5.5r3 added the security records: `Hash` and `Hmac`, a SHA-256
//! digest and an HMAC-SHA-256 tag as `hash.sha256` and `crypto.hmac` give
//! them, and `EncryptedValue`, the sealed bytes `crypto.encrypt` gives
//! (each a type built on `Bytes`, [`Holds::Bytes`], compared in constant
//! time); `OneTimeCode`, the code a person types, a secret value;
//! `PasswordPolicy`, NIST SP 800-63B's rules for a password; and
//! `WebAuthnCredential`, a passkey as `webauthn-rs` keeps one.

use super::across::{Order, StandardRule, in_order};
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
/// `Password` (area 19, chunk 5.5r2).
pub const PASSWORD: &str = "Password";
/// `ApiKey` (area 19, chunk 5.5r2).
pub const API_KEY: &str = "ApiKey";
/// `BearerToken` (area 19, chunk 5.5r2).
pub const BEARER_TOKEN: &str = "BearerToken";
/// `TotpSecret` (area 19, chunk 5.5r2).
pub const TOTP_SECRET: &str = "TotpSecret";
/// `SecretText` (area 19, chunk 5.5r2).
pub const SECRET_TEXT: &str = "SecretText";
/// `Hash` (area 19, chunk 5.5r3).
pub const HASH: &str = "Hash";
/// `Hmac` (area 19, chunk 5.5r3).
pub const HMAC: &str = "Hmac";
/// `EncryptedValue` (area 19, chunk 5.5r3).
pub const ENCRYPTED_VALUE: &str = "EncryptedValue";
/// `OneTimeCode` (area 19, chunk 5.5r3).
pub const ONE_TIME_CODE: &str = "OneTimeCode";
/// `PasswordPolicy` (area 19, chunk 5.5r3).
pub const PASSWORD_POLICY: &str = "PasswordPolicy";
/// `WebAuthnCredential` (area 19, chunk 5.5r3).
pub const WEB_AUTHN_CREDENTIAL: &str = "WebAuthnCredential";

/// The secret values (area 19, "Secret values", chunk 5.5r2): never in a
/// response, an `Output`, a failure, an event, an email, a message,
/// `json.write` or a log, compared in constant time, and redacted
/// wherever a value is shown to a person.
/// `OneTimeCode` joined them in chunk 5.5r3.
pub const SECRET_VALUES: [&str; 7] = [
    PASSWORD,
    API_KEY,
    BEARER_TOKEN,
    TOTP_SECRET,
    SECRET_TEXT,
    JWT,
    ONE_TIME_CODE,
];

/// The secret value standard type named `name`, as the static name the
/// runtime keeps with a secret's text.
pub fn secret_value(name: &str) -> Option<&'static str> {
    SECRET_VALUES.into_iter().find(|secret| *secret == name)
}

/// Whether a secret value of type `name` may one day be stored, sealed
/// with an encrypted field's key ("Language gaps": keys held as data, with
/// encrypted fields), so that storing one now is not built yet rather than
/// refused: a `TotpSecret`, which checking a code needs back, and a
/// `SecretText`. A password, a key, a token and a JWT are never stored as
/// themselves, only as a hash (chosen in chunk 5.5r2).
pub fn stored_encrypted_later(name: &str) -> bool {
    matches!(name, TOTP_SECRET | SECRET_TEXT)
}

/// The fewest and most characters a `Password` has: NIST SP 800-63B's
/// minimum of 8, which an application raises with its own `min_length:`
/// (15 where a password is the only factor), and a bound past its "at
/// least 64" so hashing one never works through unbounded text (chosen in
/// chunk 5.5r2).
pub const PASSWORD_MIN_LENGTH: usize = 8;
pub const PASSWORD_MAX_LENGTH: usize = 256;

/// The most characters an `ApiKey` has (chosen in chunk 5.5r2).
pub const API_KEY_MAX_LENGTH: usize = 512;

/// The most characters a `BearerToken` has: a `Jwt`'s, since a bearer
/// token is often one (chosen in chunk 5.5r2).
pub const BEARER_TOKEN_MAX_LENGTH: usize = JWT_MAX_LENGTH;

/// The fewest and most bytes a `TotpSecret` holds: RFC 4226's 128 bits
/// (R6, "MUST be at least 128 bits"), and 512 (chosen in chunk 5.5r2).
pub const TOTP_SECRET_MIN_BYTES: usize = 16;
pub const TOTP_SECRET_MAX_BYTES: usize = 64;

/// The most characters a `SecretText` has: room for a PEM private key or
/// a service account's JSON (chosen in chunk 5.5r2).
pub const SECRET_TEXT_MAX_LENGTH: usize = 8192;

/// Whether `text` is a `Password`: 8 to 256 characters, none of them a
/// control character; spaces and any other Unicode are allowed, as NIST SP
/// 800-63B asks.
pub fn is_password(text: &str) -> bool {
    let length = text.chars().count();
    (PASSWORD_MIN_LENGTH..=PASSWORD_MAX_LENGTH).contains(&length)
        && !text.chars().any(char::is_control)
}

/// Whether `text` is an `ApiKey`: 1 to 512 printable ASCII characters
/// with no spaces, which every provider's keys are, whatever their prefix.
pub fn is_api_key(text: &str) -> bool {
    (1..=API_KEY_MAX_LENGTH).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_graphic())
}

/// Whether `text` is a `BearerToken`: RFC 6750's `b64token`, one or more
/// of letters, digits, `-`, `.`, `_`, `~`, `+` and `/`, then any `=`, at
/// most 8,192 characters.
pub fn is_bearer_token(text: &str) -> bool {
    let body = text.trim_end_matches('=');
    text.len() <= BEARER_TOKEN_MAX_LENGTH
        && !body.is_empty()
        && body
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~+/".contains(&b))
}

/// The bytes a `TotpSecret`'s text holds: RFC 4648 Base32 in upper case
/// with no padding, in its one spelling (the bits past the last whole byte
/// zero), as authenticator apps read a key; `None` for anything else.
pub fn decode_base32(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 5 / 8);
    let mut bits: u32 = 0;
    let mut held: u32 = 0;
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'2'..=b'7' => byte - b'2' + 26,
            _ => return None,
        };
        bits = (bits << 5) | u32::from(value);
        held += 5;
        if held >= 8 {
            held -= 8;
            out.push((bits >> held) as u8);
            bits &= (1 << held) - 1;
        }
    }
    // What is left over is padding bits, fewer than 5 and all zero; a
    // length no bytes give (1, 3 or 6 characters over) leaves 5 or more.
    (held < 5 && bits == 0 && !text.is_empty()).then_some(out)
}

/// Whether `text` is a `TotpSecret`: 16 to 64 bytes as Base32
/// ([`decode_base32`]).
pub fn is_totp_secret(text: &str) -> bool {
    decode_base32(text)
        .is_some_and(|bytes| (TOTP_SECRET_MIN_BYTES..=TOTP_SECRET_MAX_BYTES).contains(&bytes.len()))
}

/// Whether `text` is a `SecretText`: 1 to 8,192 characters of any kind,
/// line breaks included.
pub fn is_secret_text(text: &str) -> bool {
    (1..=SECRET_TEXT_MAX_LENGTH).contains(&text.chars().count())
}

/// What every secret value shares: read from text and JSON, never
/// written, shown or stored as itself, and not ordered.
const fn secret_entry(
    name: &'static str,
    pattern: &'static str,
    validator: Validator,
    examples: Examples,
) -> StandardType {
    StandardType {
        name,
        holds: Holds::Text {
            pattern,
            personal: None,
        },
        text: Some(TextForm::Secret),
        json: Json::Secret,
        storage: Storage::Never,
        format: Format {
            key: FormatKey::Redacted,
            options: NO_OPTIONS,
        },
        ordered: false,
        validator: Some(validator),
        shape: None,
        examples,
        release: FIRST_RELEASE,
        signals: crate::standard::NO_SIGNALS,
    }
}

/// A password as a person types it (area 19, chunk 5.5r2): taken from an
/// `Input`, hashed with `crypto.hash_password`, never kept or sent back.
pub const PASSWORD_ENTRY: StandardType = secret_entry(
    PASSWORD,
    r"[^\p{Cc}]+",
    Validator::Password,
    Examples {
        valid: &[
            ("correct horse battery staple", "<redacted Password>"),
            ("Tr0ub4dor&3", "<redacted Password>"),
            ("mot de passe très long", "<redacted Password>"),
        ],
        format: None,
        // Seven characters, a tab, a line break, nothing at all.
        malformed: &["Tr0ub4d", "tab\there!", "two\nlines", ""],
        of: None,
        settings: "",
    },
);

/// An API key (area 19, chunk 5.5r2): kept, where it must be, as a hash.
pub const API_KEY_ENTRY: StandardType = secret_entry(
    API_KEY,
    r"[\x21-\x7E]+",
    Validator::ApiKey,
    Examples {
        valid: &[
            ("example_key_0123456789abcdef", "<redacted ApiKey>"),
            ("AKEXAMPLE7QZ2", "<redacted ApiKey>"),
        ],
        format: None,
        // A space, a line break, a letter outside ASCII, nothing at all.
        malformed: &["example key", "example\nkey", "exämple_key", ""],
        of: None,
        settings: "",
    },
);

/// An access token sent as `Authorization: Bearer` (RFC 6750, area 19,
/// chunk 5.5r2).
pub const BEARER_TOKEN_ENTRY: StandardType = secret_entry(
    BEARER_TOKEN,
    r"[A-Za-z0-9\-._~+/]+=*",
    Validator::BearerToken,
    Examples {
        // RFC 6750 §2.1's example, and one padded.
        valid: &[
            ("mF_9.B5f-4.1JqM", "<redacted BearerToken>"),
            ("dGhpcyBpcyBhIHRva2Vu==", "<redacted BearerToken>"),
        ],
        format: None,
        // A space, `=` before the end, a character outside the grammar,
        // only padding, nothing at all.
        malformed: &["mF_9 B5f", "mF=9", "mF_9!B5f", "==", ""],
        of: None,
        settings: "",
    },
);

/// An authenticator's shared secret (RFC 6238, area 19, chunk 5.5r2), as
/// RFC 4648 Base32: stored once keys held as data arrive.
pub const TOTP_SECRET_ENTRY: StandardType = secret_entry(
    TOTP_SECRET,
    r"[A-Z2-7]+",
    Validator::TotpSecret,
    Examples {
        // RFC 6238's test secret, `12345678901234567890`, and 16 bytes,
        // the fewest.
        valid: &[
            ("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ", "<redacted TotpSecret>"),
            ("AAAQEAYEAUDAOCAJBIFQYDIOB4", "<redacted TotpSecret>"),
        ],
        format: None,
        // Lower case, padded, ten bytes, a bit set past the last byte, a
        // digit Base32 doesn't use, nothing at all.
        malformed: &[
            "gezdgnbvgy3tqojqgezdgnbvgy3tqojq",
            "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ====",
            "GAYTEMZUGU3DOOBZ",
            "AAAQEAYEAUDAOCAJBIFQYDIOB5",
            "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJ1",
            "",
        ],
        of: None,
        settings: "",
    },
);

/// Any other secret (area 19, chunk 5.5r2): never printed in a log, a
/// failure or a trace.
pub const SECRET_TEXT_ENTRY: StandardType = secret_entry(
    SECRET_TEXT,
    r"(?s).+",
    Validator::SecretText,
    Examples {
        valid: &[
            ("whsec_example", "<redacted SecretText>"),
            (
                "-----BEGIN EXAMPLE-----\nAAAA\n-----END EXAMPLE-----",
                "<redacted SecretText>",
            ),
        ],
        format: None,
        malformed: &[""],
        of: None,
        settings: "",
    },
);

/// The fewest and most digits a `OneTimeCode` has: RFC 4226's six to
/// eight (§5.3, "at least 6 digits"), which RFC 6238 keeps (chosen in
/// chunk 5.5r3).
pub const ONE_TIME_CODE_DIGITS: (usize, usize) = (6, 8);

/// Whether `text` is a `OneTimeCode`: six to eight ASCII digits, a leading
/// zero kept, as an authenticator app or a text message shows one.
pub fn is_one_time_code(text: &str) -> bool {
    (ONE_TIME_CODE_DIGITS.0..=ONE_TIME_CODE_DIGITS.1).contains(&text.len())
        && text.bytes().all(|b| b.is_ascii_digit())
}

/// The code a person types from an authenticator app, a text message or
/// an email (area 19, chunk 5.5r3; RFC 4226, RFC 6238): a secret value,
/// checked when it arrives and never kept, so a code can't be replayed
/// from a log or a response.
pub const ONE_TIME_CODE_ENTRY: StandardType = secret_entry(
    ONE_TIME_CODE,
    r"[0-9]{6,8}",
    Validator::OneTimeCode,
    Examples {
        // RFC 4226's HOTP value for count 1, and RFC 6238's eight-digit
        // TOTP at 59 seconds, with a leading zero kept.
        valid: &[
            ("287082", "<redacted OneTimeCode>"),
            ("94287082", "<redacted OneTimeCode>"),
            ("012345", "<redacted OneTimeCode>"),
        ],
        format: None,
        // Five digits, nine, a space, digits that aren't ASCII, letters,
        // nothing at all.
        malformed: &[
            "28708",
            "942870820",
            "287 082",
            "\u{661}\u{662}\u{663}\u{664}\u{665}\u{666}",
            "abcdef",
            "",
        ],
        of: None,
        settings: "",
    },
);

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
    signals: crate::standard::NO_SIGNALS,
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
    signals: crate::standard::NO_SIGNALS,
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
    signals: crate::standard::NO_SIGNALS,
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
    // A token is a credential, so a secret value since chunk 5.5r2: read,
    // never written, shown or stored (decided by the lead session).
    text: Some(TextForm::Secret),
    json: Json::Secret,
    storage: Storage::Never,
    format: Format {
        key: FormatKey::Redacted,
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
                "<redacted Jwt>",
            ),
            (
                "eyJhbGciOiJFZERTQSJ9.RXhhbXBsZSBvZiBFZDI1NTE5IHNpZ25pbmc.hgyY0il_MGCjP0JzlnLWG1PPOt7-09PGcvMg3AIbQR6dWbhijcNR4ki4iylGjg5BhVsPt9g7sVvpAr_MuM0KAg",
                "<redacted Jwt>",
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
    signals: crate::standard::NO_SIGNALS,
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
    storage: Storage::Columns {
        parts: &[
            ("iss", "text"),
            ("sub", "text"),
            ("aud", "text[]"),
            ("exp", "timestamptz(3)"),
            ("nbf", "timestamptz(3)"),
            ("iat", "timestamptz(3)"),
            ("jti", "text"),
        ],
        fixed: &[],
    },
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
    signals: crate::standard::NO_SIGNALS,
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
    storage: Storage::Columns {
        parts: &[
            ("kty", "text"),
            ("crv", "text"),
            ("x", "text"),
            ("kid", "text"),
            ("alg", "text"),
            ("use", "text"),
        ],
        fixed: &[],
    },
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
    signals: crate::standard::NO_SIGNALS,
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
    signals: crate::standard::NO_SIGNALS,
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
    storage: Storage::Columns {
        parts: &[
            ("sub", "text"),
            ("name", "text"),
            ("given_name", "text"),
            ("family_name", "text"),
            ("preferred_username", "text"),
            ("email", "text"),
            ("email_verified", "boolean"),
            ("phone_number", "text"),
            ("phone_number_verified", "boolean"),
            ("picture", "text"),
        ],
        fixed: &[],
    },
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
    signals: crate::standard::NO_SIGNALS,
};

/// How many bytes a `Hash` and an `Hmac` hold: SHA-256's digest, which
/// HMAC-SHA-256's tag is too (FIPS 180-4, RFC 2104).
pub const DIGEST_BYTES: usize = 32;

/// The sealed format's first two bytes, its version and its algorithm
/// (XChaCha20-Poly1305), and the fewest bytes a sealed value has: those
/// two, the key's identity (4), the nonce (24) and the tag (16), with
/// nothing sealed (chosen in chunk 5.5q2; `jadpo-value`'s `crypto` module
/// writes them).
pub const SEALED_VERSION: u8 = 1;
pub const SEALED_ALGORITHM: u8 = 1;
pub const SEALED_MIN_BYTES: usize = 2 + 4 + 24 + 16;

/// Whether `bytes` are a `Hash`: a SHA-256 digest's 32.
pub fn is_hash(bytes: &[u8]) -> bool {
    bytes.len() == DIGEST_BYTES
}

/// Whether `bytes` are an `Hmac`: an HMAC-SHA-256 tag's 32.
pub fn is_hmac(bytes: &[u8]) -> bool {
    bytes.len() == DIGEST_BYTES
}

/// Whether `bytes` are an `EncryptedValue`: the sealed format's version
/// and algorithm, and room for its key identity, nonce and tag. Whether
/// they open is `crypto.decrypt`'s to say, with the key.
pub fn is_sealed(bytes: &[u8]) -> bool {
    bytes.len() >= SEALED_MIN_BYTES
        && bytes.first() == Some(&SEALED_VERSION)
        && bytes.get(1) == Some(&SEALED_ALGORITHM)
}

/// The bytes `text` holds in RFC 4648's standard alphabet, padded, in its
/// one spelling, as area 2 writes every `Bytes` in JSON; `None` for
/// anything else, an empty text included.
pub fn decode_base64(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(4) || text.contains(['-', '_']) {
        return None;
    }
    let body = text.trim_end_matches('=');
    let padding = text.len() - body.len();
    if padding != (4 - body.len() % 4) % 4 {
        return None;
    }
    decode_base64url(&body.replace('+', "-").replace('/', "_"))
}

/// The standard type built on `Bytes` whose check every result of `op`
/// passes (chunk 5.5r3), so building one from the call needs no `attempt`:
/// `Hash(hash.sha256(body))`, `Hmac(crypto.hmac(body, key: ...))`,
/// `EncryptedValue(crypto.encrypt(...))`.
pub fn made_by(op: crate::Op) -> Option<&'static str> {
    match op {
        crate::Op::HashSha256 => Some(HASH),
        crate::Op::CryptoHmac => Some(HMAC),
        crate::Op::CryptoEncrypt => Some(ENCRYPTED_VALUE),
        _ => None,
    }
}

/// A type built on `Bytes` (chunk 5.5r3): its canonical text and its JSON
/// are area 2's Base64 of its bytes, so a field of one is sent as a field
/// of `Bytes` is; it is stored as `bytea`, shown as its text, and not
/// ordered.
const fn bytes_entry(name: &'static str, validator: Validator, examples: Examples) -> StandardType {
    StandardType {
        name,
        holds: Holds::Bytes,
        text: Some(TextForm::Base64),
        json: Json::Text,
        storage: Storage::Column(SqlType::Bytes),
        format: Format {
            key: FormatKey::Canonical,
            options: NO_OPTIONS,
        },
        ordered: false,
        validator: Some(validator),
        shape: None,
        examples,
        release: FIRST_RELEASE,
        signals: crate::standard::NO_SIGNALS,
    }
}

/// A SHA-256 digest (area 19, chunk 5.5r3; FIPS 180-4): what `hash.sha256`
/// gives, kept as a key's verifier or a file's content key.
pub const HASH_ENTRY: StandardType = bytes_entry(
    HASH,
    Validator::Hash,
    Examples {
        // FIPS 180-4's digests of nothing and of `abc`.
        valid: &[
            (
                "47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=",
                "47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=",
            ),
            (
                "ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0=",
                "ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0=",
            ),
        ],
        format: None,
        // 31 bytes, 33, hexadecimal, unpadded, the URL alphabet, nothing.
        malformed: &[
            "ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFQ==",
            "ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0A",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            "47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU",
            "47DEQpj8HBSa-_TImW-5JCeuQeRkm5NMpJWZG3hSuFU=",
            "",
        ],
        of: None,
        settings: "",
    },
);

/// An HMAC-SHA-256 tag (area 19, chunk 5.5r3; RFC 2104): what
/// `crypto.hmac` gives, a webhook's signature among them.
pub const HMAC_ENTRY: StandardType = bytes_entry(
    HMAC,
    Validator::Hmac,
    Examples {
        // RFC 4231's first and second test cases.
        valid: &[
            (
                "sDRMYdjbOFNcqK/OrwvxK4gdwgDJgz2nJuk3bC4yz/c=",
                "sDRMYdjbOFNcqK/OrwvxK4gdwgDJgz2nJuk3bC4yz/c=",
            ),
            (
                "W9zBRr9gdU5qBCQmCJV1x1oAPwidJzmDnexYuWTsOEM=",
                "W9zBRr9gdU5qBCQmCJV1x1oAPwidJzmDnexYuWTsOEM=",
            ),
        ],
        format: None,
        // 31 bytes, 33, unpadded, nothing.
        malformed: &[
            "ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFQ==",
            "ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0A",
            "sDRMYdjbOFNcqK/OrwvxK4gdwgDJgz2nJuk3bC4yz/c",
            "",
        ],
        of: None,
        settings: "",
    },
);

/// A value sealed by `crypto.encrypt` (area 19, chunk 5.5r3; the format
/// chosen in chunk 5.5q2): ciphertext, so it may be stored and sent, and
/// only `crypto.decrypt`, with the key, reads what it holds.
pub const ENCRYPTED_VALUE_ENTRY: StandardType = bytes_entry(
    ENCRYPTED_VALUE,
    Validator::EncryptedValue,
    Examples {
        // Nothing sealed, and five bytes, each under key identity
        // a1b2c3d4.
        valid: &[
            (
                "AQGhssPUAAECAwQFBgcICQoLDA0ODxAREhMUFRYXgIGCg4SFhoeIiYqLjI2Ojw==",
                "AQGhssPUAAECAwQFBgcICQoLDA0ODxAREhMUFRYXgIGCg4SFhoeIiYqLjI2Ojw==",
            ),
            (
                "AQGhssPUGBkaGxwdHh8gISIjJCUmJygpKissLS4vaGVsbG+QkZKTlJWWl5iZmpucnZ6f",
                "AQGhssPUGBkaGxwdHh8gISIjJCUmJygpKissLS4vaGVsbG+QkZKTlJWWl5iZmpucnZ6f",
            ),
        ],
        format: None,
        // Version 2, algorithm 2, a byte short of a tag, the URL alphabet
        // unpadded, nothing.
        malformed: &[
            "AgGhssPUAAECAwQFBgcICQoLDA0ODxAREhMUFRYXgIGCg4SFhoeIiYqLjI2Ojw==",
            "AQKhssPUAAECAwQFBgcICQoLDA0ODxAREhMUFRYXgIGCg4SFhoeIiYqLjI2Ojw==",
            "AQGhssPUAAECAwQFBgcICQoLDA0ODxAREhMUFRYXgIGCg4SFhoeIiYqLjI2O",
            "AQGhssPUAAECAwQFBgcICQoLDA0ODxAREhMUFRYXgIGCg4SFhoeIiYqLjI2Ojw",
            "",
        ],
        of: None,
        settings: "",
    },
);

/// `PasswordPolicy`'s parts, in order (NIST SP 800-63B, chunk 5.5r3): the
/// fewest characters, from NIST's 8 (15 where a password is the only
/// factor) to a `Password`'s 256; the most, at least NIST's 64; and whether
/// a new password is compared against a list of breached ones. There are no
/// composition rules and no expiry, which NIST says not to impose.
pub const PASSWORD_POLICY_FIELDS: [(&str, Part); 3] = [
    ("min_length", Part::Whole(8, 256)),
    ("max_length", Part::Whole(64, 256)),
    ("check_breached", Part::Of(Builtin::Bool)),
];

/// `PasswordPolicy`'s rule across its parts (chunk 5.5t2): its fewest
/// characters are never more than its most (chunk 5.5r3).
pub const PASSWORD_POLICY_RULES: &[StandardRule] = &[StandardRule {
    name: "min_not_above_max",
    written: "min_length <= max_length",
    label: "more than its `max_length`",
    reason: "A password policy's fewest characters can't be more than its most, or no password would meet it.",
    next: "Give `min_length` a number no greater than `max_length`",
    check: in_order(&[Order::at_most("min_length", "max_length")], "min_length"),
}];

/// The rules a password must meet (NIST SP 800-63B, area 19, chunk 5.5r3):
/// sent as an object of its parts, stored as that JSON, and shown as its
/// lengths, `15–64`.
pub const PASSWORD_POLICY_ENTRY: StandardType = StandardType {
    name: PASSWORD_POLICY,
    holds: Holds::Object(&PASSWORD_POLICY_FIELDS),
    text: None,
    json: Json::Fields(&["min_length", "max_length", "check_breached"]),
    storage: Storage::Columns {
        parts: &[
            ("min_length", "smallint"),
            ("max_length", "smallint"),
            ("check_breached", "boolean"),
        ],
        fixed: &[],
    },
    format: Format {
        key: FormatKey::Parts {
            parts: &["min_length", "max_length"],
            separator: "–",
        },
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: Examples {
        valid: &[
            (
                r#"{"min_length":"15","max_length":"64","check_breached":true}"#,
                "15–64",
            ),
            (
                r#"{"min_length":"8","max_length":"256","check_breached":false}"#,
                "8–256",
            ),
        ],
        format: None,
        // Fewer than NIST's 8, a most below 64, a fewest above the most, a
        // composition rule it doesn't have, no breach check said, and not
        // an object.
        malformed: &[
            r#"{"min_length":"6","max_length":"64","check_breached":true}"#,
            r#"{"min_length":"8","max_length":"32","check_breached":true}"#,
            r#"{"min_length":"100","max_length":"64","check_breached":true}"#,
            r#"{"min_length":"8","max_length":"64","check_breached":true,"require_digit":true}"#,
            r#"{"min_length":"8","max_length":"64"}"#,
            r#""15-64""#,
        ],
        of: None,
        settings: "",
    },
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// The transports a passkey says it can be reached by, as WebAuthn names
/// them and `webauthn-rs` reads them (chunk 5.5r3): `smart-card`, which
/// `webauthn-rs` doesn't read, and its own `test` aren't kept.
pub const WEB_AUTHN_TRANSPORT_PATTERN: &str = "usb|nfc|ble|internal|hybrid";

/// `WebAuthnCredential`'s parts, in order (W3C WebAuthn Level 3, chunk
/// 5.5r3), each mapped to `webauthn-rs` 0.5's stored `Credential`:
/// `credential_id` its `cred_id` (WebAuthn's at most 1,023 bytes),
/// `public_key` its `cred` as the authenticator gave it, a COSE_Key (RFC
/// 9052 §7) in CBOR, `sign_count` its `counter` (a `u32`), `transports` its
/// `transports` (none for an empty list), `backup_eligible` and `backed_up`
/// its `backup_eligible` and `backup_state`; the person's `name` for it and
/// when it was made and last used are the application's. No biometric is
/// held: the authenticator keeps that.
pub const WEB_AUTHN_CREDENTIAL_FIELDS: [(&str, Part); 9] = [
    (
        "credential_id",
        Part::Bounded {
            of: Builtin::Bytes,
            min: Some(crate::Num {
                mantissa: 1,
                scale: 0,
            }),
            max: Some(crate::Num {
                mantissa: 1023,
                scale: 0,
            }),
        },
    ),
    (
        "public_key",
        Part::Bounded {
            of: Builtin::Bytes,
            min: Some(crate::Num {
                mantissa: 1,
                scale: 0,
            }),
            max: Some(crate::Num {
                mantissa: 2048,
                scale: 0,
            }),
        },
    ),
    ("sign_count", Part::Whole(0, 4_294_967_295)),
    (
        "transports",
        Part::List(&Part::Matching(WEB_AUTHN_TRANSPORT_PATTERN)),
    ),
    ("backup_eligible", Part::Of(Builtin::Bool)),
    ("backed_up", Part::Of(Builtin::Bool)),
    ("name", Part::Line),
    ("registered_at", Part::Of(Builtin::Instant)),
    ("last_used_at", Part::Optional(&Part::Of(Builtin::Instant))),
];

/// A passkey (area 19, chunk 5.5r3; area 8, "Passkeys"): what the server
/// keeps of a credential a person registered, sent with its parts' names,
/// stored as that JSON, and shown by the name the person gave it.
pub const WEB_AUTHN_CREDENTIAL_ENTRY: StandardType = StandardType {
    name: WEB_AUTHN_CREDENTIAL,
    holds: Holds::Object(&WEB_AUTHN_CREDENTIAL_FIELDS),
    text: None,
    json: Json::Members(&[
        "credential_id",
        "public_key",
        "sign_count",
        "transports",
        "backup_eligible",
        "backed_up",
        "name",
        "registered_at",
        "last_used_at",
    ]),
    storage: Storage::Columns {
        parts: &[
            ("credential_id", "bytea"),
            ("public_key", "bytea"),
            ("sign_count", "bigint"),
            ("transports", "text[]"),
            ("backup_eligible", "boolean"),
            ("backed_up", "boolean"),
            ("name", "text"),
            ("registered_at", "timestamptz(3)"),
            ("last_used_at", "timestamptz(3)"),
        ],
        fixed: &[],
    },
    format: Format {
        key: FormatKey::FirstPart(&["name"]),
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: Examples {
        // An ES256 key (`webauthn-rs`'s own COSE test vector) on a phone,
        // synced, used once; an Ed25519 key (RFC 8032's first) on a
        // security key, never used since it was made.
        valid: &[
            (
                r#"{"credential_id":"AAECAwQFBgcICQoLDA0ODw==","public_key":"pQECAyYgASFYIGXtpaEld8K66ClDf+M4cBoQqqN14btbXeEI3kOcCFUdIlggHlLtdXARY/f55A3fnzQbPcm6hgr34Mp8p+nuzQCE0Zw=","sign_count":"0","transports":["internal","hybrid"],"backup_eligible":true,"backed_up":true,"name":"Work phone","registered_at":"2026-10-18T09:00:00Z","last_used_at":"2026-10-18T09:05:00Z"}"#,
                "Work phone",
            ),
            (
                r#"{"credential_id":"8dA=","public_key":"pAEBAycgBiFYINdamAGCsQq31Uv+08lkBzoO4XLz2qYjJa8CGmj3B1Ea","sign_count":"42","transports":["usb","nfc"],"backup_eligible":false,"backed_up":false,"name":"Security key","registered_at":"2026-10-18T09:00:00Z"}"#,
                "Security key",
            ),
        ],
        format: None,
        // No credential ID, a count past a `u32`, a transport WebAuthn
        // doesn't name, an empty name, a biometric it doesn't hold, and not
        // an object.
        malformed: &[
            r#"{"credential_id":"","public_key":"8dA=","sign_count":"0","transports":[],"backup_eligible":false,"backed_up":false,"name":"Phone","registered_at":"2026-10-18T09:00:00Z"}"#,
            r#"{"credential_id":"8dA=","public_key":"8dA=","sign_count":"4294967296","transports":[],"backup_eligible":false,"backed_up":false,"name":"Phone","registered_at":"2026-10-18T09:00:00Z"}"#,
            r#"{"credential_id":"8dA=","public_key":"8dA=","sign_count":"0","transports":["bluetooth"],"backup_eligible":false,"backed_up":false,"name":"Phone","registered_at":"2026-10-18T09:00:00Z"}"#,
            r#"{"credential_id":"8dA=","public_key":"8dA=","sign_count":"0","transports":[],"backup_eligible":false,"backed_up":false,"name":"","registered_at":"2026-10-18T09:00:00Z"}"#,
            r#"{"credential_id":"8dA=","public_key":"8dA=","sign_count":"0","transports":[],"backup_eligible":false,"backed_up":false,"name":"Phone","registered_at":"2026-10-18T09:00:00Z","fingerprint":"8dA="}"#,
            r#""8dA=""#,
        ],
        of: None,
        settings: "",
    },
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
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

    #[test]
    fn records_built_on_bytes_hold_what_crypto_gives() {
        for entry in [HASH_ENTRY, HMAC_ENTRY, ENCRYPTED_VALUE_ENTRY] {
            for (valid, _) in entry.examples.valid {
                assert!(entry.accepts(valid), "{}: {valid}", entry.name);
            }
            for malformed in entry.examples.malformed {
                assert!(!entry.accepts(malformed), "{}: {malformed}", entry.name);
            }
        }
        assert_eq!(decode_base64("AQAB"), Some(vec![1, 0, 1]));
        assert_eq!(decode_base64("AQ=="), Some(vec![1]));
        // Unpadded, over-padded, bits past the last byte, the URL alphabet.
        assert_eq!(decode_base64("AQ"), None);
        assert_eq!(decode_base64("AQ==="), None);
        assert_eq!(decode_base64("AR=="), None);
        assert_eq!(decode_base64("A-_B"), None);
        assert!(is_sealed(&[[1, 1].as_slice(), &[0; 44]].concat()));
        assert!(!is_sealed(&[[1, 1].as_slice(), &[0; 43]].concat()));
        assert_eq!(made_by(crate::Op::HashSha256), Some(HASH));
        assert_eq!(made_by(crate::Op::CryptoSign), None);
    }

    #[test]
    fn a_one_time_code_is_six_to_eight_digits() {
        assert!(is_one_time_code("000000"));
        assert!(is_one_time_code("12345678"));
        assert!(!is_one_time_code("1234567a"));
        assert!(!is_one_time_code("123456789"));
    }

    #[test]
    fn secret_values_have_their_bounds() {
        for entry in [
            PASSWORD_ENTRY,
            API_KEY_ENTRY,
            BEARER_TOKEN_ENTRY,
            TOTP_SECRET_ENTRY,
            SECRET_TEXT_ENTRY,
            JWT_ENTRY,
            ONE_TIME_CODE_ENTRY,
        ] {
            assert_eq!(secret_value(entry.name), Some(entry.name));
            for (valid, _) in entry.examples.valid {
                assert!(entry.accepts(valid), "{}: {valid}", entry.name);
            }
            for malformed in entry.examples.malformed {
                assert!(!entry.accepts(malformed), "{}: {malformed}", entry.name);
            }
        }
        assert_eq!(secret_value(PASSWORD_HASH), None);
        // A password's length is counted in characters, at both ends.
        assert!(is_password("ééééééé!"));
        assert!(is_password(&"é".repeat(PASSWORD_MAX_LENGTH)));
        assert!(!is_password(&"é".repeat(PASSWORD_MAX_LENGTH + 1)));
        assert!(is_api_key(&"k".repeat(API_KEY_MAX_LENGTH)));
        assert!(!is_api_key(&"k".repeat(API_KEY_MAX_LENGTH + 1)));
        assert!(!is_bearer_token(&"t".repeat(BEARER_TOKEN_MAX_LENGTH + 1)));
        assert!(is_secret_text(&"s".repeat(SECRET_TEXT_MAX_LENGTH)));
        assert!(!is_secret_text(&"s".repeat(SECRET_TEXT_MAX_LENGTH + 1)));
        // 64 bytes is the most a TOTP secret holds.
        let most = "AAAQEAYEAUDAOCAJBIFQYDIOB4IBCEQTCQKRMFYYDENBWHA5DYPSAIJCEMSCKJRHFAUSUKZMFUXC6MBRGIZTINJWG44DSOR3HQ6T4PY";
        assert_eq!(decode_base32(most).map(|bytes| bytes.len()), Some(64));
        assert!(is_totp_secret(most));
        assert!(!is_totp_secret(&format!("{most}AA")));
        assert_eq!(
            decode_base32("GEZDGNBVGY3TQOJQ").as_deref(),
            Some(&b"1234567890"[..])
        );
        assert!(stored_encrypted_later(TOTP_SECRET) && !stored_encrypted_later(PASSWORD));
    }
}
