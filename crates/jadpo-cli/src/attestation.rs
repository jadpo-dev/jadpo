//! The GitHub artifact attestation of a release's archive, checked before
//! the toolchain launcher runs a `jadpo` it fetched (area 14, "Which jadpo
//! runs"; roadmap chunk D14).
//!
//! Each archive a release carries is attested as it is built
//! (`.github/workflows/release.yml`, `github-attestations` in
//! `dist-workspace.toml`): a Sigstore bundle whose DSSE envelope holds an
//! in-toto statement naming the archive's SHA-256, signed with a Fulcio
//! certificate for the workflow run. [`Sigstore`] fetches the bundles
//! GitHub holds for the archive's digest
//! (`api.github.com/repos/<repository>/attestations/sha256:<digest>`) and
//! accepts the archive when one of them verifies with Sigstore's own
//! verifier, `sigstore-verify`, against the public-good trusted root it
//! carries: the certificate chains to Fulcio and was valid when the
//! transparency log recorded the signature, the log entry and its
//! inclusion are proved, the envelope's signature is good, the statement
//! names this digest, and the certificate was issued by GitHub Actions to
//! `release.yml` running at the release's tag, `refs/tags/v<version>`,
//! in this repository. Nothing else is accepted.
//!
//! The verifier's cryptography is RustCrypto's, through [`RustCrypto`],
//! a provider of this crate's own over the curves and hashes the workspace
//! already holds (P-256, P-384, Ed25519, SHA-2), so no C library is built;
//! RSA and ML-DSA, which neither Fulcio nor Rekor uses, are refused.

use sigstore_crypto::{CryptoProvider, HashContext, SigningScheme};
use sigstore_trust_root::{SIGSTORE_PRODUCTION_TRUSTED_ROOT, TrustedRoot};
use sigstore_types::{Artifact, Bundle, HashAlgorithm, Sha256Hash};
use sigstore_verify::{IdentityMatcher, VerificationPolicy};

use crate::releases::{self, Network};

/// The issuer of every GitHub Actions workflow's certificates.
const ACTIONS: &str = "https://token.actions.githubusercontent.com";

/// The most an attestation answer may be.
const LONGEST: usize = 16 << 20;

/// How a release's archive is checked against its attestation.
pub(crate) trait Attest {
    /// Whether `digest`, the SHA-256 in hex of release `version`'s archive,
    /// is attested as built by this repository's release workflow at that
    /// version's tag: why not, when it isn't.
    fn check(&self, network: &dyn Network, version: &str, digest: &str) -> Result<(), String>;
}

/// The attestation GitHub holds for a release of `repository`
/// (`jadpo-dev/jadpo`), verified with Sigstore's verifier.
pub(crate) struct Sigstore {
    pub(crate) repository: &'static str,
}

impl Sigstore {
    /// Jadpo's own releases'.
    pub(crate) const JADPO: Sigstore = Sigstore {
        repository: releases::REPOSITORY,
    };
}

impl Attest for Sigstore {
    fn check(&self, network: &dyn Network, version: &str, digest: &str) -> Result<(), String> {
        let url = format!(
            "https://api.github.com/repos/{}/attestations/sha256:{digest}",
            self.repository
        );
        let answer = network.get(&url, LONGEST)?;
        let bundles = bundles(&answer)?;
        if bundles.is_empty() {
            return Err("GitHub holds no attestation for its archive.".to_string());
        }
        let identity = workflow(self.repository, version);
        let mut last = String::new();
        for bundle in &bundles {
            match verify(bundle, digest, &identity) {
                Ok(()) => return Ok(()),
                Err(why) => last = why,
            }
        }
        Err(format!("its archive's attestation doesn't verify: {last}"))
    }
}

/// The certificate identity `repository`'s release `version`'s archives
/// are signed with: its `release.yml` at the release's tag, which
/// release-please starts it on.
pub(crate) fn workflow(repository: &str, version: &str) -> String {
    format!("https://github.com/{repository}/.github/workflows/release.yml@refs/tags/v{version}")
}

/// The bundles in GitHub's answer for a digest, as JSON.
fn bundles(answer: &[u8]) -> Result<Vec<String>, String> {
    let read: serde_json::Value = serde_json::from_slice(answer)
        .map_err(|_| "GitHub's answer about its attestation isn't JSON.".to_string())?;
    Ok(read
        .get("attestations")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|attestation| attestation.get("bundle"))
        .filter(|bundle| bundle.is_object())
        .map(serde_json::Value::to_string)
        .collect())
}

/// Whether `bundle` attests `digest` (SHA-256, hex), signed by a GitHub
/// Actions certificate issued to `identity`, against the public-good
/// trusted root.
pub(crate) fn verify(bundle: &str, digest: &str, identity: &str) -> Result<(), String> {
    let _ = sigstore_crypto::install_default_provider(&RUST_CRYPTO);
    let root = TrustedRoot::from_json(SIGSTORE_PRODUCTION_TRUSTED_ROOT)
        .map_err(|error| format!("the trusted root this jadpo carries doesn't read: {error}"))?;
    let bundle = Bundle::from_json(bundle).map_err(|error| error.to_string())?;
    let digest = Sha256Hash::from_hex(digest).map_err(|error| error.to_string())?;
    let policy = VerificationPolicy::new(IdentityMatcher::Uri(identity.to_string()), ACTIONS);
    sigstore_verify::verify(Artifact::from_digest(digest), &bundle, &policy, &root)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// The verifier's cryptography over RustCrypto's crates.
#[derive(Debug)]
pub(crate) struct RustCrypto;

static RUST_CRYPTO: RustCrypto = RustCrypto;

/// A SHA-2 digest under way.
struct Hashing(Sha2);

enum Sha2 {
    S256(sha2::Sha256),
    S384(sha2::Sha384),
    S512(sha2::Sha512),
}

impl HashContext for Hashing {
    fn update(&mut self, data: &[u8]) {
        use sha2::Digest;
        match &mut self.0 {
            Sha2::S256(hasher) => hasher.update(data),
            Sha2::S384(hasher) => hasher.update(data),
            Sha2::S512(hasher) => hasher.update(data),
        }
    }

    fn finalize(self: Box<Self>) -> Vec<u8> {
        use sha2::Digest;
        match self.0 {
            Sha2::S256(hasher) => hasher.finalize().to_vec(),
            Sha2::S384(hasher) => hasher.finalize().to_vec(),
            Sha2::S512(hasher) => hasher.finalize().to_vec(),
        }
    }
}

fn invalid(scheme: SigningScheme) -> sigstore_crypto::Error {
    sigstore_crypto::Error::Verification(format!("{} signature invalid", scheme.name()))
}

impl CryptoProvider for RustCrypto {
    fn hash(&self, algorithm: HashAlgorithm) -> sigstore_crypto::Result<Box<dyn HashContext>> {
        use sha2::Digest;
        Ok(Box::new(Hashing(match algorithm {
            HashAlgorithm::Sha2256 => Sha2::S256(sha2::Sha256::new()),
            HashAlgorithm::Sha2384 => Sha2::S384(sha2::Sha384::new()),
            HashAlgorithm::Sha2512 => Sha2::S512(sha2::Sha512::new()),
            other => {
                return Err(sigstore_crypto::Error::UnsupportedAlgorithm(format!(
                    "{other:?} isn't a SHA-2 digest"
                )));
            }
        })))
    }

    fn verify_raw(
        &self,
        scheme: SigningScheme,
        key: &[u8],
        data: &[u8],
        signature: &[u8],
    ) -> sigstore_crypto::Result<()> {
        if scheme == SigningScheme::Ed25519 {
            let key = <[u8; 32]>::try_from(key)
                .ok()
                .and_then(|key| ed25519_dalek::VerifyingKey::from_bytes(&key).ok())
                .ok_or_else(|| invalid(scheme))?;
            let signature = <[u8; 64]>::try_from(signature).map_err(|_| invalid(scheme))?;
            return key
                .verify_strict(data, &ed25519_dalek::Signature::from_bytes(&signature))
                .map_err(|_| invalid(scheme));
        }
        let algorithm = scheme.hash_algorithm().ok_or_else(|| unsupported(scheme))?;
        let mut hashing = self.hash(algorithm)?;
        hashing.update(data);
        self.verify_prehashed(scheme, key, &hashing.finalize(), signature)
    }

    fn verify_prehashed(
        &self,
        scheme: SigningScheme,
        key: &[u8],
        digest: &[u8],
        signature: &[u8],
    ) -> sigstore_crypto::Result<()> {
        use p256::ecdsa::signature::hazmat::PrehashVerifier;
        let good = match scheme {
            SigningScheme::EcdsaP256Sha256 | SigningScheme::EcdsaP256Sha384 => {
                let key =
                    p256::ecdsa::VerifyingKey::from_sec1_bytes(key).map_err(|_| invalid(scheme))?;
                let raw = der_signature::<32>(signature).ok_or_else(|| invalid(scheme))?;
                let signature =
                    p256::ecdsa::Signature::from_slice(&raw).map_err(|_| invalid(scheme))?;
                key.verify_prehash(digest, &signature).is_ok()
            }
            SigningScheme::EcdsaP384Sha256 | SigningScheme::EcdsaP384Sha384 => {
                let key =
                    p384::ecdsa::VerifyingKey::from_sec1_bytes(key).map_err(|_| invalid(scheme))?;
                let raw = der_signature::<48>(signature).ok_or_else(|| invalid(scheme))?;
                let signature =
                    p384::ecdsa::Signature::from_slice(&raw).map_err(|_| invalid(scheme))?;
                key.verify_prehash(digest, &signature).is_ok()
            }
            _ => return Err(unsupported(scheme)),
        };
        if good { Ok(()) } else { Err(invalid(scheme)) }
    }
}

fn unsupported(scheme: SigningScheme) -> sigstore_crypto::Error {
    sigstore_crypto::Error::UnsupportedAlgorithm(format!(
        "{} isn't used by Sigstore's public-good instance",
        scheme.name()
    ))
}

/// An ECDSA-Sig-Value (RFC 3279 §2.2.3) in DER as R and S, `N` bytes each;
/// `None` for anything not in DER's one encoding of it.
fn der_signature<const N: usize>(der: &[u8]) -> Option<Vec<u8>> {
    let (&tag, rest) = der.split_first()?;
    if tag != 0x30 {
        return None;
    }
    let (length, body) = der_length(rest)?;
    if length != body.len() {
        return None;
    }
    let (r, rest) = der_integer::<N>(body)?;
    let (s, rest) = der_integer::<N>(rest)?;
    if !rest.is_empty() {
        return None;
    }
    let mut raw = vec![0u8; 2 * N];
    raw.get_mut(N - r.len()..N)?.copy_from_slice(r);
    raw.get_mut(2 * N - s.len()..)?.copy_from_slice(s);
    Some(raw)
}

/// A DER length in its shortest form, and what follows it: one byte below
/// 128, else `0x81` and one byte, which is as long as an ECDSA signature
/// gets.
fn der_length(der: &[u8]) -> Option<(usize, &[u8])> {
    match der.split_first()? {
        (&short, rest) if short < 0x80 => Some((usize::from(short), rest)),
        (0x81, rest) => {
            let (&long, rest) = rest.split_first()?;
            (long >= 0x80).then_some((usize::from(long), rest))
        }
        _ => None,
    }
}

/// A positive DER `INTEGER` of at most `N` bytes' value at the start of
/// `der`, without its sign byte, and what follows it.
fn der_integer<const N: usize>(der: &[u8]) -> Option<(&[u8], &[u8])> {
    let (&tag, rest) = der.split_first()?;
    let (length, rest) = der_length(rest)?;
    if tag != 0x02 || length == 0 || rest.len() < length {
        return None;
    }
    let (value, after) = rest.split_at(length);
    let first = *value.first()?;
    if first & 0x80 != 0 {
        return None;
    }
    let value = match value {
        [0, second, ..] if second & 0x80 != 0 => value.get(1..)?,
        [0, ..] if length > 1 => return None,
        _ => value,
    };
    (value.len() <= N).then_some((value, after))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real GitHub Actions attestation from Sigstore's public-good
    /// instance, from sigstore-rust's own tests (Apache-2.0): a DSSE
    /// envelope over an in-toto statement for a conda package, signed by
    /// `prefix-dev/sigstore-example`'s workflow.
    const BUNDLE: &str = include_str!("../tests/fixtures/attestation/conda.sigstore.json");
    const DIGEST: &str = "54303491a8418fbed24344b513546182c29b43bf282ceb433af65e2299f9271f";
    const SIGNER: &str = "https://github.com/prefix-dev/sigstore-example/.github/workflows/action.yaml@refs/heads/main";

    /// GitHub's real answer for the archive of dist 0.33.1 for
    /// aarch64-apple-darwin, whose SHA-256 `.github/install-dist.sh` pins:
    /// two attestations, GitHub's own release attestation and the SLSA
    /// provenance dist's `release.yml` made at the tag, as Jadpo's own
    /// release workflow makes for each archive.
    const DIST: &str =
        include_str!("../tests/fixtures/attestation/dist-0.33.1-aarch64-apple-darwin.json");
    const DIST_DIGEST: &str = "b8bf041a38e9e54ecf41eee2574ca6644043796620bb0ee8340e9c0acbaaae8f";

    /// GitHub serving `answer` for `repository`'s attestations of `digest`.
    struct Answers(String, Vec<u8>);

    impl Network for Answers {
        fn get(&self, url: &str, _: usize) -> Result<Vec<u8>, String> {
            if url == self.0 {
                Ok(self.1.clone())
            } else {
                Err(format!("No connection for `{url}`."))
            }
        }
    }

    fn dist() -> Answers {
        Answers(
            format!(
                "https://api.github.com/repos/axodotdev/cargo-dist/attestations/sha256:{DIST_DIGEST}"
            ),
            DIST.as_bytes().to_vec(),
        )
    }

    #[test]
    fn a_real_release_archives_attestation_verifies_for_its_tag() {
        let dist_releases = Sigstore {
            repository: "axodotdev/cargo-dist",
        };
        dist_releases
            .check(&dist(), "0.33.1", DIST_DIGEST)
            .expect("verifies");
        // Another tag's workflow didn't build it.
        let refused = dist_releases
            .check(&dist(), "0.33.0", DIST_DIGEST)
            .expect_err("another tag");
        assert!(
            refused.starts_with("its archive's attestation doesn't verify"),
            "{refused}"
        );
        // Nor did Jadpo's.
        assert!(
            Sigstore::JADPO
                .check(&dist(), "0.33.1", DIST_DIGEST)
                .is_err()
        );
    }

    #[test]
    fn a_real_attestation_verifies_for_its_digest_and_signer() {
        verify(BUNDLE, DIGEST, SIGNER).expect("verifies");
    }

    #[test]
    fn another_digest_or_signer_is_refused() {
        let other = DIGEST.replace('5', "6");
        assert!(verify(BUNDLE, &other, SIGNER).is_err());
        let ours = workflow(releases::REPOSITORY, "0.1.0");
        assert_eq!(
            ours,
            "https://github.com/jadpo-dev/jadpo/.github/workflows/release.yml@refs/tags/v0.1.0"
        );
        assert!(verify(BUNDLE, DIGEST, &ours).is_err());
    }

    #[test]
    fn a_tampered_signature_is_refused() {
        let mut read: serde_json::Value = serde_json::from_str(BUNDLE).expect("JSON");
        let signature = read["dsseEnvelope"]["signatures"][0]["sig"]
            .as_str()
            .expect("a signature")
            .to_string();
        let flipped = if signature.starts_with('M') { "N" } else { "M" };
        read["dsseEnvelope"]["signatures"][0]["sig"] =
            serde_json::Value::from(format!("{flipped}{}", &signature[1..]));
        assert!(verify(&read.to_string(), DIGEST, SIGNER).is_err());
    }

    #[test]
    fn github_answers_are_read_for_their_bundles() {
        let answer = format!(r#"{{"attestations":[{{"bundle":{BUNDLE},"repository_id":1}}]}}"#);
        assert_eq!(bundles(answer.as_bytes()).expect("reads").len(), 1);
        assert_eq!(bundles(b"{\"attestations\":[]}").expect("reads").len(), 0);
        assert!(bundles(b"<html>").is_err());
    }

    #[test]
    fn signatures_are_read_from_der_strictly() {
        // SEQUENCE { INTEGER 1, INTEGER 0x80 (with its sign byte) }.
        let der = [0x30, 0x07, 0x02, 0x01, 0x01, 0x02, 0x02, 0x00, 0x80];
        let raw = der_signature::<32>(&der).expect("reads");
        assert_eq!((raw[31], raw[63]), (1, 0x80));
        // A sign byte that isn't needed, and a long length that could be short.
        assert!(
            der_signature::<32>(&[0x30, 0x07, 0x02, 0x02, 0x00, 0x01, 0x02, 0x01, 0x01]).is_none()
        );
        assert!(
            der_signature::<32>(&[0x30, 0x81, 0x06, 0x02, 0x01, 0x01, 0x02, 0x01, 0x01]).is_none()
        );
    }
}
