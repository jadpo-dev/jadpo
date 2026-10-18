//! Area 19, "Security records" (chunk 5.5r5): a `Certificate` is read
//! from its PEM, by `text.parse` and `json.read`, and read through by its
//! parts, which code never sets.

use crate::support;

use jadpo_db::Layout;
use support::rules;

const TYPES: &str = "type Pem = Text {\n    max_length: 100000\n}\n\ntype Wire = Text {}\n\ntype Name = Text {}\n\ntype Serial = Text {}\n\ntype Same = Bool {}\n\ntype At = Instant {}\n\ntype Der = Bytes {}\n\ntype Pinned = Hash {}\n\ntype Unreadable = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype Partner = Object {\n    name: Name\n    certificate: Certificate\n}\n\n";

fn check(source: &str) -> Vec<String> {
    rules(
        Layout::Loose,
        &[("main.jadpo", &format!("{TYPES}{source}"))],
    )
}

const NONE: &[&str] = &[];

#[test]
fn a_certificate_is_read_and_read_through() {
    assert_eq!(
        check(
            "function read(raw: Pem) fails Unreadable -> Certificate {\n    return attempt text.parse(raw, as: Certificate) {\n        malformed: Unreadable\n    }\n}\n\nfunction received(raw: Wire) fails Unreadable -> Partner {\n    return attempt json.read(raw, as: Partner) {\n        malformed: Unreadable\n        mismatch: Unreadable\n    }\n}\n\nfunction subject(offered: Certificate) -> Name {\n    return Name(offered.subject)\n}\n\nfunction issuer(offered: Certificate) -> Name {\n    return Name(offered.issuer)\n}\n\nfunction serial(offered: Certificate) -> Serial {\n    return Serial(offered.serial_number)\n}\n\nfunction valid_at(offered: Certificate, at: At) -> Same {\n    return Same(offered.not_before <= at and at <= offered.not_after)\n}\n\nfunction pinned(offered: Certificate, kept: Pinned) -> Same {\n    return Same(offered.fingerprint == kept)\n}\n\nfunction same(a: Certificate, b: Certificate) -> Same {\n    return Same(a == b)\n}\n\nfunction written(offered: Certificate) -> Wire {\n    return Wire(text.from(offered))\n}\n\nfunction sent(partner: Partner) -> Wire {\n    return Wire(json.write(partner))\n}\n"
        ),
        NONE
    );
}

#[test]
fn code_never_sets_a_certificate_s_parts() {
    assert_eq!(
        check(
            "function forged(der: Der, fingerprint: Hash, at: At) -> Certificate {\n    return Certificate {\n        subject: \"CN=bank.example\"\n        issuer: \"CN=Trusted Root\"\n        serial_number: \"01\"\n        not_before: at\n        not_after: at\n        fingerprint: fingerprint\n        der: der\n    }\n}\n"
        ),
        ["type.constructor_input"]
    );
    // Nor are its parts compared in order, since a certificate has none.
    assert_eq!(
        check(
            "function earlier(a: Certificate, b: Certificate) -> Same {\n    return Same(a < b)\n}\n"
        )
        .len(),
        1
    );
}
