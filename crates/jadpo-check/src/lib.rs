//! Checking a Jadpo project (docs/compiler-runtime.md §2.2): the rules of
//! each language area as salsa queries over `jadpo-db`. Areas 1 to 4 are
//! built: layout, naming, visibility and resolution, then values and types
//! and the logic of bodies ([`types`]), and failures (`failures`, with the
//! body rules beside the others in [`types`]). [`graph`] builds the
//! application graph from these queries.
//!
//! [`check`] assembles every diagnostic for a project, sorted by file and
//! position so the same source always gives the same output.

mod case;
pub mod data;
mod failures;
pub mod graph;
mod layout;
pub mod lookups;
mod naming;
mod resolve;
mod std_lib;
mod tree;
pub mod types;
mod usage;
mod visibility;

pub use failures::{
    DECLARABLE_KINDS, FailureInfo, FailureTable, MEMBERS as FAILURE_MEMBERS, failure_problems,
    failure_table,
};
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

/// Every problem in `project`, in a stable order. Syntax errors come alone:
/// a file that does not parse would only add knock-on errors.
pub fn check(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let files = project.files(db);
    let mut out: Vec<Diagnostic> = files
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
    if out.is_empty() {
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
            out.extend(usage_problems(db, project, file).iter().cloned());
            out.extend(
                visibility::public_problems(db, project, file)
                    .iter()
                    .cloned(),
            );
            out.extend(types::type_problems(db, project, file).iter().cloned());
            out.extend(failure_problems(db, project, file).iter().cloned());
            out.extend(types::body_problems(db, project, file).iter().cloned());
            out.extend(types::recursion_problems(db, project, file).iter().cloned());
            out.extend(graph::similar_problems(db, project, file).iter().cloned());
            out.extend(data::data_problems(db, project, file).iter().cloned());
            out.extend(
                data::queries::query_problems(db, project, file)
                    .iter()
                    .cloned(),
            );
        }
        out.extend(types::call_problems(db, project).iter().cloned());
        out.extend(
            data::migrations::migration_problems(db, project)
                .iter()
                .cloned(),
        );
    }
    sort(&mut out);
    out
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

pub(crate) fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [only] => only.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}
