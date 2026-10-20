//! What the name at a place in a file refers to, for the language server:
//! a node of the graph, or a parameter or variable. Inside a callable, its
//! typed body says, which records what the check resolved each name to, so
//! the editor, `jadpo check` and the graph never disagree. Outside bodies,
//! names are declarations, their members, and types, resolved as the
//! checker resolves types.

use jadpo_db::{Db, File, Project};
use jadpo_graph::Key;
use jadpo_graph::body::Refers;
use jadpo_syntax::SyntaxKind::*;
use jadpo_syntax::ast::{self, AstNode, Decl, Expr};
use jadpo_syntax::{SyntaxNode, SyntaxToken, TokenAtOffset};

use super::paths::Resolver;
use super::{declaration_key, field_key, graph, library_function_key, variant_key};
use crate::lookups::Lookup;
use crate::tree::{self, dotted};
use crate::types::{Callable, typed_body};

/// What a name refers to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Found {
    /// A node of the graph.
    Node(Key),
    /// A parameter, variable, loop item or binding: its name, its type as
    /// written when it states one, and what it is.
    Local {
        name: String,
        ty: Option<String>,
        what: &'static str,
    },
}

/// The name at byte `offset` of `file`, and what it refers to, with the
/// name's own byte range.
pub fn lookup(
    db: &dyn Db,
    project: Project,
    file: File,
    offset: u32,
) -> Option<(Found, (u32, u32))> {
    let source = tree::source(db, file)?;
    let token = name_token(source.syntax().token_at_offset(offset.into()))?;
    let range = (
        u32::from(token.text_range().start()),
        u32::from(token.text_range().end()),
    );
    let parent = token.parent()?;
    let names = Lookup::for_file(db, project, file);
    let lookup = Names {
        resolver: Resolver {
            symbols: &names,
            table: &names,
        },
    };
    let found = match in_body(db, project, file, &source, range.0) {
        Some(found) => found,
        None => match parent.kind() {
            NAME => lookup.name(&parent, token.text())?,
            NAME_REF => lookup.name_ref(&parent, token.text())?,
            _ => return None,
        },
    };
    match &found {
        Found::Node(key) if graph(db, project).find(key).is_none() => None,
        _ => Some((found, range)),
    }
}

/// The identifier at an offset, preferring one on either side of it.
fn name_token(tokens: TokenAtOffset<SyntaxToken>) -> Option<SyntaxToken> {
    match tokens {
        TokenAtOffset::None => None,
        TokenAtOffset::Single(token) => (token.kind() == IDENT).then_some(token),
        TokenAtOffset::Between(left, right) => {
            if right.kind() == IDENT {
                Some(right)
            } else {
                (left.kind() == IDENT).then_some(left)
            }
        }
    }
}

/// What the typed body of the callable around `offset` says the name
/// starting there refers to.
fn in_body(
    db: &dyn Db,
    project: Project,
    file: File,
    source: &ast::SourceFile,
    offset: u32,
) -> Option<Found> {
    let callable = callable_at(db, file, source, offset, false)?;
    let body = &typed_body(db, project, callable).body;
    let name = body.name_at(offset)?;
    Some(match &name.refers {
        Refers::Local(id) => {
            let local = body.local(*id);
            let ty = local.ty.to_string();
            Found::Local {
                name: local.name.clone(),
                ty: (ty != "?").then_some(ty),
                what: local.kind.word(),
            }
        }
        Refers::Node { key, .. } => Found::Node(key.clone()),
    })
}

/// The callable with a body around byte `offset` of `file`: a function,
/// a constant, an action, a fixture, a test, a seed, or a library's
/// function or constant. `at_end` counts the byte just past it, where the
/// editor's cursor stands after its last character.
pub(crate) fn callable_at<'db>(
    db: &'db dyn Db,
    file: File,
    source: &ast::SourceFile,
    offset: u32,
    at_end: bool,
) -> Option<Callable<'db>> {
    let contains = |node: &SyntaxNode| {
        let range = node.text_range();
        u32::from(range.start()) <= offset
            && (offset < u32::from(range.end()) || (at_end && offset == u32::from(range.end())))
    };
    let (index, decl) = source
        .decls()
        .enumerate()
        .find(|(_, decl)| contains(decl.syntax()))?;
    let function = match &decl {
        Decl::LibraryDecl(library) => Some(
            tree::library_members(library)
                .iter()
                .position(|member| contains(member.syntax()))? as u32,
        ),
        Decl::FunctionDecl(_)
        | Decl::ConstantDecl(_)
        | Decl::ActionDecl(_)
        | Decl::FixtureDecl(_)
        | Decl::TestDecl(_)
        | Decl::SeedDecl(_) => None,
        _ => return None,
    };
    Some(Callable::new(db, file, index as u32, function))
}

struct Names<'a> {
    resolver: Resolver<'a>,
}

impl Names<'_> {
    /// A name where it is declared.
    fn name(&self, name: &SyntaxNode, text: &str) -> Option<Found> {
        let holder = name.parent()?;
        match holder.kind() {
            FUNCTION_DECL => {
                let library = holder
                    .ancestors()
                    .skip(1)
                    .find_map(ast::LibraryDecl::cast)
                    .and_then(|library| library.name())
                    .as_ref()
                    .and_then(tree::name_text);
                Some(Found::Node(match library {
                    Some(library) => library_function_key(
                        self.resolver.symbols.registry(),
                        &self.resolver.symbols.canonical(&library),
                        text,
                    ),
                    None => declaration_key(self.resolver.symbols, text)?,
                }))
            }
            TYPE_DECL | ACTION_DECL | QUERY_DECL | FIXTURE_DECL | ENTITY_DECL | LIBRARY_DECL
            | ROUTES_DECL | SCHEDULE_DECL | SUBSCRIBER_DECL | SEED_DECL => {
                Some(Found::Node(declaration_key(self.resolver.symbols, text)?))
            }
            MEMBER_KEY => self.member(&holder.parent()?),
            BARE_MEMBER => self.member(&holder),
            PARAM => {
                let param = ast::Param::cast(holder)?;
                Some(local(
                    text,
                    param.ty().map(|ty| tree::written(ty.syntax())),
                    "parameter",
                ))
            }
            VAR_STMT => {
                let var = ast::VarStmt::cast(holder)?;
                Some(local(
                    text,
                    var.ty().map(|ty| tree::written(ty.syntax())),
                    "variable",
                ))
            }
            FOR_STMT => Some(local(text, None, "loop item")),
            PAT_BINDING => Some(local(text, None, "binding")),
            LAMBDA_EXPR => Some(local(text, None, "inline parameter")),
            _ => None,
        }
    }

    /// A member of a type or entity: a field, a variant or a variant's
    /// field.
    fn member(&self, member: &SyntaxNode) -> Option<Found> {
        let mut path = Vec::new();
        let mut node = member.clone();
        let decl = loop {
            match node.kind() {
                MEMBER => {
                    let name = ast::Member::cast(node.clone())?
                        .member_key()?
                        .names()
                        .next()
                        .as_ref()
                        .and_then(tree::name_text)?;
                    path.push(name);
                }
                BARE_MEMBER => {
                    path.push(
                        ast::BareMember::cast(node.clone())?
                            .name()
                            .as_ref()
                            .and_then(tree::name_text)?,
                    );
                }
                TYPE_DECL | ENTITY_DECL => break node,
                // A field given while building a value: `Note { body: ... }`.
                RECORD_EXPR => {
                    let record = ast::RecordExpr::cast(node)?;
                    let (first, rest) = dotted(&record.expr()?)?;
                    let names: Vec<String> = std::iter::once(&first)
                        .chain(rest.iter())
                        .filter_map(tree::name_ref_text)
                        .collect();
                    path.reverse();
                    let [host] = names.as_slice() else {
                        return None;
                    };
                    let field = path.first()?;
                    return self
                        .resolver
                        .table
                        .field(host, std::slice::from_ref(field))
                        .map(|_| {
                            Found::Node(field_key(
                                self.resolver.symbols,
                                host,
                                std::slice::from_ref(field),
                            ))
                        });
                }
                MEMBER_LIST => {}
                _ => return None,
            }
            node = node.parent()?;
        };
        path.reverse();
        let host = tree::decl_name(&ast::Decl::cast(decl)?)
            .as_ref()
            .and_then(tree::name_text)?;
        let table = self.resolver.table;
        if let Some(variants) = table.variants(&host) {
            let variant = path.first()?;
            if !variants.iter().any(|v| &v.name == variant) {
                return None;
            }
            return Some(Found::Node(match path.len() {
                1 => variant_key(self.resolver.symbols, &host, variant),
                _ => field_key(self.resolver.symbols, &host, &path[..2]),
            }));
        }
        // A failure's fields sit inside `client: { }` or `internal: { }`.
        if matches!(
            path.first().map(String::as_str),
            Some("client" | "internal")
        ) && path.len() > 1
        {
            path.remove(0);
        }
        table
            .field(&host, &path[..1])
            .map(|_| Found::Node(field_key(self.resolver.symbols, &host, &path[..1])))
    }

    /// A name where it is used outside a body: a type, its member, a
    /// declaration named by name, or a name on a `use` line (area 1,
    /// Imports): what it brings in, or the domain `use menu` names.
    fn name_ref(&self, name_ref: &SyntaxNode, text: &str) -> Option<Found> {
        match name_ref.parent().map(|parent| parent.kind()) {
            Some(USE_NAMES) => {
                return declaration_key(self.resolver.symbols, text).map(Found::Node);
            }
            Some(USE_PATH) => {
                let first = name_ref.prev_sibling().is_none();
                return (first && self.resolver.symbols.is_domain(text)).then(|| {
                    Found::Node(self.resolver.symbols.registry().key(
                        jadpo_graph::Kind::Domain,
                        text,
                        None,
                    ))
                });
            }
            _ => {}
        }
        if let Some(path) = name_ref.parent().and_then(ast::Path::cast) {
            let names: Vec<String> = path
                .name_refs()
                .map_while(|name| {
                    let written = tree::name_ref_text(&name)?;
                    Some((written, name.syntax() == name_ref))
                })
                .scan(false, |done, (written, this)| {
                    if *done {
                        return None;
                    }
                    *done = this;
                    Some(written)
                })
                .collect();
            return self.resolver.path(&names).map(Found::Node);
        }
        // The dotted chain this name is part of, up to this name.
        let mut outer = name_ref.clone();
        while let Some(parent) = outer.parent()
            && parent.kind() == FIELD_EXPR
        {
            outer = parent;
        }
        let expr = Expr::cast(outer)?;
        let (first, rest) =
            dotted(&expr).or_else(|| Some((ast::NameRef::cast(name_ref.clone())?, Vec::new())))?;
        let chain: Vec<ast::NameRef> = std::iter::once(first).chain(rest).collect();
        let at = chain.iter().position(|name| name.syntax() == name_ref)?;
        let names: Vec<String> = chain[..=at]
            .iter()
            .filter_map(tree::name_ref_text)
            .collect();
        if names
            .first()
            .is_some_and(|head| head.starts_with(|c: char| c.is_ascii_uppercase()))
        {
            return self.resolver.path(&names).map(Found::Node);
        }
        match names.as_slice() {
            [_] => declaration_key(self.resolver.symbols, text).map(Found::Node),
            _ => None,
        }
    }
}

fn local(name: &str, ty: Option<String>, what: &'static str) -> Found {
    Found::Local {
        name: name.to_string(),
        ty,
        what,
    }
}
