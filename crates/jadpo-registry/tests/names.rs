//! A registry's names and digests: the scoped name, the version and the
//! release digest, each read in exactly one spelling.

use jadpo_registry::names::{Digest, ScopedName, Version};

#[test]
fn a_scoped_name_is_its_publisher_and_its_name() {
    let name = ScopedName::parse("@acme/postcodes").expect("reads");
    assert_eq!(name.publisher(), "acme");
    assert_eq!(name.name(), "postcodes");
    assert_eq!(name.publisher_segment(), "@acme");
    assert_eq!(name.to_string(), "@acme/postcodes");
    assert!(ScopedName::parse("@jadpo/stripe-billing").is_ok());
    assert!(ScopedName::parse("@a1/b2").is_ok());
}

#[test]
fn a_scoped_name_has_one_spelling() {
    for bad in [
        "acme/postcodes",
        "@Acme/postcodes",
        "@acme/Postcodes",
        "@acme/",
        "@/postcodes",
        "@acme",
        "@acme/post_codes",
        "@acme/-postcodes",
        "@acme/postcodes-",
        "@ac--me/postcodes",
        "@1acme/postcodes",
        "@acme/postcodes/extra",
        "@acme/post codes",
        " @acme/postcodes",
        "@acme/postcodés",
        "@aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/x",
    ] {
        assert!(ScopedName::parse(bad).is_err(), "{bad:?} was read");
    }
    // Thirty-nine characters is the longest part.
    let longest = format!("@{}/x", "a".repeat(39));
    assert!(ScopedName::parse(&longest).is_ok());
}

#[test]
fn a_version_is_semantic_with_no_leading_zeros() {
    for good in ["2.1.0", "0.0.1", "10.20.30", "2.0.0-beta.1", "1.0.0-rc-2"] {
        assert!(Version::parse(good).is_ok(), "{good:?} was refused");
    }
    for bad in [
        "01.0.0",
        "1.0",
        "v1.0.0",
        "1.0.0-",
        "1.0.0+build",
        "1.0.0-é",
        "",
    ] {
        assert!(Version::parse(bad).is_err(), "{bad:?} was read");
    }
}

#[test]
fn a_digest_is_the_locks_sha256_spelling() {
    let hex = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    assert!(Digest::parse(&format!("sha256:{hex}")).is_ok());
    for bad in [
        hex.to_string(),
        format!("sha256:{}", hex.to_uppercase()),
        format!("sha256:{}", &hex[1..]),
        format!("sha512:{hex}"),
        format!("sha256:{hex}0"),
    ] {
        assert!(Digest::parse(&bad).is_err(), "{bad:?} was read");
    }
}

#[test]
fn a_digest_of_files_is_the_one_the_lock_records() {
    let files: Vec<(&str, &[u8])> = vec![
        ("package.jadpo", b"package Postcodes.v2\n"),
        ("a.jadpo", b"x"),
    ];
    assert_eq!(
        Digest::of_files(&files).as_str(),
        jadpo_db::package_digest(&files)
    );
}

#[test]
fn names_are_json_text_and_refused_when_misspelt() {
    let name = ScopedName::parse("@acme/postcodes").unwrap();
    assert_eq!(serde_json::to_string(&name).unwrap(), "\"@acme/postcodes\"");
    let back: ScopedName = serde_json::from_str("\"@acme/postcodes\"").unwrap();
    assert_eq!(back, name);
    assert!(serde_json::from_str::<ScopedName>("\"acme/postcodes\"").is_err());
    assert!(serde_json::from_str::<Version>("\"1.0\"").is_err());
    assert!(serde_json::from_str::<Digest>("\"sha256:00\"").is_err());
}
