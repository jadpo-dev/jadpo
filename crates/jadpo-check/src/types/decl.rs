//! Area 2's rules on declarations: each type's settings fit its base and
//! agree with each other, an object's lines build its fields as decided,
//! enums have variants, types are written with the right arguments and no
//! redundant `?`, bases appear only after `=`, and signatures use named
//! types.

use std::collections::BTreeMap;

use jadpo_db::{Db, File, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_syntax::SyntaxNode;
use jadpo_syntax::ast::{self, AstNode, Decl, Expr, MemberItem};

use super::literal;
use super::rules::{self, CONSTRAINTS, Const, FIELD_SETTINGS, Problem, Rules, Subject};
use super::shape::{self, FieldsBase, Line, ShapeKind};
use super::table::{Def, Found, LineKind, LineProblem, TypeView, derive};
use super::ty::{BASES, Prim, Ty, TypeNode, TypeRef, container_arity};
use crate::lookups::Lookup;
use crate::std_lib::StdNames;
use crate::tree::{self, node_span};

/// Area 2's problems with one file's declarations.
#[salsa::tracked(returns(ref))]
pub fn type_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let Some(source) = tree::source(db, file) else {
        return Vec::new();
    };
    let lookup = Lookup::for_file(db, project, file);
    let mut checker = Decls {
        path: file.path(db),
        table: &lookup,
        out: Vec::new(),
        headers: Vec::new(),
        failure: false,
    };
    for decl in source.decls() {
        checker.decl(&decl);
    }
    checker.out
}

/// Where a parameter or result type sits, for the signature rule.
#[derive(Clone, Copy)]
enum Role {
    Parameter,
    Result,
    /// A constant's type, named as a signature's is (area 3).
    Constant,
}

pub(super) struct Decls<'a> {
    pub path: &'a str,
    pub table: &'a dyn TypeView,
    pub out: Vec<Diagnostic>,
    /// The headers the `Input` being checked reads so far, lower case, and
    /// where (area 7, "One input").
    pub headers: Vec<(String, Span)>,
    /// Whether the fields being checked are a failure's (area 8: a caller
    /// is never put in one).
    pub failure: bool,
}

impl Decls<'_> {
    pub(super) fn span(&self, node: &SyntaxNode) -> Span {
        node_span(self.path, node)
    }

    fn decl(&mut self, decl: &Decl) {
        match decl {
            Decl::TypeDecl(d) => self.type_decl(d, decl),
            Decl::EntityDecl(d) => {
                if let Some(shape) = shape::shape(decl)
                    && let ShapeKind::Fields { base, lines } = &shape.kind
                {
                    self.lines(d.member_list(), *base, &shape.name, lines);
                }
            }
            Decl::FunctionDecl(d) => self.signature(d.param_list(), d.ret_type(), true),
            Decl::ActionDecl(d) => self.signature(d.param_list(), d.ret_type(), true),
            Decl::QueryDecl(d) => self.signature(d.param_list(), d.ret_type(), true),
            Decl::FixtureDecl(d) => self.signature(d.param_list(), d.ret_type(), false),
            Decl::ConstantDecl(d) => self.constant(d),
            Decl::LibraryDecl(d) => {
                for member in tree::library_members(d) {
                    match member {
                        Decl::FunctionDecl(function) => {
                            self.signature(function.param_list(), function.ret_type(), true);
                        }
                        Decl::ConstantDecl(constant) => self.constant(&constant),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    // --- signatures -------------------------------------------------------

    fn signature(
        &mut self,
        params: Option<ast::ParamList>,
        result: Option<ast::RetType>,
        named_only: bool,
    ) {
        for param in params.iter().flat_map(|list| list.params()) {
            if let Some(ty) = param.ty() {
                let node = TypeNode::Type(ty);
                if self.type_node(&node, false) && named_only {
                    self.primitive(&node, Role::Parameter, None);
                }
            }
        }
        if let Some(ty) = result.and_then(|ret| ret.ty()) {
            let node = TypeNode::Type(ty);
            if self.type_node(&node, false) && named_only {
                self.primitive(&node, Role::Result, None);
            }
        }
    }

    /// A constant's type follows the rule a signature's does (area 3).
    fn constant(&mut self, constant: &ast::ConstantDecl) {
        if let Some(ty) = constant.ty() {
            let node = TypeNode::Type(ty);
            if self.type_node(&node, false) {
                self.primitive(&node, Role::Constant, None);
            }
        }
    }

    /// Functions, actions and queries take and return named types, with
    /// collection elements checked too (area 2, question 11).
    fn primitive(&mut self, node: &TypeNode, role: Role, container: Option<&Ty>) {
        let ty = self.table.resolve(&node.to_ref());
        match ty.strip_nullable() {
            Ty::Prim(prim) if prim.is_representation() => {
                let word = match role {
                    Role::Parameter => "parameter",
                    Role::Result => "result",
                    Role::Constant => "constant",
                };
                let name = prim.name();
                let message = match container {
                    Some(container) => {
                        format!("This {word}'s `{container}` holds `{name}` directly")
                    }
                    None => format!("This {word} uses `{name}` directly"),
                };
                let span = self.span(node.syntax());
                self.out.push(
                    Diagnostic::new(&rule::TYPE_PRIMITIVE_SIGNATURE, message)
                        .at(span, "a built-in type")
                        .reason(match role {
                            Role::Constant => format!(
                                "A constant's type is named as a signature's is, so its value says what it means and carries its rules; `{name}` says neither."
                            ),
                            _ => format!(
                                "Functions, actions and queries take and return named types, so each value says what it means and carries its rules; `{name}` says neither."
                            ),
                        })
                        .next(format!("Declare a named type built on `{name}` and use it here")),
                );
            }
            Ty::List(_) | Ty::Set(_) | Ty::Map(..) => {
                let inner = node.nullable_inner().unwrap_or_else(|| node.clone());
                for arg in inner.args() {
                    self.primitive(&arg, role, Some(ty.strip_nullable()));
                }
            }
            _ => {}
        }
    }

    // --- type positions ---------------------------------------------------

    /// Checks a type as written: no base outside a declaration, the right
    /// number of type arguments, no `?` on a type already nullable, and a
    /// field type naming a real field. False when it reported a problem.
    pub fn type_node(&mut self, node: &TypeNode, base_allowed: bool) -> bool {
        let node = node.without_settings();
        match node.to_ref() {
            TypeRef::Invalid => true,
            TypeRef::Nullable(inner_ref) => {
                let Some(inner) = node.nullable_inner() else {
                    return true;
                };
                if !self.type_node(&inner, false) {
                    return false;
                }
                let inner_ty = self.table.resolve(&inner_ref);
                if !inner_ty.has_error() && self.table.is_nullable(&inner_ty) {
                    let span = self.span(node.syntax());
                    let end = span.end;
                    self.out.push(
                        Diagnostic::new(
                            &rule::TYPE_REDUNDANT_NULLABILITY,
                            format!("`{inner_ty}` already permits `none`"),
                        )
                        .at(span, "nullable twice")
                        .reason(format!(
                            "`{inner_ty}` takes `none` from its declared type; a second `?` adds nothing."
                        ))
                        .next("Remove the extra `?`")
                        .edit(Span::new(self.path, end - 1, end), ""),
                    );
                    return false;
                }
                true
            }
            TypeRef::Path(names, args) => {
                let head = node.head();
                let first = names[0].as_str();
                if names.len() == 1 && BASES.contains(&first) && !base_allowed {
                    let span = self.span(&head);
                    self.out.push(
                        Diagnostic::new(
                            &rule::TYPE_BASE_OUTSIDE_DECLARATION,
                            format!("`{first}` is used as a type"),
                        )
                        .at(span, "only a base")
                        .reason(format!(
                            "`{first}` is written only after `=` in a `type` declaration; every shape has a name, so a field or signature names that type."
                        ))
                        .next(format!(
                            "Declare `type Name = {first} {{ ... }}` and use its name here"
                        )),
                    );
                    return false;
                }
                if names.len() == 1
                    && let Some(Def::Format { .. }) = self.table.def(first)
                {
                    let span = self.span(&head);
                    self.out
                        .push(super::formats::not_a_value(first, span, true));
                    return false;
                }
                if names.len() == 1 {
                    let arity = match container_arity(first) {
                        Some(arity) => Some(arity),
                        // `Range<T>` (area 19, "Ranges").
                        None if first == jadpo_std::RANGE => Some(1),
                        None if Prim::from_name(first).is_some()
                            || self.table.contains(first)
                            || BASES.contains(&first) =>
                        {
                            Some(0)
                        }
                        // A later area's type, or a name resolution reports.
                        None => None,
                    };
                    if let Some(arity) = arity
                        && arity != args.len()
                    {
                        let span = self.span(node.syntax());
                        let needs = match arity {
                            0 => "no type arguments".to_string(),
                            1 => format!("one type argument, as in `{first}<Tag>`"),
                            _ => format!("{arity} type arguments, as in `{first}<Key, Value>`"),
                        };
                        self.out.push(
                            Diagnostic::new(
                                &rule::TYPE_TYPE_ARGUMENT_COUNT,
                                format!("`{first}` takes {needs}"),
                            )
                            .at(span, format!("{} given", args.len()))
                            .reason(match arity {
                                0 => format!("`{first}` isn't generic: only `List`, `Set`, `Map` and `Range` take types in `< >`."),
                                _ => format!("`{first}` holds values of the type in `< >`, so it needs exactly {arity}."),
                            })
                            .next(format!("Write `{first}` with {needs}")),
                        );
                        return false;
                    }
                } else if self.table.contains(first)
                    && self
                        .table
                        .resolve_with(&TypeRef::Path(names.clone(), Vec::new()), false)
                        .is_error()
                {
                    return self.unknown_field_type(&names, &head);
                }
                let mut ok = true;
                for arg in node.args() {
                    ok &= self.type_node(&arg, false);
                }
                // A range's ends are a type `<` orders (area 19).
                if ok
                    && names.len() == 1
                    && first == jadpo_std::RANGE
                    && let [item] = args.as_slice()
                {
                    let item_ty = self.table.resolve(item);
                    let ordered = !self.table.is_nullable(&item_ty)
                        && self
                            .table
                            .representation(&item_ty)
                            .is_some_and(|prim| jadpo_std::RANGE_ITEMS.contains(&prim));
                    if !item_ty.has_error() && !ordered {
                        let span = self.span(node.syntax());
                        self.out.push(super::body::range_unordered(&item_ty, span));
                        return false;
                    }
                }
                ok
            }
        }
    }

    /// `Type.field` naming a field `Owner` doesn't have.
    fn unknown_field_type(&mut self, names: &[String], head: &SyntaxNode) -> bool {
        let host = &names[0];
        let (fields, word): (Vec<String>, &str) =
            match (self.table.fields(host), self.table.variants(host)) {
                (Some(fields), _) if names.len() == 2 => {
                    (fields.iter().map(|f| f.name.clone()).collect(), "field")
                }
                (_, Some(variants)) if names.len() == 3 => (
                    variants
                        .iter()
                        .find(|v| v.name == names[1])
                        .and_then(|v| v.fields.as_ref())
                        .map(|fields| fields.iter().map(|f| f.name.clone()).collect())
                        .unwrap_or_default(),
                    "field",
                ),
                _ => return true,
            };
        let missing = names.last().cloned().unwrap_or_default();
        let prefix = names[..names.len() - 1].join(".");
        let near = nearest(&missing, fields.iter().map(String::as_str)).map(str::to_string);
        let span = self.span(head);
        let mut problem = Diagnostic::new(
            &rule::TYPE_UNKNOWN_FIELD,
            format!("`{prefix}` has no {word} `{missing}`"),
        )
        .at(span.clone(), "not declared")
        .reason(format!(
            "A field type is written `Type.field`, naming a field the type declares.{}",
            did_you_mean(near.as_deref())
        ));
        problem = match near {
            Some(near) => problem
                .next(format!("Use `{prefix}.{near}`"))
                .edit(span, format!("{prefix}.{near}")),
            None => problem.next(format!("Name a field of `{prefix}`")),
        };
        self.out.push(problem);
        false
    }

    // --- type declarations ------------------------------------------------

    fn type_decl(&mut self, decl: &ast::TypeDecl, whole: &Decl) {
        let Some(shape) = shape::shape(whole) else {
            return;
        };
        if let Some(base) = decl.base()
            && !self.type_node(&TypeNode::Type(base), true)
        {
            return;
        }
        let list = decl.member_list();
        match &shape.kind {
            ShapeKind::Scalar { parent, .. } => {
                let parent = self.table.resolve(parent);
                if parent.has_error() {
                    return;
                }
                let subject = self.table.subject(&parent);
                let own = self.settings(list, subject, None, &parent);
                if self.table.base_prim(&parent) == Some(Prim::Decimal)
                    && own.scale.is_none()
                    && !self
                        .table
                        .rule_chain(&parent)
                        .iter()
                        .any(|(_, rules)| rules.scale.is_some())
                    && let Some(name) = decl.name()
                {
                    let span = self.span(name.syntax());
                    self.out.push(
                        Diagnostic::new(
                            &rule::TYPE_DECIMAL_SCALE_REQUIRED,
                            format!("`{}` needs a `scale`", shape.name),
                        )
                        .at(span, "a `Decimal` type")
                        .reason("A `Decimal` is exact with a fixed number of decimal places, its `scale`, which sets how it is stored and sent.")
                        .next("Add `scale:` with the number of decimal places, such as `scale: 2`"),
                    );
                }
            }
            ShapeKind::Fields {
                base: FieldsBase::Failure,
                ..
            } => {
                // A failure's fields sit in its `client:` and `internal:`
                // blocks, each an ordinary block of fields (area 4); the
                // rest of the failure is area 4's to check.
                for (key, section) in tree::members(list) {
                    if !matches!(
                        tree::name_text(&key).as_deref(),
                        Some("client" | "internal")
                    ) {
                        continue;
                    }
                    let block = section.block();
                    let lines = shape::lines(block.clone(), FieldsBase::Object);
                    self.failure = true;
                    self.lines(block, FieldsBase::Object, &shape.name, &lines);
                    self.failure = false;
                }
            }
            ShapeKind::Fields { base, lines } => {
                self.lines(list, *base, &shape.name, lines);
                if *base == FieldsBase::Object {
                    self.standard_exists(decl, &shape.name);
                }
            }
            ShapeKind::Format { .. } => self.format_decl(decl, &shape.name),
            ShapeKind::Enum { variants } => {
                if variants.is_empty() {
                    let span = decl
                        .name()
                        .map_or_else(|| self.span(decl.syntax()), |name| self.span(name.syntax()));
                    self.out.push(
                        Diagnostic::new(
                            &rule::TYPE_ENUM_EMPTY,
                            format!("`{}` has no variants", shape.name),
                        )
                        .at(span, "an empty enum")
                        .reason("An enum is a closed list of named alternatives, so it needs at least one for a value to be.")
                        .next("Add the enum's variants, one name per line"),
                    );
                }
                self.variants(list, &shape.name);
            }
        }
    }

    /// `type.standard_exists` (area 19, "Packages use the standard
    /// types"): an `Object` whose only fields are a standard type's, by its
    /// fields' common names in any order and what they are built on, is
    /// that standard type under another name. Only the distinctive shapes
    /// the data entries list match (question 5). A declaration named like
    /// a standard type is `type.standard_clash` instead, its one error.
    fn standard_exists(&mut self, decl: &ast::TypeDecl, name: &str) {
        if jadpo_std::standard_type(name).is_some() {
            return;
        }
        let Some(found) = standard_match(self.table, name, None) else {
            return;
        };
        let Some(written) = decl.name() else { return };
        let entry = found.standard;
        let meanings: Vec<String> = found
            .fields
            .iter()
            .map(|field| field.meaning.to_string())
            .collect();
        let named: Vec<String> = found
            .fields
            .iter()
            .map(|field| format!("{} (`{}`)", field.meaning, field.written))
            .collect();
        let mut built: Vec<String> = entry
            .shape
            .unwrap_or_default()
            .iter()
            .flat_map(|field| {
                field
                    .built_on
                    .iter()
                    .map(|prim| format!("`{}`", prim.name()))
            })
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        if entry
            .shape
            .unwrap_or_default()
            .iter()
            .any(|field| field.or_enum)
        {
            built.push("an enum".to_string());
        }
        let standard = entry.name;
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_STANDARD_EXISTS,
                format!("`{name}` is the standard `{standard}` under another name"),
            )
            .at(self.span(written.syntax()), crate::and_list(&meanings))
            .reason(format!(
                "`{name}` has only {}, each built on {}, which is the standard `{standard}`. Declarations of one shape under different names disagree on names and meaning, and every package and application has to convert between them; the standard type has one construction, one wire shape and one storage shape.",
                crate::and_list(&named),
                crate::or_list(&built)
            ))
            .next(format!(
                "Run `jadpo fix {name} swap` to use `{standard}` where `{name}` is used, then convert at the edge where data arrives in another shape"
            )),
        );
    }

    fn variants(&mut self, list: Option<ast::MemberList>, host: &str) {
        let mut seen: BTreeMap<String, Span> = BTreeMap::new();
        for item in list.iter().flat_map(|list| list.member_items()) {
            let (name, block) = match item {
                MemberItem::BareMember(variant) => (variant.name(), None),
                MemberItem::Member(variant) => (
                    variant.member_key().and_then(|key| key.names().next()),
                    variant.block(),
                ),
                _ => continue,
            };
            let Some(name) = name else { continue };
            let Some(text) = tree::name_text(&name) else {
                continue;
            };
            let span = self.span(name.syntax());
            if let Some(first) = seen.get(&text) {
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_DUPLICATE_VARIANT,
                        format!("`{host}.{text}` is declared twice"),
                    )
                    .at(span, "declared again")
                    .also(first.clone(), "first declared here")
                    .reason("Each variant of an enum has its own name, so a value and a `match` arm name exactly one.")
                    .next(format!("Remove one `{text}`, or rename it")),
                );
                continue;
            }
            seen.insert(text.clone(), span);
            if let Some(block) = block {
                let lines = shape::lines(Some(block.clone()), FieldsBase::Object);
                self.lines(
                    Some(block),
                    FieldsBase::Object,
                    &format!("{host}.{text}"),
                    &lines,
                );
            }
        }
    }

    /// An object's lines (area 2): its written fields and their settings,
    /// and the lines that copy fields from other types, read top to bottom.
    fn lines(
        &mut self,
        list: Option<ast::MemberList>,
        base: FieldsBase,
        host: &str,
        lines: &[Line],
    ) {
        let items: Vec<MemberItem> = list.iter().flat_map(|list| list.member_items()).collect();
        let member = |line: usize| match items.get(line) {
            Some(MemberItem::Member(member)) => Some(member.clone()),
            _ => None,
        };
        let area_two = base.is_area_two();
        self.headers.clear();
        for (index, line) in lines.iter().enumerate() {
            let Some(member) = member(index) else {
                continue;
            };
            match line {
                Line::Field { name, ty, .. } => {
                    if jadpo_syntax::RESERVED_NAMES.contains(&name.as_str())
                        && let Some(key) = member.member_key()
                    {
                        let span = self.span(key.syntax());
                        self.out.push(
                            Diagnostic::new(
                                &rule::SYNTAX_EXPECTED_NAME,
                                format!("`{name}` is a reserved word"),
                            )
                            .at(span, "can't be a name")
                            .reason("Control-flow and declaration words are reserved, so they can't name a declaration, field, parameter or variable.")
                            .next(format!("Choose another name than `{name}`")),
                        );
                    }
                    let Some(value) = member.value() else {
                        continue;
                    };
                    let node = TypeNode::Expr(value.clone());
                    if !self.type_node(&node, false) {
                        continue;
                    }
                    let declared = self.table.resolve(ty);
                    if declared.has_error() {
                        continue;
                    }
                    if holds_principal(self.table, &declared, &mut Vec::new()) {
                        self.principal_field(base, host, name, &value);
                        continue;
                    }
                    let field = FieldCtx {
                        input: base == FieldsBase::Input,
                        later: base.has_later_settings(),
                        ty: Ty::Field(host.to_string(), vec![name.clone()]),
                        declared: declared.clone(),
                    };
                    let subject = self.table.subject(&declared);
                    self.settings(node.settings(), subject, Some(&field), &declared);
                    if matches!(base, FieldsBase::Input | FieldsBase::Output) {
                        self.boundary_map_key(&declared, &node, host);
                    }
                }
                Line::NotAList(word) => {
                    let Some(value) = member.value() else {
                        continue;
                    };
                    let span = self.span(value.syntax());
                    let written = tree::written(value.syntax());
                    self.out.push(
                        Diagnostic::new(
                            &rule::TYPE_SETTING_VALUE,
                            format!("`{word}` takes a list in brackets"),
                        )
                        .at(span.clone(), "not a list")
                        .reason(format!(
                            "Lists are always written in square brackets, even with one entry: `{word}: [{written}]`."
                        ))
                        .next(format!("Write `[{written}]`"))
                        .edit(span, format!("[{written}]")),
                    );
                }
                Line::Include(entries) => {
                    let cycle = |name: &str| match self.table.def(name) {
                        Some(super::table::Def::Fields { copies_itself, .. }) => {
                            copies_itself.clone()
                        }
                        _ => None,
                    };
                    // One error for the loop, on its first type by name.
                    let Some(first) = cycle(host).filter(|first| first == host) else {
                        continue;
                    };
                    let copies_itself = |name: &str| cycle(name).as_deref() == Some(first.as_str());
                    let Some(Expr::ListExpr(list)) = member.value() else {
                        continue;
                    };
                    let looping = entries.iter().zip(list.exprs()).find(|(entry, _)| {
                        matches!(entry, TypeRef::Path(names, _) if names.len() == 1
                            && (names[0] == host || copies_itself(&names[0])))
                    });
                    if let Some((TypeRef::Path(names, _), written)) = looping {
                        let span = self.span(written.syntax());
                        let through = if names[0] == host {
                            format!("`{host}` lists itself in `include`")
                        } else {
                            format!(
                                "`{}` leads back to `{host}` through its own `include`",
                                names[0]
                            )
                        };
                        self.out.push(
                            Diagnostic::new(
                                &rule::TYPE_COPY_CYCLE,
                                format!("`{host}` includes its own fields"),
                            )
                            .at(span, "leads back here")
                            .reason(format!(
                                "{through}, so its fields would be copied from themselves and the type could never be written out."
                            ))
                            .next(format!("Remove `{}` from `include`, or write the fields it needs", names[0])),
                        );
                    }
                }
                Line::Reserved(name) => {
                    let Some(key) = member.member_key() else {
                        continue;
                    };
                    let span = self.span(key.syntax());
                    self.out.push(
                        Diagnostic::new(
                            &rule::TYPE_RESERVED_FIELD_NAME,
                            format!("`{name}` can't be a field's name"),
                        )
                        .at(span, "reserved")
                        .reason("`include`, `pick`, `omit` and `optional` are lines that build a type's fields, so no field may take their names.")
                        .next("Rename the field"),
                    );
                }
                _ => {}
            }
        }
        if !area_two && base != FieldsBase::Object {
            return;
        }
        let table = self.table;
        let (_, problems) = derive(base, lines, &mut |name| match table.fields(name) {
            Some(fields) => Found::Fields(fields.iter().map(|f| f.name.clone()).collect()),
            None if table.contains(name) || StdNames::get().is_type(name) => Found::NotFields,
            None => Found::Unknown,
        });
        for problem in problems {
            if let Some(this) = member(problem.line) {
                let first = match &problem.kind {
                    LineKind::Duplicate(name) => lines
                        .iter()
                        .position(|line| matches!(line, Line::Field { name: n, .. } if n == name))
                        .and_then(member)
                        .and_then(|first| first.member_key())
                        .map(|key| self.span(key.syntax())),
                    _ => None,
                };
                self.line_problem(&problem, &this, host, first);
            }
        }
    }

    /// A map at a boundary is a JSON object, whose keys are text, so its
    /// key type is built on `Text` or `Uuid` (area 2, question 5).
    fn boundary_map_key(&mut self, declared: &Ty, node: &TypeNode, host: &str) {
        let mut current = declared.clone();
        for _ in 0..16 {
            match self.table.structure(&current) {
                Ty::List(inner) | Ty::Set(inner) => current = *inner,
                Ty::Map(key, value) => {
                    let text = matches!(
                        self.table.representation(&key),
                        Some(Prim::Text | Prim::Uuid)
                    );
                    if !key.has_error() && !text {
                        let span = self.span(node.syntax());
                        self.out.push(
                            Diagnostic::new(
                                &rule::TYPE_MAP_KEY_BOUNDARY,
                                format!("`{host}` holds a map keyed by `{key}`"),
                            )
                            .at(span, format!("keys are `{key}`"))
                            .reason(format!(
                                "`{host}` crosses a boundary as JSON, where a map is an object and its keys are text; `{key}` is {}.",
                                literal::what(self.table, &key)
                            ))
                            .next("Key the map by a type built on `Text` or `Uuid`, or send a list of objects instead"),
                        );
                        return;
                    }
                    current = *value;
                }
                _ => return,
            }
        }
    }

    fn line_problem(
        &mut self,
        problem: &LineProblem,
        member: &ast::Member,
        host: &str,
        first: Option<Span>,
    ) {
        let key_span = member
            .member_key()
            .map_or_else(|| self.span(member.syntax()), |key| self.span(key.syntax()));
        let entry = problem.entry.and_then(|entry| match member.value() {
            Some(Expr::ListExpr(list)) => list.exprs().nth(entry),
            _ => None,
        });
        let entry_span = entry
            .as_ref()
            .map_or_else(|| key_span.clone(), |entry| self.span(entry.syntax()));
        let entry_text = entry
            .as_ref()
            .map(|entry| tree::written(entry.syntax()))
            .unwrap_or_default();
        let diagnostic = match &problem.kind {
            LineKind::Duplicate(name) => {
                let mut found = Diagnostic::new(
                    &rule::TYPE_DUPLICATE_FIELD,
                    format!("`{host}` declares `{name}` twice"),
                )
                .at(key_span, "declared again");
                if let Some(first) = first {
                    found = found.also(first, "first declared here");
                }
                found
                    .reason("Each field is written once; a second line with the same name would leave its reader guessing which one counts.")
                    .next(format!("Remove one `{name}`, or rename it"))
            }
            LineKind::CopyIntoInput => Diagnostic::new(
                &rule::TYPE_COPY_INTO_INPUT,
                format!("`{host}` is an `Input` and can't use `include`"),
            )
            .at(key_span, "copies every field")
            .reason("`include` would also accept every field added to the source type later, so a request could set fields nobody chose to expose.")
            .next("Name the fields this input accepts with `pick: [...]`"),
            LineKind::NotFields(name) => Diagnostic::new(
                &rule::TYPE_NOT_RECORD,
                format!("`{name}` has no fields to copy"),
            )
            .at(entry_span, "no fields")
            .reason("`include` and `pick` copy fields from objects, inputs, outputs, events and entities.")
            .next(format!("Remove `{entry_text}` from the list")),
            LineKind::UnknownField { host: source, field, fields } => {
                let near = nearest(field, fields.iter().map(String::as_str)).map(str::to_string);
                let found = Diagnostic::new(
                    &rule::TYPE_UNKNOWN_FIELD,
                    format!("`{source}` has no field `{field}`"),
                )
                .at(entry_span.clone(), "not declared")
                .reason(format!(
                    "`pick` copies fields `{source}` declares.{}",
                    did_you_mean(near.as_deref())
                ));
                match near {
                    Some(near) => found
                        .next(format!("Use `{source}.{near}`"))
                        .edit(entry_span, format!("{source}.{near}")),
                    None => found.next(format!("Pick a field `{source}` declares")),
                }
            }
            LineKind::NotAFieldPath => Diagnostic::new(
                &rule::TYPE_SETTING_VALUE,
                format!("`{entry_text}` isn't a field"),
            )
            .at(entry_span, "expected `Type.field`")
            .reason("`pick`, `omit` and `optional` name fields as `Type.field`, such as `Type.field`.")
            .next("Name a field as `Type.field`"),
            LineKind::Absent { field, word } => Diagnostic::new(
                &rule::TYPE_ABSENT_FIELD,
                format!("`{host}` has no field `{field}` at this point"),
            )
            .at(entry_span, "not present")
            .reason(format!(
                "`{word}` names a field the lines above bring in, so a typo or a field removed since is caught; none of them brings in `{field}`."
            ))
            .next(format!("Remove `{entry_text}` from the list")),
            LineKind::CopyClash { field, earlier } => Diagnostic::new(
                &rule::TYPE_COPY_CLASH,
                format!("`{field}` is already in `{host}`, from {earlier}"),
            )
            .at(entry_span, "copied again")
            .reason("A copied field never replaces another field, because a field added to a source type would then change this type unseen.")
            .next(format!(
                "Remove `{field}` from one of them with `omit: [...]`, or write `{field}` as a field below them"
            )),
            LineKind::OptionalOutsideInput => optional_outside(host, key_span),
        };
        self.out.push(diagnostic);
    }

    // --- settings ---------------------------------------------------------

    /// Checks a block of settings: a scalar type's constraints, or a
    /// field's constraints and, on an `Input`, `optional` and `default`.
    /// Returns the rules that apply.
    fn settings(
        &mut self,
        list: Option<ast::MemberList>,
        subject: Subject,
        field: Option<&FieldCtx>,
        parent: &Ty,
    ) -> Rules {
        let mut rules = Rules::default();
        let mut seen: BTreeMap<String, (Span, Const)> = BTreeMap::new();
        for (name, member) in tree::members(list) {
            let Some(key) = tree::name_text(&name) else {
                continue;
            };
            let key_span = self.span(name.syntax());
            let value_node = member.value();
            let value = value_node.as_ref().map_or(Const::Other, Const::from_expr);
            let value_span = value_node
                .as_ref()
                .map_or_else(|| self.span(member.syntax()), |v| self.span(v.syntax()));
            if let Some((first, _)) = seen.get(&key) {
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_CONSTRAINT_REPEATED,
                        format!("`{key}` is given twice"),
                    )
                    .at(key_span, "given again")
                    .also(first.clone(), "first given here")
                    .reason("Each setting is given once, so there is never a question of which one counts.")
                    .next(format!("Remove one `{key}`")),
                );
                continue;
            }
            seen.insert(key.clone(), (key_span.clone(), value.clone()));
            if field.is_some_and(|field| field.later)
                && LATER_FIELD_SETTINGS.contains(&key.as_str())
            {
                continue;
            }
            if let Some(field) = field
                && FIELD_SETTINGS.contains(&key.as_str())
            {
                self.field_setting(
                    &key,
                    &value,
                    value_node.as_ref(),
                    key_span,
                    value_span,
                    field,
                );
                continue;
            }
            match rules::constraint(subject, &key, &value, &mut rules) {
                Ok(()) => {}
                Err(Problem::Unknown) => {
                    let known: Vec<&str> = CONSTRAINTS
                        .iter()
                        .copied()
                        .chain(
                            field
                                .map_or(&[][..], |_| &FIELD_SETTINGS[..])
                                .iter()
                                .copied(),
                        )
                        .collect();
                    let near = nearest(&key, known.iter().copied());
                    let mut problem = Diagnostic::new(
                        &rule::TYPE_UNKNOWN_SETTING,
                        format!("`{key}` isn't a setting Jadpo knows"),
                    )
                    .at(key_span.clone(), "unknown setting")
                    .reason(format!(
                        "A type's block holds the constraints {}.{}",
                        crate::and_list(
                            &CONSTRAINTS
                                .iter()
                                .map(|c| format!("`{c}`"))
                                .collect::<Vec<_>>()
                        ),
                        did_you_mean(near)
                    ));
                    problem = match near {
                        Some(near) => problem
                            .next(format!("Use `{near}` instead of `{key}`"))
                            .edit(key_span, near),
                        None => problem.next(format!("Remove `{key}`")),
                    };
                    self.out.push(problem);
                }
                Err(Problem::Base(fits)) => {
                    let base = match subject {
                        Subject::Prim(prim) => {
                            let an = if prim.name().starts_with(['A', 'E', 'I', 'O', 'U']) {
                                "an"
                            } else {
                                "a"
                            };
                            format!("{an} `{}` type", prim.name())
                        }
                        Subject::Collection => "a collection".to_string(),
                        Subject::Other => "this type".to_string(),
                    };
                    self.out.push(
                        Diagnostic::new(
                            &rule::TYPE_CONSTRAINT_BASE,
                            format!("`{key}` doesn't apply to {base}"),
                        )
                        .at(key_span, "doesn't fit the base")
                        .reason(format!("`{key}` constrains {fits}."))
                        .next(format!("Remove `{key}`, or use a constraint that fits")),
                    );
                }
                Err(Problem::Value(expected, fix)) => {
                    let problem = Diagnostic::new(
                        &rule::TYPE_SETTING_VALUE,
                        format!("`{key}` takes {expected}"),
                    )
                    .at(value_span.clone(), format!("{} here", value.describe()))
                    .reason(format!(
                        "`{key}` is a constraint, and its value must be {expected}."
                    ));
                    self.out.push(match fix {
                        Some(fix) => problem.next(format!("Write `{fix}`")).edit(value_span, fix),
                        None => problem.next(format!("Give `{key}` {expected}")),
                    });
                }
                Err(Problem::Pattern(why)) => {
                    self.out.push(
                        Diagnostic::new(
                            &rule::TYPE_INVALID_PATTERN,
                            "This pattern can't be used",
                        )
                        .at(value_span, why.clone())
                        .reason("A pattern is a regular expression that must match the whole text, without backreferences or lookaround, so it runs in linear time and the same way on every target.")
                        .next("Rewrite the pattern without the unsupported part"),
                    );
                }
            }
        }
        self.contradictions(
            &seen,
            "min",
            "max",
            rules.min.zip(rules.max).map(|(a, b)| a.compare(b).is_gt()),
        );
        self.contradictions(
            &seen,
            "min_length",
            "max_length",
            rules.min_length.zip(rules.max_length).map(|(a, b)| a > b),
        );
        self.contradictions(
            &seen,
            "min",
            "max",
            rules
                .min_duration
                .zip(rules.max_duration)
                .map(|(a, b)| a > b),
        );
        self.inherited(&seen, &rules, parent);
        rules
    }

    /// A refinement's rules against those of the types it builds on (area
    /// 2): no value could follow a `max` below an inherited `min`, or the
    /// other way round, and a `Decimal`'s scale is fixed where it is first
    /// declared, since it decides how the value is stored and sent.
    fn inherited(&mut self, seen: &BTreeMap<String, (Span, Const)>, own: &Rules, parent: &Ty) {
        let chain: Vec<(String, Rules)> = self
            .table
            .rule_chain(parent)
            .into_iter()
            .map(|(host, rules)| (host, rules.clone()))
            .collect();
        let mut found: Option<(&str, String, String, String)> = None;
        for (host, rules) in &chain {
            let pairs: [(&str, Option<bool>, String, String); 6] = [
                (
                    "max",
                    own.max.zip(rules.min).map(|(a, b)| a.compare(b).is_lt()),
                    format!("min: {}", rules.min.map(rules::show).unwrap_or_default()),
                    "below".into(),
                ),
                (
                    "min",
                    own.min.zip(rules.max).map(|(a, b)| a.compare(b).is_gt()),
                    format!("max: {}", rules.max.map(rules::show).unwrap_or_default()),
                    "above".into(),
                ),
                (
                    "max_length",
                    own.max_length.zip(rules.min_length).map(|(a, b)| a < b),
                    format!("min_length: {}", rules.min_length.unwrap_or_default()),
                    "below".into(),
                ),
                (
                    "min_length",
                    own.min_length.zip(rules.max_length).map(|(a, b)| a > b),
                    format!("max_length: {}", rules.max_length.unwrap_or_default()),
                    "above".into(),
                ),
                (
                    "max",
                    own.max_duration.zip(rules.min_duration).map(|(a, b)| a < b),
                    format!(
                        "min: {}",
                        rules
                            .min_duration
                            .map(|d| d.to_string())
                            .unwrap_or_default()
                    ),
                    "below".into(),
                ),
                (
                    "min",
                    own.min_duration.zip(rules.max_duration).map(|(a, b)| a > b),
                    format!(
                        "max: {}",
                        rules
                            .max_duration
                            .map(|d| d.to_string())
                            .unwrap_or_default()
                    ),
                    "above".into(),
                ),
            ];
            if let Some((key, _, theirs, word)) = pairs
                .into_iter()
                .find(|(_, broken, _, _)| *broken == Some(true))
            {
                found = Some((key, theirs, word, host.clone()));
                break;
            }
            if let (Some(mine), Some(theirs)) = (own.scale, rules.scale)
                && mine != theirs
            {
                found = Some((
                    "scale",
                    format!("scale: {theirs}"),
                    "different from".into(),
                    host.clone(),
                ));
                break;
            }
        }
        let Some((key, theirs, word, host)) = found else {
            self.loosened(seen, own, &chain);
            return;
        };
        let Some((span, value)) = seen.get(key) else {
            return;
        };
        let written = match value {
            Const::Number { value, .. } => rules::show(*value),
            Const::Duration(text) => text.clone(),
            _ => String::new(),
        };
        let reason = if key == "scale" {
            format!(
                "`{host}` fixes the scale at {}: a `Decimal`'s scale decides how it is stored and sent, so a refinement keeps it.",
                theirs.trim_start_matches("scale: ")
            )
        } else {
            format!(
                "A refinement adds rules to `{host}`'s and can't loosen them, so no value could follow both, and the type could never be built."
            )
        };
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_CONSTRAINT_CONTRADICTION,
                format!("`{key}: {written}` is {word} `{host}`'s `{theirs}`"),
            )
            .at(span.clone(), format!("{word} `{host}`'s `{theirs}`"))
            .reason(reason)
            .next(if key == "scale" {
                format!("Remove `scale:`, or build on `Decimal` instead of `{host}`")
            } else {
                format!("Correct `{key}`, or build on a type whose rules it fits")
            }),
        );
    }

    /// A refinement's bound looser than one it inherits (area 2: a
    /// refinement can only add to the rules of the type it builds on).
    /// Every rule along the chain still holds, so the looser one would
    /// never apply and only misleads the reader.
    fn loosened(
        &mut self,
        seen: &BTreeMap<String, (Span, Const)>,
        own: &Rules,
        chain: &[(String, Rules)],
    ) {
        for (host, rules) in chain {
            let show_duration = |d: Option<jadpo_std::Dur>| d.map(|d| d.to_string());
            let pairs: [(&str, bool, Option<String>, &str); 6] = [
                (
                    "max",
                    own.max
                        .zip(rules.max)
                        .is_some_and(|(a, b)| a.compare(b).is_gt()),
                    rules.max.map(rules::show),
                    "at most",
                ),
                (
                    "min",
                    own.min
                        .zip(rules.min)
                        .is_some_and(|(a, b)| a.compare(b).is_lt()),
                    rules.min.map(rules::show),
                    "at least",
                ),
                (
                    "max_length",
                    own.max_length
                        .zip(rules.max_length)
                        .is_some_and(|(a, b)| a > b),
                    rules.max_length.map(|n| n.to_string()),
                    "at most",
                ),
                (
                    "min_length",
                    own.min_length
                        .zip(rules.min_length)
                        .is_some_and(|(a, b)| a < b),
                    rules.min_length.map(|n| n.to_string()),
                    "at least",
                ),
                (
                    "max",
                    own.max_duration
                        .zip(rules.max_duration)
                        .is_some_and(|(a, b)| a > b),
                    show_duration(rules.max_duration),
                    "at most",
                ),
                (
                    "min",
                    own.min_duration
                        .zip(rules.min_duration)
                        .is_some_and(|(a, b)| a < b),
                    show_duration(rules.min_duration),
                    "at least",
                ),
            ];
            let Some((key, _, Some(theirs), bound)) =
                pairs.into_iter().find(|(_, looser, _, _)| *looser)
            else {
                continue;
            };
            let Some((span, value)) = seen.get(key) else {
                return;
            };
            let written = match value {
                Const::Number { value, .. } => rules::show(*value),
                Const::Duration(text) => text.clone(),
                _ => String::new(),
            };
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_CONSTRAINT_LOOSENED,
                    format!("`{key}: {written}` loosens `{host}`'s `{key}: {theirs}`"),
                )
                .at(span.clone(), format!("looser than `{key}: {theirs}`"))
                .reason(format!(
                    "A refinement only adds to the rules of the type it builds on, and every rule along the way still holds, so a value still needs `{key}: {theirs}` and this one would never apply."
                ))
                .next(format!(
                    "Make `{key}` {bound} {theirs}, or remove it"
                )),
            );
            return;
        }
    }

    fn contradictions(
        &mut self,
        seen: &BTreeMap<String, (Span, Const)>,
        low: &str,
        high: &str,
        contradicts: Option<bool>,
    ) {
        if contradicts != Some(true) {
            return;
        }
        let (Some((low_span, low_value)), Some((high_span, high_value))) =
            (seen.get(low), seen.get(high))
        else {
            return;
        };
        let show = |value: &Const| match value {
            Const::Number { value, .. } => rules::show(*value),
            Const::Duration(text) => text.clone(),
            _ => String::new(),
        };
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_CONSTRAINT_CONTRADICTION,
                format!(
                    "`{high}: {}` is below `{low}: {}`",
                    show(high_value),
                    show(low_value)
                ),
            )
            .at(high_span.clone(), "below the minimum")
            .also(low_span.clone(), "the minimum")
            .reason("No value could follow both rules, so the type could never be built.")
            .next(format!("Correct `{low}` or `{high}`")),
        );
    }

    fn field_setting(
        &mut self,
        key: &str,
        value: &Const,
        value_node: Option<&Expr>,
        key_span: Span,
        value_span: Span,
        field: &FieldCtx,
    ) {
        let host = match &field.ty {
            Ty::Field(host, _) => host.clone(),
            other => other.to_string(),
        };
        if !field.input {
            self.out.push(optional_outside(&host, key_span));
            return;
        }
        if key == "header" {
            self.header(value, value_span, field);
            return;
        }
        if key == "optional" {
            if !matches!(value, Const::Bool(_)) {
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_SETTING_VALUE,
                        "`optional` takes `true` or `false`",
                    )
                    .at(value_span, format!("{} here", value.describe()))
                    .reason("`optional: true` lets a request leave the field out.")
                    .next("Write `optional: true`"),
                );
            }
            return;
        }
        // `default:` is checked like a literal going into the field.
        let lower_path = matches!(value, Const::Path(names)
            if !names.first().is_some_and(|head| crate::case::starts_upper(head)));
        if let Const::Path(names) = value
            && !lower_path
        {
            // A variant, `Status.open`, is checked where it is used; any
            // other declaration's name is no value (area 3).
            let head = names.first().map_or("", String::as_str);
            let variant = names.len() == 2
                && matches!(self.table.def(head), Some(super::table::Def::Enum { .. }));
            if !variant && let Some(named) = crate::not_value::classify_type(self.table, head) {
                self.out
                    .push(crate::not_value::diagnostic(head, &named, value_span));
            }
            return;
        }
        if matches!(value, Const::Other) || lower_path {
            self.out.push(
                Diagnostic::new(&rule::TYPE_SETTING_VALUE, "`default` takes a literal")
                    .at(value_span, "a computed value")
                    .reason("A default is filled in when a request leaves the field out, and is checked while compiling, so it is written as a literal.")
                    .next("Write the default as a literal, such as `25` or `none`"),
            );
            return;
        }
        if let Err(problem) = literal::check(self.table, &field.ty, value) {
            let written = value_node
                .map(|v| tree::written(v.syntax()))
                .unwrap_or_default();
            let mut expected = field.ty.clone();
            if self.table.is_nullable(&field.declared) && !self.table.is_nullable(&expected) {
                expected = expected.nullable();
            }
            self.out.push(literal::diagnostic(
                self.table, problem, &written, &expected, value_span, false,
            ));
        }
    }
}

impl Decls<'_> {
    /// An `Input` field's `header:` (area 7, "One input"): a header named
    /// once, by a valid name, holding one value, and never one
    /// authentication owns (area 8).
    fn header(&mut self, value: &Const, span: Span, field: &FieldCtx) {
        let Const::Str(name) = value else {
            self.out.push(
                Diagnostic::new(&rule::TYPE_SETTING_VALUE, "`header` takes the header's name")
                    .at(span, format!("{} here", value.describe()))
                    .reason("`header:` names the request header the field is read from, as text: `header: \"X-Request-Tag\"`.")
                    .next("Write the header's name in quotes"),
            );
            return;
        };
        let valid = !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b));
        if !valid {
            self.out.push(
                Diagnostic::new(
                    &rule::ROUTE_HEADER_NAME_INVALID,
                    format!("`{name}` isn't a header name"),
                )
                .at(span, "not a header name")
                .reason("A header name is letters, digits and `-` (HTTP allows a few more symbols), with no spaces: `X-Request-Tag`.")
                .next("Write the header's name without spaces"),
            );
            return;
        }
        let lower = name.to_ascii_lowercase();
        // The runtime reads `Idempotency-Key` to answer a retried write
        // once (area 7, question 17); the action never sees it.
        if lower == "idempotency-key" {
            self.out.push(
                Diagnostic::new(
                    &rule::ROUTE_HEADER_CREDENTIAL,
                    format!("`{name}` belongs to the runtime"),
                )
                .at(span, "read by the runtime")
                .reason("The runtime reads `Idempotency-Key` to answer a retried write once, before anything runs, so the action never sees the key (area 7).")
                .next("Remove the field; a retry with the same key is answered by the runtime"),
            );
            return;
        }
        if matches!(
            lower.as_str(),
            "authorization" | "proxy-authorization" | "cookie" | "set-cookie"
        ) || lower.starts_with("x-jadpo-")
        {
            self.out.push(
                Diagnostic::new(
                    &rule::ROUTE_HEADER_CREDENTIAL,
                    format!("`{name}` belongs to authentication"),
                )
                .at(span, "read by authentication")
                .reason("Credentials (`Authorization`, cookies) and Jadpo's own `X-Jadpo-` headers are read by authentication (area 8), so a route never sees them as input.")
                .next("Remove the field, and take the caller as the route's `current_principal`"),
            );
            return;
        }
        let structure = self.table.structure(&field.declared);
        let scalar = match structure.strip_nullable() {
            Ty::Prim(_) | Ty::Error => true,
            Ty::Named(name) => self
                .table
                .variants(name)
                .is_some_and(|variants| variants.iter().all(|v| v.fields.is_none())),
            _ => false,
        };
        if !scalar {
            self.out.push(
                Diagnostic::new(
                    &rule::ROUTE_HEADER_NOT_SCALAR,
                    format!("A header can't hold `{}`", field.declared),
                )
                .at(span, "one value per header")
                .reason("A header holds one value, so a field read from one is a scalar or an enum without data; lists and objects come in the body.")
                .next("Move the field to the body, or give it a scalar type"),
            );
            return;
        }
        if let Some((_, first)) = self.headers.iter().find(|(seen, _)| *seen == lower) {
            let first = first.clone();
            self.out.push(
                Diagnostic::new(
                    &rule::ROUTE_HEADER_DUPLICATE,
                    format!("Header `{name}` is read twice"),
                )
                .at(span, "read again")
                .also(first, "first read here")
                .reason("Header names are compared without case, so two fields reading one header would always hold the same value.")
                .next("Remove one of the fields"),
            );
            return;
        }
        self.headers.push((lower, span));
    }
}

struct FieldCtx {
    input: bool,
    /// Whether later areas add settings of their own to this field.
    later: bool,
    /// The field type, `Type.field`.
    ty: Ty,
    declared: Ty,
}

/// Settings later areas give an entity's or event's fields: data (area 5),
/// policy (area 9) and personal data (area 15). They are checked when
/// those areas are built.
const LATER_FIELD_SETTINGS: [&str; 9] = [
    "erase",
    "generated",
    "immutable",
    "on_delete",
    "personal",
    "policy",
    "retention",
    "role",
    "unique",
];

/// Whether a type is a caller, or holds one (area 8): in a list, a map or
/// an option, or in a field of an object or a variant's data, however
/// deep. An input, output or failure holding one is reported at its own
/// declaration, so it isn't followed again.
fn holds_principal(table: &dyn TypeView, ty: &Ty, seen: &mut Vec<String>) -> bool {
    match ty {
        Ty::Opaque(name, _) => {
            name == crate::identity::PRINCIPAL
                || name.starts_with(&format!("{}.", crate::identity::PRINCIPAL))
        }
        Ty::List(inner) | Ty::Set(inner) | Ty::Nullable(inner) => {
            holds_principal(table, inner, seen)
        }
        Ty::Map(key, value) => {
            holds_principal(table, key, seen) || holds_principal(table, value, seen)
        }
        Ty::Named(name) => {
            if seen.contains(name) {
                return false;
            }
            seen.push(name.clone());
            match table.def(name) {
                Some(super::table::Def::Fields { base, fields, .. })
                    if !matches!(
                        base,
                        FieldsBase::Input | FieldsBase::Output | FieldsBase::Failure
                    ) =>
                {
                    fields
                        .iter()
                        .any(|field| holds_principal(table, &field.declared, seen))
                }
                Some(super::table::Def::Enum { variants }) => variants
                    .iter()
                    .flat_map(|variant| variant.fields.iter().flatten())
                    .any(|field| holds_principal(table, &field.declared, seen)),
                _ => false,
            }
        }
        _ => false,
    }
}

impl Decls<'_> {
    /// A field typed with a caller, where none may go (area 8): a caller
    /// never reaches a client, never comes from a request and never sits
    /// in a failure.
    fn principal_field(&mut self, base: FieldsBase, host: &str, name: &str, value: &Expr) {
        let span = self.span(value.syntax());
        let (rule, why, next) = if self.failure {
            (
                &rule::TYPE_AUTH_PRINCIPAL_FAILURE_CONTEXT,
                "A failure is logged and its client fields reach the client, while a caller value holds who signed in, so a failure carries the caller's `id` instead.",
                format!("Type `{name}` with the entity's id, such as `User.id`"),
            )
        } else {
            match base {
                FieldsBase::Input => (
                    &rule::TYPE_AUTH_PRINCIPAL_INPUT,
                    "A caller comes only from signing in, never from what a request sends, so a route takes it as `current_principal` and passes it on.",
                    format!("Remove `{name}`, and pass `current_principal` to what the route runs"),
                ),
                FieldsBase::Output => (
                    &rule::TYPE_AUTH_PRINCIPAL_OUTPUT,
                    "A caller value is who signed in, as the runtime holds it, and is never sent to a client; an output sends the fields a client needs.",
                    format!(
                        "Type `{name}` with the entity's id, such as `User.id`, or send the fields a client needs"
                    ),
                ),
                _ => return,
            }
        };
        self.out.push(
            Diagnostic::new(rule, format!("`{host}.{name}` holds a caller"))
                .at(span, "a caller")
                .reason(why)
                .next(next),
        );
    }
}

fn optional_outside(host: &str, span: Span) -> Diagnostic {
    Diagnostic::new(
        &rule::TYPE_OPTIONAL_OUTSIDE_INPUT,
        format!("`{host}` isn't an `Input`, so its fields are always given"),
    )
    .at(span, "only for an `Input`")
    .reason("Only a request arriving from outside may leave fields out or have them filled by default; an object built inside the application states every field, using `none` for a field declared `T?`.")
    .next("Remove the setting, or make the field's type `T?` if it may be absent")
}

fn did_you_mean(near: Option<&str>) -> String {
    near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
}

/// A type with a standard type's shape: the standard type, and each of its
/// fields with the application's field it matched.
#[derive(Clone, Debug)]
pub(crate) struct StandardMatch {
    pub standard: &'static jadpo_std::StandardType,
    pub fields: Vec<MatchedField>,
}

#[derive(Clone, Debug)]
pub(crate) struct MatchedField {
    /// What the field is, as an error says it: "a start".
    pub meaning: &'static str,
    /// The application's field.
    pub written: String,
    /// The standard type's own name for it: `start`.
    pub standard: &'static str,
}

/// Whether `ty` is an enum without data, as a currency held as a
/// project's own enum is (area 19).
fn is_enum(table: &dyn TypeView, ty: &Ty) -> bool {
    match table.structure(ty) {
        Ty::Named(name) => matches!(
            table.def(&name),
            Some(Def::Enum { variants }) if variants.iter().all(|v| v.fields.is_none())
        ),
        _ => false,
    }
}

/// The standard type whose shape `name`'s fields are (area 19, question
/// 5): an `Object` whose fields are exactly a shape's, by their common
/// names in any order and what each is built on, none of them optional;
/// `only` keeps to the standard type of that name.
pub(crate) fn standard_match(
    table: &dyn TypeView,
    name: &str,
    only: Option<&str>,
) -> Option<StandardMatch> {
    let fields = table.fields(name)?;
    for entry in jadpo_std::STANDARD_TYPES {
        if only.is_some_and(|only| only != entry.name) {
            continue;
        }
        let Some(shape) = entry.shape else { continue };
        if shape.len() != fields.len() {
            continue;
        }
        let mut matched: Vec<MatchedField> = Vec::new();
        for wanted in shape {
            let found = fields.iter().find(|field| {
                wanted.names.contains(&field.name.as_str())
                    && !table.is_nullable(&field.declared)
                    && (table
                        .base_prim(&field.declared)
                        .is_some_and(|prim| wanted.built_on.contains(&prim))
                        || (wanted.or_enum && is_enum(table, &field.declared)))
                    && !matched.iter().any(|taken| taken.written == field.name)
            });
            match found {
                Some(field) => matched.push(MatchedField {
                    meaning: wanted.meaning,
                    written: field.name.clone(),
                    standard: wanted.names[0],
                }),
                None => break,
            }
        }
        if matched.len() == shape.len() {
            return Some(StandardMatch {
                standard: entry,
                fields: matched,
            });
        }
    }
    None
}
