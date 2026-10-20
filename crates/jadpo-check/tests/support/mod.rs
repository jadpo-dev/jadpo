//! Projects built in memory for the checker's tests.

#![allow(dead_code)]

use jadpo_db::{Database, File, Layout, Project, Record};

/// A project of `files`, in the order given. A project with entities
/// gets the migration that creates their tables, as `jadpo migrate new`
/// would write it, so tests of other rules aren't told the migrations are
/// out of date; and each domain with events gets its event record, as
/// `jadpo records update` would write it.
pub fn project(db: &Database, layout: Layout, files: &[(&str, &str)]) -> Project {
    // An email's templates are read as records, as the loader reads them
    // (area 16).
    let template = |path: &str| layout == Layout::Project && jadpo_db::is_email_template(path);
    let templates: Vec<Record> = files
        .iter()
        .filter(|(path, _)| template(path))
        .map(|(path, text)| Record {
            path: path.to_string(),
            text: Ok(text.to_string()),
        })
        .collect();
    let make = |db: &Database, mut records: Vec<Record>| {
        let inputs: Vec<File> = files
            .iter()
            .filter(|(path, _)| !template(path))
            .map(|(path, text)| File::new(db, path.to_string(), text.to_string()))
            .collect();
        records.extend(templates.iter().cloned());
        Project::new(db, inputs, Vec::new(), layout, Default::default(), records)
    };
    let scratch = Database::default();
    let scratched = make(&scratch, Vec::new());
    let mut records = migration(&scratch, scratched);
    records.extend(event_records(&scratch, scratched));
    make(db, records)
}

/// Each domain's event record, as `jadpo records update` writes it
/// (chunk 6.1c).
fn event_records(db: &Database, project: Project) -> Vec<Record> {
    if project.layout(db) != Layout::Project {
        return Vec::new();
    }
    jadpo_check::events::record::record_texts(db, project)
        .iter()
        .map(|(domain, text)| Record {
            path: jadpo_check::events::record::path(domain),
            text: Ok(text.clone()),
        })
        .collect()
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

/// A project of `files` with the generated records at `records`, as they
/// would be read from disk.
pub fn project_with_records(
    db: &Database,
    layout: Layout,
    files: &[(&str, &str)],
    records: &[(&str, &str)],
) -> Project {
    let inputs: Vec<File> = files
        .iter()
        .map(|(path, text)| File::new(db, path.to_string(), text.to_string()))
        .collect();
    let records = records
        .iter()
        .map(|(path, text)| Record {
            path: path.to_string(),
            text: Ok(text.to_string()),
        })
        .collect();
    Project::new(db, inputs, Vec::new(), layout, Default::default(), records)
}

/// A project of `files`, created in the order given and listed in path
/// order, as `jadpo_db::load` lists them: only the order the inputs were
/// made in differs.
pub fn project_in_path_order(db: &Database, layout: Layout, files: &[(&str, &str)]) -> Project {
    let mut inputs: Vec<File> = files
        .iter()
        .map(|(path, text)| File::new(db, path.to_string(), text.to_string()))
        .collect();
    inputs.sort_by(|a, b| a.path(db).cmp(b.path(db)));
    Project::new(
        db,
        inputs,
        Vec::new(),
        layout,
        Default::default(),
        Vec::new(),
    )
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
