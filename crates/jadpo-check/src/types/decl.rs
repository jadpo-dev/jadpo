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
use super::table::{Found, LineKind, LineProblem, TypeTable, derive, type_table};
use super::ty::{BASES, Prim, Ty, TypeNode, TypeRef, container_arity};
use crate::std_lib::StdNames;
use crate::tree::{self, node_span};

/// Area 2's problems with one file's declarations.
#[salsa::tracked(returns(ref))]
pub fn type_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let Some(source) = tree::source(db, file) else {
        return Vec::new();
    };
    let mut checker = Decls {
        path: file.path(db),
        table: type_table(db, project),
        out: Vec::new(),
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
}

pub(super) struct Decls<'a> {
    pub path: &'a str,
    pub table: &'a TypeTable,
    pub out: Vec<Diagnostic>,
}

impl Decls<'_> {
    fn span(&self, node: &SyntaxNode) -> Span {
        node_span(self.path, node)
    }

    fn decl(&mut self, decl: &Decl) {
        match decl {
            Decl::TypeDecl(d) => self.type_decl(d, decl),
            Decl::EntityDecl(d) => {
                for (_, member) in tree::members(d.member_list()) {
                    if let Some(value) = member.value()
                        && shape::looks_like_type(&value)
                    {
                        self.type_node(&TypeNode::Expr(value), false);
                    }
                }
            }
            Decl::FunctionDecl(d) => self.signature(d.param_list(), d.ret_type(), true),
            Decl::ActionDecl(d) => self.signature(d.param_list(), d.ret_type(), true),
            Decl::QueryDecl(d) => self.signature(d.param_list(), d.ret_type(), true),
            Decl::FixtureDecl(d) => self.signature(d.param_list(), d.ret_type(), false),
            Decl::LibraryDecl(d) => {
                for function in tree::library_functions(d) {
                    self.signature(function.param_list(), function.ret_type(), true);
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

    /// Functions, actions and queries take and return named types, with
    /// collection elements checked too (area 2, question 11).
    fn primitive(&mut self, node: &TypeNode, role: Role, container: Option<&Ty>) {
        let ty = self.table.resolve(&node.to_ref());
        match ty.strip_nullable() {
            Ty::Prim(prim) if prim.is_representation() => {
                let word = match role {
                    Role::Parameter => "parameter",
                    Role::Result => "result",
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
                        .reason(format!(
                            "Functions, actions and queries take and return named types, so each value says what it means and carries its rules; `{name}` says neither."
                        ))
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
                if names.len() == 1 {
                    let arity = match container_arity(first) {
                        Some(arity) => Some(arity),
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
                                0 => format!("`{first}` isn't generic: only `List`, `Set` and `Map` take types in `< >`."),
                                _ => format!("`{first}` holds values of the type in `< >`, so it needs exactly {arity}."),
                            })
                            .next(format!("Write `{first}` with {needs}")),
                        );
                        return false;
                    }
                } else if self.table.contains(first)
                    && self
                        .table
                        .resolve(&TypeRef::Path(names.clone(), Vec::new()))
                        .is_error()
                {
                    return self.unknown_field_type(&names, &head);
                }
                let mut ok = true;
                for arg in node.args() {
                    ok &= self.type_node(&arg, false);
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
                let own = self.settings(list, subject, None);
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
                    self.lines(block, FieldsBase::Object, &shape.name, &lines);
                }
            }
            ShapeKind::Fields { base, lines } => {
                self.lines(list, *base, &shape.name, lines);
            }
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
        for (index, line) in lines.iter().enumerate() {
            let Some(member) = member(index) else {
                continue;
            };
            match line {
                Line::Field { name, ty, .. } => {
                    let Some(value) = member.value() else {
                        continue;
                    };
                    let node = TypeNode::Expr(value);
                    if !self.type_node(&node, false) || !area_two {
                        continue;
                    }
                    let declared = self.table.resolve(ty);
                    if declared.has_error() {
                        continue;
                    }
                    let field = FieldCtx {
                        input: base == FieldsBase::Input,
                        ty: Ty::Field(host.to_string(), vec![name.clone()]),
                        declared: declared.clone(),
                    };
                    let subject = self.table.subject(&declared);
                    self.settings(node.settings(), subject, Some(&field));
                }
                Line::NotAList(word) if area_two => {
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
                Line::Reserved(name) if area_two => {
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
                        Subject::Prim(prim) => format!("a `{}` type", prim.name()),
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
        rules
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
        if matches!(value, Const::Other | Const::Path(_)) {
            if matches!(value, Const::Path(_)) {
                return;
            }
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

struct FieldCtx {
    input: bool,
    /// The field type, `Type.field`.
    ty: Ty,
    declared: Ty,
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
