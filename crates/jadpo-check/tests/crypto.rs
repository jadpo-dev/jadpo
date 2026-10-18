//! Area 19, "Hashing and cryptography" (chunk 5.5q2, question 9): the
//! eight `crypto` operations that need no keys held as data, the key
//! settings they take as `key: config.<name>`, a place the compiler owns,
//! and the security types `PasswordHash`, `PasswordCheck` and `PublicKey`.
//! Reading a key anywhere but a `key:` is `configuration.secret_flow`;
//! code never makes a key; `encrypt` and `hash_password` read the
//! generator, so only actions and tests call them.

use crate::support;

use jadpo_db::Layout;
use support::{problems, rules};

const TYPES: &str = "type Note = Text {\n    max_length: 2000\n}\n\ntype RecordId = Text {}\n\ntype Sealed = Bytes {}\n\ntype Opened = Bytes {}\n\ntype Mac = Bytes {}\n\ntype Genuine = Bool {}\n\ntype PasswordText = Text {\n    min_length: 12\n}\n\ntype Shown = Text {}\n\ntype NoteUnreadable = Failure {\n    kind: FailureKind.invalid_value\n}\n\nconfig {\n    notes_key: EncryptionKey\n    receipt_key: PrivateKey\n    webhook_secret: SigningKey\n}\n\n";

fn check(source: &str) -> Vec<String> {
    rules(
        Layout::Loose,
        &[("main.jadpo", &format!("{TYPES}{source}"))],
    )
}

fn shown(source: &str) -> Vec<String> {
    problems(
        Layout::Loose,
        &[("main.jadpo", &format!("{TYPES}{source}"))],
    )
}

const NONE: &[&str] = &[];

const EVERY_OPERATION: &str = "action seal(note: Note, record: RecordId) -> Sealed {\n    return Sealed(crypto.encrypt(note, key: config.notes_key, context: record))\n}\n\nfunction open(sealed: Sealed, record: RecordId) fails NoteUnreadable -> Opened {\n    var bytes: Bytes = attempt crypto.decrypt(sealed, key: config.notes_key, context: record) {\n        unreadable: NoteUnreadable\n    }\n    return Opened(bytes)\n}\n\naction register(password: PasswordText) -> PasswordHash {\n    return crypto.hash_password(password)\n}\n\nfunction signs_in(password: PasswordText, stored: PasswordHash) -> PasswordCheck {\n    return crypto.verify_password(password, stored)\n}\n\nfunction mac(body: Note) -> Mac {\n    return Mac(crypto.hmac(body, key: config.webhook_secret))\n}\n\nfunction genuine(body: Note, given: Mac) -> Genuine {\n    return Genuine(crypto.verify_hmac(body, given, key: config.webhook_secret))\n}\n\nfunction signed(body: Note) -> Mac {\n    return Mac(crypto.sign(body, key: config.receipt_key))\n}\n\nfunction verified(body: Note, signature: Mac, signer: PublicKey) -> Genuine {\n    return Genuine(crypto.verify_signature(body, signature, signer))\n}\n";

#[test]
fn every_operation_checks_with_its_key_setting() {
    assert_eq!(check(EVERY_OPERATION), NONE);
}

#[test]
fn a_test_seals_and_hashes_too() {
    assert_eq!(
        rules(
            Layout::Loose,
            &[
                ("main.jadpo", &format!("{TYPES}{EVERY_OPERATION}")),
                (
                    "main.test.jadpo",
                    "test \"a note opens for its record\" {\n    var sealed: Sealed = seal(\"seen on Monday\", \"note-1\")\n    var opened: Opened = attempt open(sealed, \"note-1\")\n    assert opened == encoding.to_utf8(\"seen on Monday\")\n    assert open(sealed, \"note-2\") fails NoteUnreadable\n    var hashed: PasswordHash = crypto.hash_password(\"correct horse battery staple\")\n    assert signs_in(\"correct horse battery staple\", hashed) == PasswordCheck.correct\n}\n"
                ),
            ]
        ),
        NONE
    );
}

#[test]
fn a_pure_body_cant_seal_or_hash() {
    // A nonce and a salt are read from the generator, as `random` is.
    for call in [
        "Sealed(crypto.encrypt(note, key: config.notes_key, context: \"row\"))",
        "Sealed(encoding.to_utf8(crypto.hash_password(note)))",
    ] {
        assert_eq!(
            check(&format!(
                "function f(note: Note) -> Sealed {{\n    return {call}\n}}\n"
            )),
            ["type.random_context"],
            "{call}"
        );
    }
}

#[test]
fn decrypting_can_fail_unreadable_only_under_attempt() {
    assert_eq!(
        check(
            "function f(sealed: Sealed, record: RecordId) -> Opened {\n    return Opened(crypto.decrypt(sealed, key: config.notes_key, context: record))\n}\n"
        ),
        ["failure.attempt_required"]
    );
    // Not a construction's rule: `invalid` doesn't cover it.
    assert_eq!(
        check(
            "function f(sealed: Sealed, record: RecordId) fails NoteUnreadable -> Opened {\n    var bytes: Bytes = attempt crypto.decrypt(sealed, key: config.notes_key, context: record) {\n        malformed: NoteUnreadable\n    }\n    return Opened(bytes)\n}\n"
        )
        .first()
        .map(String::as_str),
        Some("failure.unknown_mapping")
    );
}

#[test]
fn reading_a_key_anywhere_but_key_is_secret_flow() {
    for body in [
        // Held in a variable, even one never used again.
        "    var copied: EncryptionKey = config.notes_key\n    return Mac(crypto.hmac(body, key: config.webhook_secret))",
        // Given by position rather than as `key:`.
        "    return Mac(crypto.hmac(body, config.webhook_secret))",
        // Hashed, signed or used as the value.
        "    return Mac(crypto.hmac(config.webhook_secret, key: config.webhook_secret))",
        "    return Mac(hash.sha256(config.webhook_secret))",
        // Written out as text, which a key has none of either.
        "    return Mac(encoding.to_utf8(text.from(config.receipt_key)))",
    ] {
        let found = check(&format!("function f(body: Note) -> Mac {{\n{body}\n}}\n"));
        assert!(
            !found.is_empty()
                && found.iter().all(|rule| rule == "configuration.secret_flow"
                    || rule == "semantic.unused_variable"
                    || rule == "type.mismatch"),
            "{body}: {found:?}"
        );
        assert!(
            found.iter().any(|rule| rule == "configuration.secret_flow"),
            "{body}"
        );
    }
    // As a call's context, compared, or passed to code of the app's own.
    assert_eq!(
        check(
            "action f(body: Note) -> Sealed {\n    return Sealed(crypto.encrypt(body, key: config.notes_key, context: config.webhook_secret))\n}\n"
        ),
        ["configuration.secret_flow"]
    );
    assert_eq!(
        check(
            "function f(body: Note) -> Genuine {\n    return Genuine(config.notes_key == config.notes_key)\n}\n"
        ),
        ["configuration.secret_flow", "configuration.secret_flow"]
    );
    assert_eq!(
        check(
            "function sealer(body: Note, key: EncryptionKey) -> Note {\n    return body\n}\n\nfunction f(body: Note) -> Note {\n    return sealer(body, config.notes_key)\n}\n"
        ),
        ["configuration.secret_flow"]
    );
}

#[test]
fn a_key_read_says_where_it_may_go() {
    let full = |source: &str| -> Vec<String> {
        let db = jadpo_db::Database::default();
        let project = support::project(
            &db,
            Layout::Loose,
            &[("main.jadpo", &format!("{TYPES}{source}"))],
        );
        jadpo_check::check(&db, project)
            .iter()
            .map(|d| format!("{} {:?} {:?}", d.message, d.reason, d.next))
            .collect()
    };
    let found =
        full("function f(body: Note) -> Mac {\n    return Mac(hash.sha256(config.notes_key))\n}\n");
    let [problem] = found.as_slice() else {
        panic!("{found:?}");
    };
    assert!(problem.contains("`config.notes_key` is a key"), "{problem}");
    assert!(problem.contains("`key: config.notes_key`"), "{problem}");
    // A signing key also signs sessions, so its read names every place.
    let found = full(
        "function f(body: Note) -> Mac {\n    return Mac(hash.sha256(config.webhook_secret))\n}\n",
    );
    assert!(
        found.iter().any(|p| p.contains("a `crypto` call's `key:`")),
        "{found:?}"
    );
}

#[test]
fn key_takes_a_setting_of_its_own_type() {
    // Another kind of key.
    assert_eq!(
        check(
            "action f(body: Note, record: RecordId) -> Sealed {\n    return Sealed(crypto.encrypt(body, key: config.webhook_secret, context: record))\n}\n"
        ),
        ["type.mismatch"]
    );
    assert_eq!(
        check(
            "function f(body: Note) -> Mac {\n    return Mac(crypto.hmac(body, key: config.notes_key))\n}\n"
        ),
        ["type.mismatch"]
    );
    assert_eq!(
        check(
            "function f(body: Note) -> Mac {\n    return Mac(crypto.sign(body, key: config.webhook_secret))\n}\n"
        ),
        ["type.mismatch"]
    );
    // A value that isn't a setting: a parameter, or text in the source.
    let found = shown(
        "function f(body: Note, given: SigningKey) -> Mac {\n    return Mac(crypto.hmac(body, key: given))\n}\n",
    );
    assert!(
        found.len() == 1
            && found[0].contains("`crypto.hmac`'s `key:` takes a setting, not `given`"),
        "{found:?}"
    );
    assert_eq!(
        check(
            "function f(body: Note) -> Mac {\n    return Mac(crypto.hmac(body, key: \"0123456789abcdef0123456789abcdef\"))\n}\n"
        ),
        ["type.mismatch"]
    );
    // A public key is not secret: a value, a literal or a setting.
    assert_eq!(
        check(
            "function f(body: Note, signature: Mac) -> Genuine {\n    return Genuine(crypto.verify_signature(body, signature, \"11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo\"))\n}\n"
        ),
        NONE
    );
}

#[test]
fn code_never_makes_a_key() {
    for (target, value) in [
        (
            "EncryptionKey",
            "\"gIGCg4SFhoeIiYqLjI2Oj5CRkpOUlZaXmJmam5ydnp8\"",
        ),
        ("PrivateKey", "raw"),
        ("SigningKey", "\"0123456789abcdef0123456789abcdef\""),
    ] {
        let found = shown(&format!(
            "type Raw = Text {{}}\n\nfunction f(raw: Raw) -> {target} {{\n    return {target}({value})\n}}\n"
        ));
        assert!(
            found.len() == 1
                && found[0].contains(&format!(
                    "type.constructor_input: `{target}` isn't built from a value"
                )),
            "{target}: {found:?}"
        );
    }
}

#[test]
fn a_key_setting_has_no_value_in_source() {
    for (ty, option) in [("EncryptionKey", "default"), ("PrivateKey", "test")] {
        assert_eq!(
            rules(
                Layout::Loose,
                &[(
                    "main.jadpo",
                    &format!(
                        "config {{\n    archive_key: {ty} {{\n        {option}: \"gIGCg4SFhoeIiYqLjI2Oj5CRkpOUlZaXmJmam5ydnp8\"\n    }}\n}}\n"
                    )
                )]
            ),
            ["configuration.secret_default"],
            "{ty}"
        );
    }
}

#[test]
fn an_encryption_key_rotates_and_a_private_key_doesnt() {
    // An encryption key reads `NAME_PREVIOUS` as a signing key does.
    assert_eq!(
        rules(
            Layout::Loose,
            &[(
                "main.jadpo",
                "config {\n    archive_key: EncryptionKey\n    old: EncryptionKey {\n        binding: \"ARCHIVE_KEY_PREVIOUS\"\n    }\n}\n"
            )]
        ),
        ["configuration.duplicate_binding"]
    );
    assert_eq!(
        rules(
            Layout::Loose,
            &[(
                "main.jadpo",
                "config {\n    receipt_key: PrivateKey\n    other: PrivateKey {\n        binding: \"RECEIPT_KEY_PREVIOUS\"\n    }\n}\n"
            )]
        ),
        NONE
    );
}

#[test]
fn keys_have_no_json_form() {
    assert_eq!(
        check(
            "type Raw = Text {}\n\nfunction f(raw: Raw) -> Shown {\n    return Shown(json.write(config.notes_key))\n}\n"
        ),
        ["configuration.secret_flow", "type.mismatch"]
    );
}

#[test]
fn the_security_types_are_standard_types() {
    // A public key and a hash are checked as they are written; a
    // password check is matched like any enum.
    assert_eq!(
        check("function f() -> PublicKey {\n    return PublicKey(\"not a key\")\n}\n"),
        ["type.invalid_literal"]
    );
    assert_eq!(
        check(
            "function f() -> PasswordHash {\n    return PasswordHash(\"$argon2i$v=19$m=19456,t=2,p=1$AAECAwQFBgcICQoLDA0ODw$gYJZtjEAJqjg26xdLmknq8/bB7MiWPrE9hsYuA+SkIU\")\n}\n"
        ),
        ["type.invalid_literal"]
    );
    assert_eq!(
        check(
            "type Outcome = Text {}\n\nfunction f(password: PasswordText, stored: PasswordHash) -> Outcome {\n    return match crypto.verify_password(password, stored) {\n        PasswordCheck.correct -> \"signed in\"\n        PasswordCheck.correct_outdated -> \"signed in, rehash\"\n        PasswordCheck.wrong -> \"refused\"\n    }\n}\n"
        ),
        NONE
    );
    assert_eq!(
        check(
            "type Account = Output {\n    password_hash: PasswordHash\n    signer: PublicKey\n    last_check: PasswordCheck\n}\n"
        ),
        NONE
    );
}
