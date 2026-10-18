//! `Certificate` (area 19, "Security records", chunk 5.5r5): an X.509
//! certificate (RFC 5280), read from its PEM (RFC 7468) and never built
//! from its parts. Its subject, issuer, serial number, validity and SHA-256
//! fingerprint are read from the DER its issuer signed, so code reads them
//! and never sets them: an object literal of one is `type.constructor_input`,
//! and the runtime makes one only by reading the DER (`jadpo-value`'s
//! `certificate` module, on `x509-parser`). Its chain isn't checked.
//!
//! Its canonical text and its JSON are its PEM, one `CERTIFICATE` block
//! of 64-character lines, each ending in a line feed; it is stored as that
//! JSON. It is shown to people by its subject, else its serial number.

use super::{
    Examples, FIRST_RELEASE, Format, FormatKey, Holds, Json, NO_OPTIONS, Part, StandardType,
    Storage, TextForm,
};
use crate::Builtin;
use crate::read::SqlType;

/// `Certificate` (area 19).
pub const CERTIFICATE: &str = "Certificate";

/// The most bytes a certificate's DER may take (chosen in chunk 5.5r5):
/// 64 KiB, several times the largest a public CA issues, so reading one
/// from outside is bounded.
pub const DER_MAX_BYTES: usize = 65_536;

/// The most octets a certificate's serial number takes (RFC 5280
/// §4.1.2.2), its sign byte included.
pub const SERIAL_MAX_OCTETS: usize = 20;

/// The characters of a PEM line (RFC 7468 §2): all but the last are
/// exactly this many.
pub const PEM_LINE: usize = 64;

/// A serial number as `Certificate` writes it: the positive integer's
/// octets in lowercase hexadecimal, two digits an octet, with no sign octet
/// and no separators.
pub const SERIAL_PATTERN: &str = "(?:[0-9a-f]{2}){1,20}";

/// `Certificate`'s parts, in order: its subject and issuer as RFC 4514
/// writes a distinguished name, its serial number, the first and last
/// instants it is valid at, both included (RFC 5280 §4.1.2.5), the SHA-256
/// of its DER, and the DER itself.
pub const CERTIFICATE_FIELDS: [(&str, Part); 7] = [
    ("subject", Part::Of(Builtin::Text)),
    ("issuer", Part::Of(Builtin::Text)),
    ("serial_number", Part::Matching(SERIAL_PATTERN)),
    ("not_before", Part::Of(Builtin::Instant)),
    ("not_after", Part::Of(Builtin::Instant)),
    ("fingerprint", Part::Standard(super::security::HASH)),
    ("der", Part::Of(Builtin::Bytes)),
];

/// Whether standard type `name` is read only from what it parses, so
/// code never builds one from its parts (chunk 5.5r5): a `Certificate`.
pub fn read_only(name: &str) -> bool {
    name == CERTIFICATE
}

/// The Base64 lines of the self-signed Ed25519 certificate the examples
/// and tests read, made for them with OpenSSL 3.6: `jadpo-test.example`,
/// valid from 2026-10-01 to 2036-10-01, serial `1f2e3d4c5b6a7988`.
macro_rules! self_signed_body {
    () => {
        concat!(
            "MIIBljCCAUigAwIBAgIIHy49TFtqeYgwBQYDK2VwMEgxCzAJBgNVBAYTAkdCMRww\n",
            "GgYDVQQKDBNKYWRwbyBUZXN0LCBTaWduaW5nMRswGQYDVQQDDBJqYWRwby10ZXN0\n",
            "LmV4YW1wbGUwHhcNMjYxMDAxMDAwMDAwWhcNMzYxMDAxMDAwMDAwWjBIMQswCQYD\n",
            "VQQGEwJHQjEcMBoGA1UECgwTSmFkcG8gVGVzdCwgU2lnbmluZzEbMBkGA1UEAwwS\n",
            "amFkcG8tdGVzdC5leGFtcGxlMCowBQYDK2VwAyEAAJyTOxfkgCmTOYhl/AXEMRpT\n",
            "07QqDzpBZIRCSESxhYejUDBOMB0GA1UdDgQWBBTH4Arltue6aEupl+oqXRoIZos6\n",
            "LTAfBgNVHSMEGDAWgBTH4Arltue6aEupl+oqXRoIZos6LTAMBgNVHRMBAf8EAjAA\n",
            "MAUGAytlcANBAHkV9ilHQ5TVpSZyPobRcjo9bERYbugOAsKXD4ZJjCRPqpyK+aDc\n",
            "+FxV2FFnXI2UQxnYsieWNSclooiCSIuDmwA=\n",
        )
    };
}

/// The self-signed certificate, as its canonical text.
pub const SELF_SIGNED: &str = concat!(
    "-----BEGIN CERTIFICATE-----\n",
    self_signed_body!(),
    "-----END CERTIFICATE-----\n"
);

/// RFC 8410 §10.1's Ed25519 public key: a PEM block, but not a
/// certificate's.
pub const PUBLIC_KEY_BLOCK: &str = concat!(
    "-----BEGIN PUBLIC KEY-----\n",
    "MCowBQYDK2VwAyEAGb9ECWmEzf6FQbrBZ9w7lshQhqowtrbLDFw4rXAxZuE=\n",
    "-----END PUBLIC KEY-----\n"
);

/// The self-signed certificate twice: two blocks, where one is read.
pub const TWO_BLOCKS: &str = concat!(
    "-----BEGIN CERTIFICATE-----\n",
    self_signed_body!(),
    "-----END CERTIFICATE-----\n",
    "-----BEGIN CERTIFICATE-----\n",
    self_signed_body!(),
    "-----END CERTIFICATE-----\n"
);

/// The self-signed certificate with OpenSSL's explanatory text before it.
pub const WITH_TEXT_BEFORE: &str = concat!(
    "subject=CN=jadpo-test.example\n",
    "-----BEGIN CERTIFICATE-----\n",
    self_signed_body!(),
    "-----END CERTIFICATE-----\n"
);

/// The self-signed certificate under OpenSSL's own `TRUSTED CERTIFICATE`
/// label, which adds trust settings after the DER.
pub const OTHER_LABEL: &str = concat!(
    "-----BEGIN TRUSTED CERTIFICATE-----\n",
    self_signed_body!(),
    "-----END TRUSTED CERTIFICATE-----\n"
);

/// ISRG Root X1, Let's Encrypt's root, as it publishes it: an RSA
/// certificate with a positive serial whose DER starts with a zero byte.
pub const ISRG_ROOT_X1: &str = concat!(
    "-----BEGIN CERTIFICATE-----\n",
    "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\n",
    "-----END CERTIFICATE-----\n",
);

/// A certificate whose serial is -5, which RFC 5280 §4.1.2.2 forbids.
pub const NEGATIVE_SERIAL: &str = concat!(
    "-----BEGIN CERTIFICATE-----\n",
    "MIIBMjCB5aADAgECAgH7MAUGAytlcDAaMRgwFgYDVQQDDA9uZWdhdGl2ZSBzZXJp\n",
    "YWwwHhcNMjYxMDAxMDAwMDAwWhcNMzYxMDAxMDAwMDAwWjAaMRgwFgYDVQQDDA9u\n",
    "ZWdhdGl2ZSBzZXJpYWwwKjAFBgMrZXADIQAAnJM7F+SAKZM5iGX8BcQxGlPTtCoP\n",
    "OkFkhEJIRLGFh6NQME4wHQYDVR0OBBYEFMfgCuW257poS6mX6ipdGghmizotMB8G\n",
    "A1UdIwQYMBaAFMfgCuW257poS6mX6ipdGghmizotMAwGA1UdEwEB/wQCMAAwBQYD\n",
    "K2VwA0EA6MexNhQ3N2Nme1n/akVkOC27IDTha1x2rQTIfAqDhJFugcjLYkwTRLFj\n",
    "rzyhZiLYPZUb/UeGlOK6WjIU1XCbBA==\n",
    "-----END CERTIFICATE-----\n",
);

/// A certificate whose serial is 0, not a positive integer.
pub const ZERO_SERIAL: &str = concat!(
    "-----BEGIN CERTIFICATE-----\n",
    "MIIBKjCB3aADAgECAgEAMAUGAytlcDAWMRQwEgYDVQQDDAt6ZXJvIHNlcmlhbDAe\n",
    "Fw0yNjEwMDEwMDAwMDBaFw0zNjEwMDEwMDAwMDBaMBYxFDASBgNVBAMMC3plcm8g\n",
    "c2VyaWFsMCowBQYDK2VwAyEAAJyTOxfkgCmTOYhl/AXEMRpT07QqDzpBZIRCSESx\n",
    "hYejUDBOMB0GA1UdDgQWBBTH4Arltue6aEupl+oqXRoIZos6LTAfBgNVHSMEGDAW\n",
    "gBTH4Arltue6aEupl+oqXRoIZos6LTAMBgNVHRMBAf8EAjAAMAUGAytlcANBAP17\n",
    "4bjdzp5QMFc9jmbgvatTab9n1i6uBEX1tyTLNNN0+PYYXXkhVT+w6EQrS4RIo/2v\n",
    "vEZrF9ZJPt6u7WmiugA=\n",
    "-----END CERTIFICATE-----\n",
);

/// A certificate whose serial takes 21 octets, past RFC 5280's 20.
pub const OVERSIZED_SERIAL: &str = concat!(
    "-----BEGIN CERTIFICATE-----\n",
    "MIIBSDCB+6ADAgECAhUBAgMEBQYHCAkKCwwNDg8QERITFBUwBQYDK2VwMBsxGTAX\n",
    "BgNVBAMMEG92ZXJzaXplZCBzZXJpYWwwHhcNMjYxMDAxMDAwMDAwWhcNMzYxMDAx\n",
    "MDAwMDAwWjAbMRkwFwYDVQQDDBBvdmVyc2l6ZWQgc2VyaWFsMCowBQYDK2VwAyEA\n",
    "AJyTOxfkgCmTOYhl/AXEMRpT07QqDzpBZIRCSESxhYejUDBOMB0GA1UdDgQWBBTH\n",
    "4Arltue6aEupl+oqXRoIZos6LTAfBgNVHSMEGDAWgBTH4Arltue6aEupl+oqXRoI\n",
    "Zos6LTAMBgNVHRMBAf8EAjAAMAUGAytlcANBAJeEGmI30IQ4vxLVWtAijyNYfwms\n",
    "uHBbTnxzzNCEYSfzmxuP9qQ1zln+aPD93t+ycRuErrXYiuHkytcH+UbpqgQ=\n",
    "-----END CERTIFICATE-----\n",
);

/// The self-signed certificate's DER with a byte after it.
pub const TRAILING_BYTE: &str = concat!(
    "-----BEGIN CERTIFICATE-----\n",
    "MIIBljCCAUigAwIBAgIIHy49TFtqeYgwBQYDK2VwMEgxCzAJBgNVBAYTAkdCMRww\n",
    "GgYDVQQKDBNKYWRwbyBUZXN0LCBTaWduaW5nMRswGQYDVQQDDBJqYWRwby10ZXN0\n",
    "LmV4YW1wbGUwHhcNMjYxMDAxMDAwMDAwWhcNMzYxMDAxMDAwMDAwWjBIMQswCQYD\n",
    "VQQGEwJHQjEcMBoGA1UECgwTSmFkcG8gVGVzdCwgU2lnbmluZzEbMBkGA1UEAwwS\n",
    "amFkcG8tdGVzdC5leGFtcGxlMCowBQYDK2VwAyEAAJyTOxfkgCmTOYhl/AXEMRpT\n",
    "07QqDzpBZIRCSESxhYejUDBOMB0GA1UdDgQWBBTH4Arltue6aEupl+oqXRoIZos6\n",
    "LTAfBgNVHSMEGDAWgBTH4Arltue6aEupl+oqXRoIZos6LTAMBgNVHRMBAf8EAjAA\n",
    "MAUGAytlcANBAHkV9ilHQ5TVpSZyPobRcjo9bERYbugOAsKXD4ZJjCRPqpyK+aDc\n",
    "+FxV2FFnXI2UQxnYsieWNSclooiCSIuDmwAA\n",
    "-----END CERTIFICATE-----\n",
);

/// The self-signed certificate's DER without its last byte.
pub const TRUNCATED: &str = concat!(
    "-----BEGIN CERTIFICATE-----\n",
    "MIIBljCCAUigAwIBAgIIHy49TFtqeYgwBQYDK2VwMEgxCzAJBgNVBAYTAkdCMRww\n",
    "GgYDVQQKDBNKYWRwbyBUZXN0LCBTaWduaW5nMRswGQYDVQQDDBJqYWRwby10ZXN0\n",
    "LmV4YW1wbGUwHhcNMjYxMDAxMDAwMDAwWhcNMzYxMDAxMDAwMDAwWjBIMQswCQYD\n",
    "VQQGEwJHQjEcMBoGA1UECgwTSmFkcG8gVGVzdCwgU2lnbmluZzEbMBkGA1UEAwwS\n",
    "amFkcG8tdGVzdC5leGFtcGxlMCowBQYDK2VwAyEAAJyTOxfkgCmTOYhl/AXEMRpT\n",
    "07QqDzpBZIRCSESxhYejUDBOMB0GA1UdDgQWBBTH4Arltue6aEupl+oqXRoIZos6\n",
    "LTAfBgNVHSMEGDAWgBTH4Arltue6aEupl+oqXRoIZos6LTAMBgNVHRMBAf8EAjAA\n",
    "MAUGAytlcANBAHkV9ilHQ5TVpSZyPobRcjo9bERYbugOAsKXD4ZJjCRPqpyK+aDc\n",
    "+FxV2FFnXI2UQxnYsieWNSclooiCSIuDmw==\n",
    "-----END CERTIFICATE-----\n",
);

/// The self-signed certificate with its outer length in four bytes
/// where DER takes two.
pub const LONG_LENGTH: &str = concat!(
    "-----BEGIN CERTIFICATE-----\n",
    "MIMAAZYwggFIoAMCAQICCB8uPUxbanmIMAUGAytlcDBIMQswCQYDVQQGEwJHQjEc\n",
    "MBoGA1UECgwTSmFkcG8gVGVzdCwgU2lnbmluZzEbMBkGA1UEAwwSamFkcG8tdGVz\n",
    "dC5leGFtcGxlMB4XDTI2MTAwMTAwMDAwMFoXDTM2MTAwMTAwMDAwMFowSDELMAkG\n",
    "A1UEBhMCR0IxHDAaBgNVBAoME0phZHBvIFRlc3QsIFNpZ25pbmcxGzAZBgNVBAMM\n",
    "EmphZHBvLXRlc3QuZXhhbXBsZTAqMAUGAytlcAMhAACckzsX5IApkzmIZfwFxDEa\n",
    "U9O0Kg86QWSEQkhEsYWHo1AwTjAdBgNVHQ4EFgQUx+AK5bbnumhLqZfqKl0aCGaL\n",
    "Oi0wHwYDVR0jBBgwFoAUx+AK5bbnumhLqZfqKl0aCGaLOi0wDAYDVR0TAQH/BAIw\n",
    "ADAFBgMrZXADQQB5FfYpR0OU1aUmcj6G0XI6PWxEWG7oDgLClw+GSYwkT6qcivmg\n",
    "3PhcVdhRZ1yNlEMZ2LInljUnJaKIgkiLg5sA\n",
    "-----END CERTIFICATE-----\n",
);

/// The self-signed certificate claiming Ed448 outside what was signed,
/// Ed25519 inside (RFC 5280 §4.1.1.2).
pub const OTHER_ALGORITHM: &str = concat!(
    "-----BEGIN CERTIFICATE-----\n",
    "MIIBljCCAUigAwIBAgIIHy49TFtqeYgwBQYDK2VwMEgxCzAJBgNVBAYTAkdCMRww\n",
    "GgYDVQQKDBNKYWRwbyBUZXN0LCBTaWduaW5nMRswGQYDVQQDDBJqYWRwby10ZXN0\n",
    "LmV4YW1wbGUwHhcNMjYxMDAxMDAwMDAwWhcNMzYxMDAxMDAwMDAwWjBIMQswCQYD\n",
    "VQQGEwJHQjEcMBoGA1UECgwTSmFkcG8gVGVzdCwgU2lnbmluZzEbMBkGA1UEAwwS\n",
    "amFkcG8tdGVzdC5leGFtcGxlMCowBQYDK2VwAyEAAJyTOxfkgCmTOYhl/AXEMRpT\n",
    "07QqDzpBZIRCSESxhYejUDBOMB0GA1UdDgQWBBTH4Arltue6aEupl+oqXRoIZos6\n",
    "LTAfBgNVHSMEGDAWgBTH4Arltue6aEupl+oqXRoIZos6LTAMBgNVHRMBAf8EAjAA\n",
    "MAUGAytlcQNBAHkV9ilHQ5TVpSZyPobRcjo9bERYbugOAsKXD4ZJjCRPqpyK+aDc\n",
    "+FxV2FFnXI2UQxnYsieWNSclooiCSIuDmwA=\n",
    "-----END CERTIFICATE-----\n",
);

/// The self-signed certificate wrapped at 76 characters, not RFC 7468's
/// 64.
pub const WIDE_LINES: &str = concat!(
    "-----BEGIN CERTIFICATE-----\n",
    "MIIBljCCAUigAwIBAgIIHy49TFtqeYgwBQYDK2VwMEgxCzAJBgNVBAYTAkdCMRwwGgYDVQQKDBNK\n",
    "YWRwbyBUZXN0LCBTaWduaW5nMRswGQYDVQQDDBJqYWRwby10ZXN0LmV4YW1wbGUwHhcNMjYxMDAx\n",
    "MDAwMDAwWhcNMzYxMDAxMDAwMDAwWjBIMQswCQYDVQQGEwJHQjEcMBoGA1UECgwTSmFkcG8gVGVz\n",
    "dCwgU2lnbmluZzEbMBkGA1UEAwwSamFkcG8tdGVzdC5leGFtcGxlMCowBQYDK2VwAyEAAJyTOxfk\n",
    "gCmTOYhl/AXEMRpT07QqDzpBZIRCSESxhYejUDBOMB0GA1UdDgQWBBTH4Arltue6aEupl+oqXRoI\n",
    "Zos6LTAfBgNVHSMEGDAWgBTH4Arltue6aEupl+oqXRoIZos6LTAMBgNVHRMBAf8EAjAAMAUGAytl\n",
    "cANBAHkV9ilHQ5TVpSZyPobRcjo9bERYbugOAsKXD4ZJjCRPqpyK+aDc+FxV2FFnXI2UQxnYsieW\n",
    "NSclooiCSIuDmwA=\n",
    "-----END CERTIFICATE-----\n",
);

/// An X.509 certificate, read from its PEM: sent and stored as that text,
/// and shown by its subject.
pub const CERTIFICATE_ENTRY: StandardType = StandardType {
    name: CERTIFICATE,
    holds: Holds::Object(&CERTIFICATE_FIELDS),
    text: Some(TextForm::Pem),
    json: Json::Text,
    storage: Storage::Column(SqlType::Json),
    format: Format {
        key: FormatKey::FirstPart(&["subject", "serial_number"]),
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: Examples {
        valid: &[
            (
                SELF_SIGNED,
                "CN=jadpo-test.example,O=Jadpo Test\\, Signing,C=GB",
            ),
            (
                ISRG_ROOT_X1,
                "CN=ISRG Root X1,O=Internet Security Research Group,C=US",
            ),
        ],
        format: None,
        // Not PEM; a key's block; two blocks; text before the block;
        // another label; a serial that is negative, zero or too long; a
        // byte after the DER or one short; a length DER writes shorter; an
        // algorithm outside the signature that isn't the one inside; lines
        // of 76 characters.
        malformed: &[
            "",
            "MIIBljCCAUigAwIBAgIIHy49TFtqeYgw",
            PUBLIC_KEY_BLOCK,
            TWO_BLOCKS,
            WITH_TEXT_BEFORE,
            OTHER_LABEL,
            NEGATIVE_SERIAL,
            ZERO_SERIAL,
            OVERSIZED_SERIAL,
            TRAILING_BYTE,
            TRUNCATED,
            LONG_LENGTH,
            OTHER_ALGORITHM,
            WIDE_LINES,
        ],
        of: None,
        settings: "",
    },
    release: FIRST_RELEASE,
};
