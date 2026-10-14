//! The SQL each query shape, `get` and write runs on PostgreSQL, held in
//! golden files so every change to it is reviewed (area 6;
//! docs/compiler-runtime.md §2.1). `tests/sql/project.jadpo` declares one
//! query per shape and one action per write shape; each plan is written to
//! `tests/sql/<name>.sql` with its parameters, includes and refusals, and
//! compared byte for byte.
//!
//! `JADPO_BLESS=1 cargo test -p jadpo-plan --test sql` rewrites them.

use std::path::{Path, PathBuf};

use jadpo_db::{Database, File, Layout, Project};
use jadpo_plan::{Decoded, Plan, Refusal, WriteSitePlan};
use jadpo_std::read::{Cardinality, Column, Field, IncludeShape, ReadPlan, Select, Size, Source};
use jadpo_std::write::WriteSource;

fn folder() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/sql")
}

fn plan() -> Plan {
    let source = std::fs::read_to_string(folder().join("project.jadpo")).expect("the fixture");
    let db = Database::default();
    let file = File::new(&db, "main.jadpo".to_string(), source);
    let project = Project::new(
        &db,
        vec![file],
        Vec::new(),
        Layout::Loose,
        Default::default(),
        Vec::new(),
    );
    let problems = jadpo_check::check(&db, project);
    assert!(problems.is_empty(), "the fixture checks: {problems:#?}");
    Plan::build(&db, project)
}

fn source(source: &Source) -> String {
    match source {
        Source::Arg { index, path } if path.is_empty() => format!("argument {index}"),
        Source::Arg { index, path } => format!("argument {index}.{}", path.join(".")),
        Source::Now => "clock.now".to_string(),
        Source::Keys => "keys".to_string(),
        Source::Cursor(i) => format!("cursor {i}"),
        Source::SizePlusOne => "page size + 1".to_string(),
    }
}

fn column(column: &Column) -> String {
    match column {
        Column::Enum { ty, .. } => format!("enum {ty}"),
        other => format!("{other:?}").to_lowercase(),
    }
}

fn select(out: &mut String, select: &Select, depth: usize) {
    let indent = "  ".repeat(depth);
    for line in select.sql.lines() {
        out.push_str(&format!("{indent}{line}\n"));
    }
    for (i, param) in select.params.iter().enumerate() {
        out.push_str(&format!(
            "{indent}-- ${}: {} as {:?}\n",
            i + 1,
            source(&param.source),
            param.ty
        ));
    }
    let columns: Vec<String> = select.columns.iter().map(column).collect();
    out.push_str(&format!("{indent}-- columns: {}\n", columns.join(", ")));
    let fields: Vec<String> = select
        .record
        .fields
        .iter()
        .map(|(name, field)| match field {
            Field::Column(at) => format!("{name} = column {at}"),
            Field::Include(at) => format!("{name} = include {at}"),
        })
        .collect();
    out.push_str(&format!(
        "{indent}-- record {}: {}\n",
        select.record.ty,
        fields.join(", ")
    ));
    for (i, include) in select.includes.iter().enumerate() {
        let shape = match include.shape {
            IncludeShape::One => "one",
            IncludeShape::Optional => "optional",
            IncludeShape::List => "list",
        };
        out.push_str(&format!(
            "\n{indent}-- include {i}: {shape}, matching column {}\n",
            include.key
        ));
        self::select(out, &include.select, depth + 1);
    }
}

fn read(name: &str, read: &ReadPlan) -> String {
    let cardinality = match read.cardinality {
        Cardinality::One => "one",
        Cardinality::Optional => "optional",
        Cardinality::List => "list",
        Cardinality::Page => "page",
    };
    let mut out = format!("-- {name}: {cardinality}, store {}\n", read.store);
    if let Some(page) = &read.page {
        let size = match &page.size {
            Size::Fixed(n) => n.to_string(),
            Size::From(from) => source(from),
        };
        let cursor: Vec<String> = page
            .cursor
            .iter()
            .map(|(at, ty)| format!("column {at} as {ty:?}"))
            .collect();
        out.push_str(&format!(
            "-- page: size {size}, after {}, cursor {}\n",
            source(&page.after),
            cursor.join(", ")
        ));
    }
    select(&mut out, &read.select, 0);
    out
}

fn write(name: &str, write: &WriteSitePlan) -> String {
    let plan = &write.plan;
    let mut out = format!(
        "-- {name}: {}{}, store {}\n{}\n",
        plan.op.word(),
        if plan.by_id { " by id" } else { "" },
        plan.store,
        plan.sql
    );
    for (i, param) in plan.params.iter().enumerate() {
        let source = match &param.source {
            WriteSource::Input(at) => format!("input {at}"),
            WriteSource::Field { input, field } => format!("input {input}.{field}"),
            WriteSource::Supplied { input, field } => format!("whether input {input} supplied {field}"),
            WriteSource::AnySupplied { input, fields } => {
                format!("whether input {input} supplied any of {}", fields.join(", "))
            }
            WriteSource::NewId => "a new identity".to_string(),
            WriteSource::Now => "clock.now".to_string(),
        };
        out.push_str(&format!("-- ${}: {source} as {:?}\n", i + 1, param.ty));
    }
    if let Some(returned) = &plan.returns {
        let columns: Vec<String> = returned.columns.iter().map(column).collect();
        out.push_str(&format!("-- returns: {}\n", columns.join(", ")));
    }
    for (constraint, refusal) in plan.refusals.iter().zip(&write.refusals) {
        let raises = match refusal {
            Refusal::Arm(at) => format!("conflict arm {at}"),
            Refusal::NotFound(missing) => format!("not found, failure {}", missing.failure.0),
            Refusal::Gone(what) => format!("fault: {what}"),
        };
        out.push_str(&format!("-- refused by {constraint}: {raises}\n"));
    }
    if write.missing.is_some() {
        out.push_str("-- no row: the entity's not-found failure\n");
    }
    out
}

#[test]
fn every_query_shape_matches_its_golden_sql() {
    let plan = plan();
    let bless = std::env::var_os("JADPO_BLESS").is_some();
    let mut written: Vec<(String, String)> = Vec::new();
    for query in plan.queries() {
        written.push((
            query.name.clone(),
            read(&format!("query {}", query.name), &query.read),
        ));
    }
    for entity in plan.entities() {
        let name = entity.name.to_lowercase();
        written.push((
            format!("{name}_get"),
            read(&format!("{}.get", entity.name), &entity.get),
        ));
    }
    for callable in plan.callables() {
        for site in &callable.sites {
            if let Decoded::Write { plan: Some(id), .. } = &site.decoded
                && let Some(found) = plan.write(*id)
            {
                let name = callable.key.name().to_string();
                written.push((name.clone(), write(&format!("action {name}"), found)));
            }
        }
    }
    assert!(written.len() >= 6 + 5 + 7, "every query, entity and write is planned");
    let mut different = Vec::new();
    for (name, text) in &written {
        let path = folder().join(format!("{name}.sql"));
        if bless {
            std::fs::write(&path, text).expect("writes the golden file");
            continue;
        }
        let saved = std::fs::read_to_string(&path).unwrap_or_default();
        if saved != *text {
            different.push(format!("{name}.sql:\n{text}"));
        }
    }
    assert!(
        different.is_empty(),
        "the SQL changed; review it and run with JADPO_BLESS=1:\n\n{}",
        different.join("\n")
    );
}
