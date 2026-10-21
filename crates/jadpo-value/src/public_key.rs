//! Public-key signatures as WebAuthn's devices make them (area 8,
//! "Passkeys"; chunk 8.2l): ES256, ECDSA on P-256 over SHA-256 with its
//! signature in ASN.1 DER (WebAuthn Level 3 §6.5.6), and RS256,
//! RSASSA-PKCS1-v1_5 over SHA-256 (RFC 8017 §8.2). RustCrypto's `p256` and
//! `rsa`, pure Rust, so the Workers host checks a passkey as the native
//! host's `webauthn-rs` does over OpenSSL.
//!
//! - **A P-256 key** is its two 32-byte coordinates, a point on the curve
//!   and not at infinity.
//! - **An ES256 signature** is DER read strictly: one `SEQUENCE` of two
//!   positive `INTEGER`s in their shortest form, nothing after it, as
//!   OpenSSL accepts only the encoding it would write itself; R and S
//!   from 1 to below the group's order. A high S is accepted, as OpenSSL
//!   and `p256` accept it.
//! - **An RS256 signature** is exactly as long as the modulus, its
//!   padding and digest compared whole.

use p256::ecdsa::signature::hazmat::PrehashVerifier;

/// The DER of SHA-256's `DigestInfo` up to the digest itself, which
/// RSASSA-PKCS1-v1_5 signs (RFC 8017 §9.2, note 1).
pub(crate) const SHA256_DIGEST_INFO: [u8; 19] = [
    0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01, 0x05,
    0x00, 0x04, 0x20,
];

/// The P-256 key whose coordinates are `x` and `y`, when they are a point
/// on the curve.
fn p256_key(x: &[u8], y: &[u8]) -> Option<p256::ecdsa::VerifyingKey> {
    if x.len() != 32 || y.len() != 32 {
        return None;
    }
    let mut point = Vec::with_capacity(65);
    point.push(4);
    point.extend_from_slice(x);
    point.extend_from_slice(y);
    p256::ecdsa::VerifyingKey::from_sec1_bytes(&point).ok()
}

/// Whether `x` and `y` are a P-256 public key: 32 bytes each, a point on
/// the curve.
pub fn is_p256_key(x: &[u8], y: &[u8]) -> bool {
    p256_key(x, y).is_some()
}

/// Whether `signature`, an ECDSA signature in DER, is the P-256 key
/// (`x`, `y`)'s signature of `signed`'s SHA-256 digest.
pub fn es256_der(x: &[u8], y: &[u8], signed: &[u8], signature: &[u8]) -> bool {
    let (Some(key), Some(raw)) = (p256_key(x, y), der_signature(signature)) else {
        return false;
    };
    p256::ecdsa::Signature::from_slice(&raw).is_ok_and(|signature| {
        key.verify_prehash(&crate::hash::sha256(signed), &signature)
            .is_ok()
    })
}

/// The RSA key with modulus `n` and exponent `e`, both big-endian.
pub(crate) fn rsa_key(n: &[u8], e: &[u8]) -> Option<rsa::RsaPublicKey> {
    rsa::RsaPublicKey::new(
        rsa::BigUint::from_bytes_be(n),
        rsa::BigUint::from_bytes_be(e),
    )
    .ok()
}

/// Whether `n` and `e` are an RSA public key this module checks with.
pub fn is_rsa_key(n: &[u8], e: &[u8]) -> bool {
    rsa_key(n, e).is_some()
}

/// Whether `signature` is `key`'s RS256 signature of `signed`.
pub(crate) fn rs256_with(key: &rsa::RsaPublicKey, signed: &[u8], signature: &[u8]) -> bool {
    let scheme = rsa::Pkcs1v15Sign {
        hash_len: Some(32),
        prefix: Box::from(&SHA256_DIGEST_INFO[..]),
    };
    key.verify(scheme, &crate::hash::sha256(signed), signature)
        .is_ok()
}

/// Whether `signature` is the RSA key (`n`, `e`)'s RS256 signature of
/// `signed`.
pub fn rs256(n: &[u8], e: &[u8], signed: &[u8], signature: &[u8]) -> bool {
    rsa_key(n, e).is_some_and(|key| rs256_with(&key, signed, signature))
}

/// An ECDSA-Sig-Value (RFC 3279 §2.2.3) in DER as R and S, 32 bytes
/// each; `None` for anything not in DER's one encoding of it.
fn der_signature(der: &[u8]) -> Option<[u8; 64]> {
    let (&tag, rest) = der.split_first()?;
    let (&length, body) = rest.split_first()?;
    // Two integers of at most 33 bytes: always a short-form length.
    if tag != 0x30 || length >= 0x80 || usize::from(length) != body.len() {
        return None;
    }
    let (r, rest) = der_integer(body)?;
    let (s, rest) = der_integer(rest)?;
    if !rest.is_empty() {
        return None;
    }
    let mut raw = [0u8; 64];
    raw.get_mut(32 - r.len()..32)?.copy_from_slice(r);
    raw.get_mut(64 - s.len()..)?.copy_from_slice(s);
    Some(raw)
}

/// A positive DER `INTEGER` of at most 32 bytes' value at the start of
/// `der`, without its sign byte, and what follows it.
fn der_integer(der: &[u8]) -> Option<(&[u8], &[u8])> {
    let (&tag, rest) = der.split_first()?;
    let (&length, rest) = rest.split_first()?;
    let length = usize::from(length);
    if tag != 0x02 || length == 0 || length >= 0x80 || rest.len() < length {
        return None;
    }
    let (value, after) = rest.split_at(length);
    let first = *value.first()?;
    // Negative, or not in its shortest form.
    if first & 0x80 != 0 {
        return None;
    }
    let value = match value {
        [0, second, ..] if second & 0x80 != 0 => value.get(1..)?,
        [0, ..] if length > 1 => return None,
        _ => value,
    };
    (value.len() <= 32).then_some((value, after))
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::SigningKey;
    use p256::ecdsa::signature::hazmat::PrehashSigner;

    /// R and S written as DER, shortest form.
    fn der(raw: &[u8]) -> Vec<u8> {
        let integer = |half: &[u8]| {
            let trimmed: Vec<u8> = half.iter().copied().skip_while(|byte| *byte == 0).collect();
            let mut value = if trimmed.first().is_some_and(|byte| byte & 0x80 != 0) {
                vec![0]
            } else {
                Vec::new()
            };
            value.extend(trimmed);
            let mut out = vec![0x02, value.len() as u8];
            out.extend(value);
            out
        };
        let mut body = integer(&raw[..32]);
        body.extend(integer(&raw[32..]));
        let mut out = vec![0x30, body.len() as u8];
        out.extend(body);
        out
    }

    fn signed(message: &[u8]) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let key = SigningKey::from_slice(&[7u8; 32]).unwrap();
        let point = key.verifying_key().to_sec1_point(false);
        let signature: p256::ecdsa::Signature =
            key.sign_prehash(&crate::hash::sha256(message)).unwrap();
        (
            point.x().unwrap().to_vec(),
            point.y().unwrap().to_vec(),
            der(&signature.to_bytes()),
        )
    }

    #[test]
    fn an_es256_signature_in_der_checks() {
        let (x, y, signature) = signed(b"message");
        assert!(is_p256_key(&x, &y));
        assert!(es256_der(&x, &y, b"message", &signature));
        assert!(!es256_der(&x, &y, b"another", &signature));
        let mut wrong_y = y.clone();
        wrong_y[31] ^= 1;
        assert!(!is_p256_key(&x, &wrong_y));
        assert!(!es256_der(&x, &wrong_y, b"message", &signature));
    }

    /// A sequence of the two integers given as their contents.
    fn sequence(r: &[u8], s: &[u8]) -> Vec<u8> {
        let mut body = vec![0x02, r.len() as u8];
        body.extend(r);
        body.extend([0x02, s.len() as u8]);
        body.extend(s);
        let mut out = vec![0x30, body.len() as u8];
        out.extend(body);
        out
    }

    #[test]
    fn only_ders_one_encoding_is_read() {
        let (x, y, signature) = signed(b"message");
        let mut longer = signature.clone();
        longer.push(0);
        assert!(!es256_der(&x, &y, b"message", &longer));
        assert_eq!(
            der_signature(&sequence(&[1], &[2])).map(|raw| (raw[31], raw[63])),
            Some((1, 2))
        );
        // A needless leading zero, a negative integer, an empty one, zero.
        assert!(der_signature(&sequence(&[0, 1], &[2])).is_none());
        assert!(der_signature(&sequence(&[0x81], &[2])).is_none());
        assert!(der_signature(&sequence(&[], &[2])).is_none());
        assert!(der_signature(&sequence(&[0, 0x81], &[2])).is_some());
        assert!(!es256_der(&x, &y, b"message", &sequence(&[0], &[1])));
        // Longer than the group's order allows.
        assert!(der_signature(&sequence(&[1; 33], &[2])).is_none());
        // R and S raw, as a JWS writes them, aren't DER.
        assert!(der_signature(&[0u8; 64]).is_none());
    }

    /// RFC 7515 §A.2's RS256 signature, over its signing input.
    #[test]
    fn an_rs256_signature_checks() {
        let decode = |text: &str| crate::encoding::from_base64url(text).unwrap();
        let n = decode(
            "ofgWCuLjybRlzo0tZWJjNiuSfb4p4fAkd_wWJcyQoTbji9k0l8W26mPddxHmfHQp-Vaw-4qPCJrcS2mJPMEzP1Pt0Bm4d4QlL-yRT-SFd2lZS-pCgNMsD1W_YpRPEwOWvG6b32690r2jZ47soMZo9wGzjb_7OMg0LOL-bSf63kpaSHSXndS5z5rexMdbBYUsLA9e-KXBdQOS-UTo7WTBEMa2R2CapHg665xsmtdVMTBQY4uDZlxvb3qCo5ZwKh9kG4LT6_I5IhlJH7aGhyxXFvUK-DWNmoudF8NAco9_h9iaGNj8q2ethFkMLs91kzk2PAcDTW9gb54h4FRWyuXpoQ",
        );
        let e = decode("AQAB");
        let input = b"eyJhbGciOiJSUzI1NiJ9.eyJpc3MiOiJqb2UiLA0KICJleHAiOjEzMDA4MTkzODAsDQogImh0dHA6Ly9leGFtcGxlLmNvbS9pc19yb290Ijp0cnVlfQ";
        let signature = decode(
            "cC4hiUPoj9Eetdgtv3hF80EGrhuB__dzERat0XF9g2VtQgr9PJbu3XOiZj5RZmh7AAuHIm4Bh-0Qc_lF5YKt_O8W2Fp5jujGbds9uJdbF9CUAr7t1dnZcAcQjbKBYNX4BAynRFdiuB--f_nZLgrnbyTyWzO75vRK5h6xBArLIARNPvkSjtQBMHlb1L07Qe7K0GarZRmB_eSN9383LcOLn6_dO--xi12jzDwusC-eOkHWEsqtFZESc6BfI7noOPqvhJ1phCnvWh6IeYI2w9QOYEUipUTI8np6LbgGY9Fs98rqVt5AXLIhWkWywlVmtVrBp0igcN_IoypGlUPQGe77Rw",
        );
        assert!(is_rsa_key(&n, &e));
        assert!(rs256(&n, &e, input, &signature));
        assert!(!rs256(&n, &e, b"another", &signature));
        assert!(!rs256(&n, &e, input, &signature[1..]));
    }
}
