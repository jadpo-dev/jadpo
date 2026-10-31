//! The local issuer (area 8, "External tokens in tests and `jadpo dev`";
//! chunk A3): what tests and `jadpo dev` run in the configured
//! `external_token` issuer's place. Its key is made for the run, its
//! tokens carry the configured issuer and audience, and nothing is
//! fetched. Only a host that holds one ([`crate::Host::local_issuer`])
//! accepts its tokens; it is built only with the `testing` feature, so a
//! deployable holds none (`tools/checks/deployable.py`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use jadpo_value::Value;

/// The key id a local issuer's tokens name.
pub const LOCAL_KEY_ID: &str = "jadpo-local";

/// How long a local issuer's token signs in for: an hour, in seconds.
pub const LOCAL_TOKEN_SECONDS: i64 = 3_600;

/// Where a sign-in provider's routes send the browser and exchange codes
/// while the local issuer stands in for the provider (area 8,
/// "Providers"; chunk A7b): addresses no network has, which only the
/// local issuer answers, in process.
pub const AUTHORIZE: &str = "https://local-issuer.invalid/authorize";
pub const TOKEN: &str = "https://local-issuer.invalid/token";

/// How long a code the local issuer gives works: a minute, in
/// milliseconds.
const CODE_MS: i64 = 60_000;

/// What a provider says about the person it signs in: what a route
/// test's `TestAccount` holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Account {
    pub subject: String,
    pub email: Option<String>,
    pub email_verified: bool,
    pub name: Option<String>,
    /// The company the account belongs to, which its ID token names as
    /// Google's `hd` and Microsoft's `tid` (chunk A10).
    pub organisation: Option<String>,
    /// Whether the person signed in with a second factor, which its ID
    /// token's `amr` says.
    pub multi_factor: bool,
}

/// What a code the local issuer gave was given for.
struct Grant {
    account: Account,
    client_id: String,
    redirect_uri: String,
    nonce: String,
    challenge: String,
    expires: i64,
}

/// The codes a local issuer gave and hasn't been asked to exchange, by
/// code, and how many it has given. `jadpo dev`'s account-chooser page
/// gives codes on a thread of its own and the application's workers
/// exchange them on theirs (chunk A7c), so every local issuer made from
/// one key shares one set.
#[derive(Clone, Default)]
pub struct Codes(Arc<Mutex<Granted>>);

impl std::fmt::Debug for Codes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Codes")
    }
}

#[derive(Default)]
struct Granted {
    grants: HashMap<String, Grant>,
    count: u64,
}

impl Codes {
    fn lock(&self) -> std::sync::MutexGuard<'_, Granted> {
        match self.0.lock() {
            Ok(granted) => granted,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

/// The issuer tests and `jadpo dev` run in the configured one's place: an
/// Ed25519 key made for the run, and its public key as the one `Jwk` its
/// tokens are checked against. Its `Debug` never shows the key.
pub struct LocalIssuer {
    /// The key's 32 bytes as unpadded Base64url, which `jadpo dev` keeps
    /// for `jadpo dev token` while it runs.
    seed: String,
    ring: jadpo_value::crypto::KeyRing,
    keys: [Value; 1],
    codes: Codes,
    /// Where `jadpo dev` serves the account-chooser page (chunk A7c), the
    /// provider's sign-in address in its place; a test's issuer has none
    /// and answers in process at [`AUTHORIZE`].
    page: Option<String>,
}

impl std::fmt::Debug for LocalIssuer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LocalIssuer")
    }
}

impl LocalIssuer {
    /// A local issuer with a key made from `random`'s bytes; `None` when
    /// it gives none.
    pub fn new(random: &dyn Fn(&mut [u8]) -> bool) -> Option<LocalIssuer> {
        let mut bytes = [0u8; jadpo_std::KEY_SIZE];
        if !random(&mut bytes) {
            return None;
        }
        LocalIssuer::from_seed(&jadpo_value::encoding::to_base64url(&bytes))
    }

    /// The local issuer whose key is `seed`, 32 bytes as unpadded
    /// Base64url, as [`LocalIssuer::seed`] gives it.
    pub fn from_seed(seed: &str) -> Option<LocalIssuer> {
        let ring = jadpo_value::crypto::KeyRing::from_key_text(seed, None)?;
        let public = jadpo_value::crypto::public_key(&ring).ok()?;
        let key = jadpo_value::jwt::read_jwk(&serde_json::json!({
            "kty": "OKP",
            "crv": "Ed25519",
            "x": public,
            "kid": LOCAL_KEY_ID,
            "alg": "EdDSA",
            "use": "sig",
        }))?;
        Some(LocalIssuer {
            seed: seed.to_string(),
            ring,
            keys: [key],
            codes: Codes::default(),
            page: None,
        })
    }

    /// This issuer, giving and taking codes from `codes`, which another
    /// made from the same key shares.
    pub fn with_codes(mut self, codes: Codes) -> LocalIssuer {
        self.codes = codes;
        self
    }

    /// The codes it gives and takes.
    pub fn codes(&self) -> &Codes {
        &self.codes
    }

    /// This issuer, sending the browser to the account-chooser page at
    /// `page` (`http://localhost:<port>`) to sign in (chunk A7c).
    pub fn with_page(mut self, page: &str) -> LocalIssuer {
        self.page = Some(page.trim_end_matches('/').to_string());
        self
    }

    /// The sign-in address of the provider `provider`: its page on the
    /// account-chooser, or [`AUTHORIZE`], answered in process.
    pub fn authorization(&self, provider: &str) -> String {
        match &self.page {
            Some(page) => format!("{page}/{provider}/authorize"),
            None => AUTHORIZE.to_string(),
        }
    }

    /// The key, as [`LocalIssuer::from_seed`] reads it back.
    pub fn seed(&self) -> &str {
        &self.seed
    }

    /// A token for `subject` from `issuer` to `audience`, issued at `now`
    /// (milliseconds) and signing in for [`LOCAL_TOKEN_SECONDS`].
    pub fn token(&self, issuer: &str, audience: &str, subject: &str, now: i64) -> Option<String> {
        self.token_with(issuer, audience, subject, now, false)
    }

    /// [`LocalIssuer::token`], its `amr` naming a second factor when
    /// `multi_factor` (chunk A10).
    pub fn token_with(
        &self,
        issuer: &str,
        audience: &str,
        subject: &str,
        now: i64,
        multi_factor: bool,
    ) -> Option<String> {
        use jadpo_value::encoding::to_base64url;
        let issued = now.div_euclid(1000);
        let header = serde_json::json!({"alg": "EdDSA", "kid": LOCAL_KEY_ID, "typ": "JWT"});
        let mut claims = serde_json::json!({
            "iss": issuer,
            "aud": audience,
            "sub": subject,
            "iat": issued,
            "exp": issued.saturating_add(LOCAL_TOKEN_SECONDS),
        });
        if multi_factor && let Some(object) = claims.as_object_mut() {
            object.insert("amr".into(), serde_json::json!(["pwd", "mfa"]));
        }
        let body = format!(
            "{}.{}",
            to_base64url(header.to_string().as_bytes()),
            to_base64url(claims.to_string().as_bytes())
        );
        let signature = jadpo_value::crypto::sign(body.as_bytes(), &self.ring).ok()?;
        Some(format!("{body}.{}", to_base64url(&signature)))
    }

    pub(crate) fn keys(&self) -> &[Value] {
        &self.keys
    }

    /// Signs `account` in as a provider's authorization endpoint does
    /// (chunk A7b): a code for `client_id` coming back to `redirect_uri`,
    /// carrying `nonce` and bound to the PKCE `challenge`, working once,
    /// for a minute from `now` (milliseconds). The code is keyed by the
    /// issuer's own key, so no one else can guess one.
    pub fn authorize(
        &self,
        account: Account,
        client_id: &str,
        redirect_uri: &str,
        nonce: &str,
        challenge: &str,
        now: i64,
    ) -> String {
        let mut granted = self.codes.lock();
        granted.count = granted.count.wrapping_add(1);
        let key = blake3::derive_key("jadpo local issuer codes", self.seed.as_bytes());
        let code = jadpo_value::encoding::to_base64url(
            blake3::keyed_hash(&key, &granted.count.to_be_bytes()).as_bytes(),
        );
        granted.grants.retain(|_, grant| grant.expires > now);
        granted.grants.insert(
            code.clone(),
            Grant {
                account,
                client_id: client_id.to_string(),
                redirect_uri: redirect_uri.to_string(),
                nonce: nonce.to_string(),
                challenge: challenge.to_string(),
                expires: now.saturating_add(CODE_MS),
            },
        );
        code
    }

    /// Exchanges `code` as a provider's token endpoint does: once, for the
    /// client and the address it was given for, with the PKCE `verifier`
    /// its challenge was made from; the ID token from `issuer` naming the
    /// account and the nonce. `None` for any code that doesn't hold.
    pub fn redeem(
        &self,
        code: &str,
        verifier: &str,
        client_id: &str,
        redirect_uri: &str,
        issuer: &str,
        now: i64,
    ) -> Option<String> {
        use jadpo_value::encoding::to_base64url;
        let grant = self.codes.lock().grants.remove(code)?;
        let challenge = to_base64url(&jadpo_value::hash::sha256(verifier.as_bytes()));
        if grant.client_id != client_id
            || grant.redirect_uri != redirect_uri
            || grant.challenge != challenge
            || grant.expires <= now
        {
            return None;
        }
        let issued = now.div_euclid(1000);
        let mut claims = serde_json::json!({
            "iss": issuer,
            "aud": client_id,
            "sub": grant.account.subject,
            "nonce": grant.nonce,
            "email_verified": grant.account.email_verified,
            "iat": issued,
            "exp": issued.saturating_add(LOCAL_TOKEN_SECONDS),
        });
        if let Some(object) = claims.as_object_mut() {
            if let Some(email) = &grant.account.email {
                object.insert("email".into(), email.clone().into());
            }
            if let Some(name) = &grant.account.name {
                object.insert("name".into(), name.clone().into());
            }
            if let Some(organisation) = &grant.account.organisation {
                object.insert("hd".into(), organisation.clone().into());
                object.insert("tid".into(), organisation.clone().into());
            }
            let amr: &[&str] = if grant.account.multi_factor {
                &["pwd", "mfa"]
            } else {
                &["pwd"]
            };
            object.insert("amr".into(), amr.into());
        }
        let header = serde_json::json!({"alg": "EdDSA", "kid": LOCAL_KEY_ID, "typ": "JWT"});
        let body = format!(
            "{}.{}",
            to_base64url(header.to_string().as_bytes()),
            to_base64url(claims.to_string().as_bytes())
        );
        let signature = jadpo_value::crypto::sign(body.as_bytes(), &self.ring).ok()?;
        Some(format!("{body}.{}", to_base64url(&signature)))
    }
}

/// The store `jadpo dev token` reads a user's subject from: the user
/// entity's, when the program signs in with `external_token`.
pub fn dev_store<'p>(plan: &crate::Plan<'p>) -> Option<&'p str> {
    let identity = plan.routes?.identity.as_ref()?;
    identity.external_token.as_ref()?;
    Some(identity.user.as_ref()?.store.as_str())
}

/// `jadpo dev token <user-id>` (area 8, "External tokens in tests and
/// `jadpo dev`"; chunk A3): the subject user `user` holds now, as a token
/// sends it, read from `database`, the user entity's store; `None` for a
/// user the lifecycle doesn't show, or none.
pub async fn dev_subject(
    plan: &crate::Plan<'_>,
    database: &dyn crate::Database,
    user: [u8; 16],
    now: i64,
) -> Result<Option<String>, crate::StoreError> {
    let refused = |why: &str| crate::StoreError::Fault(why.to_string());
    let identity = plan
        .routes
        .and_then(|routes| routes.identity.as_ref())
        .ok_or_else(|| refused("the program signs no one in"))?;
    let (Some(external), Some(principal)) = (&identity.external_token, &identity.user) else {
        return Err(refused("the program takes no external token"));
    };
    let statement = external
        .subject_of
        .as_deref()
        .ok_or_else(|| refused("the plan has no subject statement"))?;
    let rows = database
        .query_once(
            statement,
            &[
                crate::SqlValue::Uuid(user),
                crate::SqlValue::Timestamp(now.saturating_mul(1000)),
            ],
        )
        .await?;
    let Some([sql]) = rows.first().map(Vec::as_slice) else {
        return Ok(None);
    };
    let value = crate::from_sql(sql, &principal.column).map_err(|why| refused(&why))?;
    Ok(super::external_token::subject_text(
        &plan.json, external, &value,
    ))
}
