//! `crypto.verify_jwt` (area 19, "Hashing and cryptography", chunks 5.5r
//! and 5.5r4): a JWS in compact form (RFC 7515) carrying a JWT (RFC 7519),
//! checked against a set of public keys, each with its own algorithm.
//!
//! Every check follows the attacks the standards' security considerations
//! name (RFC 8725, "JSON Web Token Best Current Practices"):
//!
//! - **The algorithm is the key's, never the token's.** A `Jwk` holds an
//!   Ed25519 key (RFC 8037), checked with `EdDSA`, an RSA key, checked with
//!   `RS256` (RSASSA-PKCS1-v1_5 with SHA-256, RFC 7518 §3.3), or a P-256
//!   key, checked with `ES256` (RFC 7518 §3.4). A key is tried only for a
//!   token whose header names its algorithm, so `none`, `HS256` (the
//!   confusion of a public key for a shared secret) and a key of one kind
//!   named for a token of another all fail `invalid_signature`.
//! - **Keys come only from the call.** A header's `jwk`, `jku`, `x5u` and
//!   `x5c` are never read; its `kid` only narrows which of the given keys
//!   are tried. A header with `crit` names an extension nothing here
//!   understands, so it fails `invalid_signature` (RFC 7515 §4.1.11). A
//!   key that carries certificates (`x5c`) is tried only when the first
//!   holds the same key (RFC 7517 §4.7).
//! - **The signature first.** Nothing the payload says is read until a key
//!   has verified it: strictly for Ed25519 (RFC 8032's checks, as
//!   `verify_signature`), by encoding and comparing the whole padded digest
//!   for RS256, which also needs a signature exactly as long as the
//!   modulus, and for ES256 with R and S each below the group's order.
//! - **Times against the clock** with [`JWT_LEEWAY_SECONDS`]' leeway: `exp`
//!   is required and must be later than now, `nbf` and `iat` no later.
//! - **Issuer and audience** compared exactly, as RFC 7519 §4.1 compares a
//!   StringOrURI; a token with an audience is refused unless the call names
//!   one of them (RFC 7519 §4.1.3).
//! - **One reading of the JSON.** The header and the claims are each one
//!   JSON object with no member named twice, so no other reader can take
//!   a different value from the same token (RFC 7515 §5.2, RFC 7519 §7.2).
//!   A numeric date is a whole number of seconds.
//! - **Claims of the application's own type** (`as:`, chunk 5.5r4) are
//!   read last, once everything else holds: the claims its type names,
//!   through `json.read`'s decoding, the others ignored, as RFC 7519 §4
//!   requires of claims not understood; a registered one it names is the
//!   checked value, as `JwtClaims` holds it.

use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;

use jadpo_std::RuleKey;
use jadpo_std::http::{Codec, CodecId};
use jadpo_std::standard::jwk::{self, EC, ED25519, OKP, P256, RSA};
use jadpo_std::standard::security::{
    JWT_CLAIMS, JWT_CLAIMS_FIELDS, JWT_LEEWAY_SECONDS, decode_base64url, is_jwt,
};
use p256::ecdsa::signature::hazmat::PrehashVerifier;
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::Value as Json;

use crate::certificate::SubjectKey;
use crate::json::{Codecs, Path, Problem};
use crate::{Fault, Value};

/// Reads one provider document while refusing duplicate members at every
/// depth, as JWT header and claim documents are read. The caller bounds
/// its byte length before parsing; serde also bounds nesting depth.
pub fn strict_document(bytes: &[u8]) -> Option<Json> {
    serde_json::from_slice::<Strict>(bytes)
        .ok()
        .map(|Strict(value)| value)
}

/// A provider's public JWK, through the standard type's ordinary field
/// and cross-field rules. Unknown public metadata is ignored; private
/// material and unsupported key operations are always refused.
pub fn read_jwk(json: &Json) -> Option<Value> {
    let members = json.as_object()?;
    if members
        .keys()
        .any(|name| jwk::REFUSED_MEMBERS.contains(&name.as_str()))
    {
        return None;
    }
    let parts = jadpo_std::standard::security::JWK_FIELDS
        .iter()
        .map(|(name, _)| {
            let wire = if *name == "use_" { "use" } else { name };
            match members.get(wire) {
                None | Some(Json::Null) => Some(Value::None),
                Some(Json::String(text)) if *name != "x5c" => Some(Value::text(text)),
                Some(Json::Array(items)) if *name == "x5c" => {
                    let items = items
                        .iter()
                        .map(|item| item.as_str().map(Value::text))
                        .collect::<Option<Vec<_>>>()?;
                    Some(Value::list(items))
                }
                _ => None,
            }
        })
        .collect::<Option<Vec<_>>>()?;
    for ((_, part), value) in jadpo_std::standard::security::JWK_FIELDS.iter().zip(&parts) {
        if !jwk_part(*part, value) {
            return None;
        }
    }
    let value =
        crate::standard::from_parts(&jadpo_std::standard::security::JWK_ENTRY, &parts).ok()?;
    if crate::across::broken_standard(&value).is_some() || key_of(&value).is_none() {
        return None;
    }
    Some(value)
}

fn jwk_part(part: jadpo_std::standard::Part, value: &Value) -> bool {
    use jadpo_std::standard::Part;
    match (part, value) {
        (Part::Optional(_), Value::None) => true,
        (Part::Optional(inner), value) => jwk_part(*inner, value),
        (Part::List(inner), Value::List(items)) => items.iter().all(|item| jwk_part(*inner, item)),
        (Part::Matching(_), Value::Text(_)) => {
            crate::validate::Validator::new(part.builtin(), vec![part.rules()])
                .check(value)
                .is_ok()
        }
        _ => false,
    }
}

/// Why a token is refused, as the call's `attempt` maps it.
fn refused(key: RuleKey) -> Fault {
    Fault::Broke(key)
}

/// Why `verify_jwt(..., as:)` gives no claims: a condition of the token,
/// or claims that aren't the type `as:` names, with the first problem
/// found, as `json.read`'s `mismatch` names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    Token(Fault),
    Mismatch(Problem),
}

impl From<Fault> for Refusal {
    fn from(fault: Fault) -> Refusal {
        Refusal::Token(fault)
    }
}

/// JSON read whole, refusing an object that names a member twice at any
/// depth.
struct Strict(Json);

impl<'de> Deserialize<'de> for Strict {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Strict, D::Error> {
        deserializer.deserialize_any(StrictVisitor)
    }
}

struct StrictVisitor;

impl<'de> Visitor<'de> for StrictVisitor {
    type Value = Strict;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JSON")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Strict, E> {
        Ok(Strict(Json::Bool(value)))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Strict, E> {
        Ok(Strict(Json::from(value)))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Strict, E> {
        Ok(Strict(Json::from(value)))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Strict, E> {
        Ok(Strict(
            serde_json::Number::from_f64(value).map_or(Json::Null, Json::Number),
        ))
    }

    fn visit_str<E>(self, value: &str) -> Result<Strict, E> {
        Ok(Strict(Json::String(value.to_string())))
    }

    fn visit_string<E>(self, value: String) -> Result<Strict, E> {
        Ok(Strict(Json::String(value)))
    }

    fn visit_unit<E>(self) -> Result<Strict, E> {
        Ok(Strict(Json::Null))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Strict, A::Error> {
        let mut items = Vec::new();
        while let Some(Strict(item)) = seq.next_element()? {
            items.push(item);
        }
        Ok(Strict(Json::Array(items)))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Strict, A::Error> {
        let mut members = serde_json::Map::new();
        while let Some(name) = map.next_key::<String>()? {
            if members.contains_key(&name) {
                return Err(de::Error::custom("a member named twice"));
            }
            let Strict(value) = map.next_value()?;
            members.insert(name, value);
        }
        Ok(Strict(Json::Object(members)))
    }
}

/// A part of the token as one JSON object, or `malformed`.
fn object(part: &str) -> Result<serde_json::Map<String, Json>, Fault> {
    let bytes = decode_base64url(part).ok_or(refused(RuleKey::Malformed))?;
    match serde_json::from_slice::<Strict>(&bytes) {
        Ok(Strict(Json::Object(members))) => Ok(members),
        _ => Err(refused(RuleKey::Malformed)),
    }
}

/// The public half of one of the keys a call gives, ready to check with.
enum Public<'a> {
    /// An Ed25519 key's `x`, checked by `crypto`'s strict verifier.
    Ed25519(&'a str),
    Rsa(rsa::RsaPublicKey),
    P256(p256::ecdsa::VerifyingKey),
}

/// One of the keys a call gives: its id, the one algorithm it is used
/// with, and its public half.
struct Key<'a> {
    kid: Option<&'a str>,
    algorithm: &'static str,
    public: Public<'a>,
}

/// A `Jwk` value's key, or `None` for one nothing can check with (which
/// its type's rules already ensure), one for encryption, one whose `alg`
/// isn't its `kty`'s, or one whose first certificate holds another key.
fn key_of(value: &Value) -> Option<Key<'_>> {
    let Value::Record(record) = value else {
        return None;
    };
    let text = |name: &str| match record.get(name) {
        Some(Value::Text(text)) => Some(&**text),
        _ => None,
    };
    let bytes = |name: &str| text(name).and_then(decode_base64url);
    let kty = text("kty")?;
    let algorithm = jwk::algorithm_of(kty)?;
    if text("use_").is_some_and(|usage| usage != "sig")
        || text("alg").is_some_and(|alg| alg != algorithm)
    {
        return None;
    }
    let (public, held) = match kty {
        OKP if text("crv")? == ED25519 => (
            Public::Ed25519(text("x")?),
            SubjectKey::Ed25519(bytes("x")?),
        ),
        RSA => {
            let (n, e) = (bytes("n")?, bytes("e")?);
            if !jwk::is_rsa_modulus(&n) || !jwk::is_rsa_exponent(&e) {
                return None;
            }
            let key = crate::public_key::rsa_key(&n, &e)?;
            (Public::Rsa(key), SubjectKey::Rsa { n, e })
        }
        EC if text("crv")? == P256 => {
            let (x, y) = (bytes("x")?, bytes("y")?);
            if x.len() != 32 || y.len() != 32 {
                return None;
            }
            let mut point = Vec::with_capacity(65);
            point.push(4);
            point.extend_from_slice(&x);
            point.extend_from_slice(&y);
            // A point off the curve, or at infinity, is refused here.
            let key = p256::ecdsa::VerifyingKey::from_sec1_bytes(&point).ok()?;
            (Public::P256(key), SubjectKey::P256 { x, y })
        }
        _ => return None,
    };
    // A key that sends certificates is tried only when the first holds the
    // same key (RFC 7517 §4.7); nothing checks the chain.
    if let Some(Value::List(certificates)) = record.get("x5c")
        && let Some(first) = certificates.first()
    {
        let Value::Text(first) = first else {
            return None;
        };
        let der = crate::encoding::from_base64(first)?;
        if crate::certificate::subject_key(&der)? != held {
            return None;
        }
    }
    Some(Key {
        kid: text("kid"),
        algorithm,
        public,
    })
}

/// Whether `signature` is `key`'s signature of `signed`, with the key's
/// own algorithm.
fn verifies(key: &Key<'_>, signed: &[u8], signature: &[u8]) -> bool {
    match &key.public {
        Public::Ed25519(x) => crate::crypto::verify_signature(signed, signature, x),
        Public::Rsa(public) => crate::public_key::rs256_with(public, signed, signature),
        // R and S, each 32 octets (RFC 7518 §3.4), each from 1 to below
        // the group's order. A high S is accepted, as RFC 7518 and `p256`
        // accept it (chosen in chunk 5.5r4).
        Public::P256(public) => {
            p256::ecdsa::Signature::from_slice(signature).is_ok_and(|signature| {
                public
                    .verify_prehash(&crate::hash::sha256(signed), &signature)
                    .is_ok()
            })
        }
    }
}

/// A numeric date (RFC 7519 §2) as an instant's milliseconds: a whole
/// number of seconds an `Instant` holds; `malformed` for anything else.
fn numeric_date(value: &Json) -> Result<i64, Fault> {
    value
        .as_i64()
        .and_then(|seconds| seconds.checked_mul(1000))
        .filter(|millis| (jadpo_std::INSTANT_MIN..=jadpo_std::INSTANT_MAX).contains(millis))
        .ok_or(refused(RuleKey::Malformed))
}

/// A StringOrURI claim, as `JwtClaims` holds it: one line of text.
fn claim_text(value: &Json) -> Result<Rc<str>, Fault> {
    match value {
        Json::String(text) if claim_pattern(text) => Ok(Rc::from(text.as_str())),
        _ => Err(refused(RuleKey::Malformed)),
    }
}

/// Whether `text` is what a `JwtClaims`' text claims hold
/// (`security::CLAIM_PATTERN`): 1 to 1,024 characters, none a control
/// character (general category Cc).
fn claim_pattern(text: &str) -> bool {
    let count = text.chars().count();
    (1..=1024).contains(&count) && !text.chars().any(char::is_control)
}

/// `crypto.verify_jwt(token, keys:, issuer:, audience:)` at `now` (an
/// instant's milliseconds): the token's registered claims as a
/// `JwtClaims`, or the condition it fails, for the call's `attempt`.
pub fn verify_jwt(
    token: &str,
    keys: &[Value],
    issuer: Option<&str>,
    audience: Option<&str>,
    now: i64,
) -> Result<Value, Fault> {
    verified(token, keys, issuer, audience, now).map(|(claims, _)| claims)
}

/// `crypto.verify_jwt(token, keys:, issuer:, audience:, as:)` at `now`
/// (chunk 5.5r4): once the token holds as [`verify_jwt`] checks it, its
/// claims as the application's object type whose codec is `codec`, those
/// its type doesn't name ignored (RFC 7519 §4) and a registered one it
/// names given as `JwtClaims` holds it, read through `json.read`'s
/// decoding, so a claim missing, of the wrong kind or breaking a rule is
/// `mismatch` with its problem.
pub fn verify_jwt_as(
    token: &str,
    keys: &[Value],
    issuer: Option<&str>,
    audience: Option<&str>,
    now: i64,
    codecs: &Codecs<'_>,
    codec: CodecId,
) -> Result<Value, Refusal> {
    let (registered, members) = verified(token, keys, issuer, audience, now)?;
    let named = claim_names(codecs, codec);
    let mut custom: serde_json::Map<String, Json> = members
        .into_iter()
        .filter(|(name, _)| {
            !JWT_CLAIMS_FIELDS.iter().any(|(claim, _)| claim == name)
                && named.iter().any(|field| field == name)
        })
        .collect();
    // A registered claim the type names is the checked one, as `JwtClaims`
    // holds it (decided by the lead session for chunk 5.5r4); one the token
    // hasn't is left out, so a field that must have it is `required`.
    if let Value::Record(record) = &registered {
        for (claim, _) in JWT_CLAIMS_FIELDS {
            if !named.iter().any(|field| field == claim) {
                continue;
            }
            if let Some(json) = record.get(claim).and_then(held_json) {
                custom.insert(claim.to_string(), json);
            }
        }
    }
    let mut problems = Vec::new();
    match codecs.read(
        codec,
        &Json::Object(custom),
        &Path::default(),
        &mut problems,
    ) {
        Some(value) => Ok(value),
        None => Err(Refusal::Mismatch(problems.into_iter().next().unwrap_or(
            Problem {
                field: String::new(),
                rule: "type".to_string(),
            },
        ))),
    }
}

/// A registered claim as `JwtClaims` holds it, written as JSON reads it
/// back: text, a list of text, or an instant as its canonical text;
/// `None` for a claim the token hasn't.
fn held_json(value: &Value) -> Option<Json> {
    match value {
        Value::Text(text) => Some(Json::String(text.to_string())),
        Value::Instant(millis) => Some(Json::String(jadpo_std::format_instant(*millis))),
        Value::List(items) => Some(Json::Array(items.iter().filter_map(held_json).collect())),
        _ => None,
    }
}

/// The claims an object type's codec names, as they are sent.
fn claim_names(codecs: &Codecs<'_>, codec: CodecId) -> Vec<String> {
    let Some(Codec::Object { shape, .. }) = codecs.codecs.get(codec.0 as usize) else {
        return Vec::new();
    };
    let Some(shape) = codecs.shapes.get(shape.0 as usize) else {
        return Vec::new();
    };
    (0..shape.len())
        .filter_map(|slot| shape.field(slot))
        .map(|(name, _)| jadpo_std::standard::wire_name(shape.ty(), name).to_string())
        .collect()
}

/// A compact token's untrusted structure, shared by verification and
/// provider preflight. No claim becomes authority at this boundary.
struct Compact<'a> {
    header: &'a str,
    payload: &'a str,
    protected: serde_json::Map<String, Json>,
    signature: Vec<u8>,
}

fn compact(token: &str) -> Result<Compact<'_>, Fault> {
    if !is_jwt(token) {
        return Err(refused(RuleKey::Malformed));
    }
    let mut parts = token.split('.');
    let (Some(header), Some(payload), Some(signature)) = (parts.next(), parts.next(), parts.next())
    else {
        return Err(refused(RuleKey::Malformed));
    };
    let protected = object(header)?;
    let Some(Json::String(_algorithm)) = protected.get("alg") else {
        return Err(refused(RuleKey::Malformed));
    };
    match protected.get("kid") {
        None | Some(Json::String(_)) => {}
        Some(_) => return Err(refused(RuleKey::Malformed)),
    }
    // A nested token's claims are another token, which this doesn't read.
    if protected
        .get("cty")
        .and_then(Json::as_str)
        .is_some_and(|content| content.eq_ignore_ascii_case("JWT"))
    {
        return Err(refused(RuleKey::Malformed));
    }
    // No extension this can't check.
    if protected.contains_key("crit") {
        return Err(refused(RuleKey::InvalidSignature));
    }
    let signature = decode_base64url(signature).ok_or(refused(RuleKey::Malformed))?;
    Ok(Compact {
        header,
        payload,
        protected,
        signature,
    })
}

/// Rejects independently invalid token structure before provider I/O.
/// It reads strict JSON structure only, never trusts a claim or a token
/// URL, and does not replace signature, issuer, audience or time checks.
pub fn preflight_jwt(token: &str) -> Result<(), Fault> {
    let token = compact(token)?;
    if !matches!(
        token.protected.get("alg").and_then(Json::as_str),
        Some("EdDSA" | "RS256" | "ES256")
    ) {
        return Err(refused(RuleKey::InvalidSignature));
    }
    object(token.payload)?;
    Ok(())
}

/// The token's registered claims as a `JwtClaims`, and every claim it
/// makes, once its signature, times, issuer and audience hold.
fn verified(
    token: &str,
    keys: &[Value],
    issuer: Option<&str>,
    audience: Option<&str>,
    now: i64,
) -> Result<(Value, serde_json::Map<String, Json>), Fault> {
    let compact = compact(token)?;
    let header = compact.header;
    let payload = compact.payload;
    let protected = &compact.protected;
    let algorithm = protected
        .get("alg")
        .and_then(Json::as_str)
        .ok_or(refused(RuleKey::Malformed))?;
    let kid = protected.get("kid").and_then(Json::as_str);
    let signed = format!("{header}.{payload}");
    let signature = compact.signature;
    // Only a key used with the algorithm the header names is tried, with
    // that key's own algorithm: `none` and every algorithm no key is used
    // with find none.
    let signed_by_a_key = keys
        .iter()
        .filter_map(key_of)
        .filter(|key| kid.is_none_or(|kid| key.kid == Some(kid)))
        .filter(|key| key.algorithm == algorithm)
        .any(|key| verifies(&key, signed.as_bytes(), &signature));
    if !signed_by_a_key {
        return Err(refused(RuleKey::InvalidSignature));
    }
    let members = object(payload)?;
    let claims = claims(&members, issuer, audience, now)?;
    Ok((claims, members))
}

/// The claims of a token whose signature held, checked against the clock,
/// the issuer and the audience, as a `JwtClaims`.
fn claims(
    members: &serde_json::Map<String, Json>,
    issuer: Option<&str>,
    audience: Option<&str>,
    now: i64,
) -> Result<Value, Fault> {
    let text = |name: &str| members.get(name).map(claim_text).transpose();
    let date = |name: &str| members.get(name).map(numeric_date).transpose();
    let iss = text("iss")?;
    let sub = text("sub")?;
    let jti = text("jti")?;
    let aud: Vec<Rc<str>> = match members.get("aud") {
        None => Vec::new(),
        Some(Json::Array(items)) => items.iter().map(claim_text).collect::<Result<_, _>>()?,
        Some(one) => vec![claim_text(one)?],
    };
    // A token that never expires is refused.
    let exp = date("exp")?.ok_or(refused(RuleKey::Malformed))?;
    let nbf = date("nbf")?;
    let iat = date("iat")?;
    let leeway = JWT_LEEWAY_SECONDS * 1000;
    if now >= exp.saturating_add(leeway) {
        return Err(refused(RuleKey::Expired));
    }
    let latest = now.saturating_add(leeway);
    if nbf.is_some_and(|nbf| nbf > latest) || iat.is_some_and(|iat| iat > latest) {
        return Err(refused(RuleKey::NotYetValid));
    }
    if let Some(issuer) = issuer
        && iss.as_deref() != Some(issuer)
    {
        return Err(refused(RuleKey::WrongIssuer));
    }
    let for_us = match audience {
        Some(audience) => aud.iter().any(|named| &**named == audience),
        None => aud.is_empty(),
    };
    if !for_us {
        return Err(refused(RuleKey::WrongAudience));
    }
    let optional_text = |text: Option<Rc<str>>| text.map_or(Value::None, Value::Text);
    let optional_date = |at: Option<i64>| at.map_or(Value::None, Value::Instant);
    let fields: BTreeMap<&str, Value> = BTreeMap::from([
        ("iss", optional_text(iss)),
        ("sub", optional_text(sub)),
        (
            "aud",
            Value::list(aud.into_iter().map(Value::Text).collect()),
        ),
        ("exp", Value::Instant(exp)),
        ("nbf", optional_date(nbf)),
        ("iat", optional_date(iat)),
        ("jti", optional_text(jti)),
    ]);
    let ordered = JWT_CLAIMS_FIELDS
        .iter()
        .map(|(name, _)| {
            let value = fields.get(name).cloned().unwrap_or(Value::None);
            (Rc::<str>::from(*name), value)
        })
        .collect();
    Ok(Value::record(JWT_CLAIMS, ordered))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tokens made with RFC 8032's first test key (RFC 8037 §A.1's), whose
    // public key is `KEY`, by its id `2026-10`, unless they say otherwise;
    // `OTHER` is RFC 8032's second test key. The clock reads `NOW`,
    // 2026-10-18T09:00:00Z, and each token's times are around it.
    const KEY: &str = "11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo";
    const OTHER: &str = "PUAXw-hDiVqStwqnTRt-vJyYLM8uxJaMwM1V8Sr0Zgw";
    const NOW: i64 = 1_792_314_000_000;
    const GOOD: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIn0.nOqN7320O3T8e_AyF7XNt32jmgjE8VCU7GVHL89ZUeiRcU3gGM3EgiNxVp3_y422De1PZWd2tZ0LuSqdfm6QCg";
    const EXPIRED: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzEzOTY5LCJqdGkiOiJhMWIyYzMifQ.0XDMfIlEkopB8bJAJCe95i2zQMGu9mbv_kUtc1QC0xMm5wxmsS9munCCG36_4NkaNcfDKNgaGpswq5QNuh_KDw";
    const WITHIN_LEEWAY: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzEzOTcxLCJqdGkiOiJhMWIyYzMifQ.6KemiuXAOr5hQgxNjEVvm6KOeb7WfwdpUjY3SXZm99xDa-1SlxCKwqYOBOrnAhpXbasM2YhlMCo79X1yBMbeBw";
    const NOT_YET: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzIxMjAwLCJuYmYiOjE3OTIzMTc2MDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIn0.NEOn2CiCjIrOo3-lfR85g4KGRcmsDvmDFxLoSa_pCxov4sxnnaq4mVgiwKpIEglhlUzNAN5f9x1_8_4pv9JuBA";
    const ISSUED_LATER: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJpYXQiOjE3OTIzMTc2MDAsImp0aSI6ImExYjJjMyJ9.q2nPL0HmqqKJq7jfJGhRB6qzZPl_MyYPpK_uTy7my1vpF683pEUQmA3lRxhtBg1HZu4THA4pJ_T3kUi-BMXZBA";
    const OTHER_AUDIENCE: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoicGhhcm1hY3kiLCJleHAiOjE3OTIzMTc2MDAsIm5iZiI6MTc5MjMxMzk0MCwiaWF0IjoxNzkyMzEzOTQwLCJqdGkiOiJhMWIyYzMifQ.ysn61bX4ZDaJ7OCiuPNtYwJ1POMD7b1tVLIfZsDS1oimtwY3XgyhZDpLSkDph6j3fol8nv4KPZQ0g7Ai9pd4Bw";
    const AUDIENCES: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjpbInBoYXJtYWN5IiwiY2xpbmljIl0sImV4cCI6MTc5MjMxNzYwMCwibmJmIjoxNzkyMzEzOTQwLCJpYXQiOjE3OTIzMTM5NDAsImp0aSI6ImExYjJjMyJ9.d4qSrObZRPioEK0dNsuX42415FXLFQt7J4UpZD3pCmi-iATntnaFAKj_rrQg_lFmamYDo5dwT_iBPOsRMrmTDQ";
    const NO_AUDIENCE: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIn0.HAaY5dQsAgNBv0-VMMdJlNfjsTWLzRTMQEtnzMUuZiRP9WI6K2ZazVFtbhXp7O3tG6k7MYELoH1Ybkqn7uZADQ";
    const NO_EXPIRY: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwibmJmIjoxNzkyMzEzOTQwLCJpYXQiOjE3OTIzMTM5NDAsImp0aSI6ImExYjJjMyJ9.U70xc8AeLGIBUiBSymvHQ4KTyfwxGFF2KRVIUwJ7NYmWD1vJq7RSJxksR3_Qgy-c3ylShb1NPYtDj0eiqiPjDw";
    const FRACTIONAL_EXPIRY: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJzdWIiOiJtZW1iZXItNDIiLCJhdWQiOiJjbGluaWMiLCJleHAiOjE3OTIzMTc2MDAuNX0.VOp-9yMFcYGCQv2_2oNxGvFU4WvioLObRhkkrEMDPU3QXcmaIk87aXoOmNRIiJXeuktI9BbPmykFFaMs8f06DQ";
    const DUPLICATE_SUBJECT: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJzdWIiOiJtZW1iZXItNDIiLCJzdWIiOiJtZW1iZXItMSIsImF1ZCI6ImNsaW5pYyIsImV4cCI6MTc5MjMxNzYwMH0.b0Q4gmWRQmy_p1wAXaMBVBG8RiYF0pSN1pgrHN32M9px5stq6U2EisyxiXmbSG8rsiVt-kcbl1DxgHEQ2jPgCA";
    const NOT_AN_OBJECT: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.WyJtZW1iZXItNDIiXQ.PmC15ewNEx2t5HtN6xT806liScBLFGYNRDtybWej7lHhL4F3So-ZMIWLz_mFg0t3XqgP1CRlEqtTwIwhp2F9Bg";
    const OTHER_ISSUER: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2V2aWwuZXhhbXBsZS5jb20iLCJzdWIiOiJtZW1iZXItNDIiLCJhdWQiOiJjbGluaWMiLCJleHAiOjE3OTIzMTc2MDAsIm5iZiI6MTc5MjMxMzk0MCwiaWF0IjoxNzkyMzEzOTQwLCJqdGkiOiJhMWIyYzMifQ.Gmk9tIA-Ivmifdrhf0jKtGXPD87trYF59gq4DB0b91vwD1zAKRzpLhnsxOFYmK9UF8py09jUVBq81sKlhcGZAA";
    const NO_ISSUER: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJzdWIiOiJtZW1iZXItNDIiLCJhdWQiOiJjbGluaWMiLCJleHAiOjE3OTIzMTc2MDAsIm5iZiI6MTc5MjMxMzk0MCwiaWF0IjoxNzkyMzEzOTQwLCJqdGkiOiJhMWIyYzMifQ.RmGGM6eALOKM6WdAXpC6_t69k9xkuIZYQsjV4iEY37W6jk2arLbHhZhRfWrGfITsxgb9j_hNhK0voYgGqZo7Bw";
    const ALG_NONE: &str = "eyJhbGciOiJub25lIiwia2lkIjoiMjAyNi0xMCJ9.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIn0.bUBJ-HYfcv-x-XVebqotpXUryxrUsu15W2sejqXFebmY2OB3-XXAQIWG71XLjOBQDrJnDyfN4RTsPW20OqqBDA";
    const ALG_NONE_UPPER: &str = "eyJhbGciOiJOT05FIn0.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIn0.vv8oLp5Rsxgw8GjogyfpI__2_FakfozAm3k-WbGC8EhvgoPZzCjFrveCySXrn8YVc9NoYZu21IVBXMnmhubYDA";
    const HS256_PUBLIC_TEXT: &str = "eyJhbGciOiJIUzI1NiIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIn0.LEqGD4Thr8CB21DsacQjPDe-b_CRUkja0n3gjc6sT7M";
    const HS256_PUBLIC_BYTES: &str = "eyJhbGciOiJIUzI1NiIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIn0._A-I0KfSvML_c2U1P1gFB6-NQve1SFTCqc-bAc5-OKg";
    const TAMPERED: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTEiLCJhdWQiOiJjbGluaWMiLCJleHAiOjE3OTIzMTc2MDAsIm5iZiI6MTc5MjMxMzk0MCwiaWF0IjoxNzkyMzEzOTQwLCJqdGkiOiJhMWIyYzMifQ.nOqN7320O3T8e_AyF7XNt32jmgjE8VCU7GVHL89ZUeiRcU3gGM3EgiNxVp3_y422De1PZWd2tZ0LuSqdfm6QCg";
    const OTHER_KEY_SAME_KID: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIn0.BNLMxgdo1dxi32SMHI1U9QMTXce-KJPd003uD4XCWRG21wgz3aqJl3zI6X9mA2c_ond_hOcUc_3lUKoyxhvTAw";
    const OTHER_KEY_NO_KID: &str = "eyJhbGciOiJFZERTQSIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIn0.MoObSJTByl9ztamQPITPsfIdIBPpXnxaSkuM-ub4C5T56wNbZXIh_ERcqBdK1ovl76Ei2M_FsxsHi0zPmqp3AQ";
    const EMBEDDED_KEY: &str = "eyJhbGciOiJFZERTQSIsImp3ayI6eyJrdHkiOiJPS1AiLCJjcnYiOiJFZDI1NTE5IiwieCI6IlBVQVh3LWhEaVZxU3R3cW5UUnQtdkp5WUxNOHV4SmFNd00xVjhTcjBaZ3cifX0.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIn0.Ccw9Kvg8GAZtTfr10y_2wLThxpCwQpbXtPgl9IRIGXmn0-XTP1QSPqHQ0YWRPpXRuhhCoc_OT39E7HNDck1fCw";
    const CRITICAL: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJjcml0IjpbImV4cCJdLCJleHAiOjE3OTIzMTc2MDB9.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIn0.unE4y_YMImD0jGvd-V4GZ2EuqySXEUsS5YXHcfVmmW9xs-dsEJafT-MvQu76Um1KEKKH97NpIoZls7lIVsWKDQ";
    const NESTED: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJjdHkiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIn0.cTRR_HTuZCZpy-VLcdDR9zpT8PKI2TB37RaEP4KLRcg-C4QrWND_MBmfLKnnv7A4v0kpd-GcEfskGC5pQHXBAg";
    const KID_NOT_TEXT: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6N30.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIn0.dUYTo4YnuUAXV9fIkEP0oRLBr_kE5WlSndWKn6u-DgvsXonlWI9Du8cTouHZbAh_AXMa4hA01wmDVE0rm_PrBQ";
    const CONTROL_IN_SUBJECT: &str = "eyJhbGciOiJFZERTQSIsImtpZCI6IjIwMjYtMTAiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyXG40MiIsImF1ZCI6ImNsaW5pYyIsImV4cCI6MTc5MjMxNzYwMCwibmJmIjoxNzkyMzEzOTQwLCJpYXQiOjE3OTIzMTM5NDAsImp0aSI6ImExYjJjMyJ9.WHNq-upLFM8gvkAamTwj4CdNfOhBQgQtWRrv5KFrXCqf5VvzitSikUryPb7j_K03MRH_SA3vit4BXrVj66RSAA";
    /// RFC 7519 §3.1's example, HMAC SHA-256 under RFC 7515 §A.1's key.
    const RFC_7519: &str = "eyJ0eXAiOiJKV1QiLA0KICJhbGciOiJIUzI1NiJ9.eyJpc3MiOiJqb2UiLA0KICJleHAiOjEzMDA4MTkzODAsDQogImh0dHA6Ly9leGFtcGxlLmNvbS9pc19yb290Ijp0cnVlfQ.dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    /// RFC 8037 §A.4's Ed25519 signature of a payload that isn't claims.
    const RFC_8037: &str = "eyJhbGciOiJFZERTQSJ9.RXhhbXBsZSBvZiBFZDI1NTE5IHNpZ25pbmc.hgyY0il_MGCjP0JzlnLWG1PPOt7-09PGcvMg3AIbQR6dWbhijcNR4ki4iylGjg5BhVsPt9g7sVvpAr_MuM0KAg";

    /// A `Jwk` value with these parts, each other part `none`.
    pub(crate) fn key(parts: &[(&str, Value)]) -> Value {
        Value::record(
            "Jwk",
            jadpo_std::standard::security::JWK_FIELDS
                .iter()
                .map(|(name, _)| {
                    let value = parts
                        .iter()
                        .find(|(given, _)| given == name)
                        .map_or(Value::None, |(_, value)| value.clone());
                    (Rc::from(*name), value)
                })
                .collect(),
        )
    }

    fn jwk(x: &str, kid: Option<&str>) -> Value {
        key(&[
            ("kty", Value::text("OKP")),
            ("crv", Value::text("Ed25519")),
            ("x", Value::text(x)),
            ("kid", kid.map_or(Value::None, Value::text)),
        ])
    }

    fn keys() -> Vec<Value> {
        vec![jwk(KEY, Some("2026-10"))]
    }

    fn checked(token: &str) -> Result<Value, Fault> {
        verify_jwt(
            token,
            &keys(),
            Some("https://id.example.com"),
            Some("clinic"),
            NOW,
        )
    }

    fn fails(token: &str, key: RuleKey) {
        assert_eq!(checked(token), Err(Fault::Broke(key)), "{token}");
    }

    fn field<'a>(claims: &'a Value, name: &str) -> &'a Value {
        let Value::Record(record) = claims else {
            panic!("claims: {claims:?}");
        };
        record.get(name).expect("a claim")
    }

    #[test]
    fn provider_documents_and_public_keys_use_the_checked_readers() {
        assert!(strict_document(br#"{"keys":[],"keys":[]}"#).is_none());
        assert!(strict_document(br#"{"keys":[{"kid":"a","kid":"b"}]}"#).is_none());
        let mut json = serde_json::json!({"kty":"OKP", "crv":"Ed25519", "x": KEY, "kid":"current"});
        assert!(read_jwk(&json).is_some());
        json["d"] = Json::String("private".into());
        assert!(read_jwk(&json).is_none());
        json.as_object_mut().expect("object").remove("d");
        json["alg"] = Json::String("HS256".into());
        assert!(read_jwk(&json).is_none());
    }

    #[test]
    fn rfc_8037_s_signature_verifies_and_its_payload_isn_t_claims() {
        // The signature holds under RFC 8037 §A.2's key, so the payload is
        // read, and it isn't a JSON object.
        assert_eq!(
            verify_jwt(RFC_8037, &[jwk(KEY, None)], None, None, NOW),
            Err(Fault::Broke(RuleKey::Malformed))
        );
        let (signed, signature) = RFC_8037.rsplit_once('.').expect("three parts");
        let signature = decode_base64url(signature).expect("a signature");
        assert!(crate::crypto::verify_signature(
            signed.as_bytes(),
            &signature,
            KEY
        ));
    }

    #[test]
    fn a_genuine_token_gives_its_claims() {
        let claims = checked(GOOD).expect("genuine");
        assert_eq!(
            field(&claims, "iss"),
            &Value::text("https://id.example.com")
        );
        assert_eq!(field(&claims, "sub"), &Value::text("member-42"));
        assert_eq!(
            field(&claims, "aud"),
            &Value::list(vec![Value::text("clinic")])
        );
        assert_eq!(field(&claims, "exp"), &Value::Instant(NOW + 3_600_000));
        assert_eq!(field(&claims, "nbf"), &Value::Instant(NOW - 60_000));
        assert_eq!(field(&claims, "iat"), &Value::Instant(NOW - 60_000));
        assert_eq!(field(&claims, "jti"), &Value::text("a1b2c3"));
        // One of several audiences, and no issuer asked for.
        assert!(checked(AUDIENCES).is_ok());
        assert!(verify_jwt(GOOD, &keys(), None, Some("clinic"), NOW).is_ok());
        // The key found by its id among others, and a key with no id
        // tried when the token names none.
        let both = vec![jwk(OTHER, None), jwk(KEY, Some("2026-10"))];
        assert!(verify_jwt(GOOD, &both, None, Some("clinic"), NOW).is_ok());
        assert!(verify_jwt(OTHER_KEY_NO_KID, &both, None, Some("clinic"), NOW).is_ok());
    }

    #[test]
    fn none_and_a_token_chosen_algorithm_are_refused() {
        fails(ALG_NONE, RuleKey::InvalidSignature);
        fails(ALG_NONE_UPPER, RuleKey::InvalidSignature);
        // HMAC SHA-256 keyed with the public key, as text and as bytes: the
        // key's algorithm is Ed25519, whatever the token says.
        fails(HS256_PUBLIC_TEXT, RuleKey::InvalidSignature);
        fails(HS256_PUBLIC_BYTES, RuleKey::InvalidSignature);
        fails(RFC_7519, RuleKey::InvalidSignature);
        // A key whose own `alg` is another's is never tried.
        let other_alg = key(&[
            ("kty", Value::text("OKP")),
            ("crv", Value::text("Ed25519")),
            ("x", Value::text(KEY)),
            ("kid", Value::text("2026-10")),
            ("alg", Value::text("ES256")),
        ]);
        assert_eq!(
            verify_jwt(GOOD, &[other_alg], None, Some("clinic"), NOW),
            Err(Fault::Broke(RuleKey::InvalidSignature))
        );
    }

    #[test]
    fn a_tampered_or_wrongly_signed_token_is_refused() {
        fails(TAMPERED, RuleKey::InvalidSignature);
        fails(OTHER_KEY_SAME_KID, RuleKey::InvalidSignature);
        fails(OTHER_KEY_NO_KID, RuleKey::InvalidSignature);
        // A key the token carries itself is never trusted.
        fails(EMBEDDED_KEY, RuleKey::InvalidSignature);
        // An extension nothing here understands.
        fails(CRITICAL, RuleKey::InvalidSignature);
        // With no keys at all, nothing verifies.
        assert_eq!(
            verify_jwt(GOOD, &[], None, Some("clinic"), NOW),
            Err(Fault::Broke(RuleKey::InvalidSignature))
        );
        // A signature's last byte flipped.
        let (signed, signature) = GOOD.rsplit_once('.').expect("three parts");
        let mut bytes = decode_base64url(signature).expect("a signature");
        if let Some(last) = bytes.last_mut() {
            *last ^= 1;
        }
        let flipped = format!("{signed}.{}", crate::encoding::to_base64url(&bytes));
        fails(&flipped, RuleKey::InvalidSignature);
    }

    #[test]
    fn times_are_checked_against_the_clock_with_a_small_leeway() {
        fails(EXPIRED, RuleKey::Expired);
        assert!(checked(WITHIN_LEEWAY).is_ok());
        fails(NOT_YET, RuleKey::NotYetValid);
        fails(ISSUED_LATER, RuleKey::NotYetValid);
        // The genuine token itself expires an hour on, less the leeway.
        let later = NOW + 3_600_000 + JWT_LEEWAY_SECONDS * 1000;
        assert_eq!(
            verify_jwt(GOOD, &keys(), None, Some("clinic"), later),
            Err(Fault::Broke(RuleKey::Expired))
        );
        assert!(verify_jwt(GOOD, &keys(), None, Some("clinic"), later - 1).is_ok());
    }

    #[test]
    fn the_issuer_and_the_audience_are_checked_when_named() {
        fails(OTHER_ISSUER, RuleKey::WrongIssuer);
        fails(NO_ISSUER, RuleKey::WrongIssuer);
        fails(OTHER_AUDIENCE, RuleKey::WrongAudience);
        // A token with an audience is refused when the call names none; one
        // with none, only when the call names one.
        assert_eq!(
            verify_jwt(GOOD, &keys(), None, None, NOW),
            Err(Fault::Broke(RuleKey::WrongAudience))
        );
        fails(NO_AUDIENCE, RuleKey::WrongAudience);
        assert!(verify_jwt(NO_AUDIENCE, &keys(), None, None, NOW).is_ok());
    }

    #[test]
    fn a_token_that_isn_t_one_is_malformed() {
        fails(NO_EXPIRY, RuleKey::Malformed);
        fails(FRACTIONAL_EXPIRY, RuleKey::Malformed);
        fails(DUPLICATE_SUBJECT, RuleKey::Malformed);
        fails(NOT_AN_OBJECT, RuleKey::Malformed);
        fails(NESTED, RuleKey::Malformed);
        fails(KID_NOT_TEXT, RuleKey::Malformed);
        fails(CONTROL_IN_SUBJECT, RuleKey::Malformed);
        fails("not.a.token", RuleKey::Malformed);
        fails("", RuleKey::Malformed);
    }

    #[test]
    fn json_naming_a_member_twice_is_refused_at_any_depth() {
        let read = |text: &str| serde_json::from_str::<Strict>(text).map(|Strict(json)| json);
        assert!(read(r#"{"a":1,"b":{"c":2}}"#).is_ok());
        assert!(read(r#"{"a":1,"a":1}"#).is_err());
        assert!(read(r#"{"a":{"c":1,"c":2}}"#).is_err());
        assert!(read(r#"[{"a":1,"a":2}]"#).is_err());
    }

    // Chunk 5.5r4: RS256 and ES256. RFC 7515 §A.2's RSA key, by the id
    // `rsa-1`, and §A.3's P-256 key, by `ec-1`, sign the tokens below, made
    // with OpenSSL around `NOW`, unless the name says otherwise; `EVIL_*`
    // are an attacker's keys and `SMALL_RSA_N` a 1,024-bit modulus.
    const RSA_GOOD: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6InJzYS0xIiwidHlwIjoiSldUIn0.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.a-gfyWZTUFKEgxlMnq0ofX78bU2tIVyLxPyt2ggwIsODue5o4CFhEJAfwRWjIpCUeFAlOosx3uLMfsrIDtLfBosQ-3azQNWxhdSg6t5rBFLqKi6Hp2-ywT3scG9AiCWbR7gq57SEC3bMQzw7jrz4tMwjUUfgEMcn14_VjngNnbFYvV24ZNqrcxFBKqHJXrUVKxh79WgevMgicURa8toA99GWuq0VXSjVSYa789VRtwHelwyA1lUDtvimVWKl3WQh2cv4MTFhU5yDj8OEPT0GaKCJSpGp8pwXGn0pbfnI1e1noiDq02q8ckpvAhKypU6yEIv46NqA8nNhPqQ95FMgYw";
    const RSA_EXPIRED: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6InJzYS0xIiwidHlwIjoiSldUIn0.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzEzOTY5LCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIn0.iMiip0IrwCDvepOh5LiKBeGn1YiX_7m7U-AqG-EeIk5aNB3Gfola9QZ9RsJfOm_GIpnV2XtFo8RecX4ZXHrPHWqwJl4HqF2ua9eBs8TA10-HZBQXcm-TyabNWLeLqoHn_p5EMayNVhx6bBhUtf6idDUd_7oV5ULx9E7CKKynev-zil4KOFS-J894ZUetMl2gHUXWddDR1mkD3mCRcz77sTCM97q3JtKWqbBe9NRaZT56KYSal-AvL_u09-0oKu_Pv6YuhsVtm88_stN5QTakweiR2MifU8CBJ0TuTheMsm61VGQ7T3hLCQk4Oz-J4d2loxeDCMrJHwbr21l8V0WICw";
    const RSA_OTHER_AUDIENCE: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6InJzYS0xIiwidHlwIjoiSldUIn0.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoicGhhcm1hY3kiLCJleHAiOjE3OTIzMTc2MDAsIm5iZiI6MTc5MjMxMzk0MCwiaWF0IjoxNzkyMzEzOTQwLCJqdGkiOiJhMWIyYzMifQ.nVnE89EGV9S3LVtGcpFZeYj1Ge6_ktUEIK80AJA0y7qTzFmY8aLSQTQeTQ3d_bOnaivYXYG9-NK8XVm_mIbwHkn1Ic3RNspK3_8ZRRkNlXKMHhOFnTowyHQOtl-g15FRHFl586tHS887V99D9AMcIfUuNJwqrND5u89x2IlgGAoL2MYMhVJu8rtx7Y1I5yLFUcewO6p-esby8TvRHOYgQUcyVSA0GnsVLP67O58uJCCjnmvQCDDxuXsb7HJ6lJe2iFKCcFR0LGHH7hsyrCrQmREjhc0LkeMi3JKokPxWAd84SkDa8Izd27Iav9GiTyU__nQzPQUbwHxP3cFAV-v1iw";
    const RSA_OTHER_ISSUER: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6InJzYS0xIiwidHlwIjoiSldUIn0.eyJpc3MiOiJodHRwczovL2V2aWwuZXhhbXBsZS5jb20iLCJzdWIiOiJtZW1iZXItNDIiLCJhdWQiOiJjbGluaWMiLCJleHAiOjE3OTIzMTc2MDAsIm5iZiI6MTc5MjMxMzk0MCwiaWF0IjoxNzkyMzEzOTQwLCJqdGkiOiJhMWIyYzMifQ.DdnmKrD-0KARl4ugdBiv6LHp3Z17xVMQ0UTjv1A_R-zLnAAb4zgWw10wSSNBFVbq6alED4QLLjETlDCbOXiUyJ20Dv18IjjlMznPoe7wrEnozqbzp3HAAr-euc2JrGhBWXZPFnDohSRGmhhhuhhooWloY1LKxjnaOQDk-cQx79SKFDIBB_ZYI3Da7g_Bd1tL7HpgXoj7Q3c_RdD9WpqtRK6gduiCiS5MTeiWpjw2ulwscVOdoOUnArwnNLxleW-hlhf8EGEXTFpIE44Rw7zNcUnxXLf-f0Ce-yonHr5GaiX_o1BZ3n7GkIF5lhLgpyHj6t9zSB_-oj_US2bgTcXErA";
    const RSA_TAMPERED: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6InJzYS0xIiwidHlwIjoiSldUIn0.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTEiLCJhdWQiOiJjbGluaWMiLCJleHAiOjE3OTIzMTc2MDAsIm5iZiI6MTc5MjMxMzk0MCwiaWF0IjoxNzkyMzEzOTQwLCJqdGkiOiJhMWIyYzMiLCJlbWFpbCI6ImFkYUBleGFtcGxlLmNvbSIsImVtYWlsX3ZlcmlmaWVkIjp0cnVlLCJhenAiOiJjbGluaWMtd2ViIn0.a-gfyWZTUFKEgxlMnq0ofX78bU2tIVyLxPyt2ggwIsODue5o4CFhEJAfwRWjIpCUeFAlOosx3uLMfsrIDtLfBosQ-3azQNWxhdSg6t5rBFLqKi6Hp2-ywT3scG9AiCWbR7gq57SEC3bMQzw7jrz4tMwjUUfgEMcn14_VjngNnbFYvV24ZNqrcxFBKqHJXrUVKxh79WgevMgicURa8toA99GWuq0VXSjVSYa789VRtwHelwyA1lUDtvimVWKl3WQh2cv4MTFhU5yDj8OEPT0GaKCJSpGp8pwXGn0pbfnI1e1noiDq02q8ckpvAhKypU6yEIv46NqA8nNhPqQ95FMgYw";
    const RSA_ALG_NONE: &str = "eyJhbGciOiJub25lIiwia2lkIjoicnNhLTEifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.a-gfyWZTUFKEgxlMnq0ofX78bU2tIVyLxPyt2ggwIsODue5o4CFhEJAfwRWjIpCUeFAlOosx3uLMfsrIDtLfBosQ-3azQNWxhdSg6t5rBFLqKi6Hp2-ywT3scG9AiCWbR7gq57SEC3bMQzw7jrz4tMwjUUfgEMcn14_VjngNnbFYvV24ZNqrcxFBKqHJXrUVKxh79WgevMgicURa8toA99GWuq0VXSjVSYa789VRtwHelwyA1lUDtvimVWKl3WQh2cv4MTFhU5yDj8OEPT0GaKCJSpGp8pwXGn0pbfnI1e1noiDq02q8ckpvAhKypU6yEIv46NqA8nNhPqQ95FMgYw";
    const RSA_HS256_PUBLIC_KEY: &str = "eyJhbGciOiJIUzI1NiIsImtpZCI6InJzYS0xIiwidHlwIjoiSldUIn0.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.EsNP8oGfkbz3bTQAGTA06RNXvHCRBc5QFbPsEEJi4LM";
    const RSA_OTHER_KEY_SAME_KID: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6InJzYS0xIiwidHlwIjoiSldUIn0.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.JKzBmgqO4HuNIEFk5Kn1Qkdvea639aYpy8OtDtW5F6b9KCa3ttNF6tuUDLd_Z0Fhw5euMs4MZjQtgZ78GV2WIZY7mgT8mFmHwvnwK_Kx_240QOomLhTrGjxSwiZvmpmBtX9vMyScW9Wu6bpLNidMzeIJBHDoxpn8lw7JAI2ynMfNy6LjmzwJUOqCBF2sj2fEQXLQzORCgU0qEJBtGmbkCYZgkS06tEW78V-YdukV-vcdE38g5oEQf4mhicqvA3-XEna9eO34Zc2jWTKmVCEP9ztM-E3kVF-_fXUjqZ7ct9BAy6L8w0AE5rddGR3F3mLwRKNVFgABTfbGerGA5L8euQ";
    const RSA_EMBEDDED_JWK: &str = "eyJhbGciOiJSUzI1NiIsImp3ayI6eyJrdHkiOiJSU0EiLCJuIjoick9wYjNTbXNhdGl4ZGZvME9pUmlZQ1lxYlhSSXpRWlhnRkJncS1GQ1B6dndxQkJQZDNIOEZnMmVJMnBJbE9GeW4yUHpIaWNTRGMzZnpDTFJiSHFNelhCYk1GM0ZLUF9sSHltbzJZZjlmQUpPZUZ0elRhQVRKelJUVTBVTDhZWjZCbVhVVXhvV21LYW1uMnJaVzZnWXE3bVM1UjRUcDVaVzFLeGpHdWhYU1BvRy1rTm10YnF0WU81NElxVllodkgzeU03LVJGZWtybS04MnhudGNVSjBGUmpHX01YV1FlXzNKTDlqbHRlbDFEQ25Zb3dfaFhHWmZrSktja2F4YjVLNkw1QVpRWm1melVkU1BkOEFORmhiaUJZQm5kZi1UelhxRmNub1Bwb3dPTVRUdlBxNlNVdzFjMUhEcUlNYnpmRlJtRzlxcFE1OTVmTkZEdFU1RTFTam93IiwiZSI6IkFRQUIifX0.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.Q6C-GgUc9ybRTYram8A4KNPQ95OTyliHJDsC8bbKIeOsG1I0N5cUoAnD6vWzH25cMiWUjxt_zCuruRkEfjUSsKJdJOtqfl_qUT-r8ta6UoPpRxGYl4YoGR0N3swPC-206cx7LdALDv-WMk6ovzbcVw4QdsgIn5DNy_Ckfh_OiZDgE9x_DmJYqIUFNh-RAvmYadSxBTzwtoGYdYbQB6kyJFSG-UG69MszvXywMG0VoKH7493MHQRsTdOxljowuuLPylDsmjZIxVbB-YlbVkdR3EGrpBlzYxFwlXollyRicMnyQHdr6_Rq96lNZGceVo5Id0QyiYbUvX8djzmcusC94Q";
    const RSA_JKU: &str = "eyJhbGciOiJSUzI1NiIsImprdSI6Imh0dHBzOi8vZXZpbC5leGFtcGxlLmNvbS9rZXlzIn0.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.AcNmxrppfCh11L4-IyYxoPkLLXhZgyhPWHkOMrzkcvZUiaR9o4tq3p4oxXUPL1Ujk2XW85DHCgn1IeTmI4tfU5-yAwkW8GdNq_JoBlebu7NDweDE_Hejgzn7efbbA6r4EQPPpg3rdWRp2Fx7Vu2Gw2ycttN9pfUACx12DXPJRlt-mgEVlbhBQ8tRH_DybxPN-rzl8Q51kFmq7NcoNF4JeMcqKV4k9Zt513QnRgK_H0Kbhuf1UCAcc47WgDk5zZAW5g0HspP5mKn-hNwywf91BSJgyI-x8EBo40HbHh-WwkEW48vbKnz73H5C_AcEiMwOxlgUYGyITdCmapQ7NCAgyA";
    const RSA_X5U: &str = "eyJhbGciOiJSUzI1NiIsIng1dSI6Imh0dHBzOi8vZXZpbC5leGFtcGxlLmNvbS9jZXJ0LnBlbSJ9.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.eB5PBJG78t0FFNMKYalPOpwU7aY3PiEAdL1MaJJEN2WJ46o9hNVkRvxD-EyJh3qv-tgMROgbfKKwnu1PNt-3NstFkPui0vDdLs3Ere69tpeiAGs9O7_xNlvxQNV0-MdrWsy-GXCPRvGX5sDbb5OFWW8xfjU-YLzek6D8j_WFAvCcq3XwjBdHvnBslrSZThI0O4x5huwRZ4GNdsZV9zb79XGwSWZU5hSWs3wOm0_XTWPt8qaYCDKsa0KtsrvGz1y8IU1wEKauJCQognWdJpIjWvqOy-f32PEcabSF5ciBWx4iC7_brTmY21J_5RqvRVPv33zHP3w1hD4wFSsEKFkPFQ";
    const RSA_JKU_GENUINE: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6InJzYS0xIiwiamt1IjoiaHR0cHM6Ly9ldmlsLmV4YW1wbGUuY29tL2tleXMifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.lSNC-D4eYWja_83IA5SB2HzJFFNQegTij10bd_Zbz1xt6abD0LIZp211WitrekD_tXyIj94IKiZQ3eenjWS_xYidiKbjJtSI9v_wecGbeApwtQyqyqwKxA4R3w8qZldBu_3Lnz4rhXjaqhSboKkE7sfIHWSrgwWG0TWWSKYcRkMe9gje16BeG1jVocqscMvlrUSFAbnk67cuO20OBiFNehhRWphbVHPrh2ekznNsaNVT4gTDHbWDYem4TKG_F007wgKKlryfxV2Q6wBSLNXPMmGEtWhyO4JmDZJBrNWK1YP3YMF1aTrViKNrfu9gUFmQPdPvBjsNPDAxk-tyQvlJSg";
    const RSA_CRIT: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6InJzYS0xIiwiY3JpdCI6WyJleHAiXSwiZXhwIjoxNzkyMzE3NjAwfQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.lmOHPv8hmh96heQx_2jBi5qIxT6GAw6_x7z8fdQFE359sq1OrW7GnAvHnUfMEKFlZfuN7GR7G5Mhag6pyTsLizOUEciU3of5KBTJEXGaYkevCeCUF3njSlzPbCst1m0ejRN8YauG7F9SharyZpcmi8X-_otrGUXrUgyGUFu1pW6K-SByQWMai-ddfWeQwlpHDixJ6-1_Jva1M5n5ZpgDpC9u-peQisDDTj4Wl1rQBoROPA-BSi_Sq8kxau8gKRjnZBTJEV6CEcezktaNwUGX0WlzophNaE2znDzaLzr33Ueek3AuiDI-EOHYRqAL2jo0QrRuTR5HbqLk_GndYxCboA";
    const RSA_DUPLICATE_SUBJECT: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6InJzYS0xIiwidHlwIjoiSldUIn0.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiIsInN1YiI6Im1lbWJlci0xIn0.gLlnrGftDriLHBKVhrBXwaXAgZgl7KAGUPjcMQifdTmQiRv_NCgBaZ-zEx8tYoT3alFj_y2HtQtyrIYYsO2N9DEtSAhfpuRh6NzwDO7PDtSTg0ICaS4cgr4Ftsbn-69-yTIGKWFjVa6G6cW1pxiaOCbB6q-Xlj--EiIDMz8FhkzxBSPNKzveXPPguK40RdT9sDzdzsWv0zniG9jrTI-TpymQvj6gwjwP32vXnvtqPySn2WvpF1M6LZHF_ZXu3xSNnw3qL-nj_NVtXnTW4YlVqlyN-5tcGSD12XX60xsd5ZWK4Z6huwGbJ1p1UBwGGq3VIG0yYPceY4FpgOabgZJQSQ";
    const RSA_EMAIL_NOT_A_FLAG: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6InJzYS0xIiwidHlwIjoiSldUIn0.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6InllcyIsImF6cCI6ImNsaW5pYy13ZWIifQ.QNbt57Bah-sLVNeOfEL4yMyNmUdJmNSUPkDyebskCF_7ai6bJCkgGLaR3mtkuR2NnwYM8RTJYKws0LAHbCojPhqCj8f2h_MuM51YnbVzxhmVM13_IbdYYIqv1SxIQczETo-t_3MVVoLb6mewLo_Ma4x4eJQ8P9Q2jxa552EfwzVbCvqXsK39V3qUa5Qiju0eTfnJaY2VVRGQa5FUKoHU901FAONaQhMm6h7_O7yJ8SCHqoP7nbRqkizUftnrntHIyVe3ggwgr2eU6UOW3WcnkUKpWyYFlys7p22xXgvfODUgJickJYQAwsew1XcFAidXDVQzFV0NrNRil5ZYBLjkgw";
    const RSA_NO_EMAIL: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6InJzYS0xIiwidHlwIjoiSldUIn0.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIn0.LUmK07jeTEZTJApBtHhgQ6R5qpiMMCGPVSOxt6wtiA12YQg_29H1yvTdyBZdqh4I374zOwPaLU3eYlbGL4Si9kD3C4Eg99WEriLiSi_gLIi6nfiYzIxmYUvolVQbHS_xRo7cnJnOxbKPNtq3wT1VXBlN59aByGhr4bpyALo7JBEFqh8WOuaaT0cMIUYTHDuGsY8fU60mqcGGoNVtu2vW1i5MUBT8bJ3AsNhOIgWc7sTd2ATPZdt2Ia6NlTkopfl45oXnpCdHaKKRlohyfaB2eFYthkWzLEP48vmMOsKZoBXgjvJtKtLTMTLj1g4rtCuE2H-0rj3zEOjkBY5Q-t3obg";
    const EC_GOOD: &str = "eyJhbGciOiJFUzI1NiIsImtpZCI6ImVjLTEiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.t9RtrfYGqeVUUB7C2n9TofDzbl0SSaY-VTjSAydqG0BlYP_KEAWZNzs2aClILrxc2-i9wXyNSnzDP2GaVkRuQA";
    const EC_EXPIRED: &str = "eyJhbGciOiJFUzI1NiIsImtpZCI6ImVjLTEiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzEzOTY5LCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIn0.OPybO4zZlnSREk3n5c6T_-nd3tm3Bu1mp4NnNbuQVd1MQXuyV00-1IY3HoiYy1pJuOGlN_UN4hrRGNl5k6MIWw";
    const EC_OTHER_AUDIENCE: &str = "eyJhbGciOiJFUzI1NiIsImtpZCI6ImVjLTEiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoicGhhcm1hY3kiLCJleHAiOjE3OTIzMTc2MDAsIm5iZiI6MTc5MjMxMzk0MCwiaWF0IjoxNzkyMzEzOTQwLCJqdGkiOiJhMWIyYzMifQ.fwNFvMFXdaRLkM8MZTQeHNeoD_F8ahXpcx92bIT8URWQs9QeuSy0RPpdef2ep1LFPtIi6R-w8hbcXVCTIlZ3Kg";
    const EC_OTHER_ISSUER: &str = "eyJhbGciOiJFUzI1NiIsImtpZCI6ImVjLTEiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2V2aWwuZXhhbXBsZS5jb20iLCJzdWIiOiJtZW1iZXItNDIiLCJhdWQiOiJjbGluaWMiLCJleHAiOjE3OTIzMTc2MDAsIm5iZiI6MTc5MjMxMzk0MCwiaWF0IjoxNzkyMzEzOTQwLCJqdGkiOiJhMWIyYzMifQ.WI5PvaRXiMMAq5uPk_pqkAVH02JT2SQf5ZTJTJctUBR1jwuSpOj99BAyXPRgEqEdyZA79bHvsbc4nYoTlvGaMw";
    const EC_TAMPERED: &str = "eyJhbGciOiJFUzI1NiIsImtpZCI6ImVjLTEiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTEiLCJhdWQiOiJjbGluaWMiLCJleHAiOjE3OTIzMTc2MDAsIm5iZiI6MTc5MjMxMzk0MCwiaWF0IjoxNzkyMzEzOTQwLCJqdGkiOiJhMWIyYzMiLCJlbWFpbCI6ImFkYUBleGFtcGxlLmNvbSIsImVtYWlsX3ZlcmlmaWVkIjp0cnVlLCJhenAiOiJjbGluaWMtd2ViIn0.t9RtrfYGqeVUUB7C2n9TofDzbl0SSaY-VTjSAydqG0BlYP_KEAWZNzs2aClILrxc2-i9wXyNSnzDP2GaVkRuQA";
    const EC_ALG_NONE: &str = "eyJhbGciOiJub25lIiwia2lkIjoiZWMtMSJ9.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.t9RtrfYGqeVUUB7C2n9TofDzbl0SSaY-VTjSAydqG0BlYP_KEAWZNzs2aClILrxc2-i9wXyNSnzDP2GaVkRuQA";
    const EC_HS256_PUBLIC_KEY: &str = "eyJhbGciOiJIUzI1NiIsImtpZCI6ImVjLTEiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.A_e1k62suuCvcp2YwcqJgES-BC7e7PBYJ1SQO7i3vbA";
    const EC_OTHER_KEY_SAME_KID: &str = "eyJhbGciOiJFUzI1NiIsImtpZCI6ImVjLTEiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.F8Mc4zLzSqbfY7DjYopyNqxtFYYPmzb_Ejy1GcCl6wKzYyMzOvSl4cot0TEPKW5xtEn7568ANi72R_dcJCdxbg";
    const EC_EMBEDDED_JWK: &str = "eyJhbGciOiJFUzI1NiIsImp3ayI6eyJrdHkiOiJFQyIsImNydiI6IlAtMjU2IiwieCI6IldmNldacGMyMjQ3ZnVoQk5rT202UldlTl96Ykp3MjRlYTdyVERDVFVhMlEiLCJ5IjoiRzVKbmRSRU1UR0FtRE9JSDFMMWIyZVdxTExUZ3NOalpWREZFSFVLZGNtQSJ9fQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.CKhilASsqJUPc_0UGEErXN8oeEC3NpEgIjNpeKgFXhqBZ1yj4JayjMk7cXHKf_H3Xw_PyxQt8azdyacsNrRZow";
    const EC_JKU: &str = "eyJhbGciOiJFUzI1NiIsImprdSI6Imh0dHBzOi8vZXZpbC5leGFtcGxlLmNvbS9rZXlzIn0.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.O5n3GAQ6kiTBKVD900xtePKC8HHjVZ_ds1blRpuszAPQ15BmqPufwnJZ_vf9RK3y8KPylZZILqZmCmbokmR7TA";
    const EC_X5U: &str = "eyJhbGciOiJFUzI1NiIsIng1dSI6Imh0dHBzOi8vZXZpbC5leGFtcGxlLmNvbS9jZXJ0LnBlbSJ9.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.5g1Q50Pu2YgZqWtJd3Uzdix0y14gbHGPl4s6t4dtf288LHbLpm7QN68dLCRjapqoI7RWU0v8u75931uRwwIVBg";
    const EC_JKU_GENUINE: &str = "eyJhbGciOiJFUzI1NiIsImtpZCI6ImVjLTEiLCJqa3UiOiJodHRwczovL2V2aWwuZXhhbXBsZS5jb20va2V5cyJ9.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.y-hNpGh5IGc0unEhQADtfeQP9Ja66w281fmuLeCokQ7d3fsJD1BR_BSOLPRIOhiWTOMB9G3tlj93l94w2zyaQg";
    const EC_CRIT: &str = "eyJhbGciOiJFUzI1NiIsImtpZCI6ImVjLTEiLCJjcml0IjpbImV4cCJdLCJleHAiOjE3OTIzMTc2MDB9.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.AnQvQk-FgwHESNpeeTW5Sh6h8tJP7yFJ46J1Mp02JwGwJZkiiBg0Nq7YabnTKmDfI09ZpaqrxwlsHiKto29zMg";
    const EC_DUPLICATE_SUBJECT: &str = "eyJhbGciOiJFUzI1NiIsImtpZCI6ImVjLTEiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiIsInN1YiI6Im1lbWJlci0xIn0.99ipXDg95m1LBHQCi-4N3E6D1VXkEpUkCC6tPr3mjC5h0PECGVEK6IbdtxfLBXX__jJUkARR051gZXJ8qoBoLA";
    const EC_EMAIL_NOT_A_FLAG: &str = "eyJhbGciOiJFUzI1NiIsImtpZCI6ImVjLTEiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6InllcyIsImF6cCI6ImNsaW5pYy13ZWIifQ.O55ykoHdkjcZ7TFe7RDU3tLOJbLjum2ngB8xoWyg8J8XnCRCBm-Y3H6hgZAxPGz8mY0CYuq30I3riGYuN3mX6A";
    const EC_NO_EMAIL: &str = "eyJhbGciOiJFUzI1NiIsImtpZCI6ImVjLTEiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIn0.K612c1Htjg5APrJ297tfbFon6kOz7tSAjQ1ZuUCSf1uZD2uftvt4w-X9khNWOAhQ-5AiUEb7Wr3TRqIo751dZA";
    const RSA_SIGNED_EC_KID: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6ImVjLTEifQ.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.ZrJYeDnIAWdpUEiAQib_QHQgpEEiv726xbD4jjazvDcUr62oFjN2ZWb65f7OoP4SLQUv93q-l8r4dx6wB_o5PsZP2jlNeW38X9reEIcalFtDh0D_x7hW03yDzwBKfiif0ewwp8qtTFvA9etFsrO2wjUY1Fyqim6JXaTxQo9-Y3tZU1gKtPgc-bONVak6LxGdh1sw3BobpEG40KuL7NpjZKBpJC0vuMai9OHvqPG-iSOKtLrQXYcm4LZjM6TNghjdq05sGrNJ6R0K-tXUpr6rrrzniPStnS_quQrGSqSmRS9HaccRlA7LFjQyC0GQ0RuFpxeW0UqpP6dJsbgrLINUDQ";
    const EC_SIGNED_RSA_KID: &str = "eyJhbGciOiJFUzI1NiIsImtpZCI6InJzYS0xIn0.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.DedL0SvCvju1My8cy8MLvVbzP9Um-zH6R6RBcd4L3C0iE-24u8LOjfWrMIUsYmd4k37v0rGivYkGo8Z8C-QJkg";
    const RSA_SIGNED_ES256_HEADER: &str = "eyJhbGciOiJFUzI1NiIsImtpZCI6InJzYS0xIn0.eyJpc3MiOiJodHRwczovL2lkLmV4YW1wbGUuY29tIiwic3ViIjoibWVtYmVyLTQyIiwiYXVkIjoiY2xpbmljIiwiZXhwIjoxNzkyMzE3NjAwLCJuYmYiOjE3OTIzMTM5NDAsImlhdCI6MTc5MjMxMzk0MCwianRpIjoiYTFiMmMzIiwiZW1haWwiOiJhZGFAZXhhbXBsZS5jb20iLCJlbWFpbF92ZXJpZmllZCI6dHJ1ZSwiYXpwIjoiY2xpbmljLXdlYiJ9.JKIVLPZwlkVBjdq0cRPnVVOZ_La5_lfPIXtN2Sk_Z0sbdY5WmUJSP-tMMPegP5MRzRp2NkkxGMdALs6iDnYWm-l1t7HBQgVs36mx8sPg2TmOlCnxlnS1ZH4yB4wYYJuEEyx6hoi-xN2RqN6iKl6w2e4PH6WUfcttrja9PfNV8Du5Hsm44CRuqUooJ-5YC55xp3WCgI5CrfVMzO8xZXOpYmsEumI_XbqCTAD2KC317q7xdlULZcyeBIezyIK9w2gqnpLjz9Vmhf2Bgv_MrF2atC3V8WKnGLNz1sYINj9wvTfR1YkyNP639t_vd4qsLzqJRtiSgq9TLR1awLNurWnplw";
    const SMALL_RSA_N: &str = "pb1wVenLotoly3kkTJYZAgVZitibPm2YTBTdYJobj6gVv5md3RjcXfdHd_X63n5hbTwk-sggCicy68o4dl0uSsIqmLp20jv1jmC3kz1a4RNPMcAl3Okn_4GAiZN2ZY7KVgngzSdpM4zJYDg19chUZ6hFuS7zyuXVA9Y9fSuCDzk";
    const EVIL_RSA_N: &str = "rOpb3Smsatixdfo0OiRiYCYqbXRIzQZXgFBgq-FCPzvwqBBPd3H8Fg2eI2pIlOFyn2PzHicSDc3fzCLRbHqMzXBbMF3FKP_lHymo2Yf9fAJOeFtzTaATJzRTU0UL8YZ6BmXUUxoWmKamn2rZW6gYq7mS5R4Tp5ZW1KxjGuhXSPoG-kNmtbqtYO54IqVYhvH3yM7-RFekrm-82xntcUJ0FRjG_MXWQe_3JL9jltel1DCnYow_hXGZfkJKckaxb5K6L5AZQZmfzUdSPd8ANFhbiBYBndf-TzXqFcnoPpowOMTTvPq6SUw1c1HDqIMbzfFRmG9qpQ595fNFDtU5E1Sjow";
    const RSA_CERT: &str = "MIIDCzCCAfOgAwIBAgIEHy49TDANBgkqhkiG9w0BAQsFADAdMRswGQYDVQQDDBJqYWRwby1qd2tzLmV4YW1wbGUwHhcNMjYxMDAxMDAwMDAwWhcNMzYxMDAxMDAwMDAwWjAdMRswGQYDVQQDDBJqYWRwby1qd2tzLmV4YW1wbGUwggEiMA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQCh+BYK4uPJtGXOjS1lYmM2K5J9vinh8CR3/BYlzJChNuOL2TSXxbbqY913EeZ8dCn5VrD7io8ImtxLaYk8wTM/U+3QGbh3hCUv7JFP5IV3aVlL6kKA0ywPVb9ilE8TA5a8bpvfbr3SvaNnjuygxmj3AbONv/s4yDQs4v5tJ/reSlpIdJed1LnPmt7Ex1sFhSwsD174pcF1A5L5ROjtZMEQxrZHYJqkeDrrnGya11UxMFBji4NmXG9veoKjlnAqH2QbgtPr8jkiGUkftoaHLFcW9Qr4NY2ai50Xw0Byj3+H2JoY2PyrZ62EWQwuz3WTOTY8BwNNb2BvniHgVFbK5emhAgMBAAGjUzBRMB0GA1UdDgQWBBQzGIiPp7+DkM3e011UielRYxna2TAfBgNVHSMEGDAWgBQzGIiPp7+DkM3e011UielRYxna2TAPBgNVHRMBAf8EBTADAQH/MA0GCSqGSIb3DQEBCwUAA4IBAQB4Wur8KUXryaWEVRE2RxcMV6KlJ5YyznKsfUZJcEBAB9woW1MEol8LHFz1hIyB6nQYAul25kaAhIrCh0OHZeix2GkrmFHmDvNPFoOXu/hSBSI/XxFCoUCY3QsUaHoWNqN/VBmjpMiaOGYO5G/LV80uG4/8w2pdG/C96PWkInb+iLl1n+0gt4uOlfsJeLv941A5s24zhT1gVlhucKo38HxGfugjT9C4l1mTo4Hiu/lG/gEgmEM6NF7pbymasTYDmRx6kI31Zb/Ai6ELBeIfokOvvCUNGFsTnK5+bZtiq1yGfly8ahVWYh34h3bpLSi89/eNAHa/rwYUbGvi/nbmAaPx";
    const EVIL_CERT: &str = "MIIDCzCCAfOgAwIBAgIEHy49TDANBgkqhkiG9w0BAQsFADAdMRswGQYDVQQDDBJqYWRwby1qd2tzLmV4YW1wbGUwHhcNMjYxMDAxMDAwMDAwWhcNMzYxMDAxMDAwMDAwWjAdMRswGQYDVQQDDBJqYWRwby1qd2tzLmV4YW1wbGUwggEiMA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQCs6lvdKaxq2LF1+jQ6JGJgJiptdEjNBleAUGCr4UI/O/CoEE93cfwWDZ4jakiU4XKfY/MeJxINzd/MItFseozNcFswXcUo/+UfKajZh/18Ak54W3NNoBMnNFNTRQvxhnoGZdRTGhaYpqafatlbqBiruZLlHhOnllbUrGMa6FdI+gb6Q2a1uq1g7ngipViG8ffIzv5EV6Sub7zbGe1xQnQVGMb8xdZB7/ckv2OW16XUMKdijD+FcZl+QkpyRrFvkrovkBlBmZ/NR1I93wA0WFuIFgGd1/5PNeoVyeg+mjA4xNO8+rpJTDVzUcOogxvN8VGYb2qlDn3l80UO1TkTVKOjAgMBAAGjUzBRMB0GA1UdDgQWBBS5i01sJa2jd1jR9aZ4KA4rcIhjzDAfBgNVHSMEGDAWgBS5i01sJa2jd1jR9aZ4KA4rcIhjzDAPBgNVHRMBAf8EBTADAQH/MA0GCSqGSIb3DQEBCwUAA4IBAQAEz30hZw31MjGmt9jTq1reKEUrgMmn4uvB1ZkmyTqx3aP/D9beHsa/eXOt3SesWFbGNtaGy9TnugCbaPX9iQCWpuN1XTQ9VjW09P7O5voehwHjhkM+u1QXs6uS68VQ2n1L5gUQTN/TIU/skEwFEIbsq/RklGHe8f3Y58tlxMtjivfkC0A0lqJ47Z3lUfqKmvcWRs0mUX+px9pSH8AzI3T/HyIYFdrlEfU9slQDWjrhzvjBdbly4b0BYaVO5x5uWcAYnJNvkHgv3naWLtyXZ6GR4g8SLKh19G0Fp+mG5tKMAjOFOTyHta2CCKx9y6P8H2bfdCjQ/IvohyWhSe303gRL";
    const RFC_7515_RSA_N: &str = "ofgWCuLjybRlzo0tZWJjNiuSfb4p4fAkd_wWJcyQoTbji9k0l8W26mPddxHmfHQp-Vaw-4qPCJrcS2mJPMEzP1Pt0Bm4d4QlL-yRT-SFd2lZS-pCgNMsD1W_YpRPEwOWvG6b32690r2jZ47soMZo9wGzjb_7OMg0LOL-bSf63kpaSHSXndS5z5rexMdbBYUsLA9e-KXBdQOS-UTo7WTBEMa2R2CapHg665xsmtdVMTBQY4uDZlxvb3qCo5ZwKh9kG4LT6_I5IhlJH7aGhyxXFvUK-DWNmoudF8NAco9_h9iaGNj8q2ethFkMLs91kzk2PAcDTW9gb54h4FRWyuXpoQ";
    const RFC_7515_EC_X: &str = "f83OJ3D2xF1Bg8vub9tLe1gHMzV76e8Tus9uPHvRVEU";
    const RFC_7515_EC_Y: &str = "x_FEzRu9m36HLN_tue659LNpXW6pCyStikYjKIWI5a0";
    /// RFC 7515 §A.2's RS256 token and §A.3's ES256 token, whose claims
    /// are `{"iss":"joe","exp":1300819380,"http://example.com/is_root":true}`.
    const RFC_7515_RS256: &str = "eyJhbGciOiJSUzI1NiJ9.eyJpc3MiOiJqb2UiLA0KICJleHAiOjEzMDA4MTkzODAsDQogImh0dHA6Ly9leGFtcGxlLmNvbS9pc19yb290Ijp0cnVlfQ.cC4hiUPoj9Eetdgtv3hF80EGrhuB__dzERat0XF9g2VtQgr9PJbu3XOiZj5RZmh7AAuHIm4Bh-0Qc_lF5YKt_O8W2Fp5jujGbds9uJdbF9CUAr7t1dnZcAcQjbKBYNX4BAynRFdiuB--f_nZLgrnbyTyWzO75vRK5h6xBArLIARNPvkSjtQBMHlb1L07Qe7K0GarZRmB_eSN9383LcOLn6_dO--xi12jzDwusC-eOkHWEsqtFZESc6BfI7noOPqvhJ1phCnvWh6IeYI2w9QOYEUipUTI8np6LbgGY9Fs98rqVt5AXLIhWkWywlVmtVrBp0igcN_IoypGlUPQGe77Rw";
    const RFC_7515_ES256: &str = "eyJhbGciOiJFUzI1NiJ9.eyJpc3MiOiJqb2UiLA0KICJleHAiOjEzMDA4MTkzODAsDQogImh0dHA6Ly9leGFtcGxlLmNvbS9pc19yb290Ijp0cnVlfQ.DtEhU3ljbEg8L38VWAfUAqOyKAM6-Xx-F4GawxaepmXFCgfTjDxw5djxLa8ISlSApmWQxfKTUJqPP3-Kg6NU1Q";
    /// 2011-03-22T18:42:00Z, a minute before RFC 7515's tokens expire.
    const RFC_7515_NOW: i64 = 1_300_819_320_000;
    /// RFC 7520 §3.3's RSA key and §4.1's RS256 signature, by its id, of a
    /// payload that isn't claims.
    const RFC_7520_N: &str = "n4EPtAOCc9AlkeQHPzHStgAbgs7bTZLwUBZdR8_KuKPEHLd4rHVTeT-O-XV2jRojdNhxJWTDvNd7nqQ0VEiZQHz_AJmSCpMaJMRBSFKrKb2wqVwGU_NsYOYL-QtiWN2lbzcEe6XC0dApr5ydQLrHqkHHig3RBordaZ6Aj-oBHqFEHYpPe7Tpe-OfVfHd1E6cS6M1FZcD1NNLYD5lFHpPI9bTwJlsde3uhGqC0ZCuEHg8lhzwOHrtIQbS0FVbb9k3-tVTU4fg_3L_vniUFAKwuCLqKnS2BYwdq_mzSnbLY7h_qixoR7jig3__kRhuaxwUkRz5iaiQkqgc5gHdrNP5zw";
    const RFC_7520_RS256: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6ImJpbGJvLmJhZ2dpbnNAaG9iYml0b24uZXhhbXBsZSJ9.SXTigJlzIGEgZGFuZ2Vyb3VzIGJ1c2luZXNzLCBGcm9kbywgZ29pbmcgb3V0IHlvdXIgZG9vci4gWW91IHN0ZXAgb250byB0aGUgcm9hZCwgYW5kIGlmIHlvdSBkb24ndCBrZWVwIHlvdXIgZmVldCwgdGhlcmXigJlzIG5vIGtub3dpbmcgd2hlcmUgeW91IG1pZ2h0IGJlIHN3ZXB0IG9mZiB0by4.MRjdkly7_-oTPTS3AXP41iQIGKa80A0ZmTuV5MEaHoxnW2e5CZ5NlKtainoFmKZopdHM1O2U4mwzJdQx996ivp83xuglII7PNDi84wnB-BDkoBwA78185hX-Es4JIwmDLJK3lfWRa-XtL0RnltuYv746iYTh_qHRD68BNt1uSNCrUCTJDt5aAE6x8wW1Kt9eRo4QPocSadnHXFxnt8Is9UzpERV0ePPQdLuW3IS_de3xyIrDaLGdjluPxUAhb6L2aXic1U12podGU0KLUQSE_oI-ZnmKJ3F4uOZDnd6QZWJushZ41Axf_fcIe8u9ipH84ogoree7vjbU5y18kDquDg";

    fn rsa_key(n: &str, kid: Option<&str>) -> Value {
        key(&[
            ("kty", Value::text("RSA")),
            ("n", Value::text(n)),
            ("e", Value::text("AQAB")),
            ("kid", kid.map_or(Value::None, Value::text)),
        ])
    }

    fn ec_key(x: &str, y: &str, kid: Option<&str>) -> Value {
        key(&[
            ("kty", Value::text("EC")),
            ("crv", Value::text("P-256")),
            ("x", Value::text(x)),
            ("y", Value::text(y)),
            ("kid", kid.map_or(Value::None, Value::text)),
        ])
    }

    /// Both identity-provider keys, as one key set sends them.
    fn provider_keys() -> Vec<Value> {
        vec![
            rsa_key(RFC_7515_RSA_N, Some("rsa-1")),
            ec_key(RFC_7515_EC_X, RFC_7515_EC_Y, Some("ec-1")),
        ]
    }

    fn provider(token: &str) -> Result<Value, Fault> {
        verify_jwt(
            token,
            &provider_keys(),
            Some("https://id.example.com"),
            Some("clinic"),
            NOW,
        )
    }

    #[test]
    fn rfc_7515_s_rs256_and_es256_tokens_verify() {
        for (token, keys) in [
            (RFC_7515_RS256, vec![rsa_key(RFC_7515_RSA_N, None)]),
            (
                RFC_7515_ES256,
                vec![ec_key(RFC_7515_EC_X, RFC_7515_EC_Y, None)],
            ),
        ] {
            let claims = verify_jwt(token, &keys, Some("joe"), None, RFC_7515_NOW).expect(token);
            assert_eq!(field(&claims, "iss"), &Value::text("joe"));
            assert_eq!(field(&claims, "exp"), &Value::Instant(1_300_819_380_000));
            // Each against the other's key, and after it expires.
            let swapped = if token == RFC_7515_RS256 {
                vec![ec_key(RFC_7515_EC_X, RFC_7515_EC_Y, None)]
            } else {
                vec![rsa_key(RFC_7515_RSA_N, None)]
            };
            assert_eq!(
                verify_jwt(token, &swapped, None, None, RFC_7515_NOW),
                Err(Fault::Broke(RuleKey::InvalidSignature))
            );
            assert_eq!(
                verify_jwt(token, &keys, None, None, RFC_7515_NOW + 120_000),
                Err(Fault::Broke(RuleKey::Expired))
            );
        }
        // RFC 7515 §A.3's S is above half the group's order: a high S is
        // accepted, as RFC 7518 and `p256` accept it.
        let signature = decode_base64url(RFC_7515_ES256.rsplit('.').next().unwrap_or_default())
            .unwrap_or_default();
        assert_eq!(signature[32], 197);
    }

    #[test]
    fn rfc_7520_s_rs256_signature_verifies_and_its_payload_isn_t_claims() {
        let bilbo = rsa_key(RFC_7520_N, Some("bilbo.baggins@hobbiton.example"));
        assert_eq!(
            verify_jwt(
                RFC_7520_RS256,
                std::slice::from_ref(&bilbo),
                None,
                None,
                NOW
            ),
            Err(Fault::Broke(RuleKey::Malformed))
        );
        // Under another key the signature fails first.
        assert_eq!(
            verify_jwt(
                RFC_7520_RS256,
                &[rsa_key(RFC_7515_RSA_N, None)],
                None,
                None,
                NOW
            ),
            Err(Fault::Broke(RuleKey::InvalidSignature))
        );
        let Some(Key {
            public: Public::Rsa(_),
            ..
        }) = key_of(&bilbo)
        else {
            panic!("an RSA key");
        };
    }

    #[test]
    fn a_genuine_rs256_or_es256_token_gives_its_claims() {
        for token in [RSA_GOOD, EC_GOOD, RSA_JKU_GENUINE, EC_JKU_GENUINE] {
            let claims = provider(token).expect(token);
            assert_eq!(field(&claims, "sub"), &Value::text("member-42"));
            assert_eq!(field(&claims, "exp"), &Value::Instant(NOW + 3_600_000));
        }
        // Each found by its id among Ed25519 keys too; a token naming an id
        // finds no key without one, and one naming none tries every key.
        let mut mixed = vec![jwk(KEY, Some("2026-10"))];
        mixed.extend(provider_keys());
        assert!(verify_jwt(RSA_GOOD, &mixed, None, Some("clinic"), NOW).is_ok());
        assert!(verify_jwt(GOOD, &mixed, None, Some("clinic"), NOW).is_ok());
        let unnamed = vec![
            rsa_key(RFC_7515_RSA_N, None),
            ec_key(RFC_7515_EC_X, RFC_7515_EC_Y, None),
        ];
        assert!(verify_jwt(RSA_GOOD, &unnamed, None, Some("clinic"), NOW).is_err());
        assert!(verify_jwt(RFC_7515_RS256, &unnamed, None, None, RFC_7515_NOW).is_ok());
        assert!(verify_jwt(RFC_7515_ES256, &unnamed, None, None, RFC_7515_NOW).is_ok());
    }

    /// RFC 8725's attacks, each refused under RS256 and under ES256.
    #[test]
    fn the_attack_suite_is_refused_under_both_algorithms() {
        for (token, key) in [
            // `alg: none`, with the genuine token's signature.
            (RSA_ALG_NONE, RuleKey::InvalidSignature),
            (EC_ALG_NONE, RuleKey::InvalidSignature),
            // The public key's PEM as an HMAC secret.
            (RSA_HS256_PUBLIC_KEY, RuleKey::InvalidSignature),
            (EC_HS256_PUBLIC_KEY, RuleKey::InvalidSignature),
            // A key of one kind named for a token of the other.
            (RSA_SIGNED_EC_KID, RuleKey::InvalidSignature),
            (EC_SIGNED_RSA_KID, RuleKey::InvalidSignature),
            (RSA_SIGNED_ES256_HEADER, RuleKey::InvalidSignature),
            // Expired, for another audience, from another issuer.
            (RSA_EXPIRED, RuleKey::Expired),
            (EC_EXPIRED, RuleKey::Expired),
            (RSA_OTHER_AUDIENCE, RuleKey::WrongAudience),
            (EC_OTHER_AUDIENCE, RuleKey::WrongAudience),
            (RSA_OTHER_ISSUER, RuleKey::WrongIssuer),
            (EC_OTHER_ISSUER, RuleKey::WrongIssuer),
            // A payload changed after signing.
            (RSA_TAMPERED, RuleKey::InvalidSignature),
            (EC_TAMPERED, RuleKey::InvalidSignature),
            // Signed by the attacker, with the key, or a link to it, in the
            // header, or under the genuine key's id.
            (RSA_EMBEDDED_JWK, RuleKey::InvalidSignature),
            (EC_EMBEDDED_JWK, RuleKey::InvalidSignature),
            (RSA_JKU, RuleKey::InvalidSignature),
            (EC_JKU, RuleKey::InvalidSignature),
            (RSA_X5U, RuleKey::InvalidSignature),
            (EC_X5U, RuleKey::InvalidSignature),
            (RSA_OTHER_KEY_SAME_KID, RuleKey::InvalidSignature),
            (EC_OTHER_KEY_SAME_KID, RuleKey::InvalidSignature),
            // An extension nothing here understands.
            (RSA_CRIT, RuleKey::InvalidSignature),
            (EC_CRIT, RuleKey::InvalidSignature),
            // A claim named twice.
            (RSA_DUPLICATE_SUBJECT, RuleKey::Malformed),
            (EC_DUPLICATE_SUBJECT, RuleKey::Malformed),
        ] {
            assert_eq!(provider(token), Err(Fault::Broke(key)), "{token}");
        }
    }

    #[test]
    fn a_signature_of_the_wrong_size_or_altered_is_refused() {
        for token in [RSA_GOOD, EC_GOOD] {
            let (signed, signature) = token.rsplit_once('.').expect("three parts");
            let bytes = decode_base64url(signature).expect("a signature");
            let mut flipped = bytes.clone();
            if let Some(last) = flipped.last_mut() {
                *last ^= 1;
            }
            let mut longer = bytes.clone();
            longer.push(0);
            let mut shorter = bytes.clone();
            shorter.remove(0);
            let mut zeros = bytes.clone();
            zeros.iter_mut().for_each(|byte| *byte = 0);
            for altered in [flipped, longer, shorter, zeros] {
                let token = format!("{signed}.{}", crate::encoding::to_base64url(&altered));
                assert_eq!(
                    provider(&token),
                    Err(Fault::Broke(RuleKey::InvalidSignature))
                );
            }
        }
    }

    #[test]
    fn a_key_nothing_can_check_with_is_never_tried() {
        // A 1,024-bit modulus, a P-256 point off the curve, a key for
        // encryption, and an algorithm that isn't the key's.
        assert!(key_of(&rsa_key(SMALL_RSA_N, None)).is_none());
        let mut y = decode_base64url(RFC_7515_EC_Y).expect("y");
        y[31] ^= 1;
        let off = ec_key(RFC_7515_EC_X, &crate::encoding::to_base64url(&y), None);
        assert!(key_of(&off).is_none());
        let encryption = key(&[
            ("kty", Value::text("RSA")),
            ("n", Value::text(RFC_7515_RSA_N)),
            ("e", Value::text("AQAB")),
            ("use_", Value::text("enc")),
        ]);
        assert!(key_of(&encryption).is_none());
        let confused = key(&[
            ("kty", Value::text("RSA")),
            ("n", Value::text(RFC_7515_RSA_N)),
            ("e", Value::text("AQAB")),
            ("alg", Value::text("ES256")),
        ]);
        assert!(key_of(&confused).is_none());
        assert_eq!(
            verify_jwt(RFC_7515_RS256, &[confused], None, None, RFC_7515_NOW),
            Err(Fault::Broke(RuleKey::InvalidSignature))
        );
        // The checker's curve test agrees with `p256`'s.
        assert!(jwk::on_p256(
            &decode_base64url(RFC_7515_EC_X).unwrap_or_default(),
            &decode_base64url(RFC_7515_EC_Y).unwrap_or_default()
        ));
        assert!(!jwk::on_p256(
            &decode_base64url(RFC_7515_EC_X).unwrap_or_default(),
            &y
        ));
    }

    #[test]
    fn a_key_s_certificate_must_hold_the_same_key() {
        let with = |certificate: &str| {
            key(&[
                ("kty", Value::text("RSA")),
                ("n", Value::text(RFC_7515_RSA_N)),
                ("e", Value::text("AQAB")),
                ("kid", Value::text("rsa-1")),
                ("x5c", Value::list(vec![Value::text(certificate)])),
            ])
        };
        assert!(verify_jwt(RSA_GOOD, &[with(RSA_CERT)], None, Some("clinic"), NOW).is_ok());
        for wrong in [EVIL_CERT, "AAAA"] {
            assert_eq!(
                verify_jwt(RSA_GOOD, &[with(wrong)], None, Some("clinic"), NOW),
                Err(Fault::Broke(RuleKey::InvalidSignature))
            );
        }
        let der = crate::encoding::from_base64(RSA_CERT).expect("Base64");
        assert_eq!(
            crate::certificate::subject_key(&der),
            Some(SubjectKey::Rsa {
                n: decode_base64url(RFC_7515_RSA_N).unwrap_or_default(),
                e: vec![1, 0, 1],
            })
        );
        let evil = crate::encoding::from_base64(EVIL_CERT).expect("Base64");
        assert_eq!(
            crate::certificate::subject_key(&evil),
            Some(SubjectKey::Rsa {
                n: decode_base64url(EVIL_RSA_N).unwrap_or_default(),
                e: vec![1, 0, 1],
            })
        );
    }

    /// `verify_jwt(..., as: SignIn)` with `SignIn` an object of `email: Text`
    /// and `email_verified: Bool`.
    #[test]
    fn a_token_s_own_claims_read_as_the_application_s_type_or_fail_mismatch() {
        use jadpo_std::types::{FieldPlan, ShapeId, ShapePlan, ValidatorId};
        let field = |name: &str, validator: u32| FieldPlan {
            key: format!("field SignIn.{name}"),
            name: name.to_string(),
            validator: ValidatorId(validator),
            omittable: false,
            absent: None,
            none_left_out: false,
            section: None,
            wire: None,
        };
        let shapes = [crate::shape::Shape::new(&ShapePlan {
            ty: "SignIn".to_string(),
            open: false,
            variant: None,
            fields: vec![field("email", 0), field("email_verified", 1)],
        })];
        let validators = [
            crate::Validator::new(Some(jadpo_std::Builtin::Text), Vec::new()),
            crate::Validator::new(Some(jadpo_std::Builtin::Bool), Vec::new()),
        ];
        let table = [
            Codec::Scalar {
                builtin: jadpo_std::Builtin::Text,
                validator: ValidatorId(0),
                int_as_number: false,
            },
            Codec::Scalar {
                builtin: jadpo_std::Builtin::Bool,
                validator: ValidatorId(1),
                int_as_number: false,
            },
            Codec::Object {
                shape: ShapeId(0),
                fields: vec![CodecId(0), CodecId(1)],
                validator: None,
            },
        ];
        let codecs = Codecs {
            codecs: &table,
            validators: &validators,
            shapes: &shapes,
            coder: None,
        };
        let read = |token: &str| {
            verify_jwt_as(
                token,
                &provider_keys(),
                Some("https://id.example.com"),
                Some("clinic"),
                NOW,
                &codecs,
                CodecId(2),
            )
        };
        for token in [RSA_GOOD, EC_GOOD] {
            // `azp`, which `SignIn` doesn't name, is ignored.
            let Ok(Value::Record(sign_in)) = read(token) else {
                panic!("{token}");
            };
            assert_eq!(sign_in.get("email"), Some(&Value::text("ada@example.com")));
            assert_eq!(sign_in.get("email_verified"), Some(&Value::Bool(true)));
        }
        for (token, field, rule) in [
            (RSA_EMAIL_NOT_A_FLAG, "email_verified", "type"),
            (EC_EMAIL_NOT_A_FLAG, "email_verified", "type"),
            (RSA_NO_EMAIL, "email", "required"),
            (EC_NO_EMAIL, "email", "required"),
        ] {
            assert_eq!(
                read(token),
                Err(Refusal::Mismatch(Problem {
                    field: field.to_string(),
                    rule: rule.to_string(),
                })),
                "{token}"
            );
        }
        // Nothing is read as the type until the token holds.
        for (token, key) in [
            (RSA_TAMPERED, RuleKey::InvalidSignature),
            (EC_ALG_NONE, RuleKey::InvalidSignature),
            (RSA_EXPIRED, RuleKey::Expired),
            (EC_OTHER_AUDIENCE, RuleKey::WrongAudience),
        ] {
            assert_eq!(
                read(token),
                Err(Refusal::Token(Fault::Broke(key))),
                "{token}"
            );
        }
    }
}
