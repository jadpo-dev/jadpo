//! Signed credentials (area 8, "The application and revocation"; chosen in
//! chunk 5.1b): with `revocation_delay:`, a browser session, an API token
//! and a token exchanged for a service key carry a part the runtime signed
//! when it last read the caller's record, which signs the request in
//! without a read until it expires, never later than the delay after that
//! read.
//!
//! A signed part is `<payload>.<tag>`, both base64url without padding: the
//! payload is JSON of what the read found (the credential's method, its
//! row and verifier, the caller's `id` and subject, when it expires, and a
//! browser session's CSRF verifier) and the encoding it was written in
//! (area 19, "Changing a standard type's encoding"; chunk 5.9), the tag
//! its keyed BLAKE3 hash under a key derived from the method's
//! `SigningKey` setting. New parts are signed with the setting's current
//! value; a part signed with its `_PREVIOUS` value is still accepted (area
//! 10, "Signing keys"). The tag is compared in constant time, and the code
//! is host-agnostic, so it builds for wasm32 as it does natively.

use std::fmt;

use jadpo_std::AuthenticationMethod;
use jadpo_std::http::IdentityPlan;
use jadpo_std::standard::ENCODING;

use crate::SqlValue;
use crate::config::ConfigValues;

/// What a key is derived for: BLAKE3's key derivation keeps these keys
/// apart from any other use of the same setting.
const CONTEXT: &str = "jadpo 2026-10-16 signed credential v1";

/// One method's signing key: the current value and, while a rotation
/// lasts, the previous one.
#[derive(Clone)]
pub struct SigningKey {
    current: [u8; 32],
    previous: Option<[u8; 32]>,
}

impl fmt::Debug for SigningKey {
    /// Never the key.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SigningKey")
            .field("previous", &self.previous.is_some())
            .finish()
    }
}

impl SigningKey {
    /// The key a `SigningKey` setting's text gives, and its previous
    /// value's.
    pub fn new(current: &str, previous: Option<&str>) -> SigningKey {
        SigningKey {
            current: blake3::derive_key(CONTEXT, current.as_bytes()),
            previous: previous.map(|text| blake3::derive_key(CONTEXT, text.as_bytes())),
        }
    }

    /// `payload` signed with the current key: `<payload>.<tag>`.
    pub fn sign(&self, payload: &str) -> String {
        let body = base64url(payload.as_bytes());
        let tag = blake3::keyed_hash(&self.current, body.as_bytes());
        format!("{body}.{}", base64url(tag.as_bytes()))
    }

    /// The payload of a part signed with the current or the previous key;
    /// `None` when neither signed it.
    pub fn open(&self, signed: &str) -> Option<String> {
        let (body, tag) = signed.split_once('.')?;
        let tag: [u8; 32] = unbase64url(tag)?.try_into().ok()?;
        let sent = blake3::Hash::from_bytes(tag);
        let signed_by = |key: &[u8; 32]| blake3::keyed_hash(key, body.as_bytes()) == sent;
        if !(signed_by(&self.current) || self.previous.as_ref().is_some_and(signed_by)) {
            return None;
        }
        String::from_utf8(unbase64url(body)?).ok()
    }
}

/// The settings signing in reads (area 10): each method's signing key and
/// the browser session's origin, read once as the application starts.
#[derive(Clone, Default)]
pub struct AuthSettings {
    keys: Vec<(AuthenticationMethod, SigningKey)>,
    /// `scheme://host[:port]`, in lower case and without a default port.
    origin: Option<String>,
    external: Option<super::external_token::Settings>,
}

impl fmt::Debug for AuthSettings {
    /// The methods that have keys and the origin; never a key.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let methods: Vec<&str> = self.keys.iter().map(|(method, _)| method.name()).collect();
        f.debug_struct("AuthSettings")
            .field("keys", &methods)
            .field("origin", &self.origin)
            .finish()
    }
}

impl AuthSettings {
    /// No keys and no origin: signed parts are neither made nor accepted,
    /// and a cookie write's `Origin` must name the server.
    pub const NONE: AuthSettings = AuthSettings {
        keys: Vec::new(),
        origin: None,
        external: None,
    };

    /// What `plan`'s `signing_key:` and `origin:` settings hold in
    /// `values`. A setting no value reached gives nothing.
    pub fn read(plan: Option<&IdentityPlan>, values: &ConfigValues) -> AuthSettings {
        let Some(plan) = plan else {
            return AuthSettings::default();
        };
        let keys = plan
            .signing_keys
            .iter()
            .filter_map(|(method, setting)| {
                let current = values.text(setting)?;
                Some((*method, SigningKey::new(current, values.previous(setting))))
            })
            .collect();
        let origin = plan
            .origin
            .as_deref()
            .and_then(|setting| values.text(setting))
            .and_then(origin_of);
        let external = plan
            .external_token
            .as_ref()
            .and_then(|plan| super::external_token::Settings::read(plan, values));
        AuthSettings {
            keys,
            origin,
            external,
        }
    }

    /// The external issuer and audience bound at startup.
    pub fn external(&self) -> Option<&super::external_token::Settings> {
        self.external.as_ref()
    }

    /// `method`'s signing key.
    pub fn key(&self, method: AuthenticationMethod) -> Option<&SigningKey> {
        self.keys
            .iter()
            .find(|(found, _)| *found == method)
            .map(|(_, key)| key)
    }

    /// The origin a cookie write must come from, when one is configured.
    pub fn origin(&self) -> Option<&str> {
        self.origin.as_deref()
    }
}

/// A URL's origin as a browser sends it in `Origin`: its scheme and
/// authority in lower case, without a path or a default port.
pub fn origin_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.trim().split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next()?.to_ascii_lowercase();
    let scheme = scheme.to_ascii_lowercase();
    if authority.is_empty() {
        return None;
    }
    let authority = match (scheme.as_str(), authority.rsplit_once(':')) {
        ("https", Some((host, "443"))) | ("http", Some((host, "80"))) => host.to_string(),
        _ => authority,
    };
    Some(format!("{scheme}://{authority}"))
}

/// What a signed part says: what the record held when it was read.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Claims {
    pub method: AuthenticationMethod,
    /// The session's or API token's row, which `sessions.end()` ends;
    /// `None` for an exchanged service token.
    pub row: Option<[u8; 16]>,
    /// The credential's verifier, in hexadecimal: its secret's, or for an
    /// exchanged token the service key's, which a `fresh` route looks the
    /// record up by.
    pub verifier: String,
    pub principal: [u8; 16],
    pub subject: SqlValue,
    /// When it stops signing in, in milliseconds since 1970.
    pub expires: i64,
    /// A browser session's CSRF token's verifier.
    pub csrf: Option<String>,
}

impl Claims {
    /// The payload, in this release's encoding, which it records as `enc`.
    pub fn to_json(&self) -> Option<String> {
        let mut object = serde_json::Map::new();
        object.insert("enc".into(), ENCODING.into());
        object.insert("m".into(), self.method.name().into());
        if let Some(row) = &self.row {
            object.insert("r".into(), hex(row).into());
        }
        object.insert("v".into(), self.verifier.clone().into());
        object.insert("p".into(), hex(&self.principal).into());
        object.insert("s".into(), subject_text(&self.subject)?.into());
        object.insert("e".into(), self.expires.into());
        if let Some(csrf) = &self.csrf {
            object.insert("c".into(), csrf.clone().into());
        }
        Some(serde_json::Value::Object(object).to_string())
    }

    /// What a payload says, read by the encoding it records: this
    /// release's, or none, a part signed before encodings were recorded
    /// (chunk 5.1b), whose members are encoding 1's. A part of any other
    /// encoding says nothing, so the request reads the record, as it does
    /// for one that has expired.
    pub fn from_json(text: &str) -> Option<Claims> {
        let serde_json::Value::Object(object) = serde_json::from_str(text).ok()? else {
            return None;
        };
        match object.get("enc") {
            None => Claims::members(&object),
            Some(encoding) if encoding.as_u64() == Some(u64::from(ENCODING)) => {
                Claims::members(&object)
            }
            Some(_) => None,
        }
    }

    /// Encoding 1's members.
    fn members(object: &serde_json::Map<String, serde_json::Value>) -> Option<Claims> {
        let text = |name: &str| object.get(name).and_then(|value| value.as_str());
        Some(Claims {
            method: AuthenticationMethod::from_name(text("m")?)?,
            row: match text("r") {
                Some(row) => Some(unhex16(row)?),
                None => None,
            },
            verifier: text("v")?.to_string(),
            principal: unhex16(text("p")?)?,
            subject: subject_value(text("s")?)?,
            expires: object.get("e")?.as_i64()?,
            csrf: text("c").map(str::to_string),
        })
    }
}

/// A subject as a signed part keeps it: its column's kind, then its text.
fn subject_text(value: &SqlValue) -> Option<String> {
    Some(match value {
        SqlValue::Text(text) => format!("t{text}"),
        SqlValue::Int2(n) => format!("2{n}"),
        SqlValue::Int4(n) => format!("4{n}"),
        SqlValue::Int8(n) => format!("8{n}"),
        SqlValue::Numeric(text) => format!("n{text}"),
        SqlValue::Uuid(bytes) => format!("u{}", hex(bytes)),
        SqlValue::Bool(flag) => format!("b{}", u8::from(*flag)),
        SqlValue::Date(n) => format!("d{n}"),
        SqlValue::Timestamp(micros) => format!("s{micros}"),
        SqlValue::Time(micros) => format!("h{micros}"),
        _ => return None,
    })
}

fn subject_value(text: &str) -> Option<SqlValue> {
    let mut chars = text.chars();
    let kind = chars.next()?;
    let rest = chars.as_str();
    Some(match kind {
        't' => SqlValue::Text(rest.to_string()),
        '2' => SqlValue::Int2(rest.parse().ok()?),
        '4' => SqlValue::Int4(rest.parse().ok()?),
        '8' => SqlValue::Int8(rest.parse().ok()?),
        'n' => SqlValue::Numeric(rest.to_string()),
        'u' => SqlValue::Uuid(unhex16(rest)?),
        'b' => SqlValue::Bool(rest == "1"),
        'd' => SqlValue::Date(rest.parse().ok()?),
        's' => SqlValue::Timestamp(rest.parse().ok()?),
        'h' => SqlValue::Time(rest.parse().ok()?),
        _ => return None,
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex16(text: &str) -> Option<[u8; 16]> {
    if text.len() != 32 {
        return None;
    }
    let mut out = [0u8; 16];
    for (at, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(text.get(at * 2..at * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// `bytes` in base64url, without padding.
fn base64url(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (at, byte)| {
            n | (u32::from(*byte) << (16 - 8 * at))
        });
        for at in 0..=chunk.len() {
            let index = ((n >> (18 - 6 * at)) & 0x3f) as usize;
            out.extend(ALPHABET.get(index).map(|digit| char::from(*digit)));
        }
    }
    out
}

/// The bytes base64url text without padding holds; `None` when it isn't
/// such text.
fn unbase64url(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let digits: Vec<u32> = text
        .bytes()
        .map(|byte| ALPHABET.iter().position(|c| *c == byte).map(|at| at as u32))
        .collect::<Option<_>>()?;
    for chunk in digits.chunks(4) {
        if chunk.len() == 1 {
            return None;
        }
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (at, digit)| n | (digit << (18 - 6 * at)));
        for at in 0..chunk.len() - 1 {
            out.push((n >> (16 - 8 * at)) as u8);
        }
    }
    // Only the canonical spelling: the bits padding leaves are zero.
    (base64url(&out) == text).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "0123456789abcdef0123456789abcdef";
    const OTHER: &str = "fedcba9876543210fedcba9876543210";

    #[test]
    fn base64url_round_trips_every_length() {
        for length in 0..40 {
            let bytes: Vec<u8> = (0..length).map(|n| (n * 37 + 11) as u8).collect();
            let text = base64url(&bytes);
            assert!(!text.contains('='));
            assert_eq!(unbase64url(&text), Some(bytes));
        }
        assert_eq!(base64url(b"\xfb\xff"), "-_8");
        assert_eq!(unbase64url("a"), None);
        assert_eq!(unbase64url("ab+c"), None);
        // `QQ` is `A`; `QR` spells it with bits padding should leave zero.
        assert_eq!(unbase64url("QQ"), Some(b"A".to_vec()));
        assert_eq!(unbase64url("QR"), None);
    }

    #[test]
    fn a_signed_part_opens_only_with_its_key() {
        let key = SigningKey::new(KEY, None);
        let signed = key.sign("{\"a\":1}");
        assert_eq!(key.open(&signed).as_deref(), Some("{\"a\":1}"));
        assert_eq!(SigningKey::new(OTHER, None).open(&signed), None);
        // A changed payload or tag opens with no key.
        let (body, tag) = signed.split_once('.').unwrap();
        let other_body = base64url(b"{\"a\":2}");
        assert_eq!(key.open(&format!("{other_body}.{tag}")), None);
        let mut flipped = tag.as_bytes().to_vec();
        flipped[0] = if flipped[0] == b'A' { b'B' } else { b'A' };
        let flipped = String::from_utf8(flipped).unwrap();
        assert_eq!(key.open(&format!("{body}.{flipped}")), None);
        assert_eq!(key.open(body), None);
    }

    #[test]
    fn a_rotated_key_still_opens_what_the_previous_one_signed() {
        let old = SigningKey::new(OTHER, None);
        let signed = old.sign("payload");
        let rotated = SigningKey::new(KEY, Some(OTHER));
        assert_eq!(rotated.open(&signed).as_deref(), Some("payload"));
        // New parts are signed with the current key alone.
        let fresh = rotated.sign("payload");
        assert_eq!(old.open(&fresh), None);
        assert_eq!(
            SigningKey::new(KEY, None).open(&fresh).as_deref(),
            Some("payload")
        );
    }

    #[test]
    fn claims_round_trip_with_every_kind_of_subject() {
        for subject in [
            SqlValue::Text("ann".into()),
            SqlValue::Int8(-42),
            SqlValue::Int4(7),
            SqlValue::Uuid([7; 16]),
            SqlValue::Numeric("12.50".into()),
        ] {
            let claims = Claims {
                method: AuthenticationMethod::BrowserSession,
                row: Some([1; 16]),
                verifier: "ab".repeat(32),
                principal: [2; 16],
                subject,
                expires: 1_792_504_800_000,
                csrf: Some("cd".repeat(32)),
            };
            let json = claims.to_json().unwrap();
            assert_eq!(Claims::from_json(&json), Some(claims));
        }
    }

    /// A part records the encoding it was written in (area 19, chunk 5.9);
    /// one written before encodings were recorded, as chunk 5.1b wrote it,
    /// still reads, and one of an encoding this release doesn't know
    /// doesn't, so the request reads the record.
    #[test]
    fn claims_record_their_encoding_and_older_parts_still_read() {
        let claims = Claims {
            method: AuthenticationMethod::BrowserSession,
            row: Some([1; 16]),
            verifier: "ab".repeat(32),
            principal: [2; 16],
            subject: SqlValue::Text("ann".into()),
            expires: 1_792_504_800_000,
            csrf: Some("cd".repeat(32)),
        };
        let json = claims.to_json().unwrap();
        let written: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(written["enc"], 1, "{json}");
        // As chunk 5.1b wrote it, with no `enc`.
        let before = format!(
            r#"{{"c":"{csrf}","e":1792504800000,"m":"browser_session","p":"{principal}","r":"{row}","s":"tann","v":"{verifier}"}}"#,
            csrf = "cd".repeat(32),
            principal = "02".repeat(16),
            row = "01".repeat(16),
            verifier = "ab".repeat(32),
        );
        assert_eq!(Claims::from_json(&before), Some(claims.clone()));
        let later = json.replace("\"enc\":1", "\"enc\":2");
        assert_ne!(later, json);
        assert_eq!(Claims::from_json(&later), None);
        let unreadable = json.replace("\"enc\":1", "\"enc\":\"1\"");
        assert_eq!(Claims::from_json(&unreadable), None);
    }

    #[test]
    fn an_origin_is_its_scheme_and_authority() {
        assert_eq!(
            origin_of("https://App.Example/").as_deref(),
            Some("https://app.example")
        );
        assert_eq!(
            origin_of("https://app.example:443/pages").as_deref(),
            Some("https://app.example")
        );
        assert_eq!(
            origin_of("http://localhost:8080").as_deref(),
            Some("http://localhost:8080")
        );
        assert_eq!(origin_of("app.example"), None);
    }
}
