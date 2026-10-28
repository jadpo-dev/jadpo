//! The passkey ceremonies on Workers (area 8, "Passkeys"; chunk 8.2l): the
//! Workers host's [`Passkeys`] capability, in pure Rust, since
//! `webauthn-rs` 0.5 verifies with OpenSSL, which doesn't build for
//! wasm32 (decided 2026-10-21).
//!
//! Every check is the one `webauthn-rs` makes in the native host's
//! ceremonies (`jadpo_host_native::passkeys`), and its tests hold the two
//! to the same answers through the test authenticator:
//!
//! - **The options** are `webauthn-rs`'s, member for member: a
//!   discoverable passkey that verifies the person, no attestation, ES256
//!   or RS256, the relying party the configured origin's host; signing in
//!   with any discoverable passkey, offered in the address field's
//!   autofill. The challenge is 32 bytes from Web Crypto's generator.
//! - **The answer** is read as `webauthn-rs` reads it: its client data's
//!   type, challenge and origin (the configured one only: no subdomain,
//!   no other port), not made in another site's frame when registering;
//!   the authenticator data's relying party, presence and verification;
//!   its backup flags, which can't say backed up without being eligible
//!   and, signing in, must say the eligibility they said when the passkey
//!   was made; the signature over the authenticator data and the client
//!   data's digest with the stored key (jadpo-value's ES256 and RS256);
//!   and the counter, which must go up when either count is above zero.
//!   Byte strings are read in any of the four base64 forms `webauthn-rs`
//!   reads, and CBOR with the `serde_cbor_2` it reads it with.
//! - **A key** is ES256 on P-256, a point on the curve, or RS256 with a
//!   2,048-bit modulus and a three-byte exponent, the two `webauthn-rs`
//!   calls secure; it is kept as the same COSE key in CBOR the native host
//!   writes, so a passkey's record reads the same on either host.
//! - **Attestation** isn't asked for, so a device's statement is `none`,
//!   or packed self-attestation, whose signature by the new key is
//!   checked. Unlike `webauthn-rs`, a statement signed by a certificate
//!   (packed with `x5c`, TPM, Android key, Apple, FIDO U2F) is refused
//!   rather than checked against its chain: nothing here reads X.509
//!   chains, and browsers send `none` when none is asked for (area 8,
//!   "Passkey attestation on Workers").
//!
//! A ceremony's state is JSON the runtime keeps server-side and gives back
//! once: the challenge and, registering, the passkeys not to make again.

use std::collections::BTreeMap;
use std::net::Ipv4Addr;

use jadpo_runtime::http::signed::origin_of;
use jadpo_runtime::{PasskeyAnswer, PasskeyError, PasskeyParts, Passkeys};
use jadpo_value::encoding::{from_base64url, to_base64url};
use jadpo_value::hash::sha256;
use jadpo_value::public_key;
use serde::Deserialize;
use serde_cbor_2::Value as Cbor;
use serde_json::{Map, Value as Json, json};

/// A challenge's length, as `webauthn-rs` makes one.
const CHALLENGE_BYTES: usize = 32;

/// COSE's algorithms (RFC 9053): ES256 and RS256.
const ES256: i128 = -7;
const RS256: i128 = -257;

/// COSE's key types: an elliptic curve with two coordinates, and RSA.
const EC2: i128 = 2;
const RSA: i128 = 3;

/// COSE's P-256 curve.
const P256: i128 = 1;

/// The authenticator data's flags (WebAuthn Level 3 §6.1).
const USER_PRESENT: u8 = 0x01;
const USER_VERIFIED: u8 = 0x04;
const BACKUP_ELIGIBLE: u8 = 0x08;
const BACKED_UP: u8 = 0x10;
const ATTESTED: u8 = 0x40;
const EXTENSIONS: u8 = 0x80;

/// The passkey ceremonies, with the generator a challenge is drawn from.
#[derive(Debug, Clone, Copy)]
pub struct WorkersPasskeys {
    random: fn(&mut [u8]) -> bool,
}

impl WorkersPasskeys {
    /// The ceremonies drawing challenges from `random`, which fills its
    /// bytes or says it couldn't.
    pub const fn new(random: fn(&mut [u8]) -> bool) -> WorkersPasskeys {
        WorkersPasskeys { random }
    }

    fn challenge(&self) -> Result<[u8; CHALLENGE_BYTES], PasskeyError> {
        let mut challenge = [0u8; CHALLENGE_BYTES];
        if (self.random)(&mut challenge) {
            Ok(challenge)
        } else {
            Err(PasskeyError::Misconfigured)
        }
    }
}

/// The relying party for a configured origin.
struct Site {
    /// The origin, as a browser writes it in client data.
    origin: String,
    /// Its host, the relying-party ID.
    id: String,
}

impl Site {
    /// The relying party for `origin`, whose host must be a domain name:
    /// `webauthn-rs` refuses an address, and so does WebAuthn.
    fn of(origin: &str) -> Result<Site, PasskeyError> {
        let origin = origin_of(origin).ok_or(PasskeyError::Misconfigured)?;
        let (_, authority) = origin
            .split_once("://")
            .ok_or(PasskeyError::Misconfigured)?;
        let host = match authority.rsplit_once(':') {
            Some((host, port)) if port.bytes().all(|byte| byte.is_ascii_digit()) => host,
            _ => authority,
        };
        let domain = !host.is_empty()
            && host.parse::<Ipv4Addr>().is_err()
            && host
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_'));
        if !domain {
            return Err(PasskeyError::Misconfigured);
        }
        Ok(Site {
            id: host.to_string(),
            origin,
        })
    }

    /// Whether a client's `origin` is this one: the same scheme, host and
    /// port.
    fn accepts(&self, origin: &str) -> bool {
        origin_of(origin).is_some_and(|origin| origin == self.origin)
    }
}

/// Bytes as WebAuthn's JSON gives them: base64 in the URL alphabet or the
/// standard one, padded or not, as `webauthn-rs` reads them.
fn bytes_of(text: &str) -> Option<Vec<u8>> {
    let unpadded = text.trim_end_matches('=');
    let padding = text.len() - unpadded.len();
    if padding > 2 || (padding > 0 && !text.len().is_multiple_of(4)) {
        return None;
    }
    let url = unpadded.contains(['-', '_']);
    let standard = unpadded.contains(['+', '/']);
    if url && standard {
        return None;
    }
    from_base64url(&unpadded.replace('+', "-").replace('/', "_"))
}

/// The member `name` of `object` as bytes, when it is a string of them.
fn member_bytes(object: &Map<String, Json>, name: &str) -> Option<Vec<u8>> {
    object.get(name)?.as_str().and_then(bytes_of)
}

/// What the client says it did (WebAuthn Level 3 §5.8.1).
struct ClientData {
    kind: String,
    challenge: Vec<u8>,
    origin: String,
    cross_origin: bool,
}

/// `clientDataJSON` read as `webauthn-rs` reads it: an object with a type,
/// a challenge, an origin, and when given a `crossOrigin` that is true or
/// false and a `tokenBinding` with a status.
fn client_data(bytes: &[u8]) -> Option<ClientData> {
    let Json::Object(object) = serde_json::from_slice(bytes).ok()? else {
        return None;
    };
    let text = |name: &str| object.get(name)?.as_str().map(str::to_string);
    let cross_origin = match object.get("crossOrigin") {
        None | Some(Json::Null) => false,
        Some(Json::Bool(cross)) => *cross,
        Some(_) => return None,
    };
    match object.get("tokenBinding") {
        None | Some(Json::Null) => {}
        Some(Json::Object(binding)) if binding.get("status").is_some_and(Json::is_string) => {}
        Some(_) => return None,
    }
    let origin = text("origin")?;
    origin_of(&origin)?;
    Some(ClientData {
        kind: text("type")?,
        challenge: member_bytes(&object, "challenge")?,
        origin,
        cross_origin,
    })
}

/// A new passkey's credential, in its authenticator data.
struct Attested {
    id: Vec<u8>,
    key: Cbor,
}

/// The authenticator data (WebAuthn Level 3 §6.1).
struct AuthData {
    rp_id_hash: [u8; 32],
    flags: u8,
    counter: u32,
    attested: Option<Attested>,
}

impl AuthData {
    fn has(&self, flag: u8) -> bool {
        self.flags & flag != 0
    }
}

/// One CBOR item at the start of `bytes`, and how long it was.
fn cbor_item(bytes: &[u8]) -> Option<(Cbor, usize)> {
    let mut reader = serde_cbor_2::Deserializer::from_slice(bytes);
    let value = Cbor::deserialize(&mut reader).ok()?;
    Some((value, reader.byte_offset()))
}

/// The authenticator data `bytes` hold, read as `webauthn-rs` reads it:
/// the credential when it says it has one, then the extensions as a map
/// when it says it has them.
fn auth_data(bytes: &[u8]) -> Option<AuthData> {
    let rp_id_hash: [u8; 32] = bytes.get(..32)?.try_into().ok()?;
    let flags = *bytes.get(32)?;
    let counter = u32::from_be_bytes(bytes.get(33..37)?.try_into().ok()?);
    let mut rest = bytes.get(37..)?;
    let attested = if flags & ATTESTED != 0 {
        // The device's AAGUID, which no check here reads.
        let after = rest.get(16..)?;
        let length = usize::from(u16::from_be_bytes(after.get(..2)?.try_into().ok()?));
        let id = after.get(2..2 + length)?.to_vec();
        let (key, read) = cbor_item(after.get(2 + length..)?)?;
        rest = after.get(2 + length + read..)?;
        Some(Attested { id, key })
    } else {
        None
    };
    if flags & EXTENSIONS != 0 {
        let (Cbor::Map(_), _) = cbor_item(rest)? else {
            return None;
        };
    }
    Some(AuthData {
        rp_id_hash,
        flags,
        counter,
        attested,
    })
}

/// A passkey's public key.
enum Key {
    Es256 { x: Vec<u8>, y: Vec<u8> },
    Rs256 { n: Vec<u8>, e: Vec<u8> },
}

impl Key {
    /// A COSE key (RFC 9052 §7) ES256 or RS256 checks with, as
    /// `webauthn-rs` reads one: a P-256 point on the curve, or a 2,048-bit
    /// modulus with a three-byte exponent.
    fn from_cose(value: &Cbor) -> Option<Key> {
        let Cbor::Map(map) = value else {
            return None;
        };
        let get = |label: i128| map.get(&Cbor::Integer(label));
        let integer = |label: i128| match get(label) {
            Some(Cbor::Integer(value)) => Some(*value),
            _ => None,
        };
        let bytes = |label: i128| match get(label) {
            Some(Cbor::Bytes(bytes)) => Some(bytes.clone()),
            _ => None,
        };
        match (integer(1)?, integer(3)?) {
            (EC2, ES256) => {
                let (x, y) = (bytes(-2)?, bytes(-3)?);
                (integer(-1)? == P256 && public_key::is_p256_key(&x, &y))
                    .then_some(Key::Es256 { x, y })
            }
            (RSA, RS256) => {
                let (n, e) = (bytes(-1)?, bytes(-2)?);
                (n.len() == 256 && e.len() == 3 && public_key::is_rsa_key(&n, &e))
                    .then_some(Key::Rs256 { n, e })
            }
            _ => None,
        }
    }

    /// The key read from what was kept of it.
    fn from_kept(bytes: &[u8]) -> Option<Key> {
        Key::from_cose(&serde_cbor_2::from_slice(bytes).ok()?)
    }

    fn algorithm(&self) -> i128 {
        match self {
            Key::Es256 { .. } => ES256,
            Key::Rs256 { .. } => RS256,
        }
    }

    /// The key as the native host keeps it: COSE in CBOR, `1` its key
    /// type, `3` its algorithm, then `-1` the curve or RSA's `n`, `-2` `x`
    /// or RSA's `e`, `-3` `y`.
    fn to_cose(&self) -> Option<Vec<u8>> {
        let mut map = BTreeMap::new();
        map.insert(Cbor::Integer(3), Cbor::Integer(self.algorithm()));
        match self {
            Key::Es256 { x, y } => {
                map.insert(Cbor::Integer(1), Cbor::Integer(EC2));
                map.insert(Cbor::Integer(-1), Cbor::Integer(P256));
                map.insert(Cbor::Integer(-2), Cbor::Bytes(x.clone()));
                map.insert(Cbor::Integer(-3), Cbor::Bytes(y.clone()));
            }
            Key::Rs256 { n, e } => {
                map.insert(Cbor::Integer(1), Cbor::Integer(RSA));
                map.insert(Cbor::Integer(-1), Cbor::Bytes(n.clone()));
                map.insert(Cbor::Integer(-2), Cbor::Bytes(e.clone()));
            }
        }
        serde_cbor_2::to_vec(&Cbor::Map(map)).ok()
    }

    /// Whether `signature` is this key's signature of `signed`.
    fn signed(&self, signed: &[u8], signature: &[u8]) -> bool {
        match self {
            Key::Es256 { x, y } => public_key::es256_der(x, y, signed, signature),
            Key::Rs256 { n, e } => public_key::rs256(n, e, signed, signature),
        }
    }
}

/// What a device signs: its authenticator data and the client data's
/// SHA-256 digest.
fn signed_data(auth_data: &[u8], client_data: &[u8]) -> Vec<u8> {
    let mut signed = auth_data.to_vec();
    signed.extend(sha256(client_data));
    signed
}

/// A new passkey as `navigator.credentials.create()` answers, read as
/// `webauthn-rs` reads `RegisterPublicKeyCredential`.
struct Registration {
    raw_id: Vec<u8>,
    client_data: Vec<u8>,
    attestation_object: Vec<u8>,
    transports: Vec<String>,
}

fn registration(credential: &str) -> Option<Registration> {
    let Json::Object(answer) = serde_json::from_str(credential).ok()? else {
        return None;
    };
    let Some(Json::Object(response)) = answer.get("response") else {
        return None;
    };
    if !answer.get("id")?.is_string() || !answer.get("type")?.is_string() {
        return None;
    }
    let transports = match response.get("transports") {
        None | Some(Json::Null) => Vec::new(),
        Some(Json::Array(names)) => names
            .iter()
            .map(|name| name.as_str().map(str::to_string))
            .collect::<Option<_>>()?,
        Some(_) => return None,
    };
    Some(Registration {
        raw_id: member_bytes(&answer, "rawId")?,
        client_data: member_bytes(response, "clientDataJSON")?,
        attestation_object: member_bytes(response, "attestationObject")?,
        transports,
    })
}

/// An answer to `navigator.credentials.get()`, read as `webauthn-rs`
/// reads `PublicKeyCredential`.
struct Assertion {
    raw_id: Vec<u8>,
    client_data: Vec<u8>,
    auth_data: Vec<u8>,
    signature: Vec<u8>,
    user_handle: Option<Vec<u8>>,
}

fn assertion(credential: &str) -> Option<Assertion> {
    let Json::Object(answer) = serde_json::from_str(credential).ok()? else {
        return None;
    };
    let Some(Json::Object(response)) = answer.get("response") else {
        return None;
    };
    if !answer.get("id")?.is_string() || !answer.get("type")?.is_string() {
        return None;
    }
    let user_handle = match response.get("userHandle") {
        None | Some(Json::Null) => None,
        Some(_) => Some(member_bytes(response, "userHandle")?),
    };
    Some(Assertion {
        raw_id: member_bytes(&answer, "rawId")?,
        client_data: member_bytes(response, "clientDataJSON")?,
        auth_data: member_bytes(response, "authenticatorData")?,
        signature: member_bytes(response, "signature")?,
        user_handle,
    })
}

/// The attestation object (WebAuthn Level 3 §6.5): its format, statement
/// and authenticator data, nothing after it.
fn attestation_object(bytes: &[u8]) -> Option<(String, Cbor, Vec<u8>)> {
    let Cbor::Map(map) = serde_cbor_2::from_slice(bytes).ok()? else {
        return None;
    };
    let get = |name: &str| map.get(&Cbor::Text(name.to_string()));
    match get("epAtt") {
        None | Some(Cbor::Null | Cbor::Bool(_)) => {}
        Some(_) => return None,
    }
    match get("largeBlobKey") {
        None | Some(Cbor::Null | Cbor::Bytes(_)) => {}
        Some(_) => return None,
    }
    let (Some(Cbor::Text(format)), Some(statement), Some(Cbor::Bytes(auth_data))) =
        (get("fmt"), get("attStmt"), get("authData"))
    else {
        return None;
    };
    Some((format.clone(), statement.clone(), auth_data.clone()))
}

/// Whether the attestation statement checks for a passkey with `key`:
/// `none`, or packed self-attestation signed by the key itself.
fn attestation_checks(
    format: &str,
    statement: &Cbor,
    key: &Key,
    auth_data: &[u8],
    client_data: &[u8],
) -> bool {
    match format {
        "none" => true,
        "packed" => {
            let Cbor::Map(statement) = statement else {
                return false;
            };
            let get = |name: &str| statement.get(&Cbor::Text(name.to_string()));
            if get("x5c").is_some() || get("ecdaaKeyId").is_some() {
                return false;
            }
            let (Some(Cbor::Integer(algorithm)), Some(Cbor::Bytes(signature))) =
                (get("alg"), get("sig"))
            else {
                return false;
            };
            *algorithm == key.algorithm()
                && key.signed(&signed_data(auth_data, client_data), signature)
        }
        _ => false,
    }
}

/// The ceremony's state as kept: its challenge and, registering, the
/// passkeys not to make again.
fn state_of(state: &str) -> Result<(Vec<u8>, Vec<Vec<u8>>), PasskeyError> {
    let state: Json = serde_json::from_str(state).map_err(|_| PasskeyError::Misconfigured)?;
    let challenge = state
        .get("challenge")
        .and_then(Json::as_str)
        .and_then(from_base64url)
        .ok_or(PasskeyError::Misconfigured)?;
    let exclude = match state.get("exclude") {
        None => Vec::new(),
        Some(Json::Array(ids)) => ids
            .iter()
            .map(|id| id.as_str().and_then(from_base64url))
            .collect::<Option<_>>()
            .ok_or(PasskeyError::Misconfigured)?,
        Some(_) => return Err(PasskeyError::Misconfigured),
    };
    Ok((challenge, exclude))
}

/// Whether the client data is what `site` asked `kind` for with
/// `challenge`.
fn client_checks(client: &ClientData, kind: &str, challenge: &[u8], site: &Site) -> bool {
    client.kind == kind && client.challenge == challenge && site.accepts(&client.origin)
}

/// The transports `webauthn-rs` keeps, by WebAuthn's names.
const TRANSPORTS: [&str; 5] = ["usb", "nfc", "ble", "internal", "hybrid"];

impl Passkeys for WorkersPasskeys {
    fn start_registration(
        &self,
        origin: &str,
        timeout_ms: u32,
        user_handle: [u8; 16],
        account: &str,
        exclude: &[Vec<u8>],
    ) -> Result<(String, String), PasskeyError> {
        let site = Site::of(origin)?;
        if account.is_empty() {
            return Err(PasskeyError::Misconfigured);
        }
        let challenge = to_base64url(&self.challenge()?);
        let excluded: Vec<String> = exclude.iter().map(|id| to_base64url(id)).collect();
        let options = json!({
            "publicKey": {
                "rp": { "id": site.id, "name": site.id },
                "user": {
                    "id": to_base64url(&user_handle),
                    "name": account,
                    "displayName": account,
                },
                "challenge": challenge,
                "pubKeyCredParams": [
                    // ES256 and RS256.
                    { "type": "public-key", "alg": -7 },
                    { "type": "public-key", "alg": -257 },
                ],
                "timeout": timeout_ms,
                "excludeCredentials": excluded
                    .iter()
                    .map(|id| json!({ "type": "public-key", "id": id }))
                    .collect::<Vec<_>>(),
                "authenticatorSelection": {
                    "residentKey": "required",
                    "requireResidentKey": true,
                    "userVerification": "required",
                },
                "attestation": "none",
                "extensions": {
                    "credentialProtectionPolicy": "userVerificationRequired",
                    "enforceCredentialProtectionPolicy": false,
                    "uvm": true,
                    "credProps": true,
                },
            }
        });
        let state = json!({ "challenge": challenge, "exclude": excluded });
        Ok((options.to_string(), state.to_string()))
    }

    fn finish_registration(
        &self,
        origin: &str,
        _timeout_ms: u32,
        state: &str,
        credential: &str,
    ) -> Result<PasskeyParts, PasskeyError> {
        let site = Site::of(origin)?;
        let (challenge, exclude) = state_of(state)?;
        let answer = registration(credential).ok_or(PasskeyError::Rejected)?;
        let client = client_data(&answer.client_data).ok_or(PasskeyError::Rejected)?;
        let (format, statement, auth_bytes) =
            attestation_object(&answer.attestation_object).ok_or(PasskeyError::Rejected)?;
        let auth = auth_data(&auth_bytes).ok_or(PasskeyError::Rejected)?;
        if !client_checks(&client, "webauthn.create", &challenge, &site)
            || client.cross_origin
            || auth.rp_id_hash != sha256(site.id.as_bytes())
            || !auth.has(USER_PRESENT)
            || !auth.has(USER_VERIFIED)
        {
            return Err(PasskeyError::Rejected);
        }
        let attested = auth.attested.as_ref().ok_or(PasskeyError::Rejected)?;
        let key = Key::from_cose(&attested.key).ok_or(PasskeyError::Rejected)?;
        if !attestation_checks(&format, &statement, &key, &auth_bytes, &answer.client_data)
            || (auth.has(BACKED_UP) && !auth.has(BACKUP_ELIGIBLE))
            || exclude.contains(&attested.id)
        {
            return Err(PasskeyError::Rejected);
        }
        // `webauthn-rs` keeps the transports only from a packed statement.
        let transports = if format == "packed" {
            answer
                .transports
                .into_iter()
                .filter(|name| TRANSPORTS.contains(&name.as_str()))
                .collect()
        } else {
            Vec::new()
        };
        Ok(PasskeyParts {
            credential_id: attested.id.clone(),
            public_key: key.to_cose().ok_or(PasskeyError::Misconfigured)?,
            sign_count: auth.counter,
            transports,
            backup_eligible: auth.has(BACKUP_ELIGIBLE),
            backed_up: auth.has(BACKED_UP),
        })
    }

    fn start_sign_in(
        &self,
        origin: &str,
        timeout_ms: u32,
    ) -> Result<(String, String), PasskeyError> {
        let site = Site::of(origin)?;
        let challenge = to_base64url(&self.challenge()?);
        let options = json!({
            "publicKey": {
                "challenge": challenge,
                "timeout": timeout_ms,
                "rpId": site.id,
                "allowCredentials": [],
                "userVerification": "required",
                "extensions": { "uvm": true },
            },
            "mediation": "conditional",
        });
        let state = json!({ "challenge": challenge });
        Ok((options.to_string(), state.to_string()))
    }

    fn read_answer(&self, credential: &str) -> Option<PasskeyAnswer> {
        let (client_data, credential_id, user_handle) = match assertion(credential) {
            Some(answer) => (answer.client_data, answer.raw_id, answer.user_handle),
            None => {
                let answer = registration(credential)?;
                (answer.client_data, answer.raw_id, None)
            }
        };
        let client: Json = serde_json::from_slice(&client_data).ok()?;
        let challenge = client.get("challenge")?.as_str().and_then(bytes_of)?;
        Some(PasskeyAnswer {
            challenge,
            credential_id,
            user_handle,
        })
    }

    fn finish_sign_in(
        &self,
        origin: &str,
        _timeout_ms: u32,
        state: &str,
        credential: &str,
        stored: &PasskeyParts,
        user_handle: [u8; 16],
    ) -> Result<PasskeyParts, PasskeyError> {
        let site = Site::of(origin)?;
        let (challenge, _) = state_of(state)?;
        let answer = assertion(credential).ok_or(PasskeyError::Rejected)?;
        // A discoverable passkey's answer names its user, who must be the
        // passkey's.
        if answer.user_handle.as_deref() != Some(&user_handle[..]) {
            return Err(PasskeyError::Rejected);
        }
        let key = Key::from_kept(&stored.public_key).ok_or(PasskeyError::Misconfigured)?;
        let client = client_data(&answer.client_data).ok_or(PasskeyError::Rejected)?;
        let auth = auth_data(&answer.auth_data).ok_or(PasskeyError::Rejected)?;
        let counter = auth.counter;
        let went_back = (counter > 0 || stored.sign_count > 0) && counter <= stored.sign_count;
        if answer.raw_id != stored.credential_id
            || !client_checks(&client, "webauthn.get", &challenge, &site)
            || auth.rp_id_hash != sha256(site.id.as_bytes())
            || !auth.has(USER_PRESENT)
            || !auth.has(USER_VERIFIED)
            || auth.has(BACKUP_ELIGIBLE) != stored.backup_eligible
            || (auth.has(BACKED_UP) && !stored.backup_eligible)
            || !key.signed(
                &signed_data(&answer.auth_data, &answer.client_data),
                &answer.signature,
            )
            || went_back
        {
            return Err(PasskeyError::Rejected);
        }
        Ok(PasskeyParts {
            sign_count: counter.max(stored.sign_count),
            backed_up: auth.has(BACKED_UP),
            ..stored.clone()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_are_read_in_the_four_base64_forms() {
        for text in ["-_8", "-_8=", "+/8", "+/8="] {
            assert_eq!(bytes_of(text).as_deref(), Some(&[0xfb, 0xff][..]), "{text}");
        }
        assert_eq!(bytes_of("AQ==").as_deref(), Some(&[1u8][..]));
        assert_eq!(bytes_of("AQ="), None);
        assert_eq!(bytes_of("AQ==="), None);
        assert_eq!(bytes_of("-/8"), None);
        assert_eq!(bytes_of("AR"), None);
    }

    #[test]
    fn the_relying_party_is_the_origins_domain() {
        let site = Site::of("https://notes.example").unwrap();
        assert_eq!(site.id, "notes.example");
        assert!(site.accepts("https://notes.example"));
        assert!(site.accepts("https://Notes.Example:443"));
        assert!(!site.accepts("https://notes.example:8443"));
        assert!(!site.accepts("http://notes.example"));
        assert!(!site.accepts("https://eu.notes.example"));
        assert_eq!(
            Site::of("http://localhost:8080").map(|site| site.id).ok(),
            Some("localhost".to_string())
        );
        assert!(Site::of("https://127.0.0.1").is_err());
        assert!(Site::of("https://[::1]").is_err());
        assert!(Site::of("not a url").is_err());
    }
}
