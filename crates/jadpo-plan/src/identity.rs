//! Who can sign in and how (area 8; docs/compiler-runtime.md §2.6): the
//! checked identity declarations as `jadpo-std`'s [`IdentityPlan`], which
//! the runtime's identity component runs for either executor. The one
//! statement each kind of caller is looked up with is written here, from
//! the principal's entity, its `active:` and its lifecycle's `visible:`, so
//! a session is read with its user's record in one primary-key lookup
//! (area 8, question 12).

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
    /// The project's identity plan; `None` when nothing signs anyone in.
    pub(crate) fn identity(&self, db: &dyn Db, project: Project) -> Option<IdentityPlan> {
        let identity = jadpo_check::identity::identity(db, project);
        let store = identity.sessions_store()?.to_string();
        let user = identity
            .variant(PrincipalKind::User)
            .and_then(|variant| self.principal(variant));
        user.as_ref()?;
        Some(IdentityPlan {
            store,
            cookie: identity.cookie().map(str::to_string),
            user,
            service: None,
        })
    }

    fn principal(&self, variant: &jadpo_check::identity::Variant) -> Option<PrincipalPlan> {
        let entity = self.model.entity(&variant.entity)?;
        let subject = entity.fields.iter().find(|f| f.name == variant.subject)?;
        let column = self.column(entity, subject)?;
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
        let mut wheres = vec![format!("{} = s.{}", ident("id"), ident("user_id"))];
        wheres.extend(self.access(&mut bind, entity, Reach::Read)?);
        let session = format!(
            "SELECT s.{id}, s.{csrf}, u.{id}, u.{subject}, u.{active} FROM {sessions} AS s CROSS JOIN LATERAL (SELECT {id}, {column} AS {subject}, {active_sql} AS {active} FROM {table} WHERE {wheres}) AS u WHERE s.{verifier} = $1 AND s.{expires} > $2",
            id = ident("id"),
            csrf = ident("csrf_verifier"),
            subject = ident("subject"),
            active = ident("active"),
            sessions = ident(jadpo_check::data::SESSIONS),
            column = ident(&subject.born),
            active_sql = active,
            table = ident(&entity.table),
            wheres = wheres.join(" AND "),
            verifier = ident("verifier"),
            expires = ident("expires_at"),
        );
        Some(PrincipalPlan {
            entity: entity.name.clone(),
            subject: subject.name.clone(),
            column,
            session,
        })
    }
}
