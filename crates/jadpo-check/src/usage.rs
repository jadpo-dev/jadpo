//! Who uses each declaration, for area 1's promotion and dead-code rules: a
//! type or helper used by a second file moves to the domain's `types/` or a
//! library, one used by a second domain to `shared/`, a failure raised by a
//! second domain to `shared/failures/`, and a shared declaration nothing
//! uses is an error. Tests never count as users.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{
    Base, Db, File, Folder, Item, ItemKind, Layout, Ownership, Place, Project, item_tree, location,
};
use jadpo_diag::{Diagnostic, rule};
use jadpo_graph::body::{Callee, Expr, Node, Refers};
use jadpo_graph::{Key, Kind};
use jadpo_syntax::ast::{self, AstNode, Decl};

use crate::case::{starts_upper, to_snake};
use crate::lookups::{Lookup, Name, NameView};
use crate::tree::{self, name_span};
use crate::types::{callables_of, typed_body};

/// The declarations one file mentions.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Mentions {
    /// Types, entities and failures it names, and callables it calls.
    names: BTreeSet<String>,
    /// The library functions it calls.
    library_calls: BTreeSet<(String, String)>,
    /// Failures it raises with `reject` or maps a construction's rules to.
    raised: BTreeSet<String>,
}

/// What one file mentions: in its declarations, every upper-case name
/// (field types, copying lines, refinement bases, a query's block); in its
/// callables, what their typed bodies say each name refers to, so a call a
/// local shadows or a name only resolution knew about counts as the check
/// resolved it.
#[salsa::tracked(returns(ref))]
pub fn mentions(db: &dyn Db, project: Project, file: File) -> Mentions {
    let mut found = Mentions::default();
    let Some(source) = tree::source(db, file) else {
        return found;
    };
    for decl in source.decls() {
        if matches!(
            decl,
            Decl::FunctionDecl(_)
                | Decl::ActionDecl(_)
                | Decl::FixtureDecl(_)
                | Decl::TestDecl(_)
                | Decl::SeedDecl(_)
                | Decl::LibraryDecl(_)
        ) {
            continue;
        }
        for name in decl.syntax().descendants().filter_map(ast::NameRef::cast) {
            if let Some(text) = tree::name_ref_text(&name)
                && starts_upper(&text)
            {
                found.names.insert(text);
            }
        }
    }
    let host_of = |key: &Key| key.name().split('.').next().unwrap_or_default().to_string();
    for callable in callables_of(db, file) {
        let body = &typed_body(db, project, callable).body;
        for name in &body.names {
            let Refers::Node { key, .. } = &name.refers else {
                continue;
            };
            if let Some(Kind::Type | Kind::Entity | Kind::Field | Kind::Variant) = key.kind() {
                found.names.insert(host_of(key));
            }
        }
        for (_, callee) in body.calls() {
            match callee {
                Callee::LibraryFunction(key) => {
                    if let Some((library, function)) = key.name().split_once('.') {
                        found
                            .library_calls
                            .insert((library.to_string(), function.to_string()));
                    }
                }
                Callee::Function(key)
                | Callee::Action(key)
                | Callee::Query(key)
                | Callee::Fixture(key) => {
                    found.names.insert(key.name().to_string());
                }
                Callee::Object(key) | Callee::Variant(key) | Callee::Entity { entity: key, .. } => {
                    found.names.insert(host_of(key));
                }
                Callee::Construct(ty) => {
                    found.names.extend(ty.keys().into_iter().map(host_of));
                }
                Callee::Std(_) | Callee::Unknown => {}
            }
        }
        for site in &body.sites {
            match &site.node {
                Node::Expr(Expr::Reject {
                    failure: Some(failure),
                    ..
                }) => {
                    found.raised.insert(failure.name().to_string());
                }
                Node::Expr(Expr::Attempt { arms, .. }) => {
                    for arm in arms {
                        found.raised.insert(arm.failure.name().to_string());
                    }
                }
                _ => {}
            }
        }
    }
    found
}

/// A file that uses a declaration: its path, owner, and whether it holds
/// routes.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct User {
    pub path: String,
    pub ownership: Ownership,
    pub route: bool,
}

/// Who uses what, across the project's non-test files.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Users {
    /// By declaration name: the files that mention it.
    by_name: BTreeMap<String, BTreeSet<User>>,
    /// By `library.function`: the files that call it.
    by_function: BTreeMap<String, BTreeSet<User>>,
    /// By failure: the domains that raise it.
    raised_by: BTreeMap<String, BTreeSet<String>>,
}

#[salsa::tracked(returns(ref))]
pub fn users(db: &dyn Db, project: Project) -> Users {
    let mut users = Users::default();
    for &file in project.files(db) {
        let location = location(db, project, file);
        if location.test || location.place == Place::Tests {
            continue;
        }
        let user = User {
            path: file.path(db).clone(),
            ownership: location.ownership.clone(),
            route: location.place == Place::Folder(Folder::Routes),
        };
        let found = mentions(db, project, file);
        for name in &found.names {
            users
                .by_name
                .entry(name.clone())
                .or_default()
                .insert(user.clone());
        }
        for (library, function) in &found.library_calls {
            users
                .by_function
                .entry(format!("{library}.{function}"))
                .or_default()
                .insert(user.clone());
        }
        if let Ownership::Domain(domain) = &location.ownership {
            for failure in &found.raised {
                users
                    .raised_by
                    .entry(failure.clone())
                    .or_default()
                    .insert(domain.clone());
            }
        }
    }
    users
}

/// The files that mention `name`: a per-name lookup, so a file's usage
/// rules depend only on its own declarations' users.
#[salsa::tracked(returns(ref))]
fn name_users<'db>(db: &'db dyn Db, project: Project, name: Name<'db>) -> BTreeSet<User> {
    users(db, project)
        .by_name
        .get(name.text(db))
        .cloned()
        .unwrap_or_default()
}

/// The files that call library function `library.function`.
#[salsa::tracked(returns(ref))]
fn function_users<'db>(db: &'db dyn Db, project: Project, name: Name<'db>) -> BTreeSet<User> {
    users(db, project)
        .by_function
        .get(name.text(db))
        .cloned()
        .unwrap_or_default()
}

/// The domains that raise failure `name`.
#[salsa::tracked(returns(ref))]
fn raisers<'db>(db: &'db dyn Db, project: Project, name: Name<'db>) -> BTreeSet<String> {
    users(db, project)
        .raised_by
        .get(name.text(db))
        .cloned()
        .unwrap_or_default()
}

/// Promotion and dead-code problems of one file's declarations.
#[salsa::tracked(returns(ref))]
pub fn usage_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    if project.layout(db) == Layout::Loose {
        return Vec::new();
    }
    let location = location(db, project, file);
    if location.test {
        return Vec::new();
    }
    let Place::Folder(folder) = location.place else {
        return Vec::new();
    };
    let Some(source) = tree::source(db, file) else {
        return Vec::new();
    };
    let path = file.path(db);
    let lookup = Lookup::new(db, project);
    let named = |name: &str| name_users(db, project, Name::new(db, name.to_string()));
    let ours = |user: &&User| user.ownership == location.ownership;
    let mut out = Vec::new();
    for item in &item_tree(db, file).items {
        match (&location.ownership, folder, &item.kind) {
            (Ownership::Domain(domain), folder, kind)
                if !matches!(folder, Folder::Types | Folder::Libraries)
                    && (*kind == ItemKind::Function || is_plain_type(kind)) =>
            {
                let mut others: Vec<&User> = named(&item.name)
                    .iter()
                    .filter(|user| user.path != *path)
                    .collect();
                // A route only counts when nothing else uses the type.
                if folder != Folder::Routes || others.iter().any(|user| !user.route) {
                    others.retain(|user| !user.route);
                }
                let elsewhere = others.iter().copied().find(|user| !ours(user));
                let same = others.iter().copied().find(ours);
                match (elsewhere, same) {
                    (Some(user), _) if item.public && pure(&source, path, item.index, &lookup) => {
                        out.push(promote(path, &source, item, Move::Shared, &user.path));
                    }
                    (_, Some(user)) => {
                        out.push(promote(
                            path,
                            &source,
                            item,
                            Move::Domain(domain),
                            &user.path,
                        ));
                    }
                    _ => {}
                }
            }
            (Ownership::Domain(_), Folder::Failures, ItemKind::Type(Base::Failure)) => {
                let domains = raisers(db, project, Name::new(db, item.name.clone())).len();
                if domains > 1 {
                    let file = to_snake(&item.name);
                    out.push(
                        Diagnostic::new(
                            &rule::LAYOUT_PROMOTION_REQUIRED,
                            format!("`{}` is raised by {domains} domains", item.name),
                        )
                        .at(name_span(&source, path, item.index, None), "raised beyond its domain")
                        .reason("A failure lives in the `failures/` folder of the domain that raises it; once a second domain raises it, it moves to `shared/failures/`.")
                        .next(format!("Move `{}` to `shared/failures/{file}.jadpo`", item.name)),
                    );
                }
            }
            (ownership, Folder::Types, kind) if is_plain_type(kind) => {
                let others: Vec<&User> = named(&item.name)
                    .iter()
                    .filter(|user| user.path != *path)
                    .collect();
                let elsewhere = others.iter().copied().find(|user| !ours(user));
                if others.is_empty() {
                    out.push(unused(path, &source, item, None));
                } else if let (Ownership::Domain(_), Some(user)) = (ownership, elsewhere)
                    && item.public
                    && pure(&source, path, item.index, &lookup)
                {
                    out.push(promote(path, &source, item, Move::Shared, &user.path));
                }
            }
            (ownership, Folder::Libraries, ItemKind::Library) => {
                let mut callers: BTreeSet<&User> = BTreeSet::new();
                for function in &item.functions {
                    let found = function_users(
                        db,
                        project,
                        Name::new(db, format!("{}.{}", item.name, function.name)),
                    );
                    if found.is_empty() {
                        out.push(unused(path, &source, function, Some(item.index)));
                    } else {
                        callers.extend(found);
                    }
                }
                if let (Ownership::Domain(_), Some(user)) =
                    (ownership, callers.iter().copied().find(|user| !ours(user)))
                    && item.public
                    && pure(&source, path, item.index, &lookup)
                {
                    out.push(promote(path, &source, item, Move::Shared, &user.path));
                }
            }
            _ => {}
        }
    }
    out
}

/// Whether declaration `index` depends on no domain's code: nothing it
/// names belongs to a domain, apart from what its own file declares.
fn pure(source: &ast::SourceFile, path: &str, index: u32, table: &dyn NameView) -> bool {
    let Some(decl) = tree::decl_at(source, index) else {
        return true;
    };
    decl.syntax()
        .descendants()
        .filter_map(ast::NameRef::cast)
        .filter_map(|name| tree::name_ref_text(&name))
        .all(|name| {
            !table.is_domain(&name)
                && table
                    .declared(&name)
                    .iter()
                    .all(|symbol| symbol.path == path || symbol.ownership.domain().is_none())
        })
}

/// Where a promoted declaration moves.
enum Move<'a> {
    Domain(&'a str),
    Shared,
}

fn is_plain_type(kind: &ItemKind) -> bool {
    matches!(
        kind,
        ItemKind::Type(base) if !matches!(base, Base::Failure | Base::Event | Base::EmailMessage)
    )
}

fn promote(path: &str, source: &ast::SourceFile, item: &Item, to: Move, user: &str) -> Diagnostic {
    let name = &item.name;
    let file = to_snake(name);
    let (scope, home) = match to {
        Move::Domain(domain) => ("file", format!("domains/{domain}")),
        Move::Shared => ("domain", "shared".to_string()),
    };
    let (what, target) = match item.kind {
        ItemKind::Function => (
            format!(
                "A helper function used by one {scope} lives in it; once another {scope} uses it, it moves to a library in `{home}/libraries/`"
            ),
            format!(
                "Move `{name}` into a library in `{home}/libraries/` and call it through the library"
            ),
        ),
        ItemKind::Library => (
            format!(
                "A library used by one {scope} lives in it; once another {scope} uses it, it moves to `{home}/libraries/`"
            ),
            format!("Move `{name}` to `{home}/libraries/{file}.jadpo`"),
        ),
        _ => (
            format!(
                "A type used by one {scope} lives in it; once another {scope} uses it, it moves to `{home}/types/`"
            ),
            format!("Move `{name}` to `{home}/types/{file}.jadpo`"),
        ),
    };
    Diagnostic::new(
        &rule::LAYOUT_PROMOTION_REQUIRED,
        format!("`{name}` is used beyond this {scope}"),
    )
    .at(
        name_span(source, path, item.index, None),
        format!("used by another {scope}"),
    )
    .reason(format!("{what}. `{user}` uses it too."))
    .next(target)
}

fn unused(path: &str, source: &ast::SourceFile, item: &Item, library: Option<u32>) -> Diagnostic {
    let span = match library {
        Some(index) => name_span(source, path, index, Some(item.index)),
        None => name_span(source, path, item.index, None),
    };
    let what = if library.is_some() {
        "library function"
    } else {
        "shared type"
    };
    Diagnostic::new(
        &rule::LAYOUT_UNUSED_DECLARATION,
        format!("`{}` isn't used", item.name),
    )
    .at(span, "nothing uses this")
    .reason(format!(
        "A {what} exists to be used by several files; one that nothing uses is dead code. Tests don't count as users."
    ))
    .next(format!("Remove `{}`", item.name))
}
