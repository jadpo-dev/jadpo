//! Area 19, "The standard types" (chunk 5.5r3): the security records.
//! `Hash`, `Hmac` and `EncryptedValue` are built on `Bytes` and built
//! without an arm from the call that makes their bytes; `OneTimeCode` is a
//! secret value; `PasswordPolicy`'s fewest characters are never more than
//! its most; a `WebAuthnCredential` is read through by its parts.

use crate::support;

use jadpo_db::Layout;
use support::rules;

const TYPES: &str = "config {\n    notes_key: EncryptionKey\n    webhook_secret: SigningKey\n}\n\ntype Body = Text {\n    max_length: 100000\n}\n\ntype NoteId = Text {}\n\ntype Wire = Text {}\n\ntype Same = Bool {}\n\ntype KeyVerifier = Hash {}\n\ntype Raw = Bytes {}\n\ntype Refused = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";

fn check(source: &str) -> Vec<String> {
    rules(
        Layout::Loose,
        &[("main.jadpo", &format!("{TYPES}{source}"))],
    )
}

const NONE: &[&str] = &[];

#[test]
fn records_on_bytes_are_built_from_what_makes_them() {
    assert_eq!(
        check(
            "function verifier(key: ApiKey) -> KeyVerifier {\n    return KeyVerifier(hash.sha256(key))\n}\n\nfunction signature(body: Body) -> Hmac {\n    return Hmac(crypto.hmac(body, key: config.webhook_secret))\n}\n\naction sealed(body: Body, record: NoteId) -> EncryptedValue {\n    return EncryptedValue(crypto.encrypt(body, key: config.notes_key, context: record))\n}\n\nfunction opened(kept: EncryptedValue, record: NoteId) fails Refused -> Raw {\n    var opened_bytes: Bytes = attempt crypto.decrypt(kept, key: config.notes_key, context: record) {\n        unreadable: Refused\n    }\n    return Raw(opened_bytes)\n}\n\nfunction same(a: KeyVerifier, b: Hash) -> Same {\n    return Same(a == b)\n}\n\nfunction written(digest: Hash) -> Wire {\n    return Wire(text.from(digest))\n}\n\nfunction read(raw: Wire) fails Refused -> Hmac {\n    return attempt text.parse(raw, as: Hmac) {\n        malformed: Refused\n    }\n}\n"
        ),
        NONE
    );
}

#[test]
fn other_bytes_are_checked_as_they_are_built() {
    // Bytes from anywhere else may not be the type's: the construction
    // names its check, `format`.
    assert_eq!(
        check(
            "function digest(raw: Raw) fails Refused -> Hash {\n    return attempt Hash(raw) {\n        format: Refused\n    }\n}\n\nfunction tag(body: Body) fails Refused -> Hmac {\n    return attempt Hmac(hash.sha256(body)) {\n        format: Refused\n    }\n}\n"
        ),
        NONE
    );
    assert_ne!(
        check("function digest(raw: Raw) -> Hash {\n    return Hash(raw)\n}\n"),
        NONE,
        "unchecked bytes need an arm"
    );
    assert_ne!(
        check(
            "action sealed(body: Body) -> EncryptedValue {\n    return EncryptedValue(hash.sha256(body))\n}\n"
        ),
        NONE,
        "a digest isn't sealed"
    );
}

#[test]
fn a_one_time_code_is_checked_and_never_kept() {
    assert_eq!(
        check(
            "type CodeEntry = Input {\n    code: OneTimeCode\n}\n\nfunction matches_sent(form: CodeEntry, sent: OneTimeCode) -> Same {\n    return Same(form.code == sent)\n}\n"
        ),
        NONE
    );
    assert_eq!(
        check("type Sent = Output {\n    code: OneTimeCode\n}\n"),
        ["type.secret_exposed"]
    );
    assert_eq!(
        check("entity Challenge {\n    id: Uuid\n    code: OneTimeCode\n}\n"),
        ["type.secret_exposed"]
    );
    assert_eq!(
        check("function shown(code: OneTimeCode) -> Wire {\n    return Wire(text.from(code))\n}\n"),
        ["type.secret_exposed"]
    );
}

#[test]
fn a_password_policy_s_lengths_are_in_order() {
    let policy = |min: &str, max: &str| {
        check(&format!(
            "function policy() -> PasswordPolicy {{\n    return PasswordPolicy {{\n        min_length: {min}\n        max_length: {max}\n        check_breached: true\n    }}\n}}\n"
        ))
    };
    assert_eq!(policy("15", "64"), NONE);
    assert_eq!(policy("64", "64"), NONE, "the same length");
    assert_eq!(
        policy("100", "64"),
        ["type.invalid_literal"],
        "fewest above most"
    );
    assert_eq!(
        policy("6", "64"),
        ["type.invalid_literal"],
        "below NIST's 8"
    );
    assert_eq!(
        policy("8", "32"),
        ["type.invalid_literal"],
        "a most below 64"
    );
}

#[test]
fn a_passkey_is_read_through_by_its_parts() {
    assert_eq!(
        check(
            "type Count = Int {}\n\ntype Label = Text {}\n\nfunction count(passkey: WebAuthnCredential) -> Count {\n    return Count(passkey.sign_count)\n}\n\nfunction label(passkey: WebAuthnCredential) -> Label {\n    return Label(passkey.name)\n}\n\nfunction id(passkey: WebAuthnCredential) -> Raw {\n    return Raw(passkey.credential_id)\n}\n"
        ),
        NONE
    );
}
