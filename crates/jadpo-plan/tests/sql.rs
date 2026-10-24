//! The SQL each query shape, `get` and write runs on PostgreSQL, held in
//! golden files so every change to it is reviewed (area 6;
//! docs/compiler-runtime.md §2.1). `tests/sql/project.jadpo` declares one
//! query per shape and one action per write shape; each plan is written to
//! `tests/sql/<name>.sql` with its parameters, includes and refusals, and
//! compared byte for byte.
//!
//! `JADPO_BLESS=1 cargo test -p jadpo-plan --test integration sql::` rewrites them.

use std::path::{Path, PathBuf};

use jadpo_db::{Database, File, Layout, Project};
use jadpo_plan::{Decoded, Plan, PurgePlan, Raises, WriteSitePlan};
use jadpo_std::read::{
    Check, Column, Field, IncludeShape, Operand, ReadPlan, Select, Size, Source,
};
use jadpo_std::write::WriteSource;

fn folder() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/sql")
}

fn plan() -> Plan {
    let source = std::fs::read_to_string(folder().join("project.jadpo")).expect("the fixture");
    planned(source)
}

fn planned(source: String) -> Plan {
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
        Source::Caller(part) => format!("caller {part:?}"),
        Source::Stored(value) => format!("literal {value:?}"),
        Source::Fetch => "rows to fetch".to_string(),
        Source::Envelope { index, path, part } => format!(
            "{part:?} of {}",
            self::source(&Source::Arg {
                index: *index,
                path: path.clone()
            })
        ),
        Source::DistanceBound {
            index,
            path,
            strict,
        } => format!(
            "distance bound{} of {}",
            if *strict { " (strict)" } else { "" },
            self::source(&Source::Arg {
                index: *index,
                path: path.clone()
            })
        ),
    }
}

/// A check the runtime holds each row to, as code calls it:
/// `geo.within(column 2, argument 0, argument 1)`.
fn check(check: &Check) -> String {
    let name = jadpo_std::OPERATIONS
        .iter()
        .find(|operation| operation.op == check.op)
        .map_or_else(
            || format!("{:?}", check.op),
            |o| format!("{}.{}", o.namespace, o.name),
        );
    let args: Vec<String> = check
        .args
        .iter()
        .map(|arg| match arg {
            Operand::Column(at) => format!("column {at}"),
            Operand::Value(from) => source(from),
            Operand::Constant(value) => format!("{value:?}"),
            Operand::Absent => "none".to_string(),
        })
        .collect();
    format!("{name}({})", args.join(", "))
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
    if let Some(at) = select.permitted {
        out.push_str(&format!(
            "{indent}-- the caller may read a row: column {at}\n"
        ));
    }
    fn shown(record: &jadpo_std::read::Record) -> String {
        let fields: Vec<String> = record
            .fields
            .iter()
            .map(|(name, field)| match field {
                Field::Column(at) => format!("{name} = column {at}"),
                Field::Include(at) => format!("{name} = include {at}"),
                Field::Record(inner) => format!("{name} = ({})", shown(inner)),
            })
            .collect();
        format!("{}: {}", record.ty, fields.join(", "))
    }
    out.push_str(&format!("{indent}-- record {}\n", shown(&select.record)));
    // What an outside call reads of the rows it loaded before it closes
    // the snapshot (chunk 7.4).
    if let Some(versions) = &select.versions {
        out.push_str(&format!(
            "{indent}-- row versions of {}, by the id in column {}:\n",
            versions.table, versions.id
        ));
        for line in versions.sql.lines() {
            out.push_str(&format!("{indent}--   {line}\n"));
        }
    }
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
    let cardinality = read.cardinality.word();
    let mut out = format!("-- {name}: {cardinality}, store {}\n", read.store);
    for setting in &read.settings {
        out.push_str(&format!("{setting};\n"));
    }
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
    if let Some(recheck) = &read.recheck {
        for check in &recheck.checks {
            out.push_str(&format!("-- recheck: {}\n", self::check(check)));
        }
        if let Some(order) = &recheck.order {
            out.push_str(&format!(
                "-- recheck order: {}, the database's in column {}, ties by column {}\n",
                self::check(&order.key),
                order.measured,
                order.id
            ));
        }
        if let Some(limit) = &recheck.limit {
            let limit = match limit {
                Size::Fixed(n) => n.to_string(),
                Size::From(from) => source(from),
            };
            out.push_str(&format!("-- recheck keeps: {limit}\n"));
        }
    }
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
            WriteSource::Supplied { input, field } => {
                format!("whether input {input} supplied {field}")
            }
            WriteSource::AnySupplied { input, fields } => {
                format!(
                    "whether input {input} supplied any of {}",
                    fields.join(", ")
                )
            }
            WriteSource::NewId => "a new identity".to_string(),
            WriteSource::Now => "clock.now".to_string(),
            WriteSource::Caller(part) => format!("caller {part:?}"),
            WriteSource::Part { of, index } => format!("part {index} of {of:?}"),
            WriteSource::Json { of, codec } => format!("JSON {codec} of {of:?}"),
            WriteSource::Stored(value) => format!("literal {value:?}"),
        };
        out.push_str(&format!("-- ${}: {source} as {:?}\n", i + 1, param.ty));
    }
    if let Some(returned) = &plan.returns {
        let columns: Vec<String> = returned.columns.iter().map(column).collect();
        out.push_str(&format!("-- returns: {}\n", columns.join(", ")));
    }
    for refusal in &plan.refusals {
        let raises = match &refusal.raises {
            Raises::Arm(at) => format!("conflict arm {at}"),
            Raises::NotFound(missing) => format!("not found, failure {}", missing.failure.0),
            Raises::Gone(what) => format!("fault: {what}"),
        };
        out.push_str(&format!("-- refused by {}: {raises}\n", refusal.constraint));
    }
    if let Some(guard) = &plan.guard {
        out.push_str(&format!(
            "-- no row: asks, refused by {} when the record is there\n",
            plan.refusals[guard.refusal as usize].constraint
        ));
        for line in guard.sql.lines() {
            out.push_str(&format!("--   {line}\n"));
        }
        for (i, param) in guard.params.iter().enumerate() {
            out.push_str(&format!(
                "--   ${}: {:?} as {:?}\n",
                i + 1,
                param.source,
                param.ty
            ));
        }
    }
    if let Some(jadpo_std::write::PolicyRefusal::Probe {
        fields: Some(probe),
        ..
    }) = &plan.policy
    {
        out.push_str(
            "-- no row: asks, refused by a field's own policy when the record's grants let the caller in\n",
        );
        for line in probe.sql.lines() {
            out.push_str(&format!("--   {line}\n"));
        }
        for (i, param) in probe.params.iter().enumerate() {
            out.push_str(&format!(
                "--   ${}: {:?} as {:?}\n",
                i + 1,
                param.source,
                param.ty
            ));
        }
    }
    if plan.missing.is_some() {
        out.push_str("-- no row: the entity's not-found failure\n");
    }
    // Successful writes compare the version in their own WHERE. A write
    // changing no row probes the version before classifying its refusal.
    if let Some(row) = &plan.row {
        let id = match &row.id {
            WriteSource::Input(at) => format!("input {at}"),
            WriteSource::Field { input, field } => format!("input {input}.{field}"),
            other => format!("{other:?}"),
        };
        out.push_str(&format!(
            "-- loaded before a call: {} row {id}, expected version in ${}; no row probes version first\n",
            row.table, row.version + 1
        ));
        for line in row.sql.lines() {
            out.push_str(&format!("--   {line}\n"));
        }
    }
    out
}

fn purge(plan: &PurgePlan) -> String {
    let mut out = format!(
        "-- purge {}: after {}ms, store {}\n{}\n",
        plan.entity, plan.after, plan.store, plan.sql
    );
    for (i, param) in plan.params.iter().enumerate() {
        out.push_str(&format!(
            "-- ${}: {:?} as {:?}\n",
            i + 1,
            param.source,
            param.ty
        ));
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
    for plan in plan.purges() {
        written.push((format!("{}_purge", plan.entity.to_lowercase()), purge(plan)));
    }
    assert!(
        written.len() >= 6 + 5 + 7,
        "every query, entity and write is planned"
    );
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

/// The caller a test's, fixture's or seed's call runs as is the argument
/// the callee's signature types as a `Principal`, wherever it comes, which
/// the plan names (chunk 4.10), never a value's type name.
#[test]
fn the_plan_names_each_callables_caller_parameter() {
    let mut source = std::fs::read_to_string(folder().join("project.jadpo")).expect("the fixture");
    source.push_str(
        r#"
query own_projects(name: Project.name, owner: Principal.user?) -> List<Project> {
    from: Project
    where: Project.name == name
    order: [Project.name asc]
    limit: 10
}
"#,
    );
    let plan = planned(source);
    let caller = |name: &str| {
        let id = plan.find(name).unwrap_or_else(|| panic!("`{name}`"));
        plan.callable(id).expect("a callable").caller
    };
    assert_eq!(caller("open_ledger"), Some(0));
    assert_eq!(caller("add_todo"), Some(0));
    assert_eq!(
        caller("set_balance"),
        None,
        "no caller: runs without policy"
    );
    let query = |name: &str| plan.query_named(name).expect("a query").caller;
    assert_eq!(query("own_projects"), Some(1), "an optional caller, second");
    assert_eq!(query("project_by_id"), None);
}

/// Every compiler-generated mutation, including its policy-free form,
/// compares the pre-call row version in the mutation's own WHERE.
#[test]
fn every_mutation_holds_its_row_version_in_its_own_where() {
    let plan = plan();
    let mut checked = 0;
    for callable in plan.callables() {
        for site in &callable.sites {
            if let Decoded::Write { plan: Some(id), .. } = &site.decoded
                && let Some(found) = plan.write(*id)
                && found.plan.row.is_some()
            {
                for sql in std::iter::once(&found.plan.sql).chain(found.plan.unpoliced.iter()) {
                    assert!(
                        sql.contains("\"xmin\"::text = $"),
                        "{}: {sql}",
                        callable.key.name()
                    );
                    assert!(sql.contains("::text IS NULL OR"), "{sql}");
                }
                checked += 1;
            }
        }
    }
    assert!(checked >= 3);
}
