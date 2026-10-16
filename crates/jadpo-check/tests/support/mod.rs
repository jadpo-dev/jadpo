//! Projects built in memory for the checker's tests.

#![allow(dead_code)]

use jadpo_db::{Database, File, Layout, Project, Record};

/// A project of `files`, in the order given. A project with entities
/// gets the migration that creates their tables, as `jadpo migrate new`
/// would write it, so tests of other rules aren't told the migrations are
/// out of date.
pub fn project(db: &Database, layout: Layout, files: &[(&str, &str)]) -> Project {
    let make = |db: &Database, records: Vec<Record>| {
        let inputs: Vec<File> = files
            .iter()
            .filter(|(path, _)| !jadpo_db::is_translation(path))
            .map(|(path, text)| File::new(db, path.to_string(), text.to_string()))
            .collect();
        Project::builder(inputs, Vec::new(), layout, Default::default(), records)
            .translations(translations(files))
            .new(db)
    };
    let scratch = Database::default();
    let records = migration(&scratch, make(&scratch, Vec::new()));
    make(db, records)
}

/// The translation files among `files`, in path order, as the project
/// holds them.
pub fn translations(files: &[(&str, &str)]) -> Vec<Record> {
    let mut found: Vec<Record> = files
        .iter()
        .filter(|(path, _)| jadpo_db::is_translation(path))
        .map(|(path, text)| Record {
            path: path.to_string(),
            text: Ok(text.to_string()),
        })
        .collect();
    found.sort_by(|a, b| a.path.cmp(&b.path));
    found
}

fn migration(db: &Database, project: Project) -> Vec<Record> {
    if project.layout(db) != Layout::Project {
        return Vec::new();
    }
    let schema = (**jadpo_check::data::schema(db, project)).clone();
    if schema.stores.is_empty() {
        return Vec::new();
    }
    let name = "20260101T0000_initial";
    let sql = jadpo_graph::migration::migration_sql(&Default::default(), &schema, &|_| None)
        .map(|sql| sql.to_document(name))
        .unwrap_or_default();
    vec![
        Record {
            path: format!("database/migrations/{name}/postgres.sql"),
            text: Ok(sql),
        },
        Record {
            path: format!("database/migrations/{name}/snapshot.json"),
            text: Ok(schema.to_document()),
        },
    ]
}

/// A project of `files`, created in the order given and listed in path
/// order, as `jadpo_db::load` lists them: only the order the inputs were
/// made in differs.
pub fn project_in_path_order(db: &Database, layout: Layout, files: &[(&str, &str)]) -> Project {
    let mut inputs: Vec<File> = files
        .iter()
        .filter(|(path, _)| !jadpo_db::is_translation(path))
        .map(|(path, text)| File::new(db, path.to_string(), text.to_string()))
        .collect();
    inputs.sort_by(|a, b| a.path(db).cmp(b.path(db)));
    Project::builder(
        inputs,
        Vec::new(),
        layout,
        Default::default(),
        Vec::new(),
    )
    .translations(translations(files))
    .new(db)
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
