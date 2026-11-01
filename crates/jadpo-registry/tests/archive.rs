//! A release's archive (area 11, `GET archives/{digest}`): a tar archive
//! whose files, unpacked, have the release's digest as the lock computes
//! it. It holds regular files only, and unpacking refuses anything else.

use jadpo_registry::archive::{ArchiveError, pack, unpack};
use jadpo_registry::names::Digest;

fn files() -> Vec<(String, Vec<u8>)> {
    vec![
        (
            "package.jadpo".into(),
            b"package Postcodes.v2 {\n}\n".to_vec(),
        ),
        ("CHANGELOG.md".into(), b"# Changelog\n".to_vec()),
        ("shared/types/postcode.jadpo".into(), vec![b'x'; 1500]),
        ("empty.md".into(), Vec::new()),
    ]
}

#[test]
fn an_archive_unpacks_to_its_files_and_digest() {
    let files = files();
    let bytes = pack(&files).expect("packs");
    assert_eq!(bytes.len() % 512, 0);
    assert_eq!(pack(&files).unwrap(), bytes, "packing is deterministic");
    let mut unpacked = unpack(&bytes).expect("unpacks");
    let mut sorted = files.clone();
    sorted.sort();
    unpacked.sort();
    assert_eq!(unpacked, sorted);
    let digest = |list: &[(String, Vec<u8>)]| {
        let view: Vec<(&str, &[u8])> = list
            .iter()
            .map(|(p, b)| (p.as_str(), b.as_slice()))
            .collect();
        Digest::of_files(&view)
    };
    assert_eq!(digest(&unpacked), digest(&files));
}

#[test]
fn a_long_path_is_split_across_the_prefix() {
    let long = format!("{}/{}.jadpo", "a".repeat(120), "b".repeat(90));
    let files = vec![(long.clone(), b"x".to_vec())];
    let bytes = pack(&files).unwrap();
    assert_eq!(unpack(&bytes).unwrap(), files);
    let too_long = vec![(
        format!("{}/{}", "a".repeat(200), "b".repeat(120)),
        Vec::new(),
    )];
    assert!(matches!(pack(&too_long), Err(ArchiveError::BadPath(_))));
}

#[test]
fn packing_refuses_paths_that_could_escape() {
    for bad in [
        "/etc/passwd",
        "../x",
        "a/../b",
        "a//b",
        "a/./b",
        "a\\b",
        "",
        "a/",
        ".hidden",
        "a/.git/config",
    ] {
        let files = vec![(bad.to_string(), Vec::new())];
        assert!(
            matches!(pack(&files), Err(ArchiveError::BadPath(_))),
            "{bad:?}"
        );
    }
    let twice = vec![("a".to_string(), Vec::new()), ("a".to_string(), Vec::new())];
    assert!(matches!(pack(&twice), Err(ArchiveError::Duplicate(_))));
}

/// A header block for `name` with type flag `kind` and `size` bytes,
/// checksummed, as a hostile archive might carry.
fn header(name: &str, kind: u8, size: usize) -> Vec<u8> {
    let mut block = vec![0u8; 512];
    block[..name.len()].copy_from_slice(name.as_bytes());
    block[100..108].copy_from_slice(b"0000644\0");
    block[108..116].copy_from_slice(b"0000000\0");
    block[116..124].copy_from_slice(b"0000000\0");
    let size = format!("{size:011o}\0");
    block[124..136].copy_from_slice(size.as_bytes());
    block[136..148].copy_from_slice(b"00000000000\0");
    block[156] = kind;
    block[257..263].copy_from_slice(b"ustar\0");
    block[263..265].copy_from_slice(b"00");
    block[148..156].copy_from_slice(b"        ");
    let sum: u32 = block.iter().map(|&b| u32::from(b)).sum();
    block[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    block
}

fn archive(blocks: &[Vec<u8>]) -> Vec<u8> {
    let mut out: Vec<u8> = blocks.concat();
    out.extend_from_slice(&[0u8; 1024]);
    out
}

#[test]
fn unpacking_reads_a_well_formed_hand_made_archive() {
    let mut body = b"hello".to_vec();
    body.resize(512, 0);
    let bytes = archive(&[header("a.jadpo", b'0', 5), body]);
    assert_eq!(
        unpack(&bytes).unwrap(),
        vec![("a.jadpo".to_string(), b"hello".to_vec())]
    );
}

#[test]
fn unpacking_refuses_anything_but_regular_files_inside_the_package() {
    let empty = |name: &str, kind: u8| archive(&[header(name, kind, 0)]);
    for (name, kind) in [
        ("link", b'2'),
        ("hard", b'1'),
        ("dir/", b'5'),
        ("fifo", b'6'),
        ("pax", b'x'),
        ("gnu", b'L'),
    ] {
        assert!(
            matches!(unpack(&empty(name, kind)), Err(ArchiveError::NotAFile(_))),
            "{name}"
        );
    }
    for name in ["../escape", "/abs", "a/../../b", ".hidden"] {
        assert!(
            matches!(unpack(&empty(name, b'0')), Err(ArchiveError::BadPath(_))),
            "{name}"
        );
    }
    let twice = archive(&[header("a", b'0', 0), header("a", b'0', 0)]);
    assert!(matches!(unpack(&twice), Err(ArchiveError::Duplicate(_))));
}

#[test]
fn unpacking_refuses_a_damaged_archive() {
    let good = pack(&files()).unwrap();
    let mut bad_sum = good.clone();
    bad_sum[0] ^= 1;
    assert!(matches!(unpack(&bad_sum), Err(ArchiveError::Damaged(_))));
    assert!(matches!(
        unpack(&good[..good.len() - 1024 - 512]),
        Err(ArchiveError::Damaged(_))
    ));
    assert!(matches!(
        unpack(&good[..700]),
        Err(ArchiveError::Damaged(_))
    ));
    assert!(matches!(unpack(&[]), Err(ArchiveError::Damaged(_))));
    // Bytes after the end are refused, so one digest has one archive.
    let mut trailing = good.clone();
    trailing.extend_from_slice(&[1u8; 512]);
    assert!(matches!(unpack(&trailing), Err(ArchiveError::Damaged(_))));
    // A size that runs past the end.
    let past = archive(&[header("a", b'0', 4096)]);
    assert!(matches!(unpack(&past), Err(ArchiveError::Damaged(_))));
}

#[test]
fn unpacking_stops_at_the_size_limit() {
    let big = vec![("big.md".to_string(), vec![0u8; 3 * 1024 * 1024])];
    let bytes = pack(&big).unwrap();
    assert!(matches!(
        jadpo_registry::archive::unpack_within(&bytes, 1024 * 1024),
        Err(ArchiveError::TooLarge)
    ));
    assert!(jadpo_registry::archive::unpack_within(&bytes, 4 * 1024 * 1024).is_ok());
}
