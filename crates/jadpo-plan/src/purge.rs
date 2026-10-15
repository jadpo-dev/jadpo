//! The SQL a lifecycle's `purge:` runs (area 5, "Lifecycle"): one batch
//! deletes the oldest hidden records whose `since:` field is older than the
//! run's time less `after:`, at most [`PURGE_BATCH`] of them, as
//! [`jadpo_std::write::PurgePlan`]s the runtime's purge entry point runs.
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
    match held_by(model, &entity.name) {
        Some(paths) => {
            for path in paths {
                conditions.push(not_held(model, entity, &path)?);
            }
        }
        None => conditions.push(not_held_anywhere(model, entity)?),
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
    let mut edges: Vec<String> = Vec::new();
    let mut holds: Vec<String> = Vec::new();
    let mut at = 0;
    while let Some(current) = reached.get(at).cloned() {
        for (child, field_name, on_delete) in referring(model, &current) {
            let holder = model.entity(&child)?;
            let column = quoted(&field(holder, &field_name)?.born);
            let table = quoted(&holder.table);
            match on_delete {
                Some(OnDelete::Cascade) => {
                    let number = match reached.iter().position(|name| *name == child) {
                        Some(number) => number,
                        None => {
                            reached.push(child.clone());
                            reached.len() - 1
                        }
                    };
                    let alias = format!("\"edge{}\"", edges.len() + 1);
                    edges.push(format!(
                        "SELECT {at} AS \"parent\", {alias}.{column} AS \"parent_id\", {number} AS \"entity\", {alias}.\"id\" AS \"id\" FROM {table} AS {alias}"
                    ));
                }
                Some(OnDelete::Restricted) | None => {
                    let alias = format!("\"held{}\"", holds.len() + 1);
                    holds.push(format!(
                        "(\"reach\".\"entity\" = {at} AND EXISTS (SELECT 1 FROM {table} AS {alias} WHERE {alias}.{column} = \"reach\".\"id\"))"
                    ));
                }
                Some(OnDelete::Cleared) => {}
            }
        }
        at += 1;
    }
    if holds.is_empty() {
        return Some("TRUE".to_string());
    }
    Some(format!(
        "NOT EXISTS (\n      WITH RECURSIVE \"reach\" (\"entity\", \"id\") AS (\n        SELECT 0, {}.\"id\"\n        UNION\n        SELECT \"edge\".\"entity\", \"edge\".\"id\"\n        FROM \"reach\"\n        JOIN (\n          {}\n        ) AS \"edge\" ON \"edge\".\"parent\" = \"reach\".\"entity\" AND \"edge\".\"parent_id\" = \"reach\".\"id\"\n      )\n      SELECT 1\n      FROM \"reach\"\n      WHERE {}\n    )",
        quoted(&entity.table),
        edges.join("\n          UNION ALL\n          "),
        holds.join("\n        OR "),
    ))
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
