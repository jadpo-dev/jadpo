//! A certificate authority a test makes as it runs, and a server
//! certificate it signs: what a test's TLS server on this machine is
//! checked against (chunk 8.1b), never one a network vouches for.

use std::sync::Arc;

use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, Issuer, KeyPair, KeyUsagePurpose};
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};

/// An authority made for one test, and a server certificate it signed.
pub struct Authority {
    pub root: Vec<u8>,
    certificate: Vec<u8>,
    key: Vec<u8>,
}

impl Authority {
    /// An authority, and a certificate it signs for `name`.
    pub fn signing(name: &str) -> Authority {
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
        let certificate = CertificateParams::new(vec![name.to_string()])
            .expect("params")
            .signed_by(&key, &issuer)
            .expect("the server's certificate");
        Authority {
            root: root.der().to_vec(),
            certificate: certificate.der().to_vec(),
            key: key.serialize_der(),
        }
    }

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
