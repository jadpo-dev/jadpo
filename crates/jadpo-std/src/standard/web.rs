//! The web and network types (area 19, "Web and software", chunk 5.5n):
//! each held as its canonical text and checked by a pattern and, where a
//! pattern can't say it, a named validator here, or held as a whole number
//! (`HttpStatus`, `NetworkPort`), an enum (`HttpMethod`) or an object of
//! parts (`ProblemDetails`, `UtmParameters`). The governing standards are
//! cited at each: RFC 3986 for URI references, UTS 46 (IDNA 2008) for
//! domain names, RFC 9110 for HTTP, RFC 9457 for problem details, RFC 6901
//! for JSON Pointer, RFC 6265 for cookies, RFC 4632 and RFC 5952 for
//! address ranges, IEEE 802 for MAC addresses, 3GPP TS 23.003 for IMEIs and
//! UTS #51 for emoji. As everywhere, only the canonical text is read: a
//! domain name in lower case and A-labels, a header name in lower case, an
//! IPv6 address as RFC 5952 writes it.
//!
//! The functions `web`'s operations run are here too, beside the grammar
//! they share with the validators: RFC 3986's resolution of a reference
//! against a base, a query added to a URL, and whether a range holds an
//! address.

use std::net::IpAddr;

use super::emoji_data::EMOJI;
use super::{
    Examples, FIRST_RELEASE, Format, FormatKey, Holds, Json, NO_OPTIONS, Part, StandardType,
    Storage, TextForm, Validator,
};
use crate::read::SqlType;

/// RFC 3986's unreserved characters, kept as they are by percent-encoding.
fn unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

/// RFC 3986's sub-delimiters.
fn sub_delim(byte: u8) -> bool {
    matches!(
        byte,
        b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
    )
}

/// Whether `text` is made of the bytes `allowed` takes and
/// percent-encodings, each `%` and two upper-case hexadecimal digits, as
/// RFC 3986 asks producers to write them (§2.1), so one reference has one
/// text.
fn encoded_of(text: &str, allowed: impl Fn(u8) -> bool) -> bool {
    let bytes = text.as_bytes();
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        if byte == b'%' {
            let hex = |offset: usize| {
                bytes
                    .get(at + offset)
                    .is_some_and(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(b))
            };
            if !hex(1) || !hex(2) {
                return false;
            }
            at += 3;
        } else if allowed(byte) {
            at += 1;
        } else {
            return false;
        }
    }
    true
}

/// RFC 3986's `pchar`, less percent-encodings.
fn pchar(byte: u8) -> bool {
    unreserved(byte) || sub_delim(byte) || matches!(byte, b':' | b'@')
}

/// A path segment, which may be empty (`segment`).
fn segment(text: &str) -> bool {
    encoded_of(text, pchar)
}

/// A query or a fragment (RFC 3986 §3.4, §3.5).
fn query_or_fragment(text: &str) -> bool {
    encoded_of(text, |byte| pchar(byte) || matches!(byte, b'/' | b'?'))
}

/// A path that is empty or begins with `/` (`path-abempty`).
fn path_abempty(path: &str) -> bool {
    path.is_empty()
        || path
            .strip_prefix('/')
            .is_some_and(|rest| rest.split('/').all(segment))
}

/// `path-absolute`: `/`, and segments after it, the first not empty, so
/// it is never read as an authority.
fn path_absolute(path: &str) -> bool {
    let Some(rest) = path.strip_prefix('/') else {
        return false;
    };
    rest.is_empty() || (!rest.starts_with('/') && rest.split('/').all(segment))
}

/// `path-noscheme`: segments, the first not empty and with no `:`, so it
/// is never read as a scheme.
fn path_noscheme(path: &str) -> bool {
    let mut segments = path.split('/');
    let first = segments.next().unwrap_or_default();
    !first.is_empty()
        && encoded_of(first, |byte| unreserved(byte) || sub_delim(byte) || byte == b'@')
        && segments.all(segment)
}

/// A port as written after `:`: from 1 to 65535 with no leading zero, as
/// a `NetworkPort` is.
fn port(text: &str) -> bool {
    whole_in(text, 1, 65_535)
}

/// Whether `text` is a whole number's canonical text from `min` to `max`:
/// digits with no sign and no leading zero.
pub fn whole_in(text: &str, min: i64, max: i64) -> bool {
    !text.is_empty()
        && text.bytes().all(|b| b.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'))
        && text
            .parse::<i64>()
            .is_ok_and(|n| (min..=max).contains(&n))
}

/// An IP address in its canonical text: IPv4 in dotted decimal with no
/// leading zeros, IPv6 as RFC 5952 writes it (lower case, the longest run
/// of zeros shortened).
fn canonical_ip(text: &str) -> Option<IpAddr> {
    text.parse::<IpAddr>()
        .ok()
        .filter(|address| address.to_string() == text)
}

/// A host in a URI reference or an origin: a domain name, an IPv4
/// address, or an IPv6 address in brackets (RFC 3986 §3.2.2), each in its
/// canonical text. A user's name and password before the host are
/// refused: RFC 3986 §3.2.1 deprecates them.
fn uri_host(text: &str) -> bool {
    match text.strip_prefix('[') {
        Some(rest) => rest
            .strip_suffix(']')
            .and_then(canonical_ip)
            .is_some_and(|address| address.is_ipv6()),
        None => {
            canonical_ip(text).is_some_and(|address| address.is_ipv4()) || is_domain_name(text)
        }
    }
}

/// An authority: a host and, where given, a port (RFC 3986 §3.2).
fn authority(text: &str) -> bool {
    // The port follows the last `:` outside an IPv6 address's brackets.
    let (host, port_text) = match text.rfind(':') {
        Some(at) if !text[at..].contains(']') => (&text[..at], Some(&text[at + 1..])),
        _ => (text, None),
    };
    uri_host(host) && port_text.is_none_or(port)
}

/// Splits a reference into the part before its query, its query and its
/// fragment, each without its `?` or `#`.
fn split_reference(text: &str) -> (&str, Option<&str>, Option<&str>) {
    let (rest, fragment) = match text.split_once('#') {
        Some((rest, fragment)) => (rest, Some(fragment)),
        None => (text, None),
    };
    let (path, query) = match rest.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (rest, None),
    };
    (path, query, fragment)
}

/// A relative reference (RFC 3986 §4.2, `relative-ref`): no scheme, an
/// authority after `//` or a path of one of the three kinds, and a query
/// and a fragment where given; never empty, since an empty reference names
/// the base itself.
pub fn is_relative_url(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    let (relative, query, fragment) = split_reference(text);
    let relative_ok = match relative.strip_prefix("//") {
        Some(rest) => {
            let (host, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
            authority(host) && path_abempty(path)
        }
        None if relative.starts_with('/') => path_absolute(relative),
        None => relative.is_empty() || path_noscheme(relative),
    };
    relative_ok && query.is_none_or(query_or_fragment) && fragment.is_none_or(query_or_fragment)
}

/// A domain name (RFC 1035, IDNA 2008 by UTS 46): its canonical text is
/// what UTS 46's strict processing gives it, lower case, its labels of
/// letters, digits and hyphens with each beyond ASCII as its A-label
/// (`xn--mnchen-3ya`), each label 1 to 63 characters and the whole at most
/// 253, with no dot at the end; its last label isn't all digits, so it is
/// never an IPv4 address (RFC 3696 §2).
pub fn is_domain_name(text: &str) -> bool {
    text.is_ascii()
        && idna::domain_to_ascii_strict(text).is_ok_and(|ascii| ascii == text)
        && text
            .rsplit('.')
            .next()
            .is_some_and(|last| !last.bytes().all(|b| b.is_ascii_digit()))
}

/// A host name or an address (RFC 3986's `host`, without brackets): a
/// domain name, or an IP address in its canonical text.
pub fn is_hostname(text: &str) -> bool {
    is_domain_name(text) || canonical_ip(text).is_some()
}

/// The default port of each scheme a `WebOrigin` may have.
const ORIGIN_SCHEMES: &[(&str, &str)] = &[("http", "80"), ("https", "443")];

/// A web origin as the HTML standard serialises it (RFC 6454): `http` or
/// `https`, `://`, its host, and its port after `:` only where it isn't the
/// scheme's default; nothing after it.
pub fn is_web_origin(text: &str) -> bool {
    let Some((scheme, rest)) = text.split_once("://") else {
        return false;
    };
    let Some((_, default)) = ORIGIN_SCHEMES.iter().find(|(name, _)| *name == scheme) else {
        return false;
    };
    if rest.contains(['/', '?', '#', '@']) || !authority(rest) {
        return false;
    }
    match rest.rfind(':') {
        Some(at) if !rest[at..].contains(']') => &rest[at + 1..] != *default,
        _ => true,
    }
}

/// An HTTP token (RFC 9110 §5.6.2).
fn token(text: &str) -> bool {
    !text.is_empty()
        && text.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
        })
}

/// A comment in a `User-Agent` (RFC 9110 §5.6.5): text in parentheses,
/// which may hold comments of its own and `\` escapes; `None` when `text`
/// doesn't start with one, else what follows it.
fn comment(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    if bytes.first() != Some(&b'(') {
        return None;
    }
    let mut depth = 0usize;
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return text.get(at + 1..);
                }
            }
            b'\\' => {
                at += 1;
                // A quoted pair: a tab, a space or a visible character.
                if !bytes
                    .get(at)
                    .is_some_and(|b| *b == b'\t' || (0x20..=0x7e).contains(b))
                {
                    return None;
                }
            }
            b'\t' | 0x20..=0x7e => {}
            _ => return None,
        }
        at += 1;
    }
    None
}

/// A product, `name/version` or `name` (RFC 9110 §10.1.5); `None` when
/// `text` doesn't start with one, else what follows it.
fn product(text: &str) -> Option<&str> {
    let end = text.find([' ', '(']).unwrap_or(text.len());
    let (product, rest) = text.split_at(end);
    let (name, version) = match product.split_once('/') {
        Some((name, version)) => (name, Some(version)),
        None => (product, None),
    };
    (token(name) && version.is_none_or(token)).then_some(rest)
}

/// A `User-Agent` (RFC 9110 §10.1.5): a product, then products and
/// comments, each after one space, in ASCII.
pub fn is_user_agent(text: &str) -> bool {
    let Some(mut rest) = product(text) else {
        return false;
    };
    while !rest.is_empty() {
        let Some(next) = rest.strip_prefix(' ') else {
            return false;
        };
        rest = match comment(next) {
            Some(after) => after,
            None => match product(next) {
                Some(after) => after,
                None => return false,
            },
        };
    }
    true
}

/// A cookie's value (RFC 6265 §4.1.1): cookie-octets, or cookie-octets in
/// double quotes.
fn cookie_value(text: &str) -> bool {
    let inner = text
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(text);
    inner.bytes().all(|byte| {
        matches!(byte, 0x21 | 0x23..=0x2b | 0x2d..=0x3a | 0x3c..=0x5b | 0x5d..=0x7e)
    })
}

/// RFC 1123's date, as `Expires` writes it (RFC 6265 §4.1.1, `sane-cookie-date`):
/// `Wed, 21 Oct 2026 07:28:00 GMT`, its weekday the date's.
fn cookie_date(text: &str) -> bool {
    const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let parts: Vec<&str> = text.split(' ').collect();
    let [weekday, day, month, year, clock, "GMT"] = parts.as_slice() else {
        return false;
    };
    let digits = |part: &str, count: usize| {
        part.len() == count && part.bytes().all(|b| b.is_ascii_digit())
    };
    let Some(weekday) = weekday
        .strip_suffix(',')
        .and_then(|name| DAYS.iter().position(|day| *day == name))
    else {
        return false;
    };
    let Some(month) = MONTHS.iter().position(|name| name == month) else {
        return false;
    };
    let clock: Vec<&str> = clock.split(':').collect();
    let [hour, minute, second] = clock.as_slice() else {
        return false;
    };
    let in_range = |part: &str, max: u32| digits(part, 2) && part.parse::<u32>().is_ok_and(|n| n <= max);
    if !digits(day, 2) || !digits(year, 4) || !in_range(hour, 23) || !in_range(minute, 59) || !in_range(second, 59) {
        return false;
    }
    crate::date_number(&format!("{year}-{:02}-{day}", month + 1)).is_some_and(|epoch_day| {
        // 1970-01-01 was a Thursday.
        usize::try_from((i64::from(epoch_day) + 3).rem_euclid(7)).ok() == Some(weekday)
    })
}

/// The attributes a cookie may carry, in the order its canonical text
/// writes them (RFC 6265 §4.1.1, and `SameSite` from RFC 6265bis).
const COOKIE_ATTRIBUTES: [&str; 7] = [
    "Expires", "Max-Age", "Domain", "Path", "Secure", "HttpOnly", "SameSite",
];

/// A cookie as a server sets it (RFC 6265 §4.1.1's `set-cookie-string`,
/// with RFC 6265bis's `SameSite` and name prefixes): a name and a value,
/// then attributes, each after `; `, in the order [`COOKIE_ATTRIBUTES`]
/// lists them, each at most once and spelt as the RFC spells it; an
/// attribute the RFCs don't define is refused. `SameSite=None` needs
/// `Secure`, a `__Secure-` name needs `Secure`, and a `__Host-` name needs
/// `Secure` and `Path=/` and no `Domain`.
pub fn is_cookie(text: &str) -> bool {
    let mut parts = text.split("; ");
    let Some((name, value)) = parts.next().and_then(|pair| pair.split_once('=')) else {
        return false;
    };
    if !token(name) || !cookie_value(value) {
        return false;
    }
    let mut seen: [Option<&str>; 7] = [None; 7];
    let mut last = None;
    for attribute in parts {
        if attribute.contains(';') {
            return false;
        }
        let (key, value) = match attribute.split_once('=') {
            Some((key, value)) => (key, Some(value)),
            None => (attribute, None),
        };
        let Some(at) = COOKIE_ATTRIBUTES.iter().position(|name| *name == key) else {
            return false;
        };
        if last.is_some_and(|before| before >= at) {
            return false;
        }
        last = Some(at);
        let valid = match (key, value) {
            ("Expires", Some(date)) => cookie_date(date),
            ("Max-Age", Some(seconds)) => {
                seconds.len() <= 10
                    && seconds.bytes().all(|b| b.is_ascii_digit())
                    && !seconds.starts_with('0')
                    && !seconds.is_empty()
            }
            ("Domain", Some(domain)) => is_domain_name(domain),
            ("Path", Some(path)) => {
                path.starts_with('/') && !path.bytes().any(|b| b.is_ascii_control())
            }
            ("Secure" | "HttpOnly", None) => true,
            ("SameSite", Some(mode)) => matches!(mode, "Strict" | "Lax" | "None"),
            _ => false,
        };
        if !valid {
            return false;
        }
        seen[at] = Some(value.unwrap_or_default());
    }
    let secure = seen[4].is_some();
    let lower = name.to_ascii_lowercase();
    (seen[6] != Some("None") || secure)
        && (!lower.starts_with("__secure-") || secure)
        && (!lower.starts_with("__host-") || (secure && seen[3] == Some("/") && seen[2].is_none()))
}

/// An address range (RFC 4632 for IPv4, RFC 4291 §2.3 for IPv6): its
/// first address in its canonical text, `/`, and the prefix's length in
/// bits with no leading zero, every bit after the prefix zero.
pub fn is_cidr(text: &str) -> bool {
    cidr(text).is_some()
}

/// A range's first address and prefix length, when `text` is one.
fn cidr(text: &str) -> Option<(IpAddr, u32)> {
    let (address, length) = text.split_once('/')?;
    let address = canonical_ip(address)?;
    let bits = if address.is_ipv4() { 32 } else { 128 };
    if !whole_in(length, 0, bits) {
        return None;
    }
    let length: u32 = length.parse().ok()?;
    (host_bits(address, length) == 0).then_some((address, length))
}

/// The address as a number, and how many bits it has.
fn address_bits(address: IpAddr) -> (u128, u32) {
    match address {
        IpAddr::V4(v4) => (u128::from(u32::from(v4)), 32),
        IpAddr::V6(v6) => (u128::from(v6), 128),
    }
}

/// The bits of `address` after the first `length`.
fn host_bits(address: IpAddr, length: u32) -> u128 {
    let (value, bits) = address_bits(address);
    let host = bits - length;
    if host == 0 {
        0
    } else {
        value & (u128::MAX >> (128 - host))
    }
}

/// `web.cidr_contains(range, address)`: whether `address` is inside
/// `range`. An IPv4 address is never inside an IPv6 range, nor the other
/// way round, as PostgreSQL's `>>=` has it; `None` when either text isn't
/// what its type holds.
pub fn cidr_contains(range: &str, address: &str) -> Option<bool> {
    let (first, length) = cidr(range)?;
    let address: IpAddr = address.parse().ok()?;
    if first.is_ipv4() != address.is_ipv4() {
        return Some(false);
    }
    let (start, bits) = address_bits(first);
    let (value, _) = address_bits(address);
    let shift = bits - length;
    Some(length == 0 || (start >> shift) == (value >> shift))
}

/// Whether `text` is one fully-qualified emoji (UTS #51), from the list
/// in [`EMOJI`].
pub fn is_emoji(text: &str) -> bool {
    EMOJI.binary_search(&text).is_ok()
}

/// `text` with everything but RFC 3986's unreserved characters
/// percent-encoded as UTF-8, in upper case: what `encoding.to_url_component`
/// gives, so a query `web.with_query` writes and a `UtmParameters`' text
/// read alike.
pub fn encode_component(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if unreserved(byte) {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// A component [`encode_component`] wrote, decoded; `None` when it isn't
/// one exactly (anything left unencoded that would be encoded, lower-case
/// hexadecimal, or bytes that aren't UTF-8).
pub fn decode_component(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        if byte == b'%' {
            let hex = text.get(at + 1..at + 3)?;
            if hex.bytes().any(|b| b.is_ascii_lowercase()) {
                return None;
            }
            out.push(u8::from_str_radix(hex, 16).ok()?);
            at += 3;
        } else {
            out.push(byte);
            at += 1;
        }
    }
    let decoded = String::from_utf8(out).ok()?;
    (encode_component(&decoded) == text).then_some(decoded)
}

/// `web.with_query(url, query)`: `url` with each `(key, value)` added to
/// its query as `key=value`, each percent-encoded, after any query it has
/// and before its fragment; the entries are given in key order.
pub fn with_query(url: &str, entries: &[(&str, &str)]) -> String {
    if entries.is_empty() {
        return url.to_string();
    }
    let (rest, fragment) = match url.split_once('#') {
        Some((rest, fragment)) => (rest, Some(fragment)),
        None => (url, None),
    };
    let added = entries
        .iter()
        .map(|(key, value)| format!("{}={}", encode_component(key), encode_component(value)))
        .collect::<Vec<_>>()
        .join("&");
    let joiner = match rest.split_once('?') {
        None => "?",
        Some((_, "")) => "",
        Some((_, query)) if query.ends_with('&') => "",
        Some(_) => "&",
    };
    let mut out = format!("{rest}{joiner}{added}");
    if let Some(fragment) = fragment {
        out.push('#');
        out.push_str(fragment);
    }
    out
}

/// A URI's five parts (RFC 3986 §3): its scheme, authority, path, query
/// and fragment, the scheme and authority `None` for a relative
/// reference without them.
struct Parts<'a> {
    scheme: Option<&'a str>,
    authority: Option<&'a str>,
    path: &'a str,
    query: Option<&'a str>,
    fragment: Option<&'a str>,
}

impl<'a> Parts<'a> {
    /// RFC 3986 Appendix B's split, which reads any reference.
    fn of(text: &'a str) -> Parts<'a> {
        let (rest, fragment) = match text.split_once('#') {
            Some((rest, fragment)) => (rest, Some(fragment)),
            None => (text, None),
        };
        let (rest, query) = match rest.split_once('?') {
            Some((rest, query)) => (rest, Some(query)),
            None => (rest, None),
        };
        let (scheme, rest) = match rest.find([':', '/']) {
            Some(at) if rest.as_bytes().get(at) == Some(&b':') && at > 0 => {
                (Some(&rest[..at]), &rest[at + 1..])
            }
            _ => (None, rest),
        };
        let (authority, path) = match rest.strip_prefix("//") {
            Some(after) => {
                let end = after.find('/').unwrap_or(after.len());
                (Some(&after[..end]), &after[end..])
            }
            None => (None, rest),
        };
        Parts {
            scheme,
            authority,
            path,
            query,
            fragment,
        }
    }
}

/// RFC 3986 §5.2.4: a path with its `.` and `..` segments removed.
fn remove_dot_segments(path: &str) -> String {
    let mut input = path.to_string();
    let mut output = String::new();
    while !input.is_empty() {
        if let Some(rest) = input.strip_prefix("../") {
            input = rest.to_string();
        } else if let Some(rest) = input.strip_prefix("./") {
            input = rest.to_string();
        } else if input.starts_with("/./") {
            input = input[2..].to_string();
        } else if input == "/." {
            input = "/".to_string();
        } else if input.starts_with("/../") || input == "/.." {
            input = format!("/{}", &input[if input == "/.." { 3 } else { 4 }..]);
            match output.rfind('/') {
                Some(at) => output.truncate(at),
                None => output.clear(),
            }
        } else if input == "." || input == ".." {
            input.clear();
        } else {
            let start = usize::from(input.starts_with('/'));
            let end = input[start..].find('/').map_or(input.len(), |at| at + start);
            output.push_str(&input[..end]);
            input = input[end..].to_string();
        }
    }
    output
}

/// RFC 3986 §5.2.3: a relative path merged with the base's.
fn merge(base: &Parts<'_>, path: &str) -> String {
    if base.authority.is_some() && base.path.is_empty() {
        return format!("/{path}");
    }
    match base.path.rfind('/') {
        Some(at) => format!("{}{path}", &base.path[..=at]),
        None => path.to_string(),
    }
}

/// `web.resolve(base, reference)`: the reference resolved against the base
/// URI by RFC 3986 §5.2.2 (strict), and recomposed by §5.3.
pub fn resolve(base: &str, reference: &str) -> String {
    let base = Parts::of(base);
    let r = Parts::of(reference);
    let (scheme, authority, path, query) = if r.scheme.is_some() {
        (r.scheme, r.authority, remove_dot_segments(r.path), r.query)
    } else if r.authority.is_some() {
        (base.scheme, r.authority, remove_dot_segments(r.path), r.query)
    } else if r.path.is_empty() {
        (
            base.scheme,
            base.authority,
            base.path.to_string(),
            r.query.or(base.query),
        )
    } else if r.path.starts_with('/') {
        (base.scheme, base.authority, remove_dot_segments(r.path), r.query)
    } else {
        (
            base.scheme,
            base.authority,
            remove_dot_segments(&merge(&base, r.path)),
            r.query,
        )
    };
    let mut out = String::new();
    if let Some(scheme) = scheme {
        out.push_str(scheme);
        out.push(':');
    }
    if let Some(authority) = authority {
        out.push_str("//");
        out.push_str(authority);
    }
    out.push_str(&path);
    if let Some(query) = query {
        out.push('?');
        out.push_str(query);
    }
    if let Some(fragment) = r.fragment {
        out.push('#');
        out.push_str(fragment);
    }
    out
}

/// `UtmParameters` (area 19, "Web and software").
pub const UTM_PARAMETERS: &str = "UtmParameters";

/// `UtmParameters`' parts, in order: where a visit came from (Google
/// Analytics' convention), each one line of text; the term and the
/// content only where a campaign uses them.
pub const UTM_FIELDS: [(&str, Part); 5] = [
    ("source", Part::Line),
    ("medium", Part::Line),
    ("campaign", Part::Line),
    ("term", Part::Optional(&Part::Line)),
    ("content", Part::Optional(&Part::Line)),
];

/// A `UtmParameters`' canonical text: `utm_<part>=<value>` for each part
/// it has, in its parts' order, joined by `&`, each value percent-encoded
/// as [`encode_component`] writes it.
pub fn utm_text(parts: &[Option<&str>]) -> String {
    UTM_FIELDS
        .iter()
        .zip(parts)
        .filter_map(|((name, _), value)| {
            value.map(|value| format!("utm_{name}={}", encode_component(value)))
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// A `UtmParameters`' parts read from its canonical text, each a line of
/// text or absent; `None` for text that isn't its canonical text.
pub fn read_utm(raw: &str) -> Option<Vec<Option<String>>> {
    let mut parts: Vec<Option<String>> = vec![None; UTM_FIELDS.len()];
    let mut next = 0;
    for pair in raw.split('&') {
        let (key, value) = pair.split_once('=')?;
        let name = key.strip_prefix("utm_")?;
        let at = UTM_FIELDS.iter().position(|(field, _)| *field == name)?;
        if at < next {
            return None;
        }
        next = at + 1;
        let value = decode_component(value)?;
        Part::Line.rules().check_text(&value).ok()?;
        parts[at] = Some(value);
    }
    let required = UTM_FIELDS
        .iter()
        .zip(&parts)
        .all(|((_, part), value)| part.required().1 || value.is_some());
    required.then_some(parts)
}

/// `ProblemDetails` (area 19, "Web and software").
pub const PROBLEM_DETAILS: &str = "ProblemDetails";

/// `ProblemDetails`' members (RFC 9457 §3.1), in its order, each one that
/// may be left out: the problem's type, a URL (`about:blank` when left
/// out), the HTTP status, a short title, a detail for people, and the URL
/// of this occurrence. `type` is a reserved word, so code writes `type_`
/// and the wire `type`, as a closed list's code that is a reserved word is
/// (question 22).
pub const PROBLEM_FIELDS: [(&str, Part); 5] = [
    ("type_", Part::Optional(&Part::Of(crate::Builtin::Url))),
    ("status", Part::Optional(&Part::Standard("HttpStatus"))),
    ("title", Part::Optional(&Part::Line)),
    ("detail", Part::Optional(&Part::Paragraph)),
    ("instance", Part::Optional(&Part::Of(crate::Builtin::Url))),
];

/// `HttpMethod`'s variants with their tokens (RFC 9110 §9, and `PATCH` from
/// RFC 5789): the methods a route is written with (area 7), in
/// `jadpo_std::HttpMethod`'s order; the runtime answers `HEAD` and
/// `OPTIONS` itself.
pub const HTTP_METHODS: &[(&str, &str)] = &[
    ("get", "GET"),
    ("post", "POST"),
    ("put", "PUT"),
    ("patch", "PATCH"),
    ("delete", "DELETE"),
];

/// A standard type held as text, shown as its canonical text.
const fn text_type(
    name: &'static str,
    pattern: &'static str,
    validator: Option<Validator>,
    personal: Option<&'static str>,
    valid: &'static [(&'static str, &'static str)],
    malformed: &'static [&'static str],
) -> StandardType {
    StandardType {
        name,
        holds: Holds::Text { pattern, personal },
        text: Some(TextForm::Checked),
        json: Json::Text,
        storage: Storage::Column(SqlType::Text),
        format: Format {
            key: FormatKey::Canonical,
            options: NO_OPTIONS,
        },
        ordered: false,
        validator,
        shape: None,
        examples: Examples {
            valid,
            format: None,
            malformed,
            of: None,
            settings: "",
        },
        release: FIRST_RELEASE,
    }
}

/// A standard type held as a whole number from `min` to `max`: sent as a
/// JSON number, stored in an `integer` column, ordered, and shown as its
/// digits, which no locale groups (a port isn't `8,080`).
const fn whole_type(
    name: &'static str,
    min: i64,
    max: i64,
    valid: &'static [(&'static str, &'static str)],
    malformed: &'static [&'static str],
) -> StandardType {
    StandardType {
        name,
        holds: Holds::Whole { min, max },
        text: Some(TextForm::Whole),
        json: Json::Number,
        storage: Storage::Column(SqlType::Int4),
        format: Format {
            key: FormatKey::Canonical,
            options: NO_OPTIONS,
        },
        ordered: true,
        validator: None,
        shape: None,
        examples: Examples {
            valid,
            format: None,
            malformed,
            of: None,
            settings: "",
        },
        release: FIRST_RELEASE,
    }
}

/// RFC 3986's `pchar` as a pattern, upper-case percent-encodings only.
macro_rules! pchar {
    () => {
        r"(?:[A-Za-z0-9\-._~!$&'()*+,;=:@]|%[0-9A-F]{2})"
    };
}

/// An address range: IPv4 (RFC 4632) or IPv6 (RFC 4291), its host bits
/// zero.
pub const CIDR: StandardType = text_type(
    "Cidr",
    r"[0-9a-f.:]+/[0-9]{1,3}",
    Some(Validator::Cidr),
    None,
    // RFC 5737's and RFC 3849's documentation ranges.
    &[
        ("192.0.2.0/24", "192.0.2.0/24"),
        ("198.51.100.7/32", "198.51.100.7/32"),
        ("0.0.0.0/0", "0.0.0.0/0"),
        ("2001:db8::/32", "2001:db8::/32"),
        ("2001:db8:0:1::/64", "2001:db8:0:1::/64"),
    ],
    &[
        // A host bit set.
        "192.0.2.1/24",
        "192.0.2.0/33",
        "192.0.2.0",
        "192.0.2.0/024",
        // Upper case, and not RFC 5952's shortest form.
        "2001:DB8::/32",
        "2001:0db8::/32",
        "2001:db8::/129",
        "",
    ],
);

/// A cookie as a server sets it (RFC 6265, with RFC 6265bis's
/// `SameSite`); online personal data, since its value is often a session.
pub const COOKIE: StandardType = text_type(
    "Cookie",
    r"[\x21-\x7E](?:[\x20-\x7E]*[\x21-\x7E])?",
    Some(Validator::Cookie),
    Some("online"),
    // Made up to RFC 6265's examples (§3.1).
    &[
        ("lang=en-GB", "lang=en-GB"),
        (
            "SID=31d4d96e407aad42; Path=/; Secure; HttpOnly; SameSite=Lax",
            "SID=31d4d96e407aad42; Path=/; Secure; HttpOnly; SameSite=Lax",
        ),
        (
            "lang=en-US; Expires=Wed, 21 Oct 2026 07:28:00 GMT; Domain=example.com",
            "lang=en-US; Expires=Wed, 21 Oct 2026 07:28:00 GMT; Domain=example.com",
        ),
        (
            "__Host-SID=31d4d96e407aad42; Max-Age=3600; Path=/; Secure; SameSite=None",
            "__Host-SID=31d4d96e407aad42; Max-Age=3600; Path=/; Secure; SameSite=None",
        ),
    ],
    &[
        // A space in the value.
        "SID=31d4 d96e",
        // Attributes spelt otherwise, out of order, twice, or unknown.
        "SID=31d4d96e; secure",
        "SID=31d4d96e; HttpOnly; Secure",
        "SID=31d4d96e; Path=/; Path=/docs",
        "SID=31d4d96e; Partitioned",
        "SID=31d4d96e;Path=/",
        // `SameSite=None` without `Secure`, and the name prefixes' rules.
        "SID=31d4d96e; SameSite=None",
        "__Secure-SID=31d4d96e; Path=/",
        "__Host-SID=31d4d96e; Domain=example.com; Path=/; Secure",
        // The weekday isn't the date's, and a zero `Max-Age`.
        "lang=en-US; Expires=Thu, 21 Oct 2026 07:28:00 GMT",
        "lang=en-US; Max-Age=0",
        "=31d4d96e",
        "",
    ],
);

/// A domain name (IDNA 2008, by UTS 46's strict processing), lower case,
/// in A-labels, with no dot at the end.
pub const DOMAIN_NAME: StandardType = text_type(
    "DomainName",
    r"[a-z0-9](?:[a-z0-9.\-]*[a-z0-9])?",
    Some(Validator::DomainName),
    None,
    // RFC 2606's reserved names, and münchen.de's A-label.
    &[
        ("example.com", "example.com"),
        ("www.example.co.uk", "www.example.co.uk"),
        ("xn--mnchen-3ya.de", "xn--mnchen-3ya.de"),
        ("localhost", "localhost"),
    ],
    &[
        "Example.com",
        "example.com.",
        "-example.com",
        "exa_mple.com",
        "münchen.de",
        "ab--cd.example",
        "a..example",
        "192.0.2.1",
        "",
    ],
);

/// One fully-qualified emoji (UTS #51), from Unicode's list.
pub const EMOJI_ENTRY: StandardType = text_type(
    "Emoji",
    r"\P{Cc}{1,16}",
    Some(Validator::Emoji),
    None,
    // A face, a thumbs up with a skin tone, a family joined by ZWJ, the
    // United Kingdom's flag and a red heart with its presentation
    // selector.
    &[
        ("\u{1F600}", "\u{1F600}"),
        ("\u{1F44D}\u{1F3FD}", "\u{1F44D}\u{1F3FD}"),
        (
            "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}",
            "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}",
        ),
        ("\u{1F1EC}\u{1F1E7}", "\u{1F1EC}\u{1F1E7}"),
        ("\u{2764}\u{FE0F}", "\u{2764}\u{FE0F}"),
    ],
    &[
        // Unqualified: no presentation selector.
        "\u{2764}",
        // A skin tone on its own is a component.
        "\u{1F3FD}",
        "\u{1F600}\u{1F600}",
        "a",
        ":smile:",
        "",
    ],
);

/// An HTTP field name (RFC 9110 §5.1), a token in lower case, as HTTP/2
/// and HTTP/3 send it, since field names ignore case.
pub const HTTP_HEADER_NAME: StandardType = text_type(
    "HttpHeaderName",
    r"[!#$%&'*+.^_`|~0-9a-z\-]+",
    None,
    None,
    &[
        ("content-type", "content-type"),
        ("accept-language", "accept-language"),
        ("x-request-id", "x-request-id"),
    ],
    &["Content-Type", "content type", "content-type:", ""],
);

/// The methods a route is written with (RFC 9110 §9, RFC 5789), by their
/// tokens.
pub const HTTP_METHOD: StandardType = StandardType {
    name: "HttpMethod",
    holds: Holds::Enum(HTTP_METHODS),
    text: Some(TextForm::Code),
    json: Json::Text,
    storage: Storage::Column(SqlType::Text),
    format: Format {
        key: FormatKey::Canonical,
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: Examples {
        valid: &[("GET", "GET"), ("PATCH", "PATCH"), ("DELETE", "DELETE")],
        format: None,
        malformed: &["FETCH", "HEAD", "GET ", ""],
        of: None,
        settings: "",
    },
    release: FIRST_RELEASE,
};

/// An HTTP response's status code (RFC 9110 §15): three digits from 100
/// to 599, a number, since the registry is open by design.
pub const HTTP_STATUS: StandardType = whole_type(
    "HttpStatus",
    100,
    599,
    &[("200", "200"), ("404", "404"), ("599", "599")],
    &["99", "600", "0200", "2O0", "200.0", "+200", "-200", ""],
);

/// A host: a domain name or an IP address, an IPv6 address without
/// brackets (RFC 3986's `host`).
pub const HOSTNAME: StandardType = text_type(
    "Hostname",
    r"[0-9a-z.:\-]+",
    Some(Validator::Hostname),
    None,
    &[
        ("example.com", "example.com"),
        ("localhost", "localhost"),
        ("192.0.2.1", "192.0.2.1"),
        ("2001:db8::1", "2001:db8::1"),
    ],
    &[
        "Example.com",
        "[2001:db8::1]",
        "2001:DB8::1",
        "192.0.2.01",
        "example.com:80",
        "",
    ],
);

/// A mobile device's IMEI (3GPP TS 23.003 §6.2.1): 15 digits, the last a
/// Luhn check digit; online personal data.
pub const IMEI: StandardType = text_type(
    "Imei",
    r"[0-9]{15}",
    Some(Validator::Luhn),
    Some("online"),
    // Made up to the rules: a type allocation code and a serial with its
    // check digit.
    &[
        ("490154203237518", "490154203237518"),
        ("356938035643809", "356938035643809"),
    ],
    &[
        "490154203237519",
        "49015420323751",
        "4901542032375180",
        "49-015420-323751-8",
        "",
    ],
);

/// A pointer into a JSON document (RFC 6901): `/` before each reference
/// token, `~0` for `~` and `~1` for `/`; empty for the whole document.
pub const JSON_POINTER: StandardType = text_type(
    "JsonPointer",
    r"(?:/(?:[^~/]|~[01])*)*",
    None,
    None,
    // RFC 6901 §5's examples.
    &[
        ("/foo/0", "/foo/0"),
        ("/a~1b", "/a~1b"),
        ("/m~0n", "/m~0n"),
        ("", ""),
    ],
    &["foo", "/a~2b", "/a~", "#/foo"],
);

/// A hardware address (IEEE 802, EUI-48) in IEEE's form: six pairs of
/// upper-case hexadecimal digits joined by `-`; online personal data.
pub const MAC_ADDRESS: StandardType = text_type(
    "MacAddress",
    r"[0-9A-F]{2}(?:-[0-9A-F]{2}){5}",
    None,
    Some("online"),
    // IEEE 802's own example.
    &[
        ("AC-DE-48-00-00-80", "AC-DE-48-00-00-80"),
        ("00-00-5E-00-53-01", "00-00-5E-00-53-01"),
    ],
    &[
        "ac-de-48-00-00-80",
        "AC:DE:48:00:00:80",
        "ACDE48000080",
        "AC-DE-48-00-00",
        "",
    ],
);

/// A TCP or UDP port (IANA's registry): 1 to 65535, port 0 being
/// reserved.
pub const NETWORK_PORT: StandardType = whole_type(
    "NetworkPort",
    1,
    65_535,
    &[("22", "22"), ("443", "443"), ("8080", "8080"), ("65535", "65535")],
    &["0", "65536", "080", "-1", "http", ""],
);

/// An HTTP API's error body (RFC 9457): each of its five members may be
/// left out, as the RFC allows; sent and stored as its object, and shown
/// as its title and detail.
pub const PROBLEM_DETAILS_ENTRY: StandardType = StandardType {
    name: PROBLEM_DETAILS,
    holds: Holds::Object(&PROBLEM_FIELDS),
    text: None,
    json: Json::Members(&["type", "status", "title", "detail", "instance"]),
    // Stored as its JSON, as `InstantRange` is: every member may be
    // `none`, so a column per member couldn't tell an empty problem from
    // no problem at all.
    storage: Storage::Column(SqlType::Json),
    format: Format {
        key: FormatKey::ProblemDetails,
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    // RFC 9457's own example, and problems with fewer members. It has no
    // single text form; its examples are its JSON.
    examples: Examples {
        valid: &[
            (
                r#"{"type":"https://example.com/probs/out-of-credit","status":403,"title":"You do not have enough credit.","detail":"Your current balance is 30, but that costs 50.","instance":"https://example.net/account/12345/msgs/abc"}"#,
                "You do not have enough credit.\nYour current balance is 30, but that costs 50.",
            ),
            (r#"{"status":404,"title":"Not Found"}"#, "Not Found"),
            (r#"{"status":503}"#, "503"),
            (r#"{}"#, "about:blank"),
        ],
        format: None,
        malformed: &[
            // A relative type, a status out of range, an empty title, a
            // member the RFC doesn't define, and not an object.
            r#"{"type":"/probs/out-of-credit"}"#,
            r#"{"status":600}"#,
            r#"{"title":""}"#,
            r#"{"title":"Not Found","balance":30}"#,
            r#""Not Found""#,
        ],
        of: None,
        settings: "",
    },
    release: FIRST_RELEASE,
};

/// A device's push notification token, from Apple's or Google's service:
/// opaque, of the characters their tokens use; online personal data.
pub const PUSH_TOKEN: StandardType = text_type(
    "PushToken",
    r"[A-Za-z0-9_:\-]{1,4096}",
    None,
    Some("online"),
    // Made up in the shapes the services give: APNs' 64 hexadecimal
    // digits, and FCM's registration token.
    &[
        (
            "740f4707bebcf74f9b7c25d48e3358945f6aa01da5ddb387462c7eaf61bb78ad",
            "740f4707bebcf74f9b7c25d48e3358945f6aa01da5ddb387462c7eaf61bb78ad",
        ),
        (
            "bk3RNwTe3H0:CI2k_HHwgIpoDKCIZvvDMExUdFQ3P1",
            "bk3RNwTe3H0:CI2k_HHwgIpoDKCIZvvDMExUdFQ3P1",
        ),
    ],
    &["a token", "token!", ""],
);

/// A relative reference (RFC 3986 §4.2): a path, a query or a fragment,
/// or a host after `//`, with no scheme.
pub const RELATIVE_URL: StandardType = text_type(
    "RelativeUrl",
    r"[A-Za-z0-9\-._~!$&'()*+,;=:@/?#%\[\]]+",
    Some(Validator::RelativeUrl),
    None,
    &[
        ("../images/logo.png", "../images/logo.png"),
        ("/docs?page=2#intro", "/docs?page=2#intro"),
        ("//cdn.example.com/app.js", "//cdn.example.com/app.js"),
        ("?sort=name", "?sort=name"),
        ("#top", "#top"),
        ("caf%C3%A9.html", "caf%C3%A9.html"),
    ],
    &[
        // A scheme makes it a URL.
        "https://example.com/",
        "mailto:someone@example.com",
        "a:b",
        "/a b",
        "/caf%c3%a9",
        "/%zz",
        "//",
        "//user@example.com/",
        "/a#b#c",
        "",
    ],
);

/// A URL-safe identifier: lower-case letters and digits in words joined by
/// single hyphens, as `text.slug` writes one.
pub const SLUG: StandardType = text_type(
    "Slug",
    r"[a-z0-9]+(?:-[a-z0-9]+)*",
    None,
    None,
    &[
        ("hello-world", "hello-world"),
        ("2026-autumn-sale", "2026-autumn-sale"),
        ("a", "a"),
    ],
    &["Hello-World", "hello--world", "-hello", "hello-", "hello_world", ""],
);

/// A URL's path (RFC 3986 `path-absolute`): `/` and segments, with no
/// query or fragment; every one is a `RelativeUrl` too.
pub const URL_PATH: StandardType = text_type(
    "UrlPath",
    concat!("/(?:", pchar!(), "+(?:/", pchar!(), "*)*)?"),
    None,
    None,
    &[
        ("/", "/"),
        ("/products/42", "/products/42"),
        ("/search/caf%C3%A9", "/search/caf%C3%A9"),
        ("/docs/", "/docs/"),
    ],
    &[
        "products/42",
        "//cdn.example.com",
        "/a b",
        "/search?q=1",
        "/caf%c3%a9",
        "",
    ],
);

/// A `User-Agent` (RFC 9110 §10.1.5): products and comments; online
/// personal data, since it helps tell one browser from another.
pub const USER_AGENT: StandardType = text_type(
    "UserAgent",
    r"[\x21-\x7E](?:[\x20-\x7E]*[\x21-\x7E])?",
    Some(Validator::UserAgent),
    Some("online"),
    &[
        ("curl/8.7.1", "curl/8.7.1"),
        (
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36",
        ),
        ("ExampleBot/1.0 (+https://example.com/bot)", "ExampleBot/1.0 (+https://example.com/bot)"),
    ],
    &[
        "(compatible)",
        "curl /8.7.1",
        "Mozilla/5.0  (X11)",
        "Mozilla/5.0 (unclosed",
        "",
    ],
);

/// Where a visit came from, by Google Analytics' UTM convention: a
/// source, a medium and a campaign, and a term and a content where a
/// campaign uses them; written as the query that carries them.
pub const UTM_PARAMETERS_ENTRY: StandardType = StandardType {
    name: UTM_PARAMETERS,
    holds: Holds::Object(&UTM_FIELDS),
    text: Some(TextForm::UtmQuery),
    json: Json::Fields(&["source", "medium", "campaign", "term", "content"]),
    storage: Storage::Columns {
        parts: &[
            ("source", "text"),
            ("medium", "text"),
            ("campaign", "text"),
            ("term", "text"),
            ("content", "text"),
        ],
        fixed: None,
    },
    format: Format {
        key: FormatKey::Canonical,
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: Examples {
        valid: &[
            (
                "utm_source=newsletter&utm_medium=email&utm_campaign=autumn_sale",
                "utm_source=newsletter&utm_medium=email&utm_campaign=autumn_sale",
            ),
            (
                "utm_source=search&utm_medium=cpc&utm_campaign=spring&utm_term=running%20shoes&utm_content=banner",
                "utm_source=search&utm_medium=cpc&utm_campaign=spring&utm_term=running%20shoes&utm_content=banner",
            ),
        ],
        format: None,
        malformed: &[
            // Out of order, a part missing, empty, or not encoded.
            "utm_medium=email&utm_source=newsletter&utm_campaign=autumn_sale",
            "utm_source=newsletter",
            "utm_source=&utm_medium=email&utm_campaign=autumn_sale",
            "utm_source=news letter&utm_medium=email&utm_campaign=autumn_sale",
            "utm_source=newsletter&utm_medium=email&utm_campaign=autumn%5fsale",
            "source=newsletter&medium=email&campaign=autumn_sale",
            "",
        ],
        of: None,
        settings: "",
    },
    release: FIRST_RELEASE,
};

/// A web origin (RFC 6454, as the HTML standard serialises it): `http` or
/// `https`, its host, and a port only where it isn't the default.
pub const WEB_ORIGIN: StandardType = text_type(
    "WebOrigin",
    r"https?://[0-9a-z.:\-\[\]]+",
    Some(Validator::WebOrigin),
    None,
    &[
        ("https://example.com", "https://example.com"),
        ("http://localhost:8080", "http://localhost:8080"),
        ("https://[2001:db8::1]", "https://[2001:db8::1]"),
        ("http://192.0.2.1:3000", "http://192.0.2.1:3000"),
    ],
    &[
        "https://example.com/",
        "https://example.com:443",
        "http://example.com:80",
        "HTTPS://example.com",
        "https://Example.com",
        "ftp://example.com",
        "https://example.com:0",
        "null",
        "",
    ],
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emoji_are_sorted_for_a_binary_search() {
        assert!(EMOJI.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(EMOJI.len(), 3963, "Emoji 18.0's fully-qualified emoji");
    }

    /// RFC 3986 §5.4's examples, against its base.
    #[test]
    fn references_resolve_as_rfc_3986_says() {
        let base = "http://a/b/c/d;p?q";
        let cases = [
            ("g", "http://a/b/c/g"),
            ("./g", "http://a/b/c/g"),
            ("g/", "http://a/b/c/g/"),
            ("/g", "http://a/g"),
            ("//g", "http://g"),
            ("?y", "http://a/b/c/d;p?y"),
            ("g?y", "http://a/b/c/g?y"),
            ("#s", "http://a/b/c/d;p?q#s"),
            ("g#s", "http://a/b/c/g#s"),
            ("g?y#s", "http://a/b/c/g?y#s"),
            (";x", "http://a/b/c/;x"),
            ("g;x", "http://a/b/c/g;x"),
            ("g;x?y#s", "http://a/b/c/g;x?y#s"),
            ("", "http://a/b/c/d;p?q"),
            (".", "http://a/b/c/"),
            ("./", "http://a/b/c/"),
            ("..", "http://a/b/"),
            ("../", "http://a/b/"),
            ("../g", "http://a/b/g"),
            ("../..", "http://a/"),
            ("../../", "http://a/"),
            ("../../g", "http://a/g"),
            // §5.4.2's abnormal examples.
            ("../../../g", "http://a/g"),
            ("../../../../g", "http://a/g"),
            ("/./g", "http://a/g"),
            ("/../g", "http://a/g"),
            ("g.", "http://a/b/c/g."),
            (".g", "http://a/b/c/.g"),
            ("g..", "http://a/b/c/g.."),
            ("..g", "http://a/b/c/..g"),
            ("./../g", "http://a/b/g"),
            ("./g/.", "http://a/b/c/g/"),
            ("g/./h", "http://a/b/c/g/h"),
            ("g/../h", "http://a/b/c/h"),
            ("g;x=1/./y", "http://a/b/c/g;x=1/y"),
            ("g;x=1/../y", "http://a/b/c/y"),
            ("g?y/./x", "http://a/b/c/g?y/./x"),
            ("g?y/../x", "http://a/b/c/g?y/../x"),
            ("g#s/./x", "http://a/b/c/g#s/./x"),
            ("g#s/../x", "http://a/b/c/g#s/../x"),
            ("g:h", "g:h"),
        ];
        for (reference, expected) in cases {
            assert_eq!(resolve(base, reference), expected, "`{reference}`");
            if !reference.is_empty() && reference != "g:h" {
                assert!(is_relative_url(reference), "`{reference}` is relative");
            }
        }
        assert!(!is_relative_url("g:h"));
    }

    #[test]
    fn a_query_goes_before_the_fragment() {
        assert_eq!(
            with_query("https://example.com/search", &[("q", "café au lait"), ("page", "2")]),
            "https://example.com/search?q=caf%C3%A9%20au%20lait&page=2"
        );
        assert_eq!(
            with_query("https://example.com/a?x=1#top", &[("y", "2")]),
            "https://example.com/a?x=1&y=2#top"
        );
        assert_eq!(with_query("https://example.com/?", &[("y", "&")]), "https://example.com/?y=%26");
        assert_eq!(with_query("https://example.com/", &[]), "https://example.com/");
    }

    #[test]
    fn a_range_holds_the_addresses_its_prefix_covers() {
        assert_eq!(cidr_contains("192.0.2.0/24", "192.0.2.255"), Some(true));
        assert_eq!(cidr_contains("192.0.2.0/24", "192.0.3.0"), Some(false));
        assert_eq!(cidr_contains("0.0.0.0/0", "203.0.113.9"), Some(true));
        assert_eq!(cidr_contains("2001:db8::/32", "2001:db8:ffff::1"), Some(true));
        assert_eq!(cidr_contains("2001:db8::/32", "2001:db9::1"), Some(false));
        assert_eq!(cidr_contains("::/0", "192.0.2.1"), Some(false));
        assert_eq!(cidr_contains("198.51.100.7/32", "198.51.100.7"), Some(true));
        assert_eq!(cidr_contains("192.0.2.1/24", "192.0.2.1"), None);
    }

    #[test]
    fn utm_parameters_read_back_their_text() {
        let raw = "utm_source=search&utm_medium=cpc&utm_campaign=spring&utm_term=running%20shoes";
        let parts = read_utm(raw).expect("read");
        assert_eq!(parts[3].as_deref(), Some("running shoes"));
        assert_eq!(parts[4], None);
        let borrowed: Vec<Option<&str>> = parts.iter().map(Option::as_deref).collect();
        assert_eq!(utm_text(&borrowed), raw);
    }

    #[test]
    fn a_cookie_date_s_weekday_is_its_date_s() {
        assert!(cookie_date("Wed, 21 Oct 2026 07:28:00 GMT"));
        assert!(cookie_date("Thu, 01 Jan 1970 00:00:00 GMT"));
        assert!(!cookie_date("Thu, 21 Oct 2026 07:28:00 GMT"));
        assert!(!cookie_date("Wed, 31 Feb 2026 07:28:00 GMT"));
        assert!(!cookie_date("Wed, 21 Oct 2026 24:00:00 GMT"));
        assert!(!cookie_date("Wednesday, 21-Oct-26 07:28:00 GMT"));
    }
}
