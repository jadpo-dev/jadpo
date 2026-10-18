//! Area 19, "Secret values" (chunk 5.5r2), run: a secret value read from
//! JSON, written in a test or passed in is held as a secret, compared in
//! constant time, hashed by the operation made for it, and written only
//! redacted wherever a report or a failed assertion shows a value.

use crate::support;

use jadpo_interp::{AssertionDetail, Outcome, Value};
use support::{program, run, text, value};

const SOURCE: &str = "type Wire = Text {}\n\ntype Same = Bool {}\n\ntype Verifier = Bytes {}\n\ntype StaffPassword = Password {\n    min_length: 12\n}\n\ntype Unreadable = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype TooShort = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype Credentials = Object {\n    key: ApiKey\n}\n\nfunction same(given: ApiKey, expected: ApiKey) -> Same {\n    return Same(given == expected)\n}\n\nfunction read_key(raw: Wire) fails Unreadable -> ApiKey {\n    var sent: Credentials = attempt json.read(raw, as: Credentials) {\n        malformed: Unreadable\n        mismatch: Unreadable\n    }\n    return sent.key\n}\n\nfunction verifier(key: ApiKey) -> Verifier {\n    return Verifier(hash.sha256(key))\n}\n\nfunction staff(given: Password) fails TooShort -> StaffPassword {\n    return attempt StaffPassword(given) {\n        min_length: TooShort\n    }\n}\n\naction stored(password: StaffPassword) -> PasswordHash {\n    return crypto.hash_password(password)\n}\n\nfunction signs_in(password: Password, kept: PasswordHash) -> PasswordCheck {\n    return crypto.verify_password(password, kept)\n}\n";

const TESTS: &str = "test \"the keys differ\" {\n    var key: ApiKey = \"example_key_0123456789abcdef\"\n    assert key == \"example_key_0123456789abcdeX\"\n}\n";

const KEY: &str = "example_key_0123456789abcdef";

fn secrets() -> jadpo_interp::Program {
    program(&[("main.jadpo", SOURCE), ("main.test.jadpo", TESTS)])
}

#[test]
fn a_secret_read_from_json_is_held_secret_and_compared() {
    let program = secrets();
    let read = value(
        &program,
        "read_key",
        vec![text(&format!("{{\"key\": \"{KEY}\"}}"))],
    );
    assert_eq!(read, Value::secret("ApiKey", KEY));
    assert_eq!(read.to_string(), "<redacted ApiKey>");
    assert!(!format!("{read:?}").contains(KEY));
    // Equal to the same text, and not to text one character apart, at the
    // start or at the end.
    let same = |expected: &str| {
        value(
            &program,
            "same",
            vec![read.clone(), Value::secret("ApiKey", expected)],
        )
    };
    assert_eq!(same(KEY), Value::Bool(true));
    assert_eq!(same("example_key_0123456789abcdeX"), Value::Bool(false));
    assert_eq!(same("Xxample_key_0123456789abcdef"), Value::Bool(false));
    assert_eq!(same("example_key_0123456789abcde"), Value::Bool(false));
}

#[test]
fn a_key_s_verifier_is_its_sha256() {
    let program = secrets();
    let Value::Bytes(digest) = value(&program, "verifier", vec![Value::secret("ApiKey", KEY)])
    else {
        panic!("a digest");
    };
    assert_eq!(&*digest, &jadpo_value::hash::sha256(KEY.as_bytes())[..]);
}

#[test]
fn a_password_is_hashed_and_checked_as_a_secret() {
    let program = secrets();
    let password = Value::secret("Password", "correct horse battery staple");
    let kept = value(&program, "stored", vec![password.clone()]);
    assert_eq!(
        value(&program, "signs_in", vec![password, kept.clone()]).to_string(),
        "PasswordCheck.correct"
    );
    assert_eq!(
        value(
            &program,
            "signs_in",
            vec![
                Value::secret("Password", "correct horse battery stapler"),
                kept
            ]
        )
        .to_string(),
        "PasswordCheck.wrong"
    );
    // A rule it breaks is named, never the password.
    let Outcome::Failed(failure) = run(
        &program,
        "staff",
        vec![Value::secret("Password", "Tr0ub4dor&3")],
    ) else {
        panic!("too short");
    };
    assert_eq!(failure.name, "TooShort");
    assert!(!format!("{failure:?}").contains("Tr0ub4dor&3"));
}

#[test]
fn a_failed_assertion_holds_both_sides_redacted() {
    let program = secrets();
    let outcome = run(&program, "test main \"the keys differ\"", vec![]);
    let Outcome::Assertion(assertion) = &outcome else {
        panic!("the assertion holds: {outcome:?}");
    };
    let AssertionDetail::False {
        comparison: Some(("==", actual, expected)),
    } = &assertion.detail
    else {
        panic!("a comparison");
    };
    assert_eq!(actual, &Value::secret("ApiKey", KEY));
    assert_eq!(expected.to_string(), "<redacted ApiKey>");
    let report = program
        .report(&outcome)
        .map(|report| report.plain())
        .expect("a report");
    assert!(report.contains("expected: <redacted ApiKey>"), "{report}");
    assert!(report.contains("actual: <redacted ApiKey>"), "{report}");
    // The assertion's own line is shown as written; the key it holds,
    // written on another, never is.
    assert!(!report.contains(KEY), "{report}");
}
