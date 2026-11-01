//! The log: an append-only Merkle tree of releases, hashed as RFC 6962
//! does with SHA-256 (area 11, "The registry protocol"). Its proofs are
//! checked against RFC 6962's own test vectors, against a second,
//! recursive implementation of the RFC's definitions for every pair of
//! sizes, and with every way of tampering with them.

use jadpo_registry::log::{
    Hash, Log, LogEntry, LogError, LogHead, check_extends, check_included, leaf_hash,
    verify_consistency, verify_inclusion,
};
use jadpo_registry::names::{Digest, ScopedName, Version};
use jadpo_registry::protocol::{ConsistencyProof, InclusionProof};
use sha2::{Digest as _, Sha256};

fn h(hex: &str) -> Hash {
    Hash::from_hex(hex).expect("a hash")
}

fn bytes(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

/// RFC 6962's eight test leaves, as the certificate-transparency
/// repositories test with.
const LEAVES: [&str; 8] = [
    "",
    "00",
    "10",
    "2021",
    "3031",
    "40414243",
    "5051525354555657",
    "606162636465666768696a6b6c6d6e6f",
];

/// The root of the tree of the first `n` leaves above, for `n` from 1.
const ROOTS: [&str; 8] = [
    "6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d",
    "fac54203e7cc696cf0dfcb42c92a1d9dbaf70ad9e621f4bd8d98662f00e3c125",
    "aeb6bcfe274b70a14fb067a5e5578264db0fa9b51af5e0ba159158f329e06e77",
    "d37ee418976dd95753c1c73862b9398fa2a2cf9b4ff0fdfe8b30cd95209614b7",
    "4e3bbb1f7b478dcfe71fb631631519a3bca12c9aefca1612bfce4c13a86264d4",
    "76e67dadbcdf1e10e1b74ddc608abd2f98dfb16fbce75277b5232a127f2087ef",
    "ddb89be403809e325750d3d263cd78929c2942b7942a34b77e122c9594a74c8c",
    "5dc9da79a70659a9ad559cb701ded9a2ab9d823aad2f4960cfe370eff4604328",
];

fn rfc_log() -> Log {
    let mut log = Log::new();
    for leaf in LEAVES {
        log.append_leaf(&bytes(leaf));
    }
    log
}

#[test]
fn the_empty_tree_is_the_hash_of_nothing() {
    let log = Log::new();
    assert_eq!(
        log.root(0),
        Some(h(
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        ))
    );
    assert_eq!(log.root(1), None);
}

#[test]
fn roots_match_rfc_6962s_test_vectors() {
    let log = rfc_log();
    for (n, root) in ROOTS.iter().enumerate() {
        assert_eq!(log.root(n as u64 + 1), Some(h(root)), "size {}", n + 1);
    }
}

#[test]
fn inclusion_proofs_match_rfc_6962s_test_vectors() {
    let log = rfc_log();
    let cases: [(u64, u64, &[&str]); 5] = [
        (0, 1, &[]),
        (
            0,
            8,
            &[
                "96a296d224f285c67bee93c30f8a309157f0daa35dc5b87e410b78630a09cfc7",
                "5f083f0a1a33ca076a95279832580db3e0ef4584bdff1f54c8a360f50de3031e",
                "6b47aaf29ee3c2af9af889bc1fb9254dabd31177f16232dd6aab035ca39bf6e4",
            ],
        ),
        (
            5,
            8,
            &[
                "bc1a0643b12e4d2d7c77918f44e0f4f79a838b6cf9ec5b5c283e1f4d88599e6b",
                "ca854ea128ed050b41b35ffc1b87b8eb2bde461e9e3b5596ece6b9d5975a0ae0",
                "d37ee418976dd95753c1c73862b9398fa2a2cf9b4ff0fdfe8b30cd95209614b7",
            ],
        ),
        (
            2,
            3,
            &["fac54203e7cc696cf0dfcb42c92a1d9dbaf70ad9e621f4bd8d98662f00e3c125"],
        ),
        (
            1,
            5,
            &[
                "6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d",
                "5f083f0a1a33ca076a95279832580db3e0ef4584bdff1f54c8a360f50de3031e",
                "bc1a0643b12e4d2d7c77918f44e0f4f79a838b6cf9ec5b5c283e1f4d88599e6b",
            ],
        ),
    ];
    for (index, size, expected) in cases {
        let proof = log.inclusion_proof(index, size).expect("a proof");
        let expected: Vec<Hash> = expected.iter().map(|x| h(x)).collect();
        assert_eq!(proof, expected, "leaf {index} of {size}");
        let leaf = leaf_hash(&bytes(LEAVES[index as usize]));
        assert!(verify_inclusion(
            &leaf,
            index,
            size,
            &proof,
            &log.root(size).unwrap()
        ));
    }
}

#[test]
fn consistency_proofs_match_rfc_6962s_test_vectors() {
    let log = rfc_log();
    let cases: [(u64, u64, &[&str]); 4] = [
        (1, 1, &[]),
        (
            1,
            8,
            &[
                "96a296d224f285c67bee93c30f8a309157f0daa35dc5b87e410b78630a09cfc7",
                "5f083f0a1a33ca076a95279832580db3e0ef4584bdff1f54c8a360f50de3031e",
                "6b47aaf29ee3c2af9af889bc1fb9254dabd31177f16232dd6aab035ca39bf6e4",
            ],
        ),
        (
            6,
            8,
            &[
                "0ebc5d3437fbe2db158b9f126a1d118e308181031d0a949f8dededebc558ef6a",
                "ca854ea128ed050b41b35ffc1b87b8eb2bde461e9e3b5596ece6b9d5975a0ae0",
                "d37ee418976dd95753c1c73862b9398fa2a2cf9b4ff0fdfe8b30cd95209614b7",
            ],
        ),
        (
            2,
            5,
            &[
                "5f083f0a1a33ca076a95279832580db3e0ef4584bdff1f54c8a360f50de3031e",
                "bc1a0643b12e4d2d7c77918f44e0f4f79a838b6cf9ec5b5c283e1f4d88599e6b",
            ],
        ),
    ];
    for (from, to, expected) in cases {
        let proof = log.consistency_proof(from, to).expect("a proof");
        let expected: Vec<Hash> = expected.iter().map(|x| h(x)).collect();
        assert_eq!(proof, expected, "{from} to {to}");
        assert!(verify_consistency(
            from,
            to,
            &log.root(from).unwrap(),
            &log.root(to).unwrap(),
            &proof
        ));
    }
}

/// RFC 6962 §2.1's definitions, written again as the RFC writes them, by
/// recursion over the leaves, so the log's own iterative proofs are held
/// to an implementation that shares nothing with them.
mod reference {
    use super::*;

    fn sha(parts: &[&[u8]]) -> [u8; 32] {
        let mut hasher = Sha256::new();
        for part in parts {
            hasher.update(part);
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(&hasher.finalize());
        out
    }

    fn split(n: usize) -> usize {
        let mut k = 1;
        while k * 2 < n {
            k *= 2;
        }
        k
    }

    pub fn mth(leaves: &[Vec<u8>]) -> [u8; 32] {
        match leaves.len() {
            0 => sha(&[]),
            1 => sha(&[&[0], &leaves[0]]),
            n => {
                let k = split(n);
                sha(&[&[1], &mth(&leaves[..k]), &mth(&leaves[k..])])
            }
        }
    }

    pub fn path(m: usize, leaves: &[Vec<u8>]) -> Vec<[u8; 32]> {
        let n = leaves.len();
        if n == 1 {
            return Vec::new();
        }
        let k = split(n);
        if m < k {
            let mut out = path(m, &leaves[..k]);
            out.push(mth(&leaves[k..]));
            out
        } else {
            let mut out = path(m - k, &leaves[k..]);
            out.push(mth(&leaves[..k]));
            out
        }
    }

    fn subproof(m: usize, leaves: &[Vec<u8>], whole: bool) -> Vec<[u8; 32]> {
        let n = leaves.len();
        if m == n {
            return if whole { Vec::new() } else { vec![mth(leaves)] };
        }
        let k = split(n);
        if m <= k {
            let mut out = subproof(m, &leaves[..k], whole);
            out.push(mth(&leaves[k..]));
            out
        } else {
            let mut out = subproof(m - k, &leaves[k..], false);
            out.push(mth(&leaves[..k]));
            out
        }
    }

    pub fn proof(m: usize, leaves: &[Vec<u8>]) -> Vec<[u8; 32]> {
        subproof(m, leaves, true)
    }
}

/// Leaves that are all different, so no tampered proof can verify by a
/// coincidence of equal subtrees.
fn distinct_leaves(n: usize) -> Vec<Vec<u8>> {
    (0..n)
        .map(|i| format!("release {i}").into_bytes())
        .collect()
}

fn as_bytes(proof: &[Hash]) -> Vec<[u8; 32]> {
    proof.iter().map(|hash| *hash.as_bytes()).collect()
}

const N: usize = 40;

#[test]
fn every_inclusion_proof_is_the_rfcs_and_verifies() {
    let leaves = distinct_leaves(N);
    let mut log = Log::new();
    for leaf in &leaves {
        log.append_leaf(leaf);
    }
    for size in 1..=N {
        let root = log.root(size as u64).unwrap();
        assert_eq!(
            *root.as_bytes(),
            reference::mth(&leaves[..size]),
            "root {size}"
        );
        for index in 0..size {
            let proof = log.inclusion_proof(index as u64, size as u64).unwrap();
            assert_eq!(
                as_bytes(&proof),
                reference::path(index, &leaves[..size]),
                "leaf {index} of {size}"
            );
            let leaf = leaf_hash(&leaves[index]);
            assert!(
                verify_inclusion(&leaf, index as u64, size as u64, &proof, &root),
                "leaf {index} of {size}"
            );
        }
    }
    assert_eq!(log.inclusion_proof(N as u64, N as u64), None);
    assert_eq!(log.inclusion_proof(0, N as u64 + 1), None);
}

#[test]
fn every_consistency_proof_is_the_rfcs_and_verifies() {
    let leaves = distinct_leaves(N);
    let mut log = Log::new();
    for leaf in &leaves {
        log.append_leaf(leaf);
    }
    for to in 1..=N {
        for from in 1..=to {
            let proof = log.consistency_proof(from as u64, to as u64).unwrap();
            assert_eq!(
                as_bytes(&proof),
                reference::proof(from, &leaves[..to]),
                "{from} to {to}"
            );
            assert!(
                verify_consistency(
                    from as u64,
                    to as u64,
                    &log.root(from as u64).unwrap(),
                    &log.root(to as u64).unwrap(),
                    &proof
                ),
                "{from} to {to}"
            );
        }
    }
    // Any tree extends the empty one, with nothing to prove it.
    assert_eq!(log.consistency_proof(0, 5), Some(Vec::new()));
    assert!(verify_consistency(
        0,
        5,
        &log.root(0).unwrap(),
        &log.root(5).unwrap(),
        &[]
    ));
    assert_eq!(log.consistency_proof(5, 4), None);
    assert_eq!(log.consistency_proof(1, N as u64 + 1), None);
}

/// Every copy of `proof` with one byte of one hash changed, one hash
/// left off or added, or two hashes swapped.
fn tampered(proof: &[Hash]) -> Vec<Vec<Hash>> {
    let mut out = Vec::new();
    for i in 0..proof.len() {
        for byte in [0usize, 13, 31] {
            let mut copy = proof.to_vec();
            let mut raw = *copy[i].as_bytes();
            raw[byte] ^= 0x01;
            copy[i] = Hash::from_bytes(raw);
            out.push(copy);
        }
        let mut shorter = proof.to_vec();
        shorter.remove(i);
        out.push(shorter);
        for j in i + 1..proof.len() {
            let mut swapped = proof.to_vec();
            swapped.swap(i, j);
            if swapped != proof {
                out.push(swapped);
            }
        }
    }
    let mut longer = proof.to_vec();
    longer.push(leaf_hash(b"extra"));
    out.push(longer);
    let mut longer = proof.to_vec();
    longer.insert(0, leaf_hash(b"extra"));
    out.push(longer);
    out
}

#[test]
fn a_tampered_inclusion_proof_is_refused() {
    let leaves = distinct_leaves(N);
    let mut log = Log::new();
    for leaf in &leaves {
        log.append_leaf(leaf);
    }
    for size in 1..=N as u64 {
        let root = log.root(size).unwrap();
        for index in 0..size {
            let proof = log.inclusion_proof(index, size).unwrap();
            let leaf = leaf_hash(&leaves[index as usize]);
            for bad in tampered(&proof) {
                assert!(
                    !verify_inclusion(&leaf, index, size, &bad, &root),
                    "leaf {index} of {size} with {bad:?}"
                );
            }
            // Another leaf, another index, another size or another root.
            let other = leaf_hash(&leaves[(index as usize + 1) % N]);
            assert!(!verify_inclusion(&other, index, size, &proof, &root));
            assert!(!verify_inclusion(
                &leaf,
                index,
                size,
                &proof,
                &leaf_hash(b"root")
            ));
            assert!(!verify_inclusion(&leaf, size, size, &proof, &root));
            for wrong in [index + 1, index.wrapping_sub(1)] {
                if wrong < size {
                    assert!(
                        !verify_inclusion(&leaf, wrong, size, &proof, &root),
                        "leaf {index} of {size} as {wrong}"
                    );
                }
            }
            for wrong in [size - 1, size + 1] {
                if wrong <= index || wrong == 0 || wrong > N as u64 {
                    continue;
                }
                if log.inclusion_proof(index, wrong).as_deref() == Some(&proof[..]) {
                    continue;
                }
                assert!(
                    !verify_inclusion(&leaf, index, wrong, &proof, &log.root(wrong).unwrap()),
                    "leaf {index} of {size} as of {wrong}"
                );
            }
        }
    }
}

#[test]
fn a_tampered_consistency_proof_is_refused() {
    let leaves = distinct_leaves(N);
    let mut log = Log::new();
    for leaf in &leaves {
        log.append_leaf(leaf);
    }
    for to in 2..=N as u64 {
        for from in 1..to {
            let proof = log.consistency_proof(from, to).unwrap();
            let old = log.root(from).unwrap();
            let new = log.root(to).unwrap();
            for bad in tampered(&proof) {
                assert!(
                    !verify_consistency(from, to, &old, &new, &bad),
                    "{from} to {to} with {bad:?}"
                );
            }
            assert!(!verify_consistency(from, to, &new, &old, &proof));
            assert!(!verify_consistency(
                from,
                to,
                &leaf_hash(b"old"),
                &new,
                &proof
            ));
            assert!(!verify_consistency(
                from,
                to,
                &old,
                &leaf_hash(b"new"),
                &proof
            ));
            assert!(!verify_consistency(to, from, &new, &old, &proof));
            for (f, t) in [
                (from + 1, to),
                (from - 1, to),
                (from, to + 1),
                (from, to - 1),
            ] {
                if f == 0 || f >= t || t > N as u64 || (f, t) == (from, to) {
                    continue;
                }
                if log.consistency_proof(f, t).as_deref() == Some(&proof[..]) {
                    continue;
                }
                assert!(
                    !verify_consistency(f, t, &log.root(f).unwrap(), &log.root(t).unwrap(), &proof),
                    "{from} to {to} as {f} to {t}"
                );
            }
        }
    }
}

#[test]
fn equal_sizes_need_equal_roots_and_no_hashes() {
    let log = rfc_log();
    let root = log.root(5).unwrap();
    assert!(verify_consistency(5, 5, &root, &root, &[]));
    assert!(!verify_consistency(5, 5, &root, &log.root(6).unwrap(), &[]));
    assert!(!verify_consistency(5, 5, &root, &root, &[root]));
    // The empty tree's root is the hash of nothing, and nothing else.
    assert!(!verify_consistency(0, 5, &root, &log.root(5).unwrap(), &[]));
    assert!(!verify_consistency(
        0,
        5,
        &log.root(0).unwrap(),
        &root,
        &[root]
    ));
}

fn entry(name: &str, version: &str, seed: &str) -> LogEntry {
    let files: Vec<(&str, &[u8])> = vec![("package.jadpo", seed.as_bytes())];
    LogEntry {
        package: ScopedName::parse(name).unwrap(),
        version: Version::parse(version).unwrap(),
        digest: Digest::of_files(&files),
    }
}

#[test]
fn an_entry_binds_its_name_version_and_digest() {
    let one = entry("@acme/postcodes", "2.1.0", "a");
    assert_eq!(
        one.leaf(),
        format!("@acme/postcodes 2.1.0 {}", one.digest).into_bytes()
    );
    let mut log = Log::new();
    assert_eq!(log.append(&one), 0);
    assert_eq!(log.append(&entry("@acme/postcodes", "2.1.1", "b")), 1);
    assert_eq!(log.index_of(&one.digest), Some(0));
    assert_eq!(log.entry(0), Some(&one));
    assert_eq!(log.entry(2), None);
}

#[test]
fn a_client_checks_a_release_is_in_the_head_it_holds() {
    let mut log = Log::new();
    let entries: Vec<LogEntry> = (0..7)
        .map(|i| entry("@acme/postcodes", &format!("1.0.{i}"), &i.to_string()))
        .collect();
    for e in &entries {
        log.append(e);
    }
    let head = log.head();
    assert_eq!(head.size, 7);
    for (index, e) in entries.iter().enumerate() {
        let proof = InclusionProof {
            entry: e.clone(),
            index: index as u64,
            size: head.size,
            hashes: log.inclusion_proof(index as u64, head.size).unwrap(),
        };
        assert_eq!(check_included(&head, e, &proof), Ok(()));

        // An answer naming another release, or proving against another
        // head, or with a changed hash, is refused.
        let other = &entries[(index + 1) % entries.len()];
        assert_eq!(
            check_included(&head, other, &proof),
            Err(LogError::NotTheRelease)
        );
        let mut older = proof.clone();
        older.size = 6;
        assert_eq!(check_included(&head, e, &older), Err(LogError::NotTheHead));
        let mut moved = proof.clone();
        moved.index = (moved.index + 1) % head.size;
        assert_eq!(check_included(&head, e, &moved), Err(LogError::NotInLog));
        if let Some(first) = proof.hashes.first() {
            let mut raw = *first.as_bytes();
            raw[0] ^= 0x80;
            let mut changed = proof.clone();
            changed.hashes[0] = Hash::from_bytes(raw);
            assert_eq!(check_included(&head, e, &changed), Err(LogError::NotInLog));
        }
    }
}

#[test]
fn a_client_refuses_a_head_that_does_not_extend_the_last_it_saw() {
    let mut log = Log::new();
    for i in 0..5 {
        log.append(&entry(
            "@acme/postcodes",
            &format!("1.0.{i}"),
            &i.to_string(),
        ));
    }
    let seen = log.head();
    for i in 5..9 {
        log.append(&entry(
            "@acme/postcodes",
            &format!("1.0.{i}"),
            &i.to_string(),
        ));
    }
    let now = log.head();
    let proof = ConsistencyProof {
        from: seen.size,
        to: now.size,
        hashes: log.consistency_proof(seen.size, now.size).unwrap(),
    };
    assert_eq!(check_extends(&seen, &now, &proof), Ok(()));
    assert_eq!(
        check_extends(
            &now,
            &now,
            &ConsistencyProof {
                from: 9,
                to: 9,
                hashes: vec![]
            }
        ),
        Ok(())
    );

    // A registry that rewrote its history: the same first four releases,
    // then a different fifth, and the same four after.
    let mut forked = Log::new();
    for i in 0..9 {
        let seed = if i == 4 {
            "swapped".to_string()
        } else {
            i.to_string()
        };
        forked.append(&entry("@acme/postcodes", &format!("1.0.{i}"), &seed));
    }
    let fork = forked.head();
    let fork_proof = ConsistencyProof {
        from: seen.size,
        to: fork.size,
        hashes: forked.consistency_proof(seen.size, fork.size).unwrap(),
    };
    assert_eq!(
        check_extends(&seen, &fork, &fork_proof),
        Err(LogError::DoesNotExtend)
    );

    // A head that shrank, or a proof for other sizes.
    assert_eq!(check_extends(&now, &seen, &proof), Err(LogError::Shrank));
    let other = ConsistencyProof {
        from: 4,
        to: now.size,
        hashes: log.consistency_proof(4, now.size).unwrap(),
    };
    assert_eq!(
        check_extends(&seen, &now, &other),
        Err(LogError::NotTheHead)
    );
    let same_size = LogHead {
        size: seen.size,
        root: leaf_hash(b"other"),
    };
    assert_eq!(
        check_extends(
            &seen,
            &same_size,
            &ConsistencyProof {
                from: 5,
                to: 5,
                hashes: vec![]
            }
        ),
        Err(LogError::DoesNotExtend)
    );
}

#[test]
fn hashes_are_lower_case_hex_in_json() {
    let hash = leaf_hash(b"");
    let json = serde_json::to_string(&hash).unwrap();
    assert_eq!(
        json,
        "\"6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d\""
    );
    assert_eq!(serde_json::from_str::<Hash>(&json).unwrap(), hash);
    assert!(serde_json::from_str::<Hash>(&json.to_uppercase()).is_err());
    assert!(serde_json::from_str::<Hash>("\"6e34\"").is_err());
}
