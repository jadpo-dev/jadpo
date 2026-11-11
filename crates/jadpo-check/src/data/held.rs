//! Where each store of files keeps its record of which files its records
//! hold (area 16, "Deleting and erasing"; chunk B3): each database store
//! whose entities' `File` fields name it keeps `jadpo_files` and, on each
//! table with a column holding files (a `File` column, or a structured
//! value with files inside), a trigger counting each file's holders as
//! rows change, so a file's bytes are deleted once no record holds it and
//! never while one does.
//!
//! A trigger counts only its own store's rows, so when fields in entities
//! of several databases name one store of files, each counts its own, and
//! the runtime deletes a file's bytes only once none of them holds it
//! (chunk B14).

use std::collections::BTreeMap;

use jadpo_db::{Db, Project};

use super::data_model;

/// By each store of files a `File` field names, the database stores
/// counting its files' holders: the store of the first entity, in name
/// order, with a field naming it, where a put records its file, then each
/// other in name order. A structured field holding a `File` inside it,
/// however deep (`List<File>`, an object's field), names no store, so a
/// database with one counts the holders of every store of files.
#[salsa::tracked(returns(ref))]
pub fn counters(db: &dyn Db, project: Project) -> BTreeMap<String, Vec<String>> {
    let model = data_model(db, project);
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for entity in &model.entities {
        let Some(store) = &entity.store else {
            continue;
        };
        for field in &entity.fields {
            if let Some(file) = &field.file {
                let counting = out.entry(file.store.clone()).or_default();
                if !counting.contains(store) {
                    counting.push(store.clone());
                }
            }
        }
    }
    for entity in &model.entities {
        let Some(store) = &entity.store else {
            continue;
        };
        if !entity.fields.iter().any(holds_files_within) {
            continue;
        }
        for counting in out.values_mut() {
            if !counting.contains(store) {
                counting.push(store.clone());
            }
        }
    }
    for counting in out.values_mut() {
        if let Some((_, rest)) = counting.split_first_mut() {
            rest.sort();
        }
    }
    out
}

/// Whether `field` keeps a structured value with a `File` inside it.
fn holds_files_within(field: &super::FieldModel) -> bool {
    field.file.is_none()
        && field
            .allows
            .json
            .as_ref()
            .is_some_and(|reads| jadpo_graph::stored_json::files_within(reads).is_some())
}
