//! Completion (area 14, "Completion", decided 2026-10-17): what the editor
//! offers at a place in a file.
//!
//! Every item comes from what the checker itself reads, never from a list
//! kept here: the project's name and type tables, the standard library's
//! stubs (`std/*.jadpo`) and the standard types' entries, `jadpo-std`'s
//! operation, unit and enum tables, the rule catalogue, the settings, the
//! translations' messages, the parser's own keyword sets, and the checked
//! body around the place, which says the type each expression has and the
//! type each position requires. So a type, a unit or an operation a later
//! chunk adds is offered with no code here.
//!
//! The context is read from the tree of the text as it is being typed,
//! which the parser keeps whole however unfinished it is: after a value's
//! dot its fields, after a namespace's its operations, after a type's its
//! variants, after `config.` the settings, after `translations.` the
//! scopes and their messages; straight after a number the units the type
//! it goes into is written with; in a call the named arguments not given
//! yet; in a type position the type names; after `decline` the declinable
//! rules; where a statement or a declaration may start its keywords; and
//! anywhere else in a body the names in scope. Where the position requires
//! a type, what fits it comes first, and a unit or a variant that can't go
//! there isn't offered.

use std::collections::BTreeSet;
use std::sync::Arc;

use jadpo_db::{Db, File, ItemKind, Ownership, Project, Symbol};
use jadpo_syntax::SyntaxKind::*;
use jadpo_syntax::ast::{self, AstNode, Expr};
use jadpo_syntax::{SyntaxKind, SyntaxNode, SyntaxToken, TokenAtOffset};

use crate::case::starts_upper;
use crate::graph::callable_at;
use crate::imports::{Import, domain_imports_for, imports_for};
use crate::lookups::{Lookup, Name, NameView, SigView, file_at};
use crate::std_lib::{Namespace, StdNames};
use crate::tree;
use crate::types::{
    BASES, BodyView, LocalKind, Sig, Ty, TypeRef, TypeView, ViewLocal, members_of, typed_body,
};

/// What the editor offers at one place: the items, and the range each
/// replaces, from the start of what is typed there to the cursor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Completions {
    pub start: u32,
    pub end: u32,
    pub items: Vec<Item>,
}

/// One thing the editor offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub label: String,
    pub what: What,
    /// A signature or a type, on one line.
    pub detail: Option<String>,
    /// Its description, as Markdown.
    pub docs: Option<String>,
    /// What goes in instead of the label: `zone: ` for a named argument,
    /// or a call with its required arguments when `snippet`.
    pub insert: Option<String>,
    /// `insert` is a snippet, with `${1:name}` placeholders.
    pub snippet: bool,
    /// It fits the type the position requires; for a named argument or a
    /// record's field, the call or record can't leave it out. Either way
    /// the editor sorts it first.
    pub fits: bool,
    /// The edit that imports it, when the file doesn't yet.
    pub import: Option<(u32, u32, String)>,
    /// For an enum's variant, the enum: a standard type's variant is shown
    /// with the name its entry gives people (`Locale.en_gb` is "British
    /// English"), which the editor works out with the runtime's code.
    pub variant_of: Option<String>,
}

impl Item {
    fn new(label: impl Into<String>, what: What) -> Item {
        Item {
            label: label.into(),
            what,
            detail: None,
            docs: None,
            insert: None,
            snippet: false,
            fits: false,
            import: None,
            variant_of: None,
        }
    }

    fn detail(mut self, detail: impl Into<String>) -> Item {
        self.detail = Some(detail.into());
        self
    }

    fn docs(mut self, docs: Option<String>) -> Item {
        self.docs = docs;
        self
    }
}

/// What an item is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum What {
    Argument,
    Parameter,
    Local,
    Field,
    Variant,
    Unit,
    Rule,
    Setting,
    Message,
    Constant,
    Function,
    Action,
    Operation,
    Library,
    Domain,
    Namespace,
    Type,
    Keyword,
}

/// What the editor offers at byte `offset` of `file`, as it is now; `None`
/// where nothing is offered at all: inside a comment or a string.
pub fn complete(db: &dyn Db, project: Project, file: File, offset: u32) -> Option<Completions> {
    let source = tree::source(db, file)?;
    let text = file.text(db);
    if offset as usize > text.len() || !text.is_char_boundary(offset as usize) {
        return None;
    }
    let view = callable_at(db, file, &source, offset, true)
        .map(|callable| Arc::clone(&typed_body(db, project, callable).view));
    let place = Place {
        db,
        project,
        file,
        text,
        root: source.syntax().clone(),
        lookup: Lookup::for_file(db, project, file),
        std: StdNames::get(),
        offset,
        view,
        ownership: jadpo_db::location(db, project, file).ownership.clone(),
    };
    place.complete()
}

/// The place being completed, and what reads it.
struct Place<'a> {
    db: &'a dyn Db,
    project: Project,
    file: File,
    text: &'a str,
    root: SyntaxNode,
    lookup: Lookup<'a>,
    std: &'static StdNames,
    offset: u32,
    /// The checked body around the place, if it is in one.
    view: Option<Arc<BodyView>>,
    ownership: Ownership,
}

/// Where the place is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Where {
    /// Inside a body, where a statement or an expression goes.
    Body,
    /// Between declarations.
    Top,
    /// A declaration's header or members: nothing is offered.
    Other,
}

fn start(token: &SyntaxToken) -> u32 {
    token.text_range().start().into()
}

fn end(token: &SyntaxToken) -> u32 {
    token.text_range().end().into()
}

/// A syntax node as the checker's facts key it: its range and its kind.
fn key(node: &SyntaxNode) -> (u32, u32, u16) {
    let range = node.text_range();
    (range.start().into(), range.end().into(), node.kind() as u16)
}

/// Whether a token is a word: a name or a keyword being typed.
fn is_word(token: &SyntaxToken) -> bool {
    let text = token.text();
    text.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A type as a stub writes it: `Dst?`, `List<T>`.
fn written(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Path(names, args) if args.is_empty() => names.join("."),
        TypeRef::Path(names, args) => format!(
            "{}<{}>",
            names.join("."),
            args.iter().map(written).collect::<Vec<_>>().join(", ")
        ),
        TypeRef::Nullable(inner) => format!("{}?", written(inner)),
        TypeRef::Invalid => "?".to_string(),
    }
}

/// A parameter of something being called, as completion needs it.
struct Param {
    name: String,
    /// Its type written out, for the detail.
    shown: String,
    ty: Ty,
    /// A call may leave it out.
    optional: bool,
}

impl Place<'_> {
    fn complete(&self) -> Option<Completions> {
        let left = self.token_left(self.offset);
        if let Some(token) = &left
            && self.inside_text(token)
        {
            return None;
        }
        if let Some(token) = &left
            && matches!(token.kind(), INT_NUMBER | DECIMAL_NUMBER | UNIT_NUMBER)
            && end(token) >= self.offset
            && start(token) < self.offset
        {
            return Some(self.units(token));
        }
        if let Some(found) = self.decline_rules() {
            return Some(found);
        }
        let word = left.filter(|token| {
            is_word(token) && start(token) < self.offset && self.offset <= end(token)
        });
        let from = word.as_ref().map_or(self.offset, start);
        let mut items = self.items(from, word.as_ref());
        // One item per label: the first, which is the nearest local.
        let mut seen = BTreeSet::new();
        items.retain(|item| seen.insert(item.label.clone()));
        Some(Completions {
            start: from,
            end: self.offset,
            items,
        })
    }

    // --- reading the tree ---------------------------------------------------

    /// The token ending at or covering `at`, preferring the one before it.
    fn token_left(&self, at: u32) -> Option<SyntaxToken> {
        match self.root.token_at_offset(at.into()) {
            TokenAtOffset::None => None,
            TokenAtOffset::Single(token) => Some(token),
            TokenAtOffset::Between(left, _) => Some(left),
        }
    }

    /// The last token before `at` that isn't whitespace or a comment.
    fn previous(&self, at: u32) -> Option<SyntaxToken> {
        let mut token = self.token_left(at)?;
        loop {
            if end(&token) <= at && !matches!(token.kind(), WHITESPACE | COMMENT | DOC_COMMENT) {
                return Some(token);
            }
            token = token.prev_token()?;
        }
    }

    /// Whether the cursor is inside a comment or a string.
    fn inside_text(&self, token: &SyntaxToken) -> bool {
        match token.kind() {
            COMMENT | DOC_COMMENT => start(token) < self.offset,
            STRING => {
                let text = token.text();
                let closed = text.len() >= 2 && text.ends_with('"');
                start(token) < self.offset && (self.offset < end(token) || !closed)
            }
            _ => false,
        }
    }

    /// Whether only whitespace stands between the line's start and `at`.
    fn at_line_start(&self, at: u32) -> bool {
        let before = &self.text[..at as usize];
        let line = before.rfind('\n').map_or(0, |i| i + 1);
        before[line..].trim().is_empty()
    }

    /// Where the place at `token` is.
    fn where_at(&self, token: &SyntaxToken) -> Where {
        for node in token.parent_ancestors() {
            match node.kind() {
                BLOCK => return Where::Body,
                MEMBER_LIST if node.parent().is_some_and(|p| p.kind() == RECORD_EXPR) => {}
                PARAM_LIST | RET_TYPE | FAILS_CLAUSE | MEMBER_LIST | USE_PATH | USE_NAMES
                | DECLINE => return Where::Other,
                CONSTANT_DECL => return Where::Body,
                SOURCE_FILE => return Where::Top,
                kind if is_declaration(kind) => return Where::Other,
                _ => {}
            }
        }
        Where::Top
    }

    /// The nearest brackets around `token` that decide what goes at a
    /// place inside them: a call's arguments, a record's members, a block.
    fn enclosing(token: &SyntaxToken) -> Option<SyntaxNode> {
        token.parent_ancestors().find(|node| {
            matches!(
                node.kind(),
                ARG_LIST | MEMBER_LIST | BLOCK | LIST_EXPR | PARAM_LIST
            )
        })
    }

    // --- what goes where ----------------------------------------------------

    fn items(&self, from: u32, word: Option<&SyntaxToken>) -> Vec<Item> {
        let prev = self.previous(from);
        if let Some(dot) = &prev
            && dot.kind() == DOT
            && end(dot) == from
        {
            return self.after_dot(dot);
        }
        if self.in_type(word, prev.as_ref()) {
            return self.type_names();
        }
        let anchor = match word {
            Some(word) => Some(word.clone()),
            None => self.token_left(from),
        };
        let Some(anchor) = anchor else {
            return self.declaration_keywords();
        };
        let enclosing = Self::enclosing(&anchor);
        let starts_entry = prev
            .as_ref()
            .is_some_and(|p| matches!(p.kind(), L_PAREN | L_BRACE | COMMA) || p.text() == ",")
            || self.at_line_start(from);
        if let Some(list) = &enclosing
            && list.kind() == MEMBER_LIST
            && let Some(record) = list.parent().and_then(ast::RecordExpr::cast)
            && starts_entry
        {
            return self.record_fields(&record, list);
        }
        let mut items = Vec::new();
        let call = enclosing
            .as_ref()
            .filter(|list| list.kind() == ARG_LIST)
            .and_then(|list| list.parent())
            .and_then(ast::CallExpr::cast);
        if let Some(call) = &call
            && starts_entry
        {
            items.extend(self.named_arguments(call, from));
        }
        let expected = self.expected_here(word, prev.as_ref(), call.as_ref(), from);
        match self.where_at(&anchor) {
            Where::Body => {
                if self.at_line_start(from) && call.is_none() {
                    items.extend(keywords(jadpo_syntax::STATEMENT_KEYWORDS));
                }
                items.extend(self.names(expected.as_ref(), from));
                items.extend(keywords(jadpo_syntax::EXPRESSION_KEYWORDS));
            }
            Where::Top if self.at_line_start(from) => items.extend(self.declaration_keywords()),
            Where::Top | Where::Other => {}
        }
        items
    }

    /// The words a declaration starts with: the reserved ones, those that
    /// start one only where one is written, and `use`, `decline` and
    /// `public`.
    fn declaration_keywords(&self) -> Vec<Item> {
        let mut items = keywords(jadpo_syntax::DECLARATION_KEYWORDS);
        items.extend(
            jadpo_syntax::CONTEXTUAL_DECLARATIONS
                .iter()
                .map(|word| Item::new(*word, What::Keyword)),
        );
        items.extend(keywords(&[USE_KW, DECLINE_KW, PUBLIC_KW]));
        items
    }

    /// Whether the place is where a type is written: a parameter's,
    /// variable's or field's type, a result, a type's base, a type
    /// argument, a `fails` list.
    fn in_type(&self, word: Option<&SyntaxToken>, prev: Option<&SyntaxToken>) -> bool {
        if let Some(word) = word {
            for node in word.parent_ancestors().take(4) {
                match node.kind() {
                    PATH_TYPE | NULLABLE_TYPE | GENERIC_ARG_LIST | FAILS_CLAUSE => return true,
                    MEMBER => return self.member_holds_type(&node),
                    _ => {}
                }
            }
            return false;
        }
        let Some(prev) = prev else { return false };
        let Some(parent) = prev.parent() else {
            return false;
        };
        match (prev.kind(), parent.kind()) {
            (COLON, PARAM | VAR_STMT) => true,
            (THIN_ARROW, RET_TYPE) => true,
            (EQ, TYPE_DECL) => true,
            (L_ANGLE | COMMA, GENERIC_ARG_LIST) => true,
            (FAILS_KW | COMMA, FAILS_CLAUSE) => true,
            (COLON, MEMBER) => self.member_holds_type(&parent),
            _ => false,
        }
    }

    /// Whether a member's value is a type: a field of an object, input,
    /// output, event, failure or entity, or a setting in `config { }`.
    fn member_holds_type(&self, member: &SyntaxNode) -> bool {
        let Some(holder) = member.parent().and_then(|list| list.parent()) else {
            return false;
        };
        match holder.kind() {
            ENTITY_DECL => true,
            SINGLETON_DECL => holder
                .first_token()
                .is_some_and(|token| token.text() == crate::config::CONFIG),
            TYPE_DECL => ast::TypeDecl::cast(holder)
                .and_then(|decl| decl.base())
                .is_some_and(|base| match TypeRef::from_type(&base) {
                    TypeRef::Path(names, _) => names
                        .first()
                        .is_some_and(|name| BASES.contains(&name.as_str()) && name != "Enum"),
                    _ => false,
                }),
            _ => false,
        }
    }

    /// The type the place requires, when the check says or the place
    /// shows it: what the expression being typed is checked against, a
    /// named or positional argument's parameter, a record field, a
    /// variable's declared type, the callable's result.
    fn expected_here(
        &self,
        word: Option<&SyntaxToken>,
        prev: Option<&SyntaxToken>,
        call: Option<&ast::CallExpr>,
        from: u32,
    ) -> Option<Ty> {
        if let Some(word) = word
            && let Some(name) = word.parent()
            && let Some(ty) = self.expected_of(&name)
        {
            return Some(ty);
        }
        let prev = prev?;
        let parent = prev.parent()?;
        match (prev.kind(), parent.kind()) {
            (COLON, ARG) => {
                let arg = ast::Arg::cast(parent)?;
                let name = tree::name_text(&arg.name()?)?;
                let call = arg
                    .syntax()
                    .parent()?
                    .parent()
                    .and_then(ast::CallExpr::cast)?;
                let (params, _) = self.callee_params(&call)?;
                return params.into_iter().find(|p| p.name == name).map(|p| p.ty);
            }
            (COLON, MEMBER) => {
                let member = ast::Member::cast(parent)?;
                let field = tree::name_text(&member.member_key()?.names().next()?)?;
                let record = member
                    .syntax()
                    .parent()?
                    .parent()
                    .and_then(ast::RecordExpr::cast)?;
                return self
                    .record_field_defs(&record)
                    .into_iter()
                    .find(|(name, _, _)| *name == field)
                    .map(|(_, ty, _)| ty);
            }
            (EQ, VAR_STMT) => {
                let ty = ast::VarStmt::cast(parent)?.ty()?;
                return Some(self.lookup.resolve(&TypeRef::from_type(&ty)));
            }
            (RETURN_KW, _) => {
                let decl = parent
                    .ancestors()
                    .find(|node| is_declaration(node.kind()))?;
                let ty = decl
                    .children()
                    .find_map(ast::RetType::cast)
                    .and_then(|ret| ret.ty())?;
                return Some(self.lookup.resolve(&TypeRef::from_type(&ty)));
            }
            _ => {}
        }
        // A positional argument: the parameter in its place.
        let call = call?;
        let index = call
            .arg_list()?
            .args()
            .take_while(|arg| u32::from(arg.syntax().text_range().end()) < from)
            .count();
        let (params, _) = self.callee_params(call)?;
        params.into_iter().nth(index).map(|p| p.ty)
    }

    /// What the check required of `node`, if it is an expression it
    /// checked against a type.
    fn expected_of(&self, node: &SyntaxNode) -> Option<Ty> {
        let ty = self.view.as_ref()?.expected.get(&key(node))?;
        (!ty.has_error()).then(|| ty.clone())
    }

    /// Whether `ty` goes where `expected` is required.
    fn fits(&self, ty: &Ty, expected: Option<&Ty>) -> bool {
        let Some(expected) = expected else {
            return false;
        };
        !ty.has_error()
            && !expected.has_error()
            && !matches!(ty, Ty::Var(_) | Ty::Unknown(_))
            && self.lookup.assignable(ty, expected)
    }

    // --- locals -------------------------------------------------------------

    /// The locals in scope at `at`, nearest first.
    fn locals(&self, at: u32) -> Vec<&ViewLocal> {
        let Some(view) = &self.view else {
            return Vec::new();
        };
        let mut found: Vec<&ViewLocal> = view
            .locals
            .iter()
            .filter(|local| {
                local.end <= at && self.in_scope(local, at) && !self.declaring(local, at)
            })
            .collect();
        found.reverse();
        found
    }

    /// Whether `local` is in scope at `at`: inside the block, arm, loop or
    /// inline function it was declared in, or anywhere in its callable for
    /// a parameter.
    fn in_scope(&self, local: &ViewLocal, at: u32) -> bool {
        let token = match self.root.token_at_offset(local.start.into()) {
            TokenAtOffset::Single(token) | TokenAtOffset::Between(_, token) => token,
            TokenAtOffset::None => return false,
        };
        for node in token.parent_ancestors() {
            let scope = match node.kind() {
                PARAM_LIST => match node.parent() {
                    Some(decl) => decl,
                    None => return false,
                },
                BLOCK | FOR_STMT | MATCH_ARM | LAMBDA_EXPR => node,
                _ => continue,
            };
            let range = scope.text_range();
            let end = u32::from(range.end());
            let closed = self.text[..end as usize].ends_with('}');
            return u32::from(range.start()) <= at && (at < end || (at == end && !closed));
        }
        local.kind == LocalKind::Param
    }

    /// Whether `at` is in the value of the `var` that declares `local`: a
    /// variable isn't read in its own value.
    fn declaring(&self, local: &ViewLocal, at: u32) -> bool {
        let Some(token) = self.root.token_at_offset(local.start.into()).right_biased() else {
            return false;
        };
        let Some(statement) = token
            .parent()
            .and_then(|name| name.parent())
            .and_then(ast::VarStmt::cast)
        else {
            return false;
        };
        let range = statement.syntax().text_range();
        let end = u32::from(range.end());
        // With no value yet, the statement ends at its `=`.
        let reaches = at <= end
            || (statement.expr().is_none()
                && self.text[end as usize..at as usize].trim().is_empty());
        u32::from(range.start()) <= at && reaches
    }

    fn is_local(&self, name: &str) -> bool {
        self.locals(self.offset)
            .iter()
            .any(|local| local.name == name)
    }

    /// The type of an expression, as the check worked it out, or as its
    /// parts give it where the check stopped short of it.
    fn type_of(&self, expr: &Expr) -> Option<Ty> {
        if let Some(view) = &self.view
            && let Some(ty) = view.types.get(&key(expr.syntax()))
            && !ty.has_error()
        {
            return Some(ty.clone());
        }
        match expr {
            Expr::NameRef(name) => {
                let name = tree::name_ref_text(name)?;
                self.locals(self.offset)
                    .into_iter()
                    .find(|local| local.name == name)
                    .map(|local| local.ty.clone())
            }
            Expr::FieldExpr(field) => {
                let receiver = self.type_of(&field.expr()?)?;
                let name = tree::name_ref_text(&field.name_ref()?)?;
                members_of(&self.lookup, &self.lookup.identity(), &receiver)
                    .into_iter()
                    .find(|member| member.name == name)
                    .map(|member| member.ty)
            }
            Expr::ParenExpr(paren) => self.type_of(&paren.expr()?),
            _ => None,
        }
    }

    // --- names in scope -----------------------------------------------------

    /// Every name an expression at `at` may start with: its locals, the
    /// project's names the file may use (with the import each needs), the
    /// domains it may call into, the namespaces, and, where an enum is
    /// required, that enum's variants.
    fn names(&self, expected: Option<&Ty>, at: u32) -> Vec<Item> {
        let mut items = Vec::new();
        for local in self.locals(at) {
            let what = if local.kind == LocalKind::Param {
                What::Parameter
            } else {
                What::Local
            };
            let mut item = Item::new(&local.name, what).detail(local.ty.to_string());
            item.fits = self.fits(&local.ty, expected);
            items.push(item);
        }
        let path = self.file.path(self.db);
        let mut offered: Vec<(&Symbol, Item)> = Vec::new();
        // Reading the whole table, as suggestions do (`NameView::all`).
        for symbols in self.lookup.all().declarations.values() {
            let Some(symbol) = symbols.iter().find(|symbol| {
                symbol.library.is_none() && symbol.provided.is_none() && !symbol.test
            }) else {
                continue;
            };
            let item = match symbol.kind {
                ItemKind::Type(_) | ItemKind::Entity => {
                    let mut item = self.type_item(symbol);
                    item.fits = expected.is_some_and(
                        |ty| matches!(ty.strip_nullable(), Ty::Named(n) if *n == symbol.canonical),
                    );
                    item
                }
                ItemKind::Function | ItemKind::Action | ItemKind::Query | ItemKind::Fixture
                    if symbol.ownership == self.ownership =>
                {
                    let Some(sig) = self.lookup.signature(&symbol.name) else {
                        continue;
                    };
                    let mut item = self.callable_item(&symbol.name, symbol, sig);
                    item.fits = self.fits(&sig.result, expected);
                    item
                }
                ItemKind::Constant if symbol.path == *path => {
                    let mut item =
                        Item::new(&symbol.name, What::Constant).docs(self.docs_of(symbol));
                    if let Some(ty) = self.lookup.constant_type(None, &symbol.name) {
                        item.fits = self.fits(ty, expected);
                        item = item.detail(ty.to_string());
                    }
                    item
                }
                ItemKind::Library => Item::new(&symbol.name, What::Library)
                    .detail("library")
                    .docs(self.docs_of(symbol)),
                _ => continue,
            };
            offered.push((symbol, item));
        }
        let symbols: Vec<&Symbol> = offered.iter().map(|(symbol, _)| *symbol).collect();
        let imports = imports_for(self.db, self.project, self.file, &symbols);
        items.extend(with_imports(
            offered.into_iter().map(|(_, item)| item),
            imports,
        ));
        let domains: Vec<&str> = self
            .lookup
            .all()
            .domains
            .iter()
            .map(String::as_str)
            .collect();
        let imports = domain_imports_for(self.db, self.project, self.file, &domains);
        let offered = domains
            .iter()
            .map(|domain| Item::new(*domain, What::Domain).detail("domain"));
        items.extend(with_imports(offered, imports));
        for (namespace, kind) in self.std.namespaces() {
            if kind == Namespace::Generated && self.members_of_generated(namespace).is_empty() {
                continue;
            }
            items.push(Item::new(namespace, What::Namespace).detail("standard library"));
        }
        if let Some(Ty::Named(name)) = expected.map(Ty::strip_nullable)
            && let Some(variants) = self.variants(name)
        {
            let shown = crate::types::shown(name);
            for variant in variants {
                let mut item = Item::new(format!("{shown}.{variant}"), What::Variant);
                item.fits = true;
                item.variant_of = Some(shown.to_string());
                items.push(item);
            }
        }
        items
    }

    /// A project type, with its kind and description.
    fn type_item(&self, symbol: &Symbol) -> Item {
        Item::new(&symbol.name, What::Type)
            .detail(symbol.kind.word())
            .docs(self.docs_of(symbol))
    }

    /// A function, action, query or fixture: its signature, its
    /// description and a call with its parameters.
    fn callable_item(&self, name: &str, symbol: &Symbol, sig: &Sig) -> Item {
        let params: Vec<String> = sig
            .params
            .iter()
            .map(|(param, ty)| format!("{param}: {ty}"))
            .collect();
        let what = match symbol.kind {
            ItemKind::Function => What::Function,
            _ => What::Action,
        };
        let mut item = Item::new(name, what)
            .detail(format!(
                "{} {name}({}) -> {}",
                symbol.kind.word(),
                params.join(", "),
                sig.result
            ))
            .docs(self.docs_of(symbol));
        let params: Vec<(String, bool)> = sig
            .params
            .iter()
            .map(|(param, _)| (param.clone(), false))
            .collect();
        self.call_snippet(&mut item, name, &params, false);
        item
    }

    /// A call's text with a placeholder for each required argument: the
    /// first in its place and the rest by name for a standard-library
    /// operation, every one in its place otherwise. Nothing when a `(`
    /// already follows.
    fn call_snippet(&self, item: &mut Item, name: &str, params: &[(String, bool)], named: bool) {
        if self.text[self.offset as usize..].starts_with('(') {
            return;
        }
        let required: Vec<&String> = params
            .iter()
            .filter(|(_, optional)| !optional)
            .map(|(name, _)| name)
            .collect();
        if required.is_empty() {
            item.insert = Some(format!("{name}()"));
            return;
        }
        let parts: Vec<String> = required
            .iter()
            .enumerate()
            .map(|(index, param)| {
                let placeholder = format!("${{{}:{param}}}", index + 1);
                if named && index > 0 {
                    format!("{param}: {placeholder}")
                } else {
                    placeholder
                }
            })
            .collect();
        item.insert = Some(format!("{name}({})", parts.join(", ")));
        item.snippet = true;
    }

    /// The `///` description of a project declaration.
    fn docs_of(&self, symbol: &Symbol) -> Option<String> {
        let file = file_at(
            self.db,
            self.project,
            Name::new(self.db, symbol.path.clone()),
        )?;
        let source = tree::source(self.db, file)?;
        let decl = tree::decl_at(&source, symbol.index)?;
        let node = match (&decl, &symbol.library) {
            (ast::Decl::LibraryDecl(library), Some((_, index))) => tree::library_members(library)
                .get(*index as usize)?
                .syntax()
                .clone(),
            _ => decl.syntax().clone(),
        };
        crate::graph::text::doc(&node)
    }

    /// The enum `name`'s variants: a project's or a standard one's.
    fn variants(&self, name: &str) -> Option<Vec<String>> {
        if let Some(variants) = self.lookup.variants(name) {
            return Some(variants.iter().map(|v| v.name.clone()).collect());
        }
        self.std.enums().get(name).cloned()
    }

    fn members_of_generated(&self, namespace: &str) -> Vec<Item> {
        match namespace {
            crate::config::CONFIG => self.settings(None),
            crate::translations::TRANSLATIONS => self.translation_scopes(),
            _ => self
                .std
                .generated_members(namespace)
                .into_iter()
                .map(|member| {
                    Item::new(member, What::Operation).detail(format!("{namespace}.{member}"))
                })
                .collect(),
        }
    }

    // --- after a dot ----------------------------------------------------------

    fn after_dot(&self, dot: &SyntaxToken) -> Vec<Item> {
        let Some(parent) = dot.parent() else {
            return Vec::new();
        };
        match parent.kind() {
            FIELD_EXPR => {
                let Some(field) = ast::FieldExpr::cast(parent.clone()) else {
                    return Vec::new();
                };
                let Some(receiver) = field.expr() else {
                    return Vec::new();
                };
                let expected = self.expected_of(&parent);
                self.members(&receiver, expected.as_ref())
            }
            // `Owner.` written as a type: its fields, or an enum's variants.
            PATH => {
                let Some(first) = parent.children().find_map(ast::NameRef::cast) else {
                    return Vec::new();
                };
                let Some(host) = tree::name_ref_text(&first) else {
                    return Vec::new();
                };
                let canonical = self.lookup.canonical_name(&host);
                match self.lookup.fields(&canonical) {
                    Some(fields) => fields
                        .iter()
                        .map(|f| Item::new(&f.name, What::Field).detail(f.declared.to_string()))
                        .collect(),
                    None => self.type_members(&host),
                }
            }
            _ => Vec::new(),
        }
    }

    /// What follows `receiver.`: a namespace's, setting's, scope's,
    /// type's, library's or domain's members, or a value's fields.
    fn members(&self, receiver: &Expr, expected: Option<&Ty>) -> Vec<Item> {
        if let Some((first, rest)) = tree::dotted(receiver) {
            let head = tree::name_ref_text(&first).unwrap_or_default();
            let rest: Vec<String> = rest.iter().filter_map(tree::name_ref_text).collect();
            if !self.is_local(&head) {
                match rest.as_slice() {
                    [] => {
                        if head == crate::config::CONFIG {
                            return self.settings(expected);
                        }
                        if head == crate::translations::TRANSLATIONS {
                            return self.translation_scopes();
                        }
                        match self.std.namespace(&head) {
                            Some(Namespace::Standard) => return self.operations(&head, expected),
                            Some(Namespace::Generated) => return self.members_of_generated(&head),
                            None => {}
                        }
                        if starts_upper(&head) {
                            return self.type_members(&head);
                        }
                        if self.lookup.library(&head).is_some() {
                            return self.library_members(&head, expected);
                        }
                        if self.lookup.is_domain(&head) {
                            return self.domain_members(&head, expected);
                        }
                    }
                    [scope] if head == crate::translations::TRANSLATIONS => {
                        return self.messages(scope);
                    }
                    _ => {}
                }
            }
        }
        let Some(ty) = self.type_of(receiver) else {
            return Vec::new();
        };
        members_of(&self.lookup, &self.lookup.identity(), &ty)
            .into_iter()
            .map(|member| {
                let mut item =
                    Item::new(&member.name, What::Field).detail(member.declared.to_string());
                item.fits = self.fits(&member.ty, expected);
                item
            })
            .collect()
    }

    /// A standard namespace's operations, each with its signature as its
    /// stub declares it, its description, and the failures `jadpo-std`'s
    /// table says it can raise.
    fn operations(&self, namespace: &str, expected: Option<&Ty>) -> Vec<Item> {
        let mut items = Vec::new();
        for name in self.std.operations(namespace) {
            // A name known for a later area has nothing to run yet.
            let Some(operation) = self.std.operation(namespace, name) else {
                continue;
            };
            let params: Vec<String> = operation
                .params
                .iter()
                .map(|(param, ty)| format!("{param}: {}", written(ty)))
                .collect();
            let mut docs: Vec<String> = operation.docs.iter().cloned().collect();
            if let Some(entry) = jadpo_std::operation(namespace, name)
                && !entry.conditions.is_empty()
            {
                let failures: Vec<String> = entry
                    .conditions
                    .iter()
                    .map(|condition| match condition.option {
                        Some(option) => format!(
                            "`{}` unless `{option}:` says what to do",
                            condition.key.word()
                        ),
                        // What text or numbers read into a geometry is
                        // known only when the program runs (chunk 5.11).
                        None if condition.key.only_when_run() || namespace == "geo" => {
                            format!("`{}`", condition.key.word())
                        }
                        None => format!(
                            "`{}` unless its value is proved above 0",
                            condition.key.word()
                        ),
                    })
                    .collect();
                docs.push(format!("Can fail: {}.", failures.join("; ")));
            }
            let mut item = Item::new(name, What::Operation)
                .detail(format!(
                    "{namespace}.{name}({}) -> {}",
                    params.join(", "),
                    written(&operation.result)
                ))
                .docs((!docs.is_empty()).then(|| docs.join("\n\n")));
            item.fits = self.fits(&self.lookup.resolve_std(&operation.result), expected);
            let params: Vec<(String, bool)> = operation
                .params
                .iter()
                .map(|(param, ty)| (param.clone(), matches!(ty, TypeRef::Nullable(_))))
                .collect();
            self.call_snippet(&mut item, name, &params, true);
            items.push(item);
        }
        items
    }

    /// A type's members written after its name: an enum's variants.
    fn type_members(&self, written: &str) -> Vec<Item> {
        let canonical = self.lookup.canonical_name(written);
        let Some(variants) = self.variants(&canonical) else {
            return Vec::new();
        };
        variants
            .into_iter()
            .map(|variant| {
                let mut item = Item::new(variant, What::Variant).detail(written.to_string());
                item.variant_of = Some(written.to_string());
                item
            })
            .collect()
    }

    /// A library's functions and constants.
    fn library_members(&self, library: &str, expected: Option<&Ty>) -> Vec<Item> {
        let mut items = Vec::new();
        let table = crate::types::callables(self.db, self.project);
        for ((host, name), sig) in &table.library {
            if host != library {
                continue;
            }
            let Some(symbol) = self.lookup.library_function(library, name) else {
                continue;
            };
            let mut item = self.callable_item(name, symbol, sig);
            item.fits = self.fits(&sig.result, expected);
            items.push(item);
        }
        for ((host, name), ty) in &table.library_constants {
            if host != library {
                continue;
            }
            let mut item = Item::new(name, What::Constant).detail(ty.to_string());
            if let Some(symbol) = self.lookup.library_function(library, name) {
                item = item.docs(self.docs_of(symbol));
            }
            item.fits = self.fits(ty, expected);
            items.push(item);
        }
        items
    }

    /// The actions, queries and fixtures another domain lets this file
    /// call through it.
    fn domain_members(&self, domain: &str, expected: Option<&Ty>) -> Vec<Item> {
        let mut items = Vec::new();
        for symbols in self.lookup.all().declarations.values() {
            for symbol in symbols {
                if symbol.ownership.domain() != Some(domain)
                    || !matches!(
                        symbol.kind,
                        ItemKind::Action | ItemKind::Query | ItemKind::Fixture
                    )
                    || !symbol.visible_from(&self.ownership)
                {
                    continue;
                }
                let Some(sig) = self.lookup.signature(&symbol.canonical) else {
                    continue;
                };
                let mut item = self.callable_item(&symbol.name, symbol, sig);
                item.fits = self.fits(&sig.result, expected);
                items.push(item);
            }
        }
        items
    }

    /// `config.`: the project's settings.
    fn settings(&self, expected: Option<&Ty>) -> Vec<Item> {
        crate::config::config(self.db, self.project)
            .settings
            .iter()
            .map(|setting| {
                let detail = if setting.secret {
                    format!("{} (secret)", setting.ty)
                } else {
                    setting.ty.to_string()
                };
                let mut item = Item::new(&setting.name, What::Setting).detail(detail);
                item.docs = Some(format!("Read from `{}`.", setting.variable));
                item.fits = self.fits(&setting.ty, expected);
                item
            })
            .collect()
    }

    /// `translations.`: the scopes with a message code can call.
    fn translation_scopes(&self) -> Vec<Item> {
        crate::translations::scopes(self.db, self.project)
            .into_iter()
            .filter(|scope| {
                !crate::translations::callable_keys(self.db, self.project, scope).is_empty()
            })
            .map(|scope| Item::new(scope, What::Namespace).detail("translations"))
            .collect()
    }

    /// `translations.<scope>.`: the messages and worded enums code can
    /// call, each with `locale:` and its own parameters.
    fn messages(&self, scope: &str) -> Vec<Item> {
        let subjects = crate::translations::subjects(
            self.db,
            self.project,
            Name::new(self.db, scope.to_string()),
        );
        subjects
            .list
            .iter()
            .filter(|subject| subject.callable())
            .map(|subject| {
                let mut params = vec![("locale".to_string(), false)];
                params.extend(
                    subject
                        .params
                        .iter()
                        .map(|param| (param.name.clone(), false)),
                );
                let names: Vec<&str> = params.iter().map(|(name, _)| name.as_str()).collect();
                let mut item = Item::new(&subject.key, What::Message)
                    .detail(format!(
                        "translations.{scope}.{}({})",
                        subject.key,
                        names
                            .iter()
                            .map(|n| format!("{n}:"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                    .docs(subject.description.clone());
                self.call_snippet(&mut item, &subject.key, &params, true);
                // Every argument of a translation is named.
                if let Some(insert) = &item.insert
                    && item.snippet
                {
                    item.insert = Some(insert.replacen("(${1:locale}", "(locale: ${1:locale}", 1));
                }
                item
            })
            .collect()
    }

    // --- calls and records ----------------------------------------------------

    /// What `call` calls, as its parameters, and whether they may be named:
    /// a standard-library operation's from its stub, a translation's, or a
    /// function's, action's or library function's signature.
    fn callee_params(&self, call: &ast::CallExpr) -> Option<(Vec<Param>, bool)> {
        let callee = call.expr()?;
        let (first, rest) = tree::dotted(&callee)?;
        let head = tree::name_ref_text(&first)?;
        let rest: Vec<String> = rest.iter().filter_map(tree::name_ref_text).collect();
        let from_sig = |sig: &Sig| -> Vec<Param> {
            sig.params
                .iter()
                .map(|(name, ty)| Param {
                    name: name.clone(),
                    shown: ty.to_string(),
                    ty: ty.clone(),
                    optional: false,
                })
                .collect()
        };
        if self.is_local(&head) {
            return None;
        }
        match rest.as_slice() {
            [] => Some((from_sig(self.lookup.signature(&head)?), false)),
            [name] if self.std.namespace(&head) == Some(Namespace::Standard) => {
                let operation = self.std.operation(&head, name)?;
                let params = operation
                    .params
                    .iter()
                    .map(|(param, ty)| Param {
                        name: param.clone(),
                        shown: written(ty),
                        ty: self.lookup.resolve_std(ty),
                        optional: matches!(ty, TypeRef::Nullable(_)),
                    })
                    .collect();
                Some((params, true))
            }
            [name] if self.lookup.library(&head).is_some() => {
                Some((from_sig(self.lookup.library_signature(&head, name)?), false))
            }
            [name] if self.lookup.is_domain(&head) => {
                Some((from_sig(self.lookup.signature(name)?), false))
            }
            [scope, key] if head == crate::translations::TRANSLATIONS => {
                let subjects = crate::translations::subjects(
                    self.db,
                    self.project,
                    Name::new(self.db, scope.clone()),
                );
                let subject = subjects.get(key)?;
                let mut params = vec![Param {
                    name: "locale".to_string(),
                    shown: jadpo_std::LOCALE.to_string(),
                    ty: Ty::Named(jadpo_std::LOCALE.to_string()),
                    optional: false,
                }];
                params.extend(subject.params.iter().map(|param| {
                    Param {
                        name: param.name.clone(),
                        shown: param
                            .enum_type
                            .clone()
                            .unwrap_or_else(|| format!("{:?}", param.kind)),
                        ty: param.enum_type.clone().map_or(Ty::Error, Ty::Named),
                        optional: false,
                    }
                }));
                Some((params, true))
            }
            _ => None,
        }
    }

    /// The named arguments of `call` not given yet, the required first.
    fn named_arguments(&self, call: &ast::CallExpr, from: u32) -> Vec<Item> {
        let Some((params, true)) = self.callee_params(call) else {
            return Vec::new();
        };
        let mut given = vec![false; params.len()];
        if let Some(list) = call.arg_list() {
            for (index, arg) in list.args().enumerate() {
                let range = arg.syntax().text_range();
                // The argument being typed fills nothing yet.
                if u32::from(range.start()) <= from && from <= u32::from(range.end()) {
                    continue;
                }
                match arg.name().as_ref().and_then(tree::name_text) {
                    Some(name) => {
                        if let Some(at) = params.iter().position(|p| p.name == name) {
                            given[at] = true;
                        }
                    }
                    None => {
                        if let Some(slot) = given.get_mut(index) {
                            *slot = true;
                        }
                    }
                }
            }
        }
        let mut items: Vec<Item> = params
            .iter()
            .zip(given)
            .filter(|(_, given)| !given)
            .map(|(param, _)| {
                let mut item = Item::new(&param.name, What::Argument).detail(param.shown.clone());
                item.insert = Some(format!("{}: ", param.name));
                item.fits = !param.optional;
                item
            })
            .collect();
        items.sort_by_key(|item| !item.fits);
        items
    }

    /// The fields a record being built has, with their types and whether
    /// it may leave each out.
    fn record_field_defs(&self, record: &ast::RecordExpr) -> Vec<(String, Ty, bool)> {
        let Some(head) = record.expr() else {
            return Vec::new();
        };
        let Some((first, rest)) = tree::dotted(&head) else {
            return Vec::new();
        };
        let Some(host) = tree::name_ref_text(&first) else {
            return Vec::new();
        };
        let canonical = self.lookup.canonical_name(&host);
        let fields = match rest.as_slice() {
            [] => self.lookup.fields(&canonical).map(<[_]>::to_vec),
            [variant] => {
                let variant = tree::name_ref_text(variant).unwrap_or_default();
                self.lookup
                    .variants(&canonical)
                    .and_then(|variants| variants.iter().find(|v| v.name == variant))
                    .and_then(|v| v.fields.clone())
            }
            _ => None,
        };
        fields
            .unwrap_or_default()
            .into_iter()
            .filter(|f| !f.generated)
            .map(|f| (f.name, f.declared, f.optional || f.default.is_some()))
            .collect()
    }

    /// The fields of a record being built that aren't given yet, the
    /// required first.
    fn record_fields(&self, record: &ast::RecordExpr, list: &SyntaxNode) -> Vec<Item> {
        let given: BTreeSet<String> = tree::members(ast::MemberList::cast(list.clone()))
            .into_iter()
            .filter(|(_, member)| member.colon_token().is_some())
            .filter_map(|(name, _)| tree::name_text(&name))
            .collect();
        let mut items: Vec<Item> = self
            .record_field_defs(record)
            .into_iter()
            .filter(|(name, _, _)| !given.contains(name))
            .map(|(name, ty, optional)| {
                let mut item = Item::new(&name, What::Field).detail(ty.to_string());
                item.insert = Some(format!("{name}: "));
                item.fits = !optional;
                item
            })
            .collect();
        items.sort_by_key(|item| !item.fits);
        items
    }

    // --- types ----------------------------------------------------------------

    /// Every type a file may write: the project's (with the import each
    /// needs) and the language's own.
    fn type_names(&self) -> Vec<Item> {
        let symbols: Vec<&Symbol> = self
            .lookup
            .all()
            .declarations
            .values()
            .filter_map(|symbols| {
                symbols.iter().find(|symbol| {
                    symbol.library.is_none()
                        && symbol.provided.is_none()
                        && matches!(symbol.kind, ItemKind::Type(_) | ItemKind::Entity)
                })
            })
            .collect();
        let imports = imports_for(self.db, self.project, self.file, &symbols);
        let offered = symbols.iter().map(|symbol| self.type_item(symbol));
        let mut items: Vec<Item> = with_imports(offered, imports).collect();
        let mut standard: BTreeSet<&str> = self.std.types().collect();
        standard.extend(self.std.standard().keys().map(String::as_str));
        for name in standard {
            items.push(Item::new(name, What::Type).detail("standard type"));
        }
        items
    }

    // --- units ------------------------------------------------------------------

    /// The units a number may be written with where it stands: those of
    /// the type it goes into, or every unit where nothing says which.
    fn units(&self, number: &SyntaxToken) -> Completions {
        let text = number.text();
        let digits = text
            .find(|c: char| c.is_ascii_alphabetic())
            .unwrap_or(text.len()) as u32;
        let from = start(number) + digits;
        if self.offset < from {
            return Completions {
                start: self.offset,
                end: self.offset,
                items: Vec::new(),
            };
        }
        let expected = self.number_expected(number);
        let all = jadpo_std::unit_suffixes();
        let units: Vec<jadpo_std::UnitSuffix> = match expected.as_ref().map(Ty::strip_nullable) {
            None => all,
            Some(ty) if ty.has_error() || matches!(ty, Ty::Var(_) | Ty::Unknown(_)) => all,
            Some(ty) => match self.lookup.base_prim(ty) {
                Some(prim) => all.into_iter().filter(|unit| unit.of == prim).collect(),
                None => Vec::new(),
            },
        };
        let items = units
            .into_iter()
            .map(|unit| {
                let mut item = Item::new(unit.suffix, What::Unit).detail(format!(
                    "{} in {}",
                    unit.of.name(),
                    unit.name
                ));
                item.fits = true;
                item
            })
            .collect();
        Completions {
            start: from,
            end: self.offset,
            items,
        }
    }

    /// The type a number goes into: what the check required of it, or, in
    /// a type's declaration, the type its rules are written in (`min:
    /// 30m` on a `Duration`).
    fn number_expected(&self, number: &SyntaxToken) -> Option<Ty> {
        let literal = number.parent()?;
        let negated = literal
            .parent()
            .filter(|parent| parent.kind() == PREFIX_EXPR);
        for node in [Some(literal.clone()), negated].into_iter().flatten() {
            if let Some(ty) = self.expected_of(&node) {
                return Some(ty);
            }
        }
        let decl = literal.ancestors().find_map(ast::TypeDecl::cast)?;
        let base = decl.base()?;
        let ty = self.lookup.resolve(&TypeRef::from_type(&base));
        (!ty.has_error()).then_some(ty)
    }

    // --- declines -----------------------------------------------------------------

    /// After `decline` on a line of its own: every rule a decline may
    /// answer, from the rule catalogue.
    fn decline_rules(&self) -> Option<Completions> {
        let before = &self.text[..self.offset as usize];
        let line = &before[before.rfind('\n').map_or(0, |i| i + 1)..];
        let rest = line.trim_start().strip_prefix(DECLINE_KW.text()?)?;
        if !rest.starts_with([' ', '\t']) {
            return None;
        }
        let written = rest.trim_start();
        if !written
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.')
        {
            return None;
        }
        let items = jadpo_diag::CATALOGUE
            .iter()
            .filter(|rule| rule.declinable)
            .map(|rule| {
                Item::new(rule.id, What::Rule)
                    .detail(rule.summary)
                    .docs(Some(format!("[{}]({})", rule.id, rule.help())))
            })
            .collect();
        Some(Completions {
            start: self.offset - written.len() as u32,
            end: self.offset,
            items,
        })
    }
}

/// Each item with the import it needs, leaving out those the file may not
/// use.
fn with_imports(
    items: impl Iterator<Item = Item>,
    imports: Vec<Import>,
) -> impl Iterator<Item = Item> {
    items
        .zip(imports)
        .filter_map(|(mut item, import)| match import {
            Import::Refused => None,
            Import::Needs(edit) => {
                item.import = Some(edit);
                Some(item)
            }
            Import::Seen => Some(item),
        })
}

fn keywords(kinds: &[SyntaxKind]) -> Vec<Item> {
    kinds
        .iter()
        .filter_map(|kind| kind.text())
        .map(|word| Item::new(word, What::Keyword))
        .collect()
}

fn is_declaration(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        TYPE_DECL
            | FUNCTION_DECL
            | CONSTANT_DECL
            | ACTION_DECL
            | QUERY_DECL
            | FIXTURE_DECL
            | ENTITY_DECL
            | LIBRARY_DECL
            | ROUTES_DECL
            | SCHEDULE_DECL
            | SUBSCRIBER_DECL
            | TEST_DECL
            | SEED_DECL
            | PACKAGE_DECL
            | RESOURCE_DECL
            | PROVIDE_DECL
            | SINGLETON_DECL
    )
}
