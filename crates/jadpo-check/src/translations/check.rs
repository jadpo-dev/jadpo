//! Checking a scope's language files against what they word (area 10,
//! "What the compiler checks"): that every member words something, in the
//! shape it needs, writing only the fields it has; that every supported
//! locale not in progress words everything; and the wording the runtime
//! writes from, for the locales that have it.

use std::collections::BTreeMap;

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_std::translation::{
    ParamKind, PluralCategory, Template, TranslationEntry, Wording, plural_categories,
};
use jadpo_std::{LOCALES, LocalePlan};

use super::declared::{Kind, Mode, Subject, Subjects, language_files, subjects};
use super::language::{Body, LanguageFile, Member, Variant, Written, language_file};
use crate::locales::{Locales, locales};
use crate::lookups::Name;

/// What checking one scope found: the mistakes, and the entries the
/// runtime writes its messages from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Analysis {
    pub problems: Vec<Diagnostic>,
    pub entries: Vec<TranslationEntry>,
    /// The wording of each failure, in each supported locale that gave it
    /// cleanly, for the export client apps read.
    pub failures: Vec<FailureWords>,
}

/// One failure's text for client apps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FailureWords {
    /// The failure's name.
    pub name: String,
    /// Its code, which keys the wording.
    pub code: String,
    /// The placeholders a client formats, and how, by name.
    pub formats: Vec<(String, jadpo_std::translation::ClientFormat)>,
    /// The wording in each locale (its place among `Locale`'s variants).
    pub wordings: Vec<(u32, Template)>,
}

/// One scope's language files checked against its subjects.
#[salsa::tracked(returns(ref))]
pub fn analyse<'db>(db: &'db dyn Db, project: Project, scope: Name<'db>) -> Analysis {
    let mut out = Analysis::default();
    let scope_name = scope.text(db).clone();
    let subjects = subjects(db, project, scope);
    let declared = locales(db, project);
    let files: Vec<&LanguageFile> = language_files(db, project, &scope_name)
        .into_iter()
        .filter_map(|file| language_file(db, project, file).as_ref())
        .collect();
    for file in &files {
        out.problems.extend(file.problems.iter().cloned());
    }
    let Some(plan) = &declared.plan else {
        return out;
    };
    let reachable = super::failures::reachable_names(db, project);
    let mut ctx = Ctx {
        subjects,
        plan,
        declared,
        reachable,
        scope: &scope_name,
        features: None,
        found: BTreeMap::new(),
        problems: Vec::new(),
    };
    for file in &files {
        ctx.file(file);
    }
    ctx.completeness(&files);
    out.problems.extend(ctx.problems);
    for subject in &subjects.list {
        let wordings = ctx.found.remove(&subject.key).unwrap_or_default();
        let worded = !wordings.is_empty();
        if subject.kind == Kind::Failure {
            if !subject.broken && worded {
                out.failures.push(FailureWords {
                    name: subject.type_name.clone(),
                    code: subject.key.clone(),
                    formats: subject.formats.clone(),
                    wordings: wordings
                        .into_iter()
                        .filter_map(|(locale, wording)| match wording {
                            Wording::Plain(template) => Some((locale, template)),
                            _ => None,
                        })
                        .collect(),
                });
            }
            continue;
        }
        if subject.broken || (subject.kind != Kind::Message && !worded) {
            continue;
        }
        out.entries.push(TranslationEntry {
            key: format!("{scope_name}.{}", subject.key),
            params: subject.call_params(),
            wordings,
        });
    }
    out
}

pub(super) struct Ctx<'a> {
    pub(super) subjects: &'a Subjects,
    pub(super) plan: &'a LocalePlan,
    pub(super) declared: &'a Locales,
    /// The failures a route can answer with.
    pub(super) reachable: &'a std::collections::BTreeSet<String>,
    /// The scope being checked: `shared` or a domain.
    pub(super) scope: &'a str,
    /// For Jadpo's own messages (the root file's `jadpo: { }`): the
    /// features the application has on.
    pub(super) features: Option<&'a std::collections::BTreeSet<jadpo_std::jadpo_text::Feature>>,
    /// The wording each subject got, complete and clean, in each locale.
    pub(super) found: BTreeMap<String, Vec<(u32, Wording)>>,
    pub(super) problems: Vec<Diagnostic>,
}

pub(super) fn variant_name(ordinal: u32) -> &'static str {
    LOCALES
        .get(ordinal as usize)
        .map_or("", |(variant, _)| *variant)
}

/// What a wording's placeholders are called in an error.
fn field_word(subject: &Subject) -> &'static str {
    if subject.kind == Kind::Failure {
        "client field"
    } else {
        "field"
    }
}

fn braces(names: &[String]) -> String {
    let listed: Vec<String> = names.iter().map(|name| format!("`{{{name}}}`")).collect();
    crate::and_list(&listed)
}

impl Ctx<'_> {
    fn file(&mut self, file: &LanguageFile) {
        let Some((locale, in_progress)) = self.locale_of(file) else {
            return;
        };
        self.members(file, locale, in_progress, &file.members);
    }

    /// The file's locale and whether it is in progress, if it is one the
    /// application supports.
    pub(super) fn locale_of(&mut self, file: &LanguageFile) -> Option<(u32, bool)> {
        let Some(locale) = file.locale else {
            self.problems.push(not_a_locale(file));
            return None;
        };
        if !self.plan.supports(locale) {
            self.problems.push(unsupported(file, self.declared));
            return None;
        }
        let in_progress = self.plan.in_progress.contains(&locale) || LocalePlan::is_pseudo(locale);
        Some((locale, in_progress))
    }

    /// Checks the members of a language file, or of a root file's group.
    pub(super) fn members(
        &mut self,
        file: &LanguageFile,
        locale: u32,
        in_progress: bool,
        members: &[Member],
    ) {
        let mut seen: BTreeMap<&str, Span> = BTreeMap::new();
        for member in members {
            if let Some(first) = seen.get(member.name.as_str()) {
                self.problems.push(
                    Diagnostic::new(
                        &rule::TRANSLATION_MEMBER_DUPLICATE,
                        format!("`{}` is written twice in `{}`", member.name, file.path),
                    )
                    .at(member.key_span.clone(), "written again")
                    .also(first.clone(), "first written here")
                    .reason("A member words one message, enum or email, once; the second would be dropped or the first, whichever a reader of the file believed.")
                    .next(format!("Remove one `{}`", member.name)),
                );
                continue;
            }
            seen.insert(&member.name, member.key_span.clone());
            let Some(subject) = self.subjects.get(&member.name) else {
                self.problems.push(self.unknown(file, member));
                continue;
            };
            if subject.broken {
                continue;
            }
            if let (Some(on), Some(feature)) = (self.features, subject.builtin)
                && !on.contains(&feature)
            {
                self.problems.push(feature_off(file, member, feature));
                continue;
            }
            if let Some(wording) = self.member(file, locale, in_progress, subject, member) {
                self.found
                    .entry(subject.key.clone())
                    .or_default()
                    .push((locale, wording));
            }
        }
    }

    fn unknown(&self, file: &LanguageFile, member: &Member) -> Diagnostic {
        if self.features.is_some() {
            return self.unknown_jadpo(file, member);
        }
        let keys: Vec<&str> = self
            .subjects
            .list
            .iter()
            .map(|subject| subject.key.as_str())
            .collect();
        let near = nearest(&member.name, keys.iter().copied());
        let reason = match near {
            Some(near) => format!(
                "A member of `translations {{ }}` words one message or email subject of this folder's scope, or one of its enums, by its name in lower_snake_case (`NoticeText` is `notice_text`), or one of its failures by its code. `{}` is none of them. Did you mean `{near}`?",
                member.name
            ),
            None if keys.is_empty() => format!(
                "A member of `translations {{ }}` words a message, an enum or an email of this folder's scope, by its name in lower_snake_case, or a failure by its code. This scope declares none, so `{}` words nothing.",
                member.name
            ),
            None => {
                let listed: Vec<String> = keys.iter().map(|key| format!("`{key}`")).collect();
                format!(
                    "A member of `translations {{ }}` words one message, enum or email of this folder's scope, by its name in lower_snake_case, or a failure by its code. `{}` is none of them; the scope has {}.",
                    member.name,
                    crate::and_list(&listed)
                )
            }
        };
        let problem = Diagnostic::new(
            &rule::TRANSLATION_MEMBER_UNKNOWN,
            format!("`{}` words nothing", member.name),
        )
        .at(member.key_span.clone(), "no such message, enum or email")
        .reason(reason);
        match near {
            Some(near) => problem
                .next(format!("Rename the member to `{near}`"))
                .edit(member.key_span.clone(), near),
            None => problem.next(format!(
                "Remove `{}` from `{}`, or declare the message it words beside the language files",
                member.name, file.path
            )),
        }
    }

    /// A member of `jadpo: { }` that is none of Jadpo's messages.
    fn unknown_jadpo(&self, file: &LanguageFile, member: &Member) -> Diagnostic {
        let keys: Vec<&str> = self
            .subjects
            .list
            .iter()
            .filter(|subject| {
                subject
                    .builtin
                    .is_some_and(|feature| self.features.is_some_and(|on| on.contains(&feature)))
            })
            .map(|subject| subject.key.as_str())
            .collect();
        let near = nearest(&member.name, keys.iter().copied());
        let problem = Diagnostic::new(
            &rule::TRANSLATION_MEMBER_UNKNOWN,
            format!("`{}` words nothing of Jadpo's", member.name),
        )
        .at(member.key_span.clone(), "no such message of Jadpo's")
        .reason(format!(
            "`jadpo: {{ }}` words Jadpo's own messages, by their names; `jadpo translations builtin` lists the ones this application can show. `{}` is none of them.{}",
            member.name,
            near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
        ));
        match near {
            Some(near) => problem
                .next(format!("Rename the member to `{near}`"))
                .edit(member.key_span.clone(), near),
            None => problem.next(format!(
                "Remove `{}` from `{}`, or run `jadpo translations builtin` for the names",
                member.name, file.path
            )),
        }
    }

    /// Checks a member against what it words; its wording when it is
    /// complete and clean.
    fn member(
        &mut self,
        file: &LanguageFile,
        locale: u32,
        in_progress: bool,
        subject: &Subject,
        member: &Member,
    ) -> Option<Wording> {
        let before = self.problems.len();
        let mut used: Vec<String> = Vec::new();
        let result = match (&subject.mode, &member.body) {
            (_, Body::Invalid) => return None,
            (Mode::Plain, Body::Text(written)) => {
                let allowed = subject.placeholders.clone();
                self.template(subject, written, &allowed, &mut used)
                    .map(Wording::Plain)
            }
            (Mode::Plain, Body::Variants(_)) => {
                self.shape(subject, member, "one wording, a string");
                None
            }
            (Mode::Variants(_) | Mode::Forms, Body::Text(_)) => {
                let hint = self.variants_hint(subject, locale);
                self.shape(subject, member, &hint);
                None
            }
            (Mode::Variants(variants), Body::Variants(entries)) => {
                let expected: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
                let templates = self.variants(
                    file,
                    locale,
                    in_progress,
                    subject,
                    member,
                    entries,
                    &expected,
                    |this, name, written, used| {
                        let variant = variants.iter().find(|v| v.name == name)?;
                        let mut allowed = subject.placeholders.clone();
                        for (field, kind) in &variant.payload {
                            if let Some(kind) = kind {
                                allowed.push((field.clone(), *kind));
                            }
                        }
                        this.template(subject, written, &allowed, used)
                    },
                    &mut used,
                );
                templates.map(Wording::Variants)
            }
            (Mode::Forms, Body::Variants(entries)) => {
                let categories = plural_categories(locale);
                let expected: Vec<String> =
                    categories.iter().map(|c| c.word().to_string()).collect();
                let templates = self.variants(
                    file,
                    locale,
                    in_progress,
                    subject,
                    member,
                    entries,
                    &expected,
                    |this, _, written, used| {
                        let allowed = subject.placeholders.clone();
                        this.template(subject, written, &allowed, used)
                    },
                    &mut used,
                );
                templates.map(|list| {
                    Wording::Forms(
                        list.into_iter()
                            .filter_map(|(name, template)| {
                                Some((PluralCategory::from_word(&name)?, template))
                            })
                            .collect(),
                    )
                })
            }
        };
        let clean = self.problems.len() == before;
        if clean && result.is_some() {
            self.unused(subject, member, &used);
        }
        if clean { result } else { None }
    }

    fn variants_hint(&self, subject: &Subject, locale: u32) -> String {
        match &subject.mode {
            Mode::Variants(variants) => {
                let names: Vec<String> = variants.iter().map(|v| format!("`{}`", v.name)).collect();
                format!(
                    "a block with a wording for each value: {}",
                    crate::and_list(&names)
                )
            }
            _ => {
                let names: Vec<String> = plural_categories(locale)
                    .iter()
                    .map(|c| format!("`{}`", c.word()))
                    .collect();
                format!(
                    "a block with a wording for each plural form: {}",
                    crate::and_list(&names)
                )
            }
        }
    }

    fn shape(&mut self, subject: &Subject, member: &Member, wanted: &str) {
        let (written, needs) = match &member.body {
            Body::Text(_) => ("a string", "one wording for each value or plural form"),
            _ => ("a block", "one wording, a string"),
        };
        self.problems.push(
            Diagnostic::new(
                &rule::TRANSLATION_WORDING_SHAPE,
                format!("`{}` is {written}, but needs {needs}", member.name),
            )
            .at(member.key_span.clone(), format!("needs {wanted}"))
            .reason(format!(
                "How a {} is worded follows its type: no choice, a plain string; one enum field, a wording for each value of the enum; one `PluralCount` field, a wording for each plural form of the language.",
                subject.kind.word()
            ))
            .next(format!("Write `{}` as {wanted}", member.name)),
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn variants(
        &mut self,
        file: &LanguageFile,
        locale: u32,
        in_progress: bool,
        subject: &Subject,
        member: &Member,
        entries: &[Variant],
        expected: &[String],
        mut each: impl FnMut(&mut Self, &str, &Written, &mut Vec<String>) -> Option<Template>,
        used: &mut Vec<String>,
    ) -> Option<Vec<(String, Template)>> {
        let plural = matches!(subject.mode, Mode::Forms);
        let mut seen: BTreeMap<&str, Span> = BTreeMap::new();
        let mut got: Vec<(String, Template)> = Vec::new();
        let mut clean = true;
        for entry in entries {
            if let Some(first) = seen.get(entry.name.as_str()) {
                self.problems.push(
                    Diagnostic::new(
                        &rule::TRANSLATION_MEMBER_DUPLICATE,
                        format!("`{}` is written twice in `{}`", entry.name, member.name),
                    )
                    .at(entry.key_span.clone(), "written again")
                    .also(first.clone(), "first written here")
                    .reason("Each value or plural form has one wording.")
                    .next(format!("Remove one `{}`", entry.name)),
                );
                clean = false;
                continue;
            }
            seen.insert(&entry.name, entry.key_span.clone());
            if !expected.contains(&entry.name) {
                self.problems
                    .push(self.variant_unknown(subject, member, entry, expected, plural, locale));
                clean = false;
                continue;
            }
            let template = entry
                .written
                .as_ref()
                .and_then(|written| each(self, &entry.name, written, used));
            match template {
                Some(template) => got.push((entry.name.clone(), template)),
                None => clean = false,
            }
        }
        let missing: Vec<&String> = expected
            .iter()
            .filter(|name| !entries.iter().any(|entry| &entry.name == *name))
            .collect();
        if !missing.is_empty() {
            clean = false;
            if !in_progress {
                self.problems
                    .push(self.variant_missing(file, subject, member, &missing, plural, locale));
            }
        }
        clean.then_some(got)
    }

    fn variant_unknown(
        &self,
        subject: &Subject,
        member: &Member,
        entry: &Variant,
        expected: &[String],
        plural: bool,
        locale: u32,
    ) -> Diagnostic {
        let near = nearest(&entry.name, expected.iter().map(String::as_str));
        let listed: Vec<String> = expected.iter().map(|name| format!("`{name}`")).collect();
        let reason = if plural {
            let is_category = PluralCategory::from_word(&entry.name).is_some();
            if is_category {
                format!(
                    "`Locale.{}` has no use for the plural form `{}`: CLDR gives it {}, so those are the forms a wording is written for.",
                    variant_name(locale),
                    entry.name,
                    crate::and_list(&listed)
                )
            } else {
                format!(
                    "`{}` isn't a plural form. CLDR's forms are `zero`, `one`, `two`, `few`, `many` and `other`, and `Locale.{}` needs {}.",
                    entry.name,
                    variant_name(locale),
                    crate::and_list(&listed)
                )
            }
        } else {
            let ty = subject_enum(subject).unwrap_or_else(|| subject.type_name.clone());
            format!(
                "`{ty}` has no value `{}`. Its values are {}.{}",
                entry.name,
                crate::and_list(&listed),
                near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
            )
        };
        let problem = Diagnostic::new(
            &rule::TRANSLATION_VARIANT_UNKNOWN,
            if plural {
                format!(
                    "`{}` isn't a plural form of `Locale.{}`",
                    entry.name,
                    variant_name(locale)
                )
            } else {
                format!(
                    "`{}` isn't a value of the enum `{}`",
                    entry.name,
                    subject_enum(subject).unwrap_or_default()
                )
            },
        )
        .at(entry.key_span.clone(), "no such wording to give")
        .reason(reason);
        match near {
            Some(near) if !plural || PluralCategory::from_word(&entry.name).is_none() => problem
                .next(format!("Rename it to `{near}`"))
                .edit(entry.key_span.clone(), near),
            _ => problem.next(format!(
                "Remove `{}` from `{}`{}",
                entry.name,
                member.name,
                if plural {
                    ", or move its wording into one of the forms the language uses"
                } else {
                    ""
                }
            )),
        }
    }

    fn variant_missing(
        &self,
        file: &LanguageFile,
        subject: &Subject,
        member: &Member,
        missing: &[&String],
        plural: bool,
        locale: u32,
    ) -> Diagnostic {
        let listed: Vec<String> = missing.iter().map(|name| format!("`{name}`")).collect();
        let (what, reason) = if plural {
            (
                "plural form",
                format!(
                    "`Locale.{}` needs a wording for each plural form CLDR gives it, so a count in any of them has words to show.",
                    variant_name(locale)
                ),
            )
        } else {
            (
                "value",
                format!(
                    "`{}` is worded once for each value of `{}`, so every value has words to show.",
                    subject.type_name,
                    subject_enum(subject).unwrap_or_default()
                ),
            )
        };
        let plural_s = if missing.len() == 1 { "" } else { "s" };
        let mut problem = Diagnostic::new(
            &rule::TRANSLATION_VARIANT_MISSING,
            format!(
                "`{}` in `{}` has no wording for the {what}{plural_s} {}",
                member.name,
                file.path,
                crate::and_list(&listed)
            ),
        )
        .at(
            member.key_span.clone(),
            format!("no {}", crate::and_list(&listed)),
        );
        if subject.builtin.is_none() {
            problem = problem.also(subject.name_span.clone(), "declared here");
        }
        problem.reason(reason).next(format!(
            "Add a wording for {} to `{}`{}",
            crate::and_list(&listed),
            member.name,
            fields_note(subject, "each")
        ))
    }

    /// Checks one wording's text and the fields it writes; the template when
    /// it is clean.
    fn template(
        &mut self,
        subject: &Subject,
        written: &Written,
        allowed: &[(String, ParamKind)],
        used: &mut Vec<String>,
    ) -> Option<Template> {
        let (template, uses) = match &written.template {
            Ok(found) => found.clone(),
            Err(error) => {
                self.problems.push(
                    Diagnostic::new(
                        &rule::TRANSLATION_WORDING_INVALID,
                        "This wording isn't a plain string with `{field}` placeholders",
                    )
                    .at(written.range(error.start, error.end), &error.label)
                    .reason(&error.reason)
                    .next(&error.next),
                );
                return None;
            }
        };
        let mut clean = true;
        for field in uses {
            used.push(field.name.clone());
            if let Some((_, kind)) = allowed.iter().find(|(name, _)| *name == field.name) {
                // An amount of money and a percentage are each written with
                // its style, and nothing else is (area 19, question 16).
                if jadpo_std::translation::Style::of(*kind) != field.style {
                    clean = false;
                    self.problems
                        .push(style_problem(subject, written, &field, *kind));
                }
                continue;
            }
            clean = false;
            let names: Vec<String> = allowed.iter().map(|(name, _)| name.clone()).collect();
            let near = nearest(&field.name, names.iter().map(String::as_str));
            let choice = subject
                .params
                .iter()
                .find(|param| param.name == field.name && param.kind == ParamKind::Choice);
            let reason = if choice.is_some() {
                format!(
                    "`{}` is the enum that picks which wording is written, so it has no placeholder of its own: each value has its own wording.",
                    field.name
                )
            } else if names.is_empty() {
                format!(
                    "`{}` has no {} `{}` a wording can write.",
                    subject.type_name,
                    field_word(subject),
                    field.name
                )
            } else {
                format!(
                    "`{}` has no {} `{}` a wording can write; this wording may write {}.{}",
                    subject.type_name,
                    field_word(subject),
                    field.name,
                    braces(&names),
                    near.map_or_else(String::new, |near| format!(" Did you mean `{{{near}}}`?"))
                )
            };
            let problem = Diagnostic::new(
                &rule::TRANSLATION_PLACEHOLDER_UNKNOWN,
                format!(
                    "`{{{}}}` isn't a field of `{}`",
                    field.name, subject.type_name
                ),
            )
            .at(written.range(field.start, field.end), "no such field")
            .reason(reason);
            self.problems.push(match near {
                Some(near) => problem
                    .next(format!("Write `{{{near}}}` instead"))
                    .edit(written.range(field.start, field.end), format!("{{{near}}}")),
                None => problem.next(format!(
                    "Remove `{{{}}}`, or add the field to {}",
                    field.name,
                    if subject.kind == Kind::Failure {
                        format!("`{}`'s `client:` block", subject.type_name)
                    } else {
                        format!("`{}`", subject.type_name)
                    }
                )),
            });
        }
        clean.then_some(template)
    }

    /// A field no wording of the member writes: a judgement call.
    fn unused(&mut self, subject: &Subject, member: &Member, used: &[String]) {
        if subject.kind != Kind::Message || subject.builtin.is_some() {
            return;
        }
        for (name, _) in &subject.placeholders {
            if used.contains(name) {
                continue;
            }
            self.problems.push(
                Diagnostic::new(
                    &rule::TRANSLATION_FIELD_UNUSED,
                    format!("No wording of `{}` writes `{{{name}}}`", member.name),
                )
                .at(member.key_span.clone(), format!("`{{{name}}}` is never written"))
                .also(subject.name_span.clone(), "declared here")
                .reason(format!(
                    "`{}` has a field `{name}`, and a call gives it a value, but this language never writes it. That is right for a language that needs no such word, and it is also what a forgotten placeholder looks like.",
                    subject.type_name
                ))
                .next(format!(
                    "Write `{{{name}}}` in a wording of `{}`, or remove the field",
                    member.name
                )),
            );
        }
    }

    /// Every supported locale not in progress words everything it must.
    fn completeness(&mut self, files: &[&LanguageFile]) {
        let required = required(self.subjects, files, self.reachable);
        if required.is_empty() {
            return;
        }
        for &locale in &self.plan.supported {
            // A locale in progress falls back, and the pseudo-locale writes
            // the default's wording, so neither is asked for any.
            if (self.plan.in_progress.contains(&locale) || LocalePlan::is_pseudo(locale))
                && locale != self.plan.default
            {
                continue;
            }
            let file = files.iter().find(|file| file.locale == Some(locale));
            let Some(file) = file else {
                self.problems.push(self.file_missing(locale, &required));
                continue;
            };
            for subject in &required {
                if file.member(&subject.key).is_none() {
                    self.problems.push(missing(file, subject, locale));
                }
            }
        }
    }

    fn file_missing(&self, locale: u32, required: &[&Subject]) -> Diagnostic {
        let tag = variant_name(locale);
        let at = self
            .declared
            .entries
            .iter()
            .find(|(seen, _)| *seen == locale)
            .map(|(_, at)| at.clone())
            .or_else(|| self.declared.keyword.clone());
        let folder = if self.scope == super::declared::SHARED {
            format!("shared/translations/{tag}.jadpo")
        } else {
            format!("domains/{}/translations/{tag}.jadpo", self.scope)
        };
        let names: Vec<String> = required
            .iter()
            .map(|subject| format!("`{}`", subject.key))
            .collect();
        let default = locale == self.plan.default;
        let reason = if default {
            "The default locale is what every other locale falls back to, so it is worded in full, and it is where the wording of each message is first written.".to_string()
        } else {
            "Every supported locale has a language file for each folder that has messages, so a call with `locale: Locale.".to_string()
                + tag
                + "` has wording to show. A locale that isn't finished yet is listed in `in_progress:`, and falls back to the default until it is."
        };
        let mut problem = Diagnostic::new(
            &rule::TRANSLATION_FILE_MISSING,
            format!(
                "`Locale.{tag}` is supported but has no language file for {}",
                crate::and_list(&names)
            ),
        );
        if let Some(at) = at {
            problem = problem.at(at, format!("no `{folder}`"));
        }
        problem.reason(reason).next(format!(
            "Add `{folder}` with a `translations {{ }}` that words {}",
            crate::and_list(&names)
        ))
    }
}

/// What every supported locale that isn't in progress must word: every
/// message, an enum once any language file words it, and a failure a route
/// can answer with.
pub(super) fn required<'a>(
    subjects: &'a Subjects,
    files: &[&LanguageFile],
    reachable: &std::collections::BTreeSet<String>,
) -> Vec<&'a Subject> {
    subjects
        .list
        .iter()
        .filter(|subject| !subject.broken)
        .filter(|subject| match subject.kind {
            Kind::Message => true,
            Kind::Enum => files.iter().any(|file| file.member(&subject.key).is_some()),
            Kind::Failure => reachable.contains(&subject.type_name),
            Kind::Email => false,
        })
        .collect()
}

/// A message's wording missing in a language file that exists.
fn missing(file: &LanguageFile, subject: &Subject, locale: u32) -> Diagnostic {
    let tag = variant_name(locale);
    Diagnostic::new(
        &rule::TRANSLATION_WORDING_MISSING,
        format!("`{}` has no wording in `{}`", subject.key, file.path),
    )
    .at(file.keyword.clone(), format!("no `{}`", subject.key))
    .also(subject.name_span.clone(), format!("`{}` declared here", subject.type_name))
    .reason(match subject.kind {
        Kind::Failure => format!(
            "A failure a route can answer with is worded in every supported locale that isn't in progress, so a client app showing `Locale.{tag}` has text for its code."
        ),
        _ => format!(
            "A {} is worded in every supported locale that isn't in progress, so a call with `locale: Locale.{tag}` has words to show.",
            subject.kind.word()
        ),
    })
    .next(format!(
        "Add {} to `{}`{}",
        needs(subject, locale),
        file.path,
        fields_note(subject, "which")
    ))
}

/// The member to add, written out.
fn needs(subject: &Subject, locale: u32) -> String {
    let key = &subject.key;
    match &subject.mode {
        Mode::Plain => format!("`{key}: \"...\"`"),
        Mode::Variants(variants) => {
            let names: Vec<String> = variants.iter().map(|v| format!("`{}`", v.name)).collect();
            format!(
                "`{key}`, with a wording for each value of `{}` ({})",
                subject_enum(subject).unwrap_or_default(),
                crate::and_list(&names)
            )
        }
        Mode::Forms => {
            let names: Vec<String> = plural_categories(locale)
                .iter()
                .map(|c| format!("`{}`", c.word()))
                .collect();
            format!(
                "`{key}`, with a wording for each plural form ({})",
                crate::and_list(&names)
            )
        }
    }
}

/// ", which may write `{name}` and `{count}`", or nothing.
fn fields_note(subject: &Subject, who: &str) -> String {
    let names: Vec<String> = subject
        .placeholders
        .iter()
        .map(|(name, _)| name.clone())
        .collect();
    if names.is_empty() {
        String::new()
    } else if who == "each" {
        format!(", each able to write {}", braces(&names))
    } else {
        format!(", {who} may write {}", braces(&names))
    }
}

/// The enum that chooses a subject's wording.
fn subject_enum(subject: &Subject) -> Option<String> {
    if subject.kind == Kind::Enum {
        return Some(subject.type_name.clone());
    }
    subject
        .params
        .iter()
        .find_map(|param| param.enum_type.clone())
}

pub(super) fn not_a_locale(file: &LanguageFile) -> Diagnostic {
    let near = nearest(&file.tag, LOCALES.iter().map(|(name, _)| *name)).map(str::to_string);
    let folded = LOCALES
        .iter()
        .find(|(_, tag)| tag.eq_ignore_ascii_case(&file.tag))
        .map(|(name, _)| name.to_string());
    let better = folded.or(near);
    let dir = file.path.rsplit_once('/').map_or("", |(dir, _)| dir);
    let problem = Diagnostic::new(
        &rule::TRANSLATION_FILE_NAME_INVALID,
        format!("`{}` isn't named for a locale", file.path),
    )
    .at(file.keyword.clone(), format!("`{}` is no `Locale`", file.tag))
    .reason(format!(
        "A language file is named for the locale it words, the tag in lower case with `_` for `-`: `en_gb.jadpo` is `Locale.en_gb`.{}",
        better
            .as_ref()
            .map_or_else(String::new, |name| format!(" Did you mean `{name}`?"))
    ));
    match better {
        Some(name) => problem.next(format!("Rename the file to `{dir}/{name}.jadpo`")),
        None => problem.next(format!(
            "Rename the file after one of the application's supported locales, such as `{dir}/en_gb.jadpo`"
        )),
    }
}

pub(super) fn unsupported(file: &LanguageFile, declared: &Locales) -> Diagnostic {
    let mut problem = Diagnostic::new(
        &rule::TRANSLATION_LOCALE_UNSUPPORTED,
        format!("`{}` is for a locale this application doesn't support", file.path),
    )
    .at(file.keyword.clone(), format!("`Locale.{}` isn't supported", file.tag))
    .reason("Wording is written for the locales listed in `locales { supported: }`, in `app.jadpo`; a file for another is never used, and the server never guesses a language.");
    if let Some(at) = &declared.keyword {
        problem = problem.also(at.clone(), "the supported locales");
    }
    problem.next(format!(
        "Add `Locale.{}` to `supported:` in `app.jadpo`, or remove the file",
        file.tag
    ))
}

/// A member of `jadpo: { }` for a message of a feature the application has
/// off.
fn feature_off(
    file: &LanguageFile,
    member: &Member,
    feature: jadpo_std::jadpo_text::Feature,
) -> Diagnostic {
    Diagnostic::new(
        &rule::TRANSLATION_FEATURE_OFF,
        format!(
            "`{}` is a message of a feature this application has off",
            member.name
        ),
    )
    .at(
        member.key_span.clone(),
        "a feature this application has off",
    )
    .reason(format!(
        "`{}` is part of Jadpo's {} text, which Jadpo writes when {}. This application doesn't, so wording it changes nothing and would hide a mistake.",
        member.name,
        feature.heading().to_lowercase(),
        feature.when()
    ))
    .next(format!(
        "Remove `{}` from `{}`, or switch the feature on",
        member.name, file.path
    ))
}

/// A field written with the wrong style: `{price}` for an amount of
/// money, which is written `{price, number, currency}`, `{rate}` for a
/// percentage, written `{rate, number, percent}`, or a style on a field
/// of another kind.
fn style_problem(
    subject: &Subject,
    written: &Written,
    field: &jadpo_std::translation::FieldUse,
    kind: ParamKind,
) -> Diagnostic {
    use jadpo_std::translation::Style;
    let name = &field.name;
    let plain = format!("{{{name}}}");
    let span = written.range(field.start, field.end);
    let styled = |style: Style| format!("{{{name}{}}}", style.written());
    if let Some(style) = Style::of(kind) {
        let right = styled(style);
        let (what, shown) = match style {
            Style::Currency => ("money", "with its symbol and the currency's places"),
            Style::Percent => ("a percentage", "with the `%` where the locale writes it"),
        };
        let (as_written, label) = match field.style {
            Some(other) => (styled(other), "another style"),
            None => (plain, "no style"),
        };
        return Diagnostic::new(
            &rule::TRANSLATION_PLACEHOLDER_STYLE,
            format!("`{as_written}` is {}, written `{right}`", kind.word()),
        )
        .at(span.clone(), label)
        .reason(format!(
            "`{}.{name}` is {what}, and a wording writes {} with its style, `{right}`, so the translator sees it is one; it is shown as `text.format` shows it in the reader's locale, {shown}.",
            subject.type_name,
            kind.word()
        ))
        .next(format!("Write `{right}`"))
        .edit(span, right);
    }
    let written_style = field.style.unwrap_or(Style::Currency);
    let wrong = styled(written_style);
    let as_what = written_style.kind().word();
    Diagnostic::new(
        &rule::TRANSLATION_PLACEHOLDER_STYLE,
        format!("`{wrong}` writes {} as {as_what}", kind.word()),
    )
    .at(span.clone(), format!("not {as_what}"))
    .reason(format!(
        "`{}` is the style of {as_what}, and `{}.{name}` is {}, which a wording writes as `{plain}`.",
        written_style.written(),
        subject.type_name,
        kind.word()
    ))
    .next(format!("Write `{plain}`"))
    .edit(span, plain)
}
