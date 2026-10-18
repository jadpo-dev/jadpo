//! Area 19, "The standard types" (chunk 5.5r3), run: a `Hash`, an `Hmac`
//! and an `EncryptedValue` are the bytes `hash.sha256`, `crypto.hmac` and
//! `crypto.encrypt` give, written as their Base64, sent and read back as
//! `Bytes` are, and compared in constant time; an `EncryptedValue` opens
//! with `crypto.decrypt` alone; a `OneTimeCode` is held as a secret.

use std::collections::BTreeMap;

use crate::support;

use jadpo_interp::{Outcome, Program, Value};
use jadpo_runtime::config::Mode;
use support::{main, text, value};

const SOURCE: &str = "type Note = Text {\n    max_length: 2000\n}\n\ntype RecordId = Text {}\n\ntype Wire = Text {}\n\ntype Same = Bool {}\n\ntype Opened = Bytes {}\n\ntype KeyVerifier = Hash {}\n\ntype Unreadable = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype Kept = Object {\n    verifier: KeyVerifier\n    note: EncryptedValue\n}\n\nconfig {\n    notes_key: EncryptionKey\n    webhook_secret: SigningKey\n}\n\nfunction verifier(key: ApiKey) -> KeyVerifier {\n    return KeyVerifier(hash.sha256(key))\n}\n\nfunction signature(body: Note) -> Hmac {\n    return Hmac(crypto.hmac(body, key: config.webhook_secret))\n}\n\naction seal(note: Note, record: RecordId) -> EncryptedValue {\n    return EncryptedValue(crypto.encrypt(note, key: config.notes_key, context: record))\n}\n\nfunction open(sealed: EncryptedValue, record: RecordId) fails Unreadable -> Opened {\n    var bytes: Bytes = attempt crypto.decrypt(sealed, key: config.notes_key, context: record) {\n        unreadable: Unreadable\n    }\n    return Opened(bytes)\n}\n\nfunction same(a: KeyVerifier, b: KeyVerifier) -> Same {\n    return Same(a == b)\n}\n\nfunction written(digest: KeyVerifier) -> Wire {\n    return Wire(text.from(digest))\n}\n\nfunction read(raw: Wire) fails Unreadable -> KeyVerifier {\n    var digest: KeyVerifier = attempt text.parse(raw, as: KeyVerifier) {\n        malformed: Unreadable\n    }\n    return digest\n}\n\nfunction sent(kept: Kept) -> Wire {\n    return Wire(json.write(kept))\n}\n\nfunction received(raw: Wire) fails Unreadable -> Kept {\n    var kept: Kept = attempt json.read(raw, as: Kept) {\n        malformed: Unreadable\n        mismatch: Unreadable\n    }\n    return kept\n}\n\nfunction code_matches(given: OneTimeCode, sent_code: OneTimeCode) -> Same {\n    return Same(given == sent_code)\n}\n";

/// The draft-irtf-cfrg-xchacha key, 80 81 ... 9f.
const NOTES_KEY: &str = "gIGCg4SFhoeIiYqLjI2Oj5CRkpOUlZaXmJmam5ydnp8";
const WEBHOOK_SECRET: &str = "test-signing-key-0123456789abcdef0123456789abcdef";
/// SHA-256 of `abc` (FIPS 180-4), as Base64.
const ABC: &str = "ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0=";

fn started() -> Program {
    let program = main(SOURCE);
    let variables: BTreeMap<String, String> =
        [("NOTES_KEY", NOTES_KEY), ("WEBHOOK_SECRET", WEBHOOK_SECRET)]
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
    let values = program
        .prepared()
        .config(&variables, Mode::Environment, &|_: &mut [u8]| false)
        .unwrap_or_else(|problems| panic!("doesn't start: {problems:?}"));
    program.with_config(&values)
}

fn digest(bytes: &[u8]) -> Value {
    Value::Bytes(std::rc::Rc::from(bytes))
}

#[test]
fn a_hash_is_the_digest_written_as_its_base64() {
    let program = started();
    let made = value(&program, "verifier", vec![Value::secret("ApiKey", "abc")]);
    assert_eq!(made, digest(&jadpo_value::hash::sha256(b"abc")));
    assert_eq!(value(&program, "written", vec![made.clone()]), text(ABC));
    assert_eq!(value(&program, "read", vec![text(ABC)]), made);
    // 31 bytes and hexadecimal aren't a `Hash`.
    for raw in [
        "ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFQ==",
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    ] {
        assert!(
            matches!(
                support::run(&program, "read", vec![text(raw)]),
                Outcome::Failed(_)
            ),
            "{raw}"
        );
    }
    let mut other = jadpo_value::hash::sha256(b"abc").to_vec();
    other[31] ^= 1;
    assert_eq!(
        value(&program, "same", vec![made.clone(), made.clone()]),
        Value::Bool(true)
    );
    assert_eq!(
        value(&program, "same", vec![made, digest(&other)]),
        Value::Bool(false)
    );
}

#[test]
fn an_encrypted_value_opens_with_its_key_alone() {
    let program = started();
    let sealed = value(
        &program,
        "seal",
        vec![text("Allergic to penicillin."), text("note-1")],
    );
    let Value::Bytes(bytes) = &sealed else {
        panic!("sealed bytes");
    };
    assert_eq!(
        &bytes[..2],
        &[1, 1],
        "the sealed format's version and algorithm"
    );
    assert_eq!(
        value(&program, "open", vec![sealed.clone(), text("note-1")]),
        digest(b"Allergic to penicillin.")
    );
    assert!(matches!(
        support::run(&program, "open", vec![sealed.clone(), text("note-2")]),
        Outcome::Failed(_)
    ));
    // Sent and read back with a digest beside it, each as its Base64.
    let kept = value(
        &program,
        "received",
        vec![text(&format!(
            "{{\"verifier\": \"{ABC}\", \"note\": \"{}\"}}",
            jadpo_value::encoding::to_base64(bytes)
        ))],
    );
    let Value::Text(json) = value(&program, "sent", vec![kept.clone()]) else {
        panic!("JSON");
    };
    assert!(json.contains(ABC), "{json}");
    assert_eq!(value(&program, "received", vec![text(&json)]), kept);
    // A digest isn't a sealed value.
    assert!(matches!(
        support::run(
            &program,
            "received",
            vec![text(&format!(
                "{{\"verifier\": \"{ABC}\", \"note\": \"{ABC}\"}}"
            ))]
        ),
        Outcome::Failed(_)
    ));
}

#[test]
fn an_hmac_is_the_signing_key_s_tag() {
    let program = started();
    let tag = value(&program, "signature", vec![text("{\"paid\": true}")]);
    let Value::Bytes(bytes) = &tag else {
        panic!("a tag");
    };
    assert_eq!(bytes.len(), 32, "HMAC-SHA-256's");
    assert_eq!(
        value(&program, "signature", vec![text("{\"paid\": true}")]),
        tag
    );
}

#[test]
fn a_one_time_code_is_held_secret() {
    let program = started();
    let code = Value::secret("OneTimeCode", "287082");
    assert_eq!(code.to_string(), "<redacted OneTimeCode>");
    assert_eq!(
        value(&program, "code_matches", vec![code.clone(), code.clone()]),
        Value::Bool(true)
    );
    assert_eq!(
        value(
            &program,
            "code_matches",
            vec![code, Value::secret("OneTimeCode", "287083")]
        ),
        Value::Bool(false)
    );
}
