//! Area 19, "Secret values" (chunk 5.5r2): `Password`, `ApiKey`,
//! `BearerToken`, `TotpSecret`, `SecretText` and `Jwt` arrive in an
//! `Input`, are compared with `==` and `!=` and passed on as themselves,
//! and go only to the operation made for each. `type.secret_exposed`
//! refuses one in an `Output`, a failure, an event, an email, a message
//! and an entity, however deep, and passed to any other operation or
//! operator; `json.write` can't write one; storing a `TotpSecret` or a
//! `SecretText` is not built yet.

use crate::support;

use jadpo_db::Layout;
use support::{problems, rules};

const TYPES: &str = "type Handle = Text {\n    max_length: 40\n}\n\ntype Wire = Text {}\n\ntype Count = Int {}\n\ntype Same = Bool {}\n\ntype Verifier = Bytes {}\n\ntype StaffPassword = Password {\n    min_length: 12\n}\n\ntype Refused = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";

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

#[test]
fn a_secret_arrives_is_compared_and_goes_where_it_is_made_to() {
    assert_eq!(
        check(
            "type SignUp = Input {\n    handle: Handle\n    password: StaffPassword\n}\n\ntype Held = Object {\n    token: Jwt?\n    keys: List<ApiKey>\n}\n\naction stored(form: SignUp) -> PasswordHash {\n    return crypto.hash_password(form.password)\n}\n\nfunction signs_in(given: Password, kept: PasswordHash) -> PasswordCheck {\n    return crypto.verify_password(given, kept)\n}\n\nfunction verifier(key: ApiKey) -> Verifier {\n    return Verifier(hash.sha256(key))\n}\n\nfunction token_verifier(token: BearerToken) -> Verifier {\n    return Verifier(hash.sha256(token))\n}\n\nfunction same(given: ApiKey, expected: ApiKey) -> Same {\n    return Same(given == expected and given != \"example_key_0\")\n}\n\nfunction narrowed(given: Password) fails Refused -> StaffPassword {\n    return attempt StaffPassword(given) {\n        min_length: Refused\n    }\n}\n\nfunction read(raw: Wire) fails Refused -> TotpSecret {\n    return attempt text.parse(raw, as: TotpSecret) {\n        malformed: Refused\n    }\n}\n\nfunction passed_on(held: Held) -> Jwt? {\n    return held.token\n}\n"
        ),
        NONE
    );
}

#[test]
fn a_secret_never_leaves_in_a_declaration() {
    for (declaration, why) in [
        (
            "type Profile = Output {\n    handle: Handle\n    key: ApiKey\n}\n",
            "sent",
        ),
        (
            "type Profile = Output {\n    tokens: List<Jwt>\n}\n",
            "sent in a list",
        ),
        (
            "type Profile = Output {\n    passwords: Map<Handle, StaffPassword>\n}\n",
            "sent in a map, built on one",
        ),
        (
            "type Held = Object {\n    secret: SecretText?\n}\n\ntype Profile = Output {\n    held: Held\n}\n",
            "sent inside an object",
        ),
        (
            "type SignUp = Input {\n    password: Password\n}\n\ntype Echo = Output {\n    form: SignUp\n}\n",
            "sent back inside the input it came in",
        ),
        (
            "type Choice = Enum {\n    keyed: {\n        key: ApiKey\n    }\n    open\n}\n\ntype Profile = Output {\n    choice: Choice\n}\n",
            "sent in a variant's data",
        ),
        (
            "type WrongToken = Failure {\n    kind: FailureKind.invalid_value\n    client: {\n        tried: BearerToken\n    }\n}\n",
            "in a failure's client fields",
        ),
        (
            "type WrongToken = Failure {\n    kind: FailureKind.invalid_value\n    internal: {\n        tried: TotpSecret\n    }\n}\n",
            "in a failure's internal fields, which are logged",
        ),
        (
            "type KeyIssued = Event {\n    key: ApiKey\n}\n",
            "in an event",
        ),
    ] {
        assert_eq!(check(declaration), ["type.secret_exposed"], "{why}");
    }
    assert_eq!(
        shown("type Profile = Output {\n    key: ApiKey\n}\n"),
        ["main.jadpo type.secret_exposed: `Profile.key` holds an `ApiKey`"]
    );
}

#[test]
fn a_secret_is_never_emailed_or_shown_in_a_message() {
    assert_eq!(
        rules(
            Layout::Project,
            &[
                ("app.jadpo", "application {}\n"),
                (
                    "domains/accounts/emails/reset_email.jadpo",
                    "/// Sent when a password is reset.\ntype ResetEmail = EmailMessage {\n    new_password: Password\n}\n",
                ),
            ]
        ),
        ["type.secret_exposed"]
    );
    assert_eq!(
        check("/// Shows a key.\ntype KeyShown = Message {\n    key: ApiKey\n}\n"),
        ["type.secret_exposed"]
    );
}

#[test]
fn a_secret_is_never_stored_as_itself() {
    for (field, expected) in [
        ("password: Password", "type.secret_exposed"),
        ("password: StaffPassword", "type.secret_exposed"),
        ("key: ApiKey", "type.secret_exposed"),
        ("token: BearerToken", "type.secret_exposed"),
        ("token: Jwt?", "type.secret_exposed"),
        ("keys: List<ApiKey>", "type.secret_exposed"),
        // Kept sealed once keys held as data and encrypted fields arrive.
        ("secret: TotpSecret", "toolchain.not_built_yet"),
        ("note: SecretText", "toolchain.not_built_yet"),
    ] {
        assert_eq!(
            check(&format!(
                "entity Member {{\n    id: Uuid\n    {field}\n}}\n"
            )),
            [expected],
            "{field}"
        );
    }
    // What is kept is the hash.
    assert_eq!(
        check(
            "entity Member {\n    id: Uuid\n    password_hash: PasswordHash\n    key_verifier: Verifier\n}\n"
        ),
        NONE
    );
    assert_eq!(
        shown("entity Member {\n    id: Uuid\n    password: Password\n}\n"),
        ["main.jadpo type.secret_exposed: `Member.password` keeps a `Password`"]
    );
}

#[test]
fn nothing_else_is_made_from_a_secret() {
    for (body, expected) in [
        // Read as text, a part, a format for people.
        ("return Count(text.length(given))", "type.secret_exposed"),
        ("return Wire(text.from(given))", "type.secret_exposed"),
        (
            "return Wire(text.keep_first(given, 4))",
            "type.secret_exposed",
        ),
        // Joined, ordered.
        ("return Wire(given + given)", "type.secret_exposed"),
        ("return Count(list.count([given]))", "type.secret_exposed"),
        // Built into text.
        ("return Wire(given)", "type.secret_exposed"),
        // Written as JSON.
        ("return Wire(json.write(given))", "type.mismatch"),
    ] {
        let source = format!(
            "function f(given: ApiKey) -> {} {{\n    {body}\n}}\n",
            if body.starts_with("return Count") {
                "Count"
            } else {
                "Wire"
            }
        );
        assert_eq!(check(&source), [expected], "{body}");
    }
    assert_eq!(
        check("function f(given: ApiKey) -> Same {\n    return Same(given < \"m\")\n}\n"),
        ["type.secret_exposed"]
    );
    assert_eq!(
        check(
            "function f(given: Password, shown: Locale) -> PresentationText {\n    return text.format(given, locale: shown)\n}\n"
        ),
        ["type.secret_exposed"]
    );
    // A password is hashed by Argon2id alone, and another secret builds no
    // password.
    assert_eq!(
        check(
            "function f(given: Password) -> Verifier {\n    return Verifier(hash.sha256(given))\n}\n"
        ),
        ["type.secret_exposed"]
    );
    assert_eq!(
        check("function f(given: ApiKey) -> Password {\n    return Password(given)\n}\n"),
        ["type.secret_exposed"]
    );
    // And a password is a `Password`: text isn't hashed as one.
    assert_eq!(
        check(
            "action f(given: Handle) -> PasswordHash {\n    return crypto.hash_password(given)\n}\n"
        ),
        ["type.mismatch"]
    );
    assert_eq!(
        shown("function f(given: ApiKey) -> Count {\n    return Count(text.length(given))\n}\n"),
        ["main.jadpo type.secret_exposed: `text.length` can't take an `ApiKey`"]
    );
}

#[test]
fn a_test_writes_a_secret_and_compares_it() {
    assert_eq!(
        rules(
            Layout::Loose,
            &[
                (
                    "main.jadpo",
                    &format!(
                        "{TYPES}function same(given: ApiKey, expected: ApiKey) -> Same {{\n    return Same(given == expected)\n}}\n"
                    )
                ),
                (
                    "main.test.jadpo",
                    "test \"keys compare\" {\n    var key: ApiKey = \"example_key_0123456789abcdef\"\n    assert same(key, \"example_key_0123456789abcdef\")\n    assert key != \"example_key_0123456789abcdeX\"\n}\n"
                ),
            ]
        ),
        NONE
    );
}
