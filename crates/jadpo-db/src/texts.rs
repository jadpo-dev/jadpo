//! The text of every file a diagnostic can point into, by project-relative
//! path: what the renderer quotes a line from.

use std::collections::BTreeMap;

use crate::{Db, Project};

/// Each source file's text, by path.
pub fn source_texts(db: &dyn Db, project: Project) -> BTreeMap<String, String> {
    project
        .files(db)
        .iter()
        .map(|file| (file.path(db).clone(), file.text(db).clone()))
        .collect()
}
