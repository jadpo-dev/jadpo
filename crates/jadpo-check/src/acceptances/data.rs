//! What needs a person in the migrations themselves (area 5, "Migrations";
//! area 18, `DataChange`; chunk V3): each consecutive pair of snapshots says
//! what its migration takes out of the app's reach or removes for good.
//!
//! A migration this change adds needs an acceptance this change adds, as
//! every change that needs a person does ([`needs`]). A migration already
//! on the before side needs one on the before side, so editing one can't
//! slip a removal in. With no before side, and whenever a migration is about
//! to run, a removal needs an acceptance with a reason anywhere in the
//! project ([`unaccepted`]): that doesn't depend on git, so a migration
//! that drops data never runs without one.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{BeforeSide, Db, Project};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_graph::migration::{Removal, removals};
use jadpo_graph::schema::Schema;

use super::compare::{Family, Need};
use super::{files, words};
use crate::data::migrations::{Migration, migrations};

/// Each removal of each migration, in order, with the migration it is in. A
/// migration whose snapshot can't be read says nothing, and is reported.
fn every_removal(db: &dyn Db, project: Project) -> Vec<(Migration, Need)> {
    let mut out: Vec<(Migration, Need)> = Vec::new();
    let empty = Schema::default();
    let mut previous = &empty;
    for migration in &migrations(db, project).list {
        let Some(schema) = migration.schema() else {
            continue;
        };
        let path = if migration.changes {
            migration.changes_path()
        } else {
            migration.snapshot_path()
        };
        for removal in removals(previous, schema) {
            let (subject, change) = (removal.subject(), removal.change());
            let said = words::removal(&removal);
            // One subject, in two stores: a table left behind by each of
            // two moves, dropped together.
            if let Some((_, known)) = out.iter_mut().find(|(other, need)| {
                other.name == migration.name && need.subject == subject && need.change == change
            }) {
                known.said = format!("{}; {said}", known.said);
                continue;
            }
            out.push((
                migration.clone(),
                Need {
                    heading: None,
                    said,
                    change: change.to_string(),
                    family: Family::Data,
                    path: path.clone(),
                    subject,
                    who: Vec::new(),
                },
            ));
        }
        previous = schema;
    }
    out
}

/// What each migration removes from `store`, by the migration's name: what
/// `jadpo migrate apply` says it did.
pub fn removed_by(db: &dyn Db, project: Project, store: &str) -> BTreeMap<String, Vec<Removal>> {
    let mut out: BTreeMap<String, Vec<Removal>> = BTreeMap::new();
    let empty = Schema::default();
    let mut previous = &empty;
    for migration in &migrations(db, project).list {
        let Some(schema) = migration.schema() else {
            continue;
        };
        for removal in removals(previous, schema) {
            if removal.store == store {
                out.entry(migration.name.clone()).or_default().push(removal);
            }
        }
        previous = schema;
    }
    out
}

/// Whether any migration removes data, which `jadpo check` judges even
/// outside git.
pub fn removes_data(db: &dyn Db, project: Project) -> bool {
    !every_removal(db, project).is_empty()
}

/// The acceptances with a person's reason among `all` that `kept` keeps,
/// each entry as what it allows.
fn allowed(
    all: &[files::Acceptance],
    kept: impl Fn(&files::Acceptance) -> bool,
) -> BTreeSet<(String, String)> {
    all.iter()
        .filter(|acceptance| acceptance.reason.is_some() && kept(acceptance))
        .flat_map(|acceptance| acceptance.entries.iter())
        .map(|entry| (entry.subject.clone(), entry.change.clone()))
        .collect()
}

/// What the migrations need a person for, since `before`: every removal
/// of a migration this change adds, and any of one already there with no
/// acceptance there either.
pub fn needs(db: &dyn Db, project: Project, before: &BeforeSide, out: &mut Vec<Need>) {
    let earlier = allowed(&files::acceptances(db, project), |acceptance| {
        before.text(&acceptance.path).is_some()
    });
    for (migration, need) in every_removal(db, project) {
        if !before.migrations.contains(&migration.name)
            || !earlier.contains(&(need.subject.clone(), need.change.clone()))
        {
            out.push(need);
        }
    }
}

/// Each removal no acceptance anywhere in the project allows with a reason:
/// what stops `jadpo migrate apply`, and `jadpo check` with no before side.
pub fn unaccepted(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let allowed = allowed(&files::acceptances(db, project), |_| true);
    every_removal(db, project)
        .into_iter()
        .filter(|(_, need)| !allowed.contains(&(need.subject.clone(), need.change.clone())))
        .map(|(migration, need)| {
            let (subject, change) = (&need.subject, &need.change);
            Diagnostic::new(
                &rule::MIGRATION_DROP_NOT_ACCEPTED,
                format!(
                    "Migration `{}` removes data nobody accepted: {subject}",
                    migration.name
                ),
            )
            .at(span(db, project, &need), change.as_str())
            .reason(format!(
                "{}. So a person accepts it first, and no acceptance with a reason lists `{subject}: {change}`.",
                need.said
            ))
            .next(format!(
                "A person writes an acceptance in `acceptances/` listing `{subject}: {change}`, with why on a `///` line above `acceptance`"
            ))
        })
        .collect()
}

/// Where `need` shows: its decision's line in its `changes.jadpo`, else
/// the file.
pub fn span(db: &dyn Db, project: Project, need: &Need) -> Span {
    let line = project
        .files(db)
        .iter()
        .find(|file| file.path(db) == &need.path)
        .and_then(|file| {
            let text = file.text(db);
            let mut start = 0usize;
            for line in text.split_inclusive('\n') {
                let written = line.trim_start();
                if written.starts_with(&format!("{}:", need.subject)) {
                    let at = start + (line.len() - written.len());
                    return Some((at, at + need.subject.len()));
                }
                start += line.len();
            }
            None
        });
    match line {
        Some((start, end)) => Span::new(&need.path, start as u32, end as u32),
        None => crate::tree::file_span(&need.path),
    }
}
