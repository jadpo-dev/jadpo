//! The `crypto` namespace (area 19, "Hashing and cryptography", question
//! 9): one construction per job and no algorithm choices, each on
//! RustCrypto's or dalek's audited, pure-Rust crates, so every target
//! gives the same results.
//!
//! | Job | Construction |
//! |---|---|
//! | `encrypt`, `decrypt` | XChaCha20-Poly1305 (draft-irtf-cfrg-xchacha), a fresh random 192-bit nonce each time |
//! | `hash_password`, `verify_password` | Argon2id (RFC 9106) at OWASP's parameters, kept as a PHC string |
//! | `hmac`, `verify_hmac` | HMAC-SHA-256 (RFC 2104, RFC 4231), checked in constant time |
//! | `sign`, `verify_signature` | Ed25519 (RFC 8032), verified strictly |
//!
//! A key is never a [`Value`]: an executor gives each operation the
//! [`KeyRing`] its `key: config.<name>` names, which nothing can read,
//! print or compare. A nonce and a salt are the invocation's random bytes
//! (a [`Source`]): the host's generator in a deployable, a test's seed in
//! a test, so a rerun repeats. A salt is recorded and replayed as
//! `random`'s bytes are; a nonce is secret randomness, never kept on a
//! recording's tape nor taken from a replayed one, so a replay never seals
//! with a nonce production used (chunk 5.8a).
//!
//! A sealed value is laid out as
//!
//! ```text
//! version (1) | algorithm (1) | key identity (4) | nonce (24) | ciphertext | tag (16)
//! ```
//!
//! with the first six bytes and the call's `context:` as the associated
//! data, so the header, the context and the ciphertext are all
//! authenticated: changing any of them, or opening it with another key,
//! fails `unreadable`. The key identity is the first four bytes of
//! HMAC-SHA-256 of a fixed label under the key, so `decrypt` knows which of
//! the current and the previous key sealed a value without trying both,
//! and learns nothing of the key from it. A later release that changes the
//! construction writes another version and keeps reading this one.

use std::fmt;

use argon2::{Algorithm, Argon2, Params, PasswordHasher, PasswordVerifier, Version};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hmac::{Hmac, Mac};
use jadpo_std::standard::security::{
    ARGON2_HASH_BYTES, ARGON2_LANES, ARGON2_MEMORY_KIB, ARGON2_PASSES, ARGON2_SALT_BYTES,
    PASSWORD_CHECK, read_phc,
};
use jadpo_std::{KEY_SIZE, RuleKey};
use sha2::Sha256;

use crate::random::Source;
use crate::{Fault, Op, Value};

type HmacSha256 = Hmac<Sha256>;

/// The sealed format's version, the first byte of every sealed value.
pub const SEALED_VERSION: u8 = 1;
/// The construction it names: XChaCha20-Poly1305.
pub const XCHACHA20_POLY1305: u8 = 1;
/// The bytes before the ciphertext: version, algorithm, key identity and
/// nonce.
const HEADER: usize = 2 + KEY_IDENTITY + NONCE;
/// What the associated data authenticates of the header: all but the
/// nonce, which the construction authenticates itself.
const BOUND_HEADER: usize = 2 + KEY_IDENTITY;
const KEY_IDENTITY: usize = 4;
const NONCE: usize = 24;
const TAG: usize = 16;
/// The label a key's identity is the HMAC of.
const IDENTITY_LABEL: &[u8] = b"jadpo encryption key identity v1";

/// An Ed25519 signature's length.
pub const SIGNATURE_BYTES: usize = 64;

/// One key setting's key, and the one it replaced while a deployment
/// rotates (`NAME_PREVIOUS`, area 10): what a `crypto` call's `key:` gives
/// the operation. It has no accessor, no equality and no printed form
/// beyond its name.
#[derive(Clone)]
pub struct KeyRing {
    current: Vec<u8>,
    previous: Option<Vec<u8>>,
}

impl fmt::Debug for KeyRing {
    /// Never the key.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyRing").finish_non_exhaustive()
    }
}

impl KeyRing {
    /// An `EncryptionKey`'s or a `PrivateKey`'s ring from its setting's
    /// text and its previous one's: each 32 bytes as unpadded Base64url,
    /// which the check before the app starts has already held them to;
    /// `None` for text that isn't one.
    pub fn from_key_text(current: &str, previous: Option<&str>) -> Option<KeyRing> {
        let previous = match previous {
            Some(text) => Some(jadpo_std::decode_key(text)?.to_vec()),
            None => None,
        };
        Some(KeyRing {
            current: jadpo_std::decode_key(current)?.to_vec(),
            previous,
        })
    }

    /// A `SigningKey`'s ring: its text's bytes are the HMAC key, so an
    /// outside system that shares the secret can check what the app signs
    /// (decided by the lead session for chunk 5.5q2).
    pub fn from_signing_text(current: &str, previous: Option<&str>) -> KeyRing {
        KeyRing {
            current: current.as_bytes().to_vec(),
            previous: previous.map(|text| text.as_bytes().to_vec()),
        }
    }

    /// The current key and then the previous one.
    fn each(&self) -> impl Iterator<Item = &[u8]> {
        std::iter::once(self.current.as_slice()).chain(self.previous.as_deref())
    }

    /// The current key as the 32 bytes encryption and signing take.
    fn current_32(&self) -> Result<&[u8; KEY_SIZE], Fault> {
        self.current.as_slice().try_into().map_err(|_| Fault::Shape)
    }
}

fn fill(source: &mut Source<'_>, bytes: &mut [u8]) -> Result<(), Fault> {
    if source(bytes) {
        Ok(())
    } else {
        Err(Fault::NoRandomness)
    }
}

/// HMAC-SHA-256 of `parts`, one after another, under `key`. HMAC takes a
/// key of any length, so this never fails in practice.
fn hmac_sha256(key: &[u8], parts: &[&[u8]]) -> Result<[u8; 32], Fault> {
    let mut mac = <HmacSha256 as KeyInit>::new_from_slice(key).map_err(|_| Fault::Shape)?;
    for part in parts {
        mac.update(part);
    }
    Ok(mac.finalize().into_bytes().into())
}

/// The identity a sealed value carries of the key that sealed it.
fn key_identity(key: &[u8]) -> Result<[u8; KEY_IDENTITY], Fault> {
    let mac = hmac_sha256(key, &[IDENTITY_LABEL])?;
    let mut out = [0u8; KEY_IDENTITY];
    out.copy_from_slice(&mac[..KEY_IDENTITY]);
    Ok(out)
}

/// The associated data: the header's version, algorithm and key identity,
/// then the context.
fn associated(bound: &[u8], context: &str) -> Vec<u8> {
    let mut aad = Vec::with_capacity(bound.len() + context.len());
    aad.extend_from_slice(bound);
    aad.extend_from_slice(context.as_bytes());
    aad
}

/// `crypto.encrypt(value, key:, context:)`: `value` sealed under the
/// current key with a fresh random nonce, bound to `context`.
pub fn encrypt(
    value: &[u8],
    keys: &KeyRing,
    context: &str,
    source: &mut Source<'_>,
) -> Result<Vec<u8>, Fault> {
    let key = keys.current_32()?;
    let mut sealed = Vec::with_capacity(HEADER + value.len() + TAG);
    sealed.push(SEALED_VERSION);
    sealed.push(XCHACHA20_POLY1305);
    sealed.extend_from_slice(&key_identity(key)?);
    let aad = associated(&sealed, context);
    let mut nonce = [0u8; NONCE];
    fill(source, &mut nonce)?;
    sealed.extend_from_slice(&nonce);
    let cipher = XChaCha20Poly1305::new(key.into());
    let body = cipher
        .encrypt(
            &XNonce::from(nonce),
            Payload {
                msg: value,
                aad: &aad,
            },
        )
        .map_err(|_| Fault::Shape)?;
    sealed.extend_from_slice(&body);
    Ok(sealed)
}

/// `crypto.decrypt(sealed, key:, context:)`: the value, opened with
/// whichever of the current and the previous key sealed it; `unreadable`
/// for a value neither sealed for this context, an altered one, or bytes
/// that aren't a sealed value at all, never saying which.
pub fn decrypt(sealed: &[u8], keys: &KeyRing, context: &str) -> Result<Vec<u8>, Fault> {
    let unreadable = Fault::Broke(RuleKey::Unreadable);
    let (Some(bound), Some(nonce), Some(body)) = (
        sealed.get(..BOUND_HEADER),
        sealed.get(BOUND_HEADER..HEADER),
        sealed.get(HEADER..),
    ) else {
        return Err(unreadable);
    };
    let [version, algorithm, identity @ ..] = bound else {
        return Err(unreadable);
    };
    if *version != SEALED_VERSION || *algorithm != XCHACHA20_POLY1305 || body.len() < TAG {
        return Err(unreadable);
    }
    let aad = associated(bound, context);
    let nonce = XNonce::try_from(nonce).map_err(|_| Fault::Shape)?;
    for key in keys.each() {
        let Ok(key) = <&[u8; KEY_SIZE]>::try_from(key) else {
            continue;
        };
        // Only a key whose identity the value carries; two whose identities
        // collide are both tried.
        if key_identity(key)? != identity {
            continue;
        }
        let cipher = XChaCha20Poly1305::new(key.into());
        if let Ok(value) = cipher.decrypt(
            &nonce,
            Payload {
                msg: body,
                aad: &aad,
            },
        ) {
            return Ok(value);
        }
    }
    Err(unreadable)
}

/// Argon2id at the release's parameters.
fn argon2() -> Result<Argon2<'static>, Fault> {
    let params = Params::new(
        ARGON2_MEMORY_KIB,
        ARGON2_PASSES,
        ARGON2_LANES,
        Some(ARGON2_HASH_BYTES),
    )
    .map_err(|_| Fault::Shape)?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

/// `crypto.hash_password(password)`: the PHC string of Argon2id at the
/// release's parameters, with a fresh random 16-byte salt.
pub fn hash_password(password: &str, source: &mut Source<'_>) -> Result<String, Fault> {
    let mut salt = [0u8; ARGON2_SALT_BYTES];
    fill(source, &mut salt)?;
    let hash = argon2()?
        .hash_password_with_salt(password.as_bytes(), &salt)
        .map_err(|_| Fault::Shape)?;
    Ok(hash.to_string())
}

/// What checking a password says: `PasswordCheck`'s variant name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasswordCheck {
    Wrong,
    Correct,
    CorrectOutdated,
}

impl PasswordCheck {
    /// Its place among `PasswordCheck`'s variants, and its name.
    pub fn variant(self) -> (u32, &'static str) {
        match self {
            PasswordCheck::Wrong => (0, "wrong"),
            PasswordCheck::Correct => (1, "correct"),
            PasswordCheck::CorrectOutdated => (2, "correct_outdated"),
        }
    }

    pub fn value(self) -> Value {
        let (ordinal, name) = self.variant();
        Value::variant(PASSWORD_CHECK, ordinal, name, Vec::new())
    }
}

/// `crypto.verify_password(password, hash)`: `correct` when `password` is
/// the one `hash` was made from, compared in constant time, and
/// `correct_outdated` when it is but `hash` was made with other
/// parameters; `wrong` otherwise, a hash that isn't one included.
pub fn verify_password(password: &str, hash: &str) -> PasswordCheck {
    // A `PasswordHash` was checked when it was built; read again here, so
    // its costs are bounded whatever reached this.
    let Some(phc) = read_phc(hash) else {
        return PasswordCheck::Wrong;
    };
    let Ok(parsed) = argon2::PasswordHash::new(hash) else {
        return PasswordCheck::Wrong;
    };
    // The parameters come from the hash; the algorithm's object only
    // supplies the construction.
    let Ok(argon2) = argon2() else {
        return PasswordCheck::Wrong;
    };
    match argon2.verify_password(password.as_bytes(), &parsed) {
        Ok(()) if phc.is_current() => PasswordCheck::Correct,
        Ok(()) => PasswordCheck::CorrectOutdated,
        Err(_) => PasswordCheck::Wrong,
    }
}

/// `crypto.hmac(value, key:)`: HMAC-SHA-256 under the current key.
pub fn hmac(value: &[u8], keys: &KeyRing) -> Result<[u8; 32], Fault> {
    hmac_sha256(&keys.current, &[value])
}

/// `crypto.verify_hmac(value, mac, key:)`: whether `mac` is `value`'s
/// HMAC-SHA-256 under the current or the previous key, each compared in
/// constant time, both always computed so the time says nothing of which.
pub fn verify_hmac(value: &[u8], mac: &[u8], keys: &KeyRing) -> bool {
    let mut matched = false;
    for key in keys.each() {
        let Ok(check) = <HmacSha256 as KeyInit>::new_from_slice(key) else {
            continue;
        };
        matched |= check.chain_update(value).verify_slice(mac).is_ok();
    }
    matched
}

/// `crypto.sign(value, key:)`: the 64-byte Ed25519 signature of `value`
/// under the `PrivateKey`'s seed.
pub fn sign(value: &[u8], keys: &KeyRing) -> Result<[u8; SIGNATURE_BYTES], Fault> {
    let key = SigningKey::from_bytes(keys.current_32()?);
    Ok(key.sign(value).to_bytes())
}

/// `crypto.public_key(key:)`: the Ed25519 public key of the `PrivateKey`
/// setting's seed (RFC 8032, section 5.1.5), as a `PublicKey`'s unpadded
/// Base64url, which the app may publish so others can check what it
/// signs.
pub fn public_key(keys: &KeyRing) -> Result<String, Fault> {
    let key = SigningKey::from_bytes(keys.current_32()?);
    Ok(crate::encoding::to_base64url(
        key.verifying_key().as_bytes(),
    ))
}

/// `crypto.verify_signature(value, signature, key)`: whether `signature`
/// is `value`'s under the Ed25519 public key `public_key` (unpadded
/// Base64url), verified strictly (RFC 8032's checks, with weak keys and
/// non-canonical signatures refused); `false` for a key that isn't a point
/// on the curve and for a signature of the wrong length.
pub fn verify_signature(value: &[u8], signature: &[u8], public_key: &str) -> bool {
    let Some(key) = jadpo_std::decode_key(public_key) else {
        return false;
    };
    let Ok(key) = VerifyingKey::from_bytes(&key) else {
        return false;
    };
    let Ok(signature) = <[u8; SIGNATURE_BYTES]>::try_from(signature) else {
        return false;
    };
    key.verify_strict(value, &Signature::from_bytes(&signature))
        .is_ok()
}

/// A `Text`'s UTF-8 or a value built on `Bytes`.
fn bytes_of(value: &Value) -> Result<&[u8], Fault> {
    match value {
        Value::Bytes(bytes) => Ok(bytes),
        Value::Text(text) => Ok(text.as_bytes()),
        _ => Err(Fault::Shape),
    }
}

fn text_of(value: &Value) -> Result<&str, Fault> {
    match value {
        Value::Text(text) => Ok(text),
        _ => Err(Fault::Shape),
    }
}

/// The text of a secret value an operation is made to take: a `Password`
/// to hash or check, a `Jwt` to verify (chunk 5.5r2).
fn secret_of(value: &Value) -> Result<&str, Fault> {
    match value {
        Value::Secret(secret) => Ok(secret.expose()),
        Value::Text(text) => Ok(text),
        _ => Err(Fault::Shape),
    }
}

fn bytes_value(bytes: &[u8]) -> Value {
    Value::Bytes(std::rc::Rc::from(bytes))
}

/// Whether `op` is one of `crypto`'s, which each executor runs through
/// [`run`] with the key its call names.
pub fn is_crypto(op: Op) -> bool {
    matches!(
        op,
        Op::CryptoDecrypt
            | Op::CryptoEncrypt
            | Op::CryptoHashPassword
            | Op::CryptoHmac
            | Op::CryptoPublicKey
            | Op::CryptoSign
            | Op::CryptoVerifyHmac
            | Op::CryptoVerifyJwt
            | Op::CryptoVerifyPassword
            | Op::CryptoVerifySignature
    )
}

/// Runs `crypto` operation `op` on `values`, its arguments in its
/// parameters' order, with `keys` the ring its `key:` names (the value in
/// that place is only the executor's handle, never the key), `source` the
/// invocation's random bytes and `now` its clock, an instant's
/// milliseconds, which `verify_jwt` checks a token's times against.
pub fn run(
    op: Op,
    values: &[Value],
    keys: Option<&KeyRing>,
    source: &mut Source<'_>,
    now: i64,
) -> Result<Value, Fault> {
    let keys = || keys.ok_or(Fault::Shape);
    match (op, values) {
        (Op::CryptoEncrypt, [value, _, context]) => Ok(bytes_value(&encrypt(
            bytes_of(value)?,
            keys()?,
            text_of(context)?,
            source,
        )?)),
        (Op::CryptoDecrypt, [Value::Bytes(sealed), _, context]) => {
            Ok(bytes_value(&decrypt(sealed, keys()?, text_of(context)?)?))
        }
        (Op::CryptoHashPassword, [password]) => {
            Ok(Value::text(&hash_password(secret_of(password)?, source)?))
        }
        (Op::CryptoVerifyPassword, [password, hash]) => {
            Ok(verify_password(secret_of(password)?, text_of(hash)?).value())
        }
        (Op::CryptoHmac, [value, _]) => Ok(bytes_value(&hmac(bytes_of(value)?, keys()?)?)),
        (Op::CryptoVerifyHmac, [value, Value::Bytes(mac), _]) => {
            Ok(Value::Bool(verify_hmac(bytes_of(value)?, mac, keys()?)))
        }
        (Op::CryptoPublicKey, [_]) => Ok(Value::text(&public_key(keys()?)?)),
        (Op::CryptoSign, [value, _]) => Ok(bytes_value(&sign(bytes_of(value)?, keys()?)?)),
        (Op::CryptoVerifySignature, [value, Value::Bytes(signature), key]) => Ok(Value::Bool(
            verify_signature(bytes_of(value)?, signature, text_of(key)?),
        )),
        // Without `as:`, whose claims of the application's own type each
        // executor reads with the plan's codecs ([`crate::jwt::verify_jwt_as`]).
        (Op::CryptoVerifyJwt, [token, Value::List(keys), issuer, audience, ..]) => {
            let option = |value: &Value| match value {
                Value::None => Ok(None),
                Value::Text(text) => Ok(Some(text.to_string())),
                _ => Err(Fault::Shape),
            };
            crate::jwt::verify_jwt(
                secret_of(token)?,
                keys,
                option(issuer)?.as_deref(),
                option(audience)?.as_deref(),
                now,
            )
        }
        _ => Err(Fault::Shape),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use chacha20poly1305::ChaCha20Poly1305;

    use super::*;
    use crate::encoding::{from_hex, to_hex};

    fn hex(text: &str) -> Vec<u8> {
        from_hex(&text.split_whitespace().collect::<String>()).expect("hexadecimal")
    }

    /// A seeded stand-in for the host's generator: a counter through
    /// SHA-256, so draws differ and a run repeats.
    fn seeded(seed: u8) -> impl FnMut(&mut [u8]) -> bool {
        let mut counter = 0u64;
        move |out: &mut [u8]| {
            for chunk in out.chunks_mut(32) {
                counter += 1;
                let mut input = vec![seed];
                input.extend_from_slice(&counter.to_be_bytes());
                let block = crate::hash::sha256(&input);
                chunk.copy_from_slice(&block[..chunk.len()]);
            }
            true
        }
    }

    const KEY_TEXT: &str = "gIGCg4SFhoeIiYqLjI2Oj5CRkpOUlZaXmJmam5ydnp8";
    const OTHER_KEY_TEXT: &str = "nWGxne_9WmC6hEr0kuwsxERJxWl7MmkZcDusAxyuf2A";

    fn ring(current: &str, previous: Option<&str>) -> KeyRing {
        KeyRing::from_key_text(current, previous).expect("keys")
    }

    // --- known answers ---------------------------------------------------

    const SUNSCREEN: &[u8] = b"Ladies and Gentlemen of the class of '99: \
        If I could offer you only one tip for the future, sunscreen would be it.";

    #[test]
    fn chacha20_poly1305_gives_rfc_8439_s_answer() {
        // RFC 8439, section 2.8.2.
        let key = hex("808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f");
        let nonce = hex("070000004041424344454647");
        let aad = hex("50515253c0c1c2c3c4c5c6c7");
        let cipher = ChaCha20Poly1305::new_from_slice(&key).expect("a key");
        let sealed = cipher
            .encrypt(
                nonce.as_slice().try_into().expect("a nonce"),
                Payload {
                    msg: SUNSCREEN,
                    aad: &aad,
                },
            )
            .expect("sealed");
        assert_eq!(
            to_hex(&sealed),
            "d31a8d34648e60db7b86afbc53ef7ec2a4aded51296e08fea9e2b5a736ee62d63dbea45e8ca96712\
             82fafb69da92728b1a71de0a9e060b2905d6a5b67ecd3b3692ddbd7f2d778b8c9803aee328091b58\
             fab324e4fad675945585808b4831d7bc3ff4def08e4b7a9de576d26586cec64b6116\
             1ae10b594f09e26a7e902ecbd0600691"
        );
    }

    #[test]
    fn xchacha20_poly1305_gives_the_draft_s_answer() {
        // draft-irtf-cfrg-xchacha-03, appendix A.3.1.
        let key = hex("808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f");
        let nonce = hex("404142434445464748494a4b4c4d4e4f5051525354555657");
        let aad = hex("50515253c0c1c2c3c4c5c6c7");
        let cipher = XChaCha20Poly1305::new_from_slice(&key).expect("a key");
        let nonce = XNonce::try_from(nonce.as_slice()).expect("a nonce");
        let sealed = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: SUNSCREEN,
                    aad: &aad,
                },
            )
            .expect("sealed");
        assert_eq!(
            to_hex(&sealed),
            "bd6d179d3e83d43b9576579493c0e939572a1700252bfaccbed2902c21396cbb731c7f1b0b4aa644\
             0bf3a82f4eda7e39ae64c6708c54c216cb96b72e1213b4522f8c9ba40db5d945b11b69b982c1bb9e\
             3f3fac2bc369488f76b2383565d3fff921f9664c97637da9768812f615c68b13b52e\
             c0875924c1c7987947deafd8780acf49"
        );
        // The same key as an `EncryptionKey` setting's text.
        assert_eq!(
            jadpo_std::decode_key(KEY_TEXT).map(|k| k.to_vec()),
            Some(key)
        );
    }

    #[test]
    fn hmac_sha256_gives_rfc_4231_s_answers() {
        let long_key = vec![0xaa; 131];
        let cases: [(&[u8], &[u8], &str); 7] = [
            (
                &[0x0b; 20],
                b"Hi There",
                "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7",
            ),
            (
                b"Jefe",
                b"what do ya want for nothing?",
                "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843",
            ),
            (
                &[0xaa; 20],
                &[0xdd; 50],
                "773ea91e36800e46854db8ebd09181a72959098b3ef8c122d9635514ced565fe",
            ),
            (
                &hex("0102030405060708090a0b0c0d0e0f10111213141516171819"),
                &[0xcd; 50],
                "82558a389a443c0ea4cc819899f2083a85f0faa3e578f8077a2e3ff46729665b",
            ),
            // Test case 5 checks the first 128 bits only.
            (
                &[0x0c; 20],
                b"Test With Truncation",
                "a3b6167473100ee06e0c796c2955552b",
            ),
            (
                &long_key,
                b"Test Using Larger Than Block-Size Key - Hash Key First",
                "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54",
            ),
            (
                &long_key,
                b"This is a test using a larger than block-size key and a larger than block-size data. The key needs to be hashed before being used by the HMAC algorithm.",
                "9b09ffa71b942fcb27635fbcd5b0e944bfdc63644f0713938a7f51535c3a35e2",
            ),
        ];
        for (number, (key, data, expected)) in cases.iter().enumerate() {
            let keys = KeyRing {
                current: key.to_vec(),
                previous: None,
            };
            let mac = hmac(data, &keys).expect("a MAC");
            assert!(
                to_hex(&mac).starts_with(expected),
                "RFC 4231 test case {}",
                number + 1
            );
            if expected.len() == 64 {
                assert!(verify_hmac(data, &mac, &keys));
            }
        }
    }

    #[test]
    fn ed25519_gives_rfc_8032_s_answers() {
        // RFC 8032, section 7.1: tests 1, 2 and 3.
        let cases = [
            (
                "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
                "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a",
                "",
                "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b",
            ),
            (
                "4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb",
                "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c",
                "72",
                "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00",
            ),
            (
                "c5aa8df43f9f837bedb7442f31dcb7b166d38535076f094b85ce3a2e0b4458f7",
                "fc51cd8e6218a1a38da47ed00230f0580816ed13ba3303ac5deb911548908025",
                "af82",
                "6291d657deec24024827e69c3abe01a30ce548a284743a445e3680d7db5ac3ac18ff9b538d16f290ae67f760984dc6594a7c15e9716ed28dc027beceea1ec40a",
            ),
        ];
        for (secret, public, message, signature) in cases {
            let seed = crate::encoding::to_base64url(&hex(secret));
            let public = crate::encoding::to_base64url(&hex(public));
            let message = hex(message);
            let keys = ring(&seed, None);
            assert_eq!(public_key(&keys).as_deref(), Ok(public.as_str()));
            let signed = sign(&message, &keys).expect("signed");
            assert_eq!(to_hex(&signed), signature);
            assert!(verify_signature(&message, &signed, &public));
            // Another message, a changed signature, a short one, and
            // another key all fail.
            assert!(!verify_signature(b"another", &signed, &public));
            let mut changed = signed;
            changed[10] ^= 1;
            assert!(!verify_signature(&message, &changed, &public));
            assert!(!verify_signature(&message, &signed[..63], &public));
            assert!(!verify_signature(
                &message,
                &signed,
                "11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURp"
            ));
        }
    }

    #[test]
    fn argon2id_gives_rfc_9106_s_answer() {
        // RFC 9106, section 5.3: Argon2id with a secret and associated
        // data, which `crypto` never uses, so the construction itself is
        // checked against the reference.
        let params = argon2::ParamsBuilder::new()
            .m_cost(32)
            .t_cost(3)
            .p_cost(4)
            .data(argon2::AssociatedData::new(&[0x04; 12]).expect("data"))
            .build()
            .expect("parameters");
        let argon2 =
            Argon2::new_with_secret(&[0x03; 8], Algorithm::Argon2id, Version::V0x13, params)
                .expect("Argon2id");
        let mut out = [0u8; 32];
        argon2
            .hash_password_into(&[0x01; 32], &[0x02; 16], &mut out)
            .expect("hashed");
        assert_eq!(
            to_hex(&out),
            "0d640df58d78766c08c037a34a8b53c9d01ef0452d75b65eb52520e96b01e659"
        );
    }

    #[test]
    fn a_password_hash_is_openssl_s_argon2id_at_owasp_s_parameters() {
        // OpenSSL 3.6's Argon2id of the password with the salt 00..0f at
        // 19456 KiB, 2 passes and 1 lane, as the PHC string writes it.
        let mut salt = (0u8..16).collect::<Vec<_>>().into_iter();
        let mut source = |out: &mut [u8]| {
            for byte in out.iter_mut() {
                *byte = salt.next().unwrap_or(0);
            }
            true
        };
        let hash = hash_password("correct horse battery staple", &mut source).expect("hashed");
        assert_eq!(
            hash,
            "$argon2id$v=19$m=19456,t=2,p=1$AAECAwQFBgcICQoLDA0ODw$gYJZtjEAJqjg26xdLmknq8/bB7MiWPrE9hsYuA+SkIU"
        );
        assert!(jadpo_std::standard::security::is_password_hash(&hash));
    }

    // --- passwords ------------------------------------------------------

    #[test]
    fn a_password_checks_correct_wrong_or_outdated() {
        let mut source = seeded(1);
        let hash = hash_password("correct horse battery staple", &mut source).expect("hashed");
        assert_eq!(
            verify_password("correct horse battery staple", &hash),
            PasswordCheck::Correct
        );
        assert_eq!(
            verify_password("correct horse battery stapler", &hash),
            PasswordCheck::Wrong
        );
        assert_eq!(verify_password("", &hash), PasswordCheck::Wrong);
        // Two hashes of one password differ by their salts.
        let again = hash_password("correct horse battery staple", &mut source).expect("hashed");
        assert_ne!(hash, again);
        // A hash made with older parameters (OpenSSL's, at 4 MiB and 3
        // passes) is correct but outdated.
        let older = "$argon2id$v=19$m=4096,t=3,p=1$AAECAwQFBgcICQoLDA0ODw$VTsSHiMxkq48zEo8LUSymjr7JjaRSgsxGX2Z8HQE9Cw";
        assert_eq!(
            verify_password("correct horse battery staple", older),
            PasswordCheck::CorrectOutdated
        );
        assert_eq!(verify_password("Tr0ub4dor&3", older), PasswordCheck::Wrong);
        // Text that isn't a hash, or a hash past the bounds, is wrong.
        assert_eq!(
            verify_password(
                "correct horse battery staple",
                "correct horse battery staple"
            ),
            PasswordCheck::Wrong
        );
        assert_eq!(
            verify_password(
                "x",
                "$argon2id$v=19$m=4194304,t=2,p=1$AAECAwQFBgcICQoLDA0ODw$gYJZtjEAJqjg26xdLmknq8/bB7MiWPrE9hsYuA+SkIU"
            ),
            PasswordCheck::Wrong
        );
        // Without randomness there is no salt, and no weaker one.
        assert_eq!(
            hash_password("x", &mut |_: &mut [u8]| false),
            Err(Fault::NoRandomness)
        );
    }

    // --- encryption -----------------------------------------------------

    #[test]
    fn a_sealed_value_opens_with_its_key_and_context_only() {
        let keys = ring(KEY_TEXT, None);
        let mut source = seeded(2);
        let sealed = encrypt(b"clinical note", &keys, "note 42", &mut source).expect("sealed");
        assert_eq!(sealed.len(), HEADER + 13 + TAG);
        assert_eq!(&sealed[..2], &[SEALED_VERSION, XCHACHA20_POLY1305]);
        assert_eq!(
            decrypt(&sealed, &keys, "note 42").expect("opened"),
            b"clinical note"
        );
        let unreadable = Err(Fault::Broke(RuleKey::Unreadable));
        // Another context: moved to another row.
        assert_eq!(decrypt(&sealed, &keys, "note 43"), unreadable);
        assert_eq!(decrypt(&sealed, &keys, ""), unreadable);
        // Another key.
        assert_eq!(
            decrypt(&sealed, &ring(OTHER_KEY_TEXT, None), "note 42"),
            unreadable
        );
        // Every byte altered in turn: the header, the nonce, the
        // ciphertext and the tag.
        for at in 0..sealed.len() {
            let mut altered = sealed.clone();
            altered[at] ^= 0x01;
            assert_eq!(decrypt(&altered, &keys, "note 42"), unreadable, "byte {at}");
        }
        // Cut short, lengthened, empty.
        assert_eq!(
            decrypt(&sealed[..sealed.len() - 1], &keys, "note 42"),
            unreadable
        );
        let mut longer = sealed.clone();
        longer.push(0);
        assert_eq!(decrypt(&longer, &keys, "note 42"), unreadable);
        assert_eq!(decrypt(&[], &keys, "note 42"), unreadable);
        assert_eq!(
            decrypt(&sealed[..HEADER + TAG - 1], &keys, "note 42"),
            unreadable
        );
        // Nothing sealed opens empty but as nothing.
        let empty = encrypt(b"", &keys, "note 42", &mut source).expect("sealed");
        assert_eq!(decrypt(&empty, &keys, "note 42").expect("opened"), b"");
    }

    #[test]
    fn a_rotated_key_still_opens_what_the_previous_key_sealed() {
        let old = ring(OTHER_KEY_TEXT, None);
        let rotated = ring(KEY_TEXT, Some(OTHER_KEY_TEXT));
        let mut source = seeded(3);
        let sealed_before = encrypt(b"before", &old, "row", &mut source).expect("sealed");
        let sealed_after = encrypt(b"after", &rotated, "row", &mut source).expect("sealed");
        assert_eq!(
            decrypt(&sealed_before, &rotated, "row").expect("opened"),
            b"before"
        );
        assert_eq!(
            decrypt(&sealed_after, &rotated, "row").expect("opened"),
            b"after"
        );
        // New values are sealed with the current key alone.
        assert_eq!(
            decrypt(&sealed_after, &old, "row"),
            Err(Fault::Broke(RuleKey::Unreadable))
        );
        assert_eq!(
            &sealed_after[2..6],
            &key_identity(&rotated.current).expect("an identity")
        );
        // Once the previous key is removed, what it sealed is unreadable.
        assert_eq!(
            decrypt(&sealed_before, &ring(KEY_TEXT, None), "row"),
            Err(Fault::Broke(RuleKey::Unreadable))
        );
    }

    #[test]
    fn nonces_never_repeat_across_many_encryptions_with_one_key() {
        let keys = ring(KEY_TEXT, None);
        let mut source = seeded(4);
        let mut nonces = BTreeSet::new();
        let mut sealed_values = BTreeSet::new();
        for _ in 0..10_000 {
            let sealed = encrypt(b"same value", &keys, "same row", &mut source).expect("sealed");
            nonces.insert(sealed[BOUND_HEADER..HEADER].to_vec());
            sealed_values.insert(sealed);
        }
        assert_eq!(nonces.len(), 10_000);
        assert_eq!(sealed_values.len(), 10_000);
        // Without randomness there is no nonce, and no weaker one.
        assert_eq!(
            encrypt(b"x", &keys, "row", &mut |_: &mut [u8]| false),
            Err(Fault::NoRandomness)
        );
    }

    // --- HMAC -----------------------------------------------------------

    #[test]
    fn an_hmac_uses_the_signing_key_s_text_and_accepts_the_previous_key() {
        let key = "test-signing-key-0123456789abcdef0123456789abcdef";
        let old = "test-signing-key-fedcba9876543210fedcba9876543210";
        let keys = KeyRing::from_signing_text(key, None);
        let mac = hmac(b"{\"paid\":true}", &keys).expect("a MAC");
        // What any HMAC-SHA-256 gives with the text's bytes as the key, as
        // a provider sharing the secret computes it.
        assert_eq!(
            Ok(mac),
            hmac_sha256(key.as_bytes(), &[b"{\"paid\":", b"true}"])
        );
        assert!(verify_hmac(b"{\"paid\":true}", &mac, &keys));
        assert!(!verify_hmac(b"{\"paid\":false}", &mac, &keys));
        assert!(!verify_hmac(b"{\"paid\":true}", &mac[..31], &keys));
        assert!(!verify_hmac(b"{\"paid\":true}", &[], &keys));
        let mut altered = mac;
        altered[31] ^= 0x80;
        assert!(!verify_hmac(b"{\"paid\":true}", &altered, &keys));
        // Signed with the old key: accepted while it is the previous one.
        let old_mac = hmac(b"body", &KeyRing::from_signing_text(old, None)).expect("a MAC");
        assert!(verify_hmac(
            b"body",
            &old_mac,
            &KeyRing::from_signing_text(key, Some(old))
        ));
        assert!(!verify_hmac(b"body", &old_mac, &keys));
    }

    #[test]
    fn a_key_ring_never_shows_its_keys() {
        let keys = ring(KEY_TEXT, Some(OTHER_KEY_TEXT));
        let shown = format!("{keys:?}");
        assert_eq!(shown, "KeyRing { .. }");
        assert!(KeyRing::from_key_text("not a key", None).is_none());
        assert!(KeyRing::from_key_text(KEY_TEXT, Some("not a key")).is_none());
    }

    #[test]
    fn run_takes_the_ring_beside_the_values() {
        let keys = ring(KEY_TEXT, None);
        let mut source = seeded(5);
        let handle = Value::Int(0);
        let sealed = run(
            Op::CryptoEncrypt,
            &[Value::text("note"), handle.clone(), Value::text("row")],
            Some(&keys),
            &mut source,
            0,
        )
        .expect("sealed");
        let opened = run(
            Op::CryptoDecrypt,
            &[sealed.clone(), handle.clone(), Value::text("row")],
            Some(&keys),
            &mut source,
            0,
        )
        .expect("opened");
        assert_eq!(opened, Value::Bytes(std::rc::Rc::from(&b"note"[..])));
        assert_eq!(
            run(
                Op::CryptoDecrypt,
                &[sealed, handle.clone(), Value::text("other")],
                Some(&keys),
                &mut source,
                0,
            ),
            Err(Fault::Broke(RuleKey::Unreadable))
        );
        // A key-taking operation with no ring is a defect, never a
        // keyless result.
        assert_eq!(
            run(
                Op::CryptoHmac,
                &[Value::text("x"), handle],
                None,
                &mut source,
                0,
            ),
            Err(Fault::Shape)
        );
        let check = run(
            Op::CryptoVerifyPassword,
            &[Value::text("x"), Value::text("not a hash")],
            None,
            &mut source,
            0,
        )
        .expect("checked");
        assert_eq!(check, PasswordCheck::Wrong.value());
    }
}
