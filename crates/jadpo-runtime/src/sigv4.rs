//! AWS Signature Version 4 (area 11, `credential:`; chunk 7.7c): how a
//! call to an AWS API is signed, as AWS's documentation and its published
//! test suite define it, so the provider knows the request came from the
//! holder of the secret access key and wasn't changed on the way.
//!
//! [`sign`] gives the two headers a request gains, `X-Amz-Date` and
//! `Authorization`, from its method, address, the headers it is signed
//! with, its body, the time and the credential. The secret never leaves
//! this module: only the key it derives for the day signs, and the
//! `Authorization` header carries the access key id and the signature,
//! never the secret.

use std::fmt;

use jadpo_value::crypto::mac;
use jadpo_value::hash::sha256;

/// The algorithm every signed request names.
pub const ALGORITHM: &str = "AWS4-HMAC-SHA256";

/// What a request is signed with: the access key id and its secret, and
/// the region and service it is signed for.
#[derive(Clone, Copy)]
pub struct Signer<'a> {
    pub key_id: &'a str,
    pub secret: &'a str,
    pub region: &'a str,
    pub service: &'a str,
}

impl fmt::Debug for Signer<'_> {
    /// Never the secret.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Signer")
            .field("region", &self.region)
            .field("service", &self.service)
            .finish_non_exhaustive()
    }
}

/// One request to sign: its method, its whole address, the headers it is
/// signed with (besides `Host`, read from the address, and `X-Amz-Date`,
/// which signing adds), and its body.
#[derive(Clone, Copy, Debug)]
pub struct Unsigned<'a> {
    pub method: &'a str,
    pub url: &'a str,
    pub headers: &'a [(String, String)],
    pub body: &'a [u8],
}

/// The headers `request`, made at `at_ms`, gains when signed by `signer`:
/// `X-Amz-Date`, then `Authorization`. `None` for an address that has no
/// host.
pub fn sign(
    request: &Unsigned<'_>,
    signer: &Signer<'_>,
    at_ms: i64,
) -> Option<[(String, String); 2]> {
    let (date, stamp) = amz_date(at_ms);
    let (host, path, query) = split(request.url)?;
    let mut headers: Vec<(String, String)> = request
        .headers
        .iter()
        .map(|(name, value)| (name.to_ascii_lowercase(), trimmed(value)))
        .collect();
    headers.push(("host".to_string(), host));
    headers.push(("x-amz-date".to_string(), stamp.clone()));
    headers.sort_by(|a, b| a.0.cmp(&b.0));
    // One line per name, repeated values joined with commas in the order
    // they were given.
    let mut joined: Vec<(String, String)> = Vec::new();
    for (name, value) in headers {
        match joined.last_mut() {
            Some((last, values)) if *last == name => {
                values.push(',');
                values.push_str(&value);
            }
            _ => joined.push((name, value)),
        }
    }
    let signed_headers = joined
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>()
        .join(";");
    let canonical_headers: String = joined
        .iter()
        .map(|(name, value)| format!("{name}:{value}\n"))
        .collect();
    let canonical = format!(
        "{}\n{}\n{}\n{canonical_headers}\n{signed_headers}\n{}",
        request.method,
        canonical_path(path),
        canonical_query(query),
        hex(&sha256(request.body)),
    );
    let scope = format!("{date}/{}/{}/aws4_request", signer.region, signer.service);
    let to_sign = format!(
        "{ALGORITHM}\n{stamp}\n{scope}\n{}",
        hex(&sha256(canonical.as_bytes()))
    );
    let key = signing_key(signer.secret, &date, signer.region, signer.service);
    let signature = hex(&mac(&key, &[to_sign.as_bytes()]));
    Some([
        ("X-Amz-Date".to_string(), stamp),
        (
            "Authorization".to_string(),
            format!(
                "{ALGORITHM} Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
                signer.key_id
            ),
        ),
    ])
}

/// The key a day's requests are signed with, for one region and service.
pub fn signing_key(secret: &str, date: &str, region: &str, service: &str) -> [u8; 32] {
    let day = mac(format!("AWS4{secret}").as_bytes(), &[date.as_bytes()]);
    let region = mac(&day, &[region.as_bytes()]);
    let service = mac(&region, &[service.as_bytes()]);
    mac(&service, &[b"aws4_request"])
}

/// The region an AWS address is in, from its host,
/// `<service>.<region>.amazonaws.com`; `us-east-1`, what AWS's own tools
/// sign for, for any other address, such as a local emulator's.
pub fn region_of(url: &str) -> String {
    let host = split(url).map(|(host, _, _)| host).unwrap_or_default();
    let host = host.split(':').next().unwrap_or_default();
    let labels: Vec<&str> = host.split('.').collect();
    match labels.as_slice() {
        [_, region, "amazonaws", "com", ..] if region.contains('-') => (*region).to_string(),
        _ => "us-east-1".to_string(),
    }
}

/// `YYYYMMDD` and `YYYYMMDDTHHMMSSZ` for `at_ms`, in UTC.
fn amz_date(at_ms: i64) -> (String, String) {
    let seconds = at_ms.div_euclid(1000);
    let days = seconds.div_euclid(86_400);
    let of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil(days);
    let date = format!("{year:04}{month:02}{day:02}");
    let stamp = format!(
        "{date}T{:02}{:02}{:02}Z",
        of_day / 3600,
        of_day % 3600 / 60,
        of_day % 60
    );
    (date, stamp)
}

/// The proleptic Gregorian date `days` after 1970-01-01 (Howard Hinnant's
/// `civil_from_days`).
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// An address's `Host` header as a client sends it (the host, and its port
/// unless the scheme's own), its path and its query.
fn split(url: &str) -> Option<(String, &str, &str)> {
    let (secure, rest) = if let Some(rest) = url.strip_prefix("https://") {
        (true, rest)
    } else {
        (false, url.strip_prefix("http://")?)
    };
    let (authority, rest) = match rest.find('/') {
        Some(at) => (&rest[..at], &rest[at..]),
        None => match rest.find('?') {
            Some(at) => (&rest[..at], &rest[at..]),
            None => (rest, ""),
        },
    };
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let mut host = authority.to_ascii_lowercase();
    let default = if secure { ":443" } else { ":80" };
    if let Some(bare) = host.strip_suffix(default) {
        host = bare.to_string();
    }
    if host.is_empty() {
        return None;
    }
    Some((host, path, query))
}

/// A path as AWS signs it for any service but S3: `/` when empty, each
/// segment encoded again.
fn canonical_path(path: &str) -> String {
    if path.is_empty() {
        return "/".to_string();
    }
    path.split('/')
        .map(|segment| encode(segment.as_bytes()))
        .collect::<Vec<_>>()
        .join("/")
}

/// A query as AWS signs it: each name and value decoded, encoded again
/// with only the unreserved characters left as they are, sorted by name
/// and then value.
fn canonical_query(query: &str) -> String {
    let mut pairs: Vec<(String, String)> = query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            (encode(&decode(name)), encode(&decode(value)))
        })
        .collect();
    pairs.sort();
    pairs
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

/// A header's value as signed: trimmed, and each run of spaces inside it
/// one space.
fn trimmed(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Percent-encodes every byte but `A`-`Z`, `a`-`z`, `0`-`9`, `-`, `_`, `.`
/// and `~`, in upper-case hex.
fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &byte in bytes {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Decodes `%XX` escapes; a `%` not followed by two hex digits is kept.
fn decode(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        if byte == b'%'
            && let Some(decoded) = bytes
                .get(at + 1..at + 3)
                .and_then(|hex| std::str::from_utf8(hex).ok())
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            out.push(decoded);
            at += 3;
            continue;
        }
        out.push(byte);
        at += 1;
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The test suite's credential, region, service and time:
    /// 2015-08-30T12:36:00Z.
    const KEY_ID: &str = "AKIDEXAMPLE";
    const SECRET: &str = "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY";
    const AT: i64 = 1_440_938_160_000;

    fn suite() -> Signer<'static> {
        Signer {
            key_id: KEY_ID,
            secret: SECRET,
            region: "us-east-1",
            service: "service",
        }
    }

    fn headers(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    }

    /// The `Authorization` header `request` is signed with.
    fn authorization(request: &Unsigned<'_>, signer: &Signer<'_>) -> String {
        let [date, authorization] = sign(request, signer, AT).expect("signed");
        assert_eq!(
            date,
            ("X-Amz-Date".to_string(), "20150830T123600Z".to_string())
        );
        authorization.1
    }

    /// AWS's own worked example (Signature Version 4, "Create a signed
    /// request"): the key derived for IAM in us-east-1 on 2015-08-30, and
    /// IAM's `ListUsers` signed with it.
    #[test]
    fn the_signing_key_and_signature_are_those_of_awss_worked_example() {
        assert_eq!(
            hex(&signing_key(SECRET, "20150830", "us-east-1", "iam")),
            "c4afb1cc5771d871763a393e44b703571b55cc28424d1a5e86da6ed3c154a4b9"
        );
        let signed = headers(&[(
            "Content-Type",
            "application/x-www-form-urlencoded; charset=utf-8",
        )]);
        let request = Unsigned {
            method: "GET",
            url: "https://iam.amazonaws.com/?Action=ListUsers&Version=2010-05-08",
            headers: &signed,
            body: b"",
        };
        let signer = Signer {
            service: "iam",
            ..suite()
        };
        assert_eq!(
            authorization(&request, &signer),
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/iam/aws4_request, SignedHeaders=content-type;host;x-amz-date, Signature=5d672d79c15b13162d9279b0855cfba6789a8edb4c82c400e06b5924a6f2b5d7"
        );
    }

    /// Cases of AWS's Signature Version 4 test suite, each its request and
    /// the `Authorization` header the suite gives for it.
    #[test]
    fn requests_are_signed_as_awss_test_suite_signs_them() {
        let none = headers(&[]);
        let form = headers(&[("Content-Type", "application/x-www-form-urlencoded")]);
        let sorted = headers(&[("My-Header1", "value1")]);
        let trim = headers(&[("My-Header1", " value1"), ("My-Header2", " \"a   b   c\"")]);
        let cases: [(&str, Unsigned<'_>, &str, &str); 6] = [
            (
                "get-vanilla",
                Unsigned {
                    method: "GET",
                    url: "https://example.amazonaws.com/",
                    headers: &none,
                    body: b"",
                },
                "host;x-amz-date",
                "5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31",
            ),
            (
                "post-vanilla",
                Unsigned {
                    method: "POST",
                    url: "https://example.amazonaws.com/",
                    headers: &none,
                    body: b"",
                },
                "host;x-amz-date",
                "5da7c1a2acd57cee7505fc6676e4e544621c30862966e37dddb68e92efbe5d6b",
            ),
            (
                "get-vanilla-query-order-key-case",
                Unsigned {
                    method: "GET",
                    url: "https://example.amazonaws.com/?Param2=value2&Param1=value1",
                    headers: &none,
                    body: b"",
                },
                "host;x-amz-date",
                "b97d918cfa904a5beff61c982a1b6f458b799221646efd99d3219ec94cdf2500",
            ),
            (
                "post-x-www-form-urlencoded",
                Unsigned {
                    method: "POST",
                    url: "https://example.amazonaws.com/",
                    headers: &form,
                    body: b"Param1=value1",
                },
                "content-type;host;x-amz-date",
                "ff11897932ad3f4e8b18135d722051e5ac45fc38421b1da7b9d196a0fe09473a",
            ),
            (
                "post-header-key-sort",
                Unsigned {
                    method: "POST",
                    url: "https://example.amazonaws.com/",
                    headers: &sorted,
                    body: b"",
                },
                "host;my-header1;x-amz-date",
                "c5410059b04c1ee005303aed430f6e6645f61f4dc9e1461ec8f8916fdf18852c",
            ),
            (
                "get-header-value-trim",
                Unsigned {
                    method: "GET",
                    url: "https://example.amazonaws.com/",
                    headers: &trim,
                    body: b"",
                },
                "host;my-header1;my-header2;x-amz-date",
                "acc3ed3afb60bb290fc8d2dd0098b9911fcaa05412b367055dee359757a9c736",
            ),
        ];
        for (name, request, signed, signature) in cases {
            assert_eq!(
                authorization(&request, &suite()),
                format!(
                    "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request, SignedHeaders={signed}, Signature={signature}"
                ),
                "{name}"
            );
        }
    }

    /// The region comes from an AWS host; any other is signed for
    /// us-east-1. A port that is the scheme's own isn't part of `Host`.
    #[test]
    fn the_region_is_read_from_an_aws_host() {
        assert_eq!(
            region_of("https://sqs.eu-west-2.amazonaws.com"),
            "eu-west-2"
        );
        assert_eq!(
            region_of("https://sqs.eu-west-2.amazonaws.com/"),
            "eu-west-2"
        );
        assert_eq!(region_of("http://127.0.0.1:9324"), "us-east-1");
        assert_eq!(
            split("https://example.amazonaws.com:443/a?b=c").map(|(host, _, _)| host),
            Some("example.amazonaws.com".to_string())
        );
        assert_eq!(
            split("http://127.0.0.1:9324").map(|(host, path, _)| (host, path)),
            Some(("127.0.0.1:9324".to_string(), ""))
        );
        assert_eq!(amz_date(0).1, "19700101T000000Z");
        assert_eq!(amz_date(951_782_400_000).0, "20000229");
    }

    /// The secret is never in what is signed or shown.
    #[test]
    fn the_secret_appears_in_no_header_and_no_debug_text() {
        let none = headers(&[]);
        let request = Unsigned {
            method: "POST",
            url: "https://sqs.eu-west-2.amazonaws.com/",
            headers: &none,
            body: b"{}",
        };
        let signed = sign(&request, &suite(), AT).expect("signed");
        for (_, value) in &signed {
            assert!(!value.contains(SECRET));
        }
        assert!(!format!("{:?}", suite()).contains(SECRET));
    }
}
