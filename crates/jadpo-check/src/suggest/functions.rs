//! The app's own functions and actions against ones that already exist
//! (area 14, "Signals"; roadmap R8): the `published` signal, a copy of a
//! function or action a registry package publishes, and the `function`
//! signal, a function named for a standard operation that takes and gives
//! what it does.
//!
//! - **Published.** Each own function's and action's source fingerprint is
//!   worked out as a release's is ([`crate::release::digest_of`]: its
//!   tokens as the formatter writes them, without its description,
//!   comments, visibility, `decline` lines or own name, its parameters and
//!   variables named by position, as the similar-helper check names them),
//!   so a renamed, reformatted copy of a published function has the
//!   published one's fingerprint. The registry is asked by prefix
//!   ([`prefixes`]), never by the fingerprints themselves, and answers every
//!   published fingerprint with one of them; they are compared here.
//! - **Function.** A standard operation has no source of its own to
//!   fingerprint, so its fingerprint is its name and its parameters and
//!   result as the operation table gives them: a function fires when its
//!   own name is the operation's (with or without the operation's
//!   namespace) and each of its parameters and its result fits the
//!   operation's.
//!
//! Installed packages, tests, fixtures and seeds are left out, and a
//! package of the app's own is never told about its own releases.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, Ownership, Project};
use jadpo_graph::{Detail, Graph, Kind, TypeRef};
use jadpo_std::{Builtin, Kind as OpKind, OPERATIONS, Operation};
use jadpo_syntax::SyntaxNode;
use jadpo_syntax::ast::{self, AstNode, Decl};

use super::canonical::C;
use super::{Finding, Signal, Snippet, base_builtin, first_line, hash, line_of, slice, words};
use crate::tree;

/// How many hex digits of a source fingerprint the registry is asked
/// about: the protocol's `POST fingerprints/query` prefix.
pub const PREFIX_DIGITS: usize = 5;

/// One of the app's own functions or actions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Own {
    /// As written in its file: `parse`, or `postcodes.parse` in a library.
    pub name: String,
    pub kind: &'static str,
    /// `sha256:…` of its normalised source, as a release's `source` is.
    pub source: String,
    pub snippet: Snippet,
    /// The publisher the package it is in names, when it is in one of the
    /// app's own packages.
    pub publisher: Option<String>,
}

/// A published function or action, as the registry answers it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Published {
    /// `@acme/postcodes`.
    pub package: String,
    pub version: String,
    /// `uk.parse`.
    pub declaration: String,
    /// `sha256:…`.
    pub source: String,
}

/// The app's own functions and actions, in file and source order.
pub fn own_functions(db: &dyn Db, project: Project) -> Vec<Own> {
    let mut files: Vec<jadpo_db::File> = project.files(db).clone();
    files.sort_by(|a, b| a.path(db).cmp(b.path(db)));
    let mut out = Vec::new();
    for file in files {
        let path = file.path(db);
        if path.starts_with(&format!("{}/", jadpo_db::PACKAGES)) || path.ends_with(".test.jadpo") {
            continue;
        }
        let publisher = match &jadpo_db::location(db, project, file).ownership {
            Ownership::Package(home) => {
                crate::surface::surface(db, project, &home.folder).and_then(|s| s.publisher)
            }
            _ => None,
        };
        let Some(source) = tree::source(db, file) else {
            continue;
        };
        let text = file.text(db);
        let mut add = |name: String,
                       kind: &'static str,
                       node: &SyntaxNode,
                       own: Option<ast::Name>| {
            let source = crate::release::digest_of(node, own.as_ref(), true);
            let tokens =
                crate::graph::similar::normalised(node, own.as_ref().map(AstNode::syntax), true);
            let canonical = C::Tag("source", vec![C::Atom(tokens.join(" "))]).render();
            let span = tree::node_span(path, node);
            let whole = slice(text, span.start, span.end);
            let signature = whole
                .find('{')
                .map_or(whole.as_str(), |at| &whole[..at])
                .trim_end();
            out.push(Own {
                name,
                kind,
                source,
                snippet: Snippet {
                    path: path.clone(),
                    line: line_of(text, span.start),
                    start: span.start,
                    end: span.end,
                    shown: first_line(signature),
                    hash: hash(&canonical),
                    text_hash: hash(&super::without_comments(&whole)),
                    text: whole,
                    canonical,
                },
                publisher: publisher.clone(),
            });
        };
        for decl in source.decls() {
            match &decl {
                Decl::FunctionDecl(function) => {
                    if let Some(name) = function.name().as_ref().and_then(tree::name_text) {
                        add(name, "function", function.syntax(), function.name());
                    }
                }
                Decl::ActionDecl(action) => {
                    if let Some(name) = action.name().as_ref().and_then(tree::name_text) {
                        add(name, "action", action.syntax(), action.name());
                    }
                }
                Decl::LibraryDecl(library) => {
                    let Some(library_name) = library.name().as_ref().and_then(tree::name_text)
                    else {
                        continue;
                    };
                    for member in tree::library_members(library) {
                        let Decl::FunctionDecl(function) = &member else {
                            continue;
                        };
                        if let Some(name) = function.name().as_ref().and_then(tree::name_text) {
                            add(
                                format!("{library_name}.{name}"),
                                "function",
                                function.syntax(),
                                function.name(),
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// The prefixes to ask the registry about: the first [`PREFIX_DIGITS`]
/// hex digits of each own source fingerprint, each once and sorted, so
/// the request says nothing of the order or number of functions sharing
/// one.
pub fn prefixes(own: &[Own]) -> Vec<String> {
    let set: BTreeSet<String> = own
        .iter()
        .filter_map(|function| {
            let hex = function.source.strip_prefix("sha256:")?;
            hex.get(..PREFIX_DIGITS).map(str::to_string)
        })
        .collect();
    set.into_iter().collect()
}

/// Each own function or action whose source fingerprint is a published
/// one's, naming the first package and declaration the registry gave.
pub(super) fn published(own: &[Own], published: &[Published], found: &mut Vec<Finding>) {
    for function in own {
        let Some(copy) = published.iter().find(|p| {
            p.source == function.source
                && function
                    .publisher
                    .as_deref()
                    .is_none_or(|own| !p.package.starts_with(&format!("{own}/")))
        }) else {
            continue;
        };
        let written = function.name.rsplit('.').next().unwrap_or(&function.name);
        found.push(Finding {
            snippet: function.snippet.clone(),
            signal: Signal::Published,
            suggestion: format!("{} {}", copy.package, copy.declaration),
            reason: format!(
                "`{written}` is a copy of `{}` in `{}` {}, apart from the names of its variables, so a fix or an advisory for it never reaches this copy.",
                copy.declaration, copy.package, copy.version
            ),
            strength: usize::MAX,
            order: 0,
            package: Some((copy.package.clone(), copy.version.clone())),
        });
    }
}

/// Each own function named for a standard operation that takes and gives
/// what the operation does.
pub(super) fn standard(graph: &Graph, own: &[Own], found: &mut Vec<Finding>) {
    // Each own function's signature, by where its declaration starts,
    // which the graph places from its first token as a snippet does.
    let mut signatures: BTreeMap<(&str, u32), &jadpo_graph::Signature> = BTreeMap::new();
    for node in graph.nodes() {
        if node.kind != Kind::Function {
            continue;
        }
        let (Some(place), Detail::Callable { signature, .. }) = (&node.place, &node.detail) else {
            continue;
        };
        signatures.insert((place.file.as_str(), place.start), signature);
    }
    for function in own.iter().filter(|f| f.kind == "function") {
        let snippet = &function.snippet;
        let Some(signature) = signatures.get(&(snippet.path.as_str(), snippet.start)) else {
            continue;
        };
        let written = function.name.rsplit('.').next().unwrap_or(&function.name);
        let Some((order, operation)) = OPERATIONS.iter().enumerate().find(|(_, operation)| {
            named_for(written, operation) && takes(graph, signature, operation)
        }) else {
            continue;
        };
        let qualified = operation.qualified();
        let canonical = C::Tag(
            "function",
            std::iter::once(C::Atom(written.to_string()))
                .chain(
                    signature
                        .params
                        .iter()
                        .map(|(_, ty)| C::Atom(ty.to_string())),
                )
                .chain(std::iter::once(C::Atom(signature.result.to_string())))
                .collect(),
        )
        .render();
        found.push(Finding {
            snippet: Snippet {
                hash: hash(&canonical),
                canonical,
                ..snippet.clone()
            },
            signal: Signal::Function,
            suggestion: qualified.clone(),
            reason: format!(
                "`{written}` is named for `{qualified}` and takes and gives what it does, so the standard operation may already do its job."
            ),
            strength: 1,
            order,
            package: None,
        });
    }
}

/// Whether a function's own name is the operation's, with or without the
/// operation's namespace, word for word in any order.
fn named_for(written: &str, operation: &Operation) -> bool {
    let own: BTreeSet<String> = words(written).into_iter().collect();
    let name: BTreeSet<String> = words(operation.name).into_iter().collect();
    if own == name {
        return true;
    }
    let mut qualified = name;
    qualified.extend(words(operation.namespace));
    own == qualified
}

/// Whether a function with `signature` takes and gives what `operation`
/// does: each parameter in order fits the operation's, every one the
/// operation needs is there, and its result fits the operation's.
fn takes(graph: &Graph, signature: &jadpo_graph::Signature, operation: &Operation) -> bool {
    let params: Vec<_> = operation.params.iter().filter(|p| !p.implicit).collect();
    let needed = params.iter().filter(|p| !p.optional).count();
    let given = signature.params.len();
    if given < needed.max(1) || given > params.len() {
        return false;
    }
    let unit = TypeRef::Builtin(Builtin::Unit);
    if signature.result == unit {
        return false;
    }
    signature
        .params
        .iter()
        .zip(&params)
        .all(|((_, ty), param)| {
            let ty = match ty {
                TypeRef::Nullable(inner) if param.optional || param.nullable => inner.as_ref(),
                TypeRef::Nullable(_) => return false,
                other => other,
            };
            fits(graph, ty, param.kind)
        })
        && fits(graph, &signature.result, operation.result)
}

/// Whether a value of `ty` is of the operation's `kind`. Kinds a function
/// can't write as a parameter (an inline function, a unit, a format) never
/// fit.
fn fits(graph: &Graph, ty: &TypeRef, kind: OpKind) -> bool {
    match kind {
        OpKind::Any => !matches!(ty, TypeRef::Nullable(_)),
        OpKind::List => matches!(ty, TypeRef::List(_)),
        OpKind::Set => matches!(ty, TypeRef::Set(_)),
        OpKind::Map => matches!(ty, TypeRef::Map(..)),
        OpKind::Number => matches!(
            base_builtin(graph, ty),
            Some(Builtin::Int | Builtin::Decimal)
        ),
        OpKind::Builtin(builtin) => base_builtin(graph, ty) == Some(builtin),
        OpKind::OneOf(builtins) => base_builtin(graph, ty).is_some_and(|b| builtins.contains(&b)),
        OpKind::Std(name) | OpKind::Enum(name) => standard_named(graph, ty) == Some(name),
        _ => false,
    }
}

/// The standard type `ty` is or is built on.
fn standard_named<'a>(graph: &'a Graph, ty: &'a TypeRef) -> Option<&'a str> {
    let mut ty = ty;
    for _ in 0..16 {
        match ty {
            TypeRef::Std(name, _) => return Some(name.as_str()),
            TypeRef::Node(key) => match graph.get(key).map(|n| &n.detail) {
                Some(Detail::Type {
                    base: jadpo_graph::Base::Refines(inner),
                    ..
                }) => ty = inner,
                _ => return None,
            },
            _ => return None,
        }
    }
    None
}
