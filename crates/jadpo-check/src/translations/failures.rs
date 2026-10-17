//! Failures worded for client apps (area 10, "Failures for client apps"):
//! which failures a route can answer with, and the text each API's clients
//! are sent for them. A failure's wording is a member of its domain's
//! language files, keyed by its code; the server's `message` stays English.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, Project};
use jadpo_std::translation::{ClientFormat, Template};

use super::check::analyse;
use super::declared::scopes;
use crate::lookups::Name;

/// The failures each API's routes answer with, by API, as their names.
#[salsa::tracked(returns(ref))]
pub fn reachable(db: &dyn Db, project: Project) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (group, route) in crate::routes::groups(db, project).routes() {
        out.entry(group.api.clone())
            .or_default()
            .extend(route.fails.iter().cloned());
    }
    out
}

/// Every failure some route answers with, by name.
pub fn reachable_names(db: &dyn Db, project: Project) -> &BTreeSet<String> {
    all_reachable(db, project)
}

#[salsa::tracked(returns(ref))]
fn all_reachable(db: &dyn Db, project: Project) -> BTreeSet<String> {
    reachable(db, project).values().flatten().cloned().collect()
}

/// The scopes that hold a failure some route answers with: where its
/// wording is required even when the scope has no message or language
/// file yet.
pub fn failure_scopes(db: &dyn Db, project: Project) -> Vec<String> {
    let names = reachable_names(db, project);
    if names.is_empty() {
        return Vec::new();
    }
    let table = jadpo_db::symbols(db, project);
    let mut found: BTreeSet<String> = BTreeSet::new();
    for name in names {
        for symbol in table.declarations.get(name).into_iter().flatten() {
            if matches!(
                symbol.kind,
                jadpo_db::ItemKind::Type(jadpo_db::Base::Failure)
            ) && !symbol.test
                && let Some(scope) = super::declared::scope_of(&symbol.ownership)
            {
                found.insert(scope);
            }
        }
    }
    found.into_iter().collect()
}

/// One failure's text for client apps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FailureText {
    pub code: String,
    /// The placeholders a client formats, and how, by name.
    pub formats: Vec<(String, ClientFormat)>,
    /// The wording in each locale that gave it, by the locale's place
    /// among `Locale`'s variants.
    pub wordings: Vec<(u32, Template)>,
}

/// The text each API's clients are sent, by API: every failure one of its
/// routes answers with that the language files word, in code order.
pub fn texts(db: &dyn Db, project: Project) -> BTreeMap<String, Vec<FailureText>> {
    let by_name: BTreeMap<String, FailureText> = scopes(db, project)
        .into_iter()
        .chain(failure_scopes(db, project))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .flat_map(|scope| analyse(db, project, Name::new(db, scope)).failures.clone())
        .map(|words| {
            (
                words.name,
                FailureText {
                    code: words.code,
                    formats: words.formats,
                    wordings: words.wordings,
                },
            )
        })
        .collect();
    // Jadpo's own failures join each API's: the ones its routes can meet,
    // unless the application words a failure under the same code.
    let jadpo = super::jadpo::failure_texts(db, project);
    let mut out: BTreeMap<String, Vec<FailureText>> = reachable(db, project)
        .iter()
        .map(|(api, names)| {
            let texts: Vec<FailureText> = names
                .iter()
                .filter_map(|name| by_name.get(name).cloned())
                .collect();
            (api.clone(), texts)
        })
        .collect();
    for (api, own) in jadpo {
        let texts = out.entry(api).or_default();
        for text in own {
            if !texts.iter().any(|seen| seen.code == text.code) {
                texts.push(text);
            }
        }
    }
    for texts in out.values_mut() {
        texts.sort_by(|a, b| a.code.cmp(&b.code));
    }
    out
}
