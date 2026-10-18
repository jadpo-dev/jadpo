//! Who can sign in and how (area 8; docs/compiler-runtime.md §2.6): the
//! checked identity declarations as `jadpo-std`'s [`IdentityPlan`], which
//! the runtime's identity component runs for either executor. The one
//! statement each kind of caller is looked up with is written here, from
//! the principal's entity, its `active:` and its lifecycle's `visible:`, so
//! a session, an API token or a service key is read with its caller's
//! record in one lookup by its verifier (area 8, question 12).

use jadpo_check::policy::Effect;
use jadpo_db::{Db, Project};
use jadpo_sql::{Bind, Value, ident};
use jadpo_std::http::{GatedStatements, IdentityPlan, PasskeyPlan, PrincipalKind, PrincipalPlan};
use jadpo_std::read::SqlType;

use crate::queries::{Planner, Reach};

/// A user's API tokens that haven't ended, oldest first, whose user the
/// caller may read.
const TOKENS: &str = "SELECT \"id\", \"started_at\", \"ends_at\" FROM \"jadpo_sessions\" WHERE \"principal_id\" = $1 AND \"csrf_verifier\" IS NULL AND \"ends_at\" > $2 AND {held} ORDER BY \"started_at\", \"id\"";

/// Ends one API token, never a browser session, whose user the caller may
/// update.
const REVOKE_TOKEN: &str =
    "DELETE FROM \"jadpo_sessions\" WHERE \"id\" = $1 AND \"csrf_verifier\" IS NULL AND {held}";

/// Keeps a new service key, for a service the caller may update.
const ISSUE_KEY: &str = "INSERT INTO \"jadpo_service_keys\" (\"id\", \"principal_id\", \"verifier\", \"started_at\", \"ends_at\") SELECT $1, $2, $3, $4, $5 WHERE {held}";

/// A service's keys that haven't expired, oldest first, whose service the
/// caller may read.
const KEYS: &str = "SELECT \"id\", \"started_at\", \"ends_at\" FROM \"jadpo_service_keys\" WHERE \"principal_id\" = $1 AND \"ends_at\" > $2 AND {held} ORDER BY \"started_at\", \"id\"";

/// Deletes one service key, whose service the caller may update.
const REVOKE_KEY: &str = "DELETE FROM \"jadpo_service_keys\" WHERE \"id\" = $1 AND {held}";

/// Binds `clock.now` in a caller's lookup to its `$2`, the time now.
struct Lookup;

impl Bind for Lookup {
    fn bind(&mut self, value: &Value, _ty: SqlType) -> Option<String> {
        matches!(value, Value::Now).then(|| "$2".to_string())
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
            gated.tokens = self.gated(entity, Effect::Read, 2, "\"principal_id\"", TOKENS);
            gated.revoke_token =
                self.gated(entity, Effect::Update, 1, "\"principal_id\"", REVOKE_TOKEN);
        }
        if let Some(entity) = service.and_then(|variant| self.model.entity(&variant.entity))
            && keys
        {
            gated.issue_key = self.gated(entity, Effect::Update, 5, "$2", ISSUE_KEY);
            gated.keys = self.gated(entity, Effect::Read, 2, "\"principal_id\"", KEYS);
            gated.revoke_key =
                self.gated(entity, Effect::Update, 1, "\"principal_id\"", REVOKE_KEY);
        }
        Some(IdentityPlan {
            store,
            signs_in: identity.signs_in(),
            cookie: identity.cookie().map(str::to_string),
            api_tokens: identity.built(jadpo_std::AuthenticationMethod::ApiToken),
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
                })
            }),
        })
    }

    /// How a passkey is found by its credential ID with its user's record
    /// (area 8, "Passkeys"): `$1` the credential ID, `$2` the time now.
    fn passkey_lookup(&self, variant: &jadpo_check::identity::Variant) -> Option<String> {
        let entity = self.model.entity(&variant.entity)?;
        let mut bind = Lookup;
        let active = match &variant.active {
            Some(condition) => format!(
                "({}) IS TRUE",
                jadpo_sql::condition(&mut bind, entity, condition, false)?
            ),
            None => "TRUE".to_string(),
        };
        let mut wheres = vec![format!("{} = s.{}", ident("id"), ident("principal_id"))];
        wheres.extend(self.access(&mut bind, entity, Reach::Read)?);
        Some(format!(
            "SELECT s.{id}, s.{credential}, s.{handle}, u.{id}, u.{active} FROM {table} AS s CROSS JOIN LATERAL (SELECT {id}, {active_sql} AS {active} FROM {entity_table} WHERE {wheres}) AS u WHERE s.{credential_id} = $1 AND $2::timestamptz IS NOT NULL",
            id = ident("id"),
            credential = ident("credential"),
            handle = ident("user_handle"),
            active = ident("active"),
            table = ident(jadpo_check::data::PASSKEYS),
            active_sql = active,
            entity_table = ident(&entity.table),
            wheres = wheres.join(" AND "),
            credential_id = ident("credential_id"),
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
            Some(condition) => {
                format!(
                    "({}) IS TRUE",
                    jadpo_sql::condition(&mut bind, entity, condition, false)?
                )
            }
            None => "TRUE".to_string(),
        };
        let mut wheres = vec![format!("{} = s.{}", ident("id"), ident("principal_id"))];
        wheres.extend(self.access(&mut bind, entity, Reach::Read)?);
        let (table, csrf) = match variant.kind {
            PrincipalKind::User => (
                jadpo_check::data::SESSIONS,
                format!("s.{}, ", ident("csrf_verifier")),
            ),
            PrincipalKind::Service => (jadpo_check::data::SERVICE_KEYS, String::new()),
        };
        plan.lookup = Some(format!(
            "SELECT s.{id}, {csrf}u.{id}, u.{subject}, u.{active}, s.{ends} FROM {table} AS s CROSS JOIN LATERAL (SELECT {id}, {column} AS {subject}, {active_sql} AS {active} FROM {entity_table} WHERE {wheres}) AS u WHERE s.{verifier} = $1 AND s.{ends} > $2",
            id = ident("id"),
            subject = ident("subject"),
            active = ident("active"),
            table = ident(table),
            column = ident(&subject.born),
            active_sql = active,
            entity_table = ident(&entity.table),
            wheres = wheres.join(" AND "),
            verifier = ident("verifier"),
            ends = ident("ends_at"),
        ));
        Some(plan)
    }
}
