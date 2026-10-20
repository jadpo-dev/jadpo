//! What is left to do in the translations (area 10, "Tooling"): the
//! wording a locale lacks, which `jadpo translations missing` lists, and
//! the messages no code uses, which `jadpo translations unused` lists. The
//! errors say the same about a locale that is complete; these also cover a
//! locale in progress, which falls back instead of failing.

use std::collections::BTreeSet;

use jadpo_db::{Db, Project};

use super::check::required;
use super::declared::{Kind, SHARED, language_files, scopes, subjects};
use super::failures::{failure_scopes, reachable_names};
use super::language::language_file;
use crate::lookups::Name;

/// One member a supported locale lacks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gap {
    pub scope: String,
    /// The locale's place among `Locale`'s variants, and its name.
    pub locale: u32,
    pub tag: String,
    pub member: String,
    pub kind: Kind,
    /// The language file that should word it, whether or not it exists yet.
    pub path: String,
    pub file_exists: bool,
    /// Whether the locale's translations are in progress, so the gap is
    /// filled by falling back rather than failing the check.
    pub in_progress: bool,
    /// The supported locales that do word it, for the fallback to choose
    /// among.
    pub worded_in: Vec<u32>,
}

/// Every gap, by scope, then locale, then member.
pub fn gaps(db: &dyn Db, project: Project) -> Vec<Gap> {
    let declared = crate::locales::locales(db, project);
    let Some(plan) = &declared.plan else {
        return Vec::new();
    };
    let mut out = Vec::new();
    // Jadpo's own messages, worded in the root folder or by Jadpo.
    for gap in super::jadpo::gaps(db, project) {
        out.push(Gap {
            scope: super::sources::ROOT.to_string(),
            locale: gap.locale,
            tag: gap.tag,
            member: super::jadpo::member_name(gap.message.key),
            kind: match gap.message.kind {
                jadpo_std::jadpo_text::Kind::Message => Kind::Message,
                jadpo_std::jadpo_text::Kind::Failure => Kind::Failure,
                jadpo_std::jadpo_text::Kind::Enum => Kind::Enum,
            },
            path: gap.path,
            file_exists: gap.file_exists,
            in_progress: gap.in_progress,
            worded_in: gap.worded_in,
        });
    }
    // Library packages' text, worded in the root folder or by the package.
    out.extend(super::packages::gaps(db, project));
    let mut all: BTreeSet<String> = scopes(db, project).into_iter().collect();
    all.extend(failure_scopes(db, project));
    let reachable = reachable_names(db, project);
    for scope in all {
        let subjects = subjects(db, project, Name::new(db, scope.clone()));
        let files: Vec<_> = language_files(db, project, &scope)
            .into_iter()
            .filter_map(|file| language_file(db, project, file).as_ref())
            .collect();
        let required = required(subjects, &files, reachable);
        for &locale in &plan.supported {
            if locale == plan.default || jadpo_std::LocalePlan::is_pseudo(locale) {
                continue;
            }
            let tag = jadpo_std::LOCALES
                .get(locale as usize)
                .map_or("", |(variant, _)| *variant);
            let file = files.iter().find(|file| file.locale == Some(locale));
            let path = file.map_or_else(
                || {
                    if scope == SHARED {
                        format!("shared/translations/{tag}.jadpo")
                    } else {
                        format!("domains/{scope}/translations/{tag}.jadpo")
                    }
                },
                |file| file.path.clone(),
            );
            for subject in &required {
                if file.is_some_and(|file| file.member(&subject.key).is_some()) {
                    continue;
                }
                let worded_in = plan
                    .supported
                    .iter()
                    .copied()
                    .filter(|&other| {
                        files.iter().any(|file| {
                            file.locale == Some(other) && file.member(&subject.key).is_some()
                        })
                    })
                    .collect();
                out.push(Gap {
                    scope: scope.clone(),
                    locale,
                    tag: tag.to_string(),
                    member: subject.key.clone(),
                    kind: subject.kind,
                    path: path.clone(),
                    file_exists: file.is_some(),
                    in_progress: plan.in_progress.contains(&locale),
                    worded_in,
                });
            }
        }
    }
    out
}

/// A message or enum that code never calls.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unused {
    pub scope: String,
    pub type_name: String,
    pub key: String,
    pub kind: Kind,
    /// Where it is declared.
    pub path: String,
    pub span: jadpo_diag::Span,
}

/// The messages, and the enums with wording, that no code calls, and how
/// many there are to call. Tests do not count as users: a message only a
/// test writes is unused by the application.
pub fn unused(db: &dyn Db, project: Project) -> (Vec<Unused>, usize) {
    let used = crate::usage::translation_keys_used(db, project);
    let mut out = Vec::new();
    let mut total = 0;
    for scope in scopes(db, project) {
        let worded: Vec<_> = language_files(db, project, &scope)
            .into_iter()
            .filter_map(|file| language_file(db, project, file).as_ref())
            .collect();
        for subject in &subjects(db, project, Name::new(db, scope.clone())).list {
            if !subject.callable() {
                continue;
            }
            // An enum nobody words is not a translation at all.
            if subject.kind == Kind::Enum
                && !worded
                    .iter()
                    .any(|file| file.member(&subject.key).is_some())
            {
                continue;
            }
            total += 1;
            if !used.contains(&format!("{scope}.{}", subject.key)) {
                out.push(Unused {
                    scope: scope.clone(),
                    type_name: subject.type_name.clone(),
                    key: subject.key.clone(),
                    kind: subject.kind,
                    path: subject.symbol.path.clone(),
                    span: subject.name_span.clone(),
                });
            }
        }
    }
    (out, total)
}
