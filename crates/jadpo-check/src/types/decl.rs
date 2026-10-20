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
use super::table::{Def, FieldDef, Found, LineKind, LineProblem, TypeView, WrittenCoding, derive};
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
    // A provider package's types may say a field's or variant's spelling on
    // the wire (area 11, decided 2026-10-20).
    let provider = jadpo_db::location(db, project, file)
        .ownership
        .package()
        .is_some_and(|home| crate::connections::reach(db, project, home).is_some());
    let mut checker = Decls {
        path: file.path(db),
        table: &lookup,
        out: Vec::new(),
        headers: Vec::new(),
        failure: false,
        provider,
    };
    for decl in source.decls() {
        checker.decl(&decl);
    }
    // Where each rule across fields is written, and what it is called
    // (area 2, "Rules across fields").
    let fields = |host: &str| match lookup.def(host) {
        Some(Def::Fields { fields, .. }) => fields.iter().map(|f| f.name.clone()).collect(),
        _ => Vec::new(),
    };
    checker.out.extend(super::across::rule_problems(
        file.path(db),
        source.syntax(),
        &fields,
    ));
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
    /// Whether the file is in a provider package, whose fields and
    /// variants may give `wire_name:` (area 11).
    pub provider: bool,
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
                        // `Feature<G, P>` and `FeatureCollection<G, P>`
                        // (area 20, "Interop").
                        None if super::body::is_feature(first) => Some(2),
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
                            _ if super::body::is_feature(first) => {
                                "two type arguments, a geometry type and the properties' `Object` type"
                                    .to_string()
                            }
                            _ => format!("{arity} type arguments, as in `{first}<Key, Value>`"),
                        };
                        self.out.push(
                            Diagnostic::new(
                                &rule::TYPE_TYPE_ARGUMENT_COUNT,
                                format!("`{first}` takes {needs}"),
                            )
                            .at(span, format!("{} given", args.len()))
                            .reason(match arity {
                                0 => format!("`{first}` isn't generic: only `List`, `Set`, `Map`, `Range`, `Feature` and `FeatureCollection` take types in `< >`."),
                                _ => format!("`{first}` holds values of the type in `< >`, so it needs exactly {arity}."),
                            })
                            .next(format!("Write `{first}` with {needs}")),
                        );
                        return false;
                    }
                } else if (self.table.contains(first)
                    || crate::std_lib::StdNames::get().is_type(first))
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
                // A feature's geometry is a geometry type and its properties
                // an `Object` type (area 20).
                if ok && names.len() == 1 && super::body::is_feature(first) {
                    let resolved: Vec<Ty> =
                        args.iter().map(|arg| self.table.resolve(arg)).collect();
                    let span = self.span(node.syntax());
                    if let Some(wrong) =
                        super::body::feature_arguments(self.table, first, &resolved, span)
                    {
                        self.out.push(wrong);
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
        // A built-in type's fields name a type only where it is a standard
        // object (`LocalTime.hour`); a batch type's are read from the
        // value the runtime gives, never named (chunk 6.6b).
        if !self.table.contains(host) && jadpo_std::batch::object_fields(host).is_some() {
            let written = names.join(".");
            self.out.push(
                Diagnostic::new(&rule::TYPE_UNKNOWN_FIELD, format!("`{written}` isn't a type"))
                    .at(self.span(head), "a batch type's field")
                    .reason(format!(
                        "A field type names a field an entity, an object or a standard object declares, such as `LocalTime.hour`; `{host}`'s fields are read from the value the runtime gives, never named as a type."
                    ))
                    .next("Name the field's own type, or a type of your own built on it"),
            );
            return false;
        }
        let standard = (!self.table.contains(host))
            .then(|| super::table::standard_fields(host).unwrap_or_default());
        let (fields, word): (Vec<String>, &str) =
            match (standard, self.table.fields(host), self.table.variants(host)) {
                (Some(standard), _, _) => {
                    (standard.iter().map(|f| f.name.clone()).collect(), "field")
                }
                (None, Some(fields), _) if names.len() == 2 => {
                    (fields.iter().map(|f| f.name.clone()).collect(), "field")
                }
                (None, _, Some(variants)) if names.len() == 3 => (
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
                    if self.provider {
                        self.standard_candidates(decl, &shape.name);
                    }
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
        // In a package, only its public types: its own shapes are what it
        // decodes from before a value leaves (area 19, question 1; chunk
        // 7.1b).
        let in_package = jadpo_db::package_at(self.path).is_some();
        if in_package && decl.visibility().is_none() {
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
        // A list's items, as the reason says them: `Point`s, or a type of
        // the application's of `Point`'s shape (chunk 5.5u6).
        let mut items: Vec<String> = Vec::new();
        for wanted in entry.shape.unwrap_or_default() {
            let Some(item) = wanted.list_of else { continue };
            let declared = found
                .fields
                .iter()
                .find(|field| field.standard == wanted.names[0])
                .and_then(|field| {
                    self.table
                        .fields(name)?
                        .iter()
                        .find(|own| own.name == field.written)
                })
                .map(|own| self.table.structure(&own.declared));
            items.push(match declared {
                Some(Ty::List(of)) if !matches!(*of, Ty::Opaque(..)) => {
                    format!("each a `{of}`, the standard `{item}` under another name")
                }
                _ => format!("each a `{item}`"),
            });
        }
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
        // One shape that is more than one standard type's names them all;
        // the swap is to the first (chunk 5.5u6).
        let all: Vec<String> = same_shape(&found)
            .into_iter()
            .map(|name| format!("`{name}`"))
            .collect();
        let others = &all[1..];
        let made_of = if items.is_empty() {
            format!(", each built on {}", crate::or_list(&built))
        } else {
            format!(", {}", items.join(", "))
        };
        let next = if others.is_empty() {
            format!(
                "Run `jadpo fix {name} swap` to use `{standard}` where `{name}` is used, then convert at the edge where data arrives in another shape"
            )
        } else {
            format!(
                "Run `jadpo fix {name} swap` to use `{standard}` where `{name}` is used, or use {} if that is what it means, then convert at the edge where data arrives in another shape",
                crate::or_list(others)
            )
        };
        let (reason, next) = if in_package {
            // A package's public type is converted at the edge, in the
            // package, from whatever the provider sends (area 19).
            let edge = if standard == "Point" {
                " with `geo.from_lat_lng` or `geo.from_lng_lat` where the provider sends a pair"
            } else {
                " where the provider sends another shape"
            };
            (
                format!(
                    "`{name}` has only {}, which is the standard {}. Packages that each declare their own disagree on order and on names, and an app has to convert between them.",
                    crate::and_list(&named),
                    crate::or_list(&all)
                ),
                format!(
                    "Use `{standard}` in the package's public types, converting at the edge{edge}"
                ),
            )
        } else {
            (
                format!(
                    "`{name}` has only {}{made_of}, which is the standard {}. Declarations of one shape under different names disagree on names and meaning, and every package and application has to convert between them; the standard type has one construction, one wire shape and one storage shape.",
                    crate::and_list(&named),
                    crate::or_list(&all)
                ),
                next,
            )
        };
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_STANDARD_EXISTS,
                format!(
                    "`{name}` is the standard {} under another name",
                    crate::or_list(&all)
                ),
            )
            .at(self.span(written.syntax()), crate::and_list(&meanings))
            .reason(reason)
            .next(next),
        );
    }

    /// `package.standard_candidate` (area 11, "Mapping imported types to
    /// standard types"; chunk 7.2e): in a provider package's public
    /// object, a field the provider sends in its own shape that looks
    /// like a standard type the package doesn't use. A whole number named
    /// like a time may be an `Instant` sent as seconds; one named like an
    /// amount beside a currency, `Money` in minor units; a latitude beside
    /// a longitude, a `Point`. The package says how each is sent once it is
    /// swapped, so a field that says so already is never one. An object
    /// that is a standard type whole is `type.standard_exists` instead.
    fn standard_candidates(&mut self, decl: &ast::TypeDecl, name: &str) {
        use jadpo_std::provider::{
            amount_like, currency_named, latitude_named, longitude_named, time_named,
        };
        if decl.visibility().is_none()
            || jadpo_std::standard_type(name).is_some()
            || standard_match(self.table, name, None).is_some()
        {
            return;
        }
        let Some(fields) = self.table.fields(name).map(<[FieldDef]>::to_vec) else {
            return;
        };
        let spans: BTreeMap<String, Span> = tree::members(decl.member_list())
            .into_iter()
            .filter_map(|(key, _)| Some((tree::name_text(&key)?, self.span(key.syntax()))))
            .collect();
        let plain = |field: &FieldDef| field.coding.is_none() && !field.copied;
        let built_on = |field: &FieldDef, prims: &[Prim]| {
            !is_enum(self.table, &field.declared)
                && self
                    .table
                    .base_prim(field.declared.strip_nullable())
                    .is_some_and(|prim| prims.contains(&prim))
        };
        let spelled = |field: &FieldDef| field.wire_name.clone().unwrap_or(field.name.clone());
        // Where the object's currency is sent: a field of its own, or a
        // part another field reads.
        let currency = fields.iter().find_map(|field| match &field.coding {
            None if currency_named(&field.name) => Some((spelled(field), true)),
            Some(WrittenCoding::Parts(parts)) => parts
                .iter()
                .find(|(part, _)| part == "currency")
                .map(|(_, at)| (at.clone(), false)),
            _ => None,
        });
        let longitude = fields.iter().find(|field| {
            plain(field)
                && longitude_named(&field.name)
                && built_on(field, &[Prim::Int, Prim::Decimal])
        });
        let mut found: Vec<Diagnostic> = Vec::new();
        for field in fields.iter().filter(|field| plain(field)) {
            let Some(span) = spans.get(&field.name).cloned() else {
                continue;
            };
            let at = format!("{name}.{}", field.name);
            let own = &field.name;
            let wire = spelled(field);
            if built_on(field, &[Prim::Int]) && time_named(own) {
                found.push(
                    Diagnostic::new(
                        &rule::PACKAGE_STANDARD_CANDIDATE,
                        format!("`{at}` may be an `Instant` sent as seconds"),
                    )
                    .at(span, "a whole number named like a time")
                    .reason(format!(
                        "`{own}` is a whole number named like a time, which providers usually send as seconds since 1970. A package's public types use the standard types, so an app reads an `Instant` rather than a number to convert, and the package decodes it at the edge."
                    ))
                    .next(format!(
                        "Check the provider's documentation, then make it `{own}: Instant {{ wire: Wire.unix_seconds }}`, or `Wire.unix_milliseconds` for milliseconds"
                    )),
                );
                continue;
            }
            if built_on(field, &[Prim::Int])
                && amount_like(own)
                && let Some((currency, own_field)) = &currency
            {
                let removing = if *own_field {
                    format!(", and remove the field sent as `{currency}`, which it then reads")
                } else {
                    String::new()
                };
                found.push(
                    Diagnostic::new(
                        &rule::PACKAGE_STANDARD_CANDIDATE,
                        format!("`{at}` may be `Money` in minor units"),
                    )
                    .at(span, "a whole number named like an amount")
                    .reason(format!(
                        "`{own}` is a whole number named like an amount beside a currency, `{currency}`, which providers usually send as minor units, pence or cents. A package's public types use the standard types, so an app reads `Money` rather than a number and a code it has to combine."
                    ))
                    .next(format!(
                        "Check the provider's documentation, then make it `{own}: Money {{ wire_parts: {{ amount: \"{wire}\" currency: \"{currency}\" }} }}`{removing}"
                    )),
                );
                continue;
            }
            if built_on(field, &[Prim::Int, Prim::Decimal])
                && latitude_named(own)
                && let Some(longitude) = longitude
            {
                let other = &longitude.name;
                let other_wire = spelled(longitude);
                found.push(
                    Diagnostic::new(
                        &rule::PACKAGE_STANDARD_CANDIDATE,
                        format!("`{at}` and `{name}.{other}` may be a `Point`"),
                    )
                    .at(span, "a latitude beside a longitude")
                    .reason(format!(
                        "`{own}` and `{other}` are a latitude and a longitude, which are the standard `Point`. A package's public types use the standard types, so an app reads one `Point` rather than two numbers whose order it has to know."
                    ))
                    .next(format!(
                        "Check the provider's documentation, then replace both with `location: Point {{ wire_parts: {{ latitude: \"{wire}\" longitude: \"{other_wire}\" }} }}`"
                    )),
                );
            }
        }
        self.out.extend(found);
    }

    fn variants(&mut self, list: Option<ast::MemberList>, host: &str) {
        let mut seen: BTreeMap<String, Span> = BTreeMap::new();
        // Each variant's spelling on the wire, its name unless it gives
        // `wire_name:`, with the variant and where.
        let mut wires: BTreeMap<String, (String, Span)> = BTreeMap::new();
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
            // `Enum.all` lists an enum's variants (area 19, question 22).
            if text == "all" {
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_RESERVED_VARIANT_NAME,
                        format!("`all` can't be a variant of `{host}`"),
                    )
                    .at(span.clone(), "reserved")
                    .reason(format!(
                        "`{host}.all` lists every variant of `{host}`, so no variant takes the name `all`."
                    ))
                    .next("Rename the variant"),
                );
            }
            seen.insert(text.clone(), span.clone());
            // A variant's spelling on the wire (area 11): `after_noon: {
            // wire_name: "after-noon" }`, never one another variant is
            // sent as.
            let wire = block.as_ref().and_then(|block| {
                tree::members(Some(block.clone()))
                    .into_iter()
                    .find(|(_, member)| shape::is_wire_name(member))
            });
            let mut sent = text.clone();
            if let Some((key, member)) = &wire {
                let value = member
                    .value()
                    .as_ref()
                    .map_or(Const::Other, Const::from_expr);
                let value_span = member
                    .value()
                    .map_or_else(|| self.span(member.syntax()), |v| self.span(v.syntax()));
                let key_span = self.span(key.syntax());
                self.wire_name(&value, key_span, value_span.clone(), &text, "variant");
                if let Const::Str(spelling) = value {
                    sent = spelling;
                }
                if let Some((other, first)) = wires.get(&sent) {
                    self.out.push(wire_clash(
                        host,
                        &text,
                        other,
                        &sent,
                        value_span,
                        first.clone(),
                    ));
                    continue;
                }
            } else if let Some((other, first)) = wires.get(&sent) {
                self.out.push(wire_clash(
                    host,
                    &text,
                    other,
                    &sent,
                    span.clone(),
                    first.clone(),
                ));
                continue;
            }
            wires.insert(sent, (text.clone(), span));
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
        self.wire_clashes(&items, lines, host);
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
                    if (self.failure || matches!(base, FieldsBase::Input | FieldsBase::Output))
                        && holds_password_hash(self.table, &declared, &mut Vec::new())
                    {
                        self.password_hash_field(base, host, name, &value);
                        continue;
                    }
                    // Only the runtime builds a batch type, so no request
                    // carries one (area 12; chunk 6.6b).
                    if base == FieldsBase::Input
                        && let Some(batch) = holds_batch(self.table, &declared, &mut Vec::new())
                    {
                        let span = self.span(value.syntax());
                        self.out
                            .push(batch_input(&format!("`{host}.{name}`"), batch, span));
                        continue;
                    }
                    // A secret value never leaves the server, nor is kept
                    // in an event (area 19, chunk 5.5r2).
                    if super::secrets::refuses(base, self.failure)
                        && let Some(secret) =
                            super::secrets::held_secret(self.table, &declared, &mut Vec::new())
                    {
                        let span = self.span(value.syntax());
                        self.out.push(super::secrets::field(
                            base,
                            self.failure,
                            host,
                            name,
                            secret,
                            span,
                        ));
                        continue;
                    }
                    // A sign-in code is sent only in its email or by the
                    // application's own sending, and a link's token comes
                    // only from a request (area 8).
                    if let Some(held) =
                        super::secrets::held_sign_in(self.table, &declared, &mut Vec::new())
                        && super::secrets::sign_in_refused(base, self.failure, held)
                    {
                        let span = self.span(value.syntax());
                        self.out.push(super::secrets::sign_in_code_field(
                            base,
                            self.failure,
                            host,
                            name,
                            held,
                            span,
                        ));
                        continue;
                    }
                    let field = FieldCtx {
                        input: base == FieldsBase::Input,
                        output: base == FieldsBase::Output,
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
        // An email's `pick:` copies from the built-in `MagicLinkEmail` too
        // (area 16, "Built-in emails", level 4), so what it names is
        // checked as an object's is; nothing else copies from it, since
        // its link and code go only into the sign-in email.
        let email = base == FieldsBase::EmailMessage;
        if !email {
            for (index, line) in lines.iter().enumerate() {
                let names: Vec<String> = match line {
                    Line::Pick(paths) => paths
                        .iter()
                        .filter_map(|path| path.first().cloned())
                        .collect(),
                    Line::Include(types) => types
                        .iter()
                        .filter_map(|ty| match ty {
                            TypeRef::Path(names, _) => names.first().cloned(),
                            _ => None,
                        })
                        .collect(),
                    _ => continue,
                };
                if !names
                    .iter()
                    .any(|name| super::table::builtin_fields(name).is_some())
                {
                    continue;
                }
                let Some(member) = member(index) else {
                    continue;
                };
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_NOT_RECORD,
                        format!(
                            "`{host}` can't copy from `{}`",
                            jadpo_std::email::MAGIC_LINK_EMAIL_TYPE
                        ),
                    )
                    .at(self.span(member.syntax()), "the sign-in email's fields")
                    .reason("`MagicLinkEmail`'s fields, a sign-in link and its code, go only into the application's own sign-in email, `type <Name> = EmailMessage { pick: [...] }` named by `magic_link: { email: }`, which Jadpo fills as it sends it.")
                    .next("Remove the copy, or make this the sign-in email"),
                );
            }
        }
        let partial = base == FieldsBase::EmailPartial;
        if !area_two && base != FieldsBase::Object && !email && !partial {
            return;
        }
        let table = self.table;
        let (_, problems) = derive(base, lines, &mut |name| match table.fields(name) {
            Some(fields) => Found::Fields(fields.iter().map(|f| f.name.clone()).collect()),
            None if let Some(fields) = super::table::builtin_fields(name) => {
                Found::Fields(fields.iter().map(|f| f.name.clone()).collect())
            }
            None if table.contains(name) || StdNames::get().is_type(name) => Found::NotFields,
            None => Found::Unknown,
        });
        for problem in problems {
            if (email || partial)
                && !matches!(
                    problem.kind,
                    LineKind::UnknownField { .. }
                        | LineKind::NotFields(_)
                        | LineKind::NotAFieldPath
                )
                && !(partial && matches!(problem.kind, LineKind::Duplicate(_)))
            {
                continue;
            }
            if let Some(this) = member(problem.line) {
                let first = match &problem.kind {
                    LineKind::Duplicate(name) => lines
                        .iter()
                        .position(|line| matches!(line, Line::Field { name: n, .. } if n == name))
                        // An email's own `to:` or `locale:` is written
                        // once: the first is the one every email has.
                        .filter(|at| *at != problem.line)
                        .and_then(member)
                        .and_then(|first| first.member_key())
                        .map(|key| self.span(key.syntax())),
                    _ => None,
                };
                self.line_problem(&problem, &this, host, first, base);
            }
        }
    }

    /// A map at a boundary is a JSON object, whose keys are text, so its
    /// key type is built on `Text` or `Uuid` (area 2, question 5).
    fn boundary_map_key(&mut self, declared: &Ty, node: &TypeNode, host: &str) {
        let mut current = declared.clone();
        for _ in 0..16 {
            // A standard named collection is sent as its contract's JSON,
            // whose keys its entry chose: a `LocalisedText`'s locales'
            // tags (area 19, chunk 5.5o2).
            if let Ty::Named(name) = current.strip_nullable()
                && jadpo_std::standard_type(name).is_some()
            {
                return;
            }
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
        base: FieldsBase,
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
            // Every partial has `locale:` already (area 16; chunk 7.3c).
            LineKind::Duplicate(name)
                if first.is_none()
                    && base == FieldsBase::EmailPartial
                    && super::table::PARTIAL_FIELDS
                        .iter()
                        .any(|(field, _)| field == name) =>
            {
                Diagnostic::new(
                    &rule::TYPE_DUPLICATE_FIELD,
                    format!("`{host}` declares `{name}`, which every partial has"),
                )
                .at(key_span, "already a field")
                .reason("Every `EmailPartial` has `locale:`, the locale of the template that includes it, which goes with it, before the fields it declares.")
                .next(format!("Remove `{name}`; `{{{{ include ... }}}}` gives it"))
            }
            // Every email has `to:` and `locale:` already (area 16).
            LineKind::Duplicate(name)
                if first.is_none()
                    && super::table::EMAIL_FIELDS
                        .iter()
                        .any(|(field, _)| field == name) =>
            {
                Diagnostic::new(
                    &rule::TYPE_DUPLICATE_FIELD,
                    format!("`{host}` declares `{name}`, which every email has"),
                )
                .at(key_span, "already a field")
                .reason("Every `EmailMessage` has `to:`, the address it is sent to, and `locale:`, the language it is written in, before the fields it declares.")
                .next(format!("Remove `{name}`; `emit {host} {{ ... }}` gives it"))
            }
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
        let mut written: BTreeMap<String, ast::Member> = BTreeMap::new();
        for (name, member) in tree::members(list) {
            let Some(key) = tree::name_text(&name) else {
                continue;
            };
            written.entry(key.clone()).or_insert_with(|| member.clone());
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
                // Read for what reaches a model (area 21; chunk 5.5v5b).
                if let Some(problem) = crate::ai_personal::mark_problem(&key, &value, value_span) {
                    self.out.push(problem);
                }
                continue;
            }
            if let Some(field) = field
                && key == shape::WIRE_NAME
            {
                let name = match &field.ty {
                    Ty::Field(_, path) => path.last().cloned().unwrap_or_default(),
                    _ => String::new(),
                };
                self.wire_name(&value, key_span, value_span, &name, "field");
                continue;
            }
            // How a provider package's field is sent (area 11, "Decoding";
            // chunk 7.2c2): checked once the block is read, as a whole.
            if field.is_some()
                && [shape::WIRE, shape::DECODE, shape::ENCODE, shape::WIRE_PARTS]
                    .contains(&key.as_str())
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
                        Subject::ExchangeRate => "an `ExchangeRate` type".to_string(),
                        Subject::Vector => "a `Vector` type".to_string(),
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
                Err(Problem::UnitRequired) => {
                    if let Subject::Prim(Prim::Quantity(kind)) = subject {
                        let written = value_node
                            .as_ref()
                            .map_or_else(String::new, |node| crate::tree::written(node.syntax()));
                        self.out.push(super::literal::unit_required(
                            kind,
                            &written,
                            &format!("{key}:"),
                            value_span,
                        ));
                    }
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
        // A vector type's one length against its own bounds (chunk 5.5s2).
        self.contradictions(
            &seen,
            "min_length",
            "dimensions",
            rules.min_length.zip(rules.dimensions).map(|(a, b)| a > b),
        );
        self.contradictions(
            &seen,
            "dimensions",
            "max_length",
            rules.dimensions.zip(rules.max_length).map(|(a, b)| a > b),
        );
        self.inherited(&seen, &rules, parent);
        self.fixed_currencies(&written, &rules, parent);
        if let Some(field) = field {
            self.coding(&written, field, &rules);
        }
        rules
    }

    /// A provider package's field's `wire:`, or its `decode:` and
    /// `encode:` (area 11, "Decoding"; chunk 7.2c2): only in a provider
    /// package, one way or the other, `wire:` naming a built-in encoding
    /// that holds the field's type, and `decode:` and `encode:` each one of
    /// the package's functions, `library.function`, given together. Whether
    /// those functions take and give the right types is checked with the
    /// package's operations (`crate::providers`).
    fn coding(&mut self, written: &BTreeMap<String, ast::Member>, field: &FieldCtx, own: &Rules) {
        let given: Vec<(&str, &ast::Member)> =
            [shape::WIRE, shape::DECODE, shape::ENCODE, shape::WIRE_PARTS]
                .into_iter()
                .filter_map(|key| written.get(key).map(|member| (key, member)))
                .collect();
        if given.is_empty() {
            return;
        }
        let name = match &field.ty {
            Ty::Field(_, path) => path.last().cloned().unwrap_or_default(),
            _ => String::new(),
        };
        let key_span = |this: &Self, member: &ast::Member| {
            member
                .member_key()
                .map_or_else(|| this.span(member.syntax()), |key| this.span(key.syntax()))
        };
        let value_span = |this: &Self, member: &ast::Member| {
            member
                .value()
                .map_or_else(|| this.span(member.syntax()), |v| this.span(v.syntax()))
        };
        let invalid = |message: String| Diagnostic::new(&rule::PACKAGE_WIRE_INVALID, message);
        if !self.provider {
            for (key, member) in given {
                let span = key_span(self, member);
                self.out.push(
                    invalid(format!("`{key}` is for a provider package's fields"))
                        .at(span, "outside a provider package")
                        .reason(format!(
                            "`{key}:` says how a provider sends a field, so only a provider package's types say it; the app's own types are sent as its contract writes them."
                        ))
                        .next(format!("Remove `{key}:`")),
                );
            }
            return;
        }
        let wire = written.get(shape::WIRE);
        let decode = written.get(shape::DECODE);
        let encode = written.get(shape::ENCODE);
        if let Some(parts) = written.get(shape::WIRE_PARTS) {
            if let Some((other, member)) = [
                (shape::WIRE, wire),
                (shape::DECODE, decode),
                (shape::ENCODE, encode),
            ]
            .into_iter()
            .find_map(|(key, member)| member.map(|member| (key, member)))
            {
                let span = key_span(self, member);
                self.out.push(
                    invalid(format!("`{name}` gives both `wire_parts:` and `{other}:`"))
                        .at(span, "a second way to send it")
                        .also(key_span(self, parts), "the first")
                        .reason("A field is sent one way: as its standard type's parts among its object's members with `wire_parts:`, in a built-in encoding with `wire:`, or by the package's own `decode:` and `encode:` functions.")
                        .next(format!("Keep `wire_parts:` or `{other}:`, and remove the other")),
                );
                return;
            }
            self.wire_parts(parts, &name, field);
            return;
        }
        if let (Some(wire), Some(other)) = (wire, decode.or(encode)) {
            let which = if decode.is_some() { "decode" } else { "encode" };
            let span = key_span(self, other);
            self.out.push(
                invalid(format!("`{name}` gives both `wire:` and `{which}:`"))
                    .at(span, "a second way to send it")
                    .also(key_span(self, wire), "the first")
                    .reason("A field is sent one way: in one of the built-in encodings its `wire:` names, or by the package's own `decode:` and `encode:` functions.")
                    .next("Keep `wire:`, or `decode:` and `encode:`, and remove the other"),
            );
            return;
        }
        if let Some(wire) = wire {
            self.wire_encoding(wire, &name, field, own, value_span(self, wire));
            return;
        }
        for (key, member) in [(shape::DECODE, decode), (shape::ENCODE, encode)] {
            let Some(member) = member else {
                continue;
            };
            let value = member
                .value()
                .as_ref()
                .map_or(Const::Other, Const::from_expr);
            if !matches!(&value, Const::Path(names) if names.len() == 2) {
                let span = value_span(self, member);
                self.out.push(
                    invalid(format!("`{key}` names one of the package's functions"))
                        .at(span, format!("{} here", value.describe()))
                        .reason(format!(
                            "`{key}:` is a function in the package's own `libraries/`, written `library.function`, that the runtime runs to {} the field.",
                            if key == shape::DECODE { "read" } else { "write" }
                        ))
                        .next(format!("Write `{key}: <library>.<function>`")),
                );
            }
        }
        let (missing, present) = match (decode, encode) {
            (Some(decode), None) => ("encode", decode),
            (None, Some(encode)) => ("decode", encode),
            _ => return,
        };
        let span = key_span(self, present);
        let declared = field.declared.to_string();
        let given = if missing == "encode" {
            "decode"
        } else {
            "encode"
        };
        self.out.push(
            invalid(format!("`{name}` gives `{given}:` without `{missing}:`"))
                .at(span, format!("no `{missing}:`"))
                .reason("A field the package's own function decodes is encoded by its counterpart, so every value can be sent as well as read, and the two are checked to give back what they were given.")
                .next(if missing == "encode" {
                    format!("Add `encode: <library>.<function>`, turning a `{declared}` back into what the provider sends")
                } else {
                    format!("Add `decode: <library>.<function>`, turning what the provider sends into a `{declared}`")
                }),
        );
    }

    /// The standard type `ty` is built on, by its name: `Money`, `Point`.
    fn standard_of(&self, ty: &Ty) -> Option<String> {
        let ty = ty.strip_nullable();
        if let Some(entry) = self
            .table
            .base_prim(ty)
            .and_then(jadpo_std::standard::of_builtin)
        {
            return Some(entry.name.to_string());
        }
        match self.table.structure(ty) {
            Ty::Opaque(name, args) if args.is_empty() => Some(name),
            _ => None,
        }
    }

    /// `wire_parts: { part: "spelling" }` (chunk 7.2e): the field is a
    /// standard type made of parts, and the block spells each of them, as
    /// text, once.
    fn wire_parts(&mut self, member: &ast::Member, name: &str, field: &FieldCtx) {
        let invalid = |message: String| Diagnostic::new(&rule::PACKAGE_WIRE_INVALID, message);
        let key = member
            .member_key()
            .map_or_else(|| self.span(member.syntax()), |key| self.span(key.syntax()));
        let shown = field.declared.to_string();
        let standard = self.standard_of(&field.declared);
        let Some((standard, wanted)) = standard
            .as_deref()
            .and_then(|standard| Some((standard, jadpo_std::provider::wire_parts(standard)?)))
        else {
            self.out.push(
                invalid(format!(
                    "`wire_parts` is for a standard type made of parts, and `{name}` is a `{shown}`"
                ))
                .at(key, format!("a `{shown}` field"))
                .reason("`wire_parts:` reads a standard type made of parts, such as `Money` or `Point`, from the members of the object that holds the field, each part under its own spelling.")
                .next("Remove `wire_parts:`, or give the field the standard type the provider's members make"),
            );
            return;
        };
        let Some(block) = member.block() else {
            self.out.push(
                invalid(format!("`wire_parts` takes a block of `{standard}`'s parts"))
                    .at(key, "no block")
                    .reason(format!(
                        "`wire_parts:` spells each of `{standard}`'s parts ({}) as the provider's member that holds it.",
                        crate::and_list(&wanted.iter().map(|part| format!("`{part}`")).collect::<Vec<_>>())
                    ))
                    .next(format!(
                        "Write `wire_parts: {{ {} }}`",
                        wanted
                            .iter()
                            .map(|part| format!("{part}: \"<spelling>\""))
                            .collect::<Vec<_>>()
                            .join(" ")
                    )),
            );
            return;
        };
        let mut given: Vec<String> = Vec::new();
        for (part_key, part) in tree::members(Some(block)) {
            let Some(part_name) = tree::name_text(&part_key) else {
                continue;
            };
            let part_span = self.span(part_key.syntax());
            if !wanted.contains(&part_name.as_str()) {
                let near = nearest(&part_name, wanted.iter().copied());
                let mut problem = invalid(format!("`{standard}` has no part `{part_name}`"))
                    .at(part_span.clone(), "no such part")
                    .reason(format!(
                        "`{standard}`'s parts are {}.",
                        crate::and_list(
                            &wanted
                                .iter()
                                .map(|part| format!("`{part}`"))
                                .collect::<Vec<_>>()
                        )
                    ));
                problem = match near {
                    Some(near) => problem
                        .next(format!("Use `{near}` instead"))
                        .edit(part_span, near.to_string()),
                    None => problem.next(format!("Remove `{part_name}`")),
                };
                self.out.push(problem);
                continue;
            }
            if given.contains(&part_name) {
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_CONSTRAINT_REPEATED,
                        format!("`{part_name}` is given twice"),
                    )
                    .at(part_span, "given again")
                    .reason("Each part is spelled once, so there is never a question of which member it is read from.")
                    .next(format!("Remove one `{part_name}`")),
                );
                continue;
            }
            given.push(part_name.clone());
            let value = part.value().as_ref().map_or(Const::Other, Const::from_expr);
            let value_span = part
                .value()
                .map_or_else(|| self.span(part.syntax()), |v| self.span(v.syntax()));
            if !matches!(&value, Const::Str(text) if !text.is_empty()) {
                self.out.push(
                    invalid("`wire_parts` takes each part's spelling as text".to_string())
                        .at(value_span, format!("{} here", value.describe()))
                        .reason("Each part is read from the member of the provider's object that its spelling names, in quotes, as the provider's documentation writes it.")
                        .next(format!("Write `{part_name}: \"<the provider's spelling>\"`")),
                );
            }
        }
        let missing: Vec<&str> = wanted
            .iter()
            .copied()
            .filter(|part| !given.iter().any(|given| given == part))
            .collect();
        if !missing.is_empty() {
            let listed: Vec<String> = missing.iter().map(|part| format!("`{part}`")).collect();
            self.out.push(
                invalid(format!(
                    "`{name}`'s `wire_parts` leaves out {}",
                    crate::and_list(&listed)
                ))
                .at(key, format!("no {}", crate::and_list(&listed)))
                .reason(format!(
                    "A `{standard}` is read from all its parts, so `wire_parts:` spells each of them."
                ))
                .next(format!(
                    "Add {}",
                    crate::and_list(
                        &missing
                            .iter()
                            .map(|part| format!("`{part}: \"<spelling>\"`"))
                            .collect::<Vec<_>>()
                    )
                )),
            );
        }
    }

    /// `wire: Wire.<name>`: one of the built-in encodings, holding the
    /// field's type.
    fn wire_encoding(
        &mut self,
        member: &ast::Member,
        name: &str,
        field: &FieldCtx,
        own: &Rules,
        span: Span,
    ) {
        use jadpo_std::provider::WireEncoding;
        let invalid = |message: String| Diagnostic::new(&rule::PACKAGE_WIRE_INVALID, message);
        let listed = || {
            crate::and_list(
                &WireEncoding::ALL
                    .iter()
                    .map(|encoding| format!("`Wire.{}`", encoding.name()))
                    .collect::<Vec<_>>(),
            )
        };
        let value = member
            .value()
            .as_ref()
            .map_or(Const::Other, Const::from_expr);
        let written = match &value {
            Const::Path(names) if names.len() == 2 && names[0] == "Wire" => names[1].clone(),
            other => {
                self.out.push(
                    invalid("`wire` takes one of the built-in encodings".to_string())
                        .at(span, format!("{} here", other.describe()))
                        .reason(format!(
                            "`wire:` names how the provider sends the field: {}.",
                            listed()
                        ))
                        .next("Write `wire: Wire.unix_seconds`, or the encoding the provider uses"),
                );
                return;
            }
        };
        let Some(encoding) = WireEncoding::from_name(&written) else {
            let near = nearest(
                &written,
                WireEncoding::ALL.iter().map(|encoding| encoding.name()),
            );
            let mut problem = invalid(format!("`Wire.{written}` isn't one of the built-in encodings"))
                .at(span.clone(), "no such encoding")
                .reason(format!(
                    "The built-in encodings are {}; anything else is decoded by the package's own `decode:` and `encode:` functions.",
                    listed()
                ));
            problem = match near {
                Some(near) => problem
                    .next(format!("Use `Wire.{near}` instead"))
                    .edit(span, format!("Wire.{near}")),
                None => problem.next(
                    "Name a built-in encoding, or decode the field with `decode:` and `encode:`",
                ),
            };
            self.out.push(problem);
            return;
        };
        let declared = field.declared.strip_nullable().clone();
        let prim = self.table.base_prim(&declared);
        let representation = prim.map(Prim::representation);
        let single = |this: &Self, item: &Ty| -> bool {
            match this.table.representation(item) {
                Some(Prim::Text | Prim::Int | Prim::Decimal) => true,
                _ => match item.strip_nullable() {
                    Ty::Named(name) => matches!(
                        this.table.def(name),
                        Some(Def::Enum { variants }) if variants.iter().all(|v| v.fields.is_none())
                    ),
                    _ => false,
                },
            }
        };
        let fits = match encoding {
            WireEncoding::UnixSeconds | WireEncoding::UnixMilliseconds | WireEncoding::Iso8601 => {
                prim == Some(Prim::Instant)
            }
            WireEncoding::DayMonthYear => prim == Some(Prim::CalendarDate),
            WireEncoding::Text | WireEncoding::Number => {
                matches!(representation, Some(Prim::Int | Prim::Decimal))
            }
            WireEncoding::CommaSeparated => match &declared {
                Ty::List(item) => single(self, item),
                _ => false,
            },
            WireEncoding::Base64 => representation == Some(Prim::Text),
            WireEncoding::JsonText => true,
            WireEncoding::MinorUnits => prim == Some(Prim::Money),
        };
        let shown = field.declared.to_string();
        if !fits {
            self.out.push(
                invalid(format!(
                    "`Wire.{written}` sends {}, and `{name}` is a `{shown}`",
                    encoding.holds()
                ))
                .at(span, format!("a `{shown}` field"))
                .reason(format!(
                    "`Wire.{written}` is how a provider sends {}; a field of another type is sent as the contract writes it, in another encoding, or by the package's own `decode:` and `encode:` functions.",
                    encoding.holds()
                ))
                .next(format!("Name the encoding the provider sends a `{shown}` in, or remove `wire:`")),
            );
            return;
        }
        // An amount sent as a number of minor units carries no currency,
        // so the field's type fixes the one it is in.
        let currency = own.currency.is_some()
            || self
                .table
                .rule_chain(&field.ty)
                .iter()
                .any(|(_, rules)| rules.currency.is_some());
        if encoding == WireEncoding::MinorUnits && !currency {
            self.out.push(
                invalid(format!("`Wire.minor_units` needs `{name}`'s one currency"))
                    .at(span, "no currency")
                    .reason("An amount sent as a whole number of minor units carries no currency, so the field's type fixes the one currency it is in with `currency:`.")
                    .next(format!("Add `currency: Currency.gbp` to `{name}`, or the currency the provider uses")),
            );
        }
    }

    /// A currency the type built on fixes already, given again as another
    /// one (area 19, chunk 5.5h4, decided by the lead session): a `Money`
    /// type's `currency:`, a rate type's `from:` and `to:`. A type built on
    /// another only narrows it, so it repeats the currency or leaves it
    /// out; another one leaves no value that follows both.
    fn fixed_currencies(
        &mut self,
        written: &BTreeMap<String, ast::Member>,
        own: &Rules,
        parent: &Ty,
    ) {
        let chain: Vec<(String, Rules)> = self
            .table
            .rule_chain(parent)
            .into_iter()
            .map(|(host, rules)| (host, rules.clone()))
            .collect();
        let pick = |rules: &Rules, key: &str| match key {
            "currency" => rules.currency,
            "from" => rules.rate_from,
            _ => rules.rate_to,
        };
        let shown = |code: u32| {
            format!(
                "{}.{}",
                jadpo_std::CURRENCY,
                jadpo_std::money::variant(code).unwrap_or("?")
            )
        };
        for key in ["currency", "from", "to"] {
            let Some(mine) = pick(own, key) else {
                continue;
            };
            let Some((host, theirs)) = chain
                .iter()
                .find_map(|(host, rules)| pick(rules, key).map(|code| (host, code)))
            else {
                continue;
            };
            let Some(member) = written.get(key) else {
                continue;
            };
            if mine == theirs {
                continue;
            }
            let code = jadpo_std::money::code(theirs).unwrap_or("?");
            let span = member
                .value()
                .map_or_else(|| self.span(member.syntax()), |v| self.span(v.syntax()));
            let what = match key {
                "currency" => format!("the currency at {code}"),
                "from" => format!("the currency its rates convert from at {code}"),
                _ => format!("the currency its rates convert to at {code}"),
            };
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_CONSTRAINT_CONTRADICTION,
                    format!(
                        "`{key}: {}` is different from `{host}`'s `{key}: {}`",
                        shown(mine),
                        shown(theirs)
                    ),
                )
                .at(span, format!("different from `{host}`'s `{key}: {}`", shown(theirs)))
                .reason(format!(
                    "`{host}` fixes {what}, and a type built on another only narrows it, so it repeats that currency or leaves it out; with another one no value could follow both, and the type could never be built."
                ))
                .next(format!("Remove `{key}:`, so the type keeps `{host}`'s {code}"))
                .edit(tree::removal(self.path, member), ""),
            );
        }
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
            // A vector type's one length, against the lengths and the one
            // length of the type it builds on (chunk 5.5s2).
            let dimensions = [
                (
                    "dimensions",
                    own.dimensions.zip(rules.dimensions).map(|(a, b)| a != b),
                    format!("dimensions: {}", rules.dimensions.unwrap_or_default()),
                    "different from",
                ),
                (
                    "dimensions",
                    own.dimensions.zip(rules.max_length).map(|(a, b)| a > b),
                    format!("max_length: {}", rules.max_length.unwrap_or_default()),
                    "above",
                ),
                (
                    "dimensions",
                    own.dimensions.zip(rules.min_length).map(|(a, b)| a < b),
                    format!("min_length: {}", rules.min_length.unwrap_or_default()),
                    "below",
                ),
                (
                    "min_length",
                    own.min_length.zip(rules.dimensions).map(|(a, b)| a > b),
                    format!("dimensions: {}", rules.dimensions.unwrap_or_default()),
                    "above",
                ),
                (
                    "max_length",
                    own.max_length.zip(rules.dimensions).map(|(a, b)| a < b),
                    format!("dimensions: {}", rules.dimensions.unwrap_or_default()),
                    "below",
                ),
                // How its vectors are compared, which a type built on it
                // keeps (area 21, chunk 5.5v1).
                (
                    "distance",
                    own.distance.zip(rules.distance).map(|(a, b)| a != b),
                    format!(
                        "distance: {}.{}",
                        jadpo_std::VectorDistance::NAME,
                        rules.distance.map_or("cosine", |d| d.name())
                    ),
                    "different from",
                ),
            ];
            if let Some((key, _, theirs, word)) = dimensions
                .into_iter()
                .find(|(_, broken, _, _)| *broken == Some(true))
            {
                found = Some((key, theirs, word.into(), host.clone()));
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
            Const::Path(path) => path.join("."),
            _ => String::new(),
        };
        let reason = if key == "distance" {
            format!(
                "`{host}` compares its vectors by `{}`: how vectors are compared belongs to the model that made them, so a type built on it keeps it, and its queries and index follow it.",
                theirs.trim_start_matches("distance: ")
            )
        } else if key == "scale" {
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
            } else if key == "distance" {
                format!("Remove `distance:`, or build on `Vector` instead of `{host}`")
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

    /// A field's or variant's `wire_name:` (area 11, decided 2026-10-20):
    /// only in a provider package, its spelling as text, and only where
    /// it differs from the name.
    fn wire_name(
        &mut self,
        value: &Const,
        key_span: Span,
        value_span: Span,
        name: &str,
        what: &str,
    ) {
        if !self.provider {
            self.out.push(
                Diagnostic::new(
                    &rule::PACKAGE_WIRE_NAME_INVALID,
                    format!("`wire_name` is for a provider package's {what}s"),
                )
                .at(key_span, "outside a provider package")
                .reason(format!(
                    "`wire_name:` gives a {what}'s spelling on a provider's wire, where it differs from its Jadpo name, so only a provider package's types say it; the app's own types are sent as its contract writes them."
                ))
                .next("Remove `wire_name:`"),
            );
            return;
        }
        match value {
            Const::Str(spelling) if spelling.is_empty() => self.out.push(
                Diagnostic::new(
                    &rule::PACKAGE_WIRE_NAME_INVALID,
                    format!("`{name}`'s `wire_name` is empty"),
                )
                .at(value_span, "no spelling")
                .reason(format!(
                    "`wire_name:` is the {what}'s spelling on the provider's wire, as its documentation writes it."
                ))
                .next("Write the provider's spelling, or remove `wire_name:`"),
            ),
            Const::Str(spelling) if spelling == name => self.out.push(
                Diagnostic::new(
                    &rule::PACKAGE_WIRE_NAME_INVALID,
                    format!("`{name}`'s `wire_name` is its own name"),
                )
                .at(value_span, "the same spelling")
                .reason(format!(
                    "A {what} is sent by its name; `wire_name:` is written only where the provider spells it differently."
                ))
                .next("Remove `wire_name:`"),
            ),
            Const::Str(_) => {}
            other => self.out.push(
                Diagnostic::new(
                    &rule::PACKAGE_WIRE_NAME_INVALID,
                    "`wire_name` takes the provider's spelling as text",
                )
                .at(value_span, format!("{} here", other.describe()))
                .reason(format!(
                    "`wire_name:` is the {what}'s spelling on the provider's wire, in quotes, as its documentation writes it."
                ))
                .next("Write the provider's spelling in quotes"),
            ),
        }
    }

    /// Two written fields of one type sent under one spelling: a field's
    /// `wire_name:` and another's name or `wire_name:` (area 11).
    fn wire_clashes(&mut self, items: &[MemberItem], lines: &[Line], host: &str) {
        let mut sent: BTreeMap<String, (String, Span, Option<String>)> = BTreeMap::new();
        for (index, line) in lines.iter().enumerate() {
            let Line::Field { name, settings, .. } = line else {
                continue;
            };
            let Some(MemberItem::Member(member)) = items.get(index) else {
                continue;
            };
            let wire = settings.iter().find_map(|(key, value)| match value {
                Const::Str(text) if key == shape::WIRE_NAME && !text.is_empty() => {
                    Some(text.clone())
                }
                _ => None,
            });
            let span = match (&wire, member.value()) {
                (Some(_), Some(Expr::RecordExpr(record))) => tree::members(record.member_list())
                    .into_iter()
                    .find(|(key, _)| tree::name_text(key).as_deref() == Some(shape::WIRE_NAME))
                    .and_then(|(_, setting)| setting.value())
                    .map_or_else(|| self.span(member.syntax()), |v| self.span(v.syntax())),
                _ => member
                    .member_key()
                    .map_or_else(|| self.span(member.syntax()), |key| self.span(key.syntax())),
            };
            // A field read from its standard type's parts sends each part
            // under its spelling, and none of its own (chunk 7.2e); fields
            // may share one part's member, as a charge's amounts share its
            // currency.
            let parts = settings.iter().find_map(|(key, value)| match value {
                Const::Block(parts) if key == shape::WIRE_PARTS => Some(parts.clone()),
                _ => None,
            });
            let spellings: Vec<(String, Option<String>)> = match parts {
                Some(parts) => parts
                    .into_iter()
                    .filter_map(|(part, value)| match value {
                        Const::Str(text) if !text.is_empty() => Some((text, Some(part))),
                        _ => None,
                    })
                    .collect(),
                None => vec![(wire.unwrap_or_else(|| name.clone()), None)],
            };
            for (spelling, part) in spellings {
                match sent.get(&spelling) {
                    Some((_, _, Some(theirs))) if part.as_ref() == Some(theirs) => {}
                    Some((other, first, _)) => {
                        let problem =
                            wire_clash(host, name, other, &spelling, span.clone(), first.clone())
                                .reason(format!(
                                    "A provider's object has one member per spelling, so two fields of `{host}` can't both be `{spelling}` on the wire: one would be read into both."
                                ));
                        self.out.push(problem);
                    }
                    None => {
                        sent.insert(spelling, (name.clone(), span.clone(), part));
                    }
                }
            }
        }
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
        // The column a file helper reads the field from (area 12,
        // "Batches"; chunk 6.4b), or writes it to (chunk 6.4b2).
        let column = key == "column" && (field.input || field.output);
        if !field.input && !column {
            self.out.push(optional_outside(&host, key_span));
            return;
        }
        if key == "header" {
            self.header(value, value_span, field);
            return;
        }
        if column {
            if !matches!(value, Const::Str(name) if !name.is_empty()) {
                self.out.push(
                    Diagnostic::new(&rule::TYPE_SETTING_VALUE, "`column` takes the column's name")
                        .at(value_span, format!("{} here", value.describe()))
                        .reason("`column:` names the column of a CSV or TSV file a row's field is read from or written to, when it isn't the field's name, as text: `column: \"Email address\"`.")
                        .next("Write the column's name in quotes"),
                );
            }
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
    /// An `Output`'s field, whose `column:` a file helper writes it to
    /// (chunk 6.4b2).
    output: bool,
    /// Whether later areas add settings of their own to this field.
    later: bool,
    /// The field type, `Type.field`.
    ty: Ty,
    declared: Ty,
}

/// Settings later areas give an entity's or event's fields: data (area 5),
/// policy (area 9), personal data (area 15, and area 21's `to_models:`)
/// and a `File` field's store, size and types (area 16). They are checked
/// when those areas are built; `personal:` and `to_models:` are read since
/// chunk 5.5v5b.
const LATER_FIELD_SETTINGS: [&str; 13] = [
    "erase",
    "generated",
    "immutable",
    "max_size",
    "on_delete",
    "personal",
    "policy",
    "retention",
    "role",
    "store",
    "to_models",
    "types",
    "unique",
];

/// Whether a type is a caller, or holds one (area 8): in a list, a map or
/// an option, or in a field of an object or a variant's data, however
/// deep. An input, output or failure holding one is reported at its own
/// declaration, so it isn't followed again.
pub(crate) fn holds_principal(table: &dyn TypeView, ty: &Ty, seen: &mut Vec<String>) -> bool {
    match ty {
        // A feature's properties (area 20, chunk 5.5u7).
        Ty::Opaque(name, args) if super::is_feature_type(name) => {
            args.iter().any(|arg| holds_principal(table, arg, seen))
        }
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

/// Whether a type is a `PasswordHash`, or holds one, however deep (area
/// 19): in a list, a map or an option, or in a field of an object or a
/// variant's data. An input, output or failure holding one is reported at
/// its own declaration, so it isn't followed again.
pub(crate) fn holds_password_hash(table: &dyn TypeView, ty: &Ty, seen: &mut Vec<String>) -> bool {
    match ty {
        // A feature's properties (area 20, chunk 5.5u7).
        Ty::Opaque(name, args) if super::is_feature_type(name) => {
            args.iter().any(|arg| holds_password_hash(table, arg, seen))
        }
        Ty::List(inner) | Ty::Set(inner) | Ty::Nullable(inner) => {
            holds_password_hash(table, inner, seen)
        }
        Ty::Map(key, value) => {
            holds_password_hash(table, key, seen) || holds_password_hash(table, value, seen)
        }
        Ty::Named(name) => {
            if name == jadpo_std::standard::security::PASSWORD_HASH {
                return true;
            }
            if seen.contains(name) {
                return false;
            }
            seen.push(name.clone());
            match table.def(name) {
                Some(super::table::Def::Scalar { parent, .. }) => {
                    holds_password_hash(table, parent, seen)
                }
                Some(super::table::Def::Fields { base, fields, .. })
                    if !matches!(
                        base,
                        FieldsBase::Input | FieldsBase::Output | FieldsBase::Failure
                    ) =>
                {
                    fields
                        .iter()
                        .any(|field| holds_password_hash(table, &field.declared, seen))
                }
                Some(super::table::Def::Enum { variants }) => variants
                    .iter()
                    .flat_map(|variant| variant.fields.iter().flatten())
                    .any(|field| holds_password_hash(table, &field.declared, seen)),
                _ => false,
            }
        }
        _ => false,
    }
}

/// The batch type (area 12, "Batches: work over many items") a type is,
/// or holds, however deep: in a list, a map or an option, built on one,
/// in a field of an object or a variant's data, or a field type naming a
/// field that holds one (`Sweep.run`). An input holding one is reported at
/// its own declaration, so it isn't followed again.
pub(crate) fn holds_batch(
    table: &dyn TypeView,
    ty: &Ty,
    seen: &mut Vec<String>,
) -> Option<&'static str> {
    match ty {
        Ty::Opaque(name, args) => jadpo_std::batch::OBJECTS
            .into_iter()
            .find(|batch| batch == name)
            .or_else(|| args.iter().find_map(|arg| holds_batch(table, arg, seen))),
        Ty::List(inner) | Ty::Set(inner) | Ty::Nullable(inner) => holds_batch(table, inner, seen),
        Ty::Map(key, value) => {
            holds_batch(table, key, seen).or_else(|| holds_batch(table, value, seen))
        }
        Ty::Field(host, path) => {
            let at = format!("{host}.{}", path.join("."));
            if seen.contains(&at) {
                return None;
            }
            seen.push(at);
            let declared = table.field(host, path)?.declared.clone();
            holds_batch(table, &declared, seen)
        }
        Ty::Named(name) => {
            if seen.contains(name) {
                return None;
            }
            seen.push(name.clone());
            match table.def(name) {
                Some(super::table::Def::Scalar { parent, .. }) => holds_batch(table, parent, seen),
                Some(super::table::Def::Fields { base, fields, .. })
                    if *base != FieldsBase::Input =>
                {
                    fields
                        .iter()
                        .find_map(|field| holds_batch(table, &field.declared, seen))
                }
                Some(super::table::Def::Enum { variants }) => variants
                    .iter()
                    .flat_map(|variant| variant.fields.iter().flatten())
                    .find_map(|field| holds_batch(table, &field.declared, seen)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// `type.batch_input`: `what`, an `Input`'s field or a route's
/// placeholder, at `span`, holds the batch type `batch`, which only the
/// runtime builds (area 2), so a request that named one could read or
/// cancel any run (chunk 6.6b).
pub(crate) fn batch_input(what: &str, batch: &str, span: Span) -> Diagnostic {
    let problem = Diagnostic::new(
        &rule::TYPE_BATCH_INPUT,
        format!("{what} holds a `{batch}`, which no request carries"),
    )
    .at(span, format!("a `{batch}`"));
    if batch == jadpo_std::batch::BATCH_RUN {
        problem
            .reason("Only the runtime builds a `BatchRun`: a run comes from `batches.each`, or from a field the caller may read where the application kept it, and `batches.status` and `batches.cancel` find it by its `id` alone. A request that named one could read or stop any run.")
            .next("Take the `id` of the record that keeps the run, and read the `BatchRun` from its field")
    } else {
        problem
            .reason(format!(
                "Only the runtime builds a `{batch}`, from a run it keeps, so one a request sent would be a run's progress or problems made up by whoever sent it."
            ))
            .next("Take what the request says in fields of the application's own types")
    }
}

impl Decls<'_> {
    /// A field holding a password's hash where none may go (area 19,
    /// decided by the lead session for chunk 5.5q2): a hash is stored and
    /// checked, never sent to a client, read from a request or put in a
    /// failure, which is logged and whose client fields reach the client.
    fn password_hash_field(&mut self, base: FieldsBase, host: &str, name: &str, value: &Expr) {
        let span = self.span(value.syntax());
        let (label, why, next) = if self.failure {
            (
                "in a failure",
                "A failure is logged and its client fields reach the client, so a password's hash in one would leave the server.",
                format!("Remove `{name}`; the record the hash belongs to is named by its id"),
            )
        } else if base == FieldsBase::Input {
            (
                "read from a request",
                "A hash comes only from `crypto.hash_password`, so a request sends the password, which the action hashes; a hash a client chose would let it set any password without knowing it.",
                format!(
                    "Take the password in `{name}`'s place as text, and store `crypto.hash_password(...)` of it"
                ),
            )
        } else {
            (
                "sent to a client",
                "A client never needs a password's hash, and anyone who gets one can guess passwords against it offline, at their own pace.",
                format!(
                    "Remove `{name}`, and send what the client needs instead, such as whether a password is set"
                ),
            )
        };
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_PASSWORD_HASH_EXPOSED,
                format!("`{host}.{name}` holds a password's hash"),
            )
            .at(span, label)
            .reason(format!(
                "A `PasswordHash` is stored and checked with `crypto.verify_password`, and never leaves the server. {why}"
            ))
            .next(next),
        );
    }

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

/// Whether `ty` is a list whose items are the standard type `item`, or an
/// `Object` of its shape (`LineString`'s points, chunk 5.5u6).
fn list_of(table: &dyn TypeView, ty: &Ty, item: &str) -> bool {
    let Ty::List(of) = table.structure(ty) else {
        return false;
    };
    if table.is_nullable(&of) {
        return false;
    }
    match table.structure(&of) {
        Ty::Opaque(name, args) => name == item && args.is_empty(),
        Ty::Named(name) => standard_match(table, &name, Some(item)).is_some(),
        _ => false,
    }
}

/// Every standard type with `found`'s shape, `found`'s own first: a list
/// of points is a `LineString`'s or a `Polygon`'s (chunk 5.5u6).
pub(crate) fn same_shape(found: &StandardMatch) -> Vec<&'static str> {
    let mut names = vec![found.standard.name];
    for entry in jadpo_std::STANDARD_TYPES {
        if entry.name != found.standard.name
            && let (Some(theirs), Some(ours)) = (entry.shape, found.standard.shape)
            && std::ptr::eq(theirs, ours)
        {
            names.push(entry.name);
        }
    }
    names
}

/// The standard type whose shape `name`'s fields are (area 19, question
/// 5): an `Object` whose fields are exactly a shape's, by their common
/// names in any order and what each is built on, none of them optional,
/// though the shape may let it leave out a field it marks optional;
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
        let required = shape.iter().filter(|wanted| !wanted.optional).count();
        if fields.len() < required || fields.len() > shape.len() {
            continue;
        }
        let mut matched: Vec<MatchedField> = Vec::new();
        let mut missing = false;
        for wanted in shape {
            let found = fields.iter().find(|field| {
                wanted.names.contains(&field.name.as_str())
                    && !table.is_nullable(&field.declared)
                    && (table
                        .base_prim(&field.declared)
                        .is_some_and(|prim| wanted.built_on.contains(&prim))
                        || (wanted.or_enum && is_enum(table, &field.declared))
                        || wanted
                            .list_of
                            .is_some_and(|item| list_of(table, &field.declared, item)))
                    && !matched.iter().any(|taken| taken.written == field.name)
            });
            match found {
                Some(field) => matched.push(MatchedField {
                    meaning: wanted.meaning,
                    written: field.name.clone(),
                    standard: wanted.names[0],
                }),
                // A colour's alpha may be left out (chunk 5.5p).
                None if wanted.optional => {}
                None => {
                    missing = true;
                    break;
                }
            }
        }
        if !missing && matched.len() == fields.len() {
            return Some(StandardMatch {
                standard: entry,
                fields: matched,
            });
        }
    }
    None
}

/// Two members of `host` sent under one spelling (area 11, "Decoding").
fn wire_clash(
    host: &str,
    name: &str,
    other: &str,
    spelling: &str,
    span: Span,
    first: Span,
) -> Diagnostic {
    Diagnostic::new(
        &rule::PACKAGE_WIRE_NAME_INVALID,
        format!("`{host}.{name}` and `{host}.{other}` are both `{spelling}` on the wire"),
    )
    .at(span, "sent as `".to_string() + spelling + "` again")
    .also(first, format!("`{other}` is sent as `{spelling}`"))
    .reason(format!(
        "A provider sends each of `{host}`'s spellings for one thing, so two can't share `{spelling}`: one would be read as both."
    ))
    .next(format!("Give `{name}` the provider's own spelling, or remove it"))
}
