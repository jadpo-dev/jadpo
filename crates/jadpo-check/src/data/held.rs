//! Where each store of files keeps its record of which files its records
//! hold (area 16, "Deleting and erasing"; chunk B3): each database store
//! whose entities' `File` fields name it keeps `jadpo_files` and, on each
//! table with a `File` column, a trigger counting each file's holders as
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
/// other in name order.
#[salsa::tracked(returns(ref))]
pub fn counters(db: &dyn Db, project: Project) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for entity in &data_model(db, project).entities {
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
    for counting in out.values_mut() {
        if let Some((_, rest)) = counting.split_first_mut() {
            rest.sort();
        }
    }
    out
}
