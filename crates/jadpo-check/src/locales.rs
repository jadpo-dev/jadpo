//! The locales an application supports (area 10, "Locales"):
//! `locales { default: Locale.en_gb supported: [Locale.en_gb, Locale.fr_fr] }`
//! in `app.jadpo`, once. Formatting accepts only those, and so do
//! translations; a `Locale` read from a request or a setting is one of
//! them or an invalid value.
//!
//! Read once per project ([`locales`]) into the plan the runtime carries
//! ([`LocalePlan`]) and the problems in how it is written.

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_std::{LOCALE, LOCALES, LocalePlan, locale_ordinal};
use jadpo_syntax::ast::{self, AstNode, Expr};

use crate::config::singletons;
use crate::tree::{self, node_span};
use crate::types::Const;

/// The only options of a `locales { }` declaration.
const OPTIONS: [&str; 2] = ["default", "supported"];

/// The project's locales declaration, with its problems.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Locales {
    /// Whether the project declares `locales { }` at all.
    pub declared: bool,
    /// What it reads cleanly as; `None` when nothing is declared, and
    /// when what is declared has a problem.
    pub plan: Option<LocalePlan>,
    pub problems: Vec<Diagnostic>,
    /// The `locales` keyword, for a problem about the declaration as a
    /// whole.
    pub keyword: Option<Span>,
    /// Each locale `supported:` lists and where it is written, in order.
    pub entries: Vec<(u32, Span)>,
}

/// The `locales { }` of `app.jadpo`.
#[salsa::tracked(returns(ref))]
pub fn locales(db: &dyn Db, project: Project) -> Locales {
    let mut out = Locales::default();
    let mut all = singletons(db, project, "locales").into_iter();
    let Some(first) = all.next() else {
        return out;
    };
    out.declared = true;
    out.keyword = Some(first.keyword.clone());
    for other in all {
        out.problems.push(
            Diagnostic::new(
                &rule::SEMANTIC_MULTIPLE_LOCALE_DECLARATIONS,
                "`locales { }` is declared twice",
            )
            .at(other.keyword.clone(), "declared again")
            .also(first.keyword.clone(), "first declared here")
            .reason("An application lists the locales it supports once, in `app.jadpo`, so formatting and translations all accept the same ones.")
            .next("Move what this `locales` lists into the first, and remove it"),
        );
    }
    let mut reader = Reader {
        path: &first.path,
        problems: &mut out.problems,
        entries: Vec::new(),
    };
    let plan = reader.read(&first);
    let entries = std::mem::take(&mut reader.entries);
    out.plan = plan;
    out.entries = entries;
    out
}

/// The problems in the project's locales declaration.
pub fn locale_problems(db: &dyn Db, project: Project) -> &[Diagnostic] {
    &locales(db, project).problems
}

struct Reader<'a> {
    path: &'a str,
    problems: &'a mut Vec<Diagnostic>,
    entries: Vec<(u32, Span)>,
}

/// The entries of a `supported:` list that read as locales, whether all
/// of them did, and where the list is.
type Listed = (Vec<(u32, Span)>, bool, Span);

/// What `Locale.x` written in source is.
enum Written {
    /// A variant of `Locale`.
    Locale(u32),
    /// Anything else; it has been reported.
    Bad,
}

impl Reader<'_> {
    fn read(&mut self, found: &crate::config::Found) -> Option<LocalePlan> {
        let mut default: Option<(Option<u32>, Span)> = None;
        let mut supported: Option<Listed> = None;
        for (key, member) in tree::members(found.list.clone()) {
            let Some(name) = tree::name_text(&key) else {
                continue;
            };
            let key_span = node_span(self.path, key.syntax());
            match name.as_str() {
                "default" => {
                    if let Some((_, first)) = &default {
                        self.duplicate("default", key_span, first);
                        continue;
                    }
                    let Some(value) = member.value() else {
                        default = Some((None, key_span));
                        continue;
                    };
                    let span = node_span(self.path, value.syntax());
                    let ordinal = match self.locale(&value, &span) {
                        Written::Locale(ordinal) => Some(ordinal),
                        Written::Bad => None,
                    };
                    default = Some((ordinal, span));
                }
                "supported" => {
                    if let Some((_, _, first)) = &supported {
                        self.duplicate("supported", key_span, first);
                        continue;
                    }
                    let Some(value) = member.value() else {
                        supported = Some((Vec::new(), false, key_span));
                        continue;
                    };
                    let span = node_span(self.path, value.syntax());
                    let Expr::ListExpr(list) = &value else {
                        self.problems.push(
                            Diagnostic::new(
                                &rule::TYPE_LOCALE_INVALID,
                                format!(
                                    "`supported:` is a list of locales, not `{}`",
                                    tree::written(value.syntax())
                                ),
                            )
                            .at(span.clone(), "not a list")
                            .reason("`supported:` lists every locale the application accepts, in square brackets: `supported: [Locale.en_gb, Locale.fr_fr]`.")
                            .next("Write the locales in a list, such as `supported: [Locale.en_gb]`"),
                        );
                        supported = Some((Vec::new(), false, span));
                        continue;
                    };
                    let mut entries: Vec<(u32, Span)> = Vec::new();
                    let mut clean = true;
                    for entry in list.exprs() {
                        let at = node_span(self.path, entry.syntax());
                        match self.locale(&entry, &at) {
                            Written::Locale(ordinal) => {
                                if let Some((_, first)) =
                                    entries.iter().find(|(seen, _)| *seen == ordinal)
                                {
                                    let tag = LOCALES
                                        .get(ordinal as usize)
                                        .map_or("", |(variant, _)| *variant);
                                    self.problems.push(
                                        Diagnostic::new(
                                            &rule::TYPE_LOCALE_DUPLICATE,
                                            format!("`Locale.{tag}` is listed twice"),
                                        )
                                        .at(at, "listed again")
                                        .also(first.clone(), "first listed here")
                                        .reason("A locale is supported or it is not; listing it twice says nothing more.")
                                        .next(format!("Remove the second `Locale.{tag}`")),
                                    );
                                    clean = false;
                                } else {
                                    entries.push((ordinal, at));
                                }
                            }
                            Written::Bad => clean = false,
                        }
                    }
                    supported = Some((entries, clean, span));
                }
                other => {
                    let near = nearest(other, OPTIONS);
                    let reason = if other == "unsupported" {
                        "There is no `unsupported:` setting. A request field typed `Locale` is decoded like any enum, so a tag the application doesn't support is an `invalid_value` failure, and choosing a language from an `Accept-Language` header falls back to `default:`.".to_string()
                    } else {
                        format!(
                            "`locales` takes `default:` and `supported:`, and nothing else.{}",
                            near.map_or_else(String::new, |near| format!(
                                " Did you mean `{near}`?"
                            ))
                        )
                    };
                    let problem = Diagnostic::new(
                        &rule::SYNTAX_LOCALES_UNKNOWN_OPTION,
                        format!("`{other}` isn't an option of `locales`"),
                    )
                    .at(key_span.clone(), "unknown option")
                    .reason(reason);
                    self.problems.push(match near {
                        Some(near) if other != "unsupported" => problem
                            .next(format!("Use `{near}` instead of `{other}`"))
                            .edit(key_span, near),
                        _ => problem.next(format!("Remove `{other}:`")),
                    });
                }
            }
        }
        if let Some((entries, ..)) = &supported {
            self.entries = entries.clone();
        }
        let missing_default = default.is_none();
        let missing_supported = supported.is_none();
        if missing_default {
            let example = supported
                .as_ref()
                .and_then(|(entries, ..)| entries.first())
                .and_then(|(ordinal, _)| LOCALES.get(*ordinal as usize))
                .map_or("en_gb", |(variant, _)| *variant);
            self.problems.push(
                Diagnostic::new(
                    &rule::SYNTAX_LOCALES_DEFAULT_REQUIRED,
                    "`locales` has no `default:`",
                )
                .at(found.keyword.clone(), "no `default:`")
                .reason("The default is the locale a choice from an `Accept-Language` header falls back to, so every request ends up with a language the application supports.")
                .next(format!("Add `default: Locale.{example}`")),
            );
        }
        if missing_supported {
            self.problems.push(
                Diagnostic::new(
                    &rule::SYNTAX_LOCALES_SUPPORTED_REQUIRED,
                    "`locales` has no `supported:`",
                )
                .at(found.keyword.clone(), "no `supported:`")
                .reason("`supported:` lists every locale formatting and translations accept; a locale not listed is a compile error where it is written and an invalid value where it is read.")
                .next("Add `supported: [Locale.en_gb]`, listing every locale the application speaks"),
            );
        }
        let (default_ordinal, default_span) =
            default.map(|(o, s)| (o, Some(s))).unwrap_or((None, None));
        let (entries, clean, supported_span) = match supported {
            Some((entries, clean, span)) => (entries, clean, Some(span)),
            None => (Vec::new(), false, None),
        };
        if let (Some(ordinal), Some(at), Some(listed)) = (
            default_ordinal,
            default_span.clone(),
            supported_span.clone(),
        ) && clean
            && !entries.iter().any(|(seen, _)| *seen == ordinal)
        {
            let tag = LOCALES
                .get(ordinal as usize)
                .map_or("", |(variant, _)| *variant);
            self.problems.push(
                Diagnostic::new(
                    &rule::TYPE_LOCALE_DEFAULT_UNSUPPORTED,
                    format!("The default `Locale.{tag}` isn't a supported locale"),
                )
                .at(at, "not in `supported:`")
                .also(listed, "the supported locales")
                .reason("The default is what a request with no usable `Accept-Language` is given, and formatting and translations accept only the supported locales, so the default has to be one.")
                .next(format!("Add `Locale.{tag}` to `supported:`, or choose a default that is listed")),
            );
            return None;
        }
        if !clean || missing_default || missing_supported {
            return None;
        }
        let default = default_ordinal?;
        Some(LocalePlan {
            default,
            supported: entries.into_iter().map(|(ordinal, _)| ordinal).collect(),
        })
    }

    fn duplicate(&mut self, name: &str, at: Span, first: &Span) {
        self.problems.push(
            Diagnostic::new(
                &rule::SYNTAX_LOCALES_DUPLICATE_OPTION,
                format!("`{name}:` is given twice"),
            )
            .at(at, "given again")
            .also(first.clone(), "first given here")
            .reason("`locales` says each of `default:` and `supported:` once.")
            .next(format!("Keep one `{name}:` and remove the other")),
        );
    }

    /// `Locale.en_gb`, or why a value is not one.
    fn locale(&mut self, value: &Expr, span: &Span) -> Written {
        let written = tree::written(value.syntax());
        if let Const::Path(names) = Const::from_expr(value)
            && let [host, variant] = names.as_slice()
            && host == LOCALE
        {
            if let Some(ordinal) = locale_ordinal(variant) {
                return Written::Locale(ordinal);
            }
            let near = nearest(variant, LOCALES.iter().map(|(name, _)| *name));
            let problem = Diagnostic::new(
                &rule::TYPE_LOCALE_INVALID,
                format!("`{written}` isn't a locale"),
            )
            .at(span.clone(), "no such locale")
            .reason(format!(
                "`Locale` has no variant called `{variant}`; a variant is the locale's tag in lower case with `_` for `-`, such as `en_gb` for en-GB.{}",
                near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
            ));
            self.problems.push(match near {
                Some(near) => problem
                    .next(format!("Use `Locale.{near}` instead"))
                    .edit(span.clone(), format!("Locale.{near}")),
                None => problem.next("Use a locale from `Locale`, such as `Locale.en_gb`"),
            });
            return Written::Bad;
        }
        // A tag in quotes is the common slip: `"en-GB"` is `Locale.en_gb`.
        let tagged = match Const::from_expr(value) {
            Const::Str(text) => LOCALES
                .iter()
                .find(|(_, tag)| tag.eq_ignore_ascii_case(&text))
                .map(|(variant, _)| *variant),
            _ => None,
        };
        let problem = Diagnostic::new(
            &rule::TYPE_LOCALE_INVALID,
            format!("`{written}` isn't written as a locale"),
        )
        .at(span.clone(), "not a `Locale`")
        .reason("A locale is a name in `Locale`, written `Locale.<tag>` with the tag in lower case and `_` for `-`: `Locale.en_gb` is en-GB. Closed choices are names, not text, so a misspelt one is caught while compiling.");
        self.problems.push(match tagged {
            Some(variant) => problem
                .next(format!("Write `Locale.{variant}` instead"))
                .edit(span.clone(), format!("Locale.{variant}")),
            None => problem.next("Write a locale from `Locale`, such as `Locale.en_gb`"),
        });
        Written::Bad
    }
}

/// Whether `value` is the literal `Locale.<variant>`, and which variant.
pub fn literal_locale(value: &ast::Expr) -> Option<(u32, String)> {
    let Const::Path(names) = Const::from_expr(value) else {
        return None;
    };
    let [host, variant] = names.as_slice() else {
        return None;
    };
    if host != LOCALE {
        return None;
    }
    Some((locale_ordinal(variant)?, variant.clone()))
}
