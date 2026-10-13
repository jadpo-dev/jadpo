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
            "Failure" => FieldsBase::Failure,
            _ => return None,
        })
    }

    /// Whether area 2 decides its fields' settings and lines: the other
    /// bases are checked by their own areas.
    pub fn is_area_two(self) -> bool {
        matches!(
            self,
            FieldsBase::Object | FieldsBase::Input | FieldsBase::Output
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
    /// Not a field: an entity's setting or section, or something the
    /// parser has already reported.
    Other,
}

/// A variant and, when it carries data, its fields.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VariantShape {
    pub name: String,
    pub fields: Option<Vec<Line>>,
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
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Shape {
    pub name: String,
    pub kind: ShapeKind,
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
            let parent = decl.base().map_or(TypeRef::Invalid, |base| TypeRef::from_type(&base));
            let list = decl.member_list();
            let kind = match &parent {
                TypeRef::Path(names, args) if names.len() == 1 && args.is_empty() => {
                    match (names[0].as_str(), FieldsBase::from_base(&names[0])) {
                        ("Enum", _) => ShapeKind::Enum {
                            variants: variants(list),
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
            _ => Line::Other,
        })
        .collect()
}

fn line(member: &ast::Member, base: FieldsBase) -> Line {
    let Some(key) = single_key(member) else {
        return Line::Other;
    };
    let value = member.value();
    let copying = matches!(key.as_str(), "include" | "pick" | "omit" | "optional");
    if copying && base != FieldsBase::Entity {
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
                Expr::RecordExpr(record) => settings(record.member_list()),
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
            }),
            MemberItem::Member(variant) => Some(VariantShape {
                name: single_key(&variant)?,
                fields: variant
                    .block()
                    .map(|block| lines(Some(block), FieldsBase::Object)),
            }),
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
                member.value().map_or(Const::Other, |v| Const::from_expr(&v)),
            ))
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
