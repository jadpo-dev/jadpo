//! A value is not a type (area 2, `type.not_a_type`; chunk 7.9f). Where a
//! field's type goes, in any block of fields, an entity or `config { }`,
//! a call (`Status("open")`), a literal (`5`) or an enum's variant
//! (`Status.open`) is one error naming the field and what was written;
//! in any other type position an enum's variant is the same error without
//! a field. The mirror of `type.not_a_value`, worked out here so every
//! place says the same.

use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_syntax::ast::{self, AstNode, Expr, Type};

use crate::std_lib::StdNames;
use crate::tree::{self, dotted, node_span};
use crate::types::{Def, TypeView};

/// The part of a written type that is a value.
#[derive(Clone, Debug)]
pub(crate) enum Found {
    /// `Status.open`: a value of the enum `host`; `known` when it names
    /// one of its variants.
    Variant {
        span: Span,
        written: String,
        host: String,
        known: bool,
    },
    /// `Status("open")`, with the type it calls when it names one.
    Call {
        span: Span,
        written: String,
        callee: Option<String>,
    },
    /// `[Tag]`, a list type as Swift writes it, with its item type.
    List {
        span: Span,
        written: String,
        item: String,
    },
    /// `5`, `"Mug"` or any other value.
    Value { span: Span, written: String },
}

/// The field a type is written for: `("field", "status")`.
pub(crate) type Field<'a> = (&'a str, &'a str);

/// The first part of a member's value, written where a type goes, that is
/// a value: a call or literal anywhere in it, or an enum's variant as
/// `types` knows them.
pub(crate) fn in_value(path: &str, expr: &Expr, types: Option<&dyn TypeView>) -> Option<Found> {
    match expr {
        Expr::NameRef(_) | Expr::ElisionExpr(_) => None,
        Expr::FieldExpr(_) => {
            let (first, rest) = dotted(expr)?;
            variant(path, &first, &rest, expr.syntax(), types?)
        }
        Expr::GenericExpr(generic) => {
            if let Some(inner) = generic.expr()
                && let Some(found) = in_value(path, &inner, types)
            {
                return Some(found);
            }
            generic
                .generic_arg_list()
                .into_iter()
                .flat_map(|list| list.types())
                .find_map(|arg| in_type(path, &arg, types?))
        }
        Expr::NullableExpr(nullable) => in_value(path, &nullable.expr()?, types),
        // `Text { max_length: 80 }`: the type before its settings.
        Expr::RecordExpr(record) => in_value(path, &record.expr()?, types),
        Expr::CallExpr(call) => {
            let callee = call
                .expr()
                .and_then(|callee| dotted(&callee))
                .filter(|(first, rest)| {
                    rest.is_empty()
                        && tree::name_ref_text(first)
                            .is_some_and(|name| name.starts_with(|c: char| c.is_ascii_uppercase()))
                })
                .and_then(|(first, _)| tree::name_ref_text(&first));
            Some(Found::Call {
                span: node_span(path, call.syntax()),
                written: written_spaced(call.syntax()),
                callee,
            })
        }
        // `[Tag]`, Swift's list type.
        Expr::ListExpr(list) => {
            let mut items = list.exprs();
            match (items.next(), items.next()) {
                (Some(item), None) if crate::types::looks_like_type(&item) => Some(Found::List {
                    span: node_span(path, list.syntax()),
                    written: written_spaced(list.syntax()),
                    item: written_spaced(item.syntax()),
                }),
                _ => Some(Found::Value {
                    span: node_span(path, list.syntax()),
                    written: written_spaced(list.syntax()),
                }),
            }
        }
        other => Some(Found::Value {
            span: node_span(path, other.syntax()),
            written: written_spaced(other.syntax()),
        }),
    }
}

/// An enum's variant anywhere in a signature's type: `List<Status.open>`.
pub(crate) fn in_type(path: &str, ty: &Type, types: &dyn TypeView) -> Option<Found> {
    match ty {
        Type::PathType(path_type) => {
            let names: Vec<ast::NameRef> = path_type
                .path()
                .into_iter()
                .flat_map(|p| p.name_refs())
                .collect();
            if let Some((first, rest)) = names.split_first()
                && let Some(found) = variant(path, first, rest, path_type.syntax(), types)
            {
                return Some(found);
            }
            path_type
                .generic_arg_list()
                .into_iter()
                .flat_map(|list| list.types())
                .find_map(|arg| in_type(path, &arg, types))
        }
        Type::NullableType(nullable) => in_type(path, &nullable.ty()?, types),
    }
}

/// `Enum.variant`: a value of an enum, never a type (a variant's field
/// type is `Enum.variant.field`).
fn variant(
    path: &str,
    first: &ast::NameRef,
    rest: &[ast::NameRef],
    node: &jadpo_syntax::SyntaxNode,
    types: &dyn TypeView,
) -> Option<Found> {
    let [second] = rest else { return None };
    let host = tree::name_ref_text(first)?;
    let named = tree::name_ref_text(second)?;
    let variants: Vec<String> = match types.def(&types.canonical(&host)) {
        Some(Def::Enum { variants }) => variants.iter().map(|v| v.name.clone()).collect(),
        Some(_) => return None,
        None => StdNames::get().enums().get(&host)?.clone(),
    };
    let span = node_span(path, node);
    let written = tree::written(node);
    Some(Found::Variant {
        span,
        written,
        known: variants.contains(&named),
        host,
    })
}

/// Whether `host` is an enum: `host.name` is then one of its values, and
/// never a type, which `type.not_a_type` reports where it is written.
pub(crate) fn is_enum(types: &dyn TypeView, host: &str) -> bool {
    matches!(types.def(&types.canonical(host)), Some(Def::Enum { .. }))
        || (!types.contains(host) && StdNames::get().enums().contains_key(host))
}

const TYPES_ARE: &str = "a type's name, a field type, `Type.field`, `List<T>`, `T?`, or a type with rules in a block, such as `Text { max_length: 80 }`";

impl Found {
    /// The error, naming `field` when the type is written for one.
    pub(crate) fn diagnostic(&self, field: Option<Field<'_>>) -> Diagnostic {
        let (word, whose, subject) = match field {
            Some((word, name)) => (word, format!("`{name}`'s type"), format!("A {word}'s type")),
            None => ("value", "This type".to_string(), "A type".to_string()),
        };
        match self {
            Found::Variant {
                span,
                written,
                host,
                known,
            } => {
                let (label, what) = if *known {
                    (
                        format!("a value of `{host}`, not a type"),
                        format!("`{written}` is one of those values, given where a value is built"),
                    )
                } else {
                    (
                        "not a type".to_string(),
                        format!(
                            "`{host}` is an enum, so it has no fields to name as a type, and `{written}` isn't one"
                        ),
                    )
                };
                Diagnostic::new(
                    &rule::TYPE_NOT_A_TYPE,
                    format!("{whose} is written as a value, `{written}`"),
                )
                .at(span.clone(), label)
                .reason(format!(
                    "{subject} says which values it may hold, such as `{host}`; {what}."
                ))
                .next(format!("Write `{host}`"))
                .edit(span.clone(), host.clone())
            }
            Found::Call {
                span,
                written,
                callee,
            } => {
                let problem = Diagnostic::new(
                    &rule::TYPE_NOT_A_TYPE,
                    format!("{whose} is written as a call, `{written}`"),
                )
                .at(span.clone(), "a call, not a type")
                .reason(format!(
                    "{subject} names a type: {TYPES_ARE}. A call makes a value, which can't say what the {word} holds."
                ));
                match callee {
                    Some(callee) => problem
                        .next(format!("Write `{callee}`"))
                        .edit(span.clone(), callee.clone()),
                    None => problem.next(format!("Write the type the {word} holds")),
                }
            }
            Found::List {
                span,
                written,
                item,
            } => Diagnostic::new(
                &rule::SYNTAX_COLLECTION_TYPE,
                format!("Jadpo writes `List<{item}>`, not `{written}`"),
            )
            .at(span.clone(), format!("write `List<{item}>`"))
            .reason("`[T]` is a list type in Swift; Jadpo writes a collection's types in angle brackets, as in `List<T>`, `Set<T>` and `Map<K, V>`.")
            .next(format!("Write `List<{item}>`"))
            .edit(span.clone(), format!("List<{item}>")),
            Found::Value { span, written } => Diagnostic::new(
                &rule::TYPE_NOT_A_TYPE,
                format!("{whose} is written as a value, `{written}`"),
            )
            .at(span.clone(), "a value, not a type")
            .reason(format!(
                "{subject} names a type: {TYPES_ARE}. A value can't say what the {word} holds."
            ))
            .next(format!(
                "Write the type the {word} holds in place of `{written}`"
            )),
        }
    }
}

/// A field whose type is written as a block of fields: `address: {
/// street: Street }`. There is no unnamed object (area 2, "Objects").
pub(crate) fn block(path: &str, field: &str, block: &ast::MemberList) -> Diagnostic {
    let span = node_span(path, block.syntax());
    let name = crate::case::to_camel(field);
    Diagnostic::new(
        &rule::TYPE_NOT_A_TYPE,
        format!("`{field}`'s type is written as a block of fields"),
    )
    .at(span, "an unnamed object, not a type")
    .reason("There is no unnamed object: a nested shape is declared once, with a name, and each field that holds it uses that name.")
    .next(format!(
        "Declare `type {name} = Object {{ ... }}` with these fields, and write `{field}: {name}`"
    ))
}

/// A node's text as written, with its spaces.
fn written_spaced(node: &jadpo_syntax::SyntaxNode) -> String {
    node.text().to_string().trim().to_string()
}
