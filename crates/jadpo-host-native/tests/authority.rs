//! A certificate authority a test makes as it runs, and a server
//! certificate it signs: what a test's TLS server on this machine is
//! checked against (chunk 8.1b), never one a network vouches for. Its PEM
//! is what a PostgreSQL server the harness starts is given, and what a
//! connection's `Certificate` setting holds (chunk 8.1n); jadpo-test's
//! tests load this file too.

use std::sync::Arc;

use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, Issuer, KeyPair, KeyUsagePurpose};
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};

/// An authority made for one test, and a server certificate it signed.
pub struct Authority {
    pub root: Vec<u8>,
    certificate: Vec<u8>,
    key: Vec<u8>,
    /// The authority's certificate, the server's and its key, as PEM.
    #[allow(dead_code)]
    pub root_pem: String,
    #[allow(dead_code)]
    pub certificate_pem: String,
    #[allow(dead_code)]
    pub key_pem: String,
}

impl Authority {
    /// An authority, and a certificate it signs for `name`.
    pub fn signing(name: &str) -> Authority {
        Authority::signing_for(&[name])
    }

    /// An authority, and a certificate it signs for each of `names`.
    pub fn signing_for(names: &[&str]) -> Authority {
        let authority_key = KeyPair::generate().expect("a key");
        let mut params = CertificateParams::new(Vec::<String>::new()).expect("params");
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params
            .distinguished_name
            .push(DnType::CommonName, "Jadpo test authority");
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::DigitalSignature,
        ];
        let root = params.self_signed(&authority_key).expect("the authority");
        let issuer = Issuer::new(params, &authority_key);
        let key = KeyPair::generate().expect("a key");
        let names: Vec<String> = names.iter().map(|name| name.to_string()).collect();
        let certificate = CertificateParams::new(names)
            .expect("params")
            .signed_by(&key, &issuer)
            .expect("the server's certificate");
        Authority {
            root: root.der().to_vec(),
            certificate: certificate.der().to_vec(),
            key: key.serialize_der(),
            root_pem: pem("CERTIFICATE", root.der()),
            certificate_pem: pem("CERTIFICATE", certificate.der()),
            key_pem: pem("PRIVATE KEY", &key.serialize_der()),
        }
    }

    #[allow(dead_code)]
    pub fn server_config(&self) -> Arc<rustls::ServerConfig> {
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .expect("versions")
        .with_no_client_auth()
        .with_single_cert(
            vec![self.certificate.clone().into()],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.key.clone())),
        )
        .expect("a server configuration");
        Arc::new(config)
    }
}

/// `der` as PEM under `label`: Base64 in lines of 64.
fn pem(label: &str, der: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut base64 = String::new();
    for chunk in der.chunks(3) {
        let bytes = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let bits = u32::from(bytes[0]) << 16 | u32::from(bytes[1]) << 8 | u32::from(bytes[2]);
        for at in 0..4 {
            if at <= chunk.len() {
                base64.push(char::from(ALPHABET[(bits >> (18 - 6 * at) & 63) as usize]));
            } else {
                base64.push('=');
            }
        }
    }
    let mut out = format!("-----BEGIN {label}-----\n");
    for line in base64.as_bytes().chunks(64) {
        out.push_str(std::str::from_utf8(line).expect("ASCII"));
        out.push('\n');
    }
    out.push_str(&format!("-----END {label}-----\n"));
    out
}
