//! External JWT authentication (area 8). A verified provider subject is
//! resolved against current application authority on every request; no
//! provider profile or permission claim becomes an application identity.

use jadpo_std::standard::security::JWT_MAX_LENGTH;
use jadpo_value::Value;

/// Values bound at startup. Debug never writes deployment values.
#[derive(Clone)]
pub struct Settings {
    pub(crate) issuer: String,
    pub(crate) audience: String,
}

impl std::fmt::Debug for Settings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ExternalTokenSettings")
    }
}

impl Settings {
    pub(crate) fn read(
        plan: &jadpo_std::http::ExternalTokenPlan,
        values: &crate::config::ConfigValues,
    ) -> Option<Self> {
        let issuer = values.text(&plan.issuer)?.to_string();
        let audience = values.text(&plan.audience)?.to_string();
        if issuer.is_empty() || audience.is_empty() {
            return None;
        }
        Some(Self { issuer, audience })
    }
}

/// Only a verified nonempty subject leaves the cryptographic boundary.
/// All signature, claim and temporal checks are area 19's shared checks.
pub(crate) fn verified_subject(
    token: &str,
    keys: &[Value],
    issuer: &str,
    audience: &str,
    now: i64,
) -> Option<String> {
    if token.len() > JWT_MAX_LENGTH {
        return None;
    }
    let Value::Record(claims) =
        jadpo_value::jwt::verify_jwt(token, keys, Some(issuer), Some(audience), now).ok()?
    else {
        return None;
    };
    match claims.get("sub") {
        Some(Value::Text(subject)) if !subject.is_empty() => Some(subject.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jadpo_value::crypto::{KeyRing, public_key, sign};
    use jadpo_value::encoding::to_base64url;

    const NOW: i64 = 1_800_000_000_000;
    const ISSUER: &str = "https://identity.example/tenant/";
    const AUDIENCE: &str = "application";

    fn signer() -> KeyRing {
        KeyRing::from_signing_text("0123456789abcdef0123456789abcdef", None)
    }

    fn key() -> Value {
        jadpo_value::jwt::read_jwk(&serde_json::json!({
            "kty":"OKP", "crv":"Ed25519", "kid":"current",
            "x":public_key(&signer()).expect("public key")
        }))
        .expect("a public JWK")
    }

    fn token(header: &str, claims: &str) -> String {
        let body = format!(
            "{}.{}",
            to_base64url(header.as_bytes()),
            to_base64url(claims.as_bytes())
        );
        let signature = sign(body.as_bytes(), &signer()).expect("signed");
        format!("{body}.{}", to_base64url(&signature))
    }

    fn claims() -> serde_json::Value {
        serde_json::json!({
            "iss":ISSUER, "aud":AUDIENCE, "sub":"provisioned-subject", "exp":NOW / 1000 + 60,
            "email":"attacker@example.test", "role":"administrator", "scope":"everything"
        })
    }

    fn verify(token: &str) -> Option<String> {
        verified_subject(token, &[key()], ISSUER, AUDIENCE, NOW)
    }

    const HEADER: &str = r#"{"alg":"EdDSA","kid":"current"}"#;

    #[test]
    fn only_the_verified_subject_crosses_into_identity_resolution() {
        let token = token(HEADER, &claims().to_string());
        assert_eq!(verify(&token).as_deref(), Some("provisioned-subject"));
        assert!(
            verified_subject(
                &token,
                &[key()],
                ISSUER.trim_end_matches('/'),
                AUDIENCE,
                NOW
            )
            .is_none()
        );
        assert!(verified_subject(&token, &[key()], ISSUER, "another-app", NOW).is_none());
        assert!(verified_subject(&token, &[], ISSUER, AUDIENCE, NOW).is_none());
    }

    #[test]
    fn invalid_credentials_never_produce_a_subject() {
        let mut expired = claims();
        expired["exp"] = (NOW / 1000 - 31).into();
        let mut future = claims();
        future["nbf"] = (NOW / 1000 + 31).into();
        let mut missing_subject = claims();
        missing_subject
            .as_object_mut()
            .expect("claims")
            .remove("sub");
        let mut empty_subject = claims();
        empty_subject["sub"] = "".into();
        for denied in [
            token(HEADER, &expired.to_string()),
            token(HEADER, &future.to_string()),
            token(HEADER, &missing_subject.to_string()),
            token(HEADER, &empty_subject.to_string()),
            token(r#"{"alg":"none","kid":"current"}"#, &claims().to_string()),
            token(r#"{"alg":"HS256","kid":"current"}"#, &claims().to_string()),
            token(r#"{"alg":"EdDSA","kid":"unknown"}"#, &claims().to_string()),
            token(r#"{"alg":"EdDSA","alg":"EdDSA"}"#, &claims().to_string()),
            token(
                HEADER,
                r#"{"iss":"https://identity.example/tenant/","aud":"application","sub":"first","sub":"second","exp":1800000060}"#,
            ),
            "not-a-token".into(),
            "x".repeat(JWT_MAX_LENGTH + 1),
        ] {
            assert!(
                verify(&denied).is_none(),
                "a refused credential produced a subject"
            );
        }
    }
}
