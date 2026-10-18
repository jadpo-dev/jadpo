//! `Certificate` (area 19, "Security records", chunk 5.5r5): an X.509
//! certificate (RFC 5280) read from its PEM (RFC 7468), its parts read from
//! the DER and never set by code.
//!
//! Reading is strict, so no two readers take different certificates from
//! the same text: the PEM is one `CERTIFICATE` block and nothing else; the
//! DER is DER throughout, every length definite and in its fewest bytes,
//! every integer in its fewest octets, every time in RFC 5280's one form,
//! and nothing after it; Rusticata's `x509-parser` then reads it as a
//! certificate, whose serial number must be positive and at most 20
//! octets, and whose two signature algorithms must be the same. Its chain
//! and its signature aren't checked.

use std::rc::Rc;

use jadpo_std::standard::certificates::{
    CERTIFICATE, CERTIFICATE_FIELDS, DER_MAX_BYTES, PEM_LINE, SERIAL_MAX_OCTETS,
};
use x509_parser::certificate::X509Certificate;
use x509_parser::der_parser::asn1_rs::{Any, Tag, ToDer};
use x509_parser::prelude::FromDer;
use x509_parser::x509::{AttributeTypeAndValue, X509Name};

use crate::{Fault, Record, Value};

const BEGIN: &str = "-----BEGIN CERTIFICATE-----";
const END: &str = "-----END CERTIFICATE-----";

/// How deep DER may nest: far more than a certificate does, so a crafted
/// one can't exhaust the stack.
const DEPTH_MAX: usize = 32;

/// A `Certificate` read from its PEM; `None` for anything else.
pub fn read_pem(raw: &str) -> Option<Value> {
    read_der(&pem_der(raw)?)
}

/// A `Certificate` read from its DER; `None` for anything else.
pub fn read_der(der: &[u8]) -> Option<Value> {
    let parts = parts(der)?;
    let values = [
        Value::text(&parts.subject),
        Value::text(&parts.issuer),
        Value::text(&parts.serial_number),
        Value::Instant(parts.not_before),
        Value::Instant(parts.not_after),
        Value::Bytes(Rc::from(crate::hash::sha256(der).as_slice())),
        Value::Bytes(Rc::from(der)),
    ];
    Some(Value::record(
        CERTIFICATE,
        CERTIFICATE_FIELDS
            .iter()
            .zip(values)
            .map(|((name, _), value)| ((*name).into(), value))
            .collect(),
    ))
}

/// A certificate's canonical text: its DER as one PEM block.
pub fn pem_text(record: &Record) -> Result<String, Fault> {
    let Some(Value::Bytes(der)) = record.get("der") else {
        return Err(Fault::Shape);
    };
    Ok(pem(der))
}

/// `der` as one `CERTIFICATE` block of 64-character lines, each ending
/// in a line feed (RFC 7468 §2).
pub fn pem(der: &[u8]) -> String {
    let body = crate::encoding::to_base64(der);
    let mut out = String::with_capacity(body.len() + body.len() / PEM_LINE + 60);
    out.push_str(BEGIN);
    out.push('\n');
    let mut rest = body.as_str();
    while !rest.is_empty() {
        let (line, after) = rest.split_at(rest.len().min(PEM_LINE));
        out.push_str(line);
        out.push('\n');
        rest = after;
    }
    out.push_str(END);
    out.push('\n');
    out
}

/// The DER in one `CERTIFICATE` block and nothing else (RFC 7468's strict
/// form): its lines end in line feeds, or all in carriage returns and line
/// feeds, the last line's ending optional; each Base64 line is 64
/// characters but the last, which is 1 to 64. A key's block, another
/// label, explanatory text, a second block, headers and white space are
/// refused.
fn pem_der(raw: &str) -> Option<Vec<u8>> {
    // The Base64 of the largest DER, two octets of line ending for every
    // 64 characters of it, and its two labels.
    if raw.len() > (DER_MAX_BYTES / 3 + 1) * 4 * (PEM_LINE + 2) / PEM_LINE + 128 {
        return None;
    }
    let eol = if raw.contains('\r') { "\r\n" } else { "\n" };
    let text = raw.strip_suffix(eol).unwrap_or(raw);
    let lines: Vec<&str> = text.split(eol).collect();
    let [first, body @ .., end] = lines.as_slice() else {
        return None;
    };
    if *first != BEGIN || *end != END {
        return None;
    }
    let (last, full) = body.split_last()?;
    let alphabet = |line: &str| {
        line.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/' || b == b'=')
    };
    let shaped = full
        .iter()
        .all(|line| line.len() == PEM_LINE && alphabet(line))
        && (1..=PEM_LINE).contains(&last.len())
        && alphabet(last);
    if !shaped {
        return None;
    }
    let der = crate::encoding::from_base64(&body.concat())?;
    (der.len() <= DER_MAX_BYTES).then_some(der)
}

/// What a certificate's DER says, as its parts hold it.
struct Parts {
    subject: String,
    issuer: String,
    serial_number: String,
    not_before: i64,
    not_after: i64,
}

/// The parts of the certificate `der` is, read strictly; `None` for DER
/// that isn't one.
fn parts(der: &[u8]) -> Option<Parts> {
    if der.is_empty() || der.len() > DER_MAX_BYTES || !strict(der) {
        return None;
    }
    let (rest, certificate) = X509Certificate::from_der(der).ok()?;
    if !rest.is_empty() {
        return None;
    }
    // The algorithm outside what was signed is the one inside (RFC 5280
    // §4.1.1.2), each compared as its DER.
    let (whole, _) = element(der)?;
    let outer = top_level(whole.content)?;
    let tbs = top_level(outer.first()?.content)?;
    let at = usize::from(tbs.first()?.tag == 0xa0);
    let serial = tbs.get(at)?;
    let inner_algorithm = tbs.get(at + 1)?;
    if outer.get(1)?.whole != inner_algorithm.whole {
        return None;
    }
    let tbs_certificate = &certificate.tbs_certificate;
    if serial.content != tbs_certificate.raw_serial() {
        return None;
    }
    Some(Parts {
        subject: distinguished_name(&tbs_certificate.subject)?,
        issuer: distinguished_name(&tbs_certificate.issuer)?,
        serial_number: serial_number(serial.content)?,
        not_before: instant(tbs_certificate.validity.not_before.timestamp())?,
        not_after: instant(tbs_certificate.validity.not_after.timestamp())?,
    })
}

/// A positive serial number of at most 20 octets (RFC 5280 §4.1.2.2) in
/// lowercase hexadecimal, without its sign octet.
fn serial_number(octets: &[u8]) -> Option<String> {
    let (first, _) = octets.split_first()?;
    if octets.len() > SERIAL_MAX_OCTETS || first & 0x80 != 0 || octets.iter().all(|b| *b == 0) {
        return None;
    }
    let value = match octets {
        [0, rest @ ..] => rest,
        _ => octets,
    };
    Some(crate::encoding::to_hex(value))
}

/// An instant from a time's seconds since 1970. GeneralizedTime's
/// `99991231235959Z`, which RFC 5280 §4.1.2.5 writes for a certificate with
/// no well-defined expiry, is past the latest instant Jadpo holds, so a
/// time in the last hours of 9999 reads as that instant (chosen in chunk
/// 5.5r5).
fn instant(seconds: i64) -> Option<i64> {
    let millis = seconds.checked_mul(1000)?;
    if millis < jadpo_std::INSTANT_MIN {
        return None;
    }
    Some(millis.min(jadpo_std::INSTANT_MAX))
}

/// A distinguished name as RFC 4514 writes it: its relative names last
/// first, joined by `,`, the attributes of one joined by `+`.
fn distinguished_name(name: &X509Name<'_>) -> Option<String> {
    let mut relative = name
        .iter()
        .map(|rdn| {
            rdn.iter()
                .map(attribute)
                .collect::<Option<Vec<_>>>()
                .map(|attributes| attributes.join("+"))
        })
        .collect::<Option<Vec<_>>>()?;
    relative.reverse();
    Some(relative.join(","))
}

/// One attribute as RFC 4514 §2.3 writes it: the short name RFC 4514 §3
/// gives its type, else its dotted OID; then its value as text, escaped,
/// where the type has a short name and the value is a string of a known
/// kind, else `#` and the hexadecimal of its DER.
fn attribute(attribute: &AttributeTypeAndValue<'_>) -> Option<String> {
    let oid = attribute.attr_type().to_id_string();
    let value = attribute.attr_value();
    let short = SHORT_NAMES
        .iter()
        .find(|(dotted, _)| *dotted == oid)
        .map(|(_, short)| *short);
    Some(match (short, string_value(value)?) {
        (Some(short), Some(text)) => format!("{short}={}", escape(&text)),
        (Some(short), None) => format!("{short}=#{}", der_hex(value)?),
        (None, _) => format!("{oid}=#{}", der_hex(value)?),
    })
}

/// The attribute types RFC 4514 §3 names, by their OIDs.
const SHORT_NAMES: &[(&str, &str)] = &[
    ("2.5.4.3", "CN"),
    ("2.5.4.7", "L"),
    ("2.5.4.8", "ST"),
    ("2.5.4.10", "O"),
    ("2.5.4.11", "OU"),
    ("2.5.4.6", "C"),
    ("2.5.4.9", "STREET"),
    ("0.9.2342.19200300.100.1.25", "DC"),
    ("0.9.2342.19200300.100.1.1", "UID"),
];

/// The text a string value holds: `Some(None)` for a value of another
/// kind, written as its DER; `None` for a string that isn't its kind's,
/// which makes the certificate malformed.
fn string_value(value: &Any<'_>) -> Option<Option<String>> {
    let data = value.data;
    let ascii = |allowed: fn(u8) -> bool| {
        data.iter()
            .all(|b| allowed(*b))
            .then(|| data.iter().map(|b| char::from(*b)).collect::<String>())
    };
    let tag = value.tag();
    if value.header.is_constructed() {
        return Some(None);
    }
    Some(Some(if tag == Tag::Utf8String {
        String::from_utf8(data.to_vec()).ok()?
    } else if tag == Tag::PrintableString {
        ascii(|b| b.is_ascii_alphanumeric() || b" '()+,-./:=?".contains(&b))?
    } else if tag == Tag::Ia5String {
        ascii(|b| b.is_ascii())?
    } else if tag == Tag::NumericString {
        ascii(|b| b.is_ascii_digit() || b == b' ')?
    } else if tag == Tag::VisibleString {
        ascii(|b| (0x20..0x7f).contains(&b))?
    } else if tag == Tag::BmpString {
        if data.len() % 2 != 0 {
            return None;
        }
        let units = data
            .chunks_exact(2)
            .map(|pair| Some(u16::from_be_bytes(pair.try_into().ok()?)))
            .collect::<Option<Vec<u16>>>()?;
        char::decode_utf16(units)
            .collect::<Result<String, _>>()
            .ok()?
    } else if tag == Tag::UniversalString {
        if data.len() % 4 != 0 {
            return None;
        }
        data.chunks_exact(4)
            .map(|quad| {
                let bytes: [u8; 4] = quad.try_into().ok()?;
                char::from_u32(u32::from_be_bytes(bytes))
            })
            .collect::<Option<String>>()?
    } else {
        return Some(None);
    }))
}

/// A value as RFC 4514 §2.4 escapes it: `"`, `+`, `,`, `;`, `<`, `>` and
/// `\` after a `\`, a leading space or `#` and a trailing space likewise,
/// and every control character as `\` and the hexadecimal of its UTF-8
/// octets, so a name read from outside shows on one line.
fn escape(text: &str) -> String {
    let count = text.chars().count();
    let mut out = String::with_capacity(text.len());
    for (at, c) in text.chars().enumerate() {
        let edge = (at == 0 && (c == ' ' || c == '#')) || (at + 1 == count && c == ' ');
        if edge || "\"+,;<>\\".contains(c) {
            out.push('\\');
            out.push(c);
        } else if c.is_control() {
            let mut buffer = [0u8; 4];
            for byte in c.encode_utf8(&mut buffer).bytes() {
                out.push('\\');
                out.push_str(&crate::encoding::to_hex(&[byte]));
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// A value's DER in hexadecimal, as RFC 4514 §2.4 writes one it has no
/// string form for.
fn der_hex(value: &Any<'_>) -> Option<String> {
    value
        .to_der_vec()
        .ok()
        .map(|bytes| crate::encoding::to_hex(&bytes))
}

/// One DER element: its identifier octet, the whole element, and its
/// contents.
#[derive(Clone, Copy)]
struct Element<'a> {
    tag: u8,
    whole: &'a [u8],
    content: &'a [u8],
}

/// The element at the start of `input`, its length definite and in its
/// fewest octets, and what follows it; `None` for a tag number of 31 or
/// more, which no certificate uses, or a length DER doesn't write.
fn element(input: &[u8]) -> Option<(Element<'_>, &[u8])> {
    let (&tag, rest) = input.split_first()?;
    if tag & 0x1f == 0x1f {
        return None;
    }
    let (&first, rest) = rest.split_first()?;
    let (length, rest) = match first {
        0..=0x7f => (usize::from(first), rest),
        0x81..=0x84 => {
            let count = usize::from(first & 0x7f);
            let octets = rest.get(..count)?;
            if octets.first() == Some(&0) {
                return None;
            }
            let length = octets
                .iter()
                .fold(0usize, |length, octet| (length << 8) | usize::from(*octet));
            if length < 0x80 {
                return None;
            }
            (length, rest.get(count..)?)
        }
        // Indefinite (BER only), or longer than any certificate.
        _ => return None,
    };
    let content = rest.get(..length)?;
    let header = input.len() - rest.len();
    let whole = input.get(..header + length)?;
    Some((
        Element {
            tag,
            whole,
            content,
        },
        rest.get(length..)?,
    ))
}

/// The elements `content` is made of, in order; `None` unless it is
/// exactly a run of them.
fn top_level(content: &[u8]) -> Option<Vec<Element<'_>>> {
    let mut out = Vec::new();
    let mut rest = content;
    while !rest.is_empty() {
        let (found, after) = element(rest)?;
        out.push(found);
        rest = after;
    }
    Some(out)
}

/// Whether `der` is exactly one element and DER throughout.
fn strict(der: &[u8]) -> bool {
    match element(der) {
        Some((found, [])) => strict_element(found, 0),
        _ => false,
    }
}

/// Whether one element is DER: a constructed one's contents are elements
/// that are; a universal primitive one's contents are in their one form.
fn strict_element(found: Element<'_>, depth: usize) -> bool {
    let constructed = found.tag & 0x20 != 0;
    let universal = found.tag & 0xc0 == 0;
    let number = found.tag & 0x1f;
    let content = found.content;
    if constructed {
        // In DER only a SEQUENCE or a SET is constructed among the
        // universal types; a string is never sent in pieces.
        if (universal && number != 0x10 && number != 0x11) || depth >= DEPTH_MAX {
            return false;
        }
        return top_level(content)
            .is_some_and(|inner| inner.into_iter().all(|e| strict_element(e, depth + 1)));
    }
    if !universal {
        return true;
    }
    match number {
        // A BOOLEAN is one octet, all ones or all zeros.
        0x01 => matches!(content, [0x00] | [0xff]),
        // An INTEGER or an ENUMERATED in its fewest octets.
        0x02 | 0x0a => match content {
            [] => false,
            [0x00, next, ..] => next & 0x80 != 0,
            [0xff, next, ..] => next & 0x80 == 0,
            _ => true,
        },
        // A BIT STRING: its unused bits counted and zero.
        0x03 => match content.split_first() {
            Some((&unused, [])) => unused == 0,
            Some((&unused, bits)) => {
                unused <= 7
                    && bits
                        .last()
                        .is_some_and(|last| last & ((1u8 << unused) - 1) == 0)
            }
            None => false,
        },
        // NULL is empty.
        0x05 => content.is_empty(),
        // An OBJECT IDENTIFIER: each arc in its fewest octets, the last
        // ending.
        0x06 => {
            let mut starts = true;
            for &octet in content {
                if starts && octet == 0x80 {
                    return false;
                }
                starts = octet & 0x80 == 0;
            }
            !content.is_empty() && starts
        }
        // A SEQUENCE or a SET is never primitive.
        0x10 | 0x11 => false,
        // RFC 5280 §4.1.2.5: `YYMMDDHHMMSSZ` and `YYYYMMDDHHMMSSZ`.
        0x17 => time_digits(content, 12),
        0x18 => time_digits(content, 14),
        _ => true,
    }
}

/// Whether a time is `digits` digits and `Z`.
fn time_digits(content: &[u8], digits: usize) -> bool {
    content.len() == digits + 1
        && content.last() == Some(&b'Z')
        && content
            .get(..digits)
            .is_some_and(|all| all.iter().all(u8::is_ascii_digit))
}

#[cfg(test)]
mod tests {
    use super::*;
    use jadpo_std::standard::certificates as fixtures;

    /// The self-signed certificate's parts, as OpenSSL prints them.
    #[test]
    fn a_self_signed_certificate_is_read_with_its_parts() {
        let Some(Value::Record(record)) = read_pem(fixtures::SELF_SIGNED) else {
            panic!("reads");
        };
        let text = |name: &str| match record.get(name) {
            Some(Value::Text(text)) => text.to_string(),
            other => panic!("{name}: {other:?}"),
        };
        assert_eq!(
            text("subject"),
            "CN=jadpo-test.example,O=Jadpo Test\\, Signing,C=GB"
        );
        assert_eq!(text("issuer"), text("subject"));
        assert_eq!(text("serial_number"), "1f2e3d4c5b6a7988");
        assert_eq!(
            record.get("not_before"),
            Some(&Value::Instant(
                jadpo_std::instant_millis("2026-10-01T00:00:00Z").unwrap()
            ))
        );
        assert_eq!(
            record.get("not_after"),
            Some(&Value::Instant(
                jadpo_std::instant_millis("2036-10-01T00:00:00Z").unwrap()
            ))
        );
        let Some(Value::Bytes(fingerprint)) = record.get("fingerprint") else {
            panic!("a fingerprint");
        };
        assert_eq!(
            crate::encoding::to_hex(fingerprint),
            "d7c31adc4023149797ca76975893830076e7087afdd200ecb6d133609a975a0d"
        );
        assert_eq!(pem_text(&record).as_deref(), Ok(fixtures::SELF_SIGNED));
    }

    /// ISRG Root X1, as `openssl x509 -nameopt RFC2253` prints it: its
    /// serial's sign octet dropped, and its SHA-256 fingerprint.
    #[test]
    fn a_public_root_is_read_as_openssl_reads_it() {
        let Some(Value::Record(record)) = read_pem(fixtures::ISRG_ROOT_X1) else {
            panic!("reads");
        };
        assert_eq!(
            record.get("subject"),
            Some(&Value::text(
                "CN=ISRG Root X1,O=Internet Security Research Group,C=US"
            ))
        );
        assert_eq!(
            record.get("serial_number"),
            Some(&Value::text("8210cfb0d240e3594463e0bb63828b00"))
        );
        assert_eq!(
            record.get("not_after"),
            Some(&Value::Instant(
                jadpo_std::instant_millis("2035-06-04T11:04:38Z").unwrap()
            ))
        );
        let Some(Value::Bytes(fingerprint)) = record.get("fingerprint") else {
            panic!("a fingerprint");
        };
        assert_eq!(
            crate::encoding::to_hex(fingerprint),
            "96bcec06264976f37460779acf28c5a7cfe8a3c0aae11a8ffcee05c0bddf08c6"
        );
        assert_eq!(pem_text(&record).as_deref(), Ok(fixtures::ISRG_ROOT_X1));
    }

    /// Lines ending in carriage returns and line feeds read as the same
    /// certificate; the last line's ending may be left off.
    #[test]
    fn line_endings_are_read_either_way() {
        let crlf = fixtures::SELF_SIGNED.replace('\n', "\r\n");
        assert_eq!(read_pem(&crlf), read_pem(fixtures::SELF_SIGNED));
        let bare = fixtures::SELF_SIGNED.trim_end_matches('\n');
        assert_eq!(read_pem(bare), read_pem(fixtures::SELF_SIGNED));
        // Mixed endings, a blank line, a space and a second line ending
        // aren't.
        let mixed = crlf.replacen("\r\n", "\n", 2);
        let blank = fixtures::SELF_SIGNED.replacen('\n', "\n\n", 1);
        let spaced = fixtures::SELF_SIGNED.replacen('\n', " \n", 2);
        let doubled = format!("{}\n", fixtures::SELF_SIGNED);
        for refused in [mixed, blank, spaced, doubled] {
            assert_eq!(read_pem(&refused), None, "{refused:?}");
        }
    }

    /// A key's PEM is never a certificate, and nothing of it is kept.
    #[test]
    fn a_key_block_is_refused() {
        // RFC 8410 §10.3's Ed25519 private key.
        let label = format!("{} KEY", "PRIVATE");
        let key = format!(
            "-----BEGIN {label}-----\nMC4CAQAwBQYDK2VwBCIEINTuctv5E1hK1bbY8fdp+K06/nwoy/HU++CXqI9EdVhC\n-----END {label}-----\n"
        );
        assert_eq!(read_pem(&key), None);
        assert_eq!(read_pem(fixtures::PUBLIC_KEY_BLOCK), None);
        // A key's block before the certificate's is two blocks.
        assert_eq!(read_pem(&format!("{key}{}", fixtures::SELF_SIGNED)), None);
    }

    /// RFC 5280's serial numbers: positive, at most 20 octets.
    #[test]
    fn a_serial_is_positive_and_at_most_twenty_octets() {
        assert_eq!(
            serial_number(&[0x7f; 20]).as_deref(),
            Some(&*"7f".repeat(20))
        );
        assert_eq!(serial_number(&[0x00, 0x80]).as_deref(), Some("80"));
        assert_eq!(serial_number(&[0x7f; 21]), None);
        assert_eq!(serial_number(&[0xfb]), None);
        assert_eq!(serial_number(&[0x00]), None);
        assert_eq!(serial_number(&[]), None);
        for refused in [
            fixtures::NEGATIVE_SERIAL,
            fixtures::ZERO_SERIAL,
            fixtures::OVERSIZED_SERIAL,
        ] {
            assert!(pem_der(refused).is_some() && read_pem(refused).is_none());
        }
    }

    /// DER's one form: what BER allows besides is refused.
    #[test]
    fn only_der_is_read() {
        let der = pem_der(fixtures::SELF_SIGNED).unwrap();
        assert!(strict(&der));
        // An indefinite length, a length in more octets than it needs, an
        // integer with a needless leading octet, a BOOLEAN of 0x01, a
        // constructed string and a time without its seconds.
        for refused in [
            &[0x30, 0x80, 0x00, 0x00][..],
            &[0x30, 0x81, 0x03, 0x02, 0x01, 0x05],
            &[0x30, 0x04, 0x02, 0x02, 0x00, 0x05],
            &[0x30, 0x03, 0x01, 0x01, 0x01],
            &[0x30, 0x06, 0x24, 0x04, 0x04, 0x02, 0x41, 0x42],
            &[
                0x30, 0x0d, 0x17, 0x0b, b'2', b'6', b'1', b'0', b'0', b'1', b'0', b'0', b'0', b'0',
                b'Z',
            ],
        ] {
            assert!(!strict(refused), "{refused:02x?}");
        }
        assert!(strict(&[0x30, 0x03, 0x01, 0x01, 0xff]));
        for refused in [
            fixtures::TRAILING_BYTE,
            fixtures::TRUNCATED,
            fixtures::LONG_LENGTH,
            fixtures::OTHER_ALGORITHM,
        ] {
            assert_eq!(read_pem(refused), None);
        }
    }

    /// RFC 4514's escapes, and a value of a kind with no string form
    /// written as its DER.
    #[test]
    fn names_are_written_as_rfc_4514_writes_them() {
        assert_eq!(escape("Jadpo Test, Signing"), "Jadpo Test\\, Signing");
        assert_eq!(escape(" #a+b "), "\\ #a\\+b\\ ");
        assert_eq!(escape("#x"), "\\#x");
        assert_eq!(escape("a<b>;\"c\\"), "a\\<b\\>\\;\\\"c\\\\");
        assert_eq!(escape("line\nbreak\0"), "line\\0abreak\\00");
        assert_eq!(escape("Zoë"), "Zoë");
    }

    /// A certificate with no well-defined expiry (RFC 5280 §4.1.2.5) runs
    /// to the latest instant Jadpo holds.
    #[test]
    fn no_expiry_is_the_latest_instant() {
        let forever = concat!(
            "-----BEGIN CERTIFICATE-----\n",
            "MIIBKDCB26ADAgECAgEqMAUGAytlcDAUMRIwEAYDVQQDDAlubyBleHBpcnkwIBcN\n",
            "MjYxMDAxMDAwMDAwWhgPOTk5OTEyMzEyMzU5NTlaMBQxEjAQBgNVBAMMCW5vIGV4\n",
            "cGlyeTAqMAUGAytlcAMhAACckzsX5IApkzmIZfwFxDEaU9O0Kg86QWSEQkhEsYWH\n",
            "o1AwTjAdBgNVHQ4EFgQUx+AK5bbnumhLqZfqKl0aCGaLOi0wHwYDVR0jBBgwFoAU\n",
            "x+AK5bbnumhLqZfqKl0aCGaLOi0wDAYDVR0TAQH/BAIwADAFBgMrZXADQQBmNe4o\n",
            "8tzxQ+b7f7vGoh++h67IGlGPMNlCnY+5SwVyyIzmNC4GPWbgehqImrRNBXNW0TcA\n",
            "WHllrsHJV4mO18kL\n",
            "-----END CERTIFICATE-----\n",
        );
        let Some(Value::Record(record)) = read_pem(forever) else {
            panic!("reads");
        };
        assert_eq!(
            record.get("not_after"),
            Some(&Value::Instant(jadpo_std::INSTANT_MAX))
        );
        assert_eq!(pem_text(&record).as_deref(), Ok(forever));
    }
}
