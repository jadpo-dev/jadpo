//! A software authenticator for tests (area 8, "Passkeys"; chunk 5.5z):
//! what a browser and a device do between a passkey route's options and
//! the answer posted back, so a test drives the generated routes for real.
//! Built only with the `test-authenticator` feature, which no deployable
//! turns on.
//!
//! It makes ES256 passkeys that are discoverable, verify the person and
//! carry no attestation (`fmt: "none"`), as `navigator.credentials.create`
//! gives them, and answers `navigator.credentials.get` with one: its
//! `clientDataJSON` names the origin the authenticator was made for, so a
//! test can answer from the wrong site, and its counter can be set, so a
//! test can send one that went backwards.

use std::collections::BTreeMap;

use openssl::bn::{BigNum, BigNumContext};
use openssl::ec::{EcGroup, EcKey, EcPoint};
use openssl::ecdsa::EcdsaSig;
use openssl::nid::Nid;
use openssl::pkey::Private;
use openssl::sha::sha256;
use serde_cbor_2::Value as Cbor;
use serde_json::{Value, json};

/// One passkey the authenticator holds.
#[derive(Debug)]
struct Held {
    credential_id: Vec<u8>,
    rp_id: String,
    user_handle: Vec<u8>,
    key: EcKey<Private>,
    sign_count: u32,
}

/// A device holding passkeys, in a browser at `origin`.
#[derive(Debug)]
pub struct TestAuthenticator {
    origin: String,
    held: Vec<Held>,
    /// Whether each signature counts up; a synced passkey sends `0`.
    counts: bool,
    /// What its passkeys' keys and ids are drawn from when they are the
    /// same every run (chunk 8.1c); `None` draws them at random, as a
    /// device does.
    seed: Option<u64>,
}

impl TestAuthenticator {
    /// A device in a browser at `origin`, whose passkeys count their uses.
    pub fn new(origin: &str) -> TestAuthenticator {
        TestAuthenticator {
            origin: origin.to_string(),
            held: Vec::new(),
            counts: true,
            seed: None,
        }
    }

    /// A device as [`TestAuthenticator::new`] makes one, whose passkeys'
    /// keys and ids come from `seed` and how many it made before, so a
    /// test that makes them reads the same every run (chunk 8.1c): only
    /// what it signs varies. Devices given different seeds make different
    /// passkeys.
    pub fn seeded(origin: &str, seed: u64) -> TestAuthenticator {
        TestAuthenticator {
            seed: Some(seed),
            ..TestAuthenticator::new(origin)
        }
    }

    /// The `n`th passkey's private key and id, from the device's seed, or
    /// at random without one.
    fn new_passkey(&self, group: &EcGroup) -> Result<(EcKey<Private>, Vec<u8>), String> {
        let Some(seed) = self.seed else {
            let mut credential_id = vec![0u8; 32];
            openssl::rand::rand_bytes(&mut credential_id).map_err(|e| e.to_string())?;
            return Ok((
                EcKey::generate(group).map_err(|e| e.to_string())?,
                credential_id,
            ));
        };
        let drawn = |purpose: &[u8]| {
            let mut input = b"jadpo test authenticator ".to_vec();
            input.extend(seed.to_be_bytes());
            input.extend((self.held.len() as u64).to_be_bytes());
            input.extend(purpose);
            sha256(&input)
        };
        let mut context = BigNumContext::new().map_err(|e| e.to_string())?;
        let mut order = BigNum::new().map_err(|e| e.to_string())?;
        group
            .order(&mut order, &mut context)
            .map_err(|e| e.to_string())?;
        // A scalar from 1 to the order less one.
        let one = BigNum::from_u32(1).map_err(|e| e.to_string())?;
        let mut below = BigNum::new().map_err(|e| e.to_string())?;
        below.checked_sub(&order, &one).map_err(|e| e.to_string())?;
        let drawn_key = BigNum::from_slice(&drawn(b"key")).map_err(|e| e.to_string())?;
        let mut scalar = BigNum::new().map_err(|e| e.to_string())?;
        scalar
            .nnmod(&drawn_key, &below, &mut context)
            .map_err(|e| e.to_string())?;
        scalar.add_word(1).map_err(|e| e.to_string())?;
        let mut public = EcPoint::new(group).map_err(|e| e.to_string())?;
        public
            .mul_generator2(group, &scalar, &mut context)
            .map_err(|e| e.to_string())?;
        let key =
            EcKey::from_private_components(group, &scalar, &public).map_err(|e| e.to_string())?;
        Ok((key, drawn(b"id").to_vec()))
    }

    /// The same device in a browser at another origin: its answers name
    /// that origin, as a phishing site's page would make them.
    pub fn at(&mut self, origin: &str) -> &mut TestAuthenticator {
        self.origin = origin.to_string();
        self
    }

    /// Whether its passkeys count their uses.
    pub fn counting(&mut self, counts: bool) -> &mut TestAuthenticator {
        self.counts = counts;
        self
    }

    /// Sets every passkey's counter, as a copied key's would be.
    pub fn set_sign_count(&mut self, count: u32) {
        for held in &mut self.held {
            held.sign_count = count;
        }
    }

    /// How many passkeys it holds.
    pub fn len(&self) -> usize {
        self.held.len()
    }

    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }

    /// Forgets every passkey: a lost phone.
    pub fn lose(&mut self) {
        self.held.clear();
    }

    /// `navigator.credentials.create` with `options`, the registration
    /// options route's answer: the new credential as `toJSON()` writes it.
    pub fn create(&mut self, options: &str) -> Result<String, String> {
        let options: Value = serde_json::from_str(options).map_err(|e| e.to_string())?;
        let key = at(&options, "/publicKey");
        let challenge = text(key, "challenge")?;
        let rp_id = at(key, "/rp/id").as_str().ok_or("no rp.id")?.to_string();
        let user_handle = from_b64url(at(key, "/user/id").as_str().ok_or("no user.id")?)
            .ok_or("user.id isn't base64url")?;
        let es256 = at(key, "/pubKeyCredParams")
            .as_array()
            .is_some_and(|params| params.iter().any(|param| at(param, "/alg") == -7));
        if !es256 {
            return Err("ES256 isn't offered".into());
        }
        let excluded: Vec<Vec<u8>> = at(key, "/excludeCredentials")
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|credential| at(credential, "/id").as_str().and_then(from_b64url))
            .collect();
        if self
            .held
            .iter()
            .any(|held| held.rp_id == rp_id && excluded.contains(&held.credential_id))
        {
            return Err("InvalidStateError: this device holds a passkey already".into());
        }
        let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1).map_err(|e| e.to_string())?;
        let (secret, credential_id) = self.new_passkey(&group)?;
        let (mut x, mut y) = (
            BigNum::new().map_err(|e| e.to_string())?,
            BigNum::new().map_err(|e| e.to_string())?,
        );
        let mut context = BigNumContext::new().map_err(|e| e.to_string())?;
        secret
            .public_key()
            .affine_coordinates(&group, &mut x, &mut y, &mut context)
            .map_err(|e| e.to_string())?;
        let mut cose = BTreeMap::new();
        cose.insert(Cbor::Integer(1), Cbor::Integer(2));
        cose.insert(Cbor::Integer(3), Cbor::Integer(-7));
        cose.insert(Cbor::Integer(-1), Cbor::Integer(1));
        cose.insert(
            Cbor::Integer(-2),
            Cbor::Bytes(x.to_vec_padded(32).map_err(|e| e.to_string())?),
        );
        cose.insert(
            Cbor::Integer(-3),
            Cbor::Bytes(y.to_vec_padded(32).map_err(|e| e.to_string())?),
        );
        let cose = serde_cbor_2::to_vec(&Cbor::Map(cose)).map_err(|e| e.to_string())?;
        // Present, verified, attested credential data; a counter of 0.
        let mut auth_data = sha256(rp_id.as_bytes()).to_vec();
        auth_data.push(0x01 | 0x04 | 0x40);
        auth_data.extend(0u32.to_be_bytes());
        auth_data.extend([0u8; 16]);
        auth_data.extend(
            u16::try_from(credential_id.len())
                .map_err(|e| e.to_string())?
                .to_be_bytes(),
        );
        auth_data.extend(&credential_id);
        auth_data.extend(&cose);
        let mut attestation = BTreeMap::new();
        attestation.insert(Cbor::Text("fmt".into()), Cbor::Text("none".into()));
        attestation.insert(Cbor::Text("attStmt".into()), Cbor::Map(BTreeMap::new()));
        attestation.insert(Cbor::Text("authData".into()), Cbor::Bytes(auth_data));
        let attestation =
            serde_cbor_2::to_vec(&Cbor::Map(attestation)).map_err(|e| e.to_string())?;
        let client_data = self.client_data("webauthn.create", &challenge);
        self.held.push(Held {
            credential_id: credential_id.clone(),
            rp_id,
            user_handle,
            key: secret,
            sign_count: 0,
        });
        let id = to_b64url(&credential_id);
        Ok(json!({
            "id": id,
            "rawId": id,
            "type": "public-key",
            "response": {
                "clientDataJSON": to_b64url(client_data.as_bytes()),
                "attestationObject": to_b64url(&attestation),
                "transports": ["internal"],
            },
            "clientExtensionResults": {},
        })
        .to_string())
    }

    /// `navigator.credentials.get` with `options`, the sign-in options
    /// route's answer: the assertion of the newest passkey it holds for
    /// the relying party, as `toJSON()` writes it.
    pub fn get(&mut self, options: &str) -> Result<String, String> {
        let options: Value = serde_json::from_str(options).map_err(|e| e.to_string())?;
        let key = at(&options, "/publicKey");
        let challenge = text(key, "challenge")?;
        let rp_id = at(key, "/rpId").as_str().ok_or("no rpId")?.to_string();
        let counts = self.counts;
        let client_data = self.client_data("webauthn.get", &challenge);
        let held = self
            .held
            .iter_mut()
            .rev()
            .find(|held| held.rp_id == rp_id)
            .ok_or("NotAllowedError: no passkey for this site")?;
        if counts {
            held.sign_count = held.sign_count.saturating_add(1);
        }
        let mut auth_data = sha256(rp_id.as_bytes()).to_vec();
        auth_data.push(0x01 | 0x04);
        auth_data.extend(held.sign_count.to_be_bytes());
        let mut signed = auth_data.clone();
        signed.extend(sha256(client_data.as_bytes()));
        let signature = EcdsaSig::sign(&sha256(&signed), &held.key)
            .and_then(|signature| signature.to_der())
            .map_err(|e| e.to_string())?;
        let id = to_b64url(&held.credential_id);
        Ok(json!({
            "id": id,
            "rawId": id,
            "type": "public-key",
            "response": {
                "clientDataJSON": to_b64url(client_data.as_bytes()),
                "authenticatorData": to_b64url(&auth_data),
                "signature": to_b64url(&signature),
                "userHandle": to_b64url(&held.user_handle),
            },
            "clientExtensionResults": {},
        })
        .to_string())
    }

    fn client_data(&self, kind: &str, challenge: &str) -> String {
        json!({
            "type": kind,
            "challenge": challenge,
            "origin": self.origin,
            "crossOrigin": false,
        })
        .to_string()
    }
}

/// The value at `pointer`, or `null`.
fn at<'v>(value: &'v Value, pointer: &str) -> &'v Value {
    value.pointer(pointer).unwrap_or(&Value::Null)
}

fn text(value: &Value, key: &str) -> Result<String, String> {
    at(value, &format!("/{key}"))
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("no {key}"))
}

const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Base64url without padding, as WebAuthn's JSON writes bytes.
pub fn to_b64url(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, byte)| n | (u32::from(*byte) << (16 - 8 * i)));
        for i in 0..=chunk.len() {
            let index = (n >> (18 - 6 * i)) & 63;
            if let Some(letter) = ALPHABET.get(index as usize) {
                out.push(char::from(*letter));
            }
        }
    }
    out
}

/// Base64url read with or without padding.
pub fn from_b64url(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let (mut bits, mut count) = (0u32, 0);
    for byte in text.bytes().filter(|byte| *byte != b'=') {
        let value = ALPHABET.iter().position(|a| *a == byte)?;
        bits = (bits << 6) | u32::try_from(value).ok()?;
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push(u8::try_from((bits >> count) & 0xff).ok()?);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_round_trips() {
        for bytes in [&b""[..], b"f", b"fo", b"foo", b"foob", &[0xfb, 0xff]] {
            assert_eq!(from_b64url(&to_b64url(bytes)).as_deref(), Some(bytes));
        }
        assert_eq!(to_b64url(&[0xfb, 0xff]), "-_8");
    }
}
