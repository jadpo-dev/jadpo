//! `suggestions { project_token: "…" share: false }` in `app.jadpo` (area
//! 14, "Sharing with Jadpo HQ"; chunk 5.5y): the project's token, random
//! and tied to nothing, and whether `jadpo suggest share` may send at all.
//! Read once per project ([`settings`]) with the problems in how it is
//! written; `jadpo suggest share` reads it, and writes the token.

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_syntax::ast::AstNode;

use crate::config::singletons;
use crate::tree::{self, node_span};
use crate::types::Const;

/// The only options of a `suggestions { }` declaration.
pub const OPTIONS: [&str; 2] = ["project_token", "share"];

/// How many hex digits a project token has.
pub const TOKEN_DIGITS: usize = 32;

/// The project's suggestions settings, with their problems.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    /// Where the first `suggestions { }` is: its keyword.
    pub keyword: Option<Span>,
    /// The offset just after its `{`, where a member can be added.
    pub open: Option<u32>,
    /// The token, when it reads as one, and where its literal is.
    pub token: Option<(String, Span)>,
    /// `share:`, when it reads as `true` or `false`, and where.
    pub share: Option<(bool, Span)>,
    pub problems: Vec<Diagnostic>,
}

/// Whether `text` is a token Jadpo writes: 32 lower-case hex digits.
pub fn is_token(text: &str) -> bool {
    text.len() == TOKEN_DIGITS
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The project's `suggestions { }`.
#[salsa::tracked(returns(ref))]
pub fn settings(db: &dyn Db, project: Project) -> Settings {
    let mut out = Settings::default();
    let mut all = singletons(db, project, "suggestions").into_iter();
    let Some(first) = all.next() else {
        return out;
    };
    out.keyword = Some(first.keyword.clone());
    for other in all {
        out.problems.push(
            Diagnostic::new(
                &rule::SEMANTIC_MULTIPLE_SUGGESTIONS,
                "`suggestions { }` is declared twice",
            )
            .at(other.keyword.clone(), "declared again")
            .also(first.keyword.clone(), "first declared here")
            .reason("A project has one token and one answer to whether it shares, in one `suggestions { }` in `app.jadpo`.")
            .next("Move what this `suggestions` holds into the first, and remove it"),
        );
    }
    let path = first.path.clone();
    if let Some(list) = &first.list
        && let Some(brace) = list.syntax().first_token()
    {
        out.open = Some(u32::from(brace.text_range().end()));
    }
    let mut seen: Vec<(String, Span)> = Vec::new();
    for (key, member) in tree::members(first.list.clone()) {
        let Some(name) = tree::name_text(&key) else {
            continue;
        };
        let key_span = node_span(&path, key.syntax());
        if !OPTIONS.contains(&name.as_str()) {
            let diagnostic = Diagnostic::new(
                &rule::SYNTAX_SUGGESTIONS_UNKNOWN_OPTION,
                format!("`suggestions` has no option `{name}:`"),
            )
            .at(key_span, "no such option");
            out.problems.push(match nearest(&name, OPTIONS) {
                Some(near) => diagnostic
                    .reason(format!(
                        "`suggestions` takes `project_token:` and `share:`. Did you mean `{near}`?"
                    ))
                    .next(format!("Use `{near}:` instead of `{name}:`")),
                None => diagnostic
                    .reason("`suggestions` takes `project_token:`, which `jadpo suggest share` writes, and `share:`, which turns sharing off with `false`.")
                    .next(format!("Remove `{name}:`")),
            });
            continue;
        }
        if let Some((_, at)) = seen.iter().find(|(seen, _)| *seen == name) {
            out.problems.push(
                Diagnostic::new(
                    &rule::SYNTAX_SUGGESTIONS_DUPLICATE_OPTION,
                    format!("`{name}:` is given twice"),
                )
                .at(key_span, "given again")
                .also(at.clone(), "first given here")
                .reason("`suggestions` says each of `project_token:` and `share:` once.")
                .next(format!("Keep one `{name}:` and remove the other")),
            );
            continue;
        }
        seen.push((name.clone(), key_span.clone()));
        let Some(value) = member.value() else {
            out.problems.push(invalid(&name, key_span, None));
            continue;
        };
        let span = node_span(&path, value.syntax());
        match (name.as_str(), Const::from_expr(&value)) {
            ("project_token", Const::Str(text)) if is_token(&text) => {
                out.token = Some((text, span));
            }
            ("share", Const::Bool(on)) => out.share = Some((on, span)),
            _ => {
                let written = tree::written(value.syntax());
                out.problems.push(invalid(&name, span, Some(&written)));
            }
        }
    }
    out
}

fn invalid(name: &str, at: Span, written: Option<&str>) -> Diagnostic {
    let message = match written {
        Some(written) => format!("`{name}: {written}` isn't a value `{name}` takes"),
        None => format!("`{name}:` has no value"),
    };
    let (label, reason, next) = if name == "project_token" {
        (
            "not a token",
            "A project token is 32 random hex digits, tied to nothing, so Jadpo HQ can count distinct projects without knowing which they are.",
            "Remove `project_token:`; `jadpo suggest share` writes a new one",
        )
    } else {
        (
            "not true or false",
            "`share: false` turns sharing off for the project; `share: true` is the same as leaving it out.",
            "Write `share: false` to turn sharing off, or remove `share:`",
        )
    };
    Diagnostic::new(&rule::TYPE_SUGGESTIONS_VALUE_INVALID, message)
        .at(at, label)
        .reason(reason)
        .next(next)
}

/// The problems in the project's suggestions settings.
pub fn settings_problems(db: &dyn Db, project: Project) -> &[Diagnostic] {
    &settings(db, project).problems
}
