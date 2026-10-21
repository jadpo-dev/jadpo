//! How a PostgreSQL connection is secured (area 5, "PostgreSQL over TLS";
//! chunk 8.1n), shared by the checker, the start-up check and the driver:
//! which servers are on this machine, the TLS settings a database's
//! address may not carry, since its connection in `app.jadpo` decides,
//! and the certificates a `Certificate` setting holds.

use crate::standard::security::decode_base64;

/// Whether `host`, as a database address names it, is this machine: a
/// Unix socket's folder, `localhost`, `127.0.0.1` or `::1`, the one place
/// a database is reached without TLS unless its connection says
/// `tls: false`.
pub fn is_this_machine(host: &str) -> bool {
    host.starts_with('/')
        || matches!(
            host.trim_start_matches('[').trim_end_matches(']'),
            "localhost" | "127.0.0.1" | "::1"
        )
}

/// The first TLS setting `url` carries (`sslmode`, `sslrootcert`,
/// `sslnegotiation` and the rest of libpq's `ssl…`, and the old
/// `requiressl`), in either of libpq's forms
/// (`postgresql://host/db?sslmode=require` or `host=db sslmode=require`):
/// what a database's address may not say, since its connection in
/// `app.jadpo` decides. `None` when it carries none.
pub fn tls_parameter(url: &str) -> Option<String> {
    let url = url.trim();
    let keys: Vec<String> = if url.contains("://") {
        let query = url.split_once('?').map(|(_, query)| query).unwrap_or("");
        let query = query.split('#').next().unwrap_or("");
        query
            .split('&')
            .filter_map(|pair| pair.split('=').next())
            .map(percent_decoded)
            .collect()
    } else {
        key_value_keys(url)
    };
    keys.into_iter().find(|key| {
        let key = key.trim().to_ascii_lowercase();
        key.starts_with("ssl") || key == "requiressl"
    })
}

/// `text` with its `%xx` escapes decoded, as a URL's query writes a key.
fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        if byte == b'%'
            && let Some(hex) = text.get(at + 1..at + 3)
            && let Ok(decoded) = u8::from_str_radix(hex, 16)
        {
            out.push(decoded);
            at += 3;
            continue;
        }
        out.push(byte);
        at += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The keys of libpq's `key=value` form, each value plain or in single
/// quotes with `\` escaping the next character.
fn key_value_keys(text: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut chars = text.chars().peekable();
    loop {
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        let mut key = String::new();
        while let Some(c) = chars.next_if(|c| *c != '=' && !c.is_whitespace()) {
            key.push(c);
        }
        if key.is_empty() {
            break;
        }
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        if chars.next_if_eq(&'=').is_none() {
            break;
        }
        keys.push(key);
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        if chars.next_if_eq(&'\'').is_some() {
            while let Some(c) = chars.next() {
                match c {
                    '\\' => {
                        chars.next();
                    }
                    '\'' => break,
                    _ => {}
                }
            }
        } else {
            while chars.next_if(|c| !c.is_whitespace()).is_some() {}
        }
    }
    keys
}

/// The certificates a `Certificate` setting's text holds, each in DER:
/// one or more PEM blocks (`-----BEGIN CERTIFICATE-----`, Base64,
/// `-----END CERTIFICATE-----`), as an authority publishes its bundle, on
/// as many lines as it likes or on one, a `\n` written out counting as a
/// line's end, since an environment variable often holds it so. The
/// reason it refuses `text`, completing "The value ...", otherwise.
pub fn certificates(text: &str) -> Result<Vec<Vec<u8>>, String> {
    const BEGIN: &str = "-----BEGIN CERTIFICATE-----";
    const END: &str = "-----END CERTIFICATE-----";
    let refused = || {
        "isn't a certificate, written as PEM: `-----BEGIN CERTIFICATE-----`, its Base64, then `-----END CERTIFICATE-----`".to_string()
    };
    let text = text.replace("\\n", "\n");
    let mut rest = text.trim();
    let mut out = Vec::new();
    while !rest.is_empty() {
        let after = rest.strip_prefix(BEGIN).ok_or_else(refused)?;
        let (body, next) = after.split_once(END).ok_or_else(refused)?;
        let base64: String = body.chars().filter(|c| !c.is_whitespace()).collect();
        let der = decode_base64(&base64).ok_or_else(refused)?;
        // A certificate is a DER `SEQUENCE`; the TLS stack reads the rest.
        if der.first() != Some(&0x30) {
            return Err(refused());
        }
        out.push(der);
        rest = next.trim_start();
    }
    if out.is_empty() {
        return Err(refused());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_machine_is_a_socket_localhost_or_a_loopback_address() {
        for host in ["/tmp/pg", "localhost", "127.0.0.1", "::1", "[::1]"] {
            assert!(is_this_machine(host), "{host}");
        }
        for host in [
            "db.example.com",
            "10.0.0.5",
            "127.0.0.2",
            "localhost.example",
        ] {
            assert!(!is_this_machine(host), "{host}");
        }
    }

    #[test]
    fn a_tls_setting_in_either_form_is_found() {
        assert_eq!(
            tls_parameter("postgresql://u@db/app?sslmode=require").as_deref(),
            Some("sslmode")
        );
        assert_eq!(
            tls_parameter("postgres://db/app?application_name=x&SSLRootCert=/ca.pem").as_deref(),
            Some("SSLRootCert")
        );
        assert_eq!(
            tls_parameter("postgres://db/app?%73slmode=disable").as_deref(),
            Some("sslmode")
        );
        assert_eq!(
            tls_parameter("host=db sslnegotiation = direct").as_deref(),
            Some("sslnegotiation")
        );
        assert_eq!(
            tls_parameter("host=db requiressl=1").as_deref(),
            Some("requiressl")
        );
        assert_eq!(tls_parameter("postgres://db/sslmode"), None);
        assert_eq!(tls_parameter("postgres://u:ssl@db/app?dbname=x"), None);
        assert_eq!(
            tls_parameter("host=/tmp/pg dbname='sslmode=require' user=x"),
            None
        );
        assert_eq!(tls_parameter("host=db password='a\\' sslmode=x'"), None);
    }

    #[test]
    fn pem_certificates_read_on_many_lines_or_one() {
        // A DER sequence's first bytes, as Base64.
        let one = "-----BEGIN CERTIFICATE-----\nMIIB\nAAAA\n-----END CERTIFICATE-----\n";
        assert_eq!(
            certificates(one),
            Ok(vec![vec![0x30, 0x82, 0x01, 0x00, 0x00, 0x00]])
        );
        let two = format!("{one}{one}");
        assert_eq!(certificates(&two).map(|list| list.len()), Ok(2));
        let escaped = one.replace('\n', "\\n");
        assert_eq!(certificates(&escaped).map(|list| list.len()), Ok(1));
        let flat = one.replace('\n', " ");
        assert_eq!(certificates(&flat).map(|list| list.len()), Ok(1));
        for refused in [
            "",
            "MIIBAAAA",
            "-----BEGIN CERTIFICATE-----\nMIIB",
            "-----BEGIN CERTIFICATE-----\n!!!!\n-----END CERTIFICATE-----",
            "-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----",
            "-----BEGIN PRIVATE KEY-----\nMIIB\n-----END PRIVATE KEY-----",
        ] {
            assert!(certificates(refused).is_err(), "{refused:?}");
        }
    }
}
