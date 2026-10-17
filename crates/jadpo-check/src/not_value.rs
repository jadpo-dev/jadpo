//! A declaration's name is not a value (area 3, `type.not_a_value`). A
//! type, failure, entity, enum, function, action, query, library or other
//! name written where a value is read is one error, whatever reads it: a
//! body, a route's `run:` or a query's condition. What each kind is, and
//! the one step that gives a value, is worked out here, so every place says
//! the same.

use jadpo_db::ItemKind;
use jadpo_diag::{Diagnostic, Span, rule};

use crate::lookups::NameView;
use crate::std_lib::{Namespace, StdNames};
use crate::types::{Def, FieldsBase, TypeView};

/// What a name stands for, when it is no value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Named {
    /// A named or built-in type that holds one value: `Count`, `Text`;
    /// `built_in` when no `Count(...)` builds it.
    Scalar {
        built_in: bool,
    },
    /// An object, input, output, event or email.
    Object,
    /// An enum, with its first variant.
    Enum {
        first: Option<String>,
    },
    Failure,
    Entity,
    /// A function, action, query or fixture, called and never passed.
    Callable(&'static str),
    /// A name that groups others: a standard or generated namespace, a
    /// domain, a library, a connection or a package.
    Group(&'static str),
    /// Any other declaration, by its kind.
    Other(&'static str),
}

/// What a type's definition makes its name.
fn from_def(def: &Def) -> Option<Named> {
    match def {
        Def::Scalar { .. } => Some(Named::Scalar { built_in: false }),
        Def::Fields { base, .. } => Some(match base {
            FieldsBase::Entity => Named::Entity,
            FieldsBase::Failure => Named::Failure,
            _ => Named::Object,
        }),
        Def::Enum { variants } => Some(Named::Enum {
            first: variants.first().map(|v| v.name.clone()),
        }),
        // A time format has its own message (area 10).
        Def::Format { .. } => None,
    }
}

/// What the standard library makes `name`, when it is no value.
fn std_named(name: &str) -> Option<Named> {
    let std = StdNames::get();
    match std.namespace(name) {
        Some(Namespace::Standard) => Some(Named::Group("standard library namespace")),
        Some(Namespace::Generated) => Some(Named::Group("generated namespace")),
        None => std
            .is_type(name)
            .then_some(Named::Scalar { built_in: true }),
    }
}

/// What `name` stands for among the types, when it names one or a
/// standard name that is no value.
pub(crate) fn classify_type(types: &(impl TypeView + ?Sized), name: &str) -> Option<Named> {
    match types.def(name) {
        Some(def) => from_def(def),
        None => std_named(name),
    }
}

/// What `name` stands for, when it names a declaration or a built-in that
/// is no value. A time format is the one exception: it has its own
/// message (area 10).
pub(crate) fn classify(
    symbols: &(impl NameView + ?Sized),
    types: &(impl TypeView + ?Sized),
    name: &str,
) -> Option<Named> {
    if let Some(def) = types.def(name) {
        return from_def(def);
    }
    if let Some(symbol) = symbols.get(name) {
        return match &symbol.kind {
            ItemKind::Function => Some(Named::Callable("function")),
            ItemKind::Action => Some(Named::Callable("action")),
            ItemKind::Query => Some(Named::Callable("query")),
            ItemKind::Fixture => Some(Named::Callable("fixture")),
            ItemKind::Entity => Some(Named::Entity),
            ItemKind::Library => Some(Named::Group("library")),
            ItemKind::Type(jadpo_db::Base::Failure) => Some(Named::Failure),
            ItemKind::Type(_) => Some(Named::Scalar { built_in: false }),
            other => Some(Named::Other(other.word())),
        };
    }
    if let Some(named) = std_named(name) {
        return Some(named);
    }
    if symbols.is_domain(name) {
        return Some(Named::Group("domain"));
    }
    if symbols.is_connection(name) {
        return Some(Named::Group("connection"));
    }
    if symbols.is_package(name) {
        return Some(Named::Group("package"));
    }
    None
}

fn with_a(word: &str) -> String {
    if word.starts_with(['a', 'e', 'i', 'o', 'u']) {
        format!("an {word}")
    } else {
        format!("a {word}")
    }
}

/// `name`, which is `named`, where a value is read.
pub(crate) fn diagnostic(name: &str, named: &Named, span: Span) -> Diagnostic {
    let (what, reason, next) = match named {
        Named::Scalar { built_in: false } => (
            "a type".to_string(),
            format!("A type says what a value looks like and holds none itself; a value of `{name}` is built from one of its own."),
            format!("Build a value with `{name}(...)`"),
        ),
        Named::Scalar { built_in: true } => (
            "a type".to_string(),
            "A type says what a value looks like and holds none itself.".to_string(),
            format!("Write a value of `{name}` here, such as a literal or a variable"),
        ),
        Named::Object => (
            "a type".to_string(),
            "A type says what a value looks like and holds none itself; a value is built with the type's name and every field in a block.".to_string(),
            format!("Build a value with `{name} {{ ... }}`, giving each field"),
        ),
        Named::Enum { first } => (
            "an enum".to_string(),
            format!("An enum is the set of values a thing can take, and each value is one of its variants; `{name}` is the set, never a value of it."),
            match first {
                Some(first) => format!("Name a variant, as in `{name}.{first}`"),
                None => format!("Name one of `{name}`'s variants, as in `{name}.<variant>`"),
            },
        ),
        Named::Failure => (
            "a failure".to_string(),
            "A failure is a way a callable stops, raised with `reject`; it is not a value to hold, pass or compare.".to_string(),
            format!("Raise it with `reject {name}`, or use a value here"),
        ),
        Named::Entity => (
            "an entity".to_string(),
            "An entity names a table of stored records, not a value; a record is read and written through the entity's operations.".to_string(),
            format!("Read a record with `{name}.get(id)`, or use a value here"),
        ),
        Named::Callable(word) => (
            with_a(word),
            "Functions and actions aren't values: they are called, and only a standard-library collection operation takes an inline function, `item -> expression`.".to_string(),
            format!("Call it, as in `{name}(...)`"),
        ),
        Named::Group(word) => (
            with_a(word),
            format!("{} holds names; it is no value itself, and what it holds is reached through a dot.", capital(&with_a(word))),
            match *word {
                "generated namespace" if name == "config" => {
                    "Read a setting, as in `config.<setting>`".to_string()
                }
                "generated namespace" => format!("Read one of its members, as in `{name}.now`"),
                "connection" => format!("Call one of its operations, as in `{name}.<operation>(...)`"),
                _ => format!("Call something it holds, as in `{name}.<name>(...)`"),
            },
        ),
        Named::Other(word) => (
            with_a(word),
            format!("{} is declared to be used in its own way, never read as a value.", capital(&with_a(word))),
            "Use a value here".to_string(),
        ),
    };
    Diagnostic::new(
        &rule::TYPE_NOT_A_VALUE,
        format!("`{name}` is {what}, not a value"),
    )
    .at(span, what)
    .reason(reason)
    .next(next)
}

fn capital(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// `collection.count` read without a call: what a namespace, domain,
/// library or connection holds is called, and none of it is a value.
pub(crate) fn member_diagnostic(written: &str, span: Span) -> Diagnostic {
    Diagnostic::new(
        &rule::TYPE_NOT_A_VALUE,
        format!("`{written}` is called, not a value"),
    )
    .at(span, "to call")
    .reason("What a namespace, domain, library or connection holds is an operation, function, action or query; each is called, and none is a value.")
    .next(format!("Call it, as in `{written}(...)`"))
}
