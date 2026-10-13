//! Reading positions and names back from a file's syntax tree. Queries keep
//! no positions of other files, so every span here is in the file being
//! checked.

use jadpo_db::{Db, File, parse};
use jadpo_diag::Span;
use jadpo_syntax::SyntaxNode;
use jadpo_syntax::ast::{self, AstNode, Decl, Expr, MemberItem};

pub fn source(db: &dyn Db, file: File) -> Option<ast::SourceFile> {
    ast::SourceFile::cast(parse(db, file).syntax())
}

pub fn decl_at(source: &ast::SourceFile, index: u32) -> Option<Decl> {
    source.decls().nth(index as usize)
}

/// The name a declaration is written with.
pub fn decl_name(decl: &Decl) -> Option<ast::Name> {
    match decl {
        Decl::TypeDecl(d) => d.name(),
        Decl::FunctionDecl(d) => d.name(),
        Decl::ActionDecl(d) => d.name(),
        Decl::QueryDecl(d) => d.name(),
        Decl::FixtureDecl(d) => d.name(),
        Decl::EntityDecl(d) => d.name(),
        Decl::LibraryDecl(d) => d.name(),
        Decl::RoutesDecl(d) => d.name(),
        Decl::ScheduleDecl(d) => d.name(),
        Decl::SubscriberDecl(d) => d.name(),
        Decl::SeedDecl(d) => d.name(),
        Decl::ResourceDecl(d) => d.name(),
        Decl::ProvideDecl(d) => d.name(),
        Decl::TestDecl(_) | Decl::PackageDecl(_) | Decl::SingletonDecl(_) => None,
    }
}

/// The functions a library holds, in order.
pub fn library_functions(library: &ast::LibraryDecl) -> Vec<ast::FunctionDecl> {
    library
        .member_list()
        .into_iter()
        .flat_map(|list| list.member_items())
        .filter_map(|item| match item {
            MemberItem::Decl(Decl::FunctionDecl(function)) => Some(function),
            _ => None,
        })
        .collect()
}

/// The span of the name of declaration `index`, or of function `function`
/// in it when it is a library; the whole declaration when it has no name.
pub fn name_span(source: &ast::SourceFile, path: &str, index: u32, function: Option<u32>) -> Span {
    let Some(decl) = decl_at(source, index) else {
        return Span::new(path, 0, 0);
    };
    let name = match (&decl, function) {
        (Decl::LibraryDecl(library), Some(i)) => library_functions(library)
            .get(i as usize)
            .and_then(ast::FunctionDecl::name),
        _ => decl_name(&decl),
    };
    match name {
        Some(name) => node_span(path, name.syntax()),
        None => node_span(path, decl.syntax()),
    }
}

/// A node's span without the comments and whitespace kept beside its
/// tokens, so a label never runs on into a trailing comment.
pub fn node_span(path: &str, node: &SyntaxNode) -> Span {
    let range = node.text_range();
    let mut tokens = node
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia());
    let Some(first) = tokens.next() else {
        return Span::new(path, range.start().into(), range.end().into());
    };
    let last = tokens.last().unwrap_or_else(|| first.clone());
    Span::new(
        path,
        first.text_range().start().into(),
        last.text_range().end().into(),
    )
}

/// A span covering a whole file: the location prints the path alone.
pub fn file_span(path: &str) -> Span {
    Span::new(path, 0, 0)
}

pub fn name_text(name: &ast::Name) -> Option<String> {
    Some(name.ident_token()?.text().to_string())
}

pub fn name_ref_text(name: &ast::NameRef) -> Option<String> {
    Some(name.ident_token()?.text().to_string())
}

/// `a.b.c` as its first name and the names after it, when it starts with
/// a name.
pub fn dotted(expr: &Expr) -> Option<(ast::NameRef, Vec<ast::NameRef>)> {
    match expr {
        Expr::NameRef(name) => Some((name.clone(), Vec::new())),
        Expr::FieldExpr(field) => {
            let inner = field.expr()?;
            let (first, mut rest) = dotted(&inner)?;
            rest.push(field.name_ref()?);
            Some((first, rest))
        }
        _ => None,
    }
}

/// The members of a block with a single-name key.
pub fn members(list: Option<ast::MemberList>) -> Vec<(ast::Name, ast::Member)> {
    list.into_iter()
        .flat_map(|list| list.member_items())
        .filter_map(|item| match item {
            MemberItem::Member(member) => {
                let mut names = member.member_key()?.names();
                let first = names.next()?;
                names.next().is_none().then_some((first, member))
            }
            _ => None,
        })
        .collect()
}

/// A node's tokens without trivia: `List<Tag>`, `/todos`, `Todo.id`.
pub fn written(node: &SyntaxNode) -> String {
    node.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .map(|token| token.text().to_string())
        .collect()
}
