//! `jadpo sql <query>` (area 6; area 14's command table): the PostgreSQL a
//! query sends, statement by statement, each with the parameters that fill
//! it. One statement for the query, and one more for each include however
//! many records it is included in.
//!
//! The screen: a bold `query <name>` heading over the statement, whole and
//! never cut, then on the label grid what it reads, where it is declared,
//! how many statements it sends and each `$n` with where its value comes
//! from and its type; each include repeats the shape under `include
//! <field>`. A success prints nothing more, as the design language's calm
//! default asks; `--json` gives the same as data.

use std::path::PathBuf;

use jadpo_check::data::queries::{Include, IncludeKind, QueryModel, query_models};
use jadpo_diag::{Diagnostic, Severity, rule};
use jadpo_plan::{Plan, QueryPlan};
use jadpo_std::read::{IncludeRead, Select, Source, SqlType};
use jadpo_term::{Block, Json, Screen};

use crate::fmt::project_not_found;
use crate::run::{Call, Reply, quoted};
use crate::suggest::nearest;

pub fn sql(call: &Call) -> Reply {
    if call.flag("--explain") {
        return Reply::failed(
            Diagnostic::new(
                &rule::TOOLCHAIN_NOT_BUILT_YET,
                "`jadpo sql --explain` isn't built yet",
            )
            .reason("`--explain` runs the query on a disposable database and shows PostgreSQL's plan (area 14); it is decided, and comes later.")
            .next("Run `jadpo sql <query>` to see its statements"),
        );
    }
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return project_not_found("sql");
    };
    let loaded = jadpo_db::load(&root);
    let db = &loaded.db;
    let queries = query_models(db, loaded.project);
    let mut names: Vec<String> = queries.queries.iter().map(|q| q.name.clone()).collect();
    names.sort();
    let Some(name) = call.arguments.first() else {
        let reason = if names.is_empty() {
            "It prints the statements a query sends, and this project declares no query."
                .to_string()
        } else {
            format!(
                "It prints the statements a query sends; this project's queries are {}.",
                crate::run::both(&quoted(&names))
            )
        };
        return Reply::failed(
            Diagnostic::new(&rule::CLI_SQL_ARGUMENTS, "`jadpo sql` needs a query")
                .reason(reason)
                .next(match names.first() {
                    Some(first) => format!("Name a query, for example `jadpo sql {first}`"),
                    None => "Declare a query in a domain's `queries/` folder".to_string(),
                }),
        );
    };
    let errors: Vec<Diagnostic> = jadpo_check::check(db, loaded.project)
        .into_iter()
        .filter(|d| d.rule.severity == Severity::Error)
        .collect();
    if !errors.is_empty() {
        return crate::migrate::blocked(&loaded, &cwd, errors, "sql not shown");
    }
    let Some(query) = queries.get(name) else {
        let diagnostic =
            Diagnostic::new(&rule::CLI_SQL_ARGUMENTS, format!("`{name}` isn't a query"));
        return Reply::failed(match nearest(name, names.iter().map(String::as_str)) {
            Some(near) => diagnostic
                .reason(format!(
                    "This project declares no query called `{name}`. Did you mean `{near}`?"
                ))
                .next(format!("Run `jadpo sql {near}`")),
            None => diagnostic
                .reason(format!(
                    "This project declares no query called `{name}`; its queries are {}.",
                    crate::run::both(&quoted(&names))
                ))
                .next("Name one of the project's queries"),
        });
    };
    let plan = Plan::build(db, loaded.project);
    let Some(planned) = plan.query_named(name) else {
        return Reply::failed(
            Diagnostic::new(&rule::CLI_SQL_ARGUMENTS, format!("`{name}` has no SQL"))
                .reason(
                    "The query checks, but no statement could be made for it: a mistake in Jadpo.",
                )
                .next("Report it to Jadpo's maintainers"),
        );
    };
    let line = jadpo_check::graph::graph(db, loaded.project)
        .get(&query.key)
        .and_then(|node| node.place.as_ref())
        .map_or(1, |place| place.lines.0);
    let root_shown = jadpo_db::shown_root(&cwd, &loaded.root);
    let file = jadpo_term::shown_path(root_shown.as_deref(), &query.path);
    render(query, planned, &format!("{file}:{line}"))
}

/// The statements, as a screen and as data.
fn render(query: &QueryModel, plan: &QueryPlan, place: &str) -> Reply {
    let mut screen = Screen::new();
    let mut statements = Vec::new();
    let count = 1 + count_includes(&plan.read.select.includes);
    let what = match query.cardinality.word() {
        "one" => format!("one {}", query.element.ty),
        "optional" => format!("at most one {}", query.element.ty),
        "list" => format!("a list of {}", query.element.ty),
        _ => format!("a page of {}", query.element.ty),
    };
    let names = Names {
        query,
        include: None,
    };
    statement(
        &mut screen,
        &mut statements,
        format!("query {}", query.name),
        vec![
            ("reads".to_string(), format!("{}, {what}", query.entity)),
            ("file".to_string(), place.to_string()),
            (
                "sends".to_string(),
                if count == 1 {
                    "1 statement".to_string()
                } else {
                    format!("{count} statements: this one, and one for each include")
                },
            ),
        ],
        &plan.read.select,
        &names,
    );
    includes(
        &mut screen,
        &mut statements,
        query,
        &query.includes,
        &plan.read.select.includes,
        &query.entity,
    );
    let data = Json::object([
        ("query", query.name.as_str().into()),
        ("file", query.path.as_str().into()),
        ("entity", query.entity.as_str().into()),
        ("cardinality", query.cardinality.word().into()),
        ("engine", "postgres".into()),
        ("statements", Json::array(statements)),
    ]);
    Reply::screen(screen, data)
}

fn count_includes(includes: &[IncludeRead]) -> usize {
    includes
        .iter()
        .map(|include| 1 + count_includes(&include.select.includes))
        .sum()
}

fn includes(
    screen: &mut Screen,
    statements: &mut Vec<Json>,
    query: &QueryModel,
    model: &[Include],
    plans: &[IncludeRead],
    host: &str,
) {
    for (include, plan) in model.iter().zip(plans) {
        let follows = match include.kind {
            IncludeKind::Parent => format!(
                "{host}.{}, the {} it points at",
                include.via, include.entity
            ),
            IncludeKind::Children => format!(
                "{}.{}, each {host}'s list of {} records",
                include.entity, include.via, include.entity
            ),
            IncludeKind::Child => format!(
                "{}.{}, at most one for each {host}",
                include.entity, include.via
            ),
        };
        let keys = match include.kind {
            IncludeKind::Parent => format!("each {host}'s {}", include.via),
            IncludeKind::Children | IncludeKind::Child => format!("each {host}'s id"),
        };
        let names = Names {
            query,
            include: Some(&keys),
        };
        statement(
            screen,
            statements,
            format!("include {}", include.name),
            vec![("follows".to_string(), follows)],
            &plan.select,
            &names,
        );
        self::includes(
            screen,
            statements,
            query,
            &include.includes,
            &plan.select.includes,
            &include.entity,
        );
    }
}

/// How a parameter's source reads.
struct Names<'a> {
    query: &'a QueryModel,
    /// For an include: what its keys are.
    include: Option<&'a str>,
}

impl Names<'_> {
    fn source(&self, source: &Source) -> String {
        match source {
            Source::Arg { index, path } => {
                let name = self
                    .query
                    .params
                    .get(*index as usize)
                    .map_or("?", |(name, _)| name.as_str());
                std::iter::once(name)
                    .chain(path.iter().map(String::as_str))
                    .collect::<Vec<_>>()
                    .join(".")
            }
            Source::Now => "clock.now".to_string(),
            Source::Keys => self.include.unwrap_or("the keys").to_string(),
            Source::Cursor(i) => {
                let field = self
                    .query
                    .order
                    .get(*i as usize)
                    .map_or("?", |order| order.field.as_str());
                format!("after: the cursor's {field}, or none")
            }
            Source::SizePlusOne => "the page size + 1, to see if more follow".to_string(),
        }
    }
}

fn statement(
    screen: &mut Screen,
    statements: &mut Vec<Json>,
    heading: String,
    mut fields: Vec<(String, String)>,
    select: &Select,
    names: &Names<'_>,
) {
    screen.push(Block::section(heading.clone(), select.sql.lines()));
    let mut params = Vec::new();
    for (i, param) in select.params.iter().enumerate() {
        let source = names.source(&param.source);
        let ty = pg_type(param.ty);
        fields.push((format!("${}", i + 1), format!("{source} · {ty}")));
        params.push(Json::object([
            ("name", format!("${}", i + 1).into()),
            ("from", source.into()),
            ("type", ty.into()),
        ]));
    }
    screen.push(Block::fields(fields));
    statements.push(Json::object([
        ("for", heading.into()),
        ("sql", select.sql.as_str().into()),
        ("params", Json::array(params)),
    ]));
}

/// A parameter's type as PostgreSQL names it.
fn pg_type(ty: SqlType) -> String {
    match ty {
        SqlType::Bool => "boolean".into(),
        SqlType::Int2 => "smallint".into(),
        SqlType::Int4 => "integer".into(),
        SqlType::Int8 => "bigint".into(),
        SqlType::Numeric => "numeric".into(),
        SqlType::Text => "text".into(),
        SqlType::Bytes => "bytea".into(),
        SqlType::Uuid => "uuid".into(),
        SqlType::Timestamp => "timestamptz".into(),
        SqlType::Date => "date".into(),
        SqlType::Time => "time".into(),
        SqlType::Json => "jsonb".into(),
        SqlType::Array(inner) => format!("{}[]", pg_type(*inner)),
    }
}

/// The names of the queries in the project around `cwd`, for Tab
/// completion: each query is its file in a domain's `queries/` folder.
pub fn query_names(cwd: &std::path::Path) -> Vec<String> {
    let Some(root) = jadpo_db::find_root(cwd) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let Ok(domains) = std::fs::read_dir(root.join("domains")) else {
        return out;
    };
    for domain in domains.filter_map(Result::ok) {
        let Ok(files) = std::fs::read_dir(domain.path().join("queries")) else {
            continue;
        };
        for file in files.filter_map(Result::ok) {
            let name = file.file_name().to_string_lossy().to_string();
            if let Some(stem) = name.strip_suffix(".jadpo")
                && !stem.ends_with(".test")
            {
                out.push(stem.to_string());
            }
        }
    }
    out.sort();
    out
}
