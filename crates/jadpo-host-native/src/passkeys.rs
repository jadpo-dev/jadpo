//! The passkey ceremonies on `webauthn-rs` (area 8, "Passkeys"; chunk
//! 5.5z): the native host's [`Passkeys`] capability.
//!
//! Every ceremony is `webauthn-rs`'s passkey one: user verification
//! required, no attestation, the algorithms it calls secure (ES256 and
//! RS256), the relying party the configured origin's host, and only that
//! origin accepted (no subdomains, no other port). Two things are added
//! for discoverable sign-in: registration asks for a resident key
//! (`residentKey: "required"`), so every passkey can sign in without an
//! address first, and signing in uses `webauthn-rs`'s discoverable
//! ceremony (`conditional-ui`). The counter is `webauthn-rs`'s strict
//! check: when either count is above zero, one that didn't go up is
//! refused as a possible copy.
//!
//! A stored `WebAuthnCredential` is loaded part by part into
//! `webauthn-rs`'s `Credential` (area 19, chosen in chunk 5.5r3): what it
//! doesn't hold is the same for every passkey registered here. Its
//! `extensions` are left as none: `webauthn-rs` 0.5.5 reads a stored
//! credential's key, counter, user verification, registration policy and
//! backup flags when checking an answer, never its extensions (chunk
//! 5.5z's finding).

use std::collections::BTreeMap;
use std::time::Duration;

use jadpo_runtime::{PasskeyAnswer, PasskeyError, PasskeyParts, Passkeys};
use serde_cbor_2::Value as Cbor;
use webauthn_rs::prelude::{
    Base64UrlSafeData, COSEKey, COSEKeyType, Credential, DiscoverableAuthentication,
    DiscoverableKey, Passkey, PasskeyRegistration, PublicKeyCredential,
    RegisterPublicKeyCredential, Url, Uuid, Webauthn, WebauthnBuilder,
};
use webauthn_rs_core::proto::{
    AttestationFormat, AuthenticatorTransport, ParsedAttestation, RegisteredExtensions,
    UserVerificationPolicy,
};

/// `webauthn-rs`, for any origin the runtime names.
#[derive(Debug, Default, Clone, Copy)]
pub struct WebauthnPasskeys;

impl WebauthnPasskeys {
    /// The relying party for `origin`: its host the ID, it the only origin
    /// accepted.
    fn site(origin: &str, timeout_ms: u32) -> Result<Webauthn, PasskeyError> {
        let url = Url::parse(origin).map_err(|_| PasskeyError::Misconfigured)?;
        let host = url
            .host_str()
            .ok_or(PasskeyError::Misconfigured)?
            .to_string();
        WebauthnBuilder::new(&host, &url)
            .and_then(|builder| {
                builder
                    .allow_subdomains(false)
                    .allow_any_port(false)
                    .timeout(Duration::from_millis(u64::from(timeout_ms)))
                    .build()
            })
            .map_err(|_| PasskeyError::Misconfigured)
    }
}

impl Passkeys for WebauthnPasskeys {
    fn start_registration(
        &self,
        origin: &str,
        timeout_ms: u32,
        user_handle: [u8; 16],
        account: &str,
        exclude: &[Vec<u8>],
    ) -> Result<(String, String), PasskeyError> {
        let site = Self::site(origin, timeout_ms)?;
        let exclude = exclude.iter().map(|id| id.clone().into()).collect();
        let (options, state) = site
            .start_passkey_registration(
                Uuid::from_bytes(user_handle),
                account,
                account,
                Some(exclude),
            )
            .map_err(|_| PasskeyError::Misconfigured)?;
        // Every passkey is discoverable, so signing in asks for no address.
        let mut options =
            serde_json::to_value(&options).map_err(|_| PasskeyError::Misconfigured)?;
        let selection = options
            .get_mut("publicKey")
            .and_then(|key| key.get_mut("authenticatorSelection"))
            .and_then(serde_json::Value::as_object_mut)
            .ok_or(PasskeyError::Misconfigured)?;
        selection.insert("residentKey".into(), "required".into());
        selection.insert("requireResidentKey".into(), true.into());
        let state = serde_json::to_string(&state).map_err(|_| PasskeyError::Misconfigured)?;
        Ok((options.to_string(), state))
    }

    fn finish_registration(
        &self,
        origin: &str,
        timeout_ms: u32,
        state: &str,
        credential: &str,
    ) -> Result<PasskeyParts, PasskeyError> {
        let site = Self::site(origin, timeout_ms)?;
        let state: PasskeyRegistration =
            serde_json::from_str(state).map_err(|_| PasskeyError::Misconfigured)?;
        let answer: RegisterPublicKeyCredential =
            serde_json::from_str(credential).map_err(|_| PasskeyError::Rejected)?;
        let passkey = site
            .finish_passkey_registration(&answer, &state)
            .map_err(|_| PasskeyError::Rejected)?;
        parts(&Credential::from(passkey))
    }

    fn start_sign_in(
        &self,
        origin: &str,
        timeout_ms: u32,
    ) -> Result<(String, String), PasskeyError> {
        let site = Self::site(origin, timeout_ms)?;
        let (options, state) = site
            .start_discoverable_authentication()
            .map_err(|_| PasskeyError::Misconfigured)?;
        let options = serde_json::to_string(&options).map_err(|_| PasskeyError::Misconfigured)?;
        let state = serde_json::to_string(&state).map_err(|_| PasskeyError::Misconfigured)?;
        Ok((options, state))
    }

    fn read_answer(&self, credential: &str) -> Option<PasskeyAnswer> {
        let (client_data, credential_id, user_handle) =
            match serde_json::from_str::<PublicKeyCredential>(credential) {
                Ok(answer) => (
                    answer.response.client_data_json.to_vec(),
                    answer.raw_id.to_vec(),
                    answer.response.user_handle.map(|handle| handle.to_vec()),
                ),
                Err(_) => {
                    let answer: RegisterPublicKeyCredential =
                        serde_json::from_str(credential).ok()?;
                    (
                        answer.response.client_data_json.to_vec(),
                        answer.raw_id.to_vec(),
                        None,
                    )
                }
            };
        let client_data: serde_json::Value = serde_json::from_slice(&client_data).ok()?;
        let challenge: Base64UrlSafeData =
            serde_json::from_value(client_data.get("challenge")?.clone()).ok()?;
        Some(PasskeyAnswer {
            challenge: challenge.to_vec(),
            credential_id,
            user_handle,
        })
    }

    fn finish_sign_in(
        &self,
        origin: &str,
        timeout_ms: u32,
        state: &str,
        credential: &str,
        stored: &PasskeyParts,
        user_handle: [u8; 16],
    ) -> Result<PasskeyParts, PasskeyError> {
        let site = Self::site(origin, timeout_ms)?;
        let state: DiscoverableAuthentication =
            serde_json::from_str(state).map_err(|_| PasskeyError::Misconfigured)?;
        let answer: PublicKeyCredential =
            serde_json::from_str(credential).map_err(|_| PasskeyError::Rejected)?;
        let (handle, _) = site
            .identify_discoverable_authentication(&answer)
            .map_err(|_| PasskeyError::Rejected)?;
        if handle != Uuid::from_bytes(user_handle) {
            return Err(PasskeyError::Rejected);
        }
        let mut passkey = Passkey::from(load(stored)?);
        let result = site
            .finish_discoverable_authentication(&answer, state, &[DiscoverableKey::from(&passkey)])
            .map_err(|_| PasskeyError::Rejected)?;
        if passkey.update_credential(&result).is_none() {
            return Err(PasskeyError::Rejected);
        }
        parts(&Credential::from(passkey))
    }
}

/// What is kept of `credential`.
fn parts(credential: &Credential) -> Result<PasskeyParts, PasskeyError> {
    let transports = credential
        .transports
        .iter()
        .flatten()
        .filter_map(|transport| {
            Some(
                match transport {
                    AuthenticatorTransport::Usb => "usb",
                    AuthenticatorTransport::Nfc => "nfc",
                    AuthenticatorTransport::Ble => "ble",
                    AuthenticatorTransport::Internal => "internal",
                    AuthenticatorTransport::Hybrid => "hybrid",
                    _ => return None,
                }
                .to_string(),
            )
        })
        .collect();
    Ok(PasskeyParts {
        credential_id: credential.cred_id.to_vec(),
        public_key: cose_bytes(&credential.cred)?,
        sign_count: credential.counter,
        transports,
        backup_eligible: credential.backup_eligible,
        backed_up: credential.backup_state,
    })
}

/// A kept passkey as `webauthn-rs` checks one: what isn't kept is the
/// same for every passkey registered here (area 19, chosen in chunk
/// 5.5r3).
fn load(stored: &PasskeyParts) -> Result<Credential, PasskeyError> {
    let key: Cbor =
        serde_cbor_2::from_slice(&stored.public_key).map_err(|_| PasskeyError::Misconfigured)?;
    let cred = COSEKey::try_from(&key).map_err(|_| PasskeyError::Misconfigured)?;
    let transports: Vec<AuthenticatorTransport> = stored
        .transports
        .iter()
        .filter_map(|name| name.parse().ok())
        .collect();
    Ok(Credential {
        cred_id: stored.credential_id.clone().into(),
        cred,
        counter: stored.sign_count,
        transports: (!transports.is_empty()).then_some(transports),
        user_verified: true,
        backup_eligible: stored.backup_eligible,
        backup_state: stored.backed_up,
        registration_policy: UserVerificationPolicy::Required,
        extensions: RegisteredExtensions::none(),
        attestation: ParsedAttestation::default(),
        attestation_format: AttestationFormat::None,
    })
}

/// A COSE key as CBOR (RFC 9052 §7): `1` its key type, `3` its algorithm,
/// then `-1` the curve or RSA's `n`, `-2` `x` or RSA's `e`, `-3` `y`.
fn cose_bytes(key: &COSEKey) -> Result<Vec<u8>, PasskeyError> {
    let mut map = BTreeMap::new();
    let int = |n: i128| Cbor::Integer(n);
    map.insert(int(3), int(key.type_ as i128));
    match &key.key {
        COSEKeyType::EC_EC2(ec) => {
            map.insert(int(1), int(2));
            map.insert(int(-1), int(ec.curve.clone() as i128));
            map.insert(int(-2), Cbor::Bytes(ec.x.to_vec()));
            map.insert(int(-3), Cbor::Bytes(ec.y.to_vec()));
        }
        COSEKeyType::EC_OKP(okp) => {
            map.insert(int(1), int(1));
            map.insert(int(-1), int(okp.curve.clone() as i128));
            map.insert(int(-2), Cbor::Bytes(okp.x.to_vec()));
        }
        COSEKeyType::RSA(rsa) => {
            map.insert(int(1), int(3));
            map.insert(int(-1), Cbor::Bytes(rsa.n.to_vec()));
            map.insert(int(-2), Cbor::Bytes(rsa.e.to_vec()));
        }
    }
    serde_cbor_2::to_vec(&Cbor::Map(map)).map_err(|_| PasskeyError::Misconfigured)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cose_key_goes_to_cbor_and_back() {
        // `webauthn-rs`'s own ES256 test vector, as area 19's example keeps it.
        let cbor = jadpo_std_base64(
            "pQECAyYgASFYIGXtpaEld8K66ClDf+M4cBoQqqN14btbXeEI3kOcCFUdIlggHlLtdXARY/f55A3fnzQbPcm6hgr34Mp8p+nuzQCE0Zw=",
        );
        let value: Cbor = serde_cbor_2::from_slice(&cbor).unwrap();
        let key = COSEKey::try_from(&value).unwrap();
        let again = cose_bytes(&key).unwrap();
        let reread: Cbor = serde_cbor_2::from_slice(&again).unwrap();
        assert_eq!(COSEKey::try_from(&reread).unwrap(), key);
    }

    #[test]
    fn the_relying_party_is_the_origins_host() {
        assert!(WebauthnPasskeys::site("https://notes.example", 300_000).is_ok());
        assert!(WebauthnPasskeys::site("not a url", 300_000).is_err());
        let (options, _) = WebauthnPasskeys
            .start_sign_in("https://notes.example", 120_000)
            .unwrap();
        let options: serde_json::Value = serde_json::from_str(&options).unwrap();
        assert_eq!(options["publicKey"]["rpId"], "notes.example");
        assert_eq!(options["publicKey"]["userVerification"], "required");
        assert_eq!(options["publicKey"]["timeout"], 120_000);
        let (options, _) = WebauthnPasskeys
            .start_registration(
                "https://notes.example",
                120_000,
                [7; 16],
                "ada@example.com",
                &[vec![1, 2, 3]],
            )
            .unwrap();
        let options: serde_json::Value = serde_json::from_str(&options).unwrap();
        let key = &options["publicKey"];
        assert_eq!(key["authenticatorSelection"]["residentKey"], "required");
        assert_eq!(
            key["authenticatorSelection"]["userVerification"],
            "required"
        );
        assert_eq!(key["attestation"], "none");
        assert_eq!(key["rp"]["id"], "notes.example");
        assert_eq!(key["excludeCredentials"][0]["id"], "AQID");
    }

    fn jadpo_std_base64(text: &str) -> Vec<u8> {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = Vec::new();
        let mut bits = 0u32;
        let mut count = 0;
        for byte in text.bytes().filter(|b| *b != b'=') {
            let value = ALPHABET.iter().position(|a| *a == byte).unwrap() as u32;
            bits = (bits << 6) | value;
            count += 6;
            if count >= 8 {
                count -= 8;
                out.push((bits >> count) as u8);
            }
        }
        out
    }
}
