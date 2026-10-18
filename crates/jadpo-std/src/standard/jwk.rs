//! What a `Jwk` holds (area 19, "Hashing and cryptography", chunks 5.5r
//! and 5.5r4): one public key of one of three kinds, each used with one
//! algorithm, which a key set (RFC 7517) sends and `crypto.verify_jwt`
//! checks a token against.
//!
//! - an `OKP` key (RFC 8037): `crv` `Ed25519` and `x`, used with `EdDSA`;
//! - an `RSA` key (RFC 7518 §6.3.1): `n` and `e`, used with `RS256`, its
//!   modulus from 2,048 to 4,096 bits ([`RSA_MIN_BITS`], [`RSA_MAX_BITS`]);
//! - an `EC` key (RFC 7518 §6.2.1): `crv` `P-256`, `x` and `y`, a point on
//!   the curve, used with `ES256`.
//!
//! Its rules across its parts ([`RULES`]) hold it to one of those: the
//! parts of its `kty` and no other's, the algorithm of its `kty` where it
//! names one, and a key that can be used, so a key set read from outside
//! never holds a key nothing could check with. Nothing here holds a
//! private key or a shared secret: JSON naming one ([`REFUSED_MEMBERS`])
//! isn't a `Jwk`, and any other member it doesn't hold is ignored, as RFC
//! 7517 §4 requires.

use super::across::{Check, Known, Parts, StandardRule, Verdict};
use super::security::decode_base64url;

/// A key's `kty`s, the kinds of key a `Jwk` holds.
pub const OKP: &str = "OKP";
pub const RSA: &str = "RSA";
pub const EC: &str = "EC";

/// The curves a `Jwk`'s `crv` names: an `OKP` key's and an `EC` key's.
pub const ED25519: &str = "Ed25519";
pub const P256: &str = "P-256";

/// The algorithms a key is used with, one for each `kty` (RFC 7518 §3.1,
/// RFC 8037 §3.1).
pub const EDDSA: &str = "EdDSA";
pub const RS256: &str = "RS256";
pub const ES256: &str = "ES256";

/// The fewest bits an RSA key's modulus has: 2,048, the least RFC 7518
/// §3.3 allows and NIST SP 800-131A still accepts (chosen in chunk 5.5r4).
pub const RSA_MIN_BITS: usize = 2048;

/// The most bits an RSA key's modulus has: 4,096, the most `rsa` checks by
/// default, so a key set can't make checking a token cost without bound
/// (chosen in chunk 5.5r4).
pub const RSA_MAX_BITS: usize = 4096;

/// The members of a JWK that hold a private key or a shared secret (RFC
/// 7518 §6.2.2, §6.3.2 and §6.4.1, RFC 8037 §2), and `key_ops`, which
/// limits what a key is for in a way a `Jwk` doesn't hold: JSON naming any
/// of them isn't a `Jwk` (chosen in chunks 5.5r and 5.5r4).
pub const REFUSED_MEMBERS: &[&str] = &["d", "p", "q", "dp", "dq", "qi", "oth", "k", "key_ops"];

/// Whether a member a `Jwk` doesn't hold is ignored when one is read, as
/// RFC 7517 §4 requires of members an implementation doesn't understand
/// (`x5t`, `x5u`, Microsoft's `issuer`): every one but [`REFUSED_MEMBERS`].
pub fn ignores_member(name: &str) -> bool {
    !REFUSED_MEMBERS.contains(&name)
}

/// The one algorithm a key of `kty` is used with.
pub fn algorithm_of(kty: &str) -> Option<&'static str> {
    match kty {
        OKP => Some(EDDSA),
        RSA => Some(RS256),
        EC => Some(ES256),
        _ => None,
    }
}

/// What a `Jwk`'s `kty` takes: its curve, if it names one, the parts it
/// must have, and the parts it never has.
type Shape = (
    Option<&'static str>,
    &'static [&'static str],
    &'static [&'static str],
);

fn shape_of(kty: &str) -> Option<Shape> {
    match kty {
        OKP => Some((Some(ED25519), &["crv", "x"], &["y", "n", "e"])),
        RSA => Some((None, &["n", "e"], &["crv", "x", "y"])),
        EC => Some((Some(P256), &["crv", "x", "y"], &["n", "e"])),
        _ => None,
    }
}

/// Whether `n` is an RSA modulus a `Jwk` holds: in its fewest octets (RFC
/// 7518 §6.3.1.1), odd, as a product of two odd primes is, and of
/// [`RSA_MIN_BITS`] to [`RSA_MAX_BITS`] bits.
pub fn is_rsa_modulus(n: &[u8]) -> bool {
    let Some((&first, _)) = n.split_first() else {
        return false;
    };
    let bits = n.len() * 8 - first.leading_zeros() as usize;
    first != 0
        && n.last().is_some_and(|last| last & 1 == 1)
        && (RSA_MIN_BITS..=RSA_MAX_BITS).contains(&bits)
}

/// Whether `e` is an RSA public exponent a `Jwk` holds: in its fewest
/// octets, odd, and from 3 to 2^32 - 1, which holds 65,537, every
/// identity provider's (chosen in chunk 5.5r4).
pub fn is_rsa_exponent(e: &[u8]) -> bool {
    if e.is_empty() || e.len() > 4 || e.first() == Some(&0) {
        return false;
    }
    let value = e
        .iter()
        .fold(0u64, |sum, byte| (sum << 8) | u64::from(*byte));
    value >= 3 && value & 1 == 1
}

/// A number below 2^256 as four 64-bit limbs, the least significant first.
type U256 = [u64; 4];

/// P-256's prime, 2^256 - 2^224 + 2^192 + 2^96 - 1 (SEC 2 §2.4.2).
const P: U256 = [
    0xffff_ffff_ffff_ffff,
    0x0000_0000_ffff_ffff,
    0x0000_0000_0000_0000,
    0xffff_ffff_0000_0001,
];

/// P-256's `b`; its `a` is -3 (SEC 2 §2.4.2).
const B: U256 = [
    0x3bce_3c3e_27d2_604b,
    0x651d_06b0_cc53_b0f6,
    0xb3eb_bd55_7698_86bc,
    0x5ac6_35d8_aa3a_93e7,
];

fn from_big_endian(bytes: &[u8]) -> Option<U256> {
    if bytes.len() != 32 {
        return None;
    }
    let mut out = [0u64; 4];
    for (limb, chunk) in out.iter_mut().rev().zip(bytes.chunks(8)) {
        *limb = chunk
            .iter()
            .fold(0u64, |sum, byte| (sum << 8) | u64::from(*byte));
    }
    Some(out)
}

fn below(a: &U256, b: &U256) -> bool {
    for (x, y) in a.iter().rev().zip(b.iter().rev()) {
        if x != y {
            return x < y;
        }
    }
    false
}

/// `a - b` modulo 2^256.
fn wrapping_sub(a: &U256, b: &U256) -> U256 {
    let mut out = [0u64; 4];
    let mut borrow = false;
    for ((slot, x), y) in out.iter_mut().zip(a).zip(b) {
        let (step, under) = x.overflowing_sub(*y);
        let (step, under_again) = step.overflowing_sub(u64::from(borrow));
        *slot = step;
        borrow = under || under_again;
    }
    out
}

/// `a + b` modulo 2^256.
fn wrapping_add(a: &U256, b: &U256) -> U256 {
    let mut out = [0u64; 4];
    let mut carry = false;
    for ((slot, x), y) in out.iter_mut().zip(a).zip(b) {
        let (step, over) = x.overflowing_add(*y);
        let (step, over_again) = step.overflowing_add(u64::from(carry));
        *slot = step;
        carry = over || over_again;
    }
    out
}

/// `a - b` modulo P, for `a` and `b` below P.
fn sub_mod(a: &U256, b: &U256) -> U256 {
    let difference = wrapping_sub(a, b);
    if below(a, b) {
        wrapping_add(&difference, &P)
    } else {
        difference
    }
}

/// `a * b` modulo P, for `a` and `b` below P: the 512-bit product,
/// reduced a bit at a time. Slow and plain, which is all checking a key
/// needs; nothing here is secret.
fn mul_mod(a: &U256, b: &U256) -> U256 {
    let mut product = [0u64; 8];
    for (i, x) in a.iter().enumerate() {
        let mut carry = 0u128;
        for (slot, y) in product.iter_mut().skip(i).zip(b) {
            let at = u128::from(*slot) + u128::from(*x) * u128::from(*y) + carry;
            *slot = at as u64;
            carry = at >> 64;
        }
        if let Some(slot) = product.get_mut(i + 4) {
            *slot = carry as u64;
        }
    }
    let mut rest: U256 = [0; 4];
    for bit in (0..512).rev() {
        // Shift in the product's next bit, keeping the bit shifted out.
        let mut carry = product
            .get(bit / 64)
            .map_or(0, |word| (word >> (bit % 64)) & 1);
        for limb in rest.iter_mut() {
            let next = *limb >> 63;
            *limb = (*limb << 1) | carry;
            carry = next;
        }
        // Below 2P before this, so one subtraction brings it below P; the
        // bit shifted out is the 257th of a value that fits once reduced.
        if carry == 1 || !below(&rest, &P) {
            rest = wrapping_sub(&rest, &P);
        }
    }
    rest
}

/// Whether (`x`, `y`), each 32 octets big-endian, is a point on P-256: both
/// below the prime, and y² = x³ - 3x + b (SEC 1 §3.2.2.1's check of a public
/// key, the point at infinity having no coordinates to send).
pub fn on_p256(x: &[u8], y: &[u8]) -> bool {
    let (Some(x), Some(y)) = (from_big_endian(x), from_big_endian(y)) else {
        return false;
    };
    if !below(&x, &P) || !below(&y, &P) {
        return false;
    }
    let left = mul_mod(&y, &y);
    let cube = mul_mod(&mul_mod(&x, &x), &x);
    let less_three_x = sub_mod(&sub_mod(&sub_mod(&cube, &x), &x), &x);
    // Adding b is taking away P - b.
    let right = sub_mod(&less_three_x, &sub_mod(&[0; 4], &B));
    left == right
}

/// A part's text, where the rule knows it; `None` for one only the running
/// program knows.
fn text<'a>(parts: &'a dyn Parts, name: &str) -> Option<Option<&'a str>> {
    match parts.part(name) {
        Known::Absent => Some(None),
        Known::Given(Some(given)) => given.text().map(Some),
        _ => None,
    }
}

/// Whether a part is given; `None` where only the running program knows.
fn given(parts: &dyn Parts, name: &str) -> Option<bool> {
    match parts.part(name) {
        Known::Absent => Some(false),
        Known::Given(_) => Some(true),
        Known::Unknown => None,
    }
}

fn parts_of_its_kty(parts: &dyn Parts) -> Verdict {
    let Some(Some(kty)) = text(parts, "kty") else {
        return Verdict::Unknown;
    };
    let Some((curve, needed, never)) = shape_of(kty) else {
        return Verdict::Breaks("kty");
    };
    let mut unknown = false;
    for &part in needed {
        match given(parts, part) {
            Some(false) => return Verdict::Breaks(part),
            None => unknown = true,
            Some(true) => {}
        }
    }
    for &part in never {
        match given(parts, part) {
            Some(true) => return Verdict::Breaks(part),
            None => unknown = true,
            Some(false) => {}
        }
    }
    if let Some(curve) = curve {
        match text(parts, "crv") {
            Some(Some(crv)) if crv != curve => return Verdict::Breaks("crv"),
            Some(Some(_)) => {}
            _ => unknown = true,
        }
    }
    if unknown {
        Verdict::Unknown
    } else {
        Verdict::Holds
    }
}

fn algorithm_of_its_kty(parts: &dyn Parts) -> Verdict {
    match (text(parts, "kty"), text(parts, "alg")) {
        (_, Some(None)) => Verdict::Holds,
        (Some(Some(kty)), Some(Some(alg))) if algorithm_of(kty) == Some(alg) => Verdict::Holds,
        (Some(Some(_)), Some(Some(_))) => Verdict::Breaks("alg"),
        _ => Verdict::Unknown,
    }
}

fn a_usable_key(parts: &dyn Parts) -> Verdict {
    let bytes = |name: &str| -> Option<Option<Vec<u8>>> {
        Some(text(parts, name)?.map(|text| decode_base64url(text).unwrap_or_default()))
    };
    match text(parts, "kty") {
        Some(Some(RSA)) => match (bytes("n"), bytes("e")) {
            (Some(Some(n)), _) if !is_rsa_modulus(&n) => Verdict::Breaks("n"),
            (_, Some(Some(e))) if !is_rsa_exponent(&e) => Verdict::Breaks("e"),
            (Some(_), Some(_)) => Verdict::Holds,
            _ => Verdict::Unknown,
        },
        Some(Some(EC)) => match (bytes("x"), bytes("y")) {
            (Some(Some(x)), Some(Some(y))) if !on_p256(&x, &y) => Verdict::Breaks("y"),
            (Some(_), Some(_)) => Verdict::Holds,
            _ => Verdict::Unknown,
        },
        Some(_) => Verdict::Holds,
        None => Verdict::Unknown,
    }
}

/// `Jwk`'s rules across its parts, in the order they are checked.
pub const RULES: &[StandardRule] = &[
    StandardRule {
        name: "parts_of_its_kty",
        written: "an `OKP` key has `crv` `Ed25519` and `x`, an `RSA` key `n` and `e`, and an `EC` key `crv` `P-256`, `x` and `y`, and none another's",
        label: "not a part of this kind of key",
        reason: "A key's `kty` says which kind of public key it is, and each kind has its own parts (RFC 7517, RFC 7518 §6, RFC 8037): an `OKP` key `crv` `Ed25519` and `x`, an `RSA` key `n` and `e`, an `EC` key `crv` `P-256`, `x` and `y`.",
        next: "Give the key the parts of its `kty`, and none of another's",
        check: Check::By(parts_of_its_kty),
    },
    StandardRule {
        name: "algorithm_of_its_kty",
        written: "alg is EdDSA for an `OKP` key, RS256 for an `RSA` key and ES256 for an `EC` key",
        label: "not this kind of key's algorithm",
        reason: "Each kind of key is used with one algorithm, so a token can never choose another (RFC 8725 §3.1): `EdDSA` for an `OKP` key, `RS256` for an `RSA` key and `ES256` for an `EC` key.",
        next: "Leave `alg` out, or give the one its `kty` is used with",
        check: Check::By(algorithm_of_its_kty),
    },
    StandardRule {
        name: "a_usable_key",
        written: "an `RSA` key's modulus is 2,048 to 4,096 bits and odd, its exponent odd and from 3 to 2^32 - 1; an `EC` key's point is on P-256",
        label: "not a key anything can check with",
        reason: "An RSA modulus under 2,048 bits can be factored, and one above 4,096 costs too much to check with; an exponent must be odd and at least 3; and an EC key's `x` and `y` must be a point on P-256, or the key is a way to attack whoever checks with it.",
        next: "Give an RSA key of 2,048 to 4,096 bits, or the point of a P-256 key",
        check: Check::By(a_usable_key),
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(text: &str) -> Vec<u8> {
        decode_base64url(text).expect("Base64url")
    }

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&text[at..at + 2], 16).expect("hexadecimal"))
            .collect()
    }

    #[test]
    fn points_on_p256_are_told_from_points_off_it() {
        // The generator (SEC 2 §2.4.2), RFC 7515 §A.3's key and RFC 7517
        // §A.1's.
        assert!(on_p256(
            &hex("6b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c296"),
            &hex("4fe342e2fe1a7f9b8ee7eb4a7c0f9e162bce33576b315ececbb6406837bf51f5")
        ));
        let (x, y) = (
            bytes("f83OJ3D2xF1Bg8vub9tLe1gHMzV76e8Tus9uPHvRVEU"),
            bytes("x_FEzRu9m36HLN_tue659LNpXW6pCyStikYjKIWI5a0"),
        );
        assert!(on_p256(&x, &y));
        assert!(on_p256(
            &bytes("MKBCTNIcKUSDii11ySs3526iDZ8AiTo7Tu6KPAqv7D4"),
            &bytes("4Etl6SRW2YiLUrN5vfvVHuhp7x8PxltmWWlbbM4IFyM")
        ));
        // The same x with y one more, its y negated plus one, coordinates
        // past the prime, and the wrong lengths.
        let mut off = y.clone();
        off[31] ^= 1;
        assert!(!on_p256(&x, &off));
        assert!(!on_p256(&[0xff; 32], &y));
        assert!(!on_p256(&x, &[0xff; 32]));
        assert!(!on_p256(&[0; 32], &[0; 32]));
        assert!(!on_p256(&x[1..], &y));
        // -y is on the curve too.
        let p = hex("ffffffff00000001000000000000000000000000ffffffffffffffffffffffff");
        let (Some(p), Some(y_value)) = (from_big_endian(&p), from_big_endian(&y)) else {
            panic!("32 octets");
        };
        let minus_y = wrapping_sub(&p, &y_value);
        let minus_y: Vec<u8> = minus_y
            .iter()
            .rev()
            .flat_map(|limb| limb.to_be_bytes())
            .collect();
        assert!(on_p256(&x, &minus_y));
    }

    #[test]
    fn an_rsa_key_is_big_enough_and_odd() {
        // RFC 7515 §A.2's modulus, 2,048 bits.
        let n = bytes(
            "ofgWCuLjybRlzo0tZWJjNiuSfb4p4fAkd_wWJcyQoTbji9k0l8W26mPddxHmfHQp-Vaw-4qPCJrcS2mJPMEzP1Pt0Bm4d4QlL-yRT-SFd2lZS-pCgNMsD1W_YpRPEwOWvG6b32690r2jZ47soMZo9wGzjb_7OMg0LOL-bSf63kpaSHSXndS5z5rexMdbBYUsLA9e-KXBdQOS-UTo7WTBEMa2R2CapHg665xsmtdVMTBQY4uDZlxvb3qCo5ZwKh9kG4LT6_I5IhlJH7aGhyxXFvUK-DWNmoudF8NAco9_h9iaGNj8q2ethFkMLs91kzk2PAcDTW9gb54h4FRWyuXpoQ",
        );
        assert!(is_rsa_modulus(&n));
        let mut even = n.clone();
        if let Some(last) = even.last_mut() {
            *last &= 0xfe;
        }
        assert!(!is_rsa_modulus(&even));
        assert!(!is_rsa_modulus(&n[1..]));
        let mut padded = vec![0];
        padded.extend_from_slice(&n);
        assert!(!is_rsa_modulus(&padded));
        let mut huge = vec![0xff; 513];
        huge[512] = 1;
        assert!(!is_rsa_modulus(&huge));
        assert!(is_rsa_modulus(&huge[1..]));
        assert!(is_rsa_exponent(&[1, 0, 1]));
        assert!(is_rsa_exponent(&[3]));
        assert!(!is_rsa_exponent(&[1]));
        assert!(!is_rsa_exponent(&[2]));
        assert!(!is_rsa_exponent(&[0, 1, 0, 1]));
        assert!(!is_rsa_exponent(&[1, 0, 0, 0, 1]));
        assert!(!is_rsa_exponent(&[]));
    }

    #[test]
    fn each_kty_has_its_own_algorithm() {
        assert_eq!(algorithm_of(OKP), Some(EDDSA));
        assert_eq!(algorithm_of(RSA), Some(RS256));
        assert_eq!(algorithm_of(EC), Some(ES256));
        assert_eq!(algorithm_of("oct"), None);
        assert!(ignores_member("x5t"));
        assert!(ignores_member("issuer"));
        assert!(!ignores_member("d"));
        assert!(!ignores_member("k"));
        assert!(!ignores_member("key_ops"));
    }
}
