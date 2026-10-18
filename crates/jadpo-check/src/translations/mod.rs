//! Translations as messages (area 10, "Translations as messages"): a
//! message is a type, a language is a file of wordings, and both live in
//! the domain that uses them, in `translations/` (or in `shared/`'s).
//!
//! The work is in four parts, so an edit rechecks only what it affects:
//!
//! - [`declared`] reads what can be worded: the messages, the enums and the
//!   email subjects of a scope, and what a wording may write;
//! - [`language`] reads one language file by itself;
//! - [`check`] compares a scope's language files with its subjects and with
//!   the application's locales, and builds the entries the runtime writes
//!   from;
//! - [`callable`] is what one call asks: the parameters of the message it
//!   names. It is a query per name, so a call depends on the declaration it
//!   names and on nothing in the wording.

mod check;
mod declared;
pub mod definitions;
pub mod exchange;
pub mod failures;
pub mod jadpo;
mod language;
pub mod outstanding;
pub mod sources;

use jadpo_db::{Db, Project, Symbol};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_std::translation::{ParamKind, TranslationPlan};

use crate::lookups::Name;
use crate::types::Prim;

pub use declared::{Kind, Mode, ParamSpec, SHARED, Subject, Subjects, scope_of, scopes, subjects};

/// The namespace a message is called through: `translations.<scope>.<key>`.
pub const TRANSLATIONS: &str = "translations";
pub use language::{Body, LanguageFile, Member, Variant, language_file};

use check::analyse;

/// The problems in the project's translations.
#[salsa::tracked(returns(ref))]
pub fn translation_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let mut scopes = scopes(db, project);
    // What `translations:` in `app.jadpo` says is read whether or not the
    // project words anything yet.
    let mut out: Vec<Diagnostic> = sources::review(db, project).problems.clone();
    // A failure a route can answer with is worded in every supported
    // locale, wherever its domain keeps no message or language file yet;
    // with no `locales { }` there is nothing to word it for.
    if crate::locales::locales(db, project).declared {
        for scope in failures::failure_scopes(db, project) {
            if !scopes.contains(&scope) {
                scopes.push(scope);
            }
        }
        scopes.sort();
    }
    let roots = jadpo::root_language_files(db, project);
    for scope in &scopes {
        out.extend(
            subjects(db, project, Name::new(db, scope.clone()))
                .problems
                .iter()
                .cloned(),
        );
    }
    let declared = crate::locales::locales(db, project);
    if !declared.declared {
        if !scopes.is_empty() || !roots.is_empty() {
            out.push(locales_required(db, project, &scopes));
        }
        return out;
    }
    // Jadpo's own messages, worded in the root folder and by what Jadpo
    // ships, and that folder's record.
    out.extend(jadpo::analysis(db, project).problems.iter().cloned());
    out.extend(
        sources::standings(db, project, Name::new(db, sources::ROOT.to_string()))
            .problems
            .iter()
            .cloned(),
    );
    for scope in &scopes {
        out.extend(
            analyse(db, project, Name::new(db, scope.clone()))
                .problems
                .iter()
                .cloned(),
        );
        out.extend(
            sources::standings(db, project, Name::new(db, scope.clone()))
                .problems
                .iter()
                .cloned(),
        );
    }
    out
}

/// Translations with no `locales { }`: pointed at the first language file,
/// or the first message.
fn locales_required(db: &dyn Db, project: Project, scopes: &[String]) -> Diagnostic {
    let mut at: Option<Span> = None;
    for scope in scopes {
        let files = declared::language_files(db, project, scope);
        if let Some(file) = files
            .first()
            .and_then(|&file| language_file(db, project, file).as_ref())
        {
            at = Some(file.keyword.clone());
            break;
        }
        if let Some(subject) = subjects(db, project, Name::new(db, scope.clone()))
            .list
            .first()
        {
            at = Some(subject.name_span.clone());
            break;
        }
    }
    let at = at
        .or_else(|| {
            jadpo::root_language_files(db, project)
                .first()
                .map(|file| file.keyword.clone())
        })
        .unwrap_or_else(|| Span::new("app.jadpo", 0, 0));
    Diagnostic::new(
        &rule::TRANSLATION_LOCALES_REQUIRED,
        "Translations need the application's locales",
    )
    .at(at, "no locales declared")
    .reason("Each language file is for one of the locales the application supports, and each supported locale words every message. None is declared, so there is nothing to check the wording against.")
    .next("Declare them in `app.jadpo`, for instance `locales { default: Locale.en_gb supported: [Locale.en_gb] }`")
}

/// What a call of `translations.<scope>.<key>` finds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Callable {
    /// The scope has no such message or enum: the key is not one.
    Unknown,
    /// It exists but has a mistake in its declaration, reported there.
    Broken,
    /// The parameters of the call, after `locale:`, and the message's type.
    Found {
        params: Vec<ParamSpec>,
        type_name: String,
        symbol: Box<Symbol>,
    },
}

/// What the scope's declarations say about `key`.
#[salsa::tracked(returns(ref))]
pub fn callable<'db>(
    db: &'db dyn Db,
    project: Project,
    scope: Name<'db>,
    key: Name<'db>,
) -> Callable {
    match subjects(db, project, scope).get(key.text(db)) {
        Some(subject) if !matches!(subject.kind, Kind::Email | Kind::Failure) => {
            if subject.broken {
                Callable::Broken
            } else {
                Callable::Found {
                    params: subject.params.clone(),
                    type_name: subject.type_name.clone(),
                    symbol: Box::new(subject.symbol.clone()),
                }
            }
        }
        _ => Callable::Unknown,
    }
}

/// The scopes a call could name, and the keys each has, for the nearest
/// name a mistyped call might have meant.
pub fn callable_keys(db: &dyn Db, project: Project, scope: &str) -> Vec<String> {
    subjects(db, project, Name::new(db, scope.to_string()))
        .list
        .iter()
        .filter(|subject| subject.callable())
        .map(|subject| subject.key.clone())
        .collect()
}

/// Whether a scope name is `shared` or a domain.
pub fn is_scope(db: &dyn Db, project: Project, scope: &str) -> bool {
    scope == SHARED || jadpo_db::symbols(db, project).domains.contains(scope)
}

/// The type a call's key `<scope>.<key>` names, for who uses it.
pub fn type_of_key(db: &dyn Db, project: Project, key: &str) -> Option<String> {
    let (scope, member) = key.split_once('.')?;
    subjects(db, project, Name::new(db, scope.to_string()))
        .get(member)
        .map(|subject| subject.type_name.clone())
}

/// Each entry the runtime writes from, with the place it is first worded,
/// so the graph can key it and a change to any wording changes it.
#[derive(Clone, Debug)]
pub struct EntryFacts {
    pub key: String,
    pub scope: String,
    pub name: String,
    pub place: Option<(String, Span)>,
    /// The wording in each supported locale's file, as its text, in the
    /// order the locales are listed.
    pub text: Vec<String>,
}

/// Every message, enum and email subject worded in the project, for the
/// graph (area 10).
pub fn entry_facts(db: &dyn Db, project: Project) -> Vec<EntryFacts> {
    let mut out = Vec::new();
    for scope in scopes(db, project) {
        let name = Name::new(db, scope.clone());
        let subjects = subjects(db, project, name);
        let files: Vec<&LanguageFile> = declared::language_files(db, project, &scope)
            .into_iter()
            .filter_map(|file| language_file(db, project, file).as_ref())
            .collect();
        for subject in &subjects.list {
            if subject.broken || matches!(subject.kind, Kind::Email | Kind::Failure) {
                continue;
            }
            let mut text = Vec::new();
            for file in &files {
                if let Some(member) = file.member(&subject.key) {
                    text.push(format!("{}:{}", file.tag, language::flatten(&member.body)));
                }
            }
            out.push(EntryFacts {
                key: format!("{scope}.{}", subject.key),
                scope: scope.clone(),
                name: subject.key.clone(),
                place: Some((subject.symbol.path.clone(), subject.name_span.clone())),
                text,
            });
        }
    }
    out
}

/// The plan the runtime writes from: every message and enum worded, with
/// its wording in each locale that has it. Meant for a project that
/// checks.
pub fn plan(db: &dyn Db, project: Project) -> TranslationPlan {
    let mut entries = Vec::new();
    for scope in scopes(db, project) {
        entries.extend(
            analyse(db, project, Name::new(db, scope))
                .entries
                .iter()
                .cloned(),
        );
    }
    entries.sort_by(|a, b| a.key.cmp(&b.key));
    TranslationPlan { entries }
}

/// What a call's argument for a parameter of this kind may be built on.
pub fn accepts(kind: ParamKind) -> &'static [Prim] {
    match kind {
        ParamKind::Text => &[Prim::Text, Prim::PresentationText],
        ParamKind::Number => &[Prim::Int, Prim::Decimal],
        ParamKind::Money => &[Prim::Money],
        ParamKind::Percent => &[Prim::Percentage],
        ParamKind::Plural => &[Prim::Int],
        ParamKind::Choice => &[],
    }
}
