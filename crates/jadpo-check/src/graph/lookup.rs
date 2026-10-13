//! What the name at a place in a file refers to, for the language server:
//! a node of the graph, or a parameter or variable. The checker's own
//! resolution decides, so the editor and `jadpo check` never disagree.

use jadpo_db::{Db, File, Project, symbols};
use jadpo_graph::Key;
use jadpo_syntax::SyntaxKind::*;
use jadpo_syntax::ast::{self, AstNode, Expr};
use jadpo_syntax::{SyntaxNode, SyntaxToken, TokenAtOffset};

use super::paths::Resolver;
use super::{declaration_key, field_key, graph, library_function_key, registry, variant_key};
use crate::tree::{self, dotted};
use crate::types::{TypeRef, callables, type_table};

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
    let lookup = Lookup {
        resolver: Resolver {
            symbols: symbols(db, project),
            table: type_table(db, project),
            callables: callables(db, project),
        },
    };
    let found = match parent.kind() {
        NAME => lookup.name(&parent, token.text())?,
        NAME_REF => lookup.name_ref(&parent, token.text())?,
        _ => return None,
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

struct Lookup<'a> {
    resolver: Resolver<'a>,
}

impl Lookup<'_> {
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
                    Some(library) => library_function_key(&library, text),
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
                1 => variant_key(&host, variant),
                _ => field_key(&host, &path[..2]),
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
            .map(|_| Found::Node(field_key(&host, &path[..1])))
    }

    /// A name where it is used.
    fn name_ref(&self, name_ref: &SyntaxNode, text: &str) -> Option<Found> {
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
        // The whole dotted chain this name is part of, up to this name: from
        // the field read it names, or the name itself when it comes first.
        let mut outer = match name_ref.parent() {
            Some(parent)
                if parent.kind() == FIELD_EXPR
                    && ast::FieldExpr::cast(parent.clone())
                        .and_then(|field| field.name_ref())
                        .is_some_and(|name| name.syntax() == name_ref) =>
            {
                parent
            }
            _ => name_ref.clone(),
        };
        while let Some(parent) = outer.parent()
            && parent.kind() == FIELD_EXPR
            && ast::FieldExpr::cast(parent.clone())
                .and_then(|field| field.expr())
                .is_some_and(|expr| expr.syntax() == &outer)
        {
            outer = parent;
        }
        let expr = Expr::cast(outer.clone())?;
        let (first, rest) =
            dotted(&expr).or_else(|| Some((ast::NameRef::cast(name_ref.clone())?, Vec::new())))?;
        let chain: Vec<ast::NameRef> = std::iter::once(first).chain(rest).collect();
        let at = chain.iter().position(|name| name.syntax() == name_ref)?;
        let names: Vec<String> = chain[..=at]
            .iter()
            .filter_map(tree::name_ref_text)
            .collect();
        let head = names.first()?;
        if head.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
            return self.resolver.path(&names).map(Found::Node);
        }
        let symbols = self.resolver.symbols;
        let in_scope = declared_type(name_ref, head);
        match names.as_slice() {
            [_] => {
                if let Some(found) = in_scope {
                    return Some(found);
                }
                let called = chain.len() > 1 || is_callee(&outer);
                if chain.len() > 1 && symbols.library(text).is_some() {
                    return Some(Found::Node(registry().key(
                        jadpo_graph::Kind::Library,
                        text,
                        None,
                    )));
                }
                if chain.len() > 1 && symbols.domains.contains(text) {
                    return Some(Found::Node(registry().key(
                        jadpo_graph::Kind::Domain,
                        text,
                        None,
                    )));
                }
                if called {
                    let library = name_ref
                        .ancestors()
                        .find_map(ast::LibraryDecl::cast)
                        .and_then(|library| library.name())
                        .as_ref()
                        .and_then(tree::name_text);
                    if let Some(library) = library
                        && self
                            .resolver
                            .callables
                            .library
                            .contains_key(&(library.clone(), text.to_string()))
                    {
                        return Some(Found::Node(library_function_key(&library, text)));
                    }
                }
                declaration_key(symbols, text).map(Found::Node)
            }
            [head_name, member] => {
                if let Some(Found::Local { ty: Some(ty), .. }) = in_scope {
                    let host = match TypeRef::parse(&ty) {
                        TypeRef::Path(names, args) if names.len() == 1 && args.is_empty() => {
                            names[0].clone()
                        }
                        _ => return None,
                    };
                    let path = [member.clone()];
                    return self
                        .resolver
                        .table
                        .field(&host, &path)
                        .map(|_| Found::Node(field_key(&host, &path)));
                }
                if self
                    .resolver
                    .callables
                    .library
                    .contains_key(&(head_name.clone(), member.clone()))
                {
                    return Some(Found::Node(library_function_key(head_name, member)));
                }
                if symbols.domains.contains(head_name) {
                    return declaration_key(symbols, member).map(Found::Node);
                }
                None
            }
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

/// Whether an expression is the callee of a call.
fn is_callee(expr: &SyntaxNode) -> bool {
    expr.parent()
        .and_then(ast::CallExpr::cast)
        .and_then(|call| call.expr())
        .is_some_and(|callee| callee.syntax() == expr)
}

/// The parameter or variable called `name` in scope at `at`: the nearest
/// declaration before it in an enclosing callable.
fn declared_type(at: &SyntaxNode, name: &str) -> Option<Found> {
    let offset = at.text_range().start();
    let callable = at.ancestors().find(|node| {
        matches!(
            node.kind(),
            FUNCTION_DECL | ACTION_DECL | QUERY_DECL | FIXTURE_DECL | TEST_DECL | SEED_DECL
        )
    })?;
    let mut found = None;
    for node in callable.descendants() {
        if node.text_range().start() >= offset {
            break;
        }
        let Some(declared) = ast::Name::cast(node.clone()) else {
            continue;
        };
        if tree::name_text(&declared).as_deref() != Some(name) {
            continue;
        }
        let holder = node.parent()?;
        found = match holder.kind() {
            PARAM => Some(local(
                name,
                ast::Param::cast(holder)?
                    .ty()
                    .map(|ty| tree::written(ty.syntax())),
                "parameter",
            )),
            VAR_STMT => Some(local(
                name,
                ast::VarStmt::cast(holder)?
                    .ty()
                    .map(|ty| tree::written(ty.syntax())),
                "variable",
            )),
            FOR_STMT => Some(local(name, None, "loop item")),
            PAT_BINDING => Some(local(name, None, "binding")),
            LAMBDA_EXPR => Some(local(name, None, "inline parameter")),
            _ => found,
        };
    }
    found
}
