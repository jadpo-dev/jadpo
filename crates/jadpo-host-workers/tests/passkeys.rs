//! The Workers host's passkey ceremonies held to `webauthn-rs`'s answers
//! (area 8, "Passkeys"; chunk 8.2l), natively: every ceremony is run on
//! both, the native host's `WebauthnPasskeys` and the Workers host's
//! `WorkersPasskeys`, each with its own challenge, and the answer a device
//! gives each, made by the test authenticator or by hand where a device
//! shouldn't answer so, must come out the same way on both: the same
//! passkey kept, or the same refusal.

use std::collections::BTreeMap;

use jadpo_host_native::passkeys::WebauthnPasskeys;
use jadpo_host_native::test_authenticator::TestAuthenticator;
use jadpo_host_workers::WorkersPasskeys;
use jadpo_runtime::{PasskeyError, PasskeyParts, Passkeys};
use jadpo_value::encoding::{from_base64url, to_base64url};
use openssl::bn::{BigNum, BigNumContext};
use openssl::ec::{EcGroup, EcKey};
use openssl::ecdsa::EcdsaSig;
use openssl::hash::MessageDigest;
use openssl::nid::Nid;
use openssl::pkey::{PKey, Private};
use openssl::rsa::Rsa;
use openssl::sha::sha256;
use openssl::sign::Signer;
use serde_cbor_2::Value as Cbor;
use serde_json::{Value as Json, json};

const ORIGIN: &str = "https://notes.example";
const RP_ID: &str = "notes.example";
const TIMEOUT: u32 = 300_000;
const HANDLE: [u8; 16] = [7; 16];
const ACCOUNT: &str = "ada@example.com";

fn random(bytes: &mut [u8]) -> bool {
    openssl::rand::rand_bytes(bytes).is_ok()
}

const NATIVE: WebauthnPasskeys = WebauthnPasskeys;
const WORKERS: WorkersPasskeys = WorkersPasskeys::new(random);

/// Both hosts' ceremonies, the native one first.
fn both() -> [&'static dyn Passkeys; 2] {
    [&NATIVE, &WORKERS]
}

/// The challenge some options ask a device to sign.
fn challenge_of(options: &str) -> Vec<u8> {
    let options: Json = serde_json::from_str(options).unwrap();
    from_base64url(options["publicKey"]["challenge"].as_str().unwrap()).unwrap()
}

/// A device's key.
enum Signing {
    Es256(EcKey<Private>),
    Rs256(PKey<Private>),
    /// An ES256 key that signs with a high S.
    HighS(EcKey<Private>),
}

impl Signing {
    fn es256() -> Signing {
        let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1).unwrap();
        Signing::Es256(EcKey::generate(&group).unwrap())
    }

    fn rs256(bits: u32) -> Signing {
        Signing::Rs256(PKey::from_rsa(Rsa::generate(bits).unwrap()).unwrap())
    }

    fn ec_coordinates(key: &EcKey<Private>) -> (Vec<u8>, Vec<u8>) {
        let (mut x, mut y) = (BigNum::new().unwrap(), BigNum::new().unwrap());
        let mut context = BigNumContext::new().unwrap();
        key.public_key()
            .affine_coordinates(key.group(), &mut x, &mut y, &mut context)
            .unwrap();
        (x.to_vec_padded(32).unwrap(), y.to_vec_padded(32).unwrap())
    }

    /// Its COSE key.
    fn cose(&self) -> BTreeMap<Cbor, Cbor> {
        let mut map = BTreeMap::new();
        match self {
            Signing::Es256(key) | Signing::HighS(key) => {
                let (x, y) = Signing::ec_coordinates(key);
                map.insert(Cbor::Integer(1), Cbor::Integer(2));
                map.insert(Cbor::Integer(3), Cbor::Integer(-7));
                map.insert(Cbor::Integer(-1), Cbor::Integer(1));
                map.insert(Cbor::Integer(-2), Cbor::Bytes(x));
                map.insert(Cbor::Integer(-3), Cbor::Bytes(y));
            }
            Signing::Rs256(key) => {
                let rsa = key.rsa().unwrap();
                map.insert(Cbor::Integer(1), Cbor::Integer(3));
                map.insert(Cbor::Integer(3), Cbor::Integer(-257));
                map.insert(Cbor::Integer(-1), Cbor::Bytes(rsa.n().to_vec()));
                map.insert(Cbor::Integer(-2), Cbor::Bytes(rsa.e().to_vec()));
            }
        }
        map
    }

    fn sign(&self, data: &[u8]) -> Vec<u8> {
        match self {
            Signing::Es256(key) => EcdsaSig::sign(&sha256(data), key)
                .unwrap()
                .to_der()
                .unwrap(),
            Signing::HighS(key) => {
                let signature = EcdsaSig::sign(&sha256(data), key).unwrap();
                let mut order = BigNum::new().unwrap();
                let mut context = BigNumContext::new().unwrap();
                key.group().order(&mut order, &mut context).unwrap();
                let mut half = order.to_owned().unwrap();
                half.rshift1(&order).unwrap();
                let s = if signature.s() > &half {
                    signature.s().to_owned().unwrap()
                } else {
                    let mut high = BigNum::new().unwrap();
                    high.checked_sub(&order, signature.s()).unwrap();
                    high
                };
                EcdsaSig::from_private_components(signature.r().to_owned().unwrap(), s)
                    .unwrap()
                    .to_der()
                    .unwrap()
            }
            Signing::Rs256(key) => {
                let mut signer = Signer::new(MessageDigest::sha256(), key).unwrap();
                signer.sign_oneshot_to_vec(data).unwrap()
            }
        }
    }
}

const UP: u8 = 0x01;
const UV: u8 = 0x04;
const BE: u8 = 0x08;
const BS: u8 = 0x10;
const AT: u8 = 0x40;
const ED: u8 = 0x80;

/// The attestation statement a made registration carries.
#[derive(Clone)]
enum Statement {
    None,
    /// Packed self-attestation, signed by the new key with `alg`.
    PackedSelf {
        alg: i128,
        spoiled: bool,
    },
    /// Any other format, with an empty statement.
    Other(&'static str),
}

/// A registration's answer, made part by part.
struct Making {
    client: serde_json::Map<String, Json>,
    rp_id: String,
    flags: u8,
    counter: u32,
    credential_id: Vec<u8>,
    key: Cbor,
    statement: Statement,
    extensions: Option<Cbor>,
    transports: Json,
    trailing: bool,
}

impl Making {
    /// What a device makes for `options`, with `key`.
    fn new(options: &str, key: &Signing) -> Making {
        Making {
            client: client("webauthn.create", &challenge_of(options)),
            rp_id: RP_ID.to_string(),
            flags: UP | UV | AT,
            counter: 0,
            credential_id: vec![0xc1; 32],
            key: Cbor::Map(key.cose()),
            statement: Statement::None,
            extensions: None,
            transports: json!(["internal"]),
            trailing: false,
        }
    }

    fn answer(&self, key: &Signing) -> String {
        let client = Json::Object(self.client.clone()).to_string();
        let mut auth = sha256(self.rp_id.as_bytes()).to_vec();
        auth.push(self.flags);
        auth.extend(self.counter.to_be_bytes());
        if self.flags & AT != 0 {
            auth.extend([0u8; 16]);
            auth.extend(
                u16::try_from(self.credential_id.len())
                    .unwrap()
                    .to_be_bytes(),
            );
            auth.extend(&self.credential_id);
            auth.extend(serde_cbor_2::to_vec(&self.key).unwrap());
        }
        if let Some(extensions) = &self.extensions {
            auth.extend(serde_cbor_2::to_vec(extensions).unwrap());
        }
        let (format, statement) = match &self.statement {
            Statement::None => ("none", Cbor::Map(BTreeMap::new())),
            Statement::PackedSelf { alg, spoiled } => {
                let mut signed = auth.clone();
                signed.extend(sha256(client.as_bytes()));
                let mut signature = key.sign(&signed);
                if *spoiled {
                    signed.push(0);
                    signature = key.sign(&signed);
                }
                let mut map = BTreeMap::new();
                map.insert(Cbor::Text("alg".into()), Cbor::Integer(*alg));
                map.insert(Cbor::Text("sig".into()), Cbor::Bytes(signature));
                ("packed", Cbor::Map(map))
            }
            Statement::Other(format) => (*format, Cbor::Map(BTreeMap::new())),
        };
        let mut object = BTreeMap::new();
        object.insert(Cbor::Text("fmt".into()), Cbor::Text(format.into()));
        object.insert(Cbor::Text("attStmt".into()), statement);
        object.insert(Cbor::Text("authData".into()), Cbor::Bytes(auth));
        let mut object = serde_cbor_2::to_vec(&Cbor::Map(object)).unwrap();
        if self.trailing {
            object.push(0);
        }
        let id = to_base64url(&self.credential_id);
        json!({
            "id": id,
            "rawId": id,
            "type": "public-key",
            "response": {
                "clientDataJSON": to_base64url(client.as_bytes()),
                "attestationObject": to_base64url(&object),
                "transports": self.transports,
            },
            "clientExtensionResults": {},
        })
        .to_string()
    }
}

/// Client data as a browser at `ORIGIN` writes it.
fn client(kind: &str, challenge: &[u8]) -> serde_json::Map<String, Json> {
    let Json::Object(object) = json!({
        "type": kind,
        "challenge": to_base64url(challenge),
        "origin": ORIGIN,
        "crossOrigin": false,
    }) else {
        unreachable!()
    };
    object
}

/// What each host makes of a registration with `key` that `change`
/// alters, excluding `exclude`.
fn register_with(
    key: &Signing,
    exclude: &[Vec<u8>],
    change: impl Fn(&mut Making),
) -> [Result<PasskeyParts, PasskeyError>; 2] {
    both().map(|ceremonies| {
        let (options, state) = ceremonies
            .start_registration(ORIGIN, TIMEOUT, HANDLE, ACCOUNT, exclude)
            .unwrap();
        let mut making = Making::new(&options, key);
        change(&mut making);
        ceremonies.finish_registration(ORIGIN, TIMEOUT, &state, &making.answer(key))
    })
}

/// That both hosts made the same of a registration, and what.
fn same(
    results: [Result<PasskeyParts, PasskeyError>; 2],
    case: &str,
) -> Result<PasskeyParts, PasskeyError> {
    let [native, workers] = results;
    assert_eq!(native, workers, "{case}: native, then Workers");
    native
}

fn registers(key: &Signing, case: &str, change: impl Fn(&mut Making)) -> PasskeyParts {
    same(register_with(key, &[], change), case).unwrap_or_else(|error| panic!("{case}: {error:?}"))
}

fn refused(key: &Signing, case: &str, change: impl Fn(&mut Making)) {
    assert_eq!(
        same(register_with(key, &[], change), case),
        Err(PasskeyError::Rejected),
        "{case}"
    );
}

#[test]
fn the_options_are_webauthn_rss() {
    let blank = |options: String| {
        let mut options: Json = serde_json::from_str(&options).unwrap();
        options["publicKey"]["challenge"] = Json::Null;
        options
    };
    for exclude in [vec![], vec![vec![1, 2, 3], vec![4; 32]]] {
        let [native, workers] = both().map(|ceremonies| {
            let (options, _) = ceremonies
                .start_registration(ORIGIN, 120_000, HANDLE, ACCOUNT, &exclude)
                .unwrap();
            blank(options)
        });
        assert_eq!(native, workers);
    }
    let [native, workers] = both().map(|ceremonies| {
        blank(
            ceremonies
                .start_sign_in("http://localhost:8080", 60_000)
                .unwrap()
                .0,
        )
    });
    assert_eq!(native, workers);
    // Each challenge is new, and 32 bytes.
    let (first, _) = WORKERS.start_sign_in(ORIGIN, TIMEOUT).unwrap();
    let (second, _) = WORKERS.start_sign_in(ORIGIN, TIMEOUT).unwrap();
    assert_eq!(challenge_of(&first).len(), 32);
    assert_ne!(challenge_of(&first), challenge_of(&second));
}

#[test]
fn an_origin_that_cant_be_a_relying_party_is_misconfigured_on_both() {
    for origin in ["not a url", "https://127.0.0.1", "https://"] {
        let [native, workers] =
            both().map(|ceremonies| ceremonies.start_sign_in(origin, TIMEOUT).err());
        assert_eq!(native, Some(PasskeyError::Misconfigured), "{origin}");
        assert_eq!(workers, Some(PasskeyError::Misconfigured), "{origin}");
    }
    let [native, workers] = both().map(|ceremonies| {
        ceremonies
            .start_registration(ORIGIN, TIMEOUT, HANDLE, "", &[])
            .err()
    });
    assert_eq!(
        (native, workers),
        (
            Some(PasskeyError::Misconfigured),
            Some(PasskeyError::Misconfigured)
        )
    );
}

#[test]
fn a_passkey_registers_the_same_on_both() {
    let es256 = Signing::es256();
    let parts = registers(&es256, "ES256, no attestation", |_| {});
    assert_eq!(parts.credential_id, vec![0xc1; 32]);
    assert_eq!(parts.sign_count, 0);
    assert!(parts.transports.is_empty());
    registers(&Signing::rs256(2048), "RS256", |_| {});
    let packed = registers(&es256, "packed self-attestation", |making| {
        making.statement = Statement::PackedSelf {
            alg: -7,
            spoiled: false,
        };
        making.transports = json!(["usb", "internal", "test", "smoke", "hybrid"]);
    });
    assert_eq!(packed.transports, ["usb", "internal", "hybrid"]);
    registers(
        &Signing::rs256(2048),
        "RS256 packed self-attestation",
        |making| {
            making.statement = Statement::PackedSelf {
                alg: -257,
                spoiled: false,
            };
        },
    );
    let synced = registers(&es256, "backed up", |making| making.flags |= BE | BS);
    assert!(synced.backup_eligible && synced.backed_up);
    registers(&es256, "the default port written", |making| {
        making
            .client
            .insert("origin".into(), json!("https://notes.example:443"));
    });
    registers(&es256, "a token binding's status", |making| {
        making
            .client
            .insert("tokenBinding".into(), json!({ "status": "supported" }));
    });
    registers(&es256, "no crossOrigin", |making| {
        making.client.remove("crossOrigin");
    });
    registers(
        &es256,
        "the challenge in standard base64, padded",
        |making| {
            let challenge = from_base64url(making.client["challenge"].as_str().unwrap()).unwrap();
            let standard = jadpo_value::encoding::to_base64(&challenge);
            making.client.insert("challenge".into(), json!(standard));
        },
    );
    registers(&es256, "the device's extensions", |making| {
        making.flags |= ED;
        let mut map = BTreeMap::new();
        map.insert(Cbor::Text("credProtect".into()), Cbor::Integer(3));
        making.extensions = Some(Cbor::Map(map));
    });
    registers(&es256, "a counter already counting", |making| {
        making.counter = 5
    });
}

#[test]
fn what_webauthn_rs_refuses_registering_both_refuse() {
    let es256 = Signing::es256();
    refused(&es256, "another site", |making| {
        making
            .client
            .insert("origin".into(), json!("https://evil.example"));
    });
    refused(&es256, "a subdomain", |making| {
        making
            .client
            .insert("origin".into(), json!("https://eu.notes.example"));
    });
    refused(&es256, "another port", |making| {
        making
            .client
            .insert("origin".into(), json!("https://notes.example:8443"));
    });
    refused(&es256, "plain HTTP", |making| {
        making
            .client
            .insert("origin".into(), json!("http://notes.example"));
    });
    refused(&es256, "an origin that isn't one", |making| {
        making
            .client
            .insert("origin".into(), json!("notes.example"));
    });
    refused(&es256, "signing in's type", |making| {
        making.client.insert("type".into(), json!("webauthn.get"));
    });
    refused(&es256, "another challenge", |making| {
        making
            .client
            .insert("challenge".into(), json!(to_base64url(&[9; 32])));
    });
    refused(&es256, "another site's frame", |making| {
        making.client.insert("crossOrigin".into(), json!(true));
    });
    refused(&es256, "crossOrigin not a boolean", |making| {
        making.client.insert("crossOrigin".into(), json!("no"));
    });
    refused(&es256, "a token binding with no status", |making| {
        making.client.insert("tokenBinding".into(), json!({}));
    });
    refused(&es256, "another relying party", |making| {
        making.rp_id = "evil.example".into();
    });
    refused(&es256, "not present", |making| making.flags &= !UP);
    refused(&es256, "not verified", |making| making.flags &= !UV);
    refused(&es256, "backed up but not eligible", |making| {
        making.flags |= BS
    });
    refused(&es256, "no credential", |making| making.flags &= !AT);
    refused(&es256, "extensions that aren't a map", |making| {
        making.flags |= ED;
        making.extensions = Some(Cbor::Integer(3));
    });
    refused(&es256, "packed, signed by another algorithm", |making| {
        making.statement = Statement::PackedSelf {
            alg: -257,
            spoiled: false,
        };
    });
    refused(&es256, "packed, its signature spoiled", |making| {
        making.statement = Statement::PackedSelf {
            alg: -7,
            spoiled: true,
        };
    });
    refused(&es256, "a format nobody knows", |making| {
        making.statement = Statement::Other("smoke");
    });
    refused(&es256, "SafetyNet", |making| {
        making.statement = Statement::Other("android-safetynet");
    });
    refused(&es256, "something after the attestation object", |making| {
        making.trailing = true;
    });
    refused(&es256, "transports that aren't names", |making| {
        making.transports = json!([1]);
    });
    refused(&es256, "a key off the curve", |making| {
        let Cbor::Map(map) = &mut making.key else {
            unreachable!()
        };
        if let Some(Cbor::Bytes(y)) = map.get_mut(&Cbor::Integer(-3)) {
            y[31] ^= 1;
        }
    });
    refused(&es256, "ES256 named for P-384", |making| {
        let Cbor::Map(map) = &mut making.key else {
            unreachable!()
        };
        map.insert(Cbor::Integer(-1), Cbor::Integer(2));
    });
    refused(&es256, "an Ed25519 key", |making| {
        let mut map = BTreeMap::new();
        map.insert(Cbor::Integer(1), Cbor::Integer(1));
        map.insert(Cbor::Integer(3), Cbor::Integer(-8));
        map.insert(Cbor::Integer(-1), Cbor::Integer(6));
        map.insert(Cbor::Integer(-2), Cbor::Bytes(vec![5; 32]));
        making.key = Cbor::Map(map);
    });
    refused(&Signing::rs256(1024), "a 1,024-bit RSA key", |_| {});
    let excluded = register_with(&es256, &[vec![0xc1; 32]], |_| {});
    assert_eq!(same(excluded, "excluded"), Err(PasskeyError::Rejected));
    for (case, credential) in [
        ("not JSON", "{".to_string()),
        (
            "no response",
            json!({ "id": "AQ", "rawId": "AQ", "type": "public-key" }).to_string(),
        ),
    ] {
        let [native, workers] = both().map(|ceremonies| {
            let (_, state) = ceremonies
                .start_registration(ORIGIN, TIMEOUT, HANDLE, ACCOUNT, &[])
                .unwrap();
            ceremonies.finish_registration(ORIGIN, TIMEOUT, &state, &credential)
        });
        assert_eq!(native, Err(PasskeyError::Rejected), "{case}");
        assert_eq!(workers, Err(PasskeyError::Rejected), "{case}");
    }
    let [native, workers] = both()
        .map(|ceremonies| ceremonies.finish_registration(ORIGIN, TIMEOUT, "not a state", "{}"));
    assert_eq!(native, Err(PasskeyError::Misconfigured));
    assert_eq!(workers, Err(PasskeyError::Misconfigured));
}

/// Attestation signed by a certificate is refused on Workers, which reads
/// no certificate chains (area 8, chosen in chunk 8.2l): `webauthn-rs`
/// would check it, and browsers send `none` when none is asked for.
#[test]
fn a_statement_signed_by_a_certificate_is_refused_on_workers() {
    let es256 = Signing::es256();
    for format in ["fido-u2f", "tpm", "android-key", "apple"] {
        let [native, workers] = register_with(&es256, &[], |making| {
            making.statement = Statement::Other(format);
        });
        assert_eq!(
            native,
            Err(PasskeyError::Rejected),
            "{format}: an empty statement"
        );
        assert_eq!(workers, Err(PasskeyError::Rejected), "{format}");
    }
    let (options, state) = WORKERS
        .start_registration(ORIGIN, TIMEOUT, HANDLE, ACCOUNT, &[])
        .unwrap();
    let making = Making::new(&options, &es256);
    let answer = making.answer(&es256);
    // The same answer with an `x5c` in a packed statement.
    let mut credential: Json = serde_json::from_str(&answer).unwrap();
    let object = from_base64url(
        credential["response"]["attestationObject"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let Cbor::Map(mut object) = serde_cbor_2::from_slice(&object).unwrap() else {
        unreachable!()
    };
    let mut statement = BTreeMap::new();
    statement.insert(Cbor::Text("alg".into()), Cbor::Integer(-7));
    statement.insert(Cbor::Text("sig".into()), Cbor::Bytes(vec![0; 70]));
    statement.insert(
        Cbor::Text("x5c".into()),
        Cbor::Array(vec![Cbor::Bytes(vec![0x30, 0x00])]),
    );
    object.insert(Cbor::Text("fmt".into()), Cbor::Text("packed".into()));
    object.insert(Cbor::Text("attStmt".into()), Cbor::Map(statement));
    credential["response"]["attestationObject"] = json!(to_base64url(
        &serde_cbor_2::to_vec(&Cbor::Map(object)).unwrap()
    ));
    assert_eq!(
        WORKERS.finish_registration(ORIGIN, TIMEOUT, &state, &credential.to_string()),
        Err(PasskeyError::Rejected)
    );
}

/// A sign-in's answer, made part by part.
struct Asserting {
    client: serde_json::Map<String, Json>,
    rp_id: String,
    flags: u8,
    counter: u32,
    raw_id: Vec<u8>,
    user_handle: Json,
    spoiled: bool,
    extensions: Option<Cbor>,
}

impl Asserting {
    fn new(options: &str) -> Asserting {
        Asserting {
            client: client("webauthn.get", &challenge_of(options)),
            rp_id: RP_ID.to_string(),
            flags: UP | UV,
            counter: 1,
            raw_id: vec![0xc1; 32],
            user_handle: json!(to_base64url(&HANDLE)),
            spoiled: false,
            extensions: None,
        }
    }

    fn answer(&self, key: &Signing) -> String {
        let client = Json::Object(self.client.clone()).to_string();
        let mut auth = sha256(self.rp_id.as_bytes()).to_vec();
        auth.push(self.flags);
        auth.extend(self.counter.to_be_bytes());
        if let Some(extensions) = &self.extensions {
            auth.extend(serde_cbor_2::to_vec(extensions).unwrap());
        }
        let mut signed = auth.clone();
        signed.extend(sha256(client.as_bytes()));
        if self.spoiled {
            signed.push(0);
        }
        let id = to_base64url(&self.raw_id);
        json!({
            "id": id,
            "rawId": id,
            "type": "public-key",
            "response": {
                "clientDataJSON": to_base64url(client.as_bytes()),
                "authenticatorData": to_base64url(&auth),
                "signature": to_base64url(&key.sign(&signed)),
                "userHandle": self.user_handle,
            },
            "clientExtensionResults": {},
        })
        .to_string()
    }
}

/// What each host makes of signing in with `key`, kept as `stored`, by an
/// answer `change` alters.
fn sign_in_with(
    key: &Signing,
    stored: &PasskeyParts,
    change: impl Fn(&mut Asserting),
) -> [Result<PasskeyParts, PasskeyError>; 2] {
    both().map(|ceremonies| {
        let (options, state) = ceremonies.start_sign_in(ORIGIN, TIMEOUT).unwrap();
        let mut asserting = Asserting::new(&options);
        change(&mut asserting);
        ceremonies.finish_sign_in(
            ORIGIN,
            TIMEOUT,
            &state,
            &asserting.answer(key),
            stored,
            HANDLE,
        )
    })
}

fn signs_in(
    key: &Signing,
    stored: &PasskeyParts,
    case: &str,
    change: impl Fn(&mut Asserting),
) -> PasskeyParts {
    same(sign_in_with(key, stored, change), case)
        .unwrap_or_else(|error| panic!("{case}: {error:?}"))
}

fn sign_in_refused(
    key: &Signing,
    stored: &PasskeyParts,
    case: &str,
    change: impl Fn(&mut Asserting),
) {
    assert_eq!(
        same(sign_in_with(key, stored, change), case),
        Err(PasskeyError::Rejected),
        "{case}"
    );
}

#[test]
fn signing_in_comes_out_the_same_on_both() {
    let es256 = Signing::es256();
    let stored = registers(&es256, "registering", |_| {});
    let used = signs_in(&es256, &stored, "a counter going up", |_| {});
    assert_eq!(used.sign_count, 1);
    assert_eq!(used.public_key, stored.public_key);
    let none = signs_in(
        &es256,
        &stored,
        "a passkey that keeps no counter",
        |asserting| {
            asserting.counter = 0;
        },
    );
    assert_eq!(none.sign_count, 0);
    signs_in(
        &es256,
        &stored,
        "made in another site's frame, which webauthn-rs allows",
        |asserting| {
            asserting.client.insert("crossOrigin".into(), json!(true));
        },
    );
    signs_in(&es256, &stored, "the device's extensions", |asserting| {
        asserting.flags |= ED;
        asserting.extensions = Some(Cbor::Map(BTreeMap::new()));
    });
    let high_s = Signing::HighS(match &es256 {
        Signing::Es256(key) => key.clone(),
        _ => unreachable!(),
    });
    signs_in(&high_s, &stored, "a high S", |_| {});

    let counted = PasskeyParts {
        sign_count: 5,
        ..stored.clone()
    };
    for (case, counter) in [("the same count", 5), ("a lower count", 4), ("none", 0)] {
        sign_in_refused(&es256, &counted, case, |asserting| {
            asserting.counter = counter
        });
    }
    assert_eq!(
        signs_in(&es256, &counted, "a higher count", |asserting| asserting
            .counter =
            9)
        .sign_count,
        9
    );

    sign_in_refused(&es256, &stored, "another site", |asserting| {
        asserting
            .client
            .insert("origin".into(), json!("https://evil.example"));
    });
    sign_in_refused(&es256, &stored, "registering's type", |asserting| {
        asserting
            .client
            .insert("type".into(), json!("webauthn.create"));
    });
    sign_in_refused(&es256, &stored, "another challenge", |asserting| {
        asserting
            .client
            .insert("challenge".into(), json!(to_base64url(&[9; 32])));
    });
    sign_in_refused(&es256, &stored, "another relying party", |asserting| {
        asserting.rp_id = "evil.example".into();
    });
    sign_in_refused(&es256, &stored, "not present", |asserting| {
        asserting.flags &= !UP
    });
    sign_in_refused(&es256, &stored, "not verified", |asserting| {
        asserting.flags &= !UV
    });
    sign_in_refused(&es256, &stored, "now eligible for backup", |asserting| {
        asserting.flags |= BE;
    });
    sign_in_refused(&es256, &stored, "backed up, never eligible", |asserting| {
        asserting.flags |= BS;
    });
    sign_in_refused(&es256, &stored, "a spoiled signature", |asserting| {
        asserting.spoiled = true
    });
    sign_in_refused(&Signing::es256(), &stored, "another key", |_| {});
    sign_in_refused(&es256, &stored, "no user handle", |asserting| {
        asserting.user_handle = Json::Null;
    });
    sign_in_refused(&es256, &stored, "another user handle", |asserting| {
        asserting.user_handle = json!(to_base64url(&[8; 16]));
    });
    sign_in_refused(&es256, &stored, "a short user handle", |asserting| {
        asserting.user_handle = json!(to_base64url(&[7; 15]));
    });
    sign_in_refused(&es256, &stored, "another passkey's id", |asserting| {
        asserting.raw_id = vec![0xc2; 32];
    });

    // A passkey that may be backed up says so each time; its state follows.
    let synced = registers(&es256, "a synced passkey", |making| making.flags |= BE);
    let backed = signs_in(&es256, &synced, "now backed up", |asserting| {
        asserting.flags |= BE | BS;
    });
    assert!(backed.backed_up);
    sign_in_refused(&es256, &synced, "no longer eligible", |_| {});

    let rs256 = Signing::rs256(2048);
    let stored = registers(&rs256, "an RS256 passkey", |_| {});
    signs_in(&rs256, &stored, "RS256", |_| {});
    sign_in_refused(&rs256, &stored, "RS256 spoiled", |asserting| {
        asserting.spoiled = true
    });

    // A kept key that can't be read is the store's fault, not the device's.
    let unreadable = PasskeyParts {
        public_key: vec![0xa0],
        ..stored.clone()
    };
    let [native, workers] = sign_in_with(&rs256, &unreadable, |_| {});
    assert_eq!(native, Err(PasskeyError::Misconfigured));
    assert_eq!(workers, Err(PasskeyError::Misconfigured));
}

/// The test authenticator, the device the live route tests sign in with,
/// gets the same answers from both: a passkey made and used, an answer
/// from another site, a counter gone back, and one that keeps none.
#[test]
fn the_test_authenticator_gets_the_same_answers_from_both() {
    let mut kept = Vec::new();
    for ceremonies in both() {
        let mut device = TestAuthenticator::seeded(ORIGIN, 81);
        let (options, state) = ceremonies
            .start_registration(ORIGIN, TIMEOUT, HANDLE, ACCOUNT, &[])
            .unwrap();
        let made = device.create(&options).unwrap();
        let answer = ceremonies.read_answer(&made).unwrap();
        assert_eq!(answer.challenge, challenge_of(&options));
        assert_eq!(answer.user_handle, None);
        let stored = ceremonies
            .finish_registration(ORIGIN, TIMEOUT, &state, &made)
            .unwrap();
        let mut outcomes = Vec::new();
        let mut stored_now = stored.clone();
        let sign_in = |device: &mut TestAuthenticator, stored_now: &mut PasskeyParts| {
            let (options, state) = ceremonies.start_sign_in(ORIGIN, TIMEOUT).unwrap();
            let answer = device.get(&options).unwrap();
            let read = ceremonies.read_answer(&answer).unwrap();
            assert_eq!(read.challenge, challenge_of(&options));
            assert_eq!(read.user_handle.as_deref(), Some(&HANDLE[..]));
            assert_eq!(read.credential_id, stored_now.credential_id);
            let result =
                ceremonies.finish_sign_in(ORIGIN, TIMEOUT, &state, &answer, stored_now, HANDLE);
            if let Ok(updated) = &result {
                *stored_now = updated.clone();
            }
            result.map(|parts| parts.sign_count)
        };
        outcomes.push(sign_in(&mut device, &mut stored_now));
        outcomes.push(sign_in(&mut device, &mut stored_now));
        device.at("https://evil.example");
        outcomes.push(sign_in(&mut device, &mut stored_now));
        device.at(ORIGIN);
        device.set_sign_count(0);
        outcomes.push(sign_in(&mut device, &mut stored_now));
        let mut synced = TestAuthenticator::seeded(ORIGIN, 82);
        synced.counting(false);
        let (options, state) = ceremonies
            .start_registration(ORIGIN, TIMEOUT, HANDLE, ACCOUNT, &[])
            .unwrap();
        let made = synced.create(&options).unwrap();
        let mut synced_stored = ceremonies
            .finish_registration(ORIGIN, TIMEOUT, &state, &made)
            .unwrap();
        outcomes.push(sign_in(&mut synced, &mut synced_stored));
        outcomes.push(sign_in(&mut synced, &mut synced_stored));
        kept.push((stored, outcomes));
    }
    let workers = kept.pop().unwrap();
    let native = kept.pop().unwrap();
    assert_eq!(native, workers);
    assert_eq!(
        native.1,
        [
            Ok(1),
            Ok(2),
            Err(PasskeyError::Rejected),
            Err(PasskeyError::Rejected),
            Ok(0),
            Ok(0)
        ]
    );
}
