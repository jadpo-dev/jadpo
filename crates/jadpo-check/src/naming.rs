//! Area 1's naming rules for declarations: each kind's case, names that
//! survive the trip to a file name, unique names, names that fold to the
//! same file, reserved and generic names.

use jadpo_db::{Db, File, Item, ItemKind, Layout, Ownership, Project, Symbol, item_tree, symbols};
use jadpo_diag::{Decider, Diagnostic, Span, rule};

use crate::case::{is_lower_snake, is_upper_camel, round_trips, to_camel, to_snake};
use crate::lookups::{Lookup, NameView};
use crate::std_lib::StdNames;
use crate::tree::{self, file_span, name_span};

/// Where a declaration sits, to tell the first of two from the second.
fn order(path: &str, index: u32, function: Option<u32>) -> (String, u32, u32) {
    (path.to_string(), index, function.map_or(0, |f| f + 1))
}

fn symbol_order(symbol: &Symbol) -> (String, u32, u32) {
    order(
        &symbol.path,
        symbol.index,
        symbol.library.as_ref().map(|(_, i)| *i),
    )
}

/// Which kind of thing a name names, in the reader's words, with where.
fn described(symbol: &Symbol) -> String {
    format!("{} in `{}`", symbol.kind.with_a(), symbol.path)
}

/// A test's sentence is its name, unique across the project, so a report
/// or a rerun names exactly one test (area 13; its key is its subject and
/// sentence, area 18).
fn duplicate_test(
    db: &dyn Db,
    project: Project,
    source: &jadpo_syntax::ast::SourceFile,
    path: &str,
    item: &Item,
) -> Option<Diagnostic> {
    let name = crate::lookups::Name::new(db, item.name.clone());
    let found = crate::lookups::tests_called(db, project, name);
    let (first_path, _) = found
        .iter()
        .find(|(p, index)| (p.as_str(), *index) < (path, item.index))?;
    let span = match tree::decl_at(source, item.index) {
        Some(jadpo_syntax::ast::Decl::TestDecl(test)) => match test.string_token() {
            Some(token) => Span::new(
                path,
                token.text_range().start().into(),
                token.text_range().end().into(),
            ),
            None => name_span(source, path, item.index, None),
        },
        _ => name_span(source, path, item.index, None),
    };
    let sentence = &item.name;
    Some(
        Diagnostic::new(
            &rule::SEMANTIC_DUPLICATE_DECLARATION,
            format!("The test \"{sentence}\" is declared twice"),
        )
        .at(span, "declared again here")
        .reason(format!(
            "A test's sentence is its name, unique across the project, so a report or a rerun names exactly one test; it is already a test in `{first_path}`."
        ))
        .next("Reword this test's sentence to say what it checks that the other doesn't, or remove it if it's a copy"),
    )
}

/// The naming problems of one file's declarations.
#[salsa::tracked(returns(ref))]
pub fn naming_problems(db: &dyn Db, project: Project, file: File) -> Vec<Diagnostic> {
    let Some(source) = tree::source(db, file) else {
        return Vec::new();
    };
    let path = file.path(db);
    let table = Lookup::new(db, project);
    let mut out = Vec::new();
    for item in &item_tree(db, file).items {
        if item.kind == ItemKind::Test {
            if let Some(problem) = duplicate_test(db, project, &source, path, item) {
                out.push(problem);
            }
            continue;
        }
        match item.kind {
            ItemKind::Singleton | ItemKind::Package | ItemKind::Resource | ItemKind::Provide => {
                continue;
            }
            _ => {}
        }
        let span = name_span(&source, path, item.index, None);
        if let Some(problem) = declaration(&table, path, item, None, span) {
            out.push(problem);
        }
        for function in &item.functions {
            let span = name_span(&source, path, item.index, Some(function.index));
            if let Some(problem) =
                declaration(&table, path, function, Some((item.index, &item.name)), span)
            {
                out.push(problem);
            }
        }
    }
    out
}

/// The first naming problem of one declaration.
fn declaration(
    table: &Lookup<'_>,
    path: &str,
    item: &Item,
    library: Option<(u32, &str)>,
    span: Span,
) -> Option<Diagnostic> {
    let name = item.name.as_str();
    let names = StdNames::get();
    if let Some(problem) = case(name, &item.kind, span.clone()) {
        return Some(problem);
    }
    if names.is_type(name) {
        return Some(
            Diagnostic::new(
                &rule::SEMANTIC_DUPLICATE_DECLARATION,
                format!("`{name}` is a built-in type"),
            )
            .at(span, "already declared by Jadpo")
            .reason("Built-in types exist without being declared, and their names can't be reused.")
            .next(format!("Choose another name for this {}", item.kind.word())),
        );
    }
    if names.reserved(name) {
        return Some(reserved(name, span));
    }
    let (index, function) = match library {
        Some((library_index, _)) => (library_index, Some(item.index)),
        None => (item.index, None),
    };
    let here = order(path, index, function);
    if let Some(first) = table
        .declared(name)
        .iter()
        .min_by_key(|symbol| symbol_order(symbol))
        && symbol_order(first) < here
    {
        return Some(
            Diagnostic::new(
                &rule::SEMANTIC_DUPLICATE_DECLARATION,
                format!("`{name}` is declared twice"),
            )
            .at(span, "declared again here")
            .reason(format!(
                "Names are unique across the whole project, whatever they name; `{name}` is already {}.",
                described(first)
            ))
            .next(format!("Rename this {}, or remove it if it's a copy", item.kind.word())),
        );
    }
    if item.kind == ItemKind::Library && table.is_domain(name) {
        return Some(
            Diagnostic::new(
                &rule::SEMANTIC_DUPLICATE_DECLARATION,
                format!("`{name}` is already a domain"),
            )
            .at(span, "named like a domain")
            .reason("Domain, library and standard-library names are unique among themselves, so `name.call(...)` means one thing.")
            .next("Rename the library"),
        );
    }
    // The names folding to each file name are found once per symbol
    // table, not by comparing every pair of declarations.
    let folded = to_snake(name);
    if let Some(other) = table
        .same_file_name(&folded)
        .iter()
        .filter(|symbol| symbol.name != name)
        .filter(|symbol| symbol_order(symbol) < here)
        .min_by_key(|symbol| symbol_order(symbol))
    {
        return Some(
            Diagnostic::new(
                &rule::NAMING_FOLDED_COLLISION,
                format!("`{name}` and `{}` share a file name", other.name),
            )
            .at(span, format!("also `{folded}.jadpo`"))
            .reason(format!(
                "Both names become `{folded}.jadpo`, so a search for one finds the other; `{}` is {}.",
                other.name,
                described(other)
            ))
            .next("Rename one of them so their file names differ"),
        );
    }
    if item.kind == ItemKind::Library && names.is_generic(name) {
        return Some(generic(name, "library", span).next_for(
            "Rename the library after what its functions do",
            Decider::Agent,
        ));
    }
    None
}

/// The case a kind of name takes, and the round trip for `UpperCamelCase`.
fn case(name: &str, kind: &ItemKind, span: Span) -> Option<Diagnostic> {
    let word = kind.word();
    if kind.upper_camel() {
        if !is_upper_camel(name) {
            let fixed = to_camel(name);
            return Some(name_case(name, &fixed, word, "UpperCamelCase", span));
        }
        if !round_trips(name) {
            let fixed = to_camel(&to_snake(name));
            let file = to_snake(name);
            return Some(
                Diagnostic::new(
                    &rule::NAMING_ROUND_TRIP,
                    format!("`{name}` has consecutive capitals"),
                )
                .at(span.clone(), format!("reads back as `{fixed}`"))
                .reason(format!(
                    "A name must convert to its file name and back unchanged: `{name}` becomes `{file}.jadpo`, which reads back as `{fixed}`."
                ))
                .next(format!("Rename it `{fixed}`"))
                .edit(span, fixed),
            );
        }
    } else if !is_lower_snake(name) {
        let fixed = to_snake(name);
        return Some(name_case(name, &fixed, word, "lower_snake_case", span));
    }
    None
}

pub fn name_case(name: &str, fixed: &str, word: &str, form: &str, span: Span) -> Diagnostic {
    let mut chars = word.chars();
    let word = chars
        .next()
        .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default();
    Diagnostic::new(
        &rule::SEMANTIC_NAME_CASE,
        format!("{word} `{name}` isn't written in {form}"),
    )
    .at(span.clone(), format!("should be `{fixed}`"))
    .reason("Types, entities and route groups are written in UpperCamelCase; functions, actions, queries, fields, variables and the rest in lower_snake_case.")
    .next(format!("Rename `{name}` to `{fixed}`, here and where it's used"))
    .edit(span, fixed)
}

pub fn reserved(name: &str, span: Span) -> Diagnostic {
    Diagnostic::new(
        &rule::SEMANTIC_STANDARD_NAMESPACE_RESERVED,
        format!("`{name}` is a standard-library namespace"),
    )
    .at(span, "reserved")
    .reason(format!(
        "Standard-library namespaces can't name a declaration, domain, library or variable, so `{name}.` always means the standard library."
    ))
    .next(format!("Choose another name than `{name}`"))
}

fn generic(name: &str, what: &str, span: Span) -> Diagnostic {
    let generic: Vec<String> = StdNames::get()
        .generic_names()
        .map(|name| format!("`{name}`"))
        .collect();
    Diagnostic::new(
        &rule::NAMING_GENERIC_NAME,
        format!("`{name}` is too generic for a {what}"),
    )
    .at(span, "says nothing about what it holds")
    .reason(format!(
        "Domains and libraries are named after what they hold, because the name appears in every call; {} say nothing.",
        crate::and_list(&generic)
    ))
}

/// Naming problems of the domains themselves, read from folder names.
#[salsa::tracked(returns(ref))]
pub fn domain_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    if project.layout(db) == Layout::Loose {
        return Vec::new();
    }
    let names = StdNames::get();
    let mut out = Vec::new();
    for domain in &symbols(db, project).domains {
        let Some(first) = project
            .files(db)
            .iter()
            .filter(|file| {
                jadpo_db::location(db, project, **file).ownership
                    == Ownership::Domain(domain.clone())
            })
            .map(|file| file.path(db))
            .min()
        else {
            continue;
        };
        let span = file_span(first);
        if !is_lower_snake(domain) {
            let fixed = to_snake(domain);
            out.push(
                Diagnostic::new(
                    &rule::SEMANTIC_NAME_CASE,
                    "Domains are written in lower_snake_case",
                )
                .at(span, "")
                .reason(format!(
                    "A domain's folder name is its name, used in calls such as `{fixed}.action(...)`."
                ))
                .next(format!("Rename the folder `domains/{domain}/` to `domains/{fixed}/`")),
            );
        } else if names.reserved(domain) {
            out.push(reserved(domain, span));
        } else if names.is_generic(domain) {
            out.push(generic(domain, "domain", span).next_for(
                format!("Rename `domains/{domain}/` after the business area it covers"),
                Decider::Human,
            ));
        }
    }
    out
}
