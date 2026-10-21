//! Area 12's schedules (chunk 6.2): each schedule read into a
//! [`ScheduleModel`] (how often it runs and the action of its own domain
//! it runs, with no arguments), `application { schedules: { store: } }`,
//! where every schedule's state and lease are kept, and the problems found
//! on the way.
//!
//! A schedule runs as the system with no tenant (area 9, `Access.system`;
//! area 12, "A schedule and tenants"), so what its action reaches that
//! only `Access.public` grants is `policy.system_grant_missing`, as for a
//! subscriber (decided 2026-10-19). Its state and lease go in the store
//! `schedules: { store: }` names, or the only database when there is one;
//! with several and none named, each schedule is
//! `schedule.store_required` (area 12, "Outbox placement, settled").

use std::collections::BTreeMap;
use std::sync::Arc;

use jadpo_db::{Db, File, ItemKind, Project, item_tree, location};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_graph::{Key, Kind};
use jadpo_syntax::ast::{self, AstNode, Decl, Expr};

use crate::lookups::{Lookup, NameView, SigView};
use crate::resolve::Names;
use crate::std_lib::StdNames;
use crate::tree::{self, node_span};

/// What a schedule holds, in the order `jadpo fmt` writes them.
pub const SCHEDULE_ITEMS: [&str; 2] = ["every", "run"];

/// The units `every:` is written in (area 12): seconds, minutes, hours or
/// days, a day being exactly 24 hours.
pub const EVERY_UNITS: [&str; 4] = ["s", "m", "h", "d"];

/// One schedule, without positions.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ScheduleModel {
    pub key: Key,
    pub name: String,
    /// Its project-relative file and the line it starts on.
    pub path: String,
    pub line: u32,
    /// `every:`, in milliseconds: a whole number of seconds above zero.
    pub every_ms: u64,
    /// What it runs.
    pub run: ScheduleRun,
}

/// What a schedule runs.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ScheduleRun {
    /// An action of its own domain, by key, called with nothing.
    Action(Key),
    /// The built-in purge of the entity of this name (area 5,
    /// "Lifecycle"): every entity whose lifecycle has `purge:` gets one,
    /// with nothing written for it (area 12, "Schedules"; chunk 6.2c).
    Purge(String),
}

impl ScheduleRun {
    /// The action it runs, when it runs one.
    pub fn action(&self) -> Option<&Key> {
        match self {
            ScheduleRun::Action(key) => Some(key),
            ScheduleRun::Purge(_) => None,
        }
    }
}

/// How often the built-in purge schedule runs: hourly (area 12).
pub const PURGE_EVERY_MS: u64 = 60 * 60 * 1000;

/// The built-in purge schedule of `entity`, when its lifecycle purges and
/// it has a store: `Member.purge`, keyed by the entity's born name so a
/// rename keeps its state and run records, declared where its `purge:`
/// is written.
fn purge_schedule(
    db: &dyn Db,
    project: Project,
    read: &crate::data::EntityRead,
) -> Option<ScheduleModel> {
    let entity = &read.model;
    entity.lifecycle.as_ref()?.purge.as_ref()?;
    entity.store.as_ref()?;
    let span = read.spans.purge.as_ref()?;
    Some(ScheduleModel {
        key: Key::new(Kind::Schedule, &format!("{}.purge", entity.born)),
        name: format!("{}.purge", entity.name),
        path: entity.path.clone(),
        line: crate::events::location_line(db, project, &entity.path, span.start),
        every_ms: PURGE_EVERY_MS,
        run: ScheduleRun::Purge(entity.name.clone()),
    })
}

/// Every entity's built-in purge schedule, by the entity's name, with
/// where its `purge:` is written.
#[salsa::tracked(returns(ref))]
pub fn purge_schedules(db: &dyn Db, project: Project) -> Vec<(Arc<ScheduleModel>, Span)> {
    crate::data::entity_names(db, project)
        .iter()
        .filter_map(|name| {
            let read =
                crate::data::entity_read(db, project, crate::lookups::Name::new(db, name.clone()))
                    .as_ref()?;
            let model = purge_schedule(db, project, read)?;
            Some((Arc::new(model), read.spans.purge.clone()?))
        })
        .collect()
}

/// One schedule as read, with its problems.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduleRead {
    pub name: String,
    pub model: Option<Arc<ScheduleModel>>,
    /// Where its declaration's name is, for a problem found later.
    pub span: Span,
    /// Where its `every:` value is written, when it is.
    pub every: Option<Span>,
    pub problems: Vec<Diagnostic>,
}

/// The schedules `file` declares, read.
#[salsa::tracked(returns(ref))]
pub fn schedules_in(db: &dyn Db, project: Project, file: File) -> Vec<ScheduleRead> {
    let items = item_tree(db, file);
    if !items
        .items
        .iter()
        .any(|item| item.kind == ItemKind::Schedule)
    {
        return Vec::new();
    }
    let Some(source) = tree::source(db, file) else {
        return Vec::new();
    };
    let path = file.path(db);
    let ownership = &location(db, project, file).ownership;
    let lookup = Lookup::within(db, project, ownership);
    let names = Names {
        path,
        ownership,
        table: &lookup,
        types: &lookup,
        std: StdNames::get(),
        library: None,
    };
    let mut out = Vec::new();
    for item in &items.items {
        if item.kind != ItemKind::Schedule {
            continue;
        }
        let Some(Decl::ScheduleDecl(decl)) = tree::decl_at(&source, item.index) else {
            continue;
        };
        let mut reader = Reader {
            db,
            project,
            path,
            lookup: &lookup,
            names: &names,
            domain: ownership.domain().map(str::to_string),
            out: Vec::new(),
        };
        let model = reader.schedule(&decl, &item.name, ownership);
        let span = decl.name().map_or_else(
            || node_span(path, decl.syntax()),
            |name| node_span(path, name.syntax()),
        );
        let every = decl
            .member_list()
            .iter()
            .flat_map(|list| list.member_items())
            .filter_map(|item| match item {
                ast::MemberItem::Member(member) => Some(member),
                _ => None,
            })
            .find(|member| {
                member
                    .member_key()
                    .is_some_and(|key| tree::written(key.syntax()) == "every")
            })
            .and_then(|member| member.value())
            .map(|value| node_span(path, value.syntax()));
        out.push(ScheduleRead {
            name: item.name.clone(),
            model: model.map(Arc::new),
            span,
            every,
            problems: reader.out,
        });
    }
    out
}

/// Every schedule that reads cleanly, the built-in purge schedules
/// among them, in key order.
#[salsa::tracked(returns(ref))]
pub fn schedules(db: &dyn Db, project: Project) -> Arc<Vec<Arc<ScheduleModel>>> {
    let mut out: Vec<Arc<ScheduleModel>> = project
        .files(db)
        .iter()
        .flat_map(|&file| schedules_in(db, project, file).iter())
        .filter_map(|read| read.model.clone())
        .chain(
            purge_schedules(db, project)
                .iter()
                .map(|(model, _)| Arc::clone(model)),
        )
        .collect();
    out.sort_by(|a, b| a.key.cmp(&b.key));
    out.dedup_by(|a, b| a.key == b.key);
    Arc::new(out)
}

/// Whether the project has a schedule at all: one it declares, read
/// cleanly or not, or an entity's built-in purge schedule.
#[salsa::tracked]
pub fn declares_schedules(db: &dyn Db, project: Project) -> bool {
    project.files(db).iter().any(|&file| {
        item_tree(db, file)
            .items
            .iter()
            .any(|item| item.kind == ItemKind::Schedule)
    }) || !purge_schedules(db, project).is_empty()
}

/// The problems of the schedules `file` declares: how each reads, where
/// its state is kept, then what its action reaches that only
/// `Access.public` grants (area 9).
#[salsa::tracked(returns(ref))]
pub fn schedule_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let settings = schedules_settings(db, project);
    for read in schedules_in(db, project, file) {
        out.extend(read.problems.iter().cloned());
        if settings.store.is_none() && settings.problems.is_empty() {
            out.push(store_required(db, project, &read.name, &read.span));
        }
        let Some(model) = &read.model else { continue };
        let Some(action) = model.run.action() else {
            continue;
        };
        if !crate::policy::policy_clean(db, project) {
            continue;
        }
        if let Some(found) = crate::policy::reach::public_only(db, project, action.name()) {
            out.push(crate::events::public_only(
                "schedule",
                &model.name,
                &read.span,
                &found,
            ));
        }
    }
    // An entity's built-in purge schedule keeps its state where every
    // schedule does.
    if settings.store.is_none() && settings.problems.is_empty() {
        let path = file.path(db);
        for (model, span) in purge_schedules(db, project) {
            if model.path == *path
                && let ScheduleRun::Purge(entity) = &model.run
            {
                out.push(purge_store_required(db, project, entity, span));
            }
        }
    }
    out
}

/// `schedule.store_required` for an entity's built-in purge schedule, at
/// its `purge:`.
fn purge_store_required(db: &dyn Db, project: Project, entity: &str, span: &Span) -> Diagnostic {
    let databases = crate::data::connections(db, project).databases();
    let listed = crate::and_list(
        &databases
            .iter()
            .map(|d| format!("`{d}`"))
            .collect::<Vec<_>>(),
    );
    // The database the entity is in is the one to suggest.
    let model = crate::data::data_model(db, project);
    let first = model
        .entity(entity)
        .and_then(|entity| entity.store.as_deref())
        .or(databases.first().map(String::as_str))
        .unwrap_or("main");
    Diagnostic::new(
        &rule::SCHEDULE_STORE_REQUIRED,
        format!("`{entity}`'s purge has no database named to keep its state in"),
    )
    .at(span.clone(), "a schedule with no store for its state")
    .reason(format!(
        "Purge runs as a built-in schedule every hour, and each schedule keeps its next due time and the lease that lets one instance run it at a time in a database; this application has {listed}, and `app.jadpo` names none with `schedules: {{ store: }}`."
    ))
    .next(format!("Add `schedules: {{ store: {first} }}` to `application {{ }}` in `app.jadpo`"))
}

/// `schedule.store_required`: no database is named to keep a schedule's
/// state and lease in, and there isn't exactly one to default to.
fn store_required(db: &dyn Db, project: Project, name: &str, span: &Span) -> Diagnostic {
    let databases = crate::data::connections(db, project).databases();
    let listed = crate::and_list(
        &databases
            .iter()
            .map(|d| format!("`{d}`"))
            .collect::<Vec<_>>(),
    );
    let (message, reason, next) = match databases.first() {
        Some(first) => (
            format!("`{name}` has no database named to keep its state in"),
            format!(
                "Each schedule keeps its next due time and the lease that lets one instance run it at a time in a database; this application has {listed}, and `app.jadpo` names none with `schedules: {{ store: }}`."
            ),
            format!("Add `schedules: {{ store: {first} }}` to `application {{ }}` in `app.jadpo`"),
        ),
        None => (
            format!("`{name}` has no database to keep its state in"),
            "Each schedule keeps its next due time and the lease that lets one instance run it at a time in a database, and this application has none.".to_string(),
            "Add a PostgreSQL connection under `connections:` in `app.jadpo`".to_string(),
        ),
    };
    Diagnostic::new(&rule::SCHEDULE_STORE_REQUIRED, message)
        .at(span.clone(), "no store for its state")
        .reason(reason)
        .next(next)
}

/// `application { schedules: { store: } }`, read (area 12).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SchedulesRead {
    /// The store every schedule's state and lease are kept in: the one
    /// named, or the only database. `None` with several databases and
    /// none named, or none at all.
    pub store: Option<String>,
    pub problems: Vec<Diagnostic>,
}

/// The project's `schedules:` settings.
#[salsa::tracked(returns(ref))]
pub fn schedules_settings(db: &dyn Db, project: Project) -> Arc<SchedulesRead> {
    let mut out = SchedulesRead::default();
    let connections = crate::data::connections(db, project);
    let databases = connections.databases();
    if let [only] = databases.as_slice() {
        out.store = Some(only.clone());
    }
    let Some(file) = project
        .files(db)
        .iter()
        .find(|file| file.path(db) == jadpo_db::PROJECT_FILE)
    else {
        return Arc::new(out);
    };
    let Some(source) = tree::source(db, *file) else {
        return Arc::new(out);
    };
    let path = file.path(db);
    for decl in source.decls() {
        let Decl::SingletonDecl(singleton) = decl else {
            continue;
        };
        if singleton.keyword().map(|t| t.text().to_string()).as_deref() != Some("application") {
            continue;
        }
        for (key, member) in tree::members(singleton.member_list()) {
            if tree::name_text(&key).as_deref() != Some("schedules") {
                continue;
            }
            let block = member.block();
            if block.is_none() {
                out.problems.push(
                    Diagnostic::new(&rule::SCHEDULE_SETTINGS_INVALID, "`schedules:` holds no block")
                        .at(node_span(path, member.syntax()), "not a block")
                        .reason("`schedules: { store: <connection> }` names the database every schedule's state and lease are kept in.")
                        .next("Write `schedules: { store: main }`, naming a database connection"),
                );
            }
            for (setting, entry) in tree::members(block) {
                let name = tree::name_text(&setting).unwrap_or_default();
                if name != "store" {
                    out.problems.push(
                        Diagnostic::new(
                            &rule::SCHEDULE_SETTINGS_INVALID,
                            format!("`{name}` isn't a schedules setting"),
                        )
                        .at(node_span(path, setting.syntax()), "not a setting")
                        .reason("`schedules:` holds one setting, `store:`, the database every schedule's state and lease are kept in.")
                        .next(format!("Remove `{name}:`")),
                    );
                    continue;
                }
                let written = entry
                    .value()
                    .map(|value| tree::written(value.syntax()))
                    .unwrap_or_default();
                if connections.kind(&written) == Some(crate::data::ConnectionKind::Postgres) {
                    out.store = Some(written);
                    continue;
                }
                let span = entry.value().map_or_else(
                    || node_span(path, entry.syntax()),
                    |v| node_span(path, v.syntax()),
                );
                let mut problem = Diagnostic::new(
                    &rule::SCHEDULE_SETTINGS_INVALID,
                    format!("`store: {written}` isn't a database"),
                )
                .at(span.clone(), "not a database")
                .reason(format!(
                    "A schedule's next due time and its lease are kept in a database; this application's databases are {}.",
                    crate::and_list(&databases.iter().map(|d| format!("`{d}`")).collect::<Vec<_>>())
                ));
                problem = match nearest(&written, databases.iter().map(String::as_str)) {
                    Some(near) => problem
                        .next(format!("Use `{near}` instead of `{written}`"))
                        .edit(span, near),
                    None => problem.next("Name a database connection"),
                };
                out.problems.push(problem);
            }
        }
    }
    Arc::new(out)
}

/// Reads one schedule.
struct Reader<'a> {
    db: &'a dyn Db,
    project: Project,
    path: &'a str,
    lookup: &'a Lookup<'a>,
    names: &'a Names<'a>,
    /// The schedule's own domain, whose action it runs.
    domain: Option<String>,
    out: Vec<Diagnostic>,
}

impl Reader<'_> {
    fn span(&self, node: &impl AstNode) -> Span {
        node_span(self.path, node.syntax())
    }

    fn schedule(
        &mut self,
        decl: &ast::ScheduleDecl,
        name: &str,
        ownership: &jadpo_db::Ownership,
    ) -> Option<ScheduleModel> {
        let mut items: BTreeMap<String, ast::Member> = BTreeMap::new();
        for item in decl
            .member_list()
            .iter()
            .flat_map(|list| list.member_items())
        {
            let ast::MemberItem::Member(member) = item else {
                self.out.push(self.item_invalid(
                    self.span(&item),
                    "This isn't one of a schedule's items".to_string(),
                    "not an item",
                ));
                continue;
            };
            let Some(key) = member.member_key() else {
                continue;
            };
            let written = tree::written(key.syntax());
            if !SCHEDULE_ITEMS.contains(&written.as_str()) {
                let near = nearest(&written, SCHEDULE_ITEMS.iter().copied());
                let mut problem = self.item_invalid(
                    self.span(&key),
                    format!("`{written}` isn't something a schedule holds"),
                    "not a schedule item",
                );
                if let Some(near) = near {
                    problem = problem
                        .next(format!("Write `{near}:` instead of `{written}:`"))
                        .edit(self.span(&key), near);
                }
                self.out.push(problem);
                continue;
            }
            if items.contains_key(&written) {
                self.out.push(self.item_invalid(
                    self.span(&key),
                    format!("`{written}:` is given twice"),
                    "given again",
                ));
                continue;
            }
            items.insert(written, member);
        }
        let head = decl
            .name()
            .map_or_else(|| self.span(decl), |name| self.span(&name));
        for required in SCHEDULE_ITEMS {
            if !items.contains_key(required) {
                self.out.push(self.item_invalid(
                    head.clone(),
                    format!("`{name}` has no `{required}:`"),
                    &format!("no `{required}:`"),
                ));
            }
        }
        let every = items.get("every").and_then(|member| self.every(member));
        let run = items.get("run").and_then(|member| self.run(member));
        let (every_ms, run) = (every?, run?);
        let registry = crate::graph::registry(self.db, self.project);
        let key = crate::graph::item_key(
            registry,
            Kind::Schedule,
            &ItemKind::Schedule,
            name,
            ownership,
        );
        let line = decl.syntax().text_range().start().into();
        let line = crate::events::location_line(self.db, self.project, self.path, line);
        Some(ScheduleModel {
            key,
            name: name.to_string(),
            path: self.path.to_string(),
            line,
            every_ms,
            run: ScheduleRun::Action(run),
        })
    }

    fn item_invalid(&self, span: Span, message: String, label: &str) -> Diagnostic {
        Diagnostic::new(&rule::SCHEDULE_ITEM_INVALID, message)
            .at(span, label.to_string())
            .reason("A schedule says how often it runs with `every:` and the action it runs with `run:`; nothing else, since one run at a time, merged missed ticks and the next tick as the retry are how every schedule runs.")
            .next("Write `every: <duration>` and `run: <action>()`, once each")
    }

    /// `every:`: a whole number of seconds, minutes, hours or days, above
    /// zero.
    fn every(&mut self, member: &ast::Member) -> Option<u64> {
        let value = member.value();
        let written = value
            .as_ref()
            .map(|v| tree::written(v.syntax()))
            .unwrap_or_default();
        let span = value
            .as_ref()
            .map_or_else(|| self.span(member), |v| self.span(v));
        let unit_ok = EVERY_UNITS.iter().any(|unit| {
            written
                .strip_suffix(unit)
                .is_some_and(|n| !n.ends_with('m'))
        });
        let millis = jadpo_std::Dur::parse(&written)
            .filter(|_| unit_ok)
            .and_then(|dur| u64::try_from(dur.millis).ok())
            .filter(|&millis| millis > 0 && i64::try_from(millis).is_ok());
        if millis.is_none() {
            self.out.push(
                Diagnostic::new(
                    &rule::SCHEDULE_EVERY_INVALID,
                    format!("`every: {written}` can't be how often a schedule runs"),
                )
                .at(span, "not seconds, minutes, hours or days")
                .reason("`every:` is a whole number of seconds, minutes, hours or days above zero (`30s`, `15m`, `1h`, `1d`), counted in UTC from when the application starts.")
                .next("Write a duration such as `every: 15m`"),
            );
        }
        millis
    }

    /// `run:`: a call of an action in the schedule's own domain, which
    /// takes nothing.
    fn run(&mut self, member: &ast::Member) -> Option<Key> {
        let value = member.value();
        let Some(Expr::CallExpr(call)) = value.clone() else {
            let span = value
                .as_ref()
                .map_or_else(|| self.span(member), |v| self.span(v));
            let written = value
                .as_ref()
                .map(|v| tree::written(v.syntax()))
                .unwrap_or_default();
            self.out.push(run_invalid(
                span,
                format!("`run: {written}` isn't a call"),
                "not a call",
            ));
            return None;
        };
        let mut problems = Vec::new();
        self.names.callee(&call, false, &mut problems);
        if !problems.is_empty() {
            self.out.extend(problems);
            return None;
        }
        let callee = call.expr()?;
        let name = match &callee {
            Expr::NameRef(name) => tree::name_ref_text(name)?,
            Expr::FieldExpr(_) => {
                let (_, rest) = tree::dotted(&callee)?;
                tree::name_ref_text(rest.last()?)?
            }
            _ => return None,
        };
        let symbol = self.lookup.declared(&name).first()?.clone();
        let same_domain = symbol.ownership.domain().map(str::to_string) == self.domain;
        if symbol.kind != ItemKind::Action || !same_domain {
            let what = if symbol.kind == ItemKind::Action {
                format!(
                    "an action of domain `{}`",
                    symbol.ownership.domain().unwrap_or("the root")
                )
            } else {
                symbol.kind.with_a().to_string()
            };
            self.out.push(
                run_invalid(self.span(&callee), format!("`{name}` is {what}"), &what).next(
                    format!(
                        "Run an action of this domain, calling `{name}` from it if it is another domain's public action"
                    ),
                ),
            );
            return None;
        }
        let written = call.arg_list().iter().flat_map(|list| list.args()).count();
        if written > 0 {
            let span = call
                .arg_list()
                .map_or_else(|| self.span(&callee), |list| self.span(&list));
            self.out.push(
                run_invalid(
                    span,
                    format!("A schedule passes nothing to `{name}`"),
                    "arguments given",
                )
                .next(format!(
                    "Write `run: {name}()`, and let `{name}` read `clock.now` and its queries for what it needs"
                )),
            );
            return None;
        }
        let sig = self.lookup.signature(&name)?.clone();
        if !sig.params.is_empty() {
            self.out.push(
                run_invalid(
                    self.span(&callee),
                    format!("`{name}` takes parameters, which a schedule has nothing to pass"),
                    "takes parameters",
                )
                .next(format!(
                    "Run an action that takes nothing, working out what `{name}` needs inside it"
                )),
            );
            return None;
        }
        crate::graph::declaration_key(self.lookup, &name)
    }
}

/// `schedule.run_invalid`, with the shared reason and a default next step.
fn run_invalid(span: Span, message: String, label: &str) -> Diagnostic {
    Diagnostic::new(&rule::SCHEDULE_RUN_INVALID, message)
        .at(span, label.to_string())
        .reason("`run:` calls one action of the schedule's own domain with no arguments; the action reads `clock.now`, fixed for the run, and queries for anything else, so all the logic lives in the action.")
        .next("Write `run: <action>()`")
}
