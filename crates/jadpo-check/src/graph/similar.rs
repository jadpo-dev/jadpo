//! The similar-helper check (area 1, "Tools for agents" in area 14):
//! when a function does what a library function already does, `jadpo
//! check` says so and names the library function, so a helper is not
//! written twice.
//!
//! "Does the same" is decided by digest: two functions are alike when
//! their parameter types, result, `fails` list and body are the same
//! tokens once each parameter's and variable's name is replaced by its
//! position. That finds copies, renamed or reformatted; a looser
//! likeness would be a later, separate choice. A library function is
//! compared only with the library functions before it, in file and
//! source order, so of two alike only the later is reported.

use std::collections::BTreeMap;

use jadpo_db::{Db, File, Project};
use jadpo_diag::{Diagnostic, rule};
use jadpo_graph::Digest;
use jadpo_syntax::SyntaxKind::{FIELD_EXPR, IDENT, NAME, NAME_REF};
use jadpo_syntax::SyntaxNode;
use jadpo_syntax::ast::{self, AstNode, Decl};

use crate::tree::{self, name_span};

/// One function's likeness digest.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FunctionShape {
    /// `name`, or `library.name` for a library's function.
    pub name: String,
    /// Its declaration's position in the file, and its position in its
    /// library.
    pub index: u32,
    pub function: Option<u32>,
    pub library: bool,
    pub digest: Digest,
}

/// The likeness of every function in a file, in source order. Without
/// positions, so an edit that keeps every function's shape is cut off here.
#[salsa::tracked(returns(ref))]
pub fn function_shapes(db: &dyn Db, file: File) -> Vec<FunctionShape> {
    let Some(source) = tree::source(db, file) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (index, decl) in source.decls().enumerate() {
        match &decl {
            Decl::FunctionDecl(function) => {
                if let Some(name) = function.name().as_ref().and_then(tree::name_text) {
                    out.push(FunctionShape {
                        name,
                        index: index as u32,
                        function: None,
                        library: false,
                        digest: likeness(function),
                    });
                }
            }
            Decl::LibraryDecl(library) => {
                let Some(library_name) = library.name().as_ref().and_then(tree::name_text) else {
                    continue;
                };
                for (i, member) in tree::library_members(library).iter().enumerate() {
                    let Decl::FunctionDecl(function) = member else {
                        continue;
                    };
                    if let Some(name) = function.name().as_ref().and_then(tree::name_text) {
                        out.push(FunctionShape {
                            name: format!("{library_name}.{name}"),
                            index: index as u32,
                            function: Some(i as u32),
                            library: true,
                            digest: likeness(function),
                        });
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Every library function's likeness: the first of each, by file and
/// source order, with its file.
#[salsa::tracked(returns(ref))]
fn library_shapes(db: &dyn Db, project: Project) -> BTreeMap<Digest, (String, String, u32, u32)> {
    let mut files: Vec<File> = project.files(db).clone();
    files.sort_by(|a, b| a.path(db).cmp(b.path(db)));
    let mut out = BTreeMap::new();
    for file in files {
        for shape in function_shapes(db, file) {
            if shape.library {
                out.entry(shape.digest).or_insert_with(|| {
                    (
                        shape.name.clone(),
                        file.path(db).clone(),
                        shape.index,
                        shape.function.unwrap_or(0),
                    )
                });
            }
        }
    }
    out
}

/// The functions in `file` that a library function already does.
#[salsa::tracked(returns(ref))]
pub fn similar_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let shapes = function_shapes(db, file);
    if shapes.is_empty() {
        return Vec::new();
    }
    let libraries = library_shapes(db, project);
    let Some(source) = tree::source(db, file) else {
        return Vec::new();
    };
    let path = file.path(db);
    let mut out = Vec::new();
    for shape in shapes {
        let Some((existing, at, index, function)) = libraries.get(&shape.digest) else {
            continue;
        };
        if *existing == shape.name {
            continue;
        }
        if shape.library {
            let earlier = (at.as_str(), *index, *function)
                < (path.as_str(), shape.index, shape.function.unwrap_or(0));
            if !earlier {
                continue;
            }
        }
        let span = name_span(&source, path, shape.index, shape.function);
        let written = shape.name.rsplit('.').next().unwrap_or(&shape.name);
        out.push(
            Diagnostic::new(
                &rule::LIBRARY_SIMILAR_EXISTS,
                format!("A similar library function exists: `{existing}`"),
            )
            .at(span, format!("same as `{existing}`"))
            .reason(format!(
                "`{written}` takes the same parameters, gives the same result and has the same body as `{existing}` in `{at}`, apart from the names of its variables."
            ))
            .next(format!("Call `{existing}` instead, and remove `{written}`")),
        );
    }
    out
}

/// A function's likeness: its canonical tokens after its own name, with
/// each parameter and variable named by its position, and visibility and
/// `decline` lines left out.
fn likeness(function: &ast::FunctionDecl) -> Digest {
    let own = function.name().map(|name| name.syntax().clone());
    let mut locals: BTreeMap<String, usize> = BTreeMap::new();
    for name in function.syntax().descendants().filter_map(ast::Name::cast) {
        if Some(name.syntax()) == own.as_ref() {
            continue;
        }
        if let Some(text) = tree::name_text(&name) {
            let next = locals.len();
            locals.entry(text).or_insert(next);
        }
    }
    let tokens = jadpo_fmt::canonical_tokens(function.syntax());
    let mut parts: Vec<String> = Vec::new();
    for token in &tokens {
        let Some(parent) = token.parent() else {
            continue;
        };
        if Some(&parent) == own.as_ref()
            || parent.kind() == jadpo_syntax::SyntaxKind::VISIBILITY
            || parent
                .ancestors()
                .any(|node| node.kind() == jadpo_syntax::SyntaxKind::DECLINE)
        {
            continue;
        }
        if token.kind() == IDENT
            && is_local_position(&parent)
            && let Some(position) = locals.get(token.text())
        {
            parts.push(format!("${position}"));
        } else {
            parts.push(token.text().to_string());
        }
    }
    Digest::of(parts.iter().map(String::as_str))
}

/// Whether a name token stands for a local: a binding, or a reference that
/// isn't the field after a dot.
fn is_local_position(parent: &SyntaxNode) -> bool {
    match parent.kind() {
        NAME => true,
        NAME_REF => match parent.parent() {
            Some(field) if field.kind() == FIELD_EXPR => ast::FieldExpr::cast(field)
                .and_then(|field| field.name_ref())
                .is_none_or(|name| name.syntax() != parent),
            _ => true,
        },
        _ => false,
    }
}
