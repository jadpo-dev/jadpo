//! Making a constant a setting (area 10, "Making a constant a setting",
//! decided 2026-10-16; built in chunk 5.3f): `jadpo config move
//! <constant>` and the editor's "Make this a setting" both run this code,
//! so they move the same way and refuse the same things.
//!
//! The move happens in two steps over a loaded project. [`plan`] reads the
//! project and decides: the setting's name, type, `default:` and `///`
//! description, every read of the constant, and the refusals, which edit
//! nothing: a type a setting can't hold, a name a setting already has, and
//! a read where a setting can't be (a query, an update by `id`, a backfill
//! or another constant's value). [`apply`] then edits the loaded project's
//! texts: it adds the field to `config { }` in name order (or fills the
//! `default:` of the setting that names the constant), rewrites each read
//! to `config.<name>`, fixes the imports of every file it touched, deletes
//! the constant (and a library or a file it leaves empty), formats each file
//! that was formatted, and checks the result. A move whose result would not
//! check is undone and refused with the errors it would cause. The command
//! writes what [`apply`] gives back to disk; the editor sends it as one
//! workspace edit and puts the server's texts back with [`restore`].

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, File, Loaded, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_graph::Key;
use jadpo_graph::body::{Expr as BodyExpr, Node, Refers};
use jadpo_syntax::SyntaxKind::*;
use jadpo_syntax::ast::{self, AstNode, Decl, Expr, MemberItem};
use jadpo_syntax::{SyntaxNode, SyntaxToken};

use crate::constants::{ConstantId, constant_table};
use crate::lookups::{Lookup, NameView, SigView};
use crate::tree::{self, dotted, name_text, node_span};
use crate::types::{Const, callables_of, typed_body};

/// Where a new `config { }` goes when the project has none (area 10).
pub const CONFIG_FILE: &str = "config.jadpo";

/// The constant that `names` (`largest_page`, `security.max_login_attempts`)
/// reads, as `lookup`'s reader sees it.
pub(crate) fn names_constant(lookup: &Lookup<'_>, names: &[String]) -> Option<ConstantId> {
    match names {
        [name] => crate::constants::find(lookup, None, name),
        [library, name] if lookup.library(library).is_some() => {
            crate::constants::find(lookup, Some(library), name)
        }
        _ => None,
    }
}

/// One read of the constant: where it is, and the line it is on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Read {
    pub path: String,
    pub start: u32,
    pub end: u32,
    pub line: u32,
}

/// Where a read is, which decides whether a setting can be read there.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Place {
    /// Anywhere a setting can be read: rewritten to `config.<name>`.
    Code,
    Query,
    UpdateById,
    Backfill,
    /// The value of the constant named.
    Constant(String),
}

/// What [`plan`] decided.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub constant: ConstantId,
    /// The constant as code reads it: `security.max_login_attempts`, or
    /// `free_delivery_from` beside its use.
    pub shown: String,
    /// The setting it becomes.
    pub setting: String,
    /// The environment variable the setting reads.
    pub variable: String,
    /// Its `default:`, written as a literal.
    pub default: String,
    /// Each read, rewritten to `config.<setting>`, in file and source order.
    pub reads: Vec<Read>,
    /// The setting's type as the constant's declaration writes it.
    ty: String,
    /// The constant's `///` lines, as written.
    doc: Vec<String>,
    /// The setting whose `default:` names the constant, and that value's
    /// span: it becomes the constant's setting.
    fills: Option<Span>,
    /// Where the field goes: the file holding `config { }`, if any.
    config: Option<String>,
}

/// A change to one file, with the lines it shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    /// Its text before the move; `None` for a file the move made.
    pub before: Option<String>,
    /// Its text after; `None` for a file the move deleted.
    pub after: Option<String>,
    pub rows: Vec<Row>,
}

/// One line of what changed in a file: added, changed or removed, what,
/// and a note.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub mark: Mark,
    pub subject: String,
    pub note: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Added,
    Changed,
    Removed,
}

/// What [`apply`] did: every file it changed, the setting's file first,
/// then the constant's, then the readers' in path order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Moved {
    pub files: Vec<FileChange>,
}

/// A move [`apply`] undid: the errors its result would have, and the
/// texts their positions are in.
#[derive(Clone, Debug)]
pub struct Stopped {
    pub problems: Vec<Diagnostic>,
    pub sources: BTreeMap<String, String>,
}

// --- naming the constant --------------------------------------------------------

/// The constant `written` names on the command line: `name`,
/// `library.name`, or either after its domain (`accounts.security.name`),
/// as its identity key writes it.
pub fn find(db: &dyn Db, project: Project, written: &str) -> Result<ConstantId, Box<Diagnostic>> {
    let table = constant_table(db, project);
    let mut found: Vec<(&ConstantId, String)> = Vec::new();
    let mut all: Vec<String> = Vec::new();
    for (id, evaluated) in table {
        let domain = domain_of(db, project, &id.path);
        let mut names = vec![evaluated.name.clone()];
        if let Some(domain) = &domain {
            names.push(format!("{domain}.{}", evaluated.name));
        }
        all.push(evaluated.name.clone());
        if names.iter().any(|name| name == written) {
            found.push((id, names.last().cloned().unwrap_or_default()));
        }
    }
    match found.as_slice() {
        [(id, _)] => Ok((*id).clone()),
        [] => {
            let diagnostic = Diagnostic::new(
                &rule::CLI_CONFIG_ARGUMENTS,
                format!("`{written}` isn't a constant"),
            );
            Err(Box::new(match nearest(written, all.iter().map(String::as_str)) {
                Some(near) => diagnostic
                    .reason(format!(
                        "`jadpo config move` makes a constant a setting, and no constant is called `{written}`. Did you mean `{near}`?"
                    ))
                    .next(format!("Use `{near}` instead of `{written}`")),
                None if all.is_empty() => diagnostic
                    .reason("`jadpo config move` makes a constant a setting, and the project declares no constants.")
                    .next("Name a constant declared with `constant name: Type = value`"),
                None => diagnostic
                    .reason(format!(
                        "`jadpo config move` makes a constant a setting, and no constant is called `{written}`; a library's is named after its library, `library.name`."
                    ))
                    .next("Name the constant as code reads it: `name`, or `library.name` for a library's"),
            }))
        }
        many => {
            let names: Vec<String> = many.iter().map(|(_, full)| format!("`{full}`")).collect();
            Err(Box::new(
                Diagnostic::new(
                    &rule::CLI_CONFIG_ARGUMENTS,
                    format!("`{written}` names more than one constant"),
                )
                .reason(format!(
                    "Constants private to different domains may share a name: {}.",
                    crate::and_list(&names)
                ))
                .next("Name the one to move after its domain, as listed"),
            ))
        }
    }
}

/// The constant whose declaration's name, or one of whose reads, is at
/// byte `offset` of `file`: where the editor offers "Make this a setting".
pub fn at(db: &dyn Db, project: Project, file: File, offset: u32) -> Option<ConstantId> {
    let path = file.path(db).clone();
    let source = tree::source(db, file)?;
    // On a declaration's name.
    for id in crate::constants::constants_of(db, file) {
        let span = tree::name_span(&source, &path, id.index, id.member);
        if span.start <= offset && offset <= span.end {
            return Some(id);
        }
    }
    // On a read.
    let (crate::graph::Found::Node(found), _) = crate::graph::lookup(db, project, file, offset)?
    else {
        return None;
    };
    constant_table(db, project)
        .keys()
        .find(|id| key_of(db, project, id).as_ref() == Some(&found))
        .cloned()
}

fn domain_of(db: &dyn Db, project: Project, path: &str) -> Option<String> {
    let file = project
        .files(db)
        .iter()
        .find(|file| file.path(db) == path)?;
    jadpo_db::location(db, project, *file)
        .ownership
        .domain()
        .map(str::to_string)
}

/// The constant's key in the graph, which a body's names refer to.
fn key_of(db: &dyn Db, project: Project, id: &ConstantId) -> Option<Key> {
    let evaluated = constant_table(db, project).get(id)?;
    let file = file_at(db, project, &id.path)?;
    let lookup = Lookup::for_file(db, project, file);
    let (library, name) = split(&evaluated.name);
    crate::graph::constant_key(&lookup, library, name)
}

fn split(name: &str) -> (Option<&str>, &str) {
    match name.split_once('.') {
        Some((library, name)) => (Some(library), name),
        None => (None, name),
    }
}

fn file_at(db: &dyn Db, project: Project, path: &str) -> Option<File> {
    project
        .files(db)
        .iter()
        .copied()
        .find(|file| file.path(db) == path)
}

/// The constant's declaration in its file.
fn declaration(source: &ast::SourceFile, id: &ConstantId) -> Option<ast::ConstantDecl> {
    let decl = tree::decl_at(source, id.index)?;
    match (decl, id.member) {
        (Decl::ConstantDecl(constant), None) => Some(constant),
        (Decl::LibraryDecl(library), Some(member)) => {
            match tree::library_members(&library)
                .into_iter()
                .nth(member as usize)?
            {
                Decl::ConstantDecl(constant) => Some(constant),
                _ => None,
            }
        }
        _ => None,
    }
}

// --- deciding ----------------------------------------------------------------

/// The problems that leave a project still movable: a `default:` naming
/// this constant, which the move answers. Every other error stops it.
pub fn answered(db: &dyn Db, project: Project, id: &ConstantId, problem: &Diagnostic) -> bool {
    problem.rule.id == rule::CONFIGURATION_DEFAULT_LITERAL_REQUIRED.id
        && problem.primary.as_ref().is_some_and(|label| {
            fill_target(db, project, id).is_some_and(|(span, _)| span == label.span)
        })
}

/// The one setting whose `default:` names constant `id`: that value's span
/// and the setting's name.
fn fill_target(db: &dyn Db, project: Project, id: &ConstantId) -> Option<(Span, String)> {
    let mut found = Vec::new();
    for config in crate::config::singletons(db, project, "config") {
        let Some(file) = file_at(db, project, &config.path) else {
            continue;
        };
        let lookup = Lookup::for_file(db, project, file);
        for (name, member) in tree::members(config.list.clone()) {
            let Some(setting) = name_text(&name) else {
                continue;
            };
            for (key, option) in tree::members(options(&member)) {
                if name_text(&key).as_deref() != Some("default") {
                    continue;
                }
                let Some(value) = option.value() else {
                    continue;
                };
                if let Const::Path(names) = Const::from_expr(&value)
                    && names_constant(&lookup, &names).as_ref() == Some(id)
                {
                    found.push((node_span(&config.path, value.syntax()), setting.clone()));
                }
            }
        }
    }
    (found.len() == 1).then(|| found.remove(0))
}

/// A setting's options: `Type { default: ... }` is read as a record whose
/// fields are the options.
fn options(member: &ast::Member) -> Option<ast::MemberList> {
    match member.value()? {
        Expr::RecordExpr(record) => record.member_list(),
        _ => member.block(),
    }
}

/// Decides the move of constant `id`, or refuses it with every reason it
/// can't be made, editing nothing. The project must check, apart from a
/// `default:` naming the constant ([`answered`]).
pub fn plan(db: &dyn Db, project: Project, id: &ConstantId) -> Result<Plan, Vec<Diagnostic>> {
    let table = constant_table(db, project);
    let evaluated = table.get(id).ok_or_else(Vec::new)?;
    let file = file_at(db, project, &id.path).ok_or_else(Vec::new)?;
    let source = tree::source(db, file).ok_or_else(Vec::new)?;
    let decl = declaration(&source, id).ok_or_else(Vec::new)?;
    let value = evaluated.value.clone().ok_or_else(Vec::new)?;
    let (library, name) = split(&evaluated.name);
    let lookup = Lookup::for_file(db, project, file);
    let ty = lookup.constant_type(library, name).cloned();
    let ty_node = decl.ty();
    let ty_written = ty_node
        .as_ref()
        .map(|ty| tree::written(ty.syntax()))
        .unwrap_or_default();
    let shown = evaluated.name.clone();
    let fills = fill_target(db, project, id);
    let setting = fills
        .as_ref()
        .map_or_else(|| name.to_string(), |(_, setting)| setting.clone());
    let mut refusals = Vec::new();

    // A type a setting can hold.
    if let Some(ty) = &ty
        && let Err(why) = crate::config::holds(db, project, ty)
    {
        let at = ty_node.as_ref().map_or_else(
            || node_span(&id.path, decl.syntax()),
            |t| node_span(&id.path, t.syntax()),
        );
        let why = if why.is_empty() {
            String::new()
        } else {
            format!(" {why}")
        };
        refusals.push(
            Diagnostic::new(
                &rule::CONFIGURATION_MOVE_TYPE_UNSUPPORTED,
                format!("`{shown}` can't be a setting"),
            )
            .at(at, "not read from one variable")
            .reason(format!(
                "A setting is read from one environment variable, so its type must be decodable from one piece of text: text-based types, `Int`, `Decimal`, `Bool`, `Uuid`, `Email`, `Url`, `IpAddress`, `Duration`, `Instant`, `CalendarDate`, `ModelId` and enums.{why}"
            ))
            .next(format!(
                "Keep `{name}` a constant, or split it into settings of those types by hand"
            )),
        );
    }

    // A name no setting has.
    let config = crate::config::config(db, project);
    if fills.is_none() && config.written.iter().any(|written| written == name) {
        refusals.push(
            Diagnostic::new(
                &rule::CONFIGURATION_MOVE_NAME_TAKEN,
                format!("A setting is already called `{name}`"),
            )
            .at(tree::name_span(&source, &id.path, id.index, id.member), "a setting has this name")
            .reason(format!(
                "`config {{ }}` declares `{name}` already, so `{shown}` can't become `config.{name}` without two settings of one name."
            ))
            .next(format!(
                "Rename the constant or the setting `{name}`, then run `jadpo config move` again"
            )),
        );
    }

    // Reads where a setting can be.
    let key = key_of(db, project, id);
    let mut reads = Vec::new();
    for (read, place) in reads_of(db, project, id, key.as_ref()) {
        let span = Span::new(&read.path, read.start, read.end);
        let (label, why, next) = match &place {
            Place::Code => {
                reads.push(read);
                continue;
            }
            Place::Query => (
                "in a query",
                "A query's values are its parameters, literals, constants, enum values and `clock.now`, and a setting is none of those.".to_string(),
                "Pass the value to the query as a parameter".to_string(),
            ),
            Place::UpdateById => (
                "in an update by `id`",
                "An update by `id` is worked out by the database in one step, from the action's parameters, constants, `clock.now` and the record's stored fields, and a setting is none of those.".to_string(),
                "Pass the value to the action as a parameter, or load the record and update it".to_string(),
            ),
            Place::Backfill => (
                "in a backfill",
                "A backfill is worked out by the database when the migration runs, from the row's stored fields, constants and pure functions, and a setting is none of those.".to_string(),
                "Write the value out in the backfill".to_string(),
            ),
            Place::Constant(other) => (
                "in a constant's value",
                format!("`{other}`'s value is worked out while compiling, and a setting is known only when the application starts."),
                format!("Write `{other}`'s value out, or make `{other}` a setting first"),
            ),
        };
        refusals.push(
            Diagnostic::new(
                &rule::CONFIGURATION_MOVE_READ_UNSUPPORTED,
                format!("`{shown}` is read {label}"),
            )
            .at(span, label)
            .reason(format!(
                "{why} Moving `{shown}` would make this read `config.{setting}`."
            ))
            .next(format!(
                "{next}, then run `jadpo config move {shown}` again"
            )),
        );
    }
    if !refusals.is_empty() {
        return Err(refusals);
    }

    let doc = jadpo_syntax::ast::doc_comments(decl.syntax())
        .iter()
        .map(|token| token.text().trim_end().to_string())
        .collect();
    let default = default_literal(decl.value().as_ref(), &value);
    let config_file = crate::config::singletons(db, project, "config")
        .into_iter()
        .next()
        .map(|found| found.path);
    Ok(Plan {
        constant: id.clone(),
        shown,
        variable: jadpo_std::config::derived_variable(&setting),
        setting,
        default,
        reads,
        ty: ty_written,
        doc,
        fills: fills.map(|(span, _)| span),
        config: config_file,
    })
}

/// The constant's value as a literal: as written when it is one, otherwise
/// the value the compiler worked out.
fn default_literal(written: Option<&Expr>, value: &crate::constants::Value) -> String {
    if let Some(expr) = written {
        let literal = match Const::from_expr(expr) {
            Const::Number { .. } | Const::Str(_) | Const::Bool(_) | Const::Duration(_) => true,
            // An enum's variant, `Verbosity.normal`.
            Const::Path(names) => {
                names.len() == 2 && names[0].chars().next().is_some_and(char::is_uppercase)
            }
            _ => false,
        };
        if literal {
            return tree::written(expr.syntax());
        }
    }
    value.written()
}

/// Every read of constant `id` (`key` in the graph), with where it is, in
/// file and source order.
fn reads_of(
    db: &dyn Db,
    project: Project,
    id: &ConstantId,
    key: Option<&Key>,
) -> Vec<(Read, Place)> {
    let table = constant_table(db, project);
    let mut files: Vec<File> = project.files(db).clone();
    files.sort_by(|a, b| a.path(db).cmp(b.path(db)));
    let mut out = Vec::new();
    for file in files {
        let path = file.path(db).clone();
        let Some(source) = tree::source(db, file) else {
            continue;
        };
        let text = file.text(db);
        let mut found: Vec<(u32, u32, Place)> = Vec::new();
        let mut bodies: Vec<(u32, u32)> = Vec::new();
        // In bodies, from what the check resolved each name to.
        for callable in callables_of(db, file) {
            let index = callable.index(db);
            let function = callable.function(db);
            let Some(node) = callable_node(&source, index, function) else {
                continue;
            };
            let range = node.text_range();
            bodies.push((range.start().into(), range.end().into()));
            let Some(key) = key else { continue };
            let checked = typed_body(db, project, callable);
            let body = &checked.body;
            let constant = table
                .get(&ConstantId {
                    path: path.clone(),
                    index,
                    member: function,
                })
                .filter(|_| ast::ConstantDecl::can_cast(node.kind()))
                .map(|evaluated| evaluated.name.clone());
            for name in &body.names {
                let Refers::Node { key: refers, .. } = &name.refers else {
                    continue;
                };
                if refers != key {
                    continue;
                }
                let Some((start, end)) = read_range(source.syntax(), &path, name.start, name.end)
                else {
                    continue;
                };
                let place = if let Some(constant) = &constant {
                    Place::Constant(constant.clone())
                } else if in_update_by_id(body, start) {
                    Place::UpdateById
                } else {
                    Place::Code
                };
                found.push((start, end, place));
            }
        }
        // Everywhere else: queries, backfills and the values outside a body.
        let lookup = Lookup::for_file(db, project, file);
        for node in source.syntax().descendants() {
            let Some(expr) = Expr::cast(node.clone()) else {
                continue;
            };
            let range = node.text_range();
            let (start, end): (u32, u32) = (range.start().into(), range.end().into());
            if bodies.iter().any(|(s, e)| *s <= start && end <= *e) {
                continue;
            }
            let Some((first, rest)) = dotted(&expr) else {
                continue;
            };
            let names: Option<Vec<String>> = std::iter::once(&first)
                .chain(rest.iter())
                .map(tree::name_ref_text)
                .collect();
            let Some(names) = names else { continue };
            let query = node.ancestors().find_map(ast::QueryDecl::cast);
            if let Some(query) = &query {
                let params: BTreeSet<String> = query
                    .param_list()
                    .into_iter()
                    .flat_map(|list| list.params())
                    .filter_map(|param| param.name().as_ref().and_then(name_text))
                    .collect();
                if params.contains(&names[0]) {
                    continue;
                }
            }
            if in_setting(&node) {
                continue;
            }
            if names_constant(&lookup, &names).as_ref() != Some(id) {
                continue;
            }
            let span = node_span(&path, &node);
            let place = if query.is_some() {
                Place::Query
            } else if in_backfill(&node) {
                Place::Backfill
            } else {
                Place::Code
            };
            found.push((span.start, span.end, place));
        }
        found.sort_by_key(|(start, end, _)| (*start, std::cmp::Reverse(*end)));
        // The outermost read only: `security.max_login_attempts`, not a
        // part of it.
        let mut last_end = 0;
        for (start, end, place) in found {
            if start < last_end {
                continue;
            }
            last_end = end;
            let line = text[..start as usize].matches('\n').count() as u32 + 1;
            out.push((
                Read {
                    path: path.clone(),
                    start,
                    end,
                    line,
                },
                place,
            ));
        }
    }
    out
}

/// The syntax of callable `index` (`function` of a library) in its file.
fn callable_node(
    source: &ast::SourceFile,
    index: u32,
    function: Option<u32>,
) -> Option<SyntaxNode> {
    let decl = tree::decl_at(source, index)?;
    match (&decl, function) {
        (Decl::LibraryDecl(library), Some(member)) => tree::library_members(library)
            .into_iter()
            .nth(member as usize)
            .map(|member| member.syntax().clone()),
        _ => Some(decl.syntax().clone()),
    }
}

/// The whole expression reading a constant whose last name is at
/// `start..end`: `security.max_login_attempts`, or `max_login_attempts`.
fn read_range(root: &SyntaxNode, path: &str, start: u32, end: u32) -> Option<(u32, u32)> {
    let token = root
        .token_at_offset(start.into())
        .find(|token| token.kind() == IDENT && u32::from(token.text_range().end()) == end)?;
    let mut best = None;
    for node in token.parent_ancestors() {
        let Some(expr) = Expr::cast(node.clone()) else {
            continue;
        };
        let span = node_span(path, &node);
        if span.end != end || dotted(&expr).is_none() {
            break;
        }
        best = Some((span.start, span.end));
    }
    best
}

/// Whether `offset` is in a value an update by `id` sets.
fn in_update_by_id(body: &jadpo_graph::body::Body, offset: u32) -> bool {
    body.sites.iter().any(|site| match &site.node {
        Node::Expr(BodyExpr::Write {
            by_id: true,
            set,
            when_supplied,
            ..
        }) => set
            .iter()
            .chain(when_supplied.iter().flat_map(|(_, set)| set.iter()))
            .any(|(_, value)| {
                let value = body.site(*value);
                value.start <= offset && offset < value.end
            }),
        _ => false,
    })
}

/// Whether `node` is in a `backfill:` decision (area 5).
fn in_backfill(node: &SyntaxNode) -> bool {
    node.ancestors()
        .filter_map(ast::Member::cast)
        .any(|member| {
            member
                .member_key()
                .and_then(|key| key.names().next())
                .and_then(|name| name_text(&name))
                .as_deref()
                == Some("backfill")
        })
}

/// Whether `node` is a value written in `config { }`: a `default:` naming
/// the constant is the setting [`apply`] fills, not a read.
fn in_setting(node: &SyntaxNode) -> bool {
    node.ancestors()
        .filter_map(ast::SingletonDecl::cast)
        .any(|decl| {
            decl.keyword()
                .is_some_and(|keyword| keyword.text() == "config")
        })
}

// --- moving ------------------------------------------------------------------

/// Makes the move `plan` decided in `loaded`'s texts. On success the
/// database holds the moved project; a result that wouldn't check is
/// undone and given back as [`Stopped`].
pub fn apply(loaded: &mut Loaded, plan: &Plan) -> Result<Moved, Stopped> {
    let project = loaded.project;
    let mut before: BTreeMap<String, Option<String>> = BTreeMap::new();
    let text_of =
        |loaded: &Loaded, path: &str| loaded.file(path).map(|file| file.text(&loaded.db).clone());
    let config_path = plan
        .config
        .clone()
        .unwrap_or_else(|| CONFIG_FILE.to_string());
    let constant_path = plan.constant.path.clone();
    let mut paths: BTreeSet<String> = plan.reads.iter().map(|r| r.path.clone()).collect();
    paths.insert(config_path.clone());
    paths.insert(constant_path.clone());
    for path in &paths {
        before.insert(path.clone(), text_of(loaded, path));
    }

    // 1. The setting, and every read rewritten, while the constant is
    //    still there, so the imports it no longer needs read as unused.
    let mut edits: BTreeMap<String, Vec<(u32, u32, String)>> = BTreeMap::new();
    for read in &plan.reads {
        edits.entry(read.path.clone()).or_default().push((
            read.start,
            read.end,
            format!("config.{}", plan.setting),
        ));
    }
    let config_text = before.get(&config_path).cloned().flatten();
    match (&plan.fills, &config_text) {
        (Some(value), _) => edits.entry(value.path.clone()).or_default().push((
            value.start,
            value.end,
            plan.default.clone(),
        )),
        (None, Some(text)) => {
            if let Some(edit) = insert_field(text, plan) {
                edits.entry(config_path.clone()).or_default().push(edit);
            }
        }
        (None, None) => {}
    }
    for (path, file_edits) in edits {
        let text = text_of(loaded, &path).unwrap_or_default();
        loaded.set_text(&path, crate::apply_edits(&text, file_edits));
    }
    if config_text.is_none() && plan.fills.is_none() {
        loaded.set_text(
            &config_path,
            format!("config {{\n{}}}\n", field_text(plan, "    ")),
        );
    }
    let touched: Vec<String> = paths
        .iter()
        .filter(|p| **p != constant_path)
        .cloned()
        .collect();
    for path in &touched {
        crate::fix_imports(loaded, path);
    }

    // 2. The constant, and a library or file it leaves empty.
    let mut library_removed = None;
    let mut file_removed = false;
    if let Some(text) = text_of(loaded, &constant_path) {
        let parse = jadpo_syntax::parse_file(&text);
        if let Some(source) = ast::SourceFile::cast(parse.syntax())
            && let Some(decl) = declaration(&source, &plan.constant)
        {
            let mut target = decl.syntax().clone();
            if plan.constant.member.is_some()
                && let Some(Decl::LibraryDecl(library)) =
                    tree::decl_at(&source, plan.constant.index)
                && tree::library_members(&library).len() == 1
                && library
                    .member_list()
                    .is_some_and(|list| list.member_items().count() == 1)
            {
                library_removed = library.name().as_ref().and_then(name_text);
                target = library.syntax().clone();
            }
            let rest = source.decls().count()
                - usize::from(library_removed.is_some() || plan.constant.member.is_none());
            if rest == 0 {
                file_removed = true;
                loaded.remove(&constant_path);
            } else {
                loaded.set_text(&constant_path, remove_node(&text, &target));
                crate::fix_imports(loaded, &constant_path);
            }
        }
    }

    // 3. Formatted as the files were.
    for path in &paths {
        let Some(after) = text_of(loaded, path) else {
            continue;
        };
        let was = before.get(path).cloned().flatten();
        let project = loaded.project.layout(&loaded.db) == jadpo_db::Layout::Project;
        let canonical = |text: &str| canonical(path, text, project);
        let formatted = match &was {
            Some(was) => canonical(was).as_deref() == Some(was.as_str()),
            None => true,
        };
        if formatted && let Some(text) = canonical(&after) {
            loaded.set_text(path, text);
        }
    }

    // 4. Checked: a move never leaves a project that doesn't.
    let problems: Vec<Diagnostic> = crate::check(&loaded.db, project);
    let after: BTreeMap<String, Option<String>> = paths
        .iter()
        .map(|path| (path.clone(), text_of(loaded, path)))
        .collect();
    if !problems.is_empty() {
        let sources = jadpo_db::source_texts(&loaded.db, project);
        restore_texts(loaded, &before);
        return Err(Stopped { problems, sources });
    }

    // What changed, file by file.
    let mut files = Vec::new();
    let mut order: Vec<String> = vec![config_path.clone(), constant_path.clone()];
    let others: Vec<String> = paths
        .iter()
        .filter(|p| !order.contains(p))
        .cloned()
        .collect();
    order.extend(others);
    for path in order {
        let was = before.get(&path).cloned().flatten();
        let now = after.get(&path).cloned().flatten();
        if was == now {
            continue;
        }
        let mut rows = Vec::new();
        if path == config_path {
            rows.push(match &plan.fills {
                Some(value) => Row {
                    mark: Mark::Changed,
                    subject: plan.setting.clone(),
                    note: format!(
                        "default {}, was {}",
                        plan.default,
                        was.as_deref()
                            .and_then(|text| text.get(value.start as usize..value.end as usize))
                            .unwrap_or_default()
                    ),
                },
                None => Row {
                    mark: Mark::Added,
                    subject: plan.setting.clone(),
                    note: format!("setting, default {}", plan.default),
                },
            });
        }
        if path == constant_path {
            let (_, name) = split(&plan.shown);
            rows.push(Row {
                mark: Mark::Removed,
                subject: name.to_string(),
                note: "constant".to_string(),
            });
            if let Some(library) = &library_removed {
                rows.push(Row {
                    mark: Mark::Removed,
                    subject: library.clone(),
                    note: "library, nothing left in it".to_string(),
                });
            }
            if file_removed {
                rows.push(Row {
                    mark: Mark::Removed,
                    subject: path.rsplit('/').next().unwrap_or(&path).to_string(),
                    note: "file, nothing left in it".to_string(),
                });
            }
        }
        for read in plan.reads.iter().filter(|read| read.path == path) {
            rows.push(Row {
                mark: Mark::Changed,
                subject: format!("line {}", read.line),
                note: format!("reads config.{}", plan.setting),
            });
        }
        files.push(FileChange {
            path,
            before: was,
            after: now,
            rows,
        });
    }
    Ok(Moved { files })
}

/// Puts `loaded`'s texts back as they were before `moved`, as the editor
/// does once it has sent the edit.
pub fn restore(loaded: &mut Loaded, moved: &Moved) {
    let before: BTreeMap<String, Option<String>> = moved
        .files
        .iter()
        .map(|change| (change.path.clone(), change.before.clone()))
        .collect();
    restore_texts(loaded, &before);
}

pub(crate) fn restore_texts(loaded: &mut Loaded, before: &BTreeMap<String, Option<String>>) {
    for (path, text) in before {
        match text {
            Some(text) => loaded.set_text(path, text.clone()),
            None => loaded.remove(path),
        }
    }
}

/// `text` formatted as `jadpo fmt` formats a file at `path`, with its
/// declarations in definition-before-use order in a project; `None` when it
/// can't be.
pub(crate) fn canonical(path: &str, text: &str, project: bool) -> Option<String> {
    let primary = project
        .then(|| {
            let parse = jadpo_syntax::parse_file(text);
            let source = ast::SourceFile::cast(parse.syntax())?;
            let items = jadpo_db::items_of(&source);
            let location = jadpo_db::locate(path, jadpo_db::Layout::Project);
            jadpo_db::primary(&location, &items).map(|item| item.name.clone())
        })
        .flatten();
    jadpo_fmt::format_file_in_order(text, primary.as_deref()).ok()
}

/// The field the move adds, each line after `indent`.
fn field_text(plan: &Plan, indent: &str) -> String {
    let mut out = String::new();
    for line in &plan.doc {
        out.push_str(&format!("{indent}{line}\n"));
    }
    out.push_str(&format!(
        "{indent}{}: {} {{\n{indent}    default: {}\n{indent}}}\n",
        plan.setting, plan.ty, plan.default
    ));
    out
}

/// The edit adding the field to the `config { }` in `text`, before the
/// first field whose name comes after it, so the fields stay in name order
/// (area 10, "Fields in name order").
fn insert_field(text: &str, plan: &Plan) -> Option<(u32, u32, String)> {
    let parse = jadpo_syntax::parse_file(text);
    let source = ast::SourceFile::cast(parse.syntax())?;
    let config = source.decls().find_map(|decl| match decl {
        Decl::SingletonDecl(singleton)
            if singleton.keyword().is_some_and(|k| k.text() == "config") =>
        {
            Some(singleton)
        }
        _ => None,
    })?;
    let list = config.member_list()?;
    let items: Vec<MemberItem> = list.member_items().collect();
    let first = items.first().map(|item| item.syntax().clone());
    let indent = first.as_ref().and_then(significant).map_or_else(
        || "    ".to_string(),
        |token| column_text(text, token.text_range().start().into()),
    );
    let field = field_text(plan, &indent);
    let after = items.iter().find(|item| match item {
        MemberItem::Member(member) => member
            .member_key()
            .and_then(|key| key.names().next())
            .and_then(|name| name_text(&name))
            .is_some_and(|name| name > plan.setting),
        _ => false,
    });
    match after {
        Some(item) => {
            let start = first_line_start(text, item.syntax());
            Some((start, start, field))
        }
        None => {
            let close = list.r_brace_token()?;
            let at: u32 = close.text_range().start().into();
            let line_start = line_start(text, at);
            if text[line_start as usize..at as usize].trim().is_empty() {
                Some((line_start, line_start, field))
            } else {
                // `config {}`: the field on lines of its own.
                Some((at, at, format!("\n{field}")))
            }
        }
    }
}

/// The first token of `node` that isn't whitespace: a comment or code.
fn significant(node: &SyntaxNode) -> Option<SyntaxToken> {
    node.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| token.kind() != WHITESPACE)
}

/// The start of the line holding the first comment or code of `node`.
fn first_line_start(text: &str, node: &SyntaxNode) -> u32 {
    let at = significant(node).map_or_else(
        || node.text_range().start().into(),
        |token| u32::from(token.text_range().start()),
    );
    line_start(text, at)
}

fn line_start(text: &str, at: u32) -> u32 {
    text[..at as usize].rfind('\n').map_or(0, |i| i as u32 + 1)
}

/// The spaces before byte `at` on its line.
fn column_text(text: &str, at: u32) -> String {
    let start = line_start(text, at) as usize;
    text[start..at as usize]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

/// `text` without `node`: from the line its comments or code start on to
/// the end of the line it ends on, and one blank line fewer where two
/// would meet.
pub(crate) fn remove_node(text: &str, node: &SyntaxNode) -> String {
    let start = first_line_start(text, node) as usize;
    let last = node
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .last()
        .map_or_else(
            || node.text_range().end().into(),
            |token| u32::from(token.text_range().end()),
        );
    let mut end = last as usize;
    let rest = &text[end..];
    end += rest.find('\n').map_or(rest.len(), |i| i + 1);
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..start]);
    let tail = &text[end..];
    // A blank line before and after: keep one.
    if out.ends_with("\n\n") && tail.starts_with('\n') {
        out.push_str(&tail[1..]);
    } else if out.ends_with("{\n") && tail.starts_with('\n') {
        out.push_str(tail.trim_start_matches('\n'));
    } else {
        out.push_str(tail);
    }
    out
}
