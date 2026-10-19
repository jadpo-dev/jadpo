//! `jadpo sql <query>` and `jadpo sql <action>` (area 6; area 14's
//! command table, "Write SQL"): the PostgreSQL a query sends, statement by
//! statement, each with the parameters that fill it, or the statements an
//! action's writes send. A query sends one statement, and one more for
//! each include however many records it is included in; an action sends
//! one for each `create`, `update`, `delete` and lifecycle transition it
//! writes, in the order they are written.
//!
//! The screen: a bold heading over the statement, whole and never cut,
//! then on the label grid what it does, where it is written, how many
//! statements there are and each `$n` with where its value comes from and
//! its type. A query's heading is `query <name>`, each include's `include
//! <field>`; an action's writes are `write <Entity>.<operation>`, and the
//! grid ends with what each refusal raises: the constraint and the
//! failure its `conflict:` arm names, the not-found failure, or the fault.
//! A transition whose `when:` can refuse a record shows the statement that
//! asks, under `when no row changed`, and a write policy limits the one
//! that asks whether policy refused the caller, under `when policy may
//! have refused` (area 9). A success prints nothing more, as
//! the design language's calm default asks; `--json` gives the same as
//! data.

use std::path::PathBuf;

use jadpo_check::data::queries::{Include, IncludeKind, QueryModel, query_models};
use jadpo_diag::{Diagnostic, rule};
use jadpo_graph::Kind;
use jadpo_graph::body::{Body, Expr, Node, SiteId};
use jadpo_plan::{CallablePlan, Decoded, Plan, QueryPlan, Raises, WriteSitePlan};
use jadpo_std::read::{IncludeRead, Select, Source, SqlType};
use jadpo_std::write::{WriteOp, WriteSource};
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
    let actions: Vec<String> = action_names(&root);
    let Some(name) = call.arguments.first() else {
        let reason = match (names.is_empty(), actions.is_empty()) {
            (true, true) => "It prints the statements a query or an action's writes send, and this project declares neither.".to_string(),
            _ => format!(
                "It prints the statements a query or an action's writes send; this project's queries are {}, and its actions {}.",
                listed(&names),
                listed(&actions)
            ),
        };
        return Reply::failed(
            Diagnostic::new(
                &rule::CLI_SQL_ARGUMENTS,
                "`jadpo sql` needs a query or an action",
            )
            .reason(reason)
            .next(match names.first().or(actions.first()) {
                Some(first) => format!("Name one, for example `jadpo sql {first}`"),
                None => "Declare a query in a domain's `queries/` folder".to_string(),
            }),
        );
    };
    let errors: Vec<Diagnostic> = jadpo_check::check(db, loaded.project);
    if !errors.is_empty() {
        return crate::migrate::blocked(&loaded, &cwd, errors, "sql not shown");
    }
    let root_shown = jadpo_db::shown_root(&cwd, &loaded.root);
    let Some(query) = queries.get(name) else {
        let plan = Plan::build(db, loaded.project);
        if let Some(action) = plan
            .callables()
            .iter()
            .find(|c| c.kind == Kind::Action && c.key.name() == name.as_str())
        {
            let file = jadpo_term::shown_path(root_shown.as_deref(), &action.file);
            return render_action(&plan, action, &file);
        }
        let diagnostic = Diagnostic::new(
            &rule::CLI_SQL_ARGUMENTS,
            format!("`{name}` isn't a query or an action"),
        );
        let all: Vec<&str> = names.iter().chain(&actions).map(String::as_str).collect();
        return Reply::failed(match nearest(name, all.iter().copied()) {
            Some(near) => diagnostic
                .reason(format!(
                    "This project declares no query or action called `{name}`. Did you mean `{near}`?"
                ))
                .next(format!("Run `jadpo sql {near}`")),
            None => diagnostic
                .reason(format!(
                    "This project declares no query or action called `{name}`; its queries are {}, and its actions {}.",
                    listed(&names),
                    listed(&actions)
                ))
                .next("Name one of the project's queries or actions"),
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
    let file = jadpo_term::shown_path(root_shown.as_deref(), &query.path);
    render(query, planned, &format!("{file}:{line}"))
}

/// Names as a list for a sentence: "`a` and `b`", or "none".
fn listed(names: &[String]) -> String {
    if names.is_empty() {
        "none".to_string()
    } else {
        crate::run::both(&quoted(names))
    }
}

// --- an action's writes -----------------------------------------------------------

/// The statements an action's writes send, as a screen and as data.
fn render_action(plan: &Plan, action: &CallablePlan, file: &str) -> Reply {
    let text = plan.text(&action.file).unwrap_or_default();
    let writes: Vec<(SiteId, &WriteSitePlan)> = action
        .sites
        .iter()
        .enumerate()
        .filter_map(|(at, site)| match &site.decoded {
            Decoded::Write { plan: Some(id), .. } => Some((SiteId(at as u32), plan.write(*id)?)),
            _ => None,
        })
        .collect();
    let name = action.key.name().to_string();
    if writes.is_empty() {
        return Reply::failed(
            Diagnostic::new(&rule::CLI_SQL_ARGUMENTS, format!("`{name}` writes nothing"))
                .reason(format!(
                    "`jadpo sql` shows the statements an action's writes send, and `{name}` has no `create`, `update`, `delete` or transition; the queries it calls are shown by name."
                ))
                .next("Name a query, or an action that writes"),
        );
    }
    let mut screen = Screen::new();
    let mut statements = Vec::new();
    let count = writes.len();
    for (i, (site, write)) in writes.iter().enumerate() {
        let body = action.body.as_ref();
        let found = body.site(*site);
        let Node::Expr(Expr::Write {
            entity,
            op,
            transition,
            by_id,
            conflicts,
            ..
        }) = &found.node
        else {
            continue;
        };
        let entity = entity.name().to_string();
        let operation = match transition {
            Some(key) => key.name().to_string(),
            None => format!("{entity}.{}", op.word()),
        };
        let target = if *by_id {
            format!("the {entity} with that id")
        } else {
            format!("the {entity} loaded earlier")
        };
        let does = match (op, transition) {
            (WriteOp::Create, _) => format!("creates a {entity}"),
            (WriteOp::Delete, Some(_)) => {
                format!("runs {operation} on {target}, which keeps the record")
            }
            (_, Some(_)) => format!("runs {operation} on {target}"),
            (WriteOp::Update, None) => format!("updates {target}"),
            (WriteOp::Delete, None) => format!("removes {target}"),
        };
        let inputs = |source: &WriteSource| input_name(source, write, body, text);
        let mut fields = vec![
            ("writes".to_string(), does),
            ("file".to_string(), format!("{file}:{}", found.line)),
        ];
        if i == 0 {
            fields.push((
                "sends".to_string(),
                if count == 1 {
                    "1 statement".to_string()
                } else {
                    format!("{count} statements, one for each write")
                },
            ));
        }
        let mut params = Vec::new();
        for (n, param) in write.plan.params.iter().enumerate() {
            let source = inputs(&param.source);
            let ty = pg_type(param.ty);
            fields.push((format!("${}", n + 1), format!("{source} · {ty}")));
            params.push(param_json(n, &source, &ty));
        }
        let mut refusals = Vec::new();
        for refusal in &write.plan.refusals {
            let constraint = &refusal.constraint;
            let raises = match &refusal.raises {
                Raises::Arm(at) => conflicts.get(*at as usize).map_or_else(
                    || "the failure its arm names".to_string(),
                    |arm| format!("{}, mapped in conflict:", arm.failure.written()),
                ),
                Raises::NotFound(missing) => plan
                    .types()
                    .failure(missing.failure)
                    .map_or_else(|| "not found".to_string(), |f| f.name.clone()),
                Raises::Gone(what) => format!("a fault: {what}"),
            };
            let shown = match constraint.strip_prefix("when: ") {
                Some(rule) => format!("when: of {rule} → {raises}"),
                None => format!("{constraint} → {raises}"),
            };
            fields.push(("refused".to_string(), shown));
            refusals.push(jadpo_term::Json::object([
                ("constraint", constraint.as_str().into()),
                ("raises", raises.into()),
            ]));
        }
        let not_found = write
            .plan
            .missing
            .as_ref()
            .and_then(|missing| plan.types().failure(missing.failure))
            .map(|failure| failure.name.clone());
        if let Some(failure) = &not_found {
            fields.push(("no row".to_string(), failure.clone()));
        }
        if write.plan.policy == Some(jadpo_std::write::PolicyRefusal::Create) {
            fields.push((
                "no row".to_string(),
                "policy refused the caller → not_permitted".to_string(),
            ));
        }
        let heading = format!("write {operation}");
        screen.push(Block::section(heading.clone(), write.plan.sql.lines()));
        screen.push(Block::fields(fields));
        let mut statement = vec![
            ("for", heading.as_str().into()),
            ("line", i64::from(found.line).into()),
            ("sql", write.plan.sql.as_str().into()),
            ("params", Json::array(params)),
            ("refusals", Json::array(refusals)),
        ];
        if let Some(failure) = &not_found {
            statement.push(("noRow", failure.as_str().into()));
        }
        if let Some(guard) = &write.plan.guard {
            let refused = write
                .plan
                .refusals
                .iter()
                .find(|refusal| refusal.constraint.starts_with("when: "))
                .and_then(|refusal| match refusal.raises {
                    Raises::Arm(at) => conflicts.get(at as usize),
                    _ => None,
                })
                .map_or_else(String::new, |arm| format!(" → {}", arm.failure.written()));
            let mut asked = vec![(
                "then".to_string(),
                format!("a row means the record is there, so when: refused it{refused}"),
            )];
            let mut guard_params = Vec::new();
            for (n, param) in guard.params.iter().enumerate() {
                let source = inputs(&param.source);
                let ty = pg_type(param.ty);
                asked.push((format!("${}", n + 1), format!("{source} · {ty}")));
                guard_params.push(param_json(n, &source, &ty));
            }
            screen.push(Block::section("when no row changed", guard.sql.lines()));
            screen.push(Block::fields(asked));
            statement.push((
                "whenNoRowChanged",
                Json::object([
                    ("sql", guard.sql.as_str().into()),
                    ("params", Json::array(guard_params)),
                ]),
            ));
        }
        if let Some(jadpo_std::write::PolicyRefusal::Probe {
            sql,
            params,
            fields: field_probe,
        }) = &write.plan.policy
        {
            let then = match &not_found {
                Some(failure) => format!(
                    "a row means policy refused the caller → {failure}, logged as concealed"
                ),
                None => "a row means policy refused the caller → not_permitted".to_string(),
            };
            let mut asked = vec![("then".to_string(), then)];
            let mut probe_params = Vec::new();
            for (n, param) in params.iter().enumerate() {
                let source = inputs(&param.source);
                let ty = pg_type(param.ty);
                asked.push((format!("${}", n + 1), format!("{source} · {ty}")));
                probe_params.push(param_json(n, &source, &ty));
            }
            screen.push(Block::section("when policy may have refused", sql.lines()));
            screen.push(Block::fields(asked));
            statement.push((
                "whenPolicyMayHaveRefused",
                Json::object([
                    ("sql", sql.as_str().into()),
                    ("params", Json::array(probe_params)),
                ]),
            ));
            if let Some(probe) = field_probe {
                // A reference into another tenant refuses it the same way
                // (area 9, "References stay inside the tenant").
                let refused_by = if write.plan.kept_in_tenant {
                    "a field's own policy or a reference into another tenant"
                } else {
                    "a field's own policy"
                };
                let mut asked = vec![(
                    "then".to_string(),
                    format!(
                        "a row means the record's grants let the caller in, so {refused_by} refused them → not_permitted"
                    ),
                )];
                let mut probe_params = Vec::new();
                for (n, param) in probe.params.iter().enumerate() {
                    let source = inputs(&param.source);
                    let ty = pg_type(param.ty);
                    asked.push((format!("${}", n + 1), format!("{source} · {ty}")));
                    probe_params.push(param_json(n, &source, &ty));
                }
                screen.push(Block::section(
                    "when a field may have refused",
                    probe.sql.lines(),
                ));
                screen.push(Block::fields(asked));
                statement.push((
                    "whenAFieldMayHaveRefused",
                    Json::object([
                        ("sql", probe.sql.as_str().into()),
                        ("params", Json::array(probe_params)),
                    ]),
                ));
            }
        }
        statements.push(Json::object(statement));
    }
    let data = Json::object([
        ("action", name.as_str().into()),
        ("file", action.file.as_str().into()),
        ("engine", "postgres".into()),
        ("statements", Json::array(statements)),
    ]);
    Reply::screen(screen, data)
}

fn param_json(n: usize, source: &str, ty: &str) -> Json {
    Json::object([
        ("name", format!("${}", n + 1).into()),
        ("from", source.into()),
        ("type", ty.into()),
    ])
}

/// Where a write's parameter comes from, as the action writes it.
fn input_name(source: &WriteSource, write: &WriteSitePlan, body: &Body, text: &str) -> String {
    let input = |at: u32| {
        write
            .inputs
            .get(at as usize)
            .map(|(site, _)| {
                let site = body.site(*site);
                text.get(site.start as usize..site.end as usize)
                    .unwrap_or("?")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_else(|| "?".to_string())
    };
    match source {
        WriteSource::Input(at) => input(*at),
        WriteSource::Field { input: at, field } => format!("{}.{field}", input(*at)),
        WriteSource::Supplied { input: at, field } => {
            format!("whether {} supplies {field}", input(*at))
        }
        WriteSource::AnySupplied { input: at, fields } => format!(
            "whether {} supplies any of {}",
            input(*at),
            fields.join(", ")
        ),
        WriteSource::NewId => "a new identity".to_string(),
        WriteSource::Now => "clock.now".to_string(),
        WriteSource::Caller(part) => caller(*part),
        WriteSource::Part { of, index } => {
            format!(
                "part {} of {}",
                index + 1,
                input_name(of, write, body, text)
            )
        }
        // A JSON-held value, sent as its JSON (chunk 5.5w2b).
        WriteSource::Json { of, .. } => {
            format!("{} as JSON", input_name(of, write, body, text))
        }
        WriteSource::Stored(value) => stored(value),
    }
}

/// How a literal or an enum value bound as a parameter reads: as SQL text
/// writes the value the storage codec made (chunk 5.5w3b).
fn stored(value: &jadpo_std::SqlValue) -> String {
    jadpo_sql::literal::sql(value).unwrap_or_else(|| "a literal".to_string())
}

/// How a caller parameter of a statement's policy reads (area 9).
fn caller(part: jadpo_std::policy::CallerPart) -> String {
    use jadpo_std::policy::CallerPart;
    match part {
        CallerPart::User => "the signed-in user's id, or none",
        CallerPart::Service => "the signed-in service's id, or none",
        CallerPart::Signed => "whether anyone is signed in",
    }
    .to_string()
}

/// The names of the actions in the project at `root`: each action is its
/// file in a domain's `actions/` folder.
fn action_names(root: &std::path::Path) -> Vec<String> {
    names_in(root, "actions")
}

/// The statements, as a screen and as data.
fn render(query: &QueryModel, plan: &QueryPlan, place: &str) -> Reply {
    let mut screen = Screen::new();
    let mut statements = Vec::new();
    let count = 1 + count_includes(&plan.read.select.includes);
    let what = match query.cardinality.word() {
        "one" => format!("one {}", query.element.ty),
        "optional" => format!("at most one {}", query.element.ty),
        // A nearest-neighbour query's records, each with its distance
        // (area 21, chunk 5.5v1b).
        "list" if query.nearest.is_some() => format!("a list of Nearest<{}>", query.element.ty),
        // A map layer's features (chunk 5.5u8).
        "list" if query.geometry.is_some() => format!(
            "a map layer, each feature's geometry its {} and properties a {}",
            query.geometry.as_deref().unwrap_or_default(),
            query.element.ty
        ),
        "list" => format!("a list of {}", query.element.ty),
        _ => format!("a page of {}", query.element.ty),
    };
    let names = Names {
        query,
        include: None,
    };
    let mut fields = vec![
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
    ];
    // What runs first in its transaction: pgvector's search settings.
    for setting in &plan.read.settings {
        fields.push(("first".to_string(), setting.clone()));
    }
    // What the runtime works out again on the rows found (area 20, chunk
    // 5.5u5).
    if let Some(recheck) = &plan.read.recheck {
        let mut then = Vec::new();
        if !recheck.checks.is_empty() {
            then.push("each row is held to its `geo.` conditions in Jadpo's own code".to_string());
        }
        if recheck.order.is_some() {
            then.push("sorted by `geo.distance` as the code works it out, then `id`".to_string());
        }
        fields.push(("then".to_string(), then.join("; ")));
    }
    statement(
        &mut screen,
        &mut statements,
        format!("query {}", query.name),
        fields,
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
        (
            "settings",
            Json::array(
                plan.read
                    .settings
                    .iter()
                    .map(|s| Json::from(s.as_str()))
                    .collect::<Vec<Json>>(),
            ),
        ),
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
            Source::Caller(part) => caller(*part),
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
            Source::Stored(value) => stored(value),
            Source::Fetch => {
                "as many rows as it keeps, more if the code's recheck leaves it short".to_string()
            }
            Source::Envelope { index, path, part } => {
                let bounds = self.source(&Source::Arg {
                    index: *index,
                    path: path.clone(),
                });
                let side = match part {
                    jadpo_std::read::EnvelopePart::FirstWest => "the first envelope's west",
                    jadpo_std::read::EnvelopePart::FirstEast => "the first envelope's east",
                    jadpo_std::read::EnvelopePart::SecondWest => "the second envelope's west",
                    jadpo_std::read::EnvelopePart::SecondEast => "the second envelope's east",
                };
                format!("{side}, from {bounds}")
            }
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
        SqlType::LocalTimestamp => "timestamp".into(),
        SqlType::Json => "jsonb".into(),
        SqlType::Vector => "vector".into(),
        SqlType::Geography => "geography".into(),
        SqlType::Float8 => "double precision".into(),
        SqlType::Array(inner) => format!("{}[]", pg_type(*inner)),
        SqlType::Range(inner) => match inner {
            SqlType::Date => "daterange".into(),
            SqlType::Timestamp => "tstzrange".into(),
            SqlType::Numeric => "numrange".into(),
            _ => "int8range".into(),
        },
    }
}

/// The names of the queries and actions in the project around `cwd`, for
/// Tab completion of `jadpo sql`.
pub fn query_and_action_names(cwd: &std::path::Path) -> Vec<(String, &'static str)> {
    let Some(root) = jadpo_db::find_root(cwd) else {
        return Vec::new();
    };
    let mut out: Vec<(String, &'static str)> = names_in(&root, "queries")
        .into_iter()
        .map(|name| (name, "Query"))
        .collect();
    out.extend(
        names_in(&root, "actions")
            .into_iter()
            .map(|name| (name, "Action")),
    );
    out.sort();
    out
}

/// The declarations in each domain's `folder`, by file name.
fn names_in(root: &std::path::Path, folder: &str) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(domains) = std::fs::read_dir(root.join("domains")) else {
        return out;
    };
    for domain in domains.filter_map(Result::ok) {
        let Ok(files) = std::fs::read_dir(domain.path().join(folder)) else {
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
