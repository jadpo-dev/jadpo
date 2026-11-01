//! Where each store of files keeps its record of which files its records
//! hold (area 16, "Deleting and erasing"; chunk B3): the database store
//! whose entities' `File` fields name it, which keeps `jadpo_files` and,
//! on each table with a `File` column, a trigger counting each file's
//! holders as rows change, so a file's bytes are deleted once no record
//! holds it and never while one does.
//!
//! A trigger counts only its own store's rows, so the fields naming one
//! store of files must all be in entities of one database store. One in
//! another is `toolchain.not_built_yet` where its field is written: the
//! two records couldn't be counted as one, and bytes one still held
//! could be deleted.

use std::collections::BTreeMap;

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, rule};

use super::data_model;

/// By each store of files a `File` field names, the database store its
/// record of held files is kept in: the store of the first entity, in
/// name order, with a field naming it.
#[salsa::tracked(returns(ref))]
pub fn counters(db: &dyn Db, project: Project) -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    for entity in &data_model(db, project).entities {
        let Some(store) = &entity.store else {
            continue;
        };
        for field in &entity.fields {
            if let Some(file) = &field.file {
                out.entry(file.store.clone())
                    .or_insert_with(|| store.clone());
            }
        }
    }
    out
}

/// A `File` field whose store of files another database store's entities
/// already name.
#[salsa::tracked(returns(ref))]
pub fn counting_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let counters = counters(db, project);
    let mut out = Vec::new();
    for entity in &data_model(db, project).entities {
        let Some(store) = &entity.store else {
            continue;
        };
        for field in &entity.fields {
            let Some(file) = &field.file else {
                continue;
            };
            let Some(counter) = counters
                .get(&file.store)
                .filter(|counter| *counter != store)
            else {
                continue;
            };
            let Some(at) = super::retention::field_span(db, project, &entity.name, &field.name)
            else {
                continue;
            };
            out.push(
                Diagnostic::new(
                    &rule::TOOLCHAIN_NOT_BUILT_YET,
                    format!(
                        "`{}.{}` keeps its bytes in `{}`, as records in `{counter}` do",
                        entity.name, field.name, file.store
                    ),
                )
                .at(at, format!("an entity in `{store}`"))
                .reason(format!(
                    "Jadpo deletes a file's bytes once no record holds it, counting the records that do in the database they are kept in. Records in `{store}` and `{counter}` holding files of one store can't be counted as one yet, so bytes one of them still held could be deleted."
                ))
                .next(format!(
                    "Keep `{}` in `{counter}`, or give its files a store of their own",
                    entity.name
                )),
            );
        }
    }
    out
}
