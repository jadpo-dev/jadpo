//! The client's address (area 10, "Client addresses"; chosen in chunk
//! 5.1c): which address a request counts as, for the default rate limits
//! (areas 7 and 16), `jadpo dev`'s request lines and the request timeline
//! (area 17).
//!
//! The host gives the address of the connection; the application says,
//! with `client_address:`, which header of a proxy of its own to believe
//! instead ([`ClientAddress`]). With `hops: N` the client is the N-th
//! address from the right of the header, every line of it joined in
//! order: each trusted proxy adds the address it saw on its right, and
//! whatever a caller sends sits to the left of that. A header with fewer
//! than N addresses, or one whose chosen entry is not an address, gives
//! the connection's address instead, never a guess. This is the runtime's
//! and not a host's, so `jadpo dev`, a native deployable and a Workers
//! deployable choose alike; a host only gives the connection's address.
//!
//! Addresses are kept in one spelling, so a client is one count however a
//! proxy writes it: IPv6 in lower case with the zeros shortened, and an
//! IPv4 address written as IPv6 (`::ffff:203.0.113.9`) as IPv4.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use jadpo_std::ClientAddressSource;
use jadpo_std::http::{ClientAddressPlan, Hops};

use super::Request;
use crate::config::ConfigValues;

/// The header `forwarded_for` reads.
const FORWARDED_FOR: &str = "x-forwarded-for";
/// The header `forwarded` reads.
const FORWARDED: &str = "forwarded";
/// The header `cloudflare` reads.
const CLOUDFLARE: &str = "cf-connecting-ip";

/// What the deployable runs on: it decides the source an application that
/// declares none takes, and which sources are accepted at all, and which
/// drivers it can build (chunk 8.1d).
pub use jadpo_std::connections::Target;

/// A source the target does not accept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotAccepted {
    pub source: ClientAddressSource,
    pub target: Target,
}

impl fmt::Display for NotAccepted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "a deployable for Workers reads the client's address from `CF-Connecting-IP` only, not from `{}`",
            self.source.name()
        )
    }
}

/// How a request's client address is chosen: the application's
/// `client_address:` read with the settings it started with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClientAddress {
    source: ClientAddressSource,
    /// Proxies to count from the right; 0 when the source counts none.
    hops: usize,
}

impl Default for ClientAddress {
    /// The native default: the connection's own address.
    fn default() -> ClientAddress {
        ClientAddress::CONNECTION
    }
}

impl ClientAddress {
    /// The connection's own address; no header is read.
    pub const CONNECTION: ClientAddress = ClientAddress {
        source: ClientAddressSource::Connection,
        hops: 0,
    };

    /// What `plan` says on `target`, reading a `hops:` setting from
    /// `values`. No plan is the target's default. A count that cannot be
    /// read (the checker guarantees a setting is an `Int` of at least 1,
    /// so only a value that never arrived) trusts no header: the
    /// connection's address, never a guess. On Workers a source other
    /// than `cloudflare` is refused, for the deployable to report as a
    /// problem with how it starts.
    pub fn read(
        plan: Option<&ClientAddressPlan>,
        values: &ConfigValues,
        target: Target,
    ) -> Result<ClientAddress, NotAccepted> {
        let Some(plan) = plan else {
            return Ok(match target {
                Target::Native => ClientAddress::CONNECTION,
                Target::Workers => ClientAddress {
                    source: ClientAddressSource::Cloudflare,
                    hops: 0,
                },
            });
        };
        if target == Target::Workers && plan.source != ClientAddressSource::Cloudflare {
            return Err(NotAccepted {
                source: plan.source,
                target,
            });
        }
        let counts = matches!(
            plan.source,
            ClientAddressSource::ForwardedFor | ClientAddressSource::Forwarded
        );
        if !counts {
            return Ok(ClientAddress {
                source: plan.source,
                hops: 0,
            });
        }
        let hops = match &plan.hops {
            Some(Hops::Count(count)) => usize::try_from(*count).ok(),
            Some(Hops::Setting(name)) => values
                .text(name)
                .and_then(|text| text.trim().parse::<u32>().ok())
                .and_then(|count| usize::try_from(count).ok()),
            None => None,
        }
        .filter(|count| *count >= 1);
        Ok(match hops {
            Some(hops) => ClientAddress {
                source: plan.source,
                hops,
            },
            None => ClientAddress::CONNECTION,
        })
    }

    /// Where it reads the address.
    pub fn source(&self) -> ClientAddressSource {
        self.source
    }

    /// The address `request` counts as: the client in the header the
    /// source reads, or else the connection's own. `None` when neither is
    /// known, as on Workers with no `CF-Connecting-IP`.
    pub fn of(&self, request: &Request) -> Option<String> {
        let header = match self.source {
            ClientAddressSource::Connection => None,
            ClientAddressSource::ForwardedFor => from_the_right(&forwarded_for(request), self.hops),
            ClientAddressSource::Forwarded => {
                forwarded(request).and_then(|entries| from_the_right(&entries, self.hops))
            }
            ClientAddressSource::Cloudflare => cloudflare(request),
        };
        header
            .map(spell)
            .or_else(|| request.address.as_deref().map(connection))
    }
}

/// The connection's address as the host gave it: in one spelling when it is
/// an address, otherwise as given.
fn connection(address: &str) -> String {
    match address.parse::<IpAddr>() {
        Ok(ip) => spell(ip),
        Err(_) => address.to_string(),
    }
}

/// An address in its one spelling.
fn spell(ip: IpAddr) -> String {
    ip.to_canonical().to_string()
}

/// The entry `hops` places from the right, when there are that many and it
/// is an address.
fn from_the_right(entries: &[Option<IpAddr>], hops: usize) -> Option<IpAddr> {
    if hops == 0 {
        return None;
    }
    entries.iter().rev().nth(hops - 1).copied().flatten()
}

/// Every header line called `name`, in the order received.
fn lines<'a>(request: &'a Request, name: &'a str) -> impl Iterator<Item = &'a str> {
    request
        .headers
        .iter()
        .filter(move |(header, _)| header.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

/// `X-Forwarded-For`: its lines joined, each split at commas. An entry
/// that is no address, an empty one included, keeps its place.
fn forwarded_for(request: &Request) -> Vec<Option<IpAddr>> {
    lines(request, FORWARDED_FOR)
        .flat_map(|line| line.split(','))
        .map(address)
        .collect()
}

/// `Forwarded` (RFC 7239): one entry for each element, the address in its
/// `for` parameter; an element without one is an entry that is no address.
/// `None` when the header does not parse.
fn forwarded(request: &Request) -> Option<Vec<Option<IpAddr>>> {
    let mut entries = Vec::new();
    for line in lines(request, FORWARDED) {
        for element in split(line, ',')? {
            entries.push(element_for(&element)?);
        }
    }
    Some(entries)
}

/// The address a `Forwarded` element's `for` gives; `None` if the element
/// does not parse, and `Some(None)` if it has no `for` or its `for` is no
/// address (`unknown`, `_hidden`).
fn element_for(element: &str) -> Option<Option<IpAddr>> {
    let mut found: Option<Option<IpAddr>> = None;
    for pair in split(element, ';')? {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        let (name, value) = pair.split_once('=')?;
        let name = name.trim();
        if name.is_empty() || name.contains('"') {
            return None;
        }
        let value = value.trim();
        let value = if value.starts_with('"') {
            unquote(value)?
        } else if value.is_empty() || value.contains('"') {
            return None;
        } else {
            value.to_string()
        };
        if name.eq_ignore_ascii_case("for") {
            if found.is_some() {
                return None;
            }
            found = Some(address(&value));
        }
    }
    Some(found.unwrap_or(None))
}

/// `text` split at `separator` outside quoted strings; `None` when a quote
/// is not closed.
fn split(text: &str, separator: char) -> Option<Vec<String>> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for c in text.chars() {
        if quoted {
            current.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                quoted = false;
            }
        } else if c == '"' {
            quoted = true;
            current.push(c);
        } else if c == separator {
            parts.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    if quoted {
        return None;
    }
    parts.push(current);
    Some(parts)
}

/// A quoted string's text, its escapes undone; `None` unless the whole
/// value is one quoted string.
fn unquote(value: &str) -> Option<String> {
    let inner = value.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.push(chars.next()?),
            // An unescaped quote ends the string before its last
            // character: two strings, not one.
            '"' => return None,
            c => out.push(c),
        }
    }
    Some(out)
}

/// `CF-Connecting-IP`: exactly one line holding one address.
fn cloudflare(request: &Request) -> Option<IpAddr> {
    let mut all = lines(request, CLOUDFLARE);
    let line = all.next()?;
    if all.next().is_some() || line.contains(',') {
        return None;
    }
    address(line)
}

/// An address as a proxy writes one: IPv4 or IPv6, in brackets, with a
/// port. A name, `unknown`, an obfuscated identifier, an empty entry and
/// an IPv6 zone are not addresses.
fn address(entry: &str) -> Option<IpAddr> {
    let entry = entry.trim();
    if entry.is_empty() {
        return None;
    }
    if let Some(rest) = entry.strip_prefix('[') {
        let (inside, after) = rest.split_once(']')?;
        if !after.is_empty() {
            port(after.strip_prefix(':')?)?;
        }
        return inside.parse::<Ipv6Addr>().ok().map(IpAddr::V6);
    }
    if let Ok(ip) = entry.parse::<IpAddr>() {
        return Some(ip);
    }
    // IPv4 with a port; an IPv6 address has more than one colon.
    let (host, number) = entry.split_once(':')?;
    port(number)?;
    host.parse::<Ipv4Addr>().ok().map(IpAddr::V4)
}

/// A port: one to five digits, at most 65535.
fn port(text: &str) -> Option<u16> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(headers: &[(&str, &str)], connection: Option<&str>) -> Request {
        Request {
            headers: headers
                .iter()
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .collect(),
            address: connection.map(str::to_string),
            ..Request::default()
        }
    }

    fn reading(source: ClientAddressSource, hops: u32) -> ClientAddress {
        let plan = ClientAddressPlan {
            source,
            hops: match source {
                ClientAddressSource::ForwardedFor | ClientAddressSource::Forwarded => {
                    Some(Hops::Count(hops))
                }
                _ => None,
            },
        };
        ClientAddress::read(Some(&plan), &ConfigValues::default(), Target::Native)
            .expect("accepted")
    }

    fn xff(hops: u32, headers: &[(&str, &str)]) -> Option<String> {
        reading(ClientAddressSource::ForwardedFor, hops)
            .of(&request(headers, Some("10.0.0.1")))
            .filter(|found| found != "10.0.0.1")
    }

    const PROXY: Option<&str> = Some("10.0.0.1");

    // --- the connection ---------------------------------------------------------

    #[test]
    fn the_connection_is_the_default_and_reads_no_header() {
        let found = ClientAddress::default().of(&request(
            &[
                ("X-Forwarded-For", "203.0.113.9"),
                ("CF-Connecting-IP", "198.51.100.4"),
            ],
            PROXY,
        ));
        assert_eq!(found.as_deref(), Some("10.0.0.1"));
        assert_eq!(ClientAddress::default().of(&request(&[], None)), None);
        let declared = reading(ClientAddressSource::Connection, 0);
        assert_eq!(
            declared
                .of(&request(&[("X-Forwarded-For", "203.0.113.9")], PROXY))
                .as_deref(),
            Some("10.0.0.1")
        );
    }

    #[test]
    fn the_connections_address_is_spelled_once() {
        let of = |connection: &str| {
            ClientAddress::default()
                .of(&request(&[], Some(connection)))
                .unwrap_or_default()
        };
        assert_eq!(of("::ffff:203.0.113.9"), "203.0.113.9");
        assert_eq!(of("2001:0DB8:0:0:0:0:0:1"), "2001:db8::1");
        assert_eq!(of("route test"), "route test", "not an address, as given");
    }

    // --- X-Forwarded-For --------------------------------------------------------

    #[test]
    fn the_hops_th_address_from_the_right_is_the_client() {
        let header = [("X-Forwarded-For", "10.9.9.9, 198.51.100.4, 203.0.113.9")];
        assert_eq!(xff(1, &header).as_deref(), Some("203.0.113.9"));
        assert_eq!(xff(2, &header).as_deref(), Some("198.51.100.4"));
        assert_eq!(xff(3, &header).as_deref(), Some("10.9.9.9"));
    }

    #[test]
    fn a_caller_cannot_choose_its_address_by_writing_to_the_left() {
        let honest = [("X-Forwarded-For", "203.0.113.9")];
        let claimed = [("X-Forwarded-For", "192.0.2.77, 203.0.113.9")];
        assert_eq!(xff(1, &honest), xff(1, &claimed));
        assert_eq!(xff(1, &claimed).as_deref(), Some("203.0.113.9"));
    }

    #[test]
    fn a_missing_header_gives_the_connection() {
        let reads = reading(ClientAddressSource::ForwardedFor, 1);
        assert_eq!(reads.of(&request(&[], PROXY)).as_deref(), Some("10.0.0.1"));
        assert_eq!(reads.of(&request(&[], None)), None);
    }

    #[test]
    fn fewer_addresses_than_hops_gives_the_connection() {
        let header = [("X-Forwarded-For", "203.0.113.9")];
        assert_eq!(xff(1, &header).as_deref(), Some("203.0.113.9"));
        assert_eq!(xff(2, &header), None, "one address is short of two proxies");
        let found = reading(ClientAddressSource::ForwardedFor, 2).of(&request(&header, PROXY));
        assert_eq!(found.as_deref(), Some("10.0.0.1"), "never a guess");
    }

    #[test]
    fn an_entry_that_is_not_an_address_gives_the_connection() {
        for bad in [
            "unknown",
            "_hidden",
            "not-an-ip",
            "999.1.1.1",
            "203.0.113",
            "203.0.113.9.1",
            "fe80::1%eth0",
            "203.0.113.9:port",
            "203.0.113.9:99999",
            "[203.0.113.9]",
            "",
            "  ",
        ] {
            assert_eq!(
                xff(1, &[("X-Forwarded-For", bad)]),
                None,
                "{bad:?} is no address"
            );
            assert_eq!(
                xff(1, &[("X-Forwarded-For", &format!("203.0.113.9, {bad}"))]),
                None,
                "{bad:?} on the right is the chosen entry"
            );
        }
    }

    #[test]
    fn an_empty_entry_keeps_its_place_in_the_count() {
        // The gap is the second from the right, not squeezed out: counting
        // two must not slide left to the caller's own claim.
        let header = [("X-Forwarded-For", "192.0.2.77, , 203.0.113.9")];
        assert_eq!(xff(1, &header).as_deref(), Some("203.0.113.9"));
        assert_eq!(xff(2, &header), None);
        assert_eq!(xff(3, &header).as_deref(), Some("192.0.2.77"));
    }

    #[test]
    fn several_lines_join_in_order() {
        let lines = [
            ("X-Forwarded-For", "192.0.2.77"),
            ("x-forwarded-for", "198.51.100.4, 203.0.113.9"),
        ];
        assert_eq!(xff(1, &lines).as_deref(), Some("203.0.113.9"));
        assert_eq!(xff(2, &lines).as_deref(), Some("198.51.100.4"));
        assert_eq!(xff(3, &lines).as_deref(), Some("192.0.2.77"));
    }

    #[test]
    fn ipv6_comes_in_every_spelling_and_in_one() {
        for written in [
            "2001:db8::17",
            "2001:DB8:0:0:0:0:0:17",
            "[2001:db8::17]",
            "[2001:db8::17]:4711",
            "  2001:0db8::0017 ",
        ] {
            assert_eq!(
                xff(
                    1,
                    &[("X-Forwarded-For", &format!("203.0.113.9, {written}"))]
                )
                .as_deref(),
                Some("2001:db8::17"),
                "{written}"
            );
        }
        assert_eq!(
            xff(1, &[("X-Forwarded-For", "::ffff:203.0.113.9")]).as_deref(),
            Some("203.0.113.9"),
            "an IPv4 address written as IPv6 is the IPv4 address"
        );
        assert_eq!(
            xff(1, &[("X-Forwarded-For", "203.0.113.9:4711")]).as_deref(),
            Some("203.0.113.9"),
            "a port is dropped"
        );
        assert_eq!(
            xff(1, &[("X-Forwarded-For", "::1")]).as_deref(),
            Some("::1")
        );
    }

    #[test]
    fn a_name_is_matched_without_case() {
        assert_eq!(
            xff(1, &[("X-FORWARDED-FOR", "203.0.113.9")]).as_deref(),
            Some("203.0.113.9")
        );
    }

    #[test]
    fn another_header_is_not_read() {
        let header = [
            ("Forwarded", "for=192.0.2.60"),
            ("CF-Connecting-IP", "198.51.100.4"),
        ];
        assert_eq!(xff(1, &header), None);
    }

    // --- Forwarded --------------------------------------------------------------

    fn rfc(hops: u32, headers: &[(&str, &str)]) -> Option<String> {
        reading(ClientAddressSource::Forwarded, hops)
            .of(&request(headers, Some("10.0.0.1")))
            .filter(|found| found != "10.0.0.1")
    }

    #[test]
    fn forwarded_gives_the_for_of_the_hops_th_element_from_the_right() {
        let header = [(
            "Forwarded",
            "for=192.0.2.43, for=198.51.100.17;by=203.0.113.60;proto=http;host=example.com",
        )];
        assert_eq!(rfc(1, &header).as_deref(), Some("198.51.100.17"));
        assert_eq!(rfc(2, &header).as_deref(), Some("192.0.2.43"));
        assert_eq!(rfc(3, &header), None, "short");
    }

    #[test]
    fn forwarded_reads_quoted_and_bracketed_forms() {
        for (written, address) in [
            (r#"for="[2001:db8:cafe::17]:4711""#, "2001:db8:cafe::17"),
            (r#"for="[2001:db8:cafe::17]""#, "2001:db8:cafe::17"),
            (r#"for="192.0.2.43:47011""#, "192.0.2.43"),
            (r#"for="192.0.2.43""#, "192.0.2.43"),
            ("for=192.0.2.43", "192.0.2.43"),
            ("FOR=192.0.2.43", "192.0.2.43"),
            ("For = 192.0.2.43", "192.0.2.43"),
            (
                r#"proto=https; for="192.0.2.43"; by=_gateway"#,
                "192.0.2.43",
            ),
            (r#"by="[::1]:80";for="[::ffff:192.0.2.43]""#, "192.0.2.43"),
        ] {
            assert_eq!(
                rfc(1, &[("Forwarded", written)]).as_deref(),
                Some(address),
                "{written}"
            );
        }
    }

    #[test]
    fn forwarded_keeps_commas_and_semicolons_inside_quotes() {
        // The quoted `by` holds a comma and a semicolon; it is one element.
        let header = [(
            "Forwarded",
            r#"for=192.0.2.43;by="a,b;c", for=198.51.100.17"#,
        )];
        assert_eq!(rfc(1, &header).as_deref(), Some("198.51.100.17"));
        assert_eq!(rfc(2, &header).as_deref(), Some("192.0.2.43"));
        let escaped = [("Forwarded", r#"by="a\",b";for=192.0.2.43"#)];
        assert_eq!(rfc(1, &escaped).as_deref(), Some("192.0.2.43"));
    }

    #[test]
    fn forwarded_elements_without_an_address_keep_their_place() {
        for (written, hops, found) in [
            ("for=192.0.2.43, by=203.0.113.60", 1, None),
            ("for=192.0.2.43, by=203.0.113.60", 2, Some("192.0.2.43")),
            ("for=192.0.2.43, for=unknown", 1, None),
            ("for=192.0.2.43, for=_hidden", 1, None),
            ("for=192.0.2.43, for=_hidden", 2, Some("192.0.2.43")),
            ("for=192.0.2.43, , for=198.51.100.17", 2, None),
            ("for=192.0.2.43, , for=198.51.100.17", 3, Some("192.0.2.43")),
        ] {
            assert_eq!(
                rfc(hops, &[("Forwarded", written)]).as_deref(),
                found,
                "{written} with {hops}"
            );
        }
    }

    #[test]
    fn a_forwarded_header_that_does_not_parse_gives_the_connection() {
        for bad in [
            r#"for="192.0.2.43"#,
            r#"for="192.0.2.43" extra"#,
            r#"for="192.0.2.43""x""#,
            "for",
            "for=",
            "=192.0.2.43",
            "for=192.0.2.43;for=198.51.100.17",
            r#"for=192.0.2"43"#,
            "192.0.2.43",
        ] {
            assert_eq!(rfc(1, &[("Forwarded", bad)]), None, "{bad}");
        }
        // One bad line spoils the lot: the good line is not trusted alone.
        let lines = [("Forwarded", "for=192.0.2.43"), ("Forwarded", r#"for="x"#)];
        assert_eq!(rfc(1, &lines), None);
    }

    #[test]
    fn several_forwarded_lines_join_in_order() {
        let lines = [
            ("Forwarded", "for=192.0.2.43"),
            ("forwarded", "for=198.51.100.17, for=203.0.113.9"),
        ];
        assert_eq!(rfc(1, &lines).as_deref(), Some("203.0.113.9"));
        assert_eq!(rfc(3, &lines).as_deref(), Some("192.0.2.43"));
    }

    // --- Cloudflare -------------------------------------------------------------

    fn cf(headers: &[(&str, &str)], connection: Option<&str>) -> Option<String> {
        reading(ClientAddressSource::Cloudflare, 0).of(&request(headers, connection))
    }

    #[test]
    fn cloudflare_gives_the_one_address_in_its_header() {
        assert_eq!(
            cf(&[("CF-Connecting-IP", "203.0.113.9")], PROXY).as_deref(),
            Some("203.0.113.9")
        );
        assert_eq!(
            cf(&[("cf-connecting-ip", "2001:DB8::17")], None).as_deref(),
            Some("2001:db8::17")
        );
        assert_eq!(
            cf(&[("X-Forwarded-For", "192.0.2.1")], PROXY).as_deref(),
            Some("10.0.0.1"),
            "no other header counts"
        );
    }

    #[test]
    fn cloudflare_falls_back_when_the_header_is_not_exactly_one_address() {
        for headers in [
            vec![],
            vec![("CF-Connecting-IP", "unknown")],
            vec![("CF-Connecting-IP", "203.0.113.9, 198.51.100.4")],
            vec![
                ("CF-Connecting-IP", "203.0.113.9"),
                ("CF-Connecting-IP", "198.51.100.4"),
            ],
            vec![("CF-Connecting-IP", "")],
        ] {
            assert_eq!(
                cf(&headers, PROXY).as_deref(),
                Some("10.0.0.1"),
                "{headers:?}"
            );
            assert_eq!(
                cf(&headers, None),
                None,
                "{headers:?} with nothing to fall back to"
            );
        }
    }

    // --- what the plan and the target say ---------------------------------------

    fn plan(source: ClientAddressSource, hops: Option<Hops>) -> ClientAddressPlan {
        ClientAddressPlan { source, hops }
    }

    #[test]
    fn a_plan_with_no_declaration_takes_the_targets_default() {
        let values = ConfigValues::default();
        assert_eq!(
            ClientAddress::read(None, &values, Target::Native),
            Ok(ClientAddress::CONNECTION)
        );
        let on_workers = ClientAddress::read(None, &values, Target::Workers).expect("accepted");
        assert_eq!(on_workers.source(), ClientAddressSource::Cloudflare);
        assert_eq!(
            on_workers
                .of(&request(&[("CF-Connecting-IP", "203.0.113.9")], None))
                .as_deref(),
            Some("203.0.113.9")
        );
    }

    #[test]
    fn workers_accept_cloudflare_and_nothing_else() {
        let values = ConfigValues::default();
        for source in [
            ClientAddressSource::Connection,
            ClientAddressSource::ForwardedFor,
            ClientAddressSource::Forwarded,
        ] {
            let hops = matches!(
                source,
                ClientAddressSource::ForwardedFor | ClientAddressSource::Forwarded
            )
            .then_some(Hops::Count(1));
            let refused =
                ClientAddress::read(Some(&plan(source, hops.clone())), &values, Target::Workers);
            assert_eq!(
                refused,
                Err(NotAccepted {
                    source,
                    target: Target::Workers
                }),
                "{source:?}"
            );
            assert!(
                ClientAddress::read(Some(&plan(source, hops)), &values, Target::Native).is_ok()
            );
        }
        assert!(
            ClientAddress::read(
                Some(&plan(ClientAddressSource::Cloudflare, None)),
                &values,
                Target::Workers
            )
            .is_ok()
        );
        let message = NotAccepted {
            source: ClientAddressSource::ForwardedFor,
            target: Target::Workers,
        }
        .to_string();
        assert!(message.contains("forwarded_for") && message.contains("CF-Connecting-IP"));
    }

    #[test]
    fn hops_come_from_a_setting() {
        let plan = plan(
            ClientAddressSource::ForwardedFor,
            Some(Hops::Setting("proxy_hops".into())),
        );
        let header = [("X-Forwarded-For", "198.51.100.4, 203.0.113.9")];
        let of = |values: &[(&str, &str)]| {
            ClientAddress::read(Some(&plan), &ConfigValues::for_test(values), Target::Native)
                .expect("accepted")
                .of(&request(&header, PROXY))
        };
        assert_eq!(of(&[("proxy_hops", "1")]).as_deref(), Some("203.0.113.9"));
        assert_eq!(of(&[("proxy_hops", "2")]).as_deref(), Some("198.51.100.4"));
        assert_eq!(
            of(&[("proxy_hops", "3")]).as_deref(),
            Some("10.0.0.1"),
            "short"
        );
        for unreadable in ["0", "-1", "two", "", "1.5"] {
            assert_eq!(
                of(&[("proxy_hops", unreadable)]).as_deref(),
                Some("10.0.0.1"),
                "{unreadable:?} trusts no header"
            );
        }
        assert_eq!(
            of(&[]).as_deref(),
            Some("10.0.0.1"),
            "no value, no header trusted"
        );
    }
}
