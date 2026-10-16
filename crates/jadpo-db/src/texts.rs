//! The text of every file a diagnostic can point into, by project-relative
//! path: what the renderer quotes a line from.

use std::collections::BTreeMap;

use crate::{Db, Project};

/// Each source file's text and each readable translation file's, by path.
pub fn source_texts(db: &dyn Db, project: Project) -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = project
        .files(db)
        .iter()
        .map(|file| (file.path(db).clone(), file.text(db).clone()))
        .collect();
    for record in project.translations(db) {
        if let Ok(text) = &record.text {
            out.insert(record.path.clone(), text.clone());
        }
    }
    out
}
