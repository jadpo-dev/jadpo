//! The encodings (area 19, "Encodings"): bytes written as text and read
//! back, each one function both executors call. Writing never fails;
//! reading gives `None` for text that isn't the encoding at all, which the
//! operation fails as `malformed`, and reads only the one form writing
//! gives, so every value has exactly one encoded text.
//!
//! The wire's `Bytes` (area 2) is [`to_base64`] and [`from_base64`] too:
//! a request, a response and `json.read` read bytes alike.

const LOWER_HEX: &[u8; 16] = b"0123456789abcdef";
const UPPER_HEX: &[u8; 16] = b"0123456789ABCDEF";
const STANDARD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// `bytes` in RFC 4648's standard alphabet, padded with `=` to a multiple
/// of four characters.
pub fn to_base64(bytes: &[u8]) -> String {
    base64(bytes, STANDARD, true)
}

/// The bytes `text` holds in the standard alphabet, padded; `None` for
/// anything else: unpadded or over-padded text, white space, another
/// alphabet's characters, or bits after the last byte that aren't zero
/// (RFC 4648, 3.5), so no two texts read as the same bytes.
pub fn from_base64(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(4) {
        return None;
    }
    let body = text.strip_suffix("==").or_else(|| text.strip_suffix('='));
    let body = body.unwrap_or(text);
    unbase64(body, STANDARD)
}

/// `bytes` in RFC 4648's URL alphabet, unpadded, as area 8's signed parts
/// are written.
pub fn to_base64url(bytes: &[u8]) -> String {
    base64(bytes, URL, false)
}

/// The bytes `text` holds in the URL alphabet, unpadded; `None` for text
/// with `=` padding, another alphabet's characters, a length no bytes
/// give, or bits after the last byte that aren't zero.
pub fn from_base64url(text: &str) -> Option<Vec<u8>> {
    unbase64(text, URL)
}

/// `bytes` as lowercase hexadecimal, two digits a byte.
pub fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(letter(LOWER_HEX, u32::from(byte >> 4)));
        out.push(letter(LOWER_HEX, u32::from(byte & 0x0f)));
    }
    out
}

/// Letter `n` of `alphabet`, which always has one: `n` is masked to it.
fn letter(alphabet: &[u8], n: u32) -> char {
    alphabet.get(n as usize).copied().map_or('?', char::from)
}

/// The bytes hexadecimal `text` holds, its digits in either case; `None`
/// for an odd number of digits or anything that isn't one.
pub fn from_hex(text: &str) -> Option<Vec<u8>> {
    let (pairs, odd) = text.as_bytes().as_chunks::<2>();
    if !odd.is_empty() {
        return None;
    }
    let digit = |c: u8| char::from(c).to_digit(16);
    pairs
        .iter()
        .map(|[high, low]| Some(((digit(*high)? << 4) | digit(*low)?) as u8))
        .collect()
}

/// The text UTF-8 `bytes` hold; `None` when they aren't valid UTF-8,
/// which is refused, never replaced.
pub fn from_utf8(bytes: &[u8]) -> Option<String> {
    std::str::from_utf8(bytes).ok().map(str::to_string)
}

/// `text` with every character but RFC 3986's unreserved ones (letters,
/// digits, `-`, `.`, `_` and `~`) written as its UTF-8 bytes, each as `%`
/// and two uppercase hexadecimal digits, as RFC 3986 (2.1) recommends.
pub fn to_url_component(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push('%');
            out.push(letter(UPPER_HEX, u32::from(byte >> 4)));
            out.push(letter(UPPER_HEX, u32::from(byte & 0x0f)));
        }
    }
    out
}

fn base64(bytes: &[u8], alphabet: &[u8; 64], padded: bool) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    let letter = |n: u32| letter(alphabet, n & 63);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (at, byte)| n | u32::from(*byte) << (16 - 8 * at));
        out.push(letter(n >> 18));
        out.push(letter(n >> 12));
        if chunk.len() > 1 {
            out.push(letter(n >> 6));
        } else if padded {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(letter(n));
        } else if padded {
            out.push('=');
        }
    }
    out
}

/// The bytes unpadded `text` holds in `alphabet`.
fn unbase64(text: &str, alphabet: &[u8; 64]) -> Option<Vec<u8>> {
    // One character over a multiple of four holds 6 bits: not a byte.
    if text.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() / 4 * 3 + 2);
    let mut buffer: u32 = 0;
    let mut bits = 0;
    for c in text.bytes() {
        let value = alphabet.iter().position(|letter| *letter == c)? as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    // The bits left over pad the last character, and are zero in the one
    // text that writes these bytes.
    (buffer == 0).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_is_rfc_4648_s_test_vectors() {
        // RFC 4648, section 10.
        let vectors = [
            ("", "", ""),
            ("f", "Zg==", "Zg"),
            ("fo", "Zm8=", "Zm8"),
            ("foo", "Zm9v", "Zm9v"),
            ("foob", "Zm9vYg==", "Zm9vYg"),
            ("fooba", "Zm9vYmE=", "Zm9vYmE"),
            ("foobar", "Zm9vYmFy", "Zm9vYmFy"),
        ];
        for (bytes, standard, url) in vectors {
            assert_eq!(to_base64(bytes.as_bytes()), standard);
            assert_eq!(from_base64(standard).as_deref(), Some(bytes.as_bytes()));
            assert_eq!(to_base64url(bytes.as_bytes()), url);
            assert_eq!(from_base64url(url).as_deref(), Some(bytes.as_bytes()));
        }
        // The two alphabets differ in their last two letters.
        assert_eq!(to_base64(&[0xfb, 0xff]), "+/8=");
        assert_eq!(to_base64url(&[0xfb, 0xff]), "-_8");
    }

    #[test]
    fn base64_reads_only_the_text_it_writes() {
        for refused in ["Zg", "Zg=", "Zg===", "Zh==", "Z g==", "Zm9v\n", "-_8=", "=Zg="] {
            assert_eq!(from_base64(refused), None, "{refused:?}");
        }
        for refused in ["Zg==", "Zh", "Z", "+/8", "Zm9vY"] {
            assert_eq!(from_base64url(refused), None, "{refused:?}");
        }
        for length in 0..40u8 {
            let bytes: Vec<u8> = (0..length).map(|n| n.wrapping_mul(37)).collect();
            assert_eq!(from_base64(&to_base64(&bytes)), Some(bytes.clone()));
            assert_eq!(from_base64url(&to_base64url(&bytes)), Some(bytes));
        }
    }

    #[test]
    fn hex_is_lowercase_and_reads_either_case() {
        assert_eq!(to_hex(&[0x00, 0x7f, 0xab, 0xff]), "007fabff");
        assert_eq!(from_hex("007FabFF"), Some(vec![0x00, 0x7f, 0xab, 0xff]));
        assert_eq!(from_hex(""), Some(Vec::new()));
        assert_eq!(from_hex("abc"), None);
        assert_eq!(from_hex("0g"), None);
        assert_eq!(from_hex("+1"), None);
    }

    #[test]
    fn utf8_is_refused_never_replaced() {
        assert_eq!(from_utf8("crème".as_bytes()).as_deref(), Some("crème"));
        assert_eq!(from_utf8(&[0x63, 0xff]), None);
        assert_eq!(from_utf8(&[0xed, 0xa0, 0x80]), None, "a lone surrogate");
    }

    #[test]
    fn a_url_component_keeps_only_the_unreserved_characters() {
        assert_eq!(to_url_component("AZaz09-._~"), "AZaz09-._~");
        assert_eq!(
            to_url_component("a b&c=d/e?f#g+h"),
            "a%20b%26c%3Dd%2Fe%3Ff%23g%2Bh"
        );
        assert_eq!(to_url_component("crème brûlée"), "cr%C3%A8me%20br%C3%BBl%C3%A9e");
        assert_eq!(to_url_component("🍞"), "%F0%9F%8D%9E");
    }
}
