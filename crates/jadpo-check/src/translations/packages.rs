//! A library package's translations (area 10, "Jadpo's and packages'
//! text"; chunk 7.1b3): its `translations/` folder holds its messages and
//! its failures' wording, laid out as a domain's, with one `<locale>.jadpo`
//! for each language it words. The app words the rest, and may reword the
//! package's, in the root `translations/<locale>.jadpo` under the package's
//! folder name (`postcodes: { }`).
//!
//! The package's own files are checked by themselves, each against the
//! locale it is named for: a package is published apart from any app, so
//! a language the app doesn't support is the package's business. The app's
//! side is checked against the app's locales: each supported locale that
//! is not in progress words every public message of the package, every
//! enum of it that is worded, and each of its failures a route can answer
//! with, in the root file or in the package's own file for that locale.
//! A message is looked up in the app's root wording for the locale, then
//! the package's own, so the entries the runtime writes from hold, for
//! each supported locale, whichever of the two is found first; the
//! runtime's fallback for a locale in progress then follows the same
//! order for the sibling and the default.

use std::collections::BTreeMap;

use jadpo_db::{Db, PackageHome, Project};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_std::LocalePlan;
use jadpo_std::translation::{TranslationEntry, Wording};

use super::check::{Ctx, FailureWords, not_a_locale, variant_name};
use super::declared::{Kind, Subject, Subjects, language_files, package_of, subjects};
use super::language::{LanguageFile, language_file};
use crate::locales::{Locales, locales};
use crate::lookups::Name;

/// One package's translations, checked.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackageWords {
    pub problems: Vec<Diagnostic>,
    /// What the runtime writes the package's messages from, in the app's
    /// supported locales.
    pub entries: Vec<TranslationEntry>,
    /// Its failures' text for client apps.
    pub failures: Vec<FailureWords>,
    /// The locales the package words itself, by their place among
    /// `Locale`'s variants, in order.
    pub locales: Vec<u32>,
}

/// The package's subjects the app can reach: the public ones.
fn visible(all: &Subjects) -> Subjects {
    Subjects {
        list: all
            .list
            .iter()
            .filter(|subject| subject.symbol.public)
            .cloned()
            .collect(),
        problems: Vec::new(),
    }
}

/// The package whose translations are in `scope`, checked.
#[salsa::tracked(returns(ref))]
pub fn analyse_package<'db>(db: &'db dyn Db, project: Project, scope: Name<'db>) -> PackageWords {
    let mut out = PackageWords::default();
    let scope_name = scope.text(db).clone();
    let Some(home) = package_of(db, project, &scope_name) else {
        return out;
    };
    let all = subjects(db, project, scope);
    out.problems.extend(all.problems.iter().cloned());
    let files: Vec<&LanguageFile> = language_files(db, project, &scope_name)
        .into_iter()
        .filter_map(|file| language_file(db, project, file).as_ref())
        .collect();
    for file in &files {
        out.problems.extend(file.problems.iter().cloned());
    }
    out.locales = files.iter().filter_map(|file| file.locale).collect();
    out.locales.sort_unstable();
    out.locales.dedup();

    // The package's own files, each for the locale it is named for.
    let own_plan = LocalePlan {
        default: out.locales.first().copied().unwrap_or_default(),
        supported: out.locales.clone(),
        in_progress: Vec::new(),
    };
    let nothing = Locales::default();
    let unreachable = std::collections::BTreeSet::new();
    let mut own = Ctx {
        subjects: all,
        plan: &own_plan,
        declared: &nothing,
        reachable: &unreachable,
        scope: &scope_name,
        features: None,
        package_code: None,
        found: BTreeMap::new(),
        problems: Vec::new(),
    };
    for file in &files {
        match file.locale {
            Some(locale) => own.members(file, locale, false, &file.members),
            None => own.problems.push(not_a_locale(file)),
        }
    }
    out.problems.append(&mut own.problems);
    let own_found = own.found;

    // The app's side, in the locales the app supports.
    let declared = locales(db, project);
    let Some(plan) = &declared.plan else {
        return out;
    };
    let shown = visible(all);
    let roots = super::jadpo::root_language_files(db, project);
    let reachable = super::failures::reachable_names(db, project);
    let mut app = Ctx {
        subjects: &shown,
        plan,
        declared,
        reachable,
        scope: &scope_name,
        features: None,
        package_code: Some(&home.code),
        found: BTreeMap::new(),
        problems: Vec::new(),
    };
    for file in &roots {
        // A root file named for no locale, or for one the app doesn't
        // support, is reported with Jadpo's own.
        let Some(locale) = file.locale.filter(|&locale| plan.supports(locale)) else {
            continue;
        };
        let in_progress = plan.in_progress.contains(&locale) || LocalePlan::is_pseudo(locale);
        if let Some(group) = file.group(&scope_name) {
            app.members(file, locale, in_progress, &group.members);
        }
    }
    out.problems.append(&mut app.problems);
    let root_found = app.found;
    out.problems.extend(completeness(
        &home,
        &shown,
        &files,
        &roots,
        plan,
        declared,
        reachable,
        &scope_name,
    ));

    // The wording the runtime writes from: the app's root wording for the
    // locale, else the package's own.
    for subject in &shown.list {
        if subject.broken || subject.kind == Kind::Email {
            continue;
        }
        let mut wordings: Vec<(u32, Wording)> = Vec::new();
        for &locale in &plan.supported {
            let pick = |found: &BTreeMap<String, Vec<(u32, Wording)>>| {
                found
                    .get(&subject.key)
                    .and_then(|list| list.iter().find(|(at, _)| *at == locale))
                    .map(|(_, wording)| wording.clone())
            };
            if let Some(wording) = pick(&root_found).or_else(|| pick(&own_found)) {
                wordings.push((locale, wording));
            }
        }
        match subject.kind {
            Kind::Failure => {
                if wordings.is_empty() {
                    continue;
                }
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
            Kind::Enum if wordings.is_empty() => {}
            _ => out.entries.push(TranslationEntry {
                key: format!("{scope_name}.{}", subject.key),
                params: subject.call_params(),
                wordings,
            }),
        }
    }
    out
}

/// What each supported locale that isn't in progress must word of the
/// package: every public message, an enum once the package or the app
/// words it, and a failure a route can answer with.
fn required<'a>(
    shown: &'a Subjects,
    files: &[&LanguageFile],
    roots: &[&LanguageFile],
    scope: &str,
    reachable: &std::collections::BTreeSet<String>,
) -> Vec<&'a Subject> {
    shown
        .list
        .iter()
        .filter(|subject| !subject.broken)
        .filter(|subject| match subject.kind {
            Kind::Message => true,
            Kind::Enum => {
                files.iter().any(|file| file.member(&subject.key).is_some())
                    || roots.iter().any(|file| {
                        file.group(scope)
                            .is_some_and(|group| group.member(&subject.key).is_some())
                    })
            }
            Kind::Failure => reachable.contains(&subject.type_name),
            Kind::Email => false,
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn completeness(
    home: &PackageHome,
    shown: &Subjects,
    files: &[&LanguageFile],
    roots: &[&LanguageFile],
    plan: &LocalePlan,
    declared: &Locales,
    reachable: &std::collections::BTreeSet<String>,
    scope: &str,
) -> Vec<Diagnostic> {
    let required = required(shown, files, roots, scope, reachable);
    let mut out = Vec::new();
    if required.is_empty() {
        return out;
    }
    for &locale in &plan.supported {
        if (plan.in_progress.contains(&locale) || LocalePlan::is_pseudo(locale))
            && locale != plan.default
        {
            continue;
        }
        let own = files.iter().find(|file| file.locale == Some(locale));
        let root = roots.iter().find(|file| file.locale == Some(locale));
        let missing: Vec<&Subject> = required
            .iter()
            .copied()
            .filter(|subject| {
                let in_root = root
                    .and_then(|file| file.group(scope))
                    .is_some_and(|group| group.member(&subject.key).is_some());
                let in_own = own.is_some_and(|file| file.member(&subject.key).is_some());
                !in_root && !in_own
            })
            .collect();
        if missing.is_empty() {
            continue;
        }
        match root {
            Some(root) => {
                out.extend(
                    missing
                        .iter()
                        .map(|subject| wording_missing(home, root, subject, locale)),
                );
            }
            None => out.push(file_missing(home, &missing, locale, declared)),
        }
    }
    out
}

/// A package's wording a supported locale lacks, where the root file for
/// that locale exists.
fn wording_missing(
    home: &PackageHome,
    root: &LanguageFile,
    subject: &Subject,
    locale: u32,
) -> Diagnostic {
    let tag = variant_name(locale);
    let name = format!("{}.{}", home.name, subject.key);
    Diagnostic::new(
        &rule::TRANSLATION_WORDING_MISSING,
        format!("`{name}` has no wording in `{}`", root.path),
    )
    .at(root.keyword.clone(), format!("no `{name}`"))
    .also(
        subject.name_span.clone(),
        format!("`{}` declared here", subject.type_name),
    )
    .reason(format!(
        "Package `{}` doesn't word its {} `{}` in `Locale.{tag}`, which the application supports, so the application words it in the root file, under `{}: {{ }}`, for a call with `locale: Locale.{tag}` to have words to show.",
        home.code,
        subject.kind.word(),
        subject.key,
        home.name
    ))
    .next(format!(
        "Add `{}` to the `{}` group of `{}`{}",
        subject.key,
        home.name,
        root.path,
        placeholders(subject)
    ))
}

/// A supported locale with no root file, where the package doesn't word
/// everything in it.
fn file_missing(
    home: &PackageHome,
    missing: &[&Subject],
    locale: u32,
    declared: &Locales,
) -> Diagnostic {
    let tag = variant_name(locale);
    let path = super::jadpo::root_path(tag);
    let names: Vec<String> = missing
        .iter()
        .map(|subject| format!("`{}`", subject.key))
        .collect();
    let at = declared
        .entries
        .iter()
        .find(|(seen, _)| *seen == locale)
        .map(|(_, at)| at.clone())
        .or_else(|| declared.keyword.clone())
        .unwrap_or_else(|| Span::new("app.jadpo", 0, 0));
    Diagnostic::new(
        &rule::TRANSLATION_FILE_MISSING,
        format!(
            "`Locale.{tag}` is supported but package `{}` has no wording in it for {}",
            home.code,
            crate::and_list(&names)
        ),
    )
    .at(at, format!("no `{path}`"))
    .reason(format!(
        "Package `{}` ships no `Locale.{tag}` wording of {}, and the application supports `Locale.{tag}`, so the application words it in its root file, under the package's folder name. A locale that isn't finished yet is listed in `in_progress:`, and falls back to the default until it is.",
        home.code,
        crate::and_list(&names)
    ))
    .next(format!(
        "Add `{path}` wording {} under `{}: {{ }}`",
        crate::and_list(&names),
        home.name
    ))
}

/// ", which may write `{name}`", or nothing.
fn placeholders(subject: &Subject) -> String {
    let names: Vec<String> = subject
        .placeholders
        .iter()
        .map(|(name, _)| format!("`{{{name}}}`"))
        .collect();
    if names.is_empty() {
        String::new()
    } else {
        format!(", which may write {}", crate::and_list(&names))
    }
}

/// A package's text the app exchanges with translation services (chunk
/// 7.1b4): what a supported locale must word of it, and anything else of
/// it the app words in its root file, so the app's wording round-trips.
pub fn exchanged(db: &dyn Db, project: Project, scope: &str) -> Vec<Subject> {
    let all = subjects(db, project, Name::new(db, scope.to_string()));
    let shown = visible(all);
    let files: Vec<&LanguageFile> = language_files(db, project, scope)
        .into_iter()
        .filter_map(|file| language_file(db, project, file).as_ref())
        .collect();
    let roots = super::jadpo::root_language_files(db, project);
    let reachable = super::failures::reachable_names(db, project);
    let needed: Vec<String> = required(&shown, &files, &roots, scope, reachable)
        .into_iter()
        .map(|subject| subject.key.clone())
        .collect();
    shown
        .list
        .iter()
        .filter(|subject| !subject.broken && subject.kind != Kind::Email)
        .filter(|subject| {
            needed.contains(&subject.key)
                || roots.iter().any(|file| {
                    file.group(scope)
                        .is_some_and(|group| group.member(&subject.key).is_some())
                })
        })
        .cloned()
        .collect()
}

/// The scopes of the library packages that carry translations.
pub fn scopes(db: &dyn Db, project: Project) -> Vec<String> {
    super::declared::package_scopes(db, project)
        .iter()
        .map(|(scope, _)| scope.clone())
        .collect()
}

/// What the publishing bar holds a library package's translations to
/// (area 11, "What publishing requires"): every locale it words, words
/// every public message, and an enum once any file words it, so an app
/// is never shown half a language by the package. The locales it words,
/// as `Locale`'s variants write them, and what is missing.
pub fn publishing(db: &dyn Db, project: Project, folder: &str) -> (Vec<String>, Vec<Diagnostic>) {
    let Some((scope, home)) = super::declared::package_scopes(db, project)
        .iter()
        .find(|(_, home)| home.folder == folder)
        .cloned()
    else {
        return (Vec::new(), Vec::new());
    };
    let all = subjects(db, project, Name::new(db, scope.clone()));
    let shown = visible(all);
    let files: Vec<&LanguageFile> = language_files(db, project, &scope)
        .into_iter()
        .filter_map(|file| language_file(db, project, file).as_ref())
        .collect();
    let none = std::collections::BTreeSet::new();
    let required = required(&shown, &files, &[], &scope, &none);
    let mut locales: Vec<String> = Vec::new();
    let mut out = Vec::new();
    let prefix = format!("{}/", home.folder);
    for file in &files {
        let Some(locale) = file.locale else { continue };
        locales.push(variant_name(locale).to_string());
        // Named as the package's own repository has it.
        let path = file.path.strip_prefix(&prefix).unwrap_or(&file.path);
        for subject in &required {
            if file.member(&subject.key).is_none() {
                out.push(
                    Diagnostic::new(
                        &rule::TRANSLATION_WORDING_MISSING,
                        format!("`{}` has no wording in `{path}`", subject.key),
                    )
                    .at(file.keyword.clone(), format!("no `{}`", subject.key))
                    .also(
                        subject.name_span.clone(),
                        format!("`{}` declared here", super::declared::bare_name(&subject.symbol)),
                    )
                    .reason(format!(
                        "A published package words every public {} in each language it ships, so an app that supports `Locale.{}` isn't shown part of it in another language.",
                        subject.kind.word(),
                        variant_name(locale)
                    ))
                    .next(format!(
                        "Add `{}` to `{path}`{}, or remove the file until the language is finished",
                        subject.key,
                        placeholders(subject)
                    )),
                );
            }
        }
    }
    if files.iter().all(|file| file.locale.is_none()) {
        for subject in required
            .iter()
            .filter(|subject| subject.kind == Kind::Message)
        {
            out.push(
                Diagnostic::new(
                    &rule::TRANSLATION_FILE_MISSING,
                    format!(
                        "Package `{}` words `{}` in no language",
                        home.code, subject.key
                    ),
                )
                .at(subject.name_span.clone(), "no language file words it")
                .reason("A published package ships complete wording in its own default locale: one `<locale>.jadpo` in its `translations/` folder that words every public message, so an app in that language has text to show without wording it itself.")
                .next(format!(
                    "Add `translations/en_gb.jadpo`, or the file of the language the package is written in, wording `{}`",
                    subject.key
                )),
            );
        }
    }
    locales.sort();
    locales.dedup();
    (locales, out)
}

/// Each library package's text a supported locale besides the default
/// lacks, in the root file and the package's own: what `jadpo translations
/// missing` lists under the root file, as `<package>.<member>`.
pub fn gaps(db: &dyn Db, project: Project) -> Vec<super::outstanding::Gap> {
    let declared = locales(db, project);
    let Some(plan) = &declared.plan else {
        return Vec::new();
    };
    let roots = super::jadpo::root_language_files(db, project);
    let reachable = super::failures::reachable_names(db, project);
    let mut out = Vec::new();
    for scope in scopes(db, project) {
        let all = subjects(db, project, Name::new(db, scope.clone()));
        let shown = visible(all);
        let files: Vec<&LanguageFile> = language_files(db, project, &scope)
            .into_iter()
            .filter_map(|file| language_file(db, project, file).as_ref())
            .collect();
        let worded = |subject: &Subject, locale: u32| {
            roots.iter().any(|file| {
                file.locale == Some(locale)
                    && file
                        .group(&scope)
                        .is_some_and(|group| group.member(&subject.key).is_some())
            }) || files
                .iter()
                .any(|file| file.locale == Some(locale) && file.member(&subject.key).is_some())
        };
        for subject in required(&shown, &files, &roots, &scope, reachable) {
            for &locale in &plan.supported {
                if locale == plan.default || LocalePlan::is_pseudo(locale) {
                    continue;
                }
                if worded(subject, locale) {
                    continue;
                }
                let tag = variant_name(locale);
                out.push(super::outstanding::Gap {
                    scope: super::sources::ROOT.to_string(),
                    locale,
                    tag: tag.to_string(),
                    member: format!("{scope}.{}", subject.key),
                    kind: subject.kind,
                    path: super::jadpo::root_path(tag),
                    file_exists: roots.iter().any(|file| file.locale == Some(locale)),
                    in_progress: plan.in_progress.contains(&locale),
                    worded_in: plan
                        .supported
                        .iter()
                        .copied()
                        .filter(|&other| worded(subject, other))
                        .collect(),
                });
            }
        }
    }
    out
}
