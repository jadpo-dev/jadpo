//! Area 1, "Unused `public`" (Richard, 2026-10-14): `public` offers a
//! domain's declaration to the rest of the project, so on a declaration
//! nothing outside its domain uses it is `visibility.public_unused`, with
//! an edit that removes it.
//!
//! A user is any file outside the declaration's domain that names it:
//! another domain's code or tests, root `tests/` scenarios, seeds. The
//! domain's own files, its tests included, don't count. A public
//! declaration a user reaches also uses whatever public declarations its
//! signature names (a parameter's type, a result, a failure it lets out, a
//! type's fields), transitively, since the user has to build or handle
//! them. Only domain declarations are checked: a package's public members
//! are its interface to other projects (area 11), and anything owned by
//! something other than a domain is left alone.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, File, ItemKind, Layout, Ownership, Project, item_tree, location};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_syntax::SyntaxKind;
use jadpo_syntax::ast::{self, AstNode};

use crate::tree;
use crate::usage::mentions;

/// Every name a file mentions: each name written in it, and what its typed
/// bodies say each call and name refers to.
#[salsa::tracked(returns(ref))]
fn named_in(db: &dyn Db, project: Project, file: File) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    if let Some(source) = tree::source(db, file) {
        // A `use` line says where a name comes from; only its uses count.
        for name in source
            .decls()
            .flat_map(|decl| decl.syntax().descendants().collect::<Vec<_>>())
            .filter_map(ast::NameRef::cast)
        {
            if let Some(text) = tree::name_ref_text(&name) {
                names.insert(text);
            }
        }
    }
    let found = mentions(db, project, file);
    names.extend(found.names.iter().cloned());
    names.extend(found.raised.iter().cloned());
    names.extend(
        found
            .library_calls
            .iter()
            .map(|(library, _)| library.clone()),
    );
    names
}

/// One public declaration of a domain.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Public {
    domain: String,
    /// The names its signature mentions: everything outside its bodies.
    signature: BTreeSet<String>,
}

/// The public declarations of the project's domains that something
/// outside their domain uses, by name.
#[salsa::tracked(returns(ref))]
fn used_publics(db: &dyn Db, project: Project) -> BTreeSet<String> {
    let mut publics: BTreeMap<String, Public> = BTreeMap::new();
    for &file in project.files(db) {
        let location = location(db, project, file);
        let Ownership::Domain(domain) = &location.ownership else {
            continue;
        };
        if location.test {
            continue;
        }
        let Some(source) = tree::source(db, file) else {
            continue;
        };
        for item in item_tree(db, file).items.iter().filter(|item| item.public) {
            let signature = tree::decl_at(&source, item.index)
                .map(|decl| signature_names(decl.syntax()))
                .unwrap_or_default();
            publics.insert(
                item.name.clone(),
                Public {
                    domain: domain.clone(),
                    signature,
                },
            );
        }
    }
    let mut used: BTreeSet<String> = BTreeSet::new();
    for &file in project.files(db) {
        let home = location(db, project, file).ownership.domain();
        for name in named_in(db, project, file) {
            if let Some(public) = publics.get(name)
                && home != Some(public.domain.as_str())
            {
                used.insert(name.clone());
            }
        }
    }
    let mut queue: Vec<String> = used.iter().cloned().collect();
    while let Some(name) = queue.pop() {
        let Some(public) = publics.get(&name) else {
            continue;
        };
        for named in &public.signature {
            if publics.contains_key(named) && used.insert(named.clone()) {
                queue.push(named.clone());
            }
        }
    }
    used
}

/// The names written in a declaration outside its bodies: a callable's
/// parameters, result and `fails` list, a type's fields and base, a
/// library's functions' signatures.
fn signature_names(node: &jadpo_syntax::SyntaxNode) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut pending = vec![node.clone()];
    while let Some(next) = pending.pop() {
        for child in next.children() {
            if child.kind() == SyntaxKind::BLOCK {
                continue;
            }
            if let Some(name) = ast::NameRef::cast(child.clone())
                && let Some(text) = tree::name_ref_text(&name)
            {
                names.insert(text);
            }
            pending.push(child);
        }
    }
    names
}

/// The kinds a domain may offer (`visibility.public_not_allowed` covers
/// the rest).
fn may_be_public(kind: &ItemKind) -> bool {
    matches!(
        kind,
        ItemKind::Type(_)
            | ItemKind::Entity
            | ItemKind::Query
            | ItemKind::Library
            | ItemKind::Action
            | ItemKind::Fixture
    )
}

/// `visibility.public_unused` for one file's public declarations.
#[salsa::tracked(returns(ref))]
pub fn public_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    if project.layout(db) != Layout::Project {
        return Vec::new();
    }
    let location = location(db, project, file);
    let Ownership::Domain(domain) = &location.ownership else {
        return Vec::new();
    };
    if location.test {
        return Vec::new();
    }
    let Some(source) = tree::source(db, file) else {
        return Vec::new();
    };
    let path = file.path(db);
    let used = used_publics(db, project);
    let mut out = Vec::new();
    for item in item_tree(db, file).items.iter().filter(|item| item.public) {
        // `public` where it isn't allowed is that error alone.
        if used.contains(&item.name) || !may_be_public(&item.kind) {
            continue;
        }
        let Some(visibility) = tree::decl_at(&source, item.index).and_then(|decl| {
            decl.syntax()
                .children()
                .find(|child| child.kind() == SyntaxKind::VISIBILITY)
        }) else {
            continue;
        };
        let range = visibility.text_range();
        let span = Span::new(path, range.start().into(), range.end().into());
        out.push(
            Diagnostic::new(
                &rule::VISIBILITY_PUBLIC_UNUSED,
                format!(
                    "`{}` is `public`, but nothing outside `{domain}` uses it",
                    item.name
                ),
            )
            .at(span.clone(), "offered to no one")
            .reason(format!(
                "`public` offers a declaration to other domains, and no other domain's code or tests, no root `tests/` scenario and no public declaration they use names `{}`; `{domain}`'s own tests don't count.",
                item.name
            ))
            .next("Remove `public`")
            .edit(Span::new(path, span.start, span.end + 1), ""),
        );
    }
    out
}
