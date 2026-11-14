//! A `RedisUrl` as the driver reads it (area 10, "Connection types"):
//! `redis://` or `rediss://`, the user and password to sign in with, the
//! host, the port (6379 when it gives none) and the database (0 when it
//! gives none). `rediss://` is TLS from the first byte, the server's
//! certificate checked against the host's roots for the host's name;
//! `redis://` is plain TCP. The setting's type has already checked the
//! address's shape; what it can't, the driver refuses here, saying what is
//! wrong and never the address: options after `?`, which it reads none of,
//! so none is quietly ignored (a `?ssl=true`, say, that would leave the
//! connection plain), and a user with no password, which Redis can't sign
//! in with.

use std::fmt;

/// Redis's port when an address gives none.
pub const DEFAULT_PORT: u16 = 6379;

/// Where a Redis server is and how to sign in to it.
#[derive(Clone, PartialEq, Eq)]
pub struct Address {
    pub host: String,
    pub port: u16,
    /// `rediss://`.
    pub tls: bool,
    /// The ACL user, when the address names one before its password.
    pub user: Option<String>,
    pub password: Option<String>,
    pub database: u32,
}

impl fmt::Debug for Address {
    /// Never its password.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Address")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("tls", &self.tls)
            .field("user", &self.user)
            .field("database", &self.database)
            .finish_non_exhaustive()
    }
}

impl Address {
    /// `url` read, or why not, never naming its parts.
    pub fn parse(url: &str) -> Result<Address, String> {
        let not = |why: &str| Err(format!("its `url:` {why}"));
        let Some((scheme, rest)) = url.split_once("://") else {
            return not("isn't a Redis address");
        };
        let tls = match scheme.to_ascii_lowercase().as_str() {
            "redis" => false,
            "rediss" => true,
            _ => return not("isn't a Redis address, which starts `redis://` or `rediss://`"),
        };
        if rest.contains('?') {
            return not("has options after `?`, which the Redis driver doesn't read");
        }
        let (authority, path) = match rest.split_once('/') {
            Some((authority, path)) => (authority, path),
            None => (rest, ""),
        };
        let (credentials, server) = match authority.rsplit_once('@') {
            Some((credentials, server)) => (Some(credentials), server),
            None => (None, authority),
        };
        let (user, password) = match credentials {
            None => (None, None),
            Some(credentials) => {
                let (user, password) = match credentials.split_once(':') {
                    Some((user, password)) => (user, Some(password)),
                    None => (credentials, None),
                };
                let malformed = || "its `url:` has a malformed escape".to_string();
                let user = decode(user).ok_or_else(malformed)?;
                let password = match password {
                    Some(password) => Some(decode(password).ok_or_else(malformed)?),
                    None => None,
                };
                // `redis://:password@host` signs in as the default user.
                let user = (!user.is_empty()).then_some(user);
                let password = password.filter(|password| !password.is_empty());
                if user.is_some() && password.is_none() {
                    return not("names a user with no password, which Redis can't sign in with");
                }
                (user, password)
            }
        };
        let (host, port) = match server.strip_prefix('[') {
            Some(bracketed) => {
                let Some((host, after)) = bracketed.split_once(']') else {
                    return not("has an unclosed `[`");
                };
                let port = match after.strip_prefix(':') {
                    Some(port) => Some(port),
                    None if after.is_empty() => None,
                    None => return not("has something after its host's `]`"),
                };
                (host.to_string(), port)
            }
            None => match server.rsplit_once(':') {
                Some((host, port)) => (host.to_string(), Some(port)),
                None => (server.to_string(), None),
            },
        };
        if host.is_empty() {
            return not("has no host");
        }
        let port = match port {
            None | Some("") => DEFAULT_PORT,
            Some(port) => match port.parse::<u16>() {
                Ok(port) if port > 0 => port,
                _ => return not("has a port that isn't one"),
            },
        };
        let database = match path {
            "" => 0,
            number if number.starts_with('+') => {
                return not("has a database that isn't a number from 0");
            }
            number => match number.parse::<u32>() {
                Ok(number) => number,
                Err(_) => return not("has a database that isn't a number from 0"),
            },
        };
        Ok(Address {
            host,
            port,
            tls,
            user,
            password,
            database,
        })
    }

    /// The commands that sign in and choose the database, before anything
    /// else on a new connection; none when it needs neither.
    pub fn hello(&self) -> Vec<Vec<Vec<u8>>> {
        let mut out = Vec::new();
        match (&self.user, &self.password) {
            (Some(user), Some(password)) => out.push(vec![
                b"AUTH".to_vec(),
                user.as_bytes().to_vec(),
                password.as_bytes().to_vec(),
            ]),
            (None, Some(password)) => {
                out.push(vec![b"AUTH".to_vec(), password.as_bytes().to_vec()]);
            }
            _ => {}
        }
        if self.database != 0 {
            out.push(vec![
                b"SELECT".to_vec(),
                self.database.to_string().into_bytes(),
            ]);
        }
        out
    }
}

/// `text` with its percent escapes decoded; `None` for a malformed one or
/// bytes that aren't UTF-8.
fn decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        if byte == b'%' {
            let hex = bytes.get(at + 1..at + 3)?;
            let hex = std::str::from_utf8(hex).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            at += 3;
        } else {
            out.push(byte);
            at += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_gives_its_server_its_sign_in_and_its_database() {
        let upstash = Address::parse("rediss://default:s3cr%40t@eu1-cosmic-ant.upstash.io:6379")
            .expect("an address");
        assert_eq!(upstash.host, "eu1-cosmic-ant.upstash.io");
        assert_eq!(upstash.port, 6379);
        assert!(upstash.tls);
        assert_eq!(upstash.user.as_deref(), Some("default"));
        assert_eq!(upstash.password.as_deref(), Some("s3cr@t"));
        assert_eq!(upstash.database, 0);
        assert_eq!(
            upstash.hello(),
            [vec![
                b"AUTH".to_vec(),
                b"default".to_vec(),
                b"s3cr@t".to_vec()
            ]]
        );

        let local = Address::parse("redis://localhost").expect("an address");
        assert_eq!(
            (local.host.as_str(), local.port, local.tls, local.database),
            ("localhost", 6379, false, 0)
        );
        assert!(local.hello().is_empty());

        let numbered = Address::parse("redis://:pw@[::1]:6380/12").expect("an address");
        assert_eq!((numbered.host.as_str(), numbered.port), ("::1", 6380));
        assert_eq!(numbered.user, None);
        assert_eq!(
            numbered.hello(),
            [
                vec![b"AUTH".to_vec(), b"pw".to_vec()],
                vec![b"SELECT".to_vec(), b"12".to_vec()]
            ]
        );
    }

    /// What the driver can't read is refused, saying what is wrong and
    /// never the address or its password.
    #[test]
    fn what_the_driver_cant_read_is_refused_without_the_address() {
        let cases = [
            ("redis://cache:6379/0?ssl=true", "has options after `?`"),
            ("redis://app@cache", "names a user with no password"),
            ("redis://:pw@:6379", "has no host"),
            ("redis://cache:0", "has a port that isn't one"),
            ("redis://cache:http", "has a port that isn't one"),
            ("redis://cache/one", "has a database that isn't a number"),
            ("redis://cache/-1", "has a database that isn't a number"),
            ("redis://:p%zzw@cache", "has a malformed escape"),
            ("rediss://[::1", "has an unclosed `[`"),
            ("http://cache", "isn't a Redis address"),
        ];
        for (url, why) in cases {
            let error = Address::parse(url).expect_err(url);
            assert!(error.contains(why), "{url}: {error}");
            assert!(!error.contains("cache") && !error.contains("pw"), "{error}");
        }
    }

    #[test]
    fn its_debug_form_never_holds_the_password() {
        let address = Address::parse("redis://app:hunter2@cache/3").expect("an address");
        let shown = format!("{address:?}");
        assert!(!shown.contains("hunter2"), "{shown}");
        assert!(shown.contains("cache"), "{shown}");
    }
}
