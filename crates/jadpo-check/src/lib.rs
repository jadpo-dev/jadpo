//! Checking a Jadpo project (docs/compiler-runtime.md §2.2): the rules of
//! each language area as salsa queries over `jadpo-db`. Areas 1 to 6 are
//! built: layout, naming, visibility and resolution, then values and types
//! and the logic of bodies ([`types`]), failures (`failures`, with the
//! body rules beside the others in [`types`]), and data, entities, queries
//! and migrations ([`data`]). [`graph`] builds the application graph from
//! these queries.
//!
//! [`check`] assembles every diagnostic for a project, sorted by file and
//! position so the same source always gives the same output.

mod case;
pub mod client_address;
pub mod config;
pub mod data;
mod failures;
pub mod graph;
pub mod identity;
pub mod imports;
mod layout;
pub mod lookups;
mod naming;
pub mod policy;
mod resolve;
pub mod routes;
mod std_lib;
mod tree;
pub mod types;
mod usage;
mod visibility;

pub use failures::{
    DECLARABLE_KINDS, FailureInfo, FailureTable, MEMBERS as FAILURE_MEMBERS, failure_problems,
    failure_table,
};
pub use imports::{apply_edits, fix_imports, import_edits, import_problems};
pub use layout::layout_problems;
pub use naming::{domain_problems, naming_problems};
pub use resolve::resolve_file;
pub use usage::{mentions, usage_problems, users};

use jadpo_db::{Db, Project, item_tree, parse};
use jadpo_diag::{Diagnostic, rule};

pub use std_lib::StubSignature;

/// Every standard-library operation the stubs declare, with its
/// parameters' kinds and its result's: each must match its entry in
/// `jadpo-std`'s operation table, which binds it to one Rust function in
/// `jadpo-value`.
pub fn standard_operations() -> Vec<StubSignature> {
    std_lib::StdNames::get().signatures()
}

/// Every problem in `project`, in a stable order. Syntax errors come alone,
/// since a file that does not parse would only add knock-on errors, unless
/// every one is a habit from another language that parsed as its Jadpo
/// form (`Parse::checkable`): then the rest of the project is checked too,
/// and no other problem is reported on a line with a syntax error.
pub fn check(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let files = project.files(db);
    let syntax: Vec<Diagnostic> = files
        .iter()
        .flat_map(|file| {
            let path = file.path(db);
            parse(db, *file)
                .errors()
                .iter()
                .map(|error| error.to_diagnostic(path))
                .collect::<Vec<_>>()
        })
        .collect();
    let checkable = files.iter().all(|file| parse(db, *file).checkable());
    let mut out = Vec::new();
    if syntax.is_empty() || checkable {
        if files
            .iter()
            .all(|file| item_tree(db, *file).items.is_empty())
        {
            out.push(empty_project(files.len()));
        }
        out.extend(domain_problems(db, project).iter().cloned());
        for &file in files {
            out.extend(layout_problems(db, project, file).iter().cloned());
            out.extend(naming_problems(db, project, file).iter().cloned());
            out.extend(resolve_file(db, project, file).iter().cloned());
            out.extend(import_problems(db, project, file).iter().cloned());
            out.extend(usage_problems(db, project, file).iter().cloned());
            out.extend(
                visibility::public_problems(db, project, file)
                    .iter()
                    .cloned(),
            );
            out.extend(config::use_problems(db, project, file).iter().cloned());
            out.extend(types::type_problems(db, project, file).iter().cloned());
            out.extend(failure_problems(db, project, file).iter().cloned());
            out.extend(types::body_problems(db, project, file).iter().cloned());
            out.extend(types::recursion_problems(db, project, file).iter().cloned());
            out.extend(types::store_problems(db, project, file).iter().cloned());
            out.extend(graph::similar_problems(db, project, file).iter().cloned());
            out.extend(data::data_problems(db, project, file).iter().cloned());
            out.extend(
                data::queries::query_problems(db, project, file)
                    .iter()
                    .cloned(),
            );
            out.extend(routes::route_problems(db, project, file).iter().cloned());
            out.extend(
                policy::reach::reach_problems(db, project, file)
                    .iter()
                    .cloned(),
            );
            out.extend(policy::policy_problems(db, project, file).iter().cloned());
            out.extend(
                policy::outputs::query_policy_problems(db, project, file)
                    .iter()
                    .cloned(),
            );
            out.extend(
                policy::flow::flow_problems(db, project, file)
                    .iter()
                    .cloned(),
            );
        }
        out.extend(config::config_problems(db, project).iter().cloned());
        out.extend(config::connection_problems(db, project).iter().cloned());
        out.extend(config::test_value_problems(db, project).iter().cloned());
        out.extend(identity::identity_problems(db, project).iter().cloned());
        out.extend(
            client_address::client_address_problems(db, project)
                .iter()
                .cloned(),
        );
        out.extend(routes::api_problems(db, project).iter().cloned());
        out.extend(routes::table_problems(db, project).iter().cloned());
        out.extend(routes::signin_problems(db, project).iter().cloned());
        out.extend(routes::record::record_problems(db, project).iter().cloned());
        out.extend(types::call_problems(db, project).iter().cloned());
        out.extend(
            data::migrations::migration_problems(db, project)
                .iter()
                .cloned(),
        );
    }
    if !syntax.is_empty() {
        let line = |path: &str, offset: u32| {
            files
                .iter()
                .find(|file| file.path(db) == path)
                .map(|file| line_of(file.text(db), offset))
        };
        let broken: std::collections::HashSet<(String, Option<usize>)> = syntax
            .iter()
            .filter_map(|d| d.primary.as_ref())
            .map(|label| {
                let path = label.span.path.clone();
                let at = line(&path, label.span.start);
                (path, at)
            })
            .collect();
        out.retain(|d| {
            d.primary.as_ref().is_none_or(|label| {
                let path = label.span.path.clone();
                let at = line(&path, label.span.start);
                !broken.contains(&(path, at))
            })
        });
        out.extend(syntax);
    }
    sort(&mut out);
    out
}

/// Whether `diagnostic` stops a program running (`jadpo test`, `jadpo
/// dev`): every error but an API record out of date, which is a promise
/// to clients that `jadpo check` and CI hold to (area 7), not a reason
/// the program can't run.
pub fn blocks_running(diagnostic: &Diagnostic) -> bool {
    diagnostic.severity() == jadpo_diag::Severity::Error
        && diagnostic.rule.id != rule::API_RECORD_OUT_OF_DATE.id
}

/// The zero-based line `offset` is on.
fn line_of(text: &str, offset: u32) -> usize {
    let end = (offset as usize).min(text.len());
    text.as_bytes()[..end]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
}

/// By file, then position, then rule, so output never depends on the
/// order checks ran in; repeats are dropped.
fn sort(diagnostics: &mut Vec<Diagnostic>) {
    let key = |d: &Diagnostic| {
        let (path, start, end) = d.primary.as_ref().map_or((String::new(), 0, 0), |label| {
            (label.span.path.clone(), label.span.start, label.span.end)
        });
        (path, start, end, d.rule.id, d.message.clone())
    };
    diagnostics.sort_by_key(key);
    diagnostics.dedup();
}

fn empty_project(files: usize) -> Diagnostic {
    let reason = match files {
        0 => "Jadpo found no `.jadpo` files.".to_string(),
        1 => "Its one `.jadpo` file holds no declarations, only comments or nothing at all."
            .to_string(),
        n => {
            format!("Its {n} `.jadpo` files hold no declarations, only comments or nothing at all.")
        }
    };
    Diagnostic::new(
        &rule::TOOLCHAIN_EMPTY_PROJECT,
        "This project declares nothing",
    )
    .reason(reason)
    .next("Add a declaration, such as a `type`, `function` or `action`")
}

/// `type.foreign_value`: `null`, `None` or `True` written for a value Jadpo
/// spells `none`, `true` or `false` (decision register, 2026-10-15), when
/// `written` is one; the caller has checked the name means nothing else.
pub(crate) fn foreign_value(written: &str, span: jadpo_diag::Span) -> Option<Diagnostic> {
    let (meaning, jadpo) = jadpo_syntax::habits::foreign_value(written)?;
    let habit = &jadpo_syntax::habits::FOREIGN_VALUE;
    Some(
        Diagnostic::new(habit.rule, habit.message(written, jadpo))
            .at(span.clone(), habit.label_for(written, jadpo))
            .reason(jadpo_syntax::habits::foreign_reason(
                written,
                meaning,
                jadpo,
                "writes it",
            ))
            .next(habit.next_for(written, jadpo))
            .edit(span, jadpo),
    )
}

pub(crate) fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [only] => only.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}
