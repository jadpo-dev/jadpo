//! Structural checks for secret connection addresses, shared by literals
//! and configuration. No resolution or connection attempt happens here.

pub(crate) fn postgres(text: &str) -> bool {
    connection(text, &["postgres", "postgresql"], true)
}

pub(crate) fn redis(text: &str) -> bool {
    connection(text, &["redis", "rediss"], false)
}

fn connection(text: &str, schemes: &[&str], postgres: bool) -> bool {
    if text
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || "\"<>\\^`{|}".contains(c))
        || text.contains('#')
        || !escapes(text)
    {
        return false;
    }
    let Some((scheme, rest)) = text.split_once("://") else {
        return false;
    };
    if !schemes.contains(&scheme.to_ascii_lowercase().as_str()) {
        return false;
    }
    let end = rest.find(['/', '?']).unwrap_or(rest.len());
    let Some(authority) = rest.get(..end) else {
        return false;
    };
    let Some(tail) = rest.get(end..) else {
        return false;
    };
    let host = match authority.rsplit_once('@') {
        Some((credentials, host)) if !credentials.contains('@') => host,
        Some(_) => return false,
        None => authority,
    };
    if !host.is_empty() && !hosts(host, postgres) {
        return false;
    }
    let (path, query) = tail.split_once('?').unwrap_or((tail, ""));
    let mut explicit_host = false;
    if !query.is_empty() {
        for pair in query.split('&') {
            let Some((key, value)) = pair.split_once('=') else {
                return false;
            };
            let (Some(key), Some(value)) = (decode(key), decode(value)) else {
                return false;
            };
            if postgres && matches!(key.as_str(), "host" | "hostaddr") {
                if value.is_empty() || !hosts(&value, true) {
                    return false;
                }
                explicit_host = true;
            }
            if postgres && key == "port" && !value.split(',').all(port) {
                return false;
            }
        }
    }
    if postgres {
        // A named local database (`postgres:///app`) and an explicit
        // socket host are accepted alongside TCP and multi-host URIs.
        !host.is_empty() || explicit_host || path.strip_prefix('/').is_some_and(|p| !p.is_empty())
    } else {
        !host.is_empty()
            && (path.is_empty()
                || path == "/"
                || path.strip_prefix('/').is_some_and(|db| {
                    !db.is_empty()
                        && db.bytes().all(|b| b.is_ascii_digit())
                        && db.parse::<u32>().is_ok()
                }))
    }
}

fn hosts(text: &str, postgres: bool) -> bool {
    (!text.contains(',') || postgres) && text.split(',').all(|host| endpoint(host, postgres))
}

fn endpoint(text: &str, postgres: bool) -> bool {
    if postgres && text.starts_with('/') {
        return !text.chars().any(char::is_control);
    }
    let (host, given_port) = if let Some(ip) = text.strip_prefix('[') {
        let Some((ip, rest)) = ip.split_once(']') else {
            return false;
        };
        let Some(ip) = decode(ip) else {
            return false;
        };
        if ip.parse::<std::net::Ipv6Addr>().is_err() {
            return false;
        }
        if rest.is_empty() {
            return true;
        }
        return rest.strip_prefix(':').is_some_and(port);
    } else {
        text.split_once(':')
            .map_or((text, None), |(host, port)| (host, Some(port)))
    };
    if given_port.is_some_and(|value| !port(value)) {
        return false;
    }
    let Some(host) = decode(host) else {
        return false;
    };
    if postgres && host.starts_with('/') {
        return !host.chars().any(char::is_control);
    }
    !host.is_empty()
        && !host
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || "/:@[]?#".contains(c))
}

fn port(text: &str) -> bool {
    decode(text).is_some_and(|value| {
        !value.is_empty()
            && value.bytes().all(|b| b.is_ascii_digit())
            && value.parse::<u16>().is_ok()
    })
}

fn escapes(text: &str) -> bool {
    let mut bytes = text.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%'
            && !(bytes.next().is_some_and(|b| b.is_ascii_hexdigit())
                && bytes.next().is_some_and(|b| b.is_ascii_hexdigit()))
        {
            return false;
        }
    }
    true
}

fn decode(text: &str) -> Option<String> {
    let mut bytes = text.bytes();
    let mut out = Vec::new();
    while let Some(byte) = bytes.next() {
        out.push(if byte == b'%' {
            let high = char::from(bytes.next()?).to_digit(16)?;
            let low = char::from(bytes.next()?).to_digit(16)?;
            u8::try_from(high * 16 + low).ok()?
        } else {
            byte
        });
    }
    String::from_utf8(out).ok()
}
