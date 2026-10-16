//! Who can sign in and how (area 8; docs/compiler-runtime.md §2.6): the
//! checked identity declarations as `jadpo-std`'s [`IdentityPlan`], which
//! the runtime's identity component runs for either executor. The one
//! statement each kind of caller is looked up with is written here, from
//! the principal's entity, its `active:` and its lifecycle's `visible:`, so
//! a session, an API token or a service key is read with its caller's
//! record in one lookup by its verifier (area 8, question 12).

use jadpo_db::{Db, Project};
use jadpo_sql::{Bind, Value, ident};
use jadpo_std::http::{IdentityPlan, PrincipalKind, PrincipalPlan};
use jadpo_std::read::SqlType;

use crate::queries::{Planner, Reach};

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
        Some(IdentityPlan {
            store,
            signs_in: identity.signs_in(),
            cookie: identity.cookie().map(str::to_string),
            api_tokens: identity.built(jadpo_std::AuthenticationMethod::ApiToken),
            user: user.and_then(|variant| self.principal(variant, sessions)),
            service: service.and_then(|variant| self.principal(variant, keys)),
        })
    }

    /// How one kind of caller is made and, when `signed_in` says something
    /// signs it in, looked up: a user by a session's or an API token's
    /// verifier in `jadpo_sessions`, a service by a key's in
    /// `jadpo_service_keys`.
    fn principal(
        &self,
        variant: &jadpo_check::identity::Variant,
        signed_in: bool,
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
        if !signed_in {
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
            "SELECT s.{id}, {csrf}u.{id}, u.{subject}, u.{active} FROM {table} AS s CROSS JOIN LATERAL (SELECT {id}, {column} AS {subject}, {active_sql} AS {active} FROM {entity_table} WHERE {wheres}) AS u WHERE s.{verifier} = $1 AND s.{ends} > $2",
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
