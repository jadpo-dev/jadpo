//! What a request may name. Repository addresses come from anyone, since
//! publishing needs no sign-in, so an address that could reach anything but
//! a public git host is refused here, before anything is run.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// The longest repository address taken.
const LONGEST: usize = 512;

/// A repository reached over HTTPS at a host name, on the default port.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Repository {
    host: String,
    url: String,
}

impl Repository {
    /// Reads `text`, or says why it isn't a repository this can fetch.
    pub fn parse(text: &str) -> Result<Repository, String> {
        if text.len() > LONGEST {
            return Err("its address is too long".to_string());
        }
        let Some(rest) = text.strip_prefix("https://") else {
            return Err("it isn't reached over HTTPS".to_string());
        };
        let (authority, path) = rest
            .split_once('/')
            .ok_or_else(|| "its address names no repository".to_string())?;
        if authority.contains('@') {
            return Err("its address holds a credential".to_string());
        }
        if authority.contains(':') || authority.contains('[') {
            return Err("its address names a port or an IP address".to_string());
        }
        let host = authority.to_ascii_lowercase();
        if !is_host_name(&host) {
            return Err(format!("`{authority}` isn't a public host name"));
        }
        let parts: Vec<&str> = path.split('/').collect();
        let well_formed = !path.is_empty()
            && parts.iter().all(|part| {
                !part.is_empty()
                    && *part != "."
                    && *part != ".."
                    && !part.starts_with('-')
                    && part
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '~'))
            });
        if !well_formed {
            return Err("its path isn't a plain repository path".to_string());
        }
        Ok(Repository {
            url: format!("https://{host}/{path}"),
            host,
        })
    }

    /// A repository at an address the caller vouches for, which only
    /// [`crate::git::GitPolicy::local_for_tests`] makes.
    pub(crate) fn vouched(url: String) -> Repository {
        Repository {
            host: String::new(),
            url,
        }
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn url(&self) -> &str {
        &self.url
    }
}

/// A DNS name of at least two labels, whose last isn't a number, under no
/// suffix that only ever names something private.
fn is_host_name(host: &str) -> bool {
    let labels: Vec<&str> = host.split('.').collect();
    let private = [
        "localhost",
        "local",
        "internal",
        "intranet",
        "lan",
        "home",
        "corp",
    ];
    labels.len() >= 2
        && host.len() <= 253
        && labels.iter().all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
        && labels.last().is_some_and(|last| {
            !private.contains(last) && last.chars().any(|c| c.is_ascii_lowercase())
        })
        && !host.starts_with("0x")
}

/// A commit's full object name: SHA-1's 40 hex digits or SHA-256's 64.
pub fn is_commit(text: &str) -> bool {
    matches!(text.len(), 40 | 64)
        && text
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

/// A tag's short name, in the characters release tags use.
pub fn is_tag(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 128
        && !text.starts_with(['-', '.'])
        && !text.ends_with(['.', '-'])
        && !text.contains("..")
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// A release's digest, as the lock writes it.
pub fn is_digest(text: &str) -> bool {
    text.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
    })
}

/// The address to reach a host at: its first, when every address it has is
/// public, so a name can't point part of the way at something private.
pub fn public_address(addresses: &[IpAddr]) -> Option<IpAddr> {
    let first = *addresses.first()?;
    addresses.iter().all(|a| is_public(*a)).then_some(first)
}

/// Whether `address` is on the public internet: not this machine, a private
/// network (Fly.io's own included), a cloud's metadata service or an address
/// set aside for something else.
pub fn is_public(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => is_public_v6(v6),
    }
}

fn is_public_v4(address: Ipv4Addr) -> bool {
    let [a, b, c, _] = address.octets();
    !(a == 0
        || a == 10
        || a == 127
        || a >= 224
        || (a == 100 && (64..128).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..32).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 192 && b == 0 && c == 2)
        || (a == 192 && b == 168)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113))
}

fn is_public_v6(address: Ipv6Addr) -> bool {
    let segments = address.segments();
    if let Some(mapped) = address.to_ipv4_mapped() {
        return is_public_v4(mapped);
    }
    // Only global unicast (2000::/3) is public, less what stands for an
    // IPv4 address (6to4) or is set aside for documentation.
    (segments[0] & 0xe000) == 0x2000
        && segments[0] != 0x2002
        && !(segments[0] == 0x2001 && segments[1] == 0x0db8)
}
