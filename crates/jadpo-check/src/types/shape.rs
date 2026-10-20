//! Each file's type declarations and entities, lowered without positions:
//! a scalar's base and settings, an object's lines, an enum's variants. An
//! edit inside a function body leaves a file's shapes equal, so the type
//! table built from them is reused.

use jadpo_db::{Db, File};
use jadpo_syntax::ast::{self, Decl, Expr, MemberItem};

use super::rules::Const;
use super::ty::TypeRef;
use crate::case::starts_upper;
use crate::tree;

/// What a declaration with fields is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FieldsBase {
    Object,
    Input,
    Output,
    Event,
    EmailMessage,
    /// `type X = EmailPartial { }` (area 16): a piece of an email's
    /// templates, its fields what it is included with.
    EmailPartial,
    /// `type X = Message { }` (area 10): text for people, its fields the
    /// wording's placeholders.
    Message,
    /// `type X = Prompt { }` (area 21): what a model is asked, its fields
    /// the placeholders of its `.prompt` file.
    Prompt,
    Failure,
    Entity,
}

impl FieldsBase {
    pub fn from_base(name: &str) -> Option<FieldsBase> {
        Some(match name {
            "Object" => FieldsBase::Object,
            "Input" => FieldsBase::Input,
            "Output" => FieldsBase::Output,
            "Event" => FieldsBase::Event,
            "EmailMessage" => FieldsBase::EmailMessage,
            "EmailPartial" => FieldsBase::EmailPartial,
            "Message" => FieldsBase::Message,
            "Prompt" => FieldsBase::Prompt,
            "Failure" => FieldsBase::Failure,
            _ => return None,
        })
    }

    /// Whether area 2 decides all its lines, the copying ones included:
    /// the other bases' copying lines are their own areas'. Every base's
    /// fields follow area 2's settings.
    pub fn is_area_two(self) -> bool {
        matches!(
            self,
            FieldsBase::Object | FieldsBase::Input | FieldsBase::Output
        )
    }

    /// Whether later areas add settings to its fields beside area 2's
    /// (an entity's `on_delete:`, a personal-data mark): those are left to
    /// the areas that decide them.
    pub fn has_later_settings(self) -> bool {
        matches!(
            self,
            FieldsBase::Entity | FieldsBase::Event | FieldsBase::EmailMessage
        )
    }
}

/// A setting as written: its name and value.
pub type Setting = (String, Const);

/// One line of a body with fields, read top to bottom (area 2, "Building a
/// type from other types").
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Line {
    /// `name: Type`, with the field's settings block.
    Field {
        name: String,
        ty: TypeRef,
        settings: Vec<Setting>,
    },
    /// `include: [Todo]`
    Include(Vec<TypeRef>),
    /// `pick: [Todo.id, Todo.done]`; each entry as its names.
    Pick(Vec<Vec<String>>),
    /// `omit: [Todo.id]`
    Omit(Vec<Vec<String>>),
    /// `optional: all` (`None`) or `optional: [Todo.done]`.
    Optional(Option<Vec<Vec<String>>>),
    /// A copying line whose value isn't a list in brackets.
    NotAList(&'static str),
    /// `optional`, `include`, `pick` or `omit` used as a field's name.
    Reserved(String),
    /// `rule ends_after_start: ends > starts`: a rule across the type's
    /// fields (area 2, "Rules across fields").
    Rule(super::across::CrossDef),
    /// Not a field: an entity's setting or section, or something the
    /// parser has already reported.
    Other,
}

/// A variant and, when it carries data, its fields.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VariantShape {
    pub name: String,
    pub fields: Option<Vec<Line>>,
    /// Its spelling on a provider's wire, when its block gives
    /// `wire_name:` (area 11, decided 2026-10-20).
    pub wire_name: Option<String>,
}

/// The setting a provider package's field or enum variant gives its
/// spelling on the wire with (area 11, "Decoding"; decided 2026-10-20):
/// `collect_at: Instant { wire_name: "collectAt" }`, `after_noon: {
/// wire_name: "after-noon" }`.
pub const WIRE_NAME: &str = "wire_name";

/// The setting a provider package's webhook event names the type its
/// provider waits for in the response with (area 11, "Webhooks that need
/// a reply"; chunk 7.7b): `reply: CallReply`.
pub const REPLY: &str = "reply";

/// The settings a provider package's field says how it is sent with (area
/// 11, "Decoding"; chunk 7.2c2): one of the built-in encodings,
/// `wire: Wire.unix_seconds`, or the package's own pair of functions,
/// `decode: slots.parse_window` and `encode: slots.format_window`.
pub const WIRE: &str = "wire";
pub const DECODE: &str = "decode";
/// A provider package's field read from its standard type's parts among
/// its object's own members (chunk 7.2e): `wire_parts: { amount: "amount"
/// currency: "currency" }`.
pub const WIRE_PARTS: &str = jadpo_std::provider::WIRE_PARTS;
pub const ENCODE: &str = "encode";

/// Whether `member` of a variant's block is its `wire_name:`, a setting
/// rather than a field: the key `wire_name` with a value that isn't a
/// type.
pub fn is_wire_name(member: &ast::Member) -> bool {
    single_key(member).as_deref() == Some(WIRE_NAME)
        && !member.value().as_ref().is_some_and(looks_like_type)
}

/// A variant's block read as its settings and its fields: the spelling
/// its `wire_name:` gives, if text, and whether anything else is in it, so
/// a block holding only `wire_name:` is a variant without data.
pub fn variant_block(block: Option<&ast::MemberList>) -> (Option<String>, bool) {
    let Some(block) = block else {
        return (None, false);
    };
    let mut wire = None;
    let mut fields = false;
    for item in block.member_items() {
        match item {
            MemberItem::Member(member) if is_wire_name(&member) => {
                if let Some(Const::Str(text)) = member.value().as_ref().map(Const::from_expr) {
                    wire = Some(text);
                }
            }
            _ => fields = true,
        }
    }
    (wire, fields)
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ShapeKind {
    /// A refinement of a built-in or named type, with its settings.
    Scalar {
        parent: TypeRef,
        settings: Vec<Setting>,
    },
    Fields {
        base: FieldsBase,
        lines: Vec<Line>,
    },
    Enum {
        variants: Vec<VariantShape>,
    },
    /// A time format, `type ShortDate = TimeFormat { day: ... }`, with its
    /// components as written (area 10).
    Format {
        settings: Vec<Setting>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Shape {
    pub name: String,
    pub kind: ShapeKind,
}

impl TypeRef {
    /// The same type with each name it starts from read through `canonical`
    /// (area 1, "Files and names"): `Address` becomes `billing.Address`
    /// where that is a contested private name of the writer's domain.
    pub fn scoped(&self, canonical: &dyn Fn(&str) -> String) -> TypeRef {
        match self {
            TypeRef::Path(names, args) => {
                let mut names = names.clone();
                if let Some(first) = names.first_mut() {
                    *first = canonical(first);
                }
                TypeRef::Path(
                    names,
                    args.iter().map(|arg| arg.scoped(canonical)).collect(),
                )
            }
            TypeRef::Nullable(inner) => TypeRef::Nullable(Box::new(inner.scoped(canonical))),
            TypeRef::Invalid => TypeRef::Invalid,
        }
    }
}

fn scoped_path(path: &[String], canonical: &dyn Fn(&str) -> String) -> Vec<String> {
    let mut path = path.to_vec();
    if let Some(first) = path.first_mut() {
        *first = canonical(first);
    }
    path
}

fn scoped_const(value: &Const, canonical: &dyn Fn(&str) -> String) -> Const {
    match value {
        Const::Path(path) if path.len() > 1 => Const::Path(scoped_path(path, canonical)),
        Const::List(items) => Const::List(
            items
                .iter()
                .map(|item| scoped_const(item, canonical))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn scoped_settings(settings: &[Setting], canonical: &dyn Fn(&str) -> String) -> Vec<Setting> {
    settings
        .iter()
        .map(|(name, value)| (name.clone(), scoped_const(value, canonical)))
        .collect()
}

fn scoped_lines(lines: &[Line], canonical: &dyn Fn(&str) -> String) -> Vec<Line> {
    let paths = |paths: &[Vec<String>]| -> Vec<Vec<String>> {
        paths
            .iter()
            .map(|path| scoped_path(path, canonical))
            .collect()
    };
    lines
        .iter()
        .map(|line| match line {
            Line::Field { name, ty, settings } => Line::Field {
                name: name.clone(),
                ty: ty.scoped(canonical),
                settings: scoped_settings(settings, canonical),
            },
            Line::Include(types) => {
                Line::Include(types.iter().map(|ty| ty.scoped(canonical)).collect())
            }
            Line::Pick(found) => Line::Pick(paths(found)),
            Line::Omit(found) => Line::Omit(paths(found)),
            Line::Optional(found) => Line::Optional(found.as_deref().map(paths)),
            other => other.clone(),
        })
        .collect()
}

impl Shape {
    /// The shape as the type tables hold it: its name and every name it
    /// writes read through `canonical`, as its own domain's code reads them
    /// (area 1, "Files and names").
    pub fn scoped(&self, canonical: &dyn Fn(&str) -> String) -> Shape {
        let kind = match &self.kind {
            ShapeKind::Scalar { parent, settings } => ShapeKind::Scalar {
                parent: parent.scoped(canonical),
                settings: scoped_settings(settings, canonical),
            },
            ShapeKind::Fields { base, lines } => ShapeKind::Fields {
                base: *base,
                lines: scoped_lines(lines, canonical),
            },
            ShapeKind::Enum { variants } => ShapeKind::Enum {
                variants: variants
                    .iter()
                    .map(|variant| VariantShape {
                        name: variant.name.clone(),
                        fields: variant
                            .fields
                            .as_deref()
                            .map(|lines| scoped_lines(lines, canonical)),
                        wire_name: variant.wire_name.clone(),
                    })
                    .collect(),
            },
            ShapeKind::Format { settings } => ShapeKind::Format {
                settings: settings.clone(),
            },
        };
        Shape {
            name: canonical(&self.name),
            kind,
        }
    }
}

/// The shapes a file declares, in source order.
#[salsa::tracked(returns(ref))]
pub fn type_shapes(db: &dyn Db, file: File) -> Vec<Shape> {
    let Some(source) = tree::source(db, file) else {
        return Vec::new();
    };
    source.decls().filter_map(|decl| shape(&decl)).collect()
}

pub fn shape(decl: &Decl) -> Option<Shape> {
    match decl {
        Decl::TypeDecl(decl) => {
            let name = decl.name().as_ref().and_then(tree::name_text)?;
            let parent = decl
                .base()
                .map_or(TypeRef::Invalid, |base| TypeRef::from_type(&base));
            let list = decl.member_list();
            let kind = match &parent {
                TypeRef::Path(names, args) if names.len() == 1 && args.is_empty() => {
                    match (names[0].as_str(), FieldsBase::from_base(&names[0])) {
                        ("Enum", _) => ShapeKind::Enum {
                            variants: variants(list),
                        },
                        (super::ty::TIME_FORMAT, _) => ShapeKind::Format {
                            settings: settings(list),
                        },
                        (_, Some(FieldsBase::Failure)) => ShapeKind::Fields {
                            base: FieldsBase::Failure,
                            lines: failure_lines(list),
                        },
                        (_, Some(base)) => ShapeKind::Fields {
                            base,
                            lines: lines(list, base),
                        },
                        (_, None) => ShapeKind::Scalar {
                            parent,
                            settings: settings(list),
                        },
                    }
                }
                _ => ShapeKind::Scalar {
                    parent,
                    settings: settings(list),
                },
            };
            Some(Shape { name, kind })
        }
        Decl::EntityDecl(decl) => Some(Shape {
            name: decl.name().as_ref().and_then(tree::name_text)?,
            kind: ShapeKind::Fields {
                base: FieldsBase::Entity,
                lines: lines(decl.member_list(), FieldsBase::Entity),
            },
        }),
        _ => None,
    }
}

/// The members of a block, one line each, in order; anything that isn't a
/// `name: value` member is [`Line::Other`], so line numbers follow the
/// block's items.
pub fn lines(list: Option<ast::MemberList>, base: FieldsBase) -> Vec<Line> {
    list.into_iter()
        .flat_map(|list| list.member_items())
        .map(|item| match item {
            MemberItem::Member(member) => line(&member, base),
            MemberItem::RuleMember(rule) => {
                super::across::CrossDef::from_member(&rule).map_or(Line::Other, Line::Rule)
            }
            _ => Line::Other,
        })
        .collect()
}

/// The lines that build a type's fields from other types (area 2), read
/// from top to bottom.
pub const COPY_LINES: [&str; 4] = ["include", "pick", "omit", "optional"];

fn line(member: &ast::Member, base: FieldsBase) -> Line {
    let Some(key) = single_key(member) else {
        return Line::Other;
    };
    let value = member.value();
    if base == FieldsBase::Entity && crate::data::SECTIONS.contains(&key.as_str()) {
        // An entity's section, never a field (area 5).
        return Line::Other;
    }
    if base == FieldsBase::Event && key == REPLY {
        // An event's reply type, never a field (area 11, "Webhooks that
        // need a reply"; chunk 7.7b): a package's webhook event names it,
        // and any other event giving it is told so.
        return Line::Other;
    }
    let copying = COPY_LINES.contains(&key.as_str());
    if copying && base == FieldsBase::Entity {
        // An entity copies no fields (area 5), so a copying word with a
        // type is a field that took a reserved name.
        if value.as_ref().is_some_and(looks_like_type) {
            return Line::Reserved(key);
        }
        return Line::Other;
    }
    if copying {
        // `optional: all` is a line; `optional: Flag` is a field that took
        // a reserved name. `pick: Todo.id` is a line missing its brackets.
        if key == "optional" && value.as_ref().is_some_and(looks_like_type) {
            return Line::Reserved(key);
        }
        return match (key.as_str(), value) {
            ("optional", Some(Expr::NameRef(name)))
                if tree::name_ref_text(&name).as_deref() == Some("all") =>
            {
                Line::Optional(None)
            }
            ("optional", Some(Expr::ListExpr(list))) => Line::Optional(Some(paths(&list))),
            ("include", Some(Expr::ListExpr(list))) => {
                Line::Include(list.exprs().map(|e| TypeRef::from_expr(&e)).collect())
            }
            ("pick", Some(Expr::ListExpr(list))) => Line::Pick(paths(&list)),
            ("omit", Some(Expr::ListExpr(list))) => Line::Omit(paths(&list)),
            ("include", _) => Line::NotAList("include"),
            ("pick", _) => Line::NotAList("pick"),
            ("omit", _) => Line::NotAList("omit"),
            _ => Line::NotAList("optional"),
        };
    }
    match value {
        Some(value) if looks_like_type(&value) => Line::Field {
            name: key,
            ty: TypeRef::from_expr(&value),
            settings: match &value {
                Expr::RecordExpr(record) => field_settings(record.member_list()),
                _ => Vec::new(),
            },
        },
        _ => Line::Other,
    }
}

/// A failure's fields, from its `client:` and `internal:` blocks (area 4).
fn failure_lines(list: Option<ast::MemberList>) -> Vec<Line> {
    tree::members(list)
        .into_iter()
        .filter(|(name, _)| {
            matches!(
                tree::name_text(name).as_deref(),
                Some("client" | "internal")
            )
        })
        .flat_map(|(_, member)| lines(member.block(), FieldsBase::Failure))
        .collect()
}

fn variants(list: Option<ast::MemberList>) -> Vec<VariantShape> {
    list.into_iter()
        .flat_map(|list| list.member_items())
        .filter_map(|item| match item {
            MemberItem::BareMember(variant) => Some(VariantShape {
                name: variant.name().as_ref().and_then(tree::name_text)?,
                fields: None,
                wire_name: None,
            }),
            MemberItem::Member(variant) => {
                let block = variant.block();
                let (wire_name, fields) = variant_block(block.as_ref());
                Some(VariantShape {
                    name: single_key(&variant)?,
                    fields: block
                        .filter(|_| fields)
                        .map(|block| lines(Some(block), FieldsBase::Object)),
                    wire_name,
                })
            }
            _ => None,
        })
        .collect()
}

/// A block's settings: every `name: value` member.
pub fn settings(list: Option<ast::MemberList>) -> Vec<Setting> {
    tree::members(list)
        .into_iter()
        .filter_map(|(name, member)| {
            Some((
                tree::name_text(&name)?,
                member
                    .value()
                    .map_or(Const::Other, |v| Const::from_expr(&v)),
            ))
        })
        .collect()
}

/// A field's settings: every `name: value` member, and a member with a
/// block of settings, as a provider package's `wire_parts:` is, its
/// block's (chunk 7.2e).
pub fn field_settings(list: Option<ast::MemberList>) -> Vec<Setting> {
    tree::members(list)
        .into_iter()
        .filter_map(|(name, member)| {
            let value = match (member.value(), member.block()) {
                (Some(value), _) => Const::from_expr(&value),
                (None, Some(block)) => Const::Block(settings(Some(block))),
                (None, None) => Const::Other,
            };
            Some((tree::name_text(&name)?, value))
        })
        .collect()
}

fn single_key(member: &ast::Member) -> Option<String> {
    let mut names = member.member_key()?.names();
    let first = names.next()?;
    if names.next().is_some() {
        return None;
    }
    tree::name_text(&first)
}

/// Entries of a list as names: `Todo.done` as `["Todo", "done"]`; an entry
/// that isn't a name is empty.
fn paths(list: &ast::ListExpr) -> Vec<Vec<String>> {
    list.exprs()
        .map(|expr| match TypeRef::from_expr(&expr) {
            TypeRef::Path(names, args) if args.is_empty() => names,
            _ => Vec::new(),
        })
        .collect()
}

/// Whether a member's value is written as a type: it starts with an
/// upper-case name (`Count`, `List<Tag>`, `Todo.id?`).
pub fn looks_like_type(expr: &Expr) -> bool {
    match TypeRef::from_expr(expr) {
        TypeRef::Path(names, _) => names.first().is_some_and(|first| starts_upper(first)),
        TypeRef::Nullable(_) => true,
        TypeRef::Invalid => false,
    }
}
