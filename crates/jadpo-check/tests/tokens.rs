//! Area 19, "Hashing and cryptography" and "The standard types" (chunk
//! 5.5r): `crypto.verify_jwt` and the token types `Jwt`, `JwtClaims`,
//! `Jwk`, `OAuthScope` and `OidcClaims`. `verify_jwt` reads the clock, so
//! only actions and tests call it; what a token says is known only when the
//! program runs, so each way it is refused needs an arm, `wrong_issuer`
//! only when the call names an `issuer:`. A `Jwk` holds a public key, never
//! a secret.
//!
//! Chunk 5.5r4: a `Jwk` holds an Ed25519, an RSA or a P-256 key, each held
//! to its parts by its rules; `as:` reads a token's own claims as an
//! application's `Object`, which `mismatch` needs an arm for only then;
//! and `crypto.read_certificate` reads DER, failing `malformed`.

use crate::support;

use jadpo_db::Layout;
use support::{problems, rules};

const TYPES: &str = "type Refused = Failure {\n    kind: FailureKind.invalid_value\n}\n\ntype Expired = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";

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

const ARMS: &str = "        invalid_signature: Refused\n        expired: Expired\n        not_yet_valid: Refused\n        malformed: Refused\n        wrong_audience: Refused\n";

fn verified(call: &str, arms: &str) -> String {
    format!(
        "action verified(token: Jwt, keys: List<Jwk>) fails Refused, Expired -> JwtClaims {{\n    return attempt {call} {{\n{arms}    }}\n}}\n"
    )
}

#[test]
fn an_action_checks_a_token_against_its_keys() {
    let call = "crypto.verify_jwt(token, keys: keys, issuer: \"https://id.example.com\", audience: \"clinic\")";
    assert_eq!(
        check(&verified(
            call,
            &format!("{ARMS}        wrong_issuer: Refused\n")
        )),
        NONE
    );
    // With no issuer named, none is checked, so `wrong_issuer` needs no
    // arm; the audience is always checked.
    assert_eq!(
        check(&verified(
            "crypto.verify_jwt(token, keys: keys, audience: \"clinic\")",
            ARMS
        )),
        NONE
    );
    assert_eq!(
        check(&verified("crypto.verify_jwt(token, keys: keys)", ARMS)),
        NONE
    );
}

#[test]
fn every_way_a_token_is_refused_needs_an_arm() {
    assert_eq!(
        check(
            "action verified(token: Jwt, keys: List<Jwk>) -> JwtClaims {\n    return crypto.verify_jwt(token, keys: keys)\n}\n"
        ),
        ["failure.attempt_required"]
    );
    // `expired` left out.
    let without_expired = ARMS.replace("        expired: Expired\n", "");
    assert_eq!(
        check(&verified(
            "crypto.verify_jwt(token, keys: keys)",
            &without_expired
        ))
        .first()
        .map(String::as_str),
        Some("failure.rule_unmapped")
    );
    // `wrong_issuer` named by an issuer.
    assert_eq!(
        check(&verified(
            "crypto.verify_jwt(token, keys: keys, issuer: \"https://id.example.com\")",
            ARMS
        ))
        .first()
        .map(String::as_str),
        Some("failure.rule_unmapped")
    );
}

#[test]
fn a_pure_body_cant_check_a_token() {
    // Its times are checked against the clock, as `clock.now` reads it.
    let source = format!(
        "function verified(token: Jwt, keys: List<Jwk>) fails Refused, Expired -> JwtClaims {{\n    return attempt crypto.verify_jwt(token, keys: keys) {{\n{ARMS}    }}\n}}\n"
    );
    assert_eq!(check(&source), ["type.clock_context"]);
    let message = shown(&source);
    assert!(
        message[0].contains("A function can't call `crypto.verify_jwt`"),
        "{message:?}"
    );
}

#[test]
fn keys_are_jwks_and_a_token_is_a_jwt() {
    assert_eq!(
        check(
            "action verified(token: Jwt, keys: List<PublicKey>) fails Refused, Expired -> JwtClaims {\n    return attempt crypto.verify_jwt(token, keys: keys) {\n        invalid_signature: Refused\n        expired: Expired\n        not_yet_valid: Refused\n        malformed: Refused\n        wrong_audience: Refused\n    }\n}\n"
        )
        .first()
        .map(String::as_str),
        Some("type.mismatch")
    );
}

#[test]
fn a_token_s_claims_are_read_by_name() {
    assert_eq!(
        check(
            "type Granted = Bool {}\n\ntype AudienceCount = Int {}\n\ntype ExpiresAt = Instant {}\n\nfunction for_member(claims: JwtClaims) -> Granted {\n    return Granted(claims.sub == \"member-42\")\n}\n\nfunction audiences(claims: JwtClaims) -> AudienceCount {\n    return AudienceCount(list.count(claims.aud))\n}\n\nfunction expires(claims: JwtClaims) -> ExpiresAt {\n    return ExpiresAt(claims.exp)\n}\n"
        ),
        NONE
    );
}

#[test]
fn a_jwk_holds_an_ed25519_public_key_and_no_secret() {
    let key = |kty: &str, crv: &str| {
        format!(
            "function key() -> Jwk {{\n    return Jwk {{\n        kty: \"{kty}\"\n        crv: \"{crv}\"\n        x: \"11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo\"\n        y: none\n        n: none\n        e: none\n        kid: \"2026-10\"\n        alg: none\n        use_: \"sig\"\n        x5c: none\n    }}\n}}\n"
        )
    };
    assert_eq!(check(&key("OKP", "Ed25519")), NONE);
    // A shared secret's type and another curve are refused as written.
    assert_ne!(check(&key("oct", "Ed25519")), NONE);
    assert_ne!(check(&key("EC", "P-256")), NONE);
    // A private key has nowhere to go.
    assert_ne!(
        check(
            "function key() -> Jwk {\n    return Jwk {\n        kty: \"OKP\"\n        crv: \"Ed25519\"\n        x: \"11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo\"\n        d: \"nWGxne_9WmC6hEr0kuwsxERJxWl7MmkZcDusAxyuf2A\"\n        kid: none\n        alg: none\n        use_: none\n    }\n}\n"
        ),
        NONE
    );
}

#[test]
fn scopes_and_identity_claims_are_standard_types() {
    assert_eq!(
        check(
            "type Granted = Bool {}\n\nfunction reads(scope: OAuthScope) -> Granted {\n    return Granted(scope == \"todos:read\")\n}\n\nfunction verified_address(claims: OidcClaims) -> Granted {\n    return Granted(claims.email_verified == true and claims.email != none)\n}\n"
        ),
        NONE
    );
    // A scope-token has no space: two scopes are two values.
    assert_ne!(
        check("function scope() -> OAuthScope {\n    return \"openid email\"\n}\n"),
        NONE
    );
}

const RSA_N: &str = "ofgWCuLjybRlzo0tZWJjNiuSfb4p4fAkd_wWJcyQoTbji9k0l8W26mPddxHmfHQp-Vaw-4qPCJrcS2mJPMEzP1Pt0Bm4d4QlL-yRT-SFd2lZS-pCgNMsD1W_YpRPEwOWvG6b32690r2jZ47soMZo9wGzjb_7OMg0LOL-bSf63kpaSHSXndS5z5rexMdbBYUsLA9e-KXBdQOS-UTo7WTBEMa2R2CapHg665xsmtdVMTBQY4uDZlxvb3qCo5ZwKh9kG4LT6_I5IhlJH7aGhyxXFvUK-DWNmoudF8NAco9_h9iaGNj8q2ethFkMLs91kzk2PAcDTW9gb54h4FRWyuXpoQ";

/// A 1,024-bit modulus, too small to hold.
const SMALL_N: &str = "pb1wVenLotoly3kkTJYZAgVZitibPm2YTBTdYJobj6gVv5md3RjcXfdHd_X63n5hbTwk-sggCicy68o4dl0Wrdb5lErhZhvhYy5ScBk3rOL5AKX04D9jwI_AtX6kJK42OaTEEeYNsJVKHv7P1-4_6ATy6u8nlyQUcIbsGsXtNu0";

#[test]
fn a_jwk_holds_an_rsa_or_a_p256_key_by_its_rules() {
    let rsa = |n: &str, alg: &str| {
        format!(
            "function key() -> Jwk {{\n    return Jwk {{\n        kty: \"RSA\"\n        crv: none\n        x: none\n        y: none\n        n: \"{n}\"\n        e: \"AQAB\"\n        kid: \"rsa-1\"\n        alg: {alg}\n        use_: \"sig\"\n        x5c: none\n    }}\n}}\n"
        )
    };
    assert_eq!(check(&rsa(RSA_N, "\"RS256\"")), NONE);
    assert_eq!(check(&rsa(RSA_N, "none")), NONE);
    // Another kind's algorithm, and a modulus too small to hold.
    assert_ne!(check(&rsa(RSA_N, "\"ES256\"")), NONE);
    assert_ne!(check(&rsa(SMALL_N, "none")), NONE);
    let ec = |y: &str| {
        format!(
            "function key() -> Jwk {{\n    return Jwk {{\n        kty: \"EC\"\n        crv: \"P-256\"\n        x: \"f83OJ3D2xF1Bg8vub9tLe1gHMzV76e8Tus9uPHvRVEU\"\n        y: \"{y}\"\n        n: none\n        e: none\n        kid: none\n        alg: \"ES256\"\n        use_: none\n        x5c: none\n    }}\n}}\n"
        )
    };
    assert_eq!(
        check(&ec("x_FEzRu9m36HLN_tue659LNpXW6pCyStikYjKIWI5a0")),
        NONE
    );
    // A point off the curve.
    let refused = shown(&ec("x_FEzRu9m36HLN_tue659LNpXW6pCyStikYjKIWI5a4"));
    assert!(
        refused
            .iter()
            .any(|problem| problem.contains("a_usable_key")),
        "{refused:?}"
    );
}

const SIGN_IN: &str = "type SignIn = Object {\n    email: Email\n    email_verified: Bool\n}\n\n";

fn signed_in(call: &str, arms: &str) -> String {
    format!(
        "{SIGN_IN}type Unexpected = Failure {{\n    kind: FailureKind.invalid_value\n    internal: {{\n        problem: DecodeProblem\n    }}\n}}\n\naction signed_in(token: Jwt, keys: List<Jwk>) fails Refused, Expired, Unexpected -> SignIn {{\n    return attempt {call} {{\n{arms}    }}\n}}\n"
    )
}

#[test]
fn a_token_s_own_claims_are_read_as_an_object_of_the_application_s() {
    let call = "crypto.verify_jwt(token, keys: keys, audience: \"clinic\", as: SignIn)";
    assert_eq!(
        check(&signed_in(
            call,
            &format!("{ARMS}        mismatch: Unexpected\n")
        )),
        NONE
    );
    // Claims that may not be the type need their arm.
    assert_eq!(
        check(&signed_in(call, ARMS)).first().map(String::as_str),
        Some("failure.rule_unmapped")
    );
    // It gives the type `as:` names, not `JwtClaims`.
    let as_claims = format!(
        "{SIGN_IN}action claims(token: Jwt, keys: List<Jwk>) fails Refused, Expired -> JwtClaims {{\n    return attempt {call} {{\n{ARMS}        mismatch: Refused\n    }}\n}}\n"
    );
    assert_eq!(
        check(&as_claims).first().map(String::as_str),
        Some("type.mismatch")
    );
}

#[test]
fn as_names_an_object_without_the_registered_claims() {
    for (declared, ty, expected) in [
        (
            "",
            "Text",
            "is an object type of the application's, not `Text`",
        ),
        (
            "",
            "JwtClaims",
            "is an object type of the application's, not `JwtClaims`",
        ),
        (
            "type Member = Object {\n    sub: Text\n    email: Email\n}\n\n",
            "Member",
            "`Member.sub` is a registered claim",
        ),
    ] {
        let source = format!(
            "{declared}action claims(token: Jwt, keys: List<Jwk>) fails Refused, Expired -> Text {{\n    var read: Text = attempt crypto.verify_jwt(token, keys: keys, as: {ty}) {{\n{ARMS}        mismatch: Refused\n    }}\n    return \"read\"\n}}\n"
        );
        let found = shown(&source);
        assert!(
            found.iter().any(|problem| problem.contains(expected)),
            "{ty}: {found:?}"
        );
    }
}

#[test]
fn a_certificate_is_read_from_its_der() {
    let read = |arms: &str| {
        format!(
            "type Subject = Text {{}}\n\ntype Der = Bytes {{}}\n\nfunction subject(der: Der) fails Refused -> Subject {{\n    var certificate: Certificate = attempt crypto.read_certificate(der) {{\n{arms}    }}\n    return Subject(certificate.subject)\n}}\n"
        )
    };
    assert_eq!(check(&read("        malformed: Refused\n")), NONE);
    assert_eq!(
        check(&read("")).first().map(String::as_str),
        Some("failure.rule_unmapped")
    );
}
