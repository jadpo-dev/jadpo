//! The audit trail's plans (area 16, "Audit trail"; chunk B11), which
//! both executors run through the runtime: what each write of an audited
//! entity records beside its statement ([`Planner::audit`]), and what an
//! audited entity's `history` reads ([`histories`]). The retention of its
//! entries is a purge plan ([`crate::purge`]), and a person's erasure
//! deletes the entries of every record it deletes
//! ([`crate::writes`]).

use jadpo_check::data::EntityModel;
use jadpo_check::lookups::Lookup;
use jadpo_check::policy::Effect;
use jadpo_graph::{Key, TypeRef};
use jadpo_sql::statement as text;
use jadpo_std::audit::{AuditBefore, AuditPlan, AuditedField, HistoryPlan};
use jadpo_std::read::{Source, SqlType};
use jadpo_std::write::{WriteOp, WriteSource};

use crate::policy::Row;
use crate::queries::{Planner, Reach, Statement, tag};
use crate::types::Types;

impl Planner<'_> {
    /// What a write of `op` to `entity` records, when it is audited: the
    /// record before it, read by the `id` from `id` for an update or a
    /// delete, and the fields an entry holds. `Some(None)` when `entity`
    /// keeps no history; `None` when a field can't be read back.
    pub(crate) fn audit(
        &mut self,
        entity: &EntityModel,
        op: WriteOp,
        transition: bool,
        id: Option<WriteSource>,
    ) -> Option<Option<AuditPlan>> {
        if entity.audit.is_none() {
            return Some(None);
        }
        let policy = self.access.entity(&entity.name);
        let mut fields = Vec::new();
        for field in &entity.fields {
            // The record's `id` is the entry's, and a field no request may
            // touch is left out of its history (area 16).
            let server_only = policy
                .and_then(|policy| policy.field(&field.name))
                .is_some_and(|field| field.server_only);
            if field.name == "id" || server_only {
                continue;
            }
            fields.push(AuditedField {
                name: field.name.clone(),
                json: self.json_codec(entity, field),
                sealed: field.sealed.is_some(),
            });
        }
        let columns: Vec<String> = entity
            .fields
            .iter()
            .flat_map(|field| field.columns().into_iter().map(|(column, _)| column))
            .collect();
        // A lifecycle's `delete` runs its transition, which keeps the
        // record: locked as an update locks it, and read again after.
        let removes = op == WriteOp::Delete && !transition;
        let read = |planner: &mut Self, id: &WriteSource| -> Option<AuditBefore> {
            Some(AuditBefore {
                sql: text::audit_before(&columns, &entity.table, removes),
                id: id.clone(),
                returned: planner.returned(entity)?,
            })
        };
        let (before, after) = match (op, &id) {
            (WriteOp::Create, _) | (_, None) => (None, None),
            (WriteOp::Delete, Some(id)) if transition => {
                (Some(read(self, id)?), Some(read(self, id)?))
            }
            (_, Some(id)) => (Some(read(self, id)?), None),
        };
        Some(Some(AuditPlan {
            entity: entity.name.clone(),
            born: entity.born.clone(),
            before,
            after,
            fields,
            subscribers: Vec::new(),
        }))
    }

    /// `entity`'s `history`, when it is audited: whether the caller may
    /// read the record, under its lifecycle and its `read` grant, and each
    /// field whose own `policy:` narrows who reads it.
    fn history(&mut self, entity: &EntityModel) -> Option<HistoryPlan> {
        entity.audit.as_ref()?;
        let store = entity.store.clone()?;
        let mut statement = Statement::default();
        let id = statement.param(
            Source::Arg {
                index: 0,
                path: Vec::new(),
            },
            SqlType::Uuid,
        );
        let mut conditions = vec![text::equal("id", &id)];
        conditions.extend(self.access(&mut statement, entity, Reach::Read)?);
        let granted = self.policy(&mut statement, entity, Effect::Read, &Row::Table)?;
        if granted != jadpo_sql::policy::ANYONE {
            conditions.push(granted);
        }
        let mut grants = Vec::new();
        let mut restricted = Vec::new();
        let policy = self.access.entity(&entity.name);
        for field in &entity.fields {
            // A field no request may touch is never kept in a history.
            let server_only = policy
                .and_then(|policy| policy.field(&field.name))
                .is_some_and(|field| field.server_only);
            if field.name == "id" || server_only {
                continue;
            }
            if let Some(grant) = self.field_policy(
                &mut statement,
                entity,
                &field.name,
                Effect::Read,
                &Row::Table,
            )? {
                grants.push(grant);
                restricted.push(field.name.clone());
            }
        }
        let mut access = text::history_access(&entity.table, &grants);
        text::where_clause(&mut access, &conditions, "");
        Some(HistoryPlan {
            store,
            born: entity.born.clone(),
            access,
            params: statement.into_params(),
            restricted,
            tag: tag(&Key::from_text(format!("history {}", entity.born))),
        })
    }
}

/// Each audited entity's `history`, by the entity's key, in key order.
pub(crate) fn histories(
    db: &dyn jadpo_db::Db,
    project: jadpo_db::Project,
    types: &mut Types,
    json: &std::cell::RefCell<Vec<TypeRef>>,
) -> Vec<(Key, HistoryPlan)> {
    let model = jadpo_check::data::data_model(db, project);
    let lookup = Lookup::new(db, project);
    let access = jadpo_check::policy::access(db, project);
    let mut planner = Planner {
        model,
        lookup: &lookup,
        types,
        access,
        action: None,
        json,
        sign_in: Default::default(),
    };
    let mut out: Vec<(Key, HistoryPlan)> = model
        .entities
        .iter()
        .filter_map(|entity| {
            let key = planner.types.named(&entity.name)?;
            Some((key, planner.history(entity)?))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// The identity keys of the subscribers `AuditEntryRecorded` is delivered
/// to, in key order: an application's own, since no package's subscriber
/// hears an event its app emits.
pub(crate) fn subscribers(db: &dyn jadpo_db::Db, project: jadpo_db::Project) -> Vec<String> {
    let mut out: Vec<String> = jadpo_check::events::subscribers(db, project)
        .iter()
        .filter(|model| {
            model.event.as_str() == jadpo_std::audit::AUDIT_EVENT_KEY && model.package.is_none()
        })
        .map(|model| model.key.as_str().to_string())
        .collect();
    out.sort();
    out
}
