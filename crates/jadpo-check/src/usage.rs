//! Who uses each declaration, for area 1's promotion and dead-code rules: a
//! type or helper used by a second file moves to the domain's `types/` or a
//! library, one used by a second domain to `shared/`, a failure raised by a
//! second domain to `shared/failures/`, and a shared declaration nothing
//! uses is an error. Tests never count as users.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{
    Base, Db, File, Folder, Item, ItemKind, Layout, Location, Ownership, Place, Project, item_tree,
    location,
};
use jadpo_diag::{Diagnostic, rule};
use jadpo_graph::body::{Callee, Expr, Node, Refers};
use jadpo_graph::{Key, Kind};
use jadpo_syntax::ast::{self, AstNode, Decl};

use crate::case::{starts_upper, to_snake};
use crate::lookups::{Lookup, Name, NameView};
use crate::std_lib::StdNames;
use crate::tree::{self, name_span};
use crate::types::{backfill_callables, callables_of, typed_body};

/// The declarations one file mentions.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Mentions {
    /// Types, entities and failures it names, and callables it calls.
    pub(crate) names: BTreeSet<String>,
    /// The library functions it calls.
    pub(crate) library_calls: BTreeSet<(String, String)>,
    /// Failures it raises with `reject` or maps a construction's rules to.
    pub(crate) raised: BTreeSet<String>,
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
    // Names as the type tables know them, so a domain's private name and
    // another domain's of the same spelling have users of their own (area
    // 1, "Files and names").
    let contested = jadpo_db::contested(db, project);
    let domain = location(db, project, file).ownership.scope();
    for decl in source.decls() {
        if matches!(
            decl,
            Decl::FunctionDecl(_)
                | Decl::ConstantDecl(_)
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
                found.names.insert(contested.canonical(&text, domain));
            }
        }
        // A query takes a constant where it takes a literal (area 3).
        if let Decl::QueryDecl(query) = &decl {
            constants_in_query(db, project, file, query, contested, domain, &mut found);
        }
    }
    let host_of = |key: &Key| canonical_of(contested, key);
    // The latest migration's computed backfills use what they call (area
    // 5, chunk 5.6b).
    for callable in callables_of(db, file)
        .into_iter()
        .chain(backfill_callables(db, project, file))
    {
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
                    if let Some((library, function)) = key.name().rsplit_once('.') {
                        found
                            .library_calls
                            .insert((library.to_string(), function.to_string()));
                    }
                }
                Callee::Function(key)
                | Callee::Action(key)
                | Callee::Query(key)
                | Callee::Fixture(key)
                | Callee::Schedule { schedule: key, .. } => {
                    found.names.insert(canonical_of(contested, key));
                }
                Callee::Object(key) | Callee::Variant(key) | Callee::Entity { entity: key, .. } => {
                    found.names.insert(host_of(key));
                }
                Callee::Construct(ty) => {
                    found.names.extend(ty.keys().into_iter().map(host_of));
                }
                Callee::Std(_)
                | Callee::Caller(_)
                | Callee::Session(_)
                | Callee::Batch(_)
                | Callee::Files { .. }
                | Callee::AdvanceClock
                | Callee::Unknown => {}
                // A prompt is used by the value built of its type.
                Callee::Prompt(_) => {}
                // A message is used by what calls it (area 10).
                Callee::Translation(key) => {
                    if let Some(name) = crate::translations::type_of_key(db, project, key) {
                        found.names.insert(contested.canonical(&name, domain));
                    }
                }
            }
        }
        for site in &body.sites {
            match &site.node {
                // A constant read: a library's counts as a call of it, one
                // beside its use as a mention (area 3).
                Node::Expr(Expr::Constant { constant }) => match constant.name().split_once('.') {
                    Some((library, own))
                        if !jadpo_db::symbols(db, project).domains.contains(library) =>
                    {
                        found
                            .library_calls
                            .insert((library.to_string(), own.to_string()));
                    }
                    _ => {
                        found.names.insert(canonical_of(contested, constant));
                    }
                },
                Node::Expr(Expr::Reject {
                    failure: Some(failure),
                    ..
                }) => {
                    found.raised.insert(canonical_of(contested, failure));
                }
                Node::Expr(Expr::Attempt { arms, .. }) => {
                    for arm in arms {
                        found.raised.insert(canonical_of(contested, &arm.failure));
                    }
                }
                _ => {}
            }
        }
    }
    found
}

/// The constants a query's block reads: one beside its use by name, a
/// library's as a call of it.
fn constants_in_query(
    db: &dyn Db,
    project: Project,
    file: File,
    query: &ast::QueryDecl,
    contested: &jadpo_db::Contested,
    domain: Option<&str>,
    found: &mut Mentions,
) {
    let lookup = Lookup::for_file(db, project, file);
    let params: BTreeSet<String> = query
        .param_list()
        .into_iter()
        .flat_map(|list| list.params())
        .filter_map(|param| param.name().as_ref().and_then(tree::name_text))
        .collect();
    let Some(block) = query.member_list() else {
        return;
    };
    for name in block.syntax().descendants().filter_map(ast::NameRef::cast) {
        let Some(text) = tree::name_ref_text(&name) else {
            continue;
        };
        if starts_upper(&text) || params.contains(&text) {
            continue;
        }
        let parent = name.syntax().parent();
        let field = parent.clone().and_then(ast::FieldExpr::cast);
        // The name after a dot: `pricing.vat_rate`'s `vat_rate`.
        if let Some(field) = &field
            && field.name_ref().is_some_and(|after| after == name)
        {
            let Some(ast::Expr::NameRef(head)) = field.expr() else {
                continue;
            };
            let Some(library) = tree::name_ref_text(&head) else {
                continue;
            };
            if lookup
                .library_function(&library, &text)
                .is_some_and(|symbol| symbol.kind == ItemKind::Constant)
            {
                found.library_calls.insert((library, text));
            }
            continue;
        }
        if lookup
            .declared(&text)
            .iter()
            .any(|symbol| symbol.kind == ItemKind::Constant && symbol.library.is_none())
        {
            found.names.insert(contested.canonical(&text, domain));
        }
    }
}

/// The name the checker's tables know the declaration `key` names by, or
/// its host's for a field or variant: a key of a function or type declared
/// in a domain carries the domain (`type billing.Address`), which the
/// tables use only for a contested private name ([`jadpo_db::Contested`]).
fn canonical_of(contested: &jadpo_db::Contested, key: &Key) -> String {
    let name = key.name();
    let parts: Vec<&str> = name.split('.').collect();
    match key.kind() {
        Some(Kind::Type | Kind::Entity | Kind::Field | Kind::Variant) => {
            // The type's own name: a package's or a domain's comes after
            // it (`Postcodes.UkPostcode`, `billing.Address`), a field or a
            // variant after the type.
            let first = parts
                .iter()
                .position(|part| starts_upper(part))
                .unwrap_or(0);
            let at = match parts.get(first + 1) {
                Some(next) if first == 0 && starts_upper(next) => 1,
                _ => first,
            };
            let domain = (at == 1).then(|| parts[0]);
            contested.canonical(parts[at], domain)
        }
        Some(Kind::Function | Kind::Constant) if parts.len() == 2 => {
            contested.canonical(parts[1], Some(parts[0]))
        }
        _ => name.to_string(),
    }
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
    let Some(source) = tree::source(db, file) else {
        return Vec::new();
    };
    let mut out = unused_beside_primary(db, project, file, location, &source);
    let Place::Folder(folder) = location.place else {
        return out;
    };
    let path = file.path(db);
    let lookup = Lookup::for_file(db, project, file);
    let contested = jadpo_db::contested(db, project);
    let named = |name: &str| {
        let canonical = contested.canonical(name, location.ownership.scope());
        name_users(db, project, Name::new(db, canonical))
    };
    let ours = |user: &&User| user.ownership == location.ownership;
    // A name declared twice in one scope is `semantic.duplicate_declaration`
    // alone: who uses it can't be told apart, so no move is asked for.
    let declared_twice = |item: &Item| {
        let all = &lookup.facts(&item.name).declarations;
        let Some(own) = all.iter().find(|symbol| {
            symbol.path == *path && symbol.index == item.index && symbol.library.is_none()
        }) else {
            return false;
        };
        all.iter()
            .filter(|symbol| symbol.library.is_none() && symbol.scope == own.scope)
            .count()
            > 1
    };
    for item in &item_tree(db, file).items {
        if declared_twice(item) {
            continue;
        }
        // A package's public declarations are its interface to the apps
        // that install it (area 11): nothing in the package has to use
        // them.
        if item.public && location.ownership.package().is_some() {
            continue;
        }
        match (&location.ownership, folder, &item.kind) {
            (Ownership::Domain(domain), folder, kind)
                if folder != Folder::Types
                    && (matches!(kind, ItemKind::Function | ItemKind::Constant)
                        || is_plain_type(kind)) =>
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
                        out.push(promoted(
                            db,
                            project,
                            file,
                            &source,
                            item,
                            Move::Shared,
                            &user.path,
                        ));
                    }
                    (_, Some(user)) => {
                        out.push(promoted(
                            db,
                            project,
                            file,
                            &source,
                            item,
                            Move::Domain(domain),
                            &user.path,
                        ));
                    }
                    _ => {}
                }
            }
            // A type beside a shared library that another file uses moves
            // to a file of its own in `shared/types/`.
            (Ownership::Shared, folder, kind) if folder != Folder::Types && is_plain_type(kind) => {
                if let Some(user) = named(&item.name).iter().find(|user| user.path != *path) {
                    out.push(promoted(
                        db,
                        project,
                        file,
                        &source,
                        item,
                        Move::OwnFile("shared/types"),
                        &user.path,
                    ));
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
                // A file is named after its main declaration, or meant to
                // be when it is misnamed (the first type, as the layout
                // reads it); the others support that one (area 1, "A file
                // per shared name"), and `unused_beside_primary` checks
                // that it uses them.
                let main = jadpo_db::primary(location, &item_tree(db, file).items)
                    .is_some_and(|primary| primary.index == item.index);
                if others.is_empty() {
                    if main {
                        out.push(unused(path, &source, item, None));
                    }
                } else if let (Ownership::Domain(_), Some(user)) = (ownership, elsewhere)
                    && item.public
                    && pure(&source, path, item.index, &lookup)
                {
                    out.push(promoted(
                        db,
                        project,
                        file,
                        &source,
                        item,
                        Move::Shared,
                        &user.path,
                    ));
                } else if !main && let Some(user) = others.first() {
                    let folder = path.rsplit_once('/').map_or("", |(folder, _)| folder);
                    out.push(promoted(
                        db,
                        project,
                        file,
                        &source,
                        item,
                        Move::OwnFile(folder),
                        &user.path,
                    ));
                }
            }
            (ownership, Folder::Libraries, ItemKind::Library) => {
                let mut callers: BTreeSet<&User> = BTreeSet::new();
                let library = contested.canonical(&item.name, location.ownership.scope());
                for function in &item.functions {
                    let found = function_users(
                        db,
                        project,
                        Name::new(db, format!("{library}.{}", function.name)),
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
                    out.push(promoted(
                        db,
                        project,
                        file,
                        &source,
                        item,
                        Move::Shared,
                        &user.path,
                    ));
                }
            }
            _ => {}
        }
    }
    out
}

/// `layout.unused_declaration` for each declaration beside the file's
/// primary one that the primary doesn't use, directly or through one
/// another (area 1, "Files and names"), so a file never collects code for
/// something else. It reads the file's own tree and, per supporting
/// declaration, who else uses it: one another file uses counts as used,
/// since `layout.promotion_required` moves it out instead. Tests never
/// count.
fn unused_beside_primary(
    db: &dyn Db,
    project: Project,
    file: File,
    location: &Location,
    source: &ast::SourceFile,
) -> Vec<Diagnostic> {
    let items = &item_tree(db, file).items;
    let Some(primary) = jadpo_db::primary(location, items) else {
        return Vec::new();
    };
    let path = file.path(db);
    let supporting: Vec<&Item> = items
        .iter()
        .filter(|item| {
            item.index != primary.index
                && (matches!(item.kind, ItemKind::Function | ItemKind::Constant)
                    || is_plain_type(&item.kind))
        })
        .collect();
    if supporting.is_empty() {
        return Vec::new();
    }
    let mut used = BTreeSet::from([primary.index]);
    let mut pending = vec![primary.index];
    for item in &supporting {
        let canonical =
            jadpo_db::contested(db, project).canonical(&item.name, location.ownership.scope());
        let elsewhere = name_users(db, project, Name::new(db, canonical))
            .iter()
            .any(|user| user.path != *path);
        if elsewhere && used.insert(item.index) {
            pending.push(item.index);
        }
    }
    while let Some(index) = pending.pop() {
        let names = named_in(source, index);
        for item in &supporting {
            if names.contains(&item.name) && used.insert(item.index) {
                pending.push(item.index);
            }
        }
    }
    if supporting.iter().all(|item| used.contains(&item.index))
        || names_nothing(db, project, file, source, &used)
    {
        return Vec::new();
    }
    supporting
        .into_iter()
        .filter(|item| !used.contains(&item.index))
        .map(|item| unused_beside(path, source, item, primary))
        .collect()
}

/// Whether a declaration in `used` writes a name that names nothing. That
/// is resolution's to report, and it may be a misspelling of the
/// supporting declaration it was meant to use, so while one does, nothing
/// beside the primary is called unused: one mistake, one error.
fn names_nothing(
    db: &dyn Db,
    project: Project,
    file: File,
    source: &ast::SourceFile,
    used: &BTreeSet<u32>,
) -> bool {
    let lookup = Lookup::for_file(db, project, file);
    let std = StdNames::get();
    let unknown_type = used.iter().any(|&index| {
        named_in(source, index).iter().any(|name| {
            starts_upper(name)
                && lookup.declared(name).is_empty()
                && !std.is_type(name)
                && !std.reserved(name)
                && !std.is_generic(name)
                && !lookup.is_package(name)
        })
    });
    unknown_type
        || callables_of(db, file)
            .into_iter()
            .filter(|callable| used.contains(&callable.index(db)))
            .any(|callable| {
                typed_body(db, project, callable)
                    .body
                    .calls()
                    .any(|(_, callee)| matches!(callee, Callee::Unknown))
            })
}

/// Every name declaration `index` reads: types, calls and values, but not
/// a field after a dot, which names no declaration.
fn named_in(source: &ast::SourceFile, index: u32) -> BTreeSet<String> {
    let Some(decl) = tree::decl_at(source, index) else {
        return BTreeSet::new();
    };
    decl.syntax()
        .descendants()
        .filter_map(ast::NameRef::cast)
        .filter(|name| {
            !name
                .syntax()
                .parent()
                .and_then(ast::FieldExpr::cast)
                .and_then(|field| field.name_ref())
                .is_some_and(|field| field == *name)
        })
        .filter_map(|name| tree::name_ref_text(&name))
        .collect()
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
#[derive(Clone, Copy)]
enum Move<'a> {
    Domain(&'a str),
    Shared,
    /// A file of its own in the folder it is in, `types/` here.
    OwnFile(&'a str),
}

fn is_plain_type(kind: &ItemKind) -> bool {
    matches!(
        kind,
        ItemKind::Type(base) if !matches!(base, Base::Failure | Base::Event | Base::EmailMessage | Base::Message | Base::Prompt)
    )
}

/// `layout.promotion_required`, with the edits that make the move when
/// the declaration is a type: a helper function becomes a library's,
/// which changes its calls, so it gets no edit.
fn promoted(
    db: &dyn Db,
    project: Project,
    file: File,
    source: &ast::SourceFile,
    item: &Item,
    to: Move,
    user: &str,
) -> Diagnostic {
    let path = file.path(db);
    let name = &item.name;
    let file_name = to_snake(name);
    let target = match &to {
        Move::Domain(domain) => format!("domains/{domain}/types/{file_name}.jadpo"),
        Move::Shared if item.kind == ItemKind::Library => {
            format!("shared/libraries/{file_name}.jadpo")
        }
        Move::Shared => format!("shared/types/{file_name}.jadpo"),
        Move::OwnFile(folder) => format!("{folder}/{file_name}.jadpo"),
    };
    let problem = promote(path, source, item, to, user, &target);
    if matches!(item.kind, ItemKind::Function | ItemKind::Constant) {
        return problem;
    }
    let Some(decl) = tree::decl_at(source, item.index) else {
        return problem;
    };
    let mut edits = crate::imports::move_edits(db, project, file, decl.syntax(), name, &target);
    if edits.is_empty() {
        return problem;
    }
    // A type's key carries its domain until it moves to `shared/`: the
    // move writes the registry line that keeps it (area 18, question 8).
    if let (Move::Shared, ItemKind::Type(_), Some(domain)) = (
        &to,
        &item.kind,
        location(db, project, file).ownership.domain(),
    ) {
        edits.push(registry_line(
            db,
            project,
            &format!("type {name}"),
            &format!("{domain}.{name}"),
        ));
    }
    let problem = problem.next(format!(
        "Move `{name}` to `{target}`, or run `jadpo fmt --layout`, which moves it and updates the imports"
    ));
    edits
        .into_iter()
        .fold(problem, |problem, (span, text)| problem.edit(span, text))
}

/// The edit that adds `"<now>": "<born name of identity>"` to the identity
/// registry, `database/identities.json`, writing the file whole.
fn registry_line(
    db: &dyn Db,
    project: Project,
    now: &str,
    identity: &str,
) -> (jadpo_diag::Span, String) {
    let registry = crate::graph::registry(db, project);
    let born = registry.born(Kind::Type, identity, None);
    let mut lines = project.identities(db).clone();
    lines.insert(now.to_string(), born);
    let written = project
        .records(db)
        .iter()
        .find(|record| record.path == jadpo_db::IDENTITIES)
        .and_then(|record| record.text.as_ref().ok())
        .map_or(0, |text| text.len() as u32);
    (
        jadpo_diag::Span::new(jadpo_db::IDENTITIES, 0, written),
        jadpo_db::write_identities(&lines),
    )
}

fn promote(
    path: &str,
    source: &ast::SourceFile,
    item: &Item,
    to: Move,
    user: &str,
    target: &str,
) -> Diagnostic {
    let name = &item.name;
    let (scope, home) = match to {
        Move::Domain(domain) => ("file", format!("domains/{domain}")),
        Move::Shared => ("domain", "shared".to_string()),
        Move::OwnFile(folder) => ("file", folder.to_string()),
    };
    let (what, next) = match (&to, &item.kind) {
        (_, ItemKind::Function) => (
            format!(
                "A helper function used by one {scope} lives in it; once another {scope} uses it, it moves to a library in `{home}/libraries/`"
            ),
            format!(
                "Move `{name}` into a library in `{home}/libraries/` and call it through the library"
            ),
        ),
        (_, ItemKind::Constant) => (
            format!(
                "A constant read by one {scope} lives in it; once another {scope} reads it, it moves to a library in `{home}/libraries/`"
            ),
            format!(
                "Move `{name}` into a library in `{home}/libraries/` and read it through the library"
            ),
        ),
        (Move::OwnFile(folder), _) => (
            format!(
                "A declaration another file uses has a file of its own named after it, so its `use` line says where it is; `{name}` is in `{}`, named after another declaration, and moves to `{folder}/`",
                path.rsplit('/').next().unwrap_or(path)
            ),
            format!("Move `{name}` to `{target}`"),
        ),
        (_, ItemKind::Library) => (
            format!(
                "A library used by one {scope} lives in it; once another {scope} uses it, it moves to `{home}/libraries/`"
            ),
            format!("Move `{name}` to `{target}`"),
        ),
        _ => (
            format!(
                "A type used by one {scope} lives in it; once another {scope} uses it, it moves to a file of its own in `{home}/types/`"
            ),
            format!("Move `{name}` to `{target}`"),
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
    .next(next)
}

fn unused(path: &str, source: &ast::SourceFile, item: &Item, library: Option<u32>) -> Diagnostic {
    let span = match library {
        Some(index) => name_span(source, path, index, Some(item.index)),
        None => name_span(source, path, item.index, None),
    };
    let what = if library.is_some() && item.kind == ItemKind::Constant {
        "library constant"
    } else if library.is_some() {
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

fn unused_beside(path: &str, source: &ast::SourceFile, item: &Item, primary: &Item) -> Diagnostic {
    let (name, primary) = (&item.name, &primary.name);
    Diagnostic::new(
        &rule::LAYOUT_UNUSED_DECLARATION,
        format!("`{name}` isn't used by `{primary}`"),
    )
    .at(
        name_span(source, path, item.index, None),
        format!("`{primary}` doesn't use this"),
    )
    .reason(
        "Everything in a file besides the declaration it is named after is there for that declaration, used by it directly or through one another; anything else is dead code. Tests don't count as users.",
    )
    .next(format!("Remove `{name}`"))
}

/// The translation calls the application's code makes, as `<scope>.<key>`:
/// what `jadpo translations unused` compares the messages with. Tests do
/// not count as users.
pub fn translation_keys_used(db: &dyn Db, project: Project) -> BTreeSet<String> {
    let mut used = BTreeSet::new();
    for &file in project.files(db) {
        if location(db, project, file).test {
            continue;
        }
        for callable in callables_of(db, file) {
            for (_, callee) in typed_body(db, project, callable).body.calls() {
                if let Callee::Translation(key) = callee {
                    used.insert(key.clone());
                }
            }
        }
    }
    used
}
