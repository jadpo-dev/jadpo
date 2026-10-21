//! Area 19, "Hashing and cryptography" (chunk 5.5q2), run: a sealed value
//! opens only with its key and context, and fails `unreadable` when altered
//! or opened with another key; a rotated key still opens what the previous
//! one sealed; nonces never repeat; a password checks correct, wrong or
//! outdated; an HMAC is the signing key's text's and checks against the
//! previous key; a signature is Ed25519's. Keys reach an operation from
//! their settings alone, and a key setting that isn't 32 bytes of unpadded
//! Base64url stops the application starting.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::rc::Rc;

use crate::support;

use jadpo_interp::{Executor, Outcome, Program, Value};
use jadpo_runtime::config::{Mode, ProblemKind};
use jadpo_runtime::{Clock, Ctx, Env, MemoryHost};
use support::{failure, main, text, value};

const SOURCE: &str = "type Note = Text {\n    max_length: 2000\n}\n\ntype RecordId = Text {}\n\ntype Sealed = Bytes {}\n\ntype Opened = Bytes {}\n\ntype Mac = Bytes {}\n\ntype Genuine = Bool {}\n\ntype PasswordText = Password {}\n\ntype NoteUnreadable = Failure {\n    kind: FailureKind.invalid_value\n}\n\nconfig {\n    notes_key: EncryptionKey\n    receipt_key: PrivateKey\n    webhook_secret: SigningKey\n}\n\naction seal(note: Note, record: RecordId) -> Sealed {\n    return Sealed(crypto.encrypt(note, key: config.notes_key, context: record))\n}\n\nfunction open(sealed: Sealed, record: RecordId) fails NoteUnreadable -> Opened {\n    var bytes: Bytes = attempt crypto.decrypt(sealed, key: config.notes_key, context: record) {\n        unreadable: NoteUnreadable\n    }\n    return Opened(bytes)\n}\n\naction register(password: PasswordText) -> PasswordHash {\n    return crypto.hash_password(password)\n}\n\nfunction signs_in(password: PasswordText, stored: PasswordHash) -> PasswordCheck {\n    return crypto.verify_password(password, stored)\n}\n\nfunction mac(body: Note) -> Mac {\n    return Mac(crypto.hmac(body, key: config.webhook_secret))\n}\n\nfunction genuine(body: Note, given: Mac) -> Genuine {\n    return Genuine(crypto.verify_hmac(body, given, key: config.webhook_secret))\n}\n\nfunction signed(body: Note) -> Mac {\n    return Mac(crypto.sign(body, key: config.receipt_key))\n}\n\nfunction verified(body: Note, signature: Mac, signer: PublicKey) -> Genuine {\n    return Genuine(crypto.verify_signature(body, signature, signer))\n}\n\nfunction published() -> PublicKey {\n    return crypto.public_key(key: config.receipt_key)\n}\n";

/// The draft-irtf-cfrg-xchacha key, 80 81 ... 9f.
const NOTES_KEY: &str = "gIGCg4SFhoeIiYqLjI2Oj5CRkpOUlZaXmJmam5ydnp8";
/// RFC 8032's first test key's seed.
const OTHER_KEY: &str = "nWGxne_9WmC6hEr0kuwsxERJxWl7MmkZcDusAxyuf2A";
/// RFC 8032's second test key's seed, and its public key.
const RECEIPT_KEY: &str = "TM0Imyj_ltqdtsNG7BFOD1uKMZ81q6Yk2oz27U-4pvs";
const RECEIPT_PUBLIC: &str = "PUAXw-hDiVqStwqnTRt-vJyYLM8uxJaMwM1V8Sr0Zgw";
const WEBHOOK_SECRET: &str = "test-signing-key-0123456789abcdef0123456789abcdef";

fn variables(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
}

/// The program started with `pairs` as its environment.
fn started(pairs: &[(&str, &str)]) -> Program {
    let program = main(SOURCE);
    let values = program
        .prepared()
        .config(&variables(pairs), Mode::Environment, &|_: &mut [u8]| false)
        .unwrap_or_else(|problems| panic!("doesn't start: {problems:?}"));
    program.with_config(&values)
}

fn keyed(notes_key: &str) -> Program {
    started(&[
        ("NOTES_KEY", notes_key),
        ("RECEIPT_KEY", RECEIPT_KEY),
        ("WEBHOOK_SECRET", WEBHOOK_SECRET),
    ])
}

fn bytes(value: &Value) -> Vec<u8> {
    match value {
        Value::Bytes(bytes) => bytes.to_vec(),
        other => panic!("not bytes: {other:?}"),
    }
}

fn of_bytes(bytes: &[u8]) -> Value {
    Value::Bytes(Rc::from(bytes))
}

fn sealed(program: &Program, note: &str, record: &str) -> Vec<u8> {
    bytes(&value(program, "seal", vec![text(note), text(record)]))
}

fn opens(program: &Program, sealed: &[u8], record: &str) -> Option<Vec<u8>> {
    match support::run(program, "open", vec![of_bytes(sealed), text(record)]) {
        Outcome::Value(value) => Some(bytes(&value)),
        Outcome::Failed(failure) => {
            assert_eq!(failure.name, "NoteUnreadable");
            None
        }
        other => panic!("open: {other:?}"),
    }
}

#[test]
fn a_sealed_note_opens_for_its_own_record_only() {
    let program = keyed(NOTES_KEY);
    let note = sealed(&program, "seen on Monday", "note-1");
    // Version 1, XChaCha20-Poly1305, the key's identity, a 24-byte nonce,
    // the ciphertext and a 16-byte tag.
    assert_eq!(&note[..2], &[1, 1]);
    assert_eq!(note.len(), 6 + 24 + 14 + 16);
    assert_eq!(
        opens(&program, &note, "note-1").as_deref(),
        Some(&b"seen on Monday"[..])
    );
    assert_eq!(opens(&program, &note, "note-2"), None);
    let failed = failure(&program, "open", vec![of_bytes(&note), text("note-2")]);
    assert_eq!(failed.name, "NoteUnreadable");
}

#[test]
fn a_tampered_note_fails_to_open() {
    let program = keyed(NOTES_KEY);
    let note = sealed(&program, "seen on Monday", "note-1");
    // The header, the nonce, the ciphertext and the tag, each altered.
    for at in [0, 1, 3, 8, 30, 35, note.len() - 1] {
        let mut altered = note.clone();
        altered[at] ^= 0x40;
        assert_eq!(opens(&program, &altered, "note-1"), None, "byte {at}");
    }
    assert_eq!(opens(&program, &note[..note.len() - 1], "note-1"), None);
    assert_eq!(opens(&program, b"", "note-1"), None);
}

#[test]
fn a_wrongly_keyed_note_fails_to_open_until_rotated() {
    let before = keyed(OTHER_KEY);
    let after = keyed(NOTES_KEY);
    let note = sealed(&before, "seen on Monday", "note-1");
    assert_eq!(opens(&after, &note, "note-1"), None);
    // With the old key moved to `NOTES_KEY_PREVIOUS`, it opens again.
    let rotated = started(&[
        ("NOTES_KEY", NOTES_KEY),
        ("NOTES_KEY_PREVIOUS", OTHER_KEY),
        ("RECEIPT_KEY", RECEIPT_KEY),
        ("WEBHOOK_SECRET", WEBHOOK_SECRET),
    ]);
    assert_eq!(
        opens(&rotated, &note, "note-1").as_deref(),
        Some(&b"seen on Monday"[..])
    );
    // What the rotated app seals, only the new key opens.
    let fresh = sealed(&rotated, "seen on Tuesday", "note-1");
    assert_eq!(
        opens(&after, &fresh, "note-1").as_deref(),
        Some(&b"seen on Tuesday"[..])
    );
    assert_eq!(opens(&before, &fresh, "note-1"), None);
}

#[test]
fn nonces_never_repeat_across_many_seals_with_one_key() {
    let program = keyed(NOTES_KEY);
    let id = program.plan().find("seal").expect("seal");
    let host = MemoryHost::new();
    // One invocation's draws, as a test's run of many seals.
    let mut ctx = Ctx::of(
        &program,
        &host,
        Env {
            seed: Some(7),
            clock: Clock::TEST,
        },
    );
    let mut nonces = BTreeSet::new();
    for _ in 0..2000 {
        let outcome = jadpo_runtime::block_on(program.run(
            &mut ctx,
            id,
            vec![text("same note"), text("same record")],
        ))
        .expect("ran");
        let Outcome::Value(value) = outcome else {
            panic!("{outcome:?}");
        };
        nonces.insert(bytes(&value)[6..30].to_vec());
    }
    assert_eq!(nonces.len(), 2000);
}

/// A nonce comes from secret randomness: a recorded run keeps none on its
/// tape, and a replayed one draws a fresh nonce rather than the recorded
/// bytes, so a replay never seals with a nonce production used (chunk
/// 5.8a).
#[test]
fn a_nonce_is_never_recorded_or_replayed() {
    let program = keyed(NOTES_KEY);
    let id = program.plan().find("seal").expect("seal");
    let host = MemoryHost::new();
    let env = Env {
        seed: Some(7),
        clock: Clock::TEST,
    };
    let mut recording = Ctx::of(&program, &host, env);
    recording.record_run();
    let args = || vec![text("same note"), text("same record")];
    let outcome = jadpo_runtime::block_on(program.run(&mut recording, id, args())).expect("ran");
    assert!(matches!(outcome, Outcome::Value(_)), "{outcome:?}");
    assert_eq!(
        recording.recorded_run().map(|kept| kept.random.as_slice()),
        Some(&[][..]),
        "no nonce kept"
    );
    let tape = vec![0x5a; 24];
    let mut replaying = Ctx::of(&program, &host, env);
    replaying.replay_run(jadpo_runtime::capability::Recording {
        random: tape.clone(),
        calls: Vec::new(),
    });
    let outcome = jadpo_runtime::block_on(program.run(&mut replaying, id, args())).expect("ran");
    let Outcome::Value(value) = outcome else {
        panic!("{outcome:?}");
    };
    assert_ne!(bytes(&value)[6..30], tape[..], "a fresh nonce");
}

#[test]
fn a_password_checks_correct_wrong_or_outdated() {
    let program = keyed(NOTES_KEY);
    let hashed = value(
        &program,
        "register",
        vec![text("correct horse battery staple")],
    );
    let Value::Text(phc) = &hashed else {
        panic!("{hashed:?}");
    };
    assert!(phc.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"), "{phc}");
    let check = |password: &str, stored: Value| {
        let Value::Variant(variant) = value(&program, "signs_in", vec![text(password), stored])
        else {
            panic!("a PasswordCheck");
        };
        variant.name.to_string()
    };
    assert_eq!(
        check("correct horse battery staple", hashed.clone()),
        "correct"
    );
    assert_eq!(check("Correct horse battery staple", hashed), "wrong");
    let older = text(
        "$argon2id$v=19$m=4096,t=3,p=1$AAECAwQFBgcICQoLDA0ODw$VTsSHiMxkq48zEo8LUSymjr7JjaRSgsxGX2Z8HQE9Cw",
    );
    assert_eq!(
        check("correct horse battery staple", older),
        "correct_outdated"
    );
}

#[test]
fn an_hmac_is_the_signing_key_s_and_checks_in_both_keys() {
    let program = keyed(NOTES_KEY);
    let made = value(&program, "mac", vec![text("{\"paid\":true}")]);
    assert_eq!(bytes(&made).len(), 32);
    assert_eq!(
        value(
            &program,
            "genuine",
            vec![text("{\"paid\":true}"), made.clone()]
        ),
        Value::Bool(true)
    );
    assert_eq!(
        value(
            &program,
            "genuine",
            vec![text("{\"paid\":false}"), made.clone()]
        ),
        Value::Bool(false)
    );
    // After a rotation, the old key's MAC still checks.
    let rotated = started(&[
        ("NOTES_KEY", NOTES_KEY),
        ("RECEIPT_KEY", RECEIPT_KEY),
        (
            "WEBHOOK_SECRET",
            "test-signing-key-fedcba9876543210fedcba9876543210",
        ),
        ("WEBHOOK_SECRET_PREVIOUS", WEBHOOK_SECRET),
    ]);
    assert_eq!(
        value(
            &rotated,
            "genuine",
            vec![text("{\"paid\":true}"), made.clone()]
        ),
        Value::Bool(true)
    );
    assert_ne!(value(&rotated, "mac", vec![text("{\"paid\":true}")]), made);
}

#[test]
fn a_signature_is_rfc_8032_s() {
    let program = keyed(NOTES_KEY);
    // RFC 8032's second test: the message 0x72.
    let signature = value(&program, "signed", vec![text("r")]);
    assert_eq!(
        jadpo_value::encoding::to_hex(&bytes(&signature)),
        "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00"
    );
    assert_eq!(
        value(
            &program,
            "verified",
            vec![text("r"), signature.clone(), text(RECEIPT_PUBLIC)]
        ),
        Value::Bool(true)
    );
    assert_eq!(
        value(
            &program,
            "verified",
            vec![text("s"), signature, text(RECEIPT_PUBLIC)]
        ),
        Value::Bool(false)
    );
}

#[test]
fn what_the_app_signs_checks_with_the_public_key_it_publishes() {
    // RFC 8032's second test key gives its public key.
    let program = keyed(NOTES_KEY);
    let public = value(&program, "published", vec![]);
    assert_eq!(public, text(RECEIPT_PUBLIC));
    let signature = value(&program, "signed", vec![text("a receipt")]);
    assert_eq!(
        value(
            &program,
            "verified",
            vec![text("a receipt"), signature.clone(), public.clone()]
        ),
        Value::Bool(true)
    );
    assert_eq!(
        value(
            &program,
            "verified",
            vec![text("another"), signature, public]
        ),
        Value::Bool(false)
    );
    // A test run's own throwaway key round-trips too.
    let counter = std::cell::Cell::new(0u8);
    let values = main(SOURCE)
        .prepared()
        .config(&|_: &str| None, Mode::Test, &|bytes: &mut [u8]| {
            for byte in bytes {
                counter.set(counter.get().wrapping_add(13));
                *byte = counter.get();
            }
            true
        })
        .expect("a test run starts");
    let run = main(SOURCE).with_config(&values);
    let public = value(&run, "published", vec![]);
    let signature = value(&run, "signed", vec![text("a receipt")]);
    assert_eq!(
        value(&run, "verified", vec![text("a receipt"), signature, public]),
        Value::Bool(true)
    );
}

#[test]
fn a_key_that_isn_t_32_bytes_of_unpadded_base64url_stops_the_start() {
    let program = main(SOURCE);
    for bad in [
        // Padded, the standard alphabet, a signing key's hexadecimal, short.
        "gIGCg4SFhoeIiYqLjI2Oj5CRkpOUlZaXmJmam5ydnp8=",
        "gIGCg4SFhoeIiYqLjI2Oj5CRkpOUlZaXmJmam5ydnp/",
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        "gIGCg4SFhoeIiYqLjI2Oj5CRkpOUlZaXmJmam5ydnp",
    ] {
        for (variable, others) in [
            (
                "NOTES_KEY",
                [
                    ("RECEIPT_KEY", RECEIPT_KEY),
                    ("WEBHOOK_SECRET", WEBHOOK_SECRET),
                ],
            ),
            (
                "RECEIPT_KEY",
                [("NOTES_KEY", NOTES_KEY), ("WEBHOOK_SECRET", WEBHOOK_SECRET)],
            ),
            (
                "NOTES_KEY_PREVIOUS",
                [
                    ("RECEIPT_KEY", RECEIPT_KEY),
                    ("WEBHOOK_SECRET", WEBHOOK_SECRET),
                ],
            ),
        ] {
            let mut pairs = vec![(variable, bad)];
            pairs.extend(others);
            if variable == "NOTES_KEY_PREVIOUS" {
                pairs.push(("NOTES_KEY", NOTES_KEY));
            }
            let problems = program
                .prepared()
                .config(&variables(&pairs), Mode::Environment, &|_: &mut [u8]| false)
                .expect_err("refused");
            assert_eq!(problems.len(), 1, "{variable}={bad}: {problems:?}");
            assert_eq!(problems[0].variable, variable);
            assert!(problems[0].secret);
            assert!(
                matches!(&problems[0].kind, ProblemKind::Invalid { .. }),
                "{problems:?}"
            );
            // The refusal names the variable and the rule, never the value.
            assert!(!format!("{problems:?}").contains(bad));
        }
    }
}

#[test]
fn a_test_run_makes_its_own_keys() {
    let program = main(SOURCE);
    let mut counter = 0u8;
    let random = std::cell::RefCell::new(move |bytes: &mut [u8]| {
        for byte in bytes {
            counter = counter.wrapping_add(17);
            *byte = counter;
        }
        true
    });
    let values = program
        .prepared()
        .config(&|_: &str| None, Mode::Test, &|bytes: &mut [u8]| {
            (random.borrow_mut())(bytes)
        })
        .expect("a test run starts");
    let program = program.with_config(&values);
    let note = sealed(&program, "seen on Monday", "note-1");
    assert_eq!(
        opens(&program, &note, "note-1").as_deref(),
        Some(&b"seen on Monday"[..])
    );
    assert_eq!(bytes(&value(&program, "signed", vec![text("r")])).len(), 64);
}
