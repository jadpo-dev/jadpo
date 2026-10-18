//! `crypto.verify_jwt` (area 19, "Hashing and cryptography", chunk 5.5r):
//! a JWS in compact form (RFC 7515) carrying a JWT (RFC 7519), checked
//! against a set of public keys, each with its own algorithm.
//!
//! Every check follows the attacks the standards' security considerations
//! name (RFC 8725, "JSON Web Token Best Current Practices"):
//!
//! - **The algorithm is the key's, never the token's.** A `Jwk` holds an
//!   Ed25519 key (RFC 8037), so a token is checked with Ed25519 alone; a
//!   header naming `none`, `HS256` (the confusion of a public key for a
//!   shared secret) or anything else fails `invalid_signature` before any
//!   key is tried. A key's own `alg`, where it has one, must agree.
//! - **Keys come only from the call.** A header's `jwk`, `jku`, `x5u` and
//!   `x5c` are never read; its `kid` only narrows which of the given keys
//!   are tried. A header with `crit` names an extension nothing here
//!   understands, so it fails `invalid_signature` (RFC 7515 §4.1.11).
//! - **The signature first.** Nothing the payload says is read until a key
//!   has verified it, strictly (RFC 8032's checks, as `verify_signature`).
//! - **Times against the clock** with [`JWT_LEEWAY_SECONDS`]' leeway: `exp`
//!   is required and must be later than now, `nbf` and `iat` no later.
//! - **Issuer and audience** compared exactly, as RFC 7519 §4.1 compares a
//!   StringOrURI; a token with an audience is refused unless the call names
//!   one of them (RFC 7519 §4.1.3).
//! - **One reading of the JSON.** The header and the claims are each one
//!   JSON object with no member named twice, so no other reader can take
//!   a different value from the same token (RFC 7515 §5.2, RFC 7519 §7.2).
//!   A numeric date is a whole number of seconds.

use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;

use jadpo_std::RuleKey;
use jadpo_std::standard::security::{
    JWT_ALGORITHM, JWT_CLAIMS, JWT_LEEWAY_SECONDS, decode_base64url, is_jwt,
};
use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::Value as Json;

use crate::{Fault, Value};

/// Why a token is refused, as the call's `attempt` maps it.
fn refused(key: RuleKey) -> Fault {
    Fault::Broke(key)
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

/// One of the keys a call gives: a `Jwk`'s parts as a value holds them.
struct Key<'a> {
    x: &'a str,
    kid: Option<&'a str>,
    alg: Option<&'a str>,
}

/// A `Jwk` value's key, or `None` for one that isn't an Ed25519 public
/// key (which its type already ensures).
fn key_of(value: &Value) -> Option<Key<'_>> {
    let Value::Record(record) = value else {
        return None;
    };
    let text = |name: &str| match record.get(name) {
        Some(Value::Text(text)) => Some(&**text),
        _ => None,
    };
    if text("kty")? != "OKP" || text("crv")? != "Ed25519" {
        return None;
    }
    if text("use_").is_some_and(|usage| usage != "sig") {
        return None;
    }
    Some(Key {
        x: text("x")?,
        kid: text("kid"),
        alg: text("alg"),
    })
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
    if !is_jwt(token) {
        return Err(refused(RuleKey::Malformed));
    }
    let mut parts = token.split('.');
    let (Some(header), Some(payload), Some(signature)) = (parts.next(), parts.next(), parts.next())
    else {
        return Err(refused(RuleKey::Malformed));
    };
    let protected = object(header)?;
    let Some(Json::String(algorithm)) = protected.get("alg") else {
        return Err(refused(RuleKey::Malformed));
    };
    let kid = match protected.get("kid") {
        None => None,
        Some(Json::String(kid)) => Some(kid.as_str()),
        Some(_) => return Err(refused(RuleKey::Malformed)),
    };
    // A nested token's claims are another token, which this doesn't read.
    if protected
        .get("cty")
        .and_then(Json::as_str)
        .is_some_and(|content| content.eq_ignore_ascii_case("JWT"))
    {
        return Err(refused(RuleKey::Malformed));
    }
    // The key's algorithm, never the token's; and no extension this can't
    // check.
    if algorithm != JWT_ALGORITHM || protected.contains_key("crit") {
        return Err(refused(RuleKey::InvalidSignature));
    }
    let signed = format!("{header}.{payload}");
    let signature = decode_base64url(signature).ok_or(refused(RuleKey::Malformed))?;
    let signed_by_a_key = keys
        .iter()
        .filter_map(key_of)
        .filter(|key| kid.is_none_or(|kid| key.kid == Some(kid)))
        .filter(|key| key.alg.is_none_or(|alg| alg == JWT_ALGORITHM))
        .any(|key| crate::crypto::verify_signature(signed.as_bytes(), &signature, key.x));
    if !signed_by_a_key {
        return Err(refused(RuleKey::InvalidSignature));
    }
    claims(&object(payload)?, issuer, audience, now)
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
    let ordered = jadpo_std::standard::security::JWT_CLAIMS_FIELDS
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

    fn jwk(x: &str, kid: Option<&str>) -> Value {
        Value::record(
            "Jwk",
            vec![
                (Rc::from("kty"), Value::text("OKP")),
                (Rc::from("crv"), Value::text("Ed25519")),
                (Rc::from("x"), Value::text(x)),
                (Rc::from("kid"), kid.map_or(Value::None, Value::text)),
                (Rc::from("alg"), Value::None),
                (Rc::from("use_"), Value::None),
            ],
        )
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
        let mut other_alg = jwk(KEY, Some("2026-10"));
        if let Value::Record(record) = &mut other_alg {
            let record = Rc::make_mut(record);
            record.fields[4].1 = Value::text("ES256");
        }
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
}
