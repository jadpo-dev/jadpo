//! What `jadpo translations export` and `import` exchange with translation
//! services (area 10, "Tooling"): every message, enum, failure and email
//! subject the project words, and Jadpo's own messages, as units of
//! wording, one for each plain wording, enum value or plural form; the
//! wording a locale has for them; and how a unit of wording is written back
//! into a language file.
//!
//! This is the reading and the writing of Jadpo's side. XLIFF and JSON,
//! and the checking of what comes back, belong to the command.

use std::collections::BTreeSet;

use jadpo_db::{Db, File, Project};
use jadpo_std::jadpo_text::{GROUP, Shape as ShippedShape};
use jadpo_std::translation::plural_categories;
use jadpo_std::{LOCALES, LocalePlan};

use super::declared::{Kind, Mode, Subject, language_files, scopes, subjects};
use super::failures::failure_scopes;
use super::language::{Body, LanguageFile, Member, language_file};
use super::sources::{self, ROOT};
use crate::lookups::Name;

/// The name Jadpo's own messages are exchanged under: the unit ids of the
/// root folder's `jadpo: { }` group.
pub const JADPO: &str = GROUP;

/// How a unit of wording is chosen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shape {
    /// One wording.
    Plain,
    /// One for each value of an enum, by the field that holds it.
    Choice { field: String, values: Vec<String> },
    /// One for each plural form of the language, by the count's field.
    Plural { field: String },
}

/// What can be worded: a message, an enum with wording, a failure, an
/// email's subject, or one of Jadpo's own messages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The domain, `shared`, or [`JADPO`].
    pub scope: String,
    /// The member that words it in a language file.
    pub member: String,
    pub kind: Kind,
    /// The `///` description of its type.
    pub description: Option<String>,
    /// What a wording may write as `{name}`.
    pub placeholders: Vec<String>,
    pub shape: Shape,
    /// The default locale's wording, as `(value or plural form, text)`; the
    /// value is `None` for a plain wording.
    pub source: Vec<(Option<String>, String)>,
}

impl Entry {
    /// The id the exchange files give a unit, `scope.member` and, for a
    /// wording chosen by a value or a plural form, `.value`.
    pub fn id(&self, variant: Option<&str>) -> String {
        match variant {
            Some(variant) => format!("{}.{}.{variant}", self.scope, self.member),
            None => format!("{}.{}", self.scope, self.member),
        }
    }

    /// The units a locale words this in: one, a value each, or a plural
    /// form each.
    pub fn variants(&self, locale: u32) -> Vec<Option<String>> {
        match &self.shape {
            Shape::Plain => vec![None],
            Shape::Choice { values, .. } => values.iter().cloned().map(Some).collect(),
            Shape::Plural { .. } => plural_categories(locale)
                .iter()
                .map(|category| Some(category.word().to_string()))
                .collect(),
        }
    }

    /// The default wording a unit is translated from. A plural form the
    /// default locale does not have is translated from its `other`.
    pub fn source_for(&self, variant: Option<&str>) -> Option<&str> {
        let find = |wanted: Option<&str>| {
            self.source
                .iter()
                .find(|(seen, _)| seen.as_deref() == wanted)
                .map(|(_, text)| text.as_str())
        };
        find(variant).or_else(|| {
            matches!(self.shape, Shape::Plural { .. })
                .then(|| find(Some("other")))
                .flatten()
        })
    }

    /// The name a record gives it.
    pub fn record_member(&self) -> String {
        if self.scope == JADPO {
            super::jadpo::member_name(&self.member)
        } else {
            self.member.clone()
        }
    }

    /// The scope its record is kept under.
    pub fn record_scope(&self) -> &str {
        if self.scope == JADPO {
            ROOT
        } else {
            &self.scope
        }
    }

    /// The project-relative path of the language file for a locale.
    pub fn language_path(&self, tag: &str) -> String {
        language_path(&self.scope, tag)
    }

    /// What it words, said for a translator.
    pub fn context(&self, variant: Option<&str>) -> Option<String> {
        match (&self.shape, variant) {
            (Shape::Choice { field, .. }, Some(value)) => {
                Some(format!("Used when `{field}` is `{value}`."))
            }
            (Shape::Plural { field }, Some(form)) => {
                Some(format!("Used for the plural form `{form}` of `{field}`."))
            }
            _ => None,
        }
    }
}

/// The project-relative path of a scope's language file for a locale.
pub fn language_path(scope: &str, tag: &str) -> String {
    match scope {
        JADPO => format!("translations/{tag}.jadpo"),
        super::SHARED => format!("shared/translations/{tag}.jadpo"),
        domain => format!("domains/{domain}/translations/{tag}.jadpo"),
    }
}

/// A supported locale with translations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    /// Its place among `Locale`'s variants.
    pub ordinal: u32,
    /// The spelling `Locale.<tag>` uses.
    pub tag: &'static str,
    /// The BCP 47 tag translation services use.
    pub bcp47: &'static str,
    pub in_progress: bool,
}

/// Everything there is to exchange.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Catalogue {
    pub default: Target,
    /// The supported locales besides the default and the pseudo-locale.
    pub targets: Vec<Target>,
    pub entries: Vec<Entry>,
    /// A domain named like Jadpo's own group would share its unit ids.
    pub clash: Option<String>,
}

fn target(ordinal: u32, plan: &LocalePlan) -> Target {
    let (tag, bcp47) = LOCALES.get(ordinal as usize).copied().unwrap_or(("", ""));
    Target {
        ordinal,
        tag,
        bcp47,
        in_progress: plan.in_progress.contains(&ordinal),
    }
}

/// The locale with this spelling, as `Locale.<tag>` or as BCP 47 (`de_de`,
/// `de-DE`, `de-de`).
pub fn locale_named(text: &str) -> Option<u32> {
    let lowered = text.to_ascii_lowercase().replace('-', "_");
    LOCALES
        .iter()
        .position(|(tag, _)| *tag == lowered)
        .and_then(|at| u32::try_from(at).ok())
}

fn plain_text(body: &Body) -> Vec<(Option<String>, String)> {
    match body {
        Body::Text(written) => vec![(None, written.text.clone())],
        Body::Variants(variants) => variants
            .iter()
            .filter_map(|variant| {
                Some((
                    Some(variant.name.clone()),
                    variant.written.as_ref()?.text.clone(),
                ))
            })
            .collect(),
        Body::Invalid => Vec::new(),
    }
}

/// Everything the project words, for the locales it supports. `None` when
/// it declares no locales.
pub fn catalogue(db: &dyn Db, project: Project) -> Option<Catalogue> {
    let declared = crate::locales::locales(db, project);
    let plan = declared.plan.as_ref()?;
    let default = target(plan.default, plan);
    let targets = plan
        .supported
        .iter()
        .copied()
        .filter(|&locale| locale != plan.default && !LocalePlan::is_pseudo(locale))
        .map(|locale| target(locale, plan))
        .collect();
    let mut entries = Vec::new();
    let mut all: BTreeSet<String> = scopes(db, project).into_iter().collect();
    all.extend(failure_scopes(db, project));
    for scope in &all {
        let subjects = subjects(db, project, Name::new(db, scope.clone()));
        let files: Vec<&LanguageFile> = language_files(db, project, scope)
            .into_iter()
            .filter_map(|file| language_file(db, project, file).as_ref())
            .collect();
        let default_file = files.iter().find(|file| file.locale == Some(plan.default));
        for subject in subjects.list.iter().filter(|subject| !subject.broken) {
            let Some(member) = default_file.and_then(|file| file.member(&subject.key)) else {
                continue;
            };
            let source = plain_text(&member.body);
            if source.is_empty() {
                continue;
            }
            entries.push(entry(scope, subject, source));
        }
    }
    // Jadpo's own, worded by the application in the default locale or by
    // what Jadpo ships.
    let root_default = super::jadpo::root_language_files(db, project)
        .into_iter()
        .find(|file| file.locale == Some(plan.default));
    let jadpo_subjects = super::jadpo::subjects();
    for message in super::jadpo::messages_on(db, project) {
        // Wording that is the same in every language (`TextDirection`'s) is
        // not sent out to be translated.
        if message.ships == jadpo_std::jadpo_text::Ships::Everywhere {
            continue;
        }
        let own = root_default
            .and_then(|file| file.group(GROUP))
            .and_then(|group| group.member(message.key))
            .map(|member| plain_text(&member.body))
            .filter(|source| !source.is_empty());
        let source = own.or_else(|| {
            message.ships_in(default.tag).then(|| match message.shape {
                ShippedShape::Plain(text) => vec![(None, text.to_string())],
                ShippedShape::Variants(variants) => variants
                    .iter()
                    .map(|(name, text)| (Some((*name).to_string()), (*text).to_string()))
                    .collect(),
            })
        });
        let Some(source) = source else { continue };
        if let Some(subject) = jadpo_subjects.get(message.key) {
            entries.push(entry(JADPO, subject, source));
        }
    }
    let clash = all
        .contains(JADPO)
        .then(|| format!("a domain is named `{JADPO}`, which is the name Jadpo's own messages are exchanged under"));
    entries.sort_by(|a, b| {
        (a.scope == JADPO, &a.scope, &a.member).cmp(&(b.scope == JADPO, &b.scope, &b.member))
    });
    Some(Catalogue {
        default,
        targets,
        entries,
        clash,
    })
}

fn entry(scope: &str, subject: &Subject, source: Vec<(Option<String>, String)>) -> Entry {
    let shape = match &subject.mode {
        Mode::Plain => Shape::Plain,
        Mode::Variants(variants) => Shape::Choice {
            field: subject
                .params
                .iter()
                .find(|param| param.kind == jadpo_std::translation::ParamKind::Choice)
                .map_or_else(|| "value".to_string(), |param| param.name.clone()),
            values: variants
                .iter()
                .map(|variant| variant.name.clone())
                .collect(),
        },
        Mode::Forms => Shape::Plural {
            field: subject
                .params
                .iter()
                .find(|param| param.kind == jadpo_std::translation::ParamKind::Plural)
                .map_or_else(|| "count".to_string(), |param| param.name.clone()),
        },
    };
    let mut placeholders: Vec<String> = subject
        .placeholders
        .iter()
        .map(|(name, _)| name.clone())
        .collect();
    if let Mode::Variants(variants) = &subject.mode {
        for variant in variants {
            for (field, kind) in &variant.payload {
                if kind.is_some() && !placeholders.contains(field) {
                    placeholders.push(field.clone());
                }
            }
        }
    }
    Entry {
        scope: scope.to_string(),
        member: subject.key.clone(),
        kind: subject.kind,
        description: subject.description.clone(),
        placeholders,
        shape,
        source,
    }
}

/// A wording a locale has, and where it stands against the record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Existing {
    pub wording: Vec<(Option<String>, String)>,
    /// Made from default wording that has since changed.
    pub stale: bool,
    /// A person accepted exactly this wording.
    pub accepted: bool,
}

/// The language file a scope has for a locale, read.
pub fn file_for<'a>(
    db: &'a dyn Db,
    project: Project,
    scope: &str,
    tag: &str,
) -> Option<&'a LanguageFile> {
    let files: Vec<File> = if scope == JADPO {
        super::jadpo::root_files(db, project)
    } else {
        language_files(db, project, scope)
    };
    files
        .into_iter()
        .filter_map(|file| language_file(db, project, file).as_ref())
        .find(|file| file.tag == tag)
}

pub fn member_of<'a>(file: &'a LanguageFile, entry: &Entry) -> Option<&'a Member> {
    if entry.scope == JADPO {
        file.group(GROUP)?.member(&entry.member)
    } else {
        file.member(&entry.member)
    }
}

/// The wording a locale has for an entry.
pub fn existing(db: &dyn Db, project: Project, entry: &Entry, tag: &str) -> Option<Existing> {
    let file = file_for(db, project, &entry.scope, tag)?;
    let member = member_of(file, entry)?;
    let wording = plain_text(&member.body);
    if wording.is_empty() {
        return None;
    }
    let standings =
        sources::standings(db, project, Name::new(db, entry.record_scope().to_string()));
    let record_member = entry.record_member();
    let standing = standings
        .list
        .iter()
        .find(|standing| standing.tag == tag && standing.member == record_member);
    Some(Existing {
        wording,
        stale: standing.is_some_and(|standing| standing.stale),
        accepted: standing.is_some_and(sources::Standing::accepted),
    })
}

// --- writing ---------------------------------------------------------------

/// A wording to write: the whole of one member.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Wording {
    Text(String),
    Variants(Vec<(String, String)>),
}

impl Wording {
    /// The text a record hashes, as [`sources::canonical`] makes it from a
    /// written member.
    pub fn canonical(&self) -> String {
        match self {
            Wording::Text(text) => text.clone(),
            Wording::Variants(variants) => {
                let mut parts: Vec<String> = variants
                    .iter()
                    .map(|(name, text)| format!("{name}={text}"))
                    .collect();
                parts.sort();
                parts.join("\u{1f}")
            }
        }
    }
}

/// A string as a language file writes it.
pub fn string_literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn render_member(name: &str, wording: &Wording) -> String {
    match wording {
        Wording::Text(text) => format!("{name}: {}", string_literal(text)),
        Wording::Variants(variants) => {
            let mut out = format!("{name}: {{\n");
            for (variant, text) in variants {
                out.push_str(&format!("    {variant}: {}\n", string_literal(text)));
            }
            out.push('}');
            out
        }
    }
}

/// A language file with some members written: the ones it has replaced
/// where they are, the others added at the end, and everything else as it
/// was. `changes` are `(member, wording)`; for the root file the members
/// are in the `jadpo: { }` group. The result is for `jadpo_fmt` to lay out.
pub fn write_members(
    existing: Option<(&LanguageFile, &str)>,
    root: bool,
    changes: &[(String, Wording)],
) -> String {
    let Some((file, text)) = existing else {
        let body: Vec<String> = changes
            .iter()
            .map(|(name, wording)| render_member(name, wording))
            .collect();
        return if root {
            format!(
                "translations {{\n{}: {{\n{}\n}}\n}}\n",
                GROUP,
                body.join("\n")
            )
        } else {
            format!("translations {{\n{}\n}}\n", body.join("\n"))
        };
    };
    let members: Vec<&Member> = if root {
        file.group(GROUP)
            .map(|group| group.members.iter().collect())
            .unwrap_or_default()
    } else {
        file.members.iter().collect()
    };
    // (start, end, replacement) edits, applied from the back.
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut added: Vec<String> = Vec::new();
    for (name, wording) in changes {
        match members.iter().find(|member| &member.name == name) {
            Some(member) => edits.push((
                member.span.start as usize,
                member.span.end as usize,
                render_member(name, wording),
            )),
            None => added.push(render_member(name, wording)),
        }
    }
    if !added.is_empty() {
        let joined = added.join("\n");
        if root {
            match file.group(GROUP) {
                Some(group) => {
                    if let Some(close) = group.close {
                        edits.push((close as usize, close as usize, format!("{joined}\n")));
                    }
                }
                None => {
                    if let Some(close) = file.close {
                        edits.push((
                            close as usize,
                            close as usize,
                            format!("{GROUP}: {{\n{joined}\n}}\n"),
                        ));
                    }
                }
            }
        } else if let Some(close) = file.close {
            edits.push((close as usize, close as usize, format!("{joined}\n")));
        }
    }
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.0));
    let mut out = text.to_string();
    for (start, end, replacement) in edits {
        if start <= end && end <= out.len() {
            out.replace_range(start..end, &replacement);
        }
    }
    out
}

/// The hash of a default wording, as a record keeps it.
pub fn default_hash(entry: &Entry) -> String {
    let wording = if matches!(entry.shape, Shape::Plain) {
        entry
            .source
            .first()
            .map(|(_, text)| Wording::Text(text.clone()))
    } else {
        Some(Wording::Variants(
            entry
                .source
                .iter()
                .filter_map(|(variant, text)| Some((variant.clone()?, text.clone())))
                .collect(),
        ))
    };
    sources::hash(
        &wording
            .map(|wording| wording.canonical())
            .unwrap_or_default(),
    )
}
