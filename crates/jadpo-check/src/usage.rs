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
use jadpo_syntax::ast::{self, AstNode, Expr};

use crate::case::{starts_upper, to_snake};
use crate::tree::{self, dotted, name_span};

/// The declarations one file mentions.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Mentions {
    /// Upper-case names anywhere, and lower-case names called bare.
    names: BTreeSet<String>,
    /// `library.function(...)` calls.
    library_calls: BTreeSet<(String, String)>,
    /// Failures named by `reject` or a failure mapping.
    raised: BTreeSet<String>,
}

#[salsa::tracked(returns(ref))]
pub fn mentions(db: &dyn Db, file: File) -> Mentions {
    let mut found = Mentions::default();
    let Some(source) = tree::source(db, file) else {
        return found;
    };
    for node in source.syntax().descendants() {
        if let Some(name) = ast::NameRef::cast(node.clone()) {
            if let Some(text) = tree::name_ref_text(&name)
                && starts_upper(&text)
            {
                found.names.insert(text);
            }
        } else if let Some(call) = ast::CallExpr::cast(node.clone()) {
            match call.expr() {
                Some(Expr::NameRef(name)) => {
                    found.names.extend(tree::name_ref_text(&name));
                }
                Some(callee @ Expr::FieldExpr(_)) => {
                    if let Some((first, rest)) = dotted(&callee)
                        && let [function] = rest.as_slice()
                        && let (Some(ownership), Some(function)) =
                            (tree::name_ref_text(&first), tree::name_ref_text(function))
                    {
                        found.library_calls.insert((ownership, function));
                    }
                }
                _ => {}
            }
        } else if let Some(reject) = ast::RejectExpr::cast(node.clone()) {
            let head = match reject.expr() {
                Some(Expr::RecordExpr(record)) => record.expr(),
                Some(Expr::CallExpr(call)) => call.expr(),
                other => other,
            };
            if let Some(Expr::NameRef(name)) = head {
                found.raised.extend(tree::name_ref_text(&name));
            }
        } else if let Some(mapping) = ast::Mapping::cast(node)
            && let Some(Expr::NameRef(name)) = mapping.expr()
        {
            found.raised.extend(tree::name_ref_text(&name));
        }
    }
    found
}

/// Who uses what, across the project's non-test files.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Users {
    /// By declaration name: the files that mention it.
    by_name: BTreeMap<String, BTreeSet<String>>,
    /// By library and function: the files that call it.
    by_function: BTreeMap<(String, String), BTreeSet<String>>,
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
        let path = file.path(db);
        let found = mentions(db, file);
        for name in &found.names {
            users
                .by_name
                .entry(name.clone())
                .or_default()
                .insert(path.clone());
        }
        for call in &found.library_calls {
            users
                .by_function
                .entry(call.clone())
                .or_default()
                .insert(path.clone());
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
    let users = users(db, project);
    let route_file = |user: &String| {
        project
            .files(db)
            .iter()
            .find(|file| file.path(db) == user)
            .is_some_and(|file| location_of(db, project, *file) == Place::Folder(Folder::Routes))
    };
    let domain_of = |user: &String| {
        project
            .files(db)
            .iter()
            .find(|file| file.path(db) == user)
            .map(|file| jadpo_db::location(db, project, *file).ownership.clone())
    };
    let mut out = Vec::new();
    for item in &item_tree(db, file).items {
        match (&location.ownership, folder, &item.kind) {
            (Ownership::Domain(domain), folder, kind)
                if !matches!(folder, Folder::Types | Folder::Libraries)
                    && (*kind == ItemKind::Function || is_plain_type(kind)) =>
            {
                let mut others: Vec<&String> = users
                    .by_name
                    .get(&item.name)
                    .into_iter()
                    .flatten()
                    .filter(|user| *user != path)
                    .collect();
                // A route only counts when nothing else uses the type.
                if folder != Folder::Routes || others.iter().any(|user| !route_file(user)) {
                    others.retain(|user| !route_file(user));
                }
                let same: Vec<&String> = others
                    .iter()
                    .copied()
                    .filter(|user| domain_of(user) == Some(Ownership::Domain(domain.clone())))
                    .collect();
                let elsewhere = others.len() > same.len();
                if elsewhere && item.public {
                    out.push(promote(path, &source, item, "shared", others[0]));
                } else if let Some(user) = same.first() {
                    out.push(promote(
                        path,
                        &source,
                        item,
                        &format!("domains/{domain}"),
                        user,
                    ));
                }
            }
            (Ownership::Domain(_), Folder::Failures, ItemKind::Type(Base::Failure)) => {
                let domains = users.raised_by.get(&item.name).map_or(0, BTreeSet::len);
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
            (_, Folder::Types, kind) if is_plain_type(kind) => {
                let used = users
                    .by_name
                    .get(&item.name)
                    .is_some_and(|found| found.iter().any(|user| user != path));
                if !used {
                    out.push(unused(path, &source, item, None));
                }
            }
            (_, Folder::Libraries, ItemKind::Library) => {
                for function in &item.functions {
                    let used = users
                        .by_function
                        .contains_key(&(item.name.clone(), function.name.clone()));
                    if !used {
                        out.push(unused(path, &source, function, Some(item.index)));
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn location_of(db: &dyn Db, project: Project, file: File) -> Place {
    jadpo_db::location(db, project, file).place.clone()
}

fn is_plain_type(kind: &ItemKind) -> bool {
    matches!(
        kind,
        ItemKind::Type(base) if !matches!(base, Base::Failure | Base::Event | Base::EmailMessage)
    )
}

fn promote(
    path: &str,
    source: &ast::SourceFile,
    item: &Item,
    home: &str,
    user: &str,
) -> Diagnostic {
    let name = &item.name;
    let file = to_snake(name);
    let (what, target) = if item.kind == ItemKind::Function {
        (
            "A helper function used by one file lives in it; once another file uses it, it moves to a library",
            format!(
                "Move `{name}` into a library in `{home}/libraries/` and call it through the library"
            ),
        )
    } else {
        (
            "A type used by one file lives in it; once another file uses it, it moves to `types/`",
            format!("Move `{name}` to `{home}/types/{file}.jadpo`"),
        )
    };
    Diagnostic::new(
        &rule::LAYOUT_PROMOTION_REQUIRED,
        format!("`{name}` is used beyond this file"),
    )
    .at(
        name_span(source, path, item.index, None),
        "used by another file",
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
