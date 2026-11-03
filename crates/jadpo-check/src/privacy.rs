//! `application { privacy: { } }` (area 15, "What erasure doesn't rewrite";
//! chunk P4c): for now only `erasure_log: { store: }`, where the erasure log
//! is kept. That is a store that holds no person's records, or a store of
//! files, so restoring a person's database can't take the log back with
//! them. An application that erases someone names one
//! (`data.erasure_log_required`, where `erase` is written). The rest of
//! `privacy:`, the generated routes for a person's requests, is
//! `toolchain.not_built_yet` where it is written.

use std::sync::Arc;

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_syntax::ast::{self, AstNode};

use crate::data::ConnectionKind;
use crate::tree::{self, name_text, node_span};

/// The section's name.
pub const PRIVACY: &str = "privacy";
/// Why the rest of it isn't accepted.
const ROUTES_NOT_BUILT: &str = "The generated routes for a person's requests to export or erase their data aren't built yet: accepted, no request could be made.";

/// `privacy:`, read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Privacy {
    /// The connection `erasure_log:` names, when it can keep the log, and
    /// what it is.
    pub log: Option<(String, ConnectionKind)>,
    /// Whether it says `erasure_log:` at all, read cleanly or not, so an
    /// erasure isn't told to add what is reported where it is written.
    pub log_written: bool,
    pub problems: Vec<Diagnostic>,
}

/// `privacy:` in `application { }`.
#[salsa::tracked(returns(ref))]
pub fn privacy(db: &dyn Db, project: Project) -> Arc<Privacy> {
    let mut out = Privacy::default();
    let Some(file) = project
        .files(db)
        .iter()
        .find(|file| file.path(db) == jadpo_db::PROJECT_FILE)
    else {
        return Arc::new(out);
    };
    let Some(source) = tree::source(db, *file) else {
        return Arc::new(out);
    };
    let path = file.path(db);
    for decl in source.decls() {
        let ast::Decl::SingletonDecl(singleton) = decl else {
            continue;
        };
        if singleton.keyword().map(|t| t.text().to_string()).as_deref() != Some("application") {
            continue;
        }
        for (key, member) in tree::members(singleton.member_list()) {
            if name_text(&key).as_deref() != Some(PRIVACY) {
                continue;
            }
            for (setting, entry) in tree::members(member.block()) {
                let at = node_span(path, setting.syntax());
                if name_text(&setting).as_deref() == Some("erasure_log") {
                    out.log_written = true;
                    erasure_log(db, project, path, at, &entry, &mut out);
                } else {
                    let name = name_text(&setting).unwrap_or_default();
                    out.problems.push(not_built(at, &name));
                }
            }
        }
    }
    Arc::new(out)
}

fn not_built(at: Span, name: &str) -> Diagnostic {
    Diagnostic::new(
        &rule::TOOLCHAIN_NOT_BUILT_YET,
        format!("`privacy: {{ {name}: }}` isn't built yet"),
    )
    .at(at, "decided, not built")
    .reason(ROUTES_NOT_BUILT)
    .next(format!("Remove `{name}:` for now"))
}

/// `erasure_log: { store: }`.
fn erasure_log(
    db: &dyn Db,
    project: Project,
    path: &str,
    at: Span,
    entry: &ast::Member,
    out: &mut Privacy,
) {
    let invalid = |title: String, at: Span, label: &str, next: String| {
        Diagnostic::new(&rule::DATA_ERASURE_LOG_STORE, title)
            .at(at, label.to_string())
            .reason("`erasure_log: { store: <connection> }` names where the log of everyone erased is kept: a database that holds no person's records, or a store of files.")
            .next(next)
    };
    let mut named = None;
    for (setting, value) in tree::members(entry.block()) {
        let word = name_text(&setting).unwrap_or_default();
        if word == "store" {
            named = Some(value);
        } else {
            out.problems.push(invalid(
                format!("`{word}` isn't a setting of `erasure_log:`"),
                node_span(path, setting.syntax()),
                "not a setting",
                format!("Remove `{word}:`"),
            ));
        }
    }
    let Some(value) = named else {
        out.problems.push(invalid(
            "`erasure_log:` doesn't say which store keeps it".to_string(),
            at,
            "no `store:`",
            "Write `erasure_log: { store: <connection> }`".to_string(),
        ));
        return;
    };
    let written = value
        .value()
        .map(|value| tree::written(value.syntax()))
        .unwrap_or_default();
    let span = value.value().map_or_else(
        || node_span(path, value.syntax()),
        |v| node_span(path, v.syntax()),
    );
    let connections = crate::data::connections(db, project);
    let kind = connections.kind(&written);
    if !matches!(
        kind,
        Some(ConnectionKind::Postgres | ConnectionKind::Files)
    ) {
        let mut stores = connections.databases();
        stores.extend(connections.files());
        let mut problem = Diagnostic::new(
            &rule::DATA_ERASURE_LOG_STORE,
            format!("`store: {written}` is neither a database nor a store of files"),
        )
        .at(span.clone(), "can't keep the log")
        .reason(format!(
            "The erasure log must outlast a restore of a person's database, so it is kept in a database of its own or a store of files; this application has {}.",
            if stores.is_empty() {
                "none yet".to_string()
            } else {
                crate::and_list(&stores.iter().map(|s| format!("`{s}`")).collect::<Vec<_>>())
            }
        ));
        problem = match nearest(&written, stores.iter().map(String::as_str)) {
            Some(near) => problem
                .next(format!("Use `{near}` instead of `{written}`"))
                .edit(span, near),
            None => problem.next("Add a connection for it, and name that"),
        };
        out.problems.push(problem);
        return;
    }
    if kind == Some(ConnectionKind::Postgres) {
        let holding = holds_a_persons_records(db, project, &written);
        if let Some(entity) = holding {
            out.problems.push(
                Diagnostic::new(
                    &rule::DATA_ERASURE_LOG_STORE,
                    format!("`{written}` holds a person's records, so a restore would take the log back with them"),
                )
                .at(span, format!("`{entity}` is kept here"))
                .reason(format!(
                    "The log is what erases someone again after a backup brings them back. Restoring `{written}` would bring back their `{entity}` records and lose every entry written since the backup, so nobody erased since would be erased again."
                ))
                .next("Add a database that holds no person's records and name it, or name a store of files"),
            );
            return;
        }
    }
    out.log = kind.map(|kind| (written, kind));
}

/// The first entity, by name, in database `store` whose records erasing a
/// person reaches, or who is a person.
fn holds_a_persons_records(db: &dyn Db, project: Project, store: &str) -> Option<String> {
    let erasures = crate::erasure::erasures(db, project);
    crate::data::data_model(db, project)
        .entities
        .iter()
        .find(|entity| {
            entity.store.as_deref() == Some(store) && (entity.person || erasures.reaches(entity))
        })
        .map(|entity| entity.name.clone())
}
