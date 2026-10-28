//! Hashing (area 19, "Hashing and cryptography"): a digest
//! is `Bytes`, which `encoding` writes as text. SHA-256 is RustCrypto's
//! `sha2`, pure Rust, so every target gives the same digest.

use sha2::{Digest, Sha256};

/// The 32-byte SHA-256 digest of `bytes`.
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::to_hex;

    #[test]
    fn sha256_gives_fips_180_s_examples() {
        // NIST's examples for FIPS 180-4 (SHA256.pdf): "abc" and the
        // two-block message.
        assert_eq!(
            to_hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            to_hex(&sha256(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }
}
