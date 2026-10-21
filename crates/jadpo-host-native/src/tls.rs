//! TLS natively (chunk 8.1b): rustls, the one stack every connection the
//! host makes is secured with, a driver's stream upgraded through
//! [`crate::NativeTransport`] and a provider call's HTTPS alike. A server
//! is checked against Mozilla's roots compiled in (`webpki-roots`), so a
//! deployable trusts the same servers on every machine; its certificate
//! must be issued for the name it was reached by, TLS 1.2 or 1.3, with
//! `ring`'s cryptography.

use std::io;
use std::sync::{Arc, OnceLock};

use jadpo_drivers::Stream;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, RootCertStore};

/// The configuration every connection is secured with: Mozilla's roots.
pub fn config() -> io::Result<Arc<ClientConfig>> {
    static CONFIG: OnceLock<Option<Arc<ClientConfig>>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let roots = RootCertStore {
                roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
            };
            built(roots).ok()
        })
        .clone()
        .ok_or_else(|| io::Error::other("no TLS configuration"))
}

/// A configuration trusting only `authority`, one certificate in DER: a
/// test's own, which its server's certificate is signed by.
pub fn trusting(authority: &[u8]) -> io::Result<Arc<ClientConfig>> {
    let mut roots = RootCertStore::empty();
    roots
        .add(authority.to_vec().into())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    built(roots)
}

fn built(roots: RootCertStore) -> io::Result<Arc<ClientConfig>> {
    let config =
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(io::Error::other)?
            .with_root_certificates(roots)
            .with_no_client_auth();
    Ok(Arc::new(config))
}

/// `name`, a host name or an address (an IPv6 one with or without its
/// brackets), as the certificate must name it.
pub(crate) fn server_name(name: &str) -> io::Result<ServerName<'static>> {
    let bare = name.trim_start_matches('[').trim_end_matches(']');
    ServerName::try_from(bare.to_string()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("`{name}` isn't a name a certificate can be checked against"),
        )
    })
}

/// `stream` as TLS to `name` under `config`, its handshake done: a
/// certificate or name that fails the check is
/// [`io::ErrorKind::InvalidData`].
pub async fn upgrade(
    config: Arc<ClientConfig>,
    stream: Box<dyn Stream>,
    name: &str,
) -> io::Result<Box<dyn Stream>> {
    let name = server_name(name)?;
    let secure = tokio_rustls::TlsConnector::from(config)
        .connect(name, stream)
        .await?;
    Ok(Box::new(secure))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mozillas_roots_are_compiled_in_and_names_are_checked_as_written() {
        let config = config().expect("a configuration");
        assert!(Arc::ptr_eq(&config, &super::config().expect("the same")));
        assert!(!webpki_roots::TLS_SERVER_ROOTS.is_empty());
        assert!(server_name("smtp.example.com").is_ok());
        assert!(server_name("127.0.0.1").is_ok());
        assert!(server_name("[::1]").is_ok());
        assert_eq!(
            server_name("not a name").map(|_| ()).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            trusting(b"not a certificate")
                .map(|_| ())
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
}
