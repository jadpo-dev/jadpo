//! The SQL a lifecycle's `purge:` runs (area 5, "Lifecycle"): one batch
//! deletes the oldest hidden records whose `since:` field is older than the
//! run's time less `after:`, at most [`PURGE_BATCH`] of them, as
//! [`jadpo_std::write::PurgePlan`]s the runtime's purge entry point runs.
//! An entity's `retention:` and its fields' (area 15, "Retention"; chunk
//! P2) are planned beside it ([`plans`]): one batch deletes the oldest
//! records, hidden or not, whose `since:` is older than the cutoff, or
//! clears a field's value on them.
//!
//! A record a `restricted` reference holds, or one that removing would
//! cascade to a record a `restricted` reference holds, stays: the batch
//! leaves it out with `NOT EXISTS`, so one held record never stops the
//! rest going. Each chain of cascades is a chain of joins; when one leads
//! back to an entity already on it (a folder inside a folder), a recursive
//! query follows the cascades as deep as the data goes. References that `cascade` and those `cleared` do what the
//! database's foreign keys say, as for any delete. The rows are locked
//! `FOR UPDATE SKIP LOCKED`, so a purge never waits on, or undoes, a
//! request changing one of them.

use jadpo_check::data::retention::RetentionModel;
use jadpo_check::data::{DataModel, EntityModel, FieldModel};
use jadpo_sql::statement::{self as text, Cascade, Hold};
use jadpo_sql::{Bind, Value};
use jadpo_std::OnDelete;
use jadpo_std::read::SqlType;
use jadpo_std::write::{PURGE_BATCH, PurgeKind, PurgeParam, PurgePlan, PurgeSource};

use crate::queries::{column_type, field};

/// A purge statement's parameters.
#[derive(Default)]
struct Purging {
    params: Vec<PurgeParam>,
}

impl Purging {
    fn param(&mut self, source: PurgeSource, ty: SqlType) -> String {
        let param = PurgeParam { source, ty };
        let at = match self.params.iter().position(|p| *p == param) {
            Some(at) => at,
            None => {
                self.params.push(param);
                self.params.len() - 1
            }
        };
        jadpo_sql::placeholder(at)
    }
}

impl Bind for Purging {
    fn bind(&mut self, value: &Value, ty: SqlType) -> Option<String> {
        match value {
            Value::Now => Some(self.param(PurgeSource::Now, ty)),
            _ => None,
        }
    }

    /// A literal or an enum value `visible:` compares with, bound as the
    /// storage codec made it (chunk 5.5w3b).
    fn stored(&mut self, value: jadpo_std::SqlValue) -> Option<String> {
        let ty = value.ty();
        Some(self.param(PurgeSource::Stored(value), ty))
    }
}

/// The runtime's own tables holding the sign-in state of someone who signs
/// in, each with the column holding their `id` (area 5, "Tables for
/// sign-in"): what retention deletes beside their record.
pub(crate) const SIGN_IN_STATE: [(&str, &str); 5] = [
    (jadpo_std::system::SESSIONS.table, "principal_id"),
    (jadpo_std::system::SERVICE_KEYS.table, "principal_id"),
    (jadpo_std::system::PASSKEYS.table, "principal_id"),
    (jadpo_std::system::PASSKEY_CHALLENGES.table, "principal_id"),
    (jadpo_std::system::SIGN_IN_LINKS.table, "user_id"),
];

/// Each entity someone signs in as (the `user:` or `service:` of
/// `principal { }`), with the tables of [`SIGN_IN_STATE`] its store
/// holds: what deleting one of their records deletes with them, for
/// retention (chunk P2) and erasure (chunk P3).
pub(crate) fn sign_in_state(
    db: &dyn jadpo_db::Db,
    project: jadpo_db::Project,
) -> std::collections::BTreeMap<String, Vec<(&'static str, &'static str)>> {
    let identity = jadpo_check::identity::identity(db, project);
    let schema = jadpo_check::data::schema(db, project);
    let model = jadpo_check::data::data_model(db, project);
    model
        .entities
        .iter()
        .filter(|entity| {
            identity
                .variants
                .iter()
                .any(|variant| variant.entity == entity.name)
        })
        .filter_map(|entity| {
            let store = schema.stores.get(entity.store.as_ref()?)?;
            let tables = SIGN_IN_STATE
                .iter()
                .filter(|(table, _)| store.tables.contains_key(*table))
                .copied()
                .collect();
            Some((entity.name.clone(), tables))
        })
        .collect()
}

/// Everything the purge schedule of `entity` runs, in the order a run
/// applies them: its `retention:`, which deletes whole records first, then
/// its lifecycle's `purge:`, then each field's `retention:` in field
/// order. `sign_in` are, for each entity someone signs in as, the tables
/// of [`SIGN_IN_STATE`] its store holds.
pub(crate) fn plans(
    model: &DataModel,
    entity: &EntityModel,
    setting: &dyn Fn(&str) -> Option<u32>,
    sign_in: &SignInState,
) -> Vec<PurgePlan> {
    let mut out = Vec::new();
    if let Some(retention) = &entity.retention
        && let Some(found) = expired(model, entity, retention, setting, sign_in)
    {
        out.push(found);
    }
    out.extend(plan(model, entity, setting));
    for field in &entity.fields {
        if let Some(retention) = &field.retention
            && let Some(found) = cleared(entity, field, retention, setting)
        {
            out.push(found);
        }
    }
    if let Some(found) = audited(entity, setting) {
        out.push(found);
    }
    out
}

/// For each entity someone signs in as, the tables of [`SIGN_IN_STATE`]
/// its store holds, each with the column holding their `id`.
pub(crate) type SignInState = std::collections::BTreeMap<String, Vec<(&'static str, &'static str)>>;

/// `entity`'s `audit: { retention: }` (area 16, "Audit trail"; chunk
/// B11): the oldest entries of its history written before the cutoff, by
/// the entity's born name, which its entries are kept under.
fn audited(entity: &EntityModel, setting: &dyn Fn(&str) -> Option<u32>) -> Option<PurgePlan> {
    let retention = entity.audit.as_ref()?.retention.as_ref()?;
    let store = entity.store.clone()?;
    let mut purging = Purging::default();
    let born = purging.param(
        PurgeSource::Stored(jadpo_std::SqlValue::Text(entity.born.clone())),
        SqlType::Text,
    );
    let cutoff = purging.param(PurgeSource::Cutoff, SqlType::Timestamp);
    Some(PurgePlan {
        entity: entity.name.clone(),
        kind: PurgeKind::Audit,
        store,
        sql: text::audit_purge(&born, &cutoff, PURGE_BATCH),
        params: purging.params,
        after: 0,
        after_setting: Some(setting(&retention.setting)?),
    })
}

/// `entity`'s retention: the oldest records, hidden or not, whose `since:`
/// field is older than the cutoff, deleted with what cascades from them,
/// and with the sign-in state of every record deleted, or reached
/// through a cascade (chunk P4), of an entity someone signs in as. The
/// check refuses a retention a `restricted` reference could hold, so none
/// is left out here; one that did would make its batch fail on its
/// foreign key rather than keep the record unseen.
fn expired(
    model: &DataModel,
    entity: &EntityModel,
    retention: &RetentionModel,
    setting: &dyn Fn(&str) -> Option<u32>,
    sign_in: &SignInState,
) -> Option<PurgePlan> {
    let store = entity.store.clone()?;
    let since = field(entity, &retention.since)?;
    let mut purging = Purging::default();
    let cutoff = purging.param(PurgeSource::Cutoff, column_type(since)?.param());
    let conditions = vec![text::less_than(&since.born, &cutoff)];
    let sql = text::purge(&entity.table, &conditions, &since.born, PURGE_BATCH);
    let (reached, cascades) = cascades_from(model, entity)?;
    let mut removed = Vec::new();
    for (at, name) in reached.iter().enumerate() {
        for (table, column) in sign_in.get(name).into_iter().flatten() {
            removed.push((at, *table, *column));
        }
    }
    // Only the cascades that lead to someone who signs in are followed.
    let leading = leading_to(&cascades, &removed);
    let sql = if removed.is_empty() {
        sql
    } else {
        text::with_removed(&sql, &leading, &removed)
    };
    Some(PurgePlan {
        entity: entity.name.clone(),
        kind: PurgeKind::Retention,
        store,
        sql,
        params: purging.params,
        after: 0,
        after_setting: Some(setting(&retention.setting)?),
    })
}

/// The entities deleting a record of `entity` cascades to, its own first,
/// each numbered by its place, and each cascading reference followed, as
/// the walk of [`not_held_anywhere`] numbers them.
fn cascades_from<'m>(
    model: &'m DataModel,
    entity: &EntityModel,
) -> Option<(Vec<String>, Vec<Cascade<'m>>)> {
    let mut reached: Vec<String> = vec![entity.name.clone()];
    let mut cascades: Vec<Cascade<'m>> = Vec::new();
    let mut at = 0;
    while let Some(current) = reached.get(at).cloned() {
        for (child, field_name, on_delete) in referring(model, &current) {
            if on_delete != Some(OnDelete::Cascade) {
                continue;
            }
            let holder = model.entity(&child)?;
            let number = match reached.iter().position(|name| *name == child) {
                Some(number) => number,
                None => {
                    reached.push(child.clone());
                    reached.len() - 1
                }
            };
            cascades.push(Cascade {
                from: at,
                to: number,
                table: holder.table.as_str(),
                column: field(holder, &field_name)?.born.as_str(),
            });
        }
        at += 1;
    }
    Some((reached, cascades))
}

/// The cascades of `cascades` on a way to one of the entities `removed`
/// numbers: each one whose end leads, through cascades, to one of them.
fn leading_to<'m>(cascades: &[Cascade<'m>], removed: &[(usize, &str, &str)]) -> Vec<Cascade<'m>> {
    let mut wanted: std::collections::BTreeSet<usize> =
        removed.iter().map(|(entity, ..)| *entity).collect();
    loop {
        let before = wanted.len();
        for cascade in cascades {
            if wanted.contains(&cascade.to) {
                wanted.insert(cascade.from);
            }
        }
        if wanted.len() == before {
            break;
        }
    }
    cascades
        .iter()
        .filter(|cascade| wanted.contains(&cascade.to))
        .cloned()
        .collect()
}

/// `field`'s retention: its value cleared on the oldest records, hidden or
/// not, that hold one and whose `since:` field is older than the cutoff,
/// every `Generated.on_change` field set to the run's time, since the
/// record changes.
fn cleared(
    entity: &EntityModel,
    field_model: &FieldModel,
    retention: &RetentionModel,
    setting: &dyn Fn(&str) -> Option<u32>,
) -> Option<PurgePlan> {
    let store = entity.store.clone()?;
    let since = field(entity, &retention.since)?;
    let mut purging = Purging::default();
    let cutoff = purging.param(PurgeSource::Cutoff, column_type(since)?.param());
    let columns: Vec<String> = field_model
        .columns()
        .into_iter()
        .map(|(column, _)| column)
        .collect();
    if columns.is_empty() {
        return None;
    }
    let mut set: Vec<(String, String)> = Vec::new();
    for changed in &entity.fields {
        if changed.generated == Some(jadpo_std::Generated::OnChange) {
            let now = purging.param(PurgeSource::Now, column_type(changed)?.param());
            set.push((changed.born.clone(), now));
        }
    }
    let held: Vec<&str> = columns.iter().map(String::as_str).collect();
    let conditions = vec![
        text::any_present(&held),
        text::less_than(&since.born, &cutoff),
    ];
    let sql = text::clear(
        &entity.table,
        &columns,
        &set,
        &conditions,
        &since.born,
        PURGE_BATCH,
    );
    Some(PurgePlan {
        entity: entity.name.clone(),
        kind: PurgeKind::FieldRetention(field_model.name.clone()),
        store,
        sql,
        params: purging.params,
        after: 0,
        after_setting: Some(setting(&retention.setting)?),
    })
}

/// `entity`'s purge, when its lifecycle has one.
pub(crate) fn plan(
    model: &DataModel,
    entity: &EntityModel,
    setting: &dyn Fn(&str) -> Option<u32>,
) -> Option<PurgePlan> {
    let lifecycle = entity.lifecycle.as_ref()?;
    let purge = lifecycle.purge.as_ref()?;
    let visible = lifecycle.visible.as_ref()?;
    let store = entity.store.clone()?;
    let since = field(entity, &purge.since)?;
    let mut purging = Purging::default();
    let cutoff = purging.param(PurgeSource::Cutoff, column_type(since)?.param());
    let mut conditions = vec![
        // Hidden: `visible:` doesn't hold.
        jadpo_sql::condition(&mut purging, entity, visible, true)?,
        text::less_than(&since.born, &cutoff),
    ];
    match held_by(model, &entity.name) {
        Some(paths) => {
            for path in paths {
                conditions.push(not_held(model, entity, &path)?);
            }
        }
        None => conditions.push(not_held_anywhere(model, entity)?),
    }
    let sql = text::purge(&entity.table, &conditions, &since.born, PURGE_BATCH);
    Some(PurgePlan {
        entity: entity.name.clone(),
        kind: PurgeKind::Hidden,
        store,
        sql,
        params: purging.params,
        after: i64::try_from(purge.after).ok()?,
        after_setting: match &purge.setting {
            Some(name) => Some(setting(name)?),
            None => None,
        },
    })
}

/// Each way a `restricted` reference can hold a record of `entity`: the
/// references followed from it, each `(holder, field)`, through
/// references that cascade, ending at one that is restricted. Every path
/// is kept, so a record reached two ways is checked both ways; `None` when
/// a cascade leads back to an entity already on its path (a folder inside
/// a folder), which no fixed number of joins follows.
fn held_by(model: &DataModel, entity: &str) -> Option<Vec<Vec<(String, String)>>> {
    let mut out = Vec::new();
    let mut stack: Vec<(String, Vec<(String, String)>)> = vec![(entity.to_string(), Vec::new())];
    while let Some((current, path)) = stack.pop() {
        for (child, field_name, on_delete) in referring(model, &current) {
            let mut next = path.clone();
            next.push((child.clone(), field_name));
            match on_delete {
                Some(OnDelete::Restricted) | None => out.push(next),
                Some(OnDelete::Cascade) => {
                    let looped = child == entity || path.iter().any(|(holder, _)| *holder == child);
                    if looped {
                        return None;
                    }
                    stack.push((child, next));
                }
                Some(OnDelete::Cleared) => {}
            }
        }
    }
    Some(out)
}

/// Each reference to `target`: its entity, its field and what deleting
/// the target does to it.
fn referring(model: &DataModel, target: &str) -> Vec<(String, String, Option<OnDelete>)> {
    let mut out = Vec::new();
    for child in &model.entities {
        for field in &child.fields {
            if let Some(reference) = &field.reference
                && reference.target == target
            {
                out.push((child.name.clone(), field.name.clone(), reference.on_delete));
            }
        }
    }
    out
}

/// `NOT EXISTS` a record a `restricted` reference holds among those
/// deleting this one would cascade to, however deep and wherever the
/// cascades lead back: a recursive query walks every cascading reference
/// from the record (each record once, so a loop in the data ends too),
/// tagging each record it reaches with its entity's number, then asks
/// whether any restricted reference holds one.
fn not_held_anywhere(model: &DataModel, entity: &EntityModel) -> Option<String> {
    // The entities deleting one cascades to, the record's own first.
    let mut reached: Vec<String> = vec![entity.name.clone()];
    let mut cascades: Vec<Cascade<'_>> = Vec::new();
    let mut holds: Vec<Hold<'_>> = Vec::new();
    let mut at = 0;
    while let Some(current) = reached.get(at).cloned() {
        for (child, field_name, on_delete) in referring(model, &current) {
            let holder = model.entity(&child)?;
            let column = field(holder, &field_name)?.born.as_str();
            let table = holder.table.as_str();
            match on_delete {
                Some(OnDelete::Cascade) => {
                    let number = match reached.iter().position(|name| *name == child) {
                        Some(number) => number,
                        None => {
                            reached.push(child.clone());
                            reached.len() - 1
                        }
                    };
                    cascades.push(Cascade {
                        from: at,
                        to: number,
                        table,
                        column,
                    });
                }
                Some(OnDelete::Restricted) | None => holds.push(Hold {
                    entity: at,
                    table,
                    column,
                }),
                Some(OnDelete::Cleared) => {}
            }
        }
        at += 1;
    }
    Some(text::not_held_anywhere(&entity.table, &cascades, &holds))
}

/// `NOT EXISTS` a record holding this one through `path`.
fn not_held(model: &DataModel, entity: &EntityModel, path: &[(String, String)]) -> Option<String> {
    let mut steps = Vec::new();
    for (holder, field_name) in path {
        let holder = model.entity(holder)?;
        steps.push((
            holder.table.as_str(),
            field(holder, field_name)?.born.as_str(),
        ));
    }
    Some(text::not_held(&entity.table, &steps))
}
