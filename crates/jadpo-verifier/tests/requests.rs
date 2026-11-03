//! What a request may name (area 11, "The verifier").

use std::net::IpAddr;

use jadpo_verifier::request::{
    Repository, is_commit, is_digest, is_public, is_tag, public_address,
};

#[test]
fn a_repository_is_reached_over_https_at_a_host_name() {
    let repository = Repository::parse("https://github.com/acme/postcodes.git").expect("fine");
    assert_eq!(repository.host(), "github.com");
    assert_eq!(repository.url(), "https://github.com/acme/postcodes.git");
    for refused in [
        "http://github.com/acme/postcodes",
        "ssh://git@github.com/acme/postcodes",
        "git@github.com:acme/postcodes",
        "file:///etc",
        "ext::sh -c id",
        "-upload-pack=id",
        "https://user:secret@github.com/acme/postcodes",
        "https://github.com:8443/acme/postcodes",
        "https://github.com/acme/postcodes?x=1",
        "https://github.com/acme/postcodes#x",
        "https://github.com/acme/%2e%2e/postcodes",
        "https://github.com/acme/../postcodes",
        "https://github.com/",
        "https://github.com",
        "https://127.0.0.1/acme/postcodes",
        "https://[::1]/acme/postcodes",
        "https://2130706433/acme/postcodes",
        "https://localhost/acme/postcodes",
        "https://verifier.internal/acme/postcodes",
        "https://printer.local/acme/postcodes",
        "https://github.com/acme/post codes",
        "https://github.com/acme/postcodes\n",
    ] {
        assert!(Repository::parse(refused).is_err(), "{refused}");
    }
}

#[test]
fn only_public_addresses_are_public() {
    for address in [
        "127.0.0.1",
        "10.0.0.1",
        "172.16.0.1",
        "192.168.1.1",
        "169.254.169.254",
        "100.64.0.1",
        "0.0.0.0",
        "192.0.2.1",
        "198.18.0.1",
        "224.0.0.1",
        "255.255.255.255",
        "::",
        "::1",
        "fe80::1",
        "fc00::1",
        // Fly.io's private network.
        "fdaa::1",
        "ff02::1",
        "2001:db8::1",
        "::ffff:127.0.0.1",
        "::ffff:169.254.169.254",
        "64:ff9b::7f00:1",
        "2002:7f00:1::",
    ] {
        let address: IpAddr = address.parse().expect("an address");
        assert!(!is_public(address), "{address}");
    }
    for address in ["140.82.121.3", "8.8.8.8", "2606:50c0:8000::153"] {
        let address: IpAddr = address.parse().expect("an address");
        assert!(is_public(address), "{address}");
    }
}

#[test]
fn a_host_with_any_private_address_is_refused() {
    let public: IpAddr = "140.82.121.3".parse().expect("an address");
    let metadata: IpAddr = "169.254.169.254".parse().expect("an address");
    assert_eq!(public_address(&[public]), Some(public));
    assert_eq!(public_address(&[public, metadata]), None);
    assert_eq!(public_address(&[metadata]), None);
    assert_eq!(public_address(&[]), None);
}

#[test]
fn a_commit_a_tag_and_a_digest_have_their_own_forms() {
    assert!(is_commit(&"a1".repeat(20)));
    assert!(is_commit(&"a1".repeat(32)));
    assert!(!is_commit("main"));
    assert!(!is_commit(&"A1".repeat(20)));
    assert!(!is_commit(&format!("-{}", "a".repeat(39))));
    assert!(is_tag("v2.1.0"));
    assert!(is_tag("postcodes.v2-2.1.0"));
    for tag in [
        "-v1",
        "v1..2",
        "v1 2",
        "v1^{}",
        "refs/heads/main",
        "",
        "v1\n",
    ] {
        assert!(!is_tag(tag), "{tag}");
    }
    assert!(is_digest(&format!("sha256:{}", "a".repeat(64))));
    assert!(!is_digest(&format!("sha256:{}/../x", "a".repeat(58))));
    assert!(!is_digest("sha256:abc"));
}
