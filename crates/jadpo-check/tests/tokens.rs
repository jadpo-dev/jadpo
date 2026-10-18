//! Area 19, "Hashing and cryptography" and "The standard types" (chunk
//! 5.5r): `crypto.verify_jwt` and the token types `Jwt`, `JwtClaims`,
//! `Jwk`, `OAuthScope` and `OidcClaims`. `verify_jwt` reads the clock, so
//! only actions and tests call it; what a token says is known only when the
//! program runs, so each way it is refused needs an arm, `wrong_issuer`
//! only when the call names an `issuer:`. A `Jwk` holds a public key, never
//! a secret.

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
            "function key() -> Jwk {{\n    return Jwk {{\n        kty: \"{kty}\"\n        crv: \"{crv}\"\n        x: \"11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo\"\n        kid: \"2026-10\"\n        alg: none\n        use_: \"sig\"\n    }}\n}}\n"
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
