//! External JWT authentication (area 8). A verified provider subject is
//! resolved against current application authority on every request; no
//! provider profile or permission claim becomes an application identity.

use super::identity::{
    AUTHENTICATION_MISCONFIGURED, AUTHENTICATION_UNAVAILABLE, INVALID_CREDENTIALS,
};
use jadpo_std::standard::security::JWT_MAX_LENGTH;
use jadpo_value::Value;
use std::cell::RefCell;
use std::rc::Rc;

/// Compiler-owned bounds (area 8): fixed per provider and startup state.
pub const DOCUMENT_BYTES: usize = 65_536;
pub const JWKS_BYTES: usize = 262_144;
pub const MOST_KEYS: usize = 64;
pub const FETCH_MS: u64 = 3_000;
const CACHE_MS: i64 = 300_000;
const REFRESH_MS: i64 = 30_000;
type Failure = (u16, &'static str, &'static str);

#[derive(Default)]
struct State {
    keys: Vec<Value>,
    expires: i64,
    next_refresh: i64,
    busy_until: i64,
    generation: u64,
    failure: Option<Failure>,
}

/// Values bound at startup. Debug never writes deployment values.
#[derive(Clone)]
pub struct Settings {
    pub(crate) issuer: String,
    pub(crate) audience: String,
    state: Rc<RefCell<State>>,
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
        Some(Self {
            issuer,
            audience,
            state: Rc::default(),
        })
    }
}

/// The configured issuer's discovery address: preserve the issuer value
/// itself exactly, and remove only its final slash from the request base.
fn discovery(issuer: &str) -> Option<String> {
    if issuer.contains(['?', '#']) || !https(issuer) {
        return None;
    }
    Some(format!(
        "{}/.well-known/openid-configuration",
        issuer.strip_suffix('/').unwrap_or(issuer)
    ))
}

/// URL shape only. Both hosts additionally guard resolved addresses,
/// TLS identity, redirects, response bytes and request cancellation.
fn https(url: &str) -> bool {
    if url.len() > 8_192 || !jadpo_std::is_url(url) || url.contains('#') {
        return false;
    }
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let authority = rest.split(['/', '?']).next().unwrap_or("");
    !authority.is_empty() && !authority.contains('@')
}

impl Settings {
    /// Verifies using bounded cached public keys. A cache is owned by this
    /// application's startup state, never global or keyed by token claims.
    pub(crate) async fn subject(
        &self,
        host: &dyn crate::Host,
        token: &str,
        now: i64,
    ) -> Result<String, Failure> {
        if jadpo_value::jwt::preflight_jwt(token).is_err() {
            return Err(INVALID_CREDENTIALS);
        }
        if discovery(&self.issuer).is_none() || self.audience.is_empty() {
            return Err(AUTHENTICATION_MISCONFIGURED);
        }
        let time = || host.now_ms().unwrap_or(now);
        let mut fresh = false;
        loop {
            let at = time();
            let (subject, refresh, busy, expired, failure) = {
                let state = self.state.borrow();
                let subject = if state.expires > at {
                    match check_subject(token, &state.keys, &self.issuer, &self.audience, now) {
                        Ok(subject) => Some(subject),
                        Err(jadpo_value::Fault::Broke(jadpo_std::RuleKey::InvalidSignature)) => {
                            None
                        }
                        Err(_) => return Err(INVALID_CREDENTIALS),
                    }
                } else {
                    None
                };
                (
                    subject,
                    state.next_refresh <= at,
                    state.busy_until > at,
                    state.expires <= at,
                    state.failure,
                )
            };
            if let Some(subject) = subject {
                return Ok(subject);
            }
            if fresh {
                return Err(INVALID_CREDENTIALS);
            }
            if busy {
                host.sleep(10).await;
                continue;
            }
            if !refresh {
                return Err(failure.unwrap_or(if expired {
                    AUTHENTICATION_UNAVAILABLE
                } else {
                    INVALID_CREDENTIALS
                }));
            }
            let generation = {
                let mut state = self.state.borrow_mut();
                state.generation = state.generation.wrapping_add(1);
                // Reserve the cooldown before awaiting: cancellation cannot
                // turn requests into an unbounded provider-fetch loop.
                state.next_refresh = at.saturating_add(REFRESH_MS);
                state.failure = Some(AUTHENTICATION_UNAVAILABLE);
                state.busy_until = at.saturating_add(2 * FETCH_MS as i64 + 1_000);
                state.generation
            };
            let guard = Refresh {
                state: Rc::clone(&self.state),
                generation,
            };
            let fetched = self.fetch(host).await;
            {
                let mut state = self.state.borrow_mut();
                // A request whose owner expired cannot replace a newer refresh.
                if state.generation == generation {
                    state.next_refresh = time().saturating_add(REFRESH_MS);
                    state.failure = fetched.as_ref().err().copied();
                    if let Ok(keys) = &fetched {
                        state.keys = keys.clone();
                        state.expires = time().saturating_add(CACHE_MS);
                    }
                }
            }
            drop(guard);
            fetched?;
            fresh = true;
        }
    }

    async fn fetch(&self, host: &dyn crate::Host) -> Result<Vec<Value>, Failure> {
        let address = discovery(&self.issuer).ok_or(AUTHENTICATION_MISCONFIGURED)?;
        let outbound = host.outbound().ok_or(AUTHENTICATION_UNAVAILABLE)?;
        let (status, bytes) = outbound
            .get(&address, DOCUMENT_BYTES, FETCH_MS)
            .await
            .map_err(|_| AUTHENTICATION_UNAVAILABLE)?;
        if status != 200 {
            return Err(AUTHENTICATION_UNAVAILABLE);
        }
        if bytes.len() > DOCUMENT_BYTES {
            return Err(AUTHENTICATION_MISCONFIGURED);
        }
        let document =
            jadpo_value::jwt::strict_document(&bytes).ok_or(AUTHENTICATION_MISCONFIGURED)?;
        if document.get("issuer").and_then(serde_json::Value::as_str) != Some(self.issuer.as_str())
        {
            return Err(AUTHENTICATION_MISCONFIGURED);
        }
        let jwks = document
            .get("jwks_uri")
            .and_then(serde_json::Value::as_str)
            .filter(|url| https(url))
            .ok_or(AUTHENTICATION_MISCONFIGURED)?;
        let (status, bytes) = outbound
            .get(jwks, JWKS_BYTES, FETCH_MS)
            .await
            .map_err(|_| AUTHENTICATION_UNAVAILABLE)?;
        if status != 200 {
            return Err(AUTHENTICATION_UNAVAILABLE);
        }
        if bytes.len() > JWKS_BYTES {
            return Err(AUTHENTICATION_MISCONFIGURED);
        }
        let document =
            jadpo_value::jwt::strict_document(&bytes).ok_or(AUTHENTICATION_MISCONFIGURED)?;
        let keys = document
            .get("keys")
            .and_then(serde_json::Value::as_array)
            .filter(|keys| !keys.is_empty() && keys.len() <= MOST_KEYS)
            .ok_or(AUTHENTICATION_MISCONFIGURED)?;
        // OIDC publishes encryption keys too. They never become a Jwk:
        // skip only explicitly non-signing keys, and validate every signing key.
        let mut signing = Vec::new();
        for key in keys {
            if key.get("use").and_then(serde_json::Value::as_str) == Some("enc") {
                continue;
            }
            signing.push(jadpo_value::jwt::read_jwk(key).ok_or(AUTHENTICATION_MISCONFIGURED)?);
        }
        if signing.is_empty() {
            return Err(AUTHENTICATION_MISCONFIGURED);
        }
        Ok(signing)
    }
}

/// Clearing an in-flight refresh when its request is cancelled neither
/// poisons the provider state nor lets an older request clear a new owner.
struct Refresh {
    state: Rc<RefCell<State>>,
    generation: u64,
}
impl Drop for Refresh {
    fn drop(&mut self) {
        let mut state = self.state.borrow_mut();
        if state.generation == self.generation {
            state.busy_until = 0;
        }
    }
}

/// Only a verified nonempty subject leaves the cryptographic boundary.
/// All signature, claim and temporal checks are area 19's shared checks.
#[cfg(test)]
fn verified_subject(
    token: &str,
    keys: &[Value],
    issuer: &str,
    audience: &str,
    now: i64,
) -> Option<String> {
    check_subject(token, keys, issuer, audience, now).ok()
}

fn check_subject(
    token: &str,
    keys: &[Value],
    issuer: &str,
    audience: &str,
    now: i64,
) -> Result<String, jadpo_value::Fault> {
    let refused = || jadpo_value::Fault::Broke(jadpo_std::RuleKey::Malformed);
    if token.len() > JWT_MAX_LENGTH {
        return Err(refused());
    }
    let Value::Record(claims) =
        jadpo_value::jwt::verify_jwt(token, keys, Some(issuer), Some(audience), now)?
    else {
        return Err(refused());
    };
    match claims.get("sub") {
        Some(Value::Text(subject)) if !subject.is_empty() => Ok(subject.to_string()),
        _ => Err(refused()),
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
    struct Provider {
        now: std::cell::Cell<i64>,
        calls: RefCell<Vec<String>>,
        keys: RefCell<serde_json::Value>,
        offline: std::cell::Cell<bool>,
        hang: std::cell::Cell<bool>,
        mismatch: std::cell::Cell<bool>,
    }
    impl Provider {
        fn new() -> Self {
            Self {
                now: std::cell::Cell::new(NOW),
                calls: RefCell::default(),
                keys: RefCell::new(serde_json::json!({"keys":[{
                    "kty":"OKP", "crv":"Ed25519", "kid":"current", "x":public_key(&signer()).expect("key")
                }, {"kty":"RSA", "use":"enc", "n":"unused", "e":"unused"}]})),
                offline: std::cell::Cell::new(false),
                hang: std::cell::Cell::new(false),
                mismatch: std::cell::Cell::new(false),
            }
        }
    }
    impl crate::Host for Provider {
        fn database(&self, _: &str) -> Option<&dyn crate::Database> {
            None
        }
        fn random(&self, _: &mut [u8]) -> bool {
            false
        }
        fn outbound(&self) -> Option<&dyn crate::Outbound> {
            Some(self)
        }
        fn now_ms(&self) -> Option<i64> {
            Some(self.now.get())
        }
    }
    impl crate::Outbound for Provider {
        fn get<'a>(
            &'a self,
            url: &'a str,
            limit: usize,
            timeout_ms: u64,
        ) -> crate::LocalFuture<'a, Result<(u16, Vec<u8>), String>> {
            Box::pin(async move {
                assert_eq!(timeout_ms, FETCH_MS);
                self.calls.borrow_mut().push(url.into());
                if self.hang.get() {
                    std::future::pending::<()>().await;
                }
                if self.offline.get() {
                    return Err("provider-private-diagnostic".into());
                }
                let document = if url.ends_with("/.well-known/openid-configuration") {
                    assert_eq!(limit, DOCUMENT_BYTES);
                    serde_json::json!({"issuer":if self.mismatch.get() { "https://wrong.example/" } else { ISSUER }, "jwks_uri":"https://keys.example/published"})
                } else {
                    assert_eq!(url, "https://keys.example/published");
                    assert_eq!(limit, JWKS_BYTES);
                    self.keys.borrow().clone()
                };
                Ok((200, document.to_string().into_bytes()))
            })
        }
    }
    fn settings() -> Settings {
        Settings {
            issuer: ISSUER.into(),
            audience: AUDIENCE.into(),
            state: Rc::default(),
        }
    }
    fn run(settings: &Settings, host: &Provider, token: &str) -> Result<String, Failure> {
        crate::block_on(settings.subject(host, token, host.now.get()))
            .expect("provider future completes")
    }

    #[test]
    fn discovery_keeps_tenant_paths_and_exact_issuer_identity() {
        assert_eq!(
            discovery(ISSUER).as_deref(),
            Some("https://identity.example/tenant/.well-known/openid-configuration")
        );
        assert_eq!(
            discovery("https://identity.example/tenant").as_deref(),
            Some("https://identity.example/tenant/.well-known/openid-configuration")
        );
        for invalid in [
            "http://identity.example",
            "https://identity.example/?query",
            "https://identity.example/#fragment",
            "https://user:secret@identity.example",
        ] {
            assert!(discovery(invalid).is_none());
        }
        let host = Provider::new();
        let settings = settings();
        host.mismatch.set(true);
        let token = token(HEADER, &claims().to_string());
        assert_eq!(
            run(&settings, &host, &token),
            Err(AUTHENTICATION_MISCONFIGURED)
        );
        assert_eq!(
            run(&settings, &host, &token),
            Err(AUTHENTICATION_MISCONFIGURED)
        );
        assert_eq!(host.calls.borrow().len(), 1);
    }

    #[test]
    fn cold_invalid_structure_is_refused_before_an_offline_provider_is_contacted() {
        for denied in [
            token(r#"{"alg":"none"}"#, &claims().to_string()),
            token(r#"{"alg":"HS256"}"#, &claims().to_string()),
            token(r#"{"alg":"EdDSA","alg":"EdDSA"}"#, &claims().to_string()),
            token(HEADER, r#"{"sub":"first","sub":"second"}"#),
            token(HEADER, "{"),
        ] {
            let host = Provider::new();
            host.offline.set(true);
            assert_eq!(run(&settings(), &host, &denied), Err(INVALID_CREDENTIALS));
            assert!(
                host.calls.borrow().is_empty(),
                "independently invalid tokens do not fetch"
            );
        }
        // Area 19 ignores these headers; they are not independently invalid
        // and must never choose a provider URL or become a key source.
        let host = Provider::new();
        host.offline.set(true);
        let supplied = token(
            r#"{"alg":"EdDSA","jku":"https://token-controlled.example/keys","x5u":"https://token-controlled.example/cert"}"#,
            &claims().to_string(),
        );
        assert_eq!(
            run(&settings(), &host, &supplied),
            Err(AUTHENTICATION_UNAVAILABLE)
        );
        assert_eq!(
            &*host.calls.borrow(),
            &["https://identity.example/tenant/.well-known/openid-configuration"]
        );
    }

    #[test]
    fn cached_claim_refusals_do_not_fetch_or_change_into_provider_outages() {
        let host = Provider::new();
        let settings = settings();
        let good = token(HEADER, &claims().to_string());
        assert_eq!(
            run(&settings, &host, &good).as_deref(),
            Ok("provisioned-subject")
        );
        assert_eq!(
            &*host.calls.borrow(),
            &[
                "https://identity.example/tenant/.well-known/openid-configuration",
                "https://keys.example/published"
            ]
        );
        host.offline.set(true);
        host.now.set(NOW + REFRESH_MS + 1);
        for field in ["iss", "aud", "exp"] {
            let mut bad = claims();
            bad[field] = if field == "exp" {
                (NOW / 1000 - 60).into()
            } else {
                "wrong".into()
            };
            assert_eq!(
                run(&settings, &host, &token(HEADER, &bad.to_string())),
                Err(INVALID_CREDENTIALS)
            );
        }
        assert_eq!(host.calls.borrow().len(), 2);
        host.now.set(NOW + CACHE_MS + 1);
        assert_eq!(
            run(&settings, &host, &good),
            Err(AUTHENTICATION_UNAVAILABLE)
        );
        assert_eq!(
            run(&settings, &host, &good),
            Err(AUTHENTICATION_UNAVAILABLE)
        );
        assert_eq!(host.calls.borrow().len(), 3);
    }

    #[test]
    fn refreshed_rotation_admits_new_keys_and_cached_keys_survive_an_offline_provider() {
        let host = Provider::new();
        let settings = settings();
        let old = token(HEADER, &claims().to_string());
        assert!(run(&settings, &host, &old).is_ok());
        let next = KeyRing::from_signing_text("abcdef0123456789abcdef0123456789", None);
        let body = format!(
            "{}.{}",
            to_base64url(br#"{"alg":"EdDSA","kid":"next"}"#),
            to_base64url(claims().to_string().as_bytes())
        );
        let rotated = format!(
            "{body}.{}",
            to_base64url(&sign(body.as_bytes(), &next).expect("signature"))
        );
        host.keys.borrow_mut()["keys"].as_array_mut().expect("keys").push(serde_json::json!({"kty":"OKP","crv":"Ed25519","kid":"next","x":public_key(&next).expect("key")}));
        assert_eq!(run(&settings, &host, &rotated), Err(INVALID_CREDENTIALS));
        assert_eq!(
            host.calls.borrow().len(),
            2,
            "cooldown reserves refresh traffic"
        );
        host.now.set(NOW + REFRESH_MS + 1);
        assert!(run(&settings, &host, &rotated).is_ok());
        assert_eq!(
            host.calls.borrow().len(),
            4,
            "one complete discovery/key refresh"
        );
        host.offline.set(true);
        assert!(run(&settings, &host, &old).is_ok());
        assert!(run(&settings, &host, &rotated).is_ok());
        assert_eq!(
            host.calls.borrow().len(),
            4,
            "valid cached keys do not need the provider"
        );
    }

    #[test]
    fn cancelling_a_refresh_keeps_its_cooldown_and_does_not_clear_a_new_owner() {
        use std::future::Future;
        use std::task::{Context, Poll, Waker};
        let host = Provider::new();
        host.hang.set(true);
        let settings = settings();
        let token = token(HEADER, &claims().to_string());
        let mut old = Box::pin(settings.subject(&host, &token, NOW));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(matches!(old.as_mut().poll(&mut cx), Poll::Pending));
        assert_eq!(host.calls.borrow().len(), 1);
        drop(old);
        assert_eq!(
            run(&settings, &host, &token),
            Err(AUTHENTICATION_UNAVAILABLE)
        );
        assert_eq!(host.calls.borrow().len(), 1);
        host.now.set(NOW + REFRESH_MS + 1);
        let mut old = Box::pin(settings.subject(&host, &token, host.now.get()));
        assert!(matches!(old.as_mut().poll(&mut cx), Poll::Pending));
        host.now.set(NOW + 2 * REFRESH_MS + 2);
        host.hang.set(false);
        assert_eq!(
            run(&settings, &host, &token).as_deref(),
            Ok("provisioned-subject")
        );
        let generation = settings.state.borrow().generation;
        drop(old);
        assert_eq!(settings.state.borrow().generation, generation);
        assert!(!settings.state.borrow().keys.is_empty());
        assert_eq!(settings.state.borrow().busy_until, 0);
    }

    #[test]
    fn signing_keys_keep_standard_rules_while_explicit_encryption_keys_are_ignored() {
        let host = Provider::new();
        let good = token(HEADER, &claims().to_string());
        assert!(run(&settings(), &host, &good).is_ok());
        host.keys.borrow_mut()["keys"]
            .as_array_mut()
            .expect("keys")
            .push(serde_json::json!({"kty":"oct", "alg":"HS256", "k":"private"}));
        assert_eq!(
            run(&settings(), &host, &good),
            Err(AUTHENTICATION_MISCONFIGURED)
        );
        host.keys.borrow_mut()["keys"] = serde_json::json!([]);
        assert_eq!(
            run(&settings(), &host, &good),
            Err(AUTHENTICATION_MISCONFIGURED)
        );
    }
}
