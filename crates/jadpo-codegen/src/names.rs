//! What generated code calls things: a module per domain, and each
//! callable's function by its number and label, so a reader of the crate
//! finds `c12_action_create_parcel` where a report names `action
//! create_parcel`.

use jadpo_plan::CallablePlan;
use proc_macro2::Ident;
use quote::format_ident;

/// Words a module can't be called, being Rust's own or a crate generated
/// code names: a domain called one of them takes a `_` after it.
const RESERVED: &[&str] = &[
    "abstract",
    "alloc",
    "as",
    "async",
    "await",
    "become",
    "box",
    "break",
    "const",
    "continue",
    "core",
    "crate",
    "do",
    "dyn",
    "else",
    "enum",
    "extern",
    "false",
    "final",
    "fn",
    "for",
    "gen",
    "if",
    "impl",
    "in",
    "jadpo_runtime",
    "jadpo_std",
    "jadpo_value",
    "let",
    "loop",
    "macro",
    "match",
    "mod",
    "move",
    "mut",
    "override",
    "priv",
    "pub",
    "ref",
    "return",
    "self",
    "static",
    "std",
    "struct",
    "super",
    "trait",
    "true",
    "try",
    "type",
    "typeof",
    "unsafe",
    "unsized",
    "use",
    "virtual",
    "where",
    "while",
    "yield",
];

/// `text` as lower snake case of ASCII letters and digits, at most 48
/// characters: `test "a todo is read back"` is `test_a_todo_is_read_back`.
pub(crate) fn snake(text: &str) -> String {
    let mut out = String::new();
    let mut gap = false;
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            if gap && !out.is_empty() {
                out.push('_');
            }
            gap = false;
            out.push(c.to_ascii_lowercase());
        } else {
            gap = true;
        }
    }
    out.truncate(48);
    out.trim_end_matches('_').to_string()
}

/// The module a callable in `file` goes in: its domain's
/// (`domains/<domain>/…`), else its top folder's (`shared/`, `tests/`),
/// else `app`, for the files at the project's root. Two callables in one
/// module never clash, since each function is named by its number.
pub(crate) fn module(file: &str) -> String {
    let mut parts = file.split('/');
    let first = parts.next().unwrap_or_default();
    let second = parts.next();
    let name = match (first, second) {
        ("domains", Some(domain)) if parts.next().is_some() => snake(domain),
        (folder, Some(_)) => snake(folder),
        _ => String::new(),
    };
    let name = if name.is_empty() {
        "app".to_string()
    } else if name.starts_with(|c: char| c.is_ascii_digit()) {
        format!("m{name}")
    } else {
        name
    };
    if RESERVED.contains(&name.as_str()) {
        format!("{name}_")
    } else {
        name
    }
}

/// Callable `number`'s function: `c12_action_create_parcel`.
pub(crate) fn function(number: u32, callable: &CallablePlan) -> Ident {
    let label = snake(&callable.label);
    if label.is_empty() {
        format_ident!("c{}", number)
    } else {
        format_ident!("c{}_{}", number, label)
    }
}

/// The function callable `number`'s table entry runs, taking its values as
/// a list.
pub(crate) fn entry(number: u32) -> Ident {
    format_ident!("s{}", number)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modules_come_from_domains_and_top_folders() {
        assert_eq!(module("domains/billing/actions/pay.jadpo"), "billing");
        assert_eq!(module("domains/match/types/x.jadpo"), "match_");
        assert_eq!(module("shared/types/money.jadpo"), "shared");
        assert_eq!(module("tests/journeys.test.jadpo"), "tests");
        assert_eq!(module("main.jadpo"), "app");
        assert_eq!(module("app.jadpo"), "app");
        assert_eq!(module("2026/x.jadpo"), "m2026");
    }

    #[test]
    fn labels_become_snake_case() {
        assert_eq!(
            snake("test \"a todo is read back by its id\""),
            "test_a_todo_is_read_back_by_its_id"
        );
        assert_eq!(
            snake("function rounding.half_even"),
            "function_rounding_half_even"
        );
        assert!(snake(&"x ".repeat(80)).len() <= 48);
    }
}
