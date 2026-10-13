//! What a type is to the checker (area 2): the built-in types, the types a
//! program declares, field types, collections, nullable types, and `Error`,
//! which stands for a type already reported as wrong and absorbs every
//! check made with it, so one mistake gives one error.

use std::fmt;

use jadpo_syntax::ast::{self, AstNode, Expr, Type};

use crate::tree::{self, dotted};

/// The types built into the language, from `jadpo-std`, which the runtime
/// shares: no Jadpo declaration can say what they are made of.
pub use jadpo_std::Builtin as Prim;

/// The bases used only after `=` in a `type` declaration (area 2).
pub const BASES: [&str; 7] = [
    "Object",
    "Enum",
    "Input",
    "Output",
    "Failure",
    "Event",
    "EmailMessage",
];

/// The generic containers and how many type arguments each takes.
pub fn container_arity(name: &str) -> Option<usize> {
    match name {
        "List" | "Set" => Some(1),
        "Map" => Some(2),
        _ => None,
    }
}

/// A type as the checker sees it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Ty {
    /// A type already reported as wrong: compatible with everything.
    Error,
    Prim(Prim),
    /// A declared type, an entity, or a standard-library enum.
    Named(String),
    /// `Owner.field`, or `Enum.variant.field` for a variant's payload
    /// field: a refinement of the field's declared type.
    Field(String, Vec<String>),
    List(Box<Ty>),
    Set(Box<Ty>),
    Map(Box<Ty>, Box<Ty>),
    Nullable(Box<Ty>),
    /// The type of `none`.
    None,
    /// A built-in type a later area defines (`Page<T>`, `Principal`): only
    /// equal to itself until that area is built.
    Opaque(String, Vec<Ty>),
    /// A type variable in a standard-library signature.
    Var(String),
    /// A type variable no argument fixed at a call: anything fits where it
    /// stands, so it absorbs checks as `Error` does, but the containers
    /// around it are still checked (a `List<T>` parameter takes only a
    /// list).
    Unknown(String),
    /// Any type built on `Int` or `Decimal`; only in standard-library
    /// signatures.
    Number,
    /// An argument written inline as `item -> expression`; only in
    /// standard-library signatures.
    Function(Box<Ty>, Box<Ty>),
    /// The data a payload variant carries, bound by a `match` arm
    /// (`PaymentOutcome.paid(paid)`): its fields are the variant's
    /// (area 3, question 9).
    Variant(String, String),
}

impl Ty {
    pub fn is_error(&self) -> bool {
        matches!(self, Ty::Error)
    }

    /// Whether `Error`, or a type variable no argument fixed, appears
    /// anywhere inside.
    pub fn has_error(&self) -> bool {
        match self {
            Ty::Error | Ty::Unknown(_) => true,
            Ty::List(inner) | Ty::Set(inner) | Ty::Nullable(inner) => inner.has_error(),
            Ty::Map(key, value) | Ty::Function(key, value) => key.has_error() || value.has_error(),
            Ty::Opaque(_, args) => args.iter().any(Ty::has_error),
            _ => false,
        }
    }

    /// Whether a type variable no argument fixed appears anywhere inside.
    pub fn has_unknown(&self) -> bool {
        match self {
            Ty::Unknown(_) => true,
            Ty::List(inner) | Ty::Set(inner) | Ty::Nullable(inner) => inner.has_unknown(),
            Ty::Map(key, value) | Ty::Function(key, value) => {
                key.has_unknown() || value.has_unknown()
            }
            _ => false,
        }
    }

    /// The type without an outer `?`.
    pub fn strip_nullable(&self) -> &Ty {
        match self {
            Ty::Nullable(inner) => inner.strip_nullable(),
            other => other,
        }
    }

    pub fn nullable(self) -> Ty {
        match self {
            Ty::Nullable(_) | Ty::Error | Ty::None => self,
            other => Ty::Nullable(Box::new(other)),
        }
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Error => f.write_str("?"),
            Ty::Prim(prim) => f.write_str(prim.name()),
            Ty::Named(name) | Ty::Var(name) | Ty::Unknown(name) => f.write_str(name),
            Ty::Number => f.write_str("Number"),
            Ty::Field(host, path) => write!(f, "{host}.{}", path.join(".")),
            Ty::List(inner) => write!(f, "List<{inner}>"),
            Ty::Set(inner) => write!(f, "Set<{inner}>"),
            Ty::Map(key, value) => write!(f, "Map<{key}, {value}>"),
            Ty::Nullable(inner) => write!(f, "{inner}?"),
            Ty::None => f.write_str("none"),
            Ty::Opaque(name, args) if args.is_empty() => f.write_str(name),
            Ty::Opaque(name, args) => {
                let args: Vec<String> = args.iter().map(Ty::to_string).collect();
                write!(f, "{name}<{}>", args.join(", "))
            }
            Ty::Function(from, to) => write!(f, "{from} -> {to}"),
            Ty::Variant(host, variant) => write!(f, "{host}.{variant}"),
        }
    }
}

/// A type as written, before its names are looked up: `List<Tag>`,
/// `Customer.email?`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TypeRef {
    Path(Vec<String>, Vec<TypeRef>),
    Nullable(Box<TypeRef>),
    /// Not a type at all; the parser or resolution has said so.
    Invalid,
}

impl TypeRef {
    pub fn from_type(ty: &Type) -> TypeRef {
        match ty {
            Type::PathType(path) => {
                let names: Vec<String> = path
                    .path()
                    .into_iter()
                    .flat_map(|path| path.name_refs())
                    .filter_map(|name| tree::name_ref_text(&name))
                    .collect();
                if names.is_empty() {
                    return TypeRef::Invalid;
                }
                let args = path
                    .generic_arg_list()
                    .into_iter()
                    .flat_map(|list| list.types())
                    .map(|arg| TypeRef::from_type(&arg))
                    .collect();
                TypeRef::Path(names, args)
            }
            Type::NullableType(nullable) => match nullable.ty() {
                Some(inner) => TypeRef::Nullable(Box::new(TypeRef::from_type(&inner))),
                None => TypeRef::Invalid,
            },
        }
    }

    /// A type written as a member's value: `Count`, `Todo.id`, `List<Tag>`,
    /// `Name?`, and `Name { max_length: 12 }` without its settings.
    pub fn from_expr(expr: &Expr) -> TypeRef {
        match expr {
            Expr::NameRef(_) | Expr::FieldExpr(_) => match dotted(expr) {
                Some((first, rest)) => {
                    let names: Option<Vec<String>> = std::iter::once(&first)
                        .chain(rest.iter())
                        .map(tree::name_ref_text)
                        .collect();
                    names.map_or(TypeRef::Invalid, |names| TypeRef::Path(names, Vec::new()))
                }
                None => TypeRef::Invalid,
            },
            Expr::GenericExpr(generic) => {
                let Some(TypeRef::Path(names, _)) = generic.expr().map(|e| TypeRef::from_expr(&e))
                else {
                    return TypeRef::Invalid;
                };
                let args = generic
                    .generic_arg_list()
                    .into_iter()
                    .flat_map(|list| list.types())
                    .map(|arg| TypeRef::from_type(&arg))
                    .collect();
                TypeRef::Path(names, args)
            }
            Expr::NullableExpr(nullable) => match nullable.expr() {
                Some(inner) => TypeRef::Nullable(Box::new(TypeRef::from_expr(&inner))),
                None => TypeRef::Invalid,
            },
            Expr::RecordExpr(record) => record
                .expr()
                .map_or(TypeRef::Invalid, |inner| TypeRef::from_expr(&inner)),
            _ => TypeRef::Invalid,
        }
    }

    /// A type as `jadpo-db` records it in a signature: written without
    /// spaces, such as `List<Map<Tag,Count>>?`.
    pub fn parse(written: &str) -> TypeRef {
        let mut chars = written.chars().peekable();
        let parsed = parse_ref(&mut chars);
        if chars.peek().is_some() {
            return TypeRef::Invalid;
        }
        parsed
    }
}

fn parse_ref(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> TypeRef {
    let mut names = vec![String::new()];
    while let Some(&c) = chars.peek() {
        if c.is_ascii_alphanumeric() || c == '_' {
            if let Some(name) = names.last_mut() {
                name.push(c);
            }
            chars.next();
        } else if c == '.' {
            names.push(String::new());
            chars.next();
        } else {
            break;
        }
    }
    if names.iter().any(String::is_empty) {
        return TypeRef::Invalid;
    }
    let mut args = Vec::new();
    if chars.peek() == Some(&'<') {
        chars.next();
        loop {
            args.push(parse_ref(chars));
            match chars.next() {
                Some(',') => {}
                Some('>') => break,
                _ => return TypeRef::Invalid,
            }
        }
    }
    let mut parsed = TypeRef::Path(names, args);
    while chars.peek() == Some(&'?') {
        chars.next();
        parsed = TypeRef::Nullable(Box::new(parsed));
    }
    parsed
}

/// The node of a type as written: a signature's `Type` or a member's value.
#[derive(Clone, Debug)]
pub enum TypeNode {
    Type(Type),
    Expr(Expr),
}

impl TypeNode {
    pub fn syntax(&self) -> &jadpo_syntax::SyntaxNode {
        match self {
            TypeNode::Type(ty) => ty.syntax(),
            TypeNode::Expr(expr) => expr.syntax(),
        }
    }

    pub fn to_ref(&self) -> TypeRef {
        match self {
            TypeNode::Type(ty) => TypeRef::from_type(ty),
            TypeNode::Expr(expr) => TypeRef::from_expr(expr),
        }
    }

    /// The type inside `?`, if this is a nullable type.
    pub fn nullable_inner(&self) -> Option<TypeNode> {
        match self {
            TypeNode::Type(Type::NullableType(nullable)) => nullable.ty().map(TypeNode::Type),
            TypeNode::Expr(Expr::NullableExpr(nullable)) => nullable.expr().map(TypeNode::Expr),
            TypeNode::Expr(Expr::RecordExpr(record)) => {
                record.expr().map(TypeNode::Expr)?.nullable_inner()
            }
            _ => None,
        }
    }

    /// The written type arguments: `Tag` in `List<Tag>`.
    pub fn args(&self) -> Vec<TypeNode> {
        let list = match self {
            TypeNode::Type(Type::PathType(path)) => path.generic_arg_list(),
            TypeNode::Expr(Expr::GenericExpr(generic)) => generic.generic_arg_list(),
            TypeNode::Expr(Expr::RecordExpr(record)) => {
                return record
                    .expr()
                    .map(|inner| TypeNode::Expr(inner).args())
                    .unwrap_or_default();
            }
            _ => None,
        };
        list.into_iter()
            .flat_map(|list| list.types())
            .map(TypeNode::Type)
            .collect()
    }

    /// The node naming the type: `List` in `List<Tag>`, `Todo.id` in
    /// `Todo.id?`.
    pub fn head(&self) -> jadpo_syntax::SyntaxNode {
        match self {
            TypeNode::Type(Type::PathType(path)) => path
                .path()
                .map_or_else(|| path.syntax().clone(), |p| p.syntax().clone()),
            TypeNode::Expr(Expr::GenericExpr(generic)) => generic
                .expr()
                .map_or_else(|| generic.syntax().clone(), |e| e.syntax().clone()),
            TypeNode::Expr(Expr::RecordExpr(record)) => record.expr().map_or_else(
                || record.syntax().clone(),
                |inner| TypeNode::Expr(inner).head(),
            ),
            other => other.syntax().clone(),
        }
    }

    /// The written type without a field's settings block.
    pub fn without_settings(&self) -> TypeNode {
        match self {
            TypeNode::Expr(Expr::RecordExpr(record)) => {
                record.expr().map_or_else(|| self.clone(), TypeNode::Expr)
            }
            other => other.clone(),
        }
    }

    /// A field's settings block: `{ max_length: 12 }`.
    pub fn settings(&self) -> Option<ast::MemberList> {
        match self {
            TypeNode::Expr(Expr::RecordExpr(record)) => record.member_list(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(names: &[&str], args: Vec<TypeRef>) -> TypeRef {
        TypeRef::Path(names.iter().map(|n| n.to_string()).collect(), args)
    }

    #[test]
    fn written_types_parse_back() {
        assert_eq!(TypeRef::parse("Count"), path(&["Count"], vec![]));
        assert_eq!(
            TypeRef::parse("List<Map<A,B.c>>?"),
            TypeRef::Nullable(Box::new(path(
                &["List"],
                vec![path(
                    &["Map"],
                    vec![path(&["A"], vec![]), path(&["B", "c"], vec![])]
                )]
            )))
        );
        assert_eq!(TypeRef::parse("List<"), TypeRef::Invalid);
        assert_eq!(TypeRef::parse(""), TypeRef::Invalid);
    }

    #[test]
    fn types_display_as_written() {
        let ty = Ty::Map(
            Box::new(Ty::Named("Key".into())),
            Box::new(Ty::Field("Host".into(), vec!["field".into()]).nullable()),
        );
        assert_eq!(ty.to_string(), "Map<Key, Host.field?>");
    }
}
