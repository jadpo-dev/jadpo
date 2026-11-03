//! Which of a commit's files a release may hold (area 11, "Releasing").

use jadpo_verifier::files::{Entry, Judged, SIZE_LIMIT, judge, judge_tree};

fn entry(path: &str, size: u64) -> Entry {
    Entry {
        path: path.to_string(),
        mode: "100644".to_string(),
        object: "a".repeat(40),
        size: Some(size),
    }
}

#[test]
fn jadpo_source_and_a_packages_text_files_are_the_release() {
    for path in [
        "package.jadpo",
        "provider.jadpo",
        "surface.jadpo_record",
        "types/uk_postcode.jadpo",
        "libraries/uk.test.jadpo",
        "translations/en_gb.jadpo",
        "GUIDE.md",
        "CHANGELOG.md",
        "recordings/payment_intents.create.json",
        "recipes/track_parcels/domains/shipping/track.jadpo",
        "sample/app.jadpo",
        "sample/jadpo.lock",
        "sample/access.jadpo_record",
    ] {
        assert_eq!(judge(path), Judged::Release, "{path}");
    }
}

#[test]
fn a_repositorys_own_files_are_left_out_of_the_release() {
    for path in [
        ".gitignore",
        ".github/workflows/ci.yml",
        "libraries/.DS_Store",
        "README.md",
        "LICENSE",
        "LICENCE.txt",
        "COPYING",
    ] {
        assert_eq!(judge(path), Judged::LeftOut, "{path}");
    }
}

#[test]
fn anything_else_is_refused() {
    for path in [
        "payload.bin",
        "run.sh",
        "libraries/uk.rs",
        "notes/GUIDE.md",
        "types/README.md",
        "data.json",
        "recordings/fetch.sh",
        "build/app.jadpo",
        "jadpo.lock",
        "app.jadpo",
        "a\\b.jadpo",
        "bad\nname.jadpo",
    ] {
        assert!(matches!(judge(path), Judged::Refused), "{path}");
    }
}

#[test]
fn a_link_or_a_submodule_is_refused_whatever_its_name() {
    let mut link = entry("types/uk.jadpo", 10);
    link.mode = "120000".to_string();
    let mut module = entry("vendored.jadpo", 0);
    module.mode = "160000".to_string();
    module.size = None;
    let judged = judge_tree(vec![link, module, entry("package.jadpo", 10)]);
    assert_eq!(judged.release.len(), 1);
    assert_eq!(judged.refused.len(), 2);
    assert!(judged.refused[0].1.contains("link"), "{:?}", judged.refused);
    assert!(
        judged.refused[1].1.contains("submodule"),
        "{:?}",
        judged.refused
    );
}

#[test]
fn a_link_among_what_is_left_out_changes_nothing() {
    let mut link = entry(".github/link", 10);
    link.mode = "120000".to_string();
    let judged = judge_tree(vec![link, entry("package.jadpo", 10)]);
    assert!(judged.refused.is_empty());
    assert_eq!(judged.left_out, [".github/link"]);
}

#[test]
fn the_size_limit_counts_only_the_release() {
    let judged = judge_tree(vec![
        entry("package.jadpo", SIZE_LIMIT / 2),
        entry(".github/large", SIZE_LIMIT * 4),
    ]);
    assert!(judged.size < SIZE_LIMIT);
    assert!(!judged.too_large());
    let judged = judge_tree(vec![
        entry("package.jadpo", SIZE_LIMIT),
        entry("types/uk.jadpo", 1),
    ]);
    assert!(judged.too_large());
}

#[test]
fn the_size_is_the_archives_to_the_byte() {
    // Headers count: ten thousand small files are far larger as an archive.
    let files: Vec<(String, Vec<u8>)> = vec![
        ("package.jadpo".to_string(), vec![b'x'; 1]),
        ("types/a.jadpo".to_string(), vec![b'x'; 512]),
        ("types/b.jadpo".to_string(), vec![b'x'; 513]),
        ("types/c.jadpo".to_string(), Vec::new()),
    ];
    let entries = files
        .iter()
        .map(|(path, bytes)| entry(path, bytes.len() as u64))
        .collect();
    let packed = jadpo_registry::archive::pack(&files).expect("it packs");
    assert_eq!(judge_tree(entries).size, packed.len() as u64);
}
