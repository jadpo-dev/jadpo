//! Translations (area 10, "Translations"): `translations/<locale>.json`,
//! one file per supported locale of ICU MessageFormat messages. The default
//! locale's file defines the keys; each becomes the typed call
//! `translations.<key>(locale:, ...)`, and every other file is checked to
//! have the same keys and, for each key, the same placeholders.
//!
//! The work is in four queries, so an edit rechecks only what it affects:
//!
//! - [`translation_files`] reads every file into messages (nothing else
//!   reads the text), with the problems each file has on its own;
//! - [`translation_problems`] compares the files with each other and with
//!   the locales the application declares;
//! - [`lookup`] is what one call asks: the key's placeholders, as the
//!   default locale's message gives them. It is a query per key, so a call
//!   depends on the key it names and on nothing in the words of any
//!   message;
//! - [`plan`] is what the runtime formats from, built once the project
//!   checks.

mod json;

use std::collections::BTreeMap;

use jadpo_db::{Db, Project, Record};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_std::translation::{
    self, Conflict, Message, MessageErrorKind, ParamKind, Placeholder, TranslationKey,
    TranslationParam, TranslationPlan,
};
use jadpo_std::{LOCALES, LocalePlan, locale_ordinal};

use crate::locales::{Locales, locales};
use crate::lookups::Name;

/// One key of a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    pub key_span: Span,
    /// The message with its quotes, in the file.
    pub message_span: Span,
    /// The message as written, with its escapes read.
    pub text: String,
    /// The message, when it read.
    pub message: Option<Message>,
    /// Its placeholders, when the message read and they agree.
    pub placeholders: Option<Vec<Placeholder>>,
}

/// One translation file, read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct File {
    pub path: String,
    /// The file's name without `.json`: the locale it is for, in the
    /// spelling `Locale.<tag>` uses.
    pub tag: String,
    /// That locale's place among `Locale`'s variants, when the name is one.
    pub locale: Option<u32>,
    /// Whether the file is an object of messages; none of its entries are
    /// known when it isn't.
    pub readable: bool,
    /// Where the object opens (the start of the file when it doesn't).
    pub open: Span,
    /// The keys that are names a call can use, once each, in file order.
    pub entries: Vec<Entry>,
    /// What is wrong with the file by itself.
    pub problems: Vec<Diagnostic>,
}

impl File {
    fn entry(&self, key: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.key == key)
    }
}

/// Every translation file, read, in path order.
#[salsa::tracked(returns(ref))]
pub fn translation_files(db: &dyn Db, project: Project) -> Vec<File> {
    if project.layout(db) != jadpo_db::Layout::Project {
        return Vec::new();
    }
    project
        .translations(db)
        .iter()
        .map(read_file)
        .collect()
}

/// A key a call can use: lower_snake_case, as every name in a call is.
fn is_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && !key.ends_with('_')
        && !key.contains("__")
}

/// The key `key` would be if it were written as a name: `Order-Shipped`
/// and `orderShipped` are `order_shipped`.
fn as_key(key: &str) -> Option<String> {
    let mut out = String::new();
    let mut previous_lower = false;
    for c in key.chars() {
        if c.is_ascii_uppercase() {
            if previous_lower {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
            previous_lower = false;
        } else if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
            previous_lower = true;
        } else if matches!(c, '-' | ' ' | '_' | '.') {
            if !out.ends_with('_') && !out.is_empty() {
                out.push('_');
            }
            previous_lower = false;
        } else {
            return None;
        }
    }
    let out = out.trim_end_matches('_').to_string();
    is_key(&out).then_some(out)
}

fn span(path: &str, range: (u32, u32)) -> Span {
    Span::new(path, range.0, range.1)
}

fn read_file(record: &Record) -> File {
    let path = record.path.clone();
    let tag = path
        .rsplit('/')
        .next()
        .and_then(|name| name.strip_suffix(".json"))
        .unwrap_or("")
        .to_string();
    let locale = locale_ordinal(&tag);
    let mut file = File {
        locale,
        open: Span::new(&path, 0, 0),
        readable: false,
        entries: Vec::new(),
        problems: Vec::new(),
        tag,
        path,
    };
    let text = match &record.text {
        Ok(text) => text,
        Err(error) => {
            file.problems.push(
                Diagnostic::new(
                    &rule::TRANSLATION_FILE_INVALID,
                    format!("`{}` couldn't be read", file.path),
                )
                .at(Span::new(&file.path, 0, 0), "")
                .reason(format!("Reading it failed: {error}."))
                .next("Check it can be read, then run `jadpo check` again"),
            );
            return file;
        }
    };
    let object = match json::read(text) {
        Ok(object) => object,
        Err(problem) => {
            file.problems.push(
                Diagnostic::new(
                    &rule::TRANSLATION_FILE_INVALID,
                    format!("`{}` isn't a JSON object of messages", file.path),
                )
                .at(span(&file.path, problem.range), problem.label)
                .reason(problem.reason)
                .next(problem.next),
            );
            return file;
        }
    };
    file.readable = true;
    file.open = Span::new(&file.path, object.open, object.open + 1);
    let mut seen: BTreeMap<String, Span> = BTreeMap::new();
    for pair in object.pairs {
        let key_span = span(&file.path, pair.key_range);
        if let Some(first) = seen.get(&pair.key) {
            file.problems.push(
                Diagnostic::new(
                    &rule::TRANSLATION_KEY_DUPLICATE,
                    format!("`{}` is written twice in `{}`", pair.key, file.path),
                )
                .at(key_span, "written again")
                .also(first.clone(), "first written here")
                .reason("A key names one message; the second would be dropped or the first, whichever a reader of the file believed.")
                .next(format!("Remove one `{}`, or give it another name", pair.key)),
            );
            continue;
        }
        seen.insert(pair.key.clone(), key_span.clone());
        if !is_key(&pair.key) {
            let suggestion = as_key(&pair.key);
            let problem = Diagnostic::new(
                &rule::TRANSLATION_KEY_INVALID,
                format!("`{}` isn't a key a call can use", pair.key),
            )
            .at(key_span.clone(), "not lower_snake_case")
            .reason(format!(
                "Each key becomes a call, `translations.{}(...)`, so a key is a lower_snake_case name: lower-case letters, digits and single underscores, starting with a letter.",
                suggestion.as_deref().unwrap_or("order_shipped")
            ));
            file.problems.push(match suggestion {
                Some(name) => problem
                    .next(format!("Rename the key to `{name}`, here and in every locale's file"))
                    .edit(key_span, format!("\"{name}\"")),
                None => problem.next("Rename the key to a lower_snake_case name"),
            });
            continue;
        }
        let message_span = span(&file.path, pair.message_range);
        let message = match translation::parse(&pair.message) {
            Ok(message) => Some(message),
            Err(problem) => {
                // The message's byte offsets, as the file wrote them.
                let last = pair.message_range.1.saturating_sub(1);
                let start = pair.offsets.get(problem.start).copied().unwrap_or(last);
                let end = pair.offsets.get(problem.end).copied().unwrap_or(last);
                let range = (start, if end > start { end } else { start + 1 });
                let rule = match problem.kind {
                    MessageErrorKind::Syntax => &rule::TRANSLATION_MESSAGE_INVALID,
                    MessageErrorKind::OtherMissing => &rule::TRANSLATION_OTHER_MISSING,
                };
                let title = match problem.kind {
                    MessageErrorKind::Syntax => {
                        format!("The message for `{}` isn't valid MessageFormat", pair.key)
                    }
                    MessageErrorKind::OtherMissing => format!(
                        "The message for `{}` has a case list with no `other`",
                        pair.key
                    ),
                };
                file.problems.push(
                    Diagnostic::new(rule, title)
                        .at(span(&file.path, range), problem.label)
                        .reason(problem.reason)
                        .next(problem.next),
                );
                None
            }
        };
        let placeholders = match &message {
            Some(message) => match message.placeholders() {
                Ok(found) => Some(found),
                Err(Conflict {
                    name,
                    first,
                    second,
                }) => {
                    file.problems.push(
                        Diagnostic::new(
                            &rule::TRANSLATION_PLACEHOLDER_TYPE,
                            format!(
                                "`{{{name}}}` is used as {} and as {} in one message",
                                first.word(),
                                second.word()
                            ),
                        )
                        .at(message_span.clone(), "one placeholder, two kinds of value")
                        .reason(format!(
                            "A call gives `{name}:` one argument, so the message uses it as one kind of value: `{{{name}}}` is text, `{{{name}, number}}` and a plural's count are numbers, `{{{name}, date}}` and `{{{name}, time}}` are moments."
                        ))
                        .next(format!("Use `{name}` one way in `{}`", pair.key)),
                    );
                    None
                }
            },
            None => None,
        };
        file.entries.push(Entry {
            key: pair.key,
            key_span,
            message_span,
            text: pair.message,
            message,
            placeholders,
        });
    }
    file
}

/// What the files and the locales say, compared: the problems no single
/// file has.
#[salsa::tracked(returns(ref))]
pub fn translation_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let files = translation_files(db, project);
    let mut out: Vec<Diagnostic> = files
        .iter()
        .flat_map(|file| file.problems.iter().cloned())
        .collect();
    if files.is_empty() {
        return out;
    }
    let declared = locales(db, project);
    if !declared.declared {
        if let Some(first) = files.first() {
            out.push(
                Diagnostic::new(
                    &rule::TRANSLATION_LOCALES_REQUIRED,
                    "Translations need the application's locales",
                )
                .at(Span::new(&first.path, 0, 0), "no locales declared")
                .reason("Each translation file is for one of the locales the application supports, and the default locale's file defines the keys. None is declared, so there is nothing to check the files against.")
                .next("Declare them in `app.jadpo`, for instance `locales { default: Locale.en_gb supported: [Locale.en_gb] }`"),
            );
        }
        return out;
    }
    for file in files {
        if file.locale.is_none() {
            out.push(not_a_locale(file));
        }
    }
    let Some(plan) = &declared.plan else {
        return out;
    };
    out.extend(against_locales(files, declared, plan));
    out
}

fn not_a_locale(file: &File) -> Diagnostic {
    let near = nearest(&file.tag, LOCALES.iter().map(|(name, _)| *name)).map(str::to_string);
    let folded = LOCALES
        .iter()
        .find(|(_, tag)| tag.eq_ignore_ascii_case(&file.tag))
        .map(|(name, _)| name.to_string());
    let better = folded.or(near);
    let problem = Diagnostic::new(
        &rule::TRANSLATION_FILE_NAME_INVALID,
        format!("`{}` isn't named for a locale", file.path),
    )
    .at(file.open.clone(), format!("`{}` is no `Locale`", file.tag))
    .reason(format!(
        "A translation file is named for the locale it translates into, the tag in lower case with `_` for `-`: `en_gb.json` is `Locale.en_gb`.{}",
        better
            .as_ref()
            .map_or_else(String::new, |name| format!(" Did you mean `{name}`?"))
    ));
    match better {
        Some(name) => problem.next(format!("Rename the file to `translations/{name}.json`")),
        None => problem.next("Rename the file after one of the application's supported locales, such as `translations/en_gb.json`"),
    }
}

fn variant(ordinal: u32) -> &'static str {
    LOCALES
        .get(ordinal as usize)
        .map_or("", |(variant, _)| *variant)
}

/// The files against the supported locales and each other.
fn against_locales(files: &[File], declared: &Locales, plan: &LocalePlan) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let listed = declared.keyword.clone();
    for file in files {
        let Some(locale) = file.locale else { continue };
        if !plan.supports(locale) {
            let mut problem = Diagnostic::new(
                &rule::TRANSLATION_LOCALE_UNSUPPORTED,
                format!("`{}` is for a locale this application doesn't support", file.path),
            )
            .at(file.open.clone(), format!("`Locale.{}` isn't supported", file.tag))
            .reason("Translations are written in the locales listed in `locales { supported: }`, in `app.jadpo`; a file for another is never used, and the server never guesses a language.");
            if let Some(at) = &listed {
                problem = problem.also(at.clone(), "the supported locales");
            }
            out.push(problem.next(format!(
                "Add `Locale.{}` to `supported:` in `app.jadpo`, or remove the file",
                file.tag
            )));
        }
    }
    let by_locale = |ordinal: u32| files.iter().find(|file| file.locale == Some(ordinal));
    for &ordinal in &plan.supported {
        if by_locale(ordinal).is_none() {
            let at = declared
                .entries
                .iter()
                .find(|(seen, _)| *seen == ordinal)
                .map(|(_, at)| at.clone())
                .or_else(|| listed.clone());
            let default = ordinal == plan.default;
            let tag = variant(ordinal);
            let reason = if default {
                format!(
                    "The default locale's file defines the keys, so `translations/{tag}.json` is where every translation key is written; the other locales' files are checked against it."
                )
            } else {
                format!(
                    "There is one file for each supported locale, so a call with `locale: Locale.{tag}` has a message to show."
                )
            };
            if let Some(at) = at {
                out.push(
                    Diagnostic::new(
                        &rule::TRANSLATION_FILE_MISSING,
                        format!("`Locale.{tag}` is supported but has no translation file"),
                    )
                    .at(at, format!("no `translations/{tag}.json`"))
                    .reason(reason)
                    .next(format!("Add `translations/{tag}.json` with the same keys as the default locale's file")),
                );
            }
        }
    }
    let Some(default) = by_locale(plan.default).filter(|file| file.readable) else {
        return out;
    };
    for file in files {
        let Some(locale) = file.locale else { continue };
        if locale == plan.default || !plan.supports(locale) || !file.readable {
            continue;
        }
        out.extend(compare(default, file));
    }
    out
}

/// `file` against the default locale's file.
fn compare(default: &File, file: &File) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    // Keys that are invalid or repeated in `file` have been reported there.
    for entry in &default.entries {
        let Some(other) = file.entry(&entry.key) else {
            let near = nearest(
                &entry.key,
                file.entries
                    .iter()
                    .filter(|e| default.entry(&e.key).is_none())
                    .map(|e| e.key.as_str()),
            );
            let mut problem = Diagnostic::new(
                &rule::TRANSLATION_KEY_MISSING,
                format!("`{}` has no translation for `{}`", file.path, entry.key),
            )
            .at(file.open.clone(), format!("no `{}`", entry.key))
            .also(entry.key_span.clone(), format!("defined in `{}`", default.path))
            .reason(format!(
                "`{}` defines the keys, and every locale's file has all of them: a call of `translations.{}` in this locale would have no message.",
                default.path, entry.key
            ));
            problem = match near {
                Some(near) => problem.next(format!(
                    "Add `\"{}\"` to `{}`; `{near}` may be it, misspelt",
                    entry.key, file.path
                )),
                None => problem.next(format!("Add `\"{}\"` and its message to `{}`", entry.key, file.path)),
            };
            out.push(problem);
            continue;
        };
        let (Some(wanted), Some(given)) = (&entry.placeholders, &other.placeholders) else {
            continue;
        };
        out.extend(placeholders(default, entry, file, other, wanted, given));
    }
    for entry in &file.entries {
        if default.entry(&entry.key).is_some() {
            continue;
        }
        let near = nearest(
            &entry.key,
            default
                .entries
                .iter()
                .filter(|e| file.entry(&e.key).is_none())
                .map(|e| e.key.as_str()),
        );
        let problem = Diagnostic::new(
            &rule::TRANSLATION_KEY_EXTRA,
            format!("`{}` has `{}`, which `{}` doesn't", file.path, entry.key, default.path),
        )
        .at(entry.key_span.clone(), "not in the default locale's file")
        .reason(format!(
            "`{}` defines the keys, and a key that file lacks has no call, so nothing would ever show this message.",
            default.path
        ));
        out.push(match near {
            Some(near) => problem
                .next(format!("Rename it to `{near}`, which `{}` has", default.path))
                .edit(entry.key_span.clone(), format!("\"{near}\"")),
            None => problem.next(format!(
                "Remove it, or add `\"{}\"` to `{}` first",
                entry.key, default.path
            )),
        });
    }
    out
}

fn placeholders(
    default: &File,
    entry: &Entry,
    file: &File,
    other: &Entry,
    wanted: &[Placeholder],
    given: &[Placeholder],
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let missing: Vec<&Placeholder> = wanted
        .iter()
        .filter(|w| given.iter().all(|g| g.name != w.name))
        .collect();
    let extra: Vec<&Placeholder> = given
        .iter()
        .filter(|g| wanted.iter().all(|w| w.name != g.name))
        .collect();
    if !missing.is_empty() || !extra.is_empty() {
        let braces = |list: &[&Placeholder]| -> String {
            let listed: Vec<String> = list.iter().map(|p| format!("`{{{}}}`", p.name)).collect();
            crate::and_list(&listed)
        };
        let (title, label) = match (missing.is_empty(), extra.is_empty()) {
            (false, true) => (
                format!("The message for `{}` in `{}` has no {}", entry.key, file.path, braces(&missing)),
                format!("no {}", braces(&missing)),
            ),
            (true, false) => (
                format!(
                    "The message for `{}` in `{}` has {}, which `{}` doesn't",
                    entry.key, file.path, braces(&extra), default.path
                ),
                format!("{} isn't in the default message", braces(&extra)),
            ),
            _ => (
                format!(
                    "The message for `{}` in `{}` has different placeholders from `{}`",
                    entry.key, file.path, default.path
                ),
                format!("has {}, lacks {}", braces(&extra), braces(&missing)),
            ),
        };
        let have: Vec<String> = wanted.iter().map(|p| format!("`{{{}}}`", p.name)).collect();
        out.push(
            Diagnostic::new(&rule::TRANSLATION_PLACEHOLDER_MISMATCH, title)
                .at(other.message_span.clone(), label)
                .also(entry.message_span.clone(), format!("the default message, in `{}`", default.path))
                .reason(format!(
                    "A call gives one argument for each placeholder of the default locale's message, {}, and every locale's message is shown with those and no others.",
                    if have.is_empty() { "which has none".to_string() } else { format!("here {}", crate::and_list(&have)) }
                ))
                .next(match (missing.is_empty(), extra.is_empty()) {
                    (false, _) => format!("Use {} in the message, in its place in the sentence", braces(&missing)),
                    _ => format!("Remove {} from the message, or add it to `{}` first", braces(&extra), default.path),
                }),
        );
        return out;
    }
    for w in wanted {
        let Some(g) = given.iter().find(|g| g.name == w.name) else {
            continue;
        };
        if g.kind != w.kind {
            out.push(
                Diagnostic::new(
                    &rule::TRANSLATION_PLACEHOLDER_TYPE,
                    format!(
                        "`{{{}}}` is {} in `{}` but {} in `{}`",
                        w.name,
                        w.kind.word(),
                        default.path,
                        g.kind.word(),
                        file.path
                    ),
                )
                .at(other.message_span.clone(), format!("used as {}", g.kind.word()))
                .also(entry.message_span.clone(), format!("{} in the default message", w.kind.word()))
                .reason(format!(
                    "The call gives `{}:` one argument, the kind the default locale's message uses it as, so every locale's message uses it the same way.",
                    w.name
                ))
                .next(format!("Use `{}` as {} in `{}`", w.name, w.kind.word(), file.path)),
            );
        }
    }
    out
}

// --- what a call asks ---------------------------------------------------------------

/// What the files say about the key a call names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lookup {
    /// The project has no translation files.
    Absent,
    /// The default locale's file can't say (it is missing, or has a
    /// problem reported elsewhere), so no call is checked against it.
    Broken,
    /// The default locale's file has no such key.
    Unknown,
    /// The key and its placeholders, when its message read.
    Key(Option<Vec<Placeholder>>),
}

/// The default locale's file, when it is there and an object.
fn default_file(db: &dyn Db, project: Project) -> Result<Option<&File>, ()> {
    let files = translation_files(db, project);
    if files.is_empty() {
        return Ok(None);
    }
    let Some(plan) = &locales(db, project).plan else {
        return Err(());
    };
    match files
        .iter()
        .find(|file| file.locale == Some(plan.default) && file.readable)
    {
        Some(file) => Ok(Some(file)),
        None => Err(()),
    }
}

/// What the files say about the key `name`.
#[salsa::tracked(returns(ref))]
pub fn lookup<'db>(db: &'db dyn Db, project: Project, name: Name<'db>) -> Lookup {
    match default_file(db, project) {
        Ok(None) => Lookup::Absent,
        Err(()) => Lookup::Broken,
        Ok(Some(file)) => match file.entry(name.text(db)) {
            Some(entry) => Lookup::Key(entry.placeholders.clone()),
            None => Lookup::Unknown,
        },
    }
}

/// The default locale's file, when the project has one that reads.
pub fn default_translation_file(db: &dyn Db, project: Project) -> Option<&File> {
    default_file(db, project).ok().flatten()
}

/// The text of every message of a key, in each supported locale's file in
/// the order the locales are listed: what a change to any of them changes.
pub fn messages_of<'a>(
    db: &'a dyn Db,
    project: Project,
    key: &str,
) -> Vec<(&'a str, &'a str)> {
    let supported = locales(db, project)
        .plan
        .as_ref()
        .map(|plan| plan.supported.clone())
        .unwrap_or_default();
    let files = translation_files(db, project);
    supported
        .iter()
        .filter_map(|&ordinal| {
            let file = files.iter().find(|file| file.locale == Some(ordinal))?;
            let entry = file.entry(key)?;
            Some((file.tag.as_str(), entry.text.as_str()))
        })
        .collect()
}

/// Every key the default locale's file defines, for the nearest name a
/// mistyped call might have meant.
pub fn keys(db: &dyn Db, project: Project) -> Vec<String> {
    match default_file(db, project) {
        Ok(Some(file)) => file.entries.iter().map(|entry| entry.key.clone()).collect(),
        _ => Vec::new(),
    }
}

/// The plan the runtime formats from: every key of the default locale's
/// file, with its message in each supported locale that has one. Meant for
/// a project that checks; keys with no readable default message are left
/// out.
pub fn plan(db: &dyn Db, project: Project) -> TranslationPlan {
    let Ok(Some(default)) = default_file(db, project) else {
        return TranslationPlan::default();
    };
    let files = translation_files(db, project);
    let supported = locales(db, project)
        .plan
        .as_ref()
        .map(|plan| plan.supported.clone())
        .unwrap_or_default();
    let mut keys: Vec<TranslationKey> = Vec::new();
    for entry in &default.entries {
        let Some(placeholders) = &entry.placeholders else {
            continue;
        };
        let messages = supported
            .iter()
            .filter_map(|&ordinal| {
                let file = files.iter().find(|file| file.locale == Some(ordinal))?;
                let message = file.entry(&entry.key)?.message.clone()?;
                Some((ordinal, message))
            })
            .collect();
        keys.push(TranslationKey {
            name: entry.key.clone(),
            params: placeholders
                .iter()
                .map(|p| TranslationParam {
                    name: p.name.clone(),
                    kind: p.kind,
                })
                .collect(),
            messages,
        });
    }
    keys.sort_by(|a, b| a.name.cmp(&b.name));
    TranslationPlan { keys }
}

/// A placeholder's kind, as the value a call gives it must be built.
pub fn accepts(kind: ParamKind) -> &'static [crate::types::Prim] {
    use crate::types::Prim;
    match kind {
        ParamKind::Text => &[Prim::Text, Prim::PresentationText],
        ParamKind::Number => &[Prim::Int, Prim::Decimal],
        ParamKind::Date => &[Prim::CalendarDate, Prim::ZonedTime],
        ParamKind::Time => &[Prim::ZonedTime],
    }
}
