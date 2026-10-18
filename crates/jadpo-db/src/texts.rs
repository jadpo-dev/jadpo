//! The text of every file a diagnostic can point into, by project-relative
//! path: what the renderer quotes a line from.

use std::collections::BTreeMap;

use crate::{Db, PROMPT_EXTENSION, Project};

/// Each source file's text, by path, and each prompt's (area 21), which
/// its errors point into.
pub fn source_texts(db: &dyn Db, project: Project) -> BTreeMap<String, String> {
    project
        .files(db)
        .iter()
        .map(|file| (file.path(db).clone(), file.text(db).clone()))
        .chain(project.records(db).iter().filter_map(|record| {
            let text = record.text.as_ref().ok()?;
            record
                .path
                .ends_with(PROMPT_EXTENSION)
                .then(|| (record.path.clone(), text.clone()))
        }))
        .collect()
}
