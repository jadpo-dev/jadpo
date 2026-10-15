//! The SQL a lifecycle's `purge:` runs (area 5, "Lifecycle"): one batch
//! deletes the oldest hidden records whose `since:` field is older than the
//! run's time less `after:`, at most [`PURGE_BATCH`] of them, as
//! [`jadpo_std::write::PurgePlan`]s the runtime's purge entry point runs.
//!
//! A record a `restricted` reference holds, or one that removing would
//! cascade to a record a `restricted` reference holds, stays: the batch
//! leaves it out with `NOT EXISTS`, so one held record never stops the
//! rest going. References that `cascade` and those `cleared` do what the
//! database's foreign keys say, as for any delete. The rows are locked
//! `FOR UPDATE SKIP LOCKED`, so a purge never waits on, or undoes, a
//! request changing one of them.

use jadpo_check::data::queries::Value;
use jadpo_check::data::sql::{self, Bind};
use jadpo_check::data::{DataModel, EntityModel};
use jadpo_std::OnDelete;
use jadpo_std::read::SqlType;
use jadpo_std::write::{PURGE_BATCH, PurgeParam, PurgePlan, PurgeSource};

use crate::queries::{column_type, field, quoted};

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
        format!("${}", at + 1)
    }
}

impl Bind for Purging {
    fn bind(&mut self, value: &Value, ty: SqlType) -> Option<String> {
        match value {
            Value::Now => Some(self.param(PurgeSource::Now, ty)),
            _ => None,
        }
    }
}

/// `entity`'s purge, when its lifecycle has one.
pub(crate) fn plan(model: &DataModel, entity: &EntityModel) -> Option<PurgePlan> {
    let lifecycle = entity.lifecycle.as_ref()?;
    let purge = lifecycle.purge.as_ref()?;
    let visible = lifecycle.visible.as_ref()?;
    let store = entity.store.clone()?;
    let table = quoted(&entity.table);
    let since = field(entity, &purge.since)?;
    let since_column = quoted(&since.born);
    let mut purging = Purging::default();
    let cutoff = purging.param(PurgeSource::Cutoff, column_type(since)?.param());
    let mut conditions = vec![
        // Hidden: `visible:` doesn't hold.
        sql::condition(&mut purging, entity, visible, true)?,
        format!("{since_column} <= {cutoff}"),
    ];
    for path in held_by(model, &entity.name) {
        conditions.push(not_held(model, entity, &path)?);
    }
    let mut inner = format!("  SELECT \"id\"\n  FROM {table}");
    crate::queries::where_clause(&mut inner, &conditions, "  ");
    inner.push_str(&format!(
        "\n  ORDER BY {since_column} ASC, \"id\" ASC\n  LIMIT {PURGE_BATCH}\n  FOR UPDATE SKIP LOCKED"
    ));
    let sql = format!("DELETE FROM {table}\nWHERE \"id\" IN (\n{inner}\n)\nRETURNING \"id\"");
    Some(PurgePlan {
        entity: entity.name.clone(),
        store,
        sql,
        params: purging.params,
        after: i64::try_from(purge.after).ok()?,
    })
}

/// Each way a `restricted` reference can hold a record of `entity`: the
/// references followed from it, each `(holder, field)`, through
/// references that cascade, ending at one that is restricted.
fn held_by(model: &DataModel, entity: &str) -> Vec<Vec<(String, String)>> {
    let mut out = Vec::new();
    let mut seen: Vec<String> = vec![entity.to_string()];
    let mut queue: Vec<(String, Vec<(String, String)>)> = vec![(entity.to_string(), Vec::new())];
    while !queue.is_empty() {
        let (current, path) = queue.remove(0);
        for child in &model.entities {
            for reference_field in &child.fields {
                let Some(reference) = &reference_field.reference else {
                    continue;
                };
                if reference.target != current {
                    continue;
                }
                let mut next = path.clone();
                next.push((child.name.clone(), reference_field.name.clone()));
                match reference.on_delete {
                    Some(OnDelete::Restricted) | None => out.push(next),
                    Some(OnDelete::Cascade) => {
                        if !seen.contains(&child.name) {
                            seen.push(child.name.clone());
                            queue.push((child.name.clone(), next));
                        }
                    }
                    Some(OnDelete::Cleared) => {}
                }
            }
        }
    }
    out
}

/// `NOT EXISTS` a record holding this one through `path`.
fn not_held(model: &DataModel, entity: &EntityModel, path: &[(String, String)]) -> Option<String> {
    let mut from = Vec::new();
    let mut first_link = String::new();
    for (i, (holder, field_name)) in path.iter().enumerate() {
        let holder = model.entity(holder)?;
        let column = quoted(&field(holder, field_name)?.born);
        let alias = format!("\"held{}\"", i + 1);
        if i == 0 {
            from.push(format!("{} AS {alias}", quoted(&holder.table)));
            first_link = format!("{alias}.{column} = {}.\"id\"", quoted(&entity.table));
        } else {
            from.push(format!(
                "JOIN {} AS {alias} ON {alias}.{column} = \"held{i}\".\"id\"",
                quoted(&holder.table)
            ));
        }
    }
    Some(format!(
        "NOT EXISTS (\n      SELECT 1\n      FROM {}\n      WHERE {first_link}\n    )",
        from.join("\n      ")
    ))
}
