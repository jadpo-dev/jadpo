//! Projects built in memory for the checker's tests.

#![allow(dead_code)]

use jadpo_db::{Database, File, Layout, Project};

/// A project of `files`, in the order given.
pub fn project(db: &Database, layout: Layout, files: &[(&str, &str)]) -> Project {
    let files: Vec<File> = files
        .iter()
        .map(|(path, text)| File::new(db, path.to_string(), text.to_string()))
        .collect();
    Project::new(db, files, Vec::new(), layout)
}

/// Each problem as `path rule: message`.
pub fn problems(layout: Layout, files: &[(&str, &str)]) -> Vec<String> {
    let db = Database::default();
    let project = project(&db, layout, files);
    jadpo_check::check(&db, project)
        .iter()
        .map(|d| {
            let path = d
                .primary
                .as_ref()
                .map_or("", |label| label.span.path.as_str());
            format!("{path} {}: {}", d.rule.id, d.message)
        })
        .collect()
}

/// The rules of each problem, in order.
pub fn rules(layout: Layout, files: &[(&str, &str)]) -> Vec<String> {
    let db = Database::default();
    let project = project(&db, layout, files);
    jadpo_check::check(&db, project)
        .iter()
        .map(|d| d.rule.id.to_string())
        .collect()
}

pub const APP: (&str, &str) = ("app.jadpo", "application {}\n");
