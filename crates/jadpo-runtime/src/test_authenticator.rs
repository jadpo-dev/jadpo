//! A software authenticator for tests (area 8, "In development and
//! tests"): what a browser and a device do between a passkey route's
//! options and the answer posted back, so a test drives the generated
//! routes for real. Built only with the `testing` feature, which no
//! deployable turns on, in pure Rust (RustCrypto's P-256), so a test's
//! host has one on every target: `jadpo test`'s natively and a test
//! Worker's under workerd ([`SoftwareAuthenticators`]).
//!
//! It makes ES256 passkeys that are discoverable, verify the person and
//! carry no attestation (`fmt: "none"`), as `navigator.credentials.create`
//! gives them, and answers `navigator.credentials.get` with one: its
//! `clientDataJSON` names the origin the authenticator was made for, so a
//! test can answer from the wrong site, and its counter can be set, so a
//! test can send one that went backwards. Its keys and ids are drawn from
//! its seed, so a test that makes them reads the same every run, and it
//! signs with RFC 6979's deterministic nonces, so it needs no generator.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use jadpo_value::encoding::{from_base64url, to_base64url};
use jadpo_value::hash::sha256;
use p256::ecdsa::signature::hazmat::PrehashSigner;
use p256::ecdsa::{Signature, SigningKey};
use p256::elliptic_curve::ops::ReduceNonZero;
use p256::{FieldBytes, NonZeroScalar};
use serde_cbor_2::Value as Cbor;
use serde_json::{Value, json};

use crate::TestAuthenticators;

/// One passkey the authenticator holds.
#[derive(Debug)]
struct Held {
    credential_id: Vec<u8>,
    rp_id: String,
    user_handle: Vec<u8>,
    key: SigningKey,
    sign_count: u32,
}

/// A device holding passkeys, in a browser at `origin`.
#[derive(Debug)]
pub struct TestAuthenticator {
    origin: String,
    held: Vec<Held>,
    /// Whether each signature counts up; a synced passkey sends `0`.
    counts: bool,
    /// What its passkeys' keys and ids are drawn from, with how many it
    /// made before.
    seed: u64,
}

/// The seeds [`TestAuthenticator::new`] gives, apart from any a test
/// names, so each device it makes holds passkeys of its own.
static UNNAMED: AtomicU64 = AtomicU64::new(1 << 63);

impl TestAuthenticator {
    /// A device in a browser at `origin`, whose passkeys count their uses
    /// and are unlike any other device's this process makes.
    pub fn new(origin: &str) -> TestAuthenticator {
        TestAuthenticator::seeded(origin, UNNAMED.fetch_add(1, Ordering::Relaxed))
    }

    /// A device as [`TestAuthenticator::new`] makes one, whose passkeys'
    /// keys and ids come from `seed` and how many it made before, so a
    /// test that makes them reads the same every run: only what it signs
    /// varies. Devices given different seeds make different passkeys.
    pub fn seeded(origin: &str, seed: u64) -> TestAuthenticator {
        TestAuthenticator {
            origin: origin.to_string(),
            held: Vec::new(),
            counts: true,
            seed,
        }
    }

    /// The next passkey's private key and id, from the device's seed: the
    /// key a scalar from 1 to the curve's order less one.
    fn new_passkey(&self) -> (SigningKey, Vec<u8>) {
        let drawn = |purpose: &[u8]| {
            let mut input = b"jadpo test authenticator ".to_vec();
            input.extend(self.seed.to_be_bytes());
            input.extend((self.held.len() as u64).to_be_bytes());
            input.extend(purpose);
            sha256(&input)
        };
        let scalar = NonZeroScalar::reduce_nonzero(&FieldBytes::from(drawn(b"key")));
        (SigningKey::from(scalar), drawn(b"id").to_vec())
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
        let user_handle = from_base64url(at(key, "/user/id").as_str().ok_or("no user.id")?)
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
            .filter_map(|credential| at(credential, "/id").as_str().and_then(from_base64url))
            .collect();
        if self
            .held
            .iter()
            .any(|held| held.rp_id == rp_id && excluded.contains(&held.credential_id))
        {
            return Err("InvalidStateError: this device holds a passkey already".into());
        }
        let (secret, credential_id) = self.new_passkey();
        let point = secret.verifying_key().to_sec1_point(false);
        let (Some(x), Some(y)) = (point.x(), point.y()) else {
            return Err("the key has no coordinates".into());
        };
        let mut cose = BTreeMap::new();
        cose.insert(Cbor::Integer(1), Cbor::Integer(2));
        cose.insert(Cbor::Integer(3), Cbor::Integer(-7));
        cose.insert(Cbor::Integer(-1), Cbor::Integer(1));
        cose.insert(Cbor::Integer(-2), Cbor::Bytes(x.to_vec()));
        cose.insert(Cbor::Integer(-3), Cbor::Bytes(y.to_vec()));
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
        let id = to_base64url(&credential_id);
        Ok(json!({
            "id": id,
            "rawId": id,
            "type": "public-key",
            "response": {
                "clientDataJSON": to_base64url(client_data.as_bytes()),
                "attestationObject": to_base64url(&attestation),
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
        let signature: Signature = held
            .key
            .sign_prehash(&sha256(&signed))
            .map_err(|e| e.to_string())?;
        let id = to_base64url(&held.credential_id);
        Ok(json!({
            "id": id,
            "rawId": id,
            "type": "public-key",
            "response": {
                "clientDataJSON": to_base64url(client_data.as_bytes()),
                "authenticatorData": to_base64url(&auth_data),
                "signature": to_base64url(signature.to_der().as_bytes()),
                "userHandle": to_base64url(&held.user_handle),
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

/// A test's software authenticators: one device per passkey
/// `test_passkeys.register` adds, by number, each seeded with its number,
/// so the passkeys a test makes are the same every run and on every host.
/// Only a test's host has them.
#[derive(Debug, Default)]
pub struct SoftwareAuthenticators(RefCell<Vec<TestAuthenticator>>);

impl TestAuthenticators for SoftwareAuthenticators {
    fn create(&self, origin: &str, options: &str) -> Result<(u32, String), String> {
        let mut authenticators = self.0.borrow_mut();
        let number = u32::try_from(authenticators.len()).map_err(|e| e.to_string())?;
        let mut device = TestAuthenticator::seeded(origin, u64::from(number));
        let answer = device.create(options)?;
        authenticators.push(device);
        Ok((number, answer))
    }

    fn get(&self, device: u32, options: &str) -> Result<String, String> {
        let mut authenticators = self.0.borrow_mut();
        let device = usize::try_from(device)
            .ok()
            .and_then(|device| authenticators.get_mut(device))
            .ok_or("no such device in this test")?;
        device.get(options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A device's passkeys come from its seed: the same seed the same
    /// key and id, another seed another, and each one a device makes its
    /// own.
    #[test]
    fn a_seed_draws_the_same_passkeys() {
        let options = json!({
            "publicKey": {
                "challenge": "AAAA",
                "rp": { "id": "notes.example" },
                "user": { "id": "BwcH" },
                "pubKeyCredParams": [{ "type": "public-key", "alg": -7 }],
            }
        })
        .to_string();
        let made = |seed: u64| {
            let mut device = TestAuthenticator::seeded("https://notes.example", seed);
            let first: Value = serde_json::from_str(&device.create(&options).unwrap()).unwrap();
            let second: Value = serde_json::from_str(&device.create(&options).unwrap()).unwrap();
            (first["rawId"].clone(), second["rawId"].clone())
        };
        assert_eq!(made(3), made(3));
        assert_ne!(made(3), made(4));
        assert_ne!(made(3).0, made(3).1);
        let mut one = TestAuthenticator::new("https://notes.example");
        let mut other = TestAuthenticator::new("https://notes.example");
        assert_ne!(one.create(&options), other.create(&options));
    }
}
