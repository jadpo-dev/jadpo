//! Who can sign in and how (area 8; docs/compiler-runtime.md §2.6): the
//! checked identity declarations as `jadpo-std`'s [`IdentityPlan`], which
//! the runtime's identity component runs for either executor. The one
//! statement each kind of caller is looked up with is written here, from
//! the principal's entity, its `active:` and its lifecycle's `visible:`, so
//! a session, an API token or a service key is read with its caller's
//! record in one lookup by its verifier (area 8, question 12).

use jadpo_check::policy::Effect;
use jadpo_db::{Db, Project};
use jadpo_graph::TypeRef;
use jadpo_sql::identity as statements;
use jadpo_sql::{Bind, Value};
use jadpo_std::http::{
    GatedStatements, IdentityPlan, MagicLinkPlan, PasskeyPlan, PrincipalKind, PrincipalPlan,
};
use jadpo_std::read::SqlType;

use crate::queries::{Planner, Reach};

/// Binds `clock.now` in a caller's lookup to its `$2`, the time now.
struct Lookup;

impl Bind for Lookup {
    fn bind(&mut self, value: &Value, _ty: SqlType) -> Option<String> {
        matches!(value, Value::Now).then(|| jadpo_sql::placeholder(1))
    }

    /// Written into the statement, whose parameters are fixed (`$1` and
    /// `$2`), as the storage codec made it (chunk 5.5w3b).
    fn stored(&mut self, value: jadpo_std::SqlValue) -> Option<String> {
        jadpo_sql::literal::sql(&value)
    }
}

impl Planner<'_> {
    /// The project's identity plan; `None` when it declares no caller.
    pub(crate) fn identity(&self, db: &dyn Db, project: Project) -> Option<IdentityPlan> {
        let identity = jadpo_check::identity::identity(db, project);
        let sessions = identity.sessions_store().is_some();
        let keys = identity.keys_store().is_some();
        let user = identity.variant(PrincipalKind::User);
        let service = identity.variant(PrincipalKind::Service);
        let store = user.or(service)?.store.clone();
        let mut gated = GatedStatements::default();
        if let Some(entity) = user.and_then(|variant| self.model.entity(&variant.entity))
            && sessions
        {
            let holder = statements::holder();
            gated.tokens = self.gated(entity, Effect::Read, 2, &holder, statements::tokens);
            gated.revoke_token =
                self.gated(entity, Effect::Update, 1, &holder, statements::revoke_token);
        }
        if let Some(entity) = service.and_then(|variant| self.model.entity(&variant.entity))
            && keys
        {
            let holder = statements::holder();
            // A new key's service is its second parameter.
            let new_holder = jadpo_sql::placeholder(1);
            gated.issue_key = self.gated(
                entity,
                Effect::Update,
                5,
                &new_holder,
                statements::issue_key,
            );
            gated.keys = self.gated(entity, Effect::Read, 2, &holder, statements::keys);
            gated.revoke_key =
                self.gated(entity, Effect::Update, 1, &holder, statements::revoke_key);
        }
        Some(IdentityPlan {
            store,
            signs_in: identity.signs_in(),
            cookie: identity.cookie().map(str::to_string),
            api_tokens: identity.built(jadpo_std::AuthenticationMethod::ApiToken),
            external_token: identity.external_token.as_ref().and_then(|external| {
                let user = user?;
                let entity = self.model.entity(&user.entity)?;
                let subject = entity
                    .fields
                    .iter()
                    .find(|field| field.name == user.subject)?;
                let id = self.types.graph().named(
                    jadpo_graph::Kind::Field,
                    &format!("{}.{}", entity.name, subject.name),
                )?;
                Some(jadpo_std::http::ExternalTokenPlan {
                    issuer: external.issuer.clone(),
                    audience: external.audience.clone(),
                    lookup: self.user_lookup(user, Some(&user.subject))?,
                    subject_type: self
                        .json_number(TypeRef::Field(self.types.graph().node(id).key.clone()))?,
                    subject_sql: jadpo_check::data::sql_type(subject)?.param(),
                })
            }),
            user: user.and_then(|variant| self.principal(variant, sessions)),
            service: service.and_then(|variant| self.principal(variant, keys)),
            gated,
            revocation_delay_ms: identity.revocation_delay,
            signing_keys: identity
                .methods
                .iter()
                .filter(|method| method.built)
                .filter_map(|method| Some((method.method, method.signing_key.clone()?)))
                .collect(),
            origin: identity
                .method(jadpo_std::AuthenticationMethod::BrowserSession)
                .and_then(|method| method.origin.clone()),
            passkeys: identity.passkeys().and_then(|passkey| {
                Some(PasskeyPlan {
                    path: passkey.path.clone(),
                    expires_after_ms: passkey.expires_after_ms,
                    lookup: self.passkey_lookup(user?)?,
                    credential: self.json_number(TypeRef::Std(
                        jadpo_std::standard::security::WEB_AUTHN_CREDENTIAL.to_string(),
                        Vec::new(),
                    ))?,
                    address: identity
                        .magic_links()
                        .and_then(|magic| self.address_lookup(user?, &magic.address)),
                })
            }),
            magic_links: identity.magic_links().and_then(|magic| {
                Some(MagicLinkPlan {
                    path: magic.path.clone(),
                    link: magic.link.clone(),
                    expires_after_ms: magic.expires_after_ms,
                    code_attempts: magic.code_attempts,
                    codes_per_hour: magic.codes_per_hour,
                    failed_codes_per_day: magic.failed_codes_per_day,
                    resend_after_ms: magic.resend_after_ms,
                    same_browser: magic.same_browser,
                    lookup: self.user_lookup(user?, Some(&magic.address))?,
                    lookup_id: self.user_lookup(user?, None)?,
                    // Numbered once every callable is (`crate::Plan`).
                    first_sign_in: None,
                })
            }),
        })
    }

    /// How a magic link finds its user, by the field `address` names or,
    /// without one, by its `id` (area 8, "Magic links"): `$1` the address
    /// or the `id`, `$2` the time now.
    fn user_lookup(
        &self,
        variant: &jadpo_check::identity::Variant,
        address: Option<&str>,
    ) -> Option<String> {
        let entity = self.model.entity(&variant.entity)?;
        let column = match address {
            Some(address) => entity
                .fields
                .iter()
                .find(|f| f.name == address)?
                .born
                .clone(),
            None => "id".to_string(),
        };
        let mut bind = Lookup;
        let active = match &variant.active {
            Some(condition) => Some(jadpo_sql::condition(&mut bind, entity, condition, false)?),
            None => None,
        };
        let wheres = self.access(&mut bind, entity, Reach::Read)?;
        Some(statements::user_by(
            &entity.table,
            &column,
            &statements::active(active.as_deref()),
            &wheres,
        ))
    }

    /// A user's address by its `id`, the field `address` names (area 8,
    /// "Passkeys"; chunk 5.5z2): `$1` the `id`, `$2` the time now.
    fn address_lookup(
        &self,
        variant: &jadpo_check::identity::Variant,
        address: &str,
    ) -> Option<String> {
        let entity = self.model.entity(&variant.entity)?;
        let column = entity
            .fields
            .iter()
            .find(|f| f.name == address)?
            .born
            .clone();
        let mut bind = Lookup;
        let wheres = self.access(&mut bind, entity, Reach::Read)?;
        Some(statements::address_of(&entity.table, &column, &wheres))
    }

    /// How a passkey is found by its credential ID with its user's record
    /// (area 8, "Passkeys"): `$1` the credential ID, `$2` the time now.
    fn passkey_lookup(&self, variant: &jadpo_check::identity::Variant) -> Option<String> {
        let entity = self.model.entity(&variant.entity)?;
        let mut bind = Lookup;
        let active = match &variant.active {
            Some(condition) => Some(jadpo_sql::condition(&mut bind, entity, condition, false)?),
            None => None,
        };
        let mut wheres = vec![statements::held_by_credential()];
        wheres.extend(self.access(&mut bind, entity, Reach::Read)?);
        Some(statements::passkey_lookup(
            jadpo_check::data::PASSKEYS,
            &entity.table,
            &statements::active(active.as_deref()),
            &wheres,
        ))
    }

    /// How one kind of caller is made and, when `looked_up` says something
    /// signs it in, looked up: a user by a session's or an API token's
    /// verifier in `jadpo_sessions`, a service by a key's in
    /// `jadpo_service_keys`.
    fn principal(
        &self,
        variant: &jadpo_check::identity::Variant,
        looked_up: bool,
    ) -> Option<PrincipalPlan> {
        let entity = self.model.entity(&variant.entity)?;
        let subject = entity.fields.iter().find(|f| f.name == variant.subject)?;
        let column = self.column(entity, subject)?;
        let mut plan = PrincipalPlan {
            entity: entity.name.clone(),
            store: variant.store.clone(),
            subject: subject.name.clone(),
            column,
            lookup: None,
        };
        if !looked_up {
            return Some(plan);
        }
        let mut bind = Lookup;
        let active = match &variant.active {
            Some(condition) => Some(jadpo_sql::condition(&mut bind, entity, condition, false)?),
            None => None,
        };
        let mut wheres = vec![statements::held_by_credential()];
        wheres.extend(self.access(&mut bind, entity, Reach::Read)?);
        let (table, csrf) = match variant.kind {
            PrincipalKind::User => (jadpo_check::data::SESSIONS, true),
            PrincipalKind::Service => (jadpo_check::data::SERVICE_KEYS, false),
        };
        plan.lookup = Some(statements::caller_lookup(
            table,
            csrf,
            &entity.table,
            &subject.born,
            &statements::active(active.as_deref()),
            &wheres,
        ));
        Some(plan)
    }
}
