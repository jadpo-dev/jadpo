//! Declared time formats (area 10, "Showing dates and times to people"):
//! `type ShortDate = TimeFormat { day: ComponentStyle.numeric ... }` lists
//! the parts of a date or a local time it shows, each once with a style
//! that part takes, and is named only in `temporal.format`'s `format:`,
//! since it holds no value.

use std::collections::BTreeMap;

use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_std::{Component, ComponentStyle};
use jadpo_syntax::ast::{self, AstNode};

use super::decl::Decls;
use super::rules::Const;
use super::table::component_style;
use crate::tree;

/// Each style written out in full, `ComponentStyle.numeric`.
fn styles(component: Component) -> String {
    crate::or_list(
        &component
            .styles()
            .iter()
            .map(|style| format!("`{}.{}`", ComponentStyle::NAME, style.name()))
            .collect::<Vec<_>>(),
    )
}

/// A format named where a value's type goes, built like a value, or used
/// as one.
pub(crate) fn not_a_value(name: &str, span: Span, as_type: bool) -> Diagnostic {
    let (message, place, next) = if as_type {
        (
            format!("`{name}` is a format, not a type of value"),
            "never as the type of a field, a parameter or a variable",
            "and use another type here",
        )
    } else {
        (
            format!("`{name}` is a format, not a value"),
            "never as a value of its own",
            "or use a value here",
        )
    };
    Diagnostic::new(&rule::TYPE_TEMPORAL_FORMAT_INVALID, message)
        .at(span, "a format")
        .reason(format!(
            "A format says how `temporal.format` shows a date or a time; it holds no value, so `{name}` is named only as the `format:` of a call, {place}."
        ))
        .next(format!("Give `format: {name}` to `temporal.format`, {next}"))
}

impl Decls<'_> {
    /// `type Name = TimeFormat { ... }`: each part once, with a style it
    /// takes, and at least one.
    pub(super) fn format_decl(&mut self, decl: &ast::TypeDecl, name: &str) {
        let members = tree::members(decl.member_list());
        if members.is_empty() {
            let span = decl
                .name()
                .map_or_else(|| self.span(decl.syntax()), |n| self.span(n.syntax()));
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_TEMPORAL_FORMAT_COMPONENTS_EMPTY,
                    format!("`{name}` shows nothing"),
                )
                .at(span, "no parts")
                .reason("A format lists the parts of a date or a time it shows, each with a `ComponentStyle`, and the locale arranges them; with none listed there is nothing to show.")
                .next("List the parts to show, such as `day: ComponentStyle.numeric`, or use a built-in format such as `TimeFormat.date_long`"),
            );
            return;
        }
        let mut seen: BTreeMap<Component, Span> = BTreeMap::new();
        for (key, member) in members {
            let Some(written) = tree::name_text(&key) else {
                continue;
            };
            let key_span = self.span(key.syntax());
            let Some(component) = Component::from_name(&written) else {
                let names: Vec<&str> = Component::ALL.iter().map(|c| c.name()).collect();
                let near = nearest(&written, names.iter().copied());
                let listed: Vec<String> = names.iter().map(|n| format!("`{n}`")).collect();
                let problem = Diagnostic::new(
                    &rule::TYPE_UNKNOWN_SETTING,
                    format!("`{written}` isn't a part a format shows"),
                )
                .at(key_span.clone(), "unknown part")
                .reason(format!(
                    "A `TimeFormat` lists the parts it shows, {}, each with a `ComponentStyle`.{}",
                    crate::and_list(&listed),
                    near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
                ));
                self.out.push(match near {
                    Some(near) => problem
                        .next(format!("Use `{near}` instead of `{written}`"))
                        .edit(key_span, near),
                    None => problem.next(format!("Remove `{written}`")),
                });
                continue;
            };
            if let Some(first) = seen.get(&component) {
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_TEMPORAL_FORMAT_COMPONENT_DUPLICATE,
                        format!("`{written}` is given twice"),
                    )
                    .at(key_span, "given again")
                    .also(first.clone(), "first given here")
                    .reason(format!(
                        "A format shows each part once, in one style, so there is never a question of which `{written}` counts."
                    ))
                    .next(format!("Remove one `{written}`")),
                );
                continue;
            }
            seen.insert(component, key_span);
            let Some(value) = member.value() else {
                continue;
            };
            let constant = Const::from_expr(&value);
            if component_style(component, &constant).is_some() {
                continue;
            }
            let value_span = self.span(value.syntax());
            let style = match &constant {
                Const::Path(path) => match path.as_slice() {
                    [host, style] if host == ComponentStyle::NAME => {
                        ComponentStyle::from_name(style)
                    }
                    _ => None,
                },
                _ => None,
            };
            let first = component.styles().first().copied().unwrap_or(ComponentStyle::Numeric);
            let fix = format!("{}.{}", ComponentStyle::NAME, first.name());
            let (message, label) = match style {
                Some(style) => (
                    format!("`{written}` isn't shown `{}`", style.name()),
                    format!("not a style `{written}` takes"),
                ),
                None => (
                    format!("`{written}` takes a `ComponentStyle`"),
                    format!("{} here", constant.describe()),
                ),
            };
            self.out.push(
                Diagnostic::new(&rule::TYPE_TEMPORAL_FORMAT_COMPONENT_INVALID, message)
                    .at(value_span.clone(), label)
                    .reason(format!(
                        "A format's `{written}` is shown {}; the locale decides the rest, such as the order and the punctuation.",
                        styles(component)
                    ))
                    .next(format!("Write `{written}: {fix}`"))
                    .edit(value_span, fix),
            );
        }
    }
}
